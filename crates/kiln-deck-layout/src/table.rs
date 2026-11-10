// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tables on slides: the column grid and row heights (rows grow to fit their text, as in
//! PowerPoint), merged cells, the table style's region fills, text colours and borders, cell
//! fills, borders, margins and anchoring, and the cells' text.
//!
//! Table styles are the built-in ones a presentation names by GUID (PowerPoint's default
//! "Medium Style 2 - Accent 1" and its accents, "No Style, Table Grid", "No Style, No Grid");
//! unknown ids draw like the default.

use std::sync::Arc;

use kiln_common::NodeId;
use kiln_deck::text::{ResolvedBodyProps, TextAnchor};
use kiln_deck::{Presentation, Table, TextBody};
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_scene::{Bounds, DrawOp, FillRule, LineCap, LineJoin, Paint, PathData, Stroke};
use kiln_text::TextEngine;

use crate::geom::px;
use crate::paint::{Look, Paintable, color, paintable};
use crate::text::{BodyInput, BodyLayout, TextEnv, layout_body};

/// The default style ("Medium Style 2 - Accent 1").
pub const DEFAULT_STYLE: &str = "{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}";
/// "No Style, No Grid".
pub const PLAIN_STYLE: &str = "{2D5ABB26-0587-4C30-8999-92F81FD0307C}";
/// "No Style, Table Grid".
pub const GRID_STYLE: &str = "{5940675A-B579-460E-94D1-54222C63F5DA}";
/// "Medium Style 2" (dark 1).
pub const NEUTRAL_STYLE: &str = "{073A0DAA-6AF3-43AB-8588-CEC1D06C72B9}";

/// The built-in styles Kiln draws, by GUID: `(id, family, accent scheme slot)`.
pub const STYLES: &[(&str, Family, &str)] = &[
    (DEFAULT_STYLE, Family::Medium2, "accent1"),
    (
        "{21E4AEA4-8DFA-4A89-87EB-49C32662AFE8}",
        Family::Medium2,
        "accent2",
    ),
    (
        "{F5AB1C69-6EDB-4FF4-983F-18BD219EF322}",
        Family::Medium2,
        "accent3",
    ),
    (
        "{00A15C55-8517-42AA-B614-E9B94910E393}",
        Family::Medium2,
        "accent4",
    ),
    (
        "{7DF18680-E054-41AD-8BC1-D1AEF772440D}",
        Family::Medium2,
        "accent5",
    ),
    (
        "{93296810-A885-4BE3-A3E7-6D5BEEA58F35}",
        Family::Medium2,
        "accent6",
    ),
    (NEUTRAL_STYLE, Family::Medium2, "dk1"),
    (GRID_STYLE, Family::Grid, "tx1"),
    (PLAIN_STYLE, Family::Plain, "tx1"),
];

/// How a built-in style looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
    /// Tinted body, banded rows, a solid header and total row, white rules.
    Medium2,
    /// Thin rules in the text colour, no fills.
    Grid,
    /// Nothing.
    Plain,
}

/// One laid-out cell.
#[derive(Debug, Clone, PartialEq)]
pub struct CellBox {
    /// Row of the cell's top-left grid square.
    pub row: usize,
    /// Column.
    pub col: usize,
    /// Rows and columns it spans.
    pub span: (usize, usize),
    /// Rectangle in table-local px.
    pub rect: Bounds,
    /// The text.
    pub text: Arc<BodyLayout>,
    /// The cell's fill.
    pub fill: Paintable,
}

/// A laid-out table.
#[derive(Debug, Clone, PartialEq)]
pub struct TableBox {
    /// Column edges (table-local px), `cols + 1` values.
    pub xs: Vec<f32>,
    /// Row edges, `rows + 1` values.
    pub ys: Vec<f32>,
    /// The cells (merged continuation squares are left out).
    pub cells: Vec<CellBox>,
    /// Rules: `(x0, y0, x1, y1, colour, width)`.
    pub rules: Vec<(f32, f32, f32, f32, u32, f32)>,
}

impl TableBox {
    /// The cell under a point (table-local px).
    #[must_use]
    pub fn cell_at(&self, x: f32, y: f32) -> Option<&CellBox> {
        self.cells.iter().find(|c| c.rect.contains(x, y))
    }

    /// Total height.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.ys.last().copied().unwrap_or(0.0)
    }
}

struct Region {
    fill: Paintable,
    text: Option<u32>,
    bold: bool,
}

