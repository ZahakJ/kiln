// SPDX-License-Identifier: AGPL-3.0-or-later
//! Edit builders, each applied and then undone to check that the inverse restores the document.

use super::*;
use crate::model::CommentMeta;
use crate::query::PlainTextOptions;
use crate::revision::{ParagraphPropsChange, RevisionInfo};
use crate::st::{Alignment, StyleType};
use crate::undo::UndoStack;

fn text(doc: &Document) -> String {
    doc.plain_text(PlainTextOptions::default())
}

fn first_para(doc: &Document) -> NodeId {
    doc.body_paragraphs()[0].id
}

/// Applies a transaction through an undo stack, checks validity, then undoes and checks that the
/// document is back to where it was, then redoes.
fn apply_checked(doc: &mut Document, tx: Transaction) -> Result<(), DocError> {
    let before = doc.clone();
    let mut stack = UndoStack::default();
    stack.apply(doc, tx)?;
    doc.validate()?;
    let after = doc.clone();
    stack.undo(doc)?;
    assert_eq!(*doc, before, "undo restores the document");
    stack.redo(doc)?;
    assert_eq!(*doc, after, "redo reapplies the edit");
    Ok(())
}

fn typed(doc: &mut Document, s: &str) -> Result<NodeId, DocError> {
    let p = first_para(doc);
    let tx = insert_text(doc, Pos::new(p, 0), s)?;
    apply_checked(doc, tx)?;
    Ok(p)
}

#[test]
fn typing_inherits_the_formatting_before_the_caret() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "Hello")?;
    let tx = format_runs(&doc, Pos::new(p, 0), Pos::new(p, 5), |r| {
        r.bold = Some(true)
    })?;
    apply_checked(&mut doc, tx)?;
    let tx = insert_text(&mut doc, Pos::new(p, 5), " world")?;
    assert_eq!(tx.kind, EditKind::Typing);
    apply_checked(&mut doc, tx)?;
    let para = doc.paragraph(p).ok_or(DocError::UnknownNode(p))?;
    assert_eq!(
        para.content.runs().len(),
        1,
        "typed text continues the bold run"
    );
    assert_eq!(text(&doc), "Hello world");
    // An empty paragraph types with its mark's formatting.
    let mut fresh = Document::new();
    let fp = first_para(&fresh);
    fresh.sections[0].blocks[0] = {
        let mut x = crate::Paragraph::new(fp);
        x.mark_props.italic = Some(true);
        Block::Paragraph(x)
    };
    let tx = insert_text(&mut fresh, Pos::new(fp, 0), "i")?;
    fresh.apply(tx)?;
    let fpara = fresh.paragraph(fp).ok_or(DocError::UnknownNode(fp))?;
    assert_eq!(fpara.content.props_at(0).and_then(|r| r.italic), Some(true));
    Ok(())
}

#[test]
fn typing_inside_a_link_extends_it_but_not_at_its_edge() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "see here now")?;
    let link = Mark::Link(crate::inline::Hyperlink {
        target: crate::inline::LinkTarget::Url("https://example.com".into()),
        tooltip: None,
        frame: None,
        history: true,
    });
    doc.apply(Transaction::new(vec![Op::AddMark {
        para: p,
        start: 4,
        end: 8,
        mark: link.clone(),
    }]))?;
    let tx = insert_text(&mut doc, Pos::new(p, 6), "XX")?;
    apply_checked(&mut doc, tx)?;
    let tx = insert_text(&mut doc, Pos::new(p, 10), "!")?;
    apply_checked(&mut doc, tx)?;
    let para = doc.paragraph(p).ok_or(DocError::UnknownNode(p))?;
    let spans: Vec<(usize, usize)> = para
        .content
        .marks()
        .iter()
        .map(|m| (m.start, m.end))
        .collect();
    assert_eq!(spans, vec![(4, 10)]);
    Ok(())
}

