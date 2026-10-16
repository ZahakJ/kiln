// SPDX-License-Identifier: AGPL-3.0-or-later
//! Database functions (`DSUM`, `DAVERAGE`, `DCOUNT` …) and the Web function `ENCODEURL`.
//!
//! A database is a range whose first row holds field labels. A criteria range has labels in its
//! first row and conditions below: the conditions of one row must all hold (AND), and a database
//! row matches when any criteria row holds (OR). As in Excel, a plain text condition matches
//! values that *begin* with the text, `=text` matches exactly, and comparison operators work as
//! in `COUNTIF`.

use super::agg::{self, Agg};
use super::criteria::{Criterion, parse_criterion};
use super::support::*;
use crate::engine::{Arg, CallContext};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{Array2D, CoercionOptions, ErrorValue, Value, text_equals, to_text};

/// Index of the column whose label equals `label` (trimmed, case-insensitive).
fn column_of(header: &[Value], label: &str) -> Option<usize> {
    let label = label.trim();
    header
        .iter()
        .position(|h| matches!(h, Value::Str(s) if text_equals(s.trim(), label)))
}

/// The field argument: a 1-based column number or a label; `None` when omitted.
fn field_index(db: &Array2D, args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Option<usize>> {
    if omitted(args, 1) {
        return Ok(None);
    }
    match scalar(args, 1, ctx) {
        Value::Error(e) => Err(e),
        Value::Number(n) => {
            let i = n.trunc();
            if i < 1.0 || i > db.cols() as f64 {
                Err(ErrorValue::Value)
            } else {
                Ok(Some(i as usize - 1))
            }
        }
        v => {
            let label = to_text(&v)?;
            let header: Vec<Value> = (0..db.cols()).map(|c| db.get(0, c).clone()).collect();
            column_of(&header, &label)
                .map(Some)
                .ok_or(ErrorValue::Value)
        }
    }
}

/// A criteria cell as a predicate (`None` for an empty cell, which places no condition).
fn condition(c: &Value, opts: &CoercionOptions) -> Option<Condition> {
    match c {
        Value::Blank => None,
        Value::Str(s) if s.is_empty() => None,
        Value::Str(s) if !s.starts_with(['<', '>', '=']) => Some(Condition::Prefix {
            exact: parse_criterion(c, opts),
            prefix: parse_criterion(&Value::from(format!("{s}*")), opts),
        }),
        _ => Some(Condition::Plain(parse_criterion(c, opts))),
    }
}

enum Condition {
    /// Plain text: text values match when they begin with it, other values as the criterion.
    Prefix {
        exact: Criterion,
        prefix: Criterion,
    },
    Plain(Criterion),
}

impl Condition {
    fn matches(&self, v: &Value) -> bool {
        match self {
            Condition::Prefix { exact, prefix } => match v {
                Value::Str(_) => prefix.matches(v),
                _ => exact.matches(v),
            },
            Condition::Plain(c) => c.matches(v),
        }
    }
}

/// Rows (1-based within the database, so 0 is the header) that satisfy the criteria range.
fn matching_rows(db: &Array2D, crit: &Array2D, opts: &CoercionOptions) -> Vec<usize> {
    let header: Vec<Value> = (0..db.cols()).map(|c| db.get(0, c).clone()).collect();
    // The database column of each criteria column (`None` for a blank or unknown label: a
    // condition under it matches nothing, as in v1.0).
    let cols: Vec<Option<usize>> = (0..crit.cols())
        .map(|c| match crit.get(0, c) {
            Value::Blank => None,
            h => to_text(h).ok().and_then(|t| column_of(&header, &t)),
        })
        .collect();
    let rules: Vec<Vec<(Option<usize>, Condition)>> = (1..crit.rows())
        .map(|r| {
            cols.iter()
                .enumerate()
                .filter_map(|(c, col)| condition(crit.get(r, c), opts).map(|cond| (*col, cond)))
                .collect()
        })
        .collect();
    (1..db.rows())
        .filter(|&r| {
            rules.is_empty()
                || rules.iter().any(|row| {
                    row.iter()
                        .all(|(col, cond)| col.is_some_and(|c| cond.matches(db.get(r, c))))
                })
        })
        .collect()
}

#[derive(Clone, Copy)]
enum Mode {
    Numbers(Agg),
    CountA,
    Count,
    Get,
}

fn dfunc(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, mode: Mode) -> R<Value> {
    let db = array_of(args, 0, ctx)?;
    let crit = array_of(args, 2, ctx)?;
    if db.rows() < 1 || crit.rows() < 1 {
        return Err(ErrorValue::Value);
    }
    let field = field_index(&db, args, ctx)?;
    let rows = matching_rows(&db, &crit, &ctx.coercion());
    let Some(field) = field else {
        return match mode {
            Mode::Count | Mode::CountA => Ok(Value::number(rows.len() as f64)),
            _ => Err(ErrorValue::Value),
        };
    };
    match mode {
        Mode::Get => match rows.as_slice() {
            [] => Err(ErrorValue::Value),
            [r] => Ok(db.get(*r, field).clone().blank_to_zero()),
            _ => Err(ErrorValue::Num),
        },
        Mode::CountA => Ok(Value::number(
            rows.iter()
                .filter(|&&r| !matches!(db.get(r, field), Value::Blank))
                .count() as f64,
        )),
        Mode::Count | Mode::Numbers(_) => {
            let xs: Vec<f64> = rows
                .iter()
                .filter_map(|&r| db.get(r, field).as_number())
                .collect();
            match mode {
                Mode::Numbers(f) => f(&xs).map(Value::number),
                _ => Ok(Value::number(xs.len() as f64)),
            }
        }
    }
}

/// RFC 3986 percent-encoding of everything but the unreserved characters, over UTF-8 (as
/// Excel's `ENCODEURL`).
pub fn encode_url(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

macro_rules! dspec {
    ($name:literal, $mode:expr) => {
        spec($name, 3, 3, imp!(|a, c| dfunc(a, c, $mode)))
    };
}

/// The database family and `ENCODEURL`.
pub fn specs() -> Vec<FunctionSpec> {
    vec![
        dspec!("DAVERAGE", Mode::Numbers(agg::average)),
        dspec!("DCOUNT", Mode::Count),
        dspec!("DCOUNTA", Mode::CountA),
        dspec!("DGET", Mode::Get),
        dspec!("DMAX", Mode::Numbers(agg::max_of)),
        dspec!("DMIN", Mode::Numbers(agg::min_of)),
        dspec!("DPRODUCT", Mode::Numbers(agg::product)),
        dspec!("DSTDEV", Mode::Numbers(agg::stdev_s)),
        dspec!("DSTDEVP", Mode::Numbers(agg::stdev_p)),
        dspec!("DSUM", Mode::Numbers(agg::sum_of)),
        dspec!("DVAR", Mode::Numbers(agg::var_s)),
        dspec!("DVARP", Mode::Numbers(agg::var_p)),
        spec(
            "ENCODEURL",
            1,
            1,
            imp!(|a, c| text(a, 0, c).and_then(|s| capped(encode_url(&s)))),
        )
        .lift(ArgMask::ALL),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_like_excel() {
        assert_eq!(encode_url("a b&c/é~"), "a%20b%26c%2F%C3%A9~");
        assert_eq!(encode_url("AZaz09-_.~"), "AZaz09-_.~");
    }

    #[test]
    fn criteria_rows_are_or_of_ands() {
        let opts = CoercionOptions::default();
        let db = Array2D::from_rows(vec![
            vec!["Tree".into(), "Height".into()],
            vec!["Apple".into(), 18.0.into()],
            vec!["Pear".into(), 12.0.into()],
            vec!["Apricot".into(), 9.0.into()],
        ])
        .expect("db");
        let crit = Array2D::from_rows(vec![
            vec!["Tree".into(), "Height".into()],
            vec!["Ap".into(), ">10".into()],
            vec!["Pear".into(), Value::Blank],
        ])
        .expect("crit");
        assert_eq!(matching_rows(&db, &crit, &opts), vec![1, 2]);
        let unknown = Array2D::from_rows(vec![vec!["Nope".into()], vec!["x".into()]]).expect("c");
        assert!(matching_rows(&db, &unknown, &opts).is_empty());
    }
}
