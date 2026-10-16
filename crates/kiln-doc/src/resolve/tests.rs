// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ported from `reference/ts-core/doc-model/test/resolve.test.ts`: the inheritance matrix of
//! paragraph and run properties, Word's precedence between table style, paragraph style,
//! numbering and direct formatting, and table style regions.

use super::*;
use crate::model::{Block, Cell, Document, Paragraph, Row, Table};
use crate::numbering::{AbstractNum, Level, Numbering};
use crate::props::{
    Border, DocColor, Indent, NumberingRef, ParagraphBorders, RunFonts, Spacing, TabStop,
    TableRowProps,
};
use crate::st::{
    BorderStyle, NumberFormat, ShadingPattern, TabAlignment, ThemeFontRef, UnderlineStyle,
};
use crate::style::{DocDefaults, Style, StyleSheet, TableStyleCondition};
use crate::template::default_style_sheet;
use crate::{DocError, Fragment, Inlines};
use kiln_common::NodeId;

fn s(id: &str, kind: StyleType, based_on: Option<&str>) -> Style {
    Style {
        based_on: based_on.map(str::to_owned),
        ..Style::new(id, kind, id)
    }
}

fn default(mut st: Style) -> Style {
    st.is_default = true;
    st
}

fn rgb(v: u32) -> Option<DocColor> {
    Some(DocColor::rgb(v))
}

fn shading(v: u32) -> Option<Shading> {
    Some(Shading {
        pattern: ShadingPattern::Clear,
        color: None,
        fill: rgb(v),
    })
}

