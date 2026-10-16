// SPDX-License-Identifier: AGPL-3.0-or-later
//! Workbook-level tests of AutoFill, AutoFilter, the clipboard, Text to Columns, Remove
//! Duplicates and the grid limits.

mod support;

use kiln_sheet::address::{MAX_COLS, MAX_ROWS};
use kiln_sheet::clipboard::ClipData;
use kiln_sheet::commands::{CellInput, Command, MetaField, PasteOp, PasteWhat, TextToColumns};
use kiln_sheet::data::{ColumnFormat, DateOrder};
use kiln_sheet::fill::FillMode;
use kiln_sheet::meta::{
    AutoFilter, ColumnFilter, CustomCriterion, DynamicFilter, FilterColumn, FilterOperator, Table,
    TableColumn, TableFilter,
};
use kiln_sheet::styles::StylePatch;
use kiln_sheet::{CellPos, CellValue, Range, SheetId};
use support::{Fixture, setup};

fn column(f: &Fixture, s: SheetId, col: u32, r0: u32, r1: u32) -> Vec<String> {
    (r0..=r1).map(|r| f.wb.display_text(s, r, col)).collect()
}

// ── AutoFill ──────────────────────────────────────────────────────────────

fn fill(f: &mut Fixture, s: SheetId, source: Range, target: Range, mode: FillMode) {
    f.exec(Command::AutoFill {
        sheet: s,
        source,
        target,
        mode,
    });
}

#[test]
fn fill_handle_extends_series_and_copies() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 2);
    f.set(s, "A2", 4);
    fill(
        &mut f,
        s,
        Range::new(0, 0, 1, 0),
        Range::new(0, 0, 5, 0),
        FillMode::Auto,
    );
    assert_eq!(column(&f, s, 0, 0, 5), vec!["2", "4", "6", "8", "10", "12"]);
    f.set(s, "B1", "Mon");
    fill(
        &mut f,
        s,
        Range::cell(0, 1),
        Range::new(0, 1, 3, 1),
        FillMode::Auto,
    );
    assert_eq!(column(&f, s, 1, 0, 3), vec!["Mon", "Tue", "Wed", "Thu"]);
    f.set(s, "C1", 7);
    fill(
        &mut f,
        s,
        Range::cell(0, 2),
        Range::new(0, 2, 2, 2),
        FillMode::Auto,
    );
    assert_eq!(column(&f, s, 2, 0, 2), vec!["7", "7", "7"]);
    f.undo();
    assert_eq!(column(&f, s, 2, 0, 2), vec!["7", "", ""]);
}

#[test]
fn fill_adjusts_formulas_and_keeps_formats_in_every_direction() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    for r in 1..=5 {
        f.set(s, &format!("A{r}"), r);
    }
    f.set(s, "B3", "=A3*10");
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cell(2, 1)],
        patch: Box::new(StylePatch {
            num_fmt: Some("0.0".into()),
            ..StylePatch::default()
        }),
    });
    fill(
        &mut f,
        s,
        Range::cell(2, 1),
        Range::new(0, 1, 4, 1),
        FillMode::Auto,
    );
    assert_eq!(f.formula(s, "B1").as_deref(), Some("A1*10"));
    assert_eq!(f.formula(s, "B5").as_deref(), Some("A5*10"));
    assert_eq!(
        column(&f, s, 1, 0, 4),
        vec!["10.0", "20.0", "30.0", "40.0", "50.0"]
    );
    // Rightwards: column references shift.
    f.set(s, "D1", "=A1+1");
    fill(
        &mut f,
        s,
        Range::cell(0, 3),
        Range::new(0, 3, 0, 5),
        FillMode::Auto,
    );
    assert_eq!(f.formula(s, "F1").as_deref(), Some("C1+1"));
    // Formats only / without formatting.
    f.set(s, "H1", 1);
    fill(
        &mut f,
        s,
        Range::cell(0, 7),
        Range::new(0, 7, 1, 7),
        FillMode::Series,
    );
    assert_eq!(f.num(s, "H2"), 2.0);
    fill(
        &mut f,
        s,
        Range::cell(2, 1),
        Range::new(2, 1, 2, 2),
        FillMode::FormatsOnly,
    );
    assert_eq!(f.wb.display_text(s, 2, 2), "");
    assert_eq!(
        f.wb.styles()
            .num_fmt(f.wb.sheet(s).map_or(0, |x| x.style_at(2, 2))),
        "0.0"
    );
}

