// SPDX-License-Identifier: AGPL-3.0-or-later
//! The geometry of a sheet view: cell rectangles, headers, frozen panes and hit testing.
//!
//! **Coordinates.** Sheet pixels are CSS pixels at 100 % zoom with the origin at the top-left
//! corner of A1. Row and column headers live in the same space just outside the cells: the
//! column header band is `y ∈ [-header_h, 0)`, the row header band `x ∈ [-header_w, 0)`. The
//! surface shows up to four panes of cells (frozen rows and columns stay put while the rest
//! scrolls), each a viewport onto the one sheet space, so every pane paints from the same tiles.
//!
//! **Surface pixels** are CSS pixels of the surface element (headers included), at the view's
//! zoom. A [`View`] maps them to sheet pixels.

use kiln_scene::Bounds;
use kiln_sheet::Range;

use crate::axis::AxisIndex;

/// Height of the column header band at 100 % zoom.
pub const HEADER_H: f32 = 24.0;
/// Narrowest row header (row numbers up to four digits).
pub const HEADER_W_MIN: f32 = 44.0;
/// Pixels from a header border within which the pointer resizes.
pub const RESIZE_SLOP: f32 = 4.0;
/// Side of the fill handle.
pub const FILL_HANDLE: f32 = 9.0;

/// Rows and columns of one sheet, plus what the view adds around them.
#[derive(Debug, Clone, PartialEq)]
pub struct SheetGeometry {
    /// Row offsets.
    pub rows: AxisIndex,
    /// Column offsets.
    pub cols: AxisIndex,
    /// Frozen rows (above the split).
    pub freeze_rows: u32,
    /// Frozen columns (left of the split).
    pub freeze_cols: u32,
    /// Whether row and column headings show.
    pub headings: bool,
    /// Merged ranges.
    pub merges: Vec<Range>,
}

/// One of the four panes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Pane {
    /// Frozen rows and columns.
    TopLeft,
    /// Frozen rows, scrolling columns.
    TopRight,
    /// Scrolling rows, frozen columns.
    BottomLeft,
    /// Everything scrolls.
    BottomRight,
}

/// What lies under a point of the surface.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitTarget {
    /// A cell (the top-left cell of a merge).
    Cell {
        /// Row.
        row: u32,
        /// Column.
        col: u32,
    },
    /// The fill handle of the selection.
    FillHandle,
    /// A column header.
    ColumnHeader {
        /// Column.
        col: u32,
    },
    /// A row header.
    RowHeader {
        /// Row.
        row: u32,
    },
    /// The border after a column header (dragging resizes `col`).
    ColumnBorder {
        /// The column left of the border.
        col: u32,
    },
    /// The border below a row header.
    RowBorder {
        /// The row above the border.
        row: u32,
    },
    /// The corner box (Select All).
    Corner,
}

/// A view of a sheet: the surface size, the scroll position of the scrolling pane and the zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Surface width in CSS pixels.
    pub width: f32,
    /// Surface height in CSS pixels.
    pub height: f32,
    /// Sheet x at the left edge of the scrolling columns.
    pub scroll_x: f32,
    /// Sheet y at the top edge of the scrolling rows.
    pub scroll_y: f32,
    /// Zoom factor.
    pub zoom: f32,
}

/// Pixel-snapping at a device scale: edges land on device pixels so lines are crisp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Snap(pub f32);

impl Snap {
    /// `v` rounded to the nearest device pixel.
    #[must_use]
    pub fn at(&self, v: f64) -> f32 {
        let s = f64::from(if self.0 > 0.0 { self.0 } else { 1.0 });
        ((v * s).round() / s) as f32
    }

    /// One device pixel in document pixels (at least `min` document pixels).
    #[must_use]
    pub fn px(&self, min: f32) -> f32 {
        let s = if self.0 > 0.0 { self.0 } else { 1.0 };
        (min * s).round().max(1.0) / s
    }
}

