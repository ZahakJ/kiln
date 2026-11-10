// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck on Loro.
//!
//! ## Mapping
//!
//! | Model | Loro |
//! |---|---|
//! | slides and sections | a `LoroTree` (sections at the root with their slides as children; without sections the slides are the roots) |
//! | a slide's values, comments, notes | the tree node's metadata map ([`shapes`]) |
//! | shapes of a slide or group | movable list `shapes` of shape maps |
//! | text bodies (shapes, notes) | paragraphs with the Write mapping |
//! | masters with their layouts, the notes master | JSON values (merged as a whole) |
//! | slide size, default text style, parts | root maps |
//!
//! [`DeckBinding::commit`] writes what a [`Transaction`] touched; [`DeckBinding::apply_remote`]
//! turns each slide a merge changed into the ops (shape inserts, moves, transforms, text edits
//! as kiln-doc transactions, slide properties) that make the model equal to the shared state,
//! so the editor redraws only those slides. Changes to masters and layouts rebuild the model.

pub(crate) mod shapes;

use std::collections::{HashMap, HashSet};

use kiln_common::NodeId;
use kiln_deck::{
    EditKind, Op, Owner, Part, Presentation, Section, Shape, ShapeKind, Slide, TextBody,
    TextTarget, Transaction,
};
use kiln_doc::{Block, IdLanes};
use loro::{ContainerID, LoroDoc, LoroMap, LoroTree, TreeID, TreeParentId};

use self::shapes::{
    SECTION, SLIDE, order_shapes, read_slide, write_shape, write_slide, write_slide_shallow,
};
use crate::crdt::{Changed, Crdt, LoroCrdt};
use crate::error::{Result, corrupt};
use crate::moves::{Moves, Tracker};
use crate::value::{
    get_bytes, get_id, get_json, get_json_or_default, get_str, id_value, maps_of, put_bytes,
    put_i64, put_json, put_str,
};
use crate::write::blocks::{ID, K, Nodes, kind_of, owner_of, rewrite};
use crate::write::{list_edits, paragraph_ops};

const SLIDES: &str = "slides";
const DECK: &str = "deck";
const MASTERS: &str = "masters";
const PARTS: &str = "parts";

/// What merging a remote update (or an undo) did to the presentation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DeckChange {
    /// The ops applied, in order: the slides they touch are the ones to redraw.
    pub tx: Transaction,
    /// The presentation was rebuilt from the shared state: redraw everything.
    pub reloaded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Unit {
    Slide(NodeId),
    Shapes(NodeId),
    Shape(NodeId),
    SlideList,
    Masters,
    Pres,
    Parts,
    Reload,
}

/// A presentation shared through a [`LoroCrdt`].
#[derive(Debug)]
pub struct DeckBinding {
    crdt: LoroCrdt,
    /// Slide and shape maps by id.
    nodes: Nodes,
    /// Tree nodes of slides and sections by id.
    tree: HashMap<NodeId, TreeID>,
    typing: bool,
    /// The records of moved text, by the text they took it from.
    moves: Moves,
}

impl DeckBinding {
    fn with(crdt: LoroCrdt) -> Self {
        crdt.doc().get_tree(SLIDES).enable_fractional_index(0);
        Self {
            crdt,
            nodes: Nodes::default(),
            tree: HashMap::new(),
            typing: false,
            moves: Moves::default(),
        }
    }

    /// Starts sharing a presentation (see [`crate::WriteBinding::share`]).
    ///
    /// # Errors
    ///
    /// A model value that cannot be stored.
    pub fn share(pres: &mut Presentation, peer: u64, lanes: IdLanes) -> Result<Self> {
        pres.set_id_lanes(lanes);
        let mut b = Self::with(LoroCrdt::new(peer)?);
        b.write_all(pres)?;
        b.crdt.commit_system();
        Ok(b)
    }

    /// Joins a shared presentation from a snapshot or updates.
    ///
    /// # Errors
    ///
    /// Bytes Loro cannot import, or a shared state that is not a valid presentation.
    pub fn join(state: &[u8], peer: u64, lanes: IdLanes) -> Result<(Self, Presentation)> {
        let mut crdt = LoroCrdt::new(peer)?;
        crdt.apply(state)?;
        crdt.take_changes();
        let mut b = Self::with(crdt);
        let mut pres = b.rebuild()?;
        pres.set_id_lanes(lanes);
        Ok((b, pres))
    }

    /// The presentation a snapshot (a version in history) holds.
    ///
    /// # Errors
    ///
    /// A snapshot that is not a valid presentation.
    pub fn checkout(snapshot: &[u8]) -> Result<Presentation> {
        Self::read(LoroCrdt::checkout(snapshot)?)
    }

    /// The presentation a replica holds (a version forked from history).
    pub(crate) fn read(crdt: LoroCrdt) -> Result<Presentation> {
        Self::with(crdt).rebuild()
    }

    /// The CRDT.
    #[must_use]
    pub fn crdt(&self) -> &LoroCrdt {
        &self.crdt
    }

    /// The CRDT, mutably.
    pub fn crdt_mut(&mut self) -> &mut LoroCrdt {
        &mut self.crdt
    }

    /// The presentation the shared state describes, built from scratch.
    ///
    /// # Errors
    ///
    /// A shared state that is not a valid presentation.
    pub fn decode(&self) -> Result<Presentation> {
        read_presentation(self.crdt.doc(), &mut Nodes::default(), &mut HashMap::new())
    }

    fn para_text(&self, shape: NodeId, para: NodeId) -> Option<loro::LoroText> {
        let text = crate::value::find_map(self.nodes.get(shape)?, "text")?;
        let paras = crate::value::find_list(&text, "paras")?;
        let m = maps_of(&paras)
            .into_iter()
            .find(|m| get_id(m, ID).ok() == Some(para))?;
        crate::value::find_text(&m, "text")
    }

