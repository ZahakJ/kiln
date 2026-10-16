// SPDX-License-Identifier: AGPL-3.0-or-later
//! Layout and painting of the scratch document: paragraphs laid out by `kiln-text` and stacked
//! on Letter-sized pages (Word's Normal style: Calibri 11 pt, 1.08 line spacing, 8 pt after,
//! 1-inch margins), one Scene tile per page, plus caret, selection and composition geometry and
//! hit testing.
//!
//! Document coordinates are CSS pixels at zoom 1. Pages are stacked vertically at a fixed x; a
//! surface wider than the page centres it by scrolling to a negative x.

use std::hash::{Hash, Hasher};
use std::ops::Range;

use kiln_common::NodeId;
use kiln_scene::{
    Bounds, ColorRole, DrawOp, HandleSide, HitEntry, HitIndex, LineStyle, Paint, Palette, Scene,
    Shadow, ShadowStyle, Tile, TileId, TileSlot,
};
use kiln_text::{
    Affinity, LineEnd, LineSpacing, ParagraphLayout, ParagraphSpec, SpanStyle, TextEngine,
};
use rustc_hash::{FxHashMap, FxHasher};

use crate::frame::{FrameRequest, GlyphCache};

/// Page width (8.5 in).
pub const PAGE_W: f32 = 816.0;
/// Page height (11 in).
pub const PAGE_H: f32 = 1056.0;
/// Margins (1 in).
pub const MARGIN: f32 = 96.0;
/// Width of the text column.
pub const TEXT_W: f32 = PAGE_W - 2.0 * MARGIN;
/// Vertical gap between pages.
pub const PAGE_GAP: f32 = 32.0;
/// Space above the first page.
pub const TOP_PAD: f32 = 32.0;
/// Space beside the pages (holds the page shadow).
pub const SIDE_PAD: f32 = 64.0;
/// Space below the last page (holds the page shadow).
pub const BOTTOM_PAD: f32 = 96.0;
/// Body text size: 11 pt.
pub const FONT_SIZE: f32 = 11.0 * 4.0 / 3.0;
/// Word's default body font.
pub const FONT_FAMILY: &str = "Calibri";
/// Caret width.
pub const CARET_W: f32 = 2.0;
/// Visual diameter of a touch handle (its hit target is 44 × 44 dp in the surface).
pub const HANDLE_SIZE: f32 = 22.0;

/// Where a line sits: its paragraph origin (absolute document y of the paragraph's top for this
/// line, which moves when the paragraph breaks across pages) and its page.
#[derive(Debug, Clone, Copy, PartialEq)]
struct LinePlace {
    page: usize,
    origin_y: f32,
}

/// One laid-out paragraph.
#[derive(Debug, Clone)]
struct ParaBox {
    id: NodeId,
    text_hash: u64,
    layout: ParagraphLayout,
    lines: Vec<LinePlace>,
}

/// A position for geometry: paragraph index and byte offset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct At {
    /// Paragraph index.
    pub para: usize,
    /// Byte offset.
    pub byte: usize,
}

/// A caret in document coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaretGeom {
    /// The bar (x is the caret line; the bar is [`CARET_W`] wide, centred on it).
    pub bar: Bounds,
    /// Right-to-left context.
    pub rtl: bool,
}

/// Layout, pagination, glyph cache and scene of a scratch document.
pub struct DocLayout {
    engine: TextEngine,
    paras: Vec<ParaBox>,
    pages: usize,
    page_versions: Vec<u64>,
    /// Every line, sorted by top: (top, bottom, paragraph, line).
    line_list: Vec<(f32, f32, usize, usize)>,
    glyphs: GlyphCache,
    scene: Scene,
    viewport: Option<FrameRequest>,
}

impl std::fmt::Debug for DocLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocLayout")
            .field("paragraphs", &self.paras.len())
            .field("pages", &self.pages)
            .field("glyphs", &self.glyphs)
            .finish_non_exhaustive()
    }
}

fn hash_text(text: &str) -> u64 {
    let mut h = FxHasher::default();
    text.hash(&mut h);
    h.finish()
}

fn spec(text: &str) -> ParagraphSpec {
    let mut s = ParagraphSpec::plain(text, SpanStyle::new(FONT_FAMILY, FONT_SIZE), TEXT_W);
    s.props.line_spacing = LineSpacing::Multiple(1.08);
    s.props.space_after = 8.0 * 4.0 / 3.0;
    s
}

