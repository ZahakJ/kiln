// SPDX-License-Identifier: AGPL-3.0-or-later
//! The chart model: a chart space with one chart, its plot area of chart groups and axes, the
//! series with their data references and cached values, titles, legend, data labels and the
//! formatting of every element.
//!
//! The model is shaped like DrawingML charts (`c:chartSpace`, ECMA-376 Part 1 §21.2) so Office
//! files map onto it one to one, and Excel 2016's `cx:chartSpace` (treemap, sunburst) maps onto the
//! same types with a [`GroupKind::Hierarchy`] group. Optional fields are `None` when the file did
//! not say, and the writer then leaves them out, so a round trip writes back what was read.
//! Elements the model does not cover are kept in [`Extras`] next to the element they belonged to.

use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::line::Line;
use kiln_ooxml::xml::XmlElement;

pub use crate::xmlx::Extras;

/// A chart part: the root of `xl/charts/chartN.xml` (or `chartExN.xml`).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ChartSpace {
    /// Which XML vocabulary the part uses.
    pub format: PartFormat,
    /// `c:date1904`.
    pub date1904: Option<bool>,
    /// `c:lang`.
    pub lang: Option<String>,
    /// `c:roundedCorners`.
    pub rounded_corners: Option<bool>,
    /// `c:style` (Office 2007 chart style 1–48); `mc:AlternateContent` with `c14:style` is kept
    /// in [`ChartSpace::extras`].
    pub style: Option<u32>,
    /// The chart.
    pub chart: Chart,
    /// Chart area fill and outline.
    pub shape: ShapeProps,
    /// Default text properties of the whole chart.
    pub text: TextProps,
    /// `c:externalData`: the embedded or linked workbook.
    pub external_data: Option<ExternalData>,
    /// Unknown children (`c:clrMapOvr`, `c:pivotSource`, `c:printSettings`, `c:userShapes`,
    /// `mc:AlternateContent`, `c:extLst`).
    pub extras: Extras,
    /// Namespace declarations and other attributes of the root element, kept for the writer.
    pub root_attrs: Vec<(String, String)>,
}

/// The XML vocabulary of a chart part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PartFormat {
    /// `c:chartSpace` (DrawingML charts, Office 2007).
    #[default]
    Classic,
    /// `cx:chartSpace` (chartex, Office 2016: treemap, sunburst, …).
    ChartEx,
}

impl PartFormat {
    /// The part's content type.
    #[must_use]
    pub const fn content_type(self) -> &'static str {
        match self {
            Self::Classic => "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
            Self::ChartEx => "application/vnd.ms-office.chartex+xml",
        }
    }

    /// The relationship type from the drawing (or slide, or document) to the part.
    #[must_use]
    pub const fn relationship_type(self) -> &'static str {
        match self {
            Self::Classic => {
                "http://schemas.openxmlformats.org/officeDocument/2006/relationships/chart"
            }
            Self::ChartEx => "http://schemas.microsoft.com/office/2014/relationships/chartEx",
        }
    }
}

/// `c:externalData`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExternalData {
    /// Relationship id of the package or link.
    pub rel_id: String,
    /// `c:autoUpdate`.
    pub auto_update: Option<bool>,
}

/// `c:chart`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Chart {
    /// The chart title.
    pub title: Option<Title>,
    /// `c:autoTitleDeleted`.
    pub auto_title_deleted: Option<bool>,
    /// The plot area.
    pub plot_area: PlotArea,
    /// The legend.
    pub legend: Option<Legend>,
    /// `c:plotVisOnly`.
    pub plot_vis_only: Option<bool>,
    /// `c:dispBlanksAs`.
    pub disp_blanks_as: Option<BlanksAs>,
    /// Unknown children (`c:pivotFmts`, `c:view3D`, `c:floor`, walls, `c:extLst`).
    pub extras: Extras,
}

/// How empty cells are plotted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlanksAs {
    /// Leave a gap.
    Gap,
    /// Plot as zero.
    Zero,
    /// Connect the neighbours with a line.
    Span,
}

/// `c:plotArea`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlotArea {
    /// Manual layout, if any.
    pub layout: Option<ManualLayout>,
    /// The chart groups, in file order (a combo chart has more than one).
    pub groups: Vec<ChartGroup>,
    /// The axes, in file order.
    pub axes: Vec<Axis>,
    /// Plot area fill and outline.
    pub shape: ShapeProps,
    /// Unknown children (`c:dTable`, `c:extLst`).
    pub extras: Extras,
}

