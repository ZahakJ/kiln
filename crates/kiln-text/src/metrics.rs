// SPDX-License-Identifier: AGPL-3.0-or-later
//! Font metrics with Word's line-height rule and a cache keyed by (font, size).
//!
//! Word sizes a "single" line from the OS/2 `usWinAscent`/`usWinDescent` values, unless the font sets
//! `USE_TYPO_METRICS` (`fsSelection` bit 7), in which case it uses `sTypoAscender`, `sTypoDescender` and
//! `sTypoLineGap`. Fonts without an OS/2 table (or with zero Windows metrics) fall back to `hhea`.

use std::collections::HashMap;

use rustc_hash::FxBuildHasher;

use skrifa::raw::TableProvider;
use skrifa::raw::tables::os2::SelectionFlags;

/// Unscaled metrics of one font, in font units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct UnitMetrics {
    /// Units per em.
    pub units_per_em: u16,
    /// Ascent above the baseline (positive).
    pub ascent: f32,
    /// Descent below the baseline (positive).
    pub descent: f32,
    /// Extra gap added to a single line (non-zero only with `USE_TYPO_METRICS`).
    pub line_gap: f32,
    /// Whether the font asked for typographic metrics.
    pub use_typo_metrics: bool,
    /// Underline position (negative = below the baseline, `post` convention).
    pub underline_position: f32,
    /// Underline thickness.
    pub underline_thickness: f32,
    /// Strikeout position above the baseline.
    pub strikeout_position: f32,
    /// Strikeout thickness.
    pub strikeout_thickness: f32,
    /// x-height, when the font records it.
    pub x_height: Option<f32>,
    /// Cap height, when the font records it.
    pub cap_height: Option<f32>,
}

impl UnitMetrics {
    /// Reads the metrics of the font at `index` in `data`. Returns `None` for data that is not a font.
    #[must_use]
    pub fn read(data: &[u8], index: u32) -> Option<Self> {
        let font = skrifa::FontRef::from_index(data, index).ok()?;
        let upem = font.head().ok()?.units_per_em();
        if upem == 0 {
            return None;
        }
        let os2 = font.os2().ok();
        let hhea = font.hhea().ok();
        let (hhea_ascent, hhea_descent, hhea_gap) =
            hhea.map_or((f32::from(upem) * 0.8, f32::from(upem) * 0.2, 0.0), |h| {
                (
                    f32::from(h.ascender().to_i16()),
                    -f32::from(h.descender().to_i16()),
                    f32::from(h.line_gap().to_i16()),
                )
            });
        let use_typo = os2
            .as_ref()
            .is_some_and(|o| o.fs_selection().contains(SelectionFlags::USE_TYPO_METRICS));
        let (ascent, descent, line_gap) = match &os2 {
            Some(o) if use_typo => (
                f32::from(o.s_typo_ascender()),
                -f32::from(o.s_typo_descender()),
                f32::from(o.s_typo_line_gap()).max(0.0),
            ),
            Some(o) if o.us_win_ascent() > 0 || o.us_win_descent() > 0 => (
                f32::from(o.us_win_ascent()),
                f32::from(o.us_win_descent()),
                0.0,
            ),
            _ => (hhea_ascent, hhea_descent, hhea_gap.max(0.0)),
        };
        let post = font.post().ok();
        let (underline_position, underline_thickness) =
            post.map_or((-f32::from(upem) * 0.1, f32::from(upem) * 0.05), |p| {
                (
                    f32::from(p.underline_position().to_i16()),
                    f32::from(p.underline_thickness().to_i16()),
                )
            });
        let (strikeout_position, strikeout_thickness) =
            os2.as_ref()
                .map_or((f32::from(upem) * 0.25, underline_thickness), |o| {
                    (
                        f32::from(o.y_strikeout_position()),
                        f32::from(o.y_strikeout_size()),
                    )
                });
        let x_height = os2
            .as_ref()
            .and_then(|o| o.sx_height())
            .filter(|v| *v > 0)
            .map(f32::from);
        let cap_height = os2
            .as_ref()
            .and_then(|o| o.s_cap_height())
            .filter(|v| *v > 0)
            .map(f32::from);
        Some(Self {
            units_per_em: upem,
            ascent,
            descent,
            line_gap,
            use_typo_metrics: use_typo,
            underline_position,
            underline_thickness: if underline_thickness > 0.0 {
                underline_thickness
            } else {
                f32::from(upem) * 0.05
            },
            strikeout_position,
            strikeout_thickness: if strikeout_thickness > 0.0 {
                strikeout_thickness
            } else {
                f32::from(upem) * 0.05
            },
            x_height,
            cap_height,
        })
    }

