// SPDX-License-Identifier: AGPL-3.0-or-later
//! The presentation: slide size, masters with their themes and layouts, slides, sections, the
//! notes master, the default text style and the parts store (media, charts, kept parts); id
//! allocation, lookups and validation.

use std::collections::{BTreeMap, HashSet};

use kiln_common::NodeId;
use kiln_ooxml::drawingml::theme::Theme;
use serde::{Deserialize, Serialize};

use crate::error::{DeckError, Result};
use crate::geom::MAX_COORD;
use crate::shape::{Shape, ShapeKind};
use crate::slide::{Layout, Master, NotesMaster, Section, Slide, SlideSize};
use crate::text::{ListStyle, TextBody};

/// A binary part: media, a chart part, an embedded workbook or a part kept from a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Part {
    /// The media type (`image/png`, `application/vnd.openxmlformats-officedocument.drawingml.chart+xml`).
    pub content_type: String,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// What owns a list of shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Owner {
    /// A slide.
    Slide(NodeId),
    /// A layout.
    Layout(NodeId),
    /// A master.
    Master(NodeId),
    /// The notes master.
    NotesMaster(NodeId),
}

impl Owner {
    /// The owner's id.
    #[must_use]
    pub const fn id(self) -> NodeId {
        match self {
            Self::Slide(i) | Self::Layout(i) | Self::Master(i) | Self::NotesMaster(i) => i,
        }
    }
}

/// Where a shape is: its owner, and the index path from the owner's shape list down through
/// groups.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ShapePath {
    /// The slide, layout or master.
    pub owner: Owner,
    /// Indices: `path[0]` in the owner's list, then in each group's children.
    pub path: Vec<usize>,
}

impl ShapePath {
    /// The id of the list the shape is in: the owner, or the enclosing group.
    #[must_use]
    pub fn parent_id(&self, pres: &Presentation) -> Option<NodeId> {
        if self.path.len() <= 1 {
            return Some(self.owner.id());
        }
        let parent = ShapePath {
            owner: self.owner,
            path: self.path[..self.path.len() - 1].to_vec(),
        };
        pres.shape_at(&parent).map(|s| s.id)
    }

    /// The index in the parent list.
    #[must_use]
    pub fn index(&self) -> usize {
        self.path.last().copied().unwrap_or(0)
    }
}

/// A presentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Presentation {
    /// Slide size.
    pub slide_size: SlideSize,
    /// Notes page size.
    pub notes_size: SlideSize,
    /// Masters, each with its theme and layouts (at least one master with one layout).
    pub masters: Vec<Master>,
    /// Slides in order.
    pub slides: Vec<Slide>,
    /// Sections (empty, or partitioning every slide).
    pub sections: Vec<Section>,
    /// The notes master.
    pub notes_master: Option<NotesMaster>,
    /// `p:defaultTextStyle`.
    pub default_text_style: ListStyle,
    /// `firstSlideNum`.
    pub first_slide_number: u32,
    /// Media, charts and kept parts by key (the part name in a `.pptx`).
    pub parts: BTreeMap<String, Part>,
    next_id: u64,
}

impl PartialEq for Presentation {
    /// Two presentations are equal when their content is; the id allocator is bookkeeping.
    fn eq(&self, other: &Self) -> bool {
        self.slide_size == other.slide_size
            && self.notes_size == other.notes_size
            && self.masters == other.masters
            && self.slides == other.slides
            && self.sections == other.sections
            && self.notes_master == other.notes_master
            && self.default_text_style == other.default_text_style
            && self.first_slide_number == other.first_slide_number
            && self.parts == other.parts
    }
}

/// Every id in a list of shapes (descending into groups).
pub(crate) fn shape_ids(shapes: &[Shape], out: &mut Vec<NodeId>) {
    for s in shapes {
        s.walk(&mut |x| out.push(x.id));
    }
}

