// SPDX-License-Identifier: AGPL-3.0-or-later
//! Incremental recalculation, ordering, cycles, defined names, sheets, structured references and
//! ad-hoc evaluation (ported from the v1.0 TypeScript suite).

mod engine_support;

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use engine_support::*;
use kiln_formula::engine::{
    CellRef, Engine, EngineHost, EngineOptions, IterativeOptions, MemoryHost, SheetId, TableDef,
};
use kiln_formula::refs::{Bounds, column_to_letters};
use kiln_formula::values::{ErrorValue, Value};

fn at(e: &Engine, a1: &str) -> CellRef {
    let (row, col) = addr(a1);
    CellRef {
        sheet: sheet1(e),
        row,
        col,
    }
}

#[test]
fn only_recalculates_what_depends_on_a_change() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("B1", 10.into()),
        ("A2", "=A1+1".into()),
        ("A3", "=A2+1".into()),
        ("B2", "=B1*2".into()),
        ("C1", "=A3+B2".into()),
    ]);
    assert_eq!(e.recalc().evaluated, 4);
    fill(&mut e, &[("B1", 20.into())]);
    assert_eq!(e.recalc().evaluated, 2);
    assert_eq!(get(&e, "C1"), num(43.0));
    assert_eq!(e.recalc().evaluated, 0);
}

#[test]
fn reports_which_formula_cells_changed_value() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("A2", "=A1*0".into()),
        ("A3", "=A1+1".into()),
    ]);
    e.recalc();
    fill(&mut e, &[("A1", 5.into())]);
    let r = e.recalc();
    assert_eq!(r.changed, vec![at(&e, "A3")]);
}

#[test]
fn recalc_all_re_evaluates_everything() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("A2", "=A1+1".into()),
        ("A3", "=A2+1".into()),
    ]);
    e.recalc();
    assert_eq!(e.recalc_all().evaluated, 2);
}

