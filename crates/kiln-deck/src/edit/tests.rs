// SPDX-License-Identifier: AGPL-3.0-or-later
//! Edit builders: what each command does, and that every transaction it builds is undone
//! exactly by its inverse (and redone by the inverse's inverse).

use std::collections::HashSet;

use kiln_common::NodeId;
use kiln_doc::Pos;
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;

use super::*;
use crate::animation::{AnimationEffect, EffectStart};
use crate::geom::{ROT_PER_DEGREE, Rect, Transform};
use crate::ops::{EditKind, TextTarget, Transaction};
use crate::presentation::{Part, Presentation};
use crate::shape::{Crop, PlaceholderType, Shape, ShapeKind};
use crate::slide::{Background, LayoutKind, SlideSize};
use crate::text::TextBody;
use crate::transition::{Transition, TransitionEffect};

/// The layout of a kind in the first master.
pub(crate) fn layout(p: &Presentation, kind: LayoutKind) -> NodeId {
    p.masters[0]
        .layout_of_kind(&kind)
        .expect("standard layout")
        .id
}

/// The placeholder of a type on a slide.
pub(crate) fn ph_of(p: &Presentation, slide: NodeId, kind: PlaceholderType) -> NodeId {
    p.slide(slide)
        .unwrap()
        .shapes
        .iter()
        .find(|s| s.placeholder.is_some_and(|ph| ph.kind == kind))
        .expect("placeholder")
        .id
}

/// Applies a transaction, checks the result, then checks that its inverse restores the
/// presentation exactly and that the inverse's inverse redoes it. Returns the inverse.
#[track_caller]
pub(crate) fn roundtrip(p: &mut Presentation, tx: Transaction) -> Transaction {
    let before = p.clone();
    let inverse = p.apply(tx).expect("transaction applies");
    p.validate().expect("valid after the edit");
    let after = p.clone();
    let redo = p.apply(inverse.clone()).expect("inverse applies");
    assert_eq!(*p, before, "the inverse restores the presentation");
    p.validate().expect("valid after undo");
    p.apply(redo).expect("redo applies");
    assert_eq!(*p, after, "the inverse's inverse redoes the edit");
    inverse
}

/// A five-slide deck exercising every shape kind, text, notes, motion, comments and sections.
pub(crate) fn sample() -> Presentation {
    let mut p = Presentation::new();
    let title_slide = p.slides[0].id;
    // Slide 1: title and subtitle.
    let t = ph_of(&p, title_slide, PlaceholderType::CenteredTitle);
    let s = ph_of(&p, title_slide, PlaceholderType::Subtitle);
    let tx = text::set_text(&p, TextTarget::Shape(t), "Open Studio Weekend").unwrap();
    p.apply(tx).unwrap();
    let tx = text::set_text(&p, TextTarget::Shape(s), "15–16 November").unwrap();
    p.apply(tx).unwrap();
    // Slides 2–5.
    for (i, kind) in [
        LayoutKind::TitleContent,
        LayoutKind::TwoContent,
        LayoutKind::SectionHeader,
        LayoutKind::Blank,
    ]
    .into_iter()
    .enumerate()
    {
        let l = layout(&p, kind);
        let tx = insert_slide(&mut p, l, i + 1).unwrap();
        p.apply(tx).unwrap();
    }
    let content = p.slides[1].id;
    let title = ph_of(&p, content, PlaceholderType::Title);
    let body = ph_of(&p, content, PlaceholderType::Object);
    let tx = text::set_text(&p, TextTarget::Shape(title), "This year").unwrap();
    p.apply(tx).unwrap();
    let tx = text::set_text(
        &p,
        TextTarget::Shape(body),
        "Gas and kiln time\nClay and glaze\nBursaries",
    )
    .unwrap();
    p.apply(tx).unwrap();
    let paras: Vec<NodeId> = p
        .shape(body)
        .unwrap()
        .text()
        .unwrap()
        .paragraphs()
        .map(|x| x.id)
        .collect();
    let tx = text::set_paragraph_level(&p, TextTarget::Shape(body), &paras[1..2], 1).unwrap();
    p.apply(tx).unwrap();
    // Slide 3: shapes of every kind.
    let two = p.slides[2].id;
    let r = Rect::new(914_400, 1_828_800, 2_000_000, 1_000_000);
    let mut shapes = vec![
        Shape::geometry(p.alloc_id(), "roundRect", r),
        Shape::text_box(
            p.alloc_id(),
            Rect::new(5_000_000, 1_000_000, 3_000_000, 500_000),
            "Note",
        ),
        shapes::new_table(
            &mut p,
            2,
            3,
            Rect::new(1_000_000, 4_000_000, 6_000_000, 1_200_000),
        )
        .unwrap(),
    ];
    p.parts.insert(
        "ppt/media/image1.png".into(),
        Part {
            content_type: "image/png".into(),
            bytes: vec![0x89, b'P', b'N', b'G'],
        },
    );
    shapes.push(shapes::new_picture(
        &mut p,
        "ppt/media/image1.png",
        Rect::new(8_000_000, 3_000_000, 2_000_000, 1_500_000),
    ));
    shapes.push(shapes::new_chart(
        &mut p,
        "ppt/charts/chart1.xml",
        Rect::new(500_000, 500_000, 2_000_000, 1_000_000),
    ));
    let a = shapes[0].id;
    let b = shapes[1].id;
    shapes.push(shapes::new_connector(
        &mut p,
        "straightConnector1",
        (2_914_400, 2_328_800),
        (5_000_000, 1_250_000),
        (
            Some(crate::shape::Connection { shape: a, site: 3 }),
            Some(crate::shape::Connection { shape: b, site: 1 }),
        ),
    ));
    let tx = insert_shapes(&mut p, two, shapes, None).unwrap();
    p.apply(tx).unwrap();
    // A group of two ellipses on slide 5.
    let blank = p.slides[4].id;
    let e1 = Shape::geometry(
        p.alloc_id(),
        "ellipse",
        Rect::new(1_000_000, 1_000_000, 1_000_000, 1_000_000),
    );
    let e2 = Shape::geometry(
        p.alloc_id(),
        "ellipse",
        Rect::new(2_500_000, 1_500_000, 1_000_000, 1_000_000),
    );
    let (e1, e2) = (e1.id, {
        let id = e2.id;
        let tx = insert_shapes(&mut p, blank, vec![e1, e2], None).unwrap();
        p.apply(tx).unwrap();
        id
    });
    let tx = group(&mut p, &[e1, e2]).unwrap();
    p.apply(tx).unwrap();
    // Notes, transitions, animations, comments, sections.
    let tx = set_notes_text(&mut p, content, "Start with the number people remember.").unwrap();
    p.apply(tx).unwrap();
    let tx = set_transition(
        &mut p,
        &[content, two],
        Some(Transition::new(TransitionEffect::Fade {
            through_black: false,
        })),
    )
    .unwrap();
    p.apply(tx).unwrap();
    let tx = add_effect(&mut p, body, AnimationEffect::Fade, None).unwrap();
    p.apply(tx).unwrap();
    let tx = add_effect(
        &mut p,
        a,
        AnimationEffect::FlyIn {
            from: crate::transition::Direction8::Left,
        },
        None,
    )
    .unwrap();
    p.apply(tx).unwrap();
    let by = comments::Byline {
        author: "Ada",
        initials: "AL",
        created: "2026-10-16T09:00:00Z",
    };
    let tx = add_comment(&mut p, content, Some(title), (100, 100), &by, "Bigger?").unwrap();
    p.apply(tx).unwrap();
    let tx = add_section(&mut p, 0, "Intro").unwrap();
    p.apply(tx).unwrap();
    let tx = add_section(&mut p, 3, "Details").unwrap();
    p.apply(tx).unwrap();
    p.validate().unwrap();
    p
}

