// SPDX-License-Identifier: AGPL-3.0-or-later
//! Breaks a number format code into tokens.

/// A comparison in a condition section (`[>=100]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CondOp {
    /// `=`
    Eq,
    /// `<>`
    Ne,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
}

impl CondOp {
    pub(crate) fn test(self, value: f64, operand: f64) -> bool {
        match self {
            CondOp::Eq => value == operand,
            CondOp::Ne => value != operand,
            CondOp::Lt => value < operand,
            CondOp::Le => value <= operand,
            CondOp::Gt => value > operand,
            CondOp::Ge => value >= operand,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Tok {
    General,
    Hash,
    Zero,
    Qmark,
    Slash,
    Group,
    Scale,
    Comma,
    Break,
    Text,
    Plus,
    Minus,
    Point,
    Space,
    Percent,
    Digit(char),
    Calendar(String),
    Error,
    DateTime(String),
    Duration(String),
    Condition(CondOp, f64),
    DbNum,
    Locale(String),
    Color(String),
    Modifier,
    AmPm(String),
    Escaped(String),
    Str(String),
    Skip(String),
    Exp(char),
    Fill(String),
    Paren(char),
    Char(String),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub(crate) tok: Tok,
    pub(crate) raw: String,
}

fn is_num_op(c: Option<char>) -> bool {
    c.is_some_and(|c| c == '?' || c == '#' || c.is_ascii_digit())
}

/// Length in bytes of a run of characters satisfying `f` at the start of `s`.
fn run_len(s: &str, f: impl Fn(char) -> bool) -> usize {
    s.char_indices()
        .find(|&(_, c)| !f(c))
        .map_or(s.len(), |(i, _)| i)
}

fn bracket_body(s: &str) -> Option<&str> {
    let rest = s.strip_prefix('[')?;
    let end = rest.find(']')?;
    Some(&rest[..end])
}

/// `[<=100]`, `[>5]`, `[=-1.5]`: `\[(<[=>]?|>=?|=)\s*(-?[.\d]+)\]`.
fn condition(body: &str) -> Option<(CondOp, f64)> {
    let (op, rest) = if let Some(r) = body.strip_prefix("<=") {
        (CondOp::Le, r)
    } else if let Some(r) = body.strip_prefix("<>") {
        (CondOp::Ne, r)
    } else if let Some(r) = body.strip_prefix('<') {
        (CondOp::Lt, r)
    } else if let Some(r) = body.strip_prefix(">=") {
        (CondOp::Ge, r)
    } else if let Some(r) = body.strip_prefix('>') {
        (CondOp::Gt, r)
    } else {
        (CondOp::Eq, body.strip_prefix('=')?)
    };
    let rest = rest.trim_start();
    let num = rest.strip_prefix('-').unwrap_or(rest);
    if num.is_empty() || !num.chars().all(|c| c == '.' || c.is_ascii_digit()) {
        return None;
    }
    // parseFloat semantics: the longest valid prefix.
    let mut end = 0;
    let mut seen_point = false;
    for (i, c) in rest.char_indices() {
        if i == 0 && c == '-' {
            end = 1;
            continue;
        }
        if c == '.' && !seen_point {
            seen_point = true;
            end = i + 1;
        } else if c.is_ascii_digit() {
            end = i + 1;
        } else {
            break;
        }
    }
    let v: f64 = rest[..end].trim_end_matches('.').parse().unwrap_or(0.0);
    Some((op, v))
}

const COLORS: [&str; 8] = [
    "black", "blue", "cyan", "green", "magenta", "red", "white", "yellow",
];

fn is_color(body: &str) -> bool {
    let l = body.to_ascii_lowercase();
    if COLORS.contains(&l.as_str()) {
        return true;
    }
    l.strip_prefix("color").is_some_and(|rest| {
        let rest = rest.trim_start();
        !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit())
    })
}

fn is_error_char(c: char) -> bool {
    "EÈÉÊËèéêëĒēĔĕĖėĘęĚěȄȅȆȇȨȩNnÑñŃńŅņŇňǸǹ[\"*/\\_".contains(c)
}

/// Matches one non-comma token at the start of `s`: `(token, byte length)`.
fn one(s: &str) -> (Tok, usize) {
    let mut chars = s.chars();
    let Some(c) = chars.next() else {
        return (Tok::Error, 0);
    };
    let next = chars.next();
    if s.get(..7)
        .is_some_and(|g| g.eq_ignore_ascii_case("general"))
    {
        return (Tok::General, 7);
    }
    match c {
        '#' => return (Tok::Hash, 1),
        '0' => return (Tok::Zero, 1),
        '?' => return (Tok::Qmark, 1),
        '/' => return (Tok::Slash, 1),
        ';' => return (Tok::Break, 1),
        '@' => return (Tok::Text, 1),
        '+' => return (Tok::Plus, 1),
        '-' => return (Tok::Minus, 1),
        '.' => return (Tok::Point, 1),
        ' ' => return (Tok::Space, 1),
        '%' => return (Tok::Percent, 1),
        '1'..='9' => return (Tok::Digit(c), 1),
        _ => {}
    }
    if (c == 'B' || c == 'b') && matches!(next, Some('1' | '2')) {
        return (Tok::Calendar(s[..2].to_owned()), 2);
    }
    if s == "B" {
        return (Tok::Error, 1);
    }
    // Date/time runs: h, m, s, y, b, d, g (either case), aaa+, e (lower case only).
    let lower = c.to_ascii_lowercase();
    if matches!(lower, 'h' | 'm' | 's' | 'y' | 'b' | 'd' | 'g') {
        let n = run_len(s, |x| x.to_ascii_lowercase() == lower);
        return (Tok::DateTime(s[..n].to_owned()), n);
    }
    if lower == 'a' {
        let n = run_len(s, |x| x == 'a' || x == 'A');
        if n >= 3 {
            return (Tok::DateTime(s[..n].to_owned()), n);
        }
    }
    if c == 'e' {
        let n = run_len(s, |x| x == 'e');
        return (Tok::DateTime(s[..n].to_owned()), n);
    }
    if c == '['
        && let Some(body) = bracket_body(s)
        && !body.is_empty()
    {
        let n = body.len() + 2;
        let lb = body.to_ascii_lowercase();
        if !lb.is_empty()
            && let Some(first) = lb.chars().next()
            && matches!(first, 'h' | 'm' | 's')
            && lb.chars().all(|x| x == first)
        {
            return (Tok::Duration(lb), n);
        }
        if let Some((op, v)) = condition(body) {
            return (Tok::Condition(op, v), n);
        }
        let ul = body.to_ascii_uppercase();
        let dbnum = |p: &str| {
            ul.strip_prefix(p).is_some_and(|d| {
                !d.is_empty()
                    && d.len() <= 2
                    && d.chars().all(|x| x.is_ascii_digit())
                    && (d.len() == 1 || ('0'..='4').contains(&d.chars().next().unwrap_or('9')))
            })
        };
        if dbnum("DBNUM") || dbnum("NATNUM") {
            return (Tok::DbNum, n);
        }
        if let Some(rest) = body.strip_prefix('$')
            && !rest.is_empty()
        {
            return (Tok::Locale(rest.to_owned()), n);
        }
        if is_color(body) {
            return (Tok::Color(body.to_owned()), n);
        }
        return (Tok::Modifier, n);
    }
    for ampm in ["AM/PM", "am/pm", "A/P"] {
        if s.starts_with(ampm) {
            return (Tok::AmPm(ampm.to_owned()), ampm.len());
        }
    }
    if c == '\\'
        && let Some(n) = next
    {
        return (Tok::Escaped(n.to_string()), 1 + n.len_utf8());
    }
    if c == '"'
        && let Some(end) = s[1..].find('"')
    {
        return (Tok::Str(s[1..1 + end].to_owned()), end + 2);
    }
    if c == '_'
        && let Some(n) = next
    {
        let mut len = 1 + n.len_utf8();
        let mut v = n.to_string();
        if n == '\\'
            && let Some(m) = s[len..].chars().next()
        {
            v.push(m);
            len += m.len_utf8();
        }
        return (Tok::Skip(v), len);
    }
    if (c == 'E' || c == 'e') && matches!(next, Some('+' | '-')) {
        return (Tok::Exp(next.unwrap_or('+')), 2);
    }
    if c == '*'
        && let Some(n) = next
    {
        let mut len = 1 + n.len_utf8();
        let mut v = n.to_string();
        if n == '\\'
            && let Some(m) = s[len..].chars().next()
        {
            v.push(m);
            len += m.len_utf8();
        }
        return (Tok::Fill(v), len);
    }
    if c == '(' || c == ')' {
        return (Tok::Paren(c), 1);
    }
    if is_error_char(c) {
        return (Tok::Error, c.len_utf8());
    }
    (Tok::Char(c.to_string()), c.len_utf8())
}

/// Splits a format code into tokens. Commas are resolved by context into thousands separators,
/// scaling commas or literal commas, as Excel does.
pub(crate) fn tokenize(pattern: &str) -> Vec<Token> {
    let mut tokens: Vec<Token> = Vec::new();
    let mut unresolved: Vec<usize> = Vec::new();
    let mut i = 0;
    while i < pattern.len() {
        let curr = &pattern[i..];
        if curr.starts_with(',') {
            let n = run_len(curr, |c| c == ',');
            let raw = curr[..n].to_owned();
            let look_behind = pattern[..i].chars().last();
            let look_ahead = curr[n..].chars().next();
            let (mut maybe_group, mut maybe_scale) = (false, false);
            if is_num_op(look_behind) {
                maybe_group = true;
                maybe_scale = true;
            } else if look_behind == Some('.') {
                maybe_scale = true;
            }
            if maybe_group && (look_ahead.is_none() || look_ahead == Some(';')) {
                maybe_group = false;
            }
            if maybe_scale && is_num_op(look_ahead) {
                maybe_scale = false;
            }
            let tok = match (maybe_group, maybe_scale) {
                (true, false) => Tok::Group,
                (false, true) => Tok::Scale,
                (true, true) => {
                    unresolved.push(tokens.len());
                    Tok::Scale
                }
                (false, false) => Tok::Comma,
            };
            tokens.push(Token { tok, raw });
            i += n;
            continue;
        }
        let (tok, n) = one(curr);
        let n = n.max(curr.chars().next().map_or(1, char::len_utf8));
        let raw = curr[..n].to_owned();
        if !unresolved.is_empty() && raw == ";" {
            unresolved.clear();
        }
        if !unresolved.is_empty() && is_num_op(raw.chars().next()) {
            for &u in &unresolved {
                tokens[u].tok = Tok::Group;
            }
            unresolved.clear();
        }
        tokens.push(Token { tok, raw });
        i += n;
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(p: &str) -> Vec<Tok> {
        tokenize(p).into_iter().map(|t| t.tok).collect()
    }

    #[test]
    fn commas_resolve_by_context() {
        assert_eq!(
            kinds("#,##0"),
            [Tok::Hash, Tok::Group, Tok::Hash, Tok::Hash, Tok::Zero]
        );
        assert_eq!(kinds("0,"), [Tok::Zero, Tok::Scale]);
        assert_eq!(
            kinds("0.0,,"),
            [Tok::Zero, Tok::Point, Tok::Zero, Tok::Scale]
        );
        assert_eq!(
            kinds("x,z"),
            [Tok::Char("x".into()), Tok::Comma, Tok::Char("z".into())]
        );
    }

    #[test]
    fn brackets_and_dates() {
        assert_eq!(kinds("[Red]"), [Tok::Color("Red".into())]);
        assert_eq!(kinds("[>=100]"), [Tok::Condition(CondOp::Ge, 100.0)]);
        assert_eq!(kinds("[h]"), [Tok::Duration("h".into())]);
        assert_eq!(kinds("[$-409]"), [Tok::Locale("-409".into())]);
        assert_eq!(
            kinds("yyyy-mm"),
            [
                Tok::DateTime("yyyy".into()),
                Tok::Minus,
                Tok::DateTime("mm".into())
            ]
        );
        assert_eq!(kinds("AM/PM"), [Tok::AmPm("AM/PM".into())]);
        assert_eq!(kinds("0.0E+0")[3], Tok::Exp('+'));
    }
}
