// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comparison rules.
//!
//! Comparison operators order numbers < text < logicals; text compares case-insensitively but
//! accent-sensitively with a collation close to Excel's (punctuation before digits before
//! letters, accented letters next to their base letter); numbers compare after rounding to 15
//! significant digits (so `0.1+0.2=0.3` is TRUE); a blank takes the type of the other operand
//! (0, `""` or FALSE). Errors propagate.

use std::cmp::Ordering;

use super::error::ErrorValue;
use super::number::round15;
use super::value::Value;

/// Base letter and accent rank of a Latin letter (lower-cased); `accent` 0 means unaccented.
fn fold_latin(c: char) -> (char, u8) {
    let (base, accent) = match c {
        'à' => ('a', 1),
        'á' => ('a', 2),
        'â' => ('a', 3),
        'ã' => ('a', 4),
        'ä' => ('a', 5),
        'å' => ('a', 6),
        'ā' | 'ă' | 'ą' => ('a', 7),
        'ç' => ('c', 1),
        'ć' | 'ĉ' | 'ċ' | 'č' => ('c', 2),
        'ď' | 'đ' => ('d', 1),
        'è' => ('e', 1),
        'é' => ('e', 2),
        'ê' => ('e', 3),
        'ë' => ('e', 4),
        'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => ('e', 5),
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => ('g', 1),
        'ĥ' | 'ħ' => ('h', 1),
        'ì' => ('i', 1),
        'í' => ('i', 2),
        'î' => ('i', 3),
        'ï' => ('i', 4),
        'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => ('i', 5),
        'ĵ' => ('j', 1),
        'ķ' => ('k', 1),
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => ('l', 1),
        'ñ' => ('n', 1),
        'ń' | 'ņ' | 'ň' => ('n', 2),
        'ò' => ('o', 1),
        'ó' => ('o', 2),
        'ô' => ('o', 3),
        'õ' => ('o', 4),
        'ö' => ('o', 5),
        'ø' => ('o', 6),
        'ō' | 'ŏ' | 'ő' => ('o', 7),
        'ŕ' | 'ŗ' | 'ř' => ('r', 1),
        'ś' | 'ŝ' | 'ş' | 'š' => ('s', 1),
        'ţ' | 'ť' | 'ŧ' => ('t', 1),
        'ù' => ('u', 1),
        'ú' => ('u', 2),
        'û' => ('u', 3),
        'ü' => ('u', 4),
        'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => ('u', 5),
        'ŵ' => ('w', 1),
        'ý' | 'ÿ' | 'ŷ' => ('y', 1),
        'ź' | 'ż' | 'ž' => ('z', 1),
        other => (other, 0),
    };
    (base, accent)
}

/// One collation element: primary weight and secondary (accent) weight.
fn push_elements(c: char, out: &mut Vec<(u32, u8)>) {
    let lower = c.to_lowercase().next().unwrap_or(c);
    match lower {
        'ß' => {
            out.push((primary_letter('s'), 0));
            out.push((primary_letter('s'), 0));
            return;
        }
        'æ' => {
            out.push((primary_letter('a'), 0));
            out.push((primary_letter('e'), 0));
            return;
        }
        'œ' => {
            out.push((primary_letter('o'), 0));
            out.push((primary_letter('e'), 0));
            return;
        }
        _ => {}
    }
    let (base, accent) = fold_latin(lower);
    let primary = if base.is_ascii_lowercase() {
        primary_letter(base)
    } else if let Some(d) = base.to_digit(10) {
        0x0010_0000 + d
    } else if base.is_alphanumeric() {
        0x0030_0000 + base as u32
    } else {
        // Whitespace, punctuation and symbols sort before digits and letters.
        base as u32
    };
    out.push((primary, accent));
}

fn primary_letter(c: char) -> u32 {
    0x0020_0000 + (c as u32 - 'a' as u32)
}

fn elements(s: &str) -> Vec<(u32, u8)> {
    let mut out = Vec::with_capacity(s.len());
    for c in s.chars() {
        push_elements(c, &mut out);
    }
    out
}

/// Case-insensitive, accent-sensitive text comparison.
pub fn compare_text(a: &str, b: &str) -> Ordering {
    if a == b {
        return Ordering::Equal;
    }
    if a.is_ascii() && b.is_ascii() {
        return cmp_ascii_fold(a, b);
    }
    let ea = elements(a);
    let eb = elements(b);
    let primary = ea.iter().map(|e| e.0).cmp(eb.iter().map(|e| e.0));
    if primary != Ordering::Equal {
        return primary;
    }
    ea.iter().map(|e| e.1).cmp(eb.iter().map(|e| e.1))
}

fn ascii_weight(b: u8) -> u32 {
    let l = b.to_ascii_lowercase();
    if l.is_ascii_lowercase() {
        primary_letter(char::from(l))
    } else if l.is_ascii_digit() {
        0x0010_0000 + u32::from(l - b'0')
    } else {
        u32::from(l)
    }
}

fn cmp_ascii_fold(a: &str, b: &str) -> Ordering {
    a.bytes().map(ascii_weight).cmp(b.bytes().map(ascii_weight))
}

/// True when two texts are equal ignoring case.
pub fn text_equals(a: &str, b: &str) -> bool {
    a == b || compare_text(a, b) == Ordering::Equal
}

fn type_rank(v: &Value) -> u8 {
    match v {
        Value::Number(_) => 0,
        Value::Str(_) => 1,
        Value::Bool(_) => 2,
        _ => 3,
    }
}

