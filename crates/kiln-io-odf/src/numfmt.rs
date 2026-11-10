// SPDX-License-Identifier: AGPL-3.0-or-later
//! Number formats: ODF data styles (`number:number-style`, `number:date-style`, …) ⇄ Excel
//! format codes, which is what the Grid model stores and what Write's date fields use (as Word
//! date pictures).
//!
//! | Excel | ODF |
//! |---|---|
//! | `0`, `#`, `?`, `,`, `.` | `number:number` (`min-integer-digits`, `decimal-places`, `min-decimal-places`, `grouping`), trailing commas as `display-factor` |
//! | `0.00E+00` | `number:scientific-number` |
//! | `# ?/?`, `# ?/8` | `number:fraction` (`min-denominator-digits` or `denominator-value`) |
//! | `%` | a `number:percentage-style` |
//! | `[$€-407]`, `$` in a currency section | `number:currency-symbol` in a `number:currency-style` |
//! | `d dd ddd dddd m mm mmm mmmm yy yyyy` | `number:day`, `number:day-of-week`, `number:month` (`textual`), `number:year` (`style="long"`) |
//! | `h hh m mm s ss.00 AM/PM [h]` | `number:hours`, `number:minutes`, `number:seconds` (`decimal-places`), `number:am-pm`; elapsed time as `truncate-on-overflow="false"` |
//! | `@` | `number:text-content` in a `number:text-style` |
//! | `"text"`, `\c`, other literals | `number:text` |
//! | `[Red]` | `style:text-properties fo:color` |
//! | sections `a;b;c;d` and `[>=100]` | one style per section, joined by `style:map` conditions |
//!
//! | `_x` (padding as wide as `x`) | `number:text` with `loext:blank-width-char` |
//! | `*x` (repeat to fill) | `number:fill-character` |
//! | `###-####` (literals between digits) | `number:embedded-text` at a position |
//! | `??` integer digits | `loext:max-blank-integer-digits` |
//! | `##0.0E+0` (engineering) | `number:exponent-interval` |

use std::collections::HashMap;

use kiln_ooxml::xml::XmlElement;

use crate::el::El;
use crate::xml::{all_text, kid, kids};

