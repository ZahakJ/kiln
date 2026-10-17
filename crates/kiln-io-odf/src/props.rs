// SPDX-License-Identifier: AGPL-3.0-or-later
//! The mapping tables between ODF formatting properties (`style:text-properties`,
//! `style:paragraph-properties`, borders, colours) and kiln-doc's OOXML-shaped [`RunProps`] and
//! [`ParagraphProps`]. Shared by text documents, presentation text and spreadsheet rich text.
//!
//! | ODF | Model |
//! |---|---|
//! | `fo:font-weight`, `style:font-weight-complex` | bold, bold (complex scripts) |
//! | `fo:font-style`, `style:font-style-complex` | italic, italic (complex scripts) |
//! | `style:text-underline-style/-type/-width/-color/-mode` | underline style and colour (single, words, double, thick, dotted, dash, long dash, dot-dash, dot-dot-dash, wave, each heavy) |
//! | `style:text-line-through-style/-type` | strike, double strike |
//! | `fo:text-transform="uppercase"`, `fo:font-variant="small-caps"` | all caps, small caps |
//! | `text:display="none"` | hidden |
//! | `style:font-relief`, `style:text-outline`, `fo:text-shadow` | emboss/engrave, outline, shadow |
//! | `fo:color`, `style:use-window-font-color` | colour, automatic colour |
//! | `fo:background-color` | highlight when it is one of Word's 16 highlight colours, else shading |
//! | `fo:font-size`, `style:font-size-complex` | size, complex size (half-points) |
//! | `style:font-name[-asian|-complex]`, `fo:font-family` | ASCII/high-ANSI, East Asian, complex fonts |
//! | `style:text-position` | superscript, subscript |
//! | `fo:letter-spacing`, `style:text-scale` | character spacing, scale |
//! | `fo:language`/`fo:country` (and Asian, complex) | languages |
//! | `fo:border` on text | character border |
//! | `fo:text-align`, margins, `fo:text-indent` | alignment, indents (first line or hanging) |
//! | `fo:margin-top/-bottom`, `fo:line-height`, `style:line-height-at-least` | spacing before/after, line spacing (multiple, exact, at least) |
//! | `fo:keep-with-next`, `fo:keep-together`, `fo:break-before="page"`, `fo:widows`/`fo:orphans` | keep with next, keep lines, page break before, widow control |
//! | `style:contextual-spacing`, `style:writing-mode`, `style:vertical-align` | contextual spacing, right-to-left, text alignment |
//! | `style:tab-stops` | tab stops with alignment and leader |
//! | `fo:border*`, `fo:padding*`, `fo:background-color` | paragraph borders with spacing, shading |

use std::collections::HashMap;

use kiln_common::Color;
use kiln_doc::props::{
    Border, ColorValue, DocColor, DropCap, Indent, Lang, ParagraphBorders, ParagraphProps,
    RunFonts, RunProps, Shading, Spacing, TabStop, Underline,
};
use kiln_doc::st::{
    Alignment, BorderStyle, DropCapKind, HighlightColor, LineRule, ShadingPattern, TabAlignment,
    TabLeader, TextAlignment, UnderlineStyle, VertAlign,
};
use kiln_doc::theme::{ColorSchemeMapping, DocTheme, resolve_color, resolve_run_fonts};
use kiln_ooxml::xml::XmlElement;

use crate::el::El;
use crate::units::{
    color, color_str, half_points, half_points_str, percent, points, pt_str, twips, twips_str,
};
use crate::xml::{kid, kids};

// ------------------------------------------------------------------ fonts

/// `office:font-face-decls`: font face names to font family names.
#[derive(Debug, Clone, Default)]
pub struct FontFaces(HashMap<String, String>);

impl FontFaces {
    /// Collects the declarations of several parts (`content.xml`, `styles.xml`).
    pub fn add_decls(&mut self, root: &XmlElement) {
        if let Some(decls) = kid(root, "office:font-face-decls") {
            for f in kids(decls, "style:font-face") {
                if let Some(name) = f.attr("style:name") {
                    let family = f
                        .attr("svg:font-family")
                        .map(unquote)
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| name.to_owned());
                    self.0.entry(name.to_owned()).or_insert(family);
                }
            }
        }
    }

    /// The family a font face name stands for (the name itself when undeclared).
    #[must_use]
    pub fn family(&self, name: &str) -> String {
        self.0.get(name).cloned().unwrap_or_else(|| name.to_owned())
    }
}

/// `'Liberation Sans'` → `Liberation Sans` (the first family of a CSS list).
#[must_use]
pub fn unquote(s: &str) -> String {
    let first = s.split(',').next().unwrap_or(s).trim();
    first.trim_matches(|c| c == '\'' || c == '"').to_owned()
}

// ------------------------------------------------------------------ colours and borders

fn bool_attr(v: &str) -> bool {
    v == "true"
}

/// A model colour as RGB (theme references resolved; `auto` gives `None`).
#[must_use]
pub fn rgb_of(c: Option<&DocColor>, theme: &DocTheme) -> Option<Color> {
    resolve_color(c, theme, &ColorSchemeMapping::default()).rgb()
}