/// Compares two scalars with the comparison-operator rules: the ordering, or the first error.
pub fn compare_values(a: &Value, b: &Value) -> Result<Ordering, ErrorValue> {
    if let Value::Error(e) = a {
        return Err(*e);
    }
    if let Value::Error(e) = b {
        return Err(*e);
    }
    match (a, b) {
        (Value::Blank, Value::Blank) => Ok(Ordering::Equal),
        (Value::Blank, other) => compare_values(&blank_as(other), other),
        (other, Value::Blank) => compare_values(other, &blank_as(other)),
        (Value::Number(x), Value::Number(y)) => {
            if x == y {
                return Ok(Ordering::Equal);
            }
            let (x, y) = (round15(*x), round15(*y));
            Ok(x.partial_cmp(&y).unwrap_or(Ordering::Equal))
        }
        (Value::Str(x), Value::Str(y)) => Ok(compare_text(x, y)),
        (Value::Bool(x), Value::Bool(y)) => Ok(x.cmp(y)),
        (Value::Array(_) | Value::Lambda(_), _) | (_, Value::Array(_) | Value::Lambda(_)) => {
            Err(ErrorValue::Value)
        }
        _ => Ok(type_rank(a).cmp(&type_rank(b))),
    }
}

fn blank_as(other: &Value) -> Value {
    match other {
        Value::Str(_) => Value::from(""),
        Value::Bool(_) => Value::Bool(false),
        _ => Value::Number(0.0),
    }
}

/// Total order used by SORT and friends: numbers < text < logicals < errors < blanks.
pub fn sort_compare(a: &Value, b: &Value) -> Ordering {
    let rank = |v: &Value| match v {
        Value::Blank => 4,
        Value::Error(_) => 3,
        other => type_rank(other),
    };
    let (ra, rb) = (rank(a), rank(b));
    if ra != rb {
        return ra.cmp(&rb);
    }
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.partial_cmp(y).unwrap_or(Ordering::Equal),
        (Value::Str(x), Value::Str(y)) => compare_text(x, y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Error(x), Value::Error(y)) => x.cmp(y),
        _ => Ordering::Equal,
    }
}

/// Equality used by lookups and UNIQUE: same type, numbers at 15 digits, text ignoring case.
pub fn lookup_equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x == y || round15(*x) == round15(*y),
        (Value::Str(x), Value::Str(y)) => text_equals(x, y),
        (Value::Error(x), Value::Error(y)) => x == y,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Blank, Value::Blank) => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmp(a: Value, b: Value) -> Result<Ordering, ErrorValue> {
        compare_values(&a, &b)
    }

    #[test]
    fn types_order_numbers_text_logicals() {
        assert_eq!(cmp(1e9.into(), "a".into()), Ok(Ordering::Less));
        assert_eq!(cmp("zzz".into(), false.into()), Ok(Ordering::Less));
        assert_eq!(cmp(true.into(), 5.0.into()), Ok(Ordering::Greater));
    }

    #[test]
    fn text_is_case_insensitive_and_accent_sensitive() {
        assert_eq!(cmp("abc".into(), "ABC".into()), Ok(Ordering::Equal));
        assert_eq!(cmp("apple".into(), "Banana".into()), Ok(Ordering::Less));
        assert_ne!(cmp("é".into(), "e".into()), Ok(Ordering::Equal));
        assert_eq!(compare_text("é", "f"), Ordering::Less);
        assert_eq!(compare_text("Ärger", "arger"), Ordering::Greater);
        assert_eq!(compare_text("a-b", "a1"), Ordering::Less);
        assert_eq!(compare_text("10", "9"), Ordering::Less);
        assert_eq!(compare_text("_", "a"), Ordering::Less);
        assert_eq!(compare_text("straße", "STRASSE"), Ordering::Equal);
        assert!(text_equals("Straße", "straße"));
    }

    #[test]
    fn blanks_take_the_other_type() {
        assert_eq!(cmp(Value::Blank, 0.0.into()), Ok(Ordering::Equal));
        assert_eq!(cmp(Value::Blank, "".into()), Ok(Ordering::Equal));
        assert_eq!(cmp(Value::Blank, false.into()), Ok(Ordering::Equal));
        assert_eq!(cmp(Value::Blank, (-1.0).into()), Ok(Ordering::Greater));
        assert_eq!(cmp(Value::Blank, Value::Blank), Ok(Ordering::Equal));
    }

    #[test]
    fn numbers_compare_at_15_digits_and_errors_propagate() {
        assert_eq!(cmp((0.1 + 0.2).into(), 0.3.into()), Ok(Ordering::Equal));
        assert_eq!(cmp(ErrorValue::NA.into(), 1.0.into()), Err(ErrorValue::NA));
        assert_eq!(
            cmp(1.0.into(), ErrorValue::Ref.into()),
            Err(ErrorValue::Ref)
        );
    }

    #[test]
    fn sort_order_and_lookup_equality() {
        let mut xs: Vec<Value> = vec![
            Value::Blank,
            ErrorValue::NA.into(),
            true.into(),
            "b".into(),
            2.0.into(),
            "A".into(),
            1.0.into(),
            false.into(),
        ];
        xs.sort_by(sort_compare);
        let want: Vec<Value> = vec![
            1.0.into(),
            2.0.into(),
            "A".into(),
            "b".into(),
            false.into(),
            true.into(),
            ErrorValue::NA.into(),
            Value::Blank,
        ];
        assert_eq!(xs, want);
        assert!(lookup_equals(&"Abc".into(), &"aBC".into()));
        assert!(!lookup_equals(&1.0.into(), &"1".into()));
        assert!(lookup_equals(
            &ErrorValue::NA.into(),
            &ErrorValue::NA.into()
        ));
    }
}
