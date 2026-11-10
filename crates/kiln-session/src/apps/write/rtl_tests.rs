// SPDX-License-Identifier: AGPL-3.0-or-later
//! Right-to-left editing through the commands the chrome sends:
//! the caret moves in logical order and is drawn where that position shows, with Word's second
//! caret where the directions meet; Home and End, word moves, Shift selection across a direction
//! change, Backspace and Delete at a boundary, a click on right-to-left text, and the alignment
//! buttons, on `fixtures/docx/rtl/mixed-runs.docx`.

use kiln_protocol::commands::{ParagraphFormat, TextAlign};
use kiln_protocol::geom::Point;
use kiln_protocol::{CaretMotion, Command, SelectUnit, TextPos, WriteCommand};
use kiln_scene::DrawOp;

use super::WriteDoc;
use crate::model::DocModel;

/// The right-to-left paragraph "يستخدم الفريق Rust وTypeScript لبناء الواجهة، مع مكتبة parley
/// للتخطيط." of the fixture: "Rust" is at 14..18.
const PARA: usize = 2;
const RUST: usize = 14;

fn open() -> WriteDoc {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/rtl/mixed-runs.docx");
    let read = kiln_io_docx::read_docx(&std::fs::read(path).expect("fixture")).expect("reads");
    let mut d = WriteDoc::new(read.document, read.warnings, Some("mixed-runs.docx".into()));
    d.view_synced();
    d
}

fn w(d: &mut WriteDoc, c: WriteCommand) {
    d.execute(&Command::Write(c)).expect("runs");
    d.view_synced();
}

fn para_id(d: &WriteDoc) -> kiln_common::NodeId {
    d.body()[PARA].id
}

fn text(d: &WriteDoc) -> String {
    d.body()[PARA].content.plain_text(true)
}

fn caret_at(d: &mut WriteDoc, offset: usize) {
    let block = para_id(d);
    w(
        d,
        WriteCommand::SelectTo {
            pos: TextPos {
                block,
                offset: offset as u32,
            },
            extend: false,
        },
    );
}

fn mv(d: &mut WriteDoc, motion: CaretMotion, extend: bool) -> usize {
    w(d, WriteCommand::MoveCaret { motion, extend });
    d.head.offset
}