/// Parses `0.5pt solid #000000` (any order) into a model border.
#[must_use]
pub fn parse_border(s: &str) -> Option<Border> {
    let s = s.trim();
    if s.is_empty() || s == "none" || s == "hidden" {
        return Some(Border {
            style: BorderStyle::None,
            ..Border::default()
        });
    }
    let mut width = None;
    let mut style = BorderStyle::Single;
    let mut col = None;
    for tok in s.split_whitespace() {
        if let Some(c) = color(tok) {
            col = Some(c);
        } else if let Some(p) = points(tok) {
            width = Some(p);
        } else {
            style = match tok {
                "solid" => BorderStyle::Single,
                "double" | "double-thin" => BorderStyle::Double,
                "dotted" => BorderStyle::Dotted,
                "dashed" | "fine-dashed" => BorderStyle::Dashed,
                "dash-dot" => BorderStyle::DotDash,
                "dash-dot-dot" => BorderStyle::DotDotDash,
                "groove" => BorderStyle::ThreeDEngrave,
                "ridge" => BorderStyle::ThreeDEmboss,
                "inset" => BorderStyle::Inset,
                "outset" => BorderStyle::Outset,
                "none" | "hidden" => BorderStyle::None,
                "thin" | "medium" | "thick" => {
                    width = Some(match tok {
                        "thin" => 0.5,
                        "medium" => 1.0,
                        _ => 2.25,
                    });
                    style
                }
                _ => style,
            };
        }
    }
    let size = width.map(|w| {
        // Eighths of a point, clamped to what Word accepts (2–96).
        let e = (w * 8.0).round().clamp(2.0, 96.0);
        // In range by the clamp.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let e = e as u32;
        e
    });
    Some(Border {
        style,
        size,
        space: None,
        color: col.map(|c| DocColor {
            val: ColorValue::Rgb(c),
            ..DocColor::default()
        }),
        shadow: None,
        frame: None,
    })
}

/// A model border as `0.5pt solid #000000` (`none` for no border).
#[must_use]
pub fn border_str(b: &Border, theme: &DocTheme) -> String {
    if b.style.is_none() {
        return "none".to_owned();
    }
    let style = match &b.style {
        BorderStyle::Double
        | BorderStyle::Triple
        | BorderStyle::DoubleWave
        | BorderStyle::ThinThickSmallGap
        | BorderStyle::ThickThinSmallGap
        | BorderStyle::ThinThickThinSmallGap
        | BorderStyle::ThinThickMediumGap
        | BorderStyle::ThickThinMediumGap
        | BorderStyle::ThinThickThinMediumGap
        | BorderStyle::ThinThickLargeGap
        | BorderStyle::ThickThinLargeGap
        | BorderStyle::ThinThickThinLargeGap => "double",
        BorderStyle::Dotted => "dotted",
        BorderStyle::Dashed | BorderStyle::DashSmallGap => "dashed",
        BorderStyle::DotDash | BorderStyle::DashDotStroked => "dash-dot",
        BorderStyle::DotDotDash => "dash-dot-dot",
        BorderStyle::ThreeDEngrave => "groove",
        BorderStyle::ThreeDEmboss => "ridge",
        BorderStyle::Inset => "inset",
        BorderStyle::Outset => "outset",
        _ => "solid",
    };
    let width = f64::from(b.size.unwrap_or(4)) / 8.0;
    let c = rgb_of(b.color.as_ref(), theme).unwrap_or(Color::rgb(0, 0, 0));
    format!("{} {style} {}", pt_str(width), color_str(c))
}

fn doc_color(c: Color) -> DocColor {
    DocColor {
        val: ColorValue::Rgb(c),
        ..DocColor::default()
    }
}

/// `fo:background-color` as shading (`transparent` = none).
fn shading_of(v: &str) -> Option<Shading> {
    color(v).map(|c| Shading {
        pattern: ShadingPattern::Clear,
        color: None,
        fill: Some(doc_color(c)),
    })
}

/// The colour a shading shows (fill, or the pattern colour of a solid pattern).
#[must_use]
pub fn shading_rgb(s: &Shading, theme: &DocTheme) -> Option<Color> {
    if s.pattern == ShadingPattern::Solid {
        rgb_of(s.color.as_ref(), theme).or_else(|| rgb_of(s.fill.as_ref(), theme))
    } else {
        rgb_of(s.fill.as_ref(), theme)
    }
}

// ------------------------------------------------------------------ run properties