/// The page shadow's reach beyond the page in both themes: (left, top, right, bottom).
fn shadow_reach() -> (f32, f32, f32, f32) {
    let a = Palette::light().page_shadow_reach();
    let b = Palette::dark().page_shadow_reach();
    (a.0.max(b.0), a.1.max(b.1), a.2.max(b.2), a.3.max(b.3))
}

/// Top of page `i`.
#[allow(clippy::cast_precision_loss)]
fn page_top(i: usize) -> f32 {
    TOP_PAD + i as f32 * (PAGE_H + PAGE_GAP)
}

/// The rectangle of page `i`.
fn page_rect(i: usize) -> Bounds {
    Bounds::new(SIDE_PAD, page_top(i), PAGE_W, PAGE_H)
}

/// x of every paragraph's origin.
const TEXT_X: f32 = SIDE_PAD + MARGIN;

fn ink(color: u32) -> Paint {
    if color == kiln_text::BLACK {
        Paint::Role(ColorRole::Ink)
    } else {
        Paint::Rgba(color)
    }
}

impl DocLayout {
    /// Lays out `paras` (id and text).
    #[must_use]
    pub fn new<'a>(paras: impl IntoIterator<Item = (NodeId, &'a str)>) -> Self {
        let mut l = Self {
            engine: TextEngine::new(super::fonts::store()),
            paras: Vec::new(),
            pages: 1,
            page_versions: Vec::new(),
            line_list: Vec::new(),
            glyphs: GlyphCache::new(),
            scene: Scene::new(1.0, Bounds::ZERO, Paint::Role(ColorRole::Canvas)),
            viewport: None,
        };
        l.update(paras);
        l
    }

    /// Re-lays out paragraphs whose text changed (matched by id) and repaginates.
    pub fn update<'a>(&mut self, paras: impl IntoIterator<Item = (NodeId, &'a str)>) {
        let mut old: FxHashMap<NodeId, ParaBox> = self.paras.drain(..).map(|p| (p.id, p)).collect();
        for (id, text) in paras {
            let h = hash_text(text);
            let reuse = old.remove(&id).filter(|p| p.text_hash == h);
            let pb = reuse.unwrap_or_else(|| ParaBox {
                id,
                text_hash: h,
                layout: self.engine.layout(&spec(text)),
                lines: Vec::new(),
            });
            self.paras.push(pb);
        }
        self.paginate();
    }

    fn paginate(&mut self) {
        let content_bottom = |page: usize| page_top(page) + PAGE_H - MARGIN;
        let mut page = 0usize;
        let mut y = page_top(0) + MARGIN;
        let mut line_list = Vec::new();
        let mut hashers: Vec<FxHasher> = vec![FxHasher::default()];
        let mut page_has_lines = false;
        for (pi, p) in self.paras.iter_mut().enumerate() {
            p.lines.clear();
            let mut origin = y;
            for (li, line) in p.layout.lines.iter().enumerate() {
                if origin + line.top + line.height > content_bottom(page) + 0.01 && page_has_lines {
                    page += 1;
                    hashers.push(FxHasher::default());
                    origin = page_top(page) + MARGIN - line.top;
                }
                let top = origin + line.top;
                p.lines.push(LinePlace {
                    page,
                    origin_y: origin,
                });
                line_list.push((top, top + line.height, pi, li));
                page_has_lines = true;
                let h = &mut hashers[page];
                p.id.hash(h);
                p.text_hash.hash(h);
                li.hash(h);
                top.to_bits().hash(h);
            }
            y = origin + p.layout.height;
        }
        self.pages = page + 1;
        self.page_versions = hashers.into_iter().map(|h| h.finish()).collect();
        self.line_list = line_list;
    }

    /// The document surface.
    #[must_use]
    pub fn extent(&self) -> Bounds {
        let bottom = page_top(self.pages.saturating_sub(1)) + PAGE_H + BOTTOM_PAD;
        Bounds::new(0.0, 0.0, PAGE_W + 2.0 * SIDE_PAD, bottom)
    }

    /// The scene of the last [`DocLayout::prepare`].
    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Glyph atlas uploads.
    pub fn atlas_updates(&mut self, all: bool) -> Vec<kiln_scene::AtlasUpdate> {
        self.glyphs.updates(all)
    }

    /// The last frame request (viewport), if any.
    #[must_use]
    pub fn request(&self) -> Option<&FrameRequest> {
        self.viewport.as_ref()
    }

    /// Updates the scene for a frame: tile slots for every page, tiles for pages in the send
    /// region, and the overlay.
    pub fn prepare(&mut self, req: &FrameRequest, overlay: Vec<DrawOp>) {
        self.viewport = Some(*req);
        let rescaled = self.glyphs.set_scale(req.scale);
        if rescaled || self.scene.scale.to_bits() != req.scale.to_bits() {
            self.scene = Scene::new(req.scale, self.extent(), Paint::Role(ColorRole::Canvas));
        }
        self.scene.extent = self.extent();
        let (l, t, r, b) = shadow_reach();
        let scale_bits = u64::from(req.scale.to_bits());
        let slots: Vec<TileSlot> = (0..self.pages)
            .map(|i| {
                let page = page_rect(i);
                TileSlot {
                    id: TileId::content(u32::try_from(i).unwrap_or(u32::MAX)),
                    rect: Bounds::new(page.x - l, page.y - t, page.w + l + r, page.h + t + b),
                    version: self.page_versions.get(i).copied().unwrap_or(0)
                        ^ scale_bits.rotate_left(17),
                }
            })
            .collect();
        self.scene.set_slots(slots);
        let wanted: Vec<TileSlot> = self.scene.slots_in(&req.send).copied().collect();
        for slot in wanted {
            if !self.scene.is_built(slot.id) {
                let (tile, hits) = self.build_page(slot);
                self.scene.insert_built(slot.version, tile, hits);
            }
        }
        self.scene.evict_outside(&req.keep);
        self.scene.overlay = overlay;
    }

    fn build_page(&mut self, slot: TileSlot) -> (Tile, HitIndex) {
        let page = slot.id.index as usize;
        let s = self.glyphs.scale();
        let tile_dev = ((slot.rect.x * s).round(), (slot.rect.y * s).round());
        let local = |b: Bounds| b.translate(-slot.rect.x, -slot.rect.y);
        let mut ops = vec![
            DrawOp::Shadow(Shadow {
                rect: local(page_rect(page)),
                radius: 0.0,
                style: ShadowStyle::Page,
            }),
            DrawOp::FillRect {
                rect: local(page_rect(page)),
                paint: Paint::Role(ColorRole::Page),
                radius: 0.0,
            },
        ];
        let mut hits = Vec::new();
        let start = self.line_list.partition_point(|l| l.1 <= page_top(page));
        for &(top, bottom, pi, li) in &self.line_list[start..] {
            let place = self.paras[pi].lines[li];
            if place.page != page {
                if place.page > page {
                    break;
                }
                continue;
            }
            hits.push(HitEntry {
                rect: Bounds::new(SIDE_PAD, top, PAGE_W, bottom - top),
                key: (u64::try_from(pi).unwrap_or(u64::MAX) << 24) | u64::try_from(li).unwrap_or(0),
            });
            let line = &self.paras[pi].layout.lines[li];
            let origin = (TEXT_X * s - tile_dev.0, place.origin_y * s - tile_dev.1);
            for run in &line.runs {
                self.glyphs
                    .push_run(self.engine.fonts(), run, origin, ink(run.color), &mut ops);
            }
            for d in &line.decorations {
                ops.push(DrawOp::FillRect {
                    rect: local(Bounds::new(
                        TEXT_X + d.x0,
                        place.origin_y + d.y,
                        d.x1 - d.x0,
                        d.thickness,
                    )),
                    paint: ink(d.color),
                    radius: 0.0,
                });
            }
        }
        (Tile::new(slot.id, slot.rect, ops), HitIndex::new(hits))
    }

    // ----- geometry ------------------------------------------------------------------------

    fn line_at(&self, at: At, affinity: Affinity) -> (usize, LinePlace) {
        let p = &self.paras[at.para];
        let li = p.layout.line_of(at.byte, affinity);
        let place = p.lines.get(li).copied().unwrap_or(LinePlace {
            page: 0,
            origin_y: page_top(0) + MARGIN,
        });
        (li, place)
    }

    /// The caret at a position.
    #[must_use]
    pub fn caret(&self, at: At) -> CaretGeom {
        let Some(p) = self.paras.get(at.para) else {
            return CaretGeom {
                bar: Bounds::new(TEXT_X, page_top(0) + MARGIN, CARET_W, FONT_SIZE),
                rtl: false,
            };
        };
        let c = p.layout.caret_rect(at.byte);
        let place = p.lines.get(c.line).copied().unwrap_or(LinePlace {
            page: 0,
            origin_y: page_top(0) + MARGIN,
        });
        CaretGeom {
            bar: Bounds::new(
                TEXT_X + c.x - CARET_W / 2.0,
                place.origin_y + c.top,
                CARET_W,
                c.height,
            ),
            rtl: c.rtl,
        }
    }

    /// Highlight rectangles for the range `[start, end)` (document order), including the width of
    /// a paragraph mark for every paragraph break inside the range, as Word draws it.
    #[must_use]
    pub fn selection_rects(&self, start: At, end: At) -> Vec<Bounds> {
        let mut out = Vec::new();
        if start >= end {
            return out;
        }
        for pi in start.para..=end.para.min(self.paras.len().saturating_sub(1)) {
            let p = &self.paras[pi];
            let from = if pi == start.para { start.byte } else { 0 };
            let to = if pi == end.para {
                end.byte
            } else {
                p.layout.text().len()
            };
            for r in p.layout.selection_rects(from..to) {
                let li = p
                    .layout
                    .lines
                    .iter()
                    .position(|l| (l.top - r.y).abs() < 0.01)
                    .unwrap_or(0);
                let origin = p.lines.get(li).map_or(0.0, |l| l.origin_y);
                out.push(Bounds::new(TEXT_X + r.x, origin + r.y, r.width, r.height));
            }
            if pi < end.para {
                // The paragraph mark.
                let li = p.layout.lines.len().saturating_sub(1);
                if let (Some(line), Some(place)) = (p.layout.lines.get(li), p.lines.get(li)) {
                    let x = TEXT_X + line.x + line.width;
                    out.push(Bounds::new(
                        x,
                        place.origin_y + line.top,
                        FONT_SIZE * 0.4,
                        line.height,
                    ));
                }
            }
        }
        out
    }

    /// Underlines for an IME composition range inside one paragraph.
    #[must_use]
    pub fn composition_underlines(&self, para: usize, range: Range<usize>) -> Vec<DrawOp> {
        let Some(p) = self.paras.get(para) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for r in p.layout.selection_rects(range) {
            let Some(li) = p
                .layout
                .lines
                .iter()
                .position(|l| (l.top - r.y).abs() < 0.01)
            else {
                continue;
            };
            let line = &p.layout.lines[li];
            let origin = p.lines.get(li).map_or(0.0, |l| l.origin_y);
            out.push(DrawOp::Underline {
                x: TEXT_X + r.x,
                y: origin + line.baseline + (line.descent * 0.45).max(1.5),
                width: r.width,
                thickness: 1.0,
                paint: Paint::Role(ColorRole::Composition),
                style: LineStyle::Solid,
            });
        }
        out
    }

    /// Touch handles for a selection (or a caret).
    #[must_use]
    pub fn handles(&self, start: At, end: At) -> Vec<DrawOp> {
        let mk = |at: At, side: HandleSide| {
            let c = self.caret(at);
            DrawOp::Handle {
                x: c.bar.x + CARET_W / 2.0,
                y: c.bar.bottom(),
                size: HANDLE_SIZE,
                side,
                paint: Paint::Role(ColorRole::Caret),
            }
        };
        if start == end {
            vec![mk(end, HandleSide::Caret)]
        } else {
            vec![mk(start, HandleSide::Start), mk(end, HandleSide::End)]
        }
    }

    /// The position under a document point: the line under (or nearest) the point, then the
    /// character position nearest `x` on it.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<At> {
        if !(x.is_finite() && y.is_finite()) || self.paras.is_empty() {
            return None;
        }
        // The tile index answers clicks in painted pages; anything else (keyboard paging into
        // unbuilt pages, points between pages) uses the sorted line list.
        let (pi, li) = match self.scene.hit_test(x, y) {
            Some(hit) if y >= hit.entry.rect.y && y < hit.entry.rect.bottom() => (
                usize::try_from(hit.entry.key >> 24).ok()?,
                usize::try_from(hit.entry.key & 0xFF_FFFF).ok()?,
            ),
            _ => self.nearest_line(y)?,
        };
        let p = self.paras.get(pi)?;
        let line = p.layout.lines.get(li)?;
        let byte = p.layout.hit_test(x - TEXT_X, line.top + line.height / 2.0);
        Some(At { para: pi, byte })
    }

    fn nearest_line(&self, y: f32) -> Option<(usize, usize)> {
        let i = self.line_list.partition_point(|l| l.1 <= y);
        let pick = if i >= self.line_list.len() {
            self.line_list.last()
        } else {
            let cur = self.line_list[i];
            if y >= cur.0 || i == 0 {
                Some(&self.line_list[i])
            } else {
                let prev = self.line_list[i - 1];
                if y - prev.1 <= cur.0 - y {
                    Some(&self.line_list[i - 1])
                } else {
                    Some(&self.line_list[i])
                }
            }
        }?;
        Some((pick.2, pick.3))
    }

    /// The line start and end of the line holding `at` (End stops before a wrap's trailing
    /// spaces so the caret stays on the line).
    #[must_use]
    pub fn line_bounds(&self, at: At) -> (usize, usize) {
        let p = &self.paras[at.para];
        let (li, _) = self.line_at(at, Affinity::Downstream);
        let Some(line) = p.layout.lines.get(li) else {
            return (0, p.layout.text().len());
        };
        let r = line.text_range.clone();
        let end = if matches!(line.end, LineEnd::Paragraph) || li + 1 == p.layout.lines.len() {
            r.end
        } else {
            let text = &p.layout.text()[r.clone()];
            let trimmed = text.trim_end_matches([' ', '\n', '\u{2028}', '\u{b}']);
            p.layout.snap(r.start + trimmed.len())
        };
        (r.start, end)
    }

    /// The position one line above or below `at` at document x `goal_x` (`None` at the first or
    /// last line).
    #[must_use]
    pub fn vertical(&self, at: At, goal_x: f32, down: bool) -> Option<At> {
        let (li, _) = self.line_at(at, Affinity::Downstream);
        let idx = self
            .line_list
            .iter()
            .position(|l| l.2 == at.para && l.3 == li)?;
        let target = if down {
            self.line_list.get(idx + 1)?
        } else {
            self.line_list.get(idx.checked_sub(1)?)?
        };
        let (pi, tli) = (target.2, target.3);
        let p = &self.paras[pi];
        let line = &p.layout.lines[tli];
        let byte = p
            .layout
            .hit_test(goal_x - TEXT_X, line.top + line.height / 2.0);
        Some(At { para: pi, byte })
    }

    /// The position `dy` document pixels above (negative) or below the caret at x `goal_x`.
    #[must_use]
    pub fn page_move(&self, at: At, goal_x: f32, dy: f32) -> Option<At> {
        let c = self.caret(at);
        self.hit_test(goal_x, c.bar.y + c.bar.h / 2.0 + dy)
    }

    /// The word around `at` as a byte range.
    #[must_use]
    pub fn word_at(&self, at: At) -> Range<usize> {
        self.paras[at.para].layout.word_at(at.byte)
    }

    /// The bounds of a paragraph (first line top to last line bottom; across pages, the union).
    #[must_use]
    pub fn para_bounds(&self, para: usize) -> Option<Bounds> {
        let p = self.paras.get(para)?;
        let mut b = Bounds::ZERO;
        for (line, place) in p.layout.lines.iter().zip(&p.lines) {
            b = b.union(&Bounds::new(
                TEXT_X,
                place.origin_y + line.top,
                TEXT_W,
                line.height,
            ));
        }
        Some(b)
    }

    /// Indices of the paragraphs with a line in `region`.
    #[must_use]
    pub fn paras_in(&self, region: &Bounds) -> Vec<usize> {
        let start = self.line_list.partition_point(|l| l.1 <= region.y);
        let mut out: Vec<usize> = self.line_list[start..]
            .iter()
            .take_while(|l| l.0 < region.bottom())
            .map(|l| l.2)
            .collect();
        out.dedup();
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lorem(n: usize) -> Vec<(NodeId, String)> {
        (0..n)
            .map(|i| {
                (
                    NodeId(i as u64 + 1),
                    format!(
                        "Paragraph {i}: the kiln is warm and the glaze is ready. Twice a year we load the \
                         big catenary kiln together, and every shelf holds work that will last."
                    ),
                )
            })
            .collect()
    }

    fn layout(paras: &[(NodeId, String)]) -> DocLayout {
        DocLayout::new(paras.iter().map(|(id, t)| (*id, t.as_str())))
    }

    #[test]
    fn paginates_lines_onto_letter_pages() {
        let paras = lorem(120);
        let l = layout(&paras);
        assert!(l.pages >= 6, "{} pages", l.pages);
        for &(top, bottom, _, _) in &l.line_list {
            let page = l
                .line_list
                .iter()
                .find(|x| x.0 == top)
                .map(|x| l.paras[x.2].lines[x.3].page)
                .unwrap();
            assert!(
                top >= page_top(page) + MARGIN - 0.01,
                "line above the top margin"
            );
            assert!(
                bottom <= page_top(page) + PAGE_H - MARGIN + 0.01,
                "line below the bottom margin"
            );
        }
        assert!(
            l.line_list.windows(2).all(|w| w[0].0 < w[1].0),
            "lines sorted by top"
        );
        let e = l.extent();
        assert_eq!(e.w, PAGE_W + 2.0 * SIDE_PAD);
        assert!(e.h > page_top(l.pages - 1) + PAGE_H);
    }

    #[test]
    fn carets_hit_tests_and_vertical_motion_agree() {
        let paras = lorem(40);
        let l = layout(&paras);
        for at in [
            At { para: 0, byte: 0 },
            At { para: 3, byte: 17 },
            At { para: 39, byte: 40 },
        ] {
            let c = l.caret(at);
            let back = l
                .hit_test(c.bar.x + CARET_W / 2.0, c.bar.y + c.bar.h / 2.0)
                .unwrap();
            assert_eq!(back, at);
        }
        let at = At { para: 2, byte: 10 };
        let x = l.caret(at).bar.x;
        let down = l.vertical(at, x, true).unwrap();
        assert!(l.caret(down).bar.y > l.caret(at).bar.y);
        let up = l.vertical(down, x, false).unwrap();
        assert_eq!(up, at);
        assert_eq!(l.vertical(At { para: 0, byte: 0 }, x, false), None);
        // Clicks in the margins and between pages land on the nearest line.
        let first = l.hit_test(0.0, 0.0).unwrap();
        assert_eq!(first, At { para: 0, byte: 0 });
        let last = l.hit_test(5_000.0, 1.0e7).unwrap();
        assert_eq!(last.para, 39);
        assert!(l.hit_test(f32::NAN, 0.0).is_none());
    }

    #[test]
    fn selection_rects_cover_lines_and_paragraph_marks() {
        let paras = lorem(3);
        let l = layout(&paras);
        let rects = l.selection_rects(At { para: 0, byte: 5 }, At { para: 1, byte: 4 });
        assert!(rects.len() >= 3, "{rects:?}");
        assert!(rects.iter().all(|r| r.w > 0.0 && r.h > 10.0));
        assert!(
            l.selection_rects(At { para: 1, byte: 4 }, At { para: 1, byte: 4 })
                .is_empty()
        );
        let under = l.composition_underlines(0, 0..5);
        assert_eq!(under.len(), 1);
        assert_eq!(
            l.handles(At { para: 0, byte: 0 }, At { para: 0, byte: 0 })
                .len(),
            1
        );
        assert_eq!(
            l.handles(At { para: 0, byte: 0 }, At { para: 0, byte: 3 })
                .len(),
            2
        );
    }

    #[test]
    fn edits_relayout_only_changed_paragraphs_and_version_pages() {
        let mut paras = lorem(100);
        let mut l = layout(&paras);
        let before = l.page_versions.clone();
        let (shape_before, _, _) = l.engine.shape_cache_stats();
        paras[99].1.push_str(" More.");
        l.update(paras.iter().map(|(id, t)| (*id, t.as_str())));
        let last = l.pages - 1;
        assert_eq!(
            &l.page_versions[..last],
            &before[..last],
            "earlier pages keep their version"
        );
        assert_ne!(l.page_versions[last], before[last]);
        let (shape_after, _, _) = l.engine.shape_cache_stats();
        assert!(
            shape_after - shape_before < 5,
            "only the new word was shaped"
        );
    }

    #[test]
    fn line_bounds_stop_before_wrapping_spaces() {
        let paras = lorem(1);
        let l = layout(&paras);
        let (s, e) = l.line_bounds(At { para: 0, byte: 3 });
        assert_eq!(s, 0);
        let text = l.paras[0].layout.text();
        assert!(!text[..e].ends_with(' '), "{:?}", &text[..e]);
        assert!(e < text.len());
    }
}
