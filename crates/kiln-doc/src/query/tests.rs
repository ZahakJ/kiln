// SPDX-License-Identifier: AGPL-3.0-or-later
//! Queries: word counts (ported from `layout-props.test.ts`), plain text, outline, comments,
//! revisions and bookmarks. Modules `comments` and `revisions` are what the parity tracker points
//! at (`unit:kiln-doc/comments`, `unit:kiln-doc/revisions`).

use super::*;
use crate::inline::{Fragment, Inlines, MarkSpan};
use crate::model::{Cell, Comment, CommentMeta, HeaderFooter, HeaderFooterPart, Note, Row, Table};
use crate::props::{ParagraphProps, RunProps, TableCellProps, TableProps, TableRowProps};
use crate::revision::{InsDelRevision, ParagraphPropsChange, ParagraphRevision, RevisionInfo};
use crate::st::{BreakKind, NoteKind, NoteType};
use crate::{Break, DocError, Mark, NoteRef};

fn para(doc: &mut Document, text: &str) -> Result<Paragraph, DocError> {
    let mut p = Paragraph::new(doc.alloc_id());
    p.content = Inlines::from_fragment(Fragment::text(text, RunProps::default()))?;
    Ok(p)
}

fn block(doc: &mut Document, text: &str) -> Result<Block, DocError> {
    Ok(Block::Paragraph(para(doc, text)?))
}

fn table(doc: &mut Document, rows: &[&[&str]]) -> Result<Block, DocError> {
    let mut out = Vec::new();
    for r in rows {
        let mut cells = Vec::new();
        for c in *r {
            let b = block(doc, c)?;
            cells.push(Cell {
                id: doc.alloc_id(),
                props: TableCellProps::default(),
                grid_span: 1,
                v_merge: None,
                blocks: vec![b],
            });
        }
        out.push(Row {
            id: doc.alloc_id(),
            props: TableRowProps::default(),
            revision: None,
            cells,
        });
    }
    Ok(Block::Table(Table {
        id: doc.alloc_id(),
        props: TableProps::default(),
        grid: vec![1000; rows.first().map_or(1, |r| r.len())],
        rows: out,
    }))
}

#[test]
fn counts_words_like_word() {
    for (s, n) in [
        ("The quick brown fox", 4),
        ("  spaced   out  ", 2),
        ("don't stop-believing", 2),
        ("Hello — world", 2),
        ("日本語のテキスト", 8),
        ("Kiln 日本 rocks", 4),
        ("", 0),
        ("1 2 3", 3),
        ("a\u{00A0}b\u{3000}c", 3),
    ] {
        assert_eq!(count_words(s), n, "{s:?}");
    }
}

#[test]
fn counts_a_document_like_word() -> Result<(), DocError> {
    let mut doc = Document::new();
    let one = block(&mut doc, "One two three.")?;
    let empty = block(&mut doc, "")?;
    let four = block(&mut doc, "Four\tfive")?;
    let t = table(&mut doc, &[&["six", "seven eight"]])?;
    doc.sections[0].blocks = vec![one, empty, four, t];
    doc.validate()?;
    let s = doc.statistics(StatisticsOptions::default());
    assert_eq!(s.words, 8);
    assert_eq!(s.paragraphs, 4);
    assert_eq!(
        s.characters,
        "Onetwothree.Fourfivesixseveneight".chars().count()
    );
    assert_eq!(
        s.characters_with_spaces,
        "One two three.Fourfivesixseven eight".chars().count()
    );
    let sel = Document::statistics_of_text("two words");
    assert_eq!((sel.words, sel.characters, sel.paragraphs), (2, 8, 1));
    Ok(())
}

