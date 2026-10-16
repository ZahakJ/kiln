// SPDX-License-Identifier: AGPL-3.0-or-later
//! The sparse cell store of one sheet (PLAN.md §3.4).
//!
//! Rows live in a hash map (`row → RowChunk`), so random access is O(1). A [`RowChunk`] keeps its
//! cells in parallel arrays sorted by column: `cols`, `values` and `meta` (style index and
//! [`CellKind`] packed in one `u32`). Small rows are searched by binary search; rows with more than
//! [`INDEX_THRESHOLD`] cells get a hash index built lazily on the first lookup after a change
//! (thread-safe, so parallel recalculation can read the store). The ascending list of rows and the
//! used ranges are cached and rebuilt on demand.
//!
//! A cell exists while it has a value, a formula or a non-default style (index 0 is the
//! workbook's default format); making all three empty removes it. A formula cell's value is its
//! cached result; the formula text itself lives in the formula engine (or, for text the engine
//! cannot parse, in this store as a *raw* formula so it survives a round trip).

use std::sync::{Arc, OnceLock};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::address::{MAX_COLS, MAX_ROWS, Range};
use crate::value::CellValue;

/// Rows with more cells than this get a hash index for O(1) lookup.
pub const INDEX_THRESHOLD: usize = 24;

const KIND_BITS: u32 = 2;
const KIND_MASK: u32 = (1 << KIND_BITS) - 1;
/// Largest style index the store can hold (2³⁰ − 1; Excel's limit is 64,000).
pub const MAX_STYLE: u32 = u32::MAX >> KIND_BITS;

/// What a cell holds besides its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[repr(u8)]
pub enum CellKind {
    /// A literal value (or nothing).
    #[default]
    Value = 0,
    /// A formula with dynamic-array semantics; its text is in the formula engine.
    Formula = 1,
    /// A formula written before dynamic arrays (implicit intersection, no spilling).
    LegacyFormula = 2,
    /// Formula text the engine could not parse (kept for round trips; shows the cached value).
    RawFormula = 3,
}

impl CellKind {
    /// True for any formula.
    pub fn is_formula(self) -> bool {
        self != CellKind::Value
    }

    /// True for a formula the engine calculates.
    pub fn is_engine_formula(self) -> bool {
        matches!(self, CellKind::Formula | CellKind::LegacyFormula)
    }

    fn from_bits(b: u32) -> CellKind {
        match b & KIND_MASK {
            1 => CellKind::Formula,
            2 => CellKind::LegacyFormula,
            3 => CellKind::RawFormula,
            _ => CellKind::Value,
        }
    }
}

fn pack(style: u32, kind: CellKind) -> u32 {
    (style.min(MAX_STYLE) << KIND_BITS) | kind as u32
}

/// A view of one stored cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellData<'a> {
    /// The value (cached result for formulas).
    pub value: &'a CellValue,
    /// Value or formula.
    pub kind: CellKind,
    /// `cellXfs` style index.
    pub style: u32,
}

impl CellData<'_> {
    /// True when the cell has a value or a formula (not only a format).
    pub fn has_content(&self) -> bool {
        !self.value.is_blank() || self.kind.is_formula()
    }
}

static BLANK: CellValue = CellValue::Blank;

/// One stored row: parallel arrays sorted by column.
#[derive(Debug, Clone, Default)]
pub struct RowChunk {
    cols: Vec<u16>,
    values: Vec<CellValue>,
    meta: Vec<u32>,
    index: Option<Box<OnceLock<FxHashMap<u16, u32>>>>,
}

impl RowChunk {
    /// Number of cells.
    pub fn len(&self) -> usize {
        self.cols.len()
    }

    /// True when the row has no cells.
    pub fn is_empty(&self) -> bool {
        self.cols.is_empty()
    }

    /// Columns, ascending.
    pub fn cols(&self) -> &[u16] {
        &self.cols
    }

