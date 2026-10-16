// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reference rewriting: structural edits (insert/delete rows and columns), copy with relative
//! adjustment, cut/move, sheet renames and removal, A1↔R1C1 conversion and the F4
//! absolute/relative cycle.
//!
//! Rewrites splice new reference text into the original formula, so the user's spacing, line
//! breaks and capitalisation outside the changed references are preserved.

use super::address::{Bounds, MAX_COLS, MAX_ROWS, address_bounds, format_address};
use crate::parser::{
    Node, NodeKind, ParseError, ParseOptions, PrintOptions, RefAddress, RefStyle, SheetPrefix,
    TokenKind, format_address_r1c1, leading_equals_len, parse, print, tokenize,
};
use crate::values::ErrorValue;

/// Which rows or columns change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum StructuralKind {
    /// Rows are inserted.
    InsertRows,
    /// Rows are deleted.
    DeleteRows,
    /// Columns are inserted.
    InsertCols,
    /// Columns are deleted.
    DeleteCols,
}

impl StructuralKind {
    /// True for row edits.
    pub fn is_rows(self) -> bool {
        matches!(
            self,
            StructuralKind::InsertRows | StructuralKind::DeleteRows
        )
    }

    /// True for insertions.
    pub fn is_insert(self) -> bool {
        matches!(
            self,
            StructuralKind::InsertRows | StructuralKind::InsertCols
        )
    }
}

/// An insert or delete of rows or columns on one sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructuralEdit {
    /// What changes.
    pub kind: StructuralKind,
    /// 0-based index of the first inserted or deleted row or column.
    pub at: u32,
    /// How many.
    pub count: u32,
}

/// Rewrites every reference, error and name node `mapper` returns a replacement for, splicing the
/// printed replacements into the source text. Returns the original text when nothing changed.
pub fn rewrite_formula(
    text: &str,
    parse_opts: &ParseOptions,
    print_opts: &PrintOptions,
    mut mapper: impl FnMut(&Node) -> Option<Node>,
) -> Result<String, ParseError> {
    let ast = parse(text, parse_opts)?;
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    ast.walk(&mut |n| {
        if matches!(
            n.kind,
            NodeKind::Ref { .. } | NodeKind::Error { .. } | NodeKind::Name { .. }
        ) && let Some(r) = mapper(n)
        {
            edits.push((
                n.span.start as usize,
                n.span.end as usize,
                print(&r, print_opts),
            ));
        }
        true
    });
    if edits.is_empty() {
        return Ok(text.to_owned());
    }
    edits.sort_by_key(|e| std::cmp::Reverse(e.0));
    let mut out = text.to_owned();
    for (start, end, rep) in edits {
        out.replace_range(start..end, &rep);
    }
    Ok(out)
}

fn same_sheet(a: &str, b: &str) -> bool {
    a.to_uppercase() == b.to_uppercase()
}

/// The sheet a reference points to (its prefix or the formula's own sheet); `None` for 3-D and
/// external references.
fn target_sheet<'a>(prefix: Option<&'a SheetPrefix>, formula_sheet: &'a str) -> Option<&'a str> {
    match prefix {
        None => Some(formula_sheet),
        Some(p) if p.workbook.is_some() || p.sheet2.is_some() => None,
        Some(p) => Some(&p.sheet),
    }
}

fn ref_error(node: &Node) -> Node {
    let prefix = match &node.kind {
        NodeKind::Ref { prefix, .. } => prefix.clone(),
        _ => None,
    };
    Node::new(
        NodeKind::Error {
            code: ErrorValue::Ref,
            prefix,
        },
        node.span,
    )
}

fn with_address(node: &Node, address: RefAddress) -> Node {
    let prefix = match &node.kind {
        NodeKind::Ref { prefix, .. } => prefix.clone(),
        _ => None,
    };
    Node::new(NodeKind::Ref { prefix, address }, node.span)
}

