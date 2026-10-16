// SPDX-License-Identifier: AGPL-3.0-or-later
//! Which parts of a document an editing session changed, recorded from its transactions, so a
//! save can copy every part nobody touched byte for byte from the original file (PLAN.md §3.10:
//! untouched parts are byte-identical after save).
//!
//! The record is conservative: an operation that might affect a part marks it, an undone edit
//! leaves its parts marked, and anything the recorder cannot place marks everything. Saving a
//! marked part regenerates it from the model, which is always correct; preserving is only ever an
//! optimisation of what is certainly unchanged.

use std::collections::BTreeSet;

use kiln_doc::st::NoteKind;
use kiln_doc::{Document, NodeId, Op, Story, Transaction};

/// The parts of a WordprocessingML package that edits can touch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TouchedParts {
    /// Everything is to be regenerated (an edit the recorder could not place).
    pub everything: bool,
    /// The main document part (body content, sections, header/footer references).
    pub body: bool,
    /// Header and footer stories, by node id.
    pub headers_footers: BTreeSet<NodeId>,
    /// `footnotes.xml`.
    pub footnotes: bool,
    /// `endnotes.xml`.
    pub endnotes: bool,
    /// `comments.xml` and its companions (threads, people).
    pub comments: bool,
    /// `styles.xml`.
    pub styles: bool,
    /// `numbering.xml`.
    pub numbering: bool,
    /// `settings.xml`.
    pub settings: bool,
    /// The theme.
    pub theme: bool,
    /// Document properties (`docProps/*`).
    pub properties: bool,
}

impl TouchedParts {
    /// Nothing touched.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Everything touched: a save regenerates every part from the model.
    #[must_use]
    pub fn all() -> Self {
        Self {
            everything: true,
            ..Self::default()
        }
    }

    /// True when no edit was recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Whether a header or footer story changed.
    #[must_use]
    pub fn header_footer(&self, id: NodeId) -> bool {
        self.everything || self.headers_footers.contains(&id)
    }

    /// Records the parts a transaction touches. Call it with the document *after* the
    /// transaction was applied (undo and redo apply transactions too: record those as well).
    pub fn record(&mut self, doc: &Document, tx: &Transaction) {
        for op in &tx.ops {
            self.record_op(doc, op);
        }
    }

    fn story(&mut self, doc: &Document, id: NodeId) {
        match doc.story_of(id) {
            Some(Story::Body(_)) => self.body = true,
            Some(Story::HeaderFooter(i)) => match doc.headers_footers.get(i) {
                Some(h) => {
                    self.headers_footers.insert(h.id);
                }
                None => self.everything = true,
            },
            Some(Story::Note(i)) => match doc.notes.get(i).map(|n| n.kind) {
                Some(NoteKind::Footnote) => self.footnotes = true,
                Some(NoteKind::Endnote) => self.endnotes = true,
                None => self.everything = true,
            },
            Some(Story::Comment(_)) => self.comments = true,
            None => self.everything = true,
        }
    }

    /// Records one operation.
    pub fn record_op(&mut self, doc: &Document, op: &Op) {
        match op {
            Op::InsertInline { para, .. }
            | Op::DeleteInline { para, .. }
            | Op::SetRuns { para, .. }
            | Op::AddMark { para, .. }
            | Op::RemoveMark { para, .. }
            | Op::SetMarks { para, .. }
            | Op::SetObject { para, .. }
            | Op::SetParagraphProps { para, .. }
            | Op::SetParagraphMarkProps { para, .. }
            | Op::SetParagraphRevision { para, .. }
            | Op::SplitParagraph { para, .. } => self.story(doc, *para),
            Op::MergeParagraphs { first, .. } => self.story(doc, *first),
            Op::InsertBlocks { container, .. } | Op::RemoveBlocks { container, .. } => {
                // A story's own id (section, header, note, comment) or a block container.
                if doc.sections.iter().any(|s| s.id == *container) {
                    self.body = true;
                } else if doc.headers_footers.iter().any(|h| h.id == *container) {
                    self.headers_footers.insert(*container);
                } else if let Some(n) = doc.notes.iter().find(|n| n.id == *container) {
                    match n.kind {
                        NoteKind::Footnote => self.footnotes = true,
                        NoteKind::Endnote => self.endnotes = true,
                    }
                } else if doc.comments.iter().any(|c| c.id == *container) {
                    self.comments = true;
                } else {
                    self.story(doc, *container);
                }
            }
            Op::SetTableProps { table, .. }
            | Op::SetTableGrid { table, .. }
            | Op::InsertRows { table, .. }
            | Op::RemoveRows { table, .. } => self.story(doc, *table),
            Op::SetRow { row, .. } | Op::InsertCells { row, .. } | Op::RemoveCells { row, .. } => {
                self.story(doc, *row);
            }
            Op::SetCell { cell, .. } => self.story(doc, *cell),
            Op::SetSectionProps { .. }
            | Op::SetSectionRefs { .. }
            | Op::SplitSection { .. }
            | Op::MergeSections { .. } => self.body = true,
            Op::InsertHeaderFooter { story, .. } => {
                self.headers_footers.insert(story.id);
            }
            Op::RemoveHeaderFooter { id } => {
                self.headers_footers.insert(*id);
                self.body = true;
            }
            Op::InsertNote { note, .. } => match note.kind {
                NoteKind::Footnote => self.footnotes = true,
                NoteKind::Endnote => self.endnotes = true,
            },
            Op::RemoveNote { .. } => {
                self.footnotes = true;
                self.endnotes = true;
            }
            Op::InsertComment { .. } | Op::RemoveComment { .. } | Op::SetCommentMeta { .. } => {
                self.comments = true;
            }
            Op::PutStyle { .. } | Op::DeleteStyle { .. } | Op::SetDocDefaults { .. } => {
                self.styles = true;
            }
            Op::SetAbstractNum { .. } | Op::SetNumInstance { .. } => self.numbering = true,
            Op::SetSettings { .. } => self.settings = true,
            Op::SetProperties { .. } => self.properties = true,
            Op::SetTheme { .. } => self.theme = true,
            // Media is written as the drawings that use it are.
            Op::PutMedia { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::{Fragment, RunProps};

    #[test]
    fn body_edits_touch_only_the_body() {
        let mut doc = Document::new();
        let para = doc.sections[0].blocks[0].id();
        let tx = Transaction::new(vec![Op::InsertInline {
            para,
            offset: 0,
            fragment: Fragment::text("Hello", RunProps::default()),
        }]);
        let _ = doc.apply(tx.clone());
        let mut t = TouchedParts::new();
        assert!(t.is_empty());
        t.record(&doc, &tx);
        assert!(t.body && !t.styles && !t.comments && !t.everything);
        assert!(!t.header_footer(NodeId(9999)));
        let mut all = TouchedParts::all();
        all.record_op(
            &doc,
            &Op::PutMedia {
                name: "/word/media/x.png".into(),
                media: None,
            },
        );
        assert!(all.header_footer(NodeId(1)));
    }

    #[test]
    fn unknown_nodes_touch_everything() {
        let doc = Document::new();
        let mut t = TouchedParts::new();
        t.record_op(
            &doc,
            &Op::SetMarks {
                para: NodeId(123_456),
                marks: Vec::new(),
            },
        );
        assert!(t.everything);
    }
}
