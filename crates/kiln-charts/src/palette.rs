// SPDX-License-Identifier: AGPL-3.0-or-later
//! Chart colours: the document theme, Office's automatic series colours and chart styles, and
//! Kiln's on-screen palette.
//!
//! In [`ColorMode::Document`] every colour comes from the document's theme exactly as Office
//! computes it (printing, PDF export, thumbnails). In [`ColorMode::Screen`], a chart whose document
//! uses a default Office theme is drawn with Kiln's palette instead — jade, ember, cobalt,
//! saffron, clay and plum, tuned separately for light and dark — and its automatic text, lines and
//! chart area follow the app theme, so a chart never glares white in dark mode. Charts in
//! documents with their own theme keep the theme's colours.

use kiln_ooxml::drawingml::color::{
    ColorContext, ColorMap, DrawingColor, Rgba, hex_to_rgba, resolve_color,
};
use kiln_ooxml::drawingml::theme::{Theme, office_theme, theme_color_table};
use kiln_ooxml::st::ColorSchemeIndex;
use kiln_scene::Paint;

/// How colours are chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// The document's own colours (print, export).
    Document,
    /// On screen, in the app's light or dark theme.
    Screen {
        /// Dark theme.
        dark: bool,
    },
}

/// Kiln's six chart accents, light theme: jade, ember, cobalt, saffron, clay, plum.
pub const KILN_LIGHT: [u32; 6] = [0x1D8A60, 0xE05A27, 0x3159C6, 0xE3A126, 0x8A5A3F, 0x8D4E85];
/// Kiln's chart accents, dark theme (lifted for contrast on the dark page).
pub const KILN_DARK: [u32; 6] = [0x3FBF8C, 0xF2804E, 0x7F9BF2, 0xF2C35C, 0xC9926D, 0xC783BD];

/// Accent rows of the default Office themes (2013–2022 and 2023+).
const OFFICE_ACCENTS: [[&str; 6]; 2] = [
    ["4472C4", "ED7D31", "A5A5A5", "FFC000", "5B9BD5", "70AD47"],
    ["156082", "E97132", "196B24", "0F9ED5", "A02B93", "4EA72E"],
];

fn rgb(hex: u32) -> Rgba {
    #[allow(clippy::cast_possible_truncation)]
    let c = |s: u32| f64::from((hex >> s) as u8);
    Rgba::rgb(c(16), c(8), c(0))
}

/// Resolved colours of one chart rendering.
#[derive(Debug, Clone)]
pub struct ChartColors {
    ctx: ColorContext,
    accents: [Rgba; 6],
    mode: ColorMode,
    kiln: bool,
    /// The theme's body font (`+mn-lt`).
    pub minor_font: String,
    /// The theme's heading font (`+mj-lt`).
    pub major_font: String,
}

impl ChartColors {
    /// Colours for `theme` (Office's default theme when `None`) in `mode`.
    #[must_use]
    pub fn new(theme: Option<&Theme>, mode: ColorMode) -> Self {
        let default_theme;
        let theme = match theme {
            Some(t) => t,
            None => {
                default_theme = office_theme();
                &default_theme
            }
        };
        let mut table = theme_color_table(theme);
        let accents_hex: Vec<String> = table[4..10]
            .iter()
            .map(|c| kiln_ooxml::drawingml::color::rgba_to_hex(*c))
            .collect();
        let is_default = OFFICE_ACCENTS.iter().any(|row| {
            row.iter()
                .zip(&accents_hex)
                .all(|(a, b)| a.eq_ignore_ascii_case(b.trim_start_matches('#')))
        });
        let kiln = is_default && matches!(mode, ColorMode::Screen { .. });
        if let ColorMode::Screen { dark } = mode
            && kiln
        {
            let row = if dark { KILN_DARK } else { KILN_LIGHT };
            for (i, hex) in row.iter().enumerate() {
                table[4 + i] = rgb(*hex);
            }
            // Text and background slots follow the app theme.
            let (ink, page) = if dark {
                (0xF5EBDD, 0x221A15)
            } else {
                (0x2B1C14, 0xFFFFFF)
            };
            table[ColorSchemeIndex::Dk1.index()] = rgb(ink);
            table[ColorSchemeIndex::Lt1.index()] = rgb(page);
            table[ColorSchemeIndex::Dk2.index()] = rgb(if dark { 0xB8A392 } else { 0x6B5547 });
            table[ColorSchemeIndex::Lt2.index()] = rgb(if dark { 0x2A201A } else { 0xF6EFE6 });
        }
        let accents = [table[4], table[5], table[6], table[7], table[8], table[9]];
        let font = |c: &kiln_ooxml::drawingml::theme::FontCollection, d: &str| {
            if c.latin.typeface.is_empty() {
                d.to_owned()
            } else {
                c.latin.typeface.clone()
            }
        };
        Self {
            ctx: ColorContext {
                theme_colors: table.map(Some),
                color_map: ColorMap::default(),
                ph_clr: None,
            },
            accents,
            mode,
            kiln,
            minor_font: font(&theme.font_scheme.minor, "Calibri"),
            major_font: font(&theme.font_scheme.major, "Calibri Light"),
        }
    }

