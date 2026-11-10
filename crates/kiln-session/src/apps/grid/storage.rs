// SPDX-License-Identifier: AGPL-3.0-or-later
//! A workbook as a native bundle (`.kgrid`).
//!
//! `model.bin` is the workbook (`KGRD`); the file it was opened from goes to
//! `passthrough/original.xlsx`, the charts changed in Kiln to `passthrough/charts/` (their
//! DrawingML), and the pictures added in Kiln to `media/`.
//!
//! The journal records what the ledger applied (`ledger.rs`): sheet commands, ids resolved, and
//! the few things that are not commands ([`GridOp`]). A command that reads what volatile cells
//! show is recorded as what it came to ([`Capture`]). Replaying recalculates, so volatile cells
//! get new values, as they do when a file opens; everything else is what was on screen. The entry
//! being typed in a cell is kept beside them as the journal's draft; recovery names it and does
//! not put it back on its own.

use kiln_common::{KilnError, Result};
use kiln_protocol::DocKind;
use kiln_sheet::meta::Person;
use kiln_sheet::styles::{CellStyle, Dxf};
use kiln_sheet::{Range, SheetId, Workbook};
use kiln_storage::Bundle;
use serde::{Deserialize, Serialize};

use super::GridDoc;
use crate::props::UserProps;

/// The passthrough entry of the original Excel file.
pub(super) const ORIGINAL: &str = "original.xlsx";
/// The passthrough folder of the charts changed in Kiln.
pub(super) const CHARTS: &str = "charts/";

pub(super) fn err(e: impl std::fmt::Display) -> KilnError {
    KilnError::Format {
        format: "kgrid".into(),
        message: e.to_string(),
    }
}

/// One operation of a workbook's journal. Everything a replay needs that is not a sheet command
/// is an operation too, so an edit and all that goes with it is a small record. The encoding is
/// [`kiln_storage::journal::JOURNAL_FORMAT`]'s: changing this type or `kiln_sheet::Command` raises
/// that (`the_journal_encoding_of_a_workbook_is_frozen` fails until it does).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) enum GridOp {
    /// A sheet command, ids resolved.
    Command(kiln_sheet::Command),
    /// Formats added to the workbook, in the order they were added.
    Styles {
        from: u32,
        styles: Vec<CellStyle>,
        dxf_from: u32,
        dxfs: Vec<Dxf>,
    },
    /// Rule ids handed out.
    RuleIds(u32),
    /// The people of threaded comments.
    Persons(Vec<Person>),
    /// Custom fill and sort lists.
    CustomLists(Vec<Vec<String>>),
    /// A chart as it is now, as DrawingML.
    Chart { part: String, xml: Vec<u8> },
    /// A picture added in Kiln.
    Media { part: String, bytes: Vec<u8> },
    /// The document properties.
    Properties(UserProps),
    /// The sheet on show.
    ActiveSheet(u32),
    /// Calculate Now (`None`) or Calculate Sheet.
    Recalc(Option<u32>),
    /// An undo or redo that changed nothing in the cells: the step still moved, and the
    /// document's state with it.
    Step,
}

impl GridOp {
    /// A chart as it is now.
    pub(super) fn chart(part: &str, chart: &kiln_charts::ChartSpace) -> Self {
        GridOp::Chart {
            part: part.to_owned(),
            xml: kiln_charts::write::write_chart(chart),
        }
    }
}

/// The journal format whose Kiln Grid records are bare sheet commands.
const FORMAT_BARE_COMMANDS: u32 = 2;

/// What the autosave journal has been told.
#[derive(Debug, Default)]
pub(super) struct JournalState {
    /// The draft last handed over.
    draft: Option<Vec<u8>>,
    /// The formats the journal knows of, once it has been told any.
    styles: Option<(usize, usize)>,
    /// The sheet on show, as the journal knows.
    active: Option<u32>,
    /// The format of the operations being replayed (0: this version's).
    format: u32,
}

impl JournalState {
    /// The format of the operations a replay reads.
    pub(super) fn set_format(&mut self, format: u32) {
        self.format = format;
    }
}

