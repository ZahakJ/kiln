// SPDX-License-Identifier: AGPL-3.0-or-later
//! Row and column properties (sizes, hidden, outline levels, formats) and the pixel geometry
//! derived from them.
//!
//! Units follow SpreadsheetML so files round-trip exactly: row heights in points (Excel's default
//! 15 pt is 20 px at 96 dpi); column widths in characters of the default font *including* the
//! 5 px cell padding (`<col width>`), so the default 8.43-character column is stored as 9.140625
//! and is 64 px wide with Calibri 11 (maximum digit width 7 px; ECMA-376 Part 1 §18.3.1.13).

use std::collections::BTreeMap;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use crate::address::{MAX_COLS, MAX_ROWS};
use crate::num::F64;

/// Properties of one row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RowProps {
    /// Height in points (custom or auto-fitted); `None` = the sheet default.
    #[serde(default)]
    pub height: Option<F64>,
    /// The height was set by the user (not auto-fitted).
    #[serde(default)]
    pub custom_height: bool,
    /// Hidden.
    #[serde(default)]
    pub hidden: bool,
    /// Hidden by an AutoFilter rather than manually (SUBTOTAL 1–11 still counts manually hidden
    /// rows).
    #[serde(default)]
    pub filtered: bool,
    /// Outline level 0–7.
    #[serde(default)]
    pub outline_level: u8,
    /// The outline group below is collapsed.
    #[serde(default)]
    pub collapsed: bool,
    /// Row format applied to empty cells of the row.
    #[serde(default)]
    pub style: Option<u32>,
    /// Thick top border (layout hint).
    #[serde(default)]
    pub thick_top: bool,
    /// Thick bottom border (layout hint).
    #[serde(default)]
    pub thick_bot: bool,
}

/// Properties of one column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ColProps {
    /// Width in SpreadsheetML character units (padding included); `None` = the sheet default.
    #[serde(default)]
    pub width: Option<F64>,
    /// The width was set by the user.
    #[serde(default)]
    pub custom_width: bool,
    /// Auto-fitted.
    #[serde(default)]
    pub best_fit: bool,
    /// Hidden.
    #[serde(default)]
    pub hidden: bool,
    /// Outline level 0–7.
    #[serde(default)]
    pub outline_level: u8,
    /// The outline group is collapsed.
    #[serde(default)]
    pub collapsed: bool,
    /// Column format applied to empty cells of the column.
    #[serde(default)]
    pub style: Option<u32>,
}

/// What the axis needs to know about row or column properties.
pub trait AxisProps: Clone + Default + PartialEq + std::fmt::Debug {
    /// Hidden.
    fn hidden(&self) -> bool;
    /// Outline level.
    fn outline_level(&self) -> u8;
    /// The size in the axis's stored unit, if not the default.
    fn size(&self) -> Option<f64>;
    /// The format applied to empty cells.
    fn style(&self) -> Option<u32>;
}

impl AxisProps for RowProps {
    fn hidden(&self) -> bool {
        self.hidden
    }
    fn outline_level(&self) -> u8 {
        self.outline_level
    }
    fn size(&self) -> Option<f64> {
        self.height.map(|h| h.0)
    }
    fn style(&self) -> Option<u32> {
        self.style
    }
}

impl AxisProps for ColProps {
    fn hidden(&self) -> bool {
        self.hidden
    }
    fn outline_level(&self) -> u8 {
        self.outline_level
    }
    fn size(&self) -> Option<f64> {
        self.width.map(|w| w.0)
    }
    fn style(&self) -> Option<u32> {
        self.style
    }
}

// ── unit conversions ──────────────────────────────────────────────────────

/// Excel's default row height in points.
pub const DEFAULT_ROW_HEIGHT_PT: f64 = 15.0;
/// Excel's base column width in characters (`baseColWidth`).
pub const BASE_COL_WIDTH: f64 = 8.0;

/// Points → pixels at 96 dpi.
pub fn pt_to_px(pt: f64) -> f64 {
    pt * 96.0 / 72.0
}

/// Pixels → points at 96 dpi.
pub fn px_to_pt(px: f64) -> f64 {
    px * 72.0 / 96.0
}

/// Characters (as typed in Column Width…) → stored width, for a maximum digit width in px.
pub fn chars_to_col_width(chars: f64, mdw: f64) -> f64 {
    ((chars * mdw + 5.0) / mdw * 256.0).trunc() / 256.0
}

