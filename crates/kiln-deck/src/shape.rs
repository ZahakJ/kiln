// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shapes on slides, layouts and masters (`p:spTree` children).
//!
//! §3.4's shape kinds map as follows: a **text box** is a [`ShapeKind::Geometry`] with
//! `text_box: true`; a **placeholder** is any shape whose [`Shape::placeholder`] is set (usually
//! a geometry shape, but filled picture, table and chart placeholders keep their kind); the
//! other kinds are their own variants.

use kiln_common::NodeId;
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::effects::EffectList;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::geometry::GeometryDef;
use kiln_ooxml::drawingml::line::Line;
use serde::{Deserialize, Serialize};

use crate::dml::serde_xml::{xml, xml_opt};
use crate::geom::{ChildSpace, Rect, Transform};
use crate::text::TextBody;

// ------------------------------------------------------------------ placeholders

/// `ST_PlaceholderType`.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default, Serialize, Deserialize,
)]
pub enum PlaceholderType {
    /// `title`.
    Title,
    /// `body`.
    Body,
    /// `ctrTitle`.
    CenteredTitle,
    /// `subTitle`.
    Subtitle,
    /// `dt`.
    Date,
    /// `sldNum`.
    SlideNumber,
    /// `ftr`.
    Footer,
    /// `hdr`.
    Header,
    /// `obj` (the default when `type` is omitted).
    #[default]
    Object,
    /// `chart`.
    Chart,
    /// `tbl`.
    Table,
    /// `clipArt`.
    ClipArt,
    /// `dgm`.
    Diagram,
    /// `media`.
    Media,
    /// `sldImg`.
    SlideImage,
    /// `pic`.
    Picture,
}

impl PlaceholderType {
    /// Every type.
    pub const ALL: [PlaceholderType; 16] = [
        Self::Title,
        Self::Body,
        Self::CenteredTitle,
        Self::Subtitle,
        Self::Date,
        Self::SlideNumber,
        Self::Footer,
        Self::Header,
        Self::Object,
        Self::Chart,
        Self::Table,
        Self::ClipArt,
        Self::Diagram,
        Self::Media,
        Self::SlideImage,
        Self::Picture,
    ];

    /// The DrawingML spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Title => "title",
            Self::Body => "body",
            Self::CenteredTitle => "ctrTitle",
            Self::Subtitle => "subTitle",
            Self::Date => "dt",
            Self::SlideNumber => "sldNum",
            Self::Footer => "ftr",
            Self::Header => "hdr",
            Self::Object => "obj",
            Self::Chart => "chart",
            Self::Table => "tbl",
            Self::ClipArt => "clipArt",
            Self::Diagram => "dgm",
            Self::Media => "media",
            Self::SlideImage => "sldImg",
            Self::Picture => "pic",
        }
    }

    /// Parses the DrawingML spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.as_str() == s)
    }

    /// True for the title types.
    #[must_use]
    pub const fn is_title(self) -> bool {
        matches!(self, Self::Title | Self::CenteredTitle)
    }

    /// True for the footer-area types (date, slide number, footer, header), which new slides
    /// only get when header and footer settings turn them on.
    #[must_use]
    pub const fn is_footer_area(self) -> bool {
        matches!(
            self,
            Self::Date | Self::SlideNumber | Self::Footer | Self::Header
        )
    }

    /// The type a master placeholder must have for this layout placeholder to inherit from it
    /// (PowerPoint: titles from the title, footer-area types from themselves, everything else
    /// from the body).
    #[must_use]
    pub const fn master_type(self) -> PlaceholderType {
        match self {
            Self::Title | Self::CenteredTitle => Self::Title,
            Self::Date => Self::Date,
            Self::SlideNumber => Self::SlideNumber,
            Self::Footer => Self::Footer,
            Self::Header => Self::Header,
            Self::SlideImage => Self::SlideImage,
            _ => Self::Body,
        }
    }

    /// True when the placeholder holds text (and so gets a text body when created).
    #[must_use]
    pub const fn holds_text(self) -> bool {
        !matches!(self, Self::SlideImage)
    }
}

