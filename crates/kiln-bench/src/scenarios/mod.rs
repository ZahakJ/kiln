// SPDX-License-Identifier: AGPL-3.0-or-later
//! The benchmarks. Each builds its own deterministic input, runs under the [`harness`] and
//! returns a [`Sample`]. Benchmarks are added here as features land (`doc-open` with
//! the `.docx` reader and Write layout, `xlsx-open` with the `.xlsx` reader, …) and point the
//! matching row of `bench/budgets.toml` at them.
//!
//! [`harness`]: crate::harness

#[path = "../benches/collab.rs"]
mod collab; // collaboration (B14)
#[path = "../benches/deck.rs"]
mod deck; // Kiln Deck open and frame
#[path = "../benches/grid.rs"]
mod grid; // Kiln Grid surface
mod ooxml_repack;
#[path = "../benches/pdf.rs"]
mod pdf; // PDF export (B11)
mod recalc;
mod scratch_frame;
mod text_layout;
#[path = "../benches/write.rs"]
mod write; // Kiln Write open and frame
#[path = "../benches/xlsx.rs"]
mod xlsx; // Kiln Grid file open

use std::time::Duration;

use crate::harness::{ContentionMeter, RssSampler, Sample};

/// How a benchmark runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RunOptions {
    /// Smaller inputs and fewer iterations (tests and smoke runs; the numbers mean nothing).
    pub quick: bool,
}

/// A registered benchmark.
#[derive(Debug, Clone, Copy)]
pub struct Bench {
    /// Name used on the command line and in `bench/budgets.toml`.
    pub name: &'static str,
    /// What it measures.
    pub description: &'static str,
    run: fn(&RunOptions) -> Result<Sample, String>,
}

impl Bench {
    /// Runs the benchmark, sampling the peak resident memory over set-up and runs.
    ///
    /// # Errors
    ///
    /// When the benchmark's set-up or a run fails.
    pub fn run(&self, options: &RunOptions) -> Result<Sample, String> {
        let rss = RssSampler::start(Duration::from_millis(5));
        let contention = ContentionMeter::start();
        let result = (self.run)(options);
        let contention = contention.finish();
        let peak = rss.finish();
        result.map(|mut s| {
            s.name = self.name.to_owned();
            s.peak_rss_mb = peak;
            s.contention = contention;
            s
        })
    }
}

/// Every benchmark, in report order.
#[must_use]
pub fn all() -> Vec<Bench> {
    vec![
        Bench {
            name: "text-layout-1000",
            description: "Lay out 1,000 mixed paragraphs at 600 px with warm caches (kiln-text)",
            run: text_layout::run,
        },
        Bench {
            name: "scratch-frame-production",
            description: "One frame after scrolling a 60-page scratch document (kiln-session → SceneDelta)",
            run: scratch_frame::run,
        },
        Bench {
            name: "ooxml-repack-large",
            description: "Open, inflate every part and save a 1,000,000-cell workbook package (kiln-ooxml)",
            run: ooxml_repack::run,
        },
        Bench {
            name: "recalc-full",
            description: "Full recalculation of 1,000,000 formula cells on 4 threads (kiln-formula)",
            run: recalc::full,
        },
        Bench {
            name: "recalc-incremental",
            description: "One edit + recalculation in a 100,000-formula workbook (kiln-formula)",
            run: recalc::incremental,
        },
        Bench {
            name: "write-open-300p",
            description: "Open the 300-page .docx to its first painted page; full_ms: read + full pagination (B3)",
            run: write::open_300p,
        },
        Bench {
            name: "write-frame",
            description: "One frame after jumping within the open 300-page .docx (kiln-doc-layout → SceneDelta)",
            run: write::frame,
        },
        Bench {
            name: "xlsx-open",
            description: "Read a 1,000,000-cell .xlsx ready to paint; calc_ms: bind and calculate; full_ms: both (B7)",
            run: xlsx::open,
        },
        Bench {
            name: "grid-open",
            description: "Open the 1,000,000-cell .xlsx in a session to its first painted frame (B7); frame_ms: that frame",
            run: grid::open_first_frame,
        },
        Bench {
            name: "grid-frame",
            description: "One frame after jumping within the 1,000,000-cell sheet at DPR 2 (kiln-sheet-layout → SceneDelta)",
            run: grid::frame,
        },
        Bench {
            name: "grid-scroll",
            description: "One frame per animation frame while scrolling the 1,000,000-cell sheet at 2,000 px/s (B6 core side)",
            run: grid::scroll,
        },
        Bench {
            name: "grid-cf-100k",
            description: "A colour scale, a data bar and a duplicate rule on C2:C100000 of the 1M-cell sheet, then a frame; the undo of the rules is included",
            run: grid::cf_100k,
        },
        Bench {
            name: "grid-pivot-100k",
            description: "A PivotTable over the 100,000 rows of the 1M-cell sheet: create, three fields, Refresh, and their undo",
            run: grid::pivot_100k,
        },
        Bench {
            name: "deck-open-200",
            description: "Open the 200-slide .pptx to the first painted slide (B10); frame_ms: that frame; rail_ms: 8 thumbnails",
            run: deck::open_200,
        },
        Bench {
            name: "deck-frame",
            description: "One frame after going to another slide of the 200-slide .pptx at DPR 2; drag_ms: one frame of a shape drag",
            run: deck::frame,
        },
        Bench {
            name: "deck-show-frame",
            description: "A slide-show frame half way through Push on the 200-slide .pptx, 1920 × 1080; step_ms: the first frame after Next",
            run: deck::show_frame_bench,
        },
        Bench {
            name: "pdf-export-300p",
            description: "Export the open 300-page .docx to tagged PDF: print layout + PDF writer; export_ms: the writer alone (B11)",
            run: pdf::export_300p,
        },
        Bench {
            name: "collab-apply-1000",
            description: "Merge 1,000 remote operations into a shared Write document, one update each, into model ops (B14); batch_ms: as one update",
            run: collab::apply_1000,
        },
    ]
}

/// A benchmark by name.
#[must_use]
pub fn find(name: &str) -> Option<Bench> {
    all().into_iter().find(|b| b.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_unique_and_found() {
        let all = all();
        for b in &all {
            assert_eq!(find(b.name).map(|f| f.name), Some(b.name));
            assert_eq!(all.iter().filter(|o| o.name == b.name).count(), 1);
        }
        assert!(find("nope").is_none());
    }

    #[test]
    fn every_benchmark_runs_quickly_in_quick_mode() {
        for b in all() {
            let s = b.run(&RunOptions { quick: true }).unwrap();
            assert_eq!(s.name, b.name);
            assert!(s.iterations >= 1);
            assert!(s.p95_ms >= s.p50_ms && s.p50_ms >= 0.0, "{s:?}");
            if cfg!(target_os = "linux") {
                assert!(s.peak_rss_mb.is_some());
            }
        }
    }
}
