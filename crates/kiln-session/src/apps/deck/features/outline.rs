// SPDX-License-Identifier: AGPL-3.0-or-later
//! Outline View: the text of every slide as one outline — each slide's title, then the
//! paragraphs of its body placeholders — edited line by line as PowerPoint's outline.
//!
//! Typing, Enter in a body line, Tab and Shift+Tab within the body and Backspace between lines
//! of one placeholder are ordinary text edits of the placeholder (typing merges into one undo
//! step, as on the stage). Edits that cross slides treat the outline as the linear text it is:
//! Enter in a title starts a new slide that takes the rest of the title and the lines after it;
//! Shift+Tab on a first-level line makes it the title of a new slide; Tab on a title (and
//! Backspace at its start) makes the slide part of the one before, its lines a level down, and
//! the slide goes. Each of those is one undo step. Titles and lines keep their runs and fields
//! through every split and join.

use std::collections::BTreeMap;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::edit;
use kiln_deck::ops::{Op, TextTarget, Transaction};
use kiln_deck::text::{DeckParaProps, LEVELS, TextBody};
use kiln_deck::{LayoutKind, Presentation, Shape, ShapeKind, Slide};
use kiln_doc::{Fragment, Inlines, OBJECT_CHAR, Paragraph, Pos, RunProps};
use kiln_protocol::commands::deck_content::{DeckShapeText, DeckSlideText};
use kiln_protocol::commands::deck_features::{DeckOutlineEdit, DeckOutlineEntry, DeckOutlineLine};

use super::super::DeckDoc;
use crate::model::Change;

/// The undo label of outline edits.
const LABEL: &str = "deck.outline.edit";

fn invalid(msg: impl Into<String>) -> KilnError {
    KilnError::InvalidInput(msg.into())
}

/// A paragraph's text as the outline shows it (fields as their text, line breaks as spaces),
/// with the model offset of each shown character (and one past the end).
fn shown(p: &Paragraph) -> (String, Vec<usize>) {
    let objects: Vec<(usize, String)> = p
        .content
        .objects_with_offsets()
        .map(|(at, o)| (at, o.plain_text()))
        .collect();
    let mut text = String::new();
    let mut map = Vec::new();
    let mut k = 0;
    for (range, _, run) in p.content.run_texts() {
        for (i, c) in run.chars().enumerate() {
            let at = range.start + i;
            if c == OBJECT_CHAR {
                let o = objects
                    .get(k)
                    .filter(|(o, _)| *o == at)
                    .map(|(_, s)| s.as_str())
                    .unwrap_or_default();
                k += 1;
                for oc in o.chars() {
                    text.push(if oc.is_control() { ' ' } else { oc });
                    map.push(at);
                }
            } else {
                text.push(if c.is_control() { ' ' } else { c });
                map.push(at);
            }
        }
    }
    map.push(p.content.len());
    (text, map)
}

/// A slide in the outline: its title shape and the body placeholders it shows.
#[derive(Debug, Clone)]
struct Entry {
    slide: NodeId,
    title: Option<NodeId>,
    /// Body placeholders with text, in z-order.
    bodies: Vec<NodeId>,
    /// Every text placeholder that is not the title (a slide receiving lines uses the first).
    targets: Vec<NodeId>,
}

fn is_body(s: &Shape) -> bool {
    s.placeholder
        .is_some_and(|p| !p.kind.is_title() && !p.kind.is_footer_area() && p.kind.holds_text())
        && s.text().is_some()
}

fn entries(pres: &Presentation) -> Vec<Entry> {
    pres.slides
        .iter()
        .map(|s| Entry {
            slide: s.id,
            title: s
                .shapes
                .iter()
                .find(|sh| sh.placeholder.is_some_and(|p| p.kind.is_title()) && sh.text().is_some())
                .map(|sh| sh.id),
            bodies: s
                .shapes
                .iter()
                .filter(|sh| is_body(sh) && sh.text().is_some_and(|t| !t.is_empty()))
                .map(|sh| sh.id)
                .collect(),
            targets: s
                .shapes
                .iter()
                .filter(|sh| is_body(sh))
                .map(|sh| sh.id)
                .collect(),
        })
        .collect()
}

