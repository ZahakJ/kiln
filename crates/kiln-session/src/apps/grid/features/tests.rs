// SPDX-License-Identifier: AGPL-3.0-or-later
//! The features through the Grid document, one behaviour per test: what the commands do to
//! the workbook, what the queries answer, undo, and the overlay and hit tests of their buttons.

use kiln_protocol::commands::grid_features::*;
use kiln_protocol::commands::{GridCommand as G, GridHit, GridQuery, GridQueryResult, GridRange};
use kiln_protocol::print::PrintSettings;
use kiln_protocol::{Command, Query, QueryResult};
use kiln_sheet::{CellValue, Range, Workbook};

use super::super::GridDoc;
use crate::model::DocModel;

fn doc() -> GridDoc {
    let mut d = GridDoc::new(
        Workbook::new(),
        None,
        Vec::new(),
        Some("Budget.xlsx".into()),
    );
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

fn sid(d: &GridDoc) -> kiln_sheet::SheetId {
    d.sid().expect("sheet")
}

fn text(d: &GridDoc, row: u32, col: u32) -> String {
    d.workbook.display_text(sid(d), row, col)
}

fn num(d: &GridDoc, row: u32, col: u32) -> f64 {
    d.workbook
        .value(sid(d), row, col)
        .as_number()
        .unwrap_or(f64::NAN)
}

fn hidden(d: &GridDoc, row: u32) -> bool {
    d.workbook
        .sheet(sid(d))
        .is_some_and(|s| s.is_row_hidden(row))
}

fn state(d: &GridDoc) -> GridFeatureState {
    match q(d, GridQuery::State) {
        GridQueryResult::State(s) => s.features,
        other => panic!("{other:?}"),
    }
}

/// Item, Category, Amount: a header row and six rows of data at A1:C7.
fn budget(d: &mut GridDoc) {
    let rows = [
        ["Item", "Category", "Amount"],
        ["Clay", "Materials", "420"],
        ["Glaze", "Materials", "310"],
        ["Gas", "Firing", "1180"],
        ["Shelves", "Equipment", "0"],
        ["Open day", "Events", "380"],
        ["Kiln hire", "Firing", "640"],
    ];
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            put(d, r as u32, c as u32, v);
        }
    }
    at(d, 1, 0);
}

fn undo(d: &mut GridDoc) {
    assert!(d.undo(), "undo");
}

// ── sort ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn sort_a_to_z_detects_the_header_and_sorts_the_region() {
    let mut d = doc();
    budget(&mut d);
    at(&mut d, 2, 2);
    run(&mut d, G::SortDescending);
    assert_eq!(text(&d, 0, 0), "Item");
    assert_eq!(text(&d, 1, 0), "Gas");
    assert_eq!(text(&d, 6, 0), "Shelves");
    at(&mut d, 1, 0);
    run(&mut d, G::SortAscending);
    assert_eq!(text(&d, 1, 0), "Clay");
    assert_eq!(text(&d, 6, 0), "Shelves");
    undo(&mut d);
    assert_eq!(text(&d, 1, 0), "Gas");
}

#[test]
fn the_sort_dialog_sorts_by_several_levels_and_custom_lists() {
    let mut d = doc();
    budget(&mut d);
    run(
        &mut d,
        G::ApplySort {
            spec: GridSortSpec {
                range: None,
                keys: vec![
                    GridSortKey {
                        index: 1,
                        descending: false,
                        on: GridSortOn::Values,
                        color: None,
                        icon: None,
                        custom_list: Some(vec!["Firing".into(), "Materials".into()]),
                    },
                    GridSortKey {
                        index: 2,
                        descending: true,
                        on: GridSortOn::Values,
                        color: None,
                        icon: None,
                        custom_list: None,
                    },
                ],
                header: Some(true),
                case_sensitive: false,
                by_columns: false,
            },
        },
    );
    let order: Vec<String> = (1..=6).map(|r| text(&d, r, 0)).collect();
    assert_eq!(
        order,
        ["Gas", "Kiln hire", "Clay", "Glaze", "Shelves", "Open day"]
    );
    assert!(
        fails(
            &mut d,
            G::ApplySort {
                spec: GridSortSpec {
                    range: None,
                    keys: Vec::new(),
                    header: None,
                    case_sensitive: false,
                    by_columns: false,
                },
            },
        )
        .contains("sort level")
    );
}

// ── AutoFilter ────────────────────────────────────────────────────────────────────────────

#[test]
fn filter_by_value_hides_rows_and_the_menu_shows_the_checked_values() {
    let mut d = doc();
    budget(&mut d);
    run(&mut d, G::ToggleFilter);
    assert!(state(&d).filter);
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Values {
                values: vec!["Firing".into()],
                blanks: false,
            }),
        },
    );
    assert!(hidden(&d, 1) && hidden(&d, 2) && !hidden(&d, 3) && !hidden(&d, 6));
    assert!(state(&d).filtered);
    let GridQueryResult::FilterMenu(m) = q(&d, GridQuery::FilterMenu { row: 0, col: 1 }) else {
        panic!("menu")
    };
    assert_eq!(m.header, "Category");
    assert_eq!(m.kind, GridValueKind::Text);
    let checked: Vec<&str> = m
        .items
        .iter()
        .filter(|i| i.checked)
        .map(|i| i.text.as_str())
        .collect();
    assert_eq!(checked, ["Firing"]);
    assert_eq!(m.items.len(), 4);
    // SUBTOTAL skips the filtered rows.
    put(&mut d, 8, 2, "=SUBTOTAL(109,C2:C7)");
    assert_eq!(num(&d, 8, 2), 1820.0);
    run(&mut d, G::ClearFilter);
    assert!(!hidden(&d, 1));
    assert_eq!(num(&d, 8, 2), 2930.0);
    run(&mut d, G::ToggleFilter);
    assert!(!state(&d).filter);
}

#[test]
fn number_text_and_top_10_criteria_filter_rows() {
    let mut d = doc();
    budget(&mut d);
    run(&mut d, G::ToggleFilter);
    let filter = |d: &mut GridDoc, col: u32, f: GridColumnFilter| {
        run(
            d,
            G::FilterColumn {
                row: 0,
                col,
                filter: Some(f),
            },
        );
    };
    filter(
        &mut d,
        2,
        GridColumnFilter::Custom {
            and: true,
            criteria: vec![GridCriterion {
                op: GridFilterOp::GreaterThan,
                value: "400".into(),
            }],
        },
    );
    let shown: Vec<u32> = (1..=6).filter(|&r| !hidden(&d, r)).collect();
    assert_eq!(shown, [1, 3, 6]);
    filter(
        &mut d,
        2,
        GridColumnFilter::Top10 {
            top: true,
            percent: false,
            value: 2,
        },
    );
    let shown: Vec<u32> = (1..=6).filter(|&r| !hidden(&d, r)).collect();
    assert_eq!(shown, [3, 6]);
    filter(
        &mut d,
        0,
        GridColumnFilter::Custom {
            and: false,
            criteria: vec![GridCriterion {
                op: GridFilterOp::Contains,
                value: "a".into(),
            }],
        },
    );
    let shown: Vec<u32> = (1..=6).filter(|&r| !hidden(&d, r)).collect();
    assert_eq!(shown, [3], "Gas has an a and is in the top two");
    let GridQueryResult::FilterMenu(m) = q(&d, GridQuery::FilterMenu { row: 0, col: 0 }) else {
        panic!("menu")
    };
    assert_eq!(
        m.filter,
        Some(GridColumnFilter::Custom {
            and: false,
            criteria: vec![GridCriterion {
                op: GridFilterOp::Contains,
                value: "a".into()
            }]
        })
    );
    // Sort from the dropdown keeps the header and remembers the sort.
    run(&mut d, G::ClearFilter);
    run(
        &mut d,
        G::SortFilterColumn {
            row: 0,
            col: 2,
            descending: false,
            on: GridSortOn::Values,
            color: None,
        },
    );
    assert_eq!(text(&d, 1, 0), "Shelves");
    let GridQueryResult::FilterMenu(m) = q(&d, GridQuery::FilterMenu { row: 0, col: 2 }) else {
        panic!("menu")
    };
    assert_eq!(m.sorted, Some(false));
    assert_eq!(m.kind, GridValueKind::Number);
}

#[test]
fn filter_buttons_are_drawn_and_hit() {
    let mut d = doc();
    budget(&mut d);
    run(&mut d, G::ToggleFilter);
    let geo = d.geometry().clone();
    assert!(!d.feature_overlay(&geo).is_empty());
    let hw = d.header_w(&geo);
    let cell = geo.display_rect(0, 1);
    let (x, y) = geo.to_surface(&d.last_view, cell.right() - 8.0, cell.bottom() - 8.0, hw);
    assert_eq!(
        d.feature_hit(x, y),
        Some(GridHit::FilterButton { row: 0, col: 1 })
    );
    let (x, y) = geo.to_surface(&d.last_view, cell.x + 4.0, cell.y + 4.0, hw);
    assert_eq!(d.feature_hit(x, y), None);
}

// ── conditional formatting ────────────────────────────────────────────────────────────────

fn rule(applies_to: &str, kind: GridCfRuleKind, style: Option<GridCfStyle>) -> GridCfRule {
    GridCfRule {
        id: 0,
        applies_to: applies_to.into(),
        kind,
        style,
        stop_if_true: false,
    }
}

#[test]
fn data_bars_scales_icons_and_highlight_rules_show_on_the_cells() {
    let mut d = doc();
    budget(&mut d);
    let auto = GridCfValue {
        kind: GridCfValueKind::Automatic,
        value: String::new(),
        gte: true,
    };
    run(
        &mut d,
        G::AddConditionalFormat {
            rule: rule(
                "C2:C7",
                GridCfRuleKind::DataBar {
                    min: auto.clone(),
                    max: auto,
                    color: "638EC6".into(),
                    gradient: true,
                    show_value: true,
                    negative_color: None,
                    border_color: None,
                },
                None,
            ),
        },
    );
    run(
        &mut d,
        G::AddConditionalFormat {
            rule: rule(
                "$C$2:$C$7",
                GridCfRuleKind::CellValue {
                    op: GridCompare::GreaterThan,
                    values: vec!["500".into()],
                },
                Some(GridCfStyle {
                    fill: Some("FFC7CE".into()),
                    font_color: Some("9C0006".into()),
                    ..GridCfStyle::default()
                }),
            ),
        },
    );
    let s = sid(&d);
    let cf = d.workbook.conditional_formats(s, Range::new(1, 2, 6, 2));
    let gas = cf.get(&kiln_sheet::CellPos::new(3, 2)).expect("visual");
    assert!(gas.data_bar.is_some());
    assert!(gas.dxf.as_ref().is_some_and(|x| x.fill.is_some()));
    assert!(
        cf.get(&kiln_sheet::CellPos::new(2, 2))
            .is_some_and(|v| v.dxf.is_none())
    );
    // Manage Rules lists them newest first; reordering puts the bar on top again.
    at(&mut d, 2, 2);
    let GridQueryResult::Rules(rules) = q(&d, GridQuery::ConditionalFormats { sheet: true }) else {
        panic!("rules")
    };
    assert_eq!(rules.len(), 2);
    assert!(matches!(rules[0].kind, GridCfRuleKind::CellValue { .. }));
    assert_eq!(rules[0].applies_to, "$C$2:$C$7");
    let mut reordered = rules.clone();
    reordered.reverse();
    reordered[1].stop_if_true = true;
    run(
        &mut d,
        G::SetConditionalFormats {
            rules: reordered,
            sheet: true,
        },
    );
    let GridQueryResult::Rules(rules) = q(&d, GridQuery::ConditionalFormats { sheet: true }) else {
        panic!("rules")
    };
    assert!(matches!(rules[0].kind, GridCfRuleKind::DataBar { .. }));
    assert!(rules[1].stop_if_true);
    // Icon sets and colour scales.
    run(
        &mut d,
        G::AddConditionalFormat {
            rule: rule(
                "C2:C7",
                GridCfRuleKind::IconSet {
                    set: "3TrafficLights1".into(),
                    thresholds: vec![
                        GridCfValue {
                            kind: GridCfValueKind::Percent,
                            value: "33".into(),
                            gte: true,
                        },
                        GridCfValue {
                            kind: GridCfValueKind::Percent,
                            value: "67".into(),
                            gte: true,
                        },
                    ],
                    reverse: false,
                    show_value: true,
                },
                None,
            ),
        },
    );
    let cf = d.workbook.conditional_formats(s, Range::new(1, 2, 6, 2));
    assert!(cf.values().all(|v| v.icon.is_some()));
    select(&mut d, 1, 2, 3, 2);
    run(&mut d, G::ClearConditionalFormats { sheet: false });
    let cf = d.workbook.conditional_formats(s, Range::new(1, 2, 6, 2));
    assert!(!cf.contains_key(&kiln_sheet::CellPos::new(2, 2)));
    assert!(cf.contains_key(&kiln_sheet::CellPos::new(5, 2)));
    run(&mut d, G::ClearConditionalFormats { sheet: true });
    assert!(d.workbook.sheet(s).is_some_and(|x| x.meta().cf.is_empty()));
    assert!(
        fails(
            &mut d,
            G::AddConditionalFormat {
                rule: rule(
                    "C2:C7",
                    GridCfRuleKind::Formula {
                        formula: "=SUM(".into()
                    },
                    None
                ),
            },
        )
        .contains("problem")
    );
}

