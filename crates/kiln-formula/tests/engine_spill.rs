// SPDX-License-Identifier: AGPL-3.0-or-later
//! Dynamic arrays: spilling, `#SPILL!` on collisions, the `#` spill-range operator and how spills
//! interact with incremental recalculation (ported from the v1.0 TypeScript suite).

mod engine_support;

use engine_support::*;
use kiln_formula::engine::CellInput;
use kiln_formula::refs::Bounds;
use kiln_formula::values::{ErrorValue, Value};

fn blank() -> Value {
    Value::Blank
}

#[test]
fn spills_an_array_result_into_neighbouring_cells() {
    let mut e = sheet(&[("A1", "=SEQUENCE(3,2)".into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), num(1.0));
    assert_eq!(get(&e, "B1"), num(2.0));
    assert_eq!(get(&e, "A3"), num(5.0));
    assert_eq!(get(&e, "B3"), num(6.0));
    assert_eq!(get(&e, "A4"), blank());
    let r = e.spill_range("Sheet1", 0, 0).expect("spill");
    assert_eq!(
        r.bounds,
        Bounds {
            r0: 0,
            c0: 0,
            r1: 2,
            c1: 1
        }
    );
    assert!(!r.blocked);
    assert_eq!(
        e.spill_range("Sheet1", 2, 1)
            .map(|r| (r.bounds.r0, r.bounds.c0)),
        Some((0, 0))
    );
    assert_eq!(
        e.result("Sheet1", 0, 0),
        Some(arr(vec![
            vec![num(1.0), num(2.0)],
            vec![num(3.0), num(4.0)],
            vec![num(5.0), num(6.0)]
        ]))
    );
}

#[test]
fn spills_ranges_array_constants_and_lifted_operations() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("A2", 2.into()),
        ("A3", 3.into()),
        ("C1", "=A1:A3*10".into()),
        ("D1", "={1,2,3}".into()),
    ]);
    e.recalc();
    assert_eq!(
        [get(&e, "C1"), get(&e, "C2"), get(&e, "C3")],
        [num(10.0), num(20.0), num(30.0)]
    );
    assert_eq!(
        [get(&e, "D1"), get(&e, "E1"), get(&e, "F1")],
        [num(1.0), num(2.0), num(3.0)]
    );
}

#[test]
fn shows_blanks_inside_a_spilled_range_as_zero() {
    let mut e = sheet(&[("A1", 1.into()), ("A3", 3.into()), ("C1", "=A1:A3".into())]);
    e.recalc();
    assert_eq!(get(&e, "C2"), num(0.0));
}

