// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tables: column widths (fixed layout from the grid; autofit from preferred widths, the grid
//! and the content's narrowest and widest widths, to the table's preferred width or the window),
//! cell content, row heights (auto, at least, exact; vertically merged cells stretch the last
//! row they span), cell margins, shading and borders with Word's conflict resolution.

use kiln_common::NodeId;
use kiln_doc::Table;
use kiln_doc::props::{Border, CellMargins, TableWidth};
use kiln_doc::resolve::{resolve_cell, resolve_table, table_cell_context};
use kiln_doc::st::{CellVAlign, HeightRule, TableAlignment, TableLayout, WidthType};
use kiln_scene::Bounds;
use kiln_text::Rgba;

use crate::cx::{BoxContent, Cx};
use crate::page::{Item, StoryKind};
use crate::source::FieldEnv;
use crate::style::{self, Rule};
use crate::units::twips;

/// Word's default left and right cell margin (0.075 in).
const DEFAULT_CELL_MARGIN: i32 = 108;
/// Width at which cells are measured for autofit (wide enough never to wrap).
const MEASURE_WIDTH: f32 = 20_000.0;

/// A laid-out cell.
#[derive(Debug, Clone)]
pub struct CellBox {
    /// The cell.
    pub id: NodeId,
    /// First row.
    pub row: usize,
    /// Rows spanned (vertical merge).
    pub row_span: usize,
    /// First grid column.
    pub col: usize,
    /// Grid columns spanned.
    pub col_span: usize,
    /// Left edge relative to the table.
    pub x: f32,
    /// Width.
    pub w: f32,
    /// Margins: top, left, bottom, right.
    pub margins: [f32; 4],
    /// Shading.
    pub fill: Option<Rgba>,
    /// Vertical alignment.
    pub valign: CellVAlign,
    /// Borders after conflict resolution: top, left, bottom, right.
    pub borders: [Option<Rule>; 4],
    /// Content (inside the margins).
    pub content: BoxContent,
}

impl CellBox {
    /// Height the content needs, margins included.
    #[must_use]
    pub fn needed(&self) -> f32 {
        self.content.height + self.margins[0] + self.margins[2]
    }
}

/// A laid-out row.
#[derive(Debug, Clone, PartialEq)]
pub struct RowBox {
    /// Height.
    pub height: f32,
    /// Must not break across pages.
    pub cant_split: bool,
    /// A repeated header row.
    pub header: bool,
    /// Indices into [`TableBox::cells`] of the cells starting in this row.
    pub cells: Vec<usize>,
}

/// A laid-out table.
#[derive(Debug, Clone)]
pub struct TableBox {
    /// The table.
    pub id: NodeId,
    /// Left edge relative to the text area.
    pub x: f32,
    /// Width.
    pub width: f32,
    /// Narrowest the table can be (autofit).
    pub min_width: f32,
    /// Column boundaries relative to the table's left edge (grid columns + 1).
    pub col_x: Vec<f32>,
    /// Rows.
    pub rows: Vec<RowBox>,
    /// Cells.
    pub cells: Vec<CellBox>,
    /// Leading header rows (repeated on every page).
    pub header_rows: usize,
}

fn width_px(w: Option<TableWidth>, of: f32) -> Option<f32> {
    let w = w?;
    match w.kind {
        WidthType::Dxa => Some(twips(w.value)).filter(|v| *v > 0.0),
        #[allow(clippy::cast_precision_loss)]
        WidthType::Pct => Some(of * (w.value as f32) / 5000.0).filter(|v| *v > 0.0),
        _ => None,
    }
}

fn margins(own: Option<CellMargins>, table: Option<CellMargins>) -> [f32; 4] {
    let pick =
        |a: Option<i32>, b: Option<i32>, d: i32| twips(a.or(b).unwrap_or(d).clamp(0, 31_680));
    let own = own.unwrap_or_default();
    let t = table.unwrap_or_default();
    [
        pick(own.top, t.top, 0),
        pick(own.left, t.left, DEFAULT_CELL_MARGIN),
        pick(own.bottom, t.bottom, 0),
        pick(own.right, t.right, DEFAULT_CELL_MARGIN),
    ]
}

/// Distributes `extra` px over `cols` in proportion to `weights` (equally when they are zero).
fn distribute(w: &mut [f32], cols: std::ops::Range<usize>, extra: f32, weights: &[f32]) {
    let total: f32 = cols.clone().map(|c| weights[c].max(0.0)).sum();
    #[allow(clippy::cast_precision_loss)]
    let n = cols.len().max(1) as f32;
    for c in cols {
        let share = if total > 0.0 {
            weights[c].max(0.0) / total
        } else {
            1.0 / n
        };
        w[c] += extra * share;
    }
}

