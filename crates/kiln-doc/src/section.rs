// SPDX-License-Identifier: AGPL-3.0-or-later
//! Section properties (`w:sectPr`): page size and orientation, margins, columns, page numbering,
//! line numbering and break types, plus Word's header/footer selection and inheritance rules.

use serde::{Deserialize, Serialize};

use crate::st::{
    HeaderFooterKind, LineNumberRestart, NumberFormat, Orientation, PageVAlign, SectionBreak,
};
use kiln_common::NodeId;

/// Page size (`w:pgSz`), twips.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageSize {
    /// Width.
    pub width: i32,
    /// Height.
    pub height: i32,
    /// `w:orient`.
    pub orientation: Orientation,
    /// Printer paper code (`w:code`), preserved.
    pub code: Option<u32>,
}

/// Page margins (`w:pgMar`), twips. A negative top or bottom margin means "exactly", text never
/// moves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct PageMargins {
    /// Top.
    pub top: i32,
    /// Right.
    pub right: i32,
    /// Bottom.
    pub bottom: i32,
    /// Left.
    pub left: i32,
    /// Header distance from the page edge.
    pub header: i32,
    /// Footer distance from the page edge.
    pub footer: i32,
    /// Gutter.
    pub gutter: i32,
}

/// One column of unequal columns (`w:col`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Column {
    /// Width, twips.
    pub width: i32,
    /// Space after, twips.
    pub space: i32,
}

/// Columns (`w:cols`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Columns {
    /// Number of columns.
    pub count: u32,
    /// Space between equal columns, twips.
    pub space: i32,
    /// Equal widths.
    pub equal_width: bool,
    /// A line between columns.
    pub separator: bool,
    /// Individual columns when not equal.
    pub cols: Vec<Column>,
}

impl Default for Columns {
    fn default() -> Self {
        Self {
            count: 1,
            space: 720,
            equal_width: true,
            separator: false,
            cols: Vec::new(),
        }
    }
}

/// Page numbering (`w:pgNumType`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct PageNumbering {
    /// `w:start`; `None` continues from the previous section.
    pub start: Option<i64>,
    /// `w:fmt`.
    pub format: Option<NumberFormat>,
}

/// The document grid (`w:docGrid`), preserved.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct DocGrid {
    /// `w:type`.
    pub kind: Option<String>,
    /// `w:linePitch`, twips.
    pub line_pitch: Option<i32>,
    /// `w:charSpace`.
    pub char_space: Option<i32>,
}

/// Line numbering (`w:lnNumType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LineNumbering {
    /// Number every n-th line.
    pub count_by: u32,
    /// Starting number.
    pub start: Option<u32>,
    /// Distance from the text, twips.
    pub distance: Option<i32>,
    /// When numbering restarts.
    pub restart: LineNumberRestart,
}

/// Section properties (`w:sectPr`).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SectionProps {
    /// `w:type`: how the section starts.
    pub break_type: SectionBreak,
    /// `w:pgSz`.
    pub page_size: PageSize,
    /// `w:pgMar`.
    pub margins: PageMargins,
    /// `w:cols`.
    pub columns: Columns,
    /// `w:titlePg`: different first-page header and footer.
    pub title_page: bool,
    /// `w:pgNumType`.
    pub page_numbering: Option<PageNumbering>,
    /// `w:vAlign`.
    pub v_align: Option<PageVAlign>,
    /// `w:bidi`.
    pub bidi: Option<bool>,
    /// `w:docGrid`.
    pub doc_grid: Option<DocGrid>,
    /// `w:lnNumType`.
    pub line_numbering: Option<LineNumbering>,
}

/// Paper sizes in twips (portrait).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Paper {
    /// US Letter, 8.5 × 11 in.
    Letter,
    /// US Legal, 8.5 × 14 in.
    Legal,
    /// Executive, 7.25 × 10.5 in.
    Executive,
    /// Tabloid, 11 × 17 in.
    Tabloid,
    /// A3.
    A3,
    /// A4.
    A4,
    /// A5.
    A5,
    /// B5 (JIS).
    B5,
}

impl Paper {
    /// Every paper.
    pub const ALL: [Paper; 8] = [
        Self::Letter,
        Self::Legal,
        Self::Executive,
        Self::Tabloid,
        Self::A3,
        Self::A4,
        Self::A5,
        Self::B5,
    ];

    /// (width, height, printer code) in twips, portrait.
    #[must_use]
    pub const fn dimensions(self) -> (i32, i32, u32) {
        match self {
            Self::Letter => (12240, 15840, 1),
            Self::Legal => (12240, 20160, 5),
            Self::Executive => (10440, 15120, 7),
            Self::Tabloid => (15840, 24480, 3),
            Self::A3 => (16838, 23811, 8),
            Self::A4 => (11906, 16838, 9),
            Self::A5 => (8391, 11906, 11),
            Self::B5 => (10319, 14572, 13),
        }
    }

