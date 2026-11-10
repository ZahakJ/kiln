// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every command of the catalog, then its undo, then its redo, comes back from the journal
//! alone: checkpoints are held back, so only the records can bring the workbook back. A command
//! that changes a workbook without going through the ledger (`apps/grid/ledger.rs`) fails here,
//! and so does a new command that does: the catalog's samples have every command in them.

use super::*;

type State = (
    Vec<u8>,
    BTreeMap<String, Vec<u8>>,
    BTreeMap<String, Vec<u8>>,
    Option<kiln_protocol::commands::DocPropertiesInfo>,
);

/// What a workbook holds, the view aside, as its native file keeps it (charts go through their
/// DrawingML, as every save and checkpoint takes them).
fn state(r: &Run, doc: DocId) -> State {
    let bytes = r.s.bundle_of(doc, 0, false).unwrap();
    let s = Session::new(Box::new(crate::DeferredHost::new(|| 0)));
    let bundle = kiln_storage::Bundle::from_bytes(&bytes).unwrap();
    let hint = OpenHint {
        name: None,
        path: None,
        format: None,
        read_only: false,
        kind: Some(DocKind::Grid),
        template: false,
    };
    let model = s
        .app_for_kind(DocKind::Grid)
        .unwrap()
        .open_bundle(&bundle, &hint)
        .unwrap();
    let b = model.to_bundle().unwrap();
    let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
    let mut snap = wb.snapshot();
    for sheet in &mut snap.sheets {
        sheet.meta.view.selection = Default::default();
        // A formula's value is worked out again (`RAND`, and what uses it).
        for rec in &mut sheet.cells {
            if rec.formula.is_some() {
                rec.value = kiln_sheet::CellValue::Blank;
            }
        }
    }
    snap.props.active_sheet = 0;
    let charts = b
        .passthrough
        .into_iter()
        .map(|(k, v)| {
            if k.contains("charts/") {
                let xml = String::from_utf8_lossy(&v).into_owned();
                (
                    k,
                    crate::apps::grid::storage::without_cached_values(&xml).into_bytes(),
                )
            } else {
                (k, v)
            }
        })
        .collect();
    (
        postcard::to_allocvec(&snap).unwrap(),
        charts,
        b.media,
        model.properties(),
    )
}

/// Answers what a command asks of the host: time passes, checkpoints stay held, the rest is
/// refused (no picker, file or network here).
fn settle_leniently(a: &mut Run) {
    for _ in 0..1000 {
        let requests: Vec<kiln_protocol::HostRequest> =
            a.s.poll()
                .into_iter()
                .filter_map(|e| match e {
                    Event::HostRequest(r) => Some(r),
                    _ => None,
                })
                .collect();
        if requests.is_empty() {
            return;
        }
        for r in requests {
            match &r.op {
                HostOp::Timer { ms } => {
                    a.later(u64::from(*ms));
                    let _ = a.s.host_reply(r.id, HostReply::Done);
                }
                HostOp::StorePut { .. } => a.held.push(r),
                // The clipboard gives back what was last copied.
                HostOp::ClipboardWrite { text, html } => {
                    CLIP.with(|c| *c.borrow_mut() = Some((text.clone(), html.clone())));
                    let _ = a.s.host_reply(r.id, HostReply::Done);
                }
                HostOp::ClipboardRead => {
                    let reply = CLIP.with(|c| c.borrow().clone()).map_or(
                        HostReply::Done,
                        |(text, html)| HostReply::Clipboard { text, html },
                    );
                    let _ = a.s.host_reply(r.id, reply);
                }
                _ => {
                    let _ = a.s.host_reply(
                        r.id,
                        HostReply::Failed(
                            (&kiln_common::KilnError::Unsupported("not in this test".into()))
                                .into(),
                        ),
                    );
                }
            }
        }
    }
    panic!("the session never settled");
}

/// Where a sample acts: the fixture's data first, then each thing it holds.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Place {
    /// `B2:D5` on the first sheet: numbers, formulas, an outline, a note's neighbour.
    Data,
    /// The chart.
    Chart,
    /// The picture.
    Picture,
    /// A cell of the table.
    Table,
    /// The cell with the note.
    Note,
    /// The cell with the comment.
    Comment,
    /// A formula (`D2`).
    Formula,
    /// A sparkline.
    Sparkline,
    /// The merged cells.
    Merge,
    /// The second sheet's filtered data.
    Filter,
    /// The pivot report on the third sheet.
    Pivot,
    /// The top of the pivot report.
    PivotTop,
    /// Words in a column (`A2:A4`: Clay, Glaze, Kiln).
    Words,
    /// A number stored as text (`B31`).
    NumberAsText,
    /// A cell showing an error (`C31`, `=1/0`).
    Error,
    /// A column to Flash Fill (`J2:J5`, its first example typed).
    Flash,
}

impl Place {
    const ALL: [Self; 16] = [
        Self::Data,
        Self::Chart,
        Self::Picture,
        Self::Table,
        Self::Note,
        Self::Comment,
        Self::Formula,
        Self::Sparkline,
        Self::Merge,
        Self::Filter,
        Self::Pivot,
        Self::PivotTop,
        Self::Words,
        Self::NumberAsText,
        Self::Error,
        Self::Flash,
    ];

