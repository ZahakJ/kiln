// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formula binding: the workbook model and the formula engine stay in step through edits,
//! structural changes, tables, names, sheets and calculation modes.

mod support;

use kiln_sheet::commands::{CellInput, Content, RowPatch, TablePatch};
use kiln_sheet::meta::{CalcMode, Table, TableColumn, WorkbookProps};
use kiln_sheet::styles::StylePatch;
use kiln_sheet::{CellValue, Command, Range, Workbook};
use support::{assert_engine_in_sync, setup};

fn table(name: &str, range: Range, cols: &[&str]) -> Table {
    Table {
        id: 0,
        name: name.into(),
        display_name: None,
        range,
        header_row: true,
        totals_row: false,
        columns: cols.iter().map(|c| TableColumn::named(*c)).collect(),
        style: None,
        auto_filter: None,
        comment: None,
    }
}

fn values(rows: &[&[CellValue]]) -> Vec<Vec<Option<CellValue>>> {
    rows.iter()
        .map(|r| r.iter().map(|v| Some(v.clone())).collect())
        .collect()
}

fn t(s: &str) -> CellValue {
    CellValue::text(s)
}

fn n(x: f64) -> CellValue {
    CellValue::Number(x)
}

#[test]
fn inserting_a_row_above_a_reference_shifts_formula_text_and_dependencies() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A5", 10);
    f.set(s, "A6", 32);
    f.set(s, "C1", "=A5+A6");
    f.set(s, "C2", "=SUM(A5:A6)");
    assert_eq!(f.num(s, "C1"), 42.0);
    f.exec(Command::InsertRows {
        sheet: s,
        at: 2,
        count: 1,
        format_from: Default::default(),
    });
    assert_eq!(f.formula(s, "C1").as_deref(), Some("A6+A7"));
    assert_eq!(f.formula(s, "C2").as_deref(), Some("SUM(A6:A7)"));
    f.wb.recalc_all();
    assert_eq!(f.num(s, "C1"), 42.0);
    assert_eq!(f.num(s, "C2"), 42.0);
    // The graph points at the moved cells: editing A6 (formerly A5) recalculates C1…
    f.set(s, "A6", 20);
    assert_eq!(f.num(s, "C1"), 52.0);
    assert_eq!(f.num(s, "C2"), 52.0);
    // …and the vacated row is no longer a precedent.
    f.set(s, "A5", 1000);
    assert_eq!(f.num(s, "C1"), 52.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn deleting_a_referenced_row_gives_ref_and_undo_restores_the_formula() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A3", 5);
    f.set(s, "B1", "=A3*2");
    f.exec(Command::DeleteRows {
        sheet: s,
        at: 2,
        count: 1,
    });
    assert_eq!(f.formula(s, "B1").as_deref(), Some("#REF!*2"));
    assert_eq!(f.err(s, "B1"), "#REF!");
    f.undo();
    assert_eq!(f.formula(s, "B1").as_deref(), Some("A3*2"));
    assert_eq!(f.num(s, "B1"), 10.0);
    f.redo();
    assert_eq!(f.err(s, "B1"), "#REF!");
    f.undo();
    assert_eq!(f.num(s, "B1"), 10.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn recalculates_only_dependents() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 1);
    for r in 2..=50 {
        f.set(s, &format!("A{r}"), format!("=A{}+1", r - 1).as_str());
    }
    assert_eq!(f.num(s, "A50"), 50.0);
    f.set(s, "A1", 100);
    assert_eq!(f.wb.last_evaluated(), 49);
    assert_eq!(f.num(s, "A50"), 149.0);
    f.set(s, "B11", "unrelated");
    assert_eq!(f.wb.last_evaluated(), 0);
}

#[test]
fn writes_formula_results_back_to_the_store() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 6);
    f.set(s, "B1", "=A1*7");
    let cached = f.wb.sheet(s).map(|x| x.cells().value(0, 1).clone());
    assert_eq!(cached, Some(n(42.0)));
}

