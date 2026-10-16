// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fidelity report and its Markdown form (`docs/qa/fidelity.md`).
//!
//! The summary table follows the contract `cargo xtask parity` reads for `fidelity:` verify ids
//! (parity/README.md): a `Format` column and one column per metric, one row per format, holding the
//! lowest value any file of the format scored. No other table has a `Format` column.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use super::{BaselineResult, FileResult, Metric};
use crate::fixtures::{FixtureFormat, FixtureKind};

/// The outcome of a run.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// Registered adapters that took part, by format.
    pub adapters: Vec<(FixtureFormat, &'static str)>,
    /// Formats selected for the run.
    pub formats: Vec<FixtureFormat>,
    /// Per-file scores.
    pub files: Vec<FileResult>,
    /// The OPC passthrough baseline.
    pub baseline: Vec<BaselineResult>,
    /// The LibreOffice executable used, or why the oracle metrics were skipped.
    pub oracle: Result<String, String>,
}

/// One row of the summary.
#[derive(Debug, Clone, PartialEq)]
pub struct FormatSummary {
    /// The format.
    pub format: FixtureFormat,
    /// Its adapter.
    pub adapter: &'static str,
    /// Files scored.
    pub files: usize,
    /// Files meeting every threshold.
    pub passing: usize,
    /// Lowest value per measured metric.
    pub lowest: BTreeMap<Metric, f64>,
}

pub(super) fn fmt_value(metric: Metric, v: f64) -> String {
    if metric.is_flag() {
        if v >= 1.0 { "yes" } else { "no" }.to_owned()
    } else {
        format!("{v:.3}")
    }
}

fn cell(v: Option<f64>, metric: Metric) -> String {
    v.map_or_else(|| "—".to_owned(), |v| fmt_value(metric, v))
}

