// SPDX-License-Identifier: AGPL-3.0-or-later
//! Preset and custom geometry. Ported from `reference/ts-core/ooxml/test/presets.test.ts`, plus
//! checks of the `kurbo` path conversion.

use kiln_ooxml::drawingml::geometry::{
    GeometryError, HandleKind, ResolvedHandle, Segment, adjust_values, eval_formula,
    parse_geometry, serialize_custom_geometry,
};
use kiln_ooxml::drawingml::presets::{
    PRESET_COUNT, PresetCategory, compiled_preset, is_preset_shape_type, preset_category,
    preset_definition, preset_geometry, preset_names, preset_to_svg_path, presets_in,
};
use kiln_ooxml::st::PathFillMode;
use kiln_ooxml::xml::{element_to_bytes, parse_xml, parse_xml_str};
use kurbo::{PathEl, Point, Rect, Shape};

fn open_presets() -> Vec<&'static str> {
    presets_in(PresetCategory::LinesConnectors).collect()
}

#[test]
fn has_all_187_ecma_376_presets() {
    assert_eq!(PRESET_COUNT, 187);
    let names: Vec<&str> = preset_names().collect();
    assert_eq!(names.len(), 187);
    let mut sorted = names.clone();
    sorted.sort_unstable();
    sorted.dedup();
    assert_eq!(sorted.len(), 187, "unique");
    assert!(is_preset_shape_type("rect"));
    assert!(!is_preset_shape_type("toString"));
    assert_eq!(
        preset_category("flowChartDecision"),
        Some(PresetCategory::Flowchart)
    );
    assert_eq!(preset_category("rightArrow"), Some(PresetCategory::Arrows));
    let counts: Vec<usize> = PresetCategory::ALL
        .iter()
        .map(|c| presets_in(*c).count())
        .collect();
    assert_eq!(counts, [61, 11, 24, 29, 21, 23, 6, 12]);
}

#[test]
fn refuses_an_unknown_preset() {
    assert_eq!(
        preset_to_svg_path("notAShape", 10.0, 10.0, &[]),
        Err(GeometryError::UnknownPreset("notAShape".into()))
    );
    assert!(compiled_preset("").is_err());
    assert!(preset_definition("notAShape").is_none());
}

#[test]
fn all_presets_produce_non_empty_finite_paths() {
    let open = open_presets();
    for name in preset_names() {
        for (w, h) in [
            (200.0, 100.0),
            (100.0, 300.0),
            (914_400.0, 914_400.0),
            (200.0, 0.0),
            (0.0, 100.0),
            (0.0, 0.0),
        ] {
            let g = preset_geometry(name, w, h, &[]).unwrap();
            assert!(!g.paths.is_empty(), "{name}");
            for p in &g.paths {
                let d = p.to_svg();
                assert!(!d.is_empty(), "{name}");
                assert!(
                    !d.contains("NaN") && !d.contains("inf"),
                    "{name} {w}x{h}: {d}"
                );
                assert!(d.starts_with('M'), "{name}");
                if w > 0.0 {
                    let bez = p.to_bez_path(0.1);
                    assert!(
                        bez.elements().iter().all(|el| match el {
                            PathEl::MoveTo(p) | PathEl::LineTo(p) => p.is_finite(),
                            PathEl::QuadTo(a, b) => a.is_finite() && b.is_finite(),
                            PathEl::CurveTo(a, b, c) =>
                                a.is_finite() && b.is_finite() && c.is_finite(),
                            PathEl::ClosePath => true,
                        }),
                        "{name}"
                    );
                }
            }
            if w > 0.0 && h > 0.0 {
                if open.contains(&name) {
                    assert!(g.paths.iter().any(|p| !p.closed), "{name}");
                } else {
                    // At least one closed path describes the filled area.
                    assert!(
                        g.paths
                            .iter()
                            .any(|p| p.closed && p.to_svg().ends_with('Z')),
                        "{name}"
                    );
                }
            }
            let r = g.text_rect;
            assert!(
                [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()),
                "{name}"
            );
            assert!(
                g.connection_sites.iter().all(|c| c.pos.is_finite()),
                "{name}"
            );
        }
    }
}

