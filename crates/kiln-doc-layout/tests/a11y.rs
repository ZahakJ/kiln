// SPDX-License-Identifier: AGPL-3.0-or-later
//! Completeness of the accessibility tree over the DOCX fixture corpus: every paragraph with a
//! line on a page shown (body, tables, text boxes, headers, footers, notes) has a node carrying
//! its text, every picture and shape has a node with a name, every table cell has a node, list
//! items sit in lists, and the tree is well formed (each child exists, each node is reachable
//! once). The whole-document tree of the Accessible text view holds every body paragraph.

mod common;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use common::layout;
use kiln_common::NodeId;
use kiln_doc::{Block, Document};
use kiln_doc_layout::a11y::{
    Node, RevisionShow, RevisionView, Role, document_nodes, visible_nodes,
};
use kiln_doc_layout::page::Item;

fn corpus() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut out = Vec::new();
    for dir in ["fixtures/docx/features", "fixtures/docx/manual"] {
        for e in std::fs::read_dir(root.join(dir))
            .expect("fixtures")
            .flatten()
        {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "docx") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn open(p: &Path) -> Document {
    kiln_io_docx::read_docx(&std::fs::read(p).expect("fixture"))
        .expect("reads")
        .document
}

const TEXT_ROLES: [Role; 3] = [Role::Heading, Role::Paragraph, Role::ListItem];

/// Checks the tree's shape: children exist, every node is reached exactly once from `top`.
fn well_formed(name: &str, nodes: &[Node], top: &[NodeId]) -> HashMap<NodeId, Node> {
    let by_id: HashMap<NodeId, Node> = nodes.iter().map(|n| (n.id, n.clone())).collect();
    assert_eq!(by_id.len(), nodes.len(), "{name}: duplicate node ids");
    let mut reached = HashSet::new();
    let mut stack: Vec<NodeId> = top.to_vec();
    while let Some(id) = stack.pop() {
        let n = by_id
            .get(&id)
            .unwrap_or_else(|| panic!("{name}: child {id:?} has no node"));
        assert!(reached.insert(id), "{name}: {id:?} reached twice");
        stack.extend(n.children.iter().copied());
        if n.role == Role::ListItem {
            // A list item's parent is a list.
            let parent = nodes.iter().find(|p| p.children.contains(&id));
            assert_eq!(
                parent.map(|p| p.role),
                Some(Role::List),
                "{name}: list item {id:?} outside a list"
            );
        }
    }
    assert_eq!(reached.len(), nodes.len(), "{name}: unreachable nodes");
    by_id
}

#[test]
fn every_visible_paragraph_cell_and_shape_has_a_named_node() {
    let files = corpus();
    assert!(files.len() >= 19, "{} fixtures", files.len());
    let (mut paragraphs, mut cells, mut drawings, mut stories) = (0, 0, 0, 0);
    for f in &files {
        let name = f.file_stem().unwrap().to_string_lossy().into_owned();
        let doc = open(f);
        let l = layout(&doc);
        let last = l.pages().len() - 1;
        let (nodes, top) = visible_nodes(&l, &doc, 0, last);
        let by_id = well_formed(&name, &nodes, &top);
        for (pi, page) in l.pages().iter().enumerate() {
            for line in l.page_lines(pi) {
                let id = line.para.id();
                let node = by_id
                    .get(&id)
                    .unwrap_or_else(|| panic!("{name}: paragraph {id:?} on page {pi} has no node"));
                assert!(
                    TEXT_ROLES.contains(&node.role),
                    "{name}: {id:?} is {:?}",
                    node.role
                );
                let text = node.text.as_deref().expect("paragraph text");
                if let Some(p) = doc.paragraph(id) {
                    let plain = p.content.plain_text(true);
                    assert!(text.ends_with(&plain), "{name}: {text:?} for {plain:?}");
                }
                paragraphs += 1;
            }
            for it in page
                .items
                .iter()
                .chain(&page.behind)
                .chain(&page.front)
                .chain(&page.header)
                .chain(&page.footer)
            {
                let Item::Drawing(d) = it else { continue };
                if d.drawing.decorative || d.drawing.hidden {
                    continue;
                }
                let node = by_id
                    .get(&d.drawing.id)
                    .unwrap_or_else(|| panic!("{name}: drawing {:?} has no node", d.drawing.id));
                assert!(
                    node.name.as_deref().is_some_and(|n| !n.trim().is_empty()),
                    "{name}: drawing {:?} has no name",
                    d.drawing.id
                );
                drawings += 1;
            }
        }
        for n in &nodes {
            if n.role == Role::Group {
                assert!(n.name.is_some(), "{name}: unnamed group");
                stories += 1;
            }
        }
        for s in &doc.sections {
            for b in &s.blocks {
                let Block::Table(t) = b else { continue };
                for row in &t.rows {
                    for c in &row.cells {
                        let node = by_id
                            .get(&c.id)
                            .unwrap_or_else(|| panic!("{name}: cell {:?} has no node", c.id));
                        assert!(matches!(node.role, Role::Cell | Role::ColumnHeader));
                        cells += 1;
                    }
                }
            }
        }
    }
    assert!(paragraphs > 1_000, "{paragraphs} lines checked");
    assert!(cells > 20, "{cells} cells");
    assert!(drawings > 3, "{drawings} drawings");
    assert!(stories > 3, "{stories} headers, footers and notes");
}

#[test]
fn the_text_view_tree_holds_every_body_paragraph_in_order() {
    for f in corpus() {
        let name = f.file_stem().unwrap().to_string_lossy().into_owned();
        let doc = open(&f);
        let (nodes, top) = document_nodes(&doc, RevisionView::Markup);
        let by_id = well_formed(&name, &nodes, &top);
        let mut order = Vec::new();
        let mut stack: Vec<NodeId> = top.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            let n = &by_id[&id];
            if TEXT_ROLES.contains(&n.role) {
                order.push(id);
            }
            stack.extend(n.children.iter().rev().copied());
        }
        let body: Vec<NodeId> = doc.body_paragraphs().iter().map(|p| p.id).collect();
        let shown: Vec<NodeId> = order
            .iter()
            .copied()
            .filter(|id| body.contains(id))
            .collect();
        assert_eq!(shown, body, "{name}: body paragraphs in reading order");
        assert!(
            nodes.iter().all(|n| n.bounds.is_none()),
            "{name}: no bounds"
        );
    }
}

