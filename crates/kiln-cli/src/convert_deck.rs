// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert` for presentations (WP-79): `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`
//! and `.ppsm` in and out. Every model part (presentation, masters, layouts, slides, notes,
//! comments, properties) is written from the model; parts the model does not cover, media,
//! charts and an unchanged theme are carried over byte for byte. The Compatibility Checker's
//! findings for the save are printed after the summary. See [`crate::convert`].

use std::fmt::Write as _;
use std::path::Path;

use kiln_io_pptx::{
    CompatOptions, PptxKind, WriteOptions, WriteSource, compat_report, read_package, write_pptx,
};
use kiln_ooxml::opc::OpcPackage;

use crate::CliError;

/// The presentation format of a file extension.
pub(crate) fn deck_kind(ext: &str) -> Option<PptxKind> {
    PptxKind::ALL.into_iter().find(|k| k.extension() == ext)
}

fn failed(p: &Path, e: impl std::fmt::Display) -> CliError {
    CliError::Failed(format!("{}: {e}", p.display()))
}

/// Converts a presentation into the format `target` names.
pub(crate) fn run(input: &Path, output: &Path, target: PptxKind) -> Result<String, CliError> {
    let bytes = std::fs::read(input).map_err(|e| failed(input, e))?;
    let started = std::time::Instant::now();
    let pkg = OpcPackage::open(&bytes).map_err(|e| failed(input, e))?;
    let read = read_package(&pkg).map_err(|e| failed(input, e))?;
    let source = WriteSource::from_read(&read).with_package(&pkg);
    let options = WriteOptions {
        kind: Some(target),
        now: Some(crate::convert::now_w3cdtf()),
        ..WriteOptions::default()
    };
    let written =
        write_pptx(&read.presentation, Some(&source), &options).map_err(|e| failed(input, e))?;
    let elapsed = started.elapsed();
    std::fs::write(output, &written.bytes).map_err(|e| failed(output, e))?;
    // Every file of the output package, split by whether the writer produced it or carried it
    // over from the input unchanged.
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
        "{} -> {} ({} -> {} bytes, {:.1} ms)\n{} slides; parts: {parts}, written from the model: {}, carried over: {carried}",
        input.display(),
        output.display(),
        bytes.len(),
        written.bytes.len(),
        elapsed.as_secs_f64() * 1000.0,
        read.presentation.slides.len(),
        parts - carried,
    );
    for i in &written.issues {
        let _ = write!(out, "\nwarning: {i}");
    }
    let report = compat_report(
        &read.presentation,
        CompatOptions {
            target: Some(target),
            source: Some(read.kind),
            extras: Some(&read.extras),
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
    fn converts_a_presentation_and_reads_it_back() {
        let input = kiln_testing::fixtures::fixtures_dir().join("pptx/features/animations.pptx");
        let out =
            std::env::temp_dir().join(format!("kiln-convert-deck-{}.pptx", std::process::id()));
        let text = run(&input, &out, PptxKind::Presentation).unwrap();
        assert!(text.contains("written from the model"), "{text}");
        let back = kiln_io_pptx::read_pptx(&std::fs::read(&out).unwrap()).unwrap();
        let _ = std::fs::remove_file(&out);
        let orig = kiln_io_pptx::read_pptx(&std::fs::read(&input).unwrap()).unwrap();
        assert_eq!(
            back.presentation.slides.len(),
            orig.presentation.slides.len()
        );
        assert_eq!(
            kiln_io_pptx::plain_text(&back),
            kiln_io_pptx::plain_text(&orig)
        );
        assert_eq!(deck_kind("potx"), Some(PptxKind::Template));
        assert_eq!(deck_kind("docx"), None);
    }
}
