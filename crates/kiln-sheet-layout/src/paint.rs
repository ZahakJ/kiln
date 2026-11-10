// SPDX-License-Identifier: AGPL-3.0-or-later
//! Painting cells: any rectangle of a sheet as Scene ops (a 256 px tile on screen, the whole
//! used range for `kiln render`).
//!
//! Z-order (Excel's): the page, gridlines, fills (a filled cell covers the gridlines around
//! it), data bars and icons, sparklines, text, borders, then the note, comment and error-check
//! markers.
//!
//! Text follows Excel's rules: text overflows into empty neighbours (a left-aligned entry to the
//! right, a right-aligned one to the left, centred both ways) and hides the gridlines it crosses;
//! numbers never overflow and turn into `#####` when they do not fit; wrapped, shrunk, rotated
//! and merged text stays inside its cell or merge.

use std::sync::Arc;

use kiln_formula::SheetId;
use kiln_scene::{
    Bounds, ColorRole, DrawOp, Gradient, GradientKind, GradientStop, Paint, PathData,
};
use kiln_sheet::cf::CellVisual;
use kiln_sheet::styles::{BorderSide, HAlign, PatternType};
use kiln_sheet::{CellPos, CellValue, Range, Sheet, Workbook};
use kiln_text::TextEngine;
use rustc_hash::FxHashMap;

use crate::border::{diagonal_ops, edge_ops, winner};
use crate::celltext::{
    INDENT, PAD, layout_line, layout_wrapped, place, rotation, runs_for, text_width,
};
use crate::display::{Placement, Shown, digits_that_fit, shown};
use crate::geom::{SheetGeometry, Snap};
use crate::glyphs::{GlyphCache, outline_run};
use crate::icons::{ICON, icon_ops};
use crate::look::{CellLook, Colors, FillLook, resolve, rgb};
use crate::mirror::{Group, mirror_ops};

/// Error-check marker colour (Excel's green triangle).
const ERROR_MARK: u32 = 0x1E_8449;
/// Note marker colour.
const NOTE_MARK: u32 = 0xD8_2E2E;
/// Threaded-comment marker colour.
const THREAD_MARK: u32 = 0x8E_4FB8;
/// Hyperlink text colour (Office's Hyperlink style).
const LINK: u32 = 0x05_63C1;

/// What the sheet view shows besides the cells.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewOptions {
    /// Gridlines.
    pub gridlines: bool,
    /// Gridline colour.
    pub gridline: Paint,
    /// Formulas instead of results (Ctrl+`).
    pub show_formulas: bool,
    /// Zero values (otherwise blank).
    pub show_zeros: bool,
    /// The 1904 date system.
    pub date1904: bool,
    /// Note, comment and error indicators (the screen; printing leaves them out, as Excel does).
    pub markers: bool,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            gridlines: true,
            gridline: Paint::Role(ColorRole::Gridline),
            show_formulas: false,
            show_zeros: true,
            date1904: false,
            markers: true,
        }
    }
}

/// Everything painting reads.
#[derive(Debug)]
pub struct PaintCx<'a> {
    /// The workbook (values, styles, theme, tables).
    pub wb: &'a Workbook,
    /// The sheet.
    pub sheet: &'a Sheet,
    /// Its geometry.
    pub geo: &'a SheetGeometry,
    /// Conditional-format visuals of the painted cells.
    pub cf: &'a FxHashMap<CellPos, CellVisual>,
    /// View options.
    pub opts: &'a ViewOptions,
}

impl PaintCx<'_> {
    fn sid(&self) -> SheetId {
        self.sheet.id()
    }

    fn colors(&self) -> Colors<'_> {
        Colors {
            theme: self.wb.theme(),
        }
    }
}

/// Looks by style index (cells without table or conditional formats share them).
#[derive(Debug, Default)]
pub struct LookCache {
    by_style: FxHashMap<u32, Arc<CellLook>>,
}

impl LookCache {
    /// Forgets every look (after the styles or the theme changed).
    pub fn clear(&mut self) {
        self.by_style.clear();
    }
}

/// The painter of one surface: text shaping and the glyph atlas.
#[derive(Debug)]
pub struct CellPainter<'e> {
    /// Text shaping.
    pub text: &'e mut TextEngine,
    /// Glyph atlas for the scene's scale.
    pub glyphs: &'e mut GlyphCache,
    /// Shared looks.
    pub looks: &'e mut LookCache,
    /// Printing: text goes here as glyph runs instead of into the atlas.
    pub sink: Option<Vec<PrintedText>>,
}

/// A run of cell text as printing takes it: the glyphs relative to `origin` (sheet pixels), the
/// paragraph's text, its paint and the clip of the cell it overflows into.
#[derive(Debug, Clone)]
pub struct PrintedText {
    /// The glyphs.
    pub run: kiln_text::GlyphRun,
    /// The text the run's clusters index into.
    pub text: String,
    /// The paragraph's top-left, sheet pixels.
    pub origin: (f32, f32),
    /// Paint.
    pub paint: Paint,
    /// Clip, sheet pixels.
    pub clip: Option<Bounds>,
}

/// One cell's text as positioned for painting.
struct TextItem {
    /// Paragraph layout.
    layout: kiln_text::ParagraphLayout,
    /// Paints of the runs.
    paints: Vec<Paint>,
    /// Top-left of the paragraph box, sheet pixels.
    origin: (f32, f32),
    /// Clip, sheet pixels (`None`: the text fits).
    clip: Option<Bounds>,
    /// Rotation about the cell centre (radians, counter-clockwise).
    rotate: Option<(f32, (f32, f32))>,
    /// Text after a `*x` fill, right-aligned, and its paragraph.
    right: Option<(kiln_text::ParagraphLayout, (f32, f32))>,
}

/// The box a cell's text keeps in a right-to-left sheet (tile-local): its paragraph, or the
/// centre of rotated text.
fn text_group(item: &TextItem, ops: std::ops::Range<usize>, tile: (f32, f32)) -> Group {
    if let Some((_, (cx, _))) = item.rotate {
        return Group {
            ops,
            x0: cx - tile.0,
            x1: cx - tile.0,
        };
    }
    // Wrapped text has the cell's width; a single line is as wide as its text (its layout
    // width is the unwrapped limit).
    let width = |l: &kiln_text::ParagraphLayout| {
        if l.width < 50_000.0 {
            l.width
        } else {
            text_width(l)
        }
    };
    let x0 = item.origin.0 - tile.0;
    let mut x1 = x0 + width(&item.layout);
    if let Some((l, o)) = &item.right {
        x1 = x1.max(o.0 - tile.0 + width(l));
    }
    Group { ops, x0, x1 }
}

fn snap_rect(snap: Snap, r: Bounds) -> Bounds {
    let x0 = snap.at(f64::from(r.x));
    let y0 = snap.at(f64::from(r.y));
    let x1 = snap.at(f64::from(r.right()));
    let y1 = snap.at(f64::from(r.bottom()));
    Bounds::from_corners(x0, y0, x1, y1)
}