fn body(pres: &Presentation, shape: NodeId) -> Option<&TextBody> {
    pres.shape(shape).and_then(Shape::text)
}

fn title_text(pres: &Presentation, shape: Option<NodeId>) -> String {
    shape
        .and_then(|s| body(pres, s))
        .map(|b| {
            b.paragraphs()
                .map(|p| shown(p).0)
                .collect::<Vec<_>>()
                .join(" ")
        })
        .unwrap_or_default()
}

/// A slide's title as one line of the outline: its paragraph (a title of several paragraphs
/// joined by spaces; an empty paragraph when there is no title).
fn title_para(pres: &Presentation, shape: Option<NodeId>) -> Result<Paragraph> {
    let mut paras = shape
        .and_then(|s| body(pres, s))
        .into_iter()
        .flat_map(TextBody::paragraphs);
    let Some(first) = paras.next() else {
        return Ok(Paragraph::new(NodeId(1)));
    };
    let mut out = first.clone();
    for p in paras {
        out = joined(&joined(&out, &plain_para(" ", 0)?)?, p)?;
    }
    Ok(out)
}

/// The slide holds more than the outline shows: shapes other than its text placeholders, or
/// speaker notes.
fn has_more(s: &Slide) -> bool {
    s.notes.as_ref().is_some_and(|n| !n.text.is_empty())
        || s.shapes
            .iter()
            .any(|sh| sh.placeholder.is_none() || !matches!(sh.kind, ShapeKind::Geometry(_)))
}

/// A line of the linear outline.
#[derive(Debug, Clone)]
enum Item {
    /// A slide's title (`slide: None` for a slide the edit makes).
    Title {
        slide: Option<NodeId>,
        para: Box<Paragraph>,
    },
    /// A body paragraph, with the placeholder it came from.
    Line {
        from: Option<(NodeId, NodeId)>,
        para: Box<Paragraph>,
        extras: Option<DeckParaProps>,
    },
}

fn level_of(p: &Paragraph) -> usize {
    TextBody::level_of(p)
}

fn set_level(p: &mut Paragraph, level: usize) {
    p.props.outline_level = u8::try_from(level.min(LEVELS - 1)).ok().filter(|l| *l > 0);
}

/// A paragraph of plain text at a level.
fn plain_para(text: &str, level: usize) -> Result<Paragraph> {
    let mut p = Paragraph::new(NodeId(1));
    p.content = Inlines::from_fragment(Fragment::text(text, RunProps::default()))
        .map_err(|e| invalid(e.to_string()))?;
    set_level(&mut p, level);
    Ok(p)
}

/// `a` with `b`'s content appended (runs and fields as they are).
fn joined(a: &Paragraph, b: &Paragraph) -> Result<Paragraph> {
    let tail = b
        .content
        .slice(0, b.content.len())
        .map_err(|e| invalid(e.to_string()))?;
    let mut out = a.clone();
    let end = out.content.len();
    out.content
        .insert(end, tail)
        .map_err(|e| invalid(e.to_string()))?;
    Ok(out)
}

/// A paragraph cut in two at a model offset, both halves with its formatting.
fn split_at(p: &Paragraph, at: usize) -> Result<(Paragraph, Paragraph)> {
    let mut head = p.clone();
    let end = head.content.len();
    let rest = head
        .content
        .remove(at.min(end), end)
        .map_err(|e| invalid(e.to_string()))?;
    let mut tail = p.clone();
    tail.content = Inlines::from_fragment(rest).map_err(|e| invalid(e.to_string()))?;
    Ok((head, tail))
}

/// A paragraph with `p`'s content and nothing of its paragraph formatting, at a level.
fn content_of(p: &Paragraph, level: usize) -> Paragraph {
    let mut out = Paragraph::new(NodeId(1));
    out.content = p.content.clone();
    set_level(&mut out, level);
    out
}

/// A title's text body: one paragraph with `para`'s content and the paragraph formatting of the
/// title's first.
fn title_body(old: &TextBody, para: &Paragraph) -> Result<TextBody> {
    let mut p = old
        .paragraphs()
        .next()
        .cloned()
        .unwrap_or_else(|| Paragraph::new(NodeId(1)));
    let extras = old.deck_props(p.id).cloned();
    p.content = para.content.clone();
    rebuilt(old, vec![(p, extras)])
}

