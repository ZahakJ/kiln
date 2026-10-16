// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text functions. Positions and lengths count UTF-16 code units, as Excel does. `TEXT`,
//! `DOLLAR` and `FIXED` format through [`numfmt`](crate::numfmt) (Excel number formats, en-US
//! conventions), so `TEXT(1234.5,"#,##0.00")` is `1,234.50`. The `REGEX*` functions use
//! `fancy-regex`: Perl/PCRE-style syntax with look-around, back-references and atomic groups, as
//! in Excel's PCRE2, and a backtracking limit so that no pattern can hang a recalculation (a
//! pattern that exceeds it is `#VALUE!`).

use std::sync::Arc;

use fancy_regex::{Captures, Regex, RegexBuilder};

use super::matching::{Wildcard, fold};
use super::support::*;
use super::textutil::{char_to16, find16, from16, len16, slice16};
use crate::engine::{Arg, CallContext, EvalValue};
use crate::numfmt::{FormatOptions, NumberFormat};
use crate::registry::{ArgMask, FunctionSpec};
use crate::values::{
    ErrorValue, Value, excel_round, number_to_text, parse_number_text, to_bool, to_number, to_text,
};

// ── formatting ─────────────────────────────────────────────────────────────────────────────

/// Formats a number (or text, through the text section) with an Excel format code; invalid
/// codes and dates out of range are `#VALUE!`.
pub(crate) fn format_with(value: &Value, code: &str, ctx: &dyn CallContext) -> R<String> {
    let opts = FormatOptions {
        date1904: ctx.date1904(),
        nbsp: false,
    };
    let f = NumberFormat::parse(code);
    let out = match value {
        Value::Number(n) => f.format_number(*n, &opts),
        Value::Str(s) => f.format_text(s, &opts),
        other => f.format(other, &opts),
    };
    out.map_err(|_| ErrorValue::Value)
}

fn text_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let v = scalar(args, 0, ctx);
    if let Value::Error(e) = v {
        return Err(e);
    }
    let f = text(args, 1, ctx)?;
    let x = match v {
        Value::Bool(b) => return Ok(if b { "TRUE" } else { "FALSE" }.to_owned()),
        Value::Number(n) => Value::Number(n),
        Value::Str(s) => match parse_number_text(&s, &ctx.coercion()) {
            Some(n) => Value::Number(n),
            None => Value::Str(s),
        },
        _ => Value::Number(0.0),
    };
    if f.is_empty() {
        return Ok(String::new());
    }
    if let Value::Number(n) = x
        && f.trim().eq_ignore_ascii_case("general")
    {
        return Ok(number_to_text(n));
    }
    capped(format_with(&x, &f, ctx)?)
}

fn decimals_pattern(d: f64) -> String {
    if d > 0.0 {
        format!(".{}", "0".repeat(d as usize))
    } else {
        String::new()
    }
}

fn dollar(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let x = num(args, 0, ctx, 0.0)?;
    let d = int(args, 1, ctx, 2.0)?;
    if d > 127.0 {
        return Err(ErrorValue::Value);
    }
    let r = excel_round(x, d);
    let body = format!("$#,##0{}", decimals_pattern(d));
    format_with(&Value::Number(r), &format!("{body};({body})"), ctx)
}

fn fixed(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let x = num(args, 0, ctx, 0.0)?;
    let d = int(args, 1, ctx, 2.0)?;
    let no_commas = flag(args, 2, ctx, false)?;
    if d > 127.0 {
        return Err(ErrorValue::Value);
    }
    let r = excel_round(x, d);
    let code = format!(
        "{}{}",
        if no_commas { "0" } else { "#,##0" },
        decimals_pattern(d)
    );
    format_with(&Value::Number(r), &code, ctx)
}

// ── Windows-1252 for CHAR/CODE ─────────────────────────────────────────────────────────────

const CP1252_HIGH: [(u32, u32); 27] = [
    (128, 0x20ac),
    (130, 0x201a),
    (131, 0x0192),
    (132, 0x201e),
    (133, 0x2026),
    (134, 0x2020),
    (135, 0x2021),
    (136, 0x02c6),
    (137, 0x2030),
    (138, 0x0160),
    (139, 0x2039),
    (140, 0x0152),
    (142, 0x017d),
    (145, 0x2018),
    (146, 0x2019),
    (147, 0x201c),
    (148, 0x201d),
    (149, 0x2022),
    (150, 0x2013),
    (151, 0x2014),
    (152, 0x02dc),
    (153, 0x2122),
    (154, 0x0161),
    (155, 0x203a),
    (156, 0x0153),
    (158, 0x017e),
    (159, 0x0178),
];