/// The columns of a row that hold something (for overflow): values, formulas, spilled results.
struct RowContent {
    cols: Vec<u32>,
}

impl CellPainter<'_> {
    /// The look of a cell.
    pub fn look(&mut self, cx: &PaintCx<'_>, row: u32, col: u32) -> Arc<CellLook> {
        let style = cx.sheet.style_at(row, col);
        let cf = cx.cf.get(&CellPos::new(row, col));
        // A table's style, or a PivotTable's: both sit under the cell's own format.
        let table = cx
            .sheet
            .table_at(row, col)
            .and_then(|_| cx.wb.table_format(cx.sid(), row, col))
            .or_else(|| {
                cx.sheet
                    .pivot_at(row, col)
                    .and_then(|p| kiln_sheet::pivot::style::pivot_cell_format(p, row, col))
            });
        if cf.is_none() && table.is_none() {
            if let Some(l) = self.looks.by_style.get(&style) {
                return l.clone();
            }
            let l = Arc::new(resolve(cx.colors(), &cx.wb.styles().get(style), None, None));
            self.looks.by_style.insert(style, l.clone());
            return l;
        }
        Arc::new(resolve(
            cx.colors(),
            &cx.wb.styles().get(style),
            table.as_ref(),
            cf,
        ))
    }

    fn row_content(cx: &PaintCx<'_>, row: u32, c0: u32, c1: u32) -> RowContent {
        let mut cols = Vec::new();
        if let Some(chunk) = cx.sheet.cells().chunk(row) {
            let start = chunk.lower_bound(c0);
            // The nearest content cell before the region (its text may overflow into it).
            let mut i = start;
            while i > 0 {
                i -= 1;
                let (c, d) = chunk.at(i);
                if d.has_content() {
                    cols.push(c);
                    break;
                }
            }
            let mut j = start;
            let mut after = false;
            while j < chunk.len() {
                let (c, d) = chunk.at(j);
                if d.has_content() {
                    cols.push(c);
                    if c > c1 {
                        after = true;
                        break;
                    }
                }
                j += 1;
            }
            let _ = after;
        }
        // Results spilled by dynamic-array formulas are not stored cells.
        for c in c0..=c1 {
            if cx.geo.cols.is_hidden(c) || cols.binary_search(&c).is_ok() {
                continue;
            }
            if cx.sheet.cells().get(row, c).is_none() && !cx.wb.value(cx.sid(), row, c).is_blank() {
                cols.push(c);
            }
        }
        cols.sort_unstable();
        cols.dedup();
        RowContent { cols }
    }

    /// The text a cell shows and how wide its column is in digits.
    fn shown(
        &self,
        cx: &PaintCx<'_>,
        row: u32,
        col: u32,
        look: &CellLook,
        width: f32,
    ) -> (Shown, CellValue) {
        let value = cx.wb.value(cx.sid(), row, col);
        if cx.opts.show_formulas
            && let Some(f) = cx.wb.formula(cx.sid(), row, col)
        {
            let text = if f.starts_with('=') {
                f
            } else {
                format!("={f}")
            };
            return (
                Shown {
                    text,
                    color: None,
                    placement: Placement::Left,
                    is_number: false,
                    hashes: false,
                    fill: None,
                },
                CellValue::Blank,
            );
        }
        let mdw = cx.sheet.max_digit_width() as f32;
        let s = shown(
            &value,
            &look.num_fmt,
            cx.opts.date1904,
            digits_that_fit(width, mdw),
            cx.opts.show_zeros,
        );
        (s, value)
    }

    /// Lays out the text of one cell in `rect` (its own or its merge's). `limits` are how far
    /// overflow may reach to the left and right (`None` for merges and wrapped text).
    #[allow(clippy::too_many_arguments)]
    fn text_item(
        &mut self,
        cx: &PaintCx<'_>,
        row: u32,
        col: u32,
        rect: Bounds,
        look: &CellLook,
        limits: Option<(f32, f32)>,
        link: bool,
    ) -> Option<TextItem> {
        let (shown, value) = self.shown(cx, row, col, look, rect.w);
        if shown.is_empty() && !shown.hashes {
            return None;
        }
        let colors = cx.colors();
        let mut ink = shown.color.map_or(look.ink, rgb);
        let mut look_font = look.font.clone();
        if link
            && look
                .font
                .color
                .is_none_or(|c| c == kiln_sheet::color::Color::theme(1, 0.0))
        {
            ink = rgb(LINK);
            if look_font.underline == kiln_sheet::styles::Underline::None {
                look_font.underline = kiln_sheet::styles::Underline::Single;
            }
        }
        let look_owned;
        let look = if look_font == look.font {
            look
        } else {
            look_owned = CellLook {
                font: look_font,
                ..look.clone()
            };
            &look_owned
        };
        let align = look.align;
        let numeric = shown.is_number || matches!(value, CellValue::Bool(_) | CellValue::Error(_));
        let content_w = (rect.w - 2.0 * PAD - f32::from(align.indent) * INDENT).max(1.0);
        let halign = match (align.horizontal, shown.placement) {
            (HAlign::General, Placement::Center) => HAlign::Center,
            (h, _) => h,
        };
        let numeric_right = numeric && shown.placement == Placement::Right;
        // A right-to-left sheet is painted left to right and mirrored ([`crate::mirror`]):
        // General follows the mirror (text at the right, numbers at the left, as in Excel),
        // while Left and Right name the sides of the cell, so they change places before the
        // mirror. Text keeps its own layout inside its box, so wrapped lines align visually.
        let mirrored = cx.geo.rtl && self.sink.is_none();
        let (halign, wrap_align) = if mirrored {
            let visual = match halign {
                HAlign::General if numeric_right => HAlign::Left,
                HAlign::General => HAlign::Right,
                h => h,
            };
            let placed = match halign {
                HAlign::Left => HAlign::Right,
                HAlign::Right => HAlign::Left,
                h => h,
            };
            (placed, visual)
        } else {
            (halign, halign)
        };
        let mut text = shown.text.clone();
        if shown.hashes {
            text = "#".to_owned();
        }
        let mut runs = runs_for(&text, &value, look, ink, colors, 1.0);
        // Stacked (vertical) text: one character per line, centred.
        if align.text_rotation == 255 {
            let stacked: String = text.chars().flat_map(|c| [c, '\n']).collect();
            runs = runs_for(
                stacked.trim_end(),
                &CellValue::Blank,
                look,
                ink,
                colors,
                1.0,
            );
            let layout = layout_wrapped(self.text, &runs, rect.w, HAlign::Center, false);
            let h = layout.height;
            let (_, y) = place(rect, rect.w, h, HAlign::Left, align.vertical, 0, false);
            return Some(TextItem {
                layout,
                paints: runs.paints,
                origin: (rect.x, y),
                clip: Some(rect),
                rotate: None,
                right: None,
            });
        }
        if let Some(angle) = rotation(align.text_rotation) {
            let layout = layout_line(self.text, &runs);
            let w = text_width(&layout);
            let h = layout.height;
            let (s, c) = angle.sin_cos();
            let bw = w * c.abs() + h * s.abs();
            let bh = w * s.abs() + h * c.abs();
            let (bx, by) = place(
                rect,
                bw,
                bh,
                halign,
                align.vertical,
                align.indent,
                numeric_right,
            );
            let center = (bx + bw / 2.0, by + bh / 2.0);
            return Some(TextItem {
                layout,
                paints: runs.paints,
                origin: (-w / 2.0, -h / 2.0),
                clip: Some(rect),
                rotate: Some((angle, center)),
                right: None,
            });
        }
        if align.wrap_text || matches!(halign, HAlign::Justify | HAlign::Distributed) && !numeric {
            let layout = layout_wrapped(
                self.text,
                &runs,
                content_w,
                wrap_align,
                numeric_right && !mirrored,
            );
            let h = layout.height;
            let x = rect.x
                + PAD
                + if matches!(halign, HAlign::Right) {
                    0.0
                } else {
                    f32::from(align.indent) * INDENT
                };
            let (_, y) = place(rect, content_w, h, HAlign::Left, align.vertical, 0, false);
            let clip = (h > rect.h - 1.0).then_some(rect);
            return Some(TextItem {
                layout,
                paints: runs.paints,
                origin: (x, y),
                clip,
                rotate: None,
                right: None,
            });
        }
        let mut layout = layout_line(self.text, &runs);
        let mut w = text_width(&layout);
        // Shrink to fit: a smaller font size, never larger.
        if align.shrink_to_fit && w > content_w && w > 0.0 {
            let k = (content_w / w).clamp(0.1, 1.0);
            runs = runs_for(&text, &value, look, ink, colors, k);
            layout = layout_line(self.text, &runs);
            w = text_width(&layout);
        }
        // HAlign::Fill repeats the text across the cell.
        if halign == HAlign::Fill && w > 0.0 && !text.is_empty() {
            let n = ((content_w / w).floor() as usize).clamp(1, 512);
            let repeated = text.repeat(n);
            runs = runs_for(&repeated, &CellValue::Blank, look, ink, colors, 1.0);
            layout = layout_line(self.text, &runs);
            w = text_width(&layout);
        }
        // Numbers that do not fit show `#` across the cell.
        if (shown.is_number || shown.hashes)
            && (w > rect.w - 2.0 && !align.shrink_to_fit || shown.hashes)
        {
            let hash_runs = runs_for("#", &CellValue::Blank, look, ink, colors, 1.0);
            let one = text_width(&layout_line(self.text, &hash_runs)).max(1.0);
            let n = (((rect.w - 2.0) / one).floor() as usize).clamp(1, 512);
            runs = runs_for(&"#".repeat(n), &CellValue::Blank, look, ink, colors, 1.0);
            layout = layout_line(self.text, &runs);
            w = text_width(&layout);
            let h = layout.height;
            let (x, y) = place(rect, w, h, HAlign::Center, align.vertical, 0, false);
            return Some(TextItem {
                layout,
                paints: runs.paints,
                origin: (x, y),
                clip: Some(rect),
                rotate: None,
                right: None,
            });
        }
        let h = layout.height;
        // A `*x` fill: left part at the left, right part at the right edge.
        if let Some((right_text, _fill)) = &shown.fill {
            let rruns = runs_for(right_text, &CellValue::Blank, look, ink, colors, 1.0);
            let rlayout = layout_line(self.text, &rruns);
            let rw = text_width(&rlayout);
            let (_, y) = place(rect, w, h, HAlign::Left, align.vertical, 0, false);
            let fits = w + rw <= rect.w - 2.0;
            if !fits {
                // Excel shows `#` when an accounting number does not fit.
                let hash_runs = runs_for("#", &CellValue::Blank, look, ink, colors, 1.0);
                let one = text_width(&layout_line(self.text, &hash_runs)).max(1.0);
                let n = (((rect.w - 2.0) / one).floor() as usize).clamp(1, 512);
                let hruns = runs_for(&"#".repeat(n), &CellValue::Blank, look, ink, colors, 1.0);
                let hl = layout_line(self.text, &hruns);
                let hw = text_width(&hl);
                let (x, y) = place(
                    rect,
                    hw,
                    hl.height,
                    HAlign::Center,
                    align.vertical,
                    0,
                    false,
                );
                return Some(TextItem {
                    layout: hl,
                    paints: hruns.paints,
                    origin: (x, y),
                    clip: Some(rect),
                    rotate: None,
                    right: None,
                });
            }
            let rx = rect.right() - rw;
            return Some(TextItem {
                layout,
                paints: runs.paints.clone(),
                origin: (rect.x + 1.0, y),
                clip: None,
                rotate: None,
                right: Some((rlayout, (rx, y))),
            });
        }
        let (x, y) = place(
            rect,
            w,
            h,
            halign,
            align.vertical,
            align.indent,
            numeric_right,
        );
        let fits = x >= rect.x && x + w <= rect.right();
        let clip = if fits {
            None
        } else if let Some((lo, hi)) = limits.filter(|_| !numeric) {
            let x0 = x.max(lo).min(rect.x);
            let x1 = (x + w).min(hi).max(rect.right());
            Some(Bounds::from_corners(x0, rect.y, x1, rect.bottom()))
        } else {
            Some(rect)
        };
        Some(TextItem {
            layout,
            paints: runs.paints,
            origin: (x, y),
            clip,
            rotate: None,
            right: None,
        })
    }

    /// Ops of the glyphs and decorations of a paragraph at `origin` (sheet px), in tile-local
    /// coordinates for a tile at `tile`.
    fn paragraph_ops(
        &mut self,
        layout: &kiln_text::ParagraphLayout,
        paints: &[Paint],
        origin: (f32, f32),
        tile: (f32, f32),
        out: &mut Vec<DrawOp>,
    ) {
        let s = self.glyphs.scale();
        let ox = origin.0 * s - (tile.0 * s).round();
        let oy = origin.1 * s - (tile.1 * s).round();
        let fallback = paints
            .first()
            .copied()
            .unwrap_or(Paint::Role(ColorRole::Ink));
        for line in &layout.lines {
            for run in &line.runs {
                let paint = paints.get(run.span).copied().unwrap_or(fallback);
                self.glyphs
                    .push_run(self.text.fonts(), run, (ox, oy), paint, out);
            }
            for d in &line.decorations {
                let paint = if d.color == kiln_text::BLACK {
                    fallback
                } else {
                    Paint::Rgba(d.color)
                };
                out.push(DrawOp::FillRect {
                    rect: Bounds::new(
                        origin.0 + d.x0 - tile.0,
                        origin.1 + d.y - tile.1,
                        d.x1 - d.x0,
                        d.thickness.max(1.0 / s),
                    ),
                    paint,
                    radius: 0.0,
                });
            }
        }
    }

    fn item_ops(&mut self, item: &TextItem, tile: (f32, f32), out: &mut Vec<DrawOp>) {
        let local = |b: Bounds| b.translate(-tile.0, -tile.1);
        if item.rotate.is_none()
            && let Some(sink) = &mut self.sink
        {
            // Printing: real text for the PDF; decorations stay ops.
            let fallback = item
                .paints
                .first()
                .copied()
                .unwrap_or(Paint::Role(ColorRole::Ink));
            let parts = std::iter::once((&item.layout, item.origin))
                .chain(item.right.as_ref().map(|(l, o)| (l, *o)));
            for (layout, origin) in parts {
                for line in &layout.lines {
                    for run in &line.runs {
                        sink.push(PrintedText {
                            run: run.clone(),
                            text: layout.text().to_owned(),
                            origin,
                            paint: item.paints.get(run.span).copied().unwrap_or(fallback),
                            clip: item.clip,
                        });
                    }
                    for d in &line.decorations {
                        out.push(DrawOp::FillRect {
                            rect: local(Bounds::new(
                                origin.0 + d.x0,
                                origin.1 + d.y,
                                d.x1 - d.x0,
                                d.thickness.max(0.5),
                            )),
                            paint: fallback,
                            radius: 0.0,
                        });
                    }
                }
            }
            return;
        }
        if let Some(c) = item.clip {
            out.push(DrawOp::PushClip {
                rect: local(c),
                path: None,
            });
        }
        if let Some((angle, (cx, cy))) = item.rotate {
            let (s, c) = (-angle).sin_cos();
            out.push(DrawOp::PushTransform {
                matrix: [c, s, -s, c, cx - tile.0, cy - tile.1],
            });
            let fallback = item
                .paints
                .first()
                .copied()
                .unwrap_or(Paint::Role(ColorRole::Ink));
            for line in &item.layout.lines {
                for run in &line.runs {
                    let path = outline_run(self.text.fonts(), run, item.origin);
                    if !path.is_empty() {
                        out.push(DrawOp::OutlineGlyphs {
                            path,
                            paint: item.paints.get(run.span).copied().unwrap_or(fallback),
                        });
                    }
                }
                for d in &line.decorations {
                    out.push(DrawOp::FillRect {
                        rect: Bounds::new(
                            item.origin.0 + d.x0,
                            item.origin.1 + d.y,
                            d.x1 - d.x0,
                            d.thickness.max(0.5),
                        ),
                        paint: fallback,
                        radius: 0.0,
                    });
                }
            }
            out.push(DrawOp::PopTransform);
        } else {
            self.paragraph_ops(&item.layout, &item.paints, item.origin, tile, out);
            if let Some((l, o)) = &item.right {
                self.paragraph_ops(l, &item.paints, *o, tile, out);
            }
        }
        if item.clip.is_some() {
            out.push(DrawOp::PopClip);
        }
    }

    /// The ops of the cells in `region` (sheet pixels), relative to `region`'s top-left.
    #[allow(clippy::too_many_lines)]
    pub fn paint(&mut self, cx: &PaintCx<'_>, region: Bounds) -> Vec<DrawOp> {
        let geo = cx.geo;
        let snap = Snap(self.glyphs.scale());
        let tile = (region.x, region.y);
        let local = |b: Bounds| b.translate(-tile.0, -tile.1);
        let mut out = vec![DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, region.w, region.h),
            paint: Paint::Role(ColorRole::Page),
            radius: 0.0,
        }];
        let mut groups: Vec<Group> = Vec::new();
        if geo.rows.is_empty() || geo.cols.is_empty() || region.w <= 0.0 || region.h <= 0.0 {
            return out;
        }
        let (r0, r1) = geo
            .rows
            .indices_in(f64::from(region.y), f64::from(region.bottom()));
        let (c0, c1) = geo
            .cols
            .indices_in(f64::from(region.x), f64::from(region.right()));
        let rows: Vec<u32> = (r0..=r1).filter(|&r| !geo.rows.is_hidden(r)).collect();
        let cols: Vec<u32> = (c0..=c1).filter(|&c| !geo.cols.is_hidden(c)).collect();
        if rows.is_empty() || cols.is_empty() {
            return out;
        }
        let area = Range::new(r0, c0, r1, c1);
        let merges: Vec<Range> = geo
            .merges
            .iter()
            .filter(|m| m.intersects(&area))
            .copied()
            .collect();
        let in_merge = |r: u32, c: u32| merges.iter().find(|m| m.contains(r, c));
        let row_y = |r: u32| geo.rows.span(r);
        let col_x = |c: u32| geo.cols.span(c);

        // ── text (laid out first: overflow hides gridlines) ────────────────────────────────
        let mut items: Vec<TextItem> = Vec::new();
        let mut overflow: Vec<Bounds> = Vec::new();
        for &r in &rows {
            let content = Self::row_content(cx, r, c0, c1);
            let (y, h) = row_y(r);
            for (k, &c) in content.cols.iter().enumerate() {
                if in_merge(r, c).is_some() || geo.merge_at(r, c).is_some() || geo.cols.is_hidden(c)
                {
                    continue;
                }
                let (x, w) = col_x(c);
                let rect = Bounds::new(x as f32, y as f32, w as f32, h as f32);
                // Overflow limits: the neighbouring content cells (or merges) in this row.
                let next = content.cols.get(k + 1).copied();
                let prev = k.checked_sub(1).and_then(|j| content.cols.get(j).copied());
                let merge_right = geo
                    .merges
                    .iter()
                    .filter(|m| m.r0 <= r && r <= m.r1 && m.c0 > c)
                    .map(|m| m.c0)
                    .min();
                let merge_left = geo
                    .merges
                    .iter()
                    .filter(|m| m.r0 <= r && r <= m.r1 && m.c1 < c)
                    .map(|m| m.c1)
                    .max();
                let hi_col = [next, merge_right].into_iter().flatten().min();
                let lo_col = [prev, merge_left].into_iter().flatten().max();
                let hi = hi_col.map_or(geo.cols.total() as f32, |n| geo.cols.offset(n) as f32);
                let lo = lo_col.map_or(0.0, |p| geo.cols.offset(p + 1) as f32);
                // Cells outside the region only matter when their text can reach into it.
                if (c < c0 && hi <= region.x) || (c > c1 && lo >= region.right()) {
                    continue;
                }
                let look = self.look(cx, r, c);
                let link = cx.sheet.hyperlink_at(r, c).is_some();
                if let Some(item) = self.text_item(cx, r, c, rect, &look, Some((lo, hi)), link) {
                    if let Some(clip) = item.clip
                        && (clip.x < rect.x || clip.right() > rect.right())
                    {
                        // The visible text extent decides which gridlines disappear.
                        let tw = text_width(&item.layout);
                        let x0 = item.origin.0.max(clip.x);
                        let x1 = (item.origin.0 + tw).min(clip.right());
                        if x0 < rect.x || x1 > rect.right() {
                            overflow.push(Bounds::from_corners(
                                x0.min(rect.x),
                                rect.y,
                                x1.max(rect.right()),
                                rect.bottom(),
                            ));
                        }
                    }
                    if item.clip.is_none_or(|c| c.intersects(&region)) {
                        items.push(item);
                    }
                }
            }
        }
        for m in &merges {
            if geo.rows.is_hidden(m.r0)
                && geo.rows.range_span(m.r0, m.r1).0 == geo.rows.range_span(m.r0, m.r1).1
            {
                continue;
            }
            let rect = geo.range_rect(m);
            let look = self.look(cx, m.r0, m.c0);
            let link = cx.sheet.hyperlink_at(m.r0, m.c0).is_some();
            if let Some(mut item) = self.text_item(cx, m.r0, m.c0, rect, &look, None, link) {
                if item.clip.is_none() && text_width(&item.layout) > rect.w {
                    item.clip = Some(rect);
                }
                items.push(item);
            }
        }

        // ── gridlines ───────────────────────────────────────────────────────────────────────
        let px = snap.px(1.0);
        if cx.opts.gridlines {
            let top = snap.at(geo.rows.offset(r0));
            let bottom = snap.at(geo.rows.offset(r1 + 1));
            let left = snap.at(geo.cols.offset(c0));
            let right = snap.at(geo.cols.offset(c1 + 1));
            for &c in &cols {
                let x = snap.at(geo.cols.offset(c + 1));
                out.push(DrawOp::FillRect {
                    rect: local(Bounds::new(x - px, top, px, bottom - top)),
                    paint: cx.opts.gridline,
                    radius: 0.0,
                });
            }
            for &r in &rows {
                let y = snap.at(geo.rows.offset(r + 1));
                out.push(DrawOp::FillRect {
                    rect: local(Bounds::new(left, y - px, right - left, px)),
                    paint: cx.opts.gridline,
                    radius: 0.0,
                });
            }
            // Merges and overflowing text hide the gridlines inside them.
            for m in &merges {
                let r = snap_rect(snap, geo.range_rect(m));
                out.push(DrawOp::FillRect {
                    rect: local(Bounds::new(r.x, r.y, r.w - px, r.h - px)),
                    paint: Paint::Role(ColorRole::Page),
                    radius: 0.0,
                });
            }
            for o in &overflow {
                let r = snap_rect(snap, *o);
                out.push(DrawOp::FillRect {
                    rect: local(Bounds::new(
                        r.x,
                        r.y,
                        (r.w - px).max(0.0),
                        (r.h - px).max(0.0),
                    )),
                    paint: Paint::Role(ColorRole::Page),
                    radius: 0.0,
                });
            }
        }

        // ── fills ───────────────────────────────────────────────────────────────────────────
        let fill_cell = |out: &mut Vec<DrawOp>, rect: Bounds, fill: &FillLook| {
            // A fill covers the gridlines around the cell.
            let r = snap_rect(snap, rect);
            let r = Bounds::new(r.x - px, r.y - px, r.w + px, r.h + px);
            fill_ops(out, local(r), fill);
        };
        for &r in &rows {
            let (y, h) = row_y(r);
            for &c in &cols {
                if in_merge(r, c).is_some() {
                    continue;
                }
                let look = self.look(cx, r, c);
                if look.fill.is_some() {
                    let (x, w) = col_x(c);
                    fill_cell(
                        &mut out,
                        Bounds::new(x as f32, y as f32, w as f32, h as f32),
                        &look.fill,
                    );
                }
            }
        }
        for m in &merges {
            let look = self.look(cx, m.r0, m.c0);
            if look.fill.is_some() {
                fill_cell(&mut out, geo.range_rect(m), &look.fill);
            }
        }

        // ── data bars and icons ────────────────────────────────────────────────────────────
        for &r in &rows {
            let (y, h) = row_y(r);
            for &c in &cols {
                let Some(v) = cx.cf.get(&CellPos::new(r, c)) else {
                    continue;
                };
                let (x, w) = col_x(c);
                let cell = Bounds::new(x as f32, y as f32, w as f32, h as f32);
                if let Some(bar) = &v.data_bar {
                    bar_ops(&mut out, cell.translate(-tile.0, -tile.1), bar);
                }
                if let Some(icon) = v.icon {
                    let iy = cell.y + ((cell.h - ICON) / 2.0).max(0.0);
                    let start = out.len();
                    let ix = cell.x + 2.0 - tile.0;
                    out.extend(icon_ops(icon, ix, iy - tile.1));
                    // Icons keep their drawing in a right-to-left sheet.
                    groups.push(Group {
                        ops: start..out.len(),
                        x0: ix,
                        x1: ix + ICON,
                    });
                }
            }
        }

        // ── sparklines ───────────────────────────────────────────────────────────────
        if !cx.sheet.meta().sparklines.is_empty() {
            // Sparklines read like charts, left to right (Plot Data Right-to-Left reverses
            // them), in a right-to-left sheet too.
            let start = out.len();
            sparkline_ops(cx, &rows, &cols, tile, &mut out);
            if start < out.len() {
                groups.push(Group {
                    ops: start..out.len(),
                    x0: 0.0,
                    x1: region.w,
                });
            }
        }

        // ── text ─────────────────────────────────────────────────────────────────────────────
        let mirrored = geo.rtl && self.sink.is_none();
        for item in &items {
            let start = out.len();
            self.item_ops(item, tile, &mut out);
            if mirrored {
                groups.push(text_group(item, start..out.len(), tile));
            }
        }

        // ── borders ─────────────────────────────────────────────────────────────────────────
        self.borders(cx, &rows, &cols, &merges, tile, snap, &mut out);

        // ── markers ─────────────────────────────────────────────────────────────────────────
        for &r in rows.iter().filter(|_| cx.opts.markers) {
            let (y, _) = row_y(r);
            for &c in &cols {
                if let Some(m) = in_merge(r, c)
                    && (m.r0 != r || m.c0 != c)
                {
                    continue;
                }
                let rect = geo.display_rect(r, c);
                let note = cx.sheet.note_at(r, c).is_some();
                let thread = !cx.sheet.threads_at(r, c).is_empty();
                if note || thread {
                    let (x1, y0) = (rect.right() - px, y as f32);
                    let mut p = PathData::new();
                    p.move_to(x1 - 6.0 - tile.0, y0 - tile.1)
                        .line_to(x1 - tile.0, y0 - tile.1)
                        .line_to(x1 - tile.0, y0 + 6.0 - tile.1)
                        .close();
                    out.push(DrawOp::Path {
                        path: p,
                        fill: Some(rgb(if thread { THREAD_MARK } else { NOTE_MARK })),
                        stroke: None,
                        rule: kiln_scene::FillRule::NonZero,
                    });
                }
                if error_flag(cx, r, c) {
                    let (x0, y0) = (rect.x, rect.y);
                    let mut p = PathData::new();
                    p.move_to(x0 - tile.0, y0 - tile.1)
                        .line_to(x0 + 6.0 - tile.0, y0 - tile.1)
                        .line_to(x0 - tile.0, y0 + 6.0 - tile.1)
                        .close();
                    out.push(DrawOp::Path {
                        path: p,
                        fill: Some(rgb(ERROR_MARK)),
                        stroke: None,
                        rule: kiln_scene::FillRule::NonZero,
                    });
                }
            }
        }
        if geo.rtl && self.sink.is_none() {
            mirror_ops(&mut out, region.w, snap.0, &groups);
        }
        out
    }

    #[allow(clippy::too_many_arguments)]
    fn borders(
        &mut self,
        cx: &PaintCx<'_>,
        rows: &[u32],
        cols: &[u32],
        merges: &[Range],
        tile: (f32, f32),
        snap: Snap,
        out: &mut Vec<DrawOp>,
    ) {
        let geo = cx.geo;
        let colors = cx.colors();
        let side_of = |this: &mut Self, r: u32, c: u32, which: u8| -> Option<(BorderSide, Paint)> {
            let look = this.look(cx, r, c);
            let s = match which {
                0 => look.border.left,
                1 => look.border.right,
                2 => look.border.top,
                _ => look.border.bottom,
            }?;
            Some((s, colors.edge(&s, &look.fill)))
        };
        let interior_h = |r: u32, c: u32| {
            // Edge between row r and the next visible row, column c.
            merges
                .iter()
                .any(|m| m.c0 <= c && c <= m.c1 && m.r0 <= r && r < m.r1)
                || geo
                    .merges
                    .iter()
                    .any(|m| m.c0 <= c && c <= m.c1 && m.r0 <= r && r < m.r1)
        };
        let interior_v = |r: u32, c: u32| {
            geo.merges
                .iter()
                .any(|m| m.r0 <= r && r <= m.r1 && m.c0 <= c && c < m.c1)
        };
        let (first_r, last_r) = (rows[0], rows[rows.len() - 1]);
        let (first_c, last_c) = (cols[0], cols[cols.len() - 1]);
        let prev_row = geo
            .rows
            .next_visible(first_r.saturating_sub(1), false)
            .filter(|&p| p < first_r);
        let next_row = geo.rows.next_visible(last_r + 1, true);
        let prev_col = geo
            .cols
            .next_visible(first_c.saturating_sub(1), false)
            .filter(|&p| p < first_c);
        let next_col = geo.cols.next_visible(last_c + 1, true);
        // Horizontal edges: above every row of the region, and below the last.
        let mut edge_rows: Vec<(Option<u32>, Option<u32>)> = Vec::new();
        edge_rows.push((prev_row, Some(first_r)));
        for w in rows.windows(2) {
            edge_rows.push((Some(w[0]), Some(w[1])));
        }
        edge_rows.push((Some(last_r), next_row));
        for (above, below) in edge_rows {
            let y = match below {
                Some(b) => geo.rows.offset(b),
                None => geo.rows.offset(above.unwrap_or(0) + 1),
            };
            let y = snap.at(y);
            for &c in cols {
                if let (Some(a), Some(_)) = (above, below)
                    && interior_h(a, c)
                {
                    continue;
                }
                let s_above = above.and_then(|a| side_of(self, a, c, 3));
                let s_below = below.and_then(|b| side_of(self, b, c, 2));
                if let Some((side, paint)) = winner(s_above, s_below) {
                    let (x, w) = geo.cols.span(c);
                    let a = snap.at(x);
                    let b = snap.at(x + w);
                    edge_ops(
                        out,
                        side,
                        paint,
                        true,
                        y - tile.1,
                        a - tile.0,
                        b - tile.0,
                        snap,
                    );
                }
            }
        }
        let mut edge_cols: Vec<(Option<u32>, Option<u32>)> = Vec::new();
        edge_cols.push((prev_col, Some(first_c)));
        for w in cols.windows(2) {
            edge_cols.push((Some(w[0]), Some(w[1])));
        }
        edge_cols.push((Some(last_c), next_col));
        for (left, right) in edge_cols {
            let x = match right {
                Some(rc) => geo.cols.offset(rc),
                None => geo.cols.offset(left.unwrap_or(0) + 1),
            };
            let x = snap.at(x);
            for &r in rows {
                if let (Some(l), Some(_)) = (left, right)
                    && interior_v(r, l)
                {
                    continue;
                }
                let s_left = left.and_then(|l| side_of(self, r, l, 1));
                let s_right = right.and_then(|rc| side_of(self, r, rc, 0));
                if let Some((side, paint)) = winner(s_left, s_right) {
                    let (y, h) = geo.rows.span(r);
                    let a = snap.at(y);
                    let b = snap.at(y + h);
                    edge_ops(
                        out,
                        side,
                        paint,
                        false,
                        x - tile.0,
                        a - tile.1,
                        b - tile.1,
                        snap,
                    );
                }
            }
        }
        // Diagonals.
        for &r in rows {
            for &c in cols {
                let look = self.look(cx, r, c);
                if let Some(d) = look.border.diagonal
                    && (look.border.diagonal_up || look.border.diagonal_down)
                {
                    let rect = geo.display_rect(r, c).translate(-tile.0, -tile.1);
                    let paint = colors.edge(&d, &look.fill);
                    if look.border.diagonal_up {
                        diagonal_ops(out, d, paint, rect, true);
                    }
                    if look.border.diagonal_down {
                        diagonal_ops(out, d, paint, rect, false);
                    }
                }
            }
        }
    }
}

