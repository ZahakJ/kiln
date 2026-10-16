// SPDX-License-Identifier: AGPL-3.0-or-later
//! The workbook: ordered sheets, defined names, the style table, the theme, workbook properties
//! and a passthrough slot for the file layer — bound to the formula engine.
//!
//! The formula engine owns formulas, the dependency graph and computed results; the sheets (and
//! their cell stores) live inside the engine's host so the engine reads literal values straight
//! from the store with no copy. Every method here keeps the two in step. User edits go through
//! [`Command`](crate::Command)s ([`Workbook::apply`], [`CommandLog`](crate::CommandLog)), which
//! record exact inverses; the `load_*` methods are the low-level building blocks file readers use.

use std::collections::BTreeMap;

use kiln_formula::engine::{
    CellInput, CellRef, Clock, Engine, EngineHost, EngineOptions, HostReadOnly, IterativeOptions,
    RowVisibility, TableDef,
};
use kiln_formula::refs::StructuralEdit;
use kiln_formula::values::{ErrorValue, Value};
use kiln_formula::{SheetId, functions};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::address::{CellPos, Range, check_cell};
use crate::axis::{DefaultFontMetrics, FontMetrics};
use crate::color::Theme;
use crate::error::{ModelError, Result};
use crate::meta::{
    CalcMode, DefinedName, Person, SheetVisibility, WorkbookProps, WorkbookProtection,
    builtin_names,
};
use crate::sheet::Sheet;
use crate::store::{CellData, CellKind};
use crate::styles::StyleTable;
use crate::value::CellValue;

// ── the engine host ───────────────────────────────────────────────────────

/// The sheets, as the formula engine's host: literal values come straight from the stores.
#[derive(Debug, Default)]
pub struct Sheets {
    pub(crate) list: Vec<Sheet>,
    index: FxHashMap<SheetId, usize>,
    tables: FxHashMap<String, TableDef>,
}

impl Sheets {
    pub(crate) fn reindex(&mut self) {
        self.index = self
            .list
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id(), i))
            .collect();
        self.reindex_tables();
    }

    pub(crate) fn reindex_tables(&mut self) {
        self.tables = self
            .list
            .iter()
            .flat_map(|s| {
                s.tables()
                    .iter()
                    .map(move |t| (t.name.to_uppercase(), t.to_def(s.id())))
            })
            .collect();
    }

    pub(crate) fn get(&self, id: SheetId) -> Option<&Sheet> {
        self.index.get(&id).and_then(|&i| self.list.get(i))
    }

    pub(crate) fn get_mut(&mut self, id: SheetId) -> Option<&mut Sheet> {
        let i = *self.index.get(&id)?;
        self.list.get_mut(i)
    }

    pub(crate) fn position(&self, id: SheetId) -> Option<usize> {
        self.index.get(&id).copied()
    }
}

impl EngineHost for Sheets {
    fn cell_value(&self, sheet: SheetId, row: u32, col: u32) -> Option<Value> {
        let c = self.get(sheet)?.cells.get(row, col)?;
        if c.kind.is_engine_formula() || c.value.is_blank() {
            return None;
        }
        Some(c.value.to_value())
    }

    fn set_cell_value(
        &mut self,
        _sheet: SheetId,
        _row: u32,
        _col: u32,
        _value: Option<Value>,
    ) -> std::result::Result<(), HostReadOnly> {
        // The workbook writes its own cells (with styles and kinds); the engine only reads.
        Err(HostReadOnly)
    }

    fn used_bounds(&self, sheet: SheetId) -> Option<(u32, u32)> {
        Some(self.get(sheet).map_or((0, 0), |s| s.cells.extent()))
    }

    fn table(&self, name: &str) -> Option<TableDef> {
        self.tables.get(&name.to_uppercase()).cloned()
    }

    fn table_at(&self, sheet: SheetId, row: u32, col: u32) -> Option<TableDef> {
        let t = self.get(sheet)?.table_at(row, col)?;
        self.table(&t.name)
    }

    fn row_visibility(&self, sheet: SheetId, row: u32) -> RowVisibility {
        match self.get(sheet).and_then(|s| s.rows.get(row)) {
            Some(p) if p.filtered => RowVisibility::Filtered,
            Some(p) if p.hidden => RowVisibility::Hidden,
            _ => RowVisibility::Visible,
        }
    }
}

// ── records and change notes ──────────────────────────────────────────────

/// A cell's complete state (all-empty = no cell): what undo restores.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CellRecord {
    /// Row.
    pub row: u32,
    /// Column.
    pub col: u32,
    /// Value (cached result for a formula).
    #[serde(default)]
    pub value: CellValue,
    /// Formula text without `=`.
    #[serde(default)]
    pub formula: Option<String>,
    /// Pre-dynamic-array formula.
    #[serde(default)]
    pub legacy: bool,
    /// Style index.
    #[serde(default)]
    pub style: u32,
}

impl CellRecord {
    /// An empty record (no cell).
    pub fn empty(row: u32, col: u32) -> CellRecord {
        CellRecord {
            row,
            col,
            ..CellRecord::default()
        }
    }

