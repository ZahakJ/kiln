// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text in shapes: [`TextBody`] (kiln-doc paragraphs and runs plus DrawingML body properties),
//! list styles (`a:lstStyle`, the master's title/body/other styles) and the DrawingML paragraph
//! properties Word has no equivalent for.
//!
//! **Mapping onto kiln-doc.** A text body's paragraphs are `kiln_doc::Paragraph`s held in a
//! one-section `kiln_doc::Document`, so Write's text engine, edit builders and ops work on them
//! unchanged. DrawingML paragraph properties map as follows:
//!
//! | DrawingML (`a:pPr`) | Model |
//! |---|---|
//! | `lvl` (0–8) | `ParagraphProps::outline_level` |
//! | `algn` | `ParagraphProps::alignment` |
//! | `marL`, `indent` | `ParagraphProps::indent` (`left`; `first_line`/`hanging`), twips |
//! | `rtl` | `ParagraphProps::bidi` |
//! | `a:tabLst` | `ParagraphProps::tabs` |
//! | `a:lnSpc`, `a:spcBef`, `a:spcAft` | [`DeckParaProps`] spacing (percent or points, exact) |
//! | bullets (`a:bu*`), `defTabSz`, `fontAlgn` | [`DeckParaProps`] |
//!
//! Run properties (`a:rPr`) map onto `kiln_doc::RunProps` (see [`crate::dml`]).

use std::collections::BTreeMap;

use kiln_common::NodeId;
use kiln_doc::merge::{merge_paragraph_props, merge_run_props};
use kiln_doc::{
    Block, Document, Fragment, HeaderFooterRefs, Inlines, Paragraph, ParagraphProps, RunProps,
    Section, SectionProps, Transaction,
};
use kiln_ooxml::drawingml::color::DrawingColor;
use serde::{Deserialize, Serialize};

use crate::dml::serde_xml::xml;
use crate::error::{DeckError, Result};

// ------------------------------------------------------------------ body properties

/// Vertical anchoring of text in its box (`a:bodyPr/@anchor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextAnchor {
    /// `t`.
    #[default]
    Top,
    /// `ctr`.
    Middle,
    /// `b`.
    Bottom,
    /// `just`.
    Justified,
    /// `dist`.
    Distributed,
}

impl TextAnchor {
    /// The DrawingML spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Top => "t",
            Self::Middle => "ctr",
            Self::Bottom => "b",
            Self::Justified => "just",
            Self::Distributed => "dist",
        }
    }

    /// Parses the DrawingML spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "t" => Self::Top,
            "ctr" => Self::Middle,
            "b" => Self::Bottom,
            "just" => Self::Justified,
            "dist" => Self::Distributed,
            _ => return None,
        })
    }
}

/// How text and its box fit each other (`a:noAutofit`, `a:normAutofit`, `a:spAutoFit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Autofit {
    /// Do not autofit: text may overflow.
    #[default]
    Off,
    /// Shrink text on overflow (`a:normAutofit`); the scales are what the last layout computed,
    /// in 1000ths of a percent (100 000 = 100 %).
    Shrink {
        /// `fontScale`.
        font_scale: u32,
        /// `lnSpcReduction`.
        line_reduction: u32,
    },
    /// Resize the shape to fit the text (`a:spAutoFit`).
    ResizeShape,
}

impl Autofit {
    /// Shrink on overflow with no reduction computed yet.
    pub const SHRINK: Self = Self::Shrink {
        font_scale: 100_000,
        line_reduction: 0,
    };
}

/// Text wrapping inside the box (`a:bodyPr/@wrap`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextWrap {
    /// `square`: wrap at the box edges.
    #[default]
    Square,
    /// `none`: one line per paragraph.
    None,
}

/// Text direction (`a:bodyPr/@vert`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum TextDirection {
    /// `horz`.
    #[default]
    Horizontal,
    /// `vert`: rotated 90°.
    Rotate90,
    /// `vert270`: rotated 270°.
    Rotate270,
    /// `wordArtVert`: stacked.
    Stacked,
    /// `eaVert`.
    EastAsianVertical,
    /// `mongolianVert`.
    MongolianVertical,
    /// `wordArtVertRtl`.
    StackedRtl,
}

