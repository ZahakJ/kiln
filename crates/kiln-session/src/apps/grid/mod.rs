// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid workbooks: `.xlsx`, `.xlsm` and `.xltx` files read and written by `kiln-io-xlsx`
//! and `.csv` files by `kiln-io-csv` (WP-70), into the `kiln-sheet` model (WP-69).
//!
//! - **Open**: the file's values, formulas (calculated on open), formats and metadata; the
//!   reader's source is kept so a save carries every part the model does not cover.
//! - **Save**: `.xlsx`, `.xlsm` and `.xltx` with everything kept; `.csv` writes the active sheet
//!   as displayed, as Excel does. The Compatibility Checker says what a format loses.
//! - **Status and accessibility**: the selection's statistics and the sheets as grids.
//!
//! The grid surface, selection and editing commands arrive with WP-71; until then commands
//! return `Unsupported` with the reason, never a silent no-op.

#[cfg(test)]
mod tests;

use kiln_common::{KilnError, NodeId, Result};
use kiln_io_csv::{ExportOptions, ImportOptions, read_csv, write_delimited};
use kiln_io_xlsx::{WriteOptions, XlsxKind, XlsxSource, read_xlsx, write_xlsx};
use kiln_protocol::{
    A11yNode, A11yRole, A11yTree, CellAddr, CellRange, CellStats, Command, CompatIssue,
    CompatOrigin, CompatSeverity, DocKind, FileFormat, OpenHint, SelKind, Selection, StatusBar,
};
use kiln_sheet::{CellValue, Workbook};

use crate::model::{AppFactory, Change, DocModel};
use crate::undo::StateId;

/// The formats Grid opens; the first four it also saves.
const FORMATS: [FileFormat; 4] = [
    FileFormat::Xlsx,
    FileFormat::Xlsm,
    FileFormat::Xltx,
    FileFormat::Csv,
];

/// Creates and opens Grid workbooks.
#[derive(Debug, Clone, Copy, Default)]
pub struct GridApp;

impl AppFactory for GridApp {
    fn kind(&self) -> DocKind {
        DocKind::Grid
    }

    fn opens(&self, format: FileFormat) -> bool {
        FORMATS.contains(&format)
    }

    fn create(&self) -> Box<dyn DocModel> {
        Box::new(GridDoc::new(Workbook::new(), None, Vec::new(), None))
    }

    fn open(&self, bytes: &[u8], format: FileFormat, hint: &OpenHint) -> Result<Box<dyn DocModel>> {
        let name = hint.path.clone().or_else(|| hint.name.clone());
        let format_error = |message: String| KilnError::Format {
            format: format.extension().to_owned(),
            message,
        };
        match format {
            FileFormat::Csv => {
                let stem = name
                    .as_deref()
                    .and_then(|n| std::path::Path::new(n).file_stem())
                    .and_then(|s| s.to_str())
                    .map(|s| s.chars().take(31).collect());
                let imp = read_csv(
                    bytes,
                    &ImportOptions {
                        sheet_name: stem,
                        ..ImportOptions::default()
                    },
                )
                .map_err(|e| format_error(e.to_string()))?;
                Ok(Box::new(GridDoc::new(
                    imp.workbook,
                    None,
                    imp.report.warnings,
                    name,
                )))
            }
            FileFormat::Xlsx | FileFormat::Xlsm | FileFormat::Xltx => {
                let read = read_xlsx(bytes).map_err(|e| format_error(e.to_string()))?;
                Ok(Box::new(GridDoc::new(
                    read.workbook,
                    Some(read.source),
                    read.warnings,
                    name,
                )))
            }
            other => Err(KilnError::Unsupported(format!(
                ".{} files in Kiln Grid",
                other.extension()
            ))),
        }
    }
}

/// A Grid workbook held by the session.
#[derive(Debug)]
pub struct GridDoc {
    workbook: Workbook,
    /// The file read, for a faithful save.
    source: Option<XlsxSource>,
    /// What the reader reported.
    warnings: Vec<String>,
    file_name: Option<String>,
}