/// Hand-computed expectations at w = 200, h = 100 (so ss = 100), from the formulas in
/// presetShapeDefinitions.xml with default adjust values unless stated. Arcs use DrawingML's
/// "visual" angle convention (the angle of the ray from the ellipse centre).
/// (preset, expected SVG path data, adjust values).
type HandCase = (&'static str, &'static str, &'static [(&'static str, f64)]);

const HAND: &[HandCase] = &[
    ("rect", "M0 0 L200 0 L200 100 L0 100 Z", &[]),
    ("triangle", "M0 100 L100 0 L200 100 Z", &[]),
    ("triangle", "M0 100 L50 0 L200 100 Z", &[("adj", 25000.0)]),
    ("rtTriangle", "M0 100 L0 0 L200 100 Z", &[]),
    ("diamond", "M0 50 L100 0 L200 50 L100 100 Z", &[]),
    ("parallelogram", "M0 100 L25 0 L200 0 L175 100 Z", &[]),
    ("trapezoid", "M0 100 L25 0 L175 0 L200 100 Z", &[]),
    (
        "nonIsoscelesTrapezoid",
        "M0 100 L10 0 L160 0 L200 100 Z",
        &[("adj1", 10000.0), ("adj2", 40000.0)],
    ),
    (
        "hexagon",
        "M0 50 L25 0 L175 0 L200 50 L175 100 L25 100 Z",
        &[],
    ),
    (
        "octagon",
        "M0 29.289 L29.289 0 L170.711 0 L200 29.289 L200 70.711 L170.711 100 L29.289 100 L0 70.711 Z",
        &[],
    ),
    (
        "plus",
        "M0 25 L25 25 L25 0 L175 0 L175 25 L200 25 L200 75 L175 75 L175 100 L25 100 L25 75 L0 75 Z",
        &[],
    ),
    (
        "frame",
        "M0 0 L200 0 L200 100 L0 100 Z M12.5 12.5 L12.5 87.5 L187.5 87.5 L187.5 12.5 Z",
        &[],
    ),
    (
        "halfFrame",
        "M0 0 L200 0 L160 20 L30 20 L30 85 L0 100 Z",
        &[("adj1", 20000.0), ("adj2", 30000.0)],
    ),
    ("corner", "M0 0 L50 0 L50 50 L200 50 L200 100 L0 100 Z", &[]),
    ("diagStripe", "M0 50 L100 0 L200 0 L0 100 Z", &[]),
    (
        "snip1Rect",
        "M0 0 L183.333 0 L200 16.667 L200 100 L0 100 Z",
        &[],
    ),
    (
        "snip2SameRect",
        "M16.667 0 L183.333 0 L200 16.667 L200 100 L200 100 L0 100 L0 100 L0 16.667 Z",
        &[],
    ),
    (
        "snip2DiagRect",
        "M0 0 L183.333 0 L200 16.667 L200 100 L200 100 L16.667 100 L0 83.333 L0 0 Z",
        &[],
    ),
    (
        "round1Rect",
        "M0 0 L183.333 0 A16.667 16.667 0 0 1 200 16.667 L200 100 L0 100 Z",
        &[],
    ),
    (
        "round2SameRect",
        "M16.667 0 L183.333 0 A16.667 16.667 0 0 1 200 16.667 L200 100 L200 100 L0 100 L0 100 L0 16.667 A16.667 16.667 0 0 1 16.667 0 Z",
        &[],
    ),
    (
        "round2DiagRect",
        "M16.667 0 L200 0 L200 0 L200 83.333 A16.667 16.667 0 0 1 183.333 100 L0 100 L0 100 L0 16.667 A16.667 16.667 0 0 1 16.667 0 Z",
        &[],
    ),
    (
        "roundRect",
        "M0 16.667 A16.667 16.667 0 0 1 16.667 0 L183.333 0 A16.667 16.667 0 0 1 200 16.667 L200 83.333 A16.667 16.667 0 0 1 183.333 100 L16.667 100 A16.667 16.667 0 0 1 0 83.333 Z",
        &[],
    ),
    (
        "ellipse",
        "M0 50 A100 50 0 0 1 100 0 A100 50 0 0 1 200 50 A100 50 0 0 1 100 100 A100 50 0 0 1 0 50 Z",
        &[],
    ),
    ("flowChartProcess", "M0 0 L200 0 L200 100 L0 100 Z", &[]),
    ("flowChartDecision", "M0 50 L100 0 L200 50 L100 100 Z", &[]),
    (
        "flowChartInputOutput",
        "M0 100 L40 0 L200 0 L160 100 Z",
        &[],
    ),
    (
        "flowChartManualInput",
        "M0 20 L200 0 L200 100 L0 100 Z",
        &[],
    ),
    (
        "flowChartManualOperation",
        "M0 0 L200 0 L160 100 L40 100 Z",
        &[],
    ),
    ("flowChartMerge", "M0 0 L200 0 L100 100 Z", &[]),
    ("flowChartExtract", "M0 100 L100 0 L200 100 Z", &[]),
    (
        "flowChartPreparation",
        "M0 50 L40 0 L160 0 L200 50 L160 100 L40 100 Z",
        &[],
    ),
    (
        "flowChartPunchedCard",
        "M0 20 L40 0 L200 0 L200 100 L0 100 Z",
        &[],
    ),
    (
        "flowChartOffpageConnector",
        "M0 0 L200 0 L200 80 L100 100 L0 80 Z",
        &[],
    ),
    (
        "flowChartCollate",
        "M0 0 L200 0 L100 50 L200 100 L0 100 L100 50 Z",
        &[],
    ),
    (
        "flowChartInternalStorage",
        "M0 0 L200 0 L200 100 L0 100 Z M25 0 L25 100 M0 12.5 L200 12.5 M0 0 L200 0 L200 100 L0 100 Z",
        &[],
    ),
    (
        "flowChartPredefinedProcess",
        "M0 0 L200 0 L200 100 L0 100 Z M25 0 L25 100 M175 0 L175 100 M0 0 L200 0 L200 100 L0 100 Z",
        &[],
    ),
    (
        "flowChartConnector",
        "M0 50 A100 50 0 0 1 100 0 A100 50 0 0 1 200 50 A100 50 0 0 1 100 100 A100 50 0 0 1 0 50 Z",
        &[],
    ),
    (
        "flowChartDelay",
        "M0 0 L100 0 A100 50 0 0 1 100 100 L0 100 Z",
        &[],
    ),
    ("homePlate", "M0 0 L150 0 L200 50 L150 100 L0 100 Z", &[]),
    (
        "chevron",
        "M0 0 L150 0 L200 50 L150 100 L0 100 L50 50 Z",
        &[],
    ),
    (
        "rightArrow",
        "M0 25 L150 25 L150 0 L200 50 L150 100 L150 75 L0 75 Z",
        &[],
    ),
    (
        "leftArrow",
        "M0 50 L50 0 L50 25 L200 25 L200 75 L50 75 L50 100 Z",
        &[],
    ),
    (
        "upArrow",
        "M0 50 L100 0 L200 50 L150 50 L150 100 L50 100 L50 50 Z",
        &[],
    ),
    (
        "downArrow",
        "M0 50 L50 50 L50 0 L150 0 L150 50 L200 50 L100 100 Z",
        &[],
    ),
    (
        "leftRightArrow",
        "M0 50 L50 0 L50 25 L150 25 L150 0 L200 50 L150 100 L150 75 L50 75 L50 100 Z",
        &[],
    ),
    (
        "upDownArrow",
        "M0 25 L100 0 L200 25 L150 25 L150 75 L200 75 L100 100 L0 75 L50 75 L50 25 Z",
        &[("adj2", 25000.0)],
    ),
    (
        "notchedRightArrow",
        "M0 25 L150 25 L150 0 L200 50 L150 100 L150 75 L0 75 L25 50 Z",
        &[],
    ),
    (
        "mathMinus",
        "M26.51 38.24 L173.49 38.24 L173.49 61.76 L26.51 61.76 Z",
        &[],
    ),
    (
        "mathPlus",
        "M26.51 38.24 L88.24 38.24 L88.24 13.255 L111.76 13.255 L111.76 38.24 L173.49 38.24 L173.49 61.76 L111.76 61.76 L111.76 86.745 L88.24 86.745 L88.24 61.76 L26.51 61.76 Z",
        &[],
    ),
    ("line", "M0 0 L200 100", &[]),
    ("straightConnector1", "M0 0 L200 100", &[]),
    ("bentConnector2", "M0 0 L200 0 L200 100", &[]),
    ("bentConnector3", "M0 0 L100 0 L100 100 L200 100", &[]),
    (
        "bentConnector3",
        "M0 0 L50 0 L50 100 L200 100",
        &[("adj1", 25000.0)],
    ),
    // Pie from 0° to 270° (visual angles), split into two 135° arcs; the 135° point on the
    // 100 × 50 ellipse lies on the ray y = −x: (100 − 44.721, 50 + 44.721).
    (
        "pie",
        "M200 50 A100 50 0 0 1 55.279 94.721 A100 50 0 0 1 100 0 L100 50 Z",
        &[],
    ),
];

