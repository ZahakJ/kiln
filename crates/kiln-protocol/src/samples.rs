// SPDX-License-Identifier: AGPL-3.0-or-later
//! Representative values of every protocol type.
//!
//! `cargo xtask protocol` encodes them as the postcard test vectors that the TypeScript decoder is
//! tested against, and the unit tests use them to check that every command has metadata. Values
//! are chosen to exercise the encoding: multi-byte varints, negative and fractional floats,
//! non-ASCII text, empty and non-empty collections, `None` and `Some`.

use kiln_common::{DocId, ErrorKind, NodeId};

use crate::OpenHint;
use crate::a11y::{A11yNode, A11yRole, A11yTree};
use crate::app::{App, DocKind, FileFormat};
use crate::commands::{
    Availability, CaretMotion, ChartAxisTarget, ChartBound, ChartElementKind, ChartKind,
    ChartLabelPosition, ChartLegendPosition, ChartMoveTarget, Command, CommandInfo, SelectUnit,
    SuiteCommand, Tier, WriteCommand, catalog,
};
use crate::error::ErrorInfo;
use crate::event::{Event, Progress, RecoveryEntry};
use crate::exec::{CoreInfo, ExecResult, ExecValue, Outcome};
use crate::geom::{Point, Rect, Size};
use crate::host::{FileFilter, HostOp, HostReply, HostRequest, RecentFile, RequestId};
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
            path: Some("/home/ana/Quarterly report.docx".into()),
        }
        .into(),
        SuiteCommand::New {
            kind: DocKind::Scratch,
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
        // WP-73: chart commands.
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
    ]
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
            },
            A11yNode {
                id: NodeId(2),
                role: A11yRole::Heading,
                name: None,
                text: Some("Résumé".into()),
                bounds: Some(rect()),
                level: Some(1),
                children: vec![],
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
    CaretMotion => vec![CaretMotion::Backward, CaretMotion::DocumentEnd, CaretMotion::PageDown];
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
        Query::Status, Query::A11y, Query::SelectedText,
        Query::CompatReport { format: None },
        Query::CompatReport { format: Some(FileFormat::Docx) }];
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
    ];
    ErrorKind => vec![ErrorKind::NotFound, ErrorKind::Internal];
    ErrorInfo => vec![error()];
    RequestId => vec![RequestId(1), RequestId(u32::MAX)];
    FileFilter => vec![FileFilter { name: "Word Document".into(), extensions: vec!["docx".into(), "docm".into()] }];
    RecentFile => vec![RecentFile {
        path: "/home/ana/Budget.xlsx".into(), name: "Budget".into(), kind: DocKind::Grid,
        opened_at_ms: 1_792_000_000_000, pinned: true,
    }];
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
    ];
    HostRequest => vec![HostRequest { id: RequestId(300), op: HostOp::Recents }];
    HostReply => vec![
        HostReply::Done,
        HostReply::Empty,
        HostReply::Bytes(b"Kiln\nWrite".to_vec()),
        HostReply::Path("/home/ana/x.txt".into()),
        HostReply::Text("\"dark\"".into()),
        HostReply::Recents(RecentFile::samples()),
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
    A11yRole => vec![A11yRole::Document, A11yRole::TextBox];
    A11yNode => a11y().nodes;
    A11yTree => vec![A11yTree::default(), a11y()];
    Progress => vec![Progress { doc: Some(DocId(1)), task: "progress.open".into(), done: 3, total: 0 }];
    RecoveryEntry => vec![RecoveryEntry { title: "Document1".into(), path: None, saved_at_ms: 1_792_000_000_000 }];
    Event => vec![
        Event::Opened { summary: summary() },
        Event::Changed { summary: summary(), selection: selection(), status: status() },
        Event::Saved { summary: summary() },
        Event::Closed { doc: DocId(7) },
        Event::Frame { doc: DocId(7), delta: crate::scene::samples::small_delta() },
        Event::Progress(Progress { doc: None, task: "progress.save".into(), done: 1, total: 2 }),
        Event::Recovery { entries: RecoveryEntry::samples() },
        Event::Collab { doc: DocId(7), peers: 2, connected: true },
        Event::Error { doc: Some(DocId(7)), error: error() },
        Event::HostRequest(HostRequest {
            id: RequestId(9),
            op: HostOp::PickSave { suggested_name: "Scratch1.txt".into(), filters: FileFilter::samples() },
        }),
    ];
    OpenHint => vec![OpenHint::default(), OpenHint {
        name: Some("Report.docx".into()), path: Some("/r/Report.docx".into()),
        format: Some(FileFormat::Docx), read_only: true,
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

// WP-73: every variant of the chart command arguments.
samples! {
    ChartKind => ChartKind::ALL.to_vec();
    ChartElementKind => vec![
        ChartElementKind::ChartTitle, ChartElementKind::PrimaryHorizontalAxis,
        ChartElementKind::PrimaryVerticalAxis, ChartElementKind::PrimaryHorizontalAxisTitle,
        ChartElementKind::PrimaryVerticalAxisTitle, ChartElementKind::MajorGridlines,
        ChartElementKind::MinorGridlines, ChartElementKind::CategoryGridlines,
        ChartElementKind::Legend, ChartElementKind::DataLabels, ChartElementKind::LinearTrendline,
        ChartElementKind::ExponentialTrendline, ChartElementKind::MovingAverage,
    ];
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
}
