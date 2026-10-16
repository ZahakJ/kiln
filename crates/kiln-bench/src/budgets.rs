// SPDX-License-Identifier: AGPL-3.0-or-later
//! The performance budgets of PLAN.md §3.9, read from `bench/budgets.toml`, measured and
//! asserted (`kiln-bench assert`, run by `cargo xtask budgets`).
//!
//! Every row has an id from the plan (`B3`, `B7`…) or a component name, a limit, and exactly one
//! of: `bench` (the [`crate::scenarios`] benchmark that measures it, with the `metric` to read),
//! `pending` (the work package that will add the benchmark) or `manual` (measured elsewhere, such
//! as the build time). Time limits are scaled by the machine factor of [`crate::calibrate`] and,
//! with `--ci`, by the CI tolerance; memory limits are not scaled.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use serde::Deserialize;

use crate::calibrate;
use crate::harness::Sample;
use crate::scenarios::{self, RunOptions};

/// One row of `bench/budgets.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    /// Plan id (`B8`) or component name.
    pub id: String,
    /// Which figure of a multi-figure budget (`first page painted`).
    #[serde(default)]
    pub part: Option<String>,
    /// What is measured.
    pub scenario: String,
    /// The limit, in `unit`.
    pub limit: f64,
    /// `ms` or `MB`.
    pub unit: String,
    /// The benchmark that measures it.
    #[serde(default)]
    pub bench: Option<String>,
    /// The figure of the benchmark's sample (`p95` by default).
    #[serde(default)]
    pub metric: Option<String>,
    /// The work package that adds the benchmark.
    #[serde(default)]
    pub pending: Option<String>,
    /// Where it is measured instead.
    #[serde(default)]
    pub manual: Option<String>,
    /// `plan` (§3.9) or `component`.
    #[serde(default = "default_group")]
    pub group: String,
}

fn default_group() -> String {
    "plan".into()
}

impl Budget {
    /// `B3 (first page painted)` or `B8`.
    #[must_use]
    pub fn label(&self) -> String {
        match &self.part {
            Some(p) => format!("{} ({p})", self.id),
            None => self.id.clone(),
        }
    }

    /// The sample figure this row reads.
    #[must_use]
    pub fn metric_name(&self) -> &str {
        self.metric
            .as_deref()
            .unwrap_or(if self.unit == "MB" { "rss" } else { "p95" })
    }
}

/// `bench/budgets.toml`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BudgetFile {
    /// Format version (1).
    pub version: u32,
    /// Multiplier for CI runners.
    pub ci_tolerance: f64,
    /// The calibration time of the reference machine, in ms.
    pub calibration_reference_ms: f64,
    /// Upper bound of the machine factor.
    pub max_machine_factor: f64,
    /// The rows.
    #[serde(rename = "budget")]
    pub budgets: Vec<Budget>,
}

impl BudgetFile {
    /// Parses and validates the file.
    ///
    /// # Errors
    ///
    /// TOML errors, unknown keys, rows without exactly one of `bench`/`pending`/`manual`,
    /// unknown benchmarks, units other than `ms`/`MB`, duplicate labels, bad numbers.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: BudgetFile = toml::from_str(text).map_err(|e| format!("budgets.toml: {e}"))?;
        if file.version != 1 {
            return Err(format!(
                "budgets.toml: version {} is not supported",
                file.version
            ));
        }
        let positive = |v: f64| v.is_finite() && v > 0.0;
        if !positive(file.ci_tolerance)
            || !positive(file.calibration_reference_ms)
            || !positive(file.max_machine_factor)
        {
            return Err("budgets.toml: ci_tolerance, calibration_reference_ms and max_machine_factor must be positive".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for b in &file.budgets {
            let label = b.label();
            if !seen.insert(label.clone()) {
                return Err(format!("budgets.toml: {label} is listed twice"));
            }
            let kinds = [b.bench.is_some(), b.pending.is_some(), b.manual.is_some()];
            if kinds.iter().filter(|k| **k).count() != 1 {
                return Err(format!(
                    "budgets.toml: {label} needs exactly one of bench, pending, manual"
                ));
            }
            if b.unit != "ms" && b.unit != "MB" {
                return Err(format!("budgets.toml: {label}: unit must be ms or MB"));
            }
            if !positive(b.limit) {
                return Err(format!("budgets.toml: {label}: limit must be positive"));
            }
            if b.group != "plan" && b.group != "component" {
                return Err(format!(
                    "budgets.toml: {label}: group must be plan or component"
                ));
            }
            if let Some(name) = &b.bench
                && scenarios::find(name).is_none()
            {
                return Err(format!("budgets.toml: {label}: unknown benchmark '{name}'"));
            }
        }
        Ok(file)
    }

    /// Reads the file.
    ///
    /// # Errors
    ///
    /// I/O errors and those of [`BudgetFile::parse`].
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }
}