#[test]
fn hand_computed_preset_paths() {
    for (name, d, adj) in HAND {
        assert_eq!(
            preset_to_svg_path(name, 200.0, 100.0, adj).unwrap(),
            *d,
            "{name} {adj:?}"
        );
    }
    let mut distinct: Vec<&str> = HAND.iter().map(|(n, _, _)| *n).collect();
    distinct.sort_unstable();
    distinct.dedup();
    assert!(distinct.len() >= 40);
}

#[test]
fn keeps_per_path_fill_modes_and_stroke_flags() {
    let cube = preset_geometry("cube", 200.0, 100.0, &[]).unwrap();
    let flags: Vec<(PathFillMode, bool)> = cube.paths.iter().map(|p| (p.fill, p.stroke)).collect();
    assert_eq!(
        flags,
        [
            (PathFillMode::Norm, false),
            (PathFillMode::DarkenLess, false),
            (PathFillMode::LightenLess, false),
            (PathFillMode::None, true)
        ]
    );
    assert_eq!(cube.paths[0].to_svg(), "M0 25 L175 25 L175 100 L0 100 Z");
    assert_eq!(cube.paths[1].to_svg(), "M175 25 L200 0 L200 75 L175 100 Z");
    assert_eq!(cube.paths[2].to_svg(), "M0 25 L25 0 L200 0 L175 25 Z");
    assert_eq!(
        cube.paths[3].to_svg(),
        "M0 25 L25 0 L200 0 L200 75 L175 100 L0 100 Z M0 25 L175 25 L200 0 M175 25 L175 100"
    );
    let bevel = preset_geometry("bevel", 200.0, 100.0, &[]).unwrap();
    assert_eq!(
        bevel.paths[0].to_svg(),
        "M12.5 12.5 L187.5 12.5 L187.5 87.5 L12.5 87.5 Z"
    );
    assert_eq!(
        bevel.paths[1].to_svg(),
        "M0 0 L200 0 L187.5 12.5 L12.5 12.5 Z"
    );
    assert_eq!(bevel.paths[1].fill, PathFillMode::LightenLess);
}