impl TextDirection {
    /// The DrawingML spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Horizontal => "horz",
            Self::Rotate90 => "vert",
            Self::Rotate270 => "vert270",
            Self::Stacked => "wordArtVert",
            Self::EastAsianVertical => "eaVert",
            Self::MongolianVertical => "mongolianVert",
            Self::StackedRtl => "wordArtVertRtl",
        }
    }

    /// Parses the DrawingML spelling.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "horz" => Self::Horizontal,
            "vert" => Self::Rotate90,
            "vert270" => Self::Rotate270,
            "wordArtVert" => Self::Stacked,
            "eaVert" => Self::EastAsianVertical,
            "mongolianVert" => Self::MongolianVertical,
            "wordArtVertRtl" => Self::StackedRtl,
            _ => return None,
        })
    }
}

/// Body properties (`a:bodyPr`). `None` inherits from the placeholder chain; [`BodyProps::resolve`]
/// fills in DrawingML's defaults.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct BodyProps {
    /// Insets `[left, top, right, bottom]` in EMU (`lIns`, `tIns`, `rIns`, `bIns`).
    pub insets: [Option<i64>; 4],
    /// Vertical anchoring.
    pub anchor: Option<TextAnchor>,
    /// `anchorCtr`: centre the text block horizontally.
    pub anchor_center: Option<bool>,
    /// Autofit.
    pub autofit: Option<Autofit>,
    /// `numCol`: text columns (1–16).
    pub columns: Option<u32>,
    /// `spcCol`: space between columns, EMU.
    pub column_spacing: Option<i64>,
    /// `rtlCol`: columns run right to left.
    pub rtl_columns: Option<bool>,
    /// Wrapping.
    pub wrap: Option<TextWrap>,
    /// Text direction.
    pub direction: Option<TextDirection>,
    /// `rot`: rotation of the text inside the shape, 60 000ths of a degree.
    pub rotation: Option<i32>,
    /// `upright`: keep text upright when the shape rotates.
    pub upright: Option<bool>,
    /// `spcFirstLastPara`: honour space before the first and after the last paragraph.
    pub space_first_last: Option<bool>,
    /// `a:prstTxWarp/@prst` (WordArt transform), kept for the writer and the renderer.
    pub warp: Option<String>,
}

/// Body properties with every value decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ResolvedBodyProps {
    /// Insets `[left, top, right, bottom]`, EMU.
    pub insets: [i64; 4],
    /// Vertical anchoring.
    pub anchor: TextAnchor,
    /// Centre the text block horizontally.
    pub anchor_center: bool,
    /// Autofit.
    pub autofit: Autofit,
    /// Columns (≥ 1).
    pub columns: u32,
    /// Space between columns, EMU.
    pub column_spacing: i64,
    /// Right-to-left columns.
    pub rtl_columns: bool,
    /// Wrapping.
    pub wrap: TextWrap,
    /// Direction.
    pub direction: TextDirection,
    /// Rotation of the text, 60 000ths of a degree.
    pub rotation: i32,
    /// Upright text.
    pub upright: bool,
}

/// DrawingML's default insets: 0.1" left and right, 0.05" top and bottom.
pub const DEFAULT_INSETS: [i64; 4] = [91_440, 45_720, 91_440, 45_720];

impl BodyProps {
    /// `self` over `base`: every set value of `self` wins.
    #[must_use]
    pub fn over(&self, base: &BodyProps) -> BodyProps {
        let mut insets = base.insets;
        for (o, s) in insets.iter_mut().zip(self.insets) {
            if s.is_some() {
                *o = s;
            }
        }
        BodyProps {
            insets,
            anchor: self.anchor.or(base.anchor),
            anchor_center: self.anchor_center.or(base.anchor_center),
            autofit: self.autofit.or(base.autofit),
            columns: self.columns.or(base.columns),
            column_spacing: self.column_spacing.or(base.column_spacing),
            rtl_columns: self.rtl_columns.or(base.rtl_columns),
            wrap: self.wrap.or(base.wrap),
            direction: self.direction.or(base.direction),
            rotation: self.rotation.or(base.rotation),
            upright: self.upright.or(base.upright),
            space_first_last: self.space_first_last.or(base.space_first_last),
            warp: self.warp.clone().or_else(|| base.warp.clone()),
        }
    }

