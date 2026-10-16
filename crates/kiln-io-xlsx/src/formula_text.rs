// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formula text as stored in a file ↔ formula text in the model.
//!
//! SpreadsheetML writes functions newer than Excel 2007 with a `_xlfn.` prefix
//! (`_xlfn.XLOOKUP`), the dynamic-array sort and filter functions with `_xlfn._xlws.` and
//! `LET`/`LAMBDA` parameter names with `_xlpm.`. Excel shows `#NAME?` when the prefixes are
//! missing, so the reader strips them (the model holds what the user types) and the writer puts
//! them back. Text inside string literals, quoted sheet names and brackets is never touched.
//! Ported from the v1.0 TypeScript reader (`reference/ts-core/io-xlsx/src/formula-text.ts`).

use std::collections::{BTreeSet, HashMap};

/// Functions Excel stores with the `_xlfn.` prefix ("future functions", MS-XLSX §2.2.2).
pub const FUTURE_FUNCTIONS: &[&str] = &[
    "ACOT",
    "ACOTH",
    "AGGREGATE",
    "ANCHORARRAY",
    "ARABIC",
    "ARRAYTOTEXT",
    "BASE",
    "BETA.DIST",
    "BETA.INV",
    "BINOM.DIST",
    "BINOM.DIST.RANGE",
    "BINOM.INV",
    "BITAND",
    "BITLSHIFT",
    "BITOR",
    "BITRSHIFT",
    "BITXOR",
    "BYCOL",
    "BYROW",
    "CEILING.MATH",
    "CEILING.PRECISE",
    "CHISQ.DIST",
    "CHISQ.DIST.RT",
    "CHISQ.INV",
    "CHISQ.INV.RT",
    "CHISQ.TEST",
    "CHOOSECOLS",
    "CHOOSEROWS",
    "COMBINA",
    "CONCAT",
    "CONFIDENCE.NORM",
    "CONFIDENCE.T",
    "COT",
    "COTH",
    "COVARIANCE.P",
    "COVARIANCE.S",
    "CSC",
    "CSCH",
    "DAYS",
    "DECIMAL",
    "DROP",
    "ECMA.CEILING",
    "ENCODEURL",
    "ERF.PRECISE",
    "ERFC.PRECISE",
    "EXPAND",
    "EXPON.DIST",
    "F.DIST",
    "F.DIST.RT",
    "F.INV",
    "F.INV.RT",
    "F.TEST",
    "FIELDVALUE",
    "FILTERXML",
    "FLOOR.MATH",
    "FLOOR.PRECISE",
    "FORECAST.ETS",
    "FORECAST.ETS.CONFINT",
    "FORECAST.ETS.SEASONALITY",
    "FORECAST.ETS.STAT",
    "FORECAST.LINEAR",
    "FORMULATEXT",
    "GAMMA",
    "GAMMA.DIST",
    "GAMMA.INV",
    "GAMMALN.PRECISE",
    "GAUSS",
    "GROUPBY",
    "HSTACK",
    "HYPGEOM.DIST",
    "IFNA",
    "IFS",
    "IMAGE",
    "IMCOSH",
    "IMCOT",
    "IMCSC",
    "IMCSCH",
    "IMSEC",
    "IMSECH",
    "IMSINH",
    "IMTAN",
    "ISFORMULA",
    "ISO.CEILING",
    "ISOMITTED",
    "ISOWEEKNUM",
    "LAMBDA",
    "LET",
    "LOGNORM.DIST",
    "LOGNORM.INV",
    "MAKEARRAY",
    "MAP",
    "MAXIFS",
    "MINIFS",
    "MODE.MULT",
    "MODE.SNGL",
    "MUNIT",
    "NEGBINOM.DIST",
    "NETWORKDAYS.INTL",
    "NORM.DIST",
    "NORM.INV",
    "NORM.S.DIST",
    "NORM.S.INV",
    "NUMBERVALUE",
    "PDURATION",
    "PERCENTILE.EXC",
    "PERCENTILE.INC",
    "PERCENTOF",
    "PERCENTRANK.EXC",
    "PERCENTRANK.INC",
    "PERMUTATIONA",
    "PHI",
    "PIVOTBY",
    "POISSON.DIST",
    "QUARTILE.EXC",
    "QUARTILE.INC",
    "QUERYSTRING",
    "RANDARRAY",
    "RANK.AVG",
    "RANK.EQ",
    "REDUCE",
    "REGEXEXTRACT",
    "REGEXREPLACE",
    "REGEXTEST",
    "RRI",
    "SCAN",
    "SEC",
    "SECH",
    "SEQUENCE",
    "SHEET",
    "SHEETS",
    "SINGLE",
    "SKEW.P",
    "SORTBY",
    "STDEV.P",
    "STDEV.S",
    "STOCKHISTORY",
    "SWITCH",
    "T.DIST",
    "T.DIST.2T",
    "T.DIST.RT",
    "T.INV",
    "T.INV.2T",
    "T.TEST",
    "TAKE",
    "TEXTAFTER",
    "TEXTBEFORE",
    "TEXTJOIN",
    "TEXTSPLIT",
    "TOCOL",
    "TOROW",
    "TRIMRANGE",
    "UNICHAR",
    "UNICODE",
    "UNIQUE",
    "VALUETOTEXT",
    "VAR.P",
    "VAR.S",
    "VSTACK",
    "WEBSERVICE",
    "WEIBULL.DIST",
    "WORKDAY.INTL",
    "WRAPCOLS",
    "WRAPROWS",
    "XLOOKUP",
    "XMATCH",
    "XOR",
    "Z.TEST",
];

