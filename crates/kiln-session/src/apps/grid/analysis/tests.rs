// SPDX-License-Identifier: AGPL-3.0-or-later
//! The features through the Grid document: PivotTables, sparklines, Goal Seek, Subtotal,
//! Flash Fill, protection and themes, with undo and the state the chrome shows.

use kiln_protocol::commands::grid_analysis::*;
use kiln_protocol::commands::{GridCommand as G, GridHit, GridQuery, GridQueryResult, GridRange};
use kiln_protocol::{Command, Query, QueryResult};
use kiln_sheet::{CellValue, Workbook};

use super::super::GridDoc;
use crate::model::DocModel;

fn doc() -> GridDoc {
    let mut d = GridDoc::new(Workbook::new(), None, Vec::new(), Some("Sales.xlsx".into()));
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

fn fails(d: &mut GridDoc, c: G) -> String {
    d.execute(&Command::Grid(c))
        .expect_err("should fail")
        .to_string()
}

fn q(d: &GridDoc, query: GridQuery) -> GridQueryResult {
    match d.app_query(&Query::Grid(query.clone())) {
        Ok(QueryResult::Grid(r)) => r,
        other => panic!("{query:?}: {other:?}"),
    }
}

fn state(d: &GridDoc) -> GridAnalysisState {
    match q(d, GridQuery::State) {
        GridQueryResult::State(s) => s.analysis,
        other => panic!("{other:?}"),
    }
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

fn select(d: &mut GridDoc, r0: u32, c0: u32, r1: u32, c1: u32) {
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

fn text(d: &GridDoc, row: u32, col: u32) -> String {
    let sid = d.sid().expect("sheet");
    d.workbook.display_text(sid, row, col)
}

fn num(d: &GridDoc, row: u32, col: u32) -> f64 {
    let sid = d.sid().expect("sheet");
    d.workbook
        .value(sid, row, col)
        .as_number()
        .unwrap_or(f64::NAN)
}

/// Region, Product, Amount at A1:C7 of the active sheet.
fn sales(d: &mut GridDoc) {
    let rows = [
        ["Region", "Product", "Amount"],
        ["East", "Apple", "10"],
        ["West", "Banana", "5"],
        ["East", "Banana", "20"],
        ["West", "Apple", "7"],
        ["East", "Apple", "1"],
    ];
    let sid = d.sid().expect("sheet");
    let values = rows
        .iter()
        .map(|r| {
            r.iter()
                .map(|v| {
                    Some(
                        v.parse::<f64>()
                            .map_or_else(|_| CellValue::text(*v), CellValue::Number),
                    )
                })
                .collect()
        })
        .collect();
    d.apply_for_test(&kiln_sheet::Command::SetRange {
        sheet: sid,
        row: 0,
        col: 0,
        values,
    })
    .expect("data");
    d.absorb_changes();
}

fn place(d: &mut GridDoc, field: u32, area: GridPivotArea) {
    run(
        d,
        G::Pivot {
            edit: GridPivotEdit::Place {
                field: Some(field),
                area,
                position: None,
            },
        },
    );
}

#[test]
fn a_pivot_table_on_a_new_sheet_with_fields_refresh_and_undo() {
    let mut d = doc();
    sales(&mut d);
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::CreatePivot {
            source: "Sheet1!$A$1:$C$6".into(),
            to: None,
        },
    );
    // A new sheet before the data, with the report's top-left selected and the prompt.
    assert_eq!(d.workbook.sheets().len(), 2);
    assert_eq!(d.active, 0);
    assert_eq!(d.view().sel.active, kiln_sheet::CellPos::new(2, 0));
    let s = state(&d);
    let p = s.pivot.expect("the PivotTable at the active cell");
    assert_eq!(p.name, "PivotTable1");
    assert_eq!(p.source, "Sheet1!$A$1:$C$6");
    assert_eq!(
        p.fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(),
        ["Region", "Product", "Amount"]
    );
    assert!(p.fields[2].numeric && !p.fields[0].numeric);
    assert_eq!(
        s.placeholder.as_ref().map(|p| p.0.as_str()),
        Some("PivotTable1")
    );
    // Region to Rows, Amount to Values.
    place(&mut d, 0, GridPivotArea::Rows);
    place(&mut d, 2, GridPivotArea::Values);
    assert_eq!(text(&d, 2, 0), "Row Labels");
    assert_eq!(text(&d, 2, 1), "Sum of Amount");
    assert_eq!((text(&d, 3, 0), num(&d, 3, 1)), ("East".into(), 31.0));
    assert_eq!((text(&d, 4, 0), num(&d, 4, 1)), ("West".into(), 12.0));
    assert_eq!(
        (text(&d, 5, 0), num(&d, 5, 1)),
        ("Grand Total".into(), 43.0)
    );
    assert!(state(&d).placeholder.is_none());
    // The style shades the header.
    let sid = d.sid().expect("sheet");
    assert!(
        d.workbook
            .pivot_format(sid, 2, 0)
            .and_then(|x| x.fill)
            .is_some()
    );
    // The source changes; Refresh brings it in.
    let data = d.workbook.sheets()[1].id();
    d.apply_for_test(&kiln_sheet::Command::SetCells {
        sheet: data,
        cells: vec![kiln_sheet::commands::CellInput::value(1, 2, 100.0)],
    })
    .expect("edit");
    d.absorb_changes();
    assert_eq!(num(&d, 5, 1), 43.0, "the cache holds until Refresh");
    run(&mut d, G::RefreshPivots { all: false });
    assert_eq!(num(&d, 3, 1), 121.0);
    assert_eq!(num(&d, 5, 1), 133.0);
    // Undo the refresh, then the fields.
    assert!(d.undo());
    assert_eq!(num(&d, 5, 1), 43.0);
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(text(&d, 2, 0), "");
    assert!(d.undo());
    assert_eq!(
        d.workbook.sheets().len(),
        1,
        "the new sheet goes with the PivotTable"
    );
    assert!(d.redo());
    assert_eq!(d.workbook.sheets().len(), 2);
}

#[test]
fn refresh_all_refreshes_the_pivot_tables() {
    let mut d = doc();
    sales(&mut d);
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::CreatePivot {
            source: "Sheet1!$A$1:$C$6".into(),
            to: None,
        },
    );
    place(&mut d, 0, GridPivotArea::Rows);
    place(&mut d, 2, GridPivotArea::Values);
    assert_eq!(num(&d, 5, 1), 43.0);
    let data = d.workbook.sheets()[1].id();
    d.apply_for_test(&kiln_sheet::Command::SetCells {
        sheet: data,
        cells: vec![kiln_sheet::commands::CellInput::value(1, 2, 100.0)],
    })
    .expect("edit");
    d.absorb_changes();
    // Data > Refresh All, from any sheet: every PivotTable reads its source again.
    run(&mut d, G::ActivateSheet { index: 1 });
    run(&mut d, G::RefreshAll);
    run(&mut d, G::ActivateSheet { index: 0 });
    assert_eq!(num(&d, 5, 1), 133.0);
}

#[test]
fn a_conditional_format_over_the_report_stays_through_a_refresh() {
    use kiln_protocol::commands::grid_features::{
        GridCfRule, GridCfRuleKind, GridCfValue, GridCfValueKind,
    };
    let mut d = doc();
    sales(&mut d);
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::CreatePivot {
            source: "Sheet1!$A$1:$C$6".into(),
            to: None,
        },
    );
    place(&mut d, 0, GridPivotArea::Rows);
    place(&mut d, 2, GridPivotArea::Values);
    // A data bar on the values (B4:B5): the report's cells take rules like any other.
    let auto = GridCfValue {
        kind: GridCfValueKind::Automatic,
        value: String::new(),
        gte: true,
    };
    run(
        &mut d,
        G::AddConditionalFormat {
            rule: GridCfRule {
                id: 0,
                applies_to: "$B$4:$B$5".into(),
                kind: GridCfRuleKind::DataBar {
                    min: auto.clone(),
                    max: auto,
                    color: "638EC6".into(),
                    gradient: true,
                    show_value: true,
                    negative_color: None,
                    border_color: None,
                },
                style: None,
                stop_if_true: false,
            },
        },
    );
    let sid = d.sid().expect("sheet");
    let bars = |d: &mut GridDoc| {
        let cf = d
            .workbook
            .conditional_formats(sid, kiln_sheet::Range::new(3, 1, 4, 1));
        [3, 4].map(|r| {
            cf.get(&kiln_sheet::CellPos::new(r, 1))
                .is_some_and(|v| v.data_bar.is_some())
        })
    };
    assert_eq!(bars(&mut d), [true, true]);
    run(&mut d, G::RefreshPivots { all: true });
    assert_eq!(bars(&mut d), [true, true], "the rule outlives the refresh");
}