#[test]
fn the_sample_is_valid_and_has_five_slides() {
    let p = sample();
    assert_eq!(p.slides.len(), 5);
    assert_eq!(p.sections.len(), 2);
    assert_eq!(
        p.sections.iter().map(|s| s.len).collect::<Vec<_>>(),
        vec![3, 2]
    );
    assert_eq!(
        p.slides[1].notes_text(),
        "Start with the number people remember."
    );
    assert_eq!(p.slides[1].timeline.sequence.len(), 1);
    assert_eq!(p.slides[2].timeline.sequence.len(), 1);
    // Serialisation round trip.
    let bytes = p.to_bytes().unwrap();
    assert_eq!(Presentation::from_bytes(&bytes).unwrap(), p);
}

#[test]
fn new_slide_gets_the_layouts_content_placeholders() {
    let mut p = Presentation::new();
    let l = layout(&p, LayoutKind::Comparison);
    let tx = insert_slide(&mut p, l, 1).unwrap();
    assert_eq!(tx.kind, EditKind::Structure);
    roundtrip(&mut p, tx);
    let s = &p.slides[1];
    let kinds: Vec<_> = s
        .shapes
        .iter()
        .map(|x| x.placeholder.unwrap().kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            PlaceholderType::Title,
            PlaceholderType::Body,
            PlaceholderType::Object,
            PlaceholderType::Body,
            PlaceholderType::Object
        ]
    );
    // Every placeholder inherits a position from the layout.
    for sh in &s.shapes {
        assert!(sh.xfrm.is_none());
        assert!(p.resolved_xfrm(sh.id).is_some());
    }
    assert!(insert_slide(&mut p, NodeId(999_999), 0).is_err());
    assert!(insert_slide(&mut p, l, 99).is_err());
}

