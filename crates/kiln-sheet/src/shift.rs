// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pure functions that adjust ranges and sheet metadata for structural edits (insert or delete
//! rows or columns). The rules mirror Excel and the formula engine's reference shifting:
//! inserting at or before the first index moves a range, inserting strictly inside grows it,
//! deleting shrinks it, and a range whose every row (column) is deleted disappears. Ranges that
//! span the whole axis (`A:A` for a row edit) never change.

use kiln_formula::refs::{StructuralEdit, StructuralKind};

use crate::address::{CellPos, MAX_COLS, MAX_ROWS, Range};
use crate::error::ModelError;
use crate::meta::{
    Anchor, AnchorPoint, AutoFilter, DrawingAnchor, EditAs, FilterColumn, SortState, Table,
    TableColumn, TableFilter,
};

/// A structural edit.
pub fn edit(kind: StructuralKind, at: u32, count: u32) -> StructuralEdit {
    StructuralEdit { kind, at, count }
}

fn axis_max(e: &StructuralEdit) -> u32 {
    if e.kind.is_rows() { MAX_ROWS } else { MAX_COLS }
}

/// Shifts one index; `None` when it is deleted or pushed off the sheet.
pub fn shift_index(i: u32, e: &StructuralEdit) -> Option<u32> {
    if i < e.at {
        return Some(i);
    }
    if e.kind.is_insert() {
        let n = u64::from(i) + u64::from(e.count);
        return (n < u64::from(axis_max(e))).then_some(n as u32);
    }
    if u64::from(i) < u64::from(e.at) + u64::from(e.count) {
        return None;
    }
    Some(i - e.count)
}

/// Shifts an inclusive span; `None` when all of it is deleted or pushed off the sheet.
pub fn shift_span(lo: u32, hi: u32, e: &StructuralEdit) -> Option<(u32, u32)> {
    let max = u64::from(axis_max(e));
    if lo == 0 && u64::from(hi) == max - 1 {
        return Some((lo, hi));
    }
    let (lo64, hi64, at, n) = (
        u64::from(lo),
        u64::from(hi),
        u64::from(e.at),
        u64::from(e.count),
    );
    if e.kind.is_insert() {
        let nlo = if lo64 >= at { lo64 + n } else { lo64 };
        if nlo >= max {
            return None;
        }
        let nhi = if hi64 >= at {
            (hi64 + n).min(max - 1)
        } else {
            hi64
        };
        return Some((nlo as u32, nhi as u32));
    }
    let end = at + n;
    if lo64 >= at && hi64 < end {
        return None;
    }
    let nlo = if lo64 < at {
        lo64
    } else if lo64 >= end {
        lo64 - n
    } else {
        at
    };
    let nhi = if hi64 >= end {
        hi64 - n
    } else if hi64 >= at {
        at - 1
    } else {
        hi64
    };
    (nlo <= nhi).then_some((nlo as u32, nhi as u32))
}

/// Shifts a range.
pub fn shift_range(r: &Range, e: &StructuralEdit) -> Option<Range> {
    if e.kind.is_rows() {
        let (r0, r1) = shift_span(r.r0, r.r1, e)?;
        Some(Range { r0, r1, ..*r })
    } else {
        let (c0, c1) = shift_span(r.c0, r.c1, e)?;
        Some(Range { c0, c1, ..*r })
    }
}

/// Shifts a list of ranges, dropping deleted ones.
pub fn shift_ranges(list: &[Range], e: &StructuralEdit) -> Vec<Range> {
    list.iter().filter_map(|r| shift_range(r, e)).collect()
}

/// Shifts a cell; `None` when deleted.
pub fn shift_cell(p: CellPos, e: &StructuralEdit) -> Option<CellPos> {
    if e.kind.is_rows() {
        Some(CellPos {
            row: shift_index(p.row, e)?,
            col: p.col,
        })
    } else {
        Some(CellPos {
            row: p.row,
            col: shift_index(p.col, e)?,
        })
    }
}

/// Merged regions: shifted like ranges; a merge reduced to one cell is dropped.
pub fn shift_merges(list: &[Range], e: &StructuralEdit) -> Vec<Range> {
    shift_ranges(list, e)
        .into_iter()
        .filter(|r| !r.is_single_cell())
        .collect()
}

