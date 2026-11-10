// SPDX-License-Identifier: AGPL-3.0-or-later
//! PivotTable tests: aggregates against tables computed independently in Python
//! (`testdata/expected.py`), Excel's layouts, Show Values As, sorting, filters and drill-through.

use std::collections::BTreeMap;

use rustc_hash::FxHashMap;
use serde::Deserialize;

use super::render::{ReportCell, drill, render};
use super::*;

#[derive(Deserialize)]
struct Data {
    headers: Vec<String>,
    rows: Vec<Vec<serde_json::Value>>,
}

#[derive(Deserialize)]
struct Case {
    name: String,
    rows: Vec<String>,
    cols: Vec<String>,
    values: Vec<(String, String)>,
    #[serde(default, rename = "where")]
    filter: BTreeMap<String, serde_json::Value>,
    expected: BTreeMap<String, serde_json::Value>,
}

fn cell(v: &serde_json::Value) -> CellValue {
    match v {
        serde_json::Value::Null => CellValue::Blank,
        serde_json::Value::Number(n) => CellValue::Number(n.as_f64().unwrap_or(0.0)),
        serde_json::Value::String(s) => CellValue::text(s.as_str()),
        serde_json::Value::Bool(b) => CellValue::Bool(*b),
        _ => CellValue::Blank,
    }
}

fn data() -> (Vec<CellValue>, Vec<Vec<CellValue>>) {
    let d: Data = serde_json::from_str(include_str!("testdata/data.json")).expect("data.json");
    let headers = d
        .headers
        .iter()
        .map(|h| CellValue::text(h.as_str()))
        .collect();
    let rows = d
        .rows
        .iter()
        .map(|r| r.iter().map(cell).collect())
        .collect();
    (headers, rows)
}

fn table(headers: &[CellValue], rows: &[Vec<CellValue>]) -> PivotTable {
    let cache = PivotCache::from_rows(headers, rows).expect("cache");
    PivotTable::new(
        "PivotTable1",
        PivotSource::Range {
            sheet: "Data".into(),
            range: Range::new(0, 0, 400, 6),
        },
        CellPos::new(2, 0),
        cache,
    )
}

fn cells_of(r: &PivotReport) -> FxHashMap<(u32, u32), CellValue> {
    r.cells
        .iter()
        .map(|c: &ReportCell| ((c.row, c.col), c.value.clone()))
        .collect()
}

fn label_path(t: &PivotTable, axis: &[AxisItem], path: &[u32]) -> String {
    let fields: Vec<usize> = axis
        .iter()
        .filter_map(|a| match a {
            AxisItem::Field(f) => Some(*f),
            AxisItem::Values => None,
        })
        .collect();
    path.iter()
        .zip(&fields)
        .map(|(&item, &f)| {
            let v = &t.cache.fields[f].items[item as usize];
            if v.is_empty_text_or_blank() {
                "(blank)".to_owned()
            } else {
                v.display_general()
            }
        })
        .collect::<Vec<_>>()
        .join(" / ")
}

