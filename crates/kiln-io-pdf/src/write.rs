// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write documents as [`PrintablePages`]: the pages of a `kiln-doc-layout` layout, painted
//! the way `kiln_doc_layout::paint` paints them on screen (same order, same rules, same shapes
//! and pictures) but with text as [`TextRun`]s, automatic colours as printed black, nothing
//! that only belongs on screen, and every piece of content marked against a structure tree
//! built from the model:
//!
//! - paragraphs → `P`, or `H1`–`H6` from their outline level (heading styles), with the
//!   heading's text as its title; a table of contents → `TOC` / `TOCI`;
//! - tables → `Table` / `TR` / `TH` (header rows) or `TD` with row and column spans;
//! - pictures, shapes and charts → `Figure` with their alt text (decorative ones are
//!   artifacts); text boxes → `Div`; footnotes and endnotes → `Note` at their reference;
//! - hyperlinks (`w:hyperlink` and `HYPERLINK` fields, TOC entries included) → `Link` with a
//!   link annotation per line: external URLs open, bookmarks jump to their page;
//! - headers and footers → pagination artifacts; shading, borders and rules → layout
//!   artifacts.
//!
//! [`print_document`] applies the print-only settings that change the content (hidden text,
//! orientation and margin overrides); [`print_layout`] lays a document out for print.

use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::Arc;

use kiln_common::NodeId;
use kiln_doc::st::{NumberFormat, Orientation};
use kiln_doc::{Block, Document, DrawingKind, FieldKind, InlineObject, Mark as DocMark};
use kiln_doc_layout::DocLayout;
use kiln_doc_layout::drawing::shape_visual;
use kiln_doc_layout::page::{DrawingItem, Item, LineItem, Page, StoryKind};
use kiln_doc_layout::style::{Rule, RuleKind};
use kiln_scene::{Bounds, DrawOp, FillRule, ImageId, Paint, PathData, Stroke};
use kiln_text::{FontStore, LineEnd};
use rustc_hash::FxHashMap;

use crate::printable::{
    ArtifactKind, DocInfo, EncodedImage, ImageFormat, LabelStyle, LinkTarget, Mark, OutlineEntry,
    PageCanvas, PageInfo, PageLabel, PrintImage, PrintablePages, StructId, StructKind, StructTree,
    TextRun,
};

/// Printed "automatic" text: black, whatever the screen theme.
const AUTO_INK: u32 = 0x0000_00FF;
/// Structure deeper than this (pathological nested tables) is flattened into the last level.
const MAX_DEPTH: usize = 48;
/// The model is not walked deeper than this (the walk recurses).
const STACK_DEPTH: usize = 400;

/// Print settings that change what is laid out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WriteSettings {
    /// Print tracked changes as markup (insertions underlined, deletions struck through);
    /// `false` prints the document as if every change were accepted.
    pub markup: bool,
    /// Print hidden text (`w:vanish`).
    pub hidden_text: bool,
    /// Print in this orientation (the pages turn; margins follow the page).
    pub orientation: Option<PrintOrientation>,
    /// Print with these margins, twips (top, right, bottom, left).
    pub margins: Option<[i32; 4]>,
}

/// Page orientation for printing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrintOrientation {
    /// Taller than wide.
    Portrait,
    /// Wider than tall.
    Landscape,
}

/// The document to lay out for print: `doc` itself, or a copy with hidden text shown and the
/// page setup overridden.
#[must_use]
pub fn print_document<'a>(doc: &'a Document, settings: &WriteSettings) -> Cow<'a, Document> {
    if !settings.hidden_text && settings.orientation.is_none() && settings.margins.is_none() {
        return Cow::Borrowed(doc);
    }
    let mut d = doc.clone();
    if settings.hidden_text {
        show_hidden_text(&mut d);
    }
    for s in &mut d.sections {
        let size = &mut s.props.page_size;
        if let Some(o) = settings.orientation {
            let landscape = o == PrintOrientation::Landscape;
            let (short, long) = (size.width.min(size.height), size.width.max(size.height));
            let was_landscape = size.width > size.height;
            (size.width, size.height) = if landscape {
                (long, short)
            } else {
                (short, long)
            };
            size.orientation = if landscape {
                Orientation::Landscape
            } else {
                Orientation::Portrait
            };
            if was_landscape != landscape {
                // Margins turn with the page, as in Word.
                let m = &mut s.props.margins;
                (m.top, m.right, m.bottom, m.left) = if landscape {
                    (m.left, m.top, m.right, m.bottom)
                } else {
                    (m.right, m.bottom, m.left, m.top)
                };
            }
        }
        if let Some([t, r, b, l]) = settings.margins {
            let m = &mut s.props.margins;
            (m.top, m.right, m.bottom, m.left) = (t, r, b, l);
        }
    }
    Cow::Owned(d)
}