fn char_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let n = int(args, 0, ctx, 0.0)?;
    if !(1.0..=255.0).contains(&n) {
        return Err(ErrorValue::Value);
    }
    let n = n as u32;
    let code = CP1252_HIGH
        .iter()
        .find(|(k, _)| *k == n)
        .map_or(n, |(_, v)| *v);
    Ok(char::from_u32(code).map(String::from).unwrap_or_default())
}

fn code_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = text(args, 0, ctx)?;
    let Some(c) = s.encode_utf16().next() else {
        return Err(ErrorValue::Value);
    };
    let c = u32::from(c);
    if c < 128 || (160..=255).contains(&c) {
        return Ok(f64::from(c));
    }
    Ok(CP1252_HIGH
        .iter()
        .find(|(_, v)| *v == c)
        .map_or(63.0, |(k, _)| f64::from(*k)))
}

fn unichar(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let n = int(args, 0, ctx, 0.0)?;
    if !(1.0..=1_114_111.0).contains(&n) {
        return Err(ErrorValue::Value);
    }
    if (55_296.0..=57_343.0).contains(&n) {
        return Err(ErrorValue::NA);
    }
    char::from_u32(n as u32)
        .map(String::from)
        .ok_or(ErrorValue::Value)
}

fn unicode(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let s = text(args, 0, ctx)?;
    s.chars()
        .next()
        .map(|c| f64::from(u32::from(c)))
        .ok_or(ErrorValue::Value)
}