/// A text body with these paragraphs (fresh ids) and `old`'s body properties and list style.
fn rebuilt(old: &TextBody, paras: Vec<(Paragraph, Option<DeckParaProps>)>) -> Result<TextBody> {
    let mut paras = paras;
    if paras.is_empty() {
        let mut p = old
            .paragraphs()
            .next()
            .cloned()
            .unwrap_or_else(|| Paragraph::new(NodeId(1)));
        p.content = Inlines::default();
        paras.push((p, None));
    }
    let mut list = Vec::with_capacity(paras.len());
    let mut extras = BTreeMap::new();
    for (k, (mut p, x)) in paras.into_iter().enumerate() {
        p.id = NodeId(u64::try_from(k).unwrap_or(0) + 1);
        if let Some(x) = x {
            extras.insert(p.id, x);
        }
        list.push(p);
    }
    let mut out = TextBody::from_paragraphs(list, extras).map_err(|e| invalid(e.to_string()))?;
    out.body = old.body.clone();
    out.list_style = old.list_style.clone();
    Ok(out)
}

impl DeckDoc {
    /// The outline Outline View shows.
    pub(crate) fn outline_view(&self) -> Vec<DeckOutlineEntry> {
        entries(&self.pres)
            .into_iter()
            .map(|e| DeckOutlineEntry {
                slide: e.slide,
                title: title_text(&self.pres, e.title),
                lines: e
                    .bodies
                    .iter()
                    .filter_map(|b| body(&self.pres, *b))
                    .flat_map(TextBody::paragraphs)
                    .map(|p| DeckOutlineLine {
                        level: u8::try_from(level_of(p)).unwrap_or(0),
                        text: shown(p).0,
                    })
                    .collect(),
                has_title: e.title.is_some(),
                hidden: self.pres.slide(e.slide).is_some_and(|s| s.hidden),
                more: self.pres.slide(e.slide).is_some_and(has_more),
            })
            .collect()
    }

    /// Every slide's text: each shape with text that is not hidden, back to front, and the
    /// notes.
    pub(crate) fn slide_text(&self) -> Vec<DeckSlideText> {
        let lines = |t: &TextBody| -> Vec<String> {
            t.paragraphs()
                .map(|p| shown(p).0)
                .filter(|l| !l.trim().is_empty())
                .collect()
        };
        self.pres
            .slides
            .iter()
            .map(|s| {
                let mut texts = Vec::new();
                for top in &s.shapes {
                    top.walk(&mut |shape| {
                        let found = match &shape.kind {
                            _ if shape.hidden => Vec::new(),
                            ShapeKind::Geometry(a) => {
                                a.text.as_ref().map(lines).unwrap_or_default()
                            }
                            ShapeKind::Table(t) => t
                                .rows
                                .iter()
                                .flat_map(|r| &r.cells)
                                .flat_map(|c| lines(&c.text))
                                .collect(),
                            _ => Vec::new(),
                        };
                        if !found.is_empty() {
                            texts.push(DeckShapeText {
                                name: shape.name.clone(),
                                kind: shape.kind.label().into(),
                                lines: found,
                            });
                        }
                    });
                }
                DeckSlideText {
                    slide: s.id,
                    shapes: u32::try_from(s.shapes.len()).unwrap_or(u32::MAX),
                    texts,
                    notes: s.notes.as_ref().map(|n| lines(&n.text)).unwrap_or_default(),
                    hidden: s.hidden,
                }
            })
            .collect()
    }

    /// The placeholder and paragraph of body line `line` (1-based) of a slide.
    fn line_at(&self, e: &Entry, line: usize) -> Option<(NodeId, NodeId)> {
        e.bodies
            .iter()
            .flat_map(|b| {
                body(&self.pres, *b)
                    .into_iter()
                    .flat_map(TextBody::paragraphs)
                    .map(move |p| (*b, p.id))
            })
            .nth(line.checked_sub(1)?)
    }