fn show_hidden_text(d: &mut Document) {
    fn blocks(list: &mut [Block]) {
        for b in list {
            match b {
                Block::Paragraph(p) => {
                    if p.mark_props.hidden == Some(true) {
                        p.mark_props.hidden = Some(false);
                    }
                    if p.content.runs().iter().any(|r| r.props.hidden.is_some()) {
                        let runs: Vec<_> = p
                            .content
                            .runs()
                            .iter()
                            .cloned()
                            .map(|mut r| {
                                r.props.hidden = Some(false);
                                r
                            })
                            .collect();
                        // The spans cover the paragraph exactly; this cannot fail.
                        let _ = p.content.set_runs(0, runs);
                    }
                }
                Block::Table(t) => {
                    for row in &mut t.rows {
                        for cell in &mut row.cells {
                            blocks(&mut cell.blocks);
                        }
                    }
                }
                Block::Sdt(s) => blocks(&mut s.blocks),
                Block::Field(f) => blocks(&mut f.blocks),
                Block::Opaque(_) => {}
            }
        }
    }
    for s in &mut d.sections {
        blocks(&mut s.blocks);
    }
    for h in &mut d.headers_footers {
        blocks(&mut h.blocks);
    }
    for n in &mut d.notes {
        blocks(&mut n.blocks);
    }
    let mut defaults = d.styles.doc_defaults.clone();
    defaults.rpr.hidden = None;
    let styles: Vec<_> = d
        .styles
        .iter()
        .cloned()
        .map(|mut s| {
            if let Some(r) = &mut s.rpr {
                r.hidden = None;
            }
            s
        })
        .collect();
    d.styles = kiln_doc::StyleSheet::new(defaults, styles);
}

/// Lays `doc` out completely for print over `fonts`, with tracked changes as markup or not.
#[must_use]
pub fn print_layout(
    doc: &Document,
    fonts: FontStore,
    markup: bool,
    file_name: Option<String>,
) -> DocLayout {
    let mut layout = DocLayout::new(fonts);
    let mut options = layout.options().clone();
    options.markup = markup;
    options.file_name = file_name;
    layout.set_options(options);
    layout.layout(doc);
    layout
}

/// A link inside a paragraph: its char range, where it goes, and its structure node.
#[derive(Debug, Clone)]
struct LinkSpan {
    start: usize,
    end: usize,
    target: LinkGoal,
    node: StructId,
}

#[derive(Debug, Clone)]
enum LinkGoal {
    Uri(String),
    Bookmark(String),
}

/// A Write document ready to print.
#[derive(Debug)]
pub struct WritePrint<'a> {
    doc: &'a Document,
    layout: &'a DocLayout,
    title: Option<String>,
    tree: StructTree,
    paras: FxHashMap<NodeId, StructId>,
    figures: FxHashMap<NodeId, StructId>,
    links: FxHashMap<NodeId, Vec<LinkSpan>>,
    headings: Vec<(NodeId, u8, String)>,
    bookmarks: FxHashMap<String, NodeId>,
    media: FxHashMap<String, ImageId>,
    keys: Vec<String>,
    images: RefCell<FxHashMap<ImageId, Option<Arc<PrintImage>>>>,
    expanded: rustc_hash::FxHashSet<NodeId>,
}