fn w3cdtf(unix_ms: u64) -> String {
    let secs = i64::try_from(unix_ms / 1000).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

impl GridDoc {
    fn new(
        workbook: Workbook,
        source: Option<XlsxSource>,
        warnings: Vec<String>,
        file_name: Option<String>,
    ) -> Self {
        Self {
            workbook,
            source,
            warnings,
            file_name,
        }
    }

    /// The workbook.
    #[must_use]
    pub fn workbook(&self) -> &Workbook {
        &self.workbook
    }

    /// The reader's diagnostics.
    #[must_use]
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// The file name, if the workbook came from a file.
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.file_name.as_deref()
    }

    fn active_sheet(&self) -> Option<(usize, &kiln_sheet::Sheet)> {
        let i = self.workbook.props().active_sheet;
        self.workbook
            .sheets()
            .get(i)
            .map(|s| (i, s))
            .or_else(|| self.workbook.sheets().first().map(|s| (0, s)))
    }

    fn write(&self, format: FileFormat, now: Option<String>) -> Result<Vec<u8>> {
        let failed = |e: String| KilnError::Format {
            format: format.extension().to_owned(),
            message: e,
        };
        let kind = match format {
            FileFormat::Csv => {
                let (_, sheet) = self
                    .active_sheet()
                    .ok_or_else(|| failed("the workbook has no sheet".into()))?;
                return Ok(write_delimited(
                    &self.workbook,
                    sheet.id(),
                    &ExportOptions::default(),
                ));
            }
            FileFormat::Xlsx => XlsxKind::Workbook,
            FileFormat::Xlsm => XlsxKind::MacroEnabled,
            FileFormat::Xltx => XlsxKind::Template,
            other => {
                return Err(KilnError::Unsupported(format!(
                    "saving Kiln Grid workbooks as .{}",
                    other.extension()
                )));
            }
        };
        write_xlsx(
            &self.workbook,
            self.source.as_ref(),
            &WriteOptions {
                kind: Some(kind),
                now,
                author: None,
            },
        )
        .map(|w| w.bytes)
        .map_err(|e| failed(e.to_string()))
    }
}

fn issue(
    code: &str,
    severity: CompatSeverity,
    message: String,
    count: u32,
    origin: CompatOrigin,
) -> CompatIssue {
    CompatIssue {
        code: code.to_owned(),
        severity,
        message,
        count,
        origin,
    }
}

impl DocModel for GridDoc {
    fn kind(&self) -> DocKind {
        DocKind::Grid
    }

    fn execute(&mut self, command: &Command) -> Result<Change> {
        Err(KilnError::Unsupported(format!(
            "{} in Kiln Grid (the grid editor arrives with WP-71)",
            command.id()
        )))
    }

    fn undo(&mut self) -> bool {
        false
    }

    fn redo(&mut self) -> bool {
        false
    }

    fn can_undo(&self) -> bool {
        false
    }

    fn can_redo(&self) -> bool {
        false
    }

