// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fields (ECMA-376 §17.16): the model's field object and instruction parsing.
//!
//! [`parse_field_instruction`] tokenises Word's syntax (quoted arguments, `\x` switches, the
//! general `\*`, numeric `\#` and date-time `\@` format switches) and gives typed views of PAGE,
//! NUMPAGES, SECTIONPAGES, SECTION, DATE, TIME, CREATEDATE, SAVEDATE, PRINTDATE, TOC, REF,
//! PAGEREF, NOTEREF, HYPERLINK and SEQ. Any other field keeps its tokens and raw text, so it
//! round-trips unchanged. Parsing never fails.

use serde::{Deserialize, Serialize};

use crate::numbering::formats::format_number;
use crate::st::NumberFormat;

/// Whether a field was stored as `w:fldSimple` or as `w:fldChar` begin/separate/end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum FieldForm {
    /// `w:fldSimple`.
    Simple,
    /// `w:fldChar` + `w:instrText`.
    #[default]
    Complex,
}

/// A field in a paragraph: its instruction and the result Word last computed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Field {
    /// The instruction, e.g. `PAGE \* MERGEFORMAT`.
    pub instr: String,
    /// The cached result shown until fields update.
    pub result: String,
    /// How the file stored it.
    pub form: FieldForm,
    /// `w:dirty`: the result is stale.
    pub dirty: bool,
    /// `w:fldLock`: the result must not be updated.
    pub locked: bool,
}

impl Field {
    /// A complex field with a cached result.
    #[must_use]
    pub fn new(instr: &str, result: &str) -> Self {
        Self {
            instr: instr.to_owned(),
            result: result.to_owned(),
            ..Self::default()
        }
    }

    /// The parsed instruction.
    #[must_use]
    pub fn parsed(&self) -> FieldInstruction {
        parse_field_instruction(&self.instr)
    }
}

/// A switch: name (`\o`, `\h`, `\*`, `\@`) and its argument.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldSwitch {
    /// The name including the backslash; letter switches are lower-cased.
    pub name: String,
    /// The argument, if the switch takes one.
    pub value: Option<String>,
}

/// An inclusive range of levels (`\o "1-3"`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LevelRange {
    /// First level (1-based).
    pub from: u32,
    /// Last level.
    pub to: u32,
}

/// Which pages a TOC omits page numbers for (`\n`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OmitPageNumbers {
    /// Every level.
    All,
    /// These levels.
    Levels(LevelRange),
}

/// A TOC field.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TocField {
    /// `\o "1-3"`: heading levels to include.
    pub outline_levels: Option<LevelRange>,
    /// `\t "Style,1,Other,2"`: custom styles and their TOC levels.
    pub styles: Vec<(String, u32)>,
    /// `\h`: entries are hyperlinks.
    pub hyperlinks: bool,
    /// `\z`: hide leaders and page numbers in Web Layout.
    pub hide_in_web_layout: bool,
    /// `\u`: use paragraph outline levels.
    pub use_outline_levels: bool,
    /// `\n`: omit page numbers.
    pub omit_page_numbers: Option<OmitPageNumbers>,
    /// `\c "Figure"`: a table of figures for a caption label.
    pub caption_label: Option<String>,
    /// `\b`: limit to a bookmarked range.
    pub bookmark: Option<String>,
    /// `\p`: separator between entry and page number.
    pub separator: Option<String>,
}

/// REF `\n`, `\r`, `\w`: paragraph number of the bookmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ParagraphNumberContext {
    /// `\n`.
    NoContext,
    /// `\r`.
    Relative,
    /// `\w`.
    FullContext,
}

/// REF, PAGEREF and NOTEREF.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RefField {
    /// The bookmark.
    pub bookmark: String,
    /// `\h`.
    pub hyperlink: bool,
    /// `\p`: "above"/"below".
    pub relative_position: bool,
    /// REF paragraph number switches.
    pub paragraph_number: Option<ParagraphNumberContext>,
}

/// HYPERLINK.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct HyperlinkField {
    /// The target URL.
    pub url: Option<String>,
    /// `\l`: a location (bookmark) in the target.
    pub anchor: Option<String>,
    /// `\o`: screen tip.
    pub tooltip: Option<String>,
    /// `\t`: target frame.
    pub target: Option<String>,
    /// `\n`: new window.
    pub new_window: bool,
}

/// SEQ.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SeqField {
    /// The sequence name (`Figure`, `Table`).
    pub identifier: String,
    /// A bookmark to refer to.
    pub bookmark: Option<String>,
    /// `\c`: repeat the closest preceding number.
    pub repeat: bool,
    /// `\h`: hide the result.
    pub hidden: bool,
    /// `\r n`: reset to n.
    pub reset_to: Option<i64>,
    /// `\s n`: reset at heading level n.
    pub reset_at_heading: Option<u32>,
}

/// The page-number fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PageFieldKind {
    /// PAGE.
    Page,
    /// NUMPAGES.
    NumPages,
    /// SECTIONPAGES.
    SectionPages,
    /// SECTION.
    Section,
}