impl<'a> WritePrint<'a> {
    /// Prepares `doc` laid out in `layout` (print view, complete) for printing. `title` is the
    /// title used when the document has none in its properties (usually the file name).
    #[must_use]
    pub fn new(doc: &'a Document, layout: &'a DocLayout, title: Option<String>) -> Self {
        let keys: Vec<String> = doc.media.keys().cloned().collect();
        let media = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.clone(), ImageId(u32::try_from(i + 1).unwrap_or(u32::MAX))))
            .collect();
        let mut w = Self {
            doc,
            layout,
            title,
            tree: StructTree::default(),
            paras: FxHashMap::default(),
            figures: FxHashMap::default(),
            links: FxHashMap::default(),
            headings: Vec::new(),
            bookmarks: FxHashMap::default(),
            media,
            keys,
            images: RefCell::new(FxHashMap::default()),
            expanded: rustc_hash::FxHashSet::default(),
        };
        let notes: FxHashMap<NodeId, &kiln_doc::model::Note> =
            doc.notes.iter().map(|n| (n.id, n)).collect();
        for s in &doc.sections {
            w.blocks(&s.blocks, None, 0, &notes, false);
        }
        w
    }

    fn blocks(
        &mut self,
        blocks: &[Block],
        parent: Option<StructId>,
        depth: usize,
        notes: &FxHashMap<NodeId, &kiln_doc::model::Note>,
        toc: bool,
    ) {
        if depth > STACK_DEPTH {
            // Content this deep (hostile nesting) prints, untagged.
            return;
        }
        let deeper = |kind: StructKind, tree: &mut StructTree| {
            if depth >= MAX_DEPTH {
                parent
            } else {
                Some(tree.add(kind, parent))
            }
        };
        for b in blocks {
            match b {
                Block::Paragraph(p) => self.paragraph(p, parent, depth, notes, toc),
                Block::Table(t) => {
                    let Some(table) = deeper(StructKind::Table, &mut self.tree) else {
                        continue;
                    };
                    let map = t.cell_map();
                    for (r, row) in t.rows.iter().enumerate() {
                        let tr = if depth >= MAX_DEPTH {
                            Some(table)
                        } else {
                            Some(self.tree.add(StructKind::TableRow, Some(table)))
                        };
                        let header = row.props.header == Some(true);
                        for slot in map.iter().filter(|s| s.row == r && s.row_span > 0) {
                            let Some(cell) = row.cells.get(slot.cell) else {
                                continue;
                            };
                            let rows = u32::try_from(slot.row_span).unwrap_or(1);
                            let cols = u32::try_from(slot.col_span).unwrap_or(1);
                            let kind = if header {
                                StructKind::TableHeader { rows, cols }
                            } else {
                                StructKind::TableCell { rows, cols }
                            };
                            let td = if depth >= MAX_DEPTH {
                                tr
                            } else {
                                Some(self.tree.add(kind, tr))
                            };
                            self.blocks(&cell.blocks, td, depth + 3, notes, false);
                        }
                    }
                }
                Block::Sdt(s) => self.blocks(&s.blocks, parent, depth, notes, toc),
                Block::Field(f) => {
                    let is_toc = matches!(f.field.parsed().kind, FieldKind::Toc(_));
                    if is_toc {
                        let node = deeper(StructKind::Toc, &mut self.tree);
                        self.blocks(&f.blocks, node, depth + 1, notes, true);
                    } else {
                        self.blocks(&f.blocks, parent, depth, notes, toc);
                    }
                }
                Block::Opaque(_) => {}
            }
        }
    }

    fn paragraph(
        &mut self,
        p: &kiln_doc::Paragraph,
        parent: Option<StructId>,
        depth: usize,
        notes: &FxHashMap<NodeId, &kiln_doc::model::Note>,
        toc: bool,
    ) {
        let resolved = self.doc.resolve_paragraph(p);
        let text = p.content.plain_text(true);
        let parent = if toc && depth < MAX_DEPTH {
            Some(self.tree.add(StructKind::TocItem, parent))
        } else {
            parent
        };
        let kind = if resolved.outline_level < 9 {
            let level = resolved.outline_level + 1;
            self.headings.push((p.id, level, text.trim().to_owned()));
            if level <= 6 && !toc {
                StructKind::Heading {
                    level,
                    title: text.trim().to_owned(),
                }
            } else {
                StructKind::P
            }
        } else {
            StructKind::P
        };
        let node = if depth >= MAX_DEPTH {
            parent.unwrap_or_else(|| self.tree.add(StructKind::P, None))
        } else {
            self.tree.add(kind, parent)
        };
        self.paras.insert(p.id, node);
        // Hyperlinks: marks and HYPERLINK fields.
        let mut links = Vec::new();
        for m in p.content.marks() {
            if let DocMark::Link(h) = &m.mark {
                let target = match &h.target {
                    kiln_doc::LinkTarget::Url(u) => LinkGoal::Uri(u.clone()),
                    kiln_doc::LinkTarget::Bookmark(b) => LinkGoal::Bookmark(b.clone()),
                };
                let node = self.tree.add(StructKind::Link, Some(node));
                links.push(LinkSpan {
                    start: m.start,
                    end: m.end,
                    target,
                    node,
                });
            }
        }
        for (offset, obj) in p.content.objects_with_offsets() {
            match obj {
                InlineObject::Field(f) => {
                    if let FieldKind::Hyperlink(h) = f.parsed().kind {
                        let target = match (h.url, h.anchor) {
                            (Some(u), Some(a)) if !u.is_empty() => {
                                Some(LinkGoal::Uri(format!("{u}#{a}")))
                            }
                            (Some(u), _) if !u.is_empty() => Some(LinkGoal::Uri(u)),
                            (_, Some(a)) => Some(LinkGoal::Bookmark(a)),
                            _ => None,
                        };
                        if let Some(target) = target {
                            let node = self.tree.add(StructKind::Link, Some(node));
                            links.push(LinkSpan {
                                start: offset,
                                end: offset + 1,
                                target,
                                node,
                            });
                        }
                    }
                }
                InlineObject::Drawing(d) => {
                    // Text boxes are text, not figures: their content is a `Div`.
                    if !d.decorative && d.text_box.is_none() {
                        let alt = d
                            .description
                            .clone()
                            .or_else(|| d.title.clone())
                            .filter(|a| !a.trim().is_empty());
                        let fig = self.tree.add(StructKind::Figure { alt }, Some(node));
                        self.figures.insert(d.id, fig);
                    }
                    if let Some(tb) = &d.text_box {
                        let div = if depth + 1 >= MAX_DEPTH {
                            parent
                        } else {
                            Some(self.tree.add(StructKind::Div, parent))
                        };
                        self.blocks(tb, div, depth + 1, notes, false);
                    }
                }
                InlineObject::NoteRef(r) => {
                    // A note's own text starts with a reference to itself: expand each note once.
                    if let Some(n) = notes.get(&r.note)
                        && self.expanded.insert(r.note)
                        && depth + 1 < MAX_DEPTH
                    {
                        let note = self.tree.add(StructKind::Note, Some(node));
                        self.blocks(&n.blocks, Some(note), depth + 1, notes, false);
                    }
                }
                InlineObject::BookmarkStart { name } => {
                    self.bookmarks.entry(name.clone()).or_insert(p.id);
                }
                _ => {}
            }
        }
        if !links.is_empty() {
            self.links.insert(p.id, links);
        }
    }

    /// The first place a paragraph is printed: page and y.
    fn place_of(&self, para: NodeId) -> Option<(usize, f32)> {
        let r = *self.layout.lines_of(para).first()?;
        let line = self.layout.line(r)?;
        Some((r.page as usize, line.rect().y))
    }

    fn goal(&self, g: &LinkGoal) -> Option<LinkTarget> {
        match g {
            LinkGoal::Uri(u) => Some(LinkTarget::Uri(u.clone())),
            LinkGoal::Bookmark(b) => {
                let para = self.bookmarks.get(b)?;
                let (page, y) = self.place_of(*para)?;
                Some(LinkTarget::Page { page, y })
            }
        }
    }

    fn decode(&self, id: ImageId) -> Option<Arc<PrintImage>> {
        let key = self.keys.get((id.0 as usize).checked_sub(1)?)?;
        let media = self.doc.media.get(key)?;
        let pic = kiln_doc_layout::media::decode(&media.data, id.0)?;
        let source = ImageFormat::sniff(&media.data).map(|format| EncodedImage {
            format,
            data: Arc::new(media.data.clone()),
        });
        Some(Arc::new(PrintImage {
            id,
            width: pic.width,
            height: pic.height,
            rgba: Arc::new(pic.rgba),
            source,
        }))
    }
}

