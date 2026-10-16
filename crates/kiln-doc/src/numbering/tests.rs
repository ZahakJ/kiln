// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ported from `reference/ts-core/doc-model/test/numbering.test.ts`: list label computation in
//! document order (continuation across instances, start overrides, restart rules, legal
//! numbering, style-linked outline numbering, bullets) and the numbering registry.

use super::*;
use crate::model::{Block, Cell, Document, Paragraph, Row, Table};
use crate::props::{NumberingRef, TableCellProps, TableProps, TableRowProps};
use crate::resolve::{DocContext, effective_numbering};
use crate::st::StyleType;
use crate::style::{DocDefaults, Style};
use crate::theme::DocTheme;
use crate::{DocError, Fragment, Inlines};

fn lvl(level: u8, format: &str, text: &str) -> Level {
    Level::new(level, NumberFormat::parse(format), text)
}

fn styles() -> StyleSheet {
    let mut h1 = Style::new("Heading1", StyleType::Paragraph, "heading 1");
    h1.based_on = Some("Normal".into());
    h1.ppr = Some(ParagraphProps {
        numbering: Some(NumberingRef::new(5, 0)),
        ..ParagraphProps::default()
    });
    let mut h2 = Style::new("Heading2", StyleType::Paragraph, "heading 2");
    h2.based_on = Some("Normal".into());
    h2.ppr = Some(ParagraphProps {
        numbering: Some(NumberingRef {
            num_id: Some(5),
            level: None,
        }),
        ..ParagraphProps::default()
    });
    let mut normal = Style::new("Normal", StyleType::Paragraph, "Normal");
    normal.is_default = true;
    StyleSheet::new(DocDefaults::default(), [normal, h1, h2])
}

fn abs(levels: Vec<Level>) -> AbstractNum {
    AbstractNum {
        levels,
        ..AbstractNum::default()
    }
}

/// 1 = outline 1./1.1./1.1.1., 2 = second instance of the same abstract, 3 = bullets, 4 = legal,
/// 5 = heading-linked, 6 = restart rules, 7 = start 5 with a none-format middle level.
fn numbering() -> Numbering {
    let mut n = Numbering::new();
    let add = |n: &mut Numbering, id: u32, levels: Vec<Level>, nums: &[u32]| {
        n.add_abstract(abs(levels), Some(id));
        for num in nums {
            let _ = n.add_instance(id, Vec::new(), Some(*num));
        }
    };
    add(
        &mut n,
        0,
        vec![
            lvl(0, "decimal", "%1."),
            lvl(1, "decimal", "%1.%2."),
            lvl(2, "decimal", "%1.%2.%3."),
            lvl(3, "lowerLetter", "(%4)"),
            lvl(4, "lowerRoman", "%5."),
        ],
        &[1, 2],
    );
    let symbol = Level {
        rpr: Some(RunProps {
            fonts: Some(RunFonts::latin("Symbol")),
            ..RunProps::default()
        }),
        ..lvl(0, "bullet", "\u{F0B7}")
    };
    let courier = Level {
        rpr: Some(RunProps {
            fonts: Some(RunFonts {
                ascii: Some("Courier New".into()),
                ..RunFonts::default()
            }),
            ..RunProps::default()
        }),
        ..lvl(1, "bullet", "o")
    };
    add(&mut n, 1, vec![symbol, courier], &[3]);
    add(
        &mut n,
        2,
        vec![
            lvl(0, "upperRoman", "Article %1."),
            Level {
                is_legal: true,
                ..lvl(1, "decimal", "Section %1.%2")
            },
        ],
        &[4],
    );
    add(
        &mut n,
        3,
        vec![
            Level {
                p_style: Some("Heading1".into()),
                ..lvl(0, "decimal", "%1")
            },
            Level {
                p_style: Some("Heading2".into()),
                ..lvl(1, "decimal", "%1.%2")
            },
        ],
        &[5],
    );
    add(
        &mut n,
        4,
        vec![
            lvl(0, "decimal", "%1."),
            Level {
                restart_after: Some(0),
                ..lvl(1, "decimal", "%2)")
            },
            Level {
                restart_after: Some(1),
                ..lvl(2, "lowerLetter", "%3.")
            },
        ],
        &[6],
    );
    add(
        &mut n,
        5,
        vec![
            Level {
                start: 5,
                ..lvl(0, "upperLetter", "%1.")
            },
            lvl(1, "none", ""),
            lvl(2, "decimal", "%1.%2.%3"),
        ],
        &[7],
    );
    n
}

