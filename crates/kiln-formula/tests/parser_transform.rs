// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reference rewriting: insert/delete rows and columns, copy, move, sheet renames and removal,
//! A1↔R1C1 and the F4 toggle (ported from the v1.0 TypeScript suite).

use kiln_formula::parser::{ParseOptions, PrintOptions, RefAddress, parse, print};
use kiln_formula::refs::{
    Bounds, MoveSpec, StructuralEdit, StructuralKind, ToggleResult, column_to_letters,
    copy_formula, cycle_absolute, format_cell, from_r1c1, move_formula, remove_sheet_in_formula,
    rename_sheet_in_formula, shift_address, shift_formula, to_r1c1, toggle_absolute,
};

/// mulberry32, as in the TypeScript tests.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }

    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() * (hi - lo + 1) as f64).floor() as i64
    }
}

fn canon(f: &str) -> String {
    match parse(f, &ParseOptions::default()) {
        Ok(n) => print(&n, &PrintOptions::formula()),
        Err(e) => format!("error {e}"),
    }
}

fn edit(kind: StructuralKind, at: u32, count: u32) -> StructuralEdit {
    StructuralEdit { kind, at, count }
}

fn ins(at: u32, count: u32) -> StructuralEdit {
    edit(StructuralKind::InsertRows, at, count)
}
fn del(at: u32, count: u32) -> StructuralEdit {
    edit(StructuralKind::DeleteRows, at, count)
}
fn ins_c(at: u32, count: u32) -> StructuralEdit {
    edit(StructuralKind::InsertCols, at, count)
}
fn del_c(at: u32, count: u32) -> StructuralEdit {
    edit(StructuralKind::DeleteCols, at, count)
}

fn shift(src: &str, e: StructuralEdit, edit_sheet: &str, formula_sheet: &str) -> String {
    shift_formula(src, &e, edit_sheet, formula_sheet).unwrap_or_else(|e| format!("error {e}"))
}

#[test]
fn insert_and_delete_rows_and_columns() {
    for (src, e, out) in [
        ("=A5", ins(4, 1), "=A6"),
        ("=A5", ins(5, 1), "=A5"),
        ("=A5", ins(0, 3), "=A8"),
        ("=$A$5", ins(2, 1), "=$A$6"),
        ("=SUM(A1:A10)", ins(4, 2), "=SUM(A1:A12)"),
        ("=SUM(A1:A10)", ins(0, 1), "=SUM(A2:A11)"),
        ("=SUM(A1:A10)", ins(10, 1), "=SUM(A1:A10)"),
        ("=SUM(A:A)", ins(3, 1), "=SUM(A:A)"),
        ("=SUM(3:5)", ins(2, 1), "=SUM(4:6)"),
        ("=SUM(3:5)", ins(3, 1), "=SUM(3:6)"),
        ("=A5", del(4, 1), "=#REF!"),
        ("=A5+1", del(0, 2), "=A3+1"),
        ("=SUM(A1:A10)", del(2, 3), "=SUM(A1:A7)"),
        ("=SUM(A3:A10)", del(0, 5), "=SUM(A1:A5)"),
        ("=SUM(A3:A5)", del(2, 3), "=SUM(#REF!)"),
        ("=SUM(A1:A3)", del(2, 5), "=SUM(A1:A2)"),
        ("=C1", ins_c(2, 1), "=D1"),
        ("=C1", ins_c(3, 1), "=C1"),
        ("=SUM(B:D)", ins_c(2, 1), "=SUM(B:E)"),
        ("=SUM(B:D)", del_c(1, 1), "=SUM(B:C)"),
        ("=C1", del_c(2, 1), "=#REF!"),
        ("=SUM(1:1)", ins_c(0, 1), "=SUM(1:1)"),
    ] {
        assert_eq!(shift(src, e, "S", "S"), out, "{src} {e:?}");
    }
}

