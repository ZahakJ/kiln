// SPDX-License-Identifier: AGPL-3.0-or-later
//! The outline gutters (Data ▸ Group, Subtotal), as Excel draws them beside the headings: the
//! level buttons 1, 2, 3… in the corner, a bracket along each group of rows or columns, and the
//! − or + button at the group's summary row or column. A gutter is as wide as the deepest level
//! needs (one step per button); the headings' numbers and letters move past it.

use kiln_scene::{Bounds, ColorRole, DrawOp, Paint};
use kiln_text::{ParagraphSpec, SpanStyle, TextEngine};

use crate::geom::{SheetGeometry, Snap, View};
use crate::glyphs::GlyphCache;

/// One step of a gutter: a level button and its brackets.
pub const OUTLINE_STEP: f32 = 16.0;
/// The side of a level or ± button.
pub const OUTLINE_BOX: f32 = 12.0;
/// The margin before the first step.
const LEAD: f32 = 2.0;

/// The outline levels of a sheet's rows and columns, as the gutters draw them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OutlineBands {
    /// Rows with an outline level (ascending), with the level.
    pub rows: Vec<(u32, u8)>,
    /// Columns with an outline level (ascending), with the level.
    pub cols: Vec<(u32, u8)>,
    /// Summary rows sit below their detail (otherwise above).
    pub summary_below: bool,
    /// Summary columns sit right of their detail (otherwise left).
    pub summary_right: bool,
}

/// What a click on a gutter does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineHit {
    /// A level button: show the levels up to it.
    Level {
        /// The columns' buttons (otherwise the rows').
        columns: bool,
        /// The level (1 = only the outermost summaries).
        level: u8,
    },
    /// A group's ± button at its summary row or column.
    Toggle {
        /// Columns.
        columns: bool,
        /// The summary row or column.
        index: u32,
    },
}

fn level_in(list: &[(u32, u8)], i: u32) -> u8 {
    list.binary_search_by_key(&i, |e| e.0)
        .map_or(0, |at| list[at].1)
}

fn deepest(list: &[(u32, u8)]) -> u8 {
    list.iter().map(|e| e.1).max().unwrap_or(0)
}

/// The width (or height) of a gutter for `levels`: one step per button (`levels + 1`).
#[must_use]
pub fn gutter(levels: u8) -> f32 {
    if levels == 0 {
        0.0
    } else {
        LEAD * 2.0 + f32::from(levels + 1) * OUTLINE_STEP
    }
}

/// The centre of step `i` (0-based) across a gutter.
#[must_use]
pub fn step_centre(i: u8) -> f32 {
    LEAD + f32::from(i) * OUTLINE_STEP + OUTLINE_STEP / 2.0
}

impl OutlineBands {
    /// The outline of a sheet.
    #[must_use]
    pub fn of(sheet: &kiln_sheet::Sheet) -> OutlineBands {
        let pick = |e: (u32, u8)| (e.1 > 0).then_some(e);
        OutlineBands {
            rows: sheet
                .rows()
                .entries()
                .filter_map(|(i, p)| pick((i, p.outline_level)))
                .collect(),
            cols: sheet
                .cols()
                .entries()
                .filter_map(|(i, p)| pick((i, p.outline_level)))
                .collect(),
            summary_below: sheet.meta().summary_below,
            summary_right: sheet.meta().summary_right,
        }
    }

    /// The outline level of a row.
    #[must_use]
    pub fn row_level(&self, r: u32) -> u8 {
        level_in(&self.rows, r)
    }

    /// The outline level of a column.
    #[must_use]
    pub fn col_level(&self, c: u32) -> u8 {
        level_in(&self.cols, c)
    }

    /// The deepest row level.
    #[must_use]
    pub fn row_levels(&self) -> u8 {
        deepest(&self.rows)
    }

    /// The deepest column level.
    #[must_use]
    pub fn col_levels(&self) -> u8 {
        deepest(&self.cols)
    }

    /// The width of the row gutter (left of the row numbers).
    #[must_use]
    pub fn row_gutter(&self) -> f32 {
        gutter(self.row_levels())
    }

    /// The height of the column gutter (above the column letters).
    #[must_use]
    pub fn col_gutter(&self) -> f32 {
        gutter(self.col_levels())
    }

    fn level(&self, columns: bool, i: u32) -> u8 {
        if columns {
            self.col_level(i)
        } else {
            self.row_level(i)
        }
    }