/// A compact fixture exercising every inheritance level.
fn fixture() -> (StyleSheet, Numbering) {
    let p = StyleType::Paragraph;
    let c = StyleType::Character;
    let styles = StyleSheet::new(
        DocDefaults {
            ppr: ParagraphProps {
                spacing: Some(Spacing {
                    after: Some(160),
                    line: Some(259),
                    line_rule: Some(LineRule::Auto),
                    ..Spacing::default()
                }),
                ..ParagraphProps::default()
            },
            rpr: RunProps {
                size: Some(22),
                fonts: Some(RunFonts {
                    ascii_theme: Some(ThemeFontRef::MinorHAnsi),
                    h_ansi_theme: Some(ThemeFontRef::MinorHAnsi),
                    ..RunFonts::default()
                }),
                ..RunProps::default()
            },
        },
        [
            default(s("Normal", p, None)),
            default(s("DefaultParagraphFont", c, None)),
            default(s("TableNormal", StyleType::Table, None)),
            Style {
                ppr: Some(ParagraphProps {
                    indent: Some(Indent {
                        left: Some(360),
                        ..Indent::default()
                    }),
                    tabs: Some(vec![
                        TabStop::new(720, TabAlignment::Left),
                        TabStop::new(1440, TabAlignment::Center),
                    ]),
                    ..ParagraphProps::default()
                }),
                rpr: Some(RunProps {
                    size: Some(24),
                    color: rgb(0x111111),
                    ..RunProps::default()
                }),
                ..s("Body", p, Some("Normal"))
            },
            Style {
                ppr: Some(ParagraphProps {
                    alignment: Some(Alignment::Center),
                    spacing: Some(Spacing {
                        before: Some(120),
                        ..Spacing::default()
                    }),
                    keep_next: Some(true),
                    ..ParagraphProps::default()
                }),
                rpr: Some(RunProps {
                    bold: Some(true),
                    ..RunProps::default()
                }),
                ..s("BodyBold", p, Some("Body"))
            },
            Style {
                rpr: Some(RunProps {
                    bold: Some(false),
                    ..RunProps::default()
                }),
                ..s("BodyBoldOff", p, Some("BodyBold"))
            },
            Style {
                ppr: Some(ParagraphProps {
                    indent: Some(Indent {
                        first_line: Some(300),
                        ..Indent::default()
                    }),
                    ..ParagraphProps::default()
                }),
                ..s("FirstLine", p, Some("Normal"))
            },
            Style {
                ppr: Some(ParagraphProps {
                    borders: Some(ParagraphBorders {
                        top: Some(Border::new(BorderStyle::Single, 4)),
                        bottom: Some(Border::new(BorderStyle::Single, 4)),
                        ..ParagraphBorders::default()
                    }),
                    ..ParagraphProps::default()
                }),
                ..s("Boxed", p, Some("Normal"))
            },
            Style {
                rpr: Some(RunProps {
                    italic: Some(true),
                    caps: Some(true),
                    ..RunProps::default()
                }),
                ..s("ItalicPara", p, Some("Normal"))
            },
            Style {
                ppr: Some(ParagraphProps {
                    numbering: Some(NumberingRef::new(1, 0)),
                    indent: Some(Indent {
                        left: Some(100),
                        ..Indent::default()
                    }),
                    ..ParagraphProps::default()
                }),
                ..s("ListNum", p, Some("Normal"))
            },
            Style {
                rpr: Some(RunProps {
                    italic: Some(true),
                    color: rgb(0x00FF00),
                    ..RunProps::default()
                }),
                ..s("Emph", c, Some("DefaultParagraphFont"))
            },
            Style {
                rpr: Some(RunProps {
                    bold: Some(true),
                    ..RunProps::default()
                }),
                ..s("StrongChar", c, Some("DefaultParagraphFont"))
            },
            Style {
                rpr: Some(RunProps {
                    size: Some(30),
                    ..RunProps::default()
                }),
                ..s("ChildChar", c, Some("StrongChar"))
            },
            Style {
                ppr: Some(ParagraphProps {
                    alignment: Some(Alignment::Right),
                    ..ParagraphProps::default()
                }),
                ..s("CycleA", p, Some("CycleB"))
            },
            Style {
                ppr: Some(ParagraphProps {
                    indent: Some(Indent {
                        left: Some(50),
                        ..Indent::default()
                    }),
                    ..ParagraphProps::default()
                }),
                ..s("CycleB", p, Some("CycleA"))
            },
            Style {
                ppr: Some(ParagraphProps {
                    spacing: Some(Spacing {
                        after: Some(0),
                        ..Spacing::default()
                    }),
                    alignment: Some(Alignment::Right),
                    ..ParagraphProps::default()
                }),
                rpr: Some(RunProps {
                    size: Some(18),
                    color: rgb(0xAAAAAA),
                    italic: Some(true),
                    ..RunProps::default()
                }),
                tc_pr: Some(TableCellProps {
                    v_align: Some(crate::st::CellVAlign::Center),
                    ..TableCellProps::default()
                }),
                conditions: [
                    (
                        TableCondition::FirstRow,
                        TableStyleCondition {
                            rpr: Some(RunProps {
                                bold: Some(true),
                                color: rgb(0xFFFFFF),
                                ..RunProps::default()
                            }),
                            tc_pr: Some(TableCellProps {
                                shading: shading(0x4472C4),
                                ..TableCellProps::default()
                            }),
                            ..TableStyleCondition::default()
                        },
                    ),
                    (
                        TableCondition::LastCol,
                        TableStyleCondition {
                            rpr: Some(RunProps {
                                size: Some(16),
                                ..RunProps::default()
                            }),
                            ..TableStyleCondition::default()
                        },
                    ),
                    (
                        TableCondition::Band1Horz,
                        TableStyleCondition {
                            tc_pr: Some(TableCellProps {
                                shading: shading(0xD9E2F3),
                                ..TableCellProps::default()
                            }),
                            ..TableStyleCondition::default()
                        },
                    ),
                    (
                        TableCondition::NwCell,
                        TableStyleCondition {
                            rpr: Some(RunProps {
                                underline: Some(Underline::new(UnderlineStyle::Double)),
                                ..RunProps::default()
                            }),
                            ..TableStyleCondition::default()
                        },
                    ),
                ]
                .into_iter()
                .collect(),
                ..s("TS", StyleType::Table, Some("TableNormal"))
            },
        ],
    );
    let mut numbering = Numbering::new();
    numbering.add_abstract(
        AbstractNum {
            levels: vec![
                Level {
                    ppr: Some(ParagraphProps {
                        indent: Some(Indent {
                            left: Some(720),
                            hanging: Some(360),
                            ..Indent::default()
                        }),
                        ..ParagraphProps::default()
                    }),
                    rpr: Some(RunProps {
                        bold: Some(true),
                        color: rgb(0xFF0000),
                        size: Some(28),
                        ..RunProps::default()
                    }),
                    ..Level::new(0, NumberFormat::Decimal, "%1.")
                },
                Level {
                    ppr: Some(ParagraphProps {
                        indent: Some(Indent {
                            left: Some(1440),
                            hanging: Some(360),
                            ..Indent::default()
                        }),
                        ..ParagraphProps::default()
                    }),
                    ..Level::new(1, NumberFormat::LowerLetter, "%2)")
                },
            ],
            ..AbstractNum::default()
        },
        Some(0),
    );
    let _ = numbering.add_instance(0, Vec::new(), Some(1));
    (styles, numbering)
}