fn trim(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c == ' ' {
            if !space && !out.is_empty() {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

fn proper(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_letter = false;
    for c in s.chars() {
        let lower: String = c.to_lowercase().collect();
        let upper: String = c.to_uppercase().collect();
        let is_letter = lower != upper;
        if is_letter {
            out.push_str(if prev_letter { &lower } else { &upper });
        } else {
            out.push(c);
        }
        prev_letter = is_letter;
    }
    out
}

// ── searching ──────────────────────────────────────────────────────────────────────────────

fn folded(s: &str) -> String {
    s.chars().map(fold).collect()
}

fn find_impl(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, insensitive: bool) -> R<f64> {
    let needle = text(args, 0, ctx)?;
    let hay = text(args, 1, ctx)?;
    let start = int(args, 2, ctx, 1.0)?;
    let len = len16(&hay) as f64;
    if start < 1.0 || start > len + 1.0 {
        return Err(ErrorValue::Value);
    }
    if needle.is_empty() {
        return Ok(start);
    }
    let from = start as usize - 1;
    if !insensitive {
        return find16(&hay, &needle, from)
            .map(|i| i as f64 + 1.0)
            .ok_or(ErrorValue::Value);
    }
    if Wildcard::has_wildcards(&needle) {
        let sub = from16(&hay, from);
        let w = Wildcard::new(&needle);
        return w
            .find(&sub)
            .map(|ci| (char_to16(&sub, ci) + from) as f64 + 1.0)
            .ok_or(ErrorValue::Value);
    }
    find16(&folded(&hay), &folded(&needle), from)
        .map(|i| i as f64 + 1.0)
        .ok_or(ErrorValue::Value)
}

fn replace(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let s = text(args, 0, ctx)?;
    let start = int(args, 1, ctx, 0.0)?;
    let n = int(args, 2, ctx, 0.0)?;
    let repl = text(args, 3, ctx)?;
    if start < 1.0 || n < 0.0 {
        return Err(ErrorValue::Value);
    }
    let a = (start - 1.0) as usize;
    let b = a.saturating_add(n as usize);
    capped(format!("{}{repl}{}", slice16(&s, 0, a), from16(&s, b)))
}

fn substitute(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let s = text(args, 0, ctx)?;
    let old = text(args, 1, ctx)?;
    let repl = text(args, 2, ctx)?;
    if old.is_empty() {
        return Ok(s.to_string());
    }
    if args.len() < 4 {
        return capped(s.replace(&*old, &repl));
    }
    let k = int(args, 3, ctx, 0.0)?;
    if k < 1.0 {
        return Err(ErrorValue::Value);
    }
    match s.match_indices(&*old).nth(k as usize - 1) {
        None => Ok(s.to_string()),
        Some((i, _)) => capped(format!("{}{repl}{}", &s[..i], &s[i + old.len()..])),
    }
}

fn rept(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let s = text(args, 0, ctx)?;
    let n = int(args, 1, ctx, 0.0)?;
    if n < 0.0 || len16(&s) as f64 * n > MAX_TEXT as f64 {
        return Err(ErrorValue::Value);
    }
    Ok(s.repeat(n as usize))
}

fn value_fn(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    match scalar(args, 0, ctx) {
        Value::Number(n) => Ok(n),
        Value::Error(e) => Err(e),
        Value::Blank => Ok(0.0),
        Value::Str(s) => parse_number_text(&s, &ctx.coercion()).ok_or(ErrorValue::Value),
        _ => Err(ErrorValue::Value),
    }
}

fn numbervalue(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    let t = text(args, 0, ctx)?;
    let dec = text(args, 1, ctx)?;
    let grp = text(args, 2, ctx)?;
    let d = dec.chars().next().unwrap_or('.');
    let g = grp.chars().next().unwrap_or(',');
    if d == g {
        return Err(ErrorValue::Value);
    }
    let mut s: String = t.chars().filter(|c| !c.is_whitespace()).collect();
    if s.is_empty() {
        return Ok(0.0);
    }
    let mut pct = 0;
    while s.ends_with('%') {
        pct += 1;
        s.pop();
    }
    let (int_part, frac) = match s.find(d) {
        Some(i) => {
            let rest = &s[i + d.len_utf8()..];
            if rest.contains(d) {
                return Err(ErrorValue::Value);
            }
            (&s[..i], Some(rest))
        }
        None => (s.as_str(), None),
    };
    if frac.is_some_and(|f| f.contains(g)) {
        return Err(ErrorValue::Value);
    }
    let int_part: String = int_part.chars().filter(|c| *c != g).collect();
    let normal = match frac {
        Some(f) => format!("{int_part}.{f}"),
        None => int_part,
    };
    if !is_plain_number(&normal) {
        return Err(ErrorValue::Value);
    }
    let n: f64 = normal.parse().map_err(|_| ErrorValue::Value)?;
    Ok(n / 100f64.powi(pct))
}

/// `^[+-]?(\d+\.?\d*|\.\d+)([eE][+-]?\d+)?$`
fn is_plain_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;
    let mut frac_digits = 0;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let f = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - f;
    }
    if int_digits == 0 && frac_digits == 0 {
        return false;
    }
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        i += 1;
        if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
            i += 1;
        }
        let e = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        if i == e {
            return false;
        }
    }
    i == b.len()
}

// ── CONCAT / TEXTJOIN ──────────────────────────────────────────────────────────────────────

fn concat(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let mut out = String::new();
    for a in args.iter_mut() {
        let v = ctx.force(a);
        let mut err = None;
        ctx.for_each_scalar(&v, &mut |x, _| match to_text(x) {
            Ok(t) => {
                out.push_str(&t);
                true
            }
            Err(e) => {
                err = Some(e);
                false
            }
        });
        if let Some(e) = err {
            return Err(e);
        }
    }
    capped(out)
}

fn texts_of(v: &EvalValue, ctx: &mut dyn CallContext, skip_empty: bool) -> R<Vec<Arc<str>>> {
    let mut out = Vec::new();
    let mut err = None;
    ctx.for_each_scalar(v, &mut |x, _| {
        if skip_empty && (x.is_blank() || x.as_str() == Some("")) {
            return true;
        }
        match to_text(x) {
            Ok(t) => {
                out.push(t);
                true
            }
            Err(e) => {
                err = Some(e);
                false
            }
        }
    });
    match err {
        Some(e) => Err(e),
        None => Ok(out),
    }
}

fn textjoin(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let dv = ctx.force(&mut args[0]);
    let mut delims = texts_of(&dv, ctx, false)?;
    if delims.is_empty() {
        delims.push(Arc::from(""));
    }
    let iv = ctx.force(&mut args[1]);
    let iv = ctx.to_scalar(iv);
    let ignore = to_bool(&iv)?;
    let mut parts = Vec::new();
    for a in args[2..].iter_mut() {
        let v = ctx.force(a);
        parts.extend(texts_of(&v, ctx, ignore)?);
    }
    let mut out = String::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.push_str(&delims[(i - 1) % delims.len()]);
        }
        out.push_str(p);
    }
    capped(out)
}