/// Functions stored as `_xlfn._xlws.NAME`.
pub const XLWS_FUNCTIONS: &[&str] = &["FILTER", "SORT"];

/// Whether `name` (upper case) is a future function.
#[must_use]
pub fn is_future_function(name: &str) -> bool {
    FUTURE_FUNCTIONS.binary_search(&name).is_ok()
}

#[derive(Debug, Clone)]
struct Tok {
    start: usize,
    end: usize,
    /// Identifier text (`None` for punctuation).
    ident: Option<String>,
    /// The identifier is followed by `(`.
    call: bool,
    /// `(`, `)`, `,` or `;` outside array constants.
    punct: Option<char>,
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '\\' || !c.is_ascii()
}

fn is_ident_char(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit() || c == '.' || c == '?'
}

/// Splits formula text into identifiers and structural punctuation; literals are skipped.
fn scan(text: &str) -> Vec<Tok> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let at = |i: usize| chars.get(i).map(|&(_, c)| c);
    let offset = |i: usize| chars.get(i).map_or(text.len(), |&(o, _)| o);
    let mut toks = Vec::new();
    let mut i = 0usize;
    let mut braces = 0i32;
    while i < n {
        let c = chars[i].1;
        match c {
            '"' | '\'' => {
                // "string" or 'quoted sheet' with doubled escapes
                i += 1;
                while i < n {
                    if chars[i].1 == c {
                        if at(i + 1) == Some(c) {
                            i += 2;
                        } else {
                            break;
                        }
                    } else {
                        i += 1;
                    }
                }
                i += 1;
                continue;
            }
            '[' => {
                // [bracketed] structured reference or external book index, possibly nested
                let mut depth = 0i32;
                while i < n {
                    let d = chars[i].1;
                    if d == '\'' && i + 1 < n {
                        i += 2;
                        continue;
                    }
                    if d == '[' {
                        depth += 1;
                    } else if d == ']' {
                        depth -= 1;
                        if depth == 0 {
                            i += 1;
                            break;
                        }
                    }
                    i += 1;
                }
                continue;
            }
            '{' => braces += 1,
            '}' => braces -= 1,
            _ => {}
        }
        if c == '(' || c == ')' || ((c == ',' || c == ';') && braces == 0) {
            toks.push(Tok {
                start: offset(i),
                end: offset(i + 1),
                ident: None,
                call: false,
                punct: Some(c),
            });
            i += 1;
            continue;
        }
        if is_ident_start(c) {
            let start = i;
            while i < n && is_ident_char(chars[i].1) {
                i += 1;
            }
            let mut j = i;
            while j < n && matches!(chars[j].1, ' ' | '\n' | '\r') {
                j += 1;
            }
            let call = at(j) == Some('(');
            // Identifiers glued to a number (`1E+5`) are not names.
            if start > 0 && chars[start - 1].1.is_ascii_digit() {
                continue;
            }
            toks.push(Tok {
                start: offset(start),
                end: offset(i),
                ident: Some(text[offset(start)..offset(i)].to_owned()),
                call,
                punct: None,
            });
            continue;
        }
        i += 1;
    }
    toks
}

/// The length of a `_xlfn._xlws.`, `_xlfn.`, `_xlws.` or `_xlpm.` prefix at the start of `ident`
/// (ignoring case), with whether it is the parameter prefix.
fn prefix_len(ident: &str) -> Option<(usize, bool)> {
    let lower = ident.get(..12).map(str::to_ascii_lowercase);
    if lower.as_deref() == Some("_xlfn._xlws.") {
        return Some((12, false));
    }
    let head = ident.get(..6).map(str::to_ascii_lowercase)?;
    match head.as_str() {
        "_xlfn." | "_xlws." => Some((6, false)),
        "_xlpm." => Some((6, true)),
        _ => None,
    }
}