/// Lays out a table of a shape whose box is `w` × `h` px.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn layout_table(
    engine: &mut TextEngine,
    pres: &Presentation,
    shape: NodeId,
    table: &Table,
    look: &Look<'_>,
    env: &TextEnv<'_>,
) -> TableBox {
    let style = table.style_id.as_deref().unwrap_or(DEFAULT_STYLE);
    let (family, slot) = STYLES
        .iter()
        .find(|(id, _, _)| id.eq_ignore_ascii_case(style))
        .map_or((Family::Medium2, "accent1"), |(_, f, s)| (*f, *s));
    let accent = color(&DrawingColor::scheme(slot), &look.ctx);
    let lt1 = color(&DrawingColor::scheme("lt1"), &look.ctx);
    let tx1 = color(&DrawingColor::scheme("tx1"), &look.ctx);
    let rows = table.rows.len();
    let cols = table.grid.len();
    let mut xs = Vec::with_capacity(cols + 1);
    let mut x = 0.0f32;
    xs.push(0.0);
    for w in &table.grid {
        x += px(*w).max(1.0);
        xs.push(x);
    }
    let region = |r: usize, c: usize| -> Region {
        let lk = &table.look;
        let header = lk.first_row && r == 0;
        let total = lk.last_row && r + 1 == rows && rows > 1;
        let first_col = lk.first_col && c == 0;
        let last_col = lk.last_col && c + 1 == cols && cols > 1;
        match family {
            Family::Plain => Region {
                fill: Paintable::None,
                text: None,
                bold: header || total,
            },
            Family::Grid => Region {
                fill: Paintable::None,
                text: None,
                bold: header || total,
            },
            Family::Medium2 => {
                if header || total || first_col || last_col {
                    Region {
                        fill: Paintable::Solid(accent),
                        text: Some(lt1),
                        bold: true,
                    }
                } else {
                    let body_row = if lk.first_row { r.saturating_sub(1) } else { r };
                    let body_col = if lk.first_col { c.saturating_sub(1) } else { c };
                    let banded = (lk.band_row && body_row % 2 == 0)
                        || (lk.band_col && !lk.band_row && body_col % 2 == 0);
                    // Office's tints of the accent (computed as Office does, not mixed in sRGB).
                    let tint = if banded { 40_000 } else { 20_000 };
                    Region {
                        fill: Paintable::Solid(color(
                            &DrawingColor::scheme(slot).with("tint", tint),
                            &look.ctx,
                        )),
                        text: Some(color(&DrawingColor::scheme("dk1"), &look.ctx)),
                        bold: false,
                    }
                }
            }
        }
    };
    // Cell text first: rows grow to fit.
    struct Pending {
        row: usize,
        col: usize,
        span: (usize, usize),
        fill: Paintable,
        text: Option<u32>,
        bold: bool,
        margins: [f32; 4],
        anchor: TextAnchor,
        body: TextBody,
    }
    let mut pending = Vec::new();
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if cell.h_merge || cell.v_merge || c >= cols {
                continue;
            }
            let span = (
                usize::try_from(cell.row_span.max(1))
                    .unwrap_or(1)
                    .min(rows - r),
                usize::try_from(cell.grid_span.max(1))
                    .unwrap_or(1)
                    .min(cols - c),
            );
            let reg = region(r, c);
            let fill = cell
                .props
                .fill
                .as_ref()
                .map_or(reg.fill, |f| paintable(f, &look.ctx));
            let m = |i: usize, d: i64| px(cell.props.margins[i].unwrap_or(d));
            pending.push(Pending {
                row: r,
                col: c,
                span,
                fill,
                text: reg.text,
                bold: reg.bold,
                margins: [m(0, 91_440), m(1, 45_720), m(2, 91_440), m(3, 45_720)],
                anchor: cell.props.anchor.unwrap_or(TextAnchor::Top),
                body: cell.text.clone(),
            });
        }
    }
    let mut heights: Vec<f32> = table.rows.iter().map(|r| px(r.height).max(4.0)).collect();
    let mut texts: Vec<Arc<BodyLayout>> = Vec::with_capacity(pending.len());
    for p in &pending {
        let w = xs[p.col + p.span.1] - xs[p.col];
        let props = ResolvedBodyProps {
            insets: [0; 4],
            anchor: TextAnchor::Top,
            anchor_center: false,
            autofit: kiln_deck::Autofit::Off,
            columns: 1,
            column_spacing: 0,
            rtl_columns: false,
            wrap: kiln_deck::text::TextWrap::Square,
            direction: kiln_deck::text::TextDirection::Horizontal,
            rotation: 0,
            upright: false,
        };
        let cell_env = TextEnv {
            fallback_color: p.text.or(env.fallback_color),
            fallback_bold: p.bold,
            ..*env
        };
        let area = Bounds::new(
            p.margins[0],
            p.margins[1],
            (w - p.margins[0] - p.margins[2]).max(4.0),
            10_000.0,
        );
        let layout = layout_body(
            engine,
            pres,
            &BodyInput {
                shape,
                body: &p.body,
                props: &props,
                rect: area,
            },
            &cell_env,
        );
        let need = layout.content_height + p.margins[1] + p.margins[3];
        if p.span.0 == 1 {
            heights[p.row] = heights[p.row].max(need);
        }
        texts.push(Arc::new(layout));
    }
    let mut ys = Vec::with_capacity(rows + 1);
    let mut y = 0.0f32;
    ys.push(0.0);
    for h in &heights {
        y += *h;
        ys.push(y);
    }
    let mut cells = Vec::with_capacity(pending.len());
    for (p, text) in pending.into_iter().zip(texts) {
        let rect = Bounds::new(
            xs[p.col],
            ys[p.row],
            xs[p.col + p.span.1] - xs[p.col],
            ys[p.row + p.span.0] - ys[p.row],
        );
        // Anchor the text in the cell now that the row heights are known.
        let mut text = (*text).clone();
        let inner = rect.h - p.margins[1] - p.margins[3];
        let shift = match p.anchor {
            TextAnchor::Middle => (inner - text.content_height) / 2.0,
            TextAnchor::Bottom => inner - text.content_height,
            _ => 0.0,
        }
        .max(0.0);
        text.area = Bounds::new(
            rect.x + p.margins[0],
            rect.y + p.margins[1],
            text.area.w,
            inner.max(1.0),
        );
        for l in &mut text.lines {
            l.dy += shift;
        }
        cells.push(CellBox {
            row: p.row,
            col: p.col,
            span: p.span,
            rect,
            text: Arc::new(text),
            fill: p.fill,
        });
    }
    // Rules: the edges of every cell (none run through merged cells); the header's bottom and
    // the total row's top are heavier in the Medium styles.
    let mut rules = Vec::new();
    let rule = match family {
        Family::Medium2 => Some((lt1, 1.0f32 * 4.0 / 3.0)),
        Family::Grid => Some((tx1, 1.0f32 * 4.0 / 3.0)),
        Family::Plain => None,
    };
    if let Some((c, w)) = rule {
        let heavy = 3.0f32 * 4.0 / 3.0;
        for cell in &cells {
            let r = cell.rect;
            let below_header = family == Family::Medium2 && table.look.first_row && cell.row == 1;
            let above_total = family == Family::Medium2
                && table.look.last_row
                && rows > 1
                && cell.row + cell.span.0 == rows - 1;
            rules.push((
                r.x,
                r.y,
                r.right(),
                r.y,
                c,
                if below_header { heavy } else { w },
            ));
            rules.push((
                r.x,
                r.bottom(),
                r.right(),
                r.bottom(),
                c,
                if above_total { heavy } else { w },
            ));
            rules.push((r.x, r.y, r.x, r.bottom(), c, w));
            rules.push((r.right(), r.y, r.right(), r.bottom(), c, w));
        }
    }
    // Cell borders the file sets (left, top, right, bottom).
    for (r, row) in table.rows.iter().enumerate() {
        for (c, cell) in row.cells.iter().enumerate() {
            if c >= cols {
                continue;
            }
            for (side, b) in cell.props.borders.iter().enumerate() {
                let Some(line) = &b.0 else { continue };
                let (x0, y0, x1, y1) = match side {
                    0 => (xs[c], ys[r], xs[c], ys[r + 1]),
                    1 => (xs[c], ys[r], xs[c + 1], ys[r]),
                    2 => (xs[c + 1], ys[r], xs[c + 1], ys[r + 1]),
                    _ => (xs[c], ys[r + 1], xs[c + 1], ys[r + 1]),
                };
                match &line.fill {
                    Some(Fill::None) => rules.push((x0, y0, x1, y1, 0, 0.0)),
                    Some(Fill::Solid(col)) => rules.push((
                        x0,
                        y0,
                        x1,
                        y1,
                        color(col, &look.ctx),
                        px(line.width.unwrap_or(12_700)).max(0.5),
                    )),
                    _ => {}
                }
            }
        }
    }
    TableBox {
        xs,
        ys,
        cells,
        rules,
    }
}