#[test]
fn a_1x1_array_result_does_not_spill() {
    let mut e = sheet(&[("A1", "=SEQUENCE(1)".into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), num(1.0));
    assert_eq!(e.spill_range("Sheet1", 0, 0), None);
}

#[test]
fn cells_reading_inside_a_spill_see_the_spilled_values() {
    let mut e = sheet(&[
        ("A1", "=SEQUENCE(5)".into()),
        ("C1", "=A4*100".into()),
        ("C2", "=SUM(A1:A5)".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(400.0));
    assert_eq!(get(&e, "C2"), num(15.0));
}

#[test]
fn dependents_of_spilled_cells_update_when_the_anchor_changes() {
    let mut e = sheet(&[
        ("B1", 3.into()),
        ("A1", "=SEQUENCE(B1)".into()),
        ("C1", "=SUM(A1:A10)".into()),
        ("D1", "=A3".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(6.0));
    assert_eq!(get(&e, "D1"), num(3.0));
    fill(&mut e, &[("B1", 5.into())]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(15.0));
    fill(&mut e, &[("B1", 2.into())]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(3.0));
    assert_eq!(get(&e, "D1"), num(0.0));
    assert_eq!(get(&e, "A3"), blank());
}

#[test]
fn a_dependent_evaluated_before_a_growing_spill_is_corrected_in_the_same_recalc() {
    let mut e = sheet(&[
        ("B1", 2.into()),
        ("A1", "=SEQUENCE(B1)".into()),
        ("D1", "=A4+1".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "D1"), num(1.0));
    fill(&mut e, &[("B1", 6.into())]);
    e.recalc();
    assert_eq!(get(&e, "D1"), num(5.0));
}

#[test]
fn growing_spills_are_correct_in_parallel_levels() {
    // Many independent anchors and readers in one level, so the level is evaluated in parallel
    // and readers of the new spill areas must be redone after the anchors are applied.
    let mut e = engine();
    for i in 0..400u32 {
        let col = i * 3;
        e.set_value("Sheet1", 0, col + 2, 2.0).expect("v");
        let n = kiln_formula::refs::column_to_letters(col + 2);
        e.set_formula("Sheet1", 0, col, &format!("=SEQUENCE({n}1)"))
            .expect("f");
        let a = kiln_formula::refs::column_to_letters(col);
        e.set_formula("Sheet1", 10, col, &format!("={a}4+1"))
            .expect("f");
    }
    e.recalc();
    for i in 0..400u32 {
        let col = i * 3;
        e.set_value("Sheet1", 0, col + 2, 6.0).expect("v");
    }
    e.recalc();
    for i in 0..400u32 {
        assert_eq!(
            e.value("Sheet1", 10, i * 3).ok(),
            Some(num(5.0)),
            "anchor {i}"
        );
    }
}

#[test]
fn is_spill_when_a_value_blocks_the_spill_range() {
    let mut e = sheet(&[("A1", "=SEQUENCE(3)".into()), ("A3", "x".into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), error(ErrorValue::Spill));
    assert_eq!(get(&e, "A2"), blank());
    assert_eq!(get(&e, "A3"), text("x"));
    let r = e.spill_range("Sheet1", 0, 0).expect("range");
    assert!(r.blocked);
    assert_eq!(r.bounds.r1, 2);
}

#[test]
fn recovers_when_the_blocking_value_is_cleared() {
    let mut e = sheet(&[("A1", "=SEQUENCE(3)".into()), ("A3", "x".into())]);
    e.recalc();
    e.clear_cell("Sheet1", 2, 0).expect("clear");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(1.0));
    assert_eq!(get(&e, "A3"), num(3.0));
}

#[test]
fn turns_into_spill_when_a_value_is_typed_into_the_spill_range() {
    let mut e = sheet(&[("A1", "=SEQUENCE(3)".into()), ("B1", "=SUM(A1:A3)".into())]);
    e.recalc();
    assert_eq!(get(&e, "B1"), num(6.0));
    fill(&mut e, &[("A2", "hello".into())]);
    let r = e.recalc();
    assert_eq!(get(&e, "A1"), error(ErrorValue::Spill));
    assert_eq!(get(&e, "B1"), error(ErrorValue::Spill));
    assert!(r.changed.iter().any(|c| c.row == 0 && c.col == 0));
}

#[test]
fn is_blocked_by_another_formula() {
    let mut e = sheet(&[("A1", "=SEQUENCE(3)".into()), ("A2", "=1+1".into())]);
    e.recalc();
    assert_eq!(get(&e, "A1"), error(ErrorValue::Spill));
    assert_eq!(get(&e, "A2"), num(2.0));
}

#[test]
fn is_blocked_by_another_spill_and_takes_over_when_that_spill_shrinks() {
    let mut e = sheet(&[
        ("Z1", 3.into()),
        ("A1", "=SEQUENCE(Z1,2)".into()),
        ("B3", 5.into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "A1"), error(ErrorValue::Spill));

    let mut e2 = sheet(&[
        ("Z1", 3.into()),
        ("A1", "=SEQUENCE(1,Z1)".into()),
        ("B1", 7.into()),
    ]);
    e2.recalc();
    assert_eq!(get(&e2, "A1"), error(ErrorValue::Spill));
    fill(&mut e2, &[("Z1", 1.into())]);
    e2.recalc();
    assert_eq!(get(&e2, "A1"), num(1.0));

    let mut e3 = sheet(&[
        ("Z1", 3.into()),
        ("A1", "=SEQUENCE(Z1)".into()),
        ("B2", "=SEQUENCE(1,3,10)".into()),
    ]);
    e3.recalc();
    assert_eq!(get(&e3, "B2"), num(10.0));
    // Grow A1's spill sideways into B2's spill: A1 is blocked because B2 already spills there.
    e3.set_formula("Sheet1", 0, 0, "=SEQUENCE(Z1,3)")
        .expect("f");
    e3.recalc();
    assert_eq!(get(&e3, "A1"), error(ErrorValue::Spill));
    assert_eq!(get(&e3, "C2"), num(11.0));
    // Remove B2: A1 can now spill.
    e3.clear_cell("Sheet1", 1, 1).expect("clear");
    e3.recalc();
    assert_eq!(get(&e3, "A1"), num(1.0));
    assert_eq!(get(&e3, "C3"), num(9.0));
}

#[test]
fn is_spill_when_the_result_runs_off_the_sheet() {
    let mut e = engine();
    e.set_formula("Sheet1", 1_048_575, 0, "=SEQUENCE(2)")
        .expect("f");
    e.recalc();
    assert_eq!(
        e.value("Sheet1", 1_048_575, 0).ok(),
        Some(error(ErrorValue::Spill))
    );
}

#[test]
fn legacy_formulas_never_spill() {
    let mut e = sheet(&[("A1", 5.into()), ("A2", 6.into()), ("A3", 7.into())]);
    let legacy = |t: &str| CellInput::Formula {
        text: t.into(),
        legacy: true,
    };
    e.set_cell("Sheet1", 1, 2, legacy("=A1:A3")).expect("f");
    e.set_cell("Sheet1", 0, 3, legacy("=SEQUENCE(3)"))
        .expect("f");
    e.recalc();
    assert_eq!(get(&e, "C2"), num(6.0));
    assert_eq!(get(&e, "D1"), num(1.0));
    assert_eq!(get(&e, "D2"), blank());
}

#[test]
fn the_spill_operator_refers_to_the_whole_spill_range() {
    let mut e = sheet(&[
        ("A1", "=SEQUENCE(4)".into()),
        ("C1", "=SUM(A1#)".into()),
        ("D1", "=A1#*2".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(10.0));
    assert_eq!([get(&e, "D1"), get(&e, "D4")], [num(2.0), num(8.0)]);
}

#[test]
fn the_spill_operator_follows_the_spill_as_it_resizes() {
    let mut e = sheet(&[
        ("B1", 3.into()),
        ("A1", "=SEQUENCE(B1)".into()),
        ("C1", "=SUM(A1#)".into()),
        ("D1", "=COUNT(A1#)".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(6.0));
    fill(&mut e, &[("B1", 10.into())]);
    e.recalc();
    assert_eq!(get(&e, "C1"), num(55.0));
    assert_eq!(get(&e, "D1"), num(10.0));
}

#[test]
fn the_spill_operator_works_across_sheets_and_quoted_names() {
    let mut e = engine();
    e.add_sheet("My Data").expect("sheet");
    e.set_formula("My Data", 0, 0, "=SEQUENCE(3)").expect("f");
    e.set_formula("Sheet1", 0, 0, "=SUM('My Data'!A1#)")
        .expect("f");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(6.0));
}

#[test]
fn the_spill_operator_returns_the_anchor_error_when_blocked() {
    let mut e = sheet(&[
        ("A1", "=SEQUENCE(3)".into()),
        ("A2", "x".into()),
        ("C1", "=SUM(A1#)".into()),
    ]);
    e.recalc();
    assert_eq!(get(&e, "C1"), error(ErrorValue::Spill));
}

#[test]
fn the_spill_operator_is_ref_on_a_cell_without_a_formula() {
    let mut e = sheet(&[("A1", 5.into()), ("C1", "=SUM(A1#)".into())]);
    e.recalc();
    assert_eq!(get(&e, "C1"), error(ErrorValue::Ref));
}

#[test]
fn chains_spills_a_spill_of_a_spill() {
    let mut e = sheet(&[
        ("A1", "=SEQUENCE(3)".into()),
        ("B1", "=A1#^2".into()),
        ("C1", "=SUM(B1#)".into()),
    ]);
    e.recalc();
    assert_eq!(
        [get(&e, "B1"), get(&e, "B2"), get(&e, "B3")],
        [num(1.0), num(4.0), num(9.0)]
    );
    assert_eq!(get(&e, "C1"), num(14.0));
}
