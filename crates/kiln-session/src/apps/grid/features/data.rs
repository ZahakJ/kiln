// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data ▸ Get & Transform (From Text/CSV and From JSON through `kiln-io-csv`, with the import
//! preview), Refresh All, Text to Columns, Remove Duplicates, Group, Ungroup, Show/Hide Detail
//! and outline symbols; Review ▸ Spelling over the text cells (`kiln-spell`).

use kiln_common::{KilnError, Result};
use kiln_io_csv::{Encoding, ImportOptions, read_csv, read_json};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::{
    GridImport, GridImportFormat, GridImportPreview, GridSpellingHit, GridTextToColumns,
};
use kiln_sheet::commands::{CellInput, ColPatch, Content, PasteOp, PasteWhat, RowPatch};
use kiln_sheet::data::ColumnFormat;
use kiln_sheet::{CellValue, Command, Range, SheetId, Workbook};

use super::super::GridDoc;
use crate::model::Change;

fn encoding_of(name: &str) -> Option<Encoding> {
    Some(
        match name
            .to_ascii_lowercase()
            .replace(['-', '_', ' '], "")
            .as_str()
        {
            "utf8" => Encoding::Utf8,
            "utf8bom" | "utf8withbom" => Encoding::Utf8Bom,
            "utf16le" | "utf16" => Encoding::Utf16Le,
            "utf16be" => Encoding::Utf16Be,
            "windows1252" | "cp1252" | "latin1" | "iso88591" => Encoding::Windows1252,
            _ => return None,
        },
    )
}

fn options(imp: &GridImport) -> Result<ImportOptions> {
    let delimiter = match imp.delimiter.as_deref() {
        None | Some("") => None,
        Some("\\t" | "tab") => Some('\t'),
        Some(d) => {
            let mut chars = d.chars();
            let c = chars.next();
            if chars.next().is_some() {
                return Err(KilnError::InvalidInput(
                    "the delimiter is one character".into(),
                ));
            }
            c
        }
    };
    let encoding = match imp.encoding.as_deref() {
        None | Some("") => None,
        Some(e) => Some(
            encoding_of(e)
                .ok_or_else(|| KilnError::InvalidInput(format!("unknown encoding {e}")))?,
        ),
    };
    let stem = imp
        .name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(&imp.name)
        .rsplit_once('.')
        .map_or(imp.name.as_str(), |(s, _)| s)
        .chars()
        .filter(|c| !matches!(c, ':' | '\\' | '/' | '?' | '*' | '[' | ']'))
        .take(31)
        .collect::<String>();
    Ok(ImportOptions {
        delimiter,
        encoding,
        formulas: false,
        as_text: imp.as_text,
        date1904: false,
        sheet_name: (!stem.is_empty()).then_some(stem),
        decimal_comma: imp.decimal_comma,
    })
}

/// Reads an import into a one-sheet workbook with what the dialog shows about it.
fn read(imp: &GridImport) -> Result<(Workbook, String, String, Vec<String>, usize, usize)> {
    let opts = options(imp)?;
    match imp.format {
        GridImportFormat::Text => {
            let r =
                read_csv(&imp.bytes, &opts).map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            let d = r.dialect.delimiter;
            Ok((
                r.workbook,
                r.encoding.label().to_owned(),
                if d == '\t' {
                    "\\t".into()
                } else {
                    d.to_string()
                },
                r.report.warnings,
                r.report.rows,
                r.report.cols,
            ))
        }
        GridImportFormat::Json => {
            let (wb, report) =
                read_json(&imp.bytes, &opts).map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            Ok((
                wb,
                "UTF-8".into(),
                String::new(),
                report.warnings,
                report.rows,
                report.cols,
            ))
        }
    }
}

