// SPDX-License-Identifier: AGPL-3.0-or-later
//! The host interface: the engine owns formulas, dependencies and computed results, while literal
//! cell values live in the host's storage (the sheet model). [`MemoryHost`] is a complete
//! in-memory implementation used by tests, benchmarks and headless evaluation.

use rustc_hash::FxHashMap;

use super::types::{CellKey, RowVisibility, SheetId, cell_key, unpack_key};
use crate::refs::{Bounds, StructuralEdit};
use crate::values::Value;

/// An Excel table (ListObject) as needed for structured references.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDef {
    /// Table name.
    pub name: String,
    /// Sheet.
    pub sheet: SheetId,
    /// Full range including the header and totals rows.
    pub bounds: Bounds,
    /// Has a header row (the default).
    pub header_row: bool,
    /// Has a totals row.
    pub totals_row: bool,
    /// Column names, left to right.
    pub columns: Vec<String>,
}

/// The host refused to store a value (a read-only host).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the engine host does not accept values")]
pub struct HostReadOnly;

/// Storage of literal cell values and workbook facts the engine needs. Reads happen from several
/// threads during parallel recalculation, so implementations must be `Sync`.
pub trait EngineHost: Send + Sync {
    /// Literal (non-formula) value of a cell, or `None` when it is empty.
    fn cell_value(&self, sheet: SheetId, row: u32, col: u32) -> Option<Value>;

    /// Stores a literal value (`None` clears). Hosts that own their storage elsewhere may refuse.
    fn set_cell_value(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        value: Option<Value>,
    ) -> Result<(), HostReadOnly> {
        let _ = (sheet, row, col, value);
        Err(HostReadOnly)
    }

    /// Extent `(rows, cols)` of the sheet's literal values (exclusive), which keeps whole-column
    /// references cheap. `None` means unknown (the whole sheet).
    fn used_bounds(&self, sheet: SheetId) -> Option<(u32, u32)> {
        let _ = sheet;
        None
    }

    /// A table by name (case-insensitive).
    fn table(&self, name: &str) -> Option<TableDef> {
        let _ = name;
        None
    }

    /// The table containing a cell (for unqualified `[@Col]` references).
    fn table_at(&self, sheet: SheetId, row: u32, col: u32) -> Option<TableDef> {
        let _ = (sheet, row, col);
        None
    }

    /// Called after the engine applied an insert or delete so the host can shift its cells.
    fn apply_structural_edit(&mut self, sheet: SheetId, edit: &StructuralEdit) {
        let _ = (sheet, edit);
    }

    /// Called when a sheet is removed.
    fn remove_sheet(&mut self, sheet: SheetId) {
        let _ = sheet;
    }

    /// Visibility of a row (for `SUBTOTAL`/`AGGREGATE`).
    fn row_visibility(&self, sheet: SheetId, row: u32) -> RowVisibility {
        let _ = (sheet, row);
        RowVisibility::Visible
    }
}

/// An in-memory host: a sparse map of literal values plus a table registry and row visibility.
#[derive(Debug, Default, Clone)]
pub struct MemoryHost {
    cells: FxHashMap<CellKey, Value>,
    bounds: FxHashMap<SheetId, (u32, u32)>,
    tables: FxHashMap<String, TableDef>,
    hidden_rows: FxHashMap<CellKey, RowVisibility>,
}

impl MemoryHost {
    /// An empty host.
    pub fn new() -> MemoryHost {
        MemoryHost::default()
    }

    /// Stores a value (a blank or `None` clears the cell).
    pub fn set(&mut self, sheet: SheetId, row: u32, col: u32, value: Option<Value>) {
        let k = cell_key(sheet, row, col);
        match value {
            None | Some(Value::Blank) => {
                self.cells.remove(&k);
            }
            Some(v) => {
                self.cells.insert(k, v);
                let b = self.bounds.entry(sheet).or_insert((0, 0));
                b.0 = b.0.max(row + 1);
                b.1 = b.1.max(col + 1);
            }
        }
    }

