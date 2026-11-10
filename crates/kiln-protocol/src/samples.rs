// SPDX-License-Identifier: AGPL-3.0-or-later
//! Representative values of every protocol type.
//!
//! `cargo xtask protocol` encodes them as the postcard test vectors that the TypeScript decoder is
//! tested against, and the unit tests use them to check that every command has metadata. Values
//! are chosen to exercise the encoding: multi-byte varints, negative and fractional floats,
//! non-ASCII text, empty and non-empty collections, `None` and `Some`.

use kiln_common::{DocId, ErrorKind, NodeId};

use crate::OpenHint;
use crate::a11y::{A11yNode, A11yRevision, A11yRevisionDisplay, A11yRole, A11yTree};
use crate::app::{App, DocKind, FileFormat};
use crate::commands::{
    Availability, CaretMotion, ChartAreaState, ChartAreaTarget, ChartAxisState, ChartAxisTarget,
    ChartBlanks, ChartBound, ChartElementKind, ChartKind, ChartLabelPosition, ChartLegendPosition,
    ChartMoveTarget, ChartSeriesOp, ChartSeriesState, ChartState, Command, CommandInfo, SelectUnit,
    SuiteCommand, Tier, WriteCommand, catalog,
};
use crate::error::ErrorInfo;
use crate::event::{Event, Progress, RecoveryEntry};
use crate::exec::{CoreInfo, ExecResult, ExecValue, Outcome};
use crate::geom::{Point, Rect, Size};
use crate::host::{
    FileFilter, FileStamp, HostOp, HostReply, HostRequest, RecentFile, RequestId, StoreArea,
};
use crate::print::{
    PdfFile, PrintMargins, PrintOrientation, PrintPaper, PrintParity, PrintPreview, PrintSettings,
    SlideLayout, SlidePrint,
};
use crate::query::{CompatIssue, CompatOrigin, CompatSeverity, ParagraphText, Query, QueryResult};
use crate::selection::{
    Caret, CellAddr, CellRange, Compass, Handle, HandleKind, SelKind, Selection, TextPos,
};
use crate::status::{CellStats, Position, StatusBar};
use crate::summary::DocSummary;
use crate::viewport::{ViewMode, Viewport};

fn compat_issue() -> CompatIssue {
    CompatIssue {
        code: "image.missing".into(),
        severity: CompatSeverity::Loss,
        message: "Pictures whose image data is not available are not saved — «Ünïcödé».".into(),
        count: 300,
        origin: CompatOrigin::Save,
    }
}

/// Types with representative sample values.
pub trait Samples: Sized {
    /// The samples.
    fn samples() -> Vec<Self>;
}