#[test]
fn multi_line_text_creates_paragraphs() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "end")?;
    let tx = insert_text(&mut doc, Pos::new(p, 0), "one\ntwo\r\nthree ")?;
    assert_eq!(tx.kind, EditKind::Structure);
    apply_checked(&mut doc, tx)?;
    assert_eq!(text(&doc), "one\ntwo\nthree end");
    assert!(insert_text(&mut doc, Pos::new(p, 99), "x").is_err());
    Ok(())
}

#[test]
fn deleting_within_and_across_paragraphs() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = first_para(&doc);
    let tx = insert_text(&mut doc, Pos::new(p, 0), "alpha\nbeta\ngamma\ndelta")?;
    apply_checked(&mut doc, tx)?;
    let ids: Vec<NodeId> = doc.body_paragraphs().iter().map(|x| x.id).collect();
    let tx = delete_range(&doc, Pos::new(ids[0], 1), Pos::new(ids[0], 3))?;
    apply_checked(&mut doc, tx)?;
    assert_eq!(text(&doc), "aha\nbeta\ngamma\ndelta");
    // Backwards range across three paragraphs.
    let tx = delete_range(&doc, Pos::new(ids[3], 2), Pos::new(ids[0], 2))?;
    assert_eq!(tx.kind, EditKind::Delete);
    apply_checked(&mut doc, tx)?;
    assert_eq!(text(&doc), "ahlta");
    let tx = delete_range(&doc, Pos::new(ids[0], 1), Pos::new(ids[0], 1))?;
    assert!(tx.is_empty());
    Ok(())
}

#[test]
fn deleting_across_a_table_boundary_is_refused() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "before")?;
    let tx = insert_table(&mut doc, Pos::new(p, 6), 1, 1)?;
    doc.apply(tx)?;
    let cell_para = doc.body_paragraphs()[1].id;
    assert!(delete_range(&doc, Pos::new(p, 0), Pos::new(cell_para, 0)).is_err());
    Ok(())
}

#[test]
fn enter_applies_the_next_style() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "Title text")?;
    let tx = format_paragraphs(&doc, Pos::new(p, 0), Pos::new(p, 0), |pp| {
        pp.style_id = Some("Heading1".into());
    })?;
    apply_checked(&mut doc, tx)?;
    let tx = split_paragraph(&mut doc, Pos::new(p, 10))?;
    let new_para = match &tx.ops[0] {
        Op::SplitParagraph { new_para, .. } => *new_para,
        _ => return Err(DocError::InvalidOp("expected a split".into())),
    };
    apply_checked(&mut doc, tx)?;
    let np = doc
        .paragraph(new_para)
        .ok_or(DocError::UnknownNode(new_para))?;
    assert_eq!(np.props.style_id.as_deref(), Some("Normal"));
    // Splitting in the middle keeps the style.
    let tx = split_paragraph(&mut doc, Pos::new(p, 5))?;
    apply_checked(&mut doc, tx)?;
    let mids: Vec<Option<String>> = doc
        .body_paragraphs()
        .iter()
        .map(|x| x.props.style_id.clone())
        .collect();
    assert_eq!(mids[1].as_deref(), Some("Heading1"));
    Ok(())
}

#[test]
fn toggling_bold_respects_style_bold() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "plain strong")?;
    // Ctrl+B on "plain": direct bold on.
    let tx = toggle(&doc, Pos::new(p, 0), Pos::new(p, 5), ToggleProp::Bold)?;
    apply_checked(&mut doc, tx.clone())?;
    doc.apply(tx)?;
    let para = doc.paragraph(p).ok_or(DocError::UnknownNode(p))?;
    assert_eq!(para.content.props_at(0).and_then(|r| r.bold), Some(true));
    // Again: off, direct formatting removed.
    let tx = toggle(&doc, Pos::new(p, 0), Pos::new(p, 5), ToggleProp::Bold)?;
    doc.apply(tx)?;
    let para = doc.paragraph(p).ok_or(DocError::UnknownNode(p))?;
    assert_eq!(para.content.props_at(0).and_then(|r| r.bold), None);
    // Text bold from the Strong style gets an explicit off, like <w:b w:val="0"/>.
    let tx = format_runs(&doc, Pos::new(p, 6), Pos::new(p, 12), |r| {
        r.style_id = Some("Strong".into());
    })?;
    doc.apply(tx)?;
    let tx = toggle(&doc, Pos::new(p, 6), Pos::new(p, 12), ToggleProp::Bold)?;
    doc.apply(tx)?;
    let para = doc.paragraph(p).ok_or(DocError::UnknownNode(p))?;
    assert_eq!(para.content.props_at(7).and_then(|r| r.bold), Some(false));
    assert!(!doc.resolve_run_at(para, 7).bold);
    // A mixed selection turns everything on first.
    let tx = toggle(&doc, Pos::new(p, 0), Pos::new(p, 12), ToggleProp::Italic)?;
    doc.apply(tx)?;
    let para = doc.paragraph(p).ok_or(DocError::UnknownNode(p))?;
    assert!(doc.resolve_run_at(para, 0).italic && doc.resolve_run_at(para, 8).italic);
    Ok(())
}