impl SheetGeometry {
    /// Width of the row header for the rows a view shows (it grows with the row numbers).
    #[must_use]
    pub fn header_w(&self, last_row: u32) -> f32 {
        if !self.headings {
            return 0.0;
        }
        let digits = (u64::from(last_row) + 1).to_string().len().max(4);
        #[allow(clippy::cast_precision_loss)]
        let extra = (digits - 4) as f32 * 8.0;
        HEADER_W_MIN + extra
    }

    /// Height of the column header.
    #[must_use]
    pub fn header_h(&self) -> f32 {
        if self.headings { HEADER_H } else { 0.0 }
    }

    /// Width of the frozen columns.
    #[must_use]
    pub fn frozen_w(&self) -> f32 {
        self.cols.offset(self.freeze_cols) as f32
    }

    /// Height of the frozen rows.
    #[must_use]
    pub fn frozen_h(&self) -> f32 {
        self.rows.offset(self.freeze_rows) as f32
    }

    /// The rectangle of one cell (ignoring merges).
    #[must_use]
    pub fn cell_rect(&self, row: u32, col: u32) -> Bounds {
        let (x, w) = self.cols.span(col);
        let (y, h) = self.rows.span(row);
        Bounds::new(x as f32, y as f32, w as f32, h as f32)
    }

    /// The rectangle of a range.
    #[must_use]
    pub fn range_rect(&self, r: &Range) -> Bounds {
        let (x0, x1) = self.cols.range_span(r.c0, r.c1);
        let (y0, y1) = self.rows.range_span(r.r0, r.r1);
        Bounds::from_corners(x0 as f32, y0 as f32, x1 as f32, y1 as f32)
    }

    /// The merge containing a cell.
    #[must_use]
    pub fn merge_at(&self, row: u32, col: u32) -> Option<Range> {
        self.merges.iter().find(|m| m.contains(row, col)).copied()
    }

    /// The rectangle a cell shows in: its merge's, or its own.
    #[must_use]
    pub fn display_rect(&self, row: u32, col: u32) -> Bounds {
        match self.merge_at(row, col) {
            Some(m) => self.range_rect(&m),
            None => self.cell_rect(row, col),
        }
    }

    /// A range grown to whole merges (repeatedly, as a merge can pull in another).
    #[must_use]
    pub fn expand_to_merges(&self, mut r: Range) -> Range {
        loop {
            let mut grown = r;
            for m in &self.merges {
                if m.intersects(&grown) {
                    grown = grown.union(m);
                }
            }
            if grown == r {
                return r;
            }
            r = grown;
        }
    }

    /// The sheet area at the top-left of a pane, and the pane's rectangle on the surface.
    #[must_use]
    pub fn pane(&self, view: &View, pane: Pane, header_w: f32) -> (f32, f32, Bounds) {
        let z = view.zoom;
        let hx = header_w * z;
        let hy = self.header_h() * z;
        let fw = self.frozen_w() * z;
        let fh = self.frozen_h() * z;
        let (sx, sy) = self.clamp_scroll(view.scroll_x, view.scroll_y);
        let right = (view.width - hx - fw).max(0.0);
        let bottom = (view.height - hy - fh).max(0.0);
        match pane {
            Pane::TopLeft => (0.0, 0.0, Bounds::new(hx, hy, fw, fh)),
            Pane::TopRight => (sx, 0.0, Bounds::new(hx + fw, hy, right, fh)),
            Pane::BottomLeft => (0.0, sy, Bounds::new(hx, hy + fh, fw, bottom)),
            Pane::BottomRight => (sx, sy, Bounds::new(hx + fw, hy + fh, right, bottom)),
        }
    }

    /// The scroll position kept out of the frozen area.
    #[must_use]
    pub fn clamp_scroll(&self, x: f32, y: f32) -> (f32, f32) {
        let fw = self.frozen_w();
        let fh = self.frozen_h();
        let max_x = (self.cols.total() as f32 - 1.0).max(fw);
        let max_y = (self.rows.total() as f32 - 1.0).max(fh);
        let fix = |v: f32, lo: f32, hi: f32| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        (fix(x, fw, max_x), fix(y, fh, max_y))
    }