fn shift_range(lo: u32, hi: u32, edit: &StructuralEdit, max: u32) -> Option<(u32, u32)> {
    let (lo, hi, at, count, max) = (
        u64::from(lo),
        u64::from(hi),
        u64::from(edit.at),
        u64::from(edit.count),
        u64::from(max),
    );
    if edit.kind.is_insert() {
        let nlo = if lo >= at { lo + count } else { lo };
        if nlo >= max {
            return None;
        }
        let nhi = if hi >= at {
            (hi + count).min(max - 1)
        } else {
            hi
        };
        return Some((nlo as u32, nhi as u32));
    }
    let end = at + count; // exclusive
    if lo >= at && hi < end {
        return None;
    }
    let nlo = if lo < at {
        lo
    } else if lo >= end {
        lo - count
    } else {
        at
    };
    let nhi = if hi >= end {
        hi - count
    } else if hi >= at {
        at.checked_sub(1)?
    } else {
        hi
    };
    (nlo <= nhi).then_some((nlo as u32, nhi as u32))
}

/// Shifts an address for an insert or delete. `None` means the reference was deleted.
pub fn shift_address(addr: &RefAddress, edit: &StructuralEdit) -> Option<RefAddress> {
    let rows = edit.kind.is_rows();
    let max = if rows { MAX_ROWS } else { MAX_COLS };
    let mut out = *addr;
    match &mut out {
        RefAddress::Cell { row, col, .. } => {
            let v = if rows { row } else { col };
            *v = shift_range(*v, *v, edit, max)?.0;
        }
        RefAddress::Area { r0, c0, r1, c1, .. } => {
            let (lo, hi) = if rows { (r0, r1) } else { (c0, c1) };
            (*lo, *hi) = shift_range(*lo, *hi, edit, max)?;
        }
        RefAddress::Rows { r0, r1, .. } => {
            if rows {
                (*r0, *r1) = shift_range(*r0, *r1, edit, max)?;
            }
        }
        RefAddress::Cols { c0, c1, .. } => {
            if !rows {
                (*c0, *c1) = shift_range(*c0, *c1, edit, max)?;
            }
        }
    }
    Some(out)
}

/// Rewrites a formula living on `formula_sheet` for an insert or delete on `edit_sheet`.
pub fn shift_formula(
    text: &str,
    edit: &StructuralEdit,
    edit_sheet: &str,
    formula_sheet: &str,
) -> Result<String, ParseError> {
    rewrite_formula(
        text,
        &ParseOptions::default(),
        &PrintOptions::default(),
        |n| {
            let NodeKind::Ref { prefix, address } = &n.kind else {
                return None;
            };
            let sheet = target_sheet(prefix.as_ref(), formula_sheet)?;
            if !same_sheet(sheet, edit_sheet) {
                return None;
            }
            match shift_address(address, edit) {
                None => Some(ref_error(n)),
                Some(next) if next == *address => None,
                Some(next) => Some(with_address(n, next)),
            }
        },
    )
}

fn offset(v: u32, d: i64, abs: bool, max: u32) -> Option<u32> {
    if abs {
        return Some(v);
    }
    let n = i64::from(v).checked_add(d)?;
    (0..i64::from(max)).contains(&n).then_some(n as u32)
}

/// Moves the relative parts of an address by `(d_row, d_col)`. `None` when it leaves the sheet.
pub fn offset_relative(addr: &RefAddress, d_row: i64, d_col: i64) -> Option<RefAddress> {
    Some(match *addr {
        RefAddress::Cell {
            row,
            col,
            row_abs,
            col_abs,
        } => RefAddress::Cell {
            row: offset(row, d_row, row_abs, MAX_ROWS)?,
            col: offset(col, d_col, col_abs, MAX_COLS)?,
            row_abs,
            col_abs,
        },
        RefAddress::Area {
            r0,
            c0,
            r1,
            c1,
            r0_abs,
            c0_abs,
            r1_abs,
            c1_abs,
        } => RefAddress::area(
            offset(r0, d_row, r0_abs, MAX_ROWS)?,
            offset(c0, d_col, c0_abs, MAX_COLS)?,
            r0_abs,
            c0_abs,
            offset(r1, d_row, r1_abs, MAX_ROWS)?,
            offset(c1, d_col, c1_abs, MAX_COLS)?,
            r1_abs,
            c1_abs,
        ),
        RefAddress::Rows {
            r0,
            r1,
            r0_abs,
            r1_abs,
        } => {
            let a = offset(r0, d_row, r0_abs, MAX_ROWS)?;
            let b = offset(r1, d_row, r1_abs, MAX_ROWS)?;
            if a <= b {
                RefAddress::Rows {
                    r0: a,
                    r1: b,
                    r0_abs,
                    r1_abs,
                }
            } else {
                RefAddress::Rows {
                    r0: b,
                    r1: a,
                    r0_abs: r1_abs,
                    r1_abs: r0_abs,
                }
            }
        }
        RefAddress::Cols {
            c0,
            c1,
            c0_abs,
            c1_abs,
        } => {
            let a = offset(c0, d_col, c0_abs, MAX_COLS)?;
            let b = offset(c1, d_col, c1_abs, MAX_COLS)?;
            if a <= b {
                RefAddress::Cols {
                    c0: a,
                    c1: b,
                    c0_abs,
                    c1_abs,
                }
            } else {
                RefAddress::Cols {
                    c0: b,
                    c1: a,
                    c0_abs: c1_abs,
                    c1_abs: c0_abs,
                }
            }
        }
    })
}