/// The date fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DateFieldKind {
    /// DATE.
    Date,
    /// TIME.
    Time,
    /// CREATEDATE.
    CreateDate,
    /// SAVEDATE.
    SaveDate,
    /// PRINTDATE.
    PrintDate,
}

/// What kind of field an instruction is, with its typed switches.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FieldKind {
    /// PAGE, NUMPAGES, SECTIONPAGES, SECTION.
    Page(PageFieldKind),
    /// A date field and its effective picture (Word's default when no `\@`).
    Date {
        /// Which one.
        kind: DateFieldKind,
        /// The date-time picture.
        picture: String,
    },
    /// TOC.
    Toc(TocField),
    /// REF.
    Ref(RefField),
    /// PAGEREF.
    PageRef(RefField),
    /// NOTEREF.
    NoteRef(RefField),
    /// HYPERLINK.
    Hyperlink(HyperlinkField),
    /// SEQ.
    Seq(SeqField),
    /// Anything else (MERGEFIELD, `=` formulas, INCLUDETEXT…), kept as tokens.
    Unknown,
}

/// A parsed field instruction.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldInstruction {
    /// The upper-cased keyword (`PAGE`; empty for empty input).
    pub keyword: String,
    /// Positional (non-switch) arguments.
    pub args: Vec<String>,
    /// All switches in order.
    pub switches: Vec<FieldSwitch>,
    /// General formatting switches (`\* MERGEFORMAT`, `\* roman`).
    pub general: Vec<String>,
    /// Numeric picture (`\# "0.00"`).
    pub numeric_format: Option<String>,
    /// Date-time picture (`\@ "d MMMM yyyy"`).
    pub date_format: Option<String>,
    /// The original instruction, verbatim.
    pub raw: String,
    /// The typed view.
    pub kind: FieldKind,
}

impl FieldInstruction {
    /// True when the keyword is not one of the modelled fields.
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        self.kind == FieldKind::Unknown
    }

    fn switch(&self, name: &str) -> Option<&FieldSwitch> {
        self.switches.iter().find(|s| s.name == name)
    }

    fn has(&self, name: &str) -> bool {
        self.switch(name).is_some()
    }

    fn value(&self, name: &str) -> Option<String> {
        self.switch(name).and_then(|s| s.value.clone())
    }
}

/// A token of an instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldToken {
    /// The text (quotes removed, escapes resolved).
    pub text: String,
    /// Whether it was quoted.
    pub quoted: bool,
}

fn is_quote(c: char) -> bool {
    matches!(c, '"' | '\u{201C}' | '\u{201D}')
}

/// Splits an instruction into tokens. Quotes (straight or curly, as Word accepts) group words,
/// `\"` and `\\` escape inside them, and a missing closing quote is tolerated.
#[must_use]
pub fn tokenize_field(instr: &str) -> Vec<FieldToken> {
    let s: Vec<char> = instr.chars().collect();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < s.len() {
        let ch = s[i];
        if ch.is_whitespace() {
            i += 1;
            continue;
        }
        if is_quote(ch) {
            let mut text = String::new();
            i += 1;
            while i < s.len() && !is_quote(s[i]) {
                if s[i] == '\\' && i + 1 < s.len() && (s[i + 1] == '"' || s[i + 1] == '\\') {
                    text.push(s[i + 1]);
                    i += 2;
                } else {
                    text.push(s[i]);
                    i += 1;
                }
            }
            i += 1;
            tokens.push(FieldToken { text, quoted: true });
            continue;
        }
        if ch == '\\'
            && i + 1 < s.len()
            && (s[i + 1].is_ascii_alphabetic() || matches!(s[i + 1], '*' | '#' | '@' | '!'))
        {
            tokens.push(FieldToken {
                text: format!("\\{}", s[i + 1]),
                quoted: false,
            });
            i += 2;
            continue;
        }
        let mut text = String::new();
        while i < s.len() && !s[i].is_whitespace() && s[i] != '"' {
            if s[i] == '\\' && i + 1 < s.len() && s[i + 1] == '\\' {
                text.push('\\');
                i += 2;
                continue;
            }
            text.push(s[i]);
            i += 1;
        }
        tokens.push(FieldToken {
            text,
            quoted: false,
        });
    }
    tokens
}

fn is_switch(t: &FieldToken) -> bool {
    if t.quoted {
        return false;
    }
    let mut c = t.text.chars();
    c.next() == Some('\\')
        && c.next()
            .is_some_and(|x| x.is_ascii_alphabetic() || matches!(x, '*' | '#' | '@' | '!'))
        && c.next().is_none()
}

/// Switches that never take an argument, per field type.
fn flag_switches(keyword: &str) -> &'static [&'static str] {
    match keyword {
        "TOC" => &["\\h", "\\z", "\\u", "\\w", "\\x"],
        "REF" => &["\\h", "\\n", "\\r", "\\w", "\\p", "\\f", "\\t"],
        "NOTEREF" => &["\\h", "\\p", "\\f"],
        "PAGEREF" => &["\\h", "\\p"],
        "HYPERLINK" => &["\\m", "\\n"],
        "SEQ" => &["\\c", "\\h", "\\n"],
        "DATE" => &["\\h", "\\l", "\\s"],
        _ => &[],
    }
}

