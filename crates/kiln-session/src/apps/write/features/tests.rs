// SPDX-License-Identifier: AGPL-3.0-or-later
//! The advanced features against the model: comments, track changes, tables of contents, notes, captions and
//! cross-references, floating objects, Format Painter, AutoFormat, Paste Markdown, code blocks,
//! callouts, cover pages, document options and spelling — each through the commands the ribbon,
//! panes and shortcuts send, with undo, and saved to `.docx` and read back where the feature
//! lives in the file.

use kiln_common::NodeId;
use kiln_doc::revision::InsDel;
use kiln_doc::{Block, InlineObject, Mark, Placement};
use kiln_protocol::commands::write_features::{
    AutoFormatOptions, CalloutKind, CaptionPosition, CaptionSpec, ChangeScope, CoverDesign,
    CrossRefShow, CrossRefSpec, CrossRefTarget, MarkupKind, MarkupMode, NoteOptionsSpec,
    ObjectAlign, ObjectLayoutSpec, PositionPreset, PropertiesEdit, RelativeTo, RevisionKindInfo,
    RotateOp, TocSpec, WrapMode, ZOrder,
};
use kiln_protocol::commands::{EditCommand, FormatState, GoToTarget, WriteQuery, WriteQueryResult};
use kiln_protocol::geom::{Point, Rect};
use kiln_protocol::{Command, FileFormat, SelectUnit, SuiteCommand, TextPos, WriteCommand};

use crate::apps::write::WriteDoc;
use crate::model::{Change, DocModel};

// ----- helpers -------------------------------------------------------------------------------

fn doc() -> WriteDoc {
    WriteDoc::new(kiln_doc::Document::new(), Vec::new(), None)
}

fn fixture(name: &str) -> WriteDoc {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/features")
        .join(name);
    let bytes = std::fs::read(path).expect("fixture");
    let read = kiln_io_docx::read_docx(&bytes).expect("reads");
    WriteDoc::new(read.document, read.warnings, Some(name.to_owned()))
}

fn wr(w: WriteCommand) -> Command {
    Command::Write(w)
}

fn ed(e: EditCommand) -> Command {
    Command::Suite(SuiteCommand::Edit(e))
}

fn run(d: &mut WriteDoc, c: Command) -> Change {
    let id = c.id();
    d.execute(&c).unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

fn w(d: &mut WriteDoc, c: WriteCommand) -> Change {
    run(d, wr(c))
}

fn fails(d: &mut WriteDoc, c: WriteCommand) -> String {
    d.execute(&wr(c))
        .expect_err("the command must fail")
        .to_string()
}

fn typ(d: &mut WriteDoc, text: &str) {
    w(
        d,
        WriteCommand::InsertText {
            text: text.to_owned(),
        },
    );
}

/// Types one character at a time (AutoFormat reacts to single keystrokes).
fn keys(d: &mut WriteDoc, text: &str) {
    for c in text.chars() {
        typ(d, &c.to_string());
    }
}

fn ids(d: &WriteDoc) -> Vec<NodeId> {
    d.body().iter().map(|p| p.id).collect()
}

/// Body paragraphs as the reader sees them (deleted text included).
fn texts(d: &WriteDoc) -> Vec<String> {
    d.body()
        .iter()
        .map(|p| p.content.plain_text(false))
        .collect()
}

/// Body paragraphs without the deleted text.
fn final_texts(d: &WriteDoc) -> Vec<String> {
    d.body()
        .iter()
        .map(|p| p.content.plain_text(true))
        .collect()
}

fn pos(d: &WriteDoc, para: usize, offset: u32) -> TextPos {
    TextPos {
        block: ids(d)[para],
        offset,
    }
}

fn caret(d: &mut WriteDoc, para: usize, offset: u32) {
    let p = pos(d, para, offset);
    w(
        d,
        WriteCommand::SelectTo {
            pos: p,
            extend: false,
        },
    );
}

fn select(d: &mut WriteDoc, a: (usize, u32), b: (usize, u32)) {
    caret(d, a.0, a.1);
    let p = pos(d, b.0, b.1);
    w(
        d,
        WriteCommand::SelectTo {
            pos: p,
            extend: true,
        },
    );
}

fn query(d: &WriteDoc, q: WriteQuery) -> WriteQueryResult {
    d.write_query(&q).expect("query")
}

fn format(d: &WriteDoc) -> FormatState {
    match query(d, WriteQuery::Format) {
        WriteQueryResult::Format(f) => *f,
        other => panic!("{other:?}"),
    }
}

fn three_paragraphs() -> WriteDoc {
    let mut d = doc();
    typ(&mut d, "The kiln glows\nWarm clay waits\nFire and form");
    d
}

fn marks(d: &WriteDoc, para: usize) -> Vec<kiln_doc::MarkSpan> {
    d.doc
        .paragraph(ids(d)[para])
        .map(|p| p.content.marks().to_vec())
        .unwrap_or_default()
}

fn undo(d: &mut WriteDoc) {
    assert!(d.undo(), "something to undo");
}

/// Saves as .docx and reads it back.
fn round_trip(d: &WriteDoc) -> kiln_doc::Document {
    let bytes = d.save(FileFormat::Docx).expect("saves");
    kiln_io_docx::read_docx(&bytes)
        .expect("reads back")
        .document
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(w, h, image::Rgba([200, 80, 40, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png");
    png
}

// ----- comments ------------------------------------------------------------------------------

#[test]
fn comments_new_reply_edit_resolve_delete_with_undo() {
    let mut d = three_paragraphs();
    w(
        &mut d,
        WriteCommand::SetReviewer {
            name: "Zoë Ångström".into(),
        },
    );
    select(&mut d, (0, 4), (0, 8));
    w(
        &mut d,
        WriteCommand::NewComment {
            text: "Check the cone.\nSecond line".into(),
        },
    );
    assert_eq!(d.doc.comments.len(), 1);
    let c = &d.doc.comments[0];
    assert_eq!(c.meta.author, "Zoë Ångström");
    assert_eq!(c.meta.initials.as_deref(), Some("ZÅ"));
    assert_eq!(c.blocks.len(), 2, "one paragraph per line");
    let id = c.id;
    assert!(
        marks(&d, 0)
            .iter()
            .any(|m| m.mark == Mark::Comment(id) && (m.start, m.end) == (4, 8))
    );
    // A collapsed caret comments on the word around it.
    caret(&mut d, 1, 6);
    w(
        &mut d,
        WriteCommand::NewComment {
            text: String::new(),
        },
    );
    let second = d.doc.comments[1].id;
    assert!(
        marks(&d, 1)
            .iter()
            .any(|m| m.mark == Mark::Comment(second) && (m.start, m.end) == (5, 9))
    );
    // Reply, edit, resolve.
    w(
        &mut d,
        WriteCommand::ReplyComment {
            parent: id,
            text: "Agreed.".into(),
        },
    );
    let reply = d.doc.comments[2].id;
    assert_eq!(d.doc.comments[2].meta.parent, Some(id));
    w(
        &mut d,
        WriteCommand::EditComment {
            id: reply,
            text: "Agreed, cone 10.".into(),
        },
    );
    w(
        &mut d,
        WriteCommand::ResolveComment {
            id: reply,
            resolved: true,
        },
    );
    assert!(
        d.doc
            .comments
            .iter()
            .find(|c| c.id == id)
            .unwrap()
            .meta
            .resolved,
        "the thread"
    );
    let WriteQueryResult::Comments(list) = query(&d, WriteQuery::Comments) else {
        panic!("comments");
    };
    assert_eq!(list.len(), 3);
    assert_eq!(list[0].id, id);
    assert_eq!(list[0].anchor_text.as_deref(), Some("kiln"));
    assert!(list[0].resolved);
    assert_eq!(list[1].parent, Some(id));
    assert_eq!(list[1].text, "Agreed, cone 10.");
    assert_eq!(list[2].id, second);
    assert_eq!(list[0].color.len(), 6);
    // Navigation: Next Comment from the start.
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::GoTo {
            target: GoToTarget::Comment { backward: false },
        },
    );
    w(&mut d, WriteCommand::SelectComment { id: second });
    assert_eq!(format(&d).features.comment, Some(second));
    // Delete the one at the caret, then all, then undo.
    w(&mut d, WriteCommand::DeleteComment { id: None });
    assert_eq!(d.doc.comments.len(), 2);
    w(&mut d, WriteCommand::DeleteAllComments);
    assert!(d.doc.comments.is_empty());
    assert!(marks(&d, 0).is_empty());
    undo(&mut d);
    assert_eq!(d.doc.comments.len(), 2);
    assert!(
        fails(
            &mut d,
            WriteCommand::EditComment {
                id: NodeId(999_999),
                text: "x".into()
            }
        )
        .contains("no comment")
    );
}

#[test]
fn comment_balloons_in_all_markup() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 3));
    w(
        &mut d,
        WriteCommand::NewComment {
            text: "First".into(),
        },
    );
    select(&mut d, (0, 4), (0, 8));
    w(
        &mut d,
        WriteCommand::NewComment {
            text: "Second, much longer text ".repeat(4),
        },
    );
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::None,
        },
    );
    d.view_synced();
    let WriteQueryResult::Comments(none) = query(&d, WriteQuery::Comments) else {
        panic!()
    };
    assert!(
        none.iter().all(|c| c.balloon.is_none()),
        "No Markup has no balloons"
    );
    // Simple Markup (Word's default) shows them, as All Markup does.
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::Simple,
        },
    );
    d.view_synced();
    let WriteQueryResult::Comments(simple) = query(&d, WriteQuery::Comments) else {
        panic!()
    };
    assert!(simple.iter().all(|c| c.balloon.is_some()));
    w(
        &mut d,
        WriteCommand::ShowMarkup {
            kind: MarkupKind::Comments,
        },
    );
    d.view_synced();
    let WriteQueryResult::Comments(hidden) = query(&d, WriteQuery::Comments) else {
        panic!()
    };
    assert!(
        hidden.iter().all(|c| c.balloon.is_none()),
        "Show Markup ▸ Comments off"
    );
    w(
        &mut d,
        WriteCommand::ShowMarkup {
            kind: MarkupKind::Comments,
        },
    );
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::All,
        },
    );
    d.view_synced();
    let WriteQueryResult::Comments(all) = query(&d, WriteQuery::Comments) else {
        panic!()
    };
    let (a, b) = (all[0].balloon.unwrap(), all[1].balloon.unwrap());
    assert!(b.y >= a.y + a.height, "stacked: {a:?} {b:?}");
    assert!(b.height > a.height, "longer text, taller balloon");
    assert_eq!(all[0].page, Some(1));
    // The current comment's connector is in the overlay.
    caret(&mut d, 0, 1);
    let overlay = d.feature_overlay();
    assert!(
        overlay
            .iter()
            .any(|op| matches!(op, kiln_scene::DrawOp::Guide { dashed: true, .. }))
    );
}

// ----- track changes -----------------------------------------------------------------------

fn tracking(d: &mut WriteDoc) {
    w(d, WriteCommand::SetReviewer { name: "Ana".into() });
    w(d, WriteCommand::TrackChanges);
    assert!(d.doc.settings.track_revisions);
    assert!(format(d).features.tracking);
}

fn insertions(d: &WriteDoc, para: usize) -> Vec<(usize, usize)> {
    marks(d, para)
        .iter()
        .filter(|m| matches!(m.mark, Mark::Insertion(_)))
        .map(|m| (m.start, m.end))
        .collect()
}

