// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Grid surface benchmarks, on `fixtures/large/sheet-1m.xlsx` (1,000,000 populated
//! cells, 200,000 of them formulas; generated on demand like `xlsx-open`):
//!
//! - `grid-open` (B7): open the workbook in a session (read, bind and calculate) and produce the
//!   first frame of a 1440 × 900 view at device pixel ratio 2 — the time until the grid shows
//!   the file. The extra `frame_ms` is the first frame alone.
//! - `grid-frame` (B6, core side): one frame after jumping to a random place in the sheet (new
//!   tiles laid out and painted, glyphs rasterised), at device pixel ratio 2.
//! - `grid-scroll` (B6, core side): one frame per animation frame while scrolling down at
//!   2,000 px/s (the tiles prefetched one screen ahead; most frames send nothing).
//!
//! In quick mode they use `fixtures/xlsx/features/styles.xlsx` instead.

use std::path::{Path, PathBuf};
use std::time::Instant;

use kiln_protocol::commands::{GridCommand, GridQuery, GridQueryResult};
use kiln_protocol::geom::Point;
use kiln_protocol::{FileFormat, OpenHint, ViewMode, Viewport};
use kiln_session::{DeferredHost, Session};

use crate::harness::{Config, Sample, measure, percentile, summarize};
use crate::scenarios::RunOptions;
use crate::synth::Lcg;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn input(options: &RunOptions) -> Result<Vec<u8>, String> {
    if options.quick {
        let p = repo().join("fixtures/xlsx/features/styles.xlsx");
        return std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()));
    }
    let p = repo().join("fixtures/large/sheet-1m.xlsx");
    if !p.exists() {
        let status = std::process::Command::new("python3")
            .args([
                "-I",
                "tools/make-fixtures.py",
                "--synthetic-large",
                "--only",
                "xlsx",
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

fn viewport(x: f32, y: f32) -> Viewport {
    Viewport {
        width_px: 1440.0,
        height_px: 900.0,
        scroll: Point::new(x, y),
        zoom: 1.0,
        dpr: 2.0,
        mode: ViewMode::Normal,
    }
}

fn open(session: &mut Session, bytes: &[u8]) -> Result<kiln_common::DocId, String> {
    session
        .open(
            bytes,
            OpenHint {
                name: Some("sheet-1m.xlsx".into()),
                format: Some(FileFormat::Xlsx),
                ..OpenHint::default()
            },
        )
        .map(|s| s.doc)
        .map_err(|e| e.to_string())
}

/// `grid-open`.
pub fn open_first_frame(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let config = if options.quick {
        Config::new(0, 2)
    } else {
        Config::new(1, 5)
    };
    let mut times = Vec::new();
    let mut frames = Vec::new();
    for run in 0..config.warmup + config.iterations {
        let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
        let t0 = Instant::now();
        let doc = open(&mut session, &bytes)?;
        session
            .set_viewport(doc, viewport(0.0, 0.0))
            .map_err(|e| e.to_string())?;
        let t1 = Instant::now();
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        let t2 = Instant::now();
        if d.tiles.is_empty() {
            return Err("the first frame has no tiles".into());
        }
        drop(session);
        if run >= config.warmup {
            times.push(t2 - t0);
            frames.push((t2 - t1).as_secs_f64() * 1000.0);
        }
    }
    let mut sample = summarize("grid-open", &times, None);
    frames.sort_by(f64::total_cmp);
    sample
        .extra
        .insert("frame_ms".into(), percentile(&frames, 95.0));
    Ok(sample)
}

fn opened(options: &RunOptions) -> Result<(Session, kiln_common::DocId, f32), String> {
    let bytes = input(options)?;
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = open(&mut session, &bytes)?;
    // The data is on the last sheet ("About" comes first).
    let last = session
        .query(Some(doc), &kiln_protocol::Query::Grid(GridQuery::State))
        .ok()
        .and_then(|r| match r {
            kiln_protocol::QueryResult::Grid(GridQueryResult::State(s)) => Some(s.sheets.len()),
            _ => None,
        })
        .unwrap_or(1);
    if last > 1 {
        session
            .execute(
                Some(doc),
                kiln_protocol::Command::Grid(GridCommand::ActivateSheet {
                    index: (last - 1) as u32,
                }),
            )
            .map_err(|e| e.to_string())?;
    }
    session
        .set_viewport(doc, viewport(0.0, 0.0))
        .map_err(|e| e.to_string())?;
    let first = session.frame(doc).map_err(|e| e.to_string())?;
    Ok((session, doc, first.extent.h))
}

/// `grid-frame`.
pub fn frame(options: &RunOptions) -> Result<Sample, String> {
    let (mut session, doc, height) = opened(options)?;
    let config = if options.quick {
        Config::new(0, 3)
    } else {
        Config::new(10, 200)
    };
    let mut rng = Lcg::new(17);
    let mut tiles = 0usize;
    let span = (height - 900.0).max(1.0);
    let mut sample = measure("grid-frame", config, || {
        #[allow(clippy::cast_precision_loss)]
        let y = (rng.below(10_000) as f32 / 10_000.0) * span;
        #[allow(clippy::cast_precision_loss)]
        let x = rng.below(4) as f32 * 300.0;
        session
            .set_viewport(doc, viewport(x, y))
            .map_err(|e| e.to_string())?;
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        tiles += d.tiles.len();
        Ok(())
    })?;
    if tiles == 0 {
        return Err("jumping produced no tiles".into());
    }
    sample.extra.insert("extent_px".into(), f64::from(height));
    Ok(sample)
}

/// `grid-scroll`.
pub fn scroll(options: &RunOptions) -> Result<Sample, String> {
    let (mut session, doc, _) = opened(options)?;
    let config = if options.quick {
        Config::new(0, 10)
    } else {
        Config::new(30, 600)
    };
    // 2,000 px/s at 60 frames per second.
    let step = 2000.0 / 60.0;
    let mut y = 0.0_f32;
    let mut sent = 0usize;
    let sample = measure("grid-scroll", config, || {
        y += step;
        session
            .set_viewport(doc, viewport(0.0, y))
            .map_err(|e| e.to_string())?;
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        sent += d.tiles.len();
        Ok(())
    })?;
    if sent == 0 {
        return Err("scrolling sent no tiles".into());
    }
    Ok(sample)
}

/// `grid-cf-100k`: a colour scale, a data bar and a duplicate-values rule applied to
/// C2:C100000 of the 1M-cell sheet, and the frame that paints them (each rule computes its
/// range's statistics once; they are kept until the range's cells change).
pub fn cf_100k(options: &RunOptions) -> Result<Sample, String> {
    use kiln_protocol::commands::grid_features::{
        GridCfRule, GridCfRuleKind, GridCfStyle, GridCfValue, GridCfValueKind,
    };
    let (mut session, doc, _) = opened(options)?;
    let config = if options.quick {
        Config::new(0, 2)
    } else {
        Config::new(1, 5)
    };
    let range = if options.quick {
        "C2:C40"
    } else {
        "C2:C100000"
    };
    let v = |kind| GridCfValue {
        kind,
        value: String::new(),
        gte: true,
    };
    let rules = [
        GridCfRuleKind::ColorScale {
            points: vec![v(GridCfValueKind::Min), v(GridCfValueKind::Max)],
            colors: vec!["F8696B".into(), "63BE7B".into()],
        },
        GridCfRuleKind::DataBar {
            min: v(GridCfValueKind::Automatic),
            max: v(GridCfValueKind::Automatic),
            color: "638EC6".into(),
            gradient: true,
            show_value: true,
            negative_color: None,
            border_color: None,
        },
        GridCfRuleKind::Duplicates { unique: false },
    ];
    let mut y = 0.0_f32;
    let sample = measure("grid-cf-100k", config, || {
        for kind in &rules {
            session
                .execute(
                    Some(doc),
                    kiln_protocol::Command::Grid(GridCommand::AddConditionalFormat {
                        rule: GridCfRule {
                            id: 0,
                            applies_to: range.into(),
                            kind: kind.clone(),
                            style: matches!(kind, GridCfRuleKind::Duplicates { .. }).then(|| {
                                GridCfStyle {
                                    fill: Some("FFC7CE".into()),
                                    ..GridCfStyle::default()
                                }
                            }),
                            stop_if_true: false,
                        },
                    }),
                )
                .map_err(|e| e.to_string())?;
        }
        // A new place, so the frame paints new tiles with the rules.
        y += 20_000.0;
        session
            .set_viewport(doc, viewport(0.0, y))
            .map_err(|e| e.to_string())?;
        session.frame(doc).map_err(|e| e.to_string())?;
        for _ in 0..3 {
            session
                .execute(
                    Some(doc),
                    kiln_protocol::Command::Suite(kiln_protocol::SuiteCommand::Undo),
                )
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    })?;
    Ok(sample)
}

/// `grid-pivot-100k`: a PivotTable over the 100,000 rows of the 1M-cell sheet (its first
/// row the field names) on a new sheet, a row field, a column field and a value field placed one
/// at a time, a Refresh (the cache read again from the source), then the undo of all of it.
pub fn pivot_100k(options: &RunOptions) -> Result<Sample, String> {
    use kiln_protocol::commands::grid_analysis::{GridPivotArea, GridPivotEdit};
    // Quick mode: the PivotTable fixture's Data sheet (Region, Product, Quarter, Rep, Units, Price).
    let (mut session, doc) = if options.quick {
        let p = repo().join("fixtures/xlsx/features/pivot-tables.xlsx");
        let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
        let doc = open(&mut session, &bytes)?;
        (session, doc)
    } else {
        let (session, doc, _) = opened(options)?;
        (session, doc)
    };
    let config = if options.quick {
        Config::new(0, 2)
    } else {
        Config::new(1, 5)
    };
    let (source, fields) = if options.quick {
        ("Data!$A$1:$F$25", [0, 2, 4])
    } else {
        // B (category) down, G (region) across, the sum of C (amount).
        ("Data!$A$1:$J$100000", [1, 6, 2])
    };
    let mut run = |c: kiln_protocol::Command| -> Result<(), String> {
        session
            .execute(Some(doc), c)
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    let grid = kiln_protocol::Command::Grid;
    let place = |field: u32, area: GridPivotArea| GridCommand::Pivot {
        edit: GridPivotEdit::Place {
            field: Some(field),
            area,
            position: None,
        },
    };
    let sample = measure("grid-pivot-100k", config, || {
        run(grid(GridCommand::CreatePivot {
            source: source.into(),
            to: None,
        }))?;
        run(grid(place(fields[0], GridPivotArea::Rows)))?;
        run(grid(place(fields[1], GridPivotArea::Columns)))?;
        run(grid(place(fields[2], GridPivotArea::Values)))?;
        run(grid(GridCommand::RefreshPivots { all: false }))?;
        for _ in 0..5 {
            run(kiln_protocol::Command::Suite(
                kiln_protocol::SuiteCommand::Undo,
            ))?;
        }
        Ok(())
    })?;
    Ok(sample)
}
