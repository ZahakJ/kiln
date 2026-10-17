// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every DOCX feature fixture exported to PDF and checked with poppler-utils (the oracle; lopdf
//! is not a dependency):
//!
//! - `pdfinfo`: the page count equals the layout's, the file is tagged, the producer is Kiln;
//! - `pdffonts`: every font is embedded and subset;
//! - `pdftotext`: at least 99 % of the model's words come back out (text is real text);
//! - `pdftoppm`: every page renders without error.
//!
//! The poppler tools are required (CI installs `poppler-utils`); set `KILN_SKIP_POPPLER=1` to
//! skip on a machine without them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use kiln_io_pdf::write::{WritePrint, print_layout};
use kiln_io_pdf::{PdfOptions, export_report};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(repo().join("fixtures/docx/features"))
        .expect("fixtures/docx/features")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "docx"))
        .collect();
    out.sort();
    assert!(out.len() >= 15, "the feature corpus is there");
    out
}

fn poppler() -> bool {
    if std::env::var_os("KILN_SKIP_POPPLER").is_some() {
        return false;
    }
    assert!(
        Command::new("pdfinfo").arg("-v").output().is_ok(),
        "poppler-utils (pdfinfo, pdffonts, pdftotext, pdftoppm) is required; \
         KILN_SKIP_POPPLER=1 skips these checks"
    );
    true
}

