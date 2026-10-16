// SPDX-License-Identifier: AGPL-3.0-or-later
//! Word's pagination rules, one test each: widow and orphan control, keep lines together, keep
//! with next, page break before, forced breaks, columns, tables that split or repeat their header
//! rows, footnotes that shorten the page, floating objects in every wrap mode, incremental
//! re-layout and geometry across pages.
//!
//! Paragraphs use exact 20 pt line spacing and no paragraph spacing, so a Letter page with 1-inch
//! margins holds exactly 32 lines and every expectation is arithmetic.

mod common;

use common::*;
use kiln_common::NodeId;
use kiln_doc::model::Anchor;
use kiln_doc::props::{ParagraphProps, RunProps, Spacing, TableRowProps};
use kiln_doc::st::{BreakKind, LineRule, RelativeFromH, RelativeFromV, WrapType};
use kiln_doc::{
    Block, Break, Cell, Document, Drawing, Fragment, InlineObject, Inlines, Note, Paragraph,
    Placement, PosH, PosV, Row, Table,
};
use kiln_doc_layout::{Change, DocLayout, Item, StoryKind, View, default_fonts, geometry};

const LINE: f32 = 400.0 / 15.0;

fn exact() -> ParagraphProps {
    ParagraphProps {
        spacing: Some(Spacing {
            before: Some(0),
            after: Some(0),
            line: Some(400),
            line_rule: Some(LineRule::Exact),
            ..Spacing::default()
        }),
        ..ParagraphProps::default()
    }
}

/// A paragraph of `n` lines (manual line breaks), each line `tag`-numbered.
fn lines(doc: &mut Document, n: usize, tag: &str, props: ParagraphProps) -> Block {
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    p.props = props;
    let mut c = Inlines::new();
    for i in 0..n {
        if i > 0 {
            let at = c.len();
            c.insert(
                at,
                Fragment::object(
                    InlineObject::Break(Break {
                        kind: BreakKind::TextWrapping,
                        clear: None,
                    }),
                    RunProps::default(),
                ),
            )
            .unwrap();
        }
        let at = c.len();
        c.insert(
            at,
            Fragment::text(&format!("{tag}{i}"), RunProps::default()),
        )
        .unwrap();
    }
    p.content = c;
    Block::Paragraph(p)
}

fn filler(doc: &mut Document, n: usize) -> Vec<Block> {
    (0..n)
        .map(|i| lines(doc, 1, &format!("fill{i}-"), exact()))
        .collect()
}

/// Pages of each line of a paragraph.
fn pages_of(l: &DocLayout, b: &Block) -> Vec<u32> {
    l.lines_of(b.id()).iter().map(|r| r.page).collect()
}

fn build(blocks: Vec<Block>, doc: &mut Document) -> DocLayout {
    set_body(doc, blocks);
    layout(doc)
}

#[test]
fn a_page_holds_32_exact_lines() {
    let mut doc = Document::new();
    let blocks = filler(&mut doc, 33);
    let last = blocks[32].clone();
    let l = build(blocks, &mut doc);
    assert_eq!(l.pages().len(), 2);
    assert_eq!(pages_of(&l, &last), vec![1]);
    assert!((l.pages()[0].body.h - 864.0).abs() < 0.01);
    const { assert!(LINE * 32.0 <= 864.0 && LINE * 33.0 > 864.0) };
}

#[test]
fn widow_control_never_leaves_one_line_behind_or_alone() {
    let cases: [(usize, usize, Vec<u32>); 4] = [
        // (filler lines, paragraph lines, page of each line)
        (30, 4, vec![0, 0, 1, 1]), // 2 fit, 2 follow: no rule applies
        (29, 4, vec![0, 0, 1, 1]), // 3 fit, 1 would be a widow: one more moves
        (31, 3, vec![1, 1, 1]),    // 1 fits: an orphan, the paragraph moves
        (30, 3, vec![1, 1, 1]),    // 2 fit, widow → 1 left → orphan → all move
    ];
    for (fill, n, want) in cases {
        let mut doc = Document::new();
        let mut blocks = filler(&mut doc, fill);
        let p = lines(&mut doc, n, "p", exact());
        blocks.push(p.clone());
        let l = build(blocks, &mut doc);
        assert_eq!(
            pages_of(&l, &p),
            want,
            "{fill} filler lines, {n}-line paragraph"
        );
    }
    // Without widow control the lines simply fill the page.
    let mut doc = Document::new();
    let mut blocks = filler(&mut doc, 31);
    let mut props = exact();
    props.widow_control = Some(false);
    let p = lines(&mut doc, 3, "p", props);
    blocks.push(p.clone());
    let l = build(blocks, &mut doc);
    assert_eq!(pages_of(&l, &p), vec![0, 1, 1]);
}

