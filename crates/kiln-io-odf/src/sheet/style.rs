// SPDX-License-Identifier: AGPL-3.0-or-later
//! Cell styles: `style:style style:family="table-cell"` ⇄ [`kiln_sheet::styles::CellStyle`].
//!
//! | Grid | ODF |
//! |---|---|
//! | font name, size, bold, italic, underline, strike, colour, super/subscript, outline, shadow | `style:text-properties` (`style:font-name`/`fo:font-family`, `fo:font-size`, `fo:font-weight`, `fo:font-style`, `style:text-underline-*`, `style:text-line-through-*`, `fo:color`, `style:text-position`, `style:text-outline`, `fo:text-shadow`) |
//! | fill | `fo:background-color` (pattern fills by their main colour, gradients by their first stop) |
//! | borders, diagonals | `fo:border-*`, `style:diagonal-bl-tr`, `style:diagonal-tl-br` |
//! | horizontal alignment, indent, reading order | `fo:text-align` + `style:text-align-source`, `style:repeat-content` (fill), `fo:margin-left` (9 pt per level), `style:writing-mode` |
//! | vertical alignment, wrap, shrink, rotation | `style:vertical-align`, `fo:wrap-option`, `style:shrink-to-fit`, `style:rotation-angle`, `style:direction="ttb"` (stacked) |
//! | protection | `style:cell-protect` |
//! | number format | `style:data-style-name` ([`crate::numfmt`]) |
//!
//! Border styles map by line style and width: hair 0.25 pt, thin 0.75 pt, medium 1.75 pt,
//! thick 2.5 pt, double 2.25 pt (`double`), dashed and dotted at thin and medium widths,
//! dash-dot as `dot-dash`, dash-dot-dot as `dot-dot-dash`.

use std::sync::Arc;

use kiln_ooxml::xml::XmlElement;
use kiln_sheet::Theme;
use kiln_sheet::color::Color;
use kiln_sheet::num::F64;
use kiln_sheet::styles::{
    Alignment, Border, BorderSide, BorderStyle, CellStyle, Fill, Font, HAlign, Protection,
    Underline, VAlign, VertAlign,
};

use crate::el::El;
use crate::props::{FontFaces, unquote};
use crate::units::{color, color_str, parse_inches, pt_str};
use crate::xml::kid;

/// Points per indent level.
const INDENT_PT: f64 = 9.0;

/// A length in points, exact for `pt` values (row heights and font sizes round-trip).
#[must_use]
pub fn points_exact(s: &str) -> Option<f64> {
    let t = s.trim();
    if let Some(n) = t.strip_suffix("pt") {
        return n.trim().parse::<f64>().ok().filter(|v| v.is_finite());
    }
    parse_inches(t).map(|i| (i * 72.0 * 1e6).round() / 1e6)
}

fn rgb_color(s: &str) -> Option<Color> {
    let c = color(s)?;
    Some(Color::rgb(
        (u32::from(c.r) << 16) | (u32::from(c.g) << 8) | u32::from(c.b),
    ))
}

/// A Grid colour as `#rrggbb`.
#[must_use]
pub fn color_hex(c: &Color, theme: &Theme, auto: u32) -> String {
    let v = c.resolve(theme, auto);
    format!("#{:06x}", v & 0xFF_FFFF)
}

fn border_side(s: &str) -> Option<BorderSide> {
    let mut width = None;
    let mut style = "solid";
    let mut col = None;
    for tok in s.split_whitespace() {
        if tok == "none" || tok == "hidden" {
            return None;
        }
        if let Some(c) = rgb_color(tok) {
            col = Some(c);
        } else if let Some(p) = points_exact(tok) {
            width = Some(p);
        } else {
            style = match tok {
                "solid" | "dotted" | "dashed" | "double" | "dot-dash" | "dot-dot-dash"
                | "fine-dashed" | "dash-dot" | "dash-dot-dot" => tok,
                _ => style,
            };
        }
    }
    let w = width.unwrap_or(0.75);
    let medium = w >= 1.25;
    let kind = match style {
        "double" => BorderStyle::Double,
        "dotted" => BorderStyle::Dotted,
        "dashed" | "fine-dashed" => {
            if medium {
                BorderStyle::MediumDashed
            } else {
                BorderStyle::Dashed
            }
        }
        "dot-dash" | "dash-dot" => {
            if medium {
                BorderStyle::MediumDashDot
            } else {
                BorderStyle::DashDot
            }
        }
        "dot-dot-dash" | "dash-dot-dot" => {
            if medium {
                BorderStyle::MediumDashDotDot
            } else {
                BorderStyle::DashDotDot
            }
        }
        _ if w < 0.5 => BorderStyle::Hair,
        _ if w < 1.25 => BorderStyle::Thin,
        _ if w < 2.2 => BorderStyle::Medium,
        _ => BorderStyle::Thick,
    };
    Some(BorderSide {
        style: kind,
        color: col,
    })
}

