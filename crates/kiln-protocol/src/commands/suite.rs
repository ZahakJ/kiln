// SPDX-License-Identifier: AGPL-3.0-or-later
//! Suite-wide commands: files, undo/redo, view and diagnostics.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::edit::EditCommand;
use super::write::features::PropertiesEdit;
use crate::app::{AppMask, DocKind, FileFormat};
use crate::print::PrintSettings;
use crate::query::InspectKind;
use crate::viewport::ViewMode;

/// Suite-wide commands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub enum SuiteCommand {
    /// Open a file. With `path: None` the host shows its Open dialog first.
    Open {
        /// The file to open; `None` asks the host to pick one.
        path: Option<String>,
    },
    /// Create a new, empty document.
    New {
        /// The kind of document.
        kind: DocKind,
        /// The default editing language (BCP 47, e.g. `ar-SA`) of the new document's text: Write
        /// documents take it as their document default, and start their paragraphs from the right
        /// when it is written from right to left. `None` keeps English (United States).
        #[serde(default)]
        #[ts(optional)]
        language: Option<String>,
    },
    /// Save the document to its path, or ask for one (Save As) if it has none.
    Save,
    /// Save the document under a new path and/or format.
    SaveAs {
        /// The target path; `None` asks the host to pick one.
        path: Option<String>,
        /// The format; `None` keeps the current format (or uses the path's extension).
        format: Option<FileFormat>,
    },
    /// Close the document (the UI asks about unsaved changes before sending this).
    Close,
    /// Undo the last change.
    Undo,
    /// Redo the last undone change.
    Redo,
    /// Set the zoom factor (1.0 = 100 %), between 0.1 and 5.0 like Office.
    SetZoom {
        /// The zoom factor.
        zoom: f32,
    },
    /// Switch the document view (Print Layout, Draft, Normal, Slide Sorter…).
    SetViewMode {
        /// The view.
        mode: ViewMode,
    },
    /// The surface lost its painted tiles (remounted, canvas context lost): the next frame
    /// resets the painter and resends everything visible.
    Repaint,
    /// Round trip with no effect; the reply carries the core and protocol versions.
    Ping,
    /// Round trip that returns its text (bridge diagnostics).
    Echo {
        /// Any text.
        text: String,
    },
    // ── Charts: shared by Grid, Write and Deck; the app's document model applies them
    // to the selected chart (or, for `ChartInsert`, to the selection) through `kiln-charts`.
    /// Insert a chart from the selected cells (Grid) or with sample data (Write, Deck). `None`
    /// inserts the first Recommended Chart for the selection's shape.
    ChartInsert {
        /// The chart type.
        kind: Option<ChartKind>,
    },
    /// Change the selected chart's type (Chart Design ▸ Change Chart Type).
    ChartChangeType {
        /// The new type.
        kind: ChartKind,
    },
    /// Swap the data's rows and columns (Chart Design ▸ Switch Row/Column).
    ChartSwitchRowColumn,
    /// Point the chart at another range (Chart Design ▸ Select Data), e.g. `Sheet1!$A$1:$D$5`.
    ChartSelectData {
        /// The chart data range.
        range: String,
    },
    /// Add or remove a chart element (Chart Design ▸ Add Chart Element, the + button).
    ChartSetElement {
        /// The element.
        element: ChartElementKind,
        /// Show (`true`) or remove it.
        on: bool,
    },
    /// Place or hide the legend.
    ChartSetLegend {
        /// The position; `None` removes the legend.
        position: Option<ChartLegendPosition>,
    },
    /// Show data labels at a position, or remove them.
    ChartSetDataLabels {
        /// The position; `None` removes the labels.
        position: Option<ChartLabelPosition>,
    },
    /// Apply one of Office's quick layouts (1–11).
    ChartQuickLayout {
        /// The layout number.
        layout: u8,
    },
    /// Apply a chart style (1–48, Office's numbering).
    ChartSetStyle {
        /// The style number.
        style: u8,
    },
    /// Change colours (Chart Design ▸ Change Colors): 0 = colourful, 1–6 = shades of one accent,
    /// 7 = greys.
    ChartSetColors {
        /// The colour set.
        palette: u8,
    },
    /// Set the chart title text (an empty text removes the title).
    ChartSetTitle {
        /// The title.
        text: String,
    },
    /// Move the chart to its own chart sheet or to another sheet (Grid).
    ChartMove {
        /// The target.
        to: ChartMoveTarget,
    },
    /// Format a series (Format pane ▸ Series Options, Fill & Line).
    ChartFormatSeries {
        /// The series' `idx`.
        series: u32,
        /// Fill colour `RRGGBB`; `None` keeps it, an empty string makes it automatic.
        fill: Option<String>,
        /// Line colour `RRGGBB`; `None` keeps it, an empty string makes it automatic.
        line: Option<String>,
        /// Line width in points.
        line_width: Option<f32>,
        /// Plot the series on the secondary axis.
        secondary: Option<bool>,
    },
    /// Set axis options (Format pane ▸ Axis Options). `None` fields are left alone.
    ChartFormatAxis {
        /// Which axis.
        axis: ChartAxisTarget,
        /// Minimum.
        min: Option<ChartBound>,
        /// Maximum.
        max: Option<ChartBound>,
        /// Major unit.
        major_unit: Option<ChartBound>,
        /// Logarithmic scale base (`Auto` turns the log scale off).
        log_base: Option<ChartBound>,
        /// Values in reverse order.
        reversed: Option<bool>,
        /// Number format code (an empty string links to the source).
        number_format: Option<String>,
    },
    /// Select Data ▸ Legend Entries (Series) and Horizontal (Category) Axis Labels: add, edit,
    /// remove or reorder a series, or point the labels at other cells.
    ChartEditSeries {
        /// What to do.
        op: ChartSeriesOp,
    },
    /// Select Data ▸ Hidden and Empty Cells.
    ChartHiddenEmpty {
        /// How empty cells show.
        blanks: ChartBlanks,
        /// Plot data in hidden rows and columns.
        show_hidden: bool,
    },
    /// Format pane ▸ Chart Area or Plot Area: fill, border and shadow. `None` fields are
    /// left alone; an empty colour makes it automatic, `"none"` removes it.
    ChartFormatArea {
        /// Which area.
        area: ChartAreaTarget,
        /// Fill colour `RRGGBB`, `""` automatic, `"none"` no fill.
        fill: Option<String>,
        /// Border colour `RRGGBB`, `""` automatic, `"none"` no line.
        line: Option<String>,
        /// Border width in points.
        line_width: Option<f32>,
        /// An outer shadow (Effects ▸ Shadow).
        shadow: Option<bool>,
        /// Rounded corners (chart area only).
        rounded: Option<bool>,
    },
    // ── Print and PDF.
    /// Print: natively the PDF goes to a temporary file the system viewer opens (its print
    /// dialog prints it); on the web the page prints it from a hidden frame.
    PrintPdf {
        /// What to print.
        settings: PrintSettings,
    },
    /// Export ▸ PDF where the core writes files (desktop): the save dialog when `path` is
    /// `None`, then the file; `Event::Exported` reports where (or the cancelled dialog).
    SavePdf {
        /// The target path; `None` asks the host.
        path: Option<String>,
        /// What to print.
        settings: PrintSettings,
    },
    // ── editing commands the document editors share (clipboard, formatting, lists, page
    // setup, tables); the session hands them to the document model.
    /// An editing command applied by the document's model to its selection.
    Edit(EditCommand),
    /// opens a second window on the document (Presenter View's audience window on the
    /// desktop). The session asks the host with `HostOp::OpenWindow`; both windows talk to the
    /// same core and follow the show through `Event::Show`.
    OpenWindow {
        /// Which window.
        window: DocWindow,
    },
    // ── collaboration and Version History.
    /// Shares the document: creates a room on the server and starts exchanging edits. The
    /// link is in `Query::Collab`.
    Share {
        /// The collaboration server (`wss://…` or `ws://…`).
        server: String,
        /// The server's shared token.
        token: String,
        /// This person's display name.
        name: String,
    },
    /// Opens a shared document from its link: a new document that fills when the server sends
    /// it.
    JoinShared {
        /// The kind of document.
        kind: DocKind,
        /// The collaboration server.
        server: String,
        /// The room (the link's `doc`).
        room: String,
        /// The server's shared token.
        token: String,
        /// This person's display name.
        name: String,
    },
    /// Stops sharing: the document stays open here as an ordinary one.
    StopSharing,
    /// Shows a version read-only in place of the document (`None`: back to the document).
    VersionPreview {
        /// The version.
        id: Option<String>,
    },
    /// Restores a version: the document becomes that version, as a new change everyone gets.
    VersionRestore {
        /// The version.
        id: String,
    },
    /// Names a version (an empty name removes it).
    VersionRename {
        /// The version.
        id: String,
        /// The name.
        name: String,
    },
    // ── storage, recovery, AutoSave, recents.
    /// Looks for documents a crash left unsaved; `Event::Recovery` lists them.
    RecoveryScan,
    /// Opens a document from its autosave journal (Document Recovery ▸ Open); the document is
    /// marked as changed, so saving keeps it.
    RecoveryOpen {
        /// The journal (`RecoveryEntry::key`).
        key: String,
        /// Its kind of document (`RecoveryEntry::kind`: the web opens it in that app's core).
        kind: DocKind,
    },
    /// Deletes a journal without opening it (Document Recovery ▸ Delete).
    RecoveryDelete {
        /// The journal.
        key: String,
    },
    /// Turns AutoSave to file on or off (the title bar's switch).
    SetAutoSave {
        /// On or off.
        on: bool,
    },
    /// Reads the recent and pinned files again; `Event::Recents` follows.
    RecentsLoad,
    /// Pins a recent file to the top of the list, or unpins it.
    RecentPin {
        /// The file.
        path: String,
        /// Pinned or not.
        pinned: bool,
    },
    /// Removes a file from the recent list.
    RecentRemove {
        /// The file.
        path: String,
    },
    /// Reads the document's file again, dropping the changes made here (after another program
    /// changed it).
    Reload,
    /// Keeps this window's version of a file another program changed (the question's "Keep my
    /// version"): the next save writes over the other change, and AutoSave goes on.
    KeepMine,
    /// Closes the document without saving its changes (the close question's "Don't Save"): its
    /// recovery copy goes too. `Close` of a document with unsaved changes keeps it.
    CloseWithoutSaving,
    // ── Options, Document Properties, Inspect Document.
    /// The user's options the core applies (File ▸ Options): sent when a window starts and
    /// whenever they change.
    SetOptions {
        /// The options.
        options: CoreOptions,
    },
    /// File ▸ Info ▸ Properties: sets the document's properties (Write, Grid and Deck; saved in
    /// the core properties of `.docx`, `.xlsx` and `.pptx` and in the native bundle).
    SetProperties {
        /// The changes.
        props: Box<PropertiesEdit>,
    },
    /// File ▸ Info ▸ Inspect Document ▸ Remove All: removes one kind of hidden data (one undo
    /// step where the app keeps undo).
    InspectRemove {
        /// What to remove.
        item: InspectKind,
    },
}