fn whole() -> TableCellContext {
    TableCellContext {
        table_style_id: Some("TS".into()),
        conditions: vec![TableCondition::WholeTable],
    }
}

fn header_cell() -> TableCellContext {
    TableCellContext {
        table_style_id: Some("TS".into()),
        conditions: vec![
            TableCondition::WholeTable,
            TableCondition::FirstCol,
            TableCondition::FirstRow,
            TableCondition::NwCell,
        ],
    }
}

fn last_col_cell() -> TableCellContext {
    TableCellContext {
        table_style_id: Some("TS".into()),
        conditions: vec![
            TableCondition::WholeTable,
            TableCondition::Band1Horz,
            TableCondition::LastCol,
        ],
    }
}

fn pp(style: Option<&str>) -> ParagraphProps {
    ParagraphProps {
        style_id: style.map(str::to_owned),
        ..ParagraphProps::default()
    }
}

fn para(style: Option<&str>, f: impl FnOnce(&mut ParagraphProps)) -> ParagraphProps {
    let mut p = pp(style);
    f(&mut p);
    p
}

fn rp(style: Option<&str>, f: impl FnOnce(&mut RunProps)) -> RunProps {
    let mut r = RunProps {
        style_id: style.map(str::to_owned),
        ..RunProps::default()
    };
    f(&mut r);
    r
}

struct Env {
    styles: StyleSheet,
    numbering: Numbering,
    theme: DocTheme,
}

impl Env {
    fn new() -> Self {
        let (styles, numbering) = fixture();
        Self {
            styles,
            numbering,
            theme: DocTheme::office(),
        }
    }

    fn ctx(&self) -> DocContext<'_> {
        DocContext::new(&self.styles, &self.theme).with_numbering(&self.numbering)
    }

    fn para(&self, p: &ParagraphProps, t: Option<&TableCellContext>) -> ResolvedParagraph {
        resolve_paragraph(&self.ctx(), p, t)
    }

    fn run(&self, p: &ParagraphProps, r: &RunProps, t: Option<&TableCellContext>) -> ResolvedRun {
        resolve_text_run(&self.ctx(), p, r, t)
    }

    fn label(&self, p: &ParagraphProps, mark: &RunProps) -> ResolvedRun {
        resolve_run(
            &self.ctx(),
            &RunInput {
                paragraph: p,
                mark_props: mark,
                direct: &RunProps::default(),
                table: None,
                target: RunTarget::ListLabel,
            },
        )
    }
}

// ------------------------------------------------------------------ paragraph matrix

#[test]
fn normal_takes_doc_defaults_spacing() {
    let e = Env::new();
    let r = e.para(&pp(None), None);
    assert_eq!(
        (
            r.spacing.after,
            r.spacing.line,
            r.spacing.line_rule,
            r.spacing.before
        ),
        (160, 259, LineRule::Auto, 0)
    );
    assert_eq!(
        (r.alignment, r.indent.left, r.indent.first_line),
        (Alignment::Left, 0, 0)
    );
}

#[test]
fn style_chain_indent_alignment_and_spacing() {
    let e = Env::new();
    assert_eq!(e.para(&pp(Some("Body")), None).indent.left, 360);
    let bb = e.para(&pp(Some("BodyBold")), None);
    assert_eq!(
        bb.indent.left, 360,
        "basedOn chain inherits the parent indent"
    );
    assert_eq!(bb.alignment, Alignment::Center);
    assert_eq!(
        (bb.spacing.before, bb.spacing.after),
        (120, 160),
        "spacing merges per field"
    );
    assert!(bb.keep_next);
    let direct = para(Some("BodyBold"), |p| p.alignment = Some(Alignment::Right));
    assert_eq!(e.para(&direct, None).alignment, Alignment::Right);
}

