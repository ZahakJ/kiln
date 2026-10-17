// SPDX-License-Identifier: AGPL-3.0-or-later
//! Print, print preview and PDF export in the session (WP-67).
//!
//! - `Query::PrintPreview { settings, sheet, scale }`: one sheet as a Scene for the Surface
//!   painter (Backstage ▸ Print).
//! - `Query::Pdf { settings }`: the PDF bytes (the web build downloads them for Export ▸ PDF).
//! - `SuiteCommand::SavePdf { path, settings }`: Export ▸ PDF where files are the core's to write
//!   (desktop): a save dialog when there is no path, then the file.
//! - `SuiteCommand::PrintPdf { settings }`: Print. Natively the PDF is written to a temporary
//!   file that the host opens in the system's PDF viewer, which prints it through the system
//!   print dialog (`HostOp::OpenFile`); the web build hands the bytes to the page, which prints
//!   them from a hidden frame (`HostOp::PrintPdf`). Android hands the file to `PrintManager`
//!   (WP-89).
//!
//! Apps print through [`DocModel::print`](crate::DocModel::print), implemented with
//! [`run`] over their [`PrintablePages`]. Kiln Write lays the document out afresh for print
//! (markup, hidden text and page setup overrides change the layout) and keeps that layout for
//! the next preview until the document or those settings change ([`PrintCache`]).

use std::cell::RefCell;

use kiln_common::{KilnError, Result};
use kiln_doc::Document;
use kiln_doc_layout::DocLayout;
use kiln_io_pdf::write::{PrintOrientation as WriteOrientation, WriteSettings};
use kiln_io_pdf::{PageParity, PageSelection, Paper, PdfOptions, PrintablePages, Scaling};
use kiln_protocol::print::{
    PdfFile, PrintOrientation, PrintPaper, PrintParity, PrintPreview, PrintSettings,
};

/// What a model is asked to print.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PrintJob<'a> {
    /// One sheet of the preview at `scale` device px per preview px.
    Preview {
        /// The settings.
        settings: &'a PrintSettings,
        /// The sheet (0-based).
        sheet: u32,
        /// Device scale.
        scale: f32,
    },
    /// The PDF.
    Pdf {
        /// The settings.
        settings: &'a PrintSettings,
        /// The document title (window title of PDF viewers when the document has none).
        title: &'a str,
    },
}

impl PrintJob<'_> {
    /// The settings.
    #[must_use]
    pub fn settings(&self) -> &PrintSettings {
        match self {
            Self::Preview { settings, .. } | Self::Pdf { settings, .. } => settings,
        }
    }
}

/// What a print job produced.
#[derive(Debug, Clone, PartialEq)]
pub enum PrintOutput {
    /// A preview sheet.
    Preview(PrintPreview),
    /// A PDF (`name` is filled in by the session).
    Pdf(PdfFile),
}

/// The PDF options of print settings.
///
/// # Errors
///
/// `InvalidInput` for a page box that cannot be read or numbers out of range.
pub fn pdf_options(s: &PrintSettings, title: Option<&str>) -> Result<PdfOptions> {
    let options = PdfOptions {
        pages: PageSelection::parse(&s.pages)?,
        parity: match s.parity {
            PrintParity::All => PageParity::All,
            PrintParity::Odd => PageParity::Odd,
            PrintParity::Even => PageParity::Even,
        },
        copies: s.copies,
        scaling: s.paper.map_or(Scaling::Actual, |p| {
            Scaling::FitTo(match p {
                PrintPaper::Letter => Paper::LETTER,
                PrintPaper::Legal => Paper::LEGAL,
                PrintPaper::A3 => Paper::A3,
                PrintPaper::A4 => Paper::A4,
                PrintPaper::A5 => Paper::A5,
            })
        }),
        scale: s.scale,
        per_sheet: s.per_sheet,
        grayscale: s.grayscale,
        tagged: s.tagged,
        bookmarks: s.bookmarks,
        title: title.map(str::to_owned),
    };
    options.validate()?;
    Ok(options)
}