    /// The values with DrawingML's defaults for anything unset.
    #[must_use]
    pub fn resolve(&self) -> ResolvedBodyProps {
        let mut insets = DEFAULT_INSETS;
        for (o, s) in insets.iter_mut().zip(self.insets) {
            if let Some(v) = s {
                *o = v.max(0);
            }
        }
        ResolvedBodyProps {
            insets,
            anchor: self.anchor.unwrap_or_default(),
            anchor_center: self.anchor_center.unwrap_or(false),
            autofit: self.autofit.unwrap_or_default(),
            columns: self.columns.unwrap_or(1).clamp(1, 16),
            column_spacing: self.column_spacing.unwrap_or(0).max(0),
            rtl_columns: self.rtl_columns.unwrap_or(false),
            wrap: self.wrap.unwrap_or_default(),
            direction: self.direction.unwrap_or_default(),
            rotation: self.rotation.unwrap_or(0),
            upright: self.upright.unwrap_or(false),
        }
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

// ------------------------------------------------------------------ paragraph extras

/// A bullet (`a:buNone`, `a:buChar`, `a:buAutoNum`, `a:buBlip`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Bullet {
    /// No bullet.
    None,
    /// A character bullet.
    Char(String),
    /// Automatic numbering: `ST_TextAutonumberScheme` (`arabicPeriod`, `romanUcPeriod`, …) and
    /// the first number.
    AutoNumber {
        /// The scheme, as written.
        scheme: String,
        /// `startAt` (≥ 1).
        start_at: u32,
    },
    /// A picture bullet: the media part key.
    Picture(String),
}

/// The colour of a bullet.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BulletColor {
    /// `a:buClrTx`: the colour of the first run.
    FollowText,
    /// `a:buClr`.
    Color(#[serde(with = "xml")] DrawingColor),
}

/// The size of a bullet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BulletSize {
    /// `a:buSzTx`.
    FollowText,
    /// `a:buSzPct`, 1000ths of a percent of the text size.
    Percent(i32),
    /// `a:buSzPts`, hundredths of a point.
    Points(i32),
}

/// The font of a bullet.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BulletFont {
    /// `a:buFontTx`.
    FollowText,
    /// `a:buFont/@typeface` (theme references such as `+mn-lt` allowed).
    Font(String),
}

/// Line or paragraph spacing (`a:spcPct` or `a:spcPts`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextSpacing {
    /// 1000ths of a percent of the line (100 000 = single).
    Percent(i32),
    /// Hundredths of a point.
    Points(i32),
}

/// Font alignment within a line (`a:pPr/@fontAlgn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FontAlign {
    /// `auto`.
    Auto,
    /// `t`.
    Top,
    /// `ctr`.
    Center,
    /// `base`.
    Baseline,
    /// `b`.
    Bottom,
}

/// DrawingML paragraph properties without a Word counterpart. `None` inherits.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct DeckParaProps {
    /// The bullet.
    pub bullet: Option<Bullet>,
    /// Bullet colour.
    pub bullet_color: Option<BulletColor>,
    /// Bullet size.
    pub bullet_size: Option<BulletSize>,
    /// Bullet font.
    pub bullet_font: Option<BulletFont>,
    /// `a:lnSpc`.
    pub line_spacing: Option<TextSpacing>,
    /// `a:spcBef`.
    pub space_before: Option<TextSpacing>,
    /// `a:spcAft`.
    pub space_after: Option<TextSpacing>,
    /// `defTabSz`, EMU.
    pub default_tab: Option<i64>,
    /// `fontAlgn`.
    pub font_align: Option<FontAlign>,
    /// `eaLnBrk`: East Asian line breaking.
    pub east_asian_line_break: Option<bool>,
    /// `hangingPunct`.
    pub hanging_punctuation: Option<bool>,
}

