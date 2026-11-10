// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln's ODF output read back by LibreOffice (`tools/oracle-soffice.sh`). Skipped, with a
//! message, where LibreOffice is not installed.

use std::path::{Path, PathBuf};
use std::process::Command;

use kiln_sheet::{CellValue, Workbook};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A scratch directory for one test.
fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("kiln-odf-oracle-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("scratch");
    d
}

/// Converts `input` with LibreOffice; `None` when LibreOffice is not installed.
fn convert(input: &Path, format: &str, outdir: &Path) -> Option<PathBuf> {
    if !Path::new("/usr/bin/soffice").exists() && std::env::var_os("SOFFICE").is_none() {
        eprintln!("skipped: LibreOffice is not installed");
        return None;
    }
    let status = Command::new(repo().join("tools/oracle-soffice.sh"))
        .arg(input)
        .arg(format)
        .arg(outdir)
        .status()
        .expect("oracle-soffice.sh");
    assert!(
        status.success(),
        "LibreOffice could not convert {}",
        input.display()
    );
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let ext = format.split(':').next().unwrap_or(format);
    Some(outdir.join(format!("{stem}.{ext}")))
}

fn text(s: &str) -> CellValue {
    CellValue::Text(s.into())
}

/// Identical neighbouring formula cells and rows stay separate cells in LibreOffice (a repeated
/// formula cell would be read once and the cells after it would move left or up).
#[test]
fn repeated_formula_cells_keep_their_places() {
    let mut wb = Workbook::new();
    let s = wb.first_sheet().expect("sheet");
    wb.load_formula(s, 0, 0, "$E$1*2", CellValue::Number(10.0), false)
        .unwrap();
    wb.load_formula(s, 0, 1, "$E$1*2", CellValue::Number(10.0), false)
        .unwrap();
    wb.load_value(s, 0, 2, text("x")).unwrap();
    wb.load_value(s, 0, 4, CellValue::Number(5.0)).unwrap();
    wb.load_formula(s, 2, 0, "TODAY()>0", CellValue::Bool(true), false)
        .unwrap();
    wb.load_formula(s, 3, 0, "TODAY()>0", CellValue::Bool(true), false)
        .unwrap();
    wb.load_value(s, 4, 0, text("end")).unwrap();
    let bytes = kiln_io_odf::write_ods(&wb, &kiln_io_odf::WriteOptions::default()).expect("write");
    let dir = scratch("repeat");
    let file = dir.join("repeat.ods");
    std::fs::write(&file, &bytes).unwrap();
    let Some(csv) = convert(&file, "csv", &dir.join("out")) else {
        return;
    };
    let got = std::fs::read_to_string(csv).expect("csv");
    let lines: Vec<&str> = got.lines().collect();
    assert_eq!(
        lines.first().copied(),
        Some("10,10,x,,5"),
        "LibreOffice read:\n{got}"
    );
    assert_eq!(
        lines.get(2).copied(),
        Some("TRUE,,,,"),
        "LibreOffice read:\n{got}"
    );
    assert_eq!(
        lines.get(3).copied(),
        Some("TRUE,,,,"),
        "LibreOffice read:\n{got}"
    );
    assert_eq!(
        lines.get(4).copied(),
        Some("end,,,,"),
        "LibreOffice read:\n{got}"
    );
}

fn fixture(rel: &str) -> Vec<u8> {
    std::fs::read(repo().join("fixtures").join(rel)).expect("fixture")
}

/// The chart types and series names of chart facts (what any reader must agree on).
fn chart_kinds(facts: Vec<String>) -> Vec<String> {
    let mut v: Vec<String> = facts
        .into_iter()
        .filter(|f| {
            f.contains("/group")
                && (f.contains("=bar")
                    || f.contains("=line")
                    || f.contains("=area")
                    || f.contains("=pie")
                    || f.contains("=doughnut")
                    || f.contains("=scatter")
                    || f.contains("=bubble")
                    || f.contains("=radar")
                    || f.ends_with("/name=")
                    || f.contains("/name="))
        })
        .map(|f| f.find("/group").map_or(f.clone(), |i| f[i..].to_owned()))
        .collect();
    v.sort();
    v
}

