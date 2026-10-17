// SPDX-License-Identifier: AGPL-3.0-or-later
//! Grid commands, one behaviour per test: navigation, selection, editing, autocomplete, fill,
//! rows and columns, sheets, formats, clipboard, view, undo, queries and frames.

use kiln_protocol::commands::{
    CellShift, ClipboardData, CompletionKind, EditCommand, GridBorders, GridClear,
    GridCommand as G, GridDirection as D, GridFill, GridFormat, GridHAlign, GridHit, GridLineStyle,
    GridMode, GridPaste, GridPasteOp, GridQuery, GridQueryResult, GridRange, GridState, PasteMode,
};
use kiln_protocol::geom::Point;
use kiln_protocol::{Command, Query, QueryResult, SuiteCommand, ViewMode, Viewport};
use kiln_sheet::styles::{BorderStyle, HAlign, Underline, VAlign};
use kiln_sheet::{CellPos, CellValue, MAX_COLS, MAX_ROWS, Range, Workbook};

use super::GridDoc;
use crate::frame::FrameState;
use crate::model::{Change, DocModel};

fn doc() -> GridDoc {
    let mut d = GridDoc::new(Workbook::new(), None, Vec::new(), None);
    d.last_view = kiln_sheet_layout::View {
        width: 1200.0,
        height: 700.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
    };
    d
}

fn run(d: &mut GridDoc, c: G) -> Change {
    d.execute(&Command::Grid(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"))
}

fn fails(d: &mut GridDoc, c: G) -> String {
    d.execute(&Command::Grid(c))
        .expect_err("should fail")
        .to_string()
}

fn ed(d: &mut GridDoc, e: EditCommand) -> Change {
    d.execute(&Command::Suite(SuiteCommand::Edit(e.clone())))
        .unwrap_or_else(|err| panic!("{e:?}: {err}"))
}

fn at(d: &mut GridDoc, row: u32, col: u32) {
    run(
        d,
        G::SelectCell {
            row,
            col,
            extend: false,
            add: false,
        },
    );
}

/// Types into a cell and presses Enter.
fn put(d: &mut GridDoc, row: u32, col: u32, text: &str) {
    at(d, row, col);
    run(d, G::BeginEdit { text: text.into() });
    run(d, G::CommitEdit { advance: None });
}

fn sid(d: &GridDoc) -> kiln_sheet::SheetId {
    d.sid().expect("sheet")
}

fn val(d: &GridDoc, row: u32, col: u32) -> CellValue {
    d.workbook.value(sid(d), row, col)
}

fn num(d: &GridDoc, row: u32, col: u32) -> f64 {
    val(d, row, col)
        .as_number()
        .unwrap_or_else(|| panic!("no number at {row},{col}: {:?}", val(d, row, col)))
}

fn text(d: &GridDoc, row: u32, col: u32) -> String {
    d.workbook.display_text(sid(d), row, col)
}

fn active(d: &GridDoc) -> (u32, u32) {
    let a = d.view().sel.active;
    (a.row, a.col)
}

fn ranges(d: &GridDoc) -> Vec<Range> {
    d.view().sel.ranges
}

fn state(d: &GridDoc) -> GridState {
    match d.app_query(&Query::Grid(GridQuery::State)).expect("state") {
        QueryResult::Grid(GridQueryResult::State(s)) => *s,
        other => panic!("{other:?}"),
    }
}

fn style(d: &GridDoc, row: u32, col: u32) -> kiln_sheet::styles::CellStyle {
    let s = sid(d);
    let idx = d.workbook.sheet(s).map_or(0, |sh| sh.style_at(row, col));
    d.workbook.styles().get(idx)
}

/// A 3 × 4 block of numbers at A1:C4 with a gap at row 6 and more data at A7.
fn data(d: &mut GridDoc) {
    for r in 0..4 {
        for c in 0..3 {
            put(d, r, c, &format!("{}", r * 10 + c));
        }
    }
    put(d, 6, 0, "far");
    at(d, 0, 0);
}

// ── navigation ────────────────────────────────────────────────────────────────────────────

#[test]
fn arrows_move_the_active_cell() {
    let mut d = doc();
    run(
        &mut d,
        G::Move {
            dir: D::Down,
            extend: false,
        },
    );
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(active(&d), (1, 1));
    run(
        &mut d,
        G::Move {
            dir: D::Up,
            extend: false,
        },
    );
    run(
        &mut d,
        G::Move {
            dir: D::Left,
            extend: false,
        },
    );
    run(
        &mut d,
        G::Move {
            dir: D::Left,
            extend: false,
        },
    );
    assert_eq!(active(&d), (0, 0), "stops at the edge");
}

#[test]
fn shift_arrows_extend_from_the_moving_corner() {
    let mut d = doc();
    at(&mut d, 2, 2);
    run(
        &mut d,
        G::Move {
            dir: D::Down,
            extend: true,
        },
    );
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: true,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(2, 2, 3, 3)]);
    assert_eq!(active(&d), (2, 2), "the active cell stays");
    run(
        &mut d,
        G::Move {
            dir: D::Up,
            extend: true,
        },
    );
    run(
        &mut d,
        G::Move {
            dir: D::Up,
            extend: true,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(1, 2, 2, 3)]);
}

#[test]
fn ctrl_arrows_jump_to_the_edges_of_data() {
    let mut d = doc();
    data(&mut d);
    run(
        &mut d,
        G::Jump {
            dir: D::Down,
            extend: false,
        },
    );
    assert_eq!(active(&d), (3, 0), "end of the run");
    run(
        &mut d,
        G::Jump {
            dir: D::Down,
            extend: false,
        },
    );
    assert_eq!(active(&d), (6, 0), "across the gap");
    run(
        &mut d,
        G::Jump {
            dir: D::Down,
            extend: false,
        },
    );
    assert_eq!(active(&d), (MAX_ROWS - 1, 0), "to the last row");
    run(
        &mut d,
        G::Jump {
            dir: D::Up,
            extend: false,
        },
    );
    assert_eq!(active(&d), (6, 0));
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::Jump {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(active(&d), (0, 2));
    run(
        &mut d,
        G::Jump {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(active(&d), (0, MAX_COLS - 1));
}

#[test]
fn ctrl_shift_arrows_extend_to_the_edge() {
    let mut d = doc();
    data(&mut d);
    run(
        &mut d,
        G::Jump {
            dir: D::Down,
            extend: true,
        },
    );
    run(
        &mut d,
        G::Jump {
            dir: D::Right,
            extend: true,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(0, 0, 3, 2)]);
}

#[test]
fn home_and_ctrl_home_and_end() {
    let mut d = doc();
    data(&mut d);
    at(&mut d, 2, 2);
    run(&mut d, G::RowStart { extend: false });
    assert_eq!(active(&d), (2, 0));
    run(&mut d, G::LastCell { extend: false });
    assert_eq!(active(&d), (6, 2), "the last used row and column");
    run(&mut d, G::FirstCell { extend: false });
    assert_eq!(active(&d), (0, 0));
}

#[test]
fn ctrl_home_stops_below_frozen_panes() {
    let mut d = doc();
    at(&mut d, 2, 1);
    run(&mut d, G::FreezePanes);
    at(&mut d, 30, 30);
    run(&mut d, G::FirstCell { extend: false });
    assert_eq!(active(&d), (2, 1));
    run(&mut d, G::RowStart { extend: false });
    assert_eq!(active(&d), (2, 1));
}

#[test]
fn page_down_moves_by_a_screen_and_scrolls() {
    let mut d = doc();
    run(
        &mut d,
        G::Page {
            dir: D::Down,
            extend: false,
        },
    );
    let (row, _) = active(&d);
    assert!(row >= 25, "moved {row} rows");
    let s = state(&d);
    let reveal = s.reveal.expect("scroll request");
    assert!(reveal.y > 400.0);
    run(
        &mut d,
        G::Page {
            dir: D::Up,
            extend: false,
        },
    );
    assert_eq!(active(&d), (0, 0));
    run(
        &mut d,
        G::Page {
            dir: D::Right,
            extend: true,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(0, 0, 0, 10)]);
}

#[test]
fn moving_far_requests_a_scroll() {
    let mut d = doc();
    at(&mut d, 500, 40);
    let r = state(&d).reveal.expect("reveal");
    assert!(r.y > 9000.0 && r.x > 1000.0, "{r:?}");
    let seq = r.seq;
    at(&mut d, 499, 40);
    assert_eq!(
        state(&d).reveal.map(|r| r.seq),
        Some(seq),
        "a visible cell needs no scroll"
    );
}

#[test]
fn enter_and_tab_cycle_inside_a_selection() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 1,
                c1: 1,
            },
            add: false,
        },
    );
    run(
        &mut d,
        G::Advance {
            backward: false,
            across: false,
        },
    );
    assert_eq!(active(&d), (1, 0));
    run(
        &mut d,
        G::Advance {
            backward: false,
            across: false,
        },
    );
    assert_eq!(active(&d), (0, 1), "next column at the bottom");
    run(
        &mut d,
        G::Advance {
            backward: false,
            across: true,
        },
    );
    assert_eq!(active(&d), (1, 0), "Tab goes along the row, then wraps");
    run(
        &mut d,
        G::Advance {
            backward: true,
            across: true,
        },
    );
    assert_eq!(active(&d), (0, 1));
    assert_eq!(
        ranges(&d),
        vec![Range::new(0, 0, 1, 1)],
        "the selection stays"
    );
}

#[test]
fn enter_moves_down_without_a_selection() {
    let mut d = doc();
    run(
        &mut d,
        G::Advance {
            backward: false,
            across: false,
        },
    );
    assert_eq!(active(&d), (1, 0));
    run(
        &mut d,
        G::Advance {
            backward: false,
            across: true,
        },
    );
    assert_eq!(active(&d), (1, 1));
}

#[test]
fn clicks_select_extend_and_add() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectCell {
            row: 1,
            col: 1,
            extend: false,
            add: false,
        },
    );
    run(
        &mut d,
        G::SelectCell {
            row: 3,
            col: 2,
            extend: true,
            add: false,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(1, 1, 3, 2)]);
    run(
        &mut d,
        G::SelectCell {
            row: 7,
            col: 7,
            extend: false,
            add: true,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(1, 1, 3, 2), Range::cell(7, 7)]);
    assert_eq!(active(&d), (7, 7));
    assert!(state(&d).multi);
}

#[test]
fn whole_rows_columns_and_the_sheet() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectRows {
            from: 2,
            to: 4,
            extend: false,
            add: false,
        },
    );
    assert!(ranges(&d)[0].is_whole_rows());
    assert!(state(&d).whole_rows);
    run(
        &mut d,
        G::SelectColumns {
            from: 1,
            to: 1,
            extend: false,
            add: false,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(0, 1, MAX_ROWS - 1, 1)]);
    assert!(state(&d).whole_columns);
    run(
        &mut d,
        G::SelectColumns {
            from: 1,
            to: 3,
            extend: true,
            add: false,
        },
    );
    assert_eq!(ranges(&d)[0].c1, 3);
    at(&mut d, 5, 5);
    run(&mut d, G::SelectRow);
    assert_eq!(ranges(&d), vec![Range::new(5, 0, 5, MAX_COLS - 1)]);
    at(&mut d, 5, 5);
    run(&mut d, G::SelectColumn);
    assert_eq!(ranges(&d), vec![Range::new(0, 5, MAX_ROWS - 1, 5)]);
}

#[test]
fn ctrl_a_selects_the_region_then_the_sheet() {
    let mut d = doc();
    data(&mut d);
    at(&mut d, 1, 1);
    run(&mut d, G::SelectAll);
    assert_eq!(ranges(&d), vec![Range::new(0, 0, 3, 2)]);
    run(&mut d, G::SelectAll);
    assert_eq!(
        ranges(&d),
        vec![Range::new(0, 0, MAX_ROWS - 1, MAX_COLS - 1)]
    );
    at(&mut d, 20, 20);
    run(&mut d, G::SelectAll);
    assert_eq!(
        ranges(&d)[0],
        Range::new(0, 0, MAX_ROWS - 1, MAX_COLS - 1),
        "outside data: the sheet"
    );
}