    /// The detail index next to a summary (the one its group ends or starts with).
    #[must_use]
    pub fn detail_of(&self, columns: bool, summary: u32) -> Option<u32> {
        let after = if columns {
            self.summary_right
        } else {
            self.summary_below
        };
        if after {
            summary.checked_sub(1)
        } else {
            summary.checked_add(1)
        }
    }

    /// The levels whose groups have their summary at `i`: `level(i) + 1 ..= level(detail)`.
    #[must_use]
    pub fn summary_levels(&self, columns: bool, i: u32) -> std::ops::RangeInclusive<u8> {
        let own = self.level(columns, i);
        let detail = self
            .detail_of(columns, i)
            .map_or(0, |d| self.level(columns, d));
        (own + 1)..=detail
    }
}

#[allow(clippy::too_many_arguments)]
fn digit(
    engine: &mut TextEngine,
    glyphs: &mut GlyphCache,
    text: &str,
    centre: (f32, f32),
    tile: (f32, f32),
    out: &mut Vec<DrawOp>,
) {
    let mut style = SpanStyle::new("Calibri", 10.0);
    style.kerning = false;
    let layout = engine.layout(&ParagraphSpec::plain(text, style, 1.0e6));
    let w = layout.lines.iter().map(|l| l.width).fold(0.0, f32::max);
    let s = glyphs.scale();
    let ox = (centre.0 - w / 2.0) * s - (tile.0 * s).round();
    let oy = (centre.1 - layout.height / 2.0) * s - (tile.1 * s).round();
    for line in &layout.lines {
        for run in &line.runs {
            glyphs.push_run(
                engine.fonts(),
                run,
                (ox, oy),
                Paint::Role(ColorRole::Muted),
                out,
            );
        }
    }
}

/// A button box centred at `c` (tile coordinates).
fn button(c: (f32, f32), px: f32, out: &mut Vec<DrawOp>) -> Bounds {
    let b = Bounds::new(
        (c.0 - OUTLINE_BOX / 2.0).round(),
        (c.1 - OUTLINE_BOX / 2.0).round(),
        OUTLINE_BOX,
        OUTLINE_BOX,
    );
    out.push(DrawOp::FillRect {
        rect: b,
        paint: Paint::Role(ColorRole::Page),
        radius: 2.0,
    });
    out.push(DrawOp::StrokeRect {
        rect: b.inflate(-px / 2.0, -px / 2.0),
        paint: Paint::Role(ColorRole::Line),
        width: px,
        radius: 2.0,
    });
    b
}

/// The − (or, `plus`, +) of a group's button.
fn sign(b: Bounds, plus: bool, px: f32, out: &mut Vec<DrawOp>) {
    let ink = Paint::Role(ColorRole::Muted);
    let (cx, cy) = (b.x + b.w / 2.0, b.y + b.h / 2.0);
    out.push(DrawOp::FillRect {
        rect: Bounds::new(b.x + 3.0, (cy - px / 2.0).round(), b.w - 6.0, px),
        paint: ink,
        radius: 0.0,
    });
    if plus {
        out.push(DrawOp::FillRect {
            rect: Bounds::new((cx - px / 2.0).round(), b.y + 3.0, px, b.h - 6.0),
            paint: ink,
            radius: 0.0,
        });
    }
}

fn line(rect: Bounds, out: &mut Vec<DrawOp>) {
    if rect.w > 0.0 && rect.h > 0.0 {
        out.push(DrawOp::FillRect {
            rect,
            paint: Paint::Role(ColorRole::Muted),
            radius: 0.0,
        });
    }
}

