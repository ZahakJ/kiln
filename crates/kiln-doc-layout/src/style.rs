// SPDX-License-Identifier: AGPL-3.0-or-later
//! Resolved Word formatting (`kiln-doc`) to the text engine's styles (`kiln-text`), in px.

use kiln_doc::props::{Border, Shading};
use kiln_doc::resolve::{ResolvedParagraph, ResolvedRun};
use kiln_doc::st::{
    Alignment as WAlign, BorderStyle, HighlightColor, LineRule, TabAlignment as WTab,
    TabLeader as WLeader, UnderlineStyle, VertAlign,
};
use kiln_doc::theme::ColorSchemeMapping;
use kiln_doc::{ColorValue, DocTheme, Settings};
use kiln_text::{
    Alignment, Direction, LineSpacing, ParagraphProps, Rgba, SpanStyle, StrikeKind, TabAlignment,
    TabLeader, TabStop, UnderlineKind,
};

use crate::units::{eighths, half_points, half_points_signed, twips};

/// Which of a run's four fonts a character uses (Word's `w:rFonts` slots).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FontSlot {
    /// ASCII characters.
    Ascii,
    /// Other Latin, Greek, Cyrillic… characters.
    HAnsi,
    /// Chinese, Japanese and Korean.
    EastAsia,
    /// Complex scripts (Arabic, Hebrew, Thai, Indic) and right-to-left runs.
    Complex,
}

/// The font slot of a character in a run (`rtl` runs use the complex-script font for everything
/// except ASCII digits and punctuation, as Word does).
#[must_use]
pub fn font_slot(c: char, rtl: bool) -> FontSlot {
    let u = c as u32;
    if matches!(u, 0x0590..=0x08FF | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF | 0x0900..=0x0DFF | 0x0E00..=0x0EFF | 0x1000..=0x109F | 0x1780..=0x17FF)
    {
        return FontSlot::Complex;
    }
    if matches!(u, 0x1100..=0x11FF | 0x2E80..=0x9FFF | 0xA960..=0xA97F | 0xAC00..=0xD7FF | 0xF900..=0xFAFF | 0xFE30..=0xFE4F | 0xFF00..=0xFFEF | 0x20000..=0x3FFFF)
    {
        return FontSlot::EastAsia;
    }
    if rtl && !c.is_ascii_alphabetic() {
        return FontSlot::Complex;
    }
    if c.is_ascii() {
        FontSlot::Ascii
    } else {
        FontSlot::HAnsi
    }
}

/// A colour value as RGBA, `None` for `auto`.
#[must_use]
pub fn color(value: ColorValue) -> Option<Rgba> {
    value
        .rgb()
        .map(|c| u32::from_be_bytes([c.r, c.g, c.b, 0xFF]))
}

/// The fill of a shading, `None` for none or `auto`.
#[must_use]
pub fn shading_fill(
    shading: Option<&Shading>,
    theme: &DocTheme,
    mapping: &ColorSchemeMapping,
) -> Option<Rgba> {
    kiln_doc::theme::resolve_shading_fill(shading, theme, mapping)
        .map(|c| u32::from_be_bytes([c.r, c.g, c.b, 0xFF]))
}

/// Highlight colour.
#[must_use]
pub fn highlight(h: HighlightColor) -> Option<Rgba> {
    h.rgb().map(|c| u32::from_be_bytes([c.r, c.g, c.b, 0xFF]))
}

/// A resolved border line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rule {
    /// Total width in px (both lines and the gap of a double border).
    pub width: f32,
    /// Colour (`None` = auto, painted in ink).
    pub color: Option<Rgba>,
    /// How the line is drawn.
    pub kind: RuleKind,
    /// Distance from the text in px (`w:space`, points).
    pub space: f32,
    /// Word's conflict weight (width × style number).
    pub weight: f32,
}

/// The drawing of a border line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RuleKind {
    /// One solid line.
    Single,
    /// Two lines with a gap.
    Double,
    /// Dots.
    Dotted,
    /// Dashes.
    Dashed,
}