#[test]
fn slides_duplicate_delete_hide_and_move() {
    let mut p = sample();
    let ids: Vec<NodeId> = p.slides.iter().map(|s| s.id).collect();
    // Duplicate slides 2 and 3: copies after slide 3, in the first section.
    let tx = duplicate_slides(&mut p, &[ids[2], ids[1]]).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slides.len(), 7);
    assert_eq!(p.sections[0].len, 5);
    let copy = &p.slides[3];
    assert_ne!(copy.id, ids[1]);
    assert_eq!(copy.notes_text(), p.slides[1].notes_text());
    // The copy's animation targets the copy's shape.
    let target = copy.timeline.sequence[0].target.shape;
    assert!(copy.shapes.iter().any(|s| s.id == target));
    // The copied connector points at copied shapes.
    let copy3 = &p.slides[4];
    let conn = copy3
        .shapes
        .iter()
        .find_map(|s| match &s.kind {
            ShapeKind::Connector(c) => Some(c.clone()),
            _ => None,
        })
        .unwrap();
    assert!(
        copy3
            .shapes
            .iter()
            .any(|s| Some(s.id) == conn.start.map(|c| c.shape))
    );
    p.validate().unwrap();

    // Hide, delete, move.
    let tx = set_hidden(&mut p, &[ids[0]], true).unwrap();
    roundtrip(&mut p, tx);
    assert!(p.slides[0].hidden);
    let tx = delete_slides(&mut p, &[ids[0], ids[3]]).unwrap();
    assert_eq!(tx.selection_after, vec![p.slides[1].id]);
    roundtrip(&mut p, tx);
    assert_eq!(p.slides.len(), 5);
    assert_eq!(p.sections.iter().map(|s| s.len).sum::<usize>(), 5);
    let order: Vec<NodeId> = p.slides.iter().map(|s| s.id).collect();
    let tx = move_slides(&mut p, &[order[0], order[1]], 5).unwrap();
    roundtrip(&mut p, tx);
    let now: Vec<NodeId> = p.slides.iter().map(|s| s.id).collect();
    assert_eq!(now, vec![order[2], order[3], order[4], order[0], order[1]]);
    // They joined the last section.
    assert_eq!(p.section_of_index(4), Some(p.sections.len() - 1));
    // Move to the front.
    let tx = move_slides(&mut p, &[order[1]], 0).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slides[0].id, order[1]);
    // Move to a section.
    let first_section = p.sections[0].id;
    let tx = move_slides_to_section(&mut p, &[order[0]], first_section).unwrap();
    roundtrip(&mut p, tx);
    let first_len = p.sections[0].len;
    assert_eq!(p.slides[first_len - 1].id, order[0]);
    assert!(move_slides(&mut p, &[order[0]], 99).is_err());
    assert!(delete_slides(&mut p, &[NodeId(999_999)]).is_err());
}

#[test]
fn layout_change_rematches_placeholders() {
    let mut p = sample();
    let s = p.slides[1].id;
    let body = ph_of(&p, s, PlaceholderType::Object);
    // Title and Content → Two Content: the body keeps its text and moves to the left column;
    // a second, empty content placeholder appears.
    let two = layout(&p, LayoutKind::TwoContent);
    let tx = change_layout(&mut p, &[s], two).unwrap();
    roundtrip(&mut p, tx);
    let slide = p.slide(s).unwrap();
    assert_eq!(slide.layout, two);
    assert_eq!(slide.shapes.len(), 3);
    assert!(
        p.shape(body)
            .unwrap()
            .text()
            .unwrap()
            .text()
            .starts_with("Gas")
    );
    let left = p.resolved_xfrm(body).unwrap();
    let (_, l) = p.layout(two).unwrap();
    assert_eq!(
        Some(left),
        l.shapes
            .iter()
            .find(|x| x.placeholder.is_some_and(|ph| ph.idx == 1))
            .unwrap()
            .xfrm
    );
    // → Title Only: the body has text, so it stays (with its position); the empty one goes.
    let to = layout(&p, LayoutKind::TitleOnly);
    let tx = change_layout(&mut p, &[s], to).unwrap();
    roundtrip(&mut p, tx);
    let slide = p.slide(s).unwrap();
    assert_eq!(slide.shapes.len(), 2);
    assert_eq!(p.resolved_xfrm(body), Some(left));
    // Its animation survived; an animated empty placeholder would have lost its effect.
    assert_eq!(slide.timeline.sequence.len(), 1);
    assert!(change_layout(&mut p, &[s], NodeId(999_999)).is_err());
}

#[test]
fn reset_restores_layout_formatting_and_lost_placeholders() {
    let mut p = sample();
    let s = p.slides[1].id;
    let title = ph_of(&p, s, PlaceholderType::Title);
    let body = ph_of(&p, s, PlaceholderType::Object);
    let tx = move_shapes(&mut p, &[title], 100_000, 50_000).unwrap();
    roundtrip(&mut p, tx);
    assert!(p.shape(title).unwrap().xfrm.is_some());
    let tx = set_shape_props(&mut p, &[title], |sp| {
        sp.fill = Some(Fill::Solid(DrawingColor::scheme("accent2")))
    })
    .unwrap();
    p.apply(tx).unwrap();
    let tx = edit_text(&p, TextTarget::Shape(title), |d| {
        let para = d.body_paragraphs()[0].id;
        kiln_doc::edit::format_runs(d, Pos::new(para, 0), Pos::new(para, 4), |r| {
            r.bold = Some(true)
        })
    })
    .unwrap();
    p.apply(tx).unwrap();
    // Delete the body (removes its animation too), then Reset brings an empty one back.
    let tx = delete_shapes(&mut p, &[body]).unwrap();
    roundtrip(&mut p, tx);
    assert!(p.slide(s).unwrap().timeline.sequence.is_empty());
    let tx = reset_slides(&mut p, &[s]).unwrap();
    roundtrip(&mut p, tx);
    let t = p.shape(title).unwrap();
    assert!(t.xfrm.is_none());
    assert!(t.props().unwrap().is_empty());
    let para = t.text().unwrap().paragraphs().next().unwrap();
    assert!(para.content.runs().iter().all(|r| r.props.is_empty()));
    assert_eq!(t.text().unwrap().text(), "This year");
    let slide = p.slide(s).unwrap();
    assert!(slide.shapes.iter().any(|x| {
        x.placeholder
            .is_some_and(|ph| ph.kind == PlaceholderType::Object)
    }));
}