#[test]
fn fill_dates_by_month() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput {
            row: 0,
            col: 0,
            content: kiln_sheet::commands::Content::Typed("1/31/2024".into()),
            style: None,
        }],
    });
    assert_eq!(f.wb.display_text(s, 0, 0), "1/31/2024");
    fill(
        &mut f,
        s,
        Range::cell(0, 0),
        Range::new(0, 0, 2, 0),
        FillMode::Months,
    );
    assert_eq!(
        column(&f, s, 0, 0, 2),
        vec!["1/31/2024", "2/29/2024", "3/31/2024"]
    );
    assert!(
        f.try_exec(Command::AutoFill {
            sheet: s,
            source: Range::cell(0, 0),
            target: Range::new(0, 0, 2, 2),
            mode: FillMode::Auto,
        })
        .is_err()
    );
}

// ── AutoFilter ────────────────────────────────────────────────────────────

fn sales(f: &mut Fixture, s: SheetId) {
    let rows: [(&str, f64, f64); 6] = [
        ("North", 120.0, 46_300.0),
        ("South", 80.0, 46_310.0),
        ("East", 200.0, 46_290.0),
        ("West", 50.0, 46_000.0),
        ("Northeast", 150.0, 46_311.0),
        ("", 10.0, 46_200.0),
    ];
    f.set(s, "A1", "Region");
    f.set(s, "B1", "Units");
    f.set(s, "C1", "Date");
    for (i, (r, u, d)) in rows.iter().enumerate() {
        let row = i + 2;
        if !r.is_empty() {
            f.set(s, &format!("A{row}"), *r);
        }
        f.set(s, &format!("B{row}"), *u);
        f.set(s, &format!("C{row}"), *d);
    }
    f.set(s, "E1", "=SUBTOTAL(109,B2:B7)");
}

fn auto_filter(f: &mut Fixture, s: SheetId, col: u32, filter: ColumnFilter) -> Vec<u32> {
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::AutoFilter(Some(AutoFilter {
            range: Range::new(0, 0, 6, 2),
            columns: vec![FilterColumn {
                col_id: col,
                hidden_button: false,
                show_button: true,
                filter: Some(filter),
            }],
            sort: None,
        }))),
    });
    f.exec(Command::ApplyFilter { sheet: s });
    let sheet = f.wb.sheet(s).expect("sheet");
    (1..=6)
        .filter(|&r| !sheet.is_row_hidden(r))
        .map(|r| r + 1)
        .collect()
}

#[test]
fn filter_by_values_custom_criteria_and_top_ten() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    sales(&mut f, s);
    let values = ColumnFilter::Values {
        values: vec!["north".into(), "West".into()],
        blank: true,
        date_groups: Vec::new(),
    };
    assert_eq!(auto_filter(&mut f, s, 0, values), vec![2, 5, 7]);
    assert_eq!(f.num(s, "E1"), 180.0, "SUBTOTAL skips filtered rows");
    let custom = ColumnFilter::Custom {
        and: false,
        criteria: vec![CustomCriterion {
            op: FilterOperator::Equal,
            value: "North*".into(),
        }],
    };
    assert_eq!(auto_filter(&mut f, s, 0, custom), vec![2, 6]);
    let between = ColumnFilter::Custom {
        and: true,
        criteria: vec![
            CustomCriterion {
                op: FilterOperator::GreaterThanOrEqual,
                value: "80".into(),
            },
            CustomCriterion {
                op: FilterOperator::LessThan,
                value: "150".into(),
            },
        ],
    };
    assert_eq!(auto_filter(&mut f, s, 1, between), vec![2, 3]);
    let top = ColumnFilter::Top10 {
        top: true,
        percent: false,
        value: 2.0,
    };
    assert_eq!(auto_filter(&mut f, s, 1, top), vec![4, 6]);
    let bottom = ColumnFilter::Top10 {
        top: false,
        percent: true,
        value: 34.0,
    };
    assert_eq!(auto_filter(&mut f, s, 1, bottom), vec![5, 7]);
    f.undo();
    assert_eq!(
        f.wb.sheet(s).map(|x| x.is_row_hidden(1)),
        Some(true),
        "back to the top-2 filter"
    );
}

#[test]
fn filter_by_dynamic_dates_and_average() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    sales(&mut f, s);
    // The clock says 2026-10-15 (46310): this week is 46307…46313.
    assert_eq!(
        auto_filter(&mut f, s, 2, ColumnFilter::Dynamic(DynamicFilter::ThisWeek)),
        vec![3, 6]
    );
    assert_eq!(
        auto_filter(&mut f, s, 2, ColumnFilter::Dynamic(DynamicFilter::Today)),
        vec![3]
    );
    assert_eq!(
        auto_filter(
            &mut f,
            s,
            1,
            ColumnFilter::Dynamic(DynamicFilter::AboveAverage)
        ),
        vec![2, 4, 6]
    );
}

