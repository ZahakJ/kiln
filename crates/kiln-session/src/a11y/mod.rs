// SPDX-License-Identifier: AGPL-3.0-or-later
//! Accessibility trees as the protocol carries them: the Write tree from
//! `kiln-doc-layout`'s nodes, the whole presentation for the Accessible text view, and the
//! completeness tests (every visible paragraph, cell and shape has a node with its text or name).

use kiln_common::NodeId;
use kiln_doc_layout::a11y::{Node, RevisionShow, Role};
use kiln_protocol::geom::Rect;
use kiln_protocol::selection::TextPos;
use kiln_protocol::{A11yNode, A11yRevision, A11yRevisionDisplay, A11yRole, A11yTree};

#[cfg(test)]
mod tests;

fn role(r: Role) -> A11yRole {
    match r {
        Role::Heading => A11yRole::Heading,
        Role::Paragraph => A11yRole::Paragraph,
        Role::List => A11yRole::List,
        Role::ListItem => A11yRole::ListItem,
        Role::Table => A11yRole::Table,
        Role::Row => A11yRole::Row,
        Role::Cell => A11yRole::Cell,
        Role::ColumnHeader => A11yRole::ColumnHeader,
        Role::Image => A11yRole::Image,
        Role::TextBox => A11yRole::TextBox,
        Role::Group => A11yRole::Group,
    }
}

/// A Write tree: the document node named `title` over `top`, then `nodes`.
pub(crate) fn document_tree(
    root: NodeId,
    title: &str,
    bounds: Option<Rect>,
    (nodes, top): (Vec<Node>, Vec<NodeId>),
    active: NodeId,
    caret: TextPos,
) -> A11yTree {
    let mut out = Vec::with_capacity(nodes.len() + 1);
    out.push(A11yNode {
        id: root,
        role: A11yRole::Document,
        name: Some(title.to_owned()),
        text: None,
        bounds,
        level: None,
        children: top,
        objects: None,
        revisions: None,
    });
    out.extend(nodes.into_iter().map(|n| A11yNode {
        id: n.id,
        role: role(n.role),
        name: n.name,
        text: n.text,
        bounds: n.bounds.map(|b| Rect::new(b.x, b.y, b.w, b.h)),
        level: n.level,
        children: n.children,
        objects: n.objects,
        revisions: n.revisions.map(|rs| {
            rs.into_iter()
                .map(|r| A11yRevision {
                    start: u32::try_from(r.start).unwrap_or(u32::MAX),
                    end: u32::try_from(r.end).unwrap_or(u32::MAX),
                    inserted: r.inserted,
                    display: match r.show {
                        RevisionShow::Marked => A11yRevisionDisplay::Marked,
                        RevisionShow::Plain => A11yRevisionDisplay::Plain,
                        RevisionShow::Hidden => A11yRevisionDisplay::Hidden,
                    },
                })
                .collect()
        }),
    }));
    A11yTree {
        nodes: out,
        active: Some(active),
        announce: None,
        caret: Some(caret),
    }
}

/// Every slide of a presentation as a group (named "slide n of m: title") of its shapes, under
/// a document node named `title`: the Accessible text view of Deck.
pub(crate) fn presentation_tree(
    pres: &kiln_deck::Presentation,
    title: &str,
    active: Option<NodeId>,
) -> A11yTree {
    let mut nodes = vec![A11yNode {
        id: NodeId(0),
        role: A11yRole::Document,
        name: Some(title.to_owned()),
        text: None,
        bounds: None,
        level: None,
        children: Vec::new(),
        objects: None,
        revisions: None,
    }];
    for s in &pres.slides {
        let tree = kiln_deck_layout::a11y::slide_tree(pres, s.id, title, None, None);
        if let Some(root) = tree.nodes.first() {
            nodes[0].children.push(root.id);
        }
        nodes.extend(tree.nodes);
    }
    A11yTree {
        nodes,
        active,
        announce: None,
        caret: None,
    }
}
