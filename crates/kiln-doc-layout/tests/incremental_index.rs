// SPDX-License-Identifier: AGPL-3.0-or-later
//! An update re-indexes only the pages it laid out again and keeps list labels it cannot have
//! changed (WP-64, typing latency B5): after any sequence of edits the layout's line index,
//! reading order, paragraph ranks and list labels equal those of a full layout of the same
//! document.

mod common;

use common::{layout, lorem, para, set_body};
use kiln_common::NodeId;
use kiln_doc::props::ParagraphProps;
use kiln_doc::{Document, Pos};
use kiln_doc_layout::{Change, DocLayout};

/// Everything the index answers, in a comparable form.
fn index_of(l: &DocLayout, doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for page in 0..l.pages().len() {
        for (i, line) in l.page_lines(page).iter().enumerate() {
            let src = &line.para.src;
            let label = src.map.text().get(..src.label_len).unwrap_or("");
            out.push(format!(
                "p{page} l{i} {:?} #{} x{:.2} y{:.2} {:?} {label:?}",
                line.para.id(),
                line.line,
                line.x,
                line.y,
                line.story
            ));
        }
    }
    out.push(format!("order {:?}", l.reading_order()));
    for p in doc.body_paragraphs() {
        out.push(format!(
            "{:?} lines {:?} rank {:?}",
            p.id,
            l.lines_of(p.id),
            l.rank(p.id)
        ));
    }
    out
}

fn assert_same(l: &DocLayout, doc: &Document, step: &str) {
    let full = layout(doc);
    assert_eq!(l.pages().len(), full.pages().len(), "{step}: page count");
    let (a, b) = (index_of(l, doc), index_of(&full, doc));
    if a != b {
        let first = a
            .iter()
            .zip(&b)
            .position(|(x, y)| x != y)
            .unwrap_or(a.len().min(b.len()));
        panic!(
            "{step}: index differs at entry {first}:\n incremental {:?}\n full        {:?}",
            a.get(first),
            b.get(first)
        );
    }
    assert_eq!(
        kiln_doc_layout::snapshot::summary(l),
        kiln_doc_layout::snapshot::summary(&full),
        "{step}: layout"
    );
}

fn type_at(doc: &mut Document, l: &mut DocLayout, para: NodeId, at: usize, text: &str) {
    let tx = kiln_doc::edit::insert_text(doc, Pos::new(para, at), text).unwrap();
    doc.apply(tx).unwrap();
    l.update(doc, &Change::Nodes(vec![para]));
}

#[test]
fn typing_reindexes_only_what_moved_and_matches_a_full_layout() {
    let mut doc = Document::new();
    let mut blocks = Vec::new();
    for i in 0..300 {
        blocks.push(para(&mut doc, &lorem(30 + i % 40, i), None, None));
    }
    let ids: Vec<NodeId> = blocks.iter().map(common::id_of).collect();
    set_body(&mut doc, blocks);
    let mut l = layout(&doc);
    assert!(l.pages().len() > 10);
    // A word typed in the middle: one page changes, the line index of the others stays.
    for (n, &target) in [150usize, 10, 290, 150, 0].iter().enumerate() {
        type_at(&mut doc, &mut l, ids[target], 3, "kiln ");
        assert_same(&l, &doc, &format!("word {n} in paragraph {target}"));
    }
    // Enough text to push lines onto the next page (the re-flow spans pages).
    type_at(&mut doc, &mut l, ids[120], 0, &lorem(220, 7));
    assert_same(&l, &doc, "a long insertion");
    // A new paragraph changes every later rank.
    let tx = kiln_doc::edit::split_paragraph(&mut doc, Pos::new(ids[60], 4)).unwrap();
    doc.apply(tx).unwrap();
    let all: Vec<NodeId> = doc.body_paragraphs().iter().map(|p| p.id).collect();
    l.update(&doc, &Change::Nodes(all[59..62].to_vec()));
    assert_same(&l, &doc, "Enter");
}

#[test]
fn list_labels_follow_numbering_changes_but_not_plain_typing() {
    let mut doc = Document::new();
    let num = doc.numbering.create_numbered_list().id;
    let mut blocks = Vec::new();
    for i in 0..40 {
        let props = (i % 3 == 0).then(|| ParagraphProps {
            numbering: Some(kiln_doc::props::NumberingRef::new(num, 0)),
            ..ParagraphProps::default()
        });
        blocks.push(para(&mut doc, &lorem(12, i), None, props));
    }
    let ids: Vec<NodeId> = blocks.iter().map(common::id_of).collect();
    set_body(&mut doc, blocks);
    let mut l = layout(&doc);
    // Typing in a plain paragraph and in a numbered one.
    type_at(&mut doc, &mut l, ids[1], 0, "plain ");
    assert_same(&l, &doc, "plain typing");
    type_at(&mut doc, &mut l, ids[3], 0, "listed ");
    assert_same(&l, &doc, "typing in a list item");
    // A plain paragraph joins the list: every later item renumbers.
    let at = Pos::new(ids[4], 0);
    let tx = kiln_doc::edit::format_paragraphs(&doc, at, at, |p| {
        p.numbering = Some(kiln_doc::props::NumberingRef::new(num, 0));
    })
    .unwrap();
    doc.apply(tx).unwrap();
    l.update(&doc, &Change::Nodes(vec![ids[4]]));
    assert_same(&l, &doc, "a paragraph joins the list");
    // And leaves it again.
    let tx = kiln_doc::edit::format_paragraphs(&doc, at, at, |p| p.numbering = None).unwrap();
    doc.apply(tx).unwrap();
    l.update(&doc, &Change::Nodes(vec![ids[4]]));
    assert_same(&l, &doc, "a paragraph leaves the list");
}
