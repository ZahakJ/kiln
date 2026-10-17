// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Write editor's commands and queries against the model: every ribbon group, the
//! shortcuts' handlers, the clipboard, Find and Replace, headers and footers, and the session's
//! host round trips (paste from the system clipboard, Insert ▸ Pictures).

use kiln_common::NodeId;
use kiln_doc::st::{Alignment, HighlightColor, UnderlineStyle, VertAlign};
use kiln_doc::{Block, InlineObject, Mark};
use kiln_protocol::commands::{
    BorderPreset, CellVerticalAlign, ClipboardData, EditCommand, FindQuery, FontFormat, GoToTarget,
    ListNumberStyle, MarginsPreset, PageNumberPosition, PageOrientation, PageSetupSpec, PaperSize,
    ParagraphFormat, SectionStart, StoryKind, TabAlign, TabLeaderKind, TabStopSpec, TableAutofit,
    TableDeleteTarget, TableSelectTarget, TextAlign, TextCase, TextSeparator, UnderlineKind,
    WriteQuery, WriteQueryResult,
};
use kiln_protocol::geom::Point;
use kiln_protocol::{CaretMotion, Command, SelectUnit, SuiteCommand, TextPos, WriteCommand};

use crate::apps::write::WriteDoc;
use crate::model::{Change, DocModel};

// ----- helpers -------------------------------------------------------------------------------

fn doc() -> WriteDoc {
    WriteDoc::new(kiln_doc::Document::new(), Vec::new(), None)
}

fn fixture(name: &str) -> WriteDoc {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/docx/features")
        .join(name);
    let bytes = std::fs::read(path).expect("fixture");
    let read = kiln_io_docx::read_docx(&bytes).expect("reads");
    WriteDoc::new(read.document, read.warnings, Some(name.to_owned()))
}

fn ed(e: EditCommand) -> Command {
    Command::Suite(SuiteCommand::Edit(e))
}

fn wr(w: WriteCommand) -> Command {
    Command::Write(w)
}

fn run(d: &mut WriteDoc, c: Command) -> Change {
    let id = c.id();
    d.execute(&c).unwrap_or_else(|e| panic!("{id} failed: {e}"))
}

fn fails(d: &mut WriteDoc, c: Command) -> String {
    d.execute(&c)
        .expect_err("the command must fail")
        .to_string()
}

fn typ(d: &mut WriteDoc, text: &str) {
    run(
        d,
        wr(WriteCommand::InsertText {
            text: text.to_owned(),
        }),
    );
}

fn ids(d: &WriteDoc) -> Vec<NodeId> {
    d.body().iter().map(|p| p.id).collect()
}

fn texts(d: &WriteDoc) -> Vec<String> {
    d.body()
        .iter()
        .map(|p| p.content.plain_text(false))
        .collect()
}

fn pos(d: &WriteDoc, para: usize, offset: u32) -> TextPos {
    TextPos {
        block: ids(d)[para],
        offset,
    }
}

fn caret(d: &mut WriteDoc, para: usize, offset: u32) {
    let p = pos(d, para, offset);
    run(
        d,
        wr(WriteCommand::SelectTo {
            pos: p,
            extend: false,
        }),
    );
}

fn select(d: &mut WriteDoc, a: (usize, u32), b: (usize, u32)) {
    caret(d, a.0, a.1);
    let p = pos(d, b.0, b.1);
    run(
        d,
        wr(WriteCommand::SelectTo {
            pos: p,
            extend: true,
        }),
    );
}

fn resolved(d: &WriteDoc, para: usize, offset: usize) -> kiln_doc::ResolvedRun {
    let id = ids(d)[para];
    let p = d.doc.paragraph(id).expect("paragraph");
    d.doc.resolve_run_at(p, offset)
}

fn para_props(d: &WriteDoc, para: usize) -> kiln_doc::ResolvedParagraph {
    let id = ids(d)[para];
    d.doc
        .resolve_paragraph(d.doc.paragraph(id).expect("paragraph"))
}

fn query(d: &WriteDoc, q: WriteQuery) -> WriteQueryResult {
    d.write_query(&q).expect("query")
}

fn format(d: &WriteDoc) -> kiln_protocol::commands::FormatState {
    match query(d, WriteQuery::Format) {
        WriteQueryResult::Format(f) => *f,
        other => panic!("{other:?}"),
    }
}

fn three_paragraphs() -> WriteDoc {
    let mut d = doc();
    typ(&mut d, "The kiln glows\nWarm clay waits\nFire and form");
    d
}

fn find(text: &str) -> FindQuery {
    FindQuery {
        text: text.to_owned(),
        ..FindQuery::default()
    }
}

fn first_table(d: &WriteDoc) -> kiln_doc::Table {
    d.doc.sections[0]
        .blocks
        .iter()
        .find_map(|b| match b {
            Block::Table(t) => Some(t.clone()),
            _ => None,
        })
        .expect("a table")
}

fn table_doc(rows: u32, cols: u32) -> WriteDoc {
    let mut d = doc();
    typ(&mut d, "Before");
    run(
        &mut d,
        wr(WriteCommand::InsertTable {
            rows,
            cols,
            autofit: TableAutofit::Fixed,
        }),
    );
    d
}

fn cell_text(t: &kiln_doc::Table, r: usize, c: usize) -> String {
    t.rows[r].cells[c]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Paragraph(p) => Some(p.content.plain_text(false)),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("|")
}

// ----- typing, deleting, caret ------------------------------------------------------------

#[test]
fn caret_typing_deleting_by_grapheme_and_word() {
    let mut d = doc();
    typ(&mut d, "e\u{301}t\u{e9} kiln fired");
    run(&mut d, wr(WriteCommand::DeleteBackward));
    assert_eq!(texts(&d), vec!["e\u{301}t\u{e9} kiln fire"]);
    run(&mut d, wr(WriteCommand::DeleteWordBackward));
    assert_eq!(texts(&d), vec!["e\u{301}t\u{e9} kiln "]);
    run(
        &mut d,
        wr(WriteCommand::MoveCaret {
            motion: CaretMotion::DocumentStart,
            extend: false,
        }),
    );
    run(&mut d, wr(WriteCommand::DeleteForward));
    assert_eq!(
        texts(&d),
        vec!["t\u{e9} kiln "],
        "e + combining acute is one grapheme"
    );
    run(&mut d, wr(WriteCommand::DeleteWordForward));
    assert_eq!(texts(&d), vec!["kiln "]);
}

#[test]
fn caret_motion_by_word_paragraph_and_document_with_shift() {
    let mut d = three_paragraphs();
    let mv = |d: &mut WriteDoc, motion, extend| {
        run(d, wr(WriteCommand::MoveCaret { motion, extend }));
    };
    mv(&mut d, CaretMotion::DocumentStart, false);
    mv(&mut d, CaretMotion::WordForward, false);
    assert_eq!(d.head.offset, 4);
    mv(&mut d, CaretMotion::WordForward, true);
    assert_eq!((d.anchor.offset, d.head.offset), (4, 9));
    mv(&mut d, CaretMotion::ParagraphForward, false);
    assert_eq!(d.head.para, ids(&d)[1]);
    mv(&mut d, CaretMotion::DocumentEnd, true);
    assert_eq!(d.head, kiln_doc::Pos::new(ids(&d)[2], 13));
    run(&mut d, wr(WriteCommand::SelectAll));
    assert_eq!(d.span().0, kiln_doc::Pos::new(ids(&d)[0], 0));
    // Line motion follows the layout.
    d.view_synced();
    mv(&mut d, CaretMotion::DocumentStart, false);
    mv(&mut d, CaretMotion::LineEnd, false);
    assert_eq!(d.head.offset, 14);
    mv(&mut d, CaretMotion::LineDown, false);
    assert_eq!(d.head.para, ids(&d)[1]);
}

#[test]
fn caret_clicks_select_words_and_paragraphs() {
    let mut d = three_paragraphs();
    d.view_synced();
    let c = d
        .view()
        .and_then(|v| v.caret(kiln_doc::Pos::new(ids(&d)[0], 5)))
        .expect("caret");
    let point = Point::new(c.rect.x + 1.0, c.rect.y + c.rect.h / 2.0);
    for (unit, expect) in [
        (SelectUnit::Char, (5, 5)),
        (SelectUnit::Word, (4, 8)),
        (SelectUnit::Paragraph, (0, 14)),
    ] {
        run(
            &mut d,
            wr(WriteCommand::SelectAt {
                point,
                extend: false,
                unit,
            }),
        );
        let (s, e) = d.span();
        assert_eq!((s.offset, e.offset), expect, "{unit:?}");
    }
}

#[test]
fn enter_after_a_heading_starts_the_next_style_and_ends_empty_list_items() {
    let mut d = doc();
    run(&mut d, wr(WriteCommand::StyleHeading1));
    typ(&mut d, "Chapter");
    typ(&mut d, "\n");
    assert_eq!(para_props(&d, 1).style_id, "Normal");
    typ(&mut d, "item");
    run(&mut d, ed(EditCommand::Bullets { glyph: None }));
    typ(&mut d, "\n");
    assert!(para_props(&d, 2).numbering.is_some(), "a new item");
    typ(&mut d, "\n");
    assert_eq!(texts(&d).len(), 3, "Enter on the empty item ends the list");
    assert!(para_props(&d, 2).numbering.is_none());
}

#[test]
fn ime_composition_commits_once_and_cancels_cleanly() {
    let mut d = doc();
    typ(&mut d, "A ");
    run(
        &mut d,
        wr(WriteCommand::SetComposition {
            text: "か".into(),
            caret: 1,
        }),
    );
    run(
        &mut d,
        wr(WriteCommand::SetComposition {
            text: "かん".into(),
            caret: 2,
        }),
    );
    assert_eq!(texts(&d), vec!["A かん"]);
    run(
        &mut d,
        wr(WriteCommand::CommitComposition { text: "漢".into() }),
    );
    assert_eq!(texts(&d), vec!["A 漢"]);
    run(
        &mut d,
        wr(WriteCommand::SetComposition {
            text: "x".into(),
            caret: 1,
        }),
    );
    run(&mut d, wr(WriteCommand::CancelComposition));
    assert_eq!(texts(&d), vec!["A 漢"]);
}

