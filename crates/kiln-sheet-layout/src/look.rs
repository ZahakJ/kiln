// SPDX-License-Identifier: AGPL-3.0-or-later
//! What a cell looks like: its format resolved for painting.
//!
//! Layers, bottom to top (Excel's order): the table style of a cell inside a formatted table
//! (only where the cell's own format leaves a member at its default), the cell format, then the
//! conditional formats that apply. Colours become Scene paints: "automatic" text and borders,
//! and the theme's Text 1 on an unfilled cell, use the themed ink role so dark mode is a palette
//! swap; anything the workbook names explicitly keeps its colour.

use std::sync::Arc;

use kiln_scene::{ColorRole, Paint};
use kiln_sheet::Theme;
use kiln_sheet::cf::CellVisual;
use kiln_sheet::color::Color;
use kiln_sheet::styles::{Alignment, Border, BorderSide, CellStyle, Dxf, Fill, Font, PatternType};

/// `0xRRGGBB` → an opaque paint.
#[must_use]
pub fn rgb(rgb: u32) -> Paint {
    Paint::Rgba(((rgb & 0xFF_FFFF) << 8) | 0xFF)
}

/// `0xRRGGBB` → the paint of text drawn on its cell's own fill: alpha `0xFE`, which tells the
/// painter the colour was chosen for that fill and is not to be adapted for a dark page
/// (`packages/surface/src/contrast.ts`). One alpha step is invisible.
#[must_use]
pub fn rgb_on_fill(rgb: u32) -> Paint {
    Paint::Rgba(((rgb & 0xFF_FFFF) << 8) | 0xFE)
}

/// Black or white, whichever reads better on `bg` (`0xRRGGBB`).
#[must_use]
pub fn contrast_on(bg: u32) -> Paint {
    let r = f64::from((bg >> 16) & 0xFF);
    let g = f64::from((bg >> 8) & 0xFF);
    let b = f64::from(bg & 0xFF);
    if 0.299 * r + 0.587 * g + 0.114 * b < 140.0 {
        rgb(0xFF_FFFF)
    } else {
        rgb(0)
    }
}

/// A cell background.
#[derive(Debug, Clone, PartialEq)]
pub enum FillLook {
    /// Nothing: the page (and the gridlines) show.
    None,
    /// One colour.
    Solid(Paint),
    /// A two-colour pattern (Excel's 17 patterns), drawn as a background and a foreground mask.
    Pattern {
        /// The pattern.
        pattern: PatternType,
        /// Foreground `0xRRGGBB`.
        fg: u32,
        /// Background `0xRRGGBB`.
        bg: u32,
    },
    /// A gradient with stops `(position, 0xRRGGBB)`; `degree` for linear ones, `None` for path
    /// gradients (drawn radially from the cell centre).
    Gradient {
        /// Angle of a linear gradient.
        degree: Option<f64>,
        /// Stops.
        stops: Vec<(f32, u32)>,
    },
}

impl FillLook {
    /// The colour behind the text, if the fill has one (`0xRRGGBB`).
    #[must_use]
    pub fn background(&self) -> Option<u32> {
        match self {
            FillLook::None => None,
            FillLook::Solid(Paint::Rgba(c)) => Some(c >> 8),
            FillLook::Solid(Paint::Role(_)) => None,
            FillLook::Pattern { bg, fg, pattern } => Some(if *pattern == PatternType::Solid {
                *fg
            } else {
                *bg
            }),
            FillLook::Gradient { stops, .. } => stops.first().map(|s| s.1),
        }
    }

    /// Whether the fill covers the gridlines.
    #[must_use]
    pub fn is_some(&self) -> bool {
        !matches!(self, FillLook::None)
    }
}

/// A border side ready to paint.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EdgeLook {
    /// The side as stored.
    pub side: BorderSide,
    /// Its colour.
    pub paint: Paint,
}

/// A cell format resolved for painting.
#[derive(Debug, Clone, PartialEq)]
pub struct CellLook {
    /// Background.
    pub fill: FillLook,
    /// Font (family, size, weight, decorations).
    pub font: Font,
    /// Text colour.
    pub ink: Paint,
    /// Alignment.
    pub align: Alignment,
    /// Border sides (`None` = no line).
    pub border: Border,
    /// Number format code.
    pub num_fmt: Arc<str>,
}

/// Resolves colours against the workbook theme.
#[derive(Debug, Clone, Copy)]
pub struct Colors<'a> {
    /// The workbook theme.
    pub theme: &'a Theme,
}