#[test]
fn keep_lines_together_moves_the_whole_paragraph() {
    let mut doc = Document::new();
    let mut blocks = filler(&mut doc, 28);
    let mut props = exact();
    props.keep_lines = Some(true);
    let p = lines(&mut doc, 6, "p", props);
    blocks.push(p.clone());
    let l = build(blocks, &mut doc);
    assert_eq!(pages_of(&l, &p), vec![1; 6]);
}

#[test]
fn keep_with_next_moves_the_heading_with_its_paragraph() {
    let mut doc = Document::new();
    let mut blocks = filler(&mut doc, 30);
    let mut keep = exact();
    keep.keep_next = Some(true);
    let h1 = lines(&mut doc, 1, "h", keep.clone());
    let h2 = lines(&mut doc, 1, "k", keep);
    let mut lines_kept = exact();
    lines_kept.keep_lines = Some(true);
    let p = lines(&mut doc, 4, "p", lines_kept);
    blocks.extend([h1.clone(), h2.clone(), p.clone()]);
    let l = build(blocks, &mut doc);
    assert_eq!(pages_of(&l, &p), vec![1; 4]);
    assert_eq!(pages_of(&l, &h2), vec![1], "the chain moves together");
    assert_eq!(pages_of(&l, &h1), vec![1]);
    // A chain starting at the top of a page cannot move: it breaks normally.
    let mut doc = Document::new();
    let mut keep = exact();
    keep.keep_next = Some(true);
    let mut blocks: Vec<Block> = (0..40)
        .map(|i| lines(&mut doc, 1, &format!("c{i}-"), keep.clone()))
        .collect();
    let tail = lines(&mut doc, 1, "end", exact());
    blocks.push(tail.clone());
    let l = build(blocks, &mut doc);
    assert_eq!(l.pages().len(), 2);
}

#[test]
fn page_break_before_and_forced_breaks() {
    let mut doc = Document::new();
    let a = lines(&mut doc, 1, "a", exact());
    let mut pb = exact();
    pb.page_break_before = Some(true);
    let b = lines(&mut doc, 1, "b", pb);
    // "x" PAGE BREAK "y" in one paragraph.
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    p.props = exact();
    p.content = Inlines::from_fragment(Fragment::text("x", RunProps::default())).unwrap();
    p.content
        .insert(
            1,
            Fragment::object(
                InlineObject::Break(Break {
                    kind: BreakKind::Page,
                    clear: None,
                }),
                RunProps::default(),
            ),
        )
        .unwrap();
    p.content
        .insert(2, Fragment::text("y", RunProps::default()))
        .unwrap();
    let c = Block::Paragraph(p);
    let l = build(vec![a.clone(), b.clone(), c.clone()], &mut doc);
    assert_eq!(pages_of(&l, &a), vec![0]);
    assert_eq!(pages_of(&l, &b), vec![1]);
    assert_eq!(pages_of(&l, &c), vec![1, 2]);
    assert_eq!(l.pages().len(), 3);
}

#[test]
fn columns_fill_left_to_right() {
    let mut doc = Document::new();
    doc.sections[0].props.columns.count = 2;
    let blocks = filler(&mut doc, 40);
    let first = blocks[0].clone();
    let second_col = blocks[33].clone();
    let l = build(blocks, &mut doc);
    assert_eq!(l.pages().len(), 1);
    let x0 = l.line(l.lines_of(first.id())[0]).unwrap().x;
    let x1 = l.line(l.lines_of(second_col.id())[0]).unwrap().x;
    assert!(x1 > x0 + 200.0, "{x0} {x1}");
}

fn cell(doc: &mut Document, text: &str) -> Cell {
    let p = lines(doc, 1, text, exact());
    Cell {
        id: doc.alloc_id(),
        props: Default::default(),
        grid_span: 1,
        v_merge: None,
        blocks: vec![p],
    }
}