    /// The name Word shows.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Letter => "Letter",
            Self::Legal => "Legal",
            Self::Executive => "Executive",
            Self::Tabloid => "Tabloid",
            Self::A3 => "A3",
            Self::A4 => "A4",
            Self::A5 => "A5",
            Self::B5 => "B5 (JIS)",
        }
    }

    /// The default paper for a BCP 47 locale: Letter for the US, Canada, Mexico, the Philippines,
    /// Chile, Colombia, Venezuela, Puerto Rico and Central America; A4 everywhere else.
    #[must_use]
    pub fn for_locale(locale: &str) -> Self {
        const LETTER_REGIONS: [&str; 14] = [
            "US", "CA", "MX", "PH", "CL", "CO", "VE", "PR", "GT", "CR", "PA", "SV", "DO", "NI",
        ];
        let region = locale
            .split(['-', '_'])
            .skip(1)
            .find(|p| p.len() == 2 && p.chars().all(|c| c.is_ascii_alphabetic()))
            .map(str::to_ascii_uppercase);
        match region {
            Some(r) if LETTER_REGIONS.contains(&r.as_str()) => Self::Letter,
            Some(_) => Self::A4,
            None if locale.eq_ignore_ascii_case("en") => Self::Letter,
            None => Self::A4,
        }
    }
}

/// Word's "Normal" margins: 1 inch all round, header and footer at 0.5 inch.
pub const NORMAL_MARGINS: PageMargins = PageMargins {
    top: 1440,
    right: 1440,
    bottom: 1440,
    left: 1440,
    header: 720,
    footer: 720,
    gutter: 0,
};

/// Word's Layout ▸ Margins presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MarginPreset {
    /// 1 inch.
    Normal,
    /// 0.5 inch.
    Narrow,
    /// 1 inch top/bottom, 0.75 inch left/right.
    Moderate,
    /// 1 inch top/bottom, 2 inch left/right.
    Wide,
}

impl MarginPreset {
    /// The margins.
    #[must_use]
    pub const fn margins(self) -> PageMargins {
        let (top, side) = match self {
            Self::Normal => (1440, 1440),
            Self::Narrow => (720, 720),
            Self::Moderate => (1440, 1080),
            Self::Wide => (1440, 2880),
        };
        PageMargins {
            top,
            right: side,
            bottom: top,
            left: side,
            ..NORMAL_MARGINS
        }
    }
}

impl SectionProps {
    /// A new Word document's section: the paper, Normal margins, one column.
    #[must_use]
    pub fn new(paper: Paper) -> Self {
        let (width, height, _) = paper.dimensions();
        Self {
            break_type: SectionBreak::NextPage,
            page_size: PageSize {
                width,
                height,
                orientation: Orientation::Portrait,
                code: None,
            },
            margins: NORMAL_MARGINS,
            columns: Columns::default(),
            title_page: false,
            page_numbering: None,
            v_align: None,
            bidi: None,
            doc_grid: Some(DocGrid {
                line_pitch: Some(360),
                ..DocGrid::default()
            }),
            line_numbering: None,
        }
    }

    /// The same section in another orientation, swapping width and height as Word does.
    #[must_use]
    pub fn with_orientation(&self, orientation: Orientation) -> Self {
        let mut out = self.clone();
        if self.page_size.orientation == orientation {
            return out;
        }
        let (w, h) = (self.page_size.width, self.page_size.height);
        let landscape = orientation == Orientation::Landscape;
        out.page_size.orientation = orientation;
        out.page_size.width = if landscape { w.max(h) } else { w.min(h) };
        out.page_size.height = if landscape { w.min(h) } else { w.max(h) };
        out
    }

    /// The text width: page width minus left and right margins and gutter (twips).
    #[must_use]
    pub fn content_width(&self) -> i32 {
        self.page_size
            .width
            .saturating_sub(self.margins.left)
            .saturating_sub(self.margins.right)
            .saturating_sub(self.margins.gutter)
    }

    /// The body text height (twips).
    #[must_use]
    pub fn content_height(&self) -> i32 {
        self.page_size
            .height
            .saturating_sub(self.margins.top.saturating_abs())
            .saturating_sub(self.margins.bottom.saturating_abs())
    }

