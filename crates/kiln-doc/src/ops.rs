// SPDX-License-Identifier: AGPL-3.0-or-later
//! Operations and transactions: the only way the model changes once it is built.
//!
//! An [`Op`] is one primitive change addressed by [`NodeId`]s and char offsets. Applying an op
//! validates it completely first (unknown ids, out-of-range offsets, duplicate ids, broken
//! invariants are errors and change nothing) and returns its **inverse**: the op that restores
//! the document exactly. A [`Transaction`] applies its ops in order and is atomic: if one fails,
//! the ones already applied are rolled back. Undo, collaboration (ops map to CRDT changes) and the
//! control channel all go through [`Document::apply`].

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::DocError;
use crate::inline::{Fragment, InlineObject, Mark, MarkSpan, RunSpan};
use crate::model::{
    Block, Cell, Comment, CommentMeta, Document, HeaderFooter, HeaderFooterPart, Note, Paragraph,
    Row, Section, check_blocks, ids_in_blocks,
};
use crate::numbering::{AbstractNum, NumInstance};
use crate::pos::Range;
use crate::props::{ParagraphProps, RunProps, TableCellProps, TableProps, TableRowProps};
use crate::revision::{InsDelRevision, ParagraphRevision};
use crate::section::{HeaderFooterRefs, SectionProps};
use crate::settings::{DocProperties, Media, Settings};
use crate::st::VMerge;
use crate::style::{DocDefaults, Style};
use crate::theme::DocTheme;
use kiln_common::NodeId;