    fn select(self, a: &mut Run, doc: DocId, try_run: &dyn Fn(&mut Run, Command)) {
        let (sheet, top, left, bottom, right) = match self {
            Self::Data | Self::Chart | Self::Picture => (0, 1, 1, 4, 3),
            Self::Table => (0, 11, 1, 11, 1),
            Self::Note => (0, 1, 0, 1, 0),
            Self::Comment => (0, 2, 0, 2, 0),
            Self::Formula => (0, 1, 3, 1, 3),
            Self::Sparkline => (0, 1, 7, 1, 7),
            Self::Merge => (0, 0, 5, 1, 6),
            Self::Filter => (1, 1, 0, 4, 1),
            Self::Pivot => (2, 3, 1, 3, 1),
            Self::PivotTop => (2, 0, 0, 0, 0),
            Self::Words => (0, 1, 0, 3, 0),
            Self::NumberAsText => (0, 30, 1, 30, 1),
            Self::Error => (0, 30, 2, 30, 2),
            Self::Flash => (0, 1, 9, 4, 9),
        };
        try_run(a, G::ActivateSheet { index: sheet }.into());
        select_range(a, doc, top, left, bottom, right);
        match self {
            Self::Chart => try_run(a, G::SelectDrawing { index: Some(0) }.into()),
            Self::Picture => try_run(a, G::SelectDrawing { index: Some(1) }.into()),
            _ => {}
        }
    }
}

