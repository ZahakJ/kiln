// SPDX-License-Identifier: AGPL-3.0-or-later
//! Random sessions of real commands, cut off at a random write. Each seed builds a workbook
//! (small, or over the size whose checkpoints are written as one run), runs random commands
//! (typing, entries left open, formulas over `RAND`, sorts and Remove Duplicates over them, Paste
//! Values to another sheet, Text to Columns over formulas, fills, rows and columns, sheets, undo
//! and redo) while large checkpoints land at random moments or not at all, and lets the power go
//! at a random write of a random command. Recovery must give the workbook as it was before that
//! command or after it (an action of several commands: after any of them), nothing else, with no
//! editor open and nothing left out.
//!
//! `KILN_CRASH_SEEDS` sets the seeds (default 1,000); a seed that ever fails gets a test of its own
//! at the end.

use super::*;

/// A small seeded generator (xorshift64*).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

/// What a workbook holds that recovery must give back: every sheet's cells (a formula over
/// `RAND` by its text alone, as recovery works it out afresh), metadata and order.
fn shown(r: &Run, doc: DocId) -> Shown {
    let b = r.s.docs[&doc].model.to_bundle().unwrap();
    let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
    let mut snap = wb.snapshot();
    for sheet in &mut snap.sheets {
        sheet.meta.view.selection = Default::default();
        // A formula's value is worked out again after a recovery (`RAND` and what uses it).
        for rec in &mut sheet.cells {
            if rec.formula.is_some() {
                rec.value = kiln_sheet::CellValue::Blank;
            }
        }
    }
    // The sheet on show, as the window shows it: recovery opens the document on it.
    snap.props.active_sheet = grid_state(r, doc).active_sheet as usize;
    let charts = b
        .passthrough
        .into_iter()
        .filter(|(k, _)| k.starts_with("charts/"))
        .map(|(k, v)| {
            let xml = String::from_utf8_lossy(&v).into_owned();
            (
                k,
                crate::apps::grid::storage::without_cached_values(&xml).into_bytes(),
            )
        })
        .collect();
    Shown {
        snap,
        charts,
        media: b.media,
    }
}

/// What a workbook shows, as [`shown`] reads it.
#[derive(Debug, PartialEq)]
struct Shown {
    snap: kiln_sheet::serial::WorkbookSnapshot,
    charts: BTreeMap<String, Vec<u8>>,
    media: BTreeMap<String, Vec<u8>>,
}

impl Shown {
    /// What differs from `other`, in words.
    fn differences(&self, other: &Self) -> String {
        let (a, b) = (&self.snap, &other.snap);
        let mut what = Vec::new();
        if a.sheets.len() != b.sheets.len() {
            what.push(format!(
                "{} sheets against {}",
                a.sheets.len(),
                b.sheets.len()
            ));
        }
        for (x, y) in a.sheets.iter().zip(&b.sheets) {
            if x.name != y.name || x.id != y.id {
                what.push(format!(
                    "sheet {} {} against {} {}",
                    x.id, x.name, y.id, y.name
                ));
            }
            if x.cells != y.cells {
                let first = x
                    .cells
                    .iter()
                    .zip(&y.cells)
                    .find(|(p, q)| p != q)
                    .map(|(p, q)| format!("{p:?} against {q:?}"))
                    .unwrap_or_else(|| {
                        format!("{} cells against {}", x.cells.len(), y.cells.len())
                    });
                what.push(format!("cells of {}: {first}", x.name));
            }
            if x.meta != y.meta {
                what.push(format!("metadata of {}", x.name));
            }
            if x.layout != y.layout {
                what.push(format!("layout of {}", x.name));
            }
        }
        if a.names != b.names {
            what.push("names".into());
        }
        if a.styles != b.styles {
            what.push("formats".into());
        }
        if a.props != b.props || a.counters != b.counters {
            what.push("properties or ids".into());
        }
        if self.charts != other.charts {
            what.push("charts".into());
        }
        if self.media != other.media {
            what.push("pictures".into());
        }
        what.join("; ")
    }
}

fn range_of(r0: u32, c0: u32, r1: u32, c1: u32) -> Command {
    G::SelectRange {
        range: kiln_protocol::commands::GridRange { r0, c0, r1, c1 },
        add: false,
    }
    .into()
}

