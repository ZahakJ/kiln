// SPDX-License-Identifier: AGPL-3.0-or-later
//! [`DocLayout`]: the layout of one document view, kept up to date as the document changes.
//!
//! - **First layout** flows the document from the start; [`DocLayout::layout_until`] stops once
//!   a number of pages exist (the first page paints before the rest is paginated) and
//!   [`DocLayout::resume`] continues in steps.
//! - **Edits**: [`DocLayout::update`] forgets the changed paragraphs' layouts, rewinds the flow to
//!   the first changed top-level block (or the start of its keep-with-next chain) and flows from
//!   there until the state meets the previous layout again; everything after is reused.
//! - **Finishing** places headers and footers (with PAGE, NUMPAGES, SECTIONPAGES), comment
//!   balloons and the pages' positions in the view, and indexes lines for hit testing.

use std::collections::HashMap;
use std::sync::Arc;

use kiln_common::NodeId;
use kiln_doc::numbering::ListLabel;
use kiln_doc::numbering::formats::format_number;
use kiln_doc::section::resolve_header_footer;
use kiln_doc::st::{NoteKind, NumberFormat};
use kiln_doc::{Document, InlineObject, Mark};
use kiln_scene::Bounds;
use kiln_text::{FontStore, TextEngine};
use rustc_hash::FxHashMap;

use crate::cx::{BoxContent, Caches, Cx, Options};
use crate::flow::{Flow, FlowState, View, flat_blocks};
use crate::page::{Balloon, Item, LineItem, Page, StoryKind};
use crate::source::FieldEnv;

/// Space above the first page and between pages (print layout).
pub const PAGE_GAP: f32 = 32.0;
/// Space beside the pages (holds the page shadow).
pub const SIDE_PAD: f32 = 64.0;
/// Space below the last page.
pub const BOTTOM_PAD: f32 = 96.0;
/// Width of the comment balloons right of the page.
pub const BALLOON_W: f32 = 240.0;

/// What changed in the document since the last layout.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Everything may have changed (styles, settings, theme, a new document).
    All,
    /// These nodes changed (paragraphs, tables, cells; their ancestors are found).
    Nodes(Vec<NodeId>),
}

/// Where a body line is: page and index into [`DocLayout::page_lines`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LineRef {
    /// Page index.
    pub page: u32,
    /// Index into the page's line list.
    pub index: u32,
}

/// Timing and cache figures of the last layout (for benchmarks and the status bar).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    /// Top-level blocks placed by the last layout call.
    pub blocks_placed: usize,
    /// Whether the last update converged with the previous layout before the end.
    pub converged: bool,
    /// Paragraph layouts cached.
    pub cached_paragraphs: usize,
}

/// The layout of a document.
pub struct DocLayout {
    caches: Caches,
    options: Options,
    state: FlowState,
    labels: HashMap<NodeId, ListLabel>,
    note_numbers: FxHashMap<NodeId, String>,
    finished: bool,
    /// Lines per page (header, body, notes, footer, text boxes) in page coordinates.
    page_lines: Vec<Vec<LineItem>>,
    /// Lines of each paragraph.
    para_lines: FxHashMap<NodeId, Vec<LineRef>>,
    /// Body lines in reading order.
    order: Vec<LineRef>,
    /// Rank of each paragraph in reading order (body and its tables).
    para_rank: FxHashMap<NodeId, usize>,
    hf_cache: FxHashMap<(NodeId, u64), Arc<BoxContent>>,
    stats: Stats,
    /// Pages of the previous layout after the point an incomplete re-flow reached, shown until
    /// the flow catches up (painted, never hit-tested), with the old index of the first one.
    stale: Vec<Page>,
    stale_from: usize,
    until: Option<usize>,
}

impl std::fmt::Debug for DocLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocLayout")
            .field("pages", &self.state.pages.len())
            .field("done", &self.state.done)
            .field("view", &self.state.view())
            .finish_non_exhaustive()
    }
}

