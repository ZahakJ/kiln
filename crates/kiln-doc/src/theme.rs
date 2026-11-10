// SPDX-License-Identifier: AGPL-3.0-or-later
//! The document theme as the Write model needs it, and theme colour and font resolution.
//!
//! Word stores theme references (`w:asciiTheme="minorHAnsi"`, `w:themeColor="accent1"
//! w:themeShade="BF"`) alongside cached values; the theme reference wins. The full DrawingML theme
//! (`word/theme/theme1.xml`) is parsed by `kiln-ooxml`; [`DocTheme::from_ooxml`] keeps the twelve
//! colour slots and the two font collections, which is everything resolution reads.

use kiln_common::Color;
use kiln_ooxml::drawingml::color::{hsl_to_rgb, rgb_to_hsl};
use kiln_ooxml::drawingml::theme::{Theme, theme_color_table};
use serde::{Deserialize, Serialize};

use crate::props::{ColorValue, DocColor, RunFonts, Shading};
use crate::st::{ThemeColorName, ThemeFontRef};

/// The twelve colour slots of `a:clrScheme`, in schema order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ThemeSlot {
    /// `dk1`.
    Dk1,
    /// `lt1`.
    Lt1,
    /// `dk2`.
    Dk2,
    /// `lt2`.
    Lt2,
    /// `accent1`.
    Accent1,
    /// `accent2`.
    Accent2,
    /// `accent3`.
    Accent3,
    /// `accent4`.
    Accent4,
    /// `accent5`.
    Accent5,
    /// `accent6`.
    Accent6,
    /// `hlink`.
    Hlink,
    /// `folHlink`.
    FolHlink,
}

impl ThemeSlot {
    /// The index into [`DocTheme::colors`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// The DrawingML spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dk1 => "dk1",
            Self::Lt1 => "lt1",
            Self::Dk2 => "dk2",
            Self::Lt2 => "lt2",
            Self::Accent1 => "accent1",
            Self::Accent2 => "accent2",
            Self::Accent3 => "accent3",
            Self::Accent4 => "accent4",
            Self::Accent5 => "accent5",
            Self::Accent6 => "accent6",
            Self::Hlink => "hlink",
            Self::FolHlink => "folHlink",
        }
    }
}

/// `w:clrSchemeMapping` (settings.xml): which slots the text/background aliases use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColorSchemeMapping {
    /// `bg1`.
    pub bg1: ThemeColorName,
    /// `t1`.
    pub t1: ThemeColorName,
    /// `bg2`.
    pub bg2: ThemeColorName,
    /// `t2`.
    pub t2: ThemeColorName,
}

impl Default for ColorSchemeMapping {
    /// Word's default: `bg1 → light1`, `t1 → dark1`, `bg2 → light2`, `t2 → dark2`.
    fn default() -> Self {
        Self {
            bg1: ThemeColorName::Light1,
            t1: ThemeColorName::Dark1,
            bg2: ThemeColorName::Light2,
            t2: ThemeColorName::Dark2,
        }
    }
}

/// Maps a `w:themeColor` value to its theme slot through the scheme mapping (`text1 → dk1`).
#[must_use]
pub fn theme_slot_for(name: ThemeColorName, mapping: &ColorSchemeMapping) -> Option<ThemeSlot> {
    let direct = |n: ThemeColorName| match n {
        ThemeColorName::Dark1 => Some(ThemeSlot::Dk1),
        ThemeColorName::Light1 => Some(ThemeSlot::Lt1),
        ThemeColorName::Dark2 => Some(ThemeSlot::Dk2),
        ThemeColorName::Light2 => Some(ThemeSlot::Lt2),
        ThemeColorName::Accent1 => Some(ThemeSlot::Accent1),
        ThemeColorName::Accent2 => Some(ThemeSlot::Accent2),
        ThemeColorName::Accent3 => Some(ThemeSlot::Accent3),
        ThemeColorName::Accent4 => Some(ThemeSlot::Accent4),
        ThemeColorName::Accent5 => Some(ThemeSlot::Accent5),
        ThemeColorName::Accent6 => Some(ThemeSlot::Accent6),
        ThemeColorName::Hyperlink => Some(ThemeSlot::Hlink),
        ThemeColorName::FollowedHyperlink => Some(ThemeSlot::FolHlink),
        _ => None,
    };
    match name {
        ThemeColorName::Text1 => direct(mapping.t1),
        ThemeColorName::Background1 => direct(mapping.bg1),
        ThemeColorName::Text2 => direct(mapping.t2),
        ThemeColorName::Background2 => direct(mapping.bg2),
        other => direct(other),
    }
}

