// SPDX-License-Identifier: AGPL-3.0-or-later
//! A worksheet: the sparse cell store, row and column properties and the per-sheet metadata.
//!
//! Mutation goes through the [`Workbook`](crate::Workbook) (commands record undo and keep the
//! formula engine in step); this module holds the data and the low-level operations commands are
//! built from.

use kiln_formula::SheetId;
use kiln_formula::refs::StructuralEdit;
use serde::{Deserialize, Serialize};

use crate::address::{CellPos, Range};
use crate::axis::{
    Axis, BASE_COL_WIDTH, ColProps, DEFAULT_ROW_HEIGHT_PT, RowProps, SizeUnit, col_axis,
    col_width_to_px, default_col_width_for, row_axis, row_height_to_px,
};
use crate::color::Color;
use crate::error::ModelError;
use crate::meta::{
    AutoFilter, CfRule, ConditionalFormat, DataValidation, DrawingAnchor, Hyperlink, Note,
    PrintSettings, SheetProtection, SheetView, SheetVisibility, Table, ThreadedComment,
};
use crate::shift::{
    shift_auto_filter, shift_breaks, shift_cell, shift_drawing, shift_frozen_count, shift_merges,
    shift_range, shift_ranges, shift_table,
};
use crate::store::SparseStore;

fn yes() -> bool {
    true
}

/// Every piece of per-sheet metadata (cells and row/column properties excluded).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetMeta {
    /// Merged regions.
    #[serde(default)]
    pub merges: Vec<Range>,
    /// Conditional formatting.
    #[serde(default)]
    pub cf: Vec<ConditionalFormat>,
    /// Data validation (the last rule listed for a cell wins, as in Excel).
    #[serde(default)]
    pub validations: Vec<DataValidation>,
    /// Tables.
    #[serde(default)]
    pub tables: Vec<Table>,
    /// Hyperlinks.
    #[serde(default)]
    pub hyperlinks: Vec<Hyperlink>,
    /// Notes.
    #[serde(default)]
    pub notes: Vec<Note>,
    /// Threaded comments.
    #[serde(default)]
    pub threads: Vec<ThreadedComment>,
    /// The view.
    #[serde(default)]
    pub view: SheetView,
    /// Page setup.
    #[serde(default)]
    pub print: PrintSettings,
    /// AutoFilter.
    #[serde(default)]
    pub auto_filter: Option<AutoFilter>,
    /// Charts, pictures and shapes.
    #[serde(default)]
    pub drawings: Vec<DrawingAnchor>,
    /// Visibility.
    #[serde(default)]
    pub visibility: SheetVisibility,
    /// Tab colour.
    #[serde(default)]
    pub tab_color: Option<Color>,
    /// Outline summary rows below the detail.
    #[serde(default = "yes")]
    pub summary_below: bool,
    /// Outline summary columns right of the detail.
    #[serde(default = "yes")]
    pub summary_right: bool,
    /// Protection (`None` = not protected).
    #[serde(default)]
    pub protection: Option<SheetProtection>,
    /// Default row height in points.
    pub default_row_height: f64,
    /// Default column width (stored units); `None` = derived from the default font.
    #[serde(default)]
    pub default_col_width: Option<f64>,
}

impl Default for SheetMeta {
    fn default() -> SheetMeta {
        SheetMeta {
            merges: Vec::new(),
            cf: Vec::new(),
            validations: Vec::new(),
            tables: Vec::new(),
            hyperlinks: Vec::new(),
            notes: Vec::new(),
            threads: Vec::new(),
            view: SheetView::default(),
            print: PrintSettings::default(),
            auto_filter: None,
            drawings: Vec::new(),
            visibility: SheetVisibility::Visible,
            tab_color: None,
            summary_below: true,
            summary_right: true,
            protection: None,
            default_row_height: DEFAULT_ROW_HEIGHT_PT,
            default_col_width: None,
        }
    }
}

/// Row and column properties of a sheet (for snapshots and undo).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SheetLayout {
    /// Rows with properties.
    pub rows: Vec<(u32, RowProps)>,
    /// Columns with properties.
    pub cols: Vec<(u32, ColProps)>,
}

