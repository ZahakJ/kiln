// SPDX-License-Identifier: AGPL-3.0-or-later
//! A bounded undo/redo history with typing coalescing and saved-state tracking.

use std::collections::VecDeque;

/// The most entries a document keeps (PLAN.md §3.8).
pub const MAX_ENTRIES: usize = 1_000;
/// The most bytes of undo data a document keeps (PLAN.md §3.8).
pub const MAX_BYTES: usize = 64 * 1024 * 1024;

/// One undoable change.
pub trait UndoEntry {
    /// Approximate heap size, for the memory cap.
    fn size_bytes(&self) -> usize;
    /// Tries to absorb `next`, which happened right after `self` (typing coalescing). Returns
    /// `false` to keep them as separate steps.
    fn try_merge(&mut self, next: &Self) -> bool;
}

/// Identifies a state of the document in its history: 0 is the state it was opened in; every
/// recorded (non-merged) change gets a new id. Comparing the current id with the id saved last
/// tells whether the document is dirty, also after undoing back to the saved state.
pub type StateId = u64;

/// Undo and redo stacks of one document.
#[derive(Debug, Clone)]
pub struct UndoStack<E> {
    done: VecDeque<(StateId, E)>,
    undone: Vec<(StateId, E)>,
    /// The state below the oldest kept entry (changes when old entries are evicted).
    base: StateId,
    next_id: StateId,
    bytes: usize,
    sealed: bool,
    max_entries: usize,
    max_bytes: usize,
}

impl<E> Default for UndoStack<E> {
    fn default() -> Self {
        Self::with_limits(MAX_ENTRIES, MAX_BYTES)
    }
}

impl<E: UndoEntry> UndoStack<E> {
    /// Records a change, merging it into the previous one when allowed. Clears the redo stack.
    pub fn push(&mut self, entry: E) {
        for (_, e) in self.undone.drain(..) {
            self.bytes = self.bytes.saturating_sub(e.size_bytes());
        }
        if !self.sealed
            && let Some((_, top)) = self.done.back_mut()
        {
            let before = top.size_bytes();
            if top.try_merge(&entry) {
                self.bytes = self.bytes.saturating_sub(before) + top.size_bytes();
                self.evict();
                return;
            }
        }
        self.sealed = false;
        self.bytes += entry.size_bytes();
        let id = self.next_id;
        self.next_id += 1;
        self.done.push_back((id, entry));
        self.evict();
    }

    fn evict(&mut self) {
        while self.done.len() > self.max_entries
            || (self.bytes > self.max_bytes && self.done.len() > 1)
        {
            if let Some((id, e)) = self.done.pop_front() {
                self.bytes = self.bytes.saturating_sub(e.size_bytes());
                self.base = id;
            }
        }
    }
}

impl<E> UndoStack<E> {
    /// An empty history with custom caps (at least one entry is always kept).
    #[must_use]
    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            done: VecDeque::new(),
            undone: Vec::new(),
            base: 0,
            next_id: 1,
            bytes: 0,
            sealed: false,
            max_entries: max_entries.max(1),
            max_bytes,
        }
    }

    /// Takes the entry to undo; the caller reverts it and hands it back with [`Self::undone`].
    pub fn pop_undo(&mut self) -> Option<(StateId, E)> {
        self.sealed = true;
        self.done.pop_back()
    }

    /// Stores a reverted entry on the redo stack.
    pub fn undone(&mut self, id: StateId, entry: E) {
        self.undone.push((id, entry));
    }

    /// Takes the entry to redo; the caller re-applies it and hands it back with [`Self::redone`].
    pub fn pop_redo(&mut self) -> Option<(StateId, E)> {
        self.sealed = true;
        self.undone.pop()
    }

    /// Stores a re-applied entry on the undo stack.
    pub fn redone(&mut self, id: StateId, entry: E) {
        self.done.push_back((id, entry));
    }

    /// The entry Undo would revert.
    #[must_use]
    pub fn peek_undo(&self) -> Option<&E> {
        self.done.back().map(|(_, e)| e)
    }

    /// The entry Redo would re-apply.
    #[must_use]
    pub fn peek_redo(&self) -> Option<&E> {
        self.undone.last().map(|(_, e)| e)
    }

    /// Whether Undo is possible.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    /// Whether Redo is possible.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.undone.is_empty()
    }

    /// Ends the current coalescing group: the next change becomes its own step (after a caret
    /// move, a save, an undo).
    pub fn seal(&mut self) {
        self.sealed = true;
    }

    /// The id of the current state.
    #[must_use]
    pub fn state(&self) -> StateId {
        self.done.back().map_or(self.base, |(id, _)| *id)
    }

    /// Number of undo steps.
    #[must_use]
    pub fn len(&self) -> usize {
        self.done.len()
    }

    /// Whether there is nothing to undo.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.done.is_empty()
    }

    /// Approximate bytes held by both stacks.
    #[must_use]
    pub fn bytes(&self) -> usize {
        self.bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Appends characters; merges while the previous entry is short.
    #[derive(Debug, Clone, PartialEq)]
    struct Typed(String);

    impl UndoEntry for Typed {
        fn size_bytes(&self) -> usize {
            self.0.len()
        }
        fn try_merge(&mut self, next: &Self) -> bool {
            if self.0.len() < 3 {
                self.0.push_str(&next.0);
                true
            } else {
                false
            }
        }
    }

    #[test]
    fn merges_until_the_entry_refuses() {
        let mut s = UndoStack::default();
        for c in ["a", "b", "c", "d"] {
            s.push(Typed(c.into()));
        }
        assert_eq!(s.len(), 2);
        assert_eq!(s.peek_undo(), Some(&Typed("d".into())));
        assert_eq!(s.bytes(), 4);
    }

    #[test]
    fn seal_stops_merging_and_states_track_history() {
        let mut s = UndoStack::default();
        assert_eq!(s.state(), 0);
        s.push(Typed("a".into()));
        let after_a = s.state();
        s.seal();
        s.push(Typed("b".into()));
        assert_eq!(s.len(), 2);
        let after_b = s.state();
        assert_ne!(after_a, after_b);

        let (id, e) = s.pop_undo().unwrap();
        assert_eq!(s.state(), after_a);
        s.undone(id, e);
        assert!(s.can_redo());
        let (id, e) = s.pop_redo().unwrap();
        s.redone(id, e);
        assert_eq!(s.state(), after_b, "redo returns to the same state id");
        assert!(!s.can_redo());
    }

    #[test]
    fn a_new_change_clears_redo() {
        let mut s = UndoStack::default();
        s.push(Typed("abc".into()));
        let (id, e) = s.pop_undo().unwrap();
        s.undone(id, e);
        assert_eq!(s.peek_redo(), Some(&Typed("abc".into())));
        s.push(Typed("x".into()));
        assert!(!s.can_redo());
        assert_eq!(s.bytes(), 1);
    }

    #[test]
    fn caps_entries_and_bytes() {
        let mut s = UndoStack::with_limits(3, 1_000);
        for i in 0..10 {
            s.seal();
            s.push(Typed(format!("{i}")));
        }
        assert_eq!(s.len(), 3);
        // The oldest kept state is the base, so undoing everything never reaches state 0.
        while let Some((id, e)) = s.pop_undo() {
            s.undone(id, e);
        }
        assert_ne!(s.state(), 0);

        let mut b = UndoStack::with_limits(100, 5);
        for _ in 0..4 {
            b.seal();
            b.push(Typed("xxx".into()));
        }
        assert_eq!(b.len(), 1, "byte cap keeps only the newest entry");
        assert!(!b.is_empty());
    }
}
