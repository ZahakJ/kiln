// SPDX-License-Identifier: AGPL-3.0-or-later
//! Print, PDF and picture export of presentations.
//!
//! - **Print and PDF** (`DocModel::print`): `kiln-deck-layout::print` lays out full page
//!   slides, handouts (1, 2, 3, 4, 6 or 9 per page), notes pages or the outline as the print
//!   settings' [`SlidePrint`] asks; [`DeckPages`] hands them to `kiln-io-pdf` as its
//!   `PrintablePages`. As in PowerPoint, the page box (`1-3, 5`) and odd/even pick slides, not
//!   printed pages, and hidden slides print only with Print Hidden Slides.
//! - **Pictures** (`DeckQuery::ExportPictures`): slides drawn by `kiln-raster` at the width
//!   asked for, as PNG or JPEG: one file, or a `.zip` with a folder of `Slide1.png`… named after
//!   the presentation, as PowerPoint's Export ▸ Change File Type saves them.

use std::io::Write as _;
use std::sync::Arc;

use kiln_common::{KilnError, Result};
use kiln_deck::Presentation;
use kiln_deck_layout::print::{self as dp, DeckPrint, Layout, PrintOptions};
use kiln_deck_layout::render::{Options, Renderer};
use kiln_io_pdf::printable::{
    ArtifactKind, DocInfo, EncodedImage, ImageFormat, LabelStyle, Mark, OutlineEntry, PageCanvas,
    PageInfo, PageLabel, PrintImage, PrintablePages, StructId, StructKind, StructNode, StructTree,
    TextRun,
};
use kiln_protocol::commands::deck_features::{DeckFile, DeckPictureFormat};
use kiln_protocol::print::{PrintParity, PrintSettings, SlideLayout, SlidePrint};
use kiln_scene::{DrawOp, ImageId};
use rustc_hash::FxHashMap;

use super::super::DeckDoc;
use crate::print::{PrintJob, PrintOutput};

/// A presentation's printed pages as `kiln-io-pdf` takes them.
pub struct DeckPages<'p> {
    print: &'p DeckPrint<'p>,
    tree: StructTree,
    images: FxHashMap<u32, Arc<PrintImage>>,
}

impl std::fmt::Debug for DeckPages<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DeckPages")
            .field("print", self.print)
            .finish_non_exhaustive()
    }
}

impl<'p> DeckPages<'p> {
    /// Wraps laid-out pages.
    #[must_use]
    pub fn new(print: &'p DeckPrint<'p>) -> Self {
        let tree = StructTree {
            nodes: print
                .structure()
                .nodes
                .iter()
                .map(|n| StructNode {
                    kind: match &n.kind {
                        dp::StructKind::Section => StructKind::Section,
                        dp::StructKind::Div => StructKind::Div,
                        dp::StructKind::P => StructKind::P,
                        dp::StructKind::Heading { level, title } => StructKind::Heading {
                            level: *level,
                            title: title.clone(),
                        },
                        dp::StructKind::Figure { alt } => StructKind::Figure { alt: alt.clone() },
                    },
                    parent: n.parent.map(|p| StructId(p.0)),
                })
                .collect(),
        };
        let images = print
            .pictures()
            .map(|p| {
                let source = p.file.as_ref().and_then(|f| {
                    Some(EncodedImage {
                        format: ImageFormat::sniff(f)?,
                        data: f.clone(),
                    })
                });
                (
                    p.id.0,
                    Arc::new(PrintImage {
                        id: p.id,
                        width: p.width,
                        height: p.height,
                        rgba: p.rgba.clone(),
                        source,
                    }),
                )
            })
            .collect();
        Self {
            print,
            tree,
            images,
        }
    }
}

/// The PDF canvas behind the deck's page sink.
struct Canvas<'c>(&'c mut dyn PageCanvas);