#[test]
fn formatting_spans_paragraphs() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = first_para(&doc);
    let tx = insert_text(&mut doc, Pos::new(p, 0), "one\ntwo\nthree")?;
    doc.apply(tx)?;
    let ids: Vec<NodeId> = doc.body_paragraphs().iter().map(|x| x.id).collect();
    let tx = format_runs(&doc, Pos::new(ids[0], 1), Pos::new(ids[2], 2), |r| {
        r.size = Some(30)
    })?;
    assert_eq!(tx.ops.len(), 3);
    apply_checked(&mut doc, tx)?;
    let tx = format_paragraphs(&doc, Pos::new(ids[0], 0), Pos::new(ids[1], 0), |pp| {
        pp.alignment = Some(Alignment::Center);
    })?;
    apply_checked(&mut doc, tx)?;
    assert_eq!(
        doc.paragraph(ids[1]).and_then(|x| x.props.alignment),
        Some(Alignment::Center)
    );
    assert_eq!(doc.paragraph(ids[2]).and_then(|x| x.props.alignment), None);
    assert_eq!(
        segments(&doc, Pos::new(ids[2], 0), Pos::new(ids[0], 0))?.len(),
        3
    );
    Ok(())
}

#[test]
fn tables_rows_and_columns() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "before after")?;
    let tx = insert_table(&mut doc, Pos::new(p, 7), 2, 3)?;
    apply_checked(&mut doc, tx)?;
    let table = doc.sections[0].blocks[1].id();
    let t = doc
        .table(table)
        .cloned()
        .ok_or(DocError::UnknownNode(table))?;
    assert_eq!((t.rows.len(), t.grid.len(), t.grid[0]), (2, 3, 3120));
    assert_eq!(t.props.style_id.as_deref(), Some("TableGrid"));
    assert_eq!(text(&doc), "before \n\t\t\n\t\t\nafter");

    let tx = insert_row(&mut doc, t.rows[0].id, true)?;
    apply_checked(&mut doc, tx)?;
    let tx = insert_column(&mut doc, table, 1, false)?;
    apply_checked(&mut doc, tx)?;
    let t2 = doc
        .table(table)
        .cloned()
        .ok_or(DocError::UnknownNode(table))?;
    assert_eq!((t2.rows.len(), t2.grid.len()), (3, 4));
    assert!(t2.rows.iter().all(|r| r.cells.len() == 4));

    // A merged cell spanning the boundary widens instead of gaining a neighbour.
    let wide = &t2.rows[0].cells[1];
    doc.apply(Transaction::new(vec![
        Op::SetCell {
            cell: wide.id,
            props: Box::new(wide.props.clone()),
            grid_span: 2,
            v_merge: None,
        },
        Op::RemoveCells {
            row: t2.rows[0].id,
            index: 2,
            count: 1,
        },
    ]))?;
    let tx = insert_column(&mut doc, table, 1, true)?;
    apply_checked(&mut doc, tx)?;
    let t3 = doc
        .table(table)
        .cloned()
        .ok_or(DocError::UnknownNode(table))?;
    assert_eq!(t3.rows[0].cells[1].grid_span, 3);
    assert_eq!(t3.grid_width(), 5);
    let tx = delete_column(&doc, table, 2)?;
    apply_checked(&mut doc, tx)?;
    let tx = delete_row(&doc, t3.rows[1].id)?;
    apply_checked(&mut doc, tx)?;
    assert_eq!(doc.table(table).map(|t| t.rows.len()), Some(2));
    assert!(insert_column(&mut doc, table, 99, false).is_err());
    assert!(delete_column(&doc, table, 99).is_err());
    assert!(insert_row(&mut doc, NodeId(9999), false).is_err());
    Ok(())
}

