// SPDX-License-Identifier: AGPL-3.0-or-later
//! Write documents in the session: opening, frames, editing through transactions with
//! incremental layout, caret motion across pages, undo.

use kiln_protocol::geom::Point;
use kiln_protocol::{CaretMotion, Command, DocKind, FileFormat, OpenHint, Viewport, WriteCommand};

use super::*;
use crate::frame::FrameState;

fn fixture(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/features")
        .join(name);
    std::fs::read(path).expect("fixture")
}

fn open(name: &str) -> WriteDoc {
    let read = read_docx(&fixture(name)).expect("reads");
    WriteDoc::new(read.document, read.warnings, Some(name.to_owned()))
}

fn w(cmd: WriteCommand) -> Command {
    Command::Write(cmd)
}

fn viewport() -> Viewport {
    Viewport {
        width_px: 1000.0,
        height_px: 900.0,
        scroll: Point::new(0.0, 0.0),
        zoom: 1.0,
        dpr: 1.0,
        ..Viewport::default()
    }
}

#[test]
fn opens_word_documents_into_the_write_model() {
    let app = WriteApp;
    assert!(app.opens(FileFormat::Docx) && app.opens(FileFormat::Dotx));
    assert!(!app.opens(FileFormat::Txt));
    let doc = app
        .open(
            &fixture("long-text.docx"),
            FileFormat::Docx,
            &OpenHint::default(),
        )
        .expect("opens");
    assert_eq!(doc.kind(), DocKind::Write);
    let paras = doc.paragraphs().expect("paragraphs");
    assert!(paras.len() > 20);
    assert!(doc.status().words.is_some_and(|w| w > 100));
    let tree = doc.a11y("Long text");
    assert_eq!(tree.nodes[0].role, A11yRole::Document);
    assert_eq!(
        tree.nodes
            .iter()
            .filter(|n| n.role == A11yRole::Heading)
            .count(),
        20
    );
    assert!(
        doc.save(FileFormat::Docx).is_ok(),
        "saves through the DOCX writer (WP-65)"
    );
    assert!(doc.save(FileFormat::Txt).is_err());
    assert_eq!(doc.save_formats()[0], FileFormat::Docx);
}

#[test]
fn damaged_files_are_format_errors_and_new_documents_start_blank() {
    let app = WriteApp;
    let err = app
        .open(b"not a zip", FileFormat::Docx, &OpenHint::default())
        .unwrap_err();
    assert!(matches!(err, KilnError::Format { .. }), "{err}");
    assert!(
        app.open(b"x", FileFormat::Txt, &OpenHint::default())
            .is_err()
    );
    let mut blank = app.create();
    assert_eq!(blank.paragraphs().map(|p| p.len()), Some(1));
    assert!(!blank.can_undo() && !blank.undo() && !blank.redo());
    assert_eq!(
        blank.execute(&w(WriteCommand::SelectAll)).unwrap(),
        Change::None
    );
}

#[test]
fn frames_paint_pages_and_upload_pictures() {
    let mut d = open("images-inline.docx");
    let mut frames = FrameState::new();
    let delta = frames
        .produce(d.frame_source(), &viewport(), 1)
        .expect("frame");
    assert!(delta.reset);
    assert!(!delta.tiles.is_empty(), "the first page is painted");
    assert!(!delta.atlas.is_empty());
    assert!(delta.images.len() >= 2, "{} pictures", delta.images.len());
    assert!(delta.images.iter().all(kiln_scene::ImageUpload::is_valid));
    // The next frame sends nothing new.
    let again = frames
        .produce(d.frame_source(), &viewport(), 2)
        .expect("frame");
    assert!(again.tiles.is_empty() && again.images.is_empty());
    let tree = d.a11y("Inline images");
    assert!(
        tree.nodes
            .iter()
            .any(|n| n.role == A11yRole::Image && n.bounds.is_some())
    );
}