impl Cx<'_> {
    /// Lays out a table in a text area `avail` px wide.
    pub fn table(&mut self, t: &Table, avail: f32, env: &FieldEnv, story: StoryKind) -> TableBox {
        let ctx = self.doc.context();
        let tp = resolve_table(&ctx, &t.props);
        let map = t.cell_map();
        let n = t.grid_width().max(1);
        let theme = &self.doc.theme;
        let mapping = &self.doc.settings.color_scheme_mapping;

        // Resolved cell properties, once per cell.
        let cell_props: Vec<_> = map
            .iter()
            .map(|s| resolve_cell(&ctx, t, s.row, s.col))
            .collect();
        let cell_margins: Vec<[f32; 4]> = map
            .iter()
            .zip(&cell_props)
            .map(|(_, p)| margins(p.margins, tp.cell_margins))
            .collect();

        // ----- column widths
        let mut grid: Vec<f32> = (0..n)
            .map(|c| t.grid.get(c).map_or(0.0, |g| twips((*g).max(0))))
            .collect();
        let table_pref = width_px(tp.width, avail);
        let fixed = tp.layout == Some(TableLayout::Fixed);
        let grid_known = grid.iter().all(|g| *g > 0.0);
        let mut min_w = vec![0.0_f32; n];
        let mut max_w = vec![0.0_f32; n];
        let mut pref: Vec<Option<f32>> = vec![None; n];
        let pref_base = table_pref.unwrap_or(avail);
        for (s, p) in map.iter().zip(&cell_props) {
            if s.col_span == 1
                && s.col < n
                && let Some(w) = width_px(p.width, pref_base)
            {
                pref[s.col] = Some(pref[s.col].map_or(w, |o: f32| o.max(w)));
            }
        }
        if !fixed && !grid_known {
            // Measure the content of every cell (it never wraps at this width).
            for (k, s) in map.iter().enumerate() {
                let Some(cell) = t.rows.get(s.row).and_then(|r| r.cells.get(s.cell)) else {
                    continue;
                };
                let tctx = table_cell_context(t, s.row, s.col);
                let b = self.blocks_box(&cell.blocks, MEASURE_WIDTH, Some(&tctx), story, env);
                let m = &cell_margins[k];
                let (lo, hi) = (b.min_width + m[1] + m[3], b.max_width + m[1] + m[3]);
                let end = (s.col + s.col_span).min(n);
                if s.col_span == 1 && s.col < n {
                    min_w[s.col] = min_w[s.col].max(lo);
                    max_w[s.col] = max_w[s.col].max(hi);
                } else if s.col < end {
                    let have_min: f32 = min_w[s.col..end].iter().sum();
                    if lo > have_min {
                        let weights = max_w.clone();
                        distribute(&mut min_w, s.col..end, lo - have_min, &weights);
                    }
                    let have_max: f32 = max_w[s.col..end].iter().sum();
                    if hi > have_max {
                        let weights = max_w.clone();
                        distribute(&mut max_w, s.col..end, hi - have_max, &weights);
                    }
                }
            }
        }
        for c in 0..n {
            let base = pref[c].unwrap_or(if grid[c] > 0.0 { grid[c] } else { max_w[c] });
            grid[c] = if fixed {
                base.max(1.0)
            } else {
                base.max(min_w[c]).max(1.0)
            };
        }
        let sum: f32 = grid.iter().sum();
        let target = match (fixed, table_pref) {
            (true, _) => sum,
            (false, Some(p)) => p,
            (false, None) => sum.min(avail.max(1.0)),
        };
        if !fixed && (sum - target).abs() > 0.5 {
            if sum < target {
                let weights = grid.clone();
                distribute(&mut grid, 0..n, target - sum, &weights);
            } else {
                let room: Vec<f32> = (0..n).map(|c| (grid[c] - min_w[c]).max(0.0)).collect();
                let total_room: f32 = room.iter().sum();
                let cut = (sum - target).min(total_room);
                if total_room > 0.0 {
                    for c in 0..n {
                        grid[c] -= cut * room[c] / total_room;
                    }
                }
            }
        }
        let mut col_x = Vec::with_capacity(n + 1);
        let mut x = 0.0;
        col_x.push(0.0);
        for g in &grid {
            x += g;
            col_x.push(x);
        }
        let width = x;
        let min_width: f32 = if fixed { width } else { min_w.iter().sum() };

        // ----- position
        let first_margin = cell_margins
            .first()
            .map_or(twips(DEFAULT_CELL_MARGIN), |m| m[1]);
        let indent = twips(tp.indent.unwrap_or(0));
        let outdent = if self.doc.settings.compatibility_mode < 15 {
            first_margin
        } else {
            0.0
        };
        let left = match tp.alignment.unwrap_or_default() {
            TableAlignment::Center => (avail - width) / 2.0,
            TableAlignment::Right => avail - width,
            TableAlignment::Left => indent - outdent,
        };

        // ----- borders
        let tb = tp.borders.clone().unwrap_or_default();
        let r = |b: &Option<Border>| b.as_ref().and_then(|b| style::rule(b, theme, mapping));
        let own = |k: usize, side: usize| -> Option<Option<Rule>> {
            let b = cell_props[k].borders.as_ref()?;
            let side = match side {
                0 => &b.top,
                1 => &b.left,
                2 => &b.bottom,
                _ => &b.right,
            };
            side.as_ref().map(|x| style::rule(x, theme, mapping))
        };
        let rows_n = t.rows.len();
        let default_side = |k: usize, side: usize| -> Option<Rule> {
            let s = &map[k];
            let end_row = s.row + s.row_span.max(1);
            match side {
                0 if s.row == 0 => r(&tb.top),
                0 => r(&tb.inside_h),
                2 if end_row >= rows_n => r(&tb.bottom),
                2 => r(&tb.inside_h),
                1 if s.col == 0 => r(&tb.left),
                1 => r(&tb.inside_v),
                3 if s.col + s.col_span >= n => r(&tb.right),
                _ => r(&tb.inside_v),
            }
        };
        let side_rule =
            |k: usize, side: usize| own(k, side).unwrap_or_else(|| default_side(k, side));
        // Neighbours for conflicts: the cell on the other side of each edge, from a grid of
        // which cell covers each (row, column).
        let mut occupied: Vec<Option<usize>> = vec![None; rows_n * n];
        for (k, s) in map.iter().enumerate() {
            if s.row_span == 0 {
                continue;
            }
            for row in s.row..(s.row + s.row_span).min(rows_n) {
                for col in s.col..(s.col + s.col_span).min(n) {
                    occupied[row * n + col] = Some(k);
                }
            }
        }
        let at = |row: usize, col: usize| -> Option<usize> {
            if row < rows_n && col < n {
                occupied[row * n + col]
            } else {
                None
            }
        };
        let neighbour = |k: usize, side: usize| -> Option<usize> {
            let s = &map[k];
            match side {
                0 => s.row.checked_sub(1).and_then(|r| at(r, s.col)),
                2 => at(s.row + s.row_span.max(1), s.col),
                1 => s.col.checked_sub(1).and_then(|c| at(s.row, c)),
                _ => at(s.row, s.col + s.col_span),
            }
        };

        // ----- cells
        let mut cells = Vec::new();
        let mut rows: Vec<RowBox> = t
            .rows
            .iter()
            .enumerate()
            .map(|(i, row)| RowBox {
                height: 0.0,
                cant_split: row.props.cant_split.unwrap_or(false),
                header: i < t.header_rows(),
                cells: Vec::new(),
            })
            .collect();
        for (k, s) in map.iter().enumerate() {
            if s.row_span == 0 {
                continue;
            }
            let Some(cell) = t.rows.get(s.row).and_then(|r| r.cells.get(s.cell)) else {
                continue;
            };
            let c0 = s.col.min(n);
            let c1 = (s.col + s.col_span).min(n);
            let x = col_x[c0];
            let w = (col_x[c1] - x).max(1.0);
            let m = cell_margins[k];
            let tctx = table_cell_context(t, s.row, s.col);
            let content = self.blocks_box(
                &cell.blocks,
                (w - m[1] - m[3]).max(1.0),
                Some(&tctx),
                story,
                env,
            );
            let mut borders = [None; 4];
            for (side, slot) in borders.iter_mut().enumerate() {
                let mine = side_rule(k, side);
                let theirs = neighbour(k, side).map(|o| side_rule(o, (side + 2) % 4));
                *slot = match theirs {
                    Some(t) if side == 0 || side == 1 => style::stronger(t, mine),
                    Some(t) => style::stronger(mine, t),
                    None => mine,
                };
            }
            let p = &cell_props[k];
            rows[s.row].cells.push(cells.len());
            cells.push(CellBox {
                id: s.id,
                row: s.row,
                row_span: s.row_span.max(1),
                col: s.col,
                col_span: s.col_span,
                x,
                w,
                margins: m,
                fill: style::shading_fill(p.shading.as_ref(), theme, mapping),
                valign: p.v_align.unwrap_or_default(),
                borders,
                content,
            });
        }

        // ----- row heights
        for (i, row) in t.rows.iter().enumerate() {
            let content = rows[i]
                .cells
                .iter()
                .map(|&c| &cells[c])
                .filter(|c| c.row_span == 1)
                .map(CellBox::needed)
                .fold(0.0_f32, f32::max);
            let h = row.props.height.map_or(content, |h| {
                let v = twips(h.value.clamp(0, 31_680));
                match h.rule {
                    HeightRule::Exact => v,
                    HeightRule::AtLeast => content.max(v),
                    HeightRule::Auto => content.max(v),
                }
            });
            // An empty row still shows one line of the cell's paragraph mark.
            rows[i].height = h.max(1.0);
        }
        for c in &cells {
            if c.row_span > 1 {
                let end = (c.row + c.row_span).min(rows.len());
                let have: f32 = rows[c.row..end].iter().map(|r| r.height).sum();
                let need = c.needed();
                if need > have
                    && let Some(last) = rows.get_mut(end - 1)
                {
                    last.height += need - have;
                }
            }
        }

        TableBox {
            id: t.id,
            x: left,
            width,
            min_width,
            col_x,
            rows,
            cells,
            header_rows: t.header_rows(),
        }
    }
}

