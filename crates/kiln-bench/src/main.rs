// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-bench`: runs benchmarks and asserts the performance budgets (PLAN.md §3.9).
//!
//! ```text
//! kiln-bench list                                  the benchmarks
//! kiln-bench run <name>... [--json] [--quick]      run benchmarks and print their samples
//! kiln-bench calibrate                             this machine's calibration time
//! kiln-bench assert [--ci] [--budgets bench/budgets.toml] [--report docs/qa/perf.md]
//! ```
//!
//! `assert` (also spelled `--assert`) exits 1 when a measured budget is missed; pending and
//! manual rows are listed but never fail. Numbers from a debug build are not budgets, so
//! `assert` refuses to run in one unless `--quick` is given (a smoke run).
#![forbid(unsafe_code)]

use std::path::PathBuf;
use std::process::ExitCode;

use kiln_bench::budgets::{self, AssertOptions, BudgetFile};
use kiln_bench::calibrate;
use kiln_bench::harness::Sample;
use kiln_bench::scenarios::{self, RunOptions};

const USAGE: &str = "usage: kiln-bench list
       kiln-bench run <name>... [--json] [--quick]
       kiln-bench calibrate
       kiln-bench assert [--ci] [--budgets <budgets.toml>] [--report <perf.md>] [--quick]";

enum Failure {
    Usage(String),
    Failed(String),
}

fn repo_file(rel: &str) -> PathBuf {
    let here = std::env::current_dir().unwrap_or_default();
    here.ancestors()
        .map(|d| d.join(rel))
        .find(|p| p.exists())
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(rel)
        })
}

/// Runs one benchmark in a child process (`kiln-bench run <name> --json`), so its peak memory
/// and timings are its own.
fn run_isolated(name: &str, options: &RunOptions) -> Result<Sample, String> {
    let exe = std::env::current_exe().map_err(|e| format!("cannot find kiln-bench itself: {e}"))?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["run", name, "--json"]);
    if options.quick {
        cmd.arg("--quick");
    }
    let out = cmd.output().map_err(|e| format!("{name}: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_owned());
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().last().unwrap_or_default();
    serde_json::from_str(line).map_err(|e| format!("{name}: unreadable result ({e}): {line}"))
}

fn run_benches(args: &[String]) -> Result<String, Failure> {
    let mut names = Vec::new();
    let mut json = false;
    let mut quick = false;
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            "--quick" => quick = true,
            s if s.starts_with('-') => return Err(Failure::Usage(format!("unknown option '{s}'"))),
            s => names.push(s.to_owned()),
        }
    }
    if names.is_empty() {
        return Err(Failure::Usage(
            "name at least one benchmark (see `kiln-bench list`)".into(),
        ));
    }
    let mut out = Vec::new();
    for name in &names {
        let bench = scenarios::find(name)
            .ok_or_else(|| Failure::Usage(format!("unknown benchmark '{name}'")))?;
        let s = bench
            .run(&RunOptions { quick })
            .map_err(|e| Failure::Failed(format!("{name}: {e}")))?;
        if json {
            out.push(serde_json::to_string(&s).map_err(|e| Failure::Failed(e.to_string()))?);
        } else {
            let extra: Vec<String> = s.extra.iter().map(|(k, v)| format!("{k}={v:.2}")).collect();
            out.push(format!(
                "{name}: p50 {:.2} ms, p95 {:.2} ms, min {:.2} ms, max {:.2} ms over {} runs, peak RSS {}{}{}",
                s.p50_ms,
                s.p95_ms,
                s.min_ms,
                s.max_ms,
                s.iterations,
                s.peak_rss_mb.map_or_else(|| "n/a".into(), |m| format!("{m:.0} MB")),
                if extra.is_empty() { "" } else { "; " },
                extra.join(", ")
            ));
        }
    }
    Ok(out.join("\n"))
}

fn assert_budgets(args: &[String]) -> Result<String, Failure> {
    let mut ci = false;
    let mut quick = false;
    let mut file = repo_file("bench/budgets.toml");
    let mut report: Option<PathBuf> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ci" => ci = true,
            "--quick" => quick = true,
            "--budgets" => {
                file = it
                    .next()
                    .map(PathBuf::from)
                    .ok_or_else(|| Failure::Usage("--budgets needs a file".into()))?;
            }
            "--report" => {
                report = Some(
                    it.next()
                        .map(PathBuf::from)
                        .ok_or_else(|| Failure::Usage("--report needs a file".into()))?,
                );
            }
            s => return Err(Failure::Usage(format!("unknown option '{s}'"))),
        }
    }
    if cfg!(debug_assertions) && !quick {
        return Err(Failure::Usage(
            "budgets are measured in a release build: `cargo run --release -p kiln-bench -- assert` (or `cargo xtask budgets`); --quick runs a smoke test".into(),
        ));
    }
    let budgets = BudgetFile::load(&file).map_err(Failure::Failed)?;
    let options = AssertOptions { ci, quick };
    let outcome = budgets::evaluate_with(
        &budgets,
        options,
        budgets::calibration_ms(quick),
        run_isolated,
    );
    let mut text = outcome.to_text();
    if let Some(path) = report {
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let block = outcome.to_markdown(&budgets::utc_date(secs));
        std::fs::write(&path, budgets::splice(&existing, &block))
            .map_err(|e| Failure::Failed(format!("{}: {e}", path.display())))?;
        text.push_str(&format!("wrote {}\n", path.display()));
    }
    if outcome.passed() {
        Ok(text.trim_end().to_owned())
    } else {
        Err(Failure::Failed(format!(
            "{}\nbudgets missed",
            text.trim_end()
        )))
    }
}

fn run(args: &[String]) -> Result<String, Failure> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        None | Some("-h" | "--help" | "help") => Ok(USAGE.to_owned()),
        Some("list") => Ok(scenarios::all()
            .iter()
            .map(|b| format!("{:<26} {}", b.name, b.description))
            .collect::<Vec<_>>()
            .join("\n")),
        Some("run") => run_benches(rest),
        Some("calibrate") => Ok(format!(
            "calibration {:.1} ms ({} rounds, best of 5)",
            calibrate::calibrate(),
            calibrate::ROUNDS
        )),
        Some("assert" | "--assert") => assert_budgets(rest),
        Some(other) => Err(Failure::Usage(format!("unknown command '{other}'"))),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(m)) => {
            eprintln!("kiln-bench: {m}\n\n{USAGE}");
            ExitCode::from(2)
        }
        Err(Failure::Failed(m)) => {
            eprintln!("{m}");
            ExitCode::FAILURE
        }
    }
}