fn parse_range(v: Option<&str>) -> Option<LevelRange> {
    let v = v?.trim();
    if let Some((a, b)) = v.split_once('-') {
        let from = a.trim().parse().ok()?;
        let to = b.trim().parse().ok()?;
        return Some(LevelRange { from, to });
    }
    let n = v.parse().ok()?;
    Some(LevelRange { from: n, to: n })
}

fn default_picture(kind: DateFieldKind) -> &'static str {
    match kind {
        DateFieldKind::Date => "M/d/yyyy",
        DateFieldKind::Time => "h:mm am/pm",
        _ => "M/d/yyyy h:mm:ss am/pm",
    }
}

/// Parses a field instruction. Never fails: malformed input yields an `Unknown` field.
#[must_use]
pub fn parse_field_instruction(instr: &str) -> FieldInstruction {
    let tokens = tokenize_field(instr);
    let keyword = tokens
        .first()
        .filter(|t| !t.quoted)
        .map(|t| t.text.to_uppercase())
        .unwrap_or_default();
    let flags = flag_switches(&keyword);
    let mut f = FieldInstruction {
        keyword: keyword.clone(),
        args: Vec::new(),
        switches: Vec::new(),
        general: Vec::new(),
        numeric_format: None,
        date_format: None,
        raw: instr.to_owned(),
        kind: FieldKind::Unknown,
    };
    let mut i = 1;
    while i < tokens.len() {
        let t = &tokens[i];
        if is_switch(t) {
            let name = if matches!(t.text.as_str(), "\\*" | "\\#" | "\\@" | "\\!") {
                t.text.clone()
            } else {
                t.text.to_lowercase()
            };
            let next = tokens.get(i + 1);
            let takes_value = name != "\\!"
                && !flags.contains(&name.as_str())
                && next.is_some_and(|n| !is_switch(n));
            let value = if takes_value {
                i += 1;
                next.map(|n| n.text.clone())
            } else {
                None
            };
            match (name.as_str(), &value) {
                ("\\*", Some(v)) if !v.is_empty() => f.general.push(v.clone()),
                ("\\#", Some(v)) => f.numeric_format = Some(v.clone()),
                ("\\@", Some(v)) => f.date_format = Some(v.clone()),
                _ => {}
            }
            f.switches.push(FieldSwitch { name, value });
        } else {
            f.args.push(t.text.clone());
        }
        i += 1;
    }
    f.kind = match keyword.as_str() {
        "PAGE" => FieldKind::Page(PageFieldKind::Page),
        "NUMPAGES" => FieldKind::Page(PageFieldKind::NumPages),
        "SECTIONPAGES" => FieldKind::Page(PageFieldKind::SectionPages),
        "SECTION" => FieldKind::Page(PageFieldKind::Section),
        "DATE" | "TIME" | "CREATEDATE" | "SAVEDATE" | "PRINTDATE" => {
            let kind = match keyword.as_str() {
                "DATE" => DateFieldKind::Date,
                "TIME" => DateFieldKind::Time,
                "CREATEDATE" => DateFieldKind::CreateDate,
                "SAVEDATE" => DateFieldKind::SaveDate,
                _ => DateFieldKind::PrintDate,
            };
            FieldKind::Date {
                kind,
                picture: f
                    .date_format
                    .clone()
                    .unwrap_or_else(|| default_picture(kind).to_owned()),
            }
        }
        "TOC" => FieldKind::Toc(parse_toc(&f)),
        "REF" | "PAGEREF" | "NOTEREF" => {
            let mut r = RefField {
                bookmark: f.args.first().cloned().unwrap_or_default(),
                hyperlink: f.has("\\h"),
                relative_position: f.has("\\p"),
                paragraph_number: None,
            };
            match keyword.as_str() {
                "REF" => {
                    r.paragraph_number = if f.has("\\w") {
                        Some(ParagraphNumberContext::FullContext)
                    } else if f.has("\\r") {
                        Some(ParagraphNumberContext::Relative)
                    } else if f.has("\\n") {
                        Some(ParagraphNumberContext::NoContext)
                    } else {
                        None
                    };
                    FieldKind::Ref(r)
                }
                "PAGEREF" => FieldKind::PageRef(r),
                _ => FieldKind::NoteRef(r),
            }
        }
        "HYPERLINK" => FieldKind::Hyperlink(HyperlinkField {
            url: f.args.first().cloned(),
            anchor: f.value("\\l"),
            tooltip: f.value("\\o"),
            target: f.value("\\t"),
            new_window: f.has("\\n"),
        }),
        "SEQ" => FieldKind::Seq(SeqField {
            identifier: f.args.first().cloned().unwrap_or_default(),
            bookmark: f.args.get(1).cloned(),
            repeat: f.has("\\c"),
            hidden: f.has("\\h"),
            reset_to: f.value("\\r").and_then(|v| v.trim().parse().ok()),
            reset_at_heading: f.value("\\s").and_then(|v| v.trim().parse().ok()),
        }),
        _ => FieldKind::Unknown,
    };
    f
}