#[test]
fn backgrounds_notes_and_slide_size() {
    let mut p = sample();
    let s = p.slides[0].id;
    let bg = Background::solid(DrawingColor::scheme("accent2"));
    let tx = set_background(&mut p, &[s], Some(bg.clone())).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(
        p.resolved_background(crate::presentation::Owner::Slide(s)),
        Some(&bg)
    );
    let tx = set_background(&mut p, &[s], None).unwrap();
    roundtrip(&mut p, tx);
    let tx = set_notes_text(&mut p, s, "Hello").unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slide(s).unwrap().notes_text(), "Hello");
    let tx = set_notes_text(&mut p, s, "").unwrap();
    roundtrip(&mut p, tx);
    assert!(p.slide(s).unwrap().notes.is_none());

    // 16:9 → 4:3 with Ensure Fit: a slide shape scales by 0.75 and is centred vertically.
    let two = p.slides[2].id;
    let shape = p
        .slide(two)
        .unwrap()
        .shapes
        .iter()
        .find(|x| x.placeholder.is_none())
        .unwrap()
        .id;
    let before = p.shape(shape).unwrap().xfrm.unwrap();
    let tx = set_slide_size(&mut p, SlideSize::STANDARD, slides::ScaleMode::EnsureFit).unwrap();
    roundtrip(&mut p, tx);
    let after = p.shape(shape).unwrap().xfrm.unwrap();
    assert_eq!(after.cx, before.cx * 3 / 4);
    assert_eq!(after.x, before.x * 3 / 4);
    let oy = (6_858_000 - 6_858_000 * 3 / 4) / 2;
    assert_eq!(after.y, before.y * 3 / 4 + oy);
    // Layouts stretch to the new width.
    let (_, l) = p.layout(layout(&p, LayoutKind::TitleOnly)).unwrap();
    let title = l
        .shapes
        .iter()
        .find(|x| x.placeholder.is_some())
        .unwrap()
        .xfrm
        .unwrap();
    assert!(title.x + title.cx < SlideSize::STANDARD.cx);
    assert!(
        set_slide_size(
            &mut p,
            SlideSize::custom(0, 10),
            slides::ScaleMode::DontScale
        )
        .is_err()
    );
    let tx = set_slide_size(&mut p, SlideSize::STANDARD, slides::ScaleMode::Maximize).unwrap();
    assert_eq!(tx.ops.len(), 1, "same size: only the size op");
}

#[test]
fn sections_add_rename_move_and_remove() {
    let mut p = sample();
    let lens = |p: &Presentation| p.sections.iter().map(|s| s.len).collect::<Vec<_>>();
    let tx = add_section(&mut p, 1, "Middle").unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(lens(&p), vec![1, 2, 2]);
    let mid = p.sections[1].id;
    let tx = rename_section(&mut p, mid, "Core").unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.sections[1].name, "Core");
    let order: Vec<NodeId> = p.slides.iter().map(|s| s.id).collect();
    let tx = move_section(&mut p, mid, 1).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(lens(&p), vec![1, 2, 2]);
    assert_eq!(p.sections[2].id, mid);
    let now: Vec<NodeId> = p.slides.iter().map(|s| s.id).collect();
    assert_eq!(now, vec![order[0], order[3], order[4], order[1], order[2]]);
    let tx = move_section(&mut p, mid, -2).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.sections[0].id, mid);
    assert!(move_section(&mut p, mid, -1).unwrap().is_empty());
    let tx = remove_section(&mut p, mid, false).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.sections.len(), 2);
    assert_eq!(lens(&p).iter().sum::<usize>(), 5);
    let last = p.sections[1].id;
    let tx = remove_section(&mut p, last, true).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slides.len(), 3);
    // Adding the first section to a deck without sections creates "Default Section".
    let mut q = Presentation::new();
    let l = layout(&q, LayoutKind::TitleContent);
    let tx = insert_slide(&mut q, l, 1).unwrap();
    q.apply(tx).unwrap();
    let tx = add_section(&mut q, 1, "Two").unwrap();
    roundtrip(&mut q, tx);
    assert_eq!(q.sections[0].name, "Default Section");
    assert_eq!(lens(&q), vec![1, 1]);
    assert!(add_section(&mut q, 9, "x").is_err());
    assert!(rename_section(&mut q, NodeId(999_999), "x").is_err());
}

