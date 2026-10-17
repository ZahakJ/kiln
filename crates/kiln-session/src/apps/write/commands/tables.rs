// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tables: insert (grid picker, dialog, AutoFit), rows and columns, delete, merge and split
//! cells, distribute, alignment, header rows, borders and shading, Word's table styles and style
//! options, convert to and from text, split table, select, cell size, and Tab navigation (a new
//! row after the last cell, list levels and tab characters outside tables).

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::st::{
    BorderStyle, CellVAlign, HeightRule, StyleType, TableAlignment, TableCondition, TableLayout,
    ThemeColorName, VMerge, WidthType,
};
use kiln_doc::style::TableStyleCondition;
use kiln_doc::{
    Block, Border, Cell, CellBorders, DocColor, EditKind, Op, Paragraph, Pos, Range, Row, RunProps,
    Shading, Style, Table, TableBorders, TableCellProps, TableLook, TableProps, TableRowProps,
    TableWidth, Transaction, edit,
};
use kiln_protocol::commands::{
    BorderPreset, CellVerticalAlign, TableAutofit, TableDeleteTarget, TableSelectTarget, TextAlign,
    TextSeparator,
};

use super::doc_err;
use super::format::doc_color;
use crate::apps::write::WriteDoc;

/// Where the caret is in a table.
#[derive(Debug, Clone)]
pub(super) struct TableAt {
    /// The table.
    pub table: Table,
    /// Its row index.
    pub row: usize,
    /// The cell index within the row.
    pub cell: usize,
    /// The grid column.
    pub col: usize,
}

/// A rectangle of the grid: rows `r0..=r1`, grid columns `c0..=c1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Rect {
    r0: usize,
    r1: usize,
    c0: usize,
    c1: usize,
}

fn border(style: BorderStyle, eighths: u32, color: DocColor) -> Border {
    Border {
        space: Some(0),
        color: Some(color),
        ..Border::new(style, eighths)
    }
}

fn all_borders(b: &Border) -> TableBorders {
    TableBorders {
        top: Some(b.clone()),
        left: Some(b.clone()),
        bottom: Some(b.clone()),
        right: Some(b.clone()),
        inside_h: Some(b.clone()),
        inside_v: Some(b.clone()),
    }
}

/// Word's built-in table styles offered by the gallery.
fn builtin_table_style(id: &str) -> Option<Style> {
    let accent = DocColor::theme(0x4472C4, ThemeColorName::Accent1);
    let light = DocColor::theme(0xD9E2F3, ThemeColorName::Accent1).with_tint(0x33);
    let grey = DocColor::rgb(0xBFBFBF);
    let base = |name: &str, priority: u32| Style {
        based_on: Some("TableNormal".into()),
        ui_priority: Some(priority),
        ..Style::new(id, StyleType::Table, name)
    };
    let bold = || RunProps {
        bold: Some(true),
        bold_cs: Some(true),
        ..RunProps::default()
    };
    let fill = |c: &DocColor| TableCellProps {
        shading: Some(Shading::fill(c.clone())),
        ..TableCellProps::default()
    };
    let mut s = match id {
        "TableGrid" => {
            let mut s = base("Table Grid", 39);
            s.tbl_pr = Some(TableProps {
                borders: Some(all_borders(&border(
                    BorderStyle::Single,
                    4,
                    DocColor::auto(),
                ))),
                ..TableProps::default()
            });
            s
        }
        "PlainTable1" => {
            let mut s = base("Plain Table 1", 41);
            s.tbl_pr = Some(TableProps {
                borders: Some(all_borders(&border(BorderStyle::Single, 4, grey.clone()))),
                ..TableProps::default()
            });
            s.conditions.insert(
                TableCondition::FirstRow,
                TableStyleCondition {
                    rpr: Some(bold()),
                    ..TableStyleCondition::default()
                },
            );
            s.conditions.insert(
                TableCondition::Band1Horz,
                TableStyleCondition {
                    tc_pr: Some(fill(&DocColor::rgb(0xF2F2F2))),
                    ..TableStyleCondition::default()
                },
            );
            s
        }
        "GridTable4-Accent1" => {
            let mut s = base("Grid Table 4 Accent 1", 49);
            s.tbl_pr = Some(TableProps {
                borders: Some(all_borders(&border(BorderStyle::Single, 4, accent.clone()))),
                ..TableProps::default()
            });
            s.conditions.insert(
                TableCondition::FirstRow,
                TableStyleCondition {
                    rpr: Some(RunProps {
                        color: Some(DocColor::theme(0xFFFFFF, ThemeColorName::Background1)),
                        ..bold()
                    }),
                    tc_pr: Some(fill(&accent)),
                    ..TableStyleCondition::default()
                },
            );
            s.conditions.insert(
                TableCondition::LastRow,
                TableStyleCondition {
                    rpr: Some(bold()),
                    ..TableStyleCondition::default()
                },
            );
            s.conditions.insert(
                TableCondition::FirstCol,
                TableStyleCondition {
                    rpr: Some(bold()),
                    ..TableStyleCondition::default()
                },
            );
            s.conditions.insert(
                TableCondition::Band1Horz,
                TableStyleCondition {
                    tc_pr: Some(fill(&light)),
                    ..TableStyleCondition::default()
                },
            );
            s
        }
        "ListTable3-Accent1" => {
            let mut s = base("List Table 3 Accent 1", 48);
            let line = border(BorderStyle::Single, 4, accent.clone());
            s.tbl_pr = Some(TableProps {
                borders: Some(TableBorders {
                    top: Some(line.clone()),
                    left: Some(line.clone()),
                    bottom: Some(line.clone()),
                    right: Some(line),
                    inside_h: None,
                    inside_v: None,
                }),
                ..TableProps::default()
            });
            s.conditions.insert(
                TableCondition::FirstRow,
                TableStyleCondition {
                    rpr: Some(RunProps {
                        color: Some(DocColor::theme(0xFFFFFF, ThemeColorName::Background1)),
                        ..bold()
                    }),
                    tc_pr: Some(fill(&accent)),
                    ..TableStyleCondition::default()
                },
            );
            s.conditions.insert(
                TableCondition::Band1Horz,
                TableStyleCondition {
                    tc_pr: Some(TableCellProps {
                        borders: Some(CellBorders {
                            top: Some(border(BorderStyle::Single, 4, accent.clone())),
                            bottom: Some(border(BorderStyle::Single, 4, accent)),
                            ..CellBorders::default()
                        }),
                        ..TableCellProps::default()
                    }),
                    ..TableStyleCondition::default()
                },
            );
            s
        }
        _ => return None,
    };
    s.ppr = Some(kiln_doc::ParagraphProps {
        spacing: Some(kiln_doc::Spacing {
            after: Some(0),
            line: Some(240),
            line_rule: Some(kiln_doc::st::LineRule::Auto),
            ..kiln_doc::Spacing::default()
        }),
        ..kiln_doc::ParagraphProps::default()
    });
    Some(s)
}