/// File formula → model formula: strips `_xlfn.`, `_xlfn._xlws.` and `_xlpm.` prefixes.
/// Names of prefixed functions are added to `seen` (so the writer can restore prefixes of
/// functions it does not know).
#[must_use]
pub fn from_file_formula(text: &str, mut seen: Option<&mut BTreeSet<String>>) -> String {
    if !text.contains("_x") && !text.contains("_X") {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;
    for t in scan(text) {
        let Some(ident) = &t.ident else { continue };
        let Some((len, param)) = prefix_len(ident) else {
            continue;
        };
        let bare = &ident[len..];
        if t.call
            && !param
            && let Some(s) = seen.as_deref_mut()
        {
            s.insert(bare.to_ascii_uppercase());
        }
        out.push_str(&text[last..t.start]);
        out.push_str(bare);
        last = t.end;
    }
    if last == 0 {
        return text.to_owned();
    }
    out.push_str(&text[last..]);
    out
}

#[derive(Debug, Default)]
struct Frame {
    name: String,
    arg: usize,
    /// Per argument: the token index of its only identifier (a parameter candidate).
    arg_ident: Vec<Option<usize>>,
    arg_tokens: Vec<usize>,
    params: BTreeSet<String>,
}

fn strip_prefix_upper(ident: &str) -> String {
    let bare = prefix_len(ident).map_or(ident, |(n, _)| &ident[n..]);
    bare.to_ascii_uppercase()
}

/// Model formula → file formula: adds `_xlfn.` / `_xlfn._xlws.` to future functions and
/// `_xlpm.` to `LET`/`LAMBDA` parameter names. `extra` lists more function names to prefix (seen
/// when reading).
#[must_use]
pub fn to_file_formula(text: &str, extra: Option<&BTreeSet<String>>) -> String {
    if !text.contains('(') {
        return text.to_owned();
    }
    let toks = scan(text);
    let mut stack: Vec<usize> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    let finish = |f: &mut Frame, toks: &[Tok]| {
        let bare = strip_prefix_upper(&f.name);
        let last = f.arg;
        let ident_at = |a: usize| {
            f.arg_ident
                .get(a)
                .copied()
                .flatten()
                .and_then(|k| toks[k].ident.as_deref())
                .map(strip_prefix_upper)
        };
        let names: Vec<String> = match bare.as_str() {
            "LET" => (0..last).step_by(2).filter_map(ident_at).collect(),
            "LAMBDA" => (0..last).filter_map(ident_at).collect(),
            _ => Vec::new(),
        };
        f.params.extend(names);
    };
    for (k, t) in toks.iter().enumerate() {
        match t.punct {
            Some('(') => {
                let name = k
                    .checked_sub(1)
                    .and_then(|p| toks.get(p))
                    .filter(|p| p.call && p.end <= t.start)
                    .and_then(|p| p.ident.clone())
                    .unwrap_or_default();
                frames.push(Frame {
                    name: name.to_ascii_uppercase(),
                    arg_ident: vec![None],
                    arg_tokens: vec![0],
                    ..Frame::default()
                });
                stack.push(frames.len() - 1);
            }
            Some(')') => {
                if let Some(f) = stack.pop() {
                    finish(&mut frames[f], &toks);
                }
            }
            Some(',') => {
                if let Some(&f) = stack.last() {
                    let fr = &mut frames[f];
                    fr.arg += 1;
                    fr.arg_ident.push(None);
                    fr.arg_tokens.push(0);
                }
            }
            _ => {
                if t.ident.is_some()
                    && let Some(&f) = stack.last()
                {
                    let fr = &mut frames[f];
                    let a = fr.arg;
                    fr.arg_tokens[a] += 1;
                    fr.arg_ident[a] = (fr.arg_tokens[a] == 1 && !t.call).then_some(k);
                }
            }
        }
    }
    while let Some(f) = stack.pop() {
        finish(&mut frames[f], &toks);
    }
    // Second pass: walk the tokens again, keeping a count of how many open frames bind each
    // parameter name. Each frame's names are added once when it opens and removed once when it
    // closes, so this is linear in the formula length whatever the nesting depth (hostile
    // formulas nest tens of thousands of parentheses deep).
    let mut out = String::with_capacity(text.len() + 16);
    let mut last = 0usize;
    let mut in_scope: HashMap<&str, usize> = HashMap::new();
    let mut open: Vec<usize> = Vec::new();
    let mut next_frame = 0usize;
    for t in &toks {
        match t.punct {
            Some('(') => {
                let f = next_frame;
                next_frame += 1;
                if let Some(fr) = frames.get(f) {
                    for p in &fr.params {
                        *in_scope.entry(p.as_str()).or_insert(0) += 1;
                    }
                }
                open.push(f);
                continue;
            }
            Some(')') => {
                if let Some(fr) = open.pop().and_then(|f| frames.get(f)) {
                    for p in &fr.params {
                        if let Some(n) = in_scope.get_mut(p.as_str()) {
                            *n = n.saturating_sub(1);
                        }
                    }
                }
                continue;
            }
            _ => {}
        }
        let Some(ident) = &t.ident else { continue };
        if prefix_len(ident).is_some() {
            continue;
        }
        let upper = ident.to_ascii_uppercase();
        let prefix = if t.call {
            if XLWS_FUNCTIONS.contains(&upper.as_str()) {
                Some("_xlfn._xlws.")
            } else if is_future_function(&upper) || extra.is_some_and(|e| e.contains(&upper)) {
                Some("_xlfn.")
            } else {
                None
            }
        } else if in_scope.get(upper.as_str()).is_some_and(|&n| n > 0) {
            Some("_xlpm.")
        } else {
            None
        };
        let Some(prefix) = prefix else { continue };
        out.push_str(&text[last..t.start]);
        out.push_str(prefix);
        out.push_str(ident);
        last = t.end;
    }
    if last == 0 {
        return text.to_owned();
    }
    out.push_str(&text[last..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn future_functions_are_sorted_for_binary_search() {
        assert!(FUTURE_FUNCTIONS.windows(2).all(|w| w[0] < w[1]));
        assert!(is_future_function("XLOOKUP"));
        assert!(!is_future_function("SUM"));
    }

    #[test]
    fn strips_prefixes_outside_literals() {
        let mut seen = BTreeSet::new();
        assert_eq!(
            from_file_formula(
                "_xlfn.XLOOKUP(A1,B:B,C:C)&\"_xlfn.X\"&'_xlfn.S'!A1",
                Some(&mut seen)
            ),
            "XLOOKUP(A1,B:B,C:C)&\"_xlfn.X\"&'_xlfn.S'!A1"
        );
        assert!(seen.contains("XLOOKUP"));
        assert_eq!(
            from_file_formula("_xlfn._xlws.SORT(A1:A9)", None),
            "SORT(A1:A9)"
        );
        assert_eq!(
            from_file_formula("_xlfn.LET(_xlpm.x,1,_xlpm.x+1)", None),
            "LET(x,1,x+1)"
        );
        assert_eq!(from_file_formula("SUM(A1)", None), "SUM(A1)");
    }

    #[test]
    fn adds_prefixes_and_round_trips() {
        for (model, file) in [
            ("XLOOKUP(1,A:A,B:B)", "_xlfn.XLOOKUP(1,A:A,B:B)"),
            ("SORT(A1:A3)", "_xlfn._xlws.SORT(A1:A3)"),
            ("FILTER(A1:A3,B1:B3>0)", "_xlfn._xlws.FILTER(A1:A3,B1:B3>0)"),
            (
                "LET(x,1,y,2,x+y)",
                "_xlfn.LET(_xlpm.x,1,_xlpm.y,2,_xlpm.x+_xlpm.y)",
            ),
            (
                "MAP(A1:A3,LAMBDA(v,v*2))",
                "_xlfn.MAP(A1:A3,_xlfn.LAMBDA(_xlpm.v,_xlpm.v*2))",
            ),
            ("SUM(A1:A2)+\"XLOOKUP(\"", "SUM(A1:A2)+\"XLOOKUP(\""),
            ("'XLOOKUP(x'!A1", "'XLOOKUP(x'!A1"),
            (
                "Table1[[#This Row],[SEQUENCE(]]",
                "Table1[[#This Row],[SEQUENCE(]]",
            ),
        ] {
            assert_eq!(to_file_formula(model, None), file, "{model}");
            assert_eq!(from_file_formula(file, None), model, "{file}");
        }
    }

    #[test]
    fn extra_names_and_cells_like_names_are_handled() {
        let extra: BTreeSet<String> = ["NEWFN".to_owned()].into();
        assert_eq!(to_file_formula("NEWFN(1)", Some(&extra)), "_xlfn.NEWFN(1)");
        // A cell reference followed by `(` is not a call candidate that changes.
        assert_eq!(to_file_formula("SUM(A1)*2", None), "SUM(A1)*2");
        // Already prefixed text stays as is.
        assert_eq!(
            to_file_formula("_xlfn.XLOOKUP(1,A:A,B:B)", None),
            "_xlfn.XLOOKUP(1,A:A,B:B)"
        );
        // Non-ASCII text is kept intact.
        assert_eq!(
            to_file_formula("CONCAT(\"é\",Ü1)", None),
            "_xlfn.CONCAT(\"é\",Ü1)"
        );
    }
}