/// One element of a format section.
#[derive(Debug, Clone, PartialEq)]
enum Item {
    Lit(String),
    /// `_x`: a space as wide as `x`.
    Pad(char),
    /// `*x`: `x` repeated to fill the cell.
    Fill(char),
    Number(NumSpec),
    Sci {
        num: NumSpec,
        exp_digits: u32,
    },
    Frac {
        int_digits: Option<u32>,
        num_digits: u32,
        den_digits: u32,
        den_value: Option<u32>,
    },
    Text,
    Currency(String),
    Day(bool),
    DayOfWeek(bool),
    Month {
        long: bool,
        textual: bool,
    },
    Year(bool),
    Hours {
        long: bool,
        elapsed: bool,
    },
    Minutes {
        long: bool,
        elapsed: bool,
    },
    Seconds {
        long: bool,
        decimals: u32,
        elapsed: bool,
    },
    AmPm,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct NumSpec {
    min_int: u32,
    decimals: u32,
    min_decimals: u32,
    grouping: bool,
    /// Trailing commas: the value is divided by 1000 per comma.
    scale: u32,
    /// No placeholders at all (`General`).
    general: bool,
    /// Integer placeholders of any kind (`#`, `0`, `?`).
    int_places: u32,
    /// Trailing `?` integer placeholders (blank when not needed).
    blank_int: u32,
    /// Literals between integer digits, with the number of digits to their right.
    embedded: Vec<(u32, String)>,
}

#[derive(Debug, Clone, Default, PartialEq)]
struct Section {
    items: Vec<Item>,
    color: Option<&'static str>,
    cond: Option<String>,
    percent: bool,
}

const COLORS: &[(&str, &str)] = &[
    ("Black", "#000000"),
    ("Blue", "#0000ff"),
    ("Cyan", "#00ffff"),
    ("Green", "#00ff00"),
    ("Magenta", "#ff00ff"),
    ("Red", "#ff0000"),
    ("White", "#ffffff"),
    ("Yellow", "#ffff00"),
];

fn split_sections(code: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let (mut quote, mut bracket, mut escape) = (false, false, false);
    for c in code.chars() {
        let cur = out.last_mut().expect("never empty");
        if escape {
            cur.push(c);
            escape = false;
            continue;
        }
        match c {
            '\\' if !quote => {
                cur.push(c);
                escape = true;
            }
            '"' => {
                quote = !quote;
                cur.push(c);
            }
            '[' if !quote => {
                bracket = true;
                cur.push(c);
            }
            ']' if !quote => {
                bracket = false;
                cur.push(c);
            }
            ';' if !quote && !bracket => out.push(String::new()),
            _ => cur.push(c),
        }
    }
    out
}

fn push_lit(items: &mut Vec<Item>, s: &str) {
    if s.is_empty() {
        return;
    }
    if let Some(Item::Lit(l)) = items.last_mut() {
        l.push_str(s);
    } else {
        items.push(Item::Lit(s.to_owned()));
    }
}

fn num_spec(block: &str) -> NumSpec {
    let mut spec = NumSpec::default();
    let trimmed = block.trim_end_matches(',');
    #[allow(clippy::cast_possible_truncation)]
    {
        spec.scale = (block.len() - trimmed.len()) as u32;
    }
    let (int, frac) = match trimmed.find('.') {
        Some(i) => (&trimmed[..i], &trimmed[i + 1..]),
        None => (trimmed, ""),
    };
    spec.grouping = int.contains(',');
    #[allow(clippy::cast_possible_truncation)]
    {
        spec.min_int = int.chars().filter(|c| matches!(c, '0' | '?')).count() as u32;
        spec.int_places = int.chars().filter(|c| is_placeholder(*c)).count() as u32;
        spec.blank_int = int
            .chars()
            .rev()
            .filter(|c| is_placeholder(*c))
            .take_while(|c| *c == '?')
            .count() as u32;
        spec.decimals = frac
            .chars()
            .filter(|c| matches!(c, '0' | '#' | '?'))
            .count() as u32;
        spec.min_decimals = frac.chars().filter(|c| *c == '0').count() as u32;
    }
    spec
}

fn is_placeholder(c: char) -> bool {
    matches!(c, '0' | '#' | '?')
}

fn parse_section(src: &str) -> Section {
    let mut sec = Section::default();
    let chars: Vec<char> = src.chars().collect();
    let mut i = 0;
    let mut items: Vec<Item> = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            '"' => {
                let mut j = i + 1;
                let mut s = String::new();
                while j < chars.len() && chars[j] != '"' {
                    s.push(chars[j]);
                    j += 1;
                }
                push_lit(&mut items, &s);
                i = j + 1;
            }
            '\\' => {
                if let Some(n) = chars.get(i + 1) {
                    push_lit(&mut items, &n.to_string());
                }
                i += 2;
            }
            '_' => {
                items.push(Item::Pad(chars.get(i + 1).copied().unwrap_or(' ')));
                i += 2;
            }
            '*' => {
                items.push(Item::Fill(chars.get(i + 1).copied().unwrap_or(' ')));
                i += 2;
            }
            '[' => {
                let end = chars[i..]
                    .iter()
                    .position(|c| *c == ']')
                    .map_or(chars.len(), |p| i + p);
                let inner: String = chars[i + 1..end.min(chars.len())].iter().collect();
                i = end + 1;
                let lower = inner.to_ascii_lowercase();
                if let Some(sym) = inner.strip_prefix('$') {
                    let sym = sym.split('-').next().unwrap_or_default();
                    if !sym.is_empty() {
                        items.push(Item::Currency(sym.to_owned()));
                    }
                } else if inner.starts_with(['<', '>', '=']) {
                    sec.cond = Some(inner.replace(' ', ""));
                } else if let Some((_, hex)) =
                    COLORS.iter().find(|(n, _)| n.eq_ignore_ascii_case(&inner))
                {
                    sec.color = Some(hex);
                } else if !lower.is_empty() && lower.chars().all(|c| c == 'h') {
                    items.push(Item::Hours {
                        long: lower.len() > 1,
                        elapsed: true,
                    });
                } else if !lower.is_empty() && lower.chars().all(|c| c == 'm') {
                    items.push(Item::Minutes {
                        long: lower.len() > 1,
                        elapsed: true,
                    });
                } else if !lower.is_empty() && lower.chars().all(|c| c == 's') {
                    items.push(Item::Seconds {
                        long: lower.len() > 1,
                        decimals: 0,
                        elapsed: true,
                    });
                }
                // Other brackets ([Color10], [DBNum1], locale codes) carry nothing ODF keeps.
            }
            '@' => {
                items.push(Item::Text);
                i += 1;
            }
            // A bare `$` is the currency symbol (a quoted one stays text).
            '$' => {
                items.push(Item::Currency("$".to_owned()));
                i += 1;
            }
            '%' => {
                sec.percent = true;
                push_lit(&mut items, "%");
                i += 1;
            }
            c if is_placeholder(c)
                || (c == '.' && chars.get(i + 1).is_some_and(|n| is_placeholder(*n))) =>
            {
                let start = i;
                while i < chars.len()
                    && (is_placeholder(chars[i]) || chars[i] == ',' || chars[i] == '.')
                {
                    i += 1;
                }
                let block: String = chars[start..i].iter().collect();
                // Scientific: E+ / E- followed by digits.
                if i < chars.len()
                    && (chars[i] == 'E' || chars[i] == 'e')
                    && chars.get(i + 1).is_some_and(|s| *s == '+' || *s == '-')
                {
                    let mut j = i + 2;
                    while j < chars.len() && is_placeholder(chars[j]) {
                        j += 1;
                    }
                    #[allow(clippy::cast_possible_truncation)]
                    let exp_digits = (j - i - 2) as u32;
                    items.push(Item::Sci {
                        num: num_spec(&block),
                        exp_digits: exp_digits.max(1),
                    });
                    i = j;
                    continue;
                }
                // Fraction: digits, optional space, digits '/' digits.
                let rest: String = chars[i..].iter().collect();
                let frac_after = |s: &str| -> Option<(u32, Option<u32>, usize)> {
                    let s2 = s.strip_prefix('/')?;
                    let den: String = s2
                        .chars()
                        .take_while(|c| is_placeholder(*c) || c.is_ascii_digit())
                        .collect();
                    if den.is_empty() {
                        return None;
                    }
                    let value = if den.chars().all(|c| c.is_ascii_digit()) && !den.starts_with('0')
                    {
                        den.parse().ok()
                    } else {
                        None
                    };
                    #[allow(clippy::cast_possible_truncation)]
                    Some((den.chars().count() as u32, value, 1 + den.chars().count()))
                };
                if let Some((den_digits, den_value, used)) = frac_after(&rest) {
                    #[allow(clippy::cast_possible_truncation)]
                    items.push(Item::Frac {
                        int_digits: None,
                        num_digits: block.chars().filter(|c| is_placeholder(*c)).count() as u32,
                        den_digits,
                        den_value,
                    });
                    i += used;
                    continue;
                }
                let trimmed_rest = rest.trim_start_matches(' ');
                let spaces = rest.len() - trimmed_rest.len();
                if spaces > 0 {
                    let numer: String = trimmed_rest
                        .chars()
                        .take_while(|c| is_placeholder(*c))
                        .collect();
                    if !numer.is_empty()
                        && let Some((den_digits, den_value, used)) =
                            frac_after(&trimmed_rest[numer.len()..])
                    {
                        #[allow(clippy::cast_possible_truncation)]
                        items.push(Item::Frac {
                            int_digits: Some(block.chars().filter(|c| *c == '0').count() as u32),
                            num_digits: numer.chars().count() as u32,
                            den_digits,
                            den_value,
                        });
                        i += spaces + numer.len() + used;
                        continue;
                    }
                }
                items.push(Item::Number(num_spec(&block)));
            }
            _ => {
                let lower = c.to_ascii_lowercase();
                let run = chars[i..]
                    .iter()
                    .take_while(|x| x.to_ascii_lowercase() == lower)
                    .count();
                let rest: String = chars[i..].iter().collect::<String>().to_ascii_uppercase();
                if rest.starts_with("AM/PM") {
                    items.push(Item::AmPm);
                    i += 5;
                    continue;
                }
                if rest.starts_with("A/P") {
                    items.push(Item::AmPm);
                    i += 3;
                    continue;
                }
                if rest.starts_with("GENERAL") {
                    items.push(Item::Number(NumSpec {
                        min_int: 1,
                        general: true,
                        ..NumSpec::default()
                    }));
                    i += 7;
                    continue;
                }
                match lower {
                    'y' => items.push(Item::Year(run > 2)),
                    'd' => items.push(match run {
                        1 => Item::Day(false),
                        2 => Item::Day(true),
                        3 => Item::DayOfWeek(false),
                        _ => Item::DayOfWeek(true),
                    }),
                    'h' => items.push(Item::Hours {
                        long: run > 1,
                        elapsed: false,
                    }),
                    's' => {
                        let mut decimals = 0;
                        let mut j = i + run;
                        if chars.get(j) == Some(&'.') {
                            let z = chars[j + 1..].iter().take_while(|c| **c == '0').count();
                            if z > 0 {
                                #[allow(clippy::cast_possible_truncation)]
                                {
                                    decimals = z as u32;
                                }
                                j += 1 + z;
                            }
                        }
                        items.push(Item::Seconds {
                            long: run > 1,
                            decimals,
                            elapsed: false,
                        });
                        i = j;
                        continue;
                    }
                    'm' => {
                        // Minutes after hours or before seconds, otherwise months.
                        let prev_time = items.iter().rev().find(|it| !matches!(it, Item::Lit(_)));
                        let after_h = matches!(prev_time, Some(Item::Hours { .. }));
                        let next: String = chars[i + run..]
                            .iter()
                            .filter(|c| c.is_ascii_alphabetic() || **c == '[')
                            .take(2)
                            .collect::<String>()
                            .to_ascii_lowercase();
                        let before_s = next.starts_with('s') || next.starts_with("[s");
                        if (after_h || before_s) && run <= 2 {
                            items.push(Item::Minutes {
                                long: run > 1,
                                elapsed: false,
                            });
                        } else {
                            items.push(Item::Month {
                                long: run == 2 || run >= 4,
                                textual: run >= 3,
                            });
                        }
                    }
                    'e' if run >= 1 => items.push(Item::Year(true)),
                    'b' if run >= 1 => items.push(Item::Year(run > 2)),
                    _ => {
                        push_lit(&mut items, &chars[i..i + run].iter().collect::<String>());
                    }
                }
                i += run;
            }
        }
    }
    sec.items = embed_literals(items);
    sec
}