impl DeckParaProps {
    /// `self` over `base`.
    #[must_use]
    pub fn over(&self, base: &DeckParaProps) -> DeckParaProps {
        DeckParaProps {
            bullet: self.bullet.clone().or_else(|| base.bullet.clone()),
            bullet_color: self
                .bullet_color
                .clone()
                .or_else(|| base.bullet_color.clone()),
            bullet_size: self.bullet_size.or(base.bullet_size),
            bullet_font: self
                .bullet_font
                .clone()
                .or_else(|| base.bullet_font.clone()),
            line_spacing: self.line_spacing.or(base.line_spacing),
            space_before: self.space_before.or(base.space_before),
            space_after: self.space_after.or(base.space_after),
            default_tab: self.default_tab.or(base.default_tab),
            font_align: self.font_align.or(base.font_align),
            east_asian_line_break: self.east_asian_line_break.or(base.east_asian_line_break),
            hanging_punctuation: self.hanging_punctuation.or(base.hanging_punctuation),
        }
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

// ------------------------------------------------------------------ list styles

/// The defaults of one list level (`a:lvlNpPr` with its `a:defRPr`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LevelStyle {
    /// Paragraph properties (alignment, indents, tabs, direction).
    pub para: ParagraphProps,
    /// DrawingML-only paragraph properties (spacing, bullets).
    pub deck: DeckParaProps,
    /// Default run properties (`a:defRPr`).
    pub run: RunProps,
}

impl LevelStyle {
    /// `self` over `base`.
    #[must_use]
    pub fn over(&self, base: &LevelStyle) -> LevelStyle {
        LevelStyle {
            para: merge_paragraph_props(&base.para, Some(&self.para)),
            deck: self.deck.over(&base.deck),
            run: merge_run_props(&base.run, Some(&self.run)),
        }
    }

    /// True when nothing is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// Number of list levels.
pub const LEVELS: usize = 9;

/// A list style (`a:lstStyle`, `p:titleStyle`, `p:bodyStyle`, `p:otherStyle`,
/// `p:defaultTextStyle`): defaults per paragraph level. Only the levels that are set are stored
/// (on the heap: a style is a handful of bytes until it is used).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(from = "StoredListStyle", into = "StoredListStyle")]
pub struct ListStyle {
    default: Option<Box<LevelStyle>>,
    levels: BTreeMap<u8, LevelStyle>,
}

/// The stored form: levels outside 0–8 are dropped on load.
#[derive(Serialize, Deserialize)]
struct StoredListStyle {
    default: Option<Box<LevelStyle>>,
    levels: BTreeMap<u8, LevelStyle>,
}

impl From<StoredListStyle> for ListStyle {
    fn from(mut s: StoredListStyle) -> Self {
        s.levels.retain(|k, _| usize::from(*k) < LEVELS);
        Self {
            default: s.default,
            levels: s.levels,
        }
    }
}

impl From<ListStyle> for StoredListStyle {
    fn from(s: ListStyle) -> Self {
        Self {
            default: s.default,
            levels: s.levels,
        }
    }
}

impl ListStyle {
    /// The effective defaults of a level within this style alone (`defPPr` under the level).
    #[must_use]
    pub fn level(&self, level: usize) -> LevelStyle {
        match (self.default.as_deref(), self.get_level(level)) {
            (None, None) => LevelStyle::default(),
            (Some(d), None) => d.clone(),
            (None, Some(l)) => l.clone(),
            (Some(d), Some(l)) => l.over(d),
        }
    }

    /// `a:defPPr`, if set.
    #[must_use]
    pub fn default_level(&self) -> Option<&LevelStyle> {
        self.default.as_deref()
    }

    /// Sets or clears `a:defPPr`.
    pub fn set_default_level(&mut self, style: Option<LevelStyle>) {
        self.default = style.map(Box::new);
    }

    /// The entry of a level (0-based), if set.
    #[must_use]
    pub fn get_level(&self, level: usize) -> Option<&LevelStyle> {
        u8::try_from(level).ok().and_then(|l| self.levels.get(&l))
    }

    /// The set levels, in order, with their 0-based index.
    pub fn levels(&self) -> impl Iterator<Item = (usize, &LevelStyle)> {
        self.levels.iter().map(|(k, v)| (usize::from(*k), v))
    }

    /// True when no level is set.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.default.is_none() && self.levels.is_empty()
    }

    /// `self` over `base`, level by level.
    #[must_use]
    pub fn over(&self, base: &ListStyle) -> ListStyle {
        let pick = |own: Option<&LevelStyle>, b: Option<&LevelStyle>| match (own, b) {
            (Some(o), Some(b)) => Some(o.over(b)),
            (Some(o), None) => Some(o.clone()),
            (None, b) => b.cloned(),
        };
        let mut out = ListStyle::default();
        out.set_default_level(pick(self.default_level(), base.default_level()));
        for i in 0..LEVELS {
            if let Some(l) = pick(self.get_level(i), base.get_level(i)) {
                out.set_level(i, l);
            }
        }
        out
    }

