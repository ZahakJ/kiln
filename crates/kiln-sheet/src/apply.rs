// SPDX-License-Identifier: AGPL-3.0-or-later
//! Applying [`Command`]s: validation, mutation and the exact inverse.

use kiln_formula::SheetId;
use kiln_formula::refs::{
    MoveSpec, StructuralEdit, StructuralKind, copy_formula, move_formula, remove_sheet_in_formula,
    rename_sheet_in_formula, shift_formula,
};
use rustc_hash::{FxHashMap, FxHashSet};

use crate::address::{CellPos, MAX_COLS, MAX_ROWS, Range, check_cell};
use crate::axis::{ColProps, RowProps};
use crate::commands::{
    CellInput, ClearWhat, ColPatch, Command, Content, FormatFrom, MergeMode, MetaField, PasteOp,
    PasteWhat, RowPatch, SheetCells, SheetLayoutEntry, SheetMetaEntry, SortKey, TablePatch,
};
use crate::error::{ModelError, Result};
use crate::meta::{
    DefinedName, Freeze, SheetProtection, SheetVisibility, Table, TableColumn, WorkbookProps,
};
use crate::shift::{edit as make_edit, fresh_column_names};
use crate::store::CellKind;
use crate::structured::{
    rename_table_column_in_formula, rename_table_in_formula, structured_to_a1,
};
use crate::styles::{AlignmentPatch, HAlign, Patch, StylePatch};
use crate::value::CellValue;
use crate::workbook::{CellRecord, Workbook, check_sheet_name, hidden_rows, strip_equals};

type Records = FxHashMap<SheetId, Vec<CellRecord>>;

fn add_records(map: &mut Records, sheet: SheetId, recs: impl IntoIterator<Item = CellRecord>) {
    map.entry(sheet).or_default().extend(recs);
}

/// Keeps the first record per cell.
fn dedupe(records: Vec<CellRecord>) -> Vec<CellRecord> {
    let mut seen = FxHashSet::default();
    records
        .into_iter()
        .filter(|r| seen.insert((r.row, r.col)))
        .collect()
}

fn restore_cells(map: Records) -> Vec<SheetCells> {
    let mut v: Vec<SheetCells> = map
        .into_iter()
        .map(|(sheet, records)| SheetCells {
            sheet,
            records: dedupe(records),
        })
        .collect();
    v.sort_by_key(|c| c.sheet.0);
    v
}

fn restore(cells: Vec<SheetCells>) -> Command {
    Command::Restore {
        cells,
        meta: Vec::new(),
        layout: Vec::new(),
        names: None,
    }
}

fn batch(commands: Vec<Command>) -> Command {
    Command::Batch {
        commands,
        label: None,
    }
}

fn axis_max(kind: StructuralKind) -> u32 {
    if kind.is_rows() { MAX_ROWS } else { MAX_COLS }
}

impl Workbook {
    /// Applies a command and returns its inverse. On an error nothing changed. In automatic
    /// calculation mode the workbook is recalculated when the outermost command ends.
    pub fn apply(&mut self, cmd: &Command) -> Result<Command> {
        self.calc.depth += 1;
        let r = self.apply_inner(cmd);
        self.calc.depth -= 1;
        if self.calc.depth == 0 {
            self.command_done();
        }
        r
    }

    /// Fills in what a command would otherwise choose when applied (new sheet names, ids and
    /// positions, new table ids), so replaying it (redo, journals, collaboration) produces
    /// exactly the same workbook.
    pub fn resolve(&self, cmd: &Command) -> Command {
        let mut next_sheet = self.next_sheet_id;
        let mut next_table = self.next_table_id;
        let mut names: Vec<String> = self
            .sheets()
            .iter()
            .map(|s| s.name.to_uppercase())
            .collect();
        let mut count = self.sheets().len();
        fn walk(
            c: &Command,
            next_sheet: &mut u32,
            next_table: &mut u32,
            names: &mut Vec<String>,
            count: &mut usize,
        ) -> Command {
            match c {
                Command::Batch { commands, label } => Command::Batch {
                    commands: commands
                        .iter()
                        .map(|x| walk(x, next_sheet, next_table, names, count))
                        .collect(),
                    label: label.clone(),
                },
                Command::AddSheet { name, index, id } => {
                    let name = name.clone().unwrap_or_else(|| {
                        (1..)
                            .map(|i| format!("Sheet{i}"))
                            .find(|n| !names.contains(&n.to_uppercase()))
                            .unwrap_or_default()
                    });
                    names.push(name.to_uppercase());
                    let id = id.unwrap_or_else(|| {
                        let v = *next_sheet;
                        *next_sheet += 1;
                        v
                    });
                    let out = Command::AddSheet {
                        name: Some(name),
                        index: Some(index.unwrap_or(*count)),
                        id: Some(id),
                    };
                    *count += 1;
                    out
                }
                Command::AddTable { sheet, table } if table.id == 0 => {
                    let mut t = (**table).clone();
                    t.id = *next_table;
                    *next_table += 1;
                    Command::AddTable {
                        sheet: *sheet,
                        table: Box::new(t),
                    }
                }
                other => other.clone(),
            }
        }
        walk(
            cmd,
            &mut next_sheet,
            &mut next_table,
            &mut names,
            &mut count,
        )
    }

    pub(crate) fn apply_inner(&mut self, cmd: &Command) -> Result<Command> {
        match cmd {
            Command::SetCells { sheet, cells } => self.set_cells(*sheet, cells),
            Command::SetRange {
                sheet,
                row,
                col,
                values,
            } => self.set_range(*sheet, *row, *col, values),
            Command::ClearRange {
                sheet,
                ranges,
                what,
            } => self.clear_range(*sheet, ranges, *what),
            Command::SetStyle {
                sheet,
                ranges,
                patch,
            } => {
                self.check_format_allowed(*sheet)?;
                self.set_style_with(*sheet, ranges, &mut |wb, base, cache| {
                    *cache
                        .entry(base)
                        .or_insert_with(|| wb.styles.patch(base, patch))
                })
            }
            Command::SetStyleIndex {
                sheet,
                ranges,
                style,
            } => {
                self.styles.check(*style)?;
                self.check_format_allowed(*sheet)?;
                let s = *style;
                self.set_style_with(*sheet, ranges, &mut |_, _, _| s)
            }
            Command::InsertRows {
                sheet,
                at,
                count,
                format_from,
            } => self.structural(
                *sheet,
                make_edit(StructuralKind::InsertRows, *at, *count),
                *format_from,
            ),
            Command::InsertCols {
                sheet,
                at,
                count,
                format_from,
            } => self.structural(
                *sheet,
                make_edit(StructuralKind::InsertCols, *at, *count),
                *format_from,
            ),
            Command::DeleteRows { sheet, at, count } => self.structural(
                *sheet,
                make_edit(StructuralKind::DeleteRows, *at, *count),
                FormatFrom::None,
            ),
            Command::DeleteCols { sheet, at, count } => self.structural(
                *sheet,
                make_edit(StructuralKind::DeleteCols, *at, *count),
                FormatFrom::None,
            ),
            Command::MoveRange {
                sheet,
                range,
                to_sheet,
                to,
            } => self.move_range(*sheet, *range, to_sheet.unwrap_or(*sheet), *to),
            Command::CopyRange {
                sheet,
                range,
                to_sheet,
                to,
                what,
                op,
                transpose,
                skip_blanks,
            } => self.copy_range(
                *sheet,
                *range,
                to_sheet.unwrap_or(*sheet),
                *to,
                *what,
                *op,
                *transpose,
                *skip_blanks,
            ),
            Command::Sort {
                sheet,
                range,
                keys,
                header,
                case_sensitive,
                by_columns,
                locale,
            } => self.sort_range(
                *sheet,
                *range,
                keys,
                *header,
                *case_sensitive,
                *by_columns,
                locale.as_deref(),
            ),
            Command::AutoFill {
                sheet,
                source,
                target,
                mode,
            } => {
                let records = crate::fill::autofill_records(self, *sheet, *source, *target, *mode)?;
                self.write_records_cmd(*sheet, records)
            }
            Command::ApplyFilter { sheet } => {
                let entries = crate::filter::filter_row_entries(self, *sheet)?;
                self.apply_inner(&Command::SetRowEntries {
                    sheet: *sheet,
                    entries,
                })
            }
            Command::RemoveDuplicates {
                sheet,
                range,
                columns,
                header,
            } => {
                let records =
                    crate::data::remove_duplicates_records(self, *sheet, *range, columns, *header)?;
                self.write_records_cmd(*sheet, records)
            }
            Command::TextToColumns {
                sheet,
                range,
                options,
            } => {
                let records = crate::data::text_to_columns_records(self, *sheet, *range, options)?;
                self.write_records_cmd(*sheet, records)
            }
            Command::AddSheet { name, index, id } => {
                self.add_sheet_cmd(name.as_deref(), *index, id.map(SheetId))
            }
            Command::RemoveSheet { sheet } => self.remove_sheet_cmd(*sheet),
            Command::RestoreSheet { snapshot, index } => {
                let (sheet, formulas) = (**snapshot).clone().into_sheet(self.mdw);
                let id = sheet.id();
                self.insert_sheet_object(sheet, formulas, *index)?;
                Ok(Command::RemoveSheet { sheet: id })
            }
            Command::RenameSheet { sheet, name } => self.rename_sheet_cmd(*sheet, name),
            Command::MoveSheet { sheet, index } => self.move_sheet_cmd(*sheet, *index),
            Command::SetRowProps {
                sheet,
                at,
                count,
                patch,
            } => self.set_row_props(*sheet, *at, *count, patch),
            Command::SetColProps {
                sheet,
                at,
                count,
                patch,
            } => self.set_col_props(*sheet, *at, *count, patch),
            Command::SetRowEntries { sheet, entries } => self.set_row_entries(*sheet, entries),
            Command::SetColEntries { sheet, entries } => self.set_col_entries(*sheet, entries),
            Command::Merge { sheet, range, mode } => self.merge_cmd(*sheet, *range, *mode),
            Command::Unmerge { sheet, range } => self.unmerge_cmd(*sheet, *range),
            Command::DefineName {
                name,
                formula,
                scope,
                hidden,
                comment,
            } => self.define_name_cmd(name, formula, *scope, *hidden, comment.clone()),
            Command::DeleteName { name, scope } => self.delete_name_cmd(name, *scope),
            Command::AddTable { sheet, table } => self.add_table_cmd(*sheet, table),
            Command::RemoveTable { name } => self.remove_table_cmd(name),
            Command::UpdateTable { name, patch } => self.update_table_cmd(name, patch),
            Command::SetFreeze { sheet, rows, cols } => {
                if *rows >= MAX_ROWS || *cols >= MAX_COLS {
                    return Err(ModelError::invalid("invalid freeze position"));
                }
                let mut view = self.sheet_ok(*sheet)?.meta.view.clone();
                if *rows == 0 && *cols == 0 {
                    view.freeze = None;
                } else {
                    view.freeze = Some(Freeze {
                        rows: *rows,
                        cols: *cols,
                    });
                    view.split = None;
                }
                self.set_sheet_meta(*sheet, MetaField::View(view))
            }
            Command::SetSheetMeta { sheet, field } => {
                self.set_sheet_meta(*sheet, (**field).clone())
            }
            Command::SetProps { props } => self.set_props(props),
            Command::SetWorkbookProtection { protection } => {
                let prev = std::mem::replace(&mut self.protection, protection.clone());
                self.changes.props = true;
                Ok(Command::SetWorkbookProtection { protection: prev })
            }
            Command::Batch { commands, label } => {
                let mut inverses = Vec::with_capacity(commands.len());
                for c in commands {
                    match self.apply_inner(c) {
                        Ok(inv) => inverses.push(inv),
                        Err(e) => {
                            // Roll back what was applied so a failing batch changes nothing.
                            for inv in inverses.iter().rev() {
                                let _ = self.apply_inner(inv);
                            }
                            return Err(e);
                        }
                    }
                }
                inverses.reverse();
                Ok(Command::Batch {
                    commands: inverses,
                    label: label.clone(),
                })
            }
            Command::Restore {
                cells,
                meta,
                layout,
                names,
            } => self.restore_cmd(cells, meta, layout, names.as_ref()),
        }
    }

