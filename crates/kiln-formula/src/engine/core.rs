// SPDX-License-Identifier: AGPL-3.0-or-later
//! The calculation engine: formulas, the dependency graph, incremental recalculation by
//! topological levels (independent cells of a level evaluated in parallel), dynamic-array
//! spilling, defined names, structural edits and ad-hoc evaluation.
//!
//! ```
//! use kiln_formula::engine::Engine;
//! use kiln_formula::values::Value;
//!
//! let mut engine = Engine::new();
//! engine.add_sheet("Sheet1").unwrap();
//! engine.set_value("Sheet1", 0, 0, 2.0).unwrap();
//! engine.set_formula("Sheet1", 0, 1, "=A1*21").unwrap();
//! engine.recalc();
//! assert_eq!(engine.value("Sheet1", 0, 1).unwrap(), Value::Number(42.0));
//! ```

use std::sync::Arc;

use rustc_hash::{FxHashMap, FxHashSet};

use super::eval::{CellSource, Evaluator, Workbook};
use super::explain::{ExplainStep, build_explain_tree};
use super::expr::{CompileContext, Expr, FuncRef, NameScope, SheetSel, compile};
use super::graph::{FormulaId, Graph, Precedents, Registration};
use super::host::{EngineHost, HostReadOnly, MemoryHost, TableDef};
use super::scc::tarjan;
use super::structured::resolve_structured;
use super::types::{Area, RowVisibility, SheetId, cell_key};
use crate::parser::{
    ParseError, ParseOptions, PrintOptions, RefStyle, is_name_start, match_a1_address,
    match_r1c1_address, parse, print,
};
use crate::refs::{
    Bounds, MAX_COLS, MAX_ROWS, MoveSpec, StructuralEdit, StructuralKind, move_formula,
    remove_sheet_in_formula, rename_sheet_in_formula, shift_formula,
};
use crate::registry::{FunctionId, FunctionRegistry, FunctionTable};
use crate::values::{CoercionOptions, ErrorValue, Value, serial_to_date};

/// A clock for `NOW()`/`TODAY()`: returns the current date-time as a serial in the wall-clock
/// time the workbook should show.
pub type Clock = Arc<dyn Fn() -> f64 + Send + Sync>;

/// Iterative calculation of circular references (File › Options › Formulas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IterativeOptions {
    /// Iterate cycles instead of flagging them.
    pub enabled: bool,
    /// Maximum iterations (Excel's default 100).
    pub max_iterations: u32,
    /// Stop when no value changes by more than this (Excel's default 0.001).
    pub max_change: f64,
}

impl Default for IterativeOptions {
    fn default() -> IterativeOptions {
        IterativeOptions {
            enabled: false,
            max_iterations: 100,
            max_change: 0.001,
        }
    }
}

/// Engine settings.
#[derive(Clone)]
pub struct EngineOptions {
    /// Use the 1904 date system.
    pub date1904: bool,
    /// Iterative calculation.
    pub iterative: IterativeOptions,
    /// Evaluate independent cells of a topological level on the current rayon pool. Ignored
    /// (always sequential) when the crate is built without the `parallel` feature. Results are
    /// identical either way.
    pub parallel: bool,
    /// The clock; `None` uses the system clock in UTC (on `wasm32-unknown-unknown`, where there
    /// is no system clock, the serial of 1970-01-01).
    pub clock: Option<Clock>,
    /// The functions; `None` gives the core set ([`FunctionTable::with_core`]).
    pub registry: Option<Arc<dyn FunctionRegistry>>,
}

// Not derived: `parallel` defaults to whether the `parallel` feature is built in.
#[allow(clippy::derivable_impls)]
impl Default for EngineOptions {
    fn default() -> EngineOptions {
        EngineOptions {
            date1904: false,
            iterative: IterativeOptions::default(),
            parallel: cfg!(feature = "parallel"),
            clock: None,
            registry: None,
        }
    }
}

impl std::fmt::Debug for EngineOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EngineOptions")
            .field("date1904", &self.date1904)
            .field("iterative", &self.iterative)
            .field("parallel", &self.parallel)
            .field("clock", &self.clock.as_ref().map(|_| "custom"))
            .finish_non_exhaustive()
    }
}

fn system_clock() -> f64 {
    #[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
    {
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64() * 1000.0)
            .unwrap_or(0.0);
        crate::values::serial_from_unix_ms(ms, false)
    }
    #[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
    {
        25_569.0
    }
}

/// Errors from engine operations (formula errors are values, not these).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    /// No sheet with that name or id.
    #[error("unknown sheet {0}")]
    UnknownSheet(String),
    /// A sheet with that name already exists.
    #[error("a sheet named \"{0}\" already exists")]
    DuplicateSheet(String),
    /// A sheet id is already used.
    #[error("sheet id {0} is already used")]
    DuplicateSheetId(u32),
    /// A row or column outside the sheet.
    #[error("cell ({row}, {col}) is outside the sheet")]
    OutOfRange {
        /// Row.
        row: u64,
        /// Column.
        col: u64,
    },
    /// The formula has a syntax error.
    #[error("{0}")]
    Parse(#[from] ParseError),
    /// Not a valid defined name.
    #[error("\"{0}\" is not a valid name")]
    InvalidName(String),
    /// The host does not store values.
    #[error(transparent)]
    HostReadOnly(#[from] HostReadOnly),
}

/// A sheet given by name (case-insensitive) or id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetKey<'a> {
    /// By id.
    Id(SheetId),
    /// By name.
    Name(&'a str),
}

/// Anything that names a sheet.
pub trait SheetRef {
    /// The key.
    fn sheet_key(&self) -> SheetKey<'_>;
}

impl SheetRef for SheetId {
    fn sheet_key(&self) -> SheetKey<'_> {
        SheetKey::Id(*self)
    }
}

impl SheetRef for &str {
    fn sheet_key(&self) -> SheetKey<'_> {
        SheetKey::Name(self)
    }
}

impl SheetRef for String {
    fn sheet_key(&self) -> SheetKey<'_> {
        SheetKey::Name(self)
    }
}

impl SheetRef for &String {
    fn sheet_key(&self) -> SheetKey<'_> {
        SheetKey::Name(self)
    }
}

/// What to put in a cell.
#[derive(Debug, Clone, PartialEq)]
pub enum CellInput {
    /// A formula (with or without the leading `=`).
    Formula {
        /// The text.
        text: String,
        /// Evaluate with pre-dynamic-array semantics (implicit intersection, no spilling).
        legacy: bool,
    },
    /// A literal value (a blank clears).
    Value(Value),
    /// Nothing.
    Clear,
}

/// A cell position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CellRef {
    /// Sheet.
    pub sheet: SheetId,
    /// Row (0-based).
    pub row: u32,
    /// Column (0-based).
    pub col: u32,
}

/// The rectangle a dynamic array occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpillRange {
    /// Sheet.
    pub sheet: SheetId,
    /// The anchor and the spill area.
    pub bounds: Bounds,
    /// Something is in the way, so the anchor shows `#SPILL!`.
    pub blocked: bool,
}

/// What a recalculation did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RecalcResult {
    /// Formula evaluations performed.
    pub evaluated: usize,
    /// Formula cells whose result changed (a spill anchor stands for its whole spill range).
    pub changed: Vec<CellRef>,
    /// Cells that are part of a circular reference.
    pub circular: Vec<CellRef>,
}

/// Graph statistics (diagnostics and tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GraphStats {
    /// Formula cells.
    pub formulas: usize,
    /// Distinct range precedents (each one node, however large).
    pub range_nodes: usize,
    /// Single-cell precedent edges.
    pub cell_edges: usize,
    /// Formulas waiting for recalculation.
    pub dirty: usize,
    /// Distinct compiled formulas (shared by formulas filled across cells).
    pub compiled: usize,
}

/// A defined name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameInfo {
    /// The name as defined.
    pub name: String,
    /// Its formula, with `=`.
    pub formula: String,
    /// The sheet it is local to, or `None` for workbook scope.
    pub scope: Option<SheetId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Spill {
    None,
    Ok,
    Blocked,
}

const DIRTY: u8 = 1;
const EVALUATING: u8 = 2;
const CIRCULAR: u8 = 4;
const IN_BATCH: u8 = 8;
const BATCH_STALE: u8 = 16;
const UNRESOLVED: u8 = 32;
const USES_TABLES: u8 = 64;
const VOLATILE: u8 = 128;

#[derive(Debug)]
struct FormulaCell {
    sheet: SheetId,
    row: u32,
    col: u32,
    text: Box<str>,
    code: Arc<Expr>,
    code_key: Arc<str>,
    legacy: bool,
    value: Value,
    spill: Spill,
    claim: Option<Bounds>,
    flags: u8,
    changed_epoch: u32,
    hyperlink: Option<Box<str>>,
    reg: Registration,
}

impl FormulaCell {
    fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }

    fn set(&mut self, f: u8, on: bool) {
        if on {
            self.flags |= f;
        } else {
            self.flags &= !f;
        }
    }

    /// What the anchor cell shows.
    fn anchor_value(&self) -> Value {
        match (&self.spill, &self.value) {
            (Spill::Blocked, _) => Value::Error(ErrorValue::Spill),
            (_, Value::Array(a)) => a.get(0, 0).clone(),
            (_, v) => v.clone(),
        }
    }
}

#[derive(Debug)]
struct NameDef {
    name: String,
    text: String,
    code: Arc<Expr>,
    scope: Option<SheetId>,
}

/// Most recalculation passes per `recalc()` (spills that resize can make later passes necessary;
/// the bound stops pathological oscillation).
const MAX_PASSES: usize = 64;
/// Smallest topological level worth handing to the thread pool.
#[cfg(feature = "parallel")]
const PARALLEL_THRESHOLD: usize = 128;
/// Smallest pass whose dependency edges are collected on the thread pool.
#[cfg(feature = "parallel")]
const PARALLEL_EDGES_THRESHOLD: usize = 8192;
/// Deepest chain of names followed when collecting precedents.
const MAX_NAME_DEPTH: usize = 16;

