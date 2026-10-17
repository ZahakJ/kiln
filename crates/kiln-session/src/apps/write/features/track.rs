// SPDX-License-Identifier: AGPL-3.0-or-later
//! Track Changes.
//!
//! **Recording.** While `w:trackRevisions` is on, every transaction the editor builds is
//! rewritten before it is applied ([`WriteDoc::apply_tracked_parts`]): inserted text and blocks
//! carry an insertion mark of the reviewer; deleted text stays in the paragraph with a deletion
//! mark (the reviewer's own insertions are really removed, as in Word); joined paragraphs keep
//! both paragraphs and mark the paragraph mark deleted; split paragraphs mark the new paragraph
//! mark inserted; formatting changes record the formatting before them; removed table rows are
//! marked deleted. The editor's transactions are written for the document as it looks without
//! the deleted text, so the rewrite keeps a map of the "ghost" text it kept and of the joins it
//! did not make, and moves every later position of the same transaction through it.
//!
//! Adjacent changes by the same reviewer share one revision record, so typing a word or
//! backspacing over one makes a single change in the Reviewing pane.
//!
//! **Reviewing.** Accept and Reject for the change at the selection (and move to the next), all
//! changes shown, all changes, one change of the Reviewing pane; Previous and Next; the markup
//! modes, which set the layout's options ([`WriteDoc::sync_review_display`]); the change bars
//! and comment connectors of the overlay.

use std::cmp::Ordering;
use std::collections::HashMap;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::query::{RevisionEntry, RevisionLocation};
use kiln_doc::revision::{
    InsDel, InsDelRevision, ParagraphPropsChange, ParagraphRevision, RevisionKind, RunPropsChange,
};
use kiln_doc::{
    Block, EditKind, Mark, Op, ParagraphProps, Pos, Range, RevisionInfo, RunProps, RunSpan, Story,
    Transaction, edit,
};
use kiln_doc_layout::annotations;
use kiln_protocol::commands::write_features::{
    ChangeScope, MarkupKind, MarkupMode, ReviewerInfo, RevisionEntryInfo, RevisionKindInfo,
    RevisionRef, RevisionsInfo,
};
use kiln_scene::{Bounds, DrawOp, Paint};

use super::super::WriteDoc;
use super::super::view::WriteView;
use super::iso;