#[test]
fn aggregates_match_the_python_tables() {
    let (headers, rows) = data();
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("testdata/expected.json")).expect("expected.json");
    assert!(cases.len() >= 6);
    for case in cases {
        let mut t = table(&headers, &rows);
        let f = |t: &PivotTable, n: &str| t.field_index(n).expect("field");
        for r in &case.rows {
            let i = f(&t, r);
            t.place(Some(i), PivotArea::Rows, None, None);
        }
        for c in &case.cols {
            let i = f(&t, c);
            t.place(Some(i), PivotArea::Columns, None, None);
        }
        for (field, func) in &case.values {
            let i = f(&t, field);
            let func = PivotFunction::from_file_name(func).expect("function");
            t.place(Some(i), PivotArea::Values, None, Some(func));
        }
        for (field, item) in &case.filter {
            let i = f(&t, field);
            t.place(Some(i), PivotArea::Filters, None, None);
            if let Some(p) = t.pages.iter_mut().find(|p| p.field == i) {
                p.item = Some(cell(item));
            }
        }
        if !t.pages.is_empty() {
            t.at = CellPos::new(2 + t.pages.len() as u32 + 1, 0);
        }
        let report = render(&t).expect("report");
        let cells = cells_of(&report);
        let out = &report.output;
        let r = out.range.expect("range");
        let mut seen = 0;
        for (i, line) in out.rows.iter().enumerate() {
            if !line.values {
                continue;
            }
            let rk = if line.kind == PivotLineKind::Grand {
                String::new()
            } else {
                label_path(&t, &t.rows, &line.path)
            };
            for (j, col) in out.cols.iter().enumerate() {
                let ck = if col.kind == PivotLineKind::Grand {
                    String::new()
                } else {
                    label_path(&t, &t.cols, &col.path)
                };
                let v = usize::from(line.value.or(col.value).unwrap_or(0));
                let key = format!("{v}|{rk}|{ck}");
                let at = (
                    r.r0 + out.header_rows + i as u32,
                    r.c0 + out.label_cols + j as u32,
                );
                let got = cells.get(&at).cloned().unwrap_or_default();
                match case.expected.get(&key) {
                    Some(serde_json::Value::Number(n)) => {
                        let want = n.as_f64().expect("number");
                        let got = got.as_number().unwrap_or(f64::NAN);
                        assert!(
                            (got - want).abs() <= 1e-9 * want.abs().max(1.0),
                            "{}: {key} = {got}, Python says {want}",
                            case.name
                        );
                        seen += 1;
                    }
                    Some(serde_json::Value::String(e)) => {
                        assert_eq!(got.display_general(), *e, "{}: {key}", case.name);
                        seen += 1;
                    }
                    _ => assert!(
                        got.is_blank(),
                        "{}: {key} should be empty, is {got:?}",
                        case.name
                    ),
                }
            }
        }
        assert_eq!(
            seen,
            case.expected.len(),
            "{}: every expected value is shown once",
            case.name
        );
    }
}

fn small() -> PivotTable {
    let h: Vec<CellValue> = ["Region", "Product", "Amount"]
        .map(CellValue::text)
        .to_vec();
    let rows: Vec<Vec<CellValue>> = [
        ("East", "Apple", 10.0),
        ("West", "Banana", 5.0),
        ("East", "Banana", 20.0),
        ("West", "Apple", 7.0),
        ("East", "Apple", 1.0),
    ]
    .iter()
    .map(|(a, b, c)| {
        vec![
            CellValue::text(*a),
            CellValue::text(*b),
            CellValue::Number(*c),
        ]
    })
    .collect();
    let cache = PivotCache::from_rows(&h, &rows).expect("cache");
    PivotTable::new(
        "PivotTable1",
        PivotSource::Range {
            sheet: "Sheet1".into(),
            range: Range::new(0, 0, 5, 2),
        },
        CellPos::new(2, 0),
        cache,
    )
}

