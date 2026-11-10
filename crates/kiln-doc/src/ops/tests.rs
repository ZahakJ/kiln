// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ops and transactions: unit tests for each op's validation and inverse, and property tests:
//! random op sequences followed by their inverses restore the document exactly, failed ops
//! change nothing, every intermediate document is valid, and serialisation round-trips.

use super::*;
use crate::inline::{Hyperlink, Inlines, LinkTarget};
use crate::model::{NodeKind, OpaqueBlock, SdtBlock, Story, Table};
use crate::numbering::Level;
use crate::props::{DocColor, Indent, ParagraphBorders, Spacing};
use crate::revision::RevisionInfo;
use crate::sdt::SdtInfo;
use crate::section::Paper;
use crate::st::{Alignment, BreakKind, NoteKind, NoteType, NumberFormat, StyleType};
use crate::{Break, Equation, Field, NoteRef};
use proptest::prelude::*;

fn para_with(doc: &mut Document, text: &str) -> Result<Paragraph, DocError> {
    let mut p = Paragraph::new(doc.alloc_id());
    p.content = Inlines::from_fragment(Fragment::text(text, RunProps::default()))?;
    Ok(p)
}

fn cell(doc: &mut Document, text: &str) -> Result<Cell, DocError> {
    let p = para_with(doc, text)?;
    Ok(Cell {
        id: doc.alloc_id(),
        props: TableCellProps::default(),
        grid_span: 1,
        v_merge: None,
        blocks: vec![Block::Paragraph(p)],
    })
}

fn row(doc: &mut Document, cells: &[&str]) -> Result<Row, DocError> {
    let cells = cells
        .iter()
        .map(|t| cell(doc, t))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Row {
        id: doc.alloc_id(),
        props: TableRowProps::default(),
        revision: None,
        cells,
    })
}

/// A document with every kind of story and block.
fn rich() -> Result<Document, DocError> {
    let mut doc = Document::new();
    let first = doc.body_paragraphs()[0].id;
    let p2 = para_with(&mut doc, "Second paragraph with words")?;
    let r1 = row(&mut doc, &["a1", "b1"])?;
    let r2 = row(&mut doc, &["a2", "b2"])?;
    let table = Table {
        id: doc.alloc_id(),
        props: TableProps::default(),
        grid: vec![2000, 2000],
        rows: vec![r1, r2],
    };
    let inner = para_with(&mut doc, "inside a control")?;
    let sdt = SdtBlock {
        id: doc.alloc_id(),
        info: SdtInfo::default(),
        blocks: vec![Block::Paragraph(inner)],
    };
    let opaque = OpaqueBlock {
        id: doc.alloc_id(),
        xml: "<w:altChunk/>".into(),
        label: "[chunk]".into(),
    };
    doc.sections[0].blocks.extend([
        Block::Paragraph(p2),
        Block::Table(table),
        Block::Sdt(sdt),
        Block::Opaque(opaque),
    ]);
    let hp = para_with(&mut doc, "Header text")?;
    let header = HeaderFooter {
        id: doc.alloc_id(),
        part: HeaderFooterPart::Header,
        blocks: vec![Block::Paragraph(hp)],
    };
    let fp = para_with(&mut doc, "Footer text")?;
    let footer = HeaderFooter {
        id: doc.alloc_id(),
        part: HeaderFooterPart::Footer,
        blocks: vec![Block::Paragraph(fp)],
    };
    let np = para_with(&mut doc, "A note")?;
    let note = Note {
        id: doc.alloc_id(),
        kind: NoteKind::Footnote,
        note_type: NoteType::Normal,
        blocks: vec![Block::Paragraph(np)],
    };
    let cp = para_with(&mut doc, "A comment")?;
    let comment = Comment {
        id: doc.alloc_id(),
        meta: CommentMeta {
            author: "Ada".into(),
            ..CommentMeta::default()
        },
        blocks: vec![Block::Paragraph(cp)],
    };
    let (hid, fid, nid, cid) = (header.id, footer.id, note.id, comment.id);
    doc.headers_footers.extend([header, footer]);
    doc.notes.push(note);
    doc.comments.push(comment);
    doc.sections[0].headers.default = Some(hid);
    doc.sections[0].footers.default = Some(fid);
    doc.apply(Transaction::new(vec![
        Op::InsertInline {
            para: first,
            offset: 0,
            fragment: Fragment::text("Hello world", RunProps::default()),
        },
        Op::InsertInline {
            para: first,
            offset: 11,
            fragment: Fragment::object(
                InlineObject::NoteRef(NoteRef {
                    note: nid,
                    kind: NoteKind::Footnote,
                    custom_mark: None,
                }),
                RunProps::default(),
            ),
        },
        Op::AddMark {
            para: first,
            start: 0,
            end: 5,
            mark: Mark::Comment(cid),
        },
    ]))?;
    doc.validate()?;
    Ok(doc)
}

fn first_body_para(doc: &Document) -> NodeId {
    doc.body_paragraphs()[0].id
}

fn round_trip(doc: &mut Document, op: Op) -> Result<(), DocError> {
    let before = doc.clone();
    let inv = doc.apply_op(op)?;
    doc.validate()?;
    doc.apply_op(inv)?;
    assert_eq!(*doc, before);
    doc.validate()
}

// ------------------------------------------------------------------ unit tests