/// The options of File ▸ Options that the core applies.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CoreOptions {
    /// Save ▸ AutoSave: seconds it waits after the last edit (1–600).
    pub autosave_delay_s: u32,
    /// Save ▸ Keep AutoRecover information: documents opened from now on keep a recovery
    /// journal.
    pub recovery: bool,
    /// Proofing ▸ Ignore words in UPPERCASE.
    pub spell_ignore_uppercase: bool,
    /// Proofing ▸ Ignore words that contain numbers.
    pub spell_ignore_numbers: bool,
    /// Proofing ▸ Ignore Internet and file addresses.
    pub spell_ignore_addresses: bool,
    /// Language ▸ Display language as the interface shows it (`en`, `de`, `ar`): what the
    /// core draws in the interface's language (Deck's placeholder prompts). Empty: English.
    pub ui_language: String,
}

impl Default for CoreOptions {
    fn default() -> Self {
        Self {
            autosave_delay_s: 2,
            recovery: true,
            spell_ignore_uppercase: true,
            spell_ignore_numbers: true,
            spell_ignore_addresses: true,
            ui_language: String::new(),
        }
    }
}

/// A second window on a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum DocWindow {
    /// The slide show for the audience (Presenter View stays in the first window).
    Audience,
    /// Presenter View (the show stays in the first window).
    Presenter,
}