#[test]
fn formula_text_and_top_rules_pick_their_cells() {
    let mut d = doc();
    budget(&mut d);
    let style = Some(GridCfStyle {
        bold: true,
        ..GridCfStyle::default()
    });
    for kind in [
        GridCfRuleKind::Formula {
            formula: "=$B2=\"Firing\"".into(),
        },
        GridCfRuleKind::Text {
            rule: GridTextRule::BeginsWith,
            text: "G".into(),
        },
        GridCfRuleKind::TopBottom {
            bottom: false,
            percent: false,
            rank: 1,
        },
    ] {
        run(
            &mut d,
            G::AddConditionalFormat {
                rule: rule("A2:C7", kind, style.clone()),
            },
        );
    }
    let s = sid(&d);
    let cf = d.workbook.conditional_formats(s, Range::new(1, 0, 6, 2));
    assert!(
        cf.contains_key(&kiln_sheet::CellPos::new(3, 1)),
        "Firing row"
    );
    assert!(
        cf.contains_key(&kiln_sheet::CellPos::new(2, 0)),
        "Glaze begins with G"
    );
    assert!(
        cf.contains_key(&kiln_sheet::CellPos::new(3, 2)),
        "the top value"
    );
    assert!(!cf.contains_key(&kiln_sheet::CellPos::new(4, 2)));
}

// ── validation ────────────────────────────────────────────────────────────────────────────

#[test]
fn a_validation_list_gives_a_dropdown_and_refuses_other_entries() {
    let mut d = doc();
    select(&mut d, 0, 0, 9, 0);
    run(
        &mut d,
        G::SetValidation {
            rule: Some(GridValidation {
                kind: GridValidationKind::List,
                formula1: "Low,Medium,High".into(),
                allow_blank: true,
                dropdown: true,
                show_input: true,
                input_title: "Priority".into(),
                input: "Pick one".into(),
                show_error: true,
                ..GridValidation::default()
            }),
        },
    );
    at(&mut d, 3, 0);
    let s = state(&d);
    assert!(s.dropdown);
    assert_eq!(s.prompt, Some(["Priority".into(), "Pick one".into()]));
    let GridQueryResult::Validation(Some(v)) = q(&d, GridQuery::Validation) else {
        panic!("validation")
    };
    assert_eq!(v.formula1, "Low,Medium,High");
    // The in-cell button opens the list.
    let geo = d.geometry().clone();
    let hw = d.header_w(&geo);
    let cell = geo.display_rect(3, 0);
    let (x, y) = geo.to_surface(&d.last_view, cell.right() + 8.0, cell.bottom() - 6.0, hw);
    assert_eq!(d.feature_hit(x, y), Some(GridHit::ListButton));
    run(&mut d, G::OpenDropdown);
    let list = d
        .edit
        .as_ref()
        .and_then(|e| e.pick_list.clone())
        .unwrap_or_default();
    assert_eq!(list, ["Low", "Medium", "High"]);
    run(&mut d, G::CancelEdit);
    at(&mut d, 3, 0);
    run(
        &mut d,
        G::BeginEdit {
            text: "Urgent".into(),
        },
    );
    let err = d
        .execute(&Command::Grid(G::CommitEdit { advance: None }))
        .expect_err("refused");
    assert!(!err.to_string().is_empty());
    run(&mut d, G::CancelEdit);
    put(&mut d, 3, 0, "High");
    assert_eq!(text(&d, 3, 0), "High");
    // Circle Invalid Data finds a value typed before the rule.
    put(&mut d, 12, 0, "x");
    select(&mut d, 12, 0, 12, 0);
    run(
        &mut d,
        G::SetValidation {
            rule: Some(GridValidation {
                kind: GridValidationKind::Whole,
                op: GridCompare::Between,
                formula1: "1".into(),
                formula2: "10".into(),
                ..GridValidation::default()
            }),
        },
    );
    run(&mut d, G::CircleInvalid { on: true });
    assert_eq!(d.features.invalid, [kiln_sheet::CellPos::new(12, 0)]);
    select(&mut d, 0, 0, 20, 0);
    run(&mut d, G::SetValidation { rule: None });
    assert!(
        d.workbook
            .sheet(sid(&d))
            .is_some_and(|s| s.meta().validations.is_empty())
    );
}

// ── tables and styles ─────────────────────────────────────────────────────────────────────

#[test]
fn format_as_table_with_a_total_row_and_style_options() {
    let mut d = doc();
    budget(&mut d);
    run(
        &mut d,
        G::CreateTable {
            range: None,
            header: true,
            style: "TableStyleMedium9".into(),
        },
    );
    let t = state(&d).table.expect("table");
    assert_eq!(t.name, "Table1");
    assert_eq!(
        t.range,
        GridRange {
            r0: 0,
            c0: 0,
            r1: 6,
            c1: 2
        }
    );
    assert!(t.options.filter_button && t.options.banded_rows);
    let mut o = t.options;
    o.total_row = true;
    o.banded_columns = true;
    run(&mut d, G::TableOptions { options: o });
    let t = state(&d).table.expect("table");
    assert!(t.options.total_row && t.options.banded_columns);
    assert_eq!(t.range.r1, 7);
    assert_eq!(text(&d, 7, 0), "Total");
    assert_eq!(num(&d, 7, 2), 2930.0);
    run(
        &mut d,
        G::TableTotals {
            col: 2,
            totals: GridTotals::Average,
        },
    );
    assert!((num(&d, 7, 2) - 2930.0 / 6.0).abs() < 1e-9);
    // Structured references work through the new name.
    run(
        &mut d,
        G::RenameTable {
            name: "Costs".into(),
        },
    );
    put(&mut d, 10, 0, "=SUM(Costs[Amount])");
    assert_eq!(num(&d, 10, 0), 2930.0);
    at(&mut d, 1, 0);
    assert!(fails(&mut d, G::RenameTable { name: "A1".into() }).contains("not a valid"));
    run(
        &mut d,
        G::TableStyle {
            style: "TableStyleLight2".into(),
        },
    );
    assert_eq!(state(&d).table.expect("table").style, "TableStyleLight2");
    let fill = d.workbook.table_format(sid(&d), 0, 0).and_then(|x| x.fill);
    run(&mut d, G::ConvertToRange);
    assert!(state(&d).table.is_none());
    let s = sid(&d);
    let style = d
        .workbook
        .styles()
        .get(d.workbook.sheet(s).map_or(0, |x| x.style_at(0, 0)));
    assert_eq!(Some(style.fill), fill, "the table's look stays");
    assert_eq!(num(&d, 10, 0), 2930.0, "the formula still adds the column");
    undo(&mut d);
    assert!(state(&d).table.is_some());
}

#[test]
fn table_styles_and_cell_styles_have_swatches() {
    let d = doc();
    let GridQueryResult::TableStyles(styles) = q(&d, GridQuery::TableStyles) else {
        panic!("styles")
    };
    assert_eq!(styles.len(), 60);
    assert!(
        styles
            .iter()
            .any(|s| s.name == "TableStyleMedium2" && s.header.is_some())
    );
    let GridQueryResult::CellStyles(cells) = q(&d, GridQuery::CellStyles) else {
        panic!("cell styles")
    };
    let good = cells.iter().find(|c| c.name == "Good").expect("Good");
    assert_eq!(good.fill.as_deref(), Some("C6EFCE"));
    assert!(cells.iter().any(|c| c.name == "Heading 1" && c.bold));
}

#[test]
fn cell_styles_and_format_painter_apply_formats() {
    let mut d = doc();
    budget(&mut d);
    at(&mut d, 1, 2);
    run(&mut d, G::ApplyCellStyle { name: "Bad".into() });
    let s = sid(&d);
    let style = |d: &GridDoc, r, c| {
        d.workbook
            .styles()
            .get(d.workbook.sheet(s).map_or(0, |x| x.style_at(r, c)))
    };
    assert_eq!(
        style(&d, 1, 2)
            .font
            .color
            .map(|c| c.resolve(d.workbook.theme(), 0)),
        Some(0x9C_0006)
    );
    select(&mut d, 3, 2, 4, 2);
    run(
        &mut d,
        G::PaintFormat {
            from: GridRange {
                r0: 1,
                c0: 2,
                r1: 1,
                c1: 2,
            },
        },
    );
    assert_eq!(style(&d, 4, 2).fill, style(&d, 1, 2).fill);
    assert!(
        fails(
            &mut d,
            G::ApplyCellStyle {
                name: "Sparkly".into()
            }
        )
        .contains("no cell style")
    );
}

// ── find, replace, Go To Special ──────────────────────────────────────────────────────────

#[test]
fn replace_all_and_find_all_follow_the_options() {
    let mut d = doc();
    budget(&mut d);
    put(&mut d, 8, 0, "=C2*2");
    let spec = |text: &str| GridFindSpec {
        text: text.into(),
        ..GridFindSpec::default()
    };
    let GridQueryResult::Found(hits) = q(
        &d,
        GridQuery::FindAll {
            spec: spec("firing"),
        },
    ) else {
        panic!("found")
    };
    assert_eq!(
        hits.iter().map(|h| h.cell.as_str()).collect::<Vec<_>>(),
        ["$B$4", "$B$7"]
    );
    let GridQueryResult::Found(hits) = q(
        &d,
        GridQuery::FindAll {
            spec: GridFindSpec {
                match_case: true,
                ..spec("firing")
            },
        },
    ) else {
        panic!("found")
    };
    assert!(hits.is_empty());
    let GridQueryResult::Found(hits) = q(&d, GridQuery::FindAll { spec: spec("C2") }) else {
        panic!("found")
    };
    assert_eq!(hits.len(), 1, "formulas are searched");
    let GridQueryResult::Found(hits) = q(
        &d,
        GridQuery::FindAll {
            spec: GridFindSpec {
                look_in: GridLookIn::Values,
                ..spec("840")
            },
        },
    ) else {
        panic!("found")
    };
    assert_eq!(hits.len(), 1, "values are searched");
    run(
        &mut d,
        G::Replace {
            spec: spec("Firing"),
            with: "Kiln".into(),
            all: true,
        },
    );
    assert_eq!(text(&d, 3, 1), "Kiln");
    assert_eq!(text(&d, 6, 1), "Kiln");
    assert_eq!(
        state(&d).message.as_deref(),
        Some("All done. We made 2 replacements.")
    );
    undo(&mut d);
    assert_eq!(text(&d, 3, 1), "Firing");
    // Replace: the active cell, then the next match.
    at(&mut d, 3, 1);
    run(
        &mut d,
        G::Replace {
            spec: GridFindSpec {
                whole_cell: true,
                ..spec("firing")
            },
            with: "Fire".into(),
            all: false,
        },
    );
    assert_eq!(text(&d, 3, 1), "Fire");
    assert_eq!(d.view().sel.active, kiln_sheet::CellPos::new(6, 1));
}

#[test]
fn go_to_special_selects_blanks_formulas_and_the_last_cell() {
    let mut d = doc();
    budget(&mut d);
    put(&mut d, 2, 2, "");
    put(&mut d, 8, 0, "=C2*2");
    select(&mut d, 0, 0, 6, 2);
    run(
        &mut d,
        G::SelectSpecial {
            special: GridSpecial::Blanks,
        },
    );
    assert_eq!(d.view().sel.ranges, [Range::cell(2, 2)]);
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::SelectSpecial {
            special: GridSpecial::Formulas,
        },
    );
    assert_eq!(d.view().sel.ranges, [Range::cell(8, 0)]);
    run(
        &mut d,
        G::SelectSpecial {
            special: GridSpecial::LastCell,
        },
    );
    assert_eq!(d.view().sel.active, kiln_sheet::CellPos::new(8, 2));
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::SelectSpecial {
            special: GridSpecial::CurrentRegion,
        },
    );
    assert_eq!(d.view().sel.ranges, [Range::new(0, 0, 6, 2)]);
    assert!(
        fails(
            &mut d,
            G::SelectSpecial {
                special: GridSpecial::Notes
            }
        )
        .contains("No cells")
    );
}