    /// The sheet rectangles the panes show, in pane order (empty panes left out).
    #[must_use]
    pub fn visible_regions(&self, view: &View, header_w: f32) -> Vec<(Pane, Bounds)> {
        let z = view.zoom.max(0.01);
        [
            Pane::TopLeft,
            Pane::TopRight,
            Pane::BottomLeft,
            Pane::BottomRight,
        ]
        .into_iter()
        .filter_map(|p| {
            let (sx, sy, rect) = self.pane(view, p, header_w);
            (rect.w > 0.0 && rect.h > 0.0).then(|| (p, Bounds::new(sx, sy, rect.w / z, rect.h / z)))
        })
        .collect()
    }

    /// The last row the view shows (for the header width).
    #[must_use]
    pub fn last_visible_row(&self, view: &View) -> u32 {
        let z = view.zoom.max(0.01);
        let (_, sy) = self.clamp_scroll(view.scroll_x, view.scroll_y);
        let h = (view.height / z - self.header_h() - self.frozen_h()).max(0.0);
        self.rows.index_at(f64::from(sy + h))
    }

    /// Maps a surface point to the pane under it and its sheet position.
    #[must_use]
    pub fn to_sheet(&self, view: &View, x: f32, y: f32, header_w: f32) -> Option<(Pane, f32, f32)> {
        let z = view.zoom.max(0.01);
        for p in [
            Pane::TopLeft,
            Pane::TopRight,
            Pane::BottomLeft,
            Pane::BottomRight,
        ] {
            let (sx, sy, rect) = self.pane(view, p, header_w);
            if rect.w > 0.0 && rect.h > 0.0 && rect.contains(x, y) {
                return Some((p, sx + (x - rect.x) / z, sy + (y - rect.y) / z));
            }
        }
        None
    }

    /// Maps a sheet position to the surface (through the pane that shows it, the scrolling one
    /// when it is in none).
    #[must_use]
    pub fn to_surface(&self, view: &View, x: f32, y: f32, header_w: f32) -> (f32, f32) {
        let z = view.zoom;
        let pane = match (y < self.frozen_h(), x < self.frozen_w()) {
            (true, true) => Pane::TopLeft,
            (true, false) => Pane::TopRight,
            (false, true) => Pane::BottomLeft,
            (false, false) => Pane::BottomRight,
        };
        let (sx, sy, rect) = self.pane(view, pane, header_w);
        (rect.x + (x - sx) * z, rect.y + (y - sy) * z)
    }

    /// What is under a surface point. `selection` is the selection's outline (the fill handle
    /// sits at its bottom-right corner).
    #[must_use]
    pub fn hit_test(
        &self,
        view: &View,
        x: f32,
        y: f32,
        header_w: f32,
        selection: Option<&Range>,
    ) -> Option<HitTarget> {
        let z = view.zoom.max(0.01);
        let hx = header_w * z;
        let hy = self.header_h() * z;
        if x < 0.0 || y < 0.0 || x > view.width || y > view.height {
            return None;
        }
        if x < hx && y < hy {
            return Some(HitTarget::Corner);
        }
        if y < hy {
            // Column header: the sheet x through the top panes.
            let (_, sx, _) = self.to_sheet(view, x, hy + 0.5, header_w)?;
            let col = self.cols.index_at(f64::from(sx));
            let (start, size) = self.cols.span(col);
            let end = (start + size) as f32;
            if (end - sx) * z <= RESIZE_SLOP {
                return Some(HitTarget::ColumnBorder { col });
            }
            // The border before `col` resizes the visible column before it.
            if (sx - start as f32) * z <= RESIZE_SLOP
                && col > 0
                && let Some(prev) = self.cols.next_visible(col - 1, false)
            {
                return Some(HitTarget::ColumnBorder { col: prev });
            }
            return Some(HitTarget::ColumnHeader { col });
        }
        if x < hx {
            let (_, _, sy) = self.to_sheet(view, hx + 0.5, y, header_w)?;
            let row = self.rows.index_at(f64::from(sy));
            let (start, size) = self.rows.span(row);
            let end = (start + size) as f32;
            if (end - sy) * z <= RESIZE_SLOP {
                return Some(HitTarget::RowBorder { row });
            }
            if (sy - start as f32) * z <= RESIZE_SLOP
                && row > 0
                && let Some(prev) = self.rows.next_visible(row - 1, false)
            {
                return Some(HitTarget::RowBorder { row: prev });
            }
            return Some(HitTarget::RowHeader { row });
        }
        let (_, sx, sy) = self.to_sheet(view, x, y, header_w)?;
        if let Some(sel) = selection {
            let r = self.range_rect(sel);
            let (hx, hy) = (r.right(), r.bottom());
            let reach = (FILL_HANDLE / 2.0 + 2.0) / z;
            if (sx - hx).abs() <= reach && (sy - hy).abs() <= reach {
                return Some(HitTarget::FillHandle);
            }
        }
        let row = self.rows.index_at(f64::from(sy));
        let col = self.cols.index_at(f64::from(sx));
        let (row, col) = match self.merge_at(row, col) {
            Some(m) => (m.r0, m.c0),
            None => (row, col),
        };
        Some(HitTarget::Cell { row, col })
    }

