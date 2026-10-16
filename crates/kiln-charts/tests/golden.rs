// SPDX-License-Identifier: AGPL-3.0-or-later
//! Golden renders of every chart type in the light and dark screen themes, and of the fixture
//! charts in document colours (`fixtures/golden/charts/*.png`).
//!
//! `KILN_UPDATE_GOLDEN=1 cargo nextest run -p kiln-charts --test golden` rewrites them; review the
//! images before committing. `KILN_CHART_PREVIEW=<dir>` also writes 2× renders there.

use std::path::{Path, PathBuf};

use kiln_charts::edit::{insert_from_range, set_data_labels, set_element, set_title};
use kiln_charts::palette::ColorMode;
use kiln_charts::render::scene_delta;
use kiln_charts::text::{TextMeasurer, default_fonts};
use kiln_charts::{ChartSpace, read_chart};
use kiln_protocol::commands::{ChartElementKind, ChartKind, ChartLabelPosition};
use kiln_raster::{View, golden, render_delta};
use kiln_scene::Palette;
use kiln_sheet::{CellValue, Range, Workbook};

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn sheet(rows: &[Vec<CellValue>]) -> Workbook {
    let mut wb = Workbook::new();
    let id = wb.add_sheet(Some("Data"), None, None).unwrap();
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            wb.load_value(id, r as u32, c as u32, v.clone()).unwrap();
        }
    }
    wb
}

fn t(s: &str) -> CellValue {
    CellValue::text(s)
}

fn n(v: f64) -> CellValue {
    CellValue::Number(v)
}

fn budget() -> Workbook {
    let mut rows = vec![vec![t("Quarter"), t("Materials"), t("Firing"), t("Events")]];
    for (q, a, b, c) in [
        ("Q1", 730.0, 1180.0, 90.0),
        ("Q2", 505.0, 640.0, 60.0),
        ("Q3", 745.0, 1240.0, 120.0),
        ("Q4", 650.0, 1320.0, 380.0),
    ] {
        rows.push(vec![t(q), n(a), n(b), n(c)]);
    }
    sheet(&rows)
}

fn shares() -> Workbook {
    let mut rows = vec![vec![t("Item"), t("Spend")]];
    for (k, v) in [
        ("Gas and kiln time", 42.0),
        ("Clay and glaze", 24.0),
        ("Bursaries", 19.0),
        ("Shelves and repairs", 15.0),
    ] {
        rows.push(vec![t(k), n(v)]);
    }
    sheet(&rows)
}

fn firings() -> Workbook {
    let mut rows = vec![vec![t("Temperature"), t("Shrinkage"), t("Pieces")]];
    for i in 0..10 {
        let temp = 1000.0 + 30.0 * f64::from(i);
        rows.push(vec![
            n(temp),
            n(2.0 + (temp - 1000.0) * 0.03 + f64::from(i % 3) * 0.6),
            n(f64::from(8 + (i * 7) % 23)),
        ]);
    }
    sheet(&rows)
}

fn skills() -> Workbook {
    let mut rows = vec![vec![t("Skill"), t("Ana"), t("Ben")]];
    for (k, a, b) in [
        ("Throwing", 9.0, 5.0),
        ("Trimming", 7.0, 6.0),
        ("Glazing", 6.0, 9.0),
        ("Loading", 8.0, 7.0),
        ("Firing", 5.0, 8.0),
        ("Teaching", 7.0, 4.0),
    ] {
        rows.push(vec![t(k), n(a), n(b)]);
    }
    sheet(&rows)
}

fn months() -> Workbook {
    let mut rows = vec![vec![t("Month"), t("Spend"), t("Members")]];
    let m = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let spend = [
        610., 640., 660., 580., 560., 565., 640., 650., 695., 720., 820., 810.,
    ];
    let members = [18., 18., 18., 19., 19., 19., 21., 21., 21., 23., 24., 24.];
    for i in 0..12 {
        rows.push(vec![t(m[i]), n(spend[i]), n(members[i])]);
    }
    sheet(&rows)
}