#[test]
fn table_at_the_start_of_a_paragraph_goes_before_it() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "text")?;
    let tx = insert_table(&mut doc, Pos::new(p, 0), 0, 0)?;
    apply_checked(&mut doc, tx)?;
    assert!(matches!(doc.sections[0].blocks[0], Block::Table(_)));
    assert_eq!(doc.sections[0].blocks.len(), 2);
    Ok(())
}

#[test]
fn comments_are_added_with_marks_and_deleted_with_replies() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = first_para(&doc);
    let tx = insert_text(&mut doc, Pos::new(p, 0), "first line\nsecond line")?;
    doc.apply(tx)?;
    let ids: Vec<NodeId> = doc.body_paragraphs().iter().map(|x| x.id).collect();
    let meta = CommentMeta {
        author: "Ada".into(),
        ..CommentMeta::default()
    };
    let (tx, cid) = add_comment(
        &mut doc,
        Pos::new(ids[0], 6),
        Pos::new(ids[1], 6),
        meta.clone(),
        "Why?",
    )?;
    apply_checked(&mut doc, tx)?;
    let reply_meta = CommentMeta {
        parent: Some(cid),
        ..meta
    };
    let (tx, _) = add_comment(
        &mut doc,
        Pos::new(ids[0], 0),
        Pos::new(ids[0], 0),
        reply_meta,
        "Because",
    )?;
    doc.apply(tx)?;
    let threads = doc.comment_threads();
    assert_eq!(threads.len(), 1);
    assert_eq!(threads[0].anchor_text.as_deref(), Some("line"));
    assert_eq!(threads[0].replies[0].text, "Because");
    // A comment with marks or replies cannot be removed on its own.
    assert!(
        doc.apply(Transaction::new(vec![Op::RemoveComment { id: cid }]))
            .is_err()
    );
    let tx = delete_comment(&doc, cid)?;
    apply_checked(&mut doc, tx)?;
    assert!(doc.comments.is_empty());
    assert!(
        doc.all_paragraphs()
            .iter()
            .all(|(_, p)| p.content.marks().is_empty())
    );
    assert!(delete_comment(&doc, cid).is_err());
    Ok(())
}

#[test]
fn section_breaks_split_the_body() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "first second")?;
    let tx = insert_section_break(&mut doc, Pos::new(p, 6), SectionBreak::Continuous)?;
    apply_checked(&mut doc, tx)?;
    assert_eq!(doc.sections.len(), 2);
    assert_eq!(doc.sections[1].props.break_type, SectionBreak::Continuous);
    assert_eq!(doc.sections[0].props.break_type, SectionBreak::NextPage);
    assert_eq!(text(&doc), "first \nsecond");
    // Inside a table it is refused (Word moves it after the table).
    let q = doc.sections[1].blocks[0].id();
    let tx = insert_table(&mut doc, Pos::new(q, 0), 1, 1)?;
    doc.apply(tx)?;
    let cell_para = doc.paragraphs_in(crate::model::Story::Body(1))[0].id;
    assert!(
        insert_section_break(&mut doc, Pos::new(cell_para, 0), SectionBreak::NextPage).is_err()
    );
    Ok(())
}

