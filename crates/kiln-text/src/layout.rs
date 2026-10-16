// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paragraph layout: shaping through parley (harfrust), then Word-compatible line breaking, tabs,
//! hyphenation, alignment, bidi reordering and line spacing done here.
//!
//! Parley shapes the whole paragraph once (itemisation, font selection with fallback, OpenType
//! shaping) and yields grapheme clusters with advances. Line fitting is ours because Word's rules
//! differ from CSS: tab stops measured from the margin, hanging indents with an implicit stop, spaces
//! that hang past the margin, justification that stretches only the spaces after the last tab, manual
//! line breaks that are justified, and line heights from the Windows metrics.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::ops::Range;

use parley::setting::Tag;
use parley::{
    BaseDirection, FontFamily, FontFamilyName, FontFeature, FontStyle, FontWeight, Language,
    LayoutContext, StyleRunBuilder, TextStyle,
};
use rustc_hash::FxBuildHasher;
use unicode_bidi::{BidiClass, BidiInfo, Level, bidi_class};
use unicode_linebreak::{BreakOpportunity, linebreaks};

use crate::fonts::{FontId, FontStore};
use crate::hyphen::{HyphLang, Hyphenators};
use crate::output::{
    ClusterBox, Decoration, DecorationKind, GlyphRun, Line, LineEnd, ParagraphLayout,
    PositionedGlyph,
};
use crate::shape_cache::{
    ClusterTemplate, MAX_CACHED_SEGMENT, RawGlyph, ShapeCache, ShapedSegment, StyleKey,
};
use crate::spec::{
    Alignment, Direction, LineSpacing, ParagraphSpec, SpanStyle, StrikeKind, TabAlignment,
    TabLeader, UnderlineKind,
};

/// Tolerance for width comparisons (px).
const EPS: f32 = 1e-3;

/// Largest paragraph coordinate magnitude in px (about 26 m at 96 dpi, far beyond Word's
/// 22-inch page limit). Widths, indents and tab stops from documents are clamped to it so that
/// f32 positions keep sub-pixel precision and loops stepping across a line always advance.
pub(crate) const MAX_EXTENT: f32 = 100_000.0;

/// Upper bound on the glyphs one tab leader emits, whatever the geometry.
const MAX_LEADER_GLYPHS: usize = 16_384;

/// Words longer than this many clusters are not hyphenated (long URLs, base64 blobs, pasted
/// identifiers). Real words are far shorter; browsers and LibreOffice apply similar bounds, and
/// it keeps hyphenation cost per line constant.
const MAX_HYPHENATED_WORD: usize = 96;

/// A finite coordinate clamped to [`MAX_EXTENT`]; non-finite values become 0.
fn coord(v: f32) -> f32 {
    if v.is_finite() {
        v.clamp(-MAX_EXTENT, MAX_EXTENT)
    } else {
        0.0
    }
}

const NO_KERN: &[FontFeature] = &[FontFeature::new(Tag::new(b"kern"), 0)];
const NO_LIGA: &[FontFeature] = &[
    FontFeature::new(Tag::new(b"liga"), 0),
    FontFeature::new(Tag::new(b"clig"), 0),
];
const NO_KERN_LIGA: &[FontFeature] = &[
    FontFeature::new(Tag::new(b"kern"), 0),
    FontFeature::new(Tag::new(b"liga"), 0),
    FontFeature::new(Tag::new(b"clig"), 0),
];

/// What a cluster is, for line breaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Normal,
    /// A breaking space: hangs past the end margin.
    Space,
    Tab,
    LineBreak,
    SoftHyphen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Brk {
    None,
    Allowed,
    Mandatory,
}

/// One grapheme cluster in logical order.
#[derive(Debug, Clone)]
struct Cl {
    range: Range<usize>,
    advance: f32,
    glyphs: Range<usize>,
    font: FontId,
    size: f32,
    span: usize,
    level: u8,
    kind: Kind,
    brk: Brk,
    /// Ideographic or syllabic East Asian character (inter-character justification).
    cjk: bool,
    /// Joining script (Arabic and friends): never stretched between characters.
    joining: bool,
}

/// An automatic or soft hyphen shown at a line end.
#[derive(Debug, Clone, Copy)]
struct Hyphen {
    glyph: u32,
    advance: f32,
    font: FontId,
    size: f32,
    span: usize,
}

/// The outcome of fitting one line.
#[derive(Debug, Clone)]
struct Fit {
    start: usize,
    end: usize,
    end_kind: LineEnd,
    hyphen: Option<Hyphen>,
}

/// Lays out paragraphs. Owns the [`FontStore`] and parley's scratch space; create one per thread and
/// reuse it.
pub struct TextEngine {
    fonts: FontStore,
    lcx: LayoutContext<()>,
    playout: parley::Layout<()>,
    hyph: Hyphenators,
    // Scratch buffers reused between paragraphs.
    cls: Vec<Cl>,
    glyphs: Vec<RawGlyph>,
    adv: Vec<f32>,
    leaders: Vec<TabLeader>,
    /// The paragraph text being laid out.
    playout_text: String,
    cache: ShapeCache,
    /// Font sizing manual line breaks, per style id.
    break_fonts: HashMap<u32, FontId, FxBuildHasher>,
}

impl std::fmt::Debug for TextEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextEngine")
            .field("fonts", &self.fonts)
            .finish_non_exhaustive()
    }
}

impl Default for TextEngine {
    fn default() -> Self {
        Self::new(FontStore::default())
    }
}

/// Paragraph geometry derived from the spec, in start-edge coordinates.
#[derive(Debug, Clone, Copy)]
struct Frame {
    width: f32,
    indent_start: f32,
    first_line: f32,
    limit: f32,
}

impl TextEngine {
    /// An engine over a font store.
    #[must_use]
    pub fn new(fonts: FontStore) -> Self {
        Self {
            fonts,
            lcx: LayoutContext::new(),
            playout: parley::Layout::new(),
            hyph: Hyphenators::default(),
            cls: Vec::new(),
            glyphs: Vec::new(),
            adv: Vec::new(),
            leaders: Vec::new(),
            playout_text: String::new(),
            cache: ShapeCache::default(),
            break_fonts: HashMap::default(),
        }
    }

    /// The font store.
    #[must_use]
    pub fn fonts(&self) -> &FontStore {
        &self.fonts
    }

    /// The font store, mutably (to add fonts or change fallbacks).
    pub fn fonts_mut(&mut self) -> &mut FontStore {
        // Fonts or fallbacks may change: shaped words are no longer trustworthy.
        self.cache.clear();
        self.break_fonts.clear();
        &mut self.fonts
    }

    /// Word shaping cache statistics: (cached segments, hits, misses) since the engine was created.
    #[must_use]
    pub fn shape_cache_stats(&self) -> (usize, u64, u64) {
        let (hits, misses) = self.cache.stats();
        (self.cache.len(), hits, misses)
    }

    /// Consumes the engine and returns its font store.
    #[must_use]
    pub fn into_fonts(self) -> FontStore {
        self.fonts
    }