fn underline_of(style: &str, kind: Option<&str>, width: Option<&str>, words: bool) -> UnderlineStyle {
    let heavy = matches!(width, Some("bold" | "thick" | "medium"));
    let double = kind == Some("double");
    match style {
        "none" => UnderlineStyle::None,
        "dotted" if heavy => UnderlineStyle::DottedHeavy,
        "dotted" => UnderlineStyle::Dotted,
        "dash" if heavy => UnderlineStyle::DashedHeavy,
        "dash" => UnderlineStyle::Dash,
        "long-dash" if heavy => UnderlineStyle::DashLongHeavy,
        "long-dash" => UnderlineStyle::DashLong,
        "dot-dash" if heavy => UnderlineStyle::DashDotHeavy,
        "dot-dash" => UnderlineStyle::DotDash,
        "dot-dot-dash" if heavy => UnderlineStyle::DashDotDotHeavy,
        "dot-dot-dash" => UnderlineStyle::DotDotDash,
        "wave" if double => UnderlineStyle::WavyDouble,
        "wave" if heavy => UnderlineStyle::WavyHeavy,
        "wave" => UnderlineStyle::Wave,
        _ if double => UnderlineStyle::Double,
        _ if heavy => UnderlineStyle::Thick,
        _ if words => UnderlineStyle::Words,
        _ => UnderlineStyle::Single,
    }
}

/// `(style, type, width, mode)` of an underline style.
fn underline_attrs(u: UnderlineStyle) -> (&'static str, &'static str, &'static str, bool) {
    use UnderlineStyle as U;
    match u {
        U::None => ("none", "none", "auto", false),
        U::Single => ("solid", "single", "auto", false),
        U::Words => ("solid", "single", "auto", true),
        U::Double => ("solid", "double", "auto", false),
        U::Thick => ("solid", "single", "bold", false),
        U::Dotted => ("dotted", "single", "auto", false),
        U::DottedHeavy => ("dotted", "single", "bold", false),
        U::Dash => ("dash", "single", "auto", false),
        U::DashedHeavy => ("dash", "single", "bold", false),
        U::DashLong => ("long-dash", "single", "auto", false),
        U::DashLongHeavy => ("long-dash", "single", "bold", false),
        U::DotDash => ("dot-dash", "single", "auto", false),
        U::DashDotHeavy => ("dot-dash", "single", "bold", false),
        U::DotDotDash => ("dot-dot-dash", "single", "auto", false),
        U::DashDotDotHeavy => ("dot-dot-dash", "single", "bold", false),
        U::Wave => ("wave", "single", "auto", false),
        U::WavyHeavy => ("wave", "single", "bold", false),
        U::WavyDouble => ("wave", "double", "auto", false),
    }
}

fn lang_of(lang: Option<&str>, country: Option<&str>) -> Option<String> {
    let l = lang.filter(|l| !l.is_empty() && *l != "none" && *l != "zxx")?;
    Some(match country.filter(|c| !c.is_empty() && *c != "none") {
        Some(c) => format!("{l}-{c}"),
        None => l.to_owned(),
    })
}

fn split_lang(tag: &str) -> (String, Option<String>) {
    let mut it = tag.splitn(2, '-');
    let l = it.next().unwrap_or_default().to_owned();
    (l, it.next().map(str::to_owned))
}

fn bold_of(v: &str) -> bool {
    match v {
        "bold" | "bolder" => true,
        "normal" | "lighter" => false,
        n => n.parse::<u32>().is_ok_and(|w| w >= 600),
    }
}