/// `c:layout/c:manualLayout`: a position as fractions of the chart area.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ManualLayout {
    /// `c:layoutTarget` (`inner` = the plot area without tick labels).
    pub inner: Option<bool>,
    /// x as a fraction of the chart width (`edge` mode) or an offset (`factor` mode).
    pub x: Option<f64>,
    /// y.
    pub y: Option<f64>,
    /// Width as a fraction.
    pub w: Option<f64>,
    /// Height as a fraction.
    pub h: Option<f64>,
    /// `xMode`/`yMode` are `factor` (offsets from the automatic position) rather than `edge`.
    pub factor_mode: bool,
    /// The original element, written back when the layout was not edited.
    pub raw: Option<XmlElement>,
}

/// A chart group: one chart type with its series and the axes it plots on.
#[derive(Debug, Clone, PartialEq)]
pub struct ChartGroup {
    /// The type and its options.
    pub kind: GroupKind,
    /// `c:varyColors`: one colour per point (pie charts).
    pub vary_colors: Option<bool>,
    /// The series.
    pub series: Vec<Series>,
    /// Data labels of the whole group.
    pub data_labels: Option<DataLabels>,
    /// The axis ids (two for most types, three for 3-D surfaces; none for pies).
    pub axis_ids: Vec<i64>,
    /// Unknown children (`c:serLines`, `c:dropLines`, `c:hiLowLines`, `c:upDownBars`,
    /// `c:shape`, `c:gapDepth`, `c:extLst`).
    pub extras: Extras,
}

/// Bar direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarDir {
    /// Vertical bars (a column chart).
    Column,
    /// Horizontal bars.
    Bar,
}

/// How the series of a group combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grouping {
    /// Side by side (bar) or independent (line, area).
    Standard,
    /// Side by side (bar charts call `standard` `clustered`).
    Clustered,
    /// Stacked.
    Stacked,
    /// Stacked to 100 %.
    PercentStacked,
}

impl Grouping {
    /// Whether the values are stacked.
    #[must_use]
    pub const fn is_stacked(self) -> bool {
        matches!(self, Self::Stacked | Self::PercentStacked)
    }
}

/// `c:scatterStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScatterStyle {
    /// Markers only (Excel writes `lineMarker` and hides the lines in `c:spPr`).
    Marker,
    /// Straight lines with markers.
    LineMarker,
    /// Straight lines.
    Line,
    /// Smooth lines.
    Smooth,
    /// Smooth lines with markers.
    SmoothMarker,
    /// `none`.
    None,
}

/// `c:radarStyle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarStyle {
    /// Lines.
    Standard,
    /// Lines with markers.
    Marker,
    /// Filled polygons.
    Filled,
}

/// The layout of a hierarchy (chartex) series.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchyLayout {
    /// Nested rectangles.
    Treemap,
    /// Concentric rings.
    Sunburst,
}

/// The chart type of a group and its type-specific options.
#[derive(Debug, Clone, PartialEq)]
pub enum GroupKind {
    /// `c:barChart` / `c:bar3DChart`.
    Bar {
        /// Direction.
        dir: BarDir,
        /// Grouping.
        grouping: Option<Grouping>,
        /// `c:gapWidth` (percent of a bar width, 0–500; Office's default is 150).
        gap_width: Option<u32>,
        /// `c:overlap` (−100–100).
        overlap: Option<i32>,
        /// Read from `c:bar3DChart` (drawn in 2-D).
        three_d: bool,
    },
    /// `c:lineChart` / `c:line3DChart`.
    Line {
        /// Grouping.
        grouping: Option<Grouping>,
        /// `c:marker` (show markers on the group's lines).
        markers: Option<bool>,
        /// Read from `c:line3DChart`.
        three_d: bool,
    },
    /// `c:areaChart` / `c:area3DChart`.
    Area {
        /// Grouping.
        grouping: Option<Grouping>,
        /// Read from `c:area3DChart`.
        three_d: bool,
    },
    /// `c:pieChart` / `c:pie3DChart`.
    Pie {
        /// `c:firstSliceAng` in degrees.
        first_slice_angle: Option<u32>,
        /// Read from `c:pie3DChart`.
        three_d: bool,
    },
    /// `c:doughnutChart`.
    Doughnut {
        /// `c:firstSliceAng`.
        first_slice_angle: Option<u32>,
        /// `c:holeSize` in percent (10–90).
        hole_size: Option<u32>,
    },
    /// `c:scatterChart`.
    Scatter {
        /// Style.
        style: Option<ScatterStyle>,
    },
    /// `c:bubbleChart`.
    Bubble {
        /// `c:bubble3D`.
        bubble_3d: Option<bool>,
        /// `c:bubbleScale` in percent.
        scale: Option<u32>,
        /// `c:showNegBubbles`.
        show_negative: Option<bool>,
        /// `c:sizeRepresents` is `w` (width) instead of area.
        size_is_width: Option<bool>,
    },
    /// `c:radarChart`.
    Radar {
        /// Style.
        style: Option<RadarStyle>,
    },
    /// A chartex hierarchy chart (treemap or sunburst).
    Hierarchy {
        /// Layout.
        layout: HierarchyLayout,
        /// `cx:parentLabelLayout` (treemap: `overlapping`, `banner`, `none`).
        parent_label_layout: Option<String>,
    },
    /// A group the model does not cover (`c:ofPieChart`, `c:stockChart`, `c:surfaceChart`, …),
    /// kept as written and drawn as nothing.
    Other(XmlElement),
}