#[test]
fn indent_exclusivity_across_levels() {
    let e = Env::new();
    let fl = para(Some("Body"), |p| {
        p.indent = Some(Indent {
            first_line: Some(200),
            ..Indent::default()
        });
    });
    let r = e.para(&fl, None).indent;
    assert_eq!((r.left, r.first_line), (360, 200));
    let hang = para(Some("FirstLine"), |p| {
        p.indent = Some(Indent {
            hanging: Some(100),
            ..Indent::default()
        });
    });
    let r = e.para(&hang, None).indent;
    assert_eq!((r.first_line, r.hanging), (0, 100));
}

#[test]
fn direct_spacing_after_zero_overrides_doc_defaults() {
    let e = Env::new();
    let p = para(None, |p| {
        p.spacing = Some(Spacing {
            after: Some(0),
            ..Spacing::default()
        });
    });
    let r = e.para(&p, None);
    assert_eq!((r.spacing.after, r.spacing.line), (0, 259));
}

#[test]
fn unknown_style_falls_back_to_normal() {
    let e = Env::new();
    assert_eq!(e.para(&pp(Some("Nope")), None).style_id, "Normal");
}

#[test]
fn numbering_levels_and_direct_indents() {
    let e = Env::new();
    let listed = para(Some("Body"), |p| {
        p.numbering = Some(NumberingRef::new(1, 0))
    });
    let r = e.para(&listed, None).indent;
    assert_eq!(
        (r.left, r.hanging),
        (720, 360),
        "the level indent beats the style indent"
    );
    let direct = para(Some("Body"), |p| {
        p.numbering = Some(NumberingRef::new(1, 0));
        p.indent = Some(Indent {
            left: Some(1000),
            ..Indent::default()
        });
    });
    let r = e.para(&direct, None).indent;
    assert_eq!(
        (r.left, r.hanging),
        (1000, 360),
        "direct indent beats numbering"
    );
}

#[test]
fn style_linked_numbering_and_its_cancellation() {
    let e = Env::new();
    let r = e.para(&pp(Some("ListNum")), None);
    assert_eq!(
        r.numbering,
        Some(ListRef {
            num_id: 1,
            level: 0
        })
    );
    assert_eq!(r.indent.left, 720);
    let cancelled = para(Some("ListNum"), |p| {
        p.numbering = Some(NumberingRef::new(0, 0))
    });
    let r = e.para(&cancelled, None);
    assert_eq!((r.numbering, r.indent.left), (None, 100));
    let level_only = para(Some("ListNum"), |p| {
        p.numbering = Some(NumberingRef {
            num_id: None,
            level: Some(1),
        });
    });
    let r = e.para(&level_only, None);
    assert_eq!(
        (r.numbering, r.indent.left),
        (
            Some(ListRef {
                num_id: 1,
                level: 1
            }),
            1440
        )
    );
}

#[test]
fn table_style_paragraph_layer() {
    let e = Env::new();
    let r = e.para(&pp(None), Some(&whole()));
    assert_eq!((r.alignment, r.spacing.after), (Alignment::Right, 0));
    assert_eq!(
        e.para(&pp(Some("BodyBold")), Some(&whole())).alignment,
        Alignment::Center
    );
    assert_eq!(e.para(&pp(Some("Body")), Some(&whole())).spacing.after, 0);
    let direct = para(None, |p| p.alignment = Some(Alignment::Left));
    assert_eq!(e.para(&direct, Some(&whole())).alignment, Alignment::Left);
}

#[test]
fn tabs_accumulate_and_clear_borders_merge_per_edge() {
    let e = Env::new();
    let p = para(Some("Body"), |p| {
        p.tabs = Some(vec![
            TabStop::new(1440, TabAlignment::Clear),
            TabStop::new(2000, TabAlignment::Right),
        ]);
    });
    let tabs: Vec<_> = e
        .para(&p, None)
        .tabs
        .iter()
        .map(|t| (t.pos, t.align))
        .collect();
    assert_eq!(
        tabs,
        vec![(720, TabAlignment::Left), (2000, TabAlignment::Right)]
    );
    let boxed = para(Some("Boxed"), |p| {
        p.borders = Some(ParagraphBorders {
            bottom: Some(Border::new(BorderStyle::Double, 6)),
            ..ParagraphBorders::default()
        });
    });
    let b = e.para(&boxed, None).borders;
    assert_eq!(
        (b.top.map(|x| x.style), b.bottom.map(|x| x.style)),
        (Some(BorderStyle::Single), Some(BorderStyle::Double))
    );
}