/// The calculation engine over a host that stores literal values.
pub struct Engine<H: EngineHost = MemoryHost> {
    host: H,
    registry: Arc<dyn FunctionRegistry>,
    date1904: bool,
    iterative: IterativeOptions,
    parallel: bool,
    clock: Clock,
    now: f64,
    coercion: CoercionOptions,

    sheet_names: FxHashMap<SheetId, String>,
    sheet_by_name: FxHashMap<String, SheetId>,
    sheet_order: Vec<SheetId>,
    next_sheet_id: u32,

    cells: Vec<Option<FormulaCell>>,
    free: Vec<FormulaId>,
    by_key: FxHashMap<u64, FormulaId>,
    graph: Graph,
    names: FxHashMap<(Option<SheetId>, String), NameDef>,
    formula_bounds: FxHashMap<SheetId, (u32, u32)>,
    interner: FxHashMap<Arc<str>, Arc<Expr>>,

    dirty: Vec<FormulaId>,
    dirty_count: usize,
    scratch_indeg: Vec<u32>,
    scratch_pos: Vec<u32>,
    volatiles: FxHashSet<FormulaId>,
    circular: FxHashSet<FormulaId>,
    changed: Vec<FormulaId>,
    epoch: u32,
    evaluated: usize,
}

impl<H: EngineHost> std::fmt::Debug for Engine<H> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("sheets", &self.sheet_order.len())
            .field("formulas", &self.by_key.len())
            .field("dirty", &self.dirty_count)
            .finish_non_exhaustive()
    }
}

impl Engine<MemoryHost> {
    /// An engine over an in-memory host with default options.
    pub fn new() -> Engine<MemoryHost> {
        Engine::with_host(MemoryHost::new(), EngineOptions::default())
    }

    /// An engine over an in-memory host.
    pub fn with_options(options: EngineOptions) -> Engine<MemoryHost> {
        Engine::with_host(MemoryHost::new(), options)
    }
}

impl Default for Engine<MemoryHost> {
    fn default() -> Engine<MemoryHost> {
        Engine::new()
    }
}

fn name_key(name: &str, scope: Option<SheetId>) -> (Option<SheetId>, String) {
    (scope, name.to_uppercase())
}

fn check_cell(row: u32, col: u32) -> Result<(), EngineError> {
    if row >= MAX_ROWS || col >= MAX_COLS {
        return Err(EngineError::OutOfRange {
            row: u64::from(row),
            col: u64::from(col),
        });
    }
    Ok(())
}

fn with_equals(text: &str) -> String {
    if text.trim_start().starts_with('=') {
        text.to_owned()
    } else {
        format!("={text}")
    }
}

fn numeric_of(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        Value::Bool(b) => f64::from(u8::from(*b)),
        Value::Array(a) => numeric_of(a.get(0, 0)),
        _ => 0.0,
    }
}

/// Excel's rules for defined names: a letter, `_` or `\` first; no spaces or operator
/// characters; not a cell reference (A1 or R1C1); not TRUE/FALSE, R or C.
pub fn is_valid_name(name: &str) -> bool {
    if name.is_empty() || name.chars().count() > 255 {
        return false;
    }
    if !name.chars().next().is_some_and(is_name_start) {
        return false;
    }
    if name
        .chars()
        .any(|c| c.is_whitespace() || "!\"#$%&'()*+,-/:;<=>@[]^`{|}~".contains(c))
    {
        return false;
    }
    let u = name.to_uppercase();
    if matches!(u.as_str(), "TRUE" | "FALSE" | "R" | "C") {
        return false;
    }
    if match_a1_address(name, 0).is_some_and(|(_, end)| end == name.len()) {
        return false;
    }
    if match_r1c1_address(name, 0, (0, 0)).is_some_and(|(_, end)| end == name.len()) {
        return false;
    }
    true
}

impl<H: EngineHost> Engine<H> {
    /// An engine over `host`.
    pub fn with_host(host: H, options: EngineOptions) -> Engine<H> {
        let registry = options
            .registry
            .unwrap_or_else(|| Arc::new(FunctionTable::with_core()));
        let clock = options.clock.unwrap_or_else(|| Arc::new(system_clock));
        let now = clock();
        let mut e = Engine {
            host,
            registry,
            date1904: options.date1904,
            iterative: options.iterative,
            parallel: options.parallel,
            clock,
            now,
            coercion: CoercionOptions::default(),
            sheet_names: FxHashMap::default(),
            sheet_by_name: FxHashMap::default(),
            sheet_order: Vec::new(),
            next_sheet_id: 1,
            cells: Vec::new(),
            free: Vec::new(),
            by_key: FxHashMap::default(),
            graph: Graph::default(),
            names: FxHashMap::default(),
            formula_bounds: FxHashMap::default(),
            interner: FxHashMap::default(),
            dirty: Vec::new(),
            dirty_count: 0,
            scratch_indeg: Vec::new(),
            scratch_pos: Vec::new(),
            volatiles: FxHashSet::default(),
            circular: FxHashSet::default(),
            changed: Vec::new(),
            epoch: 0,
            evaluated: 0,
        };
        e.tick();
        e
    }

    /// Reads the clock (once per recalculation, so every NOW() in it agrees).
    fn tick(&mut self) {
        self.now = (self.clock)();
        self.coercion = CoercionOptions {
            date1904: self.date1904,
            current_year: Some(serial_to_date(self.now, self.date1904).year),
        };
    }

    /// The host.
    pub fn host(&self) -> &H {
        &self.host
    }

    /// The host, mutably. Tell the engine about value changes made directly in the host with
    /// [`Engine::cell_changed`] and about table changes with [`Engine::tables_changed`].
    pub fn host_mut(&mut self) -> &mut H {
        &mut self.host
    }

    /// The function registry.
    pub fn registry(&self) -> &Arc<dyn FunctionRegistry> {
        &self.registry
    }

    /// Configures iterative calculation.
    pub fn set_iterative(&mut self, opts: IterativeOptions) {
        self.iterative = IterativeOptions {
            enabled: opts.enabled,
            max_iterations: opts.max_iterations.max(1),
            max_change: opts.max_change.max(0.0),
        };
    }

    /// Turns parallel evaluation on or off.
    pub fn set_parallel(&mut self, parallel: bool) {
        self.parallel = parallel;
    }

    // ── sheets ──────────────────────────────────────────────────────────────────────────────

    fn resolve_sheet(&self, sheet: &impl SheetRef) -> Result<SheetId, EngineError> {
        match sheet.sheet_key() {
            SheetKey::Id(id) => self
                .sheet_names
                .contains_key(&id)
                .then_some(id)
                .ok_or_else(|| EngineError::UnknownSheet(format!("#{}", id.0))),
            SheetKey::Name(n) => self
                .sheet_by_name
                .get(&n.to_uppercase())
                .copied()
                .ok_or_else(|| EngineError::UnknownSheet(format!("\"{n}\""))),
        }
    }

    /// Adds a sheet at the end and returns its id. Ids are stable across renames and moves.
    pub fn add_sheet(&mut self, name: &str) -> Result<SheetId, EngineError> {
        let id = SheetId(self.next_sheet_id);
        self.add_sheet_with_id(name, id)
    }

    /// Adds a sheet with a chosen id.
    pub fn add_sheet_with_id(&mut self, name: &str, id: SheetId) -> Result<SheetId, EngineError> {
        if self.sheet_by_name.contains_key(&name.to_uppercase()) {
            return Err(EngineError::DuplicateSheet(name.to_owned()));
        }
        if self.sheet_names.contains_key(&id) {
            return Err(EngineError::DuplicateSheetId(id.0));
        }
        self.next_sheet_id = self.next_sheet_id.max(id.0.saturating_add(1));
        self.sheet_names.insert(id, name.to_owned());
        self.sheet_by_name.insert(name.to_uppercase(), id);
        self.sheet_order.push(id);
        self.interner.clear();
        // Formulas and names that referred to this (previously missing) sheet can now bind.
        self.recompile_names();
        let waiting: Vec<FormulaId> = self
            .live_ids()
            .filter(|&id| self.cell(id).is_some_and(|c| c.has(UNRESOLVED)))
            .collect();
        for &f in &waiting {
            self.recompile(f);
            self.reregister(f);
        }
        self.mark_dirty(waiting);
        Ok(id)
    }