#[test]
fn go_to_references_names_and_sheets() {
    let mut d = doc();
    run(
        &mut d,
        G::GoTo {
            reference: "C7".into(),
            define: false,
        },
    );
    assert_eq!(active(&d), (6, 2));
    run(
        &mut d,
        G::GoTo {
            reference: "$B$2:D4".into(),
            define: false,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(1, 1, 3, 3)]);
    run(&mut d, G::InsertSheet);
    let second = d.workbook.sheets()[1].name().to_owned();
    run(&mut d, G::ActivateSheet { index: 0 });
    run(
        &mut d,
        G::GoTo {
            reference: format!("{second}!E5"),
            define: false,
        },
    );
    assert_eq!(d.active_index(), 1);
    assert_eq!(active(&d), (4, 4));
    d.workbook
        .apply(&kiln_sheet::Command::DefineName {
            name: "Total".into(),
            formula: "Sheet1!$A$3".into(),
            scope: None,
            hidden: false,
            comment: None,
        })
        .expect("name");
    run(
        &mut d,
        G::GoTo {
            reference: "Total".into(),
            define: false,
        },
    );
    assert_eq!((d.active_index(), active(&d)), (0, (2, 0)));
    assert!(
        fails(
            &mut d,
            G::GoTo {
                reference: "Nowhere".into(),
                define: false
            }
        )
        .contains("Nowhere")
    );
}

#[test]
fn the_name_box_shows_a_defined_name() {
    let mut d = doc();
    d.workbook
        .apply(&kiln_sheet::Command::DefineName {
            name: "Prices".into(),
            formula: "Sheet1!$B$2:$B$5".into(),
            scope: None,
            hidden: false,
            comment: None,
        })
        .expect("name");
    run(
        &mut d,
        G::GoTo {
            reference: "B2:B5".into(),
            define: false,
        },
    );
    assert_eq!(state(&d).name_box, "Prices");
    at(&mut d, 9, 27);
    assert_eq!(state(&d).name_box, "AB10");
    // Typing a new name in the name box names the selection; Go To does not define names.
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 1,
                c0: 2,
                r1: 3,
                c1: 3,
            },
            add: false,
        },
    );
    assert!(
        fails(
            &mut d,
            G::GoTo {
                reference: "Totals".into(),
                define: false,
            }
        )
        .contains("not a cell reference")
    );
    run(
        &mut d,
        G::GoTo {
            reference: "Totals".into(),
            define: true,
        },
    );
    assert_eq!(state(&d).name_box, "Totals");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::GoTo {
            reference: "totals".into(),
            define: true,
        },
    );
    assert_eq!(
        active(&d),
        (1, 2),
        "an existing name is gone to, not redefined"
    );
    assert!(
        fails(
            &mut d,
            G::GoTo {
                reference: "1bad name".into(),
                define: true,
            }
        )
        .contains("cannot be a name")
    );
    // Defining a name is one undo step.
    assert_eq!(d.undo_label(), Some("grid.nav.go_to"));
    assert!(d.undo());
    assert!(
        fails(
            &mut d,
            G::GoTo {
                reference: "Totals".into(),
                define: false,
            }
        )
        .contains("not a cell reference")
    );
}

#[test]
fn find_next_wraps_and_reports_misses() {
    let mut d = doc();
    put(&mut d, 0, 0, "Kiln");
    put(&mut d, 5, 3, "kiln grid");
    put(&mut d, 9, 1, "=1+1");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::FindNext {
            text: "KILN".into(),
            match_case: false,
            whole_cell: false,
            backward: false,
            in_formulas: false,
        },
    );
    assert_eq!(active(&d), (5, 3));
    run(
        &mut d,
        G::FindNext {
            text: "kiln".into(),
            match_case: false,
            whole_cell: false,
            backward: false,
            in_formulas: false,
        },
    );
    assert_eq!(active(&d), (0, 0), "wraps");
    run(
        &mut d,
        G::FindNext {
            text: "kiln grid".into(),
            match_case: true,
            whole_cell: true,
            backward: false,
            in_formulas: false,
        },
    );
    assert_eq!(active(&d), (5, 3), "whole cell, matching case");
    assert!(
        fails(
            &mut d,
            G::FindNext {
                text: "KILN".into(),
                match_case: true,
                whole_cell: true,
                backward: false,
                in_formulas: false
            }
        )
        .contains("KILN")
    );
    run(
        &mut d,
        G::FindNext {
            text: "1+1".into(),
            match_case: false,
            whole_cell: false,
            backward: false,
            in_formulas: true,
        },
    );
    assert_eq!(active(&d), (9, 1));
    assert!(
        fails(
            &mut d,
            G::FindNext {
                text: "zzz".into(),
                match_case: false,
                whole_cell: false,
                backward: false,
                in_formulas: false
            }
        )
        .contains("zzz")
    );
}

#[test]
fn merged_cells_select_and_move_as_one() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 1,
                c0: 1,
                r1: 2,
                c1: 3,
            },
            add: false,
        },
    );
    run(&mut d, G::MergeCells);
    at(&mut d, 2, 2);
    assert_eq!(active(&d), (1, 1), "a click inside selects the merge");
    assert_eq!(ranges(&d), vec![Range::new(1, 1, 2, 3)]);
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(active(&d), (1, 4), "leaves by the far edge");
    run(
        &mut d,
        G::Move {
            dir: D::Left,
            extend: false,
        },
    );
    assert_eq!(active(&d), (1, 1));
    assert!(!state(&d).multi, "a merge is one cell");
}

// ── sheets ────────────────────────────────────────────────────────────────────────────────

#[test]
fn sheets_insert_switch_rename_and_delete() {
    let mut d = doc();
    run(&mut d, G::InsertSheet);
    assert_eq!(d.workbook.sheets().len(), 2);
    assert_eq!(d.active_index(), 1);
    run(&mut d, G::PreviousSheet);
    assert_eq!(d.active_index(), 0);
    run(&mut d, G::PreviousSheet);
    assert_eq!(d.active_index(), 0, "stops at the first");
    run(&mut d, G::NextSheet);
    assert_eq!(d.active_index(), 1);
    run(
        &mut d,
        G::RenameSheet {
            index: 1,
            name: "Budget".into(),
        },
    );
    assert_eq!(state(&d).sheets[1].name, "Budget");
    assert!(
        fails(
            &mut d,
            G::RenameSheet {
                index: 0,
                name: "Budget".into()
            }
        )
        .contains("Budget")
    );
    assert!(
        fails(
            &mut d,
            G::RenameSheet {
                index: 0,
                name: "a/b".into()
            }
        )
        .len()
            > 3
    );
    run(&mut d, G::DeleteSheet { index: 1 });
    assert_eq!(d.workbook.sheets().len(), 1);
    assert_eq!(d.active_index(), 0);
    assert!(fails(&mut d, G::DeleteSheet { index: 0 }).contains("visible"));
}

#[test]
fn sheets_move_colour_hide_and_unhide() {
    let mut d = doc();
    run(&mut d, G::InsertSheet);
    run(&mut d, G::InsertSheet);
    let names: Vec<String> = state(&d).sheets.iter().map(|s| s.name.clone()).collect();
    run(&mut d, G::MoveSheet { from: 0, to: 2 });
    let after: Vec<String> = state(&d).sheets.iter().map(|s| s.name.clone()).collect();
    assert_eq!(
        after,
        vec![names[1].clone(), names[2].clone(), names[0].clone()]
    );
    run(
        &mut d,
        G::SheetTabColor {
            index: 0,
            color: Some("E05A27".into()),
        },
    );
    assert_eq!(state(&d).sheets[0].color.as_deref(), Some("E05A27"));
    run(
        &mut d,
        G::SheetTabColor {
            index: 0,
            color: None,
        },
    );
    assert_eq!(state(&d).sheets[0].color, None);
    run(&mut d, G::ActivateSheet { index: 1 });
    run(&mut d, G::HideSheet { index: 1 });
    assert!(state(&d).sheets[1].hidden);
    assert_ne!(d.active_index(), 1, "a hidden sheet is not shown");
    run(&mut d, G::NextSheet);
    run(&mut d, G::NextSheet);
    assert_ne!(d.active_index(), 1, "Ctrl+Page Down skips it");
    run(&mut d, G::UnhideSheet { index: 1 });
    assert!(!state(&d).sheets[1].hidden);
    assert_eq!(d.active_index(), 1);
}

#[test]
fn duplicating_a_sheet_copies_its_cells_and_layout() {
    let mut d = doc();
    put(&mut d, 0, 0, "5");
    put(&mut d, 1, 0, "=A1*2");
    run(
        &mut d,
        G::ResizeColumns {
            from: 0,
            to: 0,
            px: 120.0,
        },
    );
    run(&mut d, G::DuplicateSheet { index: 0 });
    assert_eq!(d.workbook.sheets().len(), 2);
    assert_eq!(d.active_index(), 1);
    assert!(state(&d).sheets[1].name.starts_with("Sheet1 ("));
    assert_eq!(num(&d, 1, 0), 10.0);
    put(&mut d, 0, 0, "7");
    assert_eq!(num(&d, 1, 0), 14.0, "formulas refer to the copy");
    assert!(d.geometry().cols.size(0) > 100.0);
}

#[test]
fn each_sheet_keeps_its_own_selection() {
    let mut d = doc();
    at(&mut d, 4, 4);
    run(&mut d, G::InsertSheet);
    at(&mut d, 1, 1);
    run(&mut d, G::ActivateSheet { index: 0 });
    assert_eq!(active(&d), (4, 4));
    run(&mut d, G::ActivateSheet { index: 1 });
    assert_eq!(active(&d), (1, 1));
    assert!(fails(&mut d, G::ActivateSheet { index: 9 }).contains('9'));
}

// ── editing ───────────────────────────────────────────────────────────────────────────────

#[test]
fn typing_starts_an_entry_and_enter_commits_and_moves() {
    let mut d = doc();
    run(&mut d, G::BeginEdit { text: "4".into() });
    assert_eq!(state(&d).mode, GridMode::Enter);
    run(
        &mut d,
        G::SetEditText {
            text: "42".into(),
            start: 2,
            end: 2,
            formula_bar: false,
        },
    );
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Down),
        },
    );
    assert_eq!(num(&d, 0, 0), 42.0);
    assert_eq!(active(&d), (1, 0));
    assert_eq!(state(&d).mode, GridMode::Ready);
}

#[test]
fn tab_and_shift_enter_commit_in_their_directions() {
    let mut d = doc();
    at(&mut d, 3, 3);
    run(&mut d, G::BeginEdit { text: "a".into() });
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Right),
        },
    );
    assert_eq!(active(&d), (3, 4));
    run(&mut d, G::BeginEdit { text: "b".into() });
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Up),
        },
    );
    assert_eq!(active(&d), (2, 4));
    assert_eq!(text(&d, 3, 4), "b");
}

#[test]
fn escape_cancels_the_entry() {
    let mut d = doc();
    put(&mut d, 0, 0, "keep");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::BeginEdit {
            text: "lose".into(),
        },
    );
    run(&mut d, G::CancelEdit);
    assert_eq!(text(&d, 0, 0), "keep");
    assert!(d.edit.is_none());
}

#[test]
fn f2_edits_in_place_with_the_formula() {
    let mut d = doc();
    put(&mut d, 0, 0, "=1+2");
    at(&mut d, 0, 0);
    run(&mut d, G::EditCell);
    let s = state(&d);
    assert_eq!(s.mode, GridMode::Edit);
    let e = s.editor.expect("editor");
    assert_eq!(e.text, "=1+2");
    assert_eq!((e.start, e.end), (4, 4), "caret at the end");
    run(&mut d, G::EditCell);
    assert_eq!(state(&d).mode, GridMode::Enter, "F2 toggles the mode");
}

