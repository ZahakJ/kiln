// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Home tab's completions, one behaviour per test: Paste Special's comments,
//! validation and column widths and Paste Link, Fill ▸ Series, Auto Fill Options, custom lists,
//! the border pen, Custom number formats and rich text typed in the cell editor.

use kiln_protocol::commands::grid_features::{GridValidation, GridValidationKind};
use kiln_protocol::commands::grid_home::{
    GridAutoFillChoice, GridDateUnit, GridPenMode, GridSeries, GridSeriesType,
};
use kiln_protocol::commands::{
    EditCommand, GridCommand as G, GridFill, GridLineStyle, GridPaste, GridPasteOp, GridQuery,
    GridQueryResult, GridRange,
};
use kiln_protocol::{Command, Query, QueryResult, SuiteCommand};
use kiln_sheet::styles::BorderStyle;
use kiln_sheet::{CellValue, Workbook};

use super::GridDoc;
use crate::model::DocModel;

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

fn run(d: &mut GridDoc, c: G) {
    d.execute(&Command::Grid(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"));
}

fn ed(d: &mut GridDoc, e: EditCommand) {
    d.execute(&Command::Suite(SuiteCommand::Edit(e.clone())))
        .unwrap_or_else(|err| panic!("{e:?}: {err}"));
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

fn sel(d: &mut GridDoc, r0: u32, c0: u32, r1: u32, c1: u32) {
    run(
        d,
        G::SelectRange {
            range: GridRange { r0, c0, r1, c1 },
            add: false,
        },
    );
}

fn put(d: &mut GridDoc, row: u32, col: u32, text: &str) {
    at(d, row, col);
    run(d, G::BeginEdit { text: text.into() });
    run(d, G::CommitEdit { advance: None });
}

fn sid(d: &GridDoc) -> kiln_sheet::SheetId {
    d.sid().expect("sheet")
}

fn num(d: &GridDoc, row: u32, col: u32) -> f64 {
    d.workbook
        .value(sid(d), row, col)
        .as_number()
        .unwrap_or_else(|| panic!("no number at {row},{col}"))
}

fn text(d: &GridDoc, row: u32, col: u32) -> String {
    d.workbook.value(sid(d), row, col).display_general()
}

fn state(d: &GridDoc) -> kiln_protocol::commands::GridState {
    match d.app_query(&Query::Grid(GridQuery::State)) {
        Ok(QueryResult::Grid(GridQueryResult::State(s))) => *s,
        other => panic!("{other:?}"),
    }
}

fn special(d: &mut GridDoc, what: GridPaste) {
    run(
        d,
        G::PasteSpecial {
            what,
            op: GridPasteOp::None,
            skip_blanks: false,
            transpose: false,
            data: None,
        },
    );
}

#[test]
fn paste_special_pastes_comments_validation_column_widths_and_links() {
    let mut d = doc();
    put(&mut d, 0, 0, "5");
    put(&mut d, 1, 0, "7");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::SetNote {
            text: Some("Checked".into()),
            author: "Ana".into(),
            cell: None,
        },
    );
    run(
        &mut d,
        G::AddComment {
            text: "Is this right?".into(),
            author: "Ana".into(),
            reply_to: None,
            now_ms: 1_760_000_000_000,
        },
    );
    sel(&mut d, 0, 0, 1, 0);
    run(
        &mut d,
        G::SetValidation {
            rule: Some(GridValidation {
                kind: GridValidationKind::Whole,
                formula1: "1".into(),
                formula2: "10".into(),
                op: kiln_protocol::commands::grid_features::GridCompare::Between,
                show_error: true,
                ..GridValidation::default()
            }),
        },
    );
    run(&mut d, G::ColumnWidth { chars: 30.0 });
    ed(&mut d, EditCommand::Copy);

    // Comments: the note and the thread, no values.
    at(&mut d, 0, 2);
    special(&mut d, GridPaste::Comments);
    let s = d.workbook.sheet(sid(&d)).expect("sheet");
    assert!(
        d.workbook.value(sid(&d), 0, 2).is_blank(),
        "comments paste no values"
    );
    assert!(
        s.meta()
            .notes
            .iter()
            .any(|n| n.at.row == 0 && n.at.col == 2)
    );
    assert!(
        s.meta()
            .threads
            .iter()
            .any(|t| t.at.row == 0 && t.at.col == 2 && t.text == "Is this right?")
    );
    let ids: Vec<&str> = s.meta().threads.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1], "the copy has its own id");

    // Validation: the rule, no values or notes.
    at(&mut d, 0, 3);
    special(&mut d, GridPaste::Validation);
    let s = d.workbook.sheet(sid(&d)).expect("sheet");
    assert!(d.workbook.value(sid(&d), 0, 3).is_blank());
    assert!(s.validation_at(1, 3).is_some(), "D2 is validated");
    assert!(s.validation_at(2, 3).is_none());
    assert!(!s.meta().notes.iter().any(|n| n.at.col == 3));

    // Column widths: column E takes A's width.
    at(&mut d, 0, 4);
    special(&mut d, GridPaste::ColumnWidths);
    let s = d.workbook.sheet(sid(&d)).expect("sheet");
    let w = |c: u32| s.cols().get(c).and_then(|p| p.width).map(|w| w.0);
    assert_eq!(w(4), w(0));
    assert!(d.workbook.value(sid(&d), 0, 4).is_blank());

    // Formulas and number formats.
    special(&mut d, GridPaste::FormulasAndNumberFormats);
    assert_eq!(num(&d, 1, 4), 7.0);

    // Undo takes a paste back as one step.
    assert!(d.undo());
    assert!(d.workbook.value(sid(&d), 0, 4).is_blank());

    // Paste Link: one cell refers absolutely, a range relatively.
    at(&mut d, 0, 0);
    ed(&mut d, EditCommand::Copy);
    at(&mut d, 5, 5);
    run(&mut d, G::PasteLink { data: None });
    assert_eq!(d.workbook.formula(sid(&d), 5, 5).as_deref(), Some("$A$1"));
    sel(&mut d, 0, 0, 1, 0);
    ed(&mut d, EditCommand::Copy);
    at(&mut d, 6, 6);
    run(&mut d, G::PasteLink { data: None });
    assert_eq!(d.workbook.formula(sid(&d), 7, 6).as_deref(), Some("A2"));
    assert_eq!(num(&d, 7, 6), 7.0);
    put(&mut d, 1, 0, "9");
    assert_eq!(num(&d, 7, 6), 9.0, "the link follows its source");
}

