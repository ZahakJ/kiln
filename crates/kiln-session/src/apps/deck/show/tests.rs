// SPDX-License-Identifier: AGPL-3.0-or-later
//! The slide show, played with an injected clock: every transition and every animation preset
//! evaluated through real frames, the keyboard of the show, pointer tools, slide timings,
//! Rehearse Timings, the previews, Set Up Slide Show and its round trip through `.pptx`.

use kiln_common::NodeId;
use kiln_deck::Presentation;
use kiln_deck::animation::EffectClass;
use kiln_protocol::commands::deck_show::{
    DeckPointerPhase, DeckShowFrame, DeckShowMode, DeckShowScreen, DeckShowSettings, DeckShowTool,
    DeckShowType,
};
use kiln_protocol::commands::{DeckCommand as D, DeckQuery, DeckQueryResult, DeckRect};
use kiln_protocol::{Command, FileFormat, Query, QueryResult};

use crate::apps::deck::DeckDoc;
use crate::apps::deck::features::vocab;
use crate::model::{Change, DocModel, ModelNotice};

const W: f32 = 1280.0;
const H: f32 = 720.0;

fn run(d: &mut DeckDoc, c: D) -> Change {
    d.execute(&Command::Deck(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"))
}

/// A deck of `n` slides, each with a rectangle filled in its own colour.
fn deck(n: usize) -> DeckDoc {
    let mut d = DeckDoc::new(Presentation::new(), None, None);
    for _ in 1..n {
        run(&mut d, D::NewSlide { layout: None });
    }
    for i in 0..n {
        run(
            &mut d,
            D::SelectSlide {
                index: u32::try_from(i).unwrap(),
                extend: false,
                toggle: false,
            },
        );
        run(
            &mut d,
            D::InsertShape {
                preset: "rect".into(),
                rect: Some(DeckRect {
                    x: 100.0,
                    y: 100.0,
                    width: 400.0,
                    height: 300.0,
                }),
            },
        );
    }
    run(
        &mut d,
        D::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    );
    d
}

fn start(d: &mut DeckDoc, from: Option<u32>) {
    run(
        d,
        D::ShowStart {
            from,
            mode: DeckShowMode::Show,
            reduced_motion: false,
        },
    );
}

fn frame(d: &mut DeckDoc, now: f64) -> DeckShowFrame {
    let q = Query::Deck(DeckQuery::ShowFrame {
        now_ms: now,
        width: W,
        height: H,
        dpr: 1.0,
    });
    d.prepare_query(&q);
    match d.app_query(&q).expect("a frame") {
        QueryResult::Deck(DeckQueryResult::ShowFrame(f)) => *f,
        other => panic!("{other:?}"),
    }
}

/// The tiles a frame's layers draw, per group, with group opacity and translation.
fn shape_of(f: &DeckShowFrame) -> Vec<(f32, [f32; 2], usize)> {
    f.groups
        .iter()
        .map(|g| (g.opacity, [g.matrix[4], g.matrix[5]], g.layers.len()))
        .collect()
}

/// The slides a frame shows (from its tiles' ids).
fn slides_in(f: &DeckShowFrame) -> Vec<u32> {
    let mut v: Vec<u32> = f
        .groups
        .iter()
        .flat_map(|g| g.layers.iter())
        .filter_map(|l| l.tile.map(|t| t.index / 1024))
        .collect();
    v.dedup();
    v
}

#[test]
fn every_transition_plays_through_its_plates_at_half_way() {
    for e in kiln_deck::transition::GALLERY {
        let id = vocab::transition_id(&e);
        let mut d = deck(2);
        run(
            &mut d,
            D::SelectSlide {
                index: 1,
                extend: false,
                toggle: false,
            },
        );
        run(&mut d, D::SetTransition { effect: id.into() });
        run(&mut d, D::TransitionDuration { ms: 1000 });
        start(&mut d, Some(0));
        let f0 = frame(&mut d, 0.0);
        assert_eq!(slides_in(&f0), vec![0], "{id}");
        run(&mut d, D::ShowNext);
        let at0 = frame(&mut d, 10_000.0);
        let mid = frame(&mut d, 10_500.0);
        let end = frame(&mut d, 11_000.0);
        let shown = slides_in(&mid);
        match e {
            kiln_deck::transition::TransitionEffect::None => {
                assert_eq!(shown, vec![1], "{id}");
                assert!(!at0.animating);
            }
            kiln_deck::transition::TransitionEffect::Flash => {
                // The flash covers the slide; one of the two is under it.
                assert!(
                    mid.groups
                        .iter()
                        .any(|g| g.layers.iter().any(|l| l.fill.is_some())),
                    "{id}"
                );
            }
            kiln_deck::transition::TransitionEffect::Cut { .. } => assert_eq!(shown, vec![1]),
            _ => {
                assert!(shown.contains(&0) && shown.contains(&1), "{id}: {shown:?}");
                assert!(mid.animating, "{id}");
            }
        }
        assert_eq!(slides_in(&end), vec![1], "{id}");
        assert_eq!(shape_of(&end)[0].0, 1.0);
        assert!(!end.animating);
    }
}

#[test]
fn push_moves_both_slides_and_fade_cross_fades() {
    let mut d = deck(2);
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    run(
        &mut d,
        D::SetTransition {
            effect: "push".into(),
        },
    );
    start(&mut d, Some(0));
    frame(&mut d, 0.0);
    run(&mut d, D::ShowNext);
    frame(&mut d, 100.0);
    let mid = frame(&mut d, 600.0);
    // From Bottom: the old slide half way up, the new one half way in from below.
    assert_eq!(
        shape_of(&mid).iter().map(|g| g.1[1]).collect::<Vec<_>>(),
        vec![-360.0, 360.0]
    );
    run(
        &mut d,
        D::SetTransition {
            effect: "fade".into(),
        },
    );
    run(&mut d, D::ShowFirst);
    frame(&mut d, 2000.0);
    run(&mut d, D::ShowNext);
    frame(&mut d, 3000.0);
    let half = frame(&mut d, 3350.0);
    let g = shape_of(&half);
    assert_eq!(g.len(), 2);
    assert!((g[1].0 - 0.5).abs() < 1e-3);
}

#[test]
fn the_first_slides_transition_comes_from_black() {
    let mut d = deck(1);
    run(
        &mut d,
        D::SetTransition {
            effect: "fade".into(),
        },
    );
    start(&mut d, None);
    frame(&mut d, 0.0);
    let f = frame(&mut d, 350.0);
    assert!(f.groups[0].layers[0].fill.is_some());
    assert!((f.groups[1].opacity - 0.5).abs() < 1e-3);
}

#[test]
fn every_animation_preset_runs_on_its_click() {
    for e in kiln_deck::animation::gallery() {
        let id = vocab::effect_id(&e);
        let mut d = deck(1);
        let shape = d.pres.slides[0].shapes.last().unwrap().id;
        run(&mut d, D::SelectShapes { ids: vec![shape] });
        run(&mut d, D::AddAnimation { effect: id.into() });
        run(&mut d, D::AnimationDuration { ms: 1000 });
        start(&mut d, None);
        let before = frame(&mut d, 0.0);
        let layers = |f: &DeckShowFrame| f.groups[0].layers.clone();
        // The animated shape has a tile of its own after the slide's static run.
        let entrance = e.class() == EffectClass::Entrance;
        assert_eq!(
            layers(&before).len(),
            if entrance { 1 } else { 2 },
            "{id} before"
        );
        run(&mut d, D::ShowNext);
        frame(&mut d, 1000.0);
        let mid = frame(&mut d, 1500.0);
        assert!(mid.animating, "{id}");
        let moving = layers(&mid);
        assert!(
            moving.len() >= 2 || e == kiln_deck::animation::AnimationEffect::Disappear,
            "{id} at 50 %"
        );
        let l = moving.get(1);
        let posed = l.is_some_and(|l| {
            l.matrix != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] || l.opacity < 1.0 || l.clip.is_some()
        });
        match e {
            kiln_deck::animation::AnimationEffect::Appear => assert!(!posed),
            kiln_deck::animation::AnimationEffect::Disappear => assert_eq!(moving.len(), 1),
            kiln_deck::animation::AnimationEffect::ColorPulse { .. } => {
                // The tinted copy over the shape at full strength half way.
                assert!(
                    moving.len() == 3 && (moving[2].opacity - 1.0).abs() < 1e-3,
                    "{id}"
                );
            }
            _ => assert!(posed, "{id}: {l:?}"),
        }
        let after = frame(&mut d, 2100.0);
        assert!(!after.animating, "{id}");
        let n = layers(&after).len();
        assert_eq!(
            n,
            if e.class() == EffectClass::Exit { 1 } else { 2 },
            "{id} after"
        );
    }
}

#[test]
fn by_paragraph_builds_show_each_paragraph_on_its_click() {
    let mut d = DeckDoc::new(Presentation::new(), None, None);
    let sub = d.pres.slides[0].shapes[1].id;
    run(&mut d, D::SelectShapes { ids: vec![sub] });
    run(&mut d, D::EditText);
    run(&mut d, D::InsertText { text: "One".into() });
    run(&mut d, D::NewParagraph);
    run(&mut d, D::InsertText { text: "Two".into() });
    run(&mut d, D::Escape);
    run(&mut d, D::SelectShapes { ids: vec![sub] });
    run(
        &mut d,
        D::AddAnimation {
            effect: "wipe".into(),
        },
    );
    run(
        &mut d,
        D::AnimationOption {
            option: "byParagraph".into(),
        },
    );
    start(&mut d, None);
    let f = frame(&mut d, 0.0);
    let clips = |f: &DeckShowFrame| {
        f.groups[0]
            .layers
            .iter()
            .filter(|l| l.clip.is_some())
            .count()
    };
    // The shape's rest; both paragraphs hidden.
    assert_eq!(clips(&f), 1);
    run(&mut d, D::ShowNext);
    frame(&mut d, 1000.0);
    let one = frame(&mut d, 5000.0);
    assert_eq!(clips(&one), 2);
    run(&mut d, D::ShowNext);
    frame(&mut d, 6000.0);
    let two = frame(&mut d, 9000.0);
    assert_eq!(clips(&two), 3);
}

#[test]
fn the_keyboard_steps_through_clicks_slides_and_the_end() {
    let mut d = deck(3);
    let shape = d.pres.slides[1].shapes.last().unwrap().id;
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    run(&mut d, D::SelectShapes { ids: vec![shape] });
    run(
        &mut d,
        D::AddAnimation {
            effect: "fade".into(),
        },
    );
    start(&mut d, None);
    let st = |d: &DeckDoc| d.show_status();
    assert_eq!((st(&d).slide, st(&d).count, st(&d).position), (0, 3, 1));
    run(&mut d, D::ShowNext);
    assert_eq!((st(&d).slide, st(&d).click, st(&d).clicks), (1, 0, 1));
    run(&mut d, D::ShowNext);
    assert_eq!((st(&d).slide, st(&d).click), (1, 1));
    // Back a click, then back a slide; forward again.
    run(&mut d, D::ShowPrevious);
    assert_eq!((st(&d).slide, st(&d).click), (1, 0));
    run(&mut d, D::ShowPrevious);
    assert_eq!(st(&d).slide, 0);
    run(&mut d, D::ShowLast);
    assert_eq!(st(&d).slide, 2);
    run(&mut d, D::ShowNext);
    assert_eq!(st(&d).screen, DeckShowScreen::End);
    let end = frame(&mut d, 100.0);
    assert!(end.groups[0].layers[0].fill.is_some());
    run(&mut d, D::ShowPrevious);
    assert_eq!(st(&d).screen, DeckShowScreen::Slide);
    // Number, then Enter; Home.
    run(&mut d, D::ShowDigit { digit: 2 });
    assert_eq!(st(&d).typed, "2");
    run(&mut d, D::ShowGoToTyped);
    assert_eq!((st(&d).slide, st(&d).typed.as_str()), (1, ""));
    run(&mut d, D::ShowFirst);
    assert_eq!(st(&d).slide, 0);
    // Enter with nothing typed is Next.
    run(&mut d, D::ShowGoToTyped);
    assert_eq!(st(&d).slide, 1);
    // B and W.
    run(&mut d, D::ShowBlack);
    assert_eq!(st(&d).screen, DeckShowScreen::Black);
    run(&mut d, D::ShowWhite);
    assert_eq!(st(&d).screen, DeckShowScreen::White);
    let white = frame(&mut d, 200.0);
    assert!(matches!(
        white.groups[0].layers[0].fill,
        Some(kiln_scene::Paint::Rgba(0xffff_ffff))
    ));
    run(&mut d, D::ShowWhite);
    assert_eq!(st(&d).screen, DeckShowScreen::Slide);
    // Black then any advance only lifts the black.
    run(&mut d, D::ShowBlack);
    run(&mut d, D::ShowNext);
    assert_eq!(
        (st(&d).screen, st(&d).slide, st(&d).click),
        (DeckShowScreen::Slide, 1, 0)
    );
    // Esc ends the show and the stage is on the slide it ended on.
    run(&mut d, D::ShowEnd);
    assert!(!st(&d).running);
    assert!(d.show.is_none());
    assert_eq!(d.view.current, 1);
}

#[test]
fn hidden_slides_are_skipped_but_can_be_gone_to() {
    let mut d = deck(3);
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    run(&mut d, D::HideSlides { hidden: Some(true) });
    start(&mut d, None);
    assert_eq!(d.show_status().count, 2);
    run(&mut d, D::ShowNext);
    assert_eq!(d.show_status().slide, 2);
    run(&mut d, D::ShowDigit { digit: 2 });
    run(&mut d, D::ShowGoToTyped);
    assert_eq!(d.show_status().slide, 1);
    assert_eq!(d.show_status().next, Some(2));
}

#[test]
fn pointer_tools_draw_erase_and_point() {
    let mut d = deck(2);
    start(&mut d, None);
    let ptr = |d: &mut DeckDoc, phase, x, y| {
        run(
            d,
            D::ShowPointer {
                phase,
                x,
                y,
                touch: false,
            },
        );
    };
    run(&mut d, D::ShowPen);
    assert_eq!(d.show_status().tool, DeckShowTool::Pen);
    ptr(&mut d, DeckPointerPhase::Down, 100.0, 100.0);
    ptr(&mut d, DeckPointerPhase::Move, 200.0, 120.0);
    ptr(&mut d, DeckPointerPhase::Move, 300.0, 150.0);
    ptr(&mut d, DeckPointerPhase::Up, 300.0, 150.0);
    assert_eq!(frame(&mut d, 0.0).ink.len(), 1);
    // The pen does not advance.
    assert_eq!(d.show_status().slide, 0);
    run(&mut d, D::ShowHighlighter);
    ptr(&mut d, DeckPointerPhase::Down, 500.0, 500.0);
    ptr(&mut d, DeckPointerPhase::Move, 600.0, 500.0);
    ptr(&mut d, DeckPointerPhase::Up, 600.0, 500.0);
    assert_eq!(frame(&mut d, 10.0).ink.len(), 2);
    run(&mut d, D::ShowEraser);
    ptr(&mut d, DeckPointerPhase::Down, 200.0, 121.0);
    ptr(&mut d, DeckPointerPhase::Up, 200.0, 121.0);
    assert_eq!(frame(&mut d, 20.0).ink.len(), 1);
    run(&mut d, D::ShowErase);
    assert!(frame(&mut d, 30.0).ink.is_empty());
    run(&mut d, D::ShowLaser);
    ptr(&mut d, DeckPointerPhase::Move, 640.0, 360.0);
    assert_eq!(frame(&mut d, 40.0).ink.len(), 4);
    ptr(&mut d, DeckPointerPhase::Leave, 0.0, 0.0);
    assert!(frame(&mut d, 50.0).ink.is_empty());
    // Ink stays with its slide.
    run(&mut d, D::ShowPen);
    ptr(&mut d, DeckPointerPhase::Down, 10.0, 10.0);
    ptr(&mut d, DeckPointerPhase::Up, 10.0, 10.0);
    run(&mut d, D::ShowArrow);
    assert_eq!(d.show_status().tool, DeckShowTool::Arrow);
    // With the arrow a click advances; on a touch screen the left third goes back.
    ptr(&mut d, DeckPointerPhase::Down, 900.0, 300.0);
    ptr(&mut d, DeckPointerPhase::Up, 900.0, 300.0);
    assert_eq!(d.show_status().slide, 1);
    assert!(frame(&mut d, 60.0).ink.is_empty());
    run(
        &mut d,
        D::ShowPointer {
            phase: DeckPointerPhase::Down,
            x: 100.0,
            y: 300.0,
            touch: true,
        },
    );
    run(
        &mut d,
        D::ShowPointer {
            phase: DeckPointerPhase::Up,
            x: 100.0,
            y: 300.0,
            touch: true,
        },
    );
    assert_eq!(d.show_status().slide, 0);
    assert_eq!(frame(&mut d, 70.0).ink.len(), 1);
}

#[test]
fn clicks_do_not_advance_when_the_slide_says_so() {
    let mut d = deck(2);
    run(&mut d, D::AdvanceOnClick { on: Some(false) });
    start(&mut d, None);
    assert!(!d.show_status().on_click);
    run(
        &mut d,
        D::ShowPointer {
            phase: DeckPointerPhase::Down,
            x: 900.0,
            y: 300.0,
            touch: false,
        },
    );
    run(
        &mut d,
        D::ShowPointer {
            phase: DeckPointerPhase::Up,
            x: 900.0,
            y: 300.0,
            touch: false,
        },
    );
    assert_eq!(d.show_status().slide, 0);
    // The keyboard still does.
    run(&mut d, D::ShowNext);
    assert_eq!(d.show_status().slide, 1);
}

#[test]
fn taps_do_not_step_a_kiosk_show() {
    let mut d = deck(3);
    let kiosk = DeckShowSettings {
        kind: DeckShowType::Kiosk,
        loop_show: true,
        ..DeckShowSettings::default()
    };
    run(&mut d, D::SetShowSettings { settings: kiosk });
    start(&mut d, Some(1));
    assert_eq!(d.show_status().slide, 1);
    for x in [900.0, 100.0] {
        for touch in [true, false] {
            for phase in [DeckPointerPhase::Down, DeckPointerPhase::Up] {
                run(
                    &mut d,
                    D::ShowPointer {
                        phase,
                        x,
                        y: 300.0,
                        touch,
                    },
                );
            }
            assert_eq!(d.show_status().slide, 1, "x {x}, touch {touch}");
        }
    }
}

#[test]
fn presenter_view_alone_does_not_change_the_file() {
    let mut d = deck(2);
    let state = d.state();
    let on = DeckShowSettings {
        presenter_view: true,
        ..DeckShowSettings::default()
    };
    assert_eq!(
        run(&mut d, D::SetShowSettings { settings: on }),
        Change::Selection
    );
    assert!(d.motion_state().settings.presenter_view);
    assert_eq!(d.state(), state);
    let bytes = d.write(FileFormat::Pptx, None).unwrap();
    let read = kiln_io_pptx::read_pptx(&bytes).unwrap();
    assert!(super::settings::part(&read.passthrough).is_none());
    // Saved settings still count, with Presenter View on or off.
    let looped = DeckShowSettings {
        loop_show: true,
        presenter_view: true,
        ..DeckShowSettings::default()
    };
    assert_eq!(
        run(&mut d, D::SetShowSettings { settings: looped }),
        Change::Content
    );
    assert_ne!(d.state(), state);
}

#[test]
fn slide_timings_advance_the_show_by_themselves() {
    let mut d = deck(3);
    run(
        &mut d,
        D::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    );
    run(&mut d, D::AdvanceAfter { ms: Some(2000) });
    start(&mut d, None);
    let f = frame(&mut d, 0.0);
    assert_eq!(f.advance_in_ms, Some(2000));
    assert!(f.animating);
    frame(&mut d, 1999.0);
    assert_eq!(d.show_status().slide, 0);
    frame(&mut d, 2000.0);
    assert_eq!(d.show_status().slide, 1);
    // Advance slides manually ignores them.
    let mut m = deck(2);
    run(&mut m, D::AdvanceAfter { ms: Some(10) });
    let settings = DeckShowSettings {
        manual: true,
        ..DeckShowSettings::default()
    };
    run(&mut m, D::SetShowSettings { settings });
    start(&mut m, None);
    frame(&mut m, 0.0);
    frame(&mut m, 5000.0);
    assert_eq!(m.show_status().slide, 0);
}

#[test]
fn loop_and_a_range_of_slides_from_set_up_show() {
    let mut d = deck(4);
    let settings = DeckShowSettings {
        loop_show: true,
        from: Some(2),
        to: Some(3),
        ..DeckShowSettings::default()
    };
    run(&mut d, D::SetShowSettings { settings });
    start(&mut d, None);
    assert_eq!((d.show_status().slide, d.show_status().count), (1, 2));
    run(&mut d, D::ShowNext);
    run(&mut d, D::ShowNext);
    // Loops back to the first slide of the range.
    assert_eq!(d.show_status().slide, 1);
    run(&mut d, D::ShowGoTo { index: 3 });
    assert_eq!(d.show_status().slide, 1);
}

#[test]
fn set_up_show_is_saved_in_pres_props_and_read_back() {
    let mut d = deck(3);
    let settings = DeckShowSettings {
        kind: DeckShowType::Kiosk,
        loop_show: true,
        no_animation: true,
        from: Some(2),
        to: Some(3),
        ..DeckShowSettings::default()
    };
    assert_eq!(
        run(
            &mut d,
            D::SetShowSettings {
                settings: settings.clone()
            }
        ),
        Change::Content
    );
    assert_eq!(d.motion_state().settings, settings);
    let bytes = d.write(FileFormat::Pptx, None).unwrap();
    let read = kiln_io_pptx::read_pptx(&bytes).unwrap();
    let (pres, original) = crate::apps::deck::save::Original::from_read(read, &bytes);
    let mut back = DeckDoc::new(pres, Some(original), None);
    assert_eq!(back.show_settings, settings);
    // And again from a file that had a `presProps.xml` already.
    let w = DeckShowSettings {
        kind: DeckShowType::Window,
        ..DeckShowSettings::default()
    };
    run(
        &mut back,
        D::SetShowSettings {
            settings: w.clone(),
        },
    );
    let bytes2 = back.write(FileFormat::Pptx, None).unwrap();
    let read2 = kiln_io_pptx::read_pptx(&bytes2).unwrap();
    let part = super::settings::part(&read2.passthrough).expect("presProps.xml");
    assert_eq!(super::settings::read(&part.bytes), w);
    // One `presProps` relationship only.
    let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes2).unwrap();
    let rels = pkg.relationships("/ppt/presentation.xml").unwrap();
    assert_eq!(
        rels.all()
            .iter()
            .filter(|r| r.rel_type == super::settings::REL_TYPE)
            .count(),
        1
    );
}

