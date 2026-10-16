// SPDX-License-Identifier: AGPL-3.0-or-later
//! The scratch document: plain paragraphs of text without layout.
//!
//! It proves the whole loop (UI → bridge → session → model → events → UI) before the Write engine
//! exists, and stays as the lightweight document behind the control channel's smoke tests. It has
//! real editing semantics: grapheme-aware caret movement and deletion, word motions, selections,
//! paragraph splitting and joining, coalesced typing undo, UTF-8/UTF-16 text files.
//!
//! Every change is an [`Edit`] that replaces a contiguous run of paragraphs, so undo and redo
//! restore paragraphs together with their node ids.
//!
//! Once a surface asks for a frame (or a click needs hit testing) the document gets a real layout
//! ([`layout::DocLayout`]: `kiln-text` paragraphs on Letter pages) and paints through the Scene:
//! caret and selection geometry, layout-aware Home/End/Up/Down/Page Up/Page Down, word and
//! paragraph selection by clicks, IME composition shown underlined, and an accessibility tree of
//! the visible pages. Documents that are never painted (the control channel, tests) never pay for
//! layout, and their line motions fall back to paragraph semantics.

pub(crate) mod fonts;
mod layout;
mod text;

#[cfg(test)]
mod golden_tests;

use kiln_common::{KilnError, NodeId, NodeIdGen, Result};
use kiln_protocol::geom::Rect;
use kiln_protocol::{
    A11yNode, A11yRole, A11yTree, Caret, CaretMotion, Command, DocKind, FileFormat, Handle,
    HandleKind, OpenHint, ParagraphText, SelectUnit, Selection, StatusBar, TextPos, WriteCommand,
};
use kiln_scene::{AtlasUpdate, Bounds, ColorRole, DrawOp, Paint, Scene};
use unicode_segmentation::UnicodeSegmentation;

use self::layout::{At, DocLayout};
use crate::frame::{FrameRequest, FrameSource};
use crate::model::{AppFactory, Change, DocModel};
use crate::undo::{StateId, UndoEntry, UndoStack};

/// Paragraphs Page Up/Page Down move without a layout.
const PAGE_PARAGRAPHS: usize = 10;

/// The scratch app (registered in `apps/mod.rs`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ScratchApp;

impl AppFactory for ScratchApp {
    fn kind(&self) -> DocKind {
        DocKind::Scratch
    }

    fn opens(&self, format: FileFormat) -> bool {
        matches!(format, FileFormat::Txt | FileFormat::Md)
    }

    fn create(&self) -> Box<dyn DocModel> {
        Box::new(ScratchDoc::new())
    }

    fn open(
        &self,
        bytes: &[u8],
        format: FileFormat,
        _hint: &OpenHint,
    ) -> Result<Box<dyn DocModel>> {
        let text = text::decode(bytes, format)?;
        Ok(Box::new(ScratchDoc::from_text(&text)))
    }
}

/// A paragraph.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Para {
    id: NodeId,
    text: String,
}

/// A position: paragraph index and byte offset (always on a grapheme boundary).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Pos {
    para: usize,
    byte: usize,
}

/// Anchor and head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sel {
    anchor: Pos,
    head: Pos,
}

impl Sel {
    fn caret(p: Pos) -> Self {
        Self { anchor: p, head: p }
    }
    fn is_empty(self) -> bool {
        self.anchor == self.head
    }
    fn ordered(self) -> (Pos, Pos) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Delete,
}

/// Replaces `removed` (starting at paragraph `start`) by `inserted`.
#[derive(Debug, Clone)]
struct Edit {
    kind: EditKind,
    start: usize,
    removed: Vec<Para>,
    inserted: Vec<Para>,
    before: Sel,
    after: Sel,
    /// The text typed by a `Typing` edit (for word-wise coalescing).
    typed: String,
}

impl Edit {
    fn command_id(&self) -> &'static str {
        match self.kind {
            EditKind::Typing => "write.edit.insert_text",
            EditKind::Delete => "write.edit.delete_backward",
        }
    }
}

impl UndoEntry for Edit {
    fn size_bytes(&self) -> usize {
        let paras = |v: &[Para]| v.iter().map(|p| p.text.len() + 32).sum::<usize>();
        std::mem::size_of::<Self>()
            + paras(&self.removed)
            + paras(&self.inserted)
            + self.typed.len()
    }