    fn undo_label(&self) -> Option<&'static str> {
        None
    }

    fn redo_label(&self) -> Option<&'static str> {
        None
    }

    fn state(&self) -> StateId {
        0
    }

    fn seal_history(&mut self) {}

    fn selection(&self) -> Selection {
        let Some((index, sheet)) = self.active_sheet() else {
            return Selection::default();
        };
        let sel = &sheet.meta().view.selection;
        let sheet_no = u32::try_from(index).unwrap_or(0);
        let addr = |row, col| CellAddr {
            sheet: sheet_no,
            row,
            col,
        };
        Selection {
            kind: SelKind::Cells {
                ranges: sel
                    .ranges
                    .iter()
                    .map(|r| CellRange {
                        start: addr(r.r0, r.c0),
                        end: addr(r.r1, r.c1),
                    })
                    .collect(),
                active: addr(sel.active.row, sel.active.col),
            },
            ..Selection::default()
        }
    }

    fn status(&self) -> StatusBar {
        let Some((_, sheet)) = self.active_sheet() else {
            return StatusBar::default();
        };
        let id = sheet.id();
        let mut stats = CellStats {
            count: 0,
            numeric_count: 0,
            sum: 0.0,
            average: None,
            min: None,
            max: None,
        };
        for r in &sheet.meta().view.selection.ranges {
            // The status bar sums what is stored (an entire column is no slower than its cells).
            for (row, col, _) in sheet.cells().iter_range(*r) {
                let v = self.workbook.value(id, row, col);
                if v.is_blank() {
                    continue;
                }
                stats.count += 1;
                if let CellValue::Number(n) = v {
                    stats.numeric_count += 1;
                    stats.sum += n;
                    stats.min = Some(stats.min.map_or(n, |m: f64| m.min(n)));
                    stats.max = Some(stats.max.map_or(n, |m: f64| m.max(n)));
                }
            }
        }
        if stats.numeric_count > 0 {
            stats.average = Some(stats.sum / f64::from(stats.numeric_count));
        }
        StatusBar {
            cells: Some(stats),
            ..StatusBar::default()
        }
    }

    fn paragraphs(&self) -> Option<Vec<kiln_protocol::ParagraphText>> {
        None
    }

    fn a11y(&self, title: &str) -> A11yTree {
        let sheets = self.workbook.sheets();
        let mut nodes = Vec::with_capacity(sheets.len() + 1);
        nodes.push(A11yNode {
            id: NodeId(1),
            role: A11yRole::Document,
            name: Some(title.to_owned()),
            text: None,
            bounds: None,
            level: None,
            children: (0..sheets.len() as u64).map(|i| NodeId(i + 2)).collect(),
        });
        for (i, s) in sheets.iter().enumerate() {
            nodes.push(A11yNode {
                id: NodeId(i as u64 + 2),
                role: A11yRole::Grid,
                name: Some(s.name().to_owned()),
                text: None,
                bounds: None,
                level: None,
                children: Vec::new(),
            });
        }
        let active = self.active_sheet().map(|(i, _)| NodeId(i as u64 + 2));
        A11yTree {
            nodes,
            active,
            announce: None,
            caret: None,
        }
    }

    fn save_formats(&self) -> &'static [FileFormat] {
        &FORMATS
    }

    fn save(&self, format: FileFormat) -> Result<Vec<u8>> {
        self.write(format, None)
    }

    fn save_at(&self, format: FileFormat, now_ms: u64) -> Result<Vec<u8>> {
        self.write(format, Some(w3cdtf(now_ms)))
    }

    fn compat_report(&self, format: FileFormat) -> Vec<CompatIssue> {
        let mut out = Vec::new();
        let wb = &self.workbook;
        match format {
            FileFormat::Csv => {
                let others = wb.sheets().len().saturating_sub(1);
                if others > 0 {
                    out.push(issue(
                        "csv.sheets",
                        CompatSeverity::Loss,
                        "Only the active sheet is saved; the other sheets are not".into(),
                        u32::try_from(others).unwrap_or(u32::MAX),
                        CompatOrigin::Save,
                    ));
                }
                let formulas: usize = wb
                    .sheets()
                    .iter()
                    .map(|s| {
                        s.cells()
                            .iter()
                            .filter(|(_, _, d)| d.kind.is_formula())
                            .count()
                    })
                    .sum();
                if formulas > 0 {
                    out.push(issue(
                        "csv.formulas",
                        CompatSeverity::Loss,
                        "Formulas are saved as their results".into(),
                        u32::try_from(formulas).unwrap_or(u32::MAX),
                        CompatOrigin::Save,
                    ));
                }
                out.push(issue(
                    "csv.formatting",
                    CompatSeverity::Loss,
                    "Formatting, merged cells, notes and everything but the displayed values are not saved"
                        .into(),
                    1,
                    CompatOrigin::Save,
                ));
            }
            FileFormat::Xlsx | FileFormat::Xltx
                if self.source.as_ref().is_some_and(|s| s.kind().has_macros()) =>
            {
                out.push(issue(
                    "macros.removed",
                    CompatSeverity::Loss,
                    "Macros are not kept in this file type (save as .xlsm to keep them)".into(),
                    1,
                    CompatOrigin::Save,
                ));
            }
            _ => {}
        }
        if !self.warnings.is_empty() {
            out.push(issue(
                "open.warnings",
                CompatSeverity::Info,
                format!(
                    "Opening the file reported {} problem(s), such as: {}",
                    self.warnings.len(),
                    self.warnings[0]
                ),
                u32::try_from(self.warnings.len()).unwrap_or(u32::MAX),
                CompatOrigin::Open,
            ));
        }
        out
    }
}