    /// Lays out one paragraph.
    pub fn layout(&mut self, spec: &ParagraphSpec) -> ParagraphLayout {
        self.playout_text.clear();
        for run in &spec.runs {
            self.playout_text.push_str(&run.text);
        }
        let text = self.playout_text.clone();
        let width = coord(spec.width).max(0.0);
        let props = &spec.props;
        let indent_start = coord(props.indent_start);
        let indent_end = coord(props.indent_end);
        let frame = Frame {
            width,
            indent_start,
            first_line: coord(props.first_line),
            limit: width - indent_end,
        };

        // Resolve families once per run.
        let mut substitutions = Vec::new();
        let families: Vec<String> = spec
            .runs
            .iter()
            .map(|r| {
                let res = self.fonts.resolve_ref(&r.style.family);
                if res.kind != crate::SubstitutionKind::Exact && !substitutions.contains(res) {
                    substitutions.push(res.clone());
                }
                res.family.clone()
            })
            .collect();

        // Bidi levels (UAX #9). Manual line breaks are not paragraph separators in Word, so they are
        // replaced by spaces for the analysis (same byte length).
        let para_level = match props.direction {
            Direction::Ltr => Some(Level::ltr()),
            Direction::Rtl => Some(Level::rtl()),
            Direction::Auto => None,
        };
        // Left-to-right text without right-to-left characters or bidi controls resolves to level 0
        // everywhere: skip the analysis (the common case).
        let (levels, base_level): (Vec<Level>, Level) =
            if props.direction != Direction::Rtl && !needs_bidi(&text) {
                (Vec::new(), Level::ltr())
            } else {
                let bidi_text: Cow<'_, str> =
                    if text.contains(['\n', '\r', '\u{b}', '\u{c}', '\u{85}', '\u{2029}']) {
                        // Same byte lengths, so levels stay aligned with the text.
                        Cow::Owned(
                            text.chars()
                                .map(|c| match c {
                                    '\n' | '\r' | '\u{b}' | '\u{c}' => ' ',
                                    '\u{85}' => '\u{a0}',
                                    '\u{2029}' => '\u{2028}',
                                    c => c,
                                })
                                .collect(),
                        )
                    } else {
                        Cow::Borrowed(&text)
                    };
                let bidi = BidiInfo::new(&bidi_text, para_level);
                let base = bidi
                    .paragraphs
                    .first()
                    .map_or_else(|| para_level.unwrap_or(Level::ltr()), |p| p.level);
                (bidi.levels, base)
            };
        let rtl = base_level.is_rtl();

        self.shape(spec, &text, &families, &levels, base_level.number());
        self.assign_levels(spec, &text, &levels, base_level.number());
        self.assign_breaks(&text);

        let mut lines = Vec::new();
        let mut y = coord(props.space_before).max(0.0);
        if self.cls.is_empty() {
            lines.push(self.empty_line(spec, &families, frame, rtl, y, text.len()));
            y += lines[0].height;
        } else {
            let mut start = 0;
            let mut first = true;
            while start < self.cls.len() {
                let fit = self.fit_line(spec, frame, start, first);
                let line = self.finish_line(spec, frame, &fit, rtl, base_level.number(), y, &text);
                y += line.height;
                start = fit.end.max(start + 1);
                first = false;
                lines.push(line);
            }
            // A paragraph that ends with a manual line break gets an empty last line, as in Word.
            if self.cls.last().is_some_and(|c| c.kind == Kind::LineBreak) {
                let mut line = self.empty_line(spec, &families, frame, rtl, y, text.len());
                line.text_range = text.len()..text.len();
                y += line.height;
                lines.push(line);
            }
        }
        let height = y + coord(props.space_after).max(0.0);

        let mut boundaries: Vec<usize> = self.cls.iter().map(|c| c.range.start).collect();
        boundaries.push(text.len());
        boundaries.dedup();

        ParagraphLayout {
            text,
            width,
            height,
            rtl,
            lines,
            substitutions,
            boundaries,
        }
    }

    /// Shapes the paragraph segment by segment through the word cache, shaping the misses with
    /// parley, and collects clusters in logical order.
    fn shape(
        &mut self,
        spec: &ParagraphSpec,
        text: &str,
        families: &[String],
        levels: &[Level],
        base: u8,
    ) {
        self.cls.clear();
        self.glyphs.clear();
        self.adv.clear();
        self.leaders.clear();
        if text.is_empty() {
            return;
        }
        self.cache.trim();
        if self.cache.len() == 0 {
            self.break_fonts.clear();
        }
        let style_ids: Vec<u32> = spec
            .runs
            .iter()
            .zip(families)
            .map(|(r, f)| self.cache.style_id(style_key(&r.style, f)))
            .collect();
        let segs = segments(spec, levels, base);

        // Shape the segments the cache does not know, one parley layout per direction.
        let mut local: Vec<(usize, ShapedSegment)> = Vec::new();
        let mut missed = 0_u64;
        for rtl in [false, true] {
            let mut batch: Vec<usize> = Vec::new();
            let mut seen: HashSet<(u32, &str), FxBuildHasher> = HashSet::default();
            for (i, s) in segs.iter().enumerate() {
                let t = &text[s.range.clone()];
                if s.rtl == rtl
                    && !s.line_break
                    && !self.cache.contains(style_ids[s.span], rtl, t)
                    && (t.len() > MAX_CACHED_SEGMENT || seen.insert((style_ids[s.span], t)))
                {
                    batch.push(i);
                }
            }
            if batch.is_empty() {
                continue;
            }
            missed += batch.len() as u64;
            let shaped = self.shape_batch(spec, text, families, &segs, &batch, rtl);
            for (i, seg) in batch.into_iter().zip(shaped) {
                let s = &segs[i];
                let t = &text[s.range.clone()];
                if t.len() > MAX_CACHED_SEGMENT {
                    local.push((i, seg));
                } else {
                    self.cache.insert(style_ids[s.span], rtl, t, seg);
                }
            }
        }

        let shaped_segments = segs.iter().filter(|s| !s.line_break).count() as u64;
        self.cache
            .record(shaped_segments.saturating_sub(missed), missed);

        // Assemble the paragraph from cached and freshly shaped segments.
        for (i, s) in segs.iter().enumerate() {
            let t = &text[s.range.clone()];
            if s.line_break {
                let font = self.break_font(spec, families, s.span, style_ids[s.span]);
                self.cls.push(Cl {
                    range: s.range.clone(),
                    advance: 0.0,
                    glyphs: self.glyphs.len()..self.glyphs.len(),
                    font,
                    size: sanitize_size(spec.runs[s.span].style.size),
                    span: s.span,
                    level: 0,
                    kind: Kind::LineBreak,
                    brk: Brk::None,
                    cjk: false,
                    joining: false,
                });
                continue;
            }
            let shaped = match local.iter().find(|(k, _)| *k == i) {
                Some((_, seg)) => Some(seg),
                None => self.cache.get(style_ids[s.span], s.rtl, t),
            };
            let Some(shaped) = shaped else { continue };
            for c in &shaped.clusters {
                let start = s.range.start + c.start as usize;
                let end = s.range.start + c.end as usize;
                let (kind, cjk, joining) = decode_class(c.class);
                let g0 = self.glyphs.len();
                self.glyphs.extend_from_slice(
                    &shaped.glyphs[c.glyph_start as usize..c.glyph_end as usize],
                );
                self.cls.push(Cl {
                    range: start..end,
                    advance: if kind == Kind::SoftHyphen {
                        0.0
                    } else {
                        c.advance
                    },
                    glyphs: g0..self.glyphs.len(),
                    font: c.font,
                    size: c.size,
                    span: s.span,
                    level: 0,
                    kind,
                    brk: Brk::None,
                    cjk,
                    joining,
                });
            }
        }
        let n = self.cls.len();
        self.adv.extend(self.cls.iter().map(|c| c.advance));
        self.leaders.resize(n, TabLeader::None);
    }

