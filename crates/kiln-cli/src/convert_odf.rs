// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert` to and from OpenDocument: `.odt`/`.ott` with the Word formats,
//! `.ods`/`.ots` with the workbook formats (`.xlsx` family, `.csv`, `.tsv`, `.json`), `.odp`/
//! `.otp` with the PowerPoint formats. The file is read into Kiln's model and written in the
//! output's format; what the target cannot hold is listed as warnings. See [`crate::convert`].

use std::fmt::Write as _;
use std::path::Path;

use kiln_io_odf::{OdfKind, WriteOptions as OdfOptions};

use crate::CliError;

fn failed(p: &Path, e: impl std::fmt::Display) -> CliError {
    CliError::Failed(format!("{}: {e}", p.display()))
}

/// The document family of an extension: text, workbook or presentation.
fn family(ext: &str) -> Option<OdfKind> {
    match ext {
        "odt" | "ott" | "docx" | "docm" | "dotx" | "dotm" => Some(OdfKind::Text),
        "ods" | "ots" | "xlsx" | "xlsm" | "xltx" | "xltm" | "csv" | "tsv" | "json" => {
            Some(OdfKind::Spreadsheet)
        }
        "odp" | "otp" | "pptx" | "pptm" | "potx" | "potm" | "ppsx" | "ppsm" => {
            Some(OdfKind::Presentation)
        }
        _ => None,
    }
}

fn is_odf(ext: &str) -> bool {
    OdfKind::from_extension(ext).is_some()
}

/// Whether a conversion involves an OpenDocument file (this module handles it).
pub(crate) fn involves_odf(in_ext: &str, out_ext: &str) -> bool {
    is_odf(in_ext) || is_odf(out_ext)
}