/// `p:ph`'s size hint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum PlaceholderSize {
    /// `full`.
    #[default]
    Full,
    /// `half`.
    Half,
    /// `quarter`.
    Quarter,
}

/// A placeholder reference (`p:nvPr/p:ph`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Placeholder {
    /// `type` (`obj` when omitted).
    pub kind: PlaceholderType,
    /// `idx` (0 when omitted).
    pub idx: u32,
    /// `orient="vert"`.
    pub vertical: bool,
    /// `sz`.
    pub size: PlaceholderSize,
    /// `hasCustomPrompt`.
    pub custom_prompt: bool,
}

impl Placeholder {
    /// A placeholder of a type with an index.
    #[must_use]
    pub const fn new(kind: PlaceholderType, idx: u32) -> Self {
        Self {
            kind,
            idx,
            vertical: false,
            size: PlaceholderSize::Full,
            custom_prompt: false,
        }
    }
}

// ------------------------------------------------------------------ formatting

/// A shape's geometry (`a:prstGeom` or `a:custGeom`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Geometry {
    /// A preset (`rect`, `roundRect`, `ellipse`, … — kiln-ooxml evaluates all 187).
    Preset {
        /// `prst`.
        name: String,
        /// Adjust values that differ from the preset's (`a:avLst`, `val n` formulas).
        adjust: Vec<(String, i64)>,
    },
    /// A custom path.
    Custom(#[serde(with = "xml")] GeometryDef),
}

impl Geometry {
    /// A preset without adjustments.
    #[must_use]
    pub fn preset(name: &str) -> Self {
        Self::Preset {
            name: name.to_owned(),
            adjust: Vec::new(),
        }
    }

    /// The preset name, if a preset.
    #[must_use]
    pub fn preset_name(&self) -> Option<&str> {
        match self {
            Self::Preset { name, .. } => Some(name),
            Self::Custom(_) => None,
        }
    }
}

/// A style-matrix reference (`a:lnRef`, `a:fillRef`, `a:effectRef`): an index into the theme's
/// format scheme and the colour substituted for `phClr`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StyleRef {
    /// `idx` (0 = none, 1–3 = subtle/moderate/intense, 1001–1003 = background fills).
    pub idx: u32,
    /// The colour.
    #[serde(with = "xml_opt")]
    pub color: Option<DrawingColor>,
}

/// Which theme font a shape's text uses (`a:fontRef/@idx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontCollectionRef {
    /// `major`: headings.
    Major,
    /// `minor`: body.
    Minor,
    /// `none`.
    None,
}

/// `p:style`: references into the theme for shapes drawn with a Quick Style.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShapeStyle {
    /// `a:lnRef`.
    pub line: StyleRef,
    /// `a:fillRef`.
    pub fill: StyleRef,
    /// `a:effectRef`.
    pub effect: StyleRef,
    /// `a:fontRef`: the collection and the text colour.
    pub font: FontCollectionRef,
    /// `a:fontRef`'s colour.
    #[serde(with = "xml_opt")]
    pub font_color: Option<DrawingColor>,
}

impl ShapeStyle {
    /// PowerPoint's default shape style: accent 1 fill, a darker accent 1 outline, white minor
    /// text.
    #[must_use]
    pub fn default_shape() -> Self {
        Self {
            line: StyleRef {
                idx: 2,
                color: Some(DrawingColor::scheme("accent1").with("shade", 15_000)),
            },
            fill: StyleRef {
                idx: 1,
                color: Some(DrawingColor::scheme("accent1")),
            },
            effect: StyleRef {
                idx: 0,
                color: Some(DrawingColor::scheme("accent1")),
            },
            font: FontCollectionRef::Minor,
            font_color: Some(DrawingColor::scheme("lt1")),
        }
    }