fn studios() -> Workbook {
    let mut rows = vec![vec![t("Region"), t("Studio"), t("Members")]];
    for (r, s, v) in [
        ("North", "Leeds", 24.0),
        ("North", "York", 13.0),
        ("North", "Hull", 9.0),
        ("South", "Bristol", 31.0),
        ("South", "Exeter", 12.0),
        ("East", "Norwich", 17.0),
        ("East", "Ipswich", 8.0),
        ("East", "Cambridge", 21.0),
        ("West", "Hereford", 6.0),
    ] {
        rows.push(vec![t(r), t(s), n(v)]);
    }
    sheet(&rows)
}

/// One sample chart per type, as a user would make it: insert from a range, then the usual
/// finishing touches.
fn samples() -> Vec<(&'static str, ChartSpace)> {
    let b = budget();
    let budget_range = Range::new(0, 0, 4, 3);
    let mk =
        |wb: &Workbook, r: Range, k: ChartKind| insert_from_range(wb, "Data", r, Some(k)).unwrap();
    let mut out = Vec::new();

    let mut c = mk(&b, budget_range, ChartKind::ColumnClustered);
    set_title(&mut c, "Spend by quarter");
    out.push(("column-clustered", c));
    let mut c = mk(&b, budget_range, ChartKind::ColumnStacked);
    set_title(&mut c, "Spend by quarter, stacked");
    set_data_labels(&mut c, Some(ChartLabelPosition::Center));
    out.push(("column-stacked", c));
    let mut c = mk(&b, budget_range, ChartKind::ColumnPercent);
    set_title(&mut c, "Share of spend");
    out.push(("column-percent", c));
    let mut c = mk(&b, budget_range, ChartKind::BarClustered);
    set_title(&mut c, "Spend by quarter");
    set_element(&mut c, ChartElementKind::PrimaryHorizontalAxisTitle, true);
    out.push(("bar-clustered", c));
    let mut c = mk(&b, budget_range, ChartKind::BarStacked);
    set_title(&mut c, "Stacked bars");
    out.push(("bar-stacked", c));
    let mut c = mk(&months(), Range::new(0, 0, 12, 1), ChartKind::LineMarkers);
    set_title(&mut c, "Monthly spend");
    set_element(&mut c, ChartElementKind::LinearTrendline, true);
    out.push(("line-markers", c));
    let mut c = mk(&b, budget_range, ChartKind::LineSmooth);
    set_title(&mut c, "Smooth lines");
    out.push(("line-smooth", c));
    let mut c = mk(&b, budget_range, ChartKind::AreaStacked);
    set_title(&mut c, "Stacked area");
    out.push(("area-stacked", c));
    let mut c = mk(&b, budget_range, ChartKind::Area);
    set_title(&mut c, "Area");
    out.push(("area", c));
    let s = shares();
    let mut c = mk(&s, Range::new(0, 0, 4, 1), ChartKind::PieExploded);
    set_title(&mut c, "Where the money went");
    set_data_labels(&mut c, Some(ChartLabelPosition::BestFit));
    out.push(("pie", c));
    let mut c = mk(&s, Range::new(0, 0, 4, 1), ChartKind::Doughnut);
    set_title(&mut c, "Ticket money");
    set_data_labels(&mut c, Some(ChartLabelPosition::Center));
    out.push(("doughnut", c));
    let f = firings();
    let mut c = mk(&f, Range::new(0, 0, 10, 1), ChartKind::ScatterMarkers);
    set_title(&mut c, "Shrinkage by temperature");
    set_element(&mut c, ChartElementKind::LinearTrendline, true);
    set_element(&mut c, ChartElementKind::PrimaryHorizontalAxisTitle, true);
    out.push(("scatter", c));
    let mut c = mk(&f, Range::new(0, 0, 10, 2), ChartKind::Bubble);
    set_title(&mut c, "Pieces per firing");
    out.push(("bubble", c));
    let mut c = mk(&skills(), Range::new(0, 0, 6, 2), ChartKind::RadarMarkers);
    set_title(&mut c, "Studio skills");
    out.push(("radar", c));
    let mut c = mk(&skills(), Range::new(0, 0, 6, 2), ChartKind::RadarFilled);
    set_title(&mut c, "Studio skills, filled");
    out.push(("radar-filled", c));
    let mut c = mk(
        &months(),
        Range::new(0, 0, 12, 2),
        ChartKind::ComboColumnLineSecondary,
    );
    set_title(&mut c, "Spend and members");
    out.push(("combo", c));
    let h = studios();
    let mut c = mk(&h, Range::new(0, 0, 9, 2), ChartKind::Treemap);
    set_title(&mut c, "Members by studio");
    out.push(("treemap", c));
    let mut c = mk(&h, Range::new(0, 0, 9, 2), ChartKind::Sunburst);
    set_title(&mut c, "Members by region");
    out.push(("sunburst", c));
    out
}