/// The verdict of one row.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Measured within the effective limit.
    Pass,
    /// Measured over the effective limit.
    Fail,
    /// A time limit missed by every attempt, but every attempt ran while other processes kept
    /// more than [`harness::CONTENDED`] of the CPUs busy (the lowest share is given): the
    /// timings measure the machine, not the code, so the row neither passes nor fails. A
    /// quiet machine (CI) gives a verdict.
    ///
    /// [`harness::CONTENDED`]: crate::harness::CONTENDED
    Busy(f64),
    /// The benchmark failed to run.
    Error(String),
    /// Not measured yet (the work package that adds the benchmark).
    Pending(String),
    /// Measured outside `kiln-bench`.
    Manual(String),
}

impl Status {
    /// Whether this row fails the assertion.
    #[must_use]
    pub fn is_failure(&self) -> bool {
        matches!(self, Self::Fail | Self::Error(_))
    }

    fn cell(&self) -> String {
        match self {
            Self::Pass => "pass".into(),
            Self::Fail => "**FAIL**".into(),
            Self::Busy(share) => format!(
                "inconclusive (other processes kept {:.0} % of the CPUs busy)",
                share * 100.0
            ),
            Self::Error(e) => format!("**error**: {e}"),
            Self::Pending(wp) => format!("pending ({wp})"),
            Self::Manual(_) => "manual".into(),
        }
    }
}

/// One evaluated row.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The budget.
    pub budget: Budget,
    /// The limit after scaling.
    pub effective: Option<f64>,
    /// The measured figure.
    pub measured: Option<f64>,
    /// The verdict.
    pub status: Status,
    /// How many times its benchmark had been measured when the row was judged (0 when it has
    /// none; more than 1 after a re-measurement, see [`REMEASURES`]).
    pub attempts: usize,
}

/// How [`evaluate`] scales and runs.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssertOptions {
    /// Apply the CI tolerance.
    pub ci: bool,
    /// Quick benchmark runs (tests only; numbers are meaningless).
    pub quick: bool,
}

/// The outcome of a budget run.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// Rows in file order.
    pub rows: Vec<Row>,
    /// The best attempt's sample (lowest p95) by benchmark name.
    pub samples: BTreeMap<String, Sample>,
    /// How many times each benchmark was measured.
    pub attempts: BTreeMap<String, usize>,
    /// This machine's calibration time (ms).
    pub calibration_ms: f64,
    /// The reference machine's (ms).
    pub reference_ms: f64,
    /// The applied machine factor.
    pub machine_factor: f64,
    /// The applied CI tolerance (1 outside CI).
    pub tolerance: f64,
    /// Whether this was a release build (debug numbers are not budgets).
    pub release_build: bool,
}

impl Outcome {
    /// Whether every measured row is within its limit.
    #[must_use]
    pub fn passed(&self) -> bool {
        !self.rows.iter().any(|r| r.status.is_failure())
    }
}

fn fmt_limit(v: f64, unit: &str) -> String {
    if unit == "ms" && v >= 10_000.0 {
        format!("{:.0} s", v / 1000.0)
    } else if v >= 100.0 {
        format!("{v:.0} {unit}")
    } else {
        format!("{v:.1} {unit}")
    }
}

