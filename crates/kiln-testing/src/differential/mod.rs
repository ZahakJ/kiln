// SPDX-License-Identifier: AGPL-3.0-or-later
//! Differential testing: other tools' round trips, scored by the fidelity scorer next to Kiln's.
//!
//! Every tool takes the same input file. Its output goes through the same comparison as Kiln's:
//! Kiln's reader for `structural`, `text`, `values` and `formulas`, and a LibreOffice render at
//! the same resolution for `visual` and `oracle_opens`. `byte_identical_untouched` is left out:
//! the other writers regenerate a package whole.
//!
//! A tool that fails on a file loses it, Kiln included. Kiln counts as at least as good as a tool
//! on a file when it is at least as good on every metric either side measured
//! ([`at_least_as_good`]); the file's other metrics cannot make up for one it lost.

mod report;
pub mod tools;

pub use report::{Differential, FileRow, keep_section, replace_section};
pub use tools::Tool;

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::fidelity::{
    self, Metric, Metrics, ModelScore, Options, Roundtrip, oracle_metrics, render_original,
    score_model,
};
use crate::fixtures::Fixture;
use crate::oracle::{Oracle, run_child};

/// The metrics compared between tools.
pub const COMPARED: [Metric; 6] = [
    Metric::Structural,
    Metric::Text,
    Metric::Visual,
    Metric::Values,
    Metric::Formulas,
    Metric::RawElements,
];

/// One tool's result on one file.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The scores.
    Scored(Metrics),
    /// Why the tool produced nothing that could be scored.
    Failed(String),
}

/// Whether Kiln is at least as good as a tool on a file.
///
/// A failed Kiln loses, whatever the tool did. A failed tool loses to a Kiln that produced a
/// result. Otherwise Kiln must not be below the tool on any metric, and a metric only the tool
/// measured counts against Kiln.
#[must_use]
pub fn at_least_as_good(kiln: &Outcome, tool: &Outcome) -> bool {
    let Outcome::Scored(k) = kiln else {
        return false;
    };
    let Outcome::Scored(t) = tool else {
        return true;
    };
    COMPARED.iter().all(|m| match (k.get(*m), t.get(*m)) {
        (Some(a), Some(b)) => a + 1e-12 >= b,
        (None, Some(_)) => false,
        _ => true,
    })
}

/// The largest lead of a tool over Kiln on any metric of a file (0 when it leads on none).
#[must_use]
pub fn tool_lead(kiln: &Outcome, tool: &Outcome) -> (f64, Option<Metric>) {
    let (Outcome::Scored(k), Outcome::Scored(t)) = (kiln, tool) else {
        return (0.0, None);
    };
    let mut lead = (0.0, None);
    for m in COMPARED {
        if let (Some(a), Some(b)) = (k.get(m), t.get(m))
            && b - a > lead.0
        {
            lead = (b - a, Some(m));
        }
    }
    lead
}

fn outcome(score: ModelScore, metrics_after: Metrics) -> Outcome {
    if score.failures.is_empty() {
        Outcome::Scored(metrics_after)
    } else {
        Outcome::Failed(score.failures.join("; "))
    }
}

/// How many pages a PDF has.
fn pdf_pages(pdf: &Path) -> Option<usize> {
    let mut cmd = Command::new("pdfinfo");
    cmd.arg(pdf);
    let done = run_child(cmd, Duration::from_mins(1)).ok()?;
    done.stdout
        .lines()
        .find_map(|l| l.strip_prefix("Pages:"))
        .and_then(|n| n.trim().parse().ok())
}

/// Kiln's own page count: `kiln-cli convert <file> out.pdf`.
fn kiln_pages(options: &Options, input: &Path, dir: &Path) -> Option<usize> {
    let how = options.isolation.as_ref()?;
    let ext = input.extension()?.to_str()?;
    let copy = dir.join(format!("pages-in.{ext}"));
    std::fs::copy(input, &copy).ok()?;
    let pdf = dir.join("kiln-pages.pdf");
    let mut cmd = Command::new(&how.exe);
    cmd.arg("convert").arg(&copy).arg(&pdf);
    if ext == "xlsx" {
        // LibreOffice prints every sheet.
        cmd.arg("--all-sheets");
    }
    let done = run_child(cmd, how.timeout).ok()?;
    if !done.success {
        return None;
    }
    pdf_pages(&pdf)
}