/// The layout-changing part of print settings, for Kiln Write.
#[must_use]
pub fn write_settings(s: &PrintSettings) -> WriteSettings {
    WriteSettings {
        markup: s.markup,
        hidden_text: s.hidden_text,
        orientation: s.orientation.map(|o| match o {
            PrintOrientation::Portrait => WriteOrientation::Portrait,
            PrintOrientation::Landscape => WriteOrientation::Landscape,
        }),
        margins: s.margins.map(kiln_protocol::print::PrintMargins::twips),
    }
}

/// Runs a print job over a printable document.
///
/// # Errors
///
/// `InvalidInput` for bad settings or an empty selection; `Format` when the PDF writer fails.
pub fn run(job: &PrintJob<'_>, doc: &dyn PrintablePages) -> Result<PrintOutput> {
    match *job {
        PrintJob::Preview {
            settings,
            sheet,
            scale,
        } => {
            let options = pdf_options(settings, None)?;
            let page_count = doc.pages().len();
            let p = kiln_io_pdf::preview(doc, &options, sheet as usize, scale)?;
            let per_copy = p.sheets / options.copies.max(1) as usize;
            let n = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
            Ok(PrintOutput::Preview(PrintPreview {
                sheets: n(per_copy.max(1)),
                sheet: n(p.sheet.min(per_copy.saturating_sub(1))),
                width_pt: p.width_pt,
                height_pt: p.height_pt,
                pages: p.pages.into_iter().map(n).collect(),
                page_count: n(page_count),
                scene: p.delta,
            }))
        }
        PrintJob::Pdf { settings, title } => {
            let options = pdf_options(settings, (!title.is_empty()).then_some(title))?;
            let e = kiln_io_pdf::export_report(doc, &options)?;
            Ok(PrintOutput::Pdf(PdfFile {
                name: String::new(),
                bytes: e.pdf,
                pdf_ua: e.pdf_ua,
                ua_issues: e.ua_issues,
            }))
        }
    }
}

/// A file name for the PDF of a document titled `title`.
#[must_use]
pub fn pdf_name(title: &str) -> String {
    let base: String = title
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                ' '
            } else {
                c
            }
        })
        .collect();
    let base = base.trim();
    format!("{}.pdf", if base.is_empty() { "Document" } else { base })
}

