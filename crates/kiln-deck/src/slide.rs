// SPDX-License-Identifier: AGPL-3.0-or-later
//! Masters, layouts, slides, notes, comments, sections, backgrounds, colour maps and slide size.

use kiln_common::NodeId;
use kiln_ooxml::drawingml::color::{ColorMap, DrawingColor};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::theme::Theme;
use kiln_ooxml::st::ColorSchemeIndex;
use serde::{Deserialize, Serialize};

use crate::animation::Timeline;
use crate::dml::serde_xml::{xml, xml_opt};
use crate::shape::Shape;
use crate::text::{ListStyle, TextBody};
use crate::transition::Transition;

// ------------------------------------------------------------------ slide size

/// The kind of slide size (`p:sldSz/@type`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum SlideSizeKind {
    /// 16:9 widescreen, 13.333" × 7.5" (PowerPoint 2013+ default).
    #[default]
    Widescreen,
    /// 4:3, 10" × 7.5" (`screen4x3`).
    Standard,
    /// 16:10 (`screen16x10`).
    Screen16x10,
    /// A4 paper (`A4`).
    A4,
    /// Letter paper (`letter`).
    Letter,
    /// Any other size (`custom`).
    Custom,
    /// A type Kiln does not name, kept as written.
    Other(String),
}

impl SlideSizeKind {
    /// The `type` attribute (`None` for widescreen, which PowerPoint writes without one).
    #[must_use]
    pub fn as_ooxml(&self) -> Option<&str> {
        match self {
            Self::Widescreen => None,
            Self::Standard => Some("screen4x3"),
            Self::Screen16x10 => Some("screen16x10"),
            Self::A4 => Some("A4"),
            Self::Letter => Some("letter"),
            Self::Custom => Some("custom"),
            Self::Other(s) => Some(s),
        }
    }

    /// Parses the `type` attribute.
    #[must_use]
    pub fn from_ooxml(s: Option<&str>) -> Self {
        match s {
            None => Self::Widescreen,
            Some("screen4x3") => Self::Standard,
            Some("screen16x10") => Self::Screen16x10,
            Some("A4") => Self::A4,
            Some("letter") => Self::Letter,
            Some("custom") => Self::Custom,
            Some(o) => Self::Other(o.to_owned()),
        }
    }
}

/// The slide size in EMU.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlideSize {
    /// Width.
    pub cx: i64,
    /// Height.
    pub cy: i64,
    /// What PowerPoint calls it.
    pub kind: SlideSizeKind,
}

impl Default for SlideSize {
    fn default() -> Self {
        Self::WIDESCREEN
    }
}

impl SlideSize {
    /// 16:9, 12 192 000 × 6 858 000 EMU.
    pub const WIDESCREEN: SlideSize = SlideSize {
        cx: 12_192_000,
        cy: 6_858_000,
        kind: SlideSizeKind::Widescreen,
    };
    /// 4:3, 9 144 000 × 6 858 000 EMU.
    pub const STANDARD: SlideSize = SlideSize {
        cx: 9_144_000,
        cy: 6_858_000,
        kind: SlideSizeKind::Standard,
    };

    /// The largest side PowerPoint allows (56 inches).
    pub const MAX_SIDE: i64 = 51_206_400;

    /// True when both sides are within PowerPoint's range (1 EMU to 56 inches).
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.cx > 0 && self.cy > 0 && self.cx <= Self::MAX_SIDE && self.cy <= Self::MAX_SIDE
    }

    /// A custom size.
    #[must_use]
    pub const fn custom(cx: i64, cy: i64) -> Self {
        Self {
            cx,
            cy,
            kind: SlideSizeKind::Custom,
        }
    }

    /// The notes page size PowerPoint uses (7.5" × 10" portrait).
    pub const NOTES: SlideSize = SlideSize {
        cx: 6_858_000,
        cy: 9_144_000,
        kind: SlideSizeKind::Custom,
    };
}

// ------------------------------------------------------------------ colour map

