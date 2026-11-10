// SPDX-License-Identifier: AGPL-3.0-or-later
//! Property tests for commands and the command log. For random command sequences:
//! - every command's inverse restores the state before it exactly, and re-applying restores the
//!   state after it (op/inverse identity);
//! - a command that fails changes nothing;
//! - the engine always holds exactly the model's formulas;
//! - undoing everything restores the initial state, redoing everything the final one;
//! - computed values equal those of a workbook rebuilt from scratch.

mod support;

use kiln_formula::values::ErrorValue;
use kiln_sheet::commands::{
    CellInput, ClearWhat, ColPatch, Content, FormatFrom, MergeMode, MetaField, PasteWhat, RowPatch,
    SortKey, TablePatch,
};
use kiln_sheet::meta::{
    CfKind, CfRule, ConditionalFormat, DataValidation, Hyperlink, Note, Table, TableColumn,
    ValidationType,
};
use kiln_sheet::num::F64;
use kiln_sheet::styles::{AlignmentPatch, Fill, FontPatch, HAlign, Patch, StylePatch};
use kiln_sheet::{
    CellPos, CellValue, Color, Command, CommandLog, Range, Workbook, WorkbookOptions,
};
use proptest::prelude::*;
use support::{assert_engine_in_sync, state};

const N: u32 = 12;

const FORMULAS: [&str; 16] = [
    "=A1+B2",
    "=SUM(A1:C5)",
    "=A1*2",
    "=COUNT(A:A)",
    "=SUM($B$2:B4)",
    "=IFERROR(C3/D4,0)",
    "=Rate*3",
    "=LEN(B1)",
    "=MAX(A1:L12)",
    "=Two!A1+1",
    "=SUM(Two!B1:B5)",
    "=SEQUENCE(2,2)",
    "=SUM(Tab[Val])",
    "=ROWS(Tab[#All])",
    "=INDEX(A1:C3,2,2)",
    "=E5&\"x\"",
];

/// A tiny deterministic stream over the raw material proptest generated for one step.
struct Dice<'a> {
    raw: &'a [u32],
    i: usize,
}

impl Dice<'_> {
    fn next(&mut self) -> u32 {
        let v = self.raw[self.i % self.raw.len()].rotate_left((self.i / self.raw.len()) as u32 * 7);
        self.i += 1;
        v
    }
    fn int(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.next() % (hi - lo + 1)
    }
    fn chance(&mut self, pct: u32) -> bool {
        self.next() % 100 < pct
    }
    fn pick<'b, T>(&mut self, list: &'b [T]) -> &'b T {
        &list[(self.next() as usize) % list.len()]
    }
}

fn rand_range(d: &mut Dice<'_>) -> Range {
    let r0 = d.int(0, N - 1);
    let c0 = d.int(0, N - 1);
    Range::new(
        r0,
        c0,
        (r0 + d.int(0, 3)).min(N + 3),
        (c0 + d.int(0, 3)).min(N + 3),
    )
}

fn rand_value(d: &mut Dice<'_>) -> CellValue {
    match d.int(0, 5) {
        0 => CellValue::Blank,
        1 => CellValue::text(*d.pick(&["x", "Pen", "apple", ""])),
        2 => CellValue::Bool(d.chance(50)),
        3 => CellValue::Error(ErrorValue::NA),
        k => {
            let n = f64::from(d.int(0, 100)) - 50.0;
            CellValue::Number(if k == 4 { n } else { n / 4.0 })
        }
    }
}

fn style_patch(d: &mut Dice<'_>) -> StylePatch {
    match d.int(0, 3) {
        0 => StylePatch {
            font: Some(FontPatch {
                bold: Some(true),
                ..FontPatch::default()
            }),
            ..StylePatch::default()
        },
        1 => StylePatch {
            num_fmt: Some("0.00".into()),
            ..StylePatch::default()
        },
        2 => StylePatch {
            fill: Patch::Set(Fill::solid(Color::rgb(0xFFFF00))),
            ..StylePatch::default()
        },
        _ => StylePatch {
            alignment: Some(AlignmentPatch {
                horizontal: Some(HAlign::Center),
                ..AlignmentPatch::default()
            }),
            ..StylePatch::default()
        },
    }
}

