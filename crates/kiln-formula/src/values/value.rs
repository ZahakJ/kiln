// SPDX-License-Identifier: AGPL-3.0-or-later
//! The engine's value model.
//!
//! A [`Value`] is what a cell or an expression holds: a number, text, a logical, an error, a blank,
//! a rectangular [`Array2D`] (dynamic arrays) or a [`Lambda`]. Text and arrays are reference
//! counted so values are cheap to clone and can cross threads during parallel recalculation.

use std::fmt;
use std::sync::Arc;

use super::error::ErrorValue;
use crate::engine::Lambda;

/// A cell or expression value.
#[derive(Clone, Default)]
pub enum Value {
    /// An empty cell. Coerces to 0, `""` or FALSE depending on context.
    #[default]
    Blank,
    /// A number (always finite).
    Number(f64),
    /// Text.
    Str(Arc<str>),
    /// A logical.
    Bool(bool),
    /// An error.
    Error(ErrorValue),
    /// A rectangular array (at least 1×1; elements are never arrays or lambdas).
    Array(Arc<Array2D>),
    /// A lambda function value (`LAMBDA(x, x+1)`).
    Lambda(Arc<Lambda>),
}

impl Value {
    /// A number, or `#NUM!` when it is not finite (Excel has no infinities or NaN).
    pub fn number(n: f64) -> Value {
        if n.is_finite() {
            // Normalise negative zero so it prints and compares like Excel's zero.
            Value::Number(if n == 0.0 { 0.0 } else { n })
        } else {
            Value::Error(ErrorValue::Num)
        }
    }

    /// A text value.
    pub fn text(s: impl Into<Arc<str>>) -> Value {
        Value::Str(s.into())
    }

    /// An array value; a 1×1 array stays an array (callers decide whether to unwrap it).
    pub fn array(a: Array2D) -> Value {
        Value::Array(Arc::new(a))
    }

    /// True for [`Value::Blank`].
    pub fn is_blank(&self) -> bool {
        matches!(self, Value::Blank)
    }

    /// The error, if this is one.
    pub fn as_error(&self) -> Option<ErrorValue> {
        match self {
            Value::Error(e) => Some(*e),
            _ => None,
        }
    }

    /// The number, if this is one.
    pub fn as_number(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            _ => None,
        }
    }

    /// The text, if this is text.
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// True for numbers, text, logicals, errors and blanks.
    pub fn is_scalar(&self) -> bool {
        !matches!(self, Value::Array(_) | Value::Lambda(_))
    }

    /// The top-left element of an array, or the value itself.
    pub fn top_left(&self) -> Value {
        match self {
            Value::Array(a) => a.get(0, 0).clone(),
            other => other.clone(),
        }
    }

    /// Excel's `TYPE()` numbering: 1 number (and blank), 2 text, 4 logical, 16 error, 64 array,
    /// 128 lambda.
    pub fn type_number(&self) -> u8 {
        match self {
            Value::Blank | Value::Number(_) => 1,
            Value::Str(_) => 2,
            Value::Bool(_) => 4,
            Value::Error(_) => 16,
            Value::Array(_) => 64,
            Value::Lambda(_) => 128,
        }
    }

    /// A blank becomes 0 (what a cell shows for a formula returning an empty reference).
    pub fn blank_to_zero(self) -> Value {
        match self {
            Value::Blank => Value::Number(0.0),
            other => other,
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Value) -> bool {
        match (self, other) {
            (Value::Blank, Value::Blank) => true,
            (Value::Number(a), Value::Number(b)) => a == b,
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Error(a), Value::Error(b)) => a == b,
            (Value::Array(a), Value::Array(b)) => Arc::ptr_eq(a, b) || a == b,
            (Value::Lambda(a), Value::Lambda(b)) => Arc::ptr_eq(a, b),
            _ => false,
        }
    }
}

impl From<f64> for Value {
    fn from(n: f64) -> Value {
        Value::number(n)
    }
}

impl From<bool> for Value {
    fn from(b: bool) -> Value {
        Value::Bool(b)
    }
}

impl From<&str> for Value {
    fn from(s: &str) -> Value {
        Value::Str(Arc::from(s))
    }
}

impl From<String> for Value {
    fn from(s: String) -> Value {
        Value::Str(Arc::from(s))
    }
}

impl From<ErrorValue> for Value {
    fn from(e: ErrorValue) -> Value {
        Value::Error(e)
    }
}

impl fmt::Debug for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Blank => f.write_str("Blank"),
            Value::Number(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Bool(b) => f.write_str(if *b { "TRUE" } else { "FALSE" }),
            Value::Error(e) => f.write_str(e.code()),
            Value::Array(a) => write!(f, "{a:?}"),
            Value::Lambda(l) => write!(f, "LAMBDA({})", l.params().join(",")),
        }
    }
}

/// A rectangular array value stored row-major. Always at least 1×1; elements are scalars.
#[derive(Clone, PartialEq)]
pub struct Array2D {
    rows: usize,
    cols: usize,
    data: Vec<Value>,
}

impl Array2D {
    /// Builds an array from row-major data. Returns `None` unless `rows * cols == data.len()`, both
    /// dimensions are at least 1 and every element is a scalar.
    pub fn new(rows: usize, cols: usize, data: Vec<Value>) -> Option<Array2D> {
        if rows == 0 || cols == 0 || rows.checked_mul(cols)? != data.len() {
            return None;
        }
        if data.iter().any(|v| !v.is_scalar()) {
            return None;
        }
        Some(Array2D { rows, cols, data })
    }

