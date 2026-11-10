// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formula binding: recalculation on edit (automatic and manual modes with Excel's F9
//! semantics), results and spill ranges, iterative calculation, ad-hoc evaluation and the Formula
//! Lab.
//!
//! Formulas are interned in `kiln-formula`'s engine (filled formulas share one compiled form),
//! which evaluates independent cells of each topological level in parallel. After every
//! recalculation the results of changed formulas are written back to the store as cached values,
//! so file writers and anything reading the store see current results.

use kiln_formula::engine::{EngineError, ExplainStep, IterativeOptions, RecalcResult, SpillRange};
use kiln_formula::values::Value;
use kiln_formula::{Engine, SheetId};

use crate::address::Range;
use crate::error::{ModelError, Result};
use crate::meta::CalcMode;
use crate::store::CellKind;
use crate::value::CellValue;
use crate::workbook::{Sheets, Workbook, engine_options, strip_equals};

fn engine_err(e: EngineError) -> ModelError {
    match e {
        EngineError::Parse(p) => ModelError::InvalidFormula {
            text: String::new(),
            message: p.message,
        },
        other => ModelError::invalid(other.to_string()),
    }
}

impl Workbook {
    /// True when edits are waiting for a recalculation (manual mode shows "Calculate").
    pub fn is_calc_pending(&self) -> bool {
        self.calc.pending || self.engine.graph_stats().dirty > 0
    }

    /// Number of formula evaluations the last recalculation performed.
    pub fn last_evaluated(&self) -> usize {
        self.calc.last_evaluated
    }

    /// Called when the outermost command ends: recalculates in automatic mode.
    pub(crate) fn command_done(&mut self) {
        let entered = std::mem::take(&mut self.calc.entered);
        if self.props.calc.mode != CalcMode::Manual {
            if self.calc.pending {
                self.recalc();
            }
            return;
        }
        // Manual: a formula is calculated as it is entered; what depends on it waits for F9.
        if !entered.is_empty() {
            let res = self.engine.calculate_cells(&entered);
            let pending = self.calc.pending;
            let last = self.calc.last_evaluated;
            self.write_back(&res);
            self.calc.pending = pending;
            self.calc.last_evaluated = last;
        }
    }

    /// Calculate Now (F9): recalculates every formula affected by changes since the last
    /// calculation, plus volatile functions and their dependents.
    pub fn recalc(&mut self) -> RecalcResult {
        self.mutations += 1;
        let res = if std::mem::take(&mut self.calc.needs_full) {
            self.engine.recalc_all()
        } else {
            self.engine.recalc()
        };
        self.write_back(&res);
        res
    }

    /// While `on`, a recalculation leaves results already in the store alone and fills only
    /// formulas that have none. A file's stored results are what it says: a load keeps them until
    /// something they depend on changes.
    pub fn keep_stored_results(&mut self, on: bool) {
        self.mutations += 1;
        self.calc.keep_stored = on;
    }

    /// Ends a load that keeps stored results: formulas that call a function nobody knows are held
    /// at theirs, what depends on them is brought up to date, and every other stored result
    /// replaces what the engine worked out.
    pub fn take_stored_results(&mut self) {
        self.mutations += 1;
        let unknown = self.engine.unknown_function_cells();
        let pins: Vec<_> = unknown
            .into_iter()
            .filter_map(|(s, r, c)| {
                let d = self.sheet(s)?.cells().get(r, c)?;
                (!d.value.is_blank()).then(|| (s, r, c, d.value.to_value()))
            })
            .collect();
        if !pins.is_empty() {
            self.engine.pin_results(pins);
            self.recalc();
        }
        let mut stored = Vec::new();
        for s in self.sheets() {
            let id = s.id();
            for (r, c, d) in s.cells().iter() {
                if d.kind.is_engine_formula() && !d.value.is_blank() {
                    stored.push((id, r, c, d.value.to_value()));
                }
            }
        }
        self.engine.take_stored_results(stored);
    }

    /// Calculate Full (Ctrl+Alt+F9): recalculates every formula.
    pub fn recalc_all(&mut self) -> RecalcResult {
        self.mutations += 1;
        self.calc.needs_full = false;
        let res = self.engine.recalc_all();
        self.write_back(&res);
        res
    }