fn table(doc: &mut Document, rows: usize, header: bool, cant_split_tall: bool) -> Table {
    let mut out = Vec::new();
    for r in 0..rows {
        let mut c1 = cell(doc, &format!("r{r}a"));
        let c2 = cell(doc, &format!("r{r}b"));
        if cant_split_tall && r == rows - 1 {
            c1.blocks = vec![lines(doc, 10, "tall", exact())];
        }
        out.push(Row {
            id: doc.alloc_id(),
            props: TableRowProps {
                header: Some(header && r == 0),
                cant_split: Some(cant_split_tall),
                ..TableRowProps::default()
            },
            revision: None,
            cells: vec![c1, c2],
        });
    }
    Table {
        id: doc.alloc_id(),
        props: Default::default(),
        grid: vec![4320, 4320],
        rows: out,
    }
}

#[test]
fn tables_repeat_header_rows_and_keep_unsplittable_rows_whole() {
    let mut doc = Document::new();
    let t = table(&mut doc, 50, true, false);
    let l = build(vec![Block::Table(t)], &mut doc);
    assert!(l.pages().len() >= 2);
    let header_on = |page: usize| {
        l.page_lines(page).iter().any(|li| {
            let info = &li.para.lines[li.line as usize];
            li.para.src.map.text()[info.bytes.clone()].starts_with("r0a")
        })
    };
    assert!(header_on(0) && header_on(1), "the header row repeats");

    let mut doc = Document::new();
    let mut blocks = filler(&mut doc, 25);
    let t = table(&mut doc, 2, false, true);
    let tall = t.rows[1].cells[0].blocks[0].clone();
    blocks.push(Block::Table(t));
    let l = build(blocks, &mut doc);
    assert_eq!(
        pages_of(&l, &tall),
        vec![1; 10],
        "a row that cannot split moves whole"
    );

    // The same row may split: its lines continue on the next page.
    let mut doc = Document::new();
    let mut blocks = filler(&mut doc, 25);
    let mut t = table(&mut doc, 2, false, true);
    for r in &mut t.rows {
        r.props.cant_split = Some(false);
    }
    let tall = t.rows[1].cells[0].blocks[0].clone();
    blocks.push(Block::Table(t));
    let l = build(blocks, &mut doc);
    let pages = pages_of(&l, &tall);
    assert!(pages.contains(&0) && pages.contains(&1), "{pages:?}");
}

#[test]
fn footnotes_shorten_the_page() {
    let mut doc = Document::new();
    let note_id = doc.alloc_id();
    let note_para = lines(&mut doc, 4, "note", exact());
    doc.notes.push(Note {
        id: note_id,
        kind: kiln_doc::st::NoteKind::Footnote,
        note_type: kiln_doc::st::NoteType::Normal,
        blocks: vec![note_para],
    });
    let mut blocks = filler(&mut doc, 29);
    // A paragraph whose only line references the note: the note needs 4 lines + separator.
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    p.props = exact();
    p.content = Inlines::from_fragment(Fragment::text("ref", RunProps::default())).unwrap();
    p.content
        .insert(
            3,
            Fragment::object(
                InlineObject::NoteRef(kiln_doc::NoteRef {
                    note: note_id,
                    kind: kiln_doc::st::NoteKind::Footnote,
                    custom_mark: None,
                }),
                RunProps::default(),
            ),
        )
        .unwrap();
    let r = Block::Paragraph(p);
    blocks.push(r.clone());
    let l = build(blocks, &mut doc);
    assert_eq!(
        pages_of(&l, &r),
        vec![1],
        "line + note do not fit on page 1"
    );
    let notes_on_2 = l.pages()[1]
        .notes
        .iter()
        .filter(|i| matches!(i, Item::Line(_)))
        .count();
    assert_eq!(notes_on_2, 4);
    // The reference shows the note's number.
    let line = l.line(l.lines_of(r.id())[0]).unwrap();
    assert_eq!(line.para.src.map.text(), "ref1");
}

fn anchored(doc: &mut Document, wrap: WrapType, polygon: Option<&str>) -> Drawing {
    let mut d = Drawing::picture(doc.alloc_id(), "word/media/none.png", 1_828_800, 914_400);
    d.placement = Placement::Anchor(Box::new(Anchor {
        wrap,
        wrap_side: None,
        relative_h: RelativeFromH::Column,
        h: PosH::Offset(0),
        relative_v: RelativeFromV::Paragraph,
        v: PosV::Offset(0),
        dist_top: 0,
        dist_bottom: 0,
        dist_left: 0,
        dist_right: 0,
        allow_overlap: true,
        layout_in_cell: true,
        locked: false,
        z_order: 1,
        wrap_polygon_xml: polygon.map(str::to_owned),
    }));
    d
}

