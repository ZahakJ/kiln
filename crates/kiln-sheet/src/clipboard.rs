// SPDX-License-Identifier: AGPL-3.0-or-later
//! The clipboard model: what Copy takes from a range (values, formulas, formats, displayed text,
//! merges), its plain-text (TSV) and HTML forms for other applications, and the mapping of
//! TSV and HTML from other applications back into cells (typed-entry recognition, colspans,
//! basic formatting). Paste within the workbook uses [`Command::CopyRange`] /
//! [`Command::MoveRange`] so formulas adjust; external data becomes a [`Command::SetCells`].

use kiln_formula::SheetId;
use serde::{Deserialize, Serialize};

use crate::address::{CellPos, Range};
use crate::color::Color;
use crate::commands::{CellInput, Command, Content, PasteOp, PasteWhat};
use crate::data::parse_input;
use crate::error::{ModelError, Result};
use crate::styles::{CellStyle, Fill, HAlign, StylePatch};
use crate::value::CellValue;
use crate::workbook::Workbook;

/// One copied cell.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClipCell {
    /// Value (cached result for formulas).
    pub value: CellValue,
    /// Formula without `=`.
    pub formula: Option<String>,
    /// The displayed text.
    pub text: String,
    /// Format (copied as a whole, so it can be pasted into another workbook).
    pub style: Option<CellStyle>,
}

/// Where copied cells came from (for paste within the workbook and for Cut).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipSource {
    /// Sheet.
    #[serde(with = "crate::meta::sheet_id")]
    pub sheet: SheetId,
    /// Range.
    pub range: Range,
    /// Cut (move on paste) rather than copy.
    pub cut: bool,
}

/// One copied cell and its position in the block.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClipEntry {
    /// Row in the block.
    pub row: u32,
    /// Column in the block.
    pub col: u32,
    /// The cell.
    pub cell: ClipCell,
}

/// Copied cells (sparse: empty cells are not stored).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ClipData {
    /// Rows.
    pub rows: u32,
    /// Columns.
    pub cols: u32,
    /// Non-empty cells, row-major.
    pub cells: Vec<ClipEntry>,
    /// Merged regions, relative to the top left.
    pub merges: Vec<Range>,
    /// The source range when copied from this workbook.
    pub source: Option<ClipSource>,
}

impl ClipData {
    /// The cell at (row, col) of the block.
    pub fn get(&self, row: u32, col: u32) -> Option<&ClipCell> {
        if row >= self.rows || col >= self.cols {
            return None;
        }
        self.cells
            .binary_search_by(|e| (e.row, e.col).cmp(&(row, col)))
            .ok()
            .map(|i| &self.cells[i].cell)
    }

    /// Tab-separated text (what other applications receive as plain text). Fields with tabs,
    /// line breaks or quotes are quoted.
    pub fn to_tsv(&self) -> String {
        let mut out = String::new();
        for r in 0..self.rows {
            for c in 0..self.cols {
                if c > 0 {
                    out.push('\t');
                }
                let t = self.get(r, c).map_or("", |x| x.text.as_str());
                if t.contains(['\t', '\n', '\r', '"']) {
                    out.push('"');
                    out.push_str(&t.replace('"', "\"\""));
                    out.push('"');
                } else {
                    out.push_str(t);
                }
            }
            out.push_str("\r\n");
        }
        out
    }

    /// An HTML table with inline styles (what word processors and browsers receive).
    pub fn to_html(&self, theme: &crate::Theme) -> String {
        let mut out = String::from("<table style=\"border-collapse:collapse\">\n");
        let covered = |r: u32, c: u32| {
            self.merges
                .iter()
                .any(|m| m.contains(r, c) && !(m.r0 == r && m.c0 == c))
        };
        for r in 0..self.rows {
            out.push_str("<tr>");
            for c in 0..self.cols {
                if covered(r, c) {
                    continue;
                }
                out.push_str("<td");
                if let Some(m) = self.merges.iter().find(|m| m.r0 == r && m.c0 == c) {
                    if m.height() > 1 {
                        out.push_str(&format!(" rowspan=\"{}\"", m.height()));
                    }
                    if m.width() > 1 {
                        out.push_str(&format!(" colspan=\"{}\"", m.width()));
                    }
                }
                let cell = self.get(r, c);
                if let Some(st) = cell.and_then(|x| x.style.as_ref()) {
                    let css = css_of(st, cell.map(|x| &x.value), theme);
                    if !css.is_empty() {
                        out.push_str(&format!(" style=\"{css}\""));
                    }
                }
                out.push('>');
                if let Some(x) = cell {
                    out.push_str(&escape_html(&x.text).replace('\n', "<br>"));
                }
                out.push_str("</td>");
            }
            out.push_str("</tr>\n");
        }
        out.push_str("</table>");
        out
    }

