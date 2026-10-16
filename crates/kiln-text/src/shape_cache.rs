// SPDX-License-Identifier: AGPL-3.0-or-later
//! The word shaping cache.
//!
//! A paragraph is cut into shaping segments that cannot interact typographically: a word with its
//! trailing spaces, a single CJK character, a tab, or a stretch of one bidi direction. Each segment is
//! shaped once per (style, direction, text) and reused, as Chromium's word cache and LibreOffice's
//! glyph cache do: documents repeat words constantly, so re-laying out an edited paragraph or a
//! whole document after a width change shapes almost nothing. Misses of one paragraph are shaped
//! together in a single parley layout per direction, separated by line breaks.

use std::hash::{BuildHasher, Hash, Hasher};

use hashbrown::{Equivalent, HashMap};
use rustc_hash::FxBuildHasher;

use crate::fonts::FontId;

/// Segments longer than this are shaped but not cached.
pub(crate) const MAX_CACHED_SEGMENT: usize = 256;

/// The cache is dropped when it holds this many segments (a few MB at most).
const MAX_ENTRIES: usize = 65_536;

/// A glyph relative to its cluster's pen position.
#[derive(Debug, Clone, Copy)]
pub(crate) struct RawGlyph {
    pub(crate) id: u32,
    pub(crate) x: f32,
    pub(crate) y: f32,
    pub(crate) advance: f32,
}

/// One shaped grapheme cluster of a segment, with byte offsets relative to the segment.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ClusterTemplate {
    pub(crate) start: u32,
    pub(crate) end: u32,
    pub(crate) advance: f32,
    pub(crate) glyph_start: u32,
    pub(crate) glyph_end: u32,
    pub(crate) font: FontId,
    pub(crate) size: f32,
    /// Line-breaking class of the cluster, encoded by the layout module.
    pub(crate) class: u8,
}

/// A shaped segment.
#[derive(Debug, Clone, Default)]
pub(crate) struct ShapedSegment {
    pub(crate) clusters: Vec<ClusterTemplate>,
    pub(crate) glyphs: Vec<RawGlyph>,
}

/// Everything about a span's style that affects shaping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct StyleKey {
    pub(crate) family: String,
    pub(crate) size: u32,
    pub(crate) weight: u16,
    pub(crate) italic: bool,
    pub(crate) letter_spacing: u32,
    pub(crate) kerning: bool,
    pub(crate) ligatures: bool,
    pub(crate) lang: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SegmentKey {
    style: u32,
    rtl: bool,
    text: Box<str>,
}

impl Hash for SegmentKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_parts(self.style, self.rtl, &self.text, state);
    }
}

/// A borrowed lookup key (no allocation on a hit).
struct SegmentRef<'a> {
    style: u32,
    rtl: bool,
    text: &'a str,
}

impl Hash for SegmentRef<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        hash_parts(self.style, self.rtl, self.text, state);
    }
}

impl Equivalent<SegmentKey> for SegmentRef<'_> {
    fn equivalent(&self, key: &SegmentKey) -> bool {
        self.style == key.style && self.rtl == key.rtl && self.text == &*key.text
    }
}

fn hash_parts<H: Hasher>(style: u32, rtl: bool, text: &str, state: &mut H) {
    state.write_u32(style);
    state.write_u8(u8::from(rtl));
    state.write(text.as_bytes());
}

/// The cache.
#[derive(Debug, Default)]
pub(crate) struct ShapeCache {
    segments: HashMap<SegmentKey, ShapedSegment, FxBuildHasher>,
    styles: HashMap<StyleKey, u32, FxBuildHasher>,
    hits: u64,
    misses: u64,
}

impl ShapeCache {
    /// The id of a style (stable until the cache is cleared).
    pub(crate) fn style_id(&mut self, key: StyleKey) -> u32 {
        let next = u32::try_from(self.styles.len()).unwrap_or(u32::MAX);
        *self.styles.entry(key).or_insert(next)
    }

    pub(crate) fn get(&self, style: u32, rtl: bool, text: &str) -> Option<&ShapedSegment> {
        let probe = SegmentRef { style, rtl, text };
        let hash = self.segments.hasher().hash_one(&probe);
        self.segments
            .raw_entry()
            .from_hash(hash, |k| probe.equivalent(k))
            .map(|(_, v)| v)
    }

    /// Counts lookups of one paragraph.
    pub(crate) fn record(&mut self, hits: u64, misses: u64) {
        self.hits += hits;
        self.misses += misses;
    }

    pub(crate) fn contains(&self, style: u32, rtl: bool, text: &str) -> bool {
        self.segments
            .get(&SegmentRef { style, rtl, text })
            .is_some()
    }

    pub(crate) fn insert(&mut self, style: u32, rtl: bool, text: &str, shaped: ShapedSegment) {
        if text.len() > MAX_CACHED_SEGMENT {
            return;
        }
        self.segments.insert(
            SegmentKey {
                style,
                rtl,
                text: text.into(),
            },
            shaped,
        );
    }

    /// Drops everything when the cache is full. Called between paragraphs only, because style ids
    /// handed out for the current paragraph must stay valid while it is laid out.
    pub(crate) fn trim(&mut self) {
        if self.segments.len() >= MAX_ENTRIES {
            self.clear();
        }
    }

    pub(crate) fn clear(&mut self) {
        self.segments.clear();
        self.styles.clear();
    }

    pub(crate) fn len(&self) -> usize {
        self.segments.len()
    }

    pub(crate) fn stats(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }
}
