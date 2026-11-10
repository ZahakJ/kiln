// SPDX-License-Identifier: AGPL-3.0-or-later
//! The formula tokenizer.
//!
//! It never fails: malformed input produces [`TokenKind::Invalid`] tokens so the formula bar can
//! still colour an incomplete formula while the user types. The parser turns invalid tokens into
//! syntax errors.

use super::ast::{RefAddress, SheetPrefix, StructuredRef, StructuredSpecial};
use crate::refs::{MAX_COLS, MAX_ROWS, letters_to_column};
use crate::values::ErrorValue;

/// Reference style of the formula text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RefStyle {
    /// `A1`, `$B$2`.
    #[default]
    A1,
    /// `R1C1`, `R[-1]C`; relative parts resolve against the anchor cell.
    R1C1,
}

/// Options for tokenizing and parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ParseOptions {
    /// Reference style.
    pub style: RefStyle,
    /// The cell the formula belongs to, `(row, col)`; needed for relative R1C1 references.
    pub anchor: (u32, u32),
}

impl ParseOptions {
    /// R1C1 notation anchored at `(row, col)`.
    pub fn r1c1(row: u32, col: u32) -> ParseOptions {
        ParseOptions {
            style: RefStyle::R1C1,
            anchor: (row, col),
        }
    }
}

/// Operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperatorToken {
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `^`
    Caret,
    /// `&`
    Amp,
    /// `=`
    Eq,
    /// `<>`
    Ne,
    /// `<`
    Lt,
    /// `>`
    Gt,
    /// `<=`
    Le,
    /// `>=`
    Ge,
    /// `%`
    Percent,
    /// `@`
    At,
    /// `:`
    Colon,
    /// `#` (spill)
    Hash,
}

impl OperatorToken {
    /// The operator as written.
    pub const fn as_str(self) -> &'static str {
        match self {
            OperatorToken::Plus => "+",
            OperatorToken::Minus => "-",
            OperatorToken::Star => "*",
            OperatorToken::Slash => "/",
            OperatorToken::Caret => "^",
            OperatorToken::Amp => "&",
            OperatorToken::Eq => "=",
            OperatorToken::Ne => "<>",
            OperatorToken::Lt => "<",
            OperatorToken::Gt => ">",
            OperatorToken::Le => "<=",
            OperatorToken::Ge => ">=",
            OperatorToken::Percent => "%",
            OperatorToken::At => "@",
            OperatorToken::Colon => ":",
            OperatorToken::Hash => "#",
        }
    }
}

/// What a token is, with its decoded payload.
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    /// A number literal.
    Number(f64),
    /// A text literal (quotes removed, `""` unescaped).
    Str(String),
    /// TRUE / FALSE.
    Bool(bool),
    /// An error literal, or `Sheet1!#REF!`.
    Error {
        /// The error.
        code: ErrorValue,
        /// Sheet qualifier.
        prefix: Option<SheetPrefix>,
    },
    /// A reference.
    Ref {
        /// Sheet qualifier.
        prefix: Option<SheetPrefix>,
        /// The address.
        address: RefAddress,
    },
    /// A name (possibly sheet-qualified).
    Name {
        /// The name as written.
        name: String,
        /// Sheet qualifier.
        prefix: Option<SheetPrefix>,
    },
    /// A function name directly followed by `(` (upper-cased).
    Function(String),
    /// A structured reference.
    Structured(Box<StructuredRef>),
    /// An operator.
    Operator(OperatorToken),
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `,`
    Comma,
    /// `;`
    Semicolon,
    /// Spaces, tabs, line breaks.
    Whitespace,
    /// Something that cannot be read; the message says why.
    Invalid(String),
}

impl TokenKind {
    fn ends_operand(&self) -> bool {
        matches!(
            self,
            TokenKind::Number(_)
                | TokenKind::Str(_)
                | TokenKind::Bool(_)
                | TokenKind::Error { .. }
                | TokenKind::Ref { .. }
                | TokenKind::Name { .. }
                | TokenKind::Structured(_)
                | TokenKind::RParen
                | TokenKind::RBrace
        )
    }
}