    /// Slot of a column.
    pub fn find(&self, col: u32) -> Option<usize> {
        let col = u16::try_from(col).ok()?;
        let n = self.cols.len();
        if n == 0 {
            return None;
        }
        if self.cols[n - 1] == col {
            return Some(n - 1);
        }
        if let Some(index) = &self.index {
            let map = index.get_or_init(|| {
                self.cols
                    .iter()
                    .enumerate()
                    .map(|(i, &c)| (c, i as u32))
                    .collect()
            });
            return map.get(&col).map(|&i| i as usize);
        }
        self.cols.binary_search(&col).ok()
    }

    /// First slot whose column is `>= col`.
    pub fn lower_bound(&self, col: u32) -> usize {
        match u16::try_from(col) {
            Ok(c) => self.cols.partition_point(|&x| x < c),
            Err(_) => self.cols.len(),
        }
    }

    /// The cell in a slot.
    pub fn at(&self, i: usize) -> (u32, CellData<'_>) {
        let m = self.meta[i];
        (
            u32::from(self.cols[i]),
            CellData {
                value: &self.values[i],
                kind: CellKind::from_bits(m),
                style: m >> KIND_BITS,
            },
        )
    }

    /// Cells in column order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, CellData<'_>)> + '_ {
        (0..self.cols.len()).map(move |i| self.at(i))
    }

    fn reset_index(&mut self) {
        self.index = (self.cols.len() > INDEX_THRESHOLD).then(Default::default);
    }

    /// Inserts or overwrites; true when a new slot was created.
    fn put(&mut self, col: u16, value: CellValue, meta: u32) -> bool {
        let n = self.cols.len();
        if n == 0 || self.cols[n - 1] < col {
            self.cols.push(col);
            self.values.push(value);
            self.meta.push(meta);
            match self.index.as_mut().and_then(|ix| ix.get_mut()) {
                Some(map) => {
                    map.insert(col, n as u32);
                }
                None => {
                    if n + 1 > INDEX_THRESHOLD && self.index.is_none() {
                        self.index = Some(Default::default());
                    }
                }
            }
            return true;
        }
        match self.cols.binary_search(&col) {
            Ok(i) => {
                self.values[i] = value;
                self.meta[i] = meta;
                false
            }
            Err(i) => {
                self.cols.insert(i, col);
                self.values.insert(i, value);
                self.meta.insert(i, meta);
                self.reset_index();
                true
            }
        }
    }

    fn remove_at(&mut self, i: usize) -> (CellValue, u32) {
        self.cols.remove(i);
        let v = self.values.remove(i);
        let m = self.meta.remove(i);
        self.reset_index();
        (v, m)
    }

    /// Shifts columns at or after `at` by `delta`, dropping deleted cells and cells pushed past
    /// the last column. Returns the number of cells dropped.
    fn shift_cols(&mut self, at: u32, delta: i64) -> usize {
        let before = self.cols.len();
        let mut w = 0;
        for i in 0..before {
            let c = i64::from(self.cols[i]);
            let n = if c >= i64::from(at) {
                if delta < 0 && c < i64::from(at) - delta {
                    continue;
                }
                c + delta
            } else {
                c
            };
            if n < 0 || n >= i64::from(MAX_COLS) {
                continue;
            }
            self.cols[w] = n as u16;
            self.values.swap(w, i);
            self.meta[w] = self.meta[i];
            w += 1;
        }
        self.cols.truncate(w);
        self.values.truncate(w);
        self.meta.truncate(w);
        self.reset_index();
        before - w
    }
}

/// Iterates candidate rows of a range: every row index for short ranges, the stored rows for
/// tall ones.
enum RowIter<'a> {
    Dense(std::ops::RangeInclusive<u32>),
    Sparse(std::slice::Iter<'a, u32>),
}

impl Iterator for RowIter<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<u32> {
        match self {
            RowIter::Dense(r) => r.next(),
            RowIter::Sparse(s) => s.next().copied(),
        }
    }
}