    // ── protection ─────────────────────────────────────────────────────────

    fn protection_of(&self, sheet: SheetId) -> Result<Option<SheetProtection>> {
        Ok(self.sheet_ok(sheet)?.meta.protection.clone())
    }

    fn check_permission(
        &self,
        sheet: SheetId,
        allowed: fn(&SheetProtection) -> bool,
        what: &str,
    ) -> Result<()> {
        match self.protection_of(sheet)? {
            Some(p) if !allowed(&p) => Err(ModelError::Protected(format!(
                "the sheet is protected: {what} is not allowed"
            ))),
            _ => Ok(()),
        }
    }

    fn check_format_allowed(&self, sheet: SheetId) -> Result<()> {
        self.check_permission(sheet, |p| p.format_cells, "formatting cells")
    }

    /// Refuses edits to locked cells of a protected sheet.
    pub fn check_editable(&self, sheet: SheetId, ranges: &[Range]) -> Result<()> {
        let s = self.sheet_ok(sheet)?;
        if s.meta.protection.is_none() {
            return Ok(());
        }
        let locked = |style: u32| self.styles.xf(style).protection.locked;
        let refuse = || {
            Err(ModelError::Protected(
                "the cell is on a protected sheet; unprotect the sheet to change it".into(),
            ))
        };
        for r in ranges {
            let mut stored = 0u64;
            for (_, _, d) in s.cells.iter_range(*r) {
                stored += 1;
                if locked(d.style) {
                    return refuse();
                }
            }
            if stored < r.cell_count() {
                // Cells without a stored format use their row or column format.
                let rows_unlocked = (r.r0..=r.r1).all(|i| {
                    s.rows
                        .get(i)
                        .and_then(|p| p.style)
                        .is_some_and(|st| !locked(st))
                });
                let cols_unlocked = !rows_unlocked
                    && (r.c0..=r.c1).all(|i| {
                        s.cols
                            .get(i)
                            .and_then(|p| p.style)
                            .is_some_and(|st| !locked(st))
                    });
                if !rows_unlocked && !cols_unlocked {
                    return refuse();
                }
            }
        }
        Ok(())
    }

    fn check_structure_unlocked(&self) -> Result<()> {
        if self.protection.as_ref().is_some_and(|p| p.lock_structure) {
            return Err(ModelError::Protected(
                "the workbook structure is protected".into(),
            ));
        }
        Ok(())
    }

    // ── helpers ────────────────────────────────────────────────────────────

    pub(crate) fn all_meta(&self) -> Vec<SheetMetaEntry> {
        self.sheets()
            .iter()
            .map(|s| SheetMetaEntry {
                sheet: s.id(),
                meta: Box::new(s.meta.clone()),
            })
            .collect()
    }

    fn meta_entry(&self, sheet: SheetId) -> Result<SheetMetaEntry> {
        Ok(SheetMetaEntry {
            sheet,
            meta: Box::new(self.sheet_ok(sheet)?.meta.clone()),
        })
    }

    /// Every formula cell: sheet, position, text without `=`.
    fn formula_cells(&self) -> Vec<(SheetId, String, u32, u32, String)> {
        let mut out = Vec::new();
        for s in self.sheets() {
            for (r, c, d) in s.cells.iter() {
                if d.kind.is_formula()
                    && let Some(t) = self.formula(s.id(), r, c)
                {
                    out.push((s.id(), s.name.clone(), r, c, t));
                }
            }
        }
        out
    }

    /// Records (as they are now) of formula cells whose text `f` would change.
    fn changed_formula_records(
        &self,
        f: &dyn Fn(&str, &str, CellPos) -> Option<String>,
    ) -> Records {
        let mut out = Records::default();
        for (sid, sname, r, c, text) in self.formula_cells() {
            if f(&text, &sname, CellPos::new(r, c)).is_some_and(|n| strip_equals(&n) != text) {
                add_records(&mut out, sid, [self.record(sid, r, c)]);
            }
        }
        out
    }

    /// Rewrites cell formulas through `f` (rebinding them) and returns their prior records.
    fn rewrite_cell_formulas(
        &mut self,
        f: &dyn Fn(&str, &str, CellPos) -> Option<String>,
    ) -> Records {
        let mut out = Records::default();
        for (sid, sname, r, c, text) in self.formula_cells() {
            if let Some(n) = f(&text, &sname, CellPos::new(r, c))
                && strip_equals(&n) != text
            {
                let before = self.record(sid, r, c);
                let legacy = before.legacy;
                add_records(&mut out, sid, [before]);
                self.bind_formula(sid, r, c, &n, legacy);
                self.note_cells(sid, Range::cell(r, c));
            }
        }
        if !out.is_empty() {
            self.calc.pending = true;
        }
        out
    }

    fn write_records(&mut self, sheet: SheetId, records: &[CellRecord]) -> Result<()> {
        for r in records {
            self.write_record(sheet, r)?;
        }
        Ok(())
    }

    /// Writes records (after checking them) and returns a restore of what was there.
    pub(crate) fn write_records_cmd(
        &mut self,
        sheet: SheetId,
        records: Vec<CellRecord>,
    ) -> Result<Command> {
        let mut seen = FxHashSet::default();
        let mut ranges = Vec::new();
        for r in &records {
            check_cell(r.row, r.col)?;
            self.styles.check(r.style)?;
            if seen.insert((r.row, r.col)) {
                ranges.push(Range::cell(r.row, r.col));
            }
        }
        if ranges.len() <= 4096 {
            self.check_editable(sheet, &ranges)?;
        } else if let Some(b) = ranges.iter().copied().reduce(|a, b| a.union(&b)) {
            self.check_editable(sheet, &[b])?;
        }
        let before: Vec<CellRecord> = ranges
            .iter()
            .map(|r| self.record(sheet, r.r0, r.c0))
            .collect();
        self.write_records(sheet, &records)?;
        Ok(restore(vec![SheetCells {
            sheet,
            records: before,
        }]))
    }

    // ── cells ──────────────────────────────────────────────────────────────

    fn set_cells(&mut self, sheet: SheetId, cells: &[CellInput]) -> Result<Command> {
        self.sheet_ok(sheet)?;
        let mut prepared = Vec::with_capacity(cells.len());
        for c in cells {
            check_cell(c.row, c.col)?;
            if let Some(st) = c.style {
                self.styles.check(st)?;
            }
            let prev = self.record(sheet, c.row, c.col);
            let s = self.sheet_ok(sheet)?;
            let exists = s.cells.contains(c.row, c.col);
            let style = c.style.unwrap_or(if exists {
                prev.style
            } else {
                s.style_at(c.row, c.col)
            });
            let rec = match &c.content {
                Content::Keep => CellRecord { style, ..prev },
                Content::Value(v) => CellRecord {
                    row: c.row,
                    col: c.col,
                    value: v.clone(),
                    formula: None,
                    legacy: false,
                    style,
                },
                Content::Formula(f) => CellRecord {
                    row: c.row,
                    col: c.col,
                    value: CellValue::Blank,
                    formula: Some(Workbook::check_formula(f)?),
                    legacy: false,
                    style,
                },
                Content::Clear => CellRecord {
                    row: c.row,
                    col: c.col,
                    value: CellValue::Blank,
                    formula: None,
                    legacy: false,
                    style,
                },
                Content::Typed(text) => {
                    match text.strip_prefix('=').filter(|f| !f.trim().is_empty()) {
                        Some(f) => CellRecord {
                            row: c.row,
                            col: c.col,
                            value: CellValue::Blank,
                            formula: Some(Workbook::check_formula(f)?),
                            legacy: false,
                            style,
                        },
                        None => {
                            let parsed = crate::data::parse_input(text, self.props.date1904);
                            let style = match parsed.num_fmt {
                                Some(code) if self.styles.num_fmt(style) == "General" => {
                                    self.styles.patch(
                                        style,
                                        &StylePatch {
                                            num_fmt: Some(code),
                                            ..StylePatch::default()
                                        },
                                    )
                                }
                                _ => style,
                            };
                            CellRecord {
                                row: c.row,
                                col: c.col,
                                value: parsed.value,
                                formula: None,
                                legacy: false,
                                style,
                            }
                        }
                    }
                }
            };
            prepared.push(rec);
        }
        let ranges: Vec<Range> = prepared.iter().map(|p| Range::cell(p.row, p.col)).collect();
        if ranges.len() <= 4096 {
            self.check_editable(sheet, &ranges)?;
        } else if let Some(b) = ranges.iter().copied().reduce(|a, b| a.union(&b)) {
            self.check_editable(sheet, &[b])?;
        }
        let before = dedupe(
            prepared
                .iter()
                .map(|p| self.record(sheet, p.row, p.col))
                .collect(),
        );
        let meta_before = self.all_meta();
        let names_before = self.names.clone();
        self.write_records(sheet, &prepared)?;
        // Header cells rename their table column; typing next to a table extends it.
        let mut formula_changes = Records::default();
        let mut tables_changed = false;
        for p in &prepared {
            if let Some(changes) = self.sync_table_header(sheet, p)? {
                tables_changed = true;
                for (sid, recs) in changes {
                    add_records(&mut formula_changes, sid, recs);
                }
            }
        }
        if self.auto_expand_tables(sheet, &prepared) {
            tables_changed = true;
        }
        if !tables_changed {
            return Ok(restore(vec![SheetCells {
                sheet,
                records: before,
            }]));
        }
        self.tables_changed();
        self.note_meta(sheet);
        add_records(&mut formula_changes, sheet, before);
        Ok(Command::Restore {
            cells: restore_cells(formula_changes),
            meta: meta_before,
            layout: Vec::new(),
            names: Some(names_before),
        })
    }

    /// Keeps a table column's name equal to its header cell's text (Excel's rule), rewriting
    /// structured references to the column.
    fn sync_table_header(&mut self, sheet: SheetId, p: &CellRecord) -> Result<Option<Records>> {
        let s = self.sheet_ok(sheet)?;
        let Some(ti) =
            s.meta.tables.iter().position(|t| {
                t.header_row && t.range.r0 == p.row && t.range.contains(p.row, p.col)
            })
        else {
            return Ok(None);
        };
        let t = &s.meta.tables[ti];
        let idx = (p.col - t.range.c0) as usize;
        let Some(col) = t.columns.get(idx) else {
            return Ok(None);
        };
        let shown = self.value(sheet, p.row, p.col).display_general();
        if shown == col.name {
            return Ok(None);
        }
        let others: Vec<String> = t
            .columns
            .iter()
            .enumerate()
            .filter(|(i, _)| *i != idx)
            .map(|(_, c)| c.name.to_uppercase())
            .collect();
        let base = if shown.is_empty() {
            format!("Column{}", idx + 1)
        } else {
            shown.clone()
        };
        let mut name = base.clone();
        let mut k = 2;
        while others.contains(&name.to_uppercase()) {
            name = format!("{base}{k}");
            k += 1;
        }
        let old = col.name.clone();
        let (tname, trange, sname) = (t.name.clone(), t.range, s.name.clone());
        self.sheet_mut(sheet)?.meta.tables[ti].columns[idx].name = name.clone();
        if name != shown {
            let style = self.sheet_ok(sheet)?.cells.style(p.row, p.col);
            self.write_record(
                sheet,
                &CellRecord {
                    row: p.row,
                    col: p.col,
                    value: CellValue::text(name.as_str()),
                    formula: None,
                    legacy: false,
                    style,
                },
            )?;
        }
        let f = |text: &str, fs: &str, at: Option<CellPos>| {
            let inside = fs.eq_ignore_ascii_case(&sname)
                && at.is_some_and(|a| trange.contains(a.row, a.col));
            rename_table_column_in_formula(text, &tname, &old, &name, inside)
        };
        let changes = self.rewrite_cell_formulas(&|t, fs, at| f(t, fs, Some(at)));
        self.rewrite_model_formulas(&|t, fs, at| f(t, fs, at));
        Ok(Some(changes))
    }

