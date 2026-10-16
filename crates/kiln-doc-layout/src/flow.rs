// SPDX-License-Identifier: AGPL-3.0-or-later
//! The page flow: sections → pages and columns, paragraphs → lines with Word's keep rules
//! (keep with next, keep lines together, widow/orphan control, page break before), forced page
//! and column breaks, tables row by row (rows that split across pages, rows that cannot, header
//! rows repeated on every page, vertically merged cells continued), floating objects and the
//! text that flows around them, footnotes reserved at the page bottom (the body gets shorter as
//! notes arrive), endnotes after the last section.
//!
//! The flow is resumable: before every top-level block it records a [`Snap`] of its state, so a
//! later layout restarts at the first changed block (or the start of its keep-with-next chain)
//! and stops early when its state meets the previous layout again (everything after is reused).

use std::sync::Arc;

use kiln_common::NodeId;
use kiln_doc::numbering::formats::format_number;
use kiln_doc::section::{break_behaviour, resolve_header_footer};
use kiln_doc::st::{NumberFormat, PageVAlign, SectionBreak, WrapType};
use kiln_doc::{Block, Document, Paragraph, SectionProps, Table};
use kiln_scene::Bounds;

use crate::cx::{BoxContent, Cx, decorations, flatten, line_items, spacing_gap};
use crate::drawing::{self, AnchorPoint, PageFrame};
use crate::page::{DrawingItem, Item, Layer, Page, StoryKind};
use crate::para::{Obstacle, ParaBox};
use crate::source::FieldEnv;
use crate::style::{Rule, RuleKind};
use crate::table::{CellBox, TableBox, cell_background, cell_borders, valign_offset};
use crate::units::twips;

/// How the document is presented.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum View {
    /// Pages as printed.
    Print,
    /// No pages: text wraps to the window, `width` px wide.
    Web {
        /// Window width in document px.
        width: f32,
    },
    /// Screen-sized pages side by side with larger type.
    Read {
        /// Page width.
        width: f32,
        /// Page height.
        height: f32,
    },
    /// One long column the width of the section's text, page breaks drawn as lines.
    Draft,
}

impl View {
    /// Whether the view has real pages (headers, footers, footnotes at the page bottom).
    #[must_use]
    pub fn paged(self) -> bool {
        matches!(self, View::Print)
    }
}

/// Height of the footnote separator band.
pub const NOTE_SEPARATOR: f32 = 16.0;
/// Text area of web, draft and read views: margins.
const SCREEN_MARGIN: f32 = 40.0;
/// "Infinite" page height of the web and draft views.
pub const ENDLESS: f32 = 4_000_000.0;
/// Blocks a keep-with-next chain may span before Word gives up on keeping it together.
const MAX_CHAIN: usize = 64;

/// Page geometry of a section in px.
#[derive(Debug, Clone, PartialEq)]
pub struct SectGeom {
    /// Section index.
    pub index: usize,
    /// Page width.
    pub width: f32,
    /// Page height.
    pub height: f32,
    /// Margins: top, right, bottom, left.
    pub margins: [f32; 4],
    /// Header distance from the top edge.
    pub header: f32,
    /// Footer distance from the bottom edge.
    pub footer: f32,
    /// Columns (x, width) in page coordinates.
    pub columns: Vec<(f32, f32)>,
    /// A rule between columns.
    pub separator: bool,
    /// Vertical alignment of the page's text.
    pub v_align: PageVAlign,
}

impl SectGeom {
    /// The geometry of section `index` in `view`.
    #[must_use]
    pub fn new(props: &SectionProps, index: usize, view: View) -> Self {
        let m = &props.margins;
        let mut g = Self {
            index,
            width: twips(props.page_size.width.clamp(1440, 31_680 * 4)),
            height: twips(props.page_size.height.clamp(1440, 31_680 * 4)),
            margins: [
                twips(m.top.abs()),
                twips(m.right.max(0)),
                twips(m.bottom.abs()),
                twips(m.left.max(0) + m.gutter.max(0)),
            ],
            header: twips(m.header.max(0)),
            footer: twips(m.footer.max(0)),
            columns: Vec::new(),
            separator: props.columns.separator,
            v_align: props.v_align.unwrap_or_default(),
        };
        // Keep at least an inch of text width and height.
        let min = 96.0;
        if g.width - g.margins[1] - g.margins[3] < min {
            g.margins[1] = 0.0;
            g.margins[3] = (g.width - min).max(0.0) / 2.0;
        }
        if g.height - g.margins[0] - g.margins[2] < min {
            g.margins[0] = (g.height - min).max(0.0) / 2.0;
            g.margins[2] = g.margins[0];
        }
        match view {
            View::Print => {}
            View::Web { width } => {
                g.width = width.max(200.0);
                g.height = ENDLESS;
                g.margins = [SCREEN_MARGIN, SCREEN_MARGIN, SCREEN_MARGIN, SCREEN_MARGIN];
            }
            View::Draft => {
                let text = g.width - g.margins[1] - g.margins[3];
                g.width = text + 2.0 * SCREEN_MARGIN;
                g.height = ENDLESS;
                g.margins = [SCREEN_MARGIN, SCREEN_MARGIN, SCREEN_MARGIN, SCREEN_MARGIN];
            }
            View::Read { width, height } => {
                g.width = width.max(240.0);
                g.height = height.max(240.0);
                g.margins = [SCREEN_MARGIN, SCREEN_MARGIN, SCREEN_MARGIN, SCREEN_MARGIN];
            }
        }
        let left = g.margins[3];
        let text_w = (g.width - g.margins[1] - g.margins[3]).max(1.0);
        let cols = &props.columns;
        if view.paged() && cols.count > 1 {
            if !cols.equal_width && !cols.cols.is_empty() {
                let mut x = left;
                for c in &cols.cols {
                    let w = twips(c.width.max(1));
                    g.columns.push((x, w));
                    x += w + twips(c.space.max(0));
                }
            } else {
                let n = cols.count.min(45);
                let space = twips(cols.space.max(0));
                #[allow(clippy::cast_precision_loss)]
                let nf = n as f32;
                let w = ((text_w - space * (nf - 1.0)) / nf).max(24.0);
                for i in 0..n {
                    #[allow(clippy::cast_precision_loss)]
                    let x = left + i as f32 * (w + space);
                    g.columns.push((x, w));
                }
            }
        } else {
            g.columns.push((left, text_w));
        }
        g
    }