#[test]
fn the_formula_bar_shows_values_as_typed() {
    let mut d = doc();
    put(&mut d, 0, 0, "1/15/2026");
    put(&mut d, 1, 0, "25%");
    put(&mut d, 2, 0, "'0042");
    put(&mut d, 3, 0, "TRUE");
    at(&mut d, 0, 0);
    assert_eq!(state(&d).formula, "1/15/2026");
    at(&mut d, 1, 0);
    assert_eq!(state(&d).formula, "25%");
    at(&mut d, 2, 0);
    assert_eq!(state(&d).formula, "'0042");
    assert_eq!(text(&d, 2, 0), "0042");
    at(&mut d, 3, 0);
    assert_eq!(state(&d).formula, "TRUE");
}

#[test]
fn formulas_calculate_and_close_their_parentheses() {
    let mut d = doc();
    put(&mut d, 0, 0, "2");
    put(&mut d, 1, 0, "3");
    put(&mut d, 2, 0, "=SUM(A1:A2");
    assert_eq!(num(&d, 2, 0), 5.0);
    at(&mut d, 2, 0);
    assert_eq!(state(&d).formula, "=SUM(A1:A2)");
    put(&mut d, 0, 0, "10");
    assert_eq!(num(&d, 2, 0), 13.0, "recalculated");
}

#[test]
fn the_formula_bar_edits_the_cell() {
    let mut d = doc();
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::SetEditText {
            text: "=6*7".into(),
            start: 4,
            end: 4,
            formula_bar: true,
        },
    );
    let s = state(&d);
    assert_eq!(s.mode, GridMode::Edit);
    assert!(s.editor.as_ref().is_some_and(|e| e.formula_bar));
    run(&mut d, G::CommitEdit { advance: None });
    assert_eq!(num(&d, 1, 1), 42.0);
    assert_eq!(active(&d), (1, 1), "the ✓ button stays");
}

#[test]
fn alt_enter_breaks_the_line_and_wraps() {
    let mut d = doc();
    run(&mut d, G::BeginEdit { text: "one".into() });
    run(&mut d, G::NewLineInCell);
    run(
        &mut d,
        G::SetEditText {
            text: "one\ntwo".into(),
            start: 7,
            end: 7,
            formula_bar: false,
        },
    );
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Down),
        },
    );
    assert_eq!(text(&d, 0, 0), "one\ntwo");
    assert!(
        style(&d, 0, 0).alignment.wrap_text,
        "Excel turns Wrap Text on"
    );
    let h = d
        .workbook
        .sheet(sid(&d))
        .map(|s| s.rows().size_px(0))
        .unwrap_or(0.0);
    assert!(h > 30.0, "the row grew to {h}");
}

#[test]
fn f4_cycles_absolute_references() {
    let mut d = doc();
    run(&mut d, G::BeginEdit { text: "=A1".into() });
    run(&mut d, G::ToggleAbsolute);
    assert_eq!(
        d.edit.as_ref().map(|e| e.text.clone()),
        Some("=$A$1".into())
    );
    run(&mut d, G::ToggleAbsolute);
    assert_eq!(d.edit.as_ref().map(|e| e.text.clone()), Some("=A$1".into()));
    run(&mut d, G::ToggleAbsolute);
    run(&mut d, G::ToggleAbsolute);
    assert_eq!(d.edit.as_ref().map(|e| e.text.clone()), Some("=A1".into()));
}

#[test]
fn function_names_autocomplete_with_their_docs() {
    let mut d = doc();
    run(
        &mut d,
        G::BeginEdit {
            text: "=VLO".into(),
        },
    );
    let e = state(&d).editor.expect("editor");
    let first = e.completions.first().expect("suggestions");
    assert_eq!(first.label, "VLOOKUP");
    assert_eq!(first.kind, CompletionKind::Function);
    assert!(
        first.detail.as_deref().is_some_and(|s| !s.is_empty()),
        "inline docs"
    );
    run(&mut d, G::AcceptCompletion { index: 0 });
    let e = state(&d).editor.expect("editor");
    assert_eq!(e.text, "=VLOOKUP(");
    assert_eq!(e.end, 9);
    let hint = e.hint.expect("argument hint");
    assert_eq!((hint.function.as_str(), hint.current), ("VLOOKUP", 0));
    run(
        &mut d,
        G::SetEditText {
            text: "=VLOOKUP(A1,".into(),
            start: 12,
            end: 12,
            formula_bar: false,
        },
    );
    assert_eq!(
        state(&d).editor.and_then(|e| e.hint).map(|h| h.current),
        Some(1)
    );
}

#[test]
fn autocomplete_suggests_defined_names_and_moves_the_highlight() {
    let mut d = doc();
    d.workbook
        .apply(&kiln_sheet::Command::DefineName {
            name: "SalesTax".into(),
            formula: "0.2".into(),
            scope: None,
            hidden: false,
            comment: None,
        })
        .expect("name");
    run(
        &mut d,
        G::BeginEdit {
            text: "=sal".into(),
        },
    );
    let e = state(&d).editor.expect("editor");
    assert!(
        e.completions
            .iter()
            .any(|c| c.label == "SalesTax" && c.kind == CompletionKind::Name)
    );
    run(&mut d, G::BeginEdit { text: "=SU".into() });
    run(&mut d, G::CancelEdit);
    run(&mut d, G::BeginEdit { text: "=SU".into() });
    let n = state(&d).editor.map_or(0, |e| e.completions.len());
    assert!(n > 3);
    run(&mut d, G::MoveCompletion { down: true });
    assert_eq!(state(&d).editor.map(|e| e.completion_index), Some(1));
    run(&mut d, G::MoveCompletion { down: false });
    run(&mut d, G::MoveCompletion { down: false });
    assert_eq!(
        state(&d).editor.map(|e| e.completion_index as usize),
        Some(n - 1),
        "wraps"
    );
}

#[test]
fn text_entries_autocomplete_from_the_column() {
    let mut d = doc();
    put(&mut d, 0, 0, "Glaze chemicals");
    put(&mut d, 1, 0, "Gas for the kiln");
    at(&mut d, 2, 0);
    run(&mut d, G::BeginEdit { text: "gl".into() });
    let e = state(&d).editor.expect("editor");
    assert_eq!(e.inline_completion.as_deref(), Some("aze chemicals"));
    assert_eq!(e.text, "glaze chemicals");
    assert_eq!((e.start, e.end), (2, 15), "the completion is selected");
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Down),
        },
    );
    assert_eq!(text(&d, 2, 0), "glaze chemicals");
    at(&mut d, 3, 0);
    run(&mut d, G::BeginEdit { text: "g".into() });
    assert_eq!(
        state(&d).editor.and_then(|e| e.inline_completion),
        None,
        "two values match: no guess"
    );
}

#[test]
fn alt_down_lists_the_column_values() {
    let mut d = doc();
    put(&mut d, 0, 1, "pear");
    put(&mut d, 1, 1, "apple");
    put(&mut d, 2, 1, "Apple");
    at(&mut d, 3, 1);
    run(&mut d, G::OpenDropdown);
    let list = state(&d).editor.and_then(|e| e.pick_list).expect("list");
    assert_eq!(list.len(), 2, "{list:?}");
    assert_eq!(list[1], "pear");
    run(&mut d, G::AcceptCompletion { index: 1 });
    run(&mut d, G::CommitEdit { advance: None });
    assert_eq!(text(&d, 3, 1), "pear");
}

#[test]
fn point_mode_inserts_references_with_clicks_and_arrows() {
    let mut d = doc();
    at(&mut d, 5, 5);
    run(&mut d, G::BeginEdit { text: "=".into() });
    run(
        &mut d,
        G::PointRef {
            row: 1,
            col: 1,
            extend: false,
        },
    );
    assert_eq!(state(&d).mode, GridMode::Point);
    assert_eq!(d.edit.as_ref().map(|e| e.text.clone()), Some("=B2".into()));
    run(
        &mut d,
        G::PointMove {
            dir: D::Down,
            extend: true,
        },
    );
    assert_eq!(
        d.edit.as_ref().map(|e| e.text.clone()),
        Some("=B2:B3".into())
    );
    run(
        &mut d,
        G::SetEditText {
            text: "=B2:B3+".into(),
            start: 7,
            end: 7,
            formula_bar: false,
        },
    );
    run(
        &mut d,
        G::PointMove {
            dir: D::Up,
            extend: false,
        },
    );
    assert_eq!(
        d.edit.as_ref().map(|e| e.text.clone()),
        Some("=B2:B3+F5".into()),
        "from the active cell"
    );
    let refs = state(&d).editor.map(|e| e.refs).unwrap_or_default();
    assert_eq!(refs.len(), 2);
    assert_ne!(refs[0].color, refs[1].color);
    run(&mut d, G::CommitEdit { advance: None });
    assert_eq!(active(&d), (5, 5));
}

#[test]
fn arrows_commit_in_enter_mode_but_not_in_edit_mode() {
    let mut d = doc();
    run(&mut d, G::BeginEdit { text: "x".into() });
    run(
        &mut d,
        G::PointMove {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(text(&d, 0, 0), "x");
    assert_eq!(active(&d), (0, 1));
    run(&mut d, G::EditCell);
    assert_eq!(
        run(
            &mut d,
            G::PointMove {
                dir: D::Right,
                extend: false
            }
        ),
        Change::None
    );
    assert!(d.edit.is_some(), "Edit mode keeps the entry");
}

#[test]
fn clicking_another_cell_commits_a_plain_entry() {
    let mut d = doc();
    run(
        &mut d,
        G::BeginEdit {
            text: "hello".into(),
        },
    );
    run(
        &mut d,
        G::PointRef {
            row: 4,
            col: 4,
            extend: false,
        },
    );
    assert_eq!(text(&d, 0, 0), "hello");
    assert_eq!(active(&d), (4, 4));
}

#[test]
fn ctrl_enter_fills_the_selection_with_adjusted_formulas() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 0, "2");
    put(&mut d, 2, 0, "3");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 1,
                r1: 2,
                c1: 1,
            },
            add: false,
        },
    );
    run(
        &mut d,
        G::BeginEdit {
            text: "=A1*10".into(),
        },
    );
    run(&mut d, G::FillSelection);
    assert_eq!(
        (num(&d, 0, 1), num(&d, 1, 1), num(&d, 2, 1)),
        (10.0, 20.0, 30.0)
    );
    assert_eq!(
        ranges(&d),
        vec![Range::new(0, 1, 2, 1)],
        "the selection stays"
    );
}

#[test]
fn ctrl_enter_without_an_entry_copies_the_active_cell() {
    let mut d = doc();
    put(&mut d, 0, 0, "7");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 3,
                c1: 0,
            },
            add: false,
        },
    );
    run(&mut d, G::FillSelection);
    assert_eq!(num(&d, 3, 0), 7.0);
}

#[test]
fn array_formulas_spill() {
    let mut d = doc();
    run(
        &mut d,
        G::BeginEdit {
            text: "=SEQUENCE(3)".into(),
        },
    );
    run(&mut d, G::ArrayFormula);
    assert_eq!((num(&d, 0, 0), num(&d, 2, 0)), (1.0, 3.0));
}

#[test]
fn data_validation_refuses_invalid_entries() {
    let mut d = doc();
    let mut rule = kiln_sheet::meta::DataValidation::new(
        1,
        vec![Range::cell(0, 0)],
        kiln_sheet::meta::ValidationType::Whole,
    );
    rule.formula1 = Some("1".into());
    rule.formula2 = Some("10".into());
    d.workbook
        .apply(&kiln_sheet::Command::SetSheetMeta {
            sheet: sid(&d),
            field: Box::new(kiln_sheet::commands::MetaField::Validations(vec![rule])),
        })
        .expect("rule");
    run(&mut d, G::BeginEdit { text: "50".into() });
    let err = fails(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Down),
        },
    );
    assert!(
        err.contains("validation") || err.contains("restrict"),
        "{err}"
    );
    assert!(d.edit.is_some(), "the entry stays for correction");
    run(
        &mut d,
        G::SetEditText {
            text: "5".into(),
            start: 1,
            end: 1,
            formula_bar: false,
        },
    );
    run(&mut d, G::CommitEdit { advance: None });
    assert_eq!(num(&d, 0, 0), 5.0);
}