#[test]
fn every_op_kind_round_trips() -> Result<(), DocError> {
    let mut doc = rich()?;
    let p = first_body_para(&doc);
    let table_id = doc.sections[0].blocks[2].id();
    let t = doc
        .table(table_id)
        .cloned()
        .ok_or(DocError::UnknownNode(table_id))?;
    let section = doc.sections[0].id;
    let cid = doc.comments[0].id;
    let bold = RunProps {
        bold: Some(true),
        ..RunProps::default()
    };
    let new_para = doc.alloc_id();
    let new_section = doc.alloc_id();
    let blk = Block::Paragraph(para_with(&mut doc, "inserted")?);
    let new_row = row(&mut doc, &["x", "y"])?;
    let new_cell = cell(&mut doc, "z")?;
    let hf_para = para_with(&mut doc, "new header")?;
    let hf = HeaderFooter {
        id: doc.alloc_id(),
        part: HeaderFooterPart::Header,
        blocks: vec![Block::Paragraph(hf_para)],
    };
    let note_para = para_with(&mut doc, "new note")?;
    let note = Note {
        id: doc.alloc_id(),
        kind: NoteKind::Endnote,
        note_type: NoteType::Normal,
        blocks: vec![Block::Paragraph(note_para)],
    };
    let c_para = para_with(&mut doc, "reply")?;
    let reply = Comment {
        id: doc.alloc_id(),
        meta: CommentMeta {
            author: "Grace".into(),
            parent: Some(cid),
            ..CommentMeta::default()
        },
        blocks: vec![Block::Paragraph(c_para)],
    };
    let ops = vec![
        Op::InsertInline {
            para: p,
            offset: 5,
            fragment: Fragment::text(" big", bold.clone()),
        },
        Op::DeleteInline {
            para: p,
            offset: 2,
            len: 5,
        },
        Op::SetRuns {
            para: p,
            offset: 1,
            runs: vec![RunSpan::new(3, bold.clone())],
        },
        Op::AddMark {
            para: p,
            start: 3,
            end: 9,
            mark: Mark::Insertion(RevisionInfo::new(1, "Ada", None)),
        },
        Op::RemoveMark {
            para: p,
            start: 0,
            end: 2,
            mark: Mark::Comment(cid),
        },
        Op::SetMarks {
            para: p,
            marks: Vec::new(),
        },
        Op::SetParagraphProps {
            para: p,
            props: Box::new(ParagraphProps {
                alignment: Some(Alignment::Center),
                ..ParagraphProps::default()
            }),
        },
        Op::SetParagraphMarkProps {
            para: p,
            props: Box::new(bold.clone()),
        },
        Op::SetParagraphRevision {
            para: p,
            revision: Box::new(ParagraphRevision {
                mark: Some(InsDelRevision {
                    info: RevisionInfo::new(2, "Ada", None),
                    kind: crate::revision::InsDel::Insertion,
                }),
                ..ParagraphRevision::default()
            }),
        },
        Op::split_paragraph(&doc, p, 4, new_para)?,
        Op::InsertBlocks {
            container: section,
            index: 1,
            blocks: vec![blk],
        },
        Op::RemoveBlocks {
            container: section,
            index: 1,
            count: 2,
        },
        Op::SetTableProps {
            table: table_id,
            props: Box::new(TableProps {
                style_id: Some("TableGrid".into()),
                ..TableProps::default()
            }),
        },
        Op::SetTableGrid {
            table: table_id,
            grid: vec![100, 200, 300],
        },
        Op::InsertRows {
            table: table_id,
            index: 1,
            rows: vec![new_row],
        },
        Op::RemoveRows {
            table: table_id,
            index: 0,
            count: 1,
        },
        Op::SetRow {
            row: t.rows[0].id,
            props: Box::new(TableRowProps {
                header: Some(true),
                ..TableRowProps::default()
            }),
            revision: None,
        },
        Op::InsertCells {
            row: t.rows[1].id,
            index: 2,
            cells: vec![new_cell],
        },
        Op::RemoveCells {
            row: t.rows[0].id,
            index: 0,
            count: 1,
        },
        Op::SetCell {
            cell: t.rows[0].cells[0].id,
            props: Box::new(TableCellProps::default()),
            grid_span: 2,
            v_merge: Some(VMerge::Restart),
        },
        Op::SetSectionProps {
            section,
            props: Box::new(SectionProps::new(Paper::A4)),
        },
        Op::SetSectionRefs {
            section,
            headers: HeaderFooterRefs::default(),
            footers: HeaderFooterRefs::default(),
        },
        Op::SplitSection {
            section,
            block_index: 2,
            new_section,
            props: Box::new(SectionProps::new(Paper::Letter)),
            headers: HeaderFooterRefs::default(),
            footers: HeaderFooterRefs::default(),
        },
        Op::InsertHeaderFooter {
            index: 0,
            story: Box::new(hf),
        },
        Op::InsertNote {
            index: 1,
            note: Box::new(note),
        },
        Op::InsertComment {
            index: 1,
            comment: Box::new(reply),
        },
        Op::SetCommentMeta {
            id: cid,
            meta: CommentMeta {
                author: "Ada".into(),
                resolved: true,
                ..CommentMeta::default()
            },
        },
        Op::PutStyle {
            style: Box::new(Style::new("Mine", StyleType::Paragraph, "Mine")),
            index: 3,
        },
        Op::PutStyle {
            style: Box::new(Style::new("Normal", StyleType::Paragraph, "Normal")),
            index: 0,
        },
        Op::DeleteStyle {
            id: "Heading2".into(),
        },
        Op::SetDocDefaults {
            defaults: Box::default(),
        },
        Op::SetAbstractNum {
            id: 4,
            def: Some(Box::new(AbstractNum {
                id: 4,
                levels: vec![Level::new(0, NumberFormat::Decimal, "%1.")],
                ..AbstractNum::default()
            })),
        },
        Op::SetNumInstance {
            id: 3,
            instance: Some(NumInstance {
                id: 3,
                abstract_id: 4,
                overrides: Vec::new(),
            }),
        },
        Op::SetSettings {
            settings: Box::new(Settings {
                track_revisions: true,
                ..Settings::default()
            }),
        },
        Op::SetProperties {
            properties: Box::new(DocProperties {
                title: Some("T".into()),
                ..DocProperties::default()
            }),
        },
        Op::SetTheme {
            theme: Box::new(DocTheme {
                name: "Other".into(),
                ..DocTheme::office()
            }),
        },
        Op::PutMedia {
            name: "word/media/image1.png".into(),
            media: Some(Media {
                content_type: "image/png".into(),
                data: vec![1, 2, 3],
            }),
        },
    ];
    for op in ops {
        let label = format!("{op:?}");
        round_trip(&mut doc, op).map_err(|e| DocError::InvalidOp(format!("{label}: {e}")))?;
    }
    Ok(())
}

