// SPDX-License-Identifier: AGPL-3.0-or-later
//! Row and column geometry: pixel offsets over up to 1,048,576 rows or 16,384 columns in
//! O(log n), with hidden indices (size 0) and live resizing.
//!
//! Indices are grouped in blocks of [`BLOCK`]; a Fenwick tree holds the size of every block and a
//! sparse map the indices whose size differs from the default. The offset of an index is the
//! Fenwick prefix of the blocks before it plus the default sizes and deltas inside its own block;
//! finding the index at a pixel descends the tree to the block, then walks at most one block.
//! Resizing one index updates one map entry and `log2(blocks)` tree nodes, so a column drag or a
//! row autofit never rebuilds the axis.
//!
//! Sizes are CSS pixels at 100 % zoom (zoom scales the whole surface in the painter).

use std::collections::BTreeMap;

use kiln_sheet::axis::{Axis, AxisProps};

/// Indices per block.
pub const BLOCK: u32 = 64;

/// The geometry of one axis (the rows or the columns of a sheet).
#[derive(Debug, Clone, PartialEq)]
pub struct AxisIndex {
    len: u32,
    default: f64,
    /// `size - default` of every index whose size is not the default (hidden: `-default`).
    deltas: BTreeMap<u32, f64>,
    /// Fenwick tree (1-based) over the block sizes.
    tree: Vec<f64>,
}

impl AxisIndex {
    /// `len` indices of `default` pixels each.
    #[must_use]
    pub fn new(len: u32, default: f64) -> Self {
        let default = if default.is_finite() && default >= 0.0 {
            default
        } else {
            0.0
        };
        let mut a = Self {
            len,
            default,
            deltas: BTreeMap::new(),
            tree: Vec::new(),
        };
        a.rebuild();
        a
    }

    /// The geometry of a `kiln-sheet` axis (its default size, custom sizes and hidden indices).
    #[must_use]
    pub fn from_axis<P: AxisProps>(axis: &Axis<P>) -> Self {
        let mut a = Self::new(axis.length(), axis.default_px());
        for (i, _) in axis.entries() {
            let d = axis.size_px(i) - a.default;
            if d != 0.0 && d.is_finite() {
                a.deltas.insert(i, d);
            }
        }
        a.rebuild();
        a
    }

    fn blocks(&self) -> usize {
        self.len.div_ceil(BLOCK) as usize
    }

    fn block_len(&self, b: usize) -> u32 {
        let start = b as u32 * BLOCK;
        (self.len - start).min(BLOCK)
    }

    fn rebuild(&mut self) {
        let n = self.blocks();
        let mut sums = vec![0.0; n + 1];
        for b in 0..n {
            sums[b + 1] = f64::from(self.block_len(b)) * self.default;
        }
        for (&i, &d) in &self.deltas {
            sums[(i / BLOCK) as usize + 1] += d;
        }
        // Linear-time Fenwick construction.
        for i in 1..=n {
            let parent = i + i.isolate_lowest_one();
            if parent <= n {
                sums[parent] += sums[i];
            }
        }
        self.tree = sums;
    }

    fn add_to_block(&mut self, b: usize, delta: f64) {
        let n = self.blocks();
        let mut i = b + 1;
        while i <= n {
            self.tree[i] += delta;
            i += i.isolate_lowest_one();
        }
    }

    /// Sum of the sizes of blocks `0..b`.
    fn blocks_prefix(&self, b: usize) -> f64 {
        let mut i = b.min(self.blocks());
        let mut s = 0.0;
        while i > 0 {
            s += self.tree[i];
            i &= i - 1;
        }
        s
    }

    /// Number of indices.
    #[must_use]
    pub fn len(&self) -> u32 {
        self.len
    }

    /// Whether the axis has no indices.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The default size.
    #[must_use]
    pub fn default_size(&self) -> f64 {
        self.default
    }

    /// The size of index `i` (0 when hidden or out of range).
    #[must_use]
    pub fn size(&self, i: u32) -> f64 {
        if i >= self.len {
            return 0.0;
        }
        self.default + self.deltas.get(&i).copied().unwrap_or(0.0)
    }