#[test]
fn the_report_refuses_edits_and_shows_details() {
    let mut d = doc();
    sales(&mut d);
    run(
        &mut d,
        G::CreatePivot {
            source: "A1:C6".into(),
            to: Some("E3".into()),
        },
    );
    place(&mut d, 0, GridPivotArea::Rows);
    place(&mut d, 1, GridPivotArea::Columns);
    place(&mut d, 2, GridPivotArea::Values);
    // E3 "Sum of Amount", F3 "Column Labels"; F4 Apple, G4 Banana; E5 East.
    assert_eq!(text(&d, 3, 5), "Apple");
    assert_eq!(num(&d, 4, 5), 11.0);
    // Typing into the report: Excel's message, nothing changes.
    at(&mut d, 4, 5);
    run(&mut d, G::BeginEdit { text: "9".into() });
    assert_eq!(state(&d).alert.as_deref(), Some(super::pivot::PIVOT_LOCKED));
    assert!(d.edit.is_none());
    run(&mut d, G::ClearContents);
    assert_eq!(state(&d).alert.as_deref(), Some(super::pivot::PIVOT_LOCKED));
    assert_eq!(num(&d, 4, 5), 11.0);
    // The active value's field and Show Details.
    let p = state(&d).pivot.expect("pivot");
    assert!(p.on_value);
    assert_eq!(p.active_value, Some(0));
    run(&mut d, G::PivotDetails { row: 4, col: 5 });
    assert_eq!(d.workbook.sheets().len(), 2);
    assert_eq!(text(&d, 0, 0), "Region");
    assert_eq!((text(&d, 1, 1), num(&d, 1, 2)), ("Apple".into(), 10.0));
    assert_eq!(num(&d, 2, 2), 1.0);
    assert_eq!(text(&d, 3, 0), "");
    assert_eq!(d.workbook.sheets()[0].tables().len(), 1);
}