/// The caret bars the overlay draws (main caret first).
fn carets(d: &WriteDoc) -> Vec<kiln_scene::Bounds> {
    d.overlay()
        .iter()
        .filter_map(|op| match op {
            DrawOp::Caret { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect()
}

fn caret_x(d: &WriteDoc) -> f32 {
    let r = carets(d)[0];
    r.x + r.w / 2.0
}

#[test]
fn the_caret_moves_in_logical_order_and_shows_where_the_position_is() {
    let mut d = open();
    caret_at(&mut d, 0);
    let start = caret_x(&d);
    // Left goes on in the text of a right-to-left paragraph: through the Arabic word towards the
    // left of the screen.
    assert_eq!(mv(&mut d, CaretMotion::Left, false), 1);
    assert!(
        caret_x(&d) < start,
        "the first Arabic letter is at the right"
    );
    // Inside the left-to-right word the same key still goes on logically, and the caret now
    // moves to the right on the screen, letter by letter.
    caret_at(&mut d, RUST + 1);
    let a = caret_x(&d);
    assert_eq!(mv(&mut d, CaretMotion::Left, false), RUST + 2);
    let b = caret_x(&d);
    assert!(b > a, "{b} > {a}: Rust reads left to right");
    // Right goes back.
    assert_eq!(mv(&mut d, CaretMotion::Right, false), RUST + 1);
    assert!((caret_x(&d) - a).abs() < 0.5);
}

#[test]
fn a_direction_change_draws_both_carets_as_word_does() {
    let mut d = open();
    // Inside a run of one direction: one caret.
    caret_at(&mut d, 3);
    assert_eq!(carets(&d).len(), 1);
    // Between the space after the Arabic and "Rust": the position before R is at R's left edge
    // (the leading edge of a left-to-right letter); the position after the space at the space's
    // left edge, the right end of "Rust". Word draws the caret at the first and a short second
    // caret at the other.
    caret_at(&mut d, RUST);
    let c = carets(&d);
    assert_eq!(c.len(), 2, "{c:?}");
    let (main, second) = (c[0], c[1]);
    let rust = second.x - main.x;
    assert!(
        (15.0..60.0).contains(&rust),
        "the second caret is a word's width to the right: {rust}"
    );
    assert!(second.h < main.h, "the second caret is the short one");
    // The selection's caret is the main one.
    let sel = d.selection();
    let r = sel.caret.expect("caret").rect;
    assert!((r.x - main.x).abs() < 0.01);
}

#[test]
fn home_and_end_go_to_the_logical_ends_of_the_line() {
    let mut d = open();
    caret_at(&mut d, 20);
    assert_eq!(mv(&mut d, CaretMotion::LineStart, false), 0);
    let home = caret_x(&d);
    let len = text(&d).chars().count();
    assert_eq!(mv(&mut d, CaretMotion::LineEnd, false), len);
    let end = caret_x(&d);
    assert!(home > end + 200.0, "Home is at the right, End at the left");
    // Shift+Home from the end selects the whole line.
    assert_eq!(mv(&mut d, CaretMotion::LineStart, true), 0);
    assert_eq!((d.anchor.offset, d.head.offset), (len, 0));
}

#[test]
fn word_moves_follow_the_paragraph_direction() {
    let mut d = open();
    caret_at(&mut d, 0);
    // Ctrl+Left is the next word in a right-to-left paragraph.
    assert_eq!(mv(&mut d, CaretMotion::WordLeft, false), 7);
    assert_eq!(mv(&mut d, CaretMotion::WordLeft, false), RUST);
    assert_eq!(mv(&mut d, CaretMotion::WordLeft, false), RUST + 5);
    // Ctrl+Right goes back a word.
    assert_eq!(mv(&mut d, CaretMotion::WordRight, false), RUST);
    assert_eq!(mv(&mut d, CaretMotion::WordRight, false), 7);
}

#[test]
fn shift_selection_crosses_a_direction_change() {
    let mut d = open();
    caret_at(&mut d, RUST - 2);
    for _ in 0..4 {
        mv(&mut d, CaretMotion::Left, true);
    }
    assert_eq!((d.anchor.offset, d.head.offset), (RUST - 2, RUST + 2));
    assert_eq!(d.selected_text().as_deref(), Some("ق Ru"));
    // The highlight is in two pieces: "ق " at the Arabic side, "Ru" at the left end of "Rust".
    let rects = d.selection().rects;
    assert!(rects.len() >= 2, "{rects:?}");
    let mut xs: Vec<(f32, f32)> = rects.iter().map(|r| (r.x, r.x + r.width)).collect();
    xs.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(
        xs.windows(2).any(|p| p[1].0 - p[0].1 > 5.0),
        "the pieces are apart: {xs:?}"
    );
    // Shift+Right shrinks it back across the boundary.
    for _ in 0..3 {
        mv(&mut d, CaretMotion::Right, true);
    }
    assert_eq!(d.selected_text().as_deref(), Some("ق"));
}

#[test]
fn backspace_and_delete_at_a_direction_change_act_on_the_logical_neighbours() {
    let mut d = open();
    let before = text(&d);
    caret_at(&mut d, RUST);
    // Backspace deletes the character before the position in the text: the space.
    w(&mut d, WriteCommand::DeleteBackward);
    assert!(text(&d).contains("الفريقRust"), "{}", text(&d));
    assert_eq!(d.head.offset, RUST - 1);
    assert!(d.undo());
    d.view_synced();
    assert_eq!(text(&d), before);
    // Delete deletes the character after it: R.
    caret_at(&mut d, RUST);
    w(&mut d, WriteCommand::DeleteForward);
    assert!(text(&d).contains("الفريق ust"), "{}", text(&d));
    assert_eq!(d.head.offset, RUST);
    // Ctrl+Backspace at the boundary deletes the Arabic word before it.
    assert!(d.undo());
    d.view_synced();
    caret_at(&mut d, RUST);
    w(&mut d, WriteCommand::DeleteWordBackward);
    assert!(text(&d).starts_with("يستخدم Rust"), "{}", text(&d));
}

#[test]
fn a_click_on_right_to_left_text_puts_the_caret_by_the_nearer_edge() {
    let mut d = open();
    caret_at(&mut d, 0);
    let first = carets(&d)[0];
    caret_at(&mut d, 1);
    let second = carets(&d)[0];
    let y = first.y + first.h / 2.0;
    let click = |d: &mut WriteDoc, x: f32| {
        w(
            d,
            WriteCommand::SelectAt {
                point: Point::new(x, y),
                extend: false,
                unit: SelectUnit::Char,
            },
        );
        (d.head.para, d.head.offset)
    };
    let id = para_id(&d);
    // The first letter spans [second, first] (right to left): its right half is before it.
    let w1 = first.x - second.x;
    assert!(w1 > 2.0, "{w1}");
    assert_eq!(click(&mut d, first.x - w1 * 0.25), (id, 0));
    assert_eq!(click(&mut d, second.x + w1 * 0.25), (id, 1));
    // Right of the paragraph's text: its start.
    assert_eq!(click(&mut d, first.x + 30.0), (id, 0));
    // On "Rust": the left half of R is before it, the right half of t after it.
    caret_at(&mut d, RUST);
    let r = carets(&d)[0];
    caret_at(&mut d, RUST + 1);
    let r1 = carets(&d)[0];
    assert_eq!(click(&mut d, r.x + (r1.x - r.x) * 0.25), (id, RUST));
    // A drag selection from there to the start of the paragraph spans the logical range.
    w(
        &mut d,
        WriteCommand::SelectAt {
            point: Point::new(first.x - w1 * 0.25, y),
            extend: true,
            unit: SelectUnit::Char,
        },
    );
    assert_eq!((d.anchor.offset, d.head.offset), (RUST, 0));
    assert_eq!(d.selected_text().as_deref(), Some("يستخدم الفريق "));
}

#[test]
fn double_click_selects_an_arabic_word() {
    let mut d = open();
    caret_at(&mut d, 9);
    let c = carets(&d)[0];
    w(
        &mut d,
        WriteCommand::SelectAt {
            point: Point::new(c.x, c.y + c.h / 2.0),
            extend: false,
            unit: SelectUnit::Word,
        },
    );
    assert_eq!(d.selected_text().as_deref(), Some("الفريق"));
}

#[test]
fn the_alignment_buttons_name_the_sides_of_the_page() {
    let mut d = open();
    caret_at(&mut d, 3);
    let set = |d: &mut WriteDoc, a: TextAlign| {
        w(
            d,
            WriteCommand::SetParagraph {
                format: Box::new(ParagraphFormat {
                    alignment: Some(a),
                    ..ParagraphFormat::default()
                }),
            },
        );
        let id = para_id(d);
        d.doc.paragraph(id).expect("para").props.alignment
    };
    // Align Left on a right-to-left paragraph is its end, stored as Word stores it (`right`).
    assert_eq!(
        set(&mut d, TextAlign::Left),
        Some(kiln_doc::st::Alignment::Right)
    );
    let x_left = {
        caret_at(&mut d, 0);
        caret_x(&d)
    };
    assert_eq!(
        set(&mut d, TextAlign::Right),
        Some(kiln_doc::st::Alignment::Left)
    );
    caret_at(&mut d, 0);
    assert!(
        caret_x(&d) > x_left + 100.0,
        "Align Right moves the text to the right"
    );
    // The ribbon reads it back as the side it shows on.
    assert_eq!(
        d.format_state().expect("format").alignment,
        Some(TextAlign::Right)
    );
}

#[test]
fn the_direction_buttons_turn_a_paragraph_round() {
    let mut d = open();
    // "The word كيلن means kiln…" runs left to right and starts at the left margin.
    let block = d.body()[1].id;
    w(
        &mut d,
        WriteCommand::SelectTo {
            pos: TextPos { block, offset: 0 },
            extend: false,
        },
    );
    let ltr = caret_x(&d);
    assert_eq!(d.format_state().expect("format").rtl, Some(false));
    w(&mut d, WriteCommand::ParagraphRightToLeft);
    assert_eq!(d.doc.paragraph(block).expect("p").props.bidi, Some(true));
    assert_eq!(d.format_state().expect("format").rtl, Some(true));
    // Its start is now the right margin: the caret at offset 0 is there.
    assert!(caret_x(&d) > ltr + 300.0, "{} vs {ltr}", caret_x(&d));
    // The arrows follow: Left goes on in the text.
    assert_eq!(mv(&mut d, CaretMotion::Left, false), 1);
    w(&mut d, WriteCommand::ParagraphLeftToRight);
    assert_eq!(d.doc.paragraph(block).expect("p").props.bidi, Some(false));
    assert!(d.undo());
    d.view_synced();
    assert_eq!(d.doc.paragraph(block).expect("p").props.bidi, Some(true));
}
