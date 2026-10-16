// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Compatibility Checker for saving as Word (Backstage ▸ Info ▸ Check for Issues ▸ Check
//! Compatibility): what in the document will be lost or changed by [`super::write_docx`],
//! grouped by feature with a count of occurrences, plus what was already lost or kept read-only
//! when the file was opened (the reader's diagnostics).

use std::collections::{BTreeMap, HashSet};

use kiln_doc::{Block, Document, DrawingKind, InlineObject, Mark, NodeId, Paragraph};
use kiln_ooxml::xml::is_xml_char;

use crate::common::warnings::{DocxWarning, Severity};
use crate::read::DocxKind;

/// How serious a compatibility issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompatSeverity {
    /// Content or formatting will be lost.
    Loss,
    /// Kept, but saved in a different form (it may look or behave differently in Word).
    Change,
    /// Kept as it was read; nothing new is lost by saving.
    Info,
}

impl CompatSeverity {
    /// `loss`, `change` or `info`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Loss => "loss",
            Self::Change => "change",
            Self::Info => "info",
        }
    }
}

/// Whether an issue comes from saving or was already there when the file was opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompatOrigin {
    /// Caused by saving.
    Save,
    /// Lost or kept read-only when the file was opened.
    Open,
}

/// One line of the compatibility report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatIssue {
    /// A stable code (`image.missing`, `macros.removed`, a reader warning code, …).
    pub code: String,
    /// Loss, change or information.
    pub severity: CompatSeverity,
    /// One-line summary for the checker list.
    pub message: String,
    /// Occurrences in the document.
    pub count: usize,
    /// Save or open.
    pub origin: CompatOrigin,
}

/// What the report is computed for.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompatOptions<'a> {
    /// The format being saved (`.docx` when absent).
    pub target: Option<DocxKind>,
    /// The format the document was read from.
    pub source: Option<DocxKind>,
    /// The reader's diagnostics.
    pub warnings: &'a [DocxWarning],
}

#[derive(Default)]
struct Counter {
    map: BTreeMap<(CompatOrigin, String, String), (CompatSeverity, usize)>,
}

impl Counter {
    fn add(&mut self, origin: CompatOrigin, code: &str, severity: CompatSeverity, message: &str) {
        let e = self
            .map
            .entry((origin, code.to_owned(), message.to_owned()))
            .or_insert((severity, 0));
        e.1 += 1;
    }
}