/// A rich workbook: values, formulas and `RAND`, a merge, a note and a comment, a name, an
/// outline, a table, a filter, a pivot, sparklines, a chart and a picture, on three sheets, the
/// selection at `place` and a copy on the clipboard; its
/// checkpoint on the disk, the ones after it held. Every sample has something to act on.
fn fixture(d: &Arc<Mutex<Disk>>, place: Place) -> (Run, DocId) {
    use kiln_protocol::commands::EditCommand as E;
    use kiln_protocol::commands::grid_features::{
        GridCfRule, GridCfRuleKind, GridCfStyle, GridColumnFilter, GridCompare, GridCriterion,
        GridFilterOp, GridLink, GridValidation,
    };
    let (mut a, doc) = slow_checkpoints(d);
    lock(d).hold_checkpoints = false;
    let try_run = |a: &mut Run, c: Command| {
        let _ = a.s.execute(Some(doc), c);
        settle_leniently(a);
    };
    let typed = |a: &mut Run, row: u32, col: u32, v: &str| {
        select_cell(a, doc, row, col);
        a.run(doc, G::BeginEdit { text: v.into() }.into());
        a.run(doc, G::CommitEdit { advance: None }.into());
    };
    for (row, values) in [
        ["Item", "Qty", "Price", "Total"],
        ["Clay", "12", "3.5", "=B2*C2"],
        ["Glaze", "4", "11", "=B3*C3"],
        ["Kiln", "1", "900", "=B4*C4"],
        ["Clay", "12", "3.5", "=B5*C5"],
        ["Sum", "=SUM(B2:B5)", "=RAND()", "=SUM(D2:D5)"],
    ]
    .iter()
    .enumerate()
    {
        for (col, v) in values.iter().enumerate() {
            typed(&mut a, row as u32, col as u32, v);
        }
    }
    for (row, values) in [
        ["Name", "Size", "Glaze"],
        ["Bowl", "3", "Ash"],
        ["Jug", "7", "Celadon"],
        ["Cup", "2", "Ash"],
    ]
    .iter()
    .enumerate()
    {
        for (col, v) in values.iter().enumerate() {
            typed(&mut a, 10 + row as u32, col as u32, v);
        }
    }
    select_range(&mut a, doc, 10, 0, 13, 2);
    try_run(
        &mut a,
        G::CreateTable {
            range: None,
            header: true,
            style: "TableStyleMedium2".into(),
        }
        .into(),
    );
    select_range(&mut a, doc, 0, 5, 1, 6);
    try_run(&mut a, G::MergeCells.into());
    select_cell(&mut a, doc, 1, 0);
    try_run(
        &mut a,
        G::SetNote {
            text: Some("a note".into()),
            author: "R".into(),
            cell: None,
        }
        .into(),
    );
    select_cell(&mut a, doc, 2, 0);
    try_run(
        &mut a,
        G::AddComment {
            text: "a comment".into(),
            author: "R".into(),
            reply_to: None,
            now_ms: 1_792_000_000_000,
        }
        .into(),
    );
    try_run(
        &mut a,
        G::DefineName {
            name: "Prices".into(),
            refers_to: "=Sheet1!$C$2:$C$5".into(),
            scope: None,
            comment: String::new(),
            previous: None,
        }
        .into(),
    );
    for (name, to) in [("Rate", "=Sheet1!$C$2"), ("Rates", "=Sheet1!$C$2:$C$3")] {
        try_run(
            &mut a,
            G::DefineName {
                name: name.into(),
                refers_to: to.into(),
                scope: None,
                comment: String::new(),
                previous: None,
            }
            .into(),
        );
    }
    // A custom number format, a validation, a conditional format and a link in the data; a
    // word to replace; an outline of rows and one of columns.
    typed(&mut a, 2, 4, "kiln");
    select_cell(&mut a, doc, 4, 4);
    try_run(
        &mut a,
        G::NumberFormat {
            format: "0.000\" kg\"".into(),
        }
        .into(),
    );
    select_cell(&mut a, doc, 2, 1);
    try_run(
        &mut a,
        G::SetValidation {
            rule: Some(GridValidation {
                formula1: "1".into(),
                formula2: "99".into(),
                ..Default::default()
            }),
        }
        .into(),
    );
    try_run(
        &mut a,
        G::AddConditionalFormat {
            rule: GridCfRule {
                id: 0,
                applies_to: "$B$2:$D$5".into(),
                kind: GridCfRuleKind::CellValue {
                    op: GridCompare::GreaterThan,
                    values: vec!["10".into()],
                },
                style: Some(GridCfStyle {
                    fill: Some("FFE699".into()),
                    bold: true,
                    ..Default::default()
                }),
                stop_if_true: false,
            },
        }
        .into(),
    );
    select_cell(&mut a, doc, 3, 2);
    try_run(
        &mut a,
        G::SetLink {
            link: Some(GridLink {
                target: "https://kiln.example/".into(),
                display: "900".into(),
                ..GridLink::default()
            }),
        }
        .into(),
    );
    select_range(&mut a, doc, 1, 0, 2, 3);
    try_run(&mut a, G::Group { columns: false }.into());
    select_range(&mut a, doc, 0, 1, 0, 2);
    try_run(&mut a, G::Group { columns: true }.into());
    try_run(
        &mut a,
        G::InsertSparklines {
            kind: Default::default(),
            data: "B2:D5".into(),
            location: "H2:H5".into(),
        }
        .into(),
    );
    select_range(&mut a, doc, 0, 0, 4, 3);
    try_run(
        &mut a,
        Command::Suite(SuiteCommand::ChartInsert { kind: None }),
    );
    select_cell(&mut a, doc, 20, 8);
    try_run(
        &mut a,
        G::InsertPicture {
            name: "pixel.png".into(),
            bytes: super::PIXEL.to_vec(),
        }
        .into(),
    );
    // A second sheet with a filter (two columns filtered), a third with a pivot of the first, a
    // fourth to delete.
    try_run(&mut a, G::InsertSheet.into());
    for (row, values) in [
        ["Kind", "n", "Stage", "Cost"],
        ["a", "3", "Firing", "120"],
        ["b", "6", "Glaze", "80"],
        ["a", "9", "Bisque", "200"],
        ["c", "12", "Firing", "40"],
    ]
    .iter()
    .enumerate()
    {
        for (col, v) in values.iter().enumerate() {
            typed(&mut a, row as u32, col as u32, v);
        }
    }
    select_range(&mut a, doc, 0, 0, 4, 3);
    try_run(&mut a, G::ToggleFilter.into());
    try_run(
        &mut a,
        G::FilterColumn {
            row: 0,
            col: 0,
            filter: Some(GridColumnFilter::Values {
                values: vec!["a".into(), "c".into()],
                blanks: false,
            }),
        }
        .into(),
    );
    try_run(
        &mut a,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Custom {
                and: true,
                criteria: vec![GridCriterion {
                    op: GridFilterOp::GreaterThan,
                    value: "1".into(),
                }],
            }),
        }
        .into(),
    );
    try_run(&mut a, G::InsertSheet.into());
    try_run(
        &mut a,
        G::CreatePivot {
            source: "Sheet1!$A$1:$D$5".into(),
            to: Some("Sheet3!$A$1".into()),
        }
        .into(),
    );
    try_run(&mut a, G::InsertSheet.into());
    typed(&mut a, 0, 0, "the fourth sheet");
    // A fifth sheet, hidden, to show again.
    try_run(&mut a, G::InsertSheet.into());
    typed(&mut a, 0, 0, "hidden");
    try_run(&mut a, G::HideSheet { index: 4 }.into());
    try_run(&mut a, G::ActivateSheet { index: 0 }.into());
    // A number stored as text, an error, a word misspelt, Flash Fill's first example, a
    // sheet-level name, the table's total row.
    typed(&mut a, 30, 1, "'42");
    typed(&mut a, 30, 2, "=1/0");
    typed(&mut a, 5, 4, "teh plan");
    typed(&mut a, 1, 9, "CLAY");
    try_run(
        &mut a,
        G::DefineName {
            name: "Rates".into(),
            refers_to: "=Sheet1!$C$2:$C$5".into(),
            scope: Some(0),
            comment: String::new(),
            previous: None,
        }
        .into(),
    );
    select_cell(&mut a, doc, 11, 1);
    try_run(
        &mut a,
        G::TableOptions {
            options: kiln_protocol::commands::grid_features::GridTableOptions {
                header_row: true,
                total_row: true,
                banded_rows: true,
                banded_columns: true,
                filter_button: true,
                ..Default::default()
            },
        }
        .into(),
    );
    // The pivot report and the filter, out of date: a refresh and a reapply have work to do.
    typed(&mut a, 1, 1, "13");
    try_run(&mut a, G::ActivateSheet { index: 1 }.into());
    typed(&mut a, 1, 0, "b");
    try_run(&mut a, G::ActivateSheet { index: 0 }.into());
    // A copy, so a paste pastes something; then the selection the samples act on.
    select_range(&mut a, doc, 0, 0, 4, 3);
    try_run(&mut a, Command::Suite(SuiteCommand::Edit(E::Copy)));
    place.select(&mut a, doc, &try_run);
    land_checkpoint(&mut a);
    a.settle();
    lock(d).hold_checkpoints = true;
    (a, doc)
}