impl Colors<'_> {
    /// `0xRRGGBB` of a colour (`auto` for automatic).
    #[must_use]
    pub fn resolve(&self, c: Color, auto: u32) -> u32 {
        c.resolve(self.theme, auto)
    }

    /// The fill as painted.
    #[must_use]
    pub fn fill(&self, fill: &Fill) -> FillLook {
        match fill {
            Fill::Pattern {
                pattern: PatternType::None,
                ..
            } => FillLook::None,
            Fill::Pattern {
                pattern: PatternType::Solid,
                fg,
                bg,
            } => {
                // Excel paints a solid fill with its foreground (a fill without one uses the
                // background colour, then automatic: the system's window colour).
                match fg.or(*bg) {
                    // Background 1 (white) is the page: in dark mode it follows the theme.
                    Some(Color::Theme { index: 0, tint }) if tint.0 == 0.0 => {
                        FillLook::Solid(Paint::Role(ColorRole::Page))
                    }
                    Some(c) => FillLook::Solid(rgb(self.resolve(c, 0xFF_FFFF))),
                    None => FillLook::Solid(rgb(0)),
                }
            }
            Fill::Pattern { pattern, fg, bg } => FillLook::Pattern {
                pattern: *pattern,
                fg: fg.map_or(0, |c| self.resolve(c, 0)),
                bg: bg.map_or(0xFF_FFFF, |c| self.resolve(c, 0xFF_FFFF)),
            },
            Fill::Gradient(g) => FillLook::Gradient {
                degree: (g.kind == kiln_sheet::styles::GradientKind::Linear).then_some(g.degree.0),
                stops: g
                    .stops
                    .iter()
                    .map(|s| (s.position.0 as f32, self.resolve(s.color, 0xFF_FFFF)))
                    .collect(),
            },
        }
    }

    /// The text colour of `font` on `fill`.
    #[must_use]
    pub fn ink(&self, font: &Font, fill: &FillLook) -> Paint {
        let automatic = match font.color {
            None | Some(Color::Auto) => true,
            // Text 1 without a tint is the theme's automatic text colour.
            Some(Color::Theme { index: 1, tint }) => tint.0 == 0.0,
            _ => false,
        };
        let on_fill = fill.background().is_some();
        if automatic {
            return match fill.background() {
                Some(bg) => match contrast_on(bg) {
                    Paint::Rgba(v) => rgb_on_fill(v >> 8),
                    p => p,
                },
                None => Paint::Role(ColorRole::Ink),
            };
        }
        font.color.map_or(Paint::Role(ColorRole::Ink), |c| {
            let v = self.resolve(c, 0);
            if on_fill { rgb_on_fill(v) } else { rgb(v) }
        })
    }

    /// The colour of a border side.
    #[must_use]
    pub fn edge(&self, side: &BorderSide, fill: &FillLook) -> Paint {
        match side.color {
            None | Some(Color::Auto) => match fill.background() {
                Some(_) => rgb(0),
                None => Paint::Role(ColorRole::Ink),
            },
            Some(Color::Theme { index: 1, tint }) if tint.0 == 0.0 && !fill.is_some() => {
                Paint::Role(ColorRole::Ink)
            }
            Some(c) => rgb(self.resolve(c, 0)),
        }
    }
}

fn is_default_side(s: Option<BorderSide>) -> bool {
    s.is_none_or(|x| x.style == kiln_sheet::styles::BorderStyle::None)
}

/// Layers a table-style format under a cell format: the table's members show where the cell's
/// own are at their defaults.
fn under(table: &Dxf, cell: &mut CellStyle) {
    if let Some(f) = &table.fill
        && matches!(
            cell.fill,
            Fill::Pattern {
                pattern: PatternType::None,
                ..
            }
        )
    {
        cell.fill = f.clone();
    }
    if let Some(p) = &table.font {
        let d = Font::default();
        let f = &mut cell.font;
        if let Some(b) = p.bold
            && !f.bold
        {
            f.bold = b;
        }
        if let Some(i) = p.italic
            && !f.italic
        {
            f.italic = i;
        }
        if let Some(c) = p.color
            && (f.color.is_none() || f.color == d.color)
        {
            f.color = (c != Color::Auto).then_some(c);
        }
        if let Some(u) = p.underline
            && f.underline == kiln_sheet::styles::Underline::None
        {
            f.underline = u;
        }
    }
    if let Some(b) = &table.border {
        let c = &mut cell.border;
        for (mine, theirs) in [
            (&mut c.left, b.left),
            (&mut c.right, b.right),
            (&mut c.top, b.top),
            (&mut c.bottom, b.bottom),
        ] {
            if is_default_side(*mine) && !is_default_side(theirs) {
                *mine = theirs;
            }
        }
    }
    if let Some(n) = &table.num_fmt
        && (cell.num_fmt.is_empty() || cell.num_fmt.eq_ignore_ascii_case("general"))
    {
        cell.num_fmt = n.clone();
    }
}

/// Layers a conditional format over a cell format.
fn over(cf: &Dxf, cell: &mut CellStyle) {
    if let Some(f) = &cf.fill {
        // A differential solid fill keeps its colour in `fg`.
        cell.fill = f.clone();
    }
    if let Some(p) = &cf.font {
        cell.font = p.apply(&cell.font);
    }
    if let Some(b) = &cf.border {
        let c = &mut cell.border;
        for (mine, theirs) in [
            (&mut c.left, b.left),
            (&mut c.right, b.right),
            (&mut c.top, b.top),
            (&mut c.bottom, b.bottom),
        ] {
            if theirs.is_some() {
                *mine = theirs;
            }
        }
    }
    if let Some(n) = &cf.num_fmt {
        cell.num_fmt = n.clone();
    }
}

