// SPDX-License-Identifier: AGPL-3.0-or-later
//! The round-trip fidelity scorer.
//!
//! A format crate makes itself measurable by implementing [`Roundtrip`] in an adapter module of
//! [`crate::formats`] and adding one line to [`crate::formats::registered`]. For every fixture of
//! that format the scorer then:
//!
//! 1. reads the original into a [`Snapshot`] (canonical model facts, plain text, cells),
//! 2. round-trips it (read → write, no edit) and reads the output back,
//! 3. scores the pair:
//!
//! | Metric | Meaning |
//! |---|---|
//! | `structural` | multiset overlap of the model facts before and after (1 = nothing lost or added) |
//! | `text` | 1 when the normalised plain text is equal, otherwise the word-multiset overlap |
//! | `values` | share of cells (union of both sides) with equal stored values, read from the file's XML, not recalculated (workbooks; ODF: Kiln's reader) |
//! | `formulas` | share of formula cells with equal normalised formulas (spreadsheets) |
//! | `byte_identical_untouched` | every part the writer did not regenerate is byte-identical |
//! | `oracle_opens` | LibreOffice converts the output to PDF (when the oracle is installed) |
//! | `visual` | page-by-page `1 − differing pixels` of LibreOffice renders of input and output at 60 dpi |
//! | `raw_elements` | for each kind of element in the original's raw XML, the share still there in the output, averaged (packages); see [`stored::raw_elements`] |
//!
//! Thresholds come from `parity/corpus.json` (or [`default_checks`] for files it does not list).
//! Independently of any adapter, every package also goes through the OPC passthrough baseline
//! ([`package::repack`]): open and save without edits must keep every part byte-identical.

pub mod package;
mod report;
pub mod stamp;
pub mod stored;

pub use report::{FormatSummary, Report};

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use crate::fixtures::{CorpusChecks, Fixture, FixtureFormat};
use crate::oracle::Oracle;

/// A fidelity metric.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Metric {
    /// Model facts preserved.
    Structural,
    /// Plain text preserved.
    Text,
    /// Rendering preserved (LibreOffice renders compared).
    Visual,
    /// Cell values preserved.
    Values,
    /// Formulas preserved.
    Formulas,
    /// LibreOffice opens the output.
    OracleOpens,
    /// Parts the writer did not regenerate are byte-identical.
    ByteIdenticalUntouched,
    /// The original's markup still in the output, from the raw XML.
    RawElements,
}

impl Metric {
    /// Every metric, in report column order.
    pub const ALL: [Metric; 8] = [
        Self::Structural,
        Self::Text,
        Self::Visual,
        Self::Values,
        Self::Formulas,
        Self::OracleOpens,
        Self::ByteIdenticalUntouched,
        Self::RawElements,
    ];

    /// The name used in `parity/corpus.json`, `docs/qa/fidelity.md` and verify ids.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Structural => "structural",
            Self::Text => "text",
            Self::Visual => "visual",
            Self::Values => "values",
            Self::Formulas => "formulas",
            Self::OracleOpens => "oracle_opens",
            Self::ByteIdenticalUntouched => "byte_identical_untouched",
            Self::RawElements => "raw_elements",
        }
    }

    /// Whether the metric is yes/no (reported as 1 or 0).
    #[must_use]
    pub const fn is_flag(self) -> bool {
        matches!(self, Self::OracleOpens | Self::ByteIdenticalUntouched)
    }
}

impl fmt::Display for Metric {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// Measured metrics of one file (absent = not measured).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Metrics(BTreeMap<Metric, f64>);

impl Metrics {
    /// Records a ratio (clamped to 0..=1; NaN counts as 0).
    pub fn set(&mut self, metric: Metric, value: f64) {
        let v = if value.is_nan() {
            0.0
        } else {
            value.clamp(0.0, 1.0)
        };
        self.0.insert(metric, v);
    }

    /// Records a yes/no metric.
    pub fn set_flag(&mut self, metric: Metric, ok: bool) {
        self.0.insert(metric, if ok { 1.0 } else { 0.0 });
    }

    /// A measured value.
    #[must_use]
    pub fn get(&self, metric: Metric) -> Option<f64> {
        self.0.get(&metric).copied()
    }

    /// Every measured value.
    pub fn iter(&self) -> impl Iterator<Item = (Metric, f64)> + '_ {
        self.0.iter().map(|(m, v)| (*m, *v))
    }
}

/// One spreadsheet cell as the scorer compares it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellFact {
    /// The value as canonical text (numbers in shortest round-trip form, `TRUE`, `#N/A`, text).
    pub value: String,
    /// The formula text, if the cell has one.
    pub formula: Option<String>,
}

