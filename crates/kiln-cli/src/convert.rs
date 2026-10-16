// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert <in> <out>`: reads a document and saves it in the format the output's
//! extension names, the way Kiln saves it (File ▸ Save As). Word documents today: `.docx`,
//! `.docm`, `.dotx` and `.dotm` in and out (a macro-enabled document saved as `.docx` loses its
//! macros); PDF export arrives with the PDF writer (WP-67).
//!
//! Every part is written from the model (nothing is copied because it was not edited); parts the
//! model does not cover, media and an unchanged theme are carried over byte for byte. The
//! Compatibility Checker's findings for the save are printed after the summary.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use kiln_io_docx::{
    CompatOptions, DocxKind, WriteOptions, WriteSource, compat_report, read_package, w3cdtf,
    write_docx,
};
use kiln_ooxml::opc::OpcPackage;

use crate::CliError;

/// Usage text.
pub const USAGE: &str = "kiln-cli convert <in.docx> <out.docx|.docm|.dotx|.dotm>";

fn extension(p: &Path) -> String {
    p.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn word_kind(ext: &str) -> Option<DocxKind> {
    Some(match ext {
        "docx" => DocxKind::Document,
        "docm" => DocxKind::MacroEnabledDocument,
        "dotx" => DocxKind::Template,
        "dotm" => DocxKind::MacroEnabledTemplate,
        _ => return None,
    })
}

/// The current time as W3CDTF (`2026-10-16T09:30:00Z`).
fn now_w3cdtf() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    w3cdtf(secs)
}

/// Runs the subcommand.
///
/// # Errors
///
/// Usage errors, unsupported formats, unreadable input, unwritable output.
pub fn run(args: &[String]) -> Result<String, CliError> {
    let (input, output) = match args {
        [i, o] if !i.starts_with('-') && !o.starts_with('-') => {
            (PathBuf::from(i), PathBuf::from(o))
        }
        _ => {
            return Err(CliError::usage(
                "convert takes an input and an output file",
                USAGE,
            ));
        }
    };
    let (in_ext, out_ext) = (extension(&input), extension(&output));
    if word_kind(&in_ext).is_none() {
        return Err(CliError::usage(
            format!(
                "cannot convert .{in_ext} files yet (Word documents: .docx, .docm, .dotx, .dotm)"
            ),
            USAGE,
        ));
    }
    let Some(target) = word_kind(&out_ext) else {
        let hint = if out_ext == "pdf" {
            " (PDF export arrives with the PDF writer)"
        } else {
            ""
        };
        return Err(CliError::usage(
            format!("cannot write .{out_ext} files{hint}"),
            USAGE,
        ));
    };
    let bytes =
        std::fs::read(&input).map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let started = std::time::Instant::now();
    let pkg = OpcPackage::open(&bytes)
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let read =
        read_package(&pkg).map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let source = WriteSource::from_read(&read).with_package(&pkg);
    let options = WriteOptions {
        kind: Some(target),
        now: Some(now_w3cdtf()),
        ..WriteOptions::default()
    };
    let written = write_docx(&read.document, Some(&source), &options)
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let elapsed = started.elapsed();
    std::fs::write(&output, &written.bytes)
        .map_err(|e| CliError::Failed(format!("{}: {e}", output.display())))?;
    let parts = OpcPackage::open(&written.bytes).map_or(0, |p| p.len());
    let mut out = format!(
        "{} -> {} ({} -> {} bytes, {:.1} ms)\nparts: {parts}, written from the model: {}, carried over: {}",
        input.display(),
        output.display(),
        bytes.len(),
        written.bytes.len(),
        elapsed.as_secs_f64() * 1000.0,
        written.regenerated.len(),
        parts.saturating_sub(written.regenerated.len()),
    );
    for i in &written.issues {
        let _ = write!(out, "\nwarning: {i}");
    }
    let report = compat_report(
        &read.document,
        CompatOptions {
            target: Some(target),
            source: Some(read.kind),
            warnings: &read.warnings,
        },
    );
    for c in &report {
        let _ = write!(
            out,
            "\n{} ({}, {}×): {}",
            c.code,
            c.severity.as_str(),
            c.count,
            c.message
        );
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_are_stamped_now() {
        assert!(now_w3cdtf().starts_with("20") && now_w3cdtf().ends_with('Z'));
    }

    #[test]
    fn converts_a_fixture_and_refuses_other_formats() {
        let input =
            kiln_testing::fixtures::fixtures_dir().join("docx/features/tracked-changes.docx");
        let out = std::env::temp_dir().join(format!("kiln-convert-{}.docx", std::process::id()));
        let text = run(&[
            input.to_string_lossy().into_owned(),
            out.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(text.contains("written from the model"), "{text}");
        let back = kiln_io_docx::read_docx(&std::fs::read(&out).unwrap()).unwrap();
        assert!(!back.document.sections.is_empty());
        let _ = std::fs::remove_file(&out);
        assert!(run(&["a.docx".into()]).is_err());
        assert!(run(&["a.txt".into(), "b.docx".into()]).is_err());
        let pdf = run(&[input.to_string_lossy().into_owned(), "x.pdf".into()]).unwrap_err();
        assert!(pdf.to_string().contains("PDF"));
    }
}
