// SPDX-License-Identifier: AGPL-3.0-or-later
//! Editing commands of Write documents: the scratch document's command set (typing, deletion,
//! caret motion, selection, IME composition) generalised to `kiln-doc` transactions, with caret
//! motion that follows the layout (lines across pages, columns and tables).
//!
//! WP-64 adds the full Write editor (formatting, styles, tables, ribbon commands) beside this in
//! `commands/`.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{Pos, Range, Transaction, edit};
use kiln_doc_layout::geometry;
use kiln_protocol::{CaretMotion, SelectUnit, TextPos, WriteCommand};
use unicode_segmentation::UnicodeSegmentation;

use super::WriteDoc;
use crate::model::Change;

/// Char offsets of the grapheme boundaries of a text (0 and the length included).
fn boundaries(text: &str) -> Vec<usize> {
    let mut out = vec![0];
    let mut n = 0;
    for g in text.graphemes(true) {
        n += g.chars().count();
        out.push(n);
    }
    out
}

/// Char offsets where words start (alphanumeric segments).
fn word_starts(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut chars = 0;
    for w in text.split_word_bounds() {
        if w.chars().any(char::is_alphanumeric) {
            out.push(chars);
        }
        chars += w.chars().count();
    }
    out
}

/// The word around a char offset as a char range.
fn word_at(text: &str, offset: usize) -> (usize, usize) {
    let mut chars = 0;
    let mut last = (0, 0);
    for w in text.split_word_bounds() {
        let n = w.chars().count();
        let r = (chars, chars + n);
        if offset < r.1 {
            return r;
        }
        last = r;
        chars += n;
    }
    last
}

impl WriteDoc {
    /// The text of a paragraph as the model counts it (objects are `U+FFFC`).
    pub(super) fn para_text(&self, id: NodeId) -> String {
        self.doc
            .paragraph(id)
            .map(|p| p.content.rope().to_string())
            .unwrap_or_default()
    }

    fn para_len(&self, id: NodeId) -> usize {
        self.doc.paragraph(id).map_or(0, |p| p.content.len())
    }

    /// Paragraphs of the story the caret is in, in order.
    pub(super) fn story_paragraphs(&self, near: NodeId) -> Vec<NodeId> {
        match self.doc.story_of(near) {
            Some(kiln_doc::Story::Body(_)) | None => {
                self.doc.body_paragraphs().iter().map(|p| p.id).collect()
            }
            Some(s) => self.doc.paragraphs_in(s).iter().map(|p| p.id).collect(),
        }
    }

    fn neighbour(&self, id: NodeId, forward: bool) -> Option<NodeId> {
        let order = self.story_paragraphs(id);
        let i = order.iter().position(|p| *p == id)?;
        if forward {
            order.get(i + 1).copied()
        } else {
            order.get(i.checked_sub(1)?).copied()
        }
    }

    fn doc_start(&self) -> Pos {
        let first = self
            .doc
            .body_paragraphs()
            .first()
            .map_or(NodeId(0), |p| p.id);
        Pos::new(first, 0)
    }

    fn doc_end(&self) -> Pos {
        let last = self
            .doc
            .body_paragraphs()
            .last()
            .map_or(NodeId(0), |p| p.id);
        Pos::new(last, self.para_len(last))
    }

    fn step(&self, p: Pos, forward: bool) -> Pos {
        let text = self.para_text(p.para);
        let b = boundaries(&text);
        if forward {
            match b.iter().find(|&&x| x > p.offset) {
                Some(&x) => Pos::new(p.para, x),
                None => self.neighbour(p.para, true).map_or(p, |n| Pos::new(n, 0)),
            }
        } else {
            match b.iter().rev().find(|&&x| x < p.offset) {
                Some(&x) => Pos::new(p.para, x),
                None => self
                    .neighbour(p.para, false)
                    .map_or(p, |n| Pos::new(n, self.para_len(n))),
            }
        }
    }

    fn word_step(&self, p: Pos, forward: bool) -> Pos {
        let text = self.para_text(p.para);
        let starts = word_starts(&text);
        if forward {
            match starts.iter().find(|&&x| x > p.offset) {
                Some(&x) => Pos::new(p.para, x),
                None if p.offset < text.chars().count() => Pos::new(p.para, text.chars().count()),
                None => self.neighbour(p.para, true).map_or(p, |n| Pos::new(n, 0)),
            }
        } else {
            match starts.iter().rev().find(|&&x| x < p.offset) {
                Some(&x) => Pos::new(p.para, x),
                None if p.offset > 0 => Pos::new(p.para, 0),
                None => self
                    .neighbour(p.para, false)
                    .map_or(p, |n| Pos::new(n, self.para_len(n))),
            }
        }
    }

    fn ordered(&self) -> (Pos, Pos) {
        Range::new(self.anchor, self.head).ordered(&self.doc)
    }

