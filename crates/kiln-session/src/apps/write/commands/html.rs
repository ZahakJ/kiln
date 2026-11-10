// SPDX-License-Identifier: AGPL-3.0-or-later
//! HTML from the clipboard (browsers, mail, other office apps), sanitised and mapped to the
//! Write model's terms: paragraphs with heading, quote and list structure, runs with bold,
//! italic, underline, strikethrough, sub/superscript, colour, highlight, size and font, links
//! (web, mail and in-document only) and tables.
//!
//! Nothing executable survives: scripts, styles, frames, forms, embedded objects and event
//! attributes are dropped with their content; links keep only safe schemes. The parser is a
//! small tolerant tokenizer (no DOM): unknown tags are transparent, unclosed tags close at the
//! end, and input size and nesting are capped.

use kiln_doc::RunProps;
use kiln_doc::st::{Alignment, HighlightColor, UnderlineStyle, VertAlign};

use super::format::{nearest_highlight, parse_rgb};

/// Larger HTML is cut (the text flavour is pasted instead by the caller).
pub(super) const MAX_HTML: usize = 8 * 1024 * 1024;
const MAX_DEPTH: usize = 256;

/// A piece of inline content.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum HtmlInline {
    /// Text with formatting and an optional link target.
    Text {
        /// The text.
        text: String,
        /// Its formatting.
        props: Box<RunProps>,
        /// A hyperlink target.
        link: Option<String>,
    },
    /// A line break (`<br>`).
    Break,
}

/// A paragraph.
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct HtmlPara {
    /// A paragraph style (`Heading1`, `Quote`, …).
    pub style: Option<String>,
    /// Alignment.
    pub align: Option<Alignment>,
    /// Content.
    pub inlines: Vec<HtmlInline>,
    /// List membership: ordered, level 0–8.
    pub list: Option<(bool, u8)>,
}

impl HtmlPara {
    fn is_blank(&self) -> bool {
        self.inlines.iter().all(|i| match i {
            HtmlInline::Text { text, .. } => text.trim().is_empty(),
            HtmlInline::Break => false,
        })
    }
}

/// A block.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum HtmlBlock {
    /// A paragraph.
    Para(HtmlPara),
    /// A table: rows of cells of paragraphs.
    Table(Vec<Vec<Vec<HtmlPara>>>),
}

/// Elements whose content is dropped.
const DROP: [&str; 17] = [
    "script", "style", "head", "title", "noscript", "template", "iframe", "object", "embed", "svg",
    "math", "canvas", "video", "audio", "select", "textarea", "button",
];
const BLOCKS: [&str; 22] = [
    "p",
    "div",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "li",
    "blockquote",
    "pre",
    "section",
    "article",
    "header",
    "footer",
    "address",
    "dd",
    "dt",
    "figcaption",
    "aside",
    "main",
    "nav",
];
const VOID: [&str; 10] = [
    "br", "img", "hr", "meta", "link", "input", "col", "area", "base", "wbr",
];

#[derive(Debug, Clone)]
struct Frame {
    tag: String,
    props: RunProps,
    link: Option<String>,
    align: Option<Alignment>,
    style: Option<String>,
    pre: bool,
}