/// What an adapter reads out of a file for comparison.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Snapshot {
    /// Canonical facts about the model, one string per node or attribute, for example
    /// `body/p[3]/r[1]@b=1`. Order does not matter; duplicates count.
    pub facts: Vec<String>,
    /// The document's plain text.
    pub text: String,
    /// Spreadsheet cells keyed by `Sheet!A1` (empty for other formats).
    pub cells: BTreeMap<String, CellFact>,
}

/// The output of a round trip.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoundtripOutput {
    /// The written file.
    pub bytes: Vec<u8>,
    /// Part names the writer regenerated from the model (`/word/document.xml`, and
    /// `/[Content_Types].xml` or `.rels` parts when it rewrites them); every other part must come
    /// out byte-identical.
    pub regenerated: Vec<String>,
}

/// A format's reader and writer, as the fidelity scorer drives them. Implemented by the adapters
/// in [`crate::formats`].
pub trait Roundtrip: Send + Sync {
    /// The format.
    fn format(&self) -> FixtureFormat;

    /// The implementation's name for reports (`kiln-io-docx`).
    fn name(&self) -> &'static str;

    /// Reads a file into comparable facts.
    ///
    /// # Errors
    ///
    /// When the reader rejects the file.
    fn snapshot(&self, bytes: &[u8]) -> Result<Snapshot, String>;

    /// Reads and writes a file without editing it.
    ///
    /// # Errors
    ///
    /// When reading or writing fails.
    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String>;
}

/// The release thresholds for files `parity/corpus.json` does not list.
#[must_use]
pub fn default_checks(format: FixtureFormat) -> CorpusChecks {
    let base = CorpusChecks {
        oracle_opens: Some(true),
        byte_identical_untouched: Some(true),
        ..CorpusChecks::default()
    };
    match format {
        FixtureFormat::Docx => CorpusChecks {
            structural: Some(0.95),
            text: Some(1.0),
            visual: Some(0.9),
            ..base
        },
        FixtureFormat::Xlsx => CorpusChecks {
            values: Some(1.0),
            formulas: Some(1.0),
            ..base
        },
        FixtureFormat::Pptx => CorpusChecks {
            structural: Some(0.95),
            visual: Some(0.85),
            ..base
        },
        // ODF packages are rewritten whole: there are no untouched parts to compare.
        FixtureFormat::Odf => CorpusChecks {
            structural: Some(0.90),
            text: Some(1.0),
            visual: Some(0.80),
            byte_identical_untouched: None,
            ..base
        },
    }
}

fn multiset<'a>(items: impl IntoIterator<Item = &'a str>) -> HashMap<&'a str, usize> {
    let mut m = HashMap::new();
    for i in items {
        *m.entry(i).or_insert(0) += 1;
    }
    m
}