/// Short literals between whole-number blocks (`###-####`, `(###) ###-####`) become embedded
/// text of one number, which is how ODF (and LibreOffice) keep them.
fn embed_literals(mut items: Vec<Item>) -> Vec<Item> {
    let joinable =
        |s: &str| !s.is_empty() && s.len() <= 3 && s.chars().all(|c| " -()/".contains(c));
    let mut k = items.len();
    while k >= 3 {
        k -= 1;
        let merge = matches!(
            (&items[k - 2], &items[k - 1], &items[k]),
            (Item::Number(a), Item::Lit(l), Item::Number(_))
                if !a.general && a.decimals == 0 && a.scale == 0 && !a.grouping && a.int_places > 0 && joinable(l)
        );
        if !merge {
            continue;
        }
        let (Item::Number(b), Item::Lit(l), Item::Number(a)) =
            (items.remove(k), items.remove(k - 1), items.remove(k - 2))
        else {
            unreachable!()
        };
        let mut n = b.clone();
        n.embedded.push((b.int_places, l));
        n.embedded.extend(
            a.embedded
                .iter()
                .map(|(p, t)| (p + b.int_places, t.clone())),
        );
        n.int_places = a.int_places + b.int_places;
        n.min_int = if b.min_int == b.int_places {
            a.min_int + b.min_int
        } else {
            b.min_int
        };
        if b.blank_int == b.int_places {
            n.blank_int = b.blank_int + a.blank_int;
        }
        items.insert(k - 2, Item::Number(n));
        k -= 1;
    }
    items
}

fn is_date_item(i: &Item) -> bool {
    matches!(
        i,
        Item::Day(_) | Item::DayOfWeek(_) | Item::Month { .. } | Item::Year(_)
    )
}

fn is_time_item(i: &Item) -> bool {
    matches!(
        i,
        Item::Hours { .. } | Item::Minutes { .. } | Item::Seconds { .. } | Item::AmPm
    )
}

fn number_el(name: &str, n: &NumSpec) -> El {
    let mut e = El::new(name);
    if !n.general {
        e.set("number:decimal-places", n.decimals.to_string());
        e.set("number:min-decimal-places", n.min_decimals.to_string());
    }
    e.set("number:min-integer-digits", n.min_int.to_string());
    if n.blank_int > 0 {
        e.set("loext:max-blank-integer-digits", n.blank_int.to_string());
    }
    if n.grouping {
        e.set("number:grouping", "true");
    }
    if n.scale > 0 {
        e.set(
            "number:display-factor",
            1000u64.saturating_pow(n.scale).to_string(),
        );
    }
    let mut embedded = n.embedded.clone();
    embedded.sort_by_key(|(p, _)| std::cmp::Reverse(*p));
    for (p, t) in embedded {
        e.children.push(
            El::new("number:embedded-text")
                .with("number:position", p.to_string())
                .with_text(t.as_str()),
        );
    }
    e
}

fn text_el(s: &str) -> El {
    El::new("number:text").with_text(s)
}

