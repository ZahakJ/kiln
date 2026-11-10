// SPDX-License-Identifier: AGPL-3.0-or-later
//! Excel error values. Errors are first-class values: they flow through formulas, propagate
//! through operators and most functions, and compare by their code.

use std::fmt;

/// An Excel error such as `#DIV/0!`. The discriminant order is `ERROR.TYPE`'s numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum ErrorValue {
    /// `#NULL!`: an intersection of ranges that do not intersect.
    Null = 1,
    /// `#DIV/0!`: division by zero.
    Div0,
    /// `#VALUE!`: a value of the wrong type.
    Value,
    /// `#REF!`: an invalid reference.
    Ref,
    /// `#NAME?`: an unknown name or function.
    Name,
    /// `#NUM!`: an invalid or out-of-range number.
    Num,
    /// `#N/A`: a value is not available.
    NA,
    /// `#GETTING_DATA`: an external value is still loading.
    GettingData,
    /// `#SPILL!`: a dynamic-array result cannot spill.
    Spill,
    /// `#CONNECT!`: an external connection failed.
    Connect,
    /// `#BLOCKED!`: access to a resource is blocked.
    Blocked,
    /// `#UNKNOWN!`: an unsupported data type.
    Unknown,
    /// `#FIELD!`: a missing field of a linked data type.
    Field,
    /// `#CALC!`: the calculation engine cannot produce a value (empty arrays, nested arrays,
    /// an uncalled lambda).
    Calc,
    /// `#BUSY!`: a value is still being computed.
    Busy,
}

impl ErrorValue {
    /// Every error, in `ERROR.TYPE` order.
    pub const ALL: [ErrorValue; 15] = [
        ErrorValue::Null,
        ErrorValue::Div0,
        ErrorValue::Value,
        ErrorValue::Ref,
        ErrorValue::Name,
        ErrorValue::Num,
        ErrorValue::NA,
        ErrorValue::GettingData,
        ErrorValue::Spill,
        ErrorValue::Connect,
        ErrorValue::Blocked,
        ErrorValue::Unknown,
        ErrorValue::Field,
        ErrorValue::Calc,
        ErrorValue::Busy,
    ];

    /// The literal as written in formulas (`#DIV/0!`).
    pub const fn code(self) -> &'static str {
        match self {
            ErrorValue::Null => "#NULL!",
            ErrorValue::Div0 => "#DIV/0!",
            ErrorValue::Value => "#VALUE!",
            ErrorValue::Ref => "#REF!",
            ErrorValue::Name => "#NAME?",
            ErrorValue::Num => "#NUM!",
            ErrorValue::NA => "#N/A",
            ErrorValue::GettingData => "#GETTING_DATA",
            ErrorValue::Spill => "#SPILL!",
            ErrorValue::Connect => "#CONNECT!",
            ErrorValue::Blocked => "#BLOCKED!",
            ErrorValue::Unknown => "#UNKNOWN!",
            ErrorValue::Field => "#FIELD!",
            ErrorValue::Calc => "#CALC!",
            ErrorValue::Busy => "#BUSY!",
        }
    }

    /// The number `ERROR.TYPE` returns (1 for `#NULL!` … 7 for `#N/A`, 14 for `#CALC!`).
    pub const fn type_number(self) -> u8 {
        self as u8
    }

    /// Looks up an error literal, ignoring case (`#n/a` → `#N/A`).
    pub fn from_code(text: &str) -> Option<ErrorValue> {
        ErrorValue::ALL
            .into_iter()
            .find(|e| e.code().eq_ignore_ascii_case(text))
    }

    /// The longest error literal at the start of `text` (ignoring case), if any.
    pub fn prefix_of(text: &str) -> Option<ErrorValue> {
        let mut best: Option<ErrorValue> = None;
        for e in ErrorValue::ALL {
            let code = e.code();
            if text.len() >= code.len()
                && text.is_char_boundary(code.len())
                && text[..code.len()].eq_ignore_ascii_case(code)
                && best.is_none_or(|b| b.code().len() < code.len())
            {
                best = Some(e);
            }
        }
        best
    }
}

impl fmt::Display for ErrorValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

#[cfg(test)]
mod tests {
    use super::ErrorValue;

    #[test]
    fn codes_round_trip_and_number_like_error_type() {
        for e in ErrorValue::ALL {
            assert_eq!(ErrorValue::from_code(e.code()), Some(e));
            assert_eq!(ErrorValue::from_code(&e.code().to_lowercase()), Some(e));
        }
        assert_eq!(ErrorValue::NA.type_number(), 7);
        assert_eq!(ErrorValue::Calc.type_number(), 14);
        assert_eq!(ErrorValue::from_code("#nope"), None);
        assert_eq!(ErrorValue::prefix_of("#n/a+1"), Some(ErrorValue::NA));
        assert_eq!(ErrorValue::prefix_of("#REF!A"), Some(ErrorValue::Ref));
        assert_eq!(ErrorValue::prefix_of("#R"), None);
        assert_eq!(ErrorValue::prefix_of("#é"), None);
    }
}