impl GroupKind {
    /// Whether the group plots against a category axis and a value axis.
    #[must_use]
    pub fn uses_category_axis(&self) -> bool {
        matches!(
            self,
            Self::Bar { .. } | Self::Line { .. } | Self::Area { .. } | Self::Radar { .. }
        )
    }

    /// Whether the group has no axes.
    #[must_use]
    pub fn is_circular(&self) -> bool {
        matches!(
            self,
            Self::Pie { .. } | Self::Doughnut { .. } | Self::Hierarchy { .. }
        )
    }

    /// Whether x values are numbers (scatter and bubble).
    #[must_use]
    pub fn is_xy(&self) -> bool {
        matches!(self, Self::Scatter { .. } | Self::Bubble { .. })
    }

    /// The grouping, for the types that have one.
    #[must_use]
    pub fn grouping(&self) -> Option<Grouping> {
        match self {
            Self::Bar { grouping, .. }
            | Self::Line { grouping, .. }
            | Self::Area { grouping, .. } => *grouping,
            _ => None,
        }
    }
}

/// A data series.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Series {
    /// `c:idx`: the series index (selects the automatic colour).
    pub idx: u32,
    /// `c:order`: the plotting order.
    pub order: u32,
    /// The series name.
    pub name: Option<SeriesText>,
    /// Fill and outline.
    pub shape: ShapeProps,
    /// `c:invertIfNegative`.
    pub invert_if_negative: Option<bool>,
    /// `c:explosion` in percent of the radius (pie slices).
    pub explosion: Option<u32>,
    /// The marker (line, scatter, radar).
    pub marker: Option<Marker>,
    /// Per-point formatting.
    pub points: Vec<DataPoint>,
    /// Data labels.
    pub data_labels: Option<DataLabels>,
    /// Trendlines.
    pub trendlines: Vec<Trendline>,
    /// Categories (`c:cat`) or x values (`c:xVal`).
    pub categories: Option<DataSource>,
    /// Values (`c:val`) or y values (`c:yVal`).
    pub values: Option<DataSource>,
    /// Bubble sizes.
    pub bubble_sizes: Option<DataSource>,
    /// `c:smooth`.
    pub smooth: Option<bool>,
    /// `c:bubble3D`.
    pub bubble_3d: Option<bool>,
    /// Plot on the secondary axes (set by the reader from the group's axis ids; the writer does not
    /// use it).
    pub secondary: bool,
    /// Unknown children (`c:errBars`, `c:pictureOptions`, `c:shape`, `c:extLst`).
    pub extras: Extras,
    /// Attributes of a chartex `cx:series` other than `layoutId` (`uniqueId`, `formatIdx`).
    pub attrs: Vec<(String, String)>,
}

/// `c:tx` of a series: a cell reference with its cached text, or literal text.
#[derive(Debug, Clone, PartialEq)]
pub enum SeriesText {
    /// `c:strRef`.
    Ref {
        /// The formula (`Sheet1!$B$1`).
        formula: String,
        /// The cached text.
        cache: Option<String>,
    },
    /// `c:v`.
    Literal(String),
}

