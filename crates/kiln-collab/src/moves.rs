// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text that moves between paragraphs.
//!
//! Splitting a paragraph writes its tail into the new paragraph and deletes it from the first;
//! joining two writes the second's text into the first and removes the second. On their own,
//! edits someone else makes to the moved stretch at the same moment would stay where the move
//! took the text from: typing at the end of the first paragraph or in a paragraph that is gone,
//! deletions in characters that are already gone; and two people moving the same characters at
//! once (two splits of one paragraph, two joins of the same pair, a split and a join) would each
//! keep a copy.
//!
//! So each move is recorded in the root map `moves`, one entry per move and text it took
//! characters from: which characters it took, which characters they became, and an operation of
//! the change that made it (changes that depend on that operation saw the move). Every replica
//! that merges a record, or merges changes made at the same moment as one, puts things right as
//! a change of its own ([`Moves::repair`]):
//!
//! - a character two moves copied stays only in the copy of the move that took fewer characters
//!   (the inner of two splits; of two joins of one pair, the record that sorts first), a deletion
//!   of the other copy by someone who kept that one is made in the copy kept, and when the inner
//!   split's paragraph came first the two paragraphs swap their text;
//! - characters typed into the moved stretch at the same moment move after the moved characters,
//!   to the same place among them and with their formatting (text typed just before the split
//!   point stays in the first paragraph); each such move is recorded like any other, so two
//!   replicas moving the same characters at once leave one copy;
//! - characters deleted from the moved stretch at the same moment are deleted from the copy,
//!   wherever it went since;
//! - a paragraph a join took that a list move at the same moment put back goes again, and two
//!   joins in opposite directions that removed both paragraphs bring them back;
//! - the second copy of the empty paragraph two replicas gave a list emptied at the same moment
//!   joins the first.
//!
//! What a change did is read from its operations (Loro's JSON form of the merged changes), not
//! from the events, which leave out a deletion of characters already gone. Deleting is
//! idempotent and moves are recorded, so any number of replicas can repair at once and nothing
//! depends on the person who typed still being there. Paragraphs are never moved by a repair: in
//! a movable list a move brings back an element deleted at the same moment.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::rc::Rc;

use kiln_common::NodeId;
use kiln_doc::{Block, Op};
use loro::cursor::{Cursor, Side};
use loro::{
    ContainerID, ContainerTrait, ContainerType, ID, IdSpan, LoroDoc, LoroMap, LoroText,
    VersionVector,
};
use serde::{Deserialize, Serialize};

use crate::crdt::Changed;
use crate::error::Result;
use crate::value::{from_json, get_str, put_str, to_json};
use crate::write::text::{Attrs, Seg, format_diff, loro_segments};

/// The root map of the records.
pub(crate) const MOVES: &str = "moves";

/// How many moves a character is followed through.
const CHAIN: usize = 8;

/// The key a paragraph brought back after two joins removed it carries.
const REVIVED: &str = "revived";

/// Consecutive character ids of one peer: `(peer, first counter, count)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Run(u64, i32, u32);

/// What a move was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum Kind {
    /// The characters left a paragraph that stays (a split).
    Split,
    /// The paragraph they left is gone (a join).
    Join,
    /// A repair moved characters typed into a moved stretch after it.
    Repair,
}

/// Characters taken from `from` that became characters of `to`, in order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Record {
    from: String,
    to: String,
    /// The character of `to` just before them (for a joined paragraph that was empty).
    after: Option<Run>,
    src: Vec<Run>,
    copy: Vec<Run>,
    /// An operation of the change that made the move.
    at: (u64, i32),
    kind: Kind,
    /// For a join, the paragraph `to` is the text of and where it is in its list (a cursor), to
    /// bring it back if two joins at the same moment removed both paragraphs.
    #[serde(default)]
    host: Option<(String, Vec<u8>)>,
}

/// A record with its characters spelt out.
struct Move {
    key: String,
    record: Record,
    src: Vec<ID>,
    copy: Vec<ID>,
    /// Where each taken character is in `src`.
    index: HashMap<ID, usize>,
}

impl Move {
    fn read(map: &LoroMap, key: &str) -> Option<Self> {
        let record: Record = get_str(map, key).and_then(|s| from_json(&s).ok())?;
        let (src, copy) = (expand(&record.src), expand(&record.copy));
        let index = src.iter().enumerate().map(|(k, id)| (*id, k)).collect();
        (src.len() == copy.len()).then(|| Self {
            key: key.to_owned(),
            record,
            src,
            copy,
            index,
        })
    }

    /// Where a taken character is among the taken ones.
    fn find(&self, id: ID) -> Option<usize> {
        self.index.get(&id).copied()
    }

    /// Whether operation `by` is the move's own deletion of `id` (a move deletes what it takes).
    fn took(&self, by: ID, id: ID) -> bool {
        let at = self.at();
        by.peer == at.peer && by.counter >= at.counter && self.index.contains_key(&id)
    }

    fn at(&self) -> ID {
        ID::new(self.record.at.0, self.record.at.1)
    }

    /// Which of two moves keeps a character both copied: the one that took fewer.
    fn rank(&self) -> (usize, &str) {
        (self.src.len(), &self.key)
    }
}

fn runs(ids: &[ID]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for id in ids {
        match out.last_mut() {
            Some(Run(peer, first, n))
                if *peer == id.peer
                    && i64::from(*first) + i64::from(*n) == i64::from(id.counter) =>
            {
                *n += 1;
            }
            _ => out.push(Run(id.peer, id.counter, 1)),
        }
    }
    out
}

fn expand(runs: &[Run]) -> Vec<ID> {
    runs.iter()
        .flat_map(|&Run(peer, first, n)| {
            (0..n).filter_map(move |k| {
                i32::try_from(k)
                    .ok()
                    .and_then(|k| first.checked_add(k))
                    .map(|c| ID::new(peer, c))
            })
        })
        .collect()
}

fn id_at(text: &LoroText, i: usize) -> Option<ID> {
    text.get_cursor(i, Side::Middle).and_then(|c| c.id)
}

fn ids_of(text: &LoroText) -> Vec<ID> {
    (0..text.len_unicode())
        .filter_map(|i| id_at(text, i))
        .collect()
}

/// The id the next local operation gets: taken before a change is written, an operation of it.
pub(crate) fn next_op(ldoc: &LoroDoc) -> (u64, i32) {
    let peer = ldoc.peer_id();
    (peer, ldoc.oplog_vv().get(&peer).copied().unwrap_or(0))
}

fn key_of(r: &Record) -> String {
    format!("{} {} {:x}.{}", r.from, r.to, r.at.0, r.at.1)
}

fn from_of(key: &str) -> &str {
    key.split(' ').next().unwrap_or(key)
}

fn to_of(key: &str) -> &str {
    key.split(' ').nth(1).unwrap_or(key)
}

/// The operation of the change that made a move, from its key.
fn at_of(key: &str) -> Option<ID> {
    let (peer, counter) = key.rsplit(' ').next()?.split_once('.')?;
    Some(ID::new(
        u64::from_str_radix(peer, 16).ok()?,
        counter.parse().ok()?,
    ))
}

// ------------------------------------------------------------------ local edits