/// How many times a benchmark that misses a budget is measured again (each time from scratch, in
/// a fresh process when run by `kiln-bench assert`) before the row fails.
///
/// A regression is slow every time; a single slow attempt is a machine that was busy (PLAN.md
/// §4.7 has two agents sharing it, and CI runners are noisy). A row therefore fails only when
/// every one of `1 + REMEASURES` attempts misses it, and reports the best attempt's figure; when
/// every attempt also ran on a busy machine, the row is [`Status::Busy`] instead.
pub const REMEASURES: usize = 2;

/// The best (lowest) value of `metric` over the successful attempts.
fn best_metric(attempts: &[Result<Sample, String>], metric: &str) -> Option<f64> {
    attempts
        .iter()
        .filter_map(|a| a.as_ref().ok()?.metric(metric))
        .min_by(f64::total_cmp)
}

/// The lowest contention of the successful attempts, when every one of them reports it.
fn least_contention(attempts: &[Result<Sample, String>]) -> Option<f64> {
    attempts
        .iter()
        .filter_map(|a| a.as_ref().ok())
        .map(|s| s.contention)
        .collect::<Option<Vec<f64>>>()?
        .into_iter()
        .min_by(f64::total_cmp)
}

/// Measures every row: runs each referenced benchmark once (with `run`, injectable for tests),
/// measures it again up to [`REMEASURES`] times while a row that reads it is over its limit, and
/// compares the best attempt with the limit.
pub fn evaluate_with(
    file: &BudgetFile,
    options: AssertOptions,
    calibration_ms: f64,
    mut run: impl FnMut(&str, &RunOptions) -> Result<Sample, String>,
) -> Outcome {
    let machine_factor = calibrate::machine_factor(
        calibration_ms,
        file.calibration_reference_ms,
        file.max_machine_factor,
    );
    let tolerance = if options.ci { file.ci_tolerance } else { 1.0 };
    let run_options = RunOptions {
        quick: options.quick,
    };
    let mut attempts: BTreeMap<String, Vec<Result<Sample, String>>> = BTreeMap::new();
    let mut rows = Vec::with_capacity(file.budgets.len());
    for b in &file.budgets {
        let (effective, measured, status, tries) = match (&b.bench, &b.pending, &b.manual) {
            (Some(name), _, _) => {
                let tries = attempts
                    .entry(name.clone())
                    .or_insert_with(|| vec![run(name, &run_options)]);
                let scale = if b.unit == "ms" {
                    machine_factor * tolerance
                } else {
                    1.0
                };
                let effective = b.limit * scale;
                let metric = b.metric_name();
                let (measured, status) = loop {
                    if let Some(Err(e)) = tries.first() {
                        // A benchmark that cannot run is broken, not slow: no second attempt.
                        break (None, Status::Error(e.clone()));
                    }
                    match best_metric(tries, metric) {
                        Some(v) if v <= effective => break (Some(v), Status::Pass),
                        Some(v) if tries.len() > REMEASURES => {
                            let busy = (b.unit == "ms")
                                .then(|| least_contention(tries))
                                .flatten()
                                .filter(|&c| c > crate::harness::CONTENDED);
                            break (Some(v), busy.map_or(Status::Fail, Status::Busy));
                        }
                        Some(_) => tries.push(run(name, &run_options)),
                        None => {
                            break (None, Status::Error(format!("{name} reports no '{metric}'")));
                        }
                    }
                };
                (Some(effective), measured, status, tries.len())
            }
            (None, Some(wp), _) => (None, None, Status::Pending(wp.clone()), 0),
            (None, None, Some(m)) => (None, None, Status::Manual(m.clone()), 0),
            (None, None, None) => (
                None,
                None,
                Status::Error("no bench, pending or manual".into()),
                0,
            ),
        };
        rows.push(Row {
            budget: b.clone(),
            effective,
            measured,
            status,
            attempts: tries,
        });
    }
    let samples = attempts
        .iter()
        .filter_map(|(k, tries)| {
            let best = tries
                .iter()
                .filter_map(|a| a.as_ref().ok())
                .min_by(|a, b| a.p95_ms.total_cmp(&b.p95_ms))?;
            Some((k.clone(), best.clone()))
        })
        .collect();
    Outcome {
        rows,
        samples,
        attempts: attempts.iter().map(|(k, v)| (k.clone(), v.len())).collect(),
        calibration_ms,
        reference_ms: file.calibration_reference_ms,
        machine_factor,
        tolerance,
        release_build: !cfg!(debug_assertions),
    }
}

