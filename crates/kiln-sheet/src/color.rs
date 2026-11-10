// SPDX-License-Identifier: AGPL-3.0-or-later
//! SpreadsheetML colours (ARGB, theme slot with tint, the legacy indexed palette, automatic) and
//! the workbook theme they resolve against.

use serde::{Deserialize, Serialize};

use crate::error::ModelError;
use crate::num::F64;

/// A colour as SpreadsheetML stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Color {
    /// `rgb="FFRRGGBB"` with an optional tint.
    Rgb {
        /// `0xAARRGGBB`.
        argb: u32,
        /// −1 (darker) … 1 (lighter).
        tint: F64,
    },
    /// `theme="n"`: a slot of the theme's colour scheme.
    Theme {
        /// Theme index (0 = Background 1, 1 = Text 1, 2 = Background 2, 3 = Text 2, 4–9 accents,
        /// 10 hyperlink, 11 followed hyperlink).
        index: u8,
        /// −1 … 1.
        tint: F64,
    },
    /// `indexed="n"`: the legacy 64-colour palette.
    Indexed {
        /// Palette index (64 = system foreground, 65 = system background).
        index: u16,
        /// −1 … 1.
        tint: F64,
    },
    /// `auto="1"`: the system colour (black text, white background).
    Auto,
}

impl Color {
    /// An opaque RGB colour from `RRGGBB` or `AARRGGBB` hex (with or without `#`).
    pub fn hex(text: &str) -> Result<Color, ModelError> {
        let h = text.trim().trim_start_matches('#');
        let bad = || ModelError::invalid(format!("invalid colour \"{text}\""));
        if !(h.len() == 6 || h.len() == 8) || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(bad());
        }
        let v = u32::from_str_radix(h, 16).map_err(|_| bad())?;
        Ok(Color::Rgb {
            argb: if h.len() == 6 { 0xFF00_0000 | v } else { v },
            tint: F64(0.0),
        })
    }

    /// An opaque RGB colour from `0xRRGGBB`.
    pub const fn rgb(rgb: u32) -> Color {
        Color::Rgb {
            argb: 0xFF00_0000 | (rgb & 0xFF_FFFF),
            tint: F64(0.0),
        }
    }

    /// A theme colour.
    pub const fn theme(index: u8, tint: f64) -> Color {
        Color::Theme {
            index,
            tint: F64(tint),
        }
    }

    /// The colour as `0xRRGGBB`, resolving theme and palette entries and applying the tint.
    /// `Auto` resolves to `auto` (the caller's default: black for text, white for fills).
    pub fn resolve(&self, theme: &Theme, auto: u32) -> u32 {
        let (rgb, tint) = match *self {
            Color::Rgb { argb, tint } => (argb & 0xFF_FFFF, tint.0),
            Color::Theme { index, tint } => (theme.color(index), tint.0),
            Color::Indexed { index, tint } => (indexed_color(index).unwrap_or(auto), tint.0),
            Color::Auto => (auto, 0.0),
        };
        apply_tint(rgb, tint)
    }
}

/// The workbook theme as the sheet model needs it: the twelve colour slots (in `a:clrScheme`
/// order: dk1, lt1, dk2, lt2, accent1–6, hlink, folHlink) and the heading and body fonts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Theme {
    /// Theme name.
    pub name: String,
    /// `0xRRGGBB` per slot.
    pub colors: [u32; 12],
    /// Major (headings) Latin font.
    pub major_font: String,
    /// Minor (body) Latin font.
    pub minor_font: String,
}

impl Default for Theme {
    /// The Office theme (2013–2022), Excel's default.
    fn default() -> Theme {
        Theme {
            name: "Office Theme".into(),
            colors: [
                0x00_0000, 0xFF_FFFF, 0x44_546A, 0xE7_E6E6, 0x44_72C4, 0xED_7D31, 0xA5_A5A5,
                0xFF_C000, 0x5B_9BD5, 0x70_AD47, 0x05_63C1, 0x95_4F72,
            ],
            major_font: "Calibri Light".into(),
            minor_font: "Calibri".into(),
        }
    }
}

impl Theme {
    /// The colour of a SpreadsheetML theme index. Excel swaps the first two pairs relative to
    /// the scheme order: 0 = lt1, 1 = dk1, 2 = lt2, 3 = dk2.
    pub fn color(&self, index: u8) -> u32 {
        let slot = match index {
            0 => 1,
            1 => 0,
            2 => 3,
            3 => 2,
            n => usize::from(n),
        };
        self.colors.get(slot).copied().unwrap_or(0)
    }
}