/// Commands that act on a workbook (the rest open, save, share or close documents).
fn acts_on_a_workbook(cmd: &Command) -> bool {
    match cmd {
        Command::Grid(_) => true,
        Command::Suite(SuiteCommand::Undo | SuiteCommand::Redo) => false,
        Command::Suite(s) => {
            let id = Command::Suite(s.clone()).id();
            [
                "suite.edit.",
                "suite.format.",
                "suite.chart.",
                "suite.insert.",
            ]
            .iter()
            .any(|p| id.starts_with(p))
                || matches!(
                    id,
                    "suite.file.properties" | "suite.file.inspect_remove" | "suite.view.zoom"
                )
        }
        _ => false,
    }
}

thread_local! {
    /// What was last copied.
    static CLIP: std::cell::RefCell<Option<(String, Option<String>)>> = const { std::cell::RefCell::new(None) };
}

/// A sample whose arguments name things of a workbook, aimed at the fixture's own: its comment's
/// id, its misspelt word, its formula for Goal Seek, its hidden sheet.
fn tuned(a: &Run, doc: DocId, cmd: &Command) -> Command {
    let b = a.s.docs[&doc].model.to_bundle().unwrap();
    let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
    let thread = wb.sheets()[0]
        .meta()
        .threads
        .first()
        .map(|t| t.id.clone())
        .unwrap_or_default();
    match cmd {
        Command::Grid(G::EditComment { text, .. }) => G::EditComment {
            id: thread,
            text: text.clone(),
        }
        .into(),
        Command::Grid(G::ResolveComment { done, .. }) => G::ResolveComment {
            id: thread,
            done: *done,
        }
        .into(),
        Command::Grid(G::SpellChange { with, all, .. }) => G::SpellChange {
            sheet: 0,
            row: 5,
            col: 4,
            offset: 0,
            word: "teh".into(),
            with: with.clone(),
            all: *all,
        }
        .into(),
        Command::Grid(G::GoalSeek { to_value, .. }) => G::GoalSeek {
            set_cell: "D2".into(),
            to_value: to_value.clone(),
            by_changing: "$B$2".into(),
        }
        .into(),
        Command::Grid(G::UnhideSheet { .. }) => G::UnhideSheet { index: 4 }.into(),
        other => other.clone(),
    }
}

/// Whether `cmd` changes the fixture at `place`: `Err` says why not.
fn changes(cmd: &Command, place: Place) -> Result<(), String> {
    let d = disk();
    let (mut a, doc) = fixture(&d, place);
    let cmd = &tuned(&a, doc, cmd);
    let start = state(&a, doc);
    let outcome = a.s.execute(Some(doc), cmd.clone());
    settle_leniently(&mut a);
    if !a.s.docs.contains_key(&doc) || state(&a, doc) != start {
        return Ok(());
    }
    Err(match outcome {
        Err(e) => format!("refused: {e}"),
        Ok(_) => "accepted, changes nothing".to_owned(),
    })
}