#[test]
fn table_filters_apply_to_their_data_rows() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    sales(&mut f, s);
    f.exec(Command::AddTable {
        sheet: s,
        table: Box::new(Table {
            id: 0,
            name: "Sales".into(),
            display_name: None,
            range: Range::new(0, 0, 6, 2),
            header_row: true,
            totals_row: false,
            columns: ["Region", "Units", "Date"]
                .into_iter()
                .map(TableColumn::named)
                .collect(),
            style: None,
            auto_filter: Some(TableFilter {
                columns: vec![FilterColumn {
                    col_id: 1,
                    hidden_button: false,
                    show_button: true,
                    filter: Some(ColumnFilter::Custom {
                        and: false,
                        criteria: vec![CustomCriterion {
                            op: FilterOperator::GreaterThan,
                            value: "100".into(),
                        }],
                    }),
                }],
                sort: None,
            }),
            comment: None,
        }),
    });
    f.exec(Command::ApplyFilter { sheet: s });
    let sheet = f.wb.sheet(s).expect("sheet");
    let shown: Vec<u32> = (1..=6).filter(|&r| !sheet.is_row_hidden(r)).collect();
    assert_eq!(shown, vec![1, 3, 5]);
    assert!(sheet.is_row_filtered(2));
}

// ── clipboard ─────────────────────────────────────────────────────────────

#[test]
fn clipboard_copy_paste_within_the_workbook_adjusts_formulas() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 2);
    f.set(s, "B1", "=A1*3");
    let clip = f.wb.copy(s, Range::new(0, 0, 0, 1), false).expect("copies");
    assert_eq!(clip.to_tsv(), "2\t6\r\n");
    let cmd =
        f.wb.paste_command(
            s,
            Range::cell(4, 2),
            &clip,
            PasteWhat::All,
            PasteOp::None,
            false,
            false,
        )
        .expect("paste");
    f.exec(cmd);
    assert_eq!(f.formula(s, "D5").as_deref(), Some("C5*3"));
    // Cut moves, so references follow.
    f.set(s, "F1", "=A1");
    let cut = f.wb.copy(s, Range::cell(0, 0), true).expect("cuts");
    let cmd =
        f.wb.paste_command(
            s,
            Range::cell(9, 0),
            &cut,
            PasteWhat::All,
            PasteOp::None,
            false,
            false,
        )
        .expect("paste");
    f.exec(cmd);
    assert_eq!(f.formula(s, "F1").as_deref(), Some("A10"));
    assert_eq!(f.num(s, "B1"), 6.0, "B1 now refers to A10 too");
}

#[test]
fn clipboard_maps_external_tsv_and_html_into_typed_cells() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let tsv = ClipData::from_tsv(
        "Item\tPrice\tShare\nPen\t$1.50\t25%\nBook\t12\t=B3*2\n",
        false,
    );
    let cmd =
        f.wb.paste_command(
            s,
            Range::cell(0, 0),
            &tsv,
            PasteWhat::All,
            PasteOp::None,
            false,
            false,
        )
        .expect("paste");
    f.exec(cmd);
    assert_eq!(f.num(s, "B2"), 1.5);
    assert_eq!(
        f.wb.display_text(s, 1, 1).trim_end(),
        "$1.50",
        "with the _) padding"
    );
    assert_eq!(f.wb.display_text(s, 1, 2), "25%");
    assert_eq!(f.num(s, "C3"), 24.0);
    let html =
        "<table><tr><td colspan=2><b>Total</b></td></tr><tr><td>1</td><td>2</td></tr></table>";
    let clip = ClipData::from_html(html, false);
    let cmd =
        f.wb.paste_command(
            s,
            Range::cell(10, 0),
            &clip,
            PasteWhat::All,
            PasteOp::None,
            false,
            false,
        )
        .expect("paste");
    f.exec(cmd);
    assert_eq!(
        f.wb.sheet(s).map(|x| x.merges().to_vec()),
        Some(vec![Range::new(10, 0, 10, 1)])
    );
    let st = f.wb.sheet(s).map_or(0, |x| x.style_at(10, 0));
    assert!(f.wb.styles().font_of(st).bold);
    assert_eq!(f.num(s, "B12"), 2.0);
    f.undo();
    assert!(f.wb.sheet(s).is_some_and(|x| x.merges().is_empty()));
    assert!(f.val(s, "B12").is_blank());
}

