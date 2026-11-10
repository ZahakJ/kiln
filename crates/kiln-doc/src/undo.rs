// SPDX-License-Identifier: AGPL-3.0-or-later
//! Undo and redo over transactions, with Word's grouping: consecutive typing in one paragraph is
//! one step per word (a word and the spaces after it), so Ctrl+Z removes the last word typed.

use crate::DocError;
use crate::inline::OBJECT_CHAR;
use crate::model::Document;
use crate::ops::{EditKind, Op, Transaction};
use kiln_common::NodeId;

/// The default number of undo steps kept.
pub const DEFAULT_UNDO_LIMIT: usize = 1000;

/// Where the last typing step ended, for coalescing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TypingTail {
    para: NodeId,
    end: usize,
    last_was_space: bool,
}

/// One undo step: what was done and how to revert it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoStep {
    /// The applied transaction (re-applied by redo).
    pub forward: Transaction,
    /// Its inverse (applied by undo).
    pub inverse: Transaction,
    typing: Option<TypingTail>,
}

/// The undo and redo history of one document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndoStack {
    undo: Vec<UndoStep>,
    redo: Vec<UndoStep>,
    limit: usize,
    open_group: bool,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(DEFAULT_UNDO_LIMIT)
    }
}

/// The paragraph, offset and text of a pure typing transaction (one plain-text insertion).
fn typing_of(tx: &Transaction) -> Option<(NodeId, usize, String)> {
    if tx.kind != EditKind::Typing {
        return None;
    }
    match tx.ops.as_slice() {
        [
            Op::InsertInline {
                para,
                offset,
                fragment,
            },
        ] if fragment.objects.is_empty()
            && fragment.marks.is_empty()
            && !fragment.text.is_empty()
            && !fragment.text.contains(OBJECT_CHAR) =>
        {
            Some((*para, *offset, fragment.text.clone()))
        }
        _ => None,
    }
}