#[test]
fn formulas_entered_before_their_precedents_still_calculate_in_order() {
    let mut e = sheet(&[
        ("A5", "=A4*2".into()),
        ("A4", "=A3+1".into()),
        ("A3", "=A2+1".into()),
        ("A2", "=A1+1".into()),
        ("A1", 1.into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "A5"), num(8.0));
}

#[test]
fn a_range_precedent_is_a_single_graph_node() {
    let mut e = engine();
    for r in 0..50 {
        e.set_formula("Sheet1", r, 1, "=SUM(A:A)").expect("set");
    }
    assert_eq!(e.graph_stats().range_nodes, 1);
    assert_eq!(e.graph_stats().compiled, 1, "one shared compiled formula");
    fill(&mut e, &[("A7", 3.into())]);
    e.recalc();
    assert_eq!(get(&e, "B50"), num(3.0));
    fill(&mut e, &[("A1000", 4.into())]);
    assert_eq!(e.recalc().evaluated, 50);
    assert_eq!(get(&e, "B1"), num(7.0));
}

#[test]
fn formulas_reading_a_range_recalculate_when_a_formula_inside_it_changes() {
    let mut e = sheet(&[
        ("Z1", 1.into()),
        ("A1", "=Z1*10".into()),
        ("A2", 5.into()),
        ("B1", "=SUM(A1:A2)".into()),
        ("C1", "=B1+1".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(16.0));
    fill(&mut e, &[("Z1", 2.into())]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(26.0));
}

#[test]
fn clearing_a_formula_updates_its_dependents() {
    let mut e = sheet(&[("A1", "=2+2".into()), ("B1", "=A1*2".into())]);
    e.recalc();
    e.clear_cell("Sheet1", 0, 0).expect("clear");
    e.recalc();
    assert_eq!(get(&e, "B1"), num(0.0));
    assert!(!e.has_formula("Sheet1", 0, 0));
}

#[test]
fn replacing_a_formula_with_a_value_updates_dependents() {
    let mut e = sheet(&[("A1", "=1".into()), ("B1", "=A1+1".into())]);
    e.recalc();
    fill(&mut e, &[("A1", 41.into())]);
    e.recalc();
    assert_eq!(get(&e, "B1"), num(42.0));
    assert_eq!(e.formula("Sheet1", 0, 0), None);
}

#[test]
fn host_side_value_changes_are_picked_up_via_cell_changed() {
    let mut e = engine();
    let s = sheet1(&e);
    e.set_formula("Sheet1", 0, 1, "=A1*3").expect("set");
    e.host_mut().set(s, 0, 0, Some(Value::Number(2.0)));
    e.cell_changed("Sheet1", 0, 0).expect("changed");
    e.recalc();
    assert_eq!(get(&e, "B1"), num(6.0));
}

#[test]
fn volatile_functions_recalculate_every_time_with_their_dependents() {
    let clock = Arc::new(AtomicU64::new(NOW.to_bits()));
    let c = clock.clone();
    let mut e = engine_with(EngineOptions {
        clock: Some(Arc::new(move || f64::from_bits(c.load(Ordering::SeqCst)))),
        ..options()
    });
    fill(
        &mut e,
        &[
            ("A1", "=NOW()".into()),
            ("B1", "=A1+1".into()),
            ("C1", 5.into()),
            ("D1", "=C1*2".into()),
        ],
    );
    e.recalc();
    assert_eq!(get(&e, "B1"), num(45307.5));
    clock.store((NOW + 1.0).to_bits(), Ordering::SeqCst);
    let r = e.recalc();
    assert_eq!(r.evaluated, 2);
    assert_eq!(get(&e, "B1"), num(45308.5));
}

#[test]
fn indirect_and_offset_see_changes_to_cells_they_reach_dynamically() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("A2", 2.into()),
        ("B1", "=INDIRECT(\"A\"&C1)".into()),
        ("C1", 1.into()),
        ("D1", "=SUM(OFFSET(A1,0,0,C1,1))".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "B1"), num(1.0));
    fill(&mut e, &[("C1", 2.into())]);
    e.recalc();
    assert_eq!(get(&e, "B1"), num(2.0));
    assert_eq!(get(&e, "D1"), num(3.0));
    fill(&mut e, &[("A2", 20.into())]);
    e.recalc();
    assert_eq!(get(&e, "B1"), num(20.0));
}

#[test]
fn a_syntax_error_keeps_the_old_content() {
    let mut e = sheet(&[("A1", "=1+1".into())]);
    let err = e.set_formula("Sheet1", 0, 0, "=1+").err();
    assert!(err.is_some_and(|x| x.to_string().contains("end of formula")));
    assert_eq!(e.formula("Sheet1", 0, 0), Some("=1+1"));
}

#[test]
fn rejects_out_of_range_coordinates_and_unknown_sheets() {
    let mut e = engine();
    assert!(e.set_value("Sheet1", 1_048_576, 0, 1.0).is_err());
    assert!(e.set_value("Sheet1", 0, 16_384, 1.0).is_err());
    let err = e.set_value("Nope", 0, 0, 1.0).err();
    assert!(err.is_some_and(|x| x.to_string().contains("unknown sheet")));
}

#[test]
fn accepts_formulas_without_the_leading_equals() {
    let mut e = engine();
    e.set_formula("Sheet1", 0, 0, "6*7").expect("set");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(42.0));
    assert_eq!(e.formula("Sheet1", 0, 0), Some("=6*7"));
}

/// Reference model for the random-DAG property test.
fn model(
    values: &std::collections::HashMap<String, f64>,
    formulas: &std::collections::HashMap<String, (Vec<String>, Vec<f64>)>,
    cell: &str,
    memo: &mut std::collections::HashMap<String, f64>,
) -> f64 {
    if let Some(&m) = memo.get(cell) {
        return m;
    }
    let v = match formulas.get(cell) {
        Some((refs, coef)) => {
            let mut v = 1.0;
            for (r, c) in refs.iter().zip(coef) {
                v += model(values, formulas, r, memo) * c;
            }
            v
        }
        None => values.get(cell).copied().unwrap_or(0.0),
    };
    memo.insert(cell.to_owned(), v);
    v
}