fn deletions(d: &WriteDoc, para: usize) -> Vec<(usize, usize)> {
    marks(d, para)
        .iter()
        .filter(|m| matches!(m.mark, Mark::Deletion(_)))
        .map(|m| (m.start, m.end))
        .collect()
}

#[test]
fn tracked_typing_and_deleting() {
    let mut d = three_paragraphs();
    tracking(&mut d);
    caret(&mut d, 0, 8);
    keys(&mut d, " hot");
    assert_eq!(texts(&d)[0], "The kiln hot glows");
    assert_eq!(insertions(&d, 0), vec![(8, 12)], "one change for the word");
    // Backspace over the reviewer's own insertion really deletes it.
    w(&mut d, WriteCommand::DeleteBackward);
    assert_eq!(texts(&d)[0], "The kiln ho glows");
    assert_eq!(insertions(&d, 0), vec![(8, 11)]);
    // Backspace over original text strikes it through and steps before it.
    caret(&mut d, 0, 3);
    w(&mut d, WriteCommand::DeleteBackward);
    w(&mut d, WriteCommand::DeleteBackward);
    assert_eq!(texts(&d)[0], "The kiln ho glows", "deleted text stays");
    assert_eq!(final_texts(&d)[0], "T kiln ho glows");
    assert_eq!(
        deletions(&d, 0),
        vec![(1, 3)],
        "consecutive backspaces make one deletion"
    );
    assert_eq!(d.head.offset, 1);
    // Delete (forward) steps over what it struck.
    caret(&mut d, 1, 0);
    w(&mut d, WriteCommand::DeleteForward);
    assert_eq!(deletions(&d, 1), vec![(0, 1)]);
    assert_eq!(d.head.offset, 1);
    // Typing over a selection: the old text struck, the new one after it.
    select(&mut d, (2, 0), (2, 4));
    typ(&mut d, "Smoke");
    assert_eq!(texts(&d)[2], "FireSmoke and form");
    assert_eq!(deletions(&d, 2), vec![(0, 4)]);
    assert_eq!(insertions(&d, 2), vec![(4, 9)]);
    let WriteQueryResult::Revisions(r) = query(&d, WriteQuery::Revisions) else {
        panic!()
    };
    assert_eq!(r.reviewers.len(), 1);
    assert_eq!(r.reviewers[0].name, "Ana");
    assert_eq!(r.insertions, 2);
    assert_eq!(r.deletions, 3);
    assert!(
        r.entries
            .iter()
            .any(|e| e.target.kind == RevisionKindInfo::Deleted && e.text == "Fire")
    );
    // Undo takes the changes away without leaving marks behind (typing over a selection is a
    // deletion and an insertion).
    undo(&mut d);
    undo(&mut d);
    assert_eq!(texts(&d)[2], "Fire and form");
    assert!(deletions(&d, 2).is_empty() && insertions(&d, 2).is_empty());
}

#[test]
fn tracked_paragraph_marks_formatting_and_paragraph_props() {
    let mut d = three_paragraphs();
    tracking(&mut d);
    // Enter in the middle: the split paragraph's mark is new.
    caret(&mut d, 0, 8);
    typ(&mut d, "\n");
    assert_eq!(texts(&d).len(), 4);
    let first = d.doc.paragraph(ids(&d)[0]).unwrap();
    assert_eq!(
        first.revision.mark.as_ref().map(|m| m.kind),
        Some(InsDel::Insertion)
    );
    // Joining paragraphs keeps both and marks the mark deleted.
    caret(&mut d, 2, 0);
    w(&mut d, WriteCommand::DeleteBackward);
    assert_eq!(texts(&d).len(), 4, "no join while tracking");
    let p1 = d.doc.paragraph(ids(&d)[1]).unwrap();
    assert_eq!(
        p1.revision.mark.as_ref().map(|m| m.kind),
        Some(InsDel::Deletion)
    );
    // A deletion across paragraphs.
    select(&mut d, (2, 5), (3, 4));
    w(&mut d, WriteCommand::DeleteBackward);
    assert_eq!(texts(&d).len(), 4);
    assert!(!deletions(&d, 2).is_empty() && !deletions(&d, 3).is_empty());
    // Bold records the formatting before it.
    select(&mut d, (3, 9), (3, 13));
    run(&mut d, ed(EditCommand::Bold));
    let fc = marks(&d, 3)
        .into_iter()
        .find_map(|m| match m.mark {
            Mark::FormatChange { before, .. } => Some((m.start, m.end, before)),
            _ => None,
        })
        .expect("a formatting change");
    assert_eq!((fc.0, fc.1), (9, 13));
    assert_eq!(fc.2.bold, None);
    // Paragraph formatting.
    run(&mut d, ed(EditCommand::AlignCenter));
    let p3 = d.doc.paragraph(ids(&d)[3]).unwrap();
    assert!(p3.revision.props_change.is_some());
    let WriteQueryResult::Revisions(r) = query(&d, WriteQuery::Revisions) else {
        panic!()
    };
    assert!(
        r.entries
            .iter()
            .any(|e| e.text.starts_with("Formatted: Bold"))
    );
    assert!(
        r.entries
            .iter()
            .any(|e| e.target.kind == RevisionKindInfo::ParagraphFormatted)
    );
    assert!(
        r.entries
            .iter()
            .any(|e| e.target.kind == RevisionKindInfo::ParagraphInserted)
    );
    assert!(r.formatting >= 2);
}

#[test]
fn accept_and_reject_all_restore_final_and_original_text() {
    let edited = || {
        let mut d = three_paragraphs();
        tracking(&mut d);
        caret(&mut d, 0, 14);
        keys(&mut d, " bright");
        select(&mut d, (1, 0), (1, 5));
        w(&mut d, WriteCommand::DeleteBackward);
        caret(&mut d, 2, 13);
        typ(&mut d, "\n");
        typ(&mut d, "Ash");
        select(&mut d, (2, 0), (2, 4));
        run(&mut d, ed(EditCommand::Italic));
        d
    };
    let mut accepted = edited();
    w(
        &mut accepted,
        WriteCommand::AcceptChange {
            scope: ChangeScope::All,
        },
    );
    assert_eq!(
        texts(&accepted),
        vec![
            "The kiln glows bright",
            "clay waits",
            "Fire and form",
            "Ash"
        ]
    );
    assert!(accepted.doc.revisions().is_empty());
    assert!(
        accepted.doc.settings.track_revisions,
        "All keeps tracking on"
    );
    let mut rejected = edited();
    w(
        &mut rejected,
        WriteCommand::RejectChange {
            scope: ChangeScope::AllAndStop,
        },
    );
    assert_eq!(
        texts(&rejected),
        vec!["The kiln glows", "Warm clay waits", "Fire and form"]
    );
    assert!(rejected.doc.revisions().is_empty());
    assert!(!rejected.doc.settings.track_revisions, "…and Stop Tracking");
    let p = rejected.doc.paragraph(ids(&rejected)[2]).unwrap();
    assert!(
        !rejected.doc.resolve_run_at(p, 1).italic,
        "formatting restored"
    );
    // One undo step brings every change back.
    undo(&mut rejected);
    assert_eq!(
        rejected.doc.revisions().len(),
        edited().doc.revisions().len()
    );
}

#[test]
fn accept_current_moves_to_next_and_pane_resolves_one() {
    let mut d = three_paragraphs();
    tracking(&mut d);
    caret(&mut d, 0, 3);
    typ(&mut d, "X");
    caret(&mut d, 2, 4);
    typ(&mut d, "Y");
    w(&mut d, WriteCommand::TrackChanges);
    caret(&mut d, 0, 0);
    // Next Change selects the first change.
    w(&mut d, WriteCommand::NextChange);
    assert_eq!((d.span().0.offset, d.span().1.offset), (3, 4));
    w(
        &mut d,
        WriteCommand::AcceptChange {
            scope: ChangeScope::CurrentAndNext,
        },
    );
    assert!(insertions(&d, 0).is_empty());
    assert_eq!(d.span().0.para, ids(&d)[2], "moved to the next change");
    w(&mut d, WriteCommand::PreviousChange);
    assert_eq!(
        d.span().0.para,
        ids(&d)[2],
        "wraps around to the only change left"
    );
    let WriteQueryResult::Revisions(r) = query(&d, WriteQuery::Revisions) else {
        panic!()
    };
    assert_eq!(r.entries.len(), 1);
    assert!(r.entries[0].current);
    w(
        &mut d,
        WriteCommand::ResolveRevision {
            target: r.entries[0].target,
            accept: false,
        },
    );
    assert_eq!(texts(&d)[2], "Fire and form");
    assert!(
        fails(
            &mut d,
            WriteCommand::ResolveRevision {
                target: r.entries[0].target,
                accept: true
            }
        )
        .contains("no longer")
    );
    assert!(fails(&mut d, WriteCommand::NextChange).contains("no tracked changes"));
}

#[test]
fn accept_all_shown_respects_show_markup() {
    let mut d = three_paragraphs();
    tracking(&mut d);
    caret(&mut d, 0, 3);
    typ(&mut d, "X");
    select(&mut d, (1, 0), (1, 4));
    run(&mut d, ed(EditCommand::Bold));
    w(
        &mut d,
        WriteCommand::ShowMarkup {
            kind: MarkupKind::Formatting,
        },
    );
    assert!(!format(&d).features.show_formatting);
    w(
        &mut d,
        WriteCommand::AcceptChange {
            scope: ChangeScope::AllShown,
        },
    );
    let left = d.doc.revisions();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].kind, kiln_doc::revision::RevisionKind::RunFormat);
}

#[test]
fn markup_modes_set_the_layout_and_show_change_bars() {
    let mut d = fixture("tracked-changes.docx");
    d.view_synced();
    let line_text = |d: &mut WriteDoc| -> String {
        let v = d.view_synced();
        let mut out = String::new();
        for page in 0..v.layout.pages().len() {
            for l in v.layout.page_lines(page) {
                out.push_str(l.para.src.map.text());
                out.push('\n');
            }
        }
        out
    };
    // Simple Markup (the default): the final text, a bar beside changed lines.
    assert_eq!(format(&d).features.markup, MarkupMode::Simple);
    let simple = line_text(&mut d);
    assert!(simple.contains("Inserted words") && !simple.contains("Deleted words"));
    assert!(
        d.feature_overlay()
            .iter()
            .any(|op| matches!(op, kiln_scene::DrawOp::FillRect { rect, .. } if rect.w <= 3.0))
    );
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::All,
        },
    );
    let all = line_text(&mut d);
    assert!(all.contains("Inserted words") && all.contains("Deleted words"));
    assert!(d.view().unwrap().layout.options().markup);
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::Original,
        },
    );
    let original = line_text(&mut d);
    assert!(!original.contains("Inserted words") && original.contains("Deleted words"));
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::None,
        },
    );
    let none = line_text(&mut d);
    assert_eq!(none, simple);
    assert!(
        d.feature_overlay()
            .iter()
            .all(|op| !matches!(op, kiln_scene::DrawOp::FillRect { .. }))
    );
}