/// The sparse cell store of one sheet.
#[derive(Debug, Default)]
pub struct SparseStore {
    rows: FxHashMap<u32, RowChunk>,
    sorted: OnceLock<Vec<u32>>,
    count: usize,
    used: OnceLock<Option<Range>>,
    used_content: OnceLock<Option<Range>>,
    /// A conservative (never too small) exclusive extent of cells with content.
    extent: (u32, u32),
    raw: FxHashMap<(u32, u32), Arc<str>>,
}

impl Clone for SparseStore {
    fn clone(&self) -> SparseStore {
        SparseStore {
            rows: self.rows.clone(),
            sorted: OnceLock::new(),
            count: self.count,
            used: OnceLock::new(),
            used_content: OnceLock::new(),
            extent: self.extent,
            raw: self.raw.clone(),
        }
    }
}

impl SparseStore {
    /// An empty store.
    pub fn new() -> SparseStore {
        SparseStore::default()
    }

    /// Number of stored cells.
    pub fn len(&self) -> usize {
        self.count
    }

    /// True when no cell is stored.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Number of rows with at least one cell.
    pub fn row_count(&self) -> usize {
        self.rows.len()
    }

    fn touch(&mut self) {
        self.used = OnceLock::new();
        self.used_content = OnceLock::new();
    }

    fn rows_changed(&mut self) {
        self.sorted = OnceLock::new();
    }

    /// The chunk of a row.
    pub fn chunk(&self, row: u32) -> Option<&RowChunk> {
        self.rows.get(&row)
    }