/// The row gutter's brackets and ± buttons over a row header tile (`region` in sheet pixels,
/// its x the header band; ops in tile coordinates).
pub fn row_gutter_ops(geo: &SheetGeometry, region: Bounds, snap: Snap, out: &mut Vec<DrawOp>) {
    let o = &geo.outline;
    if o.rows.is_empty() {
        return;
    }
    let px = snap.px(1.0);
    let (r0, r1) = geo
        .rows
        .indices_in(f64::from(region.y), f64::from(region.bottom()));
    for r in r0..=r1 {
        if geo.rows.is_hidden(r) {
            continue;
        }
        let (y, h) = geo.rows.span(r);
        let y0 = snap.at(y) - region.y;
        let y1 = snap.at(y + h) - region.y;
        let level = o.row_level(r);
        for k in 1..=level {
            let x = (step_centre(k - 1) - px / 2.0).round();
            let first = r == 0 || o.row_level(r - 1) < k;
            let last = o.row_level(r + 1) < k;
            let (top, bottom) = match (o.summary_below, first, last) {
                // The bracket's open end: a tick across.
                (true, true, _) | (false, _, true) => {
                    let at = if o.summary_below {
                        y0 + 2.0
                    } else {
                        y1 - 2.0 - px
                    };
                    line(Bounds::new(x, at, OUTLINE_STEP / 2.0 - 2.0, px), out);
                    if o.summary_below {
                        (y0 + 2.0, y1)
                    } else {
                        (y0, y1 - 2.0)
                    }
                }
                _ => (y0, y1),
            };
            line(Bounds::new(x, top, px, bottom - top), out);
        }
        for k in o.summary_levels(false, r) {
            let c = (step_centre(k - 1), (y0 + y1) / 2.0);
            let expanded = o
                .detail_of(false, r)
                .is_some_and(|d| !geo.rows.is_hidden(d));
            if expanded {
                let x = (step_centre(k - 1) - px / 2.0).round();
                if o.summary_below {
                    line(Bounds::new(x, y0, px, c.1 - OUTLINE_BOX / 2.0 - y0), out);
                } else {
                    let b = c.1 + OUTLINE_BOX / 2.0;
                    line(Bounds::new(x, b, px, y1 - b), out);
                }
            }
            let b = button(c, px, out);
            sign(b, !expanded, px, out);
        }
    }
}

/// The column gutter's brackets and ± buttons over a column header tile.
pub fn col_gutter_ops(geo: &SheetGeometry, region: Bounds, snap: Snap, out: &mut Vec<DrawOp>) {
    let o = &geo.outline;
    if o.cols.is_empty() {
        return;
    }
    let px = snap.px(1.0);
    let (c0, c1) = geo
        .cols
        .indices_in(f64::from(region.x), f64::from(region.right()));
    for c in c0..=c1 {
        if geo.cols.is_hidden(c) {
            continue;
        }
        let (x, w) = geo.cols.span(c);
        let x0 = snap.at(x) - region.x;
        let x1 = snap.at(x + w) - region.x;
        let level = o.col_level(c);
        for k in 1..=level {
            let y = (step_centre(k - 1) - px / 2.0).round();
            let first = c == 0 || o.col_level(c - 1) < k;
            let last = o.col_level(c + 1) < k;
            let (left, right) = match (o.summary_right, first, last) {
                (true, true, _) | (false, _, true) => {
                    let at = if o.summary_right {
                        x0 + 2.0
                    } else {
                        x1 - 2.0 - px
                    };
                    line(Bounds::new(at, y, px, OUTLINE_STEP / 2.0 - 2.0), out);
                    if o.summary_right {
                        (x0 + 2.0, x1)
                    } else {
                        (x0, x1 - 2.0)
                    }
                }
                _ => (x0, x1),
            };
            line(Bounds::new(left, y, right - left, px), out);
        }
        for k in o.summary_levels(true, c) {
            let ctr = ((x0 + x1) / 2.0, step_centre(k - 1));
            let expanded = o.detail_of(true, c).is_some_and(|d| !geo.cols.is_hidden(d));
            if expanded {
                let y = (step_centre(k - 1) - px / 2.0).round();
                if o.summary_right {
                    line(Bounds::new(x0, y, ctr.0 - OUTLINE_BOX / 2.0 - x0, px), out);
                } else {
                    let r = ctr.0 + OUTLINE_BOX / 2.0;
                    line(Bounds::new(r, y, x1 - r, px), out);
                }
            }
            let b = button(ctr, px, out);
            sign(b, !expanded, px, out);
        }
    }
}

/// The level buttons in the corner box (`w` × `h`, tile coordinates): the rows' along the
/// bottom of the row gutter, the columns' down the right of the column gutter.
pub fn corner_buttons(
    geo: &SheetGeometry,
    (w, h): (f32, f32),
    snap: Snap,
    engine: &mut TextEngine,
    glyphs: &mut GlyphCache,
    out: &mut Vec<DrawOp>,
) {
    let px = snap.px(1.0);
    let o = &geo.outline;
    let gw = o.row_gutter();
    let gh = o.col_gutter();
    if gw > 0.0 {
        let cy = gh + (h - gh) / 2.0;
        for i in 0..=o.row_levels() {
            let c = (step_centre(i), cy);
            button(c, px, out);
            digit(engine, glyphs, &(i + 1).to_string(), c, (0.0, 0.0), out);
        }
    }
    if gh > 0.0 {
        let cx = gw + (w - gw) / 2.0;
        for i in 0..=o.col_levels() {
            let c = (cx, step_centre(i));
            button(c, px, out);
            digit(engine, glyphs, &(i + 1).to_string(), c, (0.0, 0.0), out);
        }
    }
}