/// A fontconfig file that adds Kiln's bundled fonts to the system's, so LibreOffice lays a file
/// out with the same metric-compatible fonts as Kiln.
fn write_fontconfig(scratch: &Path) -> Result<std::path::PathBuf, String> {
    let root = crate::repo_root();
    let file = scratch.join("fonts.conf");
    let xml = format!(
        "<?xml version=\"1.0\"?><!DOCTYPE fontconfig SYSTEM \"fonts.dtd\"><fontconfig>\
<include ignore_missing=\"yes\">/etc/fonts/fonts.conf</include>\
<dir>{}</dir><dir>{}</dir><cachedir>{}</cachedir></fontconfig>",
        root.join("packages/tokens/fonts").display(),
        root.join("crates/kiln-text/tests/fonts").display(),
        scratch.join("fontconfig-cache").display(),
    );
    std::fs::create_dir_all(scratch).map_err(|e| e.to_string())?;
    std::fs::write(&file, xml).map_err(|e| e.to_string())?;
    Ok(file)
}

/// LibreOffice's page count of an original, with Kiln's fonts.
fn libreoffice_pages(oracle: &Oracle, fixture: &Fixture, conf: &Path, dir: &Path) -> Option<usize> {
    let profile = dir
        .parent()?
        .join(format!("profile-fonts-{:?}", std::thread::current().id()));
    let oracle = oracle.with_fontconfig(conf.to_path_buf(), profile.clone());
    let pdf = oracle.convert(&fixture.path, "pdf", &dir.join("fonts"));
    pdf.ok().and_then(|p| pdf_pages(&p))
}

fn score_tool(
    tool: &Tool,
    adapter: &dyn Roundtrip,
    fixture: &Fixture,
    original: &Result<Vec<crate::image::Rgba>, String>,
    oracle: &Oracle,
    options: &Options,
    dir: &Path,
) -> Outcome {
    let tool_dir = dir.join(tool.id());
    let written = match tool.round_trip(fixture, oracle, &tool_dir) {
        Ok(p) => p,
        Err(e) => return Outcome::Failed(e),
    };
    let scored = match &options.isolation {
        Some(how) => {
            fidelity::isolated(how, fixture.format, &fixture.path, Some(&written), &written)
        }
        None => match (std::fs::read(&fixture.path), std::fs::read(&written)) {
            (Ok(a), Ok(b)) => fidelity::score_written(adapter, &a, b, None),
            (Err(e), _) | (_, Err(e)) => return Outcome::Failed(e.to_string()),
        },
    };
    let mut metrics = scored.metrics.clone();
    let mut notes = Vec::new();
    oracle_metrics(
        oracle,
        fixture,
        &written,
        original,
        &tool_dir,
        options.dpi,
        &mut metrics,
        &mut notes,
    );
    outcome(scored, metrics)
}

fn score_kiln(
    adapter: &dyn Roundtrip,
    fixture: &Fixture,
    original: &Result<Vec<crate::image::Rgba>, String>,
    oracle: &Oracle,
    options: &Options,
    dir: &Path,
) -> Outcome {
    let written = dir.join(format!("kiln.{}", fidelity::extension(fixture)));
    let scored = match &options.isolation {
        Some(how) => fidelity::isolated(how, fixture.format, &fixture.path, None, &written),
        None => match fixture.read() {
            Ok(b) => score_model(adapter, &b),
            Err(e) => return Outcome::Failed(e),
        },
    };
    let Some(output) = scored.output.as_ref() else {
        return outcome(scored, Metrics::default());
    };
    if let Err(e) = std::fs::write(&written, output) {
        return Outcome::Failed(e.to_string());
    }
    let mut metrics = scored.metrics.clone();
    let mut notes = Vec::new();
    oracle_metrics(
        oracle,
        fixture,
        &written,
        original,
        dir,
        options.dpi,
        &mut metrics,
        &mut notes,
    );
    outcome(scored, metrics)
}

