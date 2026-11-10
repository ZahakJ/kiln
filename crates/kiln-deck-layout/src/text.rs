// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text bodies: a shape's paragraphs laid out by `kiln-text` with the formatting resolved through
//! the placeholder chain (`kiln_deck::inherit`), bullets and numbering from the list styles,
//! paragraph spacing, autofit (shrink on overflow, as PowerPoint's `normAutofit`), vertical
//! anchoring, columns and rotated or stacked text; then drawn as glyph runs, decorations and
//! highlights, and asked for carets, hit tests and selection rectangles while the text is edited.
//!
//! Coordinates are px in the text *area* (the shape's text rectangle inside its insets) unless a
//! function says otherwise. Paragraph positions are kiln-doc char offsets ([`kiln_doc::Pos`]);
//! layouts work in UTF-8 bytes, and [`OffsetMap`] converts.

use std::ops::Range;

use kiln_common::NodeId;
use kiln_deck::inherit::ResolvedParagraph;
use kiln_deck::text::{
    Autofit, Bullet, BulletColor, BulletFont, BulletSize, ResolvedBodyProps, TextAnchor,
    TextDirection, TextSpacing, TextWrap,
};
use kiln_deck::{Presentation, TextBody};
use kiln_doc::merge::merge_run_props;
use kiln_doc::st::{Alignment as DocAlign, ThemeFontRef, UnderlineStyle, VertAlign};
use kiln_doc::{InlineObject, Mark, Paragraph, RunProps};
use kiln_ooxml::drawingml::color::{ColorContext, DrawingColor};
use kiln_ooxml::drawingml::theme::Theme;
use kiln_scene::{Bounds, DrawOp, Paint};
use kiln_text::{
    Alignment, Direction, LineSpacing, ParagraphLayout, ParagraphProps, ParagraphSpec, Rgba,
    SpanStyle, StrikeKind, TabAlignment, TabLeader, TabStop, TextEngine, TextRun, UnderlineKind,
};
use kurbo::Affine;

use crate::geom::pt;
use crate::glyphs::Glyphs;
use crate::paint::color;

/// Default text size of DrawingML (18 pt) when nothing in the chain sets one.
pub const DEFAULT_SIZE_PT: f32 = 18.0;

/// Smallest autofit font scale tried (PowerPoint stops at 25 %).
const MIN_FONT_SCALE: f32 = 0.25;

/// What text resolves against besides the presentation.
#[derive(Debug, Clone, Copy)]
pub struct TextEnv<'a> {
    /// The theme (fonts and colours).
    pub theme: &'a Theme,
    /// Colour context of the slide.
    pub ctx: ColorContext,
    /// The slide's number (1-based) for `slidenum` fields.
    pub slide_number: usize,
    /// Today's date for `datetime` fields, as `(year, month, day)`.
    pub today: Option<(i32, u32, u32)>,
    /// Draw bullets of empty paragraphs (while editing, as PowerPoint does).
    pub editing: bool,
    /// The colour of text that sets none (a table style's cell text), instead of `tx1`.
    pub fallback_color: Option<Rgba>,
    /// Text that sets no weight is bold (a table style's header row).
    pub fallback_bold: bool,
}

/// Maps between kiln-doc char offsets of a paragraph and byte offsets of its laid-out text.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct OffsetMap {
    /// `(doc offset, doc length, byte offset, byte length)` of each piece, in order.
    pieces: Vec<(usize, usize, usize, usize)>,
    doc_len: usize,
    byte_len: usize,
}

impl OffsetMap {
    fn push(&mut self, doc_len: usize, byte_len: usize) {
        self.pieces
            .push((self.doc_len, doc_len, self.byte_len, byte_len));
        self.doc_len += doc_len;
        self.byte_len += byte_len;
    }

    /// The byte offset of a char offset (inside a multi-char piece, its start or end).
    #[must_use]
    pub fn byte_of(&self, doc: usize) -> usize {
        for &(d, dl, b, bl) in &self.pieces {
            if doc < d + dl || (doc == d + dl && dl == 0) {
                if doc <= d {
                    return b;
                }
                // Plain text: one char per char; objects (fields) snap to an end.
                if dl == bl || dl == 0 {
                    return b + (doc - d).min(bl);
                }
                return b + bl;
            }
        }
        self.byte_len
    }

    /// The char offset of a byte offset.
    #[must_use]
    pub fn doc_of(&self, byte: usize) -> usize {
        for &(d, dl, b, bl) in &self.pieces {
            if byte < b + bl {
                if byte <= b {
                    return d;
                }
                if dl == bl {
                    return d + (byte - b);
                }
                return if byte - b >= bl.div_ceil(2) {
                    d + dl
                } else {
                    d
                };
            }
        }
        self.doc_len
    }

    /// The paragraph's length in chars.
    #[must_use]
    pub fn doc_len(&self) -> usize {
        self.doc_len
    }
}

/// A bullet or number laid out in front of a paragraph's first line.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidBullet {
    /// The label's own layout.
    pub layout: ParagraphLayout,
    /// Left of the label in paragraph coordinates.
    pub x: f32,
}

/// One laid-out paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct LaidPara {
    /// The paragraph.
    pub id: NodeId,
    /// The kiln-text layout (paragraph coordinates).
    pub layout: ParagraphLayout,
    /// Char ↔ byte offsets.
    pub map: OffsetMap,
    /// Bullet or number.
    pub bullet: Option<LaidBullet>,
    /// Highlighted byte ranges with their colours.
    pub highlights: Vec<(Range<usize>, u32)>,
    /// Hyperlinked byte ranges with their targets.
    pub links: Vec<(Range<usize>, String)>,
    /// Font size of the first run, px (the empty paragraph's caret height).
    pub size: f32,
    /// The paragraph's alignment (unwrapped text is laid out left and placed by this).
    pub align: Alignment,
}

/// A line placed in the text area: add `(dx, dy)` to the paragraph's own coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlacedLine {
    /// Paragraph index.
    pub para: usize,
    /// Line index in the paragraph.
    pub line: usize,
    /// Horizontal offset.
    pub dx: f32,
    /// Vertical offset.
    pub dy: f32,
}

