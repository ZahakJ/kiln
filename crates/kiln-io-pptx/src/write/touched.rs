// SPDX-License-Identifier: AGPL-3.0-or-later
//! Which parts of a presentation an editing session changed, recorded from its transactions, so
//! a save can copy every slide, layout, master, notes page and comment part nobody touched byte
//! for byte from the original file (PLAN.md §3.10: untouched parts are byte-identical after
//! save).
//!
//! The record is conservative: an operation that might affect a part marks it, an undone edit
//! leaves its parts marked, and anything the recorder cannot place marks everything. Saving a
//! marked part regenerates it from the model, which is always correct; preserving is only ever an
//! optimisation of what is certainly unchanged.

use std::collections::BTreeSet;

use kiln_common::NodeId;
use kiln_deck::ops::{Op, TextTarget, Transaction};
use kiln_deck::{Owner, Presentation};

/// The parts of a PresentationML package that edits can touch.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TouchedParts {
    /// Everything is to be regenerated (an edit the recorder could not place).
    pub everything: bool,
    /// `presentation.xml` (slide size, default text style, sections, the slide and master
    /// lists).
    pub presentation: bool,
    /// Slides, layouts, masters and the notes master whose part changed, by id.
    pub owners: BTreeSet<NodeId>,
    /// Slides whose notes page changed.
    pub notes: BTreeSet<NodeId>,
    /// Slides whose comments changed.
    pub comments: BTreeSet<NodeId>,
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

    /// Whether `presentation.xml` changed.
    #[must_use]
    pub fn presentation_changed(&self) -> bool {
        self.everything || self.presentation
    }

    /// Whether the part of a slide, layout, master or the notes master changed.
    #[must_use]
    pub fn owner(&self, id: NodeId) -> bool {
        self.everything || self.owners.contains(&id)
    }

    /// Whether a slide's notes page changed.
    #[must_use]
    pub fn notes_of(&self, slide: NodeId) -> bool {
        self.everything || self.notes.contains(&slide)
    }

    /// Whether a slide's comments changed.
    #[must_use]
    pub fn comments_of(&self, slide: NodeId) -> bool {
        self.everything || self.comments.contains(&slide)
    }

    /// Records the parts a transaction touches. Call it with the presentation *after* the
    /// transaction was applied (undo and redo apply transactions too: record those as well).
    pub fn record(&mut self, pres: &Presentation, tx: &Transaction) {
        for op in &tx.ops {
            self.record_op(pres, op);
        }
    }

    /// The owner of a shape or shape list (a slide, layout, master or group id).
    fn shape_owner(&mut self, pres: &Presentation, id: NodeId) {
        if let Some(o) = pres.owner_of_id(id) {
            self.owners.insert(o.id());
            return;
        }
        match pres.locate(id) {
            Some(p) => {
                self.owners.insert(p.owner.id());
            }
            // A shape of a notes page, or one removed by the transaction itself.
            None => self.everything = true,
        }
    }

    fn text_target(&mut self, pres: &Presentation, t: TextTarget) {
        match t {
            TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => self.shape_owner(pres, s),
            TextTarget::Notes(slide) => {
                self.notes.insert(slide);
            }
        }
    }

    fn record_op(&mut self, pres: &Presentation, op: &Op) {
        match op {
            // Media are written from the parts store under the names the parts refer to.
            Op::SetPart { .. } => {}
            Op::SetSlideSize { .. }
            | Op::SetDefaultTextStyle { .. }
            | Op::SetSections { .. }
            | Op::RemoveSlide { .. }
            | Op::MoveSlide { .. }
            | Op::RemoveMaster { .. } => self.presentation = true,
            Op::InsertMaster { master, .. } => {
                self.presentation = true;
                self.owners.insert(master.id);
                self.owners.extend(master.layouts.iter().map(|l| l.id));
            }
            Op::SetTheme { master, .. }
            | Op::SetMasterTextStyle { master, .. }
            | Op::SetHeaderFooter { master, .. }
            | Op::RemoveLayout { master, .. }
            | Op::MoveLayout { master, .. } => {
                self.owners.insert(*master);
            }
            Op::InsertLayout { master, layout, .. } => {
                self.owners.insert(*master);
                self.owners.insert(layout.id);
            }
            Op::SetLayoutName { layout, .. } => {
                self.owners.insert(*layout);
            }
            Op::SetBackground { owner, .. }
            | Op::SetColorMap { owner, .. }
            | Op::SetShowMasterShapes { owner, .. } => {
                self.owners.insert(*owner);
            }
            Op::InsertSlide { slide, .. } => {
                self.presentation = true;
                self.owners.insert(slide.id);
                self.notes.insert(slide.id);
                self.comments.insert(slide.id);
            }
            Op::SetSlideLayout { slide, .. }
            | Op::SetSlideHidden { slide, .. }
            | Op::SetSlideName { slide, .. }
            | Op::SetTransition { slide, .. }
            | Op::InsertEffect { slide, .. }
            | Op::RemoveEffect { slide, .. }
            | Op::SetEffect { slide, .. }
            | Op::MoveEffect { slide, .. }
            | Op::SetTriggers { slide, .. } => {
                self.owners.insert(*slide);
            }
            Op::SetNotes { slide, .. } => {
                // The slide's relationship to its notes page may come or go, and the first notes
                // of a presentation bring a notes master into its list.
                self.owners.insert(*slide);
                self.notes.insert(*slide);
                if pres.notes_master.is_none() {
                    self.presentation = true;
                }
            }
            Op::InsertComment { slide, .. }
            | Op::RemoveComment { slide, .. }
            | Op::SetComment { slide, .. } => {
                // The slide links its comment part.
                self.owners.insert(*slide);
                self.comments.insert(*slide);
            }
            Op::InsertShapes { parent, .. }
            | Op::RemoveShapes { parent, .. }
            | Op::MoveShape { parent, .. } => {
                self.shape_owner(pres, *parent);
                // Animations and comments may refer to the shapes.
            }
            Op::SetTransform { shape, .. }
            | Op::SetShapeProps { shape, .. }
            | Op::SetPlaceholder { shape, .. }
            | Op::SetShapeInfo { shape, .. }
            | Op::SetShapeKind { shape, .. }
            | Op::SetTextBody { shape, .. } => self.shape_owner(pres, *shape),
            Op::SetChildSpace { group, .. } => self.shape_owner(pres, *group),
            Op::SetBodyProps { target, .. }
            | Op::SetListStyle { target, .. }
            | Op::SetParaProps { target, .. }
            | Op::EditText { target, .. } => self.text_target(pres, *target),
        }
    }

    /// The owner kinds this record marks, for diagnostics.
    #[must_use]
    pub fn describe(&self, pres: &Presentation) -> Vec<String> {
        if self.everything {
            return vec!["everything".to_owned()];
        }
        let mut out = Vec::new();
        if self.presentation {
            out.push("presentation".to_owned());
        }
        for id in &self.owners {
            out.push(match pres.owner_of_id(*id) {
                Some(Owner::Slide(_)) => format!("slide {id}"),
                Some(Owner::Layout(_)) => format!("layout {id}"),
                Some(Owner::Master(_)) => format!("master {id}"),
                Some(Owner::NotesMaster(_)) => format!("notes master {id}"),
                None => format!("removed {id}"),
            });
        }
        out.extend(self.notes.iter().map(|s| format!("notes of {s}")));
        out.extend(self.comments.iter().map(|s| format!("comments of {s}")));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_deck::Slide;

    #[test]
    fn records_owners_notes_and_comments() {
        let mut p = Presentation::new();
        let layout = p.masters[0].layouts[0].id;
        let slide = Slide::new(p.alloc_id(), layout);
        let sid = slide.id;
        p.slides.push(slide);
        let mut t = TouchedParts::new();
        assert!(t.is_empty());
        t.record(
            &p,
            &Transaction::new(vec![Op::SetSlideHidden {
                slide: sid,
                hidden: true,
            }]),
        );
        assert!(t.owner(sid) && !t.owner(layout));
        assert!(!t.presentation_changed());
        t.record(
            &p,
            &Transaction::new(vec![Op::MoveSlide { from: 0, to: 1 }]),
        );
        assert!(t.presentation_changed());
        t.record(
            &p,
            &Transaction::new(vec![Op::EditText {
                target: TextTarget::Notes(sid),
                tx: kiln_doc::Transaction::new(Vec::new()),
                para_props: None,
            }]),
        );
        assert!(t.notes_of(sid) && !t.comments_of(sid));
        // A shape nobody can place marks everything.
        t.record(
            &p,
            &Transaction::new(vec![Op::SetTransform {
                shape: NodeId(999_999),
                xfrm: None,
            }]),
        );
        assert!(t.everything && t.owner(layout));
        assert_eq!(t.describe(&p), ["everything"]);
        assert!(TouchedParts::all().comments_of(sid));
    }
}