    /// Cells from tab-separated (or comma-separated) text, with typed-entry recognition.
    pub fn from_tsv(text: &str, date1904: bool) -> ClipData {
        let delim = if text.contains('\t') { '\t' } else { ',' };
        let rows = parse_delimited(text, delim);
        ClipData::from_rows(
            rows.into_iter()
                .map(|r| r.into_iter().map(|t| (t, None)).collect())
                .collect(),
            Vec::new(),
            date1904,
        )
    }

    /// Cells from an HTML table (the first `<table>`, or the body's lines when there is none).
    pub fn from_html(html: &str, date1904: bool) -> ClipData {
        let (rows, merges) = parse_html_table(html);
        ClipData::from_rows(rows, merges, date1904)
    }

    fn from_rows(
        rows: Vec<Vec<(String, Option<HtmlStyle>)>>,
        merges: Vec<Range>,
        date1904: bool,
    ) -> ClipData {
        let n_rows = rows.len() as u32;
        let n_cols = rows.iter().map(Vec::len).max().unwrap_or(0) as u32;
        let mut cells = Vec::new();
        for (r, line) in rows.into_iter().enumerate() {
            for (c, (text, html)) in line.into_iter().enumerate() {
                if text.is_empty() && html.is_none() {
                    continue;
                }
                let (value, formula) =
                    if let Some(f) = text.strip_prefix('=').filter(|f| !f.is_empty()) {
                        (CellValue::Blank, Some(f.to_owned()))
                    } else {
                        let p = parse_input(&text, date1904);
                        (p.value, None)
                    };
                let parsed = parse_input(&text, date1904);
                let style = if html.is_some() || parsed.num_fmt.is_some() {
                    let mut st = CellStyle {
                        num_fmt: std::sync::Arc::from(
                            parsed.num_fmt.as_deref().unwrap_or("General"),
                        ),
                        ..CellStyle::default()
                    };
                    if let Some(h) = &html {
                        h.apply(&mut st);
                    }
                    Some(st)
                } else {
                    None
                };
                cells.push(ClipEntry {
                    row: r as u32,
                    col: c as u32,
                    cell: ClipCell {
                        value,
                        formula,
                        text,
                        style,
                    },
                });
            }
        }
        ClipData {
            rows: n_rows,
            cols: n_cols,
            cells,
            merges,
            source: None,
        }
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn css_of(st: &CellStyle, value: Option<&CellValue>, theme: &crate::Theme) -> String {
    let mut css = Vec::new();
    if st.font.bold {
        css.push("font-weight:bold".to_owned());
    }
    if st.font.italic {
        css.push("font-style:italic".to_owned());
    }
    if st.font.underline != crate::styles::Underline::None {
        css.push("text-decoration:underline".to_owned());
    }
    if let Some(c) = st.font.color {
        let rgb = c.resolve(theme, 0);
        if rgb != 0 {
            css.push(format!("color:#{rgb:06X}"));
        }
    }
    if let Some(c) = st.fill.main_color() {
        css.push(format!("background:#{:06X}", c.resolve(theme, 0xFFFFFF)));
    }
    let align = match st.alignment.horizontal {
        HAlign::Left => Some("left"),
        HAlign::Center | HAlign::CenterContinuous => Some("center"),
        HAlign::Right => Some("right"),
        HAlign::Justify | HAlign::Distributed => Some("justify"),
        HAlign::General if matches!(value, Some(CellValue::Number(_))) => Some("right"),
        _ => None,
    };
    if let Some(a) = align {
        css.push(format!("text-align:{a}"));
    }
    if &*st.num_fmt != "General" {
        css.push(format!(
            "mso-number-format:'{}'",
            st.num_fmt.replace('\'', "\\'")
        ));
    }
    css.join(";")
}

/// Splits delimited text into rows of fields (quotes, doubled quotes and quoted line breaks).
pub fn parse_delimited(text: &str, delim: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut at_start = true;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                cur.push(c);
            }
            continue;
        }
        match c {
            '"' if at_start => {
                quoted = true;
                at_start = false;
            }
            c if c == delim => {
                row.push(std::mem::take(&mut cur));
                at_start = true;
            }
            '\r' => {}
            '\n' => {
                row.push(std::mem::take(&mut cur));
                rows.push(std::mem::take(&mut row));
                at_start = true;
            }
            other => {
                cur.push(other);
                at_start = false;
            }
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

/// Formatting read from an HTML cell's inline style and tags.
#[derive(Debug, Clone, Default, PartialEq)]
struct HtmlStyle {
    bold: bool,
    italic: bool,
    underline: bool,
    color: Option<u32>,
    background: Option<u32>,
    align: Option<HAlign>,
}

impl HtmlStyle {
    fn apply(&self, st: &mut CellStyle) {
        st.font.bold |= self.bold;
        st.font.italic |= self.italic;
        if self.underline {
            st.font.underline = crate::styles::Underline::Single;
        }
        if let Some(c) = self.color {
            st.font.color = Some(Color::rgb(c));
        }
        if let Some(b) = self.background {
            st.fill = Fill::solid(Color::rgb(b));
        }
        if let Some(a) = self.align {
            st.alignment.horizontal = a;
        }
    }

    fn is_plain(&self) -> bool {
        *self == HtmlStyle::default()
    }
}

fn css_color(v: &str) -> Option<u32> {
    let v = v.trim().to_ascii_lowercase();
    if let Some(h) = v.strip_prefix('#') {
        return match h.len() {
            6 => u32::from_str_radix(h, 16).ok(),
            3 => {
                let x = u32::from_str_radix(h, 16).ok()?;
                let (r, g, b) = ((x >> 8) & 0xF, (x >> 4) & 0xF, x & 0xF);
                Some((r * 17) << 16 | (g * 17) << 8 | (b * 17))
            }
            _ => None,
        };
    }
    if let Some(inner) = v.strip_prefix("rgb(").and_then(|s| s.strip_suffix(')')) {
        let p: Vec<u32> = inner
            .split(',')
            .map(|x| x.trim().parse::<u32>().ok().map(|n| n.min(255)))
            .collect::<Option<_>>()?;
        if p.len() == 3 {
            return Some(p[0] << 16 | p[1] << 8 | p[2]);
        }
        return None;
    }
    Some(match v.as_str() {
        "black" => 0x000000,
        "white" => 0xFFFFFF,
        "red" => 0xFF0000,
        "green" => 0x008000,
        "blue" => 0x0000FF,
        "yellow" => 0xFFFF00,
        "gray" | "grey" => 0x808080,
        "orange" => 0xFFA500,
        "purple" => 0x800080,
        _ => return None,
    })
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let end = rest.find(';').filter(|&e| e <= 10);
        let Some(end) = end else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let ent = &rest[1..end];
        let ch = match ent {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            e if e.starts_with("#x") || e.starts_with("#X") => u32::from_str_radix(&e[2..], 16)
                .ok()
                .and_then(char::from_u32),
            e if e.starts_with('#') => e[1..].parse::<u32>().ok().and_then(char::from_u32),
            _ => None,
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let at = from + i;
        from = at + name.len();
        let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
        let after = lower[at + name.len()..].trim_start();
        if !before_ok || !after.starts_with('=') {
            continue;
        }
        let off = tag.len() - after.len() + 1;
        let v = tag[off..].trim_start();
        return Some(match v.chars().next() {
            Some(q @ ('"' | '\'')) => v[1..].split(q).next().unwrap_or("").to_owned(),
            _ => v
                .split(|c: char| c.is_whitespace() || c == '>')
                .next()
                .unwrap_or("")
                .to_owned(),
        });
    }
    None
}

fn style_of_tag(tag: &str, st: &mut HtmlStyle) {
    if let Some(css) = attr(tag, "style") {
        for decl in css.split(';') {
            let Some((k, v)) = decl.split_once(':') else {
                continue;
            };
            let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_ascii_lowercase());
            match k.as_str() {
                "font-weight" => st.bold = v == "bold" || v.parse::<u32>().is_ok_and(|n| n >= 600),
                "font-style" => st.italic = v == "italic",
                "text-decoration" | "text-decoration-line" => {
                    st.underline = v.contains("underline")
                }
                "color" => st.color = css_color(&v),
                "background" | "background-color" => st.background = css_color(&v),
                "text-align" => {
                    st.align = match v.as_str() {
                        "left" => Some(HAlign::Left),
                        "center" => Some(HAlign::Center),
                        "right" => Some(HAlign::Right),
                        "justify" => Some(HAlign::Justify),
                        _ => None,
                    }
                }
                _ => {}
            }
        }
    }
    if let Some(a) = attr(tag, "align") {
        st.align = match a.to_ascii_lowercase().as_str() {
            "center" => Some(HAlign::Center),
            "right" => Some(HAlign::Right),
            "left" => Some(HAlign::Left),
            _ => st.align,
        };
    }
    if let Some(b) = attr(tag, "bgcolor") {
        st.background = css_color(&b);
    }
}

type HtmlRows = Vec<Vec<(String, Option<HtmlStyle>)>>;

/// Parses the first table of an HTML fragment into rows (rowspan/colspan become merges).
fn parse_html_table(html: &str) -> (HtmlRows, Vec<Range>) {
    let lower = html.to_ascii_lowercase();
    let Some(start) = lower.find("<table") else {
        // No table: one cell per line of text.
        let text = strip_tags(html);
        let rows = text.lines().map(|l| vec![(l.to_owned(), None)]).collect();
        return (rows, Vec::new());
    };
    let end = lower[start..]
        .find("</table")
        .map_or(html.len(), |e| start + e);
    let body = &html[start..end];
    let lbody = &lower[start..end];
    let mut rows: HtmlRows = Vec::new();
    let mut merges = Vec::new();
    // Grid occupancy from rowspans: (row, col) taken.
    let mut taken: Vec<(u32, u32)> = Vec::new();
    let mut i = 0;
    let mut row_idx: i64 = -1;
    let mut col_idx: u32 = 0;
    while let Some(off) = lbody[i..].find('<') {
        let at = i + off;
        let close = lbody[at..].find('>').map_or(lbody.len(), |c| at + c);
        let tag = &body[at..close.min(body.len())];
        let ltag = &lbody[at..close.min(lbody.len())];
        i = close + 1;
        if ltag.starts_with("<tr") {
            row_idx += 1;
            col_idx = 0;
            rows.push(Vec::new());
            continue;
        }
        if ltag.starts_with("<td") || ltag.starts_with("<th") {
            if row_idx < 0 {
                row_idx = 0;
                rows.push(Vec::new());
            }
            let r = row_idx as u32;
            while taken.contains(&(r, col_idx)) {
                col_idx += 1;
            }
            let content_end = ["</td", "</th", "<td", "<th", "</tr", "<tr"]
                .iter()
                .filter_map(|t| lbody[i.min(lbody.len())..].find(t).map(|x| i + x))
                .min()
                .unwrap_or(lbody.len());
            let inner = &body[i.min(body.len())..content_end];
            let mut st = HtmlStyle::default();
            if ltag.starts_with("<th") {
                st.bold = true;
            }
            style_of_tag(tag, &mut st);
            let li = inner.to_ascii_lowercase();
            if li.contains("<b>") || li.contains("<strong") {
                st.bold = true;
            }
            if li.contains("<i>") || li.contains("<em") {
                st.italic = true;
            }
            if li.contains("<u>") {
                st.underline = true;
            }
            let text = strip_tags(inner).trim().to_owned();
            let rs = attr(tag, "rowspan")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, 1000);
            let cs = attr(tag, "colspan")
                .and_then(|v| v.parse::<u32>().ok())
                .unwrap_or(1)
                .clamp(1, 1000);
            let line = &mut rows[r as usize];
            while line.len() < col_idx as usize {
                line.push((String::new(), None));
            }
            line.push((text, (!st.is_plain()).then_some(st)));
            if rs > 1 || cs > 1 {
                merges.push(Range::new(r, col_idx, r + rs - 1, col_idx + cs - 1));
                for dr in 0..rs {
                    for dc in 0..cs {
                        if dr > 0 || dc > 0 {
                            taken.push((r + dr, col_idx + dc));
                        }
                    }
                }
            }
            col_idx += cs;
            for _ in 1..cs {
                rows[r as usize].push((String::new(), None));
            }
            i = content_end;
        }
    }
    (rows, merges)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    let lower = s.to_ascii_lowercase();
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if !in_tag && lower[i..].starts_with("<br") {
            out.push('\n');
        }
        let c = s[i..].chars().next().unwrap_or(' ');
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
        i += c.len_utf8();
    }
    decode_entities(&out)
}

