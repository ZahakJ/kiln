// SPDX-License-Identifier: AGPL-3.0-or-later
//! The accessibility tree the surface mirrors into an off-screen DOM (PLAN.md §3.6).

use kiln_common::NodeId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Rect;

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
    /// `grid` (Grid).
    Grid,
    /// `gridcell`.
    GridCell,
    /// `group` (a slide).
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
}