    /// PowerPoint's default connector style: an accent 1 line, no fill.
    #[must_use]
    pub fn default_line() -> Self {
        Self {
            line: StyleRef {
                idx: 1,
                color: Some(DrawingColor::scheme("accent1")),
            },
            fill: StyleRef {
                idx: 0,
                color: Some(DrawingColor::scheme("accent1")),
            },
            effect: StyleRef {
                idx: 0,
                color: Some(DrawingColor::scheme("accent1")),
            },
            font: FontCollectionRef::Minor,
            font_color: Some(DrawingColor::scheme("tx1")),
        }
    }
}

/// Shape properties (`p:spPr` plus `p:style`). `None` inherits from the placeholder chain (or
/// means "not set" outside placeholders).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ShapeProps {
    /// Geometry.
    pub geometry: Option<Geometry>,
    /// Fill.
    #[serde(with = "xml_opt")]
    pub fill: Option<Fill>,
    /// Outline.
    #[serde(with = "xml_opt")]
    pub line: Option<Line>,
    /// Effects.
    #[serde(with = "xml_opt")]
    pub effects: Option<EffectList>,
    /// Theme style references.
    pub style: Option<ShapeStyle>,
}

impl ShapeProps {
    /// `self` over `base`, property by property.
    #[must_use]
    pub fn over(&self, base: &ShapeProps) -> ShapeProps {
        ShapeProps {
            geometry: self.geometry.clone().or_else(|| base.geometry.clone()),
            fill: self.fill.clone().or_else(|| base.fill.clone()),
            line: self.line.clone().or_else(|| base.line.clone()),
            effects: self.effects.clone().or_else(|| base.effects.clone()),
            style: self.style.clone().or_else(|| base.style.clone()),
        }
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

// ------------------------------------------------------------------ kinds

/// A shape with geometry and optional text (`p:sp`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AutoShape {
    /// `txBox="1"`: a text box (PowerPoint's Insert ▸ Text Box).
    pub text_box: bool,
    /// Geometry, fill, outline and effects.
    pub props: ShapeProps,
    /// The text.
    pub text: Option<TextBody>,
}

/// Picture cropping (`a:srcRect`): insets from each edge of the image in 1000ths of a percent of
/// its size; negative values pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct Crop {
    /// Left.
    pub l: i32,
    /// Top.
    pub t: i32,
    /// Right.
    pub r: i32,
    /// Bottom.
    pub b: i32,
}

impl Crop {
    /// True when nothing is cropped.
    #[must_use]
    pub const fn is_none(&self) -> bool {
        self.l == 0 && self.t == 0 && self.r == 0 && self.b == 0
    }
}

/// A picture (`p:pic`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Picture {
    /// The image: a key into [`crate::Presentation::parts`].
    pub media: String,
    /// Cropping.
    pub crop: Crop,
    /// Geometry (the crop shape), outline and effects.
    pub props: ShapeProps,
    /// `a:blip/a:alphaModFix/@amt`: opacity in 1000ths of a percent, when not opaque.
    pub alpha: Option<u32>,
}

/// Table style options (`a:tblPr` flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct TableLook {
    /// `firstRow`: header row.
    pub first_row: bool,
    /// `firstCol`.
    pub first_col: bool,
    /// `lastRow`: total row.
    pub last_row: bool,
    /// `lastCol`.
    pub last_col: bool,
    /// `bandRow`.
    pub band_row: bool,
    /// `bandCol`.
    pub band_col: bool,
}

/// Cell formatting (`a:tcPr`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CellProps {
    /// Fill.
    #[serde(with = "xml_opt")]
    pub fill: Option<Fill>,
    /// Borders: left, right, top, bottom (`a:lnL`, `a:lnR`, `a:lnT`, `a:lnB`).
    pub borders: [CellBorder; 4],
    /// Margins `[left, top, right, bottom]`, EMU (`marL`, `marT`, `marR`, `marB`).
    pub margins: [Option<i64>; 4],
    /// Vertical anchoring.
    pub anchor: Option<crate::text::TextAnchor>,
    /// Text direction.
    pub direction: Option<crate::text::TextDirection>,
}

/// One cell border.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CellBorder(#[serde(with = "xml_opt")] pub Option<Line>);

