// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask budgets [--ci]`: the performance budgets of PLAN.md §3.9.
//!
//! Builds `kiln-bench` in release mode and runs `kiln-bench assert` against `bench/budgets.toml`,
//! which measures every row that has a benchmark, writes the generated block of
//! `docs/qa/perf.md` (every budget id with its status) and exits 1 when a measured budget is
//! missed. `--ci` applies the CI tolerance (×1.5) on top of the machine factor.

use crate::util;

/// Runs the command.
///
/// # Errors
///
/// Unknown options, a failed build, or a missed budget.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut ci = false;
    for a in args {
        match a.as_str() {
            "--ci" | "--check" => ci = true,
            other => return Err(format!("unknown option '{other}' (usage: budgets [--ci])")),
        }
    }
    let mut cmd = util::args(&[
        "run",
        "--release",
        "-q",
        "-p",
        "kiln-bench",
        "--",
        "assert",
        "--budgets",
        "bench/budgets.toml",
        "--report",
        "docs/qa/perf.md",
    ]);
    if ci {
        cmd.push("--ci".into());
    }
    util::run(&util::cargo(), &cmd)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_unknown_options() {
        let err = super::run(&["--fast".to_owned()]).unwrap_err();
        assert!(err.contains("unknown option"));
    }
}