fn border_str(b: &BorderSide, theme: &Theme) -> Option<String> {
    let (w, s) = match b.style {
        BorderStyle::None => return None,
        BorderStyle::Hair => (0.25, "solid"),
        BorderStyle::Thin => (0.75, "solid"),
        BorderStyle::Medium => (1.75, "solid"),
        BorderStyle::Thick => (2.5, "solid"),
        BorderStyle::Double => (2.25, "double"),
        BorderStyle::Dashed => (0.75, "dashed"),
        BorderStyle::MediumDashed => (1.75, "dashed"),
        BorderStyle::Dotted => (0.75, "dotted"),
        BorderStyle::DashDot => (0.75, "dot-dash"),
        BorderStyle::MediumDashDot | BorderStyle::SlantDashDot => (1.75, "dot-dash"),
        BorderStyle::DashDotDot => (0.75, "dot-dot-dash"),
        BorderStyle::MediumDashDotDot => (1.75, "dot-dot-dash"),
    };
    let c = b
        .color
        .map_or_else(|| "#000000".to_owned(), |c| color_hex(&c, theme, 0));
    Some(format!("{} {s} {c}", pt_str(w)))
}

/// Applies one style element's properties to a cell style (styles apply root first).
pub fn apply_style(cs: &mut CellStyle, style: &XmlElement, fonts: &FontFaces) {
    // Paragraph alignment first: the cell properties' alignment source can override it.
    if let Some(pp) = kid(style, "style:paragraph-properties") {
        if let Some(a) = pp.attr("fo:text-align") {
            cs.alignment.horizontal = match a {
                "center" => HAlign::Center,
                "end" | "right" => HAlign::Right,
                "justify" => HAlign::Justify,
                _ => HAlign::Left,
            };
        }
        if let Some(m) = pp.attr("fo:margin-left").and_then(points_exact) {
            let level = (m / INDENT_PT).round().clamp(0.0, 250.0);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                cs.alignment.indent = level as u8;
            }
        }
        match pp.attr("style:writing-mode") {
            Some("lr-tb") => cs.alignment.reading_order = 1,
            Some("rl-tb") => cs.alignment.reading_order = 2,
            _ => {}
        }
    }
    if let Some(tc) = kid(style, "style:table-cell-properties") {
        apply_cell_props(cs, tc);
    }
    if let Some(tp) = kid(style, "style:text-properties") {
        apply_text_props(&mut cs.font, tp, fonts);
    }
}