fn series(d: &mut GridDoc, s: GridSeries) {
    run(d, G::FillSeries { series: s });
}

#[test]
fn fill_series_steps_grows_stops_and_counts_dates() {
    let mut d = doc();
    // Linear, step 3, down a column.
    put(&mut d, 0, 0, "1");
    sel(&mut d, 0, 0, 5, 0);
    series(
        &mut d,
        GridSeries {
            columns: None,
            kind: GridSeriesType::Linear,
            unit: GridDateUnit::Day,
            step: 3.0,
            stop: None,
            trend: false,
        },
    );
    assert_eq!(
        (0..6).map(|r| num(&d, r, 0)).collect::<Vec<_>>(),
        [1.0, 4.0, 7.0, 10.0, 13.0, 16.0]
    );
    // Growth with a stop value leaves the cells past it alone.
    put(&mut d, 0, 1, "2");
    sel(&mut d, 0, 1, 5, 1);
    series(
        &mut d,
        GridSeries {
            columns: Some(true),
            kind: GridSeriesType::Growth,
            unit: GridDateUnit::Day,
            step: 2.0,
            stop: Some(20.0),
            trend: false,
        },
    );
    assert_eq!(
        (0..4).map(|r| num(&d, r, 1)).collect::<Vec<_>>(),
        [2.0, 4.0, 8.0, 16.0]
    );
    assert!(d.workbook.value(sid(&d), 4, 1).is_blank());
    // From one cell with a stop value the series runs on to it (along the row).
    put(&mut d, 10, 0, "5");
    at(&mut d, 10, 0);
    series(
        &mut d,
        GridSeries {
            columns: Some(false),
            kind: GridSeriesType::Linear,
            unit: GridDateUnit::Day,
            step: 5.0,
            stop: Some(25.0),
            trend: false,
        },
    );
    assert_eq!(
        (0..5).map(|c| num(&d, 10, c)).collect::<Vec<_>>(),
        [5.0, 10.0, 15.0, 20.0, 25.0]
    );
    assert!(d.workbook.value(sid(&d), 10, 5).is_blank());
    // Dates by month keep the day and the date format.
    put(&mut d, 0, 3, "2026-01-31");
    sel(&mut d, 0, 3, 3, 3);
    series(
        &mut d,
        GridSeries {
            columns: Some(true),
            kind: GridSeriesType::Date,
            unit: GridDateUnit::Month,
            step: 1.0,
            stop: None,
            trend: false,
        },
    );
    assert_eq!(text_of(&d, 1, 3), "2/28/2026");
    assert_eq!(text_of(&d, 3, 3), "4/30/2026");
    // Weekdays skip Saturday and Sunday (2026-01-02 is a Friday).
    put(&mut d, 0, 4, "2026-01-02");
    sel(&mut d, 0, 4, 2, 4);
    series(
        &mut d,
        GridSeries {
            columns: Some(true),
            kind: GridSeriesType::Date,
            unit: GridDateUnit::Weekday,
            step: 1.0,
            stop: None,
            trend: false,
        },
    );
    assert_eq!(text_of(&d, 1, 4), "1/5/2026");
    // Trend: the best-fit line of the first values replaces them and continues.
    put(&mut d, 0, 6, "1");
    put(&mut d, 1, 6, "3");
    put(&mut d, 2, 6, "4");
    sel(&mut d, 0, 6, 4, 6);
    series(
        &mut d,
        GridSeries {
            columns: Some(true),
            kind: GridSeriesType::Linear,
            unit: GridDateUnit::Day,
            step: 1.0,
            stop: None,
            trend: true,
        },
    );
    let got: Vec<f64> = (0..5)
        .map(|r| (num(&d, r, 6) * 1000.0).round() / 1000.0)
        .collect();
    assert_eq!(got, [1.167, 2.667, 4.167, 5.667, 7.167]);
    // AutoFill continues a list like the fill handle.
    put(&mut d, 0, 8, "Mon");
    sel(&mut d, 0, 8, 2, 8);
    series(
        &mut d,
        GridSeries {
            columns: Some(true),
            kind: GridSeriesType::AutoFill,
            unit: GridDateUnit::Day,
            step: 1.0,
            stop: None,
            trend: false,
        },
    );
    assert_eq!(text(&d, 2, 8), "Wed");
    // One undo step per series.
    assert!(d.undo());
    assert!(d.workbook.value(sid(&d), 2, 8).is_blank());
}

