// SPDX-License-Identifier: AGPL-3.0-or-later
//! Conditional formatting evaluation: one test per rule type, plus priorities, stop-if-true and
//! the x14 data-bar and icon-set options.

mod support;

use kiln_formula::values::ErrorValue;
use kiln_sheet::cf::CellVisual;
use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{
    AxisPosition, CfKind, CfRule, Cfvo, CfvoType, ConditionalFormat, CustomIcon, DataBar, IconSet,
    IconSetName, Operator, TimePeriod,
};
use kiln_sheet::styles::{Dxf, Fill, FontPatch};
use kiln_sheet::{CellPos, CellValue, Color, Command, Range, SheetId};
use rustc_hash::FxHashMap;
use support::{Fixture, NOW, setup};

/// A1:A10 = 1…10, B1:B6 = text, C1:C4 = mixed.
fn fixture() -> (Fixture, SheetId, u32, u32) {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    for i in 1..=10 {
        f.set(s, &format!("A{i}"), i);
    }
    for (i, t) in ["apple", "Banana", "grape", "pineapple", "", "Apple"]
        .iter()
        .enumerate()
    {
        if !t.is_empty() {
            f.set(s, &format!("B{}", i + 1), *t);
        }
    }
    f.set(s, "C1", "=1/0");
    f.set(s, "C2", 5);
    let red = f.wb.styles_mut().intern_dxf(Dxf {
        fill: Some(Fill::solid(Color::rgb(0xFFC7CE))),
        ..Dxf::default()
    });
    let bold = f.wb.styles_mut().intern_dxf(Dxf {
        font: Some(FontPatch {
            bold: Some(true),
            ..FontPatch::default()
        }),
        fill: Some(Fill::solid(Color::rgb(0x00FF00))),
        ..Dxf::default()
    });
    (f, s, red, bold)
}

fn apply(
    f: &mut Fixture,
    s: SheetId,
    ranges: Vec<Range>,
    kinds: Vec<CfKind>,
) -> FxHashMap<CellPos, CellVisual> {
    let rules = kinds
        .into_iter()
        .enumerate()
        .map(|(i, kind)| CfRule {
            id: i as u32 + 1,
            priority: i as u32 + 1,
            stop_if_true: false,
            kind,
        })
        .collect();
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Cf(vec![ConditionalFormat { ranges, rules }])),
    });
    f.wb.conditional_formats(s, Range::new(0, 0, 20, 5))
}

fn hits(v: &FxHashMap<CellPos, CellVisual>) -> Vec<String> {
    let mut out: Vec<String> = v
        .iter()
        .filter(|(_, x)| x.dxf.is_some())
        .map(|(p, _)| kiln_sheet::address::cell_a1(p.row, p.col, false))
        .collect();
    out.sort_by_key(|a| (a.len(), a.clone()));
    out
}

fn col_a() -> Vec<Range> {
    vec![Range::new(0, 0, 9, 0)]
}

#[test]
fn cell_value_rules_compare_with_every_operator() {
    let (mut f, s, red, _) = fixture();
    let case = |op: Operator, a: &str, b: Option<&str>| CfKind::CellIs {
        operator: op,
        formulas: std::iter::once(a.to_owned())
            .chain(b.map(str::to_owned))
            .collect(),
        dxf: red,
    };
    let mut check = |kind: CfKind, want: &[&str]| {
        let v = apply(&mut f, s, col_a(), vec![kind]);
        assert_eq!(hits(&v), want);
    };
    check(case(Operator::GreaterThan, "8", None), &["A9", "A10"]);
    check(case(Operator::LessThan, "2", None), &["A1"]);
    check(
        case(Operator::GreaterThanOrEqual, "9", None),
        &["A9", "A10"],
    );
    check(case(Operator::LessThanOrEqual, "1", None), &["A1"]);
    check(case(Operator::Equal, "5", None), &["A5"]);
    check(
        case(Operator::NotEqual, "5", None).clone(),
        &["A1", "A2", "A3", "A4", "A6", "A7", "A8", "A9", "A10"],
    );
    check(case(Operator::Between, "3", Some("4")), &["A3", "A4"]);
    check(case(Operator::Between, "4", Some("3")), &["A3", "A4"]);
    check(case(Operator::NotBetween, "2", Some("9")), &["A1", "A10"]);
    // Operands are formulas relative to the first cell: compare each cell with its neighbour.
    check(case(Operator::Equal, "$C$2", None), &["A5"]);
}

#[test]
fn expression_rules_evaluate_relative_to_each_cell() {
    let (mut f, s, red, _) = fixture();
    let v = apply(
        &mut f,
        s,
        col_a(),
        vec![CfKind::Expression {
            formula: "MOD(A1,4)=0".into(),
            dxf: red,
        }],
    );
    assert_eq!(hits(&v), vec!["A4", "A8"]);
}