/// A 192 × 96 px picture anchored at the start of a long paragraph; returns the layout and the
/// paragraph's line rectangles.
fn float_case(
    wrap: WrapType,
    polygon: Option<&str>,
) -> (DocLayout, Vec<kiln_scene::Bounds>, kiln_scene::Bounds) {
    let mut doc = Document::new();
    let d = anchored(&mut doc, wrap, polygon);
    let id = doc.alloc_id();
    let mut p = Paragraph::new(id);
    p.content = Inlines::from_fragment(Fragment::object(
        InlineObject::Drawing(Box::new(d)),
        RunProps::default(),
    ))
    .unwrap();
    let at = p.content.len();
    p.content
        .insert(at, Fragment::text(&lorem(220, 3), RunProps::default()))
        .unwrap();
    let pid = p.id;
    set_body(&mut doc, vec![Block::Paragraph(p)]);
    let l = layout(&doc);
    let rects = l
        .lines_of(pid)
        .iter()
        .map(|r| l.line(*r).unwrap().rect())
        .collect();
    let pic = l.pages()[0]
        .behind
        .iter()
        .chain(&l.pages()[0].front)
        .find_map(|i| match i {
            Item::Drawing(d) => Some(d.rect),
            _ => None,
        })
        .unwrap();
    (l, rects, pic)
}

fn overlaps(a: &kiln_scene::Bounds, b: &kiln_scene::Bounds) -> bool {
    a.x < b.right() - 0.5
        && b.x < a.right() - 0.5
        && a.y < b.bottom() - 0.5
        && b.y < a.bottom() - 0.5
}

#[test]
fn square_and_top_and_bottom_wrapping_keep_text_off_the_picture() {
    let (_, rects, pic) = float_case(WrapType::Square, None);
    assert!(
        rects.iter().all(|r| !overlaps(r, &pic)),
        "square: no line under the picture"
    );
    let beside: Vec<_> = rects.iter().filter(|r| r.y < pic.bottom()).collect();
    assert!(!beside.is_empty() && beside.iter().all(|r| r.x >= pic.right() - 0.5));
    let (_, rects, pic) = float_case(WrapType::TopAndBottom, None);
    assert!(
        rects.iter().all(|r| r.y >= pic.bottom() - 0.5),
        "top and bottom: text below only"
    );
}

#[test]
fn behind_and_in_front_do_not_move_text() {
    for wrap in [WrapType::Behind, WrapType::InFront] {
        let (l, rects, pic) = float_case(wrap, None);
        assert!(
            (rects[0].x - pic.x).abs() < 0.5,
            "{wrap:?}: the first line starts under it"
        );
        let page = &l.pages()[0];
        let behind = page.behind.iter().any(|i| matches!(i, Item::Drawing(_)));
        assert_eq!(behind, wrap == WrapType::Behind);
    }
}

#[test]
fn tight_and_through_follow_the_wrap_polygon() {
    // A triangle pointing up: narrow at the top, full width at the bottom, on the left of the
    // column. Lines beside its narrow top start further left than lines beside its base.
    let tri = r#"<wp:wrapPolygon edited="0"><wp:start x="0" y="0"/><wp:lineTo x="2000" y="0"/><wp:lineTo x="21600" y="21600"/><wp:lineTo x="0" y="21600"/><wp:lineTo x="0" y="0"/></wp:wrapPolygon>"#;
    for wrap in [WrapType::Tight, WrapType::Through] {
        let (_, rects, pic) = float_case(wrap, Some(tri));
        let beside: Vec<_> = rects.iter().filter(|r| r.y < pic.bottom() - 1.0).collect();
        assert!(beside.len() >= 3, "{wrap:?}");
        assert!(
            beside.first().unwrap().x < beside.last().unwrap().x - 40.0,
            "{wrap:?}: {:?}",
            beside.iter().map(|r| r.x).collect::<Vec<_>>()
        );
    }
}