    /// An anchor for a caret or selection end in a shape's text (for [`crate::Presence`]).
    #[must_use]
    pub fn anchor(&self, shape: NodeId, pos: kiln_doc::Pos) -> Option<crate::Anchor> {
        let text = self.para_text(shape, pos.para)?;
        crate::presence::anchor_in(&text, Some(shape), pos.para, pos.offset)
    }

    /// Where another peer's anchor is now: the shape and the position in its text.
    #[must_use]
    pub fn resolve(
        &self,
        pres: &Presentation,
        anchor: &crate::Anchor,
    ) -> Option<(NodeId, kiln_doc::Pos)> {
        let shape = anchor.shape?;
        let len = match &pres.shape(shape)?.kind {
            ShapeKind::Geometry(g) => g.text.as_ref()?.paragraph(anchor.para)?.content.len(),
            _ => return None,
        };
        let offset = crate::presence::resolve_in(self.crdt.doc(), anchor)
            .unwrap_or(anchor.offset as usize)
            .min(len);
        Some((shape, kiln_doc::Pos::new(anchor.para, offset)))
    }

    /// Records a transaction the presentation has just applied as one CRDT change.
    ///
    /// # Errors
    ///
    /// A model value that cannot be stored.
    pub fn commit(&mut self, pres: &Presentation, tx: &Transaction) -> Result<()> {
        if tx.is_empty() {
            return Ok(());
        }
        if tx.kind == EditKind::Typing {
            if !self.typing {
                self.crdt.begin_undo_group();
                self.typing = true;
            }
        } else {
            self.break_undo_group();
        }
        self.crdt
            .set_undo_selection(serde_json::to_string(&tx.selection_before).ok());
        let units = self.local_units(pres, tx);
        let at = crate::moves::next_op(self.crdt.doc());
        let moves = self.track_moves(tx);
        if self.sync(pres, &units).is_err() {
            self.write_all(pres)?;
        }
        for (target, tracker) in moves {
            tracker.record(self.crdt.doc(), at, |p| self.body_text(target, p))?;
        }
        self.crdt.commit_local();
        Ok(())
    }

    /// Follows the text a transaction's text edits move between paragraphs, per text body.
    fn track_moves(&self, tx: &Transaction) -> Vec<(TextTarget, Tracker)> {
        let mut out: Vec<(TextTarget, Tracker)> = Vec::new();
        let edits = || {
            tx.ops.iter().filter_map(|op| match op {
                Op::EditText { target, tx, .. } => Some((*target, tx)),
                _ => None,
            })
        };
        for (target, t) in edits() {
            if Tracker::needed(&t.ops) && !out.iter().any(|(x, _)| *x == target) {
                let mut tracker = Tracker::default();
                for (_, t) in edits().filter(|(x, _)| *x == target) {
                    tracker.track(&t.ops, |p| self.body_text(target, p));
                }
                out.push((target, tracker));
            }
        }
        out
    }

    /// A paragraph's text in a text body of the shared state.
    fn body_text(&self, target: TextTarget, para: NodeId) -> Option<loro::LoroText> {
        let body = match target {
            TextTarget::Shape(s) => crate::value::find_map(self.nodes.get(s)?, "text")?,
            TextTarget::Notes(slide) => {
                let notes = crate::value::find_map(self.nodes.get(slide)?, "notes")?;
                crate::value::find_map(&notes, "text")?
            }
            TextTarget::Cell { .. } => return None,
        };
        let list = crate::value::find_list(&body, shapes::PARAS)?;
        maps_of(&list)
            .into_iter()
            .find(|m| get_id(m, ID).ok() == Some(para))
            .and_then(|m| crate::value::find_text(&m, "text"))
    }

    /// Ends the open typing group.
    pub fn break_undo_group(&mut self) {
        if self.typing {
            self.crdt.end_undo_group();
            self.typing = false;
        }
    }

    /// Replaces the shared presentation with a model (restore of a version) as one change.
    ///
    /// # Errors
    ///
    /// A model value that cannot be stored.
    pub fn replace(&mut self, pres: &Presentation) -> Result<()> {
        self.break_undo_group();
        self.write_all(pres)?;
        self.crdt.commit_local();
        Ok(())
    }

    /// Merges an update from another replica.
    ///
    /// # Errors
    ///
    /// Bytes Loro cannot import, or a merged state that is not a valid presentation.
    pub fn apply_remote(&mut self, pres: &mut Presentation, update: &[u8]) -> Result<DeckChange> {
        self.break_undo_group();
        let before = self.crdt.doc().oplog_vv();
        self.crdt.apply(update)?;
        let mut changes = self.crdt.take_changes();
        if self.moves.repair(self.crdt.doc(), &before, &changes)? {
            changes.extend(self.crdt.commit_repair());
        }
        if self.delete_again(&before, &changes)? {
            changes.extend(self.crdt.commit_repair());
        }
        self.merge(pres, &changes)
    }

