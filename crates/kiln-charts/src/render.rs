// SPDX-License-Identifier: AGPL-3.0-or-later
//! Drawing a laid-out chart into Scene ops, and the selection overlay of a chart element.
//!
//! Ops are in chart px with the chart's top-left corner at (0, 0); a host places the chart with a
//! `PushTransform` (and, for crisp text, an [`crate::text::AtlasSink`] that knows the chart's
//! device offset in the tile).

use kiln_ooxml::drawingml::theme::Theme;
use kiln_scene::{Bounds, ColorRole, DrawOp, FillRule, Paint};

use crate::layout::{ChartElement, ChartLayout, layout_chart};
use crate::model::ChartSpace;
use crate::palette::{ChartColors, ColorMode};
use crate::text::{GlyphSink, TextMeasurer, draw_block};

/// Draws a laid-out chart.
#[must_use]
pub fn draw(layout: &ChartLayout, text: &TextMeasurer, sink: &mut dyn GlyphSink) -> Vec<DrawOp> {
    let mut ops = Vec::with_capacity(layout.marks.len() + layout.texts.len() * 2 + 4);
    let clip = layout.plot.inflate(6.0, 6.0);
    let mut clipped = false;
    for m in &layout.marks {
        if m.path.is_empty() || (m.fill.is_none() && m.stroke.is_none()) {
            continue;
        }
        let data = matches!(
            m.element,
            ChartElement::Point { .. }
                | ChartElement::Series { .. }
                | ChartElement::Trendline { .. }
        );
        if data != clipped {
            if data {
                ops.push(DrawOp::PushClip {
                    rect: clip,
                    path: None,
                });
            } else {
                ops.push(DrawOp::PopClip);
            }
            clipped = data;
        }
        ops.push(DrawOp::Path {
            path: m.path.clone(),
            fill: m.fill,
            stroke: m.stroke.clone(),
            rule: FillRule::NonZero,
        });
    }
    if clipped {
        ops.push(DrawOp::PopClip);
    }
    for t in &layout.texts {
        let rotation = (t.rotation != 0.0).then(|| (t.rotation, t.pivot()));
        draw_block(&t.block, text.fonts(), t.x, t.y, rotation, sink, &mut ops);
    }
    ops
}

/// Lays out and draws a chart at `width` × `height` px in one call.
#[must_use]
pub fn render(
    cs: &ChartSpace,
    width: f32,
    height: f32,
    theme: Option<&Theme>,
    mode: ColorMode,
    text: &mut TextMeasurer,
    sink: &mut dyn GlyphSink,
) -> (ChartLayout, Vec<DrawOp>) {
    let colors = ChartColors::new(theme, mode);
    let layout = layout_chart(cs, width, height, &colors, text);
    let ops = draw(&layout, text, sink);
    (layout, ops)
}

/// The selection overlay of a chart: the chart frame with its eight sizing handles, plus an
/// outline around the selected element (Office's look, in the app's accent colour).
#[must_use]
pub fn selection_ops(layout: &ChartLayout, element: Option<ChartElement>) -> Vec<DrawOp> {
    let accent = Paint::Role(ColorRole::Caret);
    let frame = Bounds::new(0.0, 0.0, layout.width, layout.height);
    let mut ops = vec![DrawOp::StrokeRect {
        rect: frame,
        paint: accent,
        width: 1.5,
        radius: 0.0,
    }];
    let hs = 8.0;
    for (x, y) in [
        (0.0, 0.0),
        (0.5, 0.0),
        (1.0, 0.0),
        (0.0, 0.5),
        (1.0, 0.5),
        (0.0, 1.0),
        (0.5, 1.0),
        (1.0, 1.0),
    ] {
        let (cx, cy) = (x * layout.width, y * layout.height);
        let r = Bounds::new(cx - hs / 2.0, cy - hs / 2.0, hs, hs);
        ops.push(DrawOp::FillRect {
            rect: r,
            paint: Paint::Role(ColorRole::Page),
            radius: hs / 2.0,
        });
        ops.push(DrawOp::StrokeRect {
            rect: r,
            paint: accent,
            width: 1.25,
            radius: hs / 2.0,
        });
    }
    if let Some(el) = element.filter(|e| *e != ChartElement::ChartArea)
        && let Some(b) = layout.element_bounds(el)
    {
        let b = b.inflate(2.0, 2.0);
        ops.push(DrawOp::StrokeRect {
            rect: b,
            paint: accent,
            width: 1.0,
            radius: 2.0,
        });
        for (x, y) in [
            (b.x, b.y),
            (b.right(), b.y),
            (b.x, b.bottom()),
            (b.right(), b.bottom()),
        ] {
            ops.push(DrawOp::FillRect {
                rect: Bounds::new(x - 2.5, y - 2.5, 5.0, 5.0),
                paint: accent,
                radius: 1.0,
            });
        }
    }
    ops
}

/// A whole chart as a one-tile [`kiln_scene::SceneDelta`] at `scale` device px per CSS px (for
/// thumbnails, `kiln render`, golden images and the chart previews of the insert galleries).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn scene_delta(
    cs: &ChartSpace,
    width: f32,
    height: f32,
    theme: Option<&Theme>,
    mode: ColorMode,
    scale: f32,
    text: &mut TextMeasurer,
) -> (ChartLayout, kiln_scene::SceneDelta) {
    let mut sink = crate::text::AtlasSink::new(scale, (0.0, 0.0));
    let (layout, ops) = render(cs, width, height, theme, mode, text, &mut sink);
    let rect = Bounds::new(0.0, 0.0, layout.width, layout.height);
    let delta = kiln_scene::SceneDelta {
        revision: 1,
        reset: true,
        scale,
        extent: rect,
        background: Paint::Role(ColorRole::Canvas),
        tiles: vec![kiln_scene::Tile::new(
            kiln_scene::TileId::content(0),
            rect,
            ops,
        )],
        atlas: sink.updates(true),
        ..kiln_scene::SceneDelta::default()
    };
    (layout, delta)
}