fn rand_command(d: &mut Dice<'_>, wb: &mut Workbook) -> Command {
    let sheets: Vec<_> = wb
        .sheets()
        .iter()
        .map(|s| (s.id(), s.name().to_owned()))
        .collect();
    let (s, sname) = d.pick(&sheets).clone();
    let other = d.pick(&sheets).0;
    match d.int(0, 29) {
        0..=2 => Command::SetCells {
            sheet: s,
            cells: (0..d.int(1, 3))
                .map(|_| CellInput {
                    row: d.int(0, N),
                    col: d.int(0, N),
                    content: Content::Value(rand_value(d)),
                    style: None,
                })
                .collect(),
        },
        3 | 4 => Command::SetCells {
            sheet: s,
            cells: vec![CellInput::formula(
                d.int(0, N),
                d.int(0, N),
                *d.pick(&FORMULAS),
            )],
        },
        5 => Command::SetRange {
            sheet: s,
            row: d.int(0, N),
            col: d.int(0, N),
            values: (0..d.int(1, 3))
                .map(|_| (0..d.int(1, 3)).map(|_| Some(rand_value(d))).collect())
                .collect(),
        },
        6 => Command::ClearRange {
            sheet: s,
            ranges: vec![rand_range(d)],
            what: *d.pick(&[
                ClearWhat::All,
                ClearWhat::Contents,
                ClearWhat::Formats,
                ClearWhat::Notes,
                ClearWhat::Hyperlinks,
            ]),
        },
        7 => Command::SetStyle {
            sheet: s,
            ranges: vec![rand_range(d)],
            patch: Box::new(style_patch(d)),
        },
        8 | 9 => {
            let (at, count) = (d.int(0, N), d.int(1, 3));
            if d.chance(50) {
                Command::InsertRows {
                    sheet: s,
                    at,
                    count,
                    format_from: FormatFrom::Before,
                }
            } else {
                Command::InsertCols {
                    sheet: s,
                    at,
                    count,
                    format_from: FormatFrom::Before,
                }
            }
        }
        10 | 11 => {
            let (at, count) = (d.int(0, N), d.int(1, 3));
            if d.chance(50) {
                Command::DeleteRows {
                    sheet: s,
                    at,
                    count,
                }
            } else {
                Command::DeleteCols {
                    sheet: s,
                    at,
                    count,
                }
            }
        }
        12 => Command::MoveRange {
            sheet: s,
            range: rand_range(d),
            to_sheet: Some(other),
            to: CellPos::new(d.int(0, N), d.int(0, N)),
        },
        13 => Command::CopyRange {
            sheet: s,
            range: rand_range(d),
            to_sheet: Some(other),
            to: rand_range(d),
            what: *d.pick(&[
                PasteWhat::All,
                PasteWhat::Values,
                PasteWhat::Formulas,
                PasteWhat::Formats,
            ]),
            op: Default::default(),
            transpose: d.chance(20),
            skip_blanks: d.chance(20),
        },
        14 => {
            let range = rand_range(d);
            Command::Sort {
                sheet: s,
                range,
                keys: vec![if d.chance(50) {
                    SortKey::desc(range.c0)
                } else {
                    SortKey::asc(range.c0)
                }],
                header: d.chance(30),
                case_sensitive: false,
                by_columns: false,
                locale: None,
            }
        }
        15 => {
            if d.chance(50) || sheets.len() < 2 {
                Command::AddSheet {
                    name: None,
                    index: Some(d.int(0, sheets.len() as u32) as usize),
                    id: None,
                }
            } else {
                Command::RemoveSheet { sheet: s }
            }
        }
        16 => Command::RenameSheet {
            sheet: s,
            name: (*d.pick(&["Two", "Data", "My Sheet", "Q1", sname.as_str()])).to_owned(),
        },
        17 => Command::MoveSheet {
            sheet: s,
            index: d.int(0, sheets.len() as u32 - 1) as usize,
        },
        18 => Command::SetRowProps {
            sheet: s,
            at: d.int(0, N),
            count: d.int(1, 3),
            patch: match d.int(0, 3) {
                0 => RowPatch {
                    hidden: Some(true),
                    ..RowPatch::default()
                },
                1 => RowPatch {
                    hidden: Some(false),
                    ..RowPatch::default()
                },
                2 => RowPatch {
                    height: Patch::Set(F64(30.0)),
                    ..RowPatch::default()
                },
                _ => RowPatch {
                    outline_level: Some(1),
                    ..RowPatch::default()
                },
            },
        },
        19 => Command::SetColProps {
            sheet: s,
            at: d.int(0, N),
            count: d.int(1, 3),
            patch: match d.int(0, 2) {
                0 => ColPatch {
                    hidden: Some(true),
                    ..ColPatch::default()
                },
                1 => ColPatch {
                    width: Patch::Set(F64(20.0)),
                    ..ColPatch::default()
                },
                _ => ColPatch {
                    width: Patch::Clear,
                    ..ColPatch::default()
                },
            },
        },
        20 => Command::Merge {
            sheet: s,
            range: rand_range(d),
            mode: *d.pick(&[MergeMode::Merge, MergeMode::Center, MergeMode::Across]),
        },
        21 => Command::Unmerge {
            sheet: s,
            range: rand_range(d),
        },
        22 => {
            if d.chance(70) {
                let formulas = [
                    "0.5".to_owned(),
                    format!("'{sname}'!$A$1"),
                    "SUM(Two!$A$1:$A$3)".to_owned(),
                ];
                Command::DefineName {
                    name: "Rate".into(),
                    formula: d.pick(&formulas).clone(),
                    scope: if d.chance(70) { None } else { Some(s) },
                    hidden: false,
                    comment: None,
                }
            } else {
                Command::DeleteName {
                    name: "Rate".into(),
                    scope: None,
                }
            }
        }
        23 => {
            let (r0, c0) = (d.int(0, N - 3), d.int(0, N - 2));
            Command::AddTable {
                sheet: s,
                table: Box::new(Table {
                    id: 0,
                    name: "Tab".into(),
                    display_name: None,
                    range: Range::new(r0, c0, r0 + 2, c0 + 1),
                    header_row: true,
                    totals_row: d.chance(30),
                    columns: vec![TableColumn::named("Key"), TableColumn::named("Val")],
                    style: None,
                    auto_filter: None,
                    comment: None,
                }),
            }
        }
        24 => {
            if d.chance(50) {
                Command::RemoveTable { name: "Tab".into() }
            } else {
                Command::UpdateTable {
                    name: "Tab".into(),
                    patch: Box::new(TablePatch {
                        name: Some((*d.pick(&["Tab", "Tab2"])).to_owned()),
                        ..TablePatch::default()
                    }),
                }
            }
        }
        25 => Command::SetFreeze {
            sheet: s,
            rows: d.int(0, 3),
            cols: d.int(0, 3),
        },
        26 => {
            let (row, col) = (d.int(0, N), d.int(0, N));
            let empty = d.chance(20);
            let field = match d.int(0, 2) {
                0 => MetaField::Notes(if empty {
                    Vec::new()
                } else {
                    vec![Note {
                        at: CellPos::new(row, col),
                        author: "A".into(),
                        text: "n".into(),
                        visible: false,
                        size: None,
                    }]
                }),
                1 => MetaField::Hyperlinks(if empty {
                    Vec::new()
                } else {
                    vec![Hyperlink {
                        range: Range::cell(row, col),
                        target: Some("https://example.org".into()),
                        location: None,
                        display: None,
                        tooltip: None,
                    }]
                }),
                _ => MetaField::Validations(if empty {
                    Vec::new()
                } else {
                    let mut v = DataValidation::new(1, vec![rand_range(d)], ValidationType::List);
                    v.formula1 = Some((*d.pick(&["\"a,b\"", "$A$1:$A$3"])).to_owned());
                    vec![v]
                }),
            };
            Command::SetSheetMeta {
                sheet: s,
                field: Box::new(field),
            }
        }
        27 => Command::AutoFill {
            sheet: s,
            source: Range::cell(d.int(0, N), d.int(0, N)),
            target: {
                let r = d.int(0, N);
                let c = d.int(0, N);
                Range::new(r, c, r + d.int(0, 3), c)
            },
            mode: Default::default(),
        },
        28 => Command::RemoveDuplicates {
            sheet: s,
            range: rand_range(d),
            columns: Vec::new(),
            header: d.chance(30),
        },
        _ => {
            let dxf = wb.styles_mut().intern_dxf(Default::default());
            Command::SetSheetMeta {
                sheet: s,
                field: Box::new(MetaField::Cf(vec![ConditionalFormat {
                    ranges: vec![rand_range(d)],
                    rules: vec![CfRule {
                        id: 1,
                        priority: 1,
                        stop_if_true: false,
                        kind: CfKind::Expression {
                            formula: "A1>0".into(),
                            dxf,
                        },
                    }],
                }])),
            }
        }
    }
}