#[test]
fn shapes_insert_move_format_and_delete() {
    let mut p = sample();
    let s = p.slides[4].id;
    let r = Rect::new(0, 0, 100, 100);
    let sh = Shape::geometry(p.alloc_id(), "star5", r);
    let id = sh.id;
    let tx = insert_shapes(&mut p, s, vec![sh], Some(0)).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slide(s).unwrap().shapes[0].id, id);
    let tx = move_shapes(&mut p, &[id], 10, 20).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(
        p.shape(id).unwrap().xfrm.unwrap().rect(),
        Rect::new(10, 20, 100, 100)
    );
    let tx = set_transform(&mut p, id, Transform::new(1, 2, 3, 4)).unwrap();
    roundtrip(&mut p, tx);
    let tx = set_shape_props(&mut p, &[id], |sp| sp.fill = Some(Fill::None)).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.shape(id).unwrap().props().unwrap().fill, Some(Fill::None));
    let tx = set_shape_info(&mut p, id, |i| {
        i.name = "Star".into();
        i.alt_text = Some("A gold star".into());
    })
    .unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(
        p.shape(id).unwrap().alt_text.as_deref(),
        Some("A gold star")
    );
    assert!(set_shape_info(&mut p, id, |_| {}).unwrap().is_empty());
    // Crop a picture.
    let pic = p.slides[2]
        .shapes
        .iter()
        .find(|x| matches!(x.kind, ShapeKind::Picture(_)))
        .unwrap()
        .id;
    let crop = Crop {
        l: 10_000,
        t: 0,
        r: 5_000,
        b: 0,
    };
    let tx = set_crop(&mut p, pic, crop).unwrap();
    roundtrip(&mut p, tx);
    assert!(matches!(&p.shape(pic).unwrap().kind, ShapeKind::Picture(x) if x.crop == crop));
    assert!(set_crop(&mut p, id, crop).is_err());
    // Tables and charts have no shape properties.
    let table = p.slides[2]
        .shapes
        .iter()
        .find(|x| matches!(x.kind, ShapeKind::Table(_)))
        .unwrap()
        .id;
    assert!(set_shape_props(&mut p, &[table], |_| {}).is_err());
    // Delete an animated shape and a group child.
    let animated = p.slides[2].timeline.sequence[0].target.shape;
    let tx = delete_shapes(&mut p, &[animated]).unwrap();
    roundtrip(&mut p, tx);
    assert!(p.slides[2].timeline.sequence.is_empty());
    let group = p.slides[4]
        .shapes
        .iter()
        .find(|x| x.children().is_some())
        .unwrap()
        .clone();
    let child = group.children().unwrap()[0].id;
    let tx = delete_shapes(&mut p, &[child]).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.shape(group.id).unwrap().children().unwrap().len(), 1);
    let tx = delete_shapes(&mut p, &[group.id, group.children().unwrap()[1].id]).unwrap();
    roundtrip(&mut p, tx);
    assert!(p.shape(group.id).is_none());
    assert!(delete_shapes(&mut p, &[NodeId(999_999)]).is_err());
}

#[test]
fn z_order_commands() {
    let mut p = Presentation::new();
    let s = p.slides[0].id;
    let base = p.slide(s).unwrap().shapes.len();
    let shapes: Vec<Shape> = (0..4)
        .map(|i| Shape::geometry(p.alloc_id(), "rect", Rect::new(i, i, 10, 10)))
        .collect();
    let ids: Vec<NodeId> = shapes.iter().map(|x| x.id).collect();
    let tx = insert_shapes(&mut p, s, shapes, None).unwrap();
    p.apply(tx).unwrap();
    let order = |p: &Presentation| -> Vec<NodeId> {
        p.slide(s)
            .unwrap()
            .shapes
            .iter()
            .map(|x| x.id)
            .filter(|i| ids.contains(i))
            .collect()
    };
    let tx = bring_to_front(&mut p, &[ids[0]]).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(order(&p), vec![ids[1], ids[2], ids[3], ids[0]]);
    let tx = send_to_back(&mut p, &[ids[0]]).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slide(s).unwrap().shapes[0].id, ids[0]);
    assert_eq!(order(&p), ids);
    let _ = base;
    let tx = bring_forward(&mut p, &[ids[1], ids[2]]).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(order(&p), vec![ids[0], ids[3], ids[1], ids[2]]);
    let tx = send_backward(&mut p, &[ids[2]]).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(order(&p), vec![ids[0], ids[3], ids[2], ids[1]]);
    // Front stays front.
    let tx = bring_to_front(&mut p, &[ids[1]]).unwrap();
    assert!(tx.is_empty());
    let other = p.slides[0].shapes[0].id;
    let mut q = sample();
    let elsewhere = q.slides[2].shapes.last().unwrap().id;
    let _ = other;
    let there = q.slides[1].shapes[0].id;
    assert!(bring_forward(&mut q, &[elsewhere, there]).is_err());
}