#[test]
fn ctrl_semicolon_enters_the_date_and_time() {
    let mut d = doc();
    d.set_clock(1_768_478_400_000); // 2026-01-15 12:00 UTC
    run(
        &mut d,
        G::InsertDateTime {
            time: false,
            utc_offset_minutes: 0,
        },
    );
    assert_eq!(text(&d, 0, 0), "1/15/2026");
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::InsertDateTime {
            time: true,
            utc_offset_minutes: -300,
        },
    );
    assert_eq!(text(&d, 1, 0), "7:00 AM");
    at(&mut d, 2, 0);
    run(
        &mut d,
        G::BeginEdit {
            text: "Due ".into(),
        },
    );
    run(
        &mut d,
        G::InsertDateTime {
            time: false,
            utc_offset_minutes: 0,
        },
    );
    assert_eq!(
        d.edit.as_ref().map(|e| e.text.clone()),
        Some("Due 1/15/2026".into())
    );
}

#[test]
fn autosum_proposes_the_range_above() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 0, "2");
    put(&mut d, 2, 0, "4");
    at(&mut d, 3, 0);
    run(&mut d, G::AutoSum { function: None });
    let s = state(&d);
    assert_eq!(s.mode, GridMode::Point);
    assert_eq!(
        s.editor.as_ref().map(|e| e.text.clone()),
        Some("=SUM(A1:A3)".into())
    );
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Down),
        },
    );
    assert_eq!(num(&d, 3, 0), 7.0);
}

#[test]
fn autosum_totals_a_selected_block() {
    let mut d = doc();
    data(&mut d);
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 3,
                c1: 2,
            },
            add: false,
        },
    );
    run(
        &mut d,
        G::AutoSum {
            function: Some("max".into()),
        },
    );
    assert_eq!((num(&d, 4, 0), num(&d, 4, 2)), (30.0, 32.0));
    assert!(
        fails(
            &mut d,
            G::AutoSum {
                function: Some("PRODUCT".into())
            }
        )
        .contains("PRODUCT")
    );
}

// ── clear and fill ──────────────────────────────────────────────────────────────────────

#[test]
fn delete_clears_contents_but_keeps_formats() {
    let mut d = doc();
    put(&mut d, 0, 0, "x");
    ed(&mut d, EditCommand::Bold);
    run(&mut d, G::ClearContents);
    assert!(val(&d, 0, 0).is_blank());
    assert!(style(&d, 0, 0).font.bold);
    run(
        &mut d,
        G::Clear {
            what: GridClear::Formats,
        },
    );
    assert!(!style(&d, 0, 0).font.bold);
    put(&mut d, 0, 0, "y");
    run(
        &mut d,
        G::Clear {
            what: GridClear::All,
        },
    );
    assert!(val(&d, 0, 0).is_blank());
}

#[test]
fn ctrl_d_and_ctrl_r_fill() {
    let mut d = doc();
    put(&mut d, 0, 0, "5");
    put(&mut d, 0, 1, "=A1+1");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 3,
                c1: 1,
            },
            add: false,
        },
    );
    run(&mut d, G::FillDown);
    assert_eq!((num(&d, 3, 0), num(&d, 3, 1)), (5.0, 6.0));
    at(&mut d, 4, 0);
    run(&mut d, G::FillDown);
    assert_eq!(num(&d, 4, 0), 5.0, "a single cell copies the cell above");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 0,
                c1: 3,
            },
            add: false,
        },
    );
    run(&mut d, G::FillRight);
    assert_eq!(num(&d, 0, 3), 5.0);
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 6,
                c0: 0,
                r1: 8,
                c1: 0,
            },
            add: false,
        },
    );
    put(&mut d, 8, 0, "9");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 6,
                c0: 0,
                r1: 8,
                c1: 0,
            },
            add: false,
        },
    );
    run(&mut d, G::FillUp);
    assert_eq!(num(&d, 6, 0), 9.0);
    put(&mut d, 10, 5, "3");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 10,
                c0: 2,
                r1: 10,
                c1: 5,
            },
            add: false,
        },
    );
    run(&mut d, G::FillLeft);
    assert_eq!(num(&d, 10, 2), 3.0);
}

#[test]
fn the_fill_handle_continues_series() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 0, "3");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 1,
                c1: 0,
            },
            add: false,
        },
    );
    run(&mut d, G::DragFill { row: 4, col: 0 });
    let preview = d.fill_preview;
    assert_eq!(preview, Some(Range::new(0, 0, 4, 0)));
    run(
        &mut d,
        G::AutoFill {
            row: 4,
            col: 0,
            mode: GridFill::Auto,
        },
    );
    assert_eq!(num(&d, 4, 0), 9.0, "linear trend");
    assert_eq!(ranges(&d), vec![Range::new(0, 0, 4, 0)]);
    assert!(d.fill_preview.is_none());
    put(&mut d, 0, 2, "Jan");
    at(&mut d, 0, 2);
    run(
        &mut d,
        G::AutoFill {
            row: 0,
            col: 4,
            mode: GridFill::Auto,
        },
    );
    assert_eq!(text(&d, 0, 4), "Mar");
    put(&mut d, 5, 0, "7");
    at(&mut d, 5, 0);
    run(
        &mut d,
        G::AutoFill {
            row: 7,
            col: 0,
            mode: GridFill::Series,
        },
    );
    assert_eq!(num(&d, 7, 0), 9.0);
    at(&mut d, 5, 0);
    run(
        &mut d,
        G::AutoFill {
            row: 7,
            col: 0,
            mode: GridFill::Copy,
        },
    );
    assert_eq!(num(&d, 7, 0), 7.0);
}

// ── rows and columns ──────────────────────────────────────────────────────────────────────

#[test]
fn insert_and_delete_rows_and_columns() {
    let mut d = doc();
    put(&mut d, 2, 2, "x");
    put(&mut d, 5, 0, "=C3");
    run(
        &mut d,
        G::SelectRows {
            from: 0,
            to: 1,
            extend: false,
            add: false,
        },
    );
    run(&mut d, G::InsertRows);
    assert_eq!(text(&d, 4, 2), "x");
    assert_eq!(text(&d, 7, 0), "x", "references follow");
    run(
        &mut d,
        G::SelectColumns {
            from: 0,
            to: 0,
            extend: false,
            add: false,
        },
    );
    run(&mut d, G::InsertColumns);
    assert_eq!(text(&d, 4, 3), "x");
    run(&mut d, G::DeleteColumns);
    run(
        &mut d,
        G::SelectRows {
            from: 0,
            to: 1,
            extend: false,
            add: false,
        },
    );
    run(&mut d, G::DeleteRows);
    assert_eq!(text(&d, 2, 2), "x");
}

#[test]
fn insert_and_delete_cells_shift_neighbours() {
    let mut d = doc();
    put(&mut d, 0, 0, "a");
    put(&mut d, 1, 0, "b");
    put(&mut d, 0, 1, "c");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::InsertCells {
            shift: CellShift::Vertical,
        },
    );
    assert_eq!((text(&d, 1, 0), text(&d, 2, 0)), ("a".into(), "b".into()));
    assert_eq!(text(&d, 0, 1), "c", "other columns stay");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::DeleteCells {
            shift: CellShift::Vertical,
        },
    );
    assert_eq!(text(&d, 0, 0), "a");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::InsertCells {
            shift: CellShift::Horizontal,
        },
    );
    assert_eq!(text(&d, 0, 2), "c");
    run(
        &mut d,
        G::DeleteCells {
            shift: CellShift::Horizontal,
        },
    );
    assert_eq!(text(&d, 0, 1), "c");
    run(
        &mut d,
        G::SelectRows {
            from: 0,
            to: 0,
            extend: false,
            add: false,
        },
    );
    run(
        &mut d,
        G::InsertCells {
            shift: CellShift::Auto,
        },
    );
    assert_eq!(text(&d, 1, 0), "a", "whole rows insert rows");
}

#[test]
fn row_heights_and_column_widths() {
    let mut d = doc();
    at(&mut d, 3, 3);
    run(&mut d, G::RowHeight { points: 30.0 });
    run(&mut d, G::ColumnWidth { chars: 20.0 });
    let g = d.geometry().clone();
    assert_eq!(g.rows.size(3), 40.0);
    assert!(
        (g.cols.size(3) - 145.0).abs() <= 1.0,
        "20 characters ≈ 145 px, got {}",
        g.cols.size(3)
    );
    assert!(fails(&mut d, G::RowHeight { points: 500.0 }).contains("409"));
    run(
        &mut d,
        G::ResizeColumns {
            from: 1,
            to: 1,
            px: 100.0,
        },
    );
    assert_eq!(d.geometry().cols.size(1), 100.0);
    run(
        &mut d,
        G::SelectColumns {
            from: 4,
            to: 6,
            extend: false,
            add: false,
        },
    );
    run(
        &mut d,
        G::ResizeColumns {
            from: 5,
            to: 5,
            px: 30.0,
        },
    );
    let g = d.geometry().clone();
    assert_eq!(
        (g.cols.size(4), g.cols.size(6)),
        (30.0, 30.0),
        "the selected columns resize together"
    );
    run(
        &mut d,
        G::ResizeRows {
            from: 9,
            to: 9,
            px: 0.0,
        },
    );
    assert!(d.geometry().rows.is_hidden(9), "dragging to zero hides");
}

#[test]
fn autofit_sizes_to_content() {
    let mut d = doc();
    put(&mut d, 0, 0, "A rather long piece of text for the column");
    run(
        &mut d,
        G::AutoFitColumns {
            from: Some(0),
            to: Some(0),
        },
    );
    let w = d.geometry().cols.size(0);
    assert!(w > 200.0, "fitted to {w}");
    at(&mut d, 1, 1);
    ed(&mut d, EditCommand::FontSize { size: 28.0 });
    put(&mut d, 1, 1, "Big");
    let h = d.geometry().rows.size(1);
    assert!(h > 35.0, "the row grew with the font: {h}");
    run(&mut d, G::RowHeight { points: 12.0 });
    run(
        &mut d,
        G::AutoFitRows {
            from: None,
            to: None,
        },
    );
    assert!(d.geometry().rows.size(1) > 35.0);
}

#[test]
fn hide_and_unhide_rows_and_columns() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectRows {
            from: 2,
            to: 3,
            extend: false,
            add: false,
        },
    );
    run(&mut d, G::HideRows);
    assert!(d.geometry().rows.is_hidden(3));
    run(
        &mut d,
        G::SelectRows {
            from: 1,
            to: 4,
            extend: false,
            add: false,
        },
    );
    run(&mut d, G::UnhideRows);
    assert!(!d.geometry().rows.is_hidden(3));
    at(&mut d, 0, 5);
    run(&mut d, G::HideColumns);
    assert!(d.geometry().cols.is_hidden(5));
    at(&mut d, 0, 4);
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(active(&d), (0, 6), "arrows skip hidden columns");
    run(
        &mut d,
        G::SelectColumns {
            from: 4,
            to: 6,
            extend: false,
            add: false,
        },
    );
    run(&mut d, G::UnhideColumns);
    assert!(!d.geometry().cols.is_hidden(5));
}

// ── formats ───────────────────────────────────────────────────────────────────────────────

#[test]
fn bold_italic_underline_strike_toggle_by_the_active_cell() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 1,
                c1: 1,
            },
            add: false,
        },
    );
    ed(&mut d, EditCommand::Bold);
    ed(&mut d, EditCommand::Italic);
    ed(&mut d, EditCommand::Underline);
    ed(&mut d, EditCommand::Strikethrough);
    let s = style(&d, 1, 1);
    assert!(s.font.bold && s.font.italic && s.font.strike);
    assert_eq!(s.font.underline, Underline::Single);
    let f = state(&d).format;
    assert!(f.bold && f.italic && f.underline && f.strike);
    ed(&mut d, EditCommand::Bold);
    assert!(!style(&d, 0, 0).font.bold);
    ed(&mut d, EditCommand::DoubleUnderline);
    assert_eq!(style(&d, 0, 0).font.underline, Underline::Double);
}