fn concatenate(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let mut out = String::new();
    for i in 0..args.len() {
        out.push_str(&text(args, i, ctx)?);
    }
    capped(out)
}

// ── TEXTBEFORE / TEXTAFTER / TEXTSPLIT ─────────────────────────────────────────────────────

fn delimiters(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Vec<String>> {
    let v = match value_at(args, i, ctx) {
        EvalValue::Missing => EvalValue::Value(Value::from("")),
        other => other,
    };
    Ok(texts_of(&v, ctx, false)?
        .into_iter()
        .map(|t| t.to_string())
        .collect())
}

/// Every non-overlapping delimiter occurrence as character ranges, scanning from the left
/// (the longest delimiter wins at a position).
fn occurrences(text: &[char], delims: &[Vec<char>]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i <= text.len() {
        let mut best: Option<(usize, usize)> = None;
        for d in delims.iter().filter(|d| !d.is_empty()) {
            if let Some(k) = (i..=text.len().saturating_sub(d.len()))
                .find(|&k| k + d.len() <= text.len() && text[k..k + d.len()] == d[..])
                && best.is_none_or(|(b, bl)| k < b || (k == b && d.len() > bl))
            {
                best = Some((k, d.len()));
            }
        }
        let Some((k, l)) = best else { break };
        out.push((k, k + l));
        i = k + l;
    }
    out
}

fn prepared(text: &str, delims: &[String], insensitive: bool) -> (Vec<char>, Vec<Vec<char>>) {
    let f = |s: &str| -> Vec<char> {
        if insensitive {
            s.chars().map(fold).collect()
        } else {
            s.chars().collect()
        }
    };
    (f(text), delims.iter().map(|d| f(d)).collect())
}

fn scalar_number(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext, dflt: f64) -> R<f64> {
    if omitted(args, i) {
        return Ok(dflt);
    }
    let v = value_at(args, i, ctx);
    let v = ctx.to_scalar(v);
    to_number(&v, &ctx.coercion())
}

fn text_before_after(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, after: bool) -> R<Value> {
    let tv = value_at(args, 0, ctx);
    let tv = ctx.to_scalar(tv);
    let t = to_text(&tv)?;
    let delims = delimiters(args, 1, ctx)?;
    let inst = scalar_number(args, 2, ctx, 1.0)?;
    let mode = scalar_number(args, 3, ctx, 0.0)?;
    let end = scalar_number(args, 4, ctx, 0.0)?;
    let n = inst.trunc();
    if n == 0.0 || n.abs() > len16(&t) as f64 + 1.0 {
        return Err(ErrorValue::Value);
    }
    let chars: Vec<char> = t.chars().collect();
    let piece =
        |a: usize, b: usize| -> Value { Value::from(chars[a..b].iter().collect::<String>()) };
    if delims.iter().all(String::is_empty) {
        // An empty delimiter matches at the very start (or the end for negative instances).
        let whole = Value::Str(t.clone());
        let empty = Value::from("");
        return Ok(match (n > 0.0, after) {
            (true, true) | (false, false) => whole,
            _ => empty,
        });
    }
    let (hay, ds) = prepared(&t, &delims, mode == 1.0);
    let mut occ = occurrences(&hay, &ds);
    if end != 0.0 {
        if n > 0.0 {
            occ.push((chars.len(), chars.len()));
        } else {
            occ.insert(0, (0, 0));
        }
    }
    let k = if n > 0.0 {
        n as i64 - 1
    } else {
        occ.len() as i64 + n as i64
    };
    match usize::try_from(k).ok().and_then(|k| occ.get(k)) {
        Some(&(a, b)) => Ok(if after {
            piece(b, chars.len())
        } else {
            piece(0, a)
        }),
        None => {
            if args.len() > 5 && !omitted(args, 5) {
                let v = value_at(args, 5, ctx);
                Ok(ctx.to_scalar(v))
            } else {
                Err(ErrorValue::NA)
            }
        }
    }
}

fn textsplit(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let tv = value_at(args, 0, ctx);
    let tv = ctx.to_scalar(tv);
    let t = to_text(&tv)?;
    let col_d = if omitted(args, 1) {
        Vec::new()
    } else {
        delimiters(args, 1, ctx)?
    };
    let row_d = if omitted(args, 2) {
        Vec::new()
    } else {
        delimiters(args, 2, ctx)?
    };
    if col_d.iter().all(String::is_empty) && row_d.iter().all(String::is_empty) {
        return Err(ErrorValue::Value);
    }
    let ignore_empty = scalar_number(args, 3, ctx, 0.0)? != 0.0;
    let mode = scalar_number(args, 4, ctx, 0.0)?;
    let pad = if omitted(args, 5) {
        Value::Error(ErrorValue::NA)
    } else {
        let v = value_at(args, 5, ctx);
        ctx.to_scalar(v)
    };
    let insensitive = mode == 1.0;
    let split = |s: &str, ds: &[String]| -> Vec<String> {
        if ds.iter().all(String::is_empty) {
            return vec![s.to_owned()];
        }
        let chars: Vec<char> = s.chars().collect();
        let (hay, dc) = prepared(s, ds, insensitive);
        let mut parts = Vec::new();
        let mut i = 0;
        for (a, b) in occurrences(&hay, &dc) {
            parts.push(chars[i..a].iter().collect::<String>());
            i = b;
        }
        parts.push(chars[i..].iter().collect());
        if ignore_empty {
            parts.retain(|p| !p.is_empty());
        }
        parts
    };
    let rows: Vec<Vec<String>> = split(&t, &row_d)
        .iter()
        .map(|r| split(r, &col_d))
        .filter(|r| !ignore_empty || !r.is_empty())
        .collect();
    if rows.is_empty() {
        return Err(ErrorValue::Calc);
    }
    let width = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
    let data: Vec<Vec<Value>> = rows
        .into_iter()
        .map(|r| {
            let mut v: Vec<Value> = r.into_iter().map(Value::from).collect();
            v.resize(width, pad.clone());
            v
        })
        .collect();
    from_rows(data)
}

// ── VALUETOTEXT / ARRAYTOTEXT ──────────────────────────────────────────────────────────────

fn value_text(v: &Value, strict: bool) -> String {
    match v {
        Value::Str(s) if strict => format!("\"{}\"", s.replace('"', "\"\"")),
        Value::Error(e) => e.code().to_owned(),
        other => to_text(other).map(|t| t.to_string()).unwrap_or_default(),
    }
}

fn valuetotext(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let v = scalar(args, 0, ctx);
    let f = int(args, 1, ctx, 0.0)?;
    if f != 0.0 && f != 1.0 {
        return Err(ErrorValue::Value);
    }
    Ok(value_text(&v, f == 1.0))
}

fn arraytotext(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let arr = array_of(args, 0, ctx)?;
    let f = scalar_number(args, 1, ctx, 0.0)?;
    if f != 0.0 && f != 1.0 {
        return Err(ErrorValue::Value);
    }
    if f == 0.0 {
        let parts: Vec<String> = arr.values().iter().map(|v| value_text(v, false)).collect();
        return capped(parts.join(", "));
    }
    let rows: Vec<String> = (0..arr.rows())
        .map(|r| {
            (0..arr.cols())
                .map(|c| value_text(arr.get(r, c), true))
                .collect::<Vec<_>>()
                .join(",")
        })
        .collect();
    capped(format!("{{{}}}", rows.join(";")))
}

// ── REGEX ──────────────────────────────────────────────────────────────────────────────────

/// Steps the backtracking matcher may take for one search before giving up with `#VALUE!`.
const BACKTRACK_LIMIT: usize = 1_000_000;

fn regex(pattern: &str, insensitive: bool) -> R<Regex> {
    RegexBuilder::new(pattern)
        .case_insensitive(insensitive)
        .backtrack_limit(BACKTRACK_LIMIT)
        .delegate_size_limit(1 << 22)
        .delegate_dfa_size_limit(1 << 22)
        .build()
        .map_err(|_| ErrorValue::Value)
}

struct RegexArgs {
    text: Arc<str>,
    re: Regex,
}

fn regex_args(args: &mut [Arg<'_>], ctx: &mut dyn CallContext, case_index: usize) -> R<RegexArgs> {
    let t = text(args, 0, ctx)?;
    let pattern = text(args, 1, ctx)?;
    let c = int(args, case_index, ctx, 0.0)?;
    if c != 0.0 && c != 1.0 {
        return Err(ErrorValue::Value);
    }
    Ok(RegexArgs {
        text: t,
        re: regex(&pattern, c == 1.0)?,
    })
}

/// Expands a replacement: PCRE2's `$1`, `${1}`, `${name}` and `$$` (Excel), plus the JavaScript
/// forms `$&`, `$<name>`, `` $` `` and `$'` that v1.0 accepted.
fn expand(repl: &str, caps: &Captures<'_, str>, text: &str, out: &mut String) {
    let whole = caps.get(0).map_or(0..0, |m| m.range());
    let groups = caps.len() - 1;
    let mut it = repl.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if c != '$' {
            out.push(c);
            continue;
        }
        let rest = &repl[i + 1..];
        match rest.chars().next() {
            Some('$') => {
                out.push('$');
                it.next();
            }
            Some('&') => {
                out.push_str(&text[whole.clone()]);
                it.next();
            }
            Some('`') => {
                out.push_str(&text[..whole.start]);
                it.next();
            }
            Some('\'') => {
                out.push_str(&text[whole.end..]);
                it.next();
            }
            Some('{') => match rest.find('}') {
                Some(close) if close > 1 => {
                    let key = &rest[1..close];
                    let m = match key.parse::<usize>() {
                        Ok(n) if n <= groups => caps.get(n),
                        Ok(_) => None,
                        Err(_) => caps.name(key),
                    };
                    if let Some(m) = m {
                        out.push_str(m.as_str());
                    }
                    for _ in 0..close + 1 {
                        it.next();
                    }
                }
                _ => out.push('$'),
            },
            Some('<') => match rest.find('>') {
                Some(close) if caps.name(&rest[1..close]).is_some() || close > 1 => {
                    if let Some(m) = caps.name(&rest[1..close]) {
                        out.push_str(m.as_str());
                    }
                    for _ in 0..close + 1 {
                        it.next();
                    }
                }
                _ => out.push('$'),
            },
            Some(d) if d.is_ascii_digit() => {
                let digits: String = rest
                    .chars()
                    .take(2)
                    .take_while(char::is_ascii_digit)
                    .collect();
                // Prefer a two-digit group when it exists ($10), else one digit ($1).
                let two = digits
                    .parse::<usize>()
                    .ok()
                    .filter(|n| digits.len() == 2 && *n >= 1 && *n <= groups);
                let one = digits[..1]
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n >= 1 && *n <= groups);
                if let Some(n) = two {
                    out.push_str(caps.get(n).map_or("", |m| m.as_str()));
                    it.next();
                    it.next();
                } else if let Some(n) = one {
                    out.push_str(caps.get(n).map_or("", |m| m.as_str()));
                    it.next();
                } else {
                    out.push('$');
                }
            }
            _ => out.push('$'),
        }
    }
}