/// One primitive change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Op {
    // ---- paragraph content
    /// Inserts content into a paragraph. Inverse: [`Op::DeleteInline`].
    InsertInline {
        /// The paragraph.
        para: NodeId,
        /// Char offset.
        offset: usize,
        /// What to insert (text, runs, marks, objects).
        fragment: Fragment,
    },
    /// Deletes `len` chars at `offset`. Inverse: [`Op::InsertInline`] of what was there.
    DeleteInline {
        /// The paragraph.
        para: NodeId,
        /// Char offset.
        offset: usize,
        /// Chars to delete.
        len: usize,
    },
    /// Replaces the direct formatting of `offset..offset + Σlen`. Inverse: the previous runs.
    SetRuns {
        /// The paragraph.
        para: NodeId,
        /// Char offset.
        offset: usize,
        /// The new runs.
        runs: Vec<RunSpan>,
    },
    /// Adds a mark. Inverse: [`Op::SetMarks`] with the previous marks.
    AddMark {
        /// The paragraph.
        para: NodeId,
        /// First char.
        start: usize,
        /// One past the last char.
        end: usize,
        /// The mark.
        mark: Mark,
    },
    /// Removes a mark (exact equality) from a range. Inverse: [`Op::SetMarks`].
    RemoveMark {
        /// The paragraph.
        para: NodeId,
        /// First char.
        start: usize,
        /// One past the last char.
        end: usize,
        /// The mark.
        mark: Mark,
    },
    /// Replaces all marks of a paragraph. Inverse: the previous marks.
    SetMarks {
        /// The paragraph.
        para: NodeId,
        /// The marks.
        marks: Vec<MarkSpan>,
    },
    /// Replaces the object at an object position (resize a picture, update a field result).
    SetObject {
        /// The paragraph.
        para: NodeId,
        /// Offset of the object's placeholder.
        offset: usize,
        /// The new object.
        object: InlineObject,
    },
    // ---- paragraphs
    /// Replaces a paragraph's direct properties.
    SetParagraphProps {
        /// The paragraph.
        para: NodeId,
        /// The properties.
        props: Box<ParagraphProps>,
    },
    /// Replaces the paragraph mark's run properties.
    SetParagraphMarkProps {
        /// The paragraph.
        para: NodeId,
        /// The properties.
        props: Box<RunProps>,
    },
    /// Replaces a paragraph's tracked changes.
    SetParagraphRevision {
        /// The paragraph.
        para: NodeId,
        /// The revision.
        revision: Box<ParagraphRevision>,
    },
    /// Splits a paragraph at `offset`; the tail becomes a new paragraph right after it.
    /// Inverse: [`Op::MergeParagraphs`].
    SplitParagraph {
        /// The paragraph.
        para: NodeId,
        /// Char offset.
        offset: usize,
        /// The new paragraph's id (from [`Document::alloc_id`]).
        new_para: NodeId,
        /// The new paragraph's properties.
        props: Box<ParagraphProps>,
        /// The new paragraph's mark properties.
        mark_props: Box<RunProps>,
        /// The new paragraph's tracked changes.
        revision: Box<ParagraphRevision>,
    },
    /// Appends `second`'s content to `first` and removes `second`; they must be neighbours in the
    /// same container. Inverse: [`Op::SplitParagraph`].
    MergeParagraphs {
        /// The paragraph that stays.
        first: NodeId,
        /// The paragraph that goes.
        second: NodeId,
    },
    // ---- blocks
    /// Inserts blocks into a container (section, cell, header/footer, note, comment, content
    /// control, field result, text box). Inverse: [`Op::RemoveBlocks`].
    InsertBlocks {
        /// The container.
        container: NodeId,
        /// Index in its block list.
        index: usize,
        /// The blocks (fresh ids).
        blocks: Vec<Block>,
    },
    /// Removes blocks from a container; at least one must remain. Inverse: [`Op::InsertBlocks`].
    RemoveBlocks {
        /// The container.
        container: NodeId,
        /// First index.
        index: usize,
        /// How many.
        count: usize,
    },
    // ---- tables
    /// Replaces a table's properties.
    SetTableProps {
        /// The table.
        table: NodeId,
        /// The properties.
        props: Box<TableProps>,
    },
    /// Replaces a table's grid.
    SetTableGrid {
        /// The table.
        table: NodeId,
        /// Column widths, twips.
        grid: Vec<i32>,
    },
    /// Inserts rows. Inverse: [`Op::RemoveRows`].
    InsertRows {
        /// The table.
        table: NodeId,
        /// Row index.
        index: usize,
        /// The rows.
        rows: Vec<Row>,
    },
    /// Removes rows; at least one must remain. Inverse: [`Op::InsertRows`].
    RemoveRows {
        /// The table.
        table: NodeId,
        /// First row.
        index: usize,
        /// How many.
        count: usize,
    },
    /// Replaces a row's properties and revision.
    SetRow {
        /// The row.
        row: NodeId,
        /// Properties.
        props: Box<TableRowProps>,
        /// Tracked insertion/deletion.
        revision: Option<InsDelRevision>,
    },
    /// Inserts cells into a row. Inverse: [`Op::RemoveCells`].
    InsertCells {
        /// The row.
        row: NodeId,
        /// Cell index.
        index: usize,
        /// The cells.
        cells: Vec<Cell>,
    },
    /// Removes cells; at least one must remain. Inverse: [`Op::InsertCells`].
    RemoveCells {
        /// The row.
        row: NodeId,
        /// First cell.
        index: usize,
        /// How many.
        count: usize,
    },
    /// Replaces a cell's properties and spans.
    SetCell {
        /// The cell.
        cell: NodeId,
        /// Properties.
        props: Box<TableCellProps>,
        /// `w:gridSpan` (at least 1).
        grid_span: u32,
        /// `w:vMerge`.
        v_merge: Option<VMerge>,
    },
    // ---- sections
    /// Replaces a section's properties.
    SetSectionProps {
        /// The section.
        section: NodeId,
        /// Properties.
        props: Box<SectionProps>,
    },
    /// Replaces a section's header and footer references.
    SetSectionRefs {
        /// The section.
        section: NodeId,
        /// Headers.
        headers: HeaderFooterRefs,
        /// Footers.
        footers: HeaderFooterRefs,
    },
    /// Moves blocks `block_index..` of a section into a new section right after it (a section
    /// break). Inverse: [`Op::MergeSections`].
    SplitSection {
        /// The section.
        section: NodeId,
        /// First block of the new section (both parts keep at least one block).
        block_index: usize,
        /// The new section's id.
        new_section: NodeId,
        /// The new section's properties.
        props: Box<SectionProps>,
        /// Its headers.
        headers: HeaderFooterRefs,
        /// Its footers.
        footers: HeaderFooterRefs,
    },
    /// Appends `second`'s blocks to `first` (its neighbour) and removes `second`.
    /// Inverse: [`Op::SplitSection`].
    MergeSections {
        /// The section that stays.
        first: NodeId,
        /// The section that goes.
        second: NodeId,
    },
    // ---- stories
    /// Adds a header or footer story. Inverse: [`Op::RemoveHeaderFooter`].
    InsertHeaderFooter {
        /// Index in [`Document::headers_footers`].
        index: usize,
        /// The story.
        story: Box<HeaderFooter>,
    },
    /// Removes an unreferenced header or footer story.
    RemoveHeaderFooter {
        /// The story.
        id: NodeId,
    },
    /// Adds a note story. Inverse: [`Op::RemoveNote`].
    InsertNote {
        /// Index in [`Document::notes`].
        index: usize,
        /// The note.
        note: Box<Note>,
    },
    /// Removes an unreferenced note.
    RemoveNote {
        /// The note.
        id: NodeId,
    },
    /// Adds a comment. Inverse: [`Op::RemoveComment`].
    InsertComment {
        /// Index in [`Document::comments`].
        index: usize,
        /// The comment.
        comment: Box<Comment>,
    },
    /// Removes a comment that no mark and no reply refers to.
    RemoveComment {
        /// The comment.
        id: NodeId,
    },
    /// Replaces a comment's metadata (author, resolved, parent).
    SetCommentMeta {
        /// The comment.
        id: NodeId,
        /// The metadata.
        meta: CommentMeta,
    },
    // ---- document parts
    /// Adds or replaces a style (a replaced style keeps its position; a new one goes to
    /// `index`). Inverse: the previous style or [`Op::DeleteStyle`].
    PutStyle {
        /// The style.
        style: Box<Style>,
        /// Position for a new style.
        index: usize,
    },
    /// Removes a style (references to it are left alone). Inverse: [`Op::PutStyle`].
    DeleteStyle {
        /// The style id.
        id: String,
    },
    /// Replaces the document defaults.
    SetDocDefaults {
        /// The defaults.
        defaults: Box<DocDefaults>,
    },
    /// Adds, replaces or removes (`None`) an abstract list definition.
    SetAbstractNum {
        /// `w:abstractNumId`.
        id: u32,
        /// The definition.
        def: Option<Box<AbstractNum>>,
    },
    /// Adds, replaces or removes (`None`) a list instance.
    SetNumInstance {
        /// `w:numId`.
        id: u32,
        /// The instance.
        instance: Option<NumInstance>,
    },
    /// Replaces the settings.
    SetSettings {
        /// The settings.
        settings: Box<Settings>,
    },
    /// Replaces the document properties.
    SetProperties {
        /// The properties.
        properties: Box<DocProperties>,
    },
    /// Replaces the theme.
    SetTheme {
        /// The theme.
        theme: Box<DocTheme>,
    },
    /// Adds, replaces or removes (`None`) a media file.
    PutMedia {
        /// The part name.
        name: String,
        /// The media.
        media: Option<Media>,
    },
}