/// Multiset overlap: `|a ∩ b| / max(|a|, |b|)`, 1 when both are empty.
#[must_use]
pub fn overlap<'a>(
    a: impl IntoIterator<Item = &'a str>,
    b: impl IntoIterator<Item = &'a str>,
) -> f64 {
    let (ma, mb) = (multiset(a), multiset(b));
    let (na, nb): (usize, usize) = (ma.values().sum(), mb.values().sum());
    let denom = na.max(nb);
    if denom == 0 {
        return 1.0;
    }
    let common: usize = ma
        .iter()
        .map(|(k, n)| (*n).min(mb.get(k).copied().unwrap_or(0)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let r = common as f64 / denom as f64;
    r
}

/// Whitespace collapsed to single spaces and trimmed.
#[must_use]
pub fn normalize_text(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text score: 1 for equal normalised text, otherwise the word-multiset overlap.
#[must_use]
pub fn text_score(a: &str, b: &str) -> f64 {
    let (na, nb) = (normalize_text(a), normalize_text(b));
    if na == nb {
        1.0
    } else {
        overlap(na.split(' '), nb.split(' ')).min(1.0 - f64::EPSILON)
    }
}

/// A formula in comparable form: no leading `=`, no whitespace and upper case outside string
/// literals.
#[must_use]
pub fn normalize_formula(f: &str) -> String {
    let body = f.trim().trim_start_matches('=');
    let mut out = String::with_capacity(body.len());
    let mut in_str = false;
    for c in body.chars() {
        if c == '"' {
            in_str = !in_str;
            out.push(c);
        } else if in_str {
            out.push(c);
        } else if !c.is_whitespace() {
            out.extend(c.to_uppercase());
        }
    }
    out
}

/// Whether two stored values are the same: equal text, or numbers equal to the 15 digits every
/// writer keeps (Excel stores 17, LibreOffice 15).
fn same_value(a: &str, b: &str) -> bool {
    if a == b {
        return true;
    }
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => (x - y).abs() <= 1e-14 * x.abs().max(y.abs()),
        _ => false,
    }
}

/// `values` and `formulas` over two cell maps (both `None` when neither side has cells; when
/// there are cells but neither side has a formula, formulas are trivially kept: 1).
#[must_use]
pub fn cell_scores(
    a: &BTreeMap<String, CellFact>,
    b: &BTreeMap<String, CellFact>,
) -> (Option<f64>, Option<f64>) {
    let keys: std::collections::BTreeSet<&String> = a.keys().chain(b.keys()).collect();
    if keys.is_empty() {
        return (None, None);
    }
    let (mut same_values, mut formula_cells, mut same_formulas) = (0usize, 0usize, 0usize);
    for k in &keys {
        let (x, y) = (a.get(*k), b.get(*k));
        if let (Some(x), Some(y)) = (x, y)
            // A formula the file saved without a result has nothing to keep.
            && (x.value.is_empty() || same_value(&x.value, &y.value))
        {
            same_values += 1;
        }
        let fx = x.and_then(|c| c.formula.as_deref()).map(normalize_formula);
        let fy = y.and_then(|c| c.formula.as_deref()).map(normalize_formula);
        if fx.is_some() || fy.is_some() {
            formula_cells += 1;
            if fx == fy {
                same_formulas += 1;
            }
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = |n: usize, d: usize| n as f64 / d as f64;
    (
        Some(ratio(same_values, keys.len())),
        Some(if formula_cells > 0 {
            ratio(same_formulas, formula_cells)
        } else {
            1.0
        }),
    )
}

/// How a run is configured.
#[derive(Debug, Clone)]
pub struct Options {
    /// Formats to score (empty = all registered).
    pub formats: Vec<FixtureFormat>,
    /// Use the LibreOffice oracle when it is installed (`oracle_opens`, `visual`).
    pub use_oracle: bool,
    /// Scratch directory for written files and renders.
    pub scratch: PathBuf,
    /// Resolution of the page renders for `visual`.
    pub dpi: u32,
    /// Runs Kiln's reader and writer in a child process per file, so a crash or a hang costs that
    /// file and not the run (`None` = in this process).
    pub isolation: Option<Isolation>,
    /// Files scored at once, each with its own LibreOffice profile.
    pub jobs: usize,
    /// Logs one line per file to standard error.
    pub progress: bool,
}

/// How a file's reader and writer run in a child process.
#[derive(Debug, Clone)]
pub struct Isolation {
    /// The program, called as `<exe> <args…> <mode> <format> <input> <output>`.
    pub exe: PathBuf,
    /// Arguments before the mode.
    pub args: Vec<String>,
    /// How long one file may take.
    pub timeout: Duration,
    /// Address-space limit of the child, in KiB.
    pub memory_kib: u64,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            formats: Vec::new(),
            use_oracle: true,
            scratch: std::env::temp_dir().join(format!("kiln-fidelity-{}", std::process::id())),
            dpi: 60,
            isolation: None,
            jobs: 1,
            progress: false,
        }
    }
}

/// The scores and verdict of one file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileResult {
    /// The fixture (path relative to `fixtures/`).
    pub fixture: Fixture,
    /// The adapter that scored it.
    pub adapter: &'static str,
    /// What was measured.
    pub metrics: Metrics,
    /// Thresholds missed (`text 0.98 < 1`), empty when the file passes.
    pub failures: Vec<String>,
    /// Remarks (parts that differ, metrics skipped).
    pub notes: Vec<String>,
}

impl FileResult {
    /// Whether every threshold was met.
    #[must_use]
    pub fn passed(&self) -> bool {
        self.failures.is_empty()
    }
}

/// The OPC passthrough baseline of one file.
#[derive(Debug, Clone, PartialEq)]
pub struct BaselineResult {
    /// The fixture.
    pub fixture: Fixture,
    /// The comparison, or why the package could not be repacked.
    pub outcome: Result<package::PartComparison, String>,
}

/// Where every render of a fixture's files is made from: one path for the original and each
/// round trip of it, the same in every run and every checkout. A document can show its own path
/// (Word's `FILENAME \p` field, which LibreOffice draws), so a render from wherever the file
/// happens to be would score the path: `external/docx/lo-2129393649.docx` came out 0.951 or
/// 0.984 by the length of the checkout's path, for every tool alike. The files of one fixture are
/// rendered one after another, so they share the place.
pub(crate) fn render_path(fixture: &Fixture, file: &Path) -> Result<PathBuf, String> {
    let dir = std::env::temp_dir()
        .join("kiln-fidelity-render")
        .join(fixture.rel.replace('/', "__"));
    std::fs::create_dir_all(&dir).map_err(|e| format!("visual: {}: {e}", dir.display()))?;
    // No dot in the stem: LibreOffice names its output after the whole stem, and
    // `Oracle::convert` looks for it by the stem up to its last dot.
    let stem = fixture.path.file_stem().map_or_else(
        || "document".into(),
        |s| s.to_string_lossy().replace('.', "-"),
    );
    let ext = file
        .extension()
        .map_or_else(String::new, |e| format!(".{}", e.to_string_lossy()));
    let at = dir.join(format!("{stem}{ext}"));
    std::fs::copy(file, &at).map_err(|e| format!("visual: {}: {e}", at.display()))?;
    Ok(at)
}

/// The LibreOffice render of an original, one image per page.
pub(crate) fn render_original(
    oracle: &Oracle,
    original: &Fixture,
    dir: &Path,
    dpi: u32,
) -> Result<Vec<crate::image::Rgba>, String> {
    if !oracle.can_render_pages() {
        return Err("visual: pdftoppm is not installed".into());
    }
    let input = render_path(original, &original.path)?;
    let pdf = oracle
        .convert(&input, "pdf", &dir.join("in"))
        .map_err(|e| format!("visual: the original does not convert: {e}"))?;
    load_pages(oracle, &pdf, dir, "in", dpi)
}

fn load_pages(
    oracle: &Oracle,
    pdf: &Path,
    dir: &Path,
    prefix: &str,
    dpi: u32,
) -> Result<Vec<crate::image::Rgba>, String> {
    oracle
        .render_pages(pdf, dpi, &dir.join("pages"), prefix)?
        .iter()
        .map(|p| crate::image::load_png(p))
        .collect()
}

/// Visual and oracle scoring of one written file against the render of the original.
#[allow(clippy::too_many_arguments)]
pub(crate) fn oracle_metrics(
    oracle: &Oracle,
    fixture: &Fixture,
    written: &Path,
    original: &Result<Vec<crate::image::Rgba>, String>,
    dir: &Path,
    dpi: u32,
    metrics: &mut Metrics,
    notes: &mut Vec<String>,
) {
    let prefix = written
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("out")
        .to_owned();
    // Rendered from the fixture's render path (`render_path`), as the original was.
    let out_pdf = render_path(fixture, written)
        .and_then(|input| oracle.convert(&input, "pdf", &dir.join(format!("{prefix}-pdf"))));
    metrics.set_flag(Metric::OracleOpens, out_pdf.is_ok());
    let out_pdf = match out_pdf {
        Ok(p) => p,
        Err(e) => {
            notes.push(e);
            return;
        }
    };
    let originals = match original {
        Ok(a) => a,
        Err(e) => {
            notes.push(e.clone());
            return;
        }
    };
    match load_pages(oracle, &out_pdf, dir, &prefix, dpi) {
        Ok(b) => metrics.set(
            Metric::Visual,
            crate::image::visual_score(originals, &b, crate::image::DEFAULT_THRESHOLD),
        ),
        Err(e) => notes.push(format!("visual: {e}")),
    }
}

/// What the reader and writer alone found: everything but the LibreOffice metrics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelScore {
    /// What was measured.
    pub metrics: Metrics,
    /// Why the file could not be scored (a reader or writer error).
    pub failures: Vec<String>,
    /// Remarks.
    pub notes: Vec<String>,
    /// The written file (`None` after a failure).
    pub output: Option<Vec<u8>>,
}