#[test]
fn tracked_changes_and_comments_survive_a_docx_round_trip() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 4), (0, 8));
    w(
        &mut d,
        WriteCommand::NewComment {
            text: "Is it hot enough?".into(),
        },
    );
    tracking(&mut d);
    caret(&mut d, 1, 15);
    keys(&mut d, " now");
    select(&mut d, (2, 0), (2, 5));
    w(&mut d, WriteCommand::DeleteBackward);
    select(&mut d, (0, 0), (0, 3));
    run(&mut d, ed(EditCommand::Bold));
    let back = round_trip(&d);
    assert!(back.settings.track_revisions);
    assert_eq!(back.comments.len(), 1);
    assert_eq!(back.comments[0].meta.author, "Author");
    let kinds: Vec<_> = back.revisions().iter().map(|r| r.kind).collect();
    use kiln_doc::revision::RevisionKind as K;
    assert!(kinds.contains(&K::Insertion));
    assert!(kinds.contains(&K::Deletion));
    assert!(kinds.contains(&K::RunFormat));
    let reviewers: Vec<String> = back
        .revisions()
        .iter()
        .map(|r| r.info.author.clone())
        .collect();
    assert!(reviewers.iter().all(|a| a == "Ana"));
}

// ----- table of contents -----------------------------------------------------------------

fn headed_doc() -> WriteDoc {
    let mut d = doc();
    typ(
        &mut d,
        "Intro text\nGlazes\nBody about glazes\nCeladon\nMore\nFiring\nEnd",
    );
    for (para, style) in [(1, "Heading1"), (3, "Heading2"), (5, "Heading1")] {
        caret(&mut d, para, 0);
        w(
            &mut d,
            WriteCommand::ApplyStyle {
                style_id: style.into(),
            },
        );
    }
    // Firing on page 2.
    caret(&mut d, 4, 4);
    w(&mut d, WriteCommand::PageBreak);
    d
}

fn toc_entries(d: &WriteDoc) -> Vec<(String, String)> {
    let Some((_, _, id)) = super::toc::find_toc(&d.doc, false) else {
        return Vec::new();
    };
    let fb = d.doc.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Field(f) if f.id == id => Some(f.clone()),
            _ => None,
        })
        .expect("toc block");
    fb.blocks
        .iter()
        .filter_map(Block::as_paragraph)
        .map(|p| {
            let page = p
                .content
                .objects()
                .iter()
                .find_map(|o| match o {
                    InlineObject::Field(f) if f.instr.starts_with("PAGEREF") => {
                        Some(f.result.clone())
                    }
                    _ => None,
                })
                .unwrap_or_default();
            (
                p.content
                    .plain_text(true)
                    .split('\t')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_owned(),
                page,
            )
        })
        .collect()
}

#[test]
fn toc_insert_update_follow_and_remove() {
    let mut d = headed_doc();
    caret(&mut d, 0, 0);
    w(&mut d, WriteCommand::InsertQuickToc);
    assert!(format(&d).features.has_toc);
    let e = toc_entries(&d);
    assert_eq!(
        e.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        vec!["Glazes", "Celadon", "Firing"]
    );
    assert_eq!(e[0].1, "1");
    assert_eq!(e[2].1, "2", "page numbers from the layout");
    // Entries are links to the headings' bookmarks, in the TOC styles.
    let first_entry = d
        .doc
        .body_paragraphs()
        .into_iter()
        .find(|p| p.props.style_id.as_deref() == Some("TOC1"))
        .expect("TOC1 entry")
        .clone();
    assert!(first_entry
        .content
        .marks()
        .iter()
        .any(|m| matches!(&m.mark, Mark::Link(h) if matches!(&h.target, kiln_doc::LinkTarget::Bookmark(b) if b.starts_with("_Toc")))));
    assert!(
        d.doc
            .body_paragraphs()
            .iter()
            .any(|p| p.props.style_id.as_deref() == Some("TOC2"))
    );
    assert!(
        d.doc
            .body_paragraphs()
            .iter()
            .any(|p| p.props.style_id.as_deref() == Some("TOCHeading"))
    );
    // Ctrl+click on the entry goes to the heading.
    let entry_id = first_entry.id;
    w(
        &mut d,
        WriteCommand::SelectTo {
            pos: TextPos {
                block: entry_id,
                offset: 2,
            },
            extend: false,
        },
    );
    w(&mut d, WriteCommand::FollowLink);
    let heading = d
        .doc
        .body_paragraphs()
        .into_iter()
        .find(|p| p.content.plain_text(true) == "Glazes")
        .unwrap()
        .id;
    assert_eq!(d.head.para, heading);
    // A page break before Glazes moves it to page 2: Update Table (page numbers only).
    caret(&mut d, 0, 0);
    let glazes_index = ids(&d).iter().position(|id| *id == heading).unwrap();
    caret(&mut d, glazes_index, 0);
    w(&mut d, WriteCommand::PageBreak);
    w(&mut d, WriteCommand::UpdateToc { entire: false });
    let e = toc_entries(&d);
    assert_eq!(e[0].1, "2");
    // A new heading appears after Update Table ▸ entire table.
    let last = ids(&d).len() - 1;
    caret(&mut d, last, 0);
    w(
        &mut d,
        WriteCommand::ApplyStyle {
            style_id: "Heading1".into(),
        },
    );
    w(&mut d, WriteCommand::UpdateToc { entire: true });
    assert_eq!(toc_entries(&d).len(), 4);
    assert_eq!(
        super::toc::find_toc(&d.doc, false).iter().count(),
        1,
        "replaced, not added"
    );
    // Add Text.
    caret(&mut d, last, 0);
    w(&mut d, WriteCommand::AddText { level: 0 });
    w(&mut d, WriteCommand::UpdateToc { entire: true });
    assert_eq!(toc_entries(&d).len(), 3);
    // The TOC survives saving.
    let back = round_trip(&d);
    assert!(super::toc::find_toc(&back, false).is_some());
    w(&mut d, WriteCommand::RemoveToc);
    assert!(super::toc::find_toc(&d.doc, false).is_none());
    assert!(
        !d.doc
            .body_paragraphs()
            .iter()
            .any(|p| p.props.style_id.as_deref() == Some("TOCHeading"))
    );
}

#[test]
fn custom_toc_levels_leaders_and_no_page_numbers() {
    let mut d = headed_doc();
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::InsertToc {
            spec: Box::new(TocSpec {
                levels: 1,
                hyperlinks: false,
                page_numbers: false,
                right_align: false,
                leader: kiln_protocol::commands::TabLeaderKind::None,
                title: None,
            }),
        },
    );
    let e = toc_entries(&d);
    assert_eq!(
        e.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(),
        vec!["Glazes", "Firing"]
    );
    assert!(e.iter().all(|x| x.1.is_empty()));
    assert!(
        fails(
            &mut d,
            WriteCommand::InsertToc {
                spec: Box::new(TocSpec {
                    levels: 0,
                    ..TocSpec::default()
                })
            }
        )
        .contains("1 to 9")
    );
}

// ----- notes ------------------------------------------------------------------------------

#[test]
fn footnotes_insert_navigate_show_convert_and_number() {
    let mut d = three_paragraphs();
    caret(&mut d, 0, 8);
    w(&mut d, WriteCommand::InsertFootnote);
    assert_eq!(
        d.doc
            .notes
            .iter()
            .filter(|n| n.kind == kiln_doc::st::NoteKind::Footnote)
            .count(),
        1
    );
    assert!(format(&d).features.in_note, "the caret is in the new note");
    typ(&mut d, "A fact.");
    let note = d.doc.notes.last().unwrap().clone();
    let note_text = match &note.blocks[0] {
        Block::Paragraph(p) => p.content.plain_text(true),
        _ => String::new(),
    };
    assert!(note_text.ends_with(" A fact."));
    // Show Notes goes back to the reference.
    w(&mut d, WriteCommand::ShowNotes);
    assert_eq!((d.head.para, d.head.offset), (ids(&d)[0], 9));
    caret(&mut d, 2, 4);
    w(&mut d, WriteCommand::InsertFootnote);
    w(&mut d, WriteCommand::ShowNotes);
    caret(&mut d, 1, 0);
    w(&mut d, WriteCommand::InsertEndnote);
    w(&mut d, WriteCommand::ShowNotes);
    // Numbering in document order.
    let numbers = kiln_doc_layout::engine::note_numbers(&d.doc);
    let first = d.doc.notes.iter().find(|n| n.id == note.id).unwrap().id;
    assert_eq!(numbers.get(&first).map(String::as_str), Some("1"));
    // Next and previous.
    caret(&mut d, 0, 0);
    w(&mut d, WriteCommand::NextNote { backward: false });
    assert_eq!(d.head.para, ids(&d)[0]);
    w(&mut d, WriteCommand::NextNote { backward: false });
    assert_eq!(d.head.para, ids(&d)[1], "the endnote");
    w(&mut d, WriteCommand::NextNote { backward: true });
    assert_eq!(d.head.para, ids(&d)[0]);
    // Options and conversion.
    w(
        &mut d,
        WriteCommand::NoteOptions {
            spec: Box::new(NoteOptionsSpec {
                endnotes: false,
                format: "lowerRoman".into(),
                start: 3,
                restart: "continuous".into(),
            }),
        },
    );
    assert_eq!(
        kiln_doc_layout::engine::note_numbers(&d.doc)
            .get(&first)
            .map(String::as_str),
        Some("iii")
    );
    assert!(
        fails(
            &mut d,
            WriteCommand::NoteOptions {
                spec: Box::new(NoteOptionsSpec {
                    endnotes: false,
                    format: "sideways".into(),
                    start: 1,
                    restart: "continuous".into()
                })
            }
        )
        .contains("number format")
    );
    w(&mut d, WriteCommand::ConvertNotes { to_endnotes: true });
    assert!(
        d.doc
            .notes
            .iter()
            .filter(|n| n.note_type == kiln_doc::st::NoteType::Normal)
            .all(|n| n.kind == kiln_doc::st::NoteKind::Endnote)
    );
    let refs: Vec<_> = d
        .doc
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects().to_vec())
        .filter_map(|o| match o {
            InlineObject::NoteRef(n) => Some(n.kind),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 3);
    assert!(refs.iter().all(|k| *k == kiln_doc::st::NoteKind::Endnote));
    let back = round_trip(&d);
    assert_eq!(
        back.notes
            .iter()
            .filter(|n| n.note_type == kiln_doc::st::NoteType::Normal)
            .count(),
        3
    );
    undo(&mut d);
    assert!(
        d.doc
            .notes
            .iter()
            .any(|n| n.kind == kiln_doc::st::NoteKind::Footnote)
    );
}

// ----- captions and cross-references -------------------------------------------------------

fn seq_results(d: &WriteDoc) -> Vec<String> {
    d.doc
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects().to_vec())
        .filter_map(|o| match o {
            InlineObject::Field(f) if f.instr.starts_with("SEQ") => Some(f.result),
            _ => None,
        })
        .collect()
}