    /// The scroll position that brings a cell into the scrolling pane (unchanged when it is
    /// already visible or frozen). Like Excel, the pane scrolls by whole rows and columns: the
    /// result is always the start of a row and of a column.
    #[must_use]
    pub fn scroll_to_cell(&self, view: &View, row: u32, col: u32, header_w: f32) -> (f32, f32) {
        let z = view.zoom.max(0.01);
        let (sx, sy) = self.clamp_scroll(view.scroll_x, view.scroll_y);
        let (_, _, pane) = self.pane(view, Pane::BottomRight, header_w);
        let rect = self.cell_rect(row, col);
        let x = if col < self.freeze_cols {
            sx
        } else {
            fit(&self.cols, sx, pane.w / z, rect.x, rect.right())
        };
        let y = if row < self.freeze_rows {
            sy
        } else {
            fit(&self.rows, sy, pane.h / z, rect.y, rect.bottom())
        };
        self.clamp_scroll(x, y)
    }

    /// How many rows fit in the scrolling pane (Page Up/Down moves by this many).
    #[must_use]
    pub fn rows_per_page(&self, view: &View, header_w: f32) -> u32 {
        let z = view.zoom.max(0.01);
        let (_, sy, pane) = self.pane(view, Pane::BottomRight, header_w);
        let top = self.rows.index_at(f64::from(sy));
        let bottom = self.rows.index_at(f64::from(sy + pane.h / z));
        bottom.saturating_sub(top).max(1)
    }
}

