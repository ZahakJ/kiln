// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every op's inverse, every op's refusals, transaction atomicity, undo grouping, and a property
//! test over random edit sequences.

use std::collections::BTreeMap;

use kiln_common::NodeId;
use kiln_doc::Pos;
use kiln_ooxml::drawingml::color::DrawingColor;
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::drawingml::theme::office_theme;
use proptest::prelude::*;

use super::*;
use crate::animation::{AnimationEffect, Effect, TriggerSequence};
use crate::edit::tests::{layout, ph_of, roundtrip, sample};
use crate::geom::{ChildSpace, Rect, Transform};
use crate::presentation::{Part, Presentation};
use crate::shape::{Placeholder, PlaceholderType, Shape, ShapeKind, ShapeProps};
use crate::slide::{
    Background, ColorMapping, Comment, HeaderFooter, Layout, LayoutKind, Notes, Section, SlideSize,
};
use crate::text::{Bullet, DeckParaProps, ListStyle, TextAnchor, TextBody};
use crate::transition::{Transition, TransitionEffect};
use crate::undo::UndoStack;

fn one(op: Op) -> Transaction {
    Transaction::new(vec![op])
}

fn comment(id: NodeId) -> Comment {
    Comment {
        id,
        author: "A".into(),
        initials: "A".into(),
        created: "2026-01-01T00:00:00Z".into(),
        text: "t".into(),
        shape: None,
        pos: (0, 0),
        replies: Vec::new(),
        resolved: false,
    }
}