/// A table cell (`a:tc`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TableCell {
    /// The text.
    pub text: TextBody,
    /// `gridSpan` (≥ 1).
    pub grid_span: u32,
    /// `rowSpan` (≥ 1).
    pub row_span: u32,
    /// `hMerge`: covered by the cell to the left.
    pub h_merge: bool,
    /// `vMerge`: covered by the cell above.
    pub v_merge: bool,
    /// Formatting.
    pub props: CellProps,
}

impl TableCell {
    /// An empty cell.
    #[must_use]
    pub fn new() -> Self {
        Self {
            grid_span: 1,
            row_span: 1,
            ..Self::default()
        }
    }
}

/// A table row (`a:tr`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TableRow {
    /// Height, EMU (a minimum: rows grow with their text).
    pub height: i64,
    /// One cell per grid column.
    pub cells: Vec<TableCell>,
}

/// A table (`p:graphicFrame` with `a:tbl`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Table {
    /// Column widths, EMU (`a:tblGrid`).
    pub grid: Vec<i64>,
    /// Rows.
    pub rows: Vec<TableRow>,
    /// `a:tableStyleId` (a GUID of the built-in table styles).
    pub style_id: Option<String>,
    /// Style options.
    pub look: TableLook,
    /// `rtl`.
    pub rtl: bool,
}

/// The most rows Insert ▸ Table makes (PowerPoint's limit).
pub const MAX_TABLE_ROWS: usize = 75;
/// The most columns Insert ▸ Table makes (PowerPoint's limit).
pub const MAX_TABLE_COLS: usize = 75;

impl Table {
    /// A table of empty cells with even column widths and row heights filling a box, with a
    /// header row and banded rows (PowerPoint's Insert ▸ Table). Rows and columns are clamped
    /// to 1 ..= [`MAX_TABLE_ROWS`] and 1 ..= [`MAX_TABLE_COLS`].
    #[must_use]
    pub fn with_size(rows: usize, cols: usize, width: i64, height: i64) -> Self {
        let rows = rows.clamp(1, MAX_TABLE_ROWS);
        let cols = cols.clamp(1, MAX_TABLE_COLS);
        let w = width / i64::try_from(cols).unwrap_or(i64::MAX).max(1);
        let h = height / i64::try_from(rows).unwrap_or(i64::MAX).max(1);
        Self {
            grid: vec![w; cols],
            rows: (0..rows)
                .map(|_| TableRow {
                    height: h,
                    cells: (0..cols).map(|_| TableCell::new()).collect(),
                })
                .collect(),
            // "Medium Style 2 - Accent 1", PowerPoint's default.
            style_id: Some("{5C22544A-7EE6-4342-B048-85BDC9FD1C3A}".to_owned()),
            look: TableLook {
                first_row: true,
                band_row: true,
                ..TableLook::default()
            },
            rtl: false,
        }
    }

    /// The cell at a row and grid column.
    #[must_use]
    pub fn cell(&self, row: usize, col: usize) -> Option<&TableCell> {
        self.rows.get(row)?.cells.get(col)
    }

    /// Mutable cell.
    pub fn cell_mut(&mut self, row: usize, col: usize) -> Option<&mut TableCell> {
        self.rows.get_mut(row)?.cells.get_mut(col)
    }
}

/// A chart (`p:graphicFrame` with `c:chart`): a handle to the chart part, which kiln-charts
/// reads and renders.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ChartRef {
    /// A key into [`crate::Presentation::parts`] (the chart part; its embedded workbook is a
    /// sibling part).
    pub part: String,
}

/// Where a connector end attaches (`a:stCxn`, `a:endCxn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Connection {
    /// The shape.
    pub shape: NodeId,
    /// The connection site index of its geometry.
    pub site: u32,
}

/// A connector (`p:cxnSp`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Connector {
    /// Geometry (`straightConnector1`, `bentConnector3`, `curvedConnector3`, …), line and
    /// effects.
    pub props: ShapeProps,
    /// Start attachment.
    pub start: Option<Connection>,
    /// End attachment.
    pub end: Option<Connection>,
}