/// Reads `style:text-properties` into run properties.
#[must_use]
pub fn read_run_props(tp: &XmlElement, fonts: &FontFaces) -> RunProps {
    let mut r = RunProps::default();
    let a = |n: &str| tp.attr(n);
    if let Some(v) = a("fo:font-weight") {
        r.bold = Some(bold_of(v));
    }
    if let Some(v) = a("style:font-weight-complex") {
        r.bold_cs = Some(bold_of(v));
    }
    if let Some(v) = a("fo:font-style") {
        r.italic = Some(v == "italic" || v == "oblique");
    }
    if let Some(v) = a("style:font-style-complex") {
        r.italic_cs = Some(v == "italic" || v == "oblique");
    }
    if let Some(style) = a("style:text-underline-style") {
        let u = underline_of(
            style,
            a("style:text-underline-type"),
            a("style:text-underline-width"),
            a("style:text-underline-mode") == Some("skip-white-space"),
        );
        let col = a("style:text-underline-color").and_then(color).map(doc_color);
        r.underline = Some(Underline {
            style: u,
            color: col,
        });
    }
    if let Some(style) = a("style:text-line-through-style") {
        if style == "none" {
            r.strike = Some(false);
        } else if a("style:text-line-through-type") == Some("double") {
            r.dstrike = Some(true);
        } else {
            r.strike = Some(true);
        }
    }
    if let Some(v) = a("fo:text-transform") {
        r.caps = Some(v == "uppercase");
    }
    if let Some(v) = a("fo:font-variant") {
        r.small_caps = Some(v == "small-caps");
    }
    if let Some(v) = a("text:display") {
        r.hidden = Some(v == "none");
    }
    if let Some(v) = a("style:font-relief") {
        r.emboss = Some(v == "embossed");
        r.imprint = Some(v == "engraved");
    }
    if let Some(v) = a("style:text-outline") {
        r.outline = Some(bool_attr(v));
    }
    if let Some(v) = a("fo:text-shadow") {
        r.shadow = Some(v != "none");
    }
    if a("style:use-window-font-color") == Some("true") {
        r.color = Some(DocColor::auto());
    } else if let Some(c) = a("fo:color").and_then(color) {
        r.color = Some(doc_color(c));
    }
    if let Some(v) = a("fo:background-color") {
        match color(v) {
            Some(c) => {
                match HighlightColor::ALL
                    .iter()
                    .find(|h| h.rgb() == Some(c))
                    .copied()
                {
                    Some(h) => r.highlight = Some(h),
                    None => r.shading = shading_of(v),
                }
            }
            None if v == "transparent" => r.highlight = Some(HighlightColor::None),
            None => {}
        }
    }
    if let Some(v) = a("fo:font-size") {
        r.size = half_points(v);
    }
    if let Some(v) = a("style:font-size-complex") {
        r.size_cs = half_points(v);
    }
    let family = |name_attr: &str, family_attr: &str| -> Option<String> {
        a(name_attr)
            .map(|n| fonts.family(n))
            .or_else(|| a(family_attr).map(unquote))
            .filter(|f| !f.is_empty())
    };
    let latin = family("style:font-name", "fo:font-family");
    let asian = family("style:font-name-asian", "style:font-family-asian");
    let complex = family("style:font-name-complex", "style:font-family-complex");
    if latin.is_some() || asian.is_some() || complex.is_some() {
        r.fonts = Some(RunFonts {
            ascii: latin.clone(),
            h_ansi: latin,
            east_asia: asian,
            cs: complex,
            ..RunFonts::default()
        });
    }
    if let Some(v) = a("style:text-position") {
        let first = v.split_whitespace().next().unwrap_or_default();
        let shift = match first {
            "super" => 33.0,
            "sub" => -33.0,
            other => percent(other).unwrap_or(0.0),
        };
        let scale = v
            .split_whitespace()
            .nth(1)
            .and_then(percent)
            .unwrap_or(100.0);
        if shift > 0.0 && scale < 100.0 {
            r.vert_align = Some(VertAlign::Superscript);
        } else if shift < 0.0 && scale < 100.0 {
            r.vert_align = Some(VertAlign::Subscript);
        } else if shift == 0.0 {
            r.vert_align = Some(VertAlign::Baseline);
        }
    }
    if let Some(v) = a("fo:letter-spacing") {
        r.spacing = if v == "normal" { Some(0) } else { twips(v) };
    }
    if let Some(v) = a("style:text-scale").and_then(percent) {
        let s = v.round().clamp(1.0, 600.0);
        // In range by the clamp.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let s = s as u32;
        r.scale = Some(s);
    }
    let latin_lang = lang_of(a("fo:language"), a("fo:country"));
    let asian_lang = lang_of(a("style:language-asian"), a("style:country-asian"));
    let complex_lang = lang_of(a("style:language-complex"), a("style:country-complex"));
    if latin_lang.is_some() || asian_lang.is_some() || complex_lang.is_some() {
        r.lang = Some(Lang {
            val: latin_lang,
            east_asia: asian_lang,
            bidi: complex_lang,
        });
    }
    if let Some(b) = a("fo:border").and_then(parse_border) {
        r.border = Some(b);
    }
    r
}