impl TableBox {
    /// Height of all rows.
    #[must_use]
    pub fn height(&self) -> f32 {
        self.rows.iter().map(|r| r.height).sum()
    }

    /// Paints the cells starting in row `r` (whose rows all follow without a break) with the
    /// table's left edge at `ox` and row `r`'s top at `oy`.
    pub fn paint_row(&self, r: usize, ox: f32, oy: f32, _h: f32, out: &mut Vec<Item>) {
        let Some(row) = self.rows.get(r) else { return };
        for &k in &row.cells {
            let c = &self.cells[k];
            let end = (c.row + c.row_span).min(self.rows.len());
            let h: f32 = self.rows[c.row..end].iter().map(|r| r.height).sum();
            let rect = Bounds::new(ox + c.x, oy, c.w, h);
            cell_background(c, rect, out);
            let dy = valign_offset(c, h);
            for band in &c.content.bands {
                for item in &band.items {
                    let mut it = item.clone();
                    it.translate(rect.x + c.margins[1], rect.y + c.margins[0] + dy);
                    it.set_fill(c.fill);
                    out.push(it);
                }
            }
            cell_borders(c, rect, true, true, out);
        }
    }
}

/// Offset of the content for the cell's vertical alignment in a segment `h` tall.
#[must_use]
pub fn valign_offset(c: &CellBox, h: f32) -> f32 {
    let free = (h - c.needed()).max(0.0);
    match c.valign {
        CellVAlign::Center => free / 2.0,
        CellVAlign::Bottom => free,
        CellVAlign::Top => 0.0,
    }
}

/// The cell's shading over `rect`.
pub fn cell_background(c: &CellBox, rect: Bounds, out: &mut Vec<Item>) {
    if let Some(fill) = c.fill {
        out.push(Item::Fill { rect, color: fill });
    }
}

/// The cell's borders around `rect` (`top`/`bottom`: whether this segment draws them).
pub fn cell_borders(c: &CellBox, rect: Bounds, top: bool, bottom: bool, out: &mut Vec<Item>) {
    let [bt, bl, bb, br] = c.borders;
    let (x0, y0, x1, y1) = (rect.x, rect.y, rect.right(), rect.bottom());
    if let (true, Some(rule)) = (top, bt) {
        out.push(Item::Rule {
            x0,
            y0,
            x1,
            y1: y0,
            rule,
        });
    }
    if let (true, Some(rule)) = (bottom, bb) {
        out.push(Item::Rule {
            x0,
            y0: y1,
            x1,
            y1,
            rule,
        });
    }
    if let Some(rule) = bl {
        out.push(Item::Rule {
            x0,
            y0,
            x1: x0,
            y1,
            rule,
        });
    }
    if let Some(rule) = br {
        out.push(Item::Rule {
            x0: x1,
            y0,
            x1,
            y1,
            rule,
        });
    }
}
