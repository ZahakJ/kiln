// SPDX-License-Identifier: AGPL-3.0-or-later
//! The round-trip fidelity scorer (PLAN.md §3.10, §5).
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
//! | `values` | share of cells (union of both sides) with equal values (spreadsheets) |
//! | `formulas` | share of formula cells with equal normalised formulas (spreadsheets) |
//! | `byte_identical_untouched` | every part the writer did not regenerate is byte-identical |
//! | `oracle_opens` | LibreOffice converts the output to PDF (when the oracle is installed) |
//! | `visual` | page-by-page `1 − differing pixels` of LibreOffice renders of input and output at 60 dpi |
//!
//! Thresholds come from `parity/corpus.json` (or [`default_checks`] for files it does not list).
//! Independently of any adapter, every package also goes through the OPC passthrough baseline
//! ([`package::repack`]): open and save without edits must keep every part byte-identical.

pub mod package;
mod report;

pub use report::{FormatSummary, Report};

use std::collections::{BTreeMap, HashMap};
use std::fmt;
use std::path::PathBuf;

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
}

impl Metric {
    /// Every metric, in report column order.
    pub const ALL: [Metric; 7] = [
        Self::Structural,
        Self::Text,
        Self::Visual,
        Self::Values,
        Self::Formulas,
        Self::OracleOpens,
        Self::ByteIdenticalUntouched,
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

/// The release thresholds of PLAN.md §3.10 for files `parity/corpus.json` does not list.
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
            && x.value == y.value
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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            formats: Vec::new(),
            use_oracle: true,
            scratch: std::env::temp_dir().join(format!("kiln-fidelity-{}", std::process::id())),
            dpi: 60,
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

/// Visual and oracle scoring of one written file.
fn oracle_metrics(
    oracle: &Oracle,
    original: &Fixture,
    output: &[u8],
    options: &Options,
    metrics: &mut Metrics,
    notes: &mut Vec<String>,
) {
    let stem = original.rel.replace('/', "__");
    let dir = options.scratch.join(&stem);
    let written = dir.join(format!("roundtrip.{}", original.format));
    if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&written, output)) {
        notes.push(format!("could not write the output for the oracle: {e}"));
        return;
    }
    let out_pdf = oracle.convert(&written, "pdf", &dir.join("out"));
    metrics.set_flag(Metric::OracleOpens, out_pdf.is_ok());
    let out_pdf = match out_pdf {
        Ok(p) => p,
        Err(e) => {
            notes.push(e);
            return;
        }
    };
    if !oracle.can_render_pages() {
        notes.push("visual: pdftoppm is not installed".into());
        return;
    }
    let in_pdf = match oracle.convert(&original.path, "pdf", &dir.join("in")) {
        Ok(p) => p,
        Err(e) => {
            notes.push(format!("visual: the original does not convert: {e}"));
            return;
        }
    };
    let load = |pdf: &PathBuf, prefix: &str| -> Result<Vec<crate::image::Rgba>, String> {
        oracle
            .render_pages(pdf, options.dpi, &dir.join("pages"), prefix)?
            .iter()
            .map(|p| crate::image::load_png(p))
            .collect()
    };
    match (load(&in_pdf, "in"), load(&out_pdf, "out")) {
        (Ok(a), Ok(b)) => metrics.set(
            Metric::Visual,
            crate::image::visual_score(&a, &b, crate::image::DEFAULT_THRESHOLD),
        ),
        (Err(e), _) | (_, Err(e)) => notes.push(format!("visual: {e}")),
    }
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
    let mut metrics = Metrics::default();
    let mut notes = Vec::new();
    let mut failures = Vec::new();
    let result = |metrics, failures, notes| FileResult {
        fixture: fixture.clone(),
        adapter: adapter.name(),
        metrics,
        failures,
        notes,
    };
    let bytes = match fixture.read() {
        Ok(b) => b,
        Err(e) => return result(metrics, vec![e], notes),
    };
    let before = match adapter.snapshot(&bytes) {
        Ok(s) => s,
        Err(e) => return result(metrics, vec![format!("read: {e}")], notes),
    };
    let output = match adapter.roundtrip(&bytes) {
        Ok(o) => o,
        Err(e) => return result(metrics, vec![format!("write: {e}")], notes),
    };
    let after = match adapter.snapshot(&output.bytes) {
        Ok(s) => s,
        Err(e) => return result(metrics, vec![format!("re-read: {e}")], notes),
    };
    metrics.set(
        Metric::Structural,
        overlap(
            before.facts.iter().map(String::as_str),
            after.facts.iter().map(String::as_str),
        ),
    );
    metrics.set(Metric::Text, text_score(&before.text, &after.text));
    let (values, formulas) = cell_scores(&before.cells, &after.cells);
    if let Some(v) = values {
        metrics.set(Metric::Values, v);
    }
    if let Some(f) = formulas {
        metrics.set(Metric::Formulas, f);
    }
    match (
        kiln_ooxml::opc::OpcPackage::open(&bytes),
        kiln_ooxml::opc::OpcPackage::open(&output.bytes),
    ) {
        (Ok(a), Ok(b)) => {
            let cmp = package::compare_packages(&a, &b, &output.regenerated);
            let missing_ok = cmp.differing.is_empty() && cmp.missing.is_empty();
            metrics.set_flag(Metric::ByteIdenticalUntouched, missing_ok);
            for p in cmp.differing.iter().take(5) {
                notes.push(format!("untouched part changed: {p}"));
            }
            for p in cmp.missing.iter().take(5) {
                notes.push(format!("part dropped: {p}"));
            }
        }
        (Err(e), _) | (_, Err(e)) => {
            metrics.set_flag(Metric::ByteIdenticalUntouched, false);
            notes.push(format!("not an OPC package: {e}"));
        }
    }
    if let Some(oracle) = oracle {
        oracle_metrics(
            oracle,
            fixture,
            &output.bytes,
            options,
            &mut metrics,
            &mut notes,
        );
    }
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
            None => failures.push(format!("{m} not measured")),
        }
    }
    result(metrics, failures, notes)
}

/// Runs the OPC passthrough baseline over `fixtures`.
#[must_use]
pub fn baseline(fixtures: &[Fixture]) -> Vec<BaselineResult> {
    fixtures
        .iter()
        .map(|f| BaselineResult {
            fixture: f.clone(),
            outcome: f
                .read()
                .and_then(|b| package::repack(&b))
                .map(|r| r.comparison),
        })
        .collect()
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
    let mut files = Vec::new();
    for adapter in adapters.iter().filter(|a| wanted(a.format())) {
        for f in fixtures.iter().filter(|f| f.format == adapter.format()) {
            let checks = corpus
                .get(&f.rel)
                .map_or_else(|| default_checks(f.format), |e| e.checks.clone());
            files.push(score_file(
                adapter.as_ref(),
                f,
                &checks,
                oracle.as_ref().ok(),
                options,
            ));
        }
    }
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
    }
}