#[test]
fn incremental_updates_match_a_full_layout_and_stop_early() {
    let mut doc = Document::new();
    let mut blocks = Vec::new();
    for i in 0..400 {
        blocks.push(para(&mut doc, &lorem(40 + i % 30, i), None, None));
    }
    let target = blocks[200].id();
    set_body(&mut doc, blocks);
    let mut l = layout(&doc);
    let pages_before = l.pages().len();
    // Type a word in paragraph 200.
    let tx = kiln_doc::edit::insert_text(&mut doc, kiln_doc::Pos::new(target, 5), "fire ").unwrap();
    doc.apply(tx).unwrap();
    l.update(&doc, &Change::Nodes(vec![target]));
    assert!(
        l.stats().converged || l.stats().blocks_placed < 40,
        "{:?}",
        l.stats()
    );
    let full = layout(&doc);
    assert_eq!(
        kiln_doc_layout::snapshot::summary(&l),
        kiln_doc_layout::snapshot::summary(&full)
    );
    assert!(l.pages().len() >= pages_before);
    // A new paragraph (Enter) also matches.
    let tx = kiln_doc::edit::split_paragraph(&mut doc, kiln_doc::Pos::new(target, 3)).unwrap();
    doc.apply(tx).unwrap();
    let new_ids: Vec<NodeId> = doc.body_paragraphs().iter().map(|p| p.id).collect();
    l.update(&doc, &Change::Nodes(new_ids[199..203].to_vec()));
    let full = layout(&doc);
    assert_eq!(
        kiln_doc_layout::snapshot::summary(&l),
        kiln_doc_layout::snapshot::summary(&full)
    );
}

#[test]
fn carets_and_hits_agree_and_motion_crosses_pages() {
    let mut doc = Document::new();
    let blocks = filler(&mut doc, 40);
    let last_on_first = blocks[31].id();
    let first_on_second = blocks[32].id();
    let l = build(blocks, &mut doc);
    for (para, off) in [(last_on_first, 2), (first_on_second, 0)] {
        let pos = kiln_doc::Pos::new(para, off);
        let c = geometry::caret_in(&l, pos).unwrap();
        let hit = geometry::hit_test(
            &l,
            c.rect.x + 1.0,
            c.rect.y + c.rect.h / 2.0,
            Some(StoryKind::Body),
        )
        .unwrap();
        assert_eq!(hit.pos, pos);
    }
    let c = geometry::caret_in(&l, kiln_doc::Pos::new(last_on_first, 1)).unwrap();
    let down =
        geometry::vertical(&l, kiln_doc::Pos::new(last_on_first, 1), c.rect.x, true).unwrap();
    assert_eq!(down.para, first_on_second, "Down moves onto the next page");
    let c2 = geometry::caret_in(&l, down).unwrap();
    assert_eq!(c2.page, 1);
    let up = geometry::vertical(&l, down, c2.rect.x, false).unwrap();
    assert_eq!(up.para, last_on_first);
    // Selections across the page break cover both pages.
    let rects = geometry::selection_rects(
        &l,
        &doc,
        kiln_doc::Pos::new(last_on_first, 0),
        kiln_doc::Pos::new(first_on_second, 3),
    );
    assert!(rects.len() >= 3);
    let pages: std::collections::BTreeSet<usize> = rects
        .iter()
        .filter_map(|r| l.page_at(r.x + 1.0, r.y + 1.0))
        .collect();
    assert_eq!(pages.len(), 2);
}

#[test]
fn views_reflow_without_pages() {
    let mut doc = Document::new();
    let blocks = (0..60)
        .map(|i| para(&mut doc, &lorem(50, i), None, None))
        .collect();
    set_body(&mut doc, blocks);
    let mut l = DocLayout::new(default_fonts());
    l.set_view(View::Web { width: 500.0 });
    l.layout(&doc);
    assert_eq!(l.pages().len(), 1);
    let narrow = l.extent().h;
    l.set_view(View::Web { width: 1200.0 });
    l.layout(&doc);
    assert!(l.extent().h < narrow, "wider window, fewer lines");
    l.set_view(View::Draft);
    l.layout(&doc);
    assert_eq!(l.pages().len(), 1);
    l.set_view(View::Read {
        width: 700.0,
        height: 900.0,
    });
    l.layout(&doc);
    assert!(l.pages().len() > 2);
    let p = &l.pages()[1];
    assert!(
        p.origin.0 > l.pages()[0].origin.0,
        "read mode pages run left to right"
    );
    l.set_view(View::Print);
    l.layout(&doc);
    assert!(l.pages().len() >= 3);
}