fn random_dag(seed: u32, parallel: bool) {
    use std::collections::HashMap;
    let _: Option<HashMap<u8, u8>> = None;
    let mut r = Rng(seed * 7919);
    let mut cells: Vec<String> = Vec::new();
    for c in 0..8 {
        for row in 1..=12 {
            cells.push(format!("{}{row}", column_to_letters(c)));
        }
    }
    // A random topological order, so the dependency order differs from the sheet order.
    let mut order = cells.clone();
    for i in (1..order.len()).rev() {
        let j = r.int(0, i as i64) as usize;
        order.swap(i, j);
    }
    let mut st = DagState {
        order,
        values: HashMap::new(),
        formulas: HashMap::new(),
    };
    let mut e = engine();
    e.set_parallel(parallel);
    let mut entry: Vec<usize> = (0..st.order.len()).collect();
    for i in (1..entry.len()).rev() {
        let j = r.int(0, i as i64) as usize;
        entry.swap(i, j);
    }
    for i in entry {
        st.set_random(&mut e, &mut r, i);
    }
    st.check(&mut e, &cells);
    for _ in 0..15 {
        for _ in 0..3 {
            let i = r.int(0, st.order.len() as i64 - 1) as usize;
            st.set_random(&mut e, &mut r, i);
        }
        st.check(&mut e, &cells);
    }
}

struct DagState {
    order: Vec<String>,
    values: std::collections::HashMap<String, f64>,
    formulas: std::collections::HashMap<String, (Vec<String>, Vec<f64>)>,
}

impl DagState {
    fn set_random(&mut self, e: &mut Engine, r: &mut Rng, i: usize) {
        let cell = self.order[i].clone();
        let (row, col) = addr(&cell);
        if i < 5 || r.next() < 0.35 {
            let v = r.int(-9, 9) as f64;
            self.values.insert(cell.clone(), v);
            self.formulas.remove(&cell);
            e.set_value("Sheet1", row, col, v).expect("value");
        } else {
            let n = r.int(1, 3);
            let refs: Vec<String> = (0..n)
                .map(|_| self.order[r.int(0, i as i64 - 1) as usize].clone())
                .collect();
            let coef: Vec<f64> = refs.iter().map(|_| r.int(-2, 2) as f64).collect();
            let text = format!(
                "=1{}",
                refs.iter()
                    .zip(&coef)
                    .map(|(x, c)| format!("+{x}*{c}"))
                    .collect::<String>()
            );
            self.values.remove(&cell);
            self.formulas.insert(cell.clone(), (refs, coef));
            e.set_formula("Sheet1", row, col, &text).expect("formula");
        }
    }

    fn check(&self, e: &mut Engine, cells: &[String]) {
        e.recalc();
        let mut memo = std::collections::HashMap::new();
        for c in cells {
            let want = model(&self.values, &self.formulas, c, &mut memo);
            assert_eq!(get(e, c), num(want), "{c}");
        }
    }
}

#[test]
fn random_dags_match_a_reference_evaluator_after_edits() {
    for seed in 1..=6 {
        random_dag(seed, false);
        random_dag(seed, true);
    }
}

#[test]
fn evaluates_each_dirty_formula_exactly_once_on_a_diamond() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("B1", "=A1+1".into()),
        ("B2", "=A1+2".into()),
        ("C1", "=B1+B2".into()),
        ("D1", "=C1+B1+B2".into()),
    ]);
    e.recalc();
    fill(&mut e, &[("A1", 2.into())]);
    assert_eq!(e.recalc().evaluated, 4);
    assert_eq!(get(&e, "D1"), num(14.0));
}