/// A cell as Excel shows it.
fn text_of(d: &GridDoc, row: u32, col: u32) -> String {
    let style = d.workbook.sheet(sid(d)).map_or(0, |s| s.style_at(row, col));
    let code = d.workbook.styles().num_fmt(style).to_owned();
    kiln_sheet::format::format_value(&d.workbook.value(sid(d), row, col), &code, false).text
}

#[test]
fn custom_lists_continue_and_auto_fill_options_redo_the_fill() {
    let mut d = doc();
    run(
        &mut d,
        G::SetCustomLists {
            lists: vec![vec![
                "North".into(),
                "East".into(),
                "South".into(),
                "West".into(),
            ]],
        },
    );
    put(&mut d, 0, 0, "East");
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::AutoFill {
            row: 3,
            col: 0,
            mode: GridFill::Auto,
        },
    );
    assert_eq!(text(&d, 1, 0), "South");
    assert_eq!(text(&d, 3, 0), "North");
    // The button offers the choices and redoes the fill.
    let s = state(&d);
    let opts = s.home.auto_fill.expect("Auto Fill Options");
    assert!(
        opts.choices
            .contains(&GridAutoFillChoice::Fill(GridFill::Copy))
    );
    assert!(opts.choices.contains(&GridAutoFillChoice::FlashFill));
    assert_eq!(opts.current, GridAutoFillChoice::Fill(GridFill::Series));
    run(
        &mut d,
        G::AutoFillOption {
            choice: GridAutoFillChoice::Fill(GridFill::Copy),
        },
    );
    assert_eq!(text(&d, 3, 0), "East");
    assert_eq!(
        state(&d).home.auto_fill.map(|o| o.current),
        Some(GridAutoFillChoice::Fill(GridFill::Copy))
    );
    // Fill Formatting Only leaves the cells as they were before the fill.
    at(&mut d, 0, 0);
    ed(&mut d, EditCommand::Bold);
    run(
        &mut d,
        G::AutoFill {
            row: 2,
            col: 0,
            mode: GridFill::Auto,
        },
    );
    run(
        &mut d,
        G::AutoFillOption {
            choice: GridAutoFillChoice::Fill(GridFill::FormatsOnly),
        },
    );
    let style = d.workbook.sheet(sid(&d)).map_or(0, |s| s.style_at(2, 0));
    assert!(d.workbook.styles().get(style).font.bold);
    assert_eq!(text(&d, 2, 0), "East", "the earlier copy stays");
    // Dates offer the date units; another edit hides the button.
    put(&mut d, 0, 2, "2026-03-01");
    at(&mut d, 0, 2);
    run(
        &mut d,
        G::AutoFill {
            row: 2,
            col: 2,
            mode: GridFill::Auto,
        },
    );
    let opts = state(&d).home.auto_fill.expect("button");
    assert!(
        opts.choices
            .contains(&GridAutoFillChoice::Fill(GridFill::Months))
    );
    run(
        &mut d,
        G::AutoFillOption {
            choice: GridAutoFillChoice::Fill(GridFill::Months),
        },
    );
    assert_eq!(text_of(&d, 2, 2), "5/1/2026");
    put(&mut d, 9, 9, "x");
    assert!(state(&d).home.auto_fill.is_none());
    // Import takes the selection's text as a list; the query lists the user's lists.
    put(&mut d, 0, 5, "Clay");
    put(&mut d, 1, 5, "Glaze");
    put(&mut d, 2, 5, "Kiln");
    sel(&mut d, 0, 5, 2, 5);
    run(&mut d, G::ImportCustomList);
    let Ok(QueryResult::Grid(GridQueryResult::Lists(lists))) =
        d.app_query(&Query::Grid(GridQuery::CustomLists))
    else {
        panic!("lists")
    };
    assert_eq!(lists.len(), 2);
    assert_eq!(lists[1], ["Clay", "Glaze", "Kiln"]);
    put(&mut d, 5, 5, "Glaze");
    at(&mut d, 5, 5);
    run(
        &mut d,
        G::AutoFill {
            row: 6,
            col: 5,
            mode: GridFill::Auto,
        },
    );
    assert_eq!(text(&d, 6, 5), "Kiln");
    assert!(
        d.execute(&Command::Grid(G::SetCustomLists {
            lists: vec![vec!["only".into()]]
        }))
        .is_err()
    );
}