/// Draws a laid-out table at `(x, y)` in scene px (an upright table).
pub fn draw_table(
    out: &mut Vec<DrawOp>,
    t: &TableBox,
    origin: (f32, f32),
    fonts: &kiln_text::FontStore,
    glyphs: &mut crate::glyphs::Glyphs,
    pictures: &mut dyn crate::paint::PictureSource,
    matrix: Option<kurbo::Affine>,
) {
    let m = matrix
        .unwrap_or_else(|| kurbo::Affine::translate((f64::from(origin.0), f64::from(origin.1))));
    for c in &t.cells {
        let rect = PathData::rect(c.rect);
        let path = crate::geom::transform_path(&rect, m);
        let b = crate::geom::transform_bounds(c.rect, m);
        crate::paint::fill_path(out, &path, b, &c.fill, pictures);
    }
    for &(x0, y0, x1, y1, col, w) in &t.rules {
        if w <= 0.0 || col & 0xff == 0 {
            continue;
        }
        let mut p = PathData::new();
        p.move_to(x0, y0).line_to(x1, y1);
        out.push(DrawOp::Path {
            path: crate::geom::transform_path(&p, m),
            fill: None,
            stroke: Some(Stroke {
                paint: Paint::Rgba(col),
                width: w,
                join: LineJoin::Miter,
                cap: LineCap::Square,
                dash: Vec::new(),
            }),
            rule: FillRule::NonZero,
        });
    }
    for c in &t.cells {
        crate::text::draw_body(out, &c.text, fonts, glyphs, m, matrix.is_none());
    }
}