fn apply_cell_props(cs: &mut CellStyle, tc: &XmlElement) {
    if let Some(bg) = tc.attr("fo:background-color") {
        cs.fill = match rgb_color(bg) {
            Some(c) => Fill::solid(c),
            None => Fill::NONE,
        };
    }
    if let Some(b) = tc.attr("fo:border") {
        let side = border_side(b);
        cs.border.left = side;
        cs.border.right = side;
        cs.border.top = side;
        cs.border.bottom = side;
    }
    for (attr, slot) in [
        ("fo:border-left", 0),
        ("fo:border-right", 1),
        ("fo:border-top", 2),
        ("fo:border-bottom", 3),
    ] {
        if let Some(b) = tc.attr(attr) {
            let side = border_side(b);
            match slot {
                0 => cs.border.left = side,
                1 => cs.border.right = side,
                2 => cs.border.top = side,
                _ => cs.border.bottom = side,
            }
        }
    }
    let up = tc.attr("style:diagonal-bl-tr").and_then(border_side);
    let down = tc.attr("style:diagonal-tl-br").and_then(border_side);
    if tc.attr("style:diagonal-bl-tr").is_some() || tc.attr("style:diagonal-tl-br").is_some() {
        cs.border.diagonal_up = up.is_some();
        cs.border.diagonal_down = down.is_some();
        cs.border.diagonal = up.or(down);
    }
    if let Some(v) = tc.attr("style:vertical-align") {
        cs.alignment.vertical = match v {
            "top" => VAlign::Top,
            "middle" => VAlign::Center,
            _ => VAlign::Bottom,
        };
    }
    if let Some(w) = tc.attr("fo:wrap-option") {
        cs.alignment.wrap_text = w == "wrap";
    }
    if let Some(s) = tc.attr("style:shrink-to-fit") {
        cs.alignment.shrink_to_fit = s == "true";
    }
    if tc.attr("style:text-align-source") == Some("value-type") {
        cs.alignment.horizontal = HAlign::General;
    }
    if tc.attr("style:repeat-content") == Some("true") {
        cs.alignment.horizontal = HAlign::Fill;
    }
    if tc.attr("style:direction") == Some("ttb") {
        cs.alignment.text_rotation = 255;
    } else if let Some(a) = tc
        .attr("style:rotation-angle")
        .and_then(crate::units::degrees)
    {
        let a = a.rem_euclid(360.0).round();
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let v = if a <= 90.0 {
            a as u8
        } else if a >= 270.0 {
            (90.0 + (360.0 - a)) as u8
        } else {
            0
        };
        cs.alignment.text_rotation = v;
    }
    if let Some(p) = tc.attr("style:cell-protect") {
        cs.protection = Protection {
            locked: p.contains("protected") && p != "none",
            hidden: p.contains("formula-hidden") || p.contains("hidden-and-protected"),
        };
        if p == "hidden-and-protected" {
            cs.protection.locked = true;
        }
    }
}

fn apply_text_props(f: &mut Font, tp: &XmlElement, fonts: &FontFaces) {
    if let Some(n) = tp.attr("style:font-name") {
        f.name = Arc::from(fonts.family(n).as_str());
    } else if let Some(n) = tp.attr("fo:font-family") {
        f.name = Arc::from(unquote(n).as_str());
    }
    if let Some(s) = tp.attr("fo:font-size").and_then(points_exact) {
        f.size = F64(s);
    }
    if let Some(w) = tp.attr("fo:font-weight") {
        f.bold = matches!(w, "bold" | "600" | "700" | "800" | "900");
    }
    if let Some(s) = tp.attr("fo:font-style") {
        f.italic = s == "italic" || s == "oblique";
    }
    if let Some(u) = tp.attr("style:text-underline-style") {
        f.underline = if u == "none" {
            Underline::None
        } else if tp.attr("style:text-underline-type") == Some("double") {
            Underline::Double
        } else {
            Underline::Single
        };
    }
    if let Some(s) = tp.attr("style:text-line-through-style") {
        f.strike = s != "none";
    }
    if tp.attr("style:use-window-font-color") == Some("true") {
        f.color = None;
    }
    if let Some(c) = tp.attr("fo:color").and_then(rgb_color)
        && tp.attr("style:use-window-font-color") != Some("true")
    {
        f.color = Some(c);
    }
    if let Some(p) = tp.attr("style:text-position") {
        let first = p.split_whitespace().next().unwrap_or_default();
        f.vert_align = match first {
            "super" => VertAlign::Superscript,
            "sub" => VertAlign::Subscript,
            v => match v.trim_end_matches('%').parse::<f64>() {
                Ok(x) if x > 0.0 => VertAlign::Superscript,
                Ok(x) if x < 0.0 => VertAlign::Subscript,
                _ => VertAlign::Baseline,
            },
        };
    }
    if let Some(o) = tp.attr("style:text-outline") {
        f.outline = o == "true";
    }
    if let Some(s) = tp.attr("fo:text-shadow") {
        f.shadow = s != "none";
    }
}