/// How a sheet command goes in the journal. Most replay to the same result from the command
/// alone. A command that reads the values cells show does not, when formulas feed it: `RAND`,
/// `NOW`, `TODAY`, `OFFSET` and whatever depends on them have other values at a replay. Those go
/// in as what they came to: the cells they wrote (for a sort, the rows in their new order) or
/// the rows a filter hid. These are all of them: `kiln_sheet::Workbook::apply` reads values for
/// no other command.
pub(super) enum Capture {
    /// The command itself.
    Command,
    /// These cells of a sheet, as they are afterwards, and with `meta` its metadata (notes and
    /// links move with sorted rows).
    Cells {
        sheet: SheetId,
        ranges: Vec<Range>,
        before: Vec<(u32, u32)>,
        meta: bool,
    },
    /// The rows a filter hid.
    Filter,
    /// A batch with such a command in it: the ledger applies it command by command.
    Split,
}

fn any_formula(wb: &Workbook, sheet: SheetId, range: Range) -> bool {
    wb.records_in(sheet, range)
        .iter()
        .any(|r| r.formula.is_some())
}

impl Capture {
    /// What `cmd` needs, before it is applied.
    pub(super) fn of(wb: &Workbook, cmd: &kiln_sheet::Command) -> Self {
        use kiln_sheet::Command as C;
        let cells = |read: (SheetId, Range), sheet: SheetId, writes: Vec<Range>, meta: bool| {
            if !any_formula(wb, read.0, read.1) {
                return Self::Command;
            }
            let before = writes
                .iter()
                .flat_map(|r| wb.records_in(sheet, *r))
                .map(|r| (r.row, r.col))
                .collect();
            Self::Cells {
                sheet,
                ranges: writes,
                before,
                meta,
            }
        };
        match cmd {
            C::Sort { sheet, range, .. } | C::RemoveDuplicates { sheet, range, .. } => {
                cells((*sheet, *range), *sheet, vec![*range], true)
            }
            C::FillSeries { sheet, range, .. } => {
                cells((*sheet, *range), *sheet, vec![*range], false)
            }
            C::AutoFill {
                sheet,
                source,
                target,
                ..
            } => cells((*sheet, *source), *sheet, vec![*target], false),
            C::CopyRange {
                sheet,
                range,
                to_sheet,
                to,
                transpose,
                ..
            } => {
                // Where it lands: `to` tiled, or one block of the source's size at its corner.
                let (h, w) = if *transpose {
                    (range.c1 - range.c0, range.r1 - range.r0)
                } else {
                    (range.r1 - range.r0, range.c1 - range.c0)
                };
                let block = Range {
                    r0: to.r0,
                    c0: to.c0,
                    r1: to
                        .r1
                        .max(to.r0.saturating_add(h))
                        .min(kiln_sheet::MAX_ROWS - 1),
                    c1: to
                        .c1
                        .max(to.c0.saturating_add(w))
                        .min(kiln_sheet::MAX_COLS - 1),
                };
                cells(
                    (*sheet, *range),
                    to_sheet.unwrap_or(*sheet),
                    vec![block],
                    false,
                )
            }
            C::TextToColumns {
                sheet,
                range,
                options,
            } => {
                // It writes the rows of the range from its destination rightwards.
                let dest = options
                    .destination
                    .unwrap_or(kiln_sheet::CellPos::new(range.r0, range.c0));
                let rows = range.r1 - range.r0;
                let block = Range {
                    r0: dest.row,
                    c0: dest.col,
                    r1: dest.row.saturating_add(rows).min(kiln_sheet::MAX_ROWS - 1),
                    c1: kiln_sheet::MAX_COLS - 1,
                };
                cells((*sheet, *range), *sheet, vec![block], false)
            }
            C::ApplyFilter { .. } => Self::Filter,
            C::Batch { commands, .. } => {
                if commands
                    .iter()
                    .all(|c| matches!(Self::of(wb, c), Self::Command))
                {
                    Self::Command
                } else {
                    Self::Split
                }
            }
            _ => Self::Command,
        }
    }

    /// What the journal keeps of `cmd`, now that it is applied with `inverse`. A command that
    /// read values is recorded as what it did, and what it did is read off its inverse, which
    /// names every cell, sheet's metadata, layout and the names it changed: the record holds
    /// those as they are now. The prediction only says where to look when the inverse is not a
    /// restore.
    pub(super) fn outcome(
        self,
        wb: &mut Workbook,
        cmd: kiln_sheet::Command,
        inverse: &kiln_sheet::Command,
    ) -> kiln_sheet::Command {
        use kiln_sheet::Command as C;
        match self {
            Self::Command | Self::Split => cmd,
            // The rows the filter showed or hid, as they are now (its inverse names them: a
            // filter worked out again after the fact finds nothing left to change).
            Self::Filter => match inverse {
                C::SetRowEntries { sheet, entries } => C::SetRowEntries {
                    sheet: *sheet,
                    entries: entries
                        .iter()
                        .map(|(row, _)| {
                            let now = wb.sheet(*sheet).and_then(|s| s.rows().get(*row).copied());
                            (*row, now)
                        })
                        .collect(),
                },
                _ => cmd,
            },
            Self::Cells {
                sheet,
                ranges,
                before,
                meta,
            } => {
                observed(wb, inverse).unwrap_or_else(|| predicted(wb, sheet, &ranges, before, meta))
            }
        }
    }
}

