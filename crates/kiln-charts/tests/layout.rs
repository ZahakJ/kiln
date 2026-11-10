// SPDX-License-Identifier: AGPL-3.0-or-later
//! Layout behaviour: legend wrapping, tick label collision avoidance, hit testing, selection, and
//! robustness against hostile parts and sizes.

use kiln_charts::edit::{insert_from_range, set_data_labels, set_legend, set_title};
use kiln_charts::layout::{ChartElement, ChartLayout, HitShape, layout_chart};
use kiln_charts::palette::{ChartColors, ColorMode};
use kiln_charts::render::{draw, selection_ops};
use kiln_charts::text::{OutlineSink, TextMeasurer, default_fonts};
use kiln_charts::{ChartSpace, read_chart, write_chart};
use kiln_protocol::commands::{ChartKind, ChartLabelPosition, ChartLegendPosition};
use kiln_scene::DrawOp;
use kiln_sheet::{CellValue, Range, Workbook};
use proptest::prelude::*;

fn wide_table(series: usize, points: usize, label: &str) -> Workbook {
    let mut wb = Workbook::new();
    let id = wb.add_sheet(Some("Data"), None, None).unwrap();
    wb.load_value(id, 0, 0, CellValue::text("Category"))
        .unwrap();
    for s in 0..series {
        wb.load_value(
            id,
            0,
            s as u32 + 1,
            CellValue::text(format!("Series number {s}")),
        )
        .unwrap();
    }
    for p in 0..points {
        wb.load_value(id, p as u32 + 1, 0, CellValue::text(format!("{label} {p}")))
            .unwrap();
        for s in 0..series {
            wb.load_value(
                id,
                p as u32 + 1,
                s as u32 + 1,
                CellValue::Number(((p * 7 + s * 13) % 50) as f64 + 1.0),
            )
            .unwrap();
        }
    }
    wb
}

fn lay(cs: &ChartSpace, w: f32, h: f32) -> ChartLayout {
    let colors = ChartColors::new(None, ColorMode::Screen { dark: false });
    let mut text = TextMeasurer::new(default_fonts());
    layout_chart(cs, w, h, &colors, &mut text)
}

#[test]
fn a_bottom_legend_wraps_into_rows() {
    let wb = wide_table(9, 12, "Q");
    let cs = insert_from_range(
        &wb,
        "Data",
        Range::new(0, 0, 12, 9),
        Some(ChartKind::ColumnClustered),
    )
    .unwrap();
    let l = lay(&cs, 420.0, 320.0);
    let entries: Vec<_> = l
        .texts
        .iter()
        .filter(|t| matches!(t.element, ChartElement::LegendEntry(_)))
        .collect();
    assert_eq!(entries.len(), 9);
    let rows: std::collections::BTreeSet<i32> =
        entries.iter().map(|t| t.y.round() as i32).collect();
    assert!(rows.len() >= 2, "legend entries should wrap: {rows:?}");
    for e in &entries {
        assert!(
            e.bounds.x >= 0.0 && e.bounds.right() <= 420.0,
            "{:?}",
            e.bounds
        );
        assert!(e.bounds.bottom() <= 320.0);
    }
    // Entries never overlap.
    for (i, a) in entries.iter().enumerate() {
        for b in &entries[i + 1..] {
            assert!(
                !a.bounds.intersects(&b.bounds),
                "{:?} / {:?}",
                a.bounds,
                b.bounds
            );
        }
    }
}

#[test]
fn crowded_category_labels_rotate_or_skip_instead_of_colliding() {
    let wb = wide_table(1, 40, "A rather long category");
    let cs = insert_from_range(
        &wb,
        "Data",
        Range::new(0, 0, 40, 1),
        Some(ChartKind::ColumnClustered),
    )
    .unwrap();
    let l = lay(&cs, 500.0, 320.0);
    let axis = cs.chart.plot_area.groups[0].axis_ids[0];
    let labels: Vec<_> = l
        .texts
        .iter()
        .filter(|t| t.element == ChartElement::Axis(axis))
        .collect();
    assert!(
        !labels.is_empty() && labels.len() < 40,
        "{} labels",
        labels.len()
    );
    for w in labels.windows(2) {
        let (a, b) = (w[0], w[1]);
        assert!(a.bounds.x >= -0.5, "inside the chart: {:?}", a.bounds);
        if a.rotation == 0.0 {
            assert!(
                !a.bounds.intersects(&b.bounds),
                "{:?} / {:?}",
                a.bounds,
                b.bounds
            );
        } else {
            // Slanted labels: neighbouring baselines are at least a line apart.
            let across = (b.bounds.x - a.bounds.x) * a.rotation.to_radians().sin().abs();
            assert!(
                across >= a.block.height * 0.95,
                "{across} < {}",
                a.block.height
            );
        }
    }
}