fn parse_toc(f: &FieldInstruction) -> TocField {
    let mut toc = TocField {
        hyperlinks: f.has("\\h"),
        hide_in_web_layout: f.has("\\z"),
        use_outline_levels: f.has("\\u"),
        ..TocField::default()
    };
    if f.has("\\o") {
        toc.outline_levels =
            Some(parse_range(f.value("\\o").as_deref()).unwrap_or(LevelRange { from: 1, to: 9 }));
    }
    if let Some(t) = f.value("\\t") {
        let sep = if t.contains(';') { ';' } else { ',' };
        let parts: Vec<&str> = t.split(sep).map(str::trim).collect();
        for pair in parts.chunks(2) {
            let style = pair[0];
            if !style.is_empty() {
                let level = pair
                    .get(1)
                    .and_then(|l| l.parse::<u32>().ok())
                    .filter(|l| *l > 0)
                    .unwrap_or(1);
                toc.styles.push((style.to_owned(), level));
            }
        }
    }
    if f.has("\\n") {
        toc.omit_page_numbers = Some(
            parse_range(f.value("\\n").as_deref())
                .map_or(OmitPageNumbers::All, OmitPageNumbers::Levels),
        );
    }
    toc.caption_label = f.value("\\c");
    toc.bookmark = f.value("\\b").filter(|b| !b.is_empty());
    toc.separator = f.value("\\p");
    toc
}

fn quote_arg(v: &str) -> String {
    if v.is_empty()
        || v.chars()
            .any(|c| c.is_whitespace() || c == '"' || c == '\\')
    {
        format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        v.to_owned()
    }
}

/// Serialises a keyword, arguments and switches in canonical form (`TOC \o "1-3" \h`).
#[must_use]
pub fn serialize_field_instruction(
    keyword: &str,
    args: &[String],
    switches: &[FieldSwitch],
) -> String {
    let mut parts = vec![keyword.to_owned()];
    parts.extend(args.iter().map(|a| quote_arg(a)));
    for s in switches {
        parts.push(s.name.clone());
        if let Some(v) = &s.value {
            parts.push(quote_arg(v));
        }
    }
    parts.join(" ")
}

/// Builds an instruction from parts, e.g. `build_field("TOC", &[], &[("\\o", Some("1-3")),
/// ("\\h", None)])`.
#[must_use]
pub fn build_field(keyword: &str, args: &[&str], switches: &[(&str, Option<&str>)]) -> String {
    let args: Vec<String> = args.iter().map(|a| (*a).to_owned()).collect();
    let switches: Vec<FieldSwitch> = switches
        .iter()
        .map(|(n, v)| FieldSwitch {
            name: (*n).to_owned(),
            value: v.map(str::to_owned),
        })
        .collect();
    serialize_field_instruction(&keyword.to_uppercase(), &args, &switches)
}

/// Word's default TOC instruction (References ▸ Table of Contents ▸ Automatic Table 1).
pub const DEFAULT_TOC_INSTRUCTION: &str = "TOC \\o \"1-3\" \\h \\z \\u";

// ------------------------------------------------------------------ results

/// A local date and time for field results.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FieldDateTime {
    /// Year (proleptic Gregorian).
    pub year: i32,
    /// Month 1–12.
    pub month: u8,
    /// Day 1–31.
    pub day: u8,
    /// Hour 0–23.
    pub hour: u8,
    /// Minute 0–59.
    pub minute: u8,
    /// Second 0–59.
    pub second: u8,
}