impl SeriesText {
    /// The text to show.
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Ref { cache, .. } => cache.as_deref(),
            Self::Literal(s) => Some(s),
        }
    }
}

/// Where a series' numbers or labels come from.
#[derive(Debug, Clone, PartialEq)]
pub enum DataSource {
    /// Numbers: `c:numRef` (with a formula) or `c:numLit`.
    Num(NumData),
    /// Text: `c:strRef` or `c:strLit`.
    Str(StrData),
    /// Multi-level text categories: `c:multiLvlStrRef`.
    MultiLevel(MultiLevelData),
}

impl DataSource {
    /// The formula, if the data is a reference.
    #[must_use]
    pub fn formula(&self) -> Option<&str> {
        match self {
            Self::Num(n) => n.formula.as_deref(),
            Self::Str(s) => s.formula.as_deref(),
            Self::MultiLevel(m) => Some(&m.formula),
        }
    }

    /// Number of points.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::Num(n) => n.points.len(),
            Self::Str(s) => s.points.len(),
            Self::MultiLevel(m) => m.levels.first().map_or(0, Vec::len),
        }
    }

    /// Whether there are no points.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The label of point `i` (numbers formatted with `General` unless a format is given).
    #[must_use]
    pub fn label(&self, i: usize) -> Option<String> {
        match self {
            Self::Num(n) => n.points.get(i).copied().flatten().map(|v| {
                crate::format::format_number(v, n.format_code.as_deref().unwrap_or("General"))
            }),
            Self::Str(s) => s.points.get(i).cloned().flatten(),
            Self::MultiLevel(m) => m.levels.first().and_then(|l| l.get(i).cloned().flatten()),
        }
    }

    /// The number at point `i` (text that parses as a number counts).
    #[must_use]
    pub fn number(&self, i: usize) -> Option<f64> {
        match self {
            Self::Num(n) => n.points.get(i).copied().flatten(),
            Self::Str(s) => s
                .points
                .get(i)
                .and_then(|p| p.as_deref())
                .and_then(|t| t.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite()),
            Self::MultiLevel(_) => None,
        }
    }
}

/// Numbers with an optional reference.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct NumData {
    /// The formula (`None` for literal data, as in Write and Deck charts without a workbook).
    pub formula: Option<String>,
    /// `c:formatCode` of the cache.
    pub format_code: Option<String>,
    /// The values (`None` = blank).
    pub points: Vec<Option<f64>>,
    /// Whether the file had a cache (`c:numCache`); the writer only writes one when it did or when
    /// the data was refreshed.
    pub has_cache: bool,
    /// Unknown children of the reference (`c:extLst`).
    pub extras: Extras,
}

/// Text with an optional reference.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StrData {
    /// The formula.
    pub formula: Option<String>,
    /// The values.
    pub points: Vec<Option<String>>,
    /// Whether the file had a cache.
    pub has_cache: bool,
    /// Unknown children of the reference.
    pub extras: Extras,
}

/// Multi-level categories: level 0 is the innermost (the one next to the axis).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MultiLevelData {
    /// The formula.
    pub formula: String,
    /// The levels; each has one entry per point (`None` where a parent label spans points).
    pub levels: Vec<Vec<Option<String>>>,
    /// Whether the file had a cache.
    pub has_cache: bool,
    /// Unknown children.
    pub extras: Extras,
}

/// `c:dPt`: formatting of one data point.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataPoint {
    /// The point index.
    pub idx: u32,
    /// `c:invertIfNegative`.
    pub invert_if_negative: Option<bool>,
    /// The marker.
    pub marker: Option<Marker>,
    /// `c:bubble3D`.
    pub bubble_3d: Option<bool>,
    /// `c:explosion` (pie slices).
    pub explosion: Option<u32>,
    /// Fill and outline.
    pub shape: ShapeProps,
    /// Unknown children.
    pub extras: Extras,
}

/// Marker symbols.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum MarkerSymbol {
    Auto,
    None,
    Circle,
    Dash,
    Diamond,
    Dot,
    Picture,
    Plus,
    Square,
    Star,
    Triangle,
    X,
}