#[test]
fn undo_groups_typing_by_word_and_formatting_as_steps() {
    let mut d = doc();
    for c in "kiln fire".chars() {
        typ(&mut d, &c.to_string());
    }
    select(&mut d, (0, 0), (0, 4));
    run(&mut d, ed(EditCommand::Bold));
    assert!(d.undo());
    assert!(!resolved(&d, 0, 0).bold);
    assert!(d.undo());
    assert_eq!(texts(&d), vec!["kiln "]);
    assert!(d.redo() && d.redo());
    assert!(resolved(&d, 0, 0).bold);
}

// ----- character formatting --------------------------------------------------------------

#[test]
fn character_format_bold_toggles_off_only_when_all_is_bold() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 4), (0, 8));
    run(&mut d, ed(EditCommand::Bold));
    assert!(resolved(&d, 0, 5).bold && !resolved(&d, 0, 2).bold);
    select(&mut d, (0, 0), (0, 8));
    run(&mut d, ed(EditCommand::Bold));
    assert!(
        resolved(&d, 0, 0).bold,
        "a partly bold selection becomes bold"
    );
    run(&mut d, ed(EditCommand::Bold));
    assert!(!resolved(&d, 0, 5).bold);
    run(&mut d, ed(EditCommand::Italic));
    assert!(resolved(&d, 0, 3).italic);
}

#[test]
fn character_format_on_a_caret_inside_a_word_formats_the_word() {
    let mut d = three_paragraphs();
    caret(&mut d, 0, 6);
    run(&mut d, ed(EditCommand::Italic));
    assert!(resolved(&d, 0, 4).italic && resolved(&d, 0, 7).italic);
    assert!(!resolved(&d, 0, 9).italic);
    assert_eq!(d.span().0, d.span().1, "the caret stays a caret");
}

#[test]
fn character_format_pending_at_the_caret_applies_to_typed_text() {
    let mut d = doc();
    typ(&mut d, "plain ");
    run(&mut d, ed(EditCommand::Bold));
    assert!(format(&d).bold, "the ribbon shows the pending bold");
    typ(&mut d, "bold");
    run(&mut d, ed(EditCommand::Bold));
    typ(&mut d, " plain");
    assert!(!resolved(&d, 0, 2).bold && resolved(&d, 0, 7).bold && !resolved(&d, 0, 12).bold);
    // Moving the caret forgets pending formatting.
    run(&mut d, ed(EditCommand::Underline));
    run(
        &mut d,
        wr(WriteCommand::MoveCaret {
            motion: CaretMotion::DocumentStart,
            extend: false,
        }),
    );
    typ(&mut d, "x");
    assert!(resolved(&d, 0, 0).underline.is_none());
}

#[test]
fn character_format_underline_strike_scripts_and_caps() {
    let mut d = three_paragraphs();
    select(&mut d, (1, 0), (1, 4));
    run(&mut d, ed(EditCommand::Underline));
    assert_eq!(
        resolved(&d, 1, 1).underline.map(|u| u.style),
        Some(UnderlineStyle::Single)
    );
    run(&mut d, ed(EditCommand::DoubleUnderline));
    assert_eq!(
        resolved(&d, 1, 1).underline.map(|u| u.style),
        Some(UnderlineStyle::Double)
    );
    run(&mut d, ed(EditCommand::DoubleUnderline));
    assert!(resolved(&d, 1, 1).underline.is_none());
    run(
        &mut d,
        wr(WriteCommand::UnderlineStyle {
            style: Some(UnderlineKind::Wave),
        }),
    );
    assert_eq!(
        resolved(&d, 1, 1).underline.map(|u| u.style),
        Some(UnderlineStyle::Wave)
    );
    run(&mut d, ed(EditCommand::Strikethrough));
    assert!(resolved(&d, 1, 0).strike);
    run(&mut d, ed(EditCommand::Superscript));
    assert_eq!(resolved(&d, 1, 0).vert_align, VertAlign::Superscript);
    run(&mut d, ed(EditCommand::Subscript));
    assert_eq!(resolved(&d, 1, 0).vert_align, VertAlign::Subscript);
    run(&mut d, ed(EditCommand::Subscript));
    assert_eq!(resolved(&d, 1, 0).vert_align, VertAlign::Baseline);
    run(&mut d, ed(EditCommand::AllCaps));
    run(&mut d, wr(WriteCommand::SmallCaps));
    let r = resolved(&d, 1, 0);
    assert!(r.caps && r.small_caps);
}

#[test]
fn character_format_fonts_sizes_grow_and_shrink() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 14));
    run(
        &mut d,
        ed(EditCommand::FontFamily {
            family: "Fraunces".into(),
        }),
    );
    assert_eq!(resolved(&d, 0, 0).fonts.ascii, "Fraunces");
    run(&mut d, ed(EditCommand::FontSize { size: 10.5 }));
    assert_eq!(resolved(&d, 0, 0).size, 21);
    run(&mut d, ed(EditCommand::GrowFont { one_point: false }));
    assert_eq!(resolved(&d, 0, 0).size, 22, "10.5 → 11");
    run(&mut d, ed(EditCommand::GrowFont { one_point: true }));
    assert_eq!(resolved(&d, 0, 0).size, 24, "11 → 12 by a point");
    run(&mut d, ed(EditCommand::ShrinkFont { one_point: false }));
    assert_eq!(resolved(&d, 0, 0).size, 22);
    assert!(fails(&mut d, ed(EditCommand::FontSize { size: 2000.0 })).contains("1–1638"));
    assert!(fails(&mut d, ed(EditCommand::FontFamily { family: " ".into() })).contains("font"));
}

#[test]
fn character_format_change_case_and_its_cycle() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 14));
    run(
        &mut d,
        ed(EditCommand::ChangeCase {
            case: Some(TextCase::Upper),
        }),
    );
    assert_eq!(texts(&d)[0], "THE KILN GLOWS");
    run(&mut d, ed(EditCommand::ChangeCase { case: None }));
    assert_eq!(texts(&d)[0], "the kiln glows", "UPPER → lower");
    run(&mut d, ed(EditCommand::ChangeCase { case: None }));
    assert_eq!(
        texts(&d)[0],
        "The Kiln Glows",
        "lower → Capitalize Each Word"
    );
    run(
        &mut d,
        ed(EditCommand::ChangeCase {
            case: Some(TextCase::Toggle),
        }),
    );
    assert_eq!(texts(&d)[0], "tHE kILN gLOWS");
}

#[test]
fn character_format_colour_highlight_and_clearing() {
    let mut d = three_paragraphs();
    select(&mut d, (2, 0), (2, 4));
    run(
        &mut d,
        ed(EditCommand::FontColor {
            color: Some("C8352B".into()),
        }),
    );
    assert_eq!(format(&d).color.as_deref(), Some("C8352B"));
    run(
        &mut d,
        ed(EditCommand::Highlight {
            color: Some("#FFE066".into()),
        }),
    );
    assert_eq!(resolved(&d, 2, 0).highlight, Some(HighlightColor::Yellow));
    run(&mut d, ed(EditCommand::Bold));
    run(&mut d, ed(EditCommand::ClearFormatting));
    let r = resolved(&d, 2, 0);
    assert!(!r.bold && r.color.rgb().is_none());
    assert_eq!(
        r.highlight,
        Some(HighlightColor::Yellow),
        "Word keeps highlighting"
    );
    run(&mut d, ed(EditCommand::Highlight { color: None }));
    assert!(resolved(&d, 2, 0).highlight.is_none());
    run(&mut d, ed(EditCommand::FontColor { color: None }));
    assert!(
        fails(
            &mut d,
            ed(EditCommand::FontColor {
                color: Some("red".into()),
            })
        )
        .contains("RRGGBB")
    );
}

#[test]
fn character_format_clear_character_formatting_keeps_character_styles() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 3));
    run(
        &mut d,
        wr(WriteCommand::ApplyStyle {
            style_id: "Strong".into(),
        }),
    );
    run(&mut d, ed(EditCommand::Italic));
    run(&mut d, wr(WriteCommand::ClearCharacterFormatting));
    let r = resolved(&d, 0, 1);
    assert!(r.bold, "Strong stays");
    assert!(!r.italic);
}

#[test]
fn character_format_font_dialog_sets_effects_and_spacing() {
    let mut d = three_paragraphs();
    select(&mut d, (1, 0), (1, 4));
    run(
        &mut d,
        wr(WriteCommand::SetFont {
            format: Box::new(FontFormat {
                family: Some("Figtree".into()),
                size: Some(14.0),
                bold: Some(true),
                small_caps: Some(true),
                hidden: Some(false),
                spacing: Some(1.5),
                scale: Some(90),
                color: Some("auto".into()),
                ..FontFormat::default()
            }),
        }),
    );
    let r = resolved(&d, 1, 0);
    assert_eq!(
        (r.size, r.bold, r.small_caps, r.spacing, r.scale),
        (28, true, true, 30, 90)
    );
    assert!(
        fails(
            &mut d,
            wr(WriteCommand::SetFont {
                format: Box::new(FontFormat {
                    scale: Some(900),
                    ..FontFormat::default()
                }),
            })
        )
        .contains("scale")
    );
}

// ----- paragraph formatting --------------------------------------------------------------

#[test]
fn paragraph_alignment_toggles_back_to_left() {
    let mut d = three_paragraphs();
    run(&mut d, ed(EditCommand::AlignCenter));
    assert_eq!(para_props(&d, 2).alignment, Alignment::Center);
    run(&mut d, ed(EditCommand::AlignCenter));
    assert_eq!(para_props(&d, 2).alignment, Alignment::Left);
    run(&mut d, ed(EditCommand::Justify));
    assert_eq!(para_props(&d, 2).alignment, Alignment::Both);
    run(&mut d, ed(EditCommand::AlignRight));
    assert_eq!(format(&d).alignment, Some(TextAlign::Right));
    run(&mut d, ed(EditCommand::AlignLeft));
    assert_eq!(para_props(&d, 2).alignment, Alignment::Left);
}

