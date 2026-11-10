// SPDX-License-Identifier: AGPL-3.0-or-later
//! Undo and redo over transactions. Typing in one text body is one step per word, as in Write
//! and PowerPoint; [`UndoStack::begin_group`]/[`UndoStack::end_group`] merge everything in
//! between into one step (a drag that sends many transforms, a multi-part command).

use kiln_common::NodeId;
use kiln_doc::OBJECT_CHAR;

use crate::error::Result;
use crate::ops::{EditKind, Op, TextTarget, Transaction};
use crate::presentation::Presentation;

/// The default number of undo steps kept.
pub const DEFAULT_UNDO_LIMIT: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TypingTail {
    target: TextTarget,
    para: NodeId,
    end: usize,
    last_was_space: bool,
}

/// One undo step.
#[derive(Debug, Clone, PartialEq)]
pub struct UndoStep {
    /// The applied transaction (re-applied by redo).
    pub forward: Transaction,
    /// Its inverse (applied by undo).
    pub inverse: Transaction,
    typing: Option<TypingTail>,
}

/// The undo and redo history of one presentation.
#[derive(Debug, Clone, PartialEq)]
pub struct UndoStack {
    undo: Vec<UndoStep>,
    redo: Vec<UndoStep>,
    limit: usize,
    typing_open: bool,
    group_depth: usize,
    group_started: bool,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::new(DEFAULT_UNDO_LIMIT)
    }
}

/// The text target, paragraph, offset and text of a pure typing transaction.
fn typing_of(tx: &Transaction) -> Option<(TextTarget, NodeId, usize, String)> {
    if tx.kind != EditKind::Typing {
        return None;
    }
    let [Op::EditText { target, tx, .. }] = tx.ops.as_slice() else {
        return None;
    };
    match tx.ops.as_slice() {
        [
            kiln_doc::Op::InsertInline {
                para,
                offset,
                fragment,
            },
        ] if fragment.objects.is_empty()
            && fragment.marks.is_empty()
            && !fragment.text.is_empty()
            && !fragment.text.contains(OBJECT_CHAR) =>
        {
            Some((*target, *para, *offset, fragment.text.clone()))
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
            typing_open: false,
            group_depth: 0,
            group_started: false,
        }
    }

    /// Applies a transaction and records it. Clears the redo history.
    ///
    /// # Errors
    /// The transaction's error; nothing is recorded then.
    pub fn apply(&mut self, pres: &mut Presentation, tx: Transaction) -> Result<()> {
        if tx.is_empty() {
            return Ok(());
        }
        let inverse = pres.apply(tx.clone())?;
        self.record(tx, inverse);
        Ok(())
    }

    /// Starts a group: every transaction recorded until the matching [`Self::end_group`] becomes
    /// one undo step. Groups nest.
    pub fn begin_group(&mut self) {
        if self.group_depth == 0 {
            self.group_started = false;
            self.typing_open = false;
        }
        self.group_depth += 1;
    }

    /// Ends a group.
    pub fn end_group(&mut self) {
        self.group_depth = self.group_depth.saturating_sub(1);
        if self.group_depth == 0 {
            self.group_started = false;
        }
    }

    fn merge_into_last(last: &mut UndoStep, forward: Transaction, inverse: Transaction) {
        let mut ops = inverse.ops;
        ops.extend(std::mem::take(&mut last.inverse.ops));
        last.inverse.ops = ops;
        // The merged step starts where the first part started and ends where this one ends.
        last.forward.ops.extend(forward.ops);
        last.forward.selection_after = forward.selection_after;
        last.inverse.selection_before = inverse.selection_before;
    }

    /// Records a transaction that was already applied, with its inverse.
    pub fn record(&mut self, forward: Transaction, inverse: Transaction) {
        self.redo.clear();
        if self.group_depth > 0 {
            if self.group_started
                && let Some(last) = self.undo.last_mut()
            {
                Self::merge_into_last(last, forward, inverse);
                last.typing = None;
                return;
            }
            self.group_started = true;
            self.push(UndoStep {
                forward,
                inverse,
                typing: None,
            });
            return;
        }
        let typing = typing_of(&forward);
        if let Some((target, para, offset, text)) = &typing
            && self.typing_open
            && let Some(last) = self.undo.last_mut()
            && let Some(tail) = last.typing
            && tail.target == *target
            && tail.para == *para
            && tail.end == *offset
        {
            let starts_word = text.chars().next().is_some_and(|c| !c.is_whitespace());
            if !(tail.last_was_space && starts_word) {
                Self::merge_into_last(last, forward, inverse);
                last.typing = Some(TypingTail {
                    target: *target,
                    para: *para,
                    end: offset + text.chars().count(),
                    last_was_space: text.chars().last().is_some_and(char::is_whitespace),
                });
                return;
            }
        }
        let tail = typing.map(|(target, para, offset, text)| TypingTail {
            target,
            para,
            end: offset + text.chars().count(),
            last_was_space: text.chars().last().is_some_and(char::is_whitespace),
        });
        self.typing_open = tail.is_some();
        self.push(UndoStep {
            forward,
            inverse,
            typing: tail,
        });
    }

    fn push(&mut self, step: UndoStep) {
        self.undo.push(step);
        if self.undo.len() > self.limit {
            let excess = self.undo.len() - self.limit;
            self.undo.drain(..excess);
        }
    }

    /// Ends the current typing run (the caret moved, the selection changed).
    pub fn break_group(&mut self) {
        self.typing_open = false;
    }

    /// Undoes the last step; returns the transaction applied (its `selection_after` is the
    /// selection to restore), or `None` when there is nothing to undo.
    ///
    /// # Errors
    /// The inverse failed (the presentation was changed outside this history); the step is
    /// dropped and the presentation is unchanged.
    pub fn undo(&mut self, pres: &mut Presentation) -> Result<Option<Transaction>> {
        self.typing_open = false;
        let Some(step) = self.undo.pop() else {
            return Ok(None);
        };
        pres.apply(step.inverse.clone())?;
        let applied = step.inverse.clone();
        self.redo.push(step);
        Ok(Some(applied))
    }

    /// Redoes the last undone step.
    ///
    /// # Errors
    /// As [`Self::undo`].
    pub fn redo(&mut self, pres: &mut Presentation) -> Result<Option<Transaction>> {
        self.typing_open = false;
        let Some(step) = self.redo.pop() else {
            return Ok(None);
        };
        pres.apply(step.forward.clone())?;
        let applied = step.forward.clone();
        self.undo.push(step);
        Ok(Some(applied))
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

    /// The kind of the step Undo would revert (for the "Undo Typing" label).
    #[must_use]
    pub fn undo_kind(&self) -> Option<EditKind> {
        self.undo.last().map(|s| s.forward.kind)
    }

    /// Forgets everything.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.typing_open = false;
        self.group_depth = 0;
        self.group_started = false;
    }
}