struct Builder {
    blocks: Vec<HtmlBlock>,
    para: HtmlPara,
    stack: Vec<Frame>,
    lists: Vec<bool>,
    /// Open tables: rows of cells of paragraphs.
    tables: Vec<Vec<Vec<Vec<HtmlPara>>>>,
    in_cell: bool,
    drop_depth: usize,
    drop_tag: Option<String>,
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail[1..]
            .find(|c: char| c == ';' || c == '&' || c.is_whitespace())
            .map(|e| e + 1);
        let Some(end) = end.filter(|e| *e <= 12 && tail[*e..].starts_with(';')) else {
            out.push('&');
            rest = &tail[1..];
            continue;
        };
        let name = &tail[1..end];
        let decoded = if let Some(num) = name.strip_prefix('#') {
            let code = match num.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => num.parse::<u32>().ok(),
            };
            code.and_then(char::from_u32)
                .filter(|c| *c != '\0')
                .map(String::from)
        } else {
            Some(
                match name {
                    "amp" => "&",
                    "lt" => "<",
                    "gt" => ">",
                    "quot" => "\"",
                    "apos" => "'",
                    "nbsp" => "\u{00A0}",
                    "mdash" => "—",
                    "ndash" => "–",
                    "hellip" => "…",
                    "copy" => "©",
                    "reg" => "®",
                    "trade" => "™",
                    "lsquo" => "‘",
                    "rsquo" => "’",
                    "ldquo" => "“",
                    "rdquo" => "”",
                    "bull" => "•",
                    "middot" => "·",
                    "euro" => "€",
                    "pound" => "£",
                    "yen" => "¥",
                    "deg" => "°",
                    "times" => "×",
                    "divide" => "÷",
                    "laquo" => "«",
                    "raquo" => "»",
                    "shy" => "\u{00AD}",
                    "zwj" => "\u{200D}",
                    "zwnj" => "\u{200C}",
                    _ => {
                        out.push('&');
                        rest = &tail[1..];
                        continue;
                    }
                }
                .to_owned(),
            )
        };
        match decoded {
            Some(d) => out.push_str(&d),
            None => out.push('\u{FFFD}'),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Attributes of a tag (names lower-cased, values decoded).
fn attributes(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = src.as_bytes();
    let mut i = 0;
    while i < b.len() {
        while i < b.len() && (b[i].is_ascii_whitespace() || b[i] == b'/') {
            i += 1;
        }
        let start = i;
        while i < b.len() && !b[i].is_ascii_whitespace() && b[i] != b'=' && b[i] != b'/' {
            i += 1;
        }
        if start == i {
            i += 1;
            continue;
        }
        let name = src[start..i].to_ascii_lowercase();
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let mut value = String::new();
        if i < b.len() && b[i] == b'=' {
            i += 1;
            while i < b.len() && b[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < b.len() && (b[i] == b'"' || b[i] == b'\'') {
                let q = b[i];
                i += 1;
                let vs = i;
                while i < b.len() && b[i] != q {
                    i += 1;
                }
                value = decode_entities(&src[vs..i.min(b.len())]);
                i += 1;
            } else {
                let vs = i;
                while i < b.len() && !b[i].is_ascii_whitespace() {
                    i += 1;
                }
                value = decode_entities(&src[vs..i]);
            }
        }
        out.push((name, value));
    }
    out
}

/// CSS colour (`#rgb`, `#rrggbb`, `rgb(r, g, b)` and a few names) to RGB.
fn css_color(v: &str) -> Option<kiln_common::Color> {
    let v = v.trim().to_ascii_lowercase();
    if let Some(h) = v.strip_prefix('#') {
        let full = if h.len() == 3 {
            h.chars().flat_map(|c| [c, c]).collect::<String>()
        } else {
            h.to_owned()
        };
        return parse_rgb(&full).ok();
    }
    if let Some(args) = v.strip_prefix("rgb(").or_else(|| v.strip_prefix("rgba(")) {
        let parts: Vec<&str> = args.trim_end_matches(')').split(',').collect();
        if parts.len() >= 3 {
            if parts.len() == 4 && parts[3].trim().parse::<f32>().is_ok_and(|a| a < 0.1) {
                return None;
            }
            let c = |s: &str| {
                s.trim()
                    .parse::<f32>()
                    .ok()
                    .map(|x| x.clamp(0.0, 255.0) as u8)
            };
            return Some(kiln_common::Color::rgb(
                c(parts[0])?,
                c(parts[1])?,
                c(parts[2])?,
            ));
        }
        return None;
    }
    let named = match v.as_str() {
        "black" => 0x000000,
        "white" => 0xFFFFFF,
        "red" => 0xFF0000,
        "green" => 0x008000,
        "blue" => 0x0000FF,
        "yellow" => 0xFFFF00,
        "gray" | "grey" => 0x808080,
        "orange" => 0xFFA500,
        "purple" => 0x800080,
        "navy" => 0x000080,
        "teal" => 0x008080,
        "maroon" => 0x800000,
        _ => return None,
    };
    let [_, r, g, b] = u32::to_be_bytes(named);
    Some(kiln_common::Color::rgb(r, g, b))
}

/// A CSS length to points.
fn css_points(v: &str, base_pt: f32) -> Option<f32> {
    let v = v.trim().to_ascii_lowercase();
    let num = |s: &str| {
        s.trim()
            .parse::<f32>()
            .ok()
            .filter(|x| x.is_finite() && *x > 0.0)
    };
    let pt = if let Some(n) = v.strip_suffix("pt") {
        num(n)?
    } else if let Some(n) = v.strip_suffix("px") {
        num(n)? * 0.75
    } else if let Some(n) = v.strip_suffix("em").or_else(|| v.strip_suffix("rem")) {
        num(n)? * base_pt
    } else if let Some(n) = v.strip_suffix('%') {
        num(n)? * base_pt / 100.0
    } else {
        match v.as_str() {
            "xx-small" => 7.0,
            "x-small" => 7.5,
            "small" => 10.0,
            "medium" => 12.0,
            "large" => 13.5,
            "x-large" => 18.0,
            "xx-large" => 24.0,
            _ => return None,
        }
    };
    Some(pt.clamp(1.0, 1638.0))
}

fn color_of(c: kiln_common::Color) -> kiln_doc::DocColor {
    kiln_doc::DocColor::rgb((u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b))
}

/// Applies a `style` attribute to run properties and returns the block alignment it sets.
fn apply_css(css: &str, p: &mut RunProps) -> Option<Alignment> {
    let mut align = None;
    for decl in css.split(';') {
        let Some((k, v)) = decl.split_once(':') else {
            continue;
        };
        let k = k.trim().to_ascii_lowercase();
        let v = v.trim().trim_end_matches("!important").trim();
        let lv = v.to_ascii_lowercase();
        match k.as_str() {
            "font-weight" => {
                let bold =
                    lv == "bold" || lv == "bolder" || lv.parse::<u32>().is_ok_and(|w| w >= 600);
                p.bold = Some(bold);
            }
            "font-style" => p.italic = Some(lv == "italic" || lv == "oblique"),
            "text-decoration" | "text-decoration-line" => {
                if lv.contains("underline") {
                    p.underline = Some(kiln_doc::Underline::new(UnderlineStyle::Single));
                }
                if lv.contains("line-through") {
                    p.strike = Some(true);
                }
                if lv == "none" {
                    p.underline = None;
                    p.strike = None;
                }
            }
            "color" => {
                if let Some(c) = css_color(&lv) {
                    p.color = Some(color_of(c));
                }
            }
            "background-color" | "background" => {
                if let Some(c) = css_color(lv.split_whitespace().next().unwrap_or(""))
                    && !(c.r > 245 && c.g > 245 && c.b > 245)
                {
                    p.highlight = Some(nearest_highlight(c));
                }
            }
            "font-size" => {
                let base = p.size.map_or(11.0, |s| s as f32 / 2.0);
                if let Some(pt) = css_points(&lv, base) {
                    let hp = (pt * 2.0).round() as u32;
                    p.size = Some(hp);
                    p.size_cs = Some(hp);
                }
            }
            "font-family" => {
                let first = v
                    .split(',')
                    .map(|f| f.trim().trim_matches(['"', '\'']).trim())
                    .find(|f| {
                        !f.is_empty()
                            && !matches!(
                                f.to_ascii_lowercase().as_str(),
                                "serif"
                                    | "sans-serif"
                                    | "monospace"
                                    | "cursive"
                                    | "fantasy"
                                    | "system-ui"
                                    | "inherit"
                                    | "initial"
                                    | "-apple-system"
                            )
                    });
                if let Some(f) = first.filter(|f| f.chars().count() <= 31) {
                    p.fonts = Some(kiln_doc::RunFonts::latin(f));
                }
            }
            "vertical-align" => match lv.as_str() {
                "super" => p.vert_align = Some(VertAlign::Superscript),
                "sub" => p.vert_align = Some(VertAlign::Subscript),
                _ => {}
            },
            "text-transform" if lv == "uppercase" => p.caps = Some(true),
            "font-variant" if lv.contains("small-caps") => p.small_caps = Some(true),
            "text-align" => {
                align = match lv.as_str() {
                    "center" => Some(Alignment::Center),
                    "right" | "end" => Some(Alignment::Right),
                    "justify" => Some(Alignment::Both),
                    "left" | "start" => Some(Alignment::Left),
                    _ => None,
                };
            }
            _ => {}
        }
    }
    align
}

impl Builder {
    fn new() -> Self {
        Self {
            blocks: Vec::new(),
            para: HtmlPara::default(),
            stack: Vec::new(),
            lists: Vec::new(),
            tables: Vec::new(),
            in_cell: false,
            drop_depth: 0,
            drop_tag: None,
        }
    }

    fn top(&self) -> Frame {
        self.stack.last().cloned().unwrap_or(Frame {
            tag: String::new(),
            props: RunProps::default(),
            link: None,
            align: None,
            style: None,
            pre: false,
        })
    }

    /// Ends the current paragraph (blank ones vanish, except list items).
    fn flush(&mut self) {
        let mut para = std::mem::take(&mut self.para);
        // Trim the paragraph's outer whitespace.
        if let Some(HtmlInline::Text { text, .. }) = para.inlines.first_mut() {
            *text = text.trim_start().to_owned();
        }
        if let Some(HtmlInline::Text { text, .. }) = para.inlines.last_mut() {
            *text = text.trim_end().to_owned();
        }
        para.inlines
            .retain(|i| !matches!(i, HtmlInline::Text { text, .. } if text.is_empty()));
        if para.is_blank() && para.list.is_none() {
            return;
        }
        if self.in_cell
            && let Some(row) = self.tables.last_mut().and_then(|t| t.last_mut())
            && let Some(cell) = row.last_mut()
        {
            cell.push(para);
            return;
        }
        if !self.tables.is_empty() {
            // Text between cells: dropped, as Word does.
            return;
        }
        self.blocks.push(HtmlBlock::Para(para));
    }

    fn text(&mut self, raw: &str) {
        if self.drop_depth > 0 {
            return;
        }
        let f = self.top();
        let decoded = decode_entities(raw);
        let text = if f.pre {
            decoded
        } else {
            let mut out = String::with_capacity(decoded.len());
            let mut space = false;
            for c in decoded.chars() {
                if c.is_whitespace() && c != '\u{00A0}' {
                    space = true;
                } else {
                    let after_text = || {
                        self.para.inlines.last().is_some_and(
                            |i| matches!(i, HtmlInline::Text { text, .. } if !text.ends_with(' ')),
                        )
                    };
                    if space && (!out.is_empty() || after_text()) {
                        out.push(' ');
                    }
                    space = false;
                    out.push(c);
                }
            }
            if space && !out.is_empty() {
                out.push(' ');
            }
            out
        };
        if text.is_empty() {
            return;
        }
        if f.pre && text.contains('\n') {
            let lines: Vec<&str> = text.split('\n').collect();
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    self.para.inlines.push(HtmlInline::Break);
                }
                if !line.is_empty() {
                    self.push_text(line, &f);
                }
            }
            return;
        }
        self.push_text(&text, &f);
    }

    fn push_text(&mut self, text: &str, f: &Frame) {
        if self.para.inlines.is_empty() {
            self.para.style.clone_from(&f.style);
            self.para.align = f.align;
        }
        let text = text.replace(['\r', '\u{FFFC}'], "");
        if let Some(HtmlInline::Text {
            text: last,
            props,
            link,
        }) = self.para.inlines.last_mut()
            && **props == f.props
            && *link == f.link
        {
            last.push_str(&text);
            return;
        }
        self.para.inlines.push(HtmlInline::Text {
            text,
            props: Box::new(f.props.clone()),
            link: f.link.clone(),
        });
    }

    fn open(&mut self, tag: &str, attrs: &[(String, String)]) {
        if self.drop_depth > 0 {
            if self.drop_tag.as_deref() == Some(tag) {
                self.drop_depth += 1;
            }
            return;
        }
        if DROP.contains(&tag) {
            self.drop_depth = 1;
            self.drop_tag = Some(tag.to_owned());
            return;
        }
        match tag {
            "br" => {
                self.para.inlines.push(HtmlInline::Break);
                return;
            }
            "hr" => {
                self.flush();
                return;
            }
            "table" => {
                self.flush();
                self.tables.push(Vec::new());
                self.in_cell = false;
            }
            "tr" => {
                self.flush();
                if let Some(t) = self.tables.last_mut() {
                    t.push(Vec::new());
                }
                self.in_cell = false;
            }
            "td" | "th" => {
                self.flush();
                if let Some(t) = self.tables.last_mut() {
                    if t.is_empty() {
                        t.push(Vec::new());
                    }
                    if let Some(row) = t.last_mut() {
                        row.push(Vec::new());
                    }
                    self.in_cell = true;
                }
            }
            "ul" | "ol" => {
                self.flush();
                if self.lists.len() < 9 {
                    self.lists.push(tag == "ol");
                }
            }
            _ => {}
        }
        if VOID.contains(&tag) {
            return;
        }
        if self.stack.len() >= MAX_DEPTH {
            return;
        }
        let mut f = self.top();
        f.tag = tag.to_owned();
        let mut align = None;
        let p = &mut f.props;
        match tag {
            "b" | "strong" | "th" => p.bold = Some(true),
            "i" | "em" | "cite" | "dfn" | "var" => p.italic = Some(true),
            "u" | "ins" => p.underline = Some(kiln_doc::Underline::new(UnderlineStyle::Single)),
            "s" | "strike" | "del" => p.strike = Some(true),
            "sub" => p.vert_align = Some(VertAlign::Subscript),
            "sup" => p.vert_align = Some(VertAlign::Superscript),
            "code" | "kbd" | "samp" | "tt" | "pre" => {
                p.fonts = Some(kiln_doc::RunFonts::latin("Courier New"));
            }
            "mark" => p.highlight = Some(HighlightColor::Yellow),
            "a" => {
                f.link = attrs
                    .iter()
                    .find(|(k, _)| k == "href")
                    .map(|(_, v)| v.trim().to_owned())
                    .filter(|v| super::insert::safe_url(v));
                if f.link.is_some() {
                    p.style_id = Some("Hyperlink".into());
                }
            }
            "font" => {
                for (k, v) in attrs {
                    match k.as_str() {
                        "color" => {
                            if let Some(c) = css_color(v) {
                                p.color = Some(color_of(c));
                            }
                        }
                        "face" => {
                            if let Some(face) = v.split(',').next().map(str::trim)
                                && !face.is_empty()
                                && face.chars().count() <= 31
                            {
                                p.fonts = Some(kiln_doc::RunFonts::latin(face));
                            }
                        }
                        "size" => {
                            let pt = match v.trim() {
                                "1" => 8.0,
                                "2" => 10.0,
                                "3" => 12.0,
                                "4" => 14.0,
                                "5" => 18.0,
                                "6" => 24.0,
                                "7" => 36.0,
                                _ => 0.0,
                            };
                            if pt > 0.0 {
                                p.size = Some((pt * 2.0) as u32);
                            }
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        for (k, v) in attrs {
            if k == "style" {
                align = apply_css(v, &mut f.props);
            } else if k == "align" {
                align = match v.to_ascii_lowercase().as_str() {
                    "center" => Some(Alignment::Center),
                    "right" => Some(Alignment::Right),
                    "justify" => Some(Alignment::Both),
                    _ => None,
                };
            }
        }
        if BLOCKS.contains(&tag) || tag == "td" || tag == "th" {
            self.flush();
            f.align = align.or(f.align);
            f.style = match tag {
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => Some(format!("Heading{}", &tag[1..])),
                "blockquote" => Some("Quote".into()),
                _ => f.style,
            };
            if tag == "pre" {
                f.pre = true;
            }
            if tag == "li" {
                let level = u8::try_from(self.lists.len().saturating_sub(1)).unwrap_or(8);
                let ordered = self.lists.last().copied().unwrap_or(false);
                self.para.list = Some((ordered, level.min(8)));
            }
        }
        self.stack.push(f);
    }

    fn close(&mut self, tag: &str) {
        if self.drop_depth > 0 {
            if self.drop_tag.as_deref() == Some(tag) {
                self.drop_depth -= 1;
                if self.drop_depth == 0 {
                    self.drop_tag = None;
                }
            }
            return;
        }
        if let Some(i) = self.stack.iter().rposition(|f| f.tag == tag) {
            if BLOCKS.contains(&tag) || matches!(tag, "td" | "th") {
                self.flush();
            }
            self.stack.truncate(i);
        }
        match tag {
            "ul" | "ol" => {
                self.flush();
                self.lists.pop();
            }
            "td" | "th" => {
                self.flush();
                self.in_cell = false;
            }
            "tr" => {
                self.flush();
                self.in_cell = false;
            }
            "table" => {
                self.flush();
                self.in_cell = false;
                if let Some(t) = self.tables.pop() {
                    let rows: Vec<Vec<Vec<HtmlPara>>> =
                        t.into_iter().filter(|r| !r.is_empty()).collect();
                    if rows.is_empty() {
                        return;
                    }
                    if let Some(outer) = self.tables.last_mut() {
                        // A nested table: its text goes into the outer cell, row by row.
                        if let Some(cell) = outer.last_mut().and_then(|r| r.last_mut()) {
                            for row in rows {
                                for c in row {
                                    cell.extend(c);
                                }
                            }
                        }
                    } else {
                        self.blocks.push(HtmlBlock::Table(rows));
                    }
                }
            }
            _ => {}
        }
    }
}

/// Parses clipboard HTML into blocks.
pub(super) fn parse(html: &str) -> Vec<HtmlBlock> {
    let html = if html.len() > MAX_HTML {
        let mut end = MAX_HTML;
        while !html.is_char_boundary(end) {
            end -= 1;
        }
        &html[..end]
    } else {
        html
    };
    // Office and browsers wrap the copied part in StartFragment/EndFragment comments.
    let html = match (
        html.find("<!--StartFragment-->"),
        html.find("<!--EndFragment-->"),
    ) {
        (Some(a), Some(b)) if a < b => &html[a + 20..b],
        _ => html,
    };
    let mut b = Builder::new();
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        b.text(&rest[..lt]);
        let tail = &rest[lt..];
        if let Some(body) = tail.strip_prefix("<!--") {
            rest = body.find("-->").map_or("", |e| &body[e + 3..]);
            continue;
        }
        if tail.starts_with("<!") || tail.starts_with("<?") {
            rest = tail.find('>').map_or("", |e| &tail[e + 1..]);
            continue;
        }
        // Find the closing '>' outside quotes.
        let bytes = tail.as_bytes();
        let mut i = 1;
        let mut quote = 0u8;
        while i < bytes.len() {
            let c = bytes[i];
            if quote != 0 {
                if c == quote {
                    quote = 0;
                }
            } else if c == b'"' || c == b'\'' {
                quote = c;
            } else if c == b'>' {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() {
            // An unterminated tag: the rest is text.
            b.text(tail);
            rest = "";
            break;
        }
        let inner = &tail[1..i];
        rest = &tail[i + 1..];
        if let Some(name) = inner.strip_prefix('/') {
            let name = name.trim().to_ascii_lowercase();
            b.close(&name);
        } else {
            let name_end = inner
                .find(|c: char| c.is_whitespace() || c == '/')
                .unwrap_or(inner.len());
            let name = inner[..name_end].to_ascii_lowercase();
            if name.is_empty() || !name.starts_with(|c: char| c.is_ascii_alphabetic()) {
                b.text(&tail[..=i]);
                continue;
            }
            let attrs = attributes(&inner[name_end..]);
            b.open(&name, &attrs);
            if inner.trim_end().ends_with('/') && !VOID.contains(&name.as_str()) {
                b.close(&name);
            }
        }
    }
    b.text(rest);
    while !b.tables.is_empty() {
        b.close("table");
    }
    b.flush();
    b.blocks
}

#[cfg(test)]
mod unit {
    use super::*;

    fn texts(blocks: &[HtmlBlock]) -> Vec<String> {
        blocks
            .iter()
            .map(|b| match b {
                HtmlBlock::Para(p) => p
                    .inlines
                    .iter()
                    .map(|i| match i {
                        HtmlInline::Text { text, .. } => text.clone(),
                        HtmlInline::Break => "\n".into(),
                    })
                    .collect(),
                HtmlBlock::Table(rows) => format!("table {}x{}", rows.len(), rows[0].len()),
            })
            .collect()
    }

    #[test]
    fn paragraphs_formatting_and_entities() {
        let b = parse(
            "<html><head><style>p{}</style></head><body><h2>Hello &amp; welcome</h2>\
             <p>A <b>bold</b>   and <i>italic</i>&nbsp;word<br>next</p><script>alert(1)</script></body>",
        );
        assert_eq!(
            texts(&b),
            vec!["Hello & welcome", "A bold and italic\u{a0}word\nnext"]
        );
        let HtmlBlock::Para(h) = &b[0] else {
            panic!("heading")
        };
        assert_eq!(h.style.as_deref(), Some("Heading2"));
        let HtmlBlock::Para(p) = &b[1] else {
            panic!("para")
        };
        assert!(
            matches!(&p.inlines[1], HtmlInline::Text { props, .. } if props.bold == Some(true))
        );
    }

    #[test]
    fn lists_tables_links_and_css() {
        let b = parse(
            "<ul><li>one</li><li>two<ol><li>deep</li></ol></li></ul>\
             <table><tr><th>A</th><td>B</td></tr><tr><td>C</td><td>D</td></tr></table>\
             <p style=\"text-align:center\"><a href=\"javascript:x()\">bad</a> \
             <a href=\"https://kiln.dev\">good</a> <span style=\"color:#ff0000;font-size:18px\">red</span></p>",
        );
        let t = texts(&b);
        assert_eq!(t[0], "one");
        assert_eq!(t[3], "table 2x2");
        let HtmlBlock::Para(deep) = &b[2] else {
            panic!("li")
        };
        assert_eq!(deep.list, Some((true, 1)));
        let HtmlBlock::Para(last) = &b[4] else {
            panic!("p")
        };
        assert_eq!(last.align, Some(Alignment::Center));
        let links: Vec<_> = last
            .inlines
            .iter()
            .filter_map(|i| match i {
                HtmlInline::Text { link: Some(l), .. } => Some(l.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(links, vec!["https://kiln.dev".to_owned()]);
        let red = last.inlines.iter().find_map(|i| match i {
            HtmlInline::Text { text, props, .. } if text == "red" => Some(props.clone()),
            _ => None,
        });
        assert_eq!(red.and_then(|p| p.size), Some(27));
    }

    #[test]
    fn hostile_input_is_harmless() {
        assert!(parse("<<<>>>&#0;&#xD800;<a href='x").len() <= 2);
        let deep = "<b>".repeat(10_000) + "x";
        assert_eq!(texts(&parse(&deep)), vec!["x"]);
        assert_eq!(
            decode_entities("&#65;&#x42;&unknown;&amp"),
            "AB&unknown;&amp"
        );
    }
}