    /// Shapes a batch of segments of one direction in a single parley layout, separated by line
    /// breaks so they cannot interact. Returns one shaped segment per batch entry.
    fn shape_batch(
        &mut self,
        spec: &ParagraphSpec,
        text: &str,
        families: &[String],
        segs: &[Seg],
        batch: &[usize],
        rtl: bool,
    ) -> Vec<ShapedSegment> {
        let mut btext = String::with_capacity(batch.iter().map(|&i| segs[i].range.len() + 1).sum());
        let mut starts = Vec::with_capacity(batch.len());
        for &i in batch {
            starts.push(btext.len());
            btext.push_str(&text[segs[i].range.clone()]);
            btext.push('\n');
        }
        let mut builder: StyleRunBuilder<'_, ()> =
            self.lcx
                .style_run_builder(&mut self.fonts.cx, &btext, 1.0, false);
        let mut style_index: Vec<Option<u16>> = vec![None; spec.runs.len()];
        for &i in batch {
            let span = segs[i].span;
            if style_index[span].is_none() {
                style_index[span] =
                    Some(builder.push_style(text_style(&spec.runs[span].style, &families[span])));
            }
        }
        for (k, &i) in batch.iter().enumerate() {
            let idx = style_index[segs[i].span].unwrap_or(0);
            builder.push_style_run(idx, starts[k]..starts[k] + segs[i].range.len() + 1);
        }
        builder.set_base_direction(if rtl {
            BaseDirection::Rtl
        } else {
            BaseDirection::Ltr
        });
        builder.build_into(&mut self.playout, &btext);
        self.playout.break_all_lines(None);

        let mut out: Vec<ShapedSegment> = vec![ShapedSegment::default(); batch.len()];
        let fonts = &mut self.fonts;
        for line in self.playout.lines() {
            for run in line.runs() {
                let font = run.font();
                let synthesis = run.synthesis();
                let coords: Vec<i16> = run
                    .normalized_coords()
                    .iter()
                    .map(|c| c.to_bits())
                    .collect();
                let id = fonts.intern(
                    &font.data,
                    font.index,
                    &coords,
                    synthesis.embolden(),
                    synthesis.skew(),
                );
                let size = run.font_size();
                for cluster in run.clusters() {
                    let range = cluster.text_range();
                    let k = starts
                        .partition_point(|&s| s <= range.start)
                        .saturating_sub(1);
                    let seg_len = segs[batch[k]].range.len();
                    let rel = range.start - starts[k];
                    if rel >= seg_len {
                        continue; // the separator
                    }
                    let seg = &mut out[k];
                    let g0 = seg.glyphs.len();
                    seg.glyphs.extend(cluster.glyphs().map(|g| RawGlyph {
                        id: g.id,
                        x: g.x,
                        y: g.y,
                        advance: g.advance,
                    }));
                    let to32 = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
                    let first = btext[range.clone()].chars().next().unwrap_or(' ');
                    seg.clusters.push(ClusterTemplate {
                        start: to32(rel),
                        end: to32((range.end - starts[k]).min(seg_len)),
                        advance: cluster.advance(),
                        glyph_start: to32(g0),
                        glyph_end: to32(seg.glyphs.len()),
                        font: id,
                        size,
                        class: encode_class(first),
                    });
                }
            }
        }
        for seg in &mut out {
            seg.clusters.sort_by_key(|c| c.start);
        }
        out
    }

    /// The font that sizes a manual line break in a span (its family's regular face).
    fn break_font(
        &mut self,
        spec: &ParagraphSpec,
        families: &[String],
        span: usize,
        style_id: u32,
    ) -> FontId {
        if let Some(id) = self.break_fonts.get(&style_id) {
            return *id;
        }
        let style = &spec.runs[span].style;
        let id = self
            .fonts
            .primary_font(&families[span], style.weight, style.italic)
            .or_else(|| self.cls.last().map(|c| c.font))
            .unwrap_or(FontId(0));
        self.break_fonts.insert(style_id, id);
        id
    }

    fn assign_levels(&mut self, spec: &ParagraphSpec, text: &str, levels: &[Level], base: u8) {
        for c in &mut self.cls {
            let mut level = levels.get(c.range.start).map_or(base, |l| l.number());
            let rtl_run = spec.runs.get(c.span).is_some_and(|r| r.style.rtl);
            if rtl_run && level % 2 == 0 {
                let ch = text[c.range.clone()].chars().next().unwrap_or(' ');
                if matches!(
                    bidi_class(ch),
                    BidiClass::WS | BidiClass::ON | BidiClass::CS | BidiClass::ES | BidiClass::ET
                ) {
                    level += 1;
                }
            }
            c.level = level;
        }
    }

    fn assign_breaks(&mut self, text: &str) {
        let mut k = 0;
        for (pos, opp) in linebreaks(text) {
            while k < self.cls.len() && self.cls[k].range.end < pos {
                k += 1;
            }
            if k < self.cls.len() && self.cls[k].range.end == pos {
                self.cls[k].brk = match opp {
                    BreakOpportunity::Mandatory => Brk::Mandatory,
                    BreakOpportunity::Allowed => Brk::Allowed,
                };
            }
        }
        // Word never breaks between a character and a following tab's left side, but always allows
        // a break after a tab; unicode-linebreak already does both. Manual breaks are mandatory.
        for c in &mut self.cls {
            if c.kind == Kind::LineBreak {
                c.brk = Brk::Mandatory;
            }
        }
    }

    /// Advance of the tab at cluster `j` when it starts at `x` (start-edge coordinates).
    fn tab_advance(
        &self,
        spec: &ParagraphSpec,
        frame: Frame,
        j: usize,
        x: f32,
        first: bool,
    ) -> (f32, TabLeader) {
        let props = &spec.props;
        // The first custom stop after x.
        let mut best: Option<(f32, TabAlignment, TabLeader)> = None;
        for stop in &props.tab_stops {
            let position = coord(stop.position);
            if stop.position.is_finite()
                && position > x + EPS
                && best.is_none_or(|(p, _, _)| position < p)
            {
                best = Some((position, stop.alignment, stop.leader));
            }
        }
        // Word puts an implicit stop at the indent of a hanging first line.
        let hanging_stop = frame.indent_start;
        if first
            && frame.first_line < 0.0
            && hanging_stop > x + EPS
            && best.is_none_or(|(p, _, _)| hanging_stop < p)
        {
            best = Some((hanging_stop, TabAlignment::Start, TabLeader::None));
        }
        let (pos, align, leader) = best.unwrap_or_else(|| {
            let interval = if props.default_tab.is_finite() && props.default_tab >= 1.0 {
                props.default_tab.min(MAX_EXTENT)
            } else {
                48.0
            };
            let next = ((x + EPS) / interval).floor() * interval + interval;
            (next, TabAlignment::Start, TabLeader::None)
        });
        let shift = match align {
            TabAlignment::Start => 0.0,
            TabAlignment::End => self.segment_width(j, None),
            TabAlignment::Center => self.segment_width(j, None) * 0.5,
            TabAlignment::Decimal => self.segment_width(j, Some(props.decimal_separator)),
        };
        ((pos - x - shift).max(0.0), leader)
    }

    /// Width of the text after the tab at `j` up to the next tab or line break (or up to the decimal
    /// separator).
    fn segment_width(&self, j: usize, until: Option<char>) -> f32 {
        let mut w = 0.0;
        for c in &self.cls[j + 1..] {
            if matches!(c.kind, Kind::Tab | Kind::LineBreak) {
                break;
            }
            if let Some(sep) = until
                && c.kind == Kind::Normal
                && self.cluster_starts_with(c, sep)
            {
                return w;
            }
            w += c.advance;
        }
        w
    }

