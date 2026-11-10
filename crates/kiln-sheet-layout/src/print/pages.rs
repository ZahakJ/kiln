// SPDX-License-Identifier: AGPL-3.0-or-later
//! The printed pages of one or more sheets as `kiln-io-pdf`'s [`PrintablePages`]: each page's
//! cells painted by the screen's [`CellPainter`] (fills, borders, conditional formats, merged
//! cells, overflowing text) at the sheet's print scale, with print titles, printed gridlines and
//! headings, and the header and footer. Text is real text (searchable, tagged as table cells of
//! one table per page).

use std::cell::RefCell;
use std::sync::Arc;

use kiln_charts::model::ChartSpace;
use kiln_charts::text::{OutlineSink, TextMeasurer};
use kiln_io_pdf::printable::{
    ArtifactKind, DocInfo, LabelStyle, Mark, OutlineEntry, PageCanvas, PageInfo, PageLabel,
    PrintImage, PrintablePages, StructId, StructKind, StructTree, TextRun,
};
use kiln_ooxml::drawingml::theme::Theme;
use kiln_scene::{Bounds, ColorRole, DrawOp, ImageId, Paint, Stroke};
use kiln_sheet::cf::CellVisual;
use kiln_sheet::meta::DrawingKind;
use kiln_sheet::{CellPos, Range, Workbook};
use kiln_text::{FontStore, ParagraphProps, ParagraphSpec};
use rustc_hash::FxHashMap;

use super::hf::{Fields, sections};
use super::{HEADING_H, HEADING_W, SheetPages, paginate_with, setup};
use crate::geom::SheetGeometry;
use crate::glyphs::GlyphCache;
use crate::headers::col_letters;
use crate::paint::{CellPainter, LookCache, PaintCx, PrintedText, ViewOptions};
use crate::render::{geometry_of, view_options};

/// What is printed and the values of header and footer fields.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PrintOptions {
    /// Sheets to print (indices); empty = the active sheet.
    pub sheets: Vec<usize>,
    /// Print only this range of the first sheet (Print Selection).
    pub selection: Option<Range>,
    /// The document title (PDF metadata).
    pub title: String,
    /// File name for `&F`.
    pub file: String,
    /// Folder for `&Z`.
    pub path: String,
    /// Date for `&D`.
    pub date: String,
    /// Time for `&T`.
    pub time: String,
    /// Print in this orientation (File ▸ Print); `None` keeps each sheet's.
    pub landscape: Option<bool>,
    /// Print with these margins in inches (top, right, bottom, left); `None` keeps each sheet's.
    pub margins: Option<[f64; 4]>,
}

/// The chart of a drawing's part name.
pub type ChartLookup<'a> = dyn Fn(&str) -> Option<&'a ChartSpace> + 'a;

/// The objects over the cells a print draws: charts by part name, the images of pictures by
/// part name, and the workbook's theme (colours of charts and shapes).
#[derive(Default)]
pub struct PrintDrawings<'a> {
    /// The chart of a drawing's part (`/xl/charts/chart1.xml`).
    pub chart: Option<&'a ChartLookup<'a>>,
    /// The image of a picture's media part.
    pub images: FxHashMap<String, Arc<PrintImage>>,
    /// The workbook's DrawingML theme.
    pub theme: Option<&'a Theme>,
}

impl std::fmt::Debug for PrintDrawings<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintDrawings")
            .field("charts", &self.chart.is_some())
            .field("images", &self.images.len())
            .finish()
    }
}

/// One sheet's share of the job.
#[derive(Debug)]
struct SheetJob {
    sheet: usize,
    name: String,
    pages: SheetPages,
    geo: SheetGeometry,
    cf: FxHashMap<CellPos, CellVisual>,
    view: ViewOptions,
    /// The table of each page in the structure tree.
    tables: Vec<StructId>,
}

/// The printed pages of a workbook's sheets.
#[derive(Debug)]
pub struct WorkbookPrint<'a> {
    wb: &'a Workbook,
    jobs: Vec<SheetJob>,
    /// Global page → (job, page in job).
    index: Vec<(usize, usize)>,
    /// Cell, header and drawing text (one engine).
    text: RefCell<TextMeasurer>,
    /// The charts and pictures drawn over the cells.
    drawings: PrintDrawings<'a>,
    looks: RefCell<LookCache>,
    glyphs: RefCell<GlyphCache>,
    opts: PrintOptions,
    tree: StructTree,
}

