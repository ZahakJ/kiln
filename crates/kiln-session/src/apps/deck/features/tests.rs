// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Transitions and Animations tabs, one behaviour per test, and their round trip through
//! `.pptx`.

use kiln_common::NodeId;
use kiln_deck::Presentation;
use kiln_deck::animation::{AnimationEffect, EffectStart};
use kiln_deck::transition::{Side, TransitionEffect};
use kiln_protocol::commands::deck_show::DeckEffectStart;
use kiln_protocol::commands::{DeckCommand as D, DeckRect};
use kiln_protocol::{Command, FileFormat, SuiteCommand};

use super::vocab;
use crate::apps::deck::DeckDoc;
use crate::model::{Change, DocModel};

fn doc() -> DeckDoc {
    DeckDoc::new(Presentation::new(), None, None)
}

fn run(d: &mut DeckDoc, c: D) -> Change {
    d.execute(&Command::Deck(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"))
}

fn fails(d: &mut DeckDoc, c: D) -> String {
    d.execute(&Command::Deck(c))
        .expect_err("should fail")
        .to_string()
}

/// A deck with a rectangle selected on its slide.
fn with_shape() -> (DeckDoc, NodeId) {
    let mut d = doc();
    run(
        &mut d,
        D::InsertShape {
            preset: "rect".into(),
            rect: Some(DeckRect {
                x: 400.0,
                y: 200.0,
                width: 300.0,
                height: 200.0,
            }),
        },
    );
    let id = d.view.shapes[0];
    (d, id)
}

/// Saves as `.pptx` and opens the bytes again.
fn reopen(d: &DeckDoc) -> DeckDoc {
    let bytes = d.write(FileFormat::Pptx, None).expect("saves");
    let read = kiln_io_pptx::read_pptx(&bytes).expect("reads");
    let (pres, original) = crate::apps::deck::save::Original::from_read(read, &bytes);
    DeckDoc::new(pres, Some(original), None)
}

#[test]
fn every_transition_of_the_gallery_is_set_and_saved() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    for e in kiln_deck::transition::GALLERY {
        let id = vocab::transition_id(&e);
        run(&mut d, D::SetTransition { effect: id.into() });
        let st = d.motion_state();
        let t = st.transition.expect("a slide");
        assert_eq!(t.effect, id);
        assert_eq!(t.duration_ms, e.default_duration_ms());
        assert_eq!(t.options.len(), vocab::transition_options(&e).len());
        if e == TransitionEffect::None {
            continue;
        }
        let back = reopen(&d);
        let slide = &back.pres.slides[d.view.current];
        assert_eq!(slide.transition.as_ref().map(|t| t.effect), Some(e), "{id}");
    }
    assert!(
        fails(
            &mut d,
            D::SetTransition {
                effect: "spiral".into()
            }
        )
        .contains("spiral")
    );
}

#[test]
fn effect_options_duration_and_advance_change_the_transition() {
    let mut d = doc();
    run(
        &mut d,
        D::SetTransition {
            effect: "push".into(),
        },
    );
    run(
        &mut d,
        D::TransitionOption {
            option: "fromLeft".into(),
        },
    );
    run(&mut d, D::TransitionDuration { ms: 1250 });
    run(&mut d, D::AdvanceOnClick { on: None });
    run(&mut d, D::AdvanceAfter { ms: Some(3000) });
    let t = d.pres.slides[0].transition.clone().unwrap();
    assert_eq!(t.effect, TransitionEffect::Push { dir: Side::Right });
    assert_eq!(t.duration_ms, 1250);
    assert!(!t.advance_on_click);
    assert_eq!(t.advance_after_ms, Some(3000));
    let info = d.motion_state().transition.unwrap();
    assert_eq!(info.option.as_deref(), Some("fromLeft"));
    assert!(
        fails(
            &mut d,
            D::TransitionOption {
                option: "throughBlack".into()
            }
        )
        .contains("Push")
    );
    // Saved as PowerPoint writes it.
    let back = reopen(&d);
    assert_eq!(back.pres.slides[0].transition, Some(t));
    // Undo goes back a step at a time.
    assert!(d.undo());
    assert_eq!(
        d.pres.slides[0]
            .transition
            .as_ref()
            .unwrap()
            .advance_after_ms,
        None
    );
}

#[test]
fn timings_without_an_effect_and_none_without_timings() {
    let mut d = doc();
    run(&mut d, D::AdvanceAfter { ms: Some(2000) });
    let t = d.pres.slides[0].transition.clone().unwrap();
    assert_eq!(
        (t.effect, t.advance_after_ms),
        (TransitionEffect::None, Some(2000))
    );
    run(&mut d, D::AdvanceAfter { ms: None });
    // Back to the defaults: no transition at all.
    assert_eq!(d.pres.slides[0].transition, None);
    assert_eq!(
        run(
            &mut d,
            D::SetTransition {
                effect: "none".into()
            }
        ),
        Change::None
    );
}

#[test]
fn apply_to_all_copies_the_transition_and_timing() {
    let mut d = doc();
    run(&mut d, D::NewSlide { layout: None });
    run(&mut d, D::NewSlide { layout: None });
    run(
        &mut d,
        D::SetTransition {
            effect: "wipe".into(),
        },
    );
    run(&mut d, D::TransitionDuration { ms: 2000 });
    run(&mut d, D::ApplyTransitionToAll);
    let all: Vec<_> = d.pres.slides.iter().map(|s| s.transition.clone()).collect();
    assert!(all.iter().all(|t| t == &all[2] && t.is_some()));
    assert!(d.deck_state().slides.iter().all(|s| s.motion));
}

#[test]
fn every_animation_preset_is_added_and_saved() {
    let (mut d, shape) = with_shape();
    for e in kiln_deck::animation::gallery() {
        let id = vocab::effect_id(&e);
        run(&mut d, D::AddAnimation { effect: id.into() });
    }
    let seq = &d.pres.slides[0].timeline.sequence;
    assert_eq!(seq.len(), kiln_deck::animation::gallery().len());
    assert!(
        seq.iter()
            .all(|e| e.target.shape == shape && e.start == EffectStart::OnClick)
    );
    let st = d.motion_state();
    assert_eq!(st.animations.len(), seq.len());
    assert_eq!(st.animations[3].click, 4);
    // The new effect is the one selected.
    assert_eq!(st.selected, vec![seq.last().unwrap().id]);
    let back = reopen(&d);
    // Colour Pulse is written with its colour (accent 1 when none was chosen).
    let plain = |e: &AnimationEffect| match e {
        AnimationEffect::ColorPulse { .. } => AnimationEffect::ColorPulse { color: None },
        other => other.clone(),
    };
    let got: Vec<AnimationEffect> = back.pres.slides[0]
        .timeline
        .sequence
        .iter()
        .map(|e| plain(&e.effect))
        .collect();
    assert_eq!(got, kiln_deck::animation::gallery());
}

#[test]
fn the_gallery_replaces_the_selected_effect_and_none_removes_it() {
    let (mut d, _) = with_shape();
    run(
        &mut d,
        D::SetAnimation {
            effect: "fade".into(),
        },
    );
    run(
        &mut d,
        D::SetAnimation {
            effect: "flyIn".into(),
        },
    );
    let seq = &d.pres.slides[0].timeline.sequence;
    assert_eq!(seq.len(), 1);
    assert!(matches!(seq[0].effect, AnimationEffect::FlyIn { .. }));
    assert_eq!(seq[0].duration_ms, 500);
    run(
        &mut d,
        D::SetAnimation {
            effect: "none".into(),
        },
    );
    assert!(d.pres.slides[0].timeline.sequence.is_empty());
    run(&mut d, D::Escape);
    assert!(
        fails(
            &mut d,
            D::AddAnimation {
                effect: "fade".into()
            }
        )
        .contains("select a shape")
    );
}

#[test]
fn effect_options_and_timing_of_the_selected_effect() {
    let (mut d, _) = with_shape();
    run(
        &mut d,
        D::AddAnimation {
            effect: "flyIn".into(),
        },
    );
    let st = d.motion_state();
    assert_eq!(st.option.as_deref(), Some("fromBottom"));
    assert_eq!(st.options.len(), 8);
    run(
        &mut d,
        D::AnimationOption {
            option: "fromTopLeft".into(),
        },
    );
    run(
        &mut d,
        D::AnimationStart {
            start: DeckEffectStart::AfterPrevious,
        },
    );
    run(&mut d, D::AnimationDuration { ms: 750 });
    run(&mut d, D::AnimationDelay { ms: 250 });
    run(&mut d, D::AnimationRepeat { count: 2 });
    let e = d.pres.slides[0].timeline.sequence[0].clone();
    assert_eq!(
        e.effect,
        AnimationEffect::FlyIn {
            from: kiln_deck::transition::Direction8::LeftUp
        }
    );
    assert_eq!(
        (e.start, e.duration_ms, e.delay_ms, e.repeat),
        (EffectStart::AfterPrevious, 750, 250, 2)
    );
    let a = &d.motion_state().animations[0];
    // After previous with nothing before it: click 0, from 250 to 1750.
    assert_eq!((a.click, a.begin_ms, a.end_ms), (0, 250, 1750));
    assert!(
        fails(
            &mut d,
            D::AnimationOption {
                option: "clockwise".into()
            }
        )
        .contains("clockwise")
    );
    let back = reopen(&d);
    assert_eq!(back.pres.slides[0].timeline.sequence[0].effect, e.effect);
    assert_eq!(back.pres.slides[0].timeline.sequence[0].delay_ms, 250);
}

#[test]
fn the_pane_selects_reorders_and_removes_effects() {
    let (mut d, a) = with_shape();
    run(
        &mut d,
        D::AddAnimation {
            effect: "fade".into(),
        },
    );
    run(
        &mut d,
        D::InsertShape {
            preset: "ellipse".into(),
            rect: Some(DeckRect {
                x: 50.0,
                y: 50.0,
                width: 100.0,
                height: 100.0,
            }),
        },
    );
    let b = d.view.shapes[0];
    run(
        &mut d,
        D::AddAnimation {
            effect: "spin".into(),
        },
    );
    run(
        &mut d,
        D::AddAnimation {
            effect: "pulse".into(),
        },
    );
    let ids: Vec<NodeId> = d.pres.slides[0]
        .timeline
        .sequence
        .iter()
        .map(|e| e.id)
        .collect();
    // Selecting an effect in the pane selects its shape.
    run(&mut d, D::SelectAnimations { ids: vec![ids[0]] });
    assert_eq!(d.view.shapes, vec![a]);
    assert_eq!(d.motion_state().selected, vec![ids[0]]);
    // Move Later twice: it ends last.
    run(&mut d, D::ReorderAnimation { earlier: false });
    run(&mut d, D::ReorderAnimation { earlier: false });
    let now: Vec<NodeId> = d.pres.slides[0]
        .timeline
        .sequence
        .iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(now, vec![ids[1], ids[2], ids[0]]);
    assert_eq!(
        run(&mut d, D::ReorderAnimation { earlier: false }),
        Change::None
    );
    // A drag to the top.
    run(&mut d, D::MoveAnimation { id: ids[2], to: 0 });
    assert_eq!(d.pres.slides[0].timeline.sequence[0].id, ids[2]);
    // Remove what is selected.
    run(
        &mut d,
        D::SelectAnimations {
            ids: vec![ids[1], ids[2]],
        },
    );
    assert_eq!(d.view.shapes, vec![b]);
    run(&mut d, D::RemoveAnimations);
    let left: Vec<NodeId> = d.pres.slides[0]
        .timeline
        .sequence
        .iter()
        .map(|e| e.id)
        .collect();
    assert_eq!(left, vec![ids[0]]);
    // Selecting the shape alone selects its effects.
    run(&mut d, D::SelectShapes { ids: vec![a] });
    assert_eq!(d.selected_effects(), vec![ids[0]]);
}

#[test]
fn the_animation_painter_copies_effects_to_another_shape() {
    let (mut d, a) = with_shape();
    run(
        &mut d,
        D::AddAnimation {
            effect: "wipe".into(),
        },
    );
    run(
        &mut d,
        D::AnimationOption {
            option: "fromLeft".into(),
        },
    );
    run(
        &mut d,
        D::AddAnimation {
            effect: "teeter".into(),
        },
    );
    run(&mut d, D::CopyAnimation);
    assert!(d.motion_state().painter);
    run(
        &mut d,
        D::InsertShape {
            preset: "ellipse".into(),
            rect: Some(DeckRect {
                x: 50.0,
                y: 50.0,
                width: 100.0,
                height: 100.0,
            }),
        },
    );
    let b = d.view.shapes[0];
    run(
        &mut d,
        D::AddAnimation {
            effect: "zoom".into(),
        },
    );
    run(&mut d, D::PasteAnimation);
    let seq = &d.pres.slides[0].timeline.sequence;
    let of = |s: NodeId| {
        seq.iter()
            .filter(|e| e.target.shape == s)
            .map(|e| e.effect.clone())
            .collect::<Vec<_>>()
    };
    assert_eq!(of(a), of(b));
    assert!(matches!(
        of(b)[0],
        AnimationEffect::Wipe { from: Side::Left }
    ));
    // Ids stay unique.
    let mut ids: Vec<NodeId> = seq.iter().map(|e| e.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), seq.len());
    assert!(d.undo());
    run(&mut d, D::Escape);
    assert!(fails(&mut d, D::CopyAnimation).contains("select"));
}