fn style_el(sec: &Section) -> El {
    let items = &sec.items;
    let date = items.iter().any(is_date_item);
    let time = items.iter().any(is_time_item);
    let currency = items.iter().any(|i| matches!(i, Item::Currency(_)));
    let text = items.iter().any(|i| matches!(i, Item::Text));
    let name = if date {
        "number:date-style"
    } else if time {
        "number:time-style"
    } else if text {
        "number:text-style"
    } else if currency {
        "number:currency-style"
    } else if sec.percent {
        "number:percentage-style"
    } else {
        "number:number-style"
    };
    let mut el = El::new(name);
    if items.iter().any(|i| {
        matches!(
            i,
            Item::Hours { elapsed: true, .. }
                | Item::Minutes { elapsed: true, .. }
                | Item::Seconds { elapsed: true, .. }
        )
    }) {
        el.set("number:truncate-on-overflow", "false");
    }
    if let Some(c) = sec.color {
        el.children
            .push(El::new("style:text-properties").with("fo:color", c));
    }
    let long = |b: bool| if b { "long" } else { "short" };
    for it in items {
        let child = match it {
            Item::Lit(s) => text_el(s),
            Item::Pad(c) => text_el(" ").with("loext:blank-width-char", c.to_string()),
            Item::Fill(c) => El::new("number:fill-character").with_text(c.to_string().as_str()),
            Item::Number(n) => number_el("number:number", n),
            Item::Sci { num, exp_digits } => {
                let mut e = number_el("number:scientific-number", num);
                e.set("number:min-exponent-digits", exp_digits.to_string());
                if num.int_places > num.min_int.max(1) {
                    e.set("number:exponent-interval", num.int_places.to_string());
                }
                e
            }
            Item::Frac {
                int_digits,
                num_digits,
                den_digits,
                den_value,
            } => {
                // As LibreOffice writes them: no integer digits for `?/?`, zero for `# ?/?`.
                let mut e = El::new("number:fraction");
                if let Some(i) = int_digits {
                    e.set("number:min-integer-digits", i.to_string());
                }
                e.set("number:min-numerator-digits", num_digits.to_string());
                e.set("loext:max-numerator-digits", num_digits.to_string());
                match den_value {
                    Some(v) => e.set("number:denominator-value", v.to_string()),
                    None => {
                        e.set("number:min-denominator-digits", den_digits.to_string());
                        e.set(
                            "number:max-denominator-value",
                            10u64
                                .saturating_pow(*den_digits)
                                .saturating_sub(1)
                                .to_string(),
                        );
                    }
                }
                e
            }
            Item::Text => El::new("number:text-content"),
            Item::Currency(s) => El::new("number:currency-symbol").with_text(s.as_str()),
            Item::Day(l) => El::new("number:day").with("number:style", long(*l)),
            Item::DayOfWeek(l) => El::new("number:day-of-week").with("number:style", long(*l)),
            Item::Month { long: l, textual } => {
                let mut e = El::new("number:month").with("number:style", long(*l));
                if *textual {
                    e.set("number:textual", "true");
                }
                e
            }
            Item::Year(l) => El::new("number:year").with("number:style", long(*l)),
            Item::Hours { long: l, .. } => El::new("number:hours").with("number:style", long(*l)),
            Item::Minutes { long: l, .. } => {
                El::new("number:minutes").with("number:style", long(*l))
            }
            Item::Seconds {
                long: l, decimals, ..
            } => {
                let mut e = El::new("number:seconds").with("number:style", long(*l));
                if *decimals > 0 {
                    e.set("number:decimal-places", decimals.to_string());
                }
                e
            }
            Item::AmPm => El::new("number:am-pm"),
        };
        el.children.push(child);
    }
    if el
        .children
        .iter()
        .all(|c| c.name == "style:text-properties")
        && name == "number:number-style"
    {
        // An empty section (`0;;` hides zeros): an empty text keeps it a valid style.
        el.children.push(text_el(""));
    }
    el
}

fn odf_condition(c: &str) -> String {
    format!("value(){c}")
}

/// The data styles of an Excel format code: the sections' styles named `{name}P0`, `{name}P1`,
/// … followed by the main style `{name}` that maps to them. Empty for `General` (and an empty
/// code), which needs no data style.
#[must_use]
pub fn data_style_els(name: &str, code: &str) -> Vec<El> {
    if code.trim().is_empty() || code.trim().eq_ignore_ascii_case("general") {
        return Vec::new();
    }
    let mut sections: Vec<Section> = split_sections(code)
        .iter()
        .map(|s| parse_section(s))
        .collect();
    // A text section must be the main (last) style; trailing empty sections stay as written.
    let n = sections.len();
    let text_last = n == 4
        || (n > 1
            && sections[n - 1]
                .items
                .iter()
                .any(|i| matches!(i, Item::Text)));
    let defaults: &[&str] = match (n, text_last) {
        (1, _) => &[],
        (2, true) => &[""],
        (2, false) => &[">=0"],
        (3, true) => &[">=0", "<0"],
        (3, false) => &[">0", "<0"],
        _ => &[">0", "<0", "=0"],
    };
    let main = sections.pop().unwrap_or_default();
    let mut out = Vec::new();
    let mut maps = Vec::new();
    for (i, sec) in sections.iter().enumerate().take(3) {
        let cond = sec
            .cond
            .clone()
            .or_else(|| defaults.get(i).map(|s| (*s).to_owned()))
            .unwrap_or_default();
        let sub = format!("{name}P{i}");
        let mut el = style_el(sec);
        el.attrs.insert(0, ("style:name".to_owned(), sub.clone()));
        el.set("style:volatile", "true");
        out.push(el);
        let condition = if cond.is_empty() {
            // `0;@`: the number section applies to every number.
            "value()>=0".to_owned()
        } else {
            odf_condition(&cond)
        };
        maps.push(
            El::new("style:map")
                .with("style:condition", condition)
                .with("style:apply-style-name", sub),
        );
    }
    let mut el = style_el(&main);
    el.attrs
        .insert(0, ("style:name".to_owned(), name.to_owned()));
    el.children.extend(maps);
    out.push(el);
    out
}