/// Writes run properties as `style:text-properties` attributes.
#[must_use]
pub fn run_props_el(r: &RunProps, theme: &DocTheme) -> El {
    let mut e = El::new("style:text-properties");
    let onoff = |v: bool, on: &str, off: &str| if v { on.to_owned() } else { off.to_owned() };
    if let Some(f) = &r.fonts {
        let resolved = resolve_run_fonts(Some(f), theme);
        let latin = if f.ascii.is_some() || f.ascii_theme.is_some() {
            Some(resolved.ascii)
        } else if f.h_ansi.is_some() || f.h_ansi_theme.is_some() {
            Some(resolved.h_ansi)
        } else {
            None
        };
        if let Some(l) = latin.filter(|l| !l.is_empty()) {
            e.set("style:font-name", l);
        }
        if (f.east_asia.is_some() || f.east_asia_theme.is_some()) && !resolved.east_asia.is_empty() {
            e.set("style:font-name-asian", resolved.east_asia);
        }
        if (f.cs.is_some() || f.cs_theme.is_some()) && !resolved.cs.is_empty() {
            e.set("style:font-name-complex", resolved.cs);
        }
    }
    if let Some(s) = r.size {
        e.set("fo:font-size", half_points_str(s));
        e.set("style:font-size-asian", half_points_str(s));
    }
    if let Some(s) = r.size_cs {
        e.set("style:font-size-complex", half_points_str(s));
    }
    if let Some(b) = r.bold {
        e.set("fo:font-weight", onoff(b, "bold", "normal"));
        e.set("style:font-weight-asian", onoff(b, "bold", "normal"));
    }
    if let Some(b) = r.bold_cs {
        e.set("style:font-weight-complex", onoff(b, "bold", "normal"));
    }
    if let Some(b) = r.italic {
        e.set("fo:font-style", onoff(b, "italic", "normal"));
        e.set("style:font-style-asian", onoff(b, "italic", "normal"));
    }
    if let Some(b) = r.italic_cs {
        e.set("style:font-style-complex", onoff(b, "italic", "normal"));
    }
    if let Some(u) = &r.underline {
        let (style, kind, width, words) = underline_attrs(u.style);
        e.set("style:text-underline-style", style);
        if style != "none" {
            e.set("style:text-underline-type", kind);
            e.set("style:text-underline-width", width);
            e.set(
                "style:text-underline-color",
                rgb_of(u.color.as_ref(), theme).map_or_else(|| "font-color".to_owned(), color_str),
            );
            if words {
                e.set("style:text-underline-mode", "skip-white-space");
            }
        }
    }
    if r.dstrike == Some(true) {
        e.set("style:text-line-through-style", "solid");
        e.set("style:text-line-through-type", "double");
    } else if let Some(s) = r.strike {
        e.set("style:text-line-through-style", onoff(s, "solid", "none"));
        if s {
            e.set("style:text-line-through-type", "single");
        }
    }
    if let Some(c) = r.caps {
        e.set("fo:text-transform", onoff(c, "uppercase", "none"));
    }
    if let Some(c) = r.small_caps {
        e.set("fo:font-variant", onoff(c, "small-caps", "normal"));
    }
    if let Some(h) = r.hidden {
        e.set("text:display", onoff(h, "none", "true"));
    }
    if r.emboss == Some(true) {
        e.set("style:font-relief", "embossed");
    } else if r.imprint == Some(true) {
        e.set("style:font-relief", "engraved");
    } else if r.emboss == Some(false) || r.imprint == Some(false) {
        e.set("style:font-relief", "none");
    }
    if let Some(o) = r.outline {
        e.set("style:text-outline", onoff(o, "true", "false"));
    }
    if let Some(s) = r.shadow {
        e.set("fo:text-shadow", onoff(s, "1pt 1pt", "none"));
    }
    if let Some(c) = &r.color {
        match rgb_of(Some(c), theme) {
            Some(rgb) => e.set("fo:color", color_str(rgb)),
            None => e.set("style:use-window-font-color", "true"),
        }
    }
    if let Some(h) = r.highlight {
        match h.rgb() {
            Some(c) => e.set("fo:background-color", color_str(c)),
            None => e.set("fo:background-color", "transparent"),
        }
    } else if let Some(s) = &r.shading
        && let Some(c) = shading_rgb(s, theme)
    {
        e.set("fo:background-color", color_str(c));
    }
    if let Some(v) = r.vert_align {
        e.set(
            "style:text-position",
            match v {
                VertAlign::Superscript => "super 58%",
                VertAlign::Subscript => "sub 58%",
                VertAlign::Baseline => "0% 100%",
            },
        );
    }
    if let Some(s) = r.spacing {
        e.set(
            "fo:letter-spacing",
            if s == 0 {
                "normal".to_owned()
            } else {
                twips_str(i64::from(s))
            },
        );
    }
    if let Some(s) = r.scale {
        e.set("style:text-scale", format!("{s}%"));
    }
    if let Some(l) = &r.lang {
        let mut put = |tag: &Option<String>, lang: &str, country: &str| {
            if let Some(t) = tag {
                let (l, c) = split_lang(t);
                e.set(lang, l);
                e.set(country, c.unwrap_or_else(|| "none".to_owned()));
            }
        };
        put(&l.val, "fo:language", "fo:country");
        put(&l.east_asia, "style:language-asian", "style:country-asian");
        put(&l.bidi, "style:language-complex", "style:country-complex");
    }
    if let Some(b) = &r.border {
        e.set("fo:border", border_str(b, theme));
    }
    e
}

// ------------------------------------------------------------------ paragraph properties

fn alignment_of(v: &str) -> Option<Alignment> {
    Some(match v {
        "start" | "left" => Alignment::Left,
        "end" | "right" => Alignment::Right,
        "center" => Alignment::Center,
        "justify" => Alignment::Both,
        _ => return None,
    })
}