/// Display numbers of footnotes and endnotes in reference order.
#[must_use]
pub fn note_numbers(doc: &Document) -> FxHashMap<NodeId, String> {
    let mut out = FxHashMap::default();
    let mut foot = doc.settings.footnotes.start.unwrap_or(1);
    let mut end = doc.settings.endnotes.start.unwrap_or(1);
    let foot_fmt = doc
        .settings
        .footnotes
        .format
        .clone()
        .unwrap_or(NumberFormat::Decimal);
    let end_fmt = doc
        .settings
        .endnotes
        .format
        .clone()
        .unwrap_or(NumberFormat::LowerRoman);
    for p in doc.body_paragraphs() {
        for o in p.content.objects() {
            if let InlineObject::NoteRef(n) = o {
                if n.custom_mark.is_some() || out.contains_key(&n.note) {
                    continue;
                }
                let text = match n.kind {
                    NoteKind::Footnote => {
                        let t = format_number(foot, &foot_fmt);
                        foot += 1;
                        t
                    }
                    NoteKind::Endnote => {
                        let t = format_number(end, &end_fmt);
                        end += 1;
                        t
                    }
                };
                out.insert(n.note, text);
            }
        }
    }
    out
}

/// The default font store: the bundled OFL fonts (identical layout on every platform).
#[must_use]
pub fn default_fonts() -> FontStore {
    #[cfg(not(target_arch = "wasm32"))]
    {
        FontStore::bundled()
    }
    #[cfg(target_arch = "wasm32")]
    {
        FontStore::new(&kiln_text::FontStoreOptions {
            bundled: false,
            system: false,
        })
    }
}

impl DocLayout {
    /// An empty layout over a font store, in print layout.
    #[must_use]
    pub fn new(fonts: FontStore) -> Self {
        Self {
            caches: Caches::new(TextEngine::new(fonts)),
            options: Options::default(),
            state: FlowState::new(View::Print),
            labels: HashMap::new(),
            note_numbers: FxHashMap::default(),
            finished: false,
            page_lines: Vec::new(),
            para_lines: FxHashMap::default(),
            order: Vec::new(),
            para_rank: FxHashMap::default(),
            hf_cache: FxHashMap::default(),
            stats: Stats::default(),
            stale: Vec::new(),
            stale_from: 0,
            until: None,
        }
    }

    /// The options.
    #[must_use]
    pub fn options(&self) -> &Options {
        &self.options
    }

    /// Changes options (the next layout starts over).
    pub fn set_options(&mut self, options: Options) {
        if options != self.options {
            self.options = options;
            self.caches.invalidate_all();
            self.reset();
        }
    }

    /// The view.
    #[must_use]
    pub fn view(&self) -> View {
        self.state.view()
    }

    /// Changes the view (the next layout starts over). Read Mode uses 1.25× type.
    pub fn set_view(&mut self, view: View) {
        if view == self.state.view() {
            return;
        }
        let scale = if matches!(view, View::Read { .. }) {
            1.25
        } else {
            1.0
        };
        if (self.options.scale - scale).abs() > f32::EPSILON {
            self.options.scale = scale;
            self.caches.invalidate_all();
        }
        self.state = FlowState::new(view);
        self.finished = false;
    }

    fn reset(&mut self) {
        self.stale.clear();
        let view = self.state.view();
        self.state = FlowState::new(view);
        self.finished = false;
        self.hf_cache.clear();
    }

    /// The text engine (fonts).
    pub fn engine_mut(&mut self) -> &mut TextEngine {
        &mut self.caches.engine
    }

    /// The text engine.
    #[must_use]
    pub fn engine(&self) -> &TextEngine {
        &self.caches.engine
    }

    /// Decoded pictures.
    pub fn media_mut(&mut self) -> &mut crate::media::MediaCache {
        &mut self.caches.media
    }

    /// The fonts and the picture cache at once (painting needs both).
    pub fn fonts_and_media(&mut self) -> (&FontStore, &mut crate::media::MediaCache) {
        (self.caches.engine.fonts(), &mut self.caches.media)
    }

    /// Lays the whole document out from scratch.
    pub fn layout(&mut self, doc: &Document) {
        self.caches.invalidate_all();
        self.reset();
        self.flow(doc, None, None, 0);
    }

    /// Lays the document out from scratch until `pages` pages are complete; [`DocLayout::resume`]
    /// continues. Returns whether the layout is complete.
    pub fn layout_until(&mut self, doc: &Document, pages: usize) -> bool {
        self.caches.invalidate_all();
        self.reset();
        self.flow(doc, Some(pages), None, 0)
    }