/// Runs `cmd` at `place` (and its undo, and its redo, as `steps` says), crashes, recovers and
/// compares.
fn replays(cmd: &Command, steps: usize, place: Place) -> Option<String> {
    let d = disk();
    let (mut a, doc) = fixture(&d, place);
    let cmd = &tuned(&a, doc, cmd);
    let _ = a.s.execute(Some(doc), cmd.clone());
    settle_leniently(&mut a);
    for undo in [SuiteCommand::Undo, SuiteCommand::Redo]
        .into_iter()
        .take(steps - 1)
    {
        let _ = a.s.execute(Some(doc), Command::Suite(undo));
        settle_leniently(&mut a);
    }
    if !a.s.docs.contains_key(&doc) {
        return None;
    }
    let live = state(&a, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    if let Some(stop) = note.and_then(|n| n.stopped) {
        return Some(format!(
            "{} ({steps}, {place:?}): recovery stopped at {stop:?}",
            cmd.id()
        ));
    }
    let back = state(&b, id);
    (back != live).then(|| {
        let what = [
            ("cells", back.0 != live.0),
            ("charts and original", back.1 != live.1),
            ("pictures", back.2 != live.2),
            ("properties", back.3 != live.3),
        ]
        .iter()
        .filter(|(_, differs)| *differs)
        .map(|(w, _)| *w)
        .collect::<Vec<_>>()
        .join(", ");
        let detail = if back.0 == live.0 {
            String::new()
        } else {
            let (x, y): (
                kiln_sheet::serial::WorkbookSnapshot,
                kiln_sheet::serial::WorkbookSnapshot,
            ) = (
                postcard::from_bytes(&back.0).unwrap(),
                postcard::from_bytes(&live.0).unwrap(),
            );
            crate::apps::grid::storage::first_difference(&format!("{x:?}"), &format!("{y:?}"))
        };
        format!("{} ({steps}, {place:?}): {what} differ {detail}", cmd.id())
    })
}

#[test]
fn every_command_of_the_catalog_and_its_undo_and_redo_replay_from_the_journal_alone() {
    let commands: Vec<Command> = kiln_protocol::samples::commands()
        .into_iter()
        .filter(acts_on_a_workbook)
        .collect();
    assert!(
        commands.len() > 200,
        "the catalog's Grid commands: {}",
        commands.len()
    );
    // Each sample acts where it changes the fixture: the data first (the chart, for a chart's
    // command), then each thing the fixture holds. One that changes it nowhere is listed below
    // with the reason.
    let mut failures = Vec::new();
    let mut nothing = Vec::new();
    let mut seen = BTreeMap::<&str, usize>::new();
    for cmd in &commands {
        let n = seen.entry(cmd.id()).or_default();
        *n += 1;
        let label = format!("{}#{n}", cmd.id());
        let first = if cmd.id().starts_with("suite.chart.") {
            Place::Chart
        } else {
            Place::Data
        };
        let mut why = Vec::<String>::new();
        let place = std::iter::once(first)
            .chain(Place::ALL.into_iter().filter(|p| *p != first))
            .find(|p| match changes(cmd, *p) {
                Ok(()) => true,
                Err(w) => {
                    if !why.contains(&w) {
                        why.push(w);
                    }
                    false
                }
            });
        let place = place.unwrap_or_else(|| {
            println!("CATALOG no change: {label} -- {}", why.join(" / "));
            nothing.push(label.clone());
            first
        });
        for steps in 1..=3 {
            failures.extend(replays(cmd, steps, place));
        }
    }
    let listed: Vec<String> = UNCHANGED.iter().map(|(l, _)| (*l).to_owned()).collect();
    let unlisted: Vec<_> = nothing.iter().filter(|l| !listed.contains(l)).collect();
    let stale: Vec<_> = listed.iter().filter(|l| !nothing.contains(l)).collect();
    println!(
        "CATALOG {} samples, {} change nothing anywhere",
        commands.len(),
        nothing.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} cases do not replay:\n{}",
        failures.len(),
        commands.len() * 3,
        failures.join("\n")
    );
    assert!(
        unlisted.is_empty() && stale.is_empty(),
        "samples that change nothing, not listed: {unlisted:?}; listed, but they change the \
         workbook now: {stale:?}"
    );
}