/// The single data style of a one-section format code, without a name (date and time fields of
/// text documents). `None` for `General` or an empty code. `time` is a hint for codes without
/// date or time items.
#[must_use]
pub fn data_style_el(code: &str, time: bool) -> Option<El> {
    let code = code.trim();
    if code.is_empty() || code.eq_ignore_ascii_case("general") {
        return None;
    }
    let first = split_sections(code).into_iter().next()?;
    let mut el = style_el(&parse_section(&first));
    if time && el.name == "number:number-style" {
        el.name = "number:time-style".to_owned();
    }
    Some(el)
}

// ------------------------------------------------------------------ ODF → Excel

fn needs_quotes(s: &str) -> bool {
    !s.chars().all(|c| {
        matches!(
            c,
            ' ' | '-'
                | '+'
                | '/'
                | '('
                | ')'
                | ':'
                | '!'
                | '^'
                | '&'
                | '\''
                | '~'
                | '{'
                | '}'
                | '<'
                | '>'
                | '='
                | ','
        )
    })
}

fn lit_code(s: &str, out: &mut String) {
    if s.is_empty() {
        return;
    }
    if s == "%" {
        out.push('%');
    } else if s.contains('%') {
        for (i, part) in s.split('%').enumerate() {
            if i > 0 {
                out.push('%');
            }
            lit_code(part, out);
        }
    } else if needs_quotes(s) {
        // Leading characters that need no quotes stay bare (`0 "kg"`, `-"£"`); the rest is
        // quoted whole (`"Total: "`).
        let bare = |c: char| !needs_quotes(&c.to_string());
        let start = s.len() - s.trim_start_matches(bare).len();
        out.push_str(&s[..start]);
        out.push('"');
        out.push_str(&s[start..].replace('"', ""));
        out.push('"');
    } else {
        out.push_str(s);
    }
}

/// A `number:text`, with `loext:blank-width-char` (`(` or `)1`: a character, then the position
/// in the text it replaces) as padding (`_(`).
fn text_code(e: &XmlElement, out: &mut String) {
    let text: Vec<char> = all_text(e).chars().collect();
    let mut pads: Vec<(usize, char)> = Vec::new();
    if let Some(spec) = e.attr("loext:blank-width-char") {
        let spec: Vec<char> = spec.chars().collect();
        let mut i = 0;
        while i < spec.len() {
            let c = spec[i];
            i += 1;
            let digits: String = spec[i..]
                .iter()
                .take_while(|d| d.is_ascii_digit())
                .collect();
            i += digits.len();
            let pos = digits.parse().unwrap_or(0);
            pads.push((pos, c));
        }
    }
    let mut lit = String::new();
    for (i, ch) in text.iter().enumerate() {
        if let Some((_, c)) = pads.iter().find(|(p, _)| *p == i) {
            lit_code(&std::mem::take(&mut lit), out);
            out.push('_');
            out.push(*c);
        } else {
            lit.push(*ch);
        }
    }
    lit_code(&lit, out);
}

fn attr_u32(e: &XmlElement, name: &str) -> Option<u32> {
    e.attr(name).and_then(|v| v.trim().parse().ok())
}

/// The most digits of any kind a number format asks for (Excel's limit on decimal places).
const MAX_DIGITS: u32 = 30;

/// A digit-count attribute, clamped to [`MAX_DIGITS`] (a file asking for billions of digits
/// would otherwise allocate the code).
fn attr_digits(e: &XmlElement, name: &str) -> Option<u32> {
    attr_u32(e, name).map(|n| n.min(MAX_DIGITS))
}

fn number_code(e: &XmlElement) -> String {
    let decimals = attr_digits(e, "number:decimal-places");
    let min_decimals = attr_digits(e, "number:min-decimal-places")
        .or_else(|| attr_digits(e, "loext:min-decimal-places"))
        .or(decimals)
        .unwrap_or(0);
    let min_int = attr_digits(e, "number:min-integer-digits").unwrap_or(0);
    let grouping = e.attr("number:grouping") == Some("true");
    if decimals.is_none() && !grouping && e.name == "number:number" && min_int <= 1 {
        let factor = e.attr("number:display-factor");
        if factor.is_none() {
            return "General".to_owned();
        }
    }
    let decimals = decimals.unwrap_or(0);
    let mut int = String::new();
    if grouping {
        let zeros = min_int.max(1) as usize;
        let total = zeros.max(4);
        let digits: String = std::iter::repeat_n('#', total - zeros)
            .chain(std::iter::repeat_n('0', zeros))
            .collect();
        // Group every three from the right: `#,##0`.
        let mut grouped = String::new();
        for (i, c) in digits.chars().enumerate() {
            let from_right = total - i;
            grouped.push(c);
            if from_right > 1 && (from_right - 1).is_multiple_of(3) {
                grouped.push(',');
            }
        }
        int.push_str(&grouped);
    } else if min_int == 0 {
        int.push('#');
    } else {
        int.extend(std::iter::repeat_n('0', min_int as usize));
    }
    // `??`: the last integer digits blank instead of zero.
    if let Some(b) = attr_digits(e, "loext:max-blank-integer-digits").filter(|b| *b > 0) {
        let mut chars: Vec<char> = int.chars().collect();
        let mut left = b;
        for c in chars.iter_mut().rev() {
            if left == 0 {
                break;
            }
            if *c == '0' {
                *c = '?';
                left -= 1;
            }
        }
        int = chars.into_iter().collect();
    }
    let mut s = int;
    if decimals > 0 {
        s.push('.');
        let md = min_decimals.min(decimals);
        s.extend(std::iter::repeat_n('0', md as usize));
        s.extend(std::iter::repeat_n('#', (decimals - md) as usize));
    }
    if let Some(f) = e
        .attr("number:display-factor")
        .and_then(|f| f.parse::<f64>().ok())
    {
        let mut v = f;
        while v >= 1000.0 - 1e-9 {
            s.push(',');
            v /= 1000.0;
        }
    }
    s
}

fn color_name(hex: &str) -> Option<&'static str> {
    let c = crate::units::color(hex)?;
    COLORS
        .iter()
        .min_by_key(|(_, h)| {
            let k = crate::units::color(h).unwrap_or(c);
            let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
            d(c.r, k.r) + d(c.g, k.g) + d(c.b, k.b)
        })
        .map(|(n, _)| *n)
}

