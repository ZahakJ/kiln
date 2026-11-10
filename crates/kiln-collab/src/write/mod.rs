// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write (and Kiln Notes, whose pages are Write documents) on Loro.
//!
//! ## Mapping
//!
//! | Model | Loro |
//! |---|---|
//! | sections, headers and footers, notes, comments | root movable lists of node maps |
//! | blocks of a section, cell, story, content control, field result | movable list `blocks` |
//! | paragraph content | `LoroText` with marks ([`text`]) |
//! | paragraph, run, table, row, cell, section properties; comment metadata; revisions | maps with one key per field |
//! | styles | root movable list; document defaults, settings, properties, theme, lists, media in root maps |
//! | inline objects, text boxes, fields, content control settings | JSON values (merged as a whole) |
//!
//! ## Changes in both directions
//!
//! [`WriteBinding::commit`] takes a [`Transaction`] the model has just applied and writes what it
//! touched into the CRDT as one change: the paragraphs, block lists, tables, sections, stories
//! and parts its ops name, each brought level with the model (text by one delete and one insert
//! around the unchanged ends, formatting by marks, lists by moves and inserts, properties field
//! by field). [`WriteBinding::apply_remote`] merges an update, reads which containers it changed
//! from Loro's events, and turns each changed paragraph, list, table, section or part into the
//! model ops that make the model equal to the shared state; those ops are returned, so the
//! editor's layout invalidation stays incremental. A change that no op sequence expresses (a
//! section break made elsewhere, content control settings) rebuilds the model from the shared
//! state and says so ([`WriteChange::reloaded`]).

pub(crate) mod blocks;
mod parts;
pub(crate) mod text;

use std::collections::HashSet;

use kiln_common::NodeId;
use kiln_doc::{Block, Document, EditKind, IdLanes, Op, Paragraph, Range, Transaction};
use loro::{ContainerID, ContainerTrait, LoroDoc, LoroMap, LoroMovableList};

use self::blocks::{
    BLOCKS, CELLS, ID, Nodes, ROWS, StoryRef, kind, kind_of, list_ids, owner_of, place, read_block,
    read_cell, read_comment, read_header_footer, read_note, read_paragraph, read_row, read_section,
    remove_missing, rewrite, write_block, write_cell_shallow, write_paragraph, write_row_shallow,
    write_section, write_section_shallow, write_story, write_story_shallow, write_table_shallow,
};
use self::parts::{install_parts, part_ops, read_parts, write_parts};
use crate::crdt::{Changed, Crdt, LoroCrdt};
use crate::error::{Result, corrupt};
use crate::moves::{Moves, Tracker};
use crate::value::{find_list, get_child_fields, get_fields, get_id, get_json_or_default, maps_of};

const SECTIONS: &str = "sections";
const HEADERS: &str = "headers";
const NOTES: &str = "notes";
const COMMENTS: &str = "comments";

/// What merging a remote update (or an undo) did to the model.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct WriteChange {
    /// The ops applied to the model, in order: what layout invalidation reads.
    pub tx: Transaction,
    /// The model was rebuilt from the shared state: lay the whole document out again.
    pub reloaded: bool,
}

/// What one transaction or one merge touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Unit {
    Para(NodeId),
    /// A paragraph whose text or formatting changed, and nothing else of it.
    Text(NodeId),
    Blocks(NodeId),
    Table(NodeId),
    Row(NodeId),
    Cell(NodeId),
    Section(NodeId),
    Story(NodeId),
    Sections,
    Stories,
    Parts,
    /// Something only a rebuild expresses.
    Reload,
}

/// A Write document shared through a [`LoroCrdt`].
#[derive(Debug)]
pub struct WriteBinding {
    crdt: LoroCrdt,
    nodes: Nodes,
    /// An undo group of typing is open.
    typing: bool,
    /// The records of moved text, by the text they took it from.
    moves: Moves,
    /// Paragraphs the merge in progress read whole from the shared state (their text changes
    /// are in already).
    read: HashSet<NodeId>,
}

impl WriteBinding {
    /// Starts sharing a document: the CRDT is created from it. `lanes` restricts the ids the
    /// document allocates from now on, so that replicas never create the same id.
    ///
    /// # Errors
    ///
    /// A model value that cannot be stored.
    pub fn share(doc: &mut Document, peer: u64, lanes: IdLanes) -> Result<Self> {
        doc.set_id_lanes(lanes);
        let mut b = Self {
            crdt: LoroCrdt::new(peer)?,
            nodes: Nodes::default(),
            typing: false,
            moves: Moves::default(),
            read: HashSet::new(),
        };
        b.write_all(doc)?;
        b.crdt.commit_system();
        Ok(b)
    }

    /// Joins a shared document from a snapshot or updates: returns the binding and the model.
    ///
    /// # Errors
    ///
    /// Bytes Loro cannot import, or a shared state that is not a valid document.
    pub fn join(state: &[u8], peer: u64, lanes: IdLanes) -> Result<(Self, Document)> {
        let mut crdt = LoroCrdt::new(peer)?;
        crdt.apply(state)?;
        crdt.take_changes();
        let mut b = Self {
            crdt,
            nodes: Nodes::default(),
            typing: false,
            moves: Moves::default(),
            read: HashSet::new(),
        };
        let mut doc = b.rebuild()?;
        doc.set_id_lanes(lanes);
        Ok((b, doc))
    }

    /// A read-only binding on a snapshot (a version in history) and its model.
    ///
    /// # Errors
    ///
    /// A snapshot that is not a valid document.
    pub fn checkout(snapshot: &[u8]) -> Result<Document> {
        Self::read(LoroCrdt::checkout(snapshot)?)
    }

    /// The model a replica holds (a version forked from history).
    pub(crate) fn read(crdt: LoroCrdt) -> Result<Document> {
        let mut b = Self {
            crdt,
            nodes: Nodes::default(),
            typing: false,
            moves: Moves::default(),
            read: HashSet::new(),
        };
        b.rebuild()
    }

    /// The CRDT (versions, exports, the local update subscription, history).
    #[must_use]
    pub fn crdt(&self) -> &LoroCrdt {
        &self.crdt
    }

    /// The CRDT, mutably.
    pub fn crdt_mut(&mut self) -> &mut LoroCrdt {
        &mut self.crdt
    }

    /// The model the shared state describes, built from scratch.
    ///
    /// # Errors
    ///
    /// A shared state that is not a valid document.
    pub fn decode(&self) -> Result<Document> {
        let mut nodes = Nodes::default();
        read_document(self.crdt.doc(), &mut nodes)
    }

    /// An anchor for a caret or selection end (for [`crate::Presence`]).
    #[must_use]
    pub fn anchor(&self, pos: kiln_doc::Pos) -> Option<crate::Anchor> {
        let text = crate::value::find_text(self.nodes.get(pos.para)?, "text")?;
        crate::presence::anchor_in(&text, None, pos.para, pos.offset)
    }