/// The text properties of a font as `style:text-properties` (`base` = the default font: only
/// what differs is written when `diff` is set).
#[must_use]
pub fn text_props_el(f: &Font, base: Option<&Font>, theme: &Theme) -> El {
    let mut e = El::new("style:text-properties");
    let differs = |pick: &dyn Fn(&Font) -> String| base.is_none_or(|b| pick(b) != pick(f));
    if differs(&|x| x.name.to_string()) {
        e.set("style:font-name", f.name.to_string());
        e.set("fo:font-family", font_family_value(&f.name));
    }
    if differs(&|x| x.size.0.to_string()) {
        let s = pt_str(f.size.0);
        e.set("fo:font-size", s.clone());
        e.set("style:font-size-asian", s.clone());
        e.set("style:font-size-complex", s);
    }
    if differs(&|x| x.bold.to_string()) {
        let w = if f.bold { "bold" } else { "normal" };
        e.set("fo:font-weight", w);
        e.set("style:font-weight-asian", w);
        e.set("style:font-weight-complex", w);
    }
    if differs(&|x| x.italic.to_string()) {
        let s = if f.italic { "italic" } else { "normal" };
        e.set("fo:font-style", s);
        e.set("style:font-style-asian", s);
        e.set("style:font-style-complex", s);
    }
    if differs(&|x| format!("{:?}", x.underline)) {
        match f.underline {
            Underline::None => e.set("style:text-underline-style", "none"),
            Underline::Single | Underline::SingleAccounting => {
                e.set("style:text-underline-style", "solid");
                e.set("style:text-underline-width", "auto");
                e.set("style:text-underline-color", "font-color");
            }
            Underline::Double | Underline::DoubleAccounting => {
                e.set("style:text-underline-style", "solid");
                e.set("style:text-underline-type", "double");
                e.set("style:text-underline-width", "auto");
                e.set("style:text-underline-color", "font-color");
            }
        }
    }
    if differs(&|x| x.strike.to_string()) {
        if f.strike {
            e.set("style:text-line-through-style", "solid");
            e.set("style:text-line-through-type", "single");
        } else {
            e.set("style:text-line-through-style", "none");
        }
    }
    if differs(&|x| format!("{:?}", x.color)) {
        match &f.color {
            Some(c) if !matches!(c, Color::Auto) => e.set("fo:color", color_hex(c, theme, 0)),
            _ => e.set("style:use-window-font-color", "true"),
        }
    }
    if differs(&|x| format!("{:?}", x.vert_align)) {
        e.set(
            "style:text-position",
            match f.vert_align {
                VertAlign::Superscript => "super 58%",
                VertAlign::Subscript => "sub 58%",
                VertAlign::Baseline => "0% 100%",
            },
        );
    }
    if differs(&|x| x.outline.to_string()) {
        e.set("style:text-outline", f.outline.to_string());
    }
    if differs(&|x| x.shadow.to_string()) {
        e.set("fo:text-shadow", if f.shadow { "1pt 1pt" } else { "none" });
    }
    e
}

/// `fo:font-family` for a family name (quoted when it has spaces).
#[must_use]
pub fn font_family_value(name: &str) -> String {
    if name.contains(' ') {
        format!("'{name}'")
    } else {
        name.to_owned()
    }
}