/// One font collection of the theme (`a:majorFont` or `a:minorFont`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ThemeFonts {
    /// `a:latin`.
    pub latin: String,
    /// `a:ea` (often empty).
    pub east_asia: String,
    /// `a:cs` (often empty).
    pub complex: String,
}

/// The parts of a theme that Write formatting resolves against.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DocTheme {
    /// The theme name.
    pub name: String,
    /// The twelve colour slots, indexed by [`ThemeSlot`].
    pub colors: [Color; 12],
    /// Headings.
    pub major: ThemeFonts,
    /// Body.
    pub minor: ThemeFonts,
}

impl Default for DocTheme {
    fn default() -> Self {
        Self::office()
    }
}

impl DocTheme {
    /// The Office theme of Office 2013–2022 (Calibri Light / Calibri, the `4472C4` accent row),
    /// which Word 2016–2021 documents and new Kiln documents use.
    #[must_use]
    pub fn office() -> Self {
        let c = |v: u32| {
            let [_, r, g, b] = v.to_be_bytes();
            Color::rgb(r, g, b)
        };
        Self {
            name: "Office Theme".to_owned(),
            colors: [
                c(0x000000),
                c(0xFFFFFF),
                c(0x44546A),
                c(0xE7E6E6),
                c(0x4472C4),
                c(0xED7D31),
                c(0xA5A5A5),
                c(0xFFC000),
                c(0x5B9BD5),
                c(0x70AD47),
                c(0x0563C1),
                c(0x954F72),
            ],
            major: ThemeFonts {
                latin: "Calibri Light".to_owned(),
                ..ThemeFonts::default()
            },
            minor: ThemeFonts {
                latin: "Calibri".to_owned(),
                ..ThemeFonts::default()
            },
        }
    }

    /// Extracts the colours and fonts of a parsed DrawingML theme.
    #[must_use]
    pub fn from_ooxml(theme: &Theme) -> Self {
        let colors = theme_color_table(theme).map(|rgba| {
            let c = rgba.to_color();
            Color::rgb(c.r, c.g, c.b)
        });
        let fonts = |f: &kiln_ooxml::drawingml::theme::FontCollection| ThemeFonts {
            latin: f.latin.typeface.clone(),
            east_asia: f.ea.typeface.clone(),
            complex: f.cs.typeface.clone(),
        };
        Self {
            name: theme.name.clone(),
            colors,
            major: fonts(&theme.font_scheme.major),
            minor: fonts(&theme.font_scheme.minor),
        }
    }

    /// The colour of a slot.
    #[must_use]
    pub const fn color(&self, slot: ThemeSlot) -> Color {
        self.colors[slot.index()]
    }

    /// The font a theme reference names (empty when the collection leaves that script empty).
    #[must_use]
    pub fn font(&self, r: ThemeFontRef) -> &str {
        let coll = if r.is_major() {
            &self.major
        } else {
            &self.minor
        };
        match r {
            ThemeFontRef::MajorEastAsia | ThemeFontRef::MinorEastAsia => &coll.east_asia,
            ThemeFontRef::MajorBidi | ThemeFontRef::MinorBidi => &coll.complex,
            _ => &coll.latin,
        }
    }
}

/// Applies Word's `w:themeTint`/`w:themeShade` (0–255) to a colour in HSL space, as Word does:
/// tint `L' = L·t + (1 − t)`, shade `L' = L·s`, channels truncated (Accent 1 `4472C4` + shade
/// `BF` = `2F5496`; + tint `33` = `D9E2F3`).
#[must_use]
pub fn apply_tint_shade(color: Color, tint: Option<u8>, shade: Option<u8>) -> Color {
    if tint.is_none() && shade.is_none() {
        return color;
    }
    let hsl = rgb_to_hsl(f64::from(color.r), f64::from(color.g), f64::from(color.b));
    let mut l = hsl.l;
    if let Some(t) = tint {
        let t = f64::from(t) / 255.0;
        l = l * t + (1.0 - t);
    }
    if let Some(s) = shade {
        l *= f64::from(s) / 255.0;
    }
    let (r, g, b) = hsl_to_rgb(hsl.h, hsl.s, l.clamp(0.0, 1.0));
    let ch = |v: f64| {
        // Truncation as Word does; the epsilon absorbs float noise such as 47.99999.
        let t = (v + 1e-6).floor().clamp(0.0, 255.0);
        // The clamp above bounds the value to 0..=255, so the cast is exact.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let byte = t as u8;
        byte
    };
    Color::rgb(ch(r), ch(g), ch(b))
}