/// Reads `style:paragraph-properties` into paragraph properties (tab positions as written).
#[must_use]
pub fn read_para_props(pp: &XmlElement) -> ParagraphProps {
    let mut p = ParagraphProps::default();
    let a = |n: &str| pp.attr(n);
    if let Some(al) = a("fo:text-align").and_then(alignment_of) {
        p.alignment = Some(al);
    }
    let mut indent = Indent::default();
    let mut any_indent = false;
    if let Some(v) = a("fo:margin-left").or_else(|| a("fo:margin-start")).and_then(twips) {
        indent.left = Some(v);
        any_indent = true;
    }
    if let Some(v) = a("fo:margin-right").or_else(|| a("fo:margin-end")).and_then(twips) {
        indent.right = Some(v);
        any_indent = true;
    }
    if let Some(v) = a("fo:text-indent").and_then(twips) {
        if v < 0 {
            indent.hanging = Some(-v);
        } else {
            indent.first_line = Some(v);
        }
        any_indent = true;
    }
    if any_indent {
        p.indent = Some(indent);
    }
    let mut spacing = Spacing::default();
    let mut any_spacing = false;
    if let Some(v) = a("fo:margin-top").and_then(twips) {
        spacing.before = Some(v);
        any_spacing = true;
    }
    if let Some(v) = a("fo:margin-bottom").and_then(twips) {
        spacing.after = Some(v);
        any_spacing = true;
    }
    if let Some(v) = a("fo:line-height") {
        if let Some(pc) = percent(v) {
            let line = (pc * 2.4).round();
            if line.is_finite() && line.abs() < 1.0e6 {
                // In range by the check above.
                #[allow(clippy::cast_possible_truncation)]
                let line = line as i32;
                spacing.line = Some(line);
                spacing.line_rule = Some(LineRule::Auto);
                any_spacing = true;
            }
        } else if let Some(tw) = twips(v) {
            spacing.line = Some(tw);
            spacing.line_rule = Some(LineRule::Exact);
            any_spacing = true;
        }
    } else if let Some(tw) = a("style:line-height-at-least").and_then(twips) {
        spacing.line = Some(tw);
        spacing.line_rule = Some(LineRule::AtLeast);
        any_spacing = true;
    }
    if any_spacing {
        p.spacing = Some(spacing);
    }
    if let Some(v) = a("fo:keep-with-next") {
        p.keep_next = Some(v == "always");
    }
    if let Some(v) = a("fo:keep-together") {
        p.keep_lines = Some(v == "always");
    }
    match a("fo:break-before") {
        Some("page") => p.page_break_before = Some(true),
        Some("auto") => p.page_break_before = Some(false),
        _ => {}
    }
    if let Some(v) = a("fo:widows").or_else(|| a("fo:orphans")) {
        p.widow_control = Some(v.trim() != "0");
    }
    if let Some(v) = a("style:contextual-spacing") {
        p.contextual_spacing = Some(bool_attr(v));
    }
    if let Some(v) = a("style:writing-mode") {
        match v {
            "rl-tb" | "rl" => p.bidi = Some(true),
            "lr-tb" | "lr" => p.bidi = Some(false),
            _ => {}
        }
    }
    if let Some(v) = a("style:vertical-align") {
        p.text_alignment = match v {
            "top" => Some(TextAlignment::Top),
            "middle" => Some(TextAlignment::Center),
            "bottom" => Some(TextAlignment::Bottom),
            "baseline" => Some(TextAlignment::Baseline),
            "auto" => Some(TextAlignment::Auto),
            _ => None,
        };
    }
    if let Some(v) = a("fo:background-color") {
        p.shading = shading_of(v);
    }
    if let Some(v) = a("text:number-lines") {
        p.suppress_line_numbers = Some(v == "false");
    }
    if let Some(v) = a("style:snap-to-layout-grid") {
        p.snap_to_grid = Some(bool_attr(v));
    }
    let side = |name: &str| a(name).or_else(|| a("fo:border")).and_then(parse_border);
    let pad = |name: &str| {
        a(name)
            .or_else(|| a("fo:padding"))
            .and_then(points)
            .map(|v| {
                let v = v.round().clamp(0.0, 31.0);
                // In range by the clamp.
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                let v = v as u32;
                v
            })
    };
    let mut borders = ParagraphBorders::default();
    let mut any_border = false;
    for (slot, b, p_) in [
        (&mut borders.top, "fo:border-top", "fo:padding-top"),
        (&mut borders.bottom, "fo:border-bottom", "fo:padding-bottom"),
        (&mut borders.left, "fo:border-left", "fo:padding-left"),
        (&mut borders.right, "fo:border-right", "fo:padding-right"),
    ] {
        if let Some(mut border) = side(b).filter(|b| !b.style.is_none()) {
            border.space = pad(p_);
            *slot = Some(border);
            any_border = true;
        }
    }
    if any_border {
        p.borders = Some(borders);
    }
    if let Some(ts) = kid(pp, "style:tab-stops") {
        let tabs: Vec<TabStop> = kids(ts, "style:tab-stop")
            .filter_map(|t| {
                let pos = t.attr("style:position").and_then(twips)?;
                let align = match t.attr("style:type").unwrap_or("left") {
                    "center" => TabAlignment::Center,
                    "right" => TabAlignment::Right,
                    "char" => TabAlignment::Decimal,
                    _ => TabAlignment::Left,
                };
                let leader = match (t.attr("style:leader-text"), t.attr("style:leader-style")) {
                    (Some("."), _) | (_, Some("dotted")) => Some(TabLeader::Dot),
                    (Some("-"), _) | (_, Some("dash" | "long-dash")) => Some(TabLeader::Hyphen),
                    (Some("_"), _) | (_, Some("solid")) => Some(TabLeader::Underscore),
                    (Some("·"), _) => Some(TabLeader::MiddleDot),
                    _ => None,
                };
                Some(TabStop { pos, align, leader })
            })
            .collect();
        p.tabs = Some(tabs);
    }
    if let Some(dc) = kid(pp, "style:drop-cap")
        && let Some(lines) = dc
            .attr("style:lines")
            .and_then(|l| l.parse::<u8>().ok())
            .filter(|l| *l > 1)
    {
        p.drop_cap = Some(DropCap {
            kind: DropCapKind::Drop,
            lines,
        });
    }
    p
}