fn section_code(style: &XmlElement) -> String {
    let mut out = String::new();
    if let Some(c) = kid(style, "style:text-properties")
        .and_then(|t| t.attr("fo:color"))
        .and_then(color_name)
    {
        out.push('[');
        out.push_str(c);
        out.push(']');
    }
    let elapsed = style.attr("number:truncate-on-overflow") == Some("false");
    let mut elapsed_done = false;
    let long = |e: &XmlElement| e.attr("number:style") == Some("long");
    let elements: Vec<&XmlElement> = style.elements().collect();
    for (k, e) in elements.iter().copied().enumerate() {
        match e.name.as_str() {
            // A dash before blank digits is quoted, as Excel writes it (`"-"??`).
            "number:text"
                if all_text(e) == "-"
                    && e.attr("loext:blank-width-char").is_none()
                    && elements.get(k + 1).is_some_and(|n| {
                        n.name == "number:number"
                            && n.attr("loext:max-blank-integer-digits").is_some()
                    }) =>
            {
                out.push_str("\"-\"");
            }
            "number:text" => text_code(e, &mut out),
            "number:fill-character" => {
                out.push('*');
                out.push(all_text(e).chars().next().unwrap_or(' '));
            }
            "number:number" => {
                let code = number_code(e);
                // Embedded text (`000-0000`) is inserted at its position from the decimal point.
                let embedded: Vec<(usize, String)> = kids(e, "number:embedded-text")
                    .filter_map(|t| {
                        let p: usize = t.attr("number:position")?.parse().ok()?;
                        Some((p, all_text(t)))
                    })
                    .collect();
                if embedded.is_empty() {
                    out.push_str(&code);
                } else {
                    let int_end = code.find('.').unwrap_or(code.len());
                    let (int, rest) = code.split_at(int_end);
                    let mut digits: Vec<String> = int
                        .chars()
                        .filter(|c| *c != ',')
                        .map(String::from)
                        .collect();
                    let mut embedded = embedded;
                    embedded.sort_by_key(|(p, _)| *p);
                    let places = |d: &[String]| {
                        d.iter()
                            .filter(|x| {
                                x.len() == 1 && is_placeholder(x.chars().next().unwrap_or(' '))
                            })
                            .count()
                    };
                    for (pos, text) in embedded {
                        let pos = pos.min(MAX_DIGITS as usize);
                        // Digits to the left of the text: optional ones (LibreOffice leaves them
                        // out, Excel needs at least one).
                        if places(&digits) <= pos {
                            while places(&digits) < pos + 3 {
                                digits.insert(0, "#".to_owned());
                            }
                        }
                        let mut lit = String::new();
                        lit_code(&text, &mut lit);
                        // The index after which `pos` placeholders remain.
                        let mut seen = 0;
                        let mut at = digits.len();
                        for (i, d) in digits.iter().enumerate().rev() {
                            if seen == pos {
                                at = i + 1;
                                break;
                            }
                            if d.len() == 1 && is_placeholder(d.chars().next().unwrap_or(' ')) {
                                seen += 1;
                            }
                            at = i;
                        }
                        digits.insert(at, lit);
                    }

                    out.push_str(&digits.concat());
                    out.push_str(rest);
                }
            }
            "number:scientific-number" => {
                let mut c = number_code(e);
                if c == "General" {
                    c = "0".to_owned();
                }
                // Engineering notation: the integer part holds `interval` digits (`##0`).
                if let Some(iv) = attr_digits(e, "number:exponent-interval").filter(|v| *v > 1) {
                    let int_len = c.find('.').unwrap_or(c.len());
                    let have = c[..int_len].chars().filter(|x| is_placeholder(*x)).count();
                    for _ in have..iv as usize {
                        c.insert(0, '#');
                    }
                }
                out.push_str(&c);
                out.push_str("E+");
                let d = attr_digits(e, "number:min-exponent-digits")
                    .unwrap_or(2)
                    .max(1);
                out.extend(std::iter::repeat_n('0', d as usize));
            }
            "number:fraction" => {
                let int = attr_digits(e, "number:min-integer-digits");
                let num = attr_digits(e, "number:min-numerator-digits")
                    .unwrap_or(1)
                    .max(1);
                if let Some(i) = int {
                    if i == 0 {
                        out.push('#');
                    } else {
                        out.extend(std::iter::repeat_n('0', i as usize));
                    }
                    out.push(' ');
                }
                out.extend(std::iter::repeat_n('?', num as usize));
                out.push('/');
                if let Some(v) = attr_u32(e, "number:denominator-value") {
                    out.push_str(&v.to_string());
                } else {
                    let d = attr_digits(e, "number:min-denominator-digits")
                        .unwrap_or(1)
                        .max(1);
                    out.extend(std::iter::repeat_n('?', d as usize));
                }
            }
            "number:text-content" => out.push('@'),
            "number:currency-symbol" => {
                let sym = all_text(e);
                if sym == "$" {
                    out.push('$');
                } else if !sym.is_empty() {
                    out.push_str("[$");
                    out.push_str(&sym);
                    out.push(']');
                }
            }
            "number:day" => out.push_str(if long(e) { "dd" } else { "d" }),
            "number:day-of-week" => out.push_str(if long(e) { "dddd" } else { "ddd" }),
            "number:month" => {
                let textual = e.attr("number:textual") == Some("true");
                out.push_str(match (textual, long(e)) {
                    (true, true) => "mmmm",
                    (true, false) => "mmm",
                    (false, true) => "mm",
                    (false, false) => "m",
                });
            }
            "number:year" => out.push_str(if long(e) { "yyyy" } else { "yy" }),
            "number:hours" | "number:minutes" | "number:seconds" => {
                let letter = match e.name.as_str() {
                    "number:hours" => 'h',
                    "number:minutes" => 'm',
                    _ => 's',
                };
                let n = if long(e) { 2 } else { 1 };
                let tok: String = std::iter::repeat_n(letter, n).collect();
                if elapsed && !elapsed_done {
                    out.push('[');
                    out.push_str(&tok);
                    out.push(']');
                    elapsed_done = true;
                } else {
                    out.push_str(&tok);
                }
                if letter == 's'
                    && let Some(d) = attr_digits(e, "number:decimal-places").filter(|d| *d > 0)
                {
                    out.push('.');
                    out.extend(std::iter::repeat_n('0', d as usize));
                }
            }
            "number:am-pm" => out.push_str("AM/PM"),
            "number:boolean" => out.push_str("\"TRUE\";\"TRUE\";\"FALSE\""),
            "number:quarter" => out.push_str("\"Q\""),
            _ => {}
        }
    }
    out
}

