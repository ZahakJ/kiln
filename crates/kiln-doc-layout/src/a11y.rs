// SPDX-License-Identifier: AGPL-3.0-or-later
//! The accessibility tree of the visible pages ± 1 (PLAN.md §3.6): headings with their level,
//! paragraphs and list items with their text, tables → rows → cells, pictures with their alt
//! text, each with its bounds in document coordinates. The session turns it into the protocol's
//! `A11yTree`.

use kiln_common::NodeId;
use kiln_doc::{Block, Document, Paragraph};
use kiln_scene::Bounds;
use rustc_hash::FxHashSet;

use crate::engine::DocLayout;
use crate::flow::flat_blocks;
use crate::page::{Item, Layer};

/// The role of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    /// A heading (with its level).
    Heading,
    /// A paragraph.
    Paragraph,
    /// A list item.
    ListItem,
    /// A table.
    Table,
    /// A table row.
    Row,
    /// A table cell.
    Cell,
    /// A picture or shape.
    Image,
}

/// A node.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// The model node.
    pub id: NodeId,
    /// Role.
    pub role: Role,
    /// Accessible name (alt text of pictures).
    pub name: Option<String>,
    /// Text (paragraphs).
    pub text: Option<String>,
    /// Bounds in document coordinates.
    pub bounds: Option<Bounds>,
    /// Heading level 1–9.
    pub level: Option<u8>,
    /// Children.
    pub children: Vec<NodeId>,
}

/// The nodes of the body content on pages `first..=last` (top-level nodes first, in reading
/// order, then their descendants) and the ids of the top-level ones.
#[must_use]
pub fn visible_nodes(
    layout: &DocLayout,
    doc: &Document,
    first: usize,
    last: usize,
) -> (Vec<Node>, Vec<NodeId>) {
    let mut nodes = Vec::new();
    let mut top = Vec::new();
    let on = |id: NodeId| {
        layout
            .lines_of(id)
            .iter()
            .any(|r| (first..=last).contains(&(r.page as usize)))
    };
    let bounds_of = |id: NodeId| -> Option<Bounds> {
        let mut b: Option<Bounds> = None;
        for r in layout.lines_of(id) {
            if !(first..=last).contains(&(r.page as usize)) {
                continue;
            }
            let l = layout.line(*r)?;
            let rect = l.rect();
            let (x, y) = layout.to_doc(r.page as usize, rect.x, rect.y);
            let rr = Bounds::new(x, y, rect.w, rect.h);
            b = Some(b.map_or(rr, |a| a.union(&rr)));
        }
        b
    };
    let labels = doc.list_labels();
    let para_node = |p: &Paragraph, nodes: &mut Vec<Node>| -> NodeId {
        let r = kiln_doc::resolve::resolve_paragraph(&doc.context(), &p.props, None);
        let (role, level) = if r.outline_level < 9 {
            (Role::Heading, Some(r.outline_level + 1))
        } else if labels.contains_key(&p.id) {
            (Role::ListItem, None)
        } else {
            (Role::Paragraph, None)
        };
        let mut text = p.content.plain_text(true);
        if let Some(l) = labels.get(&p.id) {
            text = format!("{} {text}", l.text);
        }
        nodes.push(Node {
            id: p.id,
            role,
            name: None,
            text: Some(text),
            bounds: bounds_of(p.id),
            level,
            children: Vec::new(),
        });
        p.id
    };
    fn table_on(t: &kiln_doc::Table, on: &dyn Fn(NodeId) -> bool) -> bool {
        t.rows.iter().any(|r| {
            r.cells.iter().any(|c| {
                c.blocks.iter().any(|b| match b {
                    Block::Paragraph(p) => on(p.id),
                    Block::Table(t) => table_on(t, on),
                    _ => false,
                })
            })
        })
    }
    for fb in flat_blocks(doc) {
        match fb.block {
            Block::Paragraph(p) if on(p.id) => {
                let id = para_node(p, &mut nodes);
                top.push(id);
            }
            Block::Table(t) if table_on(t, &on) => {
                let ti = nodes.len();
                nodes.push(Node {
                    id: t.id,
                    role: Role::Table,
                    name: t.props.caption.clone(),
                    text: None,
                    bounds: None,
                    level: None,
                    children: Vec::new(),
                });
                top.push(t.id);
                let mut table_bounds: Option<Bounds> = None;
                for row in &t.rows {
                    let ri = nodes.len();
                    nodes.push(Node {
                        id: row.id,
                        role: Role::Row,
                        name: None,
                        text: None,
                        bounds: None,
                        level: None,
                        children: Vec::new(),
                    });
                    nodes[ti].children.push(row.id);
                    let mut row_bounds: Option<Bounds> = None;
                    for cell in &row.cells {
                        let ci = nodes.len();
                        nodes.push(Node {
                            id: cell.id,
                            role: Role::Cell,
                            name: None,
                            text: None,
                            bounds: None,
                            level: None,
                            children: Vec::new(),
                        });
                        nodes[ri].children.push(cell.id);
                        let mut cell_bounds: Option<Bounds> = None;
                        for b in &cell.blocks {
                            if let Block::Paragraph(p) = b {
                                let id = para_node(p, &mut nodes);
                                if let Some(pb) = nodes.last().and_then(|n| n.bounds) {
                                    cell_bounds = Some(cell_bounds.map_or(pb, |a| a.union(&pb)));
                                }
                                nodes[ci].children.push(id);
                            }
                        }
                        nodes[ci].bounds = cell_bounds;
                        if let Some(cb) = cell_bounds {
                            row_bounds = Some(row_bounds.map_or(cb, |a| a.union(&cb)));
                        }
                    }
                    nodes[ri].bounds = row_bounds;
                    if let Some(rb) = row_bounds {
                        table_bounds = Some(table_bounds.map_or(rb, |a| a.union(&rb)));
                    }
                }
                nodes[ti].bounds = table_bounds;
            }
            _ => {}
        }
    }
    // Pictures and shapes with their alt text.
    let mut seen = FxHashSet::default();
    for (pi, page) in layout.pages().iter().enumerate().take(last + 1).skip(first) {
        for it in page.items.iter().chain(&page.behind).chain(&page.front) {
            if let Item::Drawing(d) = it {
                if d.drawing.decorative || !seen.insert(d.drawing.id) {
                    continue;
                }
                let (x, y) = layout.to_doc(pi, d.rect.x, d.rect.y);
                nodes.push(Node {
                    id: d.drawing.id,
                    role: Role::Image,
                    name: d
                        .drawing
                        .description
                        .clone()
                        .or_else(|| d.drawing.title.clone())
                        .or_else(|| Some(d.drawing.name.clone()).filter(|n| !n.is_empty())),
                    text: None,
                    bounds: Some(Bounds::new(x, y, d.rect.w, d.rect.h)),
                    level: None,
                    children: Vec::new(),
                });
                if d.layer != Layer::Inline {
                    top.push(d.drawing.id);
                } else if let Some(n) = nodes.iter_mut().find(|n| n.id == d.para) {
                    n.children.push(d.drawing.id);
                } else {
                    top.push(d.drawing.id);
                }
            }
        }
    }
    (nodes, top)
}