fn paragraphs<'d>(blocks: &'d [Block], out: &mut Vec<&'d Paragraph>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                out.push(p);
                for o in p.content.objects() {
                    if let InlineObject::Drawing(d) = o
                        && let Some(tb) = &d.text_box
                    {
                        paragraphs(tb, out);
                    }
                }
            }
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        paragraphs(&c.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => paragraphs(&s.blocks, out),
            Block::Field(f) => paragraphs(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

/// Lists what saving `doc` as a Word file will lose or change, then what was lost on open.
#[must_use]
pub fn compat_report(doc: &Document, options: CompatOptions<'_>) -> Vec<CompatIssue> {
    let mut c = Counter::default();
    let save = CompatOrigin::Save;
    let notes: HashSet<NodeId> = doc.notes.iter().map(|n| n.id).collect();
    let comments: HashSet<NodeId> = doc.comments.iter().map(|n| n.id).collect();
    let mut all = Vec::new();
    for s in &doc.sections {
        paragraphs(&s.blocks, &mut all);
    }
    for h in &doc.headers_footers {
        paragraphs(&h.blocks, &mut all);
    }
    for n in &doc.notes {
        paragraphs(&n.blocks, &mut all);
    }
    for k in &doc.comments {
        paragraphs(&k.blocks, &mut all);
    }
    for p in all {
        if p.content.rope().chars().any(|ch| !is_xml_char(ch)) {
            c.add(
                save,
                "text.invalidChars",
                CompatSeverity::Loss,
                "Control characters that Word files cannot contain are removed.",
            );
        }
        for m in p.content.marks() {
            match &m.mark {
                Mark::MoveFrom(_) | Mark::MoveTo(_) => c.add(
                    save,
                    "revision.move",
                    CompatSeverity::Change,
                    "Tracked moves are saved as a deletion and an insertion.",
                ),
                Mark::Comment(id) if !comments.contains(id) => c.add(
                    save,
                    "comment.missing",
                    CompatSeverity::Loss,
                    "Comment marks whose comment no longer exists are removed.",
                ),
                _ => {}
            }
        }
        for o in p.content.objects() {
            match o {
                InlineObject::Drawing(d) => {
                    if let DrawingKind::Picture { media, .. } = &d.kind
                        && !doc.media.contains_key(media)
                    {
                        c.add(
                            save,
                            "image.missing",
                            CompatSeverity::Loss,
                            "Pictures whose image data is not available are not saved.",
                        );
                    }
                    if d.xml.is_none() && matches!(d.kind, DrawingKind::Group | DrawingKind::Other)
                    {
                        c.add(
                            save,
                            "drawing.unsaved",
                            CompatSeverity::Loss,
                            "Drawings without their original markup are not saved.",
                        );
                    }
                    if d.xml
                        .as_deref()
                        .is_some_and(|x| x.starts_with("<mc:AlternateContent"))
                    {
                        c.add(
                            save,
                            "drawing.fallback",
                            CompatSeverity::Info,
                            "Shapes changed in Kiln lose their compatibility picture for Word 2007 and older.",
                        );
                    }
                }
                InlineObject::Equation(e) if e.omml.trim().is_empty() => c.add(
                    save,
                    "equation.unsaved",
                    CompatSeverity::Loss,
                    "Equations without Office Math markup are not saved.",
                ),
                InlineObject::NoteRef(n) if !notes.contains(&n.note) => c.add(
                    save,
                    "note.missing",
                    CompatSeverity::Loss,
                    "References to notes that no longer exist are removed.",
                ),
                _ => {}
            }
        }
    }
    for k in &doc.comments {
        if k.meta.parent.is_some_and(|p| !comments.contains(&p)) {
            c.add(
                save,
                "comment.orphanReply",
                CompatSeverity::Change,
                "Replies whose first comment was deleted are saved as separate comments.",
            );
        }
    }
    let referenced: HashSet<NodeId> = doc
        .sections
        .iter()
        .flat_map(|s| s.headers.ids().chain(s.footers.ids()))
        .collect();
    for h in &doc.headers_footers {
        if !referenced.contains(&h.id) {
            c.add(
                save,
                "headerFooter.unused",
                CompatSeverity::Info,
                "Headers and footers no section uses are not saved.",
            );
        }
    }
    let target = options.target.unwrap_or(DocxKind::Document);
    let macro_target = matches!(
        target,
        DocxKind::MacroEnabledDocument | DocxKind::MacroEnabledTemplate
    );
    let macro_source = matches!(
        options.source,
        Some(DocxKind::MacroEnabledDocument | DocxKind::MacroEnabledTemplate)
    );
    if macro_source && !macro_target {
        c.add(
            save,
            "macros.removed",
            CompatSeverity::Loss,
            "Macros (VBA) are removed: this format cannot contain them.",
        );
    }
    for w in options.warnings {
        let severity = match w.severity {
            Severity::Warning => CompatSeverity::Loss,
            Severity::Info => CompatSeverity::Info,
        };
        c.add(CompatOrigin::Open, w.code.as_str(), severity, &w.message);
    }
    let mut out: Vec<CompatIssue> = c
        .map
        .into_iter()
        .map(|((origin, code, message), (severity, count))| CompatIssue {
            code,
            severity,
            message,
            count,
            origin,
        })
        .collect();
    out.sort_by(|a, b| {
        (a.severity, a.origin)
            .cmp(&(b.severity, b.origin))
            .then_with(|| a.code.cmp(&b.code))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_doc::{Drawing, Fragment, Inlines, RunProps};

    #[test]
    fn a_new_document_has_nothing_to_report() {
        assert!(compat_report(&Document::new(), CompatOptions::default()).is_empty());
    }

    #[test]
    fn missing_pictures_and_macros_are_counted() {
        let mut doc = Document::new();
        let id = doc.alloc_id();
        let pic = Drawing::picture(id, "/word/media/gone.png", 10, 10);
        let frag = Fragment::object(
            InlineObject::Drawing(Box::new(pic.clone())),
            RunProps::default(),
        );
        let mut frag2 = Fragment::object(
            InlineObject::Drawing(Box::new(Drawing {
                id: doc.alloc_id(),
                ..pic
            })),
            RunProps::default(),
        );
        frag2.text.insert(0, 'x');
        frag2.runs[0].len = 2;
        if let Block::Paragraph(p) = &mut doc.sections[0].blocks[0] {
            p.content = Inlines::from_fragment(frag).unwrap();
            let at = p.content.len();
            p.content.insert(at, frag2).unwrap();
        }
        let r = compat_report(
            &doc,
            CompatOptions {
                target: Some(DocxKind::Document),
                source: Some(DocxKind::MacroEnabledDocument),
                warnings: &[],
            },
        );
        let missing = r.iter().find(|i| i.code == "image.missing").unwrap();
        assert_eq!(missing.count, 2);
        assert_eq!(missing.severity, CompatSeverity::Loss);
        assert!(r.iter().any(|i| i.code == "macros.removed"));
        assert_eq!(r[0].severity, CompatSeverity::Loss);
    }
}
