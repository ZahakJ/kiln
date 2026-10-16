// SPDX-License-Identifier: AGPL-3.0-or-later
//! Matching primitives shared by the lookup, text, criteria and database functions: Excel
//! wildcards and the exact, approximate and binary searches of `MATCH`, `VLOOKUP` and `XLOOKUP`.

use std::cmp::Ordering;

use crate::values::{Value, compare_values, lookup_equals};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    Lit(char),
    One,
    Many,
}

/// An Excel wildcard pattern: `*` any run, `?` one character, `~` escapes the next character.
/// Matching ignores case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wildcard {
    toks: Vec<Tok>,
}

/// Folds a character for case-insensitive matching, keeping one character per character so
/// positions stay aligned.
pub(crate) fn fold(c: char) -> char {
    let mut l = c.to_lowercase();
    match (l.next(), l.next()) {
        (Some(x), None) => x,
        _ => c,
    }
}

impl Wildcard {
    /// Compiles a pattern.
    pub fn new(pattern: &str) -> Wildcard {
        let mut toks = Vec::with_capacity(pattern.len());
        let mut it = pattern.chars().peekable();
        while let Some(c) = it.next() {
            match c {
                '~' => match it.next() {
                    Some(n) => toks.push(Tok::Lit(fold(n))),
                    None => toks.push(Tok::Lit('~')),
                },
                '*' => {
                    if toks.last() != Some(&Tok::Many) {
                        toks.push(Tok::Many);
                    }
                }
                '?' => toks.push(Tok::One),
                other => toks.push(Tok::Lit(fold(other))),
            }
        }
        Wildcard { toks }
    }

    /// True when the text contains a wildcard character (`*`, `?` or `~`).
    pub fn has_wildcards(text: &str) -> bool {
        text.contains(['*', '?', '~'])
    }

    /// True when the whole text matches.
    pub fn is_match(&self, text: &str) -> bool {
        let chars: Vec<char> = text.chars().map(fold).collect();
        self.run(&chars, true).is_some()
    }

    /// The character index where the leftmost match starts (the match may end anywhere).
    pub fn find(&self, text: &str) -> Option<usize> {
        let chars: Vec<char> = text.chars().map(fold).collect();
        self.run(&chars, false)
    }

    /// Simulates the pattern as an automaton over `text`, tracking for every pattern position
    /// the earliest start that reaches it: O(text × pattern) whatever the pattern.
    fn run(&self, text: &[char], anchored: bool) -> Option<usize> {
        let m = self.toks.len();
        let mut cur: Vec<Option<usize>> = vec![None; m + 1];
        let mut best: Option<usize> = None;
        let keep = |slot: &mut Option<usize>, s: usize| {
            if slot.is_none_or(|x| s < x) {
                *slot = Some(s);
            }
        };
        for i in 0..=text.len() {
            if !anchored || i == 0 {
                keep(&mut cur[0], i);
            }
            for j in 0..m {
                if self.toks[j] == Tok::Many
                    && let Some(s) = cur[j]
                {
                    keep(&mut cur[j + 1], s);
                }
            }
            if let Some(s) = cur[m]
                && (!anchored || i == text.len())
            {
                keep(&mut best, s);
                if !anchored {
                    // A later start can never be further left.
                    if cur.iter().flatten().all(|&x| x >= s) {
                        return best;
                    }
                }
            }
            if i == text.len() {
                break;
            }
            let ch = text[i];
            let mut next: Vec<Option<usize>> = vec![None; m + 1];
            let mut any = false;
            for j in 0..m {
                let Some(s) = cur[j] else { continue };
                if best.is_some_and(|b| s >= b) {
                    continue;
                }
                match self.toks[j] {
                    Tok::Many => keep(&mut next[j], s),
                    Tok::One => keep(&mut next[j + 1], s),
                    Tok::Lit(c) if c == ch => keep(&mut next[j + 1], s),
                    Tok::Lit(_) => continue,
                }
                any = true;
            }
            cur = next;
            if anchored && !any {
                return None;
            }
        }
        best
    }
}

/// Same type for approximate matching (numbers with numbers, text with text, logicals with
/// logicals; never blanks or errors).
fn same_type(a: &Value, b: &Value) -> bool {
    matches!(
        (a, b),
        (Value::Number(_), Value::Number(_))
            | (Value::Str(_), Value::Str(_))
            | (Value::Bool(_), Value::Bool(_))
    )
}

fn cmp(a: &Value, b: &Value) -> Ordering {
    compare_values(a, b).unwrap_or(Ordering::Equal)
}

/// Index of the first (or last, `reverse`) exact match; text with wildcards matches as a
/// pattern when `wild`.
pub(crate) fn find_exact(values: &[Value], x: &Value, wild: bool, reverse: bool) -> Option<usize> {
    let pattern = match x {
        Value::Str(s) if wild && Wildcard::has_wildcards(s) => Some(Wildcard::new(s)),
        _ => None,
    };
    let n = values.len();
    (0..n)
        .map(|k| if reverse { n - 1 - k } else { k })
        .find(|&i| {
            let v = &values[i];
            match &pattern {
                Some(p) => matches!(v, Value::Str(s) if p.is_match(s)),
                None => lookup_equals(v, x),
            }
        })
}

/// Binary search for the last value ≤ x in ascending data, skipping values of another type.
pub(crate) fn approx_ascending(values: &[Value], x: &Value) -> Option<usize> {
    approx(values, x, |c| c != Ordering::Greater)
}

/// Binary search for the last value ≥ x in descending data.
pub(crate) fn approx_descending(values: &[Value], x: &Value) -> Option<usize> {
    approx(values, x, |c| c != Ordering::Less)
}