fn cell_of(row: u32, col: u32) -> Command {
    G::SelectCell {
        row,
        col,
        extend: false,
        add: false,
    }
    .into()
}

/// One random action: the commands it sends.
fn action(rng: &mut Rng, sheets: &mut u32) -> (String, Vec<Command>) {
    let row = rng.below(24) as u32;
    let col = 1 + rng.below(5) as u32;
    let typed = |text: String| -> Vec<Command> {
        vec![
            G::BeginEdit { text }.into(),
            G::CommitEdit {
                advance: Some(GridDirection::Down),
            }
            .into(),
        ]
    };
    match rng.below(15) {
        0..=3 => {
            let text = match rng.below(3) {
                0 => format!("{}", rng.below(1000)),
                1 => format!("text {}", rng.below(100)),
                _ => format!("=B{}+{}", 1 + rng.below(20), rng.below(9)),
            };
            let mut c = vec![cell_of(row, col)];
            c.extend(typed(text));
            ("type".into(), c)
        }
        4 => {
            let mut c = vec![cell_of(row, 0)];
            c.extend(typed(format!("=INT(RAND()*4)+{}*0", rng.below(9))));
            ("rand".into(), c)
        }
        5 => (
            "sort over rand".into(),
            vec![range_of(0, 0, 23, 5), G::SortAscending.into()],
        ),
        6 => (
            "remove duplicates over rand".into(),
            vec![
                range_of(0, 0, 23, 5),
                G::RemoveDuplicates {
                    columns: vec![0],
                    header: Some(false),
                }
                .into(),
            ],
        ),
        7 => {
            let to = rng.below(u64::from(*sheets)) as u32;
            (
                "paste values to another sheet".into(),
                vec![
                    range_of(0, 0, 5, 1),
                    Command::Suite(SuiteCommand::Edit(
                        kiln_protocol::commands::EditCommand::Copy,
                    )),
                    G::ActivateSheet { index: to }.into(),
                    cell_of(row, 7),
                    G::PasteValues { data: None }.into(),
                    G::ActivateSheet { index: 0 }.into(),
                ],
            )
        }
        8 => {
            let mut c = vec![cell_of(row, 8)];
            c.extend(typed(format!(
                "=\"x,\"&INT(RAND()*99)&\",{}\"",
                rng.below(9)
            )));
            c.push(range_of(row, 8, row, 8));
            c.push(
                G::TextToColumns {
                    options: kiln_protocol::commands::grid_features::GridTextToColumns {
                        delimited: true,
                        delimiters: ",".into(),
                        ..Default::default()
                    },
                }
                .into(),
            );
            ("text to columns over a formula".into(), c)
        }
        9 => (
            "fill".into(),
            vec![
                range_of(row, col, row + 1 + rng.below(4) as u32, col),
                G::FillDown.into(),
            ],
        ),
        10 => {
            let c: Command = match rng.below(3) {
                0 => G::InsertRows.into(),
                1 => G::DeleteRows.into(),
                _ => G::InsertColumns.into(),
            };
            ("rows and columns".into(), vec![cell_of(row, col), c])
        }
        11 => ("undo".into(), vec![Command::Suite(SuiteCommand::Undo)]),
        12 => ("redo".into(), vec![Command::Suite(SuiteCommand::Redo)]),
        13 => {
            if *sheets < 3 && rng.chance(50) {
                *sheets += 1;
                (
                    "new sheet".into(),
                    vec![G::InsertSheet.into(), G::ActivateSheet { index: 0 }.into()],
                )
            } else {
                (
                    "another sheet".into(),
                    vec![
                        G::ActivateSheet {
                            index: rng.below(u64::from(*sheets)) as u32,
                        }
                        .into(),
                        cell_of(row, col),
                    ],
                )
            }
        }
        _ => (
            "an entry left open".into(),
            vec![
                cell_of(row, col),
                G::BeginEdit {
                    text: format!("half {}", rng.below(99)),
                }
                .into(),
            ],
        ),
    }
}

/// Lands the checkpoints held so far, or not.
fn land_some(a: &mut Run, rng: &mut Rng) {
    a.settle();
    if rng.chance(40) {
        land_checkpoint(a);
    }
}