    /// Excel's "include new rows and columns in table": a value typed just below a table (with
    /// no totals row), or a header typed just right of its header row, extends the table.
    fn auto_expand_tables(&mut self, sheet: SheetId, edits: &[CellRecord]) -> bool {
        let mut changed = false;
        for e in edits.iter().filter(|e| e.has_content()) {
            let Ok(s) = self.sheet_ok(sheet) else {
                return changed;
            };
            if s.table_at(e.row, e.col).is_some() || s.merge_at(e.row, e.col).is_some() {
                continue;
            }
            // A row typed just below the table (without a totals row), or a header typed just
            // right of the header row.
            let hit = s.meta.tables.iter().position(|t| {
                (!t.totals_row
                    && e.row == t.range.r1 + 1
                    && e.col >= t.range.c0
                    && e.col <= t.range.c1)
                    || (t.header_row && e.col == t.range.c1 + 1 && e.row == t.range.r0)
            });
            let Some(ti) = hit else { continue };
            let t = &s.meta.tables[ti];
            let mut next = t.clone();
            if e.row == t.range.r1 + 1 {
                next.range.r1 += 1;
            } else {
                next.range.c1 += 1;
                let header = if t.header_row {
                    self.value(sheet, t.range.r0, e.col).display_general()
                } else {
                    String::new()
                };
                let names: Vec<String> = t.columns.iter().map(|c| c.name.clone()).collect();
                let name =
                    if header.is_empty() || names.iter().any(|n| n.eq_ignore_ascii_case(&header)) {
                        fresh_column_names(&names, 1).pop().unwrap_or_default()
                    } else {
                        header
                    };
                next.columns.push(TableColumn::named(name));
            }
            let blocked = s
                .meta
                .tables
                .iter()
                .enumerate()
                .any(|(i, o)| i != ti && o.range.intersects(&next.range))
                || s.meta.merges.iter().any(|m| m.intersects(&next.range));
            if blocked || next.range.check().is_err() {
                continue;
            }
            let header_cell = (next.header_row && next.range.c1 != t.range.c1).then(|| {
                (
                    next.range.r0,
                    next.range.c1,
                    next.columns.last().map(|c| c.name.clone()),
                )
            });
            if let Ok(sm) = self.sheet_mut(sheet) {
                sm.meta.tables[ti] = next;
            }
            if let Some((r, c, Some(name))) = header_cell
                && self.value(sheet, r, c).display_general() != name
            {
                let style = self.sheet(sheet).map_or(0, |s| s.cells.style(r, c));
                let _ = self.write_record(
                    sheet,
                    &CellRecord {
                        row: r,
                        col: c,
                        value: CellValue::text(name.as_str()),
                        formula: None,
                        legacy: false,
                        style,
                    },
                );
            }
            changed = true;
        }
        changed
    }

    fn set_range(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        values: &[Vec<Option<CellValue>>],
    ) -> Result<Command> {
        let rows = values.len() as u32;
        let cols = values.iter().map(Vec::len).max().unwrap_or(0) as u32;
        if rows == 0 || cols == 0 {
            return Ok(Command::noop());
        }
        let area = Range {
            r0: row,
            c0: col,
            r1: row.saturating_add(rows - 1),
            c1: col.saturating_add(cols - 1),
        };
        area.check()?;
        self.check_editable(sheet, &[area])?;
        let s = self.sheet_ok(sheet)?;
        let mut records = Vec::new();
        for (i, line) in values.iter().enumerate() {
            for (j, v) in line.iter().enumerate() {
                let Some(v) = v else { continue };
                let (r, c) = (row + i as u32, col + j as u32);
                records.push(CellRecord {
                    row: r,
                    col: c,
                    value: v.clone(),
                    formula: None,
                    legacy: false,
                    style: s.style_at(r, c),
                });
            }
        }
        self.write_records_cmd(sheet, records)
    }

    fn clear_range(
        &mut self,
        sheet: SheetId,
        ranges: &[Range],
        what: ClearWhat,
    ) -> Result<Command> {
        for r in ranges {
            r.check()?;
        }
        if matches!(what, ClearWhat::All | ClearWhat::Contents) {
            self.check_editable(sheet, ranges)?;
        }
        if matches!(what, ClearWhat::All | ClearWhat::Formats) {
            self.check_format_allowed(sheet)?;
        }
        let meta = self.meta_entry(sheet)?;
        let mut before = Vec::new();
        if matches!(
            what,
            ClearWhat::All | ClearWhat::Contents | ClearWhat::Formats
        ) {
            for r in ranges {
                for rec in self.records_in(sheet, *r) {
                    let next = match what {
                        ClearWhat::Contents => CellRecord {
                            value: CellValue::Blank,
                            formula: None,
                            legacy: false,
                            ..rec.clone()
                        },
                        ClearWhat::Formats => CellRecord {
                            style: 0,
                            ..rec.clone()
                        },
                        _ => CellRecord::empty(rec.row, rec.col),
                    };
                    before.push(rec);
                    self.write_record(sheet, &next)?;
                }
            }
        }
        let in_any = |p: CellPos| ranges.iter().any(|r| r.contains(p.row, p.col));
        let s = self.sheet_mut(sheet)?;
        let mut meta_changed = false;
        if matches!(what, ClearWhat::All | ClearWhat::Notes) {
            let n0 = s.meta.notes.len() + s.meta.threads.len();
            s.meta.notes.retain(|n| !in_any(n.at));
            s.meta.threads.retain(|t| !in_any(t.at));
            meta_changed |= n0 != s.meta.notes.len() + s.meta.threads.len();
        }
        if matches!(what, ClearWhat::All | ClearWhat::Hyperlinks) {
            let n0 = s.meta.hyperlinks.len();
            s.meta
                .hyperlinks
                .retain(|h| !ranges.iter().any(|r| h.range.is_inside(r)));
            meta_changed |= n0 != s.meta.hyperlinks.len();
        }
        if matches!(what, ClearWhat::All | ClearWhat::Formats) {
            let n0 = s.meta.merges.len();
            s.meta
                .merges
                .retain(|m| !ranges.iter().any(|r| m.intersects(r)));
            meta_changed |= n0 != s.meta.merges.len();
        }
        if meta_changed {
            self.note_meta(sheet);
        }
        Ok(Command::Restore {
            cells: vec![SheetCells {
                sheet,
                records: dedupe(before),
            }],
            meta: if meta_changed { vec![meta] } else { Vec::new() },
            layout: Vec::new(),
            names: None,
        })
    }

    /// Applies a style mapping to ranges. Whole columns/rows set the column/row format (plus every
    /// existing cell in them), as Excel does; other ranges format each cell.
    fn set_style_with(
        &mut self,
        sheet: SheetId,
        ranges: &[Range],
        map: &mut StyleMap<'_>,
    ) -> Result<Command> {
        for r in ranges {
            r.check()?;
        }
        self.sheet_ok(sheet)?;
        let mut cache: FxHashMap<u32, u32> = FxHashMap::default();
        let mut before: FxHashMap<(u32, u32), CellRecord> = FxHashMap::default();
        let mut order: Vec<(u32, u32)> = Vec::new();
        let mut rows_before: Vec<(u32, Option<RowProps>)> = Vec::new();
        let mut cols_before: Vec<(u32, Option<ColProps>)> = Vec::new();
        let mut touched = Touched {
            sheet,
            before: &mut before,
            order: &mut order,
        };
        for r in ranges {
            let whole_cols = r.is_whole_cols();
            let whole_rows = r.is_whole_rows();
            if whole_cols && !whole_rows {
                for c in r.c0..=r.c1 {
                    let p = self.sheet_ok(sheet)?.cols.get(c).copied();
                    cols_before.push((c, p));
                    let base = p.and_then(|p| p.style).unwrap_or(0);
                    let next = map(self, base, &mut cache);
                    self.sheet_mut(sheet)?
                        .cols
                        .update(c, |p| p.style = Some(next));
                }
                let styled_rows: Vec<u32> = self
                    .sheet_ok(sheet)?
                    .rows
                    .entries()
                    .filter(|(_, p)| p.style.is_some())
                    .map(|(i, _)| i)
                    .collect();
                for row in styled_rows {
                    for c in r.c0..=r.c1 {
                        let base = self.sheet_ok(sheet)?.style_at(row, c);
                        touched.touch(self, row, c, base, &mut cache, map);
                    }
                }
                let cells: Vec<(u32, u32, u32)> = self
                    .sheet_ok(sheet)?
                    .cells
                    .iter_range(*r)
                    .map(|(a, b, d)| (a, b, d.style))
                    .collect();
                for (a, b, st) in cells {
                    touched.touch(self, a, b, st, &mut cache, map);
                }
            } else if whole_rows {
                for i in r.r0..=r.r1 {
                    let p = self.sheet_ok(sheet)?.rows.get(i).copied();
                    rows_before.push((i, p));
                    let base = p.and_then(|p| p.style).unwrap_or(0);
                    let next = map(self, base, &mut cache);
                    self.sheet_mut(sheet)?
                        .rows
                        .update(i, |p| p.style = Some(next));
                }
                if !whole_cols {
                    let styled_cols: Vec<(u32, u32)> = self
                        .sheet_ok(sheet)?
                        .cols
                        .entries()
                        .filter_map(|(i, p)| p.style.map(|s| (i, s)))
                        .collect();
                    for (col, cst) in styled_cols {
                        for i in r.r0..=r.r1 {
                            let base = self
                                .sheet_ok(sheet)?
                                .cells
                                .get(i, col)
                                .map_or(cst, |d| d.style);
                            touched.touch(self, i, col, base, &mut cache, map);
                        }
                    }
                }
                let cells: Vec<(u32, u32, u32)> = self
                    .sheet_ok(sheet)?
                    .cells
                    .iter_range(*r)
                    .map(|(a, b, d)| (a, b, d.style))
                    .collect();
                for (a, b, st) in cells {
                    touched.touch(self, a, b, st, &mut cache, map);
                }
            } else {
                for row in r.r0..=r.r1 {
                    for col in r.c0..=r.c1 {
                        let base = self.sheet_ok(sheet)?.style_at(row, col);
                        touched.touch(self, row, col, base, &mut cache, map);
                    }
                }
            }
        }
        for r in ranges {
            self.note_cells(sheet, *r);
        }
        let mut inv = vec![restore(vec![SheetCells {
            sheet,
            records: order
                .iter()
                .filter_map(|k| before.get(k).cloned())
                .collect(),
        }])];
        if !rows_before.is_empty() {
            self.note_layout(sheet);
            inv.push(Command::SetRowEntries {
                sheet,
                entries: rows_before,
            });
        }
        if !cols_before.is_empty() {
            self.note_layout(sheet);
            inv.push(Command::SetColEntries {
                sheet,
                entries: cols_before,
            });
        }
        Ok(if inv.len() == 1 {
            inv.pop().unwrap_or_else(Command::noop)
        } else {
            batch(inv)
        })
    }

    // ── structural edits ───────────────────────────────────────────────────

