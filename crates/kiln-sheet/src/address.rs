// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cell positions and ranges: 0-based, inclusive rectangles over the 1,048,576 × 16,384 grid, with
//! A1 parsing and printing (`B7`, `A1:C3`, whole columns `A:C`, whole rows `2:5`) and OOXML `sqref`
//! lists.

use std::fmt;

use kiln_formula::refs::{Bounds, column_to_letters, letters_to_column};
use serde::{Deserialize, Serialize};

use crate::error::ModelError;

/// Rows in a worksheet (1,048,576).
pub const MAX_ROWS: u32 = kiln_formula::refs::MAX_ROWS;
/// Columns in a worksheet (16,384, column `XFD`).
pub const MAX_COLS: u32 = kiln_formula::refs::MAX_COLS;

/// One cell position (0-based).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub struct CellPos {
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
}

impl CellPos {
    /// A position.
    pub const fn new(row: u32, col: u32) -> CellPos {
        CellPos { row, col }
    }
}

/// A rectangle of cells, 0-based and inclusive (`r0 <= r1`, `c0 <= c1`). Deserialised ranges
/// (commands from the control channel, saved data) are normalised: corners in order, clamped to
/// the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(from = "RawRange")]
pub struct Range {
    /// First row.
    pub r0: u32,
    /// First column.
    pub c0: u32,
    /// Last row.
    pub r1: u32,
    /// Last column.
    pub c1: u32,
}

#[derive(Deserialize)]
struct RawRange {
    r0: u32,
    c0: u32,
    r1: u32,
    c1: u32,
}

impl From<RawRange> for Range {
    fn from(r: RawRange) -> Range {
        Range::new(
            r.r0.min(MAX_ROWS - 1),
            r.c0.min(MAX_COLS - 1),
            r.r1.min(MAX_ROWS - 1),
            r.c1.min(MAX_COLS - 1),
        )
    }
}

impl Range {
    /// A range from two corners in any order.
    pub fn new(ra: u32, ca: u32, rb: u32, cb: u32) -> Range {
        Range {
            r0: ra.min(rb),
            c0: ca.min(cb),
            r1: ra.max(rb),
            c1: ca.max(cb),
        }
    }

    /// One cell.
    pub const fn cell(row: u32, col: u32) -> Range {
        Range {
            r0: row,
            c0: col,
            r1: row,
            c1: col,
        }
    }

    /// Whole rows `r0..=r1`.
    pub const fn rows(r0: u32, r1: u32) -> Range {
        Range {
            r0,
            c0: 0,
            r1,
            c1: MAX_COLS - 1,
        }
    }

    /// Whole columns `c0..=c1`.
    pub const fn cols(c0: u32, c1: u32) -> Range {
        Range {
            r0: 0,
            c0,
            r1: MAX_ROWS - 1,
            c1,
        }
    }

    /// The whole sheet.
    pub const fn sheet() -> Range {
        Range {
            r0: 0,
            c0: 0,
            r1: MAX_ROWS - 1,
            c1: MAX_COLS - 1,
        }
    }

    /// True when (row, col) lies inside.
    pub fn contains(&self, row: u32, col: u32) -> bool {
        row >= self.r0 && row <= self.r1 && col >= self.c0 && col <= self.c1
    }

    /// True when the rectangles share at least one cell.
    pub fn intersects(&self, other: &Range) -> bool {
        self.r0 <= other.r1 && other.r0 <= self.r1 && self.c0 <= other.c1 && other.c0 <= self.c1
    }

    /// True when `self` lies entirely inside `outer`.
    pub fn is_inside(&self, outer: &Range) -> bool {
        self.r0 >= outer.r0 && self.r1 <= outer.r1 && self.c0 >= outer.c0 && self.c1 <= outer.c1
    }

    /// The common cells, if any.
    pub fn intersection(&self, other: &Range) -> Option<Range> {
        self.intersects(other).then(|| Range {
            r0: self.r0.max(other.r0),
            c0: self.c0.max(other.c0),
            r1: self.r1.min(other.r1),
            c1: self.c1.min(other.c1),
        })
    }

    /// The smallest range containing both.
    pub fn union(&self, other: &Range) -> Range {
        Range {
            r0: self.r0.min(other.r0),
            c0: self.c0.min(other.c0),
            r1: self.r1.max(other.r1),
            c1: self.c1.max(other.c1),
        }
    }

    /// Number of rows.
    pub fn height(&self) -> u32 {
        self.r1 - self.r0 + 1
    }

    /// Number of columns.
    pub fn width(&self) -> u32 {
        self.c1 - self.c0 + 1
    }

    /// Number of cells.
    pub fn cell_count(&self) -> u64 {
        u64::from(self.height()) * u64::from(self.width())
    }

