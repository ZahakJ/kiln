// SPDX-License-Identifier: AGPL-3.0-or-later
//! The DOCX fixture corpus through the layout engine:
//!
//! - **Snapshots** (`fixtures/golden/write/snapshots/*.snap`): page count and the first and last
//!   words of every page, for every `fixtures/docx/{features,manual}` file.
//! - **Oracle**: page counts against LibreOffice's (`fixtures/golden/write/oracle-pages.json`,
//!   written by `KILN_ORACLE=1 cargo nextest run -p kiln-doc-layout --run-ignored all oracle`
//!   with `tools/oracle-soffice.sh` → PDF → page count). At least 80 % must be equal and all
//!   within one page; the known difference is where Word itself differs from LibreOffice
//!   (Word starts an odd-page section with a blank page). The same run writes
//!   `oracle-breaks.json`, the first words of each LibreOffice page, against which every page
//!   must hold LibreOffice's lines give or take one. LibreOffice must see the bundled fonts
//!   (`assets/fonts`, installed or added with `FONTCONFIG_FILE`), or it measures others.
//! - **Golden PNGs** (`fixtures/golden/write/*.png`): pages of 12 fixtures at 96 dpi, compared
//!   with a 0.3 % pixel tolerance. `KILN_UPDATE_GOLDEN=1` rewrites them; review the images.

mod common;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use common::*;
use kiln_doc_layout::snapshot::{page_line_words, summary};

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

/// A word as both sides compare it: letters and digits only, lower case (LibreOffice's text
/// export and Kiln's lines differ in quotes, dashes and punctuation spacing).
fn normal(w: &str) -> String {
    w.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Where `needle` occurs in `hay` from `from` on (empty words are skipped on both sides).
fn find_words(hay: &[String], needle: &[String], from: usize) -> Option<usize> {
    let needle: Vec<&String> = needle.iter().filter(|w| !w.is_empty()).collect();
    if needle.is_empty() {
        return None;
    }
    (from..hay.len()).find(|&i| {
        let mut j = i;
        for n in &needle {
            while j < hay.len() && hay[j].is_empty() {
                j += 1;
            }
            if j >= hay.len() || &hay[j] != *n {
                return false;
            }
            j += 1;
        }
        true
    })
}

/// `oracle-breaks.json`: for each fixture, the first words of each of LibreOffice's pages
/// after the first.
fn oracle_breaks() -> BTreeMap<String, Vec<String>> {
    let text =
        std::fs::read_to_string(golden_dir().join("oracle-breaks.json")).expect("oracle json");
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let Some((k, v)) = line.trim().trim_end_matches(',').split_once(": [") else {
            continue;
        };
        let k = k.trim().trim_matches('"').to_owned();
        let items: Vec<String> = v
            .trim_end_matches(']')
            .split("\", \"")
            .map(|s| s.trim().trim_matches('"').to_owned())
            .collect();
        out.insert(k, items);
    }
    out
}