    /// True when the record has a value or a formula.
    pub fn has_content(&self) -> bool {
        !self.value.is_blank() || self.formula.is_some()
    }
}

/// What changed since [`Workbook::take_changes`] was last called: what a renderer repaints and a
/// session reports.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Changes {
    /// Cells whose content or format changed.
    pub cells: Vec<(SheetId, Range)>,
    /// Formula cells whose result changed in a recalculation (an anchor stands for its spill
    /// range).
    pub recalculated: Vec<CellRef>,
    /// Sheets whose rows or columns were inserted, deleted or moved.
    pub structure: Vec<SheetId>,
    /// Sheets whose row or column properties changed.
    pub layout: Vec<SheetId>,
    /// Sheets whose metadata changed.
    pub meta: Vec<SheetId>,
    /// Sheets were added, removed, renamed, moved or hidden.
    pub sheets: bool,
    /// Defined names changed.
    pub names: bool,
    /// Workbook properties changed.
    pub props: bool,
}

impl Changes {
    fn push_unique(list: &mut Vec<SheetId>, id: SheetId) {
        if !list.contains(&id) {
            list.push(id);
        }
    }

    /// True when nothing changed.
    pub fn is_empty(&self) -> bool {
        *self == Changes::default()
    }
}

#[derive(Debug, Default)]
pub(crate) struct CalcState {
    /// Edits wait for a recalculation.
    pub(crate) pending: bool,
    /// Visibility of rows changed in a way the dependency graph cannot see.
    pub(crate) needs_full: bool,
    /// Nesting of command application (recalculate when the outermost one ends).
    pub(crate) depth: u32,
    /// Evaluations performed by the last recalculation.
    pub(crate) last_evaluated: usize,
}

/// Options for a new workbook.
#[derive(Clone, Default)]
pub struct WorkbookOptions {
    /// Sheets to create (`None` = one sheet named "Sheet1").
    pub sheets: Option<Vec<String>>,
    /// The clock for NOW()/TODAY() (`None` = the system clock).
    pub clock: Option<Clock>,
    /// Evaluate independent formulas in parallel (default: when the `parallel` feature is on).
    pub parallel: Option<bool>,
    /// The 1904 date system.
    pub date1904: bool,
}

impl std::fmt::Debug for WorkbookOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkbookOptions")
            .field("sheets", &self.sheets)
            .field("parallel", &self.parallel)
            .field("date1904", &self.date1904)
            .finish_non_exhaustive()
    }
}

/// Excel's sheet-name rules.
pub fn check_sheet_name(name: &str) -> Result<()> {
    let n = name.chars().count();
    if n == 0 || n > 31 {
        return Err(ModelError::invalid(
            "a sheet name must have 1 to 31 characters",
        ));
    }
    if name.contains([':', '\\', '/', '?', '*', '[', ']']) {
        return Err(ModelError::invalid(
            "a sheet name cannot contain : \\ / ? * [ or ]",
        ));
    }
    if name.starts_with('\'') || name.ends_with('\'') {
        return Err(ModelError::invalid(
            "a sheet name cannot begin or end with an apostrophe",
        ));
    }
    if name.eq_ignore_ascii_case("history") {
        return Err(ModelError::invalid("\"History\" is a reserved sheet name"));
    }
    Ok(())
}

/// True for defined-name text Excel accepts (including the reserved `_xlnm.` names).
pub fn is_defined_name_valid(name: &str) -> bool {
    if name.len() > 6 && name[..6].eq_ignore_ascii_case("_xlnm.") {
        return builtin_names::ALL
            .iter()
            .any(|b| b.eq_ignore_ascii_case(name));
    }
    kiln_formula::engine::is_valid_name(name)
}

pub(crate) fn strip_equals(text: &str) -> &str {
    let t = text.trim_start();
    t.strip_prefix('=').unwrap_or(t)
}

// ── the workbook ──────────────────────────────────────────────────────────

/// A workbook bound to the formula engine.
pub struct Workbook {
    pub(crate) engine: Engine<Sheets>,
    pub(crate) names: Vec<DefinedName>,
    pub(crate) styles: StyleTable,
    pub(crate) theme: Theme,
    pub(crate) props: WorkbookProps,
    pub(crate) protection: Option<WorkbookProtection>,
    pub(crate) persons: Vec<Person>,
    pub(crate) custom_lists: Vec<Vec<String>>,
    pub(crate) passthrough: BTreeMap<String, Vec<u8>>,
    pub(crate) next_sheet_id: u32,
    pub(crate) next_table_id: u32,
    pub(crate) next_rule_id: u32,
    pub(crate) mdw: f64,
    pub(crate) changes: Changes,
    pub(crate) calc: CalcState,
    pub(crate) clock: Option<Clock>,
    pub(crate) parallel: Option<bool>,
}

impl std::fmt::Debug for Workbook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workbook")
            .field("sheets", &self.sheet_names())
            .field("names", &self.names.len())
            .field("styles", &self.styles.len())
            .finish_non_exhaustive()
    }
}

