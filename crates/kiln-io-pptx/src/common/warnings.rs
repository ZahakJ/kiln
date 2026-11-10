// SPDX-License-Identifier: AGPL-3.0-or-later
//! Import diagnostics. Everything the reader approximates, keeps without modelling or drops is
//! reported with the part and a location, so the Compatibility Checker can say what will look
//! different, what cannot be edited and what is lost.
//!
//! Severity:
//! - [`Severity::Warning`]: content or formatting was dropped or approximated;
//! - [`Severity::Info`]: the construct is kept verbatim for saving but is not editable as itself
//!   (SmartArt shown as shapes, an OLE object shown as its picture, a transition shown as its
//!   nearest relative).

use std::collections::HashSet;
use std::fmt;

/// How serious a diagnostic is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Severity {
    /// Preserved for saving but not editable as itself.
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
        /// What a diagnostic is about. The string form (`shape.smartArt`) is stable.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum WarningCode {
            $($(#[$m])* #[doc = concat!("`", $s, "`")] $variant,)+
        }

        impl WarningCode {
            /// Every code.
            pub const ALL: &'static [WarningCode] = &[$(Self::$variant,)+];

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
    /// A slide, layout or master that cannot be used (wrong root element, no layout).
    PartUnusable = "part.unusable",
    /// SmartArt, shown as the shapes of its drawing (or as a placeholder) and kept for saving.
    SmartArt = "shape.smartArt",
    /// An embedded or linked OLE object, shown as its picture and kept for saving.
    OleObject = "shape.ole",
    /// Ink or another content part, kept for saving and not shown.
    ContentPart = "shape.contentPart",
    /// A shape kind or graphic the reader does not know, kept for saving and not shown.
    ShapeUnknown = "shape.unknown",
    /// A position, size or rotation outside the DrawingML range, clamped.
    ShapeClamped = "shape.clamped",
    /// Groups nested deeper than the reader follows; the inner content is kept for saving.
    NestingTooDeep = "shape.nestingTooDeep",
    /// A placeholder type outside the schema, read as a content placeholder.
    PlaceholderUnknown = "placeholder.unknown",
    /// A picture whose image part is missing.
    ImageMissing = "image.missing",
    /// A picture linked to an external file (not embedded).
    ImageLinked = "image.linked",
    /// Video or audio linked to an external file.
    MediaLinked = "media.linked",
    /// A table that is empty or has rows of the wrong length.
    TableIrregular = "table.irregular",
    /// A chart part kiln-charts cannot read; kept for saving.
    ChartUnreadable = "chart.unreadable",
    /// A transition shown as its nearest relative among Kiln's twelve.
    TransitionMapped = "transition.mapped",
    /// An animation effect Kiln does not play yet; kept for saving.
    AnimationUnsupported = "animation.unsupported",
    /// Animation timing the reader could not follow; the original is kept for saving.
    TimingUnreadable = "timing.unreadable",
    /// Sections that do not cover the slides in order; dropped.
    SectionsInconsistent = "sections.inconsistent",
    /// A comment by an author the file does not list.
    CommentAuthorMissing = "comment.authorMissing",
    /// A slide the slide list names twice or whose part is missing.
    SlideSkipped = "slide.skipped",
    /// A hyperlink whose target is missing.
    LinkBroken = "link.broken",
}

impl fmt::Display for WarningCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One diagnostic.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PptxWarning {
    /// What it is about.
    pub code: WarningCode,
    /// How serious it is.
    pub severity: Severity,
    /// A sentence for the Compatibility Checker.
    pub message: String,
    /// The part (`/ppt/slides/slide3.xml`).
    pub part: String,
    /// Where in the part (`shape 4 "Diagram 3"`), or empty for the whole part.
    pub location: String,
}

impl fmt::Display for PptxWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} [{}] {} ({}{})",
            self.severity.as_str(),
            self.code,
            self.message,
            self.part,
            if self.location.is_empty() {
                String::new()
            } else {
                format!(" {}", self.location)
            }
        )
    }
}

/// Collects diagnostics, merging repeats of the same code at the same place.
#[derive(Debug, Default)]
pub struct WarningCollector {
    list: Vec<PptxWarning>,
    seen: HashSet<(WarningCode, String, String)>,
}

impl WarningCollector {
    /// Records a diagnostic unless the same code was reported at the same place.
    pub fn add(&mut self, w: PptxWarning) {
        if self
            .seen
            .insert((w.code, w.part.clone(), w.location.clone()))
        {
            self.list.push(w);
        }
    }

    /// Records a diagnostic from its parts.
    pub fn push(
        &mut self,
        code: WarningCode,
        severity: Severity,
        message: impl Into<String>,
        part: &str,
        location: impl Into<String>,
    ) {
        self.add(PptxWarning {
            code,
            severity,
            message: message.into(),
            part: part.to_owned(),
            location: location.into(),
        });
    }

    /// The diagnostics in report order.
    #[must_use]
    pub fn list(&self) -> &[PptxWarning] {
        &self.list
    }

    /// Takes the diagnostics.
    #[must_use]
    pub fn into_vec(self) -> Vec<PptxWarning> {
        self.list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeats_at_one_place_are_merged() {
        let mut c = WarningCollector::default();
        for loc in ["shape 2", "shape 2", "shape 3"] {
            c.push(
                WarningCode::SmartArt,
                Severity::Info,
                "m",
                "/ppt/slides/slide1.xml",
                loc,
            );
        }
        assert_eq!(c.list().len(), 2);
        assert_eq!(
            c.list()[0].to_string(),
            "info [shape.smartArt] m (/ppt/slides/slide1.xml shape 2)"
        );
        c.push(
            WarningCode::PartMissing,
            Severity::Warning,
            "gone",
            "/ppt/x.xml",
            "",
        );
        assert_eq!(
            c.list()[2].to_string(),
            "warning [part.missing] gone (/ppt/x.xml)"
        );
        let mut names: Vec<_> = WarningCode::ALL.iter().map(|c| c.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), WarningCode::ALL.len());
        assert_eq!(c.into_vec().len(), 3);
    }
}
