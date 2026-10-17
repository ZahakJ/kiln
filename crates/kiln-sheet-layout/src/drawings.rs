// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drawings on a sheet (charts): where an anchor puts them in sheet pixels, the anchor for a
//! placement, and their Scene tiles — one tile per chart above the cells, drawn by `kiln-charts`
//! with the sheet's glyph atlas so chart text is as crisp as cell text.

use kiln_charts::model::ChartSpace;
use kiln_charts::palette::{ChartColors, ColorMode};
use kiln_charts::text::{GlyphSink, OutlineSink, TextMeasurer};
use kiln_ooxml::drawingml::theme::Theme;
use kiln_scene::{Bounds, DrawOp, Paint};
use kiln_sheet::meta::{Anchor, AnchorPoint, EditAs};
use kiln_text::FontStore;

use crate::geom::SheetGeometry;
use crate::glyphs::GlyphCache;

/// EMU per CSS pixel (914,400 EMU per inch, 96 px per inch).
pub const EMU_PER_PX: f64 = 9_525.0;

/// Excel's size for a new chart: 5 × 3 inches.
pub const NEW_CHART_SIZE: (f32, f32) = (480.0, 288.0);

/// The smallest a chart can be made by dragging.
pub const MIN_CHART_SIZE: f32 = 24.0;

fn point_px(geo: &SheetGeometry, p: &AnchorPoint) -> (f64, f64) {
    let col = p.col.min(geo.cols.len().saturating_sub(1));
    let row = p.row.min(geo.rows.len().saturating_sub(1));
    let (cx, cw) = geo.cols.span(col);
    let (ry, rh) = geo.rows.span(row);
    // Offsets are clamped to their cell, as Excel draws them.
    let dx = (p.col_off as f64 / EMU_PER_PX).clamp(0.0, cw.max(0.0));
    let dy = (p.row_off as f64 / EMU_PER_PX).clamp(0.0, rh.max(0.0));
    (cx + dx, ry + dy)
}

/// The rectangle an anchor covers, in sheet pixels at 100 %.
#[must_use]
pub fn anchor_rect(geo: &SheetGeometry, anchor: &Anchor) -> Bounds {
    let (x, y, w, h) = match anchor {
        Anchor::TwoCell { from, to, .. } => {
            let (x0, y0) = point_px(geo, from);
            let (x1, y1) = point_px(geo, to);
            (x0, y0, x1 - x0, y1 - y0)
        }
        Anchor::OneCell { from, ext } => {
            let (x0, y0) = point_px(geo, from);
            (x0, y0, ext.0 as f64 / EMU_PER_PX, ext.1 as f64 / EMU_PER_PX)
        }
        Anchor::Absolute { pos, ext } => (
            pos.0 as f64 / EMU_PER_PX,
            pos.1 as f64 / EMU_PER_PX,
            ext.0 as f64 / EMU_PER_PX,
            ext.1 as f64 / EMU_PER_PX,
        ),
    };
    #[allow(clippy::cast_possible_truncation)]
    Bounds::new(x as f32, y as f32, w.max(0.0) as f32, h.max(0.0) as f32)
}

fn point_at(geo: &SheetGeometry, x: f64, y: f64) -> AnchorPoint {
    let col = geo.cols.index_at(x.max(0.0));
    let row = geo.rows.index_at(y.max(0.0));
    let (cx, _) = geo.cols.span(col);
    let (ry, _) = geo.rows.span(row);
    #[allow(clippy::cast_possible_truncation)]
    AnchorPoint {
        row,
        col,
        row_off: ((y - ry).max(0.0) * EMU_PER_PX).round() as i64,
        col_off: ((x - cx).max(0.0) * EMU_PER_PX).round() as i64,
    }
}

/// The anchor that puts a drawing at `rect` (sheet pixels), keeping the kind of `like`
/// (two-cell anchors stay two-cell with their move-and-size behaviour).
#[must_use]
pub fn anchor_for(geo: &SheetGeometry, rect: Bounds, like: &Anchor) -> Anchor {
    let x = f64::from(rect.x.max(0.0));
    let y = f64::from(rect.y.max(0.0));
    let w = f64::from(rect.w.max(MIN_CHART_SIZE));
    let h = f64::from(rect.h.max(MIN_CHART_SIZE));
    #[allow(clippy::cast_possible_truncation)]
    let ext = (
        (w * EMU_PER_PX).round() as i64,
        (h * EMU_PER_PX).round() as i64,
    );
    match like {
        Anchor::TwoCell { edit_as, .. } => Anchor::TwoCell {
            from: point_at(geo, x, y),
            to: point_at(geo, x + w, y + h),
            edit_as: *edit_as,
        },
        Anchor::OneCell { .. } => Anchor::OneCell {
            from: point_at(geo, x, y),
            ext,
        },
        #[allow(clippy::cast_possible_truncation)]
        Anchor::Absolute { .. } => Anchor::Absolute {
            pos: (
                (x * EMU_PER_PX).round() as i64,
                (y * EMU_PER_PX).round() as i64,
            ),
            ext,
        },
    }
}