/// One sample of every command (each catalog id at least once).
#[must_use]
pub fn commands() -> Vec<Command> {
    vec![
        SuiteCommand::Open { path: None }.into(),
        SuiteCommand::Open {
            path: Some("/data/ana/Quarterly report.docx".into()),
        }
        .into(),
        SuiteCommand::New {
            kind: DocKind::Scratch,
            language: None,
        }
        .into(),
        SuiteCommand::New {
            kind: DocKind::Write,
            language: Some("ar-SA".into()),
        }
        .into(),
        SuiteCommand::Save.into(),
        SuiteCommand::SaveAs {
            path: Some("C:\\Users\\Zoë\\notes.txt".into()),
            format: Some(FileFormat::Txt),
        }
        .into(),
        SuiteCommand::Close.into(),
        SuiteCommand::Undo.into(),
        SuiteCommand::Redo.into(),
        SuiteCommand::SetZoom { zoom: 1.25 }.into(),
        SuiteCommand::SetViewMode {
            mode: ViewMode::Draft,
        }
        .into(),
        SuiteCommand::Ping.into(),
        SuiteCommand::Echo {
            text: "héllo, 世界 👋".into(),
        }
        .into(),
        WriteCommand::InsertText {
            text: "Kiln\nWrite".into(),
        }
        .into(),
        WriteCommand::DeleteBackward.into(),
        WriteCommand::MoveCaret {
            motion: CaretMotion::WordBackward,
            extend: true,
        }
        .into(),
        WriteCommand::SelectTo {
            pos: text_pos(300, 70_000),
            extend: false,
        }
        .into(),
        SuiteCommand::Repaint.into(),
        WriteCommand::DeleteForward.into(),
        WriteCommand::DeleteWordBackward.into(),
        WriteCommand::DeleteWordForward.into(),
        WriteCommand::SelectAll.into(),
        WriteCommand::SelectAt {
            point: Point::new(412.5, -3.25),
            extend: true,
            unit: SelectUnit::Word,
        }
        .into(),
        WriteCommand::SetComposition {
            text: "にほんご".into(),
            caret: 2,
        }
        .into(),
        WriteCommand::CommitComposition {
            text: "日本語".into(),
        }
        .into(),
        WriteCommand::CancelComposition.into(),
        // chart commands.
        SuiteCommand::ChartInsert {
            kind: Some(ChartKind::ComboColumnLineSecondary),
        }
        .into(),
        SuiteCommand::ChartInsert { kind: None }.into(),
        SuiteCommand::ChartChangeType {
            kind: ChartKind::Sunburst,
        }
        .into(),
        SuiteCommand::ChartSwitchRowColumn.into(),
        SuiteCommand::ChartSelectData {
            range: "'Ventes Q1 «été»'!$A$1:$D$300".into(),
        }
        .into(),
        SuiteCommand::ChartSetElement {
            element: ChartElementKind::PrimaryVerticalAxisTitle,
            on: true,
        }
        .into(),
        SuiteCommand::ChartSetLegend {
            position: Some(ChartLegendPosition::TopRight),
        }
        .into(),
        SuiteCommand::ChartSetDataLabels { position: None }.into(),
        SuiteCommand::ChartQuickLayout { layout: 11 }.into(),
        SuiteCommand::ChartSetStyle { style: 48 }.into(),
        SuiteCommand::ChartSetColors { palette: 7 }.into(),
        SuiteCommand::ChartSetTitle {
            text: "Spend by quarter — 2026 ✦".into(),
        }
        .into(),
        SuiteCommand::ChartMove {
            to: ChartMoveTarget::NewSheet {
                name: "Chart1".into(),
            },
        }
        .into(),
        SuiteCommand::ChartFormatSeries {
            series: 70_000,
            fill: Some("1D8A60".into()),
            line: Some(String::new()),
            line_width: Some(2.25),
            secondary: None,
        }
        .into(),
        SuiteCommand::ChartFormatAxis {
            axis: ChartAxisTarget::SecondaryValue,
            min: Some(ChartBound::Fixed(-12.5)),
            max: Some(ChartBound::Auto),
            major_unit: Some(ChartBound::Fixed(0.25)),
            log_base: None,
            reversed: Some(true),
            number_format: Some("\"£\"#,##0".into()),
        }
        .into(),
        SuiteCommand::ChartEditSeries {
            op: ChartSeriesOp::Add {
                name: "='Ventes Q1 «été»'!$E$1".into(),
                values: "='Ventes Q1 «été»'!$E$2:$E$300".into(),
            },
        }
        .into(),
        SuiteCommand::ChartEditSeries {
            op: ChartSeriesOp::Move {
                series: 2,
                up: true,
            },
        }
        .into(),
        SuiteCommand::ChartHiddenEmpty {
            blanks: ChartBlanks::Span,
            show_hidden: true,
        }
        .into(),
        SuiteCommand::ChartFormatArea {
            area: ChartAreaTarget::PlotArea,
            fill: Some("F3ECE3".into()),
            line: Some("none".into()),
            line_width: Some(0.75),
            shadow: Some(true),
            rounded: None,
        }
        .into(),
        SuiteCommand::PrintPdf {
            settings: print_settings(),
        }
        .into(),
        SuiteCommand::SavePdf {
            path: None,
            settings: PrintSettings::default(),
        }
        .into(),
        SuiteCommand::SavePdf {
            path: Some("/data/ana/Quarterly report.pdf".into()),
            settings: print_settings(),
        }
        .into(),
        SuiteCommand::OpenWindow {
            window: crate::commands::DocWindow::Audience,
        }
        .into(),
        // collaboration and Version History.
        SuiteCommand::Share {
            server: "wss://collab.example.org".into(),
            token: "s3cret".into(),
            name: "Ana Martín".into(),
        }
        .into(),
        SuiteCommand::JoinShared {
            kind: DocKind::Write,
            server: "ws://127.0.0.1:8640".into(),
            room: "k7p2q9xw".into(),
            token: "s3cret".into(),
            name: "Zoë".into(),
        }
        .into(),
        SuiteCommand::StopSharing.into(),
        SuiteCommand::VersionPreview {
            id: Some("v2".into()),
        }
        .into(),
        SuiteCommand::VersionPreview { id: None }.into(),
        SuiteCommand::VersionRestore { id: "v2".into() }.into(),
        SuiteCommand::VersionRename {
            id: "v2".into(),
            name: "Before the edits".into(),
        }
        .into(),
        SuiteCommand::RecoveryScan.into(),
        SuiteCommand::RecoveryOpen {
            key: "k1x9".into(),
            kind: DocKind::Grid,
        }
        .into(),
        SuiteCommand::RecoveryDelete { key: "k1x9".into() }.into(),
        SuiteCommand::SetAutoSave { on: true }.into(),
        SuiteCommand::RecentsLoad.into(),
        SuiteCommand::RecentPin {
            path: "/data/ana/Budget.xlsx".into(),
            pinned: true,
        }
        .into(),
        SuiteCommand::RecentRemove {
            path: "/data/ana/Budget.xlsx".into(),
        }
        .into(),
        SuiteCommand::Reload.into(),
        SuiteCommand::KeepMine.into(),
        SuiteCommand::CloseWithoutSaving.into(),
        SuiteCommand::SetOptions {
            options: crate::commands::CoreOptions::default(),
        }
        .into(),
        SuiteCommand::SetOptions {
            options: crate::commands::CoreOptions {
                autosave_delay_s: 30,
                recovery: false,
                spell_ignore_uppercase: false,
                spell_ignore_numbers: true,
                spell_ignore_addresses: false,
                ui_language: "ar".into(),
            },
        }
        .into(),
        SuiteCommand::SetProperties {
            props: Box::new(crate::commands::write_features::PropertiesEdit {
                title: Some("Kiln «Ünïcödé» تقرير".into()),
                company: Some(String::new()),
                ..Default::default()
            }),
        }
        .into(),
        SuiteCommand::InspectRemove {
            item: crate::query::InspectKind::Comments,
        }
        .into(),
        SuiteCommand::InspectRemove {
            item: crate::query::InspectKind::SpeakerNotes,
        }
        .into(),
    ]
    .into_iter()
    // the Write editor and the shared editing commands.
    .chain(crate::commands::write_samples::commands())
    // Kiln Grid.
    .chain(crate::commands::grid_samples::commands())
    // Kiln Deck.
    .chain(crate::commands::deck_samples::commands())
    .collect()
}

