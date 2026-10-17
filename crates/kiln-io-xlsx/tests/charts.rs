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

fn chart_anchor(id: &str, part: &str) -> kiln_sheet::meta::DrawingAnchor {
    use kiln_sheet::meta::{Anchor, AnchorPoint, DrawingAnchor, DrawingKind, EditAs};
    DrawingAnchor {
        id: id.into(),
        kind: DrawingKind::Chart,
        reference: part.into(),
        anchor: Anchor::TwoCell {
            from: AnchorPoint {
                row: 1,
                col: 4,
                row_off: 0,
                col_off: 0,
            },
            to: AnchorPoint {
                row: 16,
                col: 11,
                row_off: 0,
                col_off: 0,
            },
            edit_as: EditAs::TwoCell,
        },
        name: Some("Spend".into()),
        alt_text: Some("Spend by quarter".into()),
        hidden: false,
        locked: true,
    }
}

fn add_drawing(wb: &mut kiln_sheet::Workbook, sheet: usize, a: kiln_sheet::meta::DrawingAnchor) {
    let sid = wb.sheets()[sheet].id();
    let mut list = wb.sheets()[sheet].meta().drawings.clone();
    list.push(a);
    wb.apply(&kiln_sheet::Command::SetSheetMeta {
        sheet: sid,
        field: Box::new(kiln_sheet::commands::MetaField::Drawings(list)),
    })
    .unwrap();
}

#[test]
fn charts_added_in_kiln_are_written_with_their_drawing() {
    use kiln_protocol::commands::ChartKind;
    // A sheet without a drawing part (styles.xlsx), and a new workbook without a source.
    let bytes = std::fs::read(fixtures_dir().join("xlsx/features/styles.xlsx")).unwrap();
    let mut read = read_xlsx(&bytes).unwrap();
    let column = kiln_charts::edit::sample_chart(Some(ChartKind::ColumnClustered));
    let tree = kiln_charts::edit::sample_chart(Some(ChartKind::Treemap));
    add_drawing(
        &mut read.workbook,
        0,
        chart_anchor("k1", "/xl/charts/chart91.xml"),
    );
    add_drawing(
        &mut read.workbook,
        0,
        chart_anchor("k2", "/xl/charts/chartEx91.xml"),
    );
    let options = WriteOptions {
        charts: vec![
            ("/xl/charts/chart91.xml".into(), column.clone()),
            ("/xl/charts/chartEx91.xml".into(), tree),
        ],
        ..WriteOptions::default()
    };
    let out = write_xlsx(&read.workbook, Some(&read.source), &options).unwrap();
    assert!(
        !out.issues.iter().any(|i| i.contains("drawing")),
        "{:?}",
        out.issues
    );
    let again = read_xlsx(&out.bytes).unwrap();
    let drawings = &again.workbook.sheets()[0].meta().drawings;
    assert_eq!(drawings.len(), 2, "{drawings:?}");
    assert!(
        drawings
            .iter()
            .all(|d| d.kind == kiln_sheet::meta::DrawingKind::Chart)
    );
    let refs: Vec<&str> = drawings.iter().map(|d| d.reference.as_str()).collect();
    assert!(refs.contains(&"/xl/charts/chart91.xml"), "{refs:?}");
    assert!(refs.contains(&"/xl/charts/chartEx91.xml"), "{refs:?}");
    assert_eq!(drawings[0].alt_text.as_deref(), Some("Spend by quarter"));
    for (name, chart) in again.source.charts() {
        assert!(chart.is_ok(), "{name}: {:?}", chart.err());
    }
    // LibreOffice-free sanity: the package keeps its content types and relationships valid.
    let pkg = OpcPackage::open(&out.bytes).unwrap();
    assert!(pkg.has("/xl/drawings/drawing1.xml"));

    // A new workbook: the drawing and chart parts are created.
    let mut wb = kiln_sheet::Workbook::new();
    add_drawing(&mut wb, 0, chart_anchor("n1", "/xl/charts/chart1.xml"));
    let out = write_xlsx(
        &wb,
        None,
        &WriteOptions {
            charts: vec![("/xl/charts/chart1.xml".into(), column)],
            ..WriteOptions::default()
        },
    )
    .unwrap();
    let again = read_xlsx(&out.bytes).unwrap();
    assert_eq!(again.workbook.sheets()[0].meta().drawings.len(), 1);
    assert_eq!(again.source.charts().len(), 1);
}

#[test]
fn a_chart_that_becomes_a_chartex_chart_gets_a_new_frame() {
    use kiln_charts::model::PartFormat;
    use kiln_protocol::commands::ChartKind;
    let bytes = std::fs::read(fixtures_dir().join("xlsx/charts/column.xlsx")).unwrap();
    let read = read_xlsx(&bytes).unwrap();
    let (name, chart) = read.source.charts().into_iter().next().unwrap();
    let mut chart = chart.unwrap();
    kiln_charts::edit::change_type(&mut chart, ChartKind::Treemap);
    assert_eq!(chart.format, PartFormat::ChartEx);
    let out = write_xlsx(
        &read.workbook,
        Some(&read.source),
        &WriteOptions {
            charts: vec![(name.clone(), chart)],
            ..WriteOptions::default()
        },
    )
    .unwrap();
    let again = read_xlsx(&out.bytes).unwrap();
    let pkg = again.source.package();
    assert_eq!(
        pkg.content_type(&name),
        Some(PartFormat::ChartEx.content_type())
    );
    let (_, c) = again.source.charts().into_iter().next().unwrap();
    assert_eq!(
        kiln_charts::edit::kind_of(&c.unwrap()),
        Some(ChartKind::Treemap)
    );
    let count = |wb: &kiln_sheet::Workbook| {
        wb.sheets()
            .iter()
            .map(|s| s.meta().drawings.len())
            .sum::<usize>()
    };
    assert_eq!(count(&again.workbook), count(&read.workbook));
}