/// The parts of the workbook an inverse puts back, as they are now: what its command did.
fn observed(wb: &Workbook, inverse: &kiln_sheet::Command) -> Option<kiln_sheet::Command> {
    use kiln_sheet::Command as C;
    use kiln_sheet::commands::{SheetCells, SheetLayoutEntry, SheetMetaEntry};
    let mut cells: Vec<SheetCells> = Vec::new();
    let mut metas: Vec<SheetId> = Vec::new();
    let mut layouts: Vec<SheetId> = Vec::new();
    let mut names = false;
    fn walk(
        c: &kiln_sheet::Command,
        cells: &mut Vec<SheetCells>,
        metas: &mut Vec<SheetId>,
        layouts: &mut Vec<SheetId>,
        names: &mut bool,
    ) -> bool {
        match c {
            kiln_sheet::Command::Restore {
                cells: c,
                meta,
                layout,
                names: n,
            } => {
                cells.extend(c.iter().cloned());
                metas.extend(meta.iter().map(|m| m.sheet));
                layouts.extend(layout.iter().map(|l| l.sheet));
                *names |= n.is_some();
                true
            }
            kiln_sheet::Command::Batch { commands, .. } => commands
                .iter()
                .all(|x| walk(x, cells, metas, layouts, names)),
            _ => false,
        }
    }
    if !walk(inverse, &mut cells, &mut metas, &mut layouts, &mut names) {
        return None;
    }
    let mut by_sheet: Vec<SheetCells> = Vec::new();
    for sc in cells {
        let records = sc
            .records
            .iter()
            .map(|r| wb.record(sc.sheet, r.row, r.col))
            .collect::<Vec<_>>();
        match by_sheet.iter_mut().find(|x| x.sheet == sc.sheet) {
            Some(x) => x.records.extend(records),
            None => by_sheet.push(SheetCells {
                sheet: sc.sheet,
                records,
            }),
        }
    }
    metas.dedup();
    layouts.dedup();
    Some(C::Restore {
        cells: by_sheet,
        meta: metas
            .into_iter()
            .filter_map(|sheet| {
                wb.sheet(sheet).map(|s| SheetMetaEntry {
                    sheet,
                    meta: Box::new(s.meta().clone()),
                })
            })
            .collect(),
        layout: layouts
            .into_iter()
            .filter_map(|sheet| {
                wb.sheet(sheet).map(|s| SheetLayoutEntry {
                    sheet,
                    layout: s.layout(),
                })
            })
            .collect(),
        names: names.then(|| wb.names().to_vec()),
    })
}

/// The cells a command was predicted to write, as they are now.
fn predicted(
    wb: &Workbook,
    sheet: SheetId,
    ranges: &[Range],
    before: Vec<(u32, u32)>,
    meta: bool,
) -> kiln_sheet::Command {
    use kiln_sheet::commands::{SheetCells, SheetMetaEntry};
    let mut records: Vec<_> = ranges
        .iter()
        .flat_map(|r| wb.records_in(sheet, *r))
        .collect();
    let now: rustc_hash::FxHashSet<(u32, u32)> = records.iter().map(|r| (r.row, r.col)).collect();
    records.extend(
        before
            .into_iter()
            .filter(|p| !now.contains(p))
            .map(|(row, col)| wb.record(sheet, row, col)),
    );
    let meta = if meta {
        wb.sheet(sheet)
            .map(|s| SheetMetaEntry {
                sheet,
                meta: Box::new(s.meta().clone()),
            })
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    kiln_sheet::Command::Restore {
        cells: vec![SheetCells { sheet, records }],
        meta,
        layout: Vec::new(),
        names: None,
    }
}

/// The entry being typed, for the journal.
#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Draft {
    sheet: u32,
    row: u32,
    col: u32,
    text: String,
}