    /// Column widths (twips), equal or explicit.
    #[must_use]
    pub fn column_widths(&self) -> Vec<i32> {
        let c = &self.columns;
        if !c.equal_width && !c.cols.is_empty() {
            return c.cols.iter().map(|col| col.width).collect();
        }
        let n = c.count.clamp(1, 45);
        let gaps = i64::from(c.space) * i64::from(n - 1);
        let each = (i64::from(self.content_width()) - gaps) / i64::from(n);
        let each = i32::try_from(each).unwrap_or(0);
        vec![each; n as usize]
    }

    /// The named paper this page matches (either orientation, ±10 twips).
    #[must_use]
    pub fn paper(&self) -> Option<Paper> {
        let w = self.page_size.width.min(self.page_size.height);
        let h = self.page_size.width.max(self.page_size.height);
        Paper::ALL.into_iter().find(|p| {
            let (pw, ph, _) = p.dimensions();
            (pw - w).abs() <= 10 && (ph - h).abs() <= 10
        })
    }

    /// Which header/footer a page uses: `first` on the first page of a section with a different
    /// first page, `even` on even pages when odd/even headers are on, `default` otherwise.
    #[must_use]
    pub fn header_kind_for_page(
        &self,
        even_and_odd_headers: bool,
        is_first_of_section: bool,
        page_number: i64,
    ) -> HeaderFooterKind {
        if self.title_page && is_first_of_section {
            HeaderFooterKind::First
        } else if even_and_odd_headers && page_number % 2 == 0 {
            HeaderFooterKind::Even
        } else {
            HeaderFooterKind::Default
        }
    }
}

/// How a section break of a type starts the next section.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BreakBehaviour {
    /// Starts a new page.
    pub new_page: bool,
    /// The page must be even (`Some(true)`) or odd (`Some(false)`), inserting a blank page.
    pub even: Option<bool>,
}

/// The behaviour of a section break type.
#[must_use]
pub const fn break_behaviour(kind: SectionBreak) -> BreakBehaviour {
    match kind {
        SectionBreak::Continuous | SectionBreak::NextColumn => BreakBehaviour {
            new_page: false,
            even: None,
        },
        SectionBreak::EvenPage => BreakBehaviour {
            new_page: true,
            even: Some(true),
        },
        SectionBreak::OddPage => BreakBehaviour {
            new_page: true,
            even: Some(false),
        },
        SectionBreak::NextPage => BreakBehaviour {
            new_page: true,
            even: None,
        },
    }
}

/// The headers (or footers) a section references, by kind. `None` links to the previous
/// section's ("Link to Previous").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct HeaderFooterRefs {
    /// Odd (or all) pages.
    pub default: Option<NodeId>,
    /// The first page when `title_page` is on.
    pub first: Option<NodeId>,
    /// Even pages when odd/even headers are on.
    pub even: Option<NodeId>,
}

impl HeaderFooterRefs {
    /// The reference for a kind.
    #[must_use]
    pub const fn get(&self, kind: HeaderFooterKind) -> Option<NodeId> {
        match kind {
            HeaderFooterKind::Default => self.default,
            HeaderFooterKind::First => self.first,
            HeaderFooterKind::Even => self.even,
        }
    }

    /// Sets the reference for a kind.
    pub fn set(&mut self, kind: HeaderFooterKind, id: Option<NodeId>) {
        match kind {
            HeaderFooterKind::Default => self.default = id,
            HeaderFooterKind::First => self.first = id,
            HeaderFooterKind::Even => self.even = id,
        }
    }

    /// Every set reference.
    pub fn ids(&self) -> impl Iterator<Item = NodeId> {
        [self.default, self.first, self.even].into_iter().flatten()
    }
}

/// Resolves a header or footer through Link to Previous: a section without a reference of that
/// kind uses the nearest previous section's. A `first`/`even` kind with no reference anywhere is
/// blank (`None`); it does not fall back to `default`.
#[must_use]
pub fn resolve_header_footer(
    refs: &[HeaderFooterRefs],
    index: usize,
    kind: HeaderFooterKind,
) -> Option<NodeId> {
    let end = index.min(refs.len().checked_sub(1)?);
    refs[..=end].iter().rev().find_map(|r| r.get(kind))
}