/// The import dialog's preview (the first 50 rows and columns as displayed).
pub(crate) fn import_preview(imp: &GridImport) -> Result<GridImportPreview> {
    let (wb, encoding, delimiter, warnings, rows, cols) = read(imp)?;
    let mut out = Vec::new();
    if let Some(s) = wb.sheets().first() {
        let sid = s.id();
        for r in 0..rows.min(50) {
            out.push(
                (0..cols.min(50))
                    .map(|c| wb.display_text(sid, r as u32, c as u32))
                    .collect(),
            );
        }
    }
    Ok(GridImportPreview {
        rows: out,
        total_rows: u32::try_from(rows).unwrap_or(u32::MAX),
        total_cols: u32::try_from(cols).unwrap_or(u32::MAX),
        encoding,
        delimiter,
        warnings,
    })
}

impl GridDoc {
    /// Data ▸ Export: the selection (or the sheet) as CSV, TSV or JSON.
    pub(crate) fn export(
        &self,
        format: kiln_protocol::commands::grid_features::GridExportFormat,
        selection: bool,
    ) -> Result<kiln_protocol::commands::grid_features::GridExportFile> {
        use kiln_protocol::commands::grid_features::{GridExportFile, GridExportFormat as F};
        let sid = self.sid()?;
        let range = if selection {
            Some(self.data_range()?)
        } else {
            kiln_io_csv::export::export_range(&self.workbook, sid)
        };
        let stem = self
            .file_name
            .as_deref()
            .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f))
            .and_then(|f| f.rsplit_once('.').map(|(s, _)| s.to_owned()))
            .unwrap_or_else(|| "Book1".to_owned());
        let Some(range) = range else {
            return Err(KilnError::InvalidInput("the sheet is empty".into()));
        };
        let delimited = |dialect: kiln_io_csv::Dialect, bom: bool| {
            kiln_io_csv::write_delimited_range(
                &self.workbook,
                sid,
                range,
                &kiln_io_csv::ExportOptions {
                    dialect,
                    encoding: if bom {
                        Encoding::Utf8Bom
                    } else {
                        Encoding::Utf8
                    },
                    ..kiln_io_csv::ExportOptions::default()
                },
            )
        };
        Ok(match format {
            F::Csv => GridExportFile {
                name: format!("{stem}.csv"),
                mime: "text/csv".into(),
                bytes: delimited(kiln_io_csv::Dialect::default(), true),
            },
            F::Tsv => GridExportFile {
                name: format!("{stem}.tsv"),
                mime: "text/tab-separated-values".into(),
                bytes: delimited(
                    kiln_io_csv::Dialect {
                        delimiter: '\t',
                        ..kiln_io_csv::Dialect::default()
                    },
                    false,
                ),
            },
            F::Json => GridExportFile {
                name: format!("{stem}.json"),
                mime: "application/json".into(),
                bytes: kiln_io_csv::write_json_range(&self.workbook, sid, range),
            },
        })
    }

    pub(crate) fn data_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        let sid = self.sid()?;
        match cmd {
            GridCommand::Import { import } => self.import(import)?,
            GridCommand::RefreshAll => {
                let _ = self.log.recalc(&mut self.workbook, None);
                self.absorb_changes();
                // Filters and remembered sorts follow the new values.
                let ids: Vec<SheetId> = self
                    .workbook
                    .sheets()
                    .iter()
                    .filter(|s| {
                        s.meta().auto_filter.is_some()
                            || s.tables().iter().any(|t| t.auto_filter.is_some())
                    })
                    .map(kiln_sheet::Sheet::id)
                    .collect();
                let cmds: Vec<Command> = ids
                    .into_iter()
                    .map(|sheet| Command::ApplyFilter { sheet })
                    .collect();
                self.exec_all("grid.data.refresh_all", cmds)?;
            }
            GridCommand::TextToColumns { options: o } => self.text_to_columns(sid, o)?,
            GridCommand::RemoveDuplicates { columns, header } => {
                let range = self.data_range()?;
                let header = header.unwrap_or_else(|| {
                    self.workbook
                        .sheet(sid)
                        .and_then(|s| s.table_at(range.r0, range.c0))
                        .is_some_and(|t| t.header_row)
                        || (range.c0..=range.c1).all(|c| {
                            matches!(
                                self.workbook.value(sid, range.r0, c),
                                CellValue::Text(_) | CellValue::RichText(_)
                            )
                        })
                });
                let filled = |doc: &GridDoc| {
                    (range.r0..=range.r1)
                        .filter(|&r| (range.c0..=range.c1).any(|c| doc.has_content(sid, r, c)))
                        .count()
                };
                let before = filled(self);
                self.exec(
                    "grid.data.dedupe",
                    Command::RemoveDuplicates {
                        sheet: sid,
                        range,
                        columns: columns.clone(),
                        header,
                    },
                )?;
                let removed = before.saturating_sub(filled(self));
                let left = filled(self).saturating_sub(usize::from(header));
                self.features.message = Some(if removed == 0 {
                    "No duplicate values found.".into()
                } else {
                    format!(
                        "{removed} duplicate value{} found and removed; {left} unique value{} remain.",
                        if removed == 1 { "" } else { "s" },
                        if left == 1 { "" } else { "s" },
                    )
                });
            }
            GridCommand::Group { columns } => self.outline(sid, *columns, 1)?,
            GridCommand::Ungroup { columns } => self.outline(sid, *columns, -1)?,
            GridCommand::OutlineDetail { show } => self.detail(sid, *show)?,
            GridCommand::OutlineSymbols => {
                self.features.hide_outline = !self.features.hide_outline;
                // The gutters come and go: the headings' sizes change.
                self.geo = None;
                if let Some(scene) = &mut self.scene {
                    scene.invalidate_all();
                }
                return Ok(Change::Selection);
            }
            GridCommand::SpellChange {
                sheet,
                row,
                col,
                offset,
                word,
                with,
                all,
            } => {
                let target = self
                    .workbook
                    .sheets()
                    .get(*sheet as usize)
                    .map(kiln_sheet::Sheet::id)
                    .ok_or_else(|| KilnError::InvalidInput(format!("there is no sheet {sheet}")))?;
                self.features.spell_from = Some((
                    *sheet as usize,
                    *row,
                    *col,
                    offset + with.chars().count() as u32,
                ));
                if word == with {
                    // Ignore Once: Spelling goes on after the word.
                    return Ok(Change::Selection);
                }
                let mut cmds = Vec::new();
                let replace_word = |text: &str| -> String {
                    let chars: Vec<char> = text.chars().collect();
                    let mut out = String::new();
                    let mut last = 0;
                    for w in kiln_spell::words(text) {
                        if chars[w.start..w.end].iter().collect::<String>() == *word {
                            out.extend(&chars[last..w.start]);
                            out.push_str(with);
                            last = w.end;
                        }
                    }
                    out.extend(&chars[last.min(chars.len())..]);
                    out
                };
                if *all {
                    for s in self.workbook.sheets() {
                        let mut cells = Vec::new();
                        for (r, c, d) in s.cells().iter() {
                            if d.kind.is_formula() {
                                continue;
                            }
                            if let CellValue::Text(t) = self.workbook.value(s.id(), r, c) {
                                let next = replace_word(&t);
                                if next != *t {
                                    cells.push(CellInput {
                                        row: r,
                                        col: c,
                                        content: Content::Value(CellValue::text(next.as_str())),
                                        style: None,
                                    });
                                }
                            }
                        }
                        if !cells.is_empty() {
                            cmds.push(Command::SetCells {
                                sheet: s.id(),
                                cells,
                            });
                        }
                    }
                } else if let CellValue::Text(t) = self.workbook.value(target, *row, *col) {
                    let chars: Vec<char> = t.chars().collect();
                    let at = *offset as usize;
                    let n = word.chars().count();
                    if chars
                        .get(at..at + n)
                        .is_some_and(|w| w.iter().collect::<String>() == *word)
                    {
                        let next: String = chars[..at]
                            .iter()
                            .copied()
                            .chain(with.chars())
                            .chain(chars[at + n..].iter().copied())
                            .collect();
                        cmds.push(Command::SetCells {
                            sheet: target,
                            cells: vec![CellInput {
                                row: *row,
                                col: *col,
                                content: Content::Value(CellValue::text(next.as_str())),
                                style: None,
                            }],
                        });
                    }
                }
                self.exec_all("grid.review.spell_change", cmds)?;
            }
            GridCommand::SpellIgnore { word, add } => {
                let speller = self
                    .features
                    .speller
                    .get_or_insert_with(kiln_spell::Speller::new);
                if *add {
                    speller
                        .add_user_word(word)
                        .map_err(|e| KilnError::InvalidInput(format!("{e:?}")))?;
                } else {
                    speller.ignore_all(word);
                }
                return Ok(Change::Selection);
            }
            GridCommand::LoadDictionary { aff, dic } => {
                kiln_spell::load_dictionary(aff, dic).map_err(KilnError::InvalidInput)?;
                return Ok(Change::Selection);
            }
            _ => return Ok(Change::None),
        }
        Ok(Change::Content)
    }

    fn import(&mut self, imp: &GridImport) -> Result<()> {
        let (src, _, _, warnings, rows, cols) = read(imp)?;
        if rows == 0 || cols == 0 {
            return Err(KilnError::InvalidInput("the file holds no data".into()));
        }
        let Some(ssid) = src.first_sheet() else {
            return Err(KilnError::InvalidInput("the file holds no data".into()));
        };
        let area = Range::new(0, 0, (rows - 1) as u32, (cols - 1) as u32);
        let mut clip = src
            .copy(ssid, area, false)
            .map_err(super::super::model_err)?;
        clip.source = None;
        let name = src
            .sheets()
            .first()
            .map(|s| s.name().to_owned())
            .unwrap_or_else(|| "Data".into());
        let mut cmds = Vec::new();
        let (sheet, at, new_sheet) = if imp.at_cell {
            let a = self.view().sel.active;
            (self.sid()?, Range::cell(a.row, a.col), None)
        } else {
            // A new sheet after the active one, named after the file.
            let mut unique = name.clone();
            let mut n = 2;
            while self.workbook.sheet_by_name(&unique).is_some() {
                let suffix = format!(" ({n})");
                unique = format!(
                    "{}{suffix}",
                    name.chars().take(31 - suffix.len()).collect::<String>()
                );
                n += 1;
            }
            cmds.push(Command::AddSheet {
                name: Some(unique.clone()),
                index: Some(self.active + 1),
                id: None,
            });
            (SheetId(0), Range::cell(0, 0), Some(unique))
        };
        // The new sheet exists only once the batch runs; paste into it then.
        if imp.at_cell {
            let paste = self
                .workbook
                .paste_command(
                    sheet,
                    at,
                    &clip,
                    PasteWhat::All,
                    PasteOp::None,
                    false,
                    false,
                )
                .map_err(super::super::model_err)?;
            cmds.push(paste);
            self.exec_all("grid.data.import", cmds)?;
        } else {
            let label = "grid.data.import";
            let pushed = self.log.pushed();
            let new_name = new_sheet.unwrap_or_default();
            let result = self.log.transaction(&mut self.workbook, label, |log, wb| {
                for c in cmds {
                    log.exec_labelled(wb, c, label)?;
                }
                let sheet = wb
                    .sheet_by_name(&new_name)
                    .map(kiln_sheet::Sheet::id)
                    .ok_or_else(|| kiln_sheet::ModelError::invalid("the new sheet is missing"))?;
                let paste = wb.paste_command(
                    sheet,
                    at,
                    &clip,
                    PasteWhat::All,
                    PasteOp::None,
                    false,
                    false,
                )?;
                log.exec_labelled(wb, paste, label)?;
                Ok(sheet)
            });
            let recorded = self.log.pushed() != pushed;
            self.after_change(label, recorded);
            let sheet = result.map_err(super::super::model_err)?;
            if let Some(i) = self.workbook.sheet_index(sheet) {
                self.activate(i);
            }
        }
        self.features.message = Some(match warnings.first() {
            Some(w) => format!("Imported {rows} rows and {cols} columns. {w}"),
            None => format!("Imported {rows} rows and {cols} columns."),
        });
        Ok(())
    }

    fn text_to_columns(&mut self, sid: SheetId, o: &GridTextToColumns) -> Result<()> {
        let sel = self
            .view()
            .sel
            .ranges
            .last()
            .copied()
            .ok_or_else(|| KilnError::InvalidInput("select a column of text first".into()))?;
        let sel = self.clip_to_data(sel);
        if sel.c0 != sel.c1 {
            return Err(KilnError::InvalidInput(
                "Text to Columns works on one column at a time.".into(),
            ));
        }
        if o.delimited && o.delimiters.is_empty() {
            return Err(KilnError::InvalidInput(
                "choose at least one delimiter".into(),
            ));
        }
        let width = o
            .text_columns
            .iter()
            .chain(&o.skip_columns)
            .copied()
            .max()
            .map_or(0, |m| m + 1);
        let formats: Vec<ColumnFormat> = (0..width)
            .map(|i| {
                if o.skip_columns.contains(&i) {
                    ColumnFormat::Skip
                } else if o.text_columns.contains(&i) {
                    ColumnFormat::Text
                } else {
                    ColumnFormat::General
                }
            })
            .collect();
        let options = kiln_sheet::commands::TextToColumns {
            delimited: o.delimited,
            delimiters: o.delimiters.replace("\\t", "\t").chars().collect(),
            merge_delimiters: o.consecutive,
            qualifier: o.qualifier.chars().next(),
            breaks: o.widths.iter().map(|w| *w as usize).collect(),
            formats,
            destination: None,
        };
        self.exec(
            "grid.data.split_column",
            Command::TextToColumns {
                sheet: sid,
                range: sel,
                options,
            },
        )?;
        Ok(())
    }

    /// Group (`delta` 1) or Ungroup (−1) the selected rows or columns.
    fn outline(&mut self, sid: SheetId, columns: bool, delta: i8) -> Result<()> {
        let sel = self
            .view()
            .sel
            .ranges
            .last()
            .copied()
            .ok_or_else(|| KilnError::InvalidInput("select rows or columns first".into()))?;
        let sheet = self
            .workbook
            .sheet(sid)
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        let (lo, hi) = if columns {
            (sel.c0, sel.c1)
        } else {
            (sel.r0, sel.r1)
        };
        // A whole-column selection groups the rows down to the data, not a million rows.
        let hi = if columns || hi + 1 < kiln_sheet::MAX_ROWS {
            hi
        } else {
            hi.min(sheet.cells().used_range(false).map_or(hi, |u| u.r1.max(lo)))
        };
        let level = |i: u32| {
            if columns {
                sheet.cols().outline_level(i)
            } else {
                sheet.rows().outline_level(i)
            }
        };
        let mut cmds = Vec::new();
        let mut i = lo;
        while i <= hi {
            let l = level(i);
            let mut j = i;
            while j < hi && level(j + 1) == l {
                j += 1;
            }
            let next = i16::from(l) + i16::from(delta);
            if (0..=7).contains(&next) && next != i16::from(l) {
                let next = u8::try_from(next).unwrap_or(0);
                cmds.push(if columns {
                    Command::SetColProps {
                        sheet: sid,
                        at: i,
                        count: j - i + 1,
                        patch: ColPatch {
                            outline_level: Some(next),
                            ..ColPatch::default()
                        },
                    }
                } else {
                    Command::SetRowProps {
                        sheet: sid,
                        at: i,
                        count: j - i + 1,
                        patch: RowPatch {
                            outline_level: Some(next),
                            hidden: (next == 0).then_some(false),
                            ..RowPatch::default()
                        },
                    }
                });
            }
            j += 1;
            i = j;
        }
        if cmds.is_empty() {
            return Err(KilnError::InvalidInput(if delta > 0 {
                "the rows are grouped eight levels deep already".into()
            } else {
                "the selection is not grouped".into()
            }));
        }
        self.exec_all(
            if delta > 0 {
                "grid.data.group"
            } else {
                "grid.data.ungroup"
            },
            cmds,
        )?;
        Ok(())
    }

    /// Show Detail and Hide Detail of the row group at the active cell (rows, then columns).
    fn detail(&mut self, sid: SheetId, show: bool) -> Result<()> {
        let a = self.view().sel.active;
        let sheet = self
            .workbook
            .sheet(sid)
            .ok_or_else(|| KilnError::InvalidInput("no sheet".into()))?;
        let rows = sheet.rows();
        let below = |r: u32| r + 1 < kiln_sheet::MAX_ROWS;
        // The group: the run of rows around the active row with at least its level; Show
        // Detail from the summary row below opens the group above it.
        let mut at = a.row;
        if rows.outline_level(at) == 0 && at > 0 && rows.outline_level(at - 1) > 0 {
            at -= 1;
        }
        let level = rows.outline_level(at);
        if level == 0 {
            return Err(KilnError::InvalidInput(
                "the active cell is not in a group".into(),
            ));
        }
        let mut r0 = at;
        while r0 > 0 && rows.outline_level(r0 - 1) >= level {
            r0 -= 1;
        }
        let mut r1 = at;
        while below(r1) && rows.outline_level(r1 + 1) >= level {
            r1 += 1;
        }
        let mut cmds = vec![Command::SetRowProps {
            sheet: sid,
            at: r0,
            count: r1 - r0 + 1,
            patch: RowPatch {
                hidden: Some(!show),
                ..RowPatch::default()
            },
        }];
        if below(r1) {
            cmds.push(Command::SetRowProps {
                sheet: sid,
                at: r1 + 1,
                count: 1,
                patch: RowPatch {
                    collapsed: Some(!show),
                    ..RowPatch::default()
                },
            });
        }
        self.exec_all("grid.data.outline_detail", cmds)?;
        if !show {
            self.select_cell((r1 + 1).min(kiln_sheet::MAX_ROWS - 1), a.col, false, false);
        }
        Ok(())
    }

    /// The next misspelled word from where Spelling stands (the active cell at first), on the
    /// active sheet, wrapping once.
    pub(crate) fn next_misspelling(&self) -> Option<GridSpellingHit> {
        let fallback;
        let speller = match &self.features.speller {
            Some(s) => s,
            None => {
                fallback = kiln_spell::Speller::new();
                &fallback
            }
        };
        if !kiln_spell::has_dictionary() {
            return None;
        }
        let (index, sheet) = self.active_sheet()?;
        let a = self.view().sel.active;
        let (from_r, from_c, from_off) = match self.features.spell_from {
            Some((i, r, c, o)) if i == index => (r, c, o),
            _ => (a.row, a.col, 0),
        };
        let mut cells: Vec<(u32, u32)> = sheet
            .cells()
            .iter()
            .filter(|(_, _, d)| !d.kind.is_formula())
            .map(|(r, c, _)| (r, c))
            .collect();
        cells.sort_unstable();
        let start = cells.partition_point(|&p| p < (from_r, from_c));
        let order = cells[start..].iter().chain(cells[..start].iter());
        for &(r, c) in order {
            let CellValue::Text(t) = self.workbook.value(sheet.id(), r, c) else {
                continue;
            };
            for m in speller.misspellings(&t) {
                if (r, c) == (from_r, from_c) && (m.start as u32) < from_off {
                    continue;
                }
                return Some(GridSpellingHit {
                    sheet: u32::try_from(index).unwrap_or(0),
                    row: r,
                    col: c,
                    suggestions: speller.suggest(&m.word).into_iter().take(8).collect(),
                    word: m.word,
                    offset: m.start as u32,
                    text: t.to_string(),
                });
            }
        }
        None
    }
}