impl GridDoc {
    /// A new id for a conditional format or validation rule.
    pub(super) fn new_rule_id(&mut self) -> u32 {
        self.log.alloc_rule_id(&mut self.workbook)
    }

    /// The formats added since the journal last heard, ahead of the operations that use them.
    fn formats_added(&mut self) -> Option<GridOp> {
        let table = self.workbook.styles();
        let now = (table.len(), table.dxf_count());
        let (styles, dxfs) = self.journal.styles.replace(now).unwrap_or(now);
        if (styles, dxfs) == now {
            return None;
        }
        let at = |i: usize| u32::try_from(i).unwrap_or(u32::MAX);
        Some(GridOp::Styles {
            from: at(styles),
            styles: (styles..now.0).map(|i| table.get(at(i))).collect(),
            dxf_from: at(dxfs),
            dxfs: (dxfs..now.1)
                .filter_map(|i| table.dxf(at(i)).cloned())
                .collect(),
        })
    }

    /// The operations since the last call; `None` when nothing records.
    pub(super) fn take_journal(&mut self) -> Option<Vec<Vec<u8>>> {
        let mut ops = self.log.take_recorded()?;
        if !self.log.holds(&self.workbook) {
            return self.changed_round_the_ledger();
        }
        #[cfg(any(test, feature = "journal-check"))]
        let _tick = TickAfter(self.check_sources.clone());
        let sheet = self.sid().ok().map(|s| s.0);
        // The sheet on show is a record of its own when it is all that changed.
        if ops.is_empty() && sheet == self.journal.active {
            #[cfg(any(test, feature = "journal-check"))]
            self.check_shadow(&[]);
            return Some(Vec::new());
        }
        if let Some(formats) = self.formats_added() {
            ops.insert(0, formats);
        }
        // The sheet on show, by id, with every record: a move or a removal changes its place.
        self.journal.active = sheet;
        ops.extend(sheet.map(GridOp::ActiveSheet));
        let encoded: std::result::Result<Vec<_>, _> =
            ops.iter().map(postcard::to_allocvec).collect();
        let encoded = encoded.unwrap_or_default();
        #[cfg(any(test, feature = "journal-check"))]
        self.check_shadow(&encoded);
        Some(encoded)
    }

    /// The workbook changed without the ledger (a guard on what module privacy cannot see: the
    /// stamps of `ledger.rs`). In tests and checking builds that is a bug to stop at. Otherwise
    /// the document is kept as it is: the records taken are dropped and the change goes in the
    /// journal by name, the command's, with a checkpoint at once (`storage_after`), so a
    /// recovery gives what was on screen.
    fn changed_round_the_ledger(&mut self) -> Option<Vec<Vec<u8>>> {
        let what = self.log.describe(&self.workbook);
        #[cfg(any(test, feature = "journal-check"))]
        {
            #[cfg(test)]
            let release = super::ledger::plant::release_guard();
            #[cfg(not(test))]
            let release = false;
            assert!(
                release,
                "journal check: the workbook changed outside the ledger ({what})"
            );
        }
        let _ = what;
        self.log.adopt(&mut self.workbook, &Replaying(()));
        // A change: the document's state moves, with no undo step of its own.
        self.state = self.next_state;
        self.next_state += 1;
        #[cfg(any(test, feature = "journal-check"))]
        {
            self.install_shadow();
            self.shadow_state = self.state;
        }
        None
    }

    /// The journal starts or stops: what the document has now is what its records go on from.
    pub(super) fn start_journal(&mut self, on: bool) {
        self.log.set_recording(on);
        let table = self.workbook.styles();
        self.journal.styles = Some((table.len(), table.dxf_count()));
        self.journal.active = self.sid().ok().map(|s| s.0);
        // What replays changed (a recovery's records) is the workbook the journal goes on from.
        self.log.adopt(&mut self.workbook, &Replaying(()));
        #[cfg(any(test, feature = "journal-check"))]
        {
            if on {
                self.install_shadow();
            } else {
                self.shadow = None;
            }
            self.shadow_state = self.state;
        }
    }