#[test]
fn paragraph_line_spacing_indents_and_spacing() {
    let mut d = three_paragraphs();
    run(&mut d, ed(EditCommand::LineSpacing { spacing: 1.5 }));
    assert_eq!(para_props(&d, 2).spacing.line, 360);
    assert!(
        d.execute(&ed(EditCommand::LineSpacing { spacing: 0.0 }))
            .is_err()
    );
    run(&mut d, ed(EditCommand::Indent));
    run(&mut d, ed(EditCommand::Indent));
    assert_eq!(para_props(&d, 2).indent.left, 1440);
    run(&mut d, ed(EditCommand::Outdent));
    assert_eq!(para_props(&d, 2).indent.left, 720);
    run(&mut d, ed(EditCommand::Outdent));
    run(&mut d, ed(EditCommand::Outdent));
    assert_eq!(para_props(&d, 2).indent.left, 0, "never below zero");
    run(&mut d, wr(WriteCommand::IndentLeft { cm: 2.54 }));
    run(&mut d, wr(WriteCommand::IndentRight { cm: 1.27 }));
    run(&mut d, wr(WriteCommand::SpacingBefore { pt: 12.0 }));
    run(&mut d, wr(WriteCommand::SpacingAfter { pt: 6.0 }));
    let p = para_props(&d, 2);
    assert_eq!(
        (
            p.indent.left,
            p.indent.right,
            p.spacing.before,
            p.spacing.after
        ),
        (1440, 720, 240, 120)
    );
    let f = format(&d);
    assert_eq!((f.indent_left, f.space_before), (72.0, 12.0));
}

#[test]
fn paragraph_dialog_and_tabs_dialog() {
    let mut d = three_paragraphs();
    run(
        &mut d,
        wr(WriteCommand::SetParagraph {
            format: Box::new(ParagraphFormat {
                first_line: Some(-18.0),
                indent_left: Some(36.0),
                line_exact: Some(14.0),
                keep_next: Some(true),
                page_break_before: Some(true),
                outline_level: Some(2),
                ..ParagraphFormat::default()
            }),
        }),
    );
    let p = para_props(&d, 2);
    assert_eq!(
        (p.indent.hanging, p.indent.left, p.spacing.line),
        (360, 720, 280)
    );
    assert!(p.keep_next && p.page_break_before);
    assert_eq!(p.outline_level, 1);
    run(
        &mut d,
        wr(WriteCommand::SetTabs {
            tabs: vec![
                TabStopSpec {
                    pos: 216.0,
                    align: TabAlign::Decimal,
                    leader: TabLeaderKind::Dot,
                },
                TabStopSpec {
                    pos: 72.0,
                    align: TabAlign::Center,
                    leader: TabLeaderKind::None,
                },
            ],
        }),
    );
    let tabs = format(&d).tabs;
    assert_eq!(tabs.len(), 2);
    assert_eq!((tabs[0].pos, tabs[0].align), (72.0, TabAlign::Center));
    assert_eq!(tabs[1].leader, TabLeaderKind::Dot);
}

#[test]
fn paragraph_shading_borders_and_clearing() {
    let mut d = three_paragraphs();
    run(
        &mut d,
        wr(WriteCommand::ParagraphShading {
            color: Some("FFF2CC".into()),
        }),
    );
    run(
        &mut d,
        wr(WriteCommand::ParagraphBorders {
            preset: BorderPreset::Bottom,
        }),
    );
    let f = format(&d);
    assert_eq!(f.shading.as_deref(), Some("FFF2CC"));
    assert!(f.borders);
    run(&mut d, wr(WriteCommand::StyleHeading2));
    run(&mut d, wr(WriteCommand::ClearParagraphFormatting));
    let f = format(&d);
    assert!(f.shading.is_none() && !f.borders);
    assert_eq!(f.style_id, "Heading2", "the style stays");
}

#[test]
fn paragraph_move_up_down_and_promote_demote() {
    let mut d = three_paragraphs();
    caret(&mut d, 2, 0);
    run(&mut d, wr(WriteCommand::MoveParagraphUp));
    assert_eq!(
        texts(&d),
        vec!["The kiln glows", "Fire and form", "Warm clay waits"]
    );
    run(&mut d, wr(WriteCommand::MoveParagraphUp));
    run(&mut d, wr(WriteCommand::MoveParagraphUp));
    assert_eq!(
        texts(&d)[0],
        "Fire and form",
        "the first paragraph stays first"
    );
    run(&mut d, wr(WriteCommand::MoveParagraphDown));
    assert_eq!(texts(&d)[1], "Fire and form");
    run(&mut d, wr(WriteCommand::StyleHeading2));
    run(&mut d, wr(WriteCommand::Promote));
    assert_eq!(format(&d).style_id, "Heading1");
    run(&mut d, wr(WriteCommand::Demote));
    run(&mut d, wr(WriteCommand::Demote));
    assert_eq!(format(&d).style_id, "Heading3");
}

// ----- styles ------------------------------------------------------------------------------

#[test]
fn styles_shortcuts_and_builtin_styles_missing_from_the_document() {
    let mut d = fixture("paragraphs.docx");
    caret(&mut d, 0, 0);
    for (cmd, id) in [
        (WriteCommand::StyleHeading1, "Heading1"),
        (WriteCommand::StyleHeading2, "Heading2"),
        (WriteCommand::StyleHeading3, "Heading3"),
        (WriteCommand::StyleTitle, "Title"),
        (WriteCommand::StyleNormal, "Normal"),
    ] {
        run(&mut d, wr(cmd));
        assert_eq!(format(&d).style_id, id);
    }
    run(
        &mut d,
        wr(WriteCommand::ApplyStyle {
            style_id: "IntenseQuote".into(),
        }),
    );
    assert_eq!(format(&d).style_name, "Intense Quote");
    assert!(
        fails(
            &mut d,
            wr(WriteCommand::ApplyStyle {
                style_id: "NoSuchStyle".into(),
            })
        )
        .contains("no style")
    );
}

#[test]
fn styles_update_from_selection_new_style_and_modify() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 14));
    run(&mut d, wr(WriteCommand::StyleHeading1));
    run(&mut d, ed(EditCommand::Italic));
    run(
        &mut d,
        wr(WriteCommand::UpdateStyleFromSelection { style_id: None }),
    );
    let h1 = d.doc.styles.get("Heading1").cloned().expect("Heading1");
    assert_eq!(h1.rpr.and_then(|r| r.italic), Some(true));
    caret(&mut d, 1, 0);
    run(&mut d, ed(EditCommand::AlignCenter));
    run(
        &mut d,
        wr(WriteCommand::NewStyleFromSelection {
            name: "Pull Quote".into(),
        }),
    );
    assert_eq!(format(&d).style_name, "Pull Quote");
    assert!(
        fails(
            &mut d,
            wr(WriteCommand::NewStyleFromSelection {
                name: "Pull Quote".into(),
            })
        )
        .contains("exists")
    );
    run(
        &mut d,
        wr(WriteCommand::ModifyStyle {
            style_id: "PullQuote".into(),
            spec: Box::new(kiln_protocol::commands::StyleSpec {
                font: FontFormat {
                    size: Some(20.0),
                    ..FontFormat::default()
                },
                ..kiln_protocol::commands::StyleSpec::default()
            }),
        }),
    );
    assert_eq!(format(&d).size, 20.0);
    assert!(
        fails(
            &mut d,
            wr(WriteCommand::ModifyStyle {
                style_id: "Normal".into(),
                spec: Box::new(kiln_protocol::commands::StyleSpec {
                    based_on: Some("Heading1".into()),
                    ..kiln_protocol::commands::StyleSpec::default()
                }),
            })
        )
        .contains("based on")
    );
}

// ----- lists --------------------------------------------------------------------------------

#[test]
fn lists_bullets_toggle_and_continue_the_list_above() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (1, 2));
    run(&mut d, ed(EditCommand::Bullets { glyph: None }));
    let a = para_props(&d, 0).numbering.expect("bullet");
    assert_eq!(para_props(&d, 1).numbering, Some(a));
    assert!(format(&d).list.is_some_and(|l| l.bullet));
    caret(&mut d, 2, 0);
    run(&mut d, ed(EditCommand::Bullets { glyph: None }));
    assert_eq!(
        para_props(&d, 2).numbering.map(|l| l.num_id),
        Some(a.num_id),
        "continues"
    );
    run(&mut d, ed(EditCommand::Bullets { glyph: None }));
    assert!(para_props(&d, 2).numbering.is_none(), "toggles off");
    run(
        &mut d,
        ed(EditCommand::Bullets {
            glyph: Some("✓".into()),
        }),
    );
    assert_eq!(format(&d).list.map(|l| l.label), Some("✓".into()));
}

#[test]
fn lists_numbering_multilevel_levels_and_tab() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (2, 0));
    run(
        &mut d,
        ed(EditCommand::Numbering {
            style: Some(ListNumberStyle::UpperRoman),
        }),
    );
    let n = para_props(&d, 1).numbering.expect("numbered");
    let lvl = d.doc.numbering.level(n.num_id, 0, None).expect("level");
    assert_eq!(lvl.format, kiln_doc::st::NumberFormat::UpperRoman);
    caret(&mut d, 1, 0);
    run(&mut d, wr(WriteCommand::Tab { back: false }));
    assert_eq!(para_props(&d, 1).numbering.map(|l| l.level), Some(1));
    run(&mut d, wr(WriteCommand::Tab { back: true }));
    assert_eq!(para_props(&d, 1).numbering.map(|l| l.level), Some(0));
    run(&mut d, wr(WriteCommand::ListLevel { level: 4 }));
    assert_eq!(para_props(&d, 1).numbering.map(|l| l.level), Some(4));
    assert!(
        d.execute(&wr(WriteCommand::ListLevel { level: 9 }))
            .is_err()
    );
    run(&mut d, ed(EditCommand::MultilevelList { legal: true }));
    let m = para_props(&d, 1).numbering.expect("multilevel");
    assert!(
        d.doc
            .numbering
            .level(m.num_id, 1, None)
            .is_some_and(|l| l.is_legal)
    );
}

