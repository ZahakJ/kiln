// SPDX-License-Identifier: AGPL-3.0-or-later
//! Presentations: every `.odp` of the corpus (made by LibreOffice from the `.pptx` feature
//! files) reads, writes and reads back with the same text and facts, and every `.pptx` feature
//! file converts to `.odp` and back into the model with its text intact.

use std::path::PathBuf;

use kiln_io_odf::{WriteOptions, deck_facts, deck_text, read_odp, write_odp};

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

fn norm(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn overlap(a: &[String], b: &[String]) -> (f64, Vec<String>) {
    let mut m = std::collections::HashMap::new();
    for x in a {
        *m.entry(x.as_str()).or_insert(0i64) += 1;
    }
    let mut common = 0usize;
    let mut extra = Vec::new();
    for y in b {
        match m.get_mut(y.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                common += 1;
            }
            _ => extra.push(format!("+ {y}")),
        }
    }
    for (k, n) in m {
        for _ in 0..n {
            extra.push(format!("- {k}"));
        }
    }
    let d = a.len().max(b.len());
    (
        if d == 0 {
            1.0
        } else {
            common as f64 / d as f64
        },
        extra,
    )
}

fn dump(name: &std::ffi::OsStr, sub: &str, bytes: &[u8]) {
    if std::env::var_os("KILN_ODF_DUMP").is_some() {
        let o = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/scratch")
            .join(sub);
        std::fs::create_dir_all(&o).unwrap();
        std::fs::write(o.join(name), bytes).unwrap();
    }
}

#[test]
fn odp_fixtures_round_trip() {
    let all = files("odf/features", "odp");
    assert!(all.len() >= 19, "{}", all.len());
    let mut failures = Vec::new();
    for f in &all {
        let bytes = std::fs::read(f).unwrap();
        let a = read_odp(&bytes).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let out = write_odp(&a.presentation, &WriteOptions::default()).unwrap();
        dump(f.file_name().unwrap(), "rt", &out);
        let b = read_odp(&out).unwrap_or_else(|e| panic!("{} (written): {e}", f.display()));
        let (ta, tb) = (
            norm(&deck_text(&a.presentation)),
            norm(&deck_text(&b.presentation)),
        );
        let (s, extra) = overlap(&deck_facts(&a.presentation), &deck_facts(&b.presentation));
        eprintln!(
            "{} slides={} structural={s:.3}",
            f.display(),
            a.presentation.slides.len()
        );
        if ta != tb || s < 0.95 {
            failures.push(format!(
                "{}: text equal {} structural {s:.3}\n  {:#?}",
                f.display(),
                ta == tb,
                extra.iter().take(12).collect::<Vec<_>>()
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn pptx_features_convert_to_odp() {
    let mut failures = Vec::new();
    for f in files("pptx/features", "pptx") {
        let bytes = std::fs::read(&f).unwrap();
        let a = kiln_io_pptx::read_pptx(&bytes).unwrap();
        let out = write_odp(&a.presentation, &WriteOptions::default()).unwrap();
        dump(
            f.with_extension("odp").file_name().unwrap(),
            "from-pptx",
            &out,
        );
        let b = read_odp(&out).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        let (ta, tb) = (
            norm(&deck_text(&a.presentation)),
            norm(&deck_text(&b.presentation)),
        );
        if ta != tb {
            failures.push(format!("{}:\n  {ta}\n  {tb}", f.display()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