#[test]
fn excludes_deleted_revisions_and_notes_by_default() -> Result<(), DocError> {
    let mut doc = Document::new();
    let mut p = para(&mut doc, "keep removed words")?;
    p.content
        .add_mark(4, 18, Mark::Deletion(RevisionInfo::new(1, "x", None)))?;
    let note_para = block(&mut doc, "note words here")?;
    let note = Note {
        id: doc.alloc_id(),
        kind: NoteKind::Footnote,
        note_type: NoteType::Normal,
        blocks: vec![note_para],
    };
    doc.notes.push(note);
    doc.sections[0].blocks = vec![Block::Paragraph(p)];
    assert_eq!(doc.statistics(StatisticsOptions::default()).words, 1);
    assert_eq!(
        doc.statistics(StatisticsOptions {
            include_notes: true
        })
        .words,
        4
    );
    Ok(())
}

#[test]
fn plain_text_of_paragraphs_tabs_breaks_and_tables() -> Result<(), DocError> {
    let mut doc = Document::new();
    let title = block(&mut doc, "Title")?;
    let mut line = para(&mut doc, "a\tb")?;
    line.content.insert(
        3,
        Fragment::object(
            InlineObject::Break(Break {
                kind: BreakKind::TextWrapping,
                clear: None,
            }),
            RunProps::default(),
        ),
    )?;
    line.content
        .insert(4, Fragment::text("c", RunProps::default()))?;
    let t = table(&mut doc, &[&["1", "2"], &["3", "4"]])?;
    let mut tail = para(&mut doc, "kept deleted")?;
    tail.content
        .add_mark(4, 12, Mark::Deletion(RevisionInfo::new(1, "x", None)))?;
    doc.sections[0].blocks = vec![title, Block::Paragraph(line), t, Block::Paragraph(tail)];
    let all = doc.plain_text(PlainTextOptions::default());
    assert_eq!(all, "Title\na\tb\nc\n1\t2\n3\t4\nkept deleted");
    let accepted = doc.plain_text(PlainTextOptions {
        accept_changes: true,
        ..PlainTextOptions::default()
    });
    assert!(accepted.ends_with("kept"));
    Ok(())
}

#[test]
fn plain_text_omits_headers_and_notes_unless_asked() -> Result<(), DocError> {
    let mut doc = Document::new();
    let head = block(&mut doc, "Head")?;
    let hf = HeaderFooter {
        id: doc.alloc_id(),
        part: HeaderFooterPart::Header,
        blocks: vec![head],
    };
    let hf_id = hf.id;
    doc.headers_footers.push(hf);
    doc.sections[0].headers.default = Some(hf_id);
    let body = block(&mut doc, "Body")?;
    doc.sections[0].blocks = vec![body];
    let note_block = block(&mut doc, "Note")?;
    let note_id = doc.alloc_id();
    doc.notes.push(Note {
        id: note_id,
        kind: NoteKind::Footnote,
        note_type: NoteType::Normal,
        blocks: vec![note_block],
    });
    doc.validate()?;
    assert_eq!(doc.plain_text(PlainTextOptions::default()), "Body");
    assert_eq!(
        doc.plain_text(PlainTextOptions {
            include_headers_footers: true,
            include_notes: true,
            ..PlainTextOptions::default()
        }),
        "Head\nBody\nNote"
    );
    Ok(())
}

#[test]
fn plain_text_with_list_labels_and_outline() -> Result<(), DocError> {
    let mut doc = Document::new();
    let id = doc.numbering.create_numbered_list().id;
    let mut item = para(&mut doc, "First")?;
    item.props.numbering = Some(crate::props::NumberingRef::new(id, 0));
    let mut heading = para(&mut doc, "Intro")?;
    heading.props.style_id = Some("Heading2".into());
    doc.sections[0].blocks = vec![Block::Paragraph(heading), Block::Paragraph(item)];
    let text = doc.plain_text(PlainTextOptions {
        list_labels: true,
        ..PlainTextOptions::default()
    });
    assert_eq!(text, "Intro\n1.\tFirst");
    let outline = doc.outline();
    assert_eq!(outline.len(), 1);
    assert_eq!((outline[0].level, outline[0].text.as_str()), (1, "Intro"));
    Ok(())
}