/// Stored width → characters shown in the Column Width dialog.
pub fn col_width_to_chars(width: f64, mdw: f64) -> f64 {
    (((width * mdw - 5.0) / mdw) * 100.0 + 0.5).trunc() / 100.0
}

/// Stored width → pixels.
pub fn col_width_to_px(width: f64, mdw: f64) -> f64 {
    ((256.0 * width + (128.0 / mdw).trunc()) / 256.0 * mdw).trunc()
}

/// Pixels → stored width.
pub fn px_to_col_width(px: f64, mdw: f64) -> f64 {
    (px / mdw * 256.0).trunc() / 256.0
}

/// The default column width (stored units) for a base width in characters: base × digit width
/// plus 5 px padding, rounded up to a multiple of 8 px (ECMA-376 §18.3.1.81) — 64 px / 9.140625
/// for Calibri 11.
pub fn default_col_width_for(base_chars: f64, mdw: f64) -> f64 {
    px_to_col_width(((base_chars * mdw + 5.0) / 8.0).ceil() * 8.0, mdw)
}

/// Row height in points → whole pixels.
pub fn row_height_to_px(pt: f64) -> f64 {
    pt_to_px(pt).round()
}

/// Supplies the maximum digit width (px at 96 dpi) of a font; the grid can inject real
/// measurements.
pub trait FontMetrics {
    /// Maximum digit width of `name` at `size_pt`.
    fn max_digit_width(&self, name: &str, size_pt: f64) -> f64;
}

/// Built-in metrics: the widest digit's advance as a fraction of the em for common spreadsheet
/// fonts, rounded like Excel's GDI measurement.
#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultFontMetrics;

impl FontMetrics for DefaultFontMetrics {
    fn max_digit_width(&self, name: &str, size_pt: f64) -> f64 {
        let ratio = match name.to_lowercase().as_str() {
            "calibri" | "carlito" | "calibri light" => 0.5068,
            "aptos" => 0.553,
            "aptos narrow" => 0.48,
            "arial" | "liberation sans" | "helvetica" => 0.556,
            "arial narrow" => 0.456,
            "times new roman" | "liberation serif" | "tinos" => 0.5,
            "cambria" | "caladea" | "segoe ui" => 0.5537,
            "georgia" => 0.6157,
            "verdana" => 0.6362,
            "tahoma" => 0.5459,
            "consolas" => 0.5498,
            "courier new" | "liberation mono" => 0.6001,
            "inter" => 0.62,
            "jetbrains mono" => 0.6,
            _ => 0.55,
        };
        (pt_to_px(size_pt) * ratio).round().max(1.0)
    }
}

// ── axis ──────────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct Geometry {
    idx: Vec<u32>,
    prefix: Vec<f64>,
}

/// Sparse per-index properties along one axis plus O(log n) pixel geometry for the virtualised
/// renderer. Only indices with non-default sizes enter the prefix sums, so a million default rows
/// cost nothing.
#[derive(Debug)]
pub struct Axis<P: AxisProps> {
    props: BTreeMap<u32, P>,
    length: u32,
    default_px: f64,
    /// Stored unit → px.
    unit: SizeUnit,
    geo: OnceLock<Geometry>,
}

impl<P: AxisProps> Clone for Axis<P> {
    fn clone(&self) -> Self {
        Axis {
            props: self.props.clone(),
            length: self.length,
            default_px: self.default_px,
            unit: self.unit,
            geo: OnceLock::new(),
        }
    }
}

/// How an axis converts its stored sizes to pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SizeUnit {
    /// Row heights in points.
    Points,
    /// Column widths in characters of a font with this maximum digit width (px).
    Chars(f64),
}

impl SizeUnit {
    fn to_px(self, v: f64) -> f64 {
        match self {
            SizeUnit::Points => row_height_to_px(v),
            SizeUnit::Chars(mdw) => col_width_to_px(v, mdw),
        }
    }
}

impl<P: AxisProps> Axis<P> {
    /// An axis of `length` indices of `default_px` pixels each.
    pub fn new(length: u32, default_px: f64, unit: SizeUnit) -> Axis<P> {
        Axis {
            props: BTreeMap::new(),
            length,
            default_px,
            unit,
            geo: OnceLock::new(),
        }
    }

    /// Number of indices.
    pub fn length(&self) -> u32 {
        self.length
    }