/// A colour scheme slot (`ST_ColorSchemeIndex`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SchemeSlot {
    /// `dk1`.
    Dk1,
    /// `lt1`.
    Lt1,
    /// `dk2`.
    Dk2,
    /// `lt2`.
    Lt2,
    /// `accent1`.
    Accent1,
    /// `accent2`.
    Accent2,
    /// `accent3`.
    Accent3,
    /// `accent4`.
    Accent4,
    /// `accent5`.
    Accent5,
    /// `accent6`.
    Accent6,
    /// `hlink`.
    Hlink,
    /// `folHlink`.
    FolHlink,
}

impl SchemeSlot {
    const ALL: [SchemeSlot; 12] = [
        Self::Dk1,
        Self::Lt1,
        Self::Dk2,
        Self::Lt2,
        Self::Accent1,
        Self::Accent2,
        Self::Accent3,
        Self::Accent4,
        Self::Accent5,
        Self::Accent6,
        Self::Hlink,
        Self::FolHlink,
    ];

    /// kiln-ooxml's index.
    #[must_use]
    pub fn to_ooxml(self) -> ColorSchemeIndex {
        ColorSchemeIndex::ALL[self as usize]
    }

    /// From kiln-ooxml's index.
    #[must_use]
    pub fn from_ooxml(i: ColorSchemeIndex) -> Self {
        Self::ALL[i.index()]
    }
}

/// The colour map (`p:clrMap`): which scheme slots the text/background aliases and accents use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ColorMapping {
    /// `bg1`.
    pub bg1: SchemeSlot,
    /// `tx1`.
    pub tx1: SchemeSlot,
    /// `bg2`.
    pub bg2: SchemeSlot,
    /// `tx2`.
    pub tx2: SchemeSlot,
    /// `accent1`–`accent6`.
    pub accents: [SchemeSlot; 6],
    /// `hlink`.
    pub hlink: SchemeSlot,
    /// `folHlink`.
    pub fol_hlink: SchemeSlot,
}

impl Default for ColorMapping {
    /// The standard light mapping: dark text on a light background.
    fn default() -> Self {
        Self::from_ooxml(&ColorMap::default())
    }
}

impl ColorMapping {
    /// Light text on a dark background (`bg1 = dk1`, `tx1 = lt1`), for dark masters.
    #[must_use]
    pub fn dark() -> Self {
        Self {
            bg1: SchemeSlot::Dk1,
            tx1: SchemeSlot::Lt1,
            bg2: SchemeSlot::Dk2,
            tx2: SchemeSlot::Lt2,
            ..Self::default()
        }
    }

    /// kiln-ooxml's map.
    #[must_use]
    pub fn to_ooxml(&self) -> ColorMap {
        let a = self.accents.map(SchemeSlot::to_ooxml);
        ColorMap {
            bg1: self.bg1.to_ooxml(),
            tx1: self.tx1.to_ooxml(),
            bg2: self.bg2.to_ooxml(),
            tx2: self.tx2.to_ooxml(),
            accents: a,
            hlink: self.hlink.to_ooxml(),
            fol_hlink: self.fol_hlink.to_ooxml(),
        }
    }

    /// From kiln-ooxml's map.
    #[must_use]
    pub fn from_ooxml(m: &ColorMap) -> Self {
        Self {
            bg1: SchemeSlot::from_ooxml(m.bg1),
            tx1: SchemeSlot::from_ooxml(m.tx1),
            bg2: SchemeSlot::from_ooxml(m.bg2),
            tx2: SchemeSlot::from_ooxml(m.tx2),
            accents: m.accents.map(SchemeSlot::from_ooxml),
            hlink: SchemeSlot::from_ooxml(m.hlink),
            fol_hlink: SchemeSlot::from_ooxml(m.fol_hlink),
        }
    }
}

// ------------------------------------------------------------------ backgrounds

