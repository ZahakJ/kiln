// SPDX-License-Identifier: AGPL-3.0-or-later
//! Colours in the Scene: literal sRGBA, or a role the painter resolves against the theme.
//!
//! Document content that Word calls "automatic" (black text, white page) is emitted as a role, so
//! dark mode is a palette swap in the painter (tuned, not inverted) and never a re-layout.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A colour the painter resolves against its [`Palette`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ColorRole {
    /// The area behind pages, sheets and slides.
    Canvas,
    /// The page (Word's automatic page colour).
    Page,
    /// Automatic text colour.
    Ink,
    /// The text selection wash.
    Selection,
    /// The caret and the selection handles (the app's accent).
    Caret,
    /// The underline of an IME composition.
    Composition,
    /// Spelling squiggles.
    Squiggle,
    /// Guides, margins and other non-printing marks.
    Guide,
}

/// A paint: a literal colour or a themed role.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum Paint {
    /// Straight-alpha sRGB as `0xRRGGBBAA`.
    Rgba(u32),
    /// A colour of the theme.
    Role(ColorRole),
}

impl Paint {
    /// Opaque black.
    pub const BLACK: Paint = Paint::Rgba(0x0000_00ff);

    /// The literal colour of `kiln_common::Color`.
    #[must_use]
    pub fn from_color(c: kiln_common::Color) -> Paint {
        Paint::Rgba(u32::from_be_bytes([c.r, c.g, c.b, c.a]))
    }
}

/// One layer of a box shadow, as in CSS `box-shadow: dx dy blur spread color`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct ShadowLayer {
    /// Horizontal offset.
    pub dx: f32,
    /// Vertical offset.
    pub dy: f32,
    /// Blur radius (CSS: the Gaussian's standard deviation is half of it).
    pub blur: f32,
    /// Spread (grows or, when negative, shrinks the shadow's rectangle).
    pub spread: f32,
    /// Colour, `0xRRGGBBAA`.
    pub rgba: u32,
}

/// How the selection wash combines with what is under it.
///
/// The overlay is painted above the content, so a plain wash would veil the text. `Darken` (light
/// themes) and `Lighten` (dark themes) give exactly the look of a wash painted *under* opaque
/// text: the page takes the wash colour and the ink keeps its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum SelectionBlend {
    /// `min(dst, wash)` per channel.
    Darken,
    /// `max(dst, wash)` per channel.
    Lighten,
}

/// The theme a painter resolves roles against. The values are the brand kit's
/// (`kiln-brand/final/tokens.css`); `fixtures/golden/scene/palette.json` holds them for the
/// TypeScript painter's test.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct Palette {
    /// `light` or `dark`.
    pub name: String,
    /// [`ColorRole::Canvas`].
    pub canvas: u32,
    /// [`ColorRole::Page`].
    pub page: u32,
    /// [`ColorRole::Ink`].
    pub ink: u32,
    /// [`ColorRole::Selection`].
    pub selection: u32,
    /// [`ColorRole::Caret`].
    pub caret: u32,
    /// [`ColorRole::Composition`].
    pub composition: u32,
    /// [`ColorRole::Squiggle`].
    pub squiggle: u32,
    /// [`ColorRole::Guide`].
    pub guide: u32,
    /// The spark at the caret's tip (brand signature).
    pub spark: u32,
    /// How the selection wash blends.
    pub selection_blend: SelectionBlend,
    /// The page shadow (`--kiln-shadow-page`), painted for [`crate::ShadowStyle::Page`].
    pub page_shadow: Vec<ShadowLayer>,
}

impl Palette {
    /// The light theme.
    #[must_use]
    pub fn light() -> Self {
        Self {
            name: "light".into(),
            canvas: 0xF6EF_E6FF,
            page: 0xFFFF_FFFF,
            ink: 0x2B1C_14FF,
            selection: 0xFFD9_B8FF,
            caret: 0x3159_C6FF,
            composition: 0x2B1C_14FF,
            squiggle: 0xD93D_12FF,
            guide: 0xCDB8_A0FF,
            spark: 0xFF6A_26FF,
            selection_blend: SelectionBlend::Darken,
            page_shadow: vec![
                ShadowLayer {
                    dx: 0.0,
                    dy: 1.0,
                    blur: 2.0,
                    spread: 0.0,
                    rgba: 0x3C1E_0A14,
                },
                ShadowLayer {
                    dx: 0.0,
                    dy: 12.0,
                    blur: 40.0,
                    spread: 0.0,
                    rgba: 0x3C1E_0A1A,
                },
            ],
        }
    }