/// Scores Kiln and every tool over `fixtures`.
///
/// # Errors
///
/// When LibreOffice, which renders every comparison, is not installed.
pub fn run(
    adapters: &[Box<dyn Roundtrip>],
    fixtures: &[Fixture],
    tools: &[Tool],
    options: &Options,
) -> Result<Differential, String> {
    let oracle = Oracle::locate().map_err(|e| e.to_string())?;
    let conf = write_fontconfig(&options.scratch)?;
    let mut work: Vec<(&dyn Roundtrip, &Fixture)> = Vec::new();
    for f in fixtures.iter().filter(|f| f.format.is_opc()) {
        if let Some(a) = adapters.iter().find(|a| a.format() == f.format) {
            work.push((a.as_ref(), f));
        }
    }
    let rows = fidelity::for_each_file(&work, options, Some(&oracle), &|(adapter, f), oracle| {
        let oracle = oracle.expect("the oracle was located above");
        let dir = options.scratch.join(f.rel.replace('/', "__"));
        let _ = std::fs::create_dir_all(&dir);
        let original = render_original(oracle, f, &dir, options.dpi);
        let lo_pages = libreoffice_pages(oracle, f, &conf, &dir);
        let kiln = score_kiln(*adapter, f, &original, oracle, options, &dir);
        let others: Vec<(String, Outcome)> = tools
            .iter()
            .map(|t| {
                (
                    t.id().to_owned(),
                    score_tool(t, *adapter, f, &original, oracle, options, &dir),
                )
            })
            .collect();
        let pages = kiln_pages(options, &f.path, &dir);
        let _ = std::fs::remove_dir_all(&dir);
        let row = FileRow {
            fixture: (*f).clone(),
            kiln,
            others,
            kiln_pages: pages,
            libreoffice_pages: lo_pages,
        };
        let line = format!("{} {}", f.rel, row.summary());
        (row, line)
    });
    Ok(Differential {
        tools: tools.to_vec(),
        rows,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scored(s: f64, v: f64) -> Outcome {
        let mut m = Metrics::default();
        m.set(Metric::Structural, s);
        m.set(Metric::Text, 1.0);
        m.set(Metric::Visual, v);
        Outcome::Scored(m)
    }

    #[test]
    fn kiln_must_hold_every_metric() {
        assert!(at_least_as_good(&scored(1.0, 0.9), &scored(0.9, 0.9)));
        assert!(!at_least_as_good(&scored(1.0, 0.9), &scored(0.5, 0.91)));
    }

    #[test]
    fn a_failure_loses_for_whoever_failed_and_kiln_loses_a_double_failure() {
        let failed = Outcome::Failed("no".into());
        assert!(at_least_as_good(&scored(0.1, 0.1), &failed));
        assert!(!at_least_as_good(&failed, &scored(0.1, 0.1)));
        assert!(!at_least_as_good(&failed, &failed));
    }

    #[test]
    fn a_metric_only_the_tool_measured_counts_against_kiln() {
        let mut k = Metrics::default();
        k.set(Metric::Structural, 1.0);
        let mut t = k.clone();
        t.set(Metric::Visual, 0.1);
        assert!(!at_least_as_good(
            &Outcome::Scored(k.clone()),
            &Outcome::Scored(t.clone())
        ));
        assert!(at_least_as_good(&Outcome::Scored(t), &Outcome::Scored(k)));
    }

    #[test]
    fn the_lead_is_the_largest_gap() {
        let (lead, m) = tool_lead(&scored(0.9, 0.5), &scored(0.95, 0.8));
        assert!((lead - 0.3).abs() < 1e-9);
        assert_eq!(m, Some(Metric::Visual));
        assert_eq!(tool_lead(&scored(1.0, 1.0), &scored(0.9, 0.9)).0, 0.0);
    }
}
