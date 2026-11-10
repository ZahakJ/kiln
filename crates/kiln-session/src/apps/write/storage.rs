// SPDX-License-Identifier: AGPL-3.0-or-later
//! A Write document as a native bundle (`.kwrite`) and in the autosave journal.
//!
//! The bundle's `model.bin` is the document (`KDOC`); the file it was opened from goes to
//! `passthrough/original.docx`, and the parts the edits since opening touched to
//! `passthrough/touched.json`, so that saving it as a Word document again writes exactly what a
//! save without the bundle would: the touched parts from the model, every other part (custom
//! XML, macros, settings and styles with markup Kiln does not model) byte for byte from the
//! original. The journal's checkpoints are bundles too, so a recovered document saves the same
//! way. The journal records every transaction the editor applies (where it records them for
//! collaboration, plus Undo and Redo) and replays them on the checkpoint.

use std::collections::BTreeSet;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::{Document, Pos, Transaction};
use kiln_io_docx::{TouchedParts, read_docx};
use kiln_protocol::DocKind;
use kiln_storage::Bundle;
use serde::{Deserialize, Serialize};

use super::{WriteDoc, save};

/// The passthrough entry of the original Word file.
const ORIGINAL: &str = "original.docx";
/// The passthrough entry of the parts touched since the original was opened.
const TOUCHED: &str = "touched.json";

/// [`TouchedParts`] as kept in a bundle.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Touched {
    everything: bool,
    body: bool,
    headers_footers: BTreeSet<u64>,
    footnotes: bool,
    endnotes: bool,
    comments: bool,
    styles: bool,
    numbering: bool,
    settings: bool,
    theme: bool,
    properties: bool,
}

impl From<&TouchedParts> for Touched {
    fn from(t: &TouchedParts) -> Self {
        Self {
            everything: t.everything,
            body: t.body,
            headers_footers: t.headers_footers.iter().map(|n| n.0).collect(),
            footnotes: t.footnotes,
            endnotes: t.endnotes,
            comments: t.comments,
            styles: t.styles,
            numbering: t.numbering,
            settings: t.settings,
            theme: t.theme,
            properties: t.properties,
        }
    }
}

impl From<Touched> for TouchedParts {
    fn from(t: Touched) -> Self {
        Self {
            everything: t.everything,
            body: t.body,
            headers_footers: t.headers_footers.into_iter().map(NodeId).collect(),
            footnotes: t.footnotes,
            endnotes: t.endnotes,
            comments: t.comments,
            styles: t.styles,
            numbering: t.numbering,
            settings: t.settings,
            theme: t.theme,
            properties: t.properties,
        }
    }
}

fn doc_err(e: impl std::fmt::Display) -> KilnError {
    KilnError::Format {
        format: "kwrite".into(),
        message: e.to_string(),
    }
}

impl WriteDoc {
    /// Records a transaction for the journal (while it records).
    pub(super) fn record_journal(&mut self, tx: &Transaction) {
        if let Some(j) = &mut self.journal
            && let Ok(bytes) = tx.to_bytes()
        {
            j.push(bytes);
        }
    }

    /// The document as a bundle (the document itself while a shared version is shown).
    pub(super) fn to_bundle(&self) -> Result<Bundle> {
        let doc = self.live_document();
        let mut b = Bundle::new(DocKind::Write, doc.to_bytes().map_err(doc_err)?);
        if let Some(o) = &self.original {
            b.passthrough.insert(ORIGINAL.into(), o.bytes().to_vec());
            let touched = serde_json::to_vec(&Touched::from(&self.touched)).map_err(doc_err)?;
            b.passthrough.insert(TOUCHED.into(), touched);
        }
        Ok(b)
    }

    /// The document of a bundle: saving it as a Word document writes the parts touched since
    /// the original was opened from the model and copies every other part of the original (a
    /// bundle without that record has every part written from the model). An original that
    /// cannot be read is a loss the Compatibility Checker reports.
    pub(super) fn from_bundle(bundle: &Bundle, name: Option<String>) -> Result<Self> {
        let doc = Document::from_bytes(&bundle.model).map_err(doc_err)?;
        let mut w = Self::new(doc, Vec::new(), name);
        if let Some(bytes) = bundle.passthrough.get(ORIGINAL) {
            match read_docx(bytes) {
                Ok(read) => {
                    let (_, warnings, original) = save::Original::from_read(read, bytes);
                    w.warnings = warnings;
                    w.original = Some(original);
                    w.touched = bundle
                        .passthrough
                        .get(TOUCHED)
                        .and_then(|t| serde_json::from_slice::<Touched>(t).ok())
                        .map_or_else(TouchedParts::all, TouchedParts::from);
                }
                Err(_) => w.original_lost = true,
            }
        }
        Ok(w)
    }

    /// Starts or stops recording for the journal.
    pub(super) fn set_journaling(&mut self, on: bool) {
        self.journal = on.then(Vec::new);
    }

    /// The transactions recorded since the last call.
    pub(super) fn take_journal(&mut self) -> Option<Vec<Vec<u8>>> {
        self.journal.as_mut().map(std::mem::take)
    }

    /// Applies one recorded transaction (recovery); the selection follows it.
    pub(super) fn apply_journal(&mut self, op: &[u8]) -> Result<()> {
        let tx = Transaction::from_bytes(op).map_err(doc_err)?;
        let after = tx.selection_after;
        self.doc.apply(tx.clone()).map_err(doc_err)?;
        self.touched.record(&self.doc, &tx);
        self.note_edit(&tx);
        self.record_journal(&tx);
        let (anchor, head) = after.map_or((self.anchor, self.head), |r| (r.anchor, r.head));
        self.anchor = self.kept(anchor);
        self.head = self.kept(head);
        Ok(())
    }

    /// A position kept inside the document.
    fn kept(&self, p: Pos) -> Pos {
        match self.doc.paragraph(p.para) {
            Some(para) => Pos::new(p.para, p.offset.min(para.content.len())),
            None => Pos::new(
                self.doc
                    .body_paragraphs()
                    .first()
                    .map_or(NodeId(0), |p| p.id),
                0,
            ),
        }
    }
}

/// the comparison of a local version with the document, through the same word-level
/// diff as shared documents' Version History.
pub(super) fn compare(
    old: &[u8],
    now: &Document,
) -> Result<Vec<kiln_protocol::collab::VersionParagraph>> {
    use kiln_protocol::collab::{VersionChange, VersionParagraph, VersionSpan};
    let change = |c: kiln_collab::Change| match c {
        kiln_collab::Change::Same => VersionChange::Same,
        kiln_collab::Change::Added => VersionChange::Added,
        kiln_collab::Change::Removed => VersionChange::Removed,
    };
    let old = Document::from_bytes(old).map_err(doc_err)?;
    Ok(kiln_collab::word_diff(&old, now)
        .into_iter()
        .map(|p| VersionParagraph {
            change: change(p.change),
            spans: p
                .spans
                .into_iter()
                .map(|s| VersionSpan {
                    change: change(s.change),
                    text: s.text,
                })
                .collect(),
        })
        .collect())
}