/// The samples that change the fixture nowhere, and why: they move the view (the selection, the
/// sheet on show, a view setting: nothing the journal holds), are not Grid's, or ask for what a
/// workbook does not have.
const UNCHANGED: &[(&str, &str)] = &[
    (
        "grid.cells.autofit_rows#1",
        "the sample's rows already have the height their cells need, and AutoFit leaves rows it has nothing to fit",
    ),
    (
        "suite.view.zoom#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "suite.chart.data_labels#1",
        "the chart already shows the sample's data labels",
    ),
    (
        "suite.chart.format_series#1",
        "the sample formats series 70000, which no chart has",
    ),
    (
        "suite.file.inspect_remove#2",
        "the sample removes speaker notes, which a workbook does not have",
    ),
    (
        "suite.edit.cut#1",
        "a cut changes the workbook when it is pasted (`suite.edit.paste` covers that)",
    ),
    (
        "suite.edit.copy#1",
        "a copy fills the clipboard, not the workbook",
    ),
    ("suite.edit.repeat#1", "not a command of Kiln Grid"),
    ("suite.format.all_caps#1", "not a command of Kiln Grid"),
    ("suite.format.change_case#1", "not a command of Kiln Grid"),
    ("suite.format.highlight#1", "not a command of Kiln Grid"),
    ("suite.format.bullets#1", "not a command of Kiln Grid"),
    ("suite.format.numbering#1", "not a command of Kiln Grid"),
    (
        "suite.format.multilevel_list#1",
        "not a command of Kiln Grid",
    ),
    (
        "suite.format.outdent#1",
        "the cells have no indent to take away",
    ),
    ("suite.format.line_spacing#1", "not a command of Kiln Grid"),
    (
        "suite.insert.pictures#1",
        "asks the host for a file, which this test refuses (`grid.insert.picture` covers a picture)",
    ),
    (
        "grid.nav.move#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.jump#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.page#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.row_start#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.first_cell#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.last_cell#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.advance#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.select_cell#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.select_range#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.select_rows#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.select_columns#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.select_all#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.editing.select_row#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.editing.select_column#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.nav.go_to#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.sheet.activate#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.cells.previous_sheet#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.cells.next_sheet#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.editing.edit_cell#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.begin#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.set_text#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.commit#1",
        "no edit is open to commit (`grid.editing.begin` and a commit run in every fixture)",
    ),
    (
        "grid.editing.put_back_draft#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.cancel_edit#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.new_line_in_cell#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.toggle_absolute#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.accept_completion#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.move_completion#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.point_ref#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.point_move#1",
        "the cell editor: text being typed is a draft until it is committed",
    ),
    (
        "grid.editing.open_dropdown#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.editing.drag_fill#1",
        "a step of a fill drag; the drop is the change (`grid.editing.auto_fill` covers it)",
    ),
    (
        "grid.editing.find_next#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.formulas.calculate_now#1",
        "calculation works values out again, and the comparison leaves formula values out (`RAND`)",
    ),
    (
        "grid.formulas.calculate_sheet#1",
        "calculation works values out again, and the comparison leaves formula values out (`RAND`)",
    ),
    ("grid.cells.unhide_columns#1", "no column is hidden"),
    ("grid.font.border_none#1", "the cells have no borders"),
    (
        "grid.alignment.align_bottom#1",
        "cells are aligned to the bottom already",
    ),
    (
        "grid.drawing.select#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.view.theme#1",
        "the window's theme, not the workbook's",
    ),
    (
        "grid.data.set_validation#2",
        "the sample removes validation from cells that have none",
    ),
    (
        "grid.data.circle_invalid#1",
        "circles on the view, not in the workbook",
    ),
    (
        "grid.table.resize#1",
        "the sample's range does not overlap the table",
    ),
    (
        "grid.editing.select_special#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.review.show_all_notes#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.review.next#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.formulas.trace_precedents#1",
        "arrows on the view, not in the workbook",
    ),
    (
        "grid.formulas.trace_dependents#1",
        "arrows on the view, not in the workbook",
    ),
    (
        "grid.formulas.remove_arrows#1",
        "arrows on the view, not in the workbook",
    ),
    ("grid.data.accept_invalid#1", "no circled cell to accept"),
    (
        "grid.formulas.trace_formula#1",
        "Evaluate Formula: a dialog, not the workbook",
    ),
    (
        "grid.formulas.watch#1",
        "the Watch Window: the view, not the workbook",
    ),
    (
        "grid.data.show_outline_symbols#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.data.show_outline_level#1",
        "the outline shows that level already",
    ),
    (
        "grid.data.toggle_outline_group#1",
        "the sample's cell is not the summary of a group",
    ),
    (
        "grid.review.spell_ignore#1",
        "ignoring a word is the speller's, not the workbook's",
    ),
    (
        "grid.review.load_dictionary#1",
        "the dictionary is the speller's, not the workbook's",
    ),
    (
        "grid.insert.picture#1",
        "the sample's bytes are not an image (the fixture inserts a real one)",
    ),
    ("grid.picture.reset#1", "the picture has nothing to reset"),
    (
        "grid.pivot.create#2",
        "the sample names a sheet `Sales` the fixture does not have",
    ),
    (
        "grid.pivot.edit#2",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#3",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#4",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#5",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#6",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#7",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#8",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#11",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#13",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.edit#14",
        "the sample edits a field, value or item the fixture's report does not have",
    ),
    (
        "grid.pivot.show_details#1",
        "the sample's report cell is not a value of the fixture's report",
    ),
    (
        "grid.editing.flash_fill#1",
        "Flash Fill finds no pattern in the fixture",
    ),
    (
        "grid.review.set_sheet_protection#2",
        "the sample unprotects a sheet that is not protected",
    ),
    ("grid.editing.auto_fill_option#1", "no fill to change"),
    (
        "grid.font.border_pen#1",
        "picks the border pen; the drawing is the change",
    ),
    (
        "grid.font.border_pen#2",
        "picks the border pen; the drawing is the change",
    ),
    ("grid.font.border_pen_off#1", "puts the border pen down"),
    ("grid.font.draw_border#1", "no border pen is picked"),
    (
        "grid.nav.selection_mode#1",
        "the view: the selection, the sheet on show or the window, which the journal does not hold",
    ),
    (
        "grid.editing.drag_cells#1",
        "the sample drags cells onto themselves",
    ),
];

/// Found by the shadow check on the rich fixture: a filter's rows were worked out again after
/// the filter had hidden them, found nothing to change, and the record said so.
#[test]
fn the_rows_a_filter_hides_come_back_hidden() {
    use kiln_protocol::commands::grid_features::{GridColumnFilter, GridCriterion, GridFilterOp};
    let d = disk();
    let (mut a, doc) = slow_checkpoints(&d);
    let typed = |a: &mut Run, row: u32, col: u32, v: &str| {
        select_cell(a, doc, row, col);
        a.run(doc, G::BeginEdit { text: v.into() }.into());
        a.run(doc, G::CommitEdit { advance: None }.into());
    };
    for (row, values) in [
        ["Kind", "n", "Stage", "Cost"],
        ["a", "3", "Firing", "120"],
        ["b", "6", "Glaze", "80"],
        ["a", "9", "Bisque", "200"],
        ["c", "12", "Firing", "40"],
    ]
    .iter()
    .enumerate()
    {
        for (col, v) in values.iter().enumerate() {
            typed(&mut a, row as u32, col as u32, v);
        }
    }
    select_range(&mut a, doc, 0, 0, 4, 3);
    a.run(doc, G::ToggleFilter.into());
    a.run(
        doc,
        G::FilterColumn {
            row: 0,
            col: 0,
            filter: Some(GridColumnFilter::Values {
                values: vec!["a".into(), "c".into()],
                blanks: false,
            }),
        }
        .into(),
    );
    a.run(
        doc,
        G::FilterColumn {
            row: 0,
            col: 1,
            filter: Some(GridColumnFilter::Custom {
                and: true,
                criteria: vec![GridCriterion {
                    op: GridFilterOp::GreaterThan,
                    value: "1".into(),
                }],
            }),
        }
        .into(),
    );
    a.settle();
    let live = state(&a, doc);
    drop(a);
    let mut b = Run::new(&d);
    let (id, note) = recover_with_note(&mut b);
    assert!(note.and_then(|n| n.stopped).is_none());
    assert!(
        state(&b, id) == live,
        "the filtered rows came back otherwise"
    );
}