    /// What a journal holds of the document, to compare: every sheet's cells with their values
    /// (volatile ones too: the shadow shares the document's clock and random numbers,
    /// `ledger::CheckSources`, and a stale value under manual calculation is what was on
    /// screen), metadata, layout, order and ids, names, formats, properties, charts and
    /// pictures. The selection is the view's, not the document's.
    #[cfg(any(test, feature = "journal-check"))]
    fn journal_view(&self) -> JournalView {
        let mut snap = self.workbook.snapshot();
        for sheet in &mut snap.sheets {
            sheet.meta.view.selection = Default::default();
        }
        snap.props.active_sheet = 0;
        let mut charts: Vec<(String, String)> = self
            .charts
            .to_write()
            .into_iter()
            .map(|(p, c)| {
                let xml = kiln_charts::write::write_chart(&c);
                (p, without_cached_values(&String::from_utf8_lossy(&xml)))
            })
            .collect();
        charts.sort();
        JournalView {
            snap,
            charts,
            media: self.features.objects.media(),
            props: self.props.clone(),
        }
    }

    /// Checking builds: the records just taken, replayed onto a shadow copy, give the document
    /// as it is. Anything that changed the document without a record, or a record that does not
    /// say what happened, stops here.
    #[cfg(any(test, feature = "journal-check"))]
    fn check_shadow(&mut self, ops: &[Vec<u8>]) {
        #[cfg(test)]
        if super::ledger::plant::no_shadow() {
            return;
        }
        if ops.is_empty() && self.state == self.shadow_state {
            return;
        }
        let Some(mut shadow) = self.shadow.take() else {
            return;
        };
        shadow.journal.format = self.journal.format;
        let sources = self.check_sources.clone();
        let tape = |on: bool| {
            if let Some(s) = &sources {
                s.use_tape(on);
            }
        };
        let manual =
            |s: &GridDoc| s.workbook.props().calc.mode == kiln_sheet::meta::CalcMode::Manual;
        let before = manual(&shadow);
        for op in ops {
            // A recalculation shows what the document's did, and so does anything under manual
            // calculation; a command draws numbers of its own.
            let recalc = self.journal.format == 0
                && matches!(postcard::from_bytes(op), Ok(GridOp::Recalc(_)));
            tape(manual(&shadow) || recalc);
            if let Err(e) = shadow.apply_journal(op, &Replaying(())) {
                tape(false);
                panic!("journal check: a record does not replay: {e}");
            }
        }
        let after = manual(&shadow);
        // The volatile cells with the document's numbers, unless the shadow was under manual
        // calculation all along (it holds them already).
        tape(true);
        if !before || !after {
            super::ledger::Ledger::replay_refresh(&mut shadow.workbook, &Replaying(()));
            shadow.absorb_changes();
        }
        self.shadow_state = self.state;
        let (live, back) = (self.journal_view(), shadow.journal_view());
        if live != back {
            panic!(
                "journal check: the records do not give the document as it is ({})",
                live.differences(&back).join(", ")
            );
        }
        if shadow.sid().ok() != self.sid().ok() {
            panic!("journal check: the records do not give the sheet on show");
        }
        // The shadow's volatile cells take numbers of their own again, as a recovery's would.
        tape(false);
        if !after {
            super::ledger::Ledger::replay_refresh(&mut shadow.workbook, &Replaying(()));
            shadow.absorb_changes();
        }
        self.shadow = Some(shadow);
    }
}

/// A chart's DrawingML without the values it caches from cells (they follow the cells, which are
/// compared, and `RAND` gives them anew).
#[cfg(any(test, feature = "journal-check"))]
pub(crate) fn without_cached_values(xml: &str) -> String {
    let mut out = xml.to_owned();
    for (open, close) in [
        ("<c:numCache>", "</c:numCache>"),
        ("<c:strCache>", "</c:strCache>"),
        ("<c:multiLvlStrCache>", "</c:multiLvlStrCache>"),
    ] {
        let mut kept = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(i) = rest.find(open) {
            kept.push_str(&rest[..i]);
            rest = rest[i..]
                .find(close)
                .map_or("", |j| &rest[i + j + close.len()..]);
        }
        kept.push_str(rest);
        out = kept;
    }
    out
}

/// Tests: the text around the first place two texts differ.
#[cfg(test)]
pub(crate) fn first_difference(x: &str, y: &str) -> String {
    near_difference(x, y)
}

/// The text around the first place two texts differ.
#[cfg(any(test, feature = "journal-check"))]
fn near_difference(x: &str, y: &str) -> String {
    let at = x
        .bytes()
        .zip(y.bytes())
        .position(|(a, b)| a != b)
        .unwrap_or_else(|| x.len().min(y.len()));
    let near = |t: &str| {
        let (mut from, mut to) = (at.saturating_sub(80), (at + 80).min(t.len()));
        while !t.is_char_boundary(from) {
            from -= 1;
        }
        while to < t.len() && !t.is_char_boundary(to) {
            to += 1;
        }
        t.get(from..to).unwrap_or("").to_owned()
    };
    format!("…{}… against …{}…", near(x), near(y))
}