    fn try_merge(&mut self, next: &Self) -> bool {
        let contiguous = self.kind == next.kind
            && self.start == next.start
            && self.after == next.before
            && self.inserted == next.removed;
        if !contiguous {
            return false;
        }
        if self.kind == EditKind::Typing {
            // One step per word: typing after a space starts a new step, and paragraph breaks
            // are their own steps.
            let breaks = |s: &str| s.contains('\n');
            let word_ended = self.typed.ends_with(char::is_whitespace)
                && !next.typed.starts_with(char::is_whitespace);
            if breaks(&self.typed) || breaks(&next.typed) || word_ended {
                return false;
            }
            self.typed.push_str(&next.typed);
        }
        self.inserted.clone_from(&next.inserted);
        self.after = next.after;
        true
    }
}

/// An IME composition in progress: its text lives in the paragraph (so it lays out and paints
/// like any text) but is not part of the history until it is committed.
#[derive(Debug, Clone)]
struct Composition {
    /// Paragraph index.
    para: usize,
    /// Byte offset where the composition starts.
    start: usize,
    /// Byte length of the current composition string.
    len: usize,
    /// The paragraph's text before the composition began.
    original: String,
}

/// The unit and original range of a word or paragraph selection, so that dragging (or
/// Shift+click) extends it by the same unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnitAnchor {
    unit: SelectUnit,
    start: Pos,
    end: Pos,
}

/// The scratch document model.
#[derive(Debug)]
pub struct ScratchDoc {
    paras: Vec<Para>,
    ids: NodeIdGen,
    sel: Sel,
    history: UndoStack<Edit>,
    layout: Option<Box<DocLayout>>,
    composition: Option<Composition>,
    /// Remembered x of consecutive Up/Down/Page moves (Word keeps the column).
    goal_x: Option<f32>,
    unit_anchor: Option<UnitAnchor>,
}

impl Clone for ScratchDoc {
    /// Clones the content and history; the copy lays itself out again when it is painted.
    fn clone(&self) -> Self {
        Self {
            paras: self.paras.clone(),
            ids: self.ids.clone(),
            sel: self.sel,
            history: self.history.clone(),
            layout: None,
            composition: self.composition.clone(),
            goal_x: self.goal_x,
            unit_anchor: self.unit_anchor,
        }
    }
}

impl Default for ScratchDoc {
    fn default() -> Self {
        Self::new()
    }
}

impl ScratchDoc {
    /// An empty document: one empty paragraph.
    #[must_use]
    pub fn new() -> Self {
        Self::from_text("")
    }