fn approx(values: &[Value], x: &Value, accept: impl Fn(Ordering) -> bool) -> Option<usize> {
    let (mut lo, mut hi) = (0i64, values.len() as i64 - 1);
    let mut ans = None;
    while lo <= hi {
        let mid = (lo + hi) >> 1;
        let mut m = mid;
        while m >= lo && !same_type(&values[m as usize], x) {
            m -= 1;
        }
        if m < lo {
            lo = mid + 1;
            continue;
        }
        if accept(cmp(&values[m as usize], x)) {
            ans = Some(m as usize);
            lo = mid + 1;
        } else {
            hi = m - 1;
        }
    }
    ans
}

/// `XMATCH`/`XLOOKUP` matching with Excel's match modes (0 exact, -1 exact or next smaller, 1
/// exact or next larger, 2 wildcard) and search modes (1 first to last, -1 last to first, 2
/// binary ascending, -2 binary descending). Returns a 0-based index.
pub(crate) fn xmatch(
    values: &[Value],
    x: &Value,
    match_mode: i32,
    search_mode: i32,
) -> Option<usize> {
    if search_mode == 2 || search_mode == -2 {
        let asc = search_mode == 2;
        let (mut lo, mut hi) = (0i64, values.len() as i64 - 1);
        let (mut exact, mut below, mut above) = (None, None, None);
        while lo <= hi {
            let mid = (lo + hi) >> 1;
            let v = &values[mid as usize];
            let c = cmp(v, x);
            if c == Ordering::Equal && same_type(v, x) {
                exact = Some(mid as usize);
                hi = mid - 1;
                continue;
            }
            if c == Ordering::Less {
                below = Some(mid as usize);
                if asc {
                    lo = mid + 1;
                } else {
                    hi = mid - 1;
                }
            } else {
                above = Some(mid as usize);
                if asc {
                    hi = mid - 1;
                } else {
                    lo = mid + 1;
                }
            }
        }
        return match (exact, match_mode) {
            (Some(i), _) => Some(i),
            (None, -1) => below,
            (None, 1) => above,
            _ => None,
        };
    }
    let reverse = search_mode == -1;
    let exact = find_exact(values, x, match_mode == 2, reverse);
    if exact.is_some() || match_mode == 0 || match_mode == 2 {
        return exact;
    }
    let n = values.len();
    let mut best: Option<usize> = None;
    for k in 0..n {
        let i = if reverse { n - 1 - k } else { k };
        let v = &values[i];
        if !same_type(v, x) {
            continue;
        }
        let c = cmp(v, x);
        let better = |b: Option<usize>, want: Ordering| match b {
            None => true,
            Some(b) => cmp(v, &values[b]) == want,
        };
        if match_mode == -1 && c == Ordering::Less && better(best, Ordering::Greater) {
            best = Some(i);
        }
        if match_mode == 1 && c == Ordering::Greater && better(best, Ordering::Less) {
            best = Some(i);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_whole_text_ignoring_case() {
        let w = Wildcard::new("a*");
        assert!(w.is_match("Avocado"));
        assert!(!w.is_match("banana"));
        assert!(Wildcard::new("a?").is_match("ab"));
        assert!(!Wildcard::new("a?").is_match("abc"));
        assert!(Wildcard::new("~*").is_match("*"));
        assert!(!Wildcard::new("~*").is_match("x"));
        assert!(Wildcard::new("*.xlsx").is_match("Q1.XLSX"));
        assert!(Wildcard::new("*").is_match(""));
        assert!(Wildcard::new("a*b*c").is_match("aXXbYYc"));
        assert!(!Wildcard::new("a*b*c").is_match("aXXbYY"));
        assert!(Wildcard::new("~").is_match("~"));
    }

    #[test]
    fn find_returns_leftmost_start() {
        assert_eq!(Wildcard::new("b?d").find("abcdbxd"), Some(1));
        assert_eq!(Wildcard::new("x*").find("abc"), None);
        assert_eq!(Wildcard::new("*c").find("abc"), Some(0));
        assert_eq!(Wildcard::new("C").find("abc"), Some(2));
        assert_eq!(Wildcard::new("").find("abc"), Some(0));
    }

    #[test]
    fn long_patterns_stay_linear() {
        let text = "a".repeat(20_000);
        let pat = format!("{}b", "*a".repeat(50));
        assert!(!Wildcard::new(&pat).is_match(&text));
        assert_eq!(Wildcard::new(&pat).find(&text), None);
    }

    #[test]
    fn approximate_searches_skip_other_types() {
        let v: Vec<Value> = vec![1.0.into(), "x".into(), 3.0.into(), 5.0.into()];
        assert_eq!(approx_ascending(&v, &4.0.into()), Some(2));
        assert_eq!(approx_ascending(&v, &0.0.into()), None);
        let d: Vec<Value> = vec![5.0.into(), 3.0.into(), 1.0.into()];
        assert_eq!(approx_descending(&d, &2.0.into()), Some(1));
    }

    #[test]
    fn xmatch_modes() {
        let v: Vec<Value> = vec![10.0.into(), 20.0.into(), 30.0.into()];
        assert_eq!(xmatch(&v, &25.0.into(), -1, 1), Some(1));
        assert_eq!(xmatch(&v, &25.0.into(), 1, 1), Some(2));
        assert_eq!(xmatch(&v, &25.0.into(), 0, 1), None);
        assert_eq!(xmatch(&v, &25.0.into(), -1, 2), Some(1));
        assert_eq!(xmatch(&v, &20.0.into(), 0, 2), Some(1));
        let t: Vec<Value> = vec!["apple".into(), "banana".into()];
        assert_eq!(xmatch(&t, &"b*".into(), 2, 1), Some(1));
    }
}