    /// Deletes again the slides a move made at the same moment as their deletion kept (in a
    /// tree the later of a move and a deletion wins; a deletion should). A slide put back by
    /// someone who had seen it deleted (Undo) stays. Returns whether any was deleted.
    fn delete_again(&self, before: &loro::VersionVector, changes: &[Changed]) -> Result<bool> {
        let ldoc = self.crdt.doc();
        let tree = ldoc.get_tree(SLIDES);
        let alive = |t: &TreeID| tree.contains(*t) && !tree.is_node_deleted(t).unwrap_or(true);
        // Whether the change of operation `m` was made after operation `d` reached its author.
        let saw = |m: loro::ID, d: loro::ID| -> bool {
            if m.peer == d.peer {
                return m.counter > d.counter;
            }
            ldoc.get_change(m)
                .and_then(|c| ldoc.frontiers_to_vv(&c.deps))
                .is_some_and(|vv| vv.includes_id(d))
        };
        let mut doomed: Vec<TreeID> = Vec::new();
        // Deletions in this merge that a move made at the same moment overrode.
        let target = loro::ContainerTrait::id(&tree);
        for span in ldoc.oplog_vv().sub_iter(before) {
            let len = usize::try_from(span.counter.end - span.counter.start).unwrap_or(0);
            if !ldoc
                .get_changed_containers_in(loro::ID::new(span.peer, span.counter.start), len)
                .contains(&target)
            {
                continue;
            }
            for change in ldoc.export_json_in_id_span(span) {
                for op in change.ops.iter().filter(|o| o.container == target) {
                    let loro::json::JsonOpContent::Tree(loro::json::TreeOp::Delete { target: t }) =
                        &op.content
                    else {
                        continue;
                    };
                    let d = loro::ID::new(change.id.peer, op.counter);
                    if alive(t)
                        && tree
                            .get_last_move_id(t)
                            .is_some_and(|m| m != d && !saw(m, d))
                    {
                        doomed.push(*t);
                    }
                }
            }
        }
        // Slides deleted here (not in the presentation before the merge) that a move from this
        // merge, made without seeing that, brought back.
        let had: HashSet<&TreeID> = self.tree.values().collect();
        for t in changes.iter().flat_map(|c| &c.created) {
            if !alive(t) || had.contains(t) || !before.includes_id(loro::ID::new(t.peer, t.counter))
            {
                continue;
            }
            let Some(m) = tree.get_last_move_id(t) else {
                continue;
            };
            let Some(deps) = ldoc.get_change(m).map(|c| c.deps) else {
                continue;
            };
            let unseen = ldoc.fork_at(&deps).ok().is_some_and(|f| {
                let t2 = f.get_tree(SLIDES);
                t2.contains(*t) && !t2.is_node_deleted(t).unwrap_or(true)
            });
            if unseen {
                doomed.push(*t);
            }
        }
        doomed.sort();
        doomed.dedup();
        for t in &doomed {
            if alive(t) {
                tree.delete(*t)?;
            }
        }
        Ok(!doomed.is_empty())
    }

    /// Undoes the local peer's latest change.
    ///
    /// # Errors
    ///
    /// The CRDT failed to undo, or the result is not a valid presentation.
    pub fn undo(&mut self, pres: &mut Presentation) -> Result<Option<DeckChange>> {
        self.break_undo_group();
        if !self.crdt.undo()? {
            return Ok(None);
        }
        self.after_history(pres).map(Some)
    }

    /// Redoes the latest undone local change.
    ///
    /// # Errors
    ///
    /// The CRDT failed to redo, or the result is not a valid presentation.
    pub fn redo(&mut self, pres: &mut Presentation) -> Result<Option<DeckChange>> {
        self.break_undo_group();
        if !self.crdt.redo()? {
            return Ok(None);
        }
        self.after_history(pres).map(Some)
    }

    fn after_history(&mut self, pres: &mut Presentation) -> Result<DeckChange> {
        let changes = self.crdt.take_changes();
        let mut change = self.merge(pres, &changes)?;
        if let Some(sel) = self.crdt.take_undo_selection() {
            change.tx.selection_after = serde_json::from_str(&sel).unwrap_or_default();
        }
        Ok(change)
    }

    // -------------------------------------------------------------- local → shared

    fn local_units(&self, pres: &Presentation, tx: &Transaction) -> Vec<Unit> {
        let mut out: Vec<Unit> = Vec::new();
        let owner = |id: NodeId| match pres.owner_of_id(id) {
            Some(Owner::Slide(s)) => Unit::Slide(s),
            Some(Owner::Layout(_) | Owner::Master(_)) => Unit::Masters,
            Some(Owner::NotesMaster(_)) => Unit::Pres,
            None => Unit::Reload,
        };
        let shape_owner = |id: NodeId| match pres.locate(id) {
            Some(p) => owner(p.owner.id()),
            None => Unit::Reload,
        };
        let shape = |id: NodeId| {
            if self.nodes.get(id).is_some() {
                Unit::Shape(id)
            } else {
                shape_owner(id)
            }
        };
        for op in &tx.ops {
            let u = match op {
                Op::SetSlideSize { .. } | Op::SetDefaultTextStyle { .. } => Unit::Pres,
                Op::SetSections { .. }
                | Op::InsertSlide { .. }
                | Op::RemoveSlide { .. }
                | Op::MoveSlide { .. } => Unit::SlideList,
                Op::SetPart { .. } => Unit::Parts,
                Op::InsertMaster { .. }
                | Op::RemoveMaster { .. }
                | Op::SetTheme { .. }
                | Op::SetMasterTextStyle { .. }
                | Op::SetHeaderFooter { .. }
                | Op::InsertLayout { .. }
                | Op::RemoveLayout { .. }
                | Op::MoveLayout { .. }
                | Op::SetLayoutName { .. }
                | Op::SetMasterInfo { .. } => Unit::Masters,
                Op::SetBackground { owner: o, .. }
                | Op::SetColorMap { owner: o, .. }
                | Op::SetShowMasterShapes { owner: o, .. } => owner(*o),
                Op::SetSlideLayout { slide, .. }
                | Op::SetSlideHidden { slide, .. }
                | Op::SetSlideName { slide, .. }
                | Op::SetTransition { slide, .. }
                | Op::SetNotes { slide, .. }
                | Op::InsertEffect { slide, .. }
                | Op::RemoveEffect { slide, .. }
                | Op::SetEffect { slide, .. }
                | Op::MoveEffect { slide, .. }
                | Op::SetTriggers { slide, .. }
                | Op::InsertComment { slide, .. }
                | Op::RemoveComment { slide, .. }
                | Op::SetComment { slide, .. } => Unit::Slide(*slide),
                Op::InsertShapes { parent, .. }
                | Op::RemoveShapes { parent, .. }
                | Op::MoveShape { parent, .. } => {
                    if self.nodes.get(*parent).is_some() {
                        Unit::Shapes(*parent)
                    } else if pres.slide(*parent).is_some() {
                        Unit::Slide(*parent)
                    } else {
                        shape_owner(*parent)
                    }
                }
                Op::SetTransform { shape: s, .. }
                | Op::SetShapeProps { shape: s, .. }
                | Op::SetPlaceholder { shape: s, .. }
                | Op::SetShapeInfo { shape: s, .. }
                | Op::SetShapeKind { shape: s, .. }
                | Op::SetTextBody { shape: s, .. }
                | Op::SetChildSpace { group: s, .. } => shape(*s),
                Op::SetBodyProps { target, .. }
                | Op::SetListStyle { target, .. }
                | Op::SetParaProps { target, .. }
                | Op::EditText { target, .. } => match *target {
                    TextTarget::Shape(s) | TextTarget::Cell { shape: s, .. } => shape(s),
                    TextTarget::Notes(slide) => Unit::Slide(slide),
                },
            };
            if !out.contains(&u) {
                out.push(u);
            }
        }
        out
    }