    /// A document from plain text; line breaks (`\n`, `\r\n`, `\r`) separate paragraphs.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut ids = NodeIdGen::new();
        let paras = text::split_lines(text)
            .into_iter()
            .map(|t| Para {
                id: ids.next_id(),
                text: t,
            })
            .collect();
        Self {
            paras,
            ids,
            sel: Sel::caret(Pos { para: 0, byte: 0 }),
            history: UndoStack::default(),
            layout: None,
            composition: None,
            goal_x: None,
            unit_anchor: None,
        }
    }

    /// The text, paragraphs joined by `\n`.
    #[must_use]
    pub fn text(&self) -> String {
        let parts: Vec<&str> = self.paras.iter().map(|p| p.text.as_str()).collect();
        parts.join("\n")
    }

    fn end_of(&self, para: usize) -> Pos {
        Pos {
            para,
            byte: self.paras.get(para).map_or(0, |p| p.text.len()),
        }
    }

    fn doc_end(&self) -> Pos {
        self.end_of(self.paras.len().saturating_sub(1))
    }

    /// Replaces the selection with `text` and records the edit.
    fn replace(&mut self, range: (Pos, Pos), text: &str, kind: EditKind) -> Change {
        let (start, end) = range;
        if start == end && text.is_empty() {
            return Change::None;
        }
        let removed: Vec<Para> = self.paras[start.para..=end.para].to_vec();
        let prefix = &self.paras[start.para].text[..start.byte];
        let suffix = &self.paras[end.para].text[end.byte..];
        let lines = text::split_lines(text);
        let last_index = lines.len() - 1;
        let mut inserted = Vec::with_capacity(lines.len());
        let mut caret = start;
        for (i, line) in lines.into_iter().enumerate() {
            let mut t = if i == 0 {
                prefix.to_owned()
            } else {
                String::new()
            };
            t.push_str(&line);
            if i == last_index {
                caret = Pos {
                    para: start.para + i,
                    byte: t.len(),
                };
                t.push_str(suffix);
            }
            let id = if i == 0 {
                removed[0].id
            } else {
                self.ids.next_id()
            };
            inserted.push(Para { id, text: t });
        }
        let edit = Edit {
            kind,
            start: start.para,
            removed,
            inserted,
            before: self.sel,
            after: Sel::caret(caret),
            typed: if kind == EditKind::Typing {
                text.to_owned()
            } else {
                String::new()
            },
        };
        self.apply(&edit, true);
        self.history.push(edit);
        Change::Content
    }

    fn apply(&mut self, edit: &Edit, forward: bool) {
        let (out, inn, sel) = if forward {
            (&edit.removed, &edit.inserted, edit.after)
        } else {
            (&edit.inserted, &edit.removed, edit.before)
        };
        self.paras
            .splice(edit.start..edit.start + out.len(), inn.iter().cloned());
        self.sel = sel;
    }

    fn insert_text(&mut self, text: &str) -> Change {
        let range = self.sel.ordered();
        self.replace(range, text, EditKind::Typing)
    }

    fn delete_backward(&mut self) -> Change {
        if !self.sel.is_empty() {
            let range = self.sel.ordered();
            return self.replace(range, "", EditKind::Delete);
        }
        let head = self.sel.head;
        let from = if head.byte > 0 {
            Pos {
                para: head.para,
                byte: text::prev_grapheme(&self.paras[head.para].text, head.byte),
            }
        } else if head.para > 0 {
            self.end_of(head.para - 1)
        } else {
            return Change::None;
        };
        self.replace((from, head), "", EditKind::Delete)
    }

    fn motion_target(&self, motion: CaretMotion, extend: bool) -> Pos {
        let head = self.sel.head;
        let text = &self.paras[head.para].text;
        let (start, end) = self.sel.ordered();
        match motion {
            CaretMotion::Backward if !extend && !self.sel.is_empty() => start,
            CaretMotion::Forward if !extend && !self.sel.is_empty() => end,
            CaretMotion::Backward | CaretMotion::WordBackward if head.byte == 0 => {
                if head.para == 0 {
                    head
                } else {
                    self.end_of(head.para - 1)
                }
            }
            CaretMotion::Forward | CaretMotion::WordForward if head.byte == text.len() => {
                if head.para + 1 < self.paras.len() {
                    Pos {
                        para: head.para + 1,
                        byte: 0,
                    }
                } else {
                    head
                }
            }
            CaretMotion::Backward => Pos {
                para: head.para,
                byte: text::prev_grapheme(text, head.byte),
            },
            CaretMotion::Forward => Pos {
                para: head.para,
                byte: text::next_grapheme(text, head.byte),
            },
            CaretMotion::WordBackward => Pos {
                para: head.para,
                byte: text::prev_word_start(text, head.byte),
            },
            CaretMotion::WordForward => Pos {
                para: head.para,
                byte: text::next_word_start(text, head.byte),
            },
            CaretMotion::LineStart => Pos {
                para: head.para,
                byte: 0,
            },
            CaretMotion::LineEnd => self.end_of(head.para),
            CaretMotion::LineUp | CaretMotion::LineDown => {
                let target = if motion == CaretMotion::LineUp {
                    head.para.checked_sub(1)
                } else {
                    Some(head.para + 1).filter(|p| *p < self.paras.len())
                };
                match target {
                    None if motion == CaretMotion::LineUp => Pos { para: 0, byte: 0 },
                    None => self.doc_end(),
                    Some(p) => {
                        let column = text[..head.byte].chars().count();
                        let t = &self.paras[p].text;
                        let byte = t.char_indices().nth(column).map_or(t.len(), |(b, _)| b);
                        Pos {
                            para: p,
                            byte: text::grapheme_floor(t, byte),
                        }
                    }
                }
            }
            CaretMotion::DocumentStart => Pos { para: 0, byte: 0 },
            CaretMotion::DocumentEnd => self.doc_end(),
            CaretMotion::ParagraphBackward => match (head.byte, head.para) {
                (0, 0) => head,
                (0, p) => Pos {
                    para: p - 1,
                    byte: 0,
                },
                (_, p) => Pos { para: p, byte: 0 },
            },
            CaretMotion::ParagraphForward => {
                if head.para + 1 < self.paras.len() {
                    Pos {
                        para: head.para + 1,
                        byte: 0,
                    }
                } else {
                    self.doc_end()
                }
            }
            CaretMotion::PageUp => Pos {
                para: head.para.saturating_sub(PAGE_PARAGRAPHS),
                byte: 0,
            },
            CaretMotion::PageDown => {
                let p = head.para + PAGE_PARAGRAPHS;
                if p < self.paras.len() {
                    Pos { para: p, byte: 0 }
                } else {
                    self.doc_end()
                }
            }
        }
    }

    fn set_head(&mut self, head: Pos, extend: bool) -> Change {
        let new = if extend {
            Sel {
                anchor: self.sel.anchor,
                head,
            }
        } else {
            Sel::caret(head)
        };
        if new == self.sel {
            Change::None
        } else {
            self.sel = new;
            Change::Selection
        }
    }

    fn resolve(&self, pos: TextPos) -> Result<Pos> {
        let para = self
            .paras
            .iter()
            .position(|p| p.id == pos.block)
            .ok_or_else(|| KilnError::InvalidInput(format!("no paragraph {}", pos.block)))?;
        let text = &self.paras[para].text;
        let offset = usize::try_from(pos.offset).unwrap_or(usize::MAX);
        let byte = if offset == 0 {
            0
        } else {
            match text.char_indices().nth(offset) {
                Some((b, _)) => b,
                None if text.chars().count() == offset => text.len(),
                None => {
                    return Err(KilnError::InvalidInput(format!(
                        "offset {} is past the end of paragraph {}",
                        pos.offset, pos.block
                    )));
                }
            }
        };
        Ok(Pos {
            para,
            byte: text::grapheme_floor(text, byte),
        })
    }

    fn wire(&self, p: Pos) -> TextPos {
        let para = &self.paras[p.para];
        let offset = para.text[..p.byte].chars().count();
        TextPos {
            block: para.id,
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
        }
    }

    fn selected_text(&self) -> String {
        let (s, e) = self.sel.ordered();
        if s.para == e.para {
            return self.paras[s.para].text[s.byte..e.byte].to_owned();
        }
        let mut out = self.paras[s.para].text[s.byte..].to_owned();
        for p in &self.paras[s.para + 1..e.para] {
            out.push('\n');
            out.push_str(&p.text);
        }
        out.push('\n');
        out.push_str(&self.paras[e.para].text[..e.byte]);
        out
    }
}