fn seed_workbook() -> Workbook {
    let mut wb = Workbook::with_options(WorkbookOptions {
        sheets: Some(vec!["One".into(), "Two".into()]),
        ..WorkbookOptions::default()
    });
    let (a, b) = (wb.sheets()[0].id(), wb.sheets()[1].id());
    let n = |x: f64| Some(CellValue::Number(x));
    let cmds = [
        Command::SetRange {
            sheet: a,
            row: 0,
            col: 0,
            values: vec![
                vec![n(1.0), n(2.0), n(3.0)],
                vec![n(4.0), n(5.0), n(6.0)],
                vec![n(7.0), n(8.0), n(9.0)],
            ],
        },
        Command::SetRange {
            sheet: b,
            row: 0,
            col: 0,
            values: vec![vec![n(10.0)], vec![n(20.0)], vec![n(30.0)]],
        },
        Command::SetCells {
            sheet: a,
            cells: vec![
                CellInput::formula(4, 4, "=SUM(A1:C3)"),
                CellInput::formula(5, 4, "=Two!A1+E5"),
            ],
        },
    ];
    for c in cmds {
        wb.apply(&c).expect("seed applies");
    }
    wb
}

fn value_key(v: &CellValue) -> String {
    match v {
        CellValue::Number(n) => format!("{:.9}", n),
        other => format!("{other:?}"),
    }
}