    /// Hides or shows a row; `filtered` marks it as hidden by a filter rather than manually.
    pub fn set_row_hidden(&mut self, sheet: SheetId, row: u32, hidden: bool, filtered: bool) {
        let k = cell_key(sheet, row, 0);
        if hidden {
            let v = if filtered {
                RowVisibility::Filtered
            } else {
                RowVisibility::Hidden
            };
            self.hidden_rows.insert(k, v);
        } else {
            self.hidden_rows.remove(&k);
        }
    }

    /// Adds or replaces a table (call `Engine::tables_changed` afterwards).
    pub fn add_table(&mut self, table: TableDef) {
        self.tables.insert(table.name.to_uppercase(), table);
    }

    /// Removes a table.
    pub fn remove_table(&mut self, name: &str) {
        self.tables.remove(&name.to_uppercase());
    }

    /// Number of stored values.
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// True when no values are stored.
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }
}

impl EngineHost for MemoryHost {
    fn cell_value(&self, sheet: SheetId, row: u32, col: u32) -> Option<Value> {
        self.cells.get(&cell_key(sheet, row, col)).cloned()
    }

    fn set_cell_value(
        &mut self,
        sheet: SheetId,
        row: u32,
        col: u32,
        value: Option<Value>,
    ) -> Result<(), HostReadOnly> {
        self.set(sheet, row, col, value);
        Ok(())
    }

    fn used_bounds(&self, sheet: SheetId) -> Option<(u32, u32)> {
        Some(self.bounds.get(&sheet).copied().unwrap_or((0, 0)))
    }

    fn table(&self, name: &str) -> Option<TableDef> {
        self.tables.get(&name.to_uppercase()).cloned()
    }

    fn table_at(&self, sheet: SheetId, row: u32, col: u32) -> Option<TableDef> {
        self.tables
            .values()
            .find(|t| t.sheet == sheet && t.bounds.contains(row, col))
            .cloned()
    }

    fn apply_structural_edit(&mut self, sheet: SheetId, edit: &StructuralEdit) {
        let rows = edit.kind.is_rows();
        let insert = edit.kind.is_insert();
        let (at, count) = (u64::from(edit.at), u64::from(edit.count));
        let max = if rows {
            crate::refs::MAX_ROWS
        } else {
            crate::refs::MAX_COLS
        };
        let keys: Vec<CellKey> = self
            .cells
            .keys()
            .copied()
            .filter(|&k| {
                let (s, r, c) = unpack_key(k);
                s == sheet && u64::from(if rows { r } else { c }) >= at
            })
            .collect();
        let mut moved = Vec::with_capacity(keys.len());
        for k in keys {
            let Some(v) = self.cells.remove(&k) else {
                continue;
            };
            let (_, r, c) = unpack_key(k);
            let idx = u64::from(if rows { r } else { c });
            if !insert && idx < at + count {
                continue;
            }
            let n = if insert { idx + count } else { idx - count };
            if n >= u64::from(max) {
                continue;
            }
            let n = n as u32;
            moved.push((if rows { (n, c) } else { (r, n) }, v));
        }
        for ((r, c), v) in moved {
            self.cells.insert(cell_key(sheet, r, c), v);
        }
        if insert && let Some(b) = self.bounds.get_mut(&sheet) {
            if rows {
                b.0 = b.0.saturating_add(edit.count).min(max);
            } else {
                b.1 = b.1.saturating_add(edit.count).min(max);
            }
        }
    }

    fn remove_sheet(&mut self, sheet: SheetId) {
        self.cells.retain(|&k, _| unpack_key(k).0 != sheet);
        self.bounds.remove(&sheet);
        self.tables.retain(|_, t| t.sheet != sheet);
    }

    fn row_visibility(&self, sheet: SheetId, row: u32) -> RowVisibility {
        self.hidden_rows
            .get(&cell_key(sheet, row, 0))
            .copied()
            .unwrap_or_default()
    }
}