impl ScratchDoc {
    // ----- layout ------------------------------------------------------------------------------

    fn at(p: Pos) -> At {
        At {
            para: p.para,
            byte: p.byte,
        }
    }

    fn pos(&self, a: At) -> Pos {
        let para = a.para.min(self.paras.len().saturating_sub(1));
        let text = &self.paras[para].text;
        Pos {
            para,
            byte: text::grapheme_floor(text, a.byte.min(text.len())),
        }
    }

    /// Lays the document out if it has no layout yet.
    fn ensure_layout(&mut self) -> &mut DocLayout {
        let paras = &self.paras;
        self.layout.get_or_insert_with(|| {
            Box::new(DocLayout::new(
                paras.iter().map(|p| (p.id, p.text.as_str())),
            ))
        })
    }

    /// Brings an existing layout up to date after a change of the text.
    fn relayout(&mut self) {
        if let Some(l) = &mut self.layout {
            l.update(self.paras.iter().map(|p| (p.id, p.text.as_str())));
        }
    }

    /// The document x of the caret, for vertical motion.
    fn caret_x(&self) -> Option<f32> {
        let l = self.layout.as_ref()?;
        let c = l.caret(Self::at(self.sel.head));
        Some(c.bar.x + c.bar.w / 2.0)
    }

    // ----- deletion ----------------------------------------------------------------------------

