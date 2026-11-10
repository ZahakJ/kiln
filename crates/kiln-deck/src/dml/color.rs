// SPDX-License-Identifier: AGPL-3.0-or-later
//! DrawingML colours ⇄ kiln-doc run colours.
//!
//! Runs store colours as `kiln_doc::DocColor` (Word's model: an RGB value plus a theme slot
//! with tint or shade). DrawingML's `lumMod`/`lumOff` pairs are exactly Word's tint and shade
//! (`lumMod m` = shade `m`; `lumMod m, lumOff 1 − m` = tint `m`), so the standard theme
//! variations ("Accent 1, Darker 25 %", "Lighter 40 %") convert both ways; other transforms are
//! folded into the cached RGB value.

use kiln_doc::st::ThemeColorName;
use kiln_doc::{ColorValue, DocColor};
use kiln_ooxml::drawingml::color::{
    ColorContext, ColorModel, ColorTransform, DrawingColor, resolve_color,
};

/// The DrawingML scheme name of a Word theme colour name.
#[must_use]
pub fn scheme_name(name: ThemeColorName) -> Option<&'static str> {
    use ThemeColorName as T;
    Some(match name {
        T::Dark1 => "dk1",
        T::Light1 => "lt1",
        T::Dark2 => "dk2",
        T::Light2 => "lt2",
        T::Accent1 => "accent1",
        T::Accent2 => "accent2",
        T::Accent3 => "accent3",
        T::Accent4 => "accent4",
        T::Accent5 => "accent5",
        T::Accent6 => "accent6",
        T::Hyperlink => "hlink",
        T::FollowedHyperlink => "folHlink",
        T::Background1 => "bg1",
        T::Text1 => "tx1",
        T::Background2 => "bg2",
        T::Text2 => "tx2",
        T::None => return None,
    })
}

/// The Word theme colour name of a DrawingML scheme name.
#[must_use]
pub fn theme_color_name(scheme: &str) -> Option<ThemeColorName> {
    use ThemeColorName as T;
    Some(match scheme {
        "dk1" => T::Dark1,
        "lt1" => T::Light1,
        "dk2" => T::Dark2,
        "lt2" => T::Light2,
        "accent1" => T::Accent1,
        "accent2" => T::Accent2,
        "accent3" => T::Accent3,
        "accent4" => T::Accent4,
        "accent5" => T::Accent5,
        "accent6" => T::Accent6,
        "hlink" => T::Hyperlink,
        "folHlink" => T::FollowedHyperlink,
        "bg1" => T::Background1,
        "tx1" => T::Text1,
        "bg2" => T::Background2,
        "tx2" => T::Text2,
        _ => return None,
    })
}

fn to_byte(per100k: i64) -> u8 {
    u8::try_from(((per100k.clamp(0, 100_000) * 255 + 50_000) / 100_000).clamp(0, 255))
        .unwrap_or(255)
}

fn from_byte(b: u8) -> i64 {
    // Snap to the nearest 1000 so Office's standard steps (75 000, 50 000, …) survive the
    // 8-bit Word representation exactly.
    let v = i64::from(b) * 100_000 / 255;
    ((v + 500) / 1000) * 1000
}

/// A DrawingML colour as a run colour. With a colour context the cached RGB value is the exact
/// rendering; without one theme colours cache `auto`.
#[must_use]
pub fn drawing_to_doc_color(c: &DrawingColor, ctx: Option<&ColorContext>) -> DocColor {
    let cached = |c: &DrawingColor| -> ColorValue {
        match ctx {
            Some(ctx) => ColorValue::Rgb(resolve_color(c, ctx).to_color()),
            None => match &c.model {
                ColorModel::Srgb(hex) if c.transforms.is_empty() => {
                    ColorValue::parse(hex).unwrap_or_default()
                }
                ColorModel::Scheme(_) => ColorValue::Auto,
                _ => ColorValue::Rgb(resolve_color(c, &ColorContext::default()).to_color()),
            },
        }
    };
    let mut out = DocColor {
        val: cached(c),
        theme_color: None,
        theme_tint: None,
        theme_shade: None,
    };
    if let ColorModel::Scheme(name) = &c.model
        && let Some(theme) = theme_color_name(name)
    {
        out.theme_color = Some(theme);
        let get = |n: &str| {
            c.transforms
                .iter()
                .find(|t| t.name == n)
                .and_then(|t| t.val)
        };
        let others = c
            .transforms
            .iter()
            .any(|t| t.name != "lumMod" && t.name != "lumOff");
        if !others {
            match (get("lumMod"), get("lumOff")) {
                (Some(m), None) if m < 100_000 => out.theme_shade = Some(to_byte(m)),
                (Some(m), Some(o)) if o > 0 && m + o == 100_000 => {
                    out.theme_tint = Some(to_byte(m));
                }
                _ => {}
            }
        }
    }
    out
}