/// A laid-out text body.
#[derive(Debug, Clone, PartialEq)]
pub struct BodyLayout {
    /// The text area in shape-local px (inside the insets, before any vertical turn).
    pub area: Bounds,
    /// The paragraphs.
    pub paras: Vec<LaidPara>,
    /// Every line, in reading order.
    pub lines: Vec<PlacedLine>,
    /// The autofit scale applied to font sizes.
    pub font_scale: f32,
    /// Height of the laid-out text.
    pub content_height: f32,
    /// Width of the widest line (the area's width for wrapped text).
    pub content_width: f32,
    /// Quarter turns of vertical text: 0, 1 (`vert`, clockwise) or 3 (`vert270`).
    pub turn: u8,
    /// The text does not fit its area.
    pub overflow: bool,
}

impl BodyLayout {
    /// The matrix from text-flow coordinates to shape-local px (vertical text turns the flow).
    #[must_use]
    pub fn flow_matrix(&self) -> Affine {
        let a = self.area;
        let (x, y, w, h) = (
            f64::from(a.x),
            f64::from(a.y),
            f64::from(a.w),
            f64::from(a.h),
        );
        match self.turn {
            1 => Affine::translate((x + w, y)) * Affine::rotate(std::f64::consts::FRAC_PI_2),
            3 => Affine::translate((x, y + h)) * Affine::rotate(-std::f64::consts::FRAC_PI_2),
            _ => Affine::translate((x, y)),
        }
    }

    /// The flow box: the area's size, swapped for vertical text.
    #[must_use]
    pub fn flow_size(&self) -> (f32, f32) {
        if self.turn % 2 == 1 {
            (self.area.h, self.area.w)
        } else {
            (self.area.w, self.area.h)
        }
    }

    fn para_index(&self, id: NodeId) -> Option<usize> {
        self.paras.iter().position(|p| p.id == id)
    }

    /// The caret of a position as `(x, top, height)` in flow coordinates.
    #[must_use]
    pub fn caret(&self, para: NodeId, offset: usize) -> Option<(f32, f32, f32)> {
        let i = self.para_index(para)?;
        let p = &self.paras[i];
        let byte = p.map.byte_of(offset);
        let c = p.layout.caret_rect(byte);
        let placed = self.lines.iter().find(|l| l.para == i && l.line == c.line);
        let (dx, dy) = placed.map_or((0.0, 0.0), |l| (l.dx, l.dy));
        Some((c.x + dx, c.top + dy, c.height.max(p.size * 0.9)))
    }

    /// The position nearest a point in flow coordinates.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32) -> Option<(NodeId, usize)> {
        let mut best: Option<(f32, usize, usize)> = None;
        for (k, l) in self.lines.iter().enumerate() {
            let p = &self.paras[l.para];
            let Some(line) = p.layout.lines.get(l.line) else {
                continue;
            };
            let top = line.top + l.dy;
            let bottom = top + line.height;
            let left = l.dx - 4.0;
            let right = l.dx + p.layout.width + 4.0;
            let dyv = if y < top {
                top - y
            } else if y > bottom {
                y - bottom
            } else {
                0.0
            };
            let dxv = if x < left {
                left - x
            } else if x > right {
                x - right
            } else {
                0.0
            };
            let d = dyv * 4.0 + dxv;
            if best.is_none_or(|b| d < b.0) {
                best = Some((d, k, l.para));
            }
        }
        let (_, k, pi) = best?;
        let l = self.lines[k];
        let p = &self.paras[pi];
        let line = &p.layout.lines[l.line];
        let ly = (y - l.dy).clamp(line.top + 0.5, line.top + line.height - 0.5);
        let byte = p.layout.hit_test(x - l.dx, ly);
        Some((p.id, p.map.doc_of(byte)))
    }

    /// Selection rectangles (flow coordinates) between two positions in reading order.
    #[must_use]
    pub fn selection(&self, from: (NodeId, usize), to: (NodeId, usize)) -> Vec<Bounds> {
        let (Some(a), Some(b)) = (self.para_index(from.0), self.para_index(to.0)) else {
            return Vec::new();
        };
        let ((a, ao), (b, bo)) = if (a, from.1) <= (b, to.1) {
            ((a, from.1), (b, to.1))
        } else {
            ((b, to.1), (a, from.1))
        };
        let mut out = Vec::new();
        for i in a..=b {
            let p = &self.paras[i];
            let start = if i == a { p.map.byte_of(ao) } else { 0 };
            let end = if i == b {
                p.map.byte_of(bo)
            } else {
                p.layout.text().len()
            };
            let mut rects = p.layout.selection_rects(start..end);
            if i != b {
                // The paragraph mark: a small block at the end of the last line.
                if let Some(last) = p.layout.lines.last() {
                    rects.push(kiln_text::Rect {
                        x: last.x + last.width,
                        y: last.top,
                        width: p.size * 0.3,
                        height: last.height,
                    });
                }
            }
            for r in rects {
                let line = p
                    .layout
                    .lines
                    .iter()
                    .position(|l| {
                        r.y + r.height / 2.0 >= l.top && r.y + r.height / 2.0 <= l.top + l.height
                    })
                    .unwrap_or(0);
                let placed = self.lines.iter().find(|l| l.para == i && l.line == line);
                let (dx, dy) = placed.map_or((0.0, 0.0), |l| (l.dx, l.dy));
                out.push(Bounds::new(r.x + dx, r.y + dy, r.width.max(1.0), r.height));
            }
        }
        out
    }

    /// The position one line up or down from a caret, keeping `x` (flow coordinates).
    #[must_use]
    pub fn line_step(
        &self,
        para: NodeId,
        offset: usize,
        down: bool,
        x: f32,
    ) -> Option<(NodeId, usize)> {
        let i = self.para_index(para)?;
        let p = &self.paras[i];
        let c = p.layout.caret_rect(p.map.byte_of(offset));
        let k = self
            .lines
            .iter()
            .position(|l| l.para == i && l.line == c.line)?;
        let target = if down {
            self.lines.get(k + 1)?
        } else {
            self.lines.get(k.checked_sub(1)?)?
        };
        let tp = &self.paras[target.para];
        let line = tp.layout.lines.get(target.line)?;
        let byte = tp
            .layout
            .hit_test(x - target.dx, line.top + line.height / 2.0);
        Some((tp.id, tp.map.doc_of(byte)))
    }

    /// Whether a paragraph runs right to left (its arrows move the other way, as in Word).
    #[must_use]
    pub fn is_rtl(&self, para: NodeId) -> bool {
        self.para_index(para)
            .and_then(|i| self.paras.get(i))
            .is_some_and(|p| p.layout.rtl)
    }

    /// The start or end of the line holding a position.
    #[must_use]
    pub fn line_edge(&self, para: NodeId, offset: usize, end: bool) -> Option<usize> {
        let i = self.para_index(para)?;
        let p = &self.paras[i];
        let c = p.layout.caret_rect(p.map.byte_of(offset));
        let line = p.layout.lines.get(c.line)?;
        let mut byte = if end {
            line.text_range.end
        } else {
            line.text_range.start
        };
        if end && byte > line.text_range.start {
            // Before a trailing break or the space a wrap broke at.
            let text = p.layout.text();
            let prev = text[..byte].chars().last();
            if matches!(prev, Some('\u{2028}' | ' ')) && byte < text.len() {
                byte -= prev.map_or(0, char::len_utf8);
            }
        }
        Some(p.map.doc_of(byte))
    }

    /// The hyperlink under a point (flow coordinates), if any.
    #[must_use]
    pub fn link_at(&self, x: f32, y: f32) -> Option<&str> {
        let (id, off) = self.hit(x, y)?;
        let p = &self.paras[self.para_index(id)?];
        let b = p.map.byte_of(off);
        p.links
            .iter()
            .find(|(r, _)| r.start <= b && b < r.end)
            .map(|(_, u)| u.as_str())
    }
}