#[test]
fn parallel_levels_give_the_same_results_as_serial() {
    let build = |parallel: bool| {
        let mut e = engine();
        e.set_parallel(parallel);
        for r in 0..2000u32 {
            e.set_value("Sheet1", r, 0, f64::from(r % 97)).expect("v");
            e.set_formula("Sheet1", r, 1, &format!("=A{}*2+1", r + 1))
                .expect("f");
            e.set_formula("Sheet1", r, 2, &format!("=IF(B{0}>100,B{0},-B{0})", r + 1))
                .expect("f");
            let prev = if r == 0 {
                "0".to_owned()
            } else {
                format!("D{r}")
            };
            e.set_formula(
                "Sheet1",
                r,
                3,
                &format!("=SUM(A{}:C{})+{prev}*0", r + 1, r + 1),
            )
            .expect("f");
        }
        e.set_formula("Sheet1", 0, 4, "=SUM(D1:D2000)").expect("f");
        e.recalc();
        e
    };
    let a = build(false);
    let b = build(true);
    for r in 0..2000u32 {
        for c in 0..4u32 {
            assert_eq!(a.value("Sheet1", r, c).ok(), b.value("Sheet1", r, c).ok());
        }
    }
    assert_eq!(get(&a, "E1"), get(&b, "E1"));
}

#[test]
fn flags_a_cycle_and_shows_zero_like_excel() {
    let mut e = sheet(&[
        ("A1", "=B1+1".into()),
        ("B1", "=A1+1".into()),
        ("C1", "=A1+100".into()),
    ]);
    let r = e.recalc();
    assert_eq!(get(&e, "A1"), num(0.0));
    assert_eq!(get(&e, "B1"), num(0.0));
    assert_eq!(get(&e, "C1"), num(100.0));
    assert_eq!(r.circular.len(), 2);
    assert!(e.is_circular("Sheet1", 0, 0));
    assert!(!e.is_circular("Sheet1", 0, 2));
}

#[test]
fn detects_a_self_reference_and_a_long_cycle() {
    let mut e = sheet(&[("A1", "=A1+1".into())]);
    e.recalc();
    assert_eq!(e.circular_cells(), vec![at(&e, "A1")]);
    let mut big = engine();
    for i in 1..=5000u32 {
        let next = if i == 5000 { 1 } else { i + 1 };
        big.set_formula("Sheet1", i - 1, 0, &format!("=A{next}+1"))
            .expect("set");
    }
    big.recalc();
    assert_eq!(big.circular_cells().len(), 5000);
}

