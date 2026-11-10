// SPDX-License-Identifier: AGPL-3.0-or-later
//! Find, Replace and Replace Fonts across the slides (shapes, groups, table cells) and their
//! speaker notes.

use kiln_common::{NodeId, Result};
use kiln_deck::ops::TextTarget;
use kiln_deck::{Shape, ShapeKind};
use kiln_doc::Pos;

use super::DeckDoc;
use crate::model::Change;

/// A match: `(slide index, target, paragraph, offset)`.
type Match = (usize, TextTarget, NodeId, usize);
/// Its position in reading order.
type Key = (usize, usize, usize, usize);

/// The text bodies of a slide in reading order (z-order, groups flattened, cells row by row),
/// then its notes.
pub(super) fn targets_of(slide: &kiln_deck::Slide) -> Vec<TextTarget> {
    fn walk(s: &Shape, out: &mut Vec<TextTarget>) {
        match &s.kind {
            ShapeKind::Group(g) => {
                for c in &g.children {
                    walk(c, out);
                }
            }
            ShapeKind::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for c in 0..row.cells.len() {
                        out.push(TextTarget::Cell {
                            shape: s.id,
                            row: r,
                            col: c,
                        });
                    }
                }
            }
            ShapeKind::Geometry(a) if a.text.is_some() => out.push(TextTarget::Shape(s.id)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in &slide.shapes {
        walk(s, &mut out);
    }
    if slide.notes.is_some() {
        out.push(TextTarget::Notes(slide.id));
    }
    out
}

fn fold(c: char, case: bool) -> char {
    if case {
        c
    } else {
        c.to_lowercase().next().unwrap_or(c)
    }
}

/// Char offsets of every match of `needle` in `hay`.
fn matches(hay: &[char], needle: &[char], case: bool, whole: bool) -> Vec<usize> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.len() > hay.len() {
        return out;
    }
    let word = |c: char| c.is_alphanumeric() || c == '_';
    for i in 0..=hay.len() - needle.len() {
        if (0..needle.len()).all(|k| fold(hay[i + k], case) == fold(needle[k], case)) {
            if whole {
                let before = i > 0 && word(hay[i - 1]);
                let after = i + needle.len() < hay.len() && word(hay[i + needle.len()]);
                if before || after {
                    continue;
                }
            }
            out.push(i);
        }
    }
    out
}

impl DeckDoc {
    /// Every match in the presentation, in order from the start: `(slide index, target,
    /// paragraph, offset)`.
    fn all_matches(&self, text: &str, case: bool, whole: bool) -> Vec<Match> {
        let needle: Vec<char> = text.chars().collect();
        let mut out = Vec::new();
        for (i, slide) in self.pres.slides.iter().enumerate() {
            for t in targets_of(slide) {
                let Some(body) = self.body(t) else { continue };
                for p in body.paragraphs() {
                    let hay: Vec<char> = p.content.raw_text().chars().collect();
                    for o in matches(&hay, &needle, case, whole) {
                        out.push((i, t, p.id, o));
                    }
                }
            }
        }
        out
    }

    /// Where the search continues from: the end of the text selection, else the start of the
    /// current slide.
    pub(super) fn search_origin(&self) -> (usize, Option<(TextTarget, NodeId, usize)>) {
        let slide = self.view.current;
        match &self.view.text {
            Some(t) => {
                let body = self.body(t.target);
                let end = body.map_or(t.head, |b| super::text::ordered(b, t.anchor, t.head).1);
                (slide, Some((t.target, end.para, end.offset)))
            }
            None => (slide, None),
        }
    }

    /// Find Next (or Previous): selects the next match.
    pub(crate) fn find(
        &mut self,
        text: &str,
        case: bool,
        whole: bool,
        backward: bool,
    ) -> Result<Change> {
        let all = self.all_matches(text, case, whole);
        if all.is_empty() {
            self.find_done = true;
            return Ok(Change::Selection);
        }
        let len = text.chars().count();
        // A total order over (slide, target position, paragraph position, offset).
        let key = |doc: &Self, m: &Match| -> Key {
            let slide = &doc.pres.slides[m.0];
            let ts = targets_of(slide);
            let ti = ts.iter().position(|t| *t == m.1).unwrap_or(0);
            let pi = doc
                .body(m.1)
                .and_then(|b| b.paragraphs().position(|p| p.id == m.2))
                .unwrap_or(0);
            (m.0, ti, pi, m.3)
        };
        let (slide, at) = self.search_origin();
        let origin = match at {
            Some((t, para, off)) => {
                let k = key(self, &(slide, t, para, off));
                if backward {
                    (k.0, k.1, k.2, k.3.saturating_sub(len))
                } else {
                    k
                }
            }
            None => (slide, 0, 0, 0),
        };
        let keyed: Vec<(Key, Match)> = all.iter().map(|m| (key(self, m), *m)).collect();
        let pick = if backward {
            keyed
                .iter()
                .rev()
                .find(|(k, _)| *k < origin)
                .or_else(|| keyed.last())
        } else {
            keyed
                .iter()
                .find(|(k, _)| {
                    *k >= origin
                        && !(at.is_some() && *k == origin && self.selection_is_match(text, case))
                })
                .or_else(|| keyed.iter().find(|(k, _)| *k > origin))
                .or_else(|| keyed.first())
        };
        let Some((_, (si, target, para, off))) = pick.copied() else {
            self.find_done = true;
            return Ok(Change::Selection);
        };
        self.find_done = false;
        self.go_to(si);
        self.view.rail = false;
        if matches!(target, TextTarget::Notes(_)) {
            self.view.notes = true;
        }
        let shape = match target {
            TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => s,
            TextTarget::Notes(s) => s,
        };
        self.view.text = Some(super::TextEdit {
            shape,
            target,
            anchor: Pos::new(para, off),
            head: Pos::new(para, off + len),
            goal_x: None,
            composition: None,
        });
        if !matches!(target, TextTarget::Notes(_)) {
            self.view.shapes = vec![shape_top(self, shape)];
        }
        self.epoch += 1;
        Ok(Change::Selection)
    }