impl ModelScore {
    /// The score as one line of JSON, without the written bytes.
    #[must_use]
    pub fn to_json(&self) -> String {
        let metrics: BTreeMap<&str, f64> =
            self.metrics.iter().map(|(m, v)| (m.name(), v)).collect();
        serde_json::json!({
            "metrics": metrics,
            "failures": self.failures,
            "notes": self.notes,
        })
        .to_string()
    }

    /// Reads [`ModelScore::to_json`] back.
    ///
    /// # Errors
    ///
    /// When the text is not such a score.
    pub fn from_json(text: &str) -> Result<Self, String> {
        let v: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let mut score = Self::default();
        for m in Metric::ALL {
            if let Some(x) = v["metrics"][m.name()].as_f64() {
                score.metrics.0.insert(m, x);
            }
        }
        let strings = |key: &str| -> Vec<String> {
            v[key]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|s| s.as_str().map(str::to_owned))
                        .collect()
                })
                .unwrap_or_default()
        };
        score.failures = strings("failures");
        score.notes = strings("notes");
        Ok(score)
    }
}

/// Compares a written file with its original through Kiln's reader. `regenerated` lists the parts
/// the writer says it produced; `None` leaves `byte_identical_untouched` unmeasured (another
/// tool's writer, which regenerates a package whole).
#[must_use]
pub fn score_written(
    adapter: &dyn Roundtrip,
    original: &[u8],
    output: Vec<u8>,
    regenerated: Option<&[String]>,
) -> ModelScore {
    let mut score = ModelScore::default();
    let before = match adapter.snapshot(original) {
        Ok(s) => s,
        Err(e) => {
            score.failures.push(format!("read: {e}"));
            return score;
        }
    };
    let after = match adapter.snapshot(&output) {
        Ok(s) => s,
        Err(e) => {
            score.failures.push(format!("re-read: {e}"));
            return score;
        }
    };
    score.metrics.set(
        Metric::Structural,
        overlap(
            before.facts.iter().map(String::as_str),
            after.facts.iter().map(String::as_str),
        ),
    );
    score
        .metrics
        .set(Metric::Text, text_score(&before.text, &after.text));
    let (values, formulas) = cell_scores(&before.cells, &after.cells);
    if let Some(v) = values {
        score.metrics.set(Metric::Values, v);
    }
    if let Some(f) = formulas {
        score.metrics.set(Metric::Formulas, f);
    }
    if adapter.format().is_opc()
        && let (Ok(a), Ok(b)) = (
            kiln_ooxml::opc::OpcPackage::open(original),
            kiln_ooxml::opc::OpcPackage::open(&output),
        )
    {
        score
            .metrics
            .set(Metric::RawElements, stored::raw_elements(&a, &b));
    }
    if adapter.format().is_opc()
        && let Some(regenerated) = regenerated
    {
        match (
            kiln_ooxml::opc::OpcPackage::open(original),
            kiln_ooxml::opc::OpcPackage::open(&output),
        ) {
            (Ok(a), Ok(b)) => {
                let cmp = package::compare_packages(&a, &b, regenerated);
                let kept = cmp.differing.is_empty() && cmp.missing.is_empty();
                score.metrics.set_flag(Metric::ByteIdenticalUntouched, kept);
                for p in cmp.differing.iter().take(5) {
                    score.notes.push(format!("untouched part changed: {p}"));
                }
                for p in cmp.missing.iter().take(5) {
                    score.notes.push(format!("part dropped: {p}"));
                }
            }
            (Err(e), _) | (_, Err(e)) => {
                score
                    .metrics
                    .set_flag(Metric::ByteIdenticalUntouched, false);
                score.notes.push(format!("not an OPC package: {e}"));
            }
        }
    }
    score.output = Some(output);
    score
}

