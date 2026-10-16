// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operations and transactions: the only way a presentation changes once it is built.
//!
//! An [`Op`] is one primitive change addressed by [`NodeId`]s and indices. Applying an op checks
//! it completely first (unknown ids, out-of-range indices, reused ids, broken references such as
//! a slide on a missing layout or an effect on a missing shape) and returns its **inverse**: the
//! op that restores the presentation exactly. A [`Transaction`] applies its ops in order and is
//! atomic: if one fails, or the result breaks the section partition, everything is rolled back.
//! Text edits wrap a kiln-doc transaction ([`Op::EditText`]), so typing in a shape is the same
//! operation as typing in Write.

use std::collections::{BTreeMap, HashSet};

use kiln_common::NodeId;
use kiln_ooxml::drawingml::theme::Theme;
use serde::{Deserialize, Serialize};

use crate::animation::{Effect, TriggerSequence};
use crate::dml::serde_xml::xml;
use crate::error::{DeckError, Result};
use crate::geom::{ChildSpace, Transform};
use crate::presentation::{Owner, Part, Presentation, shape_ids};
use crate::shape::{Placeholder, Shape, ShapeKind, ShapeProps};
use crate::slide::{
    Background, ColorMapping, Comment, HeaderFooter, Layout, Master, Notes, Section, Slide,
    SlideSize,
};
use crate::text::{BodyProps, DeckParaProps, ListStyle, TextBody};
use crate::transition::Transition;

/// A text body: in a shape, a table cell or a slide's notes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextTarget {
    /// A geometry shape's text.
    Shape(NodeId),
    /// A table cell.
    Cell {
        /// The table shape.
        shape: NodeId,
        /// Row.
        row: usize,
        /// Grid column.
        col: usize,
    },
    /// A slide's speaker notes.
    Notes(NodeId),
}

/// One of a master's three text styles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MasterStyle {
    /// `p:titleStyle`.
    Title,
    /// `p:bodyStyle`.
    Body,
    /// `p:otherStyle`.
    Other,
}

/// A shape's descriptive properties (the Selection Pane and Alt Text).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct ShapeInfo {
    /// Name.
    pub name: String,
    /// Alternative text.
    pub alt_text: Option<String>,
    /// Title.
    pub title: Option<String>,
    /// Hidden.
    pub hidden: bool,
    /// Hyperlink.
    pub link: Option<String>,
}

impl ShapeInfo {
    /// A shape's info.
    #[must_use]
    pub fn of(s: &Shape) -> Self {
        Self {
            name: s.name.clone(),
            alt_text: s.alt_text.clone(),
            title: s.title.clone(),
            hidden: s.hidden,
            link: s.link.clone(),
        }
    }

    fn apply_to(self, s: &mut Shape) -> Self {
        let old = Self::of(s);
        s.name = self.name;
        s.alt_text = self.alt_text;
        s.title = self.title;
        s.hidden = self.hidden;
        s.link = self.link;
        old
    }
}

