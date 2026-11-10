// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert` for presentations: `.pptx`, `.pptm`, `.potx`, `.potm`, `.ppsx`
//! and `.ppsm` in and out. Every model part (presentation, masters, layouts, slides, notes,
//! comments, properties) is written from the model; parts the model does not cover, media,
//! charts and an unchanged theme are carried over byte for byte. The Compatibility Checker's
//! findings for the save are printed after the summary. See [`crate::convert`].
//!
//! `.pdf` out prints the presentation as File ▸ Export ▸ PDF does: full page slides,
//! `--handouts N` (1, 2, 3, 4, 6 or 9 per page, `--vertical` for columns), `--notes` (notes
//! pages) or `--outline`, with `--slides 1-3,5`, `--frame`, `--hidden` (hidden slides too),
//! `--grayscale` and `--untagged`.

use std::fmt::Write as _;
use std::path::Path;

use kiln_io_pptx::{
    CompatOptions, PptxKind, WriteOptions, WriteSource, compat_report, read_package, write_pptx,
};
use kiln_ooxml::opc::OpcPackage;
use kiln_protocol::print::{PrintSettings, SlideLayout, SlidePrint};

use crate::CliError;

/// The presentation format of a file extension.
pub(crate) fn deck_kind(ext: &str) -> Option<PptxKind> {
    PptxKind::ALL.into_iter().find(|k| k.extension() == ext)
}

fn failed(p: &Path, e: impl std::fmt::Display) -> CliError {
    CliError::Failed(format!("{}: {e}", p.display()))
}

/// Usage text of the PDF options.
pub const PDF_USAGE: &str = "kiln-cli convert <in.pptx> <out.pdf> [--handouts 1|2|3|4|6|9 [--vertical]|--notes|--outline] [--slides 1-3,5] [--frame] [--hidden] [--grayscale] [--untagged]";

/// The print settings of the options after the paths.
fn pdf_settings(args: &[String]) -> Result<PrintSettings, CliError> {
    let mut s = PrintSettings::default();
    let mut slides = SlidePrint::default();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--handouts" => {
                let v = it
                    .next()
                    .ok_or_else(|| CliError::usage("--handouts needs a number", PDF_USAGE))?;
                let n: u32 = v.parse().map_err(|_| {
                    CliError::usage(format!("--handouts: \"{v}\" is not a number"), PDF_USAGE)
                })?;
                if !matches!(n, 1 | 2 | 3 | 4 | 6 | 9) {
                    return Err(CliError::usage(
                        format!("--handouts takes 1, 2, 3, 4, 6 or 9, not {n}"),
                        PDF_USAGE,
                    ));
                }
                slides.layout = SlideLayout::Handouts;
                slides.per_page = n;
            }
            "--vertical" => slides.vertical = true,
            "--notes" => slides.layout = SlideLayout::NotesPages,
            "--outline" => slides.layout = SlideLayout::Outline,
            "--frame" => slides.frame = true,
            "--hidden" => slides.hidden = true,
            "--slides" => {
                s.pages = it
                    .next()
                    .ok_or_else(|| CliError::usage("--slides needs a value", PDF_USAGE))?
                    .clone();
            }
            "--grayscale" => s.grayscale = true,
            "--untagged" => s.tagged = false,
            other => {
                return Err(CliError::usage(
                    format!("unknown option {other}"),
                    PDF_USAGE,
                ));
            }
        }
    }
    s.slides = Some(slides);
    Ok(s)
}

/// Prints a presentation to PDF.
pub(crate) fn run_pdf(input: &Path, output: &Path, rest: &[String]) -> Result<String, CliError> {
    let settings = pdf_settings(rest)?;
    let bytes = std::fs::read(input).map_err(|e| failed(input, e))?;
    let started = std::time::Instant::now();
    let pres = kiln_io_pptx::read_pptx(&bytes)
        .map_err(|e| failed(input, e))?
        .presentation;
    let mut renderer = kiln_deck_layout::Renderer::new(
        kiln_deck_layout::fonts::default_fonts(),
        kiln_deck_layout::fonts::default_fonts(),
    );
    let title = input.file_stem().map(|n| n.to_string_lossy().into_owned());
    let (pdf, pages) = kiln_session::apps::deck::features::export::export_pdf(
        &mut renderer,
        &pres,
        &settings,
        title.as_deref(),
    )
    .map_err(|e| failed(input, e))?;
    std::fs::write(output, &pdf).map_err(|e| failed(output, e))?;
    Ok(format!(
        "{} -> {} ({} slides, {pages} pages, {} bytes, {:.1} ms)",
        input.display(),
        output.display(),
        pres.slides.len(),
        pdf.len(),
        started.elapsed().as_secs_f64() * 1000.0,
    ))
}

/// Converts a presentation into the format `target` names.
pub(crate) fn run(input: &Path, output: &Path, target: PptxKind) -> Result<String, CliError> {
    let bytes = std::fs::read(input).map_err(|e| failed(input, e))?;
    let started = std::time::Instant::now();
    let pkg = OpcPackage::open(&bytes).map_err(|e| failed(input, e))?;
    let read = read_package(&pkg).map_err(|e| failed(input, e))?;
    let source = WriteSource::from_read(&read).with_package(&pkg);
    // Saved under the extension it came with, a deck keeps its own kind: a macro-enabled deck
    // named .pptx keeps its macros.
    let same = input.extension() == output.extension();
    let options = WriteOptions {
        kind: (!same).then_some(target),
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

    #[test]
    fn prints_handouts_to_pdf() {
        let input = kiln_testing::fixtures::fixtures_dir().join("pptx/features/placeholders.pptx");
        let out =
            std::env::temp_dir().join(format!("kiln-convert-deck-{}.pdf", std::process::id()));
        let text = run_pdf(&input, &out, &["--handouts".into(), "3".into()]).unwrap();
        let pdf = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert!(pdf.starts_with(b"%PDF-"), "{text}");
        assert!(text.contains("11 slides, 4 pages"), "{text}");
        for bad in [&["--handouts", "5"][..], &["--handouts"], &["--what"]] {
            let args: Vec<String> = bad.iter().map(|s| (*s).to_owned()).collect();
            assert!(pdf_settings(&args).is_err(), "{bad:?}");
        }
    }
}
