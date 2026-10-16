// SPDX-License-Identifier: AGPL-3.0-or-later
//! Structured-reference maintenance: renaming tables and table columns inside formula text, and
//! converting a table's structured references to plain A1 references (Table Design › Convert to
//! Range). Edits are spliced into the original text, so the user's spacing is preserved.

use kiln_formula::parser::{
    Node, NodeKind, ParseOptions, RefAddress, SheetPrefix, StructuredRef, StructuredSpecial,
    format_structured, parse,
};
use kiln_formula::refs::{format_address, format_prefix};

use crate::address::{CellPos, Range};
use crate::meta::Table;

fn splice(text: &str, mut edits: Vec<(usize, usize, String)>) -> Option<String> {
    if edits.is_empty() {
        return None;
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_owned();
    for (s, e, t) in edits {
        if s > e || e > out.len() || !out.is_char_boundary(s) || !out.is_char_boundary(e) {
            return None;
        }
        out.replace_range(s..e, &t);
    }
    (out != text).then_some(out)
}

fn rewrite(text: &str, f: &mut dyn FnMut(&Node) -> Option<String>) -> Option<String> {
    let ast = parse(text, &ParseOptions::default()).ok()?;
    let mut edits = Vec::new();
    ast.walk(&mut |n: &Node| {
        if let Some(t) = f(n) {
            edits.push((n.span.start as usize, n.span.end as usize, t));
            return false;
        }
        true
    });
    splice(text, edits)
}

fn same(a: Option<&str>, b: &str) -> bool {
    a.is_some_and(|a| a.eq_ignore_ascii_case(b))
}

/// Renames a table in structured references and bare table names. `None` when nothing changes.
pub fn rename_table_in_formula(text: &str, old: &str, new: &str) -> Option<String> {
    rewrite(text, &mut |n| match &n.kind {
        NodeKind::Structured(s) if same(s.table.as_deref(), old) => {
            Some(format_structured(&StructuredRef {
                table: Some(new.to_owned()),
                ..(**s).clone()
            }))
        }
        NodeKind::Name { name, prefix: None } if name.eq_ignore_ascii_case(old) => {
            Some(new.to_owned())
        }
        _ => None,
    })
}

/// Renames a column of `table` in structured references. Unqualified references (`[@Qty]`) are
/// rewritten only for formulas inside that table (`inside_table`).
pub fn rename_table_column_in_formula(
    text: &str,
    table: &str,
    old: &str,
    new: &str,
    inside_table: bool,
) -> Option<String> {
    rewrite(text, &mut |n| {
        let NodeKind::Structured(s) = &n.kind else {
            return None;
        };
        let mine = match &s.table {
            None => inside_table,
            Some(t) => t.eq_ignore_ascii_case(table),
        };
        let (a, b) = s.columns.as_ref().filter(|_| mine)?;
        let swap = |c: &String| {
            if c.eq_ignore_ascii_case(old) {
                new.to_owned()
            } else {
                c.clone()
            }
        };
        let cols = (swap(a), b.as_ref().map(swap));
        if cols.0 == *a && cols.1.as_ref() == b.as_ref() {
            return None;
        }
        Some(format_structured(&StructuredRef {
            columns: Some(cols),
            ..(**s).clone()
        }))
    })
}

/// The A1 area a structured reference denotes (`None` = `#REF!`). `at_row` is the formula's row
/// (for `[@Col]`).
pub fn structured_area(t: &Table, n: &StructuredRef, at_row: Option<u32>) -> Option<Range> {
    let (mut c0, mut c1) = (t.range.c0, t.range.c1);
    if let Some((a, b)) = &n.columns {
        let ia = t.column_index(a)? as u32;
        let ib = match b {
            Some(b) => t.column_index(b)? as u32,
            None => ia,
        };
        c0 = t.range.c0 + ia.min(ib);
        c1 = t.range.c0 + ia.max(ib);
    }
    let data_r0 = t.range.r0 + u32::from(t.header_row);
    let data_r1 = t.range.r1.checked_sub(u32::from(t.totals_row))?;
    if n.this_row {
        let r = at_row.filter(|&r| r >= data_r0 && r <= data_r1)?;
        return Some(Range::new(r, c0, r, c1));
    }
    let specials: &[StructuredSpecial] = if n.specials.is_empty() {
        &[StructuredSpecial::Data]
    } else {
        &n.specials
    };
    let mut rows: Option<(u32, u32)> = None;
    for s in specials {
        let (a, b) = match s {
            StructuredSpecial::All => (t.range.r0, t.range.r1),
            StructuredSpecial::Data => (data_r0, data_r1),
            StructuredSpecial::Headers if t.header_row => (t.range.r0, t.range.r0),
            StructuredSpecial::Totals if t.totals_row => (t.range.r1, t.range.r1),
            _ => return None,
        };
        rows = Some(rows.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
    }
    let (r0, r1) = rows?;
    (r0 <= r1).then(|| Range::new(r0, c0, r1, c1))
}

/// Replaces structured references to `table` (on `table_sheet`) with absolute A1 references, as
/// Excel does when a table is converted to a range. `at` is the formula's own cell.
pub fn structured_to_a1(
    text: &str,
    table: &Table,
    table_sheet: &str,
    formula_sheet: &str,
    at: Option<CellPos>,
) -> Option<String> {
    let inside = at.is_some_and(|p| {
        formula_sheet.eq_ignore_ascii_case(table_sheet) && table.range.contains(p.row, p.col)
    });
    rewrite(text, &mut |n| {
        let s = match &n.kind {
            NodeKind::Structured(s) => s,
            NodeKind::Name { name, prefix: None } if name.eq_ignore_ascii_case(&table.name) => {
                return Some(a1_for(
                    table.data_range(),
                    false,
                    table_sheet,
                    formula_sheet,
                ));
            }
            _ => return None,
        };
        let mine = match &s.table {
            None => inside,
            Some(t) => t.eq_ignore_ascii_case(&table.name),
        };
        if !mine {
            return None;
        }
        let area = structured_area(table, s, at.map(|p| p.row));
        Some(a1_for(area, s.this_row, table_sheet, formula_sheet))
    })
}

fn a1_for(area: Option<Range>, this_row: bool, table_sheet: &str, formula_sheet: &str) -> String {
    let Some(a) = area else {
        return "#REF!".to_owned();
    };
    let addr = if a.is_single_cell() {
        RefAddress::Cell {
            row: a.r0,
            col: a.c0,
            row_abs: !this_row,
            col_abs: true,
        }
    } else {
        RefAddress::Area {
            r0: a.r0,
            c0: a.c0,
            r1: a.r1,
            c1: a.c1,
            r0_abs: !this_row,
            c0_abs: true,
            r1_abs: !this_row,
            c1_abs: true,
        }
    };
    let prefix = if formula_sheet.eq_ignore_ascii_case(table_sheet) {
        String::new()
    } else {
        format_prefix(&SheetPrefix::sheet(table_sheet))
    };
    format!("{prefix}{}", format_address(&addr))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::TableColumn;

    fn table() -> Table {
        Table {
            id: 1,
            name: "Sales".into(),
            display_name: None,
            range: Range::new(1, 1, 4, 3),
            header_row: true,
            totals_row: false,
            columns: ["Region", "Qty", "Price"]
                .into_iter()
                .map(TableColumn::named)
                .collect(),
            style: None,
            auto_filter: None,
            comment: None,
        }
    }

    #[test]
    fn renames_tables_and_columns_preserving_spacing() {
        assert_eq!(
            rename_table_in_formula("=SUM( Sales[Qty] ) + ROWS(Sales)", "sales", "Orders")
                .as_deref(),
            Some("=SUM( Orders[Qty] ) + ROWS(Orders)")
        );
        assert_eq!(
            rename_table_in_formula("=SUM(Other[Qty])", "Sales", "X"),
            None
        );
        assert_eq!(
            rename_table_column_in_formula(
                "=[@Qty]*Sales[[#Totals],[Qty]]",
                "Sales",
                "qty",
                "Units",
                true
            )
            .as_deref(),
            Some("=[@Units]*Sales[[#Totals],[Units]]")
        );
        assert_eq!(
            rename_table_column_in_formula("=[@Qty]", "Sales", "Qty", "Units", false),
            None,
            "unqualified references outside the table belong to another table"
        );
    }

    #[test]
    fn converts_structured_references_to_a1() {
        let t = table();
        assert_eq!(
            structured_to_a1("=SUM(Sales[Qty])", &t, "Data", "Data", None).as_deref(),
            Some("=SUM($C$3:$C$5)")
        );
        assert_eq!(
            structured_to_a1(
                "=Sales[[#Headers],[Region]:[Price]]",
                &t,
                "Data",
                "Other",
                None
            )
            .as_deref(),
            Some("=Data!$B$2:$D$2")
        );
        assert_eq!(
            structured_to_a1("=[@Qty]*2", &t, "Data", "Data", Some(CellPos::new(3, 3))).as_deref(),
            Some("=$C4*2")
        );
        assert_eq!(
            structured_to_a1("=Sales[#Totals]", &t, "Data", "Data", None).as_deref(),
            Some("=#REF!")
        );
        assert_eq!(
            structured_to_a1("=ROWS(Sales)", &t, "Data", "Data", None).as_deref(),
            Some("=ROWS($B$3:$D$5)")
        );
    }

    #[test]
    fn computes_structured_areas() {
        let t = table();
        let s = StructuredRef {
            table: Some("Sales".into()),
            specials: vec![StructuredSpecial::All],
            this_row: false,
            columns: Some(("Qty".into(), None)),
        };
        assert_eq!(structured_area(&t, &s, None), Some(Range::new(1, 2, 4, 2)));
        let missing = StructuredRef {
            columns: Some(("Nope".into(), None)),
            ..s
        };
        assert_eq!(structured_area(&t, &missing, None), None);
    }
}