#[test]
fn evaluates_text_rectangles_connection_sites_and_adjust_handles() {
    let tri = preset_geometry("triangle", 200.0, 100.0, &[]).unwrap();
    // rect l="x1" t="vc" r="x3" b="b" with x1 = w·adj/200000 = 50 and x3 = x1 + w/2 = 150.
    assert_eq!(tri.text_rect, Rect::new(50.0, 50.0, 150.0, 100.0));
    assert!(
        tri.connection_sites
            .iter()
            .any(|c| c.angle == 16_200_000.0 && c.pos == Point::new(100.0, 0.0))
    );
    assert_eq!(
        tri.handles,
        [ResolvedHandle {
            kind: HandleKind::Xy,
            pos: Point::new(100.0, 0.0),
            refs: vec!["adj".into()],
            limits: [Some(0.0), Some(100_000.0), None, None],
        }]
    );
    let rr = compiled_preset("roundRect").unwrap();
    assert_eq!(
        rr.evaluate(200.0, 100.0, &[("adj", 50000.0)]).get("x1"),
        Some(50.0)
    );
    assert_eq!(rr.adjust_names().collect::<Vec<_>>(), ["adj"]);
    // Non-finite overrides are ignored, as Office ignores unusable adjust values.
    assert_eq!(
        rr.evaluate(200.0, 100.0, &[("adj", f64::NAN)]).get("adj"),
        Some(16_667.0)
    );
    let arc = preset_geometry("arc", 200.0, 100.0, &[]).unwrap();
    assert_eq!(arc.handles[0].kind, HandleKind::Polar);
}