#[test]
fn lists_nest_by_level_and_header_rows_are_column_headers() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/features");
    let doc = open(&root.join("numbering.docx"));
    let (nodes, top) = document_nodes(&doc, RevisionView::Markup);
    let lists: Vec<&Node> = nodes.iter().filter(|n| n.role == Role::List).collect();
    assert!(!lists.is_empty());
    assert!(lists.iter().any(|l| l.level == Some(2)), "a nested list");
    for l in &lists {
        assert!(l.children.iter().all(|c| {
            nodes
                .iter()
                .find(|n| n.id == *c)
                .is_some_and(|n| n.role == Role::ListItem)
        }));
    }
    assert!(top.iter().any(|id| lists.iter().any(|l| l.id == *id)));
    let doc = open(&root.join("tables.docx"));
    let (nodes, _) = document_nodes(&doc, RevisionView::Markup);
    let headed = doc
        .sections
        .iter()
        .flat_map(|s| &s.blocks)
        .any(|b| matches!(b, Block::Table(t) if t.header_rows() > 0));
    assert_eq!(
        headed,
        nodes.iter().any(|n| n.role == Role::ColumnHeader),
        "header rows become column headers"
    );
}

#[test]
fn text_view_paragraphs_keep_model_offsets_and_say_what_objects_read_as() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/features");
    for name in [
        "images-inline.docx",
        "fields.docx",
        "footnotes.docx",
        "numbering.docx",
    ] {
        let doc = open(&root.join(name));
        let (nodes, _) = document_nodes(&doc, RevisionView::Markup);
        for p in doc.body_paragraphs() {
            let n = nodes.iter().find(|n| n.id == p.id).expect("node");
            let text = n.text.as_deref().unwrap();
            assert_eq!(
                text.chars().count(),
                p.content.len(),
                "{name}: model offsets"
            );
            let objects = n.objects.as_ref().expect("objects");
            assert_eq!(
                objects.len(),
                text.chars().filter(|c| *c == '\u{FFFC}').count()
            );
            if n.role == Role::ListItem {
                assert!(
                    n.name.as_deref().is_some_and(|l| !l.is_empty()),
                    "{name}: label"
                );
            }
        }
    }
}

#[test]
fn text_view_paragraphs_carry_tracked_changes_as_the_review_display_shows_them() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/docx/features");
    let doc = open(&root.join("tracked-changes.docx"));
    let all = |view| {
        let (nodes, _) = document_nodes(&doc, view);
        nodes
            .into_iter()
            .flat_map(|n| n.revisions.unwrap_or_default())
            .collect::<Vec<_>>()
    };
    let markup = all(RevisionView::Markup);
    assert!(markup.iter().any(|r| r.inserted) && markup.iter().any(|r| !r.inserted));
    assert!(
        markup
            .iter()
            .all(|r| r.show == RevisionShow::Marked && r.start < r.end)
    );
    for r in all(RevisionView::Final) {
        assert_eq!(
            r.show,
            if r.inserted {
                RevisionShow::Plain
            } else {
                RevisionShow::Hidden
            }
        );
    }
    for r in all(RevisionView::Original) {
        assert_eq!(
            r.show,
            if r.inserted {
                RevisionShow::Hidden
            } else {
                RevisionShow::Plain
            }
        );
    }
}