#[derive(Clone)]
enum Item {
    L(u32, u8),
    P(Box<ParagraphProps>),
}

fn style(id: &str) -> Item {
    Item::P(Box::new(ParagraphProps {
        style_id: Some(id.into()),
        ..ParagraphProps::default()
    }))
}

fn labels(items: &[Item], n: &Numbering) -> Vec<Option<String>> {
    let styles = styles();
    let theme = DocTheme::office();
    let ctx = DocContext::new(&styles, &theme).with_numbering(n);
    let mut counter = ListCounter::new(n, Some(&styles));
    items
        .iter()
        .map(|it| {
            let props = match it {
                Item::L(num, level) => ParagraphProps {
                    numbering: Some(NumberingRef::new(*num, *level)),
                    ..ParagraphProps::default()
                },
                Item::P(p) => (**p).clone(),
            };
            effective_numbering(&ctx, &props)
                .and_then(|r| counter.next(r.num_id, r.level))
                .map(|l| l.text)
        })
        .collect()
}

fn check(items: &[Item], expected: &[Option<&str>], setup: impl FnOnce(&mut Numbering)) {
    let mut n = numbering();
    setup(&mut n);
    let got = labels(items, &n);
    let expected: Vec<Option<String>> = expected.iter().map(|e| e.map(str::to_owned)).collect();
    assert_eq!(got, expected);
}

fn l(num: u32, level: u8) -> Item {
    Item::L(num, level)
}