    fn cluster_starts_with(&self, c: &Cl, ch: char) -> bool {
        // Text is not stored on the engine; the separator test uses the glyph-independent cluster
        // text through the parley layout's source, which is the paragraph text.
        self.text_of(c).starts_with(ch)
    }

    fn text_of(&self, c: &Cl) -> &str {
        self.playout_text.get(c.range.clone()).unwrap_or("")
    }

    /// Fits one line starting at cluster `start`.
    fn fit_line(&mut self, spec: &ParagraphSpec, frame: Frame, start: usize, first: bool) -> Fit {
        let start_x = frame.indent_start + if first { frame.first_line } else { 0.0 };
        let mut x = start_x;
        // Break candidates: (cluster index to end at, x at that point, ends after a soft hyphen).
        let mut candidates: Vec<(usize, f32, bool)> = Vec::new();
        let n = self.cls.len();
        let mut j = start;
        while j < n {
            let kind = self.cls[j].kind;
            let w = if kind == Kind::Tab {
                let (w, leader) = self.tab_advance(spec, frame, j, x, first);
                self.leaders[j] = leader;
                // A tab whose stop lies past the end indent stretches to the indent and no
                // further; the text after it wraps (Word).
                if x + w > frame.limit {
                    (frame.limit - x).max(0.0)
                } else {
                    w
                }
            } else {
                self.cls[j].advance
            };
            self.adv[j] = w;
            match kind {
                Kind::LineBreak => {
                    return Fit {
                        start,
                        end: j + 1,
                        end_kind: LineEnd::LineBreak,
                        hyphen: None,
                    };
                }
                Kind::Space | Kind::Tab => x += w,
                _ => {
                    if x + w > frame.limit + EPS && j > start {
                        return self.overflow(spec, frame, start, start_x, j, &candidates);
                    }
                    x += w;
                }
            }
            match self.cls[j].brk {
                Brk::Mandatory => {
                    return Fit {
                        start,
                        end: j + 1,
                        end_kind: if j + 1 == n {
                            LineEnd::Paragraph
                        } else {
                            LineEnd::LineBreak
                        },
                        hyphen: None,
                    };
                }
                Brk::Allowed => candidates.push((j + 1, x, kind == Kind::SoftHyphen)),
                Brk::None => {}
            }
            j += 1;
        }
        Fit {
            start,
            end: n,
            end_kind: LineEnd::Paragraph,
            hyphen: None,
        }
    }

    /// The cluster at `j` does not fit: hyphenate the word it belongs to, or break at the last
    /// opportunity, or break inside the word.
    fn overflow(
        &mut self,
        spec: &ParagraphSpec,
        frame: Frame,
        start: usize,
        start_x: f32,
        j: usize,
        candidates: &[(usize, f32, bool)],
    ) -> Fit {
        // Width of content (without hanging spaces) when ending before `end`.
        let content_x = |me: &Self, end: usize, x_at: f32| -> f32 {
            let mut x = x_at;
            let mut k = end;
            while k > start && me.cls[k - 1].kind == Kind::Space {
                x -= me.adv[k - 1];
                k -= 1;
            }
            x
        };
        // The word containing j starts after the last candidate.
        let word_start = candidates.last().map_or(start, |c| c.0);
        if spec.props.hyphenation
            && let Some(fit) = self.hyphenate(spec, frame, start, start_x, word_start, j)
        {
            return fit;
        }
        for &(end, x_at, shy) in candidates.iter().rev() {
            if end <= start {
                continue;
            }
            if shy {
                let hyphen = self.hyphen_for(end - 1);
                let w = content_x(self, end, x_at) + hyphen.map_or(0.0, |h| h.advance);
                if w <= frame.limit + EPS {
                    return Fit {
                        start,
                        end,
                        end_kind: LineEnd::Hyphen,
                        hyphen,
                    };
                }
                continue;
            }
            return Fit {
                start,
                end,
                end_kind: LineEnd::Wrap,
                hyphen: None,
            };
        }
        Fit {
            start,
            end: j.max(start + 1),
            end_kind: LineEnd::Emergency,
            hyphen: None,
        }
    }

    /// A hyphen glyph in the font of cluster `k` (the cluster before the break).
    fn hyphen_for(&self, k: usize) -> Option<Hyphen> {
        let mut i = k;
        // A soft hyphen's own font is fine, but look back to a visible cluster for the style.
        while i > 0 && self.cls[i].kind != Kind::Normal {
            i -= 1;
        }
        let c = &self.cls[i];
        let (glyph, advance) = self.fonts.glyph_for_char(c.font, '-', c.size)?;
        Some(Hyphen {
            glyph,
            advance,
            font: c.font,
            size: c.size,
            span: c.span,
        })
    }

    fn hyphenate(
        &mut self,
        spec: &ParagraphSpec,
        frame: Frame,
        start: usize,
        start_x: f32,
        word_start: usize,
        j: usize,
    ) -> Option<Fit> {
        let n = self.cls.len();
        // Extend the word to its end (next break opportunity), giving up on overlong tokens
        // without scanning them: the cost per line stays bounded by MAX_HYPHENATED_WORD.
        let bound = word_start.saturating_add(MAX_HYPHENATED_WORD + 2).min(n);
        if j >= bound {
            return None;
        }
        let mut word_end = j;
        while word_end < bound && self.cls[word_end].brk == Brk::None {
            word_end += 1;
        }
        if word_end == bound && bound < n {
            return None;
        }
        let word_end = (word_end + 1).min(n);
        // A line that starts inside a word (after an emergency break or an earlier hyphen) still
        // belongs to that word: measure it back to its real start, again with a bound.
        let mut real_start = word_start;
        while real_start > 0
            && self.cls[real_start - 1].brk == Brk::None
            && word_end - real_start <= MAX_HYPHENATED_WORD + 2
        {
            real_start -= 1;
        }
        if word_end - real_start > MAX_HYPHENATED_WORD + 2 {
            return None;
        }
        // Only letters are hyphenated; trim punctuation and spaces at both ends.
        let mut a = word_start;
        while a < word_end && !self.is_letter(a) {
            a += 1;
        }
        let mut b = word_end;
        while b > a && !self.is_letter(b - 1) {
            b -= 1;
        }
        if b <= a + 3 || b - a > MAX_HYPHENATED_WORD {
            return None;
        }
        if (a..b).any(|k| self.cls[k].kind != Kind::Normal || !self.is_letter(k)) {
            return None;
        }
        let lang_tag = spec
            .runs
            .get(self.cls[a].span)
            .and_then(|r| r.style.lang.as_deref());
        let lang = HyphLang::from_tag(lang_tag)?;
        let word_bytes = self.cls[a].range.start..self.cls[b - 1].range.end;
        let word = self.playout_text.get(word_bytes.clone())?.to_owned();
        let points = self.hyph.points(&word, lang);
        if points.is_empty() {
            return None;
        }
        let mut x_a = start_x;
        for k in start..a {
            x_a += self.adv[k];
        }
        for &p in points.iter().rev() {
            let at = word_bytes.start + p;
            let Some(k) = (a + 1..b).find(|&k| self.cls[k].range.start == at) else {
                continue;
            };
            if k > j + 1 {
                continue;
            }
            let Some(hyphen) = self.hyphen_for(k - 1) else {
                continue;
            };
            let w: f32 = self.adv[a..k].iter().sum::<f32>() + hyphen.advance;
            if x_a + w <= frame.limit + EPS {
                return Some(Fit {
                    start,
                    end: k,
                    end_kind: LineEnd::Hyphen,
                    hyphen: Some(hyphen),
                });
            }
        }
        None
    }