#[test]
fn outline_level_defaults_and_cycles_terminate() {
    let e = Env::new();
    assert_eq!(e.para(&pp(None), None).outline_level, 9);
    assert_eq!(
        e.para(&pp(Some("CycleA")), None).alignment,
        Alignment::Right
    );
    assert!(e.para(&pp(None), None).widow_control);
}

// ------------------------------------------------------------------ run matrix

#[test]
fn run_defaults_and_paragraph_styles() {
    let e = Env::new();
    let none = RunProps::default();
    let r = e.run(&pp(None), &none, None);
    assert_eq!(r.size, 22);
    assert_eq!((r.bold, r.italic, r.caps), (false, false, false));
    assert_eq!(r.color, ColorValue::Auto, "auto colour without settings");
    let body = e.run(&pp(Some("Body")), &none, None);
    assert_eq!(
        (body.size, body.color.to_string()),
        (24, "111111".to_owned())
    );
    assert!(e.run(&pp(Some("BodyBold")), &none, None).bold);
    assert!(!e.run(&pp(Some("BodyBoldOff")), &none, None).bold);
}

#[test]
fn character_styles_toggle_against_the_paragraph() {
    let e = Env::new();
    let strong = rp(Some("StrongChar"), |_| {});
    assert!(
        !e.run(&pp(Some("BodyBold")), &strong, None).bold,
        "bold over bold toggles off"
    );
    assert!(e.run(&pp(Some("Body")), &strong, None).bold);
    assert!(
        !e.run(&pp(Some("ItalicPara")), &rp(Some("Emph"), |_| {}), None)
            .italic,
        "emphasis inside an italic paragraph is upright"
    );
    let off = rp(None, |r| r.bold = Some(false));
    assert!(!e.run(&pp(Some("BodyBold")), &off, None).bold);
    let abs = rp(Some("StrongChar"), |r| r.bold = Some(true));
    assert!(
        e.run(&pp(Some("BodyBold")), &abs, None).bold,
        "direct bold is absolute"
    );
}

#[test]
fn character_style_colours_chains_and_misuse() {
    let e = Env::new();
    let r = e.run(&pp(Some("Body")), &rp(Some("Emph"), |_| {}), None);
    assert_eq!((r.italic, r.color.to_string()), (true, "00FF00".to_owned()));
    let direct = rp(Some("Emph"), |r| r.color = rgb(0x123456));
    assert_eq!(
        e.run(&pp(Some("Body")), &direct, None).color.to_string(),
        "123456"
    );
    let child = e.run(&pp(None), &rp(Some("ChildChar"), |_| {}), None);
    assert_eq!((child.bold, child.size), (true, 30));
    assert_eq!(e.run(&pp(None), &rp(Some("Body"), |_| {}), None).size, 22);
    let big = rp(Some("ChildChar"), |r| r.size = Some(40));
    assert_eq!(e.run(&pp(Some("Body")), &big, Some(&whole())).size, 40);
    let caps = rp(None, |r| r.caps = Some(false));
    assert!(!e.run(&pp(Some("ItalicPara")), &caps, None).caps);
}

#[test]
fn table_style_run_layers_and_regions() {
    let e = Env::new();
    let none = RunProps::default();
    let r = e.run(&pp(None), &none, Some(&whole()));
    assert_eq!(
        (r.size, r.color.to_string(), r.italic),
        (18, "AAAAAA".to_owned(), true)
    );
    let r = e.run(&pp(Some("Body")), &none, Some(&whole()));
    assert_eq!(
        (r.size, r.color.to_string(), r.italic),
        (24, "111111".to_owned(), true)
    );
    let r = e.run(&pp(None), &none, Some(&header_cell()));
    assert_eq!((r.bold, r.color.to_string()), (true, "FFFFFF".to_owned()));
    assert_eq!(r.underline.map(|u| u.style), Some(UnderlineStyle::Double));
    assert_eq!(e.run(&pp(None), &none, Some(&last_col_cell())).size, 16);
    assert!(
        e.run(&pp(Some("BodyBold")), &none, Some(&header_cell()))
            .bold
    );
}