#[test]
fn captions_number_in_order_and_cross_references_follow() {
    let mut d = three_paragraphs();
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::InsertPicture {
            name: "a.png".into(),
            bytes: png(40, 20),
            floating: false,
        },
    );
    let spec = |text: &str| CaptionSpec {
        label: "Figure".into(),
        text: text.into(),
        position: CaptionPosition::Below,
        exclude_label: false,
        chapter_level: None,
    };
    // The picture is selected after insertion? Select it explicitly.
    let pic = d
        .doc
        .paragraph(ids(&d)[0])
        .unwrap()
        .content
        .objects()
        .iter()
        .find_map(|o| match o {
            InlineObject::Drawing(dr) => Some(dr.id),
            _ => None,
        })
        .unwrap();
    w(&mut d, WriteCommand::SelectObject { id: pic, add: None });
    w(
        &mut d,
        WriteCommand::InsertCaption {
            spec: Box::new(spec(": Tiles")),
        },
    );
    assert_eq!(texts(&d)[1].replace('\u{FFFC}', ""), "Figure 1: Tiles");
    let caption_para = ids(&d)[1];
    assert_eq!(
        d.doc
            .paragraph(caption_para)
            .unwrap()
            .props
            .style_id
            .as_deref(),
        Some("Caption")
    );
    // A caption below the last paragraph, then one above the first: renumbered in order.
    caret(&mut d, 3, 0);
    w(
        &mut d,
        WriteCommand::InsertCaption {
            spec: Box::new(spec(": Kiln")),
        },
    );
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::InsertCaption {
            spec: Box::new(CaptionSpec {
                position: CaptionPosition::Above,
                ..spec(": Cover")
            }),
        },
    );
    assert_eq!(seq_results(&d), vec!["1", "2", "3"]);
    // Cross-reference to the second caption's label and number, inserted in the last
    // paragraph.
    let n = ids(&d).len();
    caret(&mut d, n - 1, 0);
    let WriteQueryResult::CrossRefTargets(targets) = query(&d, WriteQuery::CrossRefTargets) else {
        panic!()
    };
    let figure = targets
        .iter()
        .find(|t| t.category == "Figure" && t.label.contains("Tiles"))
        .expect("caption target")
        .target
        .clone();
    w(
        &mut d,
        WriteCommand::InsertCrossReference {
            spec: Box::new(CrossRefSpec {
                target: figure.clone(),
                show: CrossRefShow::Number,
                hyperlink: true,
            }),
        },
    );
    let refs: Vec<String> = d
        .doc
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects().to_vec())
        .filter_map(|o| match o {
            InlineObject::Field(f) if f.instr.starts_with("REF") => Some(f.result),
            _ => None,
        })
        .collect();
    assert_eq!(refs, vec!["Figure 2".to_owned()]);
    // Page number and above/below references.
    w(
        &mut d,
        WriteCommand::InsertCrossReference {
            spec: Box::new(CrossRefSpec {
                target: figure.clone(),
                show: CrossRefShow::PageNumber,
                hyperlink: false,
            }),
        },
    );
    w(
        &mut d,
        WriteCommand::InsertCrossReference {
            spec: Box::new(CrossRefSpec {
                target: figure,
                show: CrossRefShow::AboveBelow,
                hyperlink: false,
            }),
        },
    );
    let results: Vec<String> = d
        .doc
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects().to_vec())
        .filter_map(|o| match o {
            InlineObject::Field(f) if f.instr.starts_with("PAGEREF") || f.instr.contains("\\p") => {
                Some(f.result)
            }
            _ => None,
        })
        .collect();
    assert_eq!(results, vec!["1".to_owned(), "above".to_owned()]);
    // Table of figures.
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::InsertTableOfFigures {
            label: "Figure".into(),
        },
    );
    assert!(super::toc::find_toc(&d.doc, true).is_some());
    assert!(
        fails(
            &mut d,
            WriteCommand::InsertCaption {
                spec: Box::new(CaptionSpec {
                    label: "Bad\"".into(),
                    ..spec("")
                })
            }
        )
        .contains("label")
    );
}

#[test]
fn cross_references_to_headings_bookmarks_and_notes() {
    let mut d = headed_doc();
    caret(&mut d, 0, 5);
    w(&mut d, WriteCommand::InsertFootnote);
    typ(&mut d, "Source.");
    select(&mut d, (2, 0), (2, 17));
    w(
        &mut d,
        WriteCommand::InsertBookmark {
            name: "glazeBody".into(),
        },
    );
    let WriteQueryResult::CrossRefTargets(t) = query(&d, WriteQuery::CrossRefTargets) else {
        panic!()
    };
    assert!(
        t.iter()
            .any(|x| x.category == "Heading" && x.label.ends_with("Celadon") && x.level == 2)
    );
    assert!(
        t.iter()
            .any(|x| x.category == "Bookmark" && x.label == "glazeBody")
    );
    let note = t
        .iter()
        .find(|x| x.category == "Footnote")
        .expect("note")
        .target
        .clone();
    let heading = t
        .iter()
        .find(|x| x.label.ends_with("Firing"))
        .unwrap()
        .target
        .clone();
    let last = ids(&d).len() - 1;
    caret(&mut d, last, 3);
    for (target, show) in [
        (heading.clone(), CrossRefShow::Text),
        (heading, CrossRefShow::PageNumber),
        (note, CrossRefShow::Number),
        (
            CrossRefTarget::Bookmark {
                name: "glazeBody".into(),
            },
            CrossRefShow::Text,
        ),
    ] {
        w(
            &mut d,
            WriteCommand::InsertCrossReference {
                spec: Box::new(CrossRefSpec {
                    target,
                    show,
                    hyperlink: true,
                }),
            },
        );
    }
    let fields: Vec<(String, String)> = d
        .doc
        .paragraph(ids(&d)[last])
        .unwrap()
        .content
        .objects()
        .iter()
        .filter_map(|o| match o {
            InlineObject::Field(f) => Some((
                f.instr.split_whitespace().next().unwrap_or("").to_owned(),
                f.result.clone(),
            )),
            _ => None,
        })
        .collect();
    assert_eq!(
        fields,
        vec![
            ("REF".to_owned(), "Firing".to_owned()),
            ("PAGEREF".to_owned(), "2".to_owned()),
            ("NOTEREF".to_owned(), "1".to_owned()),
            ("REF".to_owned(), "Body about glazes".to_owned()),
        ]
    );
    // Ctrl+click on a REF \h field follows it.
    let at_pos = pos(&d, last, 4);
    w(
        &mut d,
        WriteCommand::SelectTo {
            pos: at_pos,
            extend: false,
        },
    );
    w(&mut d, WriteCommand::FollowLink);
    assert_ne!(d.head.para, ids(&d)[last]);
    assert!(
        fails(
            &mut d,
            WriteCommand::InsertCrossReference {
                spec: Box::new(CrossRefSpec {
                    target: CrossRefTarget::Bookmark {
                        name: "nope".into()
                    },
                    show: CrossRefShow::Text,
                    hyperlink: false
                })
            }
        )
        .contains("does not exist")
    );
}

// ----- objects ----------------------------------------------------------------------------

fn floating_picture_doc() -> (WriteDoc, NodeId) {
    let mut d = three_paragraphs();
    caret(&mut d, 1, 0);
    w(
        &mut d,
        WriteCommand::InsertPicture {
            name: "pic.png".into(),
            bytes: png(96, 64),
            floating: false,
        },
    );
    let id = d
        .doc
        .paragraph(ids(&d)[1])
        .unwrap()
        .content
        .objects()
        .iter()
        .find_map(|o| match o {
            InlineObject::Drawing(dr) => Some(dr.id),
            _ => None,
        })
        .unwrap();
    w(&mut d, WriteCommand::SelectObject { id, add: None });
    (d, id)
}

fn drawing(d: &WriteDoc, id: NodeId) -> kiln_doc::Drawing {
    for (_, p) in d.doc.all_paragraphs() {
        for o in p.content.objects() {
            if let InlineObject::Drawing(x) = o
                && x.id == id
            {
                return (**x).clone();
            }
        }
    }
    panic!("no drawing {id}");
}

#[test]
fn wrap_position_layout_order_rotate_align() {
    let (mut d, id) = floating_picture_doc();
    let info = format(&d).features.object.expect("selected object");
    assert_eq!(info.id, id);
    assert_eq!(info.wrap, WrapMode::InLine);
    w(
        &mut d,
        WriteCommand::ObjectWrap {
            wrap: WrapMode::Square,
        },
    );
    assert!(
        matches!(drawing(&d, id).placement, Placement::Anchor(ref a) if a.wrap == kiln_doc::st::WrapType::Square)
    );
    w(
        &mut d,
        WriteCommand::ObjectWrap {
            wrap: WrapMode::Behind,
        },
    );
    assert_eq!(format(&d).features.object.unwrap().wrap, WrapMode::Behind);
    w(
        &mut d,
        WriteCommand::ObjectPosition {
            preset: PositionPreset::BottomRight,
        },
    );
    let Placement::Anchor(a) = drawing(&d, id).placement else {
        panic!()
    };
    assert_eq!(a.h, kiln_doc::PosH::Align(kiln_doc::st::AlignH::Right));
    assert_eq!(a.v, kiln_doc::PosV::Align(kiln_doc::st::AlignV::Bottom));
    assert_eq!(a.relative_v, kiln_doc::st::RelativeFromV::Margin);
    // The layout puts it at the bottom right of the text area.
    d.view_synced();
    let rect = format(&d).features.object.unwrap().rect.expect("laid out");
    let page = d.view().unwrap().layout.pages()[0].clone();
    assert!(
        (rect.x + rect.width - (page.origin.0 + page.body.x + page.body.w)).abs() < 2.0,
        "{rect:?}"
    );
    // The Layout dialog.
    w(
        &mut d,
        WriteCommand::ObjectLayout {
            id: None,
            spec: Box::new(ObjectLayoutSpec {
                x: Some(72.0),
                x_from: Some(RelativeTo::Page),
                y: Some(144.0),
                y_from: Some(RelativeTo::Page),
                width: Some(144.0),
                height: Some(96.0),
                name: Some("Kiln photo".into()),
                alt_text: Some("A kiln at night".into()),
                ..ObjectLayoutSpec::default()
            }),
        },
    );
    let dr = drawing(&d, id);
    assert_eq!((dr.width, dr.height), (1_828_800, 1_219_200));
    assert_eq!(dr.name, "Kiln photo");
    assert_eq!(dr.description.as_deref(), Some("A kiln at night"));
    d.view_synced();
    let r = format(&d).features.object.unwrap().rect.unwrap();
    assert!(
        (r.x - (page.origin.0 + 96.0)).abs() < 1.0 && (r.y - (page.origin.1 + 192.0)).abs() < 1.0,
        "{r:?}"
    );
    // Rotate and flip.
    w(
        &mut d,
        WriteCommand::ObjectRotate {
            op: RotateOp::Right90,
        },
    );
    w(
        &mut d,
        WriteCommand::ObjectRotate {
            op: RotateOp::FlipHorizontal,
        },
    );
    let dr = drawing(&d, id);
    assert_eq!(dr.rotation, 90 * 60_000);
    assert!(dr.flip_h);
    w(
        &mut d,
        WriteCommand::ObjectRotate {
            op: RotateOp::Left90,
        },
    );
    assert_eq!(drawing(&d, id).rotation, 0);
    // Align.
    w(
        &mut d,
        WriteCommand::ObjectAlign {
            align: ObjectAlign::Center,
            relative_to: None,
        },
    );
    let Placement::Anchor(a) = drawing(&d, id).placement else {
        panic!()
    };
    assert_eq!(a.h, kiln_doc::PosH::Align(kiln_doc::st::AlignH::Center));
    // Back in line.
    w(
        &mut d,
        WriteCommand::ObjectPosition {
            preset: PositionPreset::InLine,
        },
    );
    assert!(matches!(drawing(&d, id).placement, Placement::Inline));
    assert!(
        fails(
            &mut d,
            WriteCommand::ObjectOrder {
                order: ZOrder::Forward
            }
        )
        .contains("in line")
    );
}