    /// True for one cell.
    pub fn is_single_cell(&self) -> bool {
        self.r0 == self.r1 && self.c0 == self.c1
    }

    /// True when the range covers every row (whole columns).
    pub fn is_whole_cols(&self) -> bool {
        self.r0 == 0 && self.r1 == MAX_ROWS - 1
    }

    /// True when the range covers every column (whole rows).
    pub fn is_whole_rows(&self) -> bool {
        self.c0 == 0 && self.c1 == MAX_COLS - 1
    }

    /// The range moved by `(d_row, d_col)`, or `None` when it would leave the sheet.
    pub fn offset(&self, d_row: i64, d_col: i64) -> Option<Range> {
        let f = |v: u32, d: i64, max: u32| -> Option<u32> {
            let n = i64::from(v).checked_add(d)?;
            (0..i64::from(max)).contains(&n).then_some(n as u32)
        };
        Some(Range {
            r0: f(self.r0, d_row, MAX_ROWS)?,
            c0: f(self.c0, d_col, MAX_COLS)?,
            r1: f(self.r1, d_row, MAX_ROWS)?,
            c1: f(self.c1, d_col, MAX_COLS)?,
        })
    }

    /// Checks the range lies on the sheet with its corners in order.
    pub fn check(&self) -> Result<(), ModelError> {
        check_cell(self.r0, self.c0)?;
        check_cell(self.r1, self.c1)?;
        if self.r0 > self.r1 || self.c0 > self.c1 {
            return Err(ModelError::invalid("the corners of the range are reversed"));
        }
        Ok(())
    }

    /// Every cell, row by row.
    pub fn cells(&self) -> impl Iterator<Item = CellPos> + '_ {
        let (c0, c1) = (self.c0, self.c1);
        (self.r0..=self.r1).flat_map(move |row| (c0..=c1).map(move |col| CellPos { row, col }))
    }

    /// The formula engine's rectangle.
    pub const fn to_bounds(&self) -> Bounds {
        Bounds {
            r0: self.r0,
            c0: self.c0,
            r1: self.r1,
            c1: self.c1,
        }
    }

    /// From the formula engine's rectangle.
    pub const fn from_bounds(b: Bounds) -> Range {
        Range {
            r0: b.r0,
            c0: b.c0,
            r1: b.r1,
            c1: b.c1,
        }
    }

    /// A1 text: `A1`, `A1:B2`, whole columns `A:C`, whole rows `1:3`; `$` everywhere with
    /// `absolute`.
    pub fn to_a1(&self, absolute: bool) -> String {
        let d = if absolute { "$" } else { "" };
        if self.is_whole_cols() && !self.is_whole_rows() {
            return format!(
                "{d}{}:{d}{}",
                column_to_letters(self.c0),
                column_to_letters(self.c1)
            );
        }
        if self.is_whole_rows() && !self.is_whole_cols() {
            return format!("{d}{}:{d}{}", self.r0 + 1, self.r1 + 1);
        }
        if self.is_single_cell() {
            return cell_a1(self.r0, self.c0, absolute);
        }
        format!(
            "{}:{}",
            cell_a1(self.r0, self.c0, absolute),
            cell_a1(self.r1, self.c1, absolute)
        )
    }

    /// Parses `A1`, `A1:C3`, `A:C` or `2:5` (case-insensitive, `$` allowed, no sheet prefix).
    pub fn parse_a1(text: &str) -> Option<Range> {
        let text = text.trim();
        let mut parts = text.split(':');
        let a = parts.next()?;
        let b = parts.next();
        if parts.next().is_some() {
            return None;
        }
        let Some(b) = b else {
            let p = parse_cell_a1(a)?;
            return Some(Range::cell(p.row, p.col));
        };
        if let (Some(x), Some(y)) = (parse_cell_a1(a), parse_cell_a1(b)) {
            return Some(Range::new(x.row, x.col, y.row, y.col));
        }
        if let (Some(x), Some(y)) = (parse_col(a), parse_col(b)) {
            return Some(Range::cols(x.min(y), x.max(y)));
        }
        if let (Some(x), Some(y)) = (parse_row(a), parse_row(b)) {
            return Some(Range::rows(x.min(y), x.max(y)));
        }
        None
    }
}

impl fmt::Display for Range {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_a1(false))
    }
}

impl From<CellPos> for Range {
    fn from(p: CellPos) -> Range {
        Range::cell(p.row, p.col)
    }
}

/// Checks a cell lies on the sheet.
pub fn check_cell(row: u32, col: u32) -> Result<(), ModelError> {
    if row >= MAX_ROWS || col >= MAX_COLS {
        return Err(ModelError::OutOfRange { row, col });
    }
    Ok(())
}

/// `B7` (or `$B$7`).
pub fn cell_a1(row: u32, col: u32, absolute: bool) -> String {
    let d = if absolute { "$" } else { "" };
    format!("{d}{}{d}{}", column_to_letters(col), u64::from(row) + 1)
}