/// This machine's calibration time (a shortened run for quick smoke tests).
#[must_use]
pub fn calibration_ms(quick: bool) -> f64 {
    if quick {
        calibrate::calibrate_with(calibrate::ROUNDS / 20, 1) * 20.0
    } else {
        calibrate::calibrate()
    }
}

/// [`evaluate_with`] with the real calibration and the benchmarks run in this process.
/// (`kiln-bench assert` runs each benchmark in a child process instead, so that one benchmark's
/// heap and caches do not colour the next one's figures.)
#[must_use]
pub fn evaluate(file: &BudgetFile, options: AssertOptions) -> Outcome {
    evaluate_with(file, options, calibration_ms(options.quick), |name, o| {
        scenarios::find(name)
            .ok_or_else(|| format!("unknown benchmark '{name}'"))?
            .run(o)
    })
}

/// `YYYY-MM-DD` (UTC) of a Unix time in seconds.
#[must_use]
pub fn utc_date(unix_secs: u64) -> String {
    // Howard Hinnant's days-to-civil algorithm.
    let z = i64::try_from(unix_secs / 86_400).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}")
}

/// Start marker of the generated block in `docs/qa/perf.md`.
pub const BEGIN: &str =
    "<!-- kiln-bench:begin (generated by `cargo xtask budgets`; do not edit) -->";
/// End marker.
pub const END: &str = "<!-- kiln-bench:end -->";

impl Outcome {
    /// Rows left inconclusive by a busy machine.
    #[must_use]
    pub fn inconclusive(&self) -> usize {
        self.rows
            .iter()
            .filter(|r| matches!(r.status, Status::Busy(_)))
            .count()
    }

    /// `pass`, `FAIL`, or `pass, 1 inconclusive`.
    #[must_use]
    pub fn verdict(&self) -> String {
        match (self.passed(), self.inconclusive()) {
            (false, _) => "FAIL".into(),
            (true, 0) => "pass".into(),
            (true, n) => format!("pass, {n} inconclusive"),
        }
    }