#[test]
fn by_paragraph_splits_a_text_shapes_effect_and_as_one_object_joins_it() {
    let mut d = doc();
    // The title slide's subtitle with three paragraphs.
    let sub = d.pres.slides[0].shapes[1].id;
    run(&mut d, D::SelectShapes { ids: vec![sub] });
    run(&mut d, D::EditText);
    run(&mut d, D::InsertText { text: "One".into() });
    run(&mut d, D::NewParagraph);
    run(&mut d, D::InsertText { text: "Two".into() });
    run(&mut d, D::NewParagraph);
    run(
        &mut d,
        D::InsertText {
            text: "Three".into(),
        },
    );
    run(&mut d, D::Escape);
    run(&mut d, D::SelectShapes { ids: vec![sub] });
    run(
        &mut d,
        D::AddAnimation {
            effect: "fade".into(),
        },
    );
    let st = d.motion_state();
    assert!(st.options.iter().any(|o| o.id == "byParagraph"));
    assert_eq!(st.option.as_deref(), Some("asOneObject"));
    run(
        &mut d,
        D::AnimationOption {
            option: "byParagraph".into(),
        },
    );
    let seq = d.pres.slides[0].timeline.sequence.clone();
    assert_eq!(seq.len(), 3);
    assert_eq!(
        seq.iter().map(|e| e.target.paragraphs).collect::<Vec<_>>(),
        vec![Some((0, 0)), Some((1, 1)), Some((2, 2))]
    );
    assert!(seq.iter().all(|e| e.start == EffectStart::OnClick));
    assert_eq!(d.motion_state().option.as_deref(), Some("byParagraph"));
    run(
        &mut d,
        D::SelectAnimations {
            ids: vec![seq[0].id],
        },
    );
    run(
        &mut d,
        D::AnimationOption {
            option: "allAtOnce".into(),
        },
    );
    let all = d.pres.slides[0].timeline.sequence.clone();
    assert_eq!(
        all.iter().map(|e| e.start).collect::<Vec<_>>(),
        vec![
            EffectStart::OnClick,
            EffectStart::WithPrevious,
            EffectStart::WithPrevious
        ]
    );
    // Saved as `p:txEl/p:pRg` and read back.
    let back = reopen(&d);
    assert_eq!(
        back.pres.slides[0].timeline.sequence[2].target.paragraphs,
        Some((2, 2))
    );
    run(
        &mut d,
        D::SelectAnimations {
            ids: vec![all[0].id],
        },
    );
    run(
        &mut d,
        D::AnimationOption {
            option: "asOneObject".into(),
        },
    );
    let one = &d.pres.slides[0].timeline.sequence;
    assert_eq!(one.len(), 1);
    assert_eq!(one[0].target.paragraphs, None);
}