    /// Calculate Sheet (Shift+F9): the formulas of one sheet and whatever depends on them.
    pub fn recalc_sheet(&mut self, sheet: SheetId) -> Result<RecalcResult> {
        self.mutations += 1;
        let cells: Vec<(u32, u32, String, bool)> = {
            let s = self.sheet_ok(sheet)?;
            s.cells
                .iter()
                .filter(|(_, _, d)| d.kind.is_engine_formula())
                .filter_map(|(r, c, d)| {
                    let text = self.engine.formula(sheet, r, c)?.to_owned();
                    Some((r, c, text, d.kind == CellKind::LegacyFormula))
                })
                .collect()
        };
        for (r, c, text, legacy) in cells {
            self.bind_formula(sheet, r, c, &text, legacy);
        }
        Ok(self.recalc())
    }

    fn write_back(&mut self, res: &RecalcResult) {
        self.calc.pending = false;
        self.calc.last_evaluated = res.evaluated;
        for c in &res.changed {
            if self.calc.keep_stored
                && self
                    .engine
                    .host()
                    .get(c.sheet)
                    .is_some_and(|s| !s.cells.value(c.row, c.col).is_blank())
            {
                continue;
            }
            self.cf_cache
                .invalidate(c.sheet, &crate::Range::cell(c.row, c.col));
            let v = self
                .engine
                .value(c.sheet, c.row, c.col)
                .map_or(CellValue::Blank, |v| CellValue::from_value(&v));
            if let Some(s) = self.engine.host_mut().get_mut(c.sheet) {
                s.cells.set_cached_value(c.row, c.col, v);
            }
        }
        self.changes
            .recalculated
            .extend(res.changed.iter().copied());
    }

    /// The full result of a formula (an array for a dynamic-array formula).
    pub fn result(&self, sheet: SheetId, row: u32, col: u32) -> Option<Value> {
        self.engine.result(sheet, row, col)
    }

    /// The spill range containing a cell (the anchor or any cell it spilled into); a blocked
    /// anchor reports the range it would occupy.
    pub fn spill_range(&self, sheet: SheetId, row: u32, col: u32) -> Option<SpillRange> {
        self.engine.spill_range(sheet, row, col)
    }

    /// True when the cell's formula is part of a circular reference.
    pub fn is_circular(&self, sheet: SheetId, row: u32, col: u32) -> bool {
        self.engine.is_circular(sheet, row, col)
    }

    /// Every cell in a circular reference (the status bar's "Circular References").
    pub fn circular_cells(&self) -> Vec<kiln_formula::engine::CellRef> {
        self.engine.circular_cells()
    }

    /// The link a `HYPERLINK` formula in the cell evaluated to.
    pub fn formula_hyperlink(&self, sheet: SheetId, row: u32, col: u32) -> Option<&str> {
        self.engine.hyperlink(sheet, row, col)
    }

    /// Evaluates a formula as if entered at (row, col) of `sheet`, without storing it.
    pub fn evaluate_at(
        &mut self,
        formula: &str,
        sheet: SheetId,
        row: u32,
        col: u32,
    ) -> Result<Value> {
        self.sheet_ok(sheet)?;
        let text = format!("={}", strip_equals(formula));
        self.engine
            .evaluate_at(&text, sheet, row, col, false)
            .map_err(|e| match engine_err(e) {
                ModelError::InvalidFormula { message, .. } => ModelError::InvalidFormula {
                    text: formula.to_owned(),
                    message,
                },
                other => other,
            })
    }

    /// Evaluates a formula in the first sheet's A1.
    pub fn evaluate(&mut self, formula: &str) -> Result<Value> {
        let sheet = self
            .first_sheet()
            .ok_or_else(|| ModelError::invalid("the workbook has no sheets"))?;
        self.evaluate_at(formula, sheet, 0, 0)
    }

    /// Step-by-step evaluation of a cell's formula (Formula Lab, Evaluate Formula).
    pub fn explain(&mut self, sheet: SheetId, row: u32, col: u32) -> Result<ExplainStep> {
        let f = self
            .formula(sheet, row, col)
            .ok_or_else(|| ModelError::invalid("the cell does not contain a formula"))?;
        self.engine
            .explain_at(&format!("={f}"), sheet, row, col)
            .map_err(engine_err)
    }

    /// Checks formula text parses (for commands, before anything changes).
    pub fn check_formula(text: &str) -> Result<String> {
        let t = strip_equals(text);
        kiln_formula::parse(&format!("={t}"), &kiln_formula::ParseOptions::default()).map_err(
            |e| ModelError::InvalidFormula {
                text: format!("={t}"),
                message: e.message,
            },
        )?;
        Ok(t.to_owned())
    }