    /// Where another peer's anchor is now in this replica's model.
    #[must_use]
    pub fn resolve(&self, doc: &Document, anchor: &crate::Anchor) -> Option<kiln_doc::Pos> {
        let len = doc.paragraph(anchor.para)?.content.len();
        let offset = crate::presence::resolve_in(self.crdt.doc(), anchor)
            .unwrap_or(anchor.offset as usize)
            .min(len);
        Some(kiln_doc::Pos::new(anchor.para, offset))
    }

    // -------------------------------------------------------------- local → shared

    /// Records a transaction the model has just applied as one CRDT change.
    ///
    /// # Errors
    ///
    /// A model value that cannot be stored.
    pub fn commit(&mut self, doc: &Document, tx: &Transaction) -> Result<()> {
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
            .set_undo_selection(selection_json(tx.selection_before));
        let units = local_units(doc, tx, &self.nodes);
        let at = crate::moves::next_op(self.crdt.doc());
        let mut moves = Tracker::default();
        if Tracker::needed(&tx.ops) {
            moves.track(&tx.ops, |p| {
                self.nodes
                    .get(p)
                    .and_then(|m| crate::value::find_text(m, "text"))
            });
        }
        if self.sync(doc, &units).is_err() {
            // A partly written change is completed by writing everything (unchanged values
            // write nothing).
            self.write_all(doc)?;
        }
        moves.record(self.crdt.doc(), at, |p| {
            self.nodes
                .get(p)
                .and_then(|m| crate::value::find_text(m, "text"))
        })?;
        self.crdt.commit_local();
        if tx.kind == EditKind::Typing && ends_word(tx) {
            self.break_undo_group();
        }
        Ok(())
    }

    /// Ends the open typing group (the caret moved): the next edit is a new undo step.
    pub fn break_undo_group(&mut self) {
        if self.typing {
            self.crdt.end_undo_group();
            self.typing = false;
        }
    }

    fn write_all(&mut self, doc: &Document) -> Result<()> {
        let ldoc = self.crdt.doc().clone();
        let nodes = &mut self.nodes;
        let sections = ldoc.get_movable_list(SECTIONS);
        rewrite(&sections, &doc.sections, |s| s.id, write_section, nodes)?;
        write_stories(&ldoc, doc, nodes, true)?;
        write_parts(&ldoc, doc)
    }

    fn sync(&mut self, doc: &Document, units: &[Unit]) -> Result<()> {
        let ldoc = self.crdt.doc().clone();
        if units.contains(&Unit::Reload) {
            return self.write_all(doc);
        }
        let nodes = &mut self.nodes;
        // Removals first, so a block moved from one list to another is never in both.
        for u in units {
            match *u {
                Unit::Sections => {
                    let list = ldoc.get_movable_list(SECTIONS);
                    remove_missing(&list, &doc.sections.iter().map(|s| s.id).collect(), nodes)?;
                }
                Unit::Stories => remove_stories(&ldoc, doc, nodes)?,
                Unit::Blocks(c) => {
                    if let (Some(list), Ok(blocks)) = (block_list(nodes, c), doc.container(c)) {
                        remove_missing(&list, &blocks.iter().map(Block::id).collect(), nodes)?;
                    }
                }
                _ => {}
            }
        }
        for u in units {
            match *u {
                Unit::Sections => {
                    let list = ldoc.get_movable_list(SECTIONS);
                    place(&list, &doc.sections, |s| s.id, write_section, nodes)?;
                }
                Unit::Stories => write_stories(&ldoc, doc, nodes, false)?,
                _ => {}
            }
        }
        for u in units {
            match *u {
                Unit::Blocks(c) => {
                    if let (Some(list), Ok(blocks)) = (block_list(nodes, c), doc.container(c)) {
                        place(&list, blocks, Block::id, write_block, nodes)?;
                    }
                }
                Unit::Table(t) => sync_table(doc, t, nodes)?,
                Unit::Section(s) => {
                    if let (Some((_, sec)), Some(m)) = (doc.section(s), nodes.get(s).cloned()) {
                        write_section_shallow(&m, sec, nodes)?;
                    }
                }
                Unit::Story(id) => {
                    if let (Some(s), Some(m)) = (story_ref(doc, id), nodes.get(id).cloned()) {
                        write_story_shallow(&m, &s, nodes)?;
                    }
                }
                Unit::Para(p) => {
                    if let (Some(para), Some(m)) = (doc.paragraph(p), nodes.get(p).cloned()) {
                        write_paragraph(&m, para, nodes)?;
                    }
                }
                Unit::Parts => write_parts(&ldoc, doc)?,
                _ => {}
            }
        }
        Ok(())
    }

    // -------------------------------------------------------------- shared → local

    /// Merges an update from another replica into the CRDT and the model.
    ///
    /// # Errors
    ///
    /// Bytes Loro cannot import, or a merged state that is not a valid document (the model is
    /// then left as it was).
    pub fn apply_remote(&mut self, doc: &mut Document, update: &[u8]) -> Result<WriteChange> {
        self.break_undo_group();
        let before = self.crdt.doc().oplog_vv();
        self.crdt.apply(update)?;
        let mut changes = self.crdt.take_changes();
        if self.moves.repair(self.crdt.doc(), &before, &changes)? {
            changes.extend(self.crdt.commit_repair());
        }
        if self.moves.take_reread() {
            return self.reload(doc);
        }
        self.merge(doc, &changes)
    }

    /// Undoes the local peer's latest change (other people's edits stay).
    ///
    /// # Errors
    ///
    /// The CRDT failed to undo, or the result is not a valid document.
    pub fn undo(&mut self, doc: &mut Document) -> Result<Option<WriteChange>> {
        self.break_undo_group();
        if !self.crdt.undo()? {
            return Ok(None);
        }
        let changes = self.crdt.take_changes();
        let mut change = self.merge(doc, &changes)?;
        change.tx.selection_after = self.crdt.take_undo_selection().and_then(parse_selection);
        Ok(Some(change))
    }

    /// Redoes the latest undone local change.
    ///
    /// # Errors
    ///
    /// The CRDT failed to redo, or the result is not a valid document.
    pub fn redo(&mut self, doc: &mut Document) -> Result<Option<WriteChange>> {
        self.break_undo_group();
        if !self.crdt.redo()? {
            return Ok(None);
        }
        let changes = self.crdt.take_changes();
        let mut change = self.merge(doc, &changes)?;
        change.tx.selection_after = self.crdt.take_undo_selection().and_then(parse_selection);
        Ok(Some(change))
    }

    /// Replaces the shared document with a model (Version History's restore): one change on
    /// top of the history, which stays.
    ///
    /// # Errors
    ///
    /// A model value that cannot be stored.
    pub fn replace(&mut self, doc: &Document) -> Result<()> {
        self.break_undo_group();
        self.write_all(doc)?;
        self.crdt.commit_local();
        Ok(())
    }

    fn merge(&mut self, doc: &mut Document, changes: &[Changed]) -> Result<WriteChange> {
        let units = remote_units(self.crdt.doc(), changes, &self.nodes);
        if units.is_empty() {
            return Ok(WriteChange::default());
        }
        let mut ops = Vec::new();
        let mut repair = Vec::new();
        let ok = !units.contains(&Unit::Reload)
            && self
                .reconcile(doc, &units, changes, &mut ops, &mut repair)
                .is_ok()
            && !dangling(doc, &ops);
        if ok {
            if !repair.is_empty() {
                // Lists a merge emptied keep the model's last entry; the CRDT gets it back.
                self.sync(doc, &repair)?;
                self.crdt.commit_system();
            }
            return Ok(WriteChange {
                tx: Transaction::new(ops),
                reloaded: false,
            });
        }
        self.reload(doc)
    }