fn strip_dollar(s: &str) -> &str {
    s.strip_prefix('$').unwrap_or(s)
}

fn parse_col(s: &str) -> Option<u32> {
    let s = strip_dollar(s);
    if s.is_empty() || s.len() > 3 || !s.bytes().all(|b| b.is_ascii_alphabetic()) {
        return None;
    }
    letters_to_column(s).filter(|&c| c < MAX_COLS)
}

fn parse_row(s: &str) -> Option<u32> {
    let s = strip_dollar(s);
    if s.is_empty() || s.len() > 7 || !s.bytes().all(|b| b.is_ascii_digit()) || s.starts_with('0') {
        return None;
    }
    let n: u32 = s.parse().ok()?;
    (1..=MAX_ROWS).contains(&n).then(|| n - 1)
}

/// Parses `B7` (case-insensitive, `$` allowed).
pub fn parse_cell_a1(text: &str) -> Option<CellPos> {
    let s = strip_dollar(text.trim());
    let split = s.find(|c: char| !c.is_ascii_alphabetic())?;
    let (letters, rest) = s.split_at(split);
    let col = parse_col(letters)?;
    let row = parse_row(rest)?;
    Some(CellPos { row, col })
}

/// Parses a space-separated list of ranges (an OOXML `sqref`).
pub fn parse_sqref(text: &str) -> Result<Vec<Range>, ModelError> {
    text.split_whitespace()
        .map(|part| {
            Range::parse_a1(part)
                .ok_or_else(|| ModelError::invalid(format!("invalid range \"{part}\"")))
        })
        .collect()
}

/// Prints a list of ranges as an OOXML `sqref`.
pub fn format_sqref(ranges: &[Range]) -> String {
    ranges
        .iter()
        .map(|r| r.to_a1(false))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prints_ranges_in_their_shortest_form() {
        assert_eq!(Range::cell(0, 0).to_a1(false), "A1");
        assert_eq!(Range::new(2, 3, 0, 0).to_a1(false), "A1:D3");
        assert_eq!(Range::cols(0, 2).to_a1(false), "A:C");
        assert_eq!(Range::rows(1, 4).to_a1(true), "$2:$5");
        assert_eq!(Range::sheet().to_a1(false), "A1:XFD1048576");
        assert_eq!(Range::cell(1_048_575, 16_383).to_a1(true), "$XFD$1048576");
    }

    #[test]
    fn parses_a1_forms() {
        assert_eq!(Range::parse_a1("b7"), Some(Range::cell(6, 1)));
        assert_eq!(Range::parse_a1("$C$3:A1"), Some(Range::new(0, 0, 2, 2)));
        assert_eq!(Range::parse_a1("A:C"), Some(Range::cols(0, 2)));
        assert_eq!(Range::parse_a1("5:2"), Some(Range::rows(1, 4)));
        assert_eq!(Range::parse_a1("XFE1"), None);
        assert_eq!(Range::parse_a1("A1048577"), None);
        assert_eq!(Range::parse_a1("A0"), None);
        assert_eq!(Range::parse_a1("A1:B2:C3"), None);
        assert_eq!(Range::parse_a1(""), None);
        assert_eq!(
            parse_cell_a1("XFD1048576"),
            Some(CellPos::new(1_048_575, 16_383))
        );
    }

    #[test]
    fn parses_and_validates_sqref_lists() {
        let r = parse_sqref("A1:B2 D4  F:F").expect("valid");
        assert_eq!(r.len(), 3);
        assert_eq!(format_sqref(&r), "A1:B2 D4 F:F");
        assert!(parse_sqref("A1 nonsense").is_err());
    }

    #[test]
    fn deserialised_ranges_are_normalised() {
        let r: Range = serde_json::from_str(r#"{"r0":5,"c0":9,"r1":1,"c1":99999}"#).expect("json");
        assert_eq!(r, Range::new(1, 9, 5, MAX_COLS - 1));
        assert!(r.check().is_ok());
    }

    #[test]
    fn set_operations() {
        let a = Range::new(0, 0, 4, 4);
        let b = Range::new(3, 3, 6, 6);
        assert!(a.intersects(&b));
        assert_eq!(a.intersection(&b), Some(Range::new(3, 3, 4, 4)));
        assert_eq!(a.union(&b), Range::new(0, 0, 6, 6));
        assert!(Range::cell(1, 1).is_inside(&a));
        assert_eq!(a.offset(-1, 0), None);
        assert_eq!(a.offset(2, 1), Some(Range::new(2, 1, 6, 5)));
        assert_eq!(a.cells().count(), 25);
        assert!(Range::new(0, 0, MAX_ROWS, 0).check().is_err());
    }
}