#[test]
fn settings_filters_layout_and_the_field_buttons() {
    let mut d = doc();
    sales(&mut d);
    run(
        &mut d,
        G::CreatePivot {
            source: "A1:C6".into(),
            to: Some("E3".into()),
        },
    );
    place(&mut d, 0, GridPivotArea::Rows);
    place(&mut d, 1, GridPivotArea::Rows);
    place(&mut d, 2, GridPivotArea::Values);
    // Value Field Settings: Average, renamed by the function change.
    let mut v = state(&d).pivot.expect("pivot").values[0].clone();
    v.function = GridPivotFunction::Average;
    run(
        &mut d,
        G::Pivot {
            edit: GridPivotEdit::Value { index: 0, value: v },
        },
    );
    assert_eq!(text(&d, 2, 5), "Average of Amount");
    assert_eq!(text(&d, 3, 4), "East");
    assert!((num(&d, 3, 5) - 31.0 / 3.0).abs() < 1e-9);
    // Hide West through the field's filter list.
    let items = match q(&d, GridQuery::PivotItems { field: 0 }) {
        GridQueryResult::Items(i) => i,
        other => panic!("{other:?}"),
    };
    assert_eq!(
        items.iter().map(|i| i.text.as_str()).collect::<Vec<_>>(),
        ["East", "West"]
    );
    let mut s = state(&d).pivot.expect("pivot").fields[0].settings.clone();
    s.hidden = vec!["West".into()];
    run(
        &mut d,
        G::Pivot {
            edit: GridPivotEdit::Field {
                field: 0,
                settings: s,
            },
        },
    );
    assert!(state(&d).pivot.expect("pivot").fields[0].filtered);
    assert_eq!(text(&d, 6, 4), "Grand Total");
    // Tabular form, no subtotals for rows.
    let mut o = state(&d).pivot.expect("pivot").options;
    o.layout = GridPivotLayout::Tabular;
    o.style = "PivotStyleMedium9".into();
    run(
        &mut d,
        G::Pivot {
            edit: GridPivotEdit::Options { options: o },
        },
    );
    assert_eq!(text(&d, 2, 4), "Region");
    assert_eq!(text(&d, 2, 5), "Product");
    assert_eq!(
        (text(&d, 3, 4), text(&d, 3, 5)),
        ("East".into(), "Apple".into())
    );
    // Field buttons on the captions are hit.
    let r = d.range_rect_surface(kiln_sheet::Range::cell(2, 4));
    let hit = d.analysis_hit(r[0] + r[2] - 8.0, r[1] + r[3] - 8.0);
    assert_eq!(hit, Some(GridHit::PivotButton { row: 2, col: 4 }));
    assert_eq!(d.analysis_hit(r[0] + 4.0, r[1] + 4.0), None);
    // Clear All empties the report; Delete removes it.
    run(
        &mut d,
        G::Pivot {
            edit: GridPivotEdit::ClearAll,
        },
    );
    assert!(state(&d).placeholder.is_some());
    assert_eq!(text(&d, 3, 4), "");
    at(&mut d, 2, 4);
    run(
        &mut d,
        G::Pivot {
            edit: GridPivotEdit::Delete,
        },
    );
    assert!(d.workbook.sheets()[0].pivots().is_empty());
    // A report cannot cover data.
    put(&mut d, 9, 1, "in the way");
    run(
        &mut d,
        G::CreatePivot {
            source: "A1:C6".into(),
            to: Some("B9".into()),
        },
    );
    let e = fails(
        &mut d,
        G::Pivot {
            edit: GridPivotEdit::Place {
                field: Some(0),
                area: GridPivotArea::Rows,
                position: None,
            },
        },
    );
    assert!(e.contains("There's already data"), "{e}");
}

#[test]
fn sparklines_insert_edit_and_follow_their_cells() {
    let mut d = doc();
    sales(&mut d);
    run(
        &mut d,
        G::InsertSparklines {
            kind: GridSparkKind::Line,
            data: "C2:C6".into(),
            location: "E2".into(),
        },
    );
    at(&mut d, 1, 4);
    let s = state(&d).sparkline.expect("sparkline");
    assert_eq!(
        (s.data.as_str(), s.location.as_str(), s.count),
        ("C2:C6", "E2", 1)
    );
    let mut o = s.options.clone();
    o.kind = GridSparkKind::Column;
    o.high = true;
    o.colors[0] = "1D8A60".into();
    run(
        &mut d,
        G::Sparklines {
            edit: GridSparkEdit::Options { options: o },
        },
    );
    let s = state(&d).sparkline.expect("sparkline");
    assert_eq!(s.options.kind, GridSparkKind::Column);
    assert!(s.options.high);
    assert_eq!(s.options.colors[0], "1D8A60");
    // One per row of a block.
    run(
        &mut d,
        G::InsertSparklines {
            kind: GridSparkKind::WinLoss,
            data: "B2:C6".into(),
            location: "F2:F6".into(),
        },
    );
    assert_eq!(d.workbook.sheets()[0].meta().sparklines.len(), 2);
    assert!(
        fails(
            &mut d,
            G::InsertSparklines {
                kind: GridSparkKind::Line,
                data: "B2:C6".into(),
                location: "F2:G3".into(),
            }
        )
        .contains("Location reference is not valid")
    );
    // Rows inserted above move them.
    at(&mut d, 0, 0);
    run(&mut d, G::InsertRows);
    let sheet = &d.workbook.sheets()[0];
    assert!(sheet.sparkline_at(2, 4).is_some());
    assert_eq!(
        sheet.sparkline_at(2, 4).map(|x| x.1.data),
        Some(kiln_sheet::Range::new(2, 2, 6, 2))
    );
    // Clear the group.
    select(&mut d, 2, 5, 6, 5);
    run(
        &mut d,
        G::Sparklines {
            edit: GridSparkEdit::Clear { groups: true },
        },
    );
    assert_eq!(d.workbook.sheets()[0].meta().sparklines.len(), 1);
    assert!(d.undo());
    assert_eq!(d.workbook.sheets()[0].meta().sparklines.len(), 2);
}