impl Default for Workbook {
    fn default() -> Workbook {
        Workbook::new()
    }
}

impl Workbook {
    /// A workbook with one sheet, "Sheet1".
    pub fn new() -> Workbook {
        Workbook::with_options(WorkbookOptions::default())
    }

    /// A workbook with options.
    pub fn with_options(opts: WorkbookOptions) -> Workbook {
        let styles = StyleTable::default();
        let font = styles.default_font();
        let mdw = DefaultFontMetrics.max_digit_width(&font.name, font.size.0);
        let props = WorkbookProps {
            date1904: opts.date1904,
            ..WorkbookProps::default()
        };
        let engine = Engine::with_host(
            Sheets::default(),
            engine_options(&props, opts.clock.clone(), opts.parallel),
        );
        let mut wb = Workbook {
            engine,
            names: Vec::new(),
            styles,
            theme: Theme::default(),
            props,
            protection: None,
            persons: Vec::new(),
            custom_lists: Vec::new(),
            passthrough: BTreeMap::new(),
            next_sheet_id: 1,
            next_table_id: 1,
            next_rule_id: 1,
            mdw,
            changes: Changes::default(),
            calc: CalcState::default(),
            clock: opts.clock,
            parallel: opts.parallel,
        };
        let names = opts.sheets.unwrap_or_else(|| vec!["Sheet1".to_owned()]);
        for n in names {
            // Invalid or duplicate names in the options are skipped rather than panicking.
            let _ = wb.add_sheet(Some(&n), None, None);
        }
        wb.changes = Changes::default();
        wb
    }

    // ── access ─────────────────────────────────────────────────────────────

    pub(crate) fn host(&self) -> &Sheets {
        self.engine.host()
    }

    pub(crate) fn host_mut(&mut self) -> &mut Sheets {
        self.engine.host_mut()
    }

    /// The sheets in workbook order.
    pub fn sheets(&self) -> &[Sheet] {
        &self.host().list
    }

    /// Sheet names in workbook order.
    pub fn sheet_names(&self) -> Vec<String> {
        self.sheets().iter().map(|s| s.name.clone()).collect()
    }

    /// A sheet by id.
    pub fn sheet(&self, id: SheetId) -> Option<&Sheet> {
        self.host().get(id)
    }

    /// A sheet by name (ignoring case).
    pub fn sheet_by_name(&self, name: &str) -> Option<&Sheet> {
        let u = name.to_uppercase();
        self.sheets().iter().find(|s| s.name.to_uppercase() == u)
    }

    /// The position of a sheet in workbook order.
    pub fn sheet_index(&self, id: SheetId) -> Option<usize> {
        self.host().position(id)
    }

    pub(crate) fn sheet_ok(&self, id: SheetId) -> Result<&Sheet> {
        self.sheet(id)
            .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", id.0)))
    }

    pub(crate) fn sheet_mut(&mut self, id: SheetId) -> Result<&mut Sheet> {
        self.host_mut()
            .get_mut(id)
            .ok_or_else(|| ModelError::UnknownSheet(format!("with id {}", id.0)))
    }

    /// The first sheet's id (workbooks always have at least one sheet once built).
    pub fn first_sheet(&self) -> Option<SheetId> {
        self.sheets().first().map(Sheet::id)
    }

    /// Defined names.
    pub fn names(&self) -> &[DefinedName] {
        &self.names
    }

    /// The style table.
    pub fn styles(&self) -> &StyleTable {
        &self.styles
    }

    /// The style table, for adding formats (formats are interned, so adding never changes what
    /// existing indices mean).
    pub fn styles_mut(&mut self) -> &mut StyleTable {
        &mut self.styles
    }

    /// Replaces the style table (file readers). Cells keep their indices.
    pub fn set_styles(&mut self, styles: StyleTable) {
        self.styles = styles;
        let font = self.styles.default_font();
        let mdw = DefaultFontMetrics.max_digit_width(&font.name, font.size.0);
        self.set_max_digit_width(mdw);
    }

    /// Sets the maximum digit width of the default font (from real font metrics).
    pub fn set_max_digit_width(&mut self, mdw: f64) {
        if !(mdw.is_finite() && mdw >= 1.0) {
            return;
        }
        self.mdw = mdw;
        for s in &mut self.host_mut().list {
            s.refresh_geometry(mdw);
        }
    }

    /// The maximum digit width of the default font, in px.
    pub fn max_digit_width(&self) -> f64 {
        self.mdw
    }

    /// The theme.
    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// Replaces the theme (file readers).
    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Workbook properties.
    pub fn props(&self) -> &WorkbookProps {
        &self.props
    }

    /// Workbook protection.
    pub fn protection(&self) -> Option<&WorkbookProtection> {
        self.protection.as_ref()
    }

    /// People who wrote threaded comments.
    pub fn persons(&self) -> &[Person] {
        &self.persons
    }