#[test]
fn rehearse_timings_records_each_slide_and_keeps_them() {
    let mut d = deck(2);
    run(
        &mut d,
        D::ShowStart {
            from: None,
            mode: DeckShowMode::Rehearse,
            reduced_motion: false,
        },
    );
    frame(&mut d, 0.0);
    frame(&mut d, 3000.0);
    run(&mut d, D::ShowNext);
    frame(&mut d, 3000.0);
    frame(&mut d, 4500.0);
    assert_eq!(d.show_info().rehearsed[0], 3000);
    run(&mut d, D::ShowEnd);
    run(&mut d, D::ShowKeepTimings);
    let after: Vec<Option<u32>> = d
        .pres
        .slides
        .iter()
        .map(|s| s.transition.as_ref().and_then(|t| t.advance_after_ms))
        .collect();
    assert_eq!(after, vec![Some(3000), Some(1500)]);
}

#[test]
fn previews_play_once_and_stop() {
    let mut d = deck(2);
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    run(
        &mut d,
        D::SetTransition {
            effect: "wipe".into(),
        },
    );
    let shape = d.pres.slides[1].shapes.last().unwrap().id;
    run(&mut d, D::SelectShapes { ids: vec![shape] });
    run(
        &mut d,
        D::AddAnimation {
            effect: "fade".into(),
        },
    );
    run(
        &mut d,
        D::AddAnimation {
            effect: "spin".into(),
        },
    );
    run(
        &mut d,
        D::ShowStart {
            from: Some(1),
            mode: DeckShowMode::PreviewTransition,
            reduced_motion: false,
        },
    );
    assert_eq!(slides_in(&frame(&mut d, 0.0)), vec![0]);
    // From the previous slide.
    assert_eq!(slides_in(&frame(&mut d, 500.0)), vec![0, 1]);
    assert!(d.show_status().running);
    assert!(!frame(&mut d, 1001.0).status.running);
    assert!(d.show.is_none());
    assert_eq!(d.view.current, 1);
    run(
        &mut d,
        D::ShowStart {
            from: Some(1),
            mode: DeckShowMode::PreviewAnimations,
            reduced_motion: false,
        },
    );
    frame(&mut d, 0.0);
    assert_eq!(d.show_status().click, 1);
    frame(&mut d, 600.0);
    assert_eq!(d.show_status().click, 2);
    assert!(!frame(&mut d, 3000.0).status.running);
    assert!(d.show.is_none());
}