    /// The mode.
    #[must_use]
    pub fn mode(&self) -> ColorMode {
        self.mode
    }

    /// Whether Kiln's palette replaced the theme's accents.
    #[must_use]
    pub fn uses_kiln_palette(&self) -> bool {
        self.kiln
    }

    /// Whether the dark screen theme is active.
    #[must_use]
    pub fn is_dark(&self) -> bool {
        matches!(self.mode, ColorMode::Screen { dark: true })
    }

    /// Resolves a DrawingML colour.
    #[must_use]
    pub fn resolve(&self, c: &DrawingColor) -> Rgba {
        resolve_color(c, &self.ctx)
    }

    /// Resolves a DrawingML colour to a Scene paint.
    #[must_use]
    pub fn paint(&self, c: &DrawingColor) -> Paint {
        to_paint(self.resolve(c))
    }

    /// Accent `i` (0–5).
    #[must_use]
    pub fn accent(&self, i: usize) -> Rgba {
        self.accents[i % 6]
    }

    /// The automatic colour of series (or, for charts with varied colours, point) number `i`
    /// under chart style `style` (1–48; Office's default is 2): the six accents, then darker and
    /// lighter variations of them, as Office cycles them.
    #[must_use]
    pub fn series_color(&self, i: usize, style: Option<u32>) -> Rgba {
        let style = style.unwrap_or(2).clamp(1, 48);
        let column = (style - 1) % 8;
        match column {
            // Style 1, 9, …: greys.
            0 => {
                let shades = [0x595959, 0x7F7F7F, 0xA5A5A5, 0x3F3F3F, 0xBFBFBF, 0x262626];
                rgb(shades[i % shades.len()])
            }
            // Styles 3–8, 11–16, …: shades of one accent.
            2..=7 => {
                let base = self.accent((column - 2) as usize);
                let n = i % 6;
                let (lum_mod, lum_off) = [
                    (100_000, 0),
                    (60_000, 0),
                    (60_000, 40_000),
                    (75_000, 0),
                    (40_000, 60_000),
                    (50_000, 0),
                ][n];
                transform(base, lum_mod, lum_off)
            }
            // Style 2 and every other column: the accent cycle.
            _ => {
                let base = self.accent(i % 6);
                match (i / 6) % 4 {
                    0 => base,
                    1 => transform(base, 60_000, 0),
                    2 => transform(base, 80_000, 20_000),
                    _ => transform(base, 80_000, 0),
                }
            }
        }
    }

    /// Automatic text (tick labels, legend): Office's `tx1` at 65 %, or the app's muted ink.
    #[must_use]
    pub fn text(&self) -> Rgba {
        match self.mode {
            ColorMode::Screen { dark: true } if self.kiln => rgb(0xD9CCBD),
            ColorMode::Screen { dark: false } if self.kiln => rgb(0x5A463A),
            _ => transform(self.dk1(), 65_000, 35_000),
        }
    }

    /// Automatic title text: `tx1` at 75 %, or the app's ink.
    #[must_use]
    pub fn title_text(&self) -> Rgba {
        match self.mode {
            ColorMode::Screen { .. } if self.kiln => self.dk1(),
            _ => transform(self.dk1(), 75_000, 25_000),
        }
    }