    /// The text area width (all columns).
    #[must_use]
    pub fn text_width(&self) -> f32 {
        (self.width - self.margins[1] - self.margins[3]).max(1.0)
    }

    fn frame(&self, col: usize) -> PageFrame {
        PageFrame {
            width: self.width,
            height: self.height,
            top: self.margins[0],
            bottom: self.margins[2],
            left: self.margins[3],
            right: self.margins[1],
            column: self
                .columns
                .get(col)
                .copied()
                .unwrap_or((self.margins[3], self.text_width())),
        }
    }
}

/// The flow's state before a top-level block.
#[derive(Debug, Clone)]
pub struct Snap {
    pub(crate) pages: usize,
    pub(crate) items: usize,
    pub(crate) behind: usize,
    pub(crate) front: usize,
    pub(crate) obstacles: usize,
    pub(crate) col: usize,
    pub(crate) y: f32,
    pub(crate) col_top: f32,
    pub(crate) body_bottom: f32,
    pub(crate) has: bool,
    pub(crate) page_has: bool,
    pub(crate) soft_top: bool,
    pub(crate) prev: Option<(String, f32, bool)>,
    pub(crate) notes: Vec<(NodeId, Arc<BoxContent>)>,
    pub(crate) notes_h: f32,
    pub(crate) geom_index: usize,
    /// The block this snapshot precedes.
    pub block: NodeId,
}

impl Snap {
    /// Whether two snapshots put the next block in the same place.
    #[must_use]
    pub fn same_place(&self, other: &Snap) -> bool {
        self.pages == other.pages
            && self.col == other.col
            && (self.y - other.y).abs() < 0.01
            && self.obstacles == other.obstacles
            && self.notes.len() == other.notes.len()
            && self
                .notes
                .iter()
                .zip(&other.notes)
                .all(|(a, b)| a.0 == b.0 && (a.1.height - b.1.height).abs() < 0.01)
            && self.prev == other.prev
            && self.has == other.has
            && self.page_has == other.page_has
            && self.soft_top == other.soft_top
            && self.geom_index == other.geom_index
    }
}

/// The flow's persistent state (no borrows: kept between layouts).
#[derive(Debug, Clone)]
pub struct FlowState {
    /// Pages so far.
    pub pages: Vec<Page>,
    /// Snapshots before each top-level block (global index over all sections).
    pub snaps: Vec<Snap>,
    /// The next top-level block to place.
    pub next: usize,
    /// The flow reached the end of the document.
    pub done: bool,
    view: View,
    geom: Option<SectGeom>,
    col: usize,
    y: f32,
    col_top: f32,
    body_bottom: f32,
    has: bool,
    page_has: bool,
    soft_top: bool,
    prev: Option<(String, f32, bool)>,
    notes: Vec<(NodeId, Arc<BoxContent>)>,
    notes_h: f32,
}

impl FlowState {
    /// A flow at the start of the document.
    #[must_use]
    pub fn new(view: View) -> Self {
        Self {
            pages: Vec::new(),
            snaps: Vec::new(),
            next: 0,
            done: false,
            view,
            geom: None,
            col: 0,
            y: 0.0,
            col_top: 0.0,
            body_bottom: 0.0,
            has: false,
            page_has: false,
            soft_top: false,
            prev: None,
            notes: Vec::new(),
            notes_h: 0.0,
        }
    }

    /// The view.
    #[must_use]
    pub fn view(&self) -> View {
        self.view
    }

    fn snap(&self, block: NodeId) -> Snap {
        let page = self.pages.last();
        Snap {
            pages: self.pages.len(),
            items: page.map_or(0, |p| p.items.len()),
            behind: page.map_or(0, |p| p.behind.len()),
            front: page.map_or(0, |p| p.front.len()),
            obstacles: page.map_or(0, |p| p.obstacles.len()),
            col: self.col,
            y: self.y,
            col_top: self.col_top,
            body_bottom: self.body_bottom,
            has: self.has,
            page_has: self.page_has,
            soft_top: self.soft_top,
            prev: self.prev.clone(),
            notes: self.notes.clone(),
            notes_h: self.notes_h,
            geom_index: self.geom.as_ref().map_or(usize::MAX, |g| g.index),
            block,
        }
    }

    /// Rewinds to a snapshot (pages after it are dropped).
    pub fn restore(&mut self, s: &Snap, doc: &Document) {
        self.pages.truncate(s.pages);
        if let Some(p) = self.pages.last_mut() {
            p.items.truncate(s.items);
            p.behind.truncate(s.behind);
            p.front.truncate(s.front);
            p.obstacles.truncate(s.obstacles);
            p.notes.clear();
            p.header.clear();
            p.footer.clear();
            p.balloons.clear();
            p.column_rules.clear();
            p.body_dy = 0.0;
        }
        self.col = s.col;
        self.y = s.y;
        self.col_top = s.col_top;
        self.body_bottom = s.body_bottom;
        self.has = s.has;
        self.page_has = s.page_has;
        self.soft_top = s.soft_top;
        self.prev.clone_from(&s.prev);
        self.notes.clone_from(&s.notes);
        self.notes_h = s.notes_h;
        self.geom = doc
            .sections
            .get(s.geom_index)
            .map(|sec| SectGeom::new(&sec.props, s.geom_index, self.view));
        self.done = false;
    }
}

/// A top-level block with its section.
#[derive(Debug, Clone, Copy)]
pub struct FlatBlock<'d> {
    /// Section index.
    pub section: usize,
    /// First block of its section.
    pub first_in_section: bool,
    /// The block (a paragraph or a table).
    pub block: &'d Block,
}

/// Every top-level paragraph and table in order.
#[must_use]
pub fn flat_blocks(doc: &Document) -> Vec<FlatBlock<'_>> {
    let mut out = Vec::new();
    for (si, s) in doc.sections.iter().enumerate() {
        let mut v = Vec::new();
        flatten(&s.blocks, &mut v);
        for (i, b) in v.into_iter().enumerate() {
            out.push(FlatBlock {
                section: si,
                first_in_section: i == 0,
                block: b,
            });
        }
    }
    out
}

/// Why placement stopped early.
enum Stop {
    /// Rewind to the snapshot of this block and start it in a new column.
    Backtrack(usize),
}