/// Review 4's cases: the samples the catalog once listed as changing nothing, run with real
/// arguments on the same fixture, then a crash, a recovery and a comparison (with the shadow
/// check on).
mod rv4cat {
    use super::*;
    use kiln_protocol::commands::grid_features::{GridCalcMode, GridTableOptions, GridTotals};

    fn wb_of(a: &Run, doc: DocId) -> kiln_sheet::Workbook {
        let b = a.s.docs[&doc].model.to_bundle().unwrap();
        kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap()
    }

    /// Runs `setup` then crashes and recovers: Err with what differs.
    fn round(name: &str, setup: impl FnOnce(&mut Run, DocId)) -> Result<(), String> {
        let d = disk();
        let (mut a, doc) = fixture(&d, Place::Data);
        let start = state(&a, doc);
        setup(&mut a, doc);
        settle_leniently(&mut a);
        let live = state(&a, doc);
        let changed = live != start;
        drop(a);
        let mut b = Run::new(&d);
        let (id, note) = recover_with_note(&mut b);
        if let Some(stop) = note.and_then(|n| n.stopped) {
            return Err(format!(
                "RV4CAT {name}: changed {changed}: recovery stopped at {stop:?}"
            ));
        }
        let back = state(&b, id);
        if back != live {
            return Err(format!(
                "RV4CAT {name}: changed {changed}: differs: cells {} charts {} pictures {} props {}",
                back.0 != live.0,
                back.1 != live.1,
                back.2 != live.2,
                back.3 != live.3
            ));
        }
        println!("RV4CAT {name}: changed {changed}: recovered equal");
        Ok(())
    }

    fn run(a: &mut Run, doc: DocId, c: impl Into<Command>) {
        let r = a.s.execute(Some(doc), c.into());
        if let Err(e) = r {
            println!("RV4CAT   refused: {e}");
        }
        settle_leniently(a);
    }

    fn typed(a: &mut Run, doc: DocId, row: u32, col: u32, v: &str) {
        select_cell(a, doc, row, col);
        a.run(doc, G::BeginEdit { text: v.into() }.into());
        a.run(doc, G::CommitEdit { advance: None }.into());
    }

