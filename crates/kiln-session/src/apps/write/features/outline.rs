// SPDX-License-Identifier: AGPL-3.0-or-later
//! The outline check of the Navigation pane: headings that skip levels (a Heading 3 straight
//! after a Heading 1) and heading paragraphs without text. Screen readers and the PDF tags
//! navigate by these levels, so both are accessibility problems as well as structural ones.

use kiln_doc::OBJECT_CHAR;
use kiln_protocol::commands::write_features::{OutlineIssue, OutlineIssueKind};

use super::super::WriteDoc;

impl WriteDoc {
    /// Outline problems of the body, in document order (an empty heading at the caret, about
    /// to be typed, is left out).
    pub(super) fn outline_issues(&self) -> Vec<OutlineIssue> {
        let mut out = Vec::new();
        // The level of the last heading with text (0: none yet, so a first Heading 2 is fine
        // only after a Heading 1, as in Word's accessibility checker).
        let mut last = 0u8;
        for p in self.doc.body_paragraphs() {
            let r = self.doc.resolve_paragraph(p);
            if r.outline_level >= 9 {
                continue;
            }
            let level = r.outline_level + 1;
            let text = p
                .content
                .plain_text(true)
                .replace(OBJECT_CHAR, "")
                .trim()
                .to_owned();
            if text.is_empty() {
                // The heading being typed at the caret is not a problem yet.
                if p.id == self.head.para {
                    continue;
                }
                out.push(OutlineIssue {
                    para: p.id,
                    level,
                    text,
                    kind: OutlineIssueKind::Empty,
                });
                continue;
            }
            if level > last + 1 {
                out.push(OutlineIssue {
                    para: p.id,
                    level,
                    text,
                    kind: OutlineIssueKind::SkippedLevel { expected: last + 1 },
                });
            }
            last = level;
        }
        out
    }
}
