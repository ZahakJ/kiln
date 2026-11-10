// SPDX-License-Identifier: AGPL-3.0-or-later
//! Serialisation: whole workbooks and single sheets to postcard bytes (autosave journals, the
//! wasm bridge, undo of sheet removal) and back. A workbook snapshot carries cells (with formula
//! text and cached results), row and column properties, metadata, names, styles, theme and
//! properties; the formula engine is rebuilt on load.

use std::collections::BTreeMap;

use kiln_formula::SheetId;
use serde::{Deserialize, Serialize};

use crate::color::Theme;
use crate::error::{ModelError, Result};
use crate::meta::{DefinedName, Person, WorkbookProps, WorkbookProtection};
use crate::sheet::{Sheet, SheetLayout, SheetMeta};
use crate::store::CellKind;
use crate::styles::StyleTable;
use crate::value::CellValue;
use crate::workbook::{CellRecord, Workbook, WorkbookOptions};
use kiln_formula::values::Value;

/// Magic bytes at the start of a serialised workbook.
pub const MAGIC: &[u8; 8] = b"KILNGRID";
/// Format version of serialised workbooks.
pub const FORMAT_VERSION: u8 = 1;

/// The passthrough entry that carries, under manual calculation, the arrays spilled formulas
/// show (a cell a formula spills into holds nothing of its own): a load puts them back, stale or
/// not, as Excel keeps a file's spilled values until the next calculation. Builds that do not
/// know it keep it as an opaque part and recalculate as before.
pub const SPILLS_ENTRY: &str = "kiln:manual-spills";

/// One stored spill: sheet, anchor row and column, rows, columns and the values row by row.
type StoredSpill = (u32, u32, u32, u32, u32, Vec<CellValue>);

/// Everything about one sheet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SheetSnapshot {
    /// Sheet id.
    pub id: u32,
    /// Name.
    pub name: String,
    /// Every stored cell, row-major.
    pub cells: Vec<CellRecord>,
    /// Row and column properties.
    pub layout: SheetLayout,
    /// Metadata.
    pub meta: SheetMeta,
}

impl SheetSnapshot {
    /// Builds the sheet (without binding formulas) and lists the formulas to bind.
    pub(crate) fn into_sheet(self, mdw: f64) -> (Sheet, Vec<(u32, u32, String, bool)>) {
        let mut sheet = Sheet::new(SheetId(self.id), self.name, mdw);
        let mut formulas = Vec::new();
        for rec in self.cells {
            let kind = match (&rec.formula, rec.legacy) {
                (None, _) => CellKind::Value,
                (Some(_), false) => CellKind::Formula,
                (Some(_), true) => CellKind::LegacyFormula,
            };
            sheet
                .cells
                .set(rec.row, rec.col, rec.value, kind, rec.style);
            if let Some(f) = rec.formula {
                formulas.push((rec.row, rec.col, f, rec.legacy));
            }
        }
        sheet.set_layout(self.layout);
        sheet.set_meta(self.meta);
        (sheet, formulas)
    }
}

/// Everything about a workbook.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkbookSnapshot {
    /// Sheets in order.
    pub sheets: Vec<SheetSnapshot>,
    /// Defined names.
    pub names: Vec<DefinedName>,
    /// Styles.
    pub styles: StyleTable,
    /// Theme.
    pub theme: Theme,
    /// Properties.
    pub props: WorkbookProps,
    /// Protection.
    pub protection: Option<WorkbookProtection>,
    /// People.
    pub persons: Vec<Person>,
    /// Custom lists.
    pub custom_lists: Vec<Vec<String>>,
    /// Opaque file parts.
    pub passthrough: BTreeMap<String, Vec<u8>>,
    /// Id counters (sheet, table, rule).
    pub counters: (u32, u32, u32),
}

impl PartialEq for StyleTable {
    fn eq(&self, other: &StyleTable) -> bool {
        self.to_parts() == other.to_parts()
    }
}

impl Workbook {
    /// A snapshot of one sheet.
    pub fn snapshot_sheet(&self, sheet: SheetId) -> Result<SheetSnapshot> {
        let s = self.sheet_ok(sheet)?;
        let positions: Vec<(u32, u32)> = s.cells.iter().map(|(r, c, _)| (r, c)).collect();
        Ok(SheetSnapshot {
            id: sheet.0,
            name: s.name.clone(),
            cells: positions
                .into_iter()
                .map(|(r, c)| self.record(sheet, r, c))
                .collect(),
            layout: s.layout(),
            meta: s.meta.clone(),
        })
    }

    /// A snapshot of the whole workbook.
    pub fn snapshot(&self) -> WorkbookSnapshot {
        WorkbookSnapshot {
            sheets: self
                .sheets()
                .iter()
                .filter_map(|s| self.snapshot_sheet(s.id()).ok())
                .collect(),
            names: self.names.clone(),
            styles: self.styles.clone(),
            theme: self.theme.clone(),
            props: self.props.clone(),
            protection: self.protection.clone(),
            persons: self.persons.clone(),
            custom_lists: self.custom_lists.clone(),
            passthrough: self.passthrough_with_spills(),
            counters: (self.next_sheet_id, self.next_table_id, self.next_rule_id),
        }
    }