    /// Sets a level (0-based; out-of-range levels are ignored).
    pub fn set_level(&mut self, level: usize, style: LevelStyle) {
        if level < LEVELS
            && let Ok(l) = u8::try_from(level)
        {
            self.levels.insert(l, style);
        }
    }

    /// Clears a level.
    pub fn clear_level(&mut self, level: usize) {
        if let Ok(l) = u8::try_from(level) {
            self.levels.remove(&l);
        }
    }
}

// ------------------------------------------------------------------ text body

/// Text in a shape, table cell or notes page (`p:txBody`/`a:txBody`).
///
/// The paragraphs live in a one-section `kiln_doc::Document` so kiln-doc's ops edit them;
/// [`TextBody::paragraphs`] reads them. A body always has at least one paragraph.
#[derive(Debug, Clone, PartialEq)]
pub struct TextBody {
    /// Body properties.
    pub body: BodyProps,
    /// The shape's own list style (`a:lstStyle`).
    pub list_style: ListStyle,
    doc: Box<Document>,
    para_props: BTreeMap<NodeId, DeckParaProps>,
}

impl Eq for TextBody {}

impl Default for TextBody {
    fn default() -> Self {
        Self::new()
    }
}

/// The stored form of a [`TextBody`]: only what is content.
#[derive(Serialize, Deserialize)]
struct StoredBody {
    body: BodyProps,
    list_style: ListStyle,
    section: NodeId,
    paragraphs: Vec<Paragraph>,
    para_props: BTreeMap<NodeId, DeckParaProps>,
}

impl Serialize for TextBody {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        StoredBody {
            body: self.body.clone(),
            list_style: self.list_style.clone(),
            section: self.section_id(),
            paragraphs: self.paragraphs().cloned().collect(),
            para_props: self.para_props.clone(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for TextBody {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = StoredBody::deserialize(d)?;
        let mut body =
            TextBody::build(s.section, s.paragraphs).map_err(serde::de::Error::custom)?;
        body.body = s.body;
        body.list_style = s.list_style;
        body.para_props = s.para_props;
        body.prune_para_props();
        Ok(body)
    }
}

fn section_of(id: NodeId, blocks: Vec<Block>) -> Section {
    Section {
        id,
        props: SectionProps::new(kiln_doc::Paper::for_locale("en-US")),
        headers: HeaderFooterRefs::default(),
        footers: HeaderFooterRefs::default(),
        blocks,
    }
}

impl TextBody {
    /// An empty body: one empty paragraph.
    #[must_use]
    pub fn new() -> Self {
        let mut doc = Document::empty();
        let para = Paragraph::new(doc.alloc_id());
        let section = doc.alloc_id();
        doc.sections
            .push(section_of(section, vec![Block::Paragraph(para)]));
        Self {
            body: BodyProps::default(),
            list_style: ListStyle::default(),
            doc: Box::new(doc),
            para_props: BTreeMap::new(),
        }
    }

    /// A body with one paragraph per line of `text` (`\n` separates paragraphs, `\u{b}` is a
    /// line break inside one), all without direct formatting.
    #[must_use]
    pub fn from_text(text: &str) -> Self {
        let mut doc = Document::empty();
        let mut blocks = Vec::new();
        for line in text.split('\n') {
            let mut p = Paragraph::new(doc.alloc_id());
            let mut frag = Fragment::default();
            for (i, part) in line.split('\u{b}').enumerate() {
                if i > 0 {
                    frag = concat(
                        frag,
                        Fragment::object(
                            kiln_doc::InlineObject::Break(kiln_doc::Break {
                                kind: kiln_doc::st::BreakKind::TextWrapping,
                                clear: None,
                            }),
                            RunProps::default(),
                        ),
                    );
                }
                frag = concat(frag, Fragment::text(part, RunProps::default()));
            }
            // A fragment built from text and breaks is always consistent.
            if let Ok(content) = Inlines::from_fragment(frag) {
                p.content = content;
            }
            blocks.push(Block::Paragraph(p));
        }
        let section = doc.alloc_id();
        doc.sections.push(section_of(section, blocks));
        Self {
            body: BodyProps::default(),
            list_style: ListStyle::default(),
            doc: Box::new(doc),
            para_props: BTreeMap::new(),
        }
    }

