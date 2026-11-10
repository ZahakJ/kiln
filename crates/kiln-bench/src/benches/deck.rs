// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Deck benchmarks, on `fixtures/large/deck-200.pptx` (200 slides: bullets, two
//! columns, pictures, tables, shapes, charts; generated on demand by `tools/make-fixtures.py
//! --synthetic-large --only pptx`, which the benchmark runs when the file is missing):
//!
//! - `deck-open-200` (B10): open the presentation in a session and produce the first frame of
//!   the stage — the first slide zoomed to fit a 1440 × 900 view at device pixel ratio 2 — the
//!   time until the slide shows. Extra figures: `frame_ms` (that frame alone) and `rail_ms` (the
//!   first eight thumbnails of the rail, asked for after the stage).
//! - `deck-frame`: one frame after going to another slide (the slide drawn, its tiles built and
//!   its glyphs rasterised), at device pixel ratio 2; the extra `drag_ms` is one frame while a
//!   shape is dragged (the preview, the smart guides and the slide's tiles again).
//!
//! In quick mode they use `fixtures/pptx/features/placeholders.pptx` instead.

use std::path::{Path, PathBuf};
use std::time::Instant;

use kiln_protocol::commands::{DeckCommand, DeckQuery, DeckQueryResult};
use kiln_protocol::geom::Point;
use kiln_protocol::{Command, FileFormat, OpenHint, Query, QueryResult, ViewMode, Viewport};
use kiln_session::{DeferredHost, Session};

use crate::harness::{Config, Sample, measure, percentile, summarize};
use crate::scenarios::RunOptions;
use crate::synth::Lcg;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn input(options: &RunOptions) -> Result<Vec<u8>, String> {
    if options.quick {
        let p = repo().join("fixtures/pptx/features/placeholders.pptx");
        return std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()));
    }
    let p = repo().join("fixtures/large/deck-200.pptx");
    if !p.exists() {
        let status = std::process::Command::new("python3")
            .args([
                "-I",
                "tools/make-fixtures.py",
                "--synthetic-large",
                "--only",
                "pptx",
            ])
            .current_dir(repo())
            .status()
            .map_err(|e| format!("python3 tools/make-fixtures.py: {e}"))?;
        if !status.success() {
            return Err(format!("tools/make-fixtures.py failed ({status})"));
        }
    }
    std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))
}

/// The stage of a 1440 × 900 window: the slide (1280 × 720 px) zoomed to fit with a margin and
/// centred, as the web editor shows it.
fn viewport() -> Viewport {
    let zoom = ((1440.0 - 56.0) / 1280.0_f32).min((900.0 - 56.0) / 720.0);
    Viewport {
        width_px: 1440.0,
        height_px: 900.0,
        scroll: Point::new((1280.0 - 1440.0 / zoom) / 2.0, (720.0 - 900.0 / zoom) / 2.0),
        zoom,
        dpr: 2.0,
        mode: ViewMode::SlideNormal,
    }
}

fn open(session: &mut Session, bytes: &[u8]) -> Result<kiln_common::DocId, String> {
    session
        .open(
            bytes,
            OpenHint {
                name: Some("deck-200.pptx".into()),
                format: Some(FileFormat::Pptx),
                ..OpenHint::default()
            },
        )
        .map(|s| s.doc)
        .map_err(|e| e.to_string())
}

fn slide_count(session: &Session, doc: kiln_common::DocId) -> usize {
    match session.query(Some(doc), &Query::Deck(DeckQuery::State)) {
        Ok(QueryResult::Deck(DeckQueryResult::State(s))) => s.slides.len(),
        _ => 0,
    }
}

/// `deck-open-200`.
pub fn open_200(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let config = if options.quick {
        Config::new(0, 2)
    } else {
        Config::new(1, 6)
    };
    let (mut times, mut frames, mut rails) = (Vec::new(), Vec::new(), Vec::new());
    let mut slides = 0;
    for run in 0..config.warmup + config.iterations {
        let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
        let t0 = Instant::now();
        let doc = open(&mut session, &bytes)?;
        session
            .set_viewport(doc, viewport())
            .map_err(|e| e.to_string())?;
        let t1 = Instant::now();
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        let t2 = Instant::now();
        if d.tiles.is_empty() {
            return Err("the first frame has no tiles".into());
        }
        let rail = session
            .query(
                Some(doc),
                &Query::Deck(DeckQuery::Thumbnails {
                    from: 0,
                    count: 8,
                    width: 168.0,
                    dpr: 2.0,
                }),
            )
            .map_err(|e| e.to_string())?;
        let t3 = Instant::now();
        if !matches!(rail, QueryResult::Deck(DeckQueryResult::Scene(_))) {
            return Err("the rail has no thumbnails".into());
        }
        slides = slide_count(&session, doc);
        drop(session);
        if run >= config.warmup {
            times.push(t2 - t0);
            frames.push((t2 - t1).as_secs_f64() * 1000.0);
            rails.push((t3 - t2).as_secs_f64() * 1000.0);
        }
    }
    if !options.quick && slides < 200 {
        return Err(format!("deck-200.pptx read as {slides} slides"));
    }
    let mut sample = summarize("deck-open-200", &times, None);
    frames.sort_by(f64::total_cmp);
    rails.sort_by(f64::total_cmp);
    sample
        .extra
        .insert("frame_ms".into(), percentile(&frames, 95.0));
    sample
        .extra
        .insert("rail_ms".into(), percentile(&rails, 95.0));
    #[allow(clippy::cast_precision_loss)]
    sample.extra.insert("slides".into(), slides as f64);
    Ok(sample)
}