#[test]
fn font_family_size_grow_shrink_and_colour() {
    let mut d = doc();
    ed(
        &mut d,
        EditCommand::FontFamily {
            family: "Georgia".into(),
        },
    );
    ed(&mut d, EditCommand::FontSize { size: 14.0 });
    assert_eq!(state(&d).format.font_family, "Georgia");
    assert_eq!(state(&d).format.font_size, 14.0);
    ed(&mut d, EditCommand::GrowFont { one_point: false });
    assert_eq!(state(&d).format.font_size, 16.0);
    ed(&mut d, EditCommand::ShrinkFont { one_point: true });
    assert_eq!(state(&d).format.font_size, 15.0);
    ed(
        &mut d,
        EditCommand::FontColor {
            color: Some("C00000".into()),
        },
    );
    assert_eq!(state(&d).format.font_color.as_deref(), Some("C00000"));
    ed(&mut d, EditCommand::FontColor { color: None });
    assert_eq!(state(&d).format.font_color, None);
    assert!(
        d.execute(&Command::Suite(SuiteCommand::Edit(EditCommand::FontSize {
            size: 0.0
        })))
        .is_err()
    );
}

#[test]
fn super_and_subscript() {
    let mut d = doc();
    ed(&mut d, EditCommand::Superscript);
    assert_eq!(
        style(&d, 0, 0).font.vert_align,
        kiln_sheet::styles::VertAlign::Superscript
    );
    ed(&mut d, EditCommand::Subscript);
    assert_eq!(
        style(&d, 0, 0).font.vert_align,
        kiln_sheet::styles::VertAlign::Subscript
    );
    ed(&mut d, EditCommand::Subscript);
    assert_eq!(
        style(&d, 0, 0).font.vert_align,
        kiln_sheet::styles::VertAlign::Baseline
    );
}

#[test]
fn fill_colour_and_no_fill() {
    let mut d = doc();
    run(
        &mut d,
        G::FillColor {
            color: Some("FFE066".into()),
        },
    );
    assert_eq!(state(&d).format.fill_color.as_deref(), Some("FFE066"));
    run(&mut d, G::FillColor { color: None });
    assert_eq!(state(&d).format.fill_color, None);
    assert!(
        fails(
            &mut d,
            G::FillColor {
                color: Some("zz".into())
            }
        )
        .contains("zz")
    );
}

#[test]
fn horizontal_alignment_toggles_back_to_general() {
    let mut d = doc();
    ed(&mut d, EditCommand::AlignCenter);
    assert_eq!(style(&d, 0, 0).alignment.horizontal, HAlign::Center);
    ed(&mut d, EditCommand::AlignCenter);
    assert_eq!(style(&d, 0, 0).alignment.horizontal, HAlign::General);
    ed(&mut d, EditCommand::AlignRight);
    assert_eq!(state(&d).format.h_align, GridHAlign::Right);
    run(
        &mut d,
        G::AlignHorizontal {
            align: GridHAlign::CenterAcross,
        },
    );
    assert_eq!(
        style(&d, 0, 0).alignment.horizontal,
        HAlign::CenterContinuous
    );
}

#[test]
fn vertical_alignment_and_wrap() {
    let mut d = doc();
    run(&mut d, G::AlignTop);
    assert_eq!(style(&d, 0, 0).alignment.vertical, VAlign::Top);
    run(&mut d, G::AlignMiddle);
    assert_eq!(style(&d, 0, 0).alignment.vertical, VAlign::Center);
    run(&mut d, G::AlignMiddle);
    assert_eq!(
        style(&d, 0, 0).alignment.vertical,
        VAlign::Bottom,
        "pressed again: back to bottom"
    );
    run(&mut d, G::AlignBottom);
    run(&mut d, G::WrapText);
    assert!(state(&d).format.wrap);
    run(&mut d, G::WrapText);
    assert!(!state(&d).format.wrap);
}

#[test]
fn indent_and_orientation() {
    let mut d = doc();
    ed(&mut d, EditCommand::Indent);
    ed(&mut d, EditCommand::Indent);
    let s = style(&d, 0, 0);
    assert_eq!(
        (s.alignment.indent, s.alignment.horizontal),
        (2, HAlign::Left)
    );
    ed(&mut d, EditCommand::Outdent);
    assert_eq!(state(&d).format.indent, 1);
    run(&mut d, G::Orientation { angle: 45 });
    assert_eq!(state(&d).format.rotation, 45);
    run(&mut d, G::Orientation { angle: -45 });
    assert_eq!(state(&d).format.rotation, 135);
    run(&mut d, G::Orientation { angle: -45 });
    assert_eq!(state(&d).format.rotation, 0, "the same angle toggles off");
    run(&mut d, G::Orientation { angle: 255 });
    assert_eq!(state(&d).format.rotation, 255);
    assert!(fails(&mut d, G::Orientation { angle: 120 }).contains("90"));
}

#[test]
fn merge_and_center_toggles() {
    let mut d = doc();
    put(&mut d, 0, 0, "Title");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 0,
                c1: 3,
            },
            add: false,
        },
    );
    run(&mut d, G::MergeCenter);
    assert_eq!(d.geometry().merges, vec![Range::new(0, 0, 0, 3)]);
    assert_eq!(style(&d, 0, 0).alignment.horizontal, HAlign::Center);
    assert!(state(&d).format.merged);
    run(&mut d, G::MergeCenter);
    assert!(d.geometry().merges.is_empty());
}

#[test]
fn merge_across_and_unmerge() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 2,
                c1: 2,
            },
            add: false,
        },
    );
    run(&mut d, G::MergeAcross);
    assert_eq!(d.geometry().merges.len(), 3);
    run(&mut d, G::UnmergeCells);
    assert!(d.geometry().merges.is_empty());
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 2,
                c1: 2,
            },
            add: false,
        },
    );
    run(&mut d, G::MergeCells);
    assert_eq!(d.geometry().merges, vec![Range::new(0, 0, 2, 2)]);
}

#[test]
fn number_format_presets_and_buttons() {
    let mut d = doc();
    put(&mut d, 0, 0, "1234.5");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::NumberFormat {
            format: "Currency".into(),
        },
    );
    assert_eq!(text(&d, 0, 0), "$1,234.50");
    assert_eq!(state(&d).format.number_format_name, "Currency");
    run(&mut d, G::PercentStyle);
    assert_eq!(text(&d, 0, 0), "123450%");
    run(&mut d, G::NumberStyle);
    assert_eq!(text(&d, 0, 0), "1,234.50");
    run(
        &mut d,
        G::NumberFormat {
            format: "0.000".into(),
        },
    );
    assert_eq!(text(&d, 0, 0), "1234.500");
    run(&mut d, G::DecreaseDecimal);
    assert_eq!(text(&d, 0, 0), "1234.50");
    run(&mut d, G::IncreaseDecimal);
    run(&mut d, G::IncreaseDecimal);
    assert_eq!(text(&d, 0, 0), "1234.5000");
    run(
        &mut d,
        G::NumberFormat {
            format: "General".into(),
        },
    );
    run(&mut d, G::DecreaseDecimal);
    assert_eq!(text(&d, 0, 0), "1235");
}

#[test]
fn accounting_comma_and_currency_styles() {
    let mut d = doc();
    put(&mut d, 0, 0, "-1500");
    at(&mut d, 0, 0);
    run(&mut d, G::CommaStyle);
    assert!(text(&d, 0, 0).contains("(1,500.00)"));
    run(
        &mut d,
        G::AccountingStyle {
            currency: Some("GBP".into()),
        },
    );
    assert!(state(&d).format.number_format.contains('£'));
    run(&mut d, G::CurrencyStyle);
    assert_eq!(text(&d, 0, 0), "($1,500.00)");
    run(
        &mut d,
        G::NumberFormat {
            format: "Short Date".into(),
        },
    );
    put(&mut d, 1, 0, "45000");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 1,
                c0: 0,
                r1: 1,
                c1: 0,
            },
            add: false,
        },
    );
    run(
        &mut d,
        G::NumberFormat {
            format: "Short Date".into(),
        },
    );
    assert_eq!(text(&d, 1, 0), "3/15/2023");
}

#[test]
fn border_presets() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 1,
                c0: 1,
                r1: 2,
                c1: 2,
            },
            add: false,
        },
    );
    run(
        &mut d,
        G::Borders {
            preset: GridBorders::All,
            style: None,
            color: None,
        },
    );
    let b = style(&d, 1, 1).border;
    assert!(b.top.is_some() && b.right.is_some() && b.bottom.is_some() && b.left.is_some());
    run(&mut d, G::BorderNone);
    assert!(style(&d, 1, 1).border.top.is_none());
    run(&mut d, G::BorderOutline);
    assert!(style(&d, 1, 1).border.top.is_some());
    assert!(style(&d, 1, 1).border.right.is_none(), "no inside lines");
    run(
        &mut d,
        G::Borders {
            preset: GridBorders::ThickBox,
            style: None,
            color: Some("1D8A60".into()),
        },
    );
    assert_eq!(
        style(&d, 2, 2).border.bottom.map(|s| s.style),
        Some(BorderStyle::Thick)
    );
    run(
        &mut d,
        G::Borders {
            preset: GridBorders::BottomDouble,
            style: None,
            color: None,
        },
    );
    assert_eq!(
        style(&d, 2, 1).border.bottom.map(|s| s.style),
        Some(BorderStyle::Double)
    );
    run(
        &mut d,
        G::Borders {
            preset: GridBorders::Inside,
            style: Some(GridLineStyle::Dashed),
            color: None,
        },
    );
    assert_eq!(
        style(&d, 1, 1).border.right.map(|s| s.style),
        Some(BorderStyle::Dashed)
    );
}

#[test]
fn format_cells_reads_and_applies_every_tab() {
    let mut d = doc();
    put(&mut d, 0, 0, "0.125");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::ApplyFormat {
            format: Box::new(GridFormat {
                number_format: Some("0.0%".into()),
                font_family: Some("Verdana".into()),
                font_size: Some(13.0),
                bold: Some(true),
                underline: Some("doubleAccounting".into()),
                subscript: Some(true),
                font_color: Some("3159C6".into()),
                pattern: Some("lightGrid".into()),
                pattern_color: Some("A5A5A5".into()),
                fill_color: Some("FFFFFF".into()),
                h_align: Some(GridHAlign::Distributed),
                v_align: Some(kiln_protocol::commands::GridVAlign::Center),
                shrink: Some(true),
                indent: Some(2),
                rotation: Some(30),
                border_top: Some(kiln_protocol::commands::GridBorderSide {
                    style: GridLineStyle::Medium,
                    color: Some("FF0000".into()),
                }),
                border_diagonal_up: Some(kiln_protocol::commands::GridBorderSide {
                    style: GridLineStyle::Thin,
                    color: None,
                }),
                locked: Some(false),
                hidden: Some(true),
                ..GridFormat::default()
            }),
        },
    );
    let fc = match d
        .app_query(&Query::Grid(GridQuery::FormatCells))
        .expect("dialog")
    {
        QueryResult::Grid(GridQueryResult::FormatCells(f)) => *f,
        other => panic!("{other:?}"),
    };
    assert_eq!(fc.number_format, "0.0%");
    assert_eq!(fc.category, "Percentage");
    assert_eq!(fc.sample, "12.5%");
    assert_eq!(fc.format.font_family, "Verdana");
    assert!(fc.format.bold && fc.subscript && !fc.superscript);
    assert_eq!(fc.underline, "doubleAccounting");
    assert_eq!(fc.pattern, "lightGrid");
    assert_eq!(fc.format.h_align, GridHAlign::Distributed);
    assert!(fc.shrink);
    assert_eq!(fc.format.indent, 2);
    assert_eq!(fc.format.rotation, 30);
    assert_eq!(fc.border_top.map(|s| s.style), Some(GridLineStyle::Medium));
    assert!(fc.border_diagonal_up.is_some() && fc.border_diagonal_down.is_none());
    assert!(!fc.locked && fc.hidden);
    assert!(fc.fonts.iter().any(|f| f == "Calibri"));
    let preview = d
        .app_query(&Query::Grid(GridQuery::FormatPreview {
            code: "0.000".into(),
        }))
        .expect("preview");
    assert_eq!(
        preview,
        QueryResult::Grid(GridQueryResult::Preview("0.125".into()))
    );
    assert!(
        fails(
            &mut d,
            G::ApplyFormat {
                format: Box::new(GridFormat {
                    underline: Some("wavy".into()),
                    ..GridFormat::default()
                })
            }
        )
        .contains("wavy")
    );
}