/// An entry of a paragraph as a transaction leaves it.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Origin {
    /// A character the transaction typed.
    Typed,
    /// A character that was there before: the text it was in (an index into
    /// [`Tracker::texts`]) and its id.
    Char(usize, ID),
    /// Where a joined paragraph's text begins (no character of its own: the paragraph may have
    /// been empty).
    Join(usize),
}

impl Origin {
    fn is_char(self) -> bool {
        !matches!(self, Self::Join(_))
    }
}

/// The index of the `k`-th character (the end for `k` past the last).
fn index_of(entries: &[Origin], k: usize) -> usize {
    entries
        .iter()
        .enumerate()
        .filter(|(_, o)| o.is_char())
        .nth(k)
        .map_or(entries.len(), |(i, _)| i)
}

/// Follows the characters of the paragraphs a transaction splits and joins, from the shared
/// state before it to the model after it.
#[derive(Default)]
pub(crate) struct Tracker {
    texts: Vec<LoroText>,
    paras: HashMap<NodeId, Vec<Origin>>,
    /// Paragraphs whose characters could not be followed.
    lost: HashSet<NodeId>,
}

impl Tracker {
    /// Whether a transaction moves text between paragraphs.
    pub(crate) fn needed(ops: &[Op]) -> bool {
        ops.iter()
            .any(|op| matches!(op, Op::SplitParagraph { .. } | Op::MergeParagraphs { .. }))
    }

