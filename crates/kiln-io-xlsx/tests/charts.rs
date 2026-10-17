// SPDX-License-Identifier: AGPL-3.0-or-later
//! The chart part handler: every workbook of `fixtures/xlsx/charts/` reads its charts, an edited
//! chart is written back on save, and unedited charts stay byte for byte.

use kiln_io_xlsx::{WriteOptions, read_xlsx, write_xlsx};
use kiln_ooxml::opc::OpcPackage;
use kiln_testing::fixtures::{FixtureFormat, FixtureKind, fixtures_dir, list_in};

#[test]
fn every_chart_fixture_reads_its_charts() {
    let files = list_in(&fixtures_dir(), FixtureFormat::Xlsx, FixtureKind::Charts);
    assert!(files.len() >= 10, "only {} chart workbooks", files.len());
    for f in files {
        let read = read_xlsx(&f.read().unwrap()).unwrap();
        let charts = read.source.charts();
        assert!(!charts.is_empty(), "{}: no chart parts", f.rel);
        for (name, chart) in charts {
            assert!(chart.is_ok(), "{} {name}: {:?}", f.rel, chart.err());
        }
    }
}

#[test]
fn an_edited_chart_is_saved_and_the_others_stay() {
    let bytes = std::fs::read(fixtures_dir().join("xlsx/charts/column.xlsx")).unwrap();
    let mut read = read_xlsx(&bytes).unwrap();
    let (name, chart) = read.source.charts().into_iter().next().unwrap();
    let mut chart = chart.unwrap();
    kiln_charts::edit::set_title(&mut chart, "Spend by quarter");
    read.source.set_chart(&name, &chart).unwrap();
    let out = write_xlsx(&read.workbook, Some(&read.source), &WriteOptions::default()).unwrap();
    let again = read_xlsx(&out.bytes).unwrap();
    let (name2, chart2) = again.source.charts().into_iter().next().unwrap();
    assert_eq!(name2, name);
    let facts = kiln_charts::package::package_chart_facts(again.source.package());
    assert!(
        facts.iter().any(|f| f.contains("Spend by quarter")),
        "{facts:?}"
    );
    assert!(chart2.is_ok());
    // Without an edit, the chart part is copied byte for byte.
    let plain = read_xlsx(&bytes).unwrap();
    let out = write_xlsx(
        &plain.workbook,
        Some(&plain.source),
        &WriteOptions::default(),
    )
    .unwrap();
    let a = OpcPackage::open(&bytes).unwrap();
    let b = OpcPackage::open(&out.bytes).unwrap();
    assert_eq!(a.part(&name).unwrap(), b.part(&name).unwrap());
    // A part that is not a chart is refused.
    assert!(read.source.set_chart("/xl/workbook.xml", &chart).is_err());
}