/// Runs the conversion.
///
/// # Errors
///
/// Usage errors (formats of different kinds), unreadable input, unwritable output.
pub(crate) fn run(
    input: &Path,
    output: &Path,
    in_ext: &str,
    out_ext: &str,
) -> Result<String, CliError> {
    let (Some(from), Some(to)) = (family(in_ext), family(out_ext)) else {
        return Err(CliError::usage(
            format!("cannot convert .{in_ext} to .{out_ext}"),
            crate::convert::USAGE,
        ));
    };
    if from != to {
        return Err(CliError::usage(
            format!(
                "cannot convert .{in_ext} to .{out_ext}: they hold different kinds of document"
            ),
            crate::convert::USAGE,
        ));
    }
    let bytes = std::fs::read(input).map_err(|e| failed(input, e))?;
    let started = std::time::Instant::now();
    let template = matches!(out_ext, "ott" | "ots" | "otp");
    let mut notes: Vec<String> = Vec::new();
    let out = match from {
        OdfKind::Text => text(&bytes, input, output, in_ext, out_ext, template, &mut notes)?,
        OdfKind::Spreadsheet => {
            sheet(&bytes, input, output, in_ext, out_ext, template, &mut notes)?
        }
        OdfKind::Presentation => {
            deck(&bytes, input, output, in_ext, out_ext, template, &mut notes)?
        }
    };
    std::fs::write(output, &out).map_err(|e| failed(output, e))?;
    let mut text = format!(
        "{} -> {} ({} -> {} bytes, {:.1} ms)",
        input.display(),
        output.display(),
        bytes.len(),
        out.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    for n in notes {
        let _ = write!(text, "\nwarning: {n}");
    }
    Ok(text)
}

fn text(
    bytes: &[u8],
    input: &Path,
    output: &Path,
    in_ext: &str,
    out_ext: &str,
    template: bool,
    notes: &mut Vec<String>,
) -> Result<Vec<u8>, CliError> {
    let doc = if is_odf(in_ext) {
        let r = kiln_io_odf::read_odt(bytes).map_err(|e| failed(input, e))?;
        notes.extend(r.warnings);
        r.document
    } else {
        let r = kiln_io_docx::read_docx(bytes).map_err(|e| failed(input, e))?;
        r.document
    };
    if is_odf(out_ext) {
        notes.extend(kiln_io_odf::odt_issues(&doc));
        return kiln_io_odf::write_odt(
            &doc,
            &OdfOptions {
                template,
                ..OdfOptions::default()
            },
        )
        .map_err(|e| failed(output, e));
    }
    let kind = match out_ext {
        "docm" => kiln_io_docx::DocxKind::MacroEnabledDocument,
        "dotx" => kiln_io_docx::DocxKind::Template,
        "dotm" => kiln_io_docx::DocxKind::MacroEnabledTemplate,
        _ => kiln_io_docx::DocxKind::Document,
    };
    let written = kiln_io_docx::write_docx(
        &doc,
        None,
        &kiln_io_docx::WriteOptions {
            kind: Some(kind),
            now: Some(crate::convert::now_w3cdtf()),
            ..kiln_io_docx::WriteOptions::default()
        },
    )
    .map_err(|e| failed(output, e))?;
    notes.extend(written.issues.iter().map(ToString::to_string));
    Ok(written.bytes)
}

fn sheet(
    bytes: &[u8],
    input: &Path,
    output: &Path,
    in_ext: &str,
    out_ext: &str,
    template: bool,
    notes: &mut Vec<String>,
) -> Result<Vec<u8>, CliError> {
    let mut charts: Vec<(String, kiln_charts::model::ChartSpace)> = Vec::new();
    let mut media: Vec<(String, Vec<u8>)> = Vec::new();
    let (wb, props) = match in_ext {
        "ods" | "ots" => {
            let r = kiln_io_odf::read_ods(bytes).map_err(|e| failed(input, e))?;
            notes.extend(r.warnings);
            charts = r.charts;
            (r.workbook, Some(r.properties))
        }
        "csv" | "tsv" => {
            let imp = kiln_io_csv::read_csv(
                bytes,
                &kiln_io_csv::ImportOptions {
                    delimiter: (in_ext == "tsv").then_some('\t'),
                    ..kiln_io_csv::ImportOptions::default()
                },
            )
            .map_err(|e| failed(input, e))?;
            (imp.workbook, None)
        }
        "json" => {
            let (wb, _) = kiln_io_csv::read_json(bytes, &kiln_io_csv::ImportOptions::default())
                .map_err(|e| failed(input, e))?;
            (wb, None)
        }
        _ => {
            let r = kiln_io_xlsx::read_xlsx(bytes).map_err(|e| failed(input, e))?;
            charts = r
                .source
                .charts()
                .into_iter()
                .filter_map(|(n, c)| c.ok().map(|c| (n, c)))
                .collect();
            // Pictures on sheets: their bytes are in the source package.
            media = r
                .workbook
                .sheets()
                .iter()
                .flat_map(|s| s.meta().drawings.iter())
                .filter(|d| d.kind == kiln_sheet::meta::DrawingKind::Picture)
                .filter_map(|d| {
                    let b = r.source.package().part(&d.reference).ok().flatten()?;
                    Some((d.reference.clone(), b.to_vec()))
                })
                .collect();
            (r.workbook, None)
        }
    };
    match out_ext {
        "ods" | "ots" => {
            let options = OdfOptions {
                template,
                properties: props,
                charts,
                media,
            };
            notes.extend(kiln_io_odf::ods_issues(&wb, &options));
            kiln_io_odf::write_ods(&wb, &options).map_err(|e| failed(output, e))
        }
        "csv" | "tsv" | "json" => {
            let active = wb
                .sheets()
                .get(wb.props().active_sheet)
                .or_else(|| wb.sheets().first())
                .map(kiln_sheet::Sheet::id)
                .ok_or_else(|| failed(input, "the workbook has no sheet"))?;
            if out_ext == "json" {
                return Ok(kiln_io_csv::write_json(&wb, active));
            }
            let opts = if out_ext == "tsv" {
                kiln_io_csv::ExportOptions::tsv()
            } else {
                kiln_io_csv::ExportOptions::default()
            };
            Ok(kiln_io_csv::write_delimited(&wb, active, &opts))
        }
        ext => {
            let kind = kiln_io_xlsx::XlsxKind::from_extension(ext).unwrap_or_default();
            let written = kiln_io_xlsx::write_xlsx(
                &wb,
                None,
                &kiln_io_xlsx::WriteOptions {
                    kind: Some(kind),
                    now: Some(crate::convert::now_w3cdtf()),
                    charts,
                    ..kiln_io_xlsx::WriteOptions::default()
                },
            )
            .map_err(|e| failed(output, e))?;
            notes.extend(written.issues.iter().map(ToString::to_string));
            Ok(written.bytes)
        }
    }
}

fn deck(
    bytes: &[u8],
    input: &Path,
    output: &Path,
    in_ext: &str,
    out_ext: &str,
    template: bool,
    notes: &mut Vec<String>,
) -> Result<Vec<u8>, CliError> {
    let (pres, props) = if is_odf(in_ext) {
        let r = kiln_io_odf::read_odp(bytes).map_err(|e| failed(input, e))?;
        notes.extend(r.warnings);
        (r.presentation, Some(r.properties))
    } else {
        let r = kiln_io_pptx::read_pptx(bytes).map_err(|e| failed(input, e))?;
        (r.presentation, None)
    };
    if is_odf(out_ext) {
        return kiln_io_odf::write_odp(
            &pres,
            &OdfOptions {
                template,
                properties: props,
                ..OdfOptions::default()
            },
        )
        .map_err(|e| failed(output, e));
    }
    let kind =
        crate::convert_deck::deck_kind(out_ext).unwrap_or(kiln_io_pptx::PptxKind::Presentation);
    let written = kiln_io_pptx::write_pptx(
        &pres,
        None,
        &kiln_io_pptx::WriteOptions {
            kind: Some(kind),
            now: Some(crate::convert::now_w3cdtf()),
            ..kiln_io_pptx::WriteOptions::default()
        },
    )
    .map_err(|e| failed(output, e))?;
    notes.extend(written.issues.iter().map(ToString::to_string));
    Ok(written.bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kiln-convert-odf-{}-{name}", std::process::id()))
    }

    #[test]
    fn converts_every_kind_both_ways() {
        let fx = kiln_testing::fixtures::fixtures_dir();
        for (src, mid, back) in [
            ("docx/features/tables.docx", "odt", "docx"),
            ("xlsx/features/formulas.xlsx", "ods", "xlsx"),
            ("pptx/features/notes.pptx", "odp", "pptx"),
        ] {
            let a = tmp(&format!("a.{mid}"));
            let b = tmp(&format!("b.{back}"));
            let in_ext = src.rsplit('.').next().unwrap();
            let t = run(&fx.join(src), &a, in_ext, mid).unwrap();
            assert!(t.contains("->"), "{t}");
            run(&a, &b, mid, back).unwrap();
            assert!(std::fs::metadata(&b).unwrap().len() > 1000);
            let _ = std::fs::remove_file(&a);
            let _ = std::fs::remove_file(&b);
        }
        let e = run(
            &fx.join("docx/features/tables.docx"),
            &tmp("x.ods"),
            "docx",
            "ods",
        )
        .unwrap_err();
        assert!(e.to_string().contains("different kinds"), "{e}");
        assert!(involves_odf("docx", "odt") && !involves_odf("docx", "pdf"));
    }
}
