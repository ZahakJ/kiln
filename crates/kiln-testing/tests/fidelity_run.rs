// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fidelity scorer end to end with test adapters: a faithful one (the OPC passthrough, with
//! the text of `word/document.xml` as the snapshot) and a lossy one (drops a paragraph and changes
//! an untouched part), over the committed `.docx` corpus.

use kiln_ooxml::opc::OpcPackage;
use kiln_testing::fidelity::{self, Options, Roundtrip, RoundtripOutput, Snapshot};
use kiln_testing::fixtures::{self, Corpus, CorpusChecks, FixtureFormat, FixtureKind};
use kiln_testing::{Metric, oracle::Oracle};

fn document_snapshot(bytes: &[u8]) -> Result<Snapshot, String> {
    let pkg = OpcPackage::open(bytes).map_err(|e| e.to_string())?;
    let doc = pkg
        .part_xml("/word/document.xml")
        .map_err(|e| e.to_string())?
        .ok_or("no /word/document.xml")?;
    let mut snap = Snapshot::default();
    for p in doc.root.descendants("w:p") {
        let text: String = p
            .descendants("w:t")
            .iter()
            .flat_map(|t| t.children.iter())
            .filter_map(|n| match n {
                kiln_ooxml::xml::XmlNode::Text(t) => Some(t.text.as_str()),
                _ => None,
            })
            .collect();
        snap.facts.push(format!("p:{text}"));
        snap.text.push_str(&text);
        snap.text.push('\n');
    }
    Ok(snap)
}

/// Reads and writes through the OPC layer only: everything survives.
struct Passthrough;

impl Roundtrip for Passthrough {
    fn format(&self) -> FixtureFormat {
        FixtureFormat::Docx
    }
    fn name(&self) -> &'static str {
        "test-passthrough"
    }
    fn snapshot(&self, bytes: &[u8]) -> Result<Snapshot, String> {
        document_snapshot(bytes)
    }
    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String> {
        let pkg = OpcPackage::open(bytes).map_err(|e| e.to_string())?;
        Ok(RoundtripOutput {
            bytes: pkg.save().map_err(|e| e.to_string())?,
            regenerated: vec![],
        })
    }
}

/// Drops the last paragraph and rewrites the core properties without declaring it.
struct Lossy;

impl Roundtrip for Lossy {
    fn format(&self) -> FixtureFormat {
        FixtureFormat::Docx
    }
    fn name(&self) -> &'static str {
        "test-lossy"
    }
    fn snapshot(&self, bytes: &[u8]) -> Result<Snapshot, String> {
        document_snapshot(bytes)
    }
    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String> {
        let mut pkg = OpcPackage::open(bytes).map_err(|e| e.to_string())?;
        let mut doc = pkg
            .part_xml("/word/document.xml")
            .map_err(|e| e.to_string())?
            .ok_or("no document")?;
        if let Some(body) = doc.root.child_mut("w:body") {
            let last_p = body
                .children
                .iter()
                .rposition(|n| n.as_element().is_some_and(|e| e.name == "w:p"));
            if let Some(i) = last_p {
                body.children.remove(i);
            }
        }
        pkg.set_part_xml("/word/document.xml", &doc)
            .map_err(|e| e.to_string())?;
        if pkg.has("/docProps/core.xml") {
            pkg.set_part("/docProps/core.xml", b"<changed/>".to_vec())
                .map_err(|e| e.to_string())?;
        }
        Ok(RoundtripOutput {
            bytes: pkg.save().map_err(|e| e.to_string())?,
            regenerated: vec!["/word/document.xml".into()],
        })
    }
}

fn docx_features() -> Vec<fixtures::Fixture> {
    fixtures::discover_in(
        &fixtures::fixtures_dir(),
        &[FixtureFormat::Docx],
        &[FixtureKind::Features],
    )
}

fn no_oracle() -> Options {
    Options {
        use_oracle: false,
        ..Options::default()
    }
}

