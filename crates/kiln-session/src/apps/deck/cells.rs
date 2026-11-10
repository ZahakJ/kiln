// SPDX-License-Identifier: AGPL-3.0-or-later
//! Table Layout on a range of cells: selecting cells (drag across them, Shift+click), merging
//! and splitting, and cell margins. Formatting commands of the Table tabs act on the selected
//! cells, or on the cell with the caret.

use std::collections::BTreeMap;

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::ops::{Op, TextTarget, Transaction};
use kiln_deck::{ShapeKind, Table, TableCell, TableRow, TextBody};
use kiln_protocol::commands::DeckCommand;
use kiln_protocol::commands::deck_content::DeckCellRange;

use super::DeckDoc;
use crate::model::Change;

/// EMU per point.
const EMU_PER_PT: f32 = 12_700.0;

/// The range spanning two cells (and any merged cell either one covers).
fn span(t: &Table, a: (usize, usize), b: (usize, usize)) -> DeckCellRange {
    let (mut r0, mut c0) = (a.0.min(b.0), a.1.min(b.1));
    let (mut r1, mut c1) = (a.0.max(b.0), a.1.max(b.1));
    // Grow until no merged cell crosses the edge.
    loop {
        let (mut g0, mut h0, mut g1, mut h1) = (r0, c0, r1, c1);
        for r in r0..=r1 {
            for c in c0..=c1 {
                let Some(cell) = t.cell(r, c) else { continue };
                if cell.h_merge || cell.v_merge {
                    let (or, oc) = origin(t, r, c);
                    g0 = g0.min(or);
                    h0 = h0.min(oc);
                    continue;
                }
                g1 = g1.max(r + usize::try_from(cell.row_span.max(1)).unwrap_or(1) - 1);
                h1 = h1.max(c + usize::try_from(cell.grid_span.max(1)).unwrap_or(1) - 1);
            }
        }
        if (g0, h0, g1, h1) == (r0, c0, r1, c1) {
            break;
        }
        (r0, c0, r1, c1) = (g0, h0, g1, h1);
    }
    let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
    DeckCellRange {
        row: n(r0),
        col: n(c0),
        rows: n(r1 - r0 + 1),
        cols: n(c1 - c0 + 1),
    }
}

/// The cell whose merge covers `(r, c)`.
fn origin(t: &Table, mut r: usize, mut c: usize) -> (usize, usize) {
    while c > 0 && t.cell(r, c).is_some_and(|x| x.h_merge) {
        c -= 1;
    }
    while r > 0 && t.cell(r, c).is_some_and(|x| x.v_merge) {
        r -= 1;
    }
    (r, c)
}

/// The text of merged cells: every non-empty cell's paragraphs in reading order.
fn merged_text(bodies: Vec<TextBody>) -> TextBody {
    let first = bodies.first().cloned().unwrap_or_default();
    let full: Vec<TextBody> = bodies.into_iter().filter(|b| !b.is_empty()).collect();
    if full.len() < 2 {
        return full.into_iter().next().unwrap_or(first);
    }
    let mut paragraphs = Vec::new();
    let mut props = BTreeMap::new();
    for b in &full {
        for p in b.paragraphs() {
            let mut p = p.clone();
            let id = NodeId(u64::try_from(paragraphs.len() + 1).unwrap_or(u64::MAX));
            if let Some(extra) = b.deck_props(p.id) {
                props.insert(id, extra.clone());
            }
            p.id = id;
            paragraphs.push(p);
        }
    }
    match TextBody::from_paragraphs(paragraphs, props) {
        Ok(mut t) => {
            t.body = full[0].body.clone();
            t.list_style = full[0].list_style.clone();
            t
        }
        Err(_) => full.into_iter().next().unwrap_or(first),
    }
}