/// The font family a run asks for, with theme references resolved.
#[must_use]
pub fn family(run: &RunProps, theme: &Theme) -> String {
    let f = run.fonts.as_ref();
    let theme_ref = f.and_then(|f| f.ascii_theme.or(f.h_ansi_theme));
    if let Some(t) = theme_ref {
        let fs = &theme.font_scheme;
        let c = if t.is_major() { &fs.major } else { &fs.minor };
        let name = &c.latin.typeface;
        if !name.is_empty() {
            return name.clone();
        }
    }
    let name = f
        .and_then(|f| f.ascii.clone().or_else(|| f.h_ansi.clone()))
        .unwrap_or_default();
    match name.as_str() {
        "+mj-lt" | "+mj-ea" | "+mj-cs" => theme.font_scheme.major.latin.typeface.clone(),
        "+mn-lt" | "+mn-ea" | "+mn-cs" | "" => {
            let n = theme.font_scheme.minor.latin.typeface.clone();
            if n.is_empty() { "Calibri".into() } else { n }
        }
        _ => name,
    }
}

/// The text colour of a run (`tx1` when nothing sets one).
#[must_use]
pub fn run_color(run: &RunProps, ctx: &ColorContext) -> Rgba {
    match &run.color {
        Some(c) => {
            let d = kiln_deck::dml::color::doc_to_drawing_color(c);
            let theme = color(&d, ctx);
            // A theme colour with a transform kiln-doc cannot hold (Office's `tint 75000`
            // subtitle) keeps its rendering as the cached value: use it while it is still a
            // shade or tint of the theme colour in force (so a new theme still recolours).
            match c.val {
                kiln_doc::ColorValue::Rgb(cached)
                    if c.theme_color.is_some()
                        && c.theme_tint.is_none()
                        && c.theme_shade.is_none() =>
                {
                    let cached = u32::from_be_bytes([cached.r, cached.g, cached.b, 0xff]);
                    if cached != theme && is_tint_or_shade(theme, cached) {
                        cached
                    } else {
                        theme
                    }
                }
                _ => theme,
            }
        }
        None => color(&DrawingColor::scheme("tx1"), ctx),
    }
}

/// Whether `c` lies on the line from `base` to white or to black (a tint or shade of it).
#[must_use]
pub fn is_tint_or_shade(base: u32, c: u32) -> bool {
    let ch = |v: u32, s: u32| f32::from(u8::try_from((v >> s) & 0xff).unwrap_or(0));
    let b = [ch(base, 24), ch(base, 16), ch(base, 8)];
    let x = [ch(c, 24), ch(c, 16), ch(c, 8)];
    let consistent = |ratios: Vec<f32>| {
        if ratios.is_empty() {
            return false;
        }
        let lo = ratios.iter().copied().fold(f32::INFINITY, f32::min);
        let hi = ratios.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        hi - lo < 0.12 && (0.0..=1.05).contains(&lo)
    };
    // Toward white: (255 - x) / (255 - b) is the same for every channel with room.
    let tint: Vec<f32> = (0..3)
        .filter(|&i| 255.0 - b[i] > 24.0)
        .map(|i| (255.0 - x[i]) / (255.0 - b[i]))
        .collect();
    let shade: Vec<f32> = (0..3)
        .filter(|&i| b[i] > 24.0)
        .map(|i| x[i] / b[i])
        .collect();
    let near_white = (0..3).all(|i| 255.0 - b[i] <= 24.0 || x[i] >= b[i] - 1.0);
    let near_black = (0..3).all(|i| b[i] <= 24.0 || x[i] <= b[i] + 1.0);
    (near_white && consistent(tint)) || (near_black && consistent(shade))
}