fn find_path(shapes: &[Shape], id: NodeId, path: &mut Vec<usize>) -> bool {
    for (i, s) in shapes.iter().enumerate() {
        path.push(i);
        if s.id == id {
            return true;
        }
        if let Some(c) = s.children()
            && find_path(c, id, path)
        {
            return true;
        }
        path.pop();
    }
    false
}

impl Presentation {
    /// A shell with no masters or slides (not valid until a master with a layout is added).
    /// Readers start here; editors use [`Presentation::new`].
    #[must_use]
    pub fn empty() -> Self {
        Self {
            slide_size: SlideSize::WIDESCREEN,
            notes_size: SlideSize::NOTES,
            masters: Vec::new(),
            slides: Vec::new(),
            sections: Vec::new(),
            notes_master: None,
            default_text_style: ListStyle::default(),
            first_slide_number: 1,
            parts: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// A fresh id, never handed out before in this presentation.
    pub fn alloc_id(&mut self) -> NodeId {
        let id = NodeId(self.next_id.min(NodeId::MAX.0));
        if self.next_id < NodeId::MAX.0 {
            self.next_id += 1;
        }
        id
    }

    /// The next id [`Self::alloc_id`] will return.
    #[must_use]
    pub fn peek_next_id(&self) -> NodeId {
        NodeId(self.next_id)
    }

    /// Moves the allocator past every id in use (readers that assign their own ids call this).
    pub fn refresh_id_counter(&mut self) {
        let max = self.all_ids().into_iter().map(|i| i.0).max().unwrap_or(0);
        self.next_id = self.next_id.max(max.saturating_add(1)).min(NodeId::MAX.0);
    }

    pub(crate) fn bump_ids(&mut self, ids: &[NodeId]) {
        if let Some(max) = ids.iter().map(|i| i.0).max()
            && max >= self.next_id
        {
            self.next_id = max.saturating_add(1).min(NodeId::MAX.0);
        }
    }

    // -------------------------------------------------------------- lookups

    /// The theme of the first master (the presentation's theme in the Design tab).
    #[must_use]
    pub fn theme(&self) -> Option<&Theme> {
        self.masters.first().map(|m| &m.theme)
    }

    /// A slide by id.
    #[must_use]
    pub fn slide(&self, id: NodeId) -> Option<&Slide> {
        self.slides.iter().find(|s| s.id == id)
    }

    /// Mutable slide.
    pub fn slide_mut(&mut self, id: NodeId) -> Option<&mut Slide> {
        self.slides.iter_mut().find(|s| s.id == id)
    }

    /// The index of a slide.
    #[must_use]
    pub fn slide_index(&self, id: NodeId) -> Option<usize> {
        self.slides.iter().position(|s| s.id == id)
    }

    /// A master by id.
    #[must_use]
    pub fn master(&self, id: NodeId) -> Option<&Master> {
        self.masters.iter().find(|m| m.id == id)
    }

    /// Mutable master.
    pub fn master_mut(&mut self, id: NodeId) -> Option<&mut Master> {
        self.masters.iter_mut().find(|m| m.id == id)
    }

    /// A layout and its master.
    #[must_use]
    pub fn layout(&self, id: NodeId) -> Option<(&Master, &Layout)> {
        self.masters
            .iter()
            .find_map(|m| m.layout(id).map(|l| (m, l)))
    }

    /// Mutable layout.
    pub fn layout_mut(&mut self, id: NodeId) -> Option<&mut Layout> {
        self.masters
            .iter_mut()
            .find_map(|m| m.layouts.iter_mut().find(|l| l.id == id))
    }

    /// The layout and master of a slide.
    #[must_use]
    pub fn slide_layout(&self, slide: &Slide) -> Option<(&Master, &Layout)> {
        self.layout(slide.layout)
    }

    /// Which kind of thing an id names, if any (slides, layouts, masters, the notes master).
    #[must_use]
    pub fn owner_of_id(&self, id: NodeId) -> Option<Owner> {
        if self.slide(id).is_some() {
            Some(Owner::Slide(id))
        } else if self.layout(id).is_some() {
            Some(Owner::Layout(id))
        } else if self.master(id).is_some() {
            Some(Owner::Master(id))
        } else if self.notes_master.as_ref().is_some_and(|n| n.id == id) {
            Some(Owner::NotesMaster(id))
        } else {
            None
        }
    }

    /// The shapes of an owner.
    #[must_use]
    pub fn owner_shapes(&self, owner: Owner) -> Option<&Vec<Shape>> {
        match owner {
            Owner::Slide(i) => self.slide(i).map(|s| &s.shapes),
            Owner::Layout(i) => self.layout(i).map(|(_, l)| &l.shapes),
            Owner::Master(i) => self.master(i).map(|m| &m.shapes),
            Owner::NotesMaster(i) => self
                .notes_master
                .as_ref()
                .filter(|n| n.id == i)
                .map(|n| &n.shapes),
        }
    }

    /// Mutable shapes of an owner.
    pub fn owner_shapes_mut(&mut self, owner: Owner) -> Option<&mut Vec<Shape>> {
        match owner {
            Owner::Slide(i) => self.slide_mut(i).map(|s| &mut s.shapes),
            Owner::Layout(i) => self.layout_mut(i).map(|l| &mut l.shapes),
            Owner::Master(i) => self.master_mut(i).map(|m| &mut m.shapes),
            Owner::NotesMaster(i) => self
                .notes_master
                .as_mut()
                .filter(|n| n.id == i)
                .map(|n| &mut n.shapes),
        }
    }

    fn owners(&self) -> impl Iterator<Item = Owner> + '_ {
        self.slides
            .iter()
            .map(|s| Owner::Slide(s.id))
            .chain(self.masters.iter().flat_map(|m| {
                std::iter::once(Owner::Master(m.id))
                    .chain(m.layouts.iter().map(|l| Owner::Layout(l.id)))
            }))
            .chain(self.notes_master.iter().map(|n| Owner::NotesMaster(n.id)))
    }

    /// Where a shape is.
    #[must_use]
    pub fn locate(&self, id: NodeId) -> Option<ShapePath> {
        for owner in self.owners() {
            let shapes = self.owner_shapes(owner)?;
            let mut path = Vec::new();
            if find_path(shapes, id, &mut path) {
                return Some(ShapePath { owner, path });
            }
        }
        None
    }

    /// The shape at a path.
    #[must_use]
    pub fn shape_at(&self, p: &ShapePath) -> Option<&Shape> {
        let mut list = self.owner_shapes(p.owner)?;
        let (last, init) = p.path.split_last()?;
        for i in init {
            list = list.get(*i)?.children()?;
        }
        list.get(*last)
    }

    /// Mutable shape at a path.
    pub fn shape_at_mut(&mut self, p: &ShapePath) -> Option<&mut Shape> {
        let mut list = self.owner_shapes_mut(p.owner)?;
        let (last, init) = p.path.split_last()?;
        for i in init {
            list = list.get_mut(*i)?.children_mut()?;
        }
        list.get_mut(*last)
    }

    /// A shape anywhere.
    #[must_use]
    pub fn shape(&self, id: NodeId) -> Option<&Shape> {
        self.shape_at(&self.locate(id)?)
    }

    /// Mutable shape anywhere.
    pub fn shape_mut(&mut self, id: NodeId) -> Option<&mut Shape> {
        let p = self.locate(id)?;
        self.shape_at_mut(&p)
    }

    /// The shape list with an id: a slide's, layout's or master's shapes, or a group's children.
    #[must_use]
    pub fn shape_list(&self, parent: NodeId) -> Option<&Vec<Shape>> {
        if let Some(owner) = self.owner_of_id(parent) {
            return self.owner_shapes(owner);
        }
        self.shape(parent)?.children()
    }

    /// Mutable shape list.
    pub fn shape_list_mut(&mut self, parent: NodeId) -> Option<&mut Vec<Shape>> {
        if let Some(owner) = self.owner_of_id(parent) {
            return self.owner_shapes_mut(owner);
        }
        self.shape_mut(parent)?.children_mut()
    }

    /// The slide a shape is on (`None` for layout and master shapes).
    #[must_use]
    pub fn slide_of_shape(&self, id: NodeId) -> Option<&Slide> {
        match self.locate(id)?.owner {
            Owner::Slide(s) => self.slide(s),
            _ => None,
        }
    }

    /// Every slide index with the section it is in.
    #[must_use]
    pub fn section_of_index(&self, index: usize) -> Option<usize> {
        let mut start: usize = 0;
        for (i, s) in self.sections.iter().enumerate() {
            let end = start.saturating_add(s.len);
            if index < end {
                return Some(i);
            }
            start = end;
        }
        None
    }

    /// The first slide index of a section.
    #[must_use]
    pub fn section_start(&self, section: usize) -> usize {
        self.sections
            .iter()
            .take(section)
            .fold(0, |a: usize, s| a.saturating_add(s.len))
    }

    /// Checks that the sections are empty or partition the slides exactly. Lengths come from
    /// files and the control channel, so the sum is checked: each length is bounded by the
    /// slide count and the total may not overflow.
    pub(crate) fn check_sections(&self) -> std::result::Result<(), String> {
        if self.sections.is_empty() {
            return Ok(());
        }
        let n = self.slides.len();
        let total = self.sections.iter().try_fold(0usize, |a, s| {
            if s.len > n {
                None
            } else {
                a.checked_add(s.len)
            }
        });
        match total {
            Some(t) if t == n => Ok(()),
            Some(t) => Err(format!("sections hold {t} slides but there are {n}")),
            None => Err(format!("a section holds more than the {n} slides")),
        }
    }

    // -------------------------------------------------------------- validation

    /// Every id in use.
    #[must_use]
    pub fn all_ids(&self) -> Vec<NodeId> {
        let mut out = Vec::new();
        for m in &self.masters {
            out.push(m.id);
            shape_ids(&m.shapes, &mut out);
            for l in &m.layouts {
                out.push(l.id);
                shape_ids(&l.shapes, &mut out);
            }
        }
        if let Some(n) = &self.notes_master {
            out.push(n.id);
            shape_ids(&n.shapes, &mut out);
        }
        for s in &self.slides {
            out.push(s.id);
            shape_ids(&s.shapes, &mut out);
            out.extend(s.timeline.effect_ids());
            for c in &s.comments {
                out.push(c.id);
                out.extend(c.replies.iter().map(|r| r.id));
            }
            if let Some(n) = &s.notes {
                shape_ids(&n.shapes, &mut out);
            }
        }
        out.extend(self.sections.iter().map(|s| s.id));
        out
    }

    /// Checks every invariant: unique non-zero ids below the allocator, at least one master
    /// with a layout, slides on existing layouts, sections covering the slides, animation
    /// targets on their slide, consistent tables and valid text bodies.
    ///
    /// # Errors
    /// [`DeckError::Corrupt`] naming the first problem.
    pub fn validate(&self) -> Result<()> {
        let corrupt = |m: String| Err(DeckError::Corrupt(m));
        if self.masters.is_empty() {
            return corrupt("a presentation needs a master".into());
        }
        if let Some(m) = self.masters.iter().find(|m| m.layouts.is_empty()) {
            return corrupt(format!("master {} has no layouts", m.id));
        }
        if !self.slide_size.is_valid() {
            return corrupt("the slide size is outside 1 EMU to 56 inches".into());
        }
        let mut seen = HashSet::new();
        for id in self.all_ids() {
            if id.0 == 0 || !seen.insert(id) {
                return corrupt(format!("duplicate or zero id {id}"));
            }
            if id.0 >= self.next_id {
                return corrupt(format!("{id} is not below the id allocator"));
            }
        }
        for s in &self.slides {
            if self.layout(s.layout).is_none() {
                return corrupt(format!("slide {} uses missing layout {}", s.id, s.layout));
            }
            let mut on_slide = HashSet::new();
            for sh in &s.shapes {
                sh.walk(&mut |x| {
                    on_slide.insert(x.id);
                });
            }
            if let Some(bad) = s.timeline.shape_refs().find(|t| !on_slide.contains(t)) {
                return corrupt(format!("slide {} animates missing shape {bad}", s.id));
            }
            if let Some(n) = &s.notes {
                n.text.validate()?;
            }
        }
        self.check_sections().map_err(DeckError::Corrupt)?;
        for owner in self.owners() {
            check_shapes(self.owner_shapes(owner).into_iter().flatten())?;
        }
        for s in &self.slides {
            if let Some(n) = &s.notes {
                check_shapes(&n.shapes)?;
            }
        }
        Ok(())
    }
}

/// Checks shapes and their descendants (see [`check_shape`]).
///
/// # Errors
/// [`DeckError::Corrupt`] naming the first bad shape.
pub(crate) fn check_shapes<'a>(shapes: impl IntoIterator<Item = &'a Shape>) -> Result<()> {
    for s in shapes {
        let mut result = Ok(());
        s.walk(&mut |x| {
            if result.is_ok() {
                result = check_shape(x);
            }
        });
        result?;
    }
    Ok(())
}

