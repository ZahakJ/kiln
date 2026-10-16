// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every fixture round-trips (ported from the v1.0 writer's `roundtrip/fixtures.test.ts`, made
//! stricter): read → write (every model part regenerated) → read gives the same model *and* the
//! same reader extras, node ids included; every part the writer did not regenerate is
//! byte-identical; writing is deterministic and stable (a second round trip writes the same
//! bytes); and mammoth, an independent reader, sees the same text in the rewritten file.

mod roundtrip_support;

use std::process::Command;

use kiln_io_docx::{WriteOptions, rewrite_docx};
use kiln_ooxml::opc::OpcPackage;
use kiln_testing::fidelity::package::compare_packages;
use kiln_testing::fixtures::{Fixture, FixtureFormat, FixtureKind, discover_in, fixtures_dir};
use roundtrip_support::{differences, round_trip};

/// The committed corpus (features and manual) plus the downloaded one when present (including
/// `external/docx/poi-deep-table-cell.docx`, 15,005 elements deep, whose over-deep tables are kept
/// verbatim); a file the reader refused would be left out.
fn corpus() -> Vec<(Fixture, Vec<u8>)> {
    discover_in(
        &fixtures_dir(),
        &[FixtureFormat::Docx],
        &[
            FixtureKind::Features,
            FixtureKind::Manual,
            FixtureKind::External,
        ],
    )
    .into_iter()
    .filter_map(|f| {
        let bytes = f.read().ok()?;
        kiln_io_docx::read_docx(&bytes).ok()?;
        Some((f, bytes))
    })
    .collect()
}

#[test]
fn the_committed_corpus_is_all_there() {
    let names: Vec<String> = corpus()
        .into_iter()
        .filter(|(f, _)| f.kind != FixtureKind::External)
        .map(|(f, _)| f.rel)
        .collect();
    assert_eq!(names.len(), 21, "{names:?}");
}

#[test]
fn every_fixture_comes_back_with_the_same_model_and_extras() {
    for (f, bytes) in corpus() {
        let rt = round_trip(&bytes);
        let d = differences(&rt.a, &rt.b);
        assert!(d.is_empty(), "{}:\n{}", f.rel, d.join("\n\n"));
        assert!(rt.issues.is_empty(), "{}: {:?}", f.rel, rt.issues);
        assert_eq!(rt.b.document.validate(), Ok(()), "{}", f.rel);
    }
}

#[test]
fn parts_the_writer_did_not_regenerate_are_byte_identical() {
    for (f, bytes) in corpus() {
        let written = rewrite_docx(&bytes, &WriteOptions::default()).unwrap();
        let (a, b) = (
            OpcPackage::open(&bytes).unwrap(),
            OpcPackage::open(&written.bytes).unwrap(),
        );
        let cmp = compare_packages(&a, &b, &written.regenerated);
        assert!(
            cmp.differing.is_empty() && cmp.missing.is_empty(),
            "{}: {cmp:?}",
            f.rel
        );
        assert!(
            written.dropped.is_empty(),
            "{}: {:?}",
            f.rel,
            written.dropped
        );
        // Media and passthrough parts are compared, never excused.
        for name in &written.regenerated {
            assert!(!name.contains("/media/"), "{}: {name}", f.rel);
            assert!(
                !name.starts_with("/customXml/") || name.ends_with(".rels"),
                "{}: {name}",
                f.rel
            );
        }
    }
}

#[test]
fn writing_is_deterministic_and_stable() {
    for (f, bytes) in corpus() {
        let first = rewrite_docx(&bytes, &WriteOptions::default()).unwrap();
        let again = rewrite_docx(&bytes, &WriteOptions::default()).unwrap();
        assert_eq!(first.bytes, again.bytes, "{}: deterministic", f.rel);
        // A second generation writes every part identically.
        let second = rewrite_docx(&first.bytes, &WriteOptions::default()).unwrap();
        let (p1, p2) = (
            OpcPackage::open(&first.bytes).unwrap(),
            OpcPackage::open(&second.bytes).unwrap(),
        );
        let cmp = compare_packages(&p1, &p2, &[]);
        assert!(cmp.all_identical(), "{}: stable: {cmp:?}", f.rel);
    }
}

#[test]
fn mammoth_reads_the_same_text_from_the_rewritten_files() {
    let node_ok = Command::new("node")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !node_ok {
        eprintln!("skipped: node is not installed");
        return;
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/roundtrip_support/mammoth.mjs");
    let dir = std::env::temp_dir().join(format!("kiln-mammoth-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut args = Vec::new();
    let features: Vec<_> = corpus()
        .into_iter()
        .filter(|(f, _)| f.kind == FixtureKind::Features)
        .collect();
    for (f, bytes) in &features {
        let out = dir.join(f.file_name());
        std::fs::write(
            &out,
            rewrite_docx(bytes, &WriteOptions::default()).unwrap().bytes,
        )
        .unwrap();
        args.push(f.path.clone());
        args.push(out);
    }
    let output = Command::new("node")
        .arg(&script)
        .args(&args)
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    if output.status.code() == Some(3) {
        eprintln!("skipped: mammoth is not installed (pnpm install)");
        return;
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let texts: Vec<&str> = stdout.split('\0').collect();
    assert_eq!(texts.len(), features.len() * 2 + 1, "one text per file");
    for (i, (f, _)) in features.iter().enumerate() {
        assert_eq!(texts[2 * i + 1], texts[2 * i], "{}", f.rel);
        assert!(!texts[2 * i].trim().is_empty(), "{}", f.rel);
    }
}