fn excel_condition(odf: &str) -> Option<String> {
    let c = odf.trim().strip_prefix("value()")?.trim().replace(' ', "");
    (!c.is_empty()).then_some(c)
}

/// The Excel format code of a data style, with its `style:map` sections resolved through
/// `others` (data styles by name). `None` when the element is not a data style.
#[must_use]
pub fn read_data_style_in(
    style: &XmlElement,
    others: &HashMap<&str, &XmlElement>,
) -> Option<String> {
    if !style.name.starts_with("number:") || !style.name.ends_with("-style") {
        return None;
    }
    let mut sections: Vec<(Option<String>, String)> = Vec::new();
    for m in kids(style, "style:map").take(3) {
        let (Some(cond), Some(target)) =
            (m.attr("style:condition"), m.attr("style:apply-style-name"))
        else {
            continue;
        };
        let Some(sub) = others.get(target) else {
            continue;
        };
        sections.push((excel_condition(cond), section_code(sub)));
    }
    let main = section_code(style);
    if style.name == "number:boolean-style" {
        // Logical values show TRUE and FALSE in General already.
        return Some("General".to_owned());
    }
    if sections.is_empty() {
        return Some(main);
    }
    // Drop conditions that Excel implies by position.
    let text_main = style.name == "number:text-style";
    let implied: &[&str] = match (sections.len(), text_main) {
        (1, false) => &[">=0"],
        (1, true) => &[">=0"],
        (2, _) if text_main => &[">=0", "<0"],
        (2, _) => &[">0", "<0"],
        _ => &[">0", "<0", "=0"],
    };
    let mut parts: Vec<String> = Vec::new();
    let all_implied = sections
        .iter()
        .enumerate()
        .all(|(i, (c, _))| c.as_deref() == implied.get(i).copied());
    for (c, code) in &sections {
        match c {
            Some(c) if !all_implied => parts.push(format!("[{c}]{code}")),
            _ => parts.push(code.clone()),
        }
    }
    // `0;@`: a single number section before the text section.
    if text_main && sections.len() == 1 {
        return Some(format!("{};{main}", parts[0]));
    }
    if text_main && sections.len() == 2 {
        // Excel needs the zero section before the text one.
        return Some(format!("{};{};{};{main}", parts[0], parts[1], parts[0]));
    }
    parts.push(main);
    Some(parts.join(";"))
}

/// The Excel format code of a data style without `style:map` resolution.
#[must_use]
pub fn read_data_style(style: &XmlElement) -> Option<String> {
    read_data_style_in(style, &HashMap::new())
}

// ------------------------------------------------------------------ Word date pictures

/// An Excel date/time code as a Word date picture (`d mmmm yyyy` → `d MMMM yyyy`, minutes as
/// `m`, 24-hour clocks as `H`).
#[must_use]
pub fn excel_to_word_picture(code: &str) -> String {
    let first = split_sections(code).into_iter().next().unwrap_or_default();
    let sec = parse_section(&first);
    let ampm = sec.items.iter().any(|i| matches!(i, Item::AmPm));
    let mut out = String::new();
    let rep = |c: char, n: usize| -> String { std::iter::repeat_n(c, n).collect() };
    for it in &sec.items {
        match it {
            Item::Lit(s) => {
                if s.chars().all(|c| !c.is_alphabetic()) {
                    out.push_str(s);
                } else {
                    out.push('\'');
                    out.push_str(s);
                    out.push('\'');
                }
            }
            Item::Pad(_) => out.push(' '),
            Item::Fill(_) => {}
            Item::Day(l) => out.push_str(&rep('d', if *l { 2 } else { 1 })),
            Item::DayOfWeek(l) => out.push_str(&rep('d', if *l { 4 } else { 3 })),
            Item::Month { long, textual } => out.push_str(&rep(
                'M',
                match (textual, long) {
                    (true, true) => 4,
                    (true, false) => 3,
                    (false, true) => 2,
                    (false, false) => 1,
                },
            )),
            Item::Year(l) => out.push_str(&rep('y', if *l { 4 } else { 2 })),
            Item::Hours { long, .. } => {
                out.push_str(&rep(
                    if ampm { 'h' } else { 'H' },
                    if *long { 2 } else { 1 },
                ));
            }
            Item::Minutes { long, .. } => out.push_str(&rep('m', if *long { 2 } else { 1 })),
            Item::Seconds { long, .. } => out.push_str(&rep('s', if *long { 2 } else { 1 })),
            Item::AmPm => out.push_str("AM/PM"),
            Item::Number(_)
            | Item::Sci { .. }
            | Item::Frac { .. }
            | Item::Text
            | Item::Currency(_) => {}
        }
    }
    out
}