/// One primitive change.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Op {
    // ---- presentation
    /// Replaces the slide size (shapes are not scaled; see `edit::set_slide_size`).
    SetSlideSize {
        /// The size.
        size: SlideSize,
    },
    /// Replaces the default text style.
    SetDefaultTextStyle {
        /// The style.
        style: Box<ListStyle>,
    },
    /// Replaces the sections (they must partition the slides when the transaction ends).
    SetSections {
        /// The sections.
        sections: Vec<Section>,
    },
    /// Adds, replaces or removes a part.
    SetPart {
        /// The key.
        key: String,
        /// The part, or `None` to remove it.
        part: Option<Part>,
    },
    // ---- masters and layouts
    /// Inserts a master (with its layouts). Inverse: [`Op::RemoveMaster`].
    InsertMaster {
        /// Index.
        index: usize,
        /// The master (fresh ids).
        master: Box<Master>,
    },
    /// Removes a master no slide uses; one must remain. Inverse: [`Op::InsertMaster`].
    RemoveMaster {
        /// Index.
        index: usize,
    },
    /// Replaces a master's theme.
    SetTheme {
        /// The master.
        master: NodeId,
        /// The theme.
        #[serde(with = "xml")]
        theme: Box<Theme>,
    },
    /// Replaces one of a master's text styles.
    SetMasterTextStyle {
        /// The master.
        master: NodeId,
        /// Which style.
        which: MasterStyle,
        /// The style.
        style: Box<ListStyle>,
    },
    /// Replaces a master's header/footer flags.
    SetHeaderFooter {
        /// The master.
        master: NodeId,
        /// The flags.
        header_footer: HeaderFooter,
    },
    /// Inserts a layout into a master. Inverse: [`Op::RemoveLayout`].
    InsertLayout {
        /// The master.
        master: NodeId,
        /// Index in its layouts.
        index: usize,
        /// The layout (fresh ids).
        layout: Box<Layout>,
    },
    /// Removes a layout no slide uses; the master keeps at least one. Inverse:
    /// [`Op::InsertLayout`].
    RemoveLayout {
        /// The master.
        master: NodeId,
        /// Index.
        index: usize,
    },
    /// Moves a layout within its master.
    MoveLayout {
        /// The master.
        master: NodeId,
        /// From index.
        from: usize,
        /// To index (after removal).
        to: usize,
    },
    /// Renames a layout.
    SetLayoutName {
        /// The layout.
        layout: NodeId,
        /// The name.
        name: String,
    },
    /// Replaces the background of a slide, layout or master (masters always have one).
    SetBackground {
        /// The slide, layout or master.
        owner: NodeId,
        /// The background (`None` inherits).
        background: Option<Background>,
    },
    /// Replaces the colour map (override) of a slide, layout or master.
    SetColorMap {
        /// The slide, layout or master.
        owner: NodeId,
        /// The map (`None` inherits; masters always have one).
        map: Option<ColorMapping>,
    },
    /// Shows or hides master graphics behind a slide or layout.
    SetShowMasterShapes {
        /// The slide or layout.
        owner: NodeId,
        /// Show them.
        show: bool,
    },
    // ---- slides
    /// Inserts a slide. Inverse: [`Op::RemoveSlide`].
    InsertSlide {
        /// Index.
        index: usize,
        /// The slide (fresh ids, on an existing layout).
        slide: Box<Slide>,
    },
    /// Removes a slide. Inverse: [`Op::InsertSlide`].
    RemoveSlide {
        /// Index.
        index: usize,
    },
    /// Moves a slide.
    MoveSlide {
        /// From index.
        from: usize,
        /// Final index.
        to: usize,
    },
    /// Changes a slide's layout reference.
    SetSlideLayout {
        /// The slide.
        slide: NodeId,
        /// The layout.
        layout: NodeId,
    },
    /// Hides or shows a slide in the slide show.
    SetSlideHidden {
        /// The slide.
        slide: NodeId,
        /// Hidden.
        hidden: bool,
    },
    /// Names a slide.
    SetSlideName {
        /// The slide.
        slide: NodeId,
        /// The name.
        name: Option<String>,
    },
    /// Replaces a slide's transition.
    SetTransition {
        /// The slide.
        slide: NodeId,
        /// The transition.
        transition: Option<Transition>,
    },
    /// Replaces a slide's notes page.
    SetNotes {
        /// The slide.
        slide: NodeId,
        /// The notes.
        notes: Option<Box<Notes>>,
    },
    // ---- animation
    /// Inserts an effect into the main sequence. Inverse: [`Op::RemoveEffect`].
    InsertEffect {
        /// The slide.
        slide: NodeId,
        /// Index.
        index: usize,
        /// The effect (fresh id, target on the slide).
        effect: Box<Effect>,
    },
    /// Removes an effect. Inverse: [`Op::InsertEffect`].
    RemoveEffect {
        /// The slide.
        slide: NodeId,
        /// Index.
        index: usize,
    },
    /// Replaces an effect (same id).
    SetEffect {
        /// The slide.
        slide: NodeId,
        /// Index.
        index: usize,
        /// The effect.
        effect: Box<Effect>,
    },
    /// Reorders an effect.
    MoveEffect {
        /// The slide.
        slide: NodeId,
        /// From.
        from: usize,
        /// To (after removal).
        to: usize,
    },
    /// Replaces the trigger sequences (P2).
    SetTriggers {
        /// The slide.
        slide: NodeId,
        /// The sequences.
        triggers: Vec<TriggerSequence>,
    },
    // ---- comments
    /// Inserts a comment thread. Inverse: [`Op::RemoveComment`].
    InsertComment {
        /// The slide.
        slide: NodeId,
        /// Index.
        index: usize,
        /// The comment (fresh ids).
        comment: Box<Comment>,
    },
    /// Removes a comment thread.
    RemoveComment {
        /// The slide.
        slide: NodeId,
        /// Index.
        index: usize,
    },
    /// Replaces a comment thread (same id; replies may change).
    SetComment {
        /// The slide.
        slide: NodeId,
        /// Index.
        index: usize,
        /// The comment.
        comment: Box<Comment>,
    },
    // ---- shapes
    /// Inserts shapes into a slide, layout, master or group. Inverse: [`Op::RemoveShapes`].
    InsertShapes {
        /// The slide, layout, master or group.
        parent: NodeId,
        /// Index (z-order: 0 is the back).
        index: usize,
        /// The shapes (fresh ids).
        shapes: Vec<Shape>,
    },
    /// Removes shapes no animation refers to. Inverse: [`Op::InsertShapes`].
    RemoveShapes {
        /// The parent.
        parent: NodeId,
        /// First index.
        index: usize,
        /// How many.
        count: usize,
    },
    /// Moves a shape within its list (z-order).
    MoveShape {
        /// The parent.
        parent: NodeId,
        /// From.
        from: usize,
        /// To (after removal).
        to: usize,
    },
    /// Replaces a shape's transform (`None` inherits).
    SetTransform {
        /// The shape.
        shape: NodeId,
        /// The transform.
        xfrm: Option<Transform>,
    },
    /// Replaces a shape's properties.
    SetShapeProps {
        /// The shape.
        shape: NodeId,
        /// The properties.
        props: Box<ShapeProps>,
    },
    /// Replaces a shape's placeholder reference.
    SetPlaceholder {
        /// The shape.
        shape: NodeId,
        /// The placeholder.
        placeholder: Option<Placeholder>,
    },
    /// Replaces a shape's name, alt text, title, hidden flag and link.
    SetShapeInfo {
        /// The shape.
        shape: NodeId,
        /// The info.
        info: Box<ShapeInfo>,
    },
    /// Replaces what a shape is (crop, table structure, connector ends, media settings, …).
    SetShapeKind {
        /// The shape.
        shape: NodeId,
        /// The kind (descendant ids fresh or kept).
        kind: Box<ShapeKind>,
    },
    /// Replaces a group's child coordinate space.
    SetChildSpace {
        /// The group.
        group: NodeId,
        /// The space.
        space: ChildSpace,
    },
    // ---- text
    /// Adds or removes a geometry shape's text body.
    SetTextBody {
        /// The shape.
        shape: NodeId,
        /// The body.
        text: Option<Box<TextBody>>,
    },
    /// Replaces body properties.
    SetBodyProps {
        /// The text.
        target: TextTarget,
        /// The properties.
        body: Box<BodyProps>,
    },
    /// Replaces the list style of a text body.
    SetListStyle {
        /// The text.
        target: TextTarget,
        /// The style.
        style: Box<ListStyle>,
    },
    /// Replaces a paragraph's DrawingML extras.
    SetParaProps {
        /// The text.
        target: TextTarget,
        /// The paragraph.
        para: NodeId,
        /// The extras (`None` clears them).
        props: Option<Box<DeckParaProps>>,
    },
    /// Applies a kiln-doc transaction to a text body. `para_props`, when given, replaces the
    /// paragraph extras afterwards (inverses carry it to restore them exactly).
    EditText {
        /// The text.
        target: TextTarget,
        /// The kiln-doc transaction.
        tx: kiln_doc::Transaction,
        /// Paragraph extras after the edit.
        para_props: Option<BTreeMap<NodeId, DeckParaProps>>,
    },
}