// ── notes, comments, links ────────────────────────────────────────────────────────────────

#[test]
fn notes_are_added_shown_converted_and_deleted() {
    let mut d = doc();
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::SetNote {
            text: Some("Check the price".into()),
            author: "Ana".into(),
            cell: None,
        },
    );
    assert!(state(&d).note);
    run(&mut d, G::ShowNote { show: None });
    assert_eq!(state(&d).cards.len(), 1);
    run(&mut d, G::ShowNote { show: None });
    assert!(state(&d).cards.is_empty());
    run(&mut d, G::ShowAllNotes { show: true });
    assert_eq!(state(&d).cards.len(), 1);
    at(&mut d, 0, 0);
    run(&mut d, G::NextComment { backward: false });
    assert_eq!(d.view().sel.active, kiln_sheet::CellPos::new(1, 1));
    run(
        &mut d,
        G::ConvertNotes {
            author: "Me".into(),
            now_ms: 1_760_000_000_000,
        },
    );
    let GridQueryResult::Comments(list) = q(&d, GridQuery::Comments) else {
        panic!("comments")
    };
    assert_eq!(list.len(), 1);
    assert!(!list[0].note);
    assert_eq!(list[0].author, "Ana");
    run(&mut d, G::DeleteComment { id: None });
    assert!(!state(&d).any_comments);
}

#[test]
fn a_note_saves_to_the_cell_its_editor_was_opened_on() {
    let mut d = doc();
    // The editor opened on D1; the selection has moved to C4 before it saves.
    at(&mut d, 3, 2);
    run(
        &mut d,
        G::SetNote {
            text: Some("Units shipped".into()),
            author: "Ana".into(),
            cell: Some("D1".into()),
        },
    );
    let GridQueryResult::Comments(list) = q(&d, GridQuery::Comments) else {
        panic!("comments")
    };
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].cell, "D1");
    assert!(!state(&d).note, "C4 has no note");
    // Deleting through the editor removes that note only.
    run(
        &mut d,
        G::SetNote {
            text: None,
            author: String::new(),
            cell: Some("D1".into()),
        },
    );
    assert!(!state(&d).any_comments);
    assert!(
        fails(
            &mut d,
            G::SetNote {
                text: Some("x".into()),
                author: String::new(),
                cell: Some("A1:B2".into()),
            }
        )
        .contains("not a cell")
    );
}

#[test]
fn threaded_comments_take_replies_and_resolve() {
    let mut d = doc();
    at(&mut d, 4, 2);
    let now = 1_760_000_000_000;
    run(
        &mut d,
        G::AddComment {
            text: "Is this right?".into(),
            author: "Ana".into(),
            reply_to: None,
            now_ms: now,
        },
    );
    let GridQueryResult::Comments(list) = q(&d, GridQuery::Comments) else {
        panic!()
    };
    let root = list[0].id.clone();
    assert!(root.starts_with('{') && root.ends_with('}'));
    assert_eq!(list[0].cell, "C5");
    assert!(
        fails(
            &mut d,
            G::AddComment {
                text: "again".into(),
                author: "Ana".into(),
                reply_to: None,
                now_ms: now
            }
        )
        .contains("reply")
    );
    run(
        &mut d,
        G::AddComment {
            text: "Yes".into(),
            author: "Ben".into(),
            reply_to: Some(root.clone()),
            now_ms: now + 1,
        },
    );
    run(
        &mut d,
        G::ResolveComment {
            id: root.clone(),
            done: true,
        },
    );
    let GridQueryResult::Comments(list) = q(&d, GridQuery::Comments) else {
        panic!()
    };
    assert_eq!(list.len(), 2);
    assert!(list.iter().all(|c| c.done));
    assert_eq!(list[1].parent.as_deref(), Some(root.as_str()));
    assert_eq!(d.workbook.persons().len(), 2);
    let reply = list[1].id.clone();
    run(
        &mut d,
        G::EditComment {
            id: reply.clone(),
            text: "Yes, checked".into(),
        },
    );
    run(&mut d, G::DeleteComment { id: Some(root) });
    assert!(
        !state(&d).any_comments,
        "deleting the first comment deletes the thread"
    );
}

#[test]
fn links_go_to_the_web_or_a_place_and_come_off() {
    let mut d = doc();
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::SetLink {
            link: Some(GridLink {
                target: "kiln.example".into(),
                display: "Kiln".into(),
                ..GridLink::default()
            }),
        },
    );
    let l = state(&d).link.expect("link");
    assert_eq!(l.target, "https://kiln.example");
    assert_eq!(text(&d, 0, 0), "Kiln");
    run(
        &mut d,
        G::SetLink {
            link: Some(GridLink {
                location: "Sheet1!B9".into(),
                ..GridLink::default()
            }),
        },
    );
    assert_eq!(state(&d).link.expect("link").location, "Sheet1!B9");
    run(&mut d, G::SetLink { link: None });
    assert!(state(&d).link.is_none());
}

// ── page layout and printing ──────────────────────────────────────────────────────────────

fn numbers(d: &mut GridDoc, rows: u32) {
    let s = sid(d);
    let cells = (0..rows)
        .flat_map(|r| (0..4).map(move |c| (r, c)))
        .map(|(r, c)| kiln_sheet::commands::CellInput {
            row: r,
            col: c,
            content: kiln_sheet::commands::Content::Value(CellValue::Number(f64::from(r * 4 + c))),
            style: None,
        })
        .collect();
    d.exec("test", kiln_sheet::Command::SetCells { sheet: s, cells })
        .expect("cells");
}

fn page(d: &GridDoc) -> GridPageInfo {
    match q(d, GridQuery::PageSetup) {
        GridQueryResult::PageSetup(p) => *p,
        other => panic!("{other:?}"),
    }
}

#[test]
fn page_setup_print_area_titles_and_breaks() {
    let mut d = doc();
    numbers(&mut d, 200);
    assert_eq!(page(&d).pages, 5);
    run(
        &mut d,
        G::PageSetup {
            setup: Box::new(GridPageSetup {
                landscape: Some(true),
                paper: Some(9),
                header: Some("&C&A".into()),
                footer: Some("&RPage &P of &N".into()),
                title_rows: Some("$1:$1".into()),
                gridlines: Some(true),
                ..GridPageSetup::default()
            }),
        },
    );
    let p = page(&d);
    assert!(p.landscape && p.gridlines);
    assert_eq!(p.paper, 9);
    assert_eq!(p.title_rows, "$1:$1");
    assert_eq!(p.header_preview[1], "Sheet1");
    assert_eq!(p.footer_preview[2], format!("Page 1 of {}", p.pages));
    select(&mut d, 0, 0, 99, 3);
    run(
        &mut d,
        G::PrintArea {
            action: GridPrintAreaAction::Set,
        },
    );
    assert_eq!(page(&d).print_area, "$A$1:$D$100");
    assert!(state(&d).print_area);
    run(
        &mut d,
        G::PageSetup {
            setup: Box::new(GridPageSetup {
                fit_width: Some(1),
                fit_height: Some(1),
                ..GridPageSetup::default()
            }),
        },
    );
    assert_eq!(page(&d).pages, 1);
    run(
        &mut d,
        G::PageSetup {
            setup: Box::new(GridPageSetup {
                fit: Some(false),
                ..GridPageSetup::default()
            }),
        },
    );
    at(&mut d, 30, 0);
    run(
        &mut d,
        G::PageBreak {
            action: GridBreakAction::Insert,
        },
    );
    assert_eq!(page(&d).row_breaks, [30]);
    run(
        &mut d,
        G::MovePageBreak {
            columns: false,
            from: 30,
            to: 20,
        },
    );
    assert_eq!(page(&d).row_breaks, [20]);
    run(
        &mut d,
        G::PageBreak {
            action: GridBreakAction::Reset,
        },
    );
    assert!(page(&d).row_breaks.is_empty());
    run(
        &mut d,
        G::PrintArea {
            action: GridPrintAreaAction::Clear,
        },
    );
    assert!(page(&d).print_area.is_empty());
    run(&mut d, G::PrintGridlines);
    assert!(!page(&d).gridlines);
    assert!(
        fails(
            &mut d,
            G::PageSetup {
                setup: Box::new(GridPageSetup {
                    scale: Some(5),
                    ..GridPageSetup::default()
                })
            }
        )
        .contains("scale")
    );
}

#[test]
fn printing_a_sheet_gives_one_pdf_page_per_page_break_preview_page() {
    let mut d = doc();
    numbers(&mut d, 150);
    let pages = page(&d).pages;
    assert!(pages >= 3);
    d.prepare_print();
    let settings = PrintSettings::default();
    let out = d
        .print(&crate::print::PrintJob::Pdf {
            settings: &settings,
            title: "Budget",
        })
        .expect("pdf");
    let crate::print::PrintOutput::Pdf(pdf) = out else {
        panic!("pdf")
    };
    assert!(pdf.bytes.starts_with(b"%PDF"));
    let preview = d
        .print(&crate::print::PrintJob::Preview {
            settings: &settings,
            sheet: 0,
            scale: 1.0,
        })
        .expect("preview");
    let crate::print::PrintOutput::Preview(p) = preview else {
        panic!("preview")
    };
    assert_eq!(p.page_count, pages);
}

/// The acceptance fixture: Page Break Preview counts the pages the PDF prints (4, as
/// LibreOffice prints it).
#[test]
fn print_setup_fixture_prints_the_pages_page_break_preview_shows() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/xlsx/features/print-setup.xlsx");
    let bytes = std::fs::read(&path).expect("fixture");
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
    let mut d = GridDoc::new(read.workbook, Some(read.source), read.warnings, None);
    d.last_view = doc().last_view;
    d.features.view_mode = Some(kiln_protocol::ViewMode::PageBreakPreview);
    let shown = state(&d).pages;
    assert_eq!(shown, 4);
    d.prepare_print();
    let settings = PrintSettings::default();
    let preview = d
        .print(&crate::print::PrintJob::Preview {
            settings: &settings,
            sheet: 0,
            scale: 1.0,
        })
        .expect("preview");
    let crate::print::PrintOutput::Preview(p) = preview else {
        panic!("preview")
    };
    assert_eq!(p.page_count, shown);
}

#[test]
fn page_break_preview_draws_the_pages_and_hits_breaks() {
    let mut d = doc();
    numbers(&mut d, 150);
    d.features.view_mode = Some(kiln_protocol::ViewMode::PageBreakPreview);
    assert!(state(&d).pages >= 3);
    let geo = d.geometry().clone();
    assert!(d.feature_overlay(&geo).len() > 3);
    let p = d.pages().expect("pages");
    let row = p.row_breaks[0];
    let y = geo.rows.offset(row) as f32;
    let hw = d.header_w(&geo);
    d.last_view.scroll_y = y - 200.0;
    let (x, sy) = geo.to_surface(&d.last_view, 40.0, y, hw);
    assert_eq!(
        d.feature_hit(x, sy),
        Some(GridHit::PageBreak {
            columns: false,
            index: row
        })
    );
}

// ── names ─────────────────────────────────────────────────────────────────────────────────