/// Reads and writes a file, then compares the result with the original.
#[must_use]
pub fn score_model(adapter: &dyn Roundtrip, bytes: &[u8]) -> ModelScore {
    if let Err(e) = adapter.snapshot(bytes) {
        return ModelScore {
            failures: vec![format!("read: {e}")],
            ..ModelScore::default()
        };
    }
    match adapter.roundtrip(bytes) {
        Ok(o) => score_written(adapter, bytes, o.bytes, Some(&o.regenerated)),
        Err(e) => ModelScore {
            failures: vec![format!("write: {e}")],
            ..ModelScore::default()
        },
    }
}

/// [`score_model`] or, given `written`, [`score_written`] of that file, in a child process.
pub(crate) fn isolated(
    how: &Isolation,
    format: FixtureFormat,
    input: &Path,
    written: Option<&Path>,
    output: &Path,
) -> ModelScore {
    let mut cmd = std::process::Command::new("sh");
    cmd.arg("-c")
        .arg(format!("ulimit -v {}; exec \"$@\"", how.memory_kib))
        .arg("sh")
        .arg(&how.exe)
        .args(&how.args)
        .arg(if written.is_some() {
            "--worker-compare"
        } else {
            "--worker"
        })
        .arg(format.to_string())
        .arg(input)
        .arg(written.unwrap_or(output));
    let failed = |why: String| ModelScore {
        failures: vec![why],
        ..ModelScore::default()
    };
    match crate::oracle::run_child(cmd, how.timeout) {
        Err(e) => failed(if e.contains("timed out") {
            format!("no result within {} s", how.timeout.as_secs())
        } else {
            e
        }),
        Ok(done) if !done.success => {
            let last = done.stderr.lines().rev().take(2).collect::<Vec<_>>();
            failed(format!("crashed ({}): {}", done.status, last.join(" ")))
        }
        Ok(done) => {
            let mut score = ModelScore::from_json(done.stdout.trim()).unwrap_or_else(failed);
            if written.is_none() && score.failures.is_empty() {
                match std::fs::read(output) {
                    Ok(b) => score.output = Some(b),
                    Err(e) => score.failures.push(format!("{}: {e}", output.display())),
                }
            } else if let Some(w) = written {
                score.output = std::fs::read(w).ok();
            }
            score
        }
    }
}

/// The file's own extension (`.ods` rather than the format name `odf`).
pub(crate) fn extension(f: &Fixture) -> &str {
    f.path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_else(|| f.format.extension())
}

