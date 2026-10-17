// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert <in.docx> <out.pdf> [options]` (WP-67): a Word document laid out and
//! exported to PDF exactly as Kiln Write prints it (File ▸ Export ▸ PDF).
//!
//! Options: `--pages 1-3,5`, `--odd`/`--even`, `--copies N`, `--fit letter|a4|legal|a3|a5`,
//! `--scale 0.5`, `--per-sheet 1|2|4|6|9|16`, `--grayscale`, `--no-markup`, `--hidden-text`,
//! `--landscape`/`--portrait`, `--untagged`, `--no-bookmarks`.

use std::path::Path;

use kiln_io_pdf::write::{
    PrintOrientation, WritePrint, WriteSettings, print_document, print_layout,
};
use kiln_io_pdf::{PageParity, PageSelection, Paper, PdfOptions, Scaling, export};

use crate::CliError;

/// Usage text of the PDF options.
pub const USAGE: &str = "kiln-cli convert <in.docx> <out.pdf> [--pages 1-3,5] [--odd|--even] [--copies N] [--fit letter|a4|legal|a3|a5] [--scale F] [--per-sheet N] [--grayscale] [--no-markup] [--hidden-text] [--landscape|--portrait] [--untagged] [--no-bookmarks]";

fn value<'a>(it: &mut impl Iterator<Item = &'a String>, flag: &str) -> Result<&'a str, CliError> {
    it.next()
        .map(String::as_str)
        .ok_or_else(|| CliError::usage(format!("{flag} needs a value"), USAGE))
}

fn number<T: std::str::FromStr>(v: &str, flag: &str) -> Result<T, CliError> {
    v.parse()
        .map_err(|_| CliError::usage(format!("{flag}: \"{v}\" is not a number"), USAGE))
}

/// Parses the options after the input and output paths.
fn options(args: &[String]) -> Result<(PdfOptions, WriteSettings), CliError> {
    let mut o = PdfOptions::default();
    let mut w = WriteSettings {
        markup: true,
        ..WriteSettings::default()
    };
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        match flag.as_str() {
            "--pages" => {
                o.pages = PageSelection::parse(value(&mut it, flag)?)
                    .map_err(|e| CliError::usage(e.to_string(), USAGE))?;
            }
            "--odd" => o.parity = PageParity::Odd,
            "--even" => o.parity = PageParity::Even,
            "--copies" => o.copies = number(value(&mut it, flag)?, flag)?,
            "--scale" => o.scale = number(value(&mut it, flag)?, flag)?,
            "--per-sheet" => o.per_sheet = number(value(&mut it, flag)?, flag)?,
            "--fit" => {
                let v = value(&mut it, flag)?;
                let paper = match v.to_ascii_lowercase().as_str() {
                    "letter" => Paper::LETTER,
                    "legal" => Paper::LEGAL,
                    "a3" => Paper::A3,
                    "a4" => Paper::A4,
                    "a5" => Paper::A5,
                    _ => return Err(CliError::usage(format!("--fit: unknown paper {v}"), USAGE)),
                };
                o.scaling = Scaling::FitTo(paper);
            }
            "--grayscale" => o.grayscale = true,
            "--untagged" => o.tagged = false,
            "--no-bookmarks" => o.bookmarks = false,
            "--no-markup" => w.markup = false,
            "--hidden-text" => w.hidden_text = true,
            "--landscape" => w.orientation = Some(PrintOrientation::Landscape),
            "--portrait" => w.orientation = Some(PrintOrientation::Portrait),
            other => return Err(CliError::usage(format!("unknown option {other}"), USAGE)),
        }
    }
    o.validate()
        .map_err(|e| CliError::usage(e.to_string(), USAGE))?;
    Ok((o, w))
}

/// Converts a Word document to PDF.
///
/// # Errors
///
/// Bad options, unreadable input, an empty page selection, unwritable output.
pub fn run(input: &Path, output: &Path, rest: &[String]) -> Result<String, CliError> {
    let (options, settings) = options(rest)?;
    let bytes =
        std::fs::read(input).map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let started = std::time::Instant::now();
    let read = kiln_io_docx::read_docx(&bytes)
        .map_err(|e| CliError::Failed(format!("{}: {e:?}", input.display())))?;
    let doc = print_document(&read.document, &settings);
    let name = input.file_name().map(|n| n.to_string_lossy().into_owned());
    let layout = print_layout(
        &doc,
        kiln_doc_layout::default_fonts(),
        settings.markup,
        name,
    );
    let laid_out = started.elapsed();
    let title = input.file_stem().map(|n| n.to_string_lossy().into_owned());
    let pdf = export(&WritePrint::new(&doc, &layout, title), &options)
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let elapsed = started.elapsed();
    std::fs::write(output, &pdf)
        .map_err(|e| CliError::Failed(format!("{}: {e}", output.display())))?;
    Ok(format!(
        "{} -> {} ({} pages, {} -> {} bytes, {:.1} ms: layout {:.1} ms, PDF {:.1} ms)",
        input.display(),
        output.display(),
        layout.pages().len(),
        bytes.len(),
        pdf.len(),
        elapsed.as_secs_f64() * 1000.0,
        laid_out.as_secs_f64() * 1000.0,
        (elapsed - laid_out).as_secs_f64() * 1000.0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_parse_and_refuse_nonsense() {
        let args: Vec<String> = [
            "--pages",
            "1-2",
            "--grayscale",
            "--per-sheet",
            "4",
            "--fit",
            "a4",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        let (o, w) = options(&args).unwrap();
        assert!(o.grayscale && o.per_sheet == 4 && w.markup);
        assert_eq!(o.scaling, Scaling::FitTo(Paper::A4));
        for bad in [
            &["--copies"][..],
            &["--per-sheet", "3"],
            &["--fit", "x"],
            &["--what"],
        ] {
            let args: Vec<String> = bad.iter().map(|s| (*s).to_owned()).collect();
            assert!(options(&args).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn converts_a_fixture_to_pdf() {
        let input =
            kiln_testing::fixtures::fixtures_dir().join("docx/features/headers-footers.docx");
        let out = std::env::temp_dir().join(format!("kiln-convert-{}.pdf", std::process::id()));
        let text = run(&input, &out, &["--pages".into(), "2-3".into()]).unwrap();
        let pdf = std::fs::read(&out).unwrap();
        let _ = std::fs::remove_file(&out);
        assert!(pdf.starts_with(b"%PDF-"), "{text}");
        assert!(text.contains("5 pages"), "{text}");
    }
}
