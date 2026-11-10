// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cut, Copy and Paste of text, objects and slides. The system clipboard gets plain text (and
//! Kiln's own format, which carries the objects or slides so another Deck window can paste
//! them); a paste of Kiln's format brings back the objects or slides, anything else is text —
//! into the text being edited, or as a new text box.

use std::collections::BTreeMap;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::{Op, Transaction};
use kiln_deck::{Part, Shape, ShapeKind, Slide};
use kiln_doc::Fragment;
use kiln_protocol::commands::{ClipboardData, EditCommand};
use serde::{Deserialize, Serialize};

use super::DeckDoc;
use crate::model::Change;

/// Kiln's clipboard format for Deck (`ClipboardData::kiln`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) enum Clip {
    /// Rich text runs, one fragment per paragraph.
    Text {
        /// The paragraphs.
        paragraphs: Vec<Fragment>,
    },
    /// Objects of a slide, with the media they use.
    Shapes {
        /// The shapes (placeholders made ordinary).
        shapes: Vec<Shape>,
        /// Their pictures.
        parts: BTreeMap<String, Part>,
    },
    /// Whole slides, with the name of their layout (for another presentation) and media.
    Slides {
        /// The slides.
        slides: Vec<Slide>,
        /// Their layouts' names, in order.
        layouts: Vec<String>,
        /// Their pictures.
        parts: BTreeMap<String, Part>,
    },
}

/// The prefix of Deck's clipboard payload.
const TAG: &str = "kiln-deck/1:";

/// Kiln's clipboard payload (`ClipboardData::kiln`) as a clip, when it is Deck's.
pub(crate) fn decode(kiln: &str) -> Option<Clip> {
    kiln.strip_prefix(TAG)
        .and_then(crate::b64::decode)
        .and_then(|b| postcard::from_bytes::<Clip>(&b).ok())
}

/// Media copied along at most (larger pictures stay behind; the paste shows placeholders).
const MAX_PARTS: usize = 16 * 1024 * 1024;

fn parts_of(pres: &kiln_deck::Presentation, shapes: &[Shape]) -> BTreeMap<String, Part> {
    let mut names = Vec::new();
    for s in shapes {
        s.walk(&mut |x| match &x.kind {
            ShapeKind::Picture(p) => names.push(p.media.clone()),
            ShapeKind::Chart(c) => names.push(c.part.clone()),
            _ => {}
        });
    }
    let mut out = BTreeMap::new();
    let mut total = 0usize;
    for n in names {
        if let Some(p) = pres.parts.get(&n) {
            total += p.bytes.len();
            if total > MAX_PARTS {
                break;
            }
            out.insert(n, p.clone());
        }
    }
    out
}

impl DeckDoc {
    fn put_clipboard(&mut self, text: String, clip: Clip) {
        // Postcard in base64: the web core then needs no JSON decoder for the slide model.
        let kiln = postcard::to_allocvec(&clip)
            .ok()
            .map(|b| format!("{TAG}{}", crate::b64::encode(&b)));
        self.copied = Some(ClipboardData {
            text,
            html: None,
            kiln,
        });
        self.clip = Some(clip);
    }

    /// Copy: the selected text, objects or slides.
    fn copy(&mut self) -> Result<bool> {
        if let Some(t) = self.view.text.clone() {
            if t.collapsed() {
                return Ok(false);
            }
            let body = self
                .body(t.target)
                .ok_or_else(|| KilnError::InvalidInput("no text".into()))?;
            let text = super::text::range_text(body, t.anchor, t.head);
            let mut paragraphs = Vec::new();
            for (para, s, e) in self.segments(t.target, t.anchor, t.head) {
                if let Some(p) = body.paragraph(para)
                    && let Ok(f) = p.content.slice(s, e)
                {
                    paragraphs.push(f);
                }
            }
            self.put_clipboard(text, Clip::Text { paragraphs });
            return Ok(true);
        }
        if !self.view.shapes.is_empty() && !self.view.rail {
            let ids = self.view.shapes.clone();
            let mut shapes = Vec::new();
            for id in &ids {
                if let Some(s) = self.pres.shape(*id).cloned() {
                    shapes.push(self.explicit(&s));
                }
            }
            let text = shapes
                .iter()
                .filter_map(|s| s.text().map(kiln_deck::TextBody::text))
                .collect::<Vec<_>>()
                .join("\n");
            let parts = parts_of(&self.pres, &shapes);
            self.put_clipboard(text, Clip::Shapes { shapes, parts });
            return Ok(true);
        }
        let ids = self.selected_slide_ids();
        let slides: Vec<Slide> = ids
            .iter()
            .filter_map(|id| self.pres.slide(*id).cloned())
            .collect();
        if slides.is_empty() {
            return Ok(false);
        }
        let layouts = slides
            .iter()
            .map(|s| {
                self.pres
                    .layout(s.layout)
                    .map(|(_, l)| l.name.clone())
                    .unwrap_or_default()
            })
            .collect();
        let all: Vec<Shape> = slides
            .iter()
            .flat_map(|s| s.shapes.iter().cloned())
            .collect();
        let parts = parts_of(&self.pres, &all);
        let text = ids
            .iter()
            .map(|id| kiln_deck_layout::a11y::slide_title(&self.pres, *id))
            .collect::<Vec<_>>()
            .join("\n");
        self.put_clipboard(
            text,
            Clip::Slides {
                slides,
                layouts,
                parts,
            },
        );
        Ok(true)
    }

