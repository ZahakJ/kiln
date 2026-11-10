// SPDX-License-Identifier: AGPL-3.0-or-later
//! Printing and PDF export of presentations: the pages of PowerPoint's four print
//! layouts, painted into a [`PageSink`]. `kiln-session` hands them to `kiln-io-pdf` as its
//! `PrintablePages` (the PDF writer depends on Write's layout, which paints DrawingML through
//! this crate, so the adapter lives above both).
//!
//! - **Full Page Slides**: one slide per page, the page the slide's size.
//! - **Handouts**: 1, 2, 3, 4, 6 or 9 slides per page on the notes page size, in rows
//!   (horizontal) or columns (vertical); three per page come with ruled lines for notes beside
//!   each slide, as PowerPoint prints them. The page number is at the bottom right.
//! - **Notes Pages**: the slide in the notes master's slide image box with the speaker notes in
//!   its body box under it.
//! - **Outline**: the titles and body text of every slide, numbered, flowing over pages.
//!
//! Slides are drawn by the same [`Renderer`] as the stage, through a recording glyph sink
//! ([`Glyphs::recording`]): every text run becomes real, searchable text where the screen
//! draws glyph bitmaps. The structure is tagged: one section per slide, its title as a heading,
//! text as paragraphs, pictures and charts as figures with their alternative text; master and
//! layout decoration are artifacts. Hidden slides print only when asked.

mod layout;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use kiln_common::NodeId;
use kiln_deck::{Owner, Presentation, ShapeKind};
use kiln_scene::{Bounds, DrawOp, ImageId, Paint};
use kiln_text::{FontStore, ParagraphLayout};
use rustc_hash::FxHashMap;

use crate::geom::px;
use crate::glyphs::{Glyphs, RecordedRun, recorded_index};
use crate::render::{DrawnSlide, Options, Renderer};

pub use layout::{handout_cells, notes_boxes};

/// What a print shows of each slide (File ▸ Print ▸ Print Layout).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Layout {
    /// Full Page Slides.
    #[default]
    Slides,
    /// Notes Pages.
    Notes,
    /// Outline.
    Outline,
    /// Handouts with 1, 2, 3, 4, 6 or 9 slides per page (other counts round up to one of
    /// those), in rows or, `vertical`, in columns.
    Handouts {
        /// Slides per page.
        per_page: u8,
        /// Down the columns first.
        vertical: bool,
    },
}

impl Layout {
    /// The handout counts PowerPoint offers.
    pub const HANDOUTS: [u8; 6] = [1, 2, 3, 4, 6, 9];

    /// Handouts of `n` slides per page (rounded up to a count PowerPoint offers).
    #[must_use]
    pub fn handouts(n: u32, vertical: bool) -> Self {
        let per_page = Self::HANDOUTS
            .into_iter()
            .find(|k| u32::from(*k) >= n)
            .unwrap_or(9);
        Self::Handouts { per_page, vertical }
    }
}

/// A node of the printed structure: an index into [`StructTree::nodes`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StructId(pub u32);

/// What a structure node is (the tagged PDF types a presentation uses).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StructKind {
    /// A slide.
    Section,
    /// A group or a table.
    Div,
    /// Text.
    P,
    /// A slide title.
    Heading {
        /// Level (1).
        level: u8,
        /// The title.
        title: String,
    },
    /// A picture, chart or shape (or, on handouts and notes pages, a whole slide).
    Figure {
        /// Alternative text (`None`: decorative).
        alt: Option<String>,
    },
}

/// One node of the structure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructNode {
    /// What it is.
    pub kind: StructKind,
    /// Its parent (`None`: the document).
    pub parent: Option<StructId>,
}

/// The logical structure of the printed pages, in reading order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StructTree {
    /// The nodes.
    pub nodes: Vec<StructNode>,
}

impl StructTree {
    fn add(&mut self, kind: StructKind, parent: Option<StructId>) -> StructId {
        let id = StructId(u32::try_from(self.nodes.len()).unwrap_or(u32::MAX));
        self.nodes.push(StructNode { kind, parent });
        id
    }

    /// A node.
    #[must_use]
    pub fn get(&self, id: StructId) -> Option<&StructNode> {
        self.nodes.get(id.0 as usize)
    }
}