#[test]
fn lists_restart_continue_and_set_value() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (2, 0));
    run(&mut d, ed(EditCommand::Numbering { style: None }));
    let first = para_props(&d, 0).numbering.expect("list").num_id;
    caret(&mut d, 1, 0);
    run(&mut d, wr(WriteCommand::RestartNumbering));
    let restarted = para_props(&d, 1).numbering.expect("list").num_id;
    assert_ne!(restarted, first);
    assert_eq!(
        para_props(&d, 2).numbering.map(|l| l.num_id),
        Some(restarted)
    );
    run(&mut d, wr(WriteCommand::ContinueNumbering));
    assert_eq!(para_props(&d, 1).numbering.map(|l| l.num_id), Some(first));
    run(&mut d, wr(WriteCommand::SetNumberingValue { value: 7 }));
    let labels = d.doc.list_labels();
    assert_eq!(
        labels.get(&ids(&d)[1]).map(|l| l.text.clone()),
        Some("7.".into())
    );
    assert!(
        d.execute(&wr(WriteCommand::SetNumberingValue { value: 0 }))
            .is_err()
    );
}

#[test]
fn lists_indent_shortcut_changes_the_level_of_list_items() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (1, 0));
    run(&mut d, ed(EditCommand::Bullets { glyph: None }));
    run(&mut d, ed(EditCommand::Indent));
    assert_eq!(para_props(&d, 1).numbering.map(|l| l.level), Some(1));
    run(&mut d, ed(EditCommand::Outdent));
    assert_eq!(para_props(&d, 1).numbering.map(|l| l.level), Some(0));
}

// ----- tables -------------------------------------------------------------------------------

#[test]
fn tables_insert_from_the_grid_picker() {
    let d = table_doc(3, 4);
    let t = first_table(&d);
    assert_eq!((t.rows.len(), t.grid.len()), (3, 4));
    assert_eq!(t.props.style_id.as_deref(), Some("TableGrid"));
    let f = format(&d);
    let ts = f.table.expect("in the table");
    assert_eq!((ts.rows, ts.cols, ts.row, ts.col), (3, 4, 0, 0));
    let mut e = doc();
    assert!(
        e.execute(&wr(WriteCommand::InsertTable {
            rows: 0,
            cols: 2,
            autofit: TableAutofit::Fixed,
        }))
        .is_err()
    );
    run(
        &mut e,
        wr(WriteCommand::InsertTable {
            rows: 1,
            cols: 2,
            autofit: TableAutofit::Window,
        }),
    );
    assert_eq!(
        first_table(&e).props.width,
        Some(kiln_doc::TableWidth::pct(5000))
    );
}

#[test]
fn tables_tab_moves_through_cells_and_adds_a_row_at_the_end() {
    let mut d = table_doc(2, 2);
    typ(&mut d, "a");
    for text in ["b", "c", "d"] {
        run(&mut d, wr(WriteCommand::Tab { back: false }));
        typ(&mut d, text);
    }
    let t = first_table(&d);
    assert_eq!(
        (
            cell_text(&t, 0, 1),
            cell_text(&t, 1, 0),
            cell_text(&t, 1, 1)
        ),
        ("b".into(), "c".into(), "d".into())
    );
    run(&mut d, wr(WriteCommand::Tab { back: false }));
    assert_eq!(first_table(&d).rows.len(), 3, "a new row");
    run(&mut d, wr(WriteCommand::Tab { back: true }));
    let (s, e) = d.span();
    assert_eq!(
        (s.offset, e.offset),
        (0, 1),
        "Shift+Tab selects the previous cell's text"
    );
}

#[test]
fn tables_rows_columns_and_delete() {
    let mut d = table_doc(2, 2);
    run(&mut d, ed(EditCommand::TableInsertRowAbove));
    run(&mut d, ed(EditCommand::TableInsertRowBelow));
    run(&mut d, ed(EditCommand::TableInsertColumnLeft));
    run(&mut d, ed(EditCommand::TableInsertColumnRight));
    let t = first_table(&d);
    assert_eq!((t.rows.len(), t.grid.len()), (4, 4));
    run(
        &mut d,
        ed(EditCommand::TableDelete {
            what: TableDeleteTarget::Rows,
        }),
    );
    run(
        &mut d,
        ed(EditCommand::TableDelete {
            what: TableDeleteTarget::Columns,
        }),
    );
    let t = first_table(&d);
    assert_eq!((t.rows.len(), t.grid.len()), (3, 3));
    run(
        &mut d,
        ed(EditCommand::TableDelete {
            what: TableDeleteTarget::Table,
        }),
    );
    assert!(
        d.doc.sections[0]
            .blocks
            .iter()
            .all(|b| !matches!(b, Block::Table(_)))
    );
    assert!(
        d.execute(&ed(EditCommand::TableMergeCells)).is_err(),
        "not in a table"
    );
}

#[test]
fn tables_merge_and_split_cells() {
    let mut d = table_doc(2, 3);
    typ(&mut d, "x");
    run(&mut d, wr(WriteCommand::Tab { back: false }));
    typ(&mut d, "y");
    // Select from the first cell to the second.
    let t = first_table(&d);
    let p = |r: usize, c: usize| match &t.rows[r].cells[c].blocks[0] {
        Block::Paragraph(p) => p.id,
        _ => panic!("cell"),
    };
    run(
        &mut d,
        wr(WriteCommand::SelectTo {
            pos: TextPos {
                block: p(0, 0),
                offset: 0,
            },
            extend: false,
        }),
    );
    run(
        &mut d,
        wr(WriteCommand::SelectTo {
            pos: TextPos {
                block: p(1, 1),
                offset: 0,
            },
            extend: true,
        }),
    );
    run(&mut d, ed(EditCommand::TableMergeCells));
    let t2 = first_table(&d);
    assert_eq!(t2.rows[0].cells.len(), 2);
    assert_eq!(t2.rows[0].cells[0].grid_span, 2);
    assert_eq!(
        t2.rows[1].cells[0].v_merge,
        Some(kiln_doc::st::VMerge::Continue)
    );
    assert!(cell_text(&t2, 0, 0).contains('x') && cell_text(&t2, 0, 0).contains('y'));
    // Split the last cell of the first row into 3 columns.
    let last = match &t2.rows[0].cells[1].blocks[0] {
        Block::Paragraph(p) => p.id,
        _ => panic!("cell"),
    };
    run(
        &mut d,
        wr(WriteCommand::SelectTo {
            pos: TextPos {
                block: last,
                offset: 0,
            },
            extend: false,
        }),
    );
    run(
        &mut d,
        ed(EditCommand::TableSplitCells { rows: 1, cols: 3 }),
    );
    let t3 = first_table(&d);
    assert_eq!(t3.rows[0].cells.len(), 4);
    assert_eq!(t3.grid.len(), 5, "two new grid columns");
    let width = |r: &kiln_doc::Row| r.cells.iter().map(|c| c.grid_span).sum::<u32>();
    assert_eq!(
        width(&t3.rows[0]),
        width(&t3.rows[1]),
        "rows stay rectangular"
    );
    run(
        &mut d,
        ed(EditCommand::TableSplitCells { rows: 2, cols: 1 }),
    );
    assert_eq!(first_table(&d).rows.len(), 3);
    assert!(
        d.execute(&ed(EditCommand::TableSplitCells { rows: 0, cols: 1 }))
            .is_err()
    );
}

#[test]
fn tables_distribute_align_header_rows_and_cell_size() {
    let mut d = table_doc(2, 3);
    run(
        &mut d,
        wr(WriteCommand::TableCellSize {
            width: Some(200.0),
            height: Some(30.0),
        }),
    );
    let t = first_table(&d);
    assert_eq!(t.grid[0], 4000);
    assert_eq!(t.rows[0].props.height.map(|h| h.value), Some(600));
    run(&mut d, ed(EditCommand::TableDistributeColumns));
    let t = first_table(&d);
    assert!(t.grid.windows(2).all(|w| w[0] == w[1]), "{:?}", t.grid);
    run(&mut d, ed(EditCommand::TableDistributeRows));
    run(
        &mut d,
        ed(EditCommand::TableCellAlignment {
            vertical: CellVerticalAlign::Center,
            horizontal: TextAlign::Right,
        }),
    );
    let t = first_table(&d);
    assert_eq!(
        t.rows[0].cells[0].props.v_align,
        Some(kiln_doc::st::CellVAlign::Center)
    );
    assert_eq!(format(&d).alignment, Some(TextAlign::Right));
    run(&mut d, ed(EditCommand::TableRepeatHeaderRows));
    assert_eq!(first_table(&d).rows[0].props.header, Some(true));
    assert!(format(&d).table.is_some_and(|t| t.repeat_header));
    run(&mut d, ed(EditCommand::TableRepeatHeaderRows));
    assert_eq!(first_table(&d).rows[0].props.header, None);
    run(
        &mut d,
        wr(WriteCommand::TableAutofit {
            mode: TableAutofit::Contents,
        }),
    );
    run(
        &mut d,
        wr(WriteCommand::TableAlign {
            align: TextAlign::Center,
        }),
    );
    assert_eq!(
        first_table(&d).props.alignment,
        Some(kiln_doc::st::TableAlignment::Center)
    );
}