const fn is_cell_metric(m: Metric) -> bool {
    matches!(m, Metric::Values | Metric::Formulas)
}

fn fixture_has_cells(bytes: &[u8], adapter: &dyn Roundtrip) -> bool {
    adapter.snapshot(bytes).is_ok_and(|s| !s.cells.is_empty())
}

/// Scores one file with one adapter.
#[must_use]
pub fn score_file(
    adapter: &dyn Roundtrip,
    fixture: &Fixture,
    checks: &CorpusChecks,
    oracle: Option<&Oracle>,
    options: &Options,
) -> FileResult {
    let dir = options.scratch.join(fixture.rel.replace('/', "__"));
    let result = |metrics, failures, notes| FileResult {
        fixture: fixture.clone(),
        adapter: adapter.name(),
        metrics,
        failures,
        notes,
    };
    let bytes = match fixture.read() {
        Ok(b) => b,
        Err(e) => return result(Metrics::default(), vec![e], Vec::new()),
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return result(
            Metrics::default(),
            vec![format!("{}: {e}", dir.display())],
            Vec::new(),
        );
    }
    let written = dir.join(format!("roundtrip.{}", extension(fixture)));
    let ModelScore {
        mut metrics,
        mut failures,
        mut notes,
        output,
    } = match &options.isolation {
        Some(how) => isolated(how, fixture.format, &fixture.path, None, &written),
        None => score_model(adapter, &bytes),
    };
    if !failures.is_empty() {
        let _ = std::fs::remove_dir_all(&dir);
        return result(metrics, failures, notes);
    }
    if let (Some(oracle), Some(output)) = (oracle, output) {
        match std::fs::write(&written, output) {
            Ok(()) => {
                let original = render_original(oracle, fixture, &dir, options.dpi);
                oracle_metrics(
                    oracle,
                    fixture,
                    &written,
                    &original,
                    &dir,
                    options.dpi,
                    &mut metrics,
                    &mut notes,
                );
            }
            Err(e) => notes.push(format!("could not write the output for the oracle: {e}")),
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    for m in Metric::ALL {
        let Some(min) = checks.threshold(m) else {
            continue;
        };
        match metrics.get(m) {
            Some(v) if v + 1e-12 >= min => {}
            Some(v) => failures.push(format!("{m} {} < {}", report::fmt_value(m, v), min)),
            None if matches!(m, Metric::Visual | Metric::OracleOpens) => {
                // Needs LibreOffice: reported as not measured, never as a failure.
            }
            None if !fixture_has_cells(&bytes, adapter) && is_cell_metric(m) => {
                // A workbook of drawings alone: there is nothing to compare.
            }
            None => failures.push(format!("{m} not measured")),
        }
    }
    result(metrics, failures, notes)
}

/// Runs the OPC passthrough baseline over the `fixtures` that are OPC packages.
#[must_use]
pub fn baseline(fixtures: &[Fixture]) -> Vec<BaselineResult> {
    fixtures
        .iter()
        .filter(|f| f.format.is_opc())
        .map(|f| BaselineResult {
            fixture: f.clone(),
            outcome: f
                .read()
                .and_then(|b| package::repack(&b))
                .map(|r| r.comparison),
        })
        .collect()
}

/// Scores one work item: its result and the line for the run log.
pub(crate) type FileScorer<'s, W, R> = dyn Fn(&W, Option<&Oracle>) -> (R, String) + Sync + 's;

/// Runs `score` over `work` on `options.jobs` threads, each with its own LibreOffice profile, and
/// returns the results in the order of `work`.
pub(crate) fn for_each_file<W: Sync, R: Send>(
    work: &[W],
    options: &Options,
    oracle: Option<&Oracle>,
    score: &FileScorer<'_, W, R>,
) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<(usize, R)>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for job in 0..options.jobs.max(1) {
            let (next, done, results) = (&next, &done, &results);
            let own =
                oracle.map(|o| o.with_profile(options.scratch.join(format!("profile-{job}"))));
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(item) = work.get(i) else { break };
                    let start = Instant::now();
                    let (r, line) = score(item, own.as_ref());
                    if options.progress {
                        let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                        eprintln!(
                            "[{n}/{}] {:.1}s {line}",
                            work.len(),
                            start.elapsed().as_secs_f32()
                        );
                    }
                    if let Ok(mut all) = results.lock() {
                        all.push((i, r));
                    }
                }
            });
        }
    });
    let mut all = results.into_inner().unwrap_or_default();
    all.sort_by_key(|(i, _)| *i);
    all.into_iter().map(|(_, r)| r).collect()
}