#[test]
fn the_name_manager_defines_edits_and_deletes_names() {
    let mut d = doc();
    budget(&mut d);
    run(
        &mut d,
        G::DefineName {
            name: "Amounts".into(),
            refers_to: "=Sheet1!$C$2:$C$7".into(),
            scope: None,
            comment: "Costs".into(),
            previous: None,
        },
    );
    put(&mut d, 9, 0, "=SUM(Amounts)");
    assert_eq!(num(&d, 9, 0), 2930.0);
    let GridQueryResult::Names(names) = q(&d, GridQuery::Names) else {
        panic!()
    };
    let n = names.iter().find(|n| n.name == "Amounts").expect("name");
    assert_eq!(n.refers_to, "=Sheet1!$C$2:$C$7");
    assert!(n.value.starts_with("{420;310;"));
    assert!(
        fails(
            &mut d,
            G::DefineName {
                name: "Amounts".into(),
                refers_to: "=1".into(),
                scope: None,
                comment: String::new(),
                previous: None
            }
        )
        .contains("already exists")
    );
    run(
        &mut d,
        G::DefineName {
            name: "Costs".into(),
            refers_to: "=Sheet1!$C$2:$C$4".into(),
            scope: Some(0),
            comment: String::new(),
            previous: Some(("Amounts".into(), None)),
        },
    );
    let GridQueryResult::Names(names) = q(&d, GridQuery::Names) else {
        panic!()
    };
    assert!(
        names
            .iter()
            .any(|n| n.name == "Costs" && n.scope == Some(0))
    );
    assert!(!names.iter().any(|n| n.name == "Amounts"));
    run(
        &mut d,
        G::DeleteName {
            name: "Costs".into(),
            scope: Some(0),
        },
    );
    select(&mut d, 0, 0, 6, 2);
    run(
        &mut d,
        G::CreateNames {
            top: true,
            left: false,
            bottom: false,
            right: false,
        },
    );
    let GridQueryResult::Names(names) = q(&d, GridQuery::Names) else {
        panic!()
    };
    let amount = names.iter().find(|n| n.name == "Amount").expect("Amount");
    assert_eq!(amount.refers_to, "=Sheet1!$C$2:$C$7");
}

// ── auditing and Formula Lab ──────────────────────────────────────────────────────────────

#[test]
fn trace_precedents_and_dependents_draw_arrows_level_by_level() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 0, "=A1*2");
    put(&mut d, 2, 0, "=A2+B1");
    put(&mut d, 0, 1, "5");
    at(&mut d, 2, 0);
    run(&mut d, G::TracePrecedents);
    assert_eq!(d.features.arrows.len(), 2);
    assert!(state(&d).arrows);
    run(&mut d, G::TracePrecedents);
    assert_eq!(
        d.features.arrows.len(),
        3,
        "one level further: A2's precedent A1"
    );
    let geo = d.geometry().clone();
    assert!(!d.feature_overlay(&geo).is_empty());
    run(
        &mut d,
        G::RemoveArrows {
            which: GridArrows::All,
        },
    );
    assert!(d.features.arrows.is_empty());
    at(&mut d, 0, 0);
    run(&mut d, G::TraceDependents);
    assert_eq!(d.features.arrows.len(), 1);
    assert!(d.features.arrows[0].dependent);
    at(&mut d, 5, 5);
    assert!(fails(&mut d, G::TraceDependents).contains("No formulas"));
}

#[test]
fn formula_lab_shows_the_steps_of_an_if_over_a_sum() {
    let mut d = doc();
    put(&mut d, 0, 0, "1");
    put(&mut d, 1, 0, "2");
    put(&mut d, 2, 0, "3");
    put(&mut d, 0, 1, "=IF(SUM(A1:A3)>5,\"x\",\"y\")");
    at(&mut d, 0, 1);
    run(&mut d, G::FormulaLab { open: true });
    let GridQueryResult::Trace(t) = q(&d, GridQuery::FormulaTrace) else {
        panic!()
    };
    assert_eq!(t.formula, "=IF(SUM(A1:A3)>5,\"x\",\"y\")");
    assert_eq!(t.value, "x");
    let root = t.root.expect("tree");
    assert_eq!(root.label, "IF");
    assert_eq!(root.value, "\"x\"");
    assert!(!root.children[2].evaluated, "the untaken branch");
    let texts: Vec<&str> = t.steps.iter().map(|s| s.text.as_str()).collect();
    assert_eq!(
        texts.first().copied(),
        Some("=IF(SUM(A1:A3)>5,\"x\",\"y\")")
    );
    assert!(
        texts.contains(&"=IF(SUM({1;2;3})>5,\"x\",\"y\")"),
        "{texts:?}"
    );
    assert!(texts.contains(&"=IF(6>5,\"x\",\"y\")"), "{texts:?}");
    assert!(texts.contains(&"=IF(TRUE,\"x\",\"y\")"), "{texts:?}");
    assert_eq!(texts.last().copied(), Some("=\"x\""));
    // The underline of the first step is the reference.
    let first = &t.steps[0];
    assert_eq!(
        &first.text[first.start as usize..first.end as usize],
        "A1:A3"
    );
    // The lab follows the active cell.
    at(&mut d, 0, 0);
    let GridQueryResult::Trace(t) = q(&d, GridQuery::FormulaTrace) else {
        panic!()
    };
    assert!(t.root.is_none() && t.message.is_some());
}

#[test]
fn error_checking_watches_and_calculation_options() {
    let mut d = doc();
    put(&mut d, 0, 0, "=1/0");
    put(&mut d, 1, 0, "=NOPE()");
    at(&mut d, 0, 0);
    let GridQueryResult::Errors(errors) = q(&d, GridQuery::Errors) else {
        panic!()
    };
    assert_eq!(errors.len(), 2);
    assert_eq!(errors[0].title, "Divide by Zero Error");
    run(
        &mut d,
        G::Watch {
            add: true,
            sheet: 0,
            row: 0,
            col: 0,
        },
    );
    assert_eq!(state(&d).watches[0].value, "#DIV/0!");
    run(
        &mut d,
        G::Watch {
            add: false,
            sheet: 0,
            row: 0,
            col: 0,
        },
    );
    assert!(state(&d).watches.is_empty());
    put(&mut d, 3, 0, "2");
    put(&mut d, 4, 0, "=A4*2");
    run(
        &mut d,
        G::CalculationMode {
            mode: GridCalcMode::Manual,
        },
    );
    assert_eq!(state(&d).calc, Some(GridCalcMode::Manual));
    put(&mut d, 3, 0, "5");
    assert_eq!(num(&d, 4, 0), 4.0, "manual: not recalculated yet");
    run(&mut d, G::CalculateNow);
    assert_eq!(num(&d, 4, 0), 10.0);
}

// ── data tools ────────────────────────────────────────────────────────────────────────────

#[test]
fn csv_and_json_imports_land_on_a_new_sheet_or_at_the_cell() {
    let mut d = doc();
    let csv = GridImport {
        bytes: "Item;Qty\nClay;3\nGlaze;2\n".as_bytes().to_vec(),
        name: "orders.csv".into(),
        ..GridImport::default()
    };
    let GridQueryResult::ImportPreview(p) = q(
        &d,
        GridQuery::ImportPreview {
            import: Box::new(csv.clone()),
        },
    ) else {
        panic!()
    };
    assert_eq!(p.delimiter, ";");
    assert_eq!(p.rows[1], ["Clay", "3"]);
    run(
        &mut d,
        G::Import {
            import: Box::new(csv),
        },
    );
    assert_eq!(d.workbook.sheets().len(), 2);
    assert_eq!(d.active, 1);
    assert_eq!(d.workbook.sheets()[1].name(), "orders");
    assert_eq!(num(&d, 1, 1), 3.0);
    let json = GridImport {
        bytes: br#"[{"a":1,"b":"x"},{"a":2,"b":"y"}]"#.to_vec(),
        name: "data.json".into(),
        format: GridImportFormat::Json,
        at_cell: true,
        ..GridImport::default()
    };
    at(&mut d, 5, 3);
    run(
        &mut d,
        G::Import {
            import: Box::new(json),
        },
    );
    assert_eq!(text(&d, 5, 3), "a");
    assert_eq!(text(&d, 7, 4), "y");
    undo(&mut d);
    assert_eq!(text(&d, 5, 3), "");
}

#[test]
fn text_to_columns_and_remove_duplicates() {
    let mut d = doc();
    put(&mut d, 0, 0, "Clay,3");
    put(&mut d, 1, 0, "Glaze,007");
    select(&mut d, 0, 0, 1, 0);
    run(
        &mut d,
        G::TextToColumns {
            options: GridTextToColumns {
                delimited: true,
                delimiters: ",".into(),
                qualifier: "\"".into(),
                text_columns: vec![1],
                ..GridTextToColumns::default()
            },
        },
    );
    assert_eq!(text(&d, 0, 0), "Clay");
    assert_eq!(text(&d, 1, 1), "007");
    let mut d = doc();
    budget(&mut d);
    put(&mut d, 7, 0, "Clay");
    put(&mut d, 7, 1, "Materials");
    put(&mut d, 7, 2, "420");
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::RemoveDuplicates {
            columns: vec![],
            header: None,
        },
    );
    assert_eq!(text(&d, 7, 0), "");
    assert_eq!(
        state(&d).message.as_deref(),
        Some("1 duplicate value found and removed; 6 unique values remain.")
    );
}

#[test]
fn group_hide_and_show_detail() {
    let mut d = doc();
    budget(&mut d);
    select(&mut d, 1, 0, 3, 0);
    run(&mut d, G::Group { columns: false });
    let s = sid(&d);
    assert_eq!(
        d.workbook.sheet(s).map(|x| x.rows().outline_level(2)),
        Some(1)
    );
    at(&mut d, 2, 0);
    run(&mut d, G::OutlineDetail { show: false });
    assert!(hidden(&d, 1) && hidden(&d, 3) && !hidden(&d, 4));
    at(&mut d, 4, 0);
    run(&mut d, G::OutlineDetail { show: true });
    assert!(!hidden(&d, 2));
    select(&mut d, 1, 0, 3, 0);
    run(&mut d, G::Ungroup { columns: false });
    assert_eq!(
        d.workbook.sheet(s).map(|x| x.rows().outline_level(2)),
        Some(0)
    );
    assert!(fails(&mut d, G::Ungroup { columns: false }).contains("not grouped"));
}

#[test]
fn spelling_finds_changes_and_ignores_words() {
    let mut d = doc();
    put(&mut d, 0, 0, "Blue glazze");
    put(&mut d, 1, 0, "teh kiln");
    at(&mut d, 0, 0);
    let GridQueryResult::Spelling(Some(hit)) = q(&d, GridQuery::Spelling) else {
        panic!("a misspelling")
    };
    assert_eq!(hit.word, "glazze");
    assert!(hit.suggestions.iter().any(|s| s == "glaze"));
    run(
        &mut d,
        G::SpellChange {
            sheet: 0,
            row: 0,
            col: 0,
            offset: hit.offset,
            word: hit.word.clone(),
            with: "glaze".into(),
            all: false,
        },
    );
    assert_eq!(text(&d, 0, 0), "Blue glaze");
    let GridQueryResult::Spelling(Some(hit)) = q(&d, GridQuery::Spelling) else {
        panic!()
    };
    assert_eq!(hit.word, "teh");
    run(
        &mut d,
        G::SpellIgnore {
            word: "teh".into(),
            add: false,
        },
    );
    let GridQueryResult::Spelling(next) = q(&d, GridQuery::Spelling) else {
        panic!()
    };
    assert!(next.is_none_or(|h| h.word != "teh"));
}

#[test]
fn imports_read_decimal_commas_and_ranges_export_as_csv_tsv_and_json() {
    let mut d = doc();
    let csv = GridImport {
        bytes: "Item;Price\nClay;1.234,50\nGlaze;0,75\n"
            .as_bytes()
            .to_vec(),
        name: "prices.csv".into(),
        decimal_comma: true,
        at_cell: true,
        ..GridImport::default()
    };
    at(&mut d, 0, 0);
    run(
        &mut d,
        G::Import {
            import: Box::new(csv),
        },
    );
    assert_eq!(num(&d, 1, 1), 1234.5);
    assert_eq!(num(&d, 2, 1), 0.75);
    at(&mut d, 1, 0);
    let GridQueryResult::Export(f) = q(
        &d,
        GridQuery::Export {
            format: GridExportFormat::Json,
            selection: true,
        },
    ) else {
        panic!("export")
    };
    assert_eq!(f.name, "Budget.json");
    let text = String::from_utf8(f.bytes).expect("utf-8");
    assert!(
        text.contains("\"Item\": \"Clay\"") || text.contains("\"Item\":\"Clay\""),
        "{text}"
    );
    let GridQueryResult::Export(f) = q(
        &d,
        GridQuery::Export {
            format: GridExportFormat::Tsv,
            selection: false,
        },
    ) else {
        panic!("export")
    };
    assert!(String::from_utf8_lossy(&f.bytes).starts_with("Item\tPrice"));
    let GridQueryResult::Export(f) = q(
        &d,
        GridQuery::Export {
            format: GridExportFormat::Csv,
            selection: false,
        },
    ) else {
        panic!("export")
    };
    assert!(
        f.bytes.starts_with(&[0xEF, 0xBB, 0xBF]),
        "CSV UTF-8 has a byte order mark"
    );
}

// ── round trip ────────────────────────────────────────────────────────────────────────────