impl MarkerSymbol {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "auto" => Self::Auto,
            "none" => Self::None,
            "circle" => Self::Circle,
            "dash" => Self::Dash,
            "diamond" => Self::Diamond,
            "dot" => Self::Dot,
            "picture" => Self::Picture,
            "plus" => Self::Plus,
            "square" => Self::Square,
            "star" => Self::Star,
            "triangle" => Self::Triangle,
            "x" => Self::X,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::None => "none",
            Self::Circle => "circle",
            Self::Dash => "dash",
            Self::Diamond => "diamond",
            Self::Dot => "dot",
            Self::Picture => "picture",
            Self::Plus => "plus",
            Self::Square => "square",
            Self::Star => "star",
            Self::Triangle => "triangle",
            Self::X => "x",
        }
    }
}

/// `c:marker`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Marker {
    /// Symbol.
    pub symbol: Option<MarkerSymbol>,
    /// Size in points (2–72).
    pub size: Option<u32>,
    /// Fill and outline.
    pub shape: ShapeProps,
    /// Unknown children.
    pub extras: Extras,
}

/// Where data labels sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum LabelPosition {
    BestFit,
    Bottom,
    Center,
    InsideBase,
    InsideEnd,
    Left,
    OutsideEnd,
    Right,
    Top,
}

impl LabelPosition {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "bestFit" => Self::BestFit,
            "b" => Self::Bottom,
            "ctr" => Self::Center,
            "inBase" => Self::InsideBase,
            "inEnd" => Self::InsideEnd,
            "l" => Self::Left,
            "outEnd" => Self::OutsideEnd,
            "r" => Self::Right,
            "t" => Self::Top,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::BestFit => "bestFit",
            Self::Bottom => "b",
            Self::Center => "ctr",
            Self::InsideBase => "inBase",
            Self::InsideEnd => "inEnd",
            Self::Left => "l",
            Self::OutsideEnd => "outEnd",
            Self::Right => "r",
            Self::Top => "t",
        }
    }
}

/// `c:dLbls`: data labels of a series or a group.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DataLabels {
    /// `c:delete`: no labels at all.
    pub delete: Option<bool>,
    /// Number format.
    pub num_fmt: Option<NumFmt>,
    /// Fill and outline of the label boxes.
    pub shape: ShapeProps,
    /// Text properties.
    pub text: TextProps,
    /// Position.
    pub position: Option<LabelPosition>,
    /// `c:showLegendKey`.
    pub show_legend_key: Option<bool>,
    /// `c:showVal`.
    pub show_value: Option<bool>,
    /// `c:showCatName`.
    pub show_category: Option<bool>,
    /// `c:showSerName`.
    pub show_series: Option<bool>,
    /// `c:showPercent`.
    pub show_percent: Option<bool>,
    /// `c:showBubbleSize`.
    pub show_bubble_size: Option<bool>,
    /// `c:separator`.
    pub separator: Option<String>,
    /// `c:showLeaderLines`.
    pub show_leader_lines: Option<bool>,
    /// Per-point labels (`c:dLbl`), kept as written.
    pub point_labels: Vec<XmlElement>,
    /// Unknown children (`c:leaderLines`, `c:extLst`).
    pub extras: Extras,
}

impl DataLabels {
    /// Whether any label content is switched on.
    #[must_use]
    pub fn shows_anything(&self) -> bool {
        self.delete != Some(true)
            && [
                self.show_value,
                self.show_category,
                self.show_series,
                self.show_percent,
                self.show_bubble_size,
            ]
            .contains(&Some(true))
    }
}

/// `c:numFmt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumFmt {
    /// The format code.
    pub code: String,
    /// `sourceLinked`: take the format from the cells.
    pub source_linked: Option<bool>,
}

/// Trendline types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum TrendlineKind {
    Linear,
    Exponential,
    Logarithmic,
    MovingAverage,
    Polynomial,
    Power,
}

impl TrendlineKind {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "linear" => Self::Linear,
            "exp" => Self::Exponential,
            "log" => Self::Logarithmic,
            "movingAvg" => Self::MovingAverage,
            "poly" => Self::Polynomial,
            "power" => Self::Power,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Linear => "linear",
            Self::Exponential => "exp",
            Self::Logarithmic => "log",
            Self::MovingAverage => "movingAvg",
            Self::Polynomial => "poly",
            Self::Power => "power",
        }
    }
}