/// Content that is not part of the structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactKind {
    /// Page numbers.
    Footer,
    /// Backgrounds, decoration, frames and rules.
    Layout,
}

/// What the content painted until [`PageSink::end`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mark {
    /// A structure node.
    Content(StructId),
    /// An artifact.
    Artifact(ArtifactKind),
}

/// A run of text on a page.
#[derive(Debug, Clone)]
pub struct TextRun<'a> {
    /// The fonts of the run's font ids.
    pub fonts: &'a FontStore,
    /// The glyphs.
    pub run: &'a kiln_text::GlyphRun,
    /// The glyphs to draw.
    pub glyphs: std::ops::Range<usize>,
    /// Where the run's origin is, page px.
    pub origin: (f32, f32),
    /// The text the clusters index into.
    pub text: &'a str,
    /// The colour.
    pub paint: Paint,
}

/// Receives a page: Scene ops (glyph bitmaps never come), text runs and structure marks.
/// Coordinates are page px, origin top left.
pub trait PageSink {
    /// An op.
    fn op(&mut self, op: &DrawOp);
    /// Text.
    fn text(&mut self, run: &TextRun<'_>);
    /// Starts content of `mark` (marks do not nest).
    fn begin(&mut self, mark: Mark);
    /// Ends it.
    fn end(&mut self);
}

/// A bookmark of the PDF: a slide's title on its page.
#[derive(Debug, Clone, PartialEq)]
pub struct Bookmark {
    /// The slide's title ("Slide 3" when it has none).
    pub title: String,
    /// 0-based page.
    pub page: usize,
    /// Top of the slide on the page, px.
    pub y: f32,
}

/// A picture the pages paint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// The id ops refer to.
    pub id: ImageId,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Straight-alpha RGBA.
    pub rgba: Arc<Vec<u8>>,
    /// The media file when it was decoded at full size (a PDF embeds a JPEG as it is).
    pub file: Option<Arc<Vec<u8>>>,
}

/// What and how to print.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrintOptions {
    /// The layout.
    pub layout: Layout,
    /// Frame Slides: a thin border around each slide.
    pub frame: bool,
    /// Print Hidden Slides.
    pub hidden: bool,
    /// Only these slides (0-based indices, in order); empty for every slide.
    pub slides: Vec<usize>,
    /// The document title (PDF metadata).
    pub title: Option<String>,
    /// Today's date for date fields, as `(year, month, day)`.
    pub today: Option<(i32, u32, u32)>,
}

/// One slide drawn for print.
#[derive(Debug)]
struct Printed {
    /// Slide id.
    id: NodeId,
    /// The number the slide shows (first slide number + index).
    number: usize,
    title: String,
    drawn: DrawnSlide,
    /// Text runs the marker ops of `drawn` stand for.
    runs: Vec<RecordedRun>,
    /// The structure node of each drawn shape (`None`: decoration, an artifact).
    marks: Vec<Option<StructId>>,
    /// The slide's section (Full Page Slides) or figure (handouts, notes pages).
    node: StructId,
}

/// A paragraph placed on a page (notes and the outline).
#[derive(Debug, Clone)]
struct Placed {
    layout: ParagraphLayout,
    /// Paragraph origin, page px.
    x: f32,
    y: f32,
    node: StructId,
}

/// What a page shows.
#[derive(Debug, Clone)]
enum Body {
    /// One slide filling the page.
    Slide(usize),
    /// Handout cells: the slide and where it goes; three per page draw lines beside them.
    Handout {
        cells: Vec<(usize, Bounds)>,
        lines: bool,
    },
    /// A notes page.
    Notes {
        slide: usize,
        image: Bounds,
        text: Vec<Placed>,
    },
    /// A page of the outline.
    Outline(Vec<Placed>),
}

/// The printed pages of a presentation.
pub struct DeckPrint<'a> {
    fonts: &'a FontStore,
    opts: PrintOptions,
    page: (f32, f32),
    printed: Vec<Printed>,
    pages: Vec<Body>,
    tree: StructTree,
    images: FxHashMap<u32, Arc<Picture>>,
    /// Page numbers (handouts, notes pages, the outline).
    numbers: Vec<ParagraphLayout>,
}

