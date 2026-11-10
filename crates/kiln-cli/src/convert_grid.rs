// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert` for workbooks: `.xlsx`, `.xlsm`, `.xltx`, `.xltm`, `.csv`, `.tsv`
//! and `.json` in and out. See [`crate::convert`] for the command.

use std::fmt::Write as _;
use std::path::Path;

use kiln_io_csv::{
    Encoding, ExportOptions, ImportOptions, read_csv, read_json, write_delimited, write_json,
};
use kiln_io_xlsx::{WriteOptions, XlsxKind, XlsxSource, read_xlsx, write_xlsx};
use kiln_sheet::Workbook;

use crate::CliError;

fn failed(p: &Path, e: impl std::fmt::Display) -> CliError {
    CliError::Failed(format!("{}: {e}", p.display()))
}

/// Reads a workbook file (with the source of an Office file for a faithful save).
fn read(
    input: &Path,
    ext: &str,
    notes: &mut Vec<String>,
) -> Result<(Workbook, Option<XlsxSource>), CliError> {
    let bytes = std::fs::read(input).map_err(|e| failed(input, e))?;
    match ext {
        "csv" | "tsv" => {
            let imp = read_csv(
                &bytes,
                &ImportOptions {
                    delimiter: (ext == "tsv").then_some('\t'),
                    sheet_name: input
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.chars().take(31).collect()),
                    ..ImportOptions::default()
                },
            )
            .map_err(|e| failed(input, e))?;
            notes.push(format!(
                "read {} rows, {} columns ({}, delimiter {:?})",
                imp.report.rows,
                imp.report.cols,
                imp.encoding.label(),
                imp.dialect.delimiter
            ));
            notes.extend(imp.report.warnings);
            Ok((imp.workbook, None))
        }
        "json" => {
            let (wb, report) =
                read_json(&bytes, &ImportOptions::default()).map_err(|e| failed(input, e))?;
            notes.push(format!(
                "read {} rows, {} columns",
                report.rows, report.cols
            ));
            Ok((wb, None))
        }
        _ => {
            let r = read_xlsx(&bytes).map_err(|e| failed(input, e))?;
            notes.extend(r.warnings.iter().map(|w| format!("warning: {w}")));
            Ok((r.workbook, Some(r.source)))
        }
    }
}

