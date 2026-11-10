// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert` to and from Kiln's own bundles (`.kwrite`, `.kgrid`, `.kdeck`):
//! the document is opened and saved by the session, as File ▸ Save As does, so a bundle made
//! from a Word file and saved as Word again keeps what the model does not cover.

use std::fmt::Write as _;
use std::path::Path;

use kiln_protocol::{Command, Event, FileFormat, OpenHint, SuiteCommand};
use kiln_session::{NativeHost, Session};

use crate::CliError;

/// Whether a conversion involves a bundle.
#[must_use]
pub fn involves_bundle(in_ext: &str, out_ext: &str) -> bool {
    [in_ext, out_ext]
        .into_iter()
        .any(|e| FileFormat::from_extension(e).is_some_and(FileFormat::is_bundle))
}

/// Converts `input` to `output` through the session.
///
/// # Errors
///
/// An unreadable or damaged input (a bundle of a newer Kiln says so), an output format the
/// document cannot be saved in, an unwritable output.
pub fn run(input: &Path, output: &Path) -> Result<String, CliError> {
    let failed =
        |p: &Path, e: &dyn std::fmt::Display| CliError::Failed(format!("{}: {e}", p.display()));
    let bytes = std::fs::read(input).map_err(|e| failed(input, &e))?;
    let started = std::time::Instant::now();
    let mut s = Session::new(Box::new(NativeHost));
    let summary = s
        .open(
            &bytes,
            OpenHint {
                path: Some(input.to_string_lossy().into_owned()),
                ..OpenHint::default()
            },
        )
        .map_err(|e| failed(input, &e))?;
    let format = FileFormat::from_path(&output.to_string_lossy())
        .ok_or_else(|| CliError::Failed(format!("{}: no known extension", output.display())))?;
    if !summary.save_formats.contains(&format) {
        let list: Vec<String> = summary
            .save_formats
            .iter()
            .map(|f| format!(".{}", f.extension()))
            .collect();
        return Err(CliError::Failed(format!(
            "cannot save a {:?} document as .{} ({})",
            summary.kind,
            format.extension(),
            list.join(", ")
        )));
    }
    s.execute(
        Some(summary.doc),
        Command::Suite(SuiteCommand::SaveAs {
            path: Some(output.to_string_lossy().into_owned()),
            format: Some(format),
        }),
    )
    .map_err(|e| failed(output, &e))?;
    for e in s.poll() {
        if let Event::Error { error, .. } = e {
            return Err(failed(output, &error));
        }
    }
    let written = std::fs::metadata(output).map_or(0, |m| m.len());
    let mut out = format!(
        "{} -> {} ({} -> {written} bytes, {:.1} ms)\n{:?} document",
        input.display(),
        output.display(),
        bytes.len(),
        started.elapsed().as_secs_f64() * 1000.0,
        summary.kind,
    );
    if format.is_bundle()
        && let Ok(b) = kiln_storage::Bundle::from_bytes(&std::fs::read(output).unwrap_or_default())
    {
        let _ = write!(
            out,
            ", bundle version {}, {} version(s), passthrough: {}",
            kiln_storage::BUNDLE_VERSION,
            b.versions.len(),
            if b.passthrough.is_empty() {
                "none".to_owned()
            } else {
                b.passthrough.keys().cloned().collect::<Vec<_>>().join(", ")
            }
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_testing::fidelity::text_score;
    use kiln_testing::fixtures::FixtureFormat;

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kiln-bundle-{}-{name}", std::process::id()))
    }

    /// Every docx, xlsx and pptx feature fixture survives a trip through its bundle with the
    /// same text (the fidelity scorer's text score).
    #[test]
    fn every_feature_fixture_round_trips_through_its_bundle() {
        let dir = kiln_testing::fixtures::fixtures_dir();
        let adapters = kiln_testing::formats::registered();
        let mut checked = 0;
        for (sub, ext, bundle, format) in [
            ("docx/features", "docx", "kwrite", FixtureFormat::Docx),
            ("xlsx/features", "xlsx", "kgrid", FixtureFormat::Xlsx),
            ("pptx/features", "pptx", "kdeck", FixtureFormat::Pptx),
        ] {
            let adapter = adapters
                .iter()
                .find(|a| a.format() == format)
                .expect("an adapter");
            let mut files: Vec<_> = std::fs::read_dir(dir.join(sub))
                .unwrap()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == ext))
                .collect();
            files.sort();
            for f in files {
                let stem = f.file_stem().unwrap().to_string_lossy().into_owned();
                let mid = tmp(&format!("{stem}.{bundle}"));
                let back = tmp(&format!("{stem}.{ext}"));
                run(&f, &mid).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
                run(&mid, &back).unwrap_or_else(|e| panic!("{}: {e}", mid.display()));
                let a = adapter.snapshot(&std::fs::read(&f).unwrap()).unwrap();
                let b = adapter
                    .snapshot(&std::fs::read(&back).unwrap())
                    .unwrap_or_else(|e| panic!("{}: {e}", back.display()));
                let _ = std::fs::remove_file(&mid);
                let _ = std::fs::remove_file(&back);
                let score = text_score(&a.text, &b.text);
                assert!(
                    (score - 1.0).abs() < f64::EPSILON,
                    "{}: text score {score:.3}",
                    f.display()
                );
                if format == FixtureFormat::Xlsx {
                    for (k, cell) in &a.cells {
                        let other = b
                            .cells
                            .get(k)
                            .unwrap_or_else(|| panic!("{}: {k}", f.display()));
                        assert_eq!(other.formula, cell.formula, "{}: {k}", f.display());
                        // A formula the file saved without a result gets one.
                        if !cell.value.is_empty() {
                            assert_eq!(other.value, cell.value, "{}: {k}", f.display());
                        }
                    }
                }
                checked += 1;
            }
        }
        assert!(checked >= 40, "{checked} fixtures");
    }

    #[test]
    fn a_bundle_reports_what_it_holds_and_refuses_another_kind() {
        let input = kiln_testing::fixtures::fixtures_dir().join("docx/features/tables.docx");
        let out = tmp("tables.kwrite");
        let text = run(&input, &out).unwrap();
        assert!(text.contains("bundle version 1"), "{text}");
        assert!(text.contains("original.docx"), "{text}");
        let e = run(&out, &tmp("x.kgrid")).unwrap_err().to_string();
        assert!(e.contains("cannot save a Write document as .kgrid"), "{e}");
        let _ = std::fs::remove_file(&out);
        assert!(involves_bundle("docx", "kwrite") && !involves_bundle("docx", "pdf"));
    }
}