/// A count of leading frozen rows or columns.
pub fn shift_frozen_count(n: u32, e: &StructuralEdit) -> u32 {
    if n == 0 || e.at >= n {
        return n;
    }
    if e.kind.is_insert() {
        return n.saturating_add(e.count).min(axis_max(e) - 1);
    }
    let removed = n.min(e.at.saturating_add(e.count)) - e.at;
    n - removed
}

/// Page breaks (a break before index i).
pub fn shift_breaks(list: &[u32], e: &StructuralEdit) -> Vec<u32> {
    let mut out: Vec<u32> = list
        .iter()
        .filter_map(|&i| shift_index(i, e))
        .filter(|&i| i > 0)
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// Unique default column names (`Column1`, `Column2`…) not already used.
pub fn fresh_column_names(existing: &[String], count: usize) -> Vec<String> {
    let mut used: Vec<String> = existing.iter().map(|n| n.to_uppercase()).collect();
    let mut out = Vec::with_capacity(count);
    let mut k = 1;
    while out.len() < count {
        let name = format!("Column{k}");
        k += 1;
        if !used.contains(&name.to_uppercase()) {
            used.push(name.to_uppercase());
            out.push(name);
        }
    }
    out
}

/// Shifts a table. Rows: the range moves, grows or shrinks; deleting the header row without the
/// rest of the table is refused (as Excel does); deleting the totals row turns it off. Columns:
/// inserted columns get fresh names, deleted ones leave the column list. `Ok(None)` when the
/// whole table is deleted.
pub fn shift_table(t: &Table, e: &StructuralEdit) -> Result<Option<Table>, ModelError> {
    let Some(r) = shift_range(&t.range, e) else {
        return Ok(None);
    };
    if e.kind.is_rows() {
        if e.kind.is_insert() {
            return Ok(Some(Table {
                range: r,
                ..t.clone()
            }));
        }
        let end = u64::from(e.at) + u64::from(e.count);
        let inside = |row: u32| row >= e.at && u64::from(row) < end;
        if t.header_row && inside(t.range.r0) {
            return Err(ModelError::invalid(format!(
                "cannot delete the header row of table \"{}\" without deleting the table",
                t.name
            )));
        }
        let totals_deleted = t.totals_row && inside(t.range.r1);
        let mut next = Table {
            range: r,
            totals_row: t.totals_row && !totals_deleted,
            ..t.clone()
        };
        let min_rows = 1 + u32::from(next.header_row) + u32::from(next.totals_row);
        if r.height() < min_rows {
            if next.header_row {
                next.range.r1 = r.r0 + min_rows - 1;
            } else {
                return Ok(None);
            }
        }
        return Ok(Some(next));
    }
    let c0 = t.range.c0;
    let mut columns: Vec<TableColumn> = t.columns.clone();
    if e.kind.is_insert() {
        if e.at > c0 && e.at <= t.range.c1 {
            let names: Vec<String> = columns.iter().map(|c| c.name.clone()).collect();
            let fresh = fresh_column_names(&names, e.count as usize);
            let at = (e.at - c0) as usize;
            columns.splice(at..at, fresh.into_iter().map(TableColumn::named));
        }
        columns.truncate(r.width() as usize);
    } else {
        let end = u64::from(e.at) + u64::from(e.count);
        columns = columns
            .into_iter()
            .enumerate()
            .filter(|(i, _)| {
                let c = u64::from(c0) + *i as u64;
                c < u64::from(e.at) || c >= end
            })
            .map(|(_, c)| c)
            .collect();
        if columns.is_empty() {
            return Ok(None);
        }
    }
    Ok(Some(Table {
        range: r,
        columns,
        auto_filter: t.auto_filter.as_ref().map(|f| TableFilter {
            columns: shift_filter_columns(&f.columns, c0, e),
            sort: f.sort.as_ref().and_then(|s| shift_sort_state(s, e)),
        }),
        ..t.clone()
    }))
}

/// Re-indexes filter columns (`col_id` is relative to the filter's first column `c0`).
fn shift_filter_columns(cols: &[FilterColumn], c0: u32, e: &StructuralEdit) -> Vec<FilterColumn> {
    if e.kind.is_rows() {
        return cols.to_vec();
    }
    let new_c0 = shift_index(c0, e).unwrap_or(e.at);
    cols.iter()
        .filter_map(|fc| {
            let abs = c0.checked_add(fc.col_id)?;
            let n = shift_index(abs, e)?;
            Some(FilterColumn {
                col_id: n.checked_sub(new_c0)?,
                ..fc.clone()
            })
        })
        .collect()
}

fn shift_sort_state(s: &SortState, e: &StructuralEdit) -> Option<SortState> {
    let range = shift_range(&s.range, e)?;
    let conditions = s
        .conditions
        .iter()
        .filter_map(|c| {
            Some(crate::meta::SortCondition {
                range: shift_range(&c.range, e)?,
                ..c.clone()
            })
        })
        .collect();
    Some(SortState {
        range,
        conditions,
        ..s.clone()
    })
}

/// Shifts a sheet's AutoFilter; `None` when its range is deleted.
pub fn shift_auto_filter(f: &AutoFilter, e: &StructuralEdit) -> Option<AutoFilter> {
    let range = shift_range(&f.range, e)?;
    Some(AutoFilter {
        range,
        columns: shift_filter_columns(&f.columns, f.range.c0, e),
        sort: f.sort.as_ref().and_then(|s| shift_sort_state(s, e)),
    })
}

fn shift_anchor_point(p: &AnchorPoint, e: &StructuralEdit) -> AnchorPoint {
    let rows = e.kind.is_rows();
    let i = if rows { p.row } else { p.col };
    let mut off = if rows { p.row_off } else { p.col_off };
    let n = if i < e.at {
        i
    } else if e.kind.is_insert() {
        i.saturating_add(e.count).min(axis_max(e) - 1)
    } else if u64::from(i) < u64::from(e.at) + u64::from(e.count) {
        off = 0;
        e.at
    } else {
        i - e.count
    };
    if rows {
        AnchorPoint {
            row: n,
            row_off: off,
            ..*p
        }
    } else {
        AnchorPoint {
            col: n,
            col_off: off,
            ..*p
        }
    }
}

/// Drawings move (and, for "move and size with cells", resize) with the cells under them;
/// absolute anchors never move.
pub fn shift_drawing(d: &DrawingAnchor, e: &StructuralEdit) -> DrawingAnchor {
    let anchor = match d.anchor {
        Anchor::Absolute { .. }
        | Anchor::TwoCell {
            edit_as: EditAs::Absolute,
            ..
        } => d.anchor,
        Anchor::OneCell { from, ext } => Anchor::OneCell {
            from: shift_anchor_point(&from, e),
            ext,
        },
        Anchor::TwoCell { from, to, edit_as } => {
            let nf = shift_anchor_point(&from, e);
            let nt = if edit_as == EditAs::OneCell {
                // Move without sizing: keep the extent in cells.
                if e.kind.is_rows() {
                    let d0 = i64::from(nf.row) - i64::from(from.row);
                    AnchorPoint {
                        row: (i64::from(to.row) + d0).clamp(0, i64::from(MAX_ROWS - 1)) as u32,
                        ..to
                    }
                } else {
                    let d0 = i64::from(nf.col) - i64::from(from.col);
                    AnchorPoint {
                        col: (i64::from(to.col) + d0).clamp(0, i64::from(MAX_COLS - 1)) as u32,
                        ..to
                    }
                }
            } else {
                shift_anchor_point(&to, e)
            };
            Anchor::TwoCell {
                from: nf,
                to: nt,
                edit_as,
            }
        }
    };
    DrawingAnchor {
        anchor,
        ..d.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::DrawingKind;
    use StructuralKind::*;
    use kiln_formula::parser::RefAddress;
    use kiln_formula::refs::{address_bounds, shift_address};

    fn r(a: &str) -> Range {
        Range::parse_a1(a).unwrap_or_default()
    }

    fn sr(a: &str, kind: StructuralKind, at: u32, count: u32) -> Option<String> {
        shift_range(&r(a), &edit(kind, at, count)).map(|x| x.to_a1(false))
    }

    #[test]
    fn range_shifting_matrix() {
        // Insert rows before, at, inside and after.
        assert_eq!(sr("B3:C5", InsertRows, 0, 2).as_deref(), Some("B5:C7"));
        assert_eq!(sr("B3:C5", InsertRows, 2, 1).as_deref(), Some("B4:C6"));
        assert_eq!(sr("B3:C5", InsertRows, 3, 1).as_deref(), Some("B3:C6"));
        assert_eq!(sr("B3:C5", InsertRows, 5, 1).as_deref(), Some("B3:C5"));
        // Delete rows overlapping either end, inside and all.
        assert_eq!(sr("B3:C5", DeleteRows, 1, 2).as_deref(), Some("B2:C3"));
        assert_eq!(sr("B3:C5", DeleteRows, 3, 1).as_deref(), Some("B3:C4"));
        assert_eq!(sr("B3:C5", DeleteRows, 4, 5).as_deref(), Some("B3:C4"));
        assert_eq!(sr("B3:C5", DeleteRows, 2, 3), None);
        // Columns.
        assert_eq!(sr("B3:C5", InsertCols, 1, 1).as_deref(), Some("C3:D5"));
        assert_eq!(sr("B3:C5", DeleteCols, 2, 1).as_deref(), Some("B3:B5"));
        // Whole-axis ranges never change.
        assert_eq!(sr("A:A", DeleteRows, 0, 5).as_deref(), Some("A:A"));
        assert_eq!(sr("2:2", InsertCols, 0, 5).as_deref(), Some("2:2"));
        // Pushed off the sheet.
        assert_eq!(
            shift_range(&Range::cell(MAX_ROWS - 1, 0), &edit(InsertRows, 0, 1)),
            None
        );
    }

    #[test]
    fn agrees_with_the_formula_engine() {
        let mut x: u64 = 88_172_645_463_325_252;
        let mut next = |m: u32| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x % u64::from(m)) as u32
        };
        for _ in 0..5000 {
            let (a, b, c, d) = (next(40), next(40), next(40), next(40));
            let area = Range::new(a, b, c, d);
            let kinds = [InsertRows, DeleteRows, InsertCols, DeleteCols];
            let e = edit(kinds[next(4) as usize], next(45), next(6) + 1);
            let addr = RefAddress::area(
                area.r0, area.c0, false, false, area.r1, area.c1, false, false,
            );
            let engine = shift_address(&addr, &e).map(|a| Range::from_bounds(address_bounds(&a)));
            assert_eq!(shift_range(&area, &e), engine, "{area} {e:?}");
        }
    }

    #[test]
    fn cells_merges_frozen_panes_and_breaks() {
        assert_eq!(
            shift_cell(CellPos::new(4, 1), &edit(DeleteRows, 4, 1)),
            None
        );
        assert_eq!(
            shift_cell(CellPos::new(4, 1), &edit(InsertCols, 0, 2)),
            Some(CellPos::new(4, 3))
        );
        assert_eq!(
            shift_merges(&[r("A1:B2"), r("D1:D2")], &edit(DeleteRows, 1, 1)),
            vec![r("A1:B1")]
        );
        assert_eq!(shift_frozen_count(3, &edit(InsertRows, 1, 2)), 5);
        assert_eq!(shift_frozen_count(3, &edit(DeleteRows, 1, 5)), 1);
        assert_eq!(shift_frozen_count(3, &edit(DeleteRows, 3, 5)), 3);
        assert_eq!(
            shift_breaks(&[1, 5, 9], &edit(DeleteRows, 0, 2)),
            vec![3, 7]
        );
        assert_eq!(shift_breaks(&[3, 5], &edit(DeleteRows, 3, 2)), vec![3]);
    }

    fn table() -> Table {
        Table {
            id: 1,
            name: "Sales".into(),
            display_name: None,
            range: r("B2:D6"),
            header_row: true,
            totals_row: true,
            columns: ["Region", "Qty", "Price"]
                .into_iter()
                .map(TableColumn::named)
                .collect(),
            style: None,
            auto_filter: Some(TableFilter {
                columns: vec![FilterColumn {
                    col_id: 2,
                    hidden_button: false,
                    show_button: true,
                    filter: None,
                }],
                sort: None,
            }),
            comment: None,
        }
    }

    #[test]
    fn tables_grow_move_and_keep_their_header() {
        let t = table();
        let grown = shift_table(&t, &edit(InsertRows, 3, 2)).ok().flatten();
        assert_eq!(grown.map(|t| t.range), Some(r("B2:D8")));
        let moved = shift_table(&t, &edit(InsertRows, 0, 1)).ok().flatten();
        assert_eq!(moved.map(|t| t.range), Some(r("B3:D7")));
        assert!(shift_table(&t, &edit(DeleteRows, 1, 1)).is_err());
        let no_totals = shift_table(&t, &edit(DeleteRows, 5, 1)).ok().flatten();
        assert_eq!(no_totals.as_ref().map(|t| t.totals_row), Some(false));
        assert_eq!(
            shift_table(&t, &edit(DeleteRows, 0, 10)).ok().flatten(),
            None
        );
        // Deleting every data row keeps one (Excel never leaves a table without data rows).
        let thin = shift_table(&t, &edit(DeleteRows, 2, 3)).ok().flatten();
        assert_eq!(thin.map(|t| t.range), Some(r("B2:D4")));
    }

    #[test]
    fn tables_name_inserted_columns_and_drop_deleted_ones() {
        let t = table();
        let wide = shift_table(&t, &edit(InsertCols, 2, 2)).ok().flatten();
        let wide = wide.unwrap_or_else(table);
        let names: Vec<&str> = wide.columns.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["Region", "Column1", "Column2", "Qty", "Price"]);
        assert_eq!(
            wide.auto_filter.map(|f| f.columns[0].col_id),
            Some(4),
            "filter columns are re-indexed"
        );
        let narrow = shift_table(&t, &edit(DeleteCols, 2, 1)).ok().flatten();
        let narrow = narrow.unwrap_or_else(table);
        assert_eq!(narrow.columns.len(), 2);
        assert_eq!(narrow.range, r("B2:C6"));
        assert_eq!(narrow.auto_filter.map(|f| f.columns[0].col_id), Some(1));
        assert_eq!(
            fresh_column_names(&["Column1".into()], 2),
            vec!["Column2", "Column3"]
        );
    }

    fn drawing(edit_as: EditAs) -> DrawingAnchor {
        DrawingAnchor {
            id: "1".into(),
            kind: DrawingKind::Chart,
            reference: "chart1".into(),
            anchor: Anchor::TwoCell {
                from: AnchorPoint {
                    row: 2,
                    col: 1,
                    row_off: 100,
                    col_off: 0,
                },
                to: AnchorPoint {
                    row: 6,
                    col: 4,
                    row_off: 0,
                    col_off: 0,
                },
                edit_as,
            },
            name: None,
            alt_text: None,
            hidden: false,
            locked: true,
        }
    }

    fn rows_of(d: &DrawingAnchor) -> (u32, u32) {
        match d.anchor {
            Anchor::TwoCell { from, to, .. } => (from.row, to.row),
            _ => (0, 0),
        }
    }

    #[test]
    fn drawings_move_and_size_with_cells() {
        let d = drawing(EditAs::TwoCell);
        assert_eq!(rows_of(&shift_drawing(&d, &edit(InsertRows, 4, 2))), (2, 8));
        assert_eq!(rows_of(&shift_drawing(&d, &edit(InsertRows, 0, 1))), (3, 7));
        let one = drawing(EditAs::OneCell);
        assert_eq!(
            rows_of(&shift_drawing(&one, &edit(InsertRows, 4, 2))),
            (2, 6)
        );
        assert_eq!(
            rows_of(&shift_drawing(&one, &edit(InsertRows, 0, 2))),
            (4, 8)
        );
        let abs = drawing(EditAs::Absolute);
        assert_eq!(shift_drawing(&abs, &edit(InsertRows, 0, 2)), abs);
        // Deleting the rows under the top edge collapses it onto the deletion point.
        let c = shift_drawing(&d, &edit(DeleteRows, 1, 3));
        match c.anchor {
            Anchor::TwoCell { from, .. } => {
                assert_eq!((from.row, from.row_off), (1, 0));
            }
            _ => unreachable!("still a two-cell anchor"),
        }
    }
}