/// `kiln-cli convert <book.xlsx> <out.pdf> [--all-sheets] [--grayscale]`: the active
/// sheet (or every visible sheet) printed with its page setup — print area, print titles,
/// scaling, breaks, headers and footers — as File ▸ Print does.
///
/// # Errors
///
/// Bad options, unreadable input, unwritable output.
pub fn run_pdf(input: &Path, output: &Path, rest: &[String]) -> Result<String, CliError> {
    const USAGE: &str = "kiln-cli convert <book.xlsx> <out.pdf> [--all-sheets] [--grayscale]";
    let mut all = false;
    let mut options = kiln_io_pdf::PdfOptions::default();
    for a in rest {
        match a.as_str() {
            "--all-sheets" => all = true,
            "--grayscale" => options.grayscale = true,
            other => return Err(CliError::usage(format!("unknown option {other}"), USAGE)),
        }
    }
    let ext = input
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let mut notes = Vec::new();
    let started = std::time::Instant::now();
    let (mut wb, source) = read(input, &ext, &mut notes)?;
    // The charts and pictures of the file print over the cells.
    let mut charts: std::collections::HashMap<String, kiln_charts::model::ChartSpace> = source
        .as_ref()
        .map(|s| {
            s.charts()
                .into_iter()
                .filter_map(|(n, c)| Some((n, c.ok()?)))
                .collect()
        })
        .unwrap_or_default();
    // Files written without chart caches (openpyxl) print what the cells say, as on screen.
    for c in charts.values_mut() {
        kiln_charts::data::refresh(c, &wb);
    }
    let theme = source.as_ref().and_then(|s| {
        let pkg = s.package();
        pkg.part_names()
            .find(|n| {
                pkg.content_type(n)
                    == Some("application/vnd.openxmlformats-officedocument.theme+xml")
            })
            .and_then(|n| pkg.part(n).ok().flatten())
            .and_then(|b| kiln_ooxml::drawingml::theme::parse_theme_bytes(&b).ok())
    });
    let mut images = std::collections::HashMap::new();
    if let Some(src) = &source {
        let mut next = 0u32;
        for d in wb.sheets().iter().flat_map(|s| s.meta().drawings.iter()) {
            if d.kind != kiln_sheet::meta::DrawingKind::Picture || images.contains_key(&d.reference)
            {
                continue;
            }
            let Some(bytes) = src.package().part(&d.reference).ok().flatten() else {
                continue;
            };
            next += 1;
            if let Some(img) = kiln_deck_layout::picture::decode(&bytes, kiln_scene::ImageId(next))
            {
                images.insert(
                    d.reference.clone(),
                    std::sync::Arc::new(kiln_io_pdf::printable::PrintImage {
                        id: img.id,
                        width: img.width,
                        height: img.height,
                        rgba: std::sync::Arc::new(img.rgba),
                        source: None,
                    }),
                );
            }
        }
    }
    let sheets: Vec<usize> = if all {
        wb.sheets()
            .iter()
            .enumerate()
            .filter(|(_, s)| s.meta().visibility == kiln_sheet::meta::SheetVisibility::Visible)
            .map(|(i, _)| i)
            .collect()
    } else {
        Vec::new()
    };
    let now = crate::convert::now_w3cdtf();
    let date = format!(
        "{}/{}/{}",
        now[5..7].trim_start_matches('0'),
        now[8..10].trim_start_matches('0'),
        &now[0..4]
    );
    let file = input
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let path = input
        .parent()
        .map(|p| p.display().to_string())
        .unwrap_or_default();
    options.title = input.file_stem().map(|n| n.to_string_lossy().into_owned());
    let print = kiln_sheet_layout::print::pages::WorkbookPrint::new(
        &mut wb,
        kiln_text::FontStore::bundled(),
        kiln_sheet_layout::print::pages::PrintOptions {
            sheets,
            selection: None,
            title: options.title.clone().unwrap_or_default(),
            file,
            path,
            date,
            time: now[11..16].to_owned(),
            landscape: None,
            margins: None,
        },
    );
    let chart = |p: &str| charts.get(p);
    let print = print.with_drawings(kiln_sheet_layout::print::pages::PrintDrawings {
        chart: Some(&chart),
        images: images.into_iter().collect(),
        theme: theme.as_ref(),
    });
    let counts = print.sheet_pages();
    let pages: usize = counts.iter().map(|(_, n)| n).sum();
    let pdf = kiln_io_pdf::export(&print, &options).map_err(|e| failed(input, e))?;
    std::fs::write(output, &pdf).map_err(|e| failed(output, e))?;
    Ok(format!(
        "{} -> {} ({pages} pages, {} bytes, {:.1} ms)",
        input.display(),
        output.display(),
        pdf.len(),
        started.elapsed().as_secs_f64() * 1000.0,
    ))
}

