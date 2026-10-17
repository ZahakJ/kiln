// SPDX-License-Identifier: AGPL-3.0-or-later
//! Review display (WP-66): markup modes change the layout, balloons stack, change bars sit in
//! the margin beside changed lines, squiggles under words.

use kiln_doc::props::{ParagraphProps, RunProps, Spacing};
use kiln_doc::st::LineRule;
use kiln_doc::{
    Block, Comment, CommentMeta, Document, Fragment, Inlines, Mark, Paragraph, Pos, RevisionInfo,
};
use kiln_scene::DrawOp;

use super::*;
use crate::cx::ReviewDisplay;
use crate::{DocLayout, default_fonts};

fn words(n: usize, tag: &str) -> String {
    (0..n)
        .map(|i| format!("{tag}{}", i % 10))
        .collect::<Vec<_>>()
        .join(" ")
        + " "
}

/// A paragraph with exact 20 pt lines: `kept` plain words, then `deleted` struck words, then
/// `inserted` new words (tracked by "Ana").
fn tracked_para(doc: &mut Document, kept: usize, deleted: usize, inserted: usize) -> Block {
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    p.props = ParagraphProps {
        spacing: Some(Spacing {
            before: Some(0),
            after: Some(0),
            line: Some(400),
            line_rule: Some(LineRule::Exact),
            ..Spacing::default()
        }),
        ..ParagraphProps::default()
    };
    let info = RevisionInfo::new(1, "Ana", Some("2026-10-17T09:00:00Z"));
    let mut c = Inlines::new();
    c.insert(0, Fragment::text(&words(kept, "keep"), RunProps::default()))
        .unwrap();
    let at = c.len();
    c.insert(
        at,
        Fragment::text(&words(deleted, "gone"), RunProps::default())
            .with_mark(Mark::Deletion(info.clone())),
    )
    .unwrap();
    let at = c.len();
    c.insert(
        at,
        Fragment::text(&words(inserted, "new"), RunProps::default())
            .with_mark(Mark::Insertion(info)),
    )
    .unwrap();
    p.content = c;
    Block::Paragraph(p)
}

/// Twelve paragraphs of about one plain line, eight deleted lines and two inserted lines each.
fn review_fixture() -> Document {
    let mut doc = Document::new();
    let blocks: Vec<Block> = (0..12).map(|_| tracked_para(&mut doc, 10, 120, 30)).collect();
    doc.sections[0].blocks = blocks;
    doc
}

fn pages_with(doc: &Document, markup: bool, review: ReviewDisplay) -> usize {
    let mut l = DocLayout::new(default_fonts());
    let mut o = l.options().clone();
    o.markup = markup;
    o.review = review;
    l.set_options(o);
    l.layout(doc);
    l.pages().len()
}

#[test]
fn markup_modes_change_the_page_count() {
    let doc = review_fixture();
    let all = pages_with(&doc, true, ReviewDisplay::default());
    // Simple Markup and No Markup show the final text: the deleted lines go.
    let final_text = pages_with(&doc, false, ReviewDisplay::default());
    let original = pages_with(
        &doc,
        false,
        ReviewDisplay {
            original: true,
            ..ReviewDisplay::default()
        },
    );
    // Insertions and deletions not shown under All Markup: the final text again.
    let hidden_revisions = pages_with(
        &doc,
        true,
        ReviewDisplay {
            revisions: false,
            ..ReviewDisplay::default()
        },
    );
    assert!(all > original, "all {all} > original {original}");
    assert!(original > final_text, "original {original} > final {final_text}");
    assert_eq!(hidden_revisions, final_text);
    assert!(all >= 4, "all markup shows every deleted line ({all} pages)");
}

#[test]
fn original_shows_formatting_before_the_change() {
    let mut doc = Document::new();
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    let bold = RunProps {
        bold: Some(true),
        size: Some(96),
        ..RunProps::default()
    };
    p.content = Inlines::from_fragment(
        Fragment::text(&words(60, "big"), bold).with_mark(Mark::FormatChange {
            info: RevisionInfo::new(2, "Ana", None),
            before: Box::new(RunProps::default()),
        }),
    )
    .unwrap();
    doc.sections[0].blocks = vec![Block::Paragraph(p)];
    let lines = |review: ReviewDisplay| {
        let mut l = DocLayout::new(default_fonts());
        let mut o = l.options().clone();
        o.review = review;
        l.set_options(o);
        l.layout(&doc);
        l.lines_of(id).len()
    };
    let now = lines(ReviewDisplay::default());
    let before = lines(ReviewDisplay {
        original: true,
        ..ReviewDisplay::default()
    });
    assert!(
        before < now,
        "48 pt bold text needs more lines than 11 pt text ({before} vs {now})"
    );
}