    /// The dark theme (tuned, not inverted; the page glows faintly ember).
    #[must_use]
    pub fn dark() -> Self {
        Self {
            name: "dark".into(),
            canvas: 0x1711_0EFF,
            page: 0x221A_15FF,
            ink: 0xF5EB_DDFF,
            selection: 0x5A2A_14FF,
            caret: 0x97AE_F4FF,
            composition: 0xF5EB_DDFF,
            squiggle: 0xFF8A_7AFF,
            guide: 0x4B3A_2FFF,
            spark: 0xFF6A_26FF,
            selection_blend: SelectionBlend::Lighten,
            page_shadow: vec![
                ShadowLayer {
                    dx: 0.0,
                    dy: 30.0,
                    blur: 80.0,
                    spread: -20.0,
                    rgba: 0xFF6A_2638,
                },
                ShadowLayer {
                    dx: 0.0,
                    dy: 2.0,
                    blur: 10.0,
                    spread: 0.0,
                    rgba: 0x0000_0080,
                },
            ],
        }
    }

    /// The palette called `name` (`light` or `dark`).
    #[must_use]
    pub fn named(name: &str) -> Option<Self> {
        match name {
            "light" => Some(Self::light()),
            "dark" => Some(Self::dark()),
            _ => None,
        }
    }

    /// The colour of a role, `0xRRGGBBAA`.
    #[must_use]
    pub fn role(&self, role: ColorRole) -> u32 {
        match role {
            ColorRole::Canvas => self.canvas,
            ColorRole::Page => self.page,
            ColorRole::Ink => self.ink,
            ColorRole::Selection => self.selection,
            ColorRole::Caret => self.caret,
            ColorRole::Composition => self.composition,
            ColorRole::Squiggle => self.squiggle,
            ColorRole::Guide => self.guide,
        }
    }

    /// The colour of a paint, `0xRRGGBBAA`.
    #[must_use]
    pub fn resolve(&self, paint: Paint) -> u32 {
        match paint {
            Paint::Rgba(c) => c,
            Paint::Role(r) => self.role(r),
        }
    }

    /// How far a page shadow reaches beyond the page on each side: `(left, top, right, bottom)`.
    #[must_use]
    pub fn page_shadow_reach(&self) -> (f32, f32, f32, f32) {
        self.page_shadow
            .iter()
            .fold((0.0, 0.0, 0.0, 0.0), |acc, l| {
                let r = (l.blur + l.spread).max(0.0);
                (
                    acc.0.max(r - l.dx),
                    acc.1.max(r - l.dy),
                    acc.2.max(r + l.dx),
                    acc.3.max(r + l.dy),
                )
            })
    }
}

/// Splits `0xRRGGBBAA` into its channels.
#[must_use]
pub fn channels(rgba: u32) -> [u8; 4] {
    rgba.to_be_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_resolve_per_theme() {
        let light = Palette::light();
        let dark = Palette::dark();
        assert_eq!(light.resolve(Paint::Role(ColorRole::Page)), 0xFFFF_FFFF);
        assert_eq!(dark.resolve(Paint::Role(ColorRole::Page)), 0x221A_15FF);
        assert_eq!(light.resolve(Paint::Rgba(0x1234_5678)), 0x1234_5678);
        assert_eq!(
            Paint::from_color(kiln_common::Color::rgba(1, 2, 3, 4)),
            Paint::Rgba(0x0102_0304)
        );
        assert_eq!(channels(0x0102_0304), [1, 2, 3, 4]);
        assert_eq!(Palette::named("dark"), Some(dark));
        assert_eq!(Palette::named("sepia"), None);
    }

    #[test]
    fn shadow_reach_covers_every_layer() {
        let (l, t, r, b) = Palette::light().page_shadow_reach();
        assert_eq!((l, t, r, b), (40.0, 28.0, 40.0, 52.0));
        let (_, t, _, b) = Palette::dark().page_shadow_reach();
        assert_eq!((t, b), (30.0, 90.0));
    }

    /// The TypeScript painter reads the same values from this fixture (its Vitest compares them).
    #[test]
    fn palette_fixture_is_current() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/golden/scene/palette.json");
        let expected = serde_json::to_string_pretty(&[Palette::light(), Palette::dark()])
            .expect("json")
            + "\n";
        if std::env::var_os("KILN_UPDATE_GOLDEN").is_some() {
            std::fs::write(&path, &expected).expect("write palette.json");
        }
        let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            on_disk, expected,
            "fixtures/golden/scene/palette.json is stale: rerun with KILN_UPDATE_GOLDEN=1"
        );
    }
}
