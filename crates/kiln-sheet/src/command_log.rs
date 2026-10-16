// SPDX-License-Identifier: AGPL-3.0-or-later
//! Undo and redo. Every command applied through the log records its inverse; commands can be
//! grouped into (nestable) transactions that undo as one step, and the history serialises for
//! journals and crash recovery.
//!
//! Redo re-applies the original command (computing a fresh inverse), so undo → redo → undo cycles
//! stay exact even when inverses depend on state such as style indices.

use serde::{Deserialize, Serialize};

use crate::commands::Command;
use crate::error::{ModelError, Result};
use crate::workbook::Workbook;

/// One undo step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    /// What the UI shows ("Undo Typing 'abc' in A1").
    pub label: String,
    /// The command as applied (ids resolved).
    pub forward: Command,
    /// Its inverse.
    pub inverse: Command,
}

#[derive(Debug, Default)]
struct Txn {
    label: String,
    forward: Vec<Command>,
    inverse: Vec<Command>,
}

/// The serialisable history.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct History {
    /// Undo stack, oldest first.
    pub undo: Vec<LogEntry>,
    /// Redo stack, oldest first.
    pub redo: Vec<LogEntry>,
}

/// Undo/redo stacks for a workbook.
#[derive(Debug)]
pub struct CommandLog {
    undo: Vec<LogEntry>,
    redo: Vec<LogEntry>,
    txn: Vec<Txn>,
    limit: usize,
}

impl Default for CommandLog {
    fn default() -> CommandLog {
        CommandLog::new()
    }
}

impl CommandLog {
    /// A log keeping up to 1,000 undo steps (PLAN.md §3.8).
    pub fn new() -> CommandLog {
        CommandLog::with_limit(1000)
    }

    /// A log keeping up to `limit` undo steps (the oldest are dropped).
    pub fn with_limit(limit: usize) -> CommandLog {
        CommandLog {
            undo: Vec::new(),
            redo: Vec::new(),
            txn: Vec::new(),
            limit: limit.max(1),
        }
    }