/// What kind of edit a transaction is (drives undo grouping and labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum EditKind {
    /// Typed text (coalesced per word in undo).
    Typing,
    /// Deleted text.
    Delete,
    /// Formatting.
    Format,
    /// Slides, layouts, sections, themes.
    Structure,
    /// Moving, sizing, rotating, ordering, grouping shapes.
    Arrange,
    /// Anything else.
    #[default]
    Other,
}

/// An atomic list of ops with metadata.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Transaction {
    /// The ops, applied in order.
    pub ops: Vec<Op>,
    /// What kind of edit this is.
    pub kind: EditKind,
    /// The selection (slide or shape ids) before the edit, restored by undo.
    pub selection_before: Vec<NodeId>,
    /// The selection after the edit, restored by redo.
    pub selection_after: Vec<NodeId>,
}

impl Transaction {
    /// A transaction of kind [`EditKind::Other`].
    #[must_use]
    pub fn new(ops: Vec<Op>) -> Self {
        Self {
            ops,
            ..Self::default()
        }
    }

    /// The same transaction with a kind.
    #[must_use]
    pub fn with_kind(mut self, kind: EditKind) -> Self {
        self.kind = kind;
        self
    }

    /// The same transaction with selections.
    #[must_use]
    pub fn with_selection(mut self, before: Vec<NodeId>, after: Vec<NodeId>) -> Self {
        self.selection_before = before;
        self.selection_after = after;
        self
    }

    /// True when there are no ops.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

fn range(index: usize, len: usize) -> DeckError {
    DeckError::OutOfRange { index, len }
}

fn invalid(m: impl Into<String>) -> DeckError {
    DeckError::InvalidOp(m.into())
}

fn ids_of_master(m: &Master) -> Vec<NodeId> {
    let mut out = vec![m.id];
    shape_ids(&m.shapes, &mut out);
    for l in &m.layouts {
        out.extend(ids_of_layout(l));
    }
    out
}

fn ids_of_layout(l: &Layout) -> Vec<NodeId> {
    let mut out = vec![l.id];
    shape_ids(&l.shapes, &mut out);
    out
}

fn ids_of_slide(s: &Slide) -> Vec<NodeId> {
    let mut out = vec![s.id];
    shape_ids(&s.shapes, &mut out);
    out.extend(s.timeline.effect_ids());
    for c in &s.comments {
        out.push(c.id);
        out.extend(c.replies.iter().map(|r| r.id));
    }
    if let Some(n) = &s.notes {
        shape_ids(&n.shapes, &mut out);
    }
    out
}

fn ids_of_comment(c: &Comment) -> Vec<NodeId> {
    std::iter::once(c.id)
        .chain(c.replies.iter().map(|r| r.id))
        .collect()
}

fn ids_of_kind(k: &ShapeKind) -> Vec<NodeId> {
    let mut out = Vec::new();
    if let ShapeKind::Group(g) = k {
        shape_ids(&g.children, &mut out);
    }
    out
}

fn shapes_on(shapes: &[Shape]) -> HashSet<NodeId> {
    let mut ids = Vec::new();
    shape_ids(shapes, &mut ids);
    ids.into_iter().collect()
}

impl Presentation {
    /// Applies a transaction and returns its inverse. Atomic: on error nothing changes.
    ///
    /// # Errors
    /// The first failing op's error, or [`DeckError::InvalidOp`] when the result breaks the
    /// section partition.
    pub fn apply(&mut self, tx: Transaction) -> Result<Transaction> {
        let mut inverses: Vec<Op> = Vec::with_capacity(tx.ops.len());
        let rollback = |p: &mut Presentation, inverses: Vec<Op>| {
            for inv in inverses.into_iter().rev() {
                // Inverses of ops that just succeeded cannot fail; if one ever did, the rest of
                // the rollback still runs and the original error is reported.
                let _ = p.apply_op(inv);
            }
        };
        for op in tx.ops {
            match self.apply_op(op) {
                Ok(inv) => inverses.push(inv),
                Err(e) => {
                    rollback(self, inverses);
                    return Err(e);
                }
            }
        }
        if !self.sections.is_empty() {
            let total: usize = self.sections.iter().map(|s| s.len).sum();
            if total != self.slides.len() {
                let e = invalid(format!(
                    "sections hold {total} slides but there are {}",
                    self.slides.len()
                ));
                rollback(self, inverses);
                return Err(e);
            }
        }
        inverses.reverse();
        Ok(Transaction {
            ops: inverses,
            kind: tx.kind,
            selection_before: tx.selection_after,
            selection_after: tx.selection_before,
        })
    }