/// Header text size, sheet px (Calibri 11 pt).
const HF_SIZE: f32 = 14.666_667;
/// Printed gridline colour.
const GRIDLINE: u32 = 0xBF_BF_BF_FF;

impl<'a> WorkbookPrint<'a> {
    /// Paginates the sheets and evaluates their conditional formats (which needs the workbook
    /// mutably).
    pub fn new(wb: &'a mut Workbook, fonts: FontStore, opts: PrintOptions) -> WorkbookPrint<'a> {
        let cf = conditional_formats(wb, &opts);
        let wb: &'a Workbook = wb;
        Self::with_cf(wb, fonts, opts, cf)
    }

    /// The pages with conditional formats evaluated beforehand by [`conditional_formats`] (one
    /// map per sheet printed; missing maps print without them).
    #[must_use]
    pub fn with_cf(
        wb: &'a Workbook,
        fonts: FontStore,
        opts: PrintOptions,
        cf: Vec<FxHashMap<CellPos, CellVisual>>,
    ) -> WorkbookPrint<'a> {
        let mut jobs = plan(wb, &opts);
        for (job, cf) in jobs.iter_mut().zip(cf) {
            job.cf = cf;
        }
        Self::finish(wb, jobs, fonts, opts)
    }

    fn finish(
        wb: &'a Workbook,
        mut jobs: Vec<SheetJob>,
        fonts: FontStore,
        opts: PrintOptions,
    ) -> WorkbookPrint<'a> {
        let mut tree = StructTree::default();
        let mut index = Vec::new();
        for (j, job) in jobs.iter_mut().enumerate() {
            for p in 0..job.pages.pages.len() {
                index.push((j, p));
                job.tables.push(tree.add(StructKind::Section, None));
            }
        }
        WorkbookPrint {
            wb,
            jobs,
            index,
            text: RefCell::new(TextMeasurer::new(fonts)),
            drawings: PrintDrawings::default(),
            looks: RefCell::new(LookCache::default()),
            glyphs: RefCell::new(GlyphCache::new()),
            opts,
            tree,
        }
    }

    /// The charts, pictures and shapes to print over the cells.
    #[must_use]
    pub fn with_drawings(mut self, drawings: PrintDrawings<'a>) -> Self {
        self.drawings = drawings;
        self
    }

    /// Paints the drawings of the sheet over `area` (sheet px) at `dest` (page px).
    fn drawing_ops(
        &self,
        canvas: &mut dyn PageCanvas,
        job: &SheetJob,
        area: Bounds,
        dest: (f32, f32),
        scale: f32,
    ) {
        let Some(sheet) = self.wb.sheets().get(job.sheet) else {
            return;
        };
        let mut text = self.text.borrow_mut();
        let mut started = false;
        for (i, d) in sheet.meta().drawings.iter().enumerate() {
            if d.hidden {
                continue;
            }
            let r = crate::drawings::anchor_rect(&job.geo, &d.anchor);
            let pad = match d.kind {
                DrawingKind::Shape | DrawingKind::Connector => crate::objects::shape_pad(&d.look),
                _ => 0.0,
            };
            let tile_rect = r.inflate(pad, pad);
            if !tile_rect.intersects(&area) {
                continue;
            }
            let ops = match d.kind {
                DrawingKind::Chart => {
                    let Some(chart) = self.drawings.chart.and_then(|f| f(&d.reference)) else {
                        continue;
                    };
                    kiln_charts::render::render(
                        chart,
                        r.w,
                        r.h,
                        self.drawings.theme,
                        kiln_charts::palette::ColorMode::Document,
                        &mut text,
                        &mut OutlineSink,
                    )
                    .1
                }
                DrawingKind::Picture | DrawingKind::Shape | DrawingKind::Connector => {
                    let kind = match d.kind {
                        DrawingKind::Picture => {
                            let img = self.drawings.images.get(&d.reference);
                            crate::objects::ObjectKind::Picture {
                                image: img.map(|x| x.id),
                                size: img.map_or((0, 0), |x| (x.width, x.height)),
                            }
                        }
                        DrawingKind::Connector => crate::objects::ObjectKind::Line,
                        _ => crate::objects::ObjectKind::Shape,
                    };
                    let tile = crate::objects::ObjectTile {
                        index: u32::try_from(i).unwrap_or(0),
                        rect: tile_rect,
                        pad,
                        version: 0,
                        kind,
                        look: &d.look,
                    };
                    crate::objects::object_ops(
                        &tile,
                        self.drawings.theme,
                        &mut text,
                        &mut OutlineSink,
                    )
                }
                _ => continue,
            };
            if !started {
                canvas.begin(Mark::Artifact(ArtifactKind::Layout));
                canvas.op(&DrawOp::PushClip {
                    rect: Bounds::new(dest.0, dest.1, area.w * scale, area.h * scale),
                    path: None,
                });
                started = true;
            }
            let (x, y) = if d.kind == DrawingKind::Chart {
                (r.x, r.y)
            } else {
                (tile_rect.x, tile_rect.y)
            };
            canvas.op(&DrawOp::PushTransform {
                matrix: [
                    scale,
                    0.0,
                    0.0,
                    scale,
                    dest.0 + (x - area.x) * scale,
                    dest.1 + (y - area.y) * scale,
                ],
            });
            for op in &ops {
                canvas.op(&literal(op));
            }
            canvas.op(&DrawOp::PopTransform);
        }
        if started {
            canvas.op(&DrawOp::PopClip);
            canvas.end();
        }
    }

    /// Pages of each sheet printed, in order.
    #[must_use]
    pub fn sheet_pages(&self) -> Vec<(usize, usize)> {
        self.jobs
            .iter()
            .map(|j| (j.sheet, j.pages.pages.len()))
            .collect()
    }

    /// The pagination of the first sheet printed.
    #[must_use]
    pub fn first(&self) -> Option<&SheetPages> {
        self.jobs.first().map(|j| &j.pages)
    }

    /// The fields of page `n` (0-based in the job) of `job`.
    fn fields(&self, job: &SheetJob, n: usize) -> ([String; 3], [String; 3]) {
        let first = job.pages.setup.print.first_page_number.unwrap_or(1);
        let f = Fields {
            page: first + u32::try_from(n).unwrap_or(0),
            pages: u32::try_from(job.pages.pages.len()).unwrap_or(0),
            date: &self.opts.date,
            time: &self.opts.time,
            file: &self.opts.file,
            path: &self.opts.path,
            sheet: &job.name,
        };
        let hf = &job.pages.setup.print.header_footer;
        let odd = n.is_multiple_of(2);
        let header = if hf.different_first && n == 0 {
            hf.first_header.as_deref()
        } else if hf.different_odd_even && !odd {
            hf.even_header.as_deref()
        } else {
            hf.odd_header.as_deref()
        };
        let footer = if hf.different_first && n == 0 {
            hf.first_footer.as_deref()
        } else if hf.different_odd_even && !odd {
            hf.even_footer.as_deref()
        } else {
            hf.odd_footer.as_deref()
        };
        (
            sections(header.unwrap_or(""), &f),
            sections(footer.unwrap_or(""), &f),
        )
    }

    /// Lays out one line of plain text at `size` px and paints it with its baseline region
    /// starting at `(x, y)` (top), aligned within `width` (0 left, 1 centre, 2 right).
    #[allow(clippy::too_many_arguments)]
    fn plain_text(
        &self,
        canvas: &mut dyn PageCanvas,
        text: &str,
        size: f32,
        bold: bool,
        x: f32,
        y: f32,
        width: f32,
        align: u8,
    ) {
        if text.is_empty() {
            return;
        }
        let mut text_m = self.text.borrow_mut();
        let engine = text_m.engine_mut();
        let style = kiln_text::SpanStyle {
            family: "Calibri".into(),
            size,
            weight: if bold { 700 } else { 400 },
            italic: false,
            color: kiln_text::BLACK,
            underline: kiln_text::UnderlineKind::None,
            strike: kiln_text::StrikeKind::None,
            letter_spacing: 0.0,
            baseline_shift: 0.0,
            lang: None,
            rtl: false,
            kerning: true,
            ligatures: true,
        };
        let layout = engine.layout(&ParagraphSpec {
            runs: vec![kiln_text::TextRun::new(text, style)],
            props: ParagraphProps::default(),
            width: 1.0e7,
        });
        let w = crate::celltext::text_width(&layout);
        let ox = match align {
            0 => x,
            1 => x + (width - w) / 2.0,
            _ => x + width - w,
        };
        for line in &layout.lines {
            for run in &line.runs {
                canvas.text(&TextRun {
                    fonts: engine.fonts(),
                    run,
                    glyphs: 0..run.glyphs.len(),
                    origin: (ox, y),
                    text: layout.text(),
                    paint: Paint::Rgba(0x00_00_00_FF),
                    actual: None,
                });
            }
        }
    }

    /// Paints the cells of `area` (sheet px) at `dest` (page px) with `scale`.
    fn cells(
        &self,
        canvas: &mut dyn PageCanvas,
        job: &SheetJob,
        area: Bounds,
        dest: (f32, f32),
        scale: f32,
        table: StructId,
    ) {
        let Some(sheet) = self.wb.sheets().get(job.sheet) else {
            return;
        };
        if area.w <= 0.0 || area.h <= 0.0 {
            return;
        }
        let cx = PaintCx {
            wb: self.wb,
            sheet,
            geo: &job.geo,
            cf: &job.cf,
            opts: &job.view,
        };
        let mut text_m = self.text.borrow_mut();
        let engine = text_m.engine_mut();
        let mut looks = self.looks.borrow_mut();
        let mut glyphs = self.glyphs.borrow_mut();
        let mut painter = CellPainter {
            text: engine,
            glyphs: &mut glyphs,
            looks: &mut looks,
            sink: Some(Vec::new()),
        };
        let ops = painter.paint(&cx, area);
        let texts = painter.sink.take().unwrap_or_default();
        let clip = Bounds::new(dest.0, dest.1, area.w * scale, area.h * scale);
        canvas.begin(Mark::Artifact(ArtifactKind::Layout));
        canvas.op(&DrawOp::PushClip {
            rect: clip,
            path: None,
        });
        canvas.op(&DrawOp::PushTransform {
            matrix: [scale, 0.0, 0.0, scale, dest.0, dest.1],
        });
        for op in &ops {
            canvas.op(&literal(op));
        }
        canvas.op(&DrawOp::PopTransform);
        canvas.end();
        canvas.begin(Mark::Content(table));
        for t in &texts {
            emit_text(canvas, engine.fonts(), t, area, dest, scale);
        }
        canvas.end();
        canvas.op(&DrawOp::PopClip);
    }

    /// Row or column headings of a band.
    #[allow(clippy::too_many_arguments)]
    fn headings(
        &self,
        canvas: &mut dyn PageCanvas,
        job: &SheetJob,
        items: &[(u32, f32, f32)],
        columns: bool,
        dest: (f32, f32),
        scale: f32,
    ) {
        canvas.begin(Mark::Artifact(ArtifactKind::Layout));
        for &(i, at, size) in items {
            let rect = if columns {
                Bounds::new(dest.0 + at * scale, dest.1, size * scale, HEADING_H * scale)
            } else {
                Bounds::new(dest.0, dest.1 + at * scale, HEADING_W * scale, size * scale)
            };
            canvas.op(&DrawOp::FillRect {
                rect,
                paint: Paint::Rgba(0xF2_F2_F2_FF),
                radius: 0.0,
            });
            canvas.op(&DrawOp::StrokeRect {
                rect,
                paint: Paint::Rgba(GRIDLINE),
                width: 0.5,
                radius: 0.0,
            });
            let label = if columns {
                col_letters(i)
            } else {
                (u64::from(i) + 1).to_string()
            };
            let ts = 11.0 * scale;
            self.plain_text(
                canvas,
                &label,
                ts,
                false,
                rect.x,
                rect.y + (rect.h - ts * 1.25) / 2.0,
                rect.w,
                1,
            );
        }
        canvas.end();
        let _ = job;
    }
}