    /// Applies an edit of Outline View.
    pub(crate) fn outline_command(&mut self, edit: &DeckOutlineEdit) -> Result<Change> {
        let (slide, line) = match edit {
            DeckOutlineEdit::Text { slide, line, .. }
            | DeckOutlineEdit::Split { slide, line, .. }
            | DeckOutlineEdit::Indent { slide, line, .. }
            | DeckOutlineEdit::Join { slide, line } => (*slide as usize, *line as usize),
        };
        let all = entries(&self.pres);
        let e = all
            .get(slide)
            .cloned()
            .ok_or_else(|| invalid(format!("there is no slide {}", slide + 1)))?;
        let at = if line == 0 {
            None
        } else {
            Some(
                self.line_at(&e, line)
                    .ok_or_else(|| invalid(format!("slide {} has no line {line}", slide + 1)))?,
            )
        };
        self.end_text();
        let changed = match (edit, at) {
            (DeckOutlineEdit::Text { text, .. }, None) => {
                let Some(t) = e.title else {
                    return Err(invalid("this slide has no title placeholder"));
                };
                self.set_line_text(t, None, text)?
            }
            (DeckOutlineEdit::Text { text, .. }, Some((shape, para))) => {
                self.set_line_text(shape, Some(para), text)?
            }
            (DeckOutlineEdit::Split { offset, .. }, Some((shape, para))) => {
                let p = body(&self.pres, shape)
                    .and_then(|b| b.paragraph(para))
                    .ok_or_else(|| invalid("the line is gone"))?;
                let (_, map) = shown(p);
                let at = map
                    .get(*offset as usize)
                    .copied()
                    .unwrap_or(p.content.len());
                self.apply_with(LABEL, |pr| {
                    edit::text::edit_text(pr, TextTarget::Shape(shape), |doc| {
                        kiln_doc::edit::split_paragraph(doc, Pos::new(para, at))
                    })
                })?
            }
            (DeckOutlineEdit::Indent { by, .. }, Some((shape, para))) => {
                let level = body(&self.pres, shape)
                    .and_then(|b| b.paragraph(para))
                    .map_or(0, level_of);
                if *by < 0 && level == 0 {
                    self.restructure(
                        &all,
                        |items, k| {
                            let Item::Line { para, .. } = &items[k] else {
                                return Ok(());
                            };
                            let para = Box::new(content_of(para, 0));
                            items[k] = Item::Title { slide: None, para };
                            Ok(())
                        },
                        slide,
                        line,
                    )?
                } else {
                    self.apply_with(LABEL, |pr| {
                        edit::text::set_paragraph_level(pr, TextTarget::Shape(shape), &[para], *by)
                    })?
                }
            }
            (DeckOutlineEdit::Join { .. }, Some((shape, para))) => {
                // Within one placeholder: an ordinary Backspace.
                let b = body(&self.pres, shape).ok_or_else(|| invalid("the line is gone"))?;
                let ids: Vec<NodeId> = b.paragraphs().map(|p| p.id).collect();
                let k = ids.iter().position(|i| *i == para).unwrap_or(0);
                if k > 0 {
                    let prev = ids[k - 1];
                    let end = b.paragraph(prev).map_or(0, |p| p.content.len());
                    self.apply_with(LABEL, |pr| {
                        edit::text::edit_text(pr, TextTarget::Shape(shape), |doc| {
                            kiln_doc::edit::delete_range(
                                doc,
                                Pos::new(prev, end),
                                Pos::new(para, 0),
                            )
                        })
                    })?
                } else {
                    self.restructure(&all, join_previous, slide, line)?
                }
            }
            (DeckOutlineEdit::Split { offset, .. }, None) => {
                let offset = *offset as usize;
                self.restructure(
                    &all,
                    |items, k| {
                        let Item::Title { para, .. } = &mut items[k] else {
                            return Ok(());
                        };
                        let (text, map) = shown(para);
                        let cut = offset.min(text.chars().count());
                        if cut == 0 && !text.is_empty() {
                            let empty = Box::new(content_of(&Paragraph::new(NodeId(1)), 0));
                            items.insert(
                                k,
                                Item::Title {
                                    slide: None,
                                    para: empty,
                                },
                            );
                        } else {
                            let (head, tail) = split_at(para, map[cut])?;
                            **para = head;
                            items.insert(
                                k + 1,
                                Item::Title {
                                    slide: None,
                                    para: Box::new(tail),
                                },
                            );
                        }
                        Ok(())
                    },
                    slide,
                    0,
                )?
            }
            (DeckOutlineEdit::Indent { by, .. }, None) => {
                if *by <= 0 || slide == 0 {
                    false
                } else {
                    self.restructure(
                        &all,
                        |items, k| {
                            let Item::Title { para, .. } = &items[k] else {
                                return Ok(());
                            };
                            let para = Box::new(content_of(para, 0));
                            items[k] = Item::Line {
                                from: None,
                                para,
                                extras: None,
                            };
                            for item in items.iter_mut().skip(k + 1) {
                                match item {
                                    Item::Title { .. } => break,
                                    Item::Line { para, .. } => {
                                        let l = level_of(para) + 1;
                                        set_level(para, l);
                                    }
                                }
                            }
                            Ok(())
                        },
                        slide,
                        0,
                    )?
                }
            }
            (DeckOutlineEdit::Join { .. }, None) => {
                if slide == 0 {
                    false
                } else {
                    self.restructure(&all, join_previous, slide, 0)?
                }
            }
        };
        Ok(if changed {
            Change::Content
        } else {
            Change::None
        })
    }