    fn delete_forward(&mut self) -> Change {
        if !self.sel.is_empty() {
            let range = self.sel.ordered();
            return self.replace(range, "", EditKind::Delete);
        }
        let head = self.sel.head;
        let text = &self.paras[head.para].text;
        let to = if head.byte < text.len() {
            Pos {
                para: head.para,
                byte: text::next_grapheme(text, head.byte),
            }
        } else if head.para + 1 < self.paras.len() {
            Pos {
                para: head.para + 1,
                byte: 0,
            }
        } else {
            return Change::None;
        };
        self.replace((head, to), "", EditKind::Delete)
    }

    fn delete_word(&mut self, forward: bool) -> Change {
        if !self.sel.is_empty() {
            let range = self.sel.ordered();
            return self.replace(range, "", EditKind::Delete);
        }
        let head = self.sel.head;
        let text = &self.paras[head.para].text;
        let other = if forward {
            if head.byte < text.len() {
                Pos {
                    para: head.para,
                    byte: text::next_word_start(text, head.byte),
                }
            } else if head.para + 1 < self.paras.len() {
                Pos {
                    para: head.para + 1,
                    byte: 0,
                }
            } else {
                return Change::None;
            }
        } else if head.byte > 0 {
            Pos {
                para: head.para,
                byte: text::prev_word_start(text, head.byte),
            }
        } else if head.para > 0 {
            self.end_of(head.para - 1)
        } else {
            return Change::None;
        };
        let range = if forward {
            (head, other)
        } else {
            (other, head)
        };
        self.replace(range, "", EditKind::Delete)
    }

    // ----- selection by pointer --------------------------------------------------------------

    fn unit_range(&self, p: Pos, unit: SelectUnit) -> (Pos, Pos) {
        match unit {
            SelectUnit::Char => (p, p),
            SelectUnit::Word => {
                let r = self
                    .layout
                    .as_ref()
                    .map_or(p.byte..p.byte, |l| l.word_at(Self::at(p)));
                // Like Word, a double-clicked word takes its trailing spaces with it.
                let text = &self.paras[p.para].text;
                let word = text.get(r.clone()).unwrap_or("");
                let end = if word.chars().any(|c| !c.is_whitespace()) {
                    let rest = text.get(r.end..).unwrap_or("");
                    r.end + (rest.len() - rest.trim_start_matches(' ').len())
                } else {
                    r.end
                };
                (
                    Pos {
                        para: p.para,
                        byte: r.start,
                    },
                    Pos {
                        para: p.para,
                        byte: end,
                    },
                )
            }
            SelectUnit::Paragraph => (
                Pos {
                    para: p.para,
                    byte: 0,
                },
                self.end_of(p.para),
            ),
        }
    }

    fn select_at(&mut self, x: f32, y: f32, extend: bool, unit: SelectUnit) -> Result<Change> {
        let at = self
            .ensure_layout()
            .hit_test(x, y)
            .ok_or_else(|| KilnError::InvalidInput(format!("no text position at ({x}, {y})")))?;
        let p = self.pos(at);
        let anchor = self.unit_anchor.filter(|a| extend && a.unit == unit);
        let new = match (anchor, unit) {
            (Some(a), _) => {
                let (s, e) = self.unit_range(p, unit);
                if s < a.start {
                    Sel {
                        anchor: a.end,
                        head: s,
                    }
                } else {
                    Sel {
                        anchor: a.start,
                        head: e.max(a.end),
                    }
                }
            }
            (None, SelectUnit::Char) => {
                self.unit_anchor = None;
                if extend {
                    Sel {
                        anchor: self.sel.anchor,
                        head: p,
                    }
                } else {
                    Sel::caret(p)
                }
            }
            (None, _) => {
                let (s, e) = self.unit_range(p, unit);
                self.unit_anchor = Some(UnitAnchor {
                    unit,
                    start: s,
                    end: e,
                });
                Sel { anchor: s, head: e }
            }
        };
        if new == self.sel {
            Ok(Change::None)
        } else {
            self.sel = new;
            Ok(Change::Selection)
        }
    }

    // ----- IME composition ---------------------------------------------------------------------