/// The legacy palette (`indexed` 0–63; 64/65 are the system foreground and background).
pub fn indexed_color(index: u16) -> Option<u32> {
    const PALETTE: [u32; 64] = [
        0x000000, 0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x000000,
        0xFFFFFF, 0xFF0000, 0x00FF00, 0x0000FF, 0xFFFF00, 0xFF00FF, 0x00FFFF, 0x800000, 0x008000,
        0x000080, 0x808000, 0x800080, 0x008080, 0xC0C0C0, 0x808080, 0x9999FF, 0x993366, 0xFFFFCC,
        0xCCFFFF, 0x660066, 0xFF8080, 0x0066CC, 0xCCCCFF, 0x000080, 0xFF00FF, 0xFFFF00, 0x00FFFF,
        0x800080, 0x800000, 0x008080, 0x0000FF, 0x00CCFF, 0xCCFFFF, 0xCCFFCC, 0xFFFF99, 0x99CCFF,
        0xFF99CC, 0xCC99FF, 0xFFCC99, 0x3366FF, 0x33CCCC, 0x99CC00, 0xFFCC00, 0xFF9900, 0xFF6600,
        0x666699, 0x969696, 0x003366, 0x339966, 0x003300, 0x333300, 0x993300, 0x993366, 0x333399,
        0x333333,
    ];
    match index {
        64 => Some(0x000000),
        65 => Some(0xFFFFFF),
        i => PALETTE.get(usize::from(i)).copied(),
    }
}

fn rgb_to_hsl(rgb: u32) -> (f64, f64, f64) {
    let r = f64::from((rgb >> 16) & 0xFF) / 255.0;
    let g = f64::from((rgb >> 8) & 0xFF) / 255.0;
    let b = f64::from(rgb & 0xFF) / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    if (max - min).abs() < f64::EPSILON {
        return (0.0, 0.0, l);
    }
    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < f64::EPSILON {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < f64::EPSILON {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    (h / 6.0, s, l)
}

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> u32 {
    let channel = |t: f64, p: f64, q: f64| -> f64 {
        let t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 {
            l * (1.0 + s)
        } else {
            l + s - l * s
        };
        let p = 2.0 * l - q;
        (
            channel(h + 1.0 / 3.0, p, q),
            channel(h, p, q),
            channel(h - 1.0 / 3.0, p, q),
        )
    };
    let c = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u32;
    (c(r) << 16) | (c(g) << 8) | c(b)
}

/// Applies a SpreadsheetML tint (ECMA-376 §18.8.19): negative darkens the luminance, positive
/// moves it toward white.
pub fn apply_tint(rgb: u32, tint: f64) -> u32 {
    if tint == 0.0 || !tint.is_finite() {
        return rgb & 0xFF_FFFF;
    }
    let (h, s, l) = rgb_to_hsl(rgb);
    let t = tint.clamp(-1.0, 1.0);
    let l = if t < 0.0 {
        l * (1.0 + t)
    } else {
        l * (1.0 - t) + t
    };
    hsl_to_rgb(h, s, l)
}

/// Linear interpolation between two `0xRRGGBB` colours (`t` in 0…1), as colour scales blend.
pub fn mix(a: u32, b: u32, t: f64) -> u32 {
    let t = t.clamp(0.0, 1.0);
    let ch = |shift: u32| {
        let x = f64::from((a >> shift) & 0xFF);
        let y = f64::from((b >> shift) & 0xFF);
        ((x + (y - x) * t).round() as u32) << shift
    };
    ch(16) | ch(8) | ch(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_and_resolves_theme_slots() {
        assert_eq!(
            Color::hex("#ff0000").ok(),
            Some(Color::Rgb {
                argb: 0xFFFF_0000,
                tint: F64(0.0)
            })
        );
        assert!(Color::hex("12345").is_err());
        let t = Theme::default();
        assert_eq!(Color::theme(1, 0.0).resolve(&t, 0), 0x000000);
        assert_eq!(Color::theme(0, 0.0).resolve(&t, 0), 0xFFFFFF);
        assert_eq!(Color::theme(4, 0.0).resolve(&t, 0), 0x4472C4);
        assert_eq!(Color::Auto.resolve(&t, 0x123456), 0x123456);
        assert_eq!(
            Color::Indexed {
                index: 10,
                tint: F64(0.0)
            }
            .resolve(&t, 0),
            0xFF0000
        );
    }

    #[test]
    fn tints_like_excel() {
        // Accent 1, 40 % lighter (Excel shows 8EA9DB) and 25 % darker (2F5597).
        let lighter = apply_tint(0x4472C4, 0.399_975_585_192_419_2);
        let darker = apply_tint(0x4472C4, -0.249_977_111_117_893);
        let close = |a: u32, b: u32| {
            (0..3).all(|i| {
                let x = i64::from((a >> (8 * i)) & 0xFF);
                let y = i64::from((b >> (8 * i)) & 0xFF);
                (x - y).abs() <= 2
            })
        };
        assert!(close(lighter, 0x8EA9DB), "{lighter:06X}");
        assert!(close(darker, 0x2F5597), "{darker:06X}");
        assert_eq!(mix(0x000000, 0xFFFFFF, 0.5), 0x808080);
    }
}