/// The sheets a job prints.
fn sheets_of(wb: &Workbook, opts: &PrintOptions) -> Vec<usize> {
    if opts.sheets.is_empty() {
        vec![
            wb.props()
                .active_sheet
                .min(wb.sheets().len().saturating_sub(1)),
        ]
    } else {
        opts.sheets.clone()
    }
}

/// The conditional-format visuals of what a job prints, one map per sheet printed (in
/// order), for [`WorkbookPrint::with_cf`].
pub fn conditional_formats(
    wb: &mut Workbook,
    opts: &PrintOptions,
) -> Vec<FxHashMap<CellPos, CellVisual>> {
    let jobs = plan(wb, opts);
    jobs.iter()
        .map(|job| {
            let mut out = FxHashMap::default();
            let Some(sid) = wb.sheets().get(job.sheet).map(kiln_sheet::Sheet::id) else {
                return out;
            };
            if wb.sheet(sid).is_none_or(|s| s.meta().cf.is_empty()) {
                return out;
            }
            let mut areas = job.pages.areas.clone();
            if let Some((a, b)) = job.pages.setup.title_rows {
                areas.extend(job.pages.areas.iter().map(|r| Range::new(a, r.c0, b, r.c1)));
            }
            if let Some((a, b)) = job.pages.setup.title_cols {
                areas.extend(job.pages.areas.iter().map(|r| Range::new(r.r0, a, r.r1, b)));
            }
            for a in areas {
                out.extend(wb.conditional_formats(sid, a));
            }
            out
        })
        .collect()
}

