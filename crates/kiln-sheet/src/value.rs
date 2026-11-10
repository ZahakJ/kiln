// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cell values as the sheet stores them, and their conversion to and from the formula engine's
//! values.
//!
//! A stored value is blank, a number, text, a logical, an error or rich text (text with runs of
//! character formatting, as SpreadsheetML's inline and shared rich strings). A formula cell's
//! stored value is its cached result.

use std::sync::Arc;

use kiln_formula::values::{ErrorValue, Value};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::styles::FontPatch;

/// One run of rich text: text and the character formatting it overrides.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TextRun {
    /// The text.
    pub text: String,
    /// Formatting that differs from the cell's font (`None` = the cell font).
    #[serde(default)]
    pub font: Option<FontPatch>,
}

/// Text with runs of character formatting.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(from = "Vec<TextRun>", into = "Vec<TextRun>")]
pub struct RichText {
    runs: Vec<TextRun>,
    plain: Arc<str>,
}

impl RichText {
    /// Rich text from runs.
    pub fn new(runs: Vec<TextRun>) -> RichText {
        let plain: String = runs.iter().map(|r| r.text.as_str()).collect();
        RichText {
            runs,
            plain: Arc::from(plain),
        }
    }

    /// The runs.
    pub fn runs(&self) -> &[TextRun] {
        &self.runs
    }

    /// The text without formatting (what formulas see).
    pub fn plain(&self) -> &Arc<str> {
        &self.plain
    }
}

impl From<Vec<TextRun>> for RichText {
    fn from(runs: Vec<TextRun>) -> RichText {
        RichText::new(runs)
    }
}

impl From<RichText> for Vec<TextRun> {
    fn from(r: RichText) -> Vec<TextRun> {
        r.runs
    }
}

/// A stored cell value.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum CellValue {
    /// Nothing (a cell that only has a format, or a formula whose result is blank).
    #[default]
    Blank,
    /// A number (finite).
    Number(f64),
    /// Text.
    Text(Arc<str>),
    /// A logical.
    Bool(bool),
    /// An error.
    Error(#[serde(with = "error_code")] ErrorValue),
    /// Formatted text.
    RichText(Arc<RichText>),
}

impl PartialEq for CellValue {
    fn eq(&self, other: &CellValue) -> bool {
        match (self, other) {
            (CellValue::Blank, CellValue::Blank) => true,
            (CellValue::Number(a), CellValue::Number(b)) => a.to_bits() == b.to_bits() || a == b,
            (CellValue::Text(a), CellValue::Text(b)) => a == b,
            (CellValue::Bool(a), CellValue::Bool(b)) => a == b,
            (CellValue::Error(a), CellValue::Error(b)) => a == b,
            (CellValue::RichText(a), CellValue::RichText(b)) => a == b,
            _ => false,
        }
    }
}

impl CellValue {
    /// A number; non-finite numbers become `#NUM!` (Excel has no infinities or NaN).
    pub fn number(n: f64) -> CellValue {
        if n.is_finite() {
            CellValue::Number(if n == 0.0 { 0.0 } else { n })
        } else {
            CellValue::Error(ErrorValue::Num)
        }
    }

    /// Text.
    pub fn text(s: impl Into<Arc<str>>) -> CellValue {
        CellValue::Text(s.into())
    }

    /// True for [`CellValue::Blank`].
    pub fn is_blank(&self) -> bool {
        matches!(self, CellValue::Blank)
    }

    /// True for blank or empty text (what Excel treats as an empty cell when sorting and
    /// filtering).
    pub fn is_empty_text_or_blank(&self) -> bool {
        match self {
            CellValue::Blank => true,
            CellValue::Text(s) => s.is_empty(),
            CellValue::RichText(r) => r.plain().is_empty(),
            _ => false,
        }
    }

    /// The number, if this is one.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            CellValue::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The text (plain text of rich text), if this is text.
    pub fn as_text(&self) -> Option<&str> {
        match self {
            CellValue::Text(s) => Some(s),
            CellValue::RichText(r) => Some(r.plain()),
            _ => None,
        }
    }