/// One instance of every op, valid against the sample.
fn every_op(p: &mut Presentation) -> Vec<Op> {
    let s1 = p.slides[1].id;
    let s2 = p.slides[2].id;
    let s4 = p.slides[4].id;
    let master = p.masters[0].id;
    let lay = layout(p, LayoutKind::TitleOnly);
    let title = ph_of(p, s1, PlaceholderType::Title);
    let body = ph_of(p, s1, PlaceholderType::Object);
    let table = p.slides[2]
        .shapes
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::Table(_)))
        .unwrap()
        .id;
    let pic = p.slides[2]
        .shapes
        .iter()
        .find(|s| matches!(s.kind, ShapeKind::Picture(_)))
        .unwrap()
        .clone();
    let group = p.slides[4]
        .shapes
        .iter()
        .find(|s| s.children().is_some())
        .unwrap()
        .id;
    let free = p.slides[2]
        .shapes
        .iter()
        .find(|s| {
            s.placeholder.is_none()
                && p.slides[2]
                    .timeline
                    .sequence
                    .iter()
                    .all(|e| e.target.shape != s.id)
        })
        .unwrap()
        .id;
    let animated = p.slides[2].timeline.sequence[0].target.shape;
    let para = p
        .shape(body)
        .unwrap()
        .text()
        .unwrap()
        .paragraphs()
        .next()
        .unwrap()
        .id;
    let new_layout = {
        let mut l = Layout::new(p.alloc_id(), LayoutKind::Custom);
        l.shapes
            .push(Shape::text_box(p.alloc_id(), Rect::new(0, 0, 10, 10), "x"));
        l
    };
    let new_master = {
        let theme = crate::themes::bundled_theme("jade").unwrap();
        let size = p.slide_size.clone();
        theme.build_master(p, &size, None)
    };
    let new_slide = crate::edit::new_slide(p, lay);
    let new_shape = Shape::geometry(p.alloc_id(), "hexagon", Rect::new(0, 0, 50, 50));
    let new_effect = Effect::new(p.alloc_id(), free, AnimationEffect::Pulse);
    let trigger = TriggerSequence {
        trigger: free,
        effects: vec![Effect::new(p.alloc_id(), free, AnimationEffect::Teeter)],
    };
    let new_comment = comment(p.alloc_id());
    let mut changed_comment = p.slides[1].comments[0].clone();
    changed_comment.text = "changed".into();
    let effect0 = p.slides[2].timeline.sequence[0].clone();
    let mut slow = effect0.clone();
    slow.duration_ms = 3000;
    let doc_tx = {
        let mut d = p.shape(body).unwrap().text().unwrap().doc().clone();
        kiln_doc::edit::insert_text(&mut d, Pos::new(para, 0), "Hi ").unwrap()
    };
    let mut cropped = pic.clone();
    if let ShapeKind::Picture(x) = &mut cropped.kind {
        x.crop.l = 5_000;
    }
    let new_section = Section {
        id: p.alloc_id(),
        name: "All".into(),
        len: p.slides.len(),
    };
    vec![
        Op::SetSlideSize {
            size: SlideSize::STANDARD,
        },
        Op::SetDefaultTextStyle {
            style: Box::default(),
        },
        Op::SetSections {
            sections: vec![new_section],
        },
        Op::SetPart {
            key: "ppt/media/x.png".into(),
            part: Some(Part {
                content_type: "image/png".into(),
                bytes: vec![1],
            }),
        },
        Op::SetPart {
            key: "ppt/media/image1.png".into(),
            part: None,
        },
        Op::InsertMaster {
            index: 1,
            master: Box::new(new_master),
        },
        Op::SetTheme {
            master,
            theme: Box::new(office_theme()),
        },
        Op::SetMasterTextStyle {
            master,
            which: MasterStyle::Body,
            style: Box::default(),
        },
        Op::SetMasterTextStyle {
            master,
            which: MasterStyle::Title,
            style: Box::default(),
        },
        Op::SetMasterTextStyle {
            master,
            which: MasterStyle::Other,
            style: Box::default(),
        },
        Op::SetHeaderFooter {
            master,
            header_footer: HeaderFooter {
                date: false,
                ..HeaderFooter::default()
            },
        },
        Op::InsertLayout {
            master,
            index: 0,
            layout: Box::new(new_layout),
        },
        Op::RemoveLayout { master, index: 10 },
        Op::MoveLayout {
            master,
            from: 0,
            to: 4,
        },
        Op::SetLayoutName {
            layout: lay,
            name: "Only a title".into(),
        },
        Op::SetBackground {
            owner: s1,
            background: Some(Background::solid(DrawingColor::scheme("accent3"))),
        },
        Op::SetBackground {
            owner: lay,
            background: Some(Background::solid(DrawingColor::scheme("accent4"))),
        },
        Op::SetBackground {
            owner: master,
            background: Some(Background::solid(DrawingColor::scheme("bg2"))),
        },
        Op::SetBackground {
            owner: p.notes_master.as_ref().unwrap().id,
            background: Some(Background::solid(DrawingColor::scheme("bg2"))),
        },
        Op::SetColorMap {
            owner: s1,
            map: Some(ColorMapping::dark()),
        },
        Op::SetColorMap {
            owner: lay,
            map: Some(ColorMapping::dark()),
        },
        Op::SetColorMap {
            owner: master,
            map: Some(ColorMapping::dark()),
        },
        Op::SetShowMasterShapes {
            owner: s1,
            show: false,
        },
        Op::SetShowMasterShapes {
            owner: lay,
            show: false,
        },
        Op::InsertSlide {
            index: 0,
            slide: Box::new(new_slide),
        },
        Op::RemoveSlide { index: 3 },
        Op::MoveSlide { from: 0, to: 4 },
        Op::SetSlideLayout {
            slide: s1,
            layout: lay,
        },
        Op::SetSlideHidden {
            slide: s1,
            hidden: true,
        },
        Op::SetSlideName {
            slide: s1,
            name: Some("Money".into()),
        },
        Op::SetTransition {
            slide: s4,
            transition: Some(Transition::new(TransitionEffect::Flash)),
        },
        Op::SetNotes {
            slide: s4,
            notes: Some(Box::new(Notes {
                text: TextBody::from_text("n"),
                shapes: Vec::new(),
            })),
        },
        Op::SetNotes {
            slide: s1,
            notes: None,
        },
        Op::InsertEffect {
            slide: s2,
            index: 1,
            effect: Box::new(new_effect),
        },
        Op::RemoveEffect {
            slide: s2,
            index: 0,
        },
        Op::SetEffect {
            slide: s2,
            index: 0,
            effect: Box::new(slow),
        },
        Op::MoveEffect {
            slide: s2,
            from: 0,
            to: 0,
        },
        Op::SetTriggers {
            slide: s2,
            triggers: vec![trigger],
        },
        Op::InsertComment {
            slide: s2,
            index: 0,
            comment: Box::new(new_comment),
        },
        Op::RemoveComment {
            slide: s1,
            index: 0,
        },
        Op::SetComment {
            slide: s1,
            index: 0,
            comment: Box::new(changed_comment),
        },
        Op::InsertShapes {
            parent: group,
            index: 1,
            shapes: vec![new_shape],
        },
        Op::RemoveShapes {
            parent: s2,
            index: 0,
            count: 1,
        },
        Op::MoveShape {
            parent: s2,
            from: 0,
            to: 2,
        },
        Op::SetTransform {
            shape: title,
            xfrm: Some(Transform::new(1, 2, 3, 4)),
        },
        Op::SetTransform {
            shape: free,
            xfrm: None,
        },
        Op::SetShapeProps {
            shape: free,
            props: Box::new(ShapeProps {
                fill: Some(Fill::Solid(DrawingColor::srgb("FF0000"))),
                ..ShapeProps::default()
            }),
        },
        Op::SetPlaceholder {
            shape: title,
            placeholder: Some(Placeholder::new(PlaceholderType::Body, 7)),
        },
        Op::SetShapeInfo {
            shape: title,
            info: Box::new(ShapeInfo {
                name: "T".into(),
                hidden: true,
                ..ShapeInfo::default()
            }),
        },
        Op::SetShapeKind {
            shape: pic.id,
            kind: Box::new(cropped.kind),
        },
        Op::SetChildSpace {
            group,
            space: ChildSpace {
                x: 0,
                y: 0,
                cx: 10,
                cy: 10,
            },
        },
        Op::SetTextBody {
            shape: free,
            text: Some(Box::new(TextBody::from_text("label"))),
        },
        Op::SetBodyProps {
            target: TextTarget::Shape(body),
            body: Box::new(crate::text::BodyProps {
                anchor: Some(TextAnchor::Bottom),
                ..crate::text::BodyProps::default()
            }),
        },
        Op::SetListStyle {
            target: TextTarget::Cell {
                shape: table,
                row: 0,
                col: 0,
            },
            style: Box::new(ListStyle::default().over(&crate::themes::default_body_style())),
        },
        Op::SetParaProps {
            target: TextTarget::Shape(body),
            para,
            props: Some(Box::new(DeckParaProps {
                bullet: Some(Bullet::Char("→".into())),
                ..DeckParaProps::default()
            })),
        },
        Op::EditText {
            target: TextTarget::Shape(body),
            tx: doc_tx,
            para_props: None,
        },
        Op::EditText {
            target: TextTarget::Notes(s1),
            tx: kiln_doc::Transaction::default(),
            para_props: Some(BTreeMap::new()),
        },
        // Removing the animated shape after its effect: in one transaction (below).
        Op::RemoveEffect {
            slide: s2,
            index: 0,
        },
    ]
    .into_iter()
    .chain(std::iter::once(Op::RemoveShapes {
        parent: s2,
        index: p.slides[2]
            .shapes
            .iter()
            .position(|s| s.id == animated)
            .unwrap(),
        count: 1,
    }))
    .collect()
}