/// Every feature at once, saved as .xlsx and opened again: what the file keeps.
#[test]
fn every_feature_survives_an_xlsx_round_trip() {
    let mut d = doc();
    budget(&mut d);
    put(&mut d, 9, 0, "=SUM(C2:C7)");
    // Conditional formats, a validation list, a table with a total row, a filter on it.
    run(
        &mut d,
        G::AddConditionalFormat {
            rule: rule(
                "C2:C7",
                GridCfRuleKind::DataBar {
                    min: GridCfValue {
                        kind: GridCfValueKind::Automatic,
                        value: String::new(),
                        gte: true,
                    },
                    max: GridCfValue {
                        kind: GridCfValueKind::Automatic,
                        value: String::new(),
                        gte: true,
                    },
                    color: "638EC6".into(),
                    gradient: true,
                    show_value: true,
                    negative_color: None,
                    border_color: None,
                },
                None,
            ),
        },
    );
    run(
        &mut d,
        G::AddConditionalFormat {
            rule: rule(
                "B2:B7",
                GridCfRuleKind::Text {
                    rule: GridTextRule::Contains,
                    text: "Fir".into(),
                },
                Some(GridCfStyle {
                    fill: Some("FFC7CE".into()),
                    font_color: Some("9C0006".into()),
                    ..GridCfStyle::default()
                }),
            ),
        },
    );
    run(
        &mut d,
        G::CreateTable {
            range: Some(GridRange {
                r0: 0,
                c0: 0,
                r1: 6,
                c1: 2,
            }),
            header: true,
            style: "TableStyleMedium9".into(),
        },
    );
    at(&mut d, 1, 0);
    let t = state(&d).table.expect("table");
    run(
        &mut d,
        G::TableOptions {
            options: GridTableOptions {
                total_row: true,
                ..t.options
            },
        },
    );
    // A total row cell has an in-cell dropdown button (its functions).
    let total = state(&d).table.expect("table").range.r1;
    at(&mut d, total, 2);
    {
        let geo = d.geometry().clone();
        let hw = d.header_w(&geo);
        let cell = geo.display_rect(total, 2);
        let (x, y) = geo.to_surface(&d.last_view, cell.right() + 8.0, cell.bottom() - 6.0, hw);
        assert_eq!(d.feature_hit(x, y), Some(GridHit::ListButton));
    }
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Values {
                values: vec!["Firing".into(), "Materials".into()],
                blanks: false,
            }),
        },
    );
    select(&mut d, 11, 0, 20, 0);
    run(
        &mut d,
        G::SetValidation {
            rule: Some(GridValidation {
                kind: GridValidationKind::List,
                formula1: "Low,Medium,High".into(),
                allow_blank: true,
                dropdown: true,
                show_input: true,
                input_title: "Priority".into(),
                input: "Pick one".into(),
                show_error: true,
                ..GridValidation::default()
            }),
        },
    );
    // A note, a thread with a reply, a link, names, page setup with titles and a break.
    at(&mut d, 1, 0);
    run(
        &mut d,
        G::SetNote {
            text: Some("Bought in bulk".into()),
            author: "Ana".into(),
            cell: None,
        },
    );
    at(&mut d, 3, 0);
    run(
        &mut d,
        G::AddComment {
            text: "Why so much gas?".into(),
            author: "Ana".into(),
            reply_to: None,
            now_ms: 1_760_000_000_000,
        },
    );
    let GridQueryResult::Comments(list) = q(&d, GridQuery::Comments) else {
        panic!()
    };
    let root = list
        .iter()
        .find(|c| !c.note)
        .map(|c| c.id.clone())
        .expect("thread");
    run(
        &mut d,
        G::AddComment {
            text: "Two firings".into(),
            author: "Ben".into(),
            reply_to: Some(root),
            now_ms: 1_760_000_100_000,
        },
    );
    at(&mut d, 12, 3);
    run(
        &mut d,
        G::SetLink {
            link: Some(GridLink {
                target: "https://kiln.example/".into(),
                display: "Kiln".into(),
                ..GridLink::default()
            }),
        },
    );
    run(
        &mut d,
        G::DefineName {
            name: "Amounts".into(),
            refers_to: "=Sheet1!$C$2:$C$7".into(),
            scope: None,
            comment: "The costs".into(),
            previous: None,
        },
    );
    run(
        &mut d,
        G::PageSetup {
            setup: Box::new(GridPageSetup {
                landscape: Some(true),
                paper: Some(9),
                fit_width: Some(1),
                fit_height: Some(0),
                title_rows: Some("$1:$1".into()),
                header: Some("&C&A".into()),
                footer: Some("&RPage &P of &N".into()),
                gridlines: Some(true),
                ..GridPageSetup::default()
            }),
        },
    );
    at(&mut d, 30, 0);
    run(
        &mut d,
        G::PageBreak {
            action: GridBreakAction::Insert,
        },
    );
    select(&mut d, 14, 0, 16, 0);
    run(&mut d, G::Group { columns: false });
    let bytes = d.save(kiln_protocol::FileFormat::Xlsx).expect("saved");
    if let Ok(dir) = std::env::var("KILN_GRID_ROUNDTRIP") {
        let _ = std::fs::write(std::path::Path::new(&dir).join("features.xlsx"), &bytes);
    }
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("reopened");
    let back = GridDoc::new(read.workbook, Some(read.source), read.warnings, None);
    assert_eq!(back.workbook.visible_sheet_count(), 1);
    let GridQueryResult::Rules(rules) = q(&back, GridQuery::ConditionalFormats { sheet: true })
    else {
        panic!()
    };
    assert_eq!(rules.len(), 2, "{rules:?}");
    assert!(
        rules
            .iter()
            .any(|r| matches!(r.kind, GridCfRuleKind::DataBar { .. }))
    );
    assert!(
        rules
            .iter()
            .any(|r| matches!(&r.kind, GridCfRuleKind::Text { text, .. } if text == "Fir"))
    );
    let GridQueryResult::Comments(comments) = q(&back, GridQuery::Comments) else {
        panic!()
    };
    assert!(
        comments
            .iter()
            .any(|c| c.note && c.text == "Bought in bulk"),
        "{comments:?}"
    );
    assert_eq!(
        comments.iter().filter(|c| !c.note).count(),
        2,
        "{comments:?}"
    );
    let GridQueryResult::Names(names) = q(&back, GridQuery::Names) else {
        panic!()
    };
    assert!(
        names
            .iter()
            .any(|n| n.name == "Amounts" && n.comment == "The costs")
    );
    assert!(names.iter().any(|n| n.table && n.name == "Table1"));
    let GridQueryResult::PageSetup(page) = q(&back, GridQuery::PageSetup) else {
        panic!()
    };
    assert!(page.landscape && page.fit && page.gridlines);
    assert_eq!(page.paper, 9);
    assert_eq!(page.title_rows, "$1:$1");
    assert_eq!(page.header, "&C&A");
    assert_eq!(page.row_breaks, [30]);
    let wb = &back.workbook;
    let sid = wb.first_sheet().expect("sheet");
    let sheet = wb.sheet(sid).expect("sheet");
    let t = sheet.tables().first().expect("table");
    assert!(t.totals_row);
    assert_eq!(
        t.style.as_ref().map(|s| s.name.as_str()),
        Some("TableStyleMedium9")
    );
    assert!(
        t.auto_filter
            .as_ref()
            .is_some_and(|f| f.columns.iter().any(|c| c.filter.is_some()))
    );
    assert!(
        sheet.is_row_hidden(4),
        "the filter's hidden rows stay hidden"
    );
    assert_eq!(
        wb.display_text(sid, 7, 2),
        "2550",
        "SUBTOTAL(109) counts the visible rows"
    );
    assert!(
        sheet
            .validation_at(11, 0)
            .is_some_and(|v| v.formula1.as_deref() == Some("\"Low,Medium,High\""))
    );
    assert!(
        sheet
            .hyperlink_at(12, 3)
            .is_some_and(|h| h.target.as_deref() == Some("https://kiln.example/"))
    );
    assert_eq!(sheet.rows().outline_level(15), 1);
}

// ── pictures and shapes ───────────────────────────────────────────────────────────────────

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::RgbaImage::from_pixel(w, h, image::Rgba([194, 72, 28, 255]))
        .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
        .expect("png");
    out
}

fn drawing(d: &GridDoc) -> kiln_protocol::commands::GridDrawing {
    let GridQueryResult::State(s) = q(d, GridQuery::State) else {
        panic!()
    };
    s.drawing.expect("a drawing is selected")
}

fn format(d: &mut GridDoc, f: GridObjectFormat) {
    run(
        d,
        G::FormatObject {
            format: Box::new(f),
        },
    );
}

#[test]
fn insert_picture_places_it_at_the_active_cell_and_draws_it() {
    let mut d = doc();
    at(&mut d, 2, 1);
    run(
        &mut d,
        G::InsertPicture {
            name: "logo.png".into(),
            bytes: png(200, 100),
        },
    );
    let g = drawing(&d);
    assert_eq!(g.kind, GridDrawingKind::Picture);
    assert_eq!(g.name, "Picture 1");
    assert_eq!((g.x, g.y, g.width, g.height), (64.0, 40.0, 200.0, 100.0));
    let o = g.object.expect("object");
    assert!(o.lock_aspect);
    assert_eq!((o.natural_width, o.natural_height), (200.0, 100.0));
    // A tile and its image for the surface.
    let geo = d.geometry().clone();
    let places = d.object_places(&geo);
    assert_eq!(places.len(), 1);
    let ups = d.object_images(false);
    assert_eq!(ups.len(), 1);
    assert_eq!((ups[0].width, ups[0].height), (200, 100));
    assert!(d.object_images(false).is_empty(), "sent once");
    assert_eq!(d.object_images(true).len(), 1, "again after a reset");
    // Undo removes it.
    assert!(d.undo());
    let GridQueryResult::State(s) = q(&d, GridQuery::State) else {
        panic!()
    };
    assert!(s.drawing.is_none());
    assert!(
        fails(
            &mut d,
            G::InsertPicture {
                name: "x.png".into(),
                bytes: b"not a picture".to_vec()
            }
        )
        .contains("not a picture")
    );
}

#[test]
fn picture_format_crops_sizes_and_describes_a_picture() {
    let mut d = doc();
    run(
        &mut d,
        G::InsertPicture {
            name: "logo.png".into(),
            bytes: png(200, 100),
        },
    );
    // Locked proportions: a new width sets the height.
    format(
        &mut d,
        GridObjectFormat {
            width: Some(100.0),
            ..GridObjectFormat::default()
        },
    );
    let g = drawing(&d);
    assert_eq!((g.width, g.height), (100.0, 50.0));
    // A 25 % crop on the left keeps the scale: the frame loses a quarter of its width.
    format(
        &mut d,
        GridObjectFormat {
            crop: Some([25.0, 0.0, 0.0, 0.0]),
            ..GridObjectFormat::default()
        },
    );
    let g = drawing(&d);
    assert_eq!((g.x, g.width), (25.0, 75.0));
    assert_eq!(
        g.object.as_ref().map(|o| o.crop),
        Some([25.0, 0.0, 0.0, 0.0])
    );
    format(
        &mut d,
        GridObjectFormat {
            alt_text: Some("The kiln logo".into()),
            name: Some("Logo".into()),
            placement: Some(GridPlacement::Fixed),
            ..GridObjectFormat::default()
        },
    );
    let g = drawing(&d);
    assert_eq!(g.name, "Logo");
    let o = g.object.expect("object");
    assert_eq!(o.alt_text, "The kiln logo");
    assert_eq!(o.placement, GridPlacement::Fixed);
    // Reset Picture and Size: no crop, its own size.
    run(&mut d, G::ResetPicture { size: true });
    let g = drawing(&d);
    assert_eq!((g.width, g.height), (200.0, 100.0));
    assert_eq!(g.object.map(|o| o.crop), Some([0.0; 4]));
    assert!(
        fails(
            &mut d,
            G::FormatObject {
                format: Box::new(GridObjectFormat {
                    crop: Some([60.0, 0.0, 50.0, 0.0]),
                    ..GridObjectFormat::default()
                })
            }
        )
        .contains("percentages")
    );
}