#[test]
fn z_order_text_boxes_shapes_selection_pane_and_clicks() {
    let (mut d, pic) = floating_picture_doc();
    w(
        &mut d,
        WriteCommand::ObjectWrap {
            wrap: WrapMode::InFront,
        },
    );
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::InsertTextBox {
            text: Some("Pull quote".into()),
            design: None,
            rect: None,
        },
    );
    let tb = format(&d)
        .features
        .object
        .expect("the new text box is selected");
    assert_eq!(tb.kind, "TextBox");
    assert_eq!(tb.name, "Text Box 1");
    let box_d = drawing(&d, tb.id);
    let text = box_d
        .text_box
        .as_ref()
        .and_then(|b| b.first())
        .and_then(Block::as_paragraph)
        .map(|p| p.content.plain_text(true));
    assert_eq!(text.as_deref(), Some("Pull quote"));
    caret(&mut d, 2, 0);
    w(
        &mut d,
        WriteCommand::InsertShape {
            preset: "roundRect".into(),
            text: Some("Fire".into()),
            rect: None,
        },
    );
    let shape = format(&d).features.object.unwrap();
    assert_eq!(shape.kind, "TextBox", "a shape with text holds a text box");
    assert!(
        fails(
            &mut d,
            WriteCommand::InsertShape {
                preset: "teapot".into(),
                text: None,
                rect: None,
            }
        )
        .contains("preset")
    );
    // Selection pane: front first.
    let WriteQueryResult::Objects(list) = query(&d, WriteQuery::Objects) else {
        panic!()
    };
    assert_eq!(list.len(), 3);
    assert!(list.windows(2).all(|w| w[0].z >= w[1].z));
    // Send the shape to the back, bring the picture to the front.
    w(
        &mut d,
        WriteCommand::ObjectOrder {
            order: ZOrder::ToBack,
        },
    );
    w(&mut d, WriteCommand::SelectObject { id: pic, add: None });
    w(
        &mut d,
        WriteCommand::ObjectOrder {
            order: ZOrder::ToFront,
        },
    );
    let WriteQueryResult::Objects(list) = query(&d, WriteQuery::Objects) else {
        panic!()
    };
    assert_eq!(list[0].id, pic);
    assert_eq!(list[2].id, shape.id);
    w(
        &mut d,
        WriteCommand::ObjectOrder {
            order: ZOrder::Backward,
        },
    );
    let WriteQueryResult::Objects(list) = query(&d, WriteQuery::Objects) else {
        panic!()
    };
    assert_eq!(list[1].id, pic);
    // A click on the picture selects it; a drag moves it.
    d.view_synced();
    let WriteQueryResult::Objects(list) = query(&d, WriteQuery::Objects) else {
        panic!()
    };
    let r = list.iter().find(|o| o.id == pic).unwrap().rect.unwrap();
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::SelectAt {
            point: Point::new(r.x + r.width / 2.0, r.y + r.height / 2.0),
            extend: false,
            unit: SelectUnit::Char,
        },
    );
    assert_eq!(format(&d).features.object.map(|o| o.id), Some(pic));
    let moved = Rect::new(r.x + 40.0, r.y + 30.0, r.width * 2.0, r.height * 2.0);
    w(
        &mut d,
        WriteCommand::ObjectTransform {
            id: pic,
            rect: moved,
        },
    );
    d.view_synced();
    let after = format(&d).features.object.unwrap().rect.unwrap();
    assert!(
        (after.x - moved.x).abs() < 1.5 && (after.y - moved.y).abs() < 1.5,
        "{after:?} vs {moved:?}"
    );
    assert!((after.width - moved.width).abs() < 1.0);
    // Shapes and text boxes save.
    let back = round_trip(&d);
    let saved: usize = back
        .all_paragraphs()
        .iter()
        .flat_map(|(_, p)| p.content.objects().to_vec())
        .filter(|o| matches!(o, InlineObject::Drawing(_)))
        .count();
    assert_eq!(saved, 3);
}

// ----- Format Painter -----------------------------------------------------------------------

#[test]
fn format_painter_single_sticky_and_keyboard() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 4), (0, 8));
    run(&mut d, ed(EditCommand::Bold));
    run(&mut d, ed(EditCommand::Italic));
    w(&mut d, WriteCommand::FormatPainter { sticky: false });
    assert_eq!(format(&d).features.painter, Some(false));
    select(&mut d, (1, 0), (1, 4));
    w(&mut d, WriteCommand::PasteFormat);
    let r = |d: &WriteDoc, para: usize, off: usize| {
        let p = d.doc.paragraph(ids(d)[para]).unwrap();
        d.doc.resolve_run_at(p, off)
    };
    assert!(r(&d, 1, 1).bold && r(&d, 1, 1).italic);
    assert!(!r(&d, 1, 6).bold, "only the selection");
    assert_eq!(format(&d).features.painter, None, "single use");
    // Sticky: paints twice, then Esc.
    caret(&mut d, 0, 5);
    w(&mut d, WriteCommand::FormatPainter { sticky: true });
    caret(&mut d, 2, 1);
    w(&mut d, WriteCommand::PasteFormat);
    assert!(r(&d, 2, 2).bold, "a collapsed caret paints its word");
    assert!(!r(&d, 2, 6).bold);
    select(&mut d, (2, 9), (2, 13));
    w(&mut d, WriteCommand::PasteFormat);
    assert!(r(&d, 2, 10).bold);
    assert_eq!(format(&d).features.painter, Some(true));
    w(&mut d, WriteCommand::CancelPainter);
    assert_eq!(format(&d).features.painter, None);
    // Ctrl+Shift+C / Ctrl+Shift+V, with the paragraph's formatting from a caret.
    caret(&mut d, 0, 0);
    run(&mut d, ed(EditCommand::AlignCenter));
    caret(&mut d, 0, 5);
    w(&mut d, WriteCommand::CopyFormat);
    select(&mut d, (1, 0), (1, 15));
    w(&mut d, WriteCommand::PasteFormat);
    let p1 = d.doc.paragraph(ids(&d)[1]).unwrap();
    assert_eq!(
        d.doc.resolve_paragraph(p1).alignment,
        kiln_doc::st::Alignment::Center
    );
    assert!(format(&d).features.copied_format);
    undo(&mut d);
    let p1 = d.doc.paragraph(ids(&d)[1]).unwrap();
    assert_ne!(
        d.doc.resolve_paragraph(p1).alignment,
        kiln_doc::st::Alignment::Center
    );
}

// ----- AutoFormat ---------------------------------------------------------------------------

fn style_of(d: &WriteDoc, para: usize) -> String {
    let p = d.doc.paragraph(ids(d)[para]).unwrap();
    d.doc.resolve_paragraph(p).style_id
}

#[test]
fn markdown_paragraph_shortcuts() {
    let mut d = doc();
    keys(&mut d, "## Glazes");
    assert_eq!(texts(&d), vec!["Glazes"]);
    assert_eq!(style_of(&d, 0), "Heading2");
    typ(&mut d, "\n");
    keys(&mut d, "- clay");
    assert_eq!(texts(&d)[1], "clay");
    assert!(format(&d).list.is_some_and(|l| l.bullet));
    typ(&mut d, "\n");
    typ(&mut d, "\n");
    keys(&mut d, "1. first");
    assert!(format(&d).list.is_some_and(|l| !l.bullet));
    typ(&mut d, "\n");
    typ(&mut d, "\n");
    keys(&mut d, "> wise words");
    let n = ids(&d).len() - 1;
    assert_eq!(style_of(&d, n), "Quote");
    // One Ctrl+Z brings the marker back (the AutoFormat is its own step), Ctrl+Y redoes it.
    typ(&mut d, "\n");
    keys(&mut d, "# ");
    let n = ids(&d).len() - 1;
    assert_eq!(style_of(&d, n), "Heading1");
    undo(&mut d);
    assert_eq!(texts(&d)[n], "# ");
    assert_ne!(style_of(&d, n), "Heading1");
    assert!(d.redo());
    assert_eq!(texts(&d)[n], "");
    assert_eq!(style_of(&d, n), "Heading1");
    undo(&mut d);
    assert_eq!(texts(&d)[n], "# ");
    // A list shortcut too.
    typ(&mut d, "\n");
    keys(&mut d, "1. ");
    let n = ids(&d).len() - 1;
    assert!(format(&d).list.is_some());
    undo(&mut d);
    assert_eq!(texts(&d)[n], "1. ");
    assert!(format(&d).list.is_none());
}

#[test]
fn markdown_inline_shortcuts_and_rules() {
    let mut d = doc();
    keys(&mut d, "a **bold** and *it* and `code` and ~~old~~ x");
    // AutoCorrect capitalises the sentence's first word.
    assert_eq!(texts(&d), vec!["A bold and it and code and old x"]);
    let p = d.doc.paragraph(ids(&d)[0]).unwrap();
    let at = |o: usize| d.doc.resolve_run_at(p, o);
    assert!(at(3).bold);
    assert!(!at(7).bold, "typing after the closing marker is plain");
    assert!(at(12).italic);
    assert_eq!(at(19).fonts.ascii, "Consolas");
    assert!(at(28).strike);
    assert!(!at(31).strike && !at(31).bold);
    // A rule after Enter.
    typ(&mut d, "\n");
    keys(&mut d, "---");
    typ(&mut d, "\n");
    let rule = d.doc.paragraph(ids(&d)[1]).unwrap();
    assert!(rule.content.is_empty());
    assert!(
        rule.props
            .borders
            .as_ref()
            .is_some_and(|b| b.bottom.is_some())
    );
    // Turned off: nothing changes.
    w(
        &mut d,
        WriteCommand::SetAutoFormat {
            options: AutoFormatOptions {
                markdown: false,
                smart_quotes: false,
                dashes: false,
                ellipsis: false,
                fractions: false,
                replace_text: Some(false),
                capitalize_sentences: Some(false),
                capitalize_days: Some(false),
                two_initial_caps: Some(false),
            },
        },
    );
    keys(&mut d, "# **x** \"q\"");
    let n = ids(&d).len() - 1;
    assert_eq!(texts(&d)[n], "# **x** \"q\"");
}

#[test]
fn smart_typography() {
    let mut d = doc();
    keys(&mut d, "\"Hot,\" she said -- it's 1/2 done... a - b, x--y");
    assert_eq!(texts(&d), vec!["“Hot,” she said – it’s ½ done… a – b, x—y"]);
    // Ctrl+Z after a correction restores what was typed.
    typ(&mut d, "\n");
    keys(&mut d, "x...");
    undo(&mut d);
    assert_eq!(
        texts(&d)[1],
        "X...",
        "the capital stays, the ellipsis is undone"
    );
    // Not in code.
    typ(&mut d, "\n");
    keys(&mut d, "`a \"b\"`");
    assert_eq!(texts(&d)[2], "a \"b\"");
}

