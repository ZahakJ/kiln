// SPDX-License-Identifier: AGPL-3.0-or-later
//! The DOCX fixture corpus through the layout engine:
//!
//! - **Snapshots** (`fixtures/golden/write/snapshots/*.snap`): page count and the first and last
//!   words of every page, for every `fixtures/docx/{features,manual}` file.
//! - **Oracle**: page counts against LibreOffice's (`fixtures/golden/write/oracle-pages.json`,
//!   written by `KILN_ORACLE=1 cargo nextest run -p kiln-doc-layout --run-ignored all oracle`
//!   with `tools/oracle-soffice.sh` → PDF → page count). At least 80 % must be equal and all
//!   within one page; the known differences are where Word itself differs from LibreOffice
//!   (Word starts an odd-page section with a blank page; endnotes follow the text instead of
//!   starting a page).
//! - **Golden PNGs** (`fixtures/golden/write/*.png`): pages of 12 fixtures at 96 dpi, compared
//!   with a 0.3 % pixel tolerance. `KILN_UPDATE_GOLDEN=1` rewrites them; review the images.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::*;
use kiln_doc_layout::snapshot::summary;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn golden_dir() -> PathBuf {
    repo().join("fixtures/golden/write")
}

fn corpus() -> Vec<PathBuf> {
    let mut out = Vec::new();
    for dir in ["fixtures/docx/features", "fixtures/docx/manual"] {
        let Ok(rd) = std::fs::read_dir(repo().join(dir)) else {
            continue;
        };
        for e in rd.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "docx") {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn name(p: &Path) -> String {
    p.file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn open(p: &Path) -> kiln_doc::Document {
    let bytes = std::fs::read(p).expect("fixture");
    kiln_io_docx::read_docx(&bytes).expect("reads").document
}

#[test]
fn every_fixture_paginates_like_its_snapshot() {
    let files = corpus();
    assert!(files.len() >= 19, "{} fixtures", files.len());
    let mut settings = insta::Settings::clone_current();
    settings.set_snapshot_path("../../../fixtures/golden/write/snapshots");
    settings.set_prepend_module_to_snapshot(false);
    settings.bind(|| {
        for f in &files {
            let doc = open(f);
            let l = layout(&doc);
            assert!(!l.pages().is_empty());
            insta::assert_snapshot!(name(f), summary(&l));
        }
    });
}

fn oracle_counts() -> BTreeMap<String, usize> {
    let text =
        std::fs::read_to_string(golden_dir().join("oracle-pages.json")).expect("oracle json");
    let mut out = BTreeMap::new();
    // {"name": pages, ...}: a flat object of numbers.
    for part in text
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .split(',')
    {
        let mut kv = part.split(':');
        let (Some(k), Some(v)) = (kv.next(), kv.next()) else {
            continue;
        };
        let k = k.trim().trim_matches('"').to_owned();
        if let Ok(v) = v.trim().parse::<usize>() {
            out.insert(k, v);
        }
    }
    out
}

#[test]
fn page_counts_match_libreoffice() {
    let oracle = oracle_counts();
    let files = corpus();
    let mut equal = 0usize;
    let mut report = String::new();
    for f in &files {
        let n = name(f);
        let want = *oracle
            .get(&n)
            .unwrap_or_else(|| panic!("{n} has no oracle count"));
        let got = layout(&open(f)).pages().len();
        if got == want {
            equal += 1;
        }
        report.push_str(&format!("{n}: kiln {got}, LibreOffice {want}\n"));
        assert!(
            got.abs_diff(want) <= 1,
            "{n}: {got} pages, LibreOffice {want}\n{report}"
        );
    }
    let share = equal as f64 / files.len() as f64;
    assert!(share >= 0.8, "only {:.0} % equal\n{report}", share * 100.0);
}

/// Regenerates `oracle-pages.json` with LibreOffice (needs soffice and pdfinfo).
#[test]
#[ignore = "needs LibreOffice; run with KILN_ORACLE=1 to regenerate the oracle counts"]
fn oracle() {
    if std::env::var_os("KILN_ORACLE").is_none() {
        return;
    }
    let out = std::env::temp_dir().join("kiln-write-oracle");
    let _ = std::fs::create_dir_all(&out);
    let mut counts = BTreeMap::new();
    for f in corpus() {
        let status = std::process::Command::new(repo().join("tools/oracle-soffice.sh"))
            .arg(&f)
            .arg("pdf")
            .arg(&out)
            .output()
            .expect("soffice");
        assert!(
            status.status.success(),
            "{}",
            String::from_utf8_lossy(&status.stderr)
        );
        let pdf = out.join(format!("{}.pdf", name(&f)));
        let info = std::process::Command::new("pdfinfo")
            .arg(&pdf)
            .output()
            .expect("pdfinfo");
        let text = String::from_utf8_lossy(&info.stdout);
        let pages = text
            .lines()
            .find_map(|l| l.strip_prefix("Pages:"))
            .and_then(|v| v.trim().parse::<usize>().ok())
            .expect("page count");
        counts.insert(name(&f), pages);
    }
    let body: Vec<String> = counts
        .iter()
        .map(|(k, v)| format!("  \"{k}\": {v}"))
        .collect();
    std::fs::write(
        golden_dir().join("oracle-pages.json"),
        format!("{{\n{}\n}}\n", body.join(",\n")),
    )
    .expect("write");
}

/// Fixture and page of every golden PNG.
const GOLDEN: [(&str, usize); 14] = [
    ("tables", 0),
    ("images-anchored", 0),
    ("images-anchored", 1),
    ("images-inline", 0),
    ("headers-footers", 0),
    ("footnotes", 0),
    ("numbering", 0),
    ("text-boxes", 0),
    ("sections", 2),
    ("paragraphs", 0),
    ("styles", 0),
    ("fields", 0),
    ("breaks", 0),
    ("tracked-changes", 0),
];

#[test]
fn golden_pages() {
    let mut failures = Vec::new();
    for (fixture, page) in GOLDEN {
        let path = repo().join(format!("fixtures/docx/features/{fixture}.docx"));
        let doc = open(&path);
        let mut l = layout(&doc);
        let img = render_page(&mut l, &doc, page, 96.0, false);
        let golden = golden_dir().join(format!("{fixture}-p{}.png", page + 1));
        if let Err(e) = kiln_raster::golden::check(&img, &golden) {
            failures.push(e);
        }
    }
    // The dark theme: auto-coloured text in ink on the dark page.
    let doc = open(&repo().join("fixtures/docx/features/tables.docx"));
    let mut l = layout(&doc);
    let img = render_page(&mut l, &doc, 0, 96.0, true);
    if let Err(e) = kiln_raster::golden::check(&img, &golden_dir().join("tables-p1-dark.png")) {
        failures.push(e);
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn fields_update_from_the_layout() {
    let doc = open(&repo().join("fixtures/docx/features/fields.docx"));
    let l = layout(&doc);
    let toc = kiln_doc_layout::fields::toc_entries(&l, &doc, None);
    assert!(toc.len() >= 2, "{toc:?}");
    assert!(toc.iter().all(|e| e.page == "1"), "{toc:?}");
    // Results that already agree with the layout are not rewritten; a DATE field gets today.
    let now = kiln_doc::fields::FieldDateTime {
        year: 2031,
        month: 2,
        day: 3,
        hour: 4,
        minute: 5,
        second: 6,
    };
    let updates = kiln_doc_layout::fields::field_updates(&l, &doc, Some(now));
    assert!(
        updates.iter().any(|u| u.result.contains("2031")),
        "{updates:?}"
    );
    assert!(updates.iter().all(|u| !u.result.is_empty()));
}