fn pen(d: &mut GridDoc, mode: GridPenMode, style: GridLineStyle) {
    run(
        d,
        G::BorderPen {
            mode: Some(mode),
            style: Some(style),
            color: Some("C8352B".into()),
        },
    );
}

/// The surface point of a sheet position (sheet pixels inside the scrolling pane).
fn surface(d: &mut GridDoc, row: u32, col: u32, fx: f32, fy: f32) -> (f32, f32) {
    let geo = d.geometry().clone();
    let r = geo.cell_rect(row, col);
    let hw = d.header_w(&geo);
    geo.to_surface(&d.last_view, r.x + r.w * fx, r.y + r.h * fy, hw)
}

fn stroke(d: &mut GridDoc, a: (f32, f32), b: (f32, f32)) {
    run(
        d,
        G::DrawBorder {
            x0: a.0,
            y0: a.1,
            x1: b.0,
            y1: b.1,
            done: false,
        },
    );
    run(
        d,
        G::DrawBorder {
            x0: a.0,
            y0: a.1,
            x1: b.0,
            y1: b.1,
            done: true,
        },
    );
}

fn st(s: Option<kiln_sheet::styles::BorderSide>) -> BorderStyle {
    s.map_or(BorderStyle::None, |s| s.style)
}

fn border(d: &GridDoc, row: u32, col: u32) -> kiln_sheet::styles::Border {
    let s = d.workbook.sheet(sid(d)).map_or(0, |s| s.style_at(row, col));
    d.workbook.styles().get(s).border
}