#[cfg(any(test, feature = "journal-check"))]
thread_local! {
    /// A shadow copy is being made: it has no shadow of its own.
    pub(super) static MAKING_SHADOW: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Checking builds: moves the shared clock on as the journal's turn ends.
#[cfg(any(test, feature = "journal-check"))]
struct TickAfter(Option<std::sync::Arc<super::ledger::CheckSources>>);

#[cfg(any(test, feature = "journal-check"))]
impl Drop for TickAfter {
    fn drop(&mut self) {
        if let Some(s) = &self.0 {
            s.tick();
        }
    }
}

/// What a replay of the journal holds, to call the ledger's replay entry points: only the
/// replay below ([`GridDoc::apply_journal`]) makes one (its field is private to this module), so
/// an edit cannot use them to go round the journal.
#[derive(Debug)]
pub struct Replaying(());

#[cfg(any(test, feature = "ownership-escape"))]
impl Replaying {
    /// The planted bypass B4 (`ledger::escape`): a door the type keeps shut otherwise.
    pub(super) fn for_plant() -> Self {
        Self(())
    }
}

/// The first cells two sheets differ in: `Sheet2!A5: Number(10.0) against Number(4.0)`.
#[cfg(any(test, feature = "journal-check"))]
fn cells_differ(
    x: &kiln_sheet::serial::SheetSnapshot,
    y: &kiln_sheet::serial::SheetSnapshot,
) -> String {
    fn at(
        s: &kiln_sheet::serial::SheetSnapshot,
    ) -> std::collections::BTreeMap<(u32, u32), &kiln_sheet::CellRecord> {
        s.cells.iter().map(|c| ((c.row, c.col), c)).collect()
    }
    let (a, b) = (at(x), at(y));
    let mut keys: Vec<_> = a.keys().chain(b.keys()).copied().collect();
    keys.sort_unstable();
    keys.dedup();
    let show = |c: Option<&&kiln_sheet::CellRecord>| match c {
        None => "nothing".to_owned(),
        Some(c) => match &c.formula {
            Some(f) => format!("={f} showing {:?}", c.value),
            None => format!("{:?}", c.value),
        },
    };
    keys.into_iter()
        .filter(|k| a.get(k) != b.get(k))
        .take(5)
        .map(|(row, col)| {
            format!(
                "{}!{}: {} against {}",
                x.name,
                kiln_sheet::address::cell_a1(row, col, false),
                show(a.get(&(row, col))),
                show(b.get(&(row, col)))
            )
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// What [`GridDoc::journal_view`] compares.
#[cfg(any(test, feature = "journal-check"))]
#[derive(PartialEq)]
struct JournalView {
    snap: kiln_sheet::serial::WorkbookSnapshot,
    charts: Vec<(String, String)>,
    media: Vec<(String, Vec<u8>)>,
    props: Option<UserProps>,
}

#[cfg(any(test, feature = "journal-check"))]
impl JournalView {
    /// What differs, in words.
    fn differences(&self, other: &Self) -> Vec<String> {
        let (a, b) = (&self.snap, &other.snap);
        let mut what = Vec::new();
        if a.sheets.len() != b.sheets.len() {
            what.push("the sheets".to_owned());
        }
        for (x, y) in a.sheets.iter().zip(&b.sheets) {
            if x.name != y.name || x.id != y.id {
                what.push("sheet names or ids".to_owned());
            }
            if x.cells != y.cells {
                what.push(format!("the cells of {} ({})", x.name, cells_differ(x, y)));
            }
            if x.meta != y.meta {
                what.push(format!(
                    "the metadata of {} ({})",
                    x.name,
                    near_difference(&format!("{:?}", x.meta), &format!("{:?}", y.meta))
                ));
            }
            if x.layout != y.layout {
                what.push(format!(
                    "the rows and columns of {} ({})",
                    x.name,
                    near_difference(&format!("{:?}", x.layout), &format!("{:?}", y.layout))
                ));
            }
        }
        if a.names != b.names {
            what.push("names".to_owned());
        }
        if a.styles != b.styles {
            what.push("formats".to_owned());
        }
        if a.props != b.props || a.counters != b.counters {
            what.push("workbook properties or ids".to_owned());
        }
        if self.charts != other.charts {
            let first = self
                .charts
                .iter()
                .zip(&other.charts)
                .find(|(x, y)| x != y)
                .map(|((p, x), (_, y))| format!("{p}: {}", near_difference(x, y)))
                .unwrap_or_else(|| {
                    format!(
                        "{} charts against {}",
                        self.charts.len(),
                        other.charts.len()
                    )
                });
            what.push(format!("charts ({first})"));
        }
        if self.media != other.media {
            what.push("pictures".to_owned());
        }
        if self.props != other.props {
            what.push("document properties".to_owned());
        }
        if what.is_empty() {
            what.push("theme, protection, people or lists".to_owned());
        }
        what
    }
}

/// A recovery's replay of one record ([`crate::DocModel::apply_journal`]).
pub(super) fn replay(doc: &mut GridDoc, op: &[u8], _: &crate::JournalReplay) -> Result<()> {
    doc.apply_journal(op, &Replaying(()))
}

impl GridDoc {
    /// Applies one recorded operation: a recovery's replay, or the shadow check's. It takes a
    /// [`Replaying`], which only this module makes, so no command can feed it a record.
    pub(super) fn apply_journal(&mut self, op: &[u8], _: &Replaying) -> Result<()> {
        #[cfg(test)]
        if super::ledger::plant::replay_fails() {
            return Err(err("planted: this record does not apply"));
        }
        let op = if self.journal.format != 0 && self.journal.format <= FORMAT_BARE_COMMANDS {
            GridOp::Command(kiln_sheet::Command::from_bytes(op).map_err(err)?)
        } else {
            postcard::from_bytes(op).map_err(err)?
        };
        match op {
            GridOp::Command(command) => {
                self.log
                    .replay(&mut self.workbook, &command, &Replaying(()))
                    .map_err(err)?;
            }
            GridOp::Styles {
                from,
                styles,
                dxf_from,
                dxfs,
            } => {
                let table =
                    super::ledger::Ledger::replay_styles(&mut self.workbook, &Replaying(()));
                if table.len() != from as usize || table.dxf_count() != dxf_from as usize {
                    return Err(err("the formats do not follow on from the checkpoint"));
                }
                for style in &styles {
                    table.intern(style);
                }
                for dxf in dxfs {
                    table.intern_dxf(dxf);
                }
            }
            GridOp::RuleIds(count) => {
                super::ledger::Ledger::replay_rule_ids(&mut self.workbook, count, &Replaying(()));
            }
            GridOp::Persons(people) => {
                super::ledger::Ledger::replay_persons(&mut self.workbook, people, &Replaying(()));
            }
            GridOp::CustomLists(lists) => {
                super::ledger::Ledger::replay_custom_lists(
                    &mut self.workbook,
                    lists,
                    &Replaying(()),
                );
            }
            GridOp::Recalc(sheet) => {
                super::ledger::Ledger::replay_recalc(&mut self.workbook, sheet, &Replaying(()))
            }
            GridOp::Step => {}
            GridOp::Chart { part, xml } => {
                let chart = kiln_charts::read::read_chart(&xml).map_err(err)?;
                self.restore_chart(&part, chart);
            }
            GridOp::Media { part, bytes } => {
                self.features.objects.restore_media([(part, bytes)]);
            }
            GridOp::Properties(props) => self.props = Some(props),
            GridOp::ActiveSheet(sheet) => {
                if let Some(i) = self
                    .workbook
                    .sheets()
                    .iter()
                    .position(|s| s.id().0 == sheet)
                {
                    self.activate(i);
                }
            }
        }
        self.absorb_changes();
        self.features_after();
        Ok(())
    }

    /// The entry being typed, when it differs from the cell it edits and from the last call.
    pub(super) fn take_draft(&mut self) -> Option<Option<Vec<u8>>> {
        if self.edit.is_none() && self.journal.draft.is_none() {
            return None;
        }
        let now = self.edit.as_ref().and_then(|e| {
            if e.text == self.edit_text(e.sheet, e.row, e.col) {
                return None;
            }
            serde_json::to_vec(&Draft {
                sheet: e.sheet.0,
                row: e.row,
                col: e.col,
                text: e.text.clone(),
            })
            .ok()
        });
        if now == self.journal.draft {
            return None;
        }
        // Checking builds: the draft says what is being typed, where.
        #[cfg(any(test, feature = "journal-check"))]
        if let (Some(e), Some(bytes)) = (self.edit.as_ref(), now.as_deref()) {
            let back = self.describe_draft(bytes);
            assert!(
                back.as_ref().is_some_and(|d| d.text == e.text
                    && d.sheet == e.sheet.0
                    && d.row == e.row
                    && d.col == e.col),
                "journal check: the draft does not say what is being typed ({back:?})"
            );
        }
        self.journal.draft.clone_from(&now);
        Some(now)
    }

    /// Where an interrupted entry was and what it held: `Sheet1!F8` and its text.
    pub(super) fn describe_draft(&self, bytes: &[u8]) -> Option<kiln_protocol::RecoveredDraft> {
        let d: Draft = serde_json::from_slice(bytes).ok()?;
        let sheet = self
            .workbook
            .sheets()
            .iter()
            .find(|s| s.id().0 == d.sheet)?;
        let name = super::analysis::quote_sheet(sheet.name());
        let cell = kiln_sheet::address::cell_a1(d.row, d.col, false);
        Some(kiln_protocol::RecoveredDraft {
            cell: format!("{name}!{cell}"),
            text: d.text,
            sheet: d.sheet,
            row: d.row,
            col: d.col,
        })
    }

    /// Put it back: the entry goes back into its cell, on the sheet with that id whatever it is
    /// called now, not yet entered.
    pub(super) fn put_back_draft(
        &mut self,
        sheet: u32,
        row: u32,
        col: u32,
        text: &str,
    ) -> Result<crate::model::Change> {
        use kiln_protocol::commands::GridCommand as G;
        let index = self
            .workbook
            .sheets()
            .iter()
            .position(|s| s.id().0 == sheet && !s.meta().chart_sheet)
            .filter(|_| row < kiln_sheet::MAX_ROWS && col < kiln_sheet::MAX_COLS)
            .ok_or_else(|| KilnError::not_found("the sheet the text was typed on"))?;
        self.grid(&G::ActivateSheet {
            index: u32::try_from(index).unwrap_or(u32::MAX),
        })?;
        self.grid(&G::SelectCell {
            row,
            col,
            extend: false,
            add: false,
        })?;
        self.grid(&G::BeginEdit {
            text: text.to_owned(),
        })
    }

    /// The workbook as a bundle.
    pub(super) fn to_bundle(&self) -> Result<Bundle> {
        let mut b = Bundle::new(DocKind::Grid, self.workbook.to_bytes().map_err(err)?);
        if let Some(src) = &self.source {
            b.passthrough
                .insert(ORIGINAL.into(), src.package().save().map_err(err)?);
        }
        for (part, chart) in self.charts.to_write() {
            b.passthrough.insert(
                format!("{CHARTS}{}", part.trim_start_matches('/')),
                kiln_charts::write::write_chart(&chart),
            );
        }
        // Part names start with a slash; a bundle's entries do not.
        b.media.extend(
            self.features
                .objects
                .media()
                .into_iter()
                .map(|(part, bytes)| (part.trim_start_matches('/').to_owned(), bytes)),
        );
        if let Some(p) = &self.props {
            p.to_bundle(&mut b);
        }
        Ok(b)
    }
}

/// One encoded sample of each kind of operation, and of the first and last sheet commands.
#[cfg(test)]
pub(super) fn encoded_samples() -> Vec<Vec<u8>> {
    use kiln_sheet::Command as C;
    use kiln_sheet::commands::CellInput;
    let sheet = SheetId(1);
    let ops = [
        GridOp::Command(C::SetCells {
            sheet,
            cells: vec![CellInput::value(1, 2, 3.0)],
        }),
        GridOp::Command(C::Restore {
            cells: Vec::new(),
            meta: Vec::new(),
            layout: Vec::new(),
            names: None,
        }),
        GridOp::Command(C::Batch {
            commands: Vec::new(),
            label: None,
        }),
        GridOp::Styles {
            from: 1,
            styles: Vec::new(),
            dxf_from: 2,
            dxfs: Vec::new(),
        },
        GridOp::RuleIds(1),
        GridOp::Persons(Vec::new()),
        GridOp::CustomLists(Vec::new()),
        GridOp::Chart {
            part: "p".into(),
            xml: vec![1],
        },
        GridOp::Media {
            part: "m".into(),
            bytes: vec![2],
        },
        GridOp::ActiveSheet(3),
    ];
    ops.iter()
        .map(|op| postcard::to_allocvec(op).unwrap_or_default())
        .collect()
}
