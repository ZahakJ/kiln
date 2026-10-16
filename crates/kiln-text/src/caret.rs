// SPDX-License-Identifier: AGPL-3.0-or-later
//! Carets, hit testing, selection geometry and text boundaries on a laid-out paragraph.
//!
//! Positions are byte offsets into the paragraph text at grapheme cluster boundaries. In bidi text a
//! logical position maps to the edge of the character it precedes (or, at the end of a line, follows):
//! the leading edge of an LTR character is its left side, of an RTL character its right side.

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::output::{Affinity, CaretRect, ClusterBox, Line, ParagraphLayout, Rect};

impl ParagraphLayout {
    /// Snaps a byte offset to the nearest caret position at or before it.
    #[must_use]
    pub fn snap(&self, index: usize) -> usize {
        match self.boundaries.binary_search(&index) {
            Ok(i) => self.boundaries[i],
            Err(0) => 0,
            Err(i) => self.boundaries[i - 1],
        }
    }

    /// The line a caret position is on, with downstream affinity (a position at a line wrap belongs
    /// to the next line).
    #[must_use]
    pub fn line_of(&self, index: usize, affinity: Affinity) -> usize {
        let last = self.lines.len().saturating_sub(1);
        for (i, line) in self.lines.iter().enumerate() {
            let r = &line.text_range;
            if index < r.end || i == last {
                return i;
            }
            if index == r.end {
                // At the end of a wrapped line: upstream stays here; a manual break moves on.
                let hard = self.text[..r.end].ends_with(['\n', '\r', '\u{b}', '\u{2028}']);
                if affinity == Affinity::Upstream && !hard {
                    return i;
                }
            }
        }
        last
    }

    /// The caret for a position (downstream affinity).
    #[must_use]
    pub fn caret_rect(&self, index: usize) -> CaretRect {
        self.caret_rect_with(index, Affinity::Downstream)
    }

    /// The caret for a position with an explicit affinity.
    #[must_use]
    pub fn caret_rect_with(&self, index: usize, affinity: Affinity) -> CaretRect {
        let index = self.snap(index.min(self.text.len()));
        let li = self.line_of(index, affinity);
        let Some(line) = self.lines.get(li) else {
            return CaretRect {
                x: 0.0,
                top: 0.0,
                height: 0.0,
                line: 0,
                rtl: self.rtl,
            };
        };
        let (x, rtl) = caret_x(line, index, self.rtl);
        CaretRect {
            x,
            top: line.baseline - line.ascent,
            height: line.ascent + line.descent,
            line: li,
            rtl,
        }
    }

    /// The caret position nearest to a point.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> usize {
        self.hit_test_with_affinity(x, y).0
    }

    /// The caret position nearest to a point and the affinity that keeps the caret on the line that
    /// was hit.
    #[must_use]
    pub fn hit_test_with_affinity(&self, x: f32, y: f32) -> (usize, Affinity) {
        if self.lines.is_empty() {
            return (0, Affinity::Downstream);
        }
        let li = self
            .lines
            .iter()
            .position(|l| y < l.top + l.height)
            .unwrap_or(self.lines.len() - 1);
        let line = &self.lines[li];
        let index = hit_line(line, x, self.rtl);
        let affinity = if index == line.text_range.end && li + 1 < self.lines.len() {
            Affinity::Upstream
        } else {
            Affinity::Downstream
        };
        (index, affinity)
    }

    /// Highlight rectangles for a byte range (one or more per line, merged where adjacent).
    #[must_use]
    pub fn selection_rects(&self, range: Range<usize>) -> Vec<Rect> {
        let mut rects: Vec<Rect> = Vec::new();
        if range.start >= range.end {
            return rects;
        }
        for line in &self.lines {
            let top = line.top;
            let height = line.height;
            let mut cur: Option<(f32, f32)> = None;
            for c in &line.clusters {
                let inside = c.text_range.start < range.end && c.text_range.end > range.start;
                if inside {
                    cur = Some(match cur {
                        Some((a, b)) if (c.x - b).abs() < 0.01 => (a, c.x + c.advance),
                        Some((a, b)) => {
                            rects.push(Rect {
                                x: a,
                                y: top,
                                width: b - a,
                                height,
                            });
                            (c.x, c.x + c.advance)
                        }
                        None => (c.x, c.x + c.advance),
                    });
                } else if let Some((a, b)) = cur.take() {
                    rects.push(Rect {
                        x: a,
                        y: top,
                        width: b - a,
                        height,
                    });
                }
            }
            if let Some((a, b)) = cur {
                rects.push(Rect {
                    x: a,
                    y: top,
                    width: b - a,
                    height,
                });
            }
        }
        rects
    }

    /// The next caret position after `index` (logical order), or the text length.
    #[must_use]
    pub fn next_position(&self, index: usize) -> usize {
        match self.boundaries.binary_search(&index) {
            Ok(i) => self
                .boundaries
                .get(i + 1)
                .copied()
                .unwrap_or(self.text.len()),
            Err(i) => self.boundaries.get(i).copied().unwrap_or(self.text.len()),
        }
    }

    /// The previous caret position before `index` (logical order), or 0.
    #[must_use]
    pub fn prev_position(&self, index: usize) -> usize {
        match self.boundaries.binary_search(&index) {
            Ok(0) | Err(0) => 0,
            Ok(i) | Err(i) => self.boundaries[i - 1],
        }
    }

    /// The word (UAX #29) containing `index`: used for double-click selection.
    #[must_use]
    pub fn word_at(&self, index: usize) -> Range<usize> {
        segment_at(self.text.split_word_bound_indices(), &self.text, index)
    }

    /// Start of the next word after `index` (Ctrl+Right in LTR text).
    #[must_use]
    pub fn next_word_start(&self, index: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .filter(|(_, w)| w.chars().any(char::is_alphanumeric))
            .map(|(i, _)| i)
            .find(|&i| i > index)
            .unwrap_or(self.text.len())
    }

    /// Start of the word before `index` (Ctrl+Left in LTR text).
    #[must_use]
    pub fn prev_word_start(&self, index: usize) -> usize {
        self.text
            .split_word_bound_indices()
            .filter(|(_, w)| w.chars().any(char::is_alphanumeric))
            .map(|(i, _)| i)
            .take_while(|&i| i < index)
            .last()
            .unwrap_or(0)
    }

    /// The sentence (UAX #29) containing `index`.
    #[must_use]
    pub fn sentence_at(&self, index: usize) -> Range<usize> {
        segment_at(self.text.split_sentence_bound_indices(), &self.text, index)
    }
}

