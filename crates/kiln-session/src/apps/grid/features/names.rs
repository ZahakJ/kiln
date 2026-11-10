// SPDX-License-Identifier: AGPL-3.0-or-later
//! Formulas ▸ Defined Names: the Name Manager (new, edit, delete, scope, comment, with each
//! name's value), Define Name and Create from Selection. Use in Formula and Paste Names insert
//! the name in the entry from the chrome.

use kiln_common::{KilnError, Result};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::GridNameInfo;
use kiln_sheet::{Command, Range, SheetId};

use super::super::GridDoc;
use super::super::editor::quote_sheet_name;
use crate::model::Change;

/// A defined name made from label text (`Q1 sales` → `Q1_sales`), as Create from Selection
/// does.
fn name_from_label(label: &str) -> Option<String> {
    let mut out: String = label
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if out.is_empty() {
        return None;
    }
    if out
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_digit() || c == '.')
        || kiln_sheet::address::parse_cell_a1(&out).is_some()
        || matches!(out.to_ascii_uppercase().as_str(), "R" | "C")
    {
        out.insert(0, '_');
    }
    Some(out)
}

impl GridDoc {
    fn scope_id(&self, scope: Option<u32>) -> Result<Option<SheetId>> {
        scope
            .map(|i| {
                self.workbook
                    .sheets()
                    .get(i as usize)
                    .map(kiln_sheet::Sheet::id)
                    .ok_or_else(|| KilnError::InvalidInput(format!("there is no sheet {i}")))
            })
            .transpose()
    }

    pub(crate) fn names_command(&mut self, cmd: &GridCommand) -> Result<Change> {
        match cmd {
            GridCommand::DefineName {
                name,
                refers_to,
                scope,
                comment,
                previous,
            } => {
                let name = name.trim();
                let formula = refers_to.trim().trim_start_matches('=').trim().to_owned();
                if formula.is_empty() {
                    return Err(KilnError::InvalidInput(
                        "enter what the name refers to".into(),
                    ));
                }
                kiln_formula::parser::parse(
                    &format!("={formula}"),
                    &kiln_formula::parser::ParseOptions::default(),
                )
                .map_err(|e| {
                    KilnError::InvalidInput(format!("the reference has a problem: {e}"))
                })?;
                let scope = self.scope_id(*scope)?;
                let mut cmds = Vec::new();
                if let Some((old, old_scope)) = previous {
                    let old_scope = self.scope_id(*old_scope)?;
                    if !old.eq_ignore_ascii_case(name) || old_scope != scope {
                        cmds.push(Command::DeleteName {
                            name: old.clone(),
                            scope: old_scope,
                        });
                    }
                } else if self
                    .workbook
                    .names()
                    .iter()
                    .any(|n| n.name.eq_ignore_ascii_case(name) && n.scope == scope)
                {
                    return Err(KilnError::InvalidInput(format!(
                        "The name \"{name}\" already exists. Enter a unique name."
                    )));
                }
                self.workbook
                    .check_defined_name(name, scope)
                    .map_err(super::super::model_err)?;
                cmds.push(Command::DefineName {
                    name: name.to_owned(),
                    formula,
                    scope,
                    hidden: false,
                    comment: Some(comment.clone()).filter(|c| !c.is_empty()),
                });
                self.exec_all("grid.names.define", cmds)?;
            }
            GridCommand::DeleteName { name, scope } => {
                let scope = self.scope_id(*scope)?;
                self.exec(
                    "grid.names.delete",
                    Command::DeleteName {
                        name: name.clone(),
                        scope,
                    },
                )?;
            }
            GridCommand::CreateNames {
                top,
                left,
                bottom,
                right,
            } => {
                let sid = self.sid()?;
                let r = self.view().sel.ranges.last().copied().ok_or_else(|| {
                    KilnError::InvalidInput("select the labels and values".into())
                })?;
                let r = self.clip_to_data(r);
                let sheet = quote_sheet_name(
                    self.workbook
                        .sheet(sid)
                        .map(kiln_sheet::Sheet::name)
                        .unwrap_or("Sheet1"),
                );
                let (r0, r1) = (
                    r.r0 + u32::from(*top),
                    r.r1.saturating_sub(u32::from(*bottom)),
                );
                let (c0, c1) = (
                    r.c0 + u32::from(*left),
                    r.c1.saturating_sub(u32::from(*right)),
                );
                if r0 > r1 || c0 > c1 {
                    return Err(KilnError::InvalidInput(
                        "the selection needs labels and values".into(),
                    ));
                }
                let mut defs: Vec<(String, Range)> = Vec::new();
                let mut label = |row: u32, col: u32, area: Range| {
                    let text = self.workbook.display_text(sid, row, col);
                    if let Some(n) = name_from_label(&text) {
                        defs.push((n, area));
                    }
                };
                for c in c0..=c1 {
                    if *top {
                        label(r.r0, c, Range::new(r0, c, r1, c));
                    }
                    if *bottom {
                        label(r.r1, c, Range::new(r0, c, r1, c));
                    }
                }
                for row in r0..=r1 {
                    if *left {
                        label(row, r.c0, Range::new(row, c0, row, c1));
                    }
                    if *right {
                        label(row, r.c1, Range::new(row, c0, row, c1));
                    }
                }
                if defs.is_empty() {
                    return Err(KilnError::InvalidInput(
                        "no labels to make names from".into(),
                    ));
                }
                let cmds = defs
                    .into_iter()
                    .filter(|(n, _)| self.workbook.check_defined_name(n, None).is_ok())
                    .map(|(name, area)| Command::DefineName {
                        name,
                        formula: format!(
                            "{sheet}!{}",
                            if area.is_single_cell() {
                                kiln_sheet::address::cell_a1(area.r0, area.c0, true)
                            } else {
                                area.to_a1(true)
                            }
                        ),
                        scope: None,
                        hidden: false,
                        comment: None,
                    })
                    .collect();
                self.exec_all("grid.names.create", cmds)?;
            }
            _ => return Ok(Change::None),
        }
        Ok(Change::Content)
    }