    /// Size of an index without properties, in px.
    pub fn default_px(&self) -> f64 {
        self.default_px
    }

    /// Changes the default size and unit (after the default font or default size changed).
    pub fn set_default(&mut self, default_px: f64, unit: SizeUnit) {
        self.default_px = default_px;
        self.unit = unit;
        self.geo = OnceLock::new();
    }

    /// Properties of an index.
    pub fn get(&self, i: u32) -> Option<&P> {
        self.props.get(&i)
    }

    /// Replaces the properties of an index (`None` or the default clears them).
    pub fn set(&mut self, i: u32, p: Option<P>) {
        if i >= self.length {
            return;
        }
        match p {
            Some(p) if p != P::default() => {
                self.props.insert(i, p);
            }
            _ => {
                self.props.remove(&i);
            }
        }
        self.geo = OnceLock::new();
    }

    /// Changes the properties of an index in place.
    pub fn update(&mut self, i: u32, f: impl FnOnce(&mut P)) {
        let mut p = self.props.get(&i).cloned().unwrap_or_default();
        f(&mut p);
        self.set(i, Some(p));
    }

    /// Indices with properties and their properties, ascending.
    pub fn entries(&self) -> impl Iterator<Item = (u32, &P)> + '_ {
        self.props.iter().map(|(&i, p)| (i, p))
    }

    /// Entries in `lo..=hi`.
    pub fn entries_in(&self, lo: u32, hi: u32) -> impl Iterator<Item = (u32, &P)> + '_ {
        self.props.range(lo..=hi).map(|(&i, p)| (i, p))
    }

    /// Number of indices with properties.
    pub fn count(&self) -> usize {
        self.props.len()
    }

    /// True when the index is hidden.
    pub fn is_hidden(&self, i: u32) -> bool {
        self.props.get(&i).is_some_and(P::hidden)
    }

    /// Outline level of an index.
    pub fn outline_level(&self, i: u32) -> u8 {
        self.props.get(&i).map_or(0, P::outline_level)
    }

    /// Size of an index in px (0 when hidden).
    pub fn size_px(&self, i: u32) -> f64 {
        match self.props.get(&i) {
            None => self.default_px,
            Some(p) if p.hidden() => 0.0,
            Some(p) => p.size().map_or(self.default_px, |s| self.unit.to_px(s)),
        }
    }

    fn geometry(&self) -> &Geometry {
        self.geo.get_or_init(|| {
            let mut g = Geometry {
                idx: Vec::new(),
                prefix: vec![0.0],
            };
            for &i in self.props.keys() {
                let d = self.size_px(i) - self.default_px;
                if d != 0.0 {
                    g.idx.push(i);
                    let last = g.prefix.last().copied().unwrap_or(0.0);
                    g.prefix.push(last + d);
                }
            }
            g
        })
    }

    /// Pixel offset of the start of index `i` (the sizes of `0..i`).
    pub fn offset_px(&self, i: u32) -> f64 {
        let g = self.geometry();
        let k = g.idx.partition_point(|&x| x < i);
        f64::from(i) * self.default_px + g.prefix[k]
    }

    /// Total size of the axis.
    pub fn total_px(&self) -> f64 {
        self.offset_px(self.length)
    }

    /// The index whose span contains pixel offset `px` (clamped; hidden indices are skipped).
    pub fn index_at_px(&self, px: f64) -> u32 {
        if px <= 0.0 {
            return self.first_visible(0);
        }
        let (mut lo, mut hi) = (0u32, self.length - 1);
        while lo < hi {
            let mid = lo + (hi - lo).div_ceil(2);
            if self.offset_px(mid) <= px {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        self.first_visible(lo)
    }

    fn first_visible(&self, i: u32) -> u32 {
        let mut j = i;
        while j < self.length - 1 && self.size_px(j) == 0.0 {
            j += 1;
        }
        j
    }

    /// Shifts properties at or after `at` by `delta` (insert > 0, delete < 0).
    pub fn shift(&mut self, at: u32, delta: i64) {
        if delta == 0 {
            return;
        }
        let tail = self.props.split_off(&at);
        for (i, p) in tail {
            if delta < 0 && i64::from(i) < i64::from(at) - delta {
                continue;
            }
            let n = i64::from(i) + delta;
            if (0..i64::from(self.length)).contains(&n) {
                self.props.insert(n as u32, p);
            }
        }
        self.geo = OnceLock::new();
    }

    /// The sparse properties as a list (for snapshots and undo).
    pub fn to_vec(&self) -> Vec<(u32, P)> {
        self.props.iter().map(|(&i, p)| (i, p.clone())).collect()
    }

    /// Replaces every property.
    pub fn load(&mut self, entries: Vec<(u32, P)>) {
        self.props = entries
            .into_iter()
            .filter(|(i, p)| *i < self.length && *p != P::default())
            .collect();
        self.geo = OnceLock::new();
    }
}

/// A row axis with the given default height.
pub fn row_axis(default_height_pt: f64) -> Axis<RowProps> {
    Axis::new(
        MAX_ROWS,
        row_height_to_px(default_height_pt),
        SizeUnit::Points,
    )
}

/// A column axis with the given default width (stored units) and maximum digit width.
pub fn col_axis(default_width: f64, mdw: f64) -> Axis<ColProps> {
    Axis::new(
        MAX_COLS,
        col_width_to_px(default_width, mdw),
        SizeUnit::Chars(mdw),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_excel_column_widths() {
        assert_eq!(default_col_width_for(BASE_COL_WIDTH, 7.0), 9.140625);
        assert_eq!(col_width_to_px(9.140625, 7.0), 64.0);
        assert_eq!(col_width_to_chars(9.140625, 7.0), 8.43);
        assert_eq!(chars_to_col_width(8.43, 7.0), 9.140625);
        assert_eq!(col_width_to_px(px_to_col_width(100.0, 7.0), 7.0), 100.0);
        assert_eq!(row_height_to_px(15.0), 20.0);
        assert_eq!(px_to_pt(20.0), 15.0);
    }

    #[test]
    fn derives_the_maximum_digit_width_from_the_font() {
        assert_eq!(DefaultFontMetrics.max_digit_width("Calibri", 11.0), 7.0);
        assert_eq!(DefaultFontMetrics.max_digit_width("Arial", 10.0), 7.0);
        assert!(DefaultFontMetrics.max_digit_width("Unknown", 1.0) >= 1.0);
    }

    #[test]
    fn gives_excel_default_sizes() {
        let rows = row_axis(DEFAULT_ROW_HEIGHT_PT);
        let cols = col_axis(default_col_width_for(BASE_COL_WIDTH, 7.0), 7.0);
        assert_eq!(rows.size_px(0), 20.0);
        assert_eq!(cols.size_px(0), 64.0);
        assert_eq!(rows.total_px(), 20.0 * f64::from(MAX_ROWS));
    }

    #[test]
    fn computes_offsets_with_custom_and_hidden_sizes() {
        let mut rows = row_axis(15.0);
        rows.set(
            2,
            Some(RowProps {
                height: Some(F64(30.0)),
                ..RowProps::default()
            }),
        );
        rows.update(4, |p| p.hidden = true);
        assert_eq!(rows.offset_px(2), 40.0);
        assert_eq!(rows.offset_px(3), 80.0);
        assert_eq!(rows.offset_px(5), 100.0);
        assert_eq!(rows.index_at_px(0.0), 0);
        assert_eq!(rows.index_at_px(45.0), 2);
        assert_eq!(rows.index_at_px(90.0), 3);
        assert_eq!(rows.index_at_px(105.0), 5, "hidden rows are skipped");
        assert_eq!(rows.index_at_px(1e12), MAX_ROWS - 1);
    }

    #[test]
    fn shifts_properties_for_inserts_and_deletes() {
        let mut cols = col_axis(9.140625, 7.0);
        cols.update(1, |p| p.hidden = true);
        cols.update(5, |p| p.outline_level = 2);
        cols.shift(1, 2);
        assert!(cols.is_hidden(3));
        assert_eq!(cols.outline_level(7), 2);
        cols.shift(2, -3);
        assert_eq!(cols.count(), 1);
        assert_eq!(cols.outline_level(4), 2);
        cols.update(4, |p| p.outline_level = 0);
        assert_eq!(cols.count(), 0, "default properties are dropped");
    }

    #[test]
    fn refreshes_pixels_when_the_default_changes() {
        let mut cols = col_axis(9.140625, 7.0);
        cols.update(0, |p| p.width = Some(F64(20.0)));
        let before = cols.offset_px(2);
        cols.set_default(80.0, SizeUnit::Chars(8.0));
        assert_ne!(cols.offset_px(2), before);
        assert_eq!(cols.size_px(1), 80.0);
    }
}