/// Checks one shape (not its children): a transform inside the DrawingML coordinate range, and
/// its kind (see [`check_kind`]).
fn check_shape(s: &Shape) -> Result<()> {
    if s.xfrm.is_some_and(|x| !x.is_valid()) {
        return Err(DeckError::Corrupt(format!(
            "shape {} has a position, size or rotation outside the DrawingML range",
            s.id
        )));
    }
    check_kind(s.id, &s.kind)
}

/// Checks a shape kind and, for a group, its children.
///
/// # Errors
/// [`DeckError::Corrupt`] naming the first problem.
pub(crate) fn check_kind_tree(id: NodeId, kind: &ShapeKind) -> Result<()> {
    check_kind(id, kind)?;
    match kind {
        ShapeKind::Group(g) => check_shapes(&g.children),
        _ => Ok(()),
    }
}

/// Checks a shape kind (not a group's children): a group child space inside the coordinate
/// range, a consistent table, valid text bodies.
fn check_kind(id: NodeId, kind: &ShapeKind) -> Result<()> {
    let corrupt = |m: String| Err(DeckError::Corrupt(m));
    let mut texts: Vec<&TextBody> = Vec::new();
    match kind {
        ShapeKind::Group(g) if !g.child_space.is_valid() => {
            return corrupt(format!(
                "group {id} has a child space outside the DrawingML range"
            ));
        }
        ShapeKind::Geometry(a) => texts.extend(a.text.as_ref()),
        ShapeKind::Table(t) => {
            if t.grid.is_empty() || t.rows.is_empty() {
                return corrupt(format!("table {id} is empty"));
            }
            let size_ok = |v: i64| (0..=MAX_COORD).contains(&v);
            if !t.grid.iter().all(|w| size_ok(*w)) || !t.rows.iter().all(|r| size_ok(r.height)) {
                return corrupt(format!(
                    "table {id} has a column width or row height outside the DrawingML range"
                ));
            }
            for r in &t.rows {
                if r.cells.len() != t.grid.len() {
                    return corrupt(format!(
                        "table {id} has a row of {} cells for {} columns",
                        r.cells.len(),
                        t.grid.len()
                    ));
                }
                if r.cells.iter().any(|c| c.grid_span == 0 || c.row_span == 0) {
                    return corrupt(format!("table {id} has a cell spanning nothing"));
                }
                texts.extend(r.cells.iter().map(|c| &c.text));
            }
        }
        _ => {}
    }
    for t in texts {
        t.validate()?;
    }
    Ok(())
}