/// Merges a rectangle of cells into its top-left cell, which keeps all of their text.
pub(super) fn merge_cells(t: &mut Table, r0: usize, c0: usize, nr: usize, nc: usize) {
    let mut bodies = Vec::new();
    for r in r0..r0 + nr {
        for c in c0..c0 + nc {
            if let Some(cell) = t.cell(r, c) {
                bodies.push(cell.text.clone());
            }
        }
    }
    let text = merged_text(bodies);
    for r in r0..r0 + nr {
        for c in c0..c0 + nc {
            let Some(cell) = t.cell_mut(r, c) else {
                continue;
            };
            cell.grid_span = 1;
            cell.row_span = 1;
            cell.h_merge = c > c0;
            cell.v_merge = r > r0;
            if r > r0 || c > c0 {
                cell.text = TextBody::new();
            }
        }
    }
    if let Some(cell) = t.cell_mut(r0, c0) {
        cell.grid_span = u32::try_from(nc).unwrap_or(1);
        cell.row_span = u32::try_from(nr).unwrap_or(1);
        cell.text = text;
    }
}

/// Splits a cell: a merged cell back into its cells, any other into `rows` × `cols` cells
/// (new grid rows and columns, which the cells beside it span).
pub(super) fn split_cell(t: &mut Table, r0: usize, c0: usize, rows: usize, cols: usize) {
    let Some(cell) = t.cell(r0, c0).cloned() else {
        return;
    };
    let (gs, rs) = (
        usize::try_from(cell.grid_span.max(1)).unwrap_or(1),
        usize::try_from(cell.row_span.max(1)).unwrap_or(1),
    );
    if gs > 1 || rs > 1 {
        for r in r0..r0 + rs {
            for c in c0..c0 + gs {
                if let Some(x) = t.cell_mut(r, c) {
                    x.grid_span = 1;
                    x.row_span = 1;
                    x.h_merge = false;
                    x.v_merge = false;
                }
            }
        }
        return;
    }
    let blank = |from: &TableCell| TableCell {
        props: from.props.clone(),
        ..TableCell::new()
    };
    if cols > 1 {
        let n = i64::try_from(cols).unwrap_or(1);
        let w = t.grid.get(c0).copied().unwrap_or(1_219_200);
        if let Some(g) = t.grid.get_mut(c0) {
            *g = w / n;
        }
        for _ in 1..cols {
            t.grid.insert(c0 + 1, w / n);
        }
        for r in 0..t.rows.len() {
            let Some(at) = t.cell(r, c0).cloned() else {
                continue;
            };
            let extra = if r == r0 {
                blank(&at)
            } else {
                let (orow, ocol) = if at.h_merge {
                    origin(t, r, c0)
                } else {
                    (r, c0)
                };
                if !at.v_merge
                    && let Some(o) = t.cell_mut(orow, ocol)
                {
                    o.grid_span += u32::try_from(cols - 1).unwrap_or(0);
                }
                TableCell {
                    h_merge: true,
                    v_merge: at.v_merge,
                    props: at.props.clone(),
                    ..TableCell::new()
                }
            };
            let cells = &mut t.rows[r].cells;
            for _ in 1..cols {
                cells.insert((c0 + 1).min(cells.len()), extra.clone());
            }
        }
    }
    if rows > 1 {
        let n = i64::try_from(rows).unwrap_or(1);
        let h = t.rows[r0].height;
        t.rows[r0].height = h / n;
        let width = t.grid.len();
        let mut row = Vec::with_capacity(width);
        for c in 0..width {
            let Some(at) = t.cell(r0, c).cloned() else {
                row.push(TableCell::new());
                continue;
            };
            if c >= c0 && c < c0 + cols {
                row.push(blank(&at));
                continue;
            }
            if !at.h_merge {
                let (orow, ocol) = if at.v_merge {
                    origin(t, r0, c)
                } else {
                    (r0, c)
                };
                if let Some(o) = t.cell_mut(orow, ocol) {
                    o.row_span += u32::try_from(rows - 1).unwrap_or(0);
                }
            }
            row.push(TableCell {
                h_merge: at.h_merge,
                v_merge: true,
                props: at.props.clone(),
                ..TableCell::new()
            });
        }
        for _ in 1..rows {
            t.rows.insert(
                r0 + 1,
                TableRow {
                    height: h / n,
                    cells: row.clone(),
                },
            );
        }
    }
}