/// A run colour as a DrawingML colour (`auto` becomes the text colour `tx1`).
#[must_use]
pub fn doc_to_drawing_color(c: &DocColor) -> DrawingColor {
    if let Some(name) = c.theme_color.and_then(scheme_name) {
        let mut d = DrawingColor::scheme(name);
        if let Some(t) = c.theme_tint {
            let m = from_byte(t);
            if m < 100_000 {
                d.transforms.push(ColorTransform::new("lumMod", m));
                d.transforms
                    .push(ColorTransform::new("lumOff", 100_000 - m));
            }
        } else if let Some(s) = c.theme_shade {
            let m = from_byte(s);
            if m < 100_000 {
                d.transforms.push(ColorTransform::new("lumMod", m));
            }
        }
        return d;
    }
    match c.val {
        ColorValue::Rgb(rgb) => {
            DrawingColor::srgb(&format!("{:02X}{:02X}{:02X}", rgb.r, rgb.g, rgb.b))
        }
        ColorValue::Auto => DrawingColor::scheme("tx1"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_ooxml::drawingml::theme::{office_theme, theme_color_context};

    #[test]
    fn office_theme_variations_round_trip() {
        for (m, o) in [
            (75_000, 0),
            (50_000, 0),
            (60_000, 40_000),
            (40_000, 60_000),
            (20_000, 80_000),
            (90_000, 0),
        ] {
            let mut d = DrawingColor::scheme("accent2").with("lumMod", m);
            if o > 0 {
                d = d.with("lumOff", o);
            }
            let doc = drawing_to_doc_color(&d, None);
            assert_eq!(doc.theme_color, Some(ThemeColorName::Accent2));
            assert_eq!(doc_to_drawing_color(&doc), d, "{m} {o}");
        }
    }

    #[test]
    fn rgb_and_aliases_convert() {
        let d = DrawingColor::srgb("E05A27");
        let doc = drawing_to_doc_color(&d, None);
        assert_eq!(doc, DocColor::rgb(0xE0_5A_27));
        assert_eq!(doc_to_drawing_color(&doc), d);
        let tx = DrawingColor::scheme("tx1");
        let doc = drawing_to_doc_color(&tx, None);
        assert_eq!(doc.theme_color, Some(ThemeColorName::Text1));
        assert_eq!(doc.val, ColorValue::Auto);
        assert_eq!(doc_to_drawing_color(&doc), tx);
        assert_eq!(doc_to_drawing_color(&DocColor::auto()), tx);
        for n in [
            "dk1", "lt1", "dk2", "lt2", "accent1", "accent3", "accent4", "accent5", "accent6",
            "hlink", "folHlink", "bg1", "bg2", "tx2",
        ] {
            assert_eq!(theme_color_name(n).and_then(scheme_name), Some(n));
        }
        assert_eq!(theme_color_name("phClr"), None);
        assert_eq!(scheme_name(ThemeColorName::None), None);
    }

    #[test]
    fn a_context_caches_the_exact_rendering() {
        let theme = office_theme();
        let ctx = theme_color_context(&theme, Default::default(), None);
        let d = DrawingColor::scheme("accent1").with("lumMod", 75_000);
        let doc = drawing_to_doc_color(&d, Some(&ctx));
        assert!(matches!(doc.val, ColorValue::Rgb(_)));
        assert_eq!(doc.theme_shade, Some(191));
        // Transforms Word cannot express keep only the cached value and the slot.
        let odd = DrawingColor::scheme("accent1").with("satMod", 120_000);
        let doc = drawing_to_doc_color(&odd, Some(&ctx));
        assert_eq!(doc.theme_shade, None);
        assert_eq!(doc.theme_tint, None);
        let hsl = DrawingColor {
            model: ColorModel::Preset("red".into()),
            transforms: vec![],
        };
        assert_eq!(
            drawing_to_doc_color(&hsl, None).val,
            ColorValue::hex(0xFF_00_00)
        );
    }
}