impl std::fmt::Debug for DeckPrint<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckPrint")
            .field("layout", &self.opts.layout)
            .field("slides", &self.printed.len())
            .field("pages", &self.pages.len())
            .finish_non_exhaustive()
    }
}

/// Body text of notes pages and the outline: black, as PowerPoint prints them.
const INK: u32 = 0x00_00_00_FF;
/// Frames of slides and the lines of three-per-page handouts.
const RULE: u32 = 0x80_80_80_FF;

impl<'a> DeckPrint<'a> {
    /// Draws the slides `opts` prints and lays the pages out.
    pub fn new(renderer: &'a mut Renderer, pres: &Presentation, opts: PrintOptions) -> Self {
        let mut tree = StructTree::default();
        let indices: Vec<usize> = if opts.slides.is_empty() {
            (0..pres.slides.len()).collect()
        } else {
            opts.slides
                .iter()
                .copied()
                .filter(|i| *i < pres.slides.len())
                .collect()
        };
        let indices: Vec<usize> = indices
            .into_iter()
            .filter(|i| opts.hidden || !pres.slides[*i].hidden)
            .collect();
        let draw = Options {
            today: opts.today,
            ..Options::default()
        };
        let first = usize::try_from(pres.first_slide_number.max(1)).unwrap_or(1);
        let mut printed = Vec::with_capacity(indices.len());
        let mut used: Vec<ImageId> = Vec::new();
        for &i in &indices {
            let slide = &pres.slides[i];
            let title = crate::a11y::slide_title(pres, slide.id);
            let number = first + i;
            if opts.layout == Layout::Outline {
                printed.push(Printed {
                    id: slide.id,
                    number,
                    title,
                    drawn: DrawnSlide::default(),
                    runs: Vec::new(),
                    marks: Vec::new(),
                    node: StructId(0),
                });
                continue;
            }
            let mut glyphs = Glyphs::recording();
            let drawn = renderer.draw(pres, Owner::Slide(slide.id), &draw, &mut glyphs);
            used.extend(drawn.images.iter().copied());
            let runs = glyphs.take_recorded();
            let whole = opts.layout == Layout::Slides;
            let node = if whole {
                tree.add(StructKind::Section, None)
            } else {
                let alt = if title.is_empty() {
                    format!("Slide {number}")
                } else {
                    format!("Slide {number}: {title}")
                };
                tree.add(StructKind::Figure { alt: Some(alt) }, None)
            };
            let marks = drawn
                .shapes
                .iter()
                .map(|d| {
                    if !whole {
                        return Some(node);
                    }
                    let shape = slide.shapes.iter().find(|s| s.id == d.id)?;
                    Some(tree.add(kind_of(shape, &title), Some(node)))
                })
                .collect();
            printed.push(Printed {
                id: slide.id,
                number,
                title,
                drawn,
                runs,
                marks,
                node,
            });
        }
        let images = print_images(renderer, pres, &used);
        let page = match opts.layout {
            Layout::Slides => (
                px(pres.slide_size.cx).max(1.0),
                px(pres.slide_size.cy).max(1.0),
            ),
            _ => (
                px(pres.notes_size.cx).max(1.0),
                px(pres.notes_size.cy).max(1.0),
            ),
        };
        let slide_size = (
            px(pres.slide_size.cx).max(1.0),
            px(pres.slide_size.cy).max(1.0),
        );
        let pages: Vec<Body> = match opts.layout {
            Layout::Slides => (0..printed.len()).map(Body::Slide).collect(),
            Layout::Handouts { per_page, vertical } => {
                let cells = handout_cells(per_page, vertical, page, slide_size);
                let n = cells.len().max(1);
                (0..printed.len())
                    .collect::<Vec<_>>()
                    .chunks(n)
                    .map(|chunk| Body::Handout {
                        cells: chunk.iter().zip(&cells).map(|(i, r)| (*i, *r)).collect(),
                        lines: per_page == 3,
                    })
                    .collect()
            }
            Layout::Notes => {
                let (image, body) = notes_boxes(pres, page, slide_size);
                let fonts_engine = &mut renderer.engine;
                printed
                    .iter()
                    .enumerate()
                    .map(|(k, p)| {
                        let text = layout::notes_text(fonts_engine, pres, p.id, body, &mut tree);
                        Body::Notes {
                            slide: k,
                            image,
                            text,
                        }
                    })
                    .collect()
            }
            Layout::Outline => {
                layout::outline_pages(&mut renderer.engine, pres, &printed, page, &mut tree)
                    .into_iter()
                    .map(Body::Outline)
                    .collect()
            }
        };
        let numbers = if opts.layout == Layout::Slides {
            Vec::new()
        } else {
            (1..=pages.len())
                .map(|n| layout::plain(&mut renderer.engine, &n.to_string(), 16.0, false, 1.0e6))
                .collect()
        };
        let renderer: &'a Renderer = renderer;
        Self {
            fonts: renderer.fonts(),
            opts,
            page,
            printed,
            pages,
            tree,
            images,
            numbers,
        }
    }