    /// Automatic gridlines: `tx1` at 15 %, or the app's hairline.
    #[must_use]
    pub fn gridline(&self) -> Rgba {
        match self.mode {
            ColorMode::Screen { dark: true } if self.kiln => rgb(0x3A2D25),
            ColorMode::Screen { dark: false } if self.kiln => rgb(0xEADFD2),
            _ => transform(self.dk1(), 15_000, 85_000),
        }
    }

    /// Automatic axis lines: `tx1` at 25 %.
    #[must_use]
    pub fn axis_line(&self) -> Rgba {
        match self.mode {
            ColorMode::Screen { dark: true } if self.kiln => rgb(0x5A473B),
            ColorMode::Screen { dark: false } if self.kiln => rgb(0xCDB8A0),
            _ => transform(self.dk1(), 25_000, 75_000),
        }
    }

    /// The automatic chart area fill (`bg1`).
    #[must_use]
    pub fn background(&self) -> Rgba {
        self.ctx.theme_colors[ColorSchemeIndex::Lt1.index()]
            .unwrap_or(Rgba::rgb(255.0, 255.0, 255.0))
    }

    fn dk1(&self) -> Rgba {
        self.ctx.theme_colors[ColorSchemeIndex::Dk1.index()].unwrap_or(Rgba::rgb(0.0, 0.0, 0.0))
    }
}

/// `lumMod`/`lumOff` on a colour (thousandths of a percent).
#[must_use]
pub fn transform(c: Rgba, lum_mod: i64, lum_off: i64) -> Rgba {
    let mut t = vec![kiln_ooxml::drawingml::color::ColorTransform::new(
        "lumMod", lum_mod,
    )];
    if lum_off != 0 {
        t.push(kiln_ooxml::drawingml::color::ColorTransform::new(
            "lumOff", lum_off,
        ));
    }
    kiln_ooxml::drawingml::color::apply_color_transforms(c, &t)
}

/// A resolved colour as a Scene paint.
#[must_use]
pub fn to_paint(c: Rgba) -> Paint {
    Paint::from_color(c.to_color())
}

/// A colour from `RRGGBB` text.
#[must_use]
pub fn hex(hex: &str) -> Rgba {
    hex_to_rgba(hex, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_accents_and_series_cycle() {
        let c = ChartColors::new(None, ColorMode::Document);
        assert!(!c.uses_kiln_palette());
        assert_eq!(
            kiln_ooxml::drawingml::color::rgba_to_hex(c.series_color(0, None))
                .trim_start_matches('#'),
            "4472C4"
        );
        assert_eq!(
            c.series_color(6, None).to_color(),
            transform(c.accent(0), 60_000, 0).to_color()
        );
        // Office's tx1 at 65 % is #595959.
        assert_eq!(
            kiln_ooxml::drawingml::color::rgba_to_hex(c.text()).trim_start_matches('#'),
            "595959"
        );
        // Monochrome style 3 shades accent 1.
        assert_ne!(
            c.series_color(0, Some(3)).to_color(),
            c.series_color(1, Some(3)).to_color()
        );
    }

    #[test]
    fn screen_mode_swaps_in_kiln_colours_for_default_themes_only() {
        let light = ChartColors::new(None, ColorMode::Screen { dark: false });
        assert!(light.uses_kiln_palette());
        assert_eq!(
            light.series_color(0, None).to_color(),
            rgb(KILN_LIGHT[0]).to_color()
        );
        let dark = ChartColors::new(None, ColorMode::Screen { dark: true });
        assert_eq!(
            dark.series_color(1, None).to_color(),
            rgb(KILN_DARK[1]).to_color()
        );
        assert!(dark.background().r < 60.0, "dark chart area");
        let mut custom = office_theme();
        custom.color_scheme.colors[4] = DrawingColor::srgb("123456");
        let c = ChartColors::new(Some(&custom), ColorMode::Screen { dark: true });
        assert!(!c.uses_kiln_palette());
        assert_eq!(
            kiln_ooxml::drawingml::color::rgba_to_hex(c.accent(0)).trim_start_matches('#'),
            "123456"
        );
    }
}