/// The scroll offset along one axis that shows `[a, b)` in a pane `len` long scrolled to
/// `at`, moving as little as possible and only to the start of an index.
fn fit(axis: &AxisIndex, at: f32, len: f32, a: f32, b: f32) -> f32 {
    if a >= at && b <= at + len {
        return at;
    }
    if a < at || b - a > len {
        return a;
    }
    // Scroll forward: the first index start at or after `b - len`.
    let want = f64::from(b - len);
    let i = axis.index_at(want);
    let start = axis.offset(i);
    if start + 0.01 >= want {
        start as f32
    } else {
        axis.offset(axis.step(i, 1)) as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo() -> SheetGeometry {
        SheetGeometry {
            rows: AxisIndex::new(1_048_576, 20.0),
            cols: AxisIndex::new(16_384, 64.0),
            freeze_rows: 0,
            freeze_cols: 0,
            headings: true,
            merges: vec![Range::new(2, 1, 3, 2)],
        }
    }

    fn view() -> View {
        View {
            width: 800.0,
            height: 600.0,
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 1.0,
        }
    }

    #[test]
    fn hits_cells_headers_borders_and_the_corner() {
        let g = geo();
        let v = view();
        let hw = g.header_w(30);
        assert_eq!(hw, HEADER_W_MIN);
        assert_eq!(
            g.hit_test(&v, 10.0, 10.0, hw, None),
            Some(HitTarget::Corner)
        );
        assert_eq!(
            g.hit_test(&v, hw + 70.0, 10.0, hw, None),
            Some(HitTarget::ColumnHeader { col: 1 })
        );
        assert_eq!(
            g.hit_test(&v, hw + 127.0, 10.0, hw, None),
            Some(HitTarget::ColumnBorder { col: 1 })
        );
        assert_eq!(
            g.hit_test(&v, hw + 129.0, 10.0, hw, None),
            Some(HitTarget::ColumnBorder { col: 1 })
        );
        assert_eq!(
            g.hit_test(&v, 10.0, HEADER_H + 30.0, hw, None),
            Some(HitTarget::RowHeader { row: 1 })
        );
        assert_eq!(
            g.hit_test(&v, hw + 10.0, HEADER_H + 10.0, hw, None),
            Some(HitTarget::Cell { row: 0, col: 0 })
        );
        // Inside the merge B3:C4 → its top-left cell.
        assert_eq!(
            g.hit_test(
                &v,
                hw + 64.0 * 2.0 + 5.0,
                HEADER_H + 20.0 * 3.0 + 5.0,
                hw,
                None
            ),
            Some(HitTarget::Cell { row: 2, col: 1 })
        );
        let sel = Range::new(0, 0, 1, 1);
        assert_eq!(
            g.hit_test(&v, hw + 128.0, HEADER_H + 40.0, hw, Some(&sel)),
            Some(HitTarget::FillHandle)
        );
    }

    #[test]
    fn frozen_panes_map_points_through_the_right_pane() {
        let mut g = geo();
        g.freeze_rows = 2;
        g.freeze_cols = 1;
        let v = View {
            scroll_x: 64.0 * 10.0,
            scroll_y: 20.0 * 100.0,
            ..view()
        };
        let hw = g.header_w(200);
        // Inside the frozen corner: A1.
        assert_eq!(
            g.hit_test(&v, hw + 5.0, HEADER_H + 5.0, hw, None),
            Some(HitTarget::Cell { row: 0, col: 0 })
        );
        // Right of the frozen column, in the frozen rows: column K (10), row 1.
        assert_eq!(
            g.hit_test(&v, hw + 64.0 + 5.0, HEADER_H + 25.0, hw, None),
            Some(HitTarget::Cell { row: 1, col: 10 })
        );
        // The scrolling pane.
        assert_eq!(
            g.hit_test(&v, hw + 64.0 + 5.0, HEADER_H + 40.0 + 5.0, hw, None),
            Some(HitTarget::Cell { row: 100, col: 10 })
        );
        let regions = g.visible_regions(&v, hw);
        assert_eq!(regions.len(), 4);
        assert_eq!(regions[0].1, Bounds::new(0.0, 0.0, 64.0, 40.0));
        assert_eq!(regions[3].1.x, 640.0);
        // Scrolling never shows the frozen area twice.
        assert_eq!(g.clamp_scroll(0.0, 0.0), (64.0, 40.0));
        let (sx, sy) = g.to_surface(&v, 640.0, 2000.0, hw);
        assert_eq!((sx, sy), (hw + 64.0, HEADER_H + 40.0));
    }

    #[test]
    fn scrolls_cells_into_view_by_whole_rows() {
        let g = geo();
        let v = view();
        let hw = g.header_w(100);
        assert_eq!(g.scroll_to_cell(&v, 5, 2, hw), (0.0, 0.0));
        let (_, sy) = g.scroll_to_cell(&v, 100, 0, hw);
        assert_eq!(sy % 20.0, 0.0);
        let (_, _, pane) = g.pane(&v, Pane::BottomRight, hw);
        assert!(sy + pane.h >= 101.0 * 20.0 && sy <= 100.0 * 20.0);
        let (sx, _) = g.scroll_to_cell(&v, 0, 50, hw);
        assert!(sx > 0.0 && sx % 64.0 == 0.0);
        assert!(g.rows_per_page(&v, hw) >= 25);
        assert_eq!(g.header_w(99_998), HEADER_W_MIN + 8.0);
    }

    #[test]
    fn snapping_to_device_pixels() {
        assert_eq!(Snap(2.0).at(10.3), 10.5);
        assert_eq!(Snap(1.0).at(10.3), 10.0);
        assert_eq!(Snap(2.0).px(1.0), 1.0);
        assert!((Snap(1.5).px(1.0) - 2.0 / 1.5).abs() < 1e-6);
    }
}