    /// The generated Markdown block (between [`BEGIN`] and [`END`]).
    #[must_use]
    pub fn to_markdown(&self, date: &str) -> String {
        let mut md = String::new();
        md.push_str(BEGIN);
        md.push_str("\n## Automated budgets\n\n");
        let _ = writeln!(
            md,
            "Measured by `cargo xtask budgets{}` on {date} ({} build of `kiln-bench`, {} {}, {} CPUs). \
Calibration {:.1} ms against the reference machine's {:.1} ms: machine factor {:.2}{}. \
Time limits are multiplied by the factor{}; memory limits are not. A benchmark over a limit is \
measured again, up to {} more times after waiting for a quieter machine, and the row fails only when every attempt \
misses it; when every attempt also ran while other processes kept more than {:.0} % of the CPUs busy, the row is \
inconclusive instead (it does not fail, and a quiet machine gives the verdict). Result: **{}**.\n",
            if self.tolerance > 1.0 { " --ci" } else { "" },
            if self.release_build {
                "release"
            } else {
                "debug"
            },
            std::env::consts::OS,
            std::env::consts::ARCH,
            std::thread::available_parallelism().map_or(0, std::num::NonZero::get),
            self.calibration_ms,
            self.reference_ms,
            self.machine_factor,
            if self.tolerance > 1.0 {
                format!(", CI tolerance ×{}", self.tolerance)
            } else {
                String::new()
            },
            if self.tolerance > 1.0 {
                " and the tolerance"
            } else {
                ""
            },
            REMEASURES,
            crate::harness::CONTENDED * 100.0,
            self.verdict(),
        );
        for (group, title) in [
            ("plan", "Budgets of PLAN.md §3.9"),
            ("component", "Component budgets"),
        ] {
            let rows: Vec<&Row> = self
                .rows
                .iter()
                .filter(|r| r.budget.group == group)
                .collect();
            if rows.is_empty() {
                continue;
            }
            let _ = writeln!(md, "### {title}\n");
            md.push_str("| Id | Scenario | Budget | Effective limit | Measured | Status |\n|---|---|---|---|---|---|\n");
            for r in rows {
                let b = &r.budget;
                let measured = match (r.measured, &r.status) {
                    (Some(v), _) if r.attempts > 1 => format!(
                        "{} ({}, best of {} attempts)",
                        fmt_limit(v, &b.unit),
                        b.metric_name(),
                        r.attempts
                    ),
                    (Some(v), _) => format!("{} ({})", fmt_limit(v, &b.unit), b.metric_name()),
                    (None, Status::Manual(where_)) => where_.clone(),
                    _ => "—".into(),
                };
                let _ = writeln!(
                    md,
                    "| {} | {} | < {} | {} | {} | {} |",
                    b.label(),
                    b.scenario,
                    fmt_limit(b.limit, &b.unit),
                    r.effective
                        .map_or_else(|| "—".into(), |e| fmt_limit(e, &b.unit)),
                    measured,
                    r.status.cell()
                );
            }
            md.push('\n');
        }
        if !self.samples.is_empty() {
            md.push_str("### Benchmarks\n\nThe best attempt of each benchmark (lowest p95); \"others' CPU\" is the share of all CPUs other processes used while it ran.\n\n| Benchmark | Attempts | Runs | p50 | p95 | min | max | Peak RSS | Others' CPU | Also |\n|---|---|---|---|---|---|---|---|---|---|\n");
            for (name, s) in &self.samples {
                let extra: Vec<String> = s
                    .extra
                    .iter()
                    .map(|(k, v)| {
                        if v.fract() == 0.0 && v.abs() < 1e12 {
                            format!("{k} {v:.0}")
                        } else {
                            format!("{k} {v:.2}")
                        }
                    })
                    .collect();
                let _ = writeln!(
                    md,
                    "| `{name}` | {} | {} | {:.2} ms | {:.2} ms | {:.2} ms | {:.2} ms | {} | {} | {} |",
                    self.attempts.get(name).copied().unwrap_or(1),
                    s.iterations,
                    s.p50_ms,
                    s.p95_ms,
                    s.min_ms,
                    s.max_ms,
                    s.peak_rss_mb
                        .map_or_else(|| "—".into(), |m| format!("{m:.0} MB")),
                    s.contention
                        .map_or_else(|| "—".into(), |c| format!("{:.0} %", c * 100.0)),
                    extra.join(", ")
                );
            }
            md.push('\n');
        }
        md.push_str(END);
        md.push('\n');
        md
    }

    /// A plain-text table for the terminal.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = format!(
            "calibration {:.1} ms (reference {:.1} ms), machine factor {:.2}, tolerance ×{}\n",
            self.calibration_ms, self.reference_ms, self.machine_factor, self.tolerance
        );
        for r in &self.rows {
            let b = &r.budget;
            let _ = writeln!(
                out,
                "{:<28} {:>12} {:>12}  {}",
                b.label(),
                r.measured
                    .map_or_else(|| "—".into(), |v| fmt_limit(v, &b.unit)),
                r.effective.map_or_else(
                    || format!("<{}", fmt_limit(b.limit, &b.unit)),
                    |e| format!("<{}", fmt_limit(e, &b.unit))
                ),
                match &r.status {
                    Status::Pass if r.attempts > 1 => format!("pass (attempt {})", r.attempts),
                    Status::Fail if r.attempts > 1 => {
                        format!("FAIL (all {} attempts)", r.attempts)
                    }
                    Status::Fail => "FAIL".to_owned(),
                    Status::Busy(share) => format!(
                        "INCONCLUSIVE (all {} attempts over, others used {:.0} % of the CPUs)",
                        r.attempts,
                        share * 100.0
                    ),
                    Status::Error(e) => format!("ERROR {e}"),
                    other => other.cell(),
                }
            );
        }
        out
    }
}