/// The side of a button's hit square for a mouse: the button and a pixel round it.
pub const OUTLINE_HIT: f32 = OUTLINE_BOX + 2.0;
/// The side of a button's hit square for a finger (a coarse pointer). Where two buttons'
/// squares overlap (the level buttons, one step apart), the nearer button takes the tap.
pub const OUTLINE_TOUCH_HIT: f32 = 24.0;

/// The nearest of `centres` to `v` within `half`, by distance.
fn nearest(v: f32, half: f32, centres: impl Iterator<Item = (u8, f32)>) -> Option<(u8, f32)> {
    centres
        .map(|(k, c)| (k, (v - c).abs()))
        .filter(|&(_, d)| d <= half)
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

/// What lies under a surface point in the gutters or on the corner's level buttons. `coarse`
/// (a finger) gives each button a [`OUTLINE_TOUCH_HIT`] square instead of [`OUTLINE_HIT`]; the
/// drawing does not change.
#[must_use]
pub fn outline_hit(
    geo: &SheetGeometry,
    view: &View,
    x: f32,
    y: f32,
    header_w: f32,
    coarse: bool,
) -> Option<OutlineHit> {
    let o = &geo.outline;
    let z = view.zoom.max(0.01);
    let (sx, sy) = (x / z, y / z);
    let gw = o.row_gutter();
    let gh = o.col_gutter();
    let hh = geo.header_h();
    let half = if coarse {
        OUTLINE_TOUCH_HIT
    } else {
        OUTLINE_HIT
    } / 2.0;
    let steps = |levels: u8| (0..=levels).map(|i| (i, step_centre(i)));
    // The corner: level buttons.
    if sx < header_w && sy < hh {
        if gw > 0.0
            && sx < gw
            && sy >= gh
            && let Some((i, _)) = nearest(sx, half, steps(o.row_levels()))
        {
            return Some(OutlineHit::Level {
                columns: false,
                level: i + 1,
            });
        }
        if gh > 0.0
            && sy < gh
            && sx >= gw
            && let Some((i, _)) = nearest(sy, half, steps(o.col_levels()))
        {
            return Some(OutlineHit::Level {
                columns: true,
                level: i + 1,
            });
        }
        return None;
    }
    // A group's button: the nearest of the buttons on the line under the point and, for a
    // finger, on the lines either side (a row is often lower than the target).
    let toggle = |columns: bool, across: f32, along: f32, line: u32| -> Option<u32> {
        let axis = if columns { &geo.cols } else { &geo.rows };
        let reach = if coarse { 1 } else { 0 };
        let mut best: Option<(u32, f32)> = None;
        for l in line.saturating_sub(reach)..=line.saturating_add(reach) {
            if l >= axis.len() {
                break;
            }
            let (start, size) = axis.span(l);
            #[allow(clippy::cast_possible_truncation)]
            let mid = (start + size / 2.0) as f32;
            let d_along = (along - mid).abs();
            if d_along > half {
                continue;
            }
            let levels = o.summary_levels(columns, l);
            let Some((_, d_across)) =
                nearest(across, half, levels.map(|k| (k, step_centre(k - 1))))
            else {
                continue;
            };
            let d = d_along.max(d_across);
            if best.is_none_or(|(_, b)| d < b) {
                best = Some((l, d));
            }
        }
        best.map(|b| b.0)
    };
    // The row gutter.
    if gw > 0.0 && sx < gw && sy >= hh {
        let (_, _, ry) = geo.to_sheet(view, header_w * z + 0.5, y, header_w)?;
        let r = geo.rows.index_at(f64::from(ry));
        return toggle(false, sx, ry, r).map(|index| OutlineHit::Toggle {
            columns: false,
            index,
        });
    }
    // The column gutter.
    if gh > 0.0 && sy < gh && sx >= header_w {
        let (_, cx, _) = geo.to_sheet(view, x, hh * z + 0.5, header_w)?;
        let c = geo.cols.index_at(f64::from(cx));
        return toggle(true, sy, cx, c).map(|index| OutlineHit::Toggle {
            columns: true,
            index,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::axis::AxisIndex;

    fn geo(rows: Vec<(u32, u8)>) -> SheetGeometry {
        SheetGeometry {
            rows: AxisIndex::new(100, 20.0),
            cols: AxisIndex::new(100, 64.0),
            freeze_rows: 0,
            freeze_cols: 0,
            headings: true,
            merges: Vec::new(),
            outline: OutlineBands {
                rows,
                cols: Vec::new(),
                summary_below: true,
                summary_right: true,
            },
            rtl: false,
        }
    }

    #[test]
    fn gutters_grow_with_the_levels_and_push_the_numbers() {
        // Subtotal's outline: details at level 2, totals at 1, the grand total at 0.
        let g = geo(vec![(1, 2), (2, 2), (3, 1), (4, 2), (5, 1)]);
        assert_eq!(g.outline.row_levels(), 2);
        assert_eq!(g.outline.row_gutter(), 4.0 + 3.0 * OUTLINE_STEP);
        assert_eq!(
            g.header_w(30),
            crate::geom::HEADER_W_MIN + g.outline.row_gutter()
        );
        assert_eq!(g.header_h(), crate::geom::HEADER_H, "no column outline");
        // Row 3 sums rows 1–2 (level 2); row 6 sums rows 1–5 (level 1).
        assert_eq!(g.outline.summary_levels(false, 3), 2..=2);
        assert_eq!(g.outline.summary_levels(false, 6), 1..=1);
        assert!(g.outline.summary_levels(false, 2).is_empty());
    }

    #[test]
    fn hits_find_the_level_buttons_and_the_group_buttons() {
        let g = geo(vec![(1, 2), (2, 2), (3, 1), (4, 2), (5, 1)]);
        let view = View {
            width: 800.0,
            height: 600.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 1.0,
        };
        let hw = g.header_w(30);
        let hh = g.header_h();
        // Button 2 in the corner.
        assert_eq!(
            outline_hit(&g, &view, step_centre(1), hh / 2.0, hw, false),
            Some(OutlineHit::Level {
                columns: false,
                level: 2
            })
        );
        // Row 3's − button (level 2, second step), at the row's middle.
        let (y, h) = g.rows.span(3);
        let (y, h) = (y as f32, h as f32);
        assert_eq!(
            outline_hit(&g, &view, step_centre(1), hh + y + h / 2.0, hw, false),
            Some(OutlineHit::Toggle {
                columns: false,
                index: 3
            })
        );
        // The bracket beside a detail row is not a button.
        let (y, h) = g.rows.span(1);
        let (y, h) = (y as f32, h as f32);
        assert_eq!(
            outline_hit(&g, &view, step_centre(1), hh + y + h / 2.0, hw, false),
            None
        );
    }
    #[test]
    fn a_finger_gets_a_24_px_target_and_a_mouse_keeps_the_button() {
        let g = geo(vec![(1, 2), (2, 2), (3, 1), (4, 2), (5, 1)]);
        let view = View {
            width: 800.0,
            height: 600.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 1.0,
        };
        let hw = g.header_w(30);
        let hh = g.header_h();
        let off = OUTLINE_TOUCH_HIT / 2.0 - 1.0;
        // Level button 1, 11 px left of its centre (the gutter's edge).
        let x = step_centre(0) - off.min(step_centre(0));
        assert_eq!(outline_hit(&g, &view, x, hh / 2.0, hw, false), None);
        assert_eq!(
            outline_hit(&g, &view, x, hh / 2.0, hw, true),
            Some(OutlineHit::Level {
                columns: false,
                level: 1
            })
        );
        // Between buttons 1 and 2 the nearer one takes the tap.
        assert_eq!(
            outline_hit(&g, &view, step_centre(1) - 3.0, hh / 2.0, hw, true),
            Some(OutlineHit::Level {
                columns: false,
                level: 2
            })
        );
        // Row 3's − button, 11 px below its centre (over row 4) and 11 px right of it.
        let (y, h) = g.rows.span(3);
        let mid = hh + y as f32 + h as f32 / 2.0;
        for (x, y) in [
            (step_centre(1), mid + off),
            (step_centre(1) + off, mid),
            (step_centre(1) - off, mid - off),
        ] {
            assert_eq!(outline_hit(&g, &view, x, y, hw, false), None, "{x}, {y}");
            assert_eq!(
                outline_hit(&g, &view, x, y, hw, true),
                Some(OutlineHit::Toggle {
                    columns: false,
                    index: 3
                }),
                "{x}, {y}"
            );
        }
        // Beyond the target: nothing.
        assert_eq!(
            outline_hit(&g, &view, step_centre(1), mid + OUTLINE_TOUCH_HIT, hw, true),
            None
        );
    }
}