    /// The cell at (row, col).
    pub fn get(&self, row: u32, col: u32) -> Option<CellData<'_>> {
        let ch = self.rows.get(&row)?;
        let i = ch.find(col)?;
        Some(ch.at(i).1)
    }

    /// True when a cell is stored at (row, col).
    pub fn contains(&self, row: u32, col: u32) -> bool {
        self.get(row, col).is_some()
    }

    /// The value at (row, col) (blank when there is no cell).
    pub fn value(&self, row: u32, col: u32) -> &CellValue {
        self.get(row, col).map_or(&BLANK, |c| c.value)
    }

    /// The kind of the cell at (row, col).
    pub fn kind(&self, row: u32, col: u32) -> CellKind {
        self.get(row, col).map_or(CellKind::Value, |c| c.kind)
    }

    /// The style index of the cell at (row, col) (0 when there is no cell).
    pub fn style(&self, row: u32, col: u32) -> u32 {
        self.get(row, col).map_or(0, |c| c.style)
    }

    /// The text of a raw (unparsed) formula.
    pub fn raw_formula(&self, row: u32, col: u32) -> Option<&Arc<str>> {
        self.raw.get(&(row, col))
    }

    /// Replaces a cell completely; all-empty removes it. For [`CellKind::RawFormula`] set the
    /// text with [`SparseStore::set_raw_formula`].
    pub fn set(&mut self, row: u32, col: u32, value: CellValue, kind: CellKind, style: u32) {
        if row >= MAX_ROWS || col >= MAX_COLS {
            return;
        }
        if kind != CellKind::RawFormula {
            self.raw.remove(&(row, col));
        }
        if value.is_blank() && kind == CellKind::Value && style == 0 {
            self.remove(row, col);
            return;
        }
        if !value.is_blank() || kind.is_formula() {
            self.extent.0 = self.extent.0.max(row + 1);
            self.extent.1 = self.extent.1.max(col + 1);
        }
        let ch = match self.rows.get_mut(&row) {
            Some(ch) => ch,
            None => {
                self.sorted = OnceLock::new();
                self.rows.entry(row).or_default()
            }
        };
        if ch.put(col as u16, value, pack(style, kind)) {
            self.count += 1;
        }
        self.touch();
    }

    /// Makes (row, col) a raw formula with `text` (without `=`), keeping its value and style.
    pub fn set_raw_formula(&mut self, row: u32, col: u32, text: &str, cached: CellValue) {
        let style = self.style(row, col);
        self.set(row, col, cached, CellKind::RawFormula, style);
        if self.contains(row, col) {
            self.raw.insert((row, col), Arc::from(text));
        }
    }

    /// Sets a literal value, removing any formula and keeping the style.
    pub fn set_value(&mut self, row: u32, col: u32, value: CellValue) {
        let style = self.style(row, col);
        self.set(row, col, value, CellKind::Value, style);
    }

    /// Sets the style index, keeping value and formula.
    pub fn set_style(&mut self, row: u32, col: u32, style: u32) {
        match self.get(row, col) {
            Some(c) => {
                let (v, k) = (c.value.clone(), c.kind);
                self.set(row, col, v, k, style);
            }
            None => self.set(row, col, CellValue::Blank, CellKind::Value, style),
        }
    }

    /// Writes the cached result of a formula cell (no-op for other cells).
    pub fn set_cached_value(&mut self, row: u32, col: u32, value: CellValue) {
        if let Some(ch) = self.rows.get_mut(&row)
            && let Some(i) = ch.find(col)
            && CellKind::from_bits(ch.meta[i]).is_formula()
        {
            if !value.is_blank() {
                self.extent.0 = self.extent.0.max(row + 1);
                self.extent.1 = self.extent.1.max(col + 1);
            }
            ch.values[i] = value;
        }
    }

    /// Removes a cell; returns what it held.
    pub fn remove(&mut self, row: u32, col: u32) -> Option<(CellValue, CellKind, u32)> {
        let ch = self.rows.get_mut(&row)?;
        let i = ch.find(col)?;
        let (v, m) = ch.remove_at(i);
        let empty = ch.is_empty();
        if empty {
            self.rows.remove(&row);
            self.rows_changed();
        }
        self.count -= 1;
        self.raw.remove(&(row, col));
        self.touch();
        Some((v, CellKind::from_bits(m), m >> KIND_BITS))
    }

    /// Removes every cell.
    pub fn clear(&mut self) {
        *self = SparseStore::default();
    }

    /// Stored rows, ascending.
    pub fn rows(&self) -> &[u32] {
        self.sorted.get_or_init(|| {
            let mut v: Vec<u32> = self.rows.keys().copied().collect();
            v.sort_unstable();
            v
        })
    }

    /// Every cell in row-major order.
    pub fn iter(&self) -> impl Iterator<Item = (u32, u32, CellData<'_>)> + '_ {
        self.rows().iter().flat_map(move |&r| {
            self.rows
                .get(&r)
                .into_iter()
                .flat_map(move |ch| ch.iter().map(move |(c, d)| (r, c, d)))
        })
    }

    /// The cells inside a range, row-major.
    pub fn iter_range(&self, r: Range) -> impl Iterator<Item = (u32, u32, CellData<'_>)> + '_ {
        let stored = self.rows();
        let rows = if ((r.r1 - r.r0) as usize) < stored.len() {
            RowIter::Dense(r.r0..=r.r1)
        } else {
            let lo = stored.partition_point(|&x| x < r.r0);
            let hi = stored.partition_point(|&x| x <= r.r1);
            RowIter::Sparse(stored[lo..hi].iter())
        };
        rows.flat_map(move |row| {
            self.rows.get(&row).into_iter().flat_map(move |ch| {
                let start = ch.lower_bound(r.c0);
                (start..ch.len())
                    .map(move |i| ch.at(i))
                    .take_while(move |(c, _)| *c <= r.c1)
                    .map(move |(c, d)| (row, c, d))
            })
        })
    }

    /// Number of stored cells inside a range.
    pub fn count_in_range(&self, r: Range) -> usize {
        self.iter_range(r).count()
    }

    /// The smallest range containing every stored cell (`content_only`: only cells with a value
    /// or formula), or `None` when there is none.
    pub fn used_range(&self, content_only: bool) -> Option<Range> {
        let slot = if content_only {
            &self.used_content
        } else {
            &self.used
        };
        *slot.get_or_init(|| {
            let mut out: Option<Range> = None;
            for &row in self.rows() {
                let Some(ch) = self.rows.get(&row) else {
                    continue;
                };
                let (a, b) = if content_only {
                    let mut it = ch.iter().filter(|(_, d)| d.has_content()).map(|(c, _)| c);
                    let Some(first) = it.next() else { continue };
                    (first, it.last().unwrap_or(first))
                } else {
                    match (ch.cols.first(), ch.cols.last()) {
                        (Some(&a), Some(&b)) => (u32::from(a), u32::from(b)),
                        _ => continue,
                    }
                };
                let r = Range::new(row, a, row, b);
                out = Some(out.map_or(r, |o| o.union(&r)));
            }
            out
        })
    }

    /// A conservative exclusive extent `(rows, cols)` of cells with content: never smaller than
    /// the exact one (the formula engine uses it to bound whole-column references).
    pub fn extent(&self) -> (u32, u32) {
        self.extent
    }

    fn recompute_extent(&mut self) {
        self.extent = self
            .used_range(true)
            .map_or((0, 0), |r| (r.r1 + 1, r.c1 + 1));
    }

    /// Removes every cell inside a range.
    pub fn clear_range(&mut self, r: Range) {
        let doomed: Vec<(u32, u32)> = self.iter_range(r).map(|(row, col, _)| (row, col)).collect();
        for (row, col) in doomed {
            self.remove(row, col);
        }
    }

    /// Shifts rows at or after `at` by `delta` (insert > 0, delete < 0); deleted rows and rows
    /// pushed past the last row are dropped.
    pub fn shift_rows(&mut self, at: u32, delta: i64) {
        if delta == 0 {
            return;
        }
        let moving: Vec<u32> = self.rows().iter().copied().filter(|&r| r >= at).collect();
        let mut moved = Vec::with_capacity(moving.len());
        for row in moving {
            let Some(ch) = self.rows.remove(&row) else {
                continue;
            };
            let n = i64::from(row) + delta;
            let deleted = delta < 0 && i64::from(row) < i64::from(at) - delta;
            if deleted || n >= i64::from(MAX_ROWS) || n < 0 {
                self.count -= ch.len();
                continue;
            }
            moved.push((n as u32, ch));
        }
        for (row, ch) in moved {
            self.rows.insert(row, ch);
        }
        self.raw = std::mem::take(&mut self.raw)
            .into_iter()
            .filter_map(|((r, c), t)| {
                if r < at {
                    return Some(((r, c), t));
                }
                let n = i64::from(r) + delta;
                let deleted = delta < 0 && i64::from(r) < i64::from(at) - delta;
                (!deleted && (0..i64::from(MAX_ROWS)).contains(&n)).then_some(((n as u32, c), t))
            })
            .collect();
        self.rows_changed();
        self.touch();
        self.recompute_extent();
    }

    /// Shifts columns at or after `at` by `delta` in every row.
    pub fn shift_cols(&mut self, at: u32, delta: i64) {
        if delta == 0 {
            return;
        }
        let mut emptied = Vec::new();
        let mut dropped = 0;
        for (&row, ch) in &mut self.rows {
            dropped += ch.shift_cols(at, delta);
            if ch.is_empty() {
                emptied.push(row);
            }
        }
        for row in emptied {
            self.rows.remove(&row);
        }
        self.count -= dropped;
        self.raw = std::mem::take(&mut self.raw)
            .into_iter()
            .filter_map(|((r, c), t)| {
                if c < at {
                    return Some(((r, c), t));
                }
                let n = i64::from(c) + delta;
                let deleted = delta < 0 && i64::from(c) < i64::from(at) - delta;
                (!deleted && (0..i64::from(MAX_COLS)).contains(&n)).then_some(((r, n as u32), t))
            })
            .collect();
        self.rows_changed();
        self.touch();
        self.recompute_extent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn num(n: f64) -> CellValue {
        CellValue::Number(n)
    }

    #[test]
    fn keeps_cells_sorted_by_column_whatever_the_insertion_order() {
        let mut s = SparseStore::new();
        for c in [5, 1, 9, 3, 7] {
            s.set_value(0, c, num(f64::from(c)));
        }
        let cols: Vec<u32> = s.iter().map(|(_, c, _)| c).collect();
        assert_eq!(cols, vec![1, 3, 5, 7, 9]);
        assert_eq!(s.chunk(0).map(RowChunk::len), Some(5));
    }

    #[test]
    fn finds_cells_through_binary_search_and_the_lazy_index() {
        let mut s = SparseStore::new();
        for c in (0..2000).rev() {
            s.set_value(3, c * 2, num(f64::from(c)));
        }
        for c in 0..2000 {
            assert_eq!(s.value(3, c * 2), &num(f64::from(c)));
            assert!(!s.contains(3, c * 2 + 1));
        }
        // An insertion in the middle invalidates the index; lookups still agree.
        s.set_value(3, 1, num(-1.0));
        assert_eq!(s.value(3, 1), &num(-1.0));
        assert_eq!(s.value(3, 3998), &num(1999.0));
        assert_eq!(s.len(), 2001);
    }

    #[test]
    fn overwrites_in_place_and_removes_empty_cells() {
        let mut s = SparseStore::new();
        s.set(1, 1, num(1.0), CellKind::Value, 4);
        s.set(1, 1, num(2.0), CellKind::Value, 4);
        assert_eq!(s.len(), 1);
        s.set_value(1, 1, CellValue::Blank);
        assert!(s.contains(1, 1), "a style keeps the cell");
        s.set_style(1, 1, 0);
        assert!(!s.contains(1, 1));
        assert_eq!(s.len(), 0);
        assert_eq!(s.row_count(), 0);
    }

    #[test]
    fn stores_values_kinds_and_styles_independently() {
        let mut s = SparseStore::new();
        s.set(0, 0, num(3.0), CellKind::Formula, 7);
        s.set_style(0, 0, 9);
        assert_eq!(s.kind(0, 0), CellKind::Formula);
        assert_eq!(s.value(0, 0), &num(3.0));
        assert_eq!(s.style(0, 0), 9);
        s.set_cached_value(0, 0, num(4.0));
        assert_eq!(s.value(0, 0), &num(4.0));
        s.set_value(0, 1, num(1.0));
        s.set_cached_value(0, 1, num(5.0));
        assert_eq!(
            s.value(0, 1),
            &num(1.0),
            "cached values only go to formulas"
        );
        s.set_raw_formula(2, 2, "FOO(", num(1.0));
        assert_eq!(s.kind(2, 2), CellKind::RawFormula);
        assert_eq!(s.raw_formula(2, 2).map(|t| &**t), Some("FOO("));
        s.set_value(2, 2, num(1.0));
        assert_eq!(s.raw_formula(2, 2), None);
    }

    #[test]
    fn reports_used_ranges_with_and_without_style_only_cells() {
        let mut s = SparseStore::new();
        assert_eq!(s.used_range(false), None);
        s.set_value(2, 3, num(1.0));
        s.set_style(10, 0, 5);
        assert_eq!(s.used_range(false), Some(Range::new(2, 0, 10, 3)));
        assert_eq!(s.used_range(true), Some(Range::cell(2, 3)));
        assert_eq!(s.extent(), (3, 4));
        s.remove(2, 3);
        assert_eq!(s.used_range(true), None);
    }

    #[test]
    fn visits_only_cells_inside_a_range_on_dense_and_sparse_paths() {
        let mut s = SparseStore::new();
        for r in 0..100 {
            for c in 0..10 {
                s.set_value(r * 10, c, num(f64::from(r * 10 + c)));
            }
        }
        let small = Range::new(10, 2, 30, 4);
        let got: Vec<(u32, u32)> = s.iter_range(small).map(|(r, c, _)| (r, c)).collect();
        assert_eq!(got.len(), 9);
        assert!(got.iter().all(|&(r, c)| small.contains(r, c)));
        let tall = Range::cols(9, 9);
        assert_eq!(s.count_in_range(tall), 100);
        assert_eq!(s.count_in_range(Range::sheet()), 1000);
    }

    #[test]
    fn clears_a_range() {
        let mut s = SparseStore::new();
        for r in 0..5 {
            for c in 0..5 {
                s.set_value(r, c, num(1.0));
            }
        }
        s.clear_range(Range::new(1, 1, 3, 3));
        assert_eq!(s.len(), 16);
        assert!(!s.contains(2, 2));
    }

    #[test]
    fn shifts_rows_for_inserts_and_deletes_and_drops_overflow() {
        let mut s = SparseStore::new();
        for r in [0, 5, 6, 10] {
            s.set_value(r, 0, num(f64::from(r)));
        }
        s.shift_rows(5, 2);
        assert_eq!(s.rows(), &[0, 7, 8, 12]);
        s.shift_rows(7, -2);
        assert_eq!(s.rows(), &[0, 10]);
        assert_eq!(s.len(), 2);
        s.set_value(MAX_ROWS - 1, 0, num(1.0));
        s.shift_rows(0, 1);
        assert_eq!(s.rows(), &[1, 11]);
        assert_eq!(s.len(), 2);
    }

    #[test]
    fn shifts_columns_of_every_row() {
        let mut s = SparseStore::new();
        for c in [0, 3, 4, 8] {
            s.set_value(0, c, num(f64::from(c)));
            s.set_value(1, c, num(f64::from(c)));
        }
        s.set_raw_formula(1, 8, "X(", CellValue::Blank);
        s.shift_cols(3, -2);
        assert_eq!(s.chunk(0).map(|c| c.cols().to_vec()), Some(vec![0, 6]));
        assert_eq!(s.len(), 4);
        assert!(s.raw_formula(1, 6).is_some());
        s.shift_cols(0, i64::from(MAX_COLS) - 1);
        assert_eq!(s.len(), 2, "cells pushed past XFD are dropped");
    }

    #[test]
    fn agrees_with_a_map_model_under_random_operations() {
        let mut s = SparseStore::new();
        let mut m: BTreeMap<(u32, u32), f64> = BTreeMap::new();
        let mut x: u64 = 0x2545_F491_4F6C_DD1D;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for _ in 0..20_000 {
            let r = (next() % 40) as u32;
            let c = (next() % 60) as u32;
            if next() % 4 == 0 {
                s.remove(r, c);
                m.remove(&(r, c));
            } else {
                let v = (next() % 1000) as f64;
                s.set_value(r, c, num(v));
                m.insert((r, c), v);
            }
        }
        assert_eq!(s.len(), m.len());
        let got: Vec<((u32, u32), f64)> = s
            .iter()
            .map(|(r, c, d)| ((r, c), d.value.as_number().unwrap_or(f64::NAN)))
            .collect();
        let want: Vec<((u32, u32), f64)> = m.into_iter().collect();
        assert_eq!(got, want);
    }

    #[test]
    fn clones_deeply() {
        let mut s = SparseStore::new();
        s.set_value(0, 0, num(1.0));
        let c = s.clone();
        s.set_value(0, 0, num(2.0));
        assert_eq!(c.value(0, 0), &num(1.0));
    }

    #[test]
    fn ignores_positions_off_the_sheet() {
        let mut s = SparseStore::new();
        s.set_value(MAX_ROWS, 0, num(1.0));
        s.set_value(0, MAX_COLS, num(1.0));
        assert!(s.is_empty());
        assert_eq!(s.value(MAX_ROWS, 0), &CellValue::Blank);
    }
}