#[test]
fn spills_dynamic_arrays_and_reports_spill_when_blocked() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", "=SEQUENCE(4)");
    assert_eq!(f.num(s, "A4"), 4.0);
    let spill = f.wb.spill_range(s, 2, 0).expect("spill range");
    assert_eq!(Range::from_bounds(spill.bounds), Range::new(0, 0, 3, 0));
    assert!(!spill.blocked);
    f.set(s, "A3", "x");
    assert_eq!(f.err(s, "A1"), "#SPILL!");
    f.set(s, "A3", ());
    assert_eq!(f.num(s, "A3"), 3.0);
    f.set(s, "C1", "=SUM(A1#)");
    assert_eq!(f.num(s, "C1"), 10.0);
}

#[test]
fn a_spill_blocked_by_formulas_reading_each_other_is_not_circular() {
    // D4 spills over E4:E5, which hold formulas, and E5 reads E4. Blocking depends only on the
    // formulas being there, so a full recalculation (as after opening the file or renaming the
    // sheet) must give #SPILL!, not a circular-reference 0.
    let mut f = setup(&["One"]);
    let s = f.id(0);
    f.set(s, "E4", "=1");
    f.set(s, "E5", "=E4+1");
    f.set(s, "D4", "=SEQUENCE(2,2)");
    assert_eq!(f.err(s, "D4"), "#SPILL!");
    f.wb.recalc_all();
    assert_eq!(f.err(s, "D4"), "#SPILL!");
    assert_eq!(f.num(s, "E5"), 2.0);
    assert!(f.wb.circular_cells().is_empty());
    f.exec(Command::RenameSheet {
        sheet: s,
        name: "Renamed".into(),
    });
    f.wb.recalc_all();
    assert_eq!(f.err(s, "D4"), "#SPILL!");
    let mut fresh = Workbook::from_snapshot(f.wb.snapshot(), Default::default()).expect("rebuilds");
    fresh.recalc_all();
    assert!(fresh.circular_cells().is_empty());
    assert_eq!(
        fresh.value(s, 3, 3),
        CellValue::Error(kiln_formula::values::ErrorValue::Spill)
    );
    f.set(s, "E4", ());
    f.set(s, "E5", ());
    assert_eq!(f.num(s, "E5"), 4.0);
}