#[test]
fn text_rules_ignore_case() {
    let (mut f, s, red, _) = fixture();
    let b = vec![Range::new(0, 1, 5, 1)];
    let mut run = |kind: CfKind| hits(&apply(&mut f, s, b.clone(), vec![kind]));
    assert_eq!(
        run(CfKind::ContainsText {
            text: "APPLE".into(),
            dxf: red
        }),
        vec!["B1", "B4", "B6"]
    );
    assert_eq!(
        run(CfKind::NotContainsText {
            text: "apple".into(),
            dxf: red
        }),
        vec!["B2", "B3", "B5"]
    );
    assert_eq!(
        run(CfKind::BeginsWith {
            text: "ap".into(),
            dxf: red
        }),
        vec!["B1", "B6"]
    );
    assert_eq!(
        run(CfKind::EndsWith {
            text: "PE".into(),
            dxf: red
        }),
        vec!["B3"]
    );
    assert_eq!(run(CfKind::ContainsBlanks { dxf: red }), vec!["B5"]);
    assert_eq!(run(CfKind::NotContainsBlanks { dxf: red }).len(), 5);
}

#[test]
fn error_rules() {
    let (mut f, s, red, _) = fixture();
    let c = vec![Range::new(0, 2, 1, 2)];
    assert_eq!(
        hits(&apply(
            &mut f,
            s,
            c.clone(),
            vec![CfKind::ContainsErrors { dxf: red }]
        )),
        vec!["C1"]
    );
    assert_eq!(
        hits(&apply(
            &mut f,
            s,
            c,
            vec![CfKind::NotContainsErrors { dxf: red }]
        )),
        vec!["C2"]
    );
    assert_eq!(f.val(s, "C1"), CellValue::Error(ErrorValue::Div0));
}

#[test]
fn date_occurring_rules_use_the_workbook_clock() {
    let (mut f, s, red, _) = fixture();
    let today = NOW.floor();
    for (i, d) in [-8.0, -6.0, -1.0, 0.0, 1.0, 30.0].iter().enumerate() {
        f.set(s, &format!("D{}", i + 1), today + d);
    }
    let d = vec![Range::new(0, 3, 5, 3)];
    let mut run = |p: TimePeriod| {
        hits(&apply(
            &mut f,
            s,
            d.clone(),
            vec![CfKind::TimePeriod {
                period: p,
                dxf: red,
            }],
        ))
    };
    assert_eq!(run(TimePeriod::Today), vec!["D4"]);
    assert_eq!(run(TimePeriod::Yesterday), vec!["D3"]);
    assert_eq!(run(TimePeriod::Tomorrow), vec!["D5"]);
    assert_eq!(run(TimePeriod::Last7Days), vec!["D2", "D3", "D4"]);
    // 2026-10-15 is a Thursday: this week is Sunday 11 … Saturday 17 October.
    assert_eq!(run(TimePeriod::ThisWeek), vec!["D3", "D4", "D5"]);
    assert_eq!(run(TimePeriod::LastWeek), vec!["D1", "D2"]);
    assert_eq!(run(TimePeriod::NextMonth), vec!["D6"]);
    assert_eq!(run(TimePeriod::ThisMonth).len(), 5);
}

#[test]
fn duplicate_and_unique_values_compare_text_ignoring_case() {
    let (mut f, s, red, _) = fixture();
    f.set(s, "E1", "x");
    f.set(s, "E2", "X");
    f.set(s, "E3", 1);
    f.set(s, "E4", "y");
    let e = vec![Range::new(0, 4, 4, 4)];
    assert_eq!(
        hits(&apply(
            &mut f,
            s,
            e.clone(),
            vec![CfKind::DuplicateValues { dxf: red }]
        )),
        vec!["E1", "E2"]
    );
    assert_eq!(
        hits(&apply(
            &mut f,
            s,
            e,
            vec![CfKind::UniqueValues { dxf: red }]
        )),
        vec!["E3", "E4"]
    );
}

#[test]
fn top_and_bottom_rules_by_count_and_percent() {
    let (mut f, s, red, _) = fixture();
    let mut run = |rank, percent, bottom| {
        hits(&apply(
            &mut f,
            s,
            col_a(),
            vec![CfKind::Top10 {
                rank,
                percent,
                bottom,
                dxf: red,
            }],
        ))
    };
    assert_eq!(run(3, false, false), vec!["A8", "A9", "A10"]);
    assert_eq!(run(2, false, true), vec!["A1", "A2"]);
    assert_eq!(run(20, true, false), vec!["A9", "A10"]);
    assert_eq!(run(1, true, true), vec!["A1"], "at least one item");
}