/// Replaces the generated block of `existing` (or inserts it before the first `## ` heading, or
/// appends it) with `block`.
#[must_use]
pub fn splice(existing: &str, block: &str) -> String {
    if let (Some(b), Some(e)) = (existing.find(BEGIN), existing.find(END))
        && b < e
    {
        let mut end = e + END.len();
        if existing[end..].starts_with('\n') {
            end += 1;
        }
        return format!("{}{block}{}", &existing[..b], &existing[end..]);
    }
    if let Some(i) = existing.find("\n## ") {
        return format!("{}{block}\n{}", &existing[..=i], &existing[i + 1..]);
    }
    let sep = if existing.is_empty() || existing.ends_with("\n\n") {
        ""
    } else if existing.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    format!("{existing}{sep}{block}")
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
version = 1
ci_tolerance = 1.5
calibration_reference_ms = 100.0
max_machine_factor = 3.0

[[budget]]
id = "B8"
scenario = "Full recalculation"
limit = 2000.0
unit = "ms"
bench = "recalc-full"

[[budget]]
id = "B3"
part = "first page painted"
scenario = "Open a 300-page document"
limit = 400.0
unit = "ms"
pending = "WP-62"

[[budget]]
id = "B4"
scenario = "Memory"
limit = 300.0
unit = "MB"
bench = "recalc-full"

[[budget]]
id = "B15"
scenario = "Clean build"
limit = 360000.0
unit = "ms"
manual = "see the B15 table below"

[[budget]]
id = "text-layout-1000"
scenario = "Layout"
limit = 60.0
unit = "ms"
bench = "text-layout-1000"
group = "component"
"#;

    fn sample(name: &str, p95: f64, rss: f64) -> Sample {
        let mut s =
            crate::harness::summarize(name, &[std::time::Duration::from_millis(1)], Some(rss));
        s.p95_ms = p95;
        s
    }

    #[test]
    fn parses_and_validates() {
        let f = BudgetFile::parse(FILE).unwrap();
        assert_eq!(f.budgets.len(), 5);
        assert_eq!(f.budgets[1].label(), "B3 (first page painted)");
        assert_eq!(f.budgets[2].metric_name(), "rss");
        assert_eq!(f.budgets[0].metric_name(), "p95");
        for (bad, why) in [
            (FILE.replace("pending = \"WP-62\"", ""), "exactly one"),
            (
                FILE.replace(
                    "bench = \"recalc-full\"\n\n[[budget]]\nid = \"B3\"",
                    "bench = \"nope\"\n\n[[budget]]\nid = \"B3\"",
                ),
                "unknown benchmark",
            ),
            (FILE.replace("unit = \"MB\"", "unit = \"GB\""), "unit"),
            (FILE.replace("id = \"B4\"", "id = \"B8\""), "twice"),
            (FILE.replace("version = 1", "version = 2"), "version"),
            (
                FILE.replace("ci_tolerance = 1.5", "ci_tolerance = 0.0"),
                "positive",
            ),
            (
                FILE.replace("group = \"component\"", "group = \"x\""),
                "group",
            ),
            (
                FILE.replace("limit = 60.0", "limit = 60.0\ncolour = 1"),
                "unknown field",
            ),
        ] {
            let err = BudgetFile::parse(&bad).unwrap_err();
            assert!(err.contains(why), "{why}: {err}");
        }
    }

    #[test]
    fn evaluates_with_scaling_and_statuses() {
        let f = BudgetFile::parse(FILE).unwrap();
        let mut runs = Vec::new();
        let out = evaluate_with(
            &f,
            AssertOptions {
                ci: true,
                quick: true,
            },
            150.0,
            |name, _| {
                runs.push(name.to_owned());
                match name {
                    "recalc-full" => Ok(sample(name, 2500.0, 280.0)),
                    _ => Err("broken".into()),
                }
            },
        );
        // Each benchmark runs once even when two rows read it.
        assert_eq!(runs, ["recalc-full", "text-layout-1000"]);
        assert!((out.machine_factor - 1.5).abs() < 1e-12);
        assert!((out.tolerance - 1.5).abs() < 1e-12);
        // 2000 × 1.5 × 1.5 = 4500 ≥ 2500.
        assert_eq!(out.rows[0].status, Status::Pass);
        assert_eq!(out.rows[0].effective, Some(4500.0));
        assert_eq!(out.rows[1].status, Status::Pending("WP-62".into()));
        // Memory is not scaled: 280 < 300.
        assert_eq!(out.rows[2].effective, Some(300.0));
        assert_eq!(out.rows[2].status, Status::Pass);
        assert!(matches!(out.rows[3].status, Status::Manual(_)));
        assert_eq!(out.rows[4].status, Status::Error("broken".into()));
        assert!(!out.passed());
        let md = out.to_markdown("2026-10-16");
        for id in [
            "B8",
            "B3 (first page painted)",
            "B4",
            "B15",
            "text-layout-1000",
        ] {
            assert!(md.contains(&format!("| {id} |")), "{id} missing:\n{md}");
        }
        assert!(md.contains("pending (WP-62)"));
        assert!(md.contains("### Component budgets"));
        assert!(md.contains("| `recalc-full` | 1 | 1 |"));
        assert!(out.to_text().contains("ERROR broken"));
        // Locally, without the tolerance, the same measurement fails B8.
        let local = evaluate_with(
            &f,
            AssertOptions {
                ci: false,
                quick: true,
            },
            100.0,
            |n, _| Ok(sample(n, 2500.0, 10.0)),
        );
        assert_eq!(local.rows[0].status, Status::Fail);
        assert_eq!(local.rows[4].status, Status::Fail);
        // A consistent miss is measured 1 + REMEASURES times and still fails.
        assert_eq!(local.rows[0].attempts, 1 + REMEASURES);
        assert_eq!(local.attempts["recalc-full"], 1 + REMEASURES);
        assert!(local.to_text().contains("FAIL (all 3 attempts)"));
    }

    #[test]
    fn one_slow_attempt_is_measured_again() {
        let f = BudgetFile::parse(FILE).unwrap();
        // recalc-full: an outlier first (busy machine), then a normal run. text-layout-1000:
        // always slow (a regression).
        let mut calls: BTreeMap<String, usize> = BTreeMap::new();
        let out = evaluate_with(
            &f,
            AssertOptions {
                ci: false,
                quick: true,
            },
            100.0,
            |name, _| {
                let n = calls.entry(name.to_owned()).or_default();
                *n += 1;
                match (name, *n) {
                    ("recalc-full", 1) => Ok(sample(name, 5100.0, 280.0)),
                    ("recalc-full", _) => Ok(sample(name, 760.0, 290.0)),
                    _ => Ok(sample(
                        name,
                        75.0 + f64::from(u32::try_from(*n).unwrap()),
                        9.0,
                    )),
                }
            },
        );
        assert_eq!(calls["recalc-full"], 2);
        assert_eq!(out.rows[0].status, Status::Pass);
        assert_eq!(out.rows[0].measured, Some(760.0));
        assert_eq!(out.rows[0].attempts, 2);
        // The memory row reads the same benchmark and its best attempt (280 MB).
        assert_eq!(out.rows[2].status, Status::Pass);
        assert_eq!(out.rows[2].measured, Some(280.0));
        // The regression is measured three times, fails every time and reports its best.
        assert_eq!(calls["text-layout-1000"], 1 + REMEASURES);
        assert_eq!(out.rows[4].status, Status::Fail);
        assert_eq!(out.rows[4].measured, Some(76.0));
        assert!(!out.passed());
        assert_eq!(out.samples["recalc-full"].p95_ms, 760.0);
        let md = out.to_markdown("2026-10-16");
        assert!(md.contains("760 ms (p95, best of 2 attempts)"), "{md}");
        assert!(md.contains("Result: **FAIL**"), "{md}");
        assert!(md.contains("| `recalc-full` | 2 | 1 |"), "{md}");
        assert!(out.to_text().contains("pass (attempt 2)"));
        // Every attempt over, every attempt on a busy machine: inconclusive, not a failure. One
        // quiet attempt over the limit is enough for a verdict.
        for (contention, expected) in [
            ([0.6, 0.4, 0.3], Status::Busy(0.3)),
            ([0.6, 0.1, 0.9], Status::Fail),
        ] {
            let mut n = 0;
            let busy = evaluate_with(
                &f,
                AssertOptions {
                    ci: false,
                    quick: true,
                },
                100.0,
                |name, _| {
                    let mut s = sample(name, 9000.0, 1.0);
                    if name == "recalc-full" {
                        s.contention = Some(contention[n]);
                        n += 1;
                    }
                    Ok(s)
                },
            );
            assert_eq!(busy.rows[0].status, expected);
            // The memory row reads the same attempts and is judged normally.
            assert_eq!(busy.rows[2].status, Status::Pass);
            // Without a contention figure (text-layout-1000 here) a miss is a failure.
            assert_eq!(busy.rows[4].status, Status::Fail);
            assert!(!busy.passed());
        }
        let mut n = 0;
        let only_busy = evaluate_with(
            &BudgetFile::parse(
                &FILE.replace("bench = \"text-layout-1000\"", "pending = \"WP-53\""),
            )
            .unwrap(),
            AssertOptions {
                ci: true,
                quick: true,
            },
            100.0,
            |name, _| {
                n += 1;
                let mut s = sample(name, 9000.0, 1.0);
                s.contention = Some(0.5);
                Ok(s)
            },
        );
        assert_eq!(n, 1 + REMEASURES);
        assert_eq!(only_busy.rows[0].status, Status::Busy(0.5));
        assert!(only_busy.passed());
        assert_eq!(only_busy.verdict(), "pass, 1 inconclusive");
        let md = only_busy.to_markdown("2026-10-16");
        assert!(
            md.contains("inconclusive (other processes kept 50 % of the CPUs busy)"),
            "{md}"
        );
        assert!(md.contains("| 50 % |"), "{md}");
        assert!(
            only_busy
                .to_text()
                .contains("INCONCLUSIVE (all 3 attempts over")
        );
        // A benchmark that errors is not measured again.
        let mut errors = 0;
        let broken = evaluate_with(
            &f,
            AssertOptions {
                ci: false,
                quick: true,
            },
            100.0,
            |_, _| {
                errors += 1;
                Err("boom".to_owned())
            },
        );
        assert_eq!(errors, 2);
        assert_eq!(broken.rows[0].status, Status::Error("boom".into()));
    }

    #[test]
    fn splices_the_generated_block() {
        let block = format!("{BEGIN}\nnew\n{END}\n");
        let doc = format!("# Perf\n\nIntro.\n\n{BEGIN}\nold\n{END}\n\n## B15\n\nmanual\n");
        let out = splice(&doc, &block);
        assert_eq!(
            out,
            format!("# Perf\n\nIntro.\n\n{BEGIN}\nnew\n{END}\n\n## B15\n\nmanual\n")
        );
        let fresh = splice("# Perf\n\nIntro.\n\n## B15\n\nmanual\n", &block);
        assert_eq!(
            fresh,
            format!("# Perf\n\nIntro.\n\n{BEGIN}\nnew\n{END}\n\n## B15\n\nmanual\n")
        );
        assert_eq!(splice("", &block), block);
        assert_eq!(splice("# T", &block), format!("# T\n\n{block}"));
    }

    #[test]
    fn formats_dates() {
        assert_eq!(utc_date(0), "1970-01-01");
        assert_eq!(utc_date(951_782_400), "2000-02-29");
        assert_eq!(utc_date(1_792_108_800), "2026-10-16");
    }

    #[test]
    fn the_repository_file_is_valid_and_lists_the_plan_ids() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../bench/budgets.toml");
        let f = BudgetFile::load(&path).unwrap();
        for id in ["B3", "B7", "B8", "B9", "B10", "B11", "B14", "B15"] {
            assert!(f.budgets.iter().any(|b| b.id == id), "{id} missing");
        }
        for name in [
            "text-layout-1000",
            "scratch-frame-production",
            "ooxml-repack-large",
        ] {
            assert!(
                f.budgets.iter().any(|b| b.bench.as_deref() == Some(name)),
                "{name} has no budget"
            );
        }
    }
}