    /// A body from paragraphs (with their own ids, unique within the body) and DrawingML
    /// paragraph extras keyed by paragraph id. Readers use this.
    ///
    /// # Errors
    /// [`DeckError::Corrupt`] for no paragraphs, duplicate ids or invalid paragraph content.
    pub fn from_paragraphs(
        paragraphs: Vec<Paragraph>,
        para_props: BTreeMap<NodeId, DeckParaProps>,
    ) -> Result<Self> {
        let section = NodeId(
            paragraphs
                .iter()
                .map(|p| p.id.0)
                .max()
                .unwrap_or(0)
                .saturating_add(1)
                .min(NodeId::MAX.0),
        );
        let mut body = Self::build(section, paragraphs)?;
        body.para_props = para_props;
        body.prune_para_props();
        Ok(body)
    }

    fn build(section: NodeId, paragraphs: Vec<Paragraph>) -> Result<Self> {
        if paragraphs.is_empty() {
            return Err(DeckError::Corrupt("a text body needs a paragraph".into()));
        }
        let mut doc = Document::empty();
        doc.sections.push(section_of(
            section,
            paragraphs.into_iter().map(Block::Paragraph).collect(),
        ));
        doc.refresh_id_counter();
        doc.validate()
            .map_err(|e| DeckError::Corrupt(format!("text body: {e}")))?;
        Ok(Self {
            body: BodyProps::default(),
            list_style: ListStyle::default(),
            doc: Box::new(doc),
            para_props: BTreeMap::new(),
        })
    }

    fn section_id(&self) -> NodeId {
        self.doc.sections.first().map_or(NodeId(0), |s| s.id)
    }

    /// The paragraphs.
    pub fn paragraphs(&self) -> impl Iterator<Item = &Paragraph> {
        self.doc
            .sections
            .iter()
            .flat_map(|s| s.blocks.iter())
            .filter_map(Block::as_paragraph)
    }

    /// Number of paragraphs.
    #[must_use]
    pub fn paragraph_count(&self) -> usize {
        self.paragraphs().count()
    }

    /// The paragraph with an id.
    #[must_use]
    pub fn paragraph(&self, id: NodeId) -> Option<&Paragraph> {
        self.doc.paragraph(id)
    }

    /// The kiln-doc document holding the paragraphs (for kiln-doc's edit builders and
    /// resolution). Change it only through [`crate::Op::EditText`].
    #[must_use]
    pub fn doc(&self) -> &Document {
        &self.doc
    }

    /// The container id kiln-doc ops use to insert or remove paragraphs (`Op::InsertBlocks`).
    #[must_use]
    pub fn container_id(&self) -> NodeId {
        self.section_id()
    }

    /// The plain text, paragraphs separated by `\n`.
    #[must_use]
    pub fn text(&self) -> String {
        let mut out = String::new();
        for (i, p) in self.paragraphs().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(&p.text());
        }
        out
    }

    /// True when there is no text (an empty placeholder shows its prompt).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.paragraphs().all(|p| p.content.is_empty())
    }

    /// The list level of a paragraph (0–8).
    #[must_use]
    pub fn level_of(p: &Paragraph) -> usize {
        usize::from(p.props.outline_level.unwrap_or(0)).min(LEVELS - 1)
    }

    /// The DrawingML extras of a paragraph.
    #[must_use]
    pub fn deck_props(&self, para: NodeId) -> Option<&DeckParaProps> {
        self.para_props.get(&para)
    }

    /// Every paragraph's DrawingML extras.
    #[must_use]
    pub fn all_deck_props(&self) -> &BTreeMap<NodeId, DeckParaProps> {
        &self.para_props
    }

    /// Applies a kiln-doc transaction and returns its inverse. New paragraphs created by a split
    /// take the DrawingML extras of the paragraph they were split from (PowerPoint's Enter);
    /// extras of removed paragraphs are dropped. `para_props`, when given, replaces the extras
    /// after the transaction (inverses use this to restore them exactly).
    pub(crate) fn apply_text(
        &mut self,
        tx: Transaction,
        para_props: Option<BTreeMap<NodeId, DeckParaProps>>,
    ) -> Result<(Transaction, BTreeMap<NodeId, DeckParaProps>)> {
        let splits: Vec<(NodeId, NodeId)> = tx
            .ops
            .iter()
            .filter_map(|op| match op {
                kiln_doc::Op::SplitParagraph { para, new_para, .. } => Some((*para, *new_para)),
                _ => None,
            })
            .collect();
        let before = self.para_props.clone();
        let inverse = self.doc.apply(tx)?;
        if let Err(e) = self.check_doc() {
            // Undo what kiln-doc accepted but a text body cannot hold (tables, sections).
            let _ = self.doc.apply(inverse);
            return Err(e);
        }
        match para_props {
            Some(p) => self.para_props = p,
            None => {
                for (from, to) in splits {
                    if let Some(p) = self.para_props.get(&from).cloned() {
                        self.para_props.insert(to, p);
                    }
                }
            }
        }
        self.prune_para_props();
        Ok((inverse, before))
    }

