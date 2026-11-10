// SPDX-License-Identifier: AGPL-3.0-or-later
//! Parses format codes into sections ("partitions"): positive; negative; zero; text — or up to two
//! conditional sections plus a fallback.

use super::tokenize::{CondOp, Tok, Token, tokenize};

pub(crate) const U_YEAR: u32 = 2;
pub(crate) const U_MONTH: u32 = 4;
pub(crate) const U_DAY: u32 = 8;
pub(crate) const U_HOUR: u32 = 16;
pub(crate) const U_MIN: u32 = 32;
pub(crate) const U_SEC: u32 = 64;
pub(crate) const U_DSEC: u32 = 128;
pub(crate) const U_CSEC: u32 = 256;
pub(crate) const U_MSEC: u32 = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumKind {
    Int,
    Frac,
    Man,
    Num,
    Den,
}

/// Whitespace rules around the parts of a vulgar fraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Rule {
    None,
    Num,
    NumInt,
    Den,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DateKind {
    Year,
    YearShort,
    BYear,
    BYearShort,
    Day,
    WeekdayShort,
    Weekday,
    Hour,
    MonthNameShort,
    MonthNameSingle,
    MonthName,
    Min,
    Month,
    Sec,
    HourElap,
    MinElap,
    SecElap,
    /// `g` era codes (not displayed).
    Era,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Out {
    Str { value: String, rule: Rule },
    General,
    Number { kind: NumKind, num: String },
    Div,
    Space { rule: Rule },
    Text,
    Plus,
    Minus { volatile: bool },
    Point,
    Exp,
    Skip,
    Fill,
    Date(DateBit),
    Subsec { decimals: usize },
    AmPm { short: bool },
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DateBit {
    pub(crate) kind: DateKind,
    pub(crate) size: u32,
    /// Zero-pad to two digits (or, for elapsed time, the width).
    pub(crate) pad: usize,
    used: bool,
    indeterminate: bool,
}

/// A section colour: a named colour or a palette index (`[Color 10]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatColor {
    /// `[Black]`
    Black,
    /// `[Blue]`
    Blue,
    /// `[Cyan]`
    Cyan,
    /// `[Green]`
    Green,
    /// `[Magenta]`
    Magenta,
    /// `[Red]`
    Red,
    /// `[White]`
    White,
    /// `[Yellow]`
    Yellow,
    /// `[ColorN]`, N from 1 to 56 in Excel's legacy palette.
    Indexed(u8),
}

/// Excel's legacy 56-colour palette.
const INDEX_COLORS: [u32; 56] = [
    0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000,
    0x008000, 0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366,
    0xFFFFCC, 0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00,
    0x00FFFF, 0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99,
    0x99CCFF, 0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900,
    0xFF6600, 0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366,
    0x333399, 0x333333,
];

impl FormatColor {
    /// The colour as `0xRRGGBB`.
    pub fn rgb(self) -> u32 {
        match self {
            FormatColor::Black => 0x000000,
            FormatColor::Blue => 0x0000FF,
            FormatColor::Cyan => 0x00FFFF,
            FormatColor::Green => 0x00FF00,
            FormatColor::Magenta => 0xFF00FF,
            FormatColor::Red => 0xFF0000,
            FormatColor::White => 0xFFFFFF,
            FormatColor::Yellow => 0xFFFF00,
            FormatColor::Indexed(n) => INDEX_COLORS
                .get(usize::from(n).wrapping_sub(1))
                .copied()
                .unwrap_or(0x000000),
        }
    }

    fn parse(name: &str) -> Option<FormatColor> {
        let l = name.to_ascii_lowercase();
        Some(match l.as_str() {
            "black" => FormatColor::Black,
            "blue" => FormatColor::Blue,
            "cyan" => FormatColor::Cyan,
            "green" => FormatColor::Green,
            "magenta" => FormatColor::Magenta,
            "red" => FormatColor::Red,
            "white" => FormatColor::White,
            "yellow" => FormatColor::Yellow,
            _ => {
                let n: u32 = l.strip_prefix("color")?.trim().parse().ok()?;
                FormatColor::Indexed(u8::try_from(n.min(255)).unwrap_or(255))
            }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DateSystem {
    Gregorian,
    Hijri,
}

/// One section of a format code.
#[derive(Debug, Clone)]
pub(crate) struct Part {
    pub(crate) scale: f64,
    pub(crate) percent: bool,
    pub(crate) text: bool,
    pub(crate) date: u32,
    pub(crate) date_eval: bool,
    pub(crate) date_system: DateSystem,
    pub(crate) sec_decimals: usize,
    pub(crate) general: bool,
    pub(crate) clock: u32,
    pub(crate) int_pattern: Vec<String>,
    pub(crate) frac_pattern: Vec<String>,
    pub(crate) man_pattern: Vec<String>,
    pub(crate) den_pattern: Vec<String>,
    pub(crate) num_pattern: Vec<String>,
    pub(crate) tokens: Vec<Out>,
    pub(crate) condition: Option<(CondOp, f64)>,
    pub(crate) color: Option<FormatColor>,
    pub(crate) locale: Option<String>,
    pub(crate) grouping: bool,
    pub(crate) fractions: bool,
    pub(crate) dec_fractions: bool,
    pub(crate) exponential: bool,
    pub(crate) exp_plus: bool,
    pub(crate) integer: bool,
    pub(crate) int_max: usize,
    pub(crate) int_min: usize,
    pub(crate) frac_max: usize,
    pub(crate) man_max: usize,
    pub(crate) num_min: usize,
    pub(crate) den_max: usize,
    pub(crate) den_min: usize,
    pub(crate) int_p: String,
    pub(crate) man_p: String,
    pub(crate) num_p: String,
    pub(crate) den_p: String,
    pub(crate) denominator: Option<f64>,
    pub(crate) generated: bool,
    pub(crate) error: bool,
    tokens_used: usize,
}

impl Part {
    fn new() -> Part {
        Part {
            scale: 1.0,
            percent: false,
            text: false,
            date: 0,
            date_eval: false,
            date_system: DateSystem::Gregorian,
            sec_decimals: 0,
            general: false,
            clock: 24,
            int_pattern: Vec::new(),
            frac_pattern: Vec::new(),
            man_pattern: Vec::new(),
            den_pattern: Vec::new(),
            num_pattern: Vec::new(),
            tokens: Vec::new(),
            condition: None,
            color: None,
            locale: None,
            grouping: false,
            fractions: false,
            dec_fractions: false,
            exponential: false,
            exp_plus: false,
            integer: false,
            int_max: 0,
            int_min: 0,
            frac_max: 0,
            man_max: 0,
            num_min: 0,
            den_max: 0,
            den_min: 0,
            int_p: String::new(),
            man_p: String::new(),
            num_p: String::new(),
            den_p: String::new(),
            denominator: None,
            generated: false,
            error: false,
            tokens_used: 0,
        }
    }

    /// The section shown for an unparseable code.
    pub(crate) fn error_part() -> Part {
        let mut p = Part::new();
        p.tokens.push(Out::Error);
        p.error = true;
        p
    }

    fn pattern_mut(&mut self, k: NumKind) -> &mut Vec<String> {
        match k {
            NumKind::Int => &mut self.int_pattern,
            NumKind::Frac => &mut self.frac_pattern,
            NumKind::Man => &mut self.man_pattern,
            NumKind::Num => &mut self.num_pattern,
            NumKind::Den => &mut self.den_pattern,
        }
    }
}

fn max_min(s: &str) -> (usize, usize) {
    (s.chars().count(), s.chars().filter(|&c| c != '#').count())
}

fn num_op_value(t: &Tok, current: NumKind) -> Option<char> {
    match t {
        Tok::Hash => Some('#'),
        Tok::Zero => Some('0'),
        Tok::Qmark => Some('?'),
        Tok::Digit(c) if current == NumKind::Den => Some(*c),
        _ => None,
    }
}

fn add_str(tokens: &mut Vec<Out>, s: &str) {
    tokens.push(Out::Str {
        value: s.to_owned(),
        rule: Rule::None,
    });
}

/// English locale codes (`[$-409]`) and tags (`[$-en-US]`) resolve to the English data.
pub(crate) fn resolve_locale(code: &str) -> Option<String> {
    const EN: [(u32, &str); 15] = [
        (0x0409, "en_US"),
        (0x0809, "en_GB"),
        (0x0C09, "en_AU"),
        (0x1009, "en_CA"),
        (0x1409, "en_NZ"),
        (0x1809, "en_IE"),
        (0x1C09, "en_ZA"),
        (0x2009, "en_JM"),
        (0x2409, "en_CB"),
        (0x2809, "en_BZ"),
        (0x2C09, "en_TT"),
        (0x3009, "en_ZW"),
        (0x3409, "en_PH"),
        (0x4009, "en_IN"),
        (0x04FF, "en"),
    ];
    if let Ok(n) = u32::from_str_radix(code, 16) {
        let lang = n & 0xFFFF;
        if let Some((_, tag)) = EN.iter().find(|(c, _)| *c == lang) {
            return Some((*tag).to_owned());
        }
        return (lang != 0).then(|| format!("{lang:x}"));
    }
    let valid = !code.is_empty()
        && code
            .split(['-', '_', '.', '@'])
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()));
    valid.then(|| code.replace('-', "_"))
}

/// Parses one section starting at `tokens[0]`.
fn parse_section(tokens: &[Token]) -> Result<Part, String> {
    let mut part = Part::new();
    let mut current = NumKind::Int;
    let mut last_chunk: Option<usize> = None;
    let mut date_chunks: Vec<usize> = Vec::new();
    let mut last: Option<&Tok> = None;
    let mut have_locale = false;
    let mut have_slash = false;
    let mut source = String::new();
    let mut index = 0usize;
    let mut out: Vec<Out> = Vec::new();
    while index < tokens.len() {
        let token = &tokens[index];
        source.push_str(&token.raw);
        let last_is_num_op = last.is_some_and(|l| num_op_value(l, current).is_some());
        if let Some(v) = num_op_value(&token.tok, current) {
            let last_is_group = matches!(last, Some(Tok::Group));
            if last_is_num_op || last_is_group {
                let pt = part.pattern_mut(current);
                let mut s = pt.pop().unwrap_or_default();
                s.push(v);
                pt.push(s);
                if let Some(Out::Number { num, .. }) = last_chunk.and_then(|i| out.get_mut(i)) {
                    num.push(v);
                }
            } else {
                part.pattern_mut(current).push(v.to_string());
                out.push(Out::Number {
                    kind: current,
                    num: v.to_string(),
                });
                last_chunk = Some(out.len() - 1);
            }
            last = Some(&token.tok);
            index += 1;
            continue;
        }
        match &token.tok {
            Tok::General => {
                part.general = true;
                out.push(Out::General);
            }
            Tok::Paren(c) => add_str(&mut out, &c.to_string()),
            Tok::Digit(c) => add_str(&mut out, &c.to_string()),
            Tok::Slash => {
                have_slash = true;
                if !part.pattern_mut(current).is_empty() {
                    let Some(chunk) = last_chunk else {
                        return Err("Format pattern is missing a numerator".into());
                    };
                    part.fractions = true;
                    let moved = part.pattern_mut(current).pop().unwrap_or_default();
                    part.num_pattern.push(moved);
                    if let Some(Out::Number { kind, .. }) = out.get_mut(chunk) {
                        *kind = NumKind::Num;
                    }
                    current = NumKind::Den;
                    out.push(Out::Div);
                } else {
                    add_str(&mut out, "/");
                }
            }
            Tok::Comma => add_str(&mut out, ","),
            Tok::Scale => part.scale = 0.001f64.powi(token.raw.len() as i32),
            Tok::Group => {
                if current == NumKind::Int {
                    part.grouping = true;
                }
                if current == NumKind::Den {
                    return Err("Cannot group denominator digits".into());
                }
            }
            Tok::Space => out.push(Out::Space { rule: Rule::None }),
            Tok::Break => break,
            Tok::Text => {
                part.text = true;
                out.push(Out::Text);
            }
            Tok::Plus => out.push(Out::Plus),
            Tok::Minus => out.push(Out::Minus { volatile: false }),
            Tok::Duration(v) => {
                let (kind, size) = match v.chars().next() {
                    Some('h') => (DateKind::HourElap, U_HOUR),
                    Some('m') => (DateKind::MinElap, U_MIN),
                    _ => (DateKind::SecElap, U_SEC),
                };
                part.date |= size;
                out.push(Out::Date(DateBit {
                    kind,
                    size,
                    pad: v.len(),
                    used: false,
                    indeterminate: false,
                }));
                date_chunks.push(out.len() - 1);
            }
            Tok::Point
                if part.date != 0
                    && matches!(tokens.get(index + 1).map(|t| &t.tok), Some(Tok::Zero)) =>
            {
                let mut dec = 1;
                index += 1;
                source.push('0');
                for _ in 0..2 {
                    if matches!(tokens.get(index + 1).map(|t| &t.tok), Some(Tok::Zero)) {
                        dec += 1;
                        index += 1;
                        source.push('0');
                    }
                }
                let size = [U_SEC, U_DSEC, U_CSEC, U_MSEC][dec];
                part.date |= size;
                part.date_eval = true;
                part.sec_decimals = part.sec_decimals.max(dec);
                out.push(Out::Subsec { decimals: dec });
            }
            Tok::Calendar(v) => {
                if !have_locale {
                    part.date_system = if v.ends_with('2') {
                        DateSystem::Hijri
                    } else {
                        DateSystem::Gregorian
                    };
                }
            }
            Tok::DateTime(raw) => {
                let value = raw.to_ascii_lowercase();
                let first = value.chars().next().unwrap_or(' ');
                let pad2 = |s: &str, p: &str| usize::from(s.contains(p));
                let mut bit = DateBit {
                    kind: DateKind::Era,
                    size: 0,
                    pad: 0,
                    used: false,
                    indeterminate: false,
                };
                let mut typed = true;
                if value == "y" || value == "yy" {
                    (bit.kind, bit.size) = (DateKind::YearShort, U_YEAR);
                } else if first == 'y' || first == 'e' {
                    (bit.kind, bit.size) = (DateKind::Year, U_YEAR);
                } else if value == "b" || value == "bb" {
                    (bit.kind, bit.size) = (DateKind::BYearShort, U_YEAR);
                } else if first == 'b' {
                    (bit.kind, bit.size) = (DateKind::BYear, U_YEAR);
                } else if value == "d" || value == "dd" {
                    (bit.kind, bit.size) = (DateKind::Day, U_DAY);
                    bit.pad = pad2(&value, "dd");
                } else if value == "ddd" || value == "aaa" {
                    (bit.kind, bit.size) = (DateKind::WeekdayShort, U_DAY);
                } else if first == 'd' || first == 'a' {
                    (bit.kind, bit.size) = (DateKind::Weekday, U_DAY);
                } else if first == 'h' {
                    (bit.kind, bit.size) = (DateKind::Hour, U_HOUR);
                    bit.pad = pad2(&value, "hh");
                } else if first == 'm' {
                    typed = false;
                    let n = value.chars().count();
                    if n == 3 {
                        (bit.kind, bit.size, typed) = (DateKind::MonthNameShort, U_MONTH, true);
                    } else if n == 5 {
                        (bit.kind, bit.size, typed) = (DateKind::MonthNameSingle, U_MONTH, true);
                    } else if n >= 4 {
                        (bit.kind, bit.size, typed) = (DateKind::MonthName, U_MONTH, true);
                    }
                    // m or mm is a minute after an hour or a second, otherwise a month (which a
                    // following second may still turn into a minute).
                    if !typed
                        && let Some(&li) = date_chunks.last()
                        && let Some(Out::Date(prev)) = out.get_mut(li)
                        && !prev.used
                        && prev.size & (U_HOUR | U_SEC) != 0
                    {
                        prev.used = true;
                        (bit.kind, bit.size, typed) = (DateKind::Min, U_MIN, true);
                        bit.pad = pad2(&value, "mm");
                    }
                    if !typed {
                        (bit.kind, bit.size) = (DateKind::Month, U_MONTH);
                        bit.pad = pad2(&value, "mm");
                        bit.indeterminate = true;
                    }
                } else if first == 's' {
                    (bit.kind, bit.size) = (DateKind::Sec, U_SEC);
                    bit.pad = pad2(&value, "ss");
                    if let Some(&li) = date_chunks.last()
                        && let Some(Out::Date(prev)) = out.get_mut(li)
                    {
                        if prev.size & U_MIN != 0 {
                            bit.used = true;
                        } else if prev.indeterminate {
                            prev.indeterminate = false;
                            prev.size = U_MIN;
                            prev.kind = DateKind::Min;
                            bit.used = true;
                        }
                    }
                }
                let _ = typed;
                part.date |= bit.size;
                part.date_eval = true;
                out.push(Out::Date(bit));
                date_chunks.push(out.len() - 1);
            }
            Tok::AmPm(v) => {
                part.clock = 12;
                part.date |= U_HOUR;
                part.date_eval = true;
                out.push(Out::AmPm { short: v == "A/P" });
            }
            Tok::Str(v) | Tok::Escaped(v) | Tok::Char(v) => add_str(&mut out, v),
            Tok::Condition(op, v) => part.condition = Some((*op, *v)),
            Tok::Locale(v) => {
                let (currency, code) = match v.split_once('-') {
                    Some((c, rest)) => (c, rest),
                    None => (v.as_str(), ""),
                };
                if !currency.is_empty() {
                    add_str(&mut out, currency);
                }
                if let Some(l) = resolve_locale(code) {
                    part.locale = Some(l);
                }
                if let Ok(wincode) = u32::from_str_radix(code, 16)
                    && wincode & 0xFF0000 != 0
                    && (wincode >> 16) & 0xFF == 6
                {
                    part.date_system = DateSystem::Hijri;
                }
                have_locale = true;
            }
            Tok::Color(v) => part.color = FormatColor::parse(v),
            Tok::Percent => {
                part.scale = 100.0;
                part.percent = true;
                add_str(&mut out, "%");
            }
            Tok::Point => {
                out.push(Out::Point);
                if part.date == 0 {
                    part.dec_fractions = true;
                    current = NumKind::Frac;
                }
            }
            Tok::Exp(sign) => {
                part.exponential = true;
                part.exp_plus = *sign == '+';
                current = NumKind::Man;
                out.push(Out::Exp);
            }
            Tok::Skip(_) => out.push(Out::Skip),
            Tok::Fill(_) => out.push(Out::Fill),
            Tok::DbNum | Tok::Modifier => {}
            Tok::Error => return Err(format!("Illegal character: {source}")),
            Tok::Hash | Tok::Zero | Tok::Qmark => {
                return Err(format!("Unexpected digit placeholder in {source}"));
            }
        }
        last = Some(&token.tok);
        index += 1;
    }
    part.tokens_used = index;

    // A section of only bracketed codes shows the value as text (`[Red];…`), except elapsed time.
    let trimmed = source.strip_suffix(';').unwrap_or(&source);
    let only_brackets = !trimmed.is_empty() && {
        let mut rest = trimmed;
        let mut ok = true;
        while !rest.is_empty() {
            if !rest.starts_with('[') {
                ok = false;
                break;
            }
            match rest.find(']') {
                Some(e) if e > 1 => rest = &rest[e + 1..],
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        ok
    };
    // `^\[(?:h+|m+|s+)\]` (lower case only, as numfmt does).
    let elapsed_first = source.strip_prefix('[').is_some_and(|rest| {
        rest.find(']').is_some_and(|e| {
            let body = &rest[..e];
            let f = body.chars().next().unwrap_or(' ');
            matches!(f, 'h' | 'm' | 's') && body.chars().all(|c| c == f)
        })
    });
    if only_brackets && !elapsed_first {
        out.push(Out::Text);
    }

    let int_pat: String = part.int_pattern.concat();
    let man_pat: String = part.man_pattern.concat();
    let frac_pat: String = part.frac_pattern.concat();
    if (part.fractions && part.dec_fractions)
        || (part.grouping && part.int_pattern.is_empty())
        || (part.fractions && part.exponential)
        || (part.fractions && (part.den_pattern.is_empty() || part.num_pattern.is_empty()))
        || (have_slash && !part.fractions && part.date == 0)
        || (part.exponential
            && ((part.int_pattern.is_empty() && part.frac_pattern.is_empty())
                || part.man_pattern.is_empty()))
    {
        return Err(format!("Invalid pattern: {source}"));
    }
    (part.int_max, _) = max_min(&int_pat);
    let ic: Vec<char> = int_pat.chars().collect();
    let mut min = 0;
    for i in 0..ic.len() {
        let ch = ic[ic.len() - 1 - i];
        if ch.is_ascii_digit() || ch == '?' {
            min = i + 1;
        }
    }
    part.int_min = min;
    (part.frac_max, _) = max_min(&frac_pat);
    (part.man_max, _) = max_min(&man_pat);

    let mut num_pat: String = part.num_pattern.concat();
    let mut den_pat = part.den_pattern.first().cloned().unwrap_or_default();
    let enforce_padded = den_pat.contains('?') || num_pat.contains('?');
    if enforce_padded {
        den_pat = den_pat
            .chars()
            .map(|c| if c.is_ascii_digit() { '?' } else { c })
            .collect();
        if den_pat.ends_with('#') {
            den_pat.pop();
            den_pat.push('?');
        }
        (_, part.num_min) = max_min(&num_pat);
        (part.den_max, part.den_min) = max_min(&den_pat);
        if num_pat.ends_with('#') {
            num_pat.pop();
            num_pat.push('?');
        }
    } else {
        (_, part.num_min) = max_min(&num_pat);
        (part.den_max, part.den_min) = max_min(&den_pat);
    }
    part.int_p = int_pat.clone();
    part.man_p = man_pat;
    part.num_p = num_pat;
    part.den_p = den_pat;
    if !part.den_pattern.is_empty() {
        let digits: String = part
            .den_pattern
            .concat()
            .chars()
            .filter(char::is_ascii_digit)
            .collect();
        part.denominator = digits.parse::<f64>().ok();
    }
    part.integer = !int_pat.is_empty();
    if !part.integer && !part.exponential && !frac_pat.is_empty() {
        if let Some(pi) = out.iter().position(|o| *o == Out::Point) {
            out.insert(
                pi,
                Out::Number {
                    kind: NumKind::Int,
                    num: "#".into(),
                },
            );
        }
        part.integer = true;
        part.int_pattern = vec!["#".into()];
        part.int_p = "#".into();
    }
    if part.fractions {
        for i in 0..out.len().saturating_sub(1) {
            let next = match &out[i + 1] {
                Out::Number {
                    kind: NumKind::Num, ..
                } => Rule::NumInt,
                Out::Div => Rule::Num,
                Out::Number {
                    kind: NumKind::Den, ..
                } => Rule::Den,
                _ => Rule::None,
            };
            match &mut out[i] {
                Out::Str { rule, .. } | Out::Space { rule } if next != Rule::None => {
                    *rule = next;
                }
                _ => {}
            }
        }
    }
    if part.grouping && part.int_pattern.len() > 1 {
        part.grouping = false;
    }
    part.tokens = out;
    Ok(part)
}

/// A parsed format code.
#[derive(Debug, Clone)]
pub(crate) struct Pattern {
    pub(crate) partitions: Vec<Part>,
    pub(crate) locale: Option<String>,
}

fn maybe_add_minus(part: &mut Part) {
    let exception = match part.condition {
        Some((op, v)) => {
            (v < 0.0 && matches!(op, CondOp::Lt | CondOp::Le | CondOp::Eq))
                || (v == 0.0 && op == CondOp::Lt)
        }
        None => false,
    };
    if !exception {
        part.tokens.insert(0, Out::Minus { volatile: true });
    }
}

fn parse_alone(code: &str) -> Part {
    parse_section(&tokenize(code)).unwrap_or_else(|_| Part::error_part())
}

pub(crate) fn parse_pattern(pattern: &str) -> Result<Pattern, String> {
    let mut partitions: Vec<Part> = Vec::new();
    let mut conditional = false;
    let mut locale = None;
    let mut text_partition: Option<usize> = None;
    let all = tokenize(pattern);
    let mut tokens: &[Token] = &all;
    let mut conditions = 0;
    let mut more;
    let mut i = 0;
    loop {
        let part = parse_section(tokens)?;
        if (part.date != 0 || part.general)
            && (!part.int_pattern.is_empty()
                || !part.frac_pattern.is_empty()
                || part.scale != 1.0
                || part.text)
        {
            return Err("Illegal format".into());
        }
        if part.condition.is_some() {
            conditions += 1;
            conditional = true;
        }
        if part.text {
            if text_partition.is_some() {
                return Err("Unexpected partition".into());
            }
            text_partition = Some(partitions.len());
        }
        if let Some(l) = &part.locale {
            locale = Some(l.clone());
        }
        let used = part.tokens_used;
        partitions.push(part);
        more = matches!(tokens.get(used).map(|t| &t.tok), Some(Tok::Break));
        tokens = &tokens[(used + usize::from(more)).min(tokens.len())..];
        i += 1;
        if !(more && i < 4 && conditions < 3) {
            break;
        }
    }
    if more {
        return Err("Unexpected partition".into());
    }
    if conditions > 2 {
        return Err("Unexpected condition".into());
    }
    if let Some(p3) = partitions.get(3)
        && (!p3.int_pattern.is_empty() || !p3.frac_pattern.is_empty() || p3.date != 0)
    {
        return Err("Unexpected partition".into());
    }
    if conditional {
        if partitions[0].condition.is_none() {
            partitions[0].condition = Some((CondOp::Gt, 0.0));
        }
        let n = partitions.len();
        if n == 1 {
            let mut p = parse_alone("General");
            p.generated = true;
            partitions.push(p);
        }
        if n < 3 {
            maybe_add_minus(&mut partitions[0]);
            if partitions[1].condition.is_some() {
                maybe_add_minus(&mut partitions[1]);
            } else if let Some((op, v)) = partitions[0].condition
                && (op == CondOp::Eq || (v >= 0.0 && matches!(op, CondOp::Gt | CondOp::Ge)))
            {
                partitions[1]
                    .tokens
                    .insert(0, Out::Minus { volatile: true });
            }
        } else {
            for p in &mut partitions {
                maybe_add_minus(p);
            }
        }
    } else {
        let mut text = None;
        if partitions.len() < 4
            && let Some(ti) = text_partition
        {
            text = Some(partitions.remove(ti));
        }
        if partitions.is_empty() && text.is_some() {
            let mut p = parse_alone("General");
            p.generated = true;
            partitions.push(p);
        }
        if partitions.len() < 2 {
            let mut p = partitions[0].clone();
            p.tokens.insert(0, Out::Minus { volatile: true });
            p.generated = true;
            partitions.push(p);
        }
        if partitions.len() < 3 {
            let mut p = partitions[0].clone();
            p.generated = true;
            partitions.push(p);
        }
        if partitions.len() < 4 {
            match text {
                Some(t) => partitions.push(t),
                None => {
                    let mut p = parse_alone("@");
                    p.generated = true;
                    partitions.push(p);
                }
            }
        }
        partitions[0].condition = Some((CondOp::Gt, 0.0));
        partitions[1].condition = Some((CondOp::Lt, 0.0));
        partitions[2].condition = None;
    }
    Ok(Pattern { partitions, locale })
}