/// A Word date picture as an Excel date/time code (the inverse of [`excel_to_word_picture`]).
#[must_use]
pub fn word_picture_to_excel(pic: &str) -> String {
    let chars: Vec<char> = pic.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            let mut j = i + 1;
            let mut lit = String::new();
            while j < chars.len() && chars[j] != '\'' {
                lit.push(chars[j]);
                j += 1;
            }
            lit_code(&lit, &mut out);
            i = j + 1;
            continue;
        }
        let up: String = chars[i..].iter().collect::<String>().to_ascii_uppercase();
        if up.starts_with("AM/PM") {
            out.push_str("AM/PM");
            i += 5;
            continue;
        }
        let run = chars[i..].iter().take_while(|x| **x == c).count();
        let rep = |ch: char, n: usize| -> String { std::iter::repeat_n(ch, n).collect() };
        match c {
            'M' => out.push_str(&rep('m', run.min(4))),
            'd' | 'D' => out.push_str(&rep('d', run.min(4))),
            'y' | 'Y' => out.push_str(if run > 2 { "yyyy" } else { "yy" }),
            'H' | 'h' => out.push_str(&rep('h', run.min(2))),
            'm' => out.push_str(&rep('m', run.min(2))),
            's' | 'S' => out.push_str(&rep('s', run.min(2))),
            _ => lit_code(&rep(c, run), &mut out),
        }
        i += run;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::{XmlOut, parse_part};

    /// Writes the styles of a code into a document and reads the main one back.
    fn round_trip(code: &str) -> String {
        let els = data_style_els("N1", code);
        let mut w = XmlOut::new();
        w.root(
            "office:styles",
            &["office", "style", "number", "fo", "loext"],
        );
        for e in &els {
            e.write(&mut w);
        }
        let bytes = w.finish();
        let root = parse_part("styles.xml", &bytes).unwrap();
        let all: HashMap<&str, &XmlElement> = root
            .elements()
            .filter_map(|e| Some((e.attr("style:name")?, e)))
            .collect();
        read_data_style_in(all["N1"], &all).unwrap()
    }

    #[test]
    fn common_codes_round_trip() {
        for code in [
            "0",
            "0.00",
            "#,##0",
            "#,##0.00",
            "0%",
            "0.00%",
            "0.00E+00",
            "# ?/?",
            "# ??/??",
            "# ?/4",
            "d/m/yyyy",
            "dd/mm/yyyy",
            "d mmmm yyyy",
            "dddd, d mmm yy",
            "hh:mm",
            "h:mm:ss AM/PM",
            "[h]:mm:ss",
            "mm:ss.0",
            "yyyy-mm-dd hh:mm:ss",
            "@",
            "#,##0;[Red]-#,##0",
            "#,##0.00;-#,##0.00;0.00",
            "[>=100]0;0.00",
            "#,##0,",
            "0.0#",
            "?/?",
            "??/??",
            "# ?/8",
        ] {
            assert_eq!(round_trip(code), code, "{code}");
        }
    }

    /// Codes ODF holds through LibreOffice's extensions: padding, fills, embedded literals,
    /// blank digits, engineering exponents, hidden values.
    #[test]
    fn accounting_phone_engineering_and_hidden_codes_round_trip() {
        for code in [
            r#"_("$"* #,##0.00_);_("$"* (#,##0.00);_("$"* "-"??_);_(@_)"#,
            r#"_-* #,##0.00_-;-* #,##0.00_-;_-* "-"??_-;_-@_-"#,
            "#,##0_);(#,##0)",
            "#,##0.00_);[Red](#,##0.00)",
            "[<=9999999]###-####;(###) ###-####",
            "000-00-0000",
            "##0.0E+0",
            "##0.00E+00",
            ";;;",
            r##""Total: "#,##0"##,
            r#"0 "kg""#,
        ] {
            assert_eq!(round_trip(code), code, "{code}");
        }
    }

    #[test]
    fn literals_and_currency() {
        assert_eq!(round_trip("0 \"kg\""), "0 \"kg\"");
        assert_eq!(
            round_trip(" #,##0.00 ;-#,##0.00 ; -00 ; @ "),
            " #,##0.00 ;-#,##0.00 ; -00 ; @ "
        );
        assert_eq!(round_trip("[$€]#,##0.00"), "[$€]#,##0.00");
        assert_eq!(round_trip("$#,##0.00"), "$#,##0.00");
        assert_eq!(round_trip("[$€-407]#,##0.00"), "[$€]#,##0.00");
        assert_eq!(round_trip("0.00_);(0.00)"), "0.00_);(0.00)");
        assert!(data_style_els("N", "General").is_empty());
        let els = data_style_els("N9", "0;-0;0;@");
        assert_eq!(els.last().unwrap().name, "number:text-style");
        assert_eq!(els.len(), 4);
    }

    #[test]
    fn styles_pick_their_family() {
        let k = |c: &str| data_style_els("N", c).last().unwrap().name.clone();
        assert_eq!(k("0.00"), "number:number-style");
        assert_eq!(k("0%"), "number:percentage-style");
        assert_eq!(k("[$€]0"), "number:currency-style");
        assert_eq!(k("d/m/yy"), "number:date-style");
        assert_eq!(k("hh:mm"), "number:time-style");
        assert_eq!(k("@"), "number:text-style");
        let e = data_style_els("N", "[h]:mm");
        assert_eq!(e[0].get("number:truncate-on-overflow"), Some("false"));
    }

    #[test]
    fn word_pictures_convert_both_ways() {
        assert_eq!(excel_to_word_picture("d mmmm yyyy"), "d MMMM yyyy");
        assert_eq!(excel_to_word_picture("hh:mm"), "HH:mm");
        assert_eq!(excel_to_word_picture("h:mm AM/PM"), "h:mm AM/PM");
        assert_eq!(word_picture_to_excel("d MMMM yyyy"), "d mmmm yyyy");
        assert_eq!(word_picture_to_excel("HH:mm"), "hh:mm");
        assert_eq!(word_picture_to_excel("dddd, 'the' d"), "dddd, \"the\" d");
        for pic in [
            "d MMMM yyyy",
            "dd/MM/yy",
            "HH:mm:ss",
            "h:mm AM/PM",
            "dddd d MMM",
        ] {
            assert_eq!(
                excel_to_word_picture(&word_picture_to_excel(pic)),
                pic,
                "{pic}"
            );
        }
    }

    #[test]
    fn malformed_codes_do_not_panic() {
        for code in [
            "[", "\"", "\\", "_", "*", "0.", ".0", "E+", "?/", "[$", "[>", ";;;;;;", "[Red",
            "0/0/0",
        ] {
            let _ = data_style_els("N", code);
            let _ = excel_to_word_picture(code);
            let _ = word_picture_to_excel(code);
        }
    }
}