    /// Replaces the extras of paragraphs (`None` clears them).
    pub(crate) fn set_deck_props(
        &mut self,
        para: NodeId,
        props: Option<DeckParaProps>,
    ) -> Result<Option<DeckParaProps>> {
        if self.doc.paragraph(para).is_none() {
            return Err(DeckError::UnknownNode(para));
        }
        Ok(match props {
            Some(p) if !p.is_empty() => self.para_props.insert(para, p),
            _ => self.para_props.remove(&para),
        })
    }

    fn check_doc(&self) -> Result<()> {
        if self.doc.sections.len() != 1 {
            return Err(DeckError::InvalidOp(
                "a text body has exactly one section".into(),
            ));
        }
        if self
            .doc
            .sections
            .iter()
            .flat_map(|s| &s.blocks)
            .any(|b| b.as_paragraph().is_none())
        {
            return Err(DeckError::InvalidOp(
                "a text body holds paragraphs only".into(),
            ));
        }
        Ok(())
    }

    fn prune_para_props(&mut self) {
        let doc = &self.doc;
        self.para_props
            .retain(|id, p| !p.is_empty() && doc.paragraph(*id).is_some());
    }

    /// Checks the invariants (one section of paragraphs, valid content).
    ///
    /// # Errors
    /// [`DeckError::Corrupt`] naming the problem.
    pub fn validate(&self) -> Result<()> {
        self.check_doc()
            .map_err(|e| DeckError::Corrupt(e.to_string()))?;
        self.doc
            .validate()
            .map_err(|e| DeckError::Corrupt(format!("text body: {e}")))?;
        if let Some(id) = self
            .para_props
            .keys()
            .find(|id| self.doc.paragraph(**id).is_none())
        {
            return Err(DeckError::Corrupt(format!(
                "paragraph extras for missing paragraph {id}"
            )));
        }
        Ok(())
    }
}

fn concat(mut a: Fragment, b: Fragment) -> Fragment {
    let base = a.text.chars().count();
    a.text.push_str(&b.text);
    a.runs.extend(b.runs);
    a.marks.extend(b.marks.into_iter().map(|mut m| {
        m.start += base;
        m.end += base;
        m
    }));
    a.objects.extend(b.objects);
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::Pos;
    use kiln_doc::edit::{insert_text, split_paragraph};

    #[test]
    fn bodies_hold_paragraphs() {
        let b = TextBody::new();
        assert_eq!(b.paragraph_count(), 1);
        assert!(b.is_empty());
        b.validate().unwrap();
        let t = TextBody::from_text("Hello\nWorld\u{b}again");
        assert_eq!(t.paragraph_count(), 2);
        assert_eq!(t.text(), "Hello\nWorld\nagain");
        assert!(!t.is_empty());
        t.validate().unwrap();
    }

    #[test]
    fn bodies_round_trip_through_postcard() {
        let mut t = TextBody::from_text("One\nTwo");
        t.body.anchor = Some(TextAnchor::Bottom);
        t.body.autofit = Some(Autofit::SHRINK);
        t.list_style.set_level(
            1,
            LevelStyle {
                deck: DeckParaProps {
                    bullet: Some(Bullet::Char("–".into())),
                    bullet_color: Some(BulletColor::Color(DrawingColor::scheme("accent1"))),
                    ..DeckParaProps::default()
                },
                ..LevelStyle::default()
            },
        );
        let first = t.paragraphs().next().unwrap().id;
        t.set_deck_props(
            first,
            Some(DeckParaProps {
                space_before: Some(TextSpacing::Points(600)),
                ..DeckParaProps::default()
            }),
        )
        .unwrap();
        let bytes = postcard::to_allocvec(&t).unwrap();
        let back: TextBody = postcard::from_bytes(&bytes).unwrap();
        assert_eq!(back, t);
        back.validate().unwrap();
    }

    #[test]
    fn splitting_a_paragraph_carries_its_extras_and_undo_restores_them() {
        let mut t = TextBody::from_text("Hello");
        let p = t.paragraphs().next().unwrap().id;
        let extras = DeckParaProps {
            bullet: Some(Bullet::None),
            ..DeckParaProps::default()
        };
        t.set_deck_props(p, Some(extras.clone())).unwrap();
        let before = t.clone();
        let mut doc = t.doc().clone();
        let tx = split_paragraph(&mut doc, Pos::new(p, 2)).unwrap();
        let (inverse, old) = t.apply_text(tx, None).unwrap();
        assert_eq!(t.paragraph_count(), 2);
        let second = t.paragraphs().nth(1).unwrap().id;
        assert_eq!(t.deck_props(second), Some(&extras));
        t.apply_text(inverse, Some(old)).unwrap();
        assert_eq!(t, before);
    }

    #[test]
    fn text_bodies_refuse_tables() {
        let mut t = TextBody::new();
        let mut doc = t.doc().clone();
        let tx = kiln_doc::edit::insert_table(
            &mut doc,
            Pos::new(t.paragraphs().next().unwrap().id, 0),
            2,
            2,
        );
        if let Ok(tx) = tx {
            let before = t.clone();
            assert!(t.apply_text(tx, None).is_err());
            assert_eq!(t, before);
        }
        let mut doc = t.doc().clone();
        let p = t.paragraphs().next().unwrap().id;
        let tx = insert_text(&mut doc, Pos::new(p, 0), "ok").unwrap();
        t.apply_text(tx, None).unwrap();
        assert_eq!(t.text(), "ok");
    }

    #[test]
    fn list_styles_merge_default_under_levels() {
        let mut s = ListStyle::default();
        assert!(s.is_empty());
        s.set_default_level(Some(LevelStyle {
            run: RunProps {
                size: Some(36),
                bold: Some(true),
                ..RunProps::default()
            },
            ..LevelStyle::default()
        }));
        s.set_level(
            0,
            LevelStyle {
                run: RunProps {
                    size: Some(48),
                    ..RunProps::default()
                },
                ..LevelStyle::default()
            },
        );
        s.set_level(99, LevelStyle::default());
        let l0 = s.level(0);
        assert_eq!(l0.run.size, Some(48));
        assert_eq!(l0.run.bold, Some(true));
        assert_eq!(s.level(3).run.size, Some(36));
    }

    #[test]
    fn body_props_inherit_and_resolve() {
        let base = BodyProps {
            insets: [Some(0), None, Some(0), None],
            anchor: Some(TextAnchor::Bottom),
            columns: Some(2),
            ..BodyProps::default()
        };
        let own = BodyProps {
            insets: [None, Some(10), None, None],
            anchor: Some(TextAnchor::Middle),
            ..BodyProps::default()
        };
        let r = own.over(&base).resolve();
        assert_eq!(r.insets, [0, 10, 0, 45_720]);
        assert_eq!(r.anchor, TextAnchor::Middle);
        assert_eq!(r.columns, 2);
        assert_eq!(BodyProps::default().resolve().insets, DEFAULT_INSETS);
        assert!(BodyProps::default().is_empty());
        assert_eq!(
            BodyProps {
                columns: Some(99),
                ..BodyProps::default()
            }
            .resolve()
            .columns,
            16
        );
        for a in [
            TextAnchor::Top,
            TextAnchor::Middle,
            TextAnchor::Bottom,
            TextAnchor::Justified,
            TextAnchor::Distributed,
        ] {
            assert_eq!(TextAnchor::parse(a.as_str()), Some(a));
        }
        for d in [
            TextDirection::Horizontal,
            TextDirection::Rotate90,
            TextDirection::Rotate270,
            TextDirection::Stacked,
            TextDirection::EastAsianVertical,
            TextDirection::MongolianVertical,
            TextDirection::StackedRtl,
        ] {
            assert_eq!(TextDirection::parse(d.as_str()), Some(d));
        }
        assert_eq!(TextAnchor::parse("x"), None);
        assert_eq!(TextDirection::parse("x"), None);
    }
}
