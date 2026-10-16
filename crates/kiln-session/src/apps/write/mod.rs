// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write documents: `.docx`, `.dotx` and `.docm` files read by `kiln-io-docx` into the
//! `kiln-doc` model (WP-62).
//!
//! This is the reader's entry into the session: a Write document opens, its paragraphs, word
//! count, accessibility tree and import diagnostics are available, and blank documents start
//! from Word's defaults. Painting arrives with the Write layout engine (WP-63, which owns this
//! module from here on and adds the frame source), editing commands with the Write editor
//! (WP-64) and saving with the DOCX writer (WP-65); until then those return `Unsupported` with
//! the reason, never a silent no-op.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{Block, Document, Paragraph};
use kiln_io_docx::{DocxError, DocxWarning, read_docx};
use kiln_protocol::{
    A11yNode, A11yRole, A11yTree, Command, DocKind, FileFormat, OpenHint, ParagraphText, Selection,
    StatusBar,
};

use crate::model::{AppFactory, Change, DocModel};
use crate::undo::StateId;

/// The formats Write opens and (with the writer) saves.
const FORMATS: [FileFormat; 3] = [FileFormat::Docx, FileFormat::Dotx, FileFormat::Docm];

/// Creates and opens Write documents.
#[derive(Debug, Clone, Copy, Default)]
pub struct WriteApp;

impl AppFactory for WriteApp {
    fn kind(&self) -> DocKind {
        DocKind::Write
    }

    fn opens(&self, format: FileFormat) -> bool {
        FORMATS.contains(&format)
    }

    fn create(&self) -> Box<dyn DocModel> {
        Box::new(WriteDoc {
            doc: Document::new(),
            warnings: Vec::new(),
        })
    }

    fn open(
        &self,
        bytes: &[u8],
        format: FileFormat,
        _hint: &OpenHint,
    ) -> Result<Box<dyn DocModel>> {
        if !self.opens(format) {
            return Err(KilnError::Unsupported(format!(
                ".{} files in Kiln Write",
                format.extension()
            )));
        }
        let read = read_docx(bytes).map_err(|e| KilnError::Format {
            format: format.extension().to_owned(),
            message: match e {
                DocxError::Package(p) => p.to_string(),
                DocxError::NotWord(m) => format!("not a Word document ({m})"),
            },
        })?;
        Ok(Box::new(WriteDoc {
            doc: read.document,
            warnings: read.warnings,
        }))
    }
}

/// A Write document held by the session.
#[derive(Debug)]
pub struct WriteDoc {
    doc: Document,
    /// What the reader reported (the Compatibility Checker's input).
    warnings: Vec<DocxWarning>,
}

impl WriteDoc {
    /// The document.
    #[must_use]
    pub fn document(&self) -> &Document {
        &self.doc
    }

    /// The import diagnostics.
    #[must_use]
    pub fn warnings(&self) -> &[DocxWarning] {
        &self.warnings
    }
}