    /// Replaces the list of people (file readers).
    pub fn set_persons(&mut self, persons: Vec<Person>) {
        self.persons = persons;
    }

    /// Custom fill and sort lists (File › Options › Edit Custom Lists).
    pub fn custom_lists(&self) -> &[Vec<String>] {
        &self.custom_lists
    }

    /// Replaces the custom lists.
    pub fn set_custom_lists(&mut self, lists: Vec<Vec<String>>) {
        self.custom_lists = lists;
    }

    /// Opaque parts the file layer keeps for round trips (part name → bytes).
    pub fn passthrough(&self) -> &BTreeMap<String, Vec<u8>> {
        &self.passthrough
    }

    /// The passthrough parts, mutably.
    pub fn passthrough_mut(&mut self) -> &mut BTreeMap<String, Vec<u8>> {
        &mut self.passthrough
    }

    /// The formula engine (read-only: formulas, results, spill ranges, the Formula Lab).
    pub fn engine(&self) -> &Engine<Sheets> {
        &self.engine
    }

    /// What changed since the last call.
    pub fn take_changes(&mut self) -> Changes {
        std::mem::take(&mut self.changes)
    }

    pub(crate) fn note_cells(&mut self, sheet: SheetId, r: Range) {
        let cells = &mut self.changes.cells;
        if let Some((s, last)) = cells.last_mut()
            && *s == sheet
        {
            if last.contains(r.r0, r.c0) && last.contains(r.r1, r.c1) {
                return;
            }
            // A cell continuing a row run extends it.
            if last.r0 == last.r1 && r.r0 == last.r0 && r.r1 == last.r1 && r.c0 == last.c1 + 1 {
                last.c1 = r.c1;
                return;
            }
            // A row run directly below a block of the same width extends the block.
            if r.c0 == last.c0 && r.c1 == last.c1 && r.r0 == last.r1 + 1 {
                last.r1 = r.r1;
                return;
            }
        }
        cells.push((sheet, r));
        // Large edits collapse to one bounding range per sheet.
        if cells.len() > 4096 {
            let mut boxes: Vec<(SheetId, Range)> = Vec::new();
            for (s, x) in cells.drain(..) {
                match boxes.iter_mut().find(|(b, _)| *b == s) {
                    Some((_, b)) => *b = b.union(&x),
                    None => boxes.push((s, x)),
                }
            }
            *cells = boxes;
        }
    }

    pub(crate) fn note_layout(&mut self, sheet: SheetId) {
        Changes::push_unique(&mut self.changes.layout, sheet);
    }

    pub(crate) fn note_meta(&mut self, sheet: SheetId) {
        Changes::push_unique(&mut self.changes.meta, sheet);
    }

    pub(crate) fn note_structure(&mut self, sheet: SheetId) {
        Changes::push_unique(&mut self.changes.structure, sheet);
    }

    // ── ids ────────────────────────────────────────────────────────────────

    pub(crate) fn alloc_table_id(&mut self) -> u32 {
        let id = self.next_table_id;
        self.next_table_id += 1;
        id
    }

    /// A new workbook-unique id for a conditional-format rule or validation rule.
    pub fn alloc_rule_id(&mut self) -> u32 {
        let id = self.next_rule_id;
        self.next_rule_id += 1;
        id
    }

    /// The next unused default sheet name ("Sheet2", "Sheet3"…).
    pub fn next_sheet_name(&self) -> String {
        (1..)
            .map(|i| format!("Sheet{i}"))
            .find(|n| self.sheet_by_name(n).is_none())
            .unwrap_or_default()
    }

    /// The next free default table name ("Table1", "Table2"…).
    pub fn next_table_name(&self) -> String {
        (1..)
            .map(|i| format!("Table{i}"))
            .find(|n| self.find_table(n).is_none() && self.find_name_any_scope(n).is_none())
            .unwrap_or_default()
    }

    // ── sheets (low level) ─────────────────────────────────────────────────

    /// Adds a sheet (low level: no undo). `name` defaults to the next "SheetN", `index` to the
    /// end, `id` to a fresh one.
    pub fn add_sheet(
        &mut self,
        name: Option<&str>,
        index: Option<usize>,
        id: Option<SheetId>,
    ) -> Result<SheetId> {
        let name = name.map_or_else(|| self.next_sheet_name(), str::to_owned);
        check_sheet_name(&name)?;
        if self.sheet_by_name(&name).is_some() {
            return Err(ModelError::invalid(format!(
                "a sheet named \"{name}\" already exists"
            )));
        }
        let id = id.unwrap_or(SheetId(self.next_sheet_id));
        if self.sheet(id).is_some() {
            return Err(ModelError::invalid(format!(
                "sheet id {} is already used",
                id.0
            )));
        }
        let sheet = Sheet::new(id, &name, self.mdw);
        self.insert_sheet_object(sheet, Vec::new(), index.unwrap_or(usize::MAX))?;
        Ok(id)
    }