/// Computed values equal those of a workbook rebuilt from a snapshot.
fn assert_values_fresh(wb: &Workbook) {
    let mut fresh =
        Workbook::from_snapshot(wb.snapshot(), WorkbookOptions::default()).expect("rebuilds");
    fresh.recalc_all();
    for s in wb.sheets() {
        let Some(used) = s.cells().used_range(false) else {
            continue;
        };
        for r in used.r0..=used.r1.min(used.r0 + 30) {
            for c in used.c0..=used.c1.min(used.c0 + 30) {
                assert_eq!(
                    value_key(&wb.value(s.id(), r, c)),
                    value_key(&fresh.value(s.id(), r, c)),
                    "{}!R{}C{} (formula {:?})",
                    s.name(),
                    r + 1,
                    c + 1,
                    wb.formula(s.id(), r, c)
                );
            }
        }
    }
}

fn run(steps: &[[u32; 8]]) {
    let mut wb = seed_workbook();
    let mut log = CommandLog::new();
    let initial = state(&wb);
    let mut applied = 0;
    for raw in steps {
        let mut d = Dice { raw, i: 0 };
        let cmd = rand_command(&mut d, &mut wb);
        let before = state(&wb);
        match log.exec(&mut wb, cmd.clone()) {
            Ok(()) => {
                if log.undo_depth() == 0 || !log.can_undo() {
                    continue;
                }
                let after = state(&wb);
                if after == before {
                    // A command that changed nothing still undoes to the same state.
                    applied += 1;
                    continue;
                }
                applied += 1;
                assert!(log.undo(&mut wb).expect("undo"));
                assert_eq!(state(&wb), before, "inverse of {cmd:?}");
                assert_engine_in_sync(&wb);
                assert!(log.redo(&mut wb).expect("redo"));
                assert_eq!(state(&wb), after, "redo of {cmd:?}");
            }
            Err(_) => assert_eq!(
                state(&wb),
                before,
                "failed {cmd:?} must not change the workbook"
            ),
        }
        assert_engine_in_sync(&wb);
    }
    let final_state = state(&wb);
    assert_values_fresh(&wb);
    while log.undo(&mut wb).expect("undo") {
        assert_engine_in_sync(&wb);
    }
    assert_eq!(state(&wb), initial);
    assert_values_fresh(&wb);
    let mut redone = 0;
    while log.redo(&mut wb).expect("redo") {
        redone += 1;
    }
    assert!(redone <= applied);
    assert_eq!(state(&wb), final_state);
    assert_engine_in_sync(&wb);
    assert_values_fresh(&wb);
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: std::env::var("PROPTEST_CASES").ok().and_then(|v| v.parse().ok()).unwrap_or(48),
        // Integration tests have no lib.rs/main.rs for the default `SourceParallel` lookup;
        // persist shrunk failures next to this file so they are committed and replayed.
        failure_persistence: Some(Box::new(
            proptest::test_runner::FileFailurePersistence::WithSource("proptest-regressions"),
        )),
        ..ProptestConfig::default()
    })]

    #[test]
    fn op_inverse_identity_and_full_undo_redo(steps in prop::collection::vec(any::<[u32; 8]>(), 1..25)) {
        run(&steps);
    }
}