    /// Fails when any of `ids` is 0, repeated, or used in the presentation outside `replaced`.
    fn check_fresh(&self, ids: &[NodeId], replaced: &[NodeId]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut seen = HashSet::with_capacity(ids.len());
        for id in ids {
            if id.0 == 0 || id.0 > NodeId::MAX.0 || !seen.insert(*id) {
                return Err(DeckError::DuplicateId(*id));
            }
        }
        let replaced: HashSet<NodeId> = replaced.iter().copied().collect();
        if let Some(dup) = self
            .all_ids()
            .into_iter()
            .find(|i| seen.contains(i) && !replaced.contains(i))
        {
            return Err(DeckError::DuplicateId(dup));
        }
        Ok(())
    }

    /// The slide whose shape tree contains a list (`parent` is the slide or a group on it).
    fn slide_of_parent(&self, parent: NodeId) -> Option<NodeId> {
        if self.slide(parent).is_some() {
            return Some(parent);
        }
        self.slide_of_shape(parent).map(|s| s.id)
    }

    /// Fails when an animation on `slide` targets one of `ids`.
    fn check_not_animated(&self, slide: Option<NodeId>, ids: &[NodeId]) -> Result<()> {
        let Some(slide) = slide.and_then(|s| self.slide(s)) else {
            return Ok(());
        };
        let ids: HashSet<NodeId> = ids.iter().copied().collect();
        if let Some(t) = slide.timeline.shape_refs().find(|t| ids.contains(t)) {
            return Err(invalid(format!(
                "an animation on slide {} refers to shape {t}; remove it first",
                slide.id
            )));
        }
        Ok(())
    }

    fn check_effect_target(slide: &Slide, e: &Effect) -> Result<()> {
        if shapes_on(&slide.shapes).contains(&e.target.shape) {
            Ok(())
        } else {
            Err(invalid(format!(
                "effect {} targets {}, which is not on slide {}",
                e.id, e.target.shape, slide.id
            )))
        }
    }

    fn text_body_mut(&mut self, target: TextTarget) -> Result<&mut TextBody> {
        match target {
            TextTarget::Shape(id) => self
                .shape_mut(id)
                .ok_or(DeckError::UnknownNode(id))?
                .text_mut()
                .ok_or(DeckError::WrongKind {
                    id,
                    expected: "shape with text",
                }),
            TextTarget::Cell { shape, row, col } => {
                let s = self.shape_mut(shape).ok_or(DeckError::UnknownNode(shape))?;
                let ShapeKind::Table(t) = &mut s.kind else {
                    return Err(DeckError::WrongKind {
                        id: shape,
                        expected: "table",
                    });
                };
                let rows = t.rows.len();
                let r = t.rows.get_mut(row).ok_or(range(row, rows))?;
                let cols = r.cells.len();
                Ok(&mut r.cells.get_mut(col).ok_or(range(col, cols))?.text)
            }
            TextTarget::Notes(slide) => self
                .slide_mut(slide)
                .ok_or(DeckError::UnknownNode(slide))?
                .notes
                .as_mut()
                .map(|n| &mut n.text)
                .ok_or(DeckError::WrongKind {
                    id: slide,
                    expected: "slide with notes",
                }),
        }
    }

    fn slide_or_err(&mut self, id: NodeId) -> Result<&mut Slide> {
        self.slide_mut(id).ok_or(DeckError::UnknownNode(id))
    }

    fn master_or_err(&mut self, id: NodeId) -> Result<&mut Master> {
        self.master_mut(id).ok_or(DeckError::UnknownNode(id))
    }

    fn layout_used(&self, layout: NodeId) -> bool {
        self.slides.iter().any(|s| s.layout == layout)
    }