impl DeckDoc {
    /// The selected cells of the table being edited, `(table, range)`.
    pub(crate) fn cell_range(&self) -> Option<(NodeId, DeckCellRange)> {
        let (id, r) = self.view.cells?;
        let editing = self
            .view
            .text
            .as_ref()
            .is_some_and(|t| matches!(t.target, TextTarget::Cell { shape, .. } if shape == id));
        if !editing {
            return None;
        }
        let Some(ShapeKind::Table(t)) = self.pres.shape(id).map(|s| &s.kind) else {
            return None;
        };
        let n = |v: u32| usize::try_from(v).unwrap_or(0);
        let fits = n(r.row) + n(r.rows) <= t.rows.len() && n(r.col) + n(r.cols) <= t.grid.len();
        (fits && r.rows * r.cols > 1).then_some((id, r))
    }

    /// Selects the cells from the caret's cell to `(row, col)` (a drag across cells, a
    /// Shift+click); the caret's own cell alone clears the cell selection.
    pub(crate) fn extend_cells(&mut self, anchor: (usize, usize), to: (usize, usize)) {
        let Some(t) = &self.view.text else { return };
        let TextTarget::Cell { shape, .. } = t.target else {
            return;
        };
        let Some(ShapeKind::Table(table)) = self.pres.shape(shape).map(|s| &s.kind) else {
            return;
        };
        let r = span(table, anchor, to);
        let cells = (r.rows * r.cols > 1).then_some((shape, r));
        if self.view.cells != cells {
            self.view.cells = cells;
            self.epoch += 1;
        }
    }

    /// The selected cells, shaded on the stage.
    pub(crate) fn cells_overlay(&self, ops: &mut Vec<kiln_scene::DrawOp>) {
        let Some((id, r)) = self.cell_range() else {
            return;
        };
        let Some(tb) = self.renderer.borrow_mut().table_layout(&self.pres, id) else {
            return;
        };
        let Some(xfrm) = self.xfrm_of(id) else { return };
        let m = kiln_deck_layout::geom::shape_matrix(&xfrm);
        let n = |v: u32| usize::try_from(v).unwrap_or(0);
        let (Some(&x0), Some(&x1), Some(&y0), Some(&y1)) = (
            tb.xs.get(n(r.col)),
            tb.xs.get(n(r.col + r.cols)),
            tb.ys.get(n(r.row)),
            tb.ys.get(n(r.row + r.rows)),
        ) else {
            return;
        };
        #[allow(clippy::cast_possible_truncation)]
        let pt = |x: f32, y: f32| {
            let p = m * kurbo::Point::new(f64::from(x), f64::from(y));
            (p.x as f32, p.y as f32)
        };
        ops.push(kiln_scene::DrawOp::Path {
            path: super::view::quad(&[pt(x0, y0), pt(x1, y0), pt(x1, y1), pt(x0, y1)]),
            fill: Some(kiln_scene::Paint::Role(kiln_scene::ColorRole::Selection)),
            stroke: None,
            rule: kiln_scene::FillRule::NonZero,
        });
    }