#[test]
fn every_op_is_undone_by_its_inverse() {
    let mut probe = sample();
    // Slide inserts and removals change the partition; without sections each op stands alone.
    probe.sections.clear();
    let ops = every_op(&mut probe);
    // Every variant is covered.
    let mut kinds: Vec<std::mem::Discriminant<Op>> =
        ops.iter().map(std::mem::discriminant).collect();
    kinds.sort_by_key(|d| format!("{d:?}"));
    kinds.dedup();
    // Every variant but RemoveMaster (exercised as InsertMaster's inverse).
    assert_eq!(kinds.len(), 45, "one test op per Op variant");
    let n = ops.len();
    for (i, op) in ops.into_iter().enumerate() {
        let mut p = probe.clone();
        if i + 2 == n {
            // The effect removal pairs with the shape removal that follows it.
            continue;
        }
        let tx = if i + 1 == n {
            Transaction::new(vec![
                Op::RemoveEffect {
                    slide: p.slides[2].id,
                    index: 0,
                },
                op,
            ])
        } else {
            one(op.clone())
        };
        let before = p.clone();
        roundtrip(&mut p, tx);
        assert_eq!(p.validate(), Ok(()), "op {i}");
        let _ = before;
    }
}

#[test]
fn ops_refuse_bad_input_and_change_nothing() {
    let mut p = sample();
    let s1 = p.slides[1].id;
    let s2 = p.slides[2].id;
    let master = p.masters[0].id;
    let animated = p.slides[2].timeline.sequence[0].target.shape;
    let animated_index = p.slides[2]
        .shapes
        .iter()
        .position(|s| s.id == animated)
        .unwrap();
    let used_layout = p.slides[1].layout;
    let used_index = p.masters[0]
        .layouts
        .iter()
        .position(|l| l.id == used_layout)
        .unwrap();
    let existing = p.slides[1].shapes[0].clone();
    let mut dup_slide = p.slides[1].clone();
    dup_slide.id = p.alloc_id();
    let bad = vec![
        Op::SetSlideSize {
            size: SlideSize::custom(0, 1),
        },
        Op::SetSlideSize {
            size: SlideSize::custom(1, SlideSize::MAX_SIDE + 1),
        },
        Op::RemoveMaster { index: 0 },
        Op::RemoveMaster { index: 5 },
        Op::InsertMaster {
            index: 9,
            master: Box::new(p.masters[0].clone()),
        },
        Op::RemoveLayout {
            master,
            index: used_index,
        },
        Op::RemoveLayout { master, index: 99 },
        Op::MoveLayout {
            master,
            from: 0,
            to: 99,
        },
        Op::SetBackground {
            owner: master,
            background: None,
        },
        Op::SetBackground {
            owner: NodeId(999_999),
            background: None,
        },
        Op::SetColorMap {
            owner: master,
            map: None,
        },
        Op::SetShowMasterShapes {
            owner: master,
            show: true,
        },
        Op::InsertSlide {
            index: 0,
            slide: Box::new(dup_slide),
        },
        Op::RemoveSlide { index: 99 },
        Op::MoveSlide { from: 0, to: 99 },
        Op::SetSlideLayout {
            slide: s1,
            layout: NodeId(999_999),
        },
        Op::InsertEffect {
            slide: s1,
            index: 0,
            effect: Box::new(Effect::new(
                NodeId(999_998),
                p.slides[2].shapes[0].id,
                AnimationEffect::Fade,
            )),
        },
        Op::SetEffect {
            slide: s2,
            index: 0,
            effect: Box::new(Effect::new(
                NodeId(999_997),
                animated,
                AnimationEffect::Fade,
            )),
        },
        Op::RemoveEffect {
            slide: s1,
            index: 9,
        },
        Op::RemoveShapes {
            parent: s2,
            index: animated_index,
            count: 1,
        },
        Op::RemoveShapes {
            parent: s2,
            index: 0,
            count: usize::MAX,
        },
        Op::InsertShapes {
            parent: s2,
            index: 0,
            shapes: vec![existing],
        },
        Op::InsertShapes {
            parent: NodeId(999_999),
            index: 0,
            shapes: vec![],
        },
        Op::MoveShape {
            parent: s2,
            from: 99,
            to: 0,
        },
        Op::SetShapeProps {
            shape: p.slides[2]
                .shapes
                .iter()
                .find(|s| matches!(s.kind, ShapeKind::Table(_)))
                .unwrap()
                .id,
            props: Box::default(),
        },
        Op::SetChildSpace {
            group: animated,
            space: ChildSpace::default(),
        },
        Op::SetTextBody {
            shape: p.slides[2]
                .shapes
                .iter()
                .find(|s| matches!(s.kind, ShapeKind::Picture(_)))
                .unwrap()
                .id,
            text: None,
        },
        Op::SetParaProps {
            target: TextTarget::Shape(ph_of(&p, s1, PlaceholderType::Title)),
            para: NodeId(999_999),
            props: None,
        },
        Op::SetBodyProps {
            target: TextTarget::Notes(p.slides[0].id),
            body: Box::default(),
        },
        Op::InsertComment {
            slide: s1,
            index: 0,
            comment: Box::new(comment(p.slides[1].comments[0].id)),
        },
        Op::SetComment {
            slide: s1,
            index: 0,
            comment: Box::new(comment(NodeId(999_990))),
        },
        Op::SetTriggers {
            slide: s1,
            triggers: vec![TriggerSequence {
                trigger: NodeId(999_999),
                effects: vec![],
            }],
        },
    ];
    for (i, op) in bad.into_iter().enumerate() {
        let before = p.clone();
        assert!(
            p.apply(one(op.clone())).is_err(),
            "op {i} should fail: {op:?}"
        );
        assert_eq!(p, before, "op {i} changed the presentation");
    }
    // A transaction that fails half-way rolls back the ops that worked.
    let before = p.clone();
    let tx = Transaction::new(vec![
        Op::SetSlideHidden {
            slide: s1,
            hidden: true,
        },
        Op::RemoveSlide { index: 99 },
    ]);
    assert!(p.apply(tx).is_err());
    assert_eq!(p, before);
    // Sections must still partition the slides at the end.
    let tx = Transaction::new(vec![Op::RemoveSlide { index: 0 }]);
    assert!(p.apply(tx).is_err());
    assert_eq!(p, before);
    // A text edit that would put a table in a text body is refused.
    let body = ph_of(&p, s1, PlaceholderType::Object);
    let para = p
        .shape(body)
        .unwrap()
        .text()
        .unwrap()
        .paragraphs()
        .next()
        .unwrap()
        .id;
    let mut d = p.shape(body).unwrap().text().unwrap().doc().clone();
    let t = kiln_doc::edit::insert_table(&mut d, Pos::new(para, 0), 1, 1).unwrap();
    assert!(
        p.apply(one(Op::EditText {
            target: TextTarget::Shape(body),
            tx: t,
            para_props: None,
        }))
        .is_err()
    );
    assert_eq!(p, before);
}

