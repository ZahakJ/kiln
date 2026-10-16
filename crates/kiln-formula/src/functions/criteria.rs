// SPDX-License-Identifier: AGPL-3.0-or-later
//! Criteria for `COUNTIF(S)`, `SUMIF(S)`, `AVERAGEIF(S)`, `MAXIFS`, `MINIFS` and the database
//! functions.
//!
//! A criterion is a number (`5`), a logical, an error, or text that may start with a comparison
//! operator (`">=10"`, `"<>apple"`, `"=*.xlsx"`). Excel's rules:
//!
//! - The operand after the operator is read as a number when it looks like one, including dates
//!   and times in text (`">=2024-01-01"`), percentages and currency; as a logical for
//!   `TRUE`/`FALSE`; as an error for `#N/A` and friends; otherwise as text.
//! - Numeric operands match numbers (and, for `=`, text that reads as the same number).
//! - Text operands with `=`/`<>` (or no operator) use wildcards and ignore case; `<`, `>`, `<=`,
//!   `>=` compare text alphabetically against text cells only.
//! - `""` or `"="` matches empty cells; `"<>"` matches non-empty cells.

use std::cmp::Ordering;

use super::matching::Wildcard;
use crate::values::{CoercionOptions, ErrorValue, Value, compare_text, parse_number_text, round15};

/// A comparison operator of a criterion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CriterionOp {
    /// `=` (or no operator).
    Eq,
    /// `<>`.
    Ne,
    /// `<`.
    Lt,
    /// `>`.
    Gt,
    /// `<=`.
    Le,
    /// `>=`.
    Ge,
}

impl CriterionOp {
    fn order(self, c: Ordering) -> bool {
        match self {
            CriterionOp::Lt => c == Ordering::Less,
            CriterionOp::Gt => c == Ordering::Greater,
            CriterionOp::Le => c != Ordering::Greater,
            CriterionOp::Ge => c != Ordering::Less,
            CriterionOp::Eq => c == Ordering::Equal,
            CriterionOp::Ne => c != Ordering::Equal,
        }
    }
}

fn cmp_num(op: CriterionOp, a: f64, b: f64) -> bool {
    let (x, y) = (round15(a), round15(b));
    op.order(x.partial_cmp(&y).unwrap_or(Ordering::Equal))
}

/// A parsed criterion.
#[derive(Debug, Clone)]
pub enum Criterion {
    /// A numeric comparison.
    Number(CriterionOp, f64, CoercionOptions),
    /// Exactly this logical.
    Bool(bool),
    /// Exactly this error.
    Error(ErrorValue),
    /// Empty cells and empty text (`""`, `"="`).
    Empty,
    /// Non-empty cells (`"<>"`).
    NotEmpty,
    /// Matches nothing (`"<"`, `">"` with no operand, ordering against an error).
    Never,
    /// A comparison against `TRUE`/`FALSE` written as text.
    BoolOp(CriterionOp, bool),
    /// `=#N/A` or `<>#N/A`.
    ErrorOp(bool, ErrorValue),
    /// Text matched with wildcards; `true` negates (`<>`).
    Text(bool, Wildcard),
    /// Alphabetical comparison against text cells.
    TextOrder(CriterionOp, String),
}

/// Builds a criterion from its value (`date1904` and the current year for dates in text).
pub fn parse_criterion(crit: &Value, opts: &CoercionOptions) -> Criterion {
    match crit {
        Value::Number(n) => Criterion::Number(CriterionOp::Eq, *n, *opts),
        Value::Bool(b) => Criterion::Bool(*b),
        Value::Error(e) => Criterion::Error(*e),
        Value::Str(s) => text_criterion(s, opts),
        _ => Criterion::Number(CriterionOp::Eq, 0.0, *opts),
    }
}