    #[test]
    fn rv4cat_unlisted_commands_with_real_arguments() {
        let mut fails = Vec::new();
        let mut go = |name: &str, f: &dyn Fn(&mut Run, DocId)| {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| round(name, f)));
            match r {
                Ok(Ok(())) => {}
                Ok(Err(e)) => fails.push(e),
                Err(p) => fails.push(format!(
                    "RV4CAT {name}: panicked: {}",
                    p.downcast_ref::<String>()
                        .cloned()
                        .or_else(|| p.downcast_ref::<&str>().map(|s| (*s).to_owned()))
                        .unwrap_or_default()
                )),
            }
        };
        go("unhide sheet", &|a, doc| {
            run(a, doc, G::HideSheet { index: 3 });
            run(a, doc, G::UnhideSheet { index: 3 });
        });
        go("hide sheet only", &|a, doc| {
            run(a, doc, G::HideSheet { index: 3 })
        });
        go("goal seek", &|a, doc| {
            run(
                a,
                doc,
                G::GoalSeek {
                    set_cell: "D2".into(),
                    to_value: "100".into(),
                    by_changing: "B2".into(),
                },
            )
        });
        go("edit comment", &|a, doc| {
            let id = wb_of(a, doc).sheets()[0].meta().threads[0].id.clone();
            run(
                a,
                doc,
                G::EditComment {
                    id,
                    text: "edited".into(),
                },
            );
        });
        go("resolve comment", &|a, doc| {
            let id = wb_of(a, doc).sheets()[0].meta().threads[0].id.clone();
            run(a, doc, G::ResolveComment { id, done: true });
        });
        go("spell change", &|a, doc| {
            run(
                a,
                doc,
                G::SpellChange {
                    sheet: 0,
                    row: 2,
                    col: 4,
                    offset: 0,
                    word: "kiln".into(),
                    with: "Kiln".into(),
                    all: false,
                },
            )
        });
        go("convert to number", &|a, doc| {
            typed(a, doc, 30, 1, "'42");
            select_cell(a, doc, 30, 1);
            run(a, doc, G::ConvertToNumber);
        });
        go("ignore error", &|a, doc| {
            typed(a, doc, 30, 2, "=1/0");
            select_cell(a, doc, 30, 2);
            run(a, doc, G::IgnoreError);
        });
        go("table total row and totals", &|a, doc| {
            select_cell(a, doc, 11, 1);
            run(
                a,
                doc,
                G::TableOptions {
                    options: GridTableOptions {
                        header_row: true,
                        total_row: true,
                        banded_rows: true,
                        filter_button: true,
                        ..Default::default()
                    },
                },
            );
            run(
                a,
                doc,
                G::TableTotals {
                    col: 1,
                    totals: GridTotals::Average,
                },
            );
        });
        go("sheet-level name deleted", &|a, doc| {
            run(
                a,
                doc,
                G::DefineName {
                    name: "Local".into(),
                    refers_to: "=Sheet1!$B$2".into(),
                    scope: Some(0),
                    comment: String::new(),
                    previous: None,
                },
            );
            run(
                a,
                doc,
                G::DeleteName {
                    name: "Local".into(),
                    scope: Some(0),
                },
            );
        });
        go("sheet-level name same as global", &|a, doc| {
            run(
                a,
                doc,
                G::DefineName {
                    name: "Rate".into(),
                    refers_to: "=Sheet1!$B$3".into(),
                    scope: Some(0),
                    comment: String::new(),
                    previous: None,
                },
            );
            typed(a, doc, 31, 1, "=Rate*2");
        });
        go("pivot refresh after a source edit", &|a, doc| {
            typed(a, doc, 1, 1, "50");
            run(a, doc, G::RefreshPivots { all: true });
        });
        go("pivot show details", &|a, doc| {
            run(a, doc, G::ActivateSheet { index: 2 });
            let wb = wb_of(a, doc);
            let s = &wb.sheets()[2];
            // The first numeric cell of the report.
            let at = wb
                .records_in(s.id(), kiln_sheet::Range::new(0, 0, 40, 10))
                .into_iter()
                .find(|r| matches!(r.value, kiln_sheet::CellValue::Number(_)))
                .map(|r| (r.row, r.col));
            println!("RV4CAT   pivot value at {at:?}");
            if let Some((row, col)) = at {
                run(a, doc, G::PivotDetails { row, col });
            }
        });
        go("reapply filter after an edit", &|a, doc| {
            run(a, doc, G::ActivateSheet { index: 1 });
            typed(a, doc, 4, 1, "0");
            run(a, doc, G::ReapplyFilter);
        });
        go("import custom list", &|a, doc| {
            select_range(a, doc, 1, 0, 3, 0);
            run(a, doc, G::ImportCustomList);
        });
        go("flash fill", &|a, doc| {
            typed(a, doc, 1, 9, "CLAY");
            select_range(a, doc, 1, 9, 4, 9);
            run(a, doc, G::FlashFill);
        });
        go("manual calculation then calculate now", &|a, doc| {
            run(
                a,
                doc,
                G::CalculationMode {
                    mode: GridCalcMode::Manual,
                },
            );
            typed(a, doc, 1, 1, "20");
            run(a, doc, G::CalculateNow);
        });
        for f in &fails {
            println!("{f}");
        }
        assert!(fails.is_empty(), "{}", fails.join("\n"));
    }

    /// Manual calculation: what a formula showed before the crash, against what recovery shows.
    #[test]
    fn rv4cat_manual_calculation_values_shown() {
        let d = disk();
        let (mut a, doc) = fixture(&d, Place::Data);
        run(
            &mut a,
            doc,
            G::CalculationMode {
                mode: GridCalcMode::Manual,
            },
        );
        typed(&mut a, doc, 1, 1, "20");
        settle_leniently(&mut a);
        let shown = |s: &Session, id: DocId| {
            let b = s.docs[&id].model.to_bundle().unwrap();
            let wb = kiln_sheet::Workbook::from_bytes(&b.model, Default::default()).unwrap();
            let sid = wb.sheets()[0].id();
            (1..5)
                .map(|r| format!("{:?}", wb.value(sid, r, 3)))
                .collect::<Vec<_>>()
        };
        let live = shown(&a.s, doc);
        drop(a);
        let mut b = Run::new(&d);
        let (id, _) = recover_with_note(&mut b);
        let back = shown(&b.s, id);
        println!("RV4MANUAL live {live:?}\nRV4MANUAL back {back:?}");
        assert_eq!(live, back);
    }

    /// Manual calculation: the sums the status bar showed before the crash, against after recovery.
    #[test]
    fn rv4cat_manual_calculation_stale_values_shown() {
        let d = disk();
        let (mut a, doc) = fixture(&d, Place::Data);
        lock(&d).hold_checkpoints = false;
        run(
            &mut a,
            doc,
            G::CalculationMode {
                mode: GridCalcMode::Manual,
            },
        );
        typed(&mut a, doc, 1, 1, "20");
        settle_leniently(&mut a);
        select_range(&mut a, doc, 1, 3, 4, 3);
        let live = grid_state(&a, doc).stats.and_then(|s| s.sum);
        drop(a);
        let mut b = Run::new(&d);
        let (id, _) = recover_with_note(&mut b);
        select_range(&mut b, id, 1, 3, 4, 3);
        let st = grid_state(&b, id);
        let back = st.stats.and_then(|s| s.sum);
        println!("RV4MANUAL2 live sum {live:?} back sum {back:?}");
        assert_eq!(live, back);
    }
}
