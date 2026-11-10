// SPDX-License-Identifier: AGPL-3.0-or-later
//! new sheets follow the language of the workbook's default sheet names.

use kiln_sheet::{Command, Workbook, WorkbookOptions, default_sheet_stem, sheet_stem_for};

fn add(wb: &mut Workbook) -> String {
    let cmd = wb.resolve(&Command::AddSheet {
        name: None,
        index: None,
        id: None,
    });
    wb.apply(&cmd).expect("add a sheet");
    wb.sheet_names().last().cloned().unwrap_or_default()
}

#[test]
fn new_sheets_are_named_in_the_workbooks_language() {
    assert_eq!(sheet_stem_for("de"), "Tabelle");
    assert_eq!(sheet_stem_for("AR"), "ورقة");
    assert_eq!(sheet_stem_for("fr"), "Sheet");
    assert_eq!(
        default_sheet_stem(["Budget", "TABELLE3"].into_iter()),
        "Tabelle"
    );
    assert_eq!(default_sheet_stem(["Tabellen"].into_iter()), "Sheet");

    let mut en = Workbook::new();
    assert_eq!(en.next_sheet_name(), "Sheet2");
    assert_eq!(add(&mut en), "Sheet2");

    for (first, second) in [("Tabelle1", "Tabelle2"), ("ورقة1", "ورقة2")] {
        let mut wb = Workbook::with_options(WorkbookOptions {
            sheets: Some(vec![first.to_owned()]),
            ..WorkbookOptions::default()
        });
        assert_eq!(wb.next_sheet_name(), second);
        assert_eq!(add(&mut wb), second);
    }
}