/// `c:trendline`.
#[derive(Debug, Clone, PartialEq)]
pub struct Trendline {
    /// `c:name`.
    pub name: Option<String>,
    /// Line formatting.
    pub shape: ShapeProps,
    /// Type.
    pub kind: TrendlineKind,
    /// `c:order` (polynomial).
    pub order: Option<u32>,
    /// `c:period` (moving average).
    pub period: Option<u32>,
    /// `c:forward`.
    pub forward: Option<f64>,
    /// `c:backward`.
    pub backward: Option<f64>,
    /// `c:intercept`.
    pub intercept: Option<f64>,
    /// `c:dispRSqr`.
    pub show_r_squared: Option<bool>,
    /// `c:dispEq`.
    pub show_equation: Option<bool>,
    /// Unknown children (`c:trendlineLbl`, `c:extLst`).
    pub extras: Extras,
}

/// Axis kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AxisKind {
    /// `c:catAx`.
    Category,
    /// `c:valAx`.
    Value,
    /// `c:dateAx`.
    Date,
    /// `c:serAx` (3-D depth).
    Series,
}

impl AxisKind {
    pub(crate) const fn element(self) -> &'static str {
        match self {
            Self::Category => "c:catAx",
            Self::Value => "c:valAx",
            Self::Date => "c:dateAx",
            Self::Series => "c:serAx",
        }
    }
}

/// The side of the plot area an axis is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum AxisPos {
    Bottom,
    Left,
    Right,
    Top,
}

impl AxisPos {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "b" => Self::Bottom,
            "l" => Self::Left,
            "r" => Self::Right,
            "t" => Self::Top,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "b",
            Self::Left => "l",
            Self::Right => "r",
            Self::Top => "t",
        }
    }

    /// Whether the axis runs horizontally.
    #[must_use]
    pub const fn is_horizontal(self) -> bool {
        matches!(self, Self::Bottom | Self::Top)
    }
}

/// Tick mark styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum TickMark {
    Cross,
    In,
    None,
    Out,
}

impl TickMark {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "cross" => Self::Cross,
            "in" => Self::In,
            "none" => Self::None,
            "out" => Self::Out,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Cross => "cross",
            Self::In => "in",
            Self::None => "none",
            Self::Out => "out",
        }
    }
}

/// Where tick labels go.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum TickLabelPos {
    High,
    Low,
    NextTo,
    None,
}

impl TickLabelPos {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "high" => Self::High,
            "low" => Self::Low,
            "nextTo" => Self::NextTo,
            "none" => Self::None,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::High => "high",
            Self::Low => "low",
            Self::NextTo => "nextTo",
            Self::None => "none",
        }
    }
}

/// Where the crossing axis crosses this one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Crosses {
    /// At zero (or the minimum when zero is outside the range).
    AutoZero,
    /// At the minimum.
    Min,
    /// At the maximum.
    Max,
    /// At a value (`c:crossesAt`).
    At(f64),
}

/// `c:scaling`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scaling {
    /// `c:logBase`.
    pub log_base: Option<f64>,
    /// `c:orientation` is `maxMin` (reversed).
    pub reversed: Option<bool>,
    /// `c:max`.
    pub max: Option<f64>,
    /// `c:min`.
    pub min: Option<f64>,
    /// Unknown children.
    pub extras: Extras,
}

/// Date axis time units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum TimeUnit {
    Days,
    Months,
    Years,
}

impl TimeUnit {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "days" => Self::Days,
            "months" => Self::Months,
            "years" => Self::Years,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Days => "days",
            Self::Months => "months",
            Self::Years => "years",
        }
    }
}