/// One pass of the flow over a document.
pub struct Flow<'c, 'a> {
    /// The layout context.
    pub cx: &'c mut Cx<'a>,
    /// The state.
    pub st: &'c mut FlowState,
}

impl std::fmt::Debug for Flow<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flow")
            .field("pages", &self.st.pages.len())
            .finish_non_exhaustive()
    }
}

/// The displayed page number in a section's format.
#[must_use]
pub fn page_number_text(n: i64, format: Option<&NumberFormat>) -> String {
    match format {
        Some(f) => format_number(n, f),
        None => n.to_string(),
    }
}

impl Flow<'_, '_> {
    fn geom(&self) -> &SectGeom {
        // The flow always opens a page (and so a geometry) before placing anything.
        self.st.geom.as_ref().unwrap_or_else(|| unreachable_geom())
    }

    fn page(&mut self) -> &mut Page {
        if self.st.pages.is_empty() {
            self.open_page(0, false);
        }
        let last = self.st.pages.len() - 1;
        &mut self.st.pages[last]
    }

    fn col_bottom(&self) -> f32 {
        self.st.body_bottom - self.st.notes_h
    }

    fn column(&self) -> (f32, f32) {
        let g = self.geom();
        g.columns
            .get(self.st.col)
            .copied()
            .unwrap_or((g.margins[3], g.text_width()))
    }

    /// Field values for text on the current page.
    fn env(&self) -> FieldEnv {
        let page = self.st.pages.last();
        FieldEnv {
            page: page.map(|p| p.number_text.clone()),
            page_number: page.map(|p| p.number),
            num_pages: None,
            section_pages: None,
            section: self
                .st
                .geom
                .as_ref()
                .map(|g| i64::try_from(g.index + 1).unwrap_or(1)),
            now: self.cx.options.now,
            file_name: self.cx.options.file_name.clone(),
        }
    }

    /// Height of the header or footer that a page of the current section would show.
    fn hf_height(&mut self, section: usize, footer: bool, first: bool, number: i64) -> f32 {
        if !self.st.view.paged() {
            return 0.0;
        }
        let doc = self.cx.doc;
        let even_odd = doc.settings.even_and_odd_headers;
        let Some(sec) = doc.sections.get(section) else {
            return 0.0;
        };
        let kind = sec.props.header_kind_for_page(even_odd, first, number);
        let refs: Vec<_> = doc
            .sections
            .iter()
            .map(|s| if footer { s.footers } else { s.headers })
            .collect();
        // Without a first-page or even header anywhere before, Word shows an empty one.
        let Some(id) = resolve_header_footer(&refs, section, kind) else {
            return 0.0;
        };
        let Some(hf) = doc.headers_footers.iter().find(|h| h.id == id) else {
            return 0.0;
        };
        let g = SectGeom::new(&sec.props, section, self.st.view);
        let env = FieldEnv {
            page: Some(number.to_string()),
            page_number: Some(number),
            num_pages: Some(number.max(99)),
            section_pages: Some(99),
            ..FieldEnv::default()
        };
        let b = self.cx.blocks_box(
            &hf.blocks,
            g.text_width(),
            None,
            StoryKind::HeaderFooter(id),
            &env,
        );
        b.height
    }

    /// Opens a new page of section `section`; `blank` pages carry no text.
    fn open_page(&mut self, section: usize, blank: bool) {
        self.finish_page();
        let doc = self.cx.doc;
        let Some(sec) = doc.sections.get(section) else {
            return;
        };
        let geom = SectGeom::new(&sec.props, section, self.st.view);
        let (number, index_in_section) = match self.st.pages.last() {
            Some(p) if p.section == section => (p.number + 1, p.index_in_section + 1),
            Some(p) => (
                sec.props
                    .page_numbering
                    .as_ref()
                    .and_then(|n| n.start)
                    .unwrap_or(p.number + 1),
                0,
            ),
            None => (
                sec.props
                    .page_numbering
                    .as_ref()
                    .and_then(|n| n.start)
                    .unwrap_or(1),
                0,
            ),
        };
        let first = index_in_section == 0;
        let header_h = self.hf_height(section, false, first, number);
        let footer_h = self.hf_height(section, true, first, number);
        let top = if self.st.view.paged() && header_h > 0.0 {
            geom.margins[0].max(geom.header + header_h)
        } else {
            geom.margins[0]
        };
        let bottom = if self.st.view.paged() && footer_h > 0.0 {
            geom.height - geom.margins[2].max(geom.footer + footer_h)
        } else {
            geom.height - geom.margins[2]
        };
        let bottom = bottom.max(top + 48.0);
        let format = sec
            .props
            .page_numbering
            .as_ref()
            .and_then(|n| n.format.as_ref());
        let page = Page {
            section,
            number,
            number_text: page_number_text(number, format),
            index_in_section,
            width: geom.width,
            height: geom.height,
            body: Bounds::new(geom.margins[3], top, geom.text_width(), bottom - top),
            columns: geom.columns.clone(),
            blank,
            ..Page::default()
        };
        self.st.pages.push(page);
        self.st.col = 0;
        self.st.y = top;
        self.st.col_top = top;
        self.st.body_bottom = bottom;
        self.st.has = false;
        self.st.page_has = false;
        self.st.notes.clear();
        self.st.notes_h = 0.0;
        self.st.geom = Some(geom);
    }

    /// Lays the page's footnotes, column rules and vertical alignment out.
    fn finish_page(&mut self) {
        let notes = std::mem::take(&mut self.st.notes);
        let region_top = self.st.col_top;
        let view = self.st.view;
        let Some(geom) = self.st.geom.clone() else {
            return;
        };
        let Some(page) = self.st.pages.last_mut() else {
            return;
        };
        if !notes.is_empty() && view.paged() {
            let total: f32 = notes.iter().map(|(_, b)| b.height).sum();
            let mut y = page.body.bottom() - total;
            let sep_y = y - NOTE_SEPARATOR / 2.0;
            let x = page.body.x;
            page.notes.push(Item::Rule {
                x0: x,
                y0: sep_y,
                x1: x + (page.body.w / 3.0).min(192.0),
                y1: sep_y,
                rule: Rule {
                    width: 2.0 / 3.0,
                    color: None,
                    kind: RuleKind::Single,
                    space: 0.0,
                    weight: 0.0,
                },
            });
            for (_, b) in &notes {
                for band in &b.bands {
                    for it in &band.items {
                        let mut it = it.clone();
                        it.translate(x, y);
                        page.notes.push(it);
                    }
                }
                y += b.height;
            }
        }
        if geom.separator && geom.columns.len() > 1 {
            for w in geom.columns.windows(2) {
                let x = (w[0].0 + w[0].1 + w[1].0) / 2.0;
                page.column_rules
                    .push((x, region_top.max(page.body.y), page.body.bottom()));
            }
        }
        if matches!(geom.v_align, PageVAlign::Center | PageVAlign::Bottom) && view.paged() {
            let bottom = page
                .items
                .iter()
                .map(|i| i.bounds().bottom())
                .fold(page.body.y, f32::max);
            let free = (page.body.bottom() - bottom).max(0.0);
            let dy = if geom.v_align == PageVAlign::Center {
                free / 2.0
            } else {
                free
            };
            page.body_dy = dy;
        }
    }