/// The `style:table-cell-properties` and `style:paragraph-properties` of a cell style.
#[must_use]
pub fn cell_props_els(cs: &CellStyle, theme: &Theme) -> (El, El) {
    let mut tc = El::new("style:table-cell-properties");
    let mut pp = El::new("style:paragraph-properties");
    match fill_rgb(&cs.fill, theme) {
        Some(c) => tc.set("fo:background-color", format!("#{c:06x}")),
        None => tc.set("fo:background-color", "transparent"),
    }
    let b: &Border = &cs.border;
    let side = |s: &Option<BorderSide>| s.as_ref().and_then(|s| border_str(s, theme));
    let (l, r, t, bo) = (side(&b.left), side(&b.right), side(&b.top), side(&b.bottom));
    if l.is_some() && l == r && l == t && l == bo {
        tc.set("fo:border", l.unwrap_or_default());
    } else {
        for (name, v) in [
            ("fo:border-left", l),
            ("fo:border-right", r),
            ("fo:border-top", t),
            ("fo:border-bottom", bo),
        ] {
            tc.set(name, v.unwrap_or_else(|| "none".to_owned()));
        }
    }
    let diag = side(&b.diagonal);
    tc.set(
        "style:diagonal-bl-tr",
        diag.clone()
            .filter(|_| b.diagonal_up)
            .unwrap_or_else(|| "none".to_owned()),
    );
    tc.set(
        "style:diagonal-tl-br",
        diag.filter(|_| b.diagonal_down)
            .unwrap_or_else(|| "none".to_owned()),
    );
    let a: &Alignment = &cs.alignment;
    tc.set(
        "style:vertical-align",
        match a.vertical {
            VAlign::Top => "top",
            VAlign::Center | VAlign::Justify | VAlign::Distributed => "middle",
            VAlign::Bottom => "bottom",
        },
    );
    tc.set(
        "fo:wrap-option",
        if a.wrap_text { "wrap" } else { "no-wrap" },
    );
    tc.set("style:shrink-to-fit", a.shrink_to_fit.to_string());
    match a.horizontal {
        HAlign::General => {
            tc.set("style:text-align-source", "value-type");
        }
        h => {
            tc.set("style:text-align-source", "fix");
            if h == HAlign::Fill {
                tc.set("style:repeat-content", "true");
            }
            pp.set(
                "fo:text-align",
                match h {
                    HAlign::Center | HAlign::CenterContinuous => "center",
                    HAlign::Right => "end",
                    HAlign::Justify | HAlign::Distributed => "justify",
                    _ => "start",
                },
            );
        }
    }
    if a.horizontal != HAlign::Fill {
        tc.set("style:repeat-content", "false");
    }
    match a.text_rotation {
        255 => tc.set("style:direction", "ttb"),
        r => {
            let deg = if r <= 90 {
                u32::from(r)
            } else {
                360 - (u32::from(r) - 90)
            };
            tc.set("style:rotation-angle", (deg % 360).to_string());
        }
    }
    tc.set(
        "style:cell-protect",
        match (cs.protection.locked, cs.protection.hidden) {
            (true, false) => "protected",
            (false, false) => "none",
            (true, true) => "protected formula-hidden",
            (false, true) => "formula-hidden",
        },
    );
    if a.indent > 0 {
        pp.set("fo:margin-left", pt_str(f64::from(a.indent) * INDENT_PT));
    }
    match a.reading_order {
        1 => pp.set("style:writing-mode", "lr-tb"),
        2 => pp.set("style:writing-mode", "rl-tb"),
        _ => {}
    }
    (tc, pp)
}

/// The one colour a fill shows as in ODF: solid fills as they are, patterns as their colours
/// blended by the pattern's density, gradients as the midpoint of their end stops.
#[must_use]
pub fn fill_rgb(fill: &Fill, theme: &Theme) -> Option<u32> {
    use kiln_sheet::color::mix;
    use kiln_sheet::styles::PatternType as P;
    match fill {
        Fill::Pattern {
            pattern: P::None, ..
        } => None,
        Fill::Pattern {
            pattern: P::Solid,
            fg,
            bg,
        } => fg.or(*bg).map(|c| c.resolve(theme, 0)),
        Fill::Pattern { pattern, fg, bg } => {
            let density = match pattern {
                P::Gray0625 => 0.0625,
                P::Gray125 => 0.125,
                P::LightGray
                | P::LightHorizontal
                | P::LightVertical
                | P::LightDown
                | P::LightUp
                | P::LightGrid
                | P::LightTrellis => 0.25,
                P::MediumGray | P::DarkHorizontal | P::DarkVertical | P::DarkDown | P::DarkUp => {
                    0.5
                }
                P::DarkGrid | P::DarkTrellis => 0.5,
                P::DarkGray => 0.75,
                P::None | P::Solid => 1.0,
            };
            let f = fg.map_or(0, |c| c.resolve(theme, 0));
            let b = bg.map_or(0xFF_FFFF, |c| c.resolve(theme, 0xFF_FFFF));
            Some(mix(b, f, density))
        }
        Fill::Gradient(g) => {
            let first = g.stops.first()?.color.resolve(theme, 0xFF_FFFF);
            let last = g.stops.last()?.color.resolve(theme, 0xFF_FFFF);
            Some(mix(first, last, 0.5))
        }
    }
}

