// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formulas: OpenFormula (`of:=SUM([.A1:.B2];[$Data.C3])`, ODF 1.3 Part 4) ⇄ Excel syntax
//! (`SUM(A1:B2,Data!C3)`), which is what the Grid model and its engine use.
//!
//! The translation is token by token:
//!
//! | Excel | OpenFormula |
//! |---|---|
//! | `A1`, `$A$1:B2`, `A:A`, `1:3` | `[.A1]`, `[.$A$1:.B2]`, `[.A:.A]`, `[.1:.3]` |
//! | `Data!A1`, `'My sheet'!A1:B2`, `S1:S3!A1` | `[$Data.A1]`, `[$'My sheet'.A1:.B2]`, `[$S1.A1:$S3.A1]` |
//! | `,` between arguments | `;` |
//! | `{1,2;3,4}` | `{1;2\|3;4}` |
//! | union `(A1,B1)` | `[.A1]~[.B1]` |
//! | intersection `A1:B5 B2:C3` (a space) | `!` |
//! | `TRUE`, `FALSE` | `TRUE()`, `FALSE()` |
//! | functions | the same names, except those ODF names differently ([`FUNCTIONS`]: `CEILING` is `COM.MICROSOFT.CEILING`, `NORMSDIST` is `LEGACY.NORMSDIST`, `FORMULATEXT` is `FORMULA`, …) |
//!
//! Reading also accepts LibreOffice's legacy `oooc:` prefix (same syntax) and `msoxl:` (Excel
//! syntax as is), and undoes the `_xlfn.` prefixes and lower-case names LibreOffice writes for
//! functions it does not know. Cell range addresses of attributes (`table:cell-range-address`,
//! `table:target-range-address`) are handled by [`parse_range_list`] and [`range_address`].

use kiln_formula::parser::{
    OperatorToken, ParseOptions, RefAddress, SheetPrefix, TokenKind, tokenize,
};
use kiln_formula::refs::{column_to_letters, letters_to_column, quote_sheet, sheet_needs_quotes};
use kiln_sheet::Range;