/// Writes the PDF to print into `<temp>/kiln-print/` (a fresh name per job, so a viewer still
/// showing the last one is not disturbed) and returns the path. Files older than a day are
/// removed on the way.
///
/// # Errors
///
/// `Io` when the temporary folder cannot be written.
#[cfg(not(target_arch = "wasm32"))]
pub fn write_temp(pdf: &PdfFile) -> Result<String> {
    let dir = std::env::temp_dir().join("kiln-print");
    std::fs::create_dir_all(&dir).map_err(KilnError::from)?;
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let day = std::time::Duration::from_secs(24 * 3600);
        for e in entries.flatten() {
            let old = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age > day);
            if old {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let name = pdf_name(pdf.name.trim_end_matches(".pdf"));
    let file = dir.join(format!("{}-{stamp}.pdf", name.trim_end_matches(".pdf")));
    std::fs::write(&file, &pdf.bytes).map_err(KilnError::from)?;
    Ok(file.to_string_lossy().into_owned())
}

/// What the print layout of a Write document was made from.
#[derive(Debug, Clone, PartialEq)]
struct Key {
    state: u64,
    settings: WriteSettings,
}

/// A Write document laid out for print, kept between previews.
#[derive(Debug)]
struct Prepared {
    key: Key,
    /// The document with print overrides applied (`None`: the document itself).
    doc: Option<Document>,
    layout: DocLayout,
}

/// The print layout of a Write document, kept until the document (its history state) or the
/// layout-changing settings change.
#[derive(Debug, Default)]
pub struct PrintCache {
    prepared: RefCell<Option<Prepared>>,
}

impl PrintCache {
    /// An empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forgets the layout.
    pub fn clear(&self) {
        self.prepared.replace(None);
    }

    /// Runs `job` on `doc` (at history state `state`), laying it out for print when needed.
    ///
    /// # Errors
    ///
    /// As [`run`].
    pub fn print_write(
        &self,
        doc: &Document,
        state: u64,
        file_name: Option<&str>,
        title: &str,
        job: &PrintJob<'_>,
    ) -> Result<PrintOutput> {
        let settings = write_settings(job.settings());
        let key = Key { state, settings };
        let mut slot = self
            .prepared
            .try_borrow_mut()
            .map_err(|_| KilnError::Internal("print layout in use".into()))?;
        if slot.as_ref().is_none_or(|p| p.key != key) {
            let printed = kiln_io_pdf::write::print_document(doc, &key.settings);
            let owned = match printed {
                std::borrow::Cow::Borrowed(_) => None,
                std::borrow::Cow::Owned(d) => Some(d),
            };
            let layout = kiln_io_pdf::write::print_layout(
                owned.as_ref().unwrap_or(doc),
                crate::apps::scratch::fonts::store(),
                key.settings.markup,
                file_name.map(str::to_owned),
            );
            *slot = Some(Prepared {
                key,
                doc: owned,
                layout,
            });
        }
        let Some(p) = slot.as_ref() else {
            return Err(KilnError::Internal("print layout missing".into()));
        };
        let d = p.doc.as_ref().unwrap_or(doc);
        let printable = kiln_io_pdf::write::WritePrint::new(d, &p.layout, Some(title.to_owned()));
        run(job, &printable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_map_to_pdf_options() {
        let mut s = PrintSettings {
            pages: "2-3".into(),
            paper: Some(PrintPaper::A4),
            per_sheet: 2,
            ..PrintSettings::default()
        };
        let o = pdf_options(&s, Some("T")).unwrap();
        assert_eq!(o.pages_of(5), vec![1, 2]);
        assert_eq!(o.scaling, Scaling::FitTo(Paper::A4));
        assert_eq!(o.title.as_deref(), Some("T"));
        s.pages = "x".into();
        assert!(pdf_options(&s, None).is_err());
        s.pages.clear();
        s.copies = 0;
        assert!(pdf_options(&s, None).is_err());
    }

    #[test]
    fn write_settings_carry_page_setup() {
        let s = PrintSettings {
            orientation: Some(PrintOrientation::Landscape),
            margins: Some(kiln_protocol::print::PrintMargins::Narrow),
            markup: false,
            ..PrintSettings::default()
        };
        let w = write_settings(&s);
        assert_eq!(w.orientation, Some(WriteOrientation::Landscape));
        assert_eq!(w.margins, Some([720; 4]));
        assert!(!w.markup);
    }

    #[test]
    fn pdf_names_are_safe() {
        assert_eq!(pdf_name("Q3: plan/draft"), "Q3  plan draft.pdf");
        assert_eq!(pdf_name("  "), "Document.pdf");
    }

    #[test]
    fn the_cache_lays_out_once_per_state() {
        let doc = Document::new();
        let cache = PrintCache::new();
        let settings = PrintSettings::default();
        let job = PrintJob::Preview {
            settings: &settings,
            sheet: 0,
            scale: 1.0,
        };
        let a = cache.print_write(&doc, 1, None, "Doc", &job).unwrap();
        let PrintOutput::Preview(p) = a else {
            panic!("a preview")
        };
        assert_eq!((p.sheets, p.page_count), (1, 1));
        assert!(p.scene.reset && p.scene.tiles.len() == 1);
        let pdf = cache
            .print_write(
                &doc,
                1,
                None,
                "Doc",
                &PrintJob::Pdf {
                    settings: &settings,
                    title: "Doc",
                },
            )
            .unwrap();
        let PrintOutput::Pdf(f) = pdf else {
            panic!("a pdf")
        };
        assert!(f.bytes.starts_with(b"%PDF-"));
    }

    fn fixture(name: &str) -> Vec<u8> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/docx/features")
            .join(name);
        std::fs::read(p).unwrap()
    }

    fn host_requests(s: &mut crate::Session) -> Vec<kiln_protocol::HostRequest> {
        s.poll()
            .into_iter()
            .filter_map(|e| match e {
                kiln_protocol::Event::HostRequest(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_session_previews_exports_and_prints() {
        use kiln_protocol::{
            Command, Event, FileFormat, HostOp, HostReply, OpenHint, Query, QueryResult,
            SuiteCommand,
        };
        let mut s = crate::Session::new(Box::new(crate::DeferredHost::new(|| 0)));
        let doc = s
            .open(
                &fixture("long-text.docx"),
                OpenHint {
                    name: Some("long-text.docx".into()),
                    format: Some(FileFormat::Docx),
                    ..OpenHint::default()
                },
            )
            .unwrap()
            .doc;
        let settings = PrintSettings {
            pages: "2-3".into(),
            ..PrintSettings::default()
        };
        let QueryResult::PrintPreview(p) = s
            .query(
                Some(doc),
                &Query::PrintPreview {
                    settings: settings.clone(),
                    sheet: 5,
                    scale: 2.0,
                },
            )
            .unwrap()
        else {
            panic!("a preview")
        };
        assert_eq!((p.sheets, p.sheet, p.pages.clone()), (2, 1, vec![2]));
        assert!(p.page_count >= 8 && p.scene.scale == 2.0);
        let bad = PrintSettings {
            pages: "3-1".into(),
            ..PrintSettings::default()
        };
        assert!(
            s.query(
                Some(doc),
                &Query::PrintPreview {
                    settings: bad,
                    sheet: 0,
                    scale: 1.0
                }
            )
            .is_err()
        );
        let QueryResult::Pdf(f) = s
            .query(
                Some(doc),
                &Query::Pdf {
                    settings: settings.clone(),
                },
            )
            .unwrap()
        else {
            panic!("a pdf")
        };
        assert_eq!(f.name, "long-text.pdf");
        assert!(f.bytes.starts_with(b"%PDF-"));

        // Export ▸ PDF: save dialog, write, Exported.
        s.poll();
        s.execute(
            Some(doc),
            Command::Suite(SuiteCommand::SavePdf {
                path: None,
                settings: settings.clone(),
            }),
        )
        .unwrap();
        let [pick] = host_requests(&mut s).try_into().unwrap();
        assert!(
            matches!(&pick.op, HostOp::PickSave { suggested_name, .. } if suggested_name == "long-text.pdf")
        );
        s.host_reply(pick.id, HostReply::Path("/out/x.pdf".into()))
            .unwrap();
        let [write] = host_requests(&mut s).try_into().unwrap();
        assert!(
            matches!(&write.op, HostOp::WriteFile { path, bytes } if path == "/out/x.pdf" && bytes.starts_with(b"%PDF-"))
        );
        s.host_reply(write.id, HostReply::Done).unwrap();
        assert!(s.poll().contains(&Event::Exported {
            doc,
            path: Some("/out/x.pdf".into())
        }));
        // A cancelled dialog says so.
        s.execute(
            Some(doc),
            Command::Suite(SuiteCommand::SavePdf {
                path: None,
                settings: settings.clone(),
            }),
        )
        .unwrap();
        let [pick] = host_requests(&mut s).try_into().unwrap();
        s.host_reply(pick.id, HostReply::Cancelled).unwrap();
        assert!(s.poll().contains(&Event::Exported { doc, path: None }));

        // Print: a temporary PDF the host opens.
        s.execute(
            Some(doc),
            Command::Suite(SuiteCommand::PrintPdf { settings }),
        )
        .unwrap();
        let [open] = host_requests(&mut s).try_into().unwrap();
        let HostOp::OpenFile { path } = &open.op else {
            panic!("{:?}", open.op)
        };
        let bytes = std::fs::read(path).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        let _ = std::fs::remove_file(path);
        s.host_reply(open.id, HostReply::Done).unwrap();
    }

    #[test]
    fn documents_that_do_not_print_say_so() {
        use kiln_protocol::{DocKind, Query};
        let mut s = crate::Session::new(Box::new(crate::DeferredHost::new(|| 0)));
        let doc = s.new_document(DocKind::Scratch).unwrap().doc;
        let e = s
            .query(
                Some(doc),
                &Query::Pdf {
                    settings: PrintSettings::default(),
                },
            )
            .unwrap_err();
        assert_eq!(e.kind(), kiln_common::ErrorKind::Unsupported);
    }
}