    /// Inserts a complete sheet with the formulas of its engine-formula cells (restoring a
    /// removed sheet, or a deserialised one).
    pub(crate) fn insert_sheet_object(
        &mut self,
        mut sheet: Sheet,
        formulas: Vec<(u32, u32, String, bool)>,
        index: usize,
    ) -> Result<()> {
        let id = sheet.id();
        if self.sheet(id).is_some() {
            return Err(ModelError::invalid(format!(
                "sheet id {} is already used",
                id.0
            )));
        }
        check_sheet_name(&sheet.name)?;
        if self.sheet_by_name(&sheet.name).is_some() {
            return Err(ModelError::invalid(format!(
                "a sheet named \"{}\" already exists",
                sheet.name
            )));
        }
        sheet.refresh_geometry(self.mdw);
        self.engine
            .add_sheet_with_id(&sheet.name, id)
            .map_err(|e| ModelError::invalid(e.to_string()))?;
        self.next_sheet_id = self.next_sheet_id.max(id.0.saturating_add(1));
        let at = index.min(self.sheets().len());
        let host = self.host_mut();
        host.list.insert(at, sheet);
        host.reindex();
        let _ = self.engine.move_sheet(id, at);
        for (row, col, text, legacy) in formulas {
            self.bind_formula(id, row, col, &text, legacy);
        }
        // Literal values of the new sheet may be read by formulas elsewhere (3-D references,
        // formulas that were waiting for this sheet name).
        let cells: Vec<(u32, u32)> = self
            .sheet(id)
            .map(|s| {
                s.cells
                    .iter()
                    .filter(|(_, _, d)| !d.kind.is_engine_formula() && !d.value.is_blank())
                    .map(|(r, c, _)| (r, c))
                    .collect()
            })
            .unwrap_or_default();
        for (r, c) in cells {
            let _ = self.engine.cell_changed(id, r, c);
        }
        self.engine.tables_changed();
        self.sync_names();
        self.changes.sheets = true;
        self.calc.pending = true;
        Ok(())
    }

    // ── cells ──────────────────────────────────────────────────────────────

