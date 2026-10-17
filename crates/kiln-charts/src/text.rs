// SPDX-License-Identifier: AGPL-3.0-or-later
//! Chart text through `kiln-text`: measuring labels for layout (titles wrap, tick labels are
//! measured for collision avoidance) and emitting their glyphs into Scene ops.
//!
//! Glyphs go through a [`GlyphSink`]: [`OutlineSink`] emits vector outlines (any rotation, any
//! scale, no atlas: thumbnails, PDF, tests), [`AtlasSink`] emits crisp atlas bitmaps for upright
//! text at the scene's device scale and falls back to outlines for rotated text.

use std::collections::BTreeSet;

use kiln_scene::{AtlasFormat, AtlasPageId, AtlasUpdate, DrawOp, GlyphRun, Paint, PathData};
use kiln_text::{
    Alignment, FontStore, GlyphAtlas, PageId, ParagraphLayout, ParagraphSpec, Rgba as TextRgba,
    RunGlyphs, SpanStyle, TextEngine, TextRun, glyph_outline,
};

/// The style of a piece of chart text.
#[derive(Debug, Clone, PartialEq)]
pub struct TextStyle {
    /// Font family (as written in the file; `kiln-text` substitutes metric-compatible fonts).
    pub family: String,
    /// Size in CSS px.
    pub size: f32,
    /// Bold.
    pub bold: bool,
    /// Italic.
    pub italic: bool,
    /// Colour.
    pub paint: Paint,
}

/// Laid-out text.
#[derive(Debug, Clone)]
pub struct TextBlock {
    /// The paragraph layout (lines and glyph runs, in px from the block's top-left corner).
    pub layout: ParagraphLayout,
    /// Width of the widest line.
    pub width: f32,
    /// Height of all lines.
    pub height: f32,
    /// The style.
    pub style: TextStyle,
    /// x of the widest line's left edge inside the layout box (non-zero for centred text).
    pub left: f32,
}

/// Lays chart text out with `kiln-text`.
#[derive(Debug)]
pub struct TextMeasurer {
    engine: TextEngine,
}

impl TextMeasurer {
    /// A measurer over `fonts`.
    #[must_use]
    pub fn new(fonts: FontStore) -> Self {
        Self {
            engine: TextEngine::new(fonts),
        }
    }

    /// The fonts.
    #[must_use]
    pub fn fonts(&self) -> &FontStore {
        self.engine.fonts()
    }

    /// The text engine, for a host that lays out its own text with the same fonts (so one glyph
    /// atlas serves both: Grid's cells and the charts on the sheet).
    pub fn engine_mut(&mut self) -> &mut TextEngine {
        &mut self.engine
    }

    /// Lays `text` out in `style`, wrapping at `max_width` px (lines break at spaces like Office's
    /// chart titles and legend entries) and aligning lines as `align`.
    pub fn block(
        &mut self,
        text: &str,
        style: &TextStyle,
        max_width: f32,
        align: Alignment,
    ) -> TextBlock {
        let mut span = SpanStyle::new(style.family.clone(), style.size.max(1.0));
        span.weight = if style.bold { 700 } else { 400 };
        span.italic = style.italic;
        span.color = TextRgba::default();
        let mut spec = ParagraphSpec {
            runs: vec![TextRun::new(text, span)],
            width: max_width.max(1.0),
            ..ParagraphSpec::default()
        };
        spec.props.alignment = align;
        let layout = self.engine.layout(&spec);
        let width = layout.lines.iter().map(|l| l.width).fold(0.0f32, f32::max);
        let left = layout
            .lines
            .iter()
            .map(|l| l.x)
            .fold(f32::INFINITY, f32::min);
        let height = layout.lines.last().map_or(0.0, |l| l.top + l.height);
        TextBlock {
            layout,
            width,
            height,
            style: style.clone(),
            left: if left.is_finite() { left } else { 0.0 },
        }
    }

    /// The width of `text` on one line.
    pub fn width(&mut self, text: &str, style: &TextStyle) -> f32 {
        self.block(text, style, 100_000.0, Alignment::Left).width
    }
}

/// Where glyphs go.
pub trait GlyphSink {
    /// Appends the ops of one glyph run of a block whose layout origin (top-left of the layout
    /// box) is at `origin` in chart px, rotated by `degrees` (clockwise) around `pivot`.
    fn push_run(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        rotation: Option<(f32, (f32, f32))>,
        paint: Paint,
        out: &mut Vec<DrawOp>,
    );
}

/// Emits every glyph as a vector outline.
#[derive(Debug, Default, Clone, Copy)]
pub struct OutlineSink;

fn outline_run(
    fonts: &FontStore,
    run: &kiln_text::GlyphRun,
    origin: (f32, f32),
    rotation: Option<(f32, (f32, f32))>,
    paint: Paint,
    out: &mut Vec<DrawOp>,
) {
    let mut path = kurbo::BezPath::new();
    for g in &run.glyphs {
        if let Some(mut p) = glyph_outline(fonts, run.font, g.id, run.size) {
            p.apply_affine(kurbo::Affine::translate((
                f64::from(origin.0 + g.x),
                f64::from(origin.1 + g.y),
            )));
            path.extend(p);
        }
    }
    if let Some((deg, (px, py))) = rotation {
        let (px, py) = (f64::from(px), f64::from(py));
        path.apply_affine(
            kurbo::Affine::translate((px, py))
                * kurbo::Affine::rotate(f64::from(deg).to_radians())
                * kurbo::Affine::translate((-px, -py)),
        );
    }
    let path = PathData::from(&path);
    if !path.is_empty() {
        out.push(DrawOp::OutlineGlyphs { path, paint });
    }
}