#[test]
fn the_border_pen_draws_outlines_grids_edges_and_erases() {
    let mut d = doc();
    // Draw Border: a drag over B2:C3 draws its outline only.
    pen(&mut d, GridPenMode::Draw, GridLineStyle::Medium);
    assert!(state(&d).home.pen.is_some());
    let a = surface(&mut d, 1, 1, 0.5, 0.5);
    let b = surface(&mut d, 2, 2, 0.5, 0.5);
    stroke(&mut d, a, b);
    assert_eq!(st(border(&d, 1, 1).top), BorderStyle::Medium);
    assert_eq!(st(border(&d, 1, 1).left), BorderStyle::Medium);
    assert_eq!(st(border(&d, 2, 2).bottom), BorderStyle::Medium);
    assert_eq!(
        st(border(&d, 1, 1).bottom),
        BorderStyle::None,
        "no inside lines"
    );
    assert!(d.undo());
    assert_eq!(st(border(&d, 1, 1).top), BorderStyle::None, "one undo step");
    // Draw Border Grid: every edge.
    pen(&mut d, GridPenMode::Grid, GridLineStyle::Thin);
    stroke(&mut d, a, b);
    assert_eq!(st(border(&d, 1, 1).bottom), BorderStyle::Thin);
    assert_eq!(st(border(&d, 1, 1).right), BorderStyle::Thin);
    // A click on a gridline draws that edge alone.
    pen(&mut d, GridPenMode::Draw, GridLineStyle::Thick);
    let e = surface(&mut d, 5, 5, 0.5, 0.98);
    stroke(&mut d, e, e);
    assert_eq!(st(border(&d, 5, 5).bottom), BorderStyle::Thick);
    assert_eq!(st(border(&d, 5, 5).top), BorderStyle::None);
    // Erase Border: a click erases one edge (and the neighbour's side on that line)…
    pen(&mut d, GridPenMode::Erase, GridLineStyle::Thin);
    let e = surface(&mut d, 1, 1, 0.5, 0.98);
    stroke(&mut d, e, e);
    assert_eq!(st(border(&d, 1, 1).bottom), BorderStyle::None);
    assert_eq!(st(border(&d, 2, 1).top), BorderStyle::None);
    assert_eq!(st(border(&d, 1, 1).right), BorderStyle::Thin);
    // …and a drag every edge of its cells.
    stroke(&mut d, a, b);
    assert_eq!(st(border(&d, 1, 1).right), BorderStyle::None);
    assert_eq!(st(border(&d, 2, 2).bottom), BorderStyle::None);
    // Line Style alone keeps the mode and takes the style.
    run(
        &mut d,
        G::BorderPen {
            mode: None,
            style: Some(GridLineStyle::Double),
            color: None,
        },
    );
    let p = state(&d).home.pen.expect("pen");
    assert_eq!(
        (p.mode, p.style),
        (GridPenMode::Erase, GridLineStyle::Double)
    );
    assert_eq!(p.color.as_deref(), Some("C8352B"));
    run(&mut d, G::BorderPenOff);
    assert!(state(&d).home.pen.is_none());
    assert!(
        d.execute(&Command::Grid(G::DrawBorder {
            x0: 0.0,
            y0: 0.0,
            x1: 1.0,
            y1: 1.0,
            done: true
        }))
        .is_err()
    );
}