impl Workbook {
    /// Copies a range.
    pub fn copy(&self, sheet: SheetId, range: Range, cut: bool) -> Result<ClipData> {
        range.check()?;
        let s = self
            .sheet(sheet)
            .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", sheet.0)))?;
        let (rows, cols) = (range.height(), range.width());
        // Small ranges are scanned whole (values spilled into them are copied too); large ones
        // copy their stored cells.
        let positions: Vec<(u32, u32)> = if range.cell_count() <= 1_000_000 {
            range.cells().map(|p| (p.row, p.col)).collect()
        } else {
            s.cells.iter_range(range).map(|(r, c, _)| (r, c)).collect()
        };
        let mut cells = Vec::new();
        for (r, c) in positions {
            let value = self.value(sheet, r, c);
            let formula = self.formula(sheet, r, c);
            let style = s.style_at(r, c);
            if value.is_blank() && formula.is_none() && style == 0 {
                continue;
            }
            cells.push(ClipEntry {
                row: r - range.r0,
                col: c - range.c0,
                cell: ClipCell {
                    text: self.display_text(sheet, r, c),
                    value,
                    formula,
                    style: Some(self.styles().get(style)),
                },
            });
        }
        let merges = s
            .meta
            .merges
            .iter()
            .filter(|m| m.is_inside(&range))
            .filter_map(|m| m.offset(-i64::from(range.r0), -i64::from(range.c0)))
            .collect();
        Ok(ClipData {
            rows,
            cols,
            cells,
            merges,
            source: Some(ClipSource { sheet, range, cut }),
        })
    }

