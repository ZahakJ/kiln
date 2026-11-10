// SPDX-License-Identifier: AGPL-3.0-or-later
//! The surface gestures: F8 and Shift+F8, drag and drop of cells, the error indicator.

use kiln_protocol::commands::grid_surface::{GridDragPhase, GridErrorRule, GridSelectMode};
use kiln_protocol::commands::{
    GridCommand as G, GridDirection as D, GridHit, GridQuery, GridQueryResult, GridState,
};
use kiln_protocol::{Command, Query, QueryResult};
use kiln_sheet::{CellValue, Range, Workbook};

use super::GridDoc;
use crate::model::DocModel;

fn doc() -> GridDoc {
    let mut d = GridDoc::new(Workbook::new(), None, Vec::new(), Some("Book.xlsx".into()));
    d.last_view = kiln_sheet_layout::View {
        width: 1200.0,
        height: 700.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
    };
    d
}

fn run(d: &mut GridDoc, c: G) {
    d.execute(&Command::Grid(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"));
}

fn undo(d: &mut GridDoc) {
    assert!(d.undo(), "nothing to undo");
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

fn put(d: &mut GridDoc, row: u32, col: u32, text: &str) {
    at(d, row, col);
    run(d, G::BeginEdit { text: text.into() });
    run(d, G::CommitEdit { advance: None });
}

fn select(d: &mut GridDoc, r0: u32, c0: u32, r1: u32, c1: u32) {
    run(
        d,
        G::SelectRange {
            range: kiln_protocol::commands::GridRange { r0, c0, r1, c1 },
            add: false,
        },
    );
}

fn state(d: &GridDoc) -> GridState {
    match d.app_query(&Query::Grid(GridQuery::State)).expect("state") {
        QueryResult::Grid(GridQueryResult::State(s)) => *s,
        other => panic!("{other:?}"),
    }
}

fn text(d: &GridDoc, row: u32, col: u32) -> String {
    d.workbook.display_text(d.sid().expect("sheet"), row, col)
}

fn formula(d: &GridDoc, row: u32, col: u32) -> String {
    d.workbook
        .formula(d.sid().expect("sheet"), row, col)
        .map(|f| f.trim_start_matches('=').to_owned())
        .unwrap_or_default()
}

fn ranges(d: &GridDoc) -> Vec<Range> {
    d.view().sel.ranges
}

#[allow(clippy::fn_params_excessive_bools)]
fn drag(
    d: &mut GridDoc,
    phase: GridDragPhase,
    to: (u32, u32),
    grab: (u32, u32),
    copy: bool,
    insert: bool,
) {
    run(
        d,
        G::DragCells {
            phase,
            row: to.0,
            col: to.1,
            grab_row: grab.0,
            grab_col: grab.1,
            copy,
            insert,
            replace: false,
        },
    );
}

#[test]
fn f8_extends_and_shift_f8_adds_ranges() {
    let mut d = doc();
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::SelectionMode {
            mode: GridSelectMode::Extend,
        },
    );
    assert_eq!(state(&d).surface.select_mode, GridSelectMode::Extend);
    // Arrows and a click extend, as with Shift.
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
    assert_eq!(ranges(&d), vec![Range::new(1, 1, 2, 2)]);
    run(
        &mut d,
        G::SelectCell {
            row: 4,
            col: 3,
            extend: false,
            add: false,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(1, 1, 4, 3)]);
    // F8 again ends it.
    run(
        &mut d,
        G::SelectionMode {
            mode: GridSelectMode::Extend,
        },
    );
    assert_eq!(state(&d).surface.select_mode, GridSelectMode::Normal);
    run(
        &mut d,
        G::Move {
            dir: D::Down,
            extend: false,
        },
    );
    assert_eq!(ranges(&d), vec![Range::cell(2, 1)]);

    // Shift+F8: the arrows move a new cell, Shift+arrows grow it, the first range stays.
    select(&mut d, 0, 0, 1, 1);
    run(
        &mut d,
        G::SelectionMode {
            mode: GridSelectMode::Add,
        },
    );
    assert_eq!(state(&d).surface.select_mode, GridSelectMode::Add);
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
            dir: D::Down,
            extend: false,
        },
    );
    assert_eq!(ranges(&d), vec![Range::new(0, 0, 1, 1), Range::cell(2, 0)]);
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: true,
        },
    );
    assert_eq!(
        ranges(&d),
        vec![Range::new(0, 0, 1, 1), Range::new(2, 0, 2, 1)]
    );
    // A click adds another range, as Ctrl+click does.
    run(
        &mut d,
        G::SelectCell {
            row: 6,
            col: 6,
            extend: false,
            add: false,
        },
    );
    assert_eq!(ranges(&d).len(), 3);
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(ranges(&d)[2], Range::cell(6, 7));
    assert!(state(&d).multi);
    // Esc leaves the mode and keeps the selection.
    run(&mut d, G::CancelEdit);
    assert_eq!(state(&d).surface.select_mode, GridSelectMode::Normal);
    assert_eq!(ranges(&d).len(), 3);
    run(
        &mut d,
        G::Move {
            dir: D::Down,
            extend: false,
        },
    );
    assert_eq!(ranges(&d), vec![Range::cell(7, 7)]);
}

