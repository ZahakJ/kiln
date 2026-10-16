// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving Write documents (WP-65): `.docx`, `.dotx` and `.docm` through the `kiln-io-docx`
//! writer, and the Compatibility Checker.
//!
//! A document opened from a file keeps the file's bytes and what the reader returned beside the
//! model (passthrough parts, raw XML of unmodelled properties, relationship ids, package
//! properties), and records which parts its edits touch ([`TouchedParts`]). A save writes the
//! touched parts from the model and copies every other part — styles, numbering, headers, notes,
//! media, custom XML — byte for byte from the original.

use kiln_common::{KilnError, Result};
use kiln_doc::Document;
use kiln_io_docx::{
    CompatOptions, CompatOrigin as Origin, CompatSeverity as Severity, DocxExtras, DocxKind,
    DocxRead, DocxWarning, RelTarget, TouchedParts, WriteOptions, WriteSource, compat_report,
    w3cdtf, write_docx,
};
use kiln_ooxml::opc::OpcPackage;
use kiln_ooxml::passthrough::PassthroughSet;
use kiln_ooxml::props::DocumentProperties;
use kiln_protocol::{CompatIssue, CompatOrigin, CompatSeverity, FileFormat};
use std::collections::BTreeMap;

/// What a document read from a file keeps for saving.
#[derive(Debug)]
pub(crate) struct Original {
    /// The file as opened.
    bytes: Vec<u8>,
    passthrough: PassthroughSet,
    extras: DocxExtras,
    package_properties: DocumentProperties,
    relationships: BTreeMap<String, BTreeMap<String, RelTarget>>,
    main_part: String,
    kind: DocxKind,
}

impl Original {
    /// Splits a read into the document and what saving needs beside it.
    pub(crate) fn from_read(read: DocxRead, bytes: &[u8]) -> (Document, Vec<DocxWarning>, Self) {
        let DocxRead {
            document,
            passthrough,
            extras,
            package_properties,
            relationships,
            main_part,
            kind,
            warnings,
            ..
        } = read;
        (
            document,
            warnings,
            Self {
                bytes: bytes.to_vec(),
                passthrough,
                extras,
                package_properties,
                relationships,
                main_part,
                kind,
            },
        )
    }

    /// The format the file was read as.
    pub(crate) fn kind(&self) -> DocxKind {
        self.kind
    }
}

/// The Word format of a save format.
pub(crate) fn docx_kind(format: FileFormat) -> Option<DocxKind> {
    match format {
        FileFormat::Docx => Some(DocxKind::Document),
        FileFormat::Dotx => Some(DocxKind::Template),
        FileFormat::Docm => Some(DocxKind::MacroEnabledDocument),
        _ => None,
    }
}

/// Writes the document; `now_ms` stamps the modification date when given.
pub(crate) fn save(
    doc: &Document,
    original: Option<&Original>,
    touched: &TouchedParts,
    format: FileFormat,
    now_ms: Option<u64>,
) -> Result<Vec<u8>> {
    let kind = docx_kind(format).ok_or_else(|| {
        KilnError::Unsupported(format!("saving Write documents as .{}", format.extension()))
    })?;
    let options = |preserve| WriteOptions {
        kind: Some(kind),
        now: now_ms.map(|ms| w3cdtf(i64::try_from(ms / 1000).unwrap_or(i64::MAX))),
        preserve,
        ..WriteOptions::default()
    };
    let written = match original {
        Some(o) => {
            // The original package is read lazily: only the parts copied are inflated.
            let pkg = OpcPackage::open(&o.bytes).ok();
            let source = WriteSource {
                passthrough: &o.passthrough,
                extras: &o.extras,
                package_properties: &o.package_properties,
                relationships: &o.relationships,
                main_part: &o.main_part,
                kind: o.kind,
                package: pkg.as_ref(),
            };
            write_docx(doc, Some(&source), &options(Some(touched)))
        }
        None => write_docx(doc, None, &options(None)),
    };
    written
        .map(|w| w.bytes)
        .map_err(|e| KilnError::Internal(format!("the document could not be saved: {e}")))
}