#[test]
fn shapes_hold_text_fill_and_outline_and_move_like_charts() {
    let mut d = doc();
    at(&mut d, 1, 1);
    run(
        &mut d,
        G::InsertShape {
            preset: "roundRect".into(),
            text: Some("Firing".into()),
        },
    );
    let g = drawing(&d);
    assert_eq!(g.kind, GridDrawingKind::Shape);
    let o = g.object.clone().expect("object");
    assert_eq!(
        (o.preset.as_str(), o.fill.as_str(), o.text.as_str()),
        ("roundRect", "scheme:accent1", "Firing")
    );
    format(
        &mut d,
        GridObjectFormat {
            fill: Some("c2481c".into()),
            line: Some("none".into()),
            text_size: Some(14.0),
            ..GridObjectFormat::default()
        },
    );
    let o = drawing(&d).object.expect("object");
    assert_eq!(
        (o.fill.as_str(), o.line.as_str(), o.text_size),
        ("C2481C", "none", 14.0)
    );
    // Move and size it as a chart is moved (PlaceDrawing), then click elsewhere.
    run(
        &mut d,
        G::PlaceDrawing {
            index: g.index,
            x: 300.0,
            y: 100.0,
            width: 150.0,
            height: 80.0,
        },
    );
    let g = drawing(&d);
    assert_eq!((g.x, g.y, g.width, g.height), (300.0, 100.0, 150.0, 80.0));
    // A click inside it hits it.
    let geo = d.geometry().clone();
    let hw = d.header_w(&geo);
    let (sx, sy) = geo.to_surface(&d.last_view, 350.0, 130.0, hw);
    assert_eq!(
        d.hit_chart(sx, sy),
        Some(GridHit::Drawing { index: g.index })
    );
    // A line, and Bring to Front / Send to Back.
    run(
        &mut d,
        G::InsertShape {
            preset: "line".into(),
            text: None,
        },
    );
    assert_eq!(drawing(&d).kind, GridDrawingKind::Line);
    run(
        &mut d,
        G::ArrangeDrawing {
            arrange: GridArrange::Back,
        },
    );
    assert_eq!(drawing(&d).index, 0);
    run(&mut d, G::DeleteDrawing);
    let GridQueryResult::State(s) = q(&d, GridQuery::State) else {
        panic!()
    };
    assert!(s.drawing.is_none());
    assert!(
        fails(
            &mut d,
            G::InsertShape {
                preset: "squircle".into(),
                text: None
            }
        )
        .contains("not a shape")
    );
}

#[test]
fn pictures_and_shapes_survive_an_xlsx_round_trip() {
    let mut d = doc();
    budget(&mut d);
    at(&mut d, 9, 0);
    run(
        &mut d,
        G::InsertPicture {
            name: "logo.png".into(),
            bytes: png(120, 60),
        },
    );
    format(
        &mut d,
        GridObjectFormat {
            alt_text: Some("Logo".into()),
            crop: Some([10.0, 0.0, 0.0, 0.0]),
            ..GridObjectFormat::default()
        },
    );
    at(&mut d, 9, 4);
    run(
        &mut d,
        G::InsertShape {
            preset: "ellipse".into(),
            text: Some("Kiln\nhire".into()),
        },
    );
    format(
        &mut d,
        GridObjectFormat {
            fill: Some("C2481C".into()),
            ..GridObjectFormat::default()
        },
    );
    let bytes = d.save(kiln_protocol::FileFormat::Xlsx).expect("saved");
    if let Ok(dir) = std::env::var("KILN_GRID_ROUNDTRIP") {
        let _ = std::fs::write(std::path::Path::new(&dir).join("objects.xlsx"), &bytes);
    }
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("reopened");
    let back = GridDoc::new(read.workbook, Some(read.source), read.warnings, None);
    let sheet = &back.workbook.sheets()[0];
    let ds = &sheet.meta().drawings;
    assert_eq!(ds.len(), 2, "{ds:?}");
    let pic = ds
        .iter()
        .find(|x| x.kind == kiln_sheet::meta::DrawingKind::Picture)
        .expect("picture");
    assert_eq!(pic.alt_text.as_deref(), Some("Logo"));
    assert_eq!(pic.look.crop, [10_000, 0, 0, 0]);
    assert!(pic.look.lock_aspect);
    assert_eq!(
        back.picture_bytes(&pic.reference).map(|b| b.len()),
        Some(png(120, 60).len())
    );
    let shape = ds
        .iter()
        .find(|x| x.kind == kiln_sheet::meta::DrawingKind::Shape)
        .expect("shape");
    assert_eq!(shape.look.preset.as_deref(), Some("ellipse"));
    assert_eq!(shape.look.fill.as_deref(), Some("C2481C"));
    assert_eq!(shape.look.text, "Kiln\nhire");
    // Saving again without changes keeps the drawing part as written.
    let again = back
        .save(kiln_protocol::FileFormat::Xlsx)
        .expect("saved again");
    let read2 = kiln_io_xlsx::read_xlsx(&again).expect("reopened again");
    assert_eq!(read2.workbook.sheets()[0].meta().drawings.len(), 2);
    assert!(read2.warnings.is_empty(), "{:?}", read2.warnings);
}

// ── function library ──────────────────────────────────────────────────────────────────────

/// The functions of each category of Formulas ▸ Function Library, as the ribbon's menus list
/// them (`packages/commands/src/catalog/grid-functions.ts`, written by this test with
/// `KILN_UPDATE_FUNCTIONS=1`).
#[test]
fn the_function_library_menus_list_the_engine_functions() {
    const CATEGORIES: [&str; 11] = [
        "Financial",
        "Logical",
        "Text",
        "Date & Time",
        "Lookup & Reference",
        "Math & Trig",
        "Statistical",
        "Engineering",
        "Information",
        "Compatibility",
        "Web",
    ];
    let mut out = String::from(
        "/**\n * The functions of each Formulas ▸ Function Library menu, from the engine's documentation\n * (generated by kiln-session's `the_function_library_menus_list_the_engine_functions` test with\n * `KILN_UPDATE_FUNCTIONS=1`; do not edit).\n */\n// biome-ignore format: generated, one category per line\nconst NAMES: Readonly<Record<string, string>> = {\n",
    );
    for c in CATEGORIES {
        let mut names: Vec<&str> = kiln_formula::functions::docs()
            .iter()
            .filter(|d| d.category_name == c)
            .map(|d| d.name.as_str())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert!(!names.is_empty(), "{c} has functions");
        let key = if c.chars().all(|x| x.is_ascii_alphabetic()) {
            c.to_owned()
        } else {
            format!("'{c}'")
        };
        out.push_str(&format!("  {key}: '{}',\n", names.join(" ")));
    }
    out.push_str("};\n\n/** Category → function names. */\nexport const GRID_FUNCTION_MENUS: Readonly<Record<string, readonly string[]>> = Object.fromEntries(\n  Object.entries(NAMES).map(([k, v]) => [k, v.split(' ')]),\n);\n");
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packages/commands/src/catalog/grid-functions.ts");
    if std::env::var_os("KILN_UPDATE_FUNCTIONS").is_some() {
        std::fs::write(&path, &out).expect("written");
    }
    let file = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        file == out,
        "grid-functions.ts is stale: run with KILN_UPDATE_FUNCTIONS=1"
    );
}

#[test]
fn function_arguments_reads_docs_and_evaluates_formulas() {
    let mut d = doc();
    put(&mut d, 0, 0, "4");
    put(&mut d, 1, 0, "6");
    let GridQueryResult::FunctionDoc(f) = q(&d, GridQuery::FunctionDoc { name: "pmt".into() })
    else {
        panic!()
    };
    assert_eq!(f.name, "PMT");
    assert_eq!(
        f.args.iter().map(|a| a.name.as_str()).collect::<Vec<_>>()[..3],
        ["rate", "nper", "pv"]
    );
    assert!(f.args[3].optional);
    let e = Query::Grid(GridQuery::Evaluate {
        formula: "=SUM(A1:A2)*2".into(),
    });
    d.prepare_query(&e);
    let QueryResult::Grid(GridQueryResult::Evaluated(v)) = d.app_query(&e).expect("evaluated")
    else {
        panic!()
    };
    assert_eq!(v, "20");
    let bad = Query::Grid(GridQuery::Evaluate {
        formula: "=SUM(".into(),
    });
    d.prepare_query(&bad);
    let QueryResult::Grid(GridQueryResult::Evaluated(v)) = d.app_query(&bad).expect("answered")
    else {
        panic!()
    };
    assert!(v.starts_with("Invalid"), "{v}");
}

#[test]
fn circular_references_are_listed_and_iteration_resolves_them() {
    let mut d = doc();
    put(&mut d, 0, 0, "=B1+1");
    put(&mut d, 0, 1, "=A1/2");
    assert_eq!(state(&d).circular, ["Sheet1!A1", "Sheet1!B1"]);
    run(
        &mut d,
        G::IterativeCalculation {
            enabled: true,
            max_iterations: 100,
            max_change: 0.000_001,
        },
    );
    assert_eq!(state(&d).iterative, (true, 100, 0.000_001));
    run(&mut d, G::CalculateNow);
    // A = B + 1, B = A / 2 converges to A = 2, B = 1.
    let sid = sid(&d);
    let a: f64 = d
        .workbook
        .display_text(sid, 0, 0)
        .parse()
        .expect("a number");
    assert!((a - 2.0).abs() < 0.001, "{a}");
    assert!(
        fails(
            &mut d,
            G::IterativeCalculation {
                enabled: true,
                max_iterations: 0,
                max_change: 0.1
            }
        )
        .contains("Maximum Iterations")
    );
    assert!(d.undo());
    assert!(!state(&d).iterative.0);
}

#[test]
fn a_formula_in_an_empty_table_column_becomes_a_calculated_column() {
    let mut d = doc();
    budget(&mut d);
    run(
        &mut d,
        G::CreateTable {
            range: Some(GridRange {
                r0: 0,
                c0: 0,
                r1: 6,
                c1: 3,
            }),
            header: true,
            style: "TableStyleMedium2".into(),
        },
    );
    put(&mut d, 2, 3, "=[@Amount]*2");
    let s = sid(&d);
    let shown: Vec<String> = (1..=6).map(|r| d.workbook.display_text(s, r, 3)).collect();
    assert_eq!(shown, ["840", "620", "2360", "0", "760", "1280"]);
    let t = d
        .workbook
        .sheet(s)
        .and_then(|x| x.tables().first().cloned())
        .expect("table");
    assert_eq!(
        t.columns[3].calculated_formula.as_deref(),
        Some("[@Amount]*2")
    );
    // One undo step takes the column back.
    assert!(d.undo());
    assert_eq!(d.workbook.display_text(s, 1, 3), "");
    // A formula beside other values only fills its own cell.
    put(&mut d, 1, 3, "x");
    put(&mut d, 2, 3, "=1+1");
    assert_eq!(d.workbook.display_text(s, 3, 3), "");
    // The column's formula survives a save.
    assert!(d.undo() && d.undo());
    put(&mut d, 2, 3, "=[@Amount]*2");
    let bytes = d.save(kiln_protocol::FileFormat::Xlsx).expect("saved");
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("read");
    let t = read.workbook.sheets()[0].tables()[0].clone();
    assert_eq!(
        t.columns[3].calculated_formula.as_deref(),
        Some("[@Amount]*2")
    );
}

#[test]
fn dynamic_and_colour_criteria_filter_rows() {
    let mut d = doc();
    budget(&mut d);
    run(&mut d, G::ToggleFilter);
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 2,
            filter: Some(GridColumnFilter::Dynamic {
                kind: GridDynamicFilter::AboveAverage,
            }),
        },
    );
    // The average of 420, 310, 1180, 0, 380, 640 is 488⅓: Gas and Kiln hire.
    let shown: Vec<u32> = (1..=6).filter(|&r| !hidden(&d, r)).collect();
    assert_eq!(shown, [3, 6]);
    run(&mut d, G::ClearFilter);
    // Filter by Color: the cells with the Good style's fill.
    select(&mut d, 2, 1, 2, 1);
    run(
        &mut d,
        G::ApplyCellStyle {
            name: "Good".into(),
        },
    );
    select(&mut d, 5, 1, 5, 1);
    run(
        &mut d,
        G::ApplyCellStyle {
            name: "Good".into(),
        },
    );
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Color {
                color: "C6EFCE".into(),
                font: false,
            }),
        },
    );
    let shown: Vec<u32> = (1..=6).filter(|&r| !hidden(&d, r)).collect();
    assert_eq!(shown, [2, 5]);
    // A date column: This Year keeps this year's dates.
    put(&mut d, 0, 3, "When");
    put(&mut d, 1, 3, "=TODAY()");
    put(&mut d, 2, 3, "2001-03-01");
    run(&mut d, G::ClearFilter);
    run(&mut d, G::ToggleFilter);
    run(&mut d, G::ToggleFilter);
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 3,
            filter: Some(GridColumnFilter::Dynamic {
                kind: GridDynamicFilter::ThisYear,
            }),
        },
    );
    assert!(!hidden(&d, 1) && hidden(&d, 2), "{:?}", state(&d).filter);
}