    fn selection_is_match(&self, text: &str, case: bool) -> bool {
        let Some(t) = &self.view.text else {
            return false;
        };
        let Some(body) = self.body(t.target) else {
            return false;
        };
        let sel = super::text::range_text(body, t.anchor, t.head);
        if case {
            sel == text
        } else {
            sel.to_lowercase() == text.to_lowercase()
        }
    }

    /// Replace (the current match, then Find Next) or Replace All.
    pub(crate) fn replace(
        &mut self,
        text: &str,
        with: &str,
        case: bool,
        whole: bool,
        all: bool,
    ) -> Result<Change> {
        if text.is_empty() {
            return Ok(Change::None);
        }
        if !all {
            if self.selection_is_match(text, case) {
                self.type_text(with)?;
            }
            self.find(text, case, whole, false)?;
            return Ok(Change::Content);
        }
        let mut found = self.all_matches(text, case, whole);
        if found.is_empty() {
            self.find_done = true;
            return Ok(Change::Selection);
        }
        let len = text.chars().count();
        // Later matches first, so earlier offsets stay valid.
        found.reverse();
        self.end_text();
        self.begin_group();
        let mut result = Ok(());
        for (_, target, para, off) in found {
            let r = self.edit_body("deck.edit.replace", target, |doc| {
                let mut tx = kiln_doc::edit::delete_range(
                    doc,
                    Pos::new(para, off),
                    Pos::new(para, off + len),
                )?;
                if !with.is_empty() {
                    // Insert keeps the formatting of the replaced text's first char.
                    let props = doc
                        .paragraph(para)
                        .and_then(|p| p.content.props_at(off).cloned())
                        .unwrap_or_default();
                    tx.ops.push(kiln_doc::Op::InsertInline {
                        para,
                        offset: off,
                        fragment: kiln_doc::Fragment::text(with, props),
                    });
                }
                Ok(tx)
            });
            if let Err(e) = r {
                result = Err(e);
                break;
            }
        }
        self.end_group();
        self.view.text = None;
        result?;
        Ok(Change::Content)
    }

    /// Replace Fonts: every run of `from` becomes `to`.
    pub(crate) fn replace_font(&mut self, from: &str, to: &str) -> Result<Change> {
        let targets: Vec<TextTarget> = self.pres.slides.iter().flat_map(targets_of).collect();
        self.end_text();
        self.begin_group();
        let mut result = Ok(());
        let mut changed = false;
        for t in targets {
            let Some(body) = self.body(t) else { continue };
            let Some((a, b)) = super::text::body_ends(body) else {
                continue;
            };
            let uses = body.paragraphs().any(|p| {
                p.content.runs().iter().any(|r| {
                    r.props.fonts.as_ref().is_some_and(|f| {
                        f.ascii
                            .as_deref()
                            .is_some_and(|n| n.eq_ignore_ascii_case(from))
                    })
                })
            });
            if !uses {
                continue;
            }
            changed = true;
            let to = to.to_owned();
            let from = from.to_owned();
            let r = self.edit_body("deck.edit.replace_font", t, |doc| {
                kiln_doc::edit::format_runs(doc, a, b, |p| {
                    if let Some(f) = &mut p.fonts {
                        for slot in [&mut f.ascii, &mut f.h_ansi, &mut f.east_asia, &mut f.cs] {
                            if slot
                                .as_deref()
                                .is_some_and(|n| n.eq_ignore_ascii_case(&from))
                            {
                                *slot = Some(to.clone());
                            }
                        }
                    }
                })
            });
            if let Err(e) = r {
                result = Err(e);
            }
        }
        self.end_group();
        result?;
        Ok(if changed {
            Change::Content
        } else {
            Change::None
        })
    }
}

/// The top-level shape of the current slide that holds `id`.
pub(super) fn shape_top(doc: &DeckDoc, id: NodeId) -> NodeId {
    doc.pres
        .locate(id)
        .and_then(|p| {
            let shapes = doc.pres.owner_shapes(p.owner)?;
            shapes.get(*p.path.first()?).map(|s| s.id)
        })
        .unwrap_or(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_respects_case_and_words() {
        let hay: Vec<char> = "Kiln kilns KILN".chars().collect();
        let n: Vec<char> = "kiln".chars().collect();
        assert_eq!(matches(&hay, &n, false, false), vec![0, 5, 11]);
        assert_eq!(matches(&hay, &n, true, false), vec![5]);
        assert_eq!(matches(&hay, &n, false, true), vec![0, 11]);
    }
}