#[test]
fn code_blocks_from_fences_insert_and_highlight() {
    let mut d = doc();
    keys(&mut d, "```rust");
    typ(&mut d, "\n");
    assert_eq!(style_of(&d, 0), "KilnCode");
    assert_eq!(texts(&d), vec![""]);
    typ(&mut d, "let x = \"glaze\"; // ok");
    typ(&mut d, "\n");
    let line = d.doc.paragraph(ids(&d)[0]).unwrap();
    assert!(line.content.runs().len() > 2, "coloured runs");
    assert!(line.content.runs().iter().any(|r| r.props.color.is_some()));
    // Enter on the empty last line leaves the block.
    typ(&mut d, "\n");
    assert_eq!(ids(&d).len(), 2);
    assert_eq!(style_of(&d, 1), "Normal");
    w(
        &mut d,
        WriteCommand::InsertCodeBlock {
            language: Some("python".into()),
        },
    );
    assert_eq!(style_of(&d, 1), "KilnCode");
    w(
        &mut d,
        WriteCommand::InsertCallout {
            kind: CalloutKind::Warning,
        },
    );
    let callout = d.doc.paragraph(d.head.para).unwrap();
    assert!(callout.content.plain_text(true).starts_with("Warning "));
    assert!(callout.props.shading.is_some() && callout.props.borders.is_some());
    // Code saves as Kiln's Code style with its colours.
    let back = round_trip(&d);
    assert!(back.styles.contains("KilnCode"));
}

#[test]
fn paste_markdown_makes_styled_content() {
    let mut d = doc();
    w(
        &mut d,
        WriteCommand::PasteMarkdown {
            text: "# Kiln log\n\nFired **hot** today.\n\n- cone 10\n- reduction\n\n| Cone | °C |\n|---|---|\n| 6 | 1222 |\n| 10 | 1305 |\n\n```\nfn fire() {}\n```\n".into(),
        },
    );
    let paras = texts(&d);
    assert!(paras.contains(&"Kiln log".to_owned()), "{paras:?}");
    let heading = d
        .body()
        .iter()
        .find(|p| p.content.plain_text(true) == "Kiln log")
        .unwrap()
        .id;
    assert_eq!(
        d.doc
            .resolve_paragraph(d.doc.paragraph(heading).unwrap())
            .style_id,
        "Heading1"
    );
    assert!(
        d.doc.sections[0]
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Table(t) if t.rows.len() == 3
                && t.props.style_id.as_deref() == Some("GridTable4-Accent1")))
    );
    assert!(
        d.body()
            .iter()
            .any(|p| p.props.style_id.as_deref() == Some("KilnCode")
                && p.content.plain_text(true) == "fn fire() {}")
    );
    assert!(d.body().iter().any(|p| p.props.numbering.is_some()));
    // A list at the end keeps its last item in the list.
    let mut e = doc();
    w(
        &mut e,
        WriteCommand::PasteMarkdown {
            text: "# Log

- cone 6
- cone 10"
                .into(),
        },
    );
    let body = e.body();
    let last = body
        .iter()
        .find(|p| p.content.plain_text(true) == "cone 10")
        .expect("the last item");
    assert!(last.props.numbering.is_some(), "still a list item");
    // One Ctrl+Z takes the whole paste away.
    undo(&mut d);
    assert_eq!(texts(&d), vec![String::new()]);
    assert!(
        fails(&mut d, WriteCommand::PasteMarkdown { text: "  ".into() }).contains("no Markdown")
    );
}

// ----- cover pages, properties, goals, hyphenation, focus ------------------------------------

#[test]
fn cover_pages_replace_each_other() {
    let mut d = three_paragraphs();
    w(
        &mut d,
        WriteCommand::SetProperties {
            props: Box::new(PropertiesEdit {
                title: Some("Firing Notes".into()),
                author: Some("Zoë".into()),
                ..PropertiesEdit::default()
            }),
        },
    );
    for design in [
        CoverDesign::Kiln,
        CoverDesign::Banded,
        CoverDesign::Ledger,
        CoverDesign::Classic,
        CoverDesign::Slab,
        CoverDesign::Quiet,
    ] {
        w(&mut d, WriteCommand::InsertCoverPage { design });
        let covers = d.doc.sections[0].blocks.iter().filter(|b| matches!(b, Block::Sdt(s) if s.info.doc_part_gallery.as_deref() == Some("Cover Pages"))).count();
        assert_eq!(covers, 1, "{design:?} replaces the previous cover");
        assert!(matches!(&d.doc.sections[0].blocks[0], Block::Sdt(_)));
        let text: Vec<String> = d
            .doc
            .paragraphs_in(kiln_doc::Story::Body(0))
            .iter()
            .map(|p| p.content.plain_text(true))
            .collect();
        assert!(
            text.iter().any(|t| t.contains("Firing Notes")),
            "{design:?}"
        );
    }
    d.view_synced();
    assert_eq!(
        d.view().unwrap().layout.pages().len(),
        2,
        "the cover is a page of its own"
    );
    let back = round_trip(&d);
    assert!(
        back.sections[0]
            .blocks
            .iter()
            .any(|b| matches!(b, Block::Sdt(_)))
    );
    assert_eq!(back.properties.title.as_deref(), Some("Firing Notes"));
    assert_eq!(back.properties.creator.as_deref(), Some("Zoë"));
}

#[test]
fn word_goal_hyphenation_and_focus() {
    let mut d = three_paragraphs();
    w(&mut d, WriteCommand::WordGoal { words: 500 });
    assert_eq!(format(&d).features.word_goal, 500);
    let back = round_trip(&d);
    assert!(
        back.properties
            .custom
            .iter()
            .any(|c| c.name == "KilnWordCountGoal")
    );
    w(&mut d, WriteCommand::WordGoal { words: 0 });
    assert_eq!(format(&d).features.word_goal, 0);
    assert!(!format(&d).features.hyphenation);
    w(&mut d, WriteCommand::Hyphenation);
    assert!(format(&d).features.hyphenation);
    undo(&mut d);
    assert!(!format(&d).features.hyphenation);
    assert_eq!(w(&mut d, WriteCommand::FocusMode), Change::Selection);
    assert!(format(&d).features.focus);
    w(&mut d, WriteCommand::FocusMode);
    assert!(!format(&d).features.focus);
}

// ----- spelling ------------------------------------------------------------------------------