fn stamp() -> FileStamp {
    FileStamp {
        size: 18_204,
        modified_ms: 1_792_000_000_000,
    }
}

fn print_settings() -> PrintSettings {
    PrintSettings {
        pages: "1-3, 5, 8-".into(),
        parity: PrintParity::Odd,
        copies: 300,
        paper: Some(PrintPaper::A4),
        scale: 0.75,
        per_sheet: 4,
        orientation: Some(PrintOrientation::Landscape),
        margins: Some(PrintMargins::Narrow),
        grayscale: true,
        markup: false,
        hidden_text: true,
        tagged: true,
        bookmarks: false,
        slides: Some(SlidePrint {
            layout: SlideLayout::Handouts,
            per_page: 3,
            vertical: true,
            frame: true,
            hidden: false,
        }),
    }
}

fn print_preview() -> PrintPreview {
    PrintPreview {
        sheets: 12,
        sheet: 1,
        width_pt: 612.0,
        height_pt: 792.0,
        pages: vec![2, 3],
        page_count: 300,
        scene: crate::scene::samples::small_delta(),
    }
}

fn text_pos(block: u64, offset: u32) -> TextPos {
    TextPos {
        block: NodeId(block),
        offset,
    }
}

fn rect() -> Rect {
    Rect::new(-12.5, 96.0, 0.75, 18.0)
}

fn summary() -> DocSummary {
    DocSummary {
        doc: DocId(7),
        kind: DocKind::Scratch,
        app: App::Write,
        title: "Scratch1".into(),
        path: Some("/tmp/scratch.txt".into()),
        format: Some(FileFormat::Txt),
        revision: 1_234_567_890_123,
        dirty: true,
        read_only: false,
        can_undo: true,
        can_redo: false,
        undo_label: Some("write.edit.insert_text".into()),
        redo_label: None,
        save_formats: vec![FileFormat::Txt, FileFormat::Md],
        autosave: crate::summary::AutoSave::Unavailable("Save the document first".into()),
    }
}

fn selection() -> Selection {
    Selection {
        kind: SelKind::Text {
            anchor: text_pos(1, 0),
            head: text_pos(2, 5),
        },
        rects: vec![rect(), Rect::new(0.0, 0.0, 1e6, 3.4e-3)],
        caret: Some(Caret {
            rect: rect(),
            rtl: true,
        }),
        handles: vec![
            Handle {
                kind: HandleKind::SelectionStart,
                rect: rect(),
            },
            Handle {
                kind: HandleKind::Resize(Compass::SW),
                rect: rect(),
            },
        ],
    }
}

fn status() -> StatusBar {
    StatusBar {
        page: Some(Position {
            current: 3,
            total: 12,
        }),
        words: Some(1_284),
        chars: Some(7_311),
        language: Some("en-GB".into()),
        cells: Some(CellStats {
            count: 10,
            numeric_count: 9,
            sum: 1_284.0,
            average: Some(142.666_666_666_666_66),
            min: Some(-3.5),
            max: None,
        }),
        slide: None,
        zoom: 1.5,
        view: ViewMode::PrintLayout,
    }
}

fn a11y() -> A11yTree {
    A11yTree {
        nodes: vec![
            A11yNode {
                id: NodeId(1),
                role: A11yRole::Document,
                name: Some("Scratch1".into()),
                text: None,
                bounds: None,
                level: None,
                children: vec![NodeId(2), NodeId(3)],
                objects: None,
                revisions: None,
            },
            A11yNode {
                id: NodeId(2),
                role: A11yRole::Heading,
                name: None,
                text: Some("Résumé".into()),
                bounds: Some(rect()),
                level: Some(1),
                children: vec![],
                objects: Some(vec!["1".into()]),
                revisions: Some(vec![A11yRevision {
                    start: 0,
                    end: 1,
                    inserted: false,
                    display: A11yRevisionDisplay::Marked,
                }]),
            },
        ],
        active: Some(NodeId(2)),
        announce: Some("Bold on".into()),
        caret: Some(text_pos(2, 3)),
    }
}