    /// The formula engine's value.
    pub fn to_value(&self) -> Value {
        match self {
            CellValue::Blank => Value::Blank,
            CellValue::Number(n) => Value::Number(*n),
            CellValue::Text(s) => Value::Str(s.clone()),
            CellValue::Bool(b) => Value::Bool(*b),
            CellValue::Error(e) => Value::Error(*e),
            CellValue::RichText(r) => Value::Str(r.plain().clone()),
        }
    }

    /// From an engine value: arrays become their top-left value, lambdas `#CALC!`.
    pub fn from_value(v: &Value) -> CellValue {
        match v {
            Value::Blank => CellValue::Blank,
            Value::Number(n) => CellValue::number(*n),
            Value::Str(s) => CellValue::Text(s.clone()),
            Value::Bool(b) => CellValue::Bool(*b),
            Value::Error(e) => CellValue::Error(*e),
            Value::Array(a) => CellValue::from_value(a.get(0, 0)),
            Value::Lambda(_) => CellValue::Error(ErrorValue::Calc),
        }
    }

    /// The text a cell shows without a number format (General): numbers as Excel prints them,
    /// `TRUE`/`FALSE`, error codes.
    pub fn display_general(&self) -> String {
        match self {
            CellValue::Blank => String::new(),
            CellValue::Number(n) => crate::format::general(*n, 11),
            CellValue::Text(s) => s.to_string(),
            CellValue::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
            CellValue::Error(e) => e.code().to_owned(),
            CellValue::RichText(r) => r.plain().to_string(),
        }
    }
}

impl From<f64> for CellValue {
    fn from(n: f64) -> CellValue {
        CellValue::number(n)
    }
}

impl From<i32> for CellValue {
    fn from(n: i32) -> CellValue {
        CellValue::Number(f64::from(n))
    }
}

impl From<bool> for CellValue {
    fn from(b: bool) -> CellValue {
        CellValue::Bool(b)
    }
}

impl From<&str> for CellValue {
    fn from(s: &str) -> CellValue {
        CellValue::Text(Arc::from(s))
    }
}

impl From<String> for CellValue {
    fn from(s: String) -> CellValue {
        CellValue::Text(Arc::from(s))
    }
}

impl From<ErrorValue> for CellValue {
    fn from(e: ErrorValue) -> CellValue {
        CellValue::Error(e)
    }
}

impl From<RichText> for CellValue {
    fn from(r: RichText) -> CellValue {
        CellValue::RichText(Arc::new(r))
    }
}

/// Serialises an [`ErrorValue`] as its code (`"#N/A"`).
pub(crate) mod error_code {
    use super::*;

    pub(crate) fn serialize<S: Serializer>(e: &ErrorValue, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(e.code())
    }

    pub(crate) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<ErrorValue, D::Error> {
        let code = String::deserialize(d)?;
        ErrorValue::from_code(&code)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown error code {code}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_to_and_from_engine_values() {
        let rich = RichText::new(vec![
            TextRun {
                text: "ab".into(),
                font: None,
            },
            TextRun {
                text: "c".into(),
                font: Some(FontPatch {
                    bold: Some(true),
                    ..FontPatch::default()
                }),
            },
        ]);
        let v = CellValue::from(rich);
        assert_eq!(v.to_value(), Value::from("abc"));
        assert_eq!(v.as_text(), Some("abc"));
        assert_eq!(
            CellValue::from_value(&Value::Number(2.0)),
            CellValue::Number(2.0)
        );
        assert_eq!(
            CellValue::number(f64::INFINITY),
            CellValue::Error(ErrorValue::Num)
        );
        assert_eq!(CellValue::number(-0.0).display_general(), "0");
        assert_eq!(CellValue::Error(ErrorValue::NA).display_general(), "#N/A");
    }

    #[test]
    fn serialises_errors_by_code() {
        let v = CellValue::Error(ErrorValue::Div0);
        let json = serde_json::to_string(&v).expect("json");
        assert_eq!(json, r##"{"Error":"#DIV/0!"}"##);
        let back: CellValue = serde_json::from_str(&json).expect("parse");
        assert_eq!(back, v);
        assert!(serde_json::from_str::<CellValue>(r##"{"Error":"#NOPE"}"##).is_err());
    }
}