    fn structural(
        &mut self,
        sheet: SheetId,
        e: StructuralEdit,
        format_from: FormatFrom,
    ) -> Result<Command> {
        let rows = e.kind.is_rows();
        let insert = e.kind.is_insert();
        let max = axis_max(e.kind);
        if e.count == 0 {
            return Err(ModelError::invalid("count must be positive"));
        }
        if e.at >= max || e.count > max {
            return Err(ModelError::invalid("position out of range"));
        }
        if !insert && u64::from(e.at) + u64::from(e.count) > u64::from(max) {
            return Err(ModelError::invalid(
                "cannot delete past the end of the sheet",
            ));
        }
        match e.kind {
            StructuralKind::InsertRows => {
                self.check_permission(sheet, |p| p.insert_rows, "inserting rows")?
            }
            StructuralKind::InsertCols => {
                self.check_permission(sheet, |p| p.insert_columns, "inserting columns")?
            }
            StructuralKind::DeleteRows => {
                self.check_permission(sheet, |p| p.delete_rows, "deleting rows")?
            }
            StructuralKind::DeleteCols => {
                self.check_permission(sheet, |p| p.delete_columns, "deleting columns")?
            }
        }
        let lost = if rows {
            Range::rows(max - e.count, max - 1)
        } else {
            Range::cols(max - e.count, max - 1)
        };
        let s = self.sheet_ok(sheet)?;
        if insert && s.cells.iter_range(lost).any(|(_, _, d)| d.has_content()) {
            return Err(ModelError::invalid(
                "inserting would push non-empty cells off the end of the worksheet",
            ));
        }
        s.check_structural(&e)?;
        let removed = if insert {
            lost
        } else if rows {
            Range::rows(e.at, e.at + e.count - 1)
        } else {
            Range::cols(e.at, e.at + e.count - 1)
        };
        let meta_before = self.all_meta();
        let names_before = self.names.clone();
        let layout_before = s.layout();
        let sheet_name = s.name.clone();
        let removed_records = self.records_in(sheet, removed);
        let mut records = self
            .changed_formula_records(&|text, fs, _| shift_formula(text, &e, &sheet_name, fs).ok());
        add_records(&mut records, sheet, removed_records);
        self.apply_structural_raw(sheet, &e)?;
        if insert && format_from != FormatFrom::None {
            self.copy_neighbour_format(sheet, &e, format_from)?;
        }
        let opposite = match e.kind {
            StructuralKind::InsertRows => Command::DeleteRows {
                sheet,
                at: e.at,
                count: e.count,
            },
            StructuralKind::InsertCols => Command::DeleteCols {
                sheet,
                at: e.at,
                count: e.count,
            },
            StructuralKind::DeleteRows => Command::InsertRows {
                sheet,
                at: e.at,
                count: e.count,
                format_from: FormatFrom::None,
            },
            StructuralKind::DeleteCols => Command::InsertCols {
                sheet,
                at: e.at,
                count: e.count,
                format_from: FormatFrom::None,
            },
        };
        Ok(batch(vec![
            opposite,
            Command::Restore {
                cells: restore_cells(records),
                meta: meta_before,
                layout: vec![SheetLayoutEntry {
                    sheet,
                    layout: layout_before,
                }],
                names: Some(names_before),
            },
        ]))
    }

    /// Inserted rows/columns take the format of their neighbour.
    fn copy_neighbour_format(
        &mut self,
        sheet: SheetId,
        e: &StructuralEdit,
        from: FormatFrom,
    ) -> Result<()> {
        let max = axis_max(e.kind);
        let src = match from {
            FormatFrom::Before => e.at.checked_sub(1),
            FormatFrom::After => e.at.checked_add(e.count).filter(|&x| x < max),
            FormatFrom::None => None,
        };
        let Some(src) = src else { return Ok(()) };
        let end = e.at + e.count;
        let s = self.sheet_mut(sheet)?;
        if e.kind.is_rows() {
            if let Some(st) = s.rows.get(src).and_then(|p| p.style) {
                for i in e.at..end {
                    s.rows.update(i, |p| p.style = Some(st));
                }
            }
            let styled: Vec<(u32, u32)> = s
                .cells
                .chunk(src)
                .map(|ch| {
                    ch.iter()
                        .filter(|(_, d)| d.style != 0)
                        .map(|(c, d)| (c, d.style))
                        .collect()
                })
                .unwrap_or_default();
            for (c, st) in styled {
                for i in e.at..end {
                    s.cells.set_style(i, c, st);
                }
            }
            self.note_cells(sheet, Range::rows(e.at, end - 1));
        } else {
            if let Some(st) = s.cols.get(src).and_then(|p| p.style) {
                for i in e.at..end {
                    s.cols.update(i, |p| p.style = Some(st));
                }
            }
            let styled: Vec<(u32, u32)> = s
                .cells
                .iter_range(Range::cols(src, src))
                .filter(|(_, _, d)| d.style != 0)
                .map(|(r, _, d)| (r, d.style))
                .collect();
            for (r, st) in styled {
                for i in e.at..end {
                    s.cells.set_style(r, i, st);
                }
            }
            self.note_cells(sheet, Range::cols(e.at, end - 1));
        }
        Ok(())
    }

    // ── move and copy ──────────────────────────────────────────────────────