fn segment_at<'a>(
    segments: impl Iterator<Item = (usize, &'a str)>,
    text: &str,
    index: usize,
) -> Range<usize> {
    let mut last = 0..0;
    for (i, s) in segments {
        let r = i..i + s.len();
        if index < r.end {
            return r;
        }
        last = r;
    }
    if text.is_empty() { 0..0 } else { last }
}

/// x of a caret position on a line.
fn caret_x(line: &Line, index: usize, para_rtl: bool) -> (f32, bool) {
    if let Some(c) = line.clusters.iter().find(|c| c.text_range.start == index) {
        return (leading_edge(c), c.is_rtl());
    }
    if let Some(c) = line.clusters.iter().find(|c| c.text_range.end == index) {
        return (trailing_edge(c), c.is_rtl());
    }
    // Empty line, or a position past the line's clusters: the line's start edge.
    let x = if para_rtl {
        line.x + line.width
    } else {
        line.x
    };
    (x, para_rtl)
}

fn leading_edge(c: &ClusterBox) -> f32 {
    if c.is_rtl() { c.x + c.advance } else { c.x }
}

fn trailing_edge(c: &ClusterBox) -> f32 {
    if c.is_rtl() { c.x } else { c.x + c.advance }
}

/// The caret position nearest to x on a line.
///
/// At a boundary between clusters of different directions two logical positions touch the same
/// visual x; the one whose caret is drawn at that boundary wins, so clicking where a caret is shown
/// puts the caret there.
fn hit_line(line: &Line, x: f32, para_rtl: bool) -> usize {
    let clusters = &line.clusters;
    if clusters.is_empty() {
        return line.text_range.start;
    }
    let is_break = |c: &ClusterBox| c.advance <= 0.0 && c.text_range.end == line.text_range.end;
    // Visually leftmost and rightmost clusters that are not a zero-width line break.
    let visible: Vec<usize> = (0..clusters.len())
        .filter(|&i| !is_break(&clusters[i]))
        .collect();
    let (Some(&first), Some(&last)) = (visible.first(), visible.last()) else {
        return clusters[0].text_range.start;
    };
    let (index, left_half) = if x < clusters[first].x {
        (first, true)
    } else if let Some(i) = visible
        .iter()
        .copied()
        .find(|&i| x < clusters[i].x + clusters[i].advance)
    {
        (i, x < clusters[i].x + clusters[i].advance * 0.5)
    } else {
        (last, false)
    };
    let c = &clusters[index];
    let p = side(c, left_half);
    let boundary = if left_half { c.x } else { c.x + c.advance };
    if (caret_x(line, p, para_rtl).0 - boundary).abs() > 0.01 {
        let neighbour = if left_half {
            index.checked_sub(1)
        } else {
            Some(index + 1)
        };
        if let Some(n) = neighbour
            .and_then(|n| clusters.get(n))
            .filter(|n| !is_break(n))
        {
            let q = side(n, !left_half);
            if (caret_x(line, q, para_rtl).0 - boundary).abs() <= 0.01 {
                return q;
            }
        }
    }
    p
}

/// The logical position at the left or right side of a cluster.
fn side(c: &ClusterBox, left: bool) -> usize {
    if left != c.is_rtl() {
        c.text_range.start
    } else {
        c.text_range.end
    }
}