    /// Number of slides printed.
    #[must_use]
    pub fn slide_count(&self) -> usize {
        self.printed.len()
    }

    /// Replays a drawn shape's ops, writing the recorded runs as text. The ops are under the
    /// slide's map to the page (`s`, `dest`); text is given in page px instead (print previews
    /// place glyphs by page position), so the map is lifted around each run. Turned text is
    /// drawn as outlines.
    fn replay(
        &self,
        p: &Printed,
        ops: &[DrawOp],
        (s, dest): (f32, Bounds),
        canvas: &mut dyn PageSink,
    ) {
        let mut depth = 0usize;
        for op in ops {
            match op {
                DrawOp::PushTransform { .. } => depth += 1,
                DrawOp::PopTransform => depth = depth.saturating_sub(1),
                _ => {}
            }
            let Some(run) = recorded_index(op).and_then(|i| p.runs.get(i)) else {
                canvas.op(op);
                continue;
            };
            if let Some(m) = run.matrix {
                let mut out = Vec::new();
                crate::glyphs::outline_run(
                    self.fonts, &run.run, run.origin, m, run.paint, &mut out,
                );
                for o in &out {
                    canvas.op(o);
                }
                continue;
            }
            let scaled = kiln_text::GlyphRun {
                size: run.run.size * s,
                glyphs: run
                    .run
                    .glyphs
                    .iter()
                    .map(|g| kiln_text::PositionedGlyph {
                        x: g.x * s,
                        y: g.y * s,
                        advance: g.advance * s,
                        ..*g
                    })
                    .collect(),
                ..run.run.clone()
            };
            let lift = depth == 0;
            if lift {
                canvas.op(&DrawOp::PopTransform);
            }
            canvas.text(&TextRun {
                fonts: self.fonts,
                run: if lift { &scaled } else { &run.run },
                glyphs: 0..run.run.glyphs.len(),
                origin: if lift {
                    (dest.x + run.origin.0 * s, dest.y + run.origin.1 * s)
                } else {
                    run.origin
                },
                text: &run.text,
                paint: run.paint,
            });
            if lift {
                canvas.op(&DrawOp::PushTransform {
                    matrix: [s, 0.0, 0.0, s, dest.x, dest.y],
                });
            }
        }
    }

    /// Paints a slide into `dest` (page px).
    fn paint_slide(&self, p: &Printed, dest: Bounds, canvas: &mut dyn PageSink) {
        let d = &p.drawn;
        let s = dest.w / d.width.max(1.0);
        canvas.op(&DrawOp::PushClip {
            rect: dest,
            path: None,
        });
        canvas.op(&DrawOp::PushTransform {
            matrix: [s, 0.0, 0.0, s, dest.x, dest.y],
        });
        let whole = self.opts.layout == Layout::Slides;
        canvas.begin(if whole {
            Mark::Artifact(ArtifactKind::Layout)
        } else {
            Mark::Content(p.node)
        });
        for op in &d.background {
            canvas.op(op);
        }
        canvas.end();
        for (shape, mark) in d.shapes.iter().zip(&p.marks) {
            canvas.begin(mark.map_or(Mark::Artifact(ArtifactKind::Layout), Mark::Content));
            self.replay(p, &shape.ops, (s, dest), canvas);
            canvas.end();
        }
        canvas.op(&DrawOp::PopTransform);
        canvas.op(&DrawOp::PopClip);
        if self.opts.frame || self.opts.layout == Layout::Notes {
            // The notes page always outlines its slide image (the notes master's thin frame);
            // slides and handouts with Frame Slides.
            canvas.begin(Mark::Artifact(ArtifactKind::Layout));
            canvas.op(&DrawOp::StrokeRect {
                rect: dest,
                paint: Paint::Rgba(if whole { INK } else { RULE }),
                width: 0.75,
                radius: 0.0,
            });
            canvas.end();
        }
    }