#[test]
fn transactions_serialise() {
    let mut p = sample();
    let ops = every_op(&mut p);
    let tx = Transaction::new(ops)
        .with_kind(EditKind::Structure)
        .with_selection(vec![NodeId(1)], vec![NodeId(2)]);
    let bytes = tx.to_bytes().unwrap();
    assert_eq!(Transaction::from_bytes(&bytes).unwrap(), tx);
    assert!(Transaction::from_bytes(b"KDTX").is_err());
    assert!(Transaction::from_bytes(b"nope").is_err());
    let mut wrong_version = b"KDTX".to_vec();
    wrong_version.extend_from_slice(&9u16.to_le_bytes());
    assert!(Transaction::from_bytes(&wrong_version).is_err());
    let mut trailing = bytes.clone();
    trailing.push(0);
    assert!(Transaction::from_bytes(&trailing).is_err());
}

#[test]
fn presentations_serialise_and_refuse_damage() {
    let p = sample();
    let bytes = p.to_bytes().unwrap();
    let back = Presentation::from_bytes(&bytes).unwrap();
    assert_eq!(back, p);
    assert!(Presentation::from_bytes(&bytes[..bytes.len() / 2]).is_err());
    assert!(Presentation::from_bytes(b"KDOC\x01\x00").is_err());
    let mut trailing = bytes.clone();
    trailing.push(7);
    assert!(Presentation::from_bytes(&trailing).is_err());
    // A structurally broken model is refused on load.
    let mut broken = p.clone();
    broken.slides[0].layout = NodeId(424_242);
    assert!(Presentation::from_bytes(&broken.to_bytes().unwrap()).is_err());
    let mut broken = p.clone();
    broken.sections[0].len += 1;
    assert!(Presentation::from_bytes(&broken.to_bytes().unwrap()).is_err());
    let mut broken = p;
    let dup = broken.slides[0].shapes[0].clone();
    broken.slides[1].shapes.push(dup);
    assert!(Presentation::from_bytes(&broken.to_bytes().unwrap()).is_err());
    // JSON works too (the control channel).
    let json = serde_json::to_string(&back).unwrap();
    let from_json: Presentation = serde_json::from_str(&json).unwrap();
    assert_eq!(from_json, back);
}