#[test]
fn split_and_merge_carry_the_second_paragraph() -> Result<(), DocError> {
    let mut doc = rich()?;
    let p = first_body_para(&doc);
    let id = doc.alloc_id();
    let inv = doc.apply_op(Op::SplitParagraph {
        para: p,
        offset: 5,
        new_para: id,
        props: Box::new(ParagraphProps {
            keep_next: Some(true),
            ..ParagraphProps::default()
        }),
        mark_props: Box::default(),
        revision: Box::default(),
    })?;
    assert_eq!(
        doc.paragraph(p).map(Paragraph::text).as_deref(),
        Some("Hello")
    );
    assert_eq!(
        doc.paragraph(id).and_then(|x| x.props.keep_next),
        Some(true)
    );
    let back = doc.apply_op(inv)?;
    assert!(
        matches!(back, Op::SplitParagraph { new_para, ref props, .. } if new_para == id && props.keep_next == Some(true))
    );
    // Merging non-neighbours is refused.
    let other = doc.body_paragraphs()[2].id;
    assert!(
        doc.apply_op(Op::MergeParagraphs {
            first: p,
            second: other
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn invalid_ops_fail_and_change_nothing() -> Result<(), DocError> {
    let mut doc = rich()?;
    let before = doc.clone();
    let p = first_body_para(&doc);
    let section = doc.sections[0].id;
    let table = doc.sections[0].blocks[2].id();
    let hf = doc.headers_footers[0].id;
    let note = doc.notes[0].id;
    let comment = doc.comments[0].id;
    let existing = doc.sections[0].blocks[1].id();
    let dup_para = Paragraph::new(existing);
    let bad: Vec<Op> = vec![
        Op::InsertInline {
            para: NodeId(4242),
            offset: 0,
            fragment: Fragment::text("x", RunProps::default()),
        },
        Op::InsertInline {
            para: p,
            offset: 999,
            fragment: Fragment::text("x", RunProps::default()),
        },
        Op::InsertInline {
            para: p,
            offset: 0,
            fragment: Fragment::text("x", RunProps::default())
                .with_mark(Mark::Comment(NodeId(777))),
        },
        Op::InsertInline {
            para: p,
            offset: 0,
            fragment: Fragment::object(
                InlineObject::NoteRef(NoteRef {
                    note: NodeId(888),
                    kind: NoteKind::Footnote,
                    custom_mark: None,
                }),
                RunProps::default(),
            ),
        },
        Op::DeleteInline {
            para: p,
            offset: 3,
            len: usize::MAX,
        },
        Op::DeleteInline {
            para: table,
            offset: 0,
            len: 1,
        },
        Op::SetRuns {
            para: p,
            offset: 0,
            runs: vec![RunSpan::new(500, RunProps::default())],
        },
        Op::AddMark {
            para: p,
            start: 4,
            end: 2,
            mark: Mark::Comment(comment),
        },
        Op::AddMark {
            para: p,
            start: 0,
            end: 2,
            mark: Mark::Comment(NodeId(999)),
        },
        Op::SetMarks {
            para: p,
            marks: vec![MarkSpan::new(0, 99, Mark::Comment(comment))],
        },
        Op::SetObject {
            para: p,
            offset: 0,
            object: InlineObject::Break(Break {
                kind: BreakKind::Page,
                clear: None,
            }),
        },
        Op::SplitParagraph {
            para: p,
            offset: 2,
            new_para: existing,
            props: Box::default(),
            mark_props: Box::default(),
            revision: Box::default(),
        },
        Op::SplitParagraph {
            para: p,
            offset: 200,
            new_para: NodeId(100_000),
            props: Box::default(),
            mark_props: Box::default(),
            revision: Box::default(),
        },
        Op::InsertBlocks {
            container: section,
            index: 0,
            blocks: vec![Block::Paragraph(dup_para)],
        },
        Op::InsertBlocks {
            container: section,
            index: 0,
            blocks: Vec::new(),
        },
        Op::InsertBlocks {
            container: p,
            index: 0,
            blocks: vec![Block::Paragraph(Paragraph::new(NodeId(100_001)))],
        },
        Op::InsertBlocks {
            container: section,
            index: 99,
            blocks: vec![Block::Paragraph(Paragraph::new(NodeId(100_002)))],
        },
        Op::RemoveBlocks {
            container: section,
            index: 0,
            count: 5,
        },
        Op::RemoveBlocks {
            container: section,
            index: 4,
            count: usize::MAX,
        },
        Op::RemoveRows {
            table,
            index: 0,
            count: 2,
        },
        Op::RemoveCells {
            row: p,
            index: 0,
            count: 1,
        },
        Op::SetCell {
            cell: p,
            props: Box::default(),
            grid_span: 1,
            v_merge: None,
        },
        Op::SetCell {
            cell: NodeId(1),
            props: Box::default(),
            grid_span: 0,
            v_merge: None,
        },
        Op::SetSectionRefs {
            section,
            headers: HeaderFooterRefs {
                default: Some(doc.headers_footers[1].id),
                ..HeaderFooterRefs::default()
            },
            footers: HeaderFooterRefs::default(),
        },
        Op::SplitSection {
            section,
            block_index: 0,
            new_section: NodeId(100_003),
            props: Box::new(SectionProps::new(Paper::A4)),
            headers: HeaderFooterRefs::default(),
            footers: HeaderFooterRefs::default(),
        },
        Op::MergeSections {
            first: section,
            second: NodeId(5),
        },
        Op::RemoveHeaderFooter { id: hf },
        Op::RemoveNote { id: note },
        Op::RemoveComment { id: comment },
        Op::SetCommentMeta {
            id: comment,
            meta: CommentMeta {
                parent: Some(comment),
                ..CommentMeta::default()
            },
        },
        Op::DeleteStyle { id: "Nope".into() },
        Op::PutStyle {
            style: Box::default(),
            index: 0,
        },
        Op::SetAbstractNum {
            id: 1,
            def: Some(Box::default()),
        },
        Op::SetNumInstance {
            id: 0,
            instance: None,
        },
    ];
    for op in bad {
        let label = format!("{op:?}");
        assert!(doc.apply_op(op).is_err(), "{label} should fail");
        assert_eq!(doc, before, "{label} changed the document");
    }
    // A failing op rolls back the ops before it in the same transaction.
    let tx = Transaction::new(vec![
        Op::InsertInline {
            para: p,
            offset: 0,
            fragment: Fragment::text("fine", RunProps::default()),
        },
        Op::DeleteInline {
            para: p,
            offset: 0,
            len: 10_000,
        },
    ]);
    assert!(doc.apply(tx).is_err());
    assert_eq!(doc, before);
    Ok(())
}

#[test]
fn objects_can_be_replaced_and_drawings_need_fresh_ids() -> Result<(), DocError> {
    let mut doc = rich()?;
    let p = first_body_para(&doc);
    let field = InlineObject::Field(Field::new("PAGE", "1"));
    doc.apply_op(Op::InsertInline {
        para: p,
        offset: 0,
        fragment: Fragment::object(field, RunProps::default()),
    })?;
    round_trip(
        &mut doc,
        Op::SetObject {
            para: p,
            offset: 0,
            object: InlineObject::Field(Field::new("PAGE", "2")),
        },
    )?;
    let tb_para = para_with(&mut doc, "in a box")?;
    let mut drawing = crate::Drawing::picture(doc.alloc_id(), "word/media/a.png", 10, 10);
    drawing.text_box = Some(vec![Block::Paragraph(tb_para)]);
    let draw_id = drawing.id;
    round_trip(
        &mut doc,
        Op::InsertInline {
            para: p,
            offset: 1,
            fragment: Fragment::object(
                InlineObject::Drawing(Box::new(drawing.clone())),
                RunProps::default(),
            ),
        },
    )?;
    doc.apply_op(Op::InsertInline {
        para: p,
        offset: 1,
        fragment: Fragment::object(
            InlineObject::Drawing(Box::new(drawing.clone())),
            RunProps::default(),
        ),
    })?;
    assert_eq!(doc.node_kind(draw_id), Some(NodeKind::Drawing));
    // The text box is a container: its paragraphs take ops like any other.
    let fresh = doc.alloc_id();
    round_trip(
        &mut doc,
        Op::InsertBlocks {
            container: draw_id,
            index: 1,
            blocks: vec![Block::Paragraph(Paragraph::new(fresh))],
        },
    )?;
    // Inserting the same drawing again reuses its ids and fails.
    assert!(matches!(
        doc.apply_op(Op::InsertInline {
            para: p,
            offset: 0,
            fragment: Fragment::object(
                InlineObject::Drawing(Box::new(drawing.clone())),
                RunProps::default()
            ),
        }),
        Err(DocError::DuplicateId(_))
    ));
    // Replacing the drawing with itself (resize) keeps its ids.
    let mut bigger = drawing;
    bigger.width = 99;
    round_trip(
        &mut doc,
        Op::SetObject {
            para: p,
            offset: 1,
            object: InlineObject::Drawing(Box::new(bigger)),
        },
    )?;
    Ok(())
}

#[test]
fn inserted_ids_raise_the_allocator() -> Result<(), DocError> {
    let mut doc = Document::new();
    let section = doc.sections[0].id;
    let far = NodeId(1_000_000);
    doc.apply_op(Op::InsertBlocks {
        container: section,
        index: 0,
        blocks: vec![Block::Paragraph(Paragraph::new(far))],
    })?;
    assert!(doc.alloc_id().0 > far.0);
    doc.validate()?;
    assert!(
        doc.apply_op(Op::InsertBlocks {
            container: section,
            index: 0,
            blocks: vec![Block::Paragraph(Paragraph::new(NodeId(0)))],
        })
        .is_err()
    );
    Ok(())
}

#[test]
fn transactions_carry_selections_and_serialise() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = first_body_para(&doc);
    let before = crate::Range::caret(crate::Pos::new(p, 0));
    let after = crate::Range::caret(crate::Pos::new(p, 2));
    let tx = Transaction::new(vec![Op::InsertInline {
        para: p,
        offset: 0,
        fragment: Fragment::text("hi", RunProps::default()),
    }])
    .with_kind(EditKind::Typing)
    .with_selection(Some(before), Some(after));
    let bytes = tx.to_bytes()?;
    assert_eq!(Transaction::from_bytes(&bytes)?, tx);
    let inv = doc.apply(tx)?;
    assert_eq!(inv.selection_after, Some(before));
    assert_eq!(inv.selection_before, Some(after));
    assert_eq!(inv.kind, EditKind::Typing);
    assert_eq!(inv.ops[0].paragraph(), Some(p));
    assert_eq!(
        Op::SetSettings {
            settings: Box::default()
        }
        .paragraph(),
        None
    );
    Ok(())
}

// ------------------------------------------------------------------ property tests

/// What a random step does; interpreted against the current document so most steps are valid.
type Seed = (u8, u32, u32, u32, String);

fn pick<T: Copy>(items: &[T], n: u32) -> Option<T> {
    (!items.is_empty()).then(|| items[n as usize % items.len()])
}

fn ids_of(doc: &Document, kind: NodeKind) -> Vec<NodeId> {
    doc.all_ids()
        .into_iter()
        .filter(|id| doc.node_kind(*id) == Some(kind))
        .collect()
}

fn paragraphs(doc: &Document) -> Vec<NodeId> {
    doc.all_paragraphs().iter().map(|(_, p)| p.id).collect()
}

fn containers(doc: &Document) -> Vec<NodeId> {
    let mut out: Vec<NodeId> = doc.sections.iter().map(|s| s.id).collect();
    out.extend(doc.headers_footers.iter().map(|h| h.id));
    out.extend(doc.notes.iter().map(|n| n.id));
    out.extend(doc.comments.iter().map(|c| c.id));
    out.extend(ids_of(doc, NodeKind::Cell));
    out.extend(ids_of(doc, NodeKind::Sdt));
    out
}

fn props_variant(n: u32) -> RunProps {
    match n % 4 {
        0 => RunProps::default(),
        1 => RunProps {
            bold: Some(true),
            ..RunProps::default()
        },
        2 => RunProps {
            italic: Some(true),
            size: Some(28),
            ..RunProps::default()
        },
        _ => RunProps {
            color: Some(DocColor::rgb(0xFF0000)),
            style_id: Some("Strong".into()),
            ..RunProps::default()
        },
    }
}

fn mark_variant(doc: &Document, n: u32) -> Mark {
    let comment = doc.comments.first().map(|c| c.id);
    match (n % 5, comment) {
        (0, Some(c)) => Mark::Comment(c),
        (1, _) => Mark::Insertion(RevisionInfo::new(n % 3, "Ada", None)),
        (2, _) => Mark::Deletion(RevisionInfo::new(
            n % 3,
            "Grace",
            Some("2026-10-16T00:00:00Z"),
        )),
        (3, _) => Mark::Link(Hyperlink {
            target: LinkTarget::Url(format!("https://example.com/{}", n % 2)),
            tooltip: None,
            frame: None,
            history: true,
        }),
        _ => Mark::FormatChange {
            info: RevisionInfo::new(9, "Ada", None),
            before: Box::new(props_variant(n / 5)),
        },
    }
}

fn object_variant(doc: &mut Document, n: u32) -> InlineObject {
    match n % 6 {
        0 => InlineObject::Break(Break {
            kind: BreakKind::TextWrapping,
            clear: None,
        }),
        1 => InlineObject::Field(Field::new("PAGE \\* MERGEFORMAT", &(n % 9).to_string())),
        2 => InlineObject::Equation(Equation {
            omml: "<m:oMath><m:r><m:t>x</m:t></m:r></m:oMath>".into(),
            display: false,
        }),
        3 => InlineObject::BookmarkStart {
            name: format!("_Ref{n}"),
        },
        4 => match doc.notes.first() {
            Some(note) => InlineObject::NoteRef(NoteRef {
                note: note.id,
                kind: note.kind,
                custom_mark: None,
            }),
            None => InlineObject::BookmarkEnd { name: "b".into() },
        },
        _ => {
            let mut d =
                crate::Drawing::picture(doc.alloc_id(), "word/media/i.png", 914_400, 457_200);
            if n.is_multiple_of(2) {
                let mut tb = Paragraph::new(doc.alloc_id());
                tb.props.alignment = Some(Alignment::Center);
                d.text_box = Some(vec![Block::Paragraph(tb)]);
            }
            InlineObject::Drawing(Box::new(d))
        }
    }
}

#[allow(clippy::too_many_lines)]
fn make_op(doc: &mut Document, seed: &Seed) -> Option<Op> {
    let (kind, a, b, c, text) = seed;
    let (a, b, c) = (*a, *b, *c);
    let paras = paragraphs(doc);
    let para = pick(&paras, a)?;
    let len = doc.paragraph(para).map_or(0, |p| p.content.len());
    let off = |n: u32| n as usize % (len + 1);
    let (s, e) = {
        let x = off(b);
        let y = off(c);
        (x.min(y), x.max(y))
    };
    Some(match kind % 34 {
        0..=4 => {
            let mut fragment = Fragment::text(text, props_variant(c));
            if c % 3 == 0 {
                fragment = fragment.with_mark(mark_variant(doc, b));
            }
            Op::InsertInline {
                para,
                offset: off(b),
                fragment,
            }
        }
        5 => Op::InsertInline {
            para,
            offset: off(b),
            fragment: Fragment::object(object_variant(doc, c), props_variant(c)),
        },
        6 | 7 => Op::DeleteInline {
            para,
            offset: s,
            len: e - s,
        },
        8 => {
            if s == e {
                return None;
            }
            Op::SetRuns {
                para,
                offset: s,
                runs: vec![RunSpan::new(e - s, props_variant(c))],
            }
        }
        9 => Op::AddMark {
            para,
            start: s,
            end: e.max(s + 1),
            mark: mark_variant(doc, c),
        },
        10 => {
            let m = doc
                .paragraph(para)?
                .content
                .marks()
                .get(c as usize % 4)?
                .mark
                .clone();
            Op::RemoveMark {
                para,
                start: s,
                end: e,
                mark: m,
            }
        }
        11 => {
            let p = doc.paragraph(para)?;
            let (offset, obj) = p.content.objects_with_offsets().nth(c as usize % 3)?;
            let replacement = match obj {
                InlineObject::Field(f) => InlineObject::Field(Field {
                    result: format!("{}{}", f.result, c % 10),
                    ..f.clone()
                }),
                InlineObject::Drawing(d) => {
                    let mut d = d.clone();
                    d.width += 9525;
                    InlineObject::Drawing(d)
                }
                _ => InlineObject::Symbol(crate::Symbol {
                    font: "Wingdings".into(),
                    code: 0xF0E0,
                }),
            };
            Op::SetObject {
                para,
                offset,
                object: replacement,
            }
        }
        12 => Op::SetParagraphProps {
            para,
            props: Box::new(ParagraphProps {
                style_id: ["Heading1", "Quote", "Missing"]
                    .get(c as usize % 4)
                    .map(|s| (*s).to_owned()),
                alignment: Some(Alignment::ALL[c as usize % Alignment::ALL.len()]),
                indent: (c % 2 == 0).then_some(Indent {
                    left: Some(i32::try_from(b % 2000).unwrap_or(0)),
                    ..Indent::default()
                }),
                spacing: Some(Spacing {
                    after: Some(i32::try_from(c % 400).unwrap_or(0)),
                    ..Spacing::default()
                }),
                borders: (b % 3 == 0).then(ParagraphBorders::default),
                ..ParagraphProps::default()
            }),
        },
        13 => Op::SetParagraphMarkProps {
            para,
            props: Box::new(props_variant(c)),
        },
        14 => Op::SetParagraphRevision {
            para,
            revision: Box::new(ParagraphRevision {
                mark: (c % 2 == 0).then(|| InsDelRevision {
                    info: RevisionInfo::new(c % 5, "Ada", None),
                    kind: crate::revision::InsDel::Deletion,
                }),
                ..ParagraphRevision::default()
            }),
        },
        15..=17 => {
            let new_para = doc.alloc_id();
            Op::split_paragraph(doc, para, off(b), new_para).ok()?
        }
        18 | 19 => {
            let (container, index) = doc.parent_of(para)?;
            let next = doc
                .container(container)
                .ok()?
                .get(index + 1)?
                .as_paragraph()?
                .id;
            Op::MergeParagraphs {
                first: para,
                second: next,
            }
        }
        20 => {
            let container = pick(&containers(doc), b)?;
            let len = doc.container(container).ok()?.len();
            let mut blocks = Vec::new();
            for i in 0..=(c % 2) {
                let mut p = Paragraph::new(doc.alloc_id());
                p.content =
                    Inlines::from_fragment(Fragment::text(&format!("{text}{i}"), props_variant(i)))
                        .ok()?;
                blocks.push(Block::Paragraph(p));
            }
            if c % 5 == 0 {
                let r = row(doc, &["n1", "n2"]).ok()?;
                blocks.push(Block::Table(Table {
                    id: doc.alloc_id(),
                    props: TableProps::default(),
                    grid: vec![1000, 1000],
                    rows: vec![r],
                }));
            }
            Op::InsertBlocks {
                container,
                index: a as usize % (len + 1),
                blocks,
            }
        }
        21 => {
            let container = pick(&containers(doc), b)?;
            let len = doc.container(container).ok()?.len();
            Op::RemoveBlocks {
                container,
                index: a as usize % len.max(1),
                count: 1 + c as usize % 2,
            }
        }
        22 => {
            let table = pick(&ids_of(doc, NodeKind::Table), b)?;
            let rows = doc.table(table)?.rows.len();
            let cols = doc.table(table)?.rows.first()?.cells.len();
            let names: Vec<String> = (0..cols).map(|i| format!("r{i}")).collect();
            let refs: Vec<&str> = names.iter().map(String::as_str).collect();
            let r = row(doc, &refs).ok()?;
            Op::InsertRows {
                table,
                index: c as usize % (rows + 1),
                rows: vec![r],
            }
        }
        23 => {
            let table = pick(&ids_of(doc, NodeKind::Table), b)?;
            let rows = doc.table(table)?.rows.len();
            Op::RemoveRows {
                table,
                index: c as usize % rows,
                count: 1,
            }
        }
        24 => {
            let r = pick(&ids_of(doc, NodeKind::Row), b)?;
            let cells = doc.row(r)?.1.cells.len();
            let new = cell(doc, text).ok()?;
            Op::InsertCells {
                row: r,
                index: c as usize % (cells + 1),
                cells: vec![new],
            }
        }
        25 => {
            let r = pick(&ids_of(doc, NodeKind::Row), b)?;
            let cells = doc.row(r)?.1.cells.len();
            Op::RemoveCells {
                row: r,
                index: c as usize % cells,
                count: 1,
            }
        }
        26 => {
            let target = pick(&ids_of(doc, NodeKind::Cell), b)?;
            Op::SetCell {
                cell: target,
                props: Box::new(TableCellProps {
                    no_wrap: Some(c % 2 == 0),
                    ..TableCellProps::default()
                }),
                grid_span: 1 + c % 3,
                v_merge: (c % 4 == 0).then_some(VMerge::Restart),
            }
        }
        27 => {
            let table = pick(&ids_of(doc, NodeKind::Table), b)?;
            if c % 2 == 0 {
                Op::SetTableGrid {
                    table,
                    grid: vec![i32::try_from(c % 5000).unwrap_or(0); 1 + c as usize % 3],
                }
            } else {
                Op::SetTableProps {
                    table,
                    props: Box::new(TableProps {
                        style_id: Some("TableGrid".into()),
                        indent: Some(i32::try_from(c % 300).unwrap_or(0)),
                        ..TableProps::default()
                    }),
                }
            }
        }
        28 => {
            let section = pick(&doc.sections.iter().map(|s| s.id).collect::<Vec<_>>(), b)?;
            let blocks = doc.section(section)?.1.blocks.len();
            if blocks < 2 {
                return None;
            }
            let new_section = doc.alloc_id();
            Op::SplitSection {
                section,
                block_index: 1 + c as usize % (blocks - 1),
                new_section,
                props: Box::new(SectionProps::new(Paper::ALL[c as usize % 8])),
                headers: HeaderFooterRefs::default(),
                footers: HeaderFooterRefs::default(),
            }
        }
        29 => {
            if doc.sections.len() < 2 {
                return None;
            }
            let i = b as usize % (doc.sections.len() - 1);
            Op::MergeSections {
                first: doc.sections[i].id,
                second: doc.sections[i + 1].id,
            }
        }
        30 => {
            let section = pick(&doc.sections.iter().map(|s| s.id).collect::<Vec<_>>(), b)?;
            let header = doc
                .headers_footers
                .iter()
                .find(|h| h.part == HeaderFooterPart::Header)
                .map(|h| h.id);
            Op::SetSectionRefs {
                section,
                headers: HeaderFooterRefs {
                    default: header.filter(|_| c % 2 == 0),
                    first: header.filter(|_| c % 3 == 0),
                    even: None,
                },
                footers: HeaderFooterRefs::default(),
            }
        }
        31 => {
            let body = para_with(doc, text).ok()?;
            let meta = CommentMeta {
                author: "Ada".into(),
                parent: doc.comments.get(c as usize % 3).map(|x| x.id),
                ..CommentMeta::default()
            };
            let comment = Comment {
                id: doc.alloc_id(),
                meta,
                blocks: vec![Block::Paragraph(body)],
            };
            Op::InsertComment {
                index: b as usize % (doc.comments.len() + 1),
                comment: Box::new(comment),
            }
        }
        32 => match c % 4 {
            0 => Op::RemoveComment {
                id: pick(&doc.comments.iter().map(|x| x.id).collect::<Vec<_>>(), b)?,
            },
            1 => Op::RemoveNote {
                id: pick(&doc.notes.iter().map(|x| x.id).collect::<Vec<_>>(), b)?,
            },
            2 => Op::RemoveHeaderFooter {
                id: pick(
                    &doc.headers_footers.iter().map(|x| x.id).collect::<Vec<_>>(),
                    b,
                )?,
            },
            _ => {
                let np = para_with(doc, text).ok()?;
                Op::InsertNote {
                    index: 0,
                    note: Box::new(Note {
                        id: doc.alloc_id(),
                        kind: NoteKind::Endnote,
                        note_type: NoteType::Normal,
                        blocks: vec![Block::Paragraph(np)],
                    }),
                }
            }
        },
        _ => match c % 6 {
            0 => Op::PutStyle {
                style: Box::new(Style {
                    based_on: Some("Normal".into()),
                    ..Style::new(&format!("S{}", b % 3), StyleType::Paragraph, text)
                }),
                index: a as usize % 60,
            },
            1 => Op::DeleteStyle {
                id: doc
                    .styles
                    .iter()
                    .nth(b as usize % doc.styles.len().max(1))?
                    .id
                    .clone(),
            },
            2 => Op::SetAbstractNum {
                id: b % 3,
                def: (c % 4 != 2).then(|| {
                    Box::new(AbstractNum {
                        id: b % 3,
                        levels: vec![Level::new(0, NumberFormat::LowerRoman, "%1)")],
                        ..AbstractNum::default()
                    })
                }),
            },
            3 => Op::SetNumInstance {
                id: 1 + b % 3,
                instance: (c % 4 != 3).then(|| NumInstance {
                    id: 1 + b % 3,
                    abstract_id: b % 3,
                    overrides: Vec::new(),
                }),
            },
            4 => Op::SetSettings {
                settings: Box::new(Settings {
                    track_revisions: b % 2 == 0,
                    default_tab_stop: i32::try_from(b % 1440).unwrap_or(720),
                    ..Settings::default()
                }),
            },
            _ => Op::PutMedia {
                name: format!("word/media/{}.png", b % 3),
                media: (c % 2 == 0).then(|| Media {
                    content_type: "image/png".into(),
                    data: text.as_bytes().to_vec(),
                }),
            },
        },
    })
}

fn seeds() -> impl Strategy<Value = Vec<Seed>> {
    prop::collection::vec(
        (
            any::<u8>(),
            any::<u32>(),
            any::<u32>(),
            any::<u32>(),
            "[a-zé 日\t]{0,6}",
        ),
        1..40,
    )
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 320,
        max_shrink_iters: 2000,
        ..ProptestConfig::default()
    })]

    /// Random op sequences followed by their inverses restore the document; failed ops change
    /// nothing; every intermediate document is valid.
    #[test]
    fn apply_ops_then_inverses_is_identity(steps in seeds()) {
        let mut doc = rich().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let original = doc.clone();
        let mut inverses = Vec::new();
        for seed in &steps {
            let Some(op) = make_op(&mut doc, seed) else { continue };
            let before = doc.clone();
            match doc.apply(Transaction::new(vec![op.clone()])) {
                Ok(inv) => {
                    if let Err(e) = doc.validate() {
                        return Err(TestCaseError::fail(format!("{op:?} broke the document: {e}")));
                    }
                    inverses.push(inv);
                }
                Err(_) => prop_assert_eq!(&doc, &before, "a failed op changed the document"),
            }
        }
        for inv in inverses.iter().rev() {
            if let Err(e) = doc.apply(inv.clone()) {
                return Err(TestCaseError::fail(format!("inverse {inv:?} failed: {e}")));
            }
        }
        prop_assert_eq!(&doc, &original);
    }

    /// Serialisation round-trips after arbitrary edits, and the stored model validates.
    #[test]
    fn serialisation_round_trips(steps in seeds()) {
        let mut doc = rich().map_err(|e| TestCaseError::fail(e.to_string()))?;
        for seed in &steps {
            if let Some(op) = make_op(&mut doc, seed) {
                let _ = doc.apply(Transaction::new(vec![op]));
            }
        }
        let bytes = doc.to_bytes().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let back = Document::from_bytes(&bytes).map_err(|e| TestCaseError::fail(e.to_string()))?;
        prop_assert_eq!(&back, &doc);
        prop_assert_eq!(back.peek_next_id(), doc.peek_next_id());
    }

    /// Undo after a sequence and redo of every step reproduces both states.
    #[test]
    fn undo_stack_round_trips(steps in seeds()) {
        let mut doc = rich().map_err(|e| TestCaseError::fail(e.to_string()))?;
        let original = doc.clone();
        let mut stack = crate::UndoStack::default();
        for seed in &steps {
            if let Some(op) = make_op(&mut doc, seed) {
                let kind = if matches!(op, Op::InsertInline { .. }) { EditKind::Typing } else { EditKind::Other };
                let _ = stack.apply(&mut doc, Transaction::new(vec![op]).with_kind(kind));
            }
        }
        let edited = doc.clone();
        while stack.can_undo() {
            stack.undo(&mut doc).map_err(|e| TestCaseError::fail(e.to_string()))?;
        }
        prop_assert_eq!(&doc, &original);
        while stack.can_redo() {
            stack.redo(&mut doc).map_err(|e| TestCaseError::fail(e.to_string()))?;
        }
        prop_assert_eq!(&doc, &edited);
    }

    /// Hostile bytes never panic and are refused or yield a valid document.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in prop::collection::vec(any::<u8>(), 0..256)) {
        let mut framed = b"KDOC\x01\x00".to_vec();
        framed.extend_from_slice(&bytes);
        if let Ok(doc) = Document::from_bytes(&framed) {
            prop_assert!(doc.validate().is_ok());
        }
        let _ = Transaction::from_bytes(&bytes);
    }
}