    /// Replaces the text of a line, keeping the formatting of what did not change: the common
    /// start and end stay, the middle is replaced (typing merges into one undo step).
    fn set_line_text(&mut self, shape: NodeId, para: Option<NodeId>, text: &str) -> Result<bool> {
        let b = body(&self.pres, shape).ok_or_else(|| invalid("the line is gone"))?;
        let p = match para {
            Some(id) => b.paragraph(id),
            None if b.paragraph_count() == 1 => b.paragraphs().next(),
            None => None,
        };
        let Some(p) = p else {
            // A title of several paragraphs becomes one.
            return self.apply_with(LABEL, |pr| {
                edit::text::set_text(pr, TextTarget::Shape(shape), text)
            });
        };
        let id = p.id;
        let (old, map) = shown(p);
        let old: Vec<char> = old.chars().collect();
        let new: Vec<char> = text
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
        let pre = old.iter().zip(&new).take_while(|(a, b)| a == b).count();
        let max_suf = old.len().min(new.len()) - pre;
        let suf = old
            .iter()
            .rev()
            .zip(new.iter().rev())
            .take(max_suf)
            .take_while(|(a, b)| a == b)
            .count();
        if pre + suf == old.len() && pre + suf == new.len() {
            return Ok(false);
        }
        let from = map[pre];
        let to = map[old.len() - suf];
        let insert: String = new[pre..new.len() - suf].iter().collect();
        // Typing (only an insertion or only a deletion) merges with the typing before it; a
        // replacement is one step of its own.
        let both = to > from && !insert.is_empty();
        if both {
            self.begin_group();
        }
        let result = (|| {
            if to > from {
                self.apply_with(LABEL, |pr| {
                    edit::text::edit_text(pr, TextTarget::Shape(shape), |doc| {
                        kiln_doc::edit::delete_range(doc, Pos::new(id, from), Pos::new(id, to))
                    })
                })?;
            }
            if !insert.is_empty() {
                self.apply_with(LABEL, |pr| {
                    edit::text::insert_text(
                        pr,
                        TextTarget::Shape(shape),
                        Pos::new(id, from),
                        &insert,
                    )
                })?;
            }
            Ok::<(), KilnError>(())
        })();
        if both {
            self.end_group();
        }
        result?;
        Ok(true)
    }