fn tool(name: &str, args: &[&str]) -> String {
    let out = Command::new(name).args(args).output().expect(name);
    assert!(
        out.status.success(),
        "{name} {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn words(text: &str) -> HashMap<String, usize> {
    // Soft hyphens are invisible unless a line breaks there; note reference marks print as
    // superscripts glued to the word before them ("anim.2"), so letters and digits split.
    let text = text.replace('\u{AD}', "");
    let mut m = HashMap::new();
    let mut word = String::new();
    let mut digits = false;
    let mut flush = |w: &mut String| {
        if !w.is_empty() {
            *m.entry(std::mem::take(w)).or_insert(0) += 1;
        }
    };
    for c in text.chars() {
        if c.is_alphanumeric() {
            let d = c.is_ascii_digit();
            if d != digits {
                flush(&mut word);
                digits = d;
            }
            word.extend(c.to_lowercase());
        } else if !matches!(c, '\'' | '\u{2019}' | '-' | '\u{2011}') {
            flush(&mut word);
        }
    }
    flush(&mut word);
    m
}

/// Words of the model found in the PDF, and words of the model.
fn counts(model: &str, pdf: &str) -> (usize, usize) {
    let want = words(model);
    let have = words(pdf);
    let total = want.values().sum();
    let found = want
        .iter()
        .map(|(w, n)| (*n).min(have.get(w).copied().unwrap_or(0)))
        .sum();
    (found, total)
}

/// The share of the model's words found in the PDF's text (with multiplicity).
fn recall(model: &str, pdf: &str) -> f64 {
    let want = words(model);
    let have = words(pdf);
    let total: usize = want.values().sum();
    if total == 0 {
        return 1.0;
    }
    let found: usize = want
        .iter()
        .map(|(w, n)| (*n).min(have.get(w).copied().unwrap_or(0)))
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let r = found as f64 / total as f64;
    r
}

fn missing(model: &str, pdf: &str) -> Vec<String> {
    let have = words(pdf);
    let mut out: Vec<String> = words(model)
        .into_iter()
        .filter(|(w, n)| have.get(w).copied().unwrap_or(0) < *n)
        .map(|(w, _)| w)
        .collect();
    out.sort();
    out
}

fn model_text(doc: &kiln_doc::Document) -> String {
    // Body paragraphs: deleted text left out; hidden text (styles) is counted though it does
    // not print, which the corpus-wide 99 % allows for (markup prints it struck through, but
    // pdftotext cannot tell), table cells included.
    let mut out = String::new();
    for p in doc.body_paragraphs() {
        out.push_str(&p.content.plain_text(true));
        out.push('\n');
    }
    out
}

#[test]
fn every_feature_fixture_exports_a_valid_tagged_pdf_with_real_text() {
    if !poppler() {
        return;
    }
    let dir = std::env::temp_dir().join(format!("kiln-pdf-fixtures-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut report = Vec::new();
    let (mut all_found, mut all_total) = (0usize, 0usize);
    let mut ua = 0usize;
    for path in fixtures() {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let bytes = std::fs::read(&path).unwrap();
        let doc = kiln_io_docx::read_docx(&bytes).expect("reads").document;
        let layout = print_layout(&doc, kiln_doc_layout::default_fonts(), true, None);
        let printable = WritePrint::new(&doc, &layout, Some(name.clone()));
        let exported = export_report(&printable, &PdfOptions::default()).expect("exports");
        let pdf = exported.pdf;
        assert!(pdf.starts_with(b"%PDF-"), "{name}");
        let out = dir.join(format!("{name}.pdf"));
        std::fs::write(&out, &pdf).unwrap();
        let file = out.to_string_lossy().into_owned();

        let info = tool("pdfinfo", &[&file]);
        let pages: usize = info
            .lines()
            .find_map(|l| l.strip_prefix("Pages:"))
            .and_then(|v| v.trim().parse().ok())
            .expect("page count");
        assert_eq!(pages, layout.pages().len(), "{name}: page count");
        assert!(
            info.lines()
                .any(|l| l.starts_with("Tagged:") && l.contains("yes")),
            "{name}: tagged\n{info}"
        );
        assert!(
            info.lines()
                .any(|l| l.starts_with("Producer:") && l.contains("Kiln")),
            "{name}: producer\n{info}"
        );

        let fonts = tool("pdffonts", &[&file]);
        let rows: Vec<&str> = fonts.lines().skip(2).collect();
        assert!(
            !rows.is_empty() || name == "images-inline",
            "{name}: no fonts\n{fonts}"
        );
        for row in &rows {
            // name type encoding emb sub uni object ID
            let cols: Vec<&str> = row.split_whitespace().collect();
            let n = cols.len();
            assert!(n >= 7, "{name}: {row}");
            let (emb, sub) = (cols[n - 5], cols[n - 4]);
            assert_eq!((emb, sub), ("yes", "yes"), "{name}: {row}");
        }

        let text = tool("pdftotext", &["-enc", "UTF-8", &file, "-"]);
        let r = recall(&model_text(&doc), &text);
        let model = model_text(&doc);
        let (found, total) = counts(&model, &text);
        all_found += found;
        all_total += total;
        // Per file a little slack: text in scripts the bundled fonts lack (Arabic in `rtl`)
        // prints as .notdef boxes, and poppler drops the odd character of such runs.
        assert!(
            r >= 0.97,
            "{name}: only {r:.3} of the words extracted; missing {:?}",
            missing(&model_text(&doc), &text)
        );

        let prefix = dir.join(&name).to_string_lossy().into_owned();
        tool("pdftoppm", &["-r", "20", "-png", &file, &prefix]);
        ua += usize::from(exported.pdf_ua);
        report.push(format!(
            "{name}: {pages} pages, {} fonts, recall {r:.4}, PDF/UA-1 {}",
            rows.len(),
            if exported.pdf_ua {
                "yes".to_owned()
            } else {
                format!("no: {}", exported.ua_issues.join("; "))
            }
        ));
    }
    eprintln!("{}", report.join("\n"));
    #[allow(clippy::cast_precision_loss)]
    let overall = all_found as f64 / all_total.max(1) as f64;
    eprintln!("corpus: {all_found} of {all_total} words extracted ({overall:.4})");
    assert!(overall >= 0.99, "corpus recall {overall:.4}");
    // The fixtures that cannot be PDF/UA-1 are the ones whose content prevents it: pictures
    // without alt text (images-anchored, images-inline) and scripts the bundled fonts lack
    // (breaks: CJK and emoji; rtl: Arabic).
    assert!(ua >= 15, "only {ua} fixtures came out as PDF/UA-1");
    if std::env::var_os("KILN_KEEP_PDF").is_none() {
        let _ = std::fs::remove_dir_all(&dir);
    }
}