/// One seed. `Err` says what went wrong.
fn crash_round(seed: u64) -> std::result::Result<(), String> {
    // `RAND` draws from the seed too: a seed gives the same session every time.
    kiln_formula::functions::math::seed_random(seed);
    let mut rng = Rng::new(seed);
    let d = disk();
    let mut a = Run::new(&d);
    let large = rng.chance(50);
    // Large: checkpoints over the size written in one run, landing at random moments.
    a.s.storage.inline_bytes = Some(if large { 0 } else { 1 << 30 });
    let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
    a.settle();
    for v in ["a", "b", "c"] {
        a.run(doc, G::BeginEdit { text: v.into() }.into());
        a.run(
            doc,
            G::CommitEdit {
                advance: Some(GridDirection::Down),
            }
            .into(),
        );
    }
    a.settle();
    lock(&d).hold_checkpoints = large;
    let mut sheets = 1;
    let steps = 6 + rng.below(40);
    let cut_at = rng.below(steps);
    let mut log = Vec::new();
    let mut before = shown(&a, doc);
    let mut candidates = Vec::new();
    for step in 0..steps {
        let (what, commands) = action(&mut rng, &mut sheets);
        log.push(what);
        if step == cut_at {
            // The power goes at a random write of this action.
            lock(&d).budget = Some(rng.below(6) as usize);
        }
        if step == cut_at {
            // Each command is whole or not there: the states between them all count.
            for c in commands {
                let _ = a.s.execute(Some(doc), c);
                candidates.push(shown(&a, doc));
            }
            break;
        }
        for c in commands {
            let _ = a.s.execute(Some(doc), c);
        }
        if large {
            land_some(&mut a, &mut rng);
        } else {
            a.settle();
        }
        before = shown(&a, doc);
    }
    candidates.push(before);
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    if entries.is_empty() {
        return Err(format!("seed {seed}: nothing to recover; actions {log:?}"));
    }
    let (id, note) = recover_with_note(&mut b);
    if let Some(stop) = note.and_then(|n| n.stopped) {
        return Err(format!("seed {seed}: stopped at {stop:?}; actions {log:?}"));
    }
    if grid_state(&b, id).editor.is_some() {
        return Err(format!("seed {seed}: an editor is open; actions {log:?}"));
    }
    let back = shown(&b, id);
    if !candidates.contains(&back) {
        return Err(format!(
            "seed {seed} (large {large}): not a state the workbook was in around action \
             {cut_at}; actions {log:?}"
        ));
    }
    Ok(())
}

/// The files a wide session starts from: workbooks with formats, conditional formats, names,
/// merges, notes, tables, filters, validation, charts and pictures of their own.
const FILES: [&str; 6] = [
    "xlsx/features/conditional-formatting.xlsx",
    "xlsx/features/styles.xlsx",
    "xlsx/features/tables.xlsx",
    "xlsx/features/merges.xlsx",
    "xlsx/features/comments.xlsx",
    "xlsx/features/charts.xlsx",
];