#[test]
fn spelling_flags_teh_not_the_with_suggestions_and_fixes() {
    let mut d = doc();
    typ(&mut d, "Fire teh kiln and the glaze sets.\nTeh end.");
    d.view_synced();
    let at = |d: &WriteDoc, para: usize, off: u32| match query(
        d,
        WriteQuery::SpellingAt {
            pos: pos(d, para, off),
        },
    ) {
        WriteQueryResult::Spelling(h) => h,
        other => panic!("{other:?}"),
    };
    let hit = at(&d, 0, 6).expect("teh is misspelled");
    assert_eq!(hit.word, "teh");
    assert_eq!((hit.start.offset, hit.len), (5, 3));
    assert!(hit.suggestions.iter().any(|s| s == "the"));
    assert!(at(&d, 0, 19).is_none(), "the is spelled correctly");
    // Squiggles in the overlay for the visible pages.
    let squiggles = d
        .feature_overlay()
        .iter()
        .filter(|op| matches!(op, kiln_scene::DrawOp::Squiggle { .. }))
        .count();
    assert_eq!(squiggles, 2);
    assert_eq!(format(&d).features.misspellings, 2);
    // F7: next misspelling, then Change.
    caret(&mut d, 0, 0);
    let WriteQueryResult::Spelling(Some(next)) =
        query(&d, WriteQuery::NextMisspelling { from: None })
    else {
        panic!()
    };
    assert_eq!(next.word, "teh");
    assert!(next.context.contains("Fire teh kiln"));
    w(&mut d, WriteCommand::SpellNext);
    assert_eq!((d.span().0.offset, d.span().1.offset), (5, 8));
    let at_pos = pos(&d, 0, 5);
    w(
        &mut d,
        WriteCommand::SpellReplace {
            pos: at_pos,
            with: "the".into(),
            all: false,
        },
    );
    assert_eq!(texts(&d)[0], "Fire the kiln and the glaze sets.");
    // Ignore Once, Ignore All, Add to Dictionary.
    let at_pos = pos(&d, 1, 0);
    w(&mut d, WriteCommand::SpellIgnore { pos: at_pos });
    assert!(at(&d, 1, 1).is_none());
    w(
        &mut d,
        WriteCommand::MoveCaret {
            motion: kiln_protocol::CaretMotion::DocumentEnd,
            extend: false,
        },
    );
    typ(&mut d, "\nkilnware zork");
    w(
        &mut d,
        WriteCommand::SpellAddWord {
            word: "kilnware".into(),
        },
    );
    w(
        &mut d,
        WriteCommand::SpellIgnoreAll {
            word: "zork".into(),
        },
    );
    assert!(at(&d, 2, 1).is_none() && at(&d, 2, 10).is_none());
    // Language: German text is not checked; spelling off checks nothing.
    select(&mut d, (2, 0), (2, 13));
    run(&mut d, ed(EditCommand::Cut));
    typ(&mut d, "Brennofen glasur");
    let p = ids(&d)[2];
    let len = d.doc.paragraph(p).unwrap().content.len();
    let props = kiln_doc::RunProps {
        lang: Some(kiln_doc::props::Lang {
            val: Some("de-DE".into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    d.commit(kiln_doc::Transaction::new(vec![kiln_doc::Op::SetRuns {
        para: p,
        offset: 0,
        runs: vec![kiln_doc::RunSpan::new(len, props)],
    }]))
    .unwrap();
    assert!(at(&d, 2, 2).is_none(), "unknown languages are unchecked");
    // English again in the second paragraph (its language is the document's).
    caret(&mut d, 1, 8);
    typ(&mut d, " teh");
    assert!(at(&d, 1, 10).is_some());
    w(&mut d, WriteCommand::SpellCheck { on: false });
    assert!(at(&d, 1, 10).is_none());
    assert!(
        d.feature_overlay()
            .iter()
            .all(|op| !matches!(op, kiln_scene::DrawOp::Squiggle { .. }))
    );
    // Change All (the ignored "Teh" is another word and stays).
    w(&mut d, WriteCommand::SpellCheck { on: true });
    typ(&mut d, " teh");
    let at_pos = pos(&d, 1, 9);
    w(
        &mut d,
        WriteCommand::SpellReplace {
            pos: at_pos,
            with: "the".into(),
            all: true,
        },
    );
    assert_eq!(texts(&d)[1], "Teh end. the the");
    assert!(
        fails(
            &mut d,
            WriteCommand::SpellAddWord {
                word: "two words".into()
            }
        )
        .contains("spaces")
    );
}

#[test]
fn feature_state_rides_with_the_formatting() {
    let d = three_paragraphs();
    let f = format(&d).features;
    assert!(!f.tracking);
    assert_eq!(f.author, "Author");
    assert_eq!(f.markup, MarkupMode::Simple);
    assert!(f.show_comments && f.show_insertions && f.show_formatting);
    assert!(f.spelling && f.dictionary);
    assert!(f.autoformat.markdown && f.autoformat.smart_quotes);
    assert!(f.object.is_none() && !f.in_note && !f.has_toc);
}

#[test]
#[ignore = "timing probe for the 300-page document (run with --ignored --nocapture)"]
fn probe_feature_costs_on_the_300_page_document() {
    use crate::frame::FrameState;
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/large/doc-300p.docx");
    let bytes = std::fs::read(path).expect("fixture");
    let read = kiln_io_docx::read_docx(&bytes).expect("reads");
    let mut d = WriteDoc::new(read.document, read.warnings, None);
    let vp = kiln_protocol::Viewport {
        width_px: 1200.0,
        height_px: 800.0,
        zoom: 1.0,
        dpr: 1.0,
        ..kiln_protocol::Viewport::default()
    };
    let mut frames = FrameState::new();
    for r in 1..40 {
        frames.produce(d.frame_source(), &vp, r).expect("frame");
    }
    caret(&mut d, 3, 0);
    for i in 0..5u64 {
        typ(&mut d, "k");
        let t = std::time::Instant::now();
        let _ = d.feature_state();
        let t1 = t.elapsed();
        let v = d.view.as_ref().unwrap();
        let ta = std::time::Instant::now();
        let _ = d.review_overlay(v);
        let tb = ta.elapsed();
        let _ = d.spell_overlay(v);
        let tc = ta.elapsed();
        eprintln!(
            "review {tb:?} spell {:?} visible {:?}",
            tc - tb,
            v.visible_pages()
        );
        let t2 = t.elapsed();
        d.sync_review_display();
        let t3 = t.elapsed();
        frames
            .produce(d.frame_source(), &vp, 100 + i)
            .expect("frame");
        let t4 = t.elapsed();
        eprintln!(
            "state {t1:?} overlay {:?} sync {:?} frame {:?}",
            t2 - t1,
            t3 - t2,
            t4 - t3
        );
    }
}

// ----- outline health, reviewers, pictures, drawn objects ------------------------------------

fn outline(d: &WriteDoc) -> Vec<kiln_protocol::commands::write_features::OutlineIssue> {
    match query(d, WriteQuery::OutlineIssues) {
        WriteQueryResult::OutlineIssues(v) => v,
        other => panic!("{other:?}"),
    }
}

fn styled(d: &mut WriteDoc, para: usize, style: &str) {
    caret(d, para, 0);
    w(
        d,
        WriteCommand::ApplyStyle {
            style_id: style.into(),
        },
    );
}

#[test]
fn outline_check_flags_skipped_levels_and_empty_headings() {
    use kiln_protocol::commands::write_features::OutlineIssueKind;
    let mut d = doc();
    typ(&mut d, "Firing\nLoading\nCones\n\nCooling\nBody text");
    styled(&mut d, 0, "Heading1");
    styled(&mut d, 1, "Heading3");
    styled(&mut d, 2, "Heading4");
    styled(&mut d, 3, "Heading2");
    styled(&mut d, 4, "Heading2");
    caret(&mut d, 5, 0);
    let issues = outline(&d);
    assert_eq!(issues.len(), 2, "{issues:?}");
    assert_eq!(issues[0].text, "Loading");
    assert_eq!(issues[0].level, 3);
    assert_eq!(
        issues[0].kind,
        OutlineIssueKind::SkippedLevel { expected: 2 }
    );
    // "Cones" follows on from its Heading 3; the empty Heading 2 is flagged.
    assert_eq!(issues[1].kind, OutlineIssueKind::Empty);
    assert_eq!(issues[1].para, ids(&d)[3]);
    // An empty heading at the caret is being typed: not flagged.
    caret(&mut d, 3, 0);
    assert_eq!(outline(&d).len(), 1);
    // Fixing the level clears the warning.
    styled(&mut d, 1, "Heading2");
    caret(&mut d, 5, 0);
    let issues = outline(&d);
    assert_eq!(
        issues.len(),
        2,
        "Heading 4 under the new Heading 2 skips 3: {issues:?}"
    );
    assert_eq!(issues[0].text, "Cones");
    styled(&mut d, 2, "Heading3");
    caret(&mut d, 5, 0);
    assert_eq!(outline(&d).len(), 1);
    // A first heading below level 1 skips levels too.
    let mut e = doc();
    typ(&mut e, "Start\nText");
    styled(&mut e, 0, "Heading2");
    assert_eq!(
        outline(&e)[0].kind,
        OutlineIssueKind::SkippedLevel { expected: 1 }
    );
}

#[test]
fn show_markup_for_specific_people() {
    let mut d = three_paragraphs();
    tracking(&mut d);
    caret(&mut d, 0, 3);
    typ(&mut d, " old");
    w(&mut d, WriteCommand::SetReviewer { name: "Ben".into() });
    caret(&mut d, 1, 4);
    typ(&mut d, " wet");
    select(&mut d, (2, 0), (2, 4));
    w(
        &mut d,
        WriteCommand::NewComment {
            text: "Check this".into(),
        },
    );
    w(
        &mut d,
        WriteCommand::MarkupMode {
            mode: MarkupMode::All,
        },
    );
    let WriteQueryResult::Revisions(info) = query(&d, WriteQuery::Revisions) else {
        panic!()
    };
    let names: Vec<&str> = info.reviewers.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(names, ["Ana", "Ben"]);
    let shown = |d: &mut WriteDoc| -> Vec<bool> {
        let v = d.view_synced();
        let mut flags = Vec::new();
        for page in 0..v.layout.pages().len() {
            for l in v.layout.page_lines(page) {
                flags.push(l.para.src.spec.runs.iter().any(|r| {
                    !r.text.trim().is_empty() && r.style.underline != kiln_text::UnderlineKind::None
                }));
            }
        }
        flags
    };
    assert_eq!(shown(&mut d), [true, true, false]);
    let balloons = |d: &mut WriteDoc| d.view_synced().layout.pages()[0].balloons.len();
    assert_eq!(balloons(&mut d), 1);
    // Hide Ben: his insertion shows as plain text, his comment has no balloon; the view changes
    // (the chrome asks for the formatting again).
    let change = w(
        &mut d,
        WriteCommand::ShowReviewer {
            name: Some("Ben".into()),
            show: false,
        },
    );
    assert_ne!(change, Change::None);
    assert_eq!(format(&d).features.hidden_reviewers, ["Ben"]);
    assert_eq!(shown(&mut d), [true, false, false]);
    assert_eq!(balloons(&mut d), 0);
    // Accept All Changes Shown leaves Ben's change.
    w(
        &mut d,
        WriteCommand::AcceptChange {
            scope: ChangeScope::AllShown,
        },
    );
    let left = d.doc.revisions();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].info.author, "Ben");
    // All Reviewers off hides everyone; on shows everyone again.
    w(
        &mut d,
        WriteCommand::ShowReviewer {
            name: None,
            show: false,
        },
    );
    assert_eq!(format(&d).features.hidden_reviewers, ["Ben"]);
    w(
        &mut d,
        WriteCommand::ShowReviewer {
            name: None,
            show: true,
        },
    );
    assert!(format(&d).features.hidden_reviewers.is_empty());
    assert_eq!(shown(&mut d), [false, true, false]);
    assert_eq!(balloons(&mut d), 1);
}

fn picture(d: &WriteDoc, id: NodeId) -> (String, Option<kiln_doc::Crop>, i64, i64) {
    let dr = drawing(d, id);
    match dr.kind {
        kiln_doc::DrawingKind::Picture { media, crop } => (media, crop, dr.width, dr.height),
        other => panic!("{other:?}"),
    }
}

#[test]
fn crop_aspect_reset_and_change_picture() {
    use kiln_protocol::commands::write_features::PictureCrop;
    let (mut d, id) = floating_picture_doc();
    // 96 × 64 px at 96 per inch: 914,400 × 609,600 EMU.
    assert_eq!(picture(&d, id).2, 914_400);
    w(
        &mut d,
        WriteCommand::CropPicture {
            crop: PictureCrop {
                left: 25.0,
                top: 0.0,
                right: 25.0,
                bottom: 50.0,
            },
        },
    );
    let (_, crop, w0, h0) = picture(&d, id);
    let c = crop.expect("cropped");
    assert_eq!(
        (c.left, c.right, c.top, c.bottom),
        (25_000, 25_000, 0, 50_000)
    );
    // The picture keeps its scale: the box shows half the width and half the height.
    assert_eq!((w0, h0), (457_200, 304_800));
    let info = format(&d).features.object.unwrap();
    assert_eq!(info.crop.unwrap().bottom, 50.0);
    assert!(
        fails(
            &mut d,
            WriteCommand::CropPicture {
                crop: PictureCrop {
                    left: 60.0,
                    right: 40.0,
                    ..PictureCrop::default()
                },
            },
        )
        .contains("leaving some")
    );
    // 1:1 of the whole 3:2 picture: a third cut from the width, centred.
    w(
        &mut d,
        WriteCommand::CropToAspect {
            width: 1,
            height: 1,
        },
    );
    let (_, crop, w1, h1) = picture(&d, id);
    let c = crop.unwrap();
    assert_eq!((c.left, c.right, c.top, c.bottom), (16_667, 16_667, 0, 0));
    assert!((w1 - h1).abs() <= 10, "{w1} × {h1}");
    assert_eq!(h1, 609_600);
    // Rotate, then Reset Picture: no crop, no rotation, the whole picture at its scale.
    w(
        &mut d,
        WriteCommand::ObjectRotate {
            op: RotateOp::Right90,
        },
    );
    w(&mut d, WriteCommand::ResetPicture { size: false });
    let dr = drawing(&d, id);
    assert_eq!(dr.rotation, 0);
    assert_eq!(picture(&d, id).1, None);
    assert_eq!((dr.width, dr.height), (914_400, 609_600));
    // Reset Picture & Size after a resize: the picture's own size.
    w(
        &mut d,
        WriteCommand::ObjectLayout {
            id: None,
            spec: Box::new(ObjectLayoutSpec {
                width: Some(200.0),
                height: Some(50.0),
                ..ObjectLayoutSpec::default()
            }),
        },
    );
    assert_ne!(picture(&d, id).2, 914_400);
    w(&mut d, WriteCommand::ResetPicture { size: true });
    assert_eq!(&picture(&d, id).2..=&picture(&d, id).3, &914_400..=&609_600);
    // Change Picture: a 50 × 100 image in its place, as wide, with its proportions.
    let before = picture(&d, id).0;
    w(
        &mut d,
        WriteCommand::ChangePicture {
            name: "tall.png".into(),
            bytes: png(50, 100),
        },
    );
    let (media, crop, w2, h2) = picture(&d, id);
    assert_ne!(media, before);
    assert!(d.doc.media.contains_key(&media));
    assert_eq!(crop, None);
    assert_eq!((w2, h2), (914_400, 1_828_800));
    assert_eq!(drawing(&d, id).description.as_deref(), Some("pic.png"));
    assert!(
        fails(
            &mut d,
            WriteCommand::ChangePicture {
                name: "notes.txt".into(),
                bytes: b"not a picture".to_vec(),
            },
        )
        .contains("PNG")
    );
    // One undo step each, and the crop survives a round trip.
    undo(&mut d);
    assert_eq!(picture(&d, id).0, before);
    w(
        &mut d,
        WriteCommand::CropToAspect {
            width: 16,
            height: 9,
        },
    );
    let back = round_trip(&d);
    let crop = back
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects())
        .find_map(|o| match o {
            InlineObject::Drawing(x) => match &x.kind {
                kiln_doc::DrawingKind::Picture { crop, .. } => Some(*crop),
                _ => None,
            },
            _ => None,
        })
        .flatten()
        .expect("the crop is saved");
    // 3:2 to 16:9: 1.5 / 1.778 of the height kept, 7.8 % cut from the top and the bottom.
    assert_eq!((crop.left, crop.top, crop.bottom), (0, 7_813, 7_813));
    // Not a picture.
    w(
        &mut d,
        WriteCommand::InsertShape {
            preset: "ellipse".into(),
            text: None,
            rect: None,
        },
    );
    assert!(fails(&mut d, WriteCommand::ResetPicture { size: false }).contains("not a picture"));
}

#[test]
fn text_box_gallery_drawn_shapes_and_align_to_page() {
    use kiln_protocol::commands::write_features::TextBoxDesign;
    let mut d = three_paragraphs();
    d.view_synced();
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::InsertTextBox {
            text: None,
            design: Some(TextBoxDesign::Quote),
            rect: None,
        },
    );
    let q = format(&d).features.object.unwrap();
    let qd = drawing(&d, q.id);
    let Placement::Anchor(a) = &qd.placement else {
        panic!()
    };
    assert_eq!(a.wrap, kiln_doc::st::WrapType::TopAndBottom);
    let para = qd.text_box.as_ref().unwrap()[0]
        .as_paragraph()
        .unwrap()
        .clone();
    assert!(para.content.plain_text(true).starts_with('\u{201C}'));
    assert_eq!(para.props.alignment, Some(kiln_doc::st::Alignment::Center));
    assert!(qd.xml.as_deref().unwrap().contains("<a:noFill/>"));
    w(
        &mut d,
        WriteCommand::InsertTextBox {
            text: Some("Aside".into()),
            design: Some(TextBoxDesign::Sidebar),
            rect: None,
        },
    );
    let sd = drawing(&d, format(&d).features.object.unwrap().id);
    assert!(sd.xml.as_deref().unwrap().contains("lumMod"));
    assert!(sd.height > sd.width, "a tall panel");
    // A shape drawn with the mouse over the second paragraph: its rectangle, anchored there.
    let second = ids(&d)[1];
    let v = d.view_synced();
    let page = v.layout.pages()[0].clone();
    let line = v
        .layout
        .lines_of(second)
        .first()
        .and_then(|l| v.layout.line(*l))
        .unwrap()
        .rect();
    let (x, y) = (page.origin.0 + 300.0, page.origin.1 + line.y + 4.0);
    w(
        &mut d,
        WriteCommand::InsertShape {
            preset: "ellipse".into(),
            text: None,
            // Dragged up and to the left: the same rectangle.
            rect: Some(Rect::new(x + 120.0, y + 80.0, -120.0, -80.0)),
        },
    );
    let info = format(&d).features.object.unwrap();
    assert_eq!(info.para, ids(&d)[1]);
    let r = info.rect.expect("laid out");
    assert!((r.x - x).abs() < 1.0 && (r.y - y).abs() < 1.0, "{r:?}");
    assert!((r.width - 120.0).abs() < 1.0 && (r.height - 80.0).abs() < 1.0);
    // Align Right to the page: flush with the page's right edge.
    w(
        &mut d,
        WriteCommand::ObjectAlign {
            align: ObjectAlign::Right,
            relative_to: Some(RelativeTo::Page),
        },
    );
    d.view_synced();
    let r = format(&d).features.object.unwrap().rect.unwrap();
    assert!(
        (r.x + r.width - (page.origin.0 + page.width)).abs() < 1.5,
        "{r:?}"
    );
    assert!(
        fails(
            &mut d,
            WriteCommand::ObjectAlign {
                align: ObjectAlign::Left,
                relative_to: Some(RelativeTo::ColumnOrParagraph),
            },
        )
        .contains("page or the margins")
    );
    let bad = Rect::new(f32::NAN, 0.0, 10.0, 10.0);
    assert!(
        fails(
            &mut d,
            WriteCommand::InsertShape {
                preset: "rect".into(),
                text: None,
                rect: Some(bad),
            },
        )
        .contains("not a number")
    );
}