#[test]
fn accepting_and_rejecting_text_revisions() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "keep new old")?;
    let info = RevisionInfo::new(1, "Ada", None);
    doc.apply(Transaction::new(vec![
        Op::AddMark {
            para: p,
            start: 5,
            end: 8,
            mark: Mark::Insertion(info.clone()),
        },
        Op::AddMark {
            para: p,
            start: 9,
            end: 12,
            mark: Mark::Deletion(info.clone()),
        },
    ]))?;
    let revs = doc.revisions();
    assert_eq!(revs.len(), 2);
    // Accepting the insertion keeps its text and drops the mark.
    let tx_accept = resolve_revision(&doc, revs[0].kind, revs[0].location, true)?;
    let mut probe = doc.clone();
    apply_checked(&mut probe, tx_accept)?;
    assert_eq!(text(&probe), "keep new old");
    assert_eq!(probe.revisions().len(), 1);
    // Rejecting it removes the text.
    let tx = resolve_revision(&doc, revs[0].kind, revs[0].location, false)?;
    apply_checked(&mut doc, tx)?;
    assert_eq!(text(&doc), "keep  old");
    // Accept the deletion: its text goes.
    let revs = doc.revisions();
    let tx = resolve_revision(&doc, revs[0].kind, revs[0].location, true)?;
    doc.apply(tx)?;
    assert_eq!(text(&doc), "keep  ");
    assert!(doc.revisions().is_empty());
    assert!(resolve_revision(&doc, RevisionKind::Insertion, revs[0].location, true).is_err());
    Ok(())
}

#[test]
fn rejecting_format_changes_restores_the_old_formatting() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = typed(&mut doc, "word")?;
    let info = RevisionInfo::new(2, "Ada", None);
    doc.apply(Transaction::new(vec![
        Op::SetRuns {
            para: p,
            offset: 0,
            runs: vec![RunSpan::new(
                4,
                RunProps {
                    bold: Some(true),
                    ..RunProps::default()
                },
            )],
        },
        Op::AddMark {
            para: p,
            start: 0,
            end: 4,
            mark: Mark::FormatChange {
                info: info.clone(),
                before: Box::default(),
            },
        },
        Op::SetParagraphRevision {
            para: p,
            revision: Box::new(ParagraphRevision {
                props_change: Some(Box::new(ParagraphPropsChange {
                    info: info.clone(),
                    before: ParagraphProps {
                        alignment: Some(Alignment::Right),
                        ..ParagraphProps::default()
                    },
                })),
                ..ParagraphRevision::default()
            }),
        },
    ]))?;
    let revs = doc.revisions();
    assert_eq!(revs.len(), 2);
    for r in &revs {
        let tx = resolve_revision(&doc, r.kind, r.location, false)?;
        doc.apply(tx)?;
    }
    let para = doc.paragraph(p).ok_or(DocError::UnknownNode(p))?;
    assert_eq!(para.content.props_at(0).and_then(|r| r.bold), None);
    assert_eq!(para.props.alignment, Some(Alignment::Right));
    assert!(doc.revisions().is_empty());
    Ok(())
}

#[test]
fn paragraph_mark_revisions_join_paragraphs_when_removed() -> Result<(), DocError> {
    let mut doc = Document::new();
    let p = first_para(&doc);
    let tx = insert_text(&mut doc, Pos::new(p, 0), "one\ntwo")?;
    doc.apply(tx)?;
    doc.apply(Transaction::new(vec![Op::SetParagraphRevision {
        para: p,
        revision: Box::new(ParagraphRevision {
            mark: Some(crate::revision::InsDelRevision {
                info: RevisionInfo::new(3, "Ada", None),
                kind: InsDel::Insertion,
            }),
            ..ParagraphRevision::default()
        }),
    }]))?;
    let revs = doc.revisions();
    let keep = resolve_revision(&doc, revs[0].kind, revs[0].location, true)?;
    let mut probe = doc.clone();
    apply_checked(&mut probe, keep)?;
    assert_eq!(text(&probe), "one\ntwo");
    let tx = resolve_revision(&doc, revs[0].kind, revs[0].location, false)?;
    apply_checked(&mut doc, tx)?;
    assert_eq!(text(&doc), "onetwo");
    assert!(doc.styles.by_type(StyleType::Paragraph).count() > 0);
    Ok(())
}