/// A new chart's anchor: moves and sizes with cells, as Excel's.
#[must_use]
pub fn new_chart_anchor(geo: &SheetGeometry, rect: Bounds) -> Anchor {
    anchor_for(
        geo,
        rect,
        &Anchor::TwoCell {
            from: AnchorPoint::default(),
            to: AnchorPoint::default(),
            edit_as: EditAs::TwoCell,
        },
    )
}

/// One chart for [`crate::GridScene::prepare`].
#[derive(Debug, Clone, Copy)]
pub struct ChartTile<'a> {
    /// The tile index (the drawing's number on the sheet).
    pub index: u32,
    /// Where it is, in sheet pixels.
    pub rect: Bounds,
    /// Changes whenever the chart, its size or the colours change.
    pub version: u64,
    /// The chart.
    pub chart: &'a ChartSpace,
}

/// How charts are coloured: the workbook's theme, and the app's light or dark theme.
#[derive(Debug, Clone, Copy, Default)]
pub struct ChartLook<'a> {
    /// The workbook's theme (accents, fonts); `None` uses Office's.
    pub theme: Option<&'a Theme>,
    /// The app's dark theme.
    pub dark: bool,
}

impl ChartLook<'_> {
    /// The colour mode on screen.
    #[must_use]
    pub fn mode(&self) -> ColorMode {
        ColorMode::Screen { dark: self.dark }
    }

    /// The resolved colours.
    #[must_use]
    pub fn colors(&self) -> ChartColors {
        ChartColors::new(self.theme, self.mode())
    }
}

/// Sends upright chart text to the sheet's glyph atlas, rotated text as outlines.
struct AtlasCacheSink<'a> {
    glyphs: &'a mut GlyphCache,
}

impl GlyphSink for AtlasCacheSink<'_> {
    fn push_run(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        rotation: Option<(f32, (f32, f32))>,
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        if rotation.is_some_and(|(deg, _)| deg.rem_euclid(360.0).abs() > 0.01) {
            OutlineSink.push_run(fonts, run, origin, rotation, paint, out);
            return;
        }
        let s = self.glyphs.scale();
        self.glyphs
            .push_run(fonts, run, (origin.0 * s, origin.1 * s), paint, out);
    }
}

/// The ops of a chart tile (chart pixels from the tile's top-left).
pub(crate) fn chart_ops(
    tile: &ChartTile<'_>,
    look: &ChartLook<'_>,
    text: &mut TextMeasurer,
    glyphs: &mut GlyphCache,
) -> Vec<DrawOp> {
    let mut sink = AtlasCacheSink { glyphs };
    let (_, ops) = kiln_charts::render::render(
        tile.chart,
        tile.rect.w,
        tile.rect.h,
        look.theme,
        look.mode(),
        text,
        &mut sink,
    );
    ops
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo() -> SheetGeometry {
        crate::geometry_of(&kiln_sheet::Workbook::new().sheets()[0], true)
    }

    #[test]
    fn anchors_and_rectangles_round_trip() {
        let g = geo();
        let r = Bounds::new(150.0, 47.0, 480.0, 288.0);
        let a = new_chart_anchor(&g, r);
        let back = anchor_rect(&g, &a);
        assert!(
            (back.x - r.x).abs() < 0.01 && (back.y - r.y).abs() < 0.01,
            "{back:?}"
        );
        assert!(
            (back.w - r.w).abs() < 0.01 && (back.h - r.h).abs() < 0.01,
            "{back:?}"
        );
        let Anchor::TwoCell { from, to, .. } = a else {
            panic!("two-cell");
        };
        // 64 px columns and 20 px rows: B3 + 22 px, 7 px.
        assert_eq!((from.col, from.row), (2, 2));
        assert_eq!(from.col_off, (22.0 * EMU_PER_PX) as i64);
        assert!(to.col > from.col && to.row > from.row);
        let one = anchor_for(
            &g,
            r,
            &Anchor::OneCell {
                from: AnchorPoint::default(),
                ext: (0, 0),
            },
        );
        assert_eq!(anchor_rect(&g, &one), back);
        // Too small a drag keeps a usable size.
        let tiny = anchor_rect(&g, &new_chart_anchor(&g, Bounds::new(0.0, 0.0, 2.0, 2.0)));
        assert!(tiny.w >= MIN_CHART_SIZE - 0.01 && tiny.h >= MIN_CHART_SIZE - 0.01);
    }
}