/// Runs the scorer: every registered adapter (filtered by `options.formats`) over the fixtures
/// of its format, plus the baseline over every fixture of the selected formats.
#[must_use]
pub fn run(
    adapters: &[Box<dyn Roundtrip>],
    fixtures: &[Fixture],
    corpus: &crate::fixtures::Corpus,
    options: &Options,
) -> Report {
    let wanted = |f: FixtureFormat| options.formats.is_empty() || options.formats.contains(&f);
    let oracle = if options.use_oracle {
        Oracle::locate().map_err(|e| e.to_string())
    } else {
        Err("disabled (--no-oracle)".to_owned())
    };
    let mut work: Vec<(&dyn Roundtrip, &Fixture)> = Vec::new();
    for adapter in adapters.iter().filter(|a| wanted(a.format())) {
        for f in fixtures.iter().filter(|f| f.format == adapter.format()) {
            work.push((adapter.as_ref(), f));
        }
    }
    let oracle_ref = oracle.as_ref().ok();
    let files = for_each_file(&work, options, oracle_ref, &|(adapter, f), oracle| {
        let checks = corpus
            .get(&f.rel)
            .map_or_else(|| default_checks(f.format), |e| e.checks.clone());
        let r = score_file(*adapter, f, &checks, oracle, options);
        let line = if r.passed() {
            format!("pass {}", f.rel)
        } else {
            format!("FAIL {} {}", f.rel, r.failures.join("; "))
        };
        (r, line)
    });
    let selected: Vec<Fixture> = fixtures
        .iter()
        .filter(|f| wanted(f.format))
        .cloned()
        .collect();
    Report {
        adapters: adapters
            .iter()
            .filter(|a| wanted(a.format()))
            .map(|a| (a.format(), a.name()))
            .collect(),
        formats: FixtureFormat::ALL
            .into_iter()
            .filter(|f| wanted(*f))
            .collect(),
        files,
        baseline: baseline(&selected),
        oracle: match oracle {
            Ok(o) => Ok(o.soffice().display().to_string()),
            Err(e) => Err(e),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The original and its round trip are rendered from one path, whatever checkout or scratch
    /// directory they are in (a document's `FILENAME \p` field shows it).
    #[test]
    fn every_render_of_a_fixture_is_made_from_one_path() {
        let base = std::env::temp_dir().join(format!("kiln-render-path-{}", std::process::id()));
        let (a, b) = (base.join("short"), base.join("a/much/longer/checkout/path"));
        for d in [&a, &b] {
            std::fs::create_dir_all(d).unwrap();
            std::fs::write(d.join("lo-1.docx"), b"x").unwrap();
            std::fs::write(d.join("roundtrip.docx"), b"y").unwrap();
        }
        let fixture = |dir: &Path| Fixture {
            rel: "external/docx/lo-1.docx".into(),
            path: dir.join("lo-1.docx"),
            format: FixtureFormat::Docx,
            kind: crate::fixtures::FixtureKind::Features,
        };
        let original_a = render_path(&fixture(&a), &a.join("lo-1.docx")).unwrap();
        let output_b = render_path(&fixture(&b), &b.join("roundtrip.docx")).unwrap();
        assert_eq!(original_a, output_b);
        assert_eq!(std::fs::read(&output_b).unwrap(), b"y");
        // A dotted name (`poi-SampleSS.strict.xlsx`) loses its inner dot, so LibreOffice's PDF is
        // found under the name `Oracle::convert` looks for.
        let dotted = Fixture {
            rel: "external/xlsx/a.strict.xlsx".into(),
            path: a.join("a.strict.xlsx"),
            format: FixtureFormat::Xlsx,
            kind: crate::fixtures::FixtureKind::Features,
        };
        std::fs::write(&dotted.path, b"z").unwrap();
        let at = render_path(&dotted, &dotted.path).unwrap();
        assert_eq!(
            at.file_name().and_then(|n| n.to_str()),
            Some("a-strict.xlsx")
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn overlap_counts_multisets() {
        assert_eq!(overlap(["a", "b", "b"], ["b", "a", "b"]), 1.0);
        assert!((overlap(["a", "b"], ["a"]) - 0.5).abs() < 1e-12);
        assert!((overlap(["a", "a"], ["a", "c"]) - 0.5).abs() < 1e-12);
        assert_eq!(overlap(Vec::<&str>::new(), Vec::<&str>::new()), 1.0);
    }

    #[test]
    fn text_score_normalises_whitespace() {
        assert_eq!(text_score("a  b\n c", " a b c "), 1.0);
        let s = text_score("one two three four", "one two three");
        assert!((0.75 - 1e-9..1.0).contains(&s), "{s}");
    }

    #[test]
    fn formulas_compare_without_case_and_spaces() {
        assert_eq!(normalize_formula("=sum( A1 , b2 )"), "SUM(A1,B2)");
        assert_eq!(normalize_formula("=\"a b\"&x"), "\"a b\"&X");
    }

    #[test]
    fn cell_scores_use_the_union_of_cells() {
        let cell = |v: &str, f: Option<&str>| CellFact {
            value: v.into(),
            formula: f.map(Into::into),
        };
        let mut a = BTreeMap::new();
        a.insert("S!A1".to_owned(), cell("1", None));
        a.insert("S!A2".to_owned(), cell("2", Some("=A1+1")));
        let mut b = a.clone();
        assert_eq!(cell_scores(&a, &b), (Some(1.0), Some(1.0)));
        b.insert("S!A2".to_owned(), cell("2", Some("= a1 + 1")));
        assert_eq!(cell_scores(&a, &b), (Some(1.0), Some(1.0)));
        b.insert("S!A3".to_owned(), cell("x", None));
        b.insert("S!A2".to_owned(), cell("3", None));
        let (v, f) = cell_scores(&a, &b);
        assert!((v.unwrap() - 1.0 / 3.0).abs() < 1e-12);
        assert_eq!(f, Some(0.0));
        assert_eq!(
            cell_scores(&BTreeMap::new(), &BTreeMap::new()),
            (None, None)
        );
        // Cells without formulas keep their (no) formulas.
        let mut plain = BTreeMap::new();
        plain.insert("S!A1".to_owned(), cell("1", None));
        assert_eq!(cell_scores(&plain, &plain), (Some(1.0), Some(1.0)));
    }

    #[test]
    fn a_workbook_without_cells_is_not_failed_on_cell_metrics() {
        struct NoCells;
        impl Roundtrip for NoCells {
            fn format(&self) -> FixtureFormat {
                FixtureFormat::Xlsx
            }
            fn name(&self) -> &'static str {
                "no-cells"
            }
            fn snapshot(&self, _: &[u8]) -> Result<Snapshot, String> {
                Ok(Snapshot::default())
            }
            fn roundtrip(&self, b: &[u8]) -> Result<RoundtripOutput, String> {
                Ok(RoundtripOutput {
                    bytes: b.to_vec(),
                    regenerated: Vec::new(),
                })
            }
        }
        let dir = std::env::temp_dir().join(format!("kiln-no-cells-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("a.xlsx");
        std::fs::write(&path, b"x").unwrap();
        let fixture = Fixture {
            rel: "a.xlsx".into(),
            path,
            format: FixtureFormat::Xlsx,
            kind: crate::fixtures::FixtureKind::Features,
        };
        let checks = CorpusChecks {
            values: Some(1.0),
            formulas: Some(1.0),
            ..CorpusChecks::default()
        };
        let options = Options {
            scratch: dir.join("scratch"),
            ..Options::default()
        };
        let r = score_file(&NoCells, &fixture, &checks, None, &options);
        assert!(r.passed(), "{:?}", r.failures);
        assert_eq!(r.metrics.get(Metric::Values), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn stored_values_are_compared_to_fifteen_digits() {
        assert!(same_value("0.1", "0.1"));
        assert!(same_value("44615.39583333334", "44615.3958333333"));
        assert!(!same_value("31", "32"));
        assert!(!same_value("0", "#NAME?"));
    }

    #[test]
    fn metrics_clamp_and_flag() {
        let mut m = Metrics::default();
        m.set(Metric::Text, 1.5);
        m.set(Metric::Visual, f64::NAN);
        m.set_flag(Metric::OracleOpens, true);
        assert_eq!(m.get(Metric::Text), Some(1.0));
        assert_eq!(m.get(Metric::Visual), Some(0.0));
        assert_eq!(m.get(Metric::OracleOpens), Some(1.0));
        assert_eq!(m.get(Metric::Values), None);
        assert!(Metric::ByteIdenticalUntouched.is_flag() && !Metric::Text.is_flag());
    }

    #[test]
    fn default_checks_follow_the_release_targets() {
        let d = default_checks(FixtureFormat::Docx);
        assert_eq!(d.threshold(Metric::Text), Some(1.0));
        assert_eq!(d.threshold(Metric::Values), None);
        let x = default_checks(FixtureFormat::Xlsx);
        assert_eq!(x.threshold(Metric::Formulas), Some(1.0));
        let p = default_checks(FixtureFormat::Pptx);
        assert_eq!(p.threshold(Metric::Visual), Some(0.85));
        let o = default_checks(FixtureFormat::Odf);
        assert_eq!(o.threshold(Metric::Structural), Some(0.90));
        assert_eq!(o.threshold(Metric::Visual), Some(0.80));
        assert_eq!(o.threshold(Metric::ByteIdenticalUntouched), None);
    }
}