fn escape(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

impl Report {
    /// Whether no format adapter is registered (for the selected formats).
    #[must_use]
    pub fn no_formats_registered(&self) -> bool {
        self.adapters.is_empty()
    }

    /// The per-format summary, in format order.
    #[must_use]
    pub fn summary(&self) -> Vec<FormatSummary> {
        self.adapters
            .iter()
            .map(|&(format, adapter)| {
                let files: Vec<&FileResult> = self
                    .files
                    .iter()
                    .filter(|f| f.fixture.format == format && f.adapter == adapter)
                    .collect();
                let mut lowest: BTreeMap<Metric, f64> = BTreeMap::new();
                for f in &files {
                    for (m, v) in f.metrics.iter() {
                        let e = lowest.entry(m).or_insert(v);
                        *e = e.min(v);
                    }
                }
                FormatSummary {
                    format,
                    adapter,
                    files: files.len(),
                    passing: files.iter().filter(|f| f.passed()).count(),
                    lowest,
                }
            })
            .collect()
    }

    /// Files that missed a threshold or could not be scored.
    #[must_use]
    pub fn failed_files(&self) -> Vec<&FileResult> {
        self.files.iter().filter(|f| !f.passed()).collect()
    }

    /// Baseline packages that did not repack byte-identically (or not at all).
    #[must_use]
    pub fn failed_baseline(&self) -> Vec<&BaselineResult> {
        self.baseline
            .iter()
            .filter(|b| {
                !b.outcome
                    .as_ref()
                    .is_ok_and(super::package::PartComparison::all_identical)
            })
            .collect()
    }

    /// Whether every scored file met its thresholds and every package repacked byte-identically.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.failed_files().is_empty() && self.failed_baseline().is_empty()
    }

    /// A short plain-text summary for the terminal.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        if self.no_formats_registered() {
            out.push_str("no formats registered: no Roundtrip adapter in crates/kiln-testing/src/formats/mod.rs yet\n");
        }
        for s in self.summary() {
            let _ = write!(
                out,
                "{} ({}): {}/{} files pass",
                s.format, s.adapter, s.passing, s.files
            );
            for (m, v) in &s.lowest {
                let _ = write!(out, ", {m} {}", fmt_value(*m, *v));
            }
            out.push('\n');
        }
        for f in self.failed_files() {
            let _ = writeln!(out, "FAIL {}: {}", f.fixture.rel, f.failures.join("; "));
        }
        let parts: usize = self
            .baseline
            .iter()
            .filter_map(|b| b.outcome.as_ref().ok())
            .map(|c| c.parts)
            .sum();
        let identical: usize = self
            .baseline
            .iter()
            .filter_map(|b| b.outcome.as_ref().ok())
            .map(|c| c.identical)
            .sum();
        let _ = writeln!(
            out,
            "passthrough baseline: {} packages, identical parts: {identical}/{parts}, same declared content types: {}/{}",
            self.baseline.len(),
            self.baseline
                .iter()
                .filter(|b| b
                    .outcome
                    .as_ref()
                    .is_ok_and(|c| c.content_types_equal == Some(true)))
                .count(),
            self.baseline.len()
        );
        for b in self.failed_baseline() {
            let why = match &b.outcome {
                Ok(c) => format!(
                    "{} differing, {} missing, {} added",
                    c.differing.len(),
                    c.missing.len(),
                    c.added.len()
                ),
                Err(e) => e.clone(),
            };
            let _ = writeln!(out, "FAIL baseline {}: {why}", b.fixture.rel);
        }
        match &self.oracle {
            Ok(path) => {
                let _ = writeln!(out, "oracle: {path}");
            }
            Err(why) => {
                let _ = writeln!(
                    out,
                    "oracle: not used ({why}); visual and oracle_opens not measured"
                );
            }
        }
        out
    }

    /// The Markdown report written to `docs/qa/fidelity.md`.
    #[must_use]
    pub fn to_markdown(&self) -> String {
        let mut md = String::from("# Round-trip fidelity\n\n");
        md.push_str(
            "Written by `cargo xtask fidelity` (`kiln-cli fidelity`, scorer in `crates/kiln-testing/src/fidelity/`). \
Each file is read, written back without edits and read again; the metrics and thresholds are those of \
PLAN.md §3.10 and §5, per file from `parity/corpus.json` (the §3.10 release targets for files it does not list). \
Do not edit by hand: rerun the command.\n\n",
        );
        let formats: Vec<String> = self.formats.iter().map(ToString::to_string).collect();
        let _ = writeln!(md, "- Formats: {}", formats.join(", "));
        match &self.oracle {
            Ok(p) => {
                let _ = writeln!(
                    md,
                    "- Oracle: LibreOffice (`{p}`) through `tools/oracle-soffice.sh`; pages rendered by `pdftoppm` at 60 dpi"
                );
            }
            Err(why) => {
                let _ = writeln!(
                    md,
                    "- Oracle: not used ({why}), so `visual` and `oracle_opens` are not measured (—)"
                );
            }
        }
        let _ = writeln!(
            md,
            "- Result: **{}**\n",
            if self.passed() { "pass" } else { "FAIL" }
        );

        md.push_str("## Summary\n\n");
        if self.no_formats_registered() {
            md.push_str(
                "No formats registered: no reader/writer has registered a `Roundtrip` adapter in \
`crates/kiln-testing/src/formats/mod.rs` yet. The `.docx` adapter arrives with WP-62/WP-65, `.xlsx` with WP-70 and \
`.pptx` with WP-77/WP-79; until then only the OPC passthrough baseline below is measured.\n\n",
            );
        } else {
            md.push_str("Lowest value of any file of the format (— = not measured).\n\n| Format | Adapter | Files | Passing |");
            for m in Metric::ALL {
                let _ = write!(md, " {m} |");
            }
            md.push_str("\n|---|---|---|---|");
            md.push_str(&"---|".repeat(Metric::ALL.len()));
            md.push('\n');
            for s in self.summary() {
                let _ = write!(
                    md,
                    "| {} | {} | {} | {} |",
                    s.format, s.adapter, s.files, s.passing
                );
                for m in Metric::ALL {
                    let _ = write!(md, " {} |", cell(s.lowest.get(&m).copied(), m));
                }
                md.push('\n');
            }
            md.push('\n');

            md.push_str("## Files\n\n| File | Result |");
            for m in Metric::ALL {
                let _ = write!(md, " {m} |");
            }
            md.push_str(" Notes |\n|---|---|");
            md.push_str(&"---|".repeat(Metric::ALL.len()));
            md.push_str("---|\n");
            for f in &self.files {
                let _ = write!(
                    md,
                    "| `{}` | {} |",
                    f.fixture.rel,
                    if f.passed() { "pass" } else { "**fail**" }
                );
                for m in Metric::ALL {
                    let _ = write!(md, " {} |", cell(f.metrics.get(m), m));
                }
                let notes: Vec<String> = f
                    .failures
                    .iter()
                    .chain(&f.notes)
                    .map(|n| escape(n))
                    .collect();
                let _ = writeln!(md, " {} |", notes.join("; "));
            }
            md.push('\n');
        }

        md.push_str(
            "## OPC passthrough baseline\n\nEvery package opened and saved by `kiln-ooxml` without edits \
(what `kiln-cli repack` does): every part, `.rels` parts included, must come out byte-identical \
(PLAN.md §3.1 principle 4), and `[Content_Types].xml` must declare the same content types (it is parsed on open, so it \
is compared by its declarations and not counted among the parts below).\n\n| Fixtures | Packages | Parts | Identical parts | Result |\n|---|---|---|---|---|\n",
        );
        let mut groups: BTreeMap<(FixtureFormat, FixtureKind), (usize, usize, usize, bool)> =
            BTreeMap::new();
        for b in &self.baseline {
            let g = groups
                .entry((b.fixture.format, b.fixture.kind))
                .or_insert((0, 0, 0, true));
            g.0 += 1;
            match &b.outcome {
                Ok(c) => {
                    g.1 += c.parts;
                    g.2 += c.identical;
                    g.3 &= c.all_identical();
                }
                Err(_) => g.3 = false,
            }
        }
        for ((format, kind), (n, parts, identical, ok)) in &groups {
            let _ = writeln!(
                md,
                "| `{}` | {n} | {parts} | {identical} | {} |",
                crate::fixtures::kind_dir(*format, *kind),
                if *ok { "pass" } else { "**fail**" }
            );
        }
        let failed = self.failed_baseline();
        if !failed.is_empty() {
            md.push_str("\nPackages that did not repack byte-identically:\n\n");
            for b in failed {
                let why = match &b.outcome {
                    Ok(c) => format!(
                        "differing: {}; missing: {}; added: {}",
                        c.differing.join(", "),
                        c.missing.join(", "),
                        c.added.join(", ")
                    ),
                    Err(e) => e.clone(),
                };
                let _ = writeln!(md, "- `{}`: {}", b.fixture.rel, escape(&why));
            }
        }
        md
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Metrics, package::PartComparison};
    use super::*;
    use crate::fixtures::Fixture;

    fn fixture(rel: &str, format: FixtureFormat) -> Fixture {
        Fixture {
            rel: rel.into(),
            path: rel.into(),
            format,
            kind: FixtureKind::Features,
        }
    }

    fn report() -> Report {
        let mut good = Metrics::default();
        good.set(Metric::Text, 1.0);
        good.set(Metric::Structural, 0.99);
        good.set_flag(Metric::ByteIdenticalUntouched, true);
        let mut bad = good.clone();
        bad.set(Metric::Structural, 0.9);
        Report {
            adapters: vec![(FixtureFormat::Docx, "kiln-io-docx")],
            formats: vec![FixtureFormat::Docx],
            files: vec![
                FileResult {
                    fixture: fixture("docx/features/a.docx", FixtureFormat::Docx),
                    adapter: "kiln-io-docx",
                    metrics: good,
                    failures: vec![],
                    notes: vec![],
                },
                FileResult {
                    fixture: fixture("docx/features/b.docx", FixtureFormat::Docx),
                    adapter: "kiln-io-docx",
                    metrics: bad,
                    failures: vec!["structural 0.900 < 0.95".into()],
                    notes: vec!["a|b".into()],
                },
            ],
            baseline: vec![BaselineResult {
                fixture: fixture("docx/features/a.docx", FixtureFormat::Docx),
                outcome: Ok(PartComparison {
                    parts: 10,
                    identical: 10,
                    ..PartComparison::default()
                }),
            }],
            oracle: Err("LibreOffice (soffice) is not installed".into()),
        }
    }

    #[test]
    fn summary_reports_the_lowest_value_per_format() {
        let r = report();
        let s = r.summary();
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].files, s[0].passing), (2, 1));
        assert_eq!(s[0].lowest.get(&Metric::Structural), Some(&0.9));
        assert!(!r.passed());
        let md = r.to_markdown();
        assert!(md.contains("| Format | Adapter | Files | Passing | structural |"));
        assert!(
            md.contains("| docx | kiln-io-docx | 2 | 1 | 0.900 | 1.000 | — | — | — | — | yes |")
        );
        assert!(md.contains("a\\|b"));
        assert!(md.contains("| `docx/features` | 1 | 10 | 10 | pass |"));
        // Only the summary table has a Format column (the parity contract).
        assert_eq!(md.matches("| Format |").count(), 1);
        assert!(r.to_text().contains("FAIL docx/features/b.docx"));
    }

    #[test]
    fn an_empty_registry_is_reported_and_passes_on_the_baseline() {
        let mut r = report();
        r.adapters.clear();
        r.files.clear();
        assert!(r.no_formats_registered());
        assert!(r.passed());
        let md = r.to_markdown();
        assert!(md.contains("No formats registered"));
        assert!(!md.contains("| Format |"));
        assert!(r.to_text().starts_with("no formats registered"));
        r.baseline[0].outcome = Err("broken".into());
        assert!(!r.passed());
        assert!(r.to_markdown().contains("- `docx/features/a.docx`: broken"));
    }
}