/// The pages of every sheet printed (conditional formats still to evaluate).
fn plan(wb: &Workbook, opts: &PrintOptions) -> Vec<SheetJob> {
    let mut jobs = Vec::new();
    for (k, i) in sheets_of(wb, opts).into_iter().enumerate() {
        let Some(sheet) = wb.sheets().get(i) else {
            continue;
        };
        let mut s = setup(wb, sheet.id());
        if let Some(l) = opts.landscape {
            s.print.orientation = if l {
                kiln_sheet::meta::Orientation::Landscape
            } else {
                kiln_sheet::meta::Orientation::Portrait
            };
        }
        if let Some([t, r, b, l]) = opts.margins {
            s.print.margins.top = t;
            s.print.margins.right = r;
            s.print.margins.bottom = b;
            s.print.margins.left = l;
        }
        if k == 0
            && let Some(sel) = opts.selection
        {
            s.areas = vec![sel];
        }
        let geo = geometry_of(sheet, false);
        let pages = paginate_with(sheet, &geo, s);
        let mut view = view_options(wb, sheet);
        view.gridlines = pages.setup.print.print_gridlines;
        view.gridline = Paint::Rgba(GRIDLINE);
        view.markers = false;
        jobs.push(SheetJob {
            sheet: i,
            name: sheet.name().to_owned(),
            pages,
            geo,
            cf: FxHashMap::default(),
            view,
            tables: Vec::new(),
        });
    }
    jobs
}