/// Whether Excel's default error checking flags a cell: a formula that results in an error, or
/// a number stored as text.
fn error_flag(cx: &PaintCx<'_>, row: u32, col: u32) -> bool {
    error_rule(cx.wb, cx.sheet, row, col).is_some()
}

/// An error-checking rule that flags a cell with the green triangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorRule {
    /// A formula that results in an error value.
    ErrorValue,
    /// A number stored as text.
    NumberAsText,
}

/// The rule that flags a cell (the green triangle and the error button), unless Ignore Error
/// turned it off for the cell.
#[must_use]
pub fn error_rule(wb: &Workbook, sheet: &Sheet, row: u32, col: u32) -> Option<ErrorRule> {
    let d = sheet.cells().get(row, col)?;
    let rule = match d.value {
        CellValue::Error(_) if d.kind.is_formula() => ErrorRule::ErrorValue,
        CellValue::Text(t) if !d.kind.is_formula() => {
            let s = t.trim();
            if !s.is_empty()
                && s.parse::<f64>().is_ok()
                && !wb.styles().num_fmt(d.style).trim().eq("@")
                && !wb.styles().xf(d.style).quote_prefix
            {
                ErrorRule::NumberAsText
            } else {
                return None;
            }
        }
        _ => return None,
    };
    let ignored = sheet.meta().ignored_errors.iter().any(|g| {
        (match rule {
            ErrorRule::ErrorValue => g.eval_error,
            ErrorRule::NumberAsText => g.number_as_text,
        }) && g.ranges.iter().any(|r| r.contains(row, col))
    });
    (!ignored).then_some(rule)
}