    /// Moves to the next column, or a new page after the last one. `soft` breaks (overflow)
    /// drop the next paragraph's space before.
    fn next_column(&mut self, soft: bool) {
        let n = self.geom().columns.len();
        if self.st.col + 1 < n && self.st.view.paged() {
            self.st.col += 1;
            self.st.y = self.st.col_top;
            self.st.has = false;
            self.st.soft_top = soft;
            self.st.prev = None;
        } else {
            self.new_page(soft);
        }
    }

    /// Starts a new page of the current section.
    fn new_page(&mut self, soft: bool) {
        let section = self.geom().index;
        if self.st.view.paged() || matches!(self.st.view, View::Read { .. }) {
            self.open_page(section, false);
        } else {
            // Web and draft views have one endless page; a page break is a gap.
            self.st.y += 24.0;
            self.st.has = false;
        }
        self.st.soft_top = soft;
        self.st.prev = None;
    }

    /// Starts section `si` according to its break type.
    fn start_section(&mut self, si: usize) {
        let doc = self.cx.doc;
        let Some(sec) = doc.sections.get(si) else {
            return;
        };
        if self.st.pages.is_empty() {
            self.open_page(si, false);
            return;
        }
        let geom = SectGeom::new(&sec.props, si, self.st.view);
        let same_size = self.st.geom.as_ref().is_some_and(|g| {
            (g.width - geom.width).abs() < 0.5 && (g.height - geom.height).abs() < 0.5
        });
        let kind = sec.props.break_type;
        let behaviour = break_behaviour(kind);
        if !self.st.view.paged() {
            // One flow: a section break is a gap.
            self.st.geom = Some(geom);
            self.st.col = 0;
            if self.st.page_has {
                self.st.y += 12.0;
            }
            return;
        }
        match kind {
            SectionBreak::Continuous if same_size => {
                // Continue on this page below everything placed so far.
                self.finish_columns_region();
                let y = self.st.y;
                self.st.geom = Some(geom.clone());
                if let Some(p) = self.st.pages.last_mut() {
                    p.columns.clone_from(&geom.columns);
                }
                self.st.col = 0;
                self.st.col_top = y;
                self.st.y = y;
                self.st.has = false;
            }
            SectionBreak::NextColumn if same_size && self.st.col + 1 < geom.columns.len() => {
                self.st.geom = Some(geom);
                self.next_column(false);
            }
            _ => {
                if let Some(even) = behaviour.even {
                    // The next page number's parity decides whether a blank page goes first.
                    let next_number = self.st.pages.last().map_or(1, |p| p.number + 1);
                    let next_even = next_number % 2 == 0;
                    if next_even != even {
                        let prev = self.st.pages.last().map_or(si, |p| p.section);
                        self.open_page(prev, true);
                    }
                }
                self.open_page(si, false);
            }
        }
    }

    /// For a continuous section break: the lowest point used by any column of the page.
    fn finish_columns_region(&mut self) {
        let Some(page) = self.st.pages.last() else {
            return;
        };
        let lowest = page
            .items
            .iter()
            .map(|i| i.bounds().bottom())
            .filter(|b| b.is_finite())
            .fold(self.st.y, f32::max);
        self.st.y = lowest.min(self.st.body_bottom);
    }