#[test]
fn goal_seek_finds_the_input() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 0, "=A1*A1+3");
    run(
        &mut d,
        G::GoalSeek {
            set_cell: "A2".into(),
            to_value: "52".into(),
            by_changing: "A1".into(),
        },
    );
    assert!((num(&d, 0, 0) - 7.0).abs() < 1e-3, "{}", num(&d, 0, 0));
    let s = state(&d).goal_seek.expect("status");
    assert!(s.found);
    assert_eq!(s.cell, "A2");
    assert_eq!(s.target, "52");
    assert!(d.undo());
    assert_eq!(num(&d, 0, 0), 1.0);
    assert!(
        fails(
            &mut d,
            G::GoalSeek {
                set_cell: "A1".into(),
                to_value: "1".into(),
                by_changing: "A2".into(),
            }
        )
        .contains("Cell must contain a formula")
    );
}

#[test]
fn outline_buttons_show_levels_and_toggle_groups() {
    let mut d = doc();
    sales(&mut d);
    let sid = d.sid().expect("sheet");
    for (i, (r, a)) in [
        ("East", 10.0),
        ("East", 20.0),
        ("East", 1.0),
        ("West", 5.0),
        ("West", 7.0),
    ]
    .iter()
    .enumerate()
    {
        d.apply_for_test(&kiln_sheet::Command::SetCells {
            sheet: sid,
            cells: vec![
                kiln_sheet::commands::CellInput::value(i as u32 + 1, 0, *r),
                kiln_sheet::commands::CellInput::value(i as u32 + 1, 2, *a),
            ],
        })
        .expect("data");
    }
    d.absorb_changes();
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::Subtotal {
            spec: Some(GridSubtotalSpec {
                at_change_in: 0,
                function: GridPivotFunction::Sum,
                columns: vec![2],
                replace: true,
                page_breaks: false,
                below: true,
            }),
        },
    );
    // Rows 1–3 East (level 2), 4 East Total (1), 5–6 West (2), 7 West Total (1), 8 Grand Total.
    let shown = |d: &GridDoc| -> Vec<u32> {
        (1..=8)
            .filter(|&r| !d.workbook.sheets()[0].rows().is_hidden(r))
            .collect()
    };
    // The gutter: three level buttons beside the numbers; button 2 shows only the totals.
    let s = match q(&d, GridQuery::State) {
        GridQueryResult::State(s) => s,
        _ => panic!("state"),
    };
    assert!(s.layout.header_w >= 44.0 + 52.0, "{}", s.layout.header_w);
    let hit = |d: &GridDoc, x: f32, y: f32| match q(
        d,
        GridQuery::HitTest {
            x,
            y,
            coarse: false,
        },
    ) {
        GridQueryResult::Hit(h) => h,
        _ => panic!("hit"),
    };
    assert_eq!(
        hit(&d, 26.0, 12.0),
        GridHit::OutlineLevel {
            columns: false,
            level: 2
        }
    );
    run(
        &mut d,
        G::OutlineLevel {
            columns: false,
            level: 2,
        },
    );
    assert_eq!(shown(&d), [4, 7, 8]);
    assert!(
        d.workbook.sheets()[0]
            .rows()
            .get(4)
            .is_some_and(|p| p.collapsed)
    );
    assert_eq!(
        d.view().sel.active.row,
        4,
        "the active cell left the hidden rows"
    );
    // East's +: its rows come back.
    let r = d.range_rect_surface(kiln_sheet::Range::cell(4, 0));
    assert_eq!(
        hit(&d, 26.0, r[1] + r[3] / 2.0),
        GridHit::OutlineToggle {
            columns: false,
            index: 4
        }
    );
    run(
        &mut d,
        G::OutlineToggle {
            columns: false,
            index: 4,
        },
    );
    assert_eq!(shown(&d), [1, 2, 3, 4, 7, 8]);
    // Level 1, then the grand total's +: the totals show, the groups collapsed at level 2 stay.
    run(
        &mut d,
        G::OutlineLevel {
            columns: false,
            level: 1,
        },
    );
    assert_eq!(shown(&d), [8]);
    run(
        &mut d,
        G::OutlineToggle {
            columns: false,
            index: 8,
        },
    );
    assert_eq!(shown(&d), [4, 7, 8]);
    // East's − hides its rows again; level 3 shows everything; each click is one undo step.
    run(
        &mut d,
        G::OutlineLevel {
            columns: false,
            level: 3,
        },
    );
    assert_eq!(shown(&d), [1, 2, 3, 4, 5, 6, 7, 8]);
    run(
        &mut d,
        G::OutlineToggle {
            columns: false,
            index: 4,
        },
    );
    assert_eq!(shown(&d), [4, 5, 6, 7, 8]);
    assert!(d.undo());
    assert_eq!(shown(&d), [1, 2, 3, 4, 5, 6, 7, 8]);
    // Without the symbols there is no gutter and no button.
    run(&mut d, G::OutlineSymbols);
    assert_ne!(
        hit(&d, 26.0, 12.0),
        GridHit::OutlineLevel {
            columns: false,
            level: 2
        }
    );
    assert!(
        fails(
            &mut d,
            G::OutlineToggle {
                columns: false,
                index: 2
            }
        )
        .contains("summary")
    );
}