    /// A shape with its inherited position and look made explicit (for a copy).
    fn explicit(&self, s: &Shape) -> Shape {
        let mut c = s.clone();
        if c.xfrm.is_none() {
            c.xfrm = self.pres.resolved_xfrm(s.id);
        }
        if s.placeholder.is_some() {
            if let Some(p) = self.pres.resolved_props(s.id)
                && let Some(own) = c.props_mut()
            {
                *own = p;
            }
            if let Some(b) = self
                .pres
                .resolved_body(s.id)
                .map(|_| self.pres.inherited_body(s.id))
                && let (Some(body), Some(t)) = (b, c.text_mut())
            {
                t.body = body;
            }
        }
        c
    }

    /// The clipboard commands.
    pub(crate) fn clipboard_command(&mut self, cmd: &EditCommand) -> Result<Change> {
        match cmd {
            EditCommand::Copy => {
                self.copy()?;
                Ok(Change::None)
            }
            EditCommand::Cut => {
                if !self.copy()? {
                    return Ok(Change::None);
                }
                if self.view.text.is_some() {
                    return self.text_command(&kiln_protocol::commands::DeckCommand::DeleteText {
                        forward: true,
                        word: false,
                    });
                }
                if !self.view.shapes.is_empty() && !self.view.rail {
                    return self.delete_shapes();
                }
                self.slide_command(&kiln_protocol::commands::DeckCommand::DeleteSlides)
            }
            EditCommand::Paste { data }
            | EditCommand::PasteKeepSource { data }
            | EditCommand::PasteMerge { data } => self.paste(data.as_ref(), false),
            EditCommand::PasteTextOnly { data } => self.paste(data.as_ref(), true),
            EditCommand::PasteAs { mode, data } => self.paste(
                data.as_ref(),
                matches!(mode, kiln_protocol::commands::PasteMode::TextOnly),
            ),
            _ => Ok(Change::None),
        }
    }

    fn paste(&mut self, data: Option<&ClipboardData>, text_only: bool) -> Result<Change> {
        let Some(data) = data else {
            return Ok(Change::None);
        };
        let clip = (!text_only)
            .then(|| data.kiln.as_deref().and_then(decode))
            .flatten();
        match clip {
            Some(Clip::Text { paragraphs }) if self.view.text.is_some() => {
                self.paste_fragments(paragraphs)
            }
            Some(Clip::Shapes { shapes, parts }) if self.view.text.is_none() => {
                self.paste_shapes(shapes, parts)
            }
            Some(Clip::Slides {
                slides,
                layouts,
                parts,
            }) if self.view.text.is_none() => self.paste_slides(slides, &layouts, parts),
            _ => {
                let text = data.text.replace("\r\n", "\n").replace('\r', "\n");
                if text.is_empty() {
                    return Ok(Change::None);
                }
                if self.view.text.is_some() {
                    return self.type_text(&text);
                }
                // Text pasted with no text being edited becomes a text box.
                self.insert_text_box(None)?;
                self.type_text(&text)
            }
        }
    }

    fn paste_fragments(&mut self, paragraphs: Vec<Fragment>) -> Result<Change> {
        let Some(t) = self.view.text.clone() else {
            return Ok(Change::None);
        };
        self.begin_group();
        let result = (|| {
            for (i, f) in paragraphs.into_iter().enumerate() {
                let Some(t) = self.view.text.clone() else {
                    break;
                };
                if i > 0 {
                    let at = t.head;
                    self.edit_body("suite.edit.paste", t.target, |doc| {
                        kiln_doc::edit::split_paragraph(doc, at)
                    })?;
                }
                let Some(t) = self.view.text.clone() else {
                    break;
                };
                let at = t.head;
                let len = f.len();
                if len == 0 {
                    continue;
                }
                self.edit_body("suite.edit.paste", t.target, |_| {
                    Ok(kiln_doc::Transaction::new(vec![kiln_doc::Op::InsertInline {
                        para: at.para,
                        offset: at.offset,
                        fragment: f,
                    }])
                    .with_kind(kiln_doc::EditKind::Other)
                    .with_selection(
                        Some(kiln_doc::Range::caret(at)),
                        Some(kiln_doc::Range::caret(kiln_doc::Pos::new(
                            at.para,
                            at.offset + len,
                        ))),
                    ))
                })?;
            }
            Ok::<(), KilnError>(())
        })();
        self.end_group();
        let _ = t;
        result?;
        Ok(Change::Content)
    }