/// Role paints of the screen as print colours.
fn literal_paint(p: Paint) -> Paint {
    match p {
        Paint::Role(ColorRole::Page | ColorRole::Panel) => Paint::Rgba(0xFF_FF_FF_FF),
        Paint::Role(ColorRole::Gridline | ColorRole::Line) => Paint::Rgba(GRIDLINE),
        Paint::Role(_) => Paint::Rgba(0x00_00_00_FF),
        other => other,
    }
}

fn literal(op: &DrawOp) -> DrawOp {
    match op {
        DrawOp::FillRect {
            rect,
            paint,
            radius,
        } => DrawOp::FillRect {
            rect: *rect,
            paint: literal_paint(*paint),
            radius: *radius,
        },
        DrawOp::StrokeRect {
            rect,
            paint,
            width,
            radius,
        } => DrawOp::StrokeRect {
            rect: *rect,
            paint: literal_paint(*paint),
            width: *width,
            radius: *radius,
        },
        DrawOp::Path {
            path,
            fill,
            stroke,
            rule,
        } => DrawOp::Path {
            path: path.clone(),
            fill: fill.map(literal_paint),
            stroke: stroke.as_ref().map(|s| Stroke {
                paint: literal_paint(s.paint),
                ..s.clone()
            }),
            rule: *rule,
        },
        DrawOp::OutlineGlyphs { path, paint } => DrawOp::OutlineGlyphs {
            path: path.clone(),
            paint: literal_paint(*paint),
        },
        other => other.clone(),
    }
}