#[test]
fn custom_number_formats_are_listed_and_deleted() {
    let mut d = doc();
    put(&mut d, 0, 0, "12.5");
    run(
        &mut d,
        G::NumberFormat {
            format: "0.0\" kg\"".into(),
        },
    );
    let Ok(QueryResult::Grid(GridQueryResult::FormatCells(f))) =
        d.app_query(&Query::Grid(GridQuery::FormatCells))
    else {
        panic!("format cells")
    };
    assert!(f.custom_codes.iter().any(|c| c == "0.0\" kg\""));
    assert!(
        !f.custom_codes.iter().any(|c| c == "0.00"),
        "built-in codes are not the workbook's"
    );
    assert!(
        d.execute(&Command::Grid(G::DeleteNumberFormat {
            code: "0.00".into()
        }))
        .is_err()
    );
    run(
        &mut d,
        G::DeleteNumberFormat {
            code: "0.0\" kg\"".into(),
        },
    );
    let style = d.workbook.sheet(sid(&d)).map_or(0, |s| s.style_at(0, 0));
    assert_eq!(d.workbook.styles().num_fmt(style), "General");
    let Ok(QueryResult::Grid(GridQueryResult::FormatCells(f))) =
        d.app_query(&Query::Grid(GridQuery::FormatCells))
    else {
        panic!("format cells")
    };
    assert!(f.custom_codes.is_empty());
}

#[test]
fn formatting_selected_characters_in_the_editor_makes_rich_text() {
    let mut d = doc();
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::BeginEdit {
            text: "Kiln Grid".into(),
        },
    );
    // Select "Grid" (UTF-16 offsets 5..9) and make it bold and red.
    run(
        &mut d,
        G::SetEditText {
            text: "Kiln Grid".into(),
            start: 5,
            end: 9,
            formula_bar: false,
        },
    );
    ed(&mut d, EditCommand::Bold);
    ed(
        &mut d,
        EditCommand::FontColor {
            color: Some("C8352B".into()),
        },
    );
    assert!(
        d.edit.is_some(),
        "formatting characters keeps the entry open"
    );
    // Typing after the bold part continues it.
    run(
        &mut d,
        G::SetEditText {
            text: "Kiln Grids".into(),
            start: 10,
            end: 10,
            formula_bar: false,
        },
    );
    run(&mut d, G::CommitEdit { advance: None });
    let CellValue::RichText(r) = d.workbook.value(sid(&d), 0, 0) else {
        panic!("rich text expected")
    };
    let runs = r.runs();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "Kiln ");
    assert_eq!(runs[0].font, None);
    assert_eq!(runs[1].text, "Grids");
    let f = runs[1].font.as_ref().expect("font");
    assert_eq!(f.bold, Some(true));
    assert!(f.color.is_some());
    // The cell itself is not bold.
    let style = d.workbook.sheet(sid(&d)).map_or(0, |s| s.style_at(0, 0));
    assert!(!d.workbook.styles().get(style).font.bold);
    // F2 keeps the runs; italic on "Kiln" adds a third run.
    at(&mut d, 0, 0);
    run(&mut d, G::EditCell);
    run(
        &mut d,
        G::SetEditText {
            text: "Kiln Grids".into(),
            start: 0,
            end: 4,
            formula_bar: false,
        },
    );
    ed(&mut d, EditCommand::Italic);
    run(&mut d, G::CommitEdit { advance: None });
    let CellValue::RichText(r) = d.workbook.value(sid(&d), 0, 0) else {
        panic!("rich text expected")
    };
    assert_eq!(r.runs().len(), 3);
    assert_eq!(r.runs()[0].font.as_ref().and_then(|f| f.italic), Some(true));
    // A number typed with formatted characters stays a number.
    at(&mut d, 1, 0);
    run(&mut d, G::BeginEdit { text: "123".into() });
    run(
        &mut d,
        G::SetEditText {
            text: "123".into(),
            start: 0,
            end: 1,
            formula_bar: false,
        },
    );
    ed(&mut d, EditCommand::Bold);
    run(&mut d, G::CommitEdit { advance: None });
    assert_eq!(num(&d, 1, 0), 123.0);
    // Without a selection Bold formats the cell, as before.
    at(&mut d, 2, 0);
    run(
        &mut d,
        G::BeginEdit {
            text: "plain".into(),
        },
    );
    ed(&mut d, EditCommand::Bold);
    let style = d.workbook.sheet(sid(&d)).map_or(0, |s| s.style_at(2, 0));
    assert!(d.workbook.styles().get(style).font.bold);
}