fn error() -> ErrorInfo {
    ErrorInfo {
        kind: ErrorKind::Unavailable,
        message: "write.edit.insert_text is not available: read-only".into(),
    }
}

impl Samples for Command {
    fn samples() -> Vec<Self> {
        commands()
    }
}

impl Samples for SuiteCommand {
    fn samples() -> Vec<Self> {
        commands()
            .into_iter()
            .filter_map(|c| match c {
                Command::Suite(s) => Some(s),
                _ => None,
            })
            .collect()
    }
}

impl Samples for WriteCommand {
    fn samples() -> Vec<Self> {
        commands()
            .into_iter()
            .filter_map(|c| match c {
                Command::Write(w) => Some(w),
                _ => None,
            })
            .collect()
    }
}

impl Samples for CommandInfo {
    fn samples() -> Vec<Self> {
        catalog()
    }
}

macro_rules! samples {
    ($($ty:ty => $body:expr;)*) => {
        $(impl Samples for $ty {
            fn samples() -> Vec<Self> {
                $body
            }
        })*
    };
}
pub(crate) use samples as samples_for;

samples! {
    Point => vec![Point::new(0.0, -1.5), Point::new(16_384.25, 1e-3)];
    Size => vec![Size { width: 1440.0, height: 900.0 }];
    Rect => vec![rect(), Rect::default()];
    App => App::ALL.to_vec();
    DocKind => vec![DocKind::Scratch, DocKind::Write, DocKind::Grid, DocKind::Deck, DocKind::Notes];
    FileFormat => FileFormat::ALL.to_vec();
    Tier => vec![Tier::P0, Tier::P1, Tier::P2];
    Availability => vec![Availability::Always, Availability::Document, Availability::Editable,
        Availability::CanUndo, Availability::CanRedo, Availability::Selection];
    CaretMotion => vec![CaretMotion::Backward, CaretMotion::DocumentEnd, CaretMotion::PageDown, CaretMotion::Left, CaretMotion::WordRight];
    SelectUnit => vec![SelectUnit::Char, SelectUnit::Paragraph];
    ViewMode => vec![ViewMode::PrintLayout, ViewMode::ReadingView];
    Viewport => vec![Viewport::default(), Viewport {
        width_px: 1440.0, height_px: 900.0, scroll: Point::new(0.0, 12_345.5),
        zoom: 1.1, dpr: 2.0, mode: ViewMode::Normal,
    }];
    TextPos => vec![text_pos(1, 0), text_pos(u64::from(u32::MAX) + 7, u32::MAX)];
    CellAddr => vec![CellAddr { sheet: 2, row: 1_048_575, col: 16_383 }];
    CellRange => vec![CellRange {
        start: CellAddr { sheet: 0, row: 0, col: 0 },
        end: CellAddr { sheet: 0, row: 9, col: 3 },
    }];
    SelKind => vec![SelKind::None, selection().kind, SelKind::Shapes { ids: vec![NodeId(5), NodeId(900)] },
        SelKind::Cells {
            ranges: vec![CellRange {
                start: CellAddr { sheet: 0, row: 1, col: 1 },
                end: CellAddr { sheet: 0, row: 10, col: 1 },
            }],
            active: CellAddr { sheet: 0, row: 1, col: 1 },
        }];
    Caret => vec![Caret { rect: rect(), rtl: false }];
    HandleKind => vec![HandleKind::Fill, HandleKind::Resize(Compass::NE), HandleKind::Rotate];
    Compass => vec![Compass::N, Compass::NW];
    Handle => vec![Handle { kind: HandleKind::SelectionEnd, rect: rect() }];
    Selection => vec![Selection::default(), selection()];
    Position => vec![Position { current: 1, total: 1 }];
    CellStats => status().cells.into_iter().collect();
    StatusBar => vec![StatusBar::default(), status()];
    DocSummary => vec![summary()];
    Query => vec![Query::Documents, Query::Summary, Query::Paragraphs, Query::Selection,
        Query::Status, Query::A11y, Query::A11yDocument, Query::SelectedText,
        Query::CompatReport { format: None },
        Query::CompatReport { format: Some(FileFormat::Docx) },
        Query::PrintPreview { settings: print_settings(), sheet: 1, scale: 1.5 },
        Query::Pdf { settings: PrintSettings::default() },
        Query::Write(crate::commands::WriteQuery::Rulers),
        Query::Grid(crate::commands::GridQuery::State),
        Query::Deck(crate::commands::DeckQuery::State),
        Query::Collab, Query::Versions,
        Query::VersionCompare { id: "v2".into() },
        Query::VersionSlides { id: "v2".into(), width: 160.0, dpr: 2.0 },
        Query::Recents, Query::Properties, Query::Inspect];
    CompatSeverity => vec![CompatSeverity::Loss, CompatSeverity::Change, CompatSeverity::Info];
    CompatOrigin => vec![CompatOrigin::Save, CompatOrigin::Open];
    CompatIssue => vec![compat_issue()];
    ParagraphText => vec![ParagraphText { id: NodeId(3), text: "Ünïcödé ✓".into() }];
    QueryResult => vec![
        QueryResult::Documents(vec![summary()]),
        QueryResult::Summary(summary()),
        QueryResult::Paragraphs(vec![
            ParagraphText { id: NodeId(1), text: "First".into() },
            ParagraphText { id: NodeId(2), text: String::new() },
        ]),
        QueryResult::Selection(selection()),
        QueryResult::Status(status()),
        QueryResult::A11y(a11y()),
        QueryResult::Text("Kiln\nWrite".into()),
        QueryResult::CompatReport(vec![]),
        QueryResult::CompatReport(vec![
            compat_issue(),
            CompatIssue {
                code: "drawing.smartArt".into(),
                severity: CompatSeverity::Info,
                message: "SmartArt is kept in the file but shown as a placeholder.".into(),
                count: 1,
                origin: CompatOrigin::Open,
            },
        ]),
        QueryResult::PrintPreview(print_preview()),
        QueryResult::Pdf(PdfFile {
            name: "Quarterly report «Ünïcödé».pdf".into(),
            bytes: b"%PDF-1.7".to_vec(),
            pdf_ua: false,
            ua_issues: vec!["a figure has no alt text".into()],
        }),
        QueryResult::Write(crate::commands::WriteQueryResult::Bookmarks(vec!["intro".into()])),
        QueryResult::Grid(crate::commands::GridQueryResult::Zoom(0.75)),
        QueryResult::Deck(crate::commands::DeckQueryResult::Cursor(crate::commands::DeckCursor::Text)),
        QueryResult::Collab(crate::collab::CollabState::samples().remove(0)),
        QueryResult::Versions(crate::collab::VersionEntry::samples()),
        QueryResult::VersionCompare(crate::collab::VersionParagraph::samples()),
        QueryResult::VersionSlides(Box::new(crate::collab::VersionSlides::samples().remove(0))),
        QueryResult::Recents(RecentFile::samples()),
        QueryResult::Properties(Box::new(crate::commands::DocPropertiesInfo {
            title: Some("Budget 2027".into()),
            author: Some("Ana".into()),
            company: Some("Kiln & Co".into()),
            sheets: 3,
            ..Default::default()
        })),
        QueryResult::Inspect(vec![
            crate::query::InspectItem { kind: crate::query::InspectKind::Comments, count: 2 },
            crate::query::InspectItem { kind: crate::query::InspectKind::HiddenSheets, count: 0 },
        ]),
    ];
    crate::query::InspectKind => vec![
        crate::query::InspectKind::Comments, crate::query::InspectKind::Revisions,
        crate::query::InspectKind::Properties, crate::query::InspectKind::HiddenSheets,
        crate::query::InspectKind::HiddenSlides, crate::query::InspectKind::SpeakerNotes,
    ];
    crate::query::InspectItem => vec![crate::query::InspectItem {
        kind: crate::query::InspectKind::Revisions, count: 12,
    }];
    crate::commands::CoreOptions => vec![crate::commands::CoreOptions::default()];
    ErrorKind => vec![ErrorKind::NotFound, ErrorKind::Internal];
    ErrorInfo => vec![error()];
    RequestId => vec![RequestId(1), RequestId(u32::MAX)];
    FileFilter => vec![FileFilter { name: "Word Document".into(), extensions: vec!["docx".into(), "docm".into()] }];
    RecentFile => vec![RecentFile {
        path: "/data/ana/Budget.xlsx".into(), name: "Budget".into(), kind: DocKind::Grid,
        opened_at_ms: 1_792_000_000_000, pinned: true, thumbnail: None,
    }, RecentFile {
        path: "opfs:documents/Plan.kwrite".into(), name: "Plan".into(), kind: DocKind::Write,
        opened_at_ms: 1_792_000_000_001, pinned: false,
        thumbnail: Some("data:image/png;base64,iVBORw0KGgo=".into()),
    }];
    crate::summary::AutoSave => vec![crate::summary::AutoSave::Off, crate::summary::AutoSave::On,
        crate::summary::AutoSave::Unavailable("Save the document first".into())];
    StoreArea => vec![StoreArea::Journal, StoreArea::Versions, StoreArea::Recents,
        StoreArea::Dictionary, StoreArea::Prefs];
    HostOp => vec![
        HostOp::ReadFile { path: "/a.txt".into() },
        HostOp::WriteFile { path: "/a.txt".into(), bytes: vec![0, 1, 127, 128, 255] },
        HostOp::PickOpen { filters: FileFilter::samples() },
        HostOp::PickSave { suggested_name: "Scratch1.txt".into(), filters: vec![] },
        HostOp::Recents,
        HostOp::StorePref { key: "ui.theme".into(), value: "\"dark\"".into() },
        HostOp::LoadPref { key: "ui.theme".into() },
        HostOp::ClipboardRead,
        HostOp::ClipboardWrite { text: "x".into(), html: Some("<b>x</b>".into()) },
        HostOp::OpenExternal { url: "https://zahakj.github.io/kiln/".into() },
        HostOp::OpenFile { path: "/tmp/kiln-print/Report.pdf".into() },
        HostOp::PrintPdf { name: "Report.pdf".into(), bytes: b"%PDF-1.7".to_vec() },
        HostOp::OpenWindow { route: "#/present?doc=3&view=audience".into(), title: "Kiln Deck - Slide Show".into() },
        HostOp::SocketOpen { socket: 1, url: "wss://collab.example.org/rooms/k7p2q9xw".into() },
        HostOp::SocketSend { socket: 1, bytes: vec![0, 1, 200] },
        HostOp::SocketReceive { socket: 1 },
        HostOp::SocketClose { socket: 1 },
        HostOp::Timer { ms: 15_000 },
        HostOp::StoreGet { area: StoreArea::Recents, key: "r/1f".into() },
        HostOp::StorePut { area: StoreArea::Journal, key: "k1x9.base".into(), bytes: vec![0, 255] },
        HostOp::StoreAppend { area: StoreArea::Journal, key: "k1x9.log".into(), bytes: vec![7] },
        HostOp::StoreDelete { area: StoreArea::Versions, key: "f/2a/v1".into() },
        HostOp::StoreList { area: StoreArea::Journal, prefix: String::new() },
        HostOp::WatchFile { path: "/data/ana/Report.docx".into() },
        HostOp::UnwatchFile { path: "/data/ana/Report.docx".into() },
        HostOp::StatFile { path: "/data/ana/Report.docx".into() },
    ];
    FileStamp => vec![stamp()];
    HostRequest => vec![HostRequest { id: RequestId(300), op: HostOp::Recents }];
    HostReply => vec![
        HostReply::Done,
        HostReply::Empty,
        HostReply::Bytes(b"Kiln\nWrite".to_vec()),
        HostReply::Path("/data/ana/x.txt".into()),
        HostReply::Text("\"dark\"".into()),
        HostReply::Recents(RecentFile::samples()),
        HostReply::Keys(vec!["k1x9.base.1".into(), "k1x9.log.1".into()]),
        HostReply::Stamp(stamp()),
        HostReply::Clipboard { text: "t".into(), html: None },
        HostReply::Cancelled,
        HostReply::Failed(error()),
    ];
    Outcome => vec![Outcome::Applied, Outcome::Unchanged, Outcome::Pending(RequestId(4))];
    CoreInfo => vec![core_info()];
    ExecValue => vec![ExecValue::None, ExecValue::Pong(core_info()), ExecValue::Echo("hi".into())];
    ExecResult => vec![
        ExecResult { outcome: Outcome::Applied, summary: Some(summary()), value: ExecValue::None },
        ExecResult { outcome: Outcome::Unchanged, summary: None, value: ExecValue::Pong(core_info()) },
    ];
    A11yRole => vec![A11yRole::Document, A11yRole::TextBox, A11yRole::ColumnHeader];
    A11yRevisionDisplay => vec![A11yRevisionDisplay::Marked, A11yRevisionDisplay::Hidden];
    A11yRevision => vec![A11yRevision { start: 2, end: 5, inserted: true, display: A11yRevisionDisplay::Plain }];
    A11yNode => a11y().nodes;
    A11yTree => vec![A11yTree::default(), a11y()];
    Progress => vec![Progress { doc: Some(DocId(1)), task: "progress.open".into(), done: 3, total: 0 }];
    RecoveryEntry => vec![RecoveryEntry {
        key: "k1x9".into(), kind: DocKind::Write, title: "Document1".into(), path: None,
        format: None, saved_at_ms: 1_792_000_000_000, problem: None,
    }, RecoveryEntry {
        key: "k2".into(), kind: DocKind::Grid, title: "Budget".into(),
        path: Some("/data/ana/Budget.xlsx".into()), format: Some(FileFormat::Xlsx),
        saved_at_ms: 1_792_000_000_500, problem: Some("written by a newer version of Kiln".into()),
    }];
    Event => vec![
        Event::Opened { summary: summary() },
        Event::Changed { summary: summary(), selection: selection(), status: status() },
        Event::Saved { summary: summary() },
        Event::Exported { doc: DocId(7), path: Some("/data/ana/Report.pdf".into()) },
        Event::Exported { doc: DocId(7), path: None },
        Event::Closed { doc: DocId(7) },
        Event::Frame { doc: DocId(7), delta: crate::scene::samples::small_delta() },
        Event::Progress(Progress { doc: None, task: "progress.save".into(), done: 1, total: 2 }),
        Event::Recovery { entries: RecoveryEntry::samples() },
        Event::Recents { files: RecentFile::samples() },
        Event::FileChanged { doc: DocId(7) },
        Event::Collab { doc: DocId(7), peers: 2, connected: true },
        Event::Show { doc: DocId(7), status: crate::commands::deck_show::DeckShowStatus::default() },
        Event::Recovered {
            doc: DocId(4),
            note: crate::event::RecoveryNote {
                stopped: Some(crate::event::RecoveryStop {
                    command: Some("grid.data.text_to_columns".into()),
                    after: 2,
                    at_ms: 1_792_000_000_000,
                }),
                draft: Some(crate::event::RecoveredDraft { cell: "Sheet1!F8".into(), text: "9".into(), sheet: 1, row: 7, col: 5 }),
            },
        },
        Event::Error { doc: Some(DocId(7)), error: error() },
        Event::HostRequest(HostRequest {
            id: RequestId(9),
            op: HostOp::PickSave { suggested_name: "Scratch1.txt".into(), filters: FileFilter::samples() },
        }),
    ];
    OpenHint => vec![OpenHint::default(), OpenHint {
        name: Some("Report.docx".into()), path: Some("/r/Report.docx".into()),
        format: Some(FileFormat::Docx), read_only: true, kind: None, template: false,
    }, OpenHint {
        name: Some("notes.txt".into()), path: None,
        format: None, read_only: false, kind: Some(DocKind::Scratch), template: false,
    }, OpenHint {
        name: Some("Letter.kwrite".into()), path: None,
        format: None, read_only: false, kind: None, template: true,
    }];
    DocId => vec![DocId(1), DocId(u32::MAX)];
    NodeId => vec![NodeId(1), NodeId::MAX];
}