#[test]
fn above_and_below_average_with_standard_deviations() {
    let (mut f, s, red, _) = fixture();
    let mut run = |above, equal, std_dev| {
        hits(&apply(
            &mut f,
            s,
            col_a(),
            vec![CfKind::AboveAverage {
                above,
                equal,
                std_dev,
                dxf: red,
            }],
        ))
    };
    assert_eq!(run(true, false, 0), vec!["A6", "A7", "A8", "A9", "A10"]);
    assert_eq!(run(false, false, 0), vec!["A1", "A2", "A3", "A4", "A5"]);
    // Mean 5.5, sample standard deviation 3.03: one above is 8.53.
    assert_eq!(run(true, false, 1), vec!["A9", "A10"]);
    assert_eq!(run(false, true, 1), vec!["A1", "A2"]);
}

#[test]
fn colour_scales_interpolate_between_their_thresholds() {
    let (mut f, s, _, _) = fixture();
    let v = apply(
        &mut f,
        s,
        col_a(),
        vec![CfKind::ColorScale {
            cfvos: vec![
                Cfvo::of(CfvoType::Min),
                Cfvo::with(CfvoType::Percentile, "50"),
                Cfvo::of(CfvoType::Max),
            ],
            colors: vec![
                Color::rgb(0xF8696B),
                Color::rgb(0xFFEB84),
                Color::rgb(0x63BE7B),
            ],
        }],
    );
    let color = |r: u32| v.get(&CellPos::new(r, 0)).and_then(|x| x.scale_color);
    assert_eq!(color(0), Some(0xF8696B));
    assert_eq!(color(9), Some(0x63BE7B));
    assert!(color(4).is_some());
    assert_ne!(color(4), color(5));
    // Two-colour scale with numbers.
    let v = apply(
        &mut f,
        s,
        col_a(),
        vec![CfKind::ColorScale {
            cfvos: vec![
                Cfvo::with(CfvoType::Num, "0"),
                Cfvo::with(CfvoType::Num, "10"),
            ],
            colors: vec![Color::rgb(0x000000), Color::rgb(0xFFFFFF)],
        }],
    );
    assert_eq!(
        v.get(&CellPos::new(4, 0)).and_then(|x| x.scale_color),
        Some(0x808080)
    );
}

fn bar(min: Cfvo, max: Cfvo) -> DataBar {
    DataBar {
        min,
        max,
        color: Color::rgb(0x638EC6),
        show_value: true,
        min_length: 0,
        max_length: 100,
        gradient: true,
        border_color: None,
        negative_color: Some(Color::rgb(0xFF0000)),
        negative_border_color: None,
        axis_color: Some(Color::rgb(0x000000)),
        axis_position: AxisPosition::Automatic,
        direction: Default::default(),
    }
}

#[test]
fn data_bars_scale_between_min_and_max() {
    let (mut f, s, _, _) = fixture();
    let v = apply(
        &mut f,
        s,
        col_a(),
        vec![CfKind::DataBar(bar(
            Cfvo::of(CfvoType::Min),
            Cfvo::of(CfvoType::Max),
        ))],
    );
    let b = |r: u32| {
        v.get(&CellPos::new(r, 0))
            .and_then(|x| x.data_bar.clone())
            .expect("bar")
    };
    assert_eq!(b(0).end, 0.0);
    assert_eq!(b(9).end, 1.0);
    assert!((b(4).end - 4.0 / 9.0).abs() < 1e-12);
    assert!(b(4).axis.is_none());
}

#[test]
fn data_bars_draw_negative_values_from_an_axis() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    for (i, x) in [-10, -5, 0, 10, 30].iter().enumerate() {
        f.set(s, &format!("A{}", i + 1), *x);
    }
    let mut b = bar(Cfvo::of(CfvoType::AutoMin), Cfvo::of(CfvoType::AutoMax));
    b.show_value = false;
    let v = apply(
        &mut f,
        s,
        vec![Range::new(0, 0, 4, 0)],
        vec![CfKind::DataBar(b)],
    );
    let at = |r: u32| v.get(&CellPos::new(r, 0)).expect("visual").clone();
    let neg = at(0).data_bar.expect("bar");
    // The axis sits where 0 falls between -10 and 30.
    assert!((neg.axis.map(|a| a.0).unwrap_or(0.0) - 0.25).abs() < 1e-12);
    assert!(neg.negative);
    assert_eq!(neg.color, 0xFF0000);
    assert_eq!((neg.start, neg.end), (0.0, 0.25));
    let pos = at(4).data_bar.expect("bar");
    assert!(!pos.negative);
    assert_eq!((pos.start, pos.end), (0.25, 1.0));
    assert!(at(0).hide_value);
}