#[test]
fn bookmarks_and_their_text() -> Result<(), DocError> {
    let mut doc = Document::new();
    let mut p = para(&mut doc, "See Introduction here")?;
    p.content.insert(
        4,
        Fragment::object(
            InlineObject::BookmarkStart { name: "bm".into() },
            RunProps::default(),
        ),
    )?;
    p.content.insert(
        17,
        Fragment::object(
            InlineObject::BookmarkEnd { name: "bm".into() },
            RunProps::default(),
        ),
    )?;
    let mut open = para(&mut doc, "x")?;
    open.content.insert(
        0,
        Fragment::object(
            InlineObject::BookmarkStart {
                name: "open".into(),
            },
            RunProps::default(),
        ),
    )?;
    let after = block(&mut doc, "after")?;
    doc.sections[0].blocks = vec![Block::Paragraph(p), Block::Paragraph(open), after];
    let marks = doc.bookmarks();
    assert_eq!(marks.len(), 2);
    assert_eq!((marks[0].name.as_str(), marks[0].offset), ("bm", 4));
    assert_eq!(doc.bookmark_text("bm").as_deref(), Some("Introduction"));
    assert_eq!(doc.bookmark_text("open").as_deref(), Some("x\nafter"));
    assert_eq!(doc.bookmark_text("missing"), None);
    Ok(())
}

pub(super) mod comments {
    use super::*;

    fn comment(
        doc: &mut Document,
        author: &str,
        text: &str,
        parent: Option<NodeId>,
    ) -> Result<Comment, DocError> {
        let body = block(doc, text)?;
        Ok(Comment {
            id: doc.alloc_id(),
            meta: CommentMeta {
                author: author.into(),
                initials: None,
                date: Some("2026-10-16T09:00:00Z".into()),
                parent,
                resolved: false,
            },
            blocks: vec![body],
        })
    }

    #[test]
    fn threads_group_replies_under_their_comment() -> Result<(), DocError> {
        let mut doc = Document::new();
        let root = comment(&mut doc, "Ada", "Check this", None)?;
        let root_id = root.id;
        doc.comments.push(root);
        let reply = comment(&mut doc, "Grace", "Done", Some(root_id))?;
        let reply_id = reply.id;
        doc.comments.push(reply);
        let nested = comment(&mut doc, "Ada", "Thanks", Some(reply_id))?;
        doc.comments.push(nested);
        let lone = comment(&mut doc, "Linus", "Typo?", None)?;
        doc.comments.push(lone);
        let mut p = para(&mut doc, "Some commented text")?;
        p.content.add_mark(5, 14, Mark::Comment(root_id))?;
        let pid = p.id;
        doc.sections[0].blocks = vec![Block::Paragraph(p)];
        doc.validate()?;
        let threads = doc.comment_threads();
        assert_eq!(threads.len(), 2);
        let t = &threads[0];
        assert_eq!((t.author.as_str(), t.text.as_str()), ("Ada", "Check this"));
        assert_eq!(t.anchor_text.as_deref(), Some("commented"));
        assert_eq!(t.anchor_para, Some(pid));
        assert_eq!(t.replies.len(), 1);
        assert_eq!(t.replies[0].author, "Grace");
        assert_eq!(t.replies[0].replies[0].text, "Thanks");
        assert_eq!(threads[1].anchor_text, None);
        Ok(())
    }

    #[test]
    fn comment_marks_must_point_at_comments() -> Result<(), DocError> {
        let mut doc = Document::new();
        let mut p = para(&mut doc, "text")?;
        p.content.add_mark(0, 4, Mark::Comment(NodeId(999)))?;
        doc.sections[0].blocks = vec![Block::Paragraph(p)];
        assert!(doc.validate().is_err());
        let _ = MarkSpan::new(0, 1, Mark::Comment(NodeId(1)));
        Ok(())
    }
}