    /// Places blocks from `self.st.next` until the end, or until `until_pages` pages are
    /// complete, or until the state meets `old` snapshots again (then returns the index where
    /// they meet).
    pub fn run(
        &mut self,
        blocks: &[FlatBlock<'_>],
        until_pages: Option<usize>,
        old: Option<&[Snap]>,
        dirty_until: usize,
    ) -> Option<usize> {
        let mut k = self.st.next;
        while k < blocks.len() {
            if let Some(n) = until_pages
                && self.st.pages.len() > n
            {
                self.st.next = k;
                return None;
            }
            let fb = blocks[k];
            if fb.first_in_section
                && (k > 0 || self.st.pages.is_empty())
                && self.st.geom.as_ref().is_none_or(|g| g.index != fb.section)
            {
                self.start_section(fb.section);
            }
            if self.st.pages.is_empty() {
                self.open_page(fb.section, false);
            }
            let snap = self.st.snap(fb.block.id());
            // Converged with the previous layout: everything from here is unchanged.
            if k > dirty_until
                && let Some(old) = old
                && let Some(o) = old.get(k)
                && o.block == snap.block
                && o.same_place(&snap)
            {
                self.st.next = k;
                return Some(k);
            }
            if self.st.snaps.len() > k {
                self.st.snaps.truncate(k);
            }
            self.st.snaps.push(snap);
            let r = match fb.block {
                Block::Paragraph(p) => self.place_para(p, blocks, k),
                Block::Table(t) => self.place_table(t, blocks, k),
                _ => Ok(()),
            };
            match r {
                Ok(()) => k += 1,
                Err(Stop::Backtrack(c)) => {
                    let s = self.st.snaps[c].clone();
                    self.st.restore(&s, self.cx.doc);
                    self.st.snaps.truncate(c);
                    self.next_column(true);
                    // Re-place from the chain start (now at the top of a column).
                    let snap = self.st.snap(blocks[c].block.id());
                    self.st.snaps.push(snap);
                    let r = match blocks[c].block {
                        Block::Paragraph(p) => self.place_para(p, blocks, c),
                        Block::Table(t) => self.place_table(t, blocks, c),
                        _ => Ok(()),
                    };
                    if r.is_err() {
                        // A chain start at the top of a column never backtracks; be safe.
                    }
                    k = c + 1;
                }
            }
        }
        self.st.next = blocks.len();
        self.end_of_document(blocks);
        None
    }

    /// Endnotes and the last page.
    fn end_of_document(&mut self, blocks: &[FlatBlock<'_>]) {
        let _ = blocks;
        let doc = self.cx.doc;
        let endnotes: Vec<NodeId> = endnote_order(doc);
        if !endnotes.is_empty() {
            let (x, w) = self.column();
            // Separator.
            let y = self.st.y + 8.0;
            let rule = Rule {
                width: 2.0 / 3.0,
                color: None,
                kind: RuleKind::Single,
                space: 0.0,
                weight: 0.0,
            };
            self.page().items.push(Item::Rule {
                x0: x,
                y0: y,
                x1: x + (w / 3.0).min(192.0),
                y1: y,
                rule,
            });
            self.st.y = y + NOTE_SEPARATOR / 2.0;
            let env = self.env();
            for id in endnotes {
                let Some(note) = doc.notes.iter().find(|n| n.id == id) else {
                    continue;
                };
                let b = self
                    .cx
                    .blocks_box(&note.blocks, w, None, StoryKind::Note(id), &env);
                self.place_bands(&b, x);
            }
        }
        self.finish_page();
        self.st.done = true;
    }

    /// Places the bands of a box one after the other, breaking between bands.
    fn place_bands(&mut self, b: &BoxContent, x: f32) {
        let mut offset = None::<f32>;
        for band in &b.bands {
            let top = offset.map_or(self.st.y, |o| band.y + o);
            if top + band.h > self.col_bottom() + 0.01 && self.st.has {
                self.next_column(true);
                offset = None;
            }
            let o = *offset.get_or_insert(self.st.y - band.y);
            for it in &band.items {
                let mut it = it.clone();
                it.translate(x, o);
                self.page().items.push(it);
            }
            self.st.y = band.y + o + band.h;
            self.st.has = true;
            self.st.page_has = true;
        }
        self.st.y = self.st.y.max(offset.map_or(self.st.y, |o| b.height + o));
    }

    /// The start of the keep-with-next chain that ends just before block `k` (`None` when the
    /// previous block does not keep with next, or the chain starts at the top of a column).
    fn chain_start(&self, blocks: &[FlatBlock<'_>], k: usize) -> Option<usize> {
        let mut c = k;
        while c > 0 && k - c < MAX_CHAIN {
            let prev = &blocks[c - 1];
            if prev.section != blocks[k].section {
                break;
            }
            let keeps = match prev.block {
                Block::Paragraph(p) => self.keeps_with_next(p),
                _ => false,
            };
            if !keeps {
                break;
            }
            c -= 1;
        }
        if c == k {
            return None;
        }
        let snap = self.st.snaps.get(c)?;
        // Only when the chain start is not already at the top of a column on this page.
        (snap.has && snap.pages == self.st.pages.len()).then_some(c)
    }

    fn keeps_with_next(&self, p: &Paragraph) -> bool {
        let doc = self.cx.doc;
        let r = kiln_doc::resolve::resolve_paragraph(&doc.context(), &p.props, None);
        r.keep_next
    }

    /// Registers the floating objects anchored in a paragraph whose top is at `top`.
    fn place_anchors(&mut self, pb: &ParaBox, top: f32) {
        if pb.src.anchors.is_empty() {
            return;
        }
        let geom = self.geom().clone();
        let frame = geom.frame(self.st.col);
        let (cx0, _) = self.column();
        let at = AnchorPoint {
            para_top: top,
            line_top: top,
            char_x: cx0 + pb.src.format.indent_left,
        };
        let doc = self.cx.doc;
        for a in &pb.src.anchors {
            let d = &a.drawing;
            let Some(anchor) = drawing::anchor_of(d) else {
                continue;
            };
            let rect = drawing::float_rect(d, anchor, &frame, &at);
            let picture = match &d.kind {
                kiln_doc::DrawingKind::Picture { media, .. }
                    if matches!(anchor.wrap, WrapType::Tight | WrapType::Through) =>
                {
                    self.cx.caches.media.get(doc, media)
                }
                _ => None,
            };
            let obstacle = drawing::obstacle(d, anchor, rect, picture.as_deref(), &doc.theme);
            let layer = if anchor.wrap == WrapType::Behind {
                Layer::Behind
            } else {
                Layer::Front
            };
            let item = Item::Drawing(DrawingItem {
                rect,
                drawing: d.clone(),
                layer,
                para: pb.src.id,
                offset: a.doc,
            });
            let text = self.text_box_items(d, rect);
            let page = self.page();
            let list = if layer == Layer::Behind {
                &mut page.behind
            } else {
                &mut page.front
            };
            list.push(item);
            list.extend(text);
            if let Some(o) = obstacle {
                page.obstacles.push(o);
            }
        }
    }

    /// The text of a text box laid out inside its shape.
    fn text_box_items(&mut self, d: &kiln_doc::Drawing, rect: Bounds) -> Vec<Item> {
        let Some(blocks) = &d.text_box else {
            return Vec::new();
        };
        let ([il, it, ir, ib], anchor) = drawing::body_props(d);
        let env = self.env();
        let b = self.cx.blocks_box(
            blocks,
            (rect.w - il - ir).max(1.0),
            None,
            StoryKind::TextBox(d.id),
            &env,
        );
        let free = (rect.h - it - ib - b.height).max(0.0);
        let dy = match anchor {
            "ctr" => free / 2.0,
            "b" => free,
            _ => 0.0,
        };
        let mut out = Vec::new();
        for band in &b.bands {
            for item in &band.items {
                let mut item = item.clone();
                item.translate(rect.x + il, rect.y + it + dy);
                out.push(item);
            }
        }
        out
    }

    /// Footnotes referenced on lines `range` that are not on the page yet, laid out.
    fn new_notes(
        &mut self,
        pb: &ParaBox,
        range: std::ops::Range<usize>,
    ) -> Vec<(NodeId, Arc<BoxContent>)> {
        let mut out = Vec::new();
        if !self.st.view.paged() {
            return out;
        }
        let doc = self.cx.doc;
        for i in range {
            for &id in &pb.lines[i].notes {
                if self.st.notes.iter().any(|(n, _)| *n == id)
                    || out
                        .iter()
                        .any(|(n, _): &(NodeId, Arc<BoxContent>)| *n == id)
                {
                    continue;
                }
                let Some(note) = doc.notes.iter().find(|n| n.id == id) else {
                    continue;
                };
                if note.kind != kiln_doc::st::NoteKind::Footnote {
                    continue;
                }
                let w = self.geom().text_width();
                let env = self.env();
                let b = self
                    .cx
                    .blocks_box(&note.blocks, w, None, StoryKind::Note(id), &env);
                out.push((id, Arc::new(b)));
            }
        }
        out
    }

    fn notes_extra(&self, new: &[(NodeId, Arc<BoxContent>)]) -> f32 {
        let h: f32 = new.iter().map(|(_, b)| b.height).sum();
        if h > 0.0 && self.st.notes.is_empty() {
            h + NOTE_SEPARATOR
        } else {
            h
        }
    }

    /// Places a top-level paragraph.
    fn place_para(
        &mut self,
        p: &Paragraph,
        blocks: &[FlatBlock<'_>],
        k: usize,
    ) -> Result<(), Stop> {
        let (_, col_w) = self.column();
        let env = self.env();
        let mut pb = self.cx.para(p, col_w, None, &env);
        let format = pb.src.format.clone();
        if format.page_break_before && self.st.page_has && self.st.view.paged() {
            self.new_page(false);
        }
        {
            let c = self.column();
            if (c.1 - col_w).abs() > 0.01 {
                let env = self.env();
                pb = self.cx.para(p, c.1, None, &env);
            }
        }
        // Spacing: dropped at the top of a column the text overflowed into.
        let gap = if !self.st.has && self.st.soft_top {
            0.0
        } else {
            spacing_gap(self.st.prev.as_ref(), &format)
        } + format.border_above();
        let mut top = self.st.y + gap;
        let first_h = pb.lines.first().map_or(0.0, |l| l.height);
        if top + first_h > self.col_bottom() + 0.01 && self.st.has {
            if let Some(c) = self.chain_start(blocks, k) {
                return Err(Stop::Backtrack(c));
            }
            self.next_column(true);
            top = self.st.y;
        }
        // Floating objects of this paragraph, then the lines around every obstacle.
        let marks = {
            let page = self.page();
            (page.behind.len(), page.front.len(), page.obstacles.len())
        };
        self.place_anchors(&pb, top);
        let wraps = |st: &FlowState, top: f32, h: f32| {
            st.pages.last().is_some_and(|pg| {
                pg.obstacles
                    .iter()
                    .any(|o| o.rect.bottom() > top && o.rect.y < top + h)
            })
        };
        let (cx0, _) = self.column();
        if wraps(self.st, top, pb.height().max(1.0)) {
            let obstacles: Vec<Obstacle> = self.page().obstacles.clone();
            pb = Arc::new(ParaBox::wrapped(
                pb.src.clone(),
                &mut self.cx.caches.engine,
                0,
                cx0,
                top,
                &obstacles,
            ));
        }
        let mut start = 0usize;
        let mut continued = false;
        loop {
            let n = pb.lines.len();
            if n == 0 {
                break;
            }
            let bottom = self.col_bottom();
            // The paragraph origin: lines may start below it (pushed down by an obstacle).
            let base = if start == 0 { 0.0 } else { pb.lines[start].top };
            // Lines that fit, footnotes included; a forced break ends the run.
            let mut fit = 0usize;
            let mut forced = None;
            let mut pending: Vec<(NodeId, Arc<BoxContent>)> = Vec::new();
            for i in start..n {
                let l = &pb.lines[i];
                let line_bottom = top + (l.bottom() - base);
                let new = self.new_notes(&pb, i..i + 1);
                let mut all = pending.clone();
                all.extend(new.iter().cloned());
                let extra = self.notes_extra(&all);
                if line_bottom > bottom - extra + 0.01 {
                    break;
                }
                pending = all;
                fit += 1;
                if l.forced.is_some() {
                    forced = l.forced;
                    break;
                }
            }
            let remaining = n - start;
            if fit < remaining && forced.is_none() {
                // Keep rules decide how many of the fitting lines stay here.
                let at_top = !self.st.has;
                if !continued && format.keep_lines && !at_top {
                    let whole = pb.height();
                    if whole <= bottom - self.st.col_top {
                        fit = 0;
                    }
                }
                if format.widow_control && fit > 0 {
                    // Widow: never leave the last line alone on the next page.
                    if remaining - fit == 1 && fit >= 1 {
                        fit -= 1;
                    }
                    // Orphan: never leave the first line alone at the bottom.
                    if !continued && fit == 1 && remaining > 1 {
                        fit = 0;
                    }
                    if fit == 0 && at_top {
                        fit = 1.min(remaining);
                    }
                }
                if fit == 0 && at_top {
                    // Nothing fits even in an empty column: place one line anyway.
                    fit = 1;
                }
                pending = if fit > 0 {
                    self.new_notes(&pb, start..start + fit)
                } else {
                    Vec::new()
                };
            }
            if fit == 0 {
                if !continued && start == 0 {
                    // The whole paragraph moves: drop its floats and look for a keep chain.
                    {
                        let page = self.page();
                        page.behind.truncate(marks.0);
                        page.front.truncate(marks.1);
                        page.obstacles.truncate(marks.2);
                    }
                    if let Some(c) = self.chain_start(blocks, k) {
                        return Err(Stop::Backtrack(c));
                    }
                    self.next_column(true);
                    return self.place_para_at_top(p, blocks, k);
                }
                self.next_column(true);
                top = self.st.y;
                continued = true;
                pb = self.relayout_rest(&pb, start, top);
                start = 0;
                continue;
            }
            // Place lines start..start+fit.
            let dy = top - base;
            let width = pb.width;
            let (cx0, _) = self.column();
            for i in start..start + fit {
                let mut items = Vec::new();
                decorations(&pb, &format, i, cx0, dy, width, &mut items);
                line_items(&pb, i, cx0, dy, StoryKind::Body, &mut items);
                self.page().items.extend(items);
            }
            self.add_notes(pending);
            let last = &pb.lines[start + fit - 1];
            self.st.y = top + (last.bottom() - base);
            self.st.has = true;
            self.st.page_has = true;
            start += fit;
            if let Some(column) = forced {
                if column {
                    self.next_column(false);
                } else {
                    self.new_page(false);
                }
                if start >= n {
                    // A break at the end of the paragraph: the next paragraph starts there.
                    break;
                }
                top = self.st.y;
                continued = true;
                pb = self.relayout_rest(&pb, start, top);
                start = 0;
                continue;
            }
            if start >= n {
                break;
            }
            self.next_column(true);
            top = self.st.y;
            continued = true;
            pb = self.relayout_rest(&pb, start, top);
            start = 0;
        }
        self.st.y += format.border_below();
        self.st.prev = Some((
            format.style_id.clone(),
            format.space_after,
            format.contextual_spacing,
        ));
        self.st.y += 0.0;
        if self.st.y > self.col_bottom() {
            self.st.y = self.col_bottom();
        }
        // The space after is added before the next paragraph (it may collapse).
        Ok(())
    }

    /// Places a paragraph after moving it to the top of a new column (no backtracking).
    fn place_para_at_top(
        &mut self,
        p: &Paragraph,
        blocks: &[FlatBlock<'_>],
        k: usize,
    ) -> Result<(), Stop> {
        self.st.soft_top = true;
        // At the top of a column the chain check is off (`has` is false), so this terminates.
        self.place_para(p, blocks, k)
    }

    /// The rest of a paragraph from line `start`, laid out again when the new column has
    /// obstacles (or the old layout wrapped around some).
    fn relayout_rest(&mut self, pb: &Arc<ParaBox>, start: usize, top: f32) -> Arc<ParaBox> {
        let wrapped = pb.pieces.len() > 1;
        let (cx0, w) = self.column();
        let obstacles: Vec<Obstacle> = self
            .st
            .pages
            .last()
            .map(|p| p.obstacles.clone())
            .unwrap_or_default();
        let width_changed = (w - pb.width).abs() > 0.01;
        if !wrapped && obstacles.is_empty() && !width_changed {
            // Same layout, lines from `start` on.
            return Arc::new(tail(pb, start));
        }
        let from = pb.lines.get(start).map_or(0, |l| l.bytes.start);
        let mut src = (*pb.src).clone();
        src.spec.width = w;
        Arc::new(ParaBox::wrapped(
            Arc::new(src),
            &mut self.cx.caches.engine,
            from,
            cx0,
            top,
            &obstacles,
        ))
    }

    fn add_notes(&mut self, notes: Vec<(NodeId, Arc<BoxContent>)>) {
        if notes.is_empty() {
            return;
        }
        let extra = self.notes_extra(&notes);
        self.st.notes_h += extra;
        self.st.notes.extend(notes);
    }

    // ------------------------------------------------------------------ tables

    /// Places a top-level table row by row.
    fn place_table(&mut self, t: &Table, blocks: &[FlatBlock<'_>], k: usize) -> Result<(), Stop> {
        let (_, col_w) = self.column();
        let env = self.env();
        let tb = self.cx.table(t, col_w, &env, StoryKind::Body);
        if tb.rows.is_empty() {
            return Ok(());
        }
        // Space after the previous paragraph.
        if (self.st.has || !self.st.soft_top)
            && let Some((_, after, _)) = &self.st.prev
        {
            self.st.y += after;
        }
        self.st.prev = None;
        let mut heights: Vec<f32> = tb.rows.iter().map(|r| r.height).collect();
        // Content still to place per cell (bands not yet placed), and where each cell's
        // current segment started.
        let mut queue: Vec<std::collections::VecDeque<usize>> = tb
            .cells
            .iter()
            .map(|c| (0..c.content.bands.len()).collect())
            .collect();
        let mut seg_top: Vec<Option<f32>> = vec![None; tb.cells.len()];
        let mut placed_any = false;
        // Where this table's rows start in the current column when nothing but its repeated
        // header rows sits above them. A row that cannot split but does not fit there either
        // would fit nowhere: it is split (or, with an exact height, clipped) there, as Word
        // does, instead of moving on to another empty column.
        let mut fresh_y = (!self.st.has).then_some(self.st.y);
        // Every pass places a row, moves to an empty column (at most once per row) or places
        // at least one band, so this bound is never reached by a correct layout; it only
        // guarantees termination.
        let bands: usize = tb.cells.iter().map(|c| c.content.bands.len()).sum();
        let limit = 2 * (tb.rows.len() + bands) + 64;
        let mut r = 0usize;
        let mut guard = 0usize;
        while r < tb.rows.len() {
            guard += 1;
            if guard > limit {
                break;
            }
            let (cx0, _) = self.column();
            let x = cx0 + tb.x;
            let h = heights[r];
            let avail = self.col_bottom() - self.st.y;
            let row = &tb.rows[r];
            // Open the cells starting in this row.
            for &c in &row.cells {
                if seg_top[c].is_none() {
                    seg_top[c] = Some(self.st.y);
                }
            }
            if h <= avail + 0.01 {
                let y_end = self.st.y + h;
                let mut items = Vec::new();
                for (c, cell) in tb.cells.iter().enumerate() {
                    let end_row = cell.row + cell.row_span - 1;
                    if cell.row <= r
                        && end_row == r
                        && let Some(top) = seg_top[c].take()
                    {
                        flush_cell(
                            cell,
                            c,
                            &mut queue[c],
                            x,
                            top,
                            y_end,
                            true,
                            false,
                            &mut items,
                        );
                    }
                }
                self.page().items.extend(items);
                self.st.y = y_end;
                self.st.has = true;
                self.st.page_has = true;
                placed_any = true;
                r += 1;
                continue;
            }
            let empty = fresh_y.is_some_and(|f| (self.st.y - f).abs() < 0.5);
            let can_split = !row.cant_split && !row.exact && avail > 12.0;
            if !can_split && !empty {
                if !placed_any && let Some(c) = self.chain_start(blocks, k) {
                    return Err(Stop::Backtrack(c));
                }
                // Close the open segments of spanning cells on this page.
                let mut items = Vec::new();
                for (c, cell) in tb.cells.iter().enumerate() {
                    if let Some(top) = seg_top[c]
                        && cell.row < r
                    {
                        flush_cell(
                            cell,
                            c,
                            &mut queue[c],
                            x,
                            top,
                            self.st.y,
                            false,
                            false,
                            &mut items,
                        );
                    }
                    if cell.row == r {
                        seg_top[c] = None;
                    }
                }
                self.page().items.extend(items);
                self.next_column(true);
                self.repeat_headers(&tb, r);
                fresh_y = Some(self.st.y);
                for (c, cell) in tb.cells.iter().enumerate() {
                    if cell.row < r && cell.row + cell.row_span > r {
                        seg_top[c] = Some(self.st.y);
                    }
                }
                continue;
            }
            let open = |c: usize| seg_top[c].is_some() && tb.cells[c].row <= r;
            let nothing_left = (0..tb.cells.len()).all(|c| !open(c) || queue[c].is_empty());
            if empty && (row.exact || nothing_left) {
                // Taller than an empty column with an exact height (or only cell margins
                // left): clip the row at the column bottom.
                let y_end = self.col_bottom().max(self.st.y);
                let mut items = Vec::new();
                for (c, cell) in tb.cells.iter().enumerate() {
                    if cell.row <= r
                        && cell.row + cell.row_span - 1 == r
                        && let Some(top) = seg_top[c].take()
                    {
                        flush_cell(
                            cell,
                            c,
                            &mut queue[c],
                            x,
                            top,
                            y_end,
                            false,
                            true,
                            &mut items,
                        );
                        queue[c].clear();
                    }
                }
                self.page().items.extend(items);
                self.st.y = y_end;
                self.st.has = true;
                self.st.page_has = true;
                placed_any = true;
                r += 1;
                continue;
            }
            // Split the row at the column bottom.
            let y_end = self.col_bottom();
            let col_top = self.st.col_top;
            let mut items = Vec::new();
            let mut rest = 0.0_f32;
            for (c, cell) in tb.cells.iter().enumerate() {
                if let Some(top) = seg_top[c]
                    && cell.row <= r
                {
                    flush_cell(
                        cell,
                        c,
                        &mut queue[c],
                        x,
                        top,
                        y_end,
                        false,
                        (top - col_top).abs() < 0.5
                            || fresh_y.is_some_and(|f| (top - f).abs() < 0.5),
                        &mut items,
                    );
                    if cell.row + cell.row_span - 1 == r {
                        let left: f32 = queue[c].iter().map(|&b| cell.content.bands[b].h).sum();
                        rest = rest.max(left + cell.margins[0] + cell.margins[2]);
                    }
                }
            }
            self.page().items.extend(items);
            self.st.y = y_end;
            self.st.has = true;
            self.st.page_has = true;
            placed_any = true;
            heights[r] = rest.max(1.0);
            self.next_column(true);
            self.repeat_headers(&tb, r);
            fresh_y = Some(self.st.y);
            for (c, cell) in tb.cells.iter().enumerate() {
                if cell.row <= r && cell.row + cell.row_span > r {
                    seg_top[c] = Some(self.st.y);
                }
            }
        }
        Ok(())
    }

    /// Repeats the table's header rows at the top of a new page (not when the header rows are
    /// what is being placed).
    fn repeat_headers(&mut self, tb: &TableBox, r: usize) {
        if tb.header_rows == 0 || r < tb.header_rows || !self.st.view.paged() {
            return;
        }
        let (cx0, _) = self.column();
        let x = cx0 + tb.x;
        let mut items = Vec::new();
        let mut y = self.st.y;
        for hr in 0..tb.header_rows {
            let h = tb.rows[hr].height;
            if y + h > self.col_bottom() {
                break;
            }
            tb.paint_row(hr, x, y, h, &mut items);
            y += h;
        }
        self.page().items.extend(items);
        self.st.y = y;
        self.st.has = true;
    }
}

/// Lines `start..` of a paragraph as their own box (same layout, same pieces).
fn tail(pb: &ParaBox, start: usize) -> ParaBox {
    let mut out = pb.clone();
    let base = pb.lines.get(start).map_or(0.0, |l| l.top);
    out.lines = pb.lines[start.min(pb.lines.len())..].to_vec();
    for l in &mut out.lines {
        l.top -= base;
    }
    for p in &mut out.pieces {
        p.y -= base;
    }
    out
}

/// Places a cell's segment from `top` to `bottom` (page y): its background and borders, and as
/// many of its queued bands as fit (all of them when `last`).
#[allow(clippy::too_many_arguments)]
fn flush_cell(
    cell: &CellBox,
    _index: usize,
    queue: &mut std::collections::VecDeque<usize>,
    x: f32,
    top: f32,
    bottom: f32,
    last: bool,
    fresh: bool,
    out: &mut Vec<Item>,
) {
    let rect = Bounds::new(x + cell.x, top, cell.w, (bottom - top).max(0.0));
    cell_background(cell, rect, out);
    let whole = queue.len() == cell.content.bands.len();
    let dy = if whole && last {
        valign_offset(cell, rect.h)
    } else {
        0.0
    };
    let inner_top = top + cell.margins[0];
    let inner_bottom = bottom - cell.margins[2];
    let mut offset: Option<f32> = None;
    let mut placed = 0usize;
    while let Some(&b) = queue.front() {
        let band = &cell.content.bands[b];
        let o = *offset.get_or_insert(inner_top + dy - band.y);
        // A band that does not fit waits for the next segment, except the first band of a
        // segment that starts a column (it would never fit anywhere).
        if !last && band.y + o + band.h > inner_bottom + 0.01 && (placed > 0 || !fresh) {
            break;
        }
        placed += 1;
        for it in &band.items {
            let mut it = it.clone();
            it.translate(x + cell.x + cell.margins[1], o);
            it.set_fill(cell.fill);
            out.push(it);
        }
        queue.pop_front();
    }
    cell_borders(cell, rect, true, true, out);
}

/// Endnotes in reference order.
#[must_use]
pub fn endnote_order(doc: &Document) -> Vec<NodeId> {
    let mut out = Vec::new();
    for p in doc.body_paragraphs() {
        for o in p.content.objects() {
            if let kiln_doc::InlineObject::NoteRef(n) = o
                && n.kind == kiln_doc::st::NoteKind::Endnote
                && !out.contains(&n.note)
            {
                out.push(n.note);
            }
        }
    }
    out
}

#[cold]
fn unreachable_geom() -> &'static SectGeom {
    static FALLBACK: std::sync::OnceLock<SectGeom> = std::sync::OnceLock::new();
    FALLBACK
        .get_or_init(|| SectGeom::new(&SectionProps::new(kiln_doc::Paper::Letter), 0, View::Print))
}