    fn paste_shapes(
        &mut self,
        shapes: Vec<Shape>,
        parts: BTreeMap<String, Part>,
    ) -> Result<Change> {
        let slide = self.slide_id()?;
        let existing: Vec<kiln_deck::Transform> = self
            .pres
            .slide(slide)
            .map(|s| s.shapes.iter().filter_map(|x| x.xfrm).collect())
            .unwrap_or_default();
        let mut ops: Vec<Op> = parts
            .into_iter()
            .filter(|(k, _)| !self.pres.parts.contains_key(k))
            .map(|(key, part)| Op::SetPart {
                key,
                part: Some(part),
            })
            .collect();
        let mut new = Vec::new();
        for s in shapes {
            let mut c = s.clone();
            self.renumber_shape(&mut c);
            // Pasting where the same object already sits offsets it, as PowerPoint does.
            if let Some(x) = c.xfrm
                && existing.contains(&x)
            {
                c.xfrm = Some(x.translated(
                    kiln_deck_layout::geom::emu(12.0),
                    kiln_deck_layout::geom::emu(12.0),
                ));
            }
            new.push(c);
        }
        let ids: Vec<NodeId> = new.iter().map(|s| s.id).collect();
        let ins = kiln_deck::edit::insert_shapes(&mut self.pres, slide, new, None)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        ops.extend(ins.ops);
        let mut tx = Transaction::new(ops);
        tx.selection_after = ids.clone();
        self.apply("suite.edit.paste", tx)?;
        self.view.shapes = ids;
        self.view.rail = false;
        Ok(Change::Content)
    }

    pub(crate) fn renumber_shape(&mut self, s: &mut Shape) {
        s.id = self.pres.alloc_id();
        if let ShapeKind::Group(g) = &mut s.kind {
            for c in &mut g.children {
                self.renumber_shape(c);
            }
        }
    }

    fn paste_slides(
        &mut self,
        slides: Vec<Slide>,
        layouts: &[String],
        parts: BTreeMap<String, Part>,
    ) -> Result<Change> {
        let master = self
            .master()
            .or_else(|| self.pres.masters.first())
            .ok_or_else(|| KilnError::InvalidInput("the presentation has no master".into()))?;
        let fallback = master.layouts.first().map(|l| l.id);
        let mut ops: Vec<Op> = parts
            .into_iter()
            .filter(|(k, _)| !self.pres.parts.contains_key(k))
            .map(|(key, part)| Op::SetPart {
                key,
                part: Some(part),
            })
            .collect();
        let mut at = self.view.slides.iter().max().copied().map_or(0, |i| i + 1);
        if self.pres.slides.is_empty() {
            at = 0;
        }
        let first = at;
        let mut new_slides = Vec::new();
        for (k, s) in slides.into_iter().enumerate() {
            let mut slide = s;
            // The destination's layout of the same name (Use Destination Theme), else its first.
            let same = self.pres.layout(slide.layout).is_some();
            if !same {
                let name = layouts.get(k).cloned().unwrap_or_default();
                let found = self
                    .pres
                    .masters
                    .iter()
                    .flat_map(|m| m.layouts.iter())
                    .find(|l| l.name == name)
                    .map(|l| l.id)
                    .or(fallback);
                let Some(l) = found else { continue };
                slide.layout = l;
            }
            slide.id = self.pres.alloc_id();
            // Effects point at the old shape ids: the copy starts without animations.
            slide.timeline = kiln_deck::Timeline::default();
            slide.comments.clear();
            for sh in &mut slide.shapes {
                self.renumber_shape(sh);
            }
            new_slides.push(slide);
        }
        let n = new_slides.len();
        for (k, slide) in new_slides.into_iter().enumerate() {
            ops.push(Op::InsertSlide {
                index: at + k,
                slide: Box::new(slide),
            });
        }
        if !self.pres.sections.is_empty() && n > 0 {
            let mut sec = self.pres.sections.clone();
            let target = if at == 0 {
                0
            } else {
                self.pres.section_of_index(at - 1).unwrap_or(0)
            };
            if let Some(s) = sec.get_mut(target) {
                s.len += n;
            }
            ops.push(Op::SetSections { sections: sec });
        }
        self.apply("suite.edit.paste", Transaction::new(ops))?;
        self.view.slides = (first..first + n).collect();
        self.view.current = first;
        self.view.anchor = first;
        self.view.rail = true;
        self.view.shapes.clear();
        Ok(Change::Content)
    }
}