/// A worksheet.
#[derive(Debug, Clone)]
pub struct Sheet {
    id: SheetId,
    pub(crate) name: String,
    pub(crate) cells: SparseStore,
    pub(crate) rows: Axis<RowProps>,
    pub(crate) cols: Axis<ColProps>,
    pub(crate) meta: SheetMeta,
    mdw: f64,
}

impl Sheet {
    /// A new empty sheet; `mdw` is the maximum digit width of the workbook's default font.
    pub fn new(id: SheetId, name: impl Into<String>, mdw: f64) -> Sheet {
        let width = default_col_width_for(BASE_COL_WIDTH, mdw);
        Sheet {
            id,
            name: name.into(),
            cells: SparseStore::new(),
            rows: row_axis(DEFAULT_ROW_HEIGHT_PT),
            cols: col_axis(width, mdw),
            meta: SheetMeta::default(),
            mdw,
        }
    }

    /// Stable id.
    pub fn id(&self) -> SheetId {
        self.id
    }

    /// Name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The cell store.
    pub fn cells(&self) -> &SparseStore {
        &self.cells
    }

    /// Row properties and geometry.
    pub fn rows(&self) -> &Axis<RowProps> {
        &self.rows
    }

    /// Column properties and geometry.
    pub fn cols(&self) -> &Axis<ColProps> {
        &self.cols
    }

    /// Metadata.
    pub fn meta(&self) -> &SheetMeta {
        &self.meta
    }

    /// Tables.
    pub fn tables(&self) -> &[Table] {
        &self.meta.tables
    }

    /// Merged regions.
    pub fn merges(&self) -> &[Range] {
        &self.meta.merges
    }

    /// Maximum digit width (px) the column widths are measured in.
    pub fn max_digit_width(&self) -> f64 {
        self.mdw
    }

    /// The effective default column width (stored units).
    pub fn default_col_width(&self) -> f64 {
        self.meta
            .default_col_width
            .unwrap_or_else(|| default_col_width_for(BASE_COL_WIDTH, self.mdw))
    }

    /// Re-derives pixel sizes after the default font, its metrics or the default sizes changed.
    pub(crate) fn refresh_geometry(&mut self, mdw: f64) {
        self.mdw = mdw;
        let w = self.default_col_width();
        self.cols
            .set_default(col_width_to_px(w, mdw), SizeUnit::Chars(mdw));
        self.rows.set_default(
            row_height_to_px(self.meta.default_row_height),
            SizeUnit::Points,
        );
    }

    /// Replaces the metadata (re-deriving geometry when the default sizes changed).
    pub(crate) fn set_meta(&mut self, meta: SheetMeta) {
        let sizes = meta.default_row_height != self.meta.default_row_height
            || meta.default_col_width != self.meta.default_col_width;
        self.meta = meta;
        if sizes {
            self.refresh_geometry(self.mdw);
        }
    }

    /// Row and column properties.
    pub fn layout(&self) -> SheetLayout {
        SheetLayout {
            rows: self.rows.to_vec(),
            cols: self.cols.to_vec(),
        }
    }

    pub(crate) fn set_layout(&mut self, l: SheetLayout) {
        self.rows.load(l.rows);
        self.cols.load(l.cols);
    }

    // ── lookups ────────────────────────────────────────────────────────────

    /// The merged region containing a cell.
    pub fn merge_at(&self, row: u32, col: u32) -> Option<Range> {
        self.meta
            .merges
            .iter()
            .find(|m| m.contains(row, col))
            .copied()
    }

    /// Merged regions intersecting a range.
    pub fn merges_in(&self, r: &Range) -> Vec<Range> {
        self.meta
            .merges
            .iter()
            .filter(|m| m.intersects(r))
            .copied()
            .collect()
    }

    /// Validates a new merge: at least two cells, no overlap with other merges, not in a table.
    pub fn check_merge(&self, r: &Range) -> Result<(), ModelError> {
        r.check()?;
        if r.is_single_cell() {
            return Err(ModelError::invalid(
                "a merged region needs at least two cells",
            ));
        }
        if let Some(m) = self.meta.merges.iter().find(|m| m.intersects(r)) {
            return Err(ModelError::invalid(format!(
                "{r} overlaps the merged cells {m}"
            )));
        }
        if let Some(t) = self.meta.tables.iter().find(|t| t.range.intersects(r)) {
            return Err(ModelError::invalid(format!(
                "cells in table \"{}\" cannot be merged",
                t.name
            )));
        }
        Ok(())
    }