#[test]
fn guide_formulas() {
    let env = [("a", 3.0), ("b", 4.0), ("c", -2.0)];
    let cases = [
        ("val 7", 7.0),
        ("*/ a b 2", 6.0),
        ("+- a b 1", 6.0),
        ("+/ a b 7", 1.0),
        ("?: c a b", 4.0),
        ("?: a a b", 3.0),
        ("abs c", 2.0),
        ("max a b", 4.0),
        ("min a c", -2.0),
        ("mod a b 0", 5.0),
        ("pin 0 c 10", 0.0),
        ("pin 0 b 3", 3.0),
        ("sqrt 16", 4.0),
        ("cat2 10 a b", 6.0),
        ("sat2 10 a b", 8.0),
        ("cos 2 0", 2.0),
        ("sin 2 5400000", 2.0),
        ("at2 1 1", 2_700_000.0),
        ("tan 2 2700000", 2.0),
    ];
    for (f, expected) in cases {
        let got = eval_formula(f, &env).unwrap();
        assert!((got - expected).abs() < 1e-6, "{f} = {got}");
    }
    assert_eq!(
        eval_formula("pow a b", &env),
        Err(GeometryError::UnknownOperator("pow".into()))
    );
    assert_eq!(
        eval_formula("+- zz 0 0", &env),
        Err(GeometryError::UnknownGuide("zz".into()))
    );
}

const CUST: &str = r#"<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:avLst/><a:gdLst><a:gd name="q" fmla="*/ w 1 4"/></a:gdLst><a:ahLst/><a:cxnLst><a:cxn ang="0"><a:pos x="r" y="vc"/></a:cxn></a:cxnLst><a:rect l="q" t="t" r="r" b="b"/><a:pathLst><a:path w="100" h="100"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="100" y="0"/></a:lnTo><a:quadBezTo><a:pt x="100" y="100"/><a:pt x="50" y="100"/></a:quadBezTo><a:cubicBezTo><a:pt x="25" y="100"/><a:pt x="0" y="50"/><a:pt x="0" y="0"/></a:cubicBezTo><a:close/></a:path></a:pathLst></a:custGeom>"#;

#[test]
fn custom_geometry_parses_scales_and_round_trips() {
    let def = parse_geometry(&parse_xml_str(CUST).unwrap().root);
    let g = def.resolve(200.0, 50.0, &[]).unwrap();
    assert_eq!(g.to_svg(), "M0 0 L200 0 Q200 50 100 50 C50 50 0 25 0 0 Z");
    assert_eq!(g.text_rect, Rect::new(50.0, 0.0, 200.0, 50.0));
    assert_eq!(g.connection_sites.len(), 1);
    assert_eq!(g.connection_sites[0].angle, 0.0);
    assert_eq!(g.connection_sites[0].pos, Point::new(200.0, 25.0));
    let again = parse_geometry(
        &parse_xml(&element_to_bytes(&serialize_custom_geometry(&def)).unwrap())
            .unwrap()
            .root,
    );
    assert_eq!(again, def);
}