#[test]
fn clipboard_html_carries_formats_out() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 1234.5);
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cell(0, 0)],
        patch: Box::new(StylePatch {
            num_fmt: Some("#,##0.00".into()),
            font: Some(kiln_sheet::styles::FontPatch {
                bold: Some(true),
                ..Default::default()
            }),
            ..StylePatch::default()
        }),
    });
    let clip = f.wb.copy(s, Range::cell(0, 0), false).expect("copies");
    let html = clip.to_html(f.wb.theme());
    assert!(html.contains("font-weight:bold"), "{html}");
    assert!(html.contains(">1,234.50<"), "{html}");
    let back = ClipData::from_html(&html, false);
    assert_eq!(
        back.get(0, 0).map(|c| c.value.clone()),
        Some(CellValue::Number(1234.5))
    );
}

// ── Text to Columns and Remove Duplicates ─────────────────────────────────

#[test]
fn text_to_columns_splits_and_converts() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", "Smith,John,15.01.2024,007");
    f.set(s, "A2", "\"Doe, Jr\",Jane,02.02.2024,042");
    f.exec(Command::TextToColumns {
        sheet: s,
        range: Range::new(0, 0, 1, 0),
        options: TextToColumns {
            delimited: true,
            delimiters: vec![','],
            merge_delimiters: false,
            qualifier: Some('"'),
            breaks: Vec::new(),
            formats: vec![
                ColumnFormat::General,
                ColumnFormat::Skip,
                ColumnFormat::Date(DateOrder::DMY),
                ColumnFormat::Text,
            ],
            destination: Some(CellPos::new(0, 2)),
        },
    });
    assert_eq!(f.val(s, "C1"), CellValue::text("Smith"));
    assert_eq!(f.val(s, "C2"), CellValue::text("Doe, Jr"));
    assert_eq!(f.num(s, "D1"), 45306.0);
    assert_eq!(f.val(s, "E2"), CellValue::text("042"));
    f.undo();
    assert!(f.val(s, "C1").is_blank());
}

#[test]
fn remove_duplicates_keeps_first_occurrences() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let rows = [
        ("Name", "City"),
        ("Ann", "Oslo"),
        ("bob", "Rome"),
        ("ANN", "oslo"),
        ("Ann", "Paris"),
        ("Bob", "Rome"),
    ];
    for (i, (a, b)) in rows.iter().enumerate() {
        f.set(s, &format!("A{}", i + 1), *a);
        f.set(s, &format!("B{}", i + 1), *b);
    }
    f.exec(Command::RemoveDuplicates {
        sheet: s,
        range: Range::new(0, 0, 5, 1),
        columns: Vec::new(),
        header: true,
    });
    assert_eq!(
        column(&f, s, 0, 0, 5),
        vec!["Name", "Ann", "bob", "Ann", "", ""]
    );
    assert_eq!(
        column(&f, s, 1, 0, 5),
        vec!["City", "Oslo", "Rome", "Paris", "", ""]
    );
    f.undo();
    f.exec(Command::RemoveDuplicates {
        sheet: s,
        range: Range::new(0, 0, 5, 1),
        columns: vec![0],
        header: true,
    });
    assert_eq!(column(&f, s, 0, 0, 3), vec!["Name", "Ann", "bob", ""]);
}

// ── limits ────────────────────────────────────────────────────────────────

#[test]
fn limits_of_the_grid_are_addressable_and_enforced() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![
            CellInput::value(MAX_ROWS - 1, MAX_COLS - 1, "corner"),
            CellInput::formula(0, 0, "=XFD1048576"),
            CellInput::formula(1, 0, "=COUNTA(XFD:XFD)"),
        ],
    });
    assert_eq!(f.val(s, "A1"), CellValue::text("corner"));
    assert_eq!(f.num(s, "A2"), 1.0);
    assert_eq!(
        f.wb.sheet(s).map(|x| x.cols().total_px()),
        Some(64.0 * f64::from(MAX_COLS))
    );
    assert!(
        f.try_exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput::value(MAX_ROWS, 0, 1)],
        })
        .is_err()
    );
    assert!(
        f.try_exec(Command::SetCells {
            sheet: s,
            cells: vec![CellInput::value(0, MAX_COLS, 1)],
        })
        .is_err()
    );
    // Whole-column formatting does not create a million cells.
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cols(3, 5)],
        patch: Box::new(StylePatch {
            num_fmt: Some("0%".into()),
            ..StylePatch::default()
        }),
    });
    assert_eq!(f.wb.sheet(s).map(|x| x.cells().len()), Some(3));
    f.exec(Command::DeleteRows {
        sheet: s,
        at: 0,
        count: MAX_ROWS - 1,
    });
    assert_eq!(f.formula(s, "A1"), None);
    assert_eq!(f.val(s, "XFD1"), CellValue::text("corner"));
}