impl DocWindow {
    /// The route the window shows for a document.
    #[must_use]
    pub fn route(self, doc: u64) -> String {
        match self {
            Self::Audience => format!("#/present?doc={doc}&view=audience"),
            Self::Presenter => format!("#/present?doc={doc}&view=presenter"),
        }
    }
}

crate::command_table! {
    SuiteCommand => SUITE_COMMANDS;
    Self::Open { .. } => {
        id: "suite.file.open", keytip: Some("O"), group: "file",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::New { .. } => {
        id: "suite.file.new", keytip: Some("N"), group: "file",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::Save => {
        id: "suite.file.save", keytip: Some("S"), group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::SaveAs { .. } => {
        id: "suite.file.save_as", keytip: Some("A"), group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Close => {
        id: "suite.file.close", keytip: Some("C"), group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Undo => {
        id: "suite.edit.undo", keytip: None, group: "edit",
        tier: P0, apps: AppMask::DOCUMENTS, availability: CanUndo,
    };
    Self::Redo => {
        id: "suite.edit.redo", keytip: None, group: "edit",
        tier: P0, apps: AppMask::DOCUMENTS, availability: CanRedo,
    };
    Self::SetZoom { .. } => {
        id: "suite.view.zoom", keytip: Some("Q"), group: "view",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::SetViewMode { .. } => {
        id: "suite.view.mode", keytip: None, group: "view",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Repaint => {
        id: "suite.view.repaint", keytip: None, group: "view",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::Ping => {
        id: "suite.dev.ping", keytip: None, group: "dev",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::Echo { .. } => {
        id: "suite.dev.echo", keytip: None, group: "dev",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::ChartInsert { .. } => {
        id: "suite.chart.insert", keytip: Some("C"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartChangeType { .. } => {
        id: "suite.chart.change_type", keytip: Some("C"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSwitchRowColumn => {
        id: "suite.chart.switch_row_column", keytip: Some("W"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSelectData { .. } => {
        id: "suite.chart.select_data", keytip: Some("E"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSetElement { .. } => {
        id: "suite.chart.element", keytip: Some("A"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSetLegend { .. } => {
        id: "suite.chart.legend", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSetDataLabels { .. } => {
        id: "suite.chart.data_labels", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartQuickLayout { .. } => {
        id: "suite.chart.quick_layout", keytip: Some("L"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSetStyle { .. } => {
        id: "suite.chart.style", keytip: Some("S"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSetColors { .. } => {
        id: "suite.chart.colors", keytip: Some("H"), group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartSetTitle { .. } => {
        id: "suite.chart.title", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartMove { .. } => {
        id: "suite.chart.move", keytip: Some("M"), group: "chart",
        tier: P0, apps: AppMask::GRID, availability: Editable,
    };
    Self::ChartFormatSeries { .. } => {
        id: "suite.chart.format_series", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartFormatAxis { .. } => {
        id: "suite.chart.format_axis", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartEditSeries { .. } => {
        id: "suite.chart.edit_series", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartHiddenEmpty { .. } => {
        id: "suite.chart.hidden_empty", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::ChartFormatArea { .. } => {
        id: "suite.chart.format_area", keytip: None, group: "chart",
        tier: P0, apps: CHART_APPS, availability: Editable,
    };
    Self::PrintPdf { .. } => {
        id: "suite.file.print_pdf", keytip: None, group: "file",
        tier: P0, apps: AppMask::WRITE.union(AppMask::GRID), availability: Document,
    };
    Self::SavePdf { .. } => {
        id: "suite.file.save_pdf", keytip: None, group: "file",
        tier: P0, apps: AppMask::WRITE.union(AppMask::GRID), availability: Document,
    };
    Self::OpenWindow { .. } => {
        id: "suite.view.open_window", keytip: None, group: "view",
        tier: P0, apps: AppMask::DECK, availability: Document,
    };
    Self::Share { .. } => {
        id: "suite.collab.share", keytip: None, group: "collab",
        tier: P0, apps: COLLAB_APPS, availability: Document,
    };
    Self::JoinShared { .. } => {
        id: "suite.collab.join", keytip: None, group: "collab",
        tier: P0, apps: COLLAB_APPS, availability: Always,
    };
    Self::StopSharing => {
        id: "suite.collab.stop", keytip: None, group: "collab",
        tier: P0, apps: COLLAB_APPS, availability: Document,
    };
    Self::VersionPreview { .. } => {
        id: "suite.collab.version_preview", keytip: None, group: "collab",
        tier: P0, apps: COLLAB_APPS, availability: Document,
    };
    Self::VersionRestore { .. } => {
        id: "suite.collab.version_restore", keytip: None, group: "collab",
        tier: P0, apps: COLLAB_APPS, availability: Document,
    };
    Self::VersionRename { .. } => {
        id: "suite.collab.version_rename", keytip: None, group: "collab",
        tier: P0, apps: COLLAB_APPS, availability: Document,
    };
    Self::RecoveryScan => {
        id: "suite.file.recovery_scan", keytip: None, group: "file",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::RecoveryOpen { .. } => {
        id: "suite.file.recovery_open", keytip: None, group: "file",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::RecoveryDelete { .. } => {
        id: "suite.file.recovery_delete", keytip: None, group: "file",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::SetAutoSave { .. } => {
        id: "suite.file.autosave", keytip: None, group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::RecentsLoad => {
        id: "suite.home.recents_load", keytip: None, group: "home",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::RecentPin { .. } => {
        id: "suite.home.recent_pin", keytip: None, group: "home",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::RecentRemove { .. } => {
        id: "suite.home.recent_remove", keytip: None, group: "home",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::Reload => {
        id: "suite.file.reload", keytip: None, group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::KeepMine => {
        id: "suite.file.keep_mine", keytip: None, group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::CloseWithoutSaving => {
        id: "suite.file.close_without_saving", keytip: None, group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Document,
    };
    Self::SetOptions { .. } => {
        id: "suite.options.set", keytip: None, group: "options",
        tier: P0, apps: AppMask::ALL, availability: Always,
    };
    Self::SetProperties { .. } => {
        id: "suite.file.properties", keytip: None, group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Editable,
    };
    Self::InspectRemove { .. } => {
        id: "suite.file.inspect_remove", keytip: None, group: "file",
        tier: P0, apps: AppMask::DOCUMENTS, availability: Editable,
    };
    // shared editing commands, implemented by Kiln Write (and Notes, which reuses it).
    Self::Edit(EditCommand::Cut) => {
        id: "suite.edit.cut", keytip: Some("X"), group: "edit",
        tier: P0, apps: ALL_EDIT_APPS, availability: Selection,
    };
    Self::Edit(EditCommand::Copy) => {
        id: "suite.edit.copy", keytip: Some("C"), group: "edit",
        tier: P0, apps: ALL_EDIT_APPS, availability: Selection,
    };
    Self::Edit(EditCommand::Paste { .. }) => {
        id: "suite.edit.paste", keytip: Some("V"), group: "edit",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::PasteKeepSource { .. }) => {
        id: "suite.edit.paste_keep_source", keytip: Some("K"), group: "edit",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::PasteMerge { .. }) => {
        id: "suite.edit.paste_merge_formatting", keytip: Some("M"), group: "edit",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::PasteTextOnly { .. }) => {
        id: "suite.edit.paste_text_only", keytip: Some("T"), group: "edit",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::PasteAs { .. }) => {
        id: "suite.edit.paste_as", keytip: None, group: "edit",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Repeat) => {
        id: "suite.edit.repeat", keytip: None, group: "edit",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Bold) => {
        id: "suite.format.bold", keytip: Some("1"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Italic) => {
        id: "suite.format.italic", keytip: Some("2"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Underline) => {
        id: "suite.format.underline", keytip: Some("3"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::DoubleUnderline) => {
        id: "suite.format.double_underline", keytip: None, group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Strikethrough) => {
        id: "suite.format.strikethrough", keytip: Some("4"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Subscript) => {
        id: "suite.format.subscript", keytip: Some("5"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Superscript) => {
        id: "suite.format.superscript", keytip: Some("6"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::AllCaps) => {
        id: "suite.format.all_caps", keytip: None, group: "format",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::FontFamily { .. }) => {
        id: "suite.format.font_family", keytip: Some("FF"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::FontSize { .. }) => {
        id: "suite.format.font_size", keytip: Some("FS"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::GrowFont { .. }) => {
        id: "suite.format.grow_font", keytip: Some("FG"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::ShrinkFont { .. }) => {
        id: "suite.format.shrink_font", keytip: Some("FK"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::ChangeCase { .. }) => {
        id: "suite.format.change_case", keytip: Some("7"), group: "format",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::ClearFormatting) => {
        id: "suite.format.clear_formatting", keytip: Some("E"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::FontColor { .. }) => {
        id: "suite.format.font_color", keytip: Some("FC"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Highlight { .. }) => {
        id: "suite.format.highlight", keytip: Some("I"), group: "format",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Bullets { .. }) => {
        id: "suite.format.bullets", keytip: Some("U"), group: "format",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Numbering { .. }) => {
        id: "suite.format.numbering", keytip: Some("N"), group: "format",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::MultilevelList { .. }) => {
        id: "suite.format.multilevel_list", keytip: Some("M"), group: "format",
        tier: P0, apps: TEXT_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Indent) => {
        id: "suite.format.indent", keytip: Some("AI"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Outdent) => {
        id: "suite.format.outdent", keytip: Some("AO"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::AlignLeft) => {
        id: "suite.format.align_left", keytip: Some("AL"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::AlignCenter) => {
        id: "suite.format.align_center", keytip: Some("AC"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::AlignRight) => {
        id: "suite.format.align_right", keytip: Some("AR"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Justify) => {
        id: "suite.format.justify", keytip: Some("AJ"), group: "format",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::LineSpacing { .. }) => {
        id: "suite.format.line_spacing", keytip: Some("K"), group: "format",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::InsertDate) => {
        id: "suite.insert.date", keytip: None, group: "insert",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::InsertTime) => {
        id: "suite.insert.time", keytip: None, group: "insert",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::InsertPictures) => {
        id: "suite.insert.pictures", keytip: Some("P"), group: "insert",
        tier: P0, apps: ALL_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Margins { .. }) => {
        id: "suite.layout.margins", keytip: Some("M"), group: "layout",
        tier: P0, apps: TEXT_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::Orientation { .. }) => {
        id: "suite.layout.orientation", keytip: Some("O"), group: "layout",
        tier: P0, apps: TEXT_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::PageSize { .. }) => {
        id: "suite.layout.size", keytip: Some("SZ"), group: "layout",
        tier: P0, apps: TEXT_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableInsertRowAbove) => {
        id: "suite.table.insert_row_above", keytip: Some("A"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableInsertRowBelow) => {
        id: "suite.table.insert_row_below", keytip: Some("BE"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableInsertColumnLeft) => {
        id: "suite.table.insert_column_left", keytip: Some("L"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableInsertColumnRight) => {
        id: "suite.table.insert_column_right", keytip: Some("R"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableDelete { .. }) => {
        id: "suite.table.delete", keytip: Some("D"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableMergeCells) => {
        id: "suite.table.merge_cells", keytip: Some("M"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableSplitCells { .. }) => {
        id: "suite.table.split_cells", keytip: Some("P"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableDistributeRows) => {
        id: "suite.table.distribute_rows", keytip: Some("DR"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableDistributeColumns) => {
        id: "suite.table.distribute_columns", keytip: Some("DC"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableCellAlignment { .. }) => {
        id: "suite.table.cell_alignment", keytip: None, group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableRepeatHeaderRows) => {
        id: "suite.table.repeat_header_rows", keytip: Some("RH"), group: "table",
        tier: P0, apps: TEXT_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableConvertToText { .. }) => {
        id: "suite.table.convert_to_text", keytip: Some("V"), group: "table",
        tier: P0, apps: TEXT_EDIT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableHeaderRow) => {
        id: "suite.table.header_row", keytip: Some("HR"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableTotalRow) => {
        id: "suite.table.total_row", keytip: Some("TR"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableBandedRows) => {
        id: "suite.table.banded_rows", keytip: Some("BR"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableFirstColumn) => {
        id: "suite.table.first_column", keytip: Some("FC"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableBandedColumns) => {
        id: "suite.table.banded_columns", keytip: Some("BC"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableStyle { .. }) => {
        id: "suite.table.styles", keytip: None, group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableShading { .. }) => {
        id: "suite.table.shading", keytip: Some("H"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
    Self::Edit(EditCommand::TableBorders { .. }) => {
        id: "suite.table.borders", keytip: Some("B"), group: "table",
        tier: P0, apps: DECK_TEXT_APPS, availability: Editable,
    };
}

impl SuiteCommand {
    /// The smallest and largest zoom factors (10 %–500 %, as in Office).
    pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.1..=5.0;
}

// ── Charts ───────────────────────────────────────────────────────────────────────────

/// The apps whose documents can be shared: Grid's workbooks are not yet.
const COLLAB_APPS: AppMask = AppMask::WRITE.union(AppMask::DECK).union(AppMask::NOTES);

/// The apps that host charts.
const CHART_APPS: AppMask = AppMask::GRID.union(AppMask::WRITE).union(AppMask::DECK);

/// Apps that implement the shared editing commands of [`EditCommand`]. Grid and Deck add
/// themselves when their editors implement them.
const TEXT_EDIT_APPS: AppMask = AppMask::WRITE.union(AppMask::NOTES);

/// The shared editing commands Grid's model applies to cells too: the clipboard, the font,
/// alignment and indent, and the date and time.
const GRID_EDIT_APPS: AppMask = TEXT_EDIT_APPS.union(AppMask::GRID);

/// Shared editing commands Kiln Deck also implements on the text of shapes and tables.
const DECK_TEXT_APPS: AppMask = TEXT_EDIT_APPS.union(AppMask::DECK);

/// Shared editing commands every editor implements, Deck included.
const ALL_EDIT_APPS: AppMask = GRID_EDIT_APPS.union(AppMask::DECK);

/// The chart types of the Insert ▸ Charts galleries and Change Chart Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ChartKind {
    /// Clustered column.
    ColumnClustered,
    /// Stacked column.
    ColumnStacked,
    /// 100 % stacked column.
    ColumnPercent,
    /// Clustered bar.
    BarClustered,
    /// Stacked bar.
    BarStacked,
    /// 100 % stacked bar.
    BarPercent,
    /// Line.
    Line,
    /// Line with markers.
    LineMarkers,
    /// Stacked line.
    LineStacked,
    /// 100 % stacked line.
    LinePercent,
    /// Smooth line with markers.
    LineSmooth,
    /// Area.
    Area,
    /// Stacked area.
    AreaStacked,
    /// 100 % stacked area.
    AreaPercent,
    /// Pie.
    Pie,
    /// Pie with the first slice pulled out.
    PieExploded,
    /// Doughnut.
    Doughnut,
    /// Scatter with markers only.
    ScatterMarkers,
    /// Scatter with straight lines and markers.
    ScatterLines,
    /// Scatter with smooth lines.
    ScatterSmooth,
    /// Bubble.
    Bubble,
    /// Radar.
    Radar,
    /// Radar with markers.
    RadarMarkers,
    /// Filled radar.
    RadarFilled,
    /// Clustered column and line.
    ComboColumnLine,
    /// Clustered column and line on a secondary axis.
    ComboColumnLineSecondary,
    /// Treemap.
    Treemap,
    /// Sunburst.
    Sunburst,
    /// Histogram (statistic, chartex).
    Histogram,
    /// Pareto: sorted columns and the cumulative percentage (statistic, chartex).
    Pareto,
    /// Box and whisker (statistic, chartex).
    BoxWhisker,
    /// Waterfall (chartex).
    Waterfall,
    /// Funnel (chartex).
    Funnel,
}

impl ChartKind {
    /// Every kind, in gallery order.
    pub const ALL: [ChartKind; 33] = [
        Self::ColumnClustered,
        Self::ColumnStacked,
        Self::ColumnPercent,
        Self::BarClustered,
        Self::BarStacked,
        Self::BarPercent,
        Self::Line,
        Self::LineMarkers,
        Self::LineStacked,
        Self::LinePercent,
        Self::LineSmooth,
        Self::Area,
        Self::AreaStacked,
        Self::AreaPercent,
        Self::Pie,
        Self::PieExploded,
        Self::Doughnut,
        Self::ScatterMarkers,
        Self::ScatterLines,
        Self::ScatterSmooth,
        Self::Bubble,
        Self::Radar,
        Self::RadarMarkers,
        Self::RadarFilled,
        Self::ComboColumnLine,
        Self::ComboColumnLineSecondary,
        Self::Treemap,
        Self::Sunburst,
        Self::Histogram,
        Self::Pareto,
        Self::BoxWhisker,
        Self::Waterfall,
        Self::Funnel,
    ];
}

/// Chart elements that can be added or removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ChartElementKind {
    /// The chart title.
    ChartTitle,
    /// Primary horizontal axis.
    PrimaryHorizontalAxis,
    /// Primary vertical axis.
    PrimaryVerticalAxis,
    /// Primary horizontal axis title.
    PrimaryHorizontalAxisTitle,
    /// Primary vertical axis title.
    PrimaryVerticalAxisTitle,
    /// Major gridlines of the value axis.
    MajorGridlines,
    /// Minor gridlines of the value axis.
    MinorGridlines,
    /// Vertical (category) gridlines.
    CategoryGridlines,
    /// The legend (at the right when added).
    Legend,
    /// Data labels (at the default position when added).
    DataLabels,
    /// A linear trendline on every series that can have one.
    LinearTrendline,
    /// An exponential trendline.
    ExponentialTrendline,
    /// A two-period moving average.
    MovingAverage,
    /// Error bars of one standard error.
    StandardErrorBars,
    /// Error bars of 5 % of each value.
    PercentageErrorBars,
    /// Error bars of one standard deviation.
    StandardDeviationErrorBars,
}

/// One edit of Select Data's lists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ChartSeriesOp {
    /// Add a series: its name (a reference such as `=Sheet1!$B$1`, literal text, or empty) and
    /// its values (a reference such as `=Sheet1!$B$2:$B$5`).
    Add {
        /// Series name.
        name: String,
        /// Series values.
        values: String,
    },
    /// Change a series' name and values.
    Edit {
        /// The series' `idx`.
        series: u32,
        /// Series name.
        name: String,
        /// Series values.
        values: String,
    },
    /// Remove a series.
    Remove {
        /// The series' `idx`.
        series: u32,
    },
    /// Move a series one place up or down the plot order.
    Move {
        /// The series' `idx`.
        series: u32,
        /// Towards the first series.
        up: bool,
    },
    /// Point every series' category (horizontal axis) labels at a range.
    Categories {
        /// The axis label range (`=Sheet1!$A$2:$A$5`).
        range: String,
    },
}

/// How a chart shows empty cells (`c:dispBlanksAs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ChartBlanks {
    /// Gaps.
    Gap,
    /// Zero.
    Zero,
    /// Connect data points with a line.
    Span,
}

/// The area a Format Area command changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ChartAreaTarget {
    /// The whole chart.
    ChartArea,
    /// The plot area.
    PlotArea,
}

/// Fill, border and effects of an area as [`ChartState`] reports them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ChartAreaState {
    /// The solid fill (`RRGGBB`); `None` when automatic or not solid.
    pub fill: Option<String>,
    /// No fill.
    pub no_fill: bool,
    /// The border colour (`RRGGBB`); `None` when automatic.
    pub line: Option<String>,
    /// No border.
    pub no_line: bool,
    /// The border width in points (0 when automatic).
    pub line_width: f64,
    /// An outer shadow.
    pub shadow: bool,
    /// Rounded corners.
    pub rounded: bool,
}

/// Legend positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ChartLegendPosition {
    /// Right.
    Right,
    /// Top.
    Top,
    /// Left.
    Left,
    /// Bottom.
    Bottom,
    /// Top right corner.
    TopRight,
}

/// Data label positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ChartLabelPosition {
    /// Centre.
    Center,
    /// Inside end.
    InsideEnd,
    /// Inside base.
    InsideBase,
    /// Outside end.
    OutsideEnd,
    /// Best fit (pie).
    BestFit,
    /// Left.
    Left,
    /// Right.
    Right,
    /// Above.
    Above,
    /// Below.
    Below,
}

/// Where Move Chart puts a chart.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ChartMoveTarget {
    /// A new chart sheet with this name.
    NewSheet {
        /// The sheet name.
        name: String,
    },
    /// An object on an existing sheet.
    ObjectIn {
        /// The sheet name.
        sheet: String,
    },
}

/// The axis a Format Axis command changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ChartAxisTarget {
    /// The primary value (vertical) axis.
    PrimaryValue,
    /// The primary category (horizontal) axis.
    PrimaryCategory,
    /// The secondary value axis.
    SecondaryValue,
    /// The secondary category axis.
    SecondaryCategory,
}

/// An axis option that is either automatic or fixed (Format pane's "Auto" / "Reset").
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub enum ChartBound {
    /// Office chooses (or, for a log base, no log scale).
    Auto,
    /// A fixed value.
    Fixed(f64),
}

/// What the chart chrome (Chart Design and Format tabs, Format Chart pane, Select Data) shows
/// about the selected chart. `kiln_charts::edit::chart_state` computes it from the chart and its
/// layout, so the chrome holds no chart logic.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ChartState {
    /// The chart type (`None` for a type Kiln does not edit, such as a 3-D surface).
    pub kind: Option<ChartKind>,
    /// Office chart style number (1–48), when the chart has one.
    pub style: Option<u8>,
    /// The current Change Colors set: 0 colourful, 1–6 shades of accent n, 7 greys.
    pub color_set: u8,
    /// The first six colours (`RRGGBB`) of each Change Colors set, 0–7, in the document's theme.
    pub color_sets: Vec<Vec<String>>,
    /// The title as drawn; `None` when the chart shows no title.
    pub title: Option<String>,
    /// The legend position; `None` without a legend.
    pub legend: Option<ChartLegendPosition>,
    /// The data label position of the first labelled series; `None` without data labels.
    pub data_labels: Option<ChartLabelPosition>,
    /// The Add Chart Element items that are on.
    pub elements: Vec<ChartElementKind>,
    /// Select Data's chart data range (`Sheet1!$A$1:$D$5`); empty for literal data.
    pub data_range: String,
    /// The series, in plot order.
    pub series: Vec<ChartSeriesState>,
    /// The category (horizontal axis) labels.
    pub categories: Vec<String>,
    /// Whether the chart reads cells (Write and Deck charts with literal data do not).
    pub has_cells: bool,
    /// The axes the Format pane can format.
    pub axes: Vec<ChartAxisState>,
    /// Select Data ▸ Hidden and Empty Cells: how empty cells show.
    pub blanks: ChartBlanks,
    /// Select Data ▸ Hidden and Empty Cells: data in hidden rows and columns is plotted.
    pub show_hidden: bool,
    /// Select Data ▸ the category label range (`Sheet1!$A$2:$A$5`); empty for literal labels.
    pub categories_range: String,
    /// Format pane ▸ Chart Area.
    pub chart_area: ChartAreaState,
    /// Format pane ▸ Plot Area.
    pub plot_area: ChartAreaState,
}

/// One series of [`ChartState`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ChartSeriesState {
    /// The series' `idx` (what `ChartFormatSeries` addresses).
    pub idx: u32,
    /// The name as the legend shows it.
    pub name: String,
    /// The colour drawn (`RRGGBB`): the fill, or the line of a line series.
    pub color: String,
    /// The solid fill set on the series (`RRGGBB`); `None` when automatic.
    pub fill: Option<String>,
    /// The solid line colour set on the series (`RRGGBB`); `None` when automatic.
    pub line: Option<String>,
    /// The line width in points as drawn.
    pub line_width: f64,
    /// Plotted on the secondary axis.
    pub secondary: bool,
    /// The name's reference (`=Sheet1!$B$1`) or its literal text, for Select Data ▸ Edit.
    pub name_ref: String,
    /// The values' reference (`=Sheet1!$B$2:$B$5`); empty for literal values.
    pub values_ref: String,
}

/// One axis of [`ChartState`], with the bounds the chart draws (fixed or automatic).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ChartAxisState {
    /// Which axis (what `ChartFormatAxis` addresses).
    pub target: ChartAxisTarget,
    /// The axis title, or a description (`Vertical (Value) Axis`).
    pub label: String,
    /// A value axis (bounds and units apply) rather than a category axis.
    pub is_value: bool,
    /// The minimum drawn (value axes).
    pub min: Option<f64>,
    /// The maximum drawn (value axes).
    pub max: Option<f64>,
    /// The major unit drawn (value axes; the factor between ticks on a log scale).
    pub major_unit: Option<f64>,
    /// The minimum is automatic.
    pub min_auto: bool,
    /// The maximum is automatic.
    pub max_auto: bool,
    /// The major unit is automatic.
    pub major_unit_auto: bool,
    /// The logarithm base of a log scale.
    pub log_base: Option<f64>,
    /// Values (or categories) in reverse order.
    pub reversed: bool,
    /// The number format code; `None` when linked to the source.
    pub number_format: Option<String>,
}
