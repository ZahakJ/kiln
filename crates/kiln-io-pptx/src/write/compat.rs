// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Compatibility Checker for saving as PowerPoint (Backstage ▸ Info ▸ Check for Issues ▸
//! Check Compatibility): what [`super::write_pptx`] will lose or change, grouped by feature with
//! a count, then what was already approximated or kept read-only when the file was opened (the
//! reader's diagnostics).

use std::collections::BTreeMap;

use kiln_deck::Presentation;
use kiln_deck::animation::AnimationEffect;
use kiln_deck::shape::{MediaSource, Shape, ShapeKind};
use kiln_deck::transition::TransitionEffect;

use crate::common::extras::PptxExtras;
use crate::common::kind::PptxKind;
use crate::common::warnings::{PptxWarning, Severity};

/// How serious a compatibility issue is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompatSeverity {
    /// Content or formatting will be lost.
    Loss,
    /// Kept, but saved in a different form (it may look or behave differently in PowerPoint).
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
    /// Approximated or kept read-only when the file was opened.
    Open,
}

/// One line of the compatibility report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatIssue {
    /// A stable code (`media.missing`, `macros.removed`, a reader warning code, …).
    pub code: String,
    /// Loss, change or information.
    pub severity: CompatSeverity,
    /// One-line summary for the checker list.
    pub message: String,
    /// Occurrences in the presentation.
    pub count: usize,
    /// Save or open.
    pub origin: CompatOrigin,
}

/// What the report is computed for.
#[derive(Debug, Clone, Copy, Default)]
pub struct CompatOptions<'a> {
    /// The format being saved (`.pptx` when absent).
    pub target: Option<PptxKind>,
    /// The format the presentation was read from.
    pub source: Option<PptxKind>,
    /// What the reader kept beside the model.
    pub extras: Option<&'a PptxExtras>,
    /// The reader's diagnostics.
    pub warnings: &'a [PptxWarning],
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

fn is_macro(k: PptxKind) -> bool {
    matches!(
        k,
        PptxKind::MacroEnabledPresentation
            | PptxKind::MacroEnabledTemplate
            | PptxKind::MacroEnabledShow
    )
}

fn shapes_of<'p>(pres: &'p Presentation) -> Vec<&'p Shape> {
    let mut out = Vec::new();
    let lists = pres
        .slides
        .iter()
        .map(|s| &s.shapes)
        .chain(pres.masters.iter().map(|m| &m.shapes))
        .chain(
            pres.masters
                .iter()
                .flat_map(|m| m.layouts.iter().map(|l| &l.shapes)),
        );
    for list in lists {
        let mut stack: Vec<&'p Shape> = list.iter().rev().collect();
        while let Some(s) = stack.pop() {
            out.push(s);
            if let Some(c) = s.children() {
                stack.extend(c.iter().rev());
            }
        }
    }
    out
}