/// The ops of a cell fill.
fn fill_ops(out: &mut Vec<DrawOp>, rect: Bounds, fill: &FillLook) {
    match fill {
        FillLook::None => {}
        FillLook::Solid(p) => out.push(DrawOp::FillRect {
            rect,
            paint: *p,
            radius: 0.0,
        }),
        FillLook::Pattern { pattern, fg, bg } => {
            out.push(DrawOp::FillRect {
                rect,
                paint: rgb(*bg),
                radius: 0.0,
            });
            pattern_ops(out, rect, *pattern, rgb(*fg));
        }
        FillLook::Gradient { degree, stops } => {
            if stops.is_empty() {
                return;
            }
            let kind = match degree {
                Some(d) => {
                    let a = (*d as f32).to_radians();
                    let (s, c) = a.sin_cos();
                    let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
                    let half = (rect.w * c.abs() + rect.h * s.abs()) / 2.0;
                    GradientKind::Linear {
                        x0: cx - c * half,
                        y0: cy - s * half,
                        x1: cx + c * half,
                        y1: cy + s * half,
                    }
                }
                None => GradientKind::Radial {
                    cx: rect.x + rect.w / 2.0,
                    cy: rect.y + rect.h / 2.0,
                    r: (rect.w.max(rect.h)) / 2.0,
                },
            };
            out.push(DrawOp::Gradient {
                rect,
                radius: 0.0,
                gradient: Gradient {
                    kind,
                    stops: stops
                        .iter()
                        .map(|&(o, c)| GradientStop {
                            offset: o.clamp(0.0, 1.0),
                            paint: rgb(c),
                        })
                        .collect(),
                },
            });
        }
    }
}