/// The report as text, one line per row, cells separated by `|`.
fn grid(t: &PivotTable) -> Vec<String> {
    let rep = render(t).expect("report");
    let cells = cells_of(&rep);
    let r = rep.output.range.expect("range");
    let top = rep.output.pages.map_or(r.r0, |p| p.r0);
    (top..=r.r1)
        .map(|row| {
            (r.c0..=r.c1)
                .map(|c| {
                    cells
                        .get(&(row, c))
                        .map(CellValue::display_general)
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

#[test]
fn a_new_report_is_a_placeholder_and_one_value_sums() {
    let mut t = small();
    let rep = render(&t).expect("report");
    assert!(rep.output.empty);
    assert!(rep.cells.is_empty());
    t.place(Some(2), PivotArea::Values, None, None);
    assert_eq!(t.values[0].name, "Sum of Amount");
    assert_eq!(grid(&t), ["Sum of Amount", "43"]);
    // Text fields count.
    t.place(Some(0), PivotArea::Values, None, None);
    assert_eq!(t.values[1].name, "Count of Region");
    assert_eq!(t.cols, [AxisItem::Values]);
    assert_eq!(grid(&t), ["Sum of Amount|Count of Region", "43|5"]);
}

#[test]
fn compact_outline_and_tabular_layouts() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(1), PivotArea::Rows, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    assert_eq!(
        grid(&t),
        [
            "Row Labels|Sum of Amount",
            "East|31",
            "Apple|11",
            "Banana|20",
            "West|12",
            "Apple|7",
            "Banana|5",
            "Grand Total|43"
        ]
    );
    let rep = render(&t).expect("report");
    let indents: Vec<u8> = rep
        .cells
        .iter()
        .filter(|c| c.col == 0)
        .map(|c| c.indent)
        .collect();
    assert_eq!(indents, [0, 0, 1, 1, 0, 1, 1, 0]);
    t.options.layout = PivotLayout::Outline;
    assert_eq!(
        grid(&t),
        [
            "Region|Product|Sum of Amount",
            "East||31",
            "|Apple|11",
            "|Banana|20",
            "West||12",
            "|Apple|7",
            "|Banana|5",
            "Grand Total||43"
        ]
    );
    t.options.layout = PivotLayout::Tabular;
    assert_eq!(
        grid(&t),
        [
            "Region|Product|Sum of Amount",
            "East|Apple|11",
            "|Banana|20",
            "East Total||31",
            "West|Apple|7",
            "|Banana|5",
            "West Total||12",
            "Grand Total||43"
        ]
    );
    t.options.repeat_labels = true;
    assert_eq!(grid(&t)[2], "East|Banana|20");
    // Subtotals at the bottom, then off; no grand total.
    t.options.layout = PivotLayout::Compact;
    t.options.repeat_labels = false;
    t.options.subtotals = SubtotalPlace::Bottom;
    assert_eq!(
        grid(&t)[1..5],
        ["East|", "Apple|11", "Banana|20", "East Total|31"]
    );
    t.options.subtotals = SubtotalPlace::Off;
    t.options.col_totals = false;
    assert_eq!(
        grid(&t),
        [
            "Row Labels|Sum of Amount",
            "East|",
            "Apple|11",
            "Banana|20",
            "West|",
            "Apple|7",
            "Banana|5"
        ]
    );
}

#[test]
fn columns_filters_and_grand_totals() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(1), PivotArea::Columns, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    assert_eq!(
        grid(&t),
        [
            "Sum of Amount|Column Labels||",
            "Row Labels|Apple|Banana|Grand Total",
            "East|11|20|31",
            "West|7|5|12",
            "Grand Total|18|25|43"
        ]
    );
    t.options.row_totals = false;
    assert_eq!(grid(&t)[1], "Row Labels|Apple|Banana");
    t.options.row_totals = true;
    // A filter above the table: the report moves down to make room.
    t.place(Some(1), PivotArea::Filters, None, None);
    t.at = render::table_origin(CellPos::new(0, 0), t.pages.len());
    t.pages[0].item = Some(CellValue::text("banana"));
    assert_eq!(
        grid(&t),
        [
            "Product|Banana",
            "|",
            "Row Labels|Sum of Amount",
            "East|20",
            "West|5",
            "Grand Total|25"
        ]
    );
    t.pages[0].item = None;
    t.fields[1].hidden = vec![CellValue::text("Apple")];
    assert_eq!(grid(&t)[0], "Product|(Multiple Items)");
    assert_eq!(grid(&t)[5], "Grand Total|25");
}

#[test]
fn two_values_with_a_column_field() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(1), PivotArea::Columns, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    t.place(Some(2), PivotArea::Values, None, Some(PivotFunction::Count));
    assert_eq!(t.values[1].name, "Count of Amount");
    assert_eq!(t.cols, [AxisItem::Field(1), AxisItem::Values]);
    let g = grid(&t);
    assert_eq!(g[0], "|Column Labels|||||");
    assert_eq!(
        g[1],
        "|Apple||Banana||Total Sum of Amount|Total Count of Amount"
    );
    assert_eq!(
        g[2],
        "Row Labels|Sum of Amount|Count of Amount|Sum of Amount|Count of Amount||"
    );
    assert_eq!(g[3], "East|11|2|20|1|31|3");
    // Σ Values on rows.
    t.place(None, PivotArea::Rows, None, None);
    let g = grid(&t);
    assert_eq!(g[1], "Row Labels|Apple|Banana|Grand Total");
    assert_eq!(
        g[2..5],
        ["East|||", "Sum of Amount|11|20|31", "Count of Amount|2|1|3"]
    );
}

#[test]
fn show_values_as() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(1), PivotArea::Columns, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    let at = |t: &PivotTable, row: usize| grid(t)[row].clone();
    t.values[0].show_as = ShowAs::PercentOfTotal;
    // 11/43, 20/43, 31/43
    let rep = render(&t).expect("report");
    let cells = cells_of(&rep);
    let v = cells[&(4, 1)].as_number().expect("number");
    assert!((v - 11.0 / 43.0).abs() < 1e-12);
    assert_eq!(
        rep.cells
            .iter()
            .find(|c| (c.row, c.col) == (4, 1))
            .and_then(|c| c.num_fmt.clone())
            .as_deref(),
        Some("0.00%")
    );
    t.values[0].show_as = ShowAs::PercentOfRow;
    assert_eq!(at(&t, 2), "East|0.35483871|0.64516129|1");
    t.values[0].show_as = ShowAs::PercentOfColumn;
    assert_eq!(at(&t, 4), "Grand Total|1|1|1");
    t.values[0].show_as = ShowAs::Difference;
    t.values[0].base_field = Some(1);
    t.values[0].base_item = BaseItem::Item(CellValue::text("Apple"));
    assert_eq!(at(&t, 2), "East||9|");
    t.values[0].base_item = BaseItem::Previous;
    assert_eq!(at(&t, 3), "West||-2|");
    t.values[0].show_as = ShowAs::RunningTotal;
    assert_eq!(at(&t, 2), "East|11|31|");
    t.values[0].show_as = ShowAs::RankDescending;
    assert_eq!(at(&t, 2), "East|2|1|");
    t.values[0].show_as = ShowAs::Index;
    let idx = 11.0 * 43.0 / (31.0 * 18.0);
    let rep = render(&t).expect("report");
    let v = cells_of(&rep)[&(4, 1)].as_number().expect("number");
    assert!((v - idx).abs() < 1e-12);
}

#[test]
fn sort_and_filters() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    t.fields[0].sort = PivotSort::Descending;
    assert_eq!(grid(&t)[1..3], ["West|12", "East|31"]);
    t.fields[0].sort = PivotSort::Ascending;
    t.fields[0].sort_by_value = Some(0);
    assert_eq!(grid(&t)[1..3], ["West|12", "East|31"]);
    t.fields[0].sort = PivotSort::Descending;
    assert_eq!(grid(&t)[1..3], ["East|31", "West|12"]);
    t.fields[0].sort_by_value = None;
    t.fields[0].label_filter = Some(PivotLabelFilter {
        op: PivotFilterOp::BeginsWith,
        value: "w".into(),
        value2: String::new(),
    });
    assert_eq!(
        grid(&t),
        ["Row Labels|Sum of Amount", "West|12", "Grand Total|12"]
    );
    t.fields[0].label_filter = None;
    t.place(Some(1), PivotArea::Rows, None, None);
    t.fields[1].value_filter = Some(PivotValueFilter::Top {
        value: 0,
        count: 1,
        bottom: false,
    });
    assert_eq!(
        grid(&t),
        [
            "Row Labels|Sum of Amount",
            "West|7",
            "Apple|7",
            "East|20",
            "Banana|20",
            "Grand Total|27"
        ]
    );
}