/// Audio or video.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum MediaKind {
    /// Video.
    #[default]
    Video,
    /// Audio.
    Audio,
}

/// Where media comes from.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaSource {
    /// Embedded: a key into [`crate::Presentation::parts`].
    Embedded(String),
    /// Linked: a URL or path.
    Linked(String),
}

impl Default for MediaSource {
    fn default() -> Self {
        Self::Embedded(String::new())
    }
}

/// A video or audio clip (`p:pic` with `a:videoFile`/`a:audioFile`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Media {
    /// Video or audio.
    pub kind: MediaKind,
    /// The clip.
    pub source: MediaSource,
    /// The poster frame image (a part key).
    pub poster: Option<String>,
    /// Trim from the start, milliseconds.
    pub trim_start_ms: u32,
    /// Trim from the end, milliseconds.
    pub trim_end_ms: u32,
    /// Outline, effects, geometry.
    pub props: ShapeProps,
}

/// A group (`p:grpSp`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Group {
    /// The children's coordinate space.
    pub child_space: ChildSpace,
    /// The children, back to front.
    pub children: Vec<Shape>,
    /// Group fill (used by children with `a:grpFill`) and effects.
    pub props: ShapeProps,
}

/// What a shape is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ShapeKind {
    /// A shape with geometry and text, including text boxes and text placeholders.
    Geometry(AutoShape),
    /// A picture.
    Picture(Picture),
    /// A group.
    Group(Group),
    /// A table.
    Table(Table),
    /// A chart.
    Chart(ChartRef),
    /// A connector.
    Connector(Connector),
    /// Audio or video.
    Media(Media),
}

impl ShapeKind {
    /// A short name for messages and the default shape names.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Geometry(_) => "shape",
            Self::Picture(_) => "picture",
            Self::Group(_) => "group",
            Self::Table(_) => "table",
            Self::Chart(_) => "chart",
            Self::Connector(_) => "connector",
            Self::Media(_) => "media",
        }
    }
}

/// A shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Shape {
    /// Stable id, unique in the presentation.
    pub id: NodeId,
    /// `p:cNvPr/@name` (shown in the Selection Pane).
    pub name: String,
    /// `descr`: alternative text.
    pub alt_text: Option<String>,
    /// `title`.
    pub title: Option<String>,
    /// `hidden`.
    pub hidden: bool,
    /// The placeholder this shape is (or fills).
    pub placeholder: Option<Placeholder>,
    /// Position and size; `None` inherits from the placeholder chain.
    pub xfrm: Option<Transform>,
    /// What the shape is.
    pub kind: ShapeKind,
    /// `a:hlinkClick/@r:id`'s target (a URL, or `#slide:<id>` for a slide link).
    pub link: Option<String>,
}