/// `(Excel name, OpenFormula name)` for every function whose names differ.
pub const FUNCTIONS: &[(&str, &str)] = &[
    ("AGGREGATE", "COM.MICROSOFT.AGGREGATE"),
    ("ARRAYTOTEXT", "COM.MICROSOFT.ARRAYTOTEXT"),
    ("BAHTTEXT", "COM.MICROSOFT.BAHTTEXT"),
    ("BETA.DIST", "COM.MICROSOFT.BETA.DIST"),
    ("BETA.INV", "COM.MICROSOFT.BETA.INV"),
    ("BINOM.DIST", "COM.MICROSOFT.BINOM.DIST"),
    ("BINOM.DIST.RANGE", "COM.MICROSOFT.BINOM.DIST.RANGE"),
    ("BINOM.INV", "COM.MICROSOFT.BINOM.INV"),
    ("BYCOL", "COM.MICROSOFT.BYCOL"),
    ("BYROW", "COM.MICROSOFT.BYROW"),
    ("CEILING", "COM.MICROSOFT.CEILING"),
    // OpenFormula's CEILING and FLOOR (number; significance; mode) are Excel's .MATH functions:
    // same arguments, same rounding of negative numbers.
    ("CEILING.MATH", "CEILING"),
    ("CEILING.PRECISE", "COM.MICROSOFT.CEILING.PRECISE"),
    ("CHIDIST", "LEGACY.CHIDIST"),
    ("CHIINV", "LEGACY.CHIINV"),
    ("CHISQ.DIST", "COM.MICROSOFT.CHISQ.DIST"),
    ("CHISQ.DIST.RT", "COM.MICROSOFT.CHISQ.DIST.RT"),
    ("CHISQ.INV", "COM.MICROSOFT.CHISQ.INV"),
    ("CHISQ.INV.RT", "COM.MICROSOFT.CHISQ.INV.RT"),
    ("CHISQ.TEST", "COM.MICROSOFT.CHISQ.TEST"),
    ("CHITEST", "LEGACY.CHITEST"),
    ("CHOOSECOLS", "COM.MICROSOFT.CHOOSECOLS"),
    ("CHOOSEROWS", "COM.MICROSOFT.CHOOSEROWS"),
    ("CONCAT", "COM.MICROSOFT.CONCAT"),
    ("CONFIDENCE.NORM", "COM.MICROSOFT.CONFIDENCE.NORM"),
    ("CONFIDENCE.T", "COM.MICROSOFT.CONFIDENCE.T"),
    ("COVARIANCE.P", "COM.MICROSOFT.COVARIANCE.P"),
    ("COVARIANCE.S", "COM.MICROSOFT.COVARIANCE.S"),
    ("DROP", "COM.MICROSOFT.DROP"),
    ("ENCODEURL", "COM.MICROSOFT.ENCODEURL"),
    ("ERF.PRECISE", "COM.MICROSOFT.ERF.PRECISE"),
    ("ERFC.PRECISE", "COM.MICROSOFT.ERFC.PRECISE"),
    ("EXPAND", "COM.MICROSOFT.EXPAND"),
    ("EXPON.DIST", "COM.MICROSOFT.EXPON.DIST"),
    ("F.DIST", "COM.MICROSOFT.F.DIST"),
    ("F.DIST.RT", "COM.MICROSOFT.F.DIST.RT"),
    ("F.INV", "COM.MICROSOFT.F.INV"),
    ("F.INV.RT", "COM.MICROSOFT.F.INV.RT"),
    ("F.TEST", "COM.MICROSOFT.F.TEST"),
    ("FDIST", "LEGACY.FDIST"),
    ("FILTER", "COM.MICROSOFT.FILTER"),
    ("FILTERXML", "COM.MICROSOFT.FILTERXML"),
    ("FINV", "LEGACY.FINV"),
    ("FLOOR", "COM.MICROSOFT.FLOOR"),
    ("FLOOR.MATH", "FLOOR"),
    ("FLOOR.PRECISE", "COM.MICROSOFT.FLOOR.PRECISE"),
    ("FORECAST.ETS", "COM.MICROSOFT.FORECAST.ETS"),
    ("FORECAST.ETS.CONFINT", "COM.MICROSOFT.FORECAST.ETS.CONFINT"),
    (
        "FORECAST.ETS.SEASONALITY",
        "COM.MICROSOFT.FORECAST.ETS.SEASONALITY",
    ),
    ("FORECAST.ETS.STAT", "COM.MICROSOFT.FORECAST.ETS.STAT"),
    ("FORECAST.LINEAR", "COM.MICROSOFT.FORECAST.LINEAR"),
    ("FORMULATEXT", "FORMULA"),
    ("GAMMA.DIST", "COM.MICROSOFT.GAMMA.DIST"),
    ("GAMMA.INV", "COM.MICROSOFT.GAMMA.INV"),
    ("GAMMALN.PRECISE", "COM.MICROSOFT.GAMMALN.PRECISE"),
    ("HSTACK", "COM.MICROSOFT.HSTACK"),
    ("HYPGEOM.DIST", "COM.MICROSOFT.HYPGEOM.DIST"),
    ("IFS", "COM.MICROSOFT.IFS"),
    ("LAMBDA", "COM.MICROSOFT.LAMBDA"),
    ("LET", "COM.MICROSOFT.LET"),
    ("LOGNORM.DIST", "COM.MICROSOFT.LOGNORM.DIST"),
    ("LOGNORM.INV", "COM.MICROSOFT.LOGNORM.INV"),
    ("MAKEARRAY", "COM.MICROSOFT.MAKEARRAY"),
    ("MAP", "COM.MICROSOFT.MAP"),
    ("MAXIFS", "COM.MICROSOFT.MAXIFS"),
    ("MINIFS", "COM.MICROSOFT.MINIFS"),
    ("MODE.MULT", "COM.MICROSOFT.MODE.MULT"),
    ("MODE.SNGL", "COM.MICROSOFT.MODE.SNGL"),
    ("NEGBINOM.DIST", "COM.MICROSOFT.NEGBINOM.DIST"),
    ("NORM.DIST", "COM.MICROSOFT.NORM.DIST"),
    ("NORM.INV", "COM.MICROSOFT.NORM.INV"),
    ("NORM.S.DIST", "COM.MICROSOFT.NORM.S.DIST"),
    ("NORM.S.INV", "COM.MICROSOFT.NORM.S.INV"),
    ("NORMSDIST", "LEGACY.NORMSDIST"),
    ("NORMSINV", "LEGACY.NORMSINV"),
    ("PERCENTILE.EXC", "COM.MICROSOFT.PERCENTILE.EXC"),
    ("PERCENTILE.INC", "COM.MICROSOFT.PERCENTILE.INC"),
    ("PERCENTRANK.EXC", "COM.MICROSOFT.PERCENTRANK.EXC"),
    ("PERCENTRANK.INC", "COM.MICROSOFT.PERCENTRANK.INC"),
    ("POISSON.DIST", "COM.MICROSOFT.POISSON.DIST"),
    ("QUARTILE.EXC", "COM.MICROSOFT.QUARTILE.EXC"),
    ("QUARTILE.INC", "COM.MICROSOFT.QUARTILE.INC"),
    ("RANDARRAY", "COM.MICROSOFT.RANDARRAY"),
    ("RANK.AVG", "COM.MICROSOFT.RANK.AVG"),
    ("RANK.EQ", "COM.MICROSOFT.RANK.EQ"),
    ("REDUCE", "COM.MICROSOFT.REDUCE"),
    ("SCAN", "COM.MICROSOFT.SCAN"),
    ("SEQUENCE", "COM.MICROSOFT.SEQUENCE"),
    ("SORT", "COM.MICROSOFT.SORT"),
    ("SORTBY", "COM.MICROSOFT.SORTBY"),
    ("STDEV.P", "COM.MICROSOFT.STDEV.P"),
    ("STDEV.S", "COM.MICROSOFT.STDEV.S"),
    ("SWITCH", "COM.MICROSOFT.SWITCH"),
    ("T.DIST", "COM.MICROSOFT.T.DIST"),
    ("T.DIST.2T", "COM.MICROSOFT.T.DIST.2T"),
    ("T.DIST.RT", "COM.MICROSOFT.T.DIST.RT"),
    ("T.INV", "COM.MICROSOFT.T.INV"),
    ("T.INV.2T", "COM.MICROSOFT.T.INV.2T"),
    ("T.TEST", "COM.MICROSOFT.T.TEST"),
    ("TAKE", "COM.MICROSOFT.TAKE"),
    ("TDIST", "LEGACY.TDIST"),
    ("TEXTAFTER", "COM.MICROSOFT.TEXTAFTER"),
    ("TEXTBEFORE", "COM.MICROSOFT.TEXTBEFORE"),
    ("TEXTJOIN", "COM.MICROSOFT.TEXTJOIN"),
    ("TEXTSPLIT", "COM.MICROSOFT.TEXTSPLIT"),
    ("TOCOL", "COM.MICROSOFT.TOCOL"),
    ("TOROW", "COM.MICROSOFT.TOROW"),
    ("UNIQUE", "COM.MICROSOFT.UNIQUE"),
    ("VALUETOTEXT", "COM.MICROSOFT.VALUETOTEXT"),
    ("VAR.P", "COM.MICROSOFT.VAR.P"),
    ("VAR.S", "COM.MICROSOFT.VAR.S"),
    ("VSTACK", "COM.MICROSOFT.VSTACK"),
    ("WEBSERVICE", "COM.MICROSOFT.WEBSERVICE"),
    ("WEIBULL.DIST", "COM.MICROSOFT.WEIBULL.DIST"),
    ("WRAPCOLS", "COM.MICROSOFT.WRAPCOLS"),
    ("WRAPROWS", "COM.MICROSOFT.WRAPROWS"),
    ("XLOOKUP", "COM.MICROSOFT.XLOOKUP"),
    ("XMATCH", "COM.MICROSOFT.XMATCH"),
    ("Z.TEST", "COM.MICROSOFT.Z.TEST"),
];

