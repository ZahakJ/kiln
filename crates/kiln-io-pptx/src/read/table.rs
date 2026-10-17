// SPDX-License-Identifier: AGPL-3.0-or-later
//! `a:tbl` → [`Table`]: grid, rows, cells with spans and merges, cell formatting, style and
//! style options. Ragged rows are padded or cut to the grid (with a diagnostic) so the model's
//! one-cell-per-column invariant holds.

use kiln_deck::geom::MAX_COORD;
use kiln_deck::shape::{CellBorder, CellProps, Table, TableCell, TableLook, TableRow};
use kiln_deck::text::{TextAnchor, TextDirection};
use kiln_ooxml::drawingml::fill::find_fill;
use kiln_ooxml::drawingml::line::parse_line;
use kiln_ooxml::xml::XmlElement;

use super::Reader;
use super::context::PartCx;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::attr_u32;

/// The most rows and columns read; anything beyond is kept only in the original file's XML.
const MAX_CELLS: usize = 1_000_000;

impl Reader<'_> {
    /// Reads a table; `None` for a table without rows or columns (the frame is then kept as
    /// XML).
    pub fn read_table(&mut self, cx: &PartCx, tbl: &XmlElement, loc: &str) -> Option<Table> {
        let pr = tbl.child("a:tblPr");
        let flag = |n: &str| pr.and_then(|p| p.attr_bool(n)).unwrap_or(false);
        let mut grid: Vec<i64> = tbl
            .child("a:tblGrid")
            .map(|g| {
                g.children_named("a:gridCol")
                    .map(|c| c.attr_int("w").unwrap_or(0).clamp(0, MAX_COORD))
                    .collect()
            })
            .unwrap_or_default();
        let row_elements: Vec<&XmlElement> = tbl.children_named("a:tr").collect();
        if grid.is_empty() {
            let widest = row_elements
                .iter()
                .map(|r| r.children_named("a:tc").count())
                .max()
                .unwrap_or(0);
            grid = vec![914_400; widest];
        }
        if grid.is_empty() || row_elements.is_empty() {
            self.warn(
                WarningCode::TableIrregular,
                Severity::Warning,
                "an empty table is kept for saving but not shown",
                &cx.part,
                loc.to_owned(),
            );
            return None;
        }
        let cols = grid.len();
        let mut ragged = false;
        let mut rows = Vec::with_capacity(row_elements.len());
        let mut cells_read = 0usize;
        for tr in row_elements {
            let mut cells: Vec<TableCell> = Vec::with_capacity(cols);
            for tc in tr.children_named("a:tc") {
                if cells.len() == cols {
                    ragged = true;
                    break;
                }
                cells.push(self.read_cell(cx, tc, loc));
            }
            if cells.len() < cols {
                ragged = true;
                cells.resize_with(cols, TableCell::new);
            }
            cells_read += cols;
            rows.push(TableRow {
                height: tr.attr_int("h").unwrap_or(0).clamp(0, MAX_COORD),
                cells,
            });
            if cells_read > MAX_CELLS {
                ragged = true;
                break;
            }
        }
        if ragged {
            self.warn(
                WarningCode::TableIrregular,
                Severity::Warning,
                "a table's rows do not match its columns; cells were added or dropped",
                &cx.part,
                loc.to_owned(),
            );
        }
        Some(Table {
            grid,
            rows,
            style_id: pr
                .and_then(|p| p.child("a:tableStyleId"))
                .map(XmlElement::text)
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty()),
            look: TableLook {
                first_row: flag("firstRow"),
                first_col: flag("firstCol"),
                last_row: flag("lastRow"),
                last_col: flag("lastCol"),
                band_row: flag("bandRow"),
                band_col: flag("bandCol"),
            },
            rtl: flag("rtl"),
        })
    }

    fn read_cell(&mut self, cx: &PartCx, tc: &XmlElement, loc: &str) -> TableCell {
        let mut cell = TableCell::new();
        cell.grid_span = attr_u32(tc, "gridSpan").unwrap_or(1).max(1);
        cell.row_span = attr_u32(tc, "rowSpan").unwrap_or(1).max(1);
        cell.h_merge = tc.attr_bool("hMerge").unwrap_or(false);
        cell.v_merge = tc.attr_bool("vMerge").unwrap_or(false);
        if let Some(tx) = tc.child("a:txBody") {
            cell.text = self.read_text_body(cx, tx, loc);
        }
        if let Some(pr) = tc.child("a:tcPr") {
            let border = |n: &str| CellBorder(pr.child(n).map(parse_line));
            let mut fill = find_fill(pr);
            if let Some(f) = fill.as_mut() {
                self.fix_fill(cx, f, loc);
            }
            cell.props = CellProps {
                fill,
                borders: [
                    border("a:lnL"),
                    border("a:lnR"),
                    border("a:lnT"),
                    border("a:lnB"),
                ],
                margins: [
                    pr.attr_int("marL"),
                    pr.attr_int("marT"),
                    pr.attr_int("marR"),
                    pr.attr_int("marB"),
                ],
                anchor: pr.attr("anchor").and_then(TextAnchor::parse),
                direction: pr.attr("vert").and_then(TextDirection::parse),
            };
        }
        cell
    }
}
