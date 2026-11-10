// SPDX-License-Identifier: AGPL-3.0-or-later
//! Completeness of the trees the session answers with: in Write and Grid through the session
//! with a viewport (what is on screen), in Deck over every slide of the feature fixtures, and the
//! whole-document trees of the Accessible text view.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use kiln_common::NodeId;
use kiln_deck::{Shape, ShapeKind};
use kiln_protocol::geom::Point;
use kiln_protocol::{
    A11yNode, A11yRole, A11yTree, DocKind, OpenHint, Query, QueryResult, ViewMode, Viewport,
};

use crate::{DeferredHost, Session};

fn bytes(dir: &str, name: &str) -> Vec<u8> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(dir)
        .join(name);
    std::fs::read(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn files(dir: &str, ext: &str) -> Vec<String> {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(dir);
    let mut out: Vec<String> = std::fs::read_dir(p)
        .expect("fixtures")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(ext))
        .collect();
    out.sort();
    out
}

fn tree(s: &Session, doc: kiln_common::DocId, q: &Query) -> A11yTree {
    match s.query(Some(doc), q).expect("a11y") {
        QueryResult::A11y(t) => t,
        other => panic!("{other:?}"),
    }
}

/// Every child exists, every node is reached once from the root, the root names the document.
fn well_formed(name: &str, t: &A11yTree) -> HashMap<NodeId, A11yNode> {
    let by_id: HashMap<NodeId, A11yNode> = t.nodes.iter().map(|n| (n.id, n.clone())).collect();
    assert_eq!(by_id.len(), t.nodes.len(), "{name}: duplicate ids");
    let root = t.nodes.first().expect("root");
    assert!(
        root.name.as_deref().is_some_and(|n| !n.is_empty()),
        "{name}: unnamed root"
    );
    let mut seen = HashSet::new();
    let mut stack = vec![root.id];
    while let Some(id) = stack.pop() {
        let n = by_id
            .get(&id)
            .unwrap_or_else(|| panic!("{name}: {id:?} missing"));
        assert!(seen.insert(id), "{name}: {id:?} twice");
        stack.extend(n.children.iter().copied());
    }
    assert_eq!(seen.len(), t.nodes.len(), "{name}: unreachable nodes");
    by_id
}

fn viewport(mode: ViewMode) -> Viewport {
    Viewport {
        width_px: 1280.0,
        height_px: 800.0,
        scroll: Point::new(0.0, 0.0),
        zoom: 1.0,
        dpr: 1.0,
        mode,
    }
}

#[test]
fn write_trees_name_every_paragraph_and_picture_on_screen_and_in_the_text_view() {
    for f in files("docx/features", ".docx") {
        let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
        let hint = OpenHint {
            name: Some(f.clone()),
            ..OpenHint::default()
        };
        let doc = s
            .open(&bytes("docx/features", &f), hint)
            .expect("opens")
            .doc;
        s.set_viewport(doc, viewport(ViewMode::PrintLayout))
            .expect("viewport");
        s.frame(doc).expect("frame");
        let visible = tree(&s, doc, &Query::A11y);
        let by_id = well_formed(&f, &visible);
        assert_eq!(visible.nodes[0].role, A11yRole::Document);
        assert!(visible.nodes[0].bounds.is_some(), "{f}: laid out");
        assert!(
            by_id.contains_key(&visible.active.expect("active")),
            "{f}: caret node"
        );
        for n in &visible.nodes {
            match n.role {
                A11yRole::Heading | A11yRole::Paragraph | A11yRole::ListItem => {
                    assert!(n.text.is_some(), "{f}: {:?} without text", n.id);
                }
                A11yRole::Image | A11yRole::TextBox | A11yRole::Group => assert!(
                    n.name.as_deref().is_some_and(|x| !x.trim().is_empty()),
                    "{f}: {:?} {:?} unnamed",
                    n.role,
                    n.id
                ),
                _ => {}
            }
        }
        let whole = tree(&s, doc, &Query::A11yDocument);
        let all = well_formed(&f, &whole);
        let paragraphs = match s.query(Some(doc), &Query::Paragraphs).expect("paragraphs") {
            QueryResult::Paragraphs(p) => p,
            other => panic!("{other:?}"),
        };
        for p in &paragraphs {
            let n = all
                .get(&p.id)
                .unwrap_or_else(|| panic!("{f}: paragraph {:?} not in the text view", p.id));
            assert!(n.text.is_some(), "{f}: {:?} without text", p.id);
        }
        assert!(whole.nodes.iter().all(|n| n.bounds.is_none()));
    }
}

#[test]
fn grid_trees_name_every_visible_cell_with_row_and_column_headers() {
    let mut checked = 0;
    for f in files("xlsx/features", ".xlsx") {
        let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
        let hint = OpenHint {
            name: Some(f.clone()),
            ..OpenHint::default()
        };
        let doc = s
            .open(&bytes("xlsx/features", &f), hint)
            .expect("opens")
            .doc;
        s.set_viewport(doc, viewport(ViewMode::Normal))
            .expect("viewport");
        s.frame(doc).expect("frame");
        for q in [Query::A11y, Query::A11yDocument] {
            let t = tree(&s, doc, &q);
            let by_id = well_formed(&f, &t);
            let grid = &t.nodes[0];
            assert_eq!(grid.role, A11yRole::Grid);
            let header = &by_id[&grid.children[0]];
            assert!(
                header
                    .children
                    .iter()
                    .all(|c| by_id[c].role == A11yRole::ColumnHeader)
            );
            for row in &grid.children[1..] {
                let row = &by_id[row];
                assert_eq!(by_id[&row.children[0]].role, A11yRole::RowHeader);
                for c in &row.children[1..] {
                    let cell = &by_id[c];
                    assert_eq!(cell.role, A11yRole::GridCell);
                    assert!(cell.name.as_deref().is_some_and(|n| !n.is_empty()));
                    assert!(cell.text.is_some());
                    checked += 1;
                }
            }
            let announce = t.announce.as_deref().expect("announce");
            assert!(
                announce.starts_with(by_id[&t.active.expect("active")].name.as_deref().unwrap())
            );
        }
    }
    assert!(checked > 5_000, "{checked} cells");
}

