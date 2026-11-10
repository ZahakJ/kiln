// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data ▸ Subtotal: a `SUBTOTAL` row after (or before) each group of rows that share a value in
//! one column, a grand total, and the three-level outline Excel builds (grand total, subtotals,
//! details); Remove All takes the rows and the outline away again. The list is the current
//! region at the active cell with its header row; one undo step.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::grid_analysis::{GridPivotFunction, GridSubtotalSpec};
use kiln_sheet::commands::{CellInput, Content, FormatFrom, MetaField, RowPatch};
use kiln_sheet::styles::{FontPatch, StylePatch};
use kiln_sheet::{CellValue, Command, Range, SheetId};

use super::super::GridDoc;
use crate::model::Change;

/// `SUBTOTAL`'s function number and Excel's caption for a function.
pub(crate) fn subtotal_function(f: GridPivotFunction) -> (u8, &'static str) {
    use GridPivotFunction as G;
    match f {
        G::Average => (1, "Average"),
        G::CountNums => (2, "Count"),
        G::Count => (3, "Count"),
        G::Max => (4, "Max"),
        G::Min => (5, "Min"),
        G::Product => (6, "Product"),
        G::StdDev => (7, "StdDev"),
        G::StdDevP => (8, "StdDevp"),
        G::Sum => (9, "Total"),
        G::Var => (10, "Var"),
        G::VarP => (11, "Varp"),
    }
}

fn bad(s: &str) -> KilnError {
    KilnError::InvalidInput(s.to_owned())
}

/// Whether a formula is one Subtotal wrote.
fn is_subtotal(f: &str) -> bool {
    f.trim_start_matches('=')
        .trim_start()
        .to_ascii_uppercase()
        .starts_with("SUBTOTAL(")
}

impl GridDoc {
    /// The rows of a list that hold `SUBTOTAL` formulas.
    fn subtotal_rows(&self, sid: SheetId, r: Range) -> Vec<u32> {
        let Some(sheet) = self.workbook.sheet(sid) else {
            return Vec::new();
        };
        let mut rows: Vec<u32> = sheet
            .cells()
            .iter_range(r)
            .filter(|(_, _, d)| d.kind.is_formula())
            .filter(|(row, col, _)| {
                self.workbook
                    .formula(sid, *row, *col)
                    .is_some_and(|f| is_subtotal(&f))
            })
            .map(|(row, _, _)| row)
            .collect();
        rows.sort_unstable();
        rows.dedup();
        rows
    }

    /// Remove All's commands: the subtotal rows deleted (bottom up), the outline and the page
    /// breaks of the list cleared. Returns them with the list's rows after the deletions.
    fn remove_subtotal_commands(&self, sid: SheetId, r: Range) -> (Vec<Command>, Range) {
        let rows = self.subtotal_rows(sid, Range::new(r.r0 + 1, r.c0, r.r1, r.c1));
        let mut cmds: Vec<Command> = rows
            .iter()
            .rev()
            .map(|&at| Command::DeleteRows {
                sheet: sid,
                at,
                count: 1,
            })
            .collect();
        let left = r.r1 - u32::try_from(rows.len()).unwrap_or(0);
        let r = Range::new(r.r0, r.c0, left.max(r.r0), r.c1);
        cmds.push(Command::SetRowProps {
            sheet: sid,
            at: r.r0,
            count: r.height(),
            patch: RowPatch {
                outline_level: Some(0),
                hidden: Some(false),
                collapsed: Some(false),
                ..RowPatch::default()
            },
        });
        if let Some(s) = self.workbook.sheet(sid) {
            let mut print = s.meta().print.clone();
            let before = print.row_breaks.len();
            print.row_breaks.retain(|b| !(r.r0 < *b && *b <= r.r1 + 1));
            if print.row_breaks.len() != before {
                cmds.push(Command::SetSheetMeta {
                    sheet: sid,
                    field: Box::new(MetaField::Print(print)),
                });
            }
        }
        (cmds, r)
    }

