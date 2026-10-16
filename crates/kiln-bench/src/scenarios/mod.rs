// SPDX-License-Identifier: AGPL-3.0-or-later
//! The benchmarks. Each builds its own deterministic input, runs under the [`harness`] and
//! returns a [`Sample`]. Work packages add benchmarks here as their features land (`doc-open` with
//! the `.docx` reader and Write layout, `xlsx-open` with the `.xlsx` reader, …) and point the
//! matching row of `bench/budgets.toml` at them.
//!
//! [`harness`]: crate::harness

mod ooxml_repack;
mod recalc;
mod scratch_frame;
mod text_layout;
#[path = "../benches/write.rs"]
mod write; // WP-63: Kiln Write open and frame
#[path = "../benches/xlsx.rs"]
mod xlsx; // WP-70: Kiln Grid file open

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