#[test]
fn clear_formatting_removes_formats() {
    let mut d = doc();
    put(&mut d, 0, 0, "5");
    ed(&mut d, EditCommand::Bold);
    run(
        &mut d,
        G::FillColor {
            color: Some("FFFF00".into()),
        },
    );
    ed(&mut d, EditCommand::ClearFormatting);
    assert!(!style(&d, 0, 0).font.bold);
    assert_eq!(num(&d, 0, 0), 5.0);
}

// ── clipboard ─────────────────────────────────────────────────────────────────────────────

#[test]
fn copy_puts_text_html_and_kiln_on_the_clipboard() {
    let mut d = doc();
    put(&mut d, 0, 0, "a");
    put(&mut d, 0, 1, "1");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 0,
                c1: 1,
            },
            add: false,
        },
    );
    ed(&mut d, EditCommand::Copy);
    let c = d.take_clipboard().expect("clipboard");
    assert_eq!(c.text.trim_end(), "a\t1");
    assert!(c.html.as_deref().is_some_and(|h| h.contains("<table")));
    assert!(c.kiln.as_deref().is_some_and(|k| k.contains("kilnGrid")));
    assert!(state(&d).copy_marked);
    run(&mut d, G::CancelEdit);
    assert!(!state(&d).copy_marked, "Esc clears the marquee");
}

#[test]
fn paste_adjusts_formulas_from_kilns_own_copy() {
    let mut d = doc();
    put(&mut d, 0, 0, "2");
    put(&mut d, 0, 1, "=A1*3");
    at(&mut d, 0, 1);
    ed(&mut d, EditCommand::Copy);
    let c = d.take_clipboard();
    put(&mut d, 4, 0, "10");
    at(&mut d, 4, 1);
    ed(&mut d, EditCommand::Paste { data: c });
    assert_eq!(num(&d, 4, 1), 30.0);
}

#[test]
fn cut_and_paste_moves_once() {
    let mut d = doc();
    put(&mut d, 0, 0, "moved");
    put(&mut d, 3, 0, "=A1");
    at(&mut d, 0, 0);
    ed(&mut d, EditCommand::Cut);
    let c = d.take_clipboard();
    at(&mut d, 0, 4);
    ed(&mut d, EditCommand::Paste { data: c.clone() });
    assert_eq!(text(&d, 0, 4), "moved");
    assert!(val(&d, 0, 0).is_blank());
    assert_eq!(text(&d, 3, 0), "moved", "references follow the cut");
    assert!(!state(&d).copy_marked);
}

#[test]
fn paste_from_other_apps_reads_tsv_and_html() {
    let mut d = doc();
    at(&mut d, 2, 2);
    ed(
        &mut d,
        EditCommand::Paste {
            data: Some(ClipboardData {
                text: "1\t2\n3\t4".into(),
                html: None,
                kiln: None,
            }),
        },
    );
    assert_eq!((num(&d, 2, 2), num(&d, 3, 3)), (1.0, 4.0));
    assert_eq!(
        ranges(&d),
        vec![Range::new(2, 2, 3, 3)],
        "the pasted area is selected"
    );
    at(&mut d, 10, 0);
    ed(
        &mut d,
        EditCommand::Paste {
            data: Some(ClipboardData {
                text: "x".into(),
                html: Some("<table><tr><td><b>bold</b></td></tr></table>".into()),
                kiln: None,
            }),
        },
    );
    assert_eq!(text(&d, 10, 0), "bold");
}

#[test]
fn paste_special_values_formulas_formats_transpose_and_operations() {
    let mut d = doc();
    put(&mut d, 0, 0, "2");
    put(&mut d, 1, 0, "=A1*5");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 1,
                c1: 0,
            },
            add: false,
        },
    );
    ed(&mut d, EditCommand::Bold);
    ed(&mut d, EditCommand::Copy);
    at(&mut d, 0, 3);
    run(&mut d, G::PasteValues { data: None });
    assert_eq!(num(&d, 1, 3), 10.0);
    assert_eq!(d.workbook.formula(sid(&d), 1, 3), None, "values only");
    assert!(!style(&d, 0, 3).font.bold);
    at(&mut d, 0, 5);
    run(&mut d, G::PasteFormatting { data: None });
    assert!(style(&d, 0, 5).font.bold && val(&d, 0, 5).is_blank());
    at(&mut d, 5, 0);
    run(&mut d, G::PasteTranspose { data: None });
    // The cells turn: A1:A2 lands in A6:B6. (The model moves a transposed formula's references
    // by the paste offset; Excel transposes them too — handed to WP-69.)
    assert_eq!(num(&d, 5, 0), 2.0);
    assert!(d.workbook.formula(sid(&d), 5, 1).is_some());
    at(&mut d, 0, 7);
    run(&mut d, G::PasteFormulas { data: None });
    assert!(d.workbook.formula(sid(&d), 1, 7).is_some());
    put(&mut d, 0, 9, "1");
    put(&mut d, 1, 9, "1");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 1,
                c1: 0,
            },
            add: false,
        },
    );
    ed(&mut d, EditCommand::Copy);
    at(&mut d, 0, 9);
    run(
        &mut d,
        G::PasteSpecial {
            what: GridPaste::Values,
            op: GridPasteOp::Add,
            skip_blanks: false,
            transpose: false,
            data: None,
        },
    );
    assert_eq!((num(&d, 0, 9), num(&d, 1, 9)), (3.0, 11.0));
}

#[test]
fn paste_text_only_and_merge_formatting() {
    let mut d = doc();
    put(&mut d, 0, 0, "=1+1");
    at(&mut d, 0, 0);
    ed(&mut d, EditCommand::Bold);
    ed(&mut d, EditCommand::Copy);
    let c = d.take_clipboard();
    at(&mut d, 2, 0);
    ed(&mut d, EditCommand::PasteTextOnly { data: c.clone() });
    assert_eq!(num(&d, 2, 0), 2.0);
    assert!(d.workbook.formula(sid(&d), 2, 0).is_none() && !style(&d, 2, 0).font.bold);
    at(&mut d, 3, 0);
    ed(
        &mut d,
        EditCommand::PasteAs {
            mode: PasteMode::Merge,
            data: c,
        },
    );
    assert!(d.workbook.formula(sid(&d), 3, 0).is_some() && !style(&d, 3, 0).font.bold);
}

#[test]
fn copying_several_ranges_is_refused_like_excel() {
    let mut d = doc();
    run(
        &mut d,
        G::SelectCell {
            row: 0,
            col: 0,
            extend: false,
            add: false,
        },
    );
    run(
        &mut d,
        G::SelectCell {
            row: 5,
            col: 5,
            extend: false,
            add: true,
        },
    );
    let err = d
        .execute(&Command::Suite(SuiteCommand::Edit(EditCommand::Copy)))
        .expect_err("refused");
    assert!(err.to_string().contains("multiple"));
    assert!(
        d.execute(&Command::Suite(SuiteCommand::Edit(EditCommand::Paste {
            data: None
        })))
        .is_err(),
        "nothing to paste"
    );
}

// ── view ──────────────────────────────────────────────────────────────────────────────────

#[test]
fn freeze_panes_top_row_first_column_and_unfreeze() {
    let mut d = doc();
    at(&mut d, 3, 2);
    run(&mut d, G::FreezePanes);
    let s = state(&d);
    assert_eq!((s.layout.freeze_rows, s.layout.freeze_cols), (3, 2));
    assert_eq!((s.layout.frozen_h, s.layout.frozen_w), (60.0, 128.0));
    run(&mut d, G::FreezePanes);
    assert_eq!(state(&d).layout.freeze_rows, 0, "toggles off");
    run(&mut d, G::FreezeTopRow);
    assert_eq!(
        (state(&d).layout.freeze_rows, state(&d).layout.freeze_cols),
        (1, 0)
    );
    run(&mut d, G::FreezeFirstColumn);
    assert_eq!(
        (state(&d).layout.freeze_rows, state(&d).layout.freeze_cols),
        (0, 1)
    );
    assert!(!d.can_undo(), "freezing is not an edit");
}

#[test]
fn split_toggles() {
    let mut d = doc();
    at(&mut d, 5, 3);
    run(&mut d, G::Split);
    assert!(state(&d).layout.split);
    assert_eq!(state(&d).layout.freeze_rows, 5);
    run(&mut d, G::Split);
    assert!(!state(&d).layout.split);
}

#[test]
fn gridlines_headings_and_formula_view() {
    let mut d = doc();
    put(&mut d, 0, 0, "=2*3");
    run(&mut d, G::Gridlines);
    assert!(!state(&d).gridlines);
    run(&mut d, G::Headings);
    assert!(!state(&d).headings);
    assert_eq!(state(&d).layout.header_h, 0.0);
    run(&mut d, G::ShowFormulas);
    assert!(state(&d).show_formulas);
    run(&mut d, G::Headings);
    assert_eq!(state(&d).layout.header_h, 24.0);
}

#[test]
fn calculation_commands() {
    let mut d = doc();
    put(&mut d, 0, 0, "=1+1");
    assert_eq!(run(&mut d, G::CalculateNow), Change::Selection);
    run(&mut d, G::CalculateSheet);
    assert_eq!(num(&d, 0, 0), 2.0);
}

// ── undo, status, queries, frames ───────────────────────────────────────────────────────

#[test]
fn undo_and_redo_restore_every_step() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    ed(&mut d, EditCommand::Bold);
    assert!(d.can_undo());
    assert_eq!(d.undo_label(), Some("suite.format.bold"));
    assert!(d.undo());
    assert!(!style(&d, 0, 0).font.bold);
    assert_eq!(d.undo_label(), Some("grid.editing.commit"));
    assert!(d.undo());
    assert!(val(&d, 0, 0).is_blank());
    assert!(d.redo() && d.redo());
    assert!(style(&d, 0, 0).font.bold && num(&d, 0, 0) == 1.0);
    let state_before = d.state();
    run(
        &mut d,
        G::Move {
            dir: D::Down,
            extend: false,
        },
    );
    assert_eq!(d.state(), state_before, "moving is not an edit");
}

#[test]
fn undo_while_typing_discards_the_entry() {
    let mut d = doc();
    run(&mut d, G::BeginEdit { text: "zz".into() });
    assert!(d.can_undo());
    assert!(d.undo());
    assert!(d.edit.is_none());
    assert!(!d.can_undo());
}

#[test]
fn the_status_bar_sums_the_selection() {
    let mut d = doc();
    put(&mut d, 0, 0, "10");
    put(&mut d, 1, 0, "20");
    put(&mut d, 2, 0, "x");
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 0,
                c0: 0,
                r1: 3,
                c1: 0,
            },
            add: false,
        },
    );
    let s = d.status().cells.expect("stats");
    assert_eq!(
        (s.count, s.numeric_count, s.sum, s.average),
        (3, 2, 30.0, Some(15.0))
    );
    run(
        &mut d,
        G::SelectColumns {
            from: 0,
            to: 0,
            extend: false,
            add: false,
        },
    );
    assert_eq!(
        d.status().cells.map(|s| s.sum),
        Some(30.0),
        "whole columns too"
    );
}