#[test]
fn tables_design_look_styles_shading_and_borders() {
    let mut d = table_doc(3, 3);
    run(&mut d, ed(EditCommand::TableBandedColumns));
    run(&mut d, ed(EditCommand::TableTotalRow));
    let ts = format(&d).table.expect("table");
    assert!(ts.banded_columns && ts.total_row && ts.header_row);
    for style in ["Accent", "Banded", "Plain", "Grid"] {
        run(
            &mut d,
            ed(EditCommand::TableStyle {
                style: style.into(),
            }),
        );
    }
    assert_eq!(first_table(&d).props.style_id.as_deref(), Some("TableGrid"));
    assert!(d.doc.styles.contains("GridTable4-Accent1"));
    assert!(
        d.execute(&ed(EditCommand::TableStyle {
            style: "Heading1".into()
        }))
        .is_err()
    );
    run(
        &mut d,
        ed(EditCommand::TableShading {
            color: Some("DCE6FB".into()),
        }),
    );
    run(
        &mut d,
        ed(EditCommand::TableBorders {
            preset: BorderPreset::Outside,
        }),
    );
    let c = &first_table(&d).rows[0].cells[0];
    assert!(c.props.shading.is_some());
    assert!(
        c.props
            .borders
            .as_ref()
            .is_some_and(|b| b.top.is_some() && b.left.is_some())
    );
}

#[test]
fn tables_convert_split_and_select() {
    let mut d = doc();
    typ(&mut d, "a\tb\tc\nd\te\tf");
    select(&mut d, (0, 0), (1, 5));
    run(
        &mut d,
        wr(WriteCommand::ConvertTextToTable {
            separator: TextSeparator::Tabs,
        }),
    );
    let t = first_table(&d);
    assert_eq!((t.rows.len(), t.grid.len()), (2, 3));
    assert_eq!(cell_text(&t, 1, 2), "f");
    run(
        &mut d,
        wr(WriteCommand::TableSelect {
            what: TableSelectTarget::Row,
        }),
    );
    assert_eq!(
        crate::model::DocModel::selected_text(&d).as_deref(),
        Some("a\nb\nc")
    );
    run(
        &mut d,
        ed(EditCommand::TableConvertToText {
            separator: TextSeparator::Commas,
        }),
    );
    assert_eq!(texts(&d)[..2], ["a,b,c".to_owned(), "d,e,f".to_owned()]);
    let mut d = table_doc(3, 2);
    caret_in_row(&mut d, 2);
    run(&mut d, wr(WriteCommand::TableSplit));
    let tables = d.doc.sections[0]
        .blocks
        .iter()
        .filter(|b| matches!(b, Block::Table(_)))
        .count();
    assert_eq!(tables, 2);
}

fn caret_in_row(d: &mut WriteDoc, row: usize) {
    let t = first_table(d);
    let id = match &t.rows[row].cells[0].blocks[0] {
        Block::Paragraph(p) => p.id,
        _ => panic!("cell"),
    };
    run(
        d,
        wr(WriteCommand::SelectTo {
            pos: TextPos {
                block: id,
                offset: 0,
            },
            extend: false,
        }),
    );
}

// ----- pages, sections, headers and footers ----------------------------------------------

#[test]
fn page_breaks_column_breaks_and_blank_pages() {
    use kiln_doc::st::BreakKind;
    let mut d = three_paragraphs();
    let before = texts(&d);
    caret(&mut d, 0, 3);
    run(&mut d, wr(WriteCommand::PageBreak));
    // Word: the break ends its paragraph; the rest starts the next page with the caret.
    assert_eq!(texts(&d).len(), before.len() + 1);
    assert_eq!(d.head, kiln_doc::Pos::new(ids(&d)[1], 0));
    let page = {
        let head = d.head;
        d.view_synced().caret(head).map(|c| c.page)
    };
    assert_eq!(page, Some(1), "the caret is at the top of the new page");
    // Go To the page after two breaks at the end of the document lands on that page.
    run(
        &mut d,
        wr(WriteCommand::MoveCaret {
            motion: kiln_protocol::CaretMotion::DocumentEnd,
            extend: false,
        }),
    );
    run(&mut d, wr(WriteCommand::PageBreak));
    run(&mut d, wr(WriteCommand::PageBreak));
    let pages = d.view_synced().layout.pages().len();
    run(
        &mut d,
        wr(WriteCommand::MoveCaret {
            motion: kiln_protocol::CaretMotion::DocumentStart,
            extend: false,
        }),
    );
    run(
        &mut d,
        wr(WriteCommand::GoTo {
            target: kiln_protocol::commands::GoToTarget::Page {
                number: i32::try_from(pages).unwrap(),
                relative: false,
            },
        }),
    );
    let head = d.head;
    assert_eq!(
        d.view_synced().caret(head).map(|c| c.page + 1),
        Some(pages),
        "Go To the last page"
    );
    assert!(d.undo());
    assert!(d.undo());
    caret(&mut d, 1, 0);
    run(&mut d, wr(WriteCommand::ColumnBreak));
    run(&mut d, wr(WriteCommand::LineBreak));
    let kinds_of = |d: &WriteDoc, i: usize| -> Vec<BreakKind> {
        d.doc
            .paragraph(ids(d)[i])
            .expect("paragraph")
            .content
            .objects()
            .iter()
            .filter_map(|o| match o {
                InlineObject::Break(b) => Some(b.kind),
                _ => None,
            })
            .collect()
    };
    assert_eq!(kinds_of(&d, 0), vec![BreakKind::Page]);
    assert_eq!(kinds_of(&d, 1), vec![BreakKind::Column]);
    assert_eq!(
        kinds_of(&d, 2),
        vec![BreakKind::TextWrapping],
        "a line break stays in its paragraph"
    );
    // One undo step each.
    assert!(d.undo());
    assert!(d.undo());
    assert!(d.undo());
    assert_eq!(texts(&d), before);
    run(&mut d, wr(WriteCommand::BlankPage));
    d.view_synced();
    assert!(d.view().is_some_and(|v| v.layout.pages().len() >= 3));
}

#[test]
fn page_sections_margins_orientation_size_and_columns() {
    let mut d = three_paragraphs();
    caret(&mut d, 1, 4);
    run(
        &mut d,
        wr(WriteCommand::SectionBreak {
            kind: SectionStart::Continuous,
        }),
    );
    assert_eq!(d.doc.sections.len(), 2);
    run(
        &mut d,
        ed(EditCommand::Margins {
            preset: MarginsPreset::Narrow,
        }),
    );
    run(
        &mut d,
        ed(EditCommand::Orientation {
            orientation: PageOrientation::Landscape,
        }),
    );
    run(
        &mut d,
        ed(EditCommand::PageSize {
            size: PaperSize::A4,
        }),
    );
    run(&mut d, wr(WriteCommand::Columns { count: 2 }));
    let s = &d.doc.sections[1].props;
    assert_eq!(s.margins.left, 720);
    assert_eq!(
        (s.page_size.width, s.page_size.height),
        (16838, 11906),
        "A4 landscape"
    );
    assert_eq!(s.columns.count, 2);
    assert_eq!(
        d.doc.sections[0].props.columns.count, 1,
        "only the caret's section"
    );
    let f = format(&d);
    assert!(f.section.landscape && f.section.paper == "A4" && f.section.columns == 2);
    run(
        &mut d,
        ed(EditCommand::Margins {
            preset: MarginsPreset::Mirrored,
        }),
    );
    assert!(d.doc.settings.mirror_margins);
}

#[test]
fn page_setup_dialog_validates_and_applies() {
    let mut d = three_paragraphs();
    run(
        &mut d,
        wr(WriteCommand::PageSetup {
            setup: Box::new(PageSetupSpec {
                top: Some(54.0),
                left: Some(90.0),
                gutter: Some(18.0),
                width: Some(500.0),
                height: Some(700.0),
                vertical_align: Some(CellVerticalAlign::Center),
                ..PageSetupSpec::default()
            }),
        }),
    );
    match query(&d, WriteQuery::PageSetup) {
        WriteQueryResult::PageSetup(p) => {
            assert_eq!(
                (p.top, p.left, p.width),
                (Some(54.0), Some(90.0), Some(500.0))
            );
            assert_eq!(p.vertical_align, Some(CellVerticalAlign::Center));
        }
        other => panic!("{other:?}"),
    }
    assert!(
        d.execute(&wr(WriteCommand::PageSetup {
            setup: Box::new(PageSetupSpec {
                left: Some(250.0),
                right: Some(250.0),
                ..PageSetupSpec::default()
            }),
        }))
        .is_err(),
        "no room left for text"
    );
    assert!(d.execute(&wr(WriteCommand::Columns { count: 13 })).is_err());
}

#[test]
fn header_footer_editing_creates_types_and_closes() {
    let mut d = three_paragraphs();
    caret(&mut d, 1, 2);
    d.view_synced();
    assert_eq!(run(&mut d, wr(WriteCommand::EditHeader)), Change::Content);
    assert_eq!(format(&d).story, StoryKind::Header);
    assert!(
        format(&d)
            .header_footer
            .is_some_and(|h| h.label == "Header")
    );
    typ(&mut d, "Kiln Press");
    let header = d.doc.headers_footers.first().expect("a header");
    assert!(
        header.blocks.iter().any(
            |b| matches!(b, Block::Paragraph(p) if p.content.plain_text(false) == "Kiln Press")
        )
    );
    run(&mut d, wr(WriteCommand::CloseHeaderFooter));
    assert_eq!(format(&d).story, StoryKind::Body);
    assert_eq!(
        d.head,
        kiln_doc::Pos::new(ids(&d)[1], 2),
        "back where the caret was"
    );
    run(&mut d, wr(WriteCommand::EditFooter));
    assert_eq!(format(&d).story, StoryKind::Footer);
    run(&mut d, wr(WriteCommand::DifferentFirstPage));
    assert!(d.doc.sections[0].props.title_page);
    assert!(
        format(&d)
            .header_footer
            .is_some_and(|h| h.label == "First Page Footer")
    );
    run(&mut d, wr(WriteCommand::DifferentOddEven));
    assert!(d.doc.settings.even_and_odd_headers);
}

