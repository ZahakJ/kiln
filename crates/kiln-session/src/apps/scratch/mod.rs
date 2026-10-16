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

mod text;

use kiln_common::{KilnError, NodeId, NodeIdGen, Result};
use kiln_protocol::{
    A11yNode, A11yRole, A11yTree, CaretMotion, Command, DocKind, FileFormat, OpenHint,
    ParagraphText, Selection, StatusBar, TextPos, WriteCommand,
};
use unicode_segmentation::UnicodeSegmentation;

use crate::model::{AppFactory, Change, DocModel};
use crate::undo::{StateId, UndoEntry, UndoStack};

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

/// The scratch document model.
#[derive(Debug, Clone)]
pub struct ScratchDoc {
    paras: Vec<Para>,
    ids: NodeIdGen,
    sel: Sel,
    history: UndoStack<Edit>,
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
        Ok(match cmd {
            WriteCommand::InsertText { text } => self.insert_text(text),
            WriteCommand::DeleteBackward => self.delete_backward(),
            WriteCommand::MoveCaret { motion, extend } => {
                let target = self.motion_target(*motion, *extend);
                self.set_head(target, *extend)
            }
            WriteCommand::SelectTo { pos, extend } => {
                let target = self.resolve(*pos)?;
                self.set_head(target, *extend)
            }
        })
    }

    fn undo(&mut self) -> bool {
        let Some((id, edit)) = self.history.pop_undo() else {
            return false;
        };
        self.apply(&edit, false);
        self.history.undone(id, edit);
        true
    }

    fn redo(&mut self) -> bool {
        let Some((id, edit)) = self.history.pop_redo() else {
            return false;
        };
        self.apply(&edit, true);
        self.history.redone(id, edit);
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
        Selection::text(self.wire(self.sel.anchor), self.wire(self.sel.head))
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
        let mut nodes = Vec::with_capacity(self.paras.len() + 1);
        nodes.push(A11yNode {
            id: NodeId(0),
            role: A11yRole::Document,
            name: Some(title.to_owned()),
            text: None,
            bounds: None,
            level: None,
            children: self.paras.iter().map(|p| p.id).collect(),
        });
        nodes.extend(self.paras.iter().map(|p| A11yNode {
            id: p.id,
            role: A11yRole::Paragraph,
            name: None,
            text: Some(p.text.clone()),
            bounds: None,
            level: None,
            children: Vec::new(),
        }));
        A11yTree {
            nodes,
            active: Some(self.paras[self.sel.head.para].id),
            announce: None,
        }
    }

    fn save_formats(&self) -> &'static [FileFormat] {
        &[FileFormat::Txt, FileFormat::Md]
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
