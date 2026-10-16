// SPDX-License-Identifier: AGPL-3.0-or-later
//! A sheet → delimited text, the way Excel's "Save As CSV" writes it: from A1 to the end of the
//! used range (spilled values included), each cell as displayed by its number format (General
//! numbers with up to 15 significant digits), fields quoted only when needed, CRLF line endings.

use kiln_sheet::{CellValue, Range, SheetId, Workbook};

use crate::dialect::{Dialect, push_field};
use crate::encoding::{Encoding, encode};

/// Line endings of written files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum LineEnding {
    /// `\r\n` (Excel, RFC 4180).
    #[default]
    Crlf,
    /// `\n`.
    Lf,
}

/// How to write delimited text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExportOptions {
    /// Delimiter and quote.
    pub dialect: Dialect,
    /// Text encoding.
    pub encoding: Encoding,
    /// Cells as displayed (number formats applied), as Excel saves; otherwise raw values.
    pub as_displayed: bool,
    /// Line endings.
    pub line_ending: LineEnding,
}

impl Default for ExportOptions {
    fn default() -> Self {
        ExportOptions {
            dialect: Dialect::default(),
            encoding: Encoding::Utf8,
            as_displayed: true,
            line_ending: LineEnding::Crlf,
        }
    }
}

impl ExportOptions {
    /// Tab-separated text.
    #[must_use]
    pub fn tsv() -> Self {
        ExportOptions {
            dialect: Dialect {
                delimiter: '\t',
                quote: '"',
            },
            ..ExportOptions::default()
        }
    }
}

/// A number with up to 15 significant digits, Excel's precision (`0.1`, `1234567.891`,
/// `1.23456789012346E+20`).
#[must_use]
pub fn number_text(n: f64) -> String {
    if !n.is_finite() {
        return "#NUM!".to_owned();
    }
    if n == 0.0 {
        return "0".to_owned();
    }
    // Round to 15 significant digits first.
    let sci = format!("{n:.14e}");
    let rounded: f64 = sci.parse().unwrap_or(n);
    let a = rounded.abs();
    if (1e-5..1e15).contains(&a) {
        let s = format!("{rounded}");
        return s;
    }
    let e = format!("{rounded:e}");
    match e.split_once('e') {
        Some((m, x)) => {
            let exp: i32 = x.parse().unwrap_or(0);
            let sign = if exp < 0 { '-' } else { '+' };
            format!("{m}E{sign}{:02}", exp.abs())
        }
        None => e,
    }
}

/// The extent written: A1 to the last row and column with content (spills included).
#[must_use]
pub fn export_range(wb: &Workbook, sheet: SheetId) -> Option<Range> {
    let s = wb.sheet(sheet)?;
    let mut used = s.cells().used_range(true);
    for (r, c, d) in s.cells().iter() {
        if d.kind.is_engine_formula()
            && let Some(sp) = wb.spill_range(sheet, r, c)
            && !sp.blocked
        {
            let b = sp.bounds;
            let g = Range::new(b.r0, b.c0, b.r1, b.c1);
            used = Some(used.map_or(g, |u| u.union(&g)));
        }
    }
    used.map(|u| Range::new(0, 0, u.r1, u.c1))
}

/// The text of one cell as written.
#[must_use]
pub fn cell_text(wb: &Workbook, sheet: SheetId, row: u32, col: u32, as_displayed: bool) -> String {
    let v = wb.value(sheet, row, col);
    match &v {
        CellValue::Blank => String::new(),
        CellValue::Number(n) => {
            if as_displayed {
                let style = wb.sheet(sheet).map_or(0, |s| s.style_at(row, col));
                let code = wb.styles().num_fmt(style);
                if code.eq_ignore_ascii_case("General") {
                    number_text(*n)
                } else {
                    wb.display_text(sheet, row, col)
                }
            } else {
                number_text(*n)
            }
        }
        CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        CellValue::Error(e) => e.code().to_owned(),
        CellValue::Text(t) => t.to_string(),
        CellValue::RichText(r) => r.plain().to_string(),
    }
}

/// A sheet as delimited text.
#[must_use]
pub fn sheet_to_text(wb: &Workbook, sheet: SheetId, options: &ExportOptions) -> String {
    let Some(range) = export_range(wb, sheet) else {
        return String::new();
    };
    let eol = match options.line_ending {
        LineEnding::Crlf => "\r\n",
        LineEnding::Lf => "\n",
    };
    let mut out = String::with_capacity(((range.r1 + 1) * (range.c1 + 1)) as usize * 6);
    for r in range.r0..=range.r1 {
        for c in range.c0..=range.c1 {
            if c > range.c0 {
                out.push(options.dialect.delimiter);
            }
            let t = cell_text(wb, sheet, r, c, options.as_displayed);
            push_field(&mut out, &t, options.dialect);
        }
        out.push_str(eol);
    }
    out
}

/// A sheet as an encoded delimited file.
#[must_use]
pub fn write_delimited(wb: &Workbook, sheet: SheetId, options: &ExportOptions) -> Vec<u8> {
    encode(&sheet_to_text(wb, sheet, options), options.encoding)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_have_excel_precision() {
        assert_eq!(number_text(0.1 + 0.2), "0.3");
        assert_eq!(number_text(std::f64::consts::E), "2.71828182845905");
        assert_eq!(number_text(1e20), "1E+20");
        assert_eq!(number_text(-1.5e-7), "-1.5E-07");
        assert_eq!(number_text(123_456_789.0), "123456789");
        assert_eq!(number_text(1e15), "1E+15");
    }
}