#[test]
fn resolves_structured_references_through_the_tables_model() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetRange {
        sheet: s,
        row: 0,
        col: 0,
        values: values(&[
            &[t("Item"), t("Qty"), t("Price")],
            &[t("Pen"), n(3.0), n(1.5)],
            &[t("Book"), n(2.0), n(12.0)],
            &[t("Cup"), n(4.0), n(5.0)],
        ]),
    });
    f.exec(Command::AddTable {
        sheet: s,
        table: Box::new(table(
            "Sales",
            Range::new(0, 0, 3, 2),
            &["Item", "Qty", "Price"],
        )),
    });
    f.set(s, "A6", "=SUM(Sales[Qty])");
    f.set(s, "B6", "=SUMPRODUCT(Sales[Qty],Sales[Price])");
    f.set(s, "C6", "=ROWS(Sales[#All])");
    assert_eq!(f.num(s, "A6"), 9.0);
    assert_eq!(f.num(s, "B6"), 48.5);
    assert_eq!(f.num(s, "C6"), 4.0);
    // An unqualified [@Col] outside the table has no table to resolve against.
    f.set(s, "E2", "=[@Qty]*[@Price]");
    assert_eq!(f.err(s, "E2"), "#REF!");
    // Grow the table by a row: SUM picks it up.
    f.exec(Command::UpdateTable {
        name: "Sales".into(),
        patch: Box::new(TablePatch {
            range: Some(Range::new(0, 0, 4, 2)),
            ..TablePatch::default()
        }),
    });
    f.set(s, "B5", 10);
    assert_eq!(f.num(s, "A6"), 19.0);
    // Renaming the table rewrites references.
    f.exec(Command::UpdateTable {
        name: "Sales".into(),
        patch: Box::new(TablePatch {
            name: Some("Orders".into()),
            ..TablePatch::default()
        }),
    });
    assert_eq!(f.formula(s, "A6").as_deref(), Some("SUM(Orders[Qty])"));
    assert_eq!(f.num(s, "A6"), 19.0);
    // Renaming a column through its header cell rewrites references too.
    f.set(s, "B1", "Units");
    let col = f.wb.sheet(s).map(|x| x.tables()[0].columns[1].name.clone());
    assert_eq!(col.as_deref(), Some("Units"));
    assert_eq!(f.formula(s, "A6").as_deref(), Some("SUM(Orders[Units])"));
    assert_eq!(f.num(s, "A6"), 19.0);
    f.undo();
    assert_eq!(f.formula(s, "A6").as_deref(), Some("SUM(Orders[Qty])"));
    let col = f.wb.sheet(s).map(|x| x.tables()[0].columns[1].name.clone());
    assert_eq!(col.as_deref(), Some("Qty"));
    assert_eq!(f.num(s, "A6"), 19.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn evaluates_this_row_references_and_converts_a_table_to_a_range() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetRange {
        sheet: s,
        row: 0,
        col: 0,
        values: values(&[
            &[t("Qty"), t("Price"), t("Total")],
            &[n(2.0), n(3.0)],
            &[n(4.0), n(5.0)],
        ]),
    });
    f.exec(Command::AddTable {
        sheet: s,
        table: Box::new(table(
            "T",
            Range::new(0, 0, 2, 2),
            &["Qty", "Price", "Total"],
        )),
    });
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![
            CellInput::formula(1, 2, "=[@Qty]*[@Price]"),
            CellInput::formula(2, 2, "=[@Qty]*[@Price]"),
            CellInput::formula(4, 0, "=SUM(T[Total])"),
        ],
    });
    assert_eq!(f.num(s, "C2"), 6.0);
    assert_eq!(f.num(s, "C3"), 20.0);
    assert_eq!(f.num(s, "A5"), 26.0);
    f.exec(Command::RemoveTable { name: "T".into() });
    assert_eq!(f.formula(s, "C2").as_deref(), Some("$A2*$B2"));
    assert_eq!(f.formula(s, "A5").as_deref(), Some("SUM($C$2:$C$3)"));
    assert_eq!(f.num(s, "A5"), 26.0);
    f.undo();
    assert_eq!(f.formula(s, "A5").as_deref(), Some("SUM(T[Total])"));
    assert_eq!(f.num(s, "A5"), 26.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn typing_below_a_table_extends_it() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.exec(Command::SetRange {
        sheet: s,
        row: 0,
        col: 0,
        values: values(&[&[t("Qty")], &[n(1.0)], &[n(2.0)]]),
    });
    f.exec(Command::AddTable {
        sheet: s,
        table: Box::new(table("T", Range::new(0, 0, 2, 0), &["Qty"])),
    });
    f.set(s, "C1", "=SUM(T[Qty])");
    f.set(s, "A4", 4);
    assert_eq!(
        f.wb.sheet(s).map(|x| x.tables()[0].range),
        Some(Range::new(0, 0, 3, 0))
    );
    assert_eq!(f.num(s, "C1"), 7.0);
    f.set(s, "B1", "Price");
    let cols: Vec<String> =
        f.wb.sheet(s)
            .map(|x| {
                x.tables()[0]
                    .columns
                    .iter()
                    .map(|c| c.name.clone())
                    .collect()
            })
            .unwrap_or_default();
    assert_eq!(cols, vec!["Qty", "Price"]);
    f.undo();
    f.undo();
    assert_eq!(
        f.wb.sheet(s).map(|x| x.tables()[0].range),
        Some(Range::new(0, 0, 2, 0))
    );
    assert_eq!(f.num(s, "C1"), 3.0);
}