/// A token with its byte span (already shifted by the tokenizer's offset).
#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    /// What it is.
    pub kind: TokenKind,
    /// First byte.
    pub start: u32,
    /// One past the last byte.
    pub end: u32,
}

/// True for characters that may start a name: letters, `_`, `\` and anything from U+00A1.
pub fn is_name_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '\\' || c as u32 >= 0xA1
}

/// True for characters that may continue a name.
pub fn is_name_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '.' || c == '?'
}

fn is_sheet_char(c: char) -> bool {
    is_name_start(c) || c.is_ascii_digit() || c == '.'
}

fn is_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{a0}')
}

fn char_at(s: &str, i: usize) -> Option<char> {
    s.get(i..).and_then(|t| t.chars().next())
}

/// Characters that may not directly follow a reference.
fn blocks_ref_end(s: &str, i: usize) -> bool {
    char_at(s, i).is_some_and(|c| is_name_char(c) || c == '(' || c == '[' || c == '!')
}

fn byte_at(s: &str, i: usize) -> Option<u8> {
    s.as_bytes().get(i).copied()
}

/// Scans a sheet prefix (`Sheet1!`, `'a b'!`, `S1:S3!`, `[1]S!`, `'[b.xlsx]S'!`) at `i`.
fn scan_prefix(s: &str, i: usize) -> Option<(SheetPrefix, usize)> {
    let b = s.as_bytes();
    if byte_at(s, i) == Some(b'\'') {
        let mut j = i + 1;
        let mut content = String::new();
        loop {
            let c = char_at(s, j)?;
            if c == '\'' {
                if byte_at(s, j + 1) == Some(b'\'') {
                    content.push('\'');
                    j += 2;
                    continue;
                }
                break;
            }
            content.push(c);
            j += c.len_utf8();
        }
        if byte_at(s, j + 1) != Some(b'!') || content.is_empty() {
            return None;
        }
        let mut workbook = None;
        if content.starts_with('[')
            && let Some(close) = content.find(']')
            && close > 1
        {
            workbook = Some(content[1..close].to_owned());
            content = content[close + 1..].to_owned();
        }
        let prefix = match content.find(':') {
            Some(colon) if colon > 0 => SheetPrefix {
                workbook,
                sheet: content[..colon].to_owned(),
                sheet2: Some(content[colon + 1..].to_owned()),
            },
            _ => SheetPrefix {
                workbook,
                sheet: content,
                sheet2: None,
            },
        };
        if prefix.sheet.is_empty() || prefix.sheet2.as_deref() == Some("") {
            return None;
        }
        return Some((prefix, j + 2));
    }
    let mut j = i;
    let mut workbook = None;
    if byte_at(s, i) == Some(b'[') {
        let close = i + s[i..].find(']')?;
        let book = &s[i + 1..close];
        if book.is_empty() || book.contains('[') {
            return None;
        }
        workbook = Some(book.to_owned());
        j = close + 1;
    }
    let read_name = |k: usize| -> usize {
        match char_at(s, k) {
            Some(c) if is_name_start(c) => {
                let mut e = k + c.len_utf8();
                while let Some(c) = char_at(s, e) {
                    if !is_sheet_char(c) {
                        break;
                    }
                    e += c.len_utf8();
                }
                e
            }
            _ => k,
        }
    };
    let e1 = read_name(j);
    if e1 == j {
        return None;
    }
    let mut sheet2 = None;
    let mut end = e1;
    if b.get(e1) == Some(&b':') {
        let e2 = read_name(e1 + 1);
        if e2 > e1 + 1 && b.get(e2) == Some(&b'!') {
            sheet2 = Some(s[e1 + 1..e2].to_owned());
            end = e2;
        }
    }
    if b.get(end) != Some(&b'!') {
        return None;
    }
    Some((
        SheetPrefix {
            workbook,
            sheet: s[j..e1].to_owned(),
            sheet2,
        },
        end + 1,
    ))
}