impl FieldDateTime {
    /// Day of the week, 0 = Sunday (Sakamoto's method).
    #[must_use]
    pub fn weekday(&self) -> usize {
        const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let m = usize::from(self.month.clamp(1, 12));
        let y = if m < 3 { self.year - 1 } else { self.year };
        let d = i32::from(self.day);
        let w = (y + y.div_euclid(4) - y.div_euclid(100) + y.div_euclid(400) + T[m - 1] + d)
            .rem_euclid(7);
        usize::try_from(w).unwrap_or(0)
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// Formats a date with a Word date-time picture (`\@`): `d dd ddd dddd`, `M MM MMM MMMM`,
/// `yy yyyy`, `h hh H HH`, `m mm`, `s ss`, `am/pm AM/PM`, and `'quoted literals'`. English names.
#[must_use]
pub fn format_field_date(picture: &str, date: &FieldDateTime) -> String {
    let p: Vec<char> = picture.chars().collect();
    let lower: String = picture.to_lowercase();
    let has_ampm = lower.contains("am/pm");
    let month = usize::from(date.month.clamp(1, 12)) - 1;
    let day_name = DAYS[date.weekday()];
    let mut out = String::new();
    let mut i = 0;
    while i < p.len() {
        let ch = p[i];
        if ch == '\'' {
            let rest: String = p[i + 1..].iter().collect();
            match rest.find('\'') {
                Some(end) => {
                    out.push_str(&rest[..end]);
                    i += 1 + rest[..end].chars().count() + 1;
                }
                None => {
                    out.push_str(&rest);
                    i = p.len();
                }
            }
            continue;
        }
        let token: String = p[i..p.len().min(i + 5)].iter().collect();
        if token.eq_ignore_ascii_case("am/pm") {
            let pm = date.hour >= 12;
            out.push_str(match (token.as_str(), pm) {
                ("am/pm", true) => "pm",
                ("am/pm", false) => "am",
                (_, true) => "PM",
                (_, false) => "AM",
            });
            i += 5;
            continue;
        }
        let mut n = 0;
        while i + n < p.len() && p[i + n] == ch {
            n += 1;
        }
        let two = |v: u32| format!("{v:02}");
        match ch {
            'd' => out.push_str(&match n {
                1 => date.day.to_string(),
                2 => two(u32::from(date.day)),
                3 => day_name[..3].to_owned(),
                _ => day_name.to_owned(),
            }),
            'M' => out.push_str(&match n {
                1 => (month + 1).to_string(),
                2 => format!("{:02}", month + 1),
                3 => MONTHS[month][..3].to_owned(),
                _ => MONTHS[month].to_owned(),
            }),
            'y' => out.push_str(&if n <= 2 {
                format!("{:02}", date.year.rem_euclid(100))
            } else {
                date.year.to_string()
            }),
            'h' => {
                let h = if has_ampm {
                    match date.hour % 12 {
                        0 => 12,
                        h => h,
                    }
                } else {
                    date.hour
                };
                out.push_str(&if n == 1 {
                    h.to_string()
                } else {
                    two(u32::from(h))
                });
            }
            'H' => out.push_str(&if n == 1 {
                date.hour.to_string()
            } else {
                two(u32::from(date.hour))
            }),
            'm' => out.push_str(&if n == 1 {
                date.minute.to_string()
            } else {
                two(u32::from(date.minute))
            }),
            's' => out.push_str(&if n == 1 {
                date.second.to_string()
            } else {
                two(u32::from(date.second))
            }),
            _ => out.extend(std::iter::repeat_n(ch, n)),
        }
        i += n;
    }
    out
}

fn general_number_format(key: &str) -> Option<NumberFormat> {
    Some(match key {
        "arabic" => NumberFormat::Decimal,
        "roman" => NumberFormat::LowerRoman,
        "alphabetic" => NumberFormat::LowerLetter,
        "ordinal" => NumberFormat::Ordinal,
        "cardtext" => NumberFormat::CardinalText,
        "ordtext" => NumberFormat::OrdinalText,
        "hex" => NumberFormat::Hex,
        "arabicdash" => NumberFormat::NumberInDash,
        _ => return None,
    })
}

/// Applies `\*` formats to a numeric result (`\* ROMAN` → `XIV`, `\* alphabetic` → `n`); the case
/// of the argument selects upper or lower case for roman and alphabetic, as in Word.
#[must_use]
pub fn apply_number_switches(value: i64, general: &[String]) -> String {
    let mut text = value.to_string();
    for g in general {
        let key = g.to_lowercase();
        if let Some(fmt) = general_number_format(&key) {
            text = format_number(value, &fmt);
            let upper = *g == g.to_uppercase();
            if (key == "roman" || key == "alphabetic") && upper {
                text = text.to_uppercase();
            }
            if (key == "cardtext" || key == "ordtext") && !upper {
                text = text.to_lowercase();
            }
        }
    }
    apply_text_switches(&text, general)
}

fn first_cap(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

/// Applies the case switches `Upper`, `Lower`, `FirstCap` and `Caps`; MERGEFORMAT and
/// CHARFORMAT do not change text.
#[must_use]
pub fn apply_text_switches(text: &str, general: &[String]) -> String {
    let mut out = text.to_owned();
    for g in general {
        match g.to_lowercase().as_str() {
            "upper" => out = out.to_uppercase(),
            "lower" => out = out.to_lowercase(),
            "firstcap" => out = first_cap(&out),
            "caps" => {
                let mut res = String::with_capacity(out.len());
                let mut start = true;
                for c in out.chars() {
                    if start && !c.is_whitespace() {
                        res.extend(c.to_uppercase());
                    } else {
                        res.push(c);
                    }
                    start = c.is_whitespace();
                }
                out = res;
            }
            _ => {}
        }
    }
    out
}

/// Looks up a bookmark's text (REF).
pub type BookmarkText<'a> = &'a dyn Fn(&str) -> Option<String>;
/// Looks up a bookmark's page (PAGEREF).
pub type BookmarkPage<'a> = &'a dyn Fn(&str) -> Option<i64>;

/// What [`evaluate_field`] may consult.
#[derive(Default)]
pub struct FieldContext<'a> {
    /// The page the field is on.
    pub page: Option<i64>,
    /// Pages in the document.
    pub num_pages: Option<i64>,
    /// Pages in the field's section.
    pub section_pages: Option<i64>,
    /// The section number.
    pub section: Option<i64>,
    /// The current time (DATE/TIME show it).
    pub now: Option<FieldDateTime>,
    /// The text of a bookmark (REF).
    pub bookmark_text: Option<BookmarkText<'a>>,
    /// The page of a bookmark (PAGEREF).
    pub bookmark_page: Option<BookmarkPage<'a>>,
}

impl std::fmt::Debug for FieldContext<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FieldContext")
            .field("page", &self.page)
            .field("num_pages", &self.num_pages)
            .field("section_pages", &self.section_pages)
            .field("section", &self.section)
            .field("now", &self.now)
            .finish_non_exhaustive()
    }
}