#[test]
fn uses_defined_names_at_workbook_and_sheet_scope() {
    let mut f = setup(&["Data", "Calc"]);
    let (data, c) = (f.id(0), f.id(1));
    f.set(data, "A1", 0.2);
    f.exec(Command::DefineName {
        name: "Rate".into(),
        formula: "=Data!$A$1".into(),
        scope: None,
        hidden: false,
        comment: None,
    });
    f.exec(Command::DefineName {
        name: "Rate".into(),
        formula: "0.5".into(),
        scope: Some(c),
        hidden: false,
        comment: None,
    });
    f.set(data, "A2", "=Rate*100");
    f.set(c, "A1", "=Rate*100");
    assert_eq!(f.num(data, "A2"), 20.0);
    assert_eq!(f.num(c, "A1"), 50.0, "the local name wins");
    f.set(data, "A1", 0.3);
    assert_eq!(f.num(data, "A2"), 30.0);
    f.exec(Command::InsertRows {
        sheet: data,
        at: 0,
        count: 2,
        format_from: Default::default(),
    });
    assert_eq!(
        f.wb.find_name("Rate", None)
            .map(|n| n.formula.clone())
            .as_deref(),
        Some("Data!$A$3")
    );
    assert_eq!(f.num(data, "A4"), 30.0);
    f.exec(Command::DeleteName {
        name: "Rate".into(),
        scope: Some(c),
    });
    assert_eq!(f.num(c, "A1"), 30.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn follows_sheet_renames_moves_and_removal() {
    let mut f = setup(&["A", "B"]);
    let (a, b) = (f.id(0), f.id(1));
    f.set(b, "A1", 7);
    f.set(a, "A1", "=B!A1*2");
    f.exec(Command::RenameSheet {
        sheet: b,
        name: "Budget 2026".into(),
    });
    assert_eq!(f.formula(a, "A1").as_deref(), Some("'Budget 2026'!A1*2"));
    assert_eq!(f.num(a, "A1"), 14.0);
    f.exec(Command::MoveSheet { sheet: b, index: 0 });
    assert_eq!(f.wb.sheet_names(), vec!["Budget 2026", "A"]);
    f.exec(Command::RemoveSheet { sheet: b });
    assert_eq!(f.formula(a, "A1").as_deref(), Some("#REF!*2"));
    assert_eq!(f.err(a, "A1"), "#REF!");
    f.undo();
    assert_eq!(f.formula(a, "A1").as_deref(), Some("'Budget 2026'!A1*2"));
    assert_eq!(f.num(a, "A1"), 14.0);
    assert_eq!(f.num(b, "A1"), 7.0);
    f.undo();
    f.undo();
    assert_eq!(f.formula(a, "A1").as_deref(), Some("B!A1*2"));
    assert_eq!(f.num(a, "A1"), 14.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn binds_formulas_to_a_sheet_added_later_and_keeps_them_on_undo() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", "=Later!A1+1");
    assert_eq!(f.err(s, "A1"), "#REF!");
    f.exec(Command::AddSheet {
        name: Some("Later".into()),
        index: None,
        id: None,
    });
    let later = f.wb.sheet_by_name("Later").map(|x| x.id()).expect("added");
    f.set(later, "A1", 41);
    assert_eq!(f.num(s, "A1"), 42.0);
    f.undo();
    f.undo();
    assert_eq!(f.wb.sheets().len(), 1);
    assert_eq!(f.formula(s, "A1").as_deref(), Some("Later!A1+1"));
    assert_engine_in_sync(&f.wb);
}

fn with_mode(wb: &Workbook, mode: CalcMode) -> WorkbookProps {
    let mut p = wb.props().clone();
    p.calc.mode = mode;
    p
}

#[test]
fn honours_manual_calculation_mode() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 1);
    f.set(s, "B1", "=A1+1");
    let p = with_mode(&f.wb, CalcMode::Manual);
    f.exec(Command::SetProps { props: Box::new(p) });
    f.set(s, "A1", 10);
    assert!(f.wb.is_calc_pending());
    assert_eq!(f.num(s, "B1"), 2.0, "stale until F9");
    f.wb.recalc();
    assert_eq!(f.num(s, "B1"), 11.0);
    assert!(!f.wb.is_calc_pending());
    // Back to automatic: pending edits calculate at once.
    f.set(s, "A1", 20);
    let p = with_mode(&f.wb, CalcMode::Auto);
    f.exec(Command::SetProps { props: Box::new(p) });
    assert_eq!(f.num(s, "B1"), 21.0);
}

#[test]
fn recalculates_one_sheet_on_demand() {
    let mut f = setup(&["A", "B"]);
    let a = f.id(0);
    f.set(a, "A1", "=RAND()");
    let p = with_mode(&f.wb, CalcMode::Manual);
    f.exec(Command::SetProps { props: Box::new(p) });
    let before = f.num(a, "A1");
    let res = f.wb.recalc_sheet(a).expect("sheet exists");
    assert!(res.evaluated > 0);
    assert_ne!(f.num(a, "A1"), before);
}

#[test]
fn switches_date_systems_by_rebuilding_the_engine() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", "=DATE(2023,3,15)");
    assert_eq!(f.num(s, "A1"), 45000.0);
    let mut p = f.wb.props().clone();
    p.date1904 = true;
    f.exec(Command::SetProps { props: Box::new(p) });
    assert_eq!(f.num(s, "A1"), 45000.0 - 1462.0);
    assert_eq!(f.wb.display_text(s, 0, 0), "43538");
    f.undo();
    assert_eq!(f.num(s, "A1"), 45000.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn formats_displayed_values_with_the_cell_number_format() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", "=1/3");
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cell(0, 0)],
        patch: Box::new(StylePatch {
            num_fmt: Some("0.00%".into()),
            ..StylePatch::default()
        }),
    });
    assert_eq!(f.wb.display_text(s, 0, 0), "33.33%");
    f.set(s, "A2", "=DATE(2026,10,15)");
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cell(1, 0)],
        patch: Box::new(StylePatch {
            num_fmt: Some("yyyy-mm-dd".into()),
            ..StylePatch::default()
        }),
    });
    assert_eq!(f.wb.display_text(s, 1, 0), "2026-10-15");
}