/// Series over whole rows or columns fills as far as the data reaches, as Excel does: 20 whole
/// rows with a start value once filled the 16,384 cells of its row. Rows that start empty are
/// left alone, without a notice.
#[test]
fn fill_series_over_whole_rows_stops_at_the_data() {
    use kiln_sheet::{MAX_COLS, MAX_ROWS};
    let stored = |d: &GridDoc| {
        d.workbook.sheet(sid(d)).map_or(0, |s| {
            s.cells()
                .iter_range(kiln_sheet::Range::new(0, 0, MAX_ROWS - 1, MAX_COLS - 1))
                .count()
        })
    };
    let linear = GridSeries {
        columns: None,
        kind: GridSeriesType::Linear,
        unit: GridDateUnit::Day,
        step: 1.0,
        stop: None,
        trend: false,
    };
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 2, "x");
    sel(&mut d, 0, 0, 19, MAX_COLS - 1);
    let t = std::time::Instant::now();
    series(&mut d, linear.clone());
    let took = t.elapsed();
    assert!(stored(&d) <= 4, "{} cells stored", stored(&d));
    assert!(took < std::time::Duration::from_millis(200), "{took:?}");
    assert_eq!([num(&d, 0, 1), num(&d, 0, 2)], [2.0, 3.0]);
    assert_eq!(d.workbook.value(sid(&d), 0, 3), CellValue::Blank);
    // Whole rows that start empty: nothing changes, and no error.
    let mut e = doc();
    put(&mut e, 0, 0, "1");
    sel(&mut e, 5, 0, 9, MAX_COLS - 1);
    series(&mut e, linear);
    assert_eq!(stored(&e), 1);
}

/// The same with the whole sheet selected (Ctrl+A twice): bounded by the used range.
#[test]
fn fill_series_over_the_whole_sheet_stops_at_the_data() {
    use kiln_sheet::{MAX_COLS, MAX_ROWS};
    for columns in [None, Some(true), Some(false)] {
        let mut d = doc();
        put(&mut d, 0, 0, "1");
        put(&mut d, 0, 1, "5");
        put(&mut d, 3, 1, "x");
        sel(&mut d, 0, 0, MAX_ROWS - 1, MAX_COLS - 1);
        let t = std::time::Instant::now();
        series(
            &mut d,
            GridSeries {
                columns,
                kind: GridSeriesType::Linear,
                unit: GridDateUnit::Day,
                step: 1.0,
                stop: None,
                trend: false,
            },
        );
        let took = t.elapsed();
        let n = d.workbook.sheet(sid(&d)).map_or(0, |s| {
            s.cells()
                .iter_range(kiln_sheet::Range::new(0, 0, MAX_ROWS - 1, MAX_COLS - 1))
                .count()
        });
        assert!(n <= 8, "{columns:?}: {n} cells stored");
        assert!(
            took < std::time::Duration::from_secs(3),
            "{columns:?}: {took:?}"
        );
    }
}

/// Fill Series with a stop value over whole rows: each lane fills on to its stop value, as far as
/// the selection reaches, also past the used range; data only in A1 fills A1:J1.
#[test]
fn fill_series_with_a_stop_value_over_whole_rows_reaches_the_stop() {
    use kiln_sheet::MAX_COLS;
    let to_ten = GridSeries {
        columns: None,
        kind: GridSeriesType::Linear,
        unit: GridDateUnit::Day,
        step: 1.0,
        stop: Some(10.0),
        trend: false,
    };
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    sel(&mut d, 0, 0, 0, MAX_COLS - 1);
    series(&mut d, to_ten.clone());
    assert_eq!([num(&d, 0, 1), num(&d, 0, 9)], [2.0, 10.0]);
    assert_eq!(d.workbook.value(sid(&d), 0, 10), CellValue::Blank);
    // The used range reaches column C only: the series still goes on to 10.
    let mut e = doc();
    put(&mut e, 0, 0, "1");
    put(&mut e, 3, 2, "x");
    sel(&mut e, 0, 0, 1, MAX_COLS - 1);
    series(&mut e, to_ten);
    assert_eq!([num(&e, 0, 2), num(&e, 0, 9)], [3.0, 10.0]);
    assert_eq!(e.workbook.value(sid(&e), 0, 10), CellValue::Blank);
    assert_eq!(e.workbook.value(sid(&e), 1, 1), CellValue::Blank);
}