/// One printed run of cell text, mapped from sheet px (relative to `area`) to the page.
fn emit_text(
    canvas: &mut dyn PageCanvas,
    fonts: &FontStore,
    t: &PrintedText,
    area: Bounds,
    dest: (f32, f32),
    scale: f32,
) {
    let map = |x: f32, y: f32| (dest.0 + (x - area.x) * scale, dest.1 + (y - area.y) * scale);
    if let Some(c) = t.clip {
        let (x, y) = map(c.x, c.y);
        canvas.op(&DrawOp::PushClip {
            rect: Bounds::new(x, y, c.w * scale, c.h * scale),
            path: None,
        });
    }
    let run = kiln_text::GlyphRun {
        font: t.run.font,
        size: t.run.size * scale,
        span: t.run.span,
        color: t.run.color,
        level: t.run.level,
        text_range: t.run.text_range.clone(),
        glyphs: t
            .run
            .glyphs
            .iter()
            .map(|g| kiln_text::PositionedGlyph {
                x: g.x * scale,
                y: g.y * scale,
                advance: g.advance * scale,
                ..*g
            })
            .collect(),
    };
    canvas.text(&TextRun {
        fonts,
        run: &run,
        glyphs: 0..run.glyphs.len(),
        origin: map(t.origin.0, t.origin.1),
        text: &t.text,
        paint: literal_paint(t.paint),
        actual: None,
    });
    if t.clip.is_some() {
        canvas.op(&DrawOp::PopClip);
    }
}