#[test]
fn subtotal_groups_totals_and_outline_then_remove_all() {
    let mut d = doc();
    sales(&mut d);
    // Sorted by region: East, East, East, West, West.
    let sid = d.sid().expect("sheet");
    let rows = [
        ("East", 10.0),
        ("East", 20.0),
        ("East", 1.0),
        ("West", 5.0),
        ("West", 7.0),
    ];
    for (i, (r, a)) in rows.iter().enumerate() {
        d.apply_for_test(&kiln_sheet::Command::SetCells {
            sheet: sid,
            cells: vec![
                kiln_sheet::commands::CellInput::value(i as u32 + 1, 0, *r),
                kiln_sheet::commands::CellInput::value(i as u32 + 1, 2, *a),
            ],
        })
        .expect("data");
    }
    d.absorb_changes();
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::Subtotal {
            spec: Some(GridSubtotalSpec {
                at_change_in: 0,
                function: GridPivotFunction::Sum,
                columns: vec![2],
                replace: true,
                page_breaks: false,
                below: true,
            }),
        },
    );
    assert_eq!(text(&d, 4, 0), "East Total");
    assert_eq!(num(&d, 4, 2), 31.0);
    assert_eq!(text(&d, 7, 0), "West Total");
    assert_eq!(num(&d, 7, 2), 12.0);
    assert_eq!(text(&d, 8, 0), "Grand Total");
    assert_eq!(num(&d, 8, 2), 43.0);
    let sheet = &d.workbook.sheets()[0];
    assert_eq!(sheet.rows().outline_level(1), 2);
    assert_eq!(sheet.rows().outline_level(4), 1);
    assert_eq!(sheet.rows().outline_level(8), 0);
    // Count, replacing the sums.
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::Subtotal {
            spec: Some(GridSubtotalSpec {
                at_change_in: 0,
                function: GridPivotFunction::Count,
                columns: vec![2],
                replace: true,
                page_breaks: true,
                below: true,
            }),
        },
    );
    assert_eq!(text(&d, 4, 0), "East Count");
    assert_eq!(num(&d, 4, 2), 3.0);
    assert_eq!(text(&d, 8, 0), "Grand Count");
    assert_eq!(d.workbook.sheets()[0].meta().print.row_breaks, vec![5]);
    // Remove All.
    at(&mut d, 2, 0);
    run(&mut d, G::Subtotal { spec: None });
    assert_eq!(text(&d, 4, 0), "West");
    assert_eq!(text(&d, 6, 0), "");
    assert_eq!(d.workbook.sheets()[0].rows().outline_level(1), 0);
    assert!(d.workbook.sheets()[0].meta().print.row_breaks.is_empty());
}

#[test]
fn flash_fill_learns_from_examples_and_suggests() {
    let mut d = doc();
    let names = [
        "Full name",
        "Ana Lima",
        "Ben Okoro",
        "Caro Wit",
        "Dev Patel",
    ];
    for (i, n) in names.iter().enumerate() {
        put(&mut d, i as u32, 0, n);
    }
    put(&mut d, 0, 1, "First");
    put(&mut d, 1, 1, "Ana");
    at(&mut d, 2, 1);
    run(&mut d, G::FlashFill);
    assert_eq!(text(&d, 2, 1), "Ben");
    assert_eq!(text(&d, 4, 1), "Dev");
    assert_eq!(text(&d, 0, 1), "First", "the header stays");
    assert!(d.undo());
    // Two examples: the suggestions show; Flash Fill accepts them.
    put(&mut d, 0, 2, "Last");
    put(&mut d, 1, 2, "Lima");
    put(&mut d, 2, 2, "Okoro");
    let s = state(&d);
    assert_eq!(
        s.flash.iter().map(|c| c.text.as_str()).collect::<Vec<_>>(),
        ["Wit", "Patel"]
    );
    assert_eq!(s.flash_count, 2);
    assert!(s.flash[0].rect[2] > 0.0);
    run(&mut d, G::FlashFill);
    assert_eq!(text(&d, 4, 2), "Patel");
    assert!(state(&d).flash.is_empty());
}

#[test]
fn protection_blocks_locked_cells_and_checks_passwords() {
    let mut d = doc();
    put(&mut d, 0, 0, "locked");
    // B1 unlocked before protecting.
    at(&mut d, 0, 1);
    run(&mut d, G::LockCells { locked: false });
    assert!(!state(&d).locked);
    run(
        &mut d,
        G::ProtectSheet {
            protect: true,
            password: "kiln".into(),
            allow: None,
        },
    );
    let s = state(&d);
    assert!(s.sheet_protected && s.sheet_password);
    assert!(s.permissions.select_locked && !s.permissions.format_cells);
    // Typing into A1: Excel's message.
    at(&mut d, 0, 0);
    run(&mut d, G::BeginEdit { text: "x".into() });
    assert_eq!(state(&d).alert.as_deref(), Some(super::PROTECTED_MESSAGE));
    assert!(d.edit.is_none());
    run(&mut d, G::ClearContents);
    assert_eq!(state(&d).alert.as_deref(), Some(super::PROTECTED_MESSAGE));
    assert_eq!(text(&d, 0, 0), "locked");
    // B1 takes entries.
    put(&mut d, 0, 1, "open");
    assert_eq!(text(&d, 0, 1), "open");
    // The password.
    let e = fails(
        &mut d,
        G::ProtectSheet {
            protect: false,
            password: "Kiln".into(),
            allow: None,
        },
    );
    assert!(
        e.contains("The password you supplied is not correct"),
        "{e}"
    );
    run(
        &mut d,
        G::ProtectSheet {
            protect: false,
            password: "kiln".into(),
            allow: None,
        },
    );
    assert!(!state(&d).sheet_protected);
    // The workbook's structure.
    run(
        &mut d,
        G::ProtectWorkbook {
            protect: true,
            password: String::new(),
        },
    );
    assert!(state(&d).workbook_protected);
    run(&mut d, G::InsertSheet);
    assert_eq!(state(&d).alert.as_deref(), Some(super::STRUCTURE_MESSAGE));
    assert_eq!(d.workbook.sheets().len(), 1);
    run(
        &mut d,
        G::ProtectWorkbook {
            protect: false,
            password: String::new(),
        },
    );
    run(&mut d, G::InsertSheet);
    assert_eq!(d.workbook.sheets().len(), 2);
}