/// Resolves a border; `None` for `nil`/`none`.
#[must_use]
pub fn rule(border: &Border, theme: &DocTheme, mapping: &ColorSchemeMapping) -> Option<Rule> {
    let kind = match &border.style {
        BorderStyle::Nil | BorderStyle::None => return None,
        BorderStyle::Double
        | BorderStyle::Triple
        | BorderStyle::ThinThickSmallGap
        | BorderStyle::ThickThinSmallGap
        | BorderStyle::ThinThickThinSmallGap
        | BorderStyle::ThinThickMediumGap
        | BorderStyle::ThickThinMediumGap
        | BorderStyle::ThinThickThinMediumGap
        | BorderStyle::ThinThickLargeGap
        | BorderStyle::ThickThinLargeGap
        | BorderStyle::ThinThickThinLargeGap
        | BorderStyle::DoubleWave => RuleKind::Double,
        BorderStyle::Dotted => RuleKind::Dotted,
        BorderStyle::Dashed
        | BorderStyle::DotDash
        | BorderStyle::DotDotDash
        | BorderStyle::DashSmallGap
        | BorderStyle::DashDotStroked => RuleKind::Dashed,
        _ => RuleKind::Single,
    };
    // Sizes are eighths of a point, 2..=96 (Word clamps); default 4 (½ pt).
    let size = border.size.unwrap_or(4).clamp(2, 96);
    let mut width = eighths(size).max(0.5);
    if kind == RuleKind::Double {
        width *= 3.0;
    }
    if matches!(border.style, BorderStyle::Thick) {
        width = width.max(eighths(12));
    }
    let style_number = match kind {
        RuleKind::Single => 1.0,
        RuleKind::Dotted => 0.8,
        RuleKind::Dashed => 0.9,
        RuleKind::Double => 3.0,
    };
    #[allow(clippy::cast_precision_loss)]
    let space = (border.space.unwrap_or(0).min(31) as f32) * 4.0 / 3.0;
    Some(Rule {
        width,
        color: color(kiln_doc::theme::resolve_color(
            border.color.as_ref(),
            theme,
            mapping,
        )),
        kind,
        space,
        weight: eighths(size) * style_number,
    })
}

/// Word's border conflict rule between two adjacent cells: the heavier border wins; on a tie the
/// darker colour wins; otherwise the first (left or upper) one.
#[must_use]
pub fn stronger(a: Option<Rule>, b: Option<Rule>) -> Option<Rule> {
    match (a, b) {
        (None, x) | (x, None) => x,
        (Some(a), Some(b)) => {
            let heavier = b.weight > a.weight + 1e-3;
            let darker =
                (b.weight - a.weight).abs() <= 1e-3 && brightness(b.color) < brightness(a.color);
            if heavier || darker { Some(b) } else { Some(a) }
        }
    }
}

fn brightness(c: Option<Rgba>) -> u32 {
    c.map_or(0, |c| {
        let [r, g, b, _] = c.to_be_bytes();
        u32::from(r) + u32::from(g) + u32::from(b)
    })
}

/// Character formatting for one font slot of a resolved run. `scale` multiplies sizes (Read Mode
/// uses larger type).
#[must_use]
pub fn span_style(run: &ResolvedRun, slot: FontSlot, scale: f32) -> SpanStyle {
    let complex = slot == FontSlot::Complex;
    let family = match slot {
        FontSlot::Ascii => &run.fonts.ascii,
        FontSlot::HAnsi => &run.fonts.h_ansi,
        FontSlot::EastAsia => &run.fonts.east_asia,
        FontSlot::Complex => &run.fonts.cs,
    };
    let family = if family.is_empty() {
        &run.fonts.ascii
    } else {
        family
    };
    let half = if complex { run.size_cs } else { run.size };
    let mut size = half_points(half.clamp(2, 3276)) * scale;
    let bold = if complex { run.bold_cs } else { run.bold };
    let italic = if complex { run.italic_cs } else { run.italic };
    let mut shift = half_points_signed(run.position.clamp(-3168, 3168)) * scale;
    match run.vert_align {
        VertAlign::Superscript => {
            shift += size * 0.33;
            size *= 0.65;
        }
        VertAlign::Subscript => {
            shift -= size * 0.14;
            size *= 0.65;
        }
        VertAlign::Baseline => {}
    }
    let underline = run
        .underline
        .as_ref()
        .map_or(UnderlineKind::None, |u| match u.style {
            UnderlineStyle::None => UnderlineKind::None,
            UnderlineStyle::Words => UnderlineKind::Words,
            UnderlineStyle::Double => UnderlineKind::Double,
            UnderlineStyle::Thick => UnderlineKind::Thick,
            UnderlineStyle::Dotted | UnderlineStyle::DottedHeavy => UnderlineKind::Dotted,
            UnderlineStyle::Dash
            | UnderlineStyle::DashedHeavy
            | UnderlineStyle::DashLong
            | UnderlineStyle::DashLongHeavy
            | UnderlineStyle::DotDash
            | UnderlineStyle::DashDotHeavy
            | UnderlineStyle::DotDotDash
            | UnderlineStyle::DashDotDotHeavy => UnderlineKind::Dashed,
            UnderlineStyle::Wave | UnderlineStyle::WavyHeavy | UnderlineStyle::WavyDouble => {
                UnderlineKind::Wavy
            }
            _ => UnderlineKind::Single,
        });
    let strike = if run.dstrike {
        StrikeKind::Double
    } else if run.strike {
        StrikeKind::Single
    } else {
        StrikeKind::None
    };
    let lang = match slot {
        FontSlot::EastAsia => run.lang.east_asia.clone(),
        FontSlot::Complex => run.lang.bidi.clone(),
        _ => run.lang.val.clone(),
    }
    .or_else(|| run.lang.val.clone());
    let kerning = run.kern.is_some_and(|k| k == 0 || half >= k);
    SpanStyle {
        family: family.clone(),
        size,
        weight: if bold { 700 } else { 400 },
        italic,
        color: color(run.color).unwrap_or(kiln_text::BLACK),
        underline,
        strike,
        letter_spacing: twips(run.spacing.clamp(-31_680, 31_680)) * scale,
        baseline_shift: shift,
        lang,
        rtl: run.rtl,
        kerning,
        ligatures: true,
    }
}