pub(super) mod revisions {
    use super::*;

    #[test]
    fn lists_every_kind_of_tracked_change_in_order() -> Result<(), DocError> {
        let mut doc = Document::new();
        let info = |id: u32| RevisionInfo::new(id, "Ada", Some("2026-10-16T09:00:00Z"));
        let mut p = para(&mut doc, "inserted deleted moved")?;
        p.content.add_mark(0, 8, Mark::Insertion(info(1)))?;
        p.content.add_mark(9, 16, Mark::Deletion(info(2)))?;
        p.content.add_mark(17, 22, Mark::MoveTo(info(3)))?;
        p.content.add_mark(
            0,
            8,
            Mark::FormatChange {
                info: info(4),
                before: Box::default(),
            },
        )?;
        p.revision = ParagraphRevision {
            props_change: Some(Box::new(ParagraphPropsChange {
                info: info(5),
                before: ParagraphProps::default(),
            })),
            mark: Some(InsDelRevision {
                info: info(6),
                kind: InsDel::Insertion,
            }),
            mark_props_change: Some(Box::new(crate::revision::RunPropsChange {
                info: info(8),
                before: RunProps::default(),
            })),
        };
        let mut t = table(&mut doc, &[&["cell"]])?;
        if let Block::Table(tb) = &mut t {
            tb.rows[0].revision = Some(InsDelRevision {
                info: info(7),
                kind: InsDel::Deletion,
            });
        }
        doc.sections[0].blocks = vec![Block::Paragraph(p), t];
        let kinds: Vec<RevisionKind> = doc.revisions().iter().map(|r| r.kind).collect();
        assert_eq!(
            kinds,
            vec![
                RevisionKind::ParagraphFormat,
                RevisionKind::ParagraphMarkFormat,
                RevisionKind::Insertion,
                RevisionKind::RunFormat,
                RevisionKind::Deletion,
                RevisionKind::MoveTo,
                RevisionKind::ParagraphInsertion,
                RevisionKind::RowDeletion,
            ]
        );
        let ins = &doc.revisions()[2];
        assert_eq!(ins.text.as_deref(), Some("inserted"));
        assert_eq!(ins.info.author, "Ada");
        assert!(matches!(
            ins.location,
            RevisionLocation::Text {
                start: 0,
                end: 8,
                ..
            }
        ));
        Ok(())
    }

    #[test]
    fn note_references_count_as_links_to_notes() -> Result<(), DocError> {
        let mut doc = Document::new();
        let nb = block(&mut doc, "note")?;
        let note = Note {
            id: doc.alloc_id(),
            kind: NoteKind::Footnote,
            note_type: NoteType::Normal,
            blocks: vec![nb],
        };
        let note_id = note.id;
        doc.notes.push(note);
        let mut p = para(&mut doc, "x")?;
        p.content.insert(
            1,
            Fragment::object(
                InlineObject::NoteRef(NoteRef {
                    note: note_id,
                    kind: NoteKind::Footnote,
                    custom_mark: None,
                }),
                RunProps::default(),
            ),
        )?;
        doc.sections[0].blocks = vec![Block::Paragraph(p)];
        doc.validate()?;
        doc.notes.clear();
        assert!(doc.validate().is_err());
        Ok(())
    }
}

#[test]
fn all_paragraphs_cover_every_story() -> Result<(), DocError> {
    let mut doc = Document::new();
    let head = block(&mut doc, "Head")?;
    let hf_id = doc.alloc_id();
    doc.headers_footers.push(HeaderFooter {
        id: hf_id,
        part: HeaderFooterPart::Footer,
        blocks: vec![head],
    });
    let all = doc.all_paragraphs();
    assert_eq!(all.len(), 2);
    assert!(matches!(all[1].0, Story::HeaderFooter(0)));
    Ok(())
}