#[test]
fn themes_change_colours_and_theme_fonts() {
    let mut d = doc();
    let themes = match q(&d, GridQuery::Themes) {
        GridQueryResult::Themes(t) => t,
        other => panic!("{other:?}"),
    };
    assert_eq!(themes.len(), 13);
    assert_eq!(themes[0].name, "Office Theme");
    let tenmoku = themes.iter().find(|t| t.id == "tenmoku").expect("tenmoku");
    // The default font follows the theme's body font.
    let mut font = d.workbook.styles().default_font().clone();
    font.scheme = kiln_sheet::styles::FontScheme::Minor;
    let mut styles = kiln_sheet::StyleTable::new(font);
    d.workbook.swap_styles_for_test(&mut styles);
    run(
        &mut d,
        G::SetTheme {
            theme: "tenmoku".into(),
            part: GridThemePart::All,
        },
    );
    assert_eq!(state(&d).theme, tenmoku.name);
    assert_eq!(
        &*d.workbook.styles().default_font().name,
        tenmoku.minor_font.as_str()
    );
    assert_eq!(
        format!("{:06X}", d.workbook.theme().colors[4]),
        tenmoku.colors[4]
    );
    run(
        &mut d,
        G::SetTheme {
            theme: "office".into(),
            part: GridThemePart::Colors,
        },
    );
    assert_eq!(d.workbook.theme().colors[4], 0x44_72C4);
    assert_eq!(d.workbook.theme().minor_font, tenmoku.minor_font);
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(&*d.workbook.styles().default_font().name, "Calibri");
    assert_eq!(d.workbook.theme().name, "Office Theme");
}

#[test]
fn libreoffice_verifies_the_sheet_password() {
    use kiln_protocol::FileFormat;
    let mut d = doc();
    put(&mut d, 0, 0, "locked");
    run(
        &mut d,
        G::ProtectSheet {
            protect: true,
            password: "Kiln 26".into(),
            allow: None,
        },
    );
    let bytes = d.save(FileFormat::Xlsx).expect("save");
    // The hash survives a Kiln round trip and unprotects with the password.
    let mut again = GridDoc::new(
        kiln_io_xlsx::read_xlsx(&bytes).expect("read").workbook,
        None,
        Vec::new(),
        None,
    );
    let e = again
        .execute(&Command::Grid(G::ProtectSheet {
            protect: false,
            password: "kiln 26".into(),
            allow: None,
        }))
        .expect_err("wrong password")
        .to_string();
    assert!(e.contains("not correct"), "{e}");
    run(
        &mut again,
        G::ProtectSheet {
            protect: false,
            password: "Kiln 26".into(),
            allow: None,
        },
    );
    // LibreOffice checks the same hash (skipped where it is not installed).
    if kiln_testing::oracle::Oracle::locate().is_err() {
        eprintln!("LibreOffice is not installed: skipped");
        return;
    }
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/tmp")
        .join(format!("protect-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp");
    let path = dir.join("protected.xlsx");
    std::fs::write(&path, &bytes).expect("write");
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../kiln-io-xlsx/tests/oracle/libreoffice-uno.py");
    let lo = |password: &str| -> serde_json::Value {
        let out = std::process::Command::new("python3")
            .arg(&script)
            .arg(&path)
            .arg(password)
            .stdin(std::process::Stdio::null())
            .output()
            .expect("python3");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).expect("json")
    };
    let wrong = lo("kiln 26");
    assert_eq!(wrong["sheets"][0]["protected"], true);
    assert_eq!(wrong["sheets"][0]["unprotected"], false);
    let right = lo("Kiln 26");
    assert_eq!(right["sheets"][0]["unprotected"], true, "{right}");
}

/// Opens a workbook of the fixtures on its Report sheet.
fn open_fixture(path: &str) -> GridDoc {
    let bytes = std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path))
        .expect("fixture");
    open_bytes(&bytes)
}

fn open_bytes(bytes: &[u8]) -> GridDoc {
    let read = kiln_io_xlsx::read_xlsx(bytes).expect("read");
    let mut d = GridDoc::new(read.workbook, Some(read.source), read.warnings, None);
    d.last_view = doc().last_view;
    let report = d
        .workbook
        .sheets()
        .iter()
        .position(|s| s.name() == "Report")
        .expect("Report");
    run(
        &mut d,
        G::ActivateSheet {
            index: report as u32,
        },
    );
    d
}

/// The labels of a column of the active sheet, `r0..=r1`.
fn labels(d: &GridDoc, col: u32, r0: u32, r1: u32) -> Vec<String> {
    (r0..=r1).map(|r| text(d, r, col)).collect()
}

fn refresh_at(d: &mut GridDoc, row: u32, col: u32) {
    at(d, row, col);
    run(d, G::RefreshPivots { all: false });
}

/// Puts a value on the Data sheet and comes back to Report.
fn put_data(d: &mut GridDoc, row: u32, col: u32, value: &str) {
    let back = d
        .workbook
        .sheet_index(d.sid().expect("sheet"))
        .expect("index");
    run(d, G::ActivateSheet { index: 0 });
    put(d, row, col, value);
    run(d, G::ActivateSheet { index: back as u32 });
}