#[test]
fn clears_the_flag_once_the_cycle_is_broken() {
    let mut e = sheet(&[("A1", "=B1+1".into()), ("B1", "=A1+1".into())]);
    e.recalc();
    fill(&mut e, &[("B1", 5.into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), num(6.0));
    assert_eq!(e.circular_cells(), vec![]);
}

#[test]
fn a_cycle_through_a_range_is_detected() {
    let mut e = sheet(&[("A1", 1.into()), ("A2", "=SUM(A1:A3)".into())]);
    e.recalc();
    assert!(e.is_circular("Sheet1", 1, 0));
}

fn iterative(max_iterations: u32, max_change: f64) -> EngineOptions {
    EngineOptions {
        iterative: IterativeOptions {
            enabled: true,
            max_iterations,
            max_change,
        },
        ..options()
    }
}

#[test]
fn iterative_calculation_converges() {
    let mut e = engine_with(iterative(200, 1e-9));
    fill(&mut e, &[("A1", "=B1+1".into()), ("B1", "=A1*0.5".into())]);
    e.recalc();
    let a = get(&e, "A1").as_number().unwrap_or(f64::NAN);
    let b = get(&e, "B1").as_number().unwrap_or(f64::NAN);
    assert!((a - 2.0).abs() < 1e-6 && (b - 1.0).abs() < 1e-6, "{a} {b}");
    assert!(e.is_circular("Sheet1", 0, 0));
}

#[test]
fn iterative_calculation_stops_at_the_iteration_limit() {
    let mut e = engine_with(iterative(7, 0.001));
    fill(&mut e, &[("A1", "=A1+1".into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), num(7.0));
    e.recalc_all();
    assert_eq!(get(&e, "A1"), num(14.0));
}

#[test]
fn cells_downstream_of_a_cycle_are_computed_after_it() {
    let mut e = engine_with(iterative(100, 1e-12));
    fill(
        &mut e,
        &[("A1", "=A1*0.5+5".into()), ("B1", "=A1*2".into())],
    );
    e.recalc();
    let b = get(&e, "B1").as_number().unwrap_or(f64::NAN);
    assert!((b - 20.0).abs() < 1e-6, "{b}");
}

#[test]
fn names_refer_to_ranges_constants_and_formulas() {
    let mut e = sheet(&[("A1", 1.into()), ("A2", 2.into()), ("A3", 3.into())]);
    e.define_name("Data", "=Sheet1!$A$1:$A$3", None)
        .expect("name");
    e.define_name("Rate", "=0.5", None).expect("name");
    e.define_name("Doubled", "=SUM(Data)*2", None)
        .expect("name");
    fill(
        &mut e,
        &[
            ("B1", "=SUM(Data)*Rate".into()),
            ("B2", "=Doubled".into()),
            ("B3", "=Missing".into()),
        ],
    );
    e.recalc();
    assert_eq!(get(&e, "B1"), num(3.0));
    assert_eq!(get(&e, "B2"), num(12.0));
    assert_eq!(get(&e, "B3"), error(ErrorValue::Name));
}

#[test]
fn users_recalculate_when_a_name_is_defined_redefined_or_removed() {
    let mut e = sheet(&[("A1", "=Rate*10".into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), error(ErrorValue::Name));
    e.define_name("Rate", "=2", None).expect("name");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(20.0));
    e.define_name("Rate", "=3", None).expect("name");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(30.0));
    assert!(e.remove_name("Rate", None));
    e.recalc();
    assert_eq!(get(&e, "A1"), error(ErrorValue::Name));
}

#[test]
fn tracks_cells_a_name_refers_to() {
    let mut e = sheet(&[("A1", 1.into())]);
    e.define_name("Base", "=Sheet1!$A$1", None).expect("name");
    fill(&mut e, &[("B1", "=Base+1".into())]);
    e.recalc();
    fill(&mut e, &[("A1", 10.into())]);
    e.recalc();
    assert_eq!(get(&e, "B1"), num(11.0));
}

#[test]
fn sheet_scoped_names_win_over_workbook_names_on_their_sheet() {
    let mut e = engine();
    let other = e.add_sheet("Other").expect("sheet");
    e.define_name("Tax", "=0.1", None).expect("name");
    e.define_name("Tax", "=0.2", Some(other)).expect("name");
    e.set_formula("Sheet1", 0, 0, "=Tax").expect("f");
    e.set_formula("Other", 0, 0, "=Tax").expect("f");
    e.set_formula("Sheet1", 0, 1, "=Other!Tax").expect("f");
    e.recalc();
    assert_eq!(e.value("Sheet1", 0, 0).ok(), Some(num(0.1)));
    assert_eq!(e.value("Other", 0, 0).ok(), Some(num(0.2)));
    assert_eq!(e.value("Sheet1", 0, 1).ok(), Some(num(0.2)));
    assert_eq!(e.names().len(), 2);
}

#[test]
fn a_name_that_refers_to_itself_is_a_name_error_rather_than_a_hang() {
    let mut e = engine();
    e.define_name("Loop", "=Loop+1", None).expect("name");
    assert_eq!(e.evaluate("=Loop").ok(), Some(error(ErrorValue::Name)));
}

#[test]
fn validates_names() {
    let mut e = engine();
    for bad in [
        "A1",
        "R2C3",
        "has space",
        "TRUE",
        "1abc",
        "",
        "R",
        "C",
        "XFD1048576",
    ] {
        assert!(e.define_name(bad, "=1", None).is_err(), "{bad}");
    }
    for ok in ["_ok.Name\\2", "Tax", "Sales2024", "Données", "A1B"] {
        assert!(e.define_name(ok, "=1", None).is_ok(), "{ok}");
    }
}

#[test]
fn reads_other_sheets_and_3d_ranges_in_workbook_order() {
    let mut e = Engine::with_options(options());
    for s in ["Jan", "Feb", "Mar", "Summary"] {
        e.add_sheet(s).expect("sheet");
    }
    e.set_value("Jan", 0, 0, 1.0).expect("v");
    e.set_value("Feb", 0, 0, 2.0).expect("v");
    e.set_value("Mar", 0, 0, 4.0).expect("v");
    e.set_formula("Summary", 0, 0, "=SUM(Jan:Mar!A1)")
        .expect("f");
    e.set_formula("Summary", 0, 1, "=Feb!A1*10").expect("f");
    e.recalc();
    assert_eq!(e.value("Summary", 0, 0).ok(), Some(num(7.0)));
    assert_eq!(e.value("Summary", 0, 1).ok(), Some(num(20.0)));
    e.set_value("Feb", 0, 0, 3.0).expect("v");
    e.recalc();
    assert_eq!(e.value("Summary", 0, 0).ok(), Some(num(8.0)));
    e.move_sheet("Mar", 3).expect("move");
    e.move_sheet("Summary", 0).expect("move");
    e.recalc();
    assert_eq!(e.sheets(), ["Summary", "Jan", "Feb", "Mar"]);
    // The 3-D span still runs from Jan to Mar in the new order: Jan, Feb, Mar.
    assert_eq!(e.value("Summary", 0, 0).ok(), Some(num(8.0)));
}

#[test]
fn a_reference_to_a_missing_sheet_is_ref_until_the_sheet_is_added() {
    let mut e = sheet(&[("A1", "=Later!A1+1".into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), error(ErrorValue::Ref));
    e.add_sheet("Later").expect("sheet");
    e.set_value("Later", 0, 0, 41.0).expect("v");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(42.0));
}

#[test]
fn rejects_duplicate_sheet_names() {
    let mut e = engine();
    let err = e.add_sheet("sheet1").err();
    assert!(err.is_some_and(|x| x.to_string().contains("already exists")));
}

fn table_engine() -> (Engine, SheetId) {
    let mut e = engine();
    let s = sheet1(&e);
    e.host_mut().add_table(TableDef {
        name: "Sales".into(),
        sheet: s,
        bounds: Bounds {
            r0: 0,
            c0: 0,
            r1: 4,
            c1: 3,
        },
        header_row: true,
        totals_row: true,
        columns: vec![
            "Item".into(),
            "Qty".into(),
            "Unit Price".into(),
            "Total".into(),
        ],
    });
    fill(
        &mut e,
        &[
            ("A1", "Item".into()),
            ("B1", "Qty".into()),
            ("C1", "Unit Price".into()),
            ("D1", "Total".into()),
            ("A2", "Pen".into()),
            ("B2", 2.into()),
            ("C2", 1.5.into()),
            ("A3", "Pad".into()),
            ("B3", 3.into()),
            ("C3", 4.into()),
            ("A4", "Ink".into()),
            ("B4", 1.into()),
            ("C4", 10.into()),
        ],
    );
    (e, s)
}

#[test]
fn resolves_column_special_item_and_this_row_references() {
    let (mut e, _) = table_engine();
    fill(
        &mut e,
        &[
            ("D2", "=[@Qty]*[@[Unit Price]]".into()),
            ("D3", "=Sales[@Qty]*Sales[@[Unit Price]]".into()),
            ("B5", "=SUM(Sales[Qty])".into()),
            ("F1", "=COUNTA(Sales[#Headers])".into()),
            ("F2", "=COUNTA(Sales[[#Headers],[#Data],[Item]])".into()),
            ("F3", "=SUM(Sales[[Qty]:[Unit Price]])".into()),
            ("F4", "=COUNTA(Sales[#All])".into()),
            ("F5", "=SUM(Sales)".into()),
            ("F6", "=Sales[[#Totals],[Qty]]".into()),
            ("F7", "=Sales[Nope]".into()),
            ("F8", "=Missing[Qty]".into()),
        ],
    );
    e.recalc();
    assert_eq!(get(&e, "D2"), num(3.0));
    assert_eq!(get(&e, "D3"), num(12.0));
    assert_eq!(get(&e, "B5"), num(6.0));
    assert_eq!(get(&e, "F1"), num(4.0));
    assert_eq!(get(&e, "F2"), num(4.0));
    assert_eq!(get(&e, "F3"), num(21.5));
    assert_eq!(get(&e, "F4"), num(16.0));
    assert_eq!(get(&e, "F5"), num(36.5));
    assert_eq!(get(&e, "F6"), num(6.0));
    assert_eq!(get(&e, "F7"), error(ErrorValue::Ref));
    assert_eq!(get(&e, "F8"), error(ErrorValue::Ref));
}

#[test]
fn recalculates_when_table_data_or_tables_change() {
    let (mut e, s) = table_engine();
    fill(&mut e, &[("F1", "=SUM(Sales[Qty])".into())]);
    e.recalc();
    assert_eq!(get(&e, "F1"), num(6.0));
    fill(&mut e, &[("B3", 30.into())]);
    e.recalc();
    assert_eq!(get(&e, "F1"), num(33.0));
    e.host_mut().add_table(TableDef {
        name: "Sales".into(),
        sheet: s,
        bounds: Bounds {
            r0: 0,
            c0: 0,
            r1: 2,
            c1: 3,
        },
        header_row: true,
        totals_row: false,
        columns: vec![
            "Item".into(),
            "Qty".into(),
            "Unit Price".into(),
            "Total".into(),
        ],
    });
    e.tables_changed();
    e.recalc();
    assert_eq!(get(&e, "F1"), num(32.0));
}

#[test]
fn this_row_outside_the_data_rows_is_value_and_unqualified_outside_a_table_is_ref() {
    let (mut e, _) = table_engine();
    fill(
        &mut e,
        &[("E1", "=Sales[@Qty]".into()), ("E2", "=[@Qty]".into())],
    );
    e.recalc();
    assert_eq!(get(&e, "E1"), error(ErrorValue::Value));
    assert_eq!(get(&e, "E2"), error(ErrorValue::Ref));
}

#[test]
fn evaluates_a_formula_at_a_position_without_storing_it() {
    let mut e = sheet(&[("A1", 1.into()), ("A2", 2.into()), ("A3", 3.into())]);
    e.recalc();
    assert_eq!(
        e.evaluate_at("=ROW()", "Sheet1", 9, 0, false).ok(),
        Some(num(10.0))
    );
    assert_eq!(
        e.evaluate_at("=A1:A3*2", "Sheet1", 0, 0, false).ok(),
        Some(arr(vec![vec![num(2.0)], vec![num(4.0)], vec![num(6.0)]]))
    );
    assert_eq!(
        e.evaluate_at("=A1:A3", "Sheet1", 1, 3, true).ok(),
        Some(num(2.0))
    );
    assert_eq!(e.formula_count(), 0);
}

#[test]
fn pulls_pending_formulas_when_evaluating_ad_hoc() {
    let mut e = sheet(&[("A1", "=2*3".into())]);
    assert_eq!(e.evaluate("=A1+1").ok(), Some(num(7.0)));
}

#[test]
fn a_read_only_host_works_for_formulas_and_refuses_values() {
    #[derive(Default)]
    struct ReadOnly(MemoryHost);
    impl EngineHost for ReadOnly {
        fn cell_value(&self, sheet: SheetId, row: u32, col: u32) -> Option<Value> {
            self.0.cell_value(sheet, row, col)
        }
    }
    let mut e = Engine::with_host(ReadOnly::default(), options());
    let s = e.add_sheet("S").expect("sheet");
    e.host_mut().0.set(s, 0, 0, Some(Value::Number(9.0)));
    e.set_formula("S", 0, 1, "=A1+1").expect("f");
    e.recalc();
    assert_eq!(e.value("S", 0, 1).ok(), Some(num(10.0)));
    let err = e.set_value("S", 0, 0, 1.0).err();
    assert!(err.is_some_and(|x| x.to_string().contains("does not accept values")));
}