/// Excel's pattern fills as dot and line grids in the foreground colour.
fn pattern_ops(out: &mut Vec<DrawOp>, rect: Bounds, pattern: PatternType, fg: Paint) {
    use PatternType::*;
    out.push(DrawOp::PushClip {
        rect,
        path: Option::None,
    });
    let mut path = PathData::new();
    let (x0, y0, x1, y1) = (rect.x.floor(), rect.y.floor(), rect.right(), rect.bottom());
    let dots = |path: &mut PathData, step: f32, offset_odd: bool| {
        let mut y = y0;
        let mut row = 0;
        while y < y1 {
            let mut x = x0
                + if offset_odd && row % 2 == 1 {
                    step / 2.0
                } else {
                    0.0
                };
            while x < x1 {
                path.move_to(x, y)
                    .line_to(x + 1.0, y)
                    .line_to(x + 1.0, y + 1.0)
                    .line_to(x, y + 1.0)
                    .close();
                x += step;
            }
            y += step / 2.0;
            row += 1;
        }
    };
    let hlines = |path: &mut PathData, step: f32, th: f32| {
        let mut y = y0;
        while y < y1 {
            path.move_to(x0, y)
                .line_to(x1, y)
                .line_to(x1, y + th)
                .line_to(x0, y + th)
                .close();
            y += step;
        }
    };
    let vlines = |path: &mut PathData, step: f32, th: f32| {
        let mut x = x0;
        while x < x1 {
            path.move_to(x, y0)
                .line_to(x + th, y0)
                .line_to(x + th, y1)
                .line_to(x, y1)
                .close();
            x += step;
        }
    };
    let diag = |path: &mut PathData, step: f32, th: f32, down: bool| {
        let span = (x1 - x0) + (y1 - y0);
        let mut k = -(y1 - y0);
        while k < span {
            let (ax, ay, bx, by) = if down {
                (x0 + k, y0, x0 + k + (y1 - y0), y1)
            } else {
                (x0 + k, y1, x0 + k + (y1 - y0), y0)
            };
            path.move_to(ax, ay)
                .line_to(ax + th, ay)
                .line_to(bx + th, by)
                .line_to(bx, by)
                .close();
            k += step;
        }
    };
    match pattern {
        None | Solid => {}
        Gray0625 => dots(&mut path, 8.0, true),
        Gray125 => dots(&mut path, 4.0, true),
        LightGray => dots(&mut path, 4.0, false),
        MediumGray => dots(&mut path, 2.0, true),
        DarkGray => {
            out.push(DrawOp::FillRect {
                rect,
                paint: fg,
                radius: 0.0,
            });
            dots(&mut path, 2.0, true);
        }
        DarkHorizontal => hlines(&mut path, 4.0, 2.0),
        LightHorizontal => hlines(&mut path, 4.0, 1.0),
        DarkVertical => vlines(&mut path, 4.0, 2.0),
        LightVertical => vlines(&mut path, 4.0, 1.0),
        DarkDown => diag(&mut path, 4.0, 2.0, true),
        LightDown => diag(&mut path, 4.0, 1.0, true),
        DarkUp => diag(&mut path, 4.0, 2.0, false),
        LightUp => diag(&mut path, 4.0, 1.0, false),
        DarkGrid => {
            hlines(&mut path, 4.0, 2.0);
            vlines(&mut path, 4.0, 2.0);
        }
        LightGrid => {
            hlines(&mut path, 4.0, 1.0);
            vlines(&mut path, 4.0, 1.0);
        }
        DarkTrellis => {
            diag(&mut path, 4.0, 1.5, true);
            diag(&mut path, 4.0, 1.5, false);
        }
        LightTrellis => {
            diag(&mut path, 4.0, 1.0, true);
            diag(&mut path, 4.0, 1.0, false);
        }
    }
    if !path.is_empty() {
        out.push(DrawOp::Path {
            path,
            fill: Some(fg),
            stroke: Option::None,
            rule: kiln_scene::FillRule::NonZero,
        });
    }
    out.push(DrawOp::PopClip);
}

