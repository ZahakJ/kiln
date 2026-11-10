// SPDX-License-Identifier: AGPL-3.0-or-later
//! The accessibility tree the surface mirrors into an off-screen DOM.

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Rect;
use crate::selection::TextPos;

/// ARIA role of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum A11yRole {
    /// `document` (Write, Notes).
    Document,
    /// `heading` (with `level`).
    Heading,
    /// `paragraph`.
    Paragraph,
    /// `list`.
    List,
    /// `listitem`.
    ListItem,
    /// `table`.
    Table,
    /// `row`.
    Row,
    /// `cell`.
    Cell,
    /// `columnheader` (a header row's cell; a Grid column letter).
    ColumnHeader,
    /// `rowheader` (a Grid row number).
    RowHeader,
    /// `grid` (Grid).
    Grid,
    /// `gridcell`.
    GridCell,
    /// `group` (a slide; a header, footer or the notes of a page).
    Group,
    /// `img` (pictures, charts; name = alt text).
    Image,
    /// `textbox` (shapes with text).
    TextBox,
}

/// One node of the tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct A11yNode {
    /// The model node it describes.
    pub id: NodeId,
    /// Role.
    pub role: A11yRole,
    /// Accessible name.
    pub name: Option<String>,
    /// Text content (leaf nodes).
    pub text: Option<String>,
    /// Bounds in document CSS pixels, once the document has layout.
    pub bounds: Option<Rect>,
    /// Heading level, 1–9.
    pub level: Option<u8>,
    /// Children, in reading order.
    pub children: Vec<NodeId>,
    /// The Accessible text view's paragraphs (`Query::A11yDocument`, Write): `text` is then the
    /// paragraph's own text, one `U+FFFC` per inline object (a picture, a field, a note
    /// reference), so offsets are the model's; this lists what each object reads as, in order.
    /// A list item's label is its `name` there.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub objects: Option<Vec<String>>,
    /// The Accessible text view's paragraphs: the tracked insertions and deletions in `text`
    /// (model offsets), and how the review display shows each.
    #[serde(default)]
    #[ts(optional = nullable)]
    pub revisions: Option<Vec<A11yRevision>>,
}

/// How the review display shows a tracked change (Word's markup modes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum A11yRevisionDisplay {
    /// Marked as inserted or deleted (All Markup).
    Marked,
    /// As ordinary text (an insertion in Simple or No Markup, a deletion in Original).
    Plain,
    /// Not shown (a deletion in Simple or No Markup, an insertion in Original).
    Hidden,
}

/// A tracked insertion or deletion in a text view paragraph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct A11yRevision {
    /// First character (model offset).
    pub start: u32,
    /// Past the last character.
    pub end: u32,
    /// An insertion (else a deletion).
    pub inserted: bool,
    /// How it is shown.
    pub display: A11yRevisionDisplay,
}

/// The tree for the visible region of a document (± one page); `nodes[0]` is the root.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct A11yTree {
    /// Nodes, root first.
    pub nodes: Vec<A11yNode>,
    /// The node with focus (`aria-activedescendant`).
    pub active: Option<NodeId>,
    /// Text for the polite live region ("Bold on").
    pub announce: Option<String>,
    /// The caret (the selection's head) in a text document, so the mirror can place the
    /// screen reader's caret inside `active`.
    pub caret: Option<TextPos>,
}