    fn is_letter(&self, k: usize) -> bool {
        self.text_of(&self.cls[k])
            .chars()
            .next()
            .is_some_and(char::is_alphabetic)
    }
}

/// A cluster or the line-end hyphen, as placed on a line.
#[derive(Debug, Clone, Copy)]
enum Item {
    Cluster(usize),
    Hyphen,
}

impl TextEngine {
    /// Positions the clusters of a fitted line.
    #[allow(clippy::too_many_arguments)]
    fn finish_line(
        &mut self,
        spec: &ParagraphSpec,
        frame: Frame,
        fit: &Fit,
        rtl: bool,
        base: u8,
        top: f32,
        text: &str,
    ) -> Line {
        let props = &spec.props;
        let (start, end) = (fit.start, fit.end);
        let first = start == 0;
        let start_x = frame.indent_start + if first { frame.first_line } else { 0.0 };
        let avail = frame.limit - start_x;

        // Trailing whitespace hangs: it is not part of the content width.
        let mut content_end = end;
        while content_end > start
            && matches!(
                self.cls[content_end - 1].kind,
                Kind::Space | Kind::LineBreak
            )
        {
            content_end -= 1;
        }
        let hyphen_adv = fit.hyphen.map_or(0.0, |h| h.advance);
        let content_w: f32 = self.adv[start..content_end].iter().sum::<f32>() + hyphen_adv;
        let trailing_w: f32 = self.adv[content_end..end].iter().sum();

        // Justification.
        let is_last = matches!(fit.end_kind, LineEnd::Paragraph);
        let mut extra = vec![0.0_f32; end - start];
        let slack = (avail - content_w).max(0.0);
        let stretch = match props.alignment {
            Alignment::Justify => !is_last && slack > EPS,
            Alignment::Distribute => slack > EPS,
            _ => false,
        };
        let mut justified = false;
        if stretch {
            let after_tab = (start..content_end)
                .rev()
                .find(|&k| self.cls[k].kind == Kind::Tab)
                .map_or(start, |k| k + 1);
            let spaces: Vec<usize> = (after_tab..content_end)
                .filter(|&k| self.cls[k].kind == Kind::Space)
                .collect();
            let inter_char = props.alignment == Alignment::Distribute
                || (spaces.is_empty() && (after_tab..content_end).any(|k| self.cls[k].cjk));
            if inter_char {
                let gaps: Vec<usize> = (after_tab..content_end.saturating_sub(1))
                    .filter(|&k| !self.cls[k].joining && self.cls[k].kind != Kind::SoftHyphen)
                    .collect();
                if !gaps.is_empty() {
                    #[allow(clippy::cast_precision_loss)]
                    let each = slack / gaps.len() as f32;
                    for k in gaps {
                        extra[k - start] = each;
                    }
                    justified = true;
                }
            } else if !spaces.is_empty() {
                #[allow(clippy::cast_precision_loss)]
                let each = slack / spaces.len() as f32;
                for k in spaces {
                    extra[k - start] = each;
                }
                justified = true;
            }
        }
        let content_w = content_w + if justified { slack } else { 0.0 };

        // Offset of the content from the start edge.
        let align = match (props.alignment, rtl) {
            (Alignment::Left, false) | (Alignment::Right, true) => Alignment::Start,
            (Alignment::Left, true) | (Alignment::Right, false) => Alignment::End,
            (a, _) => a,
        };
        let offset = match align {
            Alignment::Center => start_x + slack.max(0.0) * 0.5,
            Alignment::End => start_x + (avail - content_w).max(0.0),
            _ => start_x,
        };

        // Visual order (UAX #9 L1 + L2). Trailing whitespace, tabs and the whitespace before tabs take
        // the paragraph level.
        let mut items: Vec<Item> = (start..content_end).map(Item::Cluster).collect();
        if fit.hyphen.is_some() {
            items.push(Item::Hyphen);
        }
        items.extend((content_end..end).map(Item::Cluster));
        let mut levels: Vec<u8> = items
            .iter()
            .map(|it| match *it {
                Item::Cluster(k) => self.cls[k].level,
                Item::Hyphen => self.cls[content_end.max(start + 1) - 1].level,
            })
            .collect();
        let mut reset = true;
        for (i, it) in items.iter().enumerate().rev() {
            let kind = match *it {
                Item::Cluster(k) => self.cls[k].kind,
                Item::Hyphen => Kind::Normal,
            };
            match kind {
                Kind::Tab | Kind::LineBreak => {
                    levels[i] = base;
                    reset = true;
                }
                Kind::Space if reset => levels[i] = base,
                _ => reset = false,
            }
        }
        let order = visual_order(&levels);

        // Vertical metrics.
        let mut ascent = 0.0_f32;
        let mut descent = 0.0_f32;
        let mut gap = 0.0_f32;
        let mut last: Option<(FontId, u32, usize)> = None;
        for k in start..end {
            let c = &self.cls[k];
            // Consecutive clusters almost always share font, size and span.
            if last == Some((c.font, c.size.to_bits(), c.span)) {
                continue;
            }
            last = Some((c.font, c.size.to_bits(), c.span));
            let shift = spec
                .runs
                .get(c.span)
                .map_or(0.0, |r| r.style.baseline_shift);
            let m = self.fonts.metrics(c.font, c.size);
            ascent = ascent.max(m.ascent + shift.max(0.0));
            descent = descent.max(m.descent + (-shift).max(0.0));
            gap = gap.max(m.line_gap);
        }
        if end == self.cls.len()
            && let Some(mark) = &props.mark_style
        {
            let resolved = self.fonts.resolve(&mark.family);
            if let Some(id) = self
                .fonts
                .primary_font(&resolved.family, mark.weight, mark.italic)
            {
                let m = self.fonts.metrics(id, sanitize_size(mark.size));
                ascent = ascent.max(m.ascent);
                descent = descent.max(m.descent);
                gap = gap.max(m.line_gap);
            }
        }
        let (height, baseline) = line_box(props.line_spacing, ascent, descent, gap, top);

        // Horizontal placement, left to right.
        let total_w = content_w + trailing_w;
        let mut pen = if rtl {
            frame.width - offset - total_w
        } else {
            offset
        };
        let mut runs: Vec<GlyphRun> = Vec::new();
        let mut boxes: Vec<ClusterBox> = Vec::with_capacity(end - start);
        let mut decorations: Vec<Decoration> = Vec::new();
        let decorated = spec.runs.iter().any(|r| {
            r.style.underline != UnderlineKind::None || r.style.strike != StrikeKind::None
        });
        let mut placed: Vec<(usize, f32, f32)> = Vec::new();
        for &vi in &order {
            let level = levels[vi];
            match items[vi] {
                Item::Hyphen => {
                    let Some(h) = fit.hyphen else { continue };
                    let color = spec
                        .runs
                        .get(h.span)
                        .map_or(crate::spec::BLACK, |r| r.style.color);
                    let shift = spec
                        .runs
                        .get(h.span)
                        .map_or(0.0, |r| r.style.baseline_shift);
                    let at = self.cls[content_end.max(start + 1) - 1].range.end;
                    push_glyph(
                        &mut runs,
                        (h.font, h.size, h.span, color, level),
                        at..at,
                        PositionedGlyph {
                            id: h.glyph,
                            x: pen,
                            y: baseline - shift,
                            advance: h.advance,
                            cluster: u32::try_from(at).unwrap_or(u32::MAX),
                        },
                    );
                    pen += h.advance;
                }
                Item::Cluster(k) => {
                    let c = &self.cls[k];
                    let w = self.adv[k] + extra[k - start];
                    let style = spec.runs.get(c.span);
                    let color = style.map_or(crate::spec::BLACK, |r| r.style.color);
                    let shift = style.map_or(0.0, |r| r.style.baseline_shift);
                    boxes.push(ClusterBox {
                        text_range: c.range.clone(),
                        x: pen,
                        advance: w,
                        level,
                    });
                    match c.kind {
                        Kind::Tab => {
                            self.leader(
                                &mut runs,
                                &mut decorations,
                                k,
                                pen,
                                pen + w,
                                baseline - shift,
                                color,
                                level,
                            );
                        }
                        Kind::LineBreak | Kind::SoftHyphen => {}
                        Kind::Normal | Kind::Space => {
                            let mut gx = pen;
                            let cluster = u32::try_from(c.range.start).unwrap_or(u32::MAX);
                            for g in &self.glyphs[c.glyphs.clone()] {
                                push_glyph(
                                    &mut runs,
                                    (c.font, c.size, c.span, color, level),
                                    c.range.clone(),
                                    PositionedGlyph {
                                        id: g.id,
                                        x: gx + g.x,
                                        y: baseline + g.y - shift,
                                        advance: g.advance,
                                        cluster,
                                    },
                                );
                                gx += g.advance;
                            }
                        }
                    }
                    if decorated && k < content_end {
                        placed.push((k, pen, pen + w));
                    }
                    pen += w;
                }
            }
        }
        self.decorate(spec, &placed, baseline, &mut decorations);

        let line_x = if rtl {
            frame.width - offset - content_w
        } else {
            offset
        };
        let text_range = if start < end {
            self.cls[start].range.start..self.cls[end - 1].range.end
        } else {
            text.len()..text.len()
        };
        Line {
            text_range,
            top,
            height,
            baseline,
            ascent,
            descent,
            x: line_x,
            width: content_w,
            end: fit.end_kind,
            runs,
            decorations,
            clusters: boxes,
        }
    }