/// Pagination against LibreOffice line by line (`write.layout.pagination-fidelity`:
/// within one line per page for documents in the standard fonts): for every fixture with the
/// same page count, every page holds the lines LibreOffice's page holds, give or take one. The
/// oracle needs LibreOffice with the bundled fonts installed or on `FONTCONFIG_FILE`, else it
/// measures other fonts.
#[test]
fn page_breaks_match_libreoffice_within_a_line() {
    let oracle = oracle_breaks();
    let counts = oracle_counts();
    let mut checked = 0usize;
    let mut exact = 0usize;
    let mut report = String::new();
    for f in corpus() {
        let n = name(&f);
        let l = layout(&open(&f));
        if counts.get(&n) != Some(&l.pages().len()) {
            continue;
        }
        let Some(starts) = oracle.get(&n) else {
            continue;
        };
        // Kiln's words with the index of their line, and the first line of each page.
        let mut words = Vec::new();
        let mut line_of = Vec::new();
        let mut first_line = Vec::new();
        let mut line = 0usize;
        for p in l.pages() {
            first_line.push(line);
            for lw in page_line_words(p) {
                for w in lw {
                    words.push(normal(&w));
                    line_of.push(line);
                }
                line += 1;
            }
        }
        // Δ: how many lines from Kiln's page start LibreOffice's page starts; a page holds
        // within one line of LibreOffice's when Δ changes by at most one from its first line
        // to its last (the start of the next page).
        let mut previous = 0i64;
        for (k, start) in starts.iter().enumerate() {
            if start.is_empty() {
                continue;
            }
            let page = k + 1;
            let needle: Vec<String> = start.split(' ').map(str::to_owned).collect();
            // The occurrence nearest Kiln's own page start (the same words may recur).
            let around = first_line[page].saturating_sub(6);
            let from = line_of.iter().position(|&x| x >= around).unwrap_or(0);
            let mut best: Option<usize> = None;
            let mut i = from;
            while let Some(at) = find_words(&words, &needle, i) {
                if line_of[at] > first_line[page] + 6 {
                    break;
                }
                let d = |a: usize| (line_of[a] as i64 - first_line[page] as i64).abs();
                if best.is_none_or(|b| d(at) < d(b)) {
                    best = Some(at);
                }
                i = at + 1;
            }
            let Some(at) = best else {
                report.push_str(&format!("{n} p{}: \"{start}\" not found\n", page + 1));
                continue;
            };
            let delta = line_of[at] as i64 - first_line[page] as i64;
            let per_page = delta - previous;
            previous = delta;
            checked += 1;
            if delta == 0 {
                exact += 1;
            }
            report.push_str(&format!(
                "{n} p{}: starts {delta:+} lines from LibreOffice's, page {} holds {per_page:+}\n",
                page + 1,
                page
            ));
            assert!(
                per_page.abs() <= 1,
                "{n}: page {page} holds {per_page:+} lines against LibreOffice's\n{report}"
            );
        }
    }
    assert!(
        checked >= 10,
        "only {checked} page starts compared\n{report}"
    );
    eprintln!("{exact} of {checked} page starts on LibreOffice's line\n{report}");
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
    let mut breaks: BTreeMap<String, Vec<String>> = BTreeMap::new();
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
        // Where each later page starts: the first line of LibreOffice's page whose first words
        // are words of Kiln's body text (headers, footers and notes are not).
        let kiln = layout(&open(&f));
        let words: Vec<String> = kiln
            .pages()
            .iter()
            .flat_map(|p| page_line_words(p).into_iter().flatten())
            .map(|w| normal(&w))
            .collect();
        let mut starts = Vec::new();
        for page in 2..=pages {
            let text = std::process::Command::new("pdftotext")
                .args(["-f", &page.to_string(), "-l", &page.to_string(), "-layout"])
                .arg(&pdf)
                .arg("-")
                .output()
                .expect("pdftotext");
            let text = String::from_utf8_lossy(&text.stdout).into_owned();
            let found = text.lines().find_map(|line| {
                let w: Vec<String> = line.split_whitespace().map(normal).collect();
                (w.len() >= 3 && find_words(&words, &w[..w.len().min(8)], 0).is_some())
                    .then(|| w[..w.len().min(8)].join(" "))
            });
            starts.push(found.unwrap_or_default());
        }
        breaks.insert(name(&f), starts);
    }
    let body: Vec<String> = breaks
        .iter()
        .map(|(k, v)| {
            let items: Vec<String> = v
                .iter()
                .map(|s| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")))
                .collect();
            format!("  \"{k}\": [{}]", items.join(", "))
        })
        .collect();
    std::fs::write(
        golden_dir().join("oracle-breaks.json"),
        format!("{{\n{}\n}}\n", body.join(",\n")),
    )
    .expect("write");
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