    /// Sets the selection; `keep_goal` keeps the remembered column of Up/Down.
    fn select(&mut self, anchor: Pos, head: Pos, keep_goal: bool) {
        self.anchor = anchor;
        self.head = head;
        if !keep_goal {
            self.goal_x = None;
        }
        self.history.break_group();
    }

    /// Applies an edit through the history and records it for layout.
    pub(super) fn apply(&mut self, tx: Transaction) -> Result<()> {
        if tx.is_empty() {
            return Ok(());
        }
        let after = tx.selection_after;
        let before_len = self.history.undo_len();
        self.history
            .apply(&mut self.doc, tx.clone())
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        if let Some(v) = &mut self.view {
            v.note(&tx);
        }
        self.bump_state(self.history.undo_len() > before_len);
        if let Some(r) = after {
            self.anchor = r.anchor;
            self.head = r.head;
        }
        self.goal_x = None;
        Ok(())
    }

    /// Deletes the selection; returns whether there was one.
    fn delete_selection(&mut self) -> Result<bool> {
        let (s, e) = self.ordered();
        if s == e {
            return Ok(false);
        }
        let tx = edit::delete_range(&self.doc, s, e)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?
            .with_kind(kiln_doc::EditKind::Delete)
            .with_selection(
                Some(Range::new(self.anchor, self.head)),
                Some(Range::caret(s)),
            );
        self.apply(tx)?;
        Ok(true)
    }

    fn insert(&mut self, text: &str) -> Result<()> {
        self.delete_selection()?;
        let at = self.head;
        let tx = edit::insert_text(&mut self.doc, at, text)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        self.apply(tx)
    }

    fn delete_to(&mut self, to: Pos) -> Result<()> {
        if self.delete_selection()? {
            return Ok(());
        }
        let from = self.head;
        if to == from {
            return Ok(());
        }
        let (s, e) = Range::new(from, to).ordered(&self.doc);
        let tx = edit::delete_range(&self.doc, s, e)
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?
            .with_kind(kiln_doc::EditKind::Delete)
            .with_selection(Some(Range::caret(from)), Some(Range::caret(s)));
        self.apply(tx)
    }

    /// The caret's x in document coordinates (for Up/Down).
    fn caret_x(&mut self) -> Option<f32> {
        let head = self.head;
        let v = self.view_synced();
        v.caret(head).map(|c| c.rect.x + c.rect.w / 2.0)
    }

    fn motion(&mut self, motion: CaretMotion, extend: bool) -> Pos {
        let (s, e) = self.ordered();
        let collapsed = s == e;
        let head = self.head;
        match motion {
            CaretMotion::Backward if !extend && !collapsed => s,
            CaretMotion::Forward if !extend && !collapsed => e,
            CaretMotion::Backward => self.step(head, false),
            CaretMotion::Forward => self.step(head, true),
            CaretMotion::WordBackward => self.word_step(head, false),
            CaretMotion::WordForward => self.word_step(head, true),
            CaretMotion::LineStart | CaretMotion::LineEnd => {
                let v = self.view_synced();
                let b = geometry::line_bounds(&v.layout, head);
                match (b, motion) {
                    (Some((a, _)), CaretMotion::LineStart) => a,
                    (Some((_, z)), _) => z,
                    (None, CaretMotion::LineStart) => Pos::new(head.para, 0),
                    (None, _) => Pos::new(head.para, self.para_len(head.para)),
                }
            }
            CaretMotion::LineUp | CaretMotion::LineDown => {
                let goal = self.goal_x.or_else(|| self.caret_x()).unwrap_or(0.0);
                self.goal_x = Some(goal);
                let down = motion == CaretMotion::LineDown;
                let v = self.view_synced();
                geometry::vertical(&v.layout, head, goal, down).unwrap_or(if down {
                    Pos::new(head.para, self.para_len(head.para))
                } else {
                    Pos::new(head.para, 0)
                })
            }
            CaretMotion::PageUp | CaretMotion::PageDown => {
                let goal = self.goal_x.or_else(|| self.caret_x()).unwrap_or(0.0);
                self.goal_x = Some(goal);
                let v = self.view_synced();
                let h = v.request.map_or(800.0, |r| r.visible.h).max(100.0);
                let dy = if motion == CaretMotion::PageDown {
                    h
                } else {
                    -h
                };
                let found = v
                    .caret(head)
                    .and_then(|c| v.hit(goal, c.rect.y + c.rect.h / 2.0 + dy));
                found.map_or(
                    if dy > 0.0 {
                        self.doc_end()
                    } else {
                        self.doc_start()
                    },
                    |h| h.pos,
                )
            }
            CaretMotion::DocumentStart => self.doc_start(),
            CaretMotion::DocumentEnd => self.doc_end(),
            CaretMotion::ParagraphBackward => {
                if head.offset > 0 {
                    Pos::new(head.para, 0)
                } else {
                    self.neighbour(head.para, false)
                        .map_or(head, |n| Pos::new(n, 0))
                }
            }
            CaretMotion::ParagraphForward => self
                .neighbour(head.para, true)
                .map_or(Pos::new(head.para, self.para_len(head.para)), |n| {
                    Pos::new(n, 0)
                }),
        }
    }