    /// An edit that crosses slides: the outline as linear items, changed by `change` at the
    /// item of (`slide`, `line`), then written back — titles, bodies, new slides and slides that
    /// went — as one undo step.
    fn restructure(
        &mut self,
        all: &[Entry],
        change: impl FnOnce(&mut Vec<Item>, usize) -> Result<()>,
        slide: usize,
        line: usize,
    ) -> Result<bool> {
        let mut items = Vec::new();
        let mut at = None;
        for (i, e) in all.iter().enumerate() {
            if i == slide && line == 0 {
                at = Some(items.len());
            }
            items.push(Item::Title {
                slide: Some(e.slide),
                para: Box::new(title_para(&self.pres, e.title)?),
            });
            let mut n = 0;
            for b in &e.bodies {
                let Some(tb) = body(&self.pres, *b) else {
                    continue;
                };
                for p in tb.paragraphs() {
                    n += 1;
                    if i == slide && n == line {
                        at = Some(items.len());
                    }
                    items.push(Item::Line {
                        from: Some((e.slide, *b)),
                        para: Box::new(p.clone()),
                        extras: tb.deck_props(p.id).cloned(),
                    });
                }
            }
        }
        let Some(k) = at else {
            return Ok(false);
        };
        change(&mut items, k)?;
        if !matches!(items.first(), Some(Item::Title { .. })) {
            return Err(invalid("the first line of the outline is a slide title"));
        }
        // Group the items into slides.
        let mut groups: Vec<(Option<NodeId>, Box<Paragraph>, Vec<Item>)> = Vec::new();
        for item in items {
            match item {
                Item::Title { slide, para } => groups.push((slide, para, Vec::new())),
                line @ Item::Line { .. } => {
                    if let Some(g) = groups.last_mut() {
                        g.2.push(line);
                    }
                }
            }
        }
        let entry = |id: NodeId| all.iter().find(|e| e.slide == id);
        // Check before changing anything: every slide that gets lines has somewhere to put them.
        for (id, _, lines) in &groups {
            let Some(id) = id else { continue };
            let Some(e) = entry(*id) else { continue };
            let foreign = lines.iter().any(
                |l| matches!(l, Item::Line { from, .. } if from.is_none_or(|(s, _)| s != *id)),
            );
            if foreign && e.targets.is_empty() {
                return Err(invalid(format!(
                    "slide {} has no placeholder for text",
                    self.pres.slide_index(*id).map_or(0, |i| i + 1)
                )));
            }
        }
        let kept: Vec<NodeId> = groups.iter().filter_map(|g| g.0).collect();
        let gone: Vec<NodeId> = all
            .iter()
            .map(|e| e.slide)
            .filter(|s| !kept.contains(s))
            .collect();
        self.begin_group();
        let result = (|| {
            // Titles and bodies of the slides that stay.
            for (id, title, lines) in &groups {
                let Some(id) = *id else { continue };
                let Some(e) = entry(id) else { continue };
                if title_para(&self.pres, e.title)?.content != title.content {
                    if let Some(t) = e.title {
                        let old =
                            body(&self.pres, t).ok_or_else(|| invalid("the title is gone"))?;
                        let new = title_body(old, title)?;
                        self.apply(
                            LABEL,
                            Transaction::new(vec![Op::SetTextBody {
                                shape: t,
                                text: Some(Box::new(new)),
                            }]),
                        )?;
                    } else if !title.content.is_empty() {
                        return Err(invalid("this slide has no title placeholder"));
                    }
                }
                let mut per: BTreeMap<NodeId, Vec<(Paragraph, Option<DeckParaProps>)>> =
                    BTreeMap::new();
                let default = e.bodies.first().or(e.targets.first()).copied();
                let mut last = default;
                for l in lines {
                    let Item::Line { from, para, extras } = l else {
                        continue;
                    };
                    let target = match from {
                        Some((s, b)) if *s == id => Some(*b),
                        _ => last,
                    };
                    let Some(t) = target else { continue };
                    last = Some(t);
                    per.entry(t)
                        .or_default()
                        .push((Paragraph::clone(para), extras.clone()));
                }
                for b in e.bodies.iter().chain(&e.targets) {
                    per.entry(*b).or_default();
                }
                for (shape, paras) in per {
                    let Some(old) = body(&self.pres, shape) else {
                        continue;
                    };
                    let same = old.paragraph_count() == paras.len()
                        && old.paragraphs().zip(&paras).all(|(a, (b, x))| {
                            a.id == b.id && a == b && old.deck_props(a.id) == x.as_ref()
                        });
                    let shown_before = e.bodies.contains(&shape);
                    if same || (!shown_before && paras.is_empty()) {
                        continue;
                    }
                    let new = rebuilt(old, paras)?;
                    self.apply(
                        LABEL,
                        Transaction::new(vec![Op::SetTextBody {
                            shape,
                            text: Some(Box::new(new)),
                        }]),
                    )?;
                }
            }
            // Slides that went.
            if !gone.is_empty() {
                self.apply_with(LABEL, |pr| edit::delete_slides(pr, &gone))?;
            }
            // New slides, in place.
            for (i, (id, title, lines)) in groups.iter().enumerate() {
                if id.is_some() {
                    continue;
                }
                let before = groups[..i].iter().rev().find_map(|g| g.0);
                let index = before
                    .and_then(|b| self.pres.slide_index(b))
                    .map_or(0, |x| x + 1);
                let layout = self.layout_after(before)?;
                let paras: Vec<(Paragraph, Option<DeckParaProps>)> = lines
                    .iter()
                    .filter_map(|l| match l {
                        Item::Line { para, extras, .. } => {
                            Some((Paragraph::clone(para), extras.clone()))
                        }
                        Item::Title { .. } => None,
                    })
                    .collect();
                let slide = {
                    let mut s = edit::slides::new_slide(&mut self.pres, layout);
                    if let Some(t) = s
                        .shapes
                        .iter_mut()
                        .find(|sh| sh.placeholder.is_some_and(|p| p.kind.is_title()))
                        .and_then(Shape::text_mut)
                    {
                        *t = title_body(t, title)?;
                    }
                    if !paras.is_empty() {
                        let Some(b) = s.shapes.iter_mut().find(|sh| is_body(sh)) else {
                            return Err(invalid(
                                "the new slide's layout has no placeholder for text",
                            ));
                        };
                        if let ShapeKind::Geometry(g) = &mut b.kind
                            && let Some(old) = g.text.as_ref()
                        {
                            g.text = Some(rebuilt(old, paras)?);
                        }
                    }
                    s
                };
                let mut ops = vec![Op::InsertSlide {
                    index,
                    slide: Box::new(slide),
                }];
                if !self.pres.sections.is_empty() {
                    let mut sections = self.pres.sections.clone();
                    let target = if index == 0 {
                        0
                    } else {
                        self.pres.section_of_index(index - 1).unwrap_or(0)
                    };
                    if let Some(s) = sections.get_mut(target) {
                        s.len += 1;
                    }
                    ops.push(Op::SetSections { sections });
                }
                self.apply(LABEL, Transaction::new(ops))?;
            }
            Ok::<(), KilnError>(())
        })();
        self.end_group();
        result?;
        self.view.text = None;
        self.view.shapes.clear();
        let current = slide.min(self.pres.slides.len().saturating_sub(1));
        self.view.current = current;
        self.view.slides = vec![current];
        self.view.anchor = current;
        Ok(true)
    }