/// Computes the result of fields that can be evaluated without layout knowledge beyond the
/// context (PAGE, NUMPAGES, SECTIONPAGES, SECTION, DATE, TIME, REF, PAGEREF). `None` means the
/// cached result stays (TOC, SEQ and unknown fields are computed by layout or kept).
#[must_use]
pub fn evaluate_field(field: &FieldInstruction, ctx: &FieldContext<'_>) -> Option<String> {
    let g = &field.general;
    match &field.kind {
        FieldKind::Page(kind) => {
            let v = match kind {
                PageFieldKind::Page => ctx.page,
                PageFieldKind::NumPages => ctx.num_pages,
                PageFieldKind::SectionPages => ctx.section_pages,
                PageFieldKind::Section => ctx.section,
            }?;
            Some(apply_number_switches(v, g))
        }
        FieldKind::Date {
            kind: DateFieldKind::Date | DateFieldKind::Time,
            picture,
        } => Some(apply_text_switches(
            &format_field_date(picture, &ctx.now?),
            g,
        )),
        FieldKind::Ref(r) => {
            let text = (ctx.bookmark_text?)(&r.bookmark)?;
            Some(apply_text_switches(&text, g))
        }
        FieldKind::PageRef(r) => {
            let page = (ctx.bookmark_page?)(&r.bookmark)?;
            Some(apply_number_switches(page, g))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toc(f: &FieldInstruction) -> &TocField {
        match &f.kind {
            FieldKind::Toc(t) => t,
            other => panic!("not a TOC: {other:?}"),
        }
    }

    #[test]
    fn page_with_mergeformat() {
        let f = parse_field_instruction(" PAGE   \\* MERGEFORMAT ");
        assert_eq!(f.keyword, "PAGE");
        assert_eq!(f.general, vec!["MERGEFORMAT"]);
        assert!(!f.is_unknown());
        assert_eq!(f.kind, FieldKind::Page(PageFieldKind::Page));
    }

    #[test]
    fn numpages_and_lower_case_keywords() {
        assert_eq!(parse_field_instruction("numpages").keyword, "NUMPAGES");
        assert_eq!(
            parse_field_instruction("SECTIONPAGES").kind,
            FieldKind::Page(PageFieldKind::SectionPages)
        );
        assert_eq!(
            parse_field_instruction("SECTION").kind,
            FieldKind::Page(PageFieldKind::Section)
        );
    }

    #[test]
    fn date_with_picture_and_defaults() {
        let f = parse_field_instruction("DATE \\@ \"dddd, MMMM d, yyyy\" \\* MERGEFORMAT");
        assert_eq!(
            f.kind,
            FieldKind::Date {
                kind: DateFieldKind::Date,
                picture: "dddd, MMMM d, yyyy".into()
            }
        );
        assert_eq!(f.date_format.as_deref(), Some("dddd, MMMM d, yyyy"));
        let pic = |s: &str| match parse_field_instruction(s).kind {
            FieldKind::Date { picture, .. } => picture,
            _ => String::new(),
        };
        assert_eq!(pic("DATE"), "M/d/yyyy");
        assert_eq!(pic("TIME"), "h:mm am/pm");
        assert_eq!(pic("SAVEDATE"), "M/d/yyyy h:mm:ss am/pm");
        assert_eq!(pic("CREATEDATE"), "M/d/yyyy h:mm:ss am/pm");
        assert_eq!(pic("PRINTDATE"), "M/d/yyyy h:mm:ss am/pm");
    }

    #[test]
    fn toc_with_word_default_switches() {
        let f = parse_field_instruction(DEFAULT_TOC_INSTRUCTION);
        let t = toc(&f);
        assert_eq!(t.outline_levels, Some(LevelRange { from: 1, to: 3 }));
        assert!(t.hyperlinks && t.hide_in_web_layout && t.use_outline_levels);
        assert!(t.styles.is_empty());
    }

    #[test]
    fn toc_custom_styles_no_page_numbers_and_separator() {
        let f =
            parse_field_instruction("TOC \\t \"Title,1,Appendix Heading,2\" \\n \"2-3\" \\p \"-\"");
        let t = toc(&f);
        assert_eq!(
            t.styles,
            vec![("Title".to_owned(), 1), ("Appendix Heading".to_owned(), 2)]
        );
        assert_eq!(
            t.omit_page_numbers,
            Some(OmitPageNumbers::Levels(LevelRange { from: 2, to: 3 }))
        );
        assert_eq!(t.separator.as_deref(), Some("-"));
        assert_eq!(t.outline_levels, None);
    }

    #[test]
    fn toc_figures_and_n_without_range() {
        let f = parse_field_instruction("TOC \\h \\z \\c \"Figure\" \\n");
        let t = toc(&f);
        assert_eq!(t.caption_label.as_deref(), Some("Figure"));
        assert_eq!(t.omit_page_numbers, Some(OmitPageNumbers::All));
        let semi = parse_field_instruction("TOC \\t \"Heading 1;1;Heading 2;2\"");
        let levels: Vec<u32> = toc(&semi).styles.iter().map(|s| s.1).collect();
        assert_eq!(levels, vec![1, 2]);
        let all = parse_field_instruction("TOC \\o \\b sel");
        assert_eq!(
            toc(&all).outline_levels,
            Some(LevelRange { from: 1, to: 9 })
        );
    }

    #[test]
    fn ref_pageref_and_noteref() {
        let f = parse_field_instruction("REF _Ref123456 \\w \\h");
        let FieldKind::Ref(r) = &f.kind else {
            panic!("REF expected");
        };
        assert_eq!(r.bookmark, "_Ref123456");
        assert!(r.hyperlink);
        assert_eq!(
            r.paragraph_number,
            Some(ParagraphNumberContext::FullContext)
        );
        let f = parse_field_instruction("PAGEREF _Toc1 \\h \\p");
        let FieldKind::PageRef(r) = &f.kind else {
            panic!("PAGEREF expected");
        };
        assert_eq!(
            (r.bookmark.as_str(), r.hyperlink, r.relative_position),
            ("_Toc1", true, true)
        );
        assert!(matches!(
            parse_field_instruction("NOTEREF _Ref1 \\f").kind,
            FieldKind::NoteRef(_)
        ));
        let FieldKind::Ref(rel) = parse_field_instruction("REF a \\r").kind else {
            panic!("REF expected");
        };
        assert_eq!(rel.paragraph_number, Some(ParagraphNumberContext::Relative));
    }

    #[test]
    fn hyperlinks_external_and_internal() {
        let f = parse_field_instruction(
            "HYPERLINK \"https://example.com/a b\" \\o \"Open site\" \\t \"_blank\"",
        );
        let FieldKind::Hyperlink(h) = &f.kind else {
            panic!("HYPERLINK expected");
        };
        assert_eq!(h.url.as_deref(), Some("https://example.com/a b"));
        assert_eq!(h.tooltip.as_deref(), Some("Open site"));
        assert_eq!(h.target.as_deref(), Some("_blank"));
        assert!(!h.new_window);
        let f = parse_field_instruction("HYPERLINK \\l \"_Toc12345\" \\n");
        let FieldKind::Hyperlink(h) = &f.kind else {
            panic!("HYPERLINK expected");
        };
        assert_eq!(h.url, None);
        assert_eq!(h.anchor.as_deref(), Some("_Toc12345"));
        assert!(h.new_window);
    }

    #[test]
    fn seq_fields() {
        let f = parse_field_instruction("SEQ Figure \\* ARABIC \\r 3 \\s 1");
        let FieldKind::Seq(s) = &f.kind else {
            panic!("SEQ expected");
        };
        assert_eq!(s.identifier, "Figure");
        assert_eq!(f.general, vec!["ARABIC"]);
        assert_eq!(s.reset_to, Some(3));
        assert_eq!(s.reset_at_heading, Some(1));
        let FieldKind::Seq(s) = parse_field_instruction("SEQ Table \\c \\h").kind else {
            panic!("SEQ expected");
        };
        assert!(s.repeat && s.hidden);
    }

    #[test]
    fn unknown_fields_are_preserved_verbatim() {
        let raw = " MERGEFIELD  FirstName \\* Upper ";
        let f = parse_field_instruction(raw);
        assert!(f.is_unknown());
        assert_eq!(f.raw, raw);
        assert_eq!(f.keyword, "MERGEFIELD");
        assert_eq!(f.args, vec!["FirstName"]);
        assert_eq!(f.general, vec!["Upper"]);
        let n = parse_field_instruction("= SUM(ABOVE) \\# \"#,##0.00\"");
        assert_eq!(n.numeric_format.as_deref(), Some("#,##0.00"));
        assert!(n.is_unknown());
    }

    #[test]
    fn empty_and_garbage_input_never_fails() {
        assert_eq!(parse_field_instruction("").keyword, "");
        assert_eq!(parse_field_instruction("\"unterminated").keyword, "");
        let g = parse_field_instruction("\\\\ \\ \"");
        assert!(g.is_unknown());
        let _ = parse_field_instruction("TOC \\o \\t");
        let _ = parse_field_instruction("SEQ \\r x \\s");
    }

    #[test]
    fn tokenizer_escapes_and_smart_quotes() {
        let t = tokenize_field("INCLUDETEXT \"C:\\\\Docs\\\\a \\\"b\\\".docx\"");
        let texts: Vec<&str> = t.iter().map(|x| x.text.as_str()).collect();
        assert_eq!(texts, vec!["INCLUDETEXT", "C:\\Docs\\a \"b\".docx"]);
        let f = parse_field_instruction("TOC \\o \u{201C}1-2\u{201D}");
        assert_eq!(toc(&f).outline_levels, Some(LevelRange { from: 1, to: 2 }));
    }

    #[test]
    fn canonical_form_reparses_identically() {
        let instr = build_field(
            "hyperlink",
            &["https://x.org/a b"],
            &[("\\o", Some("Tip \"q\"")), ("\\n", None)],
        );
        assert_eq!(
            instr,
            "HYPERLINK \"https://x.org/a b\" \\o \"Tip \\\"q\\\"\" \\n"
        );
        let f = parse_field_instruction(&instr);
        let FieldKind::Hyperlink(h) = &f.kind else {
            panic!("HYPERLINK expected");
        };
        assert_eq!(h.url.as_deref(), Some("https://x.org/a b"));
        assert_eq!(h.tooltip.as_deref(), Some("Tip \"q\""));
        assert!(h.new_window);
        assert_eq!(
            serialize_field_instruction(&f.keyword, &f.args, &f.switches),
            instr
        );
    }

    const DATE: FieldDateTime = FieldDateTime {
        year: 2026,
        month: 10,
        day: 5,
        hour: 14,
        minute: 7,
        second: 9,
    };

    #[test]
    fn date_pictures() {
        for (pic, out) in [
            ("M/d/yyyy", "10/5/2026"),
            ("dddd, MMMM d, yyyy", "Monday, October 5, 2026"),
            ("dd.MM.yy", "05.10.26"),
            ("ddd d MMM", "Mon 5 Oct"),
            ("h:mm am/pm", "2:07 pm"),
            ("HH:mm:ss", "14:07:09"),
            ("'Week of' MMMM", "Week of October"),
            ("hh:mm AM/PM", "02:07 PM"),
            ("'open", "open"),
        ] {
            assert_eq!(format_field_date(pic, &DATE), out, "{pic}");
        }
        let midnight = FieldDateTime { hour: 0, ..DATE };
        assert_eq!(format_field_date("h AM/PM", &midnight), "12 AM");
        assert_eq!(DATE.weekday(), 1);
    }

    #[test]
    fn number_switches() {
        let g = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(apply_number_switches(14, &g(&["ROMAN"])), "XIV");
        assert_eq!(apply_number_switches(14, &g(&["roman"])), "xiv");
        assert_eq!(apply_number_switches(3, &g(&["ALPHABETIC"])), "C");
        assert_eq!(apply_number_switches(3, &g(&["Ordinal"])), "3rd");
        assert_eq!(apply_number_switches(21, &g(&["CardText"])), "twenty-one");
        assert_eq!(
            apply_number_switches(21, &g(&["CardText", "Caps"])),
            "Twenty-one"
        );
        assert_eq!(apply_number_switches(7, &g(&["MERGEFORMAT"])), "7");
        assert_eq!(
            apply_text_switches("hello world", &g(&["FirstCap"])),
            "Hello world"
        );
        assert_eq!(apply_text_switches("HeLLo", &g(&["Lower"])), "hello");
    }

    #[test]
    fn evaluates_page_and_date_fields() {
        let ctx = FieldContext {
            page: Some(4),
            num_pages: Some(12),
            now: Some(DATE),
            ..FieldContext::default()
        };
        let eval = |s: &str| evaluate_field(&parse_field_instruction(s), &ctx);
        assert_eq!(eval("PAGE \\* roman").as_deref(), Some("iv"));
        assert_eq!(eval("NUMPAGES").as_deref(), Some("12"));
        assert_eq!(eval("SECTIONPAGES"), None);
        assert_eq!(
            eval("DATE \\@ \"yyyy\" \\* MERGEFORMAT").as_deref(),
            Some("2026")
        );
        assert_eq!(
            evaluate_field(&parse_field_instruction("PAGE"), &FieldContext::default()),
            None
        );
        assert!(format!("{ctx:?}").contains("FieldContext"));
    }

    #[test]
    fn evaluates_ref_through_callbacks_and_leaves_toc_and_seq() {
        let text = |n: &str| (n == "bm").then(|| "Introduction".to_owned());
        let page = |_: &str| Some(3);
        let ctx = FieldContext {
            bookmark_text: Some(&text),
            bookmark_page: Some(&page),
            ..FieldContext::default()
        };
        let eval = |s: &str| evaluate_field(&parse_field_instruction(s), &ctx);
        assert_eq!(
            eval("REF bm \\h \\* Upper").as_deref(),
            Some("INTRODUCTION")
        );
        assert_eq!(eval("REF missing"), None);
        assert_eq!(eval("PAGEREF bm \\h").as_deref(), Some("3"));
        assert_eq!(eval("TOC \\o \"1-3\""), None);
        assert_eq!(eval("SEQ Figure"), None);
    }

    #[test]
    fn field_objects_parse_their_instruction() {
        let f = Field::new("PAGE", "1");
        assert_eq!(f.parsed().kind, FieldKind::Page(PageFieldKind::Page));
        assert_eq!(f.form, FieldForm::Complex);
    }
}