#[test]
fn icon_sets_pick_icons_by_threshold_with_x14_options() {
    let (mut f, s, _, _) = fixture();
    let set = |reverse, custom: Vec<CustomIcon>| {
        CfKind::IconSet(IconSet {
            icon_set: IconSetName::Arrows3,
            cfvos: vec![
                Cfvo::with(CfvoType::Percent, "0"),
                Cfvo::with(CfvoType::Percent, "33"),
                Cfvo::with(CfvoType::Percent, "67"),
            ],
            reverse,
            show_value: true,
            custom_icons: custom,
        })
    };
    let v = apply(&mut f, s, col_a(), vec![set(false, Vec::new())]);
    let icon = |v: &FxHashMap<CellPos, CellVisual>, r: u32| {
        v.get(&CellPos::new(r, 0))
            .and_then(|x| x.icon)
            .map(|i| i.index)
    };
    // Thresholds at 1 + 9×33 % = 3.97 and 1 + 9×67 % = 7.03.
    assert_eq!(icon(&v, 0), Some(0));
    assert_eq!(icon(&v, 3), Some(1));
    assert_eq!(icon(&v, 7), Some(2));
    let r = apply(&mut f, s, col_a(), vec![set(true, Vec::new())]);
    assert_eq!(icon(&r, 0), Some(2));
    let custom = apply(
        &mut f,
        s,
        col_a(),
        vec![set(
            false,
            vec![
                CustomIcon {
                    icon_set: IconSetName::NoIcons,
                    icon_id: 0,
                },
                CustomIcon {
                    icon_set: IconSetName::Stars3,
                    icon_id: 1,
                },
                CustomIcon {
                    icon_set: IconSetName::Flags3,
                    icon_id: 2,
                },
            ],
        )],
    );
    assert_eq!(custom.get(&CellPos::new(0, 0)).and_then(|x| x.icon), None);
    let star = custom
        .get(&CellPos::new(3, 0))
        .and_then(|x| x.icon)
        .expect("icon");
    assert_eq!((star.set, star.index), (IconSetName::Stars3, 1));
}

#[test]
fn priorities_layer_formats_and_stop_if_true_stops_lower_rules() {
    let (mut f, s, red, bold) = fixture();
    let rules = vec![
        CfRule {
            id: 1,
            priority: 2,
            stop_if_true: false,
            kind: CfKind::CellIs {
                operator: Operator::GreaterThan,
                formulas: vec!["0".into()],
                dxf: red,
            },
        },
        CfRule {
            id: 2,
            priority: 1,
            stop_if_true: true,
            kind: CfKind::CellIs {
                operator: Operator::GreaterThan,
                formulas: vec!["8".into()],
                dxf: bold,
            },
        },
        CfRule {
            id: 3,
            priority: 3,
            stop_if_true: false,
            kind: CfKind::Expression {
                formula: "TRUE".into(),
                dxf: bold,
            },
        },
    ];
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Cf(vec![ConditionalFormat {
            ranges: col_a(),
            rules,
        }])),
    });
    let v = f.wb.conditional_formats(s, Range::new(0, 0, 9, 0));
    let d = |r: u32| {
        v.get(&CellPos::new(r, 0))
            .and_then(|x| x.dxf.clone())
            .expect("format")
    };
    // A10 > 8: the bold rule applies and stops the others.
    let top = d(9);
    assert_eq!(top.fill, Some(Fill::solid(Color::rgb(0x00FF00))));
    assert_eq!(top.font.and_then(|x| x.bold), Some(true));
    // A1: red (priority 2) wins the fill over the lower-priority bold rule; bold still applies.
    let low = d(0);
    assert_eq!(low.fill, Some(Fill::solid(Color::rgb(0xFFC7CE))));
    assert_eq!(low.font.and_then(|x| x.bold), Some(true));
}

#[test]
fn rules_follow_structural_edits() {
    let (mut f, s, red, _) = fixture();
    apply(
        &mut f,
        s,
        col_a(),
        vec![CfKind::Expression {
            formula: "$A1>$C$2".into(),
            dxf: red,
        }],
    );
    f.exec(Command::InsertRows {
        sheet: s,
        at: 0,
        count: 1,
        format_from: Default::default(),
    });
    let block = f.wb.sheet(s).map(|x| x.meta().cf[0].clone()).expect("cf");
    assert_eq!(block.ranges, vec![Range::new(1, 0, 10, 0)]);
    assert_eq!(
        block.rules[0].kind,
        CfKind::Expression {
            formula: "$A2>$C$3".into(),
            dxf: red
        }
    );
    let v = f.wb.conditional_formats(s, Range::new(0, 0, 20, 5));
    assert_eq!(hits(&v), vec!["A7", "A8", "A9", "A10", "A11"]);
}