    /// The layout of a slide made after `before`: Title and Content after a title or section
    /// header slide, else the same layout (PowerPoint's New Slide).
    fn layout_after(&self, before: Option<NodeId>) -> Result<NodeId> {
        let found = before
            .and_then(|b| self.pres.slide(b))
            .and_then(|s| self.pres.layout(s.layout))
            .map(|(m, l)| {
                if matches!(l.kind, LayoutKind::Title | LayoutKind::SectionHeader) {
                    m.layout_of_kind(&LayoutKind::TitleContent)
                        .map_or(l.id, |x| x.id)
                } else {
                    l.id
                }
            })
            .or_else(|| {
                self.pres.masters.first().and_then(|m| {
                    m.layout_of_kind(&LayoutKind::TitleContent)
                        .or_else(|| m.layouts.first())
                        .map(|l| l.id)
                })
            });
        found.ok_or_else(|| invalid("no layout to use"))
    }
}

/// Backspace at the start of a line whose line before is elsewhere: a title joins the line
/// before it (its slide's lines go with it), a first body line joins the title.
fn join_previous(items: &mut Vec<Item>, k: usize) -> Result<()> {
    if k == 0 {
        return Ok(());
    }
    let cur = items.remove(k);
    let (Item::Title { para: tail, .. } | Item::Line { para: tail, .. }) = &cur;
    let (Item::Title { para, .. } | Item::Line { para, .. }) = &mut items[k - 1];
    **para = joined(para, tail)?;
    Ok(())
}