/// The sparklines of the cells in `rows` × `cols`, tile-local.
fn sparkline_ops(
    cx: &PaintCx<'_>,
    rows: &[u32],
    cols: &[u32],
    tile: (f32, f32),
    out: &mut Vec<DrawOp>,
) {
    use kiln_sheet::sparkline::{SparklineKind, axis_range, min_max, shape, values};
    let geo = cx.geo;
    let (Some(&r0), Some(&r1), Some(&c0), Some(&c1)) =
        (rows.first(), rows.last(), cols.first(), cols.last())
    else {
        return;
    };
    let theme = cx.wb.theme();
    for g in &cx.sheet.meta().sparklines {
        let visible: Vec<&kiln_sheet::sparkline::Sparkline> = g
            .sparklines
            .iter()
            .filter(|s| (r0..=r1).contains(&s.at.row) && (c0..=c1).contains(&s.at.col))
            .filter(|s| !geo.rows.is_hidden(s.at.row) && !geo.cols.is_hidden(s.at.col))
            .collect();
        if visible.is_empty() {
            continue;
        }
        // The group's range, when its axis is shared.
        let shared = matches!(g.min, kiln_sheet::sparkline::SparkAxis::Group)
            || matches!(g.max, kiln_sheet::sparkline::SparkAxis::Group);
        let group = if shared {
            g.sparklines
                .iter()
                .filter_map(|s| min_max(&values(cx.wb, cx.sid(), s, g.show_hidden)))
                .reduce(|a, b| (a.0.min(b.0), a.1.max(b.1)))
                .unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };
        #[allow(clippy::cast_possible_truncation)]
        let weight = (g.line_weight.0 * 96.0 / 72.0).max(0.75) as f32;
        let color = |c: &kiln_sheet::color::Color| rgb(c.resolve(theme, 0));
        for s in visible {
            let vals = values(cx.wb, cx.sid(), s, g.show_hidden);
            let Some(own) = min_max(&vals) else { continue };
            let range = axis_range(g, own, group);
            let shape = shape(g, &vals, range);
            let cell = geo.display_rect(s.at.row, s.at.col);
            let pad = 3.0_f32.min(cell.h / 4.0);
            let plot = Bounds::new(
                cell.x + pad - tile.0,
                cell.y + pad - tile.1,
                (cell.w - 2.0 * pad).max(1.0),
                (cell.h - 2.0 * pad).max(1.0),
            );
            let at = |x: f32, y: f32| (plot.x + x * plot.w, plot.y + y * plot.h);
            out.push(DrawOp::PushClip {
                rect: Bounds::new(cell.x - tile.0, cell.y - tile.1, cell.w, cell.h),
                path: None,
            });
            if let Some(a) = shape.axis {
                let (_, y) = at(0.0, a);
                out.push(DrawOp::FillRect {
                    rect: Bounds::new(plot.x, y - 0.5, plot.w, 1.0),
                    paint: color(&g.colors.axis),
                    radius: 0.0,
                });
            }
            match g.kind {
                SparklineKind::Line => {
                    for run in &shape.runs {
                        let mut p = PathData::new();
                        for (i, &(x, y)) in run.iter().enumerate() {
                            let (px, py) = at(x, y);
                            if i == 0 {
                                p.move_to(px, py);
                            } else {
                                p.line_to(px, py);
                            }
                        }
                        if run.len() == 1 {
                            let (px, py) = at(run[0].0, run[0].1);
                            p.line_to(px + 0.01, py);
                        }
                        out.push(DrawOp::Path {
                            path: p,
                            fill: None,
                            stroke: Some(kiln_scene::Stroke {
                                join: kiln_scene::LineJoin::Round,
                                cap: kiln_scene::LineCap::Round,
                                ..kiln_scene::Stroke::solid(color(&g.colors.series), weight)
                            }),
                            rule: kiln_scene::FillRule::NonZero,
                        });
                    }
                    let r = (weight + 1.5).clamp(2.0, 4.0);
                    for m in &shape.marks {
                        let Some(c) = &m.mark else { continue };
                        let (px, py) = at(m.x, m.y0);
                        out.push(DrawOp::FillRect {
                            rect: Bounds::new(px - r / 2.0, py - r / 2.0, r, r),
                            paint: color(c),
                            radius: r / 2.0,
                        });
                    }
                }
                SparklineKind::Column | SparklineKind::WinLoss => {
                    for m in &shape.marks {
                        let (x0, y0) = at(m.x, m.y0);
                        let (_, y1) = at(m.x, m.y1);
                        let paint = m
                            .mark
                            .as_ref()
                            .map_or_else(|| color(&g.colors.series), color);
                        out.push(DrawOp::FillRect {
                            rect: Bounds::new(x0, y0, (m.w * plot.w).max(1.0), (y1 - y0).max(1.0)),
                            paint,
                            radius: 0.0,
                        });
                    }
                }
            }
            out.push(DrawOp::PopClip);
        }
    }
}