struct CellMatch {
    row: u32,
    col: u32,
    row_abs: bool,
    col_abs: bool,
    end: usize,
}

fn run(s: &str, i: usize, pred: impl Fn(u8) -> bool) -> usize {
    let b = s.as_bytes();
    let mut j = i;
    while j < b.len() && pred(b[j]) {
        j += 1;
    }
    j
}

/// `\$?([A-Za-z]{1,3})\$?(\d{1,7})` at `i`.
fn match_a1_cell(s: &str, i: usize) -> Option<CellMatch> {
    let mut j = i;
    let col_abs = byte_at(s, j) == Some(b'$');
    if col_abs {
        j += 1;
    }
    let le = run(s, j, |b| b.is_ascii_alphabetic());
    if !(1..=3).contains(&(le - j)) {
        return None;
    }
    let col = letters_to_column(&s[j..le])?;
    j = le;
    let row_abs = byte_at(s, j) == Some(b'$');
    if row_abs {
        j += 1;
    }
    let de = run(s, j, |b| b.is_ascii_digit());
    if !(1..=7).contains(&(de - j)) {
        return None;
    }
    let row: u32 = s[j..de].parse().ok()?;
    if row == 0 || row > MAX_ROWS || col >= MAX_COLS {
        return None;
    }
    Some(CellMatch {
        row: row - 1,
        col,
        row_abs,
        col_abs,
        end: de,
    })
}

/// Matches an A1 address (cell, area, column range or row range) at `i`.
pub fn match_a1_address(s: &str, i: usize) -> Option<(RefAddress, usize)> {
    if let Some(a) = match_a1_cell(s, i) {
        if byte_at(s, a.end) == Some(b':')
            && let Some(b) = match_a1_cell(s, a.end + 1)
            && !blocks_ref_end(s, b.end)
        {
            let addr = RefAddress::area(
                a.row, a.col, a.row_abs, a.col_abs, b.row, b.col, b.row_abs, b.col_abs,
            );
            return Some((addr, b.end));
        }
        if !blocks_ref_end(s, a.end) {
            let addr = RefAddress::Cell {
                row: a.row,
                col: a.col,
                row_abs: a.row_abs,
                col_abs: a.col_abs,
            };
            return Some((addr, a.end));
        }
        return None;
    }
    // Whole columns: (\$?)([A-Za-z]{1,3}):(\$?)([A-Za-z]{1,3})
    let part = |k: usize, letters: bool| -> Option<(u32, bool, usize)> {
        let abs = byte_at(s, k) == Some(b'$');
        let k = if abs { k + 1 } else { k };
        let e = if letters {
            run(s, k, |b| b.is_ascii_alphabetic())
        } else {
            run(s, k, |b| b.is_ascii_digit())
        };
        let max = if letters { 3 } else { 7 };
        if !(1..=max).contains(&(e - k)) {
            return None;
        }
        let v = if letters {
            letters_to_column(&s[k..e])?
        } else {
            s[k..e].parse::<u32>().ok()?.checked_sub(1)?
        };
        Some((v, abs, e))
    };
    for letters in [true, false] {
        let Some((v0, a0, e0)) = part(i, letters) else {
            continue;
        };
        if byte_at(s, e0) != Some(b':') {
            continue;
        }
        let Some((v1, a1, e1)) = part(e0 + 1, letters) else {
            continue;
        };
        let max = if letters { MAX_COLS } else { MAX_ROWS };
        if v0 >= max || v1 >= max || blocks_ref_end(s, e1) {
            continue;
        }
        let ((lo, lo_abs), (hi, hi_abs)) = if v0 <= v1 {
            ((v0, a0), (v1, a1))
        } else {
            ((v1, a1), (v0, a0))
        };
        let addr = if letters {
            RefAddress::Cols {
                c0: lo,
                c1: hi,
                c0_abs: lo_abs,
                c1_abs: hi_abs,
            }
        } else {
            RefAddress::Rows {
                r0: lo,
                r1: hi,
                r0_abs: lo_abs,
                r1_abs: hi_abs,
            }
        };
        return Some((addr, e1));
    }
    None
}