#[test]
fn a_faithful_adapter_passes_the_corpus() {
    let corpus = Corpus::load_repo().unwrap();
    let files = docx_features();
    let adapters: Vec<Box<dyn Roundtrip>> = vec![Box::new(Passthrough)];
    let report = fidelity::run(&adapters, &files, &corpus, &no_oracle());
    assert_eq!(report.files.len(), files.len());
    assert!(report.passed(), "{}", report.to_text());
    let summary = report.summary();
    assert_eq!(summary.len(), 1);
    assert_eq!(summary[0].passing, files.len());
    assert_eq!(summary[0].lowest.get(&Metric::Structural), Some(&1.0));
    assert_eq!(summary[0].lowest.get(&Metric::Text), Some(&1.0));
    assert_eq!(
        summary[0].lowest.get(&Metric::ByteIdenticalUntouched),
        Some(&1.0)
    );
    // Without the oracle, visual and oracle_opens are not measured and not failures.
    assert_eq!(summary[0].lowest.get(&Metric::Visual), None);
    let md = report.to_markdown();
    assert!(md.contains("| docx | test-passthrough |"), "{md}");
    assert!(md.contains("not used (disabled (--no-oracle))"));
}

#[test]
fn a_lossy_adapter_fails_with_reasons() {
    let corpus = Corpus::load_repo().unwrap();
    let files: Vec<_> = docx_features()
        .into_iter()
        .filter(|f| f.file_name() == "paragraphs.docx")
        .collect();
    assert_eq!(files.len(), 1);
    let adapters: Vec<Box<dyn Roundtrip>> = vec![Box::new(Lossy)];
    let report = fidelity::run(&adapters, &files, &corpus, &no_oracle());
    assert!(!report.passed());
    let f = &report.files[0];
    assert!(f.metrics.get(Metric::Structural).unwrap() < 1.0);
    assert!(f.metrics.get(Metric::Text).unwrap() < 1.0);
    assert_eq!(f.metrics.get(Metric::ByteIdenticalUntouched), Some(0.0));
    assert!(
        f.failures.iter().any(|m| m.starts_with("text ")),
        "{:?}",
        f.failures
    );
    assert!(
        f.notes
            .iter()
            .any(|n| n == "untouched part changed: /docProps/core.xml"),
        "{:?}",
        f.notes
    );
    assert!(report.to_markdown().contains("**fail**"));
}

#[test]
fn the_empty_registry_measures_only_the_baseline() {
    let corpus = Corpus::load_repo().unwrap();
    let files = fixtures::discover_in(
        &fixtures::fixtures_dir(),
        &FixtureFormat::ALL,
        &[FixtureKind::Features, FixtureKind::Manual],
    );
    let report = fidelity::run(
        &kiln_testing::formats::registered(),
        &files,
        &corpus,
        &no_oracle(),
    );
    assert!(report.no_formats_registered());
    assert_eq!(report.baseline.len(), files.len());
    assert!(report.passed(), "{}", report.to_text());
}

#[test]
fn the_oracle_scores_visual_and_opens_when_installed() {
    let Ok(oracle) = Oracle::locate() else {
        eprintln!("skipped: LibreOffice is not installed");
        return;
    };
    if !oracle.can_render_pages() {
        eprintln!("skipped: pdftoppm is not installed");
        return;
    }
    let f = fixtures::find("paragraphs.docx").unwrap();
    let options = Options::default();
    let checks = CorpusChecks {
        visual: Some(0.99),
        oracle_opens: Some(true),
        ..CorpusChecks::default()
    };
    let r = fidelity::score_file(&Passthrough, &f, &checks, Some(&oracle), &options);
    assert_eq!(
        r.metrics.get(Metric::OracleOpens),
        Some(1.0),
        "{:?}",
        r.notes
    );
    let visual = r.metrics.get(Metric::Visual).unwrap();
    assert!(
        visual > 0.999,
        "identical files render identically: {visual}"
    );
    assert!(r.passed(), "{:?}", r.failures);
    let _ = std::fs::remove_dir_all(&options.scratch);
}