#[test]
fn a_text_box_is_a_white_shape_with_dark_text() {
    let mut d = doc();
    run(
        &mut d,
        G::InsertShape {
            preset: "textBox".into(),
            text: Some("Notes".into()),
        },
    );
    let g = drawing(&d);
    assert_eq!(g.name, "TextBox 1");
    let o = g.object.expect("object");
    assert_eq!(
        (o.fill.as_str(), o.text_color.as_str(), o.text.as_str()),
        ("scheme:lt1", "scheme:tx1", "Notes")
    );
}

#[test]
fn validation_alerts_stop_or_ask_before_keeping_an_entry() {
    let mut d = doc();
    select(&mut d, 0, 0, 0, 0);
    let rule = |style| GridValidation {
        kind: GridValidationKind::Whole,
        op: GridCompare::Between,
        formula1: "1".into(),
        formula2: "10".into(),
        show_error: true,
        style,
        error_title: "Shelves".into(),
        error: "Between 1 and 10".into(),
        ..GridValidation::default()
    };
    run(
        &mut d,
        G::SetValidation {
            rule: Some(rule(GridErrorStyle::Stop)),
        },
    );
    run(&mut d, G::BeginEdit { text: "12".into() });
    assert!(fails(&mut d, G::CommitEdit { advance: None }).contains("Between 1 and 10"));
    assert_eq!(
        state(&d).alert,
        Some((
            GridErrorStyle::Stop,
            "Shelves".into(),
            "Between 1 and 10".into()
        ))
    );
    // Stop has no way through.
    run(&mut d, G::AcceptInvalid);
    assert!(fails(&mut d, G::CommitEdit { advance: None }).contains("Between"));
    run(&mut d, G::CancelEdit);
    // Warning: Yes keeps the entry.
    run(
        &mut d,
        G::SetValidation {
            rule: Some(rule(GridErrorStyle::Warning)),
        },
    );
    run(&mut d, G::BeginEdit { text: "12".into() });
    assert!(fails(&mut d, G::CommitEdit { advance: None }).contains("Between"));
    assert_eq!(state(&d).alert.map(|a| a.0), Some(GridErrorStyle::Warning));
    run(&mut d, G::AcceptInvalid);
    run(&mut d, G::CommitEdit { advance: None });
    assert_eq!(text(&d, 0, 0), "12");
    assert_eq!(state(&d).alert, None);
}

#[test]
fn structured_references_autocomplete_columns_specials_and_this_row() {
    use kiln_protocol::commands::CompletionKind;
    let mut d = doc();
    for (c, h) in ["Units", "Unit Price", "Total"].iter().enumerate() {
        put(&mut d, 0, c as u32, h);
    }
    for r in 1..=3 {
        put(&mut d, r, 0, &r.to_string());
        put(&mut d, r, 1, "2");
    }
    run(
        &mut d,
        G::CreateTable {
            range: Some(GridRange {
                r0: 0,
                c0: 0,
                r1: 3,
                c1: 2,
            }),
            header: true,
            style: "TableStyleMedium2".into(),
        },
    );
    let name = state(&d).table.expect("table").name;
    let labels = |d: &GridDoc| -> Vec<String> {
        d.edit
            .as_ref()
            .map(|e| {
                assert!(
                    e.completions
                        .iter()
                        .all(|c| c.kind == CompletionKind::Structured)
                );
                e.completions.iter().map(|c| c.label.clone()).collect()
            })
            .unwrap_or_default()
    };
    // Outside the table: columns and the special items, no `@`.
    at(&mut d, 6, 0);
    run(
        &mut d,
        G::BeginEdit {
            text: format!("=SUM({name}["),
        },
    );
    let l = labels(&d);
    assert_eq!(&l[..3], ["Units", "Unit Price", "Total"]);
    for sp in ["#All", "#Data", "#Headers", "#Totals", "#This Row"] {
        assert!(l.iter().any(|x| x == sp), "{sp} in {l:?}");
    }
    assert!(!l.iter().any(|x| x == "@"));
    run(
        &mut d,
        G::SetEditText {
            text: format!("=SUM({name}[[#He"),
            start: 0,
            end: (name.len() + 10) as u32,
            formula_bar: false,
        },
    );
    assert_eq!(labels(&d), ["#Headers"]);
    run(&mut d, G::AcceptCompletion { index: 0 });
    assert_eq!(
        d.edit.as_ref().map(|e| e.text.clone()),
        Some(format!("=SUM({name}[[#Headers]"))
    );
    run(&mut d, G::CancelEdit);
    // Inside the table: `@` first, then its columns.
    at(&mut d, 1, 2);
    run(&mut d, G::BeginEdit { text: "=[".into() });
    assert_eq!(labels(&d).first().map(String::as_str), Some("@"));
    run(&mut d, G::AcceptCompletion { index: 0 });
    assert_eq!(d.edit.as_ref().map(|e| e.text.clone()), Some("=[@".into()));
    assert_eq!(labels(&d), ["@Units", "@Unit Price", "@Total"]);
    run(&mut d, G::AcceptCompletion { index: 1 });
    assert_eq!(
        d.edit.as_ref().map(|e| e.text.clone()),
        Some("=[@[Unit Price]]".into())
    );
    run(
        &mut d,
        G::SetEditText {
            text: "=[@[Unit Price]]*[@Un".into(),
            start: 21,
            end: 21,
            formula_bar: false,
        },
    );
    assert_eq!(labels(&d), ["@Units", "@Unit Price"]);
    run(&mut d, G::AcceptCompletion { index: 0 });
    run(&mut d, G::CommitEdit { advance: None });
    assert_eq!(num(&d, 1, 2), 2.0);
}

fn edit(d: &mut GridDoc, e: kiln_protocol::commands::EditCommand) {
    d.execute(&Command::Suite(
        kiln_protocol::commands::SuiteCommand::Edit(e.clone()),
    ))
    .unwrap_or_else(|err| panic!("{e:?}: {err}"));
}

#[test]
fn edits_of_a_filtered_list_leave_the_hidden_rows_alone() {
    use kiln_protocol::commands::EditCommand as E;
    let mut d = doc();
    budget(&mut d);
    run(&mut d, G::ToggleFilter);
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Values {
                values: vec!["Firing".into()],
                blanks: false,
            }),
        },
    );
    // Shown: Gas (row 3) and Kiln hire (row 6).
    assert!(hidden(&d, 1) && !hidden(&d, 3) && hidden(&d, 4) && !hidden(&d, 6));
    // The status bar counts what is shown.
    select(&mut d, 1, 2, 6, 2);
    let s = d.status().cells.expect("stats");
    assert_eq!((s.count, s.sum), (2, 1820.0));
    // Copy takes the shown rows; a paste lays them one under another.
    edit(&mut d, E::Copy);
    let c = d.take_clipboard().expect("clipboard");
    assert_eq!(c.text.replace("\r\n", "\n").trim_end(), "1180\n640");
    at(&mut d, 0, 5);
    edit(&mut d, E::Paste { data: Some(c) });
    assert_eq!((num(&d, 0, 5), num(&d, 1, 5)), (1180.0, 640.0));
    assert!(text(&d, 2, 5).is_empty());
    // One cell pasted over the filtered selection fills its shown cells.
    put(&mut d, 0, 6, "x");
    at(&mut d, 0, 6);
    edit(&mut d, E::Copy);
    let c = d.take_clipboard();
    select(&mut d, 1, 3, 6, 3);
    edit(&mut d, E::Paste { data: c });
    assert_eq!(
        (text(&d, 3, 3).as_str(), text(&d, 6, 3).as_str()),
        ("x", "x")
    );
    assert!(text(&d, 1, 3).is_empty() && text(&d, 4, 3).is_empty());
    // Fill Down fills the shown rows from the first.
    select(&mut d, 3, 0, 6, 0);
    run(&mut d, G::FillDown);
    assert_eq!(text(&d, 6, 0), "Gas");
    assert_eq!(text(&d, 4, 0), "Shelves");
    // Ctrl+Enter enters the text in the shown cells only.
    select(&mut d, 1, 4, 6, 4);
    run(&mut d, G::BeginEdit { text: "ok".into() });
    run(&mut d, G::FillSelection);
    assert_eq!(text(&d, 3, 4), "ok");
    assert!(text(&d, 2, 4).is_empty());
    // Bold formats the shown rows only.
    select(&mut d, 1, 0, 6, 2);
    edit(&mut d, E::Bold);
    let s = d.workbook.sheet(sid(&d)).expect("sheet");
    let bold = |r: u32| d.workbook.styles().get(s.style_at(r, 0)).font.bold;
    assert!(bold(3) && bold(6) && !bold(1) && !bold(5));
    // Delete clears the shown cells; the hidden ones keep their values.
    select(&mut d, 1, 0, 6, 2);
    run(&mut d, G::ClearContents);
    assert!(text(&d, 3, 0).is_empty() && text(&d, 6, 2).is_empty());
    run(&mut d, G::ClearFilter);
    assert!(!hidden(&d, 1));
    assert_eq!(text(&d, 1, 0), "Clay");
    assert_eq!(num(&d, 2, 2), 310.0);
    assert_eq!(text(&d, 5, 1), "Events");
}

#[test]
fn a_cut_from_a_filtered_list_moves_the_shown_rows_only() {
    use kiln_protocol::commands::EditCommand as E;
    let mut d = doc();
    budget(&mut d);
    run(&mut d, G::ToggleFilter);
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Values {
                values: vec!["Firing".into()],
                blanks: false,
            }),
        },
    );
    select(&mut d, 1, 0, 6, 0);
    edit(&mut d, E::Cut);
    let c = d.take_clipboard();
    at(&mut d, 0, 6);
    edit(&mut d, E::Paste { data: c });
    assert_eq!(
        (text(&d, 0, 6).as_str(), text(&d, 1, 6).as_str()),
        ("Gas", "Kiln hire")
    );
    run(&mut d, G::ClearFilter);
    assert!(text(&d, 3, 0).is_empty() && text(&d, 6, 0).is_empty());
    assert_eq!(
        (text(&d, 1, 0).as_str(), text(&d, 4, 0).as_str()),
        ("Clay", "Shelves")
    );
}

#[test]
fn filter_lists_cascade_and_dates_carry_their_day() {
    let mut d = doc();
    budget(&mut d);
    put(&mut d, 0, 3, "When");
    for (r, t) in [
        "2026-01-05",
        "2026-01-17",
        "2026-02-03",
        "2025-12-12",
        "2026-03-09",
        "2026-03-30",
    ]
    .iter()
    .enumerate()
    {
        put(&mut d, r as u32 + 1, 3, t);
    }
    run(&mut d, G::ToggleFilter);
    run(
        &mut d,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Values {
                values: vec!["Firing".into()],
                blanks: false,
            }),
        },
    );
    let menu = |d: &mut GridDoc, col: u32| {
        let query = Query::Grid(GridQuery::FilterMenu { row: 0, col });
        d.prepare_query(&query);
        let GridQueryResult::FilterMenu(m) = q(d, GridQuery::FilterMenu { row: 0, col }) else {
            panic!("menu")
        };
        m
    };
    // Item lists what the Category filter lets through.
    let items: Vec<String> = menu(&mut d, 0).items.into_iter().map(|i| i.text).collect();
    assert_eq!(items, ["Gas", "Kiln hire"]);
    // The filtered column itself still lists all its values.
    assert_eq!(menu(&mut d, 1).items.len(), 4);
    let m = menu(&mut d, 3);
    assert_eq!(m.kind, GridValueKind::Date);
    let days: Vec<Option<String>> = m.items.into_iter().map(|i| i.date).collect();
    assert_eq!(days, [Some("2026-02-03".into()), Some("2026-03-30".into())]);
}

/// Manual calculation, a value on another sheet that is stale until Calculate Now: every Grid
/// test checks the journal against a shadow copy, which compares every cell, volatile or not,
/// so this is checked with a `TODAY()` elsewhere in the workbook too.
fn stale_across_sheets(today: bool) -> GridDoc {
    let mut d = doc();
    put(&mut d, 3, 0, "2");
    if today {
        put(&mut d, 9, 0, "=TODAY()");
    }
    run(&mut d, G::InsertSheet);
    assert_eq!(d.active, 1);
    put(&mut d, 4, 0, "=Sheet1!A4*2");
    run(
        &mut d,
        G::CalculationMode {
            mode: GridCalcMode::Manual,
        },
    );
    run(&mut d, G::ActivateSheet { index: 0 });
    put(&mut d, 3, 0, "5");
    run(&mut d, G::CalculateNow);
    run(&mut d, G::ActivateSheet { index: 1 });
    d
}