fn some(v: &[&'static str]) -> Vec<Option<&'static str>> {
    v.iter().map(|s| Some(*s)).collect()
}

#[test]
fn simple_and_nested_sequences() {
    check(
        &[l(1, 0), l(1, 0), l(1, 0)],
        &some(&["1.", "2.", "3."]),
        |_| {},
    );
    check(
        &[l(1, 0), l(1, 1), l(1, 1)],
        &some(&["1.", "1.1.", "1.2."]),
        |_| {},
    );
    check(
        &[l(1, 0), l(1, 1), l(1, 1), l(1, 0), l(1, 1)],
        &some(&["1.", "1.1.", "1.2.", "2.", "2.1."]),
        |_| {},
    );
    check(
        &[l(1, 0), l(1, 1), l(1, 2), l(1, 2), l(1, 1), l(1, 2)],
        &some(&["1.", "1.1.", "1.1.1.", "1.1.2.", "1.2.", "1.2.1."]),
        |_| {},
    );
    check(
        &[l(1, 0), l(1, 3), l(1, 3)],
        &some(&["1.", "(a)", "(b)"]),
        |_| {},
    );
    check(
        &[l(1, 4), l(1, 4), l(1, 4), l(1, 4)],
        &some(&["i.", "ii.", "iii.", "iv."]),
        |_| {},
    );
    check(&[l(1, 1), l(1, 1)], &some(&["1.1.", "1.2."]), |_| {});
}

#[test]
fn continuation_across_paragraphs_and_instances() {
    check(
        &[l(1, 0), Item::P(Box::default()), l(1, 0)],
        &[Some("1."), None, Some("2.")],
        |_| {},
    );
    check(
        &[l(1, 0), l(2, 0), l(1, 0)],
        &some(&["1.", "2.", "3."]),
        |_| {},
    );
    check(
        &[l(1, 0), l(6, 0), l(1, 0), l(6, 0)],
        &some(&["1.", "1.", "2.", "2."]),
        |_| {},
    );
}

#[test]
fn start_overrides_restart_the_sequence() {
    let over = |start: i64| {
        move |n: &mut Numbering| {
            let _ = n.add_instance(
                0,
                vec![LevelOverride {
                    level: 0,
                    start_override: Some(start),
                    definition: None,
                }],
                Some(9),
            );
        }
    };
    check(
        &[l(1, 0), l(1, 0), l(9, 0), l(9, 0)],
        &some(&["1.", "2.", "1.", "2."]),
        over(1),
    );
    check(&[l(9, 0), l(9, 0)], &some(&["10.", "11."]), over(10));
    check(
        &[l(1, 0), l(1, 0), l(8, 0), l(1, 0)],
        &some(&["1.", "2.", "1.", "2."]),
        |n| {
            assert_eq!(n.restart(1, 0, None).map(|i| i.id).ok(), Some(8));
        },
    );
    check(
        &[l(9, 0), l(9, 0), l(9, 0)],
        &some(&["I:", "II:", "III:"]),
        |n| {
            let _ = n.add_instance(
                0,
                vec![LevelOverride {
                    level: 0,
                    start_override: None,
                    definition: Some(lvl(0, "upperRoman", "%1:")),
                }],
                Some(9),
            );
        },
    );
}

#[test]
fn bullets_map_symbol_font_glyphs() {
    check(&[l(3, 0), l(3, 0)], &some(&["•", "•"]), |_| {});
    check(&[l(3, 0), l(3, 1)], &some(&["•", "o"]), |_| {});
}

#[test]
fn legal_numbering_shows_decimal_under_roman_parts() {
    check(
        &[l(4, 0), l(4, 1), l(4, 1), l(4, 0), l(4, 1)],
        &some(&[
            "Article I.",
            "Section 1.1",
            "Section 1.2",
            "Article II.",
            "Section 2.1",
        ]),
        |_| {},
    );
}

#[test]
fn heading_styles_carry_outline_numbering() {
    check(
        &[
            style("Heading1"),
            style("Heading2"),
            style("Heading2"),
            style("Heading1"),
        ],
        &some(&["1", "1.1", "1.2", "2"]),
        |_| {},
    );
    check(&[style("Heading2")], &some(&["1.1"]), |_| {});
    let cancelled = Item::P(Box::new(ParagraphProps {
        style_id: Some("Heading1".into()),
        numbering: Some(NumberingRef::new(0, 0)),
        ..ParagraphProps::default()
    }));
    check(
        &[style("Heading1"), cancelled, style("Heading1")],
        &[Some("1"), None, Some("2")],
        |_| {},
    );
}

#[test]
fn restart_rules() {
    check(
        &[l(6, 0), l(6, 1), l(6, 1), l(6, 0), l(6, 1)],
        &some(&["1.", "1)", "2)", "2.", "3)"]),
        |_| {},
    );
    check(
        &[l(6, 0), l(6, 2), l(6, 1), l(6, 2), l(6, 0), l(6, 2)],
        &some(&["1.", "a.", "1)", "b.", "2.", "a."]),
        |_| {},
    );
}

#[test]
fn start_values_none_formats_and_edges() {
    check(&[l(7, 0), l(7, 0)], &some(&["E.", "F."]), |_| {});
    check(&[l(7, 0), l(7, 2)], &some(&["E.", "E..1"]), |_| {});
    check(&[l(1, clamp_level(-3))], &some(&["1."]), |_| {});
    // Levels past 8 clamp to 8, which list 1 does not define: no label.
    check(&[l(1, 200)], &[None], |_| {});
}

#[test]
fn levels_past_eight_clamp() {
    let n = numbering();
    let styles = styles();
    let mut c = ListCounter::new(&n, Some(&styles));
    assert_eq!(c.next(1, 200), None);
    assert_eq!(c.next(42, 0), None, "unknown numId");
    assert_eq!(c.next(0, 0), None);
    check(&[l(42, 0)], &[None], |_| {});
}

#[test]
fn many_items_and_number_formats() {
    let items: Vec<Item> = (0..12).map(|_| l(1, 0)).collect();
    let expected: Vec<String> = (1..=12).map(|i| format!("{i}.")).collect();
    let n = numbering();
    let got: Vec<String> = labels(&items, &n).into_iter().flatten().collect();
    assert_eq!(got, expected);
    let with = |format: &str, text: &str, custom: Option<&str>, count: usize| {
        let mut n = numbering();
        n.add_abstract(
            abs(vec![Level {
                custom_format: custom.map(str::to_owned),
                ..lvl(0, format, text)
            }]),
            Some(20),
        );
        let _ = n.add_instance(20, Vec::new(), Some(20));
        let items: Vec<Item> = (0..count).map(|_| l(20, 0)).collect();
        labels(&items, &n).into_iter().flatten().collect::<Vec<_>>()
    };
    assert_eq!(
        with("decimalZero", "%1.", None, 11),
        [
            "01.", "02.", "03.", "04.", "05.", "06.", "07.", "08.", "09.", "10.", "11."
        ]
    );
    assert_eq!(
        with("ordinalText", "%1 item", None, 3),
        ["First item", "Second item", "Third item"]
    );
    assert_eq!(
        with("custom", "%1", Some("001, 002, 003, ..."), 2),
        ["001", "002"]
    );
}

#[test]
fn built_in_list_galleries() {
    check(
        &[l(8, 0), l(8, 1), l(8, 2), l(8, 2), l(8, 0)],
        &some(&["1.", "a.", "i.", "ii.", "2."]),
        |n| assert_eq!(n.create_numbered_list().id, 8),
    );
    check(
        &[l(8, 0), l(8, 1), l(8, 2), l(8, 1)],
        &some(&["1.", "1.1.", "1.1.1.", "1.2."]),
        |n| assert_eq!(n.create_multilevel_list(true).id, 8),
    );
    check(
        &[l(8, 0), l(8, 1), l(8, 2)],
        &some(&["•", "o", "▪"]),
        |n| {
            assert_eq!(n.create_bullet_list().id, 8);
        },
    );
}

fn numbered(doc: &mut Document, num: u32, level: u8, text: &str) -> Result<Block, DocError> {
    let mut p = Paragraph::new(doc.alloc_id());
    p.props.numbering = Some(NumberingRef::new(num, level));
    p.content = Inlines::from_fragment(Fragment::text(text, RunProps::default()))?;
    Ok(Block::Paragraph(p))
}

#[test]
fn document_labels_walk_the_body_in_order_including_tables() -> Result<(), DocError> {
    let mut doc = Document::new();
    doc.styles = styles();
    doc.numbering = numbering();
    let one = numbered(&mut doc, 1, 0, "one")?;
    let two = numbered(&mut doc, 1, 0, "two")?;
    let two_one = numbered(&mut doc, 1, 1, "two.one")?;
    let three = numbered(&mut doc, 1, 0, "three")?;
    let cell = |doc: &mut Document, b: Block| Cell {
        id: doc.alloc_id(),
        props: TableCellProps::default(),
        grid_span: 1,
        v_merge: None,
        blocks: vec![b],
    };
    let c1 = cell(&mut doc, two);
    let c2 = cell(&mut doc, two_one);
    let row = Row {
        id: doc.alloc_id(),
        props: TableRowProps::default(),
        revision: None,
        cells: vec![c1, c2],
    };
    let table = Table {
        id: doc.alloc_id(),
        props: TableProps::default(),
        grid: vec![1000, 1000],
        rows: vec![row],
    };
    doc.sections[0].blocks = vec![one, Block::Table(table), three];
    doc.validate()?;
    let labels = doc.list_labels();
    let ordered: Vec<String> = doc
        .body_paragraphs()
        .iter()
        .filter_map(|p| labels.get(&p.id).map(|l| l.text.clone()))
        .collect();
    assert_eq!(ordered, ["1.", "2.", "2.1.", "3."]);
    Ok(())
}

#[test]
fn labels_carry_counters_suffix_alignment_and_run_props() {
    let n = numbering();
    let s = styles();
    let mut c = ListCounter::new(&n, Some(&s));
    let bullet = c.next(3, 0).unwrap_or_else(|| panic!("bullet label"));
    assert!(bullet.is_bullet);
    assert_eq!(bullet.raw_text, "\u{F0B7}");
    assert_eq!(
        bullet
            .rpr
            .as_ref()
            .and_then(|r| r.fonts.as_ref())
            .and_then(|f| f.ascii.as_deref()),
        Some("Symbol")
    );
    let numbered = c.next(1, 1).unwrap_or_else(|| panic!("numbered label"));
    assert_eq!(numbered.counters, vec![1, 1]);
    assert_eq!(numbered.suffix, LevelSuffix::Tab);
    assert_eq!(numbered.alignment, LevelAlignment::Left);
    assert_eq!((numbered.level, numbered.abstract_id), (1, 0));
}

#[test]
fn documents_without_lists_have_no_labels() {
    let doc = Document::new();
    assert!(doc.list_labels().is_empty());
}

#[test]
fn format_level_text_substitutes_and_forces_legal_decimal() {
    let levels = [
        lvl(0, "upperRoman", "%1"),
        lvl(1, "upperLetter", "%1.%2"),
        lvl(2, "decimal", "%1.%2.%3"),
    ];
    let refs: Vec<Option<&Level>> = levels.iter().map(Some).collect();
    assert_eq!(
        format_level_text(&refs, 2, &[Some(4), Some(2), Some(7)]),
        "IV.B.7"
    );
    let legal = [
        lvl(0, "upperRoman", "%1"),
        Level {
            is_legal: true,
            ..lvl(1, "lowerLetter", "%1.%2")
        },
    ];
    let refs: Vec<Option<&Level>> = legal.iter().map(Some).collect();
    assert_eq!(format_level_text(&refs, 1, &[Some(3), Some(2)]), "3.2");
    assert_eq!(format_level_text(&refs, 5, &[]), "");
    let none = [lvl(0, "none", "§")];
    let refs: Vec<Option<&Level>> = none.iter().map(Some).collect();
    assert_eq!(format_level_text(&refs, 0, &[Some(1)]), "§");
    let pct = [lvl(0, "decimal", "100% %1 %x %0")];
    let refs: Vec<Option<&Level>> = pct.iter().map(Some).collect();
    assert_eq!(format_level_text(&refs, 0, &[Some(2)]), "100% 2 %x %0");
}

#[test]
fn registry_round_trips_and_keeps_ids() {
    let n = numbering();
    let json = serde_json::to_string(&n).unwrap_or_default();
    let copy: Numbering = serde_json::from_str(&json).unwrap_or_default();
    assert_eq!(copy, n);
    assert_eq!(
        copy.level(1, 2, None).map(|l| l.text.as_str()),
        Some("%1.%2.%3.")
    );
}

#[test]
fn follows_num_style_link_through_a_list_style() {
    let mut n = Numbering::new();
    n.add_abstract(
        AbstractNum {
            style_link: Some("MyList".into()),
            ..abs(vec![lvl(0, "upperLetter", "%1)")])
        },
        Some(0),
    );
    let _ = n.add_instance(0, Vec::new(), Some(1));
    n.add_abstract(
        AbstractNum {
            num_style_link: Some("MyList".into()),
            ..abs(Vec::new())
        },
        Some(1),
    );
    let _ = n.add_instance(1, Vec::new(), Some(2));
    let mut list_style = Style::new("MyList", StyleType::Numbering, "My List");
    list_style.ppr = Some(ParagraphProps {
        numbering: Some(NumberingRef {
            num_id: Some(1),
            level: None,
        }),
        ..ParagraphProps::default()
    });
    let s = StyleSheet::new(DocDefaults::default(), [list_style]);
    assert_eq!(
        n.level(2, 0, Some(&s)).map(|l| l.format.clone()),
        Some(NumberFormat::UpperLetter)
    );
    assert_eq!(
        n.level(2, 0, None),
        None,
        "without styles the link cannot be followed"
    );
    // A link cycle stops.
    let mut cyc = n.clone();
    if let Some(a) = cyc.abstracts.get_mut(&0) {
        a.num_style_link = Some("MyList".into());
    }
    assert!(cyc.abstract_for(2, Some(&s)).is_some());
}

#[test]
fn rejects_instances_of_unknown_abstracts() {
    let mut n = Numbering::new();
    assert!(matches!(
        n.add_instance(3, Vec::new(), None),
        Err(DocError::UnknownList(m)) if m.contains("abstractNum")
    ));
    assert!(n.restart(9, 0, None).is_err());
    let added = n.add_abstract(abs(vec![lvl(0, "decimal", "%1")]), None).id;
    assert_eq!(added, 0);
    assert_eq!(
        n.add_instance(0, Vec::new(), None).map(|i| i.id).ok(),
        Some(1)
    );
    assert_eq!(n.levels(1, None).iter().flatten().count(), 1);
    assert_eq!(n.level_for_style(1, "Nope", None), None);
}
