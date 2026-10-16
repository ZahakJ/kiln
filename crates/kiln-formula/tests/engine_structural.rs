// SPDX-License-Identifier: AGPL-3.0-or-later
//! Inserting and deleting rows and columns, renaming, removing and moving sheets, and moving
//! ranges (ported from the v1.0 TypeScript suite).

mod engine_support;

use engine_support::*;
use kiln_formula::refs::{Bounds, column_to_letters};
use kiln_formula::values::{ErrorValue, Value};

fn formula_at<'a>(e: &'a kiln_formula::Engine, a1: &str, sheet: &str) -> Option<&'a str> {
    let (row, col) = addr(a1);
    e.formula(sheet, row, col)
}

fn f<'a>(e: &'a kiln_formula::Engine, a1: &str) -> Option<&'a str> {
    formula_at(e, a1, "Sheet1")
}

#[test]
fn moves_cells_and_adjusts_references_when_inserting_rows() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("A2", 2.into()),
        ("A3", "=A1+A2".into()),
        ("B1", "=SUM(A1:A2)".into()),
    ]);
    e.recalc();
    e.insert_rows("Sheet1", 1, 2).expect("insert");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(1.0));
    assert_eq!(get(&e, "A4"), num(2.0));
    assert_eq!(f(&e, "A5"), Some("=A1+A4"));
    assert_eq!(get(&e, "A5"), num(3.0));
    assert_eq!(f(&e, "B1"), Some("=SUM(A1:A4)"));
    assert_eq!(get(&e, "B1"), num(3.0));
    assert_eq!(f(&e, "A3"), None);
}

#[test]
fn turns_references_to_deleted_rows_into_ref_errors() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("A2", 2.into()),
        ("A3", 3.into()),
        ("B1", "=A2*10".into()),
        ("B4", "=SUM(A1:A3)".into()),
    ]);
    e.recalc();
    e.delete_rows("Sheet1", 1, 1).expect("delete");
    e.recalc();
    assert_eq!(f(&e, "B1"), Some("=#REF!*10"));
    assert_eq!(get(&e, "B1"), error(ErrorValue::Ref));
    assert_eq!(f(&e, "B3"), Some("=SUM(A1:A2)"));
    assert_eq!(get(&e, "B3"), num(4.0));
}

#[test]
fn removes_formulas_in_deleted_rows_and_updates_their_dependents() {
    let mut e = sheet(&[
        ("A2", "=5".into()),
        ("B1", "=A2+1".into()),
        ("A5", "=A2*2".into()),
    ]);
    e.recalc();
    e.delete_rows("Sheet1", 1, 1).expect("delete");
    e.recalc();
    assert_eq!(f(&e, "B1"), Some("=#REF!+1"));
    assert_eq!(f(&e, "A4"), Some("=#REF!*2"));
}

#[test]
fn inserts_and_deletes_columns() {
    let mut e = sheet(&[
        ("A1", 2.into()),
        ("B1", 3.into()),
        ("C1", "=A1*B1".into()),
        ("D1", "=SUM(A:B)".into()),
    ]);
    e.recalc();
    e.insert_cols("Sheet1", 1, 1).expect("insert");
    e.recalc();
    assert_eq!(f(&e, "D1"), Some("=A1*C1"));
    assert_eq!(f(&e, "E1"), Some("=SUM(A:C)"));
    assert_eq!(get(&e, "D1"), num(6.0));
    e.delete_cols("Sheet1", 0, 1).expect("delete");
    e.recalc();
    assert_eq!(f(&e, "C1"), Some("=#REF!*B1"));
    assert_eq!(f(&e, "D1"), Some("=SUM(A:B)"));
    assert_eq!(get(&e, "D1"), num(3.0));
}

#[test]
fn adjusts_references_from_other_sheets_and_in_defined_names() {
    let mut e = sheet(&[("A5", 7.into())]);
    e.add_sheet("Report").expect("sheet");
    e.set_formula("Report", 0, 0, "=Sheet1!A5*2").expect("f");
    e.define_name("Seven", "=Sheet1!$A$5", None).expect("name");
    e.set_formula("Report", 1, 0, "=Seven").expect("f");
    e.recalc();
    e.insert_rows("Sheet1", 0, 3).expect("insert");
    e.recalc();
    assert_eq!(e.formula("Report", 0, 0), Some("=Sheet1!A8*2"));
    assert_eq!(e.value("Report", 0, 0).ok(), Some(num(14.0)));
    assert_eq!(e.names()[0].formula, "=Sheet1!$A$8");
    assert_eq!(e.value("Report", 1, 0).ok(), Some(num(7.0)));
}

#[test]
fn keeps_spills_working_after_a_structural_change() {
    let mut e = sheet(&[("A1", "=SEQUENCE(3)".into()), ("C1", "=SUM(A1#)".into())]);
    e.recalc();
    e.insert_rows("Sheet1", 0, 1).expect("insert");
    e.recalc();
    assert_eq!(get(&e, "A2"), num(1.0));
    assert_eq!(get(&e, "A4"), num(3.0));
    assert_eq!(get(&e, "C2"), num(6.0));
}