impl dp::PageSink for Canvas<'_> {
    fn op(&mut self, op: &DrawOp) {
        self.0.op(op);
    }

    fn text(&mut self, run: &dp::TextRun<'_>) {
        self.0.text(&TextRun {
            fonts: run.fonts,
            run: run.run,
            glyphs: run.glyphs.clone(),
            origin: run.origin,
            text: run.text,
            paint: run.paint,
            actual: None,
        });
    }

    fn begin(&mut self, mark: dp::Mark) {
        self.0.begin(match mark {
            dp::Mark::Content(id) => Mark::Content(StructId(id.0)),
            dp::Mark::Artifact(dp::ArtifactKind::Footer) => Mark::Artifact(ArtifactKind::Footer),
            dp::Mark::Artifact(dp::ArtifactKind::Layout) => Mark::Artifact(ArtifactKind::Layout),
        });
    }

    fn end(&mut self) {
        self.0.end();
    }
}

impl PrintablePages for DeckPages<'_> {
    fn pages(&self) -> Vec<PageInfo> {
        let (width, height) = self.print.page_size();
        (0..self.print.page_count())
            .map(|i| PageInfo {
                width,
                height,
                label: PageLabel {
                    style: LabelStyle::Decimal,
                    prefix: String::new(),
                    number: u32::try_from(i + 1).unwrap_or(u32::MAX),
                },
            })
            .collect()
    }

    fn info(&self) -> DocInfo {
        DocInfo {
            title: self.print.title().map(str::to_owned),
            creator: Some("Kiln Deck".into()),
            ..DocInfo::default()
        }
    }

    fn structure(&self) -> StructTree {
        self.tree.clone()
    }

    fn outline(&self) -> Vec<OutlineEntry> {
        self.print
            .bookmarks()
            .into_iter()
            .map(|b| OutlineEntry {
                level: 1,
                title: b.title,
                page: b.page,
                y: b.y,
            })
            .collect()
    }

    fn image(&self, id: ImageId) -> Option<Arc<PrintImage>> {
        self.images.get(&id.0).cloned()
    }

    fn paint(&self, index: usize, canvas: &mut dyn PageCanvas) {
        self.print.paint(index, &mut Canvas(canvas));
    }
}

/// The layout of the settings' slide options.
#[must_use]
pub fn layout_of(s: Option<&SlidePrint>) -> Layout {
    let s = s.copied().unwrap_or_default();
    match s.layout {
        SlideLayout::FullPage => Layout::Slides,
        SlideLayout::NotesPages => Layout::Notes,
        SlideLayout::Outline => Layout::Outline,
        SlideLayout::Handouts => Layout::handouts(s.per_page, s.vertical),
    }
}

/// What `settings` print of a presentation of `count` slides: the print options (the page box
/// and odd/even resolved to slides) and the settings left for the PDF writer (every page).
///
/// # Errors
///
/// `InvalidInput` for a page box that cannot be read, or one that picks no slide.
pub fn print_options(
    settings: &PrintSettings,
    count: usize,
    title: Option<&str>,
) -> Result<(PrintOptions, PrintSettings)> {
    let o = crate::print::pdf_options(settings, None)?;
    let slides = o.pages_of(count);
    if slides.is_empty() && count > 0 {
        return Err(KilnError::InvalidInput(
            "the slide numbers do not name any slide".into(),
        ));
    }
    let s = settings.slides.unwrap_or_default();
    let opts = PrintOptions {
        layout: layout_of(Some(&s)),
        frame: s.frame,
        hidden: s.hidden,
        slides: if slides.len() == count {
            Vec::new()
        } else {
            slides
        },
        title: title.map(str::to_owned),
        today: None,
    };
    let mut rest = settings.clone();
    rest.pages.clear();
    rest.parity = PrintParity::All;
    Ok((opts, rest))
}