/// Whether a run colour is `auto` (painted in the theme's ink so dark mode keeps it legible).
#[must_use]
pub fn is_auto_color(run: &ResolvedRun) -> bool {
    run.color == ColorValue::Auto
}

/// Paragraph geometry and flow properties in px.
#[derive(Debug, Clone, PartialEq)]
pub struct ParaFormat {
    /// The paragraph style.
    pub style_id: String,
    /// Space above, px.
    pub space_before: f32,
    /// Space below, px.
    pub space_after: f32,
    /// Ignore spacing between paragraphs of the same style.
    pub contextual_spacing: bool,
    /// Keep with next.
    pub keep_next: bool,
    /// Keep lines together.
    pub keep_lines: bool,
    /// Page break before.
    pub page_break_before: bool,
    /// Widow and orphan control.
    pub widow_control: bool,
    /// Left indent (start edge), px.
    pub indent_left: f32,
    /// Right indent (end edge), px.
    pub indent_right: f32,
    /// Outline level 0–8 (9 = body text).
    pub outline_level: u8,
    /// Right-to-left.
    pub bidi: bool,
    /// Borders: top, left, bottom, right, between.
    pub borders: [Option<Rule>; 5],
    /// Background.
    pub shading: Option<Rgba>,
}

impl ParaFormat {
    /// Room the top border takes above the first line (line and its distance).
    #[must_use]
    pub fn border_above(&self) -> f32 {
        self.borders[0].map_or(0.0, |r| r.width + r.space)
    }

    /// Room the bottom border takes below the last line.
    #[must_use]
    pub fn border_below(&self) -> f32 {
        self.borders[2].map_or(0.0, |r| r.width + r.space)
    }

    /// Whether the paragraph draws a box (borders or shading).
    #[must_use]
    pub fn has_box(&self) -> bool {
        self.shading.is_some() || self.borders.iter().any(Option::is_some)
    }
}

