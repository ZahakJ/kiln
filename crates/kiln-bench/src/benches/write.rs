// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln Write benchmarks (WP-63), on `fixtures/large/doc-300p.docx` (300 pages of headings, body
//! text, tables and pictures; generated on demand by `tools/make-fixtures.py --synthetic-large`,
//! which the benchmark runs when the file is missing):
//!
//! - `write-open-300p` (B3): open the file in a session and produce the first frame of a
//!   1000 × 900 viewport (read, lay out the first pages, paint): the time to the first painted
//!   page. The extra figure `full_ms` is the time to read and fully paginate the document.
//! - `write-frame`: one frame after jumping to another place in the open document (new pages
//!   laid out into tiles, glyphs rasterised), at device pixel ratio 2.
//!
//! In quick mode both use `fixtures/docx/features/long-text.docx` instead.

use std::path::{Path, PathBuf};
use std::time::Instant;

use kiln_protocol::geom::Point;
use kiln_protocol::{FileFormat, OpenHint, ViewMode, Viewport};
use kiln_session::{DeferredHost, Session};

use crate::harness::{Config, Sample, measure};
use crate::scenarios::RunOptions;
use crate::synth::Lcg;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The input document's bytes (generating the large fixture when it is missing).
fn input(options: &RunOptions) -> Result<Vec<u8>, String> {
    if options.quick {
        let p = repo().join("fixtures/docx/features/long-text.docx");
        return std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()));
    }
    let p = repo().join("fixtures/large/doc-300p.docx");
    if !p.exists() {
        let status = std::process::Command::new("python3")
            .args([
                "-I",
                "tools/make-fixtures.py",
                "--synthetic-large",
                "--only",
                "docx",
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

fn viewport(y: f32) -> Viewport {
    Viewport {
        width_px: 1000.0,
        height_px: 900.0,
        scroll: Point::new(0.0, y),
        zoom: 1.0,
        dpr: 2.0,
        mode: ViewMode::PrintLayout,
    }
}

fn open(session: &mut Session, bytes: &[u8]) -> Result<kiln_common::DocId, String> {
    session
        .open(
            bytes,
            OpenHint {
                name: Some("doc-300p.docx".into()),
                format: Some(FileFormat::Docx),
                ..OpenHint::default()
            },
        )
        .map(|s| s.doc)
        .map_err(|e| e.to_string())
}

fn percentile(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(f64::total_cmp);
    if v.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((p / 100.0 * v.len() as f64).ceil() as usize).clamp(1, v.len()) - 1;
    v[i]
}

/// `write-open-300p`.
pub fn open_300p(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let config = if options.quick {
        Config::new(0, 2)
    } else {
        Config::new(2, 12)
    };
    let mut pages = 0usize;
    let mut sample = measure("write-open-300p", config, || {
        let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
        let doc = open(&mut session, &bytes)?;
        session
            .set_viewport(doc, viewport(0.0))
            .map_err(|e| e.to_string())?;
        let first = session.frame(doc).map_err(|e| e.to_string())?;
        if first.tiles.is_empty() {
            return Err("the first frame painted nothing".into());
        }
        Ok(())
    })?;
    // Full pagination: read and lay out every page.
    let mut full = Vec::new();
    for _ in 0..config.iterations.max(1) {
        let t = Instant::now();
        let read = kiln_io_docx::read_docx(&bytes).map_err(|e| e.to_string())?;
        let mut layout = kiln_doc_layout::DocLayout::new(kiln_doc_layout::default_fonts());
        layout.layout(&read.document);
        pages = layout.pages().len();
        full.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    sample
        .extra
        .insert("full_ms".into(), percentile(&mut full, 95.0));
    #[allow(clippy::cast_precision_loss)]
    sample.extra.insert("pages".into(), pages as f64);
    if !options.quick && pages < 250 {
        return Err(format!("doc-300p.docx paginated into {pages} pages"));
    }
    Ok(sample)
}

/// `write-frame`.
pub fn frame(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let config = if options.quick {
        Config::new(0, 3)
    } else {
        Config::new(10, 200)
    };
    let mut session = Session::new(Box::new(DeferredHost::new(|| 0)));
    let doc = open(&mut session, &bytes)?;
    session
        .set_viewport(doc, viewport(0.0))
        .map_err(|e| e.to_string())?;
    // Let the layout finish (a few frames).
    let mut height = 0.0_f32;
    for _ in 0..64 {
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        if (d.extent.h - height).abs() < 0.5 && height > 0.0 {
            break;
        }
        height = d.extent.h;
    }
    let mut rng = Lcg::new(11);
    let mut tiles = 0usize;
    let mut sample = measure("write-frame", config, || {
        #[allow(clippy::cast_precision_loss)]
        let y = (rng.below(1000) as f32 / 1000.0) * (height - 900.0).max(0.0);
        session
            .set_viewport(doc, viewport(y))
            .map_err(|e| e.to_string())?;
        let d = session.frame(doc).map_err(|e| e.to_string())?;
        tiles += d.tiles.len();
        Ok(())
    })?;
    if tiles == 0 {
        return Err("scrolling produced no tiles".into());
    }
    sample.extra.insert("extent_px".into(), f64::from(height));
    Ok(sample)
}
