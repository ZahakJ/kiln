// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-bench
//!
//! Benchmarks and the performance budgets of PLAN.md §3.9.
//!
//! - [`harness`]: warm-up, timed iterations, p50/p95 and peak resident memory (sampled from
//!   `/proc/self/status` on Linux).
//! - [`calibrate`]: a fixed CPU-bound workload whose time against the reference machine's gives
//!   the machine factor that scales time budgets.
//! - [`scenarios`]: the benchmarks (`text-layout-1000`, `scratch-frame-production`,
//!   `ooxml-repack-large`, `recalc-full` for B8, `recalc-incremental` for B9; format and
//!   collaboration work packages add theirs).
//! - [`budgets`]: `bench/budgets.toml`, the assertion and the generated block of
//!   `docs/qa/perf.md`.
//! - [`synth`]: deterministic synthetic inputs (word streams, a large `.xlsx` package).
//!
//! The `kiln-bench` binary drives them; `cargo xtask budgets [--ci]` builds it in release mode and
//! runs `kiln-bench assert`.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod budgets;
pub mod calibrate;
pub mod harness;
pub mod scenarios;
pub mod synth;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