#[test]
fn group_and_ungroup_keep_shapes_in_place() {
    let mut p = Presentation::new();
    let s = p.slides[0].id;
    let a = Shape::geometry(p.alloc_id(), "rect", Rect::new(1_000, 1_000, 2_000, 1_000));
    let mut b = Shape::geometry(
        p.alloc_id(),
        "ellipse",
        Rect::new(5_000, 4_000, 1_000, 1_000),
    );
    b.xfrm = b.xfrm.map(|x| x.with_rotation(30 * ROT_PER_DEGREE));
    let (ia, ib) = (a.id, b.id);
    let xa = a.xfrm.unwrap();
    let xb = b.xfrm.unwrap();
    let tx = insert_shapes(&mut p, s, vec![a, b], None).unwrap();
    p.apply(tx).unwrap();
    let tx = add_effect(&mut p, ia, AnimationEffect::Appear, None).unwrap();
    p.apply(tx).unwrap();
    let tx = group(&mut p, &[ib, ia]).unwrap();
    let gid = tx.selection_after[0];
    roundtrip(&mut p, tx);
    let g = p.shape(gid).unwrap();
    assert_eq!(g.children().unwrap().len(), 2);
    assert_eq!(
        g.xfrm.unwrap().rect(),
        Rect::new(1_000, 1_000, 2_000, 1_000).union(&xb.bounds())
    );
    assert!(
        p.slide(s).unwrap().timeline.sequence.is_empty(),
        "grouping drops child animations"
    );
    // Ungrouping an unmoved group gives the shapes back unchanged.
    let tx = ungroup(&mut p, gid).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.shape(ia).unwrap().xfrm, Some(xa));
    assert_eq!(p.shape(ib).unwrap().xfrm, Some(xb));
    // A group that was moved and doubled in size takes its children with it.
    let tx = group(&mut p, &[ia, ib]).unwrap();
    let gid = tx.selection_after[0];
    p.apply(tx).unwrap();
    let gx = p.shape(gid).unwrap().xfrm.unwrap();
    let big = Transform::new(gx.x + 500, gx.y, gx.cx * 2, gx.cy * 2);
    let tx = set_transform(&mut p, gid, big).unwrap();
    p.apply(tx).unwrap();
    let tx = ungroup(&mut p, gid).unwrap();
    roundtrip(&mut p, tx);
    let na = p.shape(ia).unwrap().xfrm.unwrap();
    assert_eq!(na.rect(), Rect::new(gx.x + 500, gx.y, 4_000, 2_000));
    // Errors.
    assert!(group(&mut p, &[ia]).is_err());
    assert!(ungroup(&mut p, ia).is_err());
    let title = ph_of(&p, s, PlaceholderType::CenteredTitle);
    assert!(group(&mut p, &[title, ia]).is_err());
}

#[test]
fn align_and_distribute() {
    let mut p = Presentation::new();
    let s = p.slides[0].id;
    let shapes: Vec<Shape> = [(0, 100), (300, 50), (1_000, 200)]
        .into_iter()
        .map(|(x, w)| Shape::geometry(p.alloc_id(), "rect", Rect::new(x, x / 2, w, 100)))
        .collect();
    let ids: Vec<NodeId> = shapes.iter().map(|x| x.id).collect();
    let tx = insert_shapes(&mut p, s, shapes, None).unwrap();
    p.apply(tx).unwrap();
    let x = |p: &Presentation, i: usize| p.shape(ids[i]).unwrap().xfrm.unwrap().rect();
    let tx = align(&mut p, &ids, Align::Left, AlignTo::Selection).unwrap();
    roundtrip(&mut p, tx);
    assert!((0..3).all(|i| x(&p, i).x == 0));
    let tx = align(&mut p, &ids, Align::Right, AlignTo::Selection).unwrap();
    p.apply(tx).unwrap();
    assert!((0..3).all(|i| x(&p, i).right() == 200));
    let tx = align(&mut p, &ids, Align::Middle, AlignTo::Slide).unwrap();
    roundtrip(&mut p, tx);
    assert!((0..3).all(|i| x(&p, i).center().1 == 6_858_000 / 2));
    for how in [Align::Center, Align::Top, Align::Bottom] {
        let tx = align(&mut p, &ids, how, AlignTo::Selection).unwrap();
        roundtrip(&mut p, tx);
    }
    // Distribute horizontally across the selection: outer shapes stay, equal gaps.
    let tx = align(&mut p, &ids, Align::Left, AlignTo::Selection).unwrap();
    p.apply(tx).unwrap();
    let tx = move_shapes(&mut p, &[ids[2]], 1_000, 0).unwrap();
    p.apply(tx).unwrap();
    let tx = distribute(&mut p, &ids, true, AlignTo::Selection).unwrap();
    roundtrip(&mut p, tx);
    let r: Vec<Rect> = (0..3).map(|i| x(&p, i)).collect();
    let mut sorted = r.clone();
    sorted.sort_by_key(|q| q.x);
    let gap1 = sorted[1].x - sorted[0].right();
    let gap2 = sorted[2].x - sorted[1].right();
    assert!((gap1 - gap2).abs() <= 1, "{gap1} {gap2}");
    // Across the slide.
    let tx = distribute(&mut p, &ids, false, AlignTo::Slide).unwrap();
    roundtrip(&mut p, tx);
    let ys: Vec<Rect> = (0..3).map(|i| x(&p, i)).collect();
    assert_eq!(ys.iter().map(|q| q.y).min(), Some(0));
    assert_eq!(ys.iter().map(Rect::bottom).max(), Some(6_858_000));
    assert!(
        distribute(&mut p, &ids[..2], true, AlignTo::Selection)
            .unwrap()
            .is_empty()
    );
    // Rotate and flip.
    let tx = rotate_by(&mut p, &ids[..1], 90 * ROT_PER_DEGREE).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(
        p.shape(ids[0]).unwrap().xfrm.unwrap().rot,
        90 * ROT_PER_DEGREE
    );
    let tx = flip(&mut p, &ids[..1], true).unwrap();
    roundtrip(&mut p, tx);
    let f = p.shape(ids[0]).unwrap().xfrm.unwrap();
    assert!(f.flip_h);
    assert_eq!(f.rot, 270 * ROT_PER_DEGREE);
    let tx = flip(&mut p, &ids[..1], false).unwrap();
    roundtrip(&mut p, tx);
}

