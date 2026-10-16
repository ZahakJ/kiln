// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared helpers for the kiln-sheet integration tests.
#![allow(dead_code)]

use std::sync::Arc;

use kiln_sheet::address::parse_cell_a1;
use kiln_sheet::commands::{CellInput, Content};
use kiln_sheet::{CellValue, Command, CommandLog, SheetId, Workbook, WorkbookOptions};

/// A workbook with a fixed clock (2026-10-15 12:00), its command log and the sheet ids.
pub struct Fixture {
    pub wb: Workbook,
    pub log: CommandLog,
    pub ids: Vec<SheetId>,
}

/// Serial of 2026-10-15 12:00.
pub const NOW: f64 = 46_310.5;

pub fn setup(sheets: &[&str]) -> Fixture {
    let wb = Workbook::with_options(WorkbookOptions {
        sheets: Some(sheets.iter().map(|s| (*s).to_owned()).collect()),
        clock: Some(Arc::new(|| NOW)),
        ..WorkbookOptions::default()
    });
    let ids = wb.sheets().iter().map(|s| s.id()).collect();
    Fixture {
        wb,
        log: CommandLog::new(),
        ids,
    }
}

pub fn pos(a1: &str) -> (u32, u32) {
    let p = parse_cell_a1(a1).unwrap_or_else(|| panic!("bad address {a1}"));
    (p.row, p.col)
}

impl Fixture {
    pub fn id(&self, i: usize) -> SheetId {
        self.ids[i]
    }

    pub fn exec(&mut self, cmd: Command) {
        if let Err(e) = self.log.exec(&mut self.wb, cmd) {
            panic!("command failed: {e}");
        }
    }

    pub fn try_exec(&mut self, cmd: Command) -> kiln_sheet::Result<()> {
        self.log.exec(&mut self.wb, cmd)
    }

    /// Types into a cell: `=…` is a formula, anything else a value.
    pub fn set(&mut self, sheet: SheetId, a1: &str, v: impl Into<Input>) {
        let (row, col) = pos(a1);
        let content = match v.into() {
            Input::Formula(f) => Content::Formula(f),
            Input::Value(v) => Content::Value(v),
            Input::Clear => Content::Clear,
        };
        self.exec(Command::SetCells {
            sheet,
            cells: vec![CellInput {
                row,
                col,
                content,
                style: None,
            }],
        });
    }

    pub fn val(&self, sheet: SheetId, a1: &str) -> CellValue {
        let (r, c) = pos(a1);
        self.wb.value(sheet, r, c)
    }

    pub fn num(&self, sheet: SheetId, a1: &str) -> f64 {
        match self.val(sheet, a1) {
            CellValue::Number(n) => n,
            other => panic!("{a1} is {other:?}, not a number"),
        }
    }

    pub fn err(&self, sheet: SheetId, a1: &str) -> &'static str {
        match self.val(sheet, a1) {
            CellValue::Error(e) => e.code(),
            other => panic!("{a1} is {other:?}, not an error"),
        }
    }

    pub fn formula(&self, sheet: SheetId, a1: &str) -> Option<String> {
        let (r, c) = pos(a1);
        self.wb.formula(sheet, r, c)
    }

    pub fn undo(&mut self) {
        assert!(
            self.log.undo(&mut self.wb).expect("undo works"),
            "nothing to undo"
        );
    }

    pub fn redo(&mut self) {
        assert!(
            self.log.redo(&mut self.wb).expect("redo works"),
            "nothing to redo"
        );
    }
}

pub enum Input {
    Formula(String),
    Value(CellValue),
    Clear,
}

impl From<&str> for Input {
    fn from(s: &str) -> Input {
        match s.strip_prefix('=') {
            Some(f) => Input::Formula(f.to_owned()),
            None => Input::Value(CellValue::text(s)),
        }
    }
}

impl From<f64> for Input {
    fn from(n: f64) -> Input {
        Input::Value(CellValue::Number(n))
    }
}

impl From<i32> for Input {
    fn from(n: i32) -> Input {
        Input::Value(CellValue::Number(f64::from(n)))
    }
}

impl From<bool> for Input {
    fn from(b: bool) -> Input {
        Input::Value(CellValue::Bool(b))
    }
}

impl From<()> for Input {
    fn from(_: ()) -> Input {
        Input::Clear
    }
}

/// Every formula cell of the model is in the engine with the same text, and the engine knows
/// exactly the model's sheets.
pub fn assert_engine_in_sync(wb: &Workbook) {
    let mut count = 0;
    for s in wb.sheets() {
        assert_eq!(wb.engine().sheet_name(s.id()), Some(s.name()));
        for (r, c, d) in s.cells().iter() {
            if d.kind.is_engine_formula() {
                count += 1;
                let model = wb.formula(s.id(), r, c);
                assert!(
                    model.is_some(),
                    "{}!R{}C{} has no formula text",
                    s.name(),
                    r + 1,
                    c + 1
                );
                assert!(wb.engine().has_formula(s.id(), r, c));
            }
        }
    }
    assert_eq!(wb.engine().formula_count(), count, "engine formula count");
    assert_eq!(wb.engine().sheets(), wb.sheet_names());
}

/// The state that must round-trip through undo (style-table growth is allowed).
pub fn state(wb: &Workbook) -> String {
    let mut snap = wb.snapshot();
    snap.styles = kiln_sheet::StyleTable::default();
    snap.counters = (0, 0, 0);
    // Cached values of formulas may differ before a recalculation; formulas and literals not.
    for s in &mut snap.sheets {
        for c in &mut s.cells {
            if c.formula.is_some() {
                c.value = CellValue::Blank;
            }
        }
    }
    format!("{snap:?}")
}