impl GlyphSink for OutlineSink {
    fn push_run(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        rotation: Option<(f32, (f32, f32))>,
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        outline_run(fonts, run, origin, rotation, paint, out);
    }
}

/// Emits upright glyphs as atlas bitmaps (crisp at the device scale) and rotated ones as outlines.
#[derive(Debug, Default)]
pub struct AtlasSink {
    atlas: GlyphAtlas,
    dirty: BTreeSet<PageId>,
    scale: f32,
    offset: (f32, f32),
}

impl AtlasSink {
    /// A sink for a scene at `scale` device px per CSS px, whose chart origin sits at `offset`
    /// device px from the tile's device origin.
    #[must_use]
    pub fn new(scale: f32, offset: (f32, f32)) -> Self {
        Self {
            scale: if scale > 0.0 && scale.is_finite() {
                scale
            } else {
                1.0
            },
            offset,
            ..Self::default()
        }
    }

    /// Moves the chart origin (another tile).
    pub fn set_offset(&mut self, offset: (f32, f32)) {
        self.offset = offset;
    }

    /// Atlas pages for the painter: every page with `all`, else the ones changed since the last
    /// call.
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

impl GlyphSink for AtlasSink {
    fn push_run(
        &mut self,
        fonts: &FontStore,
        run: &kiln_text::GlyphRun,
        origin: (f32, f32),
        rotation: Option<(f32, (f32, f32))>,
        paint: Paint,
        out: &mut Vec<DrawOp>,
    ) {
        if rotation.is_some_and(|(deg, _)| deg.rem_euclid(360.0).abs() > 0.01) {
            outline_run(fonts, run, origin, rotation, paint, out);
            return;
        }
        let s = self.scale;
        let Ok(prepared) = self.atlas.prepare_run(
            fonts,
            run,
            self.offset.0 + origin.0 * s,
            self.offset.1 + origin.1 * s,
            s,
        ) else {
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
                            quads: Vec::new(),
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
            RunGlyphs::Outlines(_) => outline_run(fonts, run, origin, None, paint, out),
        }
    }
}

/// Draws a laid-out block with its layout origin at (`x`, `y`), optionally rotated by `degrees`
/// around (`x`, `y`) plus `pivot`.
pub fn draw_block(
    block: &TextBlock,
    fonts: &FontStore,
    x: f32,
    y: f32,
    rotation: Option<(f32, (f32, f32))>,
    sink: &mut dyn GlyphSink,
    out: &mut Vec<DrawOp>,
) {
    for line in &block.layout.lines {
        for run in &line.runs {
            sink.push_run(fonts, run, (x, y), rotation, block.style.paint, out);
        }
    }
}

/// The default fonts: the bundled metric-compatible Office fonts (native targets) or an empty
/// store the host fills (web).
#[must_use]
pub fn default_fonts() -> FontStore {
    #[cfg(not(target_arch = "wasm32"))]
    {
        FontStore::bundled()
    }
    #[cfg(target_arch = "wasm32")]
    {
        FontStore::new(&kiln_text::FontStoreOptions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style() -> TextStyle {
        TextStyle {
            family: "Calibri".into(),
            size: 12.0,
            bold: false,
            italic: false,
            paint: Paint::BLACK,
        }
    }

    #[test]
    fn measures_wraps_and_emits() {
        let mut m = TextMeasurer::new(default_fonts());
        let one = m.block("Spend by quarter", &style(), 1000.0, Alignment::Left);
        assert_eq!(one.layout.lines.len(), 1);
        assert!(one.width > 60.0 && one.width < 120.0, "{}", one.width);
        let wrapped = m.block("Spend by quarter", &style(), 60.0, Alignment::Center);
        assert!(wrapped.layout.lines.len() >= 2);
        let mut ops = Vec::new();
        draw_block(
            &one,
            m.fonts(),
            10.0,
            10.0,
            None,
            &mut OutlineSink,
            &mut ops,
        );
        assert!(matches!(ops[0], DrawOp::OutlineGlyphs { .. }));
        let mut atlas = AtlasSink::new(2.0, (0.0, 0.0));
        let mut ops = Vec::new();
        draw_block(&one, m.fonts(), 10.0, 10.0, None, &mut atlas, &mut ops);
        assert!(matches!(ops[0], DrawOp::Glyphs(_)));
        assert_eq!(atlas.updates(false).len(), 1);
        let mut ops = Vec::new();
        draw_block(
            &one,
            m.fonts(),
            10.0,
            10.0,
            Some((-90.0, (0.0, 0.0))),
            &mut atlas,
            &mut ops,
        );
        assert!(
            matches!(ops[0], DrawOp::OutlineGlyphs { .. }),
            "rotated text is vector"
        );
    }
}