    /// Data ▸ Subtotal (`None`: Remove All).
    pub(crate) fn subtotal(&mut self, spec: Option<&GridSubtotalSpec>) -> Result<Change> {
        let sid = self.sid()?;
        let list = self.data_range()?;
        if list.height() < 2 {
            return Err(bad(
                "Kiln Grid cannot determine which row in your list or selection contains column \
                 labels: select the list with its header row.",
            ));
        }
        let Some(spec) = spec else {
            let (cmds, _) = self.remove_subtotal_commands(sid, list);
            self.exec_all("grid.data.apply_subtotal", cmds)?;
            return Ok(Change::Content);
        };
        if !(list.c0..=list.c1).contains(&spec.at_change_in) {
            return Err(bad("The At each change in column is not in the list."));
        }
        if spec.columns.is_empty()
            || spec
                .columns
                .iter()
                .any(|c| !(list.c0..=list.c1).contains(c))
        {
            return Err(bad("Choose the columns to add subtotals to."));
        }
        let (fnum, caption) = subtotal_function(spec.function);
        let grand = if caption == "Total" {
            "Grand Total".to_owned()
        } else {
            format!("Grand {caption}")
        };
        let label = "grid.data.apply_subtotal";
        self.exec_group(label, |doc| {
            let mut list = list;
            if spec.replace {
                let (cmds, l) = doc.remove_subtotal_commands(sid, list);
                doc.exec_all(label, cmds)?;
                list = l;
            }
            // The groups of the data rows by the key column (case-insensitive, as Excel).
            let key = |doc: &GridDoc, row: u32| -> String {
                doc.workbook
                    .value(sid, row, spec.at_change_in)
                    .display_general()
                    .to_lowercase()
            };
            let data0 = list.r0 + 1;
            let mut groups: Vec<(u32, u32)> = Vec::new();
            let mut start = data0;
            for row in data0..=list.r1 {
                if row == list.r1 || key(doc, row + 1) != key(doc, row) {
                    groups.push((start, row));
                    start = row + 1;
                }
            }
            if groups.is_empty() {
                return Err(bad("The list has no data rows."));
            }
            let n = u32::try_from(groups.len()).map_err(|_| bad("too many groups"))?;
            // Insert the subtotal rows bottom up, then the grand total.
            let below = spec.below;
            let mut cmds = Vec::new();
            for &(a, b) in groups.iter().rev() {
                cmds.push(Command::InsertRows {
                    sheet: sid,
                    at: if below { b + 1 } else { a },
                    count: 1,
                    format_from: FormatFrom::default(),
                });
            }
            cmds.push(Command::InsertRows {
                sheet: sid,
                at: if below { list.r1 + n + 1 } else { data0 },
                count: 1,
                format_from: FormatFrom::default(),
            });
            // Where everything lands: summary below, group k moves down by k rows and its
            // subtotal follows it; above, a grand total row and k + 1 subtotal rows come first.
            let mut cells = Vec::new();
            let mut total_rows = Vec::new();
            let mut details: Vec<(u32, u32)> = Vec::new();
            let mut breaks = Vec::new();
            for (k, &(a, b)) in groups.iter().enumerate() {
                let k = u32::try_from(k).unwrap_or(0);
                let (d0, d1, srow) = if below {
                    (a + k, b + k, b + k + 1)
                } else {
                    (a + k + 2, b + k + 2, a + k + 1)
                };
                details.push((d0, d1));
                total_rows.push(srow);
                let name = doc
                    .workbook
                    .value(sid, a, spec.at_change_in)
                    .display_general();
                cells.push(CellInput::value(
                    srow,
                    spec.at_change_in,
                    CellValue::text(format!("{name} {caption}")),
                ));
                for &c in &spec.columns {
                    let r = Range::new(d0, c, d1, c).to_a1(false);
                    cells.push(CellInput {
                        row: srow,
                        col: c,
                        content: Content::Formula(format!("SUBTOTAL({fnum},{r})")),
                        style: None,
                    });
                }
                if spec.page_breaks && k + 1 < n {
                    breaks.push(if below { srow + 1 } else { d1 + 1 });
                }
            }
            let last = list.r1 + n + 1;
            let grand_row = if below { last } else { data0 };
            let all = if below {
                (data0, last - 1)
            } else {
                (data0 + 1, last)
            };
            cells.push(CellInput::value(
                grand_row,
                spec.at_change_in,
                CellValue::text(grand.clone()),
            ));
            for &c in &spec.columns {
                let r = Range::new(all.0, c, all.1, c).to_a1(false);
                cells.push(CellInput {
                    row: grand_row,
                    col: c,
                    content: Content::Formula(format!("SUBTOTAL({fnum},{r})")),
                    style: None,
                });
            }
            cmds.push(Command::SetCells { sheet: sid, cells });
            // The totals in bold.
            let mut bold: Vec<Range> = total_rows
                .iter()
                .map(|&r| Range::new(r, list.c0, r, list.c1))
                .collect();
            bold.push(Range::new(grand_row, list.c0, grand_row, list.c1));
            cmds.push(Command::SetStyle {
                sheet: sid,
                ranges: bold,
                patch: Box::new(StylePatch {
                    font: Some(FontPatch {
                        bold: Some(true),
                        ..FontPatch::default()
                    }),
                    ..StylePatch::default()
                }),
            });
            // The outline: details at level 2, subtotals at level 1.
            for &(d0, d1) in &details {
                cmds.push(Command::SetRowProps {
                    sheet: sid,
                    at: d0,
                    count: d1 - d0 + 1,
                    patch: RowPatch {
                        outline_level: Some(2),
                        ..RowPatch::default()
                    },
                });
            }
            for &r in &total_rows {
                cmds.push(Command::SetRowProps {
                    sheet: sid,
                    at: r,
                    count: 1,
                    patch: RowPatch {
                        outline_level: Some(1),
                        ..RowPatch::default()
                    },
                });
            }
            if let Some(s) = doc.workbook.sheet(sid) {
                let m = s.meta();
                if m.summary_below != below {
                    cmds.push(Command::SetSheetMeta {
                        sheet: sid,
                        field: Box::new(MetaField::Outline {
                            summary_below: below,
                            summary_right: m.summary_right,
                        }),
                    });
                }
                if !breaks.is_empty() {
                    let mut print = m.print.clone();
                    print.row_breaks.extend(breaks);
                    print.row_breaks.sort_unstable();
                    print.row_breaks.dedup();
                    cmds.push(Command::SetSheetMeta {
                        sheet: sid,
                        field: Box::new(MetaField::Print(print)),
                    });
                }
            }
            doc.exec_all(label, cmds)?;
            Ok(())
        })?;
        Ok(Change::Content)
    }
}