/// A background (`p:bg`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Background {
    /// `p:bgPr`: a fill.
    Fill {
        /// The fill.
        #[serde(with = "xml")]
        fill: Fill,
        /// `shadeToTitle`.
        shade_to_title: bool,
    },
    /// `p:bgRef`: a background fill style of the theme (`idx` 1001–1003) with a colour.
    Theme {
        /// The style index.
        idx: u32,
        /// The colour substituted for `phClr`.
        #[serde(with = "xml_opt")]
        color: Option<DrawingColor>,
    },
}

impl Background {
    /// A solid background in a theme colour.
    #[must_use]
    pub fn solid(color: DrawingColor) -> Self {
        Self::Fill {
            fill: Fill::Solid(color),
            shade_to_title: false,
        }
    }
}

// ------------------------------------------------------------------ layouts

/// The standard layouts (`ST_SlideLayoutType`) plus custom ones.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum LayoutKind {
    /// Title Slide (`title`).
    Title,
    /// Title and Content (`obj`).
    #[default]
    TitleContent,
    /// Section Header (`secHead`).
    SectionHeader,
    /// Two Content (`twoObj`).
    TwoContent,
    /// Comparison (`twoTxTwoObj`).
    Comparison,
    /// Title Only (`titleOnly`).
    TitleOnly,
    /// Blank (`blank`).
    Blank,
    /// Content with Caption (`objTx`).
    ContentCaption,
    /// Picture with Caption (`picTx`).
    PictureCaption,
    /// Title and Vertical Text (`vertTx`).
    TitleVerticalText,
    /// Vertical Title and Text (`vertTitleAndTx`).
    VerticalTitleText,
    /// A custom layout (`cust`).
    Custom,
    /// Another `ST_SlideLayoutType` value, kept as written.
    Other(String),
}

impl LayoutKind {
    /// The eleven standard layouts in PowerPoint's gallery order.
    pub const STANDARD: [LayoutKind; 11] = [
        Self::Title,
        Self::TitleContent,
        Self::SectionHeader,
        Self::TwoContent,
        Self::Comparison,
        Self::TitleOnly,
        Self::Blank,
        Self::ContentCaption,
        Self::PictureCaption,
        Self::TitleVerticalText,
        Self::VerticalTitleText,
    ];

    /// The `type` attribute.
    #[must_use]
    pub fn as_ooxml(&self) -> &str {
        match self {
            Self::Title => "title",
            Self::TitleContent => "obj",
            Self::SectionHeader => "secHead",
            Self::TwoContent => "twoObj",
            Self::Comparison => "twoTxTwoObj",
            Self::TitleOnly => "titleOnly",
            Self::Blank => "blank",
            Self::ContentCaption => "objTx",
            Self::PictureCaption => "picTx",
            Self::TitleVerticalText => "vertTx",
            Self::VerticalTitleText => "vertTitleAndTx",
            Self::Custom => "cust",
            Self::Other(s) => s,
        }
    }

    /// Parses the `type` attribute (`cust` when absent).
    #[must_use]
    pub fn from_ooxml(s: Option<&str>) -> Self {
        let Some(s) = s else { return Self::Custom };
        Self::STANDARD
            .into_iter()
            .chain([Self::Custom])
            .find(|k| k.as_ooxml() == s)
            .unwrap_or_else(|| Self::Other(s.to_owned()))
    }

    /// PowerPoint's English name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        match self {
            Self::Title => "Title Slide",
            Self::TitleContent => "Title and Content",
            Self::SectionHeader => "Section Header",
            Self::TwoContent => "Two Content",
            Self::Comparison => "Comparison",
            Self::TitleOnly => "Title Only",
            Self::Blank => "Blank",
            Self::ContentCaption => "Content with Caption",
            Self::PictureCaption => "Picture with Caption",
            Self::TitleVerticalText => "Title and Vertical Text",
            Self::VerticalTitleText => "Vertical Title and Text",
            Self::Custom => "Custom Layout",
            Self::Other(s) => s,
        }
    }
}