    /// Expands a range until it fully contains every merge it touches (Excel's selection rule).
    pub fn expand_to_merges(&self, r: Range) -> Range {
        let mut out = r;
        loop {
            let mut changed = false;
            for m in &self.meta.merges {
                if m.intersects(&out) && !m.is_inside(&out) {
                    out = out.union(m);
                    changed = true;
                }
            }
            if !changed {
                return out;
            }
        }
    }

    /// The table containing a cell.
    pub fn table_at(&self, row: u32, col: u32) -> Option<&Table> {
        self.meta.tables.iter().find(|t| t.range.contains(row, col))
    }

    /// The note on a cell.
    pub fn note_at(&self, row: u32, col: u32) -> Option<&Note> {
        self.meta
            .notes
            .iter()
            .find(|n| n.at == CellPos::new(row, col))
    }

    /// The hyperlink covering a cell.
    pub fn hyperlink_at(&self, row: u32, col: u32) -> Option<&Hyperlink> {
        self.meta
            .hyperlinks
            .iter()
            .find(|h| h.range.contains(row, col))
    }

    /// Threaded comments on a cell, in order.
    pub fn threads_at(&self, row: u32, col: u32) -> Vec<&ThreadedComment> {
        self.meta
            .threads
            .iter()
            .filter(|t| t.at == CellPos::new(row, col))
            .collect()
    }

    /// The validation rule applying to a cell (the last one listed wins).
    pub fn validation_at(&self, row: u32, col: u32) -> Option<&DataValidation> {
        self.meta
            .validations
            .iter()
            .rev()
            .find(|v| v.ranges.iter().any(|r| r.contains(row, col)))
    }

    /// Conditional-format rules applying to a cell, by ascending priority.
    pub fn cf_rules_at(&self, row: u32, col: u32) -> Vec<&CfRule> {
        let mut out: Vec<&CfRule> = self
            .meta
            .cf
            .iter()
            .filter(|b| b.ranges.iter().any(|r| r.contains(row, col)))
            .flat_map(|b| b.rules.iter())
            .collect();
        out.sort_by_key(|r| r.priority);
        out
    }

    /// True when a row is hidden (manually or by a filter).
    pub fn is_row_hidden(&self, row: u32) -> bool {
        self.rows.is_hidden(row)
    }

    /// True when a row is hidden by a filter.
    pub fn is_row_filtered(&self, row: u32) -> bool {
        self.rows.get(row).is_some_and(|p| p.filtered)
    }

    /// True when a column is hidden.
    pub fn is_col_hidden(&self, col: u32) -> bool {
        self.cols.is_hidden(col)
    }

    /// The effective style of a cell: its own, else the row format, else the column format.
    pub fn style_at(&self, row: u32, col: u32) -> u32 {
        if let Some(c) = self.cells.get(row, col) {
            return c.style;
        }
        self.rows
            .get(row)
            .and_then(|p| p.style)
            .or_else(|| self.cols.get(col).and_then(|p| p.style))
            .unwrap_or(0)
    }

    // ── structural edits ───────────────────────────────────────────────────

    /// Checks an insert or delete is allowed on this sheet (table headers).
    pub fn check_structural(&self, e: &StructuralEdit) -> Result<(), ModelError> {
        for t in &self.meta.tables {
            shift_table(t, e)?;
        }
        Ok(())
    }