    /// The stored cell (value, kind, style).
    pub fn cell(&self, sheet: SheetId, row: u32, col: u32) -> Option<CellData<'_>> {
        self.sheet(sheet)?.cells.get(row, col)
    }

    /// The displayed value of a cell: a formula's result (the top-left value of a spill,
    /// `#SPILL!` when blocked), a value spilled into the cell, or the literal value.
    pub fn value(&self, sheet: SheetId, row: u32, col: u32) -> CellValue {
        let Some(s) = self.sheet(sheet) else {
            return CellValue::Blank;
        };
        match s.cells.get(row, col) {
            Some(c) if c.kind.is_engine_formula() => self
                .engine
                .value(sheet, row, col)
                .map_or(CellValue::Blank, |v| CellValue::from_value(&v)),
            Some(c) if !c.value.is_blank() => c.value.clone(),
            _ => self
                .engine
                .value(sheet, row, col)
                .map_or(CellValue::Blank, |v| CellValue::from_value(&v)),
        }
    }

    /// The formula of a cell without `=`.
    pub fn formula(&self, sheet: SheetId, row: u32, col: u32) -> Option<String> {
        let s = self.sheet(sheet)?;
        match s.cells.kind(row, col) {
            CellKind::Formula | CellKind::LegacyFormula => self
                .engine
                .formula(sheet, row, col)
                .map(|t| strip_equals(t).to_owned()),
            CellKind::RawFormula => s.cells.raw_formula(row, col).map(|t| t.to_string()),
            CellKind::Value => None,
        }
    }

    /// The complete state of a cell.
    pub fn record(&self, sheet: SheetId, row: u32, col: u32) -> CellRecord {
        let Some(c) = self.cell(sheet, row, col) else {
            return CellRecord::empty(row, col);
        };
        CellRecord {
            row,
            col,
            value: c.value.clone(),
            formula: if c.kind.is_formula() {
                self.formula(sheet, row, col)
            } else {
                None
            },
            legacy: c.kind == CellKind::LegacyFormula,
            style: c.style,
        }
    }

    /// Records of every stored cell in a range.
    pub fn records_in(&self, sheet: SheetId, r: Range) -> Vec<CellRecord> {
        let Some(s) = self.sheet(sheet) else {
            return Vec::new();
        };
        let cells: Vec<(u32, u32)> = s
            .cells
            .iter_range(r)
            .map(|(row, col, _)| (row, col))
            .collect();
        cells
            .into_iter()
            .map(|(row, col)| self.record(sheet, row, col))
            .collect()
    }

    /// Binds formula text to the engine (or keeps it raw when it does not parse).
    pub(crate) fn bind_formula(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        text: &str,
        legacy: bool,
    ) {
        let input = CellInput::Formula {
            text: format!("={}", strip_equals(text)),
            legacy,
        };
        if self.engine.set_cell(sheet, row, col, input).is_err() {
            if self.engine.has_formula(sheet, row, col) {
                let _ = self.engine.clear_cell(sheet, row, col);
            }
            if let Ok(s) = self.sheet_mut(sheet) {
                let cached = match s.cells.value(row, col) {
                    CellValue::Blank => CellValue::Error(ErrorValue::Name),
                    v => v.clone(),
                };
                s.cells
                    .set_raw_formula(row, col, strip_equals(text), cached);
            }
        }
    }

    /// Writes a complete cell record, keeping the engine in step (low level: no undo, no
    /// validation of the formula — text that does not parse becomes a raw formula).
    pub fn write_record(&mut self, sheet: SheetId, rec: &CellRecord) -> Result<()> {
        check_cell(rec.row, rec.col)?;
        let (row, col) = (rec.row, rec.col);
        let s = self.sheet_mut(sheet)?;
        let old = s.cells.get(row, col).map(|c| (c.kind, c.value.clone()));
        let old_kind = old.as_ref().map_or(CellKind::Value, |o| o.0);
        match &rec.formula {
            Some(f) => {
                let kind = if rec.legacy {
                    CellKind::LegacyFormula
                } else {
                    CellKind::Formula
                };
                s.cells.set(row, col, rec.value.clone(), kind, rec.style);
                self.bind_formula(sheet, row, col, f, rec.legacy);
            }
            None => {
                let content_changed = old
                    .as_ref()
                    .is_none_or(|(k, v)| k.is_formula() || *v != rec.value);
                s.cells
                    .set(row, col, rec.value.clone(), CellKind::Value, rec.style);
                if old_kind.is_engine_formula() {
                    let _ = self.engine.clear_cell(sheet, row, col);
                } else if content_changed {
                    let _ = self.engine.cell_changed(sheet, row, col);
                }
            }
        }
        self.note_cells(sheet, Range::cell(row, col));
        self.calc.pending = true;
        Ok(())
    }

    /// Sets a literal value (low level: no undo), keeping the cell's style.
    pub fn load_value(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        value: CellValue,
    ) -> Result<()> {
        let style = self.sheet_ok(sheet)?.cells.style(row, col);
        self.write_record(
            sheet,
            &CellRecord {
                row,
                col,
                value,
                formula: None,
                legacy: false,
                style,
            },
        )
    }

    /// Sets a formula with an optional cached result (low level: no undo). Text the engine
    /// cannot parse is kept as a raw formula showing the cached value (or `#NAME?`).
    pub fn load_formula(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        text: &str,
        cached: CellValue,
        legacy: bool,
    ) -> Result<()> {
        let style = self.sheet_ok(sheet)?.cells.style(row, col);
        self.write_record(
            sheet,
            &CellRecord {
                row,
                col,
                value: cached,
                formula: Some(strip_equals(text).to_owned()),
                legacy,
                style,
            },
        )
    }

    /// Sets a cell's style index (low level: no undo).
    pub fn load_style(&mut self, sheet: SheetId, row: u32, col: u32, style: u32) -> Result<()> {
        check_cell(row, col)?;
        self.styles.check(style)?;
        self.sheet_mut(sheet)?.cells.set_style(row, col, style);
        self.note_cells(sheet, Range::cell(row, col));
        Ok(())
    }

    // ── names ──────────────────────────────────────────────────────────────

    /// A name in exactly this scope.
    pub fn find_name(&self, name: &str, scope: Option<SheetId>) -> Option<&DefinedName> {
        self.names
            .iter()
            .find(|n| n.scope == scope && n.name.eq_ignore_ascii_case(name))
    }

    fn find_name_any_scope(&self, name: &str) -> Option<&DefinedName> {
        self.names
            .iter()
            .find(|n| n.name.eq_ignore_ascii_case(name))
    }

    /// A name as seen from a sheet: a local name wins over a workbook-level one.
    pub fn resolve_name(&self, name: &str, from: SheetId) -> Option<&DefinedName> {
        self.find_name(name, Some(from))
            .or_else(|| self.find_name(name, None))
    }

    /// Checks a defined name is valid in its scope and does not clash with a table.
    pub fn check_defined_name(&self, name: &str, scope: Option<SheetId>) -> Result<()> {
        if !is_defined_name_valid(name) {
            return Err(ModelError::invalid(format!(
                "\"{name}\" is not a valid name"
            )));
        }
        if let Some(s) = scope {
            self.sheet_ok(s)?;
        }
        if self.find_table(name).is_some() {
            return Err(ModelError::invalid(format!(
                "\"{name}\" is already used by a table"
            )));
        }
        Ok(())
    }

    /// Defines (or replaces) a name (low level: no undo).
    pub fn load_name(&mut self, def: DefinedName) -> Result<()> {
        self.check_defined_name(&def.name, def.scope)?;
        let formula = strip_equals(&def.formula).to_owned();
        let def = DefinedName { formula, ..def };
        match self
            .names
            .iter_mut()
            .find(|n| n.scope == def.scope && n.name.eq_ignore_ascii_case(&def.name))
        {
            Some(n) => *n = def,
            None => self.names.push(def),
        }
        self.sync_names();
        Ok(())
    }

    /// Brings the engine's names in line with the model's.
    pub(crate) fn sync_names(&mut self) {
        let wanted: FxHashMap<(Option<u32>, String), String> = self
            .names
            .iter()
            .map(|n| {
                (
                    (n.scope.map(|s| s.0), n.name.to_uppercase()),
                    n.formula.clone(),
                )
            })
            .collect();
        let mut have: FxHashMap<(Option<u32>, String), String> = FxHashMap::default();
        for n in self.engine.names() {
            let k = (n.scope.map(|s| s.0), n.name.to_uppercase());
            match wanted.get(&k) {
                Some(f) if strip_equals(&n.formula) == f => {
                    have.insert(k, n.formula);
                }
                _ => {
                    self.engine.remove_name(&n.name, n.scope);
                }
            }
        }
        let missing: Vec<DefinedName> = self
            .names
            .iter()
            .filter(|n| !have.contains_key(&(n.scope.map(|s| s.0), n.name.to_uppercase())))
            .cloned()
            .collect();
        for n in missing {
            // Names the engine cannot evaluate (external references, reserved names) stay in
            // the model only.
            let _ = self.engine.define_name(&n.name, &n.formula, n.scope);
        }
        self.changes.names = true;
        self.calc.pending = true;
    }

    // ── tables ─────────────────────────────────────────────────────────────

    /// A table by name (ignoring case) and its sheet.
    pub fn find_table(&self, name: &str) -> Option<(SheetId, &crate::meta::Table)> {
        self.sheets().iter().find_map(|s| {
            s.tables()
                .iter()
                .find(|t| t.name.eq_ignore_ascii_case(name))
                .map(|t| (s.id(), t))
        })
    }

    /// Tells the engine the tables changed.
    pub(crate) fn tables_changed(&mut self) {
        self.host_mut().reindex_tables();
        self.engine.tables_changed();
        self.calc.pending = true;
    }

    /// Replaces a sheet's metadata (low level: no undo), keeping the engine in step.
    pub fn load_sheet_meta(&mut self, sheet: SheetId, meta: crate::sheet::SheetMeta) -> Result<()> {
        let s = self.sheet_mut(sheet)?;
        let tables = s.meta.tables != meta.tables;
        let vis = s.meta.visibility != meta.visibility;
        s.set_meta(meta);
        if tables {
            self.tables_changed();
        }
        if vis {
            self.changes.sheets = true;
        }
        self.note_meta(sheet);
        Ok(())
    }

    /// Replaces a sheet's row and column properties (low level: no undo).
    pub fn load_sheet_layout(
        &mut self,
        sheet: SheetId,
        layout: crate::sheet::SheetLayout,
    ) -> Result<()> {
        let s = self.sheet_mut(sheet)?;
        let before = hidden_rows(s);
        s.set_layout(layout);
        let after = hidden_rows(self.sheet_ok(sheet)?);
        self.visibility_changed(sheet, before, after);
        self.note_layout(sheet);
        Ok(())
    }

    /// Tells the engine about rows whose visibility changed (SUBTOTAL/AGGREGATE read it).
    pub(crate) fn visibility_changed(
        &mut self,
        sheet: SheetId,
        before: Vec<(u32, RowVisibility)>,
        after: Vec<(u32, RowVisibility)>,
    ) {
        if before == after {
            return;
        }
        let a: FxHashMap<u32, RowVisibility> = before.into_iter().collect();
        let b: FxHashMap<u32, RowVisibility> = after.into_iter().collect();
        let mut rows: Vec<u32> = a
            .iter()
            .filter(|(r, v)| b.get(r) != Some(v))
            .map(|(&r, _)| r)
            .chain(
                b.iter()
                    .filter(|(r, v)| a.get(r) != Some(v))
                    .map(|(&r, _)| r),
            )
            .collect();
        rows.sort_unstable();
        rows.dedup();
        let Some(s) = self.sheet(sheet) else { return };
        let cells: Vec<(u32, u32)> = rows
            .iter()
            .flat_map(|&r| {
                s.cells
                    .chunk(r)
                    .into_iter()
                    .flat_map(move |ch| ch.cols().iter().map(move |&c| (r, u32::from(c))))
            })
            .collect();
        for (r, c) in cells {
            let _ = self.engine.cell_changed(sheet, r, c);
        }
        self.calc.pending = true;
    }

    // ── formula rewriting of metadata ──────────────────────────────────────

    /// Rewrites every formula the model itself stores — defined names, conditional formats,
    /// validation criteria, table column formulas — through `f(text, formula_sheet, at)`, where
    /// `formula_sheet` is the name of the sheet the formula lives on (empty for workbook-scoped
    /// names) and `at` the anchor cell of a range-relative formula. `None` keeps the text.
    pub(crate) fn rewrite_model_formulas(&mut self, f: &MetaRewrite<'_>) {
        let sheet_names: FxHashMap<SheetId, String> = self
            .sheets()
            .iter()
            .map(|s| (s.id(), s.name.clone()))
            .collect();
        for n in &mut self.names {
            let fs = n
                .scope
                .and_then(|s| sheet_names.get(&s).cloned())
                .unwrap_or_default();
            if let Some(t) = f(&n.formula, &fs, None) {
                n.formula = t;
            }
        }
        let mut tables_changed = false;
        for s in &mut self.engine.host_mut().list {
            let name = s.name.clone();
            for block in &mut s.meta.cf {
                let at = block.ranges.first().map(|r| CellPos::new(r.r0, r.c0));
                for rule in &mut block.rules {
                    for text in rule.kind.formulas_mut() {
                        if let Some(t) = f(text, &name, at) {
                            *text = t;
                        }
                    }
                }
            }
            for v in &mut s.meta.validations {
                let at = v.ranges.first().map(|r| CellPos::new(r.r0, r.c0));
                let literal_list = v.kind == crate::meta::ValidationType::List
                    && v.formula1
                        .as_deref()
                        .is_some_and(|t| t.trim_start().starts_with('"'));
                if !literal_list && let Some(t) = v.formula1.as_ref().and_then(|x| f(x, &name, at))
                {
                    v.formula1 = Some(t);
                }
                if let Some(t) = v.formula2.as_ref().and_then(|x| f(x, &name, at)) {
                    v.formula2 = Some(t);
                }
            }
            for t in &mut s.meta.tables {
                let at = Some(CellPos::new(
                    t.range.r0 + u32::from(t.header_row),
                    t.range.c0,
                ));
                for c in &mut t.columns {
                    if let Some(x) = c.calculated_formula.as_ref().and_then(|x| f(x, &name, at)) {
                        c.calculated_formula = Some(x);
                        tables_changed = true;
                    }
                    if let Some(x) = c.totals_formula.as_ref().and_then(|x| f(x, &name, at)) {
                        c.totals_formula = Some(x);
                        tables_changed = true;
                    }
                }
            }
        }
        if tables_changed {
            self.host_mut().reindex_tables();
        }
    }

    /// Applies a structural edit to model data (after validation): cells, axes and metadata of
    /// the sheet, then the engine.
    pub(crate) fn apply_structural_raw(
        &mut self,
        sheet: SheetId,
        e: &StructuralEdit,
    ) -> Result<()> {
        let name = self.sheet_ok(sheet)?.name.clone();
        self.sheet_mut(sheet)?.apply_structural(e)?;
        let edit = *e;
        self.rewrite_model_formulas(&|text, fs, _| {
            kiln_formula::refs::shift_formula(text, &edit, &name, fs)
                .ok()
                .filter(|t| t != text)
        });
        let r = match e.kind {
            kiln_formula::refs::StructuralKind::InsertRows => {
                self.engine.insert_rows(sheet, e.at, e.count)
            }
            kiln_formula::refs::StructuralKind::DeleteRows => {
                self.engine.delete_rows(sheet, e.at, e.count)
            }
            kiln_formula::refs::StructuralKind::InsertCols => {
                self.engine.insert_cols(sheet, e.at, e.count)
            }
            kiln_formula::refs::StructuralKind::DeleteCols => {
                self.engine.delete_cols(sheet, e.at, e.count)
            }
        };
        r.map_err(|x| ModelError::invalid(x.to_string()))?;
        self.host_mut().reindex_tables();
        self.engine.tables_changed();
        self.sync_names();
        self.note_structure(sheet);
        self.note_layout(sheet);
        self.note_meta(sheet);
        self.calc.pending = true;
        Ok(())
    }

    /// Calculation mode.
    pub fn calc_mode(&self) -> CalcMode {
        self.props.calc.mode
    }

    /// Visible sheets.
    pub fn visible_sheet_count(&self) -> usize {
        self.sheets()
            .iter()
            .filter(|s| s.meta.visibility == SheetVisibility::Visible)
            .count()
    }
}

/// A rewrite of formula text: `(text, formula_sheet, anchor) -> new text`.
pub(crate) type MetaRewrite<'f> = dyn Fn(&str, &str, Option<CellPos>) -> Option<String> + 'f;

pub(crate) fn hidden_rows(s: &Sheet) -> Vec<(u32, RowVisibility)> {
    s.rows
        .entries()
        .filter(|(_, p)| p.hidden)
        .map(|(r, p)| {
            (
                r,
                if p.filtered {
                    RowVisibility::Filtered
                } else {
                    RowVisibility::Hidden
                },
            )
        })
        .collect()
}

pub(crate) fn engine_options(
    props: &WorkbookProps,
    clock: Option<Clock>,
    parallel: Option<bool>,
) -> EngineOptions {
    let mut o = EngineOptions {
        registry: Some(functions::library()),
        date1904: props.date1904,
        iterative: IterativeOptions {
            enabled: props.calc.iterate,
            max_iterations: props.calc.iterate_count.max(1),
            max_change: props.calc.iterate_delta.max(0.0),
        },
        clock,
        ..EngineOptions::default()
    };
    if let Some(p) = parallel {
        o.parallel = p;
    }
    o
}