#[test]
fn subtotal_skips_rows_hidden_by_a_filter_after_the_layout_changes() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    for r in 1..=4 {
        f.set(s, &format!("A{r}"), r);
    }
    f.set(s, "A6", "=SUBTOTAL(9,A1:A4)");
    f.set(s, "B6", "=SUBTOTAL(109,A1:A4)");
    assert_eq!(f.num(s, "A6"), 10.0);
    f.exec(Command::SetRowProps {
        sheet: s,
        at: 1,
        count: 1,
        patch: RowPatch {
            hidden: Some(true),
            filtered: Some(true),
            ..RowPatch::default()
        },
    });
    assert_eq!(f.num(s, "A6"), 8.0);
    assert_eq!(f.num(s, "B6"), 8.0);
    f.undo();
    assert_eq!(f.num(s, "A6"), 10.0);
    // Manually hidden rows still count for SUBTOTAL 9 but not 109.
    f.exec(Command::SetRowProps {
        sheet: s,
        at: 2,
        count: 1,
        patch: RowPatch {
            hidden: Some(true),
            ..RowPatch::default()
        },
    });
    assert_eq!(f.num(s, "A6"), 10.0);
    assert_eq!(f.num(s, "B6"), 7.0);
}

#[test]
fn explains_a_cell_formula_step_by_step() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 4);
    f.set(s, "B1", "=IF(A1>2,A1*10,0)");
    let step = f.wb.explain(s, 0, 1).expect("a formula");
    assert_eq!(step.display, "40");
    assert!(!step.children.is_empty());
    assert!(f.wb.explain(s, 5, 5).is_err());
}

#[test]
fn keeps_moved_ranges_consistent_between_model_and_engine() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 3);
    f.set(s, "A2", "=A1*2");
    f.set(s, "C1", "=A2+A1");
    f.exec(Command::MoveRange {
        sheet: s,
        range: Range::new(0, 0, 1, 0),
        to_sheet: None,
        to: kiln_sheet::CellPos::new(5, 4),
    });
    assert_eq!(f.formula(s, "E7").as_deref(), Some("E6*2"));
    assert_eq!(f.formula(s, "C1").as_deref(), Some("E7+E6"));
    assert_eq!(f.num(s, "C1"), 9.0);
    assert_eq!(f.num(s, "E7"), 6.0);
    assert_engine_in_sync(&f.wb);
    f.undo();
    assert_eq!(f.formula(s, "C1").as_deref(), Some("A2+A1"));
    assert_eq!(f.formula(s, "A2").as_deref(), Some("A1*2"));
    assert!(f.wb.cell(s, 6, 4).is_none());
    assert_eq!(f.num(s, "C1"), 9.0);
    assert_engine_in_sync(&f.wb);
}