#[derive(Clone, Copy)]
struct R1C1Part {
    abs: bool,
    /// Absolute 1-based index or relative offset.
    n: i64,
    end: usize,
}

/// `([RrCc])(?:\[([+-]?\d+)\]|(\d+))?` with the given letter at `i`.
fn match_r1c1_part(s: &str, i: usize, letter: u8) -> Option<R1C1Part> {
    if byte_at(s, i)?.to_ascii_uppercase() != letter {
        return None;
    }
    let j = i + 1;
    if byte_at(s, j) == Some(b'[') {
        let mut k = j + 1;
        if matches!(byte_at(s, k), Some(b'+' | b'-')) {
            k += 1;
        }
        let de = run(s, k, |b| b.is_ascii_digit());
        if de > k && byte_at(s, de) == Some(b']') {
            let n = s[j + 1..de].parse::<i64>().unwrap_or(i64::MAX);
            return Some(R1C1Part {
                abs: false,
                n,
                end: de + 1,
            });
        }
    }
    let de = run(s, j, |b| b.is_ascii_digit());
    if de > j {
        let n = s[j..de].parse::<i64>().unwrap_or(i64::MAX);
        return Some(R1C1Part {
            abs: true,
            n,
            end: de,
        });
    }
    Some(R1C1Part {
        abs: false,
        n: 0,
        end: j,
    })
}

/// An R1C1 match: an address, or an address outside the sheet (`#REF!`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum R1C1Match {
    /// A valid address.
    Address(RefAddress),
    /// A reference that resolves outside the sheet.
    OutOfSheet,
}

fn resolve_part(p: R1C1Part, base: u32, max: u32) -> Option<u32> {
    let v = if p.abs {
        p.n.checked_sub(1)?
    } else {
        i64::from(base).checked_add(p.n)?
    };
    (0..i64::from(max)).contains(&v).then_some(v as u32)
}

/// Matches an R1C1 address at `i`, resolving relative parts against `anchor` `(row, col)`.
pub fn match_r1c1_address(s: &str, i: usize, anchor: (u32, u32)) -> Option<(R1C1Match, usize)> {
    let (arow, acol) = anchor;
    if let Some(row_part) = match_r1c1_part(s, i, b'R') {
        if let Some(col_part) = match_r1c1_part(s, row_part.end, b'C') {
            let r = resolve_part(row_part, arow, MAX_ROWS);
            let c = resolve_part(col_part, acol, MAX_COLS);
            if byte_at(s, col_part.end) == Some(b':')
                && let Some(r2p) = match_r1c1_part(s, col_part.end + 1, b'R')
                && let Some(c2p) = match_r1c1_part(s, r2p.end, b'C')
                && !blocks_ref_end(s, c2p.end)
            {
                let r2 = resolve_part(r2p, arow, MAX_ROWS);
                let c2 = resolve_part(c2p, acol, MAX_COLS);
                return Some(match (r, c, r2, c2) {
                    (Some(r), Some(c), Some(r2), Some(c2)) => (
                        R1C1Match::Address(RefAddress::area(
                            r,
                            c,
                            row_part.abs,
                            col_part.abs,
                            r2,
                            c2,
                            r2p.abs,
                            c2p.abs,
                        )),
                        c2p.end,
                    ),
                    _ => (R1C1Match::OutOfSheet, c2p.end),
                });
            }
            if blocks_ref_end(s, col_part.end) {
                return None;
            }
            return Some(match (r, c) {
                (Some(row), Some(col)) => (
                    R1C1Match::Address(RefAddress::Cell {
                        row,
                        col,
                        row_abs: row_part.abs,
                        col_abs: col_part.abs,
                    }),
                    col_part.end,
                ),
                _ => (R1C1Match::OutOfSheet, col_part.end),
            });
        }
        // Whole rows: R1, R[1], R, R1:R3
        let mut end = row_part.end;
        let mut second = row_part;
        if byte_at(s, end) == Some(b':')
            && let Some(p2) = match_r1c1_part(s, end + 1, b'R')
            && match_r1c1_part(s, p2.end, b'C').is_none()
        {
            second = p2;
            end = p2.end;
        }
        if blocks_ref_end(s, end) {
            return None;
        }
        let a = resolve_part(row_part, arow, MAX_ROWS);
        let b = resolve_part(second, arow, MAX_ROWS);
        let (Some(a), Some(b)) = (a, b) else {
            return Some((R1C1Match::OutOfSheet, end));
        };
        let (r0, r0_abs, r1, r1_abs) = if a <= b {
            (a, row_part.abs, b, second.abs)
        } else {
            (b, second.abs, a, row_part.abs)
        };
        return Some((
            R1C1Match::Address(RefAddress::Rows {
                r0,
                r1,
                r0_abs,
                r1_abs,
            }),
            end,
        ));
    }
    let col_part = match_r1c1_part(s, i, b'C')?;
    let mut end = col_part.end;
    let mut second = col_part;
    if byte_at(s, end) == Some(b':')
        && let Some(p2) = match_r1c1_part(s, end + 1, b'C')
    {
        second = p2;
        end = p2.end;
    }
    if blocks_ref_end(s, end) {
        return None;
    }
    let a = resolve_part(col_part, acol, MAX_COLS);
    let b = resolve_part(second, acol, MAX_COLS);
    let (Some(a), Some(b)) = (a, b) else {
        return Some((R1C1Match::OutOfSheet, end));
    };
    let (c0, c0_abs, c1, c1_abs) = if a <= b {
        (a, col_part.abs, b, second.abs)
    } else {
        (b, second.abs, a, col_part.abs)
    };
    Some((
        R1C1Match::Address(RefAddress::Cols {
            c0,
            c1,
            c0_abs,
            c1_abs,
        }),
        end,
    ))
}