impl Op {
    /// A split that gives the new paragraph the properties of `para` (what Enter does before the
    /// command layer applies the "next" style).
    ///
    /// # Errors
    ///
    /// [`DocError::UnknownNode`] when `para` is not a paragraph.
    pub fn split_paragraph(
        doc: &Document,
        para: NodeId,
        offset: usize,
        new_para: NodeId,
    ) -> Result<Op, DocError> {
        let p = doc.paragraph(para).ok_or(DocError::UnknownNode(para))?;
        Ok(Op::SplitParagraph {
            para,
            offset,
            new_para,
            props: Box::new(p.props.clone()),
            mark_props: Box::new(p.mark_props.clone()),
            revision: Box::default(),
        })
    }

    /// The paragraph this op edits, if it edits paragraph content or properties.
    #[must_use]
    pub fn paragraph(&self) -> Option<NodeId> {
        match self {
            Self::InsertInline { para, .. }
            | Self::DeleteInline { para, .. }
            | Self::SetRuns { para, .. }
            | Self::AddMark { para, .. }
            | Self::RemoveMark { para, .. }
            | Self::SetMarks { para, .. }
            | Self::SetObject { para, .. }
            | Self::SetParagraphProps { para, .. }
            | Self::SetParagraphMarkProps { para, .. }
            | Self::SetParagraphRevision { para, .. }
            | Self::SplitParagraph { para, .. } => Some(*para),
            Self::MergeParagraphs { first, .. } => Some(*first),
            _ => None,
        }
    }
}

/// What kind of edit a transaction is (drives undo grouping and labels).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum EditKind {
    /// Typed text (coalesced per word in undo).
    Typing,
    /// Backspace/Delete.
    Delete,
    /// Formatting.
    Format,
    /// Structure (paragraphs, tables, sections).
    Structure,
    /// Anything else.
    #[default]
    Other,
}