#[test]
fn refresh_keeps_the_manual_order_of_a_pivot_table_from_a_file() {
    use kiln_protocol::FileFormat;
    // Excel's file: the cache lists West, East, South, North; the report shows them in the
    // manual order East, North, South, West (no sortType).
    let mut d = open_fixture("../../fixtures/xlsx/features/pivot-tables.xlsx");
    let order = ["East", "North", "South", "West", "Grand Total"].map(String::from);
    assert_eq!(labels(&d, 0, 4, 8), order);
    refresh_at(&mut d, 4, 1);
    assert_eq!(labels(&d, 0, 4, 8), order, "a refresh with no change");
    // A new item goes at the end, as in Excel: Data!A2 (a West record) becomes Central.
    put_data(&mut d, 1, 0, "Central");
    refresh_at(&mut d, 4, 1);
    let more = ["East", "North", "South", "West", "Central", "Grand Total"].map(String::from);
    assert_eq!(labels(&d, 0, 4, 9), more);
    // Saved and opened again, the order stays, and so it does through another refresh.
    let bytes = d.save(FileFormat::Xlsx).expect("save");
    let mut again = open_bytes(&bytes);
    assert_eq!(labels(&again, 0, 4, 9), more, "reopened");
    refresh_at(&mut again, 4, 1);
    assert_eq!(labels(&again, 0, 4, 9), more, "reopened and refreshed");
    // An item that is gone leaves; the others keep their places.
    for r in 1..=25 {
        let sid = again.workbook.sheets()[0].id();
        if again.workbook.display_text(sid, r, 0) == "North" {
            put_data(&mut again, r, 0, "East");
        }
    }
    refresh_at(&mut again, 4, 1);
    assert_eq!(
        labels(&again, 0, 4, 8),
        ["East", "South", "West", "Central", "Grand Total"].map(String::from)
    );
}

#[test]
fn refresh_keeps_the_order_of_a_libreoffice_pivot_table() {
    use kiln_protocol::FileFormat;
    // LibreOffice sorts the cache (East, North, West); the records come East, West, ..., North.
    let mut d = open_fixture("../../fixtures/xlsx/manual/pivot-libreoffice.xlsx");
    let order = ["East", "North", "West", "Total Result"].map(String::from);
    let shown = labels(&d, 0, 6, 9);
    assert_eq!(shown[..3], order[..3], "{shown:?}");
    refresh_at(&mut d, 6, 1);
    assert_eq!(labels(&d, 0, 6, 8), shown[..3], "a refresh with no change");
    put_data(&mut d, 2, 0, "Central");
    refresh_at(&mut d, 6, 1);
    let more = labels(&d, 0, 6, 9);
    assert_eq!(more, ["East", "North", "West", "Central"].map(String::from));
    let bytes = d.save(FileFormat::Xlsx).expect("save");
    let mut again = open_bytes(&bytes);
    refresh_at(&mut again, 6, 1);
    assert_eq!(
        labels(&again, 0, 6, 9),
        more,
        "saved, reopened and refreshed"
    );
}

/// Runs a command on a protected sheet: refused with Excel's message (a message box, or the
/// error the editor shows as one), and nothing changes.
fn refused_on_protected(d: &mut GridDoc, c: G) {
    let before = d.save(kiln_protocol::FileFormat::Xlsx).expect("save");
    let before = kiln_io_xlsx::read_xlsx(&before).expect("read").workbook;
    let meta = |wb: &Workbook| format!("{:?}", wb.sheets()[0].meta());
    let cells = |wb: &Workbook| {
        (0..12)
            .flat_map(|r| (0..8).map(move |c| (r, c)))
            .map(|(r, c)| wb.display_text(wb.sheets()[0].id(), r, c))
            .collect::<Vec<_>>()
    };
    let (m0, c0) = (meta(&d.workbook), cells(&before));
    match d.execute(&Command::Grid(c.clone())) {
        Ok(_) => assert_eq!(
            state(d).alert.as_deref(),
            Some(super::PROTECTED_MESSAGE),
            "{c:?}"
        ),
        Err(e) => assert!(e.to_string().contains("protected sheet"), "{c:?}: {e}"),
    }
    assert_eq!(meta(&d.workbook), m0, "{c:?} changed the sheet");
    assert_eq!(cells(&d.workbook), c0, "{c:?} changed the cells");
}

#[test]
fn a_protected_sheet_refuses_every_analysis_edit() {
    let mut d = doc();
    sales(&mut d);
    put(&mut d, 1, 3, "=C2*2");
    run(
        &mut d,
        G::InsertSparklines {
            kind: GridSparkKind::Line,
            data: "C2:C6".into(),
            location: "E2".into(),
        },
    );
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::CreatePivot {
            source: "Sheet1!$A$1:$C$6".into(),
            to: Some("Sheet1!$H$1".into()),
        },
    );
    place(&mut d, 0, GridPivotArea::Rows);
    place(&mut d, 2, GridPivotArea::Values);
    run(
        &mut d,
        G::ProtectSheet {
            protect: true,
            password: String::new(),
            allow: None,
        },
    );
    // Sparklines.
    at(&mut d, 0, 6);
    refused_on_protected(
        &mut d,
        G::InsertSparklines {
            kind: GridSparkKind::Column,
            data: "C2:C6".into(),
            location: "G1".into(),
        },
    );
    at(&mut d, 1, 4);
    for edit in [
        GridSparkEdit::Style { index: 3 },
        GridSparkEdit::Clear { groups: true },
        GridSparkEdit::Ungroup,
        GridSparkEdit::Data {
            data: "C2:C5".into(),
            location: "E2".into(),
        },
    ] {
        refused_on_protected(&mut d, G::Sparklines { edit });
    }
    // Goal Seek, Subtotal, Flash Fill and Lock Cell.
    refused_on_protected(
        &mut d,
        G::GoalSeek {
            set_cell: "D2".into(),
            to_value: "100".into(),
            by_changing: "C2".into(),
        },
    );
    at(&mut d, 1, 0);
    refused_on_protected(
        &mut d,
        G::Subtotal {
            spec: Some(GridSubtotalSpec {
                at_change_in: 0,
                function: GridPivotFunction::Sum,
                columns: vec![2],
                replace: true,
                page_breaks: false,
                below: true,
            }),
        },
    );
    refused_on_protected(&mut d, G::LockCells { locked: false });
    // The PivotTable on the sheet.
    at(&mut d, 2, 8);
    refused_on_protected(&mut d, G::RefreshPivots { all: false });
    refused_on_protected(
        &mut d,
        G::Pivot {
            edit: GridPivotEdit::Place {
                field: Some(1),
                area: GridPivotArea::Columns,
                position: None,
            },
        },
    );
    refused_on_protected(
        &mut d,
        G::CreatePivot {
            source: "Sheet1!$A$1:$C$6".into(),
            to: Some("Sheet1!$H$20".into()),
        },
    );
}