#[test]
fn header_footer_link_to_previous_and_page_numbers() {
    let mut d = three_paragraphs();
    caret(&mut d, 1, 0);
    run(
        &mut d,
        wr(WriteCommand::SectionBreak {
            kind: SectionStart::NextPage,
        }),
    );
    caret(&mut d, 0, 0);
    d.view_synced();
    run(
        &mut d,
        wr(WriteCommand::PageNumber {
            position: PageNumberPosition::BottomCenter,
        }),
    );
    let footer_id = d.doc.sections[0].footers.default.expect("a footer");
    let footer = d
        .doc
        .headers_footers
        .iter()
        .find(|h| h.id == footer_id)
        .expect("footer");
    let has_page = footer.blocks.iter().any(|b| match b {
        Block::Paragraph(p) => p
            .content
            .objects()
            .iter()
            .any(|o| matches!(o, InlineObject::Field(f) if f.instr == "PAGE")),
        _ => false,
    });
    assert!(has_page);
    run(&mut d, wr(WriteCommand::CloseHeaderFooter));
    // The second section's footer is linked: unlinking copies it.
    let last = ids(&d).len() - 1;
    caret(&mut d, last, 0);
    d.view_synced();
    run(&mut d, wr(WriteCommand::EditFooter));
    assert!(
        format(&d)
            .header_footer
            .is_some_and(|h| h.linked && h.has_previous)
    );
    run(&mut d, wr(WriteCommand::LinkToPrevious));
    assert!(d.doc.sections[1].footers.default.is_some());
    assert!(format(&d).header_footer.is_some_and(|h| !h.linked));
    run(&mut d, wr(WriteCommand::LinkToPrevious));
    assert!(d.doc.sections[1].footers.default.is_none());
    run(&mut d, wr(WriteCommand::CloseHeaderFooter));
    assert!(
        d.execute(&wr(WriteCommand::LinkToPrevious)).is_err(),
        "only in a header"
    );
}

#[test]
fn header_footer_opens_with_a_double_click_in_the_margin() {
    let mut d = three_paragraphs();
    d.view_synced();
    let (x, y) = {
        let v = d.view().expect("view");
        let r = v.layout.page_rect(0).expect("page");
        (r.x + r.w / 2.0, r.y + 20.0)
    };
    run(
        &mut d,
        wr(WriteCommand::SelectAt {
            point: Point::new(x, y),
            extend: false,
            unit: SelectUnit::Word,
        }),
    );
    assert_eq!(format(&d).story, StoryKind::Header);
    run(&mut d, wr(WriteCommand::CloseHeaderFooter));
    assert_eq!(format(&d).story, StoryKind::Body);
}

// ----- insert ---------------------------------------------------------------------------

#[test]
fn insert_links_remove_them_and_refuse_scripts() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 4), (0, 8));
    run(
        &mut d,
        wr(WriteCommand::InsertLink {
            url: "https://zahakj.github.io/kiln/".into(),
            text: None,
            tooltip: Some("Kiln".into()),
        }),
    );
    assert_eq!(
        format(&d).link.as_deref(),
        Some("https://zahakj.github.io/kiln/")
    );
    run(&mut d, wr(WriteCommand::RemoveLink));
    assert!(format(&d).link.is_none());
    caret(&mut d, 1, 0);
    run(
        &mut d,
        wr(WriteCommand::InsertLink {
            url: "mailto:kiln@example.org".into(),
            text: Some("write to us ".into()),
            tooltip: None,
        }),
    );
    assert!(texts(&d)[1].starts_with("write to us Warm"));
    assert!(
        fails(
            &mut d,
            wr(WriteCommand::InsertLink {
                url: "javascript:alert(1)".into(),
                text: None,
                tooltip: None,
            })
        )
        .contains("web address")
    );
}

#[test]
fn insert_bookmarks_and_go_to_them() {
    let mut d = three_paragraphs();
    select(&mut d, (2, 0), (2, 4));
    run(
        &mut d,
        wr(WriteCommand::InsertBookmark {
            name: "fire".into(),
        }),
    );
    assert!(
        matches!(query(&d, WriteQuery::Bookmarks), WriteQueryResult::Bookmarks(b) if b == vec!["fire".to_owned()])
    );
    caret(&mut d, 0, 0);
    run(
        &mut d,
        wr(WriteCommand::GoTo {
            target: GoToTarget::Bookmark {
                name: "fire".into(),
            },
        }),
    );
    assert_eq!(d.head.para, ids(&d)[2]);
    assert!(
        d.execute(&wr(WriteCommand::InsertBookmark { name: "1x".into() }))
            .is_err()
    );
    // A second bookmark of that name moves it.
    select(&mut d, (0, 0), (0, 3));
    run(
        &mut d,
        wr(WriteCommand::InsertBookmark {
            name: "fire".into(),
        }),
    );
    assert_eq!(d.doc.bookmarks().len(), 1);
}

#[test]
fn insert_dates_times_fields_and_update() {
    let mut d = doc();
    d.editor.now_ms = Some(1_792_222_200_000); // 2026-10-17 07:30 UTC
    run(&mut d, ed(EditCommand::InsertDate));
    assert_eq!(texts(&d), vec!["10/17/2026"]);
    typ(&mut d, " ");
    run(&mut d, ed(EditCommand::InsertTime));
    assert!(texts(&d)[0].ends_with("7:30 AM"), "{:?}", texts(&d));
    typ(&mut d, " ");
    run(
        &mut d,
        wr(WriteCommand::InsertDateTime {
            format: "d MMMM yyyy".into(),
            as_field: true,
        }),
    );
    assert!(texts(&d)[0].ends_with("17 October 2026"));
    d.doc.properties.creator = Some("Zoë".into());
    run(
        &mut d,
        wr(WriteCommand::InsertField {
            code: "AUTHOR".into(),
        }),
    );
    assert!(texts(&d)[0].ends_with("Zoë"));
    d.editor.now_ms = Some(1_792_308_600_000);
    run(&mut d, wr(WriteCommand::SelectAll));
    run(&mut d, wr(WriteCommand::UpdateFields));
    assert!(texts(&d)[0].contains("18 October 2026"), "{:?}", texts(&d));
    assert!(
        d.execute(&wr(WriteCommand::InsertField {
            code: String::new()
        }))
        .is_err()
    );
}

#[test]
fn insert_drop_cap_and_pictures() {
    let mut d = three_paragraphs();
    run(
        &mut d,
        wr(WriteCommand::DropCap {
            placement: Some(kiln_protocol::commands::DropCapPlacement::Dropped),
        }),
    );
    assert!(para_props(&d, 2).drop_cap.is_some());
    let mut png = Vec::new();
    image::RgbaImage::from_pixel(1600, 400, image::Rgba([200, 80, 40, 255]))
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .expect("png");
    caret(&mut d, 0, 3);
    run(
        &mut d,
        wr(WriteCommand::InsertPicture {
            name: "glaze.png".into(),
            bytes: png,
            floating: false,
        }),
    );
    let p = d.doc.paragraph(ids(&d)[0]).expect("paragraph");
    let drawing = p.content.objects().iter().find_map(|o| match o {
        InlineObject::Drawing(dr) => Some(dr.clone()),
        _ => None,
    });
    let drawing = drawing.expect("a picture");
    assert!(
        drawing.width <= 6_500_000 && drawing.width > 4 * drawing.height - 10,
        "scaled to the column"
    );
    assert_eq!(d.doc.media.len(), 1);
    assert!(
        d.execute(&wr(WriteCommand::InsertPicture {
            name: "x.svg".into(),
            bytes: b"<svg/>".to_vec(),
            floating: false,
        }))
        .is_err()
    );
    assert!(d.undo());
    assert!(
        d.doc.media.is_empty(),
        "undo removes the picture and its media"
    );
}

// ----- clipboard --------------------------------------------------------------------------

#[test]
fn clipboard_copy_writes_kiln_html_and_text() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 4), (0, 8));
    run(&mut d, ed(EditCommand::Bold));
    select(&mut d, (0, 0), (1, 4));
    run(&mut d, ed(EditCommand::Copy));
    let c = d.take_copied().expect("copied");
    assert_eq!(c.text, "The kiln glows\nWarm");
    assert!(
        c.html
            .as_deref()
            .is_some_and(|h| h.contains("<b>") && h.contains("kiln"))
    );
    assert!(c.kiln.as_deref().is_some_and(|k| k.contains("\"v\":1")));
    assert!(d.take_copied().is_none(), "taken once");
    caret(&mut d, 0, 0);
    run(&mut d, ed(EditCommand::Copy));
    assert!(
        d.take_copied().is_none(),
        "nothing selected, nothing copied"
    );
}

#[test]
fn clipboard_cut_and_paste_keep_source_round_trip() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 4), (0, 8));
    run(&mut d, ed(EditCommand::Italic));
    select(&mut d, (0, 4), (1, 4));
    run(&mut d, ed(EditCommand::Cut));
    let c = d.take_copied().expect("cut");
    assert_eq!(texts(&d), vec!["The  clay waits", "Fire and form"]);
    caret(&mut d, 1, 13);
    run(&mut d, ed(EditCommand::Paste { data: Some(c) }));
    assert_eq!(
        texts(&d),
        vec!["The  clay waits", "Fire and formkiln glows", "Warm"]
    );
    assert!(
        resolved(&d, 1, 14).italic,
        "the source formatting comes along"
    );
    assert!(d.undo());
    assert_eq!(
        texts(&d),
        vec!["The  clay waits", "Fire and form"],
        "one undo step"
    );
}

#[test]
fn clipboard_paste_text_only_and_merge_formatting() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 3));
    run(&mut d, ed(EditCommand::Bold));
    run(
        &mut d,
        ed(EditCommand::FontColor {
            color: Some("C8352B".into()),
        }),
    );
    run(&mut d, ed(EditCommand::Copy));
    let c = d.take_copied().expect("copied");
    caret(&mut d, 2, 0);
    run(
        &mut d,
        ed(EditCommand::PasteTextOnly {
            data: Some(c.clone()),
        }),
    );
    assert!(texts(&d)[2].starts_with("TheFire"));
    assert!(
        !resolved(&d, 2, 0).bold,
        "text only takes the destination's formatting"
    );
    caret(&mut d, 1, 0);
    run(&mut d, ed(EditCommand::PasteMerge { data: Some(c) }));
    let r = resolved(&d, 1, 0);
    assert!(r.bold, "merge keeps bold");
    assert!(r.color.rgb().is_none(), "merge drops the colour");
    assert!(d.execute(&ed(EditCommand::Paste { data: None })).is_err());
}