    /// Leader glyphs (or a rule) across a tab.
    #[allow(clippy::too_many_arguments)]
    fn leader(
        &self,
        runs: &mut Vec<GlyphRun>,
        decorations: &mut Vec<Decoration>,
        k: usize,
        x0: f32,
        x1: f32,
        baseline: f32,
        color: u32,
        level: u8,
    ) {
        let c = &self.cls[k];
        let ch = match self.leaders[k] {
            TabLeader::None => return,
            TabLeader::Dot => '.',
            TabLeader::Hyphen => '-',
            TabLeader::Underscore => '_',
            TabLeader::MiddleDot => '\u{b7}',
            TabLeader::Heavy => {
                let m = self.fonts.unit_metrics(c.font).map(|u| u.scale(c.size));
                let (off, thick) = m.map_or((c.size * 0.1, c.size * 0.08), |m| {
                    (m.underline_offset, m.underline_thickness * 2.0)
                });
                if x1 - x0 > EPS {
                    decorations.push(Decoration {
                        kind: DecorationKind::LeaderRule,
                        x0,
                        x1,
                        y: baseline + off,
                        thickness: thick,
                        color,
                    });
                }
                return;
            }
        };
        let Some((glyph, adv)) = self.fonts.glyph_for_char(c.font, ch, c.size) else {
            return;
        };
        if adv <= 0.1 {
            return;
        }
        // Leader glyphs sit on a grid so that dots of consecutive lines align, as in Word.
        let pad = adv * 0.5;
        let gx0 = ((x0 + pad) / adv).ceil() * adv;
        let room = x1 - pad * 0.5 + EPS - gx0 - adv;
        if !room.is_finite() || room < 0.0 {
            return;
        }
        // Positions are computed from an integer index (never by repeated addition, which stalls
        // once `adv` is below one ulp of `gx`), and the count is bounded.
        let fits = (room / adv).floor();
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "finite, non-negative and clamped to MAX_LEADER_GLYPHS before the cast"
        )]
        let count = (fits.min(MAX_LEADER_GLYPHS as f32 - 1.0) as usize) + 1;
        let at = c.range.start;
        for i in 0..count {
            #[expect(
                clippy::cast_precision_loss,
                reason = "i < MAX_LEADER_GLYPHS, exactly representable"
            )]
            let gx = gx0 + i as f32 * adv;
            push_glyph(
                runs,
                (c.font, c.size, c.span, color, level),
                at..c.range.end,
                PositionedGlyph {
                    id: glyph,
                    x: gx,
                    y: baseline,
                    advance: adv,
                    cluster: u32::try_from(at).unwrap_or(u32::MAX),
                },
            );
        }
    }

    /// Underlines and strikethroughs over the placed content clusters.
    fn decorate(
        &mut self,
        spec: &ParagraphSpec,
        placed: &[(usize, f32, f32)],
        baseline: f32,
        out: &mut Vec<Decoration>,
    ) {
        // Group visually adjacent clusters of the same span.
        let mut i = 0;
        while i < placed.len() {
            let (k0, x0, _) = placed[i];
            let span = self.cls[k0].span;
            let Some(run) = spec.runs.get(span) else {
                i += 1;
                continue;
            };
            let style = &run.style;
            if style.underline == UnderlineKind::None && style.strike == StrikeKind::None {
                i += 1;
                continue;
            }
            let mut j = i;
            let mut x1 = placed[i].2;
            while j + 1 < placed.len()
                && self.cls[placed[j + 1].0].span == span
                && (placed[j + 1].1 - x1).abs() < 0.5
            {
                j += 1;
                x1 = placed[j].2;
            }
            let m = self.fonts.metrics(self.cls[k0].font, self.cls[k0].size);
            let base = baseline - style.baseline_shift;
            let segments: Vec<(f32, f32)> = if style.underline == UnderlineKind::Words {
                let mut segs = Vec::new();
                let mut cur: Option<(f32, f32)> = None;
                for &(k, a, b) in &placed[i..=j] {
                    if matches!(self.cls[k].kind, Kind::Space | Kind::Tab) {
                        if let Some(s) = cur.take() {
                            segs.push(s);
                        }
                    } else {
                        cur = Some(cur.map_or((a, b), |(s, _)| (s, b)));
                    }
                }
                segs.extend(cur);
                segs
            } else {
                vec![(x0, x1)]
            };
            if style.underline != UnderlineKind::None {
                let thick = if style.underline == UnderlineKind::Thick {
                    m.underline_thickness * 2.0
                } else {
                    m.underline_thickness
                };
                for &(a, b) in &segments {
                    out.push(Decoration {
                        kind: DecorationKind::Underline(style.underline),
                        x0: a,
                        x1: b,
                        y: base + m.underline_offset,
                        thickness: thick,
                        color: style.color,
                    });
                    if style.underline == UnderlineKind::Double {
                        out.push(Decoration {
                            kind: DecorationKind::Underline(style.underline),
                            x0: a,
                            x1: b,
                            y: base + m.underline_offset + thick * 2.0,
                            thickness: thick,
                            color: style.color,
                        });
                    }
                }
            }
            if style.strike != StrikeKind::None {
                let thick = m.strikeout_thickness;
                let y = base - m.strikeout_offset;
                out.push(Decoration {
                    kind: DecorationKind::Strike(style.strike),
                    x0,
                    x1,
                    y,
                    thickness: thick,
                    color: style.color,
                });
                if style.strike == StrikeKind::Double {
                    out.push(Decoration {
                        kind: DecorationKind::Strike(style.strike),
                        x0,
                        x1,
                        y: y - thick * 2.0,
                        thickness: thick,
                        color: style.color,
                    });
                }
            }
            i = j + 1;
        }
    }

    /// The line of an empty paragraph (or after a final manual break), sized by the mark's font.
    fn empty_line(
        &mut self,
        spec: &ParagraphSpec,
        families: &[String],
        frame: Frame,
        rtl: bool,
        top: f32,
        at: usize,
    ) -> Line {
        let props = &spec.props;
        let default_style = SpanStyle::default();
        let style = props
            .mark_style
            .as_ref()
            .or_else(|| spec.runs.last().map(|r| &r.style))
            .unwrap_or(&default_style);
        let family = if props.mark_style.is_none() && !spec.runs.is_empty() {
            families.last().cloned().unwrap_or_default()
        } else {
            self.fonts.resolve(&style.family).family
        };
        let size = sanitize_size(style.size);
        let (ascent, descent, gap) = self
            .fonts
            .primary_font(&family, style.weight, style.italic)
            .map_or((size * 0.8, size * 0.2, 0.0), |id| {
                let m = self.fonts.metrics(id, size);
                (m.ascent, m.descent, m.line_gap)
            });
        let (height, baseline) = line_box(props.line_spacing, ascent, descent, gap, top);
        let start_x = frame.indent_start + if at == 0 { frame.first_line } else { 0.0 };
        let avail = frame.limit - start_x;
        let offset = match props.alignment {
            Alignment::Center => start_x + avail.max(0.0) * 0.5,
            Alignment::End => frame.limit,
            Alignment::Left if rtl => frame.limit,
            Alignment::Right if !rtl => frame.limit,
            _ => start_x,
        };
        let x = if rtl { frame.width - offset } else { offset };
        Line {
            text_range: at..at,
            top,
            height,
            baseline,
            ascent,
            descent,
            x,
            width: 0.0,
            end: LineEnd::Paragraph,
            runs: Vec::new(),
            decorations: Vec::new(),
            clusters: Vec::new(),
        }
    }
}