/// Resolves a WordprocessingML colour to an RGB value or `auto`. A theme reference wins over the
/// cached `val`, except that a cached value within rounding distance of the computed one is used
/// verbatim: Word's own tint/shade arithmetic rounds slightly differently (Heading 3's cached
/// `1F3763` versus the exact `203864`), while a changed theme moves far beyond the tolerance.
#[must_use]
pub fn resolve_color(
    color: Option<&DocColor>,
    theme: &DocTheme,
    mapping: &ColorSchemeMapping,
) -> ColorValue {
    let Some(color) = color else {
        return ColorValue::Auto;
    };
    if let Some(name) = color.theme_color
        && name != ThemeColorName::None
        && let Some(slot) = theme_slot_for(name, mapping)
    {
        let computed = apply_tint_shade(theme.color(slot), color.theme_tint, color.theme_shade);
        return match color.val {
            ColorValue::Rgb(cached) if near(cached, computed, 2) => ColorValue::Rgb(cached),
            _ => ColorValue::Rgb(computed),
        };
    }
    color.val
}

fn near(a: Color, b: Color, tolerance: u8) -> bool {
    a.r.abs_diff(b.r) <= tolerance
        && a.g.abs_diff(b.g) <= tolerance
        && a.b.abs_diff(b.b) <= tolerance
}

/// The fill colour of a shading, or `None` for no fill or `auto`.
#[must_use]
pub fn resolve_shading_fill(
    shading: Option<&Shading>,
    theme: &DocTheme,
    mapping: &ColorSchemeMapping,
) -> Option<Color> {
    resolve_color(shading?.fill.as_ref(), theme, mapping).rgb()
}

/// Concrete font names per script slot after theme resolution.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ResolvedFonts {
    /// ASCII.
    pub ascii: String,
    /// High ANSI.
    pub h_ansi: String,
    /// East Asian.
    pub east_asia: String,
    /// Complex script.
    pub cs: String,
}