/// The gallery's names for Word's table style ids.
fn gallery_style(id: &str) -> &str {
    match id {
        "Plain" => "PlainTable1",
        "Grid" => "TableGrid",
        "Accent" => "GridTable4-Accent1",
        "Banded" => "ListTable3-Accent1",
        other => other,
    }
}

fn first_para(cell: &Cell) -> Option<NodeId> {
    cell.blocks.iter().find_map(|b| match b {
        Block::Paragraph(p) => Some(p.id),
        _ => None,
    })
}

fn last_para(cell: &Cell) -> Option<(NodeId, usize)> {
    cell.blocks.iter().rev().find_map(|b| match b {
        Block::Paragraph(p) => Some((p.id, p.content.len())),
        _ => None,
    })
}

fn empty_cell(id: NodeId, para: NodeId, width: i32, span: u32) -> Cell {
    Cell {
        id,
        props: TableCellProps {
            width: Some(TableWidth::dxa(width)),
            ..TableCellProps::default()
        },
        grid_span: span.max(1),
        v_merge: None,
        blocks: vec![Block::Paragraph(Paragraph::new(para))],
    }
}

/// The plain text of a cell (paragraphs joined by `\n`).
fn cell_text(cell: &Cell) -> String {
    let mut parts = Vec::new();
    for b in &cell.blocks {
        if let Block::Paragraph(p) = b {
            parts.push(
                p.content
                    .plain_text(true)
                    .replace(kiln_doc::OBJECT_CHAR, ""),
            );
        }
    }
    parts.join("\n")
}

impl WriteDoc {
    /// The table cell of a paragraph.
    pub(super) fn table_of(&self, para: NodeId) -> Option<TableAt> {
        let (table, row, col) = self.doc.enclosing_cell(para)?;
        let cell = table
            .cell_map()
            .into_iter()
            .find(|s| s.row == row && s.col == col)?
            .cell;
        Some(TableAt {
            table: table.clone(),
            row,
            cell,
            col,
        })
    }

    fn table_here(&self) -> Result<TableAt> {
        let (s, _) = self.span();
        self.table_of(s.para)
            .ok_or_else(|| KilnError::InvalidInput("the selection is not in a table".into()))
    }

    /// The grid rectangle the selection covers in the caret's table.
    fn cell_rect(&self, at: &TableAt) -> Rect {
        let (s, e) = self.span();
        let map = at.table.cell_map();
        let slot_of = |p: NodeId| -> Option<(usize, usize, usize)> {
            let t = self.table_of(p)?;
            (t.table.id == at.table.id).then_some(())?;
            let s = map.iter().find(|x| x.row == t.row && x.cell == t.cell)?;
            Some((t.row, s.col, s.col + s.col_span - 1))
        };
        let a = slot_of(s.para).unwrap_or((at.row, at.col, at.col));
        let b = slot_of(e.para).unwrap_or(a);
        Rect {
            r0: a.0.min(b.0),
            r1: a.0.max(b.0),
            c0: a.1.min(b.1),
            c1: a.2.max(b.2),
        }
    }

    /// Cells (row, cell index) overlapping a rectangle.
    fn cells_in(table: &Table, r: Rect) -> Vec<(usize, usize)> {
        table
            .cell_map()
            .into_iter()
            .filter(|s| {
                s.row >= r.r0 && s.row <= r.r1 && s.col <= r.c1 && s.col + s.col_span > r.c0
            })
            .map(|s| (s.row, s.cell))
            .collect()
    }

    fn put_caret_in_cell(&mut self, cell: &Cell, select: bool) {
        if let Some(first) = first_para(cell) {
            let start = Pos::new(first, 0);
            let end = if select {
                last_para(cell).map_or(start, |(id, len)| Pos::new(id, len))
            } else {
                start
            };
            self.set_selection(start, end);
        }
    }

    /// Inserts a table at the caret.
    pub(super) fn insert_table(
        &mut self,
        rows: u32,
        cols: u32,
        autofit: TableAutofit,
    ) -> Result<()> {
        if rows == 0 || cols == 0 || rows > 32_767 || cols > 63 {
            return Err(KilnError::InvalidInput(format!(
                "a table has 1–32,767 rows and 1–63 columns, not {rows} × {cols}"
            )));
        }
        self.ensure_style("TableGrid")?;
        self.delete_selection()?;
        let at = self.head;
        let mut tx =
            edit::insert_table(&mut self.doc, at, rows as usize, cols as usize).map_err(doc_err)?;
        if autofit != TableAutofit::Fixed {
            for op in &mut tx.ops {
                if let Op::InsertBlocks { blocks, .. } = op {
                    for b in blocks {
                        if let Block::Table(t) = b {
                            t.props.layout = Some(TableLayout::Autofit);
                            if autofit == TableAutofit::Window {
                                t.props.width = Some(TableWidth::pct(5000));
                            }
                        }
                    }
                }
            }
        }
        self.commit(tx)
    }