/// Copy/fill semantics: a formula copied from (r, c) to (r + `d_row`, c + `d_col`); relative
/// references move with it and references pushed off the sheet become `#REF!`.
pub fn copy_formula(text: &str, d_row: i64, d_col: i64) -> Result<String, ParseError> {
    if d_row == 0 && d_col == 0 {
        return Ok(text.to_owned());
    }
    rewrite_formula(
        text,
        &ParseOptions::default(),
        &PrintOptions::default(),
        |n| {
            let NodeKind::Ref { address, .. } = &n.kind else {
                return None;
            };
            match offset_relative(address, d_row, d_col) {
                None => Some(ref_error(n)),
                Some(next) if next == *address => None,
                Some(next) => Some(with_address(n, next)),
            }
        },
    )
}

/// A cut-and-paste of a block of cells.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveSpec {
    /// Sheet of the moved block.
    pub sheet: String,
    /// The block.
    pub from: Bounds,
    /// Row offset of the destination.
    pub d_row: i64,
    /// Column offset of the destination.
    pub d_col: i64,
    /// Destination sheet when moving to another sheet.
    pub to_sheet: Option<String>,
}

fn shifted(b: &Bounds, d_row: i64, d_col: i64) -> Option<Bounds> {
    let f = |v: u32, d: i64, max: u32| -> Option<u32> {
        let n = i64::from(v).checked_add(d)?;
        (0..i64::from(max)).contains(&n).then_some(n as u32)
    };
    Some(Bounds {
        r0: f(b.r0, d_row, MAX_ROWS)?,
        c0: f(b.c0, d_col, MAX_COLS)?,
        r1: f(b.r1, d_row, MAX_ROWS)?,
        c1: f(b.c1, d_col, MAX_COLS)?,
    })
}

/// Cut/paste semantics for a formula on `formula_sheet`: references that lie entirely inside the
/// moved block follow it (absolute or not); references to cells the block lands on become
/// `#REF!`. The moved formulas themselves keep their text (call this on them too so references
/// into their own block follow).
pub fn move_formula(text: &str, mv: &MoveSpec, formula_sheet: &str) -> Result<String, ParseError> {
    let Some(dest) = shifted(&mv.from, mv.d_row, mv.d_col) else {
        return Ok(text.to_owned());
    };
    let to_sheet = mv.to_sheet.as_deref().unwrap_or(&mv.sheet);
    rewrite_formula(
        text,
        &ParseOptions::default(),
        &PrintOptions::default(),
        |n| {
            let NodeKind::Ref { prefix, address } = &n.kind else {
                return None;
            };
            let sheet = target_sheet(prefix.as_ref(), formula_sheet)?;
            if !matches!(address, RefAddress::Cell { .. } | RefAddress::Area { .. }) {
                return None;
            }
            let b = address_bounds(address);
            if same_sheet(sheet, &mv.sheet) && mv.from.contains_bounds(&b) {
                let next = match *address {
                    RefAddress::Cell {
                        row,
                        col,
                        row_abs,
                        col_abs,
                    } => RefAddress::Cell {
                        row: (i64::from(row) + mv.d_row) as u32,
                        col: (i64::from(col) + mv.d_col) as u32,
                        row_abs,
                        col_abs,
                    },
                    RefAddress::Area {
                        r0,
                        c0,
                        r1,
                        c1,
                        r0_abs,
                        c0_abs,
                        r1_abs,
                        c1_abs,
                    } => RefAddress::Area {
                        r0: (i64::from(r0) + mv.d_row) as u32,
                        c0: (i64::from(c0) + mv.d_col) as u32,
                        r1: (i64::from(r1) + mv.d_row) as u32,
                        c1: (i64::from(c1) + mv.d_col) as u32,
                        r0_abs,
                        c0_abs,
                        r1_abs,
                        c1_abs,
                    },
                    other => other,
                };
                let mut prefix = prefix.clone();
                if !same_sheet(to_sheet, &mv.sheet) {
                    prefix = if same_sheet(to_sheet, formula_sheet) {
                        None
                    } else {
                        Some(SheetPrefix::sheet(to_sheet))
                    };
                }
                return Some(Node::new(
                    NodeKind::Ref {
                        prefix,
                        address: next,
                    },
                    n.span,
                ));
            }
            if same_sheet(sheet, to_sheet) && dest.contains_bounds(&b) {
                return Some(ref_error(n));
            }
            None
        },
    )
}