/// The laid-out text of every body paragraph against the model's: what the canvas paints.
fn assert_laid_out_like_the_model(d: &mut WriteDoc) {
    let model: Vec<(NodeId, String)> = d
        .doc
        .body_paragraphs()
        .iter()
        .map(|p| (p.id, p.content.plain_text(true).replace('\u{FFFC}', "")))
        .collect();
    let v = d.view_synced();
    let mut laid: std::collections::HashMap<NodeId, String> = std::collections::HashMap::new();
    for page in 0..v.layout.pages().len() {
        for l in v.layout.page_lines(page) {
            laid.entry(l.para.src.id)
                .or_insert_with(|| l.para.src.map.text().trim_end_matches('\n').to_owned());
        }
    }
    for (id, text) in model {
        if let Some(shown) = laid.get(&id) {
            // Breaks are laid out as line ends.
            let plain = |t: &str| t.replace(['\u{c}', '\u{b}', '\n'], "");
            assert_eq!(
                plain(shown),
                plain(&text),
                "paragraph {id} is painted from a stale layout"
            );
        }
    }
}

#[test]
fn toc_page_numbers_are_laid_out_live_after_insert_update_and_typing() {
    let mut d = headed_doc();
    d.view_synced();
    caret(&mut d, 0, 0);
    w(&mut d, WriteCommand::InsertQuickToc);
    let entry = d
        .doc
        .body_paragraphs()
        .into_iter()
        .find(|p| p.props.style_id.as_deref() == Some("TOC1"))
        .unwrap()
        .id;
    assert_eq!(
        d.doc.paragraph(entry).unwrap().content.plain_text(true),
        "Glazes\t1"
    );
    assert_laid_out_like_the_model(&mut d);
    // A page break before the first heading: Update Table (page numbers only) repaints them.
    let glazes = d
        .doc
        .body_paragraphs()
        .into_iter()
        .find(|p| p.content.plain_text(true) == "Glazes")
        .unwrap()
        .id;
    w(
        &mut d,
        WriteCommand::SelectTo {
            pos: TextPos {
                block: glazes,
                offset: 0,
            },
            extend: false,
        },
    );
    w(&mut d, WriteCommand::PageBreak);
    w(&mut d, WriteCommand::UpdateToc { entire: false });
    assert!(
        d.doc
            .paragraph(entry)
            .unwrap()
            .content
            .plain_text(true)
            .ends_with("\t2")
    );
    assert_laid_out_like_the_model(&mut d);
    // Typing at the end, then Update Entire Table.
    let last = ids(&d).len() - 1;
    let end = u32::try_from(d.body()[last].content.len()).unwrap();
    caret(&mut d, last, end);
    typ(&mut d, "\nCleaning up\nSweep the floor.");
    let n = ids(&d).len();
    caret(&mut d, n - 2, 0);
    w(
        &mut d,
        WriteCommand::ApplyStyle {
            style_id: "Heading1".into(),
        },
    );
    w(&mut d, WriteCommand::UpdateToc { entire: true });
    assert!(
        final_texts(&d)
            .iter()
            .any(|t| t.starts_with("Cleaning up\t"))
    );
    assert_laid_out_like_the_model(&mut d);
}

#[test]
fn shape_fill_outline_text_alignment_and_margins() {
    use kiln_protocol::commands::write_features::{ShapeMargins, TextAnchor};
    let mut d = three_paragraphs();
    d.view_synced();
    caret(&mut d, 0, 0);
    w(
        &mut d,
        WriteCommand::InsertShape {
            preset: "roundRect".into(),
            text: Some("Kiln".into()),
            rect: None,
        },
    );
    let id = format(&d).features.object.unwrap().id;
    w(
        &mut d,
        WriteCommand::ShapeFill {
            color: Some("c2481c".into()),
        },
    );
    w(
        &mut d,
        WriteCommand::ShapeOutline {
            color: Some("2B1C14".into()),
        },
    );
    w(&mut d, WriteCommand::ShapeOutlineWeight { points: 3.0 });
    w(
        &mut d,
        WriteCommand::ShapeTextLayout {
            anchor: Some(TextAnchor::Bottom),
            margins: Some(ShapeMargins {
                left: 12.0,
                top: 6.0,
                right: 12.0,
                bottom: 9.0,
            }),
        },
    );
    let look = format(&d)
        .features
        .object
        .unwrap()
        .shape
        .expect("a shape's look");
    assert_eq!(look.fill.as_deref(), Some("C2481C"));
    assert_eq!(look.outline.as_deref(), Some("2B1C14"));
    assert!((look.outline_weight - 3.0).abs() < 0.01);
    assert_eq!(look.anchor, TextAnchor::Bottom);
    assert!((look.margins.bottom - 9.0).abs() < 0.01);
    // The layout paints what the XML says.
    let dr = drawing(&d, id);
    let v = kiln_doc_layout::drawing::shape_visual(&dr, 100.0, 50.0, &d.doc.theme);
    assert_eq!(v.paths[0].fill, Some(0xC2_48_1C_FF));
    let (c, wpx) = v.paths[0].stroke.unwrap();
    assert_eq!(c, 0x2B_1C_14_FF);
    assert!((wpx - 4.0).abs() < 0.01, "3 pt is 4 px");
    let (insets, anchor) = kiln_doc_layout::drawing::body_props(&dr);
    assert_eq!(anchor, "b");
    assert!((insets[0] - 16.0).abs() < 0.01, "12 pt is 16 px");
    // No Fill and No Outline; undo is one step each.
    w(&mut d, WriteCommand::ShapeFill { color: None });
    assert_eq!(
        format(&d).features.object.unwrap().shape.unwrap().fill,
        None
    );
    assert!(
        kiln_doc_layout::drawing::shape_visual(&drawing(&d, id), 100.0, 50.0, &d.doc.theme).paths
            [0]
        .fill
        .is_none()
    );
    undo(&mut d);
    assert_eq!(
        format(&d)
            .features
            .object
            .unwrap()
            .shape
            .unwrap()
            .fill
            .as_deref(),
        Some("C2481C")
    );
    // Saved as Word's DrawingML.
    let back = round_trip(&d);
    let saved = back
        .body_paragraphs()
        .iter()
        .flat_map(|p| p.content.objects())
        .find_map(|o| match o {
            InlineObject::Drawing(x) if matches!(x.kind, kiln_doc::DrawingKind::Shape { .. }) => {
                x.xml.clone()
            }
            _ => None,
        })
        .expect("the shape is saved");
    assert!(
        saved.contains("C2481C") && saved.contains("anchor=\"b\""),
        "{saved}"
    );
    assert!(
        fails(
            &mut d,
            WriteCommand::ShapeFill {
                color: Some("red".into())
            }
        )
        .contains("RRGGBB")
    );
    assert!(
        fails(&mut d, WriteCommand::ShapeOutlineWeight { points: 99.0 }).contains("0.25 to 20")
    );
}

#[test]
fn toc_links_in_markdown_and_html_exports_reach_the_headings() {
    let mut d = headed_doc();
    caret(&mut d, 0, 0);
    w(&mut d, WriteCommand::InsertQuickToc);
    let md = crate::apps::write::export::markdown(&d.doc);
    let html = crate::apps::write::export::html(&d.doc);
    // Markdown: each TOC link is the slug Markdown renderers give its heading.
    let links: Vec<&str> = md
        .match_indices("](#")
        .map(|(i, _)| {
            let rest = &md[i + 3..];
            &rest[..rest.find(')').unwrap()]
        })
        .collect();
    assert_eq!(links, ["glazes", "celadon", "firing"], "{md}");
    for l in &links {
        assert!(
            md.lines().any(|line| line.starts_with('#')
                && line.trim_start_matches('#').trim().to_lowercase() == *l),
            "{l} has no heading in {md}"
        );
    }
    // HTML: each href names an element id.
    let hrefs: Vec<&str> = html
        .match_indices("href=\"#")
        .map(|(i, _)| {
            let rest = &html[i + 7..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect();
    assert_eq!(hrefs.len(), 3, "{html}");
    for h in hrefs {
        assert!(
            html.contains(&format!("id=\"{h}\"")),
            "{h} missing in {html}"
        );
    }
    assert!(html.contains("<h1 id=\"glazes\">"), "{html}");
}