#[test]
fn hit_testing_finds_points_legend_title_and_axes() {
    let wb = wide_table(2, 4, "Q");
    let mut cs = insert_from_range(
        &wb,
        "Data",
        Range::new(0, 0, 4, 2),
        Some(ChartKind::ColumnClustered),
    )
    .unwrap();
    set_title(&mut cs, "Hit me");
    set_legend(&mut cs, Some(ChartLegendPosition::Right));
    let l = lay(&cs, 600.0, 400.0);
    let title = l
        .texts
        .iter()
        .find(|t| t.element == ChartElement::Title)
        .unwrap();
    let c = (
        title.bounds.x + title.bounds.w / 2.0,
        title.bounds.y + title.bounds.h / 2.0,
    );
    assert_eq!(l.hit_test(c.0, c.1), Some(ChartElement::Title));
    let bar = l
        .hits
        .iter()
        .find_map(|(e, s)| {
            matches!(
                e,
                ChartElement::Point {
                    series: 1,
                    point: 2,
                    ..
                }
            )
            .then(|| s.bounds())
        })
        .unwrap();
    assert_eq!(
        l.hit_test(bar.x + bar.w / 2.0, bar.y + bar.h / 2.0),
        Some(ChartElement::Point {
            group: 0,
            series: 1,
            point: 2
        })
    );
    let entry = l
        .texts
        .iter()
        .find(|t| t.element == ChartElement::LegendEntry(1))
        .unwrap();
    assert_eq!(
        l.hit_test(entry.bounds.x + 2.0, entry.bounds.y + entry.bounds.h / 2.0),
        Some(ChartElement::LegendEntry(1))
    );
    assert_eq!(l.hit_test(2.0, 2.0), Some(ChartElement::ChartArea));
    assert_eq!(l.hit_test(-5.0, 2.0), None);
    let series = l
        .element_bounds(ChartElement::Series {
            group: 0,
            series: 0,
        })
        .unwrap();
    assert!(series.w > 100.0, "a series covers all its points");
    let sel = selection_ops(&l, Some(ChartElement::Title));
    assert!(sel.len() > 9, "frame, 8 handles and the element outline");
}

#[test]
fn pie_labels_do_not_overlap() {
    let mut wb = Workbook::new();
    let id = wb.add_sheet(Some("Data"), None, None).unwrap();
    wb.load_value(id, 0, 0, CellValue::text("Item")).unwrap();
    wb.load_value(id, 0, 1, CellValue::text("Share")).unwrap();
    for (i, v) in [60.0, 20.0, 5.0, 4.0, 3.0, 3.0, 2.0, 2.0, 1.0]
        .iter()
        .enumerate()
    {
        wb.load_value(id, i as u32 + 1, 0, CellValue::text(format!("Item {i}")))
            .unwrap();
        wb.load_value(id, i as u32 + 1, 1, CellValue::Number(*v))
            .unwrap();
    }
    let mut cs =
        insert_from_range(&wb, "Data", Range::new(0, 0, 9, 1), Some(ChartKind::Pie)).unwrap();
    set_data_labels(&mut cs, Some(ChartLabelPosition::OutsideEnd));
    let l = lay(&cs, 480.0, 360.0);
    let labels: Vec<_> = l
        .texts
        .iter()
        .filter(|t| matches!(t.element, ChartElement::DataLabels { .. }))
        .collect();
    assert_eq!(labels.len(), 9);
    for (i, a) in labels.iter().enumerate() {
        for b in &labels[i + 1..] {
            assert!(
                !a.bounds.intersects(&b.bounds),
                "{:?} / {:?}",
                a.bounds,
                b.bounds
            );
        }
    }
}

fn fixture_parts() -> Vec<Vec<u8>> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    let mut out = Vec::new();
    for f in [
        "xlsx/features/charts.xlsx",
        "pptx/features/charts.pptx",
        "xlsx/charts/treemap.xlsx",
    ] {
        let bytes = std::fs::read(root.join(f)).unwrap();
        let pkg = kiln_ooxml::opc::OpcPackage::open(&bytes).unwrap();
        let names: Vec<String> = pkg
            .part_names()
            .filter(|n| n.contains("/charts/chart") && n.ends_with(".xml"))
            .map(str::to_owned)
            .collect();
        for n in names {
            out.push(pkg.part_bytes(&n).unwrap().into_owned());
        }
    }
    out
}