impl Shape {
    /// A shape with a name and no placeholder.
    #[must_use]
    pub fn new(
        id: NodeId,
        name: impl Into<String>,
        xfrm: Option<Transform>,
        kind: ShapeKind,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            alt_text: None,
            title: None,
            hidden: false,
            placeholder: None,
            xfrm,
            kind,
            link: None,
        }
    }

    /// A text box: a rectangle without fill or outline, text that resizes the shape (PowerPoint's
    /// Insert ▸ Text Box).
    #[must_use]
    pub fn text_box(id: NodeId, rect: Rect, text: &str) -> Self {
        let mut body = TextBody::from_text(text);
        body.body.wrap = Some(crate::text::TextWrap::Square);
        body.body.autofit = Some(crate::text::Autofit::ResizeShape);
        Self::new(
            id,
            format!("TextBox {}", id.0),
            Some(Transform::from_rect(rect)),
            ShapeKind::Geometry(AutoShape {
                text_box: true,
                props: ShapeProps {
                    geometry: Some(Geometry::preset("rect")),
                    fill: Some(Fill::None),
                    ..ShapeProps::default()
                },
                text: Some(body),
            }),
        )
    }

    /// A preset shape with the theme's default shape style and centred text (PowerPoint's
    /// Insert ▸ Shapes).
    #[must_use]
    pub fn geometry(id: NodeId, preset: &str, rect: Rect) -> Self {
        let mut body = TextBody::new();
        body.body.anchor = Some(crate::text::TextAnchor::Middle);
        let name = preset_display_name(preset);
        Self::new(
            id,
            format!("{name} {}", id.0),
            Some(Transform::from_rect(rect)),
            ShapeKind::Geometry(AutoShape {
                text_box: false,
                props: ShapeProps {
                    geometry: Some(Geometry::preset(preset)),
                    style: Some(ShapeStyle::default_shape()),
                    ..ShapeProps::default()
                },
                text: Some(body),
            }),
        )
    }

    /// The text body, if the shape has one (geometry shapes only; table cells are separate).
    #[must_use]
    pub fn text(&self) -> Option<&TextBody> {
        match &self.kind {
            ShapeKind::Geometry(s) => s.text.as_ref(),
            _ => None,
        }
    }

    /// Mutable text body.
    pub fn text_mut(&mut self) -> Option<&mut TextBody> {
        match &mut self.kind {
            ShapeKind::Geometry(s) => s.text.as_mut(),
            _ => None,
        }
    }

    /// The shape properties (`None` for tables and charts, which have none of their own).
    #[must_use]
    pub fn props(&self) -> Option<&ShapeProps> {
        match &self.kind {
            ShapeKind::Geometry(s) => Some(&s.props),
            ShapeKind::Picture(p) => Some(&p.props),
            ShapeKind::Group(g) => Some(&g.props),
            ShapeKind::Connector(c) => Some(&c.props),
            ShapeKind::Media(m) => Some(&m.props),
            ShapeKind::Table(_) | ShapeKind::Chart(_) => None,
        }
    }

    /// Mutable shape properties.
    pub fn props_mut(&mut self) -> Option<&mut ShapeProps> {
        match &mut self.kind {
            ShapeKind::Geometry(s) => Some(&mut s.props),
            ShapeKind::Picture(p) => Some(&mut p.props),
            ShapeKind::Group(g) => Some(&mut g.props),
            ShapeKind::Connector(c) => Some(&mut c.props),
            ShapeKind::Media(m) => Some(&mut m.props),
            ShapeKind::Table(_) | ShapeKind::Chart(_) => None,
        }
    }

    /// True for a text box.
    #[must_use]
    pub fn is_text_box(&self) -> bool {
        matches!(&self.kind, ShapeKind::Geometry(s) if s.text_box)
    }

    /// The children of a group.
    #[must_use]
    pub fn children(&self) -> Option<&Vec<Shape>> {
        match &self.kind {
            ShapeKind::Group(g) => Some(&g.children),
            _ => None,
        }
    }

    /// Mutable children of a group.
    pub fn children_mut(&mut self) -> Option<&mut Vec<Shape>> {
        match &mut self.kind {
            ShapeKind::Group(g) => Some(&mut g.children),
            _ => None,
        }
    }

    /// This shape and every descendant, depth first.
    pub fn walk(&self, f: &mut impl FnMut(&Shape)) {
        f(self);
        if let Some(children) = self.children() {
            for c in children {
                c.walk(f);
            }
        }
    }

    /// Every text body in the shape and its descendants (shape text and table cells).
    pub fn walk_text(&self, f: &mut impl FnMut(&TextBody)) {
        self.walk(&mut |s| match &s.kind {
            ShapeKind::Geometry(a) => {
                if let Some(t) = &a.text {
                    f(t);
                }
            }
            ShapeKind::Table(t) => {
                for row in &t.rows {
                    for c in &row.cells {
                        f(&c.text);
                    }
                }
            }
            _ => {}
        });
    }
}