    /// Replaces the model with the shared state, read afresh.
    fn reload(&mut self, doc: &mut Document) -> Result<WriteChange> {
        let lanes = doc.id_lanes();
        let mut fresh = self.rebuild()?;
        fresh.set_id_lanes(lanes);
        *doc = fresh;
        Ok(WriteChange {
            tx: Transaction::default(),
            reloaded: true,
        })
    }

    /// The model from the shared state, repairing what a merge left invalid (lists emptied by
    /// concurrent deletes) in both.
    fn rebuild(&mut self) -> Result<Document> {
        self.nodes = Nodes::default();
        let mut doc = read_document(self.crdt.doc(), &mut self.nodes)?;
        if repair(&mut doc) {
            self.write_all(&doc)?;
            self.crdt.commit_system();
        }
        doc.validate()?;
        Ok(doc)
    }

    fn reconcile(
        &mut self,
        doc: &mut Document,
        units: &[Unit],
        changes: &[Changed],
        ops: &mut Vec<Op>,
        repair: &mut Vec<Unit>,
    ) -> Result<()> {
        self.read.clear();
        let ldoc = self.crdt.doc().clone();
        let mut apply = |doc: &mut Document, op: Op| -> Result<()> {
            doc.apply(Transaction::new(vec![op.clone()]))?;
            ops.push(op);
            Ok(())
        };
        if units.contains(&Unit::Sections) {
            let want = list_ids(&ldoc.get_movable_list(SECTIONS));
            let have: Vec<NodeId> = doc.sections.iter().map(|s| s.id).collect();
            if want != have {
                return Err(corrupt("sections changed"));
            }
        }
        let stories = units.contains(&Unit::Stories);
        if stories {
            for op in story_inserts(&ldoc, doc, &mut self.nodes)? {
                apply(doc, op)?;
            }
        }
        // Block lists: removals (keeping one entry per list), inserts, then the last removals.
        let mut edits = Vec::new();
        for u in units {
            if let Unit::Blocks(c) = *u
                && let (Some(list), Ok(have)) = (block_list(&self.nodes, c), doc.container(c))
            {
                let have: Vec<NodeId> = have.iter().map(Block::id).collect();
                let want = list_ids(&list);
                if have != want {
                    edits.push((c, list, list_edits(&have, &want)));
                }
            }
        }
        let mut last = Vec::new();
        for (c, _, e) in &edits {
            let (now, held) = e.split_removals();
            for &i in now {
                apply(
                    doc,
                    Op::RemoveBlocks {
                        container: *c,
                        index: i,
                        count: 1,
                    },
                )?;
            }
            if held {
                last.push(*c);
            }
        }
        for (c, list, e) in &edits {
            for &(index, id) in &e.inserts {
                let Some(m) = maps_of(list)
                    .into_iter()
                    .find(|m| get_id(m, ID).ok() == Some(id))
                else {
                    continue;
                };
                let block = read_block(&m, &mut self.nodes)?;
                paragraphs_of(std::slice::from_ref(&block), &mut self.read);
                apply(
                    doc,
                    Op::InsertBlocks {
                        container: *c,
                        index,
                        blocks: vec![block],
                    },
                )?;
            }
        }
        // The entry kept back is last once the inserts are in.
        for c in last {
            let len = doc.container(c).map_or(0, Vec::len);
            if len > 1 {
                apply(
                    doc,
                    Op::RemoveBlocks {
                        container: c,
                        index: len - 1,
                        count: 1,
                    },
                )?;
            } else {
                // Emptied by deletions at the same moment: one empty paragraph, the same
                // whoever repairs it.
                let id = empty_paragraph_id(c);
                if doc
                    .container(c)
                    .is_ok_and(|b| b.first().map(Block::id) != Some(id))
                    && doc.paragraph(id).is_none()
                {
                    apply(
                        doc,
                        Op::InsertBlocks {
                            container: c,
                            index: 0,
                            blocks: vec![Block::Paragraph(Paragraph::new(id))],
                        },
                    )?;
                    apply(
                        doc,
                        Op::RemoveBlocks {
                            container: c,
                            index: 1,
                            count: 1,
                        },
                    )?;
                }
                repair.push(Unit::Blocks(c));
            }
        }
        for u in units {
            match *u {
                Unit::Table(t) => self.reconcile_table(doc, t, &mut apply, repair)?,
                Unit::Row(r) => self.reconcile_row(doc, r, &mut apply, repair)?,
                Unit::Cell(c) => {
                    let (Some(cell), Some(m)) = (doc.cell(c), self.nodes.get(c)) else {
                        continue;
                    };
                    let want = read_cell(m, &mut Nodes::default())?;
                    if (want.props.clone(), want.grid_span, want.v_merge)
                        != (cell.props.clone(), cell.grid_span, cell.v_merge)
                    {
                        apply(
                            doc,
                            Op::SetCell {
                                cell: c,
                                props: Box::new(want.props),
                                grid_span: want.grid_span,
                                v_merge: want.v_merge,
                            },
                        )?;
                    }
                }
                _ => {}
            }
        }
        for u in units {
            match *u {
                Unit::Para(p) => {
                    if let (Some(para), Some(m)) = (doc.paragraph(p), self.nodes.get(p).cloned()) {
                        let want = read_paragraph(&m, &mut self.nodes)?;
                        for op in paragraph_ops(para, &want)? {
                            apply(doc, op)?;
                        }
                    }
                }
                Unit::Text(p) if !units.contains(&Unit::Para(p)) && !self.read.contains(&p) => {
                    let Some(t) = self
                        .nodes
                        .get(p)
                        .and_then(|m| crate::value::find_text(m, "text"))
                    else {
                        continue;
                    };
                    // The events say what changed: applied as ops without reading the text.
                    let id = t.id();
                    let deltas: Vec<&Vec<loro::TextDelta>> = changes
                        .iter()
                        .filter(|c| c.target == id)
                        .filter_map(|c| c.text.as_ref())
                        .collect();
                    let fast = !deltas.is_empty()
                        && deltas
                            .iter()
                            .try_for_each(|d| text::apply_delta(doc, p, d, &mut apply))
                            .is_ok();
                    if !fast && let Some(para) = doc.paragraph(p) {
                        let want = text::decode_text(&t)?;
                        for op in text::content_ops(p, &para.content, &want)? {
                            apply(doc, op)?;
                        }
                    }
                }
                _ => {}
            }
        }
        for u in units {
            match *u {
                Unit::Section(s) => {
                    let (Some((_, sec)), Some(m)) = (doc.section(s), self.nodes.get(s)) else {
                        continue;
                    };
                    let props = get_fields(
                        &crate::value::find_map(m, "props")
                            .ok_or_else(|| corrupt("a section without properties"))?,
                    )?;
                    let headers = get_json_or_default(m, "headers")?;
                    let footers = get_json_or_default(m, "footers")?;
                    let refs_differ = sec.headers != headers || sec.footers != footers;
                    if sec.props != props {
                        apply(
                            doc,
                            Op::SetSectionProps {
                                section: s,
                                props: Box::new(props),
                            },
                        )?;
                    }
                    if refs_differ {
                        apply(
                            doc,
                            Op::SetSectionRefs {
                                section: s,
                                headers,
                                footers,
                            },
                        )?;
                    }
                }
                Unit::Story(id) => {
                    let Some(m) = self.nodes.get(id) else {
                        continue;
                    };
                    match kind_of(m).as_deref() {
                        Some(kind::COMMENT) => {
                            let meta = get_child_fields(m, "meta")?;
                            if doc.comments.iter().any(|c| c.id == id && c.meta != meta) {
                                apply(doc, Op::SetCommentMeta { id, meta })?;
                            }
                        }
                        _ => {
                            let changed = match story_ref(doc, id) {
                                Some(StoryRef::HeaderFooter(h)) => {
                                    read_header_footer(m, &mut Nodes::default())?.part != h.part
                                }
                                Some(StoryRef::Note(n)) => {
                                    let r = read_note(m, &mut Nodes::default())?;
                                    (r.kind, r.note_type) != (n.kind, n.note_type)
                                }
                                _ => false,
                            };
                            if changed {
                                return Err(corrupt("a story changed kind"));
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        if stories {
            for op in story_removals(&ldoc, doc) {
                apply(doc, op)?;
            }
            let order = |l: &str| list_ids(&ldoc.get_movable_list(l));
            if order(HEADERS) != doc.headers_footers.iter().map(|h| h.id).collect::<Vec<_>>()
                || order(NOTES) != doc.notes.iter().map(|n| n.id).collect::<Vec<_>>()
                || order(COMMENTS) != doc.comments.iter().map(|c| c.id).collect::<Vec<_>>()
            {
                return Err(corrupt("stories moved"));
            }
        }
        if units.contains(&Unit::Parts) {
            for op in part_ops(doc, &read_parts(&ldoc)?) {
                apply(doc, op)?;
            }
        }
        Ok(())
    }

    fn reconcile_table(
        &mut self,
        doc: &mut Document,
        t: NodeId,
        apply: &mut impl FnMut(&mut Document, Op) -> Result<()>,
        repair: &mut Vec<Unit>,
    ) -> Result<()> {
        let (Some(table), Some(m)) = (doc.table(t), self.nodes.get(t).cloned()) else {
            return Ok(());
        };
        let props = get_child_fields(&m, "props")?;
        let grid: Vec<i32> = get_json_or_default(&m, "grid")?;
        let (props_differ, grid_differ) = (table.props != props, table.grid != grid);
        let have: Vec<NodeId> = table.rows.iter().map(|r| r.id).collect();
        let list = find_list(&m, ROWS);
        let want = list.as_ref().map(list_ids).unwrap_or_default();
        if props_differ {
            apply(
                doc,
                Op::SetTableProps {
                    table: t,
                    props: Box::new(props),
                },
            )?;
        }
        if grid_differ {
            apply(doc, Op::SetTableGrid { table: t, grid })?;
        }
        if have == want {
            return Ok(());
        }
        let e = list_edits(&have, &want);
        let (now, held) = e.split_removals();
        for &i in now {
            apply(
                doc,
                Op::RemoveRows {
                    table: t,
                    index: i,
                    count: 1,
                },
            )?;
        }
        let rows = list.as_ref().map(maps_of).unwrap_or_default();
        for &(index, id) in &e.inserts {
            if let Some(rm) = rows.iter().find(|r| get_id(r, ID).ok() == Some(id)) {
                let row = read_row(rm, &mut self.nodes)?;
                for c in &row.cells {
                    paragraphs_of(&c.blocks, &mut self.read);
                }
                apply(
                    doc,
                    Op::InsertRows {
                        table: t,
                        index,
                        rows: vec![row],
                    },
                )?;
            }
        }
        if held {
            let len = doc.table(t).map_or(0, |t| t.rows.len());
            if len > 1 {
                apply(
                    doc,
                    Op::RemoveRows {
                        table: t,
                        index: len - 1,
                        count: 1,
                    },
                )?;
            } else {
                repair.push(Unit::Table(t));
            }
        }
        Ok(())
    }

    fn reconcile_row(
        &mut self,
        doc: &mut Document,
        r: NodeId,
        apply: &mut impl FnMut(&mut Document, Op) -> Result<()>,
        repair: &mut Vec<Unit>,
    ) -> Result<()> {
        let (Some((table, row)), Some(m)) = (doc.row(r), self.nodes.get(r).cloned()) else {
            return Ok(());
        };
        let table_id = table.id;
        let props = get_child_fields(&m, "props")?;
        let revision = get_json_or_default(&m, "rev")?;
        let differ = row.props != props || row.revision != revision;
        let have: Vec<NodeId> = row.cells.iter().map(|c| c.id).collect();
        let list = find_list(&m, CELLS);
        let want = list.as_ref().map(list_ids).unwrap_or_default();
        if differ {
            apply(
                doc,
                Op::SetRow {
                    row: r,
                    props: Box::new(props),
                    revision,
                },
            )?;
        }
        if have == want {
            return Ok(());
        }
        let e = list_edits(&have, &want);
        let (now, held) = e.split_removals();
        for &i in now {
            apply(
                doc,
                Op::RemoveCells {
                    row: r,
                    index: i,
                    count: 1,
                },
            )?;
        }
        let cells = list.as_ref().map(maps_of).unwrap_or_default();
        for &(index, id) in &e.inserts {
            if let Some(cm) = cells.iter().find(|c| get_id(c, ID).ok() == Some(id)) {
                let cell = read_cell(cm, &mut self.nodes)?;
                paragraphs_of(&cell.blocks, &mut self.read);
                apply(
                    doc,
                    Op::InsertCells {
                        row: r,
                        index,
                        cells: vec![cell],
                    },
                )?;
            }
        }
        if held {
            let len = doc.row(r).map_or(0, |(_, row)| row.cells.len());
            if len > 1 {
                apply(
                    doc,
                    Op::RemoveCells {
                        row: r,
                        index: len - 1,
                        count: 1,
                    },
                )?;
            } else {
                repair.push(Unit::Table(table_id));
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ helpers

/// The id of the empty paragraph a block list emptied by concurrent deletions gets: derived
/// from the list's owner, so every replica that repairs the list makes the same paragraph. It
/// is in lane 0 above 2^51, which no replica allocates from (`crate::room::LANES`).
pub(crate) fn empty_paragraph_id(owner: NodeId) -> NodeId {
    let hash = crate::value::fnv64(&owner.0.to_le_bytes()) & ((1 << 31) - 1);
    NodeId((1 << 51) + hash * u64::from(crate::room::LANES))
}

/// Whether a paragraph is the one an emptied list gets.
pub(crate) fn is_empty_paragraph_id(id: NodeId) -> bool {
    id.0 >= 1 << 51 && id.0.is_multiple_of(u64::from(crate::room::LANES))
}

/// Adds the ids of the paragraphs in `blocks`, at any depth, to `out`.
fn paragraphs_of(blocks: &[Block], out: &mut HashSet<NodeId>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                out.insert(p.id);
            }
            Block::Table(t) => {
                for c in t.rows.iter().flat_map(|r| &r.cells) {
                    paragraphs_of(&c.blocks, out);
                }
            }
            _ => {
                if let Some(c) = b.children() {
                    paragraphs_of(c, out);
                }
            }
        }
    }
}

fn selection_json(r: Option<Range>) -> Option<String> {
    r.and_then(|r| serde_json::to_string(&r).ok())
}

fn parse_selection(s: String) -> Option<Range> {
    serde_json::from_str(&s).ok()
}

/// Whether typing ended a word (the next keystroke starts a new undo step, as in Word).
fn ends_word(tx: &Transaction) -> bool {
    tx.ops.iter().any(|op| match op {
        Op::InsertInline { fragment, .. } => fragment.text.ends_with(char::is_whitespace),
        Op::SplitParagraph { .. } => true,
        _ => false,
    })
}

fn block_list(nodes: &Nodes, container: NodeId) -> Option<LoroMovableList> {
    nodes.get(container).and_then(|m| find_list(m, BLOCKS))
}

fn story_ref(doc: &Document, id: NodeId) -> Option<StoryRef<'_>> {
    if let Some(h) = doc.headers_footers.iter().find(|h| h.id == id) {
        return Some(StoryRef::HeaderFooter(h));
    }
    if let Some(n) = doc.notes.iter().find(|n| n.id == id) {
        return Some(StoryRef::Note(n));
    }
    doc.comments
        .iter()
        .find(|c| c.id == id)
        .map(StoryRef::Comment)
}

fn write_stories(ldoc: &LoroDoc, doc: &Document, nodes: &mut Nodes, deep: bool) -> Result<()> {
    let lists: [(&str, Vec<StoryRef<'_>>); 3] = [
        (
            HEADERS,
            doc.headers_footers
                .iter()
                .map(StoryRef::HeaderFooter)
                .collect(),
        ),
        (NOTES, doc.notes.iter().map(StoryRef::Note).collect()),
        (
            COMMENTS,
            doc.comments.iter().map(StoryRef::Comment).collect(),
        ),
    ];
    for (name, stories) in &lists {
        let list = ldoc.get_movable_list(*name);
        if deep {
            rewrite(&list, stories, story_id, write_story, nodes)?;
        } else {
            place(&list, stories, story_id, write_story, nodes)?;
        }
    }
    Ok(())
}

fn remove_stories(ldoc: &LoroDoc, doc: &Document, nodes: &mut Nodes) -> Result<()> {
    let keep = |ids: Vec<NodeId>| ids.into_iter().collect::<HashSet<_>>();
    remove_missing(
        &ldoc.get_movable_list(HEADERS),
        &keep(doc.headers_footers.iter().map(|h| h.id).collect()),
        nodes,
    )?;
    remove_missing(
        &ldoc.get_movable_list(NOTES),
        &keep(doc.notes.iter().map(|n| n.id).collect()),
        nodes,
    )?;
    remove_missing(
        &ldoc.get_movable_list(COMMENTS),
        &keep(doc.comments.iter().map(|c| c.id).collect()),
        nodes,
    )
}

fn story_id(s: &StoryRef<'_>) -> NodeId {
    match s {
        StoryRef::HeaderFooter(h) => h.id,
        StoryRef::Note(n) => n.id,
        StoryRef::Comment(c) => c.id,
    }
}

/// Inserts of the stories the CRDT has and the model lacks, each after its predecessor.
fn story_inserts(ldoc: &LoroDoc, doc: &Document, nodes: &mut Nodes) -> Result<Vec<Op>> {
    let mut ops = Vec::new();
    let place_after = |have: &[NodeId], want: &[NodeId], i: usize| -> usize {
        want[..i]
            .iter()
            .rev()
            .find_map(|p| have.iter().position(|h| h == p).map(|k| k + 1))
            .unwrap_or(0)
    };
    for (name, have) in [
        (
            HEADERS,
            doc.headers_footers.iter().map(|h| h.id).collect::<Vec<_>>(),
        ),
        (NOTES, doc.notes.iter().map(|n| n.id).collect()),
        (COMMENTS, doc.comments.iter().map(|c| c.id).collect()),
    ] {
        let maps = maps_of(&ldoc.get_movable_list(name));
        let want: Vec<NodeId> = maps.iter().filter_map(|m| get_id(m, ID).ok()).collect();
        let mut have = have;
        for (i, id) in want.iter().enumerate() {
            if have.contains(id) {
                continue;
            }
            let index = place_after(&have, &want, i);
            let m = &maps[i];
            ops.push(match name {
                HEADERS => Op::InsertHeaderFooter {
                    index,
                    story: Box::new(read_header_footer(m, nodes)?),
                },
                NOTES => Op::InsertNote {
                    index,
                    note: Box::new(read_note(m, nodes)?),
                },
                _ => Op::InsertComment {
                    index,
                    comment: Box::new(read_comment(m, nodes)?),
                },
            });
            have.insert(index, *id);
        }
    }
    Ok(ops)
}

/// Removals of the stories the model has and the CRDT no longer does (replies first).
fn story_removals(ldoc: &LoroDoc, doc: &Document) -> Vec<Op> {
    let ids = |name: &str| {
        list_ids(&ldoc.get_movable_list(name))
            .into_iter()
            .collect::<HashSet<_>>()
    };
    let (h, n, c) = (ids(HEADERS), ids(NOTES), ids(COMMENTS));
    let mut ops: Vec<Op> = Vec::new();
    ops.extend(
        doc.headers_footers
            .iter()
            .filter(|s| !h.contains(&s.id))
            .map(|s| Op::RemoveHeaderFooter { id: s.id }),
    );
    ops.extend(
        doc.notes
            .iter()
            .filter(|s| !n.contains(&s.id))
            .map(|s| Op::RemoveNote { id: s.id }),
    );
    ops.extend(
        doc.comments
            .iter()
            .rev()
            .filter(|s| !c.contains(&s.id))
            .map(|s| Op::RemoveComment { id: s.id }),
    );
    ops
}

/// The ops that make a paragraph equal to `want` (same id).
pub(crate) fn paragraph_ops(have: &Paragraph, want: &Paragraph) -> Result<Vec<Op>> {
    let para = have.id;
    let mut ops = Vec::new();
    if have.props != want.props {
        ops.push(Op::SetParagraphProps {
            para,
            props: Box::new(want.props.clone()),
        });
    }
    if have.mark_props != want.mark_props {
        ops.push(Op::SetParagraphMarkProps {
            para,
            props: Box::new(want.mark_props.clone()),
        });
    }
    if have.revision != want.revision {
        ops.push(Op::SetParagraphRevision {
            para,
            revision: Box::new(want.revision.clone()),
        });
    }
    ops.extend(text::content_ops(para, &have.content, &want.content)?);
    Ok(ops)
}

fn sync_table(doc: &Document, t: NodeId, nodes: &mut Nodes) -> Result<()> {
    let (Some(table), Some(m)) = (doc.table(t), nodes.get(t).cloned()) else {
        return Ok(());
    };
    write_table_shallow(&m, table, nodes)?;
    let rows = crate::value::child_list(&m, ROWS)?;
    remove_missing(&rows, &table.rows.iter().map(|r| r.id).collect(), nodes)?;
    place(&rows, &table.rows, |r| r.id, write_block_row, nodes)?;
    for row in &table.rows {
        let Some(rm) = nodes.get(row.id).cloned() else {
            continue;
        };
        write_row_shallow(&rm, row, nodes)?;
        let cells = crate::value::child_list(&rm, CELLS)?;
        remove_missing(&cells, &row.cells.iter().map(|c| c.id).collect(), nodes)?;
        place(&cells, &row.cells, |c| c.id, write_block_cell, nodes)?;
        for cell in &row.cells {
            if let Some(cm) = nodes.get(cell.id).cloned() {
                write_cell_shallow(&cm, cell, nodes)?;
            }
        }
    }
    Ok(())
}

fn write_block_row(map: &LoroMap, r: &kiln_doc::Row, nodes: &mut Nodes) -> Result<()> {
    write_row_shallow(map, r, nodes)?;
    let cells = crate::value::child_list(map, CELLS)?;
    place(&cells, &r.cells, |c| c.id, write_block_cell, nodes).map(|_| ())
}

fn write_block_cell(map: &LoroMap, c: &kiln_doc::Cell, nodes: &mut Nodes) -> Result<()> {
    write_cell_shallow(map, c, nodes)?;
    blocks::write_blocks(map, &c.blocks, nodes)
}

/// The edits that turn the id list `have` into `want`: removals (indices into `have`, from
/// the back) and inserts (index in the result, id) in order. Entries kept are the longest run
/// of `have` already in `want`'s order; the others move by removal and insertion.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ListEdits {
    pub removals: Vec<usize>,
    pub inserts: Vec<(usize, NodeId)>,
    /// How many entries survive the removals.
    pub kept: usize,
}

impl ListEdits {
    /// The removals to do before the inserts, and whether one is held back until after them
    /// (when doing them all first would empty the list: a list keeps at least one entry; the
    /// entry held back is then the last one).
    pub fn split_removals(&self) -> (&[usize], bool) {
        let hold = self.kept == 0 && !self.removals.is_empty();
        (&self.removals[usize::from(hold)..], hold)
    }
}

pub(crate) fn list_edits(have: &[NodeId], want: &[NodeId]) -> ListEdits {
    let pos: std::collections::HashMap<NodeId, usize> =
        want.iter().enumerate().map(|(i, id)| (*id, i)).collect();
    // Longest increasing subsequence of want-positions over have (patience sorting).
    let mut tails: Vec<usize> = Vec::new();
    let mut prev: Vec<Option<usize>> = vec![None; have.len()];
    let mut tail_idx: Vec<usize> = Vec::new();
    let mut seen = HashSet::new();
    for (i, id) in have.iter().enumerate() {
        let Some(&p) = pos.get(id) else { continue };
        if !seen.insert(*id) {
            continue;
        }
        let k = tails.partition_point(|&t| t < p);
        if k == tails.len() {
            tails.push(p);
            tail_idx.push(i);
        } else {
            tails[k] = p;
            tail_idx[k] = i;
        }
        prev[i] = if k > 0 { Some(tail_idx[k - 1]) } else { None };
    }
    let mut keep = vec![false; have.len()];
    let mut at = tail_idx.last().copied();
    while let Some(i) = at {
        keep[i] = true;
        at = prev[i];
    }
    let removals: Vec<usize> = (0..have.len()).rev().filter(|&i| !keep[i]).collect();
    let kept: Vec<NodeId> = have
        .iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(id, _)| *id)
        .collect();
    let mut inserts = Vec::new();
    let mut k = 0;
    for (i, id) in want.iter().enumerate() {
        if kept.get(k) == Some(id) {
            k += 1;
        } else {
            inserts.push((i, *id));
        }
    }
    ListEdits {
        removals,
        inserts,
        kept: kept.len(),
    }
}

/// What a local transaction touched.
fn local_units(doc: &Document, tx: &Transaction, nodes: &Nodes) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    let mut push = |u: Unit| {
        if !out.contains(&u) {
            out.push(u);
        }
    };
    let parent = |id: NodeId| doc.parent_of(id).map(|(c, _)| c);
    for op in &tx.ops {
        let units: Vec<Unit> = match op {
            Op::InsertInline { para, .. }
            | Op::DeleteInline { para, .. }
            | Op::SetRuns { para, .. }
            | Op::AddMark { para, .. }
            | Op::RemoveMark { para, .. }
            | Op::SetMarks { para, .. }
            | Op::SetObject { para, .. }
            | Op::SetParagraphProps { para, .. }
            | Op::SetParagraphMarkProps { para, .. }
            | Op::SetParagraphRevision { para, .. } => vec![Unit::Para(*para)],
            Op::SplitParagraph { para, .. } => {
                let mut v = vec![Unit::Para(*para)];
                v.extend(parent(*para).map(Unit::Blocks));
                v
            }
            Op::MergeParagraphs { first, .. } => {
                let mut v = vec![Unit::Para(*first)];
                v.extend(parent(*first).map(Unit::Blocks));
                v
            }
            Op::InsertBlocks { container, .. } | Op::RemoveBlocks { container, .. } => {
                vec![Unit::Blocks(*container)]
            }
            Op::SetTableProps { table, .. }
            | Op::SetTableGrid { table, .. }
            | Op::InsertRows { table, .. }
            | Op::RemoveRows { table, .. } => vec![Unit::Table(*table)],
            Op::SetRow { row, .. } | Op::InsertCells { row, .. } | Op::RemoveCells { row, .. } => {
                doc.row(*row)
                    .map(|(t, _)| Unit::Table(t.id))
                    .into_iter()
                    .collect()
            }
            Op::SetCell { cell, .. } => table_of_cell(doc, *cell)
                .map(Unit::Table)
                .into_iter()
                .collect(),
            Op::SetSectionProps { section, .. } | Op::SetSectionRefs { section, .. } => {
                vec![Unit::Section(*section)]
            }
            Op::SplitSection { section, .. } => vec![Unit::Sections, Unit::Blocks(*section)],
            Op::MergeSections { first, .. } => vec![Unit::Sections, Unit::Blocks(*first)],
            Op::InsertHeaderFooter { .. }
            | Op::RemoveHeaderFooter { .. }
            | Op::InsertNote { .. }
            | Op::RemoveNote { .. }
            | Op::InsertComment { .. }
            | Op::RemoveComment { .. } => vec![Unit::Stories],
            Op::SetCommentMeta { id, .. } => vec![Unit::Story(*id)],
            Op::PutStyle { .. }
            | Op::DeleteStyle { .. }
            | Op::SetDocDefaults { .. }
            | Op::SetAbstractNum { .. }
            | Op::SetNumInstance { .. }
            | Op::SetSettings { .. }
            | Op::SetProperties { .. }
            | Op::SetTheme { .. }
            | Op::PutMedia { .. } => vec![Unit::Parts],
        };
        for u in units {
            push(text_box_host(doc, nodes, u).unwrap_or(u));
        }
    }
    out
}

/// The table a cell is in.
fn table_of_cell(doc: &Document, cell: NodeId) -> Option<NodeId> {
    fn find(blocks: &[Block], cell: NodeId) -> Option<NodeId> {
        for b in blocks {
            match b {
                Block::Table(t) => {
                    for r in &t.rows {
                        for c in &r.cells {
                            if c.id == cell {
                                return Some(t.id);
                            }
                            if let Some(found) = find(&c.blocks, cell) {
                                return Some(found);
                            }
                        }
                    }
                }
                Block::Sdt(_) | Block::Field(_) => {
                    if let Some(found) = b.children().and_then(|c| find(c, cell)) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    let story = doc.story_of(cell)?;
    find(doc.story_blocks(story)?, cell)
}

/// A unit inside a text box (which the CRDT holds inside its paragraph's object) becomes the
/// paragraph holding the drawing.
fn text_box_host(doc: &Document, nodes: &Nodes, u: Unit) -> Option<Unit> {
    let id = match u {
        Unit::Para(i) | Unit::Blocks(i) | Unit::Table(i) => i,
        _ => return None,
    };
    if nodes.get(id).is_some() || !doc.in_text_box(id) {
        return None;
    }
    fn holds(blocks: &[Block], id: NodeId) -> bool {
        blocks.iter().any(|b| {
            b.id() == id
                || match b {
                    Block::Paragraph(p) => p.content.objects().iter().any(|o| match o {
                        kiln_doc::InlineObject::Drawing(d) => {
                            d.id == id || d.text_box.as_deref().is_some_and(|tb| holds(tb, id))
                        }
                        _ => false,
                    }),
                    Block::Table(t) => t
                        .rows
                        .iter()
                        .flat_map(|r| &r.cells)
                        .any(|c| c.id == id || holds(&c.blocks, id)),
                    _ => b.children().is_some_and(|c| holds(c, id)),
                }
        })
    }
    let story = doc.story_of(id)?;
    doc.paragraphs_in(story)
        .into_iter()
        .filter(|p| nodes.get(p.id).is_some())
        .find(|p| {
            p.content.objects().iter().any(|o| match o {
                kiln_doc::InlineObject::Drawing(d) => {
                    d.id == id || d.text_box.as_deref().is_some_and(|tb| holds(tb, id))
                }
                _ => false,
            })
        })
        .map(|p| Unit::Para(p.id))
}

/// What a merge changed, from Loro's events. A change to a node the model knows under another
/// map (two copies of one paragraph met) is read whole.
fn remote_units(ldoc: &LoroDoc, changes: &[Changed], nodes: &Nodes) -> Vec<Unit> {
    let mut out: Vec<Unit> = Vec::new();
    for c in changes {
        let mut path = c.path.clone();
        if path.last().is_none_or(|(cid, _)| *cid != c.target) {
            path.push((c.target.clone(), loro::Index::Seq(0)));
        }
        let unit = match owner_of(ldoc, &path) {
            Some((m, via)) => {
                let Ok(id) = get_id(&m, ID) else { continue };
                if nodes.get(id).is_some_and(|known| known.id() != m.id()) {
                    Unit::Reload
                } else {
                    let blocks = via.as_deref() == Some(BLOCKS);
                    match kind_of(&m).as_deref() {
                        Some(kind::PARAGRAPH) if via.as_deref() == Some("text") => Unit::Text(id),
                        Some(kind::PARAGRAPH) => Unit::Para(id),
                        Some(kind::TABLE) => Unit::Table(id),
                        Some(kind::ROW) => Unit::Row(id),
                        Some(kind::CELL) if blocks => Unit::Blocks(id),
                        Some(kind::CELL) => Unit::Cell(id),
                        Some(kind::SECTION) if blocks => Unit::Blocks(id),
                        Some(kind::SECTION) => Unit::Section(id),
                        Some(kind::HEADER_FOOTER | kind::NOTE | kind::COMMENT) if blocks => {
                            Unit::Blocks(id)
                        }
                        Some(kind::HEADER_FOOTER | kind::NOTE | kind::COMMENT) => Unit::Story(id),
                        Some(kind::SDT | kind::FIELD) if blocks => Unit::Blocks(id),
                        _ => Unit::Reload,
                    }
                }
            }
            None => match root_of(ldoc, c) {
                ContainerID::Root { name, .. } => match name.as_str() {
                    SECTIONS => Unit::Sections,
                    HEADERS | NOTES | COMMENTS => Unit::Stories,
                    crate::history::VERSIONS | crate::moves::MOVES => continue,
                    _ => Unit::Parts,
                },
                _ => Unit::Parts,
            },
        };
        if !out.contains(&unit) {
            out.push(unit);
        }
    }
    out
}

/// The root container a change is under.
pub(crate) fn root_of(ldoc: &LoroDoc, c: &Changed) -> ContainerID {
    c.path
        .first()
        .map(|(cid, _)| cid.clone())
        .or_else(|| {
            ldoc.get_path_to_container(&c.target)
                .and_then(|p| p.first().map(|(cid, _)| cid.clone()))
        })
        .unwrap_or_else(|| c.target.clone())
}

/// Reads the whole document.
fn read_document(ldoc: &LoroDoc, nodes: &mut Nodes) -> Result<Document> {
    let mut doc = Document::empty();
    let mut seen = HashSet::new();
    for m in maps_of(&ldoc.get_movable_list(SECTIONS)) {
        if get_id(&m, ID).is_ok_and(|id| seen.insert(id)) {
            doc.sections.push(read_section(&m, nodes)?);
        }
    }
    if doc.sections.is_empty() {
        return Err(corrupt("no sections"));
    }
    let mut seen = HashSet::new();
    let mut fresh = |m: &LoroMap| get_id(m, ID).is_ok_and(|id| seen.insert(id));
    for m in maps_of(&ldoc.get_movable_list(HEADERS)) {
        if fresh(&m) {
            doc.headers_footers.push(read_header_footer(&m, nodes)?);
        }
    }
    for m in maps_of(&ldoc.get_movable_list(NOTES)) {
        if fresh(&m) {
            doc.notes.push(read_note(&m, nodes)?);
        }
    }
    for m in maps_of(&ldoc.get_movable_list(COMMENTS)) {
        if fresh(&m) {
            doc.comments.push(read_comment(&m, nodes)?);
        }
    }
    install_parts(&mut doc, read_parts(ldoc)?);
    doc.refresh_id_counter();
    Ok(doc)
}

/// The stories that references may point to.
struct Targets {
    comments: HashSet<NodeId>,
    notes: HashSet<NodeId>,
    headers_footers: HashSet<NodeId>,
}

impl Targets {
    fn of(doc: &Document) -> Self {
        Self {
            comments: doc.comments.iter().map(|c| c.id).collect(),
            notes: doc.notes.iter().map(|n| n.id).collect(),
            headers_footers: doc.headers_footers.iter().map(|h| h.id).collect(),
        }
    }

    fn mark_ok(&self, m: &kiln_doc::Mark) -> bool {
        !matches!(m, kiln_doc::Mark::Comment(c) if !self.comments.contains(c))
    }

    fn object_ok(&self, o: &kiln_doc::InlineObject) -> bool {
        !matches!(o, kiln_doc::InlineObject::NoteRef(n) if !self.notes.contains(&n.note))
    }

    fn blocks_ok(&self, blocks: &[Block]) -> bool {
        blocks.iter().all(|b| match b {
            Block::Paragraph(p) => {
                p.content.marks().iter().all(|m| self.mark_ok(&m.mark))
                    && p.content.objects().iter().all(|o| self.object_ok(o))
            }
            Block::Table(t) => t
                .rows
                .iter()
                .flat_map(|r| &r.cells)
                .all(|c| self.blocks_ok(&c.blocks)),
            _ => b.children().is_none_or(|c| self.blocks_ok(c)),
        })
    }

    fn refs_ok(&self, r: &kiln_doc::section::HeaderFooterRefs) -> bool {
        [r.default, r.first, r.even]
            .iter()
            .flatten()
            .all(|id| self.headers_footers.contains(id))
    }
}

/// Whether the ops of a merge left a reference to a story that no longer exists (a comment
/// deleted on one side while the other marked new text with it). Only what the ops brought in
/// is checked, so typing costs a few lookups.
fn dangling(doc: &Document, ops: &[Op]) -> bool {
    let t = Targets::of(doc);
    ops.iter().any(|op| match op {
        Op::InsertInline { fragment, .. } => {
            !fragment.marks.iter().all(|m| t.mark_ok(&m.mark))
                || !fragment.objects.iter().all(|o| t.object_ok(o))
        }
        Op::AddMark { mark, .. } => !t.mark_ok(mark),
        Op::SetMarks { marks, .. } => !marks.iter().all(|m| t.mark_ok(&m.mark)),
        Op::SetObject { object, .. } => !t.object_ok(object),
        Op::InsertBlocks { blocks, .. } => !t.blocks_ok(blocks),
        Op::InsertRows { rows, .. } => !rows
            .iter()
            .flat_map(|r| &r.cells)
            .all(|c| t.blocks_ok(&c.blocks)),
        Op::InsertCells { cells, .. } => !cells.iter().all(|c| t.blocks_ok(&c.blocks)),
        Op::InsertHeaderFooter { story, .. } => !t.blocks_ok(&story.blocks),
        Op::InsertNote { note, .. } => !t.blocks_ok(&note.blocks),
        Op::InsertComment { comment, .. } => {
            !t.blocks_ok(&comment.blocks)
                || comment
                    .meta
                    .parent
                    .is_some_and(|p| !t.comments.contains(&p))
        }
        Op::SetCommentMeta { meta, .. } => meta.parent.is_some_and(|p| !t.comments.contains(&p)),
        Op::SetSectionRefs {
            headers, footers, ..
        } => !t.refs_ok(headers) || !t.refs_ok(footers),
        _ => false,
    })
}

/// Makes a merged document valid where concurrent edits left it invalid: a block list emptied
/// by deletes gets an empty paragraph, a table without rows or a row without cells goes, and
/// references to deleted stories go (comment ranges, note references, header and footer links,
/// replies). Returns whether anything changed.
fn repair(doc: &mut Document) -> bool {
    fn fix_paragraph(p: &mut Paragraph, t: &Targets, changed: &mut bool) {
        if !p.content.marks().iter().all(|m| t.mark_ok(&m.mark)) {
            let keep = p
                .content
                .marks()
                .iter()
                .filter(|m| t.mark_ok(&m.mark))
                .cloned()
                .collect();
            let _ = p.content.set_marks(keep);
            *changed = true;
        }
        let bad: Vec<usize> = p
            .content
            .objects_with_offsets()
            .filter(|(_, o)| !t.object_ok(o))
            .map(|(i, _)| i)
            .collect();
        for i in bad.into_iter().rev() {
            let _ = p.content.remove(i, i + 1);
            *changed = true;
        }
    }
    fn fix(
        blocks: &mut Vec<Block>,
        owner: NodeId,
        t: &Targets,
        seen: &mut HashSet<NodeId>,
        fresh: &mut impl FnMut() -> NodeId,
        changed: &mut bool,
    ) {
        blocks.retain_mut(|b| {
            // A block two people moved to different places at once is in both: the first stays.
            if !seen.insert(b.id()) {
                *changed = true;
                return false;
            }
            match b {
                Block::Paragraph(p) => {
                    fix_paragraph(p, t, changed);
                    true
                }
                Block::Table(tb) => {
                    tb.rows.retain_mut(|r| {
                        if !seen.insert(r.id) {
                            *changed = true;
                            return false;
                        }
                        r.cells.retain_mut(|c| {
                            if !seen.insert(c.id) {
                                *changed = true;
                                return false;
                            }
                            fix(&mut c.blocks, c.id, t, seen, fresh, changed);
                            true
                        });
                        !r.cells.is_empty()
                    });
                    let keep = !tb.rows.is_empty();
                    *changed |= !keep;
                    keep
                }
                Block::Sdt(s) => {
                    fix(&mut s.blocks, s.id, t, seen, fresh, changed);
                    true
                }
                Block::Field(f) => {
                    fix(&mut f.blocks, f.id, t, seen, fresh, changed);
                    true
                }
                Block::Opaque(_) => true,
            }
        });
        if blocks.is_empty() {
            // The same paragraph whoever repairs it, so that two repairs leave one.
            let mut id = empty_paragraph_id(owner);
            if !seen.insert(id) {
                id = fresh();
            }
            blocks.push(Block::Paragraph(Paragraph::new(id)));
            *changed = true;
        }
    }
    let t = Targets::of(doc);
    let mut seen: HashSet<NodeId> = HashSet::new();
    let lanes = doc.id_lanes();
    let mut next = doc.peek_next_id().0;
    let mut fresh = || {
        let id = lanes.first_from(next);
        next = id + 1;
        NodeId(id)
    };
    let mut changed = false;
    for s in &mut doc.sections {
        fix(&mut s.blocks, s.id, &t, &mut seen, &mut fresh, &mut changed);
        for r in [&mut s.headers, &mut s.footers] {
            for slot in [&mut r.default, &mut r.first, &mut r.even] {
                if slot.is_some_and(|id| !t.headers_footers.contains(&id)) {
                    *slot = None;
                    changed = true;
                }
            }
        }
    }
    for h in &mut doc.headers_footers {
        fix(&mut h.blocks, h.id, &t, &mut seen, &mut fresh, &mut changed);
    }
    for n in &mut doc.notes {
        fix(&mut n.blocks, n.id, &t, &mut seen, &mut fresh, &mut changed);
    }
    for c in &mut doc.comments {
        fix(&mut c.blocks, c.id, &t, &mut seen, &mut fresh, &mut changed);
        if c.meta.parent.is_some_and(|p| !t.comments.contains(&p)) {
            c.meta.parent = None;
            changed = true;
        }
    }
    doc.refresh_id_counter();
    changed
}

#[cfg(test)]
mod tests;