impl PrintablePages for WorkbookPrint<'_> {
    fn pages(&self) -> Vec<PageInfo> {
        self.index
            .iter()
            .enumerate()
            .map(|(i, &(j, _))| {
                let p = &self.jobs[j].pages;
                PageInfo {
                    width: p.width,
                    height: p.height,
                    label: PageLabel {
                        style: LabelStyle::Decimal,
                        prefix: String::new(),
                        number: u32::try_from(i + 1).unwrap_or(u32::MAX),
                    },
                }
            })
            .collect()
    }

    fn info(&self) -> DocInfo {
        DocInfo {
            title: (!self.opts.title.is_empty()).then(|| self.opts.title.clone()),
            creator: Some("Kiln Grid".into()),
            ..DocInfo::default()
        }
    }

    fn structure(&self) -> StructTree {
        self.tree.clone()
    }

    fn outline(&self) -> Vec<OutlineEntry> {
        // One bookmark per sheet when several are printed.
        if self.jobs.len() < 2 {
            return Vec::new();
        }
        let mut at = 0;
        self.jobs
            .iter()
            .map(|j| {
                let e = OutlineEntry {
                    level: 1,
                    title: j.name.clone(),
                    page: at,
                    y: 0.0,
                };
                at += j.pages.pages.len();
                e
            })
            .collect()
    }

    fn image(&self, id: ImageId) -> Option<Arc<PrintImage>> {
        self.drawings.images.values().find(|i| i.id == id).cloned()
    }

    fn paint(&self, index: usize, canvas: &mut dyn PageCanvas) {
        let Some(&(j, n)) = self.index.get(index) else {
            return;
        };
        let job = &self.jobs[j];
        let p = &job.pages;
        let Some(page) = p.pages.get(n).copied() else {
            return;
        };
        let geo = &job.geo;
        let s = p.scale;
        let headings = p.setup.print.print_headings;
        let span_rows =
            |a: u32, b: u32| -> f32 { (geo.rows.offset(b + 1) - geo.rows.offset(a)) as f32 };
        let span_cols =
            |a: u32, b: u32| -> f32 { (geo.cols.offset(b + 1) - geo.cols.offset(a)) as f32 };
        let super::PageFrame {
            x0,
            y0,
            cx0,
            cy0,
            tw,
            th,
        } = super::page_frame(p, geo, &page);
        let bh = span_rows(page.rows.0, page.rows.1);
        let bw = span_cols(page.cols.0, page.cols.1);
        // Page background.
        canvas.begin(Mark::Artifact(ArtifactKind::Layout));
        canvas.op(&DrawOp::FillRect {
            rect: Bounds::new(0.0, 0.0, p.width, p.height),
            paint: Paint::Rgba(0xFF_FF_FF_FF),
            radius: 0.0,
        });
        canvas.end();
        let table = job.tables[n];
        let rows_y = |a: u32| geo.rows.offset(a) as f32;
        let cols_x = |a: u32| geo.cols.offset(a) as f32;
        // Corner: title rows × title columns.
        if let (Some((ra, rb)), Some((ca, cb))) = (page.title_rows, page.title_cols) {
            let area = Bounds::new(cols_x(ca), rows_y(ra), span_cols(ca, cb), span_rows(ra, rb));
            self.cells(canvas, job, area, (cx0, cy0), s, table);
        }
        // Title rows over the page's columns.
        if let Some((ra, rb)) = page.title_rows {
            let area = Bounds::new(cols_x(page.cols.0), rows_y(ra), bw, span_rows(ra, rb));
            self.cells(canvas, job, area, (cx0 + tw * s, cy0), s, table);
        }
        // Title columns beside the page's rows.
        if let Some((ca, cb)) = page.title_cols {
            let area = Bounds::new(cols_x(ca), rows_y(page.rows.0), span_cols(ca, cb), bh);
            self.cells(canvas, job, area, (cx0, cy0 + th * s), s, table);
        }
        // The page's own cells, and the charts, pictures and shapes over them.
        let area = Bounds::new(cols_x(page.cols.0), rows_y(page.rows.0), bw, bh);
        self.cells(canvas, job, area, (cx0 + tw * s, cy0 + th * s), s, table);
        self.drawing_ops(canvas, job, area, (cx0 + tw * s, cy0 + th * s), s);
        if headings {
            let mut cols: Vec<(u32, f32, f32)> = Vec::new();
            let mut at = 0.0;
            let mut push_cols = |a: u32, b: u32, out: &mut Vec<(u32, f32, f32)>| {
                for c in a..=b {
                    let w = if geo.cols.is_hidden(c) {
                        0.0
                    } else {
                        geo.cols.span(c).1 as f32
                    };
                    if w > 0.0 {
                        out.push((c, at, w));
                    }
                    at += w;
                }
            };
            if let Some((a, b)) = page.title_cols {
                push_cols(a, b, &mut cols);
            }
            push_cols(page.cols.0, page.cols.1, &mut cols);
            self.headings(canvas, job, &cols, true, (cx0, y0), s);
            let mut rows: Vec<(u32, f32, f32)> = Vec::new();
            let mut at = 0.0;
            let mut push_rows = |a: u32, b: u32, out: &mut Vec<(u32, f32, f32)>| {
                for r in a..=b {
                    let h = if geo.rows.is_hidden(r) {
                        0.0
                    } else {
                        geo.rows.span(r).1 as f32
                    };
                    if h > 0.0 {
                        out.push((r, at, h));
                    }
                    at += h;
                }
            };
            if let Some((a, b)) = page.title_rows {
                push_rows(a, b, &mut rows);
            }
            push_rows(page.rows.0, page.rows.1, &mut rows);
            self.headings(canvas, job, &rows, false, (x0, cy0), s);
        }
        // Header and footer, in the margins (scaled with the document as Excel's default).
        let (header, footer) = self.fields(job, n);
        let size = HF_SIZE
            * if p.setup.print.header_footer.scale_with_doc {
                s.max(0.5)
            } else {
                1.0
            };
        let left = p.margins[0];
        let width = p.width - p.margins[0] - p.margins[1];
        canvas.begin(Mark::Artifact(ArtifactKind::Header));
        for (k, text) in header.iter().enumerate() {
            self.plain_text(
                canvas,
                text,
                size,
                false,
                left,
                p.margins[4],
                width,
                k as u8,
            );
        }
        canvas.end();
        canvas.begin(Mark::Artifact(ArtifactKind::Footer));
        let fy = p.height - p.margins[5] - size * 1.25;
        for (k, text) in footer.iter().enumerate() {
            self.plain_text(canvas, text, size, false, left, fy, width, k as u8);
        }
        canvas.end();
    }
}