#[test]
fn hit_testing_through_the_query() {
    let mut d = doc();
    let hit = |d: &GridDoc, x: f32, y: f32| match d
        .app_query(&Query::Grid(GridQuery::HitTest { x, y }))
        .expect("hit")
    {
        QueryResult::Grid(GridQueryResult::Hit(h)) => h,
        other => panic!("{other:?}"),
    };
    assert_eq!(hit(&d, 5.0, 5.0), GridHit::Corner);
    assert_eq!(hit(&d, 44.0 + 70.0, 30.0), GridHit::Cell { row: 0, col: 1 });
    assert_eq!(hit(&d, 44.0 + 70.0, 10.0), GridHit::ColumnHeader { col: 1 });
    assert_eq!(hit(&d, 10.0, 24.0 + 45.0), GridHit::RowHeader { row: 2 });
    assert_eq!(hit(&d, 44.0 + 64.0, 24.0 + 20.0), GridHit::FillHandle);
    run(&mut d, G::BeginEdit { text: "x".into() });
    assert_eq!(
        hit(&d, 44.0 + 64.0, 24.0 + 20.0),
        GridHit::Cell { row: 1, col: 1 },
        "no handle while editing"
    );
}

#[test]
fn hints_functions_and_zoom_queries() {
    let d = doc();
    let q = |q: GridQuery| match d.app_query(&Query::Grid(q)).expect("query") {
        QueryResult::Grid(r) => r,
        other => panic!("{other:?}"),
    };
    let GridQueryResult::Hint(Some(h)) = q(GridQuery::FormulaHint {
        text: "=ROUND(A1,".into(),
        caret: 10,
    }) else {
        panic!("hint");
    };
    assert_eq!((h.function.as_str(), h.current), ("ROUND", 1));
    let GridQueryResult::Functions(list) = q(GridQuery::Functions {
        prefix: "xl".into(),
        category: None,
    }) else {
        panic!("functions");
    };
    assert!(list.iter().any(|f| f.name == "XLOOKUP"));
    let GridQueryResult::Zoom(z) = q(GridQuery::ZoomToSelection {
        width: 1000.0,
        height: 700.0,
    }) else {
        panic!("zoom");
    };
    assert!(
        (3.9..=4.0).contains(&z),
        "a single cell zooms to the maximum: {z}"
    );
    assert!(
        d.app_query(&Query::Grid(GridQuery::FormatPreview {
            code: "0".repeat(300)
        }))
        .is_err()
    );
}

#[test]
fn frames_paint_tiles_headers_and_the_overlay() {
    let mut d = doc();
    put(&mut d, 0, 0, "Kiln");
    let mut f = FrameState::new();
    let vp = Viewport {
        width_px: 900.0,
        height_px: 600.0,
        scroll: Point::new(0.0, 0.0),
        zoom: 1.0,
        dpr: 1.0,
        mode: ViewMode::Normal,
    };
    let delta = f.produce(d.frame_source(), &vp, 1).expect("frame");
    assert!(delta.reset);
    assert!(
        delta
            .tiles
            .iter()
            .any(|t| t.id.layer == kiln_sheet_layout::scene::COL_HEADER_LAYER)
    );
    assert!(
        delta
            .tiles
            .iter()
            .any(|t| t.id.layer == kiln_sheet_layout::scene::ROW_HEADER_LAYER)
    );
    assert!(
        delta
            .tiles
            .iter()
            .any(|t| t.id.layer == kiln_sheet_layout::scene::CORNER_LAYER)
    );
    assert!(
        delta
            .tiles
            .iter()
            .any(|t| kiln_sheet_layout::scene::is_cell_layer(t.id.layer))
    );
    assert!(!delta.atlas.is_empty(), "glyphs for the text");
    assert!(
        delta.overlay.as_ref().is_some_and(|o| !o.ops.is_empty()),
        "the selection"
    );
    assert!(
        delta.extent.x < 0.0 && delta.extent.y < 0.0,
        "headers left of and above the cells"
    );
    // The prefetch margins fill in over the next frames; then nothing changes and nothing is
    // sent.
    let mut frames = 0;
    while !f
        .produce(d.frame_source(), &vp, 1)
        .expect("frame")
        .is_empty()
    {
        frames += 1;
        assert!(frames < 100, "prefetch never ends");
    }
    assert!(frames > 0, "prefetch tiles came after the first frame");
    let again = f.produce(d.frame_source(), &vp, 1).expect("frame");
    assert!(again.is_empty());
    // An edit repaints its tile only.
    put(&mut d, 0, 0, "Grid");
    let after = f.produce(d.frame_source(), &vp, 2).expect("frame");
    // The edited cell's tile row (its text may overflow along the row), never the whole view.
    assert!(
        !after.tiles.is_empty() && after.tiles.len() <= 8,
        "{} tiles",
        after.tiles.len()
    );
    assert!(
        after
            .tiles
            .iter()
            .all(|t| t.rect.y <= 0.0 || !kiln_sheet_layout::scene::is_cell_layer(t.id.layer))
    );
}

#[test]
fn frozen_frames_keep_the_frozen_tiles_while_scrolling_far() {
    let mut d = doc();
    put(&mut d, 0, 0, "Header");
    at(&mut d, 1, 0);
    run(&mut d, G::FreezePanes);
    let mut f = FrameState::new();
    let vp = |y: f32| Viewport {
        width_px: 900.0,
        height_px: 600.0,
        scroll: Point::new(0.0, y),
        zoom: 1.0,
        dpr: 1.0,
        mode: ViewMode::Normal,
    };
    f.produce(d.frame_source(), &vp(0.0), 1).expect("frame");
    let far = f
        .produce(d.frame_source(), &vp(200_000.0), 1)
        .expect("frame");
    let top_tile = kiln_sheet_layout::scene::cell_tile_id(0, 0);
    assert!(
        !far.removed.contains(&top_tile),
        "the frozen row's tile stays"
    );
    assert!(
        far.tiles.iter().any(|t| t.rect.y >= 190_000.0),
        "tiles where the view went"
    );
}

#[test]
fn the_accessibility_tree_names_cells_and_announces_the_active_one() {
    let mut d = doc();
    put(&mut d, 1, 1, "=6*7");
    at(&mut d, 1, 1);
    let tree = d.a11y("Book");
    assert_eq!(tree.nodes[0].text.as_deref(), Some("1048576 16384"));
    let b2 = tree
        .nodes
        .iter()
        .find(|n| n.name.as_deref() == Some("B2"))
        .expect("B2");
    assert_eq!(b2.text.as_deref(), Some("42"));
    assert_eq!(tree.active, Some(b2.id));
    assert_eq!(tree.announce.as_deref(), Some("B2, 42, formula =6*7"));
}

#[test]
fn saving_keeps_the_selection_and_active_sheet() {
    let mut d = doc();
    run(&mut d, G::InsertSheet);
    run(
        &mut d,
        G::SelectRange {
            range: GridRange {
                r0: 2,
                c0: 1,
                r1: 4,
                c1: 3,
            },
            add: false,
        },
    );
    let bytes = d.save(kiln_protocol::FileFormat::Xlsx).expect("save");
    let back = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
    assert_eq!(back.workbook.props().active_sheet, 1);
    let sel = &back.workbook.sheets()[1].meta().view.selection;
    assert_eq!(sel.ranges, vec![Range::new(2, 1, 4, 3)]);
    assert_eq!(sel.active, CellPos::new(2, 1));
}

#[test]
fn unsupported_commands_say_so() {
    let mut d = doc();
    let err = d
        .execute(&Command::Suite(SuiteCommand::Edit(EditCommand::Bullets {
            glyph: None,
        })))
        .expect_err("not in Grid");
    assert!(err.to_string().contains("Grid"));
    assert!(
        d.execute(&Command::Grid(G::FillColor {
            color: Some("12345".into())
        }))
        .is_err()
    );
}

#[test]
fn the_painter_keeps_every_visible_tile_after_the_prefetch_frames() {
    let mut d = doc();
    put(&mut d, 0, 0, "Kiln");
    let mut f = FrameState::new();
    let vp = Viewport {
        width_px: 1398.0,
        height_px: 585.0,
        scroll: Point::new(0.0, 0.0),
        zoom: 1.0,
        dpr: 1.0,
        mode: ViewMode::Normal,
    };
    // What the painter holds, applying each delta as the surface's cache does.
    let mut held: std::collections::BTreeSet<(u16, u32)> = std::collections::BTreeSet::new();
    for _ in 0..60 {
        let delta = f.produce(d.frame_source(), &vp, 1).expect("frame");
        if delta.reset {
            held.clear();
        }
        for id in &delta.removed {
            held.remove(&(id.layer, id.index));
        }
        for t in &delta.tiles {
            held.insert((t.id.layer, t.id.index));
        }
        if delta.is_empty() {
            break;
        }
    }
    for ty in 0..3 {
        for tx in 0..6 {
            let id = kiln_sheet_layout::scene::cell_tile_id(tx, ty);
            assert!(
                held.contains(&(id.layer, id.index)),
                "tile {tx},{ty} is gone: {held:?}"
            );
        }
    }
}

#[test]
fn enter_after_tab_returns_to_the_column_the_row_started_in() {
    let mut d = doc();
    at(&mut d, 0, 0);
    for (text, dir) in [("Item", D::Right), ("Q1", D::Right), ("Q2", D::Down)] {
        run(&mut d, G::BeginEdit { text: text.into() });
        run(&mut d, G::CommitEdit { advance: Some(dir) });
    }
    assert_eq!(d.view().sel.active, CellPos::new(1, 0), "back to A2");
    // Moving elsewhere forgets the run.
    run(&mut d, G::BeginEdit { text: "x".into() });
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Right),
        },
    );
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: false,
        },
    );
    run(&mut d, G::BeginEdit { text: "y".into() });
    run(
        &mut d,
        G::CommitEdit {
            advance: Some(D::Down),
        },
    );
    assert_eq!(d.view().sel.active, CellPos::new(2, 2));
}

#[test]
fn whole_sheet_commands_finish_at_once() {
    // Ctrl+A twice, then the fills and Delete: each works on the used part instead of a million
    // rows of blanks (the formats go to row and column formats in kiln-sheet).
    let mut d = doc();
    put(&mut d, 0, 0, "5");
    put(&mut d, 1, 0, "6");
    put(&mut d, 2, 1, "x");
    let everything = G::SelectRange {
        range: GridRange {
            r0: 0,
            c0: 0,
            r1: MAX_ROWS - 1,
            c1: MAX_COLS - 1,
        },
        add: false,
    };
    for c in [
        G::FillRight,
        G::FillDown,
        G::FillLeft,
        G::FillUp,
        G::ClearContents,
    ] {
        run(&mut d, everything.clone());
        let t = std::time::Instant::now();
        run(&mut d, c.clone());
        let ms = t.elapsed().as_millis();
        assert!(ms < 3_000, "{c:?} on the whole sheet took {ms} ms");
    }
    assert_eq!(
        val(&d, 0, 0),
        CellValue::Blank,
        "Delete cleared the used part"
    );
    // Copy and paste over everything (Ctrl+A, Ctrl+C, Ctrl+Shift+V): the copied block goes in
    // once, at the top left (it does not tile a million rows).
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 1, "2");
    run(&mut d, everything.clone());
    ed(&mut d, EditCommand::Copy);
    for c in [
        G::PasteValues { data: None },
        G::PasteFormulas { data: None },
        G::PasteFormatting { data: None },
    ] {
        run(&mut d, everything.clone());
        let t = std::time::Instant::now();
        let _ = d.execute(&Command::Grid(c.clone()));
        let ms = t.elapsed().as_millis();
        assert!(ms < 3_000, "{c:?} over the whole sheet took {ms} ms");
    }
    assert_eq!(num(&d, 1, 1), 2.0);
    run(&mut d, everything);
    let t = std::time::Instant::now();
    assert!(fails(&mut d, G::FillSelection).contains("too large"));
    assert!(
        t.elapsed().as_millis() < 1_000,
        "Ctrl+Enter refuses at once"
    );
}