/// Maps a model error.
fn doc_err(e: kiln_doc::DocError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

/// Text of a paragraph a transaction treats as gone but that stays in the document (deleted
/// with tracking): `(start, len)` in the document's chars, sorted, disjoint.
type Ghosts = Vec<(usize, usize)>;

/// What the rewrite of one transaction keeps track of.
#[derive(Default)]
struct Rewrite {
    ghosts: HashMap<NodeId, Ghosts>,
    /// Paragraphs the transaction joined to their next one (`first → (second, length of
    /// first as the transaction saw it)`): the join was not made.
    joined: HashMap<NodeId, (NodeId, usize)>,
    /// Blocks the transaction removed but that stay (deleted with tracking), per container:
    /// `(index, count)` in the container's current order.
    kept_blocks: HashMap<NodeId, Vec<(usize, usize)>>,
    /// Rows the transaction removed but that stay, per table.
    kept_rows: HashMap<NodeId, Vec<(usize, usize)>>,
    /// Paragraphs the transaction created (no formatting changes are tracked on them).
    created: Vec<NodeId>,
    forward: Vec<Op>,
    inverses: Vec<Transaction>,
}

/// The kind of a model revision for the wire.
fn kind_info(k: RevisionKind) -> RevisionKindInfo {
    match k {
        RevisionKind::Insertion => RevisionKindInfo::Inserted,
        RevisionKind::Deletion => RevisionKindInfo::Deleted,
        RevisionKind::MoveFrom => RevisionKindInfo::MovedFrom,
        RevisionKind::MoveTo => RevisionKindInfo::MovedTo,
        RevisionKind::RunFormat => RevisionKindInfo::Formatted,
        RevisionKind::ParagraphFormat | RevisionKind::ParagraphMarkFormat => {
            RevisionKindInfo::ParagraphFormatted
        }
        RevisionKind::ParagraphInsertion => RevisionKindInfo::ParagraphInserted,
        RevisionKind::ParagraphDeletion => RevisionKindInfo::ParagraphDeleted,
        RevisionKind::RowInsertion => RevisionKindInfo::RowInserted,
        RevisionKind::RowDeletion => RevisionKindInfo::RowDeleted,
    }
}

/// The wire address of a revision.
pub(super) fn revision_ref(e: &RevisionEntry) -> RevisionRef {
    let (node, start, end) = match e.location {
        RevisionLocation::Text { para, start, end } => (
            para,
            u32::try_from(start).unwrap_or(u32::MAX),
            u32::try_from(end).unwrap_or(u32::MAX),
        ),
        RevisionLocation::Paragraph(p) => (p, 0, 0),
        RevisionLocation::Row(r) => (r, 0, 0),
    };
    RevisionRef {
        kind: kind_info(e.kind),
        node,
        start,
        end,
    }
}

/// Whether the change is an insertion or deletion (Show Markup ▸ Insertions and Deletions) or
/// formatting (Show Markup ▸ Formatting).
fn is_format(k: RevisionKind) -> bool {
    matches!(
        k,
        RevisionKind::RunFormat | RevisionKind::ParagraphFormat | RevisionKind::ParagraphMarkFormat
    )
}

/// The identity of a revision for repeated lookups while others are resolved around it.
#[derive(Debug, Clone, PartialEq)]
struct Target {
    kind: RevisionKind,
    info: RevisionInfo,
    location: RevisionLocation,
}

/// The mark kind of a text-level revision kind.
fn mark_matches(m: &Mark, kind: RevisionKind) -> bool {
    matches!(
        (m, kind),
        (Mark::Insertion(_), RevisionKind::Insertion)
            | (Mark::Deletion(_), RevisionKind::Deletion)
            | (Mark::MoveFrom(_), RevisionKind::MoveFrom)
            | (Mark::MoveTo(_), RevisionKind::MoveTo)
            | (Mark::FormatChange { .. }, RevisionKind::RunFormat)
    )
}

/// Names of what differs between two run formattings ("Bold, Font: Fraunces").
fn describe_runs(before: &RunProps, after: &RunProps) -> String {
    let mut out: Vec<String> = Vec::new();
    let flag = |name: &str, b: Option<bool>, a: Option<bool>, out: &mut Vec<String>| {
        if b.unwrap_or(false) != a.unwrap_or(false) {
            out.push(if a.unwrap_or(false) {
                name.to_owned()
            } else {
                format!("Not {name}")
            });
        }
    };
    flag("Bold", before.bold, after.bold, &mut out);
    flag("Italic", before.italic, after.italic, &mut out);
    flag("Strikethrough", before.strike, after.strike, &mut out);
    flag("All caps", before.caps, after.caps, &mut out);
    flag("Small caps", before.small_caps, after.small_caps, &mut out);
    if before.underline != after.underline {
        out.push(if after.underline.is_some() {
            "Underline".to_owned()
        } else {
            "No underline".to_owned()
        });
    }
    if before.size != after.size
        && let Some(s) = after.size
    {
        out.push(format!("Font size: {} pt", f64::from(s) / 2.0));
    }
    if before.fonts != after.fonts
        && let Some(f) = after.fonts.as_ref().and_then(|f| f.ascii.clone())
    {
        out.push(format!("Font: {f}"));
    }
    if before.color != after.color {
        out.push("Font colour".to_owned());
    }
    if before.highlight != after.highlight {
        out.push("Highlight".to_owned());
    }
    if before.style_id != after.style_id {
        out.push(format!(
            "Style: {}",
            after
                .style_id
                .as_deref()
                .unwrap_or("Default Paragraph Font")
        ));
    }
    if before.vert_align != after.vert_align {
        out.push("Superscript/Subscript".to_owned());
    }
    if out.is_empty() {
        "Formatted".to_owned()
    } else {
        format!("Formatted: {}", out.join(", "))
    }
}

fn describe_paragraph(before: &ParagraphProps, after: &ParagraphProps) -> String {
    let mut out: Vec<String> = Vec::new();
    if before.style_id != after.style_id {
        out.push(format!(
            "Style: {}",
            after.style_id.as_deref().unwrap_or("Normal")
        ));
    }
    if before.alignment != after.alignment {
        out.push("Alignment".to_owned());
    }
    if before.indent != after.indent {
        out.push("Indent".to_owned());
    }
    if before.spacing != after.spacing {
        out.push("Spacing".to_owned());
    }
    if before.numbering != after.numbering {
        out.push("List".to_owned());
    }
    if before.borders != after.borders || before.shading != after.shading {
        out.push("Borders and shading".to_owned());
    }
    if out.is_empty() {
        "Formatted paragraph".to_owned()
    } else {
        format!("Formatted: {}", out.join(", "))
    }
}

/// The story's name in the Reviewing pane.
fn story_label(doc: &kiln_doc::Document, para: NodeId) -> String {
    match doc.story_of(para) {
        Some(Story::HeaderFooter(i)) => match doc.headers_footers.get(i).map(|h| h.part) {
            Some(kiln_doc::HeaderFooterPart::Footer) => "Footer".to_owned(),
            _ => "Header".to_owned(),
        },
        Some(Story::Note(i)) => match doc.notes.get(i).map(|n| n.kind) {
            Some(kiln_doc::st::NoteKind::Endnote) => "Endnote".to_owned(),
            _ => "Footnote".to_owned(),
        },
        Some(Story::Comment(_)) => "Comment".to_owned(),
        _ => "Main Document".to_owned(),
    }
}

impl WriteDoc {
    // ── Recording ───────────────────────────────────────────────────────────────────────────

    /// Whether a transaction is recorded as tracked changes.
    pub(in crate::apps::write) fn tracking_applies(&self, tx: &Transaction) -> bool {
        self.doc.settings.track_revisions && !self.features.untracked && !tx.is_empty()
    }

    /// Track Changes on or off. Like Word, switching is not an undo step; the setting is saved
    /// with the document (`w:trackRevisions`).
    pub(super) fn toggle_tracking(&mut self) {
        let mut settings = self.doc.settings.clone();
        settings.track_revisions = !settings.track_revisions;
        let tx = Transaction::new(vec![Op::SetSettings {
            settings: Box::new(settings),
        }]);
        if self.doc.apply(tx.clone()).is_ok() {
            self.touched.record(&self.doc, &tx);
        }
    }

    /// The reviewer's name.
    pub(super) fn set_reviewer(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() || name.chars().count() > 128 {
            return Err(KilnError::InvalidInput(
                "a reviewer name of 1–128 characters is needed".into(),
            ));
        }
        self.features.review.author = name.to_owned();
        Ok(())
    }

    /// A new revision record for the reviewer, now.
    fn new_revision(&mut self) -> RevisionInfo {
        if self.features.next_revision == 0 {
            let max = self
                .doc
                .revisions()
                .iter()
                .map(|r| r.info.id)
                .max()
                .unwrap_or(0);
            self.features.next_revision = max.saturating_add(1).max(1);
        }
        let id = self.features.next_revision;
        self.features.next_revision = self.features.next_revision.saturating_add(1);
        RevisionInfo::new(id, &self.features.review.author, Some(&iso(&self.now())))
    }

    /// The reviewer's insertion record covering `off` (or ending there), to continue it.
    fn own_insertion_near(&self, para: NodeId, off: usize) -> Option<RevisionInfo> {
        let p = self.doc.paragraph(para)?;
        let me = &self.features.review.author;
        p.content.marks().iter().find_map(|m| match &m.mark {
            Mark::Insertion(i) if i.author == *me && m.start <= off && off <= m.end => {
                Some(i.clone())
            }
            _ => None,
        })
    }

    /// The reviewer's deletion record touching `start..end`, to continue it.
    fn own_deletion_near(&self, para: NodeId, start: usize, end: usize) -> Option<RevisionInfo> {
        let p = self.doc.paragraph(para)?;
        let me = &self.features.review.author;
        p.content.marks().iter().find_map(|m| match &m.mark {
            Mark::Deletion(i) if i.author == *me && m.start <= end && start <= m.end => {
                Some(i.clone())
            }
            _ => None,
        })
    }

    /// Applies one op now and records it.
    fn put(&mut self, rw: &mut Rewrite, op: Op) -> Result<()> {
        let tx = Transaction::new(vec![op]);
        let inverse = self.doc.apply(tx.clone()).map_err(doc_err)?;
        rw.forward.extend(tx.ops);
        rw.inverses.push(inverse);
        Ok(())
    }

    /// Where a transaction's position is in the document: through joins it did not make and
    /// past (or before) text it deleted that stays.
    fn map_pos(rw: &Rewrite, para: NodeId, off: usize, after_ghosts: bool) -> (NodeId, usize) {
        let (mut p, mut o) = (para, off);
        let mut guard = 0;
        while let Some(&(second, len)) = rw.joined.get(&p) {
            if o <= len || guard > 10_000 {
                break;
            }
            o -= len;
            p = second;
            guard += 1;
        }
        let mut real = o;
        if let Some(g) = rw.ghosts.get(&p) {
            for &(s, l) in g {
                if s < real || (s == real && after_ghosts) {
                    real += l;
                } else {
                    break;
                }
            }
        }
        (p, real)
    }

    fn add_ghost(rw: &mut Rewrite, para: NodeId, start: usize, len: usize) {
        if len == 0 {
            return;
        }
        let g = rw.ghosts.entry(para).or_default();
        g.push((start, len));
        g.sort_unstable();
        let mut merged: Ghosts = Vec::with_capacity(g.len());
        for &(s, l) in g.iter() {
            match merged.last_mut() {
                Some((ms, ml)) if *ms + *ml >= s => {
                    let end = (s + l).max(*ms + *ml);
                    *ml = end - *ms;
                }
                _ => merged.push((s, l)),
            }
        }
        *g = merged;
    }

    /// Shifts the ghosts of a paragraph at or after `at` by `delta` chars.
    fn shift_ghosts(rw: &mut Rewrite, para: NodeId, at: usize, delta: isize) {
        if let Some(g) = rw.ghosts.get_mut(&para) {
            for (s, _) in g.iter_mut() {
                if *s >= at {
                    *s = s.saturating_add_signed(delta);
                }
            }
        }
    }

    /// Whether a paragraph is in a comment (comments are not tracked, as in Word).
    fn untracked_story(&self, para: NodeId) -> bool {
        matches!(self.doc.story_of(para), Some(Story::Comment(_)))
    }

    /// Deletes `start..end` of a paragraph with tracking: the reviewer's own insertions go,
    /// everything else gets a deletion mark. Returns the chars that stayed.
    fn tracked_delete(
        &mut self,
        rw: &mut Rewrite,
        para: NodeId,
        start: usize,
        end: usize,
    ) -> Result<usize> {
        let p = self.paragraph(para)?;
        let end = end.min(p.content.len());
        if start >= end {
            return Ok(0);
        }
        let me = self.features.review.author.clone();
        // Classify each char: 0 normal, 1 already deleted, 2 the reviewer's own insertion.
        let mut class = vec![0u8; end - start];
        for m in p.content.marks() {
            let (a, b) = (m.start.max(start), m.end.min(end));
            if a >= b {
                continue;
            }
            let c = match &m.mark {
                Mark::Deletion(_) | Mark::MoveFrom(_) => 1,
                Mark::Insertion(i) if i.author == me => 2,
                _ => continue,
            };
            for x in &mut class[a - start..b - start] {
                if c == 1 || *x == 0 {
                    *x = c;
                }
            }
        }
        let info = self
            .own_deletion_near(para, start, end)
            .unwrap_or_else(|| self.new_revision());
        // Segments, applied from the end so earlier offsets stay valid.
        let mut segs: Vec<(usize, usize, u8)> = Vec::new();
        for (i, c) in class.iter().enumerate() {
            let at = start + i;
            match segs.last_mut() {
                Some((_, e, k)) if *k == *c && *e == at => *e = at + 1,
                _ => segs.push((at, at + 1, *c)),
            }
        }
        let mut removed = 0usize;
        for (a, b, c) in segs.into_iter().rev() {
            match c {
                0 => self.put(
                    rw,
                    Op::AddMark {
                        para,
                        start: a,
                        end: b,
                        mark: Mark::Deletion(info.clone()),
                    },
                )?,
                2 => {
                    self.put(
                        rw,
                        Op::DeleteInline {
                            para,
                            offset: a,
                            len: b - a,
                        },
                    )?;
                    removed += b - a;
                }
                _ => {}
            }
        }
        // Ghosts: the range's survivors are one run of chars at `start`.
        if let Some(g) = rw.ghosts.get_mut(&para) {
            g.retain(|&(s, l)| !(s >= start && s + l <= end));
        }
        #[allow(clippy::cast_possible_wrap)]
        Self::shift_ghosts(rw, para, end, -(removed as isize));
        let kept = end - start - removed;
        Self::add_ghost(rw, para, start, kept);
        Ok(kept)
    }

    /// Marks a whole paragraph (content and mark) deleted.
    fn tracked_delete_paragraph(&mut self, rw: &mut Rewrite, para: NodeId) -> Result<()> {
        let len = self.paragraph(para)?.content.len();
        let kept = self.tracked_delete(rw, para, 0, len)?;
        let _ = kept;
        let p = self.paragraph(para)?;
        if p.revision.mark.as_ref().map(|m| m.kind) != Some(InsDel::Deletion) {
            let mut r = p.revision.clone();
            let info = self.new_revision();
            r.mark = Some(InsDelRevision {
                info,
                kind: InsDel::Deletion,
            });
            self.put(
                rw,
                Op::SetParagraphRevision {
                    para,
                    revision: Box::new(r),
                },
            )?;
        }
        Ok(())
    }

    /// Marks the blocks of a list deleted (paragraphs and table rows).
    fn tracked_delete_blocks(&mut self, rw: &mut Rewrite, blocks: &[Block]) -> Result<()> {
        for b in blocks {
            match b {
                Block::Paragraph(p) => self.tracked_delete_paragraph(rw, p.id)?,
                Block::Table(t) => {
                    for row in &t.rows {
                        self.tracked_delete_row(rw, row.id)?;
                    }
                }
                Block::Sdt(s) => self.tracked_delete_blocks(rw, &s.blocks)?,
                Block::Field(f) => self.tracked_delete_blocks(rw, &f.blocks)?,
                Block::Opaque(_) => {}
            }
        }
        Ok(())
    }

    fn tracked_delete_row(&mut self, rw: &mut Rewrite, row: NodeId) -> Result<()> {
        let Some((_, r)) = self.doc.row(row) else {
            return Ok(());
        };
        let props = r.props.clone();
        let cells: Vec<Vec<Block>> = r.cells.iter().map(|c| c.blocks.clone()).collect();
        let already = r.revision.as_ref().map(|x| x.kind) == Some(InsDel::Deletion);
        if !already {
            let info = self.new_revision();
            self.put(
                rw,
                Op::SetRow {
                    row,
                    props: Box::new(props),
                    revision: Some(InsDelRevision {
                        info,
                        kind: InsDel::Deletion,
                    }),
                },
            )?;
        }
        for blocks in cells {
            let mut paras = Vec::new();
            super::paragraphs_of(&blocks, &mut paras);
            let ids: Vec<(NodeId, usize)> = paras.iter().map(|p| (p.id, p.content.len())).collect();
            for (id, len) in ids {
                self.tracked_delete(rw, id, 0, len)?;
            }
        }
        Ok(())
    }

    /// Marks inserted blocks (their text, paragraph marks and table rows) as insertions.
    fn mark_inserted_blocks(&mut self, rw: &mut Rewrite, blocks: &[Block]) -> Result<()> {
        let info = self.new_revision();
        let mut paras = Vec::new();
        super::paragraphs_of(blocks, &mut paras);
        let ids: Vec<(NodeId, usize, ParagraphRevision)> = paras
            .iter()
            .map(|p| (p.id, p.content.len(), p.revision.clone()))
            .collect();
        for (id, len, rev) in ids {
            rw.created.push(id);
            if len > 0 {
                self.put(
                    rw,
                    Op::AddMark {
                        para: id,
                        start: 0,
                        end: len,
                        mark: Mark::Insertion(info.clone()),
                    },
                )?;
            }
            self.put(
                rw,
                Op::SetParagraphRevision {
                    para: id,
                    revision: Box::new(ParagraphRevision {
                        mark: Some(InsDelRevision {
                            info: info.clone(),
                            kind: InsDel::Insertion,
                        }),
                        ..rev
                    }),
                },
            )?;
        }
        for b in blocks {
            if let Block::Table(t) = b {
                for row in &t.rows {
                    self.put(
                        rw,
                        Op::SetRow {
                            row: row.id,
                            props: Box::new(row.props.clone()),
                            revision: Some(InsDelRevision {
                                info: info.clone(),
                                kind: InsDel::Insertion,
                            }),
                        },
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Maps a block index of a container through the blocks the transaction removed but kept.
    fn map_index(
        map: &HashMap<NodeId, Vec<(usize, usize)>>,
        container: NodeId,
        index: usize,
    ) -> usize {
        let mut real = index;
        if let Some(kept) = map.get(&container) {
            let mut k = kept.clone();
            k.sort_unstable();
            for (s, n) in k {
                if s <= real {
                    real += n;
                }
            }
        }
        real
    }

    /// Records formatting changes for `start..end` of a paragraph before `runs` replace them.
    fn record_run_formats(
        &mut self,
        rw: &mut Rewrite,
        para: NodeId,
        start: usize,
        end: usize,
        new: &RunProps,
    ) -> Result<()> {
        if rw.created.contains(&para) {
            return Ok(());
        }
        let p = self.paragraph(para)?;
        let me = self.features.review.author.clone();
        // Chars already carrying a formatting change keep its "before"; the reviewer's own
        // insertions are not formatting changes.
        let mut skip = vec![false; end.saturating_sub(start)];
        for m in p.content.marks() {
            let hit = match &m.mark {
                Mark::FormatChange { .. } => true,
                Mark::Insertion(i) => i.author == me,
                _ => false,
            };
            if !hit {
                continue;
            }
            for i in m.start.max(start)..m.end.min(end) {
                skip[i - start] = true;
            }
        }
        let mut pieces: Vec<(usize, usize, RunProps)> = Vec::new();
        let mut at = 0usize;
        for r in p.content.runs() {
            let (rs, re) = (at, at + r.len);
            at = re;
            let (a, b) = (rs.max(start), re.min(end));
            if a >= b || r.props == *new {
                continue;
            }
            let mut i = a;
            while i < b {
                if skip[i - start] {
                    i += 1;
                    continue;
                }
                let s = i;
                while i < b && !skip[i - start] {
                    i += 1;
                }
                pieces.push((s, i, r.props.clone()));
            }
        }
        if pieces.is_empty() {
            return Ok(());
        }
        let info = self.new_revision();
        for (a, b, before) in pieces {
            self.put(
                rw,
                Op::AddMark {
                    para,
                    start: a,
                    end: b,
                    mark: Mark::FormatChange {
                        info: info.clone(),
                        before: Box::new(before),
                    },
                },
            )?;
        }
        Ok(())
    }

    /// The chars of `start..start + len` (transaction offsets) that are not ghosts, as document
    /// ranges.
    fn real_segments(
        rw: &Rewrite,
        para: NodeId,
        start: usize,
        len: usize,
    ) -> Vec<(NodeId, usize, usize)> {
        let mut out: Vec<(NodeId, usize, usize)> = Vec::new();
        for o in start..start + len {
            let (p, r) = Self::map_pos(rw, para, o, true);
            match out.last_mut() {
                Some((lp, _, e)) if *lp == p && *e == r => *e = r + 1,
                _ => out.push((p, r, r + 1)),
            }
        }
        out
    }

    /// Rewrites and applies one op of a tracked transaction.
    #[allow(clippy::too_many_lines)]
    fn track_op(&mut self, rw: &mut Rewrite, op: Op) -> Result<()> {
        match op {
            Op::InsertInline {
                para,
                offset,
                fragment,
            } => {
                let (p, at) = Self::map_pos(rw, para, offset, true);
                let len = fragment.len();
                if self.untracked_story(p) || len == 0 {
                    self.put(
                        rw,
                        Op::InsertInline {
                            para: p,
                            offset: at,
                            fragment,
                        },
                    )?;
                } else {
                    let info = self
                        .own_insertion_near(p, at)
                        .unwrap_or_else(|| self.new_revision());
                    self.put(
                        rw,
                        Op::InsertInline {
                            para: p,
                            offset: at,
                            fragment: fragment.with_mark(Mark::Insertion(info)),
                        },
                    )?;
                }
                #[allow(clippy::cast_possible_wrap)]
                Self::shift_ghosts(rw, p, at, len as isize);
            }
            Op::DeleteInline { para, offset, len } => {
                let (p, s) = Self::map_pos(rw, para, offset, true);
                let (p2, e) = Self::map_pos(rw, para, offset + len, false);
                if self.untracked_story(p) {
                    let n = if p2 == p { e.saturating_sub(s) } else { len };
                    self.put(
                        rw,
                        Op::DeleteInline {
                            para: p,
                            offset: s,
                            len: n,
                        },
                    )?;
                    #[allow(clippy::cast_possible_wrap)]
                    Self::shift_ghosts(rw, p, s, -(n as isize));
                } else if p2 == p {
                    self.tracked_delete(rw, p, s, e)?;
                } else {
                    // Across a join the transaction did not see: delete to the end of the first
                    // paragraph and from the start of the next.
                    let first_len = self.paragraph(p)?.content.len();
                    self.tracked_delete(rw, p, s, first_len)?;
                    self.tracked_delete(rw, p2, 0, e)?;
                }
            }
            Op::SetRuns { para, offset, runs } => {
                if self.untracked_story(para) {
                    let (p, s) = Self::map_pos(rw, para, offset, true);
                    return self.put(
                        rw,
                        Op::SetRuns {
                            para: p,
                            offset: s,
                            runs,
                        },
                    );
                }
                let mut o = offset;
                for r in runs {
                    for (p, a, b) in Self::real_segments(rw, para, o, r.len) {
                        self.record_run_formats(rw, p, a, b, &r.props)?;
                        self.put(
                            rw,
                            Op::SetRuns {
                                para: p,
                                offset: a,
                                runs: vec![RunSpan::new(b - a, r.props.clone())],
                            },
                        )?;
                    }
                    o += r.len;
                }
            }
            Op::AddMark {
                para,
                start,
                end,
                mark,
            } => {
                let (p, s) = Self::map_pos(rw, para, start, true);
                let (p2, e) = Self::map_pos(rw, para, end, false);
                let e = if p2 == p {
                    e
                } else {
                    self.paragraph(p)?.content.len()
                };
                if s < e {
                    self.put(
                        rw,
                        Op::AddMark {
                            para: p,
                            start: s,
                            end: e,
                            mark,
                        },
                    )?;
                }
            }
            Op::RemoveMark {
                para,
                start,
                end,
                mark,
            } => {
                let (p, s) = Self::map_pos(rw, para, start, true);
                let (p2, e) = Self::map_pos(rw, para, end, false);
                let e = if p2 == p {
                    e
                } else {
                    self.paragraph(p)?.content.len()
                };
                self.put(
                    rw,
                    Op::RemoveMark {
                        para: p,
                        start: s,
                        end: e,
                        mark,
                    },
                )?;
            }
            Op::SetObject {
                para,
                offset,
                object,
            } => {
                let (p, s) = Self::map_pos(rw, para, offset, true);
                self.put(
                    rw,
                    Op::SetObject {
                        para: p,
                        offset: s,
                        object,
                    },
                )?;
            }
            Op::SetParagraphProps { para, props } => {
                let p = self.paragraph(para)?;
                let tracked = !rw.created.contains(&para)
                    && !self.untracked_story(para)
                    && p.revision.props_change.is_none()
                    && p.props != *props
                    && p.revision.mark.as_ref().is_none_or(|m| {
                        !(m.kind == InsDel::Insertion
                            && m.info.author == self.features.review.author)
                    });
                if tracked {
                    let before = p.props.clone();
                    let mut r = p.revision.clone();
                    let info = self.new_revision();
                    r.props_change = Some(Box::new(ParagraphPropsChange { info, before }));
                    self.put(
                        rw,
                        Op::SetParagraphRevision {
                            para,
                            revision: Box::new(r),
                        },
                    )?;
                }
                self.put(rw, Op::SetParagraphProps { para, props })?;
            }
            Op::SetParagraphMarkProps { para, props } => {
                let p = self.paragraph(para)?;
                if !rw.created.contains(&para)
                    && !self.untracked_story(para)
                    && p.revision.mark_props_change.is_none()
                    && p.mark_props != *props
                {
                    let before = p.mark_props.clone();
                    let mut r = p.revision.clone();
                    let info = self.new_revision();
                    r.mark_props_change = Some(Box::new(RunPropsChange { info, before }));
                    self.put(
                        rw,
                        Op::SetParagraphRevision {
                            para,
                            revision: Box::new(r),
                        },
                    )?;
                }
                self.put(rw, Op::SetParagraphMarkProps { para, props })?;
            }
            Op::SplitParagraph {
                para,
                offset,
                new_para,
                props,
                mark_props,
                revision: _,
            } => {
                let (p, at) = Self::map_pos(rw, para, offset, true);
                let old = self.paragraph(p)?.revision.clone();
                let untracked = self.untracked_story(p);
                // The new paragraph ends with the original mark; the split paragraph's mark is
                // the new one.
                let tail_revision = ParagraphRevision {
                    props_change: None,
                    mark: old.mark.clone(),
                    mark_props_change: old.mark_props_change.clone(),
                };
                self.put(
                    rw,
                    Op::SplitParagraph {
                        para: p,
                        offset: at,
                        new_para,
                        props,
                        mark_props,
                        revision: Box::new(if untracked {
                            ParagraphRevision::default()
                        } else {
                            tail_revision
                        }),
                    },
                )?;
                rw.created.push(new_para);
                if !untracked {
                    let info = self
                        .own_insertion_near(p, at)
                        .unwrap_or_else(|| self.new_revision());
                    self.put(
                        rw,
                        Op::SetParagraphRevision {
                            para: p,
                            revision: Box::new(ParagraphRevision {
                                props_change: old.props_change,
                                mark: Some(InsDelRevision {
                                    info,
                                    kind: InsDel::Insertion,
                                }),
                                mark_props_change: None,
                            }),
                        },
                    )?;
                }
                // Ghosts after the split move to the new paragraph.
                if let Some(g) = rw.ghosts.get_mut(&p) {
                    let moved: Ghosts = g
                        .iter()
                        .filter(|(s, _)| *s >= at)
                        .map(|&(s, l)| (s - at, l))
                        .collect();
                    g.retain(|(s, _)| *s < at);
                    if !moved.is_empty() {
                        rw.ghosts.insert(new_para, moved);
                    }
                }
                // A join recorded on the split paragraph now belongs to its tail.
                if let Some((second, len)) = rw.joined.remove(&p) {
                    rw.joined
                        .insert(new_para, (second, len.saturating_sub(offset)));
                }
            }
            Op::MergeParagraphs { first, second } => {
                let p = self.paragraph(first)?;
                let me = self.features.review.author.clone();
                let own_mark = p
                    .revision
                    .mark
                    .as_ref()
                    .is_some_and(|m| m.kind == InsDel::Insertion && m.info.author == me);
                if own_mark || self.untracked_story(first) {
                    // The reviewer's own new paragraph mark really goes.
                    let first_len = p.content.len();
                    let moved = rw.ghosts.remove(&second);
                    self.put(rw, Op::MergeParagraphs { first, second })?;
                    if let Some(g) = moved {
                        for (s, l) in g {
                            Self::add_ghost(rw, first, s + first_len, l);
                        }
                    }
                } else {
                    let ghost_len: usize = rw
                        .ghosts
                        .get(&first)
                        .map_or(0, |g| g.iter().map(|(_, l)| *l).sum());
                    let seen_len = p.content.len().saturating_sub(ghost_len);
                    if p.revision.mark.as_ref().map(|m| m.kind) != Some(InsDel::Deletion) {
                        let mut r = p.revision.clone();
                        let info = self
                            .own_deletion_near(
                                first,
                                p.content.len().saturating_sub(1),
                                p.content.len(),
                            )
                            .unwrap_or_else(|| self.new_revision());
                        r.mark = Some(InsDelRevision {
                            info,
                            kind: InsDel::Deletion,
                        });
                        self.put(
                            rw,
                            Op::SetParagraphRevision {
                                para: first,
                                revision: Box::new(r),
                            },
                        )?;
                    }
                    rw.joined.insert(first, (second, seen_len));
                }
            }
            Op::RemoveBlocks {
                container,
                index,
                count,
            } => {
                let real = Self::map_index(&rw.kept_blocks, container, index);
                let blocks: Vec<Block> = self
                    .doc
                    .container(container)
                    .map_err(doc_err)?
                    .iter()
                    .skip(real)
                    .take(count)
                    .cloned()
                    .collect();
                self.tracked_delete_blocks(rw, &blocks)?;
                rw.kept_blocks
                    .entry(container)
                    .or_default()
                    .push((real, blocks.len()));
            }
            Op::InsertBlocks {
                container,
                index,
                blocks,
            } => {
                let real = Self::map_index(&rw.kept_blocks, container, index);
                let copy = blocks.clone();
                self.put(
                    rw,
                    Op::InsertBlocks {
                        container,
                        index: real,
                        blocks,
                    },
                )?;
                let tracked = self
                    .doc
                    .container(container)
                    .ok()
                    .and_then(|c| c.first())
                    .is_none_or(|b| !self.untracked_story(b.id()));
                if tracked {
                    self.mark_inserted_blocks(rw, &copy)?;
                }
            }
            Op::InsertRows { table, index, rows } => {
                let real = Self::map_index(&rw.kept_rows, table, index);
                let ids: Vec<(NodeId, kiln_doc::props::TableRowProps)> =
                    rows.iter().map(|r| (r.id, r.props.clone())).collect();
                self.put(
                    rw,
                    Op::InsertRows {
                        table,
                        index: real,
                        rows,
                    },
                )?;
                let info = self.new_revision();
                for (row, props) in ids {
                    self.put(
                        rw,
                        Op::SetRow {
                            row,
                            props: Box::new(props),
                            revision: Some(InsDelRevision {
                                info: info.clone(),
                                kind: InsDel::Insertion,
                            }),
                        },
                    )?;
                }
            }
            Op::RemoveRows {
                table,
                index,
                count,
            } => {
                let real = Self::map_index(&rw.kept_rows, table, index);
                let rows: Vec<NodeId> = self
                    .doc
                    .table(table)
                    .map(|t| t.rows.iter().skip(real).take(count).map(|r| r.id).collect())
                    .unwrap_or_default();
                for r in &rows {
                    self.tracked_delete_row(rw, *r)?;
                }
                rw.kept_rows
                    .entry(table)
                    .or_default()
                    .push((real, rows.len()));
            }
            other => self.put(rw, other)?,
        }
        Ok(())
    }

    /// Rewrites a transaction as tracked changes and applies it; returns the applied forward
    /// transaction (with its selection mapped) and its inverse. Nothing is applied on error.
    pub(in crate::apps::write) fn apply_tracked_parts(
        &mut self,
        tx: Transaction,
    ) -> Result<(Transaction, Transaction)> {
        let Transaction {
            ops,
            kind,
            selection_before,
            selection_after,
        } = tx;
        let mut rw = Rewrite::default();
        for op in ops {
            if let Err(e) = self.track_op(&mut rw, op) {
                for inv in rw.inverses.into_iter().rev() {
                    let _ = self.doc.apply(inv);
                }
                return Err(e);
            }
        }
        // Backspace leaves the caret before the text it struck through; Delete and replacing
        // a selection leave it after.
        let backspace = kind == EditKind::Delete
            && selection_before.is_some_and(|b| b.anchor == b.head)
            && selection_before != selection_after;
        let map = |p: Pos| {
            let (para, off) = Self::map_pos(&rw, p.para, p.offset, !backspace);
            Pos::new(para, off)
        };
        let after = selection_after.map(|r| Range::new(map(r.anchor), map(r.head)));
        let forward = Transaction::new(rw.forward)
            .with_kind(kind)
            .with_selection(selection_before, after);
        let mut inverse_ops = Vec::new();
        for inv in rw.inverses.into_iter().rev() {
            inverse_ops.extend(inv.ops);
        }
        let inverse = Transaction::new(inverse_ops)
            .with_kind(kind)
            .with_selection(after, selection_before);
        self.touched.record(&self.doc, &forward);
        self.note_edit(&forward);
        Ok((forward, inverse))
    }

    /// Applies a transaction with tracking as one undo step.
    pub(in crate::apps::write) fn apply_tracked(&mut self, tx: Transaction) -> Result<()> {
        let (forward, inverse) = self.apply_tracked_parts(tx)?;
        let after = forward.selection_after;
        let before_len = self.history.undo_len();
        self.history.record(forward, inverse);
        self.bump_state(self.history.undo_len() > before_len);
        if let Some(r) = after {
            self.anchor = r.anchor;
            self.head = r.head;
        }
        self.goal_x = None;
        Ok(())
    }

    // ── Reviewing ───────────────────────────────────────────────────────────────────────────

    /// Show Markup: one kind on or off.
    pub(super) fn toggle_show_markup(&mut self, kind: MarkupKind) {
        let r = &mut self.features.review;
        match kind {
            MarkupKind::Comments => r.show_comments = !r.show_comments,
            MarkupKind::InsertionsDeletions => r.show_insertions = !r.show_insertions,
            MarkupKind::Formatting => r.show_formatting = !r.show_formatting,
        }
    }

    /// The layout options for the review display.
    pub(in crate::apps::write) fn review_options(
        &self,
    ) -> (bool, kiln_doc_layout::cx::ReviewDisplay) {
        let r = &self.features.review;
        (
            r.markup == MarkupMode::All,
            kiln_doc_layout::cx::ReviewDisplay {
                original: r.markup == MarkupMode::Original,
                revisions: r.show_insertions,
                comments: r.show_comments
                    && matches!(r.markup, MarkupMode::All | MarkupMode::Simple),
                // Simple Markup shows comments in balloons too (Word's Show Comments).
                balloons: r.markup == MarkupMode::Simple,
            },
        )
    }

    /// Brings the layout's options in line with the review display (a new layout when they
    /// change).
    pub(in crate::apps::write) fn sync_review_display(&mut self) {
        let (markup, review) = self.review_options();
        if let Some(v) = &mut self.view {
            let mut o = v.layout.options().clone();
            if o.markup != markup || o.review != review {
                o.markup = markup;
                o.review = review;
                v.layout.set_options(o);
            }
        }
    }

    /// Body revisions in document order (text changes by position).
    fn body_revisions(&self) -> Vec<RevisionEntry> {
        let mut all: Vec<RevisionEntry> = self
            .doc
            .revisions()
            .into_iter()
            .filter(|r| {
                let node = match r.location {
                    RevisionLocation::Text { para, .. } | RevisionLocation::Paragraph(para) => para,
                    RevisionLocation::Row(row) => row,
                };
                !matches!(
                    self.doc.story_of(node),
                    Some(Story::HeaderFooter(_) | Story::Comment(_))
                )
            })
            .collect();
        all.sort_by(|a, b| self.compare_entries(a, b));
        all
    }

    fn entry_pos(&self, e: &RevisionEntry) -> Pos {
        match e.location {
            RevisionLocation::Text { para, start, .. } => Pos::new(para, start),
            RevisionLocation::Paragraph(p) => {
                Pos::new(p, self.doc.paragraph(p).map_or(0, |x| x.content.len()))
            }
            RevisionLocation::Row(row) => {
                let first = self.doc.row(row).and_then(|(_, r)| {
                    let mut ps = Vec::new();
                    for c in &r.cells {
                        super::paragraphs_of(&c.blocks, &mut ps);
                    }
                    ps.first().map(|p| p.id)
                });
                Pos::new(first.unwrap_or(row), 0)
            }
        }
    }

    fn compare_entries(&self, a: &RevisionEntry, b: &RevisionEntry) -> Ordering {
        let (pa, pb) = (self.entry_pos(a), self.entry_pos(b));
        self.doc.compare(pa, pb).unwrap_or(Ordering::Equal)
    }

    /// Whether a revision is at the caret or in the selection.
    fn in_selection(&self, e: &RevisionEntry) -> bool {
        let (s, t) = self.span();
        let le = |a: Pos, b: Pos| {
            self.doc
                .compare(a, b)
                .is_some_and(|o| o != Ordering::Greater)
        };
        match e.location {
            RevisionLocation::Text { para, start, end } => {
                let (a, b) = (Pos::new(para, start), Pos::new(para, end));
                if s == t {
                    para == s.para && start <= s.offset && s.offset <= end
                } else {
                    le(s, b) && le(a, t) && !(b == s) && !(a == t)
                }
            }
            RevisionLocation::Paragraph(p) => {
                let len = self.doc.paragraph(p).map_or(0, |x| x.content.len());
                if s == t {
                    p == s.para
                } else {
                    le(s, Pos::new(p, len)) && le(Pos::new(p, 0), t)
                }
            }
            RevisionLocation::Row(_) => {
                let pos = self.entry_pos(e);
                if s == t {
                    match (
                        self.doc.enclosing_cell(s.para),
                        self.doc.enclosing_cell(pos.para),
                    ) {
                        (Some((t1, r1, _)), Some((t2, r2, _))) => t1.id == t2.id && r1 == r2,
                        _ => false,
                    }
                } else {
                    le(s, pos) && le(pos, t)
                }
            }
        }
    }

    fn shown(&self, k: RevisionKind) -> bool {
        let r = &self.features.review;
        if is_format(k) {
            r.show_formatting
        } else {
            r.show_insertions
        }
    }

    /// Accept or Reject.
    pub(super) fn resolve_changes(&mut self, scope: ChangeScope, accept: bool) -> Result<()> {
        let all = self.doc.revisions();
        let targets: Vec<RevisionEntry> = match scope {
            ChangeScope::All | ChangeScope::AllAndStop => all,
            ChangeScope::AllShown => all.into_iter().filter(|e| self.shown(e.kind)).collect(),
            ChangeScope::Current | ChangeScope::CurrentAndNext => {
                all.into_iter().filter(|e| self.in_selection(e)).collect()
            }
        };
        if targets.is_empty() {
            if scope == ChangeScope::CurrentAndNext {
                return self.next_change(false);
            }
            if scope == ChangeScope::AllAndStop && self.doc.settings.track_revisions {
                self.toggle_tracking();
            }
            return Ok(());
        }
        let caret = self.span().0;
        self.resolve_entries(&targets, accept)?;
        if scope == ChangeScope::AllAndStop && self.doc.settings.track_revisions {
            self.toggle_tracking();
        }
        if scope == ChangeScope::CurrentAndNext {
            let here = self.doc.clamp(caret).unwrap_or(caret);
            self.set_selection(here, here);
            let _ = self.next_change_from(here, false, true);
        }
        Ok(())
    }

    /// Accept or reject one change of the Reviewing pane.
    pub(super) fn resolve_one(&mut self, target: &RevisionRef, accept: bool) -> Result<()> {
        let found: Vec<RevisionEntry> = self
            .doc
            .revisions()
            .into_iter()
            .filter(|e| revision_ref(e) == *target)
            .collect();
        if found.is_empty() {
            return Err(KilnError::InvalidInput(
                "that change is no longer in the document".into(),
            ));
        }
        self.resolve_entries(&found, accept)
    }

    /// Resolves revisions as one undo step, without tracking.
    fn resolve_entries(&mut self, entries: &[RevisionEntry], accept: bool) -> Result<()> {
        let targets: Vec<Target> = entries
            .iter()
            .map(|e| Target {
                kind: e.kind,
                info: e.info.clone(),
                location: e.location,
            })
            .collect();
        self.features.untracked = true;
        let r = self.resolve_targets(&targets, accept);
        self.features.untracked = false;
        r
    }

    #[allow(clippy::too_many_lines)]
    fn resolve_targets(&mut self, targets: &[Target], accept: bool) -> Result<()> {
        let before = self.selection_range();
        let mut batch = self.batch(EditKind::Other);
        // 1. Text changes, paragraph by paragraph, from the end of each paragraph.
        let mut by_para: Vec<(NodeId, Vec<&Target>)> = Vec::new();
        for t in targets {
            if let RevisionLocation::Text { para, .. } = t.location {
                match by_para.iter_mut().find(|(p, _)| *p == para) {
                    Some((_, v)) => v.push(t),
                    None => by_para.push((para, vec![t])),
                }
            }
        }
        for (para, ts) in by_para {
            let mut guard = 0usize;
            loop {
                guard += 1;
                let Some(p) = self.doc.paragraph(para) else {
                    break;
                };
                // The last mark of a wanted change (identity: kind + record).
                let next = p
                    .content
                    .marks()
                    .iter()
                    .filter(|m| {
                        ts.iter().any(|t| {
                            mark_matches(&m.mark, t.kind) && m.mark.revision() == Some(&t.info)
                        })
                    })
                    .max_by_key(|m| (m.start, m.end))
                    .map(|m| (m.start, m.end, m.mark.clone()));
                let Some((start, end, mark)) = next else {
                    break;
                };
                if guard > 100_000 {
                    break;
                }
                let kind = ts
                    .iter()
                    .find(|t| mark_matches(&mark, t.kind) && mark.revision() == Some(&t.info))
                    .map_or(RevisionKind::Insertion, |t| t.kind);
                let tx = edit::resolve_revision(
                    &self.doc,
                    kind,
                    RevisionLocation::Text { para, start, end },
                    accept,
                )
                .map_err(doc_err)?;
                self.batch_apply(&mut batch, tx)?;
            }
        }
        // 2. Paragraph changes, from the end of the document (joins pull the next paragraph in).
        let mut paras: Vec<&Target> = targets
            .iter()
            .filter(|t| matches!(t.location, RevisionLocation::Paragraph(_)))
            .collect();
        paras.sort_by(|a, b| {
            let pa = match a.location {
                RevisionLocation::Paragraph(p) => Pos::new(p, 0),
                _ => Pos::new(NodeId(0), 0),
            };
            let pb = match b.location {
                RevisionLocation::Paragraph(p) => Pos::new(p, 0),
                _ => Pos::new(NodeId(0), 0),
            };
            self.doc.compare(pb, pa).unwrap_or(Ordering::Equal)
        });
        for t in paras {
            let RevisionLocation::Paragraph(id) = t.location else {
                continue;
            };
            let Some(p) = self.doc.paragraph(id) else {
                continue;
            };
            let present = match t.kind {
                RevisionKind::ParagraphFormat => p.revision.props_change.is_some(),
                RevisionKind::ParagraphMarkFormat => p.revision.mark_props_change.is_some(),
                RevisionKind::ParagraphInsertion | RevisionKind::ParagraphDeletion => {
                    p.revision.mark.is_some()
                }
                _ => false,
            };
            if !present {
                continue;
            }
            let tx =
                edit::resolve_revision(&self.doc, t.kind, t.location, accept).map_err(doc_err)?;
            self.batch_apply(&mut batch, tx)?;
        }
        // 3. Rows.
        for t in targets {
            let RevisionLocation::Row(row) = t.location else {
                continue;
            };
            let Some((table, r)) = self.doc.row(row) else {
                continue;
            };
            let Some(rev) = &r.revision else { continue };
            let removes = match rev.kind {
                InsDel::Insertion => !accept,
                InsDel::Deletion => accept,
            };
            let table_id = table.id;
            let index = table.rows.iter().position(|x| x.id == row).unwrap_or(0);
            let tx = if removes && table.rows.len() > 1 {
                Transaction::new(vec![Op::RemoveRows {
                    table: table_id,
                    index,
                    count: 1,
                }])
            } else if removes {
                // The last row: the table goes.
                let (container, at) = self
                    .doc
                    .parent_of(table_id)
                    .ok_or(KilnError::InvalidInput("table without a parent".into()))?;
                Transaction::new(vec![Op::RemoveBlocks {
                    container,
                    index: at,
                    count: 1,
                }])
            } else {
                Transaction::new(vec![Op::SetRow {
                    row,
                    props: Box::new(r.props.clone()),
                    revision: None,
                }])
            };
            self.batch_apply(&mut batch, tx)?;
        }
        let a = self.doc.clamp(before.anchor).unwrap_or(self.doc_start());
        let h = self.doc.clamp(before.head).unwrap_or(a);
        let after = if self.doc.paragraph(a.para).is_some() && self.doc.paragraph(h.para).is_some()
        {
            Range::new(a, h)
        } else {
            Range::caret(self.doc_start())
        };
        self.batch_commit(batch, after);
        Ok(())
    }

    /// Previous or Next change.
    pub(super) fn next_change(&mut self, backward: bool) -> Result<()> {
        let (s, e) = self.span();
        let from = if backward { s } else { e };
        if self.next_change_from(from, backward, false)? {
            Ok(())
        } else {
            Err(KilnError::InvalidInput(
                "the document has no tracked changes or comments".into(),
            ))
        }
    }

    /// Selects the change (or comment) after or before `from`, wrapping around; `true` when one
    /// was found.
    fn next_change_from(&mut self, from: Pos, backward: bool, inclusive: bool) -> Result<bool> {
        let mut stops: Vec<(Pos, Pos)> = self
            .body_revisions()
            .iter()
            .map(|e| match e.location {
                RevisionLocation::Text { para, start, end } => {
                    (Pos::new(para, start), Pos::new(para, end))
                }
                _ => {
                    let p = self.entry_pos(e);
                    let len = self.doc.paragraph(p.para).map_or(0, |x| x.content.len());
                    (Pos::new(p.para, 0), Pos::new(p.para, len))
                }
            })
            .collect();
        stops.extend(self.comment_ranges().into_iter().map(|(_, a, b)| (a, b)));
        stops.sort_by(|a, b| self.doc.compare(a.0, b.0).unwrap_or(Ordering::Equal));
        stops.dedup();
        if stops.is_empty() {
            return Ok(false);
        }
        let cmp = |a: Pos| self.doc.compare(a, from).unwrap_or(Ordering::Equal);
        let pick = if backward {
            stops
                .iter()
                .rev()
                .find(|(a, _)| cmp(*a) == Ordering::Less)
                .or_else(|| stops.last())
        } else {
            stops
                .iter()
                .find(|(a, _)| {
                    let o = cmp(*a);
                    o == Ordering::Greater || (inclusive && o == Ordering::Equal)
                })
                .or_else(|| stops.first())
        };
        if let Some(&(a, b)) = pick {
            self.set_selection(a, b);
            return Ok(true);
        }
        Ok(false)
    }

    /// The Reviewing pane.
    pub(super) fn revisions_info(&self) -> RevisionsInfo {
        let mut entries = Vec::new();
        let mut reviewers: Vec<ReviewerInfo> = Vec::new();
        let (mut ins, mut del, mut fmt) = (0u32, 0u32, 0u32);
        let mut all = self.doc.revisions();
        all.sort_by(|a, b| self.compare_entries(a, b));
        for e in &all {
            match e.kind {
                RevisionKind::Insertion
                | RevisionKind::MoveTo
                | RevisionKind::ParagraphInsertion
                | RevisionKind::RowInsertion => ins += 1,
                RevisionKind::Deletion
                | RevisionKind::MoveFrom
                | RevisionKind::ParagraphDeletion
                | RevisionKind::RowDeletion => del += 1,
                _ => fmt += 1,
            }
            let color = annotations::author_hex(&e.info.author);
            match reviewers.iter_mut().find(|r| r.name == e.info.author) {
                Some(r) => r.changes += 1,
                None => reviewers.push(ReviewerInfo {
                    name: e.info.author.clone(),
                    color: color.clone(),
                    changes: 1,
                }),
            }
            let text = match (e.kind, e.location) {
                (RevisionKind::RunFormat, RevisionLocation::Text { para, start, .. }) => self
                    .doc
                    .paragraph(para)
                    .and_then(|p| {
                        let now = p.content.props_at(start)?.clone();
                        p.content.marks().iter().find_map(|m| match &m.mark {
                            Mark::FormatChange { info, before }
                                if *info == e.info && m.start <= start && start < m.end =>
                            {
                                Some(describe_runs(before, &now))
                            }
                            _ => None,
                        })
                    })
                    .unwrap_or_else(|| "Formatted".to_owned()),
                (RevisionKind::ParagraphFormat, RevisionLocation::Paragraph(p)) => self
                    .doc
                    .paragraph(p)
                    .and_then(|x| {
                        x.revision
                            .props_change
                            .as_ref()
                            .map(|c| describe_paragraph(&c.before, &x.props))
                    })
                    .unwrap_or_else(|| "Formatted paragraph".to_owned()),
                (RevisionKind::ParagraphMarkFormat, _) => "Formatted paragraph mark".to_owned(),
                (RevisionKind::ParagraphInsertion, _) => "Inserted paragraph".to_owned(),
                (RevisionKind::ParagraphDeletion, _) => "Deleted paragraph mark".to_owned(),
                (RevisionKind::RowInsertion, _) => "Inserted table row".to_owned(),
                (RevisionKind::RowDeletion, _) => "Deleted table row".to_owned(),
                _ => e.text.clone().unwrap_or_default(),
            };
            let node = match e.location {
                RevisionLocation::Text { para, .. } | RevisionLocation::Paragraph(para) => para,
                RevisionLocation::Row(row) => self.entry_pos(e).para.max(row),
            };
            entries.push(RevisionEntryInfo {
                target: revision_ref(e),
                author: e.info.author.clone(),
                date: e.info.date.clone(),
                text,
                color,
                story: story_label(&self.doc, node),
                current: self.in_selection(e),
            });
        }
        RevisionsInfo {
            entries,
            reviewers,
            insertions: ins,
            deletions: del,
            formatting: fmt,
            comments: u32::try_from(self.doc.comments.len()).unwrap_or(u32::MAX),
        }
    }

    /// Change bars (Simple and All Markup) and the current comment's connector (All Markup).
    pub(super) fn review_overlay(&self, v: &WriteView) -> Vec<DrawOp> {
        let r = &self.features.review;
        if !matches!(r.markup, MarkupMode::Simple | MarkupMode::All) {
            return Vec::new();
        }
        let (first, last) = v.visible_pages();
        let pages = v.layout.pages();
        let visible = |para: NodeId| {
            v.layout
                .lines_of(para)
                .iter()
                .any(|l| (l.page as usize) >= first && (l.page as usize) <= last)
        };
        let mut ranges: Vec<(Pos, Pos)> = Vec::new();
        for e in self.revisions_cached().iter() {
            if !self.shown(e.kind) {
                continue;
            }
            match e.location {
                RevisionLocation::Text { para, start, end } if visible(para) => {
                    ranges.push((Pos::new(para, start), Pos::new(para, end)));
                }
                RevisionLocation::Paragraph(p) if visible(p) => {
                    let len = self.doc.paragraph(p).map_or(0, |x| x.content.len());
                    ranges.push((Pos::new(p, 0), Pos::new(p, len)));
                }
                _ => {}
            }
        }
        let color = if r.markup == MarkupMode::Simple {
            annotations::SIMPLE_BAR
        } else {
            annotations::ALL_BAR
        };
        let mut ops = annotations::change_bars(&v.layout, &self.doc, &ranges, color);
        if r.show_comments {
            let (s, _) = self.span();
            if let Some(c) = self.comment_at(s)
                && let Some(b) = self
                    .balloon_placements(v)
                    .into_iter()
                    .find(|b| b.comment == c)
            {
                let author = self
                    .doc
                    .comments
                    .iter()
                    .find(|x| x.id == c)
                    .map(|x| x.meta.author.clone())
                    .unwrap_or_default();
                let page = pages.get(b.page);
                let edge = page.map_or(b.rect.x, |p| p.origin.0 + p.width);
                ops.extend(annotations::connector(
                    b.anchor,
                    edge,
                    Bounds::new(b.rect.x, b.rect.y, b.rect.w, b.rect.h),
                    Paint::Rgba(annotations::author_color(&author)),
                ));
            }
        }
        ops
    }
}