#[test]
fn text_edits_delegate_to_kiln_doc() {
    let mut p = sample();
    let s = p.slides[1].id;
    let body = ph_of(&p, s, PlaceholderType::Object);
    let target = TextTarget::Shape(body);
    let first = p
        .shape(body)
        .unwrap()
        .text()
        .unwrap()
        .paragraphs()
        .next()
        .unwrap()
        .id;
    let tx = insert_text(&p, target, Pos::new(first, 0), "Fuel: ").unwrap();
    assert_eq!(tx.kind, EditKind::Typing);
    roundtrip(&mut p, tx);
    assert!(
        p.shape(body)
            .unwrap()
            .text()
            .unwrap()
            .text()
            .starts_with("Fuel: Gas")
    );
    // Enter splits a paragraph, and the new one keeps the DrawingML extras.
    let tx = set_para_extras(&p, target, &[first], |d| {
        d.bullet = Some(crate::text::Bullet::None)
    })
    .unwrap();
    roundtrip(&mut p, tx);
    let tx = edit_text(&p, target, |d| {
        kiln_doc::edit::split_paragraph(d, Pos::new(first, 4))
    })
    .unwrap();
    roundtrip(&mut p, tx);
    let body_text = p.shape(body).unwrap().text().unwrap();
    let second = body_text.paragraphs().nth(1).unwrap().id;
    assert_eq!(
        body_text.deck_props(second).unwrap().bullet,
        Some(crate::text::Bullet::None)
    );
    // Levels: Tab and Shift+Tab, clamped.
    let tx = set_paragraph_level(&p, target, &[second], 3).unwrap();
    roundtrip(&mut p, tx);
    let lvl = |p: &Presentation| {
        TextBody::level_of(
            p.shape(body)
                .unwrap()
                .text()
                .unwrap()
                .paragraph(second)
                .unwrap(),
        )
    };
    assert_eq!(lvl(&p), 3);
    let tx = set_paragraph_level(&p, target, &[second], -9).unwrap();
    p.apply(tx).unwrap();
    assert_eq!(lvl(&p), 0);
    assert!(
        set_paragraph_level(&p, target, &[second], -1)
            .unwrap()
            .is_empty()
    );
    // Body properties.
    let tx = set_body_props(&p, target, |b| {
        b.anchor = Some(crate::text::TextAnchor::Bottom)
    })
    .unwrap();
    roundtrip(&mut p, tx);
    // Replace all text.
    let tx = set_text(&p, target, "One\nTwo").unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.shape(body).unwrap().text().unwrap().text(), "One\nTwo");
    // Cells and notes are text targets too.
    let table = p.slides[2]
        .shapes
        .iter()
        .find(|x| matches!(x.kind, ShapeKind::Table(_)))
        .unwrap()
        .id;
    let cell = TextTarget::Cell {
        shape: table,
        row: 1,
        col: 2,
    };
    let tx = set_text(&p, cell, "42").unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(text::text_body(&p, cell).unwrap().text(), "42");
    let notes = TextTarget::Notes(s);
    let n = text::text_body(&p, notes)
        .unwrap()
        .paragraphs()
        .next()
        .unwrap()
        .id;
    let tx = insert_text(&p, notes, Pos::new(n, 0), "Hi. ").unwrap();
    roundtrip(&mut p, tx);
    // Errors.
    assert!(text::text_body(&p, TextTarget::Notes(p.slides[0].id)).is_err());
    assert!(
        text::text_body(
            &p,
            TextTarget::Cell {
                shape: table,
                row: 9,
                col: 0
            }
        )
        .is_err()
    );
    assert!(text::text_body(&p, TextTarget::Shape(table)).is_err());
    assert!(insert_text(&p, target, Pos::new(NodeId(999_999), 0), "x").is_err());
    assert!(set_para_extras(&p, target, &[NodeId(999_999)], |_| {}).is_err());
}