#[test]
fn calculate_now_brings_a_value_on_another_sheet_up_to_date() {
    for today in [false, true] {
        let d = stale_across_sheets(today);
        assert_eq!(num(&d, 4, 0), 10.0, "TODAY() elsewhere: {today}");
    }
}

/// Review 5's plant, Calculate Now applied and not recorded: the shadow check sees the value it
/// left behind, with a `TODAY()` in the workbook or without.
#[test]
fn the_shadow_check_catches_calculate_now_unrecorded_beside_a_volatile_cell() {
    use super::super::ledger::plant;
    for today in [false, true] {
        plant::set(Some(plant::Plant::RecalcUnrecorded));
        let caught = std::panic::catch_unwind(|| {
            stale_across_sheets(today);
        });
        plant::set(None);
        let message = caught
            .err()
            .and_then(|p| p.downcast_ref::<String>().cloned())
            .unwrap_or_default();
        println!("GUARD shadow PR (TODAY() elsewhere: {today}): {message}");
        assert!(message.contains("journal check"), "{message}");
        assert!(message.contains("Sheet2!A5"), "{message}");
    }
}

/// Excel calculates a formula as it is entered, under manual calculation too.
#[test]
fn a_formula_entered_under_manual_calculation_shows_its_value() {
    let mut d = doc();
    put(&mut d, 0, 0, "2");
    put(&mut d, 0, 2, "=B1+1");
    run(
        &mut d,
        G::CalculationMode {
            mode: GridCalcMode::Manual,
        },
    );
    put(&mut d, 0, 1, "=A1*10");
    assert_eq!(num(&d, 0, 1), 20.0);
    assert_eq!(
        num(&d, 0, 2),
        1.0,
        "what depends on it waits for Calculate Now"
    );
    run(&mut d, G::CalculateNow);
    assert_eq!(num(&d, 0, 2), 21.0);
}

/// A spill shown under manual calculation comes back whole from the document's bundle (what a
/// recovery opens).
#[test]
fn a_spill_shown_under_manual_calculation_comes_back_from_the_bundle() {
    let mut d = doc();
    put(&mut d, 0, 0, "=SEQUENCE(3)");
    put(&mut d, 0, 1, "4");
    put(&mut d, 0, 2, "=SEQUENCE(B1)");
    run(
        &mut d,
        G::CalculationMode {
            mode: GridCalcMode::Manual,
        },
    );
    put(&mut d, 0, 1, "2");
    let shown: Vec<String> = (0..4)
        .flat_map(|r| [text(&d, r, 0), text(&d, r, 2)])
        .collect();
    assert_eq!(shown, ["1", "1", "2", "2", "3", "3", "", "4"]);
    let back = GridDoc::from_bundle(&d.to_bundle().unwrap(), None).unwrap();
    let got: Vec<String> = (0..4)
        .flat_map(|r| [text(&back, r, 0), text(&back, r, 2)])
        .collect();
    assert_eq!(got, shown);
}

/// The stamp guard on a large sheet: a commit at 50,000 rows against the check the journal makes
/// after it (`Ledger::holds`), each timed 50 times. The check is two integer comparisons.
#[test]
fn the_stamp_guard_is_nothing_beside_a_commit_at_50k_rows() {
    let mut d = doc();
    let rows: Vec<Vec<kiln_sheet::commands::CellInput>> = vec![
        (0..50_000)
            .map(|r| kiln_sheet::commands::CellInput::value(r, 0, f64::from(r)))
            .collect(),
    ];
    d.apply_for_test(&kiln_sheet::Command::SetCells {
        sheet: sid(&d),
        cells: rows.into_iter().flatten().collect(),
    })
    .unwrap();
    put(&mut d, 0, 1, "=SUM(A1:A50000)");
    let start = std::time::Instant::now();
    for i in 0..50 {
        put(&mut d, 1 + i, 0, &i.to_string());
    }
    let commits = start.elapsed();
    let start = std::time::Instant::now();
    let mut held = 0;
    for _ in 0..50 {
        held += usize::from(std::hint::black_box(d.log.holds(&d.workbook)));
    }
    let checks = start.elapsed();
    println!(
        "STAMP 50 commits at 50,000 rows: {commits:?}; 50 stamp checks: {checks:?} ({:.6} % of the commits)",
        checks.as_secs_f64() / commits.as_secs_f64() * 100.0
    );
    assert_eq!(held, 50);
    assert!(checks.as_secs_f64() < commits.as_secs_f64() / 1000.0);
}

/// Whole rows selected with one stored cell: the selection's text (Text to Columns' sample, the
/// screen reader's) is the used part, a few bytes, at once. It once had a tab for every cell:
/// about 80 MB for these 5,000 rows, 17 GB for the whole sheet.
#[test]
fn whole_rows_selected_text_is_the_used_part() {
    use std::time::{Duration, Instant};
    let mut d = doc();
    put(&mut d, 0, 0, "a,b");
    select(&mut d, 0, 0, 4_999, 16_383);
    let t = Instant::now();
    let text = d.selected_text().unwrap_or_default();
    let took = t.elapsed();
    assert!(text.len() < 1024, "{} bytes", text.len());
    assert!(took < Duration::from_millis(200), "{took:?}");
    assert_eq!(text, "a,b\r\n");
}

/// The whole sheet selected (the corner button): every question about the selection answers from
/// the used range, never by walking 1,048,576 × 16,384 cells.
#[test]
fn whole_sheet_selection_queries_answer_from_the_used_range() {
    use std::time::{Duration, Instant};
    let all = |d: &mut GridDoc| select(d, 0, 0, 1_048_575, 16_383);
    let limit = Duration::from_secs(5);
    let mut d = doc();
    // An empty sheet first, then one with a little data.
    all(&mut d);
    let t = Instant::now();
    assert_eq!(d.selected_text().as_deref(), Some("\r\n"));
    assert!(t.elapsed() < limit, "empty sheet: {:?}", t.elapsed());
    put(&mut d, 0, 0, "Name");
    put(&mut d, 0, 1, "Pair");
    put(&mut d, 1, 0, "Ada");
    put(&mut d, 1, 1, "a,b");
    all(&mut d);
    let t = Instant::now();
    assert_eq!(
        d.selected_text().as_deref(),
        Some("Name\tPair\r\nAda\ta,b\r\n")
    );
    assert!(t.elapsed() < limit, "SelectedText: {:?}", t.elapsed());
    // Whole columns and whole rows: their used part.
    select(&mut d, 0, 1, 1_048_575, 1);
    assert_eq!(d.selected_text().as_deref(), Some("Pair\r\na,b\r\n"));
    select(&mut d, 1, 0, 1, 16_383);
    assert_eq!(d.selected_text().as_deref(), Some("Ada\ta,b\r\n"));
    // A few cells keep their blanks, as typed.
    select(&mut d, 0, 0, 2, 2);
    assert_eq!(
        d.selected_text().as_deref(),
        Some("Name\tPair\t\r\nAda\ta,b\t\r\n\t\t\r\n")
    );
    all(&mut d);
    for query in [
        GridQuery::State,
        GridQuery::FormatCells,
        GridQuery::ZoomToSelection {
            width: 1200.0,
            height: 700.0,
        },
        GridQuery::RecommendedCharts,
        GridQuery::ConditionalFormats { sheet: false },
        GridQuery::Validation,
        GridQuery::PageSetup,
        GridQuery::FormulaTrace,
        GridQuery::Errors,
        GridQuery::Spelling,
        GridQuery::DataRange,
        GridQuery::Export {
            format: GridExportFormat::Csv,
            selection: true,
        },
    ] {
        let t = Instant::now();
        let _ = d.app_query(&Query::Grid(query.clone()));
        assert!(t.elapsed() < limit, "{query:?}: {:?}", t.elapsed());
    }
    let t = Instant::now();
    let _ = d.status();
    let _ = d.a11y("Book1");
    assert!(t.elapsed() < limit, "status and a11y: {:?}", t.elapsed());
    // Ctrl+C and Ctrl+X (the ribbon's and the context menu's too): the clipboard's text and HTML
    // hold the used range.
    for edit in [
        kiln_protocol::commands::EditCommand::Copy,
        kiln_protocol::commands::EditCommand::Cut,
    ] {
        all(&mut d);
        let t = Instant::now();
        d.execute(&Command::Suite(kiln_protocol::SuiteCommand::Edit(
            edit.clone(),
        )))
        .unwrap_or_else(|e| panic!("{edit:?}: {e}"));
        let c = d.take_clipboard().expect("the clipboard");
        assert!(t.elapsed() < limit, "{edit:?}: {:?}", t.elapsed());
        assert_eq!(c.text, "Name\tPair\r\nAda\ta,b\r\n", "{edit:?}");
        assert!(
            c.html.as_deref().is_some_and(|h| h.len() < 2000),
            "{edit:?}"
        );
    }
}

/// Whole columns on an empty sheet: a validation rule, a conditional format and a print area
/// cover the whole column, as they did before whole rows and columns were clipped for the
/// commands that walk cells.
#[test]
fn whole_columns_on_an_empty_sheet_keep_their_rules_and_print_area() {
    let whole_b = Range::new(0, 1, 1_048_575, 1);
    let mut d = doc();
    select(&mut d, 0, 1, 1_048_575, 1);
    run(
        &mut d,
        G::SetValidation {
            rule: Some(GridValidation {
                kind: GridValidationKind::List,
                formula1: "a,b".into(),
                ..GridValidation::default()
            }),
        },
    );
    run(
        &mut d,
        G::AddConditionalFormat {
            rule: rule(
                "",
                GridCfRuleKind::CellValue {
                    op: GridCompare::GreaterThan,
                    values: vec!["5".into()],
                },
                Some(GridCfStyle {
                    fill: Some("FFC7CE".into()),
                    ..GridCfStyle::default()
                }),
            ),
        },
    );
    run(
        &mut d,
        G::PrintArea {
            action: GridPrintAreaAction::Set,
        },
    );
    let s = d.workbook.sheet(sid(&d)).expect("sheet");
    assert_eq!(s.meta().validations[0].ranges, vec![whole_b]);
    assert_eq!(s.meta().cf[0].ranges, vec![whole_b]);
    assert_eq!(page(&d).print_area, "$B:$B");
}

/// Whole rows that start below row 1 and columns that start right of A clip to the used range
/// for the commands that read each cell (Sort, Remove Duplicates, Subtotal, Table, Fill Series,
/// Create from Selection): never a million blank rows or 16,383 blank columns.
#[test]
fn rows_and_columns_past_the_first_clip_to_the_used_range() {
    let mut d = doc();
    put(&mut d, 0, 0, "Name");
    put(&mut d, 3, 2, "x");
    select(&mut d, 1, 0, 1_048_575, 16_383);
    assert_eq!(d.data_range().expect("range"), Range::new(1, 0, 3, 2));
    select(&mut d, 0, 1, 1_048_575, 16_383);
    assert_eq!(d.data_range().expect("range"), Range::new(0, 1, 3, 2));
    // The whole sheet, rows 2:1048576 and columns B:XFD on an empty sheet: their first cell.
    let mut e = doc();
    select(&mut e, 0, 0, 1_048_575, 16_383);
    assert_eq!(e.data_range().expect("range"), Range::cell(0, 0));
    select(&mut e, 1, 0, 1_048_575, 16_383);
    assert_eq!(e.data_range().expect("range"), Range::cell(1, 0));
    select(&mut e, 0, 1, 1_048_575, 16_383);
    assert_eq!(e.data_range().expect("range"), Range::cell(0, 1));
    // Rows below the data: their first cell too.
    let mut f = doc();
    put(&mut f, 0, 0, "Name");
    select(&mut f, 1, 0, 1_048_575, 16_383);
    assert_eq!(f.data_range().expect("range"), Range::cell(1, 0));
    // Remove Duplicates there finishes at once.
    select(&mut e, 1, 0, 1_048_575, 16_383);
    let t = std::time::Instant::now();
    let _ = e.execute(&Command::Grid(G::RemoveDuplicates {
        columns: Vec::new(),
        header: None,
    }));
    assert!(
        t.elapsed() < std::time::Duration::from_secs(2),
        "{:?}",
        t.elapsed()
    );
}