    /// True when Undo is available.
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() && self.txn.is_empty()
    }

    /// True when Redo is available.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && self.txn.is_empty()
    }

    /// The label Undo would revert.
    pub fn undo_label(&self) -> Option<&str> {
        self.undo.last().map(|e| e.label.as_str())
    }

    /// The label Redo would re-apply.
    pub fn redo_label(&self) -> Option<&str> {
        self.redo.last().map(|e| e.label.as_str())
    }

    /// Number of undo steps.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo steps.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// True while a transaction is open.
    pub fn in_transaction(&self) -> bool {
        !self.txn.is_empty()
    }

    /// Applies a command as one undo step (or as part of the open transaction). The command's
    /// kind is the label.
    pub fn exec(&mut self, wb: &mut Workbook, command: Command) -> Result<()> {
        let label = command.kind().to_owned();
        self.exec_labelled(wb, command, &label)
    }

    /// Applies a command with a label for the Undo menu.
    pub fn exec_labelled(
        &mut self,
        wb: &mut Workbook,
        command: Command,
        label: &str,
    ) -> Result<()> {
        let cmd = wb.resolve(&command);
        let inverse = wb.apply(&cmd)?;
        if let Some(open) = self.txn.last_mut() {
            open.forward.push(cmd);
            open.inverse.push(inverse);
            return Ok(());
        }
        // A command whose inverse does nothing changed nothing: no undo step.
        if inverse.is_noop() {
            return Ok(());
        }
        self.push(LogEntry {
            label: label.to_owned(),
            forward: cmd,
            inverse,
        });
        Ok(())
    }

    fn push(&mut self, entry: LogEntry) {
        self.undo.push(entry);
        if self.undo.len() > self.limit {
            let excess = self.undo.len() - self.limit;
            self.undo.drain(..excess);
        }
        self.redo.clear();
    }

    /// Opens a transaction: commands until the matching [`CommandLog::commit`] become one undo
    /// step. Nestable.
    pub fn begin(&mut self, label: &str) {
        self.txn.push(Txn {
            label: label.to_owned(),
            ..Txn::default()
        });
    }

    /// Closes the innermost transaction.
    pub fn commit(&mut self, wb: &mut Workbook) -> Result<()> {
        let t = self
            .txn
            .pop()
            .ok_or_else(|| ModelError::invalid("no open transaction"))?;
        if t.forward.is_empty() {
            return Ok(());
        }
        let label = t.label.clone();
        let forward = if t.forward.len() == 1 {
            t.forward.into_iter().next().unwrap_or_else(Command::noop)
        } else {
            Command::Batch {
                commands: t.forward,
                label: Some(label.clone()),
            }
        };
        let inverse = if t.inverse.len() == 1 {
            t.inverse.into_iter().next().unwrap_or_else(Command::noop)
        } else {
            Command::Batch {
                commands: t.inverse.into_iter().rev().collect(),
                label: Some(label.clone()),
            }
        };
        if let Some(outer) = self.txn.last_mut() {
            outer.forward.push(forward);
            outer.inverse.push(inverse);
            return Ok(());
        }
        // Recalculate once at the end of the outermost transaction (automatic mode).
        wb.apply(&Command::noop())?;
        self.push(LogEntry {
            label,
            forward,
            inverse,
        });
        Ok(())
    }

    /// Reverts everything applied in the innermost open transaction and closes it.
    pub fn rollback(&mut self, wb: &mut Workbook) -> Result<()> {
        let t = self
            .txn
            .pop()
            .ok_or_else(|| ModelError::invalid("no open transaction"))?;
        let mut result = Ok(());
        for inv in t.inverse.iter().rev() {
            if let Err(e) = wb.apply(inv) {
                result = Err(e);
            }
        }
        result
    }

    /// Runs `f` in a transaction; rolls back and returns the error if it fails.
    pub fn transaction<T>(
        &mut self,
        wb: &mut Workbook,
        label: &str,
        f: impl FnOnce(&mut CommandLog, &mut Workbook) -> Result<T>,
    ) -> Result<T> {
        self.begin(label);
        match f(self, wb) {
            Ok(v) => {
                self.commit(wb)?;
                Ok(v)
            }
            Err(e) => {
                self.rollback(wb)?;
                Err(e)
            }
        }
    }

    /// Undo. `Ok(false)` when there is nothing to undo.
    pub fn undo(&mut self, wb: &mut Workbook) -> Result<bool> {
        if !self.txn.is_empty() {
            return Err(ModelError::invalid("cannot undo during a transaction"));
        }
        let Some(entry) = self.undo.pop() else {
            return Ok(false);
        };
        match wb.apply(&entry.inverse) {
            Ok(_) => {
                self.redo.push(entry);
                Ok(true)
            }
            Err(e) => {
                self.undo.push(entry);
                Err(e)
            }
        }
    }

    /// Redo. `Ok(false)` when there is nothing to redo.
    pub fn redo(&mut self, wb: &mut Workbook) -> Result<bool> {
        if !self.txn.is_empty() {
            return Err(ModelError::invalid("cannot redo during a transaction"));
        }
        let Some(entry) = self.redo.pop() else {
            return Ok(false);
        };
        match wb.apply(&entry.forward) {
            Ok(inverse) => {
                self.undo.push(LogEntry { inverse, ..entry });
                Ok(true)
            }
            Err(e) => {
                self.redo.push(entry);
                Err(e)
            }
        }
    }

    /// Forgets the history.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// The history (for journals and crash recovery).
    pub fn history(&self) -> History {
        History {
            undo: self.undo.clone(),
            redo: self.redo.clone(),
        }
    }

    /// Restores a history (the workbook must be in the matching state).
    pub fn load(&mut self, h: History) {
        self.undo = h.undo;
        self.redo = h.redo;
        self.txn.clear();
    }

    /// The history as JSON.
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self.history()).map_err(|e| ModelError::invalid(e.to_string()))
    }

    /// Restores a history from JSON.
    pub fn load_json(&mut self, text: &str) -> Result<()> {
        let h: History = serde_json::from_str(text)
            .map_err(|e| ModelError::invalid(format!("not a command history: {e}")))?;
        self.load(h);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::CellInput;
    use crate::value::CellValue;

    fn typing(wb: &Workbook, row: u32, v: f64) -> Command {
        Command::SetCells {
            sheet: wb.sheets()[0].id(),
            cells: vec![CellInput::value(row, 0, v)],
        }
    }

    #[test]
    fn undoes_and_redoes_with_labels() {
        let mut wb = Workbook::new();
        let mut log = CommandLog::new();
        let s = wb.sheets()[0].id();
        {
            let c = typing(&wb, 0, 1.0);
            log.exec_labelled(&mut wb, c, "Typing").expect("ok");
        }
        assert_eq!(log.undo_label(), Some("Typing"));
        assert!(log.undo(&mut wb).expect("ok"));
        assert!(wb.value(s, 0, 0).is_blank());
        assert_eq!(log.redo_label(), Some("Typing"));
        assert!(log.redo(&mut wb).expect("ok"));
        assert_eq!(wb.value(s, 0, 0), CellValue::Number(1.0));
        assert!(!log.redo(&mut wb).expect("ok"));
    }

    #[test]
    fn no_op_commands_do_not_create_undo_steps() {
        let mut wb = Workbook::new();
        let mut log = CommandLog::new();
        let s = wb.sheets()[0].id();
        log.exec(
            &mut wb,
            Command::Unmerge {
                sheet: s,
                range: crate::Range::cell(0, 0),
            },
        )
        .expect("ok");
        assert_eq!(log.undo_depth(), 0);
    }

    #[test]
    fn failed_commands_leave_the_history_alone() {
        let mut wb = Workbook::new();
        let mut log = CommandLog::new();
        {
            let c = typing(&wb, 0, 1.0);
            log.exec(&mut wb, c).expect("ok");
        }
        let bad = Command::RemoveSheet {
            sheet: wb.sheets()[0].id(),
        };
        assert!(log.exec(&mut wb, bad).is_err());
        assert_eq!(log.undo_depth(), 1);
        assert!(log.can_undo());
    }

    #[test]
    fn rollback_reverts_an_open_transaction() {
        let mut wb = Workbook::new();
        let mut log = CommandLog::new();
        let s = wb.sheets()[0].id();
        log.begin("Edit");
        {
            let c = typing(&wb, 0, 1.0);
            log.exec(&mut wb, c).expect("ok");
        }
        {
            let c = typing(&wb, 1, 2.0);
            log.exec(&mut wb, c).expect("ok");
        }
        assert!(log.in_transaction());
        assert!(!log.can_undo());
        log.rollback(&mut wb).expect("ok");
        assert!(wb.value(s, 0, 0).is_blank() && wb.value(s, 1, 0).is_blank());
        assert!(log.rollback(&mut wb).is_err());
        assert_eq!(log.undo_depth(), 0);
    }

    #[test]
    fn history_round_trips_through_json() {
        let mut wb = Workbook::new();
        let mut log = CommandLog::new();
        {
            let c = typing(&wb, 0, 1.0);
            log.exec(&mut wb, c).expect("ok");
        }
        {
            let c = typing(&wb, 1, 2.0);
            log.exec(&mut wb, c).expect("ok");
        }
        log.undo(&mut wb).expect("ok");
        let json = log.to_json().expect("json");
        let mut back = CommandLog::new();
        back.load_json(&json).expect("loads");
        assert_eq!(back.history(), log.history());
        assert!(back.load_json("{not json").is_err());
    }
}
