// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cell addresses in A1 notation, sheet qualifiers and their quoting rules.

use crate::parser::{RefAddress, SheetPrefix};

/// Rows in a worksheet (1,048,576).
pub const MAX_ROWS: u32 = 1_048_576;
/// Columns in a worksheet (16,384, `XFD`).
pub const MAX_COLS: u32 = 16_384;

/// A rectangle of cells, 0-based and inclusive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Bounds {
    /// First row.
    pub r0: u32,
    /// First column.
    pub c0: u32,
    /// Last row.
    pub r1: u32,
    /// Last column.
    pub c1: u32,
}

impl Bounds {
    /// True when (row, col) lies inside.
    pub fn contains(&self, row: u32, col: u32) -> bool {
        row >= self.r0 && row <= self.r1 && col >= self.c0 && col <= self.c1
    }

    /// True when `other` lies entirely inside.
    pub fn contains_bounds(&self, other: &Bounds) -> bool {
        other.r0 >= self.r0 && other.r1 <= self.r1 && other.c0 >= self.c0 && other.c1 <= self.c1
    }

    /// True when the rectangles share at least one cell.
    pub fn overlaps(&self, other: &Bounds) -> bool {
        self.r0 <= other.r1 && other.r0 <= self.r1 && self.c0 <= other.c1 && other.c0 <= self.c1
    }

    /// Number of cells (saturating).
    pub fn cell_count(&self) -> u64 {
        u64::from(self.r1 - self.r0 + 1) * u64::from(self.c1 - self.c0 + 1)
    }
}

/// `0` → `A`, `25` → `Z`, `26` → `AA`.
pub fn column_to_letters(col: u32) -> String {
    let mut n = u64::from(col) + 1;
    let mut out = Vec::new();
    while n > 0 {
        let r = ((n - 1) % 26) as u8;
        out.push(b'A' + r);
        n = (n - 1) / 26;
    }
    out.reverse();
    String::from_utf8(out).unwrap_or_default()
}

/// `A` → 0, `xfd` → 16383. `None` for anything but ASCII letters or an empty string.
pub fn letters_to_column(letters: &str) -> Option<u32> {
    if letters.is_empty() || letters.len() > 7 {
        return None;
    }
    let mut n: u64 = 0;
    for b in letters.bytes() {
        if !b.is_ascii_alphabetic() {
            return None;
        }
        n = n * 26 + u64::from(b.to_ascii_lowercase() - b'a' + 1);
    }
    u32::try_from(n - 1).ok()
}

/// Formats one cell such as `$B$3`.
pub fn format_cell(row: u32, col: u32, row_abs: bool, col_abs: bool) -> String {
    format!(
        "{}{}{}{}",
        if col_abs { "$" } else { "" },
        column_to_letters(col),
        if row_abs { "$" } else { "" },
        u64::from(row) + 1
    )
}

/// Formats the address part of a reference (no sheet).
pub fn format_address(a: &RefAddress) -> String {
    match *a {
        RefAddress::Cell {
            row,
            col,
            row_abs,
            col_abs,
        } => format_cell(row, col, row_abs, col_abs),
        RefAddress::Area {
            r0,
            c0,
            r1,
            c1,
            r0_abs,
            c0_abs,
            r1_abs,
            c1_abs,
        } => format!(
            "{}:{}",
            format_cell(r0, c0, r0_abs, c0_abs),
            format_cell(r1, c1, r1_abs, c1_abs)
        ),
        RefAddress::Rows {
            r0,
            r1,
            r0_abs,
            r1_abs,
        } => format!(
            "{}{}:{}{}",
            if r0_abs { "$" } else { "" },
            u64::from(r0) + 1,
            if r1_abs { "$" } else { "" },
            u64::from(r1) + 1
        ),
        RefAddress::Cols {
            c0,
            c1,
            c0_abs,
            c1_abs,
        } => format!(
            "{}{}:{}{}",
            if c0_abs { "$" } else { "" },
            column_to_letters(c0),
            if c1_abs { "$" } else { "" },
            column_to_letters(c1)
        ),
    }
}

/// The rectangle a reference address covers.
pub fn address_bounds(a: &RefAddress) -> Bounds {
    match *a {
        RefAddress::Cell { row, col, .. } => Bounds {
            r0: row,
            c0: col,
            r1: row,
            c1: col,
        },
        RefAddress::Area { r0, c0, r1, c1, .. } => Bounds { r0, c0, r1, c1 },
        RefAddress::Rows { r0, r1, .. } => Bounds {
            r0,
            c0: 0,
            r1,
            c1: MAX_COLS - 1,
        },
        RefAddress::Cols { c0, c1, .. } => Bounds {
            r0: 0,
            c0,
            r1: MAX_ROWS - 1,
            c1,
        },
    }
}