#[test]
fn undo_groups_typing_per_word_and_explicit_groups() {
    let mut p = sample();
    let s1 = p.slides[1].id;
    let body = ph_of(&p, s1, PlaceholderType::Object);
    let target = TextTarget::Shape(body);
    let para = p
        .shape(body)
        .unwrap()
        .text()
        .unwrap()
        .paragraphs()
        .next()
        .unwrap()
        .id;
    let original = p.clone();
    let mut u = UndoStack::default();
    let mut offset = 0;
    for chunk in ["h", "i", " ", "t", "h", "e", "r", "e"] {
        let tx = crate::edit::insert_text(&p, target, Pos::new(para, offset), chunk).unwrap();
        u.apply(&mut p, tx).unwrap();
        offset += 1;
    }
    assert_eq!(u.undo_len(), 2, "one step per word");
    assert_eq!(u.undo_kind(), Some(EditKind::Typing));
    assert!(
        p.shape(body)
            .unwrap()
            .text()
            .unwrap()
            .text()
            .starts_with("hi thereGas")
    );
    u.undo(&mut p).unwrap();
    assert!(
        p.shape(body)
            .unwrap()
            .text()
            .unwrap()
            .text()
            .starts_with("hi Gas")
    );
    u.undo(&mut p).unwrap();
    assert_eq!(p, original);
    assert!(u.can_redo());
    u.redo(&mut p).unwrap();
    u.redo(&mut p).unwrap();
    assert!(
        p.shape(body)
            .unwrap()
            .text()
            .unwrap()
            .text()
            .starts_with("hi thereGas")
    );
    assert!(u.redo(&mut p).unwrap().is_none());
    // Breaking the group starts a new step.
    u.break_group();
    let tx = crate::edit::insert_text(&p, target, Pos::new(para, offset), "x").unwrap();
    u.apply(&mut p, tx).unwrap();
    assert_eq!(u.undo_len(), 3);
    // An explicit group (a drag) is one step however many transactions it has.
    let free = p.slides[2]
        .shapes
        .iter()
        .find(|s| s.placeholder.is_none())
        .unwrap()
        .id;
    let before_drag = p.clone();
    u.begin_group();
    for _ in 0..5 {
        let tx = crate::edit::move_shapes(&mut p, &[free], 10, 0).unwrap();
        u.apply(&mut p, tx).unwrap();
    }
    u.end_group();
    assert_eq!(u.undo_len(), 4);
    u.undo(&mut p).unwrap();
    assert_eq!(p, before_drag);
    u.redo(&mut p).unwrap();
    // Empty transactions are not recorded; the limit drops the oldest steps.
    u.apply(&mut p, Transaction::default()).unwrap();
    assert_eq!(u.undo_len(), 4);
    let mut small = UndoStack::new(2);
    for i in 0..4 {
        let tx = crate::edit::move_shapes(&mut p, &[free], i, 0).unwrap();
        small.apply(&mut p, tx).unwrap();
    }
    assert_eq!(small.undo_len(), 2);
    small.clear();
    assert!(!small.can_undo() && !small.can_redo());
    assert!(small.undo(&mut p).unwrap().is_none());
}