#[test]
fn clipboard_html_paste_maps_headings_lists_tables_and_links() {
    let mut d = doc();
    let html = "<h2>Firing log</h2><ul><li>Bisque</li><li>Glaze <b>hot</b></li></ul>\
        <table><tr><td>Cone</td><td>06</td></tr><tr><td>Hours</td><td>8</td></tr></table>\
        <p>See <a href=\"https://kiln.dev\">notes</a><script>alert(1)</script></p>";
    run(
        &mut d,
        ed(EditCommand::Paste {
            data: Some(ClipboardData {
                text: "Firing log".into(),
                html: Some(html.into()),
                kiln: None,
            }),
        }),
    );
    let t = texts(&d);
    assert_eq!(t[0], "Firing log");
    assert_eq!(para_props(&d, 0).style_id, "Heading2");
    assert!(para_props(&d, 1).numbering.is_some() && para_props(&d, 2).numbering.is_some());
    let table = first_table(&d);
    assert_eq!(cell_text(&table, 1, 1), "8");
    assert!(!t.iter().any(|x| x.contains("alert")));
    let last = d
        .body()
        .iter()
        .rev()
        .find(|p| p.content.plain_text(false).starts_with("See"))
        .map(|p| p.content.marks().to_vec())
        .expect("link paragraph");
    assert!(last.iter().any(|m| matches!(m.mark, Mark::Link(_))));
}

#[test]
fn clipboard_query_matches_copy() {
    let mut d = three_paragraphs();
    select(&mut d, (2, 0), (2, 4));
    match query(&d, WriteQuery::Clipboard) {
        WriteQueryResult::Clipboard(Some(c)) => assert_eq!(c.text, "Fire"),
        other => panic!("{other:?}"),
    }
}

// ----- find, replace, go to ---------------------------------------------------------------

#[test]
fn find_next_wraps_and_respects_case_and_whole_words() {
    let mut d = doc();
    typ(&mut d, "Kiln kilns kiln\nthe KILN");
    run(
        &mut d,
        wr(WriteCommand::FindNext {
            query: find("kiln"),
            backward: false,
        }),
    );
    assert_eq!((d.span().0.offset, d.span().1.offset), (0, 4));
    let whole = FindQuery {
        whole_word: true,
        match_case: true,
        ..find("kiln")
    };
    run(
        &mut d,
        wr(WriteCommand::FindNext {
            query: whole.clone(),
            backward: false,
        }),
    );
    assert_eq!(d.span().0.offset, 11, "skips Kiln and kilns");
    run(
        &mut d,
        wr(WriteCommand::FindNext {
            query: whole,
            backward: false,
        }),
    );
    assert_eq!(d.span().0.offset, 11, "wraps to the only match");
    run(
        &mut d,
        wr(WriteCommand::FindNext {
            query: find("kiln"),
            backward: true,
        }),
    );
    assert_eq!(d.span().0.offset, 5);
    match query(
        &d,
        WriteQuery::FindAll {
            query: find("kiln"),
        },
    ) {
        WriteQueryResult::FindAll(r) => {
            assert_eq!(r.total, 4);
            assert_eq!(r.hits[3].before, "the ");
        }
        other => panic!("{other:?}"),
    }
    match query(
        &d,
        WriteQuery::FindAll {
            query: FindQuery {
                regex: true,
                ..find("(")
            },
        },
    ) {
        WriteQueryResult::FindAll(r) => assert!(r.error.is_some()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn find_replace_one_and_all_in_one_undo_step() {
    let mut d = doc();
    typ(&mut d, "clay clay clay\nmore clay");
    run(
        &mut d,
        wr(WriteCommand::FindNext {
            query: find("clay"),
            backward: false,
        }),
    );
    run(
        &mut d,
        wr(WriteCommand::Replace {
            query: find("clay"),
            replacement: "glaze".into(),
        }),
    );
    assert_eq!(texts(&d)[0], "glaze clay clay");
    run(
        &mut d,
        wr(WriteCommand::ReplaceAll {
            query: find("clay"),
            replacement: "^& pot".into(),
        }),
    );
    assert_eq!(texts(&d), vec!["glaze clay pot clay pot", "more clay pot"]);
    assert!(d.undo());
    assert_eq!(texts(&d), vec!["glaze clay clay", "more clay"]);
}

#[test]
fn find_wildcards_regex_and_paragraph_marks() {
    let mut d = doc();
    typ(&mut d, "cone 06 and cone 10\nend");
    run(
        &mut d,
        wr(WriteCommand::ReplaceAll {
            query: FindQuery {
                wildcards: true,
                ..find("cone ([0-9]@)")
            },
            replacement: "\\1".into(),
        }),
    );
    assert_eq!(texts(&d)[0], "06 and 10");
    run(
        &mut d,
        wr(WriteCommand::ReplaceAll {
            query: FindQuery {
                regex: true,
                ..find("\\d+")
            },
            replacement: "#".into(),
        }),
    );
    assert_eq!(texts(&d)[0], "# and #");
    run(
        &mut d,
        wr(WriteCommand::ReplaceAll {
            query: find("^p"),
            replacement: " / ".into(),
        }),
    );
    assert_eq!(texts(&d), vec!["# and # / end"]);
}

#[test]
fn find_go_to_pages_lines_sections_and_headings() {
    let mut d = fixture("long-text.docx");
    d.view_synced();
    run(
        &mut d,
        wr(WriteCommand::GoTo {
            target: GoToTarget::Page {
                number: 3,
                relative: false,
            },
        }),
    );
    let page = d.view().and_then(|v| v.caret(d.head)).map(|c| c.page);
    assert_eq!(page, Some(2));
    run(
        &mut d,
        wr(WriteCommand::GoTo {
            target: GoToTarget::Page {
                number: -1,
                relative: true,
            },
        }),
    );
    assert_eq!(
        d.view().and_then(|v| v.caret(d.head)).map(|c| c.page),
        Some(1)
    );
    run(
        &mut d,
        wr(WriteCommand::GoTo {
            target: GoToTarget::Line { number: 1 },
        }),
    );
    assert_eq!(d.head, d.doc_start());
    let headings = match query(&d, WriteQuery::Headings) {
        WriteQueryResult::Headings(h) => h,
        other => panic!("{other:?}"),
    };
    assert_eq!(headings.len(), 20);
    run(
        &mut d,
        wr(WriteCommand::GoTo {
            target: GoToTarget::Paragraph {
                para: headings[5].para,
            },
        }),
    );
    assert_eq!(d.head.para, headings[5].para);
    run(
        &mut d,
        wr(WriteCommand::GoTo {
            target: GoToTarget::Section { number: 1 },
        }),
    );
    assert!(
        d.execute(&wr(WriteCommand::GoTo {
            target: GoToTarget::Bookmark {
                name: "nowhere".into()
            },
        }))
        .is_err()
    );
}

// ----- queries ---------------------------------------------------------------------------

#[test]
fn queries_format_rulers_pages_and_counts() {
    let mut d = three_paragraphs();
    d.view_synced();
    let f = format(&d);
    assert_eq!(
        (f.font.as_str(), f.size, f.style_name.as_str()),
        ("Calibri", 11.0, "Normal")
    );
    assert!(f.collapsed && f.can_repeat, "typing repeats with F4");
    select(&mut d, (0, 0), (0, 8));
    run(&mut d, ed(EditCommand::Bold));
    select(&mut d, (0, 0), (0, 14));
    assert!(format(&d).mixed.contains(&"bold".to_owned()));
    let r = match query(&d, WriteQuery::Rulers) {
        WriteQueryResult::Rulers(Some(r)) => r,
        other => panic!("{other:?}"),
    };
    assert!(r.text_rect.width > 400.0 && r.margin_top > 0.0 && r.margin_bottom > r.margin_top);
    match query(&d, WriteQuery::Pages) {
        WriteQueryResult::Pages(p) => assert_eq!((p.count, p.rects.len()), (1, 1)),
        other => panic!("{other:?}"),
    }
    match query(
        &d,
        WriteQuery::WordCount {
            include_notes: false,
        },
    ) {
        WriteQueryResult::WordCount(w) => {
            assert!(w.selection);
            assert_eq!(w.words, 3);
        }
        other => panic!("{other:?}"),
    }
    caret(&mut d, 0, 0);
    match query(
        &d,
        WriteQuery::WordCount {
            include_notes: true,
        },
    ) {
        WriteQueryResult::WordCount(w) => assert_eq!((w.words, w.paragraphs, w.lines), (9, 3, 3)),
        other => panic!("{other:?}"),
    }
    assert!(
        matches!(query(&d, WriteQuery::Properties), WriteQueryResult::Properties(p) if p.words == 9)
    );
}

#[test]
fn queries_styles_list_for_the_gallery_and_pane() {
    let mut d = three_paragraphs();
    run(&mut d, wr(WriteCommand::StyleHeading1));
    let styles = match query(&d, WriteQuery::Styles) {
        WriteQueryResult::Styles(s) => s,
        other => panic!("{other:?}"),
    };
    let h1 = styles
        .iter()
        .find(|s| s.id == "Heading1")
        .expect("Heading 1");
    assert!(h1.current && h1.in_use && h1.quick);
    assert_eq!(h1.size, 16.0);
    assert!(styles.iter().any(|s| s.id == "Normal" && s.in_use));
    assert!(
        !styles.iter().any(|s| s.id == "Heading1Char"),
        "linked character styles fold in"
    );
}

#[test]
fn command_log_records_every_command_with_its_outcome() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 3));
    run(&mut d, ed(EditCommand::Bold));
    let _ = d.execute(&ed(EditCommand::TableMergeCells));
    run(&mut d, wr(WriteCommand::ShowMarks));
    let log = match query(&d, WriteQuery::CommandLog) {
        WriteQueryResult::CommandLog(l) => l,
        other => panic!("{other:?}"),
    };
    let tail: Vec<(&str, &str)> = log
        .iter()
        .rev()
        .take(3)
        .rev()
        .map(|e| (e.id.as_str(), e.outcome.as_str()))
        .collect();
    assert_eq!(
        tail,
        vec![
            ("suite.format.bold", "applied"),
            ("suite.table.merge_cells", "error"),
            ("write.paragraph.show_marks", "selection"),
        ]
    );
}