/// Finds the end of a bracketed structured reference starting at `[` (handles `'` escapes).
fn scan_brackets(s: &str, i: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut iter = s[i..].char_indices();
    while let Some((off, c)) = iter.next() {
        match c {
            '\'' => {
                iter.next();
            }
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + off + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn unescape_column(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\'' {
            if let Some(n) = chars.next() {
                out.push(n);
            }
        } else {
            out.push(c);
        }
    }
    out
}

enum Special {
    Item(StructuredSpecial),
    ThisRow,
}

fn special_of(item: &str) -> Option<Special> {
    Some(match item.to_uppercase().as_str() {
        "#ALL" => Special::Item(StructuredSpecial::All),
        "#DATA" => Special::Item(StructuredSpecial::Data),
        "#HEADERS" => Special::Item(StructuredSpecial::Headers),
        "#TOTALS" => Special::Item(StructuredSpecial::Totals),
        "#THIS ROW" => Special::ThisRow,
        _ => return None,
    })
}

type Inner = (
    Vec<StructuredSpecial>,
    bool,
    Option<(String, Option<String>)>,
);

/// Parses the bracketed items of `[[#Headers],[A]:[B]]` (the text between the outer brackets).
fn parse_item_list(text: &str) -> Result<Inner, String> {
    let mut specials = Vec::new();
    let mut this_row = false;
    let mut columns: Option<(String, Option<String>)> = None;
    let b = text.as_bytes();
    let mut i = 0usize;
    let skip_spaces = |i: &mut usize| {
        while *i < b.len() && b[*i] == b' ' {
            *i += 1;
        }
    };
    let read_item = |i: &mut usize| -> Option<String> {
        if b.get(*i) != Some(&b'[') {
            return None;
        }
        let end = scan_brackets(text, *i)?;
        let inner = text[*i + 1..end - 1].to_owned();
        *i = end;
        Some(inner)
    };
    loop {
        skip_spaces(&mut i);
        let Some(first) = read_item(&mut i) else {
            return Err("Expected a bracketed table item".to_owned());
        };
        skip_spaces(&mut i);
        if first.starts_with('#') {
            match special_of(&first) {
                Some(Special::ThisRow) => this_row = true,
                Some(Special::Item(sp)) => {
                    if !specials.contains(&sp) {
                        specials.push(sp);
                    }
                }
                None => return Err(format!("Unknown table item {first}")),
            }
        } else {
            if columns.is_some() {
                return Err("Only one column or column range is allowed".to_owned());
            }
            if b.get(i) == Some(&b':') {
                i += 1;
                skip_spaces(&mut i);
                match read_item(&mut i) {
                    Some(second) if !second.starts_with('#') => {
                        columns = Some((unescape_column(&first), Some(unescape_column(&second))));
                    }
                    _ => return Err("Expected a column after \":\"".to_owned()),
                }
                skip_spaces(&mut i);
            } else {
                columns = Some((unescape_column(&first), None));
            }
        }
        if i >= b.len() {
            break;
        }
        if b[i] != b',' {
            return Err("Expected \",\" between table items".to_owned());
        }
        i += 1;
    }
    Ok((specials, this_row, columns))
}

/// Parses the inside of `Table1[ ... ]`.
fn parse_structured_inner(inner: &str) -> Result<Inner, String> {
    let c = inner.trim();
    if c.is_empty() {
        return Ok((Vec::new(), false, None));
    }
    if let Some(rest) = c.strip_prefix('@') {
        let rest = rest.trim();
        if rest.is_empty() {
            return Ok((Vec::new(), true, None));
        }
        if rest.starts_with('[') {
            let (specials, _, columns) = parse_item_list(rest)?;
            if !specials.is_empty() {
                return Err("Table items cannot follow @".to_owned());
            }
            return Ok((Vec::new(), true, columns));
        }
        return Ok((Vec::new(), true, Some((unescape_column(rest), None))));
    }
    if c.starts_with('#') {
        return match special_of(c) {
            Some(Special::ThisRow) => Ok((Vec::new(), true, None)),
            Some(Special::Item(sp)) => Ok((vec![sp], false, None)),
            None => Err(format!("Unknown table item {c}")),
        };
    }
    if c.starts_with('[') {
        return parse_item_list(c);
    }
    Ok((Vec::new(), false, Some((unescape_column(inner), None))))
}

struct Lexer<'a> {
    src: &'a str,
    offset: u32,
    opts: ParseOptions,
    tokens: Vec<Token>,
    i: usize,
}

impl Lexer<'_> {
    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            start: start as u32 + self.offset,
            end: end as u32 + self.offset,
        });
        self.i = end;
    }

    fn try_reference(&mut self, start: usize) -> bool {
        let s = self.src;
        let mut j = start;
        let mut prefix = None;
        if let Some((p, end)) = scan_prefix(s, j) {
            prefix = Some(p);
            j = end;
        }
        let m = match self.opts.style {
            RefStyle::A1 => match_a1_address(s, j).map(|(a, e)| (R1C1Match::Address(a), e)),
            RefStyle::R1C1 => match_r1c1_address(s, j, self.opts.anchor),
        };
        if let Some((m, end)) = m {
            let kind = match m {
                R1C1Match::Address(address) => TokenKind::Ref { prefix, address },
                R1C1Match::OutOfSheet => TokenKind::Error {
                    code: ErrorValue::Ref,
                    prefix,
                },
            };
            self.push(kind, start, end);
            return true;
        }
        let Some(prefix) = prefix else {
            return false;
        };
        if s.get(j..j + 5)
            .is_some_and(|t| t.eq_ignore_ascii_case("#REF!"))
        {
            self.push(
                TokenKind::Error {
                    code: ErrorValue::Ref,
                    prefix: Some(prefix),
                },
                start,
                j + 5,
            );
            return true;
        }
        if let Some(c) = char_at(s, j)
            && is_name_start(c)
        {
            let mut e = j + c.len_utf8();
            while let Some(c) = char_at(s, e).filter(|&c| is_name_char(c)) {
                e += c.len_utf8();
            }
            let name = s[j..e].to_owned();
            self.push(
                TokenKind::Name {
                    name,
                    prefix: Some(prefix),
                },
                start,
                e,
            );
            return true;
        }
        self.push(
            TokenKind::Invalid("Expected a reference after the sheet name".to_owned()),
            start,
            j,
        );
        true
    }

    fn structured(&mut self, start: usize, open: usize, table: Option<String>) {
        let s = self.src;
        let Some(end) = scan_brackets(s, open) else {
            self.push(
                TokenKind::Invalid("Missing closing bracket".to_owned()),
                start,
                s.len(),
            );
            return;
        };
        match parse_structured_inner(&s[open + 1..end - 1]) {
            Ok((specials, this_row, columns)) => self.push(
                TokenKind::Structured(Box::new(StructuredRef {
                    table,
                    specials,
                    this_row,
                    columns,
                })),
                start,
                end,
            ),
            Err(msg) => self.push(TokenKind::Invalid(msg), start, end),
        }
    }

    fn run(&mut self) {
        let s = self.src;
        let mut brace_depth = 0u32;
        while self.i < s.len() {
            let start = self.i;
            let Some(ch) = char_at(s, start) else { break };
            let next = char_at(s, start + ch.len_utf8());
            if is_space(ch) {
                let mut e = start;
                while let Some(c) = char_at(s, e).filter(|&c| is_space(c)) {
                    e += c.len_utf8();
                }
                self.push(TokenKind::Whitespace, start, e);
                continue;
            }
            match ch {
                '"' => {
                    let mut e = start + 1;
                    let mut value = String::new();
                    let mut closed = false;
                    while let Some(c) = char_at(s, e) {
                        if c == '"' {
                            if byte_at(s, e + 1) == Some(b'"') {
                                value.push('"');
                                e += 2;
                                continue;
                            }
                            closed = true;
                            e += 1;
                            break;
                        }
                        value.push(c);
                        e += c.len_utf8();
                    }
                    if closed {
                        self.push(TokenKind::Str(value), start, e);
                    } else {
                        self.push(
                            TokenKind::Invalid("Missing closing quotation mark".to_owned()),
                            start,
                            e,
                        );
                    }
                    continue;
                }
                '#' => {
                    let prev = self.tokens.last();
                    let prev_operand = prev.is_some_and(|t| t.kind.ends_operand());
                    let code = ErrorValue::prefix_of(&s[start..]);
                    if prev.is_some_and(|t| {
                        t.kind.ends_operand()
                            && !matches!(t.kind, TokenKind::Error { .. })
                            && t.end == start as u32 + self.offset
                    }) && code.is_none()
                    {
                        self.push(TokenKind::Operator(OperatorToken::Hash), start, start + 1);
                        continue;
                    }
                    if let Some(code) = code {
                        self.push(
                            TokenKind::Error { code, prefix: None },
                            start,
                            start + code.code().len(),
                        );
                    } else if prev_operand {
                        self.push(TokenKind::Operator(OperatorToken::Hash), start, start + 1);
                    } else {
                        self.push(
                            TokenKind::Invalid("Unknown error value".to_owned()),
                            start,
                            start + 1,
                        );
                    }
                    continue;
                }
                '\'' | '[' => {
                    if self.try_reference(start) {
                        continue;
                    }
                    if ch == '[' {
                        self.structured(start, start, None);
                    } else {
                        self.push(
                            TokenKind::Invalid("Missing closing quote in sheet name".to_owned()),
                            start,
                            s.len(),
                        );
                    }
                    continue;
                }
                _ => {}
            }
            let digit = ch.is_ascii_digit();
            if digit || (ch == '.' && next.is_some_and(|c| c.is_ascii_digit())) || ch == '$' {
                if brace_depth == 0 && (ch == '$' || digit) && self.try_reference(start) {
                    continue;
                }
                if ch == '$' {
                    self.push(
                        TokenKind::Invalid("Unexpected \"$\"".to_owned()),
                        start,
                        start + 1,
                    );
                    continue;
                }
                let end = scan_number(s, start);
                let value = s[start..end].parse::<f64>().unwrap_or(f64::INFINITY);
                self.push(TokenKind::Number(value), start, end);
                continue;
            }
            if is_name_start(ch) {
                if self.try_reference(start) {
                    continue;
                }
                let mut e = start;
                while let Some(c) = char_at(s, e).filter(|&c| is_name_char(c)) {
                    e += c.len_utf8();
                }
                let word = &s[start..e];
                match byte_at(s, e) {
                    Some(b'(') => {
                        self.push(TokenKind::Function(word.to_uppercase()), start, e);
                        continue;
                    }
                    Some(b'[') => {
                        self.structured(start, e, Some(word.to_owned()));
                        continue;
                    }
                    _ => {}
                }
                if word.eq_ignore_ascii_case("TRUE") || word.eq_ignore_ascii_case("FALSE") {
                    let b = word.eq_ignore_ascii_case("TRUE");
                    self.push(TokenKind::Bool(b), start, e);
                } else {
                    self.push(
                        TokenKind::Name {
                            name: word.to_owned(),
                            prefix: None,
                        },
                        start,
                        e,
                    );
                }
                continue;
            }
            let one = start + 1;
            let (kind, end) = match ch {
                '(' => (TokenKind::LParen, one),
                ')' => (TokenKind::RParen, one),
                '{' => {
                    brace_depth += 1;
                    (TokenKind::LBrace, one)
                }
                '}' => {
                    brace_depth = brace_depth.saturating_sub(1);
                    (TokenKind::RBrace, one)
                }
                ',' => (TokenKind::Comma, one),
                ';' => (TokenKind::Semicolon, one),
                '<' => match next {
                    Some('=') => (TokenKind::Operator(OperatorToken::Le), start + 2),
                    Some('>') => (TokenKind::Operator(OperatorToken::Ne), start + 2),
                    _ => (TokenKind::Operator(OperatorToken::Lt), one),
                },
                '>' => match next {
                    Some('=') => (TokenKind::Operator(OperatorToken::Ge), start + 2),
                    _ => (TokenKind::Operator(OperatorToken::Gt), one),
                },
                '+' => (TokenKind::Operator(OperatorToken::Plus), one),
                '-' => (TokenKind::Operator(OperatorToken::Minus), one),
                '*' => (TokenKind::Operator(OperatorToken::Star), one),
                '/' => (TokenKind::Operator(OperatorToken::Slash), one),
                '^' => (TokenKind::Operator(OperatorToken::Caret), one),
                '&' => (TokenKind::Operator(OperatorToken::Amp), one),
                '=' => (TokenKind::Operator(OperatorToken::Eq), one),
                '%' => (TokenKind::Operator(OperatorToken::Percent), one),
                '@' => (TokenKind::Operator(OperatorToken::At), one),
                ':' => (TokenKind::Operator(OperatorToken::Colon), one),
                other => (
                    TokenKind::Invalid(format!("Unexpected character \"{other}\"")),
                    start + other.len_utf8(),
                ),
            };
            self.push(kind, start, end);
        }
    }
}

/// `(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?` at `i` (the caller checked the first character).
fn scan_number(s: &str, i: usize) -> usize {
    let mut j = run(s, i, |b| b.is_ascii_digit());
    if byte_at(s, j) == Some(b'.') {
        j = run(s, j + 1, |b| b.is_ascii_digit());
    }
    if matches!(byte_at(s, j), Some(b'e' | b'E')) {
        let mut k = j + 1;
        if matches!(byte_at(s, k), Some(b'+' | b'-')) {
            k += 1;
        }
        let e = run(s, k, |b| b.is_ascii_digit());
        if e > k {
            j = e;
        }
    }
    j
}

/// Splits a formula into tokens. `offset` is added to every position (use the length of a
/// stripped leading `=`). The input should not include the leading `=`; if it does, it is
/// tokenized as an operator.
pub fn tokenize(src: &str, opts: &ParseOptions, offset: u32) -> Vec<Token> {
    let mut lx = Lexer {
        src,
        offset,
        opts: *opts,
        tokens: Vec::new(),
        i: 0,
    };
    lx.run();
    lx.tokens
}