#[test]
fn list_labels_take_the_level_run_props() {
    let e = Env::new();
    let listed = para(Some("Body"), |p| {
        p.numbering = Some(NumberingRef::new(1, 0))
    });
    let l = e.label(&listed, &RunProps::default());
    assert_eq!(
        (l.bold, l.color.to_string(), l.size),
        (true, "FF0000".to_owned(), 28)
    );
    let text = e.run(&listed, &RunProps::default(), None);
    assert_eq!(
        (text.bold, text.size),
        (false, 24),
        "text ignores the numbering rPr"
    );
    let level1 = para(Some("Body"), |p| {
        p.numbering = Some(NumberingRef::new(1, 1))
    });
    let l = e.label(&level1, &RunProps::default());
    assert_eq!((l.bold, l.size), (false, 24));
    assert_eq!(l.style_id, None);
}

#[test]
fn theme_fonts_and_explicit_fonts() {
    let e = Env::new();
    assert_eq!(
        e.run(&pp(None), &RunProps::default(), None).fonts.ascii,
        "Calibri"
    );
    let arial = rp(None, |r| r.fonts = Some(RunFonts::latin("Arial")));
    let f = e.run(&pp(None), &arial, None).fonts;
    assert_eq!((f.ascii.as_str(), f.h_ansi.as_str()), ("Arial", "Arial"));
}

// ------------------------------------------------------------------ precedence

/// A list paragraph in a styled paragraph inside a styled table: each layer sets a property a
/// later layer may override; removing the later layer reveals the earlier one.
fn precedence_case(numbering_rpr: Option<RunProps>, para_mark: Option<RunProps>) -> ResolvedRun {
    let styles = StyleSheet::new(
        DocDefaults {
            ppr: ParagraphProps::default(),
            rpr: RunProps {
                size: Some(20),
                ..RunProps::default()
            },
        },
        [
            default(s("Normal", StyleType::Paragraph, None)),
            default(s("TableNormal", StyleType::Table, None)),
            Style {
                rpr: Some(RunProps {
                    size: Some(18),
                    color: rgb(0xAAAAAA),
                    italic: Some(true),
                    caps: Some(true),
                    ..RunProps::default()
                }),
                ..s("Grid", StyleType::Table, Some("TableNormal"))
            },
            Style {
                rpr: Some(RunProps {
                    size: Some(24),
                    color: rgb(0x111111),
                    ..RunProps::default()
                }),
                ..s("Para", StyleType::Paragraph, Some("Normal"))
            },
        ],
    );
    let mut numbering = Numbering::new();
    numbering.add_abstract(
        AbstractNum {
            levels: vec![Level {
                rpr: numbering_rpr,
                ..Level::new(0, NumberFormat::Decimal, "%1.")
            }],
            ..AbstractNum::default()
        },
        Some(0),
    );
    let _ = numbering.add_instance(0, Vec::new(), Some(1));
    let theme = DocTheme::office();
    let ctx = DocContext::new(&styles, &theme).with_numbering(&numbering);
    let table = Table {
        id: NodeId(1),
        props: TableProps {
            style_id: Some("Grid".into()),
            ..TableProps::default()
        },
        grid: vec![2000],
        rows: vec![Row {
            id: NodeId(2),
            props: TableRowProps::default(),
            revision: None,
            cells: vec![Cell {
                id: NodeId(3),
                props: TableCellProps::default(),
                grid_span: 1,
                v_merge: None,
                blocks: vec![Block::Paragraph(Paragraph::new(NodeId(4)))],
            }],
        }],
    };
    let table_ctx = table_cell_context(&table, 0, 0);
    let p = para(Some("Para"), |p| {
        p.numbering = Some(NumberingRef::new(1, 0))
    });
    resolve_run(
        &ctx,
        &RunInput {
            paragraph: &p,
            mark_props: &para_mark.unwrap_or_default(),
            direct: &RunProps::default(),
            table: Some(&table_ctx),
            target: RunTarget::ListLabel,
        },
    )
}

#[test]
fn each_property_comes_from_the_highest_layer_that_sets_it() {
    let r = precedence_case(
        Some(RunProps {
            size: Some(28),
            bold: Some(true),
            ..RunProps::default()
        }),
        Some(RunProps {
            color: rgb(0x0000FF),
            ..RunProps::default()
        }),
    );
    assert!(r.caps, "table style only");
    assert!(r.italic, "table style only");
    assert_eq!(
        r.size, 28,
        "numbering beats the paragraph style and the table style"
    );
    assert!(r.bold, "numbering");
    assert_eq!(r.color.to_string(), "0000FF", "direct beats both styles");
}