#[test]
fn story_lookup_helpers() -> Result<(), DocError> {
    let doc = rich()?;
    let hp = doc.headers_footers[0].blocks[0].id();
    assert_eq!(doc.story_of(hp), Some(Story::HeaderFooter(0)));
    assert_eq!(doc.parent_of(hp), Some((doc.headers_footers[0].id, 0)));
    Ok(())
}

/// The generator behind the property tests reaches every kind of op it is meant to, and most
/// generated ops are valid (so the identity property is not satisfied vacuously).
#[test]
fn generator_exercises_every_op_kind() -> Result<(), DocError> {
    use proptest::strategy::ValueTree;
    use proptest::test_runner::TestRunner;
    use std::collections::BTreeMap;
    let mut runner = TestRunner::deterministic();
    let mut applied: BTreeMap<String, usize> = BTreeMap::new();
    let (mut ok, mut failed) = (0usize, 0usize);
    for _ in 0..300 {
        let steps = seeds()
            .new_tree(&mut runner)
            .map_err(|e| DocError::InvalidOp(e.to_string()))?
            .current();
        let mut doc = rich()?;
        for seed in &steps {
            let Some(op) = make_op(&mut doc, seed) else {
                continue;
            };
            let name: String = format!("{op:?}")
                .chars()
                .take_while(|c| c.is_alphanumeric())
                .collect();
            if doc.apply(Transaction::new(vec![op])).is_ok() {
                ok += 1;
                *applied.entry(name).or_default() += 1;
            } else {
                failed += 1;
            }
        }
    }
    eprintln!("applied {ok}, refused {failed}: {applied:?}");
    assert!(
        ok > failed * 2,
        "most generated ops must be valid ({ok} vs {failed})"
    );
    for kind in [
        "InsertInline",
        "DeleteInline",
        "SetRuns",
        "AddMark",
        "RemoveMark",
        "SetObject",
        "SetParagraphProps",
        "SetParagraphMarkProps",
        "SetParagraphRevision",
        "SplitParagraph",
        "MergeParagraphs",
        "InsertBlocks",
        "RemoveBlocks",
        "InsertRows",
        "RemoveRows",
        "InsertCells",
        "RemoveCells",
        "SetCell",
        "SetTableGrid",
        "SetTableProps",
        "SplitSection",
        "MergeSections",
        "SetSectionRefs",
        "InsertComment",
        "RemoveComment",
        "RemoveNote",
        "InsertNote",
        "RemoveHeaderFooter",
        "PutStyle",
        "DeleteStyle",
        "SetAbstractNum",
        "SetNumInstance",
        "SetSettings",
        "PutMedia",
    ] {
        assert!(applied.contains_key(kind), "{kind} was never applied");
    }
    Ok(())
}
