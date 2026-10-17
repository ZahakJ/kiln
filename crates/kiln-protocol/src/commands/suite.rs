// SPDX-License-Identifier: AGPL-3.0-or-later
//! Suite-wide commands: files, undo/redo, view and diagnostics. Owned by WP-52; later suite work
//! packages append variants and rows here.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::app::{AppMask, DocKind, FileFormat};
use crate::print::PrintSettings;
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
    /// resets the painter and resends everything visible (WP-54).
    Repaint,
    /// Round trip with no effect; the reply carries the core and protocol versions.
    Ping,
    /// Round trip that returns its text (bridge diagnostics).
    Echo {
        /// Any text.
        text: String,
    },
    // ── Charts (WP-73): shared by Grid, Write and Deck; the app's document model applies them
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
    // ── Print and PDF (WP-67).
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
    Self::PrintPdf { .. } => {
        id: "suite.file.print_pdf", keytip: None, group: "file",
        tier: P0, apps: AppMask::WRITE, availability: Document,
    };
    Self::SavePdf { .. } => {
        id: "suite.file.save_pdf", keytip: None, group: "file",
        tier: P0, apps: AppMask::WRITE, availability: Document,
    };
}

impl SuiteCommand {
    /// The smallest and largest zoom factors (10 %–500 %, as in Office).
    pub const ZOOM_RANGE: std::ops::RangeInclusive<f32> = 0.1..=5.0;
}

// ── Charts (WP-73) ───────────────────────────────────────────────────────────────────────────

/// The apps that host charts.
const CHART_APPS: AppMask = AppMask::GRID.union(AppMask::WRITE).union(AppMask::DECK);

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
}

impl ChartKind {
    /// Every kind, in gallery order.
    pub const ALL: [ChartKind; 28] = [
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