/// Writes paragraph properties as `style:paragraph-properties` (with tab stops). Numbering,
/// style and outline level are written by the caller (they are not formatting in ODF).
#[must_use]
pub fn para_props_el(p: &ParagraphProps, theme: &DocTheme) -> El {
    let mut e = El::new("style:paragraph-properties");
    if let Some(al) = p.alignment {
        e.set(
            "fo:text-align",
            match al {
                Alignment::Left => "start",
                Alignment::Right => "end",
                Alignment::Center => "center",
                _ => "justify",
            },
        );
    }
    if let Some(i) = &p.indent {
        if let Some(l) = i.left {
            e.set("fo:margin-left", twips_str(i64::from(l)));
        }
        if let Some(r) = i.right {
            e.set("fo:margin-right", twips_str(i64::from(r)));
        }
        if let Some(h) = i.hanging {
            e.set("fo:text-indent", twips_str(-i64::from(h)));
        } else if let Some(f) = i.first_line {
            e.set("fo:text-indent", twips_str(i64::from(f)));
        }
    }
    if let Some(s) = &p.spacing {
        if let Some(b) = s.before {
            e.set("fo:margin-top", twips_str(i64::from(b)));
        }
        if let Some(a) = s.after {
            e.set("fo:margin-bottom", twips_str(i64::from(a)));
        }
        if let Some(line) = s.line {
            match s.line_rule.unwrap_or(LineRule::Auto) {
                LineRule::Auto => {
                    let pc = f64::from(line) / 2.4;
                    let mut t = format!("{pc:.2}");
                    while t.ends_with('0') {
                        t.pop();
                    }
                    if t.ends_with('.') {
                        t.pop();
                    }
                    e.set("fo:line-height", format!("{t}%"));
                }
                LineRule::Exact => e.set("fo:line-height", twips_str(i64::from(line))),
                LineRule::AtLeast => {
                    e.set("style:line-height-at-least", twips_str(i64::from(line)));
                }
            }
        }
    }
    if let Some(k) = p.keep_next {
        e.set("fo:keep-with-next", if k { "always" } else { "auto" });
    }
    if let Some(k) = p.keep_lines {
        e.set("fo:keep-together", if k { "always" } else { "auto" });
    }
    if let Some(b) = p.page_break_before {
        e.set("fo:break-before", if b { "page" } else { "auto" });
    }
    if let Some(w) = p.widow_control {
        let n = if w { "2" } else { "0" };
        e.set("fo:orphans", n);
        e.set("fo:widows", n);
    }
    if let Some(c) = p.contextual_spacing {
        e.set("style:contextual-spacing", if c { "true" } else { "false" });
    }
    if let Some(b) = p.bidi {
        e.set("style:writing-mode", if b { "rl-tb" } else { "lr-tb" });
    }
    if let Some(t) = p.text_alignment {
        e.set(
            "style:vertical-align",
            match t {
                TextAlignment::Top => "top",
                TextAlignment::Center => "middle",
                TextAlignment::Bottom => "bottom",
                TextAlignment::Baseline => "baseline",
                TextAlignment::Auto => "auto",
            },
        );
    }
    if let Some(s) = &p.shading
        && let Some(c) = shading_rgb(s, theme)
    {
        e.set("fo:background-color", color_str(c));
    }
    if let Some(s) = p.suppress_line_numbers {
        e.set("text:number-lines", if s { "false" } else { "true" });
    }
    if let Some(s) = p.snap_to_grid {
        e.set("style:snap-to-layout-grid", if s { "true" } else { "false" });
    }
    if let Some(b) = &p.borders {
        for (slot, name, pad) in [
            (&b.top, "fo:border-top", "fo:padding-top"),
            (&b.bottom, "fo:border-bottom", "fo:padding-bottom"),
            (&b.left, "fo:border-left", "fo:padding-left"),
            (&b.right, "fo:border-right", "fo:padding-right"),
        ] {
            if let Some(border) = slot {
                e.set(name, border_str(border, theme));
                if let Some(sp) = border.space {
                    e.set(pad, pt_str(f64::from(sp)));
                }
            }
        }
    }
    if let Some(tabs) = &p.tabs {
        let mut ts = El::new("style:tab-stops");
        for t in tabs {
            if matches!(t.align, TabAlignment::Clear | TabAlignment::Bar) {
                continue;
            }
            let mut stop = El::new("style:tab-stop").with("style:position", twips_str(i64::from(t.pos)));
            match t.align {
                TabAlignment::Center => stop.set("style:type", "center"),
                TabAlignment::Right => stop.set("style:type", "right"),
                TabAlignment::Decimal => {
                    stop.set("style:type", "char");
                    stop.set("style:char", ".");
                }
                _ => {}
            }
            match t.leader {
                Some(TabLeader::Dot) => {
                    stop.set("style:leader-style", "dotted");
                    stop.set("style:leader-text", ".");
                }
                Some(TabLeader::Hyphen) => {
                    stop.set("style:leader-style", "dash");
                    stop.set("style:leader-text", "-");
                }
                Some(TabLeader::Underscore | TabLeader::Heavy) => {
                    stop.set("style:leader-style", "solid");
                    stop.set("style:leader-text", "_");
                }
                Some(TabLeader::MiddleDot) => {
                    stop.set("style:leader-style", "dotted");
                    stop.set("style:leader-text", "·");
                }
                _ => {}
            }
            ts.children.push(stop);
        }
        // An empty list clears inherited stops; ODF writes that as an empty element.
        e.children.push(ts);
    }
    if let Some(dc) = &p.drop_cap {
        e.children.push(
            El::new("style:drop-cap")
                .with("style:lines", dc.lines.to_string())
                .with("style:length", "1"),
        );
    }
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::xml::XmlNode;

    fn to_xml(e: &El) -> XmlElement {
        let mut x = XmlElement::new(&e.name);
        for (k, v) in &e.attrs {
            x.attrs.set(k.clone(), v.clone());
        }
        for c in &e.children {
            x.children.push(XmlNode::Element(to_xml(c)));
        }
        x
    }

    #[test]
    fn run_props_round_trip() {
        let theme = DocTheme::office();
        let r = RunProps {
            bold: Some(true),
            italic: Some(false),
            underline: Some(Underline {
                style: UnderlineStyle::DashLongHeavy,
                color: Some(DocColor::rgb(0xff0000)),
            }),
            strike: Some(true),
            caps: Some(true),
            small_caps: Some(false),
            hidden: Some(true),
            color: Some(DocColor::rgb(0x112233)),
            highlight: Some(HighlightColor::Yellow),
            size: Some(21),
            size_cs: Some(24),
            vert_align: Some(VertAlign::Superscript),
            spacing: Some(20),
            scale: Some(150),
            fonts: Some(RunFonts {
                ascii: Some("Liberation Serif".into()),
                h_ansi: Some("Liberation Serif".into()),
                east_asia: Some("Noto Sans CJK".into()),
                cs: Some("Amiri".into()),
                ..RunFonts::default()
            }),
            lang: Some(Lang {
                val: Some("en-GB".into()),
                east_asia: Some("ja-JP".into()),
                bidi: Some("ar".into()),
            }),
            ..RunProps::default()
        };
        let el = to_xml(&run_props_el(&r, &theme));
        let back = read_run_props(&el, &FontFaces::default());
        assert_eq!(back, r);
    }

    #[test]
    fn every_underline_round_trips() {
        for u in UnderlineStyle::ALL {
            let (s, t, w, words) = underline_attrs(*u);
            assert_eq!(underline_of(s, Some(t), Some(w), words), *u, "{u:?}");
        }
    }

    #[test]
    fn para_props_round_trip() {
        let theme = DocTheme::office();
        let p = ParagraphProps {
            alignment: Some(Alignment::Both),
            indent: Some(Indent {
                left: Some(720),
                right: Some(360),
                first_line: None,
                hanging: Some(360),
            }),
            spacing: Some(Spacing {
                before: Some(120),
                after: Some(240),
                line: Some(276),
                line_rule: Some(LineRule::Auto),
                ..Spacing::default()
            }),
            keep_next: Some(true),
            keep_lines: Some(false),
            page_break_before: Some(true),
            widow_control: Some(false),
            contextual_spacing: Some(true),
            bidi: Some(true),
            tabs: Some(vec![
                TabStop {
                    pos: 4320,
                    align: TabAlignment::Right,
                    leader: Some(TabLeader::Dot),
                },
                TabStop {
                    pos: 1440,
                    align: TabAlignment::Center,
                    leader: None,
                },
            ]),
            borders: Some(ParagraphBorders {
                bottom: Some(Border {
                    style: BorderStyle::Single,
                    size: Some(6),
                    space: Some(1),
                    color: Some(DocColor::rgb(0x4f81bd)),
                    shadow: None,
                    frame: None,
                }),
                ..ParagraphBorders::default()
            }),
            shading: Some(Shading {
                pattern: ShadingPattern::Clear,
                color: None,
                fill: Some(DocColor::rgb(0xeeeeee)),
            }),
            ..ParagraphProps::default()
        };
        let el = to_xml(&para_props_el(&p, &theme));
        assert_eq!(read_para_props(&el), p);
    }

    #[test]
    fn borders_parse() {
        let b = parse_border("0.5pt solid #000000").unwrap();
        assert_eq!(b.style, BorderStyle::Single);
        assert_eq!(b.size, Some(4));
        assert_eq!(parse_border("none").unwrap().style, BorderStyle::None);
        assert_eq!(
            border_str(&b, &DocTheme::office()),
            "0.5pt solid #000000"
        );
    }
}