#[test]
fn only_shifts_references_to_the_changed_sheet() {
    assert_eq!(
        shift("=A5+Other!A5+S!A5", ins(0, 1), "S", "S"),
        "=A6+Other!A5+S!A6"
    );
    assert_eq!(shift("=A5+S!A5", ins(0, 1), "S", "Other"), "=A5+S!A6");
    assert_eq!(shift("='s'!A5", ins(0, 1), "S", "X"), "=s!A6");
}

#[test]
fn keeps_the_user_formatting_outside_changed_references() {
    assert_eq!(
        shift("= SUM( A5 ,  b7 )\n+ 1", ins(0, 1), "S", "S"),
        "= SUM( A6 ,  B8 )\n+ 1"
    );
}

#[test]
fn keeps_sheet_ref_errors_qualified_on_deletion() {
    assert_eq!(shift("=S!A5", del(4, 1), "S", "X"), "=S!#REF!");
}

#[test]
fn leaves_3d_references_alone() {
    assert_eq!(shift("=SUM(S:T!A5)", ins(0, 1), "S", "S"), "=SUM(S:T!A5)");
}

#[test]
fn clamps_areas_that_reach_the_last_row() {
    assert_eq!(
        shift("=SUM(A2:A1048576)", ins(5, 1), "S", "S"),
        "=SUM(A2:A1048576)"
    );
    assert_eq!(shift("=A1048576", ins(5, 1), "S", "S"), "=#REF!");
}

#[test]
fn property_a_shifted_cell_reference_names_the_same_logical_cell() {
    let mut r = Rng(7);
    for _ in 0..300 {
        let row = r.int(0, 200) as u32;
        let col = r.int(0, 50) as u32;
        let at = r.int(0, 200) as u32;
        let count = r.int(1, 10) as u32;
        let rows_op = r.next() < 0.5;
        let insert = r.next() < 0.5;
        let kind = match (insert, rows_op) {
            (true, true) => StructuralKind::InsertRows,
            (true, false) => StructuralKind::InsertCols,
            (false, true) => StructuralKind::DeleteRows,
            (false, false) => StructuralKind::DeleteCols,
        };
        let addr = RefAddress::Cell {
            row,
            col,
            row_abs: r.next() < 0.5,
            col_abs: r.next() < 0.5,
        };
        let out = shift_address(&addr, &edit(kind, at, count));
        let pick = |a: &RefAddress| match *a {
            RefAddress::Cell { row, col, .. } => {
                if rows_op {
                    row
                } else {
                    col
                }
            }
            _ => u32::MAX,
        };
        let idx = if rows_op { row } else { col };
        if insert {
            let want = if idx >= at { idx + count } else { idx };
            assert_eq!(out.as_ref().map(pick), Some(want));
        } else if idx >= at && idx < at + count {
            assert_eq!(out, None);
        } else {
            let want = if idx >= at + count { idx - count } else { idx };
            assert_eq!(out.as_ref().map(pick), Some(want));
        }
    }
}

#[test]
fn property_insert_then_delete_restores_every_reference() {
    let mut r = Rng(11);
    for _ in 0..200 {
        let a = format!(
            "{}{}",
            column_to_letters(r.int(0, 20) as u32),
            r.int(1, 300)
        );
        let b = format!(
            "{}{}",
            column_to_letters(r.int(0, 20) as u32),
            r.int(1, 300)
        );
        let f = format!("=SUM({a}:{b})+{a}*2");
        let at = r.int(0, 300) as u32;
        let n = r.int(1, 20) as u32;
        let there = shift(&f, ins(at, n), "S", "S");
        let back = shift(&there, del(at, n), "S", "S");
        assert_eq!(canon(&back), canon(&f), "{f} via {there}");
    }
}