fn shape_ids(shapes: &[Shape], out: &mut Vec<NodeId>) {
    for sh in shapes {
        if sh.hidden {
            continue;
        }
        out.push(sh.id);
        if let ShapeKind::Group(g) = &sh.kind {
            shape_ids(&g.children, out);
        }
    }
}

#[test]
fn deck_trees_name_every_shape_of_every_slide() {
    let mut shapes = 0;
    for f in files("pptx/features", ".pptx") {
        let pres = kiln_io_pptx::read_pptx(&bytes("pptx/features", &f))
            .expect("reads")
            .presentation;
        let whole = super::presentation_tree(&pres, &f, None);
        let by_id = well_formed(&f, &whole);
        assert_eq!(
            whole.nodes[0].children.len(),
            pres.slides.len(),
            "{f}: a group per slide"
        );
        for (i, slide) in pres.slides.iter().enumerate() {
            let group = &by_id[&slide.id];
            assert_eq!(group.role, A11yRole::Group);
            let name = group.name.as_deref().unwrap_or_default();
            assert!(
                name.contains(&format!("slide {} of {}", i + 1, pres.slides.len())),
                "{name}"
            );
            let mut ids = Vec::new();
            shape_ids(&slide.shapes, &mut ids);
            for id in ids {
                let n = by_id
                    .get(&id)
                    .unwrap_or_else(|| panic!("{f}: shape {id:?} has no node"));
                assert!(
                    n.name.as_deref().is_some_and(|x| !x.trim().is_empty()),
                    "{f}: shape {id:?} unnamed"
                );
                shapes += 1;
            }
        }
    }
    assert!(shapes > 100, "{shapes} shapes");
    // Through the session: the current slide, and the text view.
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s.new_document(DocKind::Deck).expect("deck").doc;
    let t = tree(&s, doc, &Query::A11yDocument);
    well_formed("new", &t);
    assert_eq!(t.nodes[0].children.len(), 1);
}

#[test]
fn deck_text_view_paragraphs_are_addressable_and_empty_placeholders_are_text() {
    use kiln_protocol::{Command, DeckCommand};
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s.new_document(DocKind::Deck).expect("deck").doc;
    let t = tree(&s, doc, &Query::A11yDocument);
    // The title slide's empty title and subtitle take text.
    let boxes: Vec<&A11yNode> = t
        .nodes
        .iter()
        .filter(|n| n.role == A11yRole::TextBox)
        .collect();
    assert!(boxes.len() >= 2, "{boxes:?}");
    let title = boxes[0];
    let para = t
        .nodes
        .iter()
        .find(|n| n.id == title.children[0])
        .expect("paragraph node");
    assert_eq!(para.role, A11yRole::Paragraph);
    assert_eq!(para.text.as_deref(), Some(""));
    // Typing there through SelectText.
    s.execute(
        Some(doc),
        Command::Deck(DeckCommand::SelectText {
            shape: title.id,
            anchor_para: 0,
            anchor_offset: 0,
            head_para: 0,
            head_offset: 0,
        }),
    )
    .expect("select");
    s.execute(
        Some(doc),
        Command::Deck(DeckCommand::InsertText {
            text: "Kiln".into(),
        }),
    )
    .expect("type");
    s.execute(
        Some(doc),
        Command::Deck(DeckCommand::SelectText {
            shape: title.id,
            anchor_para: 0,
            anchor_offset: 1,
            head_para: 0,
            head_offset: 1,
        }),
    )
    .expect("select");
    s.execute(
        Some(doc),
        Command::Deck(DeckCommand::InsertText { text: "X".into() }),
    )
    .expect("type");
    let t = tree(&s, doc, &Query::A11yDocument);
    assert!(
        t.nodes.iter().any(|n| n.text.as_deref() == Some("KXiln")),
        "{:?}",
        t.nodes
    );
    // The caret is on the title's paragraph, after the X.
    let caret = t.caret.expect("caret");
    assert_eq!(caret.block, title.children[0]);
    assert_eq!(caret.offset, 2);
}

#[test]
fn grid_text_view_has_a_row_and_column_to_type_into_and_ranges_are_announced() {
    use kiln_protocol::{Command, GridCommand};
    let mut s = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = s.new_document(DocKind::Grid).expect("grid").doc;
    let t = tree(&s, doc, &Query::A11yDocument);
    let names: Vec<&str> = t
        .nodes
        .iter()
        .filter(|n| n.role == A11yRole::GridCell)
        .filter_map(|n| n.name.as_deref())
        .collect();
    assert_eq!(names, ["A1", "B1", "A2", "B2"]);
    s.execute(
        Some(doc),
        Command::Grid(GridCommand::SelectCell {
            row: 1,
            col: 1,
            extend: true,
            add: false,
        }),
    )
    .expect("extend");
    let t = tree(&s, doc, &Query::A11y);
    assert_eq!(t.announce.as_deref(), Some("A1 to B2 selected, A1, blank"));
}