    /// Follows `ops`; `text_of` gives a paragraph's text in the shared state before them.
    pub(crate) fn track(&mut self, ops: &[Op], text_of: impl Fn(NodeId) -> Option<LoroText>) {
        for op in ops {
            match op {
                Op::InsertInline {
                    para,
                    offset,
                    fragment,
                } => {
                    let entries = self.entries(*para, &text_of);
                    if *offset <= entries.iter().filter(|o| o.is_char()).count() {
                        let at = index_of(entries, *offset);
                        entries.splice(at..at, vec![Origin::Typed; fragment.len()]);
                    } else {
                        self.lost.insert(*para);
                    }
                }
                Op::DeleteInline { para, offset, len } => {
                    let entries = self.entries(*para, &text_of);
                    let mut k = 0;
                    let before = entries.len();
                    entries.retain(|o| {
                        if !o.is_char() {
                            return true;
                        }
                        k += 1;
                        !(*offset < k && k <= offset + len)
                    });
                    if before - entries.len() != *len {
                        self.lost.insert(*para);
                    }
                }
                Op::SplitParagraph {
                    para,
                    offset,
                    new_para,
                    ..
                } => {
                    let entries = self.entries(*para, &text_of);
                    let tail = entries.split_off(index_of(entries, *offset));
                    self.paras.insert(*new_para, tail);
                }
                Op::MergeParagraphs { first, second } => {
                    let source = self.source(*second, &text_of);
                    let mut moved = std::mem::take(self.entries(*second, &text_of));
                    if let Some(s) = source {
                        moved.insert(0, Origin::Join(s));
                    }
                    self.entries(*first, &text_of).extend(moved);
                }
                Op::InsertBlocks { blocks, .. } => {
                    for b in blocks {
                        if let Block::Paragraph(p) = b {
                            self.paras
                                .insert(p.id, vec![Origin::Typed; p.content.len()]);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// The index of a paragraph's text before the transaction, when it had one.
    fn source(
        &mut self,
        para: NodeId,
        text_of: &impl Fn(NodeId) -> Option<LoroText>,
    ) -> Option<usize> {
        let text = text_of(para)?;
        let id = text.id();
        if let Some(i) = self.texts.iter().position(|t| t.id() == id) {
            return Some(i);
        }
        self.texts.push(text);
        Some(self.texts.len() - 1)
    }

    fn entries(
        &mut self,
        para: NodeId,
        text_of: &impl Fn(NodeId) -> Option<LoroText>,
    ) -> &mut Vec<Origin> {
        if !self.paras.contains_key(&para) {
            let entries = match self.source(para, text_of) {
                Some(i) => ids_of(&self.texts[i])
                    .into_iter()
                    .map(|id| Origin::Char(i, id))
                    .collect(),
                None => Vec::new(),
            };
            self.paras.insert(para, entries);
        }
        self.paras.entry(para).or_default()
    }

    /// Records, for each paragraph that received characters from another text, where they
    /// came from; `at` is [`next_op`] before the change was written, `text_of` gives a
    /// paragraph's text in the shared state after the transaction (written but not yet
    /// committed).
    pub(crate) fn record(
        self,
        ldoc: &LoroDoc,
        at: (u64, i32),
        text_of: impl Fn(NodeId) -> Option<LoroText>,
    ) -> Result<()> {
        struct Taken {
            source: usize,
            after: Option<ID>,
            src: Vec<ID>,
            copy: Vec<ID>,
        }
        for (para, entries) in &self.paras {
            if self.lost.contains(para) {
                continue;
            }
            let Some(text) = text_of(*para) else { continue };
            if text.len_unicode() != entries.iter().filter(|o| o.is_char()).count() {
                continue;
            }
            let own = text.id();
            let mut taken: Vec<Taken> = Vec::new();
            let mut k: usize = 0;
            for origin in entries {
                let (source, id) = match *origin {
                    Origin::Typed => {
                        k += 1;
                        continue;
                    }
                    Origin::Char(s, id) => (s, Some(id)),
                    Origin::Join(s) => (s, None),
                };
                if self.texts[source].id() != own {
                    let i = match taken.iter().position(|t| t.source == source) {
                        Some(i) => i,
                        None => {
                            taken.push(Taken {
                                source,
                                after: k.checked_sub(1).and_then(|p| id_at(&text, p)),
                                src: Vec::new(),
                                copy: Vec::new(),
                            });
                            taken.len() - 1
                        }
                    };
                    if let (Some(id), Some(copy)) = (id, id_at(&text, k)) {
                        taken[i].src.push(id);
                        taken[i].copy.push(copy);
                    }
                }
                if id.is_some() {
                    k += 1;
                }
            }
            let moves = ldoc.get_map(MOVES);
            for t in taken {
                let source = &self.texts[t.source];
                let record = Record {
                    from: source.id().to_string(),
                    to: own.to_string(),
                    after: t.after.map(|id| Run(id.peer, id.counter, 1)),
                    src: runs(&t.src),
                    copy: runs(&t.copy),
                    at,
                    kind: if source.is_deleted() {
                        Kind::Join
                    } else {
                        Kind::Split
                    },
                    host: None,
                };
                let record = Record {
                    host: (record.kind == Kind::Join)
                        .then(|| host_of(ldoc, &text))
                        .flatten(),
                    ..record
                };
                put_str(&moves, &key_of(&record), &to_json(&record)?)?;
            }
        }
        Ok(())
    }
}

// ------------------------------------------------------------------ merges

/// The records by the text they took characters from and the text they put them in, kept level
/// with the root map `moves`.
#[derive(Debug, Default)]
pub(crate) struct Moves {
    by_from: HashMap<String, Vec<String>>,
    by_to: HashMap<String, Vec<String>>,
    /// How many records the index was built from.
    indexed: usize,
    /// A repair joined two copies of one paragraph: the model is read again.
    reread: bool,
    /// The latest move of each peer (its operation's counter).
    latest: HashMap<u64, i32>,
}

/// What the changes of one merge did to a text: the characters inserted, each with the change
/// that inserted it, and the characters deleted, each with the change and the operation.
#[derive(Default)]
struct TextOps {
    inserted: Vec<(ID, ID)>,
    deleted: Vec<(ID, ID, ID)>,
}

/// Answers whether a change saw a move, once per change.
struct Seen<'a> {
    ldoc: &'a LoroDoc,
    /// The change an operation is in.
    change: HashMap<ID, Option<ID>>,
    /// The version a change was made on.
    base: HashMap<ID, Option<VersionVector>>,
}

impl<'a> Seen<'a> {
    fn new(ldoc: &'a LoroDoc) -> Self {
        Self {
            ldoc,
            change: HashMap::new(),
            base: HashMap::new(),
        }
    }

    /// The version the change starting at `change` was made on.
    fn base(&mut self, change: ID) -> Option<&VersionVector> {
        let ldoc = self.ldoc;
        self.base
            .entry(change)
            .or_insert_with(|| {
                let meta = ldoc.get_change(change)?;
                ldoc.frontiers_to_vv(&meta.deps)
            })
            .as_ref()
    }

    /// The change operation `op` is in (its first operation).
    fn change_of(&mut self, op: ID) -> Option<ID> {
        let ldoc = self.ldoc;
        *self
            .change
            .entry(op)
            .or_insert_with(|| ldoc.get_change(op).map(|c| c.id))
    }

    /// The version the change of operation `op` was made on.
    fn base_of(&mut self, op: ID) -> Option<&VersionVector> {
        let change = self.change_of(op)?;
        self.base(change)
    }

    /// Whether operation `op` of the change starting at `change` was made after the move `m`
    /// reached its author.
    fn saw(&mut self, change: ID, op: ID, m: &Move) -> bool {
        let at = m.at();
        if op.peer == at.peer {
            return op.counter >= at.counter;
        }
        self.base(change).is_some_and(|vv| vv.includes_id(at))
    }

    /// Whether operation `op` of the change starting at `change` and the move `m` were made at
    /// the same moment (neither saw the other).
    fn concurrent(&mut self, m: &Move, change: ID, op: ID) -> bool {
        let at = m.at();
        if op.peer == at.peer || self.base_of(at).is_some_and(|vv| vv.includes_id(op)) {
            return false;
        }
        !self.base(change).is_some_and(|vv| vv.includes_id(at))
    }
}

impl Moves {
    /// The keys of the records that took characters of `from`.
    fn keys(&mut self, map: &LoroMap, changed: &[String], from: &str) -> Vec<String> {
        self.refresh(map, changed);
        self.by_from.get(from).cloned().unwrap_or_default()
    }

    fn refresh(&mut self, map: &LoroMap, changed: &[String]) {
        if map.len() == self.indexed {
            for k in changed {
                self.index(k);
            }
            return;
        }
        self.by_from.clear();
        self.by_to.clear();
        self.latest.clear();
        for k in map.keys() {
            self.index(&k);
        }
        self.indexed = map.len();
    }

    fn index(&mut self, key: &str) {
        if let Some(at) = at_of(key) {
            let latest = self.latest.entry(at.peer).or_insert(at.counter);
            *latest = (*latest).max(at.counter);
        }
        for (list, text) in [
            (&mut self.by_from, from_of(key)),
            (&mut self.by_to, to_of(key)),
        ] {
            let keys = list.entry(text.to_owned()).or_default();
            if !keys.iter().any(|k| k == key) {
                keys.push(key.to_owned());
            }
        }
    }

    /// The moves that took characters of `from`, in key order.
    fn of(&mut self, map: &LoroMap, changed: &[String], from: &str) -> Vec<Move> {
        let mut keys = self.keys(map, changed, from);
        keys.sort();
        keys.iter().filter_map(|k| Move::read(map, k)).collect()
    }

    /// Puts right what a merge left of moves made at the same moment as other edits (see the
    /// module documentation). `before` is the version before the merge. Returns whether
    /// anything changed (the caller commits it as a system change).
    ///
    /// # Errors
    ///
    /// The CRDT refused an edit.
    pub(crate) fn repair(
        &mut self,
        ldoc: &LoroDoc,
        before: &VersionVector,
        changes: &[Changed],
    ) -> Result<bool> {
        let map = ldoc.get_map(MOVES);
        let root = ContainerID::new_root(MOVES, ContainerType::Map);
        let arrived: Vec<String> = changes
            .iter()
            .filter(|c| c.target == root)
            .flat_map(|c| c.keys.iter().cloned())
            .collect();
        self.refresh(&map, &arrived);
        let now = ldoc.oplog_vv();
        let mut seen = Seen::new(ldoc);
        let spans: Vec<IdSpan> = if self.by_from.is_empty() {
            Vec::new()
        } else {
            now.sub_iter(before).collect()
        };
        // The first operation of each stretch of the merge: a stretch whose first operation saw
        // every move of a text cannot have edited it at the same moment as one.
        let starts: Vec<ID> = spans
            .iter()
            .map(|s| ID::new(s.peer, s.counter.start))
            .collect();
        let saw = |seen: &mut Seen<'_>, op: ID, at: ID| {
            if op.peer == at.peer {
                op.counter > at.counter
            } else {
                seen.base_of(op).is_some_and(|vv| vv.includes_id(at))
            }
        };
        // Nothing to look at when no move arrived and the merge saw every move.
        let latest = self.latest.clone();
        let quiet = arrived.is_empty()
            && starts.iter().all(|&c| {
                latest
                    .iter()
                    .all(|(&peer, &counter)| saw(&mut seen, c, ID::new(peer, counter)))
            });
        let spans = if quiet { Vec::new() } else { spans };
        // The texts the merge touched that moves took characters from, and the texts of the
        // records that arrived.
        let mut texts: BTreeMap<String, bool> = BTreeMap::new();
        for s in &spans {
            let len = usize::try_from(s.counter.end - s.counter.start).unwrap_or(0);
            for cid in ldoc.get_changed_containers_in(ID::new(s.peer, s.counter.start), len) {
                let id = cid.to_string();
                if self.by_from.contains_key(&id) {
                    texts.entry(id.clone()).or_insert(false);
                }
                for k in self.by_to.get(&id).into_iter().flatten() {
                    texts.entry(from_of(k).to_owned()).or_insert(false);
                }
            }
        }
        for k in &arrived {
            texts.insert(from_of(k).to_owned(), true);
        }
        let mut changed = false;
        for (from, fresh) in texts {
            if !fresh {
                let keys = self.by_from.get(&from).cloned().unwrap_or_default();
                let quiet = starts.iter().all(|&c| {
                    keys.iter()
                        .filter_map(|k| at_of(k))
                        .all(|at| saw(&mut seen, c, at))
                });
                if quiet {
                    continue;
                }
            }
            let Some(text) = text_by_id(ldoc, &from) else {
                continue;
            };
            let moves = self.of(&map, &arrived, &from);
            if moves.is_empty() {
                continue;
            }
            // The changes to look at: the merge's, and for a record that arrived, everything
            // its author had not seen.
            let mut base = before.clone();
            for m in moves.iter().filter(|m| arrived.contains(&m.key)) {
                if let Some(vv) = seen.base_of(m.at()) {
                    base = meet(&base, vv);
                }
            }
            let spans: Vec<IdSpan> = now.sub_iter(&base).collect();
            let ops = text_ops(ldoc, &text, &spans);
            let mut fix = Fix {
                ldoc,
                map: &map,
                moves: self,
                arrived: &arrived,
                seen: &mut seen,
                changed: false,
                read: HashMap::new(),
            };
            fix.dedupe(&moves, fresh, &spans)?;
            if fresh {
                fix.order(&moves)?;
            }
            fix.deletions(&moves, &ops)?;
            fix.typed(&text, &moves, &ops)?;
            if fresh {
                fix.revive(&text, &moves)?;
            }
            changed |= fix.changed;
        }
        // The empty paragraph two replicas gave a list emptied at the same moment: the first
        // stays, the other's text joins it.
        let twins: Vec<(LoroMap, LoroMap)> = changes
            .iter()
            .flat_map(|c| &c.inserted)
            .filter(|cid| cid.container_type() == ContainerType::Map)
            .filter_map(|cid| twin(ldoc, &ldoc.get_map(cid.clone())))
            .collect();
        for (first, other) in twins {
            let mut fix = Fix {
                ldoc,
                map: &map,
                moves: self,
                arrived: &arrived,
                seen: &mut seen,
                changed: false,
                read: HashMap::new(),
            };
            fix.absorb(&first, &other)?;
            if fix.changed {
                changed = true;
                self.reread = true;
            }
        }
        // Paragraphs a join took that a move made at the same moment put back.
        let back: Vec<String> = changes
            .iter()
            .flat_map(|c| &c.inserted)
            .filter(|cid| cid.container_type() == ContainerType::Map)
            .filter_map(|cid| {
                let text = crate::value::find_text(&ldoc.get_map(cid.clone()), "text")?;
                let id = text.id().to_string();
                self.by_from.contains_key(&id).then_some(id)
            })
            .collect();
        for from in back {
            let Some(text) = text_by_id(ldoc, &from) else {
                continue;
            };
            let moves = self.of(&map, &arrived, &from);
            let mut fix = Fix {
                ldoc,
                map: &map,
                moves: self,
                arrived: &arrived,
                seen: &mut seen,
                changed: false,
                read: HashMap::new(),
            };
            fix.rejoin(&text, &moves)?;
            changed |= fix.changed;
        }
        Ok(changed)
    }
}

/// The version both `a` and `b` include.
fn meet(a: &VersionVector, b: &VersionVector) -> VersionVector {
    let mut out = VersionVector::default();
    for (peer, n) in a.iter() {
        if let Some(m) = b.get(peer) {
            out.insert(*peer, (*n).min(*m));
        }
    }
    out
}

/// The insertions and deletions of characters of `text` in the changes `spans`.
fn text_ops(ldoc: &LoroDoc, text: &LoroText, spans: &[IdSpan]) -> TextOps {
    use loro::json::{JsonOpContent, TextOp};
    let target = text.id();
    let mut out = TextOps::default();
    for s in spans {
        for change in ldoc.export_json_in_id_span(*s) {
            for op in change.ops.iter().filter(|o| o.container == target) {
                let JsonOpContent::Text(t) = &op.content else {
                    continue;
                };
                match t {
                    TextOp::Insert { text, .. } => {
                        for k in 0..text.chars().count() {
                            if let Some(c) = i32::try_from(k)
                                .ok()
                                .and_then(|k| op.counter.checked_add(k))
                            {
                                out.inserted.push((change.id, ID::new(change.id.peer, c)));
                            }
                        }
                    }
                    TextOp::Delete { len, start_id, .. } => {
                        // One counter per deleted character; a backward deletion's targets run
                        // from the last.
                        let n = len.unsigned_abs();
                        for k in 0..n {
                            let target = if *len < 0 { n - 1 - k } else { k };
                            let (Ok(k), Ok(target)) = (i32::try_from(k), i32::try_from(target))
                            else {
                                continue;
                            };
                            if let (Some(by), Some(c)) = (
                                op.counter.checked_add(k),
                                start_id.counter.checked_add(target),
                            ) {
                                out.deleted.push((
                                    change.id,
                                    ID::new(change.id.peer, by),
                                    ID::new(start_id.peer, c),
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    out
}

fn text_by_id(ldoc: &LoroDoc, id: &str) -> Option<LoroText> {
    let cid = ContainerID::try_from(id).ok()?;
    ldoc.has_container(&cid).then(|| ldoc.get_text(cid))
}

/// Where a character is now in its text: its index and whether it is still there (a deleted
/// one gives the number of characters before its place).
fn position(ldoc: &LoroDoc, text: &LoroText, id: ID) -> Option<(usize, bool)> {
    let r = ldoc
        .get_cursor_pos(&Cursor::new(Some(id), text.id(), Side::Middle, 0))
        .ok()?;
    Some((r.current.pos, r.update.is_none()))
}

fn alive(ldoc: &LoroDoc, text: &LoroText, id: ID) -> Option<usize> {
    match position(ldoc, text, id) {
        Some((p, true)) => Some(p),
        _ => None,
    }
}

/// A stretch of characters left behind: where it goes in its new text, and its start and end in
/// the old one.
type Stretch = (usize, usize, usize);

/// Each text's deletions in the changes looked at, by the character deleted: the change and the
/// operation.
type Deletions = HashMap<String, HashMap<ID, Vec<(ID, ID)>>>;

/// One repair pass over the moves from one text.
struct Fix<'a, 'b> {
    ldoc: &'a LoroDoc,
    map: &'a LoroMap,
    moves: &'b mut Moves,
    arrived: &'b [String],
    seen: &'b mut Seen<'a>,
    changed: bool,
    /// The moves from each text read so far in this pass.
    read: HashMap<String, Rc<Vec<Move>>>,
}

impl Fix<'_, '_> {
    fn onward(&mut self, from: &str) -> Rc<Vec<Move>> {
        if let Some(m) = self.read.get(from) {
            return Rc::clone(m);
        }
        let m = Rc::new(self.moves.of(self.map, self.arrived, from));
        self.read.insert(from.to_owned(), Rc::clone(&m));
        m
    }

    /// Where a character copied into `text` is now, following the moves that took it on: its
    /// text and id (a deleted character's id when nothing took it).
    fn locate(&mut self, text: LoroText, id: ID, depth: usize) -> Option<(LoroText, ID)> {
        let gone = text.is_deleted();
        if !gone && alive(self.ldoc, &text, id).is_some() {
            return Some((text, id));
        }
        if depth < CHAIN {
            let on = self.onward(&text.id().to_string());
            let mut on: Vec<&Move> = on.iter().collect();
            on.sort_by(|a, b| a.rank().cmp(&b.rank()));
            for m in on {
                let Some(k) = m.find(id) else {
                    continue;
                };
                let Some(next) = text_by_id(self.ldoc, &m.record.to) else {
                    continue;
                };
                if let Some(found) = self.locate(next, m.copy[k], depth + 1) {
                    return Some(found);
                }
            }
        }
        (!gone).then_some((text, id))
    }

    /// Deletes a character copied into `text`, and its copies wherever moves took it since.
    fn delete(&mut self, text: LoroText, id: ID, depth: usize) -> Result<()> {
        if !text.is_deleted()
            && let Some(p) = alive(self.ldoc, &text, id)
        {
            text.delete(p, 1)?;
            self.changed = true;
            return Ok(());
        }
        if depth >= CHAIN {
            return Ok(());
        }
        for m in self.onward(&text.id().to_string()).iter() {
            if let (Some(k), Some(next)) = (m.find(id), text_by_id(self.ldoc, &m.record.to)) {
                self.delete(next, m.copy[k], depth + 1)?;
            }
        }
        Ok(())
    }

    /// A character two moves copied keeps only the copy of the move that took fewer (when one
    /// of them arrived: `fresh`), and a deletion of a copy by someone for whom it was the copy
    /// kept (who had not seen the move that keeps another) is made in the copy kept.
    fn dedupe(&mut self, moves: &[Move], fresh: bool, spans: &[IdSpan]) -> Result<()> {
        let mut by_char: HashMap<ID, Vec<(usize, usize)>> = HashMap::new();
        for (i, m) in moves.iter().enumerate() {
            for (k, id) in m.src.iter().enumerate() {
                by_char.entry(*id).or_default().push((i, k));
            }
        }
        let shared: Vec<&Vec<(usize, usize)>> = by_char.values().filter(|h| h.len() > 1).collect();
        if shared.is_empty() {
            return Ok(());
        }
        let mut ops = Deletions::new();
        let rank = |&(i, _): &(usize, usize)| moves[i].rank();
        let mut work: Vec<(LoroText, ID)> = Vec::new();
        for holders in shared {
            let Some(&(win, wk)) = holders.iter().min_by(|a, b| rank(a).cmp(&rank(b))) else {
                continue;
            };
            let mut carry = false;
            for &(i, k) in holders.iter().filter(|(i, _)| *i != win) {
                let copy = moves[i].copy[k];
                let deletions = self.deletions_of(&moves[i].record.to, copy, spans, &mut ops, 0);
                for (change, by) in deletions {
                    // Kept for the deleter when it was the best of the moves it had seen.
                    let kept = holders
                        .iter()
                        .filter(|h| h.0 == i || self.seen.saw(change, by, &moves[h.0]))
                        .min_by(|a, b| rank(a).cmp(&rank(b)))
                        .is_some_and(|h| h.0 == i);
                    carry |= kept;
                }
                if fresh && let Some(to) = text_by_id(self.ldoc, &moves[i].record.to) {
                    work.push((to, copy));
                }
            }
            if carry && let Some(to) = text_by_id(self.ldoc, &moves[win].record.to) {
                work.push((to, moves[win].copy[wk]));
            }
        }
        for (to, id) in work {
            self.delete(to, id, 0)?;
        }
        Ok(())
    }

    /// The deletions of a character copied into a text, by the change and operation, made by
    /// people for whom that copy was the one kept: in the text itself, or of its copy wherever
    /// moves took it since when that move kept it for them (`ops` caches each text's
    /// deletions).
    fn deletions_of(
        &mut self,
        text: &str,
        id: ID,
        spans: &[IdSpan],
        ops: &mut Deletions,
        depth: usize,
    ) -> Vec<(ID, ID)> {
        let Some(t) = text_by_id(self.ldoc, text) else {
            return Vec::new();
        };
        let onward = self.onward(text);
        let deleted = ops.entry(text.to_owned()).or_insert_with(|| {
            let mut by_target: HashMap<ID, Vec<(ID, ID)>> = HashMap::new();
            for (change, by, target) in text_ops(self.ldoc, &t, spans).deleted {
                by_target.entry(target).or_default().push((change, by));
            }
            by_target
        });
        let mut out: Vec<(ID, ID)> = deleted
            .get(&id)
            .into_iter()
            .flatten()
            .filter(|(_, by)| !onward.iter().any(|o| o.took(*by, id)))
            .copied()
            .collect();
        if depth >= CHAIN {
            return out;
        }
        let holders: Vec<(usize, usize)> = onward
            .iter()
            .enumerate()
            .filter_map(|(i, o)| o.find(id).map(|k| (i, k)))
            .collect();
        for &(i, k) in &holders {
            for (change, by) in self.deletions_of(
                &onward[i].record.to,
                onward[i].copy[k],
                spans,
                ops,
                depth + 1,
            ) {
                let best = holders
                    .iter()
                    .filter(|h| h.0 == i || self.seen.saw(change, by, &onward[h.0]))
                    .min_by(|a, b| onward[a.0].rank().cmp(&onward[b.0].rank()));
                if best.is_some_and(|b| b.0 == i) {
                    out.push((change, by));
                }
            }
        }
        out
    }

    /// Two splits of one paragraph at the same moment each insert a paragraph after it, in an
    /// order neither chose: when the inner split's paragraph came first, the two paragraphs
    /// swap their text (as moves, so that edits to it follow), and the text reads in order.
    /// Paragraphs are not moved: a move would bring back a paragraph someone deleted meanwhile.
    fn order(&mut self, moves: &[Move]) -> Result<()> {
        for s in moves.iter().filter(|m| m.record.kind == Kind::Split) {
            let Some(&first) = s.src.first() else {
                continue;
            };
            let outer = moves
                .iter()
                .filter(|b| b.key != s.key && b.record.kind == Kind::Split)
                .filter_map(|b| {
                    let k = b.find(first)?;
                    (k > 0).then_some((b, k))
                })
                .min_by(|a, b| a.0.rank().cmp(&b.0.rank()));
            let Some((b, k)) = outer else { continue };
            let (Some(inner), Some(outer_to)) = (
                text_by_id(self.ldoc, &s.record.to),
                text_by_id(self.ldoc, &b.record.to),
            ) else {
                continue;
            };
            if inner.is_deleted() {
                continue;
            }
            let Some((holder, _)) = self.locate(outer_to, b.copy[k - 1], 0) else {
                continue;
            };
            let (Some(a), Some(h)) = (slot(self.ldoc, &inner), slot(self.ldoc, &holder)) else {
                continue;
            };
            if a.0 != h.0 || a.1 >= h.1 {
                continue;
            }
            let (inner_ids, holder_ids) = (ids_of(&inner), ids_of(&holder));
            let end = holder.len_unicode();
            self.transfer(&inner, &inner_ids, &holder, end)?;
            self.transfer(&holder, &holder_ids, &inner, 0)?;
        }
        Ok(())
    }

    /// Moves characters of `from` (alive, in order) to `at` in `to`, with their formatting,
    /// recorded as a repair.
    fn transfer(&mut self, from: &LoroText, ids: &[ID], to: &LoroText, at: usize) -> Result<()> {
        let ldoc = self.ldoc;
        let mut places: Vec<usize> = ids.iter().filter_map(|&id| alive(ldoc, from, id)).collect();
        places.sort_unstable();
        if places.is_empty() || from.id() == to.id() {
            return Ok(());
        }
        let me = next_op(ldoc);
        let (chars, segs) = loro_segments(from);
        let attrs = per_char(&segs);
        let stretch: String = places.iter().map(|&i| chars[i]).collect();
        let (_, have) = loro_segments(to);
        let mut want = per_char(&have);
        let at = at.min(to.len_unicode());
        to.insert(at, &stretch)?;
        want.splice(at..at, places.iter().map(|&i| attrs[i].clone()));
        let (_, have) = loro_segments(to);
        format_diff(to, &have, &segments(want))?;
        let src: Vec<ID> = places.iter().filter_map(|&i| id_at(from, i)).collect();
        let copy: Vec<ID> = (at..at + places.len())
            .filter_map(|p| id_at(to, p))
            .collect();
        for &i in places.iter().rev() {
            from.delete(i, 1)?;
        }
        let record = Record {
            from: from.id().to_string(),
            to: to.id().to_string(),
            after: None,
            src: runs(&src),
            copy: runs(&copy),
            at: me,
            kind: Kind::Repair,
            host: None,
        };
        let key = key_of(&record);
        put_str(self.map, &key, &to_json(&record)?)?;
        self.moves.note(&key);
        self.read.clear();
        self.changed = true;
        Ok(())
    }

    /// Characters deleted from a moved stretch at the same moment go from its copy too (the
    /// deletion a move made of the characters it took is not one).
    fn deletions(&mut self, moves: &[Move], ops: &TextOps) -> Result<()> {
        let own = |by: ID, id: ID| moves.iter().any(|m| m.took(by, id));
        let ops: Vec<(ID, ID, ID)> = ops
            .deleted
            .iter()
            .filter(|(_, by, id)| !own(*by, *id))
            .copied()
            .collect();
        for m in moves {
            let hits: Vec<usize> = ops
                .iter()
                .filter_map(|(change, by, id)| {
                    let k = m.find(*id)?;
                    self.seen.concurrent(m, *change, *by).then_some(k)
                })
                .collect();
            if hits.is_empty() {
                continue;
            }
            let Some(to) = text_by_id(self.ldoc, &m.record.to) else {
                continue;
            };
            for k in hits {
                self.delete(to.clone(), m.copy[k], 0)?;
            }
        }
        Ok(())
    }

    /// Characters typed into a moved stretch at the same moment move after it.
    fn typed(&mut self, from: &LoroText, moves: &[Move], ops: &TextOps) -> Result<()> {
        let mut candidates: Vec<(Option<ID>, ID)> =
            ops.inserted.iter().map(|&(c, id)| (Some(c), id)).collect();
        candidates.sort_by_key(|(_, id)| (id.peer, id.counter));
        candidates.dedup_by_key(|(_, id)| *id);
        self.carry(from, moves, candidates, from.is_deleted())
    }

    /// A paragraph a join took that a move made at the same moment put back: what was typed
    /// into it goes where the join took its text, and the paragraph goes again.
    fn rejoin(&mut self, from: &LoroText, moves: &[Move]) -> Result<()> {
        if from.is_deleted() || !moves.iter().any(|m| m.record.kind == Kind::Join) {
            return Ok(());
        }
        // Only when the join's text is somewhere: a join into a paragraph that is gone keeps
        // this one.
        for m in moves.iter().filter(|m| m.record.kind == Kind::Join) {
            let Some(to) = text_by_id(self.ldoc, &m.record.to) else {
                return Ok(());
            };
            if m.copy
                .iter()
                .any(|&c| self.locate(to.clone(), c, 0).is_none())
            {
                return Ok(());
            }
        }
        let candidates: Vec<(Option<ID>, ID)> =
            ids_of(from).into_iter().map(|id| (None, id)).collect();
        self.carry(from, moves, candidates, true)?;
        if let Some((list, index)) = slot(self.ldoc, from) {
            self.ldoc.get_movable_list(list).delete(index, 1)?;
            self.changed = true;
        }
        Ok(())
    }

    /// Two joins at the same moment that removed each other's paragraph (each joined the
    /// other's into its own, in opposite directions) left the text nowhere: each paragraph comes
    /// back where it was, with what was put into it, and the joins cancel out.
    fn revive(&mut self, from: &LoroText, moves: &[Move]) -> Result<()> {
        if !from.is_deleted() {
            return Ok(());
        }
        for m in moves.iter().filter(|m| m.record.kind == Kind::Join) {
            let Some(to) = text_by_id(self.ldoc, &m.record.to) else {
                continue;
            };
            if !to.is_deleted() {
                continue;
            }
            // Its copies are in a paragraph that is gone, and no move took them on.
            let lost = m
                .copy
                .iter()
                .any(|&c| self.locate(to.clone(), c, 0).is_none());
            if !lost {
                continue;
            }
            // Only a cycle: the paragraph joined into another that was joined back into this one.
            let from_id = from.id().to_string();
            let cycle = self
                .onward(&m.record.to)
                .iter()
                .any(|o| o.record.kind == Kind::Join && o.record.to == from_id);
            if cycle && let Some(host) = m.record.host.clone() {
                self.bring_back(&to, &host)?;
            }
        }
        Ok(())
    }

    /// Brings back a paragraph two joins removed: a copy of its map at its old place, with the
    /// characters no move took from it, recorded as a join into the copy.
    fn bring_back(&mut self, gone: &LoroText, host: &(String, Vec<u8>)) -> Result<()> {
        let Ok(map_id) = ContainerID::try_from(host.0.as_str()) else {
            return Ok(());
        };
        let Ok(cursor) = Cursor::decode(&host.1) else {
            return Ok(());
        };
        let Ok(at) = self.ldoc.get_cursor_pos(&cursor) else {
            return Ok(());
        };
        let list = self.ldoc.get_movable_list(cursor.container.clone());
        let old = self.ldoc.get_map(map_id);
        let me = next_op(self.ldoc);
        let index = at.current.pos.min(list.len());
        let map = list.insert_container(index, LoroMap::new())?;
        copy_map(&old, &map, &["text"], 0)?;
        map.insert(REVIVED, true)?;
        let text = map.insert_container("text", LoroText::new())?;
        let taken: HashSet<ID> = self
            .onward(&gone.id().to_string())
            .iter()
            .flat_map(|m| m.src.iter().copied())
            .collect();
        let (chars, segs) = loro_segments(gone);
        let attrs = per_char(&segs);
        let keep: Vec<usize> = (0..chars.len())
            .filter(|&i| id_at(gone, i).is_some_and(|id| !taken.contains(&id)))
            .collect();
        let src: Vec<ID> = keep.iter().filter_map(|&i| id_at(gone, i)).collect();
        if !keep.is_empty() {
            let s: String = keep.iter().map(|&i| chars[i]).collect();
            text.insert(0, &s)?;
            let (_, have) = loro_segments(&text);
            let want: Vec<Attrs> = keep.iter().map(|&i| attrs[i].clone()).collect();
            format_diff(&text, &have, &segments(want))?;
        }
        let copy: Vec<ID> = (0..text.len_unicode())
            .filter_map(|p| id_at(&text, p))
            .collect();
        let record = Record {
            from: gone.id().to_string(),
            to: text.id().to_string(),
            after: None,
            src: runs(&src),
            copy: runs(&copy),
            at: me,
            kind: Kind::Join,
            host: host_of(self.ldoc, &text),
        };
        let key = key_of(&record);
        put_str(self.map, &key, &to_json(&record)?)?;
        self.moves.note(&key);
        self.read.clear();
        self.changed = true;
        Ok(())
    }

    /// Joins `other`, a second copy of the empty paragraph of an emptied list, to `first`: its
    /// text (typed before the two met) goes to the end of `first`, recorded as a join, and the
    /// copy goes.
    fn absorb(&mut self, first: &LoroMap, other: &LoroMap) -> Result<()> {
        let (Some(keep), Some(gone)) = (
            crate::value::find_text(first, "text"),
            crate::value::find_text(other, "text"),
        ) else {
            return Ok(());
        };
        let Some((list, index)) = slot(self.ldoc, &gone) else {
            return Ok(());
        };
        let at = next_op(self.ldoc);
        let src = ids_of(&gone);
        if !src.is_empty() {
            let end = keep.len_unicode();
            let after = end.checked_sub(1).and_then(|p| id_at(&keep, p));
            let (chars, segs) = loro_segments(&gone);
            let (_, have) = loro_segments(&keep);
            let mut want = per_char(&have);
            keep.insert(end, &chars.iter().collect::<String>())?;
            want.extend(per_char(&segs));
            let (_, have) = loro_segments(&keep);
            format_diff(&keep, &have, &segments(want))?;
            let copy: Vec<ID> = (end..keep.len_unicode())
                .filter_map(|p| id_at(&keep, p))
                .collect();
            let record = Record {
                from: gone.id().to_string(),
                to: keep.id().to_string(),
                after: after.map(|id| Run(id.peer, id.counter, 1)),
                src: runs(&src),
                copy: runs(&copy),
                at,
                kind: Kind::Join,
                host: host_of(self.ldoc, &keep),
            };
            let key = key_of(&record);
            put_str(self.map, &key, &to_json(&record)?)?;
            self.moves.note(&key);
            self.read.clear();
        }
        self.ldoc.get_movable_list(list).delete(index, 1)?;
        self.changed = true;
        Ok(())
    }

    /// Moves the characters among `candidates` that moves left behind after the moved
    /// characters. A candidate with its change must have been typed at the same moment as the
    /// move; one without (a paragraph a join took) goes to the join. `dead`: the paragraph is
    /// gone (or going), so what moves is not deleted from it.
    fn carry(
        &mut self,
        from: &LoroText,
        moves: &[Move],
        candidates: Vec<(Option<ID>, ID)>,
        dead: bool,
    ) -> Result<()> {
        let movers: Vec<&Move> = moves
            .iter()
            .filter(|m| m.record.kind != Kind::Repair)
            .collect();
        if movers.is_empty() {
            return Ok(());
        }
        let ldoc = self.ldoc;
        let taken: HashSet<ID> = moves.iter().flat_map(|m| m.src.iter().copied()).collect();
        // Each character left behind, with the move it belongs to and how many of that move's
        // characters come before it.
        let mut left: Vec<(usize, usize, usize)> = Vec::new();
        let mut place: HashMap<(usize, usize), usize> = HashMap::new();
        for (change, id) in candidates {
            if taken.contains(&id) {
                continue;
            }
            let Some(i) = alive(ldoc, from, id) else {
                continue;
            };
            let mut best: Option<(usize, usize)> = None;
            for (mi, m) in movers.iter().enumerate() {
                let joined = m.record.kind == Kind::Join;
                let eligible = match change {
                    Some(c) => self.seen.concurrent(m, c, id),
                    None => joined,
                };
                if !eligible {
                    continue;
                }
                let j = before(ldoc, from, &m.src, i, &mut place, mi);
                if (joined || j > 0) && best.is_none_or(|(b, _)| m.rank() < movers[b].rank()) {
                    best = Some((mi, j));
                }
            }
            if let Some((mi, j)) = best {
                left.push((i, mi, j));
            }
        }
        if left.is_empty() {
            return Ok(());
        }
        left.sort_unstable();
        // Stretches of adjacent characters going to the same place: (start, end, move, place).
        let mut groups: Vec<(usize, usize, usize, usize)> = Vec::new();
        for (i, mi, j) in left {
            match groups.last_mut() {
                Some((_, end, g, p)) if *end == i && *g == mi && *p == j => *end += 1,
                _ => groups.push((i, i + 1, mi, j)),
            }
        }
        let (chars, segs) = loro_segments(from);
        let attrs = per_char(&segs);
        // Where each stretch goes: a text and a position in it.
        let mut by_text: BTreeMap<String, (LoroText, Vec<Stretch>)> = BTreeMap::new();
        for &(s, e, mi, j) in &groups {
            let Some((text, at)) = self.target(movers[mi], j) else {
                continue;
            };
            if text.id() == from.id() {
                continue;
            }
            by_text
                .entry(text.id().to_string())
                .or_insert_with(|| (text.clone(), Vec::new()))
                .1
                .push((at, s, e));
        }
        let me = next_op(ldoc);
        let mut done: Vec<(usize, usize)> = Vec::new();
        for (to_id, (to, mut inserts)) in by_text {
            // Later stretches first, so earlier positions stay valid; stretches for one place keep
            // their order.
            inserts.reverse();
            inserts.sort_by_key(|i| std::cmp::Reverse(i.0));
            let (_, have) = loro_segments(&to);
            let mut want = per_char(&have);
            let mut src: Vec<(usize, Vec<ID>, Vec<ID>)> = Vec::new();
            for &(at, s, e) in &inserts {
                let at = at.min(to.len_unicode());
                let stretch: String = chars[s..e].iter().collect();
                to.insert(at, &stretch)?;
                want.splice(at..at, attrs[s..e].iter().cloned());
                let copy: Vec<ID> = (at..at + (e - s)).filter_map(|p| id_at(&to, p)).collect();
                let ids: Vec<ID> = (s..e).filter_map(|p| id_at(from, p)).collect();
                src.push((s, ids, copy));
                done.push((s, e));
            }
            let (_, have) = loro_segments(&to);
            format_diff(&to, &have, &segments(want))?;
            src.sort_by_key(|(s, ..)| *s);
            let (ids, copy): (Vec<ID>, Vec<ID>) = src
                .into_iter()
                .flat_map(|(_, a, b)| a.into_iter().zip(b))
                .unzip();
            let record = Record {
                from: from.id().to_string(),
                to: to_id,
                after: None,
                src: runs(&ids),
                copy: runs(&copy),
                at: me,
                kind: Kind::Repair,
                host: None,
            };
            let key = key_of(&record);
            put_str(self.map, &key, &to_json(&record)?)?;
            self.moves.note(&key);
            self.read.clear();
            self.changed = true;
        }
        // The first paragraph stays, without what moved; a paragraph that is gone keeps it.
        if !dead {
            done.sort_unstable();
            for &(s, e) in done.iter().rev() {
                from.delete(s, e - s)?;
            }
        }
        Ok(())
    }

    /// Where a character typed after the first `j` characters of a move goes.
    fn target(&mut self, m: &Move, j: usize) -> Option<(LoroText, usize)> {
        let to = text_by_id(self.ldoc, &m.record.to)?;
        let ldoc = self.ldoc;
        let after = |text: &LoroText, id: ID| match position(ldoc, text, id) {
            Some((p, true)) => p + 1,
            Some((p, false)) => p,
            None => text.len_unicode(),
        };
        // After the nearest moved character before it that can still be found.
        for k in (0..j).rev() {
            if let Some((text, id)) = self.locate(to.clone(), m.copy[k], 0) {
                let at = after(&text, id);
                return Some((text, at));
            }
        }
        self.start(m, 0)
    }

    /// Where the characters a move took begin now (for a paragraph joined to another since,
    /// where its characters begin there).
    fn start(&mut self, m: &Move, depth: usize) -> Option<(LoroText, usize)> {
        let ldoc = self.ldoc;
        let to = text_by_id(ldoc, &m.record.to)?;
        if let Some(&first) = m.copy.first()
            && let Some((text, id)) = self.locate(to.clone(), first, 0)
        {
            let at = position(ldoc, &text, id).map_or(0, |p| p.0);
            return Some((text, at));
        }
        if let Some(Run(peer, c, _)) = m.record.after
            && let Some((text, id)) = self.locate(to.clone(), ID::new(peer, c), 0)
        {
            let at = match position(ldoc, &text, id) {
                Some((p, true)) => p + 1,
                Some((p, false)) => p,
                None => text.len_unicode(),
            };
            return Some((text, at));
        }
        if !to.is_deleted() {
            return Some((to, 0));
        }
        if depth >= CHAIN {
            return None;
        }
        let on = self.onward(&m.record.to);
        let mut joins: Vec<&Move> = on.iter().filter(|o| o.record.kind == Kind::Join).collect();
        joins.sort_by(|a, b| a.rank().cmp(&b.rank()));
        joins.into_iter().find_map(|o| self.start(o, depth + 1))
    }
}

impl Moves {
    /// Whether the last repair needs the model read again from the shared state.
    pub(crate) fn take_reread(&mut self) -> bool {
        std::mem::take(&mut self.reread)
    }

    fn note(&mut self, key: &str) {
        self.index(key);
        self.indexed += 1;
    }
}

/// How many of a move's characters come before index `i` of `text` (cached per move).
fn before(
    ldoc: &LoroDoc,
    text: &LoroText,
    src: &[ID],
    i: usize,
    place: &mut HashMap<(usize, usize), usize>,
    m: usize,
) -> usize {
    let (mut lo, mut hi) = (0, src.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let p = *place
            .entry((m, mid))
            .or_insert_with(|| position(ldoc, text, src[mid]).map_or(usize::MAX, |p| p.0));
        if p <= i {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    lo
}

/// The paragraph map of a text and a cursor at its place in its list.
fn host_of(ldoc: &LoroDoc, text: &LoroText) -> Option<(String, Vec<u8>)> {
    let path = ldoc.get_path_to_container(&text.id())?;
    let (map, _) = path.get(path.len().checked_sub(2)?)?;
    let (list, index) = slot(ldoc, text)?;
    let cursor = ldoc
        .get_movable_list(list)
        .get_cursor(index, Side::Middle)?;
    Some((map.to_string(), cursor.encode()))
}

/// Copies the values and containers of `from` into `to` (a new map), texts with their
/// formatting, except the keys in `skip`.
fn copy_map(from: &LoroMap, to: &LoroMap, skip: &[&str], depth: usize) -> Result<()> {
    use loro::{LoroList, LoroMovableList, ValueOrContainer};
    if depth > 16 {
        return Ok(());
    }
    for key in from.keys() {
        if skip.contains(&key.as_str()) {
            continue;
        }
        match from.get(&key) {
            Some(ValueOrContainer::Value(v)) => {
                to.insert(&key, v)?;
            }
            Some(ValueOrContainer::Container(c)) => match c {
                loro::Container::Map(m) => {
                    copy_map(
                        &m,
                        &to.insert_container(&key, LoroMap::new())?,
                        &[],
                        depth + 1,
                    )?;
                }
                loro::Container::Text(t) => {
                    copy_text(&t, &to.insert_container(&key, LoroText::new())?)?;
                }
                loro::Container::MovableList(l) => {
                    let new = to.insert_container(&key, LoroMovableList::new())?;
                    for i in 0..l.len() {
                        if let Some(ValueOrContainer::Value(v)) = l.get(i) {
                            new.push(v)?;
                        }
                    }
                }
                loro::Container::List(l) => {
                    let new = to.insert_container(&key, LoroList::new())?;
                    for i in 0..l.len() {
                        if let Some(ValueOrContainer::Value(v)) = l.get(i) {
                            new.push(v)?;
                        }
                    }
                }
                _ => {}
            },
            None => {}
        }
    }
    Ok(())
}

/// Copies a text's characters and formatting into an empty one.
fn copy_text(from: &LoroText, to: &LoroText) -> Result<()> {
    let (chars, segs) = loro_segments(from);
    if chars.is_empty() {
        return Ok(());
    }
    to.insert(0, &chars.iter().collect::<String>())?;
    let (_, have) = loro_segments(to);
    format_diff(to, &have, &segs)
}

/// For a paragraph map that is a second copy of an emptied list's empty paragraph, the first
/// copy and it.
fn twin(ldoc: &LoroDoc, map: &LoroMap) -> Option<(LoroMap, LoroMap)> {
    let id = crate::value::get_id(map, crate::write::blocks::ID).ok()?;
    if !crate::write::is_empty_paragraph_id(id) && map.get(REVIVED).is_none() {
        return None;
    }
    let path = ldoc.get_path_to_container(&map.id())?;
    let (list, _) = path.get(path.len().checked_sub(2)?)?;
    if list.container_type() != ContainerType::MovableList {
        return None;
    }
    let first = crate::value::maps_of(&ldoc.get_movable_list(list.clone()))
        .into_iter()
        .find(|m| crate::value::get_id(m, crate::write::blocks::ID).ok() == Some(id))?;
    (first.id() != map.id()).then(|| (first, map.clone()))
}

/// The list a text's paragraph is in and its index there.
fn slot(ldoc: &LoroDoc, text: &LoroText) -> Option<(ContainerID, usize)> {
    let path = ldoc.get_path_to_container(&text.id())?;
    let n = path.len();
    if n < 3 {
        return None;
    }
    let (list, _) = &path[n - 3];
    let (_, index) = &path[n - 2];
    match (list.container_type(), index) {
        (ContainerType::MovableList, loro::Index::Seq(i)) => Some((list.clone(), *i)),
        _ => None,
    }
}

fn per_char(segs: &[Seg]) -> Vec<Attrs> {
    segs.iter()
        .flat_map(|s| std::iter::repeat_n(s.attrs.clone(), s.len))
        .collect()
}

fn segments(chars: Vec<Attrs>) -> Vec<Seg> {
    let mut out: Vec<Seg> = Vec::new();
    for attrs in chars {
        match out.last_mut() {
            Some(last) if last.attrs == attrs => last.len += 1,
            _ => out.push(Seg { len: 1, attrs }),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_compress_into_runs() {
        let ids = [
            ID::new(1, 4),
            ID::new(1, 5),
            ID::new(1, 6),
            ID::new(2, 0),
            ID::new(1, 7),
        ];
        let r = runs(&ids);
        assert_eq!(r, vec![Run(1, 4, 3), Run(2, 0, 1), Run(1, 7, 1)]);
        assert_eq!(expand(&r), ids);
    }

    #[test]
    fn a_deletion_names_the_characters_it_deleted() -> Result<()> {
        let ldoc = LoroDoc::new();
        ldoc.set_peer_id(1)?;
        let t = ldoc.get_text("t");
        t.insert(0, "abcdef")?;
        ldoc.commit();
        let before = ldoc.oplog_vv();
        t.delete(2, 3)?;
        ldoc.commit();
        let spans: Vec<IdSpan> = ldoc.oplog_vv().sub_iter(&before).collect();
        let ops = text_ops(&ldoc, &t, &spans);
        let ids: Vec<ID> = ops.deleted.iter().map(|(.., id)| *id).collect();
        assert_eq!(ids, vec![ID::new(1, 2), ID::new(1, 3), ID::new(1, 4)]);
        Ok(())
    }
}