/// Resolves run fonts to names: theme references win over explicit names; theme collections that
/// leave East Asian or complex-script fonts empty fall back to the collection's Latin font; empty
/// slots fall back to the ASCII font.
#[must_use]
pub fn resolve_run_fonts(fonts: Option<&RunFonts>, theme: &DocTheme) -> ResolvedFonts {
    let pick = |name: Option<&String>, r: Option<ThemeFontRef>| -> String {
        if let Some(r) = r {
            let f = theme.font(r);
            if !f.is_empty() {
                return f.to_owned();
            }
            let latin = if r.is_major() {
                ThemeFontRef::MajorHAnsi
            } else {
                ThemeFontRef::MinorHAnsi
            };
            return theme.font(latin).to_owned();
        }
        name.cloned().unwrap_or_default()
    };
    let f = fonts.cloned().unwrap_or_default();
    let ascii = pick(f.ascii.as_ref(), f.ascii_theme);
    let mut h_ansi = pick(f.h_ansi.as_ref(), f.h_ansi_theme);
    if h_ansi.is_empty() {
        h_ansi.clone_from(&ascii);
    }
    let ascii = if ascii.is_empty() {
        h_ansi.clone()
    } else {
        ascii
    };
    let mut east_asia = pick(f.east_asia.as_ref(), f.east_asia_theme);
    if east_asia.is_empty() {
        east_asia.clone_from(&ascii);
    }
    let mut cs = pick(f.cs.as_ref(), f.cs_theme);
    if cs.is_empty() {
        cs.clone_from(&ascii);
    }
    ResolvedFonts {
        ascii,
        h_ansi,
        east_asia,
        cs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(v: u32) -> Color {
        let [_, r, g, b] = v.to_be_bytes();
        Color::rgb(r, g, b)
    }

    #[test]
    fn maps_theme_colour_names_through_the_scheme_mapping() {
        let m = ColorSchemeMapping::default();
        assert_eq!(
            theme_slot_for(ThemeColorName::Text1, &m),
            Some(ThemeSlot::Dk1)
        );
        assert_eq!(
            theme_slot_for(ThemeColorName::Background2, &m),
            Some(ThemeSlot::Lt2)
        );
        assert_eq!(
            theme_slot_for(ThemeColorName::Accent3, &m),
            Some(ThemeSlot::Accent3)
        );
        assert_eq!(
            theme_slot_for(ThemeColorName::Hyperlink, &m),
            Some(ThemeSlot::Hlink)
        );
        assert_eq!(theme_slot_for(ThemeColorName::None, &m), None);
        let swapped = ColorSchemeMapping {
            t1: ThemeColorName::Light1,
            ..m
        };
        assert_eq!(
            theme_slot_for(ThemeColorName::Text1, &swapped),
            Some(ThemeSlot::Lt1)
        );
        assert_eq!(ThemeSlot::FolHlink.as_str(), "folHlink");
    }

    #[test]
    fn applies_word_tint_and_shade_in_hsl() {
        assert_eq!(
            apply_tint_shade(hex(0x4472C4), None, Some(0xBF)),
            hex(0x2F5496)
        );
        assert_eq!(
            apply_tint_shade(hex(0x4472C4), Some(0x33), None),
            hex(0xD9E2F3)
        );
        assert_eq!(
            apply_tint_shade(hex(0x000000), Some(0xA5), None),
            hex(0x5A5A5A)
        );
        assert_eq!(apply_tint_shade(hex(0xABCDEF), None, None), hex(0xABCDEF));
    }

    #[test]
    fn theme_colours_win_over_cached_values_and_follow_theme_changes() {
        let theme = DocTheme::office();
        let m = ColorSchemeMapping::default();
        let accent2 = DocColor::theme(0x000000, ThemeColorName::Accent2);
        assert_eq!(
            resolve_color(Some(&accent2), &theme, &m).to_string(),
            "ED7D31"
        );
        let h3 = DocColor::theme(0x1F3763, ThemeColorName::Accent1).with_shade(0x7F);
        assert_eq!(resolve_color(Some(&h3), &theme, &m).to_string(), "1F3763");
        let mut changed = DocTheme::office();
        changed.colors[ThemeSlot::Accent1.index()] = hex(0xFF0000);
        let h1 = DocColor::theme(0x2F5496, ThemeColorName::Accent1).with_shade(0xBF);
        assert_eq!(resolve_color(Some(&h1), &changed, &m).to_string(), "BF0000");
        assert_eq!(
            resolve_color(Some(&DocColor::auto()), &theme, &m),
            ColorValue::Auto
        );
        assert_eq!(resolve_color(None, &theme, &m), ColorValue::Auto);
        let auto_fill = Shading::fill(DocColor::auto());
        assert_eq!(resolve_shading_fill(Some(&auto_fill), &theme, &m), None);
        let mut accent6 = DocColor::auto().with_tint(0x33);
        accent6.theme_color = Some(ThemeColorName::Accent6);
        let tinted = Shading::fill(accent6);
        assert_eq!(
            resolve_shading_fill(Some(&tinted), &theme, &m),
            Some(hex(0xE2EFD9))
        );
    }

    #[test]
    fn resolves_theme_fonts_per_slot_with_fallbacks() {
        let theme = DocTheme::office();
        let major = RunFonts {
            ascii_theme: Some(ThemeFontRef::MajorHAnsi),
            h_ansi_theme: Some(ThemeFontRef::MajorHAnsi),
            east_asia_theme: Some(ThemeFontRef::MajorEastAsia),
            cs_theme: Some(ThemeFontRef::MajorBidi),
            ..RunFonts::default()
        };
        let f = resolve_run_fonts(Some(&major), &theme);
        assert_eq!(f.ascii, "Calibri Light");
        assert_eq!(f.h_ansi, "Calibri Light");
        assert_eq!(f.east_asia, "Calibri Light");
        assert_eq!(f.cs, "Calibri Light");
        let arial = RunFonts {
            ascii: Some("Arial".into()),
            ..RunFonts::default()
        };
        let f = resolve_run_fonts(Some(&arial), &theme);
        assert_eq!(
            (
                f.ascii.as_str(),
                f.h_ansi.as_str(),
                f.east_asia.as_str(),
                f.cs.as_str()
            ),
            ("Arial", "Arial", "Arial", "Arial")
        );
        assert_eq!(resolve_run_fonts(None, &theme).ascii, "");
        let only_h = RunFonts {
            h_ansi: Some("Georgia".into()),
            ..RunFonts::default()
        };
        assert_eq!(resolve_run_fonts(Some(&only_h), &theme).ascii, "Georgia");
    }

    #[test]
    fn office_theme_matches_the_drawingml_office_theme() {
        let from_ooxml = DocTheme::from_ooxml(&kiln_ooxml::drawingml::theme::office_theme());
        let ours = DocTheme::office();
        assert_eq!(from_ooxml.colors, ours.colors);
        assert_eq!(from_ooxml.major.latin, ours.major.latin);
        assert_eq!(from_ooxml.minor.latin, ours.minor.latin);
        assert_eq!(ours.font(ThemeFontRef::MinorEastAsia), "");
        assert_eq!(DocTheme::default(), ours);
    }
}