fn regexreplace(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let a = regex_args(args, ctx, 4)?;
    let repl = text(args, 2, ctx)?;
    let occ = int(args, 3, ctx, 0.0)?;
    let t = &*a.text;
    let all: Vec<Captures<'_, str>> =
        a.re.captures_iter(t)
            .collect::<Result<_, _>>()
            .map_err(|_| ErrorValue::Value)?;
    let chosen: Vec<&Captures<'_, str>> = if occ == 0.0 {
        all.iter().collect()
    } else {
        let idx = if occ > 0.0 {
            occ as i64 - 1
        } else {
            all.len() as i64 + occ as i64
        };
        match usize::try_from(idx).ok().and_then(|i| all.get(i)) {
            Some(c) => vec![c],
            None => return Ok(t.to_owned()),
        }
    };
    let mut out = String::with_capacity(t.len());
    let mut last = 0;
    for caps in chosen {
        let Some(m) = caps.get(0) else { continue };
        out.push_str(&t[last..m.start()]);
        expand(&repl, caps, t, &mut out);
        last = m.end();
    }
    out.push_str(&t[last..]);
    capped(out)
}

fn regexextract(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let a = regex_args(args, ctx, 3)?;
    let mode = int(args, 2, ctx, 0.0)?;
    if !(0.0..=2.0).contains(&mode) {
        return Err(ErrorValue::Value);
    }
    let t = &*a.text;
    if mode == 1.0 {
        let all: Vec<Value> =
            a.re.find_iter(t)
                .map(|m| m.map(|m| Value::from(m.as_str())))
                .collect::<Result<_, _>>()
                .map_err(|_| ErrorValue::Value)?;
        return match all.len() {
            0 => Err(ErrorValue::NA),
            1 => Ok(all.into_iter().next().unwrap_or_default()),
            _ => column(all),
        };
    }
    let caps =
        a.re.captures(t)
            .map_err(|_| ErrorValue::Value)?
            .ok_or(ErrorValue::NA)?;
    if mode == 0.0 {
        return Ok(Value::from(caps.get(0).map_or("", |m| m.as_str())));
    }
    let groups: Vec<Value> = (1..caps.len())
        .map(|i| Value::from(caps.get(i).map_or("", |m| m.as_str())))
        .collect();
    match groups.len() {
        0 => Err(ErrorValue::NA),
        1 => Ok(groups.into_iter().next().unwrap_or_default()),
        _ => row(groups),
    }
}