/// The OpenFormula name of an Excel function.
#[must_use]
pub fn odf_function_name(excel: &str) -> String {
    let up = excel.to_ascii_uppercase();
    let up = up.strip_prefix("_XLFN.").unwrap_or(&up);
    let up = up.strip_prefix("_XLWS.").unwrap_or(up);
    FUNCTIONS
        .iter()
        .find(|(x, _)| *x == up)
        .map_or_else(|| up.to_owned(), |(_, o)| (*o).to_owned())
}

/// The Excel name of an OpenFormula function.
#[must_use]
pub fn excel_function_name(odf: &str) -> String {
    let up = odf.to_ascii_uppercase();
    if let Some((x, _)) = FUNCTIONS.iter().find(|(_, o)| *o == up) {
        return (*x).to_owned();
    }
    let up = up.strip_prefix("_XLFN.").unwrap_or(&up);
    let up = up.strip_prefix("_XLWS.").unwrap_or(up);
    let up = up.strip_prefix("COM.MICROSOFT.").unwrap_or(up);
    let up = up.strip_prefix("LEGACY.").unwrap_or(up);
    up.to_owned()
}

// ------------------------------------------------------------------ references

/// One end of a bracketed reference: `$Sheet.$A$1`, `.A`, `.3`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct RefPart {
    sheet: Option<String>,
    col: Option<(u32, bool)>,
    row: Option<(u32, bool)>,
    error: bool,
}

/// Splits on `sep` outside single quotes.
fn split_unquoted(s: &str, sep: char) -> Vec<&str> {
    let mut out = Vec::new();
    let mut quote = false;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        if c == '\'' {
            quote = !quote;
        } else if c == sep && !quote {
            out.push(&s[start..i]);
            start = i + c.len_utf8();
        }
    }
    out.push(&s[start..]);
    out
}

fn unquote_sheet(s: &str) -> String {
    let s = s.trim().trim_start_matches('$');
    match s.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')) {
        Some(inner) => inner.replace("''", "'"),
        None => s.to_owned(),
    }
}

fn parse_part(s: &str) -> Option<RefPart> {
    let s = s.trim();
    // The address follows the last `.` outside quotes.
    let mut quote = false;
    let mut dot = None;
    for (i, c) in s.char_indices() {
        if c == '\'' {
            quote = !quote;
        } else if c == '.' && !quote {
            dot = Some(i);
        }
    }
    let (sheet, addr) = match dot {
        Some(i) => {
            let sh = &s[..i];
            (
                if sh.trim_start_matches('$').is_empty() {
                    None
                } else {
                    Some(unquote_sheet(sh))
                },
                &s[i + 1..],
            )
        }
        None => (None, s),
    };
    let mut part = RefPart {
        sheet,
        ..RefPart::default()
    };
    if addr.contains("#REF!") || addr.is_empty() && part.sheet.is_none() {
        part.error = true;
        return Some(part);
    }
    let b = addr.as_bytes();
    let mut i = 0;
    let col_abs = b.first() == Some(&b'$');
    if col_abs {
        i += 1;
    }
    let ls = i;
    while i < b.len() && b[i].is_ascii_alphabetic() {
        i += 1;
    }
    if i > ls {
        part.col = Some((letters_to_column(&addr[ls..i])?, col_abs));
    } else if col_abs {
        // `$3`: an absolute row.
        i = ls - 1;
    }
    let row_abs = b.get(i) == Some(&b'$');
    if row_abs {
        i += 1;
    }
    let ds = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    if i > ds {
        let r: u32 = addr[ds..i].parse().ok()?;
        part.row = Some((r.checked_sub(1)?, row_abs));
    }
    if i != b.len() || (part.col.is_none() && part.row.is_none()) {
        return None;
    }
    Some(part)
}