#[test]
fn property_inserting_rows_or_columns_never_changes_computed_values() {
    let mut r = Rng(99);
    for _ in 0..8 {
        let mut e = engine();
        let mut cells = Vec::new();
        for c in 0..5 {
            for row in 1..=10 {
                cells.push(format!("{}{row}", column_to_letters(c)));
            }
        }
        for c in &cells {
            let (row, col) = addr(c);
            if r.next() < 0.6 {
                let v = r.int(-50, 50) as f64;
                e.set_value("Sheet1", row, col, v).expect("v");
            } else {
                let a = &cells[r.int(0, cells.len() as i64 - 1) as usize];
                let b = &cells[r.int(0, cells.len() as i64 - 1) as usize];
                let text = if r.next() < 0.5 {
                    format!("=IFERROR({a}*2+{b},0)")
                } else {
                    format!("=SUM({a}:{b})")
                };
                e.set_formula("Sheet1", row, col, &text).expect("f");
            }
        }
        e.recalc();
        let before: Vec<Value> = cells.iter().map(|c| get(&e, c)).collect();
        let rows_op = r.next() < 0.5;
        let at = r.int(0, if rows_op { 10 } else { 5 }) as u32;
        let n = r.int(1, 3) as u32;
        if rows_op {
            e.insert_rows("Sheet1", at, n).expect("insert");
        } else {
            e.insert_cols("Sheet1", at, n).expect("insert");
        }
        e.recalc();
        for (c, want) in cells.iter().zip(&before) {
            let (row, col) = addr(c);
            let nr = if rows_op && row >= at { row + n } else { row };
            let nc = if !rows_op && col >= at { col + n } else { col };
            assert_eq!(
                e.value("Sheet1", nr, nc).ok().as_ref(),
                Some(want),
                "{c} after insert at {at}"
            );
        }
    }
}

#[test]
fn renames_sheets_inside_formulas_quoting_when_needed() {
    let mut e = sheet(&[("A1", 5.into())]);
    e.add_sheet("Other").expect("sheet");
    e.set_formula("Other", 0, 0, "=Sheet1!A1+1").expect("f");
    e.recalc();
    e.rename_sheet("Sheet1", "My Data").expect("rename");
    e.recalc();
    assert_eq!(e.formula("Other", 0, 0), Some("='My Data'!A1+1"));
    assert_eq!(e.value("Other", 0, 0).ok(), Some(num(6.0)));
    assert!(e.sheet_id("my data").is_some());
    assert!(e.rename_sheet("Other", "MY DATA").is_err());
}

#[test]
fn a_new_sheet_with_a_renamed_sheets_old_name_is_not_confused_with_it() {
    let mut e = sheet(&[("A1", 5.into())]);
    e.add_sheet("Other").expect("sheet");
    e.set_formula("Other", 0, 0, "=Sheet1!A1+1").expect("f");
    e.recalc();
    e.rename_sheet("Sheet1", "Data").expect("rename");
    e.add_sheet("Sheet1").expect("sheet");
    e.set_value("Sheet1", 0, 0, 100.0).expect("v");
    e.set_formula("Other", 1, 0, "=Sheet1!A1+1").expect("f");
    e.recalc();
    assert_eq!(e.value("Other", 0, 0).ok(), Some(num(6.0)));
    assert_eq!(e.value("Other", 1, 0).ok(), Some(num(101.0)));
}

#[test]
fn removing_a_sheet_turns_references_into_ref_errors() {
    let mut e = engine();
    e.add_sheet("Temp").expect("sheet");
    e.set_value("Temp", 0, 0, 3.0).expect("v");
    e.set_formula("Sheet1", 0, 0, "=Temp!A1*2").expect("f");
    e.recalc();
    assert_eq!(get(&e, "A1"), num(6.0));
    e.remove_sheet("Temp").expect("remove");
    e.recalc();
    assert_eq!(e.formula("Sheet1", 0, 0), Some("=#REF!*2"));
    assert_eq!(get(&e, "A1"), error(ErrorValue::Ref));
}

#[test]
fn move_range_cuts_and_pastes_retargeting_references() {
    let mut e = sheet(&[
        ("A1", 1.into()),
        ("A2", 2.into()),
        ("B1", "=A1+A2".into()),
        ("D1", "=B1*10".into()),
    ]);
    e.recalc();
    let block = Bounds {
        r0: 0,
        c0: 0,
        r1: 1,
        c1: 1,
    };
    e.move_range("Sheet1", block, None, 4, 2).expect("move");
    e.recalc();
    assert_eq!(get(&e, "C5"), num(1.0));
    assert_eq!(get(&e, "C6"), num(2.0));
    assert_eq!(f(&e, "D5"), Some("=C5+C6"));
    assert_eq!(f(&e, "D1"), Some("=D5*10"));
    assert_eq!(get(&e, "D1"), num(30.0));
    assert_eq!(get(&e, "A1"), Value::Blank);
}

#[test]
fn move_range_to_another_sheet() {
    let mut e = sheet(&[("A1", 4.into()), ("B1", "=A1*2".into())]);
    let dest = e.add_sheet("Dest").expect("sheet");
    e.recalc();
    let cell = Bounds {
        r0: 0,
        c0: 0,
        r1: 0,
        c1: 0,
    };
    e.move_range("Sheet1", cell, Some(dest), 0, 0)
        .expect("move");
    e.recalc();
    assert_eq!(f(&e, "B1"), Some("=Dest!A1*2"));
    assert_eq!(get(&e, "B1"), num(8.0));
}