/// Updates sheet qualifiers after a sheet rename.
pub fn rename_sheet_in_formula(
    text: &str,
    old_name: &str,
    new_name: &str,
) -> Result<String, ParseError> {
    rewrite_formula(
        text,
        &ParseOptions::default(),
        &PrintOptions::default(),
        |n| {
            let p = match &n.kind {
                NodeKind::Ref { prefix, .. }
                | NodeKind::Error { prefix, .. }
                | NodeKind::Name { prefix, .. } => prefix.as_ref()?,
                _ => return None,
            };
            if p.workbook.is_some() {
                return None;
            }
            let hit1 = same_sheet(&p.sheet, old_name);
            let hit2 = p.sheet2.as_deref().is_some_and(|s| same_sheet(s, old_name));
            if !hit1 && !hit2 {
                return None;
            }
            let prefix = Some(SheetPrefix {
                workbook: None,
                sheet: if hit1 {
                    new_name.to_owned()
                } else {
                    p.sheet.clone()
                },
                sheet2: p.sheet2.as_ref().map(|s2| {
                    if hit2 {
                        new_name.to_owned()
                    } else {
                        s2.clone()
                    }
                }),
            });
            let kind = match &n.kind {
                NodeKind::Ref { address, .. } => NodeKind::Ref {
                    prefix,
                    address: *address,
                },
                NodeKind::Error { code, .. } => NodeKind::Error {
                    code: *code,
                    prefix,
                },
                NodeKind::Name { name, .. } => NodeKind::Name {
                    name: name.clone(),
                    prefix,
                },
                _ => return None,
            };
            Some(Node::new(kind, n.span))
        },
    )
}

/// Turns references to a deleted sheet into `#REF!` (and sheet-scoped names into `#NAME?`).
pub fn remove_sheet_in_formula(text: &str, sheet_name: &str) -> Result<String, ParseError> {
    rewrite_formula(
        text,
        &ParseOptions::default(),
        &PrintOptions::default(),
        |n| {
            let (p, is_ref) = match &n.kind {
                NodeKind::Ref { prefix, .. } => (prefix.as_ref()?, true),
                NodeKind::Name { prefix, .. } => (prefix.as_ref()?, false),
                _ => return None,
            };
            if p.workbook.is_some() || p.sheet2.is_some() || !same_sheet(&p.sheet, sheet_name) {
                return None;
            }
            Some(Node::new(
                NodeKind::Error {
                    code: if is_ref {
                        ErrorValue::Ref
                    } else {
                        ErrorValue::Name
                    },
                    prefix: None,
                },
                n.span,
            ))
        },
    )
}

/// Converts an A1 formula on cell `(row, col)` to R1C1 notation.
pub fn to_r1c1(text: &str, row: u32, col: u32) -> Result<String, ParseError> {
    let print_opts = PrintOptions {
        style: RefStyle::R1C1,
        anchor: (row, col),
        leading_equals: false,
    };
    rewrite_formula(text, &ParseOptions::default(), &print_opts, |n| {
        matches!(n.kind, NodeKind::Ref { .. }).then(|| n.clone())
    })
}