    fn paint_text(&self, text: &[Placed], canvas: &mut dyn PageSink) {
        for t in text {
            canvas.begin(Mark::Content(t.node));
            for line in &t.layout.lines {
                for run in &line.runs {
                    canvas.text(&TextRun {
                        fonts: self.fonts,
                        run,
                        glyphs: 0..run.glyphs.len(),
                        origin: (t.x, t.y),
                        text: t.layout.text(),
                        paint: Paint::Rgba(run.color),
                    });
                }
                for d in &line.decorations {
                    canvas.op(&DrawOp::FillRect {
                        rect: Bounds::new(
                            t.x + d.x0,
                            t.y + d.y,
                            (d.x1 - d.x0).max(0.0),
                            d.thickness.max(0.5),
                        ),
                        paint: Paint::Rgba(d.color),
                        radius: 0.0,
                    });
                }
            }
            canvas.end();
        }
    }

    /// The page number at the bottom right of handouts, notes pages and the outline.
    fn page_number(&self, index: usize, canvas: &mut dyn PageSink) {
        let Some(l) = self.numbers.get(index) else {
            return;
        };
        let w = l.lines.first().map_or(0.0, |line| line.width);
        let x = self.page.0 - layout::MARGIN - w;
        let y = self.page.1 - layout::MARGIN * 0.75 - l.height;
        canvas.begin(Mark::Artifact(ArtifactKind::Footer));
        for line in &l.lines {
            for run in &line.runs {
                canvas.text(&TextRun {
                    fonts: self.fonts,
                    run,
                    glyphs: 0..run.glyphs.len(),
                    origin: (x, y),
                    text: l.text(),
                    paint: Paint::Rgba(INK),
                });
            }
        }
        canvas.end();
    }
}

/// The structure of a shape on a slide: the title is the slide's heading, text is a paragraph,
/// pictures and charts are figures.
fn kind_of(shape: &kiln_deck::Shape, title: &str) -> StructKind {
    let alt = shape
        .alt_text
        .clone()
        .filter(|a| !a.trim().is_empty())
        .or_else(|| shape.title.clone().filter(|t| !t.trim().is_empty()));
    if shape.placeholder.is_some_and(|p| p.kind.is_title()) && !title.is_empty() {
        return StructKind::Heading {
            level: 1,
            title: title.to_owned(),
        };
    }
    match &shape.kind {
        ShapeKind::Picture(_) | ShapeKind::Chart(_) | ShapeKind::Media(_) => {
            StructKind::Figure { alt }
        }
        ShapeKind::Group(_) | ShapeKind::Table(_) => StructKind::Div,
        ShapeKind::Geometry(_) | ShapeKind::Connector(_) => {
            if shape.text().is_some_and(|t| !t.text().trim().is_empty()) {
                StructKind::P
            } else {
                StructKind::Figure { alt }
            }
        }
    }
}

/// The pictures the slides drew, with JPEG and PNG files embedded as they are when they were
/// decoded at full size.
fn print_images(
    renderer: &Renderer,
    pres: &Presentation,
    used: &[ImageId],
) -> FxHashMap<u32, Arc<Picture>> {
    let mut out = FxHashMap::default();
    for id in used {
        if out.contains_key(&id.0) {
            continue;
        }
        let Some(u) = renderer.pictures.upload(*id) else {
            continue;
        };
        let file = renderer
            .pictures
            .part_of(*id)
            .and_then(|part| pres.parts.get(part))
            .and_then(|part| {
                let (w, h) = image::ImageReader::new(std::io::Cursor::new(&part.bytes[..]))
                    .with_guessed_format()
                    .ok()?
                    .into_dimensions()
                    .ok()?;
                ((w, h) == (u.width, u.height)).then(|| Arc::new(part.bytes.to_vec()))
            });
        out.insert(
            id.0,
            Arc::new(Picture {
                id: *id,
                width: u.width,
                height: u.height,
                rgba: Arc::new(u.data),
                file,
            }),
        );
    }
    out
}

