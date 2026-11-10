// SPDX-License-Identifier: AGPL-3.0-or-later
//! Tracked changes: who and when (`w:ins`, `w:del`, `w:rPrChange`, `w:pPrChange`), and the
//! paragraph- and row-level revisions. Text-level revisions are [`crate::Mark`]s.

use serde::{Deserialize, Serialize};

use crate::props::{ParagraphProps, RunProps};

/// Author and date of a tracked change.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct RevisionInfo {
    /// `w:id` (unique per document part in DOCX; the writer renumbers).
    pub id: u32,
    /// `w:author`.
    pub author: String,
    /// `w:date`, ISO 8601 (absent in some files).
    pub date: Option<String>,
}

impl RevisionInfo {
    /// A revision record.
    #[must_use]
    pub fn new(id: u32, author: &str, date: Option<&str>) -> Self {
        Self {
            id,
            author: author.to_owned(),
            date: date.map(str::to_owned),
        }
    }
}

/// Whether a tracked change inserted or deleted something.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InsDel {
    /// Inserted.
    Insertion,
    /// Deleted.
    Deletion,
}

/// A tracked change of paragraph properties (`w:pPrChange`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ParagraphPropsChange {
    /// Who and when.
    pub info: RevisionInfo,
    /// The properties before the change.
    pub before: ParagraphProps,
}

/// A tracked change of run properties (`w:rPrChange`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RunPropsChange {
    /// Who and when.
    pub info: RevisionInfo,
    /// The properties before the change.
    pub before: RunProps,
}

/// A tracked insertion or deletion of a paragraph mark or a table row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsDelRevision {
    /// Who and when.
    pub info: RevisionInfo,
    /// Inserted or deleted.
    pub kind: InsDel,
}

/// Tracked changes attached to a paragraph.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ParagraphRevision {
    /// `w:pPrChange`.
    pub props_change: Option<Box<ParagraphPropsChange>>,
    /// `w:pPr/w:rPr/w:ins|w:del`: the paragraph mark itself was inserted or deleted.
    pub mark: Option<InsDelRevision>,
    /// `w:pPr/w:rPr/w:rPrChange`: the paragraph mark's run properties changed.
    pub mark_props_change: Option<Box<RunPropsChange>>,
}

impl ParagraphRevision {
    /// True when nothing is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.props_change.is_none() && self.mark.is_none() && self.mark_props_change.is_none()
    }
}

/// The kind of a tracked change in [`crate::query`] listings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RevisionKind {
    /// Inserted text.
    Insertion,
    /// Deleted text.
    Deletion,
    /// Text moved away.
    MoveFrom,
    /// Text moved here.
    MoveTo,
    /// Character formatting changed.
    RunFormat,
    /// Paragraph formatting changed.
    ParagraphFormat,
    /// A paragraph mark inserted (a paragraph split).
    ParagraphInsertion,
    /// A paragraph mark deleted (paragraphs joined).
    ParagraphDeletion,
    /// The paragraph mark's formatting changed.
    ParagraphMarkFormat,
    /// A table row inserted.
    RowInsertion,
    /// A table row deleted.
    RowDeletion,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn revision_records() {
        let r = RevisionInfo::new(3, "Ada", Some("2026-10-16T10:00:00Z"));
        assert_eq!(r.date.as_deref(), Some("2026-10-16T10:00:00Z"));
        let mut p = ParagraphRevision::default();
        assert!(p.is_empty());
        p.mark = Some(InsDelRevision {
            info: r,
            kind: InsDel::Insertion,
        });
        assert!(!p.is_empty());
    }
}
