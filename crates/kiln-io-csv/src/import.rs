// SPDX-License-Identifier: AGPL-3.0-or-later
//! Records → a workbook: each field is recognised the way Excel's cell entry does (numbers,
//! percentages, currency, dates, times, logicals, errors; leading `'` keeps text), with the
//! number format the typing implies. Formulas (`=…`) stay text unless asked for.

use std::sync::Arc;

use kiln_sheet::data::parse_input;
use kiln_sheet::serial::{SheetSnapshot, WorkbookSnapshot};
use kiln_sheet::styles::StyleTable;
use kiln_sheet::workbook::CellRecord;
use kiln_sheet::{
    CellValue, MAX_COLS, MAX_ROWS, SheetLayout, SheetMeta, Workbook, WorkbookOptions,
};

use crate::{CsvError, ImportOptions};

/// Excel's limit on the characters (UTF-16 code units) in one cell; longer cells make a saved
/// workbook need repair, so imports truncate them.
pub const MAX_CELL_CHARS: usize = 32_767;

/// `field` cut to [`MAX_CELL_CHARS`] UTF-16 code units (on a character boundary), or `None` when
/// it already fits.
#[must_use]
pub fn truncate_cell(field: &str) -> Option<&str> {
    // A UTF-8 string of n bytes holds at most n UTF-16 units.
    if field.len() <= MAX_CELL_CHARS {
        return None;
    }
    let mut units = 0usize;
    for (i, ch) in field.char_indices() {
        units += ch.len_utf16();
        if units > MAX_CELL_CHARS {
            return Some(&field[..i]);
        }
    }
    None
}

/// What an import reports besides the workbook.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Rows read.
    pub rows: usize,
    /// Widest row.
    pub cols: usize,
    /// What did not fit (rows or columns beyond the sheet).
    pub warnings: Vec<String>,
}

/// Builds a one-sheet workbook from records.
///
/// # Errors
///
/// [`CsvError::Model`] when the model refuses the sheet name.
pub fn records_to_workbook(
    records: &[Vec<String>],
    options: &ImportOptions,
) -> Result<(Workbook, ImportReport), CsvError> {
    let mut report = ImportReport::default();
    let mut styles = StyleTable::default();
    // Number format → style index (a handful of entries).
    let mut formats: Vec<(String, u32)> = Vec::new();
    let mut cells = Vec::new();
    let mut texts: std::collections::HashMap<String, Arc<str>> = std::collections::HashMap::new();
    // Rows wider than the sheet (first such row, count) and cells cut to the cell limit.
    let mut wide: Option<(usize, usize)> = None;
    let mut long: Option<(usize, usize, usize)> = None;
    for (r, rec) in records.iter().enumerate() {
        let Ok(row) = u32::try_from(r) else { break };
        if row >= MAX_ROWS {
            report.warnings.push(format!(
                "rows after {MAX_ROWS} do not fit on a sheet and were not imported"
            ));
            break;
        }
        report.rows = r + 1;
        report.cols = report.cols.max(rec.len());
        for (c, field) in rec.iter().enumerate() {
            let Ok(col) = u32::try_from(c) else { break };
            if col >= MAX_COLS {
                let w = wide.get_or_insert((r + 1, 0));
                w.1 += 1;
                break;
            }
            if field.is_empty() {
                continue;
            }
            let field = match truncate_cell(field) {
                Some(cut) => {
                    let l = long.get_or_insert((r + 1, c + 1, 0));
                    l.2 += 1;
                    cut
                }
                None => field.as_str(),
            };
            let (value, formula, fmt) =
                if options.formulas && field.starts_with('=') && field.len() > 1 {
                    (CellValue::Blank, Some(field[1..].to_owned()), None)
                } else if options.as_text {
                    (CellValue::text(field), None, None)
                } else {
                    let p = parse_input(field, options.date1904);
                    let v = match p.value {
                        CellValue::Text(t) if t.len() <= 64 => {
                            let a = texts
                                .entry(t.to_string())
                                .or_insert_with(|| t.clone())
                                .clone();
                            CellValue::Text(a)
                        }
                        v => v,
                    };
                    (v, None, p.num_fmt)
                };
            let style = match fmt {
                Some(f) => match formats.iter().find(|(x, _)| *x == f).map(|(_, s)| *s) {
                    Some(s) => s,
                    None => {
                        let mut st = styles.get(0);
                        st.num_fmt = Arc::from(f.as_str());
                        let s = styles.intern(&st);
                        formats.push((f, s));
                        s
                    }
                },
                None => 0,
            };
            cells.push(CellRecord {
                row,
                col,
                value,
                formula,
                legacy: false,
                style,
            });
        }
    }
    if let Some((first, n)) = wide {
        report.warnings.push(format!(
            "{n} row(s) had more than {MAX_COLS} columns (the first is row {first}); the extra \
             columns were not imported"
        ));
    }
    if let Some((row, col, n)) = long {
        report.warnings.push(format!(
            "{n} cell(s) had more than {MAX_CELL_CHARS} characters (the first is row {row}, \
             column {col}) and were cut to fit a cell"
        ));
    }
    let name = options
        .sheet_name
        .clone()
        .filter(|n| kiln_sheet::workbook::check_sheet_name(n).is_ok())
        .unwrap_or_else(|| "Sheet1".to_owned());
    let snapshot = WorkbookSnapshot {
        sheets: vec![SheetSnapshot {
            id: 1,
            name,
            cells,
            layout: SheetLayout::default(),
            meta: SheetMeta::default(),
        }],
        names: Vec::new(),
        styles,
        theme: kiln_sheet::Theme::default(),
        props: kiln_sheet::meta::WorkbookProps {
            date1904: options.date1904,
            ..Default::default()
        },
        protection: None,
        persons: Vec::new(),
        custom_lists: Vec::new(),
        passthrough: Default::default(),
        counters: (2, 1, 1),
    };
    let wb = Workbook::from_snapshot(snapshot, WorkbookOptions::default())?;
    Ok((wb, report))
}