    /// Removes a sheet; references to it become `#REF!`.
    pub fn remove_sheet(&mut self, sheet: impl SheetRef) -> Result<(), EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        let name = self.sheet_names.get(&sid).cloned().unwrap_or_default();
        let doomed: Vec<FormulaId> = self
            .live_ids()
            .filter(|&id| self.cell(id).is_some_and(|c| c.sheet == sid))
            .collect();
        for id in doomed {
            self.drop_formula(id);
        }
        self.names.retain(|_, d| d.scope != Some(sid));
        self.sheet_names.remove(&sid);
        self.sheet_by_name.remove(&name.to_uppercase());
        self.sheet_order.retain(|&s| s != sid);
        self.formula_bounds.remove(&sid);
        self.host.remove_sheet(sid);
        self.interner.clear();
        self.rewrite_all(
            |text, _| remove_sheet_in_formula(text, &name).ok(),
            |_| String::new(),
        );
        self.recompile_names();
        self.recompile_all();
        self.rebuild_all(true);
        Ok(())
    }

    /// Renames a sheet, rewriting every formula and name that refers to it.
    pub fn rename_sheet(
        &mut self,
        sheet: impl SheetRef,
        new_name: &str,
    ) -> Result<(), EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        let old = self.sheet_names.get(&sid).cloned().unwrap_or_default();
        if let Some(&existing) = self.sheet_by_name.get(&new_name.to_uppercase())
            && existing != sid
        {
            return Err(EngineError::DuplicateSheet(new_name.to_owned()));
        }
        self.sheet_by_name.remove(&old.to_uppercase());
        self.sheet_by_name.insert(new_name.to_uppercase(), sid);
        self.sheet_names.insert(sid, new_name.to_owned());
        self.interner.clear();
        self.rewrite_all(
            |text, _| rename_sheet_in_formula(text, &old, new_name).ok(),
            |_| String::new(),
        );
        self.recompile_names();
        self.recompile_all();
        self.rebuild_all(true);
        Ok(())
    }

    /// Moves a sheet to a new position in the workbook order (affects 3-D references).
    pub fn move_sheet(&mut self, sheet: impl SheetRef, index: usize) -> Result<(), EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        self.sheet_order.retain(|&s| s != sid);
        let at = index.min(self.sheet_order.len());
        self.sheet_order.insert(at, sid);
        self.rebuild_all(true);
        Ok(())
    }

    /// The id of a sheet name (case-insensitive).
    pub fn sheet_id(&self, name: &str) -> Option<SheetId> {
        self.sheet_by_name.get(&name.to_uppercase()).copied()
    }

    /// The name of a sheet.
    pub fn sheet_name(&self, id: SheetId) -> Option<&str> {
        self.sheet_names.get(&id).map(String::as_str)
    }

    /// Sheet names in workbook order.
    pub fn sheets(&self) -> Vec<String> {
        self.sheet_order
            .iter()
            .filter_map(|id| self.sheet_names.get(id).cloned())
            .collect()
    }

    // ── cells ───────────────────────────────────────────────────────────────────────────────

    fn cell(&self, id: FormulaId) -> Option<&FormulaCell> {
        self.cells.get(id as usize).and_then(Option::as_ref)
    }

    fn cell_mut(&mut self, id: FormulaId) -> Option<&mut FormulaCell> {
        self.cells.get_mut(id as usize).and_then(Option::as_mut)
    }

    fn live_ids(&self) -> impl Iterator<Item = FormulaId> + '_ {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, c)| c.is_some())
            .map(|(i, _)| i as FormulaId)
    }

    fn formula_at(&self, sheet: SheetId, row: u32, col: u32) -> Option<FormulaId> {
        self.by_key.get(&cell_key(sheet, row, col)).copied()
    }

    /// Sets a cell to a formula, a value or nothing. Formulas are parsed immediately (a syntax
    /// error leaves the cell unchanged) and calculated by the next [`Engine::recalc`].
    pub fn set_cell(
        &mut self,
        sheet: impl SheetRef,
        row: u32,
        col: u32,
        input: CellInput,
    ) -> Result<(), EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        check_cell(row, col)?;
        let existing = self.formula_at(sid, row, col);
        match input {
            CellInput::Formula { text, legacy } => {
                let text = with_equals(&text);
                let ast = parse(&text, &ParseOptions::default())?;
                if let Some(id) = existing {
                    self.remove_formula(id);
                }
                // The host may refuse values; a formula cell simply has no literal value.
                let _ = self.host.set_cell_value(sid, row, col, None);
                let (code, code_key) = self.intern(&ast, (row, col));
                let fc = FormulaCell {
                    sheet: sid,
                    row,
                    col,
                    text: text.into_boxed_str(),
                    code,
                    code_key,
                    legacy,
                    value: Value::Blank,
                    spill: Spill::None,
                    claim: None,
                    flags: 0,
                    changed_epoch: 0,
                    hyperlink: None,
                    reg: Registration::default(),
                };
                let id = self.insert_formula(fc);
                self.register(id);
                self.grow_bounds(sid, row, col);
                self.mark_dirty([id]);
            }
            CellInput::Value(v) => {
                self.host
                    .set_cell_value(sid, row, col, (!v.is_blank()).then_some(v))?;
                if let Some(id) = existing {
                    self.remove_formula(id);
                }
                self.cell_changed_id(sid, row, col);
            }
            CellInput::Clear => {
                if let Some(id) = existing {
                    self.remove_formula(id);
                }
                let r = self.host.set_cell_value(sid, row, col, None);
                if existing.is_none() {
                    r?;
                }
                self.cell_changed_id(sid, row, col);
            }
        }
        Ok(())
    }

    /// Sets a literal value.
    pub fn set_value(
        &mut self,
        sheet: impl SheetRef,
        row: u32,
        col: u32,
        value: impl Into<Value>,
    ) -> Result<(), EngineError> {
        self.set_cell(sheet, row, col, CellInput::Value(value.into()))
    }

    /// Sets a formula (dynamic-array semantics).
    pub fn set_formula(
        &mut self,
        sheet: impl SheetRef,
        row: u32,
        col: u32,
        text: &str,
    ) -> Result<(), EngineError> {
        let input = CellInput::Formula {
            text: text.to_owned(),
            legacy: false,
        };
        self.set_cell(sheet, row, col, input)
    }

    /// Empties a cell.
    pub fn clear_cell(
        &mut self,
        sheet: impl SheetRef,
        row: u32,
        col: u32,
    ) -> Result<(), EngineError> {
        self.set_cell(sheet, row, col, CellInput::Clear)
    }

    /// Tells the engine a literal value changed in host storage (when not set through
    /// [`Engine::set_cell`]).
    pub fn cell_changed(
        &mut self,
        sheet: impl SheetRef,
        row: u32,
        col: u32,
    ) -> Result<(), EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        self.cell_changed_id(sid, row, col);
        Ok(())
    }

    fn cell_changed_id(&mut self, sheet: SheetId, row: u32, col: u32) {
        let mut out = Vec::new();
        self.graph.dependents_at(sheet, row, col, &mut out);
        self.mark_dirty(out);
    }

    /// Tells the engine the host's tables changed (structured references are re-resolved).
    pub fn tables_changed(&mut self) {
        let using: Vec<FormulaId> = self
            .live_ids()
            .filter(|&id| self.cell(id).is_some_and(|c| c.has(USES_TABLES)))
            .collect();
        for &id in &using {
            self.reregister(id);
        }
        self.mark_dirty(using);
    }

    /// Formula text of a cell (with `=`).
    pub fn formula(&self, sheet: impl SheetRef, row: u32, col: u32) -> Option<&str> {
        let sid = self.resolve_sheet(&sheet).ok()?;
        let id = self.formula_at(sid, row, col)?;
        self.cell(id).map(|c| &*c.text)
    }

    /// True when the cell holds a formula.
    pub fn has_formula(&self, sheet: impl SheetRef, row: u32, col: u32) -> bool {
        self.formula(sheet, row, col).is_some()
    }

    /// Link target recorded by a `HYPERLINK` the cell's formula evaluated.
    pub fn hyperlink(&self, sheet: impl SheetRef, row: u32, col: u32) -> Option<&str> {
        let sid = self.resolve_sheet(&sheet).ok()?;
        let id = self.formula_at(sid, row, col)?;
        self.cell(id)?.hyperlink.as_deref()
    }

    /// Displayed value of a cell as of the last recalculation: a formula's result (the top-left
    /// value of a spilling anchor, `#SPILL!` when blocked), a value inside a spill range, or the
    /// host's literal value.
    pub fn value(&self, sheet: impl SheetRef, row: u32, col: u32) -> Result<Value, EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        if let Some(id) = self.formula_at(sid, row, col) {
            return Ok(self
                .cell(id)
                .map(FormulaCell::anchor_value)
                .unwrap_or_default());
        }
        if let Some(v) = self.spill_value_at(sid, row, col) {
            return Ok(v);
        }
        Ok(self.host.cell_value(sid, row, col).unwrap_or_default())
    }

    /// The full result of a formula (an array for a dynamic-array formula).
    pub fn result(&self, sheet: impl SheetRef, row: u32, col: u32) -> Option<Value> {
        let sid = self.resolve_sheet(&sheet).ok()?;
        let id = self.formula_at(sid, row, col)?;
        self.cell(id).map(|c| c.value.clone())
    }

    /// The spill range containing a cell (the anchor or any cell it spilled into). A blocked
    /// anchor reports the range it would occupy.
    pub fn spill_range(&self, sheet: impl SheetRef, row: u32, col: u32) -> Option<SpillRange> {
        let sid = self.resolve_sheet(&sheet).ok()?;
        let mut anchor = self
            .formula_at(sid, row, col)
            .filter(|&id| self.cell(id).is_some_and(|c| c.claim.is_some()));
        if anchor.is_none() {
            self.graph.claims_at(sid, row, col, &mut |a, _| {
                if self.cell(a).is_some_and(|c| c.spill == Spill::Ok) {
                    anchor = Some(a);
                }
            });
        }
        let c = self.cell(anchor?)?;
        Some(SpillRange {
            sheet: sid,
            bounds: c.claim?,
            blocked: c.spill == Spill::Blocked,
        })
    }

    /// True when the cell's formula is part of a circular reference.
    pub fn is_circular(&self, sheet: impl SheetRef, row: u32, col: u32) -> bool {
        let Ok(sid) = self.resolve_sheet(&sheet) else {
            return false;
        };
        self.formula_at(sid, row, col)
            .and_then(|id| self.cell(id))
            .is_some_and(|c| c.has(CIRCULAR))
    }

    /// Every cell in a circular reference, in sheet/row/column order.
    pub fn circular_cells(&self) -> Vec<CellRef> {
        let mut out: Vec<CellRef> = self
            .circular
            .iter()
            .filter_map(|&id| self.cell(id))
            .map(|c| CellRef {
                sheet: c.sheet,
                row: c.row,
                col: c.col,
            })
            .collect();
        out.sort_by_key(|c| (self.sheet_position(c.sheet), c.row, c.col));
        out
    }

    fn sheet_position(&self, s: SheetId) -> usize {
        self.sheet_order
            .iter()
            .position(|&x| x == s)
            .unwrap_or(usize::MAX)
    }

    /// Number of formula cells.
    pub fn formula_count(&self) -> usize {
        self.by_key.len()
    }

    /// Graph statistics.
    pub fn graph_stats(&self) -> GraphStats {
        GraphStats {
            formulas: self.by_key.len(),
            range_nodes: self.graph.range_node_count(),
            cell_edges: self.graph.cell_edge_count(),
            dirty: self.dirty_count,
            compiled: self.interner.len(),
        }
    }

    // ── names ───────────────────────────────────────────────────────────────────────────────

    /// Defines (or redefines) a name; `scope` makes it local to a sheet.
    pub fn define_name(
        &mut self,
        name: &str,
        formula: &str,
        scope: Option<SheetId>,
    ) -> Result<(), EngineError> {
        if !is_valid_name(name) {
            return Err(EngineError::InvalidName(name.to_owned()));
        }
        if let Some(s) = scope {
            self.resolve_sheet(&s)?;
        }
        let text = with_equals(formula);
        let ast = parse(&text, &ParseOptions::default())?;
        let code = Arc::new(compile(&ast, (0, 0), self));
        self.names.insert(
            name_key(name, scope),
            NameDef {
                name: name.to_owned(),
                text,
                code,
                scope,
            },
        );
        self.name_changed(name);
        Ok(())
    }

    /// Removes a defined name.
    pub fn remove_name(&mut self, name: &str, scope: Option<SheetId>) -> bool {
        let removed = self.names.remove(&name_key(name, scope)).is_some();
        if removed {
            self.name_changed(name);
        }
        removed
    }

    /// Defined names with their formulas, sorted by name.
    pub fn names(&self) -> Vec<NameInfo> {
        let mut out: Vec<NameInfo> = self
            .names
            .values()
            .map(|d| NameInfo {
                name: d.name.clone(),
                formula: d.text.clone(),
                scope: d.scope,
            })
            .collect();
        out.sort_by_key(|a| a.name.to_uppercase());
        out
    }

    fn name_changed(&mut self, name: &str) {
        let users = self.graph.formulas_using_name(&name.to_uppercase());
        for &id in &users {
            self.reregister(id);
        }
        self.mark_dirty(users);
    }

    fn recompile_names(&mut self) {
        let keys: Vec<(Option<SheetId>, String)> = self.names.keys().cloned().collect();
        for k in keys {
            let Some(text) = self.names.get(&k).map(|d| d.text.clone()) else {
                continue;
            };
            if let Ok(ast) = parse(&text, &ParseOptions::default()) {
                let code = Arc::new(compile(&ast, (0, 0), self));
                if let Some(d) = self.names.get_mut(&k) {
                    d.code = code;
                }
            }
        }
    }

    // ── structural edits ───────────────────────────────────────────────────────────────────

    /// Inserts `count` rows before row `at`.
    pub fn insert_rows(
        &mut self,
        sheet: impl SheetRef,
        at: u32,
        count: u32,
    ) -> Result<(), EngineError> {
        self.structural(sheet, StructuralKind::InsertRows, at, count)
    }

    /// Deletes `count` rows starting at row `at`.
    pub fn delete_rows(
        &mut self,
        sheet: impl SheetRef,
        at: u32,
        count: u32,
    ) -> Result<(), EngineError> {
        self.structural(sheet, StructuralKind::DeleteRows, at, count)
    }

    /// Inserts `count` columns before column `at`.
    pub fn insert_cols(
        &mut self,
        sheet: impl SheetRef,
        at: u32,
        count: u32,
    ) -> Result<(), EngineError> {
        self.structural(sheet, StructuralKind::InsertCols, at, count)
    }

    /// Deletes `count` columns starting at column `at`.
    pub fn delete_cols(
        &mut self,
        sheet: impl SheetRef,
        at: u32,
        count: u32,
    ) -> Result<(), EngineError> {
        self.structural(sheet, StructuralKind::DeleteCols, at, count)
    }

    fn structural(
        &mut self,
        sheet: impl SheetRef,
        kind: StructuralKind,
        at: u32,
        count: u32,
    ) -> Result<(), EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        let max = if kind.is_rows() { MAX_ROWS } else { MAX_COLS };
        if count == 0 || at >= max {
            return Ok(());
        }
        let edit = StructuralEdit { kind, at, count };
        let sheet_name = self.sheet_names.get(&sid).cloned().unwrap_or_default();
        self.rewrite_all(
            |text, fsheet| shift_formula(text, &edit, &sheet_name, fsheet).ok(),
            |scope| scope.unwrap_or_default(),
        );
        let rows = kind.is_rows();
        let insert = kind.is_insert();
        let ids: Vec<FormulaId> = self.live_ids().collect();
        self.graph.clear();
        self.by_key.clear();
        let mut doomed = Vec::new();
        for id in ids {
            let Some(fc) = self.cell_mut(id) else {
                continue;
            };
            if fc.sheet == sid {
                let idx = u64::from(if rows { fc.row } else { fc.col });
                let (at64, n64) = (u64::from(at), u64::from(count));
                if idx >= at64 {
                    if !insert && idx < at64 + n64 {
                        doomed.push(id);
                        continue;
                    }
                    let n = if insert { idx + n64 } else { idx - n64 };
                    if n >= u64::from(max) {
                        doomed.push(id);
                        continue;
                    }
                    if rows {
                        fc.row = n as u32;
                    } else {
                        fc.col = n as u32;
                    }
                }
            }
            if fc.spill != Spill::None {
                fc.spill = Spill::Blocked;
            }
            fc.claim = None;
            fc.reg = Registration::default();
            let key = cell_key(fc.sheet, fc.row, fc.col);
            self.by_key.insert(key, id);
        }
        for id in doomed {
            self.free_slot(id);
        }
        self.host.apply_structural_edit(sid, &edit);
        self.recompute_bounds();
        self.recompile_all();
        self.rebuild_all(false);
        Ok(())
    }

    /// Moves (cut and paste) a block of cells to `(to_row, to_col)` on `to_sheet` (or the same
    /// sheet). Formulas anywhere that refer into the block follow it; references to the cells it
    /// lands on become `#REF!`.
    pub fn move_range(
        &mut self,
        sheet: impl SheetRef,
        from: Bounds,
        to_sheet: Option<SheetId>,
        to_row: u32,
        to_col: u32,
    ) -> Result<(), EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        let tid = match to_sheet {
            Some(t) => self.resolve_sheet(&t)?,
            None => sid,
        };
        check_cell(from.r1, from.c1)?;
        check_cell(to_row, to_col)?;
        let d_row = i64::from(to_row) - i64::from(from.r0);
        let d_col = i64::from(to_col) - i64::from(from.c0);
        let last_row = i64::from(from.r1) + d_row;
        let last_col = i64::from(from.c1) + d_col;
        if last_row >= i64::from(MAX_ROWS) || last_col >= i64::from(MAX_COLS) {
            return Err(EngineError::OutOfRange {
                row: last_row.max(0) as u64,
                col: last_col.max(0) as u64,
            });
        }
        let dest = Bounds {
            r0: to_row,
            c0: to_col,
            r1: last_row as u32,
            c1: last_col as u32,
        };
        let spec = MoveSpec {
            sheet: self.sheet_names.get(&sid).cloned().unwrap_or_default(),
            from,
            d_row,
            d_col,
            to_sheet: self.sheet_names.get(&tid).cloned(),
        };
        self.rewrite_all(
            |text, fsheet| move_formula(text, &spec, fsheet).ok(),
            |scope| scope.unwrap_or_default(),
        );
        // Literal values move with the block.
        let mut vals = Vec::new();
        for r in from.r0..=from.r1 {
            for c in from.c0..=from.c1 {
                if let Some(v) = self.host.cell_value(sid, r, c) {
                    vals.push((r, c, v));
                    let _ = self.host.set_cell_value(sid, r, c, None);
                }
            }
        }
        for r in dest.r0..=dest.r1 {
            for c in dest.c0..=dest.c1 {
                let _ = self.host.set_cell_value(tid, r, c, None);
            }
        }
        for (r, c, v) in vals {
            let nr = (i64::from(r) + d_row) as u32;
            let nc = (i64::from(c) + d_col) as u32;
            let _ = self.host.set_cell_value(tid, nr, nc, Some(v));
        }
        let ids: Vec<FormulaId> = self.live_ids().collect();
        self.graph.clear();
        self.by_key.clear();
        let mut moved = Vec::new();
        let mut doomed = Vec::new();
        for id in ids {
            let Some(fc) = self.cell(id) else { continue };
            let in_src = fc.sheet == sid && from.contains(fc.row, fc.col);
            let in_dest = fc.sheet == tid && dest.contains(fc.row, fc.col);
            if in_src {
                moved.push(id);
            } else if in_dest {
                doomed.push(id);
            } else {
                self.by_key.insert(cell_key(fc.sheet, fc.row, fc.col), id);
            }
        }
        for id in doomed {
            self.free_slot(id);
        }
        for id in moved {
            let Some(fc) = self.cell_mut(id) else {
                continue;
            };
            fc.sheet = tid;
            fc.row = (i64::from(fc.row) + d_row) as u32;
            fc.col = (i64::from(fc.col) + d_col) as u32;
            let key = cell_key(fc.sheet, fc.row, fc.col);
            self.by_key.insert(key, id);
        }
        let live: Vec<FormulaId> = self.live_ids().collect();
        for id in live {
            if let Some(fc) = self.cell_mut(id) {
                if fc.spill != Spill::None {
                    fc.spill = Spill::Blocked;
                }
                fc.claim = None;
                fc.reg = Registration::default();
            }
        }
        self.recompute_bounds();
        self.recompile_all();
        self.rebuild_all(false);
        Ok(())
    }

    // ── recalculation ───────────────────────────────────────────────────────────────────────

    /// Recalculates everything that changed since the last recalculation, plus volatile
    /// formulas and their dependents.
    pub fn recalc(&mut self) -> RecalcResult {
        self.changed.clear();
        self.evaluated = 0;
        self.epoch = self.epoch.wrapping_add(1);
        self.tick();
        if !self.volatiles.is_empty() {
            let v: Vec<FormulaId> = self.volatiles.iter().copied().collect();
            self.mark_dirty(v);
        }
        let mut passes = 0;
        while self.dirty_count > 0 && passes < MAX_PASSES {
            passes += 1;
            let mut nodes = std::mem::take(&mut self.dirty);
            nodes.retain(|&id| self.cell(id).is_some_and(|c| c.has(DIRTY)));
            nodes.sort_unstable();
            nodes.dedup();
            if nodes.is_empty() {
                self.dirty_count = 0;
                break;
            }
            self.run_pass(&nodes);
        }
        let changed = std::mem::take(&mut self.changed);
        RecalcResult {
            evaluated: self.evaluated,
            changed: changed
                .iter()
                .filter_map(|&id| self.cell(id))
                .map(|c| CellRef {
                    sheet: c.sheet,
                    row: c.row,
                    col: c.col,
                })
                .collect(),
            circular: self.circular_cells(),
        }
    }

    /// Recalculates every formula.
    pub fn recalc_all(&mut self) -> RecalcResult {
        // Everything becomes dirty, so there is no need to walk dependents.
        let all: Vec<FormulaId> = self.live_ids().collect();
        for &id in &all {
            if let Some(c) = self.cell_mut(id)
                && !c.has(DIRTY)
            {
                c.set(DIRTY, true);
                self.dirty.push(id);
                self.dirty_count += 1;
            }
        }
        self.recalc()
    }

    fn run_pass(&mut self, nodes: &[FormulaId]) {
        // Scratch arrays indexed by formula id are kept between passes (all zero / unset), so an
        // incremental recalculation costs what it touches, not the size of the workbook.
        let n_slots = self.cells.len();
        let mut indeg = std::mem::take(&mut self.scratch_indeg);
        let mut pos = std::mem::take(&mut self.scratch_pos);
        indeg.resize(n_slots, 0);
        pos.resize(n_slots, u32::MAX);
        self.run_pass_with(nodes, &mut indeg, &mut pos);
        for &n in nodes {
            indeg[n as usize] = 0;
            pos[n as usize] = u32::MAX;
        }
        self.scratch_indeg = indeg;
        self.scratch_pos = pos;
    }

    fn run_pass_with(&mut self, nodes: &[FormulaId], indeg: &mut [u32], pos: &mut [u32]) {
        for (i, &n) in nodes.iter().enumerate() {
            pos[n as usize] = i as u32;
        }
        let (offsets, edges) = self.pass_edges(nodes, pos);
        for &d in &edges {
            indeg[d as usize] += 1;
        }
        let out = |n: FormulaId| -> &[FormulaId] {
            let i = pos[n as usize] as usize;
            &edges[offsets[i]..offsets[i + 1]]
        };
        let mut level: Vec<FormulaId> = nodes
            .iter()
            .copied()
            .filter(|&n| indeg[n as usize] == 0)
            .collect();
        let mut done = 0;
        while !level.is_empty() {
            self.evaluate_level(&level);
            done += level.len();
            let mut next = Vec::new();
            for &n in &level {
                for &d in out(n) {
                    indeg[d as usize] -= 1;
                    if indeg[d as usize] == 0 {
                        next.push(d);
                    }
                }
            }
            level = next;
        }
        if done < nodes.len() {
            let left: Vec<FormulaId> = nodes
                .iter()
                .copied()
                .filter(|&n| indeg[n as usize] > 0)
                .collect();
            let comps = tarjan(&left, out);
            let self_loop: Vec<bool> = comps
                .iter()
                .map(|c| c.len() == 1 && out(c[0]).contains(&c[0]))
                .collect();
            for (comp, looped) in comps.into_iter().zip(self_loop) {
                if comp.len() == 1 && !looped {
                    if self.cell(comp[0]).is_some_and(|c| c.has(DIRTY)) {
                        self.evaluate_serial(comp[0]);
                    }
                } else {
                    self.resolve_cycle(comp);
                }
            }
        }
    }

    /// The dirty dependents of every node of a pass, as a CSR adjacency (`offsets`, `edges`).
    fn pass_edges(&self, nodes: &[FormulaId], pos: &[u32]) -> (Vec<usize>, Vec<FormulaId>) {
        let in_pass = |d: FormulaId| {
            pos.get(d as usize).is_some_and(|&p| p != u32::MAX)
                && self.cell(d).is_some_and(|c| c.has(DIRTY))
        };
        let deps = |n: FormulaId, buf: &mut Vec<FormulaId>, out: &mut Vec<FormulaId>| {
            buf.clear();
            if let Some(fc) = self.cell(n) {
                self.graph
                    .dependents_of(fc.sheet, fc.row, fc.col, fc.claim.as_ref(), buf);
            }
            out.extend(buf.iter().copied().filter(|&d| in_pass(d)));
        };
        let mut offsets: Vec<usize> = Vec::with_capacity(nodes.len() + 1);
        offsets.push(0);
        #[cfg(feature = "parallel")]
        if self.parallel && nodes.len() >= PARALLEL_EDGES_THRESHOLD {
            use rayon::prelude::*;
            let chunks: Vec<(Vec<u32>, Vec<FormulaId>)> = nodes
                .par_chunks(4096)
                .map(|chunk| {
                    let mut buf = Vec::new();
                    let mut lens = Vec::with_capacity(chunk.len());
                    let mut out = Vec::with_capacity(chunk.len());
                    for &n in chunk {
                        let before = out.len();
                        deps(n, &mut buf, &mut out);
                        lens.push((out.len() - before) as u32);
                    }
                    (lens, out)
                })
                .collect();
            let total = chunks.iter().map(|c| c.1.len()).sum();
            let mut edges = Vec::with_capacity(total);
            for (lens, out) in chunks {
                for l in lens {
                    let last = offsets.last().copied().unwrap_or(0);
                    offsets.push(last + l as usize);
                }
                edges.extend(out);
            }
            return (offsets, edges);
        }
        let mut edges: Vec<FormulaId> = Vec::with_capacity(nodes.len());
        let mut buf: Vec<FormulaId> = Vec::new();
        for &n in nodes {
            deps(n, &mut buf, &mut edges);
            offsets.push(edges.len());
        }
        (offsets, edges)
    }

    fn evaluate_level(&mut self, level: &[FormulaId]) {
        let todo: Vec<FormulaId> = level
            .iter()
            .copied()
            .filter(|&id| self.cell(id).is_some_and(|c| c.has(DIRTY)))
            .collect();
        #[cfg(feature = "parallel")]
        if self.parallel && todo.len() >= PARALLEL_THRESHOLD {
            self.evaluate_batch(&todo);
            return;
        }
        for id in todo {
            if self.cell(id).is_some_and(|c| c.has(DIRTY)) {
                self.evaluate_serial(id);
            }
        }
    }

    #[cfg(feature = "parallel")]
    fn evaluate_batch(&mut self, todo: &[FormulaId]) {
        use rayon::prelude::*;
        for &id in todo {
            if let Some(c) = self.cell_mut(id) {
                c.set(IN_BATCH, true);
            }
        }
        let this: &Engine<H> = self;
        let results: Vec<Option<(Value, Option<String>)>> = todo
            .par_iter()
            .with_min_len(32)
            .map(|&id| this.evaluate_snapshot(id))
            .collect();
        for (&id, result) in todo.iter().zip(results) {
            let Some(c) = self.cell_mut(id) else { continue };
            let stale = c.has(BATCH_STALE);
            c.set(IN_BATCH | BATCH_STALE, false);
            if !c.has(DIRTY) {
                continue;
            }
            match result {
                Some((value, hyperlink)) if !stale => self.finish_evaluation(id, value, hyperlink),
                _ => self.evaluate_serial(id),
            }
        }
    }

    /// Evaluates against the current state without changing it; `None` when the formula read a
    /// cell that is still pending (it is then evaluated serially).
    #[cfg(feature = "parallel")]
    fn evaluate_snapshot(&self, id: FormulaId) -> Option<(Value, Option<String>)> {
        let fc = self.cell(id)?;
        // Borrow the shared compiled formula: cloning the `Arc` from every thread would make its
        // reference count a contended cache line.
        let code: &Expr = &fc.code;
        let mut src = Snapshot {
            eng: self,
            deferred: false,
        };
        let (value, hyperlink) = {
            let mut ev = Evaluator::new(&mut src, fc.sheet, fc.row, fc.col, fc.legacy);
            let v = ev.evaluate_formula(code);
            (v, ev.hyperlink.take())
        };
        (!src.deferred).then_some((value, hyperlink))
    }

    fn evaluate_serial(&mut self, id: FormulaId) {
        let Some(fc) = self.cell_mut(id) else { return };
        fc.set(EVALUATING, true);
        let (sheet, row, col, legacy, code) =
            (fc.sheet, fc.row, fc.col, fc.legacy, fc.code.clone());
        self.set_circular(id, false);
        let (value, hyperlink) = {
            let mut src = Serial { eng: self };
            let mut ev = Evaluator::new(&mut src, sheet, row, col, legacy);
            let v = ev.evaluate_formula(&code);
            (v, ev.hyperlink.take())
        };
        if let Some(fc) = self.cell_mut(id) {
            fc.set(EVALUATING, false);
        }
        self.finish_evaluation(id, value, hyperlink);
    }

    fn finish_evaluation(&mut self, id: FormulaId, value: Value, hyperlink: Option<String>) {
        let Some(fc) = self.cell_mut(id) else { return };
        if fc.has(DIRTY) {
            fc.set(DIRTY, false);
            self.dirty_count = self.dirty_count.saturating_sub(1);
        }
        if let Some(fc) = self.cell_mut(id) {
            fc.hyperlink = hyperlink.map(String::into_boxed_str);
        }
        self.evaluated += 1;
        self.apply_result(id, value);
    }

    fn set_circular(&mut self, id: FormulaId, on: bool) {
        if let Some(fc) = self.cell_mut(id) {
            fc.set(CIRCULAR, on);
            if on {
                self.circular.insert(id);
            } else {
                self.circular.remove(&id);
            }
        }
    }

    fn resolve_cycle(&mut self, mut members: Vec<FormulaId>) {
        members.sort_by_key(|&id| {
            self.cell(id)
                .map(|c| (self.sheet_position(c.sheet), c.row, c.col))
                .unwrap_or((usize::MAX, 0, 0))
        });
        for &m in &members {
            if let Some(fc) = self.cell_mut(m)
                && fc.has(DIRTY)
            {
                fc.set(DIRTY, false);
                self.dirty_count = self.dirty_count.saturating_sub(1);
            }
        }
        if !self.iterative.enabled {
            // Excel shows 0 in the cells of a circular reference and flags them.
            for &m in &members {
                self.apply_result(m, Value::Number(0.0));
                self.set_circular(m, true);
            }
            return;
        }
        for _ in 0..self.iterative.max_iterations {
            let mut max_delta: f64 = 0.0;
            for &m in &members {
                let before = self.cell(m).map_or(0.0, |c| numeric_of(&c.value));
                if let Some(fc) = self.cell_mut(m)
                    && !fc.has(DIRTY)
                {
                    fc.set(DIRTY, true);
                    self.dirty_count += 1;
                }
                self.evaluate_serial(m);
                let after = self.cell(m).map_or(0.0, |c| numeric_of(&c.value));
                let delta = (after - before).abs();
                max_delta = if delta.is_nan() {
                    f64::INFINITY
                } else {
                    max_delta.max(delta)
                };
            }
            if max_delta <= self.iterative.max_change {
                break;
            }
        }
        for &m in &members {
            self.set_circular(m, true);
        }
    }

    fn apply_result(&mut self, id: FormulaId, value: Value) {
        let Some(fc) = self.cell(id) else { return };
        let (sheet, row, col) = (fc.sheet, fc.row, fc.col);
        let old_claim = fc.claim;
        let old_spill = fc.spill;
        let mut new_claim = None;
        let mut spill = Spill::None;
        if let Value::Array(a) = &value
            && a.len() > 1
        {
            let rows = a.rows() as u64;
            let cols = a.cols() as u64;
            let rect = Bounds {
                r0: row,
                c0: col,
                r1: (u64::from(row) + rows - 1).min(u64::from(MAX_ROWS - 1)) as u32,
                c1: (u64::from(col) + cols - 1).min(u64::from(MAX_COLS - 1)) as u32,
            };
            let out_of_sheet = u64::from(row) + rows > u64::from(MAX_ROWS)
                || u64::from(col) + cols > u64::from(MAX_COLS);
            new_claim = Some(rect);
            if old_claim != new_claim {
                self.graph.set_claim(sheet, id, old_claim, new_claim);
            }
            spill = if out_of_sheet || self.spill_blocked(id, sheet, &rect) {
                Spill::Blocked
            } else {
                Spill::Ok
            };
            if spill == Spill::Ok {
                self.grow_bounds(sheet, rect.r1, rect.c1);
            }
        } else if old_claim.is_some() {
            self.graph.set_claim(sheet, id, old_claim, None);
        }
        let epoch = self.epoch;
        let Some(fc) = self.cell_mut(id) else { return };
        let value_changed = old_spill != spill || fc.value != value;
        fc.value = value;
        fc.spill = spill;
        fc.claim = new_claim;
        if value_changed && fc.changed_epoch != epoch {
            fc.changed_epoch = epoch;
            self.changed.push(id);
        }
        if old_claim.is_none() && new_claim.is_none() {
            return;
        }
        let rect_changed = old_claim != new_claim || old_spill != spill;
        if !value_changed && !rect_changed {
            return;
        }
        let mut affected = Vec::new();
        for r in [old_claim, new_claim].into_iter().flatten() {
            self.graph.dependents_in_rect(sheet, &r, &mut affected);
            if rect_changed {
                self.graph
                    .claims_overlapping(sheet, &r, &mut |a, _| affected.push(a));
            }
        }
        let mut to_mark = Vec::new();
        for a in affected {
            if a == id {
                continue;
            }
            let Some(c) = self.cell_mut(a) else { continue };
            if c.has(EVALUATING) {
                continue;
            }
            if c.has(DIRTY) {
                if c.has(IN_BATCH) {
                    c.set(BATCH_STALE, true);
                }
            } else {
                to_mark.push(a);
            }
        }
        self.mark_dirty(to_mark);
    }

    fn spill_blocked(&self, id: FormulaId, sheet: SheetId, rect: &Bounds) -> bool {
        for r in rect.r0..=rect.r1 {
            for c in rect.c0..=rect.c1 {
                if r == rect.r0 && c == rect.c0 {
                    continue;
                }
                if self.formula_at(sheet, r, c).is_some() {
                    return true;
                }
                if self
                    .host
                    .cell_value(sheet, r, c)
                    .is_some_and(|v| !v.is_blank())
                {
                    return true;
                }
            }
        }
        let mut blocked = false;
        self.graph.claims_overlapping(sheet, rect, &mut |a, _| {
            if a != id && self.cell(a).is_some_and(|c| c.spill == Spill::Ok) {
                blocked = true;
            }
        });
        blocked
    }

    fn mark_dirty(&mut self, start: impl IntoIterator<Item = FormulaId>) {
        let mut stack: Vec<FormulaId> = Vec::new();
        for s in start {
            if let Some(c) = self.cell_mut(s)
                && !c.has(DIRTY)
            {
                c.set(DIRTY, true);
                self.dirty.push(s);
                self.dirty_count += 1;
                stack.push(s);
            }
        }
        let mut buf = Vec::new();
        while let Some(f) = stack.pop() {
            buf.clear();
            if let Some(fc) = self.cell(f) {
                self.graph
                    .dependents_of(fc.sheet, fc.row, fc.col, fc.claim.as_ref(), &mut buf);
            }
            for &d in &buf {
                if let Some(c) = self.cell_mut(d)
                    && !c.has(DIRTY)
                {
                    c.set(DIRTY, true);
                    self.dirty.push(d);
                    self.dirty_count += 1;
                    stack.push(d);
                }
            }
        }
    }

    fn spill_value_at(&self, sheet: SheetId, row: u32, col: u32) -> Option<Value> {
        let mut out = None;
        self.graph.claims_at(sheet, row, col, &mut |a, rect| {
            if let Some(fc) = self.cell(a)
                && fc.spill == Spill::Ok
                && let Value::Array(arr) = &fc.value
            {
                out = Some(
                    arr.get((row - rect.r0) as usize, (col - rect.c0) as usize)
                        .clone(),
                );
            }
        });
        out
    }

    /// The spill area of the formula anchored at a cell (shared by both evaluation modes).
    fn spill_area_of(
        &self,
        id: FormulaId,
        sheet: SheetId,
        row: u32,
        col: u32,
    ) -> Result<Bounds, ErrorValue> {
        let fc = self.cell(id).ok_or(ErrorValue::Ref)?;
        match (fc.spill, fc.claim, &fc.value) {
            (Spill::Ok, Some(c), _) => Ok(c),
            (Spill::Blocked, _, _) => Err(ErrorValue::Spill),
            (_, _, Value::Error(e)) => Err(*e),
            _ => {
                let _ = sheet;
                Ok(Bounds {
                    r0: row,
                    c0: col,
                    r1: row,
                    c1: col,
                })
            }
        }
    }

    // ── ad-hoc evaluation ───────────────────────────────────────────────────────────────────

    /// Evaluates a formula without storing it, as if it were in the first sheet's A1.
    pub fn evaluate(&mut self, formula: &str) -> Result<Value, EngineError> {
        let sheet = match self.sheet_order.first() {
            Some(&s) => s,
            None => self.add_sheet("Sheet1")?,
        };
        self.evaluate_at(formula, sheet, 0, 0, false)
    }

    /// Evaluates a formula without storing it, as if it were in `(row, col)` of `sheet`.
    pub fn evaluate_at(
        &mut self,
        formula: &str,
        sheet: impl SheetRef,
        row: u32,
        col: u32,
        legacy: bool,
    ) -> Result<Value, EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        check_cell(row, col)?;
        let ast = parse(formula, &ParseOptions::default())?;
        let code = compile(&ast, (row, col), self);
        let mut src = Serial { eng: self };
        let mut ev = Evaluator::new(&mut src, sid, row, col, legacy);
        Ok(ev.evaluate_formula(&code))
    }

    /// Evaluates a formula step by step for the Formula Lab, as if it were in the first sheet's
    /// A1: every sub-expression with its source text, span and value; branches that were not
    /// evaluated (the untaken arm of `IF`) are marked `evaluated: false`. Nothing is stored.
    ///
    /// ```
    /// use kiln_formula::engine::Engine;
    ///
    /// let mut e = Engine::new();
    /// let step = e.explain("=IF(2>1,\"x\",1/0)").expect("valid formula");
    /// assert_eq!(step.display, "\"x\"");
    /// assert_eq!(step.children[0].display, "TRUE");
    /// assert!(!step.children[2].evaluated);
    /// ```
    pub fn explain(&mut self, formula: &str) -> Result<ExplainStep, EngineError> {
        let sheet = match self.sheet_order.first() {
            Some(&s) => s,
            None => self.add_sheet("Sheet1")?,
        };
        self.explain_at(formula, sheet, 0, 0)
    }

    /// [`explain`](Self::explain) as if the formula were in `(row, col)` of `sheet`.
    pub fn explain_at(
        &mut self,
        formula: &str,
        sheet: impl SheetRef,
        row: u32,
        col: u32,
    ) -> Result<ExplainStep, EngineError> {
        let sid = self.resolve_sheet(&sheet)?;
        check_cell(row, col)?;
        let ast = parse(formula, &ParseOptions::default())?;
        let code = compile(&ast, (row, col), self);
        let mut src = Serial { eng: self };
        let mut ev = Evaluator::new(&mut src, sid, row, col, false);
        ev.enable_trace();
        let result = ev.evaluate_formula(&code);
        Ok(build_explain_tree(formula, &ast, &code, &result, &mut ev))
    }

    // ── internals ───────────────────────────────────────────────────────────────────────────

    fn insert_formula(&mut self, fc: FormulaCell) -> FormulaId {
        let key = cell_key(fc.sheet, fc.row, fc.col);
        let id = match self.free.pop() {
            Some(id) => {
                self.cells[id as usize] = Some(fc);
                id
            }
            None => {
                self.cells.push(Some(fc));
                (self.cells.len() - 1) as FormulaId
            }
        };
        self.by_key.insert(key, id);
        id
    }

    /// Interns the compiled form of an AST anchored at `anchor`.
    fn intern(&mut self, ast: &crate::parser::Node, anchor: (u32, u32)) -> (Arc<Expr>, Arc<str>) {
        let key = print(
            ast,
            &PrintOptions {
                style: RefStyle::R1C1,
                anchor,
                leading_equals: false,
            },
        );
        if let Some((k, code)) = self.interner.get_key_value(key.as_str()) {
            return (code.clone(), k.clone());
        }
        let code = Arc::new(compile(ast, anchor, self));
        let key: Arc<str> = Arc::from(key);
        self.interner.insert(key.clone(), code.clone());
        (code, key)
    }

    fn release_code(&mut self, code: &Arc<Expr>, key: &Arc<str>) {
        // Held by the interner and by the caller only: no other formula shares it.
        if Arc::strong_count(code) == 2
            && self.interner.get(key).is_some_and(|c| Arc::ptr_eq(c, code))
        {
            self.interner.remove(key);
        }
    }

    /// Re-parses and re-compiles one formula from its text at its current position.
    fn recompile(&mut self, id: FormulaId) {
        let Some(fc) = self.cell(id) else { return };
        let (text, anchor) = (fc.text.to_string(), (fc.row, fc.col));
        let Ok(ast) = parse(&text, &ParseOptions::default()) else {
            return;
        };
        let (code, key) = self.intern(&ast, anchor);
        let old = self.cell_mut(id).map(|fc| {
            let old_code = std::mem::replace(&mut fc.code, code);
            let old_key = std::mem::replace(&mut fc.code_key, key);
            (old_code, old_key)
        });
        if let Some((c, k)) = old {
            self.release_code(&c, &k);
        }
    }

    fn recompile_all(&mut self) {
        self.interner.clear();
        let ids: Vec<FormulaId> = self.live_ids().collect();
        for id in ids {
            self.recompile(id);
        }
    }

    fn collect_precedents(&self, code: &Expr, sheet: SheetId, row: u32, col: u32) -> Precedents {
        let mut p = Precedents::default();
        let mut seen: FxHashSet<(SheetId, String)> = FxHashSet::default();
        self.visit_precedents(
            code,
            sheet,
            (row, col),
            sheet,
            row,
            col,
            0,
            &mut p,
            &mut seen,
        );
        p
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_precedents(
        &self,
        e: &Expr,
        ctx_sheet: SheetId,
        anchor: (u32, u32),
        cell_sheet: SheetId,
        row: u32,
        col: u32,
        depth: usize,
        p: &mut Precedents,
        seen: &mut FxHashSet<(SheetId, String)>,
    ) {
        let mut stack: Vec<&Expr> = vec![e];
        while let Some(e) = stack.pop() {
            match e {
                Expr::Ref(sel, addr) => {
                    let sheets: Vec<SheetId> = match sel {
                        SheetSel::Current => vec![ctx_sheet],
                        SheetSel::Sheet(s) => vec![*s],
                        SheetSel::Span(a, b) => match self.sheets_between(*a, *b) {
                            Some(v) => v,
                            None => {
                                p.unresolved_sheets = true;
                                continue;
                            }
                        },
                        SheetSel::Unresolved => {
                            p.unresolved_sheets = true;
                            continue;
                        }
                        SheetSel::External => continue,
                    };
                    let Some(b) = addr.resolve(anchor) else {
                        continue;
                    };
                    for s in sheets {
                        if addr.is_cell() {
                            p.cells.push(cell_key(s, b.r0, b.c0));
                        } else {
                            p.ranges.push(Area {
                                sheet: s,
                                bounds: b,
                            });
                        }
                    }
                }
                Expr::Name { name, scope } => {
                    self.precedent_name(
                        name, scope, ctx_sheet, cell_sheet, row, col, depth, p, seen,
                    );
                }
                Expr::Structured(s) => {
                    let t = match &s.table {
                        Some(n) => self.host.table(n),
                        None => self.host.table_at(cell_sheet, row, col),
                    };
                    p.uses_tables = true;
                    if let Some(t) = t {
                        self.add_table_precedent(&t, s, (cell_sheet, row), p);
                    }
                }
                Expr::Call(f, args) => {
                    match f {
                        FuncRef::Known(id) => {
                            if self.registry.spec(*id).is_some_and(|s| s.volatile) {
                                p.volatile = true;
                            }
                        }
                        FuncRef::Unknown(name) => {
                            self.precedent_name(
                                name,
                                &NameScope::Default,
                                ctx_sheet,
                                cell_sheet,
                                row,
                                col,
                                depth,
                                p,
                                seen,
                            );
                        }
                    }
                    stack.extend(args.iter().map(|a| &**a));
                }
                Expr::Invoke(callee, args) => {
                    stack.push(callee);
                    stack.extend(args.iter().map(|a| &**a));
                }
                Expr::Unary(_, x) | Expr::Postfix(_, x) => stack.push(x),
                Expr::Binary(_, l, r) => {
                    stack.push(l);
                    stack.push(r);
                }
                _ => {}
            }
        }
    }

    fn add_table_precedent(
        &self,
        t: &TableDef,
        s: &crate::parser::StructuredRef,
        at: (SheetId, u32),
        p: &mut Precedents,
    ) {
        if let Ok(a) = resolve_structured(t, &s.specials, s.this_row, s.columns.as_ref(), at) {
            if a.is_cell() {
                p.cells.push(cell_key(a.sheet, a.bounds.r0, a.bounds.c0));
            } else {
                p.ranges.push(a);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn precedent_name(
        &self,
        name: &str,
        scope: &NameScope,
        ctx_sheet: SheetId,
        cell_sheet: SheetId,
        row: u32,
        col: u32,
        depth: usize,
        p: &mut Precedents,
        seen: &mut FxHashSet<(SheetId, String)>,
    ) {
        let upper = name.to_uppercase();
        p.names.push(upper.clone());
        let scope_sheet = match scope {
            NameScope::Default => ctx_sheet,
            NameScope::Sheet(s) => *s,
            NameScope::Unresolved => {
                p.unresolved_sheets = true;
                return;
            }
        };
        if !seen.insert((scope_sheet, upper)) {
            return;
        }
        if let Some(def) = self.name_def(name, scope_sheet) {
            if depth < MAX_NAME_DEPTH {
                let code = def.code.clone();
                let s = def.scope.unwrap_or(ctx_sheet);
                self.visit_precedents(&code, s, (0, 0), cell_sheet, row, col, depth + 1, p, seen);
            }
            return;
        }
        if *scope == NameScope::Default
            && let Some(t) = self.host.table(name)
        {
            p.uses_tables = true;
            let all = crate::parser::StructuredRef {
                table: Some(t.name.clone()),
                specials: Vec::new(),
                this_row: false,
                columns: None,
            };
            self.add_table_precedent(&t, &all, (cell_sheet, row), p);
        }
    }

    fn name_def(&self, name: &str, sheet: SheetId) -> Option<&NameDef> {
        self.names
            .get(&name_key(name, Some(sheet)))
            .or_else(|| self.names.get(&name_key(name, None)))
    }

    fn register(&mut self, id: FormulaId) {
        let Some(fc) = self.cell(id) else { return };
        let code = fc.code.clone();
        let p = self.collect_precedents(&code, fc.sheet, fc.row, fc.col);
        let reg = self.graph.add(id, &p);
        if p.volatile {
            self.volatiles.insert(id);
        } else {
            self.volatiles.remove(&id);
        }
        if let Some(fc) = self.cell_mut(id) {
            fc.reg = reg;
            fc.set(VOLATILE, p.volatile);
            fc.set(USES_TABLES, p.uses_tables);
            fc.set(UNRESOLVED, p.unresolved_sheets);
        }
    }

    fn reregister(&mut self, id: FormulaId) {
        let Some(fc) = self.cell_mut(id) else { return };
        let reg = std::mem::take(&mut fc.reg);
        self.graph.remove(id, &reg);
        self.register(id);
    }

    fn free_slot(&mut self, id: FormulaId) {
        if let Some(fc) = self.cells.get_mut(id as usize).and_then(Option::take) {
            if fc.has(DIRTY) {
                self.dirty_count = self.dirty_count.saturating_sub(1);
            }
            self.volatiles.remove(&id);
            self.circular.remove(&id);
            self.release_code(&fc.code, &fc.code_key);
            self.free.push(id);
        }
    }

    /// Removes a formula and marks what depended on it (and on its spill) dirty.
    fn remove_formula(&mut self, id: FormulaId) {
        let Some(fc) = self.cell(id) else { return };
        let (sheet, row, col, claim) = (fc.sheet, fc.row, fc.col, fc.claim);
        let mut affected = Vec::new();
        self.graph
            .dependents_of(sheet, row, col, claim.as_ref(), &mut affected);
        if let Some(c) = claim {
            self.graph
                .claims_overlapping(sheet, &c, &mut |a, _| affected.push(a));
        }
        self.drop_formula(id);
        affected.retain(|&a| a != id);
        self.mark_dirty(affected);
    }

    fn drop_formula(&mut self, id: FormulaId) {
        let Some(fc) = self.cell_mut(id) else { return };
        let reg = std::mem::take(&mut fc.reg);
        let (sheet, row, col, claim) = (fc.sheet, fc.row, fc.col, fc.claim.take());
        self.graph.remove(id, &reg);
        if claim.is_some() {
            self.graph.set_claim(sheet, id, claim, None);
        }
        let key = cell_key(sheet, row, col);
        if self.by_key.get(&key) == Some(&id) {
            self.by_key.remove(&key);
        }
        self.free_slot(id);
    }

    /// Re-registers every formula (after renames and structural edits) and marks all dirty.
    fn rebuild_all(&mut self, keep_claims: bool) {
        let ids: Vec<FormulaId> = self.live_ids().collect();
        self.graph.clear();
        self.dirty.clear();
        self.dirty_count = 0;
        self.volatiles.clear();
        for &id in &ids {
            let mut claim = None;
            if let Some(fc) = self.cell_mut(id) {
                fc.set(DIRTY, false);
                fc.reg = Registration::default();
                if keep_claims {
                    claim = fc.claim;
                } else {
                    fc.claim = None;
                }
                let sheet = fc.sheet;
                if let Some(c) = claim {
                    self.graph.set_claim(sheet, id, None, Some(c));
                }
            }
        }
        for &id in &ids {
            self.register(id);
        }
        self.mark_dirty(ids);
    }

    /// Applies a text rewrite to every formula and defined name. `name_sheet` gives the sheet a
    /// name's formula is considered to live on.
    fn rewrite_all(
        &mut self,
        f: impl Fn(&str, &str) -> Option<String>,
        name_sheet: impl Fn(Option<String>) -> String,
    ) {
        let ids: Vec<FormulaId> = self.live_ids().collect();
        for id in ids {
            let Some(fc) = self.cell(id) else { continue };
            let sheet_name = self.sheet_names.get(&fc.sheet).cloned().unwrap_or_default();
            if let Some(next) = f(&fc.text, &sheet_name)
                && *next != *fc.text
                && let Some(fc) = self.cell_mut(id)
            {
                fc.text = next.into_boxed_str();
            }
        }
        let keys: Vec<(Option<SheetId>, String)> = self.names.keys().cloned().collect();
        for k in keys {
            let Some(def) = self.names.get(&k) else {
                continue;
            };
            let sheet = name_sheet(def.scope.and_then(|s| self.sheet_names.get(&s).cloned()));
            let sheet = if sheet.is_empty() {
                "\u{0}".to_owned()
            } else {
                sheet
            };
            if let Some(next) = f(&def.text, &sheet)
                && next != def.text
                && let Ok(ast) = parse(&next, &ParseOptions::default())
            {
                let code = Arc::new(compile(&ast, (0, 0), self));
                if let Some(d) = self.names.get_mut(&k) {
                    d.text = next;
                    d.code = code;
                }
            }
        }
    }

    fn grow_bounds(&mut self, sheet: SheetId, row: u32, col: u32) {
        let b = self.formula_bounds.entry(sheet).or_insert((0, 0));
        b.0 = b.0.max(row.saturating_add(1));
        b.1 = b.1.max(col.saturating_add(1));
    }

    fn recompute_bounds(&mut self) {
        self.formula_bounds.clear();
        let cells: Vec<(SheetId, u32, u32)> = self
            .cells
            .iter()
            .flatten()
            .map(|c| (c.sheet, c.row, c.col))
            .collect();
        for (s, r, c) in cells {
            self.grow_bounds(s, r, c);
        }
    }
}

impl<H: EngineHost> CompileContext for Engine<H> {
    fn sheet_id(&self, name: &str) -> Option<SheetId> {
        self.sheet_by_name.get(&name.to_uppercase()).copied()
    }

    fn function(&self, normalized_name: &str) -> Option<FunctionId> {
        self.registry.lookup(normalized_name)
    }
}

impl<H: EngineHost> Workbook for Engine<H> {
    fn sheet_id(&self, name: &str) -> Option<SheetId> {
        self.sheet_by_name.get(&name.to_uppercase()).copied()
    }

    fn sheet_name(&self, id: SheetId) -> Option<String> {
        self.sheet_names.get(&id).cloned()
    }

    fn sheets_between(&self, a: SheetId, b: SheetId) -> Option<Vec<SheetId>> {
        let i = self.sheet_order.iter().position(|&s| s == a)?;
        let j = self.sheet_order.iter().position(|&s| s == b)?;
        Some(self.sheet_order[i.min(j)..=i.max(j)].to_vec())
    }

    fn sheet_order(&self) -> Vec<SheetId> {
        self.sheet_order.clone()
    }

    fn lookup_name(&self, name: &str, sheet: SheetId) -> Option<Arc<Expr>> {
        self.name_def(name, sheet).map(|d| d.code.clone())
    }

    fn table(&self, name: &str) -> Option<TableDef> {
        self.host.table(name)
    }

    fn table_at(&self, sheet: SheetId, row: u32, col: u32) -> Option<TableDef> {
        self.host.table_at(sheet, row, col)
    }

    fn used_bounds(&self, sheet: SheetId) -> (u32, u32) {
        let h = self.host.used_bounds(sheet).unwrap_or((MAX_ROWS, MAX_COLS));
        match self.formula_bounds.get(&sheet) {
            Some(&(r, c)) => (h.0.max(r), h.1.max(c)),
            None => h,
        }
    }

    fn registry(&self) -> &dyn FunctionRegistry {
        &*self.registry
    }

    fn coercion(&self) -> CoercionOptions {
        self.coercion
    }

    fn now(&self) -> f64 {
        self.now
    }

    fn is_formula_cell(&self, sheet: SheetId, row: u32, col: u32) -> bool {
        self.formula_at(sheet, row, col).is_some()
    }

    fn formula_text(&self, sheet: SheetId, row: u32, col: u32) -> Option<String> {
        let id = self.formula_at(sheet, row, col)?;
        self.cell(id).map(|c| c.text.to_string())
    }

    fn row_visibility(&self, sheet: SheetId, row: u32) -> RowVisibility {
        self.host.row_visibility(sheet, row)
    }

    fn compile_text(&self, text: &str, style: RefStyle, anchor: (u32, u32)) -> Option<Expr> {
        let ast = parse(text, &ParseOptions { style, anchor }).ok()?;
        Some(compile(&ast, anchor, self))
    }
}

/// Serial evaluation: reading a pending formula evaluates it first (pull), and reading a cell
/// that is being evaluated flags a circular reference.
struct Serial<'a, H: EngineHost> {
    eng: &'a mut Engine<H>,
}

impl<H: EngineHost> CellSource for Serial<'_, H> {
    fn workbook(&self) -> &dyn Workbook {
        &*self.eng
    }

    fn read_cell(&mut self, sheet: SheetId, row: u32, col: u32) -> Value {
        let eng = &mut *self.eng;
        if let Some(id) = eng.formula_at(sheet, row, col) {
            let (evaluating, dirty) = eng
                .cell(id)
                .map_or((false, false), |c| (c.has(EVALUATING), c.has(DIRTY)));
            if evaluating {
                eng.set_circular(id, true);
            } else if dirty {
                eng.evaluate_serial(id);
            }
            return eng
                .cell(id)
                .map(FormulaCell::anchor_value)
                .unwrap_or_default();
        }
        if eng.graph.has_claims() {
            let mut pending = Vec::new();
            eng.graph.claims_at(sheet, row, col, &mut |a, _| {
                if eng
                    .cell(a)
                    .is_some_and(|c| c.has(DIRTY) && !c.has(EVALUATING))
                {
                    pending.push(a);
                }
            });
            for a in pending {
                if eng.cell(a).is_some_and(|c| c.has(DIRTY)) {
                    eng.evaluate_serial(a);
                }
            }
            if let Some(v) = eng.spill_value_at(sheet, row, col) {
                return v;
            }
        }
        eng.host.cell_value(sheet, row, col).unwrap_or_default()
    }

    fn spill_area(&mut self, sheet: SheetId, row: u32, col: u32) -> Result<Bounds, ErrorValue> {
        let eng = &mut *self.eng;
        let id = eng.formula_at(sheet, row, col).ok_or(ErrorValue::Ref)?;
        if eng
            .cell(id)
            .is_some_and(|c| c.has(DIRTY) && !c.has(EVALUATING))
        {
            eng.evaluate_serial(id);
        }
        eng.spill_area_of(id, sheet, row, col)
    }
}

/// Read-only evaluation against the state at the start of a topological level (parallel mode).
/// Reading anything still pending marks the evaluation deferred; it is then redone serially.
#[cfg(feature = "parallel")]
struct Snapshot<'a, H: EngineHost> {
    eng: &'a Engine<H>,
    deferred: bool,
}