#[test]
fn drill_through_lists_the_source_rows() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(1), PivotArea::Columns, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    let rep = render(&t).expect("report");
    t.output = rep.output;
    // East × Apple: records 0 and 4.
    assert_eq!(drill(&t, 4, 1), Some(vec![0, 4]));
    // The grand total: everything.
    assert_eq!(drill(&t, 6, 3), Some(vec![0, 1, 2, 3, 4]));
    // Headers are not values.
    assert_eq!(drill(&t, 3, 1), None);
}

#[test]
fn custom_subtotals_and_blank_lines() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(1), PivotArea::Rows, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    t.fields[0].subtotal = PivotSubtotal::Custom(vec![PivotFunction::Max, PivotFunction::Count]);
    t.options.blank_rows = true;
    let g = grid(&t);
    assert_eq!(
        g,
        [
            "Row Labels|Sum of Amount",
            "East|",
            "Apple|11",
            "Banana|20",
            "East Max|20",
            "East Count|3",
            "|",
            "West|",
            "Apple|7",
            "Banana|5",
            "West Max|7",
            "West Count|2",
            "Grand Total|43"
        ]
    );
}

#[test]
fn removing_values_keeps_references_straight() {
    let mut t = small();
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    t.place(Some(2), PivotArea::Values, None, Some(PivotFunction::Max));
    t.fields[0].sort_by_value = Some(1);
    t.remove_value(0);
    assert_eq!(t.fields[0].sort_by_value, Some(0));
    assert!(!t.cols.contains(&AxisItem::Values));
    t.remove_field(2);
    assert!(t.values.is_empty());
    assert_eq!(t.fields[0].sort_by_value, None);
}