/// Parses a plain cell address like `B3` or `$B$3` into 0-based `(row, col)`.
pub fn parse_cell_address(text: &str) -> Option<(u32, u32)> {
    let t = text.trim();
    let b = t.as_bytes();
    let mut i = 0;
    if b.first() == Some(&b'$') {
        i += 1;
    }
    let ls = i;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    let letters = &t[ls..i];
    if letters.is_empty() || letters.len() > 3 {
        return None;
    }
    if b.get(i) == Some(&b'$') {
        i += 1;
    }
    let digits = &t[i..];
    if digits.is_empty() || digits.len() > 7 || !digits.bytes().all(|d| d.is_ascii_digit()) {
        return None;
    }
    let col = letters_to_column(letters)?;
    let row: u32 = digits.parse().ok()?;
    if row == 0 || row > MAX_ROWS || col >= MAX_COLS {
        return None;
    }
    Some((row - 1, col))
}

fn is_plain_sheet_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '\\' || c as u32 >= 0xA1
}

fn is_plain_sheet_char(c: char) -> bool {
    is_plain_sheet_start(c) || c.is_ascii_digit() || c == '.'
}

/// True when a sheet name must be quoted in a formula (`'My Sheet'!A1`).
pub fn sheet_needs_quotes(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if is_plain_sheet_start(c) => {}
        _ => return true,
    }
    if !chars.all(is_plain_sheet_char) {
        return true;
    }
    // Looks like a cell (A1, XFD2).
    let b = name.as_bytes();
    let letters = b.iter().take_while(|c| c.is_ascii_alphabetic()).count();
    if (1..=3).contains(&letters)
        && letters < b.len()
        && b[letters..].iter().all(u8::is_ascii_digit)
    {
        return true;
    }
    // R1C1-like: R, C, R1, C2, R1C1, RC.
    let upper = name.to_ascii_uppercase();
    let u = upper.as_bytes();
    let digits_from = |i: usize| {
        let mut j = i;
        while j < u.len() && u[j].is_ascii_digit() {
            j += 1;
        }
        j
    };
    if u[0] == b'R' {
        let j = digits_from(1);
        if j == u.len() {
            return true;
        }
        if u[j] == b'C' && digits_from(j + 1) == u.len() {
            return true;
        }
    }
    if u[0] == b'C' && digits_from(1) == u.len() {
        return true;
    }
    upper == "TRUE" || upper == "FALSE"
}

/// Quotes a sheet name, doubling inner apostrophes.
pub fn quote_sheet(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

/// Formats a sheet prefix including the `!`.
pub fn format_prefix(p: &SheetPrefix) -> String {
    let sheets = match &p.sheet2 {
        Some(s2) => format!("{}:{s2}", p.sheet),
        None => p.sheet.clone(),
    };
    let needs = sheet_needs_quotes(&p.sheet) || p.sheet2.as_deref().is_some_and(sheet_needs_quotes);
    let book = p
        .workbook
        .as_ref()
        .map(|w| format!("[{w}]"))
        .unwrap_or_default();
    if needs {
        format!("{}!", quote_sheet(&format!("{book}{sheets}")))
    } else {
        format!("{book}{sheets}!")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn columns_round_trip() {
        for (c, s) in [
            (0, "A"),
            (25, "Z"),
            (26, "AA"),
            (701, "ZZ"),
            (16_383, "XFD"),
        ] {
            assert_eq!(column_to_letters(c), s);
            assert_eq!(letters_to_column(s), Some(c));
            assert_eq!(letters_to_column(&s.to_lowercase()), Some(c));
        }
        assert_eq!(letters_to_column("A1"), None);
        assert_eq!(letters_to_column(""), None);
    }

    #[test]
    fn cell_addresses() {
        assert_eq!(parse_cell_address("B3"), Some((2, 1)));
        assert_eq!(parse_cell_address("$B$3"), Some((2, 1)));
        assert_eq!(parse_cell_address("XFD1048576"), Some((1_048_575, 16_383)));
        assert_eq!(parse_cell_address("XFE1"), None);
        assert_eq!(parse_cell_address("A0"), None);
        assert_eq!(parse_cell_address("A1048577"), None);
        assert_eq!(format_cell(2, 1, true, false), "B$3");
    }

    #[test]
    fn sheet_quoting() {
        for (name, quoted) in [
            ("Sheet1", false),
            ("My Sheet", true),
            ("2024", true),
            ("A1", true),
            ("R1C1", true),
            ("RC", true),
            ("R", true),
            ("C3", true),
            ("TRUE", true),
            ("Données", false),
            ("Revenue.Q1", false),
            ("S2", true),
            ("Rate", false),
            ("O'Brien", true),
        ] {
            assert_eq!(sheet_needs_quotes(name), quoted, "{name}");
        }
        assert_eq!(quote_sheet("It's"), "'It''s'");
    }
}