    /// Whether index `i` takes no space (hidden, or a zero size).
    #[must_use]
    pub fn is_hidden(&self, i: u32) -> bool {
        self.size(i) <= 0.0
    }

    /// Sets the size of index `i` (`0` hides it).
    pub fn set_size(&mut self, i: u32, size: f64) {
        if i >= self.len || !size.is_finite() {
            return;
        }
        let size = size.max(0.0);
        let old = self.size(i);
        let delta = size - self.default;
        if delta == 0.0 {
            self.deltas.remove(&i);
        } else {
            self.deltas.insert(i, delta);
        }
        self.add_to_block((i / BLOCK) as usize, size - old);
    }

    /// The pixel offset of the start of index `i` (`offset(len)` is the total size).
    #[must_use]
    pub fn offset(&self, i: u32) -> f64 {
        let i = i.min(self.len);
        let b = i / BLOCK;
        let start = b * BLOCK;
        let inside: f64 = self.deltas.range(start..i).map(|(_, d)| d).sum();
        self.blocks_prefix(b as usize) + f64::from(i - start) * self.default + inside
    }

    /// The total size.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.blocks_prefix(self.blocks())
    }

    /// The span `(start, size)` of index `i`.
    #[must_use]
    pub fn span(&self, i: u32) -> (f64, f64) {
        (self.offset(i), self.size(i))
    }

    /// The span `(start, end)` of indices `a..=b`.
    #[must_use]
    pub fn range_span(&self, a: u32, b: u32) -> (f64, f64) {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        (
            self.offset(lo),
            self.offset(hi.saturating_add(1).min(self.len)),
        )
    }

    /// The index whose span contains `px`: indices of size 0 never do; offsets before the start
    /// give the first visible index and offsets past the end the last one.
    #[must_use]
    pub fn index_at(&self, px: f64) -> u32 {
        if self.len == 0 {
            return 0;
        }
        if px.is_nan() || px <= 0.0 {
            return self.next_visible(0, true).unwrap_or(0);
        }
        if px >= self.total() {
            return self
                .next_visible(self.len - 1, false)
                .unwrap_or(self.len - 1);
        }
        // Fenwick descent: the last block whose start is at or before px.
        let n = self.blocks();
        let mut pos = 0usize;
        let mut acc = 0.0;
        let mut step = n.next_power_of_two();
        while step > 0 {
            let next = pos + step;
            if next <= n && acc + self.tree[next] <= px {
                pos = next;
                acc += self.tree[next];
            }
            step >>= 1;
        }
        // `pos` blocks end at or before px; px lies in block `pos`.
        let b = pos.min(n - 1) as u32;
        let mut i = b * BLOCK;
        let end = (i + BLOCK).min(self.len);
        let mut at = acc;
        while i < end {
            let s = self.size(i);
            if px < at + s {
                return i;
            }
            at += s;
            i += 1;
        }
        // Rounding at a block edge: the next visible index.
        self.next_visible(end.min(self.len - 1), true)
            .unwrap_or(self.len - 1)
    }

    /// The first index at or after (`forward`) or at or before `i` that takes space.
    #[must_use]
    pub fn next_visible(&self, i: u32, forward: bool) -> Option<u32> {
        if self.len == 0 {
            return None;
        }
        let mut j = i.min(self.len - 1);
        loop {
            if !self.is_hidden(j) {
                return Some(j);
            }
            if forward {
                if j + 1 >= self.len {
                    return None;
                }
                j += 1;
            } else {
                if j == 0 {
                    return None;
                }
                j -= 1;
            }
        }
    }

    /// The visible index `n` steps from `i` (skipping hidden ones), clamped to the axis.
    #[must_use]
    pub fn step(&self, i: u32, n: i64) -> u32 {
        let mut j = i.min(self.len.saturating_sub(1));
        let forward = n >= 0;
        let mut left = n.unsigned_abs();
        while left > 0 {
            let next = if forward {
                if j + 1 >= self.len {
                    break;
                }
                self.next_visible(j + 1, true)
            } else {
                if j == 0 {
                    break;
                }
                self.next_visible(j - 1, false)
            };
            match next {
                Some(k) => j = k,
                None => break,
            }
            left -= 1;
        }
        j
    }

    /// The indices whose spans intersect `[a, b)` (pixels): `(first, last)` inclusive.
    #[must_use]
    pub fn indices_in(&self, a: f64, b: f64) -> (u32, u32) {
        let first = self.index_at(a.max(0.0));
        let last = self.index_at((b - 0.001).max(a));
        (first, last.max(first))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The same geometry computed naively.
    struct Naive(Vec<f64>);

    impl Naive {
        fn offset(&self, i: u32) -> f64 {
            self.0[..i as usize].iter().sum()
        }
        fn index_at(&self, px: f64) -> Option<u32> {
            let mut at = 0.0;
            for (i, s) in self.0.iter().enumerate() {
                if px >= at && px < at + s {
                    return Some(i as u32);
                }
                at += s;
            }
            None
        }
    }

    #[test]
    fn default_sizes_and_totals() {
        let a = AxisIndex::new(1_048_576, 20.0);
        assert_eq!(a.offset(0), 0.0);
        assert_eq!(a.offset(10), 200.0);
        assert_eq!(a.total(), 20.0 * 1_048_576.0);
        assert_eq!(a.index_at(0.0), 0);
        assert_eq!(a.index_at(19.99), 0);
        assert_eq!(a.index_at(20.0), 1);
        assert_eq!(a.index_at(20.0 * 1_048_575.5), 1_048_575);
        assert_eq!(a.index_at(1e12), 1_048_575);
        assert_eq!(a.index_at(-5.0), 0);
    }

    #[test]
    fn resizing_and_hiding() {
        let mut a = AxisIndex::new(1000, 20.0);
        a.set_size(3, 40.0);
        a.set_size(5, 0.0);
        a.set_size(200, 10.0);
        assert_eq!(a.offset(4), 100.0);
        assert_eq!(a.offset(6), 120.0);
        assert_eq!(a.index_at(115.0), 4);
        assert_eq!(
            a.index_at(125.0),
            6,
            "a hidden index never contains a pixel"
        );
        assert!(a.is_hidden(5));
        assert_eq!(a.total(), 1000.0 * 20.0 + 20.0 - 20.0 - 10.0);
        assert_eq!(a.step(4, 1), 6);
        assert_eq!(a.step(6, -1), 4);
        assert_eq!(a.range_span(3, 4), (60.0, 120.0));
        a.set_size(3, 20.0);
        assert_eq!(a.offset(4), 80.0);
    }

    #[test]
    fn matches_the_sheet_axis() {
        use kiln_sheet::axis::{RowProps, row_axis};
        use kiln_sheet::num::F64;
        let mut rows = row_axis(15.0);
        rows.set(
            2,
            Some(RowProps {
                height: Some(F64(30.0)),
                ..RowProps::default()
            }),
        );
        rows.update(4, |p| p.hidden = true);
        let a = AxisIndex::from_axis(&rows);
        for i in [0, 2, 3, 5, 100, 1_048_575] {
            assert_eq!(a.offset(i), rows.offset_px(i), "row {i}");
        }
        assert_eq!(a.index_at(105.0), rows.index_at_px(105.0));
    }

    #[test]
    fn agrees_with_the_naive_sums() {
        // A deterministic mix of default, custom and hidden sizes across block edges.
        let len = 700u32;
        let mut sizes = vec![17.0; len as usize];
        let mut a = AxisIndex::new(len, 17.0);
        let mut x: u32 = 12345;
        for _ in 0..400 {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let i = (x >> 8) % len;
            let s = f64::from((x >> 20) % 5) * 9.0;
            sizes[i as usize] = s;
            a.set_size(i, s);
        }
        let n = Naive(sizes);
        for i in (0..=len).step_by(7) {
            assert!((a.offset(i) - n.offset(i)).abs() < 1e-6, "offset {i}");
        }
        let total = n.offset(len);
        let mut px = 0.0;
        while px < total {
            if let Some(i) = n.index_at(px) {
                assert_eq!(a.index_at(px), i, "px {px}");
            }
            px += 3.7;
        }
    }
}