#[test]
fn a_dragged_border_moves_cells_and_references_follow() {
    let mut d = doc();
    put(&mut d, 0, 0, "4");
    put(&mut d, 1, 0, "5");
    put(&mut d, 0, 2, "=SUM(A1:A2)");
    select(&mut d, 0, 0, 1, 0);
    // The border hit test names the selection's cell under the point.
    let hw = d.header_w(&d.geometry_ref());
    let hit = d
        .app_query(&Query::Grid(GridQuery::HitTest {
            x: hw + 1.0,
            y: 24.0 + 30.0,
            coarse: false,
        }))
        .expect("hit");
    assert_eq!(
        hit,
        QueryResult::Grid(GridQueryResult::Hit(GridHit::SelectionBorder {
            row: 1,
            col: 0
        }))
    );
    // Grabbed on A2, dropped on E5: the block lands on E4:E5.
    drag(&mut d, GridDragPhase::Preview, (4, 4), (1, 0), false, false);
    let info = state(&d).surface.drag.expect("preview");
    assert_eq!(info.target, "E4:E5");
    assert!(!info.occupied);
    drag(&mut d, GridDragPhase::Drop, (4, 4), (1, 0), false, false);
    assert!(state(&d).surface.drag.is_none());
    assert_eq!(text(&d, 3, 4), "4");
    assert_eq!(text(&d, 4, 4), "5");
    assert_eq!(text(&d, 0, 0), "");
    assert_eq!(formula(&d, 0, 2), "SUM(E4:E5)");
    assert_eq!(text(&d, 0, 2), "9");
    assert_eq!(ranges(&d), vec![Range::new(3, 4, 4, 4)]);
    // One step of undo puts everything back.
    undo(&mut d);
    assert_eq!(text(&d, 0, 0), "4");
    assert_eq!(text(&d, 3, 4), "");
    assert_eq!(formula(&d, 0, 2), "SUM(A1:A2)");
    // Cancel leaves the cells where they are.
    select(&mut d, 0, 0, 1, 0);
    drag(&mut d, GridDragPhase::Preview, (6, 6), (0, 0), false, false);
    drag(&mut d, GridDragPhase::Cancel, (6, 6), (0, 0), false, false);
    assert!(state(&d).surface.drag.is_none());
    assert_eq!(text(&d, 0, 0), "4");
}

#[test]
fn ctrl_drag_copies_and_shift_drag_inserts() {
    let mut d = doc();
    for (r, v) in ["a", "b", "c", "d", "e"].iter().enumerate() {
        put(&mut d, r as u32, 0, v);
    }
    put(&mut d, 0, 1, "=A1&\"!\"");
    // Ctrl: a copy, whose relative references adjust.
    select(&mut d, 0, 1, 0, 1);
    drag(&mut d, GridDragPhase::Drop, (2, 1), (0, 1), true, false);
    assert_eq!(formula(&d, 2, 1), "A3&\"!\"");
    assert_eq!(text(&d, 2, 1), "c!");
    assert_eq!(formula(&d, 0, 1), "A1&\"!\"");
    // Shift: A1:A2 goes in at the gridline between d and e; the cells close up behind it.
    select(&mut d, 0, 0, 1, 0);
    drag(&mut d, GridDragPhase::Preview, (4, 0), (0, 0), false, true);
    let info = state(&d).surface.drag.expect("preview");
    assert_eq!(
        info.insert,
        Some(kiln_protocol::commands::grid_surface::GridDragInsert::Rows)
    );
    drag(&mut d, GridDragPhase::Drop, (4, 0), (0, 0), false, true);
    let col: Vec<String> = (0..5).map(|r| text(&d, r, 0)).collect();
    assert_eq!(col, ["c", "d", "a", "b", "e"]);
    assert_eq!(ranges(&d), vec![Range::new(2, 0, 3, 0)]);
    // The formula followed its cell.
    assert_eq!(text(&d, 0, 1), "a!");
    undo(&mut d);
    let col: Vec<String> = (0..5).map(|r| text(&d, r, 0)).collect();
    assert_eq!(col, ["a", "b", "c", "d", "e"]);
    // Upwards: e goes in above b.
    select(&mut d, 4, 0, 4, 0);
    drag(&mut d, GridDragPhase::Drop, (1, 0), (4, 0), false, true);
    let col: Vec<String> = (0..5).map(|r| text(&d, r, 0)).collect();
    assert_eq!(col, ["a", "e", "b", "c", "d"]);
    // Ctrl+Shift: a copy goes in; the cells below shift down.
    select(&mut d, 0, 0, 0, 0);
    drag(&mut d, GridDragPhase::Drop, (2, 0), (0, 0), true, true);
    let col: Vec<String> = (0..6).map(|r| text(&d, r, 0)).collect();
    assert_eq!(col, ["a", "e", "a", "b", "c", "d"]);
}