#[test]
fn copy_adjusts_relative_references() {
    for (src, dr, dc, out) in [
        ("=A1+B2", 1, 0, "=A2+B3"),
        ("=$A1+A$1+$A$1", 2, 3, "=$A3+D$1+$A$1"),
        ("=SUM(A1:B2)", 0, 2, "=SUM(C1:D2)"),
        ("=SUM(A:A)", 5, 1, "=SUM(B:B)"),
        ("=SUM(1:2)", 1, 5, "=SUM(2:3)"),
        ("=A1", -1, 0, "=#REF!"),
        ("=Sheet2!A1", 1, 1, "=Sheet2!B2"),
        ("=A1", 0, 0, "=A1"),
    ] {
        assert_eq!(
            copy_formula(src, dr, dc).ok().as_deref(),
            Some(out),
            "{src}"
        );
    }
}

#[test]
fn property_copying_there_and_back_is_the_identity_while_in_bounds() {
    let mut r = Rng(3);
    for _ in 0..200 {
        let cell = |r: &mut Rng, lo: i64, hi: i64| {
            let row = r.int(lo, hi) as u32;
            let col = r.int(lo, hi) as u32;
            let ra = r.next() < 0.3;
            let ca = r.next() < 0.3;
            format_cell(row, col, ra, ca)
        };
        let f = format!(
            "={}+SUM({}:{})",
            cell(&mut r, 50, 500),
            cell(&mut r, 50, 150),
            cell(&mut r, 250, 400)
        );
        let dr = r.int(-40, 40);
        let dc = r.int(-25, 25);
        let there = copy_formula(&f, dr, dc).unwrap_or_default();
        let back = copy_formula(&there, -dr, -dc).unwrap_or_default();
        assert_eq!(canon(&back), canon(&f));
    }
}

fn mv(to_sheet: Option<&str>) -> MoveSpec {
    MoveSpec {
        sheet: "S".into(),
        from: Bounds {
            r0: 0,
            c0: 0,
            r1: 1,
            c1: 1,
        },
        d_row: 5,
        d_col: 2,
        to_sheet: to_sheet.map(str::to_owned),
    }
}

#[test]
fn move_retargets_references_inside_the_moved_block() {
    let m = mv(None);
    assert_eq!(
        move_formula("=A1+$B$2", &m, "S").ok().as_deref(),
        Some("=C6+$D$7")
    );
    assert_eq!(
        move_formula("=SUM(A1:B2)", &m, "S").ok().as_deref(),
        Some("=SUM(C6:D7)")
    );
    assert_eq!(
        move_formula("=SUM(A1:A5)", &m, "S").ok().as_deref(),
        Some("=SUM(A1:A5)")
    );
    assert_eq!(move_formula("=C6", &m, "S").ok().as_deref(), Some("=#REF!"));
}

#[test]
fn move_to_another_sheet_adds_a_prefix() {
    assert_eq!(
        move_formula("=A1", &mv(Some("T")), "S").ok().as_deref(),
        Some("=T!C6")
    );
    assert_eq!(
        move_formula("=S!A1", &mv(Some("T")), "T").ok().as_deref(),
        Some("=C6")
    );
}

#[test]
fn renames_qualifiers_adding_quotes_when_needed() {
    assert_eq!(
        rename_sheet_in_formula("=Sheet1!A1+sheet1!B2+Other!C3", "Sheet1", "My Data")
            .ok()
            .as_deref(),
        Some("='My Data'!A1+'My Data'!B2+Other!C3")
    );
    assert_eq!(
        rename_sheet_in_formula("=SUM(Jan:Mar!A1)", "Mar", "Dec")
            .ok()
            .as_deref(),
        Some("=SUM(Jan:Dec!A1)")
    );
    assert_eq!(
        rename_sheet_in_formula("=Sheet1!Total", "Sheet1", "S2")
            .ok()
            .as_deref(),
        Some("='S2'!Total")
    );
}

#[test]
fn references_to_a_removed_sheet_become_ref_errors() {
    assert_eq!(
        remove_sheet_in_formula("=Gone!A1+A1", "Gone")
            .ok()
            .as_deref(),
        Some("=#REF!+A1")
    );
}