    /// Scales the metrics to a font size in pixels.
    #[must_use]
    pub fn scale(&self, size: f32) -> ScaledMetrics {
        let k = size / f32::from(self.units_per_em);
        ScaledMetrics {
            size,
            ascent: self.ascent * k,
            descent: self.descent * k,
            line_gap: self.line_gap * k,
            underline_offset: -self.underline_position * k,
            underline_thickness: self.underline_thickness * k,
            strikeout_offset: self.strikeout_position * k,
            strikeout_thickness: self.strikeout_thickness * k,
            x_height: self.x_height.map(|v| v * k),
            cap_height: self.cap_height.map(|v| v * k),
        }
    }
}

/// Metrics scaled to a font size, in pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaledMetrics {
    /// The font size these metrics were scaled to.
    pub size: f32,
    /// Ascent above the baseline.
    pub ascent: f32,
    /// Descent below the baseline (positive).
    pub descent: f32,
    /// Gap added below the descent of a single line.
    pub line_gap: f32,
    /// Distance from the baseline down to the top of the underline.
    pub underline_offset: f32,
    /// Underline thickness.
    pub underline_thickness: f32,
    /// Distance from the baseline up to the strikeout's top edge.
    pub strikeout_offset: f32,
    /// Strikeout thickness.
    pub strikeout_thickness: f32,
    /// x-height.
    pub x_height: Option<f32>,
    /// Cap height.
    pub cap_height: Option<f32>,
}

impl ScaledMetrics {
    /// Word's single line height: ascent + descent (+ the typographic line gap when the font uses
    /// typographic metrics).
    #[must_use]
    pub fn line_height(&self) -> f32 {
        self.ascent + self.descent + self.line_gap
    }
}

/// Cache of unit metrics per font and scaled metrics per (font, size).
#[derive(Debug, Default)]
pub(crate) struct MetricsCache {
    units: Vec<Option<UnitMetrics>>,
    scaled: HashMap<(u32, u32), ScaledMetrics, FxBuildHasher>,
}

impl MetricsCache {
    /// Records the unit metrics of font `id` (fonts are numbered densely from 0).
    pub(crate) fn insert_font(&mut self, id: u32, metrics: Option<UnitMetrics>) {
        let index = id as usize;
        if self.units.len() <= index {
            self.units.resize(index + 1, None);
        }
        self.units[index] = metrics;
    }

    pub(crate) fn units(&self, id: u32) -> Option<&UnitMetrics> {
        self.units.get(id as usize).and_then(Option::as_ref)
    }

    /// Scaled metrics for (font, size), computed once.
    pub(crate) fn scaled(&mut self, id: u32, size: f32) -> ScaledMetrics {
        let key = (id, size.to_bits());
        if let Some(m) = self.scaled.get(&key) {
            return *m;
        }
        let m = self.units(id).map_or_else(
            || ScaledMetrics {
                size,
                ascent: size * 0.8,
                descent: size * 0.2,
                line_gap: 0.0,
                underline_offset: size * 0.1,
                underline_thickness: size * 0.05,
                strikeout_offset: size * 0.25,
                strikeout_thickness: size * 0.05,
                x_height: None,
                cap_height: None,
            },
            |u| u.scale(size),
        );
        self.scaled.insert(key, m);
        m
    }

    pub(crate) fn scaled_len(&self) -> usize {
        self.scaled.len()
    }
}