    fn set_composition(&mut self, text: &str, caret: u32) -> Change {
        let text: String = text.chars().filter(|c| !matches!(c, '\n' | '\r')).collect();
        let mut changed = Change::None;
        if self.composition.is_none() {
            if !self.sel.is_empty() {
                let range = self.sel.ordered();
                changed = self.replace(range, "", EditKind::Delete);
            }
            let head = self.sel.head;
            self.composition = Some(Composition {
                para: head.para,
                start: head.byte,
                len: 0,
                original: self.paras[head.para].text.clone(),
            });
        }
        let Some(c) = &mut self.composition else {
            return changed;
        };
        let para = &mut self.paras[c.para].text;
        let end = (c.start + c.len).min(para.len());
        para.replace_range(c.start..end, &text);
        c.len = text.len();
        let caret_byte = text
            .char_indices()
            .nth(usize::try_from(caret).unwrap_or(usize::MAX))
            .map_or(text.len(), |(b, _)| b);
        self.sel = Sel::caret(Pos {
            para: c.para,
            byte: c.start + caret_byte,
        });
        Change::Content
    }

    /// Removes the composition string, restoring the paragraph and putting the caret where the
    /// composition started.
    fn drop_composition(&mut self) -> bool {
        let Some(c) = self.composition.take() else {
            return false;
        };
        if let Some(p) = self.paras.get_mut(c.para) {
            p.text = c.original;
        }
        self.sel = Sel::caret(Pos {
            para: c.para,
            byte: c.start,
        });
        true
    }

    fn commit_composition(&mut self, text: &str) -> Change {
        let had = self.drop_composition();
        match self.insert_text(text) {
            Change::None if had => Change::Content,
            other => other,
        }
    }

    /// Commits a composition that another command interrupts (a click, a shortcut, Undo).
    fn finish_composition(&mut self) -> bool {
        let Some(c) = &self.composition else {
            return false;
        };
        let current = self.paras[c.para].text[c.start..c.start + c.len].to_owned();
        self.commit_composition(&current);
        true
    }

    // ----- motions with layout -----------------------------------------------------------------

    fn layout_motion(&mut self, motion: CaretMotion) -> Option<Pos> {
        let head = Self::at(self.sel.head);
        let goal = self.goal_x.or_else(|| self.caret_x());
        let l = self.layout.as_ref()?;
        let at = match motion {
            CaretMotion::LineStart => At {
                para: head.para,
                byte: l.line_bounds(head).0,
            },
            CaretMotion::LineEnd => At {
                para: head.para,
                byte: l.line_bounds(head).1,
            },
            CaretMotion::LineUp | CaretMotion::LineDown => {
                let down = motion == CaretMotion::LineDown;
                l.vertical(head, goal?, down).unwrap_or(if down {
                    At {
                        para: self.paras.len() - 1,
                        byte: self.paras[self.paras.len() - 1].text.len(),
                    }
                } else {
                    At { para: 0, byte: 0 }
                })
            }
            CaretMotion::PageUp | CaretMotion::PageDown => {
                let h = l.request().map_or(800.0, |r| r.visible.h);
                let dy = if motion == CaretMotion::PageDown {
                    h
                } else {
                    -h
                };
                l.page_move(head, goal?, dy)?
            }
            _ => return None,
        };
        Some(self.pos(at))
    }

    // ----- painting ------------------------------------------------------------------------------

    fn overlay(&self) -> Vec<DrawOp> {
        let Some(l) = &self.layout else {
            return Vec::new();
        };
        let (s, e) = self.sel.ordered();
        let mut ops = Vec::new();
        if s != e {
            ops.push(DrawOp::SelectionRects {
                rects: l.selection_rects(Self::at(s), Self::at(e)),
                paint: Paint::Role(ColorRole::Selection),
            });
        }
        if let Some(c) = &self.composition {
            ops.extend(l.composition_underlines(c.para, c.start..c.start + c.len));
        }
        if s == e {
            ops.push(DrawOp::Caret {
                rect: l.caret(Self::at(self.sel.head)).bar,
                paint: Paint::Role(ColorRole::Caret),
                spark: true,
            });
        }
        ops.extend(l.handles(Self::at(s), Self::at(e)));
        ops
    }
}

fn rect(b: Bounds) -> Rect {
    Rect::new(b.x, b.y, b.w, b.h)
}

impl FrameSource for ScratchDoc {
    fn prepare(&mut self, request: &FrameRequest) -> Result<()> {
        self.ensure_layout();
        let overlay = self.overlay();
        if let Some(l) = &mut self.layout {
            l.prepare(request, overlay);
        }
        Ok(())
    }