/// The Compatibility Checker's list for a save in `format`.
pub(crate) fn compat(
    doc: &Document,
    original: Option<&Original>,
    warnings: &[DocxWarning],
    format: FileFormat,
) -> Vec<CompatIssue> {
    let report = compat_report(
        doc,
        CompatOptions {
            target: docx_kind(format),
            source: original.map(Original::kind),
            warnings,
        },
    );
    report
        .into_iter()
        .map(|i| CompatIssue {
            code: i.code,
            severity: match i.severity {
                Severity::Loss => CompatSeverity::Loss,
                Severity::Change => CompatSeverity::Change,
                Severity::Info => CompatSeverity::Info,
            },
            message: i.message,
            count: u32::try_from(i.count).unwrap_or(u32::MAX),
            origin: match i.origin {
                Origin::Save => CompatOrigin::Save,
                Origin::Open => CompatOrigin::Open,
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use kiln_protocol::{
        Command, CompatSeverity, DocId, Event, FileFormat, HostOp, HostReply, OpenHint, Outcome,
        Query, QueryResult, SuiteCommand, WriteCommand,
    };

    use super::super::WriteApp;
    use crate::model::{AppFactory, DocModel};
    use crate::{DeferredHost, Session};

    fn fixture(rel: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(rel);
        std::fs::read(path).expect("fixture")
    }

    fn open(rel: &str) -> Box<dyn DocModel> {
        WriteApp
            .open(&fixture(rel), FileFormat::Docx, &OpenHint::default())
            .expect("opens")
    }

    /// Parts whose bytes differ between two packages.
    fn changed(a: &[u8], b: &[u8]) -> Vec<String> {
        let (a, b) = (
            kiln_ooxml::opc::OpcPackage::open(a).unwrap(),
            kiln_ooxml::opc::OpcPackage::open(b).unwrap(),
        );
        a.all_part_names()
            .filter(|n| a.part(n).unwrap() != b.part(n).unwrap())
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn an_unedited_document_saves_with_every_model_part_unchanged() {
        let original = fixture("docx/features/sections.docx");
        let doc = open("docx/features/sections.docx");
        let saved = doc.save(FileFormat::Docx).expect("saves");
        // Only the document properties are rewritten (statistics, Kiln as the application).
        let changed = changed(&original, &saved);
        assert!(
            changed.iter().all(|p| p.starts_with("/docProps/")),
            "{changed:?}"
        );
    }

    #[test]
    fn typing_regenerates_the_body_only_and_the_text_is_saved() {
        let original = fixture("docx/features/headers-footers.docx");
        let mut doc = open("docx/features/headers-footers.docx");
        doc.execute(&Command::Write(WriteCommand::InsertText {
            text: "Saved by Kiln. ".into(),
        }))
        .expect("types");
        let saved = doc
            .save_at(FileFormat::Docx, 1_792_000_000_000)
            .expect("saves");
        let mut changed = changed(&original, &saved);
        changed.sort();
        assert_eq!(
            changed,
            [
                "/docProps/app.xml",
                "/docProps/core.xml",
                "/word/document.xml"
            ]
        );
        let back = kiln_io_docx::read_docx(&saved).expect("reads");
        assert!(
            back.document
                .body_paragraphs()
                .iter()
                .any(|p| p.text().contains("Saved by Kiln."))
        );
        assert_eq!(
            back.package_properties.core.modified.as_deref(),
            Some("2026-10-14T17:46:40Z")
        );
        // Undo and save: the body is written again (it was touched), with the original text.
        assert!(doc.undo());
        let undone = doc.save(FileFormat::Docx).expect("saves");
        let again = kiln_io_docx::read_docx(&undone).expect("reads");
        assert!(
            !again
                .document
                .body_paragraphs()
                .iter()
                .any(|p| p.text().contains("Saved by Kiln."))
        );
    }

    #[test]
    fn new_documents_save_and_other_formats_are_refused() {
        let doc = WriteApp.create();
        let bytes = doc.save(FileFormat::Docx).expect("saves");
        assert!(kiln_io_docx::read_docx(&bytes).is_ok());
        let template = doc.save(FileFormat::Dotx).expect("saves a template");
        assert_eq!(
            kiln_io_docx::read_docx(&template).unwrap().kind,
            kiln_io_docx::DocxKind::Template
        );
        assert!(doc.save(FileFormat::Txt).is_err());
        assert!(doc.compat_report(FileFormat::Docx).is_empty());
    }

    fn deferred() -> Box<DeferredHost> {
        Box::new(DeferredHost::new(|| 1_792_000_000_000))
    }

    #[test]
    fn save_as_through_the_session_writes_a_word_file_stamped_with_the_host_clock() {
        let mut s = Session::new(deferred());
        let opened = s
            .open(
                &fixture("docx/features/tables.docx"),
                OpenHint {
                    path: Some("/docs/tables.docx".into()),
                    ..OpenHint::default()
                },
            )
            .expect("opens");
        let doc: DocId = opened.doc;
        s.poll();
        let r = s
            .execute(
                Some(doc),
                SuiteCommand::SaveAs {
                    path: Some("/docs/copy.docx".into()),
                    format: None,
                }
                .into(),
            )
            .expect("saves");
        let Outcome::Pending(id) = r.outcome else {
            panic!("{r:?}")
        };
        let bytes = match s.poll().as_slice() {
            [Event::HostRequest(req)] => match &req.op {
                HostOp::WriteFile { path, bytes } => {
                    assert_eq!(path, "/docs/copy.docx");
                    bytes.clone()
                }
                other => panic!("{other:?}"),
            },
            other => panic!("{other:?}"),
        };
        s.host_reply(id, HostReply::Done).unwrap();
        let back = kiln_io_docx::read_docx(&bytes).expect("a Word file");
        assert_eq!(
            back.package_properties.core.modified.as_deref(),
            Some("2026-10-14T17:46:40Z")
        );
        assert!(matches!(s.poll().last(), Some(Event::Saved { .. })));
    }

    #[test]
    fn the_compatibility_report_is_a_query() {
        // A macro-enabled document saved as .docx loses its macros.
        let mut pkg =
            kiln_ooxml::opc::OpcPackage::open(&fixture("docx/features/fields.docx")).unwrap();
        pkg.set_content_type(
            "/word/document.xml",
            "application/vnd.ms-word.document.macroEnabled.main+xml",
        )
        .unwrap();
        let docm = pkg.save().unwrap();
        let mut s = Session::new(deferred());
        let opened = s
            .open(
                &docm,
                OpenHint {
                    name: Some("macros.docm".into()),
                    ..OpenHint::default()
                },
            )
            .expect("opens");
        let ask = |s: &Session, format| match s
            .query(Some(opened.doc), &Query::CompatReport { format })
            .unwrap()
        {
            QueryResult::CompatReport(list) => list,
            other => panic!("{other:?}"),
        };
        let as_docx = ask(&s, Some(FileFormat::Docx));
        let macros = as_docx
            .iter()
            .find(|i| i.code == "macros.removed")
            .expect("macros are reported");
        assert_eq!(macros.severity, CompatSeverity::Loss);
        assert!(
            ask(&s, Some(FileFormat::Docm))
                .iter()
                .all(|i| i.code != "macros.removed")
        );
        assert!(
            ask(&s, None).iter().all(|i| i.code != "macros.removed"),
            "its own format"
        );
    }
}