#[test]
fn show_marks_draws_pilcrows_dots_and_tab_arrows() {
    let mut d = doc();
    typ(&mut d, "a b\tc\nd");
    d.view_synced();
    let marks = super::marks::marks(d.view().expect("view"), &d.doc);
    let dots = marks
        .iter()
        .filter(|o| matches!(o, kiln_scene::DrawOp::FillRect { .. }))
        .count();
    assert_eq!(dots, 1, "one space");
    assert_eq!(
        marks
            .iter()
            .filter(|o| matches!(o, kiln_scene::DrawOp::Path { .. }))
            .count(),
        2,
        "pilcrows and the tab arrow"
    );
    run(&mut d, wr(WriteCommand::ShowMarks));
    assert!(format(&d).show_marks);
}

#[test]
fn repeat_runs_the_last_command_again() {
    let mut d = three_paragraphs();
    select(&mut d, (0, 0), (0, 3));
    run(&mut d, ed(EditCommand::Bold));
    select(&mut d, (2, 0), (2, 4));
    assert!(format(&d).can_repeat);
    run(&mut d, ed(EditCommand::Repeat));
    assert!(resolved(&d, 2, 0).bold);
    typ(&mut d, "Clay");
    caret(&mut d, 1, 0);
    run(&mut d, ed(EditCommand::Repeat));
    assert!(texts(&d)[1].starts_with("Clay"), "F4 repeats typing");
}

// ----- the session: host round trips ------------------------------------------------------

mod session {
    use std::collections::VecDeque;
    use std::sync::{Arc, Mutex};

    use kiln_protocol::{DocKind, HostOp, HostReply};

    use super::*;
    use crate::host::Host;
    use crate::session::Session;

    /// A host that answers from a script and records what it was asked.
    #[derive(Clone, Default)]
    struct Scripted {
        replies: Arc<Mutex<VecDeque<HostReply>>>,
        asked: Arc<Mutex<Vec<HostOp>>>,
    }

    impl Host for Scripted {
        fn request(&mut self, op: &HostOp) -> Option<HostReply> {
            self.asked.lock().ok()?.push(op.clone());
            if matches!(op, HostOp::ClipboardWrite { .. }) {
                return Some(HostReply::Done);
            }
            self.replies.lock().ok()?.pop_front()
        }
        fn now_ms(&self) -> u64 {
            1_792_222_200_000
        }
    }

    fn session(host: &Scripted) -> (Session, kiln_common::DocId) {
        let mut s = Session::new(Box::new(host.clone()));
        let doc = s
            .execute(
                None,
                SuiteCommand::New {
                    kind: DocKind::Write,
                }
                .into(),
            )
            .expect("new")
            .summary
            .expect("summary")
            .doc;
        (s, doc)
    }

    fn paragraphs(s: &Session, doc: kiln_common::DocId) -> Vec<String> {
        match s
            .query(Some(doc), &kiln_protocol::Query::Paragraphs)
            .expect("paragraphs")
        {
            kiln_protocol::QueryResult::Paragraphs(p) => p.into_iter().map(|p| p.text).collect(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn copy_writes_the_clipboard_and_paste_reads_it_back_rich() {
        let host = Scripted::default();
        let (mut s, doc) = session(&host);
        s.execute(
            Some(doc),
            wr(WriteCommand::InsertText {
                text: "Kiln glows".into(),
            }),
        )
        .expect("type");
        s.execute(Some(doc), wr(WriteCommand::SelectAll))
            .expect("select");
        s.execute(Some(doc), ed(EditCommand::Bold)).expect("bold");
        s.execute(Some(doc), ed(EditCommand::Copy)).expect("copy");
        let written = host
            .asked
            .lock()
            .expect("lock")
            .iter()
            .find_map(|op| match op {
                HostOp::ClipboardWrite { text, html } => Some((text.clone(), html.clone())),
                _ => None,
            });
        let (text, html) = written.expect("a clipboard write");
        assert_eq!(text, "Kiln glows");
        assert!(html.is_some());
        // Paste from the ribbon: the host's clipboard holds the same text, so the rich format
        // the session kept is used.
        host.replies
            .lock()
            .expect("lock")
            .push_back(HostReply::Clipboard {
                text: text.clone(),
                html: None,
            });
        s.execute(
            Some(doc),
            wr(WriteCommand::MoveCaret {
                motion: CaretMotion::DocumentEnd,
                extend: false,
            }),
        )
        .expect("move");
        s.execute(Some(doc), ed(EditCommand::Paste { data: None }))
            .expect("paste");
        assert_eq!(paragraphs(&s, doc), vec!["Kiln glowsKiln glows"]);
        match s
            .query(Some(doc), &kiln_protocol::Query::Write(WriteQuery::Format))
            .expect("format")
        {
            kiln_protocol::QueryResult::Write(WriteQueryResult::Format(f)) => assert!(f.bold),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn insert_pictures_picks_reads_and_inserts() {
        let host = Scripted::default();
        let (mut s, doc) = session(&host);
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
            .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
            .expect("png");
        host.replies.lock().expect("lock").extend([
            HostReply::Path("/pics/dot.png".into()),
            HostReply::Bytes(png),
        ]);
        s.execute(Some(doc), ed(EditCommand::InsertPictures))
            .expect("insert");
        let asked = host.asked.lock().expect("lock").clone();
        assert!(
            matches!(&asked[0], HostOp::PickOpen { filters } if filters[0].extensions.contains(&"png".to_owned()))
        );
        assert!(matches!(&asked[1], HostOp::ReadFile { path } if path == "/pics/dot.png"));
        assert_eq!(paragraphs(&s, doc), vec!["\u{FFFC}"]);
        assert!(s.summary(doc).expect("summary").can_undo);
    }

    #[test]
    fn edit_commands_are_unavailable_on_a_read_only_document() {
        let host = Scripted::default();
        let mut s = Session::new(Box::new(host));
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../fixtures/docx/features/paragraphs.docx"),
        )
        .expect("fixture");
        let summary = s
            .open(
                &bytes,
                kiln_protocol::OpenHint {
                    name: Some("paragraphs.docx".into()),
                    read_only: true,
                    ..kiln_protocol::OpenHint::default()
                },
            )
            .expect("opens");
        let err = s
            .execute(Some(summary.doc), ed(EditCommand::Bold))
            .expect_err("read-only");
        assert_eq!(err.kind(), kiln_common::ErrorKind::Unavailable);
        assert!(
            s.execute(Some(summary.doc), wr(WriteCommand::ShowMarks))
                .is_ok(),
            "viewing commands still run"
        );
    }
}

#[test]
#[ignore = "timing probe for the 300-page document (run with --ignored --nocapture)"]
fn probe_typing_cost_on_the_300_page_document() {
    use crate::frame::FrameState;
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/large/doc-300p.docx");
    let bytes = std::fs::read(path).expect("fixture");
    let read = kiln_io_docx::read_docx(&bytes).expect("reads");
    let mut d = WriteDoc::new(read.document, read.warnings, None);
    let vp = kiln_protocol::Viewport {
        width_px: 1200.0,
        height_px: 800.0,
        zoom: 1.0,
        dpr: 1.0,
        ..kiln_protocol::Viewport::default()
    };
    let mut frames = FrameState::new();
    frames.produce(d.frame_source(), &vp, 1).expect("frame");
    for r in 2..40 {
        frames.produce(d.frame_source(), &vp, r).expect("frame");
    }
    caret(&mut d, 3, 0);
    let mut times = Vec::new();
    for i in 0..60u64 {
        let t = std::time::Instant::now();
        typ(&mut d, "k");
        let t1 = t.elapsed();
        let _ = d.selection();
        let t2 = t.elapsed();
        let _ = d.status();
        let t3 = t.elapsed();
        let _ = format(&d);
        let t4 = t.elapsed();
        frames
            .produce(d.frame_source(), &vp, 100 + i)
            .expect("frame");
        let t5 = t.elapsed();
        times.push((t1, t2 - t1, t3 - t2, t4 - t3, t5 - t4));
    }
    times.sort_by_key(|t| t.0 + t.1 + t.2 + t.3 + t.4);
    eprintln!("median {:?}\np95 {:?}", times[30], times[57]);
}

/// The list label the layout draws before a paragraph (`""` when none).
fn drawn_label(d: &mut WriteDoc, para: usize) -> String {
    let id = ids(d)[para];
    let v = d.view_synced();
    let layout = &v.layout;
    let Some(r) = layout.lines_of(id).first().copied() else {
        return String::new();
    };
    layout.line(r).map_or_else(String::new, |l| {
        let src = &l.para.src;
        src.map.text().get(..src.label_len).unwrap_or("").to_owned()
    })
}

#[test]
fn lists_draw_their_labels_as_soon_as_they_are_applied() {
    let mut d = three_paragraphs();
    let _ = drawn_label(&mut d, 0);
    caret(&mut d, 1, 0);
    run(&mut d, ed(EditCommand::Bullets { glyph: None }));
    assert!(
        drawn_label(&mut d, 1).starts_with('•'),
        "{:?}",
        drawn_label(&mut d, 1)
    );
    assert_eq!(drawn_label(&mut d, 0), "");
    // Enter continues the list; the new item has its own bullet.
    caret(&mut d, 1, 3);
    typ(&mut d, "\n");
    assert!(
        drawn_label(&mut d, 2).starts_with('•'),
        "{:?}",
        drawn_label(&mut d, 2)
    );
    // Numbering renumbers as items join.
    select(&mut d, (0, 0), (3, 0));
    run(&mut d, ed(EditCommand::Numbering { style: None }));
    let labels: Vec<String> = (0..4).map(|i| drawn_label(&mut d, i)).collect();
    assert!(labels[0].starts_with("1."), "{labels:?}");
    assert!(labels[3].starts_with("4."), "{labels:?}");
}