fn render_png(
    cs: &ChartSpace,
    mode: ColorMode,
    scale: f32,
    text: &mut TextMeasurer,
) -> kiln_raster::Rgba {
    let (w, h) = (560.0, 360.0);
    let (_, delta) = scene_delta(cs, w, h, None, mode, scale, text);
    let palette = match mode {
        ColorMode::Screen { dark: true } => Palette::dark(),
        _ => Palette::light(),
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let view = View {
        width: (w * scale) as u32,
        height: (h * scale) as u32,
        scroll_x: 0.0,
        scroll_y: 0.0,
        options: kiln_raster::PaintOptions::default(),
    };
    render_delta(&delta, &view, &palette).expect("render")
}

#[test]
fn every_chart_type_matches_its_golden_render() {
    let mut text = TextMeasurer::new(default_fonts());
    let dir = repo().join("fixtures/golden/charts");
    let preview = std::env::var_os("KILN_CHART_PREVIEW").map(PathBuf::from);
    let samples = samples();
    assert!(samples.len() >= 13);
    let mut failures = Vec::new();
    for (name, cs) in &samples {
        for (theme, mode) in [
            ("light", ColorMode::Screen { dark: false }),
            ("dark", ColorMode::Screen { dark: true }),
        ] {
            let img = render_png(cs, mode, 1.0, &mut text);
            if let Err(e) = golden::check(&img, &dir.join(format!("{name}-{theme}.png"))) {
                failures.push(e);
            }
            if let Some(p) = &preview {
                std::fs::create_dir_all(p).unwrap();
                let big = render_png(cs, mode, 2.0, &mut text);
                std::fs::write(
                    p.join(format!("{name}-{theme}.png")),
                    big.encode_png().unwrap(),
                )
                .unwrap();
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The fixture charts as Office would print them (document colours), from the cached values.
#[test]
fn fixture_charts_render_in_document_colours() {
    let mut text = TextMeasurer::new(default_fonts());
    let dir = repo().join("fixtures/golden/charts");
    let preview = std::env::var_os("KILN_CHART_PREVIEW").map(PathBuf::from);
    let bytes = std::fs::read(repo().join("fixtures/pptx/features/charts.pptx")).unwrap();
    let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes).unwrap();
    let mut failures = Vec::new();
    for i in 1..=9 {
        let part = pkg
            .part_bytes(&format!("/ppt/charts/chart{i}.xml"))
            .unwrap();
        let cs = read_chart(&part).unwrap();
        let img = render_png(&cs, ColorMode::Document, 1.0, &mut text);
        if let Err(e) = golden::check(&img, &dir.join(format!("pptx-chart{i}.png"))) {
            failures.push(e);
        }
        if let Some(p) = &preview {
            std::fs::create_dir_all(p).unwrap();
            let big = render_png(&cs, ColorMode::Document, 2.0, &mut text);
            std::fs::write(
                p.join(format!("pptx-chart{i}.png")),
                big.encode_png().unwrap(),
            )
            .unwrap();
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
