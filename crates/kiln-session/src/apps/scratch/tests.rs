// SPDX-License-Identifier: AGPL-3.0-or-later
use super::*;
use proptest::prelude::*;

fn write(cmd: WriteCommand) -> Command {
    Command::Write(cmd)
}

fn type_text(d: &mut ScratchDoc, text: &str) {
    d.execute(&write(WriteCommand::InsertText { text: text.into() }))
        .unwrap();
}

fn mv(d: &mut ScratchDoc, motion: CaretMotion, extend: bool) -> Change {
    d.execute(&write(WriteCommand::MoveCaret { motion, extend }))
        .unwrap()
}

fn backspace(d: &mut ScratchDoc) -> Change {
    d.execute(&write(WriteCommand::DeleteBackward)).unwrap()
}

#[test]
fn typing_splits_and_backspace_joins_paragraphs() {
    let mut d = ScratchDoc::new();
    type_text(&mut d, "Hello\r\nworld");
    assert_eq!(d.text(), "Hello\nworld");
    let ids: Vec<NodeId> = d.paras.iter().map(|p| p.id).collect();
    assert_eq!(ids.len(), 2);
    mv(&mut d, CaretMotion::LineStart, false);
    assert_eq!(backspace(&mut d), Change::Content);
    assert_eq!(d.text(), "Helloworld");
    assert_eq!(d.paras[0].id, ids[0], "the first paragraph keeps its id");
    assert!(d.undo());
    assert_eq!(d.text(), "Hello\nworld");
    assert_eq!(
        d.paras.iter().map(|p| p.id).collect::<Vec<_>>(),
        ids,
        "undo restores paragraph ids"
    );
}

#[test]
fn backspace_removes_whole_grapheme_clusters() {
    let mut d = ScratchDoc::new();
    type_text(&mut d, "ae\u{301}👍🏽");
    backspace(&mut d);
    assert_eq!(d.text(), "ae\u{301}");
    backspace(&mut d);
    assert_eq!(d.text(), "a");
    mv(&mut d, CaretMotion::DocumentStart, false);
    assert_eq!(backspace(&mut d), Change::None, "nothing before the start");
}

#[test]
fn typing_replaces_the_selection() {
    let mut d = ScratchDoc::from_text("one two\nthree");
    mv(&mut d, CaretMotion::WordForward, true);
    assert_eq!(d.status().words, Some(1));
    assert_eq!(d.status().chars, Some(4));
    type_text(&mut d, "1 ");
    assert_eq!(d.text(), "1 two\nthree");
    mv(&mut d, CaretMotion::DocumentEnd, true);
    backspace(&mut d);
    assert_eq!(d.text(), "1 ");
}

#[test]
fn selection_across_paragraphs_counts_and_deletes() {
    let mut d = ScratchDoc::from_text("ab\ncd\nef");
    mv(&mut d, CaretMotion::Forward, false);
    mv(&mut d, CaretMotion::LineDown, true);
    mv(&mut d, CaretMotion::LineDown, true);
    assert_eq!(d.selected_text(), "b\ncd\ne");
    assert_eq!(d.status().chars, Some(4));
    backspace(&mut d);
    assert_eq!(d.text(), "af");
}

#[test]
fn caret_motions_follow_word_rules() {
    let mut d = ScratchDoc::from_text("Hello brave world\nnext");
    let head = |d: &ScratchDoc| (d.sel.head.para, d.sel.head.byte);
    mv(&mut d, CaretMotion::WordForward, false);
    assert_eq!(head(&d), (0, 6));
    mv(&mut d, CaretMotion::LineEnd, false);
    assert_eq!(head(&d), (0, 17));
    mv(&mut d, CaretMotion::WordForward, false);
    assert_eq!(
        head(&d),
        (1, 0),
        "at the end, Ctrl+Right goes to the next paragraph"
    );
    mv(&mut d, CaretMotion::Backward, false);
    assert_eq!(head(&d), (0, 17));
    mv(&mut d, CaretMotion::WordBackward, false);
    assert_eq!(head(&d), (0, 12));
    mv(&mut d, CaretMotion::LineDown, false);
    assert_eq!(head(&d), (1, 4), "column clamps to the shorter paragraph");
    mv(&mut d, CaretMotion::LineDown, false);
    assert_eq!(head(&d), (1, 4));
    mv(&mut d, CaretMotion::LineUp, false);
    mv(&mut d, CaretMotion::LineUp, false);
    assert_eq!(head(&d), (0, 0), "Up on the first line goes to the start");
    assert_eq!(mv(&mut d, CaretMotion::Backward, false), Change::None);
    mv(&mut d, CaretMotion::Forward, true);
    mv(&mut d, CaretMotion::Forward, true);
    assert_eq!(mv(&mut d, CaretMotion::Backward, false), Change::Selection);
    assert_eq!(head(&d), (0, 0), "Left collapses a selection to its start");
    mv(&mut d, CaretMotion::Forward, true);
    mv(&mut d, CaretMotion::Forward, false);
    assert_eq!(head(&d), (0, 1), "Right collapses a selection to its end");
}