#[test]
fn typing_updates_the_layout_incrementally() {
    let mut d = open("long-text.docx");
    let mut frames = FrameState::new();
    frames
        .produce(d.frame_source(), &viewport(), 1)
        .expect("frame");
    let pages = d.view().unwrap().layout.pages().len();
    // Click into the first body paragraph and type.
    let first = d.view().unwrap().layout.pages()[0].body;
    let at = d
        .view()
        .unwrap()
        .layout
        .to_doc(0, first.x + 40.0, first.y + 120.0);
    d.execute(&w(WriteCommand::SelectAt {
        point: Point::new(at.0, at.1),
        extend: false,
        unit: kiln_protocol::SelectUnit::Char,
    }))
    .unwrap();
    let para = d.head.para;
    let before = d.view().unwrap().layout.lines_of(para).len();
    let change = d
        .execute(&w(WriteCommand::InsertText {
            text: "kiln ".repeat(40),
        }))
        .unwrap();
    assert_eq!(change, Change::Content);
    assert!(d.can_undo());
    let delta = frames
        .produce(d.frame_source(), &viewport(), 2)
        .expect("frame");
    let l = &d.view().unwrap().layout;
    assert!(l.lines_of(para).len() > before, "the paragraph grew");
    // Lines were added: everything after moves, but the frame only re-flowed the pages near
    // the viewport; later frames catch up.
    assert!(l.stats().blocks_placed < 60, "{:?}", l.stats());
    assert!(!delta.tiles.is_empty(), "the changed page is repainted");
    for r in 3..20 {
        frames
            .produce(d.frame_source(), &viewport(), r)
            .expect("frame");
    }
    let l = &d.view().unwrap().layout;
    assert!(l.is_complete());
    assert!(l.pages().len() >= pages);
    // A small edit inside a line converges at the next paragraph.
    d.execute(&w(WriteCommand::InsertText { text: "a".into() }))
        .unwrap();
    frames
        .produce(d.frame_source(), &viewport(), 30)
        .expect("frame");
    let st = d.view().unwrap().layout.stats();
    assert!(st.converged && st.blocks_placed <= 3, "{st:?}");
    // The text is in the model and undo restores it in one step per word group.
    assert!(d.para_text(para).contains("kiln kiln"));
    while d.undo() {}
    assert!(!d.para_text(para).contains("kiln kiln"));
    frames
        .produce(d.frame_source(), &viewport(), 3)
        .expect("frame");
    assert_eq!(d.view().unwrap().layout.lines_of(para).len(), before);
}

#[test]
fn the_caret_moves_across_a_page_break() {
    let mut d = open("long-text.docx");
    let mut frames = FrameState::new();
    frames
        .produce(d.frame_source(), &viewport(), 1)
        .expect("frame");
    // The last body line of page 1.
    let (para, offset) = {
        let l = &d.view().unwrap().layout;
        let last = l
            .reading_order()
            .iter()
            .rev()
            .find(|r| r.page == 0)
            .copied()
            .unwrap();
        let line = l.line(last).unwrap();
        let info = &line.para.lines[line.line as usize];
        (line.para.id(), line.para.src.map.doc_of(info.bytes.start))
    };
    d.execute(&w(WriteCommand::SelectTo {
        pos: TextPos {
            block: para,
            offset: u32::try_from(offset).unwrap(),
        },
        extend: false,
    }))
    .unwrap();
    assert_eq!(d.status().page.map(|p| p.current), Some(1));
    d.execute(&w(WriteCommand::MoveCaret {
        motion: CaretMotion::LineDown,
        extend: false,
    }))
    .unwrap();
    assert_eq!(
        d.status().page.map(|p| p.current),
        Some(2),
        "Down crossed onto page 2"
    );
    let total = d.status().page.unwrap().total;
    assert!(total >= 10);
    d.execute(&w(WriteCommand::MoveCaret {
        motion: CaretMotion::LineUp,
        extend: true,
    }))
    .unwrap();
    assert_eq!(d.status().page.map(|p| p.current), Some(1));
    let sel = d.selection();
    assert!(!sel.rects.is_empty() && sel.caret.is_some());
    assert_eq!(d.selected_text().map(|t| t.is_empty()), Some(false));
}

#[test]
fn deleting_and_composing() {
    let mut d = WriteDoc::new(Document::new(), Vec::new(), None);
    d.execute(&w(WriteCommand::InsertText {
        text: "Hello kiln\nSecond".into(),
    }))
    .unwrap();
    assert_eq!(d.paragraphs().unwrap().len(), 2);
    d.execute(&w(WriteCommand::DeleteWordBackward)).unwrap();
    assert_eq!(d.paragraphs().unwrap()[1].text, "");
    d.execute(&w(WriteCommand::DeleteBackward)).unwrap();
    assert_eq!(d.paragraphs().unwrap().len(), 1, "joined");
    d.execute(&w(WriteCommand::SetComposition {
        text: "あ".into(),
        caret: 1,
    }))
    .unwrap();
    assert_eq!(d.paragraphs().unwrap()[0].text, "Hello kilnあ");
    let undo_before = d.history.undo_len();
    d.execute(&w(WriteCommand::CommitComposition { text: "愛".into() }))
        .unwrap();
    assert_eq!(d.paragraphs().unwrap()[0].text, "Hello kiln愛");
    assert!(d.history.undo_len() >= undo_before);
    d.execute(&w(WriteCommand::SetComposition {
        text: "x".into(),
        caret: 1,
    }))
    .unwrap();
    d.execute(&w(WriteCommand::CancelComposition)).unwrap();
    assert_eq!(d.paragraphs().unwrap()[0].text, "Hello kiln愛");
    d.execute(&w(WriteCommand::SelectAll)).unwrap();
    d.execute(&w(WriteCommand::DeleteForward)).unwrap();
    assert_eq!(d.paragraphs().unwrap()[0].text, "");
    assert!(
        d.execute(&w(WriteCommand::SelectTo {
            pos: TextPos {
                block: NodeId(999_999),
                offset: 0
            },
            extend: false
        }))
        .is_err()
    );
}