/// One random action of the wide sessions: everything [`action`] does, and formats, merges,
/// notes, comments, names, conditional formats, validation, tables, filters, charts, pictures,
/// cut and paste, copy and paste, sheets renamed, deleted and moved, manual calculation, anywhere
/// on the sheet.
fn wide_action(rng: &mut Rng, sheets: &mut u32) -> (String, Vec<Command>) {
    use kiln_protocol::commands::EditCommand as E;
    use kiln_protocol::commands::grid_features::{
        GridCalcMode, GridCfRule, GridCfRuleKind, GridCfStyle, GridCompare, GridValidation,
    };
    // Most of the time near the top; sometimes far out on the sheet.
    let (row, col) = if rng.chance(15) {
        (rng.below(1_000_000) as u32, rng.below(16_000) as u32)
    } else {
        (rng.below(40) as u32, rng.below(12) as u32)
    };
    let edit = |e: E| Command::Suite(SuiteCommand::Edit(e));
    match rng.below(34) {
        0..=14 => action(rng, sheets),
        15 => (
            "bold".into(),
            vec![range_of(row, col, row + 2, col + 1), edit(E::Bold)],
        ),
        16 => (
            "fill colour and number format".into(),
            vec![
                range_of(row, col, row + 3, col),
                G::FillColor {
                    color: Some(format!("{:06X}", rng.below(0xFF_FFFF))),
                }
                .into(),
                G::NumberFormat {
                    format: "0.00%".into(),
                }
                .into(),
            ],
        ),
        17 => {
            let m: Command = if rng.chance(50) {
                G::MergeCells.into()
            } else {
                G::UnmergeCells.into()
            };
            (
                "merge".into(),
                vec![range_of(row, col, row + 1, col + 1), m],
            )
        }
        18 => (
            "cut and paste".into(),
            vec![
                range_of(row, col, row + 2, col),
                edit(E::Cut),
                cell_of(rng.below(30) as u32, rng.below(10) as u32),
                edit(E::Paste { data: None }),
            ],
        ),
        19 => (
            "copy and paste".into(),
            vec![
                range_of(0, 0, 4, 3),
                edit(E::Copy),
                cell_of(rng.below(30) as u32, 4 + rng.below(6) as u32),
                edit(E::Paste { data: None }),
            ],
        ),
        20 => {
            let i = rng.below(u64::from(*sheets)) as u32;
            match rng.below(4) {
                0 => (
                    "rename sheet".into(),
                    vec![
                        G::RenameSheet {
                            index: i,
                            name: format!("S {}", rng.below(99)),
                        }
                        .into(),
                    ],
                ),
                1 if *sheets > 1 => {
                    *sheets -= 1;
                    (
                        "delete sheet".into(),
                        vec![
                            G::DeleteSheet { index: i }.into(),
                            G::ActivateSheet { index: 0 }.into(),
                        ],
                    )
                }
                2 if *sheets > 1 => (
                    "move sheet".into(),
                    vec![
                        G::MoveSheet {
                            from: i,
                            to: (i + 1) % *sheets,
                        }
                        .into(),
                        G::ActivateSheet { index: 0 }.into(),
                    ],
                ),
                _ if *sheets < 4 => {
                    *sheets += 1;
                    (
                        "duplicate sheet".into(),
                        vec![
                            G::DuplicateSheet { index: i }.into(),
                            G::ActivateSheet { index: 0 }.into(),
                        ],
                    )
                }
                _ => (
                    "clear".into(),
                    vec![
                        range_of(row, col, row + 2, col + 2),
                        G::ClearContents.into(),
                    ],
                ),
            }
        }
        21 => (
            "note".into(),
            vec![
                cell_of(row, col),
                G::SetNote {
                    text: Some(format!("n{}", rng.below(9))),
                    author: "R".into(),
                    cell: None,
                }
                .into(),
            ],
        ),
        22 => (
            "comment".into(),
            vec![
                cell_of(row, col),
                G::AddComment {
                    text: format!("c{}", rng.below(9)),
                    author: "R".into(),
                    reply_to: None,
                    now_ms: 1_792_000_000_000,
                }
                .into(),
            ],
        ),
        23 => (
            "column width".into(),
            vec![
                cell_of(row, col),
                G::ColumnWidth {
                    chars: 5.0 + rng.below(20) as f64,
                }
                .into(),
            ],
        ),
        24 => (
            "name".into(),
            vec![
                G::DefineName {
                    name: format!("N{}", rng.below(20)),
                    refers_to: format!("=$B${}:$C${}", 1 + row % 30, 3 + row % 30),
                    scope: None,
                    comment: String::new(),
                    previous: None,
                }
                .into(),
            ],
        ),
        25 => (
            "conditional format".into(),
            vec![
                G::AddConditionalFormat {
                    rule: GridCfRule {
                        id: 0,
                        applies_to: format!("$A${}:$F${}", 1 + row % 30, 9 + row % 30),
                        kind: GridCfRuleKind::CellValue {
                            op: GridCompare::GreaterThan,
                            values: vec![format!("{}", rng.below(50))],
                        },
                        style: Some(GridCfStyle {
                            fill: Some(format!("{:06X}", rng.below(0xFF_FFFF))),
                            bold: rng.chance(50),
                            ..Default::default()
                        }),
                        stop_if_true: false,
                    },
                }
                .into(),
            ],
        ),
        26 => (
            "validation".into(),
            vec![
                range_of(row, col, row + 3, col),
                G::SetValidation {
                    rule: Some(GridValidation {
                        formula1: "1".into(),
                        formula2: "99".into(),
                        ..Default::default()
                    }),
                }
                .into(),
            ],
        ),
        27 => (
            "table".into(),
            vec![
                range_of(30, 0, 34 + rng.below(4) as u32, 3),
                G::CreateTable {
                    range: None,
                    header: true,
                    style: "TableStyleMedium2".into(),
                }
                .into(),
            ],
        ),
        28 => (
            "filter".into(),
            vec![range_of(0, 0, 20, 5), G::ToggleFilter.into()],
        ),
        29 => (
            "chart".into(),
            vec![
                range_of(0, 1, 6, 3),
                Command::Suite(SuiteCommand::ChartInsert { kind: None }),
            ],
        ),
        30 => (
            "picture".into(),
            vec![
                cell_of(row, col),
                G::InsertPicture {
                    name: "pixel.png".into(),
                    bytes: super::PIXEL.to_vec(),
                }
                .into(),
            ],
        ),
        31 => {
            let mode = if rng.chance(50) {
                GridCalcMode::Manual
            } else {
                GridCalcMode::Automatic
            };
            (
                "calculation".into(),
                vec![G::CalculationMode { mode }.into(), G::CalculateNow.into()],
            )
        }
        32 => (
            "rows".into(),
            vec![cell_of(row % 40, col), G::InsertRows.into()],
        ),
        _ => (
            "undo twice".into(),
            vec![
                Command::Suite(SuiteCommand::Undo),
                Command::Suite(SuiteCommand::Undo),
            ],
        ),
    }
}