fn core_info() -> CoreInfo {
    CoreInfo {
        version: crate::VERSION.into(),
        protocol: crate::PROTOCOL_VERSION,
        target: "wasm32".into(),
    }
}

// printing.
samples! {
    PrintParity => vec![PrintParity::All, PrintParity::Odd, PrintParity::Even];
    PrintPaper => vec![PrintPaper::Letter, PrintPaper::Legal, PrintPaper::A3, PrintPaper::A4, PrintPaper::A5];
    PrintOrientation => vec![PrintOrientation::Portrait, PrintOrientation::Landscape];
    PrintMargins => vec![PrintMargins::Normal, PrintMargins::Narrow, PrintMargins::Moderate, PrintMargins::Wide];
    PrintSettings => vec![PrintSettings::default(), print_settings()];
    PrintPreview => vec![print_preview()];
    PdfFile => vec![PdfFile { name: "a.pdf".into(), bytes: vec![], pdf_ua: true, ua_issues: vec![] }];
    SlideLayout => vec![SlideLayout::FullPage, SlideLayout::NotesPages, SlideLayout::Outline, SlideLayout::Handouts];
    SlidePrint => vec![SlidePrint::default()];
}

// every variant of the chart command arguments.
samples! {
    ChartKind => ChartKind::ALL.to_vec();
    crate::commands::DocWindow => vec![crate::commands::DocWindow::Audience, crate::commands::DocWindow::Presenter];
    ChartElementKind => vec![
        ChartElementKind::ChartTitle, ChartElementKind::PrimaryHorizontalAxis,
        ChartElementKind::PrimaryVerticalAxis, ChartElementKind::PrimaryHorizontalAxisTitle,
        ChartElementKind::PrimaryVerticalAxisTitle, ChartElementKind::MajorGridlines,
        ChartElementKind::MinorGridlines, ChartElementKind::CategoryGridlines,
        ChartElementKind::Legend, ChartElementKind::DataLabels, ChartElementKind::LinearTrendline,
        ChartElementKind::ExponentialTrendline, ChartElementKind::MovingAverage,
        ChartElementKind::StandardErrorBars, ChartElementKind::PercentageErrorBars,
        ChartElementKind::StandardDeviationErrorBars,
    ];
    ChartSeriesOp => vec![
        ChartSeriesOp::Add { name: "=Budget!$E$1".into(), values: "=Budget!$E$2:$E$9".into() },
        ChartSeriesOp::Edit { series: 3, name: "Kiln «hire»".into(), values: "Budget!$F$2:$F$9".into() },
        ChartSeriesOp::Remove { series: 70_000 },
        ChartSeriesOp::Move { series: 0, up: false },
        ChartSeriesOp::Categories { range: "='Q1 sales'!$A$2:$A$9".into() },
    ];
    ChartBlanks => vec![ChartBlanks::Gap, ChartBlanks::Zero, ChartBlanks::Span];
    ChartAreaTarget => vec![ChartAreaTarget::ChartArea, ChartAreaTarget::PlotArea];
    ChartAreaState => vec![ChartAreaState {
        fill: Some("F3ECE3".into()),
        no_fill: false,
        line: None,
        no_line: true,
        line_width: 1.5,
        shadow: true,
        rounded: false,
    }];
    ChartLegendPosition => vec![
        ChartLegendPosition::Right, ChartLegendPosition::Top, ChartLegendPosition::Left,
        ChartLegendPosition::Bottom, ChartLegendPosition::TopRight,
    ];
    ChartLabelPosition => vec![
        ChartLabelPosition::Center, ChartLabelPosition::InsideEnd, ChartLabelPosition::InsideBase,
        ChartLabelPosition::OutsideEnd, ChartLabelPosition::BestFit, ChartLabelPosition::Left,
        ChartLabelPosition::Right, ChartLabelPosition::Above, ChartLabelPosition::Below,
    ];
    ChartMoveTarget => vec![
        ChartMoveTarget::NewSheet { name: "Chart1".into() },
        ChartMoveTarget::ObjectIn { sheet: "Budget «2026»".into() },
    ];
    ChartBound => vec![ChartBound::Auto, ChartBound::Fixed(1e-3), ChartBound::Fixed(-2.5e9)];
    ChartAxisTarget => vec![
        ChartAxisTarget::PrimaryValue, ChartAxisTarget::PrimaryCategory,
        ChartAxisTarget::SecondaryValue, ChartAxisTarget::SecondaryCategory,
    ];
    ChartSeriesState => chart_series_states();
    ChartAxisState => chart_axis_states();
    ChartState => vec![
        ChartState {
            kind: Some(ChartKind::ComboColumnLineSecondary),
            style: Some(2),
            color_set: 0,
            color_sets: vec![vec!["1D8A60".into(), "E05A27".into()]; 8],
            title: Some("Spend by quarter".into()),
            legend: Some(ChartLegendPosition::Bottom),
            data_labels: Some(ChartLabelPosition::OutsideEnd),
            elements: vec![ChartElementKind::ChartTitle, ChartElementKind::Legend],
            data_range: "'Budget «2026»'!$A$1:$D$5".into(),
            series: chart_series_states(),
            categories: vec!["Q1".into(), "Q2".into(), String::new()],
            has_cells: true,
            axes: chart_axis_states(),
            blanks: ChartBlanks::Span,
            show_hidden: true,
            categories_range: "'Budget «2026»'!$A$2:$A$5".into(),
            chart_area: ChartAreaState {
                fill: Some("FFFFFF".into()),
                no_fill: false,
                line: None,
                no_line: true,
                line_width: 0.0,
                shadow: true,
                rounded: true,
            },
            plot_area: ChartAreaState {
                fill: None,
                no_fill: true,
                line: Some("2B1C14".into()),
                no_line: false,
                line_width: 0.75,
                shadow: false,
                rounded: false,
            },
        },
        ChartState {
            kind: None,
            style: None,
            color_set: 7,
            color_sets: Vec::new(),
            title: None,
            legend: None,
            data_labels: None,
            elements: Vec::new(),
            data_range: String::new(),
            series: Vec::new(),
            categories: Vec::new(),
            has_cells: false,
            axes: Vec::new(),
            blanks: ChartBlanks::Gap,
            show_hidden: false,
            categories_range: String::new(),
            chart_area: ChartAreaState {
                fill: None,
                no_fill: false,
                line: None,
                no_line: false,
                line_width: 0.0,
                shadow: false,
                rounded: false,
            },
            plot_area: ChartAreaState {
                fill: None,
                no_fill: false,
                line: None,
                no_line: false,
                line_width: 0.0,
                shadow: false,
                rounded: false,
            },
        },
    ];
}