/// Converts an R1C1 formula on cell `(row, col)` to A1 notation.
pub fn from_r1c1(text: &str, row: u32, col: u32) -> Result<String, ParseError> {
    rewrite_formula(
        text,
        &ParseOptions::r1c1(row, col),
        &PrintOptions::default(),
        |n| matches!(n.kind, NodeKind::Ref { .. }).then(|| n.clone()),
    )
}

/// Formats one address in R1C1 relative to `(row, col)`.
pub fn address_to_r1c1(addr: &RefAddress, row: u32, col: u32) -> String {
    format_address_r1c1(addr, (row, col))
}

/// The next state of the F4 cycle: `A1` → `$A$1` → `A$1` → `$A1` → `A1`.
pub fn cycle_absolute(addr: &RefAddress) -> RefAddress {
    let next = |col_abs: bool, row_abs: bool| -> (bool, bool) {
        match (col_abs, row_abs) {
            (false, false) => (true, true),
            (true, true) => (false, true),
            (false, true) => (true, false),
            (true, false) => (false, false),
        }
    };
    let mut out = *addr;
    match &mut out {
        RefAddress::Cell {
            row_abs, col_abs, ..
        } => (*col_abs, *row_abs) = next(*col_abs, *row_abs),
        RefAddress::Area {
            r0_abs,
            c0_abs,
            r1_abs,
            c1_abs,
            ..
        } => {
            let (c, r) = next(*c0_abs, *r0_abs);
            (*c0_abs, *c1_abs, *r0_abs, *r1_abs) = (c, c, r, r);
        }
        RefAddress::Rows { r0_abs, r1_abs, .. } => {
            let v = !*r0_abs;
            (*r0_abs, *r1_abs) = (v, v);
        }
        RefAddress::Cols { c0_abs, c1_abs, .. } => {
            let v = !*c0_abs;
            (*c0_abs, *c1_abs) = (v, v);
        }
    }
    out
}

/// The text and selection after toggling `$` with F4.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToggleResult {
    /// The new formula text.
    pub text: String,
    /// Selection start (byte offset).
    pub selection_start: usize,
    /// Selection end (byte offset).
    pub selection_end: usize,
}

/// The F4 key in the formula bar: cycles `$` on the reference(s) at the caret or inside the
/// selection. Works on incomplete formulas (it only tokenizes). The returned selection covers the
/// toggled references, as Excel does. Offsets are bytes.
pub fn toggle_absolute(
    text: &str,
    selection_start: usize,
    selection_end: usize,
    opts: &ParseOptions,
) -> ToggleResult {
    let offset = leading_equals_len(text);
    let refs: Vec<(usize, usize, RefAddress)> = tokenize(&text[offset..], opts, offset as u32)
        .into_iter()
        .filter_map(|t| match t.kind {
            TokenKind::Ref { address, .. } => Some((t.start as usize, t.end as usize, address)),
            _ => None,
        })
        .collect();
    let mut hits: Vec<&(usize, usize, RefAddress)> = refs
        .iter()
        .filter(|t| t.1 > selection_start && t.0 < selection_end)
        .collect();
    if hits.is_empty()
        && let Some(t) = refs
            .iter()
            .find(|t| t.0 <= selection_start && t.1 >= selection_start)
    {
        hits.push(t);
    }
    if hits.is_empty() {
        return ToggleResult {
            text: text.to_owned(),
            selection_start,
            selection_end,
        };
    }
    let mut out = text.to_owned();
    let mut delta: i64 = 0;
    let mut first: Option<usize> = None;
    let mut last = 0usize;
    for &&(start, end, address) in &hits {
        let token_text = &text[start..end];
        let prefix_text = token_text
            .rfind('!')
            .map_or("", |bang| &token_text[..=bang]);
        let next = cycle_absolute(&address);
        let addr_text = match opts.style {
            RefStyle::A1 => format_address(&next),
            RefStyle::R1C1 => format_address_r1c1(&next, opts.anchor),
        };
        let replacement = format!("{prefix_text}{addr_text}");
        let s = (start as i64 + delta) as usize;
        let e = (end as i64 + delta) as usize;
        out.replace_range(s..e, &replacement);
        first.get_or_insert(s);
        last = s + replacement.len();
        delta += replacement.len() as i64 - (end - start) as i64;
    }
    ToggleResult {
        text: out,
        selection_start: first.unwrap_or(selection_start),
        selection_end: last,
    }
}