/// A slide layout (`p:sldLayout`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    /// Stable id.
    pub id: NodeId,
    /// `p:cSld/@name`.
    pub name: String,
    /// `type`.
    pub kind: LayoutKind,
    /// Shapes (placeholders and decoration).
    pub shapes: Vec<Shape>,
    /// Own background (`None` uses the master's).
    pub background: Option<Background>,
    /// `p:clrMapOvr` (`None` uses the master's map).
    pub color_map: Option<ColorMapping>,
    /// `showMasterSp`: draw the master's non-placeholder shapes.
    pub show_master_shapes: bool,
    /// `preserve`: keep the layout when no slide uses it.
    pub preserve: bool,
}

impl Layout {
    /// An empty layout.
    #[must_use]
    pub fn new(id: NodeId, kind: LayoutKind) -> Self {
        Self {
            id,
            name: kind.display_name().to_owned(),
            kind,
            shapes: Vec::new(),
            background: None,
            color_map: None,
            show_master_shapes: true,
            preserve: true,
        }
    }
}

/// Which footer-area placeholders the master shows (`p:hf`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct HeaderFooter {
    /// `sldNum`.
    pub slide_number: bool,
    /// `hdr`.
    pub header: bool,
    /// `ftr`.
    pub footer: bool,
    /// `dt`.
    pub date: bool,
}

impl Default for HeaderFooter {
    /// Everything on (the schema default).
    fn default() -> Self {
        Self {
            slide_number: true,
            header: true,
            footer: true,
            date: true,
        }
    }
}

/// A slide master (`p:sldMaster`) with its theme and layouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Master {
    /// Stable id.
    pub id: NodeId,
    /// The master's name (`p:cSld/@name`, usually the theme's name).
    pub name: String,
    /// Its theme.
    #[serde(with = "xml")]
    pub theme: Theme,
    /// `p:clrMap`.
    pub color_map: ColorMapping,
    /// Background.
    pub background: Background,
    /// Shapes (placeholders and decoration).
    pub shapes: Vec<Shape>,
    /// `p:titleStyle`.
    pub title_style: ListStyle,
    /// `p:bodyStyle`.
    pub body_style: ListStyle,
    /// `p:otherStyle`.
    pub other_style: ListStyle,
    /// The layouts, in gallery order.
    pub layouts: Vec<Layout>,
    /// `p:hf`.
    pub header_footer: HeaderFooter,
    /// `preserve`.
    pub preserve: bool,
}

impl Master {
    /// A layout by id.
    #[must_use]
    pub fn layout(&self, id: NodeId) -> Option<&Layout> {
        self.layouts.iter().find(|l| l.id == id)
    }

    /// The first layout of a kind.
    #[must_use]
    pub fn layout_of_kind(&self, kind: &LayoutKind) -> Option<&Layout> {
        self.layouts.iter().find(|l| &l.kind == kind)
    }
}

/// The notes master (`p:notesMaster`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotesMaster {
    /// Stable id.
    pub id: NodeId,
    /// Shapes (slide image, notes body, header, date, footer, number placeholders).
    pub shapes: Vec<Shape>,
    /// Background.
    pub background: Background,
    /// `p:notesStyle`.
    pub notes_style: ListStyle,
    /// `p:clrMap`.
    pub color_map: ColorMapping,
}

// ------------------------------------------------------------------ slides

/// Speaker notes (the notes page's body text; other shapes on the notes page are kept).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Notes {
    /// The notes text.
    pub text: TextBody,
    /// Other shapes of the notes page (custom notes pages; kept).
    pub shapes: Vec<Shape>,
}

/// A reply in a comment thread.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CommentReply {
    /// Stable id.
    pub id: NodeId,
    /// Author's display name.
    pub author: String,
    /// When (ISO 8601).
    pub created: String,
    /// The text.
    pub text: String,
}

/// A comment thread on a slide.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Comment {
    /// Stable id.
    pub id: NodeId,
    /// Author's display name.
    pub author: String,
    /// Author's initials.
    pub initials: String,
    /// When (ISO 8601).
    pub created: String,
    /// The text.
    pub text: String,
    /// The shape it is attached to (`None`: the slide).
    pub shape: Option<NodeId>,
    /// Position on the slide, EMU.
    pub pos: (i64, i64),
    /// Replies, oldest first.
    pub replies: Vec<CommentReply>,
    /// Resolved.
    pub resolved: bool,
}