#[test]
fn interleaved_undo_and_redo_follow_a_shadow_history() {
    for seed in 0u32..8 {
        let mut wb = seed_workbook();
        let mut log = CommandLog::new();
        let mut states = vec![state(&wb)];
        let mut pos = 0usize;
        let mut x = 0x9E37_79B9u32 ^ seed.wrapping_mul(2_654_435_761);
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        };
        for _ in 0..40 {
            let roll = next() % 100;
            if roll < 25 && pos > 0 {
                assert!(log.undo(&mut wb).expect("undo"));
                pos -= 1;
                assert_eq!(state(&wb), states[pos]);
            } else if roll < 40 && pos + 1 < states.len() {
                assert!(log.redo(&mut wb).expect("redo"));
                pos += 1;
                assert_eq!(state(&wb), states[pos]);
            } else {
                let raw: [u32; 8] = std::array::from_fn(|_| next());
                let mut d = Dice { raw: &raw, i: 0 };
                let cmd = rand_command(&mut d, &mut wb);
                let depth = log.undo_depth();
                if log.exec(&mut wb, cmd).is_ok() && log.undo_depth() > depth {
                    pos += 1;
                    states.truncate(pos);
                    states.push(state(&wb));
                }
            }
            assert_engine_in_sync(&wb);
        }
        assert_values_fresh(&wb);
    }
}

#[test]
fn the_generator_produces_mostly_valid_commands() {
    let mut wb = seed_workbook();
    let mut log = CommandLog::new();
    let (mut ok, mut failed) = (0, 0);
    let mut kinds = std::collections::BTreeMap::new();
    let mut x = 12_345u32;
    for _ in 0..400 {
        let raw: [u32; 8] = std::array::from_fn(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        });
        let mut d = Dice { raw: &raw, i: 0 };
        let cmd = rand_command(&mut d, &mut wb);
        let kind = cmd.kind();
        match log.exec(&mut wb, cmd) {
            Ok(()) => {
                ok += 1;
                *kinds.entry(kind).or_insert(0) += 1;
            }
            Err(_) => failed += 1,
        }
    }
    assert!(ok > failed, "{ok} applied, {failed} refused");
    assert!(kinds.len() >= 25, "{kinds:?}");
    assert_engine_in_sync(&wb);
    assert_values_fresh(&wb);
    while log.undo(&mut wb).expect("undo") {}
    assert_eq!(state(&wb), state(&seed_workbook()));
}
