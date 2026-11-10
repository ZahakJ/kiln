// SPDX-License-Identifier: AGPL-3.0-or-later
//! Right-to-left documents through the layout engine:
//!
//! - **Goldens** of `fixtures/docx/rtl/*.docx`: the first page at 96 dpi
//!   (`fixtures/golden/rtl/<name>-p1.png`, 0.3 % pixel tolerance, `KILN_UPDATE_GOLDEN=1` rewrites
//!   them) and the words of every line in reading order (`fixtures/golden/rtl/snapshots`).
//!   They were checked against LibreOffice's rendering of the same files (README.md there).
//! - **Geometry**: right-to-left paragraphs start at the right margin, `jc` left and right are the
//!   start and the end of the line, list labels sit at the right, `bidiVisual` tables put their
//!   first column at the right; carets at a direction change come in pairs; hit testing on
//!   right-to-left text.

mod common;

use std::path::{Path, PathBuf};

use common::*;
use kiln_doc::Pos;
use kiln_doc_layout::geometry::{caret_in, hit_test};
use kiln_doc_layout::snapshot::{page_line_words, summary};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const FIXTURES: [&str; 5] = [
    "arabic-digits",
    "arabic-justified",
    "bidi-table",
    "mixed-runs",
    "rtl-lists",
];

fn open(name: &str) -> kiln_doc::Document {
    let bytes =
        std::fs::read(repo().join(format!("fixtures/docx/rtl/{name}.docx"))).expect("fixture");
    kiln_io_docx::read_docx(&bytes).expect("reads").document
}

#[test]
fn rtl_fixtures_render_like_their_goldens() {
    let mut failures = Vec::new();
    for name in FIXTURES {
        let doc = open(name);
        let mut l = layout(&doc);
        let img = render_page(&mut l, &doc, 0, 96.0, false);
        let golden = repo().join(format!("fixtures/golden/rtl/{name}-p1.png"));
        if let Err(e) = kiln_raster::golden::check(&img, &golden) {
            failures.push(e);
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn rtl_fixtures_keep_their_lines() {
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../../fixtures/golden/rtl/snapshots");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        for name in FIXTURES {
            let doc = open(name);
            let l = layout(&doc);
            let mut out = summary(&l);
            for (i, p) in l.pages().iter().enumerate() {
                out.push_str(&format!("page {}\n", i + 1));
                for words in page_line_words(p) {
                    out.push_str("  ");
                    out.push_str(&words.join(" "));
                    out.push('\n');
                }
            }
            insta::assert_snapshot!(name, out);
        }
    });
}

/// Ids of the body paragraphs.
fn paras(doc: &kiln_doc::Document) -> Vec<kiln_common::NodeId> {
    doc.body_paragraphs().iter().map(|p| p.id).collect()
}

/// The left edge of the page's text area (A4, 1-inch margins) and its right edge, document px.
fn margins(l: &kiln_doc_layout::DocLayout) -> (f32, f32) {
    let page = l.page_rect(0).expect("page");
    (page.x + 96.0, page.x + page.w - 96.0)
}

#[test]
fn right_to_left_paragraphs_start_at_the_right_margin() {
    let doc = open("arabic-justified");
    let l = layout(&doc);
    let ids = paras(&doc);
    let (left, right) = margins(&l);
    // No jc and jc="both" start at the right: the caret at offset 0 is at the right margin.
    for &i in &[0usize, 1] {
        let c = caret_in(&l, Pos::new(ids[i], 0)).expect("caret");
        let x = c.rect.x + c.rect.w / 2.0;
        assert!((x - right).abs() < 2.0, "paragraph {i}: {x} vs {right}");
        assert!(c.rtl);
    }
    // jc="right" in a bidi paragraph is its end: the left margin. The text ends there.
    let last = *ids.last().expect("paras");
    let n = doc
        .paragraph(last)
        .expect("p")
        .content
        .plain_text(true)
        .chars()
        .count();
    let end = caret_in(&l, Pos::new(last, n)).expect("caret");
    assert!(
        (end.rect.x + end.rect.w / 2.0 - left).abs() < 2.0,
        "{:?}",
        end.rect
    );
    // The centred one is centred.
    let mid = ids[ids.len() - 2];
    let n = doc
        .paragraph(mid)
        .expect("p")
        .content
        .plain_text(true)
        .chars()
        .count();
    let a = caret_in(&l, Pos::new(mid, 0)).expect("caret").rect.x;
    let b = caret_in(&l, Pos::new(mid, n)).expect("caret").rect.x;
    assert!(((a + b) / 2.0 - (left + right) / 2.0).abs() < 2.0);
}