#[test]
fn a_protected_sheet_refuses_the_outline_buttons() {
    let mut d = doc();
    sales(&mut d);
    at(&mut d, 1, 0);
    run(&mut d, G::SortAscending);
    run(
        &mut d,
        G::Subtotal {
            spec: Some(GridSubtotalSpec {
                at_change_in: 0,
                function: GridPivotFunction::Sum,
                columns: vec![2],
                replace: true,
                page_breaks: false,
                below: true,
            }),
        },
    );
    run(
        &mut d,
        G::ProtectSheet {
            protect: true,
            password: String::new(),
            allow: None,
        },
    );
    refused_on_protected(
        &mut d,
        G::OutlineLevel {
            columns: false,
            level: 1,
        },
    );
    refused_on_protected(
        &mut d,
        G::OutlineToggle {
            columns: false,
            index: 4,
        },
    );
}

#[test]
fn the_saved_sheet_format_carries_the_outline_levels() {
    use kiln_protocol::FileFormat;
    let format_pr = |d: &mut GridDoc| -> String {
        let bytes = d.save(FileFormat::Xlsx).expect("save");
        let read = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
        let xml = read
            .source
            .package()
            .part("/xl/worksheets/sheet1.xml")
            .expect("part")
            .expect("sheet1");
        let xml = String::from_utf8_lossy(&xml).into_owned();
        let at = xml.find("<sheetFormatPr").expect("sheetFormatPr");
        xml[at..at + xml[at..].find('>').expect("end")].to_owned()
    };
    let mut d = doc();
    sales(&mut d);
    at(&mut d, 1, 0);
    run(&mut d, G::SortAscending);
    let plain = format_pr(&mut d);
    assert!(!plain.contains("outlineLevel"), "{plain}");
    // Subtotal: two row levels below the grand total's.
    run(
        &mut d,
        G::Subtotal {
            spec: Some(GridSubtotalSpec {
                at_change_in: 0,
                function: GridPivotFunction::Sum,
                columns: vec![2],
                replace: true,
                page_breaks: false,
                below: true,
            }),
        },
    );
    let rows = format_pr(&mut d);
    assert!(rows.contains(r#"outlineLevelRow="2""#), "{rows}");
    assert!(!rows.contains("outlineLevelCol"), "{rows}");
    // Grouped columns too.
    let sid = d.sid().expect("sheet");
    d.apply_for_test(&kiln_sheet::Command::SetColProps {
        sheet: sid,
        at: 1,
        count: 2,
        patch: kiln_sheet::commands::ColPatch {
            outline_level: Some(1),
            ..Default::default()
        },
    })
    .expect("group columns");
    d.absorb_changes();
    let both = format_pr(&mut d);
    assert!(
        both.contains(r#"outlineLevelRow="2""#) && both.contains(r#"outlineLevelCol="1""#),
        "{both}"
    );
    // Remove All: the levels go.
    at(&mut d, 1, 0);
    run(&mut d, G::Subtotal { spec: None });
    let rows_gone = format_pr(&mut d);
    assert!(!rows_gone.contains("outlineLevelRow"), "{rows_gone}");
    assert!(rows_gone.contains(r#"outlineLevelCol="1""#), "{rows_gone}");
}

#[test]
fn getpivotdata_reads_the_report_and_follows_a_refresh() {
    let mut d = doc();
    sales(&mut d);
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::CreatePivot {
            source: "Sheet1!$A$1:$C$6".into(),
            to: None,
        },
    );
    place(&mut d, 0, GridPivotArea::Rows);
    place(&mut d, 1, GridPivotArea::Columns);
    place(&mut d, 2, GridPivotArea::Values);
    let f = [
        "=GETPIVOTDATA(\"Amount\",$A$3,\"Region\",\"East\",\"Product\",\"Apple\")",
        "=GETPIVOTDATA(\"Sum of Amount\",$A$3,\"region\",\"west\")",
        "=GETPIVOTDATA(\"Amount\",B5,\"Product\",\"Banana\")",
        "=GETPIVOTDATA(\"Amount\",$A$3)",
        "=GETPIVOTDATA(\"Amount\",$A$3,\"Region\",\"North\")",
        "=GETPIVOTDATA(\"Profit\",$A$3)",
        "=GETPIVOTDATA(\"Amount\",$H$1)",
        "=GETPIVOTDATA(\"Amount\",$A$3,\"Product\",\"Apple\",\"Region\",\"West\")",
    ];
    for (i, text) in f.iter().enumerate() {
        put(&mut d, i as u32, 8, text);
    }
    let col = |d: &GridDoc| {
        (0..f.len() as u32)
            .map(|r| text(d, r, 8))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        col(&d),
        ["11", "12", "25", "43", "#REF!", "#REF!", "#REF!", "7"]
    );
    // The source changes and Refresh rewrites the report: the formulas follow.
    let data = d.workbook.sheets()[1].id();
    d.apply_for_test(&kiln_sheet::Command::SetCells {
        sheet: data,
        cells: vec![kiln_sheet::commands::CellInput::value(1, 2, 100.0)],
    })
    .expect("edit");
    d.absorb_changes();
    at(&mut d, 3, 0);
    run(&mut d, G::RefreshPivots { all: false });
    assert_eq!(text(&d, 0, 8), "101");
    assert_eq!(text(&d, 3, 8), "133");
}