#[test]
fn a_drop_on_data_asks_before_replacing() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 3, 0, "keep");
    select(&mut d, 0, 0, 0, 0);
    drag(&mut d, GridDragPhase::Preview, (3, 0), (0, 0), false, false);
    assert!(state(&d).surface.drag.expect("preview").occupied);
    let err = d
        .execute(&Command::Grid(G::DragCells {
            phase: GridDragPhase::Drop,
            row: 3,
            col: 0,
            grab_row: 0,
            grab_col: 0,
            copy: false,
            insert: false,
            replace: false,
        }))
        .expect_err("asks first");
    assert!(
        err.to_string().contains("There's already data here"),
        "{err}"
    );
    assert_eq!(text(&d, 3, 0), "keep");
    run(
        &mut d,
        G::DragCells {
            phase: GridDragPhase::Drop,
            row: 3,
            col: 0,
            grab_row: 0,
            grab_col: 0,
            copy: false,
            insert: false,
            replace: true,
        },
    );
    assert_eq!(text(&d, 3, 0), "1");
    assert_eq!(text(&d, 0, 0), "");
}

#[test]
fn the_error_indicator_names_the_error_and_ignore_error_is_saved() {
    let mut d = doc();
    put(&mut d, 0, 0, "=1/0");
    put(&mut d, 1, 0, "'12");
    at(&mut d, 0, 0);
    let e = state(&d).surface.error.expect("indicator");
    assert_eq!(e.rule, GridErrorRule::ErrorValue);
    assert_eq!(e.error, "#DIV/0!");
    assert_eq!(e.title, "Divide by Zero Error");
    // The button sits left of the cell (right of it in column A) and hit-tests.
    let hw = d.header_w(&d.geometry_ref());
    let hit = d
        .app_query(&Query::Grid(GridQuery::HitTest {
            x: hw + 72.0,
            y: 24.0 + 8.0,
            coarse: false,
        }))
        .expect("hit");
    assert_eq!(
        hit,
        QueryResult::Grid(GridQueryResult::Hit(GridHit::ErrorButton))
    );
    // A number typed after an apostrophe is a number stored as text (Excel's rule).
    at(&mut d, 1, 0);
    let e = state(&d).surface.error.expect("flagged");
    assert_eq!(e.rule, GridErrorRule::NumberAsText);
    // Ignore Error turns the indicator off, is undone in one step and is saved.
    at(&mut d, 0, 0);
    run(&mut d, G::IgnoreError);
    assert!(state(&d).surface.error.is_none());
    undo(&mut d);
    assert!(state(&d).surface.error.is_some());
    run(&mut d, G::IgnoreError);
    let bytes = d.save(kiln_protocol::FileFormat::Xlsx).expect("saved");
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("reopened");
    let back = GridDoc::new(read.workbook, Some(read.source), read.warnings, None);
    let sheet = back.workbook.sheets().first().expect("sheet");
    assert_eq!(sheet.meta().ignored_errors.len(), 1);
    assert!(sheet.meta().ignored_errors[0].eval_error);
    assert_eq!(
        sheet.meta().ignored_errors[0].ranges,
        vec![Range::cell(0, 0)]
    );
    assert!(state(&back).surface.error.is_none());
}

