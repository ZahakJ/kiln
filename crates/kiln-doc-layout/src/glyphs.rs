// SPDX-License-Identifier: AGPL-3.0-or-later
//! Glyph runs from `kiln-text` as Scene ops, and the atlas pages the painter needs.
//!
//! The same conversion as the session's scratch surface (`kiln-session` `frame::GlyphCache`); it
//! lives here too so that pages render without the session (the CLI's golden PNGs, PDF export
//! previews, tests).

use std::collections::BTreeSet;

use kiln_scene::{AtlasFormat, AtlasPageId, AtlasUpdate, DrawOp, GlyphRun, Paint, PathData};
use kiln_text::{FontStore, GlyphAtlas, PageId, RunGlyphs};

/// The glyph atlas of one document view, with the pages changed since the last upload.
#[derive(Debug, Default)]
pub struct AtlasGlyphs {
    atlas: GlyphAtlas,
    dirty: BTreeSet<PageId>,
    scale: f32,
}

impl AtlasGlyphs {
    /// An empty atlas.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Switches to another device scale (the atlas starts over). Returns whether it changed.
    pub fn set_scale(&mut self, scale: f32) -> bool {
        if self.scale.to_bits() == scale.to_bits() {
            return false;
        }
        self.scale = scale;
        self.atlas.clear();
        self.dirty.clear();
        true
    }

    /// The device scale.
    #[must_use]
    pub fn scale(&self) -> f32 {
        if self.scale > 0.0 { self.scale } else { 1.0 }
    }

    /// Appends the ops of a glyph run whose origin is at `origin` device px from the tile's
    /// device origin.
    pub fn push_run(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        let scale = self.scale();
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
                    let idx = if let Some(i) = runs.iter().position(|r| r.page == page) {
                        i
                    } else {
                        runs.push(GlyphRun {
                            page,
                            paint,
                            quads: Vec::with_capacity(run.glyphs.len() * GlyphRun::STRIDE),
                        });
                        runs.len() - 1
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

    /// Atlas pixels for the painter: every page with `all`, else the pages changed since the
    /// last call (only the rows in use).
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
            #[allow(clippy::cast_possible_truncation)]
            let v = ((u16::from(*c) * 255 + a / 2) / a).min(255) as u8;
            *c = v;
        }
    }
}
