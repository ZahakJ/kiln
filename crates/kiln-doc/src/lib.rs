// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-doc
//!
//! The Kiln Write document model, shaped like WordprocessingML (ECMA-376 Part 1 §17).
//!
//! - **Tree:** [`Document`] `>` [`Section`]`+ >` [`Block`]`*`, where a block is a [`Paragraph`],
//!   [`Table`] (rows, cells with `gridSpan`/`vMerge`), block content control ([`SdtBlock`]),
//!   multi-paragraph field result ([`FieldBlock`]) or passthrough ([`OpaqueBlock`]). Headers and
//!   footers ([`HeaderFooter`], referenced per section and kind), footnotes and endnotes
//!   ([`Note`]) and comments ([`Comment`], threaded) are separate stories. Every block-level
//!   node has a stable [`NodeId`].
//! - **Paragraph content** ([`Inlines`]): text in a [`ropey::Rope`], formatting runs
//!   ([`RunProps`] mirroring `w:rPr`), overlapping marks ([`Mark`]: hyperlinks, comment ranges,
//!   tracked changes, inline content controls) and inline objects ([`InlineObject`]: breaks,
//!   drawings, fields, note references, symbols, equations as OMML, bookmarks).
//! - **Formatting:** [`ParagraphProps`] (`w:pPr`), [`RunProps`], table/row/cell properties,
//!   [`StyleSheet`] with `docDefaults` and `basedOn`, [`Numbering`] with overrides, legal
//!   numbering and restart rules; [`resolve`] computes effective formatting the way Word does,
//!   including table style regions, toggle properties and theme fonts and colours.
//! - **Editing:** every change is a [`Transaction`] of [`Op`]s; applying one returns its inverse,
//!   so undo ([`UndoStack`], typing grouped per word), collaboration and the control channel
//!   share one path. [`edit`] builds transactions for common edits.
//! - **Queries** ([`query`]): outline, word/character counts, comments, revisions, list labels,
//!   plain text. **Storage:** postcard ([`Document::to_bytes`]).
//!
//! No layout here: `kiln-doc-layout` turns this model into pages.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod edit;
pub mod fields;
pub mod inline;
pub mod merge;
pub mod model;
pub mod numbering;
pub mod ops;
pub mod pos;
pub mod props;
pub mod query;
pub mod resolve;
pub mod revision;
pub mod sdt;
pub mod section;
mod serial;
pub mod settings;
pub mod st;
pub mod style;
pub mod template;
pub mod theme;
pub mod undo;

pub use fields::{Field, FieldInstruction, FieldKind, parse_field_instruction};
pub use inline::{
    Break, Equation, Fragment, Hyperlink, InlineObject, Inlines, LinkTarget, Mark, MarkSpan,
    NoteRef, OBJECT_CHAR, RunSpan, Symbol,
};
pub use kiln_common::NodeId;
pub use model::{
    Anchor, Block, Cell, Comment, CommentMeta, Crop, Document, Drawing, DrawingKind, FieldBlock,
    HeaderFooter, HeaderFooterPart, NodeKind, Note, OpaqueBlock, Paragraph, Placement, PosH, PosV,
    Row, SdtBlock, Section, Story, Table,
};
pub use numbering::{
    AbstractNum, Level, LevelOverride, ListCounter, ListLabel, NumInstance, Numbering,
};
pub use ops::{EditKind, Op, Transaction};
pub use pos::{Pos, Range};
pub use props::{
    Border, CellBorders, CellMargins, ColorValue, DocColor, Indent, NumberingRef, ParagraphBorders,
    ParagraphProps, RunFonts, RunProps, Shading, Spacing, TabStop, TableBorders, TableCellProps,
    TableLook, TableProps, TableRowProps, TableWidth, ToggleProp, Underline,
};
pub use resolve::{DocContext, ResolvedParagraph, ResolvedRun, TableCellContext};
pub use revision::{ParagraphRevision, RevisionInfo};
pub use sdt::SdtInfo;
pub use section::{HeaderFooterRefs, Paper, SectionProps};
pub use settings::{DocProperties, Media, Settings};
pub use style::{DocDefaults, Style, StyleSheet};
pub use template::NewDocumentOptions;
pub use theme::DocTheme;
pub use undo::UndoStack;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Why a model operation failed. Failed operations leave the document unchanged.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocError {
    /// No node has this id.
    #[error("no node {0}")]
    UnknownNode(NodeId),
    /// The node exists but is of another kind.
    #[error("{id} is not a {expected}")]
    WrongKind {
        /// The node.
        id: NodeId,
        /// What the operation needed.
        expected: &'static str,
    },
    /// The node does not hold blocks.
    #[error("{0} does not contain blocks")]
    NotAContainer(NodeId),
    /// An id that is already in use.
    #[error("{0} is already in use")]
    DuplicateId(NodeId),
    /// An offset or index beyond the content.
    #[error("position {offset} is outside 0..={len}")]
    OutOfRange {
        /// The offending offset or index.
        offset: usize,
        /// The valid length.
        len: usize,
    },
    /// An inconsistent fragment.
    #[error("invalid fragment: {0}")]
    InvalidFragment(String),
    /// An operation that would break an invariant of the model.
    #[error("invalid operation: {0}")]
    InvalidOp(String),
    /// A style problem (empty id, cycle).
    #[error("invalid style: {0}")]
    InvalidStyle(String),
    /// No style has this id.
    #[error("no style {0}")]
    UnknownStyle(String),
    /// No such list.
    #[error("no list {0}")]
    UnknownList(String),
    /// Stored data that breaks an invariant, or bytes that are not a document.
    #[error("corrupt document: {0}")]
    Corrupt(String),
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }

    #[test]
    fn errors_describe_themselves() {
        let e = super::DocError::OutOfRange { offset: 9, len: 3 };
        assert_eq!(e.to_string(), "position 9 is outside 0..=3");
        let k = super::DocError::WrongKind {
            id: super::NodeId(4),
            expected: "table",
        };
        assert_eq!(k.to_string(), "node#4 is not a table");
    }
}