/// Exports a presentation to PDF with print settings (the CLI's `convert … out.pdf`).
///
/// # Errors
///
/// As [`print_options`], and the PDF writer's errors.
pub fn export_pdf(
    renderer: &mut Renderer,
    pres: &Presentation,
    settings: &PrintSettings,
    title: Option<&str>,
) -> Result<(Vec<u8>, usize)> {
    let (opts, rest) = print_options(settings, pres.slides.len(), title)?;
    let print = DeckPrint::new(renderer, pres, opts);
    let pages = DeckPages::new(&print);
    let options = crate::print::pdf_options(&rest, title)?;
    let pdf = kiln_io_pdf::export(&pages, &options)?;
    Ok((pdf, print.page_count()))
}

/// Slides as pictures `width` px wide.
///
/// # Errors
///
/// `InvalidInput` for slides that do not exist; `Internal` when a picture cannot be encoded.
pub fn export_pictures(
    renderer: &mut Renderer,
    pres: &Presentation,
    slides: Option<&[u32]>,
    format: DeckPictureFormat,
    width: u32,
    stem: &str,
) -> Result<DeckFile> {
    let n = pres.slides.len();
    let wanted: Vec<usize> = match slides {
        None => (0..n).collect(),
        Some(list) => {
            list.iter()
                .map(|i| {
                    usize::try_from(*i).ok().filter(|i| *i < n).ok_or_else(|| {
                        KilnError::InvalidInput(format!("there is no slide {}", i + 1))
                    })
                })
                .collect::<Result<_>>()?
        }
    };
    if wanted.is_empty() {
        return Err(KilnError::InvalidInput(
            "the presentation has no slides".into(),
        ));
    }
    let (ext, mime) = match format {
        DeckPictureFormat::Png => ("png", "image/png"),
        DeckPictureFormat::Jpeg => ("jpg", "image/jpeg"),
    };
    let slide_w = kiln_deck_layout::geom::px(pres.slide_size.cx).max(1.0);
    #[allow(clippy::cast_precision_loss)]
    let w = width.clamp(16, 8192) as f32;
    let scale = w / slide_w;
    let mut pictures = Vec::with_capacity(wanted.len());
    for i in &wanted {
        let rgba = render(renderer, pres, pres.slides[*i].id, scale)?;
        pictures.push(encode(&rgba, format)?);
    }
    if wanted.len() == 1 {
        return Ok(DeckFile {
            name: format!("{stem}.{ext}"),
            mime: mime.into(),
            bytes: pictures.pop().unwrap_or_default(),
        });
    }
    // A folder of Slide1.png … in a .zip (the web saves one file).
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (i, bytes) in wanted.iter().zip(&pictures) {
        let fail = |e: &dyn std::fmt::Display| KilnError::Internal(format!("zip: {e}"));
        zip.start_file(format!("{stem}/Slide{}.{ext}", i + 1), stored)
            .map_err(|e| fail(&e))?;
        zip.write_all(bytes).map_err(|e| fail(&e))?;
    }
    let bytes = zip
        .finish()
        .map_err(|e| KilnError::Internal(format!("zip: {e}")))?
        .into_inner();
    Ok(DeckFile {
        name: format!("{stem}.zip"),
        mime: "application/zip".into(),
        bytes,
    })
}

/// A slide drawn at `scale` device px per slide px.
fn render(
    renderer: &mut Renderer,
    pres: &Presentation,
    slide: kiln_common::NodeId,
    scale: f32,
) -> Result<kiln_raster::Rgba> {
    let delta =
        kiln_deck_layout::scene::slide_delta(renderer, pres, slide, scale, &Options::default());
    let slide_w = kiln_deck_layout::geom::px(pres.slide_size.cx).max(1.0);
    let slide_h = kiln_deck_layout::geom::px(pres.slide_size.cy).max(1.0);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (
        (slide_w * scale).round().clamp(1.0, 16_384.0) as u32,
        (slide_h * scale).round().clamp(1.0, 16_384.0) as u32,
    );
    kiln_raster::render_delta(
        &delta,
        &kiln_raster::View {
            width: w,
            height: h,
            scroll_x: 0.0,
            scroll_y: 0.0,
            options: kiln_raster::PaintOptions {
                caret_visible: false,
                touch: false,
            },
        },
        &kiln_scene::Palette::light().for_app("deck"),
    )
    .map_err(|e| KilnError::Internal(e.to_string()))
}