/// Charts Kiln writes into `.ods` open in LibreOffice as the same charts: LibreOffice's `.xlsx`
/// export of the file has the chart types and series of the source.
#[test]
fn ods_charts_open_in_libreoffice() {
    let read = kiln_io_xlsx::read_xlsx(&fixture("xlsx/features/charts.xlsx")).expect("xlsx");
    let mut charts: Vec<(String, kiln_charts::model::ChartSpace)> = read
        .source
        .charts()
        .into_iter()
        .filter_map(|(n, c)| c.ok().map(|c| (n, c)))
        .collect();
    for (_, c) in &mut charts {
        kiln_charts::data::refresh(c, &read.workbook);
    }
    let expected: Vec<String> = chart_kinds(
        charts
            .iter()
            .flat_map(|(n, c)| kiln_io_odf::chart::facts(n, c))
            .collect(),
    );
    let bytes = kiln_io_odf::write_ods(
        &read.workbook,
        &kiln_io_odf::WriteOptions {
            charts,
            ..kiln_io_odf::WriteOptions::default()
        },
    )
    .expect("write");
    let dir = scratch("ods-charts");
    let file = dir.join("charts.ods");
    std::fs::write(&file, &bytes).unwrap();
    let Some(xlsx) = convert(&file, "xlsx", &dir.join("out")) else {
        return;
    };
    let back = kiln_io_xlsx::read_xlsx(&std::fs::read(xlsx).unwrap()).expect("LibreOffice's xlsx");
    let got: Vec<String> = chart_kinds(
        back.source
            .charts()
            .into_iter()
            .filter_map(|(n, c)| c.ok().map(|c| kiln_io_odf::chart::facts(&n, &c)))
            .flatten()
            .collect(),
    );
    assert_eq!(got, expected);
}

/// Charts Kiln writes into `.odp` open in LibreOffice as the same charts (its `.pptx` export).
#[test]
fn odp_charts_open_in_libreoffice() {
    let read = kiln_io_pptx::read_pptx(&fixture("pptx/features/charts.pptx")).expect("pptx");
    let facts = |p: &kiln_deck::Presentation| -> Vec<String> {
        chart_kinds(
            kiln_io_odf::deck_facts(p)
                .into_iter()
                .filter(|f| f.contains("/chart/"))
                .collect(),
        )
    };
    let expected = facts(&read.presentation);
    assert!(!expected.is_empty());
    let bytes = kiln_io_odf::write_odp(&read.presentation, &kiln_io_odf::WriteOptions::default())
        .expect("write");
    let dir = scratch("odp-charts");
    let file = dir.join("charts.odp");
    std::fs::write(&file, &bytes).unwrap();
    let Some(pptx) = convert(&file, "pptx", &dir.join("out")) else {
        return;
    };
    let back = kiln_io_pptx::read_pptx(&std::fs::read(pptx).unwrap()).expect("LibreOffice's pptx");
    assert_eq!(facts(&back.presentation), expected);
}

fn odt_chart_facts(doc: &kiln_doc::Document) -> Vec<String> {
    let mut out = Vec::new();
    for (_, p) in doc.all_paragraphs() {
        for o in p.content.objects() {
            if let kiln_doc::InlineObject::Drawing(d) = o
                && let kiln_doc::model::DrawingKind::Chart { part } = &d.kind
            {
                let cs = doc
                    .media
                    .get(part)
                    .and_then(|m| kiln_io_odf::chart::from_drawingml(&m.data))
                    .expect("chart part");
                out.extend(kiln_io_odf::chart::facts("chart", &cs));
            }
        }
    }
    out
}

/// A chart in a text document reads as a chart, writes back as a chart object and opens in
/// LibreOffice as a chart (its `.docx` export has the chart part).
#[test]
fn odt_charts_round_trip_and_open_in_libreoffice() {
    let read = kiln_io_odf::read_odt(&fixture("odf/manual/chart.odt")).expect("odt");
    let facts = odt_chart_facts(&read.document);
    assert!(
        facts.iter().any(|f| f == "chart/group0=bar:Column:None"),
        "{facts:?}"
    );
    assert!(
        facts
            .iter()
            .any(|f| f == "chart/group0/series0/values=19.2,21.4,16.7,25.1"),
        "{facts:?}"
    );
    let bytes = kiln_io_odf::write_odt(&read.document, &kiln_io_odf::WriteOptions::default())
        .expect("write");
    let again = kiln_io_odf::read_odt(&bytes).expect("written odt");
    assert_eq!(odt_chart_facts(&again.document), facts);
    let dir = scratch("odt-chart");
    let file = dir.join("chart.odt");
    std::fs::write(&file, &bytes).unwrap();
    let Some(docx) = convert(&file, "docx", &dir.join("out")) else {
        return;
    };
    let back = kiln_io_docx::read_docx(&std::fs::read(docx).unwrap()).expect("LibreOffice's docx");
    assert_eq!(back.charts.len(), 1, "LibreOffice kept no chart");
}
