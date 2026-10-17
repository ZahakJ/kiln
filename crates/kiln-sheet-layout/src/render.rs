// SPDX-License-Identifier: AGPL-3.0-or-later
//! A whole sheet area as one self-contained Scene delta (tiles, glyph atlas), for `kiln render`
//! PNGs, golden tests, thumbnails and print previews. The same painter as the screen, so a PNG
//! shows exactly what the grid shows.

use kiln_scene::{Bounds, ColorRole, Paint, SceneDelta, Tile, TileId};
use kiln_sheet::{Range, Workbook};
use kiln_text::{FontStore, TextEngine};

use crate::geom::{SheetGeometry, Snap};
use crate::glyphs::GlyphCache;
use crate::headers::{HeaderMarks, column_header_ops, corner_ops, row_header_ops};
use crate::paint::{CellPainter, LookCache, PaintCx, ViewOptions};
use crate::scene::{COL_HEADER_LAYER, CORNER_LAYER, ROW_HEADER_LAYER, cell_tile_id};

/// Tile side of renders (larger tiles, fewer seams to hide).
const BLOCK: f32 = 512.0;

/// What [`render_sheet`] draws.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderOptions {
    /// Device pixels per CSS pixel.
    pub scale: f32,
    /// Row and column headers.
    pub headings: bool,
    /// View options (gridlines, formulas…); `None` takes the sheet's own.
    pub view: Option<ViewOptions>,
    /// The area (`None`: the used range, at least A1:J20 so empty sheets still show a grid).
    pub range: Option<Range>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            scale: 1.0,
            headings: true,
            view: None,
            range: None,
        }
    }
}

/// A rendered area: the delta (to apply to a fresh compositor) and the rectangle to show, in
/// sheet pixels (headers at negative coordinates).
#[derive(Debug, Clone)]
pub struct SheetRender {
    /// Tiles and atlas pages.
    pub delta: SceneDelta,
    /// What to composite.
    pub rect: Bounds,
    /// The cells covered.
    pub range: Range,
}

/// The geometry of a sheet (rows, columns, merges, frozen panes, headings).
#[must_use]
pub fn geometry_of(sheet: &kiln_sheet::Sheet, headings: bool) -> SheetGeometry {
    let view = &sheet.meta().view;
    SheetGeometry {
        rows: crate::axis::AxisIndex::from_axis(sheet.rows()),
        cols: crate::axis::AxisIndex::from_axis(sheet.cols()),
        freeze_rows: view.freeze.map_or(0, |f| f.rows),
        freeze_cols: view.freeze.map_or(0, |f| f.cols),
        headings,
        merges: sheet.merges().to_vec(),
    }
}

/// The view options a sheet asks for.
#[must_use]
pub fn view_options(wb: &Workbook, sheet: &kiln_sheet::Sheet) -> ViewOptions {
    let v = &sheet.meta().view;
    ViewOptions {
        gridlines: v.show_gridlines,
        gridline: v
            .gridline_color
            .map_or(Paint::Role(ColorRole::Gridline), |c| {
                crate::look::rgb(c.resolve(wb.theme(), 0xD4_D4D4))
            }),
        show_formulas: v.show_formulas,
        show_zeros: v.show_zeros,
        date1904: wb.props().date1904,
    }
}

/// Renders an area of a sheet.
#[must_use]
pub fn render_sheet(
    wb: &mut Workbook,
    sheet_index: usize,
    fonts: FontStore,
    opts: &RenderOptions,
) -> Option<SheetRender> {
    let sheet = wb.sheets().get(sheet_index)?;
    let sid = sheet.id();
    let geo = geometry_of(sheet, opts.headings);
    let view = opts.view.clone().unwrap_or_else(|| view_options(wb, sheet));
    let used = sheet.cells().used_range(false);
    let range = opts.range.unwrap_or_else(|| {
        let base = Range::new(0, 0, 19, 9);
        let mut r = used.map_or(base, |u| {
            Range::new(0, 0, u.r1.max(base.r1), u.c1.max(base.c1))
        });
        for m in sheet.merges() {
            r = r.union(&Range::new(0, 0, m.r1, m.c1));
        }
        r
    });
    let cells = geo.range_rect(&range);
    let hw = geo.header_w(range.r1);
    let hh = geo.header_h();
    let cf = wb.conditional_formats(sid, range);
    let wb: &Workbook = wb;
    let sheet = wb.sheets().get(sheet_index)?;
    let mut text = TextEngine::new(fonts);
    let mut glyphs = GlyphCache::new();
    glyphs.set_scale(opts.scale);
    let mut looks = LookCache::default();
    let cx = PaintCx {
        wb,
        sheet,
        geo: &geo,
        cf: &cf,
        opts: &view,
    };
    let marks = HeaderMarks::default();
    let mut tiles = Vec::new();
    let nx = (cells.w / BLOCK).ceil().max(1.0) as u32;
    let ny = (cells.h / BLOCK).ceil().max(1.0) as u32;
    for ty in 0..ny {
        for tx in 0..nx {
            let x = cells.x + tx as f32 * BLOCK;
            let y = cells.y + ty as f32 * BLOCK;
            let rect = Bounds::new(
                x,
                y,
                BLOCK.min(cells.right() - x),
                BLOCK.min(cells.bottom() - y),
            );
            let mut painter = CellPainter {
                text: &mut text,
                glyphs: &mut glyphs,
                looks: &mut looks,
            };
            let ops = painter.paint(&cx, rect);
            tiles.push(Tile::new(cell_tile_id(tx, ty), rect, ops));
        }
    }
    if opts.headings {
        for tx in 0..nx {
            let x = cells.x + tx as f32 * BLOCK;
            let rect = Bounds::new(x, -hh, BLOCK.min(cells.right() - x), hh);
            let ops = column_header_ops(&geo, rect, &marks, &mut text, &mut glyphs);
            tiles.push(Tile::new(
                TileId {
                    layer: COL_HEADER_LAYER,
                    index: tx,
                },
                rect,
                ops,
            ));
        }
        for ty in 0..ny {
            let y = cells.y + ty as f32 * BLOCK;
            let rect = Bounds::new(-hw, y, hw, BLOCK.min(cells.bottom() - y));
            let ops = row_header_ops(&geo, rect, &marks, &mut text, &mut glyphs);
            tiles.push(Tile::new(
                TileId {
                    layer: ROW_HEADER_LAYER,
                    index: ty,
                },
                rect,
                ops,
            ));
        }
        tiles.push(Tile::new(
            TileId {
                layer: CORNER_LAYER,
                index: 0,
            },
            Bounds::new(-hw, -hh, hw, hh),
            corner_ops(hw, hh, false, Snap(opts.scale)),
        ));
    }
    let rect = if opts.headings {
        Bounds::from_corners(-hw, -hh, cells.right(), cells.bottom())
    } else {
        cells
    };
    Some(SheetRender {
        delta: SceneDelta {
            revision: 0,
            reset: true,
            scale: opts.scale,
            extent: rect,
            background: Paint::Role(ColorRole::Page),
            tiles,
            removed: Vec::new(),
            atlas: glyphs.updates(true),
            images: Vec::new(),
            overlay: None,
        },
        rect,
        range,
    })
}