fn text_criterion(raw: &str, opts: &CoercionOptions) -> Criterion {
    let (op, rest) = [
        ("<=", CriterionOp::Le),
        (">=", CriterionOp::Ge),
        ("<>", CriterionOp::Ne),
        ("=", CriterionOp::Eq),
        ("<", CriterionOp::Lt),
        (">", CriterionOp::Gt),
    ]
    .iter()
    .find_map(|(p, op)| raw.strip_prefix(p).map(|r| (*op, r)))
    .unwrap_or((CriterionOp::Eq, raw));
    if rest.is_empty() {
        return match op {
            CriterionOp::Eq => Criterion::Empty,
            // A formula returning "" is not empty.
            CriterionOp::Ne => Criterion::NotEmpty,
            _ => Criterion::Never,
        };
    }
    if let Some(n) = parse_number_text(rest, opts) {
        return Criterion::Number(op, n, *opts);
    }
    if rest.eq_ignore_ascii_case("TRUE") || rest.eq_ignore_ascii_case("FALSE") {
        return Criterion::BoolOp(op, rest.eq_ignore_ascii_case("TRUE"));
    }
    if let Some(e) = ErrorValue::from_code(rest) {
        return match op {
            CriterionOp::Eq => Criterion::ErrorOp(false, e),
            CriterionOp::Ne => Criterion::ErrorOp(true, e),
            _ => Criterion::Never,
        };
    }
    match op {
        CriterionOp::Eq => Criterion::Text(false, Wildcard::new(rest)),
        CriterionOp::Ne => Criterion::Text(true, Wildcard::new(rest)),
        _ => Criterion::TextOrder(op, rest.to_owned()),
    }
}

impl Criterion {
    /// True when the value satisfies the criterion.
    pub fn matches(&self, v: &Value) -> bool {
        match self {
            Criterion::Number(op, n, opts) => {
                match v {
                    Value::Number(x) => cmp_num(*op, *x, *n),
                    Value::Str(s) => match op {
                        CriterionOp::Ne => parse_number_text(s, opts)
                            .is_none_or(|p| !cmp_num(CriterionOp::Eq, p, *n)),
                        CriterionOp::Eq => parse_number_text(s, opts)
                            .is_some_and(|p| cmp_num(CriterionOp::Eq, p, *n)),
                        _ => false,
                    },
                    _ => *op == CriterionOp::Ne,
                }
            }
            Criterion::Bool(b) => *v == Value::Bool(*b),
            Criterion::Error(e) => v.as_error() == Some(*e),
            Criterion::Empty => matches!(v, Value::Blank) || v.as_str() == Some(""),
            Criterion::NotEmpty => !matches!(v, Value::Blank),
            Criterion::Never => false,
            Criterion::BoolOp(op, b) => match op {
                CriterionOp::Eq => *v == Value::Bool(*b),
                CriterionOp::Ne => *v != Value::Bool(*b),
                _ => match v {
                    Value::Bool(x) => op.order(x.cmp(b)),
                    _ => false,
                },
            },
            Criterion::ErrorOp(negate, e) => (v.as_error() == Some(*e)) != *negate,
            Criterion::Text(negate, w) => {
                let hit = matches!(v, Value::Str(s) if w.is_match(s));
                hit != *negate
            }
            Criterion::TextOrder(op, t) => match v {
                Value::Str(s) => op.order(compare_text(s, t)),
                _ => false,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(crit: Value, v: Value) -> bool {
        let opts = CoercionOptions {
            date1904: false,
            current_year: Some(2024),
        };
        parse_criterion(&crit, &opts).matches(&v)
    }

    #[test]
    fn excel_criteria_rules() {
        let cases: Vec<(Value, Value, bool)> = vec![
            (">5".into(), 6.0.into(), true),
            (">5".into(), 5.0.into(), false),
            (">5".into(), "6".into(), false),
            ("<>5".into(), "x".into(), true),
            ("<>5".into(), Value::Blank, true),
            ("=5".into(), "5".into(), true),
            (5.0.into(), 5.0.into(), true),
            ("apple".into(), "APPLE".into(), true),
            ("a*".into(), "avocado".into(), true),
            ("a?".into(), "ab".into(), true),
            ("a?".into(), "abc".into(), false),
            ("~*".into(), "*".into(), true),
            ("~*".into(), "x".into(), false),
            ("".into(), Value::Blank, true),
            ("".into(), "".into(), true),
            ("".into(), 0.0.into(), false),
            ("=".into(), Value::Blank, true),
            ("<>".into(), Value::Blank, false),
            ("<>".into(), "x".into(), true),
            ("<>".into(), "".into(), true),
            (">b".into(), "c".into(), true),
            (">b".into(), 5.0.into(), false),
            ("TRUE".into(), true.into(), true),
            ("TRUE".into(), "TRUE".into(), false),
            ("#N/A".into(), ErrorValue::NA.into(), true),
            (">=2024-01-01".into(), 45292.0.into(), true),
            (">=2024-01-01".into(), 45291.0.into(), false),
            ("<>apple".into(), "Apple".into(), false),
            (">=50%".into(), 0.5.into(), true),
        ];
        for (crit, v, want) in cases {
            assert_eq!(m(crit.clone(), v.clone()), want, "{crit:?} on {v:?}");
        }
    }
}