/// Lists what saving `pres` as a PowerPoint file will lose or change, then what was approximated
/// or kept read-only on open.
#[must_use]
pub fn compat_report(pres: &Presentation, options: CompatOptions<'_>) -> Vec<CompatIssue> {
    let mut c = Counter::default();
    let save = CompatOrigin::Save;
    let target = options.target.unwrap_or(PptxKind::Presentation);
    if options.source.is_some_and(is_macro) && !is_macro(target) {
        c.add(
            save,
            "macros.removed",
            CompatSeverity::Loss,
            "Macros are removed: the chosen format cannot hold them (save as .pptm to keep them).",
        );
    }
    for s in shapes_of(pres) {
        match &s.kind {
            ShapeKind::Picture(p)
                if (p.media.is_empty() || !pres.parts.contains_key(&p.media))
                    && !options.extras.is_some_and(|x| x.source_of(s.id).is_some()) =>
            {
                c.add(
                    save,
                    "image.missing",
                    CompatSeverity::Loss,
                    "A picture without its image is saved as an empty frame.",
                );
            }
            ShapeKind::Media(m) => {
                let missing = match &m.source {
                    MediaSource::Embedded(k) => !pres.parts.contains_key(k),
                    MediaSource::Linked(u) => u.is_empty(),
                };
                if missing {
                    c.add(
                        save,
                        "media.missing",
                        CompatSeverity::Loss,
                        "A clip whose file is missing is saved without it.",
                    );
                }
            }
            ShapeKind::Chart(ch) if !pres.parts.contains_key(&ch.part) => c.add(
                save,
                "chart.missing",
                CompatSeverity::Loss,
                "A chart whose part is missing is left out.",
            ),
            _ => {}
        }
        if let Some(x) = options.extras
            && x.is_smart_art(s.id)
        {
            c.add(
                save,
                "smartArt.kept",
                CompatSeverity::Info,
                "SmartArt is saved as the SmartArt the file had: its position and size follow Kiln, its content does not.",
            );
        }
    }
    let mut threaded = false;
    for s in &pres.slides {
        if let Some(t) = &s.transition {
            match t.effect {
                TransitionEffect::Morph { .. } => c.add(
                    save,
                    "transition.morph",
                    CompatSeverity::Change,
                    "Morph needs PowerPoint 2019 or later; older versions show Fade.",
                ),
                TransitionEffect::Reveal { .. } | TransitionEffect::Flash => c.add(
                    save,
                    "transition.p14",
                    CompatSeverity::Change,
                    "This transition needs PowerPoint 2010 or later; older versions show Fade.",
                ),
                _ => {}
            }
        }
        for e in s
            .timeline
            .sequence
            .iter()
            .chain(s.timeline.triggers.iter().flat_map(|t| t.effects.iter()))
        {
            if let AnimationEffect::Other { xml: None, .. } = e.effect {
                c.add(
                    save,
                    "animation.dropped",
                    CompatSeverity::Loss,
                    "An animation effect without its PowerPoint definition is left out.",
                );
            }
        }
        threaded |= s
            .comments
            .iter()
            .any(|c| c.resolved || !c.replies.is_empty());
    }
    if threaded && !options.extras.is_some_and(|x| x.modern_comments) {
        c.add(
            save,
            "comments.threaded",
            CompatSeverity::Change,
            "Comments with replies are saved as threaded comments, which PowerPoint 2019 and older show without their replies.",
        );
    }
    let open = CompatOrigin::Open;
    for w in options.warnings {
        let severity = match w.severity {
            Severity::Warning => CompatSeverity::Loss,
            Severity::Info => CompatSeverity::Info,
        };
        c.add(open, w.code.as_str(), severity, &w.message);
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
        a.origin
            .cmp(&b.origin)
            .then(a.severity.cmp(&b.severity))
            .then(a.code.cmp(&b.code))
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_deck::transition::{MorphBy, Transition};

    #[test]
    fn reports_macros_morph_and_open_diagnostics() {
        let mut p = Presentation::new();
        let layout = p.masters[0].layouts[0].id;
        let mut s = kiln_deck::Slide::new(p.alloc_id(), layout);
        s.transition = Some(Transition::new(TransitionEffect::Morph {
            by: MorphBy::Objects,
        }));
        p.slides.push(s);
        let warnings = [PptxWarning {
            code: crate::WarningCode::SmartArt,
            severity: Severity::Info,
            message: "SmartArt".into(),
            part: "/ppt/slides/slide1.xml".into(),
            location: String::new(),
        }];
        let r = compat_report(
            &p,
            CompatOptions {
                target: Some(PptxKind::Presentation),
                source: Some(PptxKind::MacroEnabledPresentation),
                extras: None,
                warnings: &warnings,
            },
        );
        assert_eq!(r[0].code, "macros.removed");
        assert!(r.iter().any(|i| i.code == "transition.morph"));
        let last = r.last().unwrap();
        assert_eq!(last.origin, CompatOrigin::Open);
        assert_eq!(last.code, "shape.smartArt");
        let clean = compat_report(&Presentation::new(), CompatOptions::default());
        assert!(clean.is_empty());
    }
}