#[test]
fn select_to_validates_positions() {
    let mut d = ScratchDoc::from_text("héllo\nworld");
    let first = d.paras[0].id;
    let second = d.paras[1].id;
    let pos = |block, offset| TextPos { block, offset };
    let select = |d: &mut ScratchDoc, p, extend| {
        d.execute(&write(WriteCommand::SelectTo { pos: p, extend }))
    };
    assert_eq!(
        select(&mut d, pos(second, 5), false).unwrap(),
        Change::Selection
    );
    assert_eq!(
        select(&mut d, pos(first, 2), true).unwrap(),
        Change::Selection
    );
    let sel = d.selection();
    match sel.kind {
        kiln_protocol::SelKind::Text { anchor, head } => {
            assert_eq!(anchor, pos(second, 5));
            assert_eq!(head, pos(first, 2));
        }
        other => panic!("{other:?}"),
    }
    assert!(select(&mut d, pos(second, 6), false).is_err());
    assert!(select(&mut d, pos(NodeId(999), 0), false).is_err());
}

#[test]
fn typing_coalesces_per_word() {
    let mut d = ScratchDoc::new();
    for c in "Hello brave world".chars() {
        type_text(&mut d, &c.to_string());
    }
    assert_eq!(d.history.len(), 3);
    assert_eq!(d.undo_label(), Some("write.edit.insert_text"));
    d.undo();
    assert_eq!(d.text(), "Hello brave ");
    d.undo();
    assert_eq!(d.text(), "Hello ");
    d.redo();
    d.redo();
    assert_eq!(d.text(), "Hello brave world");
    backspace(&mut d);
    backspace(&mut d);
    assert_eq!(d.history.len(), 4, "consecutive backspaces are one step");
    assert_eq!(d.undo_label(), Some("write.edit.delete_backward"));
    d.undo();
    assert_eq!(d.text(), "Hello brave world");
    assert_eq!(d.redo_label(), Some("write.edit.delete_backward"));
}

#[test]
fn moving_the_caret_ends_the_typing_step() {
    let mut d = ScratchDoc::new();
    type_text(&mut d, "a");
    mv(&mut d, CaretMotion::LineStart, false);
    type_text(&mut d, "b");
    assert_eq!(d.text(), "ba");
    assert_eq!(d.history.len(), 2);
}

#[test]
fn saves_and_reports_text() {
    let d = ScratchDoc::from_text("a\r\nb\n");
    assert_eq!(d.save(FileFormat::Txt).unwrap(), b"a\nb\n");
    assert!(d.save(FileFormat::Docx).is_err());
    assert_eq!(d.paragraphs().unwrap().len(), 3);
    let tree = d.a11y("Notes");
    assert_eq!(tree.nodes.len(), 4);
    assert_eq!(tree.nodes[0].name.as_deref(), Some("Notes"));
    assert_eq!(tree.nodes[0].children.len(), 3);
    assert_eq!(tree.active, Some(d.paras[0].id));
    let app = ScratchApp;
    assert!(app.opens(FileFormat::Md) && !app.opens(FileFormat::Xlsx));
    assert!(
        app.open(b"\xff\xfe\x00", FileFormat::Txt, &OpenHint::default())
            .is_err()
    );
    assert_eq!(app.create().kind(), DocKind::Scratch);
}

#[derive(Debug, Clone)]
enum Op {
    Type(String),
    Back,
    Move(CaretMotion, bool),
}

fn op() -> impl Strategy<Value = Op> {
    let motion = prop_oneof![
        Just(CaretMotion::Backward),
        Just(CaretMotion::Forward),
        Just(CaretMotion::WordBackward),
        Just(CaretMotion::WordForward),
        Just(CaretMotion::LineStart),
        Just(CaretMotion::LineEnd),
        Just(CaretMotion::LineUp),
        Just(CaretMotion::LineDown),
        Just(CaretMotion::DocumentStart),
        Just(CaretMotion::DocumentEnd),
    ];
    prop_oneof![
        "[a-c \\n\u{e9}\u{301}👍]{0,4}".prop_map(Op::Type),
        Just(Op::Back),
        (motion, any::<bool>()).prop_map(|(m, e)| Op::Move(m, e)),
    ]
}

proptest! {
    #[test]
    fn undo_all_restores_the_original_and_redo_all_the_result(
        start in "[ab \\n]{0,6}",
        ops in proptest::collection::vec(op(), 0..40),
    ) {
        let mut d = ScratchDoc::from_text(&start);
        let original = d.text();
        for op in ops {
            match op {
                Op::Type(t) => type_text(&mut d, &t),
                Op::Back => { backspace(&mut d); }
                Op::Move(m, e) => { mv(&mut d, m, e); }
            }
            // Invariants: positions in range and on char boundaries.
            for p in [d.sel.anchor, d.sel.head] {
                prop_assert!(p.para < d.paras.len());
                prop_assert!(d.paras[p.para].text.is_char_boundary(p.byte));
            }
        }
        let result = d.text();
        while d.undo() {}
        prop_assert_eq!(d.text(), original);
        while d.redo() {}
        prop_assert_eq!(d.text(), result);
    }
}