fn label_style(f: &NumberFormat) -> Option<LabelStyle> {
    Some(match f {
        NumberFormat::Decimal => LabelStyle::Decimal,
        NumberFormat::LowerRoman => LabelStyle::LowerRoman,
        NumberFormat::UpperRoman => LabelStyle::UpperRoman,
        NumberFormat::LowerLetter => LabelStyle::LowerLetter,
        NumberFormat::UpperLetter => LabelStyle::UpperLetter,
        _ => return None,
    })
}

impl PrintablePages for WritePrint<'_> {
    fn pages(&self) -> Vec<PageInfo> {
        self.layout
            .pages()
            .iter()
            .map(|p| {
                let format = self
                    .doc
                    .sections
                    .get(p.section)
                    .and_then(|s| s.props.page_numbering.as_ref())
                    .and_then(|n| n.format.as_ref());
                let number = u32::try_from(p.number.max(1)).unwrap_or(1);
                let label = match format.map_or(Some(LabelStyle::Decimal), label_style) {
                    Some(style) => PageLabel {
                        style,
                        prefix: String::new(),
                        number,
                    },
                    // Formats PDF cannot count in: the number as Word shows it.
                    None => PageLabel {
                        style: LabelStyle::None,
                        prefix: p.number_text.clone(),
                        number,
                    },
                };
                PageInfo {
                    width: p.width,
                    height: p.height,
                    label,
                }
            })
            .collect()
    }

    fn info(&self) -> DocInfo {
        let props = &self.doc.properties;
        let split = |s: &Option<String>| -> Vec<String> {
            s.as_deref()
                .map(|v| {
                    v.split([';', ','])
                        .map(str::trim)
                        .filter(|k| !k.is_empty())
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default()
        };
        DocInfo {
            title: props
                .title
                .clone()
                .filter(|t| !t.trim().is_empty())
                .or_else(|| self.title.clone()),
            authors: split(&props.creator),
            subject: props.subject.clone().or_else(|| props.description.clone()),
            keywords: split(&props.keywords),
            language: props.language.clone().filter(|l| !l.is_empty()),
            creator: Some("Kiln Write".into()),
            created: props.created.clone(),
            rtl: false,
        }
    }

    fn structure(&self) -> StructTree {
        self.tree.clone()
    }

    fn outline(&self) -> Vec<OutlineEntry> {
        self.headings
            .iter()
            .filter(|(_, _, t)| !t.is_empty())
            .filter_map(|(id, level, title)| {
                let (page, y) = self.place_of(*id)?;
                Some(OutlineEntry {
                    level: *level,
                    title: title.clone(),
                    page,
                    y,
                })
            })
            .collect()
    }

    fn image(&self, id: ImageId) -> Option<Arc<PrintImage>> {
        self.images
            .borrow_mut()
            .entry(id)
            .or_insert_with(|| self.decode(id))
            .clone()
    }

    fn paint(&self, index: usize, canvas: &mut dyn PageCanvas) {
        let Some(page) = self.layout.pages().get(index) else {
            return;
        };
        let mut p = Painter {
            w: self,
            canvas,
            fonts: self.layout.engine().fonts(),
            mark: None,
        };
        p.page(page);
    }
}

/// Paints one page.
struct Painter<'a, 'b> {
    w: &'a WritePrint<'a>,
    canvas: &'b mut dyn PageCanvas,
    fonts: &'a FontStore,
    mark: Option<Mark>,
}

impl Painter<'_, '_> {
    fn mark(&mut self, m: Mark) {
        if self.mark != Some(m) {
            self.canvas.begin(m);
            self.mark = Some(m);
        }
    }

    fn page(&mut self, page: &Page) {
        for it in &page.behind {
            self.item(it, 0.0, None);
        }
        for it in &page.header {
            self.item(it, 0.0, Some(ArtifactKind::Header));
        }
        for it in &page.footer {
            self.item(it, 0.0, Some(ArtifactKind::Footer));
        }
        for it in &page.items {
            self.item(it, page.body_dy, None);
        }
        for it in &page.notes {
            self.item(it, 0.0, None);
        }
        for &(x, y0, y1) in &page.column_rules {
            let rule = Rule {
                width: 0.75,
                color: None,
                kind: RuleKind::Single,
                space: 0.0,
                weight: 0.0,
            };
            self.mark(Mark::Artifact(ArtifactKind::Layout));
            self.rule(x, y0, x, y1, &rule);
        }
        for it in &page.front {
            self.item(it, 0.0, None);
        }
        if self.mark.take().is_some() {
            self.canvas.end();
        }
    }

    fn item(&mut self, it: &Item, dy: f32, artifact: Option<ArtifactKind>) {
        match it {
            Item::Line(l) => {
                let mut l = l.clone();
                l.y += dy;
                self.line(&l, artifact);
            }
            Item::Fill { rect, color } => {
                self.mark(Mark::Artifact(artifact.unwrap_or(ArtifactKind::Layout)));
                self.canvas.op(&DrawOp::FillRect {
                    rect: rect.translate(0.0, dy),
                    paint: Paint::Rgba(*color),
                    radius: 0.0,
                });
            }
            Item::Rule {
                x0,
                y0,
                x1,
                y1,
                rule,
            } => {
                self.mark(Mark::Artifact(artifact.unwrap_or(ArtifactKind::Layout)));
                self.rule(*x0, *y0 + dy, *x1, *y1 + dy, rule);
            }
            Item::Drawing(d) => {
                let mut d = d.clone();
                d.rect = d.rect.translate(0.0, dy);
                let m = match (artifact, self.w.figures.get(&d.drawing.id)) {
                    (Some(a), _) => Mark::Artifact(a),
                    (None, Some(f)) => Mark::Content(*f),
                    (None, None) => Mark::Artifact(ArtifactKind::Layout),
                };
                self.mark(m);
                self.drawing(&d);
            }
        }
    }

    fn line(&mut self, l: &LineItem, artifact: Option<ArtifactKind>) {
        let Some(info) = l.para.lines.get(l.line as usize) else {
            return;
        };
        let Some(piece) = l.para.pieces.get(info.piece as usize) else {
            return;
        };
        let Some(line) = piece.layout.lines.get(info.line as usize) else {
            return;
        };
        let src = &l.para.src;
        let para = src.id;
        let text = piece.layout.text();
        let origin = (l.x + piece.x, l.y + piece.y);
        let base_mark = match (artifact, l.story) {
            (Some(a), _) => Mark::Artifact(a),
            (None, StoryKind::HeaderFooter(_)) => Mark::Artifact(ArtifactKind::Header),
            (None, _) => self
                .w
                .paras
                .get(&para)
                .map_or(Mark::Artifact(ArtifactKind::Layout), |n| Mark::Content(*n)),
        };
        // Link byte ranges in this piece's text.
        let links: Vec<(std::ops::Range<usize>, &LinkSpan)> = if artifact.is_none() {
            self.w
                .links
                .get(&para)
                .map(|spans| {
                    spans
                        .iter()
                        .map(|s| {
                            let r = src.map.byte_range(s.start, s.end);
                            (
                                r.start.saturating_sub(piece.byte_start)
                                    ..r.end.saturating_sub(piece.byte_start),
                                s,
                            )
                        })
                        .filter(|(r, _)| !r.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut link_rects: Vec<(usize, Bounds)> = Vec::new();
        let last_run = line.runs.len().saturating_sub(1);
        for (ri, run) in line.runs.iter().enumerate() {
            let synthetic = run.span == usize::MAX;
            let span = piece.span_base.saturating_add(run.span);
            if !synthetic && src.hidden_spans.get(span).copied().unwrap_or(false) {
                continue;
            }
            let auto = !synthetic && src.ink_spans.get(span).copied().unwrap_or(false);
            let paint = match (auto, l.fill) {
                (true, Some(bg)) => Paint::Rgba(kiln_doc_layout::paint::contrast(bg)),
                (true, None) => Paint::Rgba(AUTO_INK),
                _ => Paint::Rgba(run.color),
            };
            let actual = if synthetic && run.text_range.is_empty() {
                if line.end == LineEnd::Hyphen && ri == last_run {
                    Some("\u{AD}")
                } else {
                    Some("")
                }
            } else {
                None
            };
            // Split the run where links start and end.
            let link_of = |g: &kiln_text::PositionedGlyph| {
                let c = g.cluster as usize;
                links.iter().position(|(r, _)| r.contains(&c))
            };
            let mut start = 0;
            while start < run.glyphs.len() {
                let k = link_of(&run.glyphs[start]);
                let mut end = start + 1;
                while end < run.glyphs.len() && link_of(&run.glyphs[end]) == k {
                    end += 1;
                }
                let mark = match k {
                    Some(i) => Mark::Content(links[i].1.node),
                    None => base_mark,
                };
                self.mark(mark);
                self.canvas.text(&TextRun {
                    fonts: self.fonts,
                    run,
                    glyphs: start..end,
                    origin,
                    text,
                    paint,
                    actual,
                });
                if let Some(i) = k {
                    let g0 = run.glyphs[start];
                    let g1 = run.glyphs[end - 1];
                    let x0 = origin.0 + g0.x.min(g1.x);
                    let x1 = origin.0 + (g0.x + g0.advance).max(g1.x + g1.advance);
                    let r = Bounds::new(x0, l.y + info.top, x1 - x0, info.height);
                    match link_rects.iter_mut().find(|(j, _)| *j == i) {
                        Some((_, b)) => *b = b.union(&r),
                        None => link_rects.push((i, r)),
                    }
                }
                start = end;
            }
        }
        for d in &line.decorations {
            let auto = d.color == kiln_text::BLACK;
            let paint = match (auto, l.fill) {
                (true, Some(bg)) => Paint::Rgba(kiln_doc_layout::paint::contrast(bg)),
                (true, None) => Paint::Rgba(AUTO_INK),
                _ => Paint::Rgba(d.color),
            };
            self.mark(Mark::Artifact(artifact.unwrap_or(ArtifactKind::Layout)));
            self.canvas.op(&DrawOp::FillRect {
                rect: Bounds::new(
                    l.x + piece.x + d.x0,
                    l.y + piece.y + d.y,
                    d.x1 - d.x0,
                    d.thickness,
                ),
                paint,
                radius: 0.0,
            });
        }
        for (i, rect) in link_rects {
            let (_, span) = &links[i];
            if let Some(target) = self.w.goal(&span.target) {
                self.canvas.link(rect, &target, span.node);
            }
        }
    }

    fn rule(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, rule: &Rule) {
        let paint = Paint::Rgba(rule.color.unwrap_or(AUTO_INK));
        let horizontal = (y1 - y0).abs() < (x1 - x0).abs();
        let w = rule.width;
        match rule.kind {
            RuleKind::Single | RuleKind::Double => {
                let lines: &[(f32, f32)] = if rule.kind == RuleKind::Double {
                    &[(-w / 3.0, w / 3.0), (w / 3.0, w / 3.0)]
                } else {
                    &[(0.0, 1.0)]
                };
                for &(off, frac) in lines {
                    let t = if rule.kind == RuleKind::Double {
                        w * frac
                    } else {
                        w
                    };
                    let rect = if horizontal {
                        Bounds::new(x0.min(x1), y0 + off - t / 2.0, (x1 - x0).abs(), t)
                    } else {
                        Bounds::new(x0 + off - t / 2.0, y0.min(y1), t, (y1 - y0).abs())
                    };
                    self.canvas.op(&DrawOp::FillRect {
                        rect,
                        paint,
                        radius: 0.0,
                    });
                }
            }
            RuleKind::Dotted | RuleKind::Dashed => {
                let mut path = PathData::new();
                path.move_to(x0, y0).line_to(x1, y1);
                let unit = w.max(0.75);
                let dash = if rule.kind == RuleKind::Dotted {
                    vec![unit, unit * 2.0]
                } else {
                    vec![unit * 4.0, unit * 3.0]
                };
                self.canvas.op(&DrawOp::Path {
                    path,
                    fill: None,
                    stroke: Some(Stroke {
                        dash,
                        ..Stroke::solid(paint, w)
                    }),
                    rule: FillRule::NonZero,
                });
            }
        }
    }

    fn drawing(&mut self, d: &DrawingItem) {
        let rect = d.rect;
        let dr = &d.drawing;
        let rotated = dr.rotation != 0 || dr.flip_h || dr.flip_v;
        if rotated {
            #[allow(clippy::cast_precision_loss)]
            let angle = (dr.rotation as f32 / 60_000.0).to_radians();
            let (c, s) = (angle.cos(), angle.sin());
            let (fx, fy) = (
                if dr.flip_h { -1.0 } else { 1.0 },
                if dr.flip_v { -1.0 } else { 1.0 },
            );
            let (cx, cy) = (rect.x + rect.w / 2.0, rect.y + rect.h / 2.0);
            let (a, b, c2, d2) = (c * fx, s * fx, -s * fy, c * fy);
            let e = cx - a * cx - c2 * cy;
            let f = cy - b * cx - d2 * cy;
            self.canvas.op(&DrawOp::PushTransform {
                matrix: [a, b, c2, d2, e, f],
            });
        }
        match &dr.kind {
            DrawingKind::Picture { media, crop } => {
                let pic = self
                    .w
                    .media
                    .get(media)
                    .copied()
                    .and_then(|id| self.w.image(id));
                if let Some(pic) = pic {
                    #[allow(clippy::cast_precision_loss)]
                    let (w, h) = (pic.width as f32, pic.height as f32);
                    let c = crop.unwrap_or_default();
                    #[allow(clippy::cast_precision_loss)]
                    let f = |v: i32| (v as f32 / 100_000.0).clamp(-1.0, 1.0);
                    let src = Bounds::new(
                        w * f(c.left).max(0.0),
                        h * f(c.top).max(0.0),
                        (w * (1.0 - f(c.left).max(0.0) - f(c.right).max(0.0))).max(1.0),
                        (h * (1.0 - f(c.top).max(0.0) - f(c.bottom).max(0.0))).max(1.0),
                    );
                    self.canvas.op(&DrawOp::Image {
                        image: pic.id,
                        src,
                        dst: rect,
                        opacity: 1.0,
                    });
                } else {
                    self.placeholder(rect);
                }
            }
            DrawingKind::Shape { .. } => {
                let v = shape_visual(dr, rect.w, rect.h, &self.w.doc.theme);
                for p in v.paths {
                    let mut path = p.path;
                    path.translate(rect.x, rect.y);
                    self.canvas.op(&DrawOp::Path {
                        path,
                        fill: p.fill.map(Paint::Rgba),
                        stroke: p.stroke.map(|(c, w)| Stroke::solid(Paint::Rgba(c), w)),
                        rule: FillRule::NonZero,
                    });
                }
            }
            DrawingKind::Chart { .. } | DrawingKind::Group | DrawingKind::Other => {
                self.placeholder(rect);
            }
        }
        if rotated {
            self.canvas.op(&DrawOp::PopTransform);
        }
    }

    /// What the screen shows for content it cannot draw yet (EMF, WMF, charts): a quiet frame.
    fn placeholder(&mut self, rect: Bounds) {
        self.canvas.op(&DrawOp::FillRect {
            rect,
            paint: Paint::Rgba(0xF2_F0_EC_FF),
            radius: 0.0,
        });
        self.canvas.op(&DrawOp::StrokeRect {
            rect: rect.inflate(-0.5, -0.5),
            paint: Paint::Rgba(0xC8_C2_B8_FF),
            width: 1.0,
            radius: 0.0,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats_map_to_label_styles() {
        assert_eq!(
            label_style(&NumberFormat::LowerRoman),
            Some(LabelStyle::LowerRoman)
        );
        assert_eq!(label_style(&NumberFormat::Hebrew1), None);
    }

    #[test]
    fn page_setup_overrides_turn_pages_and_margins() {
        let doc = Document::new();
        let s = WriteSettings {
            orientation: Some(PrintOrientation::Landscape),
            ..WriteSettings::default()
        };
        let before = doc.sections[0].props.clone();
        let printed = print_document(&doc, &s);
        let after = &printed.sections[0].props;
        assert_eq!(after.page_size.width, before.page_size.height);
        assert_eq!(after.margins.top, before.margins.left);
        assert!(matches!(
            print_document(&doc, &WriteSettings::default()),
            Cow::Borrowed(_)
        ));
        let s = WriteSettings {
            margins: Some([720, 720, 720, 720]),
            ..WriteSettings::default()
        };
        assert_eq!(print_document(&doc, &s).sections[0].props.margins.left, 720);
    }
}