/// One random edit, chosen and parameterised by the strategy's numbers.
fn random_edit(p: &mut Presentation, kind: u8, a: usize, b: usize, c: i64) -> Option<Transaction> {
    let n = p.slides.len();
    let slide = (n > 0).then(|| p.slides[a % n].id);
    let shapes: Vec<NodeId> = slide
        .and_then(|s| p.slide(s))
        .map(|s| s.shapes.iter().map(|x| x.id).collect())
        .unwrap_or_default();
    let shape = (!shapes.is_empty()).then(|| shapes[b % shapes.len()]);
    let layouts: Vec<NodeId> = p.masters[0].layouts.iter().map(|l| l.id).collect();
    let l = layouts[b % layouts.len()];
    use crate::edit::*;
    match kind % 22 {
        0 => insert_slide(p, l, a % (n + 1)).ok(),
        1 => duplicate_slides(p, &[slide?]).ok(),
        2 if n > 1 => delete_slides(p, &[slide?]).ok(),
        3 => move_slides(p, &[slide?], b % (n + 1)).ok(),
        4 => set_hidden(p, &[slide?], c % 2 == 0).ok(),
        5 => change_layout(p, &[slide?], l).ok(),
        6 => reset_slides(p, &[slide?]).ok(),
        7 => {
            let s = Shape::geometry(
                p.alloc_id(),
                "rect",
                Rect::new(c.rem_euclid(1000), 0, 100, 100),
            );
            insert_shapes(p, slide?, vec![s], None).ok()
        }
        8 => delete_shapes(p, &[shape?]).ok(),
        9 => move_shapes(p, &[shape?], c, -c).ok(),
        10 => bring_forward(p, &[shape?]).ok(),
        11 => send_to_back(p, &[shape?]).ok(),
        12 if shapes.len() >= 2 => group(p, &[shapes[0], shapes[shapes.len() - 1]]).ok(),
        13 => ungroup(p, shape?).ok(),
        14 => {
            let t = p.shape(shape?)?.text()?.paragraphs().next()?.id;
            insert_text(p, TextTarget::Shape(shape?), Pos::new(t, 0), "ab c").ok()
        }
        15 => set_text(p, TextTarget::Shape(shape?), "x\ny").ok(),
        16 => add_effect(p, shape?, AnimationEffect::Fade, None).ok(),
        17 => {
            let e = p.slide(slide?)?.timeline.sequence.first()?.id;
            remove_effects(p, &[e]).ok()
        }
        18 => add_section(p, a % (n + 1), "S").ok(),
        19 => {
            let sec = p.sections.get(b % p.sections.len().max(1))?.id;
            move_section(p, sec, if c % 2 == 0 { 1 } else { -1 }).ok()
        }
        20 => {
            let sec = p.sections.get(b % p.sections.len().max(1))?.id;
            remove_section(p, sec, c % 3 == 0).ok()
        }
        _ => align(
            p,
            &shapes.iter().copied().take(3).collect::<Vec<_>>(),
            Align::Center,
            AlignTo::Selection,
        )
        .ok(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 48, ..ProptestConfig::default() })]

    /// Random edit sequences keep the model valid; undoing everything restores the start,
    /// redoing everything restores the end, and the result survives serialisation.
    #[test]
    fn random_edits_undo_and_redo_exactly(
        steps in proptest::collection::vec((any::<u8>(), any::<usize>(), any::<usize>(), -5000i64..5000), 1..30)
    ) {
        let mut p = sample();
        let start = p.clone();
        let mut u = UndoStack::default();
        for (k, a, b, c) in steps {
            if let Some(tx) = random_edit(&mut p, k, a, b, c) {
                let before = p.clone();
                match u.apply(&mut p, tx) {
                    Ok(()) => prop_assert!(p.validate().is_ok(), "{:?}", p.validate()),
                    Err(_) => prop_assert_eq!(&p, &before),
                }
                u.break_group();
            }
        }
        let end = p.clone();
        while u.can_undo() {
            u.undo(&mut p).unwrap();
        }
        prop_assert_eq!(&p, &start);
        while u.can_redo() {
            u.redo(&mut p).unwrap();
        }
        prop_assert_eq!(&p, &end);
        let back = Presentation::from_bytes(&p.to_bytes().unwrap()).unwrap();
        prop_assert_eq!(back, end);
    }

    /// Arbitrary bytes never panic the loader.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let mut with_header = b"KDCK\x01\x00".to_vec();
        with_header.extend(&bytes);
        let _ = Presentation::from_bytes(&with_header);
        let _ = Presentation::from_bytes(&bytes);
        let mut tx = b"KDTX\x01\x00".to_vec();
        tx.extend(&bytes);
        let _ = Transaction::from_bytes(&tx);
    }

    /// Theme files with random damage are refused, never panic.
    #[test]
    fn damaged_theme_files_never_panic(cut in 0usize..9000, junk in "[<>a-z\"=/ ]{0,12}") {
        let src = crate::themes::BUNDLED[0].source;
        let cut = cut.min(src.len());
        let mut s = String::new();
        let head: String = src.chars().take(cut).collect();
        s.push_str(&head);
        s.push_str(&junk);
        if let Ok(t) = crate::themes::ThemeFile::parse(&s) {
            let mut p = Presentation::empty();
            let size = SlideSize::WIDESCREEN;
            let m = t.build_master(&mut p, &size, Some(0));
            prop_assert_eq!(m.layouts.len(), 11);
        }
    }
}
