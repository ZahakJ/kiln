// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid is not collaborative yet: its command log is what a later Loro
//! bridge will replay. It must serialise and come back able to undo and redo.

use kiln_sheet::command_log::History;
use kiln_sheet::commands::CellInput;
use kiln_sheet::workbook::Workbook;
use kiln_sheet::{CellValue, Command, CommandLog};

#[test]
fn the_grid_command_log_serialises_and_replays() -> Result<(), Box<dyn std::error::Error>> {
    let mut wb = Workbook::new();
    let mut log = CommandLog::new();
    let sheet = wb.sheets()[0].id();
    for (row, v) in [(0, 1.0), (1, 2.0), (2, 3.0)] {
        let c = Command::SetCells {
            sheet,
            cells: vec![CellInput::value(row, 0, v)],
        };
        log.exec_labelled(&mut wb, c, "Typing")?;
    }
    log.undo(&mut wb)?;
    let history = log.history();
    assert_eq!((history.undo.len(), history.redo.len()), (2, 1));

    // JSON (the journal) and postcard (the compact form a bridge would send) both round-trip.
    let json = log.to_json()?;
    let bytes = postcard::to_allocvec(&history)?;
    let back: History = postcard::from_bytes(&bytes)?;
    assert_eq!(back, history);

    let mut restored = CommandLog::new();
    restored.load_json(&json)?;
    assert_eq!(restored.history(), history);
    assert!(restored.redo(&mut wb)?);
    assert_eq!(wb.value(sheet, 2, 0), CellValue::Number(3.0));
    assert!(restored.undo(&mut wb)?);
    assert!(restored.undo(&mut wb)?);
    assert!(wb.value(sheet, 1, 0).is_blank());
    Ok(())
}