#[test]
fn removing_a_layer_reveals_the_one_below() {
    let size28 = Some(RunProps {
        size: Some(28),
        ..RunProps::default()
    });
    assert_eq!(
        precedence_case(size28.clone(), None).color.to_string(),
        "111111"
    );
    assert_eq!(precedence_case(None, None).size, 24);
    let mark = Some(RunProps {
        size: Some(32),
        ..RunProps::default()
    });
    assert_eq!(
        precedence_case(size28, mark).size,
        32,
        "direct beats numbering"
    );
}

// ------------------------------------------------------------------ tables

fn pos(row: usize, col: usize, rows: usize, cols: usize) -> CellPosition {
    CellPosition {
        row,
        col,
        row_count: rows,
        col_count: cols,
        look: TableLook::default(),
        row_band_size: 1,
        col_band_size: 1,
        header_rows: 1,
    }
}

#[test]
fn table_conditions_follow_word_order_and_band_after_the_header() {
    use TableCondition as C;
    assert_eq!(
        table_conditions(&pos(0, 0, 4, 3)),
        vec![C::WholeTable, C::FirstCol, C::FirstRow, C::NwCell]
    );
    assert_eq!(
        table_conditions(&pos(1, 1, 4, 3)),
        vec![C::WholeTable, C::Band1Horz]
    );
    assert_eq!(
        table_conditions(&pos(2, 1, 4, 3)),
        vec![C::WholeTable, C::Band2Horz]
    );
}

#[test]
fn table_conditions_respect_look_and_band_sizes() {
    use TableCondition as C;
    let look = TableLook {
        first_row: false,
        first_column: false,
        last_row: true,
        last_column: true,
        no_h_band: false,
        no_v_band: false,
    };
    let p = CellPosition {
        look,
        ..pos(3, 2, 4, 3)
    };
    assert_eq!(
        table_conditions(&p),
        vec![C::WholeTable, C::LastCol, C::LastRow, C::SeCell]
    );
    let p = CellPosition {
        look,
        row_band_size: 2,
        ..pos(2, 0, 6, 3)
    };
    assert_eq!(
        table_conditions(&p),
        vec![C::WholeTable, C::Band1Vert, C::Band2Horz]
    );
    let p = CellPosition {
        look: TableLook {
            last_row: true,
            last_column: true,
            ..TableLook::default()
        },
        ..pos(3, 0, 4, 3)
    };
    assert!(table_conditions(&p).contains(&C::SwCell));
    let p = CellPosition {
        look: TableLook {
            last_column: true,
            ..TableLook::default()
        },
        ..pos(0, 2, 4, 3)
    };
    assert!(table_conditions(&p).contains(&C::NeCell));
}

fn table_doc() -> Result<(Document, NodeId, Vec<NodeId>), DocError> {
    let mut doc = Document::new();
    let e = Env::new();
    doc.styles = e.styles;
    doc.numbering = e.numbering;
    let mut paras = Vec::new();
    let mut rows = Vec::new();
    for r in 0..2 {
        let mut cells = Vec::new();
        for c in 0..2 {
            let mut p = Paragraph::new(doc.alloc_id());
            p.content =
                Inlines::from_fragment(Fragment::text(&format!("{r}{c}"), RunProps::default()))?;
            paras.push(p.id);
            cells.push(Cell {
                id: doc.alloc_id(),
                props: TableCellProps::default(),
                grid_span: 1,
                v_merge: None,
                blocks: vec![Block::Paragraph(p)],
            });
        }
        rows.push(Row {
            id: doc.alloc_id(),
            props: TableRowProps::default(),
            revision: None,
            cells,
        });
    }
    let table = Table {
        id: doc.alloc_id(),
        props: TableProps {
            style_id: Some("TS".into()),
            ..TableProps::default()
        },
        grid: vec![2000, 2000],
        rows,
    };
    let id = table.id;
    doc.sections[0].blocks.push(Block::Table(table));
    doc.validate()?;
    Ok((doc, id, paras))
}