/// One wide seed: a rich workbook (sometimes a file from the disk), random actions, and the power
/// going at a random write, or while a held checkpoint lands.
fn wide_round(seed: u64) -> std::result::Result<(), String> {
    kiln_formula::functions::math::seed_random(seed);
    let mut rng = Rng::new(seed ^ 0x00ab_cdef);
    let d = disk();
    let mut a = Run::new(&d);
    let large = rng.chance(60);
    a.s.storage.inline_bytes = Some(if large { 0 } else { 1 << 30 });
    let doc = if rng.chance(40) {
        let file = FILES[rng.below(FILES.len() as u64) as usize];
        lock(&d).write("/docs/Start.xlsx", super::fixture(file));
        a.open_file(&d, "/docs/Start.xlsx")
    } else {
        a.s.new_document(DocKind::Grid).unwrap().doc
    };
    a.settle();
    // A rich start: values, formulas over them, RAND, a merge and a note.
    for (r, v) in ["12", "=B1*2", "=INT(RAND()*5)", "text", "=SUM(B1:B3)"]
        .iter()
        .enumerate()
    {
        a.run(doc, cell_of(r as u32, 1));
        a.run(doc, G::BeginEdit { text: (*v).into() }.into());
        a.run(doc, G::CommitEdit { advance: None }.into());
    }
    a.run(doc, range_of(8, 1, 9, 2));
    let _ = a.s.execute(Some(doc), G::MergeCells.into());
    a.settle();
    lock(&d).hold_checkpoints = large;
    let mut sheets = a.s.docs[&doc]
        .model
        .properties()
        .map_or(1, |p| p.sheets.max(1));
    let steps = 6 + rng.below(50);
    let cut_at = rng.below(steps);
    // 0: in the action's writes; 1: while a held checkpoint lands after it.
    let mode = if large { rng.below(2) } else { 0 };
    let mut log = Vec::new();
    let mut before = shown(&a, doc);
    let mut candidates = Vec::new();
    for step in 0..steps {
        let (what, commands) = wide_action(&mut rng, &mut sheets);
        log.push(what);
        if step == cut_at {
            if mode == 0 {
                lock(&d).budget = Some(rng.below(8) as usize);
                for c in commands {
                    let _ = a.s.execute(Some(doc), c);
                    candidates.push(shown(&a, doc));
                }
            } else {
                for c in commands {
                    let _ = a.s.execute(Some(doc), c);
                }
                before = shown(&a, doc);
                lock(&d).budget = Some(rng.below(4) as usize);
                land_checkpoint(&mut a);
            }
            break;
        }
        for c in commands {
            let _ = a.s.execute(Some(doc), c);
        }
        if large {
            land_some(&mut a, &mut rng);
        } else {
            a.settle();
        }
        before = shown(&a, doc);
    }
    candidates.push(before);
    drop(a);
    let mut b = Run::new(&d);
    let entries = b.recovery();
    if entries.is_empty() {
        return Err(format!("seed {seed}: nothing to recover; actions {log:?}"));
    }
    let (id, note) = recover_with_note(&mut b);
    if let Some(stop) = note.and_then(|n| n.stopped) {
        return Err(format!(
            "seed {seed}: stopped at {stop:?} mode {mode}; actions {log:?}"
        ));
    }
    if grid_state(&b, id).editor.is_some() {
        return Err(format!("seed {seed}: an editor is open; actions {log:?}"));
    }
    let back = shown(&b, id);
    if !candidates.contains(&back) {
        let why = candidates
            .last()
            .map(|c| back.differences(c))
            .unwrap_or_default();
        return Err(format!(
            "seed {seed} (large {large} mode {mode}): not a state around action {cut_at} \
             ({why}; sheet on show {} of {:?}, around it {:?}); actions {log:?}",
            back.snap.props.active_sheet,
            back.snap
                .sheets
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            candidates
                .iter()
                .map(|c| c.snap.props.active_sheet)
                .collect::<Vec<_>>()
        ));
    }
    Ok(())
}