/// Height and baseline of a line box from its metrics and the spacing rule.
///
/// Word adds the extra space of "multiple" spacing above the text, and anchors "exactly" and "at
/// least" lines at the bottom (descent at the bottom of the box).
fn line_box(spacing: LineSpacing, ascent: f32, descent: f32, gap: f32, top: f32) -> (f32, f32) {
    let natural = ascent + descent + gap;
    let height = match spacing {
        LineSpacing::Multiple(m) if m.is_finite() && m > 0.0 => natural * m,
        LineSpacing::Exactly(v) if v.is_finite() && v > 0.0 => v,
        LineSpacing::AtLeast(v) if v.is_finite() => natural.max(v),
        _ => natural,
    };
    let baseline = top + height - descent - gap;
    (height, baseline)
}

/// Appends a glyph to the last run if it shares the run's style, else starts a new run.
fn push_glyph(
    runs: &mut Vec<GlyphRun>,
    key: (FontId, f32, usize, u32, u8),
    range: Range<usize>,
    glyph: PositionedGlyph,
) {
    let (font, size, span, color, level) = key;
    if let Some(last) = runs.last_mut()
        && last.font == font
        && last.size.to_bits() == size.to_bits()
        && last.span == span
        && last.color == color
        && last.level == level
    {
        last.text_range.start = last.text_range.start.min(range.start);
        last.text_range.end = last.text_range.end.max(range.end);
        last.glyphs.push(glyph);
        return;
    }
    runs.push(GlyphRun {
        font,
        size,
        span,
        color,
        level,
        text_range: range,
        glyphs: vec![glyph],
    });
}

/// UAX #9 rule L2: indices of `levels` in visual order.
pub(crate) fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let (Some(&max), Some(&min)) = (levels.iter().max(), levels.iter().min()) else {
        return order;
    };
    if max == 0 {
        return order;
    }
    let lowest_odd = if min % 2 == 1 { min } else { min + 1 };
    let mut level = max;
    while level >= lowest_odd && level > 0 {
        let mut i = 0;
        while i < order.len() {
            if levels[order[i]] >= level {
                let s = i;
                while i < order.len() && levels[order[i]] >= level {
                    i += 1;
                }
                order[s..i].reverse();
            } else {
                i += 1;
            }
        }
        level -= 1;
    }
    order
}

fn classify(ch: char) -> Kind {
    match ch {
        '\t' => Kind::Tab,
        '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}' => Kind::LineBreak,
        '\u{ad}' => Kind::SoftHyphen,
        ' ' | '\u{3000}' | '\u{2000}'..='\u{2006}' | '\u{2008}'..='\u{200a}' | '\u{205f}' => {
            Kind::Space
        }
        _ => Kind::Normal,
    }
}

/// Packs a cluster's line-breaking class into a byte for the shaping cache.
fn encode_class(first: char) -> u8 {
    let kind = match classify(first) {
        Kind::Normal => 0,
        Kind::Space => 1,
        Kind::Tab => 2,
        Kind::LineBreak => 3,
        Kind::SoftHyphen => 4,
    };
    kind | u8::from(is_cjk(first)) << 4 | u8::from(is_joining(first)) << 5
}

fn decode_class(class: u8) -> (Kind, bool, bool) {
    let kind = match class & 0x0f {
        1 => Kind::Space,
        2 => Kind::Tab,
        3 => Kind::LineBreak,
        4 => Kind::SoftHyphen,
        _ => Kind::Normal,
    };
    (kind, class & 0x10 != 0, class & 0x20 != 0)
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{2e80}'..='\u{2fdf}'
        | '\u{3000}'..='\u{30ff}'
        | '\u{3100}'..='\u{31ff}'
        | '\u{3400}'..='\u{4dbf}'
        | '\u{4e00}'..='\u{9fff}'
        | '\u{ac00}'..='\u{d7af}'
        | '\u{f900}'..='\u{faff}'
        | '\u{ff00}'..='\u{ffef}'
        | '\u{20000}'..='\u{3134f}')
}

fn is_joining(ch: char) -> bool {
    matches!(ch,
        '\u{0600}'..='\u{08ff}'
        | '\u{fb50}'..='\u{fdff}'
        | '\u{fe70}'..='\u{feff}'
        | '\u{1800}'..='\u{18af}')
}

/// Clamps a font size to something layout can use.
pub(crate) fn sanitize_size(size: f32) -> f32 {
    if size.is_finite() {
        size.clamp(0.5, 4000.0)
    } else {
        14.0
    }
}

/// Clamps letter spacing (px per character). Expansion is limited to 1584 pt (2112 px), Word's
/// maximum. Condensing is limited to a quarter of the font size so that a paragraph cannot collapse onto
/// a single point (a deliberate bound below Word's own 1584 pt, where glyphs pile up unreadably).
pub(crate) fn sanitize_letter_spacing(style: &SpanStyle) -> f32 {
    if style.letter_spacing.is_finite() {
        style
            .letter_spacing
            .clamp(-0.25 * sanitize_size(style.size), 2112.0)
    } else {
        0.0
    }
}