/// An atomic list of ops with metadata.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Transaction {
    /// The ops, applied in order.
    pub ops: Vec<Op>,
    /// What kind of edit this is.
    pub kind: EditKind,
    /// The selection before the edit (restored by undo).
    pub selection_before: Option<Range>,
    /// The selection after the edit (restored by redo).
    pub selection_after: Option<Range>,
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
    pub fn with_selection(mut self, before: Option<Range>, after: Option<Range>) -> Self {
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

fn range_err(offset: usize, len: usize) -> DocError {
    DocError::OutOfRange { offset, len }
}

fn ids_in_object(o: &InlineObject, out: &mut Vec<NodeId>) {
    if let InlineObject::Drawing(d) = o {
        out.push(d.id);
        if let Some(tb) = &d.text_box {
            ids_in_blocks(tb, out);
        }
    }
}

fn ids_in_fragment(f: &Fragment) -> Vec<NodeId> {
    let mut out = Vec::new();
    for o in &f.objects {
        ids_in_object(o, &mut out);
    }
    out
}

impl Document {
    /// Applies a transaction and returns its inverse. Atomic: on error nothing changes.
    ///
    /// # Errors
    ///
    /// The first failing op's error.
    pub fn apply(&mut self, tx: Transaction) -> Result<Transaction, DocError> {
        let mut inverses: Vec<Op> = Vec::with_capacity(tx.ops.len());
        for op in tx.ops {
            match self.apply_op(op) {
                Ok(inv) => inverses.push(inv),
                Err(e) => {
                    for inv in inverses.into_iter().rev() {
                        // Inverses of ops that just succeeded cannot fail; if one ever did, the
                        // remaining rollback still runs and the original error is reported.
                        let _ = self.apply_op(inv);
                    }
                    return Err(e);
                }
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

    /// Fails when any of `ids` is already used in the document or repeated.
    fn check_fresh(&self, ids: &[NodeId], replaced: &[NodeId]) -> Result<(), DocError> {
        if ids.is_empty() {
            return Ok(());
        }
        let mut seen = HashSet::new();
        for id in ids {
            if !seen.insert(*id) || *id == NodeId(0) {
                return Err(DocError::DuplicateId(*id));
            }
        }
        let replaced: HashSet<NodeId> = replaced.iter().copied().collect();
        if let Some(dup) = self
            .all_ids()
            .into_iter()
            .find(|i| seen.contains(i) && !replaced.contains(i))
        {
            return Err(DocError::DuplicateId(dup));
        }
        Ok(())
    }

    /// Keeps the allocator above every id.
    fn bump_ids(&mut self, ids: &[NodeId]) {
        if let Some(max) = ids.iter().copied().max() {
            self.ensure_ids_above(max);
        }
    }

    fn check_marks(&self, marks: &[MarkSpan]) -> Result<(), DocError> {
        for m in marks {
            if let Mark::Comment(c) = m.mark
                && !self.comments.iter().any(|x| x.id == c)
            {
                return Err(DocError::InvalidOp(format!("no comment {c}")));
            }
        }
        Ok(())
    }

    fn check_objects(&self, objects: &[InlineObject]) -> Result<(), DocError> {
        let refs = self.reference_targets();
        for o in objects {
            match o {
                InlineObject::NoteRef(n) if !refs.notes.contains(&n.note) => {
                    return Err(DocError::InvalidOp(format!("no note {}", n.note)));
                }
                InlineObject::Drawing(d) => {
                    if let Some(tb) = &d.text_box {
                        if tb.is_empty() {
                            return Err(DocError::InvalidOp("empty text box".into()));
                        }
                        check_blocks(tb, &refs)?;
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn check_new_blocks(&self, blocks: &[Block]) -> Result<Vec<NodeId>, DocError> {
        check_blocks(blocks, &self.reference_targets())?;
        let mut ids = Vec::new();
        ids_in_blocks(blocks, &mut ids);
        self.check_fresh(&ids, &[])?;
        Ok(ids)
    }

    fn is_comment_referenced(&self, id: NodeId) -> bool {
        self.comments.iter().any(|c| c.meta.parent == Some(id))
            || self.stories().any(|s| {
                self.paragraphs_in(s).iter().any(|p| {
                    p.content
                        .marks()
                        .iter()
                        .any(|m| m.mark == Mark::Comment(id))
                })
            })
    }

    /// True when following parents from `from` reaches `target` (a reply cycle).
    fn reply_chain_reaches(&self, from: NodeId, target: NodeId) -> bool {
        let mut cur = Some(from);
        let mut steps = 0;
        while let Some(c) = cur {
            if c == target {
                return true;
            }
            steps += 1;
            if steps > self.comments.len() {
                return true;
            }
            cur = self
                .comments
                .iter()
                .find(|x| x.id == c)
                .and_then(|x| x.meta.parent);
        }
        false
    }

    fn is_note_referenced(&self, id: NodeId) -> bool {
        self.stories().any(|s| {
            self.paragraphs_in(s).iter().any(|p| {
                p.content
                    .objects()
                    .iter()
                    .any(|o| matches!(o, InlineObject::NoteRef(n) if n.note == id))
            })
        })
    }

    fn check_refs(
        &self,
        headers: &HeaderFooterRefs,
        footers: &HeaderFooterRefs,
    ) -> Result<(), DocError> {
        for (refs, part) in [
            (headers, HeaderFooterPart::Header),
            (footers, HeaderFooterPart::Footer),
        ] {
            for id in refs.ids() {
                if !self
                    .headers_footers
                    .iter()
                    .any(|h| h.id == id && h.part == part)
                {
                    return Err(DocError::InvalidOp(format!("{id} is not a {part:?} story")));
                }
            }
        }
        Ok(())
    }

    /// Applies one op and returns its inverse. Prefer [`Document::apply`], which is atomic over
    /// several ops.
    ///
    /// # Errors
    ///
    /// Unknown ids, offsets out of range, duplicate ids and ops that would break an invariant
    /// fail without changing the document.
    #[allow(clippy::too_many_lines)]
    pub fn apply_op(&mut self, op: Op) -> Result<Op, DocError> {
        match op {
            Op::InsertInline {
                para,
                offset,
                fragment,
            } => {
                fragment.validate()?;
                self.check_marks(&fragment.marks)?;
                self.check_objects(&fragment.objects)?;
                let ids = ids_in_fragment(&fragment);
                self.check_fresh(&ids, &[])?;
                let len = fragment.len();
                self.paragraph_mut(para)?.content.insert(offset, fragment)?;
                self.bump_ids(&ids);
                Ok(Op::DeleteInline { para, offset, len })
            }
            Op::DeleteInline { para, offset, len } => {
                let p = self.paragraph_mut(para)?;
                let end = offset
                    .checked_add(len)
                    .ok_or_else(|| range_err(offset, p.content.len()))?;
                let fragment = p.content.remove(offset, end)?;
                Ok(Op::InsertInline {
                    para,
                    offset,
                    fragment,
                })
            }
            Op::SetRuns { para, offset, runs } => {
                let old = self.paragraph_mut(para)?.content.set_runs(offset, runs)?;
                Ok(Op::SetRuns {
                    para,
                    offset,
                    runs: old,
                })
            }
            Op::AddMark {
                para,
                start,
                end,
                mark,
            } => {
                self.check_marks(&[MarkSpan::new(start, end, mark.clone())])?;
                let p = self.paragraph_mut(para)?;
                let old = p.content.marks().to_vec();
                p.content.add_mark(start, end, mark)?;
                Ok(Op::SetMarks { para, marks: old })
            }
            Op::RemoveMark {
                para,
                start,
                end,
                mark,
            } => {
                let p = self.paragraph_mut(para)?;
                let old = p.content.marks().to_vec();
                p.content.remove_mark(start, end, &mark)?;
                Ok(Op::SetMarks { para, marks: old })
            }
            Op::SetMarks { para, marks } => {
                self.check_marks(&marks)?;
                let old = self.paragraph_mut(para)?.content.set_marks(marks)?;
                Ok(Op::SetMarks { para, marks: old })
            }
            Op::SetObject {
                para,
                offset,
                object,
            } => {
                let old_ids = {
                    let p = self.paragraph(para).ok_or(DocError::UnknownNode(para))?;
                    let old = p
                        .content
                        .object_at(offset)
                        .ok_or(DocError::InvalidOp(format!("no object at {offset}")))?;
                    let mut ids = Vec::new();
                    ids_in_object(old, &mut ids);
                    ids
                };
                self.check_objects(std::slice::from_ref(&object))?;
                let mut new_ids = Vec::new();
                ids_in_object(&object, &mut new_ids);
                self.check_fresh(&new_ids, &old_ids)?;
                let slot = self
                    .paragraph_mut(para)?
                    .content
                    .object_at_mut(offset)
                    .ok_or(DocError::InvalidOp(format!("no object at {offset}")))?;
                let old = std::mem::replace(slot, object);
                self.bump_ids(&new_ids);
                Ok(Op::SetObject {
                    para,
                    offset,
                    object: old,
                })
            }
            Op::SetParagraphProps { para, props } => {
                let p = self.paragraph_mut(para)?;
                let old = std::mem::replace(&mut p.props, *props);
                Ok(Op::SetParagraphProps {
                    para,
                    props: Box::new(old),
                })
            }
            Op::SetParagraphMarkProps { para, props } => {
                let p = self.paragraph_mut(para)?;
                let old = std::mem::replace(&mut p.mark_props, *props);
                Ok(Op::SetParagraphMarkProps {
                    para,
                    props: Box::new(old),
                })
            }
            Op::SetParagraphRevision { para, revision } => {
                let p = self.paragraph_mut(para)?;
                let old = std::mem::replace(&mut p.revision, *revision);
                Ok(Op::SetParagraphRevision {
                    para,
                    revision: Box::new(old),
                })
            }
            Op::SplitParagraph {
                para,
                offset,
                new_para,
                props,
                mark_props,
                revision,
            } => {
                self.check_fresh(&[new_para], &[])?;
                let (blocks, index) = self.block_slot_mut(para)?;
                let Some(Block::Paragraph(p)) = blocks.get_mut(index) else {
                    return Err(DocError::WrongKind {
                        id: para,
                        expected: "paragraph",
                    });
                };
                if offset > p.content.len() {
                    return Err(range_err(offset, p.content.len()));
                }
                let tail = p.content.split_off(offset)?;
                let new = Paragraph {
                    id: new_para,
                    props: *props,
                    mark_props: *mark_props,
                    revision: *revision,
                    content: tail,
                };
                blocks.insert(index + 1, Block::Paragraph(new));
                self.bump_ids(&[new_para]);
                Ok(Op::MergeParagraphs {
                    first: para,
                    second: new_para,
                })
            }
            Op::MergeParagraphs { first, second } => {
                let (blocks, index) = self.block_slot_mut(first)?;
                let ok = matches!(blocks.get(index), Some(Block::Paragraph(_)))
                    && matches!(blocks.get(index + 1), Some(Block::Paragraph(p)) if p.id == second);
                if !ok {
                    return Err(DocError::InvalidOp(format!(
                        "{second} is not the paragraph right after {first}"
                    )));
                }
                let Block::Paragraph(second_p) = blocks.remove(index + 1) else {
                    return Err(DocError::InvalidOp("not a paragraph".into()));
                };
                let Some(Block::Paragraph(first_p)) = blocks.get_mut(index) else {
                    return Err(DocError::InvalidOp("not a paragraph".into()));
                };
                let offset = first_p.content.len();
                first_p.content.append(second_p.content)?;
                Ok(Op::SplitParagraph {
                    para: first,
                    offset,
                    new_para: second,
                    props: Box::new(second_p.props),
                    mark_props: Box::new(second_p.mark_props),
                    revision: Box::new(second_p.revision),
                })
            }
            Op::InsertBlocks {
                container,
                index,
                blocks,
            } => {
                if blocks.is_empty() {
                    return Err(DocError::InvalidOp("no blocks to insert".into()));
                }
                let ids = self.check_new_blocks(&blocks)?;
                let target = self.container_mut(container)?;
                if index > target.len() {
                    return Err(range_err(index, target.len()));
                }
                let count = blocks.len();
                target.splice(index..index, blocks);
                self.bump_ids(&ids);
                Ok(Op::RemoveBlocks {
                    container,
                    index,
                    count,
                })
            }
            Op::RemoveBlocks {
                container,
                index,
                count,
            } => {
                let target = self.container_mut(container)?;
                let end = index
                    .checked_add(count)
                    .filter(|e| *e <= target.len())
                    .ok_or_else(|| range_err(index.saturating_add(count), target.len()))?;
                if count == 0 || count >= target.len() {
                    return Err(DocError::InvalidOp(
                        "a container keeps at least one block".into(),
                    ));
                }
                let removed: Vec<Block> = target.drain(index..end).collect();
                Ok(Op::InsertBlocks {
                    container,
                    index,
                    blocks: removed,
                })
            }
            Op::SetTableProps { table, props } => {
                let t = self.table_mut(table)?;
                let old = std::mem::replace(&mut t.props, *props);
                Ok(Op::SetTableProps {
                    table,
                    props: Box::new(old),
                })
            }
            Op::SetTableGrid { table, grid } => {
                let t = self.table_mut(table)?;
                let old = std::mem::replace(&mut t.grid, grid);
                Ok(Op::SetTableGrid { table, grid: old })
            }
            Op::InsertRows { table, index, rows } => {
                if rows.is_empty() {
                    return Err(DocError::InvalidOp("no rows to insert".into()));
                }
                let refs = self.reference_targets();
                let mut ids = Vec::new();
                for r in &rows {
                    if r.cells.is_empty() {
                        return Err(DocError::InvalidOp(format!("row {} has no cells", r.id)));
                    }
                    ids.push(r.id);
                    for c in &r.cells {
                        if c.blocks.is_empty() || c.grid_span == 0 {
                            return Err(DocError::InvalidOp(format!("cell {} is invalid", c.id)));
                        }
                        ids.push(c.id);
                        check_blocks(&c.blocks, &refs)?;
                        ids_in_blocks(&c.blocks, &mut ids);
                    }
                }
                self.check_fresh(&ids, &[])?;
                let t = self.table_mut(table)?;
                if index > t.rows.len() {
                    return Err(range_err(index, t.rows.len()));
                }
                let count = rows.len();
                t.rows.splice(index..index, rows);
                self.bump_ids(&ids);
                Ok(Op::RemoveRows {
                    table,
                    index,
                    count,
                })
            }
            Op::RemoveRows {
                table,
                index,
                count,
            } => {
                let t = self.table_mut(table)?;
                let end = index
                    .checked_add(count)
                    .filter(|e| *e <= t.rows.len())
                    .ok_or_else(|| range_err(index.saturating_add(count), t.rows.len()))?;
                if count == 0 || count >= t.rows.len() {
                    return Err(DocError::InvalidOp("a table keeps at least one row".into()));
                }
                let rows: Vec<Row> = t.rows.drain(index..end).collect();
                Ok(Op::InsertRows { table, index, rows })
            }
            Op::SetRow {
                row,
                props,
                revision,
            } => {
                let r = self.row_mut(row)?;
                let old_props = std::mem::replace(&mut r.props, *props);
                let old_rev = std::mem::replace(&mut r.revision, revision);
                Ok(Op::SetRow {
                    row,
                    props: Box::new(old_props),
                    revision: old_rev,
                })
            }
            Op::InsertCells { row, index, cells } => {
                if cells.is_empty() {
                    return Err(DocError::InvalidOp("no cells to insert".into()));
                }
                let refs = self.reference_targets();
                let mut ids = Vec::new();
                for c in &cells {
                    if c.blocks.is_empty() || c.grid_span == 0 {
                        return Err(DocError::InvalidOp(format!("cell {} is invalid", c.id)));
                    }
                    ids.push(c.id);
                    check_blocks(&c.blocks, &refs)?;
                    ids_in_blocks(&c.blocks, &mut ids);
                }
                self.check_fresh(&ids, &[])?;
                let r = self.row_mut(row)?;
                if index > r.cells.len() {
                    return Err(range_err(index, r.cells.len()));
                }
                let count = cells.len();
                r.cells.splice(index..index, cells);
                self.bump_ids(&ids);
                Ok(Op::RemoveCells { row, index, count })
            }
            Op::RemoveCells { row, index, count } => {
                let r = self.row_mut(row)?;
                let end = index
                    .checked_add(count)
                    .filter(|e| *e <= r.cells.len())
                    .ok_or_else(|| range_err(index.saturating_add(count), r.cells.len()))?;
                if count == 0 || count >= r.cells.len() {
                    return Err(DocError::InvalidOp("a row keeps at least one cell".into()));
                }
                let cells: Vec<Cell> = r.cells.drain(index..end).collect();
                Ok(Op::InsertCells { row, index, cells })
            }
            Op::SetCell {
                cell,
                props,
                grid_span,
                v_merge,
            } => {
                if grid_span == 0 {
                    return Err(DocError::InvalidOp("gridSpan must be at least 1".into()));
                }
                let c = self.cell_mut(cell)?;
                let old_props = std::mem::replace(&mut c.props, *props);
                let old_span = std::mem::replace(&mut c.grid_span, grid_span);
                let old_merge = std::mem::replace(&mut c.v_merge, v_merge);
                Ok(Op::SetCell {
                    cell,
                    props: Box::new(old_props),
                    grid_span: old_span,
                    v_merge: old_merge,
                })
            }
            Op::SetSectionProps { section, props } => {
                let s = self.section_mut(section)?;
                let old = std::mem::replace(&mut s.props, *props);
                Ok(Op::SetSectionProps {
                    section,
                    props: Box::new(old),
                })
            }
            Op::SetSectionRefs {
                section,
                headers,
                footers,
            } => {
                self.check_refs(&headers, &footers)?;
                let s = self.section_mut(section)?;
                let old_h = std::mem::replace(&mut s.headers, headers);
                let old_f = std::mem::replace(&mut s.footers, footers);
                Ok(Op::SetSectionRefs {
                    section,
                    headers: old_h,
                    footers: old_f,
                })
            }
            Op::SplitSection {
                section,
                block_index,
                new_section,
                props,
                headers,
                footers,
            } => {
                self.check_fresh(&[new_section], &[])?;
                self.check_refs(&headers, &footers)?;
                let (i, _) = self
                    .section(section)
                    .ok_or(DocError::UnknownNode(section))?;
                let s = &mut self.sections[i];
                if block_index == 0 || block_index >= s.blocks.len() {
                    return Err(DocError::InvalidOp(
                        "both sections keep at least one block".into(),
                    ));
                }
                let tail: Vec<Block> = s.blocks.drain(block_index..).collect();
                self.sections.insert(
                    i + 1,
                    Section {
                        id: new_section,
                        props: *props,
                        headers,
                        footers,
                        blocks: tail,
                    },
                );
                self.bump_ids(&[new_section]);
                Ok(Op::MergeSections {
                    first: section,
                    second: new_section,
                })
            }
            Op::MergeSections { first, second } => {
                let (i, _) = self.section(first).ok_or(DocError::UnknownNode(first))?;
                if self.sections.get(i + 1).map(|s| s.id) != Some(second) {
                    return Err(DocError::InvalidOp(format!(
                        "{second} is not the section right after {first}"
                    )));
                }
                let removed = self.sections.remove(i + 1);
                let block_index = self.sections[i].blocks.len();
                self.sections[i].blocks.extend(removed.blocks);
                Ok(Op::SplitSection {
                    section: first,
                    block_index,
                    new_section: second,
                    props: Box::new(removed.props),
                    headers: removed.headers,
                    footers: removed.footers,
                })
            }
            Op::InsertHeaderFooter { index, story } => {
                if story.blocks.is_empty() {
                    return Err(DocError::InvalidOp("a story needs a block".into()));
                }
                if index > self.headers_footers.len() {
                    return Err(range_err(index, self.headers_footers.len()));
                }
                let mut ids = self.check_new_blocks(&story.blocks)?;
                ids.push(story.id);
                self.check_fresh(&ids, &[])?;
                let id = story.id;
                self.headers_footers.insert(index, *story);
                self.bump_ids(&ids);
                Ok(Op::RemoveHeaderFooter { id })
            }
            Op::RemoveHeaderFooter { id } => {
                let index = self
                    .headers_footers
                    .iter()
                    .position(|h| h.id == id)
                    .ok_or(DocError::UnknownNode(id))?;
                if self
                    .sections
                    .iter()
                    .any(|s| s.headers.ids().chain(s.footers.ids()).any(|r| r == id))
                {
                    return Err(DocError::InvalidOp(format!("{id} is still referenced")));
                }
                let story = self.headers_footers.remove(index);
                Ok(Op::InsertHeaderFooter {
                    index,
                    story: Box::new(story),
                })
            }
            Op::InsertNote { index, note } => {
                if note.blocks.is_empty() {
                    return Err(DocError::InvalidOp("a story needs a block".into()));
                }
                if index > self.notes.len() {
                    return Err(range_err(index, self.notes.len()));
                }
                let mut ids = self.check_new_blocks(&note.blocks)?;
                ids.push(note.id);
                self.check_fresh(&ids, &[])?;
                let id = note.id;
                self.notes.insert(index, *note);
                self.bump_ids(&ids);
                Ok(Op::RemoveNote { id })
            }
            Op::RemoveNote { id } => {
                let index = self
                    .notes
                    .iter()
                    .position(|n| n.id == id)
                    .ok_or(DocError::UnknownNode(id))?;
                if self.is_note_referenced(id) {
                    return Err(DocError::InvalidOp(format!("{id} is still referenced")));
                }
                let note = self.notes.remove(index);
                Ok(Op::InsertNote {
                    index,
                    note: Box::new(note),
                })
            }
            Op::InsertComment { index, comment } => {
                if comment.blocks.is_empty() {
                    return Err(DocError::InvalidOp("a comment needs a block".into()));
                }
                if index > self.comments.len() {
                    return Err(range_err(index, self.comments.len()));
                }
                if let Some(p) = comment.meta.parent
                    && !self.comments.iter().any(|c| c.id == p)
                {
                    return Err(DocError::InvalidOp(format!("no comment {p} to reply to")));
                }
                let mut ids = self.check_new_blocks(&comment.blocks)?;
                ids.push(comment.id);
                self.check_fresh(&ids, &[])?;
                let id = comment.id;
                self.comments.insert(index, *comment);
                self.bump_ids(&ids);
                Ok(Op::RemoveComment { id })
            }
            Op::RemoveComment { id } => {
                let index = self
                    .comments
                    .iter()
                    .position(|c| c.id == id)
                    .ok_or(DocError::UnknownNode(id))?;
                if self.is_comment_referenced(id) {
                    return Err(DocError::InvalidOp(format!(
                        "{id} still has marks or replies"
                    )));
                }
                let comment = self.comments.remove(index);
                Ok(Op::InsertComment {
                    index,
                    comment: Box::new(comment),
                })
            }
            Op::SetCommentMeta { id, meta } => {
                if let Some(p) = meta.parent
                    && (!self.comments.iter().any(|c| c.id == p) || self.reply_chain_reaches(p, id))
                {
                    return Err(DocError::InvalidOp(format!("{id} cannot reply to {p}")));
                }
                let c = self
                    .comments
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or(DocError::UnknownNode(id))?;
                let old = std::mem::replace(&mut c.meta, meta);
                Ok(Op::SetCommentMeta { id, meta: old })
            }
            Op::PutStyle { style, index } => {
                if style.id.is_empty() {
                    return Err(DocError::InvalidStyle("style id must not be empty".into()));
                }
                let id = style.id.clone();
                let position = self.styles.index_of(&id);
                match self.styles.put_at(*style, index) {
                    Some(old) => Ok(Op::PutStyle {
                        style: Box::new(old),
                        index: position.unwrap_or(index),
                    }),
                    None => Ok(Op::DeleteStyle { id }),
                }
            }
            Op::DeleteStyle { id } => {
                let (index, style) = self
                    .styles
                    .take(&id)
                    .ok_or_else(|| DocError::UnknownStyle(id.clone()))?;
                Ok(Op::PutStyle {
                    style: Box::new(style),
                    index,
                })
            }
            Op::SetDocDefaults { defaults } => {
                let old = std::mem::replace(&mut self.styles.doc_defaults, *defaults);
                Ok(Op::SetDocDefaults {
                    defaults: Box::new(old),
                })
            }
            Op::SetAbstractNum { id, def } => {
                if def.as_ref().is_some_and(|d| d.id != id) {
                    return Err(DocError::InvalidOp("abstractNum id mismatch".into()));
                }
                let old = match def {
                    Some(d) => self.numbering.abstracts.insert(id, *d),
                    None => self.numbering.abstracts.remove(&id),
                };
                Ok(Op::SetAbstractNum {
                    id,
                    def: old.map(Box::new),
                })
            }
            Op::SetNumInstance { id, instance } => {
                if id == 0 || instance.as_ref().is_some_and(|d| d.id != id) {
                    return Err(DocError::InvalidOp("numId must match and not be 0".into()));
                }
                let old = match instance {
                    Some(n) => self.numbering.instances.insert(id, n),
                    None => self.numbering.instances.remove(&id),
                };
                Ok(Op::SetNumInstance { id, instance: old })
            }
            Op::SetSettings { settings } => {
                let old = std::mem::replace(&mut self.settings, *settings);
                Ok(Op::SetSettings {
                    settings: Box::new(old),
                })
            }
            Op::SetProperties { properties } => {
                let old = std::mem::replace(&mut self.properties, *properties);
                Ok(Op::SetProperties {
                    properties: Box::new(old),
                })
            }
            Op::SetTheme { theme } => {
                let old = std::mem::replace(&mut self.theme, *theme);
                Ok(Op::SetTheme {
                    theme: Box::new(old),
                })
            }
            Op::PutMedia { name, media } => {
                let old = match media {
                    Some(m) => self.media.insert(name.clone(), m),
                    None => self.media.remove(&name),
                };
                Ok(Op::PutMedia { name, media: old })
            }
        }
    }

    fn section_mut(&mut self, id: NodeId) -> Result<&mut Section, DocError> {
        self.sections
            .iter_mut()
            .find(|s| s.id == id)
            .ok_or(DocError::UnknownNode(id))
    }
}

#[cfg(test)]
mod tests;