/// The first page number of each section given its page count (restarting at
/// `page_numbering.start`).
#[must_use]
pub fn section_start_page_numbers(sections: &[SectionProps], page_counts: &[i64]) -> Vec<i64> {
    let mut next = 1i64;
    sections
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let start = s
                .page_numbering
                .as_ref()
                .and_then(|p| p.start)
                .unwrap_or(next);
            next = start.saturating_add(page_counts.get(i).copied().unwrap_or(0));
            start
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_paper_by_locale() {
        assert_eq!(Paper::for_locale("en-US"), Paper::Letter);
        assert_eq!(Paper::for_locale("es-MX"), Paper::Letter);
        assert_eq!(Paper::for_locale("fr-FR"), Paper::A4);
        assert_eq!(Paper::for_locale("ar"), Paper::A4);
        assert_eq!(Paper::for_locale("en"), Paper::Letter);
        assert_eq!(Paper::for_locale("zh-Hant-TW"), Paper::A4);
        assert_eq!(Paper::B5.label(), "B5 (JIS)");
    }

    #[test]
    fn orientation_swaps_and_paper_names_match_either_way() {
        let land = SectionProps::new(Paper::A4).with_orientation(Orientation::Landscape);
        assert_eq!(
            (
                land.page_size.width,
                land.page_size.height,
                land.page_size.orientation
            ),
            (16838, 11906, Orientation::Landscape)
        );
        assert_eq!(land.paper(), Some(Paper::A4));
        assert_eq!(
            land.with_orientation(Orientation::Portrait).page_size.width,
            11906
        );
        assert_eq!(land.with_orientation(Orientation::Landscape), land);
        let mut odd = land;
        odd.page_size.width = 1000;
        assert_eq!(odd.paper(), None);
    }

    #[test]
    fn content_and_column_widths() {
        let s = SectionProps::new(Paper::Letter);
        assert_eq!(s.content_width(), 9360);
        assert_eq!(s.content_height(), 12960);
        let mut two = s.clone();
        two.columns = Columns {
            count: 2,
            ..Columns::default()
        };
        assert_eq!(two.column_widths(), vec![4320, 4320]);
        two.columns = Columns {
            count: 2,
            space: 0,
            equal_width: false,
            separator: true,
            cols: vec![
                Column {
                    width: 3000,
                    space: 360,
                },
                Column {
                    width: 6000,
                    space: 0,
                },
            ],
        };
        assert_eq!(two.column_widths(), vec![3000, 6000]);
        assert_eq!(s.column_widths(), vec![9360]);
        assert_eq!(MarginPreset::Narrow.margins().left, 720);
        assert_eq!(MarginPreset::Wide.margins().right, 2880);
        assert_eq!(MarginPreset::Moderate.margins().top, 1440);
        assert_eq!(MarginPreset::Normal.margins(), NORMAL_MARGINS);
    }

    #[test]
    fn break_behaviour_per_section_type() {
        assert_eq!(
            break_behaviour(SectionBreak::Continuous),
            BreakBehaviour {
                new_page: false,
                even: None
            }
        );
        assert_eq!(break_behaviour(SectionBreak::OddPage).even, Some(false));
        assert_eq!(break_behaviour(SectionBreak::EvenPage).even, Some(true));
        assert!(break_behaviour(SectionBreak::NextPage).new_page);
        assert!(!break_behaviour(SectionBreak::NextColumn).new_page);
    }

    #[test]
    fn header_kind_per_page() {
        let mut s = SectionProps::new(Paper::Letter);
        s.title_page = true;
        assert_eq!(
            s.header_kind_for_page(true, true, 1),
            HeaderFooterKind::First
        );
        assert_eq!(
            s.header_kind_for_page(true, false, 2),
            HeaderFooterKind::Even
        );
        assert_eq!(
            s.header_kind_for_page(false, false, 2),
            HeaderFooterKind::Default
        );
    }

    #[test]
    fn header_references_link_to_previous_sections() {
        let a = HeaderFooterRefs {
            default: Some(NodeId(1)),
            first: Some(NodeId(2)),
            even: None,
        };
        let b = HeaderFooterRefs::default();
        let mut c = HeaderFooterRefs::default();
        c.set(HeaderFooterKind::Default, Some(NodeId(9)));
        let all = [a, b, c];
        assert_eq!(
            resolve_header_footer(&all, 1, HeaderFooterKind::Default),
            Some(NodeId(1))
        );
        assert_eq!(
            resolve_header_footer(&all, 2, HeaderFooterKind::Default),
            Some(NodeId(9))
        );
        assert_eq!(
            resolve_header_footer(&all, 2, HeaderFooterKind::First),
            Some(NodeId(2))
        );
        assert_eq!(resolve_header_footer(&all, 2, HeaderFooterKind::Even), None);
        assert_eq!(resolve_header_footer(&[], 0, HeaderFooterKind::Even), None);
        assert_eq!(a.ids().count(), 2);
    }

    #[test]
    fn page_numbering_restarts() {
        let mut restart = SectionProps::new(Paper::Letter);
        restart.page_numbering = Some(PageNumbering {
            start: Some(1),
            format: Some(NumberFormat::Decimal),
        });
        let plain = SectionProps::new(Paper::Letter);
        assert_eq!(
            section_start_page_numbers(&[plain.clone(), plain, restart], &[3, 2, 4]),
            vec![1, 4, 1]
        );
    }
}