    /// Insert Above/Below: as many rows as the selection covers.
    pub(super) fn insert_rows(&mut self, below: bool) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let count = r.r1 - r.r0 + 1;
        let anchor_row = at.table.rows[if below { r.r1 } else { r.r0 }].id;
        let mut batch = self.batch(EditKind::Structure);
        for _ in 0..count {
            let tx = edit::insert_row(&mut self.doc, anchor_row, below).map_err(doc_err)?;
            self.batch_apply(&mut batch, tx)?;
        }
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Insert Left/Right: as many columns as the selection covers.
    pub(super) fn insert_columns(&mut self, right: bool) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let count = r.c1 - r.c0 + 1;
        let mut batch = self.batch(EditKind::Structure);
        for i in 0..count {
            let col = if right { r.c1 + i } else { r.c0 };
            let tx =
                edit::insert_column(&mut self.doc, at.table.id, col, right).map_err(doc_err)?;
            self.batch_apply(&mut batch, tx)?;
        }
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Delete ▸ Rows, Columns, Table or Cells (contents).
    pub(super) fn table_delete(&mut self, what: TableDeleteTarget) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let rows = at.table.rows.len();
        let cols = at.table.grid_width();
        let whole = match what {
            TableDeleteTarget::Table => true,
            TableDeleteTarget::Rows => r.r1 - r.r0 + 1 >= rows,
            TableDeleteTarget::Columns => r.c1 - r.c0 + 1 >= cols,
            TableDeleteTarget::CellContents => false,
        };
        if whole {
            return self.delete_table(&at.table);
        }
        let mut batch = self.batch(EditKind::Structure);
        match what {
            TableDeleteTarget::Rows => {
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::RemoveRows {
                        table: at.table.id,
                        index: r.r0,
                        count: r.r1 - r.r0 + 1,
                    }]),
                )?;
                let t = self
                    .doc
                    .table(at.table.id)
                    .cloned()
                    .ok_or_else(|| KilnError::Internal("the table vanished".into()))?;
                let row = &t.rows[r.r0.min(t.rows.len() - 1)];
                if let Some(c) = row.cells.first() {
                    let after = first_para(c).map(|p| Range::caret(Pos::new(p, 0)));
                    self.batch_commit(batch, after.unwrap_or(Range::new(self.anchor, self.head)));
                    return Ok(());
                }
            }
            TableDeleteTarget::Columns => {
                for _ in r.c0..=r.c1 {
                    let tx = edit::delete_column(&self.doc, at.table.id, r.c0).map_err(doc_err)?;
                    self.batch_apply(&mut batch, tx)?;
                }
                let t = self
                    .doc
                    .table(at.table.id)
                    .cloned()
                    .ok_or_else(|| KilnError::Internal("the table vanished".into()))?;
                let row = &t.rows[at.row.min(t.rows.len() - 1)];
                let c = row
                    .cells
                    .get(at.cell.min(row.cells.len() - 1))
                    .or(row.cells.first());
                if let Some(c) = c {
                    let after = first_para(c).map(|p| Range::caret(Pos::new(p, 0)));
                    self.batch_commit(batch, after.unwrap_or(Range::new(self.anchor, self.head)));
                    return Ok(());
                }
            }
            TableDeleteTarget::CellContents => {
                for (ri, ci) in Self::cells_in(&at.table, r) {
                    let cell = &at.table.rows[ri].cells[ci];
                    for b in &cell.blocks {
                        if let Block::Paragraph(p) = b
                            && !p.content.is_empty()
                        {
                            self.batch_apply(
                                &mut batch,
                                Transaction::new(vec![Op::DeleteInline {
                                    para: p.id,
                                    offset: 0,
                                    len: p.content.len(),
                                }]),
                            )?;
                        }
                    }
                }
                let caret = Pos::new(self.span().0.para, 0);
                self.batch_commit(batch, Range::caret(caret));
                return Ok(());
            }
            TableDeleteTarget::Table => {}
        }
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Removes a table; the caret goes to the paragraph after it (a new one when there is none).
    fn delete_table(&mut self, table: &Table) -> Result<()> {
        let (container, index) = self
            .doc
            .parent_of(table.id)
            .ok_or_else(|| KilnError::Internal("a table without a container".into()))?;
        let blocks = self.doc.container(container).map_err(doc_err)?;
        let next = blocks.get(index + 1).and_then(|b| match b {
            Block::Paragraph(p) => Some(p.id),
            _ => None,
        });
        let mut batch = self.batch(EditKind::Structure);
        let target = match next {
            Some(p) => p,
            None => {
                let p = Paragraph::new(self.doc.alloc_id());
                let id = p.id;
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::InsertBlocks {
                        container,
                        index: index + 1,
                        blocks: vec![Block::Paragraph(p)],
                    }]),
                )?;
                id
            }
        };
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::RemoveBlocks {
                container,
                index,
                count: 1,
            }]),
        )?;
        self.batch_commit(batch, Range::caret(Pos::new(target, 0)));
        Ok(())
    }

    /// Merge Cells: the selected rectangle becomes one cell (horizontal spans, then vertical
    /// merges); the cells' paragraphs are kept, in reading order, in the merged cell.
    pub(super) fn merge_cells(&mut self) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        if r.r0 == r.r1 && r.c0 == r.c1 {
            return Ok(());
        }
        let map = at.table.cell_map();
        // Every row must start and end a cell at the rectangle's edges.
        for row in r.r0..=r.r1 {
            let starts = map.iter().any(|s| s.row == row && s.col == r.c0);
            let ends = map
                .iter()
                .any(|s| s.row == row && s.col + s.col_span - 1 == r.c1);
            if !starts || !ends {
                return Err(KilnError::InvalidInput(
                    "only a rectangular block of cells can be merged".into(),
                ));
            }
        }
        let mut batch = self.batch(EditKind::Structure);
        let mut top_cell = None;
        let mut moved: Vec<Block> = Vec::new();
        for row in r.r0..=r.r1 {
            let slots: Vec<_> = map
                .iter()
                .filter(|s| s.row == row && s.col >= r.c0 && s.col <= r.c1)
                .copied()
                .collect();
            let Some(first) = slots.first().copied() else {
                continue;
            };
            let row_ref = &at.table.rows[row];
            let keep = &row_ref.cells[first.cell];
            let span: usize = slots.iter().map(|s| s.col_span).sum();
            let mut ops = Vec::new();
            for s in slots.iter().skip(1) {
                let c = &row_ref.cells[s.cell];
                moved.extend(c.blocks.iter().filter(|b| !is_empty_para(b)).cloned());
            }
            if slots.len() > 1 {
                ops.push(Op::RemoveCells {
                    row: row_ref.id,
                    index: first.cell + 1,
                    count: slots.len() - 1,
                });
            }
            let v_merge = if r.r0 == r.r1 {
                None
            } else if row == r.r0 {
                Some(VMerge::Restart)
            } else {
                Some(VMerge::Continue)
            };
            let mut props = keep.props.clone();
            props.width = Some(TableWidth::dxa(
                at.table.grid[r.c0.min(at.table.grid.len())..=r.c1.min(at.table.grid.len() - 1)]
                    .iter()
                    .sum(),
            ));
            ops.push(Op::SetCell {
                cell: keep.id,
                props: Box::new(props),
                grid_span: u32::try_from(span).unwrap_or(1),
                v_merge,
            });
            if row == r.r0 {
                top_cell = Some(keep.clone());
            } else {
                moved.extend(keep.blocks.iter().filter(|b| !is_empty_para(b)).cloned());
                // A continuation cell keeps one empty paragraph.
                let fresh = Paragraph::new(self.doc.alloc_id());
                let n = keep.blocks.len();
                ops.push(Op::InsertBlocks {
                    container: keep.id,
                    index: n,
                    blocks: vec![Block::Paragraph(fresh)],
                });
                ops.push(Op::RemoveBlocks {
                    container: keep.id,
                    index: 0,
                    count: n,
                });
            }
            self.batch_apply(&mut batch, Transaction::new(ops))?;
        }
        let Some(top) = top_cell else {
            return Ok(());
        };
        if !moved.is_empty() {
            let index = self
                .doc
                .cell(top.id)
                .map_or(top.blocks.len(), |c| c.blocks.len());
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container: top.id,
                    index,
                    blocks: moved,
                }]),
            )?;
        }
        let caret = first_para(&top).map_or(Range::new(self.anchor, self.head), |p| {
            Range::caret(Pos::new(p, 0))
        });
        self.batch_commit(batch, caret);
        Ok(())
    }

    /// Split Cells: the caret's cell into `rows` × `cols` cells (new grid columns where the cell
    /// is narrower than `cols` grid columns; new rows merged around the other cells).
    pub(super) fn split_cells(&mut self, rows: u32, cols: u32) -> Result<()> {
        if !(1..=20).contains(&rows) || !(1..=20).contains(&cols) {
            return Err(KilnError::InvalidInput(
                "split into 1–20 rows and 1–20 columns".into(),
            ));
        }
        let at = self.table_here()?;
        let map = at.table.cell_map();
        let slot = map
            .iter()
            .find(|s| s.row == at.row && s.cell == at.cell)
            .copied()
            .ok_or_else(|| KilnError::Internal("the caret's cell is not in the grid".into()))?;
        let cols = cols as usize;
        let mut batch = self.batch(EditKind::Structure);
        // Columns: widen the grid if needed, then split the span.
        let span = slot.col_span;
        if cols > span {
            let extra = cols - span;
            let last = slot.col + span - 1;
            let w = at.table.grid.get(last).copied().unwrap_or(1440);
            let mut grid = at.table.grid.clone();
            let part = (w / i32::try_from(extra + 1).unwrap_or(1)).max(1);
            if last < grid.len() {
                grid[last] = w - part * i32::try_from(extra).unwrap_or(0);
            }
            for _ in 0..extra {
                grid.insert((last + 1).min(grid.len()), part);
            }
            let mut ops = vec![Op::SetTableGrid {
                table: at.table.id,
                grid,
            }];
            // Every other cell over the widened grid column grows with it.
            for s in &map {
                if (s.row, s.cell) == (slot.row, slot.cell) {
                    continue;
                }
                if s.col <= last && last < s.col + s.col_span {
                    let c = &at.table.rows[s.row].cells[s.cell];
                    ops.push(Op::SetCell {
                        cell: c.id,
                        props: Box::new(c.props.clone()),
                        grid_span: c.grid_span + u32::try_from(extra).unwrap_or(0),
                        v_merge: c.v_merge,
                    });
                }
            }
            self.batch_apply(&mut batch, Transaction::new(ops))?;
        }
        let span = span.max(cols);
        let grid = self
            .doc
            .table(at.table.id)
            .map(|t| t.grid.clone())
            .unwrap_or_default();
        let widths: Vec<i32> = (slot.col..slot.col + span)
            .map(|c| grid.get(c).copied().unwrap_or(1440))
            .collect();
        // Spans of the new cells: as even as the grid columns allow.
        let mut spans = vec![span / cols; cols];
        for s in spans.iter_mut().take(span % cols) {
            *s += 1;
        }
        let cell = at.table.rows[at.row].cells[at.cell].clone();
        let mut ops = Vec::new();
        let mut col = 0;
        let mut new_cells = Vec::new();
        for (i, s) in spans.iter().enumerate() {
            let w: i32 = widths[col..col + s].iter().sum();
            col += s;
            if i == 0 {
                let mut props = cell.props.clone();
                props.width = Some(TableWidth::dxa(w));
                ops.push(Op::SetCell {
                    cell: cell.id,
                    props: Box::new(props),
                    grid_span: u32::try_from(*s).unwrap_or(1),
                    v_merge: cell.v_merge,
                });
            } else {
                let id = self.doc.alloc_id();
                let para = self.doc.alloc_id();
                new_cells.push(empty_cell(id, para, w, u32::try_from(*s).unwrap_or(1)));
            }
        }
        let row_id = at.table.rows[at.row].id;
        if !new_cells.is_empty() {
            ops.push(Op::InsertCells {
                row: row_id,
                index: at.cell + 1,
                cells: new_cells,
            });
        }
        self.batch_apply(&mut batch, Transaction::new(ops))?;
        // Rows: new rows below, the other cells of the row merged down over them.
        if rows > 1 {
            let t = self
                .doc
                .table(at.table.id)
                .cloned()
                .ok_or_else(|| KilnError::Internal("the table vanished".into()))?;
            let row = t.rows[at.row].clone();
            let map = t.cell_map();
            let first = at.cell;
            let last = at.cell + cols - 1;
            let mut ops = Vec::new();
            for _ in 1..rows {
                let mut cells = Vec::new();
                for (i, c) in row.cells.iter().enumerate() {
                    let id = self.doc.alloc_id();
                    let para = self.doc.alloc_id();
                    let mut nc = empty_cell(id, para, 0, c.grid_span);
                    nc.props = TableCellProps {
                        width: c.props.width,
                        ..TableCellProps::default()
                    };
                    if i < first || i > last {
                        nc.v_merge = Some(VMerge::Continue);
                    }
                    cells.push(nc);
                }
                ops.push(Op::InsertRows {
                    table: t.id,
                    index: at.row + 1,
                    rows: vec![Row {
                        id: self.doc.alloc_id(),
                        props: TableRowProps {
                            header: None,
                            ..row.props.clone()
                        },
                        revision: None,
                        cells,
                    }],
                });
            }
            for (i, c) in row.cells.iter().enumerate() {
                if (i < first || i > last) && c.v_merge.is_none() {
                    let _ = &map;
                    ops.push(Op::SetCell {
                        cell: c.id,
                        props: Box::new(c.props.clone()),
                        grid_span: c.grid_span,
                        v_merge: Some(VMerge::Restart),
                    });
                }
            }
            self.batch_apply(&mut batch, Transaction::new(ops))?;
        }
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Distribute Rows (equal heights) or Columns (equal widths) over the selection.
    pub(super) fn distribute(&mut self, rows: bool) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let (r, whole) = if r.r0 == r.r1 && r.c0 == r.c1 {
            (
                Rect {
                    r0: 0,
                    r1: at.table.rows.len() - 1,
                    c0: 0,
                    c1: at.table.grid_width().saturating_sub(1),
                },
                true,
            )
        } else {
            (r, false)
        };
        let _ = whole;
        let mut ops = Vec::new();
        if rows {
            // Equal heights: the tallest laid-out row decides, as an "at least" height.
            let heights: Vec<i32> = (r.r0..=r.r1)
                .map(|i| at.table.rows[i].props.height.map_or(0, |h| h.value))
                .collect();
            let h = heights.iter().copied().max().unwrap_or(0).max(288);
            for i in r.r0..=r.r1 {
                let row = &at.table.rows[i];
                ops.push(Op::SetRow {
                    row: row.id,
                    props: Box::new(TableRowProps {
                        height: Some(kiln_doc::props::RowHeight {
                            value: h,
                            rule: HeightRule::AtLeast,
                        }),
                        ..row.props.clone()
                    }),
                    revision: row.revision.clone(),
                });
            }
        } else {
            let mut grid = at.table.grid.clone();
            let c1 = r.c1.min(grid.len().saturating_sub(1));
            if grid.is_empty() || r.c0 > c1 {
                return Ok(());
            }
            let total: i32 = grid[r.c0..=c1].iter().sum();
            let n = i32::try_from(c1 - r.c0 + 1).unwrap_or(1);
            for g in &mut grid[r.c0..=c1] {
                *g = total / n;
            }
            ops.push(Op::SetTableGrid {
                table: at.table.id,
                grid: grid.clone(),
            });
            for s in at.table.cell_map() {
                let c = &at.table.rows[s.row].cells[s.cell];
                let w: i32 = grid[s.col.min(grid.len())..(s.col + s.col_span).min(grid.len())]
                    .iter()
                    .sum();
                ops.push(Op::SetCell {
                    cell: c.id,
                    props: Box::new(TableCellProps {
                        width: Some(TableWidth::dxa(w)),
                        ..c.props.clone()
                    }),
                    grid_span: c.grid_span,
                    v_merge: c.v_merge,
                });
            }
            let mut props = at.table.props.clone();
            props.layout = Some(TableLayout::Fixed);
            ops.push(Op::SetTableProps {
                table: at.table.id,
                props: Box::new(props),
            });
        }
        self.commit_table(ops)
    }

    fn commit_table(&mut self, ops: Vec<Op>) -> Result<()> {
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Changes the properties of the selected cells.
    fn format_cells(&mut self, f: impl Fn(&mut TableCellProps)) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let mut ops = Vec::new();
        for (ri, ci) in Self::cells_in(&at.table, r) {
            let c = &at.table.rows[ri].cells[ci];
            let mut props = c.props.clone();
            f(&mut props);
            ops.push(Op::SetCell {
                cell: c.id,
                props: Box::new(props),
                grid_span: c.grid_span,
                v_merge: c.v_merge,
            });
        }
        self.commit_table(ops)
    }

    /// Alignment: the cells' vertical alignment and their paragraphs' horizontal alignment.
    pub(super) fn cell_alignment(&mut self, v: CellVerticalAlign, h: TextAlign) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let mut batch = self.batch(EditKind::Format);
        let mut ops = Vec::new();
        let align = super::paragraph::alignment(h);
        for (ri, ci) in Self::cells_in(&at.table, r) {
            let c = &at.table.rows[ri].cells[ci];
            let mut props = c.props.clone();
            props.v_align = Some(match v {
                CellVerticalAlign::Top => CellVAlign::Top,
                CellVerticalAlign::Center => CellVAlign::Center,
                CellVerticalAlign::Bottom => CellVAlign::Bottom,
            });
            ops.push(Op::SetCell {
                cell: c.id,
                props: Box::new(props),
                grid_span: c.grid_span,
                v_merge: c.v_merge,
            });
            for b in &c.blocks {
                if let Block::Paragraph(p) = b {
                    let mut pp = p.props.clone();
                    pp.alignment = Some(align);
                    ops.push(Op::SetParagraphProps {
                        para: p.id,
                        props: Box::new(pp),
                    });
                }
            }
        }
        self.batch_apply(&mut batch, Transaction::new(ops))?;
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Repeat Header Rows: toggles the selected rows (with every row above them) as header rows.
    pub(super) fn repeat_header_rows(&mut self) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let on = !at.table.rows[r.r0..=r.r1]
            .iter()
            .all(|row| row.props.header == Some(true));
        let range = if on {
            0..=r.r1
        } else {
            r.r0..=at.table.rows.len() - 1
        };
        let mut ops = Vec::new();
        for i in range {
            let row = &at.table.rows[i];
            let header = if on { Some(true) } else { None };
            if row.props.header != header {
                ops.push(Op::SetRow {
                    row: row.id,
                    props: Box::new(TableRowProps {
                        header,
                        ..row.props.clone()
                    }),
                    revision: row.revision.clone(),
                });
            }
        }
        self.commit_table(ops)
    }

    /// Convert to Text: each row becomes a paragraph of its cells' text joined by `separator`.
    pub(super) fn table_to_text(&mut self, separator: TextSeparator) -> Result<()> {
        let at = self.table_here()?;
        let (container, index) = self
            .doc
            .parent_of(at.table.id)
            .ok_or_else(|| KilnError::Internal("a table without a container".into()))?;
        let sep = match separator {
            TextSeparator::Tabs => "\t",
            TextSeparator::Commas => ",",
            TextSeparator::Paragraphs => "\n",
        };
        let mut paras = Vec::new();
        for row in &at.table.rows {
            let line = row
                .cells
                .iter()
                .filter(|c| c.v_merge != Some(VMerge::Continue))
                .map(cell_text)
                .collect::<Vec<_>>()
                .join(sep);
            for text in line.split('\n') {
                let mut p = Paragraph::new(self.doc.alloc_id());
                p.content = kiln_doc::Inlines::from_fragment(kiln_doc::Fragment::text(
                    text,
                    RunProps::default(),
                ))
                .map_err(doc_err)?;
                paras.push(Block::Paragraph(p));
            }
        }
        let first = paras.first().map(Block::id);
        let mut batch = self.batch(EditKind::Structure);
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![
                Op::InsertBlocks {
                    container,
                    index: index + 1,
                    blocks: paras,
                },
                Op::RemoveBlocks {
                    container,
                    index,
                    count: 1,
                },
            ]),
        )?;
        let caret = first.map_or(Range::new(self.anchor, self.head), |p| {
            Range::caret(Pos::new(p, 0))
        });
        self.batch_commit(batch, caret);
        Ok(())
    }

    /// Convert Text to Table: the selected paragraphs, one row each, cells split at `separator`
    /// (one cell per paragraph with `Paragraphs`).
    pub(super) fn convert_text_to_table(&mut self, separator: TextSeparator) -> Result<()> {
        let ids = self.selected_paragraphs()?;
        let Some(first) = ids.first().copied() else {
            return Ok(());
        };
        let (container, index) = self
            .doc
            .parent_of(first)
            .ok_or_else(|| KilnError::Internal("a paragraph without a container".into()))?;
        if ids
            .iter()
            .any(|id| self.doc.parent_of(*id).map(|(c, _)| c) != Some(container))
        {
            return Err(KilnError::InvalidInput(
                "select paragraphs of one text flow to convert them".into(),
            ));
        }
        let lines: Vec<Vec<String>> = match separator {
            TextSeparator::Paragraphs => vec![ids.iter().map(|id| self.para_text(*id)).collect()],
            TextSeparator::Tabs | TextSeparator::Commas => {
                let sep = if separator == TextSeparator::Tabs {
                    '\t'
                } else {
                    ','
                };
                ids.iter()
                    .map(|id| {
                        self.para_text(*id)
                            .replace(kiln_doc::OBJECT_CHAR, "")
                            .split(sep)
                            .map(str::to_owned)
                            .collect()
                    })
                    .collect()
            }
        };
        let cols = lines.iter().map(Vec::len).max().unwrap_or(1).clamp(1, 63);
        let width = self
            .doc
            .section_of(first)
            .map_or(9360, |s| s.props.content_width());
        let w = width / i32::try_from(cols).unwrap_or(1);
        let mut rows = Vec::new();
        for line in &lines {
            let mut cells = Vec::new();
            for c in 0..cols {
                let id = self.doc.alloc_id();
                let pid = self.doc.alloc_id();
                let mut cell = empty_cell(id, pid, w, 1);
                if let Some(text) = line.get(c)
                    && let Some(Block::Paragraph(p)) = cell.blocks.first_mut()
                {
                    p.content = kiln_doc::Inlines::from_fragment(kiln_doc::Fragment::text(
                        text.trim(),
                        RunProps::default(),
                    ))
                    .map_err(doc_err)?;
                }
                cells.push(cell);
            }
            rows.push(Row {
                id: self.doc.alloc_id(),
                props: TableRowProps::default(),
                revision: None,
                cells,
            });
        }
        self.ensure_style("TableGrid")?;
        let table = Table {
            id: self.doc.alloc_id(),
            props: TableProps {
                style_id: self
                    .doc
                    .styles
                    .contains("TableGrid")
                    .then(|| "TableGrid".to_owned()),
                width: Some(TableWidth {
                    kind: WidthType::Auto,
                    value: 0,
                }),
                look: Some(TableLook::default()),
                ..TableProps::default()
            },
            grid: vec![w; cols],
            rows,
        };
        let caret = table
            .rows
            .first()
            .and_then(|r| r.cells.first())
            .and_then(first_para);
        let mut batch = self.batch(EditKind::Structure);
        let count = ids.len();
        let blocks = self.doc.container(container).map_err(doc_err)?;
        // Keep the container non-empty: insert first, then remove the paragraphs.
        let _ = blocks;
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![
                Op::InsertBlocks {
                    container,
                    index,
                    blocks: vec![Block::Table(table)],
                },
                Op::RemoveBlocks {
                    container,
                    index: index + 1,
                    count,
                },
            ]),
        )?;
        // Word keeps a paragraph after a table at the end of a cell or story.
        let after_index = index + 1;
        let needs_para = self
            .doc
            .container(container)
            .map_err(doc_err)?
            .get(after_index)
            .is_none();
        if needs_para {
            let p = Paragraph::new(self.doc.alloc_id());
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container,
                    index: after_index,
                    blocks: vec![Block::Paragraph(p)],
                }]),
            )?;
        }
        let after = caret.map_or(Range::new(self.anchor, self.head), |p| {
            Range::caret(Pos::new(p, 0))
        });
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Table Style Options.
    pub(super) fn table_look(&mut self, f: impl Fn(&mut TableLook)) -> Result<()> {
        let at = self.table_here()?;
        let mut props = at.table.props.clone();
        let mut look = props.look.unwrap_or_default();
        f(&mut look);
        props.look = Some(look);
        self.commit_table(vec![Op::SetTableProps {
            table: at.table.id,
            props: Box::new(props),
        }])
    }

    /// Table Styles: Word's built-in style (added when missing) or one of the document's.
    pub(super) fn table_style(&mut self, style: &str) -> Result<()> {
        let at = self.table_here()?;
        let id = gallery_style(style.trim()).to_owned();
        let mut batch = self.batch(EditKind::Format);
        if !self.doc.styles.contains(&id) {
            let Some(def) = builtin_table_style(&id) else {
                return Err(KilnError::InvalidInput(format!("no table style '{style}'")));
            };
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::PutStyle {
                    style: Box::new(def),
                    index: self.doc.styles.len(),
                }]),
            )?;
        } else if self
            .doc
            .styles
            .get(&id)
            .is_some_and(|s| s.kind != StyleType::Table)
        {
            return Err(KilnError::InvalidInput(format!(
                "'{id}' is not a table style"
            )));
        }
        let mut props = at.table.props.clone();
        props.style_id = Some(id);
        // A style's borders show only without direct table borders, as when Word applies one.
        props.borders = None;
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::SetTableProps {
                table: at.table.id,
                props: Box::new(props),
            }]),
        )?;
        let after = Range::new(self.anchor, self.head);
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Cell shading (`None` removes it).
    pub(super) fn cell_shading(&mut self, color: Option<&str>) -> Result<()> {
        let shading = color.map(doc_color).transpose()?.map(Shading::fill);
        self.format_cells(move |p| p.shading.clone_from(&shading))
    }

    /// Cell borders.
    pub(super) fn cell_borders(&mut self, preset: BorderPreset) -> Result<()> {
        let at = self.table_here()?;
        let r = self.cell_rect(&at);
        let line = border(BorderStyle::Single, 4, DocColor::auto());
        let none = border(BorderStyle::Nil, 0, DocColor::auto());
        let map = at.table.cell_map();
        let mut ops = Vec::new();
        for s in &map {
            if !(s.row >= r.r0 && s.row <= r.r1 && s.col <= r.c1 && s.col + s.col_span > r.c0) {
                continue;
            }
            let c = &at.table.rows[s.row].cells[s.cell];
            let mut props = c.props.clone();
            let mut b = props.borders.clone().unwrap_or_default();
            let (top, bottom) = (s.row == r.r0, s.row == r.r1);
            let (left, right) = (s.col == r.c0, s.col + s.col_span - 1 == r.c1);
            let set = |slot: &mut Option<Border>, on: bool| {
                if on {
                    *slot = Some(line.clone());
                }
            };
            match preset {
                BorderPreset::None => {
                    b = CellBorders {
                        top: Some(none.clone()),
                        left: Some(none.clone()),
                        bottom: Some(none.clone()),
                        right: Some(none.clone()),
                        ..CellBorders::default()
                    };
                }
                BorderPreset::All => {
                    set(&mut b.top, true);
                    set(&mut b.bottom, true);
                    set(&mut b.left, true);
                    set(&mut b.right, true);
                }
                BorderPreset::Outside => {
                    set(&mut b.top, top);
                    set(&mut b.bottom, bottom);
                    set(&mut b.left, left);
                    set(&mut b.right, right);
                }
                BorderPreset::Inside => {
                    set(&mut b.top, !top);
                    set(&mut b.bottom, !bottom);
                    set(&mut b.left, !left);
                    set(&mut b.right, !right);
                }
                BorderPreset::InsideHorizontal => {
                    set(&mut b.top, !top);
                    set(&mut b.bottom, !bottom);
                }
                BorderPreset::InsideVertical => {
                    set(&mut b.left, !left);
                    set(&mut b.right, !right);
                }
                BorderPreset::Top => set(&mut b.top, top),
                BorderPreset::Bottom => set(&mut b.bottom, bottom),
                BorderPreset::Left => set(&mut b.left, left),
                BorderPreset::Right => set(&mut b.right, right),
            }
            props.borders = Some(b);
            ops.push(Op::SetCell {
                cell: c.id,
                props: Box::new(props),
                grid_span: c.grid_span,
                v_merge: c.v_merge,
            });
        }
        self.commit_table(ops)
    }

    /// AutoFit Contents, AutoFit Window or Fixed Column Width.
    pub(super) fn table_autofit(&mut self, mode: TableAutofit) -> Result<()> {
        let at = self.table_here()?;
        let mut props = at.table.props.clone();
        match mode {
            TableAutofit::Fixed => {
                props.layout = Some(TableLayout::Fixed);
                props.width = Some(TableWidth::dxa(at.table.grid.iter().sum()));
            }
            TableAutofit::Contents => {
                props.layout = Some(TableLayout::Autofit);
                props.width = Some(TableWidth {
                    kind: WidthType::Auto,
                    value: 0,
                });
            }
            TableAutofit::Window => {
                props.layout = Some(TableLayout::Autofit);
                props.width = Some(TableWidth::pct(5000));
            }
        }
        self.commit_table(vec![Op::SetTableProps {
            table: at.table.id,
            props: Box::new(props),
        }])
    }

    /// The table's alignment on the page.
    pub(super) fn table_align(&mut self, align: TextAlign) -> Result<()> {
        let at = self.table_here()?;
        let mut props = at.table.props.clone();
        props.alignment = Some(match align {
            TextAlign::Center => TableAlignment::Center,
            TextAlign::Right => TableAlignment::Right,
            TextAlign::Left | TextAlign::Justify => TableAlignment::Left,
        });
        props.indent = None;
        self.commit_table(vec![Op::SetTableProps {
            table: at.table.id,
            props: Box::new(props),
        }])
    }

    /// Split Table: the rows from the caret's row become a new table after an empty paragraph.
    pub(super) fn split_table(&mut self) -> Result<()> {
        let at = self.table_here()?;
        if at.row == 0 {
            // Word inserts a paragraph above the table instead.
            let (container, index) = self
                .doc
                .parent_of(at.table.id)
                .ok_or_else(|| KilnError::Internal("a table without a container".into()))?;
            let p = Paragraph::new(self.doc.alloc_id());
            let id = p.id;
            let mut batch = self.batch(EditKind::Structure);
            self.batch_apply(
                &mut batch,
                Transaction::new(vec![Op::InsertBlocks {
                    container,
                    index,
                    blocks: vec![Block::Paragraph(p)],
                }]),
            )?;
            self.batch_commit(batch, Range::caret(Pos::new(id, 0)));
            return Ok(());
        }
        let (container, index) = self
            .doc
            .parent_of(at.table.id)
            .ok_or_else(|| KilnError::Internal("a table without a container".into()))?;
        let mut lower = at.table.clone();
        lower.id = self.doc.alloc_id();
        lower.rows = at.table.rows[at.row..].to_vec();
        for row in &mut lower.rows {
            row.props.header = None;
            for c in &mut row.cells {
                if c.v_merge == Some(VMerge::Continue) {
                    c.v_merge = Some(VMerge::Restart);
                    break;
                }
            }
        }
        let count = at.table.rows.len() - at.row;
        let gap = Paragraph::new(self.doc.alloc_id());
        let gap_id = gap.id;
        let mut batch = self.batch(EditKind::Structure);
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::RemoveRows {
                table: at.table.id,
                index: at.row,
                count,
            }]),
        )?;
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::InsertBlocks {
                container,
                index: index + 1,
                blocks: vec![Block::Paragraph(gap), Block::Table(lower)],
            }]),
        )?;
        self.batch_commit(batch, Range::caret(Pos::new(gap_id, 0)));
        Ok(())
    }

    /// Select ▸ Cell, Row, Column or Table (as a text selection from the first to the last
    /// paragraph; a column selects its cells' text from top to bottom).
    pub(super) fn table_select(&mut self, what: TableSelectTarget) -> Result<()> {
        let at = self.table_here()?;
        let t = &at.table;
        let cells: Vec<&Cell> = match what {
            TableSelectTarget::Cell => vec![&t.rows[at.row].cells[at.cell]],
            TableSelectTarget::Row => t.rows[at.row].cells.iter().collect(),
            TableSelectTarget::Column => t
                .cell_map()
                .into_iter()
                .filter(|s| s.col <= at.col && at.col < s.col + s.col_span)
                .map(|s| &t.rows[s.row].cells[s.cell])
                .collect(),
            TableSelectTarget::Table => t.rows.iter().flat_map(|r| r.cells.iter()).collect(),
        };
        let (Some(first), Some(last)) = (cells.first(), cells.last()) else {
            return Ok(());
        };
        if let (Some(a), Some((b, len))) = (first_para(first), last_para(last)) {
            self.set_selection(Pos::new(a, 0), Pos::new(b, len));
        }
        Ok(())
    }

    /// Cell Size: the caret column's width and row's height, in points.
    pub(super) fn table_cell_size(
        &mut self,
        width: Option<f32>,
        height: Option<f32>,
    ) -> Result<()> {
        let at = self.table_here()?;
        let mut ops = Vec::new();
        if let Some(w) = width {
            let tw = super::paragraph::twips(w)?.max(1);
            let mut grid = at.table.grid.clone();
            if at.col < grid.len() {
                grid[at.col] = tw;
            }
            ops.push(Op::SetTableGrid {
                table: at.table.id,
                grid: grid.clone(),
            });
            for s in at.table.cell_map() {
                if s.col <= at.col && at.col < s.col + s.col_span {
                    let c = &at.table.rows[s.row].cells[s.cell];
                    let w: i32 = grid[s.col.min(grid.len())..(s.col + s.col_span).min(grid.len())]
                        .iter()
                        .sum();
                    ops.push(Op::SetCell {
                        cell: c.id,
                        props: Box::new(TableCellProps {
                            width: Some(TableWidth::dxa(w)),
                            ..c.props.clone()
                        }),
                        grid_span: c.grid_span,
                        v_merge: c.v_merge,
                    });
                }
            }
        }
        if let Some(h) = height {
            let row = &at.table.rows[at.row];
            ops.push(Op::SetRow {
                row: row.id,
                props: Box::new(TableRowProps {
                    height: Some(kiln_doc::props::RowHeight {
                        value: super::paragraph::twips(h)?.max(0),
                        rule: HeightRule::AtLeast,
                    }),
                    ..row.props.clone()
                }),
                revision: row.revision.clone(),
            });
        }
        self.commit_table(ops)
    }

    /// Tab / Shift+Tab.
    pub(super) fn tab(&mut self, back: bool) -> Result<()> {
        let (s, e) = self.span();
        if let Some(at) = self.table_of(s.para) {
            let order: Vec<(usize, usize)> = at
                .table
                .cell_map()
                .into_iter()
                .filter(|x| x.row_span > 0)
                .map(|x| (x.row, x.cell))
                .collect();
            let i = order
                .iter()
                .position(|c| *c == (at.row, at.cell))
                .unwrap_or(0);
            if back {
                if let Some(&(r, c)) = i.checked_sub(1).and_then(|j| order.get(j)) {
                    let cell = at.table.rows[r].cells[c].clone();
                    self.put_caret_in_cell(&cell, true);
                }
                return Ok(());
            }
            if let Some(&(r, c)) = order.get(i + 1) {
                let cell = at.table.rows[r].cells[c].clone();
                self.put_caret_in_cell(&cell, true);
                return Ok(());
            }
            // After the last cell: a new row (Word).
            let last_row = at.table.rows[at.table.rows.len() - 1].id;
            let tx = edit::insert_row(&mut self.doc, last_row, true).map_err(doc_err)?;
            let new_first = tx.ops.iter().find_map(|op| match op {
                Op::InsertRows { rows, .. } => rows.first()?.cells.first().and_then(first_para),
                _ => None,
            });
            let after = new_first.map(|p| Range::caret(Pos::new(p, 0)));
            let tx = tx.with_selection(Some(Range::new(self.anchor, self.head)), after);
            return self.commit(tx);
        }
        let in_list = self.list_of(s.para).is_some();
        let at_start = s.offset == 0 && (s == e || s.para != e.para);
        if in_list && (at_start || s.para != e.para) {
            let ids = self.selected_paragraphs()?;
            return self.shift_list_level(&ids, if back { -1 } else { 1 });
        }
        if back {
            return Ok(());
        }
        self.insert_plain("\t")
    }
}

fn is_empty_para(b: &Block) -> bool {
    matches!(b, Block::Paragraph(p) if p.content.is_empty())
}
