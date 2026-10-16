// SPDX-License-Identifier: AGPL-3.0-or-later
//! The corpus manifest `parity/corpus.json` (schema `parity/schema.json#/$defs/corpusFile`):
//! every fixture with the fidelity checks it must pass and the parity rows it exercises.

use std::collections::BTreeMap;
use std::path::Path;

use serde::Deserialize;

use crate::fidelity::Metric;

/// The thresholds one file must meet. Absent checks are not required.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusChecks {
    /// Minimum structural score.
    pub structural: Option<f64>,
    /// Minimum text score.
    pub text: Option<f64>,
    /// Minimum visual score.
    pub visual: Option<f64>,
    /// Minimum cell-value score (spreadsheets).
    pub values: Option<f64>,
    /// Minimum formula score (spreadsheets).
    pub formulas: Option<f64>,
    /// LibreOffice must open the written file.
    pub oracle_opens: Option<bool>,
    /// Untouched parts must be byte-identical.
    pub byte_identical_untouched: Option<bool>,
}

impl CorpusChecks {
    /// The minimum for a metric: a ratio, or 1.0 for a required boolean check.
    #[must_use]
    pub fn threshold(&self, metric: Metric) -> Option<f64> {
        let flag = |b: Option<bool>| b.and_then(|b| b.then_some(1.0));
        match metric {
            Metric::Structural => self.structural,
            Metric::Text => self.text,
            Metric::Visual => self.visual,
            Metric::Values => self.values,
            Metric::Formulas => self.formulas,
            Metric::OracleOpens => flag(self.oracle_opens),
            Metric::ByteIdenticalUntouched => flag(self.byte_identical_untouched),
        }
    }
}

/// One file of the manifest.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CorpusEntry {
    /// Path relative to `fixtures/`.
    pub file: String,
    /// Thresholds.
    pub checks: CorpusChecks,
    /// Parity rows the file exercises.
    #[serde(default)]
    pub rows: Vec<String>,
    /// Free-form notes.
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CorpusFile {
    version: u32,
    files: Vec<CorpusEntry>,
    #[serde(rename = "$schema", default)]
    _schema: Option<String>,
}

/// The parsed manifest, keyed by file.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Corpus {
    entries: BTreeMap<String, CorpusEntry>,
}

impl Corpus {
    /// Parses the manifest text.
    ///
    /// # Errors
    ///
    /// Malformed JSON, an unknown version, unknown keys, duplicate files or ratios outside 0..=1.
    pub fn parse(text: &str) -> Result<Self, String> {
        let file: CorpusFile =
            serde_json::from_str(text).map_err(|e| format!("corpus.json: {e}"))?;
        if file.version != 1 {
            return Err(format!(
                "corpus.json: version {} is not supported",
                file.version
            ));
        }
        let mut entries = BTreeMap::new();
        for e in file.files {
            for m in Metric::ALL {
                if let Some(t) = e.checks.threshold(m)
                    && !(0.0..=1.0).contains(&t)
                {
                    return Err(format!("corpus.json: {} {m} = {t} is not a ratio", e.file));
                }
            }
            let key = e.file.clone();
            if entries.insert(key.clone(), e).is_some() {
                return Err(format!("corpus.json: {key} is listed twice"));
            }
        }
        Ok(Self { entries })
    }

    /// Reads a manifest file.
    ///
    /// # Errors
    ///
    /// I/O errors and those of [`Corpus::parse`].
    pub fn load(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text)
    }

    /// The repository's manifest (`parity/corpus.json`).
    ///
    /// # Errors
    ///
    /// As [`Corpus::load`].
    pub fn load_repo() -> Result<Self, String> {
        Self::load(&crate::repo_root().join("parity/corpus.json"))
    }

    /// The entry of a file (path relative to `fixtures/`).
    #[must_use]
    pub fn get(&self, rel: &str) -> Option<&CorpusEntry> {
        self.entries.get(rel)
    }

    /// Every entry, sorted by file.
    pub fn entries(&self) -> impl Iterator<Item = &CorpusEntry> {
        self.entries.values()
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when the manifest lists no file.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::{FixtureFormat, FixtureKind, discover_in, fixtures_dir};

    #[test]
    fn parses_entries_and_rejects_bad_manifests() {
        let c = Corpus::parse(
            r#"{"version":1,"files":[{"file":"docx/features/a.docx","checks":{"text":1.0,"oracle_opens":true,"byte_identical_untouched":false},"rows":["x"]}]}"#,
        )
        .unwrap();
        let e = c.get("docx/features/a.docx").unwrap();
        assert_eq!(e.checks.threshold(Metric::Text), Some(1.0));
        assert_eq!(e.checks.threshold(Metric::OracleOpens), Some(1.0));
        assert_eq!(e.checks.threshold(Metric::ByteIdenticalUntouched), None);
        assert_eq!(e.checks.threshold(Metric::Visual), None);
        assert_eq!(e.rows, ["x"]);
        assert!(Corpus::parse(r#"{"version":2,"files":[]}"#).is_err());
        assert!(
            Corpus::parse(r#"{"version":1,"files":[{"file":"a","checks":{"text":1.5}}]}"#).is_err()
        );
        assert!(
            Corpus::parse(r#"{"version":1,"files":[{"file":"a","checks":{"txt":1}}]}"#).is_err()
        );
        let dup = r#"{"version":1,"files":[{"file":"a","checks":{"text":1}},{"file":"a","checks":{"text":1}}]}"#;
        assert!(Corpus::parse(dup).unwrap_err().contains("twice"));
    }

    #[test]
    fn the_repository_manifest_lists_every_feature_fixture() {
        let corpus = Corpus::load_repo().unwrap();
        let features = discover_in(
            &fixtures_dir(),
            &FixtureFormat::ALL,
            &[FixtureKind::Features],
        );
        assert!(!features.is_empty());
        for f in &features {
            assert!(
                corpus.get(&f.rel).is_some(),
                "{} missing from corpus.json",
                f.rel
            );
        }
        for e in corpus.entries() {
            assert!(
                fixtures_dir().join(&e.file).is_file(),
                "corpus.json lists missing {}",
                e.file
            );
        }
    }
}
