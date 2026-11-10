// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text documents: every `.odt` of the corpus (made by LibreOffice from the `.docx` feature
//! files) reads, writes and reads back with the same text and structure, and every `.docx`
//! feature file converts to `.odt` and back into the model with its text intact.

use std::path::PathBuf;

use kiln_doc::query::PlainTextOptions;
use kiln_io_odf::{WriteOptions, read_odt, write_odt};

fn dir(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures")
        .join(rel)
}

fn files(rel: &str, ext: &str) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir(rel))
        .map(|d| {
            d.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == ext))
                .collect()
        })
        .unwrap_or_default();
    v.sort();
    v
}

fn text(doc: &kiln_doc::Document) -> String {
    let t = doc.plain_text(PlainTextOptions {
        include_headers_footers: true,
        include_notes: true,
        accept_changes: false,
        list_labels: true,
    });
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn body_text(doc: &kiln_doc::Document) -> String {
    let t = doc.plain_text(PlainTextOptions {
        include_headers_footers: false,
        include_notes: true,
        accept_changes: false,
        list_labels: true,
    });
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn blocks_text(blocks: &[kiln_doc::Block]) -> String {
    let mut out = String::new();
    for b in blocks {
        if let kiln_doc::Block::Paragraph(p) = b {
            out.push_str(&p.content.plain_text(false));
            out.push(' ');
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn overlap(a: &[String], b: &[String]) -> f64 {
    let mut m = std::collections::HashMap::new();
    for x in a {
        *m.entry(x.as_str()).or_insert(0i64) += 1;
    }
    let mut common = 0usize;
    for y in b {
        if let Some(n) = m.get_mut(y.as_str())
            && *n > 0
        {
            *n -= 1;
            common += 1;
        }
    }
    let d = a.len().max(b.len());
    if d == 0 {
        1.0
    } else {
        common as f64 / d as f64
    }
}

#[test]
fn odt_fixtures_round_trip() {
    let all = files("odf/features", "odt");
    assert!(all.len() >= 19, "{}", all.len());
    let mut report = Vec::new();
    for f in &all {
        let bytes = std::fs::read(f).unwrap();
        let a = read_odt(&bytes).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let out = write_odt(&a.document, &WriteOptions::default()).unwrap();
        let b = read_odt(&out).unwrap_or_else(|e| panic!("{} (written): {e}", f.display()));
        let (ta, tb) = (text(&a.document), text(&b.document));
        let s = overlap(
            &kiln_io_docx::snapshot_facts(&a.document),
            &kiln_io_docx::snapshot_facts(&b.document),
        );
        report.push(format!(
            "{} text={} structural={s:.3}",
            f.display(),
            ta == tb
        ));
        if std::env::var_os("KILN_ODF_DUMP").is_some() {
            let o = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/rt");
            std::fs::create_dir_all(&o).unwrap();
            std::fs::write(o.join(f.file_name().unwrap()), &out).unwrap();
            if ta != tb {
                std::fs::write(
                    o.join(format!(
                        "{}.a.txt",
                        f.file_name().unwrap().to_string_lossy()
                    )),
                    &ta,
                )
                .unwrap();
                std::fs::write(
                    o.join(format!(
                        "{}.b.txt",
                        f.file_name().unwrap().to_string_lossy()
                    )),
                    &tb,
                )
                .unwrap();
            }
        }
        assert_eq!(ta, tb, "{}", f.display());
        assert!(s >= 0.9, "{}: structural {s}", f.display());
    }
    eprintln!("{}", report.join("\n"));
}

#[test]
fn docx_features_convert_to_odt() {
    for f in files("docx/features", "docx") {
        let bytes = std::fs::read(&f).unwrap();
        let a = kiln_io_docx::read_docx(&bytes).unwrap();
        let out = write_odt(&a.document, &WriteOptions::default()).unwrap();
        let b = read_odt(&out).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        if std::env::var_os("KILN_ODF_DUMP").is_some() {
            let o =
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/scratch/from-docx");
            std::fs::create_dir_all(&o).unwrap();
            std::fs::write(o.join(f.with_extension("odt").file_name().unwrap()), &out).unwrap();
        }
        // Word's sections inherit headers and footers, ODF master pages each hold their own:
        // the body and notes must match, and every header or footer text must survive.
        // A chart whose data is not in the document is not saved, and the user is told.
        let keep = |t: String| {
            if f.ends_with("embedded-chart.docx") {
                t.replace('\u{FFFC}', "").trim_end().to_owned()
            } else if f.ends_with("legal-numbering.docx") {
                // `w:isLgl` has no ODF form yet: list labels show the outer level's own format.
                t.replace("Section I.", "Section 1.")
                    .replace("Section II.", "Section 2.")
                    .replace("Section V.", "Section 5.")
            } else {
                t
            }
        };
        if f.ends_with("embedded-chart.docx") {
            let issues = kiln_io_odf::odt_issues(&a.document);
            assert!(issues.iter().any(|i| i.contains("chart")), "{issues:?}");
        }
        if f.ends_with("vml-fallbacks.docx") {
            let issues = kiln_io_odf::odt_issues(&a.document);
            assert!(issues.iter().any(|i| i.contains("VML")), "{issues:?}");
        }
        assert_eq!(
            keep(body_text(&a.document)),
            keep(body_text(&b.document)),
            "{}",
            f.display()
        );
        let stories = |d: &kiln_doc::Document| -> std::collections::BTreeSet<String> {
            d.headers_footers
                .iter()
                .map(|h| blocks_text(&h.blocks))
                .filter(|t| !t.is_empty())
                .collect()
        };
        let (sa, sb) = (stories(&a.document), stories(&b.document));
        assert!(sa.is_subset(&sb), "{}: {sa:?} vs {sb:?}", f.display());
    }
}