#[test]
fn tiles_go_once_and_frames_carry_only_the_composition() {
    let mut d = deck(3);
    start(&mut d, None);
    let first = frame(&mut d, 0.0);
    assert!(first.delta.reset);
    assert!(!first.delta.tiles.is_empty());
    assert!(!first.delta.atlas.is_empty() || first.delta.tiles.len() >= 2);
    let again = frame(&mut d, 16.0);
    assert!(again.delta.tiles.is_empty() && !again.delta.reset);
    // The next slide was sent ahead: advancing needs no new tiles for it.
    run(&mut d, D::ShowNext);
    let next = frame(&mut d, 32.0);
    assert!(next.delta.tiles.iter().all(|t| t.id.index / 1024 == 2));
    // A new size starts over.
    let q = Query::Deck(DeckQuery::ShowFrame {
        now_ms: 48.0,
        width: 640.0,
        height: 360.0,
        dpr: 1.0,
    });
    d.prepare_query(&q);
    let QueryResult::Deck(DeckQueryResult::ShowFrame(f)) = d.app_query(&q).unwrap() else {
        panic!()
    };
    assert!(f.delta.reset);
    assert!((f.delta.scale - 0.5).abs() < 1e-4);
}

#[test]
fn windows_hear_of_every_step_once() {
    let mut d = deck(2);
    start(&mut d, None);
    let n = |d: &mut DeckDoc| {
        d.take_notices()
            .into_iter()
            .filter(|x| matches!(x, ModelNotice::Show(_)))
            .count()
    };
    assert_eq!(n(&mut d), 1);
    frame(&mut d, 0.0);
    frame(&mut d, 16.0);
    assert_eq!(n(&mut d), 0);
    run(&mut d, D::ShowNext);
    assert_eq!(n(&mut d), 1);
    let info = d.show_info();
    assert_eq!(info.status.slide, 1);
    assert_eq!(info.slides, vec![0, 1]);
}