impl DeckPrint<'_> {
    /// The size of every page, px (96 per inch).
    #[must_use]
    pub fn page_size(&self) -> (f32, f32) {
        self.page
    }

    /// The fonts text runs refer to.
    #[must_use]
    pub fn fonts(&self) -> &FontStore {
        self.fonts
    }

    /// The document title.
    #[must_use]
    pub fn title(&self) -> Option<&str> {
        self.opts.title.as_deref().filter(|t| !t.is_empty())
    }

    /// The number of pages (at least one: a presentation without slides prints a blank page).
    #[must_use]
    pub fn page_count(&self) -> usize {
        self.pages.len().max(1)
    }

    /// The logical structure.
    #[must_use]
    pub fn structure(&self) -> &StructTree {
        &self.tree
    }

    /// The bookmarks: each slide's title where it is printed.
    #[must_use]
    pub fn bookmarks(&self) -> Vec<Bookmark> {
        let mut out = Vec::new();
        for (page, body) in self.pages.iter().enumerate() {
            let mut entry = |k: usize, y: f32| {
                if let Some(p) = self.printed.get(k) {
                    out.push(Bookmark {
                        title: if p.title.is_empty() {
                            format!("Slide {}", p.number)
                        } else {
                            p.title.clone()
                        },
                        page,
                        y,
                    });
                }
            };
            match body {
                Body::Slide(k) => entry(*k, 0.0),
                Body::Notes { slide, image, .. } => entry(*slide, image.y),
                Body::Handout { cells, .. } => {
                    for (k, r) in cells {
                        entry(*k, r.y);
                    }
                }
                Body::Outline(_) => {}
            }
        }
        if self.opts.layout == Layout::Outline {
            for (page, body) in self.pages.iter().enumerate() {
                if let Body::Outline(items) = body {
                    for t in items {
                        if let Some(StructKind::Heading { title, .. }) =
                            self.tree.get(t.node).map(|n| &n.kind)
                        {
                            out.push(Bookmark {
                                title: title.clone(),
                                page,
                                y: t.y,
                            });
                        }
                    }
                }
            }
        }
        out
    }

    /// Every picture the pages paint.
    pub fn pictures(&self) -> impl Iterator<Item = &Arc<Picture>> {
        self.images.values()
    }

    /// A picture by id.
    #[must_use]
    pub fn picture(&self, id: ImageId) -> Option<&Arc<Picture>> {
        self.images.get(&id.0)
    }

    /// Paints page `index` (0-based).
    pub fn paint(&self, index: usize, canvas: &mut dyn PageSink) {
        canvas.begin(Mark::Artifact(ArtifactKind::Layout));
        canvas.op(&DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, self.page.0, self.page.1),
            paint: Paint::Rgba(0xFF_FF_FF_FF),
            radius: 0.0,
        });
        canvas.end();
        let Some(body) = self.pages.get(index) else {
            return;
        };
        match body {
            Body::Slide(k) => {
                if let Some(p) = self.printed.get(*k) {
                    self.paint_slide(p, Bounds::new(0.0, 0.0, self.page.0, self.page.1), canvas);
                }
                return;
            }
            Body::Handout { cells, lines } => {
                for (k, r) in cells {
                    if let Some(p) = self.printed.get(*k) {
                        self.paint_slide(p, *r, canvas);
                    }
                    if *lines {
                        canvas.begin(Mark::Artifact(ArtifactKind::Layout));
                        for y in layout::note_lines(*r) {
                            canvas.op(&DrawOp::FillRect {
                                rect: Bounds::new(
                                    self.page.0 / 2.0 + layout::GAP,
                                    y,
                                    self.page.0 / 2.0 - layout::MARGIN - layout::GAP,
                                    0.75,
                                ),
                                paint: Paint::Rgba(RULE),
                                radius: 0.0,
                            });
                        }
                        canvas.end();
                    }
                }
            }
            Body::Notes { slide, image, text } => {
                if let Some(p) = self.printed.get(*slide) {
                    self.paint_slide(p, *image, canvas);
                }
                self.paint_text(text, canvas);
            }
            Body::Outline(text) => self.paint_text(text, canvas),
        }
        self.page_number(index, canvas);
    }
}