    /// Continues an incomplete layout until `more` further pages exist (`None`: to the end).
    /// Returns whether the layout is complete.
    pub fn resume(&mut self, doc: &Document, more: Option<usize>) -> bool {
        if self.state.done {
            return true;
        }
        let until = more.map(|m| self.state.pages.len() + m);
        self.flow(doc, until, None, 0)
    }

    /// Whether every page is laid out.
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.state.done
    }

    /// Brings the layout up to date after a change.
    pub fn update(&mut self, doc: &Document, change: &Change) {
        self.update_until(doc, change, None);
    }

    /// [`DocLayout::update`] that stops re-flowing once `until_pages` pages are laid out (the
    /// pages near the viewport); the previous layout's later pages stay visible until
    /// [`DocLayout::resume`] catches up.
    pub fn update_until(&mut self, doc: &Document, change: &Change, until_pages: Option<usize>) {
        self.until = until_pages;
        let ids = match change {
            Change::All => {
                self.layout(doc);
                return;
            }
            Change::Nodes(ids) => ids,
        };
        if self.state.snaps.is_empty() {
            self.layout(doc);
            return;
        }
        self.caches.invalidate(ids);
        // Ancestors (cells, rows, tables) of changed nodes are stale too.
        let blocks = flat_blocks(doc);
        let mut tops: Vec<NodeId> = Vec::new();
        for id in ids {
            let mut cur = *id;
            let mut guard = 0;
            while let Some((parent, _)) = doc.parent_of(cur) {
                guard += 1;
                if guard > 256 || doc.sections.iter().any(|s| s.id == parent) {
                    break;
                }
                self.caches.invalidate(&[parent]);
                cur = parent;
            }
            tops.push(cur);
        }
        // The first top-level block that changed (new blocks have no snapshot: start at the one
        // before them).
        let index: FxHashMap<NodeId, usize> = blocks
            .iter()
            .enumerate()
            .map(|(i, b)| (b.block.id(), i))
            .collect();
        let mut first = usize::MAX;
        let mut last = 0usize;
        for t in &tops {
            if let Some(&i) = index.get(t) {
                first = first.min(i);
                last = last.max(i);
            }
        }
        // Blocks whose snapshot no longer matches (deleted or inserted blocks).
        let mismatch = self
            .state
            .snaps
            .iter()
            .zip(&blocks)
            .position(|(s, b)| s.block != b.block.id())
            .unwrap_or(self.state.snaps.len().min(blocks.len()));
        if mismatch < first {
            first = mismatch;
        }
        if blocks.len() != self.state.snaps.len() && self.state.done {
            last = last.max(mismatch);
        }
        if first == usize::MAX {
            // Nothing in the body changed (headers, notes, comments): finish again.
            if ids.iter().any(|id| doc.story_of(*id).is_some()) {
                self.finished = false;
                self.hf_cache.clear();
                // Notes change page bottoms: flow everything again but reuse paragraphs.
                self.restart_at(doc, 0, usize::MAX);
            }
            return;
        }
        // Back up over the keep-with-next chain and one block (its spacing and widows).
        let mut start = first.min(self.state.snaps.len().saturating_sub(1));
        start = start.saturating_sub(1);
        while start > 0 {
            match blocks.get(start - 1).map(|b| b.block) {
                Some(kiln_doc::Block::Paragraph(p))
                    if kiln_doc::resolve::resolve_paragraph(&doc.context(), &p.props, None)
                        .keep_next =>
                {
                    start -= 1;
                }
                _ => break,
            }
        }
        self.restart_at(doc, start, last.max(first));
    }

    fn restart_at(&mut self, doc: &Document, start: usize, dirty_until: usize) {
        let old_snaps = self.state.snaps.clone();
        let old_pages = self.state.pages.clone();
        let old_next = self.state.next;
        if let Some(s) = self.state.snaps.get(start).cloned() {
            self.state.restore(&s, doc);
            self.state.snaps.truncate(start);
            self.state.next = start;
        } else {
            self.reset();
        }
        let _ = old_next;
        self.finished = false;
        self.flow_converging(doc, &old_snaps, &old_pages, dirty_until);
        if !self.state.done {
            let live = self.state.pages.len();
            self.stale = old_pages
                .get(live..)
                .map(<[Page]>::to_vec)
                .unwrap_or_default();
            self.stale_from = live;
            self.finish(doc);
        }
    }

    fn prepare_numbers(&mut self, doc: &Document) {
        self.labels = doc.list_labels();
        self.note_numbers = note_numbers(doc);
    }

    fn flow(
        &mut self,
        doc: &Document,
        until: Option<usize>,
        old: Option<&[crate::flow::Snap]>,
        dirty: usize,
    ) -> bool {
        self.prepare_numbers(doc);
        let blocks = flat_blocks(doc);
        let before = self.state.next;
        let met = {
            let mut cx = Cx {
                doc,
                caches: &mut self.caches,
                labels: &self.labels,
                note_numbers: &self.note_numbers,
                options: &self.options,
            };
            let mut flow = Flow {
                cx: &mut cx,
                st: &mut self.state,
            };
            flow.run(&blocks, until, old, dirty)
        };
        self.stats.blocks_placed = self.state.next.saturating_sub(before);
        self.stats.converged = met.is_some();
        self.stats.cached_paragraphs = self.caches.cached_paragraphs();
        self.finished = false;
        self.finish(doc);
        self.state.done
    }

    fn flow_converging(
        &mut self,
        doc: &Document,
        old_snaps: &[crate::flow::Snap],
        old_pages: &[Page],
        dirty_until: usize,
    ) {
        self.prepare_numbers(doc);
        let blocks = flat_blocks(doc);
        let before = self.state.next;
        let met = {
            let mut cx = Cx {
                doc,
                caches: &mut self.caches,
                labels: &self.labels,
                note_numbers: &self.note_numbers,
                options: &self.options,
            };
            let mut flow = Flow {
                cx: &mut cx,
                st: &mut self.state,
            };
            flow.run(&blocks, self.until, Some(old_snaps), dirty_until)
        };
        self.stats.blocks_placed = self.state.next.saturating_sub(before);
        self.stats.converged = met.is_some();
        if let Some(k) = met {
            // Splice the previous layout from block k on: same page, same place.
            let s = &old_snaps[k];
            let cur_page = self.state.pages.len();
            debug_assert_eq!(cur_page, s.pages);
            if let (Some(new_last), Some(old_same)) = (
                self.state.pages.last_mut(),
                old_pages.get(s.pages.saturating_sub(1)),
            ) {
                // The current page: keep what the new flow placed, then the old page's items
                // from the snapshot on.
                new_last
                    .items
                    .extend(old_same.items.iter().skip(s.items).cloned());
                new_last
                    .behind
                    .extend(old_same.behind.iter().skip(s.behind).cloned());
                new_last
                    .front
                    .extend(old_same.front.iter().skip(s.front).cloned());
                new_last
                    .obstacles
                    .extend(old_same.obstacles.iter().skip(s.obstacles).cloned());
                new_last.notes.clone_from(&old_same.notes);
                new_last.column_rules.clone_from(&old_same.column_rules);
            }
            self.state
                .pages
                .extend(old_pages.iter().skip(s.pages).cloned());
            self.state.snaps.extend(old_snaps.iter().skip(k).cloned());
            self.state.next = blocks.len();
            self.state.done = true;
        }
        self.stats.cached_paragraphs = self.caches.cached_paragraphs();
        self.finished = false;
        if self.state.done {
            self.finish(doc);
        }
    }

    /// Statistics of the last layout call.
    #[must_use]
    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// The pages.
    #[must_use]
    pub fn pages(&self) -> &[Page] {
        &self.state.pages
    }

    /// Lines of a page (all stories) in page coordinates.
    #[must_use]
    pub fn page_lines(&self, page: usize) -> &[LineItem] {
        self.page_lines.get(page).map_or(&[], Vec::as_slice)
    }

    /// Body lines in reading order.
    #[must_use]
    pub fn reading_order(&self) -> &[LineRef] {
        &self.order
    }

    /// Lines of a paragraph.
    #[must_use]
    pub fn lines_of(&self, para: NodeId) -> &[LineRef] {
        self.para_lines.get(&para).map_or(&[], Vec::as_slice)
    }

    /// The line at a reference.
    #[must_use]
    pub fn line(&self, r: LineRef) -> Option<&LineItem> {
        self.page_lines.get(r.page as usize)?.get(r.index as usize)
    }

    /// The reading-order rank of a paragraph.
    #[must_use]
    pub fn rank(&self, para: NodeId) -> Option<usize> {
        self.para_rank.get(&para).copied()
    }

    /// The size of the whole surface in document px.
    #[must_use]
    pub fn extent(&self) -> Bounds {
        let mut b = Bounds::new(0.0, 0.0, 1.0, 1.0);
        for i in 0..self.display_len() {
            if let Some(p) = self.display_page(i) {
                b = b.union(&p.rect());
            }
        }
        match self.state.view() {
            View::Print => Bounds::new(
                0.0,
                0.0,
                b.right() + SIDE_PAD + self.balloon_room(),
                b.bottom() + BOTTOM_PAD,
            ),
            View::Read { .. } => Bounds::new(0.0, 0.0, b.right() + PAGE_GAP, b.bottom() + PAGE_GAP),
            View::Web { .. } | View::Draft => {
                let bottom = self.content_bottom();
                Bounds::new(0.0, 0.0, b.right(), bottom + PAGE_GAP)
            }
        }
    }

    fn balloon_room(&self) -> f32 {
        if self.state.pages.iter().any(|p| !p.balloons.is_empty()) {
            BALLOON_W + 24.0
        } else {
            0.0
        }
    }

    /// Bottom of the content of an endless page (web and draft views).
    fn content_bottom(&self) -> f32 {
        let Some(p) = self.state.pages.first() else {
            return 0.0;
        };
        let b = p
            .items
            .iter()
            .chain(&p.front)
            .chain(&p.behind)
            .map(|i| i.bounds().bottom())
            .fold(p.body.y, f32::max);
        p.origin.1 + b + 40.0
    }

    /// Pages shown: the laid-out ones, then stale pages of the previous layout not yet
    /// re-flowed.
    #[must_use]
    pub fn display_len(&self) -> usize {
        let live = self.state.pages.len();
        let skip = live.saturating_sub(self.stale_from);
        live + self.stale.len().saturating_sub(skip)
    }

    /// A shown page (see [`DocLayout::display_len`]).
    #[must_use]
    pub fn display_page(&self, i: usize) -> Option<&Page> {
        let live = self.state.pages.len();
        if i < live {
            return self.state.pages.get(i);
        }
        let skip = live.saturating_sub(self.stale_from);
        self.stale.get(skip + (i - live))
    }

    /// The rectangle of page `i` in document coordinates (web and draft views: the content).
    #[must_use]
    pub fn page_rect(&self, i: usize) -> Option<Bounds> {
        let p = self.display_page(i)?;
        Some(match self.state.view() {
            View::Web { .. } | View::Draft => Bounds::new(
                p.origin.0,
                p.origin.1,
                p.width,
                self.content_bottom() - p.origin.1,
            ),
            _ => p.rect(),
        })
    }

    /// Places headers, footers, balloons and page positions, and indexes lines.
    fn finish(&mut self, doc: &Document) {
        if self.finished {
            return;
        }
        if self.state.done {
            self.stale.clear();
        }
        let total = i64::try_from(self.display_len()).unwrap_or(i64::MAX);
        let mut section_pages: FxHashMap<usize, i64> = FxHashMap::default();
        for p in &self.state.pages {
            *section_pages.entry(p.section).or_default() += 1;
        }
        if self.state.view().paged() {
            for i in 0..self.state.pages.len() {
                self.place_headers(doc, i, total, &section_pages);
            }
            self.place_balloons(doc);
        }
        self.place_origins();
        self.index_pages();
        self.finished = self.state.done;
    }

    fn place_origins(&mut self) {
        let view = self.state.view();
        let max_w = self
            .state
            .pages
            .iter()
            .map(|p| p.width)
            .fold(0.0_f32, f32::max);
        let mut y = PAGE_GAP;
        let mut x = PAGE_GAP;
        let live = self.state.pages.len();
        let skip = live.saturating_sub(self.stale_from);
        for p in self
            .state
            .pages
            .iter_mut()
            .chain(self.stale.iter_mut().skip(skip))
        {
            match view {
                View::Print => {
                    p.origin = (SIDE_PAD + (max_w - p.width) / 2.0, y);
                    y += p.height + PAGE_GAP;
                }
                View::Read { .. } => {
                    p.origin = (x, PAGE_GAP);
                    x += p.width + PAGE_GAP;
                }
                View::Web { .. } | View::Draft => {
                    p.origin = (0.0, 0.0);
                }
            }
        }
    }

    fn place_headers(
        &mut self,
        doc: &Document,
        i: usize,
        total: i64,
        section_pages: &FxHashMap<usize, i64>,
    ) {
        let (section, number, number_text, first, width, height, blank) = {
            let p = &self.state.pages[i];
            (
                p.section,
                p.number,
                p.number_text.clone(),
                p.index_in_section == 0,
                p.width,
                p.height,
                p.blank,
            )
        };
        let Some(sec) = doc.sections.get(section) else {
            return;
        };
        let geom = crate::flow::SectGeom::new(&sec.props, section, self.state.view());
        let env = FieldEnv {
            page: Some(number_text.clone()),
            page_number: Some(number),
            num_pages: Some(total),
            section_pages: section_pages.get(&section).copied(),
            section: i64::try_from(section + 1).ok(),
            now: self.options.now,
            file_name: self.options.file_name.clone(),
        };
        let kind = sec
            .props
            .header_kind_for_page(doc.settings.even_and_odd_headers, first, number);
        let mut parts: Vec<(bool, Vec<Item>)> = Vec::new();
        for footer in [false, true] {
            let refs: Vec<_> = doc
                .sections
                .iter()
                .map(|s| if footer { s.footers } else { s.headers })
                .collect();
            let Some(id) = resolve_header_footer(&refs, section, kind) else {
                continue;
            };
            let Some(hf) = doc.headers_footers.iter().find(|h| h.id == id) else {
                continue;
            };
            if blank {
                continue;
            }
            let key = {
                use std::hash::{BuildHasher, BuildHasherDefault};
                BuildHasherDefault::<rustc_hash::FxHasher>::default().hash_one((
                    &env.page,
                    env.num_pages,
                    env.section_pages,
                    geom.text_width().to_bits(),
                ))
            };
            let content = if let Some(c) = self.hf_cache.get(&(id, key)) {
                c.clone()
            } else {
                let mut cx = Cx {
                    doc,
                    caches: &mut self.caches,
                    labels: &self.labels,
                    note_numbers: &self.note_numbers,
                    options: &self.options,
                };
                let b = Arc::new(cx.blocks_box(
                    &hf.blocks,
                    geom.text_width(),
                    None,
                    StoryKind::HeaderFooter(id),
                    &env,
                ));
                self.hf_cache.insert((id, key), b.clone());
                b
            };
            let y = if footer {
                height - geom.footer - content.height
            } else {
                geom.header
            };
            let mut items = Vec::new();
            for band in &content.bands {
                for it in &band.items {
                    let mut it = it.clone();
                    it.translate(geom.margins[3], y);
                    items.push(it);
                }
            }
            // Floating pictures in headers (logos, watermarks) are positioned on the page.
            let frame = crate::drawing::PageFrame {
                width,
                height,
                top: geom.margins[0],
                bottom: geom.margins[2],
                left: geom.margins[3],
                right: geom.margins[1],
                column: (geom.margins[3], geom.text_width()),
            };
            for (top, d) in &content.anchors {
                if let Some(a) = crate::drawing::anchor_of(&d.drawing) {
                    let at = crate::drawing::AnchorPoint {
                        para_top: y + top,
                        line_top: y + top,
                        char_x: geom.margins[3],
                    };
                    let mut d = d.clone();
                    d.rect = crate::drawing::float_rect(&d.drawing, a, &frame, &at);
                    if a.wrap == kiln_doc::st::WrapType::Behind {
                        d.layer = crate::page::Layer::Behind;
                    }
                    items.insert(0, Item::Drawing(d));
                }
            }
            parts.push((footer, items));
        }
        let page = &mut self.state.pages[i];
        page.header.clear();
        page.footer.clear();
        for (footer, items) in parts {
            if footer {
                page.footer = items;
            } else {
                page.header = items;
            }
        }
    }

    /// Comment balloons: one per comment, at the height of its anchor, stacked so they do not
    /// overlap, in the markup area right of the page.
    fn place_balloons(&mut self, doc: &Document) {
        if doc.comments.is_empty() || !self.options.markup {
            return;
        }
        // Anchor points: the start of each comment's range.
        self.index_pages();
        let mut anchors: Vec<(NodeId, usize, (f32, f32))> = Vec::new();
        for c in &doc.comments {
            if c.meta.parent.is_some() {
                continue;
            }
            let found = doc.all_paragraphs().into_iter().find_map(|(_, p)| {
                p.content
                    .marks()
                    .iter()
                    .find(|m| m.mark == Mark::Comment(c.id))
                    .map(|m| kiln_doc::Pos::new(p.id, m.start))
            });
            let Some(pos) = found else { continue };
            if let Some((page, x, y)) = self.caret_page_point(pos) {
                anchors.push((c.id, page, (x, y)));
            }
        }
        let mut next_free: FxHashMap<usize, f32> = FxHashMap::default();
        for (id, page, (x, y)) in anchors {
            let Some(p) = self.state.pages.get_mut(page) else {
                continue;
            };
            let h = 56.0;
            let top = y.max(next_free.get(&page).copied().unwrap_or(0.0));
            next_free.insert(page, top + h + 8.0);
            p.balloons.push(Balloon {
                comment: id,
                anchor: (x, y),
                rect: Bounds::new(p.width + 16.0, top, BALLOON_W, h),
            });
        }
    }

    /// The page and page-local point of a body caret position (before page origins).
    fn caret_page_point(&self, pos: kiln_doc::Pos) -> Option<(usize, f32, f32)> {
        let g = crate::geometry::caret_in(self, pos)?;
        let page = g.page;
        let o = self.state.pages.get(page)?.origin;
        Some((page, g.rect.x - o.0, g.rect.y - o.1))
    }

    fn index_pages(&mut self) {
        self.page_lines.clear();
        self.para_lines.clear();
        self.order.clear();
        self.para_rank.clear();
        for (pi, p) in self.state.pages.iter().enumerate() {
            let mut lines = Vec::new();
            let body: Vec<(&Item, f32)> = p.items.iter().map(|i| (i, p.body_dy)).collect();
            for (it, dy) in p
                .header
                .iter()
                .chain(&p.behind)
                .map(|i| (i, 0.0))
                .chain(body)
                .chain(
                    p.front
                        .iter()
                        .chain(&p.notes)
                        .chain(&p.footer)
                        .map(|i| (i, 0.0)),
                )
            {
                if let Item::Line(l) = it {
                    let mut l = l.clone();
                    l.y += dy;
                    let r = LineRef {
                        page: u32::try_from(pi).unwrap_or(u32::MAX),
                        index: u32::try_from(lines.len()).unwrap_or(u32::MAX),
                    };
                    let id = l.para.id();
                    self.para_lines.entry(id).or_default().push(r);
                    if l.story == StoryKind::Body {
                        self.order.push(r);
                        let n = self.para_rank.len();
                        self.para_rank.entry(id).or_insert(n);
                    }
                    lines.push(l);
                }
            }
            self.page_lines.push(lines);
        }
        // Body lines in reading order: pages in order, items in placement order, which the
        // walk above follows except that headers come first; sort by (page, placement).
    }

    /// Page origins are in document coordinates: a page-local point to document coordinates.
    #[must_use]
    pub fn to_doc(&self, page: usize, x: f32, y: f32) -> (f32, f32) {
        let o = self.state.pages.get(page).map_or((0.0, 0.0), |p| p.origin);
        (x + o.0, y + o.1)
    }

    /// The page under (or nearest to) a document point.
    #[must_use]
    pub fn page_at(&self, x: f32, y: f32) -> Option<usize> {
        let pages = &self.state.pages;
        if pages.is_empty() || !(x.is_finite() && y.is_finite()) {
            return None;
        }
        let mut best = (f32::INFINITY, 0usize);
        for (i, _) in pages.iter().enumerate() {
            let r = self.page_rect(i)?;
            let d = r.distance_to(x, y);
            if d < best.0 {
                best = (d, i);
            }
            if d == 0.0 {
                break;
            }
        }
        Some(best.1)
    }
}