/// A run's style for kiln-text, sizes scaled by `scale`.
#[must_use]
pub fn span_style(run: &RunProps, env: &TextEnv<'_>, scale: f32) -> SpanStyle {
    #[allow(clippy::cast_precision_loss)]
    let size_pt = run.size.map_or(DEFAULT_SIZE_PT, |hp| hp as f32 / 2.0);
    let mut size = pt(size_pt) * scale;
    let mut shift = 0.0;
    match run.vert_align {
        Some(VertAlign::Superscript) => {
            shift = size * 0.33;
            size *= 0.67;
        }
        Some(VertAlign::Subscript) => {
            shift = -size * 0.14;
            size *= 0.67;
        }
        _ => {}
    }
    if let Some(p) = run.position {
        #[allow(clippy::cast_precision_loss)]
        {
            shift += pt(p as f32 / 2.0) * scale;
        }
    }
    let underline = match run.underline.as_ref().map(|u| u.style) {
        None | Some(UnderlineStyle::None) => UnderlineKind::None,
        Some(UnderlineStyle::Double) => UnderlineKind::Double,
        Some(UnderlineStyle::Thick) => UnderlineKind::Thick,
        Some(UnderlineStyle::Dotted | UnderlineStyle::DottedHeavy) => UnderlineKind::Dotted,
        Some(
            UnderlineStyle::Dash
            | UnderlineStyle::DashedHeavy
            | UnderlineStyle::DashLong
            | UnderlineStyle::DashLongHeavy
            | UnderlineStyle::DotDash
            | UnderlineStyle::DashDotHeavy
            | UnderlineStyle::DotDotDash
            | UnderlineStyle::DashDotDotHeavy,
        ) => UnderlineKind::Dashed,
        Some(UnderlineStyle::Wave | UnderlineStyle::WavyHeavy | UnderlineStyle::WavyDouble) => {
            UnderlineKind::Wavy
        }
        Some(UnderlineStyle::Words) => UnderlineKind::Words,
        Some(UnderlineStyle::Single) => UnderlineKind::Single,
    };
    let strike = if run.dstrike == Some(true) {
        StrikeKind::Double
    } else if run.strike == Some(true) {
        StrikeKind::Single
    } else {
        StrikeKind::None
    };
    #[allow(clippy::cast_precision_loss)]
    let letter_spacing = run.spacing.map_or(0.0, |tw| tw as f32 / 15.0) * scale;
    SpanStyle {
        family: family(run, env.theme),
        size: size.max(0.5),
        weight: if run.bold.unwrap_or(env.fallback_bold) {
            700
        } else {
            400
        },
        italic: run.italic == Some(true),
        color: match (run.color.is_some(), env.fallback_color) {
            (false, Some(c)) => c,
            _ => run_color(run, &env.ctx),
        },
        underline,
        strike,
        letter_spacing,
        baseline_shift: shift,
        lang: run.lang.as_ref().and_then(|l| l.val.clone()),
        rtl: run.rtl == Some(true),
        kerning: true,
        ligatures: true,
    }
}

/// The text a field shows.
fn field_text(instr: &str, result: &str, env: &TextEnv<'_>) -> String {
    let kind = instr.trim().to_ascii_lowercase();
    if kind == "slidenum" {
        return env.slide_number.to_string();
    }
    if kind.starts_with("datetime")
        && let Some((y, m, d)) = env.today
    {
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
        let month = MONTHS
            .get(usize::try_from(m).unwrap_or(1).saturating_sub(1))
            .copied()
            .unwrap_or("January");
        return match kind.as_str() {
            "datetime2" | "datetime3" => format!("{d} {month} {y}"),
            "datetime4" => format!("{month} {d}, {y}"),
            _ => format!("{d:02}/{m:02}/{y:04}"),
        };
    }
    result.to_owned()
}

/// Symbol-font bullets as the Unicode characters the bundled fonts carry.
fn bullet_char(ch: &str, font: Option<&str>) -> String {
    let symbolic = font.is_some_and(|f| {
        let f = f.to_ascii_lowercase();
        f.contains("wingdings") || f == "symbol" || f.contains("webdings")
    });
    if !symbolic {
        return ch.to_owned();
    }
    let c = ch.chars().next().unwrap_or('•');
    let mapped = match c {
        'Ø' | 'è' => '►',
        '§' | 'n' => '■',
        'q' | 'o' => '□',
        'ü' => '√',
        'v' | 'w' | 'u' => '♦',
        'l' => '●',
        'à' | 'ð' => '→',
        '\u{b7}' => '•',
        _ => '•',
    };
    mapped.to_string()
}

/// Upper-case roman numerals.
fn roman(mut n: u32) -> String {
    const T: [(u32, &str); 13] = [
        (1000, "M"),
        (900, "CM"),
        (500, "D"),
        (400, "CD"),
        (100, "C"),
        (90, "XC"),
        (50, "L"),
        (40, "XL"),
        (10, "X"),
        (9, "IX"),
        (5, "V"),
        (4, "IV"),
        (1, "I"),
    ];
    let mut s = String::new();
    for (v, r) in T {
        while n >= v {
            s.push_str(r);
            n -= v;
        }
    }
    s
}

fn alpha(n: u32) -> String {
    let mut n = n.max(1);
    let mut s = Vec::new();
    while n > 0 {
        let r = (n - 1) % 26;
        s.push(char::from(b'A' + u8::try_from(r).unwrap_or(0)));
        n = (n - 1) / 26;
    }
    s.iter().rev().collect()
}

/// The label of an automatic number (`a:buAutoNum`).
#[must_use]
pub fn auto_number(scheme: &str, n: u32) -> String {
    let (body, style) = if let Some(s) = scheme.strip_prefix("arabic") {
        (n.to_string(), s)
    } else if let Some(s) = scheme.strip_prefix("alphaLc") {
        (alpha(n).to_lowercase(), s)
    } else if let Some(s) = scheme.strip_prefix("alphaUc") {
        (alpha(n), s)
    } else if let Some(s) = scheme.strip_prefix("romanLc") {
        (roman(n).to_lowercase(), s)
    } else if let Some(s) = scheme.strip_prefix("romanUc") {
        (roman(n), s)
    } else {
        (n.to_string(), "Period")
    };
    match style {
        "ParenBoth" => format!("({body})"),
        "ParenR" => format!("{body})"),
        "Plain" => body,
        "Minus" | "DbPlain" => format!("{body} -"),
        _ => format!("{body}."),
    }
}

fn spacing_px(s: TextSpacing, size: f32) -> f32 {
    match s {
        #[allow(clippy::cast_precision_loss)]
        TextSpacing::Points(h) => pt(h as f32 / 100.0),
        #[allow(clippy::cast_precision_loss)]
        TextSpacing::Percent(p) => size * 1.2 * (p as f32 / 100_000.0),
    }
}

/// Inputs of one body layout.
#[derive(Debug, Clone, Copy)]
pub struct BodyInput<'a> {
    /// The shape whose formatting chain applies.
    pub shape: NodeId,
    /// Its text.
    pub body: &'a TextBody,
    /// Body properties resolved through the chain.
    pub props: &'a ResolvedBodyProps,
    /// The text rectangle in shape-local px (the geometry's text rectangle), before insets.
    pub rect: Bounds,
}

struct ParaInputs {
    resolved: ResolvedParagraph,
    level: usize,
}