fn excel_sheet(name: &str) -> String {
    if sheet_needs_quotes(name) {
        quote_sheet(name)
    } else {
        name.to_owned()
    }
}

fn part_a1(p: &RefPart) -> String {
    let mut s = String::new();
    if let Some((c, abs)) = p.col {
        if abs {
            s.push('$');
        }
        s.push_str(&column_to_letters(c));
    }
    if let Some((r, abs)) = p.row {
        if abs {
            s.push('$');
        }
        s.push_str(&(r + 1).to_string());
    }
    s
}

/// A bracketed OpenFormula reference's inside (`$Data.A1:.B2`) as Excel text (`Data!A1:B2`).
fn bracket_to_excel(inner: &str) -> String {
    let (source, rest) = match inner.find('#') {
        Some(i) if inner[..i].starts_with('\'') && !inner[i..].starts_with("#REF") => {
            (Some(unquote_sheet(&inner[..i])), &inner[i + 1..])
        }
        _ => (None, inner),
    };
    let parts: Vec<Option<RefPart>> = split_unquoted(rest, ':')
        .into_iter()
        .map(parse_part)
        .collect();
    if parts.is_empty()
        || parts.len() > 2
        || parts.iter().any(|p| p.as_ref().is_none_or(|p| p.error))
    {
        return "#REF!".to_owned();
    }
    let a = parts[0].clone().unwrap_or_default();
    let b = parts.get(1).cloned().flatten();
    let book = source
        .map(|s| {
            let name = s.rsplit(['/', '\\']).next().unwrap_or(&s).to_owned();
            format!("[{name}]")
        })
        .unwrap_or_default();
    let prefix = match (&a.sheet, b.as_ref().and_then(|b| b.sheet.clone())) {
        (Some(s1), Some(s2)) if *s1 != s2 => {
            let both = format!("{book}{s1}:{s2}");
            if sheet_needs_quotes(s1) || sheet_needs_quotes(&s2) || !book.is_empty() {
                format!("{}!", quote_sheet(&both))
            } else {
                format!("{both}!")
            }
        }
        (Some(s1), _) => {
            if book.is_empty() {
                format!("{}!", excel_sheet(s1))
            } else {
                format!("{}!", quote_sheet(&format!("{book}{s1}")))
            }
        }
        (None, _) => String::new(),
    };
    match b {
        Some(b) => format!("{prefix}{}:{}", part_a1(&a), part_a1(&b)),
        None => format!("{prefix}{}", part_a1(&a)),
    }
}