fn flatten<'a>(blocks: &'a [Block], out: &mut Vec<&'a Paragraph>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(p),
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        flatten(&cell.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => flatten(&s.blocks, out),
            Block::Field(f) => flatten(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

impl WriteDoc {
    /// Body paragraphs in reading order (text boxes excluded).
    fn body(&self) -> Vec<&Paragraph> {
        let mut out = Vec::new();
        for s in &self.doc.sections {
            flatten(&s.blocks, &mut out);
        }
        out
    }
}

impl DocModel for WriteDoc {
    fn kind(&self) -> DocKind {
        DocKind::Write
    }

    fn execute(&mut self, command: &Command) -> Result<Change> {
        Err(KilnError::Unsupported(format!(
            "{} in Kiln Write: editing arrives with the Write editor (WP-64)",
            command.id()
        )))
    }

    fn undo(&mut self) -> bool {
        false
    }

    fn redo(&mut self) -> bool {
        false
    }

    fn can_undo(&self) -> bool {
        false
    }

    fn can_redo(&self) -> bool {
        false
    }

    fn undo_label(&self) -> Option<&'static str> {
        None
    }

    fn redo_label(&self) -> Option<&'static str> {
        None
    }

    fn state(&self) -> StateId {
        0
    }

    fn seal_history(&mut self) {}

    fn selection(&self) -> Selection {
        Selection::default()
    }

    fn status(&self) -> StatusBar {
        let stats = self.doc.statistics(Default::default());
        StatusBar {
            words: Some(u32::try_from(stats.words).unwrap_or(u32::MAX)),
            chars: Some(u32::try_from(stats.characters_with_spaces).unwrap_or(u32::MAX)),
            ..StatusBar::default()
        }
    }

    fn paragraphs(&self) -> Option<Vec<ParagraphText>> {
        Some(
            self.body()
                .into_iter()
                .map(|p| ParagraphText {
                    id: p.id,
                    text: p.content.plain_text(false),
                })
                .collect(),
        )
    }

    fn a11y(&self, title: &str) -> A11yTree {
        let body = self.body();
        let root = self.doc.sections.first().map_or(NodeId(0), |s| s.id);
        let mut nodes = Vec::with_capacity(body.len() + 1);
        nodes.push(A11yNode {
            id: root,
            role: A11yRole::Document,
            name: Some(title.to_owned()),
            text: None,
            bounds: None,
            level: None,
            children: body.iter().map(|p| p.id).collect(),
        });
        for p in body {
            let level = self.doc.resolve_paragraph(p).outline_level;
            let heading = level < 9;
            nodes.push(A11yNode {
                id: p.id,
                role: if heading {
                    A11yRole::Heading
                } else {
                    A11yRole::Paragraph
                },
                name: None,
                text: Some(p.content.plain_text(true)),
                bounds: None,
                level: heading.then_some(level + 1),
                children: Vec::new(),
            });
        }
        A11yTree {
            nodes,
            active: None,
            announce: None,
            caret: None,
        }
    }

    fn save_formats(&self) -> &'static [FileFormat] {
        &FORMATS
    }

    fn save(&self, format: FileFormat) -> Result<Vec<u8>> {
        Err(KilnError::Unsupported(format!(
            "saving .{} files: the DOCX writer arrives with WP-65",
            format.extension()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/docx/features")
            .join(name);
        std::fs::read(path).expect("fixture")
    }

    #[test]
    fn opens_word_documents_into_the_write_model() {
        let app = WriteApp;
        assert!(app.opens(FileFormat::Docx) && app.opens(FileFormat::Dotx));
        assert!(!app.opens(FileFormat::Txt));
        let doc = app
            .open(
                &fixture("long-text.docx"),
                FileFormat::Docx,
                &OpenHint::default(),
            )
            .expect("opens");
        assert_eq!(doc.kind(), DocKind::Write);
        let paras = doc.paragraphs().expect("paragraphs");
        assert!(paras.len() > 20);
        assert!(doc.status().words.is_some_and(|w| w > 100));
        let tree = doc.a11y("Long text");
        assert_eq!(tree.nodes[0].role, A11yRole::Document);
        assert_eq!(
            tree.nodes
                .iter()
                .filter(|n| n.role == A11yRole::Heading)
                .count(),
            20
        );
        assert!(doc.save(FileFormat::Docx).is_err());
        assert_eq!(doc.save_formats()[0], FileFormat::Docx);
    }

    #[test]
    fn damaged_files_are_format_errors_and_new_documents_start_blank() {
        let app = WriteApp;
        let err = app
            .open(b"not a zip", FileFormat::Docx, &OpenHint::default())
            .unwrap_err();
        assert!(matches!(err, KilnError::Format { .. }), "{err}");
        assert!(
            app.open(b"x", FileFormat::Txt, &OpenHint::default())
                .is_err()
        );
        let mut blank = app.create();
        assert_eq!(blank.paragraphs().map(|p| p.len()), Some(1));
        assert!(!blank.can_undo() && !blank.undo() && !blank.redo());
        let cmd = Command::Write(kiln_protocol::WriteCommand::SelectAll);
        assert!(matches!(
            blank.execute(&cmd),
            Err(KilnError::Unsupported(_))
        ));
    }
}