#[test]
fn paste_tiles_a_multiple_selection_and_pastes_once_into_any_other() {
    let mut d = doc();
    put(&mut d, 0, 0, "7");
    at(&mut d, 0, 0);
    ed(&mut d, EditCommand::Copy);
    let sel = |d: &mut GridDoc, r0, c0, r1, c1| {
        run(
            d,
            G::SelectRange {
                range: GridRange { r0, c0, r1, c1 },
                add: false,
            },
        );
    };
    // One cell into 2 × 3: six copies.
    sel(&mut d, 2, 0, 3, 2);
    run(&mut d, G::PasteValues { data: None });
    assert_eq!((num(&d, 2, 0), num(&d, 3, 2)), (7.0, 7.0));
    // A 2 × 2 block into 3 × 3 (not a multiple): once, at the top left.
    put(&mut d, 0, 1, "8");
    put(&mut d, 1, 0, "9");
    sel(&mut d, 0, 0, 1, 1);
    ed(&mut d, EditCommand::Copy);
    sel(&mut d, 10, 0, 12, 2);
    run(&mut d, G::PasteValues { data: None });
    assert_eq!(
        (num(&d, 10, 0), num(&d, 10, 1), num(&d, 11, 0)),
        (7.0, 8.0, 9.0)
    );
    assert_eq!(val(&d, 12, 2), CellValue::Blank);
    assert_eq!(val(&d, 10, 2), CellValue::Blank);
}

// ── charts (WP-73 carry-over) ───────────────────────────────────────────────────────────────

fn chart_doc() -> GridDoc {
    let mut d = doc();
    for (r, row) in [
        ["", "Q1", "Q2", "Q3"],
        ["Clay", "420", "380", "455"],
        ["Glaze", "310", "125", "290"],
    ]
    .iter()
    .enumerate()
    {
        for (c, v) in row.iter().enumerate() {
            if !v.is_empty() {
                put(&mut d, r as u32, c as u32, v);
            }
        }
    }
    at(&mut d, 1, 1);
    d
}

fn suite(d: &mut GridDoc, c: SuiteCommand) -> Change {
    d.execute(&Command::Suite(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"))
}

fn frame_of(d: &mut GridDoc, f: &mut FrameState, rev: u64) -> kiln_scene::SceneDelta {
    let vp = Viewport {
        width_px: 1200.0,
        height_px: 700.0,
        scroll: Point::new(0.0, 0.0),
        zoom: 1.0,
        dpr: 1.0,
        mode: ViewMode::Normal,
    };
    f.produce(d.frame_source(), &vp, rev).expect("frame")
}

#[test]
fn insert_chart_from_the_region_selects_it_and_draws_a_tile() {
    use kiln_protocol::commands::ChartKind;
    let mut d = chart_doc();
    let mut f = FrameState::new();
    frame_of(&mut d, &mut f, 1);
    suite(
        &mut d,
        SuiteCommand::ChartInsert {
            kind: Some(ChartKind::ColumnClustered),
        },
    );
    let s = state(&d);
    let drawing = s.drawing.expect("the new chart is selected");
    assert_eq!(drawing.chart.kind, Some(ChartKind::ColumnClustered));
    assert_eq!(drawing.chart.series.len(), 2, "{:?}", drawing.chart.series);
    assert_eq!(drawing.chart.categories, vec!["Q1", "Q2", "Q3"]);
    assert_eq!(drawing.chart.data_range, "Sheet1!$A$1:$D$3");
    assert!((drawing.width - 480.0).abs() < 0.5 && (drawing.height - 288.0).abs() < 0.5);
    let delta = frame_of(&mut d, &mut f, 2);
    let tile = delta
        .tiles
        .iter()
        .find(|t| t.id.layer == kiln_sheet_layout::scene::DRAWING_LAYER)
        .expect("a chart tile");
    assert!(tile.ops.len() > 10, "the chart is drawn");
    assert!(
        delta
            .overlay
            .iter()
            .flat_map(|o| o.ops.iter())
            .any(|op| matches!(op, kiln_scene::DrawOp::PushTransform { .. })),
        "the selection frame"
    );
    // One undo step removes it.
    assert_eq!(d.undo_label(), Some("suite.chart.insert"));
    assert!(d.undo());
    assert!(state(&d).drawing.is_none());
    assert!(d.workbook.sheets()[0].meta().drawings.is_empty());
    assert!(d.redo());
    assert_eq!(d.workbook.sheets()[0].meta().drawings.len(), 1);
}

#[test]
fn chart_commands_edit_the_selected_chart_with_undo() {
    use kiln_protocol::commands::{ChartKind, ChartLegendPosition};
    let mut d = chart_doc();
    suite(&mut d, SuiteCommand::ChartInsert { kind: None });
    let kind = state(&d).drawing.unwrap().chart.kind;
    assert!(kind.is_some(), "a recommended chart");
    at(&mut d, 1, 1);
    let GridQueryResult::Charts(rec) = d.grid_query(&GridQuery::RecommendedCharts).unwrap() else {
        panic!("charts");
    };
    assert_eq!(rec.first().copied(), kind, "{rec:?}");
    run(&mut d, G::SelectDrawing { index: Some(0) });
    suite(
        &mut d,
        SuiteCommand::ChartChangeType {
            kind: ChartKind::Line,
        },
    );
    assert_eq!(state(&d).drawing.unwrap().chart.kind, Some(ChartKind::Line));
    suite(&mut d, SuiteCommand::ChartSwitchRowColumn);
    let c = state(&d).drawing.unwrap().chart;
    assert_eq!(c.series.len(), 3, "three quarters are now the series");
    assert_eq!(c.categories, vec!["Clay", "Glaze"]);
    suite(
        &mut d,
        SuiteCommand::ChartSetTitle {
            text: "Spend".into(),
        },
    );
    suite(
        &mut d,
        SuiteCommand::ChartSetLegend {
            position: Some(ChartLegendPosition::Top),
        },
    );
    let c = state(&d).drawing.unwrap().chart;
    assert_eq!(c.title.as_deref(), Some("Spend"));
    assert_eq!(c.legend, Some(ChartLegendPosition::Top));
    // Undo walks back through the chart's own steps.
    assert_eq!(d.undo_label(), Some("suite.chart.legend"));
    assert!(d.undo());
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(state(&d).drawing.unwrap().chart.series.len(), 2);
    assert!(d.undo());
    assert_eq!(state(&d).drawing.unwrap().chart.kind, kind);
    assert!(d.can_redo());
    assert!(d.redo());
    assert_eq!(state(&d).drawing.unwrap().chart.kind, Some(ChartKind::Line));
    // A cell edit clears the redo of chart steps and the chart follows the cells.
    put(&mut d, 1, 1, "999");
    assert!(!d.can_redo());
    run(&mut d, G::SelectDrawing { index: Some(0) });
    let c = state(&d).drawing.unwrap().chart;
    let max = c.axes.iter().find_map(|a| a.max).unwrap_or_default();
    assert!(
        max >= 999.0,
        "the value axis grew to the new value: {:?}",
        c.axes
    );
}

#[test]
fn charts_are_hit_selected_moved_resized_and_deleted() {
    let mut d = chart_doc();
    let mut f = FrameState::new();
    frame_of(&mut d, &mut f, 1);
    suite(&mut d, SuiteCommand::ChartInsert { kind: None });
    let r = state(&d).drawing.unwrap();
    // A click on a cell deselects it; a click inside the chart is a hit on it.
    at(&mut d, 0, 0);
    assert!(state(&d).drawing.is_none());
    let geo = d.geometry_ref();
    let hw = d.header_w(&geo);
    let (cx, cy) = (
        hw + r.x + r.width / 2.0,
        geo.header_h() + r.y + r.height / 2.0,
    );
    let q = |d: &GridDoc, x: f32, y: f32| match d.grid_query(&GridQuery::HitTest { x, y }).unwrap()
    {
        GridQueryResult::Hit(h) => h,
        other => panic!("{other:?}"),
    };
    assert_eq!(q(&d, cx, cy), GridHit::Drawing { index: 0 });
    run(&mut d, G::SelectDrawing { index: Some(0) });
    // Its bottom-right handle.
    assert_eq!(
        q(&d, hw + r.x + r.width, geo.header_h() + r.y + r.height),
        GridHit::DrawingHandle {
            index: 0,
            handle: 4
        }
    );
    run(
        &mut d,
        G::PlaceDrawing {
            index: 0,
            x: 10.0,
            y: 30.0,
            width: 300.0,
            height: 200.0,
        },
    );
    let moved = state(&d).drawing.unwrap();
    assert!(
        (moved.x - 10.0).abs() < 0.5 && (moved.y - 30.0).abs() < 0.5,
        "{moved:?}"
    );
    assert!((moved.width - 300.0).abs() < 0.5 && (moved.height - 200.0).abs() < 0.5);
    // Rows inserted above move it down (the anchor is in the model).
    at(&mut d, 0, 0);
    run(&mut d, G::InsertRows);
    run(&mut d, G::SelectDrawing { index: Some(0) });
    assert!(state(&d).drawing.unwrap().y > moved.y + 10.0);
    // Delete removes it; Esc deselects.
    run(&mut d, G::ClearContents);
    assert!(d.workbook.sheets()[0].meta().drawings.is_empty());
    assert!(d.undo());
    run(&mut d, G::SelectDrawing { index: Some(0) });
    run(&mut d, G::CancelEdit);
    assert!(state(&d).drawing.is_none());
    assert!(fails(&mut d, G::SelectDrawing { index: Some(7) }).contains("no chart"));
}

#[test]
fn charts_are_saved_and_read_back() {
    use kiln_protocol::commands::ChartKind;
    let mut d = chart_doc();
    suite(
        &mut d,
        SuiteCommand::ChartInsert {
            kind: Some(ChartKind::Pie),
        },
    );
    suite(
        &mut d,
        SuiteCommand::ChartSetTitle {
            text: "Clay share".into(),
        },
    );
    let bytes = d.save(kiln_protocol::FileFormat::Xlsx).expect("save");
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
    let drawings = &read.workbook.sheets()[0].meta().drawings;
    assert_eq!(drawings.len(), 1);
    let charts = read.source.charts();
    assert_eq!(charts.len(), 1);
    let (_, chart) = &charts[0];
    let chart = chart.as_ref().expect("chart");
    assert_eq!(kiln_charts::edit::kind_of(chart), Some(ChartKind::Pie));
    assert_eq!(
        kiln_charts::layout::title_text(chart).as_deref(),
        Some("Clay share")
    );
    // Opened again in a session, the chart is there and can be selected.
    let mut again = GridDoc::new(read.workbook, Some(read.source), Vec::new(), None);
    run(&mut again, G::SelectDrawing { index: Some(0) });
    assert_eq!(
        state(&again).drawing.unwrap().chart.kind,
        Some(ChartKind::Pie)
    );
}

#[test]
fn the_theme_recolours_charts_and_move_chart_goes_to_another_sheet() {
    use kiln_protocol::commands::ChartMoveTarget;
    let mut d = chart_doc();
    let mut f = FrameState::new();
    suite(&mut d, SuiteCommand::ChartInsert { kind: None });
    frame_of(&mut d, &mut f, 1);
    run(&mut d, G::Theme { dark: true });
    let delta = frame_of(&mut d, &mut f, 2);
    assert!(
        delta
            .tiles
            .iter()
            .any(|t| t.id.layer == kiln_sheet_layout::scene::DRAWING_LAYER),
        "the chart is drawn again in the dark theme"
    );
    run(&mut d, G::InsertSheet);
    run(&mut d, G::ActivateSheet { index: 0 });
    run(&mut d, G::SelectDrawing { index: Some(0) });
    let other = d.workbook.sheets()[1].name().to_owned();
    suite(
        &mut d,
        SuiteCommand::ChartMove {
            to: ChartMoveTarget::ObjectIn { sheet: other },
        },
    );
    assert_eq!(d.active_index(), 1);
    assert!(state(&d).drawing.is_some());
    assert!(d.workbook.sheets()[0].meta().drawings.is_empty());
    let err = d
        .execute(&Command::Suite(SuiteCommand::ChartMove {
            to: ChartMoveTarget::NewSheet {
                name: "Chart1".into(),
            },
        }))
        .expect_err("chart sheets");
    assert!(err.to_string().contains("chart sheets"));
    // Without a chart selected, chart commands say so.
    at(&mut d, 0, 0);
    assert!(
        d.execute(&Command::Suite(SuiteCommand::ChartSwitchRowColumn))
            .is_err()
    );
}