    /// The command that pastes `data` at `at` (the top-left cell or a target selection).
    /// Data copied from this workbook pastes through Copy/Move so formulas adjust; anything
    /// else becomes cell edits (styles are added to the style table).
    #[allow(clippy::too_many_arguments)]
    pub fn paste_command(
        &mut self,
        sheet: SheetId,
        at: Range,
        data: &ClipData,
        what: PasteWhat,
        op: PasteOp,
        transpose: bool,
        skip_blanks: bool,
    ) -> Result<Command> {
        at.check()?;
        if let Some(src) = data.source
            && self.sheet(src.sheet).is_some()
        {
            if src.cut {
                return Ok(Command::MoveRange {
                    sheet: src.sheet,
                    range: src.range,
                    to_sheet: Some(sheet),
                    to: CellPos::new(at.r0, at.c0),
                });
            }
            let to = if at.is_single_cell() {
                let (h, w) = if transpose {
                    (src.range.width(), src.range.height())
                } else {
                    (src.range.height(), src.range.width())
                };
                Range {
                    r0: at.r0,
                    c0: at.c0,
                    r1: at.r0.saturating_add(h - 1),
                    c1: at.c0.saturating_add(w - 1),
                }
            } else {
                at
            };
            return Ok(Command::CopyRange {
                sheet: src.sheet,
                range: src.range,
                to_sheet: Some(sheet),
                to,
                what,
                op,
                transpose,
                skip_blanks,
            });
        }
        let mut cells = Vec::new();
        for r in 0..data.rows {
            for c in 0..data.cols {
                let (dr, dc) = if transpose { (c, r) } else { (r, c) };
                let (row, col) = (at.r0 + dr, at.c0 + dc);
                crate::address::check_cell(row, col)?;
                let cell = data.get(r, c);
                if skip_blanks && cell.is_none_or(|x| x.value.is_blank() && x.formula.is_none()) {
                    continue;
                }
                let content = match (what, cell) {
                    (PasteWhat::Formats | PasteWhat::Comments, _) => Content::Keep,
                    (_, None) => Content::Clear,
                    (PasteWhat::Values | PasteWhat::ValuesAndNumberFormats, Some(x)) => {
                        Content::Value(x.value.clone())
                    }
                    (_, Some(x)) => match &x.formula {
                        Some(f) if Workbook::check_formula(f).is_ok() => {
                            Content::Formula(f.clone())
                        }
                        _ => Content::Value(x.value.clone()),
                    },
                };
                let style = match (what, cell.and_then(|x| x.style.as_ref())) {
                    (
                        PasteWhat::All | PasteWhat::Formats | PasteWhat::AllExceptBorders,
                        Some(st),
                    ) => Some(self.styles_mut().intern(st)),
                    (
                        PasteWhat::ValuesAndNumberFormats | PasteWhat::FormulasAndNumberFormats,
                        Some(st),
                    ) => {
                        let base = self.sheet(sheet).map_or(0, |s| s.style_at(row, col));
                        Some(self.styles_mut().patch(
                            base,
                            &StylePatch {
                                num_fmt: Some(st.num_fmt.to_string()),
                                ..StylePatch::default()
                            },
                        ))
                    }
                    _ => None,
                };
                cells.push(CellInput {
                    row,
                    col,
                    content,
                    style,
                });
            }
        }
        let mut commands = vec![Command::SetCells { sheet, cells }];
        if matches!(what, PasteWhat::All | PasteWhat::Formats) {
            for m in &data.merges {
                let m = if transpose {
                    Range::new(m.c0, m.r0, m.c1, m.r1)
                } else {
                    *m
                };
                if let Some(target) = m.offset(i64::from(at.r0), i64::from(at.c0)) {
                    commands.push(Command::Merge {
                        sheet,
                        range: target,
                        mode: crate::commands::MergeMode::Merge,
                    });
                }
            }
        }
        Ok(if commands.len() == 1 {
            commands.pop().unwrap_or_else(Command::noop)
        } else {
            Command::Batch {
                commands,
                label: Some("Paste".into()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_delimited_text_with_quotes() {
        let rows = parse_delimited("a\t\"b\tc\"\r\n\"say \"\"hi\"\"\"\t2\n", '\t');
        assert_eq!(rows, vec![vec!["a", "b\tc"], vec!["say \"hi\"", "2"]]);
        assert_eq!(parse_delimited("x,y", ','), vec![vec!["x", "y"]]);
    }

    #[test]
    fn maps_tsv_to_typed_cells() {
        let d = ClipData::from_tsv("Name\tQty\nApples\t12\nPears\t1,200\n", false);
        assert_eq!((d.rows, d.cols), (3, 2));
        assert_eq!(
            d.get(1, 1).map(|c| c.value.clone()),
            Some(CellValue::Number(12.0))
        );
        let thousands = d
            .get(2, 1)
            .and_then(|c| c.style.clone())
            .map(|s| s.num_fmt.to_string());
        assert_eq!(thousands.as_deref(), Some("#,##0"));
        assert_eq!(
            d.get(0, 0).map(|c| c.value.clone()),
            Some(CellValue::text("Name"))
        );
    }

    #[test]
    fn maps_html_tables_with_spans_and_styles() {
        let html = r#"<html><body><table>
            <tr><th>Region</th><td colspan="2" style="color:#FF0000;text-align:center">Sales &amp; costs</td></tr>
            <tr><td>North</td><td>10</td><td bgcolor="yellow"><b>20</b></td></tr>
        </table></body></html>"#;
        let d = ClipData::from_html(html, false);
        assert_eq!((d.rows, d.cols), (2, 3));
        assert_eq!(d.merges, vec![Range::new(0, 1, 0, 2)]);
        let header = d
            .get(0, 0)
            .and_then(|c| c.style.clone())
            .map(|s| s.font.bold);
        assert_eq!(header, Some(true));
        let red = d.get(0, 1).cloned().unwrap_or_default();
        assert_eq!(red.text, "Sales & costs");
        assert_eq!(
            red.style.and_then(|s| s.font.color),
            Some(Color::rgb(0xFF0000))
        );
        let twenty = d.get(1, 2).cloned().unwrap_or_default();
        assert_eq!(twenty.value, CellValue::Number(20.0));
        let st = twenty.style.unwrap_or_default();
        assert!(st.font.bold);
        assert_eq!(st.fill, Fill::solid(Color::rgb(0xFFFF00)));
    }

    #[test]
    fn writes_tsv_and_html() {
        let d = ClipData::from_tsv("a\tb\tc\n1\t\"x\ty\"\t\n", false);
        assert_eq!(d.to_tsv(), "a\tb\tc\r\n1\t\"x\ty\"\t\r\n");
        let html = d.to_html(&crate::Theme::default());
        assert!(html.starts_with("<table"));
        assert!(html.contains("<td>a</td>"));
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(
            decode_entities("a&lt;b&gt;&#65;&#x42;&nbsp;&bogus;"),
            "a<b>AB &bogus;"
        );
    }
}