    /// The value a name shows in the Name Manager.
    fn name_value(&self, formula: &str) -> String {
        let f = formula.trim_start_matches('=');
        let (sheet, addr) = match f.rfind('!') {
            Some(i) => (
                Some(f[..i].trim_matches('\'').replace("''", "'")),
                &f[i + 1..],
            ),
            None => (None, f),
        };
        let Some(r) = Range::parse_a1(addr) else {
            return if f.parse::<f64>().is_ok() || f.starts_with('"') {
                f.to_owned()
            } else {
                String::new()
            };
        };
        let sid = match sheet {
            Some(s) => self.workbook.sheet_by_name(&s).map(kiln_sheet::Sheet::id),
            None => self.sid().ok(),
        };
        let Some(sid) = sid else {
            return "#REF!".into();
        };
        let mut items = Vec::new();
        for (i, p) in r.cells().enumerate() {
            if i >= 6 {
                items.push("…".to_owned());
                break;
            }
            let v = self.workbook.value(sid, p.row, p.col);
            let t = self.workbook.display_text(sid, p.row, p.col);
            items.push(match v {
                kiln_sheet::CellValue::Text(_) | kiln_sheet::CellValue::RichText(_) => {
                    format!("\"{t}\"")
                }
                _ => t,
            });
        }
        if r.is_single_cell() {
            items.pop().unwrap_or_default()
        } else {
            format!("{{{}}}", items.join(";"))
        }
    }

    /// The Name Manager's list (tables last).
    pub(crate) fn names_info(&self) -> Vec<GridNameInfo> {
        let index = |s: SheetId| {
            self.workbook
                .sheets()
                .iter()
                .position(|x| x.id() == s)
                .and_then(|i| u32::try_from(i).ok())
        };
        let mut out: Vec<GridNameInfo> = self
            .workbook
            .names()
            .iter()
            .filter(|n| !n.hidden && !n.name.starts_with("_xlnm") && !n.name.starts_with("_xlfn"))
            .map(|n| GridNameInfo {
                name: n.name.clone(),
                refers_to: format!("={}", n.formula),
                scope: n.scope.and_then(index),
                comment: n.comment.clone().unwrap_or_default(),
                value: self.name_value(&n.formula),
                table: false,
            })
            .collect();
        out.sort_by_key(|n| n.name.to_lowercase());
        for s in self.workbook.sheets() {
            for t in s.tables() {
                let data = t.data_range().unwrap_or(t.range);
                let formula = format!("{}!{}", quote_sheet_name(s.name()), data.to_a1(true));
                out.push(GridNameInfo {
                    name: t.name.clone(),
                    refers_to: format!("={formula}"),
                    scope: None,
                    comment: t.comment.clone().unwrap_or_default(),
                    value: self.name_value(&formula),
                    table: true,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::name_from_label;

    #[test]
    fn labels_become_valid_names() {
        assert_eq!(name_from_label("Q1 sales").as_deref(), Some("Q1_sales"));
        assert_eq!(name_from_label("2026").as_deref(), Some("_2026"));
        assert_eq!(name_from_label("A1").as_deref(), Some("_A1"));
        assert_eq!(name_from_label("  "), None);
    }
}