#[test]
fn wide_random_sessions_recover_what_was_shown() {
    let seeds: u64 = std::env::var("KILN_WIDE_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);
    let first: u64 = std::env::var("KILN_WIDE_FIRST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let failures: Vec<String> = (first..first + seeds)
        .filter_map(|seed| {
            std::panic::catch_unwind(|| wide_round(seed))
                .unwrap_or_else(|p| {
                    let why = p
                        .downcast_ref::<String>()
                        .cloned()
                        .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                        .unwrap_or_default();
                    Err(format!("seed {seed}: panicked: {why}"))
                })
                .err()
        })
        .collect();
    for f in &failures {
        println!("WIDEFAIL {f}");
    }
    println!(
        "WIDE seeds {first}..{}: {} of {seeds} failed",
        first + seeds - 1,
        failures.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {seeds} seeds failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Seeds that failed once, each a test of its own: every run of a seed is the same.
#[test]
fn kept_seed_13552() {
    for _ in 0..kept_runs(20) {
        crash_round(13552).unwrap();
    }
}

/// How many times a kept seed runs (`KILN_KEPT_RUNS`).
fn kept_runs(default: usize) -> usize {
    std::env::var("KILN_KEPT_RUNS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// The wide sessions' seeds that failed while the test was being made (their workbook and its
/// comparison were wrong as often as recovery was): each is kept.
const KEPT_WIDE: &[u64] = &[
    1, 2, 3, 6, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 21, 22, 23, 24, 26, 27, 28, 30, 31, 35,
    36, 37, 39, 40, 41, 42, 44, 46, 47, 48, 50, 51, 52, 53, 54, 57, 58, 60, 61, 62, 64, 65, 66, 67,
    68, 69, 70, 71, 72, 73, 76, 77, 79, 80, 81, 83, 86, 87, 88, 90, 92, 93, 94, 97, 98, 100, 101,
    102, 103, 104, 110, 112, 114, 115, 116, 117, 118, 119, 121, 122, 123, 124, 125, 127, 130, 131,
    134, 135, 137, 139, 140, 144, 145, 146, 147, 148, 149, 151, 153, 154, 155, 156, 157, 159, 160,
    161, 162, 163, 165, 166, 167, 168, 170, 171, 173, 174, 176, 177, 179, 183, 184, 185, 188, 190,
    191, 192, 193, 194, 195, 196, 197, 198, 201, 202, 204, 206, 207, 210, 211, 215, 216, 217, 218,
    219, 222, 223, 224, 225, 226, 227, 228, 229, 231, 232, 233, 234, 235, 236, 237, 239, 240, 242,
    243, 244, 246, 247, 249, 250, 251, 252, 253, 254, 257, 259, 260, 261, 262, 263, 264, 265, 266,
    267, 268, 269, 272, 273, 274, 278, 279, 280, 281, 283, 284, 285, 286, 287, 288, 289, 291, 292,
    293, 294, 295, 296, 298, 299, 300,
];

#[test]
fn kept_wide_seeds() {
    let runs = kept_runs(1);
    let mut failed = Vec::new();
    for &seed in KEPT_WIDE {
        for _ in 0..runs {
            if let Err(e) = wide_round(seed) {
                failed.push(e);
            }
        }
    }
    println!(
        "KEPT {} wide seeds, {runs} runs each: {} failed",
        KEPT_WIDE.len(),
        failed.len()
    );
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}

#[test]
fn random_sessions_cut_off_at_a_random_write_recover_what_was_shown() {
    // 1,000 by default (about 15 s): a bug the review planted that shows in about one seed in
    // 400 with the shadow check off (a redo round the capture) is then caught nine runs in ten
    // by this test alone; the shadow check catches it in every run.
    let seeds: u64 = std::env::var("KILN_CRASH_SEEDS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000);
    let first: u64 = std::env::var("KILN_CRASH_FIRST_SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1);
    let failures: Vec<String> = (first..first + seeds)
        .filter_map(|seed| crash_round(seed).err())
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {seeds} seeds failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// The third review's planted bugs, each switched on in a test of its own (`ledger::plant`):
/// every guard is shown to catch what it is there for, every run.
mod guards {
    use super::*;
    use crate::apps::grid::ledger::plant::{self, Plant};

    /// A workbook with a cell typed, then a tab colour set through planted bypass `n` of the
    /// ledger (review 5's B3 to B6, `ledger::escape`).
    fn bypass(n: u8, a: &mut Run, doc: DocId) {
        enter_cell(a, doc, "kept");
        crate::apps::grid::ledger::escape::set(Some(n));
        let r = a.s.execute(
            Some(doc),
            G::SheetTabColor {
                index: 0,
                color: Some("#C0504D".into()),
            }
            .into(),
        );
        crate::apps::grid::ledger::escape::set(None);
        r.unwrap();
        a.settle();
    }

    /// Each bypass changes the workbook without the ledger: the stamp guard stops it in a test
    /// build, the shadow check off.
    #[test]
    fn the_stamp_guard_stops_each_bypass_of_the_ledger() {
        for n in 3..=6 {
            let caught = with_plant(None, true, || {
                let d = disk();
                let mut a = Run::new(&d);
                let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
                a.settle();
                bypass(n, &mut a, doc);
            });
            println!("GUARD stamp B{n}: {caught:?}");
            assert!(
                caught.is_some_and(|w| w.contains("the workbook changed outside the ledger")),
                "B{n}"
            );
        }
    }

    /// The same bypasses where a release build would meet them: the document is kept as it is,
    /// the change goes in the journal by the command's name and a checkpoint follows at once.
    /// Once it lands a recovery gives what was on screen; before it lands the recovery stops
    /// before the change and names it.
    #[test]
    fn a_bypass_in_a_release_build_is_a_named_stop_and_a_checkpoint() {
        for n in 3..=6 {
            plant::set_release_guard(true);
            let d = disk();
            let mut a = Run::new(&d);
            let doc = a.s.new_document(DocKind::Grid).unwrap().doc;
            a.settle();
            bypass(n, &mut a, doc);
            let before = shown(&a, doc);
            let keys = journal_keys(&d);
            drop(a);
            let mut b = Run::new(&d);
            let id = b.recover_only();
            let after = shown(&b, id);
            println!(
                "RELEASE B{n}: journal {keys:?}; recovered as shown: {}",
                after == before
            );
            assert_eq!(after, before, "B{n}");

            let d = disk();
            let (mut a, doc) = slow_checkpoints(&d);
            bypass(n, &mut a, doc);
            drop(a);
            let mut b = Run::new(&d);
            let (_, note) = recover_with_note(&mut b);
            let stop = note.and_then(|n| n.stopped);
            plant::set_release_guard(false);
            println!("RELEASE B{n} before its checkpoint: {stop:?}");
            assert_eq!(
                stop.and_then(|s| s.command).as_deref(),
                Some("grid.sheet.tab_color"),
                "B{n}"
            );
        }
    }

    /// Runs `f` with `p` planted, and the shadow check off if `no_shadow`; gives what it panicked
    /// with, if it did.
    fn with_plant(p: Option<Plant>, no_shadow: bool, f: impl FnOnce()) -> Option<String> {
        plant::set(p);
        plant::set_no_shadow(no_shadow);
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        plant::set(None);
        plant::set_no_shadow(false);
        out.err().map(|p| {
            p.downcast_ref::<String>()
                .cloned()
                .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                .unwrap_or_default()
        })
    }

    /// A sort over `RAND`, undone and redone.
    fn sort_undo_redo() {
        kiln_formula::functions::math::seed_random(7);
        let d = disk();
        let (mut a, doc) = slow_checkpoints(&d);
        for row in 0..8 {
            select_cell(&mut a, doc, row, 0);
            a.run(
                doc,
                G::BeginEdit {
                    text: "=RAND()".into(),
                }
                .into(),
            );
            a.run(doc, G::CommitEdit { advance: None }.into());
            select_cell(&mut a, doc, row, 1);
            a.run(
                doc,
                G::BeginEdit {
                    text: format!("{row}"),
                }
                .into(),
            );
            a.run(doc, G::CommitEdit { advance: None }.into());
        }
        select_range(&mut a, doc, 0, 0, 7, 1);
        a.run(doc, G::SortAscending.into());
        a.run(doc, Command::Suite(SuiteCommand::Undo));
        a.run(doc, Command::Suite(SuiteCommand::Redo));
        a.settle();
    }

    /// Fill Down: a copy of cells (`CopyRange`).
    fn fill_down() {
        let d = disk();
        let (mut a, doc) = slow_checkpoints(&d);
        select_cell(&mut a, doc, 0, 2);
        a.run(doc, G::BeginEdit { text: "v".into() }.into());
        a.run(doc, G::CommitEdit { advance: None }.into());
        select_range(&mut a, doc, 0, 2, 3, 2);
        a.run(doc, G::FillDown.into());
        a.settle();
    }

    #[test]
    fn the_shadow_check_catches_a_redo_round_the_capture() {
        assert_eq!(with_plant(None, false, sort_undo_redo), None);
        let caught = with_plant(Some(Plant::RedoBareForward), false, sort_undo_redo);
        println!("GUARD shadow P1: {caught:?}");
        assert!(caught.is_some_and(|w| w.contains("journal check")));
    }

    #[test]
    fn the_shadow_check_catches_a_copy_the_journal_never_hears_of() {
        assert_eq!(with_plant(None, false, fill_down), None);
        let caught = with_plant(Some(Plant::CopyUnrecorded), false, fill_down);
        println!("GUARD shadow P2: {caught:?}");
        assert!(caught.is_some_and(|w| w.contains("journal check")));
    }

    /// The seeded random sessions, the shadow check off: they find the planted bug alone.
    fn random_sessions_find(
        p: Plant,
        round: fn(u64) -> std::result::Result<(), String>,
        seeds: std::ops::Range<u64>,
    ) -> Vec<u64> {
        let mut failed = Vec::new();
        let n = seeds.end - seeds.start;
        for seed in seeds {
            let mut result = Ok(());
            let panicked = with_plant(Some(p), true, || result = round(seed));
            if panicked.is_some() || result.is_err() {
                failed.push(seed);
            }
        }
        println!(
            "GUARD random {p:?}: {} of {n} seeds fail: {failed:?}",
            failed.len()
        );
        failed
    }

    #[test]
    fn the_random_sessions_catch_a_copy_the_journal_never_hears_of() {
        let failed = random_sessions_find(Plant::CopyUnrecorded, wide_round, 1..41);
        assert!(!failed.is_empty());
    }

    #[test]
    fn the_random_sessions_catch_a_redo_round_the_capture() {
        let failed = random_sessions_find(Plant::RedoBareForward, crash_round, 1..401);
        assert!(!failed.is_empty());
    }
}