    fn move_range(&mut self, src: SheetId, r: Range, dst: SheetId, to: CellPos) -> Result<Command> {
        r.check()?;
        let d_row = i64::from(to.row) - i64::from(r.r0);
        let d_col = i64::from(to.col) - i64::from(r.c0);
        let dest = r
            .offset(d_row, d_col)
            .ok_or_else(|| ModelError::invalid("the destination is outside the sheet"))?;
        self.sheet_ok(dst)?;
        if d_row == 0 && d_col == 0 && src == dst {
            return Ok(Command::noop());
        }
        self.check_editable(src, &[r])?;
        self.check_editable(dst, &[dest])?;
        for (sid, area, is_src) in [(src, r, true), (dst, dest, false)] {
            let s = self.sheet_ok(sid)?;
            let moving_whole = |t: &Table| sid == src && t.range.is_inside(&r);
            if let Some(t) = s.meta.tables.iter().find(|t| {
                if is_src {
                    t.range.intersects(&r) && !t.range.is_inside(&r)
                } else {
                    t.range.intersects(&dest) && !moving_whole(t)
                }
            }) {
                return Err(ModelError::invalid(format!(
                    "the move would change part of table \"{}\"",
                    t.name
                )));
            }
            if is_src
                && s.meta
                    .merges
                    .iter()
                    .any(|m| m.intersects(&area) && !m.is_inside(&area))
            {
                return Err(ModelError::invalid("cannot move part of a merged cell"));
            }
        }
        let meta_before = self.all_meta();
        let names_before = self.names.clone();
        let src_records = self.records_in(src, r);
        let dst_records = self.records_in(dst, dest);
        let spec = MoveSpec {
            sheet: self.sheet_ok(src)?.name.clone(),
            from: r.to_bounds(),
            d_row,
            d_col,
            to_sheet: Some(self.sheet_ok(dst)?.name.clone()),
        };
        let mut records =
            self.changed_formula_records(&|text, fs, _| move_formula(text, &spec, fs).ok());
        // Store-level contents of the block (kinds, cached values, raw formula text).
        let block: Vec<(u32, u32, CellValue, CellKind, u32, Option<String>)> = {
            let s = self.sheet_ok(src)?;
            s.cells
                .iter_range(r)
                .map(|(row, col, d)| {
                    (
                        row,
                        col,
                        d.value.clone(),
                        d.kind,
                        d.style,
                        s.cells.raw_formula(row, col).map(|t| t.to_string()),
                    )
                })
                .collect()
        };
        let literal_positions: Vec<(SheetId, u32, u32)> = src_records
            .iter()
            .filter(|x| x.formula.is_none())
            .map(|x| (src, x.row, x.col))
            .chain(
                dst_records
                    .iter()
                    .filter(|x| x.formula.is_none())
                    .map(|x| (dst, x.row, x.col)),
            )
            .collect();
        // The engine moves formulas and rewrites every reference.
        self.engine
            .move_range(src, r.to_bounds(), Some(dst), dest.r0, dest.c0)
            .map_err(|e| ModelError::invalid(e.to_string()))?;
        self.sheet_mut(src)?.cells.clear_range(r);
        self.sheet_mut(dst)?.cells.clear_range(dest);
        {
            let s = self.sheet_mut(dst)?;
            for (row, col, v, k, st, raw) in block.iter().cloned() {
                let (nr, nc) = (
                    (i64::from(row) + d_row) as u32,
                    (i64::from(col) + d_col) as u32,
                );
                match raw {
                    Some(t) => {
                        s.cells.set(nr, nc, v.clone(), CellKind::Value, st);
                        s.cells.set_raw_formula(nr, nc, &t, v);
                    }
                    None => s.cells.set(nr, nc, v, k, st),
                }
            }
        }
        for (sid, row, col) in literal_positions {
            let _ = self.engine.cell_changed(sid, row, col);
        }
        for (row, col, ..) in &block {
            let (nr, nc) = (
                (i64::from(*row) + d_row) as u32,
                (i64::from(*col) + d_col) as u32,
            );
            let _ = self.engine.cell_changed(dst, nr, nc);
        }
        // Metadata attached to cells travels with them.
        let shift_r = |x: &Range| x.offset(d_row, d_col).unwrap_or(*x);
        let (notes, threads, merges, links, tables) = {
            let s = self.sheet_mut(src)?;
            let take = |v: &mut Vec<crate::meta::Note>| {
                let (a, b): (Vec<_>, Vec<_>) = std::mem::take(v)
                    .into_iter()
                    .partition(|n| r.contains(n.at.row, n.at.col));
                *v = b;
                a
            };
            let notes = take(&mut s.meta.notes);
            let (threads, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut s.meta.threads)
                .into_iter()
                .partition(|t| r.contains(t.at.row, t.at.col));
            s.meta.threads = keep;
            let (merges, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut s.meta.merges)
                .into_iter()
                .partition(|m| m.is_inside(&r));
            s.meta.merges = keep;
            let (links, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut s.meta.hyperlinks)
                .into_iter()
                .partition(|h| h.range.is_inside(&r));
            s.meta.hyperlinks = keep;
            let (tables, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut s.meta.tables)
                .into_iter()
                .partition(|t| t.range.is_inside(&r));
            s.meta.tables = keep;
            (notes, threads, merges, links, tables)
        };
        let moved_tables = !tables.is_empty();
        {
            let s = self.sheet_mut(dst)?;
            let in_dest = |p: CellPos| dest.contains(p.row, p.col);
            s.meta.notes.retain(|n| !in_dest(n.at));
            s.meta.threads.retain(|t| !in_dest(t.at));
            s.meta.merges.retain(|m| !m.intersects(&dest));
            s.meta.hyperlinks.retain(|h| !h.range.is_inside(&dest));
            let mv = |p: CellPos| {
                CellPos::new(
                    (i64::from(p.row) + d_row) as u32,
                    (i64::from(p.col) + d_col) as u32,
                )
            };
            s.meta.notes.extend(
                notes
                    .into_iter()
                    .map(|n| crate::meta::Note { at: mv(n.at), ..n }),
            );
            s.meta.threads.extend(
                threads
                    .into_iter()
                    .map(|t| crate::meta::ThreadedComment { at: mv(t.at), ..t }),
            );
            s.meta.merges.extend(merges.iter().map(shift_r));
            s.meta
                .hyperlinks
                .extend(links.into_iter().map(|h| crate::meta::Hyperlink {
                    range: shift_r(&h.range),
                    ..h
                }));
            s.meta.tables.extend(tables.into_iter().map(|t| Table {
                range: shift_r(&t.range),
                ..t
            }));
        }
        if moved_tables {
            self.tables_changed();
        }
        self.sync_names();
        self.note_cells(src, r);
        self.note_cells(dst, dest);
        self.note_meta(src);
        self.note_meta(dst);
        self.calc.pending = true;
        add_records(&mut records, src, with_empties(self, src, r, src_records));
        add_records(
            &mut records,
            dst,
            with_empties(self, dst, dest, dst_records),
        );
        Ok(Command::Restore {
            cells: restore_cells(records),
            meta: meta_before,
            layout: Vec::new(),
            names: Some(names_before),
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn copy_range(
        &mut self,
        src: SheetId,
        r: Range,
        dst: SheetId,
        target: Range,
        what: PasteWhat,
        op: PasteOp,
        transpose: bool,
        skip_blanks: bool,
    ) -> Result<Command> {
        r.check()?;
        target.check()?;
        let (src_rows, src_cols) = (r.height(), r.width());
        let (bh, bw) = if transpose {
            (src_cols, src_rows)
        } else {
            (src_rows, src_cols)
        };
        // Excel tiles the block when the destination is a whole multiple of it.
        let tiles_v = if target.height().is_multiple_of(bh) {
            target.height() / bh
        } else {
            1
        };
        let tiles_h = if target.width().is_multiple_of(bw) {
            target.width() / bw
        } else {
            1
        };
        let dest = Range {
            r0: target.r0,
            c0: target.c0,
            r1: target.r0.saturating_add(bh * tiles_v - 1),
            c1: target.c0.saturating_add(bw * tiles_h - 1),
        };
        dest.check()?;
        self.check_editable(dst, &[dest])?;
        let paste_formats = matches!(
            what,
            PasteWhat::All | PasteWhat::Formats | PasteWhat::AllExceptBorders
        );
        if paste_formats
            && let Some(t) = self
                .sheet_ok(dst)?
                .meta
                .tables
                .iter()
                .find(|t| t.range.intersects(&dest) && !dest.is_inside(&t.range))
            && self
                .sheet_ok(src)?
                .meta
                .merges
                .iter()
                .any(|m| m.is_inside(&r))
        {
            return Err(ModelError::invalid(format!(
                "cannot paste merged cells into table \"{}\"",
                t.name
            )));
        }
        // Snapshot the source first (source and destination may overlap).
        let src_cells: FxHashMap<(u32, u32), CellRecord> = self
            .records_in(src, r)
            .into_iter()
            .map(|c| ((c.row - r.r0, c.col - r.c0), c))
            .collect();
        let src_style = |wb: &Workbook, i: u32, j: u32| {
            wb.sheet(src).map_or(0, |s| s.style_at(r.r0 + i, r.c0 + j))
        };
        let meta_before = self.meta_entry(dst)?;
        let mut before: Vec<CellRecord> = Vec::new();
        let mut seen = FxHashSet::default();
        let mut writes = Vec::new();
        for ti in 0..tiles_v {
            for tj in 0..tiles_h {
                for bi in 0..bh {
                    for bj in 0..bw {
                        let (i, j) = if transpose { (bj, bi) } else { (bi, bj) };
                        let row = dest.r0 + ti * bh + bi;
                        let col = dest.c0 + tj * bw + bj;
                        let s = src_cells.get(&(i, j));
                        if skip_blanks && s.is_none_or(|c| !c.has_content()) {
                            continue;
                        }
                        let prev = self.record(dst, row, col);
                        if seen.insert((row, col)) {
                            before.push(prev.clone());
                        }
                        let exists = self.sheet_ok(dst)?.cells.contains(row, col);
                        let mut next = CellRecord {
                            style: if exists {
                                prev.style
                            } else {
                                self.sheet_ok(dst)?.style_at(row, col)
                            },
                            ..prev.clone()
                        };
                        let (sr, sc) = (r.r0 + i, r.c0 + j);
                        let d_row = i64::from(row) - i64::from(sr);
                        let d_col = i64::from(col) - i64::from(sc);
                        let src_value = || s.map_or(CellValue::Blank, |c| c.value.clone());
                        match what {
                            PasteWhat::All
                            | PasteWhat::Formulas
                            | PasteWhat::FormulasAndNumberFormats
                            | PasteWhat::AllExceptBorders => {
                                next.value = src_value();
                                next.legacy = s.is_some_and(|c| c.legacy);
                                next.formula = s.and_then(|c| c.formula.as_ref()).map(|f| {
                                    copy_formula(f, d_row, d_col)
                                        .map_or_else(|_| f.clone(), |t| strip_equals(&t).to_owned())
                                });
                            }
                            PasteWhat::Values | PasteWhat::ValuesAndNumberFormats => {
                                let live = self.value(src, sr, sc);
                                next.value = live;
                                next.formula = None;
                                next.legacy = false;
                            }
                            PasteWhat::Formats | PasteWhat::Comments => {}
                        }
                        if op != PasteOp::None
                            && !matches!(what, PasteWhat::Formats | PasteWhat::Comments)
                            && next.formula.is_none()
                        {
                            let sv = self.value(src, sr, sc);
                            if let Some(v) = paste_op(op, &prev, &sv) {
                                next.value = v;
                            } else if prev.has_content() && sv.is_blank() {
                                next = CellRecord {
                                    style: next.style,
                                    ..prev.clone()
                                };
                            }
                        }
                        let sst = s.map_or_else(|| src_style(self, i, j), |c| c.style);
                        match what {
                            PasteWhat::All | PasteWhat::Formats => next.style = sst,
                            PasteWhat::AllExceptBorders => {
                                let mut st = self.styles.get(sst);
                                st.border = self.styles.get(next.style).border;
                                next.style = self.styles.intern(&st);
                            }
                            PasteWhat::ValuesAndNumberFormats
                            | PasteWhat::FormulasAndNumberFormats => {
                                let code = self.styles.num_fmt(sst).to_owned();
                                next.style = self.styles.patch(
                                    next.style,
                                    &StylePatch {
                                        num_fmt: Some(code),
                                        ..StylePatch::default()
                                    },
                                );
                            }
                            _ => {}
                        }
                        writes.push(next);
                    }
                }
            }
        }
        if what != PasteWhat::Comments {
            self.write_records(dst, &writes)?;
        }
        let mut meta_changed = false;
        if paste_formats {
            let merges: Vec<Range> = self
                .sheet_ok(src)?
                .meta
                .merges
                .iter()
                .filter(|m| m.is_inside(&r))
                .copied()
                .collect();
            let s = self.sheet_mut(dst)?;
            if !merges.is_empty() || s.meta.merges.iter().any(|m| m.intersects(&dest)) {
                s.meta.merges.retain(|m| !m.intersects(&dest));
                for ti in 0..tiles_v {
                    for tj in 0..tiles_h {
                        for m in &merges {
                            let (oi, oj, h, w) =
                                (m.r0 - r.r0, m.c0 - r.c0, m.height() - 1, m.width() - 1);
                            let (di, dj, dh, dw) = if transpose {
                                (oj, oi, w, h)
                            } else {
                                (oi, oj, h, w)
                            };
                            let r0 = dest.r0 + ti * bh + di;
                            let c0 = dest.c0 + tj * bw + dj;
                            s.meta.merges.push(Range::new(r0, c0, r0 + dh, c0 + dw));
                        }
                    }
                }
                meta_changed = true;
            }
        }
        if matches!(what, PasteWhat::All | PasteWhat::Comments) {
            let notes: Vec<crate::meta::Note> = self
                .sheet_ok(src)?
                .meta
                .notes
                .iter()
                .filter(|n| r.contains(n.at.row, n.at.col))
                .cloned()
                .collect();
            if !notes.is_empty() {
                let s = self.sheet_mut(dst)?;
                s.meta.notes.retain(|n| !dest.contains(n.at.row, n.at.col));
                for ti in 0..tiles_v {
                    for tj in 0..tiles_h {
                        for n in &notes {
                            let (oi, oj) = (n.at.row - r.r0, n.at.col - r.c0);
                            let (di, dj) = if transpose { (oj, oi) } else { (oi, oj) };
                            s.meta.notes.push(crate::meta::Note {
                                at: CellPos::new(dest.r0 + ti * bh + di, dest.c0 + tj * bw + dj),
                                ..n.clone()
                            });
                        }
                    }
                }
                meta_changed = true;
            }
        }
        if meta_changed {
            self.note_meta(dst);
        }
        Ok(Command::Restore {
            cells: vec![SheetCells {
                sheet: dst,
                records: before,
            }],
            meta: if meta_changed {
                vec![meta_before]
            } else {
                Vec::new()
            },
            layout: Vec::new(),
            names: None,
        })
    }

    // ── sort ───────────────────────────────────────────────────────────────

    #[allow(clippy::too_many_arguments)]
    fn sort_range(
        &mut self,
        sheet: SheetId,
        r: Range,
        keys: &[SortKey],
        header: bool,
        case_sensitive: bool,
        by_columns: bool,
        locale: Option<&str>,
    ) -> Result<Command> {
        r.check()?;
        if keys.is_empty() {
            return Err(ModelError::invalid("sort needs at least one key"));
        }
        self.check_permission(sheet, |p| p.sort, "sorting")?;
        self.check_editable(sheet, &[r])?;
        let first = if by_columns { r.c0 } else { r.r0 } + u32::from(header);
        let last = if by_columns { r.c1 } else { r.r1 };
        if last <= first {
            return Ok(Command::noop());
        }
        let (lo, hi) = if by_columns {
            (r.r0, r.r1)
        } else {
            (r.c0, r.c1)
        };
        if keys.iter().any(|k| k.index < lo || k.index > hi) {
            return Err(ModelError::invalid("a sort key is outside the range"));
        }
        let body = if by_columns {
            Range { c0: first, ..r }
        } else {
            Range { r0: first, ..r }
        };
        if self
            .sheet_ok(sheet)?
            .meta
            .merges
            .iter()
            .any(|m| m.intersects(&body))
        {
            return Err(ModelError::invalid(
                "to sort, all merged cells need to be the same size",
            ));
        }
        let order = crate::sort::sort_order(
            self,
            sheet,
            first..=last,
            keys,
            case_sensitive,
            by_columns,
            locale,
        );
        if order
            .iter()
            .enumerate()
            .all(|(i, &l)| l == first + i as u32)
        {
            return Ok(Command::noop());
        }
        let before = self.records_in(sheet, body);
        let mut by_line: FxHashMap<u32, Vec<CellRecord>> = FxHashMap::default();
        for rec in &before {
            by_line
                .entry(if by_columns { rec.col } else { rec.row })
                .or_default()
                .push(rec.clone());
        }
        let mut writes: FxHashMap<(u32, u32), CellRecord> = before
            .iter()
            .map(|rec| ((rec.row, rec.col), CellRecord::empty(rec.row, rec.col)))
            .collect();
        let mut touched: Vec<CellRecord> = before.clone();
        for (i, &src_line) in order.iter().enumerate() {
            let dst_line = first + i as u32;
            for rec in by_line.get(&src_line).into_iter().flatten() {
                let (row, col) = if by_columns {
                    (rec.row, dst_line)
                } else {
                    (dst_line, rec.col)
                };
                let formula = rec.formula.as_ref().map(|f| {
                    copy_formula(
                        f,
                        i64::from(row) - i64::from(rec.row),
                        i64::from(col) - i64::from(rec.col),
                    )
                    .map_or_else(|_| f.clone(), |t| strip_equals(&t).to_owned())
                });
                if !writes.contains_key(&(row, col)) {
                    touched.push(CellRecord::empty(row, col));
                }
                writes.insert(
                    (row, col),
                    CellRecord {
                        row,
                        col,
                        formula,
                        ..rec.clone()
                    },
                );
            }
        }
        let mut list: Vec<CellRecord> = writes.into_values().collect();
        list.sort_by_key(|c| (c.row, c.col));
        // Empty records first so nothing overwritten is cleared afterwards.
        list.sort_by_key(|c| c.has_content() || c.style != 0);
        self.write_records(sheet, &list)?;
        // Notes move with their cells.
        let meta = self.meta_entry(sheet)?;
        let pos: FxHashMap<u32, u32> = order
            .iter()
            .enumerate()
            .map(|(i, &l)| (l, first + i as u32))
            .collect();
        let mut meta_changed = false;
        for n in &mut self.sheet_mut(sheet)?.meta.notes {
            if !body.contains(n.at.row, n.at.col) {
                continue;
            }
            let line = if by_columns { n.at.col } else { n.at.row };
            let to = pos.get(&line).copied().unwrap_or(line);
            if to != line {
                meta_changed = true;
                if by_columns {
                    n.at.col = to;
                } else {
                    n.at.row = to;
                }
            }
        }
        if meta_changed {
            self.note_meta(sheet);
        }
        Ok(Command::Restore {
            cells: vec![SheetCells {
                sheet,
                records: dedupe(touched),
            }],
            meta: if meta_changed { vec![meta] } else { Vec::new() },
            layout: Vec::new(),
            names: None,
        })
    }

    // ── sheets ─────────────────────────────────────────────────────────────

    fn add_sheet_cmd(
        &mut self,
        name: Option<&str>,
        index: Option<usize>,
        id: Option<SheetId>,
    ) -> Result<Command> {
        self.check_structure_unlocked()?;
        let sid = self.add_sheet(name, index, id)?;
        let sname = self.sheet_ok(sid)?.name.clone();
        // Formulas that referred to this name while it did not exist now bind; removing the
        // sheet again would turn them into #REF!, so the inverse restores their text.
        let mut pending =
            self.changed_formula_records(&|t, _, _| remove_sheet_in_formula(t, &sname).ok());
        pending.remove(&sid);
        Ok(batch(vec![
            Command::RemoveSheet { sheet: sid },
            Command::Restore {
                cells: restore_cells(pending),
                meta: self
                    .all_meta()
                    .into_iter()
                    .filter(|m| m.sheet != sid)
                    .collect(),
                layout: Vec::new(),
                names: Some(self.names.clone()),
            },
        ]))
    }

    fn remove_sheet_cmd(&mut self, sheet: SheetId) -> Result<Command> {
        self.check_structure_unlocked()?;
        let s = self.sheet_ok(sheet)?;
        if self.sheets().len() == 1 {
            return Err(ModelError::invalid(
                "a workbook must contain at least one sheet",
            ));
        }
        if s.meta.visibility == SheetVisibility::Visible && self.visible_sheet_count() == 1 {
            return Err(ModelError::invalid(
                "a workbook must contain at least one visible sheet",
            ));
        }
        let name = s.name.clone();
        let had_tables = !s.meta.tables.is_empty();
        let index = self.sheet_index(sheet).unwrap_or(0);
        let snapshot = self.snapshot_sheet(sheet)?;
        let meta_before: Vec<SheetMetaEntry> = self
            .all_meta()
            .into_iter()
            .filter(|m| m.sheet != sheet)
            .collect();
        let names_before = self.names.clone();
        let props_before = self.props.clone();
        let mut changes =
            self.changed_formula_records(&|t, _, _| remove_sheet_in_formula(t, &name).ok());
        changes.remove(&sheet);
        self.engine
            .remove_sheet(sheet)
            .map_err(|e| ModelError::invalid(e.to_string()))?;
        {
            let host = self.host_mut();
            if let Some(i) = host.position(sheet) {
                host.list.remove(i);
            }
            host.reindex();
        }
        self.names.retain(|n| n.scope != Some(sheet));
        self.rewrite_model_formulas(&|t, _, _| {
            remove_sheet_in_formula(t, &name).ok().filter(|x| x != t)
        });
        for s in &mut self.host_mut().list {
            for h in &mut s.meta.hyperlinks {
                if let Some(l) = &h.location
                    && let Ok(n) = remove_sheet_in_formula(l, &name)
                {
                    h.location = Some(n);
                }
            }
        }
        if self.props.active_sheet >= self.sheets().len() {
            self.props.active_sheet = self.sheets().len() - 1;
        }
        if had_tables {
            self.tables_changed();
        }
        self.sync_names();
        self.changes.sheets = true;
        self.calc.pending = true;
        Ok(batch(vec![
            Command::RestoreSheet {
                snapshot: Box::new(snapshot),
                index,
            },
            Command::Restore {
                cells: restore_cells(changes),
                meta: meta_before,
                layout: Vec::new(),
                names: Some(names_before),
            },
            Command::SetProps {
                props: Box::new(props_before),
            },
        ]))
    }

    fn rename_sheet_cmd(&mut self, sheet: SheetId, name: &str) -> Result<Command> {
        self.check_structure_unlocked()?;
        let old = self.sheet_ok(sheet)?.name.clone();
        if old == name {
            return Ok(Command::noop());
        }
        check_sheet_name(name)?;
        if let Some(o) = self.sheet_by_name(name)
            && o.id() != sheet
        {
            return Err(ModelError::invalid(format!(
                "a sheet named \"{name}\" already exists"
            )));
        }
        let meta_before = self.all_meta();
        let names_before = self.names.clone();
        // The inverse replays `RenameSheet(new → old)` before restoring, and that reverse
        // rename also rewrites formulas which already named `new` (e.g. a dangling
        // `#REF!`-valued reference to a deleted sheet of that name). Snapshot every
        // formula either direction touches so the restore puts all of them back exactly.
        let changes = self.changed_formula_records(&|t, _, _| {
            rename_sheet_in_formula(t, &old, name)
                .ok()
                .filter(|x| x != t)
                .or_else(|| rename_sheet_in_formula(t, name, &old).ok())
        });
        self.engine
            .rename_sheet(sheet, name)
            .map_err(|e| ModelError::invalid(e.to_string()))?;
        self.sheet_mut(sheet)?.name = name.to_owned();
        let new = name.to_owned();
        self.rewrite_model_formulas(&|t, _, _| {
            rename_sheet_in_formula(t, &old, &new)
                .ok()
                .filter(|x| x != t)
        });
        for s in &mut self.host_mut().list {
            for h in &mut s.meta.hyperlinks {
                if let Some(l) = &h.location
                    && let Ok(n) = rename_sheet_in_formula(l, &old, &new)
                {
                    h.location = Some(n);
                }
            }
        }
        self.host_mut().reindex_tables();
        self.sync_names();
        self.changes.sheets = true;
        self.calc.pending = true;
        Ok(batch(vec![
            Command::RenameSheet {
                sheet,
                name: old.clone(),
            },
            Command::Restore {
                cells: restore_cells(changes),
                meta: meta_before,
                layout: Vec::new(),
                names: Some(names_before),
            },
        ]))
    }

    fn move_sheet_cmd(&mut self, sheet: SheetId, index: usize) -> Result<Command> {
        self.check_structure_unlocked()?;
        let from = self
            .sheet_index(sheet)
            .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", sheet.0)))?;
        let to = index.min(self.sheets().len() - 1);
        if from == to {
            return Ok(Command::noop());
        }
        let host = self.host_mut();
        let s = host.list.remove(from);
        host.list.insert(to, s);
        host.reindex();
        let _ = self.engine.move_sheet(sheet, to);
        self.changes.sheets = true;
        self.calc.pending = true;
        Ok(Command::MoveSheet { sheet, index: from })
    }

    // ── rows and columns ───────────────────────────────────────────────────

    fn check_span(at: u32, count: u32, max: u32) -> Result<()> {
        if count == 0 || u64::from(at) + u64::from(count) > u64::from(max) {
            return Err(ModelError::invalid("row/column span out of range"));
        }
        Ok(())
    }

    fn set_row_props(
        &mut self,
        sheet: SheetId,
        at: u32,
        count: u32,
        p: &RowPatch,
    ) -> Result<Command> {
        Self::check_span(at, count, MAX_ROWS)?;
        if let Patch::Set(h) = p.height
            && !(0.0..=409.0).contains(&h.0)
        {
            return Err(ModelError::invalid(
                "row height must be between 0 and 409 points",
            ));
        }
        if p.outline_level.is_some_and(|l| l > 7) {
            return Err(ModelError::invalid("outline level must be between 0 and 7"));
        }
        if let Patch::Set(st) = p.style {
            self.styles.check(st)?;
        }
        self.check_permission(sheet, |x| x.format_rows, "formatting rows")?;
        let s = self.sheet_ok(sheet)?;
        let mut entries = Vec::with_capacity(count as usize);
        for i in at..at + count {
            let prev = s.rows.get(i).copied();
            let mut n = prev.unwrap_or_default();
            n.height = p.height.apply_opt(n.height);
            n.style = p.style.apply_opt(n.style);
            if let Some(v) = p.custom_height {
                n.custom_height = v;
            }
            if let Some(v) = p.hidden {
                n.hidden = v;
                if !v {
                    n.filtered = false;
                }
            }
            if let Some(v) = p.filtered {
                n.filtered = v;
            }
            if let Some(v) = p.outline_level {
                n.outline_level = v;
            }
            if let Some(v) = p.collapsed {
                n.collapsed = v;
            }
            entries.push((i, Some(n)));
        }
        self.set_row_entries(sheet, &entries)
    }

    fn set_col_props(
        &mut self,
        sheet: SheetId,
        at: u32,
        count: u32,
        p: &ColPatch,
    ) -> Result<Command> {
        Self::check_span(at, count, MAX_COLS)?;
        if let Patch::Set(w) = p.width
            && !(0.0..=255.0).contains(&w.0)
        {
            return Err(ModelError::invalid(
                "column width must be between 0 and 255 characters",
            ));
        }
        if p.outline_level.is_some_and(|l| l > 7) {
            return Err(ModelError::invalid("outline level must be between 0 and 7"));
        }
        if let Patch::Set(st) = p.style {
            self.styles.check(st)?;
        }
        self.check_permission(sheet, |x| x.format_columns, "formatting columns")?;
        let s = self.sheet_ok(sheet)?;
        let mut entries = Vec::with_capacity(count as usize);
        for i in at..at + count {
            let mut n = s.cols.get(i).copied().unwrap_or_default();
            n.width = p.width.apply_opt(n.width);
            n.style = p.style.apply_opt(n.style);
            if let Some(v) = p.custom_width {
                n.custom_width = v;
            }
            if let Some(v) = p.best_fit {
                n.best_fit = v;
            }
            if let Some(v) = p.hidden {
                n.hidden = v;
            }
            if let Some(v) = p.outline_level {
                n.outline_level = v;
            }
            if let Some(v) = p.collapsed {
                n.collapsed = v;
            }
            entries.push((i, Some(n)));
        }
        self.set_col_entries(sheet, &entries)
    }

    fn set_row_entries(
        &mut self,
        sheet: SheetId,
        entries: &[(u32, Option<RowProps>)],
    ) -> Result<Command> {
        let s = self.sheet_mut(sheet)?;
        let before_vis = hidden_rows(s);
        let mut inv = Vec::with_capacity(entries.len());
        for (i, p) in entries {
            if *i >= MAX_ROWS {
                continue;
            }
            inv.push((*i, s.rows.get(*i).copied()));
            s.rows.set(*i, *p);
        }
        let after_vis = hidden_rows(s);
        self.visibility_changed(sheet, before_vis, after_vis);
        self.note_layout(sheet);
        Ok(Command::SetRowEntries {
            sheet,
            entries: inv,
        })
    }

    fn set_col_entries(
        &mut self,
        sheet: SheetId,
        entries: &[(u32, Option<ColProps>)],
    ) -> Result<Command> {
        let s = self.sheet_mut(sheet)?;
        let mut inv = Vec::with_capacity(entries.len());
        for (i, p) in entries {
            if *i >= MAX_COLS {
                continue;
            }
            inv.push((*i, s.cols.get(*i).copied()));
            s.cols.set(*i, *p);
        }
        self.note_layout(sheet);
        Ok(Command::SetColEntries {
            sheet,
            entries: inv,
        })
    }

    // ── merges ─────────────────────────────────────────────────────────────

    fn merge_cmd(&mut self, sheet: SheetId, r: Range, mode: MergeMode) -> Result<Command> {
        r.check()?;
        self.check_format_allowed(sheet)?;
        let s = self.sheet_ok(sheet)?;
        if s.meta
            .merges
            .iter()
            .any(|m| m.intersects(&r) && !m.is_inside(&r))
        {
            return Err(ModelError::invalid(
                "cannot merge: the range partly overlaps merged cells",
            ));
        }
        if let Some(t) = s.meta.tables.iter().find(|t| t.range.intersects(&r)) {
            return Err(ModelError::invalid(format!(
                "cells in table \"{}\" cannot be merged",
                t.name
            )));
        }
        let targets: Vec<Range> = if mode == MergeMode::Across {
            (r.r0..=r.r1)
                .map(|row| Range {
                    r0: row,
                    r1: row,
                    ..r
                })
                .collect()
        } else {
            vec![r]
        };
        let meta = self.meta_entry(sheet)?;
        self.sheet_mut(sheet)?
            .meta
            .merges
            .retain(|m| !m.is_inside(&r));
        let mut before: Vec<CellRecord> = Vec::new();
        let mut seen = FxHashSet::default();
        for t in &targets {
            if t.is_single_cell() {
                continue;
            }
            let cells = self.records_in(sheet, *t);
            // Keep only the upper-left-most content (Excel's rule), moved into the top-left cell.
            let keep = cells.iter().find(|c| c.has_content()).cloned();
            let tl = self.record(sheet, t.r0, t.c0);
            if seen.insert((t.r0, t.c0)) {
                before.push(tl.clone());
            }
            for c in &cells {
                if seen.insert((c.row, c.col)) {
                    before.push(c.clone());
                }
                if c.row == t.r0 && c.col == t.c0 {
                    continue;
                }
                self.write_record(
                    sheet,
                    &CellRecord {
                        style: c.style,
                        ..CellRecord::empty(c.row, c.col)
                    },
                )?;
            }
            if let Some(k) = keep
                && (k.row != t.r0 || k.col != t.c0)
            {
                let formula = k.formula.as_ref().map(|f| {
                    copy_formula(
                        f,
                        i64::from(t.r0) - i64::from(k.row),
                        i64::from(t.c0) - i64::from(k.col),
                    )
                    .map_or_else(|_| f.clone(), |x| strip_equals(&x).to_owned())
                });
                let style = self.sheet_ok(sheet)?.style_at(t.r0, t.c0);
                self.write_record(
                    sheet,
                    &CellRecord {
                        row: t.r0,
                        col: t.c0,
                        value: k.value,
                        formula,
                        legacy: k.legacy,
                        style,
                    },
                )?;
            }
            self.sheet_mut(sheet)?.meta.merges.push(*t);
        }
        if mode == MergeMode::Center {
            let mut cache: FxHashMap<u32, u32> = FxHashMap::default();
            let patch = StylePatch {
                alignment: Some(AlignmentPatch {
                    horizontal: Some(HAlign::Center),
                    ..AlignmentPatch::default()
                }),
                ..StylePatch::default()
            };
            for row in r.r0..=r.r1 {
                for col in r.c0..=r.c1 {
                    let rec = self.record(sheet, row, col);
                    if seen.insert((row, col)) {
                        before.push(rec.clone());
                    }
                    let base = self.sheet_ok(sheet)?.style_at(row, col);
                    let st = *cache
                        .entry(base)
                        .or_insert_with(|| self.styles.patch(base, &patch));
                    self.sheet_mut(sheet)?.cells.set_style(row, col, st);
                }
            }
        }
        self.note_cells(sheet, r);
        self.note_meta(sheet);
        Ok(Command::Restore {
            cells: vec![SheetCells {
                sheet,
                records: before,
            }],
            meta: vec![meta],
            layout: Vec::new(),
            names: None,
        })
    }

    fn unmerge_cmd(&mut self, sheet: SheetId, r: Range) -> Result<Command> {
        r.check()?;
        let meta = self.meta_entry(sheet)?;
        let s = self.sheet_mut(sheet)?;
        let n0 = s.meta.merges.len();
        s.meta.merges.retain(|m| !m.intersects(&r));
        if s.meta.merges.len() == n0 {
            return Ok(Command::noop());
        }
        self.note_meta(sheet);
        Ok(Command::Restore {
            cells: Vec::new(),
            meta: vec![meta],
            layout: Vec::new(),
            names: None,
        })
    }

    // ── names ──────────────────────────────────────────────────────────────

    fn define_name_cmd(
        &mut self,
        name: &str,
        formula: &str,
        scope: Option<SheetId>,
        hidden: bool,
        comment: Option<String>,
    ) -> Result<Command> {
        self.check_defined_name(name, scope)?;
        let formula = Workbook::check_formula(formula)?;
        let before = self.names.clone();
        let def = DefinedName {
            name: name.to_owned(),
            formula,
            scope,
            hidden,
            comment,
        };
        match self
            .names
            .iter_mut()
            .find(|n| n.scope == scope && n.name.eq_ignore_ascii_case(name))
        {
            Some(n) => *n = def,
            None => self.names.push(def),
        }
        self.sync_names();
        Ok(Command::Restore {
            cells: Vec::new(),
            meta: Vec::new(),
            layout: Vec::new(),
            names: Some(before),
        })
    }

    fn delete_name_cmd(&mut self, name: &str, scope: Option<SheetId>) -> Result<Command> {
        let before = self.names.clone();
        self.names
            .retain(|n| !(n.scope == scope && n.name.eq_ignore_ascii_case(name)));
        if self.names.len() == before.len() {
            return Err(ModelError::invalid(format!(
                "no name \"{name}\" in that scope"
            )));
        }
        self.sync_names();
        Ok(Command::Restore {
            cells: Vec::new(),
            meta: Vec::new(),
            layout: Vec::new(),
            names: Some(before),
        })
    }

    // ── tables ─────────────────────────────────────────────────────────────

    /// Validates a table definition for a sheet (ignoring the table named `replacing`).
    pub fn check_table(&self, sheet: SheetId, t: &Table, replacing: Option<&str>) -> Result<()> {
        let s = self.sheet_ok(sheet)?;
        t.range.check()?;
        if !kiln_formula::engine::is_valid_name(&t.name) {
            return Err(ModelError::invalid(format!(
                "\"{}\" is not a valid table name",
                t.name
            )));
        }
        let is_replaced = |n: &str| replacing.is_some_and(|r| r.eq_ignore_ascii_case(n));
        if let Some((_, o)) = self.find_table(&t.name)
            && !is_replaced(&o.name)
        {
            return Err(ModelError::invalid(format!(
                "a table named \"{}\" already exists",
                t.name
            )));
        }
        if self
            .names
            .iter()
            .any(|n| n.name.eq_ignore_ascii_case(&t.name))
        {
            return Err(ModelError::invalid(format!(
                "\"{}\" is already used by a defined name",
                t.name
            )));
        }
        let width = t.range.width() as usize;
        if t.columns.len() != width {
            return Err(ModelError::invalid(format!(
                "table \"{}\" has {} column names for {width} columns",
                t.name,
                t.columns.len()
            )));
        }
        let mut seen = FxHashSet::default();
        for c in &t.columns {
            if c.name.is_empty() {
                return Err(ModelError::invalid("table column names cannot be empty"));
            }
            if !seen.insert(c.name.to_uppercase()) {
                return Err(ModelError::invalid(format!(
                    "duplicate table column name \"{}\"",
                    c.name
                )));
            }
        }
        let min_rows = 1 + u32::from(t.header_row) + u32::from(t.totals_row);
        if t.range.height() < min_rows {
            return Err(ModelError::invalid("a table needs at least one data row"));
        }
        for o in &s.meta.tables {
            if is_replaced(&o.name) {
                continue;
            }
            if o.range.intersects(&t.range) {
                return Err(ModelError::invalid(format!(
                    "table \"{}\" would overlap table \"{}\"",
                    t.name, o.name
                )));
            }
        }
        if s.meta.merges.iter().any(|m| m.intersects(&t.range)) {
            return Err(ModelError::invalid("a table cannot contain merged cells"));
        }
        Ok(())
    }

    fn write_headers(
        &mut self,
        sheet: SheetId,
        t: &Table,
        before: &mut Vec<CellRecord>,
    ) -> Result<()> {
        if !t.header_row {
            return Ok(());
        }
        for (i, c) in t.columns.iter().enumerate() {
            let (row, col) = (t.range.r0, t.range.c0 + i as u32);
            let prev = self.record(sheet, row, col);
            if prev.formula.is_none() && prev.value.as_text() == Some(c.name.as_str()) {
                continue;
            }
            let style = if self.sheet_ok(sheet)?.cells.contains(row, col) {
                prev.style
            } else {
                self.sheet_ok(sheet)?.style_at(row, col)
            };
            before.push(prev);
            self.write_record(
                sheet,
                &CellRecord {
                    row,
                    col,
                    value: CellValue::text(c.name.as_str()),
                    formula: None,
                    legacy: false,
                    style,
                },
            )?;
        }
        Ok(())
    }

    fn add_table_cmd(&mut self, sheet: SheetId, table: &Table) -> Result<Command> {
        let mut t = table.clone();
        if t.id == 0 {
            t.id = self.alloc_table_id();
        }
        if self
            .sheets()
            .iter()
            .any(|s| s.meta.tables.iter().any(|x| x.id == t.id))
        {
            return Err(ModelError::invalid(format!(
                "table id {} is already used",
                t.id
            )));
        }
        self.check_table(sheet, &t, None)?;
        self.check_editable(sheet, &[t.range])?;
        self.next_table_id = self.next_table_id.max(t.id + 1);
        let meta = self.meta_entry(sheet)?;
        let mut before = Vec::new();
        self.write_headers(sheet, &t, &mut before)?;
        self.sheet_mut(sheet)?.meta.tables.push(t);
        self.tables_changed();
        self.note_meta(sheet);
        Ok(Command::Restore {
            cells: vec![SheetCells {
                sheet,
                records: before,
            }],
            meta: vec![meta],
            layout: Vec::new(),
            names: None,
        })
    }

    fn remove_table_cmd(&mut self, name: &str) -> Result<Command> {
        let (sheet, table) = self
            .find_table(name)
            .map(|(s, t)| (s, t.clone()))
            .ok_or_else(|| ModelError::invalid(format!("no table named \"{name}\"")))?;
        let meta_before = self.all_meta();
        let names_before = self.names.clone();
        let tsheet = self.sheet_ok(sheet)?.name.clone();
        let changes = self.rewrite_cell_formulas(&|text, fs, at| {
            structured_to_a1(text, &table, &tsheet, fs, Some(at))
        });
        self.rewrite_model_formulas(&|text, fs, at| {
            structured_to_a1(text, &table, &tsheet, fs, at)
        });
        self.sheet_mut(sheet)?
            .meta
            .tables
            .retain(|t| !t.name.eq_ignore_ascii_case(name));
        self.tables_changed();
        self.sync_names();
        self.note_meta(sheet);
        Ok(Command::Restore {
            cells: restore_cells(changes),
            meta: meta_before,
            layout: Vec::new(),
            names: Some(names_before),
        })
    }

    fn update_table_cmd(&mut self, name: &str, patch: &TablePatch) -> Result<Command> {
        let (sheet, table) = self
            .find_table(name)
            .map(|(s, t)| (s, t.clone()))
            .ok_or_else(|| ModelError::invalid(format!("no table named \"{name}\"")))?;
        let mut next = table.clone();
        if let Some(n) = &patch.name {
            next.name = n.clone();
        }
        next.display_name = patch.display_name.apply_opt(next.display_name);
        next.style = patch.style.apply_opt(next.style);
        next.auto_filter = patch.auto_filter.apply_opt(next.auto_filter);
        next.comment = patch.comment.apply_opt(next.comment);
        if let Some(h) = patch.header_row {
            next.header_row = h;
        }
        if let Some(t) = patch.totals_row {
            next.totals_row = t;
        }
        if let Some(cols) = &patch.columns {
            next.columns = cols.clone();
        }
        if let Some(r) = patch.range {
            r.check()?;
            if r.r0 != table.range.r0 || r.c0 != table.range.c0 {
                return Err(ModelError::invalid(
                    "a table can only be resized from its bottom-right corner",
                ));
            }
            next.range = r;
            if patch.columns.is_none() {
                let width = r.width() as usize;
                next.columns.truncate(width);
                let names: Vec<String> = next.columns.iter().map(|c| c.name.clone()).collect();
                let extra = width.saturating_sub(next.columns.len());
                next.columns.extend(
                    fresh_column_names(&names, extra)
                        .into_iter()
                        .map(TableColumn::named),
                );
            }
        }
        self.check_table(sheet, &next, Some(&table.name))?;
        let meta_before = self.all_meta();
        let names_before = self.names.clone();
        let mut changes = Records::default();
        if !next.name.eq_ignore_ascii_case(&table.name) || next.name != table.name {
            let (old, new) = (table.name.clone(), next.name.clone());
            for (sid, recs) in
                self.rewrite_cell_formulas(&|t, _, _| rename_table_in_formula(t, &old, &new))
            {
                add_records(&mut changes, sid, recs);
            }
            self.rewrite_model_formulas(&|t, _, _| rename_table_in_formula(t, &old, &new));
        }
        let tsheet = self.sheet_ok(sheet)?.name.clone();
        let pos = self
            .sheet_ok(sheet)?
            .meta
            .tables
            .iter()
            .position(|t| t.name.eq_ignore_ascii_case(&table.name))
            .ok_or_else(|| ModelError::invalid(format!("no table named \"{name}\"")))?;
        self.sheet_mut(sheet)?.meta.tables[pos] = next.clone();
        for (i, c) in next.columns.iter().enumerate() {
            let Some(old) = table.columns.get(i) else {
                continue;
            };
            if old.name == c.name {
                continue;
            }
            let (tname, range, oldn, newn) = (
                next.name.clone(),
                next.range,
                old.name.clone(),
                c.name.clone(),
            );
            let f = |t: &str, fs: &str, at: Option<CellPos>| {
                let inside = fs.eq_ignore_ascii_case(&tsheet)
                    && at.is_some_and(|a| range.contains(a.row, a.col));
                rename_table_column_in_formula(t, &tname, &oldn, &newn, inside)
            };
            for (sid, recs) in self.rewrite_cell_formulas(&|t, fs, at| f(t, fs, Some(at))) {
                add_records(&mut changes, sid, recs);
            }
            self.rewrite_model_formulas(&|t, fs, at| f(t, fs, at));
        }
        let mut before = Vec::new();
        self.write_headers(sheet, &next, &mut before)?;
        add_records(&mut changes, sheet, before);
        self.tables_changed();
        self.sync_names();
        self.note_meta(sheet);
        Ok(Command::Restore {
            cells: restore_cells(changes),
            meta: meta_before,
            layout: Vec::new(),
            names: Some(names_before),
        })
    }

    // ── metadata and properties ────────────────────────────────────────────

    fn set_sheet_meta(&mut self, sheet: SheetId, field: MetaField) -> Result<Command> {
        let s = self.sheet_ok(sheet)?;
        let valid = |r: &Range| r.check().is_ok();
        let bad = |what: &str| Err(ModelError::invalid(format!("invalid {what}")));
        match &field {
            MetaField::Cf(blocks) => {
                for b in blocks {
                    if b.ranges.is_empty() || !b.ranges.iter().all(valid) {
                        return bad("conditional format ranges");
                    }
                    for rule in &b.rules {
                        if let Some(d) = rule.kind.dxf()
                            && self.styles.dxf(d).is_none()
                        {
                            return bad("conditional format: unknown differential format");
                        }
                    }
                }
            }
            MetaField::Validations(list) => {
                if list
                    .iter()
                    .any(|v| v.ranges.is_empty() || !v.ranges.iter().all(valid))
                {
                    return bad("validation ranges");
                }
            }
            MetaField::Hyperlinks(list) => {
                if list
                    .iter()
                    .any(|h| !valid(&h.range) || (h.target.is_none() && h.location.is_none()))
                {
                    return bad("hyperlink");
                }
            }
            MetaField::Notes(list) => {
                if list.iter().any(|n| check_cell(n.at.row, n.at.col).is_err()) {
                    return bad("note position");
                }
            }
            MetaField::Threads(list) => {
                if list.iter().any(|n| check_cell(n.at.row, n.at.col).is_err()) {
                    return bad("comment position");
                }
            }
            MetaField::View(v) => {
                if !(10..=400).contains(&v.zoom) {
                    return Err(ModelError::invalid("zoom must be between 10% and 400%"));
                }
                if !v.selection.ranges.iter().all(valid)
                    || check_cell(v.selection.active.row, v.selection.active.col).is_err()
                {
                    return bad("selection");
                }
                if v.freeze
                    .is_some_and(|f| f.rows >= MAX_ROWS || f.cols >= MAX_COLS)
                {
                    return bad("freeze panes");
                }
            }
            MetaField::Print(p) => {
                if !(10..=400).contains(&p.scale) {
                    return bad("print scale (10–400%)");
                }
            }
            MetaField::AutoFilter(Some(f)) => {
                if !valid(&f.range) {
                    return bad("AutoFilter range");
                }
            }
            MetaField::Visibility(v) => {
                if *v != SheetVisibility::Visible {
                    self.check_structure_unlocked()?;
                    let others = self
                        .sheets()
                        .iter()
                        .filter(|o| {
                            o.id() != sheet && o.meta.visibility == SheetVisibility::Visible
                        })
                        .count();
                    if others == 0 {
                        return Err(ModelError::invalid(
                            "a workbook must contain at least one visible sheet",
                        ));
                    }
                }
            }
            MetaField::DefaultRowHeight(h) => {
                if !(*h > 0.0 && *h <= 409.0) {
                    return bad("default row height");
                }
            }
            MetaField::DefaultColWidth(Some(w)) if !(0.0..=255.0).contains(w) => {
                return bad("default column width");
            }
            _ => {}
        }
        let mut meta = s.meta.clone();
        let prev = match field {
            MetaField::Cf(v) => MetaField::Cf(std::mem::replace(&mut meta.cf, v)),
            MetaField::Validations(v) => {
                MetaField::Validations(std::mem::replace(&mut meta.validations, v))
            }
            MetaField::Hyperlinks(v) => {
                MetaField::Hyperlinks(std::mem::replace(&mut meta.hyperlinks, v))
            }
            MetaField::Notes(v) => MetaField::Notes(std::mem::replace(&mut meta.notes, v)),
            MetaField::Threads(v) => MetaField::Threads(std::mem::replace(&mut meta.threads, v)),
            MetaField::View(v) => MetaField::View(std::mem::replace(&mut meta.view, v)),
            MetaField::Print(v) => MetaField::Print(std::mem::replace(&mut meta.print, v)),
            MetaField::AutoFilter(v) => {
                MetaField::AutoFilter(std::mem::replace(&mut meta.auto_filter, v))
            }
            MetaField::Drawings(v) => MetaField::Drawings(std::mem::replace(&mut meta.drawings, v)),
            MetaField::Visibility(v) => {
                MetaField::Visibility(std::mem::replace(&mut meta.visibility, v))
            }
            MetaField::TabColor(v) => {
                MetaField::TabColor(std::mem::replace(&mut meta.tab_color, v))
            }
            MetaField::Outline {
                summary_below,
                summary_right,
            } => {
                let p = MetaField::Outline {
                    summary_below: meta.summary_below,
                    summary_right: meta.summary_right,
                };
                meta.summary_below = summary_below;
                meta.summary_right = summary_right;
                p
            }
            MetaField::Protection(v) => {
                MetaField::Protection(std::mem::replace(&mut meta.protection, v))
            }
            MetaField::DefaultRowHeight(v) => {
                MetaField::DefaultRowHeight(std::mem::replace(&mut meta.default_row_height, v))
            }
            MetaField::DefaultColWidth(v) => {
                MetaField::DefaultColWidth(std::mem::replace(&mut meta.default_col_width, v))
            }
        };
        self.load_sheet_meta(sheet, meta)?;
        Ok(Command::SetSheetMeta {
            sheet,
            field: Box::new(prev),
        })
    }

    fn set_props(&mut self, props: &WorkbookProps) -> Result<Command> {
        if props.active_sheet >= self.sheets().len().max(1) {
            return Err(ModelError::invalid("active sheet index out of range"));
        }
        if props.calc.iterate_count == 0 || props.calc.iterate_count > 32_767 {
            return Err(ModelError::invalid(
                "maximum iterations must be between 1 and 32,767",
            ));
        }
        if !(props.calc.iterate_delta >= 0.0 && props.calc.iterate_delta.is_finite()) {
            return Err(ModelError::invalid(
                "maximum change must be a non-negative number",
            ));
        }
        let prev = std::mem::replace(&mut self.props, props.clone());
        let date = prev.date1904 != props.date1904;
        let calc = prev.calc != props.calc;
        if date || calc {
            self.props_changed(date);
        }
        self.changes.props = true;
        Ok(Command::SetProps {
            props: Box::new(prev),
        })
    }

    fn restore_cmd(
        &mut self,
        cells: &[SheetCells],
        meta: &[SheetMetaEntry],
        layout: &[SheetLayoutEntry],
        names: Option<&Vec<DefinedName>>,
    ) -> Result<Command> {
        // Validate everything first so a malformed restore changes nothing.
        for c in cells {
            for r in &c.records {
                check_cell(r.row, r.col)?;
                self.styles.check(r.style)?;
            }
        }
        let mut inv_meta = Vec::new();
        let mut tables = false;
        for m in meta {
            let Some(s) = self.sheet(m.sheet) else {
                continue;
            };
            inv_meta.push(SheetMetaEntry {
                sheet: m.sheet,
                meta: Box::new(s.meta.clone()),
            });
            tables |= s.meta.tables != m.meta.tables;
            let vis = s.meta.visibility != m.meta.visibility;
            if let Ok(sm) = self.sheet_mut(m.sheet) {
                sm.set_meta((*m.meta).clone());
            }
            if vis {
                self.changes.sheets = true;
            }
            self.note_meta(m.sheet);
        }
        if tables {
            self.tables_changed();
        }
        let mut inv_layout = Vec::new();
        for l in layout {
            let Some(s) = self.sheet(l.sheet) else {
                continue;
            };
            inv_layout.push(SheetLayoutEntry {
                sheet: l.sheet,
                layout: s.layout(),
            });
            self.load_sheet_layout(l.sheet, l.layout.clone())?;
        }
        let inv_names = names.map(|n| {
            let prev = std::mem::replace(&mut self.names, n.clone());
            self.sync_names();
            prev
        });
        let mut inv_cells = Vec::new();
        for c in cells {
            if self.sheet(c.sheet).is_none() {
                continue;
            }
            inv_cells.push(SheetCells {
                sheet: c.sheet,
                records: c
                    .records
                    .iter()
                    .map(|r| self.record(c.sheet, r.row, r.col))
                    .collect(),
            });
            self.write_records(c.sheet, &c.records)?;
        }
        Ok(Command::Restore {
            cells: inv_cells,
            meta: inv_meta,
            layout: inv_layout,
            names: inv_names,
        })
    }
}

type StyleMap<'m> = dyn FnMut(&mut Workbook, u32, &mut FxHashMap<u32, u32>) -> u32 + 'm;

/// Cells a style change touched, with their prior records.
struct Touched<'a> {
    sheet: SheetId,
    before: &'a mut FxHashMap<(u32, u32), CellRecord>,
    order: &'a mut Vec<(u32, u32)>,
}

impl Touched<'_> {
    fn touch(
        &mut self,
        wb: &mut Workbook,
        row: u32,
        col: u32,
        base: u32,
        cache: &mut FxHashMap<u32, u32>,
        map: &mut StyleMap<'_>,
    ) {
        if self.before.contains_key(&(row, col)) {
            return;
        }
        let prev = wb.record(self.sheet, row, col);
        self.before.insert((row, col), prev);
        self.order.push((row, col));
        let next = map(wb, base, cache);
        if let Ok(s) = wb.sheet_mut(self.sheet) {
            s.cells.set_style(row, col, next);
        }
    }
}

/// `before` plus empty records for cells that exist in `r` now but did not before (so undo
/// removes them).
fn with_empties(
    wb: &Workbook,
    sheet: SheetId,
    r: Range,
    before: Vec<CellRecord>,
) -> Vec<CellRecord> {
    let had: FxHashSet<(u32, u32)> = before.iter().map(|b| (b.row, b.col)).collect();
    let mut out = before;
    if let Some(s) = wb.sheet(sheet) {
        for (row, col, _) in s.cells.iter_range(r) {
            if !had.contains(&(row, col)) {
                out.push(CellRecord::empty(row, col));
            }
        }
    }
    out
}

fn paste_op(op: PasteOp, dest: &CellRecord, src: &CellValue) -> Option<CellValue> {
    let a = match &dest.value {
        CellValue::Blank if dest.formula.is_none() => 0.0,
        CellValue::Number(n) if dest.formula.is_none() => *n,
        _ => return None,
    };
    let b = match src {
        CellValue::Blank => return None,
        CellValue::Number(n) => *n,
        _ => return None,
    };
    Some(match op {
        PasteOp::None => return None,
        PasteOp::Add => CellValue::number(a + b),
        PasteOp::Subtract => CellValue::number(a - b),
        PasteOp::Multiply => CellValue::number(a * b),
        PasteOp::Divide => {
            if b == 0.0 {
                CellValue::Error(kiln_formula::values::ErrorValue::Div0)
            } else {
                CellValue::number(a / b)
            }
        }
    })
}
