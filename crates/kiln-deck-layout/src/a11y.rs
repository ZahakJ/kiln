// SPDX-License-Identifier: AGPL-3.0-or-later
//! The accessibility tree of a slide: the slide as a group named after its number
//! and title, and its shapes in reading order — pictures as images with their alt text, text
//! shapes as text boxes with their text, tables, groups and charts.

use kiln_common::NodeId;
use kiln_deck::{Presentation, Shape, ShapeKind};
use kiln_protocol::geom::Rect;
use kiln_protocol::{A11yNode, A11yRole, A11yTree};

use crate::geom::px;

/// The title of a slide: the text of its title placeholder, else of its first text shape.
#[must_use]
pub fn slide_title(pres: &Presentation, slide: NodeId) -> String {
    let Some(s) = pres.slide(slide) else {
        return String::new();
    };
    let text = |sh: &Shape| sh.text().map(|t| t.text()).unwrap_or_default();
    s.shapes
        .iter()
        .find(|sh| sh.placeholder.is_some_and(|p| p.kind.is_title()))
        .map(text)
        .filter(|t| !t.trim().is_empty())
        .or_else(|| s.shapes.iter().map(text).find(|t| !t.trim().is_empty()))
        .map(|t| t.split('\n').next().unwrap_or("").trim().to_owned())
        .unwrap_or_default()
}

fn rect_of(pres: &Presentation, sh: &Shape) -> Option<Rect> {
    let x = crate::render::absolute_xfrm(pres, sh.id)?;
    let b = x.bounds();
    Some(Rect::new(px(b.x), px(b.y), px(b.cx), px(b.cy)))
}

/// Paragraph nodes of a text shape: `PARA_BASE + shape × PARAS_PER_SHAPE + index` (shape text has
/// no node ids of its own across shapes). Below 2^53 for any shape id under 2^38.
pub const PARA_BASE: u64 = 1 << 50;
/// Paragraphs of one shape with a node.
pub const PARAS_PER_SHAPE: u64 = 4096;

/// The node id of a shape's `index`-th paragraph.
#[must_use]
pub fn paragraph_id(shape: NodeId, index: usize) -> NodeId {
    NodeId(PARA_BASE + shape.0 * PARAS_PER_SHAPE + index as u64)
}

fn node_of(pres: &Presentation, sh: &Shape, nodes: &mut Vec<A11yNode>) -> Option<NodeId> {
    if sh.hidden {
        return None;
    }
    let name = sh
        .alt_text
        .clone()
        .filter(|a| !a.trim().is_empty())
        .or_else(|| sh.title.clone().filter(|t| !t.trim().is_empty()))
        .or_else(|| Some(sh.name.clone()).filter(|n| !n.trim().is_empty()))
        .or_else(|| {
            Some(
                match &sh.kind {
                    ShapeKind::Picture(_) => "Picture",
                    ShapeKind::Media(_) => "Video",
                    ShapeKind::Chart(_) => "Chart",
                    ShapeKind::Table(_) => "Table",
                    ShapeKind::Group(_) => "Group",
                    ShapeKind::Geometry(_) | ShapeKind::Connector(_) => "Shape",
                }
                .to_owned(),
            )
        });
    let (role, text, children) = match &sh.kind {
        ShapeKind::Picture(_) | ShapeKind::Media(_) => (A11yRole::Image, None, Vec::new()),
        ShapeKind::Chart(_) => (A11yRole::Image, None, Vec::new()),
        ShapeKind::Table(t) => {
            let mut rows = Vec::new();
            for row in &t.rows {
                let cells: Vec<String> = row.cells.iter().map(|c| c.text.text()).collect();
                rows.push(cells.join("\t"));
            }
            (A11yRole::Table, Some(rows.join("\n")), Vec::new())
        }
        ShapeKind::Group(g) => {
            let kids = g
                .children
                .iter()
                .filter_map(|c| node_of(pres, c, nodes))
                .collect();
            (A11yRole::Group, None, kids)
        }
        // A shape with a text body (empty placeholders too: text can be typed into them) is a
        // text box of its paragraphs; one without is a picture of a shape.
        ShapeKind::Geometry(_) | ShapeKind::Connector(_) => match sh.text() {
            Some(t) => {
                let mut kids = Vec::new();
                for (i, p) in t.paragraphs().enumerate().take(PARAS_PER_SHAPE as usize) {
                    let id = paragraph_id(sh.id, i);
                    nodes.push(A11yNode {
                        id,
                        role: A11yRole::Paragraph,
                        name: None,
                        text: Some(p.content.raw_text()),
                        bounds: None,
                        level: None,
                        children: Vec::new(),
                        objects: Some(p.content.objects().iter().map(|o| o.plain_text()).collect()),
                        revisions: None,
                    });
                    kids.push(id);
                }
                (A11yRole::TextBox, Some(t.text()), kids)
            }
            None => (A11yRole::Image, None, Vec::new()),
        },
    };
    nodes.push(A11yNode {
        id: sh.id,
        role,
        name,
        text,
        bounds: rect_of(pres, sh),
        level: None,
        children,
        objects: None,
        revisions: None,
    });
    Some(sh.id)
}

/// The tree of one slide; `title` is the presentation's accessible name.
#[must_use]
pub fn slide_tree(
    pres: &Presentation,
    slide: NodeId,
    title: &str,
    active: Option<NodeId>,
    announce: Option<String>,
) -> A11yTree {
    let mut nodes = Vec::new();
    let Some(s) = pres.slide(slide) else {
        return A11yTree::default();
    };
    let kids: Vec<NodeId> = s
        .shapes
        .iter()
        .filter_map(|sh| node_of(pres, sh, &mut nodes))
        .collect();
    let index = pres.slide_index(slide).unwrap_or(0) + 1;
    let total = pres.slides.len();
    let t = slide_title(pres, slide);
    let name = if t.is_empty() {
        format!("{title}, slide {index} of {total}")
    } else {
        format!("{title}, slide {index} of {total}: {t}")
    };
    nodes.insert(
        0,
        A11yNode {
            id: slide,
            role: A11yRole::Group,
            name: Some(name),
            text: s
                .notes
                .as_ref()
                .map(|n| n.text.text())
                .filter(|n| !n.is_empty()),
            bounds: Some(Rect::new(
                0.0,
                0.0,
                px(pres.slide_size.cx),
                px(pres.slide_size.cy),
            )),
            level: None,
            children: kids,
            objects: None,
            revisions: None,
        },
    );
    A11yTree {
        nodes,
        active,
        announce,
        caret: None,
    }
}