fn needs_odf_quotes(name: &str) -> bool {
    name.is_empty()
        || name.starts_with(|c: char| c.is_ascii_digit())
        || !name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

fn odf_sheet(name: &str) -> String {
    if needs_odf_quotes(name) {
        format!("'{}'", name.replace('\'', "''"))
    } else {
        name.to_owned()
    }
}

fn cell_text(col: Option<(u32, bool)>, row: Option<(u32, bool)>) -> String {
    part_a1(&RefPart {
        col,
        row,
        ..RefPart::default()
    })
}

/// An Excel reference as an OpenFormula bracketed reference.
fn ref_to_odf(prefix: Option<&SheetPrefix>, address: &RefAddress) -> String {
    let (a, b) = match *address {
        RefAddress::Cell {
            row,
            col,
            row_abs,
            col_abs,
        } => (cell_text(Some((col, col_abs)), Some((row, row_abs))), None),
        RefAddress::Area {
            r0,
            c0,
            r1,
            c1,
            r0_abs,
            c0_abs,
            r1_abs,
            c1_abs,
        } => (
            cell_text(Some((c0, c0_abs)), Some((r0, r0_abs))),
            Some(cell_text(Some((c1, c1_abs)), Some((r1, r1_abs)))),
        ),
        RefAddress::Rows {
            r0,
            r1,
            r0_abs,
            r1_abs,
        } => (
            cell_text(None, Some((r0, r0_abs))),
            Some(cell_text(None, Some((r1, r1_abs)))),
        ),
        RefAddress::Cols {
            c0,
            c1,
            c0_abs,
            c1_abs,
        } => (
            cell_text(Some((c0, c0_abs)), None),
            Some(cell_text(Some((c1, c1_abs)), None)),
        ),
    };
    let mut out = String::from("[");
    let (s1, s2) = match prefix {
        Some(p) => {
            if let Some(wb) = &p.workbook {
                out.push_str(&format!("'{}'#", wb.replace('\'', "''")));
            }
            (Some(p.sheet.as_str()), p.sheet2.as_deref())
        }
        None => (None, None),
    };
    if let Some(s) = s1 {
        out.push('$');
        out.push_str(&odf_sheet(s));
    }
    out.push('.');
    out.push_str(&a);
    let b = match (b, s2) {
        (Some(b), _) => Some(b),
        (None, Some(_)) => Some(a.clone()),
        (None, None) => None,
    };
    if let Some(b) = b {
        out.push(':');
        if let Some(s) = s2 {
            out.push('$');
            out.push_str(&odf_sheet(s));
        }
        out.push('.');
        out.push_str(&b);
    }
    out.push(']');
    out
}

// ------------------------------------------------------------------ OpenFormula → Excel

/// Error literals, longest first where one is a prefix of another.
const ERRORS: &[&str] = &[
    "#NULL!",
    "#DIV/0!",
    "#VALUE!",
    "#REF!",
    "#NAME?",
    "#NUM!",
    "#N/A",
    "#GETTING_DATA",
    "#SPILL!",
    "#CALC!",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ctx {
    Function,
    Group,
    Array,
}

/// An OpenFormula (`of:=…`, `oooc:=…`, `msoxl:=…` or a bare `=…`) as Excel formula text without
/// the leading `=`.
#[must_use]
pub fn odf_to_excel(text: &str) -> String {
    let t = text.trim();
    let (ns, body) = match t.split_once(':') {
        Some((ns, rest)) if matches!(ns, "of" | "oooc" | "msoxl") && rest.starts_with('=') => {
            (ns, rest)
        }
        _ => ("of", t),
    };
    let body = body.strip_prefix('=').unwrap_or(body);
    if ns == "msoxl" {
        return body.to_owned();
    }
    let chars: Vec<char> = body.chars().collect();
    let mut out = String::with_capacity(body.len());
    // Per open parenthesis: its kind, where its current argument starts in `out`, and whether
    // that argument holds a union (which Excel needs in parentheses).
    let mut stack: Vec<(Ctx, usize, bool)> = Vec::new();
    let close_arg = |out: &mut String, frame: &mut (Ctx, usize, bool)| {
        if frame.0 == Ctx::Function && frame.2 {
            out.insert(frame.1, '(');
            out.push(')');
        }
        frame.2 = false;
    };
    let mut i = 0;
    let mut last_function = false;
    while i < chars.len() {
        let c = chars[i];
        let was_function = last_function;
        last_function = false;
        match c {
            '"' => {
                out.push('"');
                i += 1;
                while i < chars.len() {
                    if chars[i] == '"' {
                        if chars.get(i + 1) == Some(&'"') {
                            out.push_str("\"\"");
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    out.push(chars[i]);
                    i += 1;
                }
                out.push('"');
                i += 1;
            }
            '[' => {
                let mut j = i + 1;
                let mut quote = false;
                while j < chars.len() && (quote || chars[j] != ']') {
                    if chars[j] == '\'' {
                        quote = !quote;
                    }
                    j += 1;
                }
                let inner: String = chars[i + 1..j.min(chars.len())].iter().collect();
                out.push_str(&bracket_to_excel(&inner));
                i = j + 1;
            }
            '(' => {
                stack.push((
                    if was_function {
                        Ctx::Function
                    } else {
                        Ctx::Group
                    },
                    out.len() + 1,
                    false,
                ));
                out.push('(');
                i += 1;
            }
            ')' => {
                if let Some(mut f) = stack.pop() {
                    close_arg(&mut out, &mut f);
                }
                out.push(')');
                i += 1;
            }
            '{' => {
                stack.push((Ctx::Array, out.len() + 1, false));
                out.push('{');
                i += 1;
            }
            '}' => {
                stack.pop();
                out.push('}');
                i += 1;
            }
            ';' => {
                if let Some(f) = stack.last_mut() {
                    let mut frame = *f;
                    close_arg(&mut out, &mut frame);
                    frame.1 = out.len() + 1;
                    *f = frame;
                }
                out.push(',');
                i += 1;
            }
            '#' => {
                // Error literals (`#DIV/0!`, `#N/A`): copied whole, their `!` is no operator.
                let rest: String = chars[i..].iter().take(16).collect();
                let code = ERRORS
                    .iter()
                    .find(|e| rest.to_ascii_uppercase().starts_with(*e))
                    .copied();
                match code {
                    Some(e) => {
                        out.push_str(e);
                        i += e.chars().count();
                    }
                    None => {
                        out.push('#');
                        i += 1;
                    }
                }
            }
            '|' if stack.last().is_some_and(|f| f.0 == Ctx::Array) => {
                out.push(';');
                i += 1;
            }
            '~' => {
                if let Some(f) = stack.last_mut() {
                    f.2 = true;
                }
                out.push(',');
                i += 1;
            }
            '!' => {
                out.push(' ');
                i += 1;
            }
            c if c.is_ascii_digit()
                || (c == '.' && chars.get(i + 1).is_some_and(char::is_ascii_digit)) =>
            {
                while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
                    out.push(chars[i]);
                    i += 1;
                }
                if i < chars.len()
                    && (chars[i] == 'e' || chars[i] == 'E')
                    && chars
                        .get(i + 1)
                        .is_some_and(|n| n.is_ascii_digit() || *n == '+' || *n == '-')
                {
                    out.push('E');
                    i += 1;
                    if chars[i] == '+' || chars[i] == '-' {
                        out.push(chars[i]);
                        i += 1;
                    }
                    while i < chars.len() && chars[i].is_ascii_digit() {
                        out.push(chars[i]);
                        i += 1;
                    }
                }
            }
            c if c.is_alphabetic() || c == '_' || c == '\\' => {
                let start = i;
                while i < chars.len()
                    && (chars[i].is_alphanumeric() || matches!(chars[i], '_' | '.' | '\\'))
                {
                    i += 1;
                }
                let word: String = chars[start..i].iter().collect();
                let mut k = i;
                while k < chars.len() && chars[k] == ' ' {
                    k += 1;
                }
                if chars.get(k) == Some(&'(') {
                    let name = excel_function_name(&word);
                    // `TRUE()` and `FALSE()` are Excel's literals.
                    if (name == "TRUE" || name == "FALSE") && chars.get(k + 1) == Some(&')') {
                        out.push_str(&name);
                        i = k + 2;
                        continue;
                    }
                    out.push_str(&name);
                    i = k;
                    last_function = true;
                } else {
                    out.push_str(&word);
                }
            }
            ' ' | '\t' | '\n' | '\r' => {
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

// ------------------------------------------------------------------ Excel → OpenFormula

/// Excel formula text (with or without `=`) as an OpenFormula `of:=…` attribute value.
/// `structured` resolves a structured reference's source text (`Table1[Qty]`) to Excel range
/// text; unresolved ones are written as they are.
#[must_use]
pub fn excel_to_odf_with(text: &str, structured: &dyn Fn(&str) -> Option<String>) -> String {
    let body = text.trim();
    let body = body.strip_prefix('=').unwrap_or(body);
    let tokens = tokenize(body, &ParseOptions::default(), 0);
    let mut out = String::from("of:=");
    let mut stack: Vec<Ctx> = Vec::new();
    let significant: Vec<usize> = tokens
        .iter()
        .enumerate()
        .filter(|(_, t)| t.kind != TokenKind::Whitespace)
        .map(|(i, _)| i)
        .collect();
    let ends_operand = |k: &TokenKind| {
        matches!(
            k,
            TokenKind::Number(_)
                | TokenKind::Str(_)
                | TokenKind::Bool(_)
                | TokenKind::Error { .. }
                | TokenKind::Ref { .. }
                | TokenKind::Name { .. }
                | TokenKind::Structured(_)
                | TokenKind::RParen
        )
    };
    let starts_operand = |k: &TokenKind| {
        matches!(
            k,
            TokenKind::Ref { .. }
                | TokenKind::Name { .. }
                | TokenKind::Structured(_)
                | TokenKind::LParen
                | TokenKind::Function(_)
        )
    };
    let mut prev_function = false;
    for (idx, t) in tokens.iter().enumerate() {
        let src = body
            .get(t.start as usize..t.end as usize)
            .unwrap_or_default();
        let was_function = prev_function;
        prev_function = false;
        match &t.kind {
            TokenKind::Number(_) => out.push_str(src),
            TokenKind::Str(s) => {
                out.push('"');
                out.push_str(&s.replace('"', "\"\""));
                out.push('"');
            }
            TokenKind::Bool(b) => out.push_str(if *b { "TRUE()" } else { "FALSE()" }),
            TokenKind::Error { code, .. } => out.push_str(code.code()),
            TokenKind::Ref { prefix, address } => {
                out.push_str(&ref_to_odf(prefix.as_ref(), address))
            }
            TokenKind::Name { name, prefix } => {
                if let Some(p) = prefix {
                    // A sheet-local name: OpenFormula has no qualifier for it.
                    let _ = p;
                }
                out.push_str(name);
            }
            TokenKind::Function(name) => {
                out.push_str(&odf_function_name(name));
                prev_function = true;
            }
            TokenKind::Structured(_) => match structured(src) {
                Some(range) => {
                    let inner = excel_to_odf_with(&range, &|_| None);
                    out.push_str(inner.trim_start_matches("of:="));
                }
                None => out.push_str(src),
            },
            TokenKind::Operator(op) => match op {
                OperatorToken::At | OperatorToken::Hash => {}
                other => out.push_str(other.as_str()),
            },
            TokenKind::LParen => {
                stack.push(if was_function {
                    Ctx::Function
                } else {
                    Ctx::Group
                });
                out.push('(');
            }
            TokenKind::RParen => {
                stack.pop();
                out.push(')');
            }
            TokenKind::LBrace => {
                stack.push(Ctx::Array);
                out.push('{');
            }
            TokenKind::RBrace => {
                stack.pop();
                out.push('}');
            }
            TokenKind::Comma => out.push(match stack.last() {
                Some(Ctx::Function | Ctx::Array) => ';',
                _ => '~',
            }),
            TokenKind::Semicolon => out.push(match stack.last() {
                Some(Ctx::Array) => '|',
                _ => ';',
            }),
            TokenKind::Whitespace => {
                let pos = significant.partition_point(|&s| s < idx);
                let prev = pos.checked_sub(1).map(|p| &tokens[significant[p]].kind);
                let next = significant.get(pos).map(|&n| &tokens[n].kind);
                if prev.is_some_and(ends_operand) && next.is_some_and(starts_operand) {
                    out.push('!');
                }
            }
            TokenKind::Invalid(_) => out.push_str(src),
        }
    }
    out
}

/// [`excel_to_odf_with`] without structured-reference resolution.
#[must_use]
pub fn excel_to_odf(text: &str) -> String {
    excel_to_odf_with(text, &|_| None)
}

// ------------------------------------------------------------------ range addresses

/// A `table:cell-range-address` list (`Sheet1.A1:Sheet1.B2 $'My sheet'.$C$3`) as sheet names
/// and ranges; parts that are not addresses are skipped.
#[must_use]
pub fn parse_range_list(s: &str) -> Vec<(Option<String>, Range)> {
    let mut out = Vec::new();
    for item in split_unquoted(s.trim(), ' ') {
        let item = item.trim();
        if item.is_empty() {
            continue;
        }
        let parts: Vec<Option<RefPart>> = split_unquoted(item, ':')
            .into_iter()
            .map(parse_part)
            .collect();
        let Some(Some(a)) = parts.first().cloned() else {
            continue;
        };
        if a.error {
            continue;
        }
        let b = parts.get(1).cloned().flatten().unwrap_or_else(|| a.clone());
        let (r0, r1) = match (a.row, b.row) {
            (Some((r0, _)), Some((r1, _))) => (r0, r1),
            _ => (0, kiln_sheet::MAX_ROWS - 1),
        };
        let (c0, c1) = match (a.col, b.col) {
            (Some((c0, _)), Some((c1, _))) => (c0, c1),
            _ => (0, kiln_sheet::MAX_COLS - 1),
        };
        if r0.max(r1) >= kiln_sheet::MAX_ROWS || c0.max(c1) >= kiln_sheet::MAX_COLS {
            continue;
        }
        out.push((a.sheet, Range::new(r0, c0, r1, c1)));
    }
    out
}

/// A range as a `table:cell-range-address` (`$Sheet1.$A$1:.$B$2`, absolute).
#[must_use]
pub fn range_address(sheet: &str, r: &Range) -> String {
    let a = cell_text(Some((r.c0, true)), Some((r.r0, true)));
    let s = format!("${}", odf_sheet(sheet));
    if r.is_single_cell() {
        format!("{s}.{a}")
    } else {
        let b = cell_text(Some((r.c1, true)), Some((r.r1, true)));
        format!("{s}.{a}:.{b}")
    }
}

/// A cell as `table:base-cell-address` / `table:target-cell-address` (`$Sheet1.$A$1`).
#[must_use]
pub fn cell_address(sheet: &str, row: u32, col: u32) -> String {
    range_address(sheet, &Range::cell(row, col))
}

/// An Excel reference (`Sheet1!$A$1:$B$2`) of a defined name as a cell range address, when the
/// formula is exactly one reference.
#[must_use]
pub fn excel_ref_to_range_address(text: &str) -> Option<String> {
    let body = text.trim().trim_start_matches('=');
    let tokens: Vec<_> = tokenize(body, &ParseOptions::default(), 0)
        .into_iter()
        .filter(|t| t.kind != TokenKind::Whitespace)
        .collect();
    let [t] = tokens.as_slice() else {
        return None;
    };
    let TokenKind::Ref { prefix, address } = &t.kind else {
        return None;
    };
    let p = prefix.as_ref()?;
    if p.workbook.is_some() || p.sheet2.is_some() {
        return None;
    }
    let bracket = ref_to_odf(Some(p), address);
    Some(
        bracket
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_owned(),
    )
}

/// A cell range address (`$Sheet1.$A$1:.$B$2`) as Excel reference text (`Sheet1!$A$1:$B$2`).
#[must_use]
pub fn range_address_to_excel(s: &str) -> String {
    split_unquoted(s.trim(), ' ')
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .map(bracket_to_excel)
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_translate() {
        assert_eq!(odf_to_excel("of:=SUM([.A1:.B2])"), "SUM(A1:B2)");
        assert_eq!(odf_to_excel("of:=[$Data.$A$1]+[.B$2]"), "Data!$A$1+B$2");
        assert_eq!(
            odf_to_excel("of:=SUM([$'My sheet'.A1:.A20];[$S1.A1:$S3.B2])"),
            "SUM('My sheet'!A1:A20,'S1:S3'!A1:B2)"
        );
        assert_eq!(odf_to_excel("of:=SUM([.A:.A];[.$3:.$5])"), "SUM(A:A,$3:$5)");
        assert_eq!(odf_to_excel("of:=[.#REF!]"), "#REF!");
        assert_eq!(odf_to_excel("of:=SUM([.A1]~[.B1];2)"), "SUM((A1,B1),2)");
        assert_eq!(
            odf_to_excel("of:=SUM([.A1:.B5]![.B2:.C3])"),
            "SUM(A1:B5 B2:C3)"
        );
    }

    #[test]
    fn functions_literals_and_arrays_translate() {
        assert_eq!(
            odf_to_excel("of:=COM.MICROSOFT.CEILING(4.3;1)"),
            "CEILING(4.3,1)"
        );
        assert_eq!(
            odf_to_excel("of:=IF(TRUE();\"a\"\"b\";FALSE())"),
            "IF(TRUE,\"a\"\"b\",FALSE)"
        );
        assert_eq!(odf_to_excel("of:=SUM({1;2|3;4})"), "SUM({1,2;3,4})");
        assert_eq!(
            odf_to_excel("of:=aggregate(9;6;[.A1:.A3])"),
            "AGGREGATE(9,6,A1:A3)"
        );
        assert_eq!(
            odf_to_excel("of:=_xlfn.workday.intl([.A1];5)"),
            "WORKDAY.INTL(A1,5)"
        );
        assert_eq!(
            odf_to_excel("of:=LEGACY.NORMSDIST(1.5E-3)"),
            "NORMSDIST(1.5E-3)"
        );
        assert_eq!(odf_to_excel("oooc:=[.A1]*2"), "A1*2");
        assert_eq!(odf_to_excel("msoxl:=A1*2"), "A1*2");
        assert_eq!(odf_to_excel("of:=FORMULA([.A1])"), "FORMULATEXT(A1)");
        assert_eq!(
            odf_to_excel("of:=IFERROR(1/0;#DIV/0!)"),
            "IFERROR(1/0,#DIV/0!)"
        );
    }

    #[test]
    fn excel_formulas_translate() {
        assert_eq!(
            excel_to_odf("SUM(A1:B2,Data!C3)"),
            "of:=SUM([.A1:.B2];[$Data.C3])"
        );
        assert_eq!(excel_to_odf("='My sheet'!$A$1"), "of:=[$'My sheet'.$A$1]");
        assert_eq!(
            excel_to_odf("CEILING(4.3,1)"),
            "of:=COM.MICROSOFT.CEILING(4.3;1)"
        );
        // ODF's own CEILING and FLOOR keep their names through Kiln.
        assert_eq!(odf_to_excel("of:=CEILING([.A1];2)"), "CEILING.MATH(A1,2)");
        assert_eq!(excel_to_odf("CEILING.MATH(A1,2)"), "of:=CEILING([.A1];2)");
        assert_eq!(odf_to_excel("of:=FLOOR(-4.5;1;1)"), "FLOOR.MATH(-4.5,1,1)");
        assert_eq!(excel_to_odf("FLOOR.MATH(-4.5,1,1)"), "of:=FLOOR(-4.5;1;1)");
        assert_eq!(
            excel_to_odf("IF(TRUE,1,{1,2;3,4})"),
            "of:=IF(TRUE();1;{1;2|3;4})"
        );
        assert_eq!(excel_to_odf("SUM((A1,B1))"), "of:=SUM(([.A1]~[.B1]))");
        assert_eq!(
            excel_to_odf("SUM(A1:B5 B2:C3)"),
            "of:=SUM([.A1:.B5]![.B2:.C3])"
        );
        assert_eq!(excel_to_odf("SUM(A:A, 3:5)"), "of:=SUM([.A:.A];[.3:.5])");
        assert_eq!(excel_to_odf("S1:S3!A1"), "of:=[$S1.A1:$S3.A1]");
        assert_eq!(excel_to_odf("\"a\"\"b\"&A1"), "of:=\"a\"\"b\"&[.A1]");
    }

    #[test]
    fn formulas_round_trip() {
        for f in [
            "SUM(A1:B2,Data!C3)",
            "IF(A1>0,\"pos\",IF(A1<0,\"neg\",\"zero\"))",
            "VLOOKUP(A2,'Price list'!$A$1:$C$100,3,FALSE)",
            "SUMPRODUCT((A1:A5>2)*(B1:B5))",
            "XLOOKUP(A1,B1:B9,C1:C9)",
            "-A1^2%",
            "A1&\" \"&B1",
            "SUM({1,2;3,4})",
            "SUM((A1,B1))",
            "NORM.S.DIST(1,TRUE)",
        ] {
            assert_eq!(odf_to_excel(&excel_to_odf(f)), f, "{f}");
        }
    }

    #[test]
    fn range_addresses() {
        let v = parse_range_list("$Sheet1.$A$1:.$B$10 'My sheet'.C3");
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], (Some("Sheet1".to_owned()), Range::new(0, 0, 9, 1)));
        assert_eq!(v[1], (Some("My sheet".to_owned()), Range::cell(2, 2)));
        assert_eq!(
            range_address("Sheet 1", &Range::new(0, 0, 9, 1)),
            "$'Sheet 1'.$A$1:.$B$10"
        );
        assert_eq!(cell_address("Data", 0, 0), "$Data.$A$1");
        assert_eq!(
            excel_ref_to_range_address("Sheet1!$A$1:$B$2").as_deref(),
            Some("$Sheet1.$A$1:.$B$2")
        );
        assert_eq!(excel_ref_to_range_address("Sheet1!$A$1+1"), None);
        assert_eq!(
            range_address_to_excel("$Sheet1.$A$1:.$B$2"),
            "Sheet1!$A$1:$B$2"
        );
    }

    #[test]
    fn hostile_input_does_not_panic() {
        for s in [
            "of:=[",
            "of:=[.",
            "of:=\"",
            "of:=((((",
            "of:=)))",
            "of:=[.A1:.]",
            "of:={;|}",
            "of:=~~",
            "of:=[.ZZZZZZZ99999999]",
            "of:=1e",
            "of:=1e+",
        ] {
            let _ = odf_to_excel(s);
        }
        for s in ["(", ")", "{", "\"", "A1:", "SUM(,,,)", "'a'!", "1e+"] {
            let _ = excel_to_odf(s);
        }
    }
}