#[test]
fn transitions_and_animations() {
    let mut p = sample();
    let s = p.slides[2].id;
    let t = Transition::new(TransitionEffect::Push {
        dir: crate::transition::Side::Up,
    });
    let tx = set_transition(&mut p, &[s], Some(t.clone())).unwrap();
    roundtrip(&mut p, tx);
    let tx = apply_transition_to_all(&mut p, s).unwrap();
    roundtrip(&mut p, tx);
    assert!(p.slides.iter().all(|x| x.transition.as_ref() == Some(&t)));
    let tx = set_transition(&mut p, &[s], None).unwrap();
    roundtrip(&mut p, tx);

    let shape = p.slides[2]
        .shapes
        .iter()
        .find(|x| x.placeholder.is_none())
        .unwrap()
        .id;
    let tx = add_effect(
        &mut p,
        shape,
        AnimationEffect::Spin { degrees: 360 },
        Some(0),
    )
    .unwrap();
    let e = tx.selection_after[0];
    roundtrip(&mut p, tx);
    assert_eq!(p.slides[2].timeline.sequence[0].id, e);
    let tx = update_effect(&mut p, e, |x| {
        x.start = EffectStart::AfterPrevious;
        x.duration_ms = 1500;
        x.repeat = 0;
    })
    .unwrap();
    roundtrip(&mut p, tx);
    let eff = &p.slides[2].timeline.sequence[0];
    assert_eq!(
        (eff.start, eff.duration_ms, eff.repeat),
        (EffectStart::AfterPrevious, 1500, 1)
    );
    let tx = move_effect(&mut p, e, 1).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slides[2].timeline.position(e), Some(1));
    let tx = remove_effects(&mut p, &[e]).unwrap();
    roundtrip(&mut p, tx);
    // Layout and master shapes cannot be animated.
    let master_shape = p.masters[0].shapes[0].id;
    assert!(add_effect(&mut p, master_shape, AnimationEffect::Fade, None).is_err());
    assert!(move_effect(&mut p, e, 99).is_err());
    assert!(remove_effects(&mut p, &[NodeId(999_999)]).is_err());
}

#[test]
fn comments_thread() {
    let mut p = sample();
    let c = p.slides[1].comments[0].id;
    let by = comments::Byline {
        author: "Grace",
        initials: "GH",
        created: "2026-10-16T10:00:00Z",
    };
    let tx = reply_to_comment(&mut p, c, &by, "Yes").unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(p.slides[1].comments[0].replies.len(), 1);
    let tx = resolve_comment(&mut p, c, true).unwrap();
    roundtrip(&mut p, tx);
    assert!(resolve_comment(&mut p, c, true).unwrap().is_empty());
    let tx = delete_comment(&mut p, c).unwrap();
    roundtrip(&mut p, tx);
    let other = p.slides[2].shapes[0].id;
    let s1 = p.slides[1].id;
    assert!(add_comment(&mut p, s1, Some(other), (0, 0), &by, "x").is_err());
    assert!(delete_comment(&mut p, NodeId(999_999)).is_err());
}

#[test]
fn every_builder_leaves_ids_unique() {
    let p = sample();
    let ids = p.all_ids();
    let unique: HashSet<_> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
}

#[test]
fn custom_layouts_inherit_and_change() {
    use crate::geom::Transform as X;
    use crate::ops::Op;
    use crate::shape::Placeholder;
    use crate::slide::Layout;
    let mut p = sample();
    let master = p.masters[0].id;
    // A custom layout as imported masters have them: a picture placeholder at idx 13 and a
    // title, with its own position for the title.
    let mut l = Layout::new(p.alloc_id(), LayoutKind::Other("picTx2".into()));
    l.name = "Photo Story".into();
    let mut title = Shape::text_box(p.alloc_id(), Rect::new(0, 0, 10, 10), "Title");
    title.placeholder = Some(Placeholder::new(PlaceholderType::Title, 0));
    title.xfrm = Some(X::new(100, 200, 3_000_000, 600_000));
    let mut pic = Shape::text_box(
        p.alloc_id(),
        Rect::new(0, 1_000_000, 5_000_000, 3_000_000),
        "",
    );
    pic.placeholder = Some(Placeholder::new(PlaceholderType::Picture, 13));
    l.shapes = vec![title, pic];
    let lid = l.id;
    let tx = Transaction::new(vec![Op::InsertLayout {
        master,
        index: 11,
        layout: Box::new(l),
    }]);
    roundtrip(&mut p, tx);
    assert_eq!(p.layout(lid).unwrap().1.kind.as_ooxml(), "picTx2");
    // A slide changed to it gets the picture placeholder and keeps its title, which now sits
    // where the custom layout puts titles.
    let s = p.slides[1].id;
    let t = ph_of(&p, s, PlaceholderType::Title);
    let tx = change_layout(&mut p, &[s], lid).unwrap();
    roundtrip(&mut p, tx);
    assert_eq!(
        p.resolved_xfrm(t),
        Some(X::new(100, 200, 3_000_000, 600_000))
    );
    let pic_ph = ph_of(&p, s, PlaceholderType::Picture);
    assert_eq!(p.shape(pic_ph).unwrap().placeholder.unwrap().idx, 13);
    assert_eq!(
        p.resolved_xfrm(pic_ph).unwrap().rect(),
        Rect::new(0, 1_000_000, 5_000_000, 3_000_000)
    );
    // Its text still comes from the master's styles.
    assert_eq!(
        p.level_style(t, 0).unwrap().run.size,
        p.masters[0].title_style.level(0).run.size
    );
    // And back to a standard layout; the custom one cannot go while it is used.
    assert!(
        p.apply(Transaction::new(vec![Op::RemoveLayout {
            master,
            index: 11
        }]))
        .is_err()
    );
    let tc = layout(&p, LayoutKind::TitleContent);
    let tx = change_layout(&mut p, &[s], tc).unwrap();
    roundtrip(&mut p, tx);
    p.apply(Transaction::new(vec![Op::RemoveLayout {
        master,
        index: 11,
    }]))
    .unwrap();
    p.validate().unwrap();
}