    /// Builds an array from rows of equal, non-zero length.
    pub fn from_rows(rows: Vec<Vec<Value>>) -> Option<Array2D> {
        let cols = rows.first()?.len();
        if rows.iter().any(|r| r.len() != cols) {
            return None;
        }
        let n = rows.len();
        Array2D::new(n, cols, rows.into_iter().flatten().collect())
    }

    /// An array of `rows × cols` copies of a scalar. Dimensions of 0 are raised to 1.
    pub fn filled(rows: usize, cols: usize, value: Value) -> Array2D {
        let (rows, cols) = (rows.max(1), cols.max(1));
        let value = if value.is_scalar() {
            value
        } else {
            Value::Error(ErrorValue::Calc)
        };
        Array2D {
            rows,
            cols,
            data: vec![value; rows * cols],
        }
    }

    /// Builds an array element by element. Non-scalar results are stored as `#CALC!` (Excel does
    /// not nest arrays). Dimensions of 0 are raised to 1.
    pub fn build(rows: usize, cols: usize, mut f: impl FnMut(usize, usize) -> Value) -> Array2D {
        let (rows, cols) = (rows.max(1), cols.max(1));
        let mut data = Vec::with_capacity(rows * cols);
        for r in 0..rows {
            for c in 0..cols {
                let v = f(r, c);
                data.push(if v.is_scalar() {
                    v
                } else {
                    Value::Error(ErrorValue::Calc)
                });
            }
        }
        Array2D { rows, cols, data }
    }

    /// A single row.
    pub fn row(values: Vec<Value>) -> Option<Array2D> {
        let n = values.len();
        Array2D::new(1, n, values)
    }

    /// A single column.
    pub fn column(values: Vec<Value>) -> Option<Array2D> {
        let n = values.len();
        Array2D::new(n, 1, values)
    }

    /// Number of rows.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Number of columns.
    pub fn cols(&self) -> usize {
        self.cols
    }

    /// Number of elements.
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Always false (arrays have at least one element); present for API symmetry.
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Element at (r, c). Out-of-range positions give `#N/A`, as Excel's broadcasting does.
    pub fn get(&self, r: usize, c: usize) -> &Value {
        const NA: Value = Value::Error(ErrorValue::NA);
        if r < self.rows && c < self.cols {
            &self.data[r * self.cols + c]
        } else {
            &NA
        }
    }

    /// The elements in row-major order.
    pub fn values(&self) -> &[Value] {
        &self.data
    }

    /// Consumes the array into its row-major elements.
    pub fn into_values(self) -> Vec<Value> {
        self.data
    }

    /// Applies `f` to every element.
    pub fn map(&self, mut f: impl FnMut(&Value) -> Value) -> Array2D {
        Array2D::build(self.rows, self.cols, |r, c| f(self.get(r, c)))
    }

    /// Rows become columns.
    pub fn transpose(&self) -> Array2D {
        Array2D::build(self.cols, self.rows, |r, c| self.get(c, r).clone())
    }
}

impl fmt::Debug for Array2D {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{")?;
        for r in 0..self.rows {
            if r > 0 {
                f.write_str(";")?;
            }
            for c in 0..self.cols {
                if c > 0 {
                    f.write_str(",")?;
                }
                write!(f, "{:?}", self.get(r, c))?;
            }
        }
        f.write_str("}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn array_helpers() {
        let a = Array2D::from_rows(vec![
            vec![1.0.into(), 2.0.into(), 3.0.into()],
            vec![4.0.into(), 5.0.into(), 6.0.into()],
        ])
        .unwrap_or_else(|| Array2D::filled(1, 1, Value::Blank));
        assert_eq!((a.rows(), a.cols(), a.len()), (2, 3, 6));
        let t = a.transpose();
        assert_eq!((t.rows(), t.cols()), (3, 2));
        assert_eq!(t.get(2, 1), &Value::Number(6.0));
        assert_eq!(a.get(5, 5), &Value::Error(ErrorValue::NA));
        assert!(Array2D::from_rows(vec![vec![1.0.into()], vec![]]).is_none());
        assert!(Array2D::new(0, 1, vec![]).is_none());
        assert_eq!(
            Array2D::column(vec![1.0.into(), 2.0.into()]).map(|a| a.rows()),
            Some(2)
        );
    }

    #[test]
    fn numbers_are_finite_and_types_follow_excel() {
        assert_eq!(Value::number(f64::INFINITY), Value::Error(ErrorValue::Num));
        assert_eq!(Value::number(f64::NAN), Value::Error(ErrorValue::Num));
        assert_eq!(
            Value::number(-0.0).as_number().map(f64::is_sign_negative),
            Some(false)
        );
        assert_eq!(Value::Number(1.0).type_number(), 1);
        assert_eq!(Value::from("a").type_number(), 2);
        assert_eq!(Value::Bool(true).type_number(), 4);
        assert_eq!(Value::Error(ErrorValue::NA).type_number(), 16);
        assert_eq!(
            Value::array(Array2D::filled(1, 1, Value::Blank)).type_number(),
            64
        );
        assert_eq!(Value::Blank.type_number(), 1);
    }
}
