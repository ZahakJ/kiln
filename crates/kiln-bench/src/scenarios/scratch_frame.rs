// SPDX-License-Identifier: AGPL-3.0-or-later
//! `scratch-frame-production`: the time `Session::frame` takes to produce the `SceneDelta` after
//! the viewport moves, in a 60-page scratch document (1000 × 900 CSS px viewport, device pixel
//! ratio 2). Each run jumps to the next position of a fixed tour through the document (so new
//! pages are laid out into tiles, glyphs rasterised and old tiles evicted), the way a scroll bar
//! drag does. `open_first_frame_ms` (open + layout + first frame) is reported as an extra figure.

use std::time::Instant;

use kiln_protocol::geom::Point;
use kiln_protocol::{FileFormat, OpenHint, ViewMode, Viewport};
use kiln_session::{DeferredHost, Session};

use super::RunOptions;
use crate::harness::{Config, Sample, measure};
use crate::synth::Lcg;

const WORDS: &str = concat!(
    "the kiln holds its heat long after fire is out stoneware porcelain glaze cone ",
    "eleven reduction shelf wadding test tiles chamber flame path spy hole we load ",
    "together twice year",
);

fn text(paragraphs: usize) -> String {
    let mut rng = Lcg::new(42);
    (0..paragraphs)
        .map(|_| {
            let n = 30 + rng.below(70);
            let mut p = rng.words(WORDS, n, " ");
            p.push('.');
            p
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn run(options: &RunOptions) -> Result<Sample, String> {
    let (paragraphs, config) = if options.quick {
        (40, Config::new(0, 3))
    } else {
        (600, Config::new(10, 200))
    };
    let e = |e: kiln_common::KilnError| e.to_string();
    let t = Instant::now();
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = session
        .open(
            text(paragraphs).as_bytes(),
            OpenHint {
                name: Some("bench.txt".into()),
                format: Some(FileFormat::Txt),
                ..OpenHint::default()
            },
        )
        .map_err(e)?
        .doc;
    let mut viewport = Viewport {
        width_px: 1000.0,
        height_px: 900.0,
        scroll: Point::new(0.0, 0.0),
        zoom: 1.0,
        dpr: 2.0,
        mode: ViewMode::PrintLayout,
    };
    session.set_viewport(doc, viewport).map_err(e)?;
    let first = session.frame(doc).map_err(e)?;
    let open_ms = t.elapsed().as_secs_f64() * 1000.0;
    let height = first.extent.h;
    if first.tiles.is_empty() || height < 1000.0 {
        return Err(format!("the first frame painted nothing (extent {height})"));
    }
    // A fixed tour: positions spread over the document, never adjacent.
    let mut rng = Lcg::new(7);
    let mut produced_tiles = 0usize;
    let mut sample = measure("scratch-frame-production", config, || {
        #[allow(clippy::cast_precision_loss)]
        let y = (rng.below(1000) as f32 / 1000.0) * (height - 900.0).max(0.0);
        viewport.scroll = Point::new(0.0, y);
        session.set_viewport(doc, viewport).map_err(e)?;
        let delta = session.frame(doc).map_err(e)?;
        produced_tiles += delta.tiles.len();
        Ok(())
    })?;
    if produced_tiles == 0 {
        return Err("scrolling produced no tiles".into());
    }
    sample.extra.insert("open_first_frame_ms".into(), open_ms);
    sample.extra.insert("extent_px".into(), f64::from(height));
    Ok(sample)
}