/// The resolved properties of the style chain `chain` (root first) over `base`.
#[must_use]
pub fn resolve_chain(base: &CellStyle, chain: &[&XmlElement], fonts: &FontFaces) -> CellStyle {
    let mut cs = base.clone();
    for s in chain {
        apply_style(&mut cs, s, fonts);
    }
    cs.border = cs.border.normalized();
    cs.fill = cs.fill.clone().normalized();
    cs
}

/// A colour for `fo:background-color`, or `None` for `transparent`.
#[must_use]
pub fn background(s: &str) -> Option<Color> {
    rgb_color(s)
}

/// `#rrggbb` of a kiln-common colour (for chart and drawing code).
#[must_use]
pub fn hex_of(c: kiln_common::Color) -> String {
    color_str(c)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse_part;

    fn round_trip(cs: &CellStyle) -> CellStyle {
        let theme = Theme::default();
        let (tc, pp) = cell_props_els(cs, &theme);
        let tp = text_props_el(&cs.font, None, &theme);
        let mut st = El::new("style:style");
        st.children = vec![tc, pp, tp];
        let mut w = crate::xml::XmlOut::new();
        w.root("office:styles", &["office", "style", "fo", "loext"]);
        st.write(&mut w);
        let root = parse_part("styles.xml", &w.finish()).unwrap();
        let el = root.elements().next().unwrap();
        let mut base = CellStyle::default();
        base.font.scheme = kiln_sheet::styles::FontScheme::None;
        base.font.family = None;
        resolve_chain(&base, &[el], &FontFaces::default())
    }

    #[test]
    fn styles_round_trip() {
        let mut cs = CellStyle::default();
        cs.font.scheme = kiln_sheet::styles::FontScheme::None;
        cs.font.family = None;
        cs.font.color = Some(Color::rgb(0xc0_0000));
        cs.font.bold = true;
        cs.font.size = F64(10.5);
        cs.font.underline = Underline::Double;
        cs.font.vert_align = VertAlign::Superscript;
        cs.fill = Fill::solid(Color::rgb(0xff_ff00));
        cs.border.left = Some(BorderSide::THIN);
        cs.border.bottom = Some(BorderSide {
            style: BorderStyle::Double,
            color: Some(Color::rgb(0x12_3456)),
        });
        cs.border.diagonal = Some(BorderSide {
            style: BorderStyle::MediumDashDot,
            color: Some(Color::rgb(0)),
        });
        cs.border.diagonal_up = true;
        cs.alignment.horizontal = HAlign::Right;
        cs.alignment.vertical = VAlign::Top;
        cs.alignment.wrap_text = true;
        cs.alignment.indent = 2;
        cs.alignment.text_rotation = 135;
        cs.protection.locked = false;
        let back = round_trip(&cs);
        assert_eq!(back.font, cs.font);
        assert_eq!(back.fill, cs.fill);
        assert_eq!(back.border.left.map(|b| b.style), Some(BorderStyle::Thin));
        assert_eq!(back.border.bottom, cs.border.bottom);
        assert_eq!(back.border.diagonal, cs.border.diagonal);
        assert!(back.border.diagonal_up && !back.border.diagonal_down);
        assert_eq!(back.alignment, cs.alignment);
        assert_eq!(back.protection, cs.protection);
        for style in [
            BorderStyle::Hair,
            BorderStyle::Thin,
            BorderStyle::Medium,
            BorderStyle::Thick,
            BorderStyle::Double,
            BorderStyle::Dashed,
            BorderStyle::MediumDashed,
            BorderStyle::Dotted,
            BorderStyle::DashDot,
            BorderStyle::MediumDashDot,
            BorderStyle::DashDotDot,
            BorderStyle::MediumDashDotDot,
        ] {
            let side = BorderSide {
                style,
                color: Some(Color::rgb(0x11_2233)),
            };
            let s = border_str(&side, &Theme::default()).unwrap();
            assert_eq!(border_side(&s), Some(side), "{s}");
        }
    }
}