    /// Applies one op and returns its inverse. Nothing changes on error.
    ///
    /// # Errors
    /// Unknown ids, out-of-range indices, reused ids, broken references.
    #[allow(clippy::too_many_lines)]
    pub fn apply_op(&mut self, op: Op) -> Result<Op> {
        Ok(match op {
            // ---------------------------------------------------- presentation
            Op::SetSlideSize { size } => {
                if !size.is_valid() {
                    return Err(invalid("the slide size is outside 1 EMU to 56 inches"));
                }
                Op::SetSlideSize {
                    size: std::mem::replace(&mut self.slide_size, size),
                }
            }
            Op::SetDefaultTextStyle { style } => Op::SetDefaultTextStyle {
                style: Box::new(std::mem::replace(&mut self.default_text_style, *style)),
            },
            Op::SetSections { sections } => {
                let ids: Vec<NodeId> = sections.iter().map(|s| s.id).collect();
                let old: Vec<NodeId> = self.sections.iter().map(|s| s.id).collect();
                self.check_fresh(&ids, &old)?;
                self.bump_ids(&ids);
                Op::SetSections {
                    sections: std::mem::replace(&mut self.sections, sections),
                }
            }
            Op::SetPart { key, part } => {
                let old = match part {
                    Some(p) => self.parts.insert(key.clone(), p),
                    None => self.parts.remove(&key),
                };
                Op::SetPart { key, part: old }
            }
            // ---------------------------------------------------- masters, layouts
            Op::InsertMaster { index, master } => {
                if index > self.masters.len() {
                    return Err(range(index, self.masters.len()));
                }
                if master.layouts.is_empty() {
                    return Err(invalid("a master needs a layout"));
                }
                let ids = ids_of_master(&master);
                self.check_fresh(&ids, &[])?;
                self.bump_ids(&ids);
                self.masters.insert(index, *master);
                Op::RemoveMaster { index }
            }
            Op::RemoveMaster { index } => {
                let len = self.masters.len();
                let m = self.masters.get(index).ok_or(range(index, len))?;
                if len == 1 {
                    return Err(invalid("a presentation keeps at least one master"));
                }
                if let Some(l) = m.layouts.iter().find(|l| self.layout_used(l.id)) {
                    return Err(invalid(format!("layout {} is in use", l.id)));
                }
                Op::InsertMaster {
                    index,
                    master: Box::new(self.masters.remove(index)),
                }
            }
            Op::SetTheme { master, theme } => {
                let m = self.master_or_err(master)?;
                Op::SetTheme {
                    master,
                    theme: Box::new(std::mem::replace(&mut m.theme, *theme)),
                }
            }
            Op::SetMasterTextStyle {
                master,
                which,
                style,
            } => {
                let m = self.master_or_err(master)?;
                let slot = match which {
                    MasterStyle::Title => &mut m.title_style,
                    MasterStyle::Body => &mut m.body_style,
                    MasterStyle::Other => &mut m.other_style,
                };
                Op::SetMasterTextStyle {
                    master,
                    which,
                    style: Box::new(std::mem::replace(slot, *style)),
                }
            }
            Op::SetHeaderFooter {
                master,
                header_footer,
            } => {
                let m = self.master_or_err(master)?;
                Op::SetHeaderFooter {
                    master,
                    header_footer: std::mem::replace(&mut m.header_footer, header_footer),
                }
            }
            Op::InsertLayout {
                master,
                index,
                layout,
            } => {
                let len = self
                    .master(master)
                    .ok_or(DeckError::UnknownNode(master))?
                    .layouts
                    .len();
                if index > len {
                    return Err(range(index, len));
                }
                let ids = ids_of_layout(&layout);
                self.check_fresh(&ids, &[])?;
                self.bump_ids(&ids);
                self.master_or_err(master)?.layouts.insert(index, *layout);
                Op::RemoveLayout { master, index }
            }
            Op::RemoveLayout { master, index } => {
                let m = self.master(master).ok_or(DeckError::UnknownNode(master))?;
                let len = m.layouts.len();
                let l = m.layouts.get(index).ok_or(range(index, len))?;
                if len == 1 {
                    return Err(invalid("a master keeps at least one layout"));
                }
                if self.layout_used(l.id) {
                    return Err(invalid(format!("layout {} is in use", l.id)));
                }
                let layout = self.master_or_err(master)?.layouts.remove(index);
                Op::InsertLayout {
                    master,
                    index,
                    layout: Box::new(layout),
                }
            }
            Op::MoveLayout { master, from, to } => {
                let m = self.master_or_err(master)?;
                let len = m.layouts.len();
                if from >= len {
                    return Err(range(from, len));
                }
                if to >= len {
                    return Err(range(to, len));
                }
                let l = m.layouts.remove(from);
                m.layouts.insert(to, l);
                Op::MoveLayout {
                    master,
                    from: to,
                    to: from,
                }
            }
            Op::SetLayoutName { layout, name } => {
                let l = self
                    .layout_mut(layout)
                    .ok_or(DeckError::UnknownNode(layout))?;
                Op::SetLayoutName {
                    layout,
                    name: std::mem::replace(&mut l.name, name),
                }
            }
            Op::SetBackground { owner, background } => {
                let old = match self.owner_of_id(owner) {
                    Some(Owner::Slide(_)) => {
                        std::mem::replace(&mut self.slide_or_err(owner)?.background, background)
                    }
                    Some(Owner::Layout(_)) => std::mem::replace(
                        &mut self
                            .layout_mut(owner)
                            .ok_or(DeckError::UnknownNode(owner))?
                            .background,
                        background,
                    ),
                    Some(Owner::Master(_)) => {
                        let bg = background
                            .ok_or_else(|| invalid("a master always has a background"))?;
                        Some(std::mem::replace(
                            &mut self.master_or_err(owner)?.background,
                            bg,
                        ))
                    }
                    Some(Owner::NotesMaster(_)) => {
                        let bg = background
                            .ok_or_else(|| invalid("a master always has a background"))?;
                        let n = self
                            .notes_master
                            .as_mut()
                            .ok_or(DeckError::UnknownNode(owner))?;
                        Some(std::mem::replace(&mut n.background, bg))
                    }
                    None => return Err(DeckError::UnknownNode(owner)),
                };
                Op::SetBackground {
                    owner,
                    background: old,
                }
            }
            Op::SetColorMap { owner, map } => {
                let old = match self.owner_of_id(owner) {
                    Some(Owner::Slide(_)) => {
                        std::mem::replace(&mut self.slide_or_err(owner)?.color_map, map)
                    }
                    Some(Owner::Layout(_)) => std::mem::replace(
                        &mut self
                            .layout_mut(owner)
                            .ok_or(DeckError::UnknownNode(owner))?
                            .color_map,
                        map,
                    ),
                    Some(Owner::Master(_)) => {
                        let map = map.ok_or_else(|| invalid("a master always has a colour map"))?;
                        Some(std::mem::replace(
                            &mut self.master_or_err(owner)?.color_map,
                            map,
                        ))
                    }
                    Some(Owner::NotesMaster(_)) | None => {
                        return Err(DeckError::WrongKind {
                            id: owner,
                            expected: "slide, layout or master",
                        });
                    }
                };
                Op::SetColorMap { owner, map: old }
            }
            Op::SetShowMasterShapes { owner, show } => {
                let old = match self.owner_of_id(owner) {
                    Some(Owner::Slide(_)) => {
                        std::mem::replace(&mut self.slide_or_err(owner)?.show_master_shapes, show)
                    }
                    Some(Owner::Layout(_)) => std::mem::replace(
                        &mut self
                            .layout_mut(owner)
                            .ok_or(DeckError::UnknownNode(owner))?
                            .show_master_shapes,
                        show,
                    ),
                    _ => {
                        return Err(DeckError::WrongKind {
                            id: owner,
                            expected: "slide or layout",
                        });
                    }
                };
                Op::SetShowMasterShapes { owner, show: old }
            }
            // ---------------------------------------------------- slides
            Op::InsertSlide { index, slide } => {
                if index > self.slides.len() {
                    return Err(range(index, self.slides.len()));
                }
                if self.layout(slide.layout).is_none() {
                    return Err(DeckError::UnknownNode(slide.layout));
                }
                let on = shapes_on(&slide.shapes);
                if let Some(bad) = slide.timeline.shape_refs().find(|t| !on.contains(t)) {
                    return Err(invalid(format!("an effect targets missing shape {bad}")));
                }
                let ids = ids_of_slide(&slide);
                self.check_fresh(&ids, &[])?;
                self.bump_ids(&ids);
                self.slides.insert(index, *slide);
                Op::RemoveSlide { index }
            }
            Op::RemoveSlide { index } => {
                if index >= self.slides.len() {
                    return Err(range(index, self.slides.len()));
                }
                Op::InsertSlide {
                    index,
                    slide: Box::new(self.slides.remove(index)),
                }
            }
            Op::MoveSlide { from, to } => {
                let len = self.slides.len();
                if from >= len {
                    return Err(range(from, len));
                }
                if to >= len {
                    return Err(range(to, len));
                }
                let s = self.slides.remove(from);
                self.slides.insert(to, s);
                Op::MoveSlide { from: to, to: from }
            }
            Op::SetSlideLayout { slide, layout } => {
                if self.layout(layout).is_none() {
                    return Err(DeckError::UnknownNode(layout));
                }
                Op::SetSlideLayout {
                    slide,
                    layout: std::mem::replace(&mut self.slide_or_err(slide)?.layout, layout),
                }
            }
            Op::SetSlideHidden { slide, hidden } => Op::SetSlideHidden {
                slide,
                hidden: std::mem::replace(&mut self.slide_or_err(slide)?.hidden, hidden),
            },
            Op::SetSlideName { slide, name } => Op::SetSlideName {
                slide,
                name: std::mem::replace(&mut self.slide_or_err(slide)?.name, name),
            },
            Op::SetTransition { slide, transition } => Op::SetTransition {
                slide,
                transition: std::mem::replace(
                    &mut self.slide_or_err(slide)?.transition,
                    transition,
                ),
            },
            Op::SetNotes { slide, notes } => {
                if let Some(n) = &notes {
                    n.text.validate()?;
                    let mut ids = Vec::new();
                    shape_ids(&n.shapes, &mut ids);
                    let mut old = Vec::new();
                    if let Some(o) = self
                        .slide(slide)
                        .ok_or(DeckError::UnknownNode(slide))?
                        .notes
                        .as_ref()
                    {
                        shape_ids(&o.shapes, &mut old);
                    }
                    self.check_fresh(&ids, &old)?;
                    self.bump_ids(&ids);
                }
                Op::SetNotes {
                    slide,
                    notes: std::mem::replace(
                        &mut self.slide_or_err(slide)?.notes,
                        notes.map(|n| *n),
                    )
                    .map(Box::new),
                }
            }
            // ---------------------------------------------------- animation
            Op::InsertEffect {
                slide,
                index,
                effect,
            } => {
                let s = self.slide(slide).ok_or(DeckError::UnknownNode(slide))?;
                let len = s.timeline.sequence.len();
                if index > len {
                    return Err(range(index, len));
                }
                Self::check_effect_target(s, &effect)?;
                self.check_fresh(&[effect.id], &[])?;
                self.bump_ids(&[effect.id]);
                self.slide_or_err(slide)?
                    .timeline
                    .sequence
                    .insert(index, *effect);
                Op::RemoveEffect { slide, index }
            }
            Op::RemoveEffect { slide, index } => {
                let s = self.slide_or_err(slide)?;
                let len = s.timeline.sequence.len();
                if index >= len {
                    return Err(range(index, len));
                }
                Op::InsertEffect {
                    slide,
                    index,
                    effect: Box::new(s.timeline.sequence.remove(index)),
                }
            }
            Op::SetEffect {
                slide,
                index,
                effect,
            } => {
                let s = self.slide(slide).ok_or(DeckError::UnknownNode(slide))?;
                let len = s.timeline.sequence.len();
                let old = s.timeline.sequence.get(index).ok_or(range(index, len))?;
                if old.id != effect.id {
                    return Err(invalid("an effect keeps its id"));
                }
                Self::check_effect_target(s, &effect)?;
                let s = self.slide_or_err(slide)?;
                Op::SetEffect {
                    slide,
                    index,
                    effect: Box::new(std::mem::replace(&mut s.timeline.sequence[index], *effect)),
                }
            }
            Op::MoveEffect { slide, from, to } => {
                let s = self.slide_or_err(slide)?;
                let len = s.timeline.sequence.len();
                if from >= len {
                    return Err(range(from, len));
                }
                if to >= len {
                    return Err(range(to, len));
                }
                let e = s.timeline.sequence.remove(from);
                s.timeline.sequence.insert(to, e);
                Op::MoveEffect {
                    slide,
                    from: to,
                    to: from,
                }
            }
            Op::SetTriggers { slide, triggers } => {
                let s = self.slide(slide).ok_or(DeckError::UnknownNode(slide))?;
                let on = shapes_on(&s.shapes);
                for t in &triggers {
                    if !on.contains(&t.trigger) {
                        return Err(invalid(format!(
                            "trigger {} is not on the slide",
                            t.trigger
                        )));
                    }
                    for e in &t.effects {
                        Self::check_effect_target(s, e)?;
                    }
                }
                let new: Vec<NodeId> = triggers
                    .iter()
                    .flat_map(|t| t.effects.iter().map(|e| e.id))
                    .collect();
                let old: Vec<NodeId> = s
                    .timeline
                    .triggers
                    .iter()
                    .flat_map(|t| t.effects.iter().map(|e| e.id))
                    .collect();
                self.check_fresh(&new, &old)?;
                self.bump_ids(&new);
                Op::SetTriggers {
                    slide,
                    triggers: std::mem::replace(
                        &mut self.slide_or_err(slide)?.timeline.triggers,
                        triggers,
                    ),
                }
            }
            // ---------------------------------------------------- comments
            Op::InsertComment {
                slide,
                index,
                comment,
            } => {
                let len = self
                    .slide(slide)
                    .ok_or(DeckError::UnknownNode(slide))?
                    .comments
                    .len();
                if index > len {
                    return Err(range(index, len));
                }
                let ids = ids_of_comment(&comment);
                self.check_fresh(&ids, &[])?;
                self.bump_ids(&ids);
                self.slide_or_err(slide)?.comments.insert(index, *comment);
                Op::RemoveComment { slide, index }
            }
            Op::RemoveComment { slide, index } => {
                let s = self.slide_or_err(slide)?;
                let len = s.comments.len();
                if index >= len {
                    return Err(range(index, len));
                }
                Op::InsertComment {
                    slide,
                    index,
                    comment: Box::new(s.comments.remove(index)),
                }
            }
            Op::SetComment {
                slide,
                index,
                comment,
            } => {
                let s = self.slide(slide).ok_or(DeckError::UnknownNode(slide))?;
                let len = s.comments.len();
                let old = s.comments.get(index).ok_or(range(index, len))?;
                if old.id != comment.id {
                    return Err(invalid("a comment keeps its id"));
                }
                let new_ids = ids_of_comment(&comment);
                let old_ids = ids_of_comment(old);
                self.check_fresh(&new_ids, &old_ids)?;
                self.bump_ids(&new_ids);
                let s = self.slide_or_err(slide)?;
                Op::SetComment {
                    slide,
                    index,
                    comment: Box::new(std::mem::replace(&mut s.comments[index], *comment)),
                }
            }
            // ---------------------------------------------------- shapes
            Op::InsertShapes {
                parent,
                index,
                shapes,
            } => {
                let len = self
                    .shape_list(parent)
                    .ok_or(DeckError::UnknownNode(parent))?
                    .len();
                if index > len {
                    return Err(range(index, len));
                }
                let mut ids = Vec::new();
                shape_ids(&shapes, &mut ids);
                self.check_fresh(&ids, &[])?;
                self.bump_ids(&ids);
                let count = shapes.len();
                let list = self
                    .shape_list_mut(parent)
                    .ok_or(DeckError::UnknownNode(parent))?;
                list.splice(index..index, shapes);
                Op::RemoveShapes {
                    parent,
                    index,
                    count,
                }
            }
            Op::RemoveShapes {
                parent,
                index,
                count,
            } => {
                let list = self
                    .shape_list(parent)
                    .ok_or(DeckError::UnknownNode(parent))?;
                let end = index.checked_add(count).ok_or(range(index, list.len()))?;
                if end > list.len() {
                    return Err(range(end, list.len()));
                }
                let mut ids = Vec::new();
                shape_ids(&list[index..end], &mut ids);
                self.check_not_animated(self.slide_of_parent(parent), &ids)?;
                let list = self
                    .shape_list_mut(parent)
                    .ok_or(DeckError::UnknownNode(parent))?;
                let shapes: Vec<Shape> = list.drain(index..end).collect();
                Op::InsertShapes {
                    parent,
                    index,
                    shapes,
                }
            }
            Op::MoveShape { parent, from, to } => {
                let list = self
                    .shape_list_mut(parent)
                    .ok_or(DeckError::UnknownNode(parent))?;
                let len = list.len();
                if from >= len {
                    return Err(range(from, len));
                }
                if to >= len {
                    return Err(range(to, len));
                }
                let s = list.remove(from);
                list.insert(to, s);
                Op::MoveShape {
                    parent,
                    from: to,
                    to: from,
                }
            }
            Op::SetTransform { shape, xfrm } => {
                let s = self.shape_mut(shape).ok_or(DeckError::UnknownNode(shape))?;
                Op::SetTransform {
                    shape,
                    xfrm: std::mem::replace(&mut s.xfrm, xfrm),
                }
            }
            Op::SetShapeProps { shape, props } => {
                let s = self.shape_mut(shape).ok_or(DeckError::UnknownNode(shape))?;
                let slot = s.props_mut().ok_or(DeckError::WrongKind {
                    id: shape,
                    expected: "shape with shape properties",
                })?;
                Op::SetShapeProps {
                    shape,
                    props: Box::new(std::mem::replace(slot, *props)),
                }
            }
            Op::SetPlaceholder { shape, placeholder } => {
                let s = self.shape_mut(shape).ok_or(DeckError::UnknownNode(shape))?;
                Op::SetPlaceholder {
                    shape,
                    placeholder: std::mem::replace(&mut s.placeholder, placeholder),
                }
            }
            Op::SetShapeInfo { shape, info } => {
                let s = self.shape_mut(shape).ok_or(DeckError::UnknownNode(shape))?;
                Op::SetShapeInfo {
                    shape,
                    info: Box::new(info.apply_to(s)),
                }
            }
            Op::SetShapeKind { shape, kind } => {
                let s = self.shape(shape).ok_or(DeckError::UnknownNode(shape))?;
                let old_ids = ids_of_kind(&s.kind);
                let new_ids = ids_of_kind(&kind);
                self.check_fresh(&new_ids, &old_ids)?;
                let gone: Vec<NodeId> = old_ids
                    .iter()
                    .copied()
                    .filter(|i| !new_ids.contains(i))
                    .collect();
                self.check_not_animated(self.slide_of_shape(shape).map(|s| s.id), &gone)?;
                self.bump_ids(&new_ids);
                let s = self.shape_mut(shape).ok_or(DeckError::UnknownNode(shape))?;
                Op::SetShapeKind {
                    shape,
                    kind: Box::new(std::mem::replace(&mut s.kind, *kind)),
                }
            }
            Op::SetChildSpace { group, space } => {
                let s = self.shape_mut(group).ok_or(DeckError::UnknownNode(group))?;
                let ShapeKind::Group(g) = &mut s.kind else {
                    return Err(DeckError::WrongKind {
                        id: group,
                        expected: "group",
                    });
                };
                Op::SetChildSpace {
                    group,
                    space: std::mem::replace(&mut g.child_space, space),
                }
            }
            // ---------------------------------------------------- text
            Op::SetTextBody { shape, text } => {
                if let Some(t) = &text {
                    t.validate()?;
                }
                let s = self.shape_mut(shape).ok_or(DeckError::UnknownNode(shape))?;
                let ShapeKind::Geometry(a) = &mut s.kind else {
                    return Err(DeckError::WrongKind {
                        id: shape,
                        expected: "geometry shape",
                    });
                };
                Op::SetTextBody {
                    shape,
                    text: std::mem::replace(&mut a.text, text.map(|t| *t)).map(Box::new),
                }
            }
            Op::SetBodyProps { target, body } => {
                let t = self.text_body_mut(target)?;
                Op::SetBodyProps {
                    target,
                    body: Box::new(std::mem::replace(&mut t.body, *body)),
                }
            }
            Op::SetListStyle { target, style } => {
                let t = self.text_body_mut(target)?;
                Op::SetListStyle {
                    target,
                    style: Box::new(std::mem::replace(&mut t.list_style, *style)),
                }
            }
            Op::SetParaProps {
                target,
                para,
                props,
            } => {
                let t = self.text_body_mut(target)?;
                let old = t.set_deck_props(para, props.map(|p| *p))?;
                Op::SetParaProps {
                    target,
                    para,
                    props: old.map(Box::new),
                }
            }
            Op::EditText {
                target,
                tx,
                para_props,
            } => {
                let t = self.text_body_mut(target)?;
                let (inverse, before) = t.apply_text(tx, para_props)?;
                Op::EditText {
                    target,
                    tx: inverse,
                    para_props: Some(before),
                }
            }
        })
    }
}

#[cfg(test)]
mod tests;