#[test]
fn cell_properties_come_from_the_table_style_and_the_cell() -> Result<(), DocError> {
    let (doc, table, _) = table_doc()?;
    let t = doc.table(table).ok_or(DocError::UnknownNode(table))?;
    let ctx = doc.context();
    let header = resolve_cell(&ctx, t, 0, 1);
    let fill = |c: &TableCellProps| {
        c.shading
            .as_ref()
            .and_then(|s| s.fill.as_ref())
            .map(|f| f.val.to_string())
    };
    assert_eq!(fill(&header).as_deref(), Some("4472C4"));
    assert_eq!(header.v_align, Some(crate::st::CellVAlign::Center));
    assert_eq!(
        fill(&resolve_cell(&ctx, t, 1, 1)).as_deref(),
        Some("D9E2F3")
    );
    Ok(())
}

#[test]
fn table_props_resolve_through_the_style_chain() {
    let styles = default_style_sheet();
    let theme = DocTheme::office();
    let ctx = DocContext::new(&styles, &theme);
    let tp = resolve_table(
        &ctx,
        &TableProps {
            style_id: Some("TableGrid".into()),
            alignment: Some(crate::st::TableAlignment::Center),
            ..TableProps::default()
        },
    );
    assert_eq!(
        tp.borders
            .as_ref()
            .and_then(|b| b.inside_h.as_ref())
            .map(|b| b.style.clone()),
        Some(BorderStyle::Single)
    );
    assert_eq!(
        tp.cell_margins,
        Some(crate::props::CellMargins {
            top: Some(0),
            left: Some(108),
            bottom: Some(0),
            right: Some(108),
        })
    );
    assert_eq!(tp.alignment, Some(crate::st::TableAlignment::Center));
    assert_eq!(tp.style_id.as_deref(), Some("TableGrid"));
}

#[test]
fn document_resolution_in_and_out_of_tables() -> Result<(), DocError> {
    let (mut doc, _, paras) = table_doc()?;
    doc.styles = default_style_sheet();
    // Heading 1 in the body; a Table Grid table.
    let body_id = doc.sections[0].blocks[0].id();
    if let Some(Block::Paragraph(p)) = doc.sections[0].blocks.first_mut() {
        p.props.style_id = Some("Heading1".into());
        p.content = Inlines::from_fragment(Fragment::text("Title", RunProps::default()))?;
    }
    if let Some(Block::Table(t)) = doc.sections[0].blocks.get_mut(1) {
        t.props.style_id = Some("TableGrid".into());
    }
    let heading = doc
        .paragraph(body_id)
        .ok_or(DocError::UnknownNode(body_id))?;
    let run = doc.resolve_run_at(heading, 1);
    assert_eq!(run.size, 32);
    assert_eq!(run.color.to_string(), "2F5496");
    assert_eq!(run.fonts.ascii, "Calibri Light");
    let cell_para = doc
        .paragraph(paras[0])
        .ok_or(DocError::UnknownNode(paras[0]))?;
    let r = doc.resolve_paragraph(cell_para);
    assert_eq!(r.spacing.after, 0, "Table Grid pPr beats docDefaults' 8 pt");
    assert_eq!(r.spacing.line, 240);
    assert!(doc.table_context(paras[3]).is_some());
    assert!(doc.table_context(body_id).is_none());
    Ok(())
}

#[test]
fn direct_run_formatting_from_runs() -> Result<(), DocError> {
    let mut doc = Document::new();
    let id = doc.sections[0].blocks[0].id();
    if let Some(Block::Paragraph(p)) = doc.sections[0].blocks.first_mut() {
        p.content = Inlines::from_fragment(Fragment::text(
            "Big",
            RunProps {
                size: Some(48),
                bold: Some(true),
                ..RunProps::default()
            },
        ))?;
    }
    let p = doc.paragraph(id).ok_or(DocError::UnknownNode(id))?;
    let r = doc.resolve_run_at(p, 1);
    assert_eq!((r.size, r.bold), (48, true));
    Ok(())
}

#[test]
fn colour_scheme_mapping_from_settings() {
    let styles = default_style_sheet();
    let theme = DocTheme::office();
    let mut settings = crate::settings::Settings::default();
    settings.color_scheme_mapping.t1 = crate::st::ThemeColorName::Light1;
    let ctx = DocContext {
        styles: &styles,
        numbering: None,
        theme: &theme,
        settings: Some(&settings),
    };
    let direct = RunProps {
        color: Some(DocColor::theme(0x000000, crate::st::ThemeColorName::Text1)),
        ..RunProps::default()
    };
    let r = resolve_text_run(&ctx, &ParagraphProps::default(), &direct, None);
    assert_eq!(r.color.to_string(), "FFFFFF");
}