#[test]
fn keeps_unparsable_formulas_from_files_as_raw_text() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.wb.load_formula(s, 0, 0, "SUM(", CellValue::Blank, false)
        .expect("loads");
    f.wb.load_formula(s, 0, 1, "1+1", CellValue::Blank, false)
        .expect("loads");
    f.wb.load_formula(s, 0, 2, "FUTURE.SYNTAX(", n(7.0), false)
        .expect("loads");
    f.wb.recalc();
    assert_eq!(f.err(s, "A1"), "#NAME?");
    assert_eq!(f.num(s, "B1"), 2.0);
    assert_eq!(f.num(s, "C1"), 7.0, "the cached result stays visible");
    assert_eq!(f.formula(s, "A1").as_deref(), Some("SUM("));
    assert_eq!(f.wb.engine().formula_count(), 1);
}

#[test]
fn iterative_calculation_follows_the_workbook_properties() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", "=A1/2+1");
    assert!(f.wb.is_circular(s, 0, 0));
    assert_eq!(f.wb.circular_cells().len(), 1);
    let mut p = f.wb.props().clone();
    p.calc.iterate = true;
    p.calc.iterate_count = 100;
    p.calc.iterate_delta = 1e-9;
    f.exec(Command::SetProps { props: Box::new(p) });
    assert!((f.num(s, "A1") - 2.0).abs() < 1e-6, "converges to 2");
}

#[test]
fn literal_edits_through_set_cells_keep_styles_and_notify_dependents() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "A1", 2);
    f.set(s, "B1", "=A1*A1");
    f.exec(Command::SetStyle {
        sheet: s,
        ranges: vec![Range::cell(0, 0)],
        patch: Box::new(StylePatch {
            num_fmt: Some("0.0".into()),
            ..StylePatch::default()
        }),
    });
    f.exec(Command::SetCells {
        sheet: s,
        cells: vec![CellInput {
            row: 0,
            col: 0,
            content: Content::Value(n(3.0)),
            style: None,
        }],
    });
    assert_eq!(f.wb.display_text(s, 0, 0), "3.0");
    assert_eq!(f.num(s, "B1"), 9.0);
}

#[test]
fn table_styles_band_rows_and_format_headers() {
    use kiln_sheet::meta::TableStyleInfo;
    use kiln_sheet::styles::{Dxf, Fill};
    use kiln_sheet::table_style::{TableStyleDef, TableStyleElement, TableStyleEntry};
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let mut t = table("T", Range::new(0, 0, 4, 1), &["a", "b"]);
    t.style = Some(TableStyleInfo::default());
    f.exec(Command::AddTable {
        sheet: s,
        table: Box::new(t),
    });
    let header = f.wb.table_format(s, 0, 0).expect("header format");
    assert_eq!(
        header.fill,
        Some(Fill::solid(kiln_sheet::Color::theme(4, 0.0)))
    );
    assert!(
        f.wb.table_format(s, 1, 0).and_then(|d| d.fill).is_some(),
        "first band"
    );
    assert!(
        f.wb.table_format(s, 2, 0).and_then(|d| d.fill).is_none(),
        "second band"
    );
    assert!(f.wb.table_format(s, 9, 0).is_none());
    // A custom style with two-row stripes.
    let band = f.wb.styles_mut().intern_dxf(Dxf {
        fill: Some(Fill::solid(kiln_sheet::Color::rgb(0xDDEEFF))),
        ..Dxf::default()
    });
    f.wb.styles_mut().set_table_style(TableStyleDef {
        name: "Mine".into(),
        table: true,
        pivot: false,
        elements: vec![TableStyleEntry {
            element: TableStyleElement::FirstRowStripe,
            dxf: band,
            size: 2,
        }],
    });
    f.exec(Command::UpdateTable {
        name: "T".into(),
        patch: Box::new(TablePatch {
            style: kiln_sheet::styles::Patch::Set(TableStyleInfo {
                name: "Mine".into(),
                ..TableStyleInfo::default()
            }),
            ..TablePatch::default()
        }),
    });
    let shaded: Vec<bool> = (1..=4)
        .map(|r| f.wb.table_format(s, r, 0).is_some())
        .collect();
    // Two shaded rows, then one plain (the second stripe keeps its default size of one).
    assert_eq!(shaded, vec![true, true, false, true]);
}