/// A data bar inside `cell` (tile-local).
fn bar_ops(out: &mut Vec<DrawOp>, cell: Bounds, bar: &kiln_sheet::cf::DataBarVisual) {
    let inner = Bounds::new(
        cell.x + 2.0,
        cell.y + 2.0,
        (cell.w - 4.0).max(0.0),
        (cell.h - 4.0).max(0.0),
    );
    let (a, b) = (bar.start.min(bar.end) as f32, bar.start.max(bar.end) as f32);
    let (a, b) = if bar.right_to_left {
        (1.0 - b, 1.0 - a)
    } else {
        (a, b)
    };
    let rect = Bounds::new(inner.x + a * inner.w, inner.y, (b - a) * inner.w, inner.h);
    if rect.w <= 0.0 {
        return;
    }
    let color = rgb(bar.color);
    if bar.gradient {
        // Excel's gradient bars fade towards the page (white on a light page; the dark page in
        // the dark theme, so automatic text stays readable on them).
        let fade = Paint::Role(ColorRole::Page);
        let (x0, x1) = if bar.negative != bar.right_to_left {
            (rect.right(), rect.x)
        } else {
            (rect.x, rect.right())
        };
        out.push(DrawOp::Gradient {
            rect,
            radius: 0.0,
            gradient: Gradient {
                kind: GradientKind::Linear {
                    x0,
                    y0: rect.y,
                    x1,
                    y1: rect.y,
                },
                stops: vec![
                    GradientStop {
                        offset: 0.0,
                        paint: color,
                    },
                    GradientStop {
                        offset: 1.0,
                        paint: fade,
                    },
                ],
            },
        });
    } else {
        out.push(DrawOp::FillRect {
            rect,
            paint: color,
            radius: 0.0,
        });
    }
    if let Some(border) = bar.border {
        out.push(DrawOp::StrokeRect {
            rect: rect.inflate(-0.5, -0.5),
            paint: rgb(border),
            width: 1.0,
            radius: 0.0,
        });
    }
    if let Some((pos, axis_color)) = bar.axis {
        let x = inner.x + pos as f32 * inner.w;
        let mut p = PathData::new();
        p.move_to(x, cell.y).line_to(x, cell.bottom());
        out.push(DrawOp::Path {
            path: p,
            fill: None,
            stroke: Some(kiln_scene::Stroke {
                dash: vec![2.0, 1.0],
                ..kiln_scene::Stroke::solid(rgb(axis_color), 1.0)
            }),
            rule: kiln_scene::FillRule::NonZero,
        });
    }
}