#[test]
fn every_fixture_lays_out_at_any_size_without_panicking() {
    let mut text = TextMeasurer::new(default_fonts());
    for part in fixture_parts() {
        let cs = read_chart(&part).unwrap();
        for (w, h) in [
            (1.0, 1.0),
            (40.0, 30.0),
            (300.0, 200.0),
            (1600.0, 900.0),
            (f32::NAN, 10.0),
        ] {
            for mode in [ColorMode::Document, ColorMode::Screen { dark: true }] {
                let colors = ChartColors::new(None, mode);
                let l = layout_chart(&cs, w, h, &colors, &mut text);
                let ops = draw(&l, &text, &mut OutlineSink);
                assert!(ops.iter().all(|op| match op {
                    DrawOp::Path { path, .. } => path.is_valid(),
                    _ => true,
                }));
            }
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Corrupted parts (bytes flipped, numbers replaced by extreme values) are read without a
    /// panic, and whatever is read lays out, writes and reads back.
    #[test]
    fn hostile_parts_never_panic(
        which in 0usize..20,
        cut in 0usize..4000,
        value in prop::sample::select(vec!["-1e308", "1e308", "NaN", "-0", "999999999999", "abc", "", "-2147483649"]),
        flips in prop::collection::vec((0usize..6000, any::<u8>()), 0..6),
    ) {
        let parts = fixture_parts();
        let part = &parts[which % parts.len()];
        let mut text_xml = String::from_utf8_lossy(part).into_owned();
        // Replace every val="number" with the hostile value.
        text_xml = regex_lite_replace(&text_xml, value);
        let mut bytes = text_xml.into_bytes();
        for (at, b) in flips {
            if !bytes.is_empty() {
                let i = at % bytes.len();
                bytes[i] = b;
            }
        }
        if cut < bytes.len() && cut % 3 == 0 {
            bytes.truncate(cut);
        }
        if let Ok(cs) = read_chart(&bytes) {
            let colors = ChartColors::new(None, ColorMode::Document);
            let mut text = TextMeasurer::new(default_fonts());
            let l = layout_chart(&cs, 320.0, 200.0, &colors, &mut text);
            let _ = draw(&l, &text, &mut OutlineSink);
            let written = write_chart(&cs);
            let again = read_chart(&written);
            prop_assert!(again.is_ok(), "{:?}\n{}", again.err(), String::from_utf8_lossy(&written).chars().take(900).collect::<String>());
        }
    }
}

/// Replaces the content of every `val="…"` attribute and `<c:v>…</c:v>` with `value`.
fn regex_lite_replace(s: &str, value: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("val=\"") {
        out.push_str(&rest[..i + 5]);
        rest = &rest[i + 5..];
        let end = rest.find('"').unwrap_or(rest.len());
        if rest[..end].parse::<f64>().is_ok() {
            out.push_str(value);
        } else {
            out.push_str(&rest[..end]);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[test]
fn inside_labels_on_slivers_of_a_stack_are_dropped() {
    let mut wb = Workbook::new();
    let id = wb.add_sheet(Some("Data"), None, None).unwrap();
    for (c, h) in ["Quarter", "Big", "Sliver"].iter().enumerate() {
        wb.load_value(id, 0, c as u32, CellValue::text(*h)).unwrap();
    }
    for r in 1..=4u32 {
        wb.load_value(id, r, 0, CellValue::text(format!("Q{r}")))
            .unwrap();
        wb.load_value(id, r, 1, CellValue::Number(1000.0 + f64::from(r) * 50.0))
            .unwrap();
        wb.load_value(id, r, 2, CellValue::Number(f64::from(r)))
            .unwrap();
    }
    for (kind, position) in [
        (ChartKind::ColumnStacked, ChartLabelPosition::Center),
        (ChartKind::ColumnStacked, ChartLabelPosition::InsideEnd),
        (ChartKind::ColumnStacked, ChartLabelPosition::InsideBase),
        (ChartKind::BarStacked, ChartLabelPosition::Center),
    ] {
        let mut cs = insert_from_range(&wb, "Data", Range::new(0, 0, 4, 2), Some(kind)).unwrap();
        set_data_labels(&mut cs, Some(position));
        let l = lay(&cs, 480.0, 360.0);
        let labels = |series: usize| {
            l.texts
                .iter()
                .filter(|t| {
                    matches!(t.element, ChartElement::DataLabels { series: s, .. } if s == series)
                })
                .collect::<Vec<_>>()
        };
        assert!(
            labels(1).is_empty(),
            "{kind:?} {position:?}: sliver labelled"
        );
        let big = labels(0);
        assert_eq!(big.len(), 4, "{kind:?} {position:?}");
        let segments: Vec<_> = l
            .hits
            .iter()
            .filter_map(|(e, h)| match (e, h) {
                (ChartElement::Point { series: 0, .. }, HitShape::Rect(r)) => Some(*r),
                _ => None,
            })
            .collect();
        for t in big {
            let b = t.bounds;
            assert!(
                segments.iter().any(|r| b.y >= r.y - 0.5
                    && b.bottom() <= r.bottom() + 0.5
                    && b.x >= r.x - 0.5
                    && b.right() <= r.right() + 0.5),
                "{kind:?} {position:?}: label {b:?} spills out of its segment"
            );
        }
    }
}
