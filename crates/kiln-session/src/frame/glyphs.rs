// SPDX-License-Identifier: AGPL-3.0-or-later
//! Glyph runs from `kiln-text` as Scene ops, and the atlas pages the painter needs.

use std::collections::BTreeSet;

use kiln_scene::{AtlasFormat, AtlasPageId, AtlasUpdate, DrawOp, GlyphRun, Paint, PathData};
use kiln_text::{FontStore, GlyphAtlas, PageId, RunGlyphs};

/// The glyph atlas of one document view, with the pages changed since the last upload.
#[derive(Debug, Default)]
pub struct GlyphCache {
    atlas: GlyphAtlas,
    dirty: BTreeSet<PageId>,
    scale: f32,
}

impl GlyphCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Switches to another device scale. Glyphs rasterised for the old scale are useless (and
    /// the painter is reset by the scale change), so the atlas starts over. Returns whether the
    /// scale changed.
    pub fn set_scale(&mut self, scale: f32) -> bool {
        if self.scale.to_bits() == scale.to_bits() {
            return false;
        }
        self.scale = scale;
        self.atlas.clear();
        self.dirty.clear();
        true
    }

    /// The device scale glyphs are rasterised for.
    #[must_use]
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Number of atlas pages.
    #[must_use]
    pub fn pages(&self) -> usize {
        self.atlas.pages().len()
    }

    /// Appends the ops of one `kiln-text` glyph run. `origin` is the device position of the
    /// paragraph origin relative to the tile's device origin (fractional: sub-pixel positioning
    /// is preserved). Bitmap glyphs become one [`DrawOp::Glyphs`] per atlas page; glyphs at or
    /// above the outline threshold become [`DrawOp::OutlineGlyphs`] in tile-local document
    /// pixels. Glyphs the atlas cannot render are skipped.
    pub fn push_run(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        let scale = if self.scale > 0.0 { self.scale } else { 1.0 };
        let Ok(prepared) = self
            .atlas
            .prepare_run(fonts, run, origin.0, origin.1, scale)
        else {
            return;
        };
        self.dirty.extend(self.atlas.take_dirty());
        match prepared {
            RunGlyphs::Bitmaps(placed) => {
                let mut runs: Vec<GlyphRun> = Vec::new();
                for g in placed {
                    let page = AtlasPageId(g.glyph.page.0);
                    let idx = match runs.iter().position(|r| r.page == page) {
                        Some(i) => i,
                        None => {
                            runs.push(GlyphRun {
                                page,
                                paint,
                                quads: Vec::with_capacity(run.glyphs.len() * GlyphRun::STRIDE),
                            });
                            runs.len() - 1
                        }
                    };
                    runs[idx].push(
                        [
                            i32::from(g.glyph.x),
                            i32::from(g.glyph.y),
                            i32::from(g.glyph.width),
                            i32::from(g.glyph.height),
                        ],
                        [g.x, g.y],
                    );
                }
                out.extend(runs.into_iter().map(DrawOp::Glyphs));
            }
            RunGlyphs::Outlines(paths) => {
                let mut path = PathData::new();
                for p in &paths {
                    path.extend(&PathData::from(p));
                }
                if !path.is_empty() {
                    path.scale(1.0 / scale);
                    out.push(DrawOp::OutlineGlyphs { path, paint });
                }
            }
        }
    }

    /// Atlas pixels for the painter: every page with `all`, else the pages changed since the last
    /// call. Each update carries the page's used rows (shelves are allocated top-down).
    pub fn updates(&mut self, all: bool) -> Vec<AtlasUpdate> {
        self.dirty.extend(self.atlas.take_dirty());
        let ids: Vec<PageId> = if all {
            self.atlas.pages().iter().map(|p| p.id).collect()
        } else {
            self.dirty.iter().copied().collect()
        };
        self.dirty.clear();
        ids.into_iter()
            .filter_map(|id| self.atlas.page(id))
            .filter_map(|page| {
                let (format, bpp) = match page.format {
                    kiln_text::AtlasFormat::A8 => (AtlasFormat::A8, 1usize),
                    kiln_text::AtlasFormat::Rgba8 => (AtlasFormat::Rgba8, 4usize),
                };
                #[allow(
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss,
                    clippy::cast_precision_loss
                )]
                let rows =
                    ((page.fill_ratio() * page.height as f32).ceil() as u32).min(page.height);
                if rows == 0 {
                    return None;
                }
                let len = rows as usize * page.width as usize * bpp;
                let mut data = page.data.get(..len)?.to_vec();
                if format == AtlasFormat::Rgba8 {
                    unpremultiply(&mut data);
                }
                Some(AtlasUpdate {
                    page: AtlasPageId(page.id.0),
                    format,
                    width: page.width,
                    height: page.height,
                    y: 0,
                    rows,
                    data,
                })
            })
            .collect()
    }
}

/// Premultiplied to straight alpha (the wire format of colour pages).
fn unpremultiply(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        let a = u16::from(px[3]);
        if a == 0 || a == 255 {
            continue;
        }
        for c in &mut px[..3] {
            *c = u8::try_from((u16::from(*c) * 255 + a / 2) / a).unwrap_or(255);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_text::{ParagraphSpec, SpanStyle, TextEngine};

    #[test]
    fn runs_become_glyph_ops_and_pages_upload_once() {
        let mut engine = TextEngine::new(crate::apps::scratch::fonts::store());
        let layout = engine.layout(&ParagraphSpec::plain(
            "Kiln Write",
            SpanStyle::new("Calibri", 14.67),
            600.0,
        ));
        let mut cache = GlyphCache::new();
        assert!(cache.set_scale(2.0));
        assert!(!cache.set_scale(2.0));
        let mut ops = Vec::new();
        for run in &layout.lines[0].runs {
            cache.push_run(engine.fonts(), run, (10.25, 40.0), Paint::BLACK, &mut ops);
        }
        let DrawOp::Glyphs(run) = &ops[0] else {
            panic!("expected glyphs, got {ops:?}");
        };
        assert_eq!(run.len(), 9, "the space has no bitmap");
        let up = cache.updates(false);
        assert_eq!(up.len(), 1);
        assert!(up[0].is_valid() && up[0].rows > 0 && up[0].rows < 100);
        assert!(cache.updates(false).is_empty(), "nothing new");
        assert_eq!(cache.updates(true).len(), 1, "everything after a reset");
        // Huge text becomes outlines in document pixels.
        let big = engine.layout(&ParagraphSpec::plain(
            "K",
            SpanStyle::new("Calibri", 120.0),
            600.0,
        ));
        let mut ops = Vec::new();
        cache.push_run(
            engine.fonts(),
            &big.lines[0].runs[0],
            (0.0, 0.0),
            Paint::BLACK,
            &mut ops,
        );
        let DrawOp::OutlineGlyphs { path, .. } = &ops[0] else {
            panic!("expected outlines");
        };
        assert!(
            path.bounds().h > 50.0 && path.bounds().h < 120.0,
            "{:?}",
            path.bounds()
        );
        // A scale change starts over.
        assert!(cache.set_scale(1.0));
        assert_eq!(cache.pages(), 0);
    }

    #[test]
    fn colour_pages_are_sent_straight() {
        let mut px = [64u8, 32, 0, 128, 10, 20, 30, 0, 1, 2, 3, 255];
        unpremultiply(&mut px);
        assert_eq!(px, [128, 64, 0, 128, 10, 20, 30, 0, 1, 2, 3, 255]);
    }
}