#[test]
fn convert_to_number_turns_text_into_numbers() {
    let mut d = doc();
    // Text-formatted cells keep typed digits as text (Excel's "Number Stored as Text" is then
    // not flagged); a number pasted as text is.
    let sid = d.sid().expect("sheet");
    d.apply_for_test(&kiln_sheet::Command::SetCells {
        sheet: sid,
        cells: vec![
            kiln_sheet::commands::CellInput::value(0, 0, CellValue::Text("42".into())),
            kiln_sheet::commands::CellInput::value(1, 0, CellValue::Text("7.5".into())),
        ],
    })
    .expect("set");
    at(&mut d, 0, 0);
    let e = state(&d).surface.error.expect("flagged");
    assert_eq!(e.rule, GridErrorRule::NumberAsText);
    assert_eq!(e.title, "Number Stored as Text");
    select(&mut d, 0, 0, 1, 0);
    run(&mut d, G::ConvertToNumber);
    assert_eq!(d.workbook.value(sid, 0, 0), CellValue::Number(42.0));
    assert_eq!(d.workbook.value(sid, 1, 0), CellValue::Number(7.5));
    assert!(state(&d).surface.error.is_none());
}

#[test]
fn the_context_menu_filters_and_sorts_by_the_active_cell() {
    let mut d = doc();
    for (r, (a, b)) in [
        ("Item", "Qty"),
        ("pot", "3"),
        ("jug", "5"),
        ("pot", "7"),
        ("cup", "1"),
    ]
    .iter()
    .enumerate()
    {
        put(&mut d, r as u32, 0, a);
        put(&mut d, r as u32, 1, b);
    }
    // No AutoFilter yet: one is added over the list, and "pot" rows stay.
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::FilterBySelection {
            by: kiln_protocol::commands::grid_surface::GridSelectionKey::Value,
        },
    );
    let sid = d.sid().expect("sheet");
    let sheet = d.workbook.sheet(sid).expect("sheet");
    assert!(sheet.meta().auto_filter.is_some());
    let hidden: Vec<bool> = (1..5).map(|r| sheet.is_row_filtered(r)).collect();
    assert_eq!(hidden, [false, true, false, true]);
    run(&mut d, G::ClearFilter);
    // Put Selected Cell Color On Top: the yellow row goes first under the headings.
    at(&mut d, 3, 0);
    run(
        &mut d,
        G::ApplyFormat {
            format: Box::new(kiln_protocol::commands::GridFormat {
                fill_color: Some("FFFF00".into()),
                ..kiln_protocol::commands::GridFormat::default()
            }),
        },
    );
    run(&mut d, G::SortBySelectionColor { font: false });
    assert_eq!(text(&d, 1, 1), "7");
    assert_eq!(text(&d, 0, 0), "Item");
}

#[test]
fn page_layout_finds_the_cells_of_each_printed_page() {
    let mut d = doc();
    for r in 0..120u32 {
        put(&mut d, r, 0, &format!("row {r}"));
    }
    put(&mut d, 0, 1, "B");
    let map = |d: &GridDoc, page: u32| match d
        .app_query(&Query::Grid(GridQuery::PageMap { page }))
        .expect("page map")
    {
        QueryResult::Grid(GridQueryResult::PageMap(m)) => m,
        other => panic!("{other:?}"),
    };
    let first = map(&d, 0).expect("page 1");
    assert_eq!(first.bands.len(), 1, "no print titles");
    let body = &first.bands[0];
    // Letter paper at 96 px an inch; the cells start at the left and top margins.
    assert!((first.width - 816.0).abs() < 1.0, "{}", first.width);
    assert!(body.x > 60.0 && body.y > 60.0, "{body:?}");
    assert_eq!(body.rows[0].index, 0);
    let last = body.rows.last().expect("rows").index;
    // Page 1 ends the data across: the empty columns that fit on it can be clicked too.
    assert_eq!(
        body.cols[..2].iter().map(|c| c.index).collect::<Vec<_>>(),
        [0, 1]
    );
    assert!(body.cols.len() > 5, "{}", body.cols.len());
    let right = body
        .cols
        .last()
        .map(|c| body.x + c.at + c.size)
        .unwrap_or_default();
    assert!(right <= first.width - 60.0, "{right}");
    // The next page starts where this one ends, as File ▸ Print paginates.
    let second = map(&d, 1).expect("page 2");
    assert_eq!(second.bands[0].rows[0].index, last + 1);
    let pages = kiln_sheet_layout::print::paginate(&d.workbook, 0).expect("pages");
    assert_eq!(pages.pages[0].rows.1, last);
    assert!(map(&d, pages.pages.len() as u32).is_none());
    // An empty sheet still has a page whose cells can be typed into.
    let empty = doc();
    let m = map(&empty, 0).expect("a blank page");
    assert!(m.bands[0].rows.len() > 30 && m.bands[0].cols.len() > 5);
}