    /// Table Layout ▸ select cells and Cell Margins.
    pub(crate) fn cells_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        let label = cmd.meta().id;
        match cmd {
            DeckCommand::SelectCells { range } => {
                let (id, row, col) = self
                    .table_cell()
                    .ok_or_else(|| KilnError::unavailable(label, "select a table first"))?;
                let Some(ShapeKind::Table(t)) = self.pres.shape(id).map(|s| &s.kind) else {
                    return Ok(Change::None);
                };
                let n = |v: u32| usize::try_from(v).unwrap_or(0);
                // No rows and no columns: the whole table (Table Layout ▸ Select ▸ Select Table).
                let all = range.rows == 0 && range.cols == 0;
                let range = if all {
                    DeckCellRange {
                        row: 0,
                        col: 0,
                        rows: u32::try_from(t.rows.len()).unwrap_or(u32::MAX),
                        cols: u32::try_from(t.grid.len()).unwrap_or(u32::MAX),
                    }
                } else {
                    *range
                };
                if range.rows == 0
                    || range.cols == 0
                    || n(range.row) + n(range.rows) > t.rows.len()
                    || n(range.col) + n(range.cols) > t.grid.len()
                {
                    return Err(KilnError::InvalidInput(
                        "those cells are not in the table".into(),
                    ));
                }
                let (r, c) = (n(range.row), n(range.col));
                let target = TextTarget::Cell {
                    shape: id,
                    row: r,
                    col: c,
                };
                if self.view.text.as_ref().map(|t| t.target) != Some(target) || (row, col) != (r, c)
                {
                    self.begin_text(target, None, false)?;
                }
                let far = (r + n(range.rows) - 1, c + n(range.cols) - 1);
                self.extend_cells((r, c), far);
                Ok(Change::Selection)
            }
            DeckCommand::CellMargins {
                left,
                top,
                right,
                bottom,
            } => {
                let (id, row, col) = self
                    .table_cell()
                    .ok_or_else(|| KilnError::unavailable(label, "select a table first"))?;
                let Some(ShapeKind::Table(old)) = self.pres.shape(id).map(|s| s.kind.clone())
                else {
                    return Ok(Change::None);
                };
                // A selected table without a caret: every cell, as PowerPoint.
                let editing = self.view.text.is_some();
                let range = self.cell_range().map(|(_, r)| r);
                let mut t = old.clone();
                #[allow(clippy::cast_possible_truncation)]
                let emu = |pt: f32| Some((pt.clamp(0.0, 400.0) * EMU_PER_PT).round() as i64);
                let margins = [emu(*left), emu(*top), emu(*right), emu(*bottom)];
                for (r, tr) in t.rows.iter_mut().enumerate() {
                    for (c, cell) in tr.cells.iter_mut().enumerate() {
                        let n = |v: u32| usize::try_from(v).unwrap_or(0);
                        let hit = match range {
                            Some(g) => {
                                r >= n(g.row)
                                    && r < n(g.row + g.rows)
                                    && c >= n(g.col)
                                    && c < n(g.col + g.cols)
                            }
                            None => !editing || (r, c) == (row, col),
                        };
                        if hit {
                            cell.props.margins = margins;
                        }
                    }
                }
                if t == old {
                    return Ok(Change::None);
                }
                self.apply(
                    label,
                    Transaction::new(vec![Op::SetShapeKind {
                        shape: id,
                        kind: Box::new(ShapeKind::Table(t)),
                    }]),
                )?;
                Ok(Change::Content)
            }
            _ => Ok(Change::None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(rows: usize, cols: usize) -> Table {
        let mut t = Table::with_size(rows, cols, 1_200_000, 400_000);
        for r in 0..rows {
            for c in 0..cols {
                if let Some(cell) = t.cell_mut(r, c) {
                    cell.text = TextBody::from_text(&format!("{r}{c}"));
                }
            }
        }
        t
    }

    #[test]
    fn merging_keeps_the_text_and_splitting_undoes_it() {
        let mut t = table(3, 3);
        merge_cells(&mut t, 0, 0, 2, 2);
        let o = t.cell(0, 0).expect("origin");
        assert_eq!((o.grid_span, o.row_span), (2, 2));
        assert_eq!(o.text.text(), "00\n01\n10\n11");
        assert!(t.cell(0, 1).expect("c").h_merge);
        let covered = t.cell(1, 1).expect("c");
        assert!(covered.h_merge && covered.v_merge);
        split_cell(&mut t, 0, 0, 1, 1);
        assert!(
            t.rows
                .iter()
                .flat_map(|r| &r.cells)
                .all(|c| { c.grid_span == 1 && c.row_span == 1 && !c.h_merge && !c.v_merge })
        );
    }

    #[test]
    fn splitting_a_cell_adds_grid_columns_the_others_span() {
        let mut t = table(2, 2);
        split_cell(&mut t, 0, 0, 2, 2);
        assert_eq!((t.grid.len(), t.rows.len()), (3, 3));
        // The cell beside the split one spans the new row; the one below spans the new column.
        assert_eq!(t.cell(0, 2).expect("c").row_span, 2);
        assert!(t.cell(1, 2).expect("c").v_merge);
        assert_eq!(t.cell(2, 0).expect("c").grid_span, 2);
        assert!(t.cell(2, 1).expect("c").h_merge);
        assert_eq!(t.grid.iter().sum::<i64>(), 1_200_000);
    }

    #[test]
    fn a_range_grows_over_merged_cells() {
        let mut t = table(3, 3);
        merge_cells(&mut t, 0, 1, 2, 2);
        let r = span(&t, (1, 0), (1, 1));
        assert_eq!((r.row, r.col, r.rows, r.cols), (0, 0, 2, 3));
    }
}