    fn scene(&self) -> &Scene {
        static EMPTY: std::sync::OnceLock<Scene> = std::sync::OnceLock::new();
        match &self.layout {
            Some(l) => l.scene(),
            None => {
                EMPTY.get_or_init(|| Scene::new(1.0, Bounds::ZERO, Paint::Role(ColorRole::Canvas)))
            }
        }
    }

    fn atlas_updates(&mut self, all: bool) -> Vec<AtlasUpdate> {
        self.layout
            .as_mut()
            .map_or_else(Vec::new, |l| l.atlas_updates(all))
    }

    fn extent(&self) -> Bounds {
        self.layout.as_ref().map_or(Bounds::ZERO, |l| l.extent())
    }
}

impl DocModel for ScratchDoc {
    fn kind(&self) -> DocKind {
        DocKind::Scratch
    }

    fn execute(&mut self, command: &Command) -> Result<Change> {
        let Command::Write(cmd) = command else {
            return Err(KilnError::Unsupported(format!(
                "{} in a scratch document",
                command.id()
            )));
        };
        let ime = matches!(
            cmd,
            WriteCommand::SetComposition { .. }
                | WriteCommand::CommitComposition { .. }
                | WriteCommand::CancelComposition
        );
        let mut committed = false;
        if !ime {
            committed = self.finish_composition();
        }
        let vertical = matches!(
            cmd,
            WriteCommand::MoveCaret {
                motion: CaretMotion::LineUp
                    | CaretMotion::LineDown
                    | CaretMotion::PageUp
                    | CaretMotion::PageDown,
                ..
            }
        );
        if vertical && self.goal_x.is_none() {
            self.goal_x = self.caret_x();
        } else if !vertical {
            self.goal_x = None;
        }
        if !matches!(cmd, WriteCommand::SelectAt { .. }) {
            self.unit_anchor = None;
        }
        let change = match cmd {
            WriteCommand::InsertText { text } => self.insert_text(text),
            WriteCommand::DeleteBackward => self.delete_backward(),
            WriteCommand::DeleteForward => self.delete_forward(),
            WriteCommand::DeleteWordBackward => self.delete_word(false),
            WriteCommand::DeleteWordForward => self.delete_word(true),
            WriteCommand::MoveCaret { motion, extend } => {
                let target = match self.layout_motion(*motion) {
                    Some(p) => p,
                    None => self.motion_target(*motion, *extend),
                };
                self.set_head(target, *extend)
            }
            WriteCommand::SelectTo { pos, extend } => {
                let target = self.resolve(*pos)?;
                self.set_head(target, *extend)
            }
            WriteCommand::SelectAll => {
                let new = Sel {
                    anchor: Pos { para: 0, byte: 0 },
                    head: self.doc_end(),
                };
                if new == self.sel {
                    Change::None
                } else {
                    self.sel = new;
                    Change::Selection
                }
            }
            WriteCommand::SelectAt {
                point,
                extend,
                unit,
            } => self.select_at(point.x, point.y, *extend, *unit)?,
            WriteCommand::SetComposition { text, caret } => self.set_composition(text, *caret),
            WriteCommand::CommitComposition { text } => self.commit_composition(text),
            WriteCommand::CancelComposition => {
                if self.drop_composition() {
                    Change::Content
                } else {
                    Change::None
                }
            }
        };
        let change = if committed && change != Change::Content {
            Change::Content
        } else {
            change
        };
        if change == Change::Content {
            self.relayout();
        }
        Ok(change)
    }

    fn undo(&mut self) -> bool {
        self.finish_composition();
        let Some((id, edit)) = self.history.pop_undo() else {
            return false;
        };
        self.apply(&edit, false);
        self.history.undone(id, edit);
        self.relayout();
        true
    }

    fn redo(&mut self) -> bool {
        self.finish_composition();
        let Some((id, edit)) = self.history.pop_redo() else {
            return false;
        };
        self.apply(&edit, true);
        self.history.redone(id, edit);
        self.relayout();
        true
    }

    fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    fn undo_label(&self) -> Option<&'static str> {
        self.history.peek_undo().map(Edit::command_id)
    }

    fn redo_label(&self) -> Option<&'static str> {
        self.history.peek_redo().map(Edit::command_id)
    }

    fn state(&self) -> StateId {
        self.history.state()
    }

    fn seal_history(&mut self) {
        self.history.seal();
    }

    fn selection(&self) -> Selection {
        let mut sel = Selection::text(self.wire(self.sel.anchor), self.wire(self.sel.head));
        if let Some(l) = &self.layout {
            let (s, e) = self.sel.ordered();
            sel.rects = l
                .selection_rects(Self::at(s), Self::at(e))
                .into_iter()
                .map(rect)
                .collect();
            let c = l.caret(Self::at(self.sel.head));
            sel.caret = Some(Caret {
                rect: rect(c.bar),
                rtl: c.rtl,
            });
            if s != e {
                let size = layout::HANDLE_SIZE;
                for (p, kind) in [
                    (s, HandleKind::SelectionStart),
                    (e, HandleKind::SelectionEnd),
                ] {
                    let b = l.caret(Self::at(p)).bar;
                    let x = b.x + b.w / 2.0;
                    let left = if kind == HandleKind::SelectionStart {
                        x - size
                    } else {
                        x
                    };
                    sel.handles.push(Handle {
                        kind,
                        rect: Rect::new(left, b.bottom(), size, size),
                    });
                }
            }
        }
        sel
    }

    fn status(&self) -> StatusBar {
        let (words, chars) = if self.sel.is_empty() {
            self.paras.iter().fold((0, 0), |(w, c), p| {
                (
                    w + p.text.unicode_words().count(),
                    c + p.text.chars().count(),
                )
            })
        } else {
            let t = self.selected_text();
            (
                t.unicode_words().count(),
                t.chars().filter(|c| *c != '\n').count(),
            )
        };
        StatusBar {
            words: Some(u32::try_from(words).unwrap_or(u32::MAX)),
            chars: Some(u32::try_from(chars).unwrap_or(u32::MAX)),
            ..StatusBar::default()
        }
    }

    fn paragraphs(&self) -> Option<Vec<ParagraphText>> {
        Some(
            self.paras
                .iter()
                .map(|p| ParagraphText {
                    id: p.id,
                    text: p.text.clone(),
                })
                .collect(),
        )
    }

    fn a11y(&self, title: &str) -> A11yTree {
        // With a layout and a viewport: the paragraphs of the visible pages ± 1 page (and always
        // the caret's), with their bounds. Otherwise every paragraph.
        let head = self.sel.head.para;
        let visible: Vec<usize> = match self.layout.as_ref().and_then(|l| Some((l, l.request()?))) {
            Some((l, req)) => {
                let region = req.visible.inflate(0.0, layout::PAGE_H + layout::PAGE_GAP);
                let mut v = l.paras_in(&region);
                if let Err(i) = v.binary_search(&head) {
                    v.insert(i, head);
                }
                v
            }
            None => (0..self.paras.len()).collect(),
        };
        let mut nodes = Vec::with_capacity(visible.len() + 1);
        nodes.push(A11yNode {
            id: NodeId(0),
            role: A11yRole::Document,
            name: Some(title.to_owned()),
            text: None,
            bounds: self.layout.as_ref().map(|l| rect(l.extent())),
            level: None,
            children: visible.iter().map(|i| self.paras[*i].id).collect(),
        });
        nodes.extend(visible.iter().map(|i| {
            let p = &self.paras[*i];
            A11yNode {
                id: p.id,
                role: A11yRole::Paragraph,
                name: None,
                text: Some(p.text.clone()),
                bounds: self
                    .layout
                    .as_ref()
                    .and_then(|l| l.para_bounds(*i))
                    .map(rect),
                level: None,
                children: Vec::new(),
            }
        }));
        A11yTree {
            nodes,
            active: Some(self.paras[head].id),
            announce: None,
            caret: Some(self.wire(self.sel.head)),
        }
    }

    fn save_formats(&self) -> &'static [FileFormat] {
        &[FileFormat::Txt, FileFormat::Md]
    }

    fn selected_text(&self) -> Option<String> {
        Some(self.selected_text())
    }

    fn frame_source(&mut self) -> Option<&mut dyn FrameSource> {
        Some(self)
    }

    fn save(&self, format: FileFormat) -> Result<Vec<u8>> {
        if self.save_formats().contains(&format) {
            Ok(self.text().into_bytes())
        } else {
            Err(KilnError::Unsupported(format!(
                "saving a scratch document as .{}",
                format.extension()
            )))
        }
    }
}

#[cfg(test)]
mod tests;