/// The look of a cell from its format, the table style under it and its conditional formats.
#[must_use]
pub fn resolve(
    colors: Colors<'_>,
    style: &CellStyle,
    table: Option<&Dxf>,
    cf: Option<&CellVisual>,
) -> CellLook {
    let mut s = style.clone();
    if let Some(t) = table {
        under(t, &mut s);
    }
    if let Some(d) = cf.and_then(|v| v.dxf.as_ref()) {
        over(d, &mut s);
    }
    let mut fill = colors.fill(&s.fill);
    if let Some(c) = cf.and_then(|v| v.scale_color) {
        fill = FillLook::Solid(rgb(c));
    }
    let ink = colors.ink(&s.font, &fill);
    CellLook {
        fill,
        ink,
        align: s.alignment,
        border: s.border.normalized(),
        num_fmt: s.num_fmt.clone(),
        font: s.font,
    }
}

/// Pixel size of a font in points.
#[must_use]
pub fn pt_to_px(pt: f64) -> f32 {
    (pt * 4.0 / 3.0) as f32
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_sheet::num::F64;

    #[test]
    fn automatic_text_follows_the_theme_unless_filled() {
        let theme = Theme::default();
        let colors = Colors { theme: &theme };
        let style = CellStyle::default();
        let look = resolve(colors, &style, None, None);
        assert_eq!(look.ink, Paint::Role(ColorRole::Ink));
        assert_eq!(look.fill, FillLook::None);
        let mut filled = style.clone();
        filled.fill = Fill::solid(Color::rgb(0x10_2030));
        let look = resolve(colors, &filled, None, None);
        assert_eq!(look.fill, FillLook::Solid(rgb(0x10_2030)));
        // Marked as chosen for its fill: the painter does not adapt it for a dark page.
        assert_eq!(
            look.ink,
            rgb_on_fill(0xFF_FFFF),
            "white text on a dark fill"
        );
        let mut red = style;
        red.font.color = Some(Color::rgb(0xFF_0000));
        assert_eq!(resolve(colors, &red, None, None).ink, rgb(0xFF_0000));
        red.fill = Fill::solid(Color::rgb(0xFF_FF00));
        assert_eq!(
            resolve(colors, &red, None, None).ink,
            rgb_on_fill(0xFF_0000)
        );
    }

    #[test]
    fn table_styles_sit_under_the_cell_format_and_cf_over_it() {
        let theme = Theme::default();
        let colors = Colors { theme: &theme };
        let table = Dxf {
            fill: Some(Fill::solid(Color::rgb(0x44_72C4))),
            font: Some(kiln_sheet::styles::FontPatch {
                bold: Some(true),
                color: Some(Color::rgb(0xFF_FFFF)),
                ..Default::default()
            }),
            ..Dxf::default()
        };
        let look = resolve(colors, &CellStyle::default(), Some(&table), None);
        assert!(look.font.bold);
        assert_eq!(look.fill, FillLook::Solid(rgb(0x44_72C4)));
        assert_eq!(look.ink, rgb_on_fill(0xFF_FFFF));
        let own = CellStyle {
            fill: Fill::solid(Color::rgb(0xFF_FF00)),
            ..CellStyle::default()
        };
        let look = resolve(colors, &own, Some(&table), None);
        assert_eq!(
            look.fill,
            FillLook::Solid(rgb(0xFF_FF00)),
            "the cell's own fill wins"
        );
        let cf = CellVisual {
            dxf: Some(Dxf {
                font: Some(kiln_sheet::styles::FontPatch {
                    color: Some(Color::rgb(0x9C_0006)),
                    ..Default::default()
                }),
                fill: Some(Fill::solid(Color::rgb(0xFF_C7CE))),
                ..Dxf::default()
            }),
            ..CellVisual::default()
        };
        let look = resolve(colors, &own, Some(&table), Some(&cf));
        assert_eq!(look.fill, FillLook::Solid(rgb(0xFF_C7CE)));
        assert_eq!(look.ink, rgb_on_fill(0x9C_0006));
        let scale = CellVisual {
            scale_color: Some(0x63_BE7B),
            ..CellVisual::default()
        };
        assert_eq!(
            resolve(colors, &CellStyle::default(), None, Some(&scale)).fill,
            FillLook::Solid(rgb(0x63_BE7B))
        );
    }

    #[test]
    fn background_1_fills_follow_the_page() {
        let theme = Theme::default();
        let colors = Colors { theme: &theme };
        let white = Fill::solid(Color::Theme {
            index: 0,
            tint: F64(0.0),
        });
        assert_eq!(
            colors.fill(&white),
            FillLook::Solid(Paint::Role(ColorRole::Page))
        );
        let tinted = Fill::solid(Color::Theme {
            index: 0,
            tint: F64(-0.15),
        });
        assert!(matches!(
            colors.fill(&tinted),
            FillLook::Solid(Paint::Rgba(_))
        ));
        assert_eq!(contrast_on(0xFF_FFFF), rgb(0));
        assert_eq!(contrast_on(0), rgb(0xFF_FFFF));
    }
}