#[test]
fn the_show_needs_slides_and_a_running_show() {
    let mut d = DeckDoc::new(Presentation::new(), None, None);
    let err = d.execute(&Command::Deck(D::ShowNext)).unwrap_err();
    assert!(err.to_string().contains("no slide show"));
    run(&mut d, D::DeleteSlides);
    assert!(d.pres.slides.is_empty());
    let err = d
        .execute(&Command::Deck(D::ShowStart {
            from: None,
            mode: DeckShowMode::Show,
            reduced_motion: false,
        }))
        .unwrap_err();
    assert!(err.to_string().contains("no slides"));
    let _: Option<NodeId> = None;
}

#[test]
fn a_snapshot_is_the_presentation_as_pptx() {
    let d = deck(2);
    let QueryResult::Deck(DeckQueryResult::Bytes(b)) =
        d.app_query(&Query::Deck(DeckQuery::Snapshot)).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        kiln_io_pptx::read_pptx(&b)
            .unwrap()
            .presentation
            .slides
            .len(),
        2
    );
}

#[test]
fn reduced_motion_plays_transitions_as_fades_and_effects_at_once() {
    let mut d = deck(2);
    run(
        &mut d,
        D::SelectSlide {
            index: 1,
            extend: false,
            toggle: false,
        },
    );
    run(
        &mut d,
        D::SetTransition {
            effect: "push".into(),
        },
    );
    let shape = d.pres.slides[1].shapes.last().unwrap().id;
    run(&mut d, D::SelectShapes { ids: vec![shape] });
    run(
        &mut d,
        D::AddAnimation {
            effect: "flyIn".into(),
        },
    );
    run(&mut d, D::AnimationDuration { ms: 1000 });
    run(
        &mut d,
        D::ShowStart {
            from: None,
            mode: DeckShowMode::Show,
            reduced_motion: true,
        },
    );
    frame(&mut d, 0.0);
    run(&mut d, D::ShowNext);
    frame(&mut d, 1000.0);
    // Half way through the push: both slides in place, the incoming one half faded in.
    let ms = f64::from(d.pres.slides[1].transition.as_ref().unwrap().duration_ms);
    let mid = frame(&mut d, 1000.0 + ms / 2.0);
    assert!(mid.groups.len() >= 2, "two plates");
    for g in &mid.groups {
        assert!(
            g.matrix[4].abs() < 1e-3 && g.matrix[5].abs() < 1e-3,
            "no movement"
        );
    }
    assert!(
        mid.groups
            .iter()
            .any(|g| g.opacity > 0.05 && g.opacity < 0.95),
        "a fade"
    );
    // The fly-in, half way through its time, is already where it ends.
    frame(&mut d, 5000.0);
    run(&mut d, D::ShowNext);
    frame(&mut d, 6000.0);
    let half = frame(&mut d, 6500.0);
    let l = &half.groups[0].layers;
    assert!(l.len() >= 2);
    assert_eq!(
        l[1].matrix,
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        "in place at once"
    );
    assert!((l[1].opacity - 1.0).abs() < 1e-3);
}