#[cfg(feature = "parallel")]
impl<H: EngineHost> CellSource for Snapshot<'_, H> {
    fn workbook(&self) -> &dyn Workbook {
        self.eng
    }

    fn read_cell(&mut self, sheet: SheetId, row: u32, col: u32) -> Value {
        let eng = self.eng;
        if let Some(id) = eng.formula_at(sheet, row, col) {
            return match eng.cell(id) {
                Some(c) if c.has(DIRTY) => {
                    self.deferred = true;
                    Value::Blank
                }
                Some(c) => c.anchor_value(),
                None => Value::Blank,
            };
        }
        if eng.graph.has_claims() {
            let mut pending = false;
            eng.graph.claims_at(sheet, row, col, &mut |a, _| {
                if eng.cell(a).is_some_and(|c| c.has(DIRTY)) {
                    pending = true;
                }
            });
            if pending {
                self.deferred = true;
                return Value::Blank;
            }
            if let Some(v) = eng.spill_value_at(sheet, row, col) {
                return v;
            }
        }
        eng.host.cell_value(sheet, row, col).unwrap_or_default()
    }

    fn spill_area(&mut self, sheet: SheetId, row: u32, col: u32) -> Result<Bounds, ErrorValue> {
        let id = self
            .eng
            .formula_at(sheet, row, col)
            .ok_or(ErrorValue::Ref)?;
        if self.eng.cell(id).is_some_and(|c| c.has(DIRTY)) {
            self.deferred = true;
            return Err(ErrorValue::Ref);
        }
        self.eng.spill_area_of(id, sheet, row, col)
    }
}