/// A slide drawn at `scale` as PNG (Design Ideas and Reuse Slides thumbnails).
///
/// # Errors
///
/// `Internal` when the slide cannot be drawn or encoded.
pub fn render_png(
    renderer: &mut Renderer,
    pres: &Presentation,
    slide: kiln_common::NodeId,
    scale: f32,
) -> Result<Vec<u8>> {
    encode(
        &render(renderer, pres, slide, scale)?,
        DeckPictureFormat::Png,
    )
}

/// A rendered slide as PNG, or as JPEG on white.
fn encode(rgba: &kiln_raster::Rgba, format: DeckPictureFormat) -> Result<Vec<u8>> {
    match format {
        DeckPictureFormat::Png => rgba.encode_png().map_err(KilnError::Internal),
        DeckPictureFormat::Jpeg => {
            let mut rgb = Vec::with_capacity(rgba.data.len() / 4 * 3);
            for px in rgba.data.as_chunks::<4>().0 {
                let a = u16::from(px[3]);
                for c in &px[..3] {
                    #[allow(clippy::cast_possible_truncation)]
                    rgb.push(((u16::from(*c) * a + 255 * (255 - a) + 127) / 255) as u8);
                }
            }
            let mut out = std::io::Cursor::new(Vec::new());
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 92)
                .encode(
                    &rgb,
                    rgba.width,
                    rgba.height,
                    image::ExtendedColorType::Rgb8,
                )
                .map_err(|e| KilnError::Internal(e.to_string()))?;
            Ok(out.into_inner())
        }
    }
}

impl DeckDoc {
    /// Print preview and PDF (`DocModel::print`).
    pub(crate) fn print_job(&self, job: &PrintJob<'_>) -> Result<PrintOutput> {
        let title = match job {
            PrintJob::Pdf { title, .. } => Some(*title).filter(|t| !t.is_empty()),
            PrintJob::Preview { .. } => None,
        };
        let (opts, rest) = print_options(job.settings(), self.pres.slides.len(), title)?;
        let mut renderer = self
            .renderer
            .try_borrow_mut()
            .map_err(|_| KilnError::Internal("the renderer is in use".into()))?;
        let print = DeckPrint::new(&mut renderer, &self.pres, opts);
        let pages = DeckPages::new(&print);
        let job = match *job {
            PrintJob::Preview { sheet, scale, .. } => PrintJob::Preview {
                settings: &rest,
                sheet,
                scale,
            },
            PrintJob::Pdf { title, .. } => PrintJob::Pdf {
                settings: &rest,
                title,
            },
        };
        crate::print::run(&job, &pages)
    }

    /// The name exported files take: the file's name without its extension.
    pub(crate) fn export_stem(&self) -> String {
        let name = self
            .file_name()
            .map(|p| p.rsplit(['/', '\\']).next().unwrap_or(p).to_owned())
            .unwrap_or_default();
        let stem = name
            .rsplit_once('.')
            .map_or(name.as_str(), |(s, _)| s)
            .trim()
            .to_owned();
        if stem.is_empty() {
            "Presentation".into()
        } else {
            stem
        }
    }

    /// File ▸ Export ▸ Change File Type.
    pub(crate) fn export_pictures(
        &self,
        slides: Option<&[u32]>,
        format: DeckPictureFormat,
        width: u32,
    ) -> Result<DeckFile> {
        let mut renderer = self
            .renderer
            .try_borrow_mut()
            .map_err(|_| KilnError::Internal("the renderer is in use".into()))?;
        export_pictures(
            &mut renderer,
            &self.pres,
            slides,
            format,
            width,
            &self.export_stem(),
        )
    }
}