/// An axis.
#[derive(Debug, Clone, PartialEq)]
pub struct Axis {
    /// Kind.
    pub kind: AxisKind,
    /// `c:axId`.
    pub id: i64,
    /// Scaling.
    pub scaling: Scaling,
    /// `c:delete`: the axis is hidden.
    pub delete: Option<bool>,
    /// `c:axPos`.
    pub position: Option<AxisPos>,
    /// Major gridlines (formatting; `Some` = shown).
    pub major_gridlines: Option<ShapeProps>,
    /// Minor gridlines.
    pub minor_gridlines: Option<ShapeProps>,
    /// Axis title.
    pub title: Option<Title>,
    /// Number format of the tick labels.
    pub num_fmt: Option<NumFmt>,
    /// `c:majorTickMark`.
    pub major_tick: Option<TickMark>,
    /// `c:minorTickMark`.
    pub minor_tick: Option<TickMark>,
    /// `c:tickLblPos`.
    pub tick_label_pos: Option<TickLabelPos>,
    /// Axis line formatting.
    pub shape: ShapeProps,
    /// Tick label text properties (rotation lives in `a:bodyPr/@rot`).
    pub text: TextProps,
    /// `c:crossAx`.
    pub cross_axis: i64,
    /// `c:crosses` / `c:crossesAt`.
    pub crosses: Option<Crosses>,
    /// `c:crossBetween` is `between` (categories sit between ticks) rather than `midCat`.
    pub cross_between: Option<bool>,
    /// `c:majorUnit`.
    pub major_unit: Option<f64>,
    /// `c:minorUnit`.
    pub minor_unit: Option<f64>,
    /// `c:auto` (category axes: detect dates).
    pub auto: Option<bool>,
    /// `c:lblAlgn`.
    pub label_align: Option<String>,
    /// `c:lblOffset`.
    pub label_offset: Option<u32>,
    /// `c:tickLblSkip`.
    pub tick_label_skip: Option<u32>,
    /// `c:tickMarkSkip`.
    pub tick_mark_skip: Option<u32>,
    /// `c:noMultiLvlLbl`.
    pub no_multi_level: Option<bool>,
    /// `c:baseTimeUnit`.
    pub base_time_unit: Option<TimeUnit>,
    /// `c:majorTimeUnit`.
    pub major_time_unit: Option<TimeUnit>,
    /// `c:minorTimeUnit`.
    pub minor_time_unit: Option<TimeUnit>,
    /// Unknown children (`c:dispUnits`, `c:extLst`).
    pub extras: Extras,
}

impl Axis {
    /// A new axis of `kind` at `position` crossing `cross_axis`, with Office's defaults for a new
    /// chart.
    #[must_use]
    pub fn new(kind: AxisKind, id: i64, position: AxisPos, cross_axis: i64) -> Self {
        let value = kind == AxisKind::Value;
        Self {
            kind,
            id,
            scaling: Scaling {
                reversed: Some(false),
                ..Scaling::default()
            },
            delete: Some(false),
            position: Some(position),
            major_gridlines: None,
            minor_gridlines: None,
            title: None,
            num_fmt: Some(NumFmt {
                code: "General".into(),
                source_linked: Some(true),
            }),
            major_tick: Some(TickMark::None),
            minor_tick: Some(TickMark::None),
            tick_label_pos: Some(TickLabelPos::NextTo),
            shape: ShapeProps::default(),
            text: TextProps::default(),
            cross_axis,
            crosses: Some(Crosses::AutoZero),
            cross_between: value.then_some(true),
            major_unit: None,
            minor_unit: None,
            auto: (!value).then_some(true),
            label_align: (!value).then(|| "ctr".to_owned()),
            label_offset: (!value).then_some(100),
            tick_label_skip: None,
            tick_mark_skip: None,
            no_multi_level: (!value).then_some(false),
            base_time_unit: None,
            major_time_unit: None,
            minor_time_unit: None,
            extras: Extras::default(),
        }
    }

    /// Whether the axis is drawn.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.delete != Some(true)
    }

    /// Whether the axis runs from maximum to minimum.
    #[must_use]
    pub fn is_reversed(&self) -> bool {
        self.scaling.reversed == Some(true)
    }
}

/// A title (chart or axis).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Title {
    /// The text (`None` = the automatic title: the series name for single-series charts, or
    /// "Chart Title"/"Axis Title").
    pub text: Option<TitleText>,
    /// Manual layout.
    pub layout: Option<ManualLayout>,
    /// `c:overlay`.
    pub overlay: Option<bool>,
    /// Fill and outline.
    pub shape: ShapeProps,
    /// Text properties.
    pub text_props: TextProps,
    /// Unknown children.
    pub extras: Extras,
    /// Attributes of a chartex `cx:title` (`pos`, `align`, `overlay`).
    pub attrs: Vec<(String, String)>,
}

/// The text of a title.
#[derive(Debug, Clone, PartialEq)]
pub enum TitleText {
    /// `c:rich`: formatted text. `plain` is the text with paragraphs joined by `\n`; `raw` is the
    /// original element, written back while the text is unchanged.
    Rich {
        /// The text.
        plain: String,
        /// The first run's properties (size in hundredths of a point, bold, colour), used when
        /// the text is replaced.
        run: RunProps,
        /// The original `c:rich`, if the text was read and not edited.
        raw: Option<XmlElement>,
    },
    /// `c:strRef`: the text of a cell.
    Ref {
        /// The formula.
        formula: String,
        /// The cached text.
        cache: Option<String>,
    },
}