    fn write_all(&mut self, pres: &Presentation) -> Result<()> {
        let ldoc = self.crdt.doc().clone();
        write_pres(&ldoc, pres)?;
        write_masters(&ldoc, pres)?;
        write_parts(&ldoc, pres)?;
        self.sync_slide_list(pres, true)
    }

    fn sync(&mut self, pres: &Presentation, units: &[Unit]) -> Result<()> {
        if units.contains(&Unit::Reload) {
            return self.write_all(pres);
        }
        let ldoc = self.crdt.doc().clone();
        if units.contains(&Unit::SlideList) {
            self.sync_slide_list(pres, false)?;
        }
        for remove in [true, false] {
            for u in units {
                if let Unit::Shapes(parent) = *u
                    && let (Some(m), Some(list)) =
                        (self.nodes.get(parent).cloned(), shape_list(pres, parent))
                {
                    order_shapes(&m, list, &mut self.nodes, remove)?;
                }
            }
        }
        for u in units {
            match *u {
                Unit::Slide(id) => {
                    if let (Some(s), Some(m)) = (pres.slide(id), self.nodes.get(id).cloned()) {
                        write_slide_shallow(&m, s, &mut self.nodes)?;
                    }
                }
                Unit::Shape(id) => {
                    if let (Some(s), Some(m)) = (pres.shape(id), self.nodes.get(id).cloned()) {
                        write_shape(&m, s, &mut self.nodes)?;
                    }
                }
                Unit::Masters => write_masters(&ldoc, pres)?,
                Unit::Pres => write_pres(&ldoc, pres)?,
                Unit::Parts => write_parts(&ldoc, pres)?,
                _ => {}
            }
        }
        Ok(())
    }

    /// Brings the slide tree level with the model: sections and slides in order, new slides
    /// written in full (`deep`: every slide written again).
    fn sync_slide_list(&mut self, pres: &Presentation, deep: bool) -> Result<()> {
        let tree = self.crdt.doc().get_tree(SLIDES);
        let mut wanted: HashSet<TreeID> = HashSet::new();
        let groups = slide_groups(pres);
        let mut root_index = 0;
        for (section, slides) in &groups {
            let parent = match section {
                Some(sec) => {
                    let node = self.place_node(&tree, sec.id, TreeParentId::Root, root_index)?;
                    root_index += 1;
                    wanted.insert(node);
                    let meta = tree.get_meta(node)?;
                    put_str(&meta, K, SECTION)?;
                    put_i64(&meta, ID, id_value(sec.id))?;
                    put_str(&meta, "name", &sec.name)?;
                    TreeParentId::Node(node)
                }
                None => TreeParentId::Root,
            };
            for (j, slide) in slides.iter().enumerate() {
                let index = if section.is_some() { j } else { root_index + j };
                let fresh = !self.tree.contains_key(&slide.id)
                    || !tree.contains(self.tree[&slide.id])
                    || tree.is_node_deleted(&self.tree[&slide.id]).unwrap_or(true);
                let node = self.place_node(&tree, slide.id, parent, index)?;
                wanted.insert(node);
                if fresh || deep {
                    write_slide(&tree.get_meta(node)?, slide, &mut self.nodes)?;
                }
            }
        }
        for n in tree.nodes() {
            if !wanted.contains(&n) && !tree.is_node_deleted(&n).unwrap_or(true) {
                if let Ok(meta) = tree.get_meta(n) {
                    self.nodes.forget(&meta);
                }
                tree.delete(n)?;
            }
        }
        self.tree.retain(|_, n| wanted.contains(n));
        Ok(())
    }

    /// The tree node of `id` at `index` under `parent`, moved there or created.
    fn place_node(
        &mut self,
        tree: &LoroTree,
        id: NodeId,
        parent: TreeParentId,
        index: usize,
    ) -> Result<TreeID> {
        let live = self
            .tree
            .get(&id)
            .copied()
            .filter(|n| tree.contains(*n) && !tree.is_node_deleted(n).unwrap_or(true));
        let node = match live {
            Some(n) => {
                let here = tree.parent(n) == Some(parent)
                    && tree
                        .children(parent)
                        .and_then(|c| c.iter().position(|x| *x == n))
                        == Some(index);
                if !here {
                    tree.mov_to(n, parent, index)?;
                }
                n
            }
            None => tree.create_at(parent, index)?,
        };
        self.tree.insert(id, node);
        Ok(node)
    }