impl UndoStack {
    /// An empty history keeping at most `limit` steps (at least 1).
    #[must_use]
    pub fn new(limit: usize) -> Self {
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            limit: limit.max(1),
            open_group: false,
        }
    }

    /// Applies a transaction to the document and records it. Clears the redo history.
    ///
    /// # Errors
    ///
    /// The transaction's error; nothing is recorded then.
    pub fn apply(&mut self, doc: &mut Document, tx: Transaction) -> Result<(), DocError> {
        if tx.is_empty() {
            return Ok(());
        }
        let inverse = doc.apply(tx.clone())?;
        self.record(tx, inverse);
        Ok(())
    }

    /// Records a transaction that was already applied, with its inverse.
    pub fn record(&mut self, forward: Transaction, inverse: Transaction) {
        self.redo.clear();
        let typing = typing_of(&forward);
        if let Some((para, offset, text)) = &typing
            && self.open_group
            && let Some(last) = self.undo.last_mut()
            && let Some(tail) = last.typing
            && tail.para == *para
            && tail.end == *offset
        {
            let starts_word = text.chars().next().is_some_and(|c| !c.is_whitespace());
            if !(tail.last_was_space && starts_word) {
                let mut ops = inverse.ops;
                ops.extend(std::mem::take(&mut last.inverse.ops));
                last.inverse.ops = ops;
                last.inverse.selection_before = forward.selection_after;
                last.forward.ops.extend(forward.ops);
                last.forward.selection_after = forward.selection_after;
                last.typing = Some(TypingTail {
                    para: *para,
                    end: offset + text.chars().count(),
                    last_was_space: text.chars().last().is_some_and(char::is_whitespace),
                });
                return;
            }
        }
        let tail = typing.map(|(para, offset, text)| TypingTail {
            para,
            end: offset + text.chars().count(),
            last_was_space: text.chars().last().is_some_and(char::is_whitespace),
        });
        self.undo.push(UndoStep {
            forward,
            inverse,
            typing: tail,
        });
        self.open_group = tail.is_some();
        if self.undo.len() > self.limit {
            let excess = self.undo.len() - self.limit;
            self.undo.drain(..excess);
        }
    }

    /// Ends the current typing group (the caret moved, or the user paused).
    pub fn break_group(&mut self) {
        self.open_group = false;
    }

    /// Undoes the last step and returns the transaction applied (its `selection_after` is the
    /// selection to restore), or `None` when there is nothing to undo.
    ///
    /// # Errors
    ///
    /// The inverse failed to apply (the document was changed outside this history); the step is
    /// dropped and the document is unchanged.
    pub fn undo(&mut self, doc: &mut Document) -> Result<Option<Transaction>, DocError> {
        self.open_group = false;
        let Some(step) = self.undo.pop() else {
            return Ok(None);
        };
        doc.apply(step.inverse.clone())?;
        let applied = step.inverse.clone();
        self.redo.push(UndoStep {
            forward: step.forward,
            inverse: step.inverse,
            typing: None,
        });
        Ok(Some(applied))
    }

    /// Redoes the last undone step and returns the transaction applied.
    ///
    /// # Errors
    ///
    /// The step failed to apply; it is dropped and the document is unchanged.
    pub fn redo(&mut self, doc: &mut Document) -> Result<Option<Transaction>, DocError> {
        self.open_group = false;
        let Some(step) = self.redo.pop() else {
            return Ok(None);
        };
        let inverse = doc.apply(step.forward.clone())?;
        let forward = step.forward;
        self.undo.push(UndoStep {
            forward: forward.clone(),
            inverse,
            typing: None,
        });
        Ok(Some(forward))
    }

    /// True when there is something to undo.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    /// True when there is something to redo.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Number of undo steps.
    #[must_use]
    pub fn undo_len(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo steps.
    #[must_use]
    pub fn redo_len(&self) -> usize {
        self.redo.len()
    }

    /// The kind of the step Ctrl+Z would undo (for "Undo Typing" labels).
    #[must_use]
    pub fn undo_kind(&self) -> Option<EditKind> {
        self.undo.last().map(|s| s.forward.kind)
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.open_group = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inline::Fragment;
    use crate::pos::{Pos, Range};
    use crate::props::RunProps;

    fn typing(para: NodeId, offset: usize, text: &str) -> Transaction {
        Transaction::new(vec![Op::InsertInline {
            para,
            offset,
            fragment: Fragment::text(text, RunProps::default()),
        }])
        .with_kind(EditKind::Typing)
        .with_selection(
            Some(Range::caret(Pos::new(para, offset))),
            Some(Range::caret(Pos::new(para, offset + text.chars().count()))),
        )
    }

    fn type_text(
        doc: &mut Document,
        stack: &mut UndoStack,
        para: NodeId,
        text: &str,
    ) -> Result<(), DocError> {
        for c in text.chars() {
            let at = doc.paragraph(para).map_or(0, |p| p.content.len());
            stack.apply(doc, typing(para, at, &c.to_string()))?;
        }
        Ok(())
    }

    fn text(doc: &Document, para: NodeId) -> String {
        doc.paragraph(para).map(|p| p.text()).unwrap_or_default()
    }

    #[test]
    fn typing_is_undone_one_word_at_a_time() -> Result<(), DocError> {
        let mut doc = Document::new();
        let para = doc.body_paragraphs()[0].id;
        let mut stack = UndoStack::default();
        type_text(&mut doc, &mut stack, para, "hello big world")?;
        assert_eq!(stack.undo_len(), 3);
        assert_eq!(stack.undo_kind(), Some(EditKind::Typing));
        let sel = stack.undo(&mut doc)?;
        assert_eq!(text(&doc, para), "hello big ");
        assert_eq!(
            sel.and_then(|t| t.selection_after).map(|r| r.head.offset),
            Some(10)
        );
        stack.undo(&mut doc)?;
        assert_eq!(text(&doc, para), "hello ");
        stack.redo(&mut doc)?;
        assert_eq!(text(&doc, para), "hello big ");
        stack.undo(&mut doc)?;
        stack.undo(&mut doc)?;
        assert_eq!(text(&doc, para), "");
        assert!(!stack.can_undo() && stack.can_redo());
        assert_eq!(stack.undo(&mut doc)?, None);
        stack.redo(&mut doc)?;
        stack.redo(&mut doc)?;
        stack.redo(&mut doc)?;
        assert_eq!(text(&doc, para), "hello big world");
        assert_eq!(stack.redo(&mut doc)?, None);
        Ok(())
    }

    #[test]
    fn moving_the_caret_breaks_the_group() -> Result<(), DocError> {
        let mut doc = Document::new();
        let para = doc.body_paragraphs()[0].id;
        let mut stack = UndoStack::default();
        type_text(&mut doc, &mut stack, para, "abc")?;
        stack.break_group();
        type_text(&mut doc, &mut stack, para, "def")?;
        assert_eq!(stack.undo_len(), 2);
        // Typing somewhere else also starts a new step.
        stack.apply(&mut doc, typing(para, 0, "X"))?;
        assert_eq!(stack.undo_len(), 3);
        stack.undo(&mut doc)?;
        assert_eq!(text(&doc, para), "abcdef");
        Ok(())
    }

    #[test]
    fn new_edits_clear_redo_and_the_limit_holds() -> Result<(), DocError> {
        let mut doc = Document::new();
        let para = doc.body_paragraphs()[0].id;
        let mut stack = UndoStack::new(2);
        for i in 0..4 {
            stack.apply(
                &mut doc,
                Transaction::new(vec![Op::InsertInline {
                    para,
                    offset: i,
                    fragment: Fragment::text("x", RunProps::default()),
                }]),
            )?;
        }
        assert_eq!(stack.undo_len(), 2);
        stack.undo(&mut doc)?;
        assert_eq!(stack.redo_len(), 1);
        stack.apply(&mut doc, typing(para, 0, "y"))?;
        assert_eq!(stack.redo_len(), 0);
        stack.apply(&mut doc, Transaction::default())?;
        assert_eq!(stack.undo_len(), 2);
        let bad = Transaction::new(vec![Op::DeleteInline {
            para,
            offset: 0,
            len: 99,
        }]);
        assert!(stack.apply(&mut doc, bad).is_err());
        assert_eq!(stack.undo_len(), 2);
        stack.clear();
        assert!(!stack.can_undo() && !stack.can_redo());
        Ok(())
    }
}
