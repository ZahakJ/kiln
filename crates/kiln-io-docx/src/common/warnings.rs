// SPDX-License-Identifier: AGPL-3.0-or-later
//! Import diagnostics. Every construct the reader cannot model is reported with the part and an
//! XPath-like location, so the Compatibility Checker can tell the user what will look different
//! or be lost.
//!
//! Severity:
//! - [`Severity::Warning`]: content or formatting was dropped or approximated while reading;
//! - [`Severity::Info`]: the construct is preserved verbatim for saving but cannot be edited (and
//!   may be shown as a placeholder).

use std::collections::HashSet;
use std::fmt;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// Preserved for saving but not editable.
    Info,
    /// Dropped or approximated.
    Warning,
}

impl Severity {
    /// `info` or `warning`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
        }
    }
}

macro_rules! codes {
    ($($(#[$m:meta])* $variant:ident = $s:literal),+ $(,)?) => {
        /// What a diagnostic is about. The string form (`drawing.smartArt`) is stable.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum WarningCode {
            $($(#[$m])* #[doc = concat!("`", $s, "`")] $variant,)+
        }

        impl WarningCode {
            /// The stable string form.
            #[must_use]
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $s,)+
                }
            }
        }
    };
}

codes! {
    /// A referenced part does not exist.
    PartMissing = "part.missing",
    /// A part is not well-formed XML.
    PartInvalidXml = "part.invalidXml",
    /// A drawing feature is approximated or unknown.
    DrawingUnsupported = "drawing.unsupported",
    /// A group of shapes.
    DrawingGroup = "drawing.group",
    /// SmartArt.
    DrawingSmartArt = "drawing.smartArt",
    /// A drawing canvas.
    DrawingCanvas = "drawing.canvas",
    /// Ink.
    DrawingInk = "drawing.ink",
    /// Legacy VML.
    DrawingVml = "drawing.vml",
    /// A picture linked to an external file.
    ImageLinked = "image.linked",
    /// A picture whose image part is missing.
    ImageMissing = "image.missing",
    /// An embedded or linked OLE object.
    ObjectEmbedded = "object.embedded",
    /// A complex field without an end.
    FieldUnterminated = "field.unterminated",
    /// A field whose result spans paragraphs (other than TOC, INDEX, …).
    FieldMultiParagraph = "field.multiParagraph",
    /// A field whose result holds objects; the result was kept and the code removed.
    FieldUnwrapped = "field.unwrapped",
    /// A legacy form field.
    FieldFormField = "field.formField",
    /// A comment that covers no text.
    CommentUnanchored = "comment.unanchored",
    /// A reference to a comment that does not exist.
    CommentMissing = "comment.missing",
    /// A reference to a note that does not exist.
    NoteMissing = "note.missing",
    /// A tracked move (kept as move marks, linked by name only).
    RevisionMove = "revision.move",
    /// A tracked cell insertion, deletion or merge.
    RevisionCell = "revision.cell",
    /// A tracked table property change.
    RevisionTable = "revision.table",
    /// A ragged table row was padded.
    TableIrregular = "table.irregular",
    /// An element the reader does not know.
    ElementUnknown = "element.unknown",
    /// An `w:altChunk` import.
    AltChunk = "altChunk",
    /// A master-document subdocument link.
    SubDocument = "subDocument",
    /// Editing permission ranges.
    Permission = "permission",
    /// A glossary document (building blocks).
    Glossary = "glossary",
    /// Content nested deeper than the reader follows.
    NestingTooDeep = "nesting.tooDeep",
}

impl fmt::Display for WarningCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DocxWarning {
    /// What it is about.
    pub code: WarningCode,
    /// How serious it is.
    pub severity: Severity,
    /// A sentence for the Compatibility Checker.
    pub message: String,
    /// The part (`/word/document.xml`).
    pub part: String,
    /// The location in the part (`/w:document/w:body[1]/w:p[3]/w:r[2]/w:object[1]`).
    pub path: String,
}

impl fmt::Display for DocxWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] {} ({}{})",
            self.severity.as_str(),
            self.code,
            self.message,
            self.part,
            if self.path == "/" {
                String::new()
            } else {
                format!(" {}", self.path)
            }
        )
    }
}

/// Collects diagnostics, merging repeats of the same code at the same location.
#[derive(Debug, Default)]
pub struct WarningCollector {
    list: Vec<DocxWarning>,
    seen: HashSet<(WarningCode, String, String)>,
}

impl WarningCollector {
    /// Records a diagnostic unless the same code was reported at the same place.
    pub fn add(&mut self, w: DocxWarning) {
        if self.seen.insert((w.code, w.part.clone(), w.path.clone())) {
            self.list.push(w);
        }
    }

    /// The diagnostics in report order.
    #[must_use]
    pub fn list(&self) -> &[DocxWarning] {
        &self.list
    }

    /// Takes the diagnostics.
    #[must_use]
    pub fn into_vec(self) -> Vec<DocxWarning> {
        self.list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_at_one_place_are_merged() {
        let w = |path: &str| DocxWarning {
            code: WarningCode::DrawingVml,
            severity: Severity::Info,
            message: "m".into(),
            part: "/word/document.xml".into(),
            path: path.into(),
        };
        let mut c = WarningCollector::default();
        c.add(w("/a"));
        c.add(w("/a"));
        c.add(w("/b"));
        assert_eq!(c.list().len(), 2);
        assert_eq!(WarningCode::DrawingSmartArt.as_str(), "drawing.smartArt");
        assert_eq!(
            w("/a").to_string(),
            "info [drawing.vml] m (/word/document.xml /a)"
        );
        assert_eq!(c.into_vec().len(), 2);
    }
}