/// parley style for a span.
fn text_style<'a>(style: &SpanStyle, family: &'a str) -> TextStyle<'a, 'static, ()> {
    let features: &'static [FontFeature] = match (style.kerning, style.ligatures) {
        (true, true) => &[],
        (false, true) => NO_KERN,
        (true, false) => NO_LIGA,
        (false, false) => NO_KERN_LIGA,
    };
    TextStyle {
        font_family: FontFamily::Single(FontFamilyName::Named(Cow::Borrowed(family))),
        font_size: sanitize_size(style.size),
        font_style: if style.italic {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
        font_weight: FontWeight::new(f32::from(style.weight.clamp(1, 1000))),
        font_features: features.into(),
        locale: style.lang.as_deref().and_then(|l| Language::parse(l).ok()),
        letter_spacing: sanitize_letter_spacing(style),
        ..TextStyle::default()
    }
}

/// Whether the text needs the bidi algorithm: it contains right-to-left characters or explicit
/// directional formatting characters.
fn needs_bidi(text: &str) -> bool {
    if text.is_ascii() {
        return false;
    }
    text.chars().any(|c| {
        matches!(c,
            '\u{0590}'..='\u{08ff}'
            | '\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
            | '\u{fb1d}'..='\u{fdff}'
            | '\u{fe70}'..='\u{fefe}'
            | '\u{10800}'..='\u{10fff}'
            | '\u{1e800}'..='\u{1efff}')
    })
}

/// A shaping segment of the paragraph.
#[derive(Debug, Clone)]
struct Seg {
    range: Range<usize>,
    span: usize,
    rtl: bool,
    line_break: bool,
}

/// Character classes that decide segment boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegClass {
    Space,
    Tab,
    Break,
    Cjk,
    Other,
    /// Combining marks and joiners: never start a segment.
    Extend,
}

fn seg_class(ch: char) -> SegClass {
    if ch.is_ascii() {
        return match ch {
            ' ' => SegClass::Space,
            '\t' => SegClass::Tab,
            '\n' | '\r' | '\u{b}' | '\u{c}' => SegClass::Break,
            _ => SegClass::Other,
        };
    }
    match classify(ch) {
        Kind::Tab => SegClass::Tab,
        Kind::LineBreak => SegClass::Break,
        Kind::Space => SegClass::Space,
        _ if is_cjk(ch) => SegClass::Cjk,
        _ if matches!(bidi_class(ch), BidiClass::NSM | BidiClass::BN) => SegClass::Extend,
        _ => SegClass::Other,
    }
}

/// The resolved level of a character, with Word's run-level `rtl` applied to neutrals.
fn char_level(levels: &[Level], base: u8, pos: usize, ch: char, rtl_run: bool) -> u8 {
    let level = levels.get(pos).map_or(base, |l| l.number());
    if !rtl_run {
        return level;
    }
    if rtl_run
        && level.is_multiple_of(2)
        && matches!(
            bidi_class(ch),
            BidiClass::WS | BidiClass::ON | BidiClass::CS | BidiClass::ES | BidiClass::ET
        )
    {
        level + 1
    } else {
        level
    }
}

/// Cuts the paragraph into segments that shape independently: words with their trailing spaces,
/// single CJK characters, tabs, manual breaks, and never across a style or direction change.
fn segments(spec: &ParagraphSpec, levels: &[Level], base: u8) -> Vec<Seg> {
    let mut segs = Vec::new();
    let mut span_start = 0;
    for (span, run) in spec.runs.iter().enumerate() {
        let range = span_start..span_start + run.text.len();
        span_start = range.end;
        if range.is_empty() {
            continue;
        }
        let rtl_run = run.style.rtl;
        let mut start = range.start;
        let mut prev: Option<(char, bool, SegClass)> = None;
        let mut start_class = SegClass::Other;
        for (i, ch) in run.text.char_indices() {
            let pos = range.start + i;
            let class = seg_class(ch);
            if class == SegClass::Extend {
                continue;
            }
            let odd = char_level(levels, base, pos, ch, rtl_run) % 2 == 1;
            if let Some((pch, podd, pclass)) = prev {
                let crlf = pch == '\r' && ch == '\n';
                let split = !crlf
                    && (odd != podd
                        || matches!(pclass, SegClass::Tab | SegClass::Break)
                        || matches!(class, SegClass::Tab | SegClass::Break)
                        || (pclass == SegClass::Space && class != SegClass::Space)
                        || pclass == SegClass::Cjk
                        || class == SegClass::Cjk);
                if split {
                    segs.push(Seg {
                        range: start..pos,
                        span,
                        rtl: podd,
                        line_break: start_class == SegClass::Break,
                    });
                    start = pos;
                    start_class = class;
                }
            } else {
                start_class = class;
            }
            prev = Some((ch, odd, class));
        }
        let odd = prev.map_or(base % 2 == 1, |p| p.1);
        segs.push(Seg {
            range: start..range.end,
            span,
            rtl: odd,
            line_break: start_class == SegClass::Break,
        });
    }
    segs
}

/// The shaping-relevant part of a span's style.
fn style_key(style: &SpanStyle, family: &str) -> StyleKey {
    StyleKey {
        family: family.to_owned(),
        size: sanitize_size(style.size).to_bits(),
        weight: style.weight,
        italic: style.italic,
        letter_spacing: sanitize_letter_spacing(style).to_bits(),
        kerning: style.kerning,
        ligatures: style.ligatures,
        lang: style.lang.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn l2_reverses_nested_levels() {
        // "abc 123 XYZ" style levels: LTR 0, RTL run with embedded numbers at 2.
        assert_eq!(
            visual_order(&[0, 0, 1, 1, 2, 2, 1]),
            vec![0, 1, 6, 4, 5, 3, 2]
        );
        assert_eq!(visual_order(&[1, 1, 1]), vec![2, 1, 0]);
        assert_eq!(visual_order(&[0, 0]), vec![0, 1]);
        assert!(visual_order(&[]).is_empty());
    }

    #[test]
    fn line_spacing_rules() {
        let (h, b) = line_box(LineSpacing::Multiple(1.0), 10.0, 4.0, 0.0, 0.0);
        assert!((h - 14.0).abs() < 1e-4 && (b - 10.0).abs() < 1e-4);
        let (h, b) = line_box(LineSpacing::Multiple(2.0), 10.0, 4.0, 0.0, 0.0);
        assert!((h - 28.0).abs() < 1e-4 && (b - 24.0).abs() < 1e-4);
        let (h, _) = line_box(LineSpacing::Exactly(9.0), 10.0, 4.0, 0.0, 0.0);
        assert!((h - 9.0).abs() < 1e-4);
        let (h, _) = line_box(LineSpacing::AtLeast(9.0), 10.0, 4.0, 0.0, 0.0);
        assert!((h - 14.0).abs() < 1e-4);
        let (h, _) = line_box(LineSpacing::Multiple(f32::NAN), 10.0, 4.0, 0.0, 0.0);
        assert!((h - 14.0).abs() < 1e-4);
    }

    #[test]
    fn classification() {
        assert_eq!(classify('\t'), Kind::Tab);
        assert_eq!(classify('\n'), Kind::LineBreak);
        assert_eq!(classify('\u{a0}'), Kind::Normal);
        assert_eq!(classify(' '), Kind::Space);
        assert!(is_cjk('漢') && !is_cjk('a'));
        assert!(is_joining('ب'));
    }
}