fn left(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let s = text(args, 0, ctx)?;
    let k = int(args, 1, ctx, 1.0)?;
    if k < 0.0 {
        return Err(ErrorValue::Value);
    }
    Ok(slice16(&s, 0, k.min(1e9) as usize))
}

fn right(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let s = text(args, 0, ctx)?;
    let k = int(args, 1, ctx, 1.0)?;
    if k < 0.0 {
        return Err(ErrorValue::Value);
    }
    let len = len16(&s);
    Ok(from16(&s, len.saturating_sub(k.min(1e9) as usize)))
}

fn mid(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<String> {
    let s = text(args, 0, ctx)?;
    let start = int(args, 1, ctx, 1.0)?;
    let k = int(args, 2, ctx, 0.0)?;
    if start < 1.0 || k < 0.0 {
        return Err(ErrorValue::Value);
    }
    let a = start.min(1e9) as usize - 1;
    Ok(slice16(&s, a, a.saturating_add(k.min(1e9) as usize)))
}

fn len(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<f64> {
    text(args, 0, ctx).map(|s| len16(&s) as f64)
}

/// The text functions.
pub fn specs() -> Vec<FunctionSpec> {
    let all = ArgMask::ALL;
    vec![
        spec("LEN", 1, 1, imp!(len)).lift(all),
        spec("LEFT", 1, 2, imp!(left)).lift(all),
        spec("RIGHT", 1, 2, imp!(right)).lift(all),
        spec("MID", 3, 3, imp!(mid)).lift(all),
        spec("CONCAT", 1, VARIADIC, imp!(concat)),
        spec("TEXTJOIN", 3, VARIADIC, imp!(textjoin)),
        spec("CHAR", 1, 1, imp!(char_fn)).lift(all),
        spec("CODE", 1, 1, imp!(code_fn)).lift(all),
        spec("UNICHAR", 1, 1, imp!(unichar)).lift(all),
        spec("UNICODE", 1, 1, imp!(unicode)).lift(all),
        spec(
            "CLEAN",
            1,
            1,
            imp!(|a, c| text(a, 0, c)
                .map(|s| s.chars().filter(|ch| *ch > '\u{1f}').collect::<String>())),
        )
        .lift(all),
        spec("TRIM", 1, 1, imp!(|a, c| text(a, 0, c).map(|s| trim(&s)))).lift(all),
        spec(
            "LOWER",
            1,
            1,
            imp!(|a, c| text(a, 0, c).map(|s| s.to_lowercase())),
        )
        .lift(all),
        spec(
            "UPPER",
            1,
            1,
            imp!(|a, c| text(a, 0, c).map(|s| s.to_uppercase())),
        )
        .lift(all),
        spec(
            "PROPER",
            1,
            1,
            imp!(|a, c| text(a, 0, c).map(|s| proper(&s))),
        )
        .lift(all),
        spec(
            "EXACT",
            2,
            2,
            imp!(|a, c| {
                let x = text(a, 0, c)?;
                let y = text(a, 1, c)?;
                Ok::<bool, ErrorValue>(x == y)
            }),
        )
        .lift(all),
        spec("FIND", 2, 3, imp!(|a, c| find_impl(a, c, false))).lift(all),
        spec("FINDB", 2, 3, imp!(|a, c| find_impl(a, c, false))).lift(all),
        spec("SEARCH", 2, 3, imp!(|a, c| find_impl(a, c, true))).lift(all),
        spec("SEARCHB", 2, 3, imp!(|a, c| find_impl(a, c, true))).lift(all),
        spec("REPLACE", 4, 4, imp!(replace)).lift(all),
        spec("REPLACEB", 4, 4, imp!(replace)).lift(all),
        spec("SUBSTITUTE", 3, 4, imp!(substitute)).lift(all),
        spec("REPT", 2, 2, imp!(rept)).lift(all),
        spec(
            "T",
            1,
            1,
            imp!(|a, c| match scalar(a, 0, c) {
                v @ (Value::Error(_) | Value::Str(_)) => v,
                _ => Value::from(""),
            }),
        )
        .lift(all),
        spec("CONCATENATE", 1, VARIADIC, imp!(concatenate)).lift(all),
        spec("TEXT", 2, 2, imp!(text_fn)).lift(all),
        spec("DOLLAR", 1, 2, imp!(dollar)).lift(all),
        spec("FIXED", 1, 3, imp!(fixed)).lift(all),
        spec("VALUE", 1, 1, imp!(value_fn)).lift(all),
        spec("NUMBERVALUE", 1, 3, imp!(numbervalue)).lift(all),
        spec("LENB", 1, 1, imp!(len)).lift(all),
        spec("LEFTB", 1, 2, imp!(left)).lift(all),
        spec("RIGHTB", 1, 2, imp!(right)).lift(all),
        spec("MIDB", 3, 3, imp!(mid)).lift(all),
        spec(
            "TEXTBEFORE",
            2,
            6,
            imp!(|a, c| text_before_after(a, c, false)),
        ),
        spec(
            "TEXTAFTER",
            2,
            6,
            imp!(|a, c| text_before_after(a, c, true)),
        ),
        spec("TEXTSPLIT", 2, 6, imp!(textsplit)),
        spec("VALUETOTEXT", 1, 2, imp!(valuetotext)).lift(all),
        spec("ARRAYTOTEXT", 1, 2, imp!(arraytotext)),
        spec(
            "REGEXTEST",
            2,
            3,
            imp!(|a, c| regex_args(a, c, 2)
                .and_then(|r| r.re.is_match(&*r.text).map_err(|_| ErrorValue::Value))),
        )
        .lift(all),
        spec("REGEXREPLACE", 3, 5, imp!(regexreplace)).lift(ArgMask::of(&[0, 1, 2, 3, 4])),
        spec("REGEXEXTRACT", 2, 4, imp!(regexextract)).lift(ArgMask::of(&[0, 1, 3])),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_and_proper() {
        assert_eq!(trim("  a   b  "), "a b");
        assert_eq!(trim(""), "");
        assert_eq!(proper("this is a TITLE"), "This Is A Title");
        assert_eq!(proper("2-way street"), "2-Way Street");
    }

    #[test]
    fn plain_numbers() {
        for ok in ["1", "-1.5", ".5", "5.", "1e3", "+2E-2"] {
            assert!(is_plain_number(ok), "{ok}");
        }
        for bad in ["", ".", "e3", "1e", "1.2.3", "--1"] {
            assert!(!is_plain_number(bad), "{bad}");
        }
    }

    #[test]
    fn replacements() {
        let re = Regex::new(r"(\d+)-(?<b>\d+)").expect("valid pattern");
        let text = "a 12-34 b";
        let caps = re.captures(text).ok().flatten().expect("a match");
        let mut out = String::new();
        expand(
            "[$2/$1/$<b>/$&/$$/$9/${1}/${b}/${7}]",
            &caps,
            text,
            &mut out,
        );
        assert_eq!(out, "[34/12/34/12-34/$/$9/12/34/]");
    }

    #[test]
    fn pcre_features_and_the_backtracking_limit() {
        let look = regex(r"\d+(?= kg)", false).expect("look-ahead");
        assert_eq!(
            look.find("5 lb, 12 kg").ok().flatten().map(|m| m.as_str()),
            Some("12")
        );
        let back = regex(r"(\w)\1", false).expect("back-reference");
        assert_eq!(back.is_match("hello").ok(), Some(true));
        assert_eq!(regex("(", false).err(), Some(ErrorValue::Value));
        // Catastrophic backtracking stops at the limit instead of hanging.
        let evil = regex(r"(a|b|ab)*\1c", false).expect("pattern");
        assert!(evil.is_match(&"ab".repeat(30)).is_err());
    }
}
