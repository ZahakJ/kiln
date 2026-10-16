// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli convert` for workbooks (WP-70): `.xlsx`, `.xlsm`, `.xltx`, `.xltm`, `.csv`, `.tsv`
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
            let written = write_xlsx(
                &wb,
                source.as_ref(),
                &WriteOptions {
                    kind: Some(kind),
                    now: Some(crate::convert::now_w3cdtf()),
                    author: None,
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
        assert_eq!(v[0]["Result"], serde_json::json!(210.0));
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