/// `deck-frame`.
pub fn frame(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = open(&mut session, &bytes)?;
    session
        .set_viewport(doc, viewport())
        .map_err(|e| e.to_string())?;
    session.frame(doc).map_err(|e| e.to_string())?;
    let count = slide_count(&session, doc).max(1);
    let config = if options.quick {
        Config::new(0, 3)
    } else {
        Config::new(10, 200)
    };
    let mut rng = Lcg::new(29);
    let mut tiles = 0usize;
    let run = |session: &mut Session, c: DeckCommand| {
        session
            .execute(Some(doc), Command::Deck(c))
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    let mut sample = measure("deck-frame", config, || {
        let index = u32::try_from(rng.below(count)).unwrap_or(0);
        run(
            &mut session,
            DeckCommand::SelectSlide {
                index,
                extend: false,
                toggle: false,
            },
        )?;
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        tiles += d.tiles.len();
        Ok(())
    })?;
    if tiles == 0 && count > 1 {
        return Err("going to other slides produced no tiles".into());
    }
    // A drag: a shape inserted in the middle of the slide, pressed and moved a few px per frame.
    run(
        &mut session,
        DeckCommand::SelectSlide {
            index: 0,
            extend: false,
            toggle: false,
        },
    )?;
    run(
        &mut session,
        DeckCommand::InsertShape {
            preset: "roundRect".into(),
            rect: None,
        },
    )?;
    let (mut x, y) = (640.0_f32, 360.0_f32);
    run(
        &mut session,
        DeckCommand::PointerDown {
            x,
            y,
            shift: false,
            ctrl: false,
            alt: false,
            clicks: 1,
        },
    )?;
    let mut drags = Vec::new();
    let mut dragged = 0usize;
    let steps = if options.quick { 3 } else { 120 };
    for _ in 0..steps {
        x += 3.0;
        let t = Instant::now();
        run(
            &mut session,
            DeckCommand::PointerMove {
                x,
                y,
                shift: false,
                ctrl: false,
                alt: false,
            },
        )?;
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        dragged += d.tiles.len() + usize::from(d.overlay.is_some());
        drags.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    run(
        &mut session,
        DeckCommand::PointerUp {
            x,
            y,
            shift: false,
            ctrl: false,
            alt: false,
        },
    )?;
    if dragged == 0 {
        return Err("the drag repainted nothing".into());
    }
    drags.sort_by(f64::total_cmp);
    sample
        .extra
        .insert("drag_ms".into(), percentile(&drags, 95.0));
    Ok(sample)
}

/// One show frame at `now` for a 1920 × 1080 screen at device pixel ratio 1.
fn show_frame(
    session: &mut Session,
    doc: kiln_common::DocId,
    now: f64,
) -> Result<kiln_protocol::commands::deck_show::DeckShowFrame, String> {
    let q = Query::Deck(DeckQuery::ShowFrame {
        now_ms: now,
        width: 1920.0,
        height: 1080.0,
        dpr: 1.0,
    });
    match session
        .query_mut(Some(doc), &q)
        .map_err(|e| e.to_string())?
    {
        QueryResult::Deck(DeckQueryResult::ShowFrame(f)) => Ok(*f),
        _ => Err("no show frame".into()),
    }
}

/// `deck-show-frame`: the slide show of the 200-slide deck with a Push on every slide, on
/// a 1920 × 1080 screen. The sample is a frame half way through a transition (the composition of
/// the two slides' tiles, sent before); `step_ms` is the first frame after Next (the next slide's
/// tiles drawn and sent ahead of it).
pub fn show_frame_bench(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = open(&mut session, &bytes)?;
    let run = |session: &mut Session, c: DeckCommand| {
        session
            .execute(Some(doc), Command::Deck(c))
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    run(
        &mut session,
        DeckCommand::SetTransition {
            effect: "push".into(),
        },
    )?;
    run(&mut session, DeckCommand::ApplyTransitionToAll)?;
    run(
        &mut session,
        DeckCommand::ShowStart {
            from: None,
            mode: kiln_protocol::commands::deck_show::DeckShowMode::Show,
            reduced_motion: false,
        },
    )?;
    let count = slide_count(&session, doc).max(2);
    let config = if options.quick {
        Config::new(0, 3)
    } else {
        Config::new(5, 120)
    };
    let mut now = 0.0_f64;
    show_frame(&mut session, doc, now)?;
    let mut steps = Vec::new();
    let mut mids = Vec::new();
    let mut shown = 0usize;
    for k in 0..config.warmup + config.iterations {
        // Next slide (back to the first after the last), its first frame, then half way.
        if shown + 1 >= count {
            run(&mut session, DeckCommand::ShowFirst)?;
            shown = 0;
        }
        run(&mut session, DeckCommand::ShowNext)?;
        shown += 1;
        now += 10_000.0;
        let t = Instant::now();
        show_frame(&mut session, doc, now)?;
        let step = t.elapsed();
        let t = Instant::now();
        let mid = show_frame(&mut session, doc, now + 500.0)?;
        let half = t.elapsed();
        if mid.groups.len() < 2 {
            return Err("half way through Push shows one slide".into());
        }
        if k >= config.warmup {
            steps.push(step.as_secs_f64() * 1000.0);
            mids.push(half);
        }
    }
    let mut sample = summarize("deck-show-frame", &mids, None);
    steps.sort_by(f64::total_cmp);
    sample
        .extra
        .insert("step_ms".into(), percentile(&steps, 95.0));
    Ok(sample)
}