    /// Applies new calculation properties to the engine (iteration settings; the date system
    /// rebuilds the engine).
    pub(crate) fn props_changed(&mut self, date_system_changed: bool) {
        self.cf_cache.clear();
        if date_system_changed {
            self.rebuild_engine();
            return;
        }
        let c = self.props.calc;
        self.engine.set_iterative(IterativeOptions {
            enabled: c.iterate,
            max_iterations: c.iterate_count.max(1),
            max_change: c.iterate_delta.max(0.0),
        });
        self.calc.needs_full = true;
        self.calc.pending = true;
    }

    /// Re-creates the engine from the model (after loading, or when the date system changes).
    pub(crate) fn rebuild_engine(&mut self) {
        let mut formulas: Vec<(SheetId, u32, u32, String, bool)> = Vec::new();
        for s in self.sheets() {
            for (r, c, d) in s.cells.iter() {
                if d.kind.is_engine_formula()
                    && let Some(t) = self.engine.formula(s.id(), r, c)
                {
                    formulas.push((
                        s.id(),
                        r,
                        c,
                        t.to_owned(),
                        d.kind == CellKind::LegacyFormula,
                    ));
                }
            }
        }
        self.reset_engine_with(formulas);
    }

    /// Replaces the engine with a fresh one over the current sheets and binds `formulas`.
    pub(crate) fn reset_engine_with(&mut self, formulas: Vec<(SheetId, u32, u32, String, bool)>) {
        // Under manual calculation a formula shows what it last came to, stale or not, until the
        // next calculation: a new engine keeps it (a load, a change of the workbook's options).
        // A spilled array keeps its whole result: the engine's, or the one a load stored.
        let manual = self.props.calc.mode == CalcMode::Manual;
        let stored = std::mem::take(&mut self.calc.stored_spills);
        let shown: Vec<(SheetId, u32, u32, CellValue, Option<Value>)> = if manual {
            formulas
                .iter()
                .filter_map(|(s, r, c, _, _)| {
                    let v = self.engine.host().get(*s)?.cells.value(*r, *c).clone();
                    let array = self
                        .engine
                        .result(*s, *r, *c)
                        .filter(|a| matches!(a, Value::Array(_)))
                        .or_else(|| {
                            stored
                                .iter()
                                .find(|x| (x.0, x.1, x.2) == (*s, *r, *c))
                                .map(|x| x.3.clone())
                        });
                    (!v.is_blank() || array.is_some()).then_some((*s, *r, *c, v, array))
                })
                .collect()
        } else {
            Vec::new()
        };
        let host: Sheets = std::mem::take(self.engine.host_mut());
        let opts = engine_options(&self.props, self.clock.clone(), self.parallel);
        let mut engine = Engine::with_host(host, opts);
        engine.set_random(self.random.clone());
        let order: Vec<(SheetId, String)> = engine
            .host()
            .list
            .iter()
            .map(|s| (s.id(), s.name.clone()))
            .collect();
        for (id, name) in order {
            // Ids and names are unique in the model, so this cannot fail.
            let _ = engine.add_sheet_with_id(&name, id);
        }
        engine.host_mut().reindex();
        self.engine = engine;
        self.sync_names();
        for (s, r, c, t, legacy) in formulas {
            self.bind_formula(s, r, c, &t, legacy);
        }
        self.engine.tables_changed();
        self.calc.needs_full = true;
        self.calc.pending = true;
        if self.props.calc.mode != CalcMode::Manual {
            self.recalc();
        }
        self.calc.entered.clear();
        if !shown.is_empty() {
            let results = shown
                .iter()
                .map(|(s, r, c, v, a)| (*s, *r, *c, a.clone().unwrap_or_else(|| v.to_value())))
                .collect();
            self.engine.restore_results(results);
            for (s, r, c, v, _) in shown {
                if let Some(sheet) = self.engine.host_mut().get_mut(s) {
                    sheet.cells.set_cached_value(r, c, v);
                }
            }
        }
    }

    /// The cells a formula's result occupies, for repainting: the spill range of an anchor or
    /// the cell itself.
    pub fn result_area(&self, sheet: SheetId, row: u32, col: u32) -> Range {
        self.spill_range(sheet, row, col)
            .map_or(Range::cell(row, col), |s| Range::from_bounds(s.bounds))
    }
}
