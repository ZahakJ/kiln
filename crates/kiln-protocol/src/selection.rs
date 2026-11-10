// SPDX-License-Identifier: AGPL-3.0-or-later
//! Selection, caret and handles. The core owns the selection; the UI mirrors it for overlays.

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Rect;

/// A position in text: a block (paragraph) and an offset in Unicode scalar values (`char`s) from
/// its start. Offsets never split a grapheme cluster when produced by the core.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct TextPos {
    /// The paragraph.
    pub block: NodeId,
    /// Offset in `char`s.
    pub offset: u32,
}

/// A cell address (zero-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct CellAddr {
    /// Sheet index.
    pub sheet: u32,
    /// Row index.
    pub row: u32,
    /// Column index.
    pub col: u32,
}

/// A rectangular cell range (inclusive, zero-based).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub struct CellRange {
    /// Top-left cell.
    pub start: CellAddr,
    /// Bottom-right cell.
    pub end: CellAddr,
}

/// What is selected.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum SelKind {
    /// Nothing (no document, or a surface without a selection).
    #[default]
    None,
    /// A text range; `anchor == head` is a caret.
    Text {
        /// Where the selection started.
        anchor: TextPos,
        /// Where it ends (the caret side).
        head: TextPos,
    },
    /// Cell ranges with an active cell.
    Cells {
        /// The ranges, in selection order.
        ranges: Vec<CellRange>,
        /// The active cell.
        active: CellAddr,
    },
    /// Shapes (Deck, drawings).
    Shapes {
        /// The selected shapes.
        ids: Vec<NodeId>,
    },
}

/// The caret as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Caret {
    /// The caret bar, in document CSS pixels.
    pub rect: Rect,
    /// Whether the caret sits in right-to-left text.
    pub rtl: bool,
}

/// A selection handle's role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum HandleKind {
    /// Touch handle at the start of a text selection.
    SelectionStart,
    /// Touch handle at the end of a text selection.
    SelectionEnd,
    /// Grid fill handle.
    Fill,
    /// Shape resize handle, by compass direction.
    Resize(Compass),
    /// Shape rotation handle.
    Rotate,
}

/// A compass direction for resize handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum Compass {
    /// Top.
    N,
    /// Top right.
    NE,
    /// Right.
    E,
    /// Bottom right.
    SE,
    /// Bottom.
    S,
    /// Bottom left.
    SW,
    /// Left.
    W,
    /// Top left.
    NW,
}

/// A handle the UI draws and hit-tests (44 × 44 dp targets on touch).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Handle {
    /// Role.
    pub kind: HandleKind,
    /// Visual bounds in document CSS pixels.
    pub rect: Rect,
}

/// The selection of a document, with the geometry the overlay layer draws. Geometry is empty until
/// the document has layout.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct Selection {
    /// What is selected.
    pub kind: SelKind,
    /// Highlight rectangles.
    pub rects: Vec<Rect>,
    /// The caret, when one is shown.
    pub caret: Option<Caret>,
    /// Handles.
    pub handles: Vec<Handle>,
}

impl Selection {
    /// A text selection without geometry.
    #[must_use]
    pub fn text(anchor: TextPos, head: TextPos) -> Self {
        Self {
            kind: SelKind::Text { anchor, head },
            ..Self::default()
        }
    }

    /// Whether something (more than a caret) is selected.
    #[must_use]
    pub fn is_non_empty(&self) -> bool {
        match &self.kind {
            SelKind::None => false,
            SelKind::Text { anchor, head } => anchor != head,
            SelKind::Cells { .. } => true,
            SelKind::Shapes { ids } => !ids.is_empty(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_and_non_empty_selections() {
        let p = |o| TextPos {
            block: NodeId(1),
            offset: o,
        };
        assert!(!Selection::default().is_non_empty());
        assert!(!Selection::text(p(2), p(2)).is_non_empty());
        assert!(Selection::text(p(2), p(3)).is_non_empty());
        let shapes = |ids: Vec<NodeId>| Selection {
            kind: SelKind::Shapes { ids },
            ..Selection::default()
        };
        assert!(!shapes(vec![]).is_non_empty());
        assert!(shapes(vec![NodeId(4)]).is_non_empty());
        let a = CellAddr {
            sheet: 0,
            row: 0,
            col: 0,
        };
        let cells = Selection {
            kind: SelKind::Cells {
                ranges: vec![CellRange { start: a, end: a }],
                active: a,
            },
            ..Selection::default()
        };
        assert!(cells.is_non_empty());
    }
}