impl TitleText {
    /// Plain text.
    #[must_use]
    pub fn plain(&self) -> Option<&str> {
        match self {
            Self::Rich { plain, .. } => Some(plain),
            Self::Ref { cache, .. } => cache.as_deref(),
        }
    }

    /// Rich text of `text` with default run properties.
    #[must_use]
    pub fn rich(text: impl Into<String>) -> Self {
        Self::Rich {
            plain: text.into(),
            run: RunProps::default(),
            raw: None,
        }
    }
}

/// Legend positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum LegendPos {
    Bottom,
    TopRight,
    Left,
    Right,
    Top,
}

impl LegendPos {
    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "b" => Self::Bottom,
            "tr" => Self::TopRight,
            "l" => Self::Left,
            "r" => Self::Right,
            "t" => Self::Top,
            _ => return None,
        })
    }

    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Bottom => "b",
            Self::TopRight => "tr",
            Self::Left => "l",
            Self::Right => "r",
            Self::Top => "t",
        }
    }
}

/// `c:legend`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Legend {
    /// Position (Office's default is right).
    pub position: Option<LegendPos>,
    /// `c:legendEntry` elements (hidden entries, entry formatting), kept as written.
    pub entries: Vec<XmlElement>,
    /// Manual layout.
    pub layout: Option<ManualLayout>,
    /// `c:overlay`.
    pub overlay: Option<bool>,
    /// Fill and outline.
    pub shape: ShapeProps,
    /// Text properties.
    pub text: TextProps,
    /// Unknown children.
    pub extras: Extras,
    /// Attributes of a chartex `cx:legend` other than `pos` and `overlay` (`align`).
    pub attrs: Vec<(String, String)>,
}

impl Legend {
    /// The indices of legend entries marked deleted.
    #[must_use]
    pub fn deleted_entries(&self) -> Vec<u32> {
        self.entries
            .iter()
            .filter(|e| crate::xmlx::val_bool(e, "c:delete") == Some(true))
            .filter_map(|e| crate::xmlx::val_int(e, "c:idx"))
            .filter_map(|i| u32::try_from(i).ok())
            .collect()
    }
}

/// Fill and outline (`c:spPr`). Parsed for rendering; the original element is written back while
/// nothing was edited, so effects and 3-D properties survive.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ShapeProps {
    /// The fill.
    pub fill: Option<Fill>,
    /// The outline.
    pub line: Option<Line>,
    /// The original element (cleared by any edit of `fill` or `line` through
    /// [`ShapeProps::set_fill`] / [`ShapeProps::set_line`]).
    pub raw: Option<XmlElement>,
}

impl ShapeProps {
    /// Whether nothing is specified.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fill.is_none() && self.line.is_none() && self.raw.is_none()
    }

    /// Replaces the fill (the element is regenerated on save).
    pub fn set_fill(&mut self, fill: Option<Fill>) {
        self.fill = fill;
        self.raw = None;
    }

    /// Replaces the outline.
    pub fn set_line(&mut self, line: Option<Line>) {
        self.line = line;
        self.raw = None;
    }
}

/// Run properties the model reads out of `a:defRPr` / `a:rPr`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RunProps {
    /// Size in hundredths of a point.
    pub size: Option<u32>,
    /// Bold.
    pub bold: Option<bool>,
    /// Italic.
    pub italic: Option<bool>,
    /// The text colour.
    pub color: Option<kiln_ooxml::drawingml::color::DrawingColor>,
    /// The Latin typeface.
    pub latin: Option<String>,
}

/// Text properties (`c:txPr`): the default run properties and the rotation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextProps {
    /// Default run properties of the first paragraph.
    pub run: RunProps,
    /// `a:bodyPr/@rot` in 60 000ths of a degree.
    pub rotation: Option<i64>,
    /// `a:bodyPr/@vert` (`horz`, `vert`, `vert270`, …).
    pub vertical: Option<String>,
    /// The original element, written back while nothing was edited.
    pub raw: Option<XmlElement>,
}

impl TextProps {
    /// Whether nothing is specified.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.raw.is_none() && self.run == RunProps::default() && self.rotation.is_none()
    }
}