/// Converts a resolved paragraph. `default_tab` is the document's default tab interval (twips).
#[must_use]
pub fn paragraph(
    p: &ResolvedParagraph,
    settings: &Settings,
    theme: &DocTheme,
    scale: f32,
) -> (ParagraphProps, ParaFormat) {
    let mapping = &settings.color_scheme_mapping;
    let alignment = match (&p.alignment, p.bidi) {
        (WAlign::Center, _) => Alignment::Center,
        (WAlign::Right, false) | (WAlign::Left, true) => Alignment::End,
        (
            WAlign::Both
            | WAlign::MediumKashida
            | WAlign::HighKashida
            | WAlign::LowKashida
            | WAlign::ThaiDistribute,
            _,
        ) => Alignment::Justify,
        (WAlign::Distribute, _) => Alignment::Distribute,
        _ => Alignment::Start,
    };
    let first_line = if p.indent.hanging > 0 {
        -twips(p.indent.hanging)
    } else {
        twips(p.indent.first_line)
    };
    let line_spacing = match p.spacing.line_rule {
        LineRule::Exact => LineSpacing::Exactly(twips(p.spacing.line.abs().max(1)) * scale),
        LineRule::AtLeast => LineSpacing::AtLeast(twips(p.spacing.line.max(0)) * scale),
        _ => {
            #[allow(clippy::cast_precision_loss)]
            let m = p.spacing.line.clamp(1, 240 * 132) as f32 / 240.0;
            LineSpacing::Multiple(m)
        }
    };
    let tab_stops = p
        .tabs
        .iter()
        .filter_map(|t| {
            let alignment = match t.align {
                WTab::Center => TabAlignment::Center,
                WTab::Right => TabAlignment::End,
                WTab::Decimal => TabAlignment::Decimal,
                WTab::Clear | WTab::Bar => return None,
                _ => TabAlignment::Start,
            };
            let leader = match t.leader.unwrap_or_default() {
                WLeader::Dot => TabLeader::Dot,
                WLeader::Hyphen => TabLeader::Hyphen,
                WLeader::Underscore => TabLeader::Underscore,
                WLeader::Heavy => TabLeader::Heavy,
                WLeader::MiddleDot => TabLeader::MiddleDot,
                _ => TabLeader::None,
            };
            Some(TabStop::new(twips(t.pos) * scale, alignment, leader))
        })
        .collect();
    let autospace = |auto: bool, v: i32| if auto { 14.0 * 4.0 / 3.0 } else { twips(v) };
    let decimal_separator = settings
        .decimal_symbol
        .as_deref()
        .and_then(|s| s.chars().next())
        .unwrap_or('.');
    let props = ParagraphProps {
        alignment,
        indent_start: twips(p.indent.left) * scale,
        indent_end: twips(p.indent.right) * scale,
        first_line: first_line * scale,
        tab_stops,
        default_tab: (twips(settings.default_tab_stop.max(36)) * scale).max(4.0),
        decimal_separator,
        line_spacing,
        // Spacing is applied by the page flow (it collapses at page tops and between
        // paragraphs of the same style with contextual spacing).
        space_before: 0.0,
        space_after: 0.0,
        direction: if p.bidi {
            Direction::Rtl
        } else {
            Direction::Ltr
        },
        hyphenation: settings.auto_hyphenation && !p.suppress_auto_hyphens,
        mark_style: None,
    };
    let b = &p.borders;
    let r = |x: &Option<Border>| x.as_ref().and_then(|x| rule(x, theme, mapping));
    let format = ParaFormat {
        style_id: p.style_id.clone(),
        space_before: autospace(p.spacing.before_autospacing, p.spacing.before.max(0)) * scale,
        space_after: autospace(p.spacing.after_autospacing, p.spacing.after.max(0)) * scale,
        contextual_spacing: p.contextual_spacing,
        keep_next: p.keep_next,
        keep_lines: p.keep_lines,
        page_break_before: p.page_break_before,
        widow_control: p.widow_control,
        indent_left: twips(p.indent.left) * scale,
        indent_right: twips(p.indent.right) * scale,
        outline_level: p.outline_level,
        bidi: p.bidi,
        borders: [
            r(&b.top),
            r(&b.left),
            r(&b.bottom),
            r(&b.right),
            r(&b.between),
        ],
        shading: shading_fill(p.shading.as_ref(), theme, mapping),
    };
    (props, format)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::props::Border;

    #[test]
    fn script_slots() {
        assert_eq!(font_slot('a', false), FontSlot::Ascii);
        assert_eq!(font_slot('é', false), FontSlot::HAnsi);
        assert_eq!(font_slot('ب', false), FontSlot::Complex);
        assert_eq!(font_slot('漢', false), FontSlot::EastAsia);
        assert_eq!(font_slot('.', true), FontSlot::Complex);
        assert_eq!(font_slot('a', true), FontSlot::Ascii);
    }

    #[test]
    fn borders_resolve_and_conflict_like_word() {
        let theme = DocTheme::default();
        let map = ColorSchemeMapping::default();
        assert!(rule(&Border::new(BorderStyle::None, 4), &theme, &map).is_none());
        let thin = rule(&Border::new(BorderStyle::Single, 4), &theme, &map).unwrap();
        let thick = rule(&Border::new(BorderStyle::Single, 24), &theme, &map).unwrap();
        let double = rule(&Border::new(BorderStyle::Double, 4), &theme, &map).unwrap();
        assert!((thin.width - 2.0 / 3.0).abs() < 1e-4);
        assert_eq!(stronger(Some(thin), Some(thick)), Some(thick));
        assert_eq!(stronger(Some(thick), Some(thin)), Some(thick));
        assert_eq!(stronger(Some(thin), Some(double)), Some(double));
        assert_eq!(stronger(None, Some(thin)), Some(thin));
        assert_eq!(stronger(None, None), None);
    }
}