#[test]
fn custom_geometry_reports_errors_instead_of_panicking() {
    let bad = CUST.replace(r#"fmla="*/ w 1 4""#, r#"fmla="*/ w nope 4""#);
    let def = parse_geometry(&parse_xml_str(&bad).unwrap().root);
    assert_eq!(
        def.compile().unwrap_err(),
        GeometryError::UnknownGuide("nope".into())
    );
    // A path that draws before moving still converts: the move is supplied.
    let no_move = r#"<a:custGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main"><a:pathLst><a:path><a:lnTo><a:pt x="w" y="h"/></a:lnTo><a:close/><a:lnTo><a:pt x="0" y="h"/></a:lnTo><a:arcTo wR="10" hR="10" stAng="0" swAng="5400000"/></a:path></a:pathLst></a:custGeom>"#;
    let g = parse_geometry(&parse_xml_str(no_move).unwrap().root)
        .resolve(100.0, 50.0, &[])
        .unwrap();
    let bez = g.paths[0].to_bez_path(0.1);
    assert!(matches!(bez.elements()[0], PathEl::MoveTo(_)));
    assert!(
        bez.elements()
            .iter()
            .filter(|e| matches!(e, PathEl::MoveTo(_)))
            .count()
            >= 2
    );
}

#[test]
fn arcs_become_accurate_cubic_paths() {
    // An ellipse preset converts to a closed cubic path whose area matches π·a·b.
    let g = preset_geometry("ellipse", 200.0, 100.0, &[]).unwrap();
    let bez = g.paths[0].to_bez_path(0.01);
    let area = bez.area().abs();
    let exact = std::f64::consts::PI * 100.0 * 50.0;
    assert!(
        (area - exact).abs() / exact < 1e-3,
        "area {area} vs {exact}"
    );
    let bb = bez.bounding_box();
    assert!((bb.x0 - 0.0).abs() < 0.05 && (bb.x1 - 200.0).abs() < 0.05);
    assert!((bb.y0 - 0.0).abs() < 0.05 && (bb.y1 - 100.0).abs() < 0.05);
    // A pie arc ends where the SVG arc ends.
    let pie = preset_geometry("pie", 200.0, 100.0, &[]).unwrap();
    let Segment::ArcTo { .. } = pie.paths[0].segments[1] else {
        panic!("expected an arc")
    };
    let bez = pie.paths[0].to_bez_path(0.01);
    let last_curve_end = bez
        .elements()
        .iter()
        .rev()
        .find_map(|e| match e {
            PathEl::CurveTo(_, _, p) => Some(*p),
            _ => None,
        })
        .unwrap();
    assert!((last_curve_end - Point::new(100.0, 0.0)).hypot() < 1e-6);
    // Negative sweeps go the other way round.
    let blk = preset_geometry("blockArc", 200.0, 100.0, &[]).unwrap();
    assert!(blk.paths[0].to_bez_path(0.1).area().abs() > 0.0);
}

#[test]
fn reads_adjust_values_from_prst_geom() {
    let prst = parse_xml_str(r#"<a:prstGeom xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 50000"/><a:gd name="bad" fmla="*/ 1 2 3"/></a:avLst></a:prstGeom>"#).unwrap().root;
    let adj = adjust_values(prst.child("a:avLst"));
    assert_eq!(adj, [("adj".to_owned(), 50000.0)]);
    let pairs: Vec<(&str, f64)> = adj.iter().map(|(n, v)| (n.as_str(), *v)).collect();
    assert_eq!(
        preset_to_svg_path("roundRect", 200.0, 100.0, &pairs).unwrap(),
        "M0 50 A50 50 0 0 1 50 0 L150 0 A50 50 0 0 1 200 50 L200 50 A50 50 0 0 1 150 100 L50 100 A50 50 0 0 1 0 50 Z"
    );
}

#[test]
fn every_preset_round_trips_through_cust_geom() {
    for name in preset_names() {
        let def = preset_definition(name).unwrap();
        let xml = element_to_bytes(&serialize_custom_geometry(&def)).unwrap();
        let back = parse_geometry(&parse_xml(&xml).unwrap().root);
        assert_eq!(back, def, "{name}");
        // And the freeform draws exactly what the preset draws.
        assert_eq!(
            back.resolve(300.0, 200.0, &[]).unwrap().to_svg(),
            preset_to_svg_path(name, 300.0, 200.0, &[]).unwrap(),
            "{name}"
        );
    }
}