#[test]
fn converts_between_a1_and_r1c1() {
    assert_eq!(
        to_r1c1("=A1+$B$2+C$3+$D4", 2, 2).ok().as_deref(),
        Some("=R[-2]C[-2]+R2C2+R3C+R[1]C4")
    );
    assert_eq!(
        to_r1c1("=SUM(A:A)+SUM(3:3)", 4, 0).ok().as_deref(),
        Some("=SUM(C)+SUM(R[-2])")
    );
    assert_eq!(
        to_r1c1("=SUM($A:$B)", 0, 5).ok().as_deref(),
        Some("=SUM(C1:C2)")
    );
    assert_eq!(
        from_r1c1("=R[-2]C[-2]+R2C2+RC", 2, 2).ok().as_deref(),
        Some("=A1+$B$2+C3")
    );
}

#[test]
fn property_a1_to_r1c1_to_a1_is_the_identity() {
    let mut r = Rng(5);
    for _ in 0..200 {
        let cell = |r: &mut Rng| {
            let row = r.int(0, 2000) as u32;
            let col = r.int(0, 400) as u32;
            let ra = r.next() < 0.5;
            let ca = r.next() < 0.5;
            format_cell(row, col, ra, ca)
        };
        let f = format!("={}*SUM({}:{})", cell(&mut r), cell(&mut r), cell(&mut r));
        let row = r.int(0, 3000) as u32;
        let col = r.int(0, 500) as u32;
        let there = to_r1c1(&f, row, col).unwrap_or_default();
        let back = from_r1c1(&there, row, col).unwrap_or_default();
        assert_eq!(canon(&back), canon(&f), "{f} → {there}");
    }
}

#[test]
fn r1c1_formulas_parse_into_the_same_ast_as_a1() {
    let a = parse("=$A$1+B2", &ParseOptions::default()).map(|n| n.without_spans());
    let b = parse("=R1C1+R[1]C[1]", &ParseOptions::r1c1(0, 0)).map(|n| n.without_spans());
    assert_eq!(a, b);
}

fn toggle(text: &str, start: usize, end: usize) -> ToggleResult {
    toggle_absolute(text, start, end, &ParseOptions::default())
}

#[test]
fn f4_cycles_a1_through_its_four_states() {
    let mut t = "=A1".to_owned();
    let mut seen = Vec::new();
    for _ in 0..4 {
        t = toggle(&t, 2, 2).text;
        seen.push(t.clone());
    }
    assert_eq!(seen, ["=$A$1", "=A$1", "=$A1", "=A1"]);
}

#[test]
fn f4_toggles_a_whole_area_and_selects_it() {
    assert_eq!(
        toggle("=SUM(A1:B2)", 6, 6),
        ToggleResult {
            text: "=SUM($A$1:$B$2)".into(),
            selection_start: 5,
            selection_end: 14
        }
    );
}

#[test]
fn f4_toggles_every_reference_in_a_selection_and_keeps_prefixes() {
    assert_eq!(toggle("=A1+'My S'!B2", 0, 13).text, "=$A$1+'My S'!$B$2");
}

#[test]
fn f4_works_on_unfinished_formulas_and_leaves_non_references_alone() {
    assert_eq!(toggle("=SUM(A1", 7, 7).text, "=SUM($A$1");
    assert_eq!(
        toggle("=SUM(1,2)", 3, 3),
        ToggleResult {
            text: "=SUM(1,2)".into(),
            selection_start: 3,
            selection_end: 3
        }
    );
}

#[test]
fn f4_cycles_whole_rows_and_columns() {
    let next = cycle_absolute(&RefAddress::Cols {
        c0: 0,
        c1: 0,
        c0_abs: false,
        c1_abs: false,
    });
    assert!(matches!(next, RefAddress::Cols { c0_abs: true, .. }));
    assert_eq!(toggle("=A:A", 2, 2).text, "=$A:$A");
}
