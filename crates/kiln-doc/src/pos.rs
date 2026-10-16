// SPDX-License-Identifier: AGPL-3.0-or-later
//! Positions and ranges: a paragraph id and a char offset, independent of layout.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

use crate::model::{Document, Story};
use kiln_common::NodeId;

/// A position: before the char at `offset` in a paragraph (`offset == len` is the end, before the
/// paragraph mark).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Pos {
    /// The paragraph.
    pub para: NodeId,
    /// Char offset in the paragraph.
    pub offset: usize,
}

impl Pos {
    /// A position.
    #[must_use]
    pub const fn new(para: NodeId, offset: usize) -> Self {
        Self { para, offset }
    }
}

/// A selection: `anchor` stays put, `head` moves with Shift+arrows. Collapsed when equal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Range {
    /// Where the selection started.
    pub anchor: Pos,
    /// Where the caret is.
    pub head: Pos,
}

impl Range {
    /// A selection.
    #[must_use]
    pub const fn new(anchor: Pos, head: Pos) -> Self {
        Self { anchor, head }
    }

    /// A caret.
    #[must_use]
    pub const fn caret(pos: Pos) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    /// True for a caret.
    #[must_use]
    pub fn is_collapsed(&self) -> bool {
        self.anchor == self.head
    }

    /// (start, end) in document order.
    #[must_use]
    pub fn ordered(&self, doc: &Document) -> (Pos, Pos) {
        if doc.compare(self.anchor, self.head) == Some(Ordering::Greater) {
            (self.head, self.anchor)
        } else {
            (self.anchor, self.head)
        }
    }
}

impl Document {
    /// The document order of two positions; `None` when a paragraph is missing or they are in
    /// different stories.
    #[must_use]
    pub fn compare(&self, a: Pos, b: Pos) -> Option<Ordering> {
        let la = self.locate(a.para)?;
        let lb = self.locate(b.para)?;
        let same_kind = |x: Story, y: Story| {
            matches!(
                (x, y),
                (Story::Body(_), Story::Body(_))
                    | (Story::HeaderFooter(_), Story::HeaderFooter(_))
                    | (Story::Note(_), Story::Note(_))
                    | (Story::Comment(_), Story::Comment(_))
            )
        };
        if !same_kind(la.story, lb.story) {
            return None;
        }
        Some(
            la.story
                .cmp(&lb.story)
                .then_with(|| la.steps.cmp(&lb.steps))
                .then(a.offset.cmp(&b.offset)),
        )
    }

    /// Clamps a position to its paragraph (offsets past the end move to the end).
    #[must_use]
    pub fn clamp(&self, pos: Pos) -> Option<Pos> {
        let p = self.paragraph(pos.para)?;
        Some(Pos::new(pos.para, pos.offset.min(p.content.len())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DocError, Fragment, Op, RunProps, Transaction};

    #[test]
    fn positions_order_in_the_document() -> Result<(), DocError> {
        let mut doc = Document::new();
        let first = doc.body_paragraphs()[0].id;
        let second = doc.alloc_id();
        doc.apply(Transaction::new(vec![
            Op::InsertInline {
                para: first,
                offset: 0,
                fragment: Fragment::text("hello", RunProps::default()),
            },
            Op::split_paragraph(&doc, first, 2, second)?,
        ]))?;
        let a = Pos::new(first, 1);
        let b = Pos::new(second, 0);
        assert_eq!(doc.compare(a, b), Some(Ordering::Less));
        assert_eq!(doc.compare(b, a), Some(Ordering::Greater));
        assert_eq!(doc.compare(a, a), Some(Ordering::Equal));
        assert_eq!(doc.compare(a, Pos::new(NodeId(999), 0)), None);
        let r = Range::new(b, a);
        assert_eq!(r.ordered(&doc), (a, b));
        assert!(!r.is_collapsed() && Range::caret(a).is_collapsed());
        assert_eq!(doc.clamp(Pos::new(second, 99)), Some(Pos::new(second, 3)));
        Ok(())
    }
}