/// Lays out a text body (with the autofit scale the body stores, as PowerPoint draws a file
/// until its text is edited).
#[must_use]
pub fn layout_body(
    engine: &mut TextEngine,
    pres: &Presentation,
    input: &BodyInput<'_>,
    env: &TextEnv<'_>,
) -> BodyLayout {
    layout_scaled(engine, pres, input, env, None)
}

/// The "Shrink text on overflow" setting a body needs now: the largest font scale (in
/// thousandths of a percent, as `a:normAutofit/@fontScale`) and line reduction at which its text
/// fits. `None` when the body does not shrink.
#[must_use]
pub fn fit_autofit(
    engine: &mut TextEngine,
    pres: &Presentation,
    input: &BodyInput<'_>,
    env: &TextEnv<'_>,
) -> Option<Autofit> {
    if !matches!(input.props.autofit, Autofit::Shrink { .. }) {
        return None;
    }
    if !layout_scaled(engine, pres, input, env, Some((1.0, 0.0))).overflow {
        return Some(Autofit::SHRINK);
    }
    let (mut lo, mut hi) = (MIN_FONT_SCALE, 1.0f32);
    for _ in 0..7 {
        let mid = (lo + hi) / 2.0;
        let red = ((1.0 - mid) * 0.5).min(0.2);
        if layout_scaled(engine, pres, input, env, Some((mid, red))).overflow {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    // PowerPoint stores the scale in steps of 2.5 %.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let scale = ((lo * 40.0).floor() / 40.0).max(MIN_FONT_SCALE);
    let red = ((1.0 - scale) * 0.5).min(0.2);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(Autofit::Shrink {
        font_scale: (scale * 100_000.0).round() as u32,
        line_reduction: ((red * 10.0).round() / 10.0 * 100_000.0).round() as u32,
    })
}

fn layout_scaled(
    engine: &mut TextEngine,
    pres: &Presentation,
    input: &BodyInput<'_>,
    env: &TextEnv<'_>,
    force: Option<(f32, f32)>,
) -> BodyLayout {
    let p = input.props;
    #[allow(clippy::cast_precision_loss)]
    let inset = |i: usize| crate::geom::px(p.insets[i]);
    let r = input.rect;
    let area = Bounds::new(
        r.x + inset(0),
        r.y + inset(1),
        (r.w - inset(0) - inset(2)).max(1.0),
        (r.h - inset(1) - inset(3)).max(1.0),
    );
    let turn = match p.direction {
        TextDirection::Rotate90 | TextDirection::EastAsianVertical => 1,
        TextDirection::Rotate270 | TextDirection::MongolianVertical => 3,
        _ => 0,
    };
    let stacked = matches!(
        p.direction,
        TextDirection::Stacked | TextDirection::StackedRtl
    );
    let paras: Vec<(&Paragraph, ParaInputs)> = input
        .body
        .paragraphs()
        .map(|para| {
            let resolved = pres
                .resolved_paragraph(input.shape, input.body, para)
                .unwrap_or_else(|| ResolvedParagraph {
                    para: para.props.clone(),
                    deck: input.body.deck_props(para.id).cloned().unwrap_or_default(),
                    run: RunProps::default(),
                });
            (
                para,
                ParaInputs {
                    resolved,
                    level: TextBody::level_of(para),
                },
            )
        })
        .collect();
    let (stored_scale, line_reduction) = match p.autofit {
        #[allow(clippy::cast_precision_loss)]
        Autofit::Shrink {
            font_scale,
            line_reduction,
        } => (
            (font_scale as f32 / 100_000.0).clamp(MIN_FONT_SCALE, 1.0),
            (line_reduction as f32 / 100_000.0).clamp(0.0, 0.8),
        ),
        _ => (1.0, 0.0),
    };
    let (scale, reduction) = force.unwrap_or((stored_scale, line_reduction));
    layout_at(
        engine, &paras, area, turn, stacked, p, env, scale, reduction,
    )
}

#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
fn layout_at(
    engine: &mut TextEngine,
    paras: &[(&Paragraph, ParaInputs)],
    area: Bounds,
    turn: u8,
    stacked: bool,
    props: &ResolvedBodyProps,
    env: &TextEnv<'_>,
    scale: f32,
    line_reduction: f32,
) -> BodyLayout {
    let (flow_w, flow_h) = if turn % 2 == 1 {
        (area.h, area.w)
    } else {
        (area.w, area.h)
    };
    let wrap = props.wrap == TextWrap::Square && !stacked;
    let columns = props.columns.max(1);
    let gap = crate::geom::px(props.column_spacing);
    #[allow(clippy::cast_precision_loss)]
    let col_w = if columns > 1 {
        ((flow_w - gap * (columns - 1) as f32) / columns as f32).max(8.0)
    } else {
        flow_w
    };
    let mut laid = Vec::with_capacity(paras.len());
    // Automatic numbers count consecutive paragraphs of the same level and scheme.
    let mut counters: Vec<(usize, String, u32)> = Vec::new();
    for (para, inputs) in paras {
        let res = &inputs.resolved;
        let mut runs: Vec<TextRun> = Vec::new();
        let mut map = OffsetMap::default();
        let mut text_len = 0usize;
        let mut first_size = None;
        let mut highlights = Vec::new();
        let mut links = Vec::new();
        let content = &para.content;
        let objects = content.objects_with_offsets().collect::<Vec<_>>();
        for (range, rprops, text) in content.run_texts() {
            let mut merged = merge_run_props(&res.run, Some(rprops));
            // A table style's text colour and weight sit under the run's own formatting but
            // over the master's text styles.
            if env.fallback_color.is_some() && rprops.color.is_none() {
                merged.color = None;
            }
            if env.fallback_bold && rprops.bold.is_none() {
                merged.bold = None;
            }
            let mut style = span_style(&merged, env, scale);
            let caps = merged.caps == Some(true) || merged.small_caps == Some(true);
            if merged.small_caps == Some(true) {
                style.size *= 0.8;
            }
            if first_size.is_none() {
                first_size = Some(style.size);
            }
            let mut piece = String::new();
            let start_byte = text_len;
            let mut obj_i = objects.iter().position(|(o, _)| *o >= range.start);
            for (k, ch) in text.chars().enumerate() {
                let doc = range.start + k;
                if ch == kiln_doc::OBJECT_CHAR {
                    let obj = obj_i
                        .and_then(|i| objects.get(i))
                        .filter(|(o, _)| *o == doc)
                        .map(|(_, o)| *o);
                    if let Some(i) = obj_i.as_mut() {
                        *i += 1;
                    }
                    let shown = match obj {
                        Some(InlineObject::Break(_)) => "\u{2028}".to_owned(),
                        Some(InlineObject::Field(f)) => field_text(&f.instr, &f.result, env),
                        // Office Math as its linear form.
                        Some(InlineObject::Equation(e)) => e.linear_text(),
                        Some(o) => o.plain_text(),
                        None => String::new(),
                    };
                    let shown = if caps { shown.to_uppercase() } else { shown };
                    map.push(1, shown.len());
                    text_len += shown.len();
                    piece.push_str(&shown);
                    continue;
                }
                let mut s = if caps {
                    ch.to_uppercase().next().unwrap_or(ch).to_string()
                } else {
                    ch.to_string()
                };
                if stacked && !piece.is_empty() {
                    s.insert(0, '\u{2028}');
                    // The break belongs to the char before.
                    map.push(0, '\u{2028}'.len_utf8());
                    text_len += '\u{2028}'.len_utf8();
                    let n = s.len() - '\u{2028}'.len_utf8();
                    map.push(1, n);
                    text_len += n;
                } else {
                    map.push(1, s.len());
                    text_len += s.len();
                }
                piece.push_str(&s);
            }
            if let Some(h) = merged.highlight
                && let Some(c) = crate::text::highlight_rgba(h)
            {
                highlights.push((start_byte..text_len, c));
            }
            runs.push(TextRun::new(piece, style));
        }
        for m in content.marks() {
            if let Mark::Link(h) = &m.mark {
                let target = match &h.target {
                    kiln_doc::LinkTarget::Url(u) => u.clone(),
                    other => format!("{other:?}"),
                };
                links.push((map.byte_of(m.start)..map.byte_of(m.end), target));
            }
        }
        // Hyperlinks take the theme's hyperlink colour and an underline (PowerPoint).
        if !links.is_empty() {
            let hl = color(&DrawingColor::scheme("hlink"), &env.ctx);
            let mut pos = 0usize;
            let mut split: Vec<TextRun> = Vec::new();
            for run in runs.drain(..) {
                let len = run.text.len();
                let mut cuts: Vec<usize> = vec![0, len];
                for (r, _) in &links {
                    for b in [r.start, r.end] {
                        if b > pos && b < pos + len {
                            cuts.push(b - pos);
                        }
                    }
                }
                cuts.sort_unstable();
                cuts.dedup();
                for w in cuts.windows(2) {
                    let (a, b) = (w[0], w[1]);
                    if a >= b || !run.text.is_char_boundary(a) || !run.text.is_char_boundary(b) {
                        continue;
                    }
                    let mut style = run.style.clone();
                    let mid = pos + a;
                    if links.iter().any(|(r, _)| r.start <= mid && mid < r.end) {
                        style.color = hl;
                        if style.underline == UnderlineKind::None {
                            style.underline = UnderlineKind::Single;
                        }
                    }
                    split.push(TextRun::new(&run.text[a..b], style));
                }
                pos += len;
            }
            runs = split;
        }
        let mark_style = {
            let mut merged = merge_run_props(&res.run, Some(&para.mark_props));
            if env.fallback_color.is_some() && para.mark_props.color.is_none() {
                merged.color = None;
            }
            if env.fallback_bold && para.mark_props.bold.is_none() {
                merged.bold = None;
            }
            span_style(&merged, env, scale)
        };
        let size = first_size.unwrap_or(mark_style.size);
        let empty = map.doc_len() == 0;
        let deck = &res.deck;
        let base_spacing = deck.line_spacing.unwrap_or(TextSpacing::Percent(100_000));
        let line_spacing = match base_spacing {
            #[allow(clippy::cast_precision_loss)]
            TextSpacing::Percent(pc) => {
                LineSpacing::Multiple(((pc as f32 / 100_000.0) * (1.0 - line_reduction)).max(0.3))
            }
            #[allow(clippy::cast_precision_loss)]
            TextSpacing::Points(h) => LineSpacing::Exactly(pt(h as f32 / 100.0) * scale),
        };
        let space_before = deck.space_before.map_or(0.0, |s| spacing_px(s, size))
            * if matches!(deck.space_before, Some(TextSpacing::Points(_))) {
                scale
            } else {
                1.0
            };
        let space_after = deck.space_after.map_or(0.0, |s| spacing_px(s, size))
            * if matches!(deck.space_after, Some(TextSpacing::Points(_))) {
                scale
            } else {
                1.0
            };
        #[allow(clippy::cast_precision_loss)]
        let twips = |v: Option<i32>| v.map_or(0.0, |t| t as f32 / 15.0);
        let ind = res.para.indent.unwrap_or_default();
        let mar_l = twips(ind.left);
        let first = twips(ind.first_line) - twips(ind.hanging);
        let alignment = match res.para.alignment {
            Some(DocAlign::Center) => Alignment::Center,
            Some(DocAlign::Right) => Alignment::Right,
            Some(DocAlign::Both) => Alignment::Justify,
            Some(DocAlign::Distribute | DocAlign::ThaiDistribute) => Alignment::Distribute,
            _ => Alignment::Left,
        };
        let tab_stops = res
            .para
            .tabs
            .as_ref()
            .map(|t| {
                t.iter()
                    .map(|s| {
                        #[allow(clippy::cast_precision_loss)]
                        TabStop::new(
                            s.pos as f32 / 15.0,
                            match s.align {
                                kiln_doc::st::TabAlignment::Center => TabAlignment::Center,
                                kiln_doc::st::TabAlignment::Right => TabAlignment::End,
                                kiln_doc::st::TabAlignment::Decimal => TabAlignment::Decimal,
                                _ => TabAlignment::Start,
                            },
                            TabLeader::None,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();
        #[allow(clippy::cast_precision_loss)]
        let default_tab = deck
            .default_tab
            .map_or(96.0, |e| crate::geom::px(e).max(8.0));
        // The bullet.
        let level = inputs.level;
        counters.retain(|(l, _, _)| *l <= level);
        let bullet = match &deck.bullet {
            Some(Bullet::Char(c)) if !empty || env.editing => {
                let font = match &deck.bullet_font {
                    Some(BulletFont::Font(f)) => Some(f.as_str()),
                    _ => None,
                };
                Some((bullet_char(c, font), font.map(str::to_owned)))
            }
            Some(Bullet::Picture(_)) if !empty || env.editing => Some(("•".to_owned(), None)),
            Some(Bullet::AutoNumber { scheme, start_at }) => {
                let n = match counters
                    .iter_mut()
                    .find(|(l, s, _)| *l == level && s == scheme)
                {
                    Some(c) => {
                        c.2 += 1;
                        c.2
                    }
                    None => {
                        counters.retain(|(l, _, _)| *l != level);
                        counters.push((level, scheme.clone(), (*start_at).max(1)));
                        (*start_at).max(1)
                    }
                };
                (!empty || env.editing).then(|| (auto_number(scheme, n), None))
            }
            _ => {
                counters.retain(|(l, _, _)| *l != level);
                None
            }
        };
        let width = if wrap { col_w } else { 100_000.0 };
        let mut pprops = ParagraphProps {
            alignment: if wrap { alignment } else { Alignment::Left },
            indent_start: mar_l,
            indent_end: twips(ind.right),
            first_line: first,
            tab_stops,
            default_tab,
            line_spacing,
            space_before,
            space_after,
            direction: if res.para.bidi == Some(true) {
                Direction::Rtl
            } else {
                Direction::Ltr
            },
            // The paragraph mark sizes an empty paragraph only (DrawingML's `endParaRPr`);
            // a line with text is as tall as its text.
            mark_style: Some(
                runs.last()
                    .filter(|_| !empty)
                    .map_or_else(|| mark_style.clone(), |r| r.style.clone()),
            ),
            ..ParagraphProps::default()
        };
        let bullet = bullet.map(|(label, font)| {
            let mut style = runs
                .first()
                .map_or_else(|| mark_style.clone(), |r| r.style.clone());
            style.underline = UnderlineKind::None;
            style.strike = StrikeKind::None;
            if let Some(f) = font
                && !(f.to_ascii_lowercase().contains("wingdings") || f == "Symbol")
            {
                style.family = f;
            }
            #[allow(clippy::cast_precision_loss)]
            match deck.bullet_size {
                Some(BulletSize::Percent(pc)) => style.size *= pc as f32 / 100_000.0,
                Some(BulletSize::Points(h)) => style.size = pt(h as f32 / 100.0) * scale,
                _ => {}
            }
            if let Some(BulletColor::Color(c)) = &deck.bullet_color {
                style.color = color(c, &env.ctx);
            }
            let spec = ParagraphSpec {
                runs: vec![TextRun::new(label, style)],
                props: ParagraphProps {
                    line_spacing: LineSpacing::Multiple(1.0),
                    ..ParagraphProps::default()
                },
                width: 10_000.0,
            };
            let l = engine.layout(&spec);
            let bw = l.lines.first().map_or(0.0, |ln| ln.width);
            let bx = (mar_l + first).max(0.0);
            // The text starts at the left margin, or after the label when the hanging indent
            // is too small for it (the next default tab, as PowerPoint).
            if bx + bw + 2.0 > mar_l {
                let next = ((bx + bw + 2.0) / default_tab).ceil() * default_tab;
                pprops.first_line = next.max(mar_l) - mar_l;
            } else {
                pprops.first_line = 0.0;
            }
            LaidBullet { layout: l, x: bx }
        });
        let spec = ParagraphSpec {
            runs,
            props: pprops,
            width,
        };
        let layout = engine.layout(&spec);
        laid.push(LaidPara {
            id: para.id,
            layout,
            map,
            bullet,
            highlights,
            links,
            size,
            align: alignment,
        });
    }
    // Lines are placed top to bottom, column by column.
    let mut lines = Vec::new();
    let mut y = 0.0f32;
    let mut col = 0u32;
    let mut col_top = 0.0f32;
    let mut widest = 0.0f32;
    for (pi, p) in laid.iter().enumerate() {
        for (li, line) in p.layout.lines.iter().enumerate() {
            let abs_top = y + line.top;
            if columns > 1
                && col + 1 < columns
                && abs_top + line.height - col_top > flow_h
                && abs_top > col_top
            {
                col += 1;
                col_top = abs_top;
            }
            #[allow(clippy::cast_precision_loss)]
            let mut dx = col as f32 * (col_w + gap);
            if !wrap {
                dx += match p.align {
                    Alignment::Center => (col_w - line.width) / 2.0 - line.x,
                    Alignment::Right => col_w - line.width - line.x,
                    _ => 0.0,
                };
            }
            widest = widest.max(line.x + line.width);
            lines.push(PlacedLine {
                para: pi,
                line: li,
                dx,
                dy: y - col_top,
            });
        }
        y += p.layout.height;
    }
    let content_height = if columns > 1 { flow_h.min(y) } else { y };
    let used = if columns > 1 {
        lines
            .iter()
            .map(|l| {
                let line = &laid[l.para].layout.lines[l.line];
                l.dy + line.top + line.height
            })
            .fold(0.0, f32::max)
    } else {
        y
    };
    let overflow = used > flow_h + 0.5
        || (columns > 1 && y - col_top > flow_h + 0.5 && col + 1 >= columns && used > flow_h);
    // Vertical anchoring.
    let shift = match props.anchor {
        TextAnchor::Middle => (flow_h - used) / 2.0,
        TextAnchor::Bottom => flow_h - used,
        _ => 0.0,
    };
    // PowerPoint lets overflowing text spill below a top-anchored box, and centres it on the box
    // otherwise.
    for l in &mut lines {
        l.dy += shift;
    }
    let content_width = if wrap { flow_w } else { widest };
    if props.anchor_center && !wrap {
        let dx = (flow_w - widest) / 2.0;
        for l in &mut lines {
            l.dx += dx.max(0.0);
        }
    }
    BodyLayout {
        area,
        paras: laid,
        lines,
        font_scale: scale,
        content_height,
        content_width,
        turn,
        overflow,
    }
}

/// `0xRRGGBBAA` of a highlight colour.
#[must_use]
pub fn highlight_rgba(h: kiln_doc::st::HighlightColor) -> Option<u32> {
    use kiln_doc::st::HighlightColor as H;
    Some(match h {
        H::None => return None,
        H::Yellow => 0xffff_00ff,
        H::Green => 0x00ff_00ff,
        H::Cyan => 0x00ff_ffff,
        H::Magenta => 0xff00_ffff,
        H::Blue => 0x0000_ffff,
        H::Red => 0xff00_00ff,
        H::DarkBlue => 0x0000_80ff,
        H::DarkCyan => 0x0080_80ff,
        H::DarkGreen => 0x0080_00ff,
        H::DarkMagenta => 0x8000_80ff,
        H::DarkRed => 0x8000_00ff,
        H::DarkYellow => 0x8080_00ff,
        H::DarkGray => 0x8080_80ff,
        H::LightGray => 0xc0c0_c0ff,
        H::Black => 0x0000_00ff,
        H::White => 0xffff_ffff,
    })
}

/// Draws a laid-out body. `to_scene` maps shape-local px to scene px; `upright` says whether that
/// map is a translation only (glyphs from the atlas) or not (outline glyphs).
pub fn draw_body(
    out: &mut Vec<DrawOp>,
    layout: &BodyLayout,
    fonts: &kiln_text::FontStore,
    glyphs: &mut Glyphs,
    to_scene: Affine,
    upright: bool,
) {
    let m = to_scene * layout.flow_matrix();
    let translate_only = upright && layout.turn == 0;
    let (ox, oy) = {
        let c = m.as_coeffs();
        #[allow(clippy::cast_possible_truncation)]
        (c[4] as f32, c[5] as f32)
    };
    for pl in &layout.lines {
        let p = &layout.paras[pl.para];
        let Some(line) = p.layout.lines.get(pl.line) else {
            continue;
        };
        let (lx, ly) = (pl.dx, pl.dy);
        // Highlights behind the text.
        for (r, c) in &p.highlights {
            let lo = r.start.max(line.text_range.start);
            let hi = r.end.min(line.text_range.end);
            if lo >= hi {
                continue;
            }
            for rect in p.layout.selection_rects(lo..hi) {
                let b = Bounds::new(rect.x + lx, rect.y + ly, rect.width, rect.height);
                push_rect(out, b, *c, m, translate_only, (ox, oy));
            }
        }
        if pl.line == 0
            && let Some(b) = &p.bullet
            && let Some(bl) = b.layout.lines.first()
        {
            // The label sits on the first line's baseline.
            let dy = line.baseline - bl.baseline;
            for run in &bl.runs {
                let origin = (b.x + lx, dy + ly);
                let text = b.layout.text();
                emit_run(
                    out,
                    glyphs,
                    fonts,
                    (run, text),
                    origin,
                    m,
                    translate_only,
                    (ox, oy),
                );
            }
        }
        for run in &line.runs {
            emit_run(
                out,
                glyphs,
                fonts,
                (run, p.layout.text()),
                (lx, ly),
                m,
                translate_only,
                (ox, oy),
            );
        }
        for d in &line.decorations {
            let b = Bounds::new(
                d.x0 + lx,
                d.y + ly,
                (d.x1 - d.x0).max(0.0),
                d.thickness.max(0.5),
            );
            push_rect(out, b, d.color, m, translate_only, (ox, oy));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn emit_run(
    out: &mut Vec<DrawOp>,
    glyphs: &mut Glyphs,
    fonts: &kiln_text::FontStore,
    (run, text): (&kiln_text::GlyphRun, &str),
    origin: (f32, f32),
    m: Affine,
    translate_only: bool,
    offset: (f32, f32),
) {
    if run.glyphs.is_empty() {
        return;
    }
    let paint = Paint::Rgba(run.color);
    if translate_only {
        glyphs.push_text(
            fonts,
            run,
            text,
            (origin.0 + offset.0, origin.1 + offset.1),
            None,
            paint,
            out,
        );
    } else {
        glyphs.push_text(fonts, run, text, origin, Some(m), paint, out);
    }
}

fn push_rect(
    out: &mut Vec<DrawOp>,
    b: Bounds,
    c: u32,
    m: Affine,
    translate_only: bool,
    offset: (f32, f32),
) {
    if translate_only {
        out.push(DrawOp::FillRect {
            rect: b.translate(offset.0, offset.1),
            paint: Paint::Rgba(c),
            radius: 0.0,
        });
    } else {
        let p = crate::geom::transform_path(&kiln_scene::PathData::rect(b), m);
        out.push(DrawOp::Path {
            path: p,
            fill: Some(Paint::Rgba(c)),
            stroke: None,
            rule: kiln_scene::FillRule::NonZero,
        });
    }
}

/// Whether a theme font reference names the heading font.
#[must_use]
pub fn is_major(r: ThemeFontRef) -> bool {
    r.is_major()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_labels() {
        assert_eq!(auto_number("arabicPeriod", 3), "3.");
        assert_eq!(auto_number("alphaLcParenR", 2), "b)");
        assert_eq!(auto_number("romanUcPeriod", 4), "IV.");
        assert_eq!(auto_number("arabicParenBoth", 12), "(12)");
        assert_eq!(auto_number("alphaUcPeriod", 27), "AA.");
    }

    #[test]
    fn offsets_map_fields_to_their_ends() {
        let mut m = OffsetMap::default();
        m.push(1, 1); // "a"
        m.push(1, 2); // a field showing "12"
        m.push(1, 1); // "b"
        assert_eq!(m.byte_of(0), 0);
        assert_eq!(m.byte_of(1), 1);
        assert_eq!(m.byte_of(2), 3);
        assert_eq!(m.byte_of(3), 4);
        assert_eq!(m.doc_of(3), 2);
        assert_eq!(m.doc_of(2), 2);
        assert_eq!(m.doc_of(4), 3);
    }

    #[test]
    fn cached_tints_are_recognised() {
        // Black at tint 75 % renders 404040.
        assert!(is_tint_or_shade(0x0000_00ff, 0x4040_40ff));
        assert!(is_tint_or_shade(0x4472_c4ff, 0x8faa_dcff));
        assert!(!is_tint_or_shade(0x4472_c4ff, 0xc000_00ff));
    }

    #[test]
    fn symbol_bullets_become_unicode() {
        assert_eq!(bullet_char("§", Some("Wingdings")), "■");
        assert_eq!(bullet_char("•", Some("Arial")), "•");
    }
}
