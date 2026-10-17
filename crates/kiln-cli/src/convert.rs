// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert <in> <out>`: reads a document and saves it in the format the output's
//! extension names, the way Kiln saves it (File ▸ Save As).
//!
//! - Word documents: `.docx`, `.docm`, `.dotx` and `.dotm` in and out (a macro-enabled document
//!   saved as `.docx` loses its macros); PDF export arrives with the PDF writer (WP-67). Every
//!   part is written from the model (nothing is copied because it was not edited); parts the
//!   model does not cover, media and an unchanged theme are carried over byte for byte. The
//!   Compatibility Checker's findings for the save are printed after the summary.
//! - Workbooks (WP-70): `.xlsx`, `.xlsm`, `.xltx`, `.xltm`, `.csv`, `.tsv` and `.json` (an array
//!   of objects) in; the same out. A workbook saved as `.xlsx` keeps every part the model does not
//!   cover; saved as `.csv`, `.tsv` or `.json` it writes its active sheet as Excel does (cells as
//!   displayed, Excel's quoting, UTF-8).
//! - Presentations (WP-79): `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx` and `.ppsm` in and out
//!   (a macro-enabled presentation saved without macros loses them); see
//!   [`crate::convert_deck`].

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
pub const USAGE: &str = "kiln-cli convert <in.docx|.xlsx|.pptx|.csv|.tsv|.json> <out.docx|.docm|.dotx|.dotm|.xlsx|.xlsm|.xltx|.xltm|.pptx|.pptm|.potx|.potm|.ppsx|.ppsm|.csv|.tsv|.json>";

/// Workbook formats `convert` reads and writes.
fn grid_format(ext: &str) -> bool {
    matches!(
        ext,
        "xlsx" | "xlsm" | "xltx" | "xltm" | "csv" | "tsv" | "json"
    )
}

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
pub(crate) fn now_w3cdtf() -> String {
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
    if grid_format(&in_ext) {
        if !grid_format(&out_ext) {
            return Err(CliError::usage(
                format!(
                    "cannot write a workbook as .{out_ext} (.xlsx, .xlsm, .xltx, .xltm, .csv, .tsv, .json)"
                ),
                USAGE,
            ));
        }
        return crate::convert_grid::run(&input, &output, &in_ext, &out_ext);
    }
    if crate::convert_deck::deck_kind(&in_ext).is_some() {
        let Some(target) = crate::convert_deck::deck_kind(&out_ext) else {
            return Err(CliError::usage(
                format!(
                    "cannot write a presentation as .{out_ext} (.pptx, .pptm, .potx, .potm, .ppsx, .ppsm)"
                ),
                USAGE,
            ));
        };
        return crate::convert_deck::run(&input, &output, target);
    }
    if word_kind(&in_ext).is_none() {
        return Err(CliError::usage(
            format!(
                "cannot convert .{in_ext} files yet (Word documents: .docx, .docm, .dotx, .dotm; workbooks: .xlsx, .xlsm, .xltx, .xltm, .csv, .tsv, .json; presentations: .pptx, .pptm, .potx, .potm, .ppsx, .ppsm)"
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
    // Every file of the output package (parts, relationships parts and the content types),
    // split by whether the writer produced it or copied it from the input unchanged.
    let (parts, carried) = OpcPackage::open(&written.bytes).map_or((0, 0), |p| {
        let names: Vec<&str> = std::iter::once(kiln_ooxml::opc::CONTENT_TYPES_PART)
            .chain(p.all_part_names())
            .collect();
        let carried = names
            .iter()
            .filter(|n| !written.regenerated.iter().any(|r| r == *n))
            .count();
        (names.len(), carried)
    });
    let mut out = format!(
        "{} -> {} ({} -> {} bytes, {:.1} ms)\nparts: {parts}, written from the model: {}, carried over: {carried}",
        input.display(),
        output.display(),
        bytes.len(),
        written.bytes.len(),
        elapsed.as_secs_f64() * 1000.0,
        parts - carried,
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

    /// The summary's part counts are consistent: written plus carried over is every part.
    fn assert_counts_add_up(text: &str) {
        let line = text
            .lines()
            .find(|l| l.starts_with("parts: "))
            .expect("summary line");
        let numbers: Vec<usize> = line
            .split(", ")
            .map(|f| f.rsplit(": ").next().unwrap().parse().unwrap())
            .collect();
        assert_eq!(numbers.len(), 3, "{line}");
        assert_eq!(numbers[1] + numbers[2], numbers[0], "{line}");
        assert!(numbers[1] > 0, "{line}");
    }

    #[test]
    fn part_counts_cover_every_file_of_the_package() {
        // Relationships parts and the content types count as parts too, so the parts written
        // from the model never outnumber the parts of the package.
        let input =
            kiln_testing::fixtures::fixtures_dir().join("docx/features/tracked-changes.docx");
        let out =
            std::env::temp_dir().join(format!("kiln-convert-count-{}.docx", std::process::id()));
        let text = run(&[
            input.to_string_lossy().into_owned(),
            out.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let written = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert_counts_add_up(&text);
        let files = OpcPackage::open(&written).unwrap().all_part_names().count() + 1;
        assert!(text.contains(&format!("parts: {files}, ")), "{text}");
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
        assert_counts_add_up(&text);
        let back = kiln_io_docx::read_docx(&std::fs::read(&out).unwrap()).unwrap();
        assert!(!back.document.sections.is_empty());
        let _ = std::fs::remove_file(&out);
        assert!(run(&["a.docx".into()]).is_err());
        assert!(run(&["a.txt".into(), "b.docx".into()]).is_err());
        let pdf = run(&[input.to_string_lossy().into_owned(), "x.pdf".into()]).unwrap_err();
        assert!(pdf.to_string().contains("PDF"));
    }
}