/// Runs the conversion.
///
/// # Errors
///
/// Unreadable input, unwritable output.
pub fn run(input: &Path, output: &Path, in_ext: &str, out_ext: &str) -> Result<String, CliError> {
    let started = std::time::Instant::now();
    let mut notes = Vec::new();
    let (wb, source) = read(input, in_ext, &mut notes)?;
    let active = wb
        .sheets()
        .get(wb.props().active_sheet)
        .or_else(|| wb.sheets().first())
        .map(|s| (s.id(), s.name().to_owned()))
        .ok_or_else(|| failed(input, "the workbook has no sheet"))?;
    let bytes = match out_ext {
        "csv" | "tsv" => {
            if wb.sheets().len() > 1 {
                notes.push(format!(
                    "only the active sheet \"{}\" is saved (of {} sheets), as Excel does",
                    active.1,
                    wb.sheets().len()
                ));
            }
            let opts = if out_ext == "tsv" {
                ExportOptions::tsv()
            } else {
                ExportOptions::default()
            };
            write_delimited(
                &wb,
                active.0,
                &ExportOptions {
                    encoding: Encoding::Utf8,
                    ..opts
                },
            )
        }
        "json" => write_json(&wb, active.0),
        ext => {
            let kind = XlsxKind::from_extension(ext).unwrap_or_default();
            // Saved under the extension it came with, a file keeps its own kind: a macro-enabled
            // workbook named .xlsx keeps its macros.
            let kind = (in_ext != out_ext).then_some(kind);
            let written = write_xlsx(
                &wb,
                source.as_ref(),
                &WriteOptions {
                    kind,
                    now: Some(crate::convert::now_w3cdtf()),
                    ..WriteOptions::default()
                },
            )
            .map_err(|e| failed(output, e))?;
            notes.extend(written.issues.iter().map(|i| format!("warning: {i}")));
            notes.push(format!(
                "parts written from the model: {}, carried over unchanged: {}",
                written.regenerated.len(),
                kiln_ooxml::opc::OpcPackage::open(&written.bytes)
                    .map_or(0, |p| p.all_part_names().count() + 1)
                    .saturating_sub(written.regenerated.len())
            ));
            written.bytes
        }
    };
    std::fs::write(output, &bytes).map_err(|e| failed(output, e))?;
    let mut out = format!(
        "{} -> {} ({} bytes, {:.1} ms)",
        input.display(),
        output.display(),
        bytes.len(),
        started.elapsed().as_secs_f64() * 1000.0
    );
    for n in notes {
        let _ = write!(out, "\n{n}");
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_macro_workbook_named_xlsx_keeps_its_macros() {
        let root = kiln_testing::repo_root().join("fixtures/external/xlsx/poi-stress002.xlsx");
        if !root.exists() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("kiln-macro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("out.xlsx");
        run(&root, &out, "xlsx", "xlsx").unwrap();
        let pkg = kiln_ooxml::opc::OpcPackage::open(&std::fs::read(&out).unwrap()).unwrap();
        assert!(pkg.has("/xl/vbaProject.bin"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tmp(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kiln-convert-grid-{}-{name}", std::process::id()))
    }

    #[test]
    fn workbooks_convert_to_every_format_and_back() {
        let input = kiln_testing::fixtures::fixtures_dir().join("xlsx/features/formulas.xlsx");
        let csv = tmp("f.csv");
        let text = crate::convert::run(&[
            input.to_string_lossy().into_owned(),
            csv.to_string_lossy().into_owned(),
        ])
        .expect("csv");
        assert!(
            text.contains("only the active sheet \"Formulas\""),
            "{text}"
        );
        let data = std::fs::read_to_string(&csv).expect("written");
        assert!(
            data.starts_with("Family,Formula,Result\r\nMath,=SUM(Data!A1:A20),210\r\n"),
            "{data}"
        );
        // CSV back to a workbook, and to JSON and xlsx.
        let xlsx = tmp("f.xlsx");
        crate::convert::run(&[
            csv.to_string_lossy().into_owned(),
            xlsx.to_string_lossy().into_owned(),
        ])
        .expect("xlsx");
        let back = read_xlsx(&std::fs::read(&xlsx).expect("xlsx")).expect("read");
        let s = back.workbook.first_sheet().expect("sheet");
        assert_eq!(
            back.workbook.value(s, 1, 2),
            kiln_sheet::CellValue::Number(210.0)
        );
        let json = tmp("f.json");
        crate::convert::run(&[
            xlsx.to_string_lossy().into_owned(),
            json.to_string_lossy().into_owned(),
        ])
        .expect("json");
        let v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&json).expect("json")).expect("valid");
        assert_eq!(v[0]["Result"], serde_json::json!(210));
        let xlsm = tmp("f.xlsm");
        crate::convert::run(&[
            input.to_string_lossy().into_owned(),
            xlsm.to_string_lossy().into_owned(),
        ])
        .expect("xlsm");
        let pkg =
            kiln_ooxml::opc::OpcPackage::open(&std::fs::read(&xlsm).expect("xlsm")).expect("pkg");
        assert_eq!(
            pkg.content_type("/xl/workbook.xml"),
            Some(XlsxKind::MacroEnabled.content_type())
        );
        for p in [csv, xlsx, json, xlsm] {
            let _ = std::fs::remove_file(p);
        }
        assert!(
            crate::convert::run(&[input.to_string_lossy().into_owned(), "x.docx".into()]).is_err()
        );
    }
}