fn laid_out(doc: &Document) -> DocLayout {
    let mut l = DocLayout::new(default_fonts());
    l.layout(doc);
    l
}

#[test]
fn balloons_stack_without_overlapping() {
    let mut doc = Document::new();
    let p1 = doc.alloc_id();
    let mut p = Paragraph::new(p1);
    p.content =
        Inlines::from_fragment(Fragment::text(&words(30, "w"), RunProps::default())).unwrap();
    let (c1, c2) = (doc.alloc_id(), doc.alloc_id());
    p.content.add_mark(0, 3, Mark::Comment(c1)).unwrap();
    p.content.add_mark(5, 8, Mark::Comment(c2)).unwrap();
    doc.sections[0].blocks = vec![Block::Paragraph(p)];
    for c in [c1, c2] {
        let body = doc.alloc_id();
        doc.comments.push(Comment {
            id: c,
            meta: CommentMeta {
                author: "Ana".into(),
                ..CommentMeta::default()
            },
            blocks: vec![Block::Paragraph(Paragraph::new(body))],
        });
    }
    let l = laid_out(&doc);
    let placed = place_balloons(&l, &|_| 80.0);
    assert_eq!(placed.len(), 2);
    let (a, b) = (&placed[0], &placed[1]);
    assert!(b.rect.y >= a.rect.bottom() + BALLOON_GAP - 0.01, "{a:?} {b:?}");
    let page = &l.pages()[0];
    assert!(
        a.rect.x >= page.origin.0 + page.width,
        "balloons are in the markup area right of the page"
    );
    // The anchor is on the commented text, in document coordinates.
    assert!(a.anchor.0 >= page.origin.0 && a.anchor.0 < page.origin.0 + page.width);
    // Without markup there are no balloons.
    let mut plain = DocLayout::new(default_fonts());
    let mut o = plain.options().clone();
    o.markup = false;
    plain.set_options(o);
    plain.layout(&doc);
    assert!(place_balloons(&plain, &|_| 80.0).is_empty());
}

#[test]
fn change_bars_sit_in_the_left_margin_one_per_line() {
    let mut doc = Document::new();
    let blocks = vec![tracked_para(&mut doc, 10, 48, 10)];
    let id = blocks[0].id();
    doc.sections[0].blocks = blocks;
    let l = laid_out(&doc);
    let len = doc.paragraph(id).unwrap().content.len();
    let bars = change_bars(&l, &doc, &[(Pos::new(id, 0), Pos::new(id, len))], SIMPLE_BAR);
    // Consecutive changed lines make one continuous bar beside the paragraph.
    assert_eq!(bars.len(), 1);
    let page = &l.pages()[0];
    let lines: Vec<_> = l.lines_of(id).iter().filter_map(|r| l.line(*r)).collect();
    assert!(lines.len() > 3);
    let DrawOp::FillRect { rect, .. } = &bars[0] else {
        panic!("a bar is a filled rectangle");
    };
    assert!(rect.x < page.origin.0 + page.body.x, "left of the text");
    assert!(rect.w <= 3.0);
    let first = lines[0].rect();
    let last = lines[lines.len() - 1].rect();
    assert!(rect.h >= last.bottom() - first.y - 1.0, "spans every line: {rect:?}");
    // A change in the middle of the paragraph marks only its lines.
    let one = change_bars(&l, &doc, &[(Pos::new(id, 2), Pos::new(id, 4))], ALL_BAR);
    let DrawOp::FillRect { rect: r1, .. } = &one[0] else {
        panic!("a bar");
    };
    assert!(r1.h < rect.h / 2.0);
}

#[test]
fn squiggles_and_connectors() {
    let mut doc = Document::new();
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    p.content =
        Inlines::from_fragment(Fragment::text("teh kiln", RunProps::default())).unwrap();
    doc.sections[0].blocks = vec![Block::Paragraph(p)];
    let l = laid_out(&doc);
    let ops = squiggles(&l, &[(id, vec![(0, 3)])]);
    assert_eq!(ops.len(), 1);
    assert!(matches!(ops[0], DrawOp::Squiggle { width, .. } if width > 5.0));
    let c = connector(
        (100.0, 200.0),
        800.0,
        Bounds::new(820.0, 180.0, 240.0, 60.0),
        Paint::Rgba(author_color("Ana")),
    );
    assert_eq!(c.len(), 2);
    assert_eq!(author_hex("Ana").len(), 6);
    assert_eq!(author_hex("Ana"), author_hex("Ana"));
}