    fn checked(&self, p: TextPos) -> Result<Pos> {
        let pos = Pos::new(p.block, p.offset as usize);
        self.doc
            .clamp(pos)
            .filter(|c| c.offset == pos.offset)
            .ok_or_else(|| {
                KilnError::InvalidInput(format!("no position {} in {}", p.offset, p.block))
            })
    }

    /// Runs a Write command.
    pub(super) fn run(&mut self, cmd: &WriteCommand) -> Result<Change> {
        let before = (self.anchor, self.head);
        let state = self.state;
        match cmd {
            WriteCommand::InsertText { text } => {
                self.end_composition();
                self.insert(text)?;
            }
            WriteCommand::DeleteBackward => {
                let to = self.step(self.head, false);
                self.delete_to(to)?;
            }
            WriteCommand::DeleteForward => {
                let to = self.step(self.head, true);
                self.delete_to(to)?;
            }
            WriteCommand::DeleteWordBackward => {
                let to = self.word_step(self.head, false);
                self.delete_to(to)?;
            }
            WriteCommand::DeleteWordForward => {
                let to = self.word_step(self.head, true);
                self.delete_to(to)?;
            }
            WriteCommand::MoveCaret { motion, extend } => {
                let keep = matches!(
                    motion,
                    CaretMotion::LineUp
                        | CaretMotion::LineDown
                        | CaretMotion::PageUp
                        | CaretMotion::PageDown
                );
                let to = self.motion(*motion, *extend);
                let anchor = if *extend { self.anchor } else { to };
                self.select(anchor, to, keep);
            }
            WriteCommand::SelectTo { pos, extend } => {
                let to = self.checked(*pos)?;
                let anchor = if *extend { self.anchor } else { to };
                self.select(anchor, to, false);
            }
            WriteCommand::SelectAll => {
                let (a, b) = (self.doc_start(), self.doc_end());
                self.select(a, b, false);
            }
            WriteCommand::SelectAt {
                point,
                extend,
                unit,
            } => {
                let hit = self.view_synced().hit(point.x, point.y);
                let Some(hit) = hit else {
                    return Ok(Change::None);
                };
                let p = hit.pos;
                let (a, b) = match unit {
                    SelectUnit::Char => (p, p),
                    SelectUnit::Word => {
                        let (s, e) = word_at(&self.para_text(p.para), p.offset);
                        (Pos::new(p.para, s), Pos::new(p.para, e))
                    }
                    SelectUnit::Paragraph => {
                        (Pos::new(p.para, 0), Pos::new(p.para, self.para_len(p.para)))
                    }
                };
                if *extend {
                    let anchor = self.anchor;
                    let head = if self.doc.compare(b, anchor) == Some(std::cmp::Ordering::Less) {
                        a
                    } else {
                        b
                    };
                    self.select(anchor, head, false);
                } else {
                    self.select(a, b, false);
                }
            }
            WriteCommand::SetComposition { text, caret } => {
                self.set_composition(text, *caret)?;
            }
            WriteCommand::CommitComposition { text } => {
                self.end_composition();
                self.insert(text)?;
            }
            WriteCommand::CancelComposition => self.end_composition(),
        }
        Ok(if self.state != state {
            Change::Content
        } else if (self.anchor, self.head) != before || self.composition.is_some() {
            Change::Selection
        } else {
            Change::None
        })
    }

    /// Shows an IME composition: its text is in the paragraph (it lays out and paints like
    /// text) but not in the history until committed.
    fn set_composition(&mut self, text: &str, caret: u32) -> Result<()> {
        self.end_composition();
        self.delete_selection()?;
        let at = self.head;
        let mut tx = edit::insert_text(&mut self.doc, at, &text.replace('\n', " "))
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        tx.kind = kiln_doc::EditKind::Other;
        let inverse = self
            .doc
            .apply(tx.clone())
            .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
        if let Some(v) = &mut self.view {
            v.note(&tx);
        }
        let len = text.chars().count();
        self.composition = Some(super::Composition { at, len, inverse });
        let c = (caret as usize).min(len);
        self.anchor = Pos::new(at.para, at.offset + c);
        self.head = self.anchor;
        Ok(())
    }

    /// Removes the composition text (commit inserts the final text through the history).
    pub(super) fn end_composition(&mut self) {
        if let Some(c) = self.composition.take() {
            if let Ok(redo) = self.doc.apply(c.inverse.clone())
                && let Some(v) = &mut self.view
            {
                v.note(&redo);
                v.note(&c.inverse);
            }
            self.anchor = c.at;
            self.head = c.at;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_units() {
        assert_eq!(boundaries("ae\u{301}x"), vec![0, 1, 3, 4]);
        assert_eq!(word_starts("the kiln, warm"), vec![0, 4, 10]);
        assert_eq!(word_at("the kiln", 5), (4, 8));
        assert_eq!(word_at("the kiln", 99), (4, 8));
    }
}