    /// Applies an insert or delete to cells, row/column properties and metadata. Validates
    /// before changing anything.
    pub(crate) fn apply_structural(&mut self, e: &StructuralEdit) -> Result<(), ModelError> {
        if e.count == 0 {
            return Err(ModelError::invalid("count must be positive"));
        }
        let mut tables = Vec::with_capacity(self.meta.tables.len());
        for t in &self.meta.tables {
            if let Some(n) = shift_table(t, e)? {
                tables.push(n);
            }
        }
        let rows = e.kind.is_rows();
        let delta = if e.kind.is_insert() {
            i64::from(e.count)
        } else {
            -i64::from(e.count)
        };
        if rows {
            self.cells.shift_rows(e.at, delta);
            self.rows.shift(e.at, delta);
        } else {
            self.cells.shift_cols(e.at, delta);
            self.cols.shift(e.at, delta);
        }
        let m = &mut self.meta;
        m.tables = tables;
        m.merges = shift_merges(&m.merges, e);
        m.cf = std::mem::take(&mut m.cf)
            .into_iter()
            .filter_map(|b| {
                let ranges = shift_ranges(&b.ranges, e);
                (!ranges.is_empty()).then_some(ConditionalFormat { ranges, ..b })
            })
            .collect();
        m.validations = std::mem::take(&mut m.validations)
            .into_iter()
            .filter_map(|v| {
                let ranges = shift_ranges(&v.ranges, e);
                (!ranges.is_empty()).then_some(DataValidation { ranges, ..v })
            })
            .collect();
        m.hyperlinks = std::mem::take(&mut m.hyperlinks)
            .into_iter()
            .filter_map(|h| {
                Some(Hyperlink {
                    range: shift_range(&h.range, e)?,
                    ..h
                })
            })
            .collect();
        m.notes = std::mem::take(&mut m.notes)
            .into_iter()
            .filter_map(|n| {
                Some(Note {
                    at: shift_cell(n.at, e)?,
                    ..n
                })
            })
            .collect();
        m.threads = std::mem::take(&mut m.threads)
            .into_iter()
            .filter_map(|t| {
                Some(ThreadedComment {
                    at: shift_cell(t.at, e)?,
                    ..t
                })
            })
            .collect();
        m.auto_filter = m.auto_filter.as_ref().and_then(|f| shift_auto_filter(f, e));
        m.drawings = m.drawings.iter().map(|d| shift_drawing(d, e)).collect();
        if let Some(f) = &mut m.view.freeze {
            if rows {
                f.rows = shift_frozen_count(f.rows, e);
            } else {
                f.cols = shift_frozen_count(f.cols, e);
            }
        }
        let sel = shift_ranges(&m.view.selection.ranges, e);
        let old = m.view.selection.active;
        let active = shift_cell(old, e).unwrap_or(CellPos {
            row: if rows {
                e.at.saturating_sub(1)
            } else {
                old.row
            },
            col: if rows {
                old.col
            } else {
                e.at.saturating_sub(1)
            },
        });
        m.view.selection.ranges = if sel.is_empty() {
            vec![Range::cell(active.row, active.col)]
        } else {
            sel
        };
        m.view.selection.active = active;
        if rows {
            m.print.row_breaks = shift_breaks(&m.print.row_breaks, e);
        } else {
            m.print.col_breaks = shift_breaks(&m.print.col_breaks, e);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::meta::{CfKind, TableColumn, ValidationType};
    use crate::shift::edit;
    use crate::value::CellValue;
    use kiln_formula::refs::StructuralKind;

    fn sheet() -> Sheet {
        Sheet::new(SheetId(1), "Sheet1", 7.0)
    }

    #[test]
    fn has_excel_default_sizes() {
        let s = sheet();
        assert_eq!(s.rows().size_px(0), 20.0);
        assert_eq!(s.cols().size_px(0), 64.0);
        assert_eq!(s.default_col_width(), 9.140625);
    }

    #[test]
    fn validates_merges() {
        let mut s = sheet();
        assert!(s.check_merge(&Range::cell(0, 0)).is_err());
        s.meta.merges.push(Range::new(0, 0, 1, 1));
        assert!(s.check_merge(&Range::new(1, 1, 2, 2)).is_err());
        assert!(s.check_merge(&Range::new(3, 0, 3, 2)).is_ok());
        assert_eq!(s.merge_at(1, 0), Some(Range::new(0, 0, 1, 1)));
        assert_eq!(
            s.expand_to_merges(Range::new(1, 1, 2, 2)),
            Range::new(0, 0, 2, 2)
        );
    }

    #[test]
    fn resolves_effective_styles_from_cell_row_and_column_formats() {
        let mut s = sheet();
        s.cols.update(2, |p| p.style = Some(3));
        s.rows.update(5, |p| p.style = Some(4));
        s.cells.set_style(5, 2, 7);
        assert_eq!(s.style_at(0, 2), 3);
        assert_eq!(s.style_at(5, 0), 4);
        assert_eq!(s.style_at(5, 2), 7);
        assert_eq!(s.style_at(0, 0), 0);
    }

    #[test]
    fn looks_up_validation_conditional_formats_notes_and_links() {
        let mut s = sheet();
        let mut v1 = DataValidation::new(1, vec![Range::new(0, 0, 9, 0)], ValidationType::Whole);
        v1.formula1 = Some("1".into());
        let v2 = DataValidation::new(2, vec![Range::cell(3, 0)], ValidationType::List);
        s.meta.validations = vec![v1, v2];
        assert_eq!(s.validation_at(3, 0).map(|v| v.id), Some(2));
        assert_eq!(s.validation_at(4, 0).map(|v| v.id), Some(1));
        assert!(s.validation_at(0, 1).is_none());
        s.meta.cf.push(ConditionalFormat {
            ranges: vec![Range::new(0, 0, 5, 5)],
            rules: vec![
                CfRule {
                    id: 1,
                    priority: 2,
                    stop_if_true: false,
                    kind: CfKind::ContainsBlanks { dxf: 0 },
                },
                CfRule {
                    id: 2,
                    priority: 1,
                    stop_if_true: false,
                    kind: CfKind::ContainsErrors { dxf: 0 },
                },
            ],
        });
        let ids: Vec<u32> = s.cf_rules_at(2, 2).iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![2, 1]);
        s.meta.notes.push(Note {
            at: CellPos::new(1, 1),
            author: "A".into(),
            text: "n".into(),
            visible: false,
            size: None,
        });
        assert!(s.note_at(1, 1).is_some());
        s.meta.hyperlinks.push(Hyperlink {
            range: Range::new(0, 3, 1, 3),
            target: Some("https://example.org".into()),
            location: None,
            display: None,
            tooltip: None,
        });
        assert!(s.hyperlink_at(1, 3).is_some());
        assert!(s.hyperlink_at(2, 3).is_none());
    }

    #[test]
    fn shifts_all_metadata_on_structural_edits() {
        let mut s = sheet();
        s.cells.set_value(4, 0, CellValue::Number(1.0));
        s.rows.update(4, |p| p.hidden = true);
        s.meta.merges.push(Range::new(4, 0, 5, 1));
        s.meta.notes.push(Note {
            at: CellPos::new(6, 0),
            author: String::new(),
            text: "x".into(),
            visible: false,
            size: None,
        });
        s.meta.tables.push(Table {
            id: 1,
            name: "T".into(),
            display_name: None,
            range: Range::new(10, 0, 12, 1),
            header_row: true,
            totals_row: false,
            columns: vec![TableColumn::named("a"), TableColumn::named("b")],
            style: None,
            auto_filter: None,
            comment: None,
        });
        s.meta.view.freeze = Some(crate::meta::Freeze { rows: 5, cols: 0 });
        s.meta.view.selection.active = CellPos::new(8, 0);
        s.meta.print.row_breaks = vec![20];
        s.apply_structural(&edit(StructuralKind::InsertRows, 2, 3))
            .expect("allowed");
        assert_eq!(s.cells().value(7, 0), &CellValue::Number(1.0));
        assert!(s.is_row_hidden(7));
        assert_eq!(s.merges(), &[Range::new(7, 0, 8, 1)]);
        assert_eq!(s.meta().notes[0].at, CellPos::new(9, 0));
        assert_eq!(s.tables()[0].range, Range::new(13, 0, 15, 1));
        assert_eq!(s.meta().view.freeze.map(|f| f.rows), Some(8));
        assert_eq!(s.meta().view.selection.active, CellPos::new(11, 0));
        assert_eq!(s.meta().print.row_breaks, vec![23]);
        // Deleting the header row of the table is refused and changes nothing.
        let before = s.meta().clone();
        assert!(
            s.apply_structural(&edit(StructuralKind::DeleteRows, 13, 1))
                .is_err()
        );
        assert_eq!(s.meta(), &before);
    }
}