    /// The passthrough parts, with the spilled arrays under manual calculation.
    fn passthrough_with_spills(&self) -> BTreeMap<String, Vec<u8>> {
        let mut parts = self.passthrough.clone();
        if self.props.calc.mode != crate::meta::CalcMode::Manual {
            return parts;
        }
        let mut spills: Vec<StoredSpill> = Vec::new();
        for s in self.sheets() {
            for (r, c, d) in s.cells.iter() {
                if !d.kind.is_engine_formula() {
                    continue;
                }
                if let Some(Value::Array(a)) = self.engine.result(s.id(), r, c) {
                    let values = (0..a.rows())
                        .flat_map(|i| (0..a.cols()).map(move |j| (i, j)))
                        .map(|(i, j)| CellValue::from_value(a.get(i, j)))
                        .collect();
                    let dim = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
                    spills.push((s.id().0, r, c, dim(a.rows()), dim(a.cols()), values));
                }
            }
        }
        if !spills.is_empty()
            && let Ok(bytes) = postcard::to_allocvec(&spills)
        {
            parts.insert(SPILLS_ENTRY.to_owned(), bytes);
        }
        parts
    }

    /// A workbook from a snapshot (the engine is rebuilt and, unless calculation is manual, every
    /// formula recalculated).
    pub fn from_snapshot(snap: WorkbookSnapshot, opts: WorkbookOptions) -> Result<Workbook> {
        let wb = Workbook::with_options(WorkbookOptions {
            sheets: Some(Vec::new()),
            date1904: snap.props.date1904,
            ..opts
        });
        Workbook::load_snapshot(wb, snap)
    }

    /// Fills an empty workbook from a snapshot.
    fn load_snapshot(mut wb: Workbook, snap: WorkbookSnapshot) -> Result<Workbook> {
        wb.styles = snap.styles;
        wb.theme = snap.theme;
        wb.props = snap.props;
        wb.protection = snap.protection;
        wb.persons = snap.persons;
        wb.custom_lists = snap.custom_lists;
        wb.passthrough = snap.passthrough;
        if let Some(bytes) = wb.passthrough.remove(SPILLS_ENTRY) {
            let spills: Vec<StoredSpill> = postcard::from_bytes(&bytes).unwrap_or_default();
            wb.calc.stored_spills = spills
                .into_iter()
                .filter_map(|(s, r, c, rows, cols, values)| {
                    let data = values.iter().map(CellValue::to_value).collect();
                    let a = kiln_formula::values::Array2D::new(rows as usize, cols as usize, data)?;
                    Some((SheetId(s), r, c, Value::Array(std::sync::Arc::new(a))))
                })
                .collect();
        }
        let font = wb.styles.default_font().clone();
        use crate::axis::FontMetrics;
        wb.mdw = crate::axis::DefaultFontMetrics.max_digit_width(&font.name, font.size.0);
        let mut formulas = Vec::new();
        let mut seen_names: Vec<String> = Vec::new();
        for s in snap.sheets {
            let id = SheetId(s.id);
            crate::workbook::check_sheet_name(&s.name)?;
            let upper = s.name.to_uppercase();
            if seen_names.contains(&upper) || wb.sheet(id).is_some() {
                return Err(ModelError::invalid(format!(
                    "duplicate sheet \"{}\" in the data",
                    s.name
                )));
            }
            seen_names.push(upper);
            let (sheet, fs) = s.into_sheet(wb.mdw);
            formulas.extend(fs.into_iter().map(|(r, c, t, l)| (id, r, c, t, l)));
            let host = wb.host_mut();
            host.list.push(sheet);
            host.reindex();
        }
        if wb.sheets().is_empty() {
            return Err(ModelError::invalid("a workbook needs at least one sheet"));
        }
        wb.names = snap.names;
        let (s, t, r) = snap.counters;
        let max_sheet = wb.sheets().iter().map(|s| s.id().0).max().unwrap_or(0);
        wb.next_sheet_id = s.max(max_sheet + 1);
        wb.next_table_id = t.max(1);
        wb.next_rule_id = r.max(1);
        wb.props.active_sheet = wb.props.active_sheet.min(wb.sheets().len() - 1);
        wb.reset_engine_with(formulas);
        wb.changes = crate::workbook::Changes::default();
        Ok(wb)
    }

    /// The workbook as bytes (magic, version, postcard).
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = MAGIC.to_vec();
        out.push(FORMAT_VERSION);
        postcard::to_extend(&self.snapshot(), out).map_err(|e| ModelError::invalid(e.to_string()))
    }

    /// A workbook from [`Workbook::to_bytes`] output that shows what the original showed: every
    /// formula's stored result stands, as a file's stored results do, until something it
    /// depends on changes (a copy to check the original against, value for value).
    pub fn from_bytes_as_shown(bytes: &[u8], opts: WorkbookOptions) -> Result<Workbook> {
        let snap = Workbook::decode(bytes)?;
        let mut wb = Workbook::with_options(WorkbookOptions {
            sheets: Some(Vec::new()),
            ..opts
        });
        wb.calc.keep_stored = true;
        wb = Workbook::load_snapshot(wb, snap)?;
        wb.take_stored_results();
        wb.calc.keep_stored = false;
        wb.changes = crate::workbook::Changes::default();
        Ok(wb)
    }

    /// A workbook from [`Workbook::to_bytes`] output.
    pub fn from_bytes(bytes: &[u8], opts: WorkbookOptions) -> Result<Workbook> {
        Workbook::from_snapshot(Workbook::decode(bytes)?, opts)
    }

    fn decode(bytes: &[u8]) -> Result<WorkbookSnapshot> {
        let body = bytes
            .strip_prefix(MAGIC.as_slice())
            .ok_or_else(|| ModelError::invalid("not a Kiln Grid workbook"))?;
        let (&version, body) = body
            .split_first()
            .ok_or_else(|| ModelError::invalid("truncated workbook data"))?;
        if version != FORMAT_VERSION {
            return Err(ModelError::invalid(format!(
                "workbook data version {version} is not supported (this build reads {FORMAT_VERSION})"
            )));
        }
        postcard::from_bytes(body)
            .map_err(|e| ModelError::invalid(format!("damaged workbook data: {e}")))
    }
}