/// The name PowerPoint gives a new shape of a preset ("Rectangle 4", "Oval 5").
#[must_use]
pub fn preset_display_name(preset: &str) -> &'static str {
    match preset {
        "rect" => "Rectangle",
        "roundRect" => "Rectangle: Rounded Corners",
        "ellipse" => "Oval",
        "triangle" => "Isosceles Triangle",
        "rtTriangle" => "Right Triangle",
        "line" | "straightConnector1" => "Straight Connector",
        "rightArrow" => "Arrow: Right",
        "leftArrow" => "Arrow: Left",
        "upArrow" => "Arrow: Up",
        "downArrow" => "Arrow: Down",
        "star5" => "Star: 5 Points",
        "hexagon" => "Hexagon",
        "pentagon" | "homePlate" => "Arrow: Pentagon",
        "chevron" => "Arrow: Chevron",
        "diamond" => "Diamond",
        "parallelogram" => "Parallelogram",
        "trapezoid" => "Trapezoid",
        "octagon" => "Octagon",
        "cloud" => "Cloud",
        "heart" => "Heart",
        "wedgeRectCallout" | "wedgeRoundRectCallout" | "wedgeEllipseCallout" => "Speech Bubble",
        _ => "Freeform",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_types_round_trip_their_names() {
        for t in PlaceholderType::ALL {
            assert_eq!(PlaceholderType::parse(t.as_str()), Some(t));
        }
        assert_eq!(PlaceholderType::parse("nope"), None);
        assert_eq!(
            PlaceholderType::Subtitle.master_type(),
            PlaceholderType::Body
        );
        assert_eq!(
            PlaceholderType::CenteredTitle.master_type(),
            PlaceholderType::Title
        );
        assert_eq!(
            PlaceholderType::Footer.master_type(),
            PlaceholderType::Footer
        );
        assert!(PlaceholderType::Date.is_footer_area());
        assert!(!PlaceholderType::SlideImage.holds_text());
    }

    #[test]
    fn constructors_name_shapes_like_powerpoint() {
        let r = Rect::new(0, 0, 100, 100);
        let t = Shape::text_box(NodeId(3), r, "Hi");
        assert_eq!(t.name, "TextBox 3");
        assert!(t.is_text_box());
        assert_eq!(t.text().unwrap().text(), "Hi");
        let g = Shape::geometry(NodeId(4), "ellipse", r);
        assert_eq!(g.name, "Oval 4");
        assert!(!g.is_text_box());
        assert_eq!(
            g.props().unwrap().geometry.as_ref().unwrap().preset_name(),
            Some("ellipse")
        );
        assert_eq!(preset_display_name("whatever"), "Freeform");
    }

    #[test]
    fn walking_visits_groups_and_tables() {
        let r = Rect::new(0, 0, 10, 10);
        let mut table = Table::with_size(2, 3, 300, 200);
        table.cell_mut(1, 2).unwrap().text = TextBody::from_text("x");
        assert_eq!(table.grid, vec![100; 3]);
        let group = Shape::new(
            NodeId(1),
            "Group 1",
            Some(Transform::from_rect(r)),
            ShapeKind::Group(Group {
                child_space: ChildSpace::identity(r),
                children: vec![
                    Shape::text_box(NodeId(2), r, "a"),
                    Shape::new(
                        NodeId(5),
                        "Table 5",
                        Some(Transform::from_rect(r)),
                        ShapeKind::Table(table),
                    ),
                ],
                props: ShapeProps::default(),
            }),
        );
        let mut ids = Vec::new();
        group.walk(&mut |s| ids.push(s.id.0));
        assert_eq!(ids, vec![1, 2, 5]);
        let mut texts = Vec::new();
        group.walk_text(&mut |t| texts.push(t.text()));
        assert_eq!(texts.len(), 7);
        assert!(texts.contains(&"x".to_owned()));
    }

    #[test]
    fn shape_props_inherit_property_by_property() {
        let base = ShapeProps {
            geometry: Some(Geometry::preset("rect")),
            fill: Some(Fill::None),
            ..ShapeProps::default()
        };
        let own = ShapeProps {
            fill: Some(Fill::Solid(DrawingColor::scheme("accent2"))),
            ..ShapeProps::default()
        };
        let r = own.over(&base);
        assert_eq!(r.geometry, Some(Geometry::preset("rect")));
        assert_eq!(r.fill, own.fill);
        assert!(ShapeProps::default().is_empty());
    }
}