fn chart_series_states() -> Vec<ChartSeriesState> {
    vec![
        ChartSeriesState {
            idx: 0,
            name: "Materials".into(),
            color: "1D8A60".into(),
            fill: None,
            line: None,
            line_width: 0.0,
            secondary: false,
            name_ref: "='Budget «2026»'!$B$1".into(),
            values_ref: "='Budget «2026»'!$B$2:$B$5".into(),
        },
        ChartSeriesState {
            idx: 2,
            name: "Events".into(),
            color: "3159C6".into(),
            fill: Some("3159C6".into()),
            line: Some("2B1C14".into()),
            line_width: 2.25,
            secondary: true,
            name_ref: "Events".into(),
            values_ref: String::new(),
        },
    ]
}

fn chart_axis_states() -> Vec<ChartAxisState> {
    vec![
        ChartAxisState {
            target: ChartAxisTarget::PrimaryValue,
            label: "Vertical (Value) Axis".into(),
            is_value: true,
            min: Some(0.0),
            max: Some(1400.0),
            major_unit: Some(200.0),
            min_auto: true,
            max_auto: false,
            major_unit_auto: true,
            log_base: Some(10.0),
            reversed: true,
            number_format: Some("#,##0".into()),
        },
        ChartAxisState {
            target: ChartAxisTarget::PrimaryCategory,
            label: "Horizontal (Category) Axis: Quarter".into(),
            is_value: false,
            min: None,
            max: None,
            major_unit: None,
            min_auto: true,
            max_auto: true,
            major_unit_auto: true,
            log_base: None,
            reversed: false,
            number_format: None,
        },
    ]
}