#[test]
fn badges_number_the_shapes_on_the_stage() {
    let (mut d, _) = with_shape();
    run(
        &mut d,
        D::AddAnimation {
            effect: "appear".into(),
        },
    );
    run(
        &mut d,
        D::AddAnimation {
            effect: "pulse".into(),
        },
    );
    let count = |d: &DeckDoc| d.overlay().len();
    let without = count(&d);
    run(&mut d, D::MotionBadges { on: true });
    assert!(d.motion_state().badges);
    // Two boxes (fill and outline each) and their digits.
    assert!(count(&d) >= without + 4);
}

#[test]
fn kept_effects_are_listed_but_not_offered_options() {
    let (mut d, shape) = with_shape();
    let id = d.pres.alloc_id();
    let e = kiln_deck::animation::Effect::new(
        id,
        shape,
        AnimationEffect::Other {
            class: kiln_deck::animation::EffectClass::Path,
            preset_id: 0,
            subtype: 0,
            xml: None,
        },
    );
    let slide = d.pres.slides[0].id;
    d.apply_with("test", |_| {
        Ok(kiln_deck::ops::Transaction::new(vec![
            kiln_deck::Op::InsertEffect {
                slide,
                index: 0,
                effect: Box::new(e),
            },
        ]))
    })
    .unwrap();
    let st = d.motion_state();
    assert!(!st.animations[0].playable);
    assert_eq!(st.animations[0].class, "path");
    assert!(st.options.is_empty());
    let _ = d.execute(&Command::Suite(SuiteCommand::Undo));
}
