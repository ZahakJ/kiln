// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask budgets [--ci] [--only <id>]`: the performance budgets of PLAN.md §3.9.
//!
//! Builds `kiln-bench` in release mode and runs `kiln-bench assert` against `bench/budgets.toml`,
//! which measures every row that has a benchmark, writes the generated block of
//! `docs/qa/perf.md` (every budget id with its status) and exits 1 when a measured budget is
//! missed. `--ci` applies the CI tolerance (×1.5) on top of the machine factor; `--only B3`
//! measures just those budgets (and leaves the report alone).

use crate::util;

/// Runs the command.
///
/// # Errors
///
/// Unknown options, a failed build, or a missed budget.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut ci = false;
    let mut only: Vec<String> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ci" | "--check" => ci = true,
            "--only" => only.push(
                it.next()
                    .cloned()
                    .ok_or_else(|| "--only needs a budget id (B3)".to_owned())?,
            ),
            other => {
                return Err(format!(
                    "unknown option '{other}' (usage: budgets [--ci] [--only <id>]...)"
                ));
            }
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
    for id in only {
        cmd.push("--only".into());
        cmd.push(id);
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