    // -------------------------------------------------------------- shared → local

    fn merge(&mut self, pres: &mut Presentation, changes: &[Changed]) -> Result<DeckChange> {
        let units = remote_units(self.crdt.doc(), changes);
        if units.is_empty() {
            return Ok(DeckChange::default());
        }
        let mut ops = Vec::new();
        let ok = !units.contains(&Unit::Reload)
            && !units.contains(&Unit::Masters)
            && self.reconcile(pres, &units, &mut ops).is_ok();
        if ok {
            return Ok(DeckChange {
                tx: Transaction::new(ops),
                reloaded: false,
            });
        }
        let lanes = pres.id_lanes();
        let mut fresh = self.rebuild()?;
        fresh.set_id_lanes(lanes);
        *pres = fresh;
        Ok(DeckChange {
            tx: Transaction::default(),
            reloaded: true,
        })
    }

    fn rebuild(&mut self) -> Result<Presentation> {
        self.nodes = Nodes::default();
        self.tree.clear();
        let mut pres = read_presentation(self.crdt.doc(), &mut self.nodes, &mut self.tree)?;
        if repair(&mut pres) {
            self.write_all(&pres)?;
            self.crdt.commit_system();
        }
        pres.validate()?;
        Ok(pres)
    }

    fn reconcile(
        &mut self,
        pres: &mut Presentation,
        units: &[Unit],
        ops: &mut Vec<Op>,
    ) -> Result<()> {
        let ldoc = self.crdt.doc().clone();
        let mut run = |pres: &mut Presentation, batch: Vec<Op>| -> Result<()> {
            if !batch.is_empty() {
                pres.apply(Transaction::new(batch.clone()))?;
                ops.extend(batch);
            }
            Ok(())
        };
        if units.contains(&Unit::SlideList) {
            let (slides, sections) = read_slide_list(&ldoc, &mut self.nodes, &mut self.tree)?;
            let have: Vec<NodeId> = pres.slides.iter().map(|s| s.id).collect();
            let want: Vec<NodeId> = slides.iter().map(|s| s.id).collect();
            let mut batch = Vec::new();
            if have != want {
                let e = list_edits(&have, &want);
                batch.extend(e.removals.iter().map(|&index| Op::RemoveSlide { index }));
                for &(index, id) in &e.inserts {
                    if let Some(s) = slides.iter().find(|s| s.id == id) {
                        batch.push(Op::InsertSlide {
                            index,
                            slide: Box::new(s.clone()),
                        });
                    }
                }
            }
            if pres.sections != sections {
                batch.push(Op::SetSections { sections });
            }
            run(pres, batch)?;
        }
        for u in units {
            if let Unit::Slide(id) = *u
                && let (Some(have), Some(m)) = (pres.slide(id), self.nodes.get(id).cloned())
            {
                let want = read_slide(&m, &mut self.nodes)?;
                let batch = slide_ops(have, &want)?;
                run(pres, batch)?;
            }
        }
        if units.contains(&Unit::Pres) {
            let root = ldoc.get_map(DECK);
            let size = get_json(&root, "size")?;
            let style = get_json(&root, "text_style")?;
            let mut batch = Vec::new();
            if pres.slide_size != size {
                batch.push(Op::SetSlideSize { size });
            }
            if pres.default_text_style != style {
                batch.push(Op::SetDefaultTextStyle {
                    style: Box::new(style),
                });
            }
            let other = (
                get_json(&root, "notes_size")?,
                get_json_or_default::<u32>(&root, "first")?,
                get_json_or_default(&root, "notes_master")?,
            );
            if (
                pres.notes_size.clone(),
                pres.first_slide_number,
                pres.notes_master.clone(),
            ) != other
            {
                return Err(corrupt("presentation settings without ops changed"));
            }
            run(pres, batch)?;
        }
        if units.contains(&Unit::Parts) {
            let parts = read_parts(&ldoc)?;
            let mut batch = Vec::new();
            for (k, p) in &parts {
                if pres.parts.get(k) != Some(p) {
                    batch.push(Op::SetPart {
                        key: k.clone(),
                        part: Some(p.clone()),
                    });
                }
            }
            for k in pres.parts.keys() {
                if !parts.contains_key(k) {
                    batch.push(Op::SetPart {
                        key: k.clone(),
                        part: None,
                    });
                }
            }
            run(pres, batch)?;
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ helpers

/// Makes a merged presentation valid where concurrent edits left it invalid: a slide whose
/// layout was deleted takes the first layout, and animations of deleted shapes go. Returns
/// whether anything changed.
fn repair(pres: &mut Presentation) -> bool {
    let layouts: HashSet<NodeId> = pres
        .masters
        .iter()
        .flat_map(|m| m.layouts.iter().map(|l| l.id))
        .collect();
    let first = pres
        .masters
        .first()
        .and_then(|m| m.layouts.first())
        .map(|l| l.id);
    let mut changed = false;
    // A shape two people moved into different groups at once is in both: the first stays, and
    // a group left empty goes.
    fn dedupe(shapes: &mut Vec<Shape>, seen: &mut HashSet<NodeId>, changed: &mut bool) {
        shapes.retain_mut(|s| {
            if !seen.insert(s.id) {
                *changed = true;
                return false;
            }
            match s.children_mut() {
                Some(c) => {
                    dedupe(c, seen, changed);
                    let keep = !c.is_empty();
                    *changed |= !keep;
                    keep
                }
                None => true,
            }
        });
    }
    let mut seen: HashSet<NodeId> = pres
        .masters
        .iter()
        .flat_map(|m| {
            let mut ids = vec![m.id];
            ids.extend(m.layouts.iter().map(|l| l.id));
            ids
        })
        .collect();
    for s in &mut pres.slides {
        seen.insert(s.id);
        dedupe(&mut s.shapes, &mut seen, &mut changed);
    }
    for s in &mut pres.slides {
        if !layouts.contains(&s.layout)
            && let Some(f) = first
        {
            s.layout = f;
            changed = true;
        }
        let mut on = HashSet::new();
        for sh in &s.shapes {
            sh.walk(&mut |x| {
                on.insert(x.id);
            });
        }
        let t = &mut s.timeline;
        let before = (t.sequence.len(), t.triggers.len());
        t.sequence.retain(|e| on.contains(&e.target.shape));
        t.triggers.retain(|tr| on.contains(&tr.trigger));
        let mut lost = false;
        for tr in &mut t.triggers {
            let n = tr.effects.len();
            tr.effects.retain(|e| on.contains(&e.target.shape));
            lost |= tr.effects.len() != n;
        }
        changed |= lost || before != (t.sequence.len(), t.triggers.len());
    }
    changed
}

fn shape_list(pres: &Presentation, parent: NodeId) -> Option<&Vec<Shape>> {
    pres.slide(parent)
        .map(|s| &s.shapes)
        .or_else(|| pres.shape(parent).and_then(Shape::children))
}

/// Slides grouped by section (one group without a section when there are none).
fn slide_groups(pres: &Presentation) -> Vec<(Option<&Section>, &[Slide])> {
    if pres.sections.is_empty() {
        return vec![(None, &pres.slides[..])];
    }
    let mut out = Vec::new();
    let mut at = 0;
    for s in &pres.sections {
        let end = (at + s.len).min(pres.slides.len());
        out.push((Some(s), &pres.slides[at..end]));
        at = end;
    }
    out
}

fn write_pres(ldoc: &LoroDoc, pres: &Presentation) -> Result<()> {
    let root = ldoc.get_map(DECK);
    put_json(&root, "size", &pres.slide_size)?;
    put_json(&root, "notes_size", &pres.notes_size)?;
    put_json(&root, "text_style", &pres.default_text_style)?;
    put_json(&root, "first", &pres.first_slide_number)?;
    put_json(&root, "notes_master", &pres.notes_master)
}

fn write_masters(ldoc: &LoroDoc, pres: &Presentation) -> Result<()> {
    let write = |m: &LoroMap, master: &kiln_deck::Master, _: &mut Nodes| -> Result<()> {
        put_i64(m, ID, id_value(master.id))?;
        put_json(m, "v", master)
    };
    rewrite(
        &ldoc.get_movable_list(MASTERS),
        &pres.masters,
        |m| m.id,
        write,
        &mut Nodes::default(),
    )
}

fn write_parts(ldoc: &LoroDoc, pres: &Presentation) -> Result<()> {
    let map = ldoc.get_map(PARTS);
    let stale: Vec<String> = map
        .keys()
        .map(|k| k.to_string())
        .filter(|k| !pres.parts.contains_key(k))
        .collect();
    for k in stale {
        map.delete(&k)?;
    }
    for (k, p) in &pres.parts {
        put_bytes(&map, k, &postcard::to_allocvec(p)?)?;
    }
    Ok(())
}

fn read_parts(ldoc: &LoroDoc) -> Result<std::collections::BTreeMap<String, Part>> {
    let map = ldoc.get_map(PARTS);
    let mut out = std::collections::BTreeMap::new();
    for k in map.keys() {
        if let Some(b) = get_bytes(&map, &k) {
            out.insert(k.to_string(), postcard::from_bytes(&b)?);
        }
    }
    Ok(out)
}

/// The slides and sections the tree holds. Slides at the root before any section belong to the
/// first section when there are sections.
fn read_slide_list(
    ldoc: &LoroDoc,
    nodes: &mut Nodes,
    tree_ids: &mut HashMap<NodeId, TreeID>,
) -> Result<(Vec<Slide>, Vec<Section>)> {
    let tree = ldoc.get_tree(SLIDES);
    let mut seen = HashSet::new();
    let mut loose: Vec<Slide> = Vec::new();
    let mut sections: Vec<(Section, Vec<Slide>)> = Vec::new();
    let mut read_slide_node = |n: TreeID,
                               nodes: &mut Nodes,
                               ids: &mut HashMap<NodeId, TreeID>|
     -> Result<Option<Slide>> {
        let meta = tree.get_meta(n)?;
        if kind_of(&meta).as_deref() != Some(SLIDE) {
            return Ok(None);
        }
        let s = read_slide(&meta, nodes)?;
        if !seen.insert(s.id) {
            return Ok(None);
        }
        ids.insert(s.id, n);
        Ok(Some(s))
    };
    for root in tree.roots() {
        let meta = tree.get_meta(root)?;
        match kind_of(&meta).as_deref() {
            Some(SECTION) => {
                let id = get_id(&meta, ID)?;
                tree_ids.insert(id, root);
                let mut slides = Vec::new();
                for c in tree.children(TreeParentId::Node(root)).unwrap_or_default() {
                    if let Some(s) = read_slide_node(c, nodes, tree_ids)? {
                        slides.push(s);
                    }
                }
                let name = get_str(&meta, "name").unwrap_or_default();
                sections.push((
                    Section {
                        id,
                        name,
                        len: slides.len(),
                    },
                    slides,
                ));
            }
            Some(SLIDE) => {
                if let Some(s) = read_slide_node(root, nodes, tree_ids)? {
                    if let Some((sec, list)) = sections.last_mut() {
                        list.push(s);
                        sec.len += 1;
                    } else {
                        loose.push(s);
                    }
                }
            }
            _ => {}
        }
    }
    if let Some((first, list)) = sections.first_mut() {
        first.len += loose.len();
        let mut all = std::mem::take(&mut loose);
        all.append(list);
        *list = all;
    }
    if sections.is_empty() {
        return Ok((loose, Vec::new()));
    }
    let mut slides = Vec::new();
    let mut secs = Vec::new();
    for (s, list) in sections {
        secs.push(s);
        slides.extend(list);
    }
    Ok((slides, secs))
}

fn read_presentation(
    ldoc: &LoroDoc,
    nodes: &mut Nodes,
    tree_ids: &mut HashMap<NodeId, TreeID>,
) -> Result<Presentation> {
    let mut pres = Presentation::empty();
    let root = ldoc.get_map(DECK);
    pres.slide_size = get_json(&root, "size")?;
    pres.notes_size = get_json(&root, "notes_size")?;
    pres.default_text_style = get_json(&root, "text_style")?;
    pres.first_slide_number = get_json_or_default(&root, "first")?;
    pres.notes_master = get_json_or_default(&root, "notes_master")?;
    let mut seen = HashSet::new();
    for m in maps_of(&ldoc.get_movable_list(MASTERS)) {
        if get_id(&m, ID).is_ok_and(|id| seen.insert(id)) {
            pres.masters.push(get_json(&m, "v")?);
        }
    }
    let (slides, sections) = read_slide_list(ldoc, nodes, tree_ids)?;
    pres.slides = slides;
    pres.sections = sections;
    pres.parts = read_parts(ldoc)?;
    pres.refresh_id_counter();
    Ok(pres)
}

/// What a merge changed.
fn remote_units(ldoc: &LoroDoc, changes: &[Changed]) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    for c in changes {
        let mut path = c.path.clone();
        if path.last().is_none_or(|(cid, _)| *cid != c.target) {
            path.push((c.target.clone(), loro::Index::Seq(0)));
        }
        let root = crate::write::root_of(ldoc, c);
        if matches!(&root, ContainerID::Root { name, .. }
            if name.as_str() == crate::history::VERSIONS || name.as_str() == crate::moves::MOVES)
        {
            continue;
        }
        let unit = slide_owner(ldoc, &path).unwrap_or_else(|| match &root {
            ContainerID::Root { name, .. } => match name.as_str() {
                SLIDES => Unit::SlideList,
                MASTERS => Unit::Masters,
                PARTS => Unit::Parts,
                DECK => Unit::Pres,
                _ => Unit::Reload,
            },
            _ => Unit::Reload,
        });
        if !out.contains(&unit) {
            out.push(unit);
        }
    }
    out
}

/// The slide (or section) a changed container belongs to.
fn slide_owner(ldoc: &LoroDoc, path: &[(ContainerID, loro::Index)]) -> Option<Unit> {
    let mut path = path.to_vec();
    while let Some((m, _)) = owner_of(ldoc, &path) {
        match kind_of(&m).as_deref() {
            Some(SLIDE) => return get_id(&m, ID).ok().map(Unit::Slide),
            Some(SECTION) => return Some(Unit::SlideList),
            _ => {
                let at = path
                    .iter()
                    .position(|(cid, _)| *cid == loro::ContainerTrait::id(&m))?;
                path.truncate(at);
            }
        }
    }
    None
}

/// The ops that make a slide equal to `want` (same id).
fn slide_ops(have: &Slide, want: &Slide) -> Result<Vec<Op>> {
    let slide = have.id;
    let mut ops = Vec::new();
    if have.layout != want.layout {
        ops.push(Op::SetSlideLayout {
            slide,
            layout: want.layout,
        });
    }
    if have.hidden != want.hidden {
        ops.push(Op::SetSlideHidden {
            slide,
            hidden: want.hidden,
        });
    }
    if have.name != want.name {
        ops.push(Op::SetSlideName {
            slide,
            name: want.name.clone(),
        });
    }
    if have.transition != want.transition {
        ops.push(Op::SetTransition {
            slide,
            transition: want.transition.clone(),
        });
    }
    if have.background != want.background {
        ops.push(Op::SetBackground {
            owner: slide,
            background: want.background.clone(),
        });
    }
    if have.color_map != want.color_map {
        ops.push(Op::SetColorMap {
            owner: slide,
            map: want.color_map,
        });
    }
    if have.show_master_shapes != want.show_master_shapes {
        ops.push(Op::SetShowMasterShapes {
            owner: slide,
            show: want.show_master_shapes,
        });
    }
    if have.notes != want.notes {
        ops.push(Op::SetNotes {
            slide,
            notes: want.notes.clone().map(Box::new),
        });
    }
    if have.comments != want.comments {
        for index in (0..have.comments.len()).rev() {
            ops.push(Op::RemoveComment { slide, index });
        }
        for (index, c) in want.comments.iter().enumerate() {
            ops.push(Op::InsertComment {
                slide,
                index,
                comment: Box::new(c.clone()),
            });
        }
    }
    let timeline = have.timeline != want.timeline;
    if timeline {
        // Effects go before the shapes they may target are removed, and come back after.
        for index in (0..have.timeline.sequence.len()).rev() {
            ops.push(Op::RemoveEffect { slide, index });
        }
        if !have.timeline.triggers.is_empty() {
            ops.push(Op::SetTriggers {
                slide,
                triggers: Vec::new(),
            });
        }
    }
    shapes_ops(slide, &have.shapes, &want.shapes, &mut ops)?;
    if timeline {
        for (index, e) in want.timeline.sequence.iter().enumerate() {
            ops.push(Op::InsertEffect {
                slide,
                index,
                effect: Box::new(e.clone()),
            });
        }
        if !want.timeline.triggers.is_empty() {
            ops.push(Op::SetTriggers {
                slide,
                triggers: want.timeline.triggers.clone(),
            });
        }
    }
    Ok(ops)
}

fn shapes_ops(parent: NodeId, have: &[Shape], want: &[Shape], ops: &mut Vec<Op>) -> Result<()> {
    let hid: Vec<NodeId> = have.iter().map(|s| s.id).collect();
    let wid: Vec<NodeId> = want.iter().map(|s| s.id).collect();
    if hid != wid {
        let e = list_edits(&hid, &wid);
        ops.extend(e.removals.iter().map(|&index| Op::RemoveShapes {
            parent,
            index,
            count: 1,
        }));
        for &(index, id) in &e.inserts {
            if let Some(s) = want.iter().find(|s| s.id == id) {
                ops.push(Op::InsertShapes {
                    parent,
                    index,
                    shapes: vec![s.clone()],
                });
            }
        }
    }
    let inserted: HashSet<NodeId> = wid.iter().filter(|id| !hid.contains(id)).copied().collect();
    for w in want {
        if inserted.contains(&w.id) {
            continue;
        }
        if let Some(h) = have.iter().find(|s| s.id == w.id) {
            shape_ops(h, w, ops)?;
        }
    }
    Ok(())
}

fn shape_ops(have: &Shape, want: &Shape, ops: &mut Vec<Op>) -> Result<()> {
    if have == want {
        return Ok(());
    }
    let shape = have.id;
    let bare = |k: &ShapeKind| {
        let mut k = k.clone();
        match &mut k {
            ShapeKind::Geometry(a) => a.text = None,
            ShapeKind::Group(g) => g.children.clear(),
            _ => {}
        }
        k
    };
    if bare(&have.kind) != bare(&want.kind) {
        ops.push(Op::SetShapeKind {
            shape,
            kind: Box::new(want.kind.clone()),
        });
    } else {
        match (&have.kind, &want.kind) {
            (ShapeKind::Geometry(a), ShapeKind::Geometry(b)) => match (&a.text, &b.text) {
                (Some(x), Some(y)) => text_ops(shape, x, y, ops)?,
                (x, y) if x != y => ops.push(Op::SetTextBody {
                    shape,
                    text: y.clone().map(Box::new),
                }),
                _ => {}
            },
            (ShapeKind::Group(a), ShapeKind::Group(b)) => {
                shapes_ops(shape, &a.children, &b.children, ops)?
            }
            _ => {}
        }
    }
    if have.xfrm != want.xfrm {
        ops.push(Op::SetTransform {
            shape,
            xfrm: want.xfrm,
        });
    }
    if have.placeholder != want.placeholder {
        ops.push(Op::SetPlaceholder {
            shape,
            placeholder: want.placeholder,
        });
    }
    let info = kiln_deck::ops::ShapeInfo::of(want);
    if kiln_deck::ops::ShapeInfo::of(have) != info {
        ops.push(Op::SetShapeInfo {
            shape,
            info: Box::new(info),
        });
    }
    Ok(())
}

/// The ops that make a text body equal to `want`: its properties, and its paragraphs as one
/// kiln-doc transaction.
fn text_ops(shape: NodeId, have: &TextBody, want: &TextBody, ops: &mut Vec<Op>) -> Result<()> {
    let target = TextTarget::Shape(shape);
    if have.body != want.body {
        ops.push(Op::SetBodyProps {
            target,
            body: Box::new(want.body.clone()),
        });
    }
    if have.list_style != want.list_style {
        ops.push(Op::SetListStyle {
            target,
            style: Box::new(want.list_style.clone()),
        });
    }
    let hp: Vec<&kiln_doc::Paragraph> = have.paragraphs().collect();
    let wp: Vec<&kiln_doc::Paragraph> = want.paragraphs().collect();
    let props_differ = have.all_deck_props() != want.all_deck_props();
    if hp == wp && !props_differ {
        return Ok(());
    }
    // The paragraph ops are worked out on a copy of the body's document, each applied as it
    // is made, so later ones see the indices earlier ones produced.
    let mut doc = have.doc().clone();
    let container = have.container_id();
    let mut doc_ops = Vec::new();
    let mut apply = |doc: &mut kiln_doc::Document, op: kiln_doc::Op| -> Result<()> {
        doc.apply(kiln_doc::Transaction::new(vec![op.clone()]))?;
        doc_ops.push(op);
        Ok(())
    };
    let hid: Vec<NodeId> = hp.iter().map(|p| p.id).collect();
    let wid: Vec<NodeId> = wp.iter().map(|p| p.id).collect();
    if hid != wid {
        let e = list_edits(&hid, &wid);
        let (now, held) = e.split_removals();
        for &index in now {
            apply(
                &mut doc,
                kiln_doc::Op::RemoveBlocks {
                    container,
                    index,
                    count: 1,
                },
            )?;
        }
        for &(index, id) in &e.inserts {
            if let Some(p) = wp.iter().find(|p| p.id == id) {
                apply(
                    &mut doc,
                    kiln_doc::Op::InsertBlocks {
                        container,
                        index,
                        blocks: vec![Block::Paragraph((*p).clone())],
                    },
                )?;
            }
        }
        if held {
            let len = doc.container(container).map_or(0, Vec::len);
            apply(
                &mut doc,
                kiln_doc::Op::RemoveBlocks {
                    container,
                    index: len - 1,
                    count: 1,
                },
            )?;
        }
    }
    for w in &wp {
        if let Some(h) = doc.paragraph(w.id).cloned() {
            for op in paragraph_ops(&h, w)? {
                apply(&mut doc, op)?;
            }
        }
    }
    ops.push(Op::EditText {
        target,
        tx: kiln_doc::Transaction::new(doc_ops),
        para_props: Some(want.all_deck_props().clone()),
    });
    Ok(())
}

#[cfg(test)]
mod tests;