/// A slide (`p:sld`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Slide {
    /// Stable id.
    pub id: NodeId,
    /// Its layout.
    pub layout: NodeId,
    /// `p:cSld/@name`.
    pub name: Option<String>,
    /// Shapes, back to front.
    pub shapes: Vec<Shape>,
    /// Own background (`None`: the layout's or master's).
    pub background: Option<Background>,
    /// `p:clrMapOvr`.
    pub color_map: Option<ColorMapping>,
    /// `showMasterSp`.
    pub show_master_shapes: bool,
    /// `show="0"`: skipped in the slide show.
    pub hidden: bool,
    /// Transition into this slide.
    pub transition: Option<Transition>,
    /// Animations.
    pub timeline: Timeline,
    /// Speaker notes.
    pub notes: Option<Notes>,
    /// Comments.
    pub comments: Vec<Comment>,
}

impl Slide {
    /// An empty slide on a layout.
    #[must_use]
    pub fn new(id: NodeId, layout: NodeId) -> Self {
        Self {
            id,
            layout,
            name: None,
            shapes: Vec::new(),
            background: None,
            color_map: None,
            show_master_shapes: true,
            hidden: false,
            transition: None,
            timeline: Timeline::default(),
            notes: None,
            comments: Vec::new(),
        }
    }

    /// The notes text, or an empty string.
    #[must_use]
    pub fn notes_text(&self) -> String {
        self.notes
            .as_ref()
            .map(|n| n.text.text())
            .unwrap_or_default()
    }
}

/// A section of the slide list: a name and the number of consecutive slides it holds.
/// Sections partition the slides in order (`p14:sectionLst`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Section {
    /// Stable id.
    pub id: NodeId,
    /// The name.
    pub name: String,
    /// How many slides, starting after the previous section's.
    pub len: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_kinds_round_trip() {
        for k in LayoutKind::STANDARD {
            assert_eq!(LayoutKind::from_ooxml(Some(k.as_ooxml())), k);
            assert!(!k.display_name().is_empty());
        }
        assert_eq!(LayoutKind::from_ooxml(None), LayoutKind::Custom);
        assert_eq!(
            LayoutKind::from_ooxml(Some("chart")),
            LayoutKind::Other("chart".into())
        );
        assert_eq!(LayoutKind::Other("chart".into()).as_ooxml(), "chart");
    }

    #[test]
    fn slide_size_kinds_round_trip() {
        for k in [
            SlideSizeKind::Widescreen,
            SlideSizeKind::Standard,
            SlideSizeKind::Screen16x10,
            SlideSizeKind::A4,
            SlideSizeKind::Letter,
            SlideSizeKind::Custom,
            SlideSizeKind::Other("B4ISO".into()),
        ] {
            assert_eq!(SlideSizeKind::from_ooxml(k.as_ooxml()), k);
        }
        assert_eq!(SlideSize::default(), SlideSize::WIDESCREEN);
        assert_eq!(SlideSize::custom(1, 2).kind, SlideSizeKind::Custom);
    }

    #[test]
    fn colour_maps_convert() {
        let light = ColorMapping::default();
        assert_eq!(light.bg1, SchemeSlot::Lt1);
        assert_eq!(light.tx1, SchemeSlot::Dk1);
        assert_eq!(ColorMapping::from_ooxml(&light.to_ooxml()), light);
        let dark = ColorMapping::dark();
        assert_eq!(dark.bg1, SchemeSlot::Dk1);
        assert_eq!(ColorMapping::from_ooxml(&dark.to_ooxml()), dark);
    }

    #[test]
    fn slides_start_empty() {
        let s = Slide::new(NodeId(2), NodeId(1));
        assert_eq!(s.notes_text(), "");
        assert!(s.show_master_shapes && !s.hidden);
        let l = Layout::new(NodeId(3), LayoutKind::Comparison);
        assert_eq!(l.name, "Comparison");
        assert!(HeaderFooter::default().slide_number);
    }
}