#[test]
fn list_labels_of_right_to_left_items_sit_at_the_right() {
    let doc = open("rtl-lists");
    let l = layout(&doc);
    let ids = paras(&doc);
    let (_, right) = margins(&l);
    // The first item: left="720" hanging="360" from the right margin, so the text starts 48 px
    // in from it and the label is in the 24 px before.
    let c = caret_in(&l, Pos::new(ids[1], 0)).expect("caret");
    let x = c.rect.x + c.rect.w / 2.0;
    assert!((right - 48.0 - x).abs() < 2.0, "{x} vs {}", right - 48.0);
}

#[test]
fn a_bidi_table_puts_its_first_column_at_the_right() {
    let doc = open("bidi-table");
    let l = layout(&doc);
    let (left, right) = margins(&l);
    let table = doc.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            kiln_doc::Block::Table(t) => Some(t),
            _ => None,
        })
        .expect("table");
    let first_cell = |r: usize, c: usize| -> kiln_common::NodeId {
        match &table.rows[r].cells[c].blocks[0] {
            kiln_doc::Block::Paragraph(p) => p.id,
            _ => panic!("paragraph"),
        }
    };
    // "المنتج" (first column) at the right, "المجموع" (last) at the left.
    let product = caret_in(&l, Pos::new(first_cell(0, 0), 0))
        .expect("caret")
        .rect
        .x;
    let total = caret_in(&l, Pos::new(first_cell(0, 3), 0))
        .expect("caret")
        .rect
        .x;
    assert!(product > total + 300.0, "{product} {total}");
    assert!(
        right - product < 12.0,
        "the first column's text starts at the right edge"
    );
    // A number cell with jc="right" (the end) ends at its left side.
    let n = first_cell(1, 3);
    let len = doc
        .paragraph(n)
        .expect("p")
        .content
        .plain_text(true)
        .chars()
        .count();
    let e = caret_in(&l, Pos::new(n, 0)).expect("caret").rect.x;
    let s = caret_in(&l, Pos::new(n, len)).expect("caret").rect.x;
    // The table (8,900 twips) hangs from the right margin.
    let table_left = right - 8_900.0 / 15.0;
    assert!(table_left > left);
    assert!((e.min(s) - table_left).abs() < 14.0, "{e} {s} {table_left}");
}

#[test]
fn carets_at_a_direction_change_come_in_pairs() {
    let doc = open("mixed-runs");
    let l = layout(&doc);
    let ids = paras(&doc);
    // Left-to-right paragraph "The word كيلن means …": at 9 the Arabic word starts. Its first
    // letter's leading edge is its right side; the second caret is where "word " ends.
    let p = ids[1];
    let at = caret_in(&l, Pos::new(p, 9)).expect("caret");
    let second = at.secondary.expect("two carets at the boundary");
    assert!(at.rect.x > second.x + 10.0, "{:?} {second:?}", at.rect);
    assert!(at.rtl);
    // Inside the Arabic word: one caret.
    assert!(
        caret_in(&l, Pos::new(p, 10))
            .expect("caret")
            .secondary
            .is_none()
    );
    // At the start of a paragraph and at its end: one caret.
    assert!(
        caret_in(&l, Pos::new(p, 0))
            .expect("caret")
            .secondary
            .is_none()
    );
    // After the Arabic word (13), before " means": the second caret is at the word's left.
    let after = caret_in(&l, Pos::new(p, 13)).expect("caret");
    let s = after.secondary.expect("two carets");
    assert!(
        (s.x - second.x).abs() < 0.5,
        "both boundaries share the word's left edge"
    );
}

#[test]
fn a_click_on_right_to_left_text_hits_the_logical_position() {
    let doc = open("arabic-digits");
    let l = layout(&doc);
    let ids = paras(&doc);
    let p = ids[1];
    let c0 = caret_in(&l, Pos::new(p, 0)).expect("caret").rect;
    let c1 = caret_in(&l, Pos::new(p, 1)).expect("caret").rect;
    let y = c0.y + c0.h / 2.0;
    assert!(c1.x < c0.x, "right to left");
    let hit = |x: f32| hit_test(&l, x, y, None).expect("hit").pos;
    assert_eq!(hit(c0.x - 0.5), Pos::new(p, 0));
    assert_eq!(hit(c1.x + 0.5), Pos::new(p, 1));
    // Every caret position of the line maps back to itself.
    let n = doc
        .paragraph(p)
        .expect("p")
        .content
        .plain_text(true)
        .chars()
        .count();
    let line_end = (0..=n)
        .take_while(|&i| {
            caret_in(&l, Pos::new(p, i)).is_some_and(|c| (c.rect.y - c0.y).abs() < 1.0)
        })
        .last()
        .expect("line");
    for i in 0..line_end {
        let c = caret_in(&l, Pos::new(p, i)).expect("caret");
        if c.secondary.is_some() {
            continue;
        }
        let got = hit(c.rect.x + c.rect.w / 2.0);
        let back = caret_in(&l, got).expect("caret").rect.x;
        assert!((back - c.rect.x).abs() < 0.5, "offset {i}: hit {got:?}");
    }
}
