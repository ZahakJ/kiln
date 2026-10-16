// SPDX-License-Identifier: AGPL-3.0-or-later
//! Element-level reader tests on small hand-written packages: every WordprocessingML construct
//! that the generated corpus does not contain (content controls, equations, moves, nested fields,
//! VML, charts, threads, …) and the edge cases of the ones it does. Ported from the v1.0
//! TypeScript reader's tests; where the Rust model is richer (block content controls, move
//! marks, one-sided wrapping, tables in Word's own cell model) the assertions follow the model.

mod read_support;

use kiln_doc::st::{
    BreakClear, BreakKind, DropCapKind, HeaderFooterKind, NumberFormat, Orientation, PageVAlign,
    RelativeFromH, RelativeFromV, SdtKind, SectionBreak, TableCondition, TextDirection,
    ThemeColorName, ThemeFontRef, UnderlineStyle, VMerge, WidthType, WrapSide, WrapType,
};
use kiln_doc::{
    Block, Crop, DocColor, DrawingKind, Indent, InlineObject, LinkTarget, Mark, NumberingRef,
    Paragraph, Placement, PosH, PosV, RevisionInfo, TableWidth,
};
use kiln_io_docx::common::xml::parse_fragment;
use kiln_io_docx::{DocxError, DocxKind, Severity, read_docx};
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, content_type, rel_type};
use kiln_testing::fixtures::find;
use read_support::{
    Build, ExtraPart, all_paragraphs, at_text, body_paragraphs, build_docx, codes, image_part,
    objects, p, p_pr, part, r, r_pr, read_body, read_build,
};

fn texts(r: &kiln_io_docx::DocxRead) -> Vec<String> {
    body_paragraphs(&r.document)
        .into_iter()
        .map(Paragraph::text)
        .collect()
}

fn first_paragraph(r: &kiln_io_docx::DocxRead) -> &Paragraph {
    body_paragraphs(&r.document)[0]
}

fn complex(instr: &str, result: &str) -> String {
    format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> {instr} </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{result}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"
    )
}

fn fields(r: &kiln_io_docx::DocxRead) -> Vec<kiln_doc::Field> {
    objects(&r.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::Field(f) => Some(f.clone()),
            _ => None,
        })
        .collect()
}

fn drawings(r: &kiln_io_docx::DocxRead) -> Vec<kiln_doc::Drawing> {
    objects(&r.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::Drawing(d) => Some((**d).clone()),
            _ => None,
        })
        .collect()
}

fn links(marks: &[&Mark]) -> Option<kiln_doc::Hyperlink> {
    marks.iter().find_map(|m| match m {
        Mark::Link(l) => Some(l.clone()),
        _ => None,
    })
}

// ------------------------------------------------------------------ package and errors

#[test]
fn rejects_packages_that_are_not_word_documents() {
    let xlsx = find("formulas.xlsx")
        .and_then(|f| f.read())
        .expect("fixture");
    assert!(matches!(read_docx(&xlsx), Err(DocxError::NotWord(_))));
    assert!(matches!(
        read_docx(b"plain text"),
        Err(DocxError::Package(_))
    ));
}

#[test]
fn rejects_a_main_part_that_is_not_well_formed() {
    let mut pkg = OpcPackage::new();
    pkg.add_part(
        "/word/document.xml",
        content_type::DOCX_DOCUMENT,
        "this is not XML",
    )
    .expect("part");
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        "/word/document.xml",
        RelationshipOptions::default(),
    )
    .expect("rel");
    let e = read_docx(&pkg.save().expect("save")).expect_err("not XML");
    assert!(matches!(e, DocxError::NotWord(_)), "{e}");
}

#[test]
fn uses_defaults_when_shared_parts_are_absent() {
    let r = read_body(&p(&r("Bare")));
    let doc = &r.document;
    assert!(doc.styles.is_empty());
    assert_eq!(doc.settings.default_tab_stop, 720);
    assert!(!doc.theme.name.is_empty());
    let s = &doc.sections[0].props;
    assert_eq!((s.page_size.width, s.page_size.height), (12240, 15840));
    assert_eq!(r.kind, DocxKind::Document);
    assert_eq!(texts(&r), ["Bare"]);
}

#[test]
fn reads_iso_29500_strict_documents() {
    let strict = "http://purl.oclc.org/ooxml/wordprocessingml/main";
    let mut pkg = OpcPackage::new();
    pkg.add_part(
        "/word/document.xml",
        content_type::DOCX_DOCUMENT,
        format!("<w:document xmlns:w=\"{strict}\"><w:body><w:p><w:pPr><w:jc w:val=\"center\"/></w:pPr><w:r><w:t>Strict</w:t></w:r></w:p><w:sectPr><w:pgSz w:w=\"21cm\" w:h=\"29.7cm\"/></w:sectPr></w:body></w:document>"),
    )
    .expect("part");
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        "/word/document.xml",
        RelationshipOptions::default(),
    )
    .expect("rel");
    let r = read_docx(&pkg.save().expect("save")).expect("reads");
    assert_eq!(texts(&r), ["Strict"]);
    assert_eq!(
        first_paragraph(&r).props.alignment,
        Some(kiln_doc::st::Alignment::Center)
    );
    assert_eq!(r.document.sections[0].props.page_size.width, 11906);
}

#[test]
fn templates_and_macro_documents_are_recognised() {
    for (ct, kind) in [
        (
            "application/vnd.openxmlformats-officedocument.wordprocessingml.template.main+xml",
            DocxKind::Template,
        ),
        (
            "application/vnd.ms-word.document.macroEnabled.main+xml",
            DocxKind::MacroEnabledDocument,
        ),
    ] {
        let bytes = build_docx(Build {
            body: p(&r("x")),
            ..Build::default()
        });
        let mut pkg = OpcPackage::open(&bytes).expect("open");
        pkg.set_content_type("/word/document.xml", ct)
            .expect("content type");
        let read = read_docx(&pkg.save().expect("save")).expect("reads");
        assert_eq!(read.kind, kind);
    }
}

// ------------------------------------------------------------------ runs

#[test]
fn toggles_including_explicit_off_values() {
    let r = read_body(&p(&(r_pr("on", "<w:b/><w:i w:val=\"1\"/>")
        + &r_pr("off", "<w:b w:val=\"0\"/><w:caps w:val=\"false\"/>"))));
    let (on, _) = at_text(&r.document, "on");
    assert_eq!((on.bold, on.italic), (Some(true), Some(true)));
    let (off, _) = at_text(&r.document, "off");
    assert_eq!((off.bold, off.caps), (Some(false), Some(false)));
}

#[test]
fn sizes_and_spacing_as_universal_measures() {
    let r = read_body(&p(&r_pr(
        "x",
        "<w:sz w:val=\"12pt\"/><w:spacing w:val=\"1pt\"/><w:position w:val=\"2pt\"/>",
    )));
    let (props, _) = at_text(&r.document, "x");
    assert_eq!(
        (props.size, props.spacing, props.position),
        (Some(24), Some(20), Some(4))
    );
}

#[test]
fn theme_colours_underline_colours_and_theme_fonts() {
    let r = read_body(&p(&r_pr(
        "c",
        "<w:rFonts w:asciiTheme=\"majorHAnsi\" w:hAnsiTheme=\"majorHAnsi\" w:hint=\"eastAsia\"/><w:color w:val=\"1F3864\" w:themeColor=\"accent1\" w:themeShade=\"80\"/><w:u w:val=\"dash\" w:color=\"FF0000\"/>",
    )));
    let (props, _) = at_text(&r.document, "c");
    assert_eq!(
        props.color,
        Some(DocColor::theme(0x1F3864, ThemeColorName::Accent1).with_shade(128))
    );
    let u = props.underline.clone().expect("underline");
    assert_eq!(
        (u.style, u.color),
        (UnderlineStyle::Dash, Some(DocColor::rgb(0xFF0000)))
    );
    let fonts = props.fonts.clone().expect("fonts");
    assert_eq!(fonts.ascii_theme, Some(ThemeFontRef::MajorHAnsi));
    assert_eq!(fonts.h_ansi_theme, Some(ThemeFontRef::MajorHAnsi));
    assert_eq!(fonts.ascii, None);
    let runs = &r.extras.paragraphs[&first_paragraph(&r).id].runs;
    assert_eq!(runs[0].font_hint.as_deref(), Some("eastAsia"));
}

#[test]
fn unmodelled_run_properties_are_kept_as_raw_xml() {
    let r = read_body(&p(
        &(r_pr("np", "<w:noProof/><w:em w:val=\"dot\"/>") + &r("plain"))
    ));
    let extras = &r.extras.paragraphs[&first_paragraph(&r).id];
    assert_eq!(extras.runs.len(), 1);
    assert_eq!((extras.runs[0].start, extras.runs[0].end), (0, 2));
    assert_eq!(
        extras.runs[0].xml,
        ["<w:noProof/>", "<w:em w:val=\"dot\"/>"]
    );
}

#[test]
fn breaks_carriage_returns_hyphens_and_clear() {
    let r = read_body(&p(
        "<w:r><w:t>a</w:t><w:br w:type=\"textWrapping\" w:clear=\"all\"/><w:t>b</w:t><w:cr/><w:t>c</w:t><w:br w:type=\"column\"/><w:noBreakHyphen/><w:softHyphen/></w:r>",
    ));
    let p = first_paragraph(&r);
    let breaks: Vec<_> = p
        .content
        .objects()
        .iter()
        .filter_map(|o| match o {
            InlineObject::Break(b) => Some(*b),
            _ => None,
        })
        .collect();
    assert_eq!(
        breaks.iter().map(|b| b.kind).collect::<Vec<_>>(),
        [
            BreakKind::TextWrapping,
            BreakKind::TextWrapping,
            BreakKind::Column
        ]
    );
    assert_eq!(breaks[0].clear, Some(BreakClear::All));
    assert_eq!(
        p.content.raw_text(),
        "a\u{FFFC}b\u{FFFC}c\u{FFFC}\u{2011}\u{00AD}"
    );
}

#[test]
fn positional_tabs_become_tabs_with_a_warning() {
    let r = read_body(&p(
        "<w:r><w:ptab w:relativeTo=\"margin\" w:alignment=\"right\" w:leader=\"none\"/></w:r>",
    ));
    assert_eq!(first_paragraph(&r).content.raw_text(), "\t");
    assert!(codes(&r).contains(&"element.unknown"));
}

#[test]
fn unmapped_symbols_keep_their_font_and_code() {
    let r = read_body(&p("<w:r><w:sym w:font=\"Marlett\" w:char=\"F031\"/></w:r>"));
    let sym = first_paragraph(&r)
        .content
        .objects()
        .iter()
        .find_map(|o| match o {
            InlineObject::Symbol(s) => Some(s.clone()),
            _ => None,
        })
        .expect("symbol");
    assert_eq!((sym.font.as_str(), sym.code), ("Marlett", 0xF031));
    assert_eq!(kiln_io_docx::symbol_text(&sym.font, sym.code), "");
}

#[test]
fn ruby_keeps_the_base_text_and_warns() {
    let r = read_body(&p(
        "<w:r><w:ruby><w:rubyPr/><w:rt><w:r><w:t>かん</w:t></w:r></w:rt><w:rubyBase><w:r><w:t>漢</w:t></w:r></w:rubyBase></w:ruby></w:r>",
    ));
    assert_eq!(texts(&r), ["漢"]);
    assert!(codes(&r).contains(&"element.unknown"));
}

// ------------------------------------------------------------------ paragraph properties

#[test]
fn start_end_indents_and_hanging_over_first_line() {
    let r = read_body(&p_pr(
        &r("i"),
        "<w:ind w:start=\"720\" w:end=\"360\" w:firstLine=\"200\" w:hanging=\"400\"/>",
    ));
    assert_eq!(
        first_paragraph(&r).props.indent,
        Some(Indent {
            left: Some(720),
            right: Some(360),
            first_line: None,
            hanging: Some(400)
        })
    );
}

#[test]
fn numbering_outline_level_mark_props_and_para_id() {
    let r = read_body(
        "<w:p w14:paraId=\"1A2B3C4D\"><w:pPr><w:numPr><w:ilvl w:val=\"2\"/><w:numId w:val=\"5\"/></w:numPr><w:outlineLvl w:val=\"1\"/><w:rPr><w:b/></w:rPr></w:pPr><w:r><w:t>n</w:t></w:r></w:p>",
    );
    let p = first_paragraph(&r);
    assert_eq!(p.props.numbering, Some(NumberingRef::new(5, 2)));
    assert_eq!(p.props.outline_level, Some(1));
    assert_eq!(p.mark_props.bold, Some(true));
    assert_eq!(
        r.extras.paragraphs[&p.id].para_id.as_deref(),
        Some("1A2B3C4D")
    );
}

#[test]
fn unmodelled_paragraph_properties_and_frames_are_kept() {
    let r = read_body(&p_pr(
        &r("f"),
        "<w:framePr w:w=\"2000\" w:h=\"400\" w:hSpace=\"180\" w:wrap=\"around\" w:vAnchor=\"text\" w:hAnchor=\"margin\" w:x=\"200\"/><w:snapToGrid w:val=\"0\"/><w:kinsoku w:val=\"0\"/>",
    ));
    let p = first_paragraph(&r);
    let xml = &r.extras.paragraphs[&p.id].ppr_xml;
    assert!(xml[0].starts_with("<w:framePr "), "{xml:?}");
    assert_eq!(xml[1], "<w:kinsoku w:val=\"0\"/>");
    assert_eq!(p.props.snap_to_grid, Some(false));
    assert_eq!(p.props.drop_cap, None);
}

#[test]
fn drop_caps() {
    let r = read_body(&p_pr(
        &r("D"),
        "<w:framePr w:dropCap=\"drop\" w:lines=\"3\" w:wrap=\"around\" w:vAnchor=\"text\" w:hAnchor=\"text\"/>",
    ));
    let dc = first_paragraph(&r).props.drop_cap.expect("drop cap");
    assert_eq!((dc.kind, dc.lines), (DropCapKind::Drop, 3));
}

#[test]
fn paragraph_mark_revisions_and_formatting_changes() {
    let r = read_body(&p_pr(
        &r("x"),
        "<w:rPr><w:ins w:id=\"9\" w:author=\"Ann\" w:date=\"2026-02-01T00:00:00Z\"/><w:i/><w:rPrChange w:id=\"10\" w:author=\"Ann\"><w:rPr><w:b/></w:rPr></w:rPrChange></w:rPr>",
    ));
    let p = first_paragraph(&r);
    let mark = p.revision.mark.as_ref().expect("mark revision");
    assert_eq!(
        mark.info,
        RevisionInfo::new(9, "Ann", Some("2026-02-01T00:00:00Z"))
    );
    assert_eq!(mark.kind, kiln_doc::revision::InsDel::Insertion);
    let change = p.revision.mark_props_change.as_ref().expect("change");
    assert_eq!(change.info.id, 10);
    assert_eq!(change.before.bold, Some(true));
    assert_eq!(p.mark_props.italic, Some(true));
}

// ------------------------------------------------------------------ revisions

#[test]
fn moves_are_move_marks_with_an_info_warning() {
    let r = read_body(
        &(p(
            "<w:moveFromRangeStart w:id=\"1\" w:name=\"move1\"/><w:moveFrom w:id=\"2\" w:author=\"Bo\" w:date=\"2026-03-01T00:00:00Z\"><w:r><w:t>moved</w:t></w:r></w:moveFrom><w:moveFromRangeEnd w:id=\"1\"/>",
        ) + &p("<w:moveTo w:id=\"3\" w:author=\"Bo\"><w:r><w:t>arrived</w:t></w:r></w:moveTo>")),
    );
    let (_, m) = at_text(&r.document, "moved");
    assert!(m.contains(&&Mark::MoveFrom(RevisionInfo::new(
        2,
        "Bo",
        Some("2026-03-01T00:00:00Z")
    ))));
    let (_, m) = at_text(&r.document, "arrived");
    assert!(m.contains(&&Mark::MoveTo(RevisionInfo::new(3, "Bo", None))));
    let w = r
        .warnings
        .iter()
        .find(|w| w.code.as_str() == "revision.move")
        .expect("warning");
    assert_eq!(w.severity, Severity::Info);
}

#[test]
fn deleted_field_codes_and_deleted_text() {
    let r = read_body(&p(
        "<w:del w:id=\"5\" w:author=\"Cy\"><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:delInstrText> PAGE </w:delInstrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r><w:r><w:delText>7</w:delText></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r></w:del>",
    ));
    let f = &fields(&r)[0];
    assert_eq!((f.instr.as_str(), f.result.as_str()), ("PAGE", "7"));
    let p = first_paragraph(&r);
    assert!(
        p.content
            .marks()
            .iter()
            .any(|m| matches!(m.mark, Mark::Deletion(_)))
    );
    assert_eq!(p.content.plain_text(true), "");
}

#[test]
fn tracked_row_insertions_and_cell_revisions() {
    let r = read_body(
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"1000\"/></w:tblGrid><w:tr><w:trPr><w:ins w:id=\"1\" w:author=\"Di\"/></w:trPr><w:tc><w:tcPr><w:cellIns w:id=\"2\" w:author=\"Di\"/></w:tcPr><w:p/></w:tc></w:tr></w:tbl>",
    );
    let Block::Table(t) = &r.document.sections[0].blocks[0] else {
        panic!("table")
    };
    let rev = t.rows[0].revision.as_ref().expect("row revision");
    assert_eq!(rev.kind, kiln_doc::revision::InsDel::Insertion);
    assert_eq!(rev.info.author, "Di");
    assert!(codes(&r).contains(&"revision.cell"));
    let cell = &r.extras.cells[&t.rows[0].cells[0].id];
    assert!(cell.xml[0].starts_with("<w:cellIns"));
}

// ------------------------------------------------------------------ fields

#[test]
fn nested_fields_inside_an_instruction_are_written_as_field_codes() {
    let r = read_body(&p(&format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText xml:space=\"preserve\"> IF </w:instrText></w:r>{}<w:r><w:instrText xml:space=\"preserve\"> = \"F\" \"Ms\" \"Mr\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        complex("MERGEFIELD Gender", &r("F")),
        r("Ms")
    )));
    let f = fields(&r);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0].instr, "IF { MERGEFIELD Gender } = \"F\" \"Ms\" \"Mr\"");
    assert_eq!(f[0].result, "Ms");
    assert_eq!(f[0].form, kiln_doc::fields::FieldForm::Complex);
}

#[test]
fn hyperlink_fields_become_links_and_toc_entries_keep_pageref_fields() {
    let entry = |n: u32| {
        p(&complex(
            &format!("HYPERLINK \\l \"_Toc{n}\""),
            &format!(
                "{}<w:r><w:tab/></w:r>{}",
                r(&format!("Entry {n}")),
                complex(&format!("PAGEREF _Toc{n} \\h"), &r(&(n + 1).to_string()))
            ),
        ))
    };
    let body = format!(
        "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> TOC \\o \"1-3\" \\h </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r></w:p>{}{}{}{}",
        entry(1),
        entry(2),
        p("<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>"),
        p(&r("After"))
    );
    let r = read_body(&body);
    let blocks = &r.document.sections[0].blocks;
    let Block::Field(toc) = &blocks[0] else {
        panic!("a field block, got {blocks:?}")
    };
    assert_eq!(toc.blocks.len(), 4);
    assert_eq!(toc.field.instr, "TOC \\o \"1-3\" \\h");
    let (_, marks) = at_text(&r.document, "Entry 2");
    assert_eq!(
        links(&marks).map(|l| l.target),
        Some(LinkTarget::Bookmark("_Toc2".into()))
    );
    let pagerefs: Vec<(String, String)> = fields(&r)
        .into_iter()
        .map(|f| (f.instr, f.result))
        .collect();
    assert_eq!(
        pagerefs,
        [
            ("PAGEREF _Toc1 \\h".to_owned(), "2".to_owned()),
            ("PAGEREF _Toc2 \\h".to_owned(), "3".to_owned())
        ]
    );
    assert_eq!(
        blocks[1].as_paragraph().map(Paragraph::text).as_deref(),
        Some("After")
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn external_hyperlink_fields_with_a_tooltip() {
    let r = read_body(&p(&complex(
        "HYPERLINK \"https://example.com/a\" \\o \"Tip\"",
        &r("site"),
    )));
    let (_, marks) = at_text(&r.document, "site");
    let l = links(&marks).expect("link");
    assert_eq!(l.target, LinkTarget::Url("https://example.com/a".into()));
    assert_eq!(l.tooltip.as_deref(), Some("Tip"));
    assert!(fields(&r).is_empty());
}

#[test]
fn fields_without_a_separator_and_dirty_locked_flags() {
    let r = read_body(&p(
        "<w:r><w:fldChar w:fldCharType=\"begin\" w:dirty=\"true\" w:fldLock=\"1\"/></w:r><w:r><w:instrText> XE \"Index entry\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
    ));
    let f = &fields(&r)[0];
    assert_eq!(f.instr, "XE \"Index entry\"");
    assert_eq!(f.result, "");
    assert!(f.dirty && f.locked);
}

#[test]
fn unterminated_and_multi_paragraph_fields_warn() {
    let open = read_body(&p(
        "<w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r>",
    ));
    assert!(codes(&open).contains(&"field.unterminated"));
    let multi = read_body(&format!(
        "<w:p><w:r><w:fldChar w:fldCharType=\"begin\"/></w:r><w:r><w:instrText> QUOTE \"x\" </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}</w:p>{}",
        r("one"),
        p(&format!(
            "{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
            r("two")
        ))
    ));
    assert_eq!(texts(&multi).concat(), "onetwo");
    let w = multi
        .warnings
        .iter()
        .find(|w| w.code.as_str() == "field.multiParagraph")
        .expect("warning");
    assert_eq!(w.path, "/w:document/w:body[1]/w:p[1]/w:r[1]/w:fldChar[1]");
}

#[test]
fn pictures_produced_by_a_field_are_kept_and_the_code_dropped() {
    let drawing = "<w:r><w:drawing><wp:inline><wp:extent cx=\"100\" cy=\"100\"/><wp:docPr id=\"1\" name=\"p\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:blipFill><a:blip r:embed=\"rIdImg\"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>";
    let r = read_build(Build {
        body: p(&complex(
            "INCLUDEPICTURE \"x.png\" \\* MERGEFORMATINET",
            drawing,
        )),
        parts: vec![image_part()],
        ..Build::default()
    });
    assert_eq!(drawings(&r).len(), 1);
    assert!(codes(&r).contains(&"field.unwrapped"));
}

#[test]
fn legacy_form_fields_are_converted_with_a_warning() {
    let r = read_body(&p(&format!(
        "<w:r><w:fldChar w:fldCharType=\"begin\"><w:ffData><w:name w:val=\"Text1\"/><w:enabled/></w:ffData></w:fldChar></w:r><w:r><w:instrText> FORMTEXT </w:instrText></w:r><w:r><w:fldChar w:fldCharType=\"separate\"/></w:r>{}<w:r><w:fldChar w:fldCharType=\"end\"/></w:r>",
        r("value")
    )));
    assert_eq!(fields(&r)[0].result, "value");
    assert!(codes(&r).contains(&"field.formField"));
}

#[test]
fn pg_num_is_a_page_field_and_simple_fields_are_simple() {
    let r = read_body(&p(
        "<w:r><w:pgNum/></w:r><w:fldSimple w:instr=\" NUMPAGES \"><w:r><w:t>4</w:t></w:r></w:fldSimple>",
    ));
    let f = fields(&r);
    assert_eq!(f[0].instr, "PAGE");
    assert_eq!(
        (f[1].instr.as_str(), f[1].result.as_str(), f[1].form),
        ("NUMPAGES", "4", kiln_doc::fields::FieldForm::Simple)
    );
}

// ------------------------------------------------------------------ content controls

#[test]
fn block_content_controls_wrap_their_blocks() {
    let r = read_body(&format!(
        "<w:sdt><w:sdtPr><w:alias w:val=\"Intro\"/><w:tag w:val=\"intro\"/><w:id w:val=\"42\"/><w:docPartObj><w:docPartGallery w:val=\"Table of Contents\"/><w:docPartUnique/></w:docPartObj></w:sdtPr><w:sdtEndPr/><w:sdtContent>{}{}</w:sdtContent></w:sdt>{}",
        p(&r("A")),
        p(&r("B")),
        p(&r("C"))
    ));
    let blocks = &r.document.sections[0].blocks;
    let Block::Sdt(sdt) = &blocks[0] else {
        panic!("sdt block")
    };
    assert_eq!(sdt.blocks.len(), 2);
    let i = &sdt.info;
    assert_eq!(i.kind, SdtKind::DocPartObj);
    assert_eq!(i.id, Some(42));
    assert_eq!(i.tag.as_deref(), Some("intro"));
    assert_eq!(i.alias.as_deref(), Some("Intro"));
    assert!(i.is_toc());
    assert_eq!(i.end_pr_xml.as_deref(), Some("<w:sdtEndPr/>"));
    assert!(i.pr_xml.starts_with("<w:sdtPr>"));
    assert!(matches!(&blocks[1], Block::Paragraph(p) if p.text() == "C"));
}

#[test]
fn inline_content_controls_are_marks() {
    let r = read_body(&p(&format!(
        "<w:sdt><w:sdtPr><w14:checkbox><w14:checked w14:val=\"1\"/></w14:checkbox></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt><w:sdt><w:sdtPr><w:dropDownList><w:listItem w:displayText=\"Yes\" w:value=\"Y\"/><w:listItem w:value=\"N\"/></w:dropDownList></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt><w:sdt><w:sdtPr><w:showingPlcHdr/><w:date><w:dateFormat w:val=\"d/M/yyyy\"/></w:date></w:sdtPr><w:sdtContent>{}</w:sdtContent></w:sdt>",
        r("☒"),
        r("Yes"),
        r("Pick")
    )));
    let sdt = |t: &str| {
        at_text(&r.document, t)
            .1
            .into_iter()
            .find_map(|m| match m {
                Mark::Sdt(s) => Some((**s).clone()),
                _ => None,
            })
            .expect("sdt mark")
    };
    let cb = sdt("☒");
    assert_eq!((cb.kind, cb.checked), (SdtKind::Checkbox, Some(true)));
    let dd = sdt("Yes");
    assert_eq!(
        dd.list_items
            .iter()
            .map(|l| (l.display_text.as_str(), l.value.as_str()))
            .collect::<Vec<_>>(),
        [("Yes", "Y"), ("N", "N")]
    );
    let date = sdt("Pick");
    assert_eq!(date.kind, SdtKind::Date);
    assert_eq!(date.date_format.as_deref(), Some("d/M/yyyy"));
    assert!(date.showing_placeholder);
}

#[test]
fn row_level_content_controls_are_unwrapped_into_the_extras() {
    let r = read_body(&format!(
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"1000\"/></w:tblGrid><w:sdt><w:sdtPr><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent><w:tr><w:tc>{}</w:tc></w:tr></w:sdtContent></w:sdt></w:tbl>",
        p(&r("row"))
    ));
    let Block::Table(t) = &r.document.sections[0].blocks[0] else {
        panic!("table")
    };
    assert_eq!(t.rows.len(), 1);
    let x = &r.extras.rows[&t.rows[0].id];
    assert_eq!(x.sdt[0].kind, SdtKind::RepeatingSectionItem);
}

// ------------------------------------------------------------------ tables

fn tbl(grid: &[u32], rows: &str, tbl_pr: &str) -> String {
    let cols: String = grid
        .iter()
        .map(|g| format!("<w:gridCol w:w=\"{g}\"/>"))
        .collect();
    format!("<w:tbl><w:tblPr>{tbl_pr}</w:tblPr><w:tblGrid>{cols}</w:tblGrid>{rows}</w:tbl>")
}

fn tc(text: &str, tc_pr: &str) -> String {
    format!(
        "<w:tc>{}<w:p>{}</w:p></w:tc>",
        if tc_pr.is_empty() {
            String::new()
        } else {
            format!("<w:tcPr>{tc_pr}</w:tcPr>")
        },
        r(text)
    )
}

fn table(r: &kiln_io_docx::DocxRead) -> &kiln_doc::Table {
    match &r.document.sections[0].blocks[0] {
        Block::Table(t) => t,
        other => panic!("not a table: {other:?}"),
    }
}

#[test]
fn widths_in_every_unit_notation() {
    let r = read_body(&tbl(
        &[1440, 1440, 1440, 1440],
        &format!(
            "<w:tr>{}{}{}{}</w:tr>",
            tc("a", "<w:tcW w:w=\"1440\" w:type=\"dxa\"/>"),
            tc("b", "<w:tcW w:w=\"2500\" w:type=\"pct\"/>"),
            tc("c", "<w:tcW w:w=\"50%\" w:type=\"pct\"/>"),
            tc("d", "<w:tcW w:w=\"1in\" w:type=\"dxa\"/>")
        ),
        "<w:tblW w:w=\"0\" w:type=\"auto\"/><w:tblInd w:w=\"0.5in\" w:type=\"dxa\"/><w:tblCellMar><w:start w:w=\"108\" w:type=\"dxa\"/><w:end w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar>",
    ));
    let t = table(&r);
    let widths: Vec<_> = t.rows[0].cells.iter().map(|c| c.props.width).collect();
    assert_eq!(
        widths,
        [
            Some(TableWidth::dxa(1440)),
            Some(TableWidth::pct(2500)),
            Some(TableWidth::pct(2500)),
            Some(TableWidth::dxa(1440))
        ]
    );
    assert_eq!(
        t.props.width,
        Some(TableWidth {
            kind: WidthType::Auto,
            value: 0
        })
    );
    assert_eq!(t.props.indent, Some(720));
    let m = t.props.cell_margins.expect("margins");
    assert_eq!((m.left, m.right, m.top), (Some(108), Some(108), None));
}

#[test]
fn hex_table_look_and_nil_widths() {
    let r = read_body(&tbl(
        &[100],
        &format!(
            "<w:tr>{}</w:tr>",
            tc("x", "<w:tcW w:w=\"0\" w:type=\"nil\"/>")
        ),
        "<w:tblLook w:val=\"04A0\"/>",
    ));
    let t = table(&r);
    let look = t.props.look.expect("look");
    assert!(look.first_row && look.first_column && look.no_v_band);
    assert!(!look.last_row && !look.last_column && !look.no_h_band);
    assert_eq!(
        t.rows[0].cells[0].props.width,
        Some(TableWidth {
            kind: WidthType::Nil,
            value: 0
        })
    );
}

#[test]
fn horizontally_and_vertically_merged_blocks() {
    let r = read_body(&tbl(
        &[1000, 1000, 1000],
        &format!(
            "<w:tr>{}{}</w:tr><w:tr>{}{}</w:tr><w:tr>{}{}{}</w:tr>",
            tc(
                "A",
                "<w:gridSpan w:val=\"2\"/><w:vMerge w:val=\"restart\"/>"
            ),
            tc("B", ""),
            tc("", "<w:gridSpan w:val=\"2\"/><w:vMerge/>"),
            tc("C", ""),
            tc("D", ""),
            tc("E", ""),
            tc("F", "")
        ),
        "",
    ));
    let t = table(&r);
    assert_eq!((t.rows.len(), t.grid_width()), (3, 3));
    let map = t.cell_map();
    let a = map.iter().find(|s| s.row == 0 && s.cell == 0).expect("A");
    assert_eq!((a.col_span, a.row_span), (2, 2));
    assert_eq!(t.rows[1].cells[0].v_merge, Some(VMerge::Continue));
}

#[test]
fn a_vertical_continuation_without_an_origin_is_an_ordinary_cell() {
    let r = read_body(&tbl(
        &[1000],
        &format!("<w:tr>{}</w:tr>", tc("orphan", "<w:vMerge/>")),
        "",
    ));
    let t = table(&r);
    assert_eq!(t.rows[0].cells[0].v_merge, None);
    assert_eq!(texts(&r), ["orphan"]);
}

#[test]
fn legacy_h_merge_cells_are_merged() {
    let r = read_body(&tbl(
        &[1000, 1000],
        &format!(
            "<w:tr>{}{}</w:tr>",
            tc("wide", "<w:hMerge w:val=\"restart\"/>"),
            tc("", "<w:hMerge/>")
        ),
        "",
    ));
    let t = table(&r);
    assert_eq!(t.rows[0].cells.len(), 1);
    assert_eq!(t.rows[0].cells[0].grid_span, 2);
}

#[test]
fn grid_before_and_ragged_rows_cover_the_grid() {
    let r = read_body(&tbl(
        &[1000, 1000, 1000],
        &format!(
            "<w:tr><w:trPr><w:gridBefore w:val=\"1\"/></w:trPr>{}{}</w:tr><w:tr>{}</w:tr>",
            tc("b", ""),
            tc("c", ""),
            tc("only", "")
        ),
        "",
    ));
    let t = table(&r);
    assert_eq!(t.rows[0].props.grid_before, Some(1));
    assert_eq!(t.rows[1].props.grid_after, Some(2));
    let w = r
        .warnings
        .iter()
        .find(|w| w.code.as_str() == "table.irregular")
        .expect("warning");
    assert_eq!(w.severity, Severity::Info);
}

#[test]
fn row_and_cell_properties() {
    let r = read_body(&tbl(
        &[1000],
        &format!(
            "<w:tr><w:trPr><w:trHeight w:val=\"400\" w:hRule=\"exact\"/><w:tblHeader/><w:cantSplit/></w:trPr>{}</w:tr>",
            tc(
                "h",
                "<w:vAlign w:val=\"center\"/><w:textDirection w:val=\"btLr\"/><w:noWrap/><w:tcBorders><w:top w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/></w:tcBorders>"
            )
        ),
        "",
    ));
    let t = table(&r);
    let row = &t.rows[0].props;
    let h = row.height.expect("height");
    assert_eq!((h.value, h.rule), (400, kiln_doc::st::HeightRule::Exact));
    assert_eq!((row.header, row.cant_split), (Some(true), Some(true)));
    let c = &t.rows[0].cells[0].props;
    assert_eq!(c.v_align, Some(kiln_doc::st::CellVAlign::Center));
    assert_eq!(c.text_direction, Some(TextDirection::BtLr));
    assert_eq!(c.no_wrap, Some(true));
    let top = c.borders.as_ref().and_then(|b| b.top.clone()).expect("top");
    assert_eq!(top.size, Some(4));
    assert_eq!(top.color, Some(DocColor::auto()));
}

#[test]
fn empty_cells_get_an_empty_paragraph() {
    let r = read_body(&tbl(&[1000], "<w:tr><w:tc><w:tcPr/></w:tc></w:tr>", ""));
    assert_eq!(table(&r).rows[0].cells[0].blocks.len(), 1);
}

#[test]
fn floating_tables_and_unmodelled_table_properties() {
    let r = read_body(&tbl(
        &[1000],
        &format!("<w:tr>{}</w:tr>", tc("x", "")),
        "<w:tblpPr w:leftFromText=\"180\" w:vertAnchor=\"page\" w:tblpX=\"2000\" w:tblpY=\"3000\"/><w:tblOverlap w:val=\"never\"/><w:tblPrChange w:id=\"1\" w:author=\"A\"><w:tblPr/></w:tblPrChange>",
    ));
    let t = table(&r);
    let pos = t.props.position.clone().expect("position");
    assert_eq!(
        (pos.x, pos.y, pos.left_from_text),
        (Some(2000), Some(3000), Some(180))
    );
    assert_eq!(pos.vert_anchor.as_deref(), Some("page"));
    assert_eq!(t.props.allow_overlap, Some(false));
    assert!(r.extras.tables[&t.id][0].starts_with("<w:tblPrChange"));
}

// ------------------------------------------------------------------ drawings

fn inline(graphic: &str, doc_pr: &str) -> String {
    format!(
        "<w:r><w:drawing><wp:inline><wp:extent cx=\"914400\" cy=\"457200\"/>{doc_pr}<a:graphic>{graphic}</a:graphic></wp:inline></w:drawing></w:r>"
    )
}

const DOC_PR: &str = "<wp:docPr id=\"1\" name=\"Picture 1\" descr=\"A chart\" title=\"T\"/>";

fn pic_graphic(blip: &str) -> String {
    format!(
        "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:blipFill>{blip}<a:srcRect l=\"10000\" r=\"5000\"/></pic:blipFill><pic:spPr><a:xfrm rot=\"5400000\" flipH=\"1\"/></pic:spPr></pic:pic></a:graphicData>"
    )
}

#[test]
fn pictures_with_alt_text_crop_and_rotation() {
    let r = read_build(Build {
        body: p(&inline(
            &pic_graphic("<a:blip r:embed=\"rIdImg\"/>"),
            DOC_PR,
        )),
        parts: vec![image_part()],
        ..Build::default()
    });
    let d = &drawings(&r)[0];
    assert_eq!(
        d.kind,
        DrawingKind::Picture {
            media: "/word/media/image1.png".into(),
            crop: Some(Crop {
                left: 10_000,
                top: 0,
                right: 5_000,
                bottom: 0
            })
        }
    );
    assert_eq!(d.description.as_deref(), Some("A chart"));
    assert_eq!(d.title.as_deref(), Some("T"));
    assert_eq!(d.name, "Picture 1");
    assert_eq!((d.width, d.height), (914_400, 457_200));
    assert_eq!(d.rotation, 5_400_000);
    assert!(d.flip_h && !d.flip_v);
    assert_eq!(
        r.document.media["/word/media/image1.png"].content_type,
        "image/png"
    );
}

#[test]
fn decorative_pictures() {
    let doc_pr = "<wp:docPr id=\"2\" name=\"d\"><a:extLst><a:ext uri=\"{C183D7F6-B498-43B3-948B-1728B52AA6E4}\"><adec:decorative xmlns:adec=\"http://schemas.microsoft.com/office/drawing/2017/decorative\" val=\"1\"/></a:ext></a:extLst></wp:docPr>";
    let r = read_build(Build {
        body: p(&inline(
            &pic_graphic("<a:blip r:embed=\"rIdImg\"/>"),
            doc_pr,
        )),
        parts: vec![image_part()],
        ..Build::default()
    });
    assert!(drawings(&r)[0].decorative);
}

#[test]
fn linked_and_missing_pictures_warn() {
    let linked = read_build(Build {
        body: p(&inline(
            &pic_graphic("<a:blip r:link=\"rIdLink\"/>"),
            DOC_PR,
        )),
        rels: vec![(
            "rIdLink",
            rel_type::IMAGE,
            "https://example.com/x.png",
            true,
        )],
        ..Build::default()
    });
    assert!(codes(&linked).contains(&"image.linked"));
    assert_eq!(drawings(&linked)[0].kind, DrawingKind::Other);
    let missing = read_body(&p(&inline(
        &pic_graphic("<a:blip r:embed=\"rIdNope\"/>"),
        DOC_PR,
    )));
    assert!(codes(&missing).contains(&"image.missing"));
    let d = &drawings(&missing)[0];
    assert_eq!(d.kind, DrawingKind::Other);
    assert!(d.xml.as_deref().is_some_and(|x| x.contains("rIdNope")));
}

#[test]
fn charts_are_chart_drawings_and_the_chart_part_is_kept() {
    let chart = ExtraPart {
        name: "/word/charts/chart1.xml",
        content_type: "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        rel_type: Some(rel_type::CHART),
        rel_id: Some("rIdChart"),
        content:
            b"<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"/>"
                .to_vec(),
    };
    let r = read_build(Build {
        body: p(&inline(
            "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart r:id=\"rIdChart\"/></a:graphicData>",
            DOC_PR,
        )),
        parts: vec![chart],
        ..Build::default()
    });
    let d = &drawings(&r)[0];
    assert_eq!(
        d.kind,
        DrawingKind::Chart {
            part: "/word/charts/chart1.xml".into()
        }
    );
    assert_eq!(r.charts.len(), 1);
    let c = &r.charts[0];
    assert_eq!(
        (
            c.source_part.as_str(),
            c.rel_id.as_str(),
            c.chart_part.as_deref(),
            c.width,
            c.height,
            c.anchored,
            c.drawing
        ),
        (
            "/word/document.xml",
            "rIdChart",
            Some("/word/charts/chart1.xml"),
            914_400,
            457_200,
            false,
            d.id
        )
    );
    assert!(r.passthrough.has("/word/charts/chart1.xml"));
    assert_eq!(
        r.relationships["/word/document.xml"]["rIdChart"].target,
        "/word/charts/chart1.xml"
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
}

#[test]
fn smart_art_and_groups_keep_their_xml_with_info_warnings() {
    let r = read_body(&p(&(inline(
        "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/>",
        DOC_PR,
    ) + &inline(
        "<a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingGroup\"/>",
        DOC_PR,
    ))));
    let kinds: Vec<_> = drawings(&r).into_iter().map(|d| d.kind).collect();
    assert_eq!(kinds, [DrawingKind::Other, DrawingKind::Group]);
    let w: Vec<_> = r
        .warnings
        .iter()
        .map(|w| (w.code.as_str(), w.severity))
        .collect();
    assert_eq!(
        w,
        [
            ("drawing.smartArt", Severity::Info),
            ("drawing.group", Severity::Info)
        ]
    );
}

#[test]
fn the_wps_choice_of_alternate_content_is_read_and_the_whole_block_kept() {
    let wsp = "<a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:spPr><a:prstGeom prst=\"ellipse\"/><a:noFill/></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>inside</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData>";
    let r = read_body(&p(&format!(
        "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:inline><wp:extent cx=\"10\" cy=\"20\"/><wp:docPr id=\"3\" name=\"s\"/><a:graphic>{wsp}</a:graphic></wp:inline></w:drawing></mc:Choice><mc:Fallback><w:pict><v:oval/></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    )));
    let d = &drawings(&r)[0];
    assert_eq!(
        d.kind,
        DrawingKind::Shape {
            preset: "ellipse".into()
        }
    );
    let xml = d.xml.as_deref().expect("xml");
    assert!(xml.starts_with("<mc:AlternateContent"));
    assert!(xml.contains("<mc:Fallback>"));
    let tb = d.text_box.as_ref().expect("text box");
    assert_eq!(
        tb[0].as_paragraph().map(Paragraph::text).as_deref(),
        Some("inside")
    );
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    // Text box paragraphs are part of the model (reachable by id) but not body text.
    assert_eq!(
        r.document.body_paragraphs().len(),
        2,
        "the body paragraph and the text box paragraph"
    );
}

#[test]
fn anchors_with_alignment_simple_pos_and_one_sided_wrapping() {
    let anchor = |attrs: &str, inner: &str| {
        format!(
            "<w:r><w:drawing><wp:anchor {attrs}>{inner}<wp:extent cx=\"1\" cy=\"1\"/><wp:docPr id=\"9\" name=\"a\"/><a:graphic>{}</a:graphic></wp:anchor></w:drawing></w:r>",
            pic_graphic("<a:blip r:embed=\"rIdImg\"/>")
        )
    };
    let r = read_build(Build {
        body: p(&(anchor(
            "behindDoc=\"0\" relativeHeight=\"5\" simplePos=\"0\"",
            "<wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"margin\"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom=\"page\"><wp:align>top</wp:align></wp:positionV><wp:wrapSquare wrapText=\"left\"/>",
        ) + &anchor(
            "behindDoc=\"1\" simplePos=\"1\"",
            "<wp:simplePos x=\"12700\" y=\"25400\"/><wp:positionH relativeFrom=\"column\"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:wrapNone/>",
        ) + &anchor(
            "behindDoc=\"0\"",
            "<wp:positionH relativeFrom=\"page\"><wp14:pctPosHOffset>40000</wp14:pctPosHOffset></wp:positionH><wp:positionV relativeFrom=\"page\"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:wrapTight wrapText=\"bothSides\"><wp:wrapPolygon edited=\"0\"><wp:start x=\"0\" y=\"0\"/></wp:wrapPolygon></wp:wrapTight>",
        ))),
        parts: vec![image_part()],
        ..Build::default()
    });
    let d = drawings(&r);
    let anchor_of = |i: usize| match &d[i].placement {
        Placement::Anchor(a) => (**a).clone(),
        Placement::Inline => panic!("anchored"),
    };
    let a = anchor_of(0);
    assert_eq!(a.relative_h, RelativeFromH::Margin);
    assert_eq!(a.h, PosH::Align(kiln_doc::st::AlignH::Center));
    assert_eq!(a.relative_v, RelativeFromV::Page);
    assert_eq!(a.v, PosV::Align(kiln_doc::st::AlignV::Top));
    assert_eq!(a.z_order, 5);
    assert_eq!(
        (a.wrap, a.wrap_side),
        (WrapType::Square, Some(WrapSide::Left))
    );
    let b = anchor_of(1);
    assert_eq!(b.wrap, WrapType::Behind);
    assert_eq!(
        (b.relative_h, b.h, b.v),
        (
            RelativeFromH::Page,
            PosH::Offset(12_700),
            PosV::Offset(25_400)
        )
    );
    let c = anchor_of(2);
    assert_eq!(c.wrap, WrapType::Tight);
    assert!(
        c.wrap_polygon_xml
            .as_deref()
            .is_some_and(|x| x.starts_with("<wp:wrapPolygon"))
    );
    assert!(codes(&r).contains(&"drawing.unsupported"));
}

#[test]
fn vml_pictures_are_pictures_and_vml_text_boxes_keep_their_text() {
    let r = read_build(Build {
        body: p(
            "<w:r><w:pict><v:shape style=\"width:72pt;height:0.5in\"><v:imagedata r:id=\"rIdImg\" o:title=\"Logo\"/></v:shape></w:pict></w:r><w:r><w:pict><v:rect style=\"width:100pt;height:50pt\"><v:textbox><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r><w:r><w:pict><v:line/></w:pict></w:r>",
        ),
        parts: vec![image_part()],
        ..Build::default()
    });
    let d = drawings(&r);
    assert_eq!(
        d[0].kind,
        DrawingKind::Picture {
            media: "/word/media/image1.png".into(),
            crop: None
        }
    );
    assert_eq!((d[0].width, d[0].height), (914_400, 457_200));
    assert_eq!(d[0].title.as_deref(), Some("Logo"));
    let tb = d[1].text_box.as_ref().expect("text box");
    assert_eq!(
        tb[0].as_paragraph().map(Paragraph::text).as_deref(),
        Some("Boxed")
    );
    assert_eq!((d[1].width, d[1].height), (1_270_000, 635_000));
    assert!(
        objects(&r.document)
            .iter()
            .any(|o| matches!(o, InlineObject::Opaque { label, .. } if label == "Drawing"))
    );
    assert!(r.warnings.iter().all(|w| w.code.as_str() == "drawing.vml"));
}

#[test]
fn embedded_ole_objects_are_opaque_and_their_parts_kept() {
    let r = read_build(Build {
        body: p(
            "<w:r><w:object><v:shape style=\"width:10pt;height:10pt\"><v:imagedata r:id=\"rIdImg\"/></v:shape><o:OLEObject Type=\"Embed\" ProgID=\"Excel.Sheet.12\" ShapeID=\"_x0000_i1025\" DrawAspect=\"Content\" ObjectID=\"_1\" r:id=\"rIdOle\"/></w:object></w:r>",
        ),
        parts: vec![
            image_part(),
            ExtraPart {
                name: "/word/embeddings/Microsoft_Excel_Worksheet.xlsx",
                content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
                rel_type: Some(rel_type::PACKAGE),
                rel_id: Some("rIdOle"),
                content: vec![1, 2, 3],
            },
        ],
        ..Build::default()
    });
    assert!(objects(&r.document).iter().any(
        |o| matches!(o, InlineObject::Opaque { label, .. } if label == "Embedded Excel.Sheet.12")
    ));
    assert_eq!(r.warnings[0].code.as_str(), "object.embedded");
    assert_eq!(r.warnings[0].severity, Severity::Info);
    assert!(
        r.passthrough
            .has("/word/embeddings/Microsoft_Excel_Worksheet.xlsx")
    );
    assert_eq!(
        r.relationships["/word/document.xml"]["rIdOle"].rel_type,
        rel_type::PACKAGE
    );
}

// ------------------------------------------------------------------ equations

#[test]
fn equations_keep_their_omml() {
    let r = read_body(
        &(p(&format!(
            "{}<m:oMath><m:r><m:t>mc</m:t></m:r><m:sSup><m:e><m:r><m:t>2</m:t></m:r></m:e></m:sSup></m:oMath>",
            r("E = ")
        )) + &p("<m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>")),
    );
    let eqs: Vec<_> = objects(&r.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::Equation(e) => Some(e.clone()),
            _ => None,
        })
        .collect();
    assert!(!eqs[0].display);
    assert_eq!(
        eqs[0].omml,
        "<m:oMath><m:r><m:t>mc</m:t></m:r><m:sSup><m:e><m:r><m:t>2</m:t></m:r></m:e></m:sSup></m:oMath>"
    );
    assert_eq!(parse_fragment(&eqs[0].omml).expect("omml").name, "m:oMath");
    assert!(eqs[1].display);
    assert!(eqs[1].omml.starts_with("<m:oMathPara"));
}

// ------------------------------------------------------------------ comments

fn comments_part(inner: &str) -> ExtraPart {
    ExtraPart {
        name: "/word/comments.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.comments+xml",
        rel_type: Some(rel_type::COMMENTS),
        rel_id: None,
        content: part("w:comments", inner).into_bytes(),
    }
}

#[test]
fn comment_threads_resolved_state_and_people() {
    let r = read_build(Build {
        body: p(&format!(
            "<w:commentRangeStart w:id=\"1\"/>{}<w:commentRangeEnd w:id=\"1\"/><w:r><w:commentReference w:id=\"1\"/></w:r><w:r><w:commentReference w:id=\"2\"/></w:r>",
            r("discussed")
        )),
        parts: vec![
            comments_part(
                "<w:comment w:id=\"1\" w:author=\"Ann\" w:date=\"2026-01-01T00:00:00Z\"><w:p w14:paraId=\"00000001\"><w:r><w:t>Why?</w:t></w:r></w:p></w:comment><w:comment w:id=\"2\" w:author=\"Bo\"><w:p w14:paraId=\"00000002\"><w:r><w:t>Because.</w:t></w:r></w:p></w:comment>",
            ),
            ExtraPart {
                name: "/word/commentsExtended.xml",
                content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.commentsExtended+xml",
                rel_type: Some(rel_type::COMMENTS_EXTENDED),
                rel_id: None,
                content: part(
                    "w15:commentsEx",
                    "<w15:commentEx w15:paraId=\"00000001\" w15:done=\"1\"/><w15:commentEx w15:paraId=\"00000002\" w15:paraIdParent=\"00000001\" w15:done=\"0\"/>",
                )
                .into_bytes(),
            },
            ExtraPart {
                name: "/word/people.xml",
                content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.people+xml",
                rel_type: Some(rel_type::PEOPLE),
                rel_id: None,
                content: part(
                    "w15:people",
                    "<w15:person w15:author=\"Ann\"><w15:presenceInfo w15:providerId=\"AD\" w15:userId=\"S-1\"/></w15:person>",
                )
                .into_bytes(),
            },
        ],
        ..Build::default()
    });
    let c = &r.document.comments;
    let text = |i: usize| -> String {
        c[i].blocks
            .iter()
            .filter_map(Block::as_paragraph)
            .map(Paragraph::text)
            .collect()
    };
    assert_eq!(
        (c[0].meta.author.as_str(), text(0).as_str()),
        ("Ann", "Why?")
    );
    assert!(c[0].meta.resolved && c[0].meta.parent.is_none());
    assert_eq!(
        (c[1].meta.author.as_str(), text(1).as_str()),
        ("Bo", "Because.")
    );
    assert_eq!(c[1].meta.parent, Some(c[0].id));
    assert!(!c[1].meta.resolved);
    let people = &r.extras.people;
    assert_eq!(people.len(), 1);
    assert_eq!(
        (
            people[0].author.as_str(),
            people[0].provider_id.as_deref(),
            people[0].user_id.as_deref()
        ),
        ("Ann", Some("AD"), Some("S-1"))
    );
    // The reply covers no text of its own (Word anchors replies to the parent's range).
    let w: Vec<_> = r.warnings.iter().map(|w| w.code.as_str()).collect();
    assert_eq!(w, ["comment.unanchored"]);
    assert!(r.warnings[0].message.contains("Comment 2"));
    assert_eq!(r.document.comment_threads().len(), 1);
}

#[test]
fn comment_ranges_span_paragraphs_and_block_level_markers() {
    let r = read_build(Build {
        body: format!(
            "<w:commentRangeStart w:id=\"7\"/>{}{}{}",
            p(&r("first")),
            p(&format!("{}<w:commentRangeEnd w:id=\"7\"/>", r("second"))),
            p(&r("third"))
        ),
        parts: vec![comments_part(
            "<w:comment w:id=\"7\" w:author=\"A\"><w:p/></w:comment>",
        )],
        ..Build::default()
    });
    let id = r.document.comments[0].id;
    let has = |t: &str| at_text(&r.document, t).1.contains(&&Mark::Comment(id));
    assert!(has("first") && has("second") && !has("third"));
}

#[test]
fn references_to_missing_comments_warn() {
    let r = read_body(&p("<w:r><w:commentReference w:id=\"99\"/></w:r>"));
    assert!(codes(&r).contains(&"comment.missing"));
}

// ------------------------------------------------------------------ notes, bookmarks, links

#[test]
fn custom_note_marks_and_missing_notes() {
    let notes = ExtraPart {
        name: "/word/footnotes.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml",
        rel_type: Some(rel_type::FOOTNOTES),
        rel_id: None,
        content: part(
            "w:footnotes",
            "<w:footnote w:type=\"separator\" w:id=\"-1\"><w:p><w:r><w:separator/></w:r></w:p></w:footnote><w:footnote w:id=\"4\"><w:p><w:r><w:t>Note four</w:t></w:r></w:p></w:footnote>",
        )
        .into_bytes(),
    };
    let r = read_build(Build {
        body: p(
            "<w:r><w:footnoteReference w:customMarkFollows=\"1\" w:id=\"4\"/><w:t>*</w:t></w:r><w:r><w:footnoteReference w:id=\"5\"/></w:r>",
        ),
        parts: vec![notes],
        ..Build::default()
    });
    let refs: Vec<_> = objects(&r.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::NoteRef(n) => Some(n.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].custom_mark.as_deref(), Some("*"));
    assert_eq!(r.extras.note_ids[&refs[0].note], "4");
    assert!(codes(&r).contains(&"note.missing"));
    assert_eq!(r.document.notes.len(), 2);
}

#[test]
fn block_level_bookmarks_go_to_the_next_paragraph_and_trailing_ones_to_the_last() {
    let r = read_body(&format!(
        "<w:bookmarkStart w:id=\"1\" w:name=\"b\"/>{}<w:bookmarkEnd w:id=\"1\"/>",
        p(&r("x"))
    ));
    let p = first_paragraph(&r);
    assert_eq!(p.content.raw_text(), "\u{FFFC}x\u{FFFC}");
    assert!(matches!(&p.content.objects()[0], InlineObject::BookmarkStart { name } if name == "b"));
    assert!(matches!(&p.content.objects()[1], InlineObject::BookmarkEnd { name } if name == "b"));
}

#[test]
fn hyperlink_relationships_tooltips_and_frames() {
    let r = read_build(Build {
        body: p(&format!(
            "<w:hyperlink r:id=\"rIdL\" w:tooltip=\"Go\" w:tgtFrame=\"_blank\" w:history=\"1\">{}</w:hyperlink>",
            r("ext")
        )),
        rels: vec![(
            "rIdL",
            rel_type::HYPERLINK,
            "https://example.org/p?q=1",
            true,
        )],
        ..Build::default()
    });
    let l = links(&at_text(&r.document, "ext").1).expect("link");
    assert_eq!(
        l.target,
        LinkTarget::Url("https://example.org/p?q=1".into())
    );
    assert_eq!(l.tooltip.as_deref(), Some("Go"));
    assert_eq!(l.frame.as_deref(), Some("_blank"));
    assert!(l.history);
    assert!(r.passthrough.relationships().is_empty());
}

#[test]
fn smart_tags_and_custom_xml_unwrap_and_permissions_warn() {
    let r = read_body(&format!(
        "{}<w:customXml w:element=\"blk\"><w:p><w:r><w:t>Block</w:t></w:r></w:p></w:customXml>",
        p(&format!(
            "<w:smartTag w:uri=\"urn:x\" w:element=\"place\"><w:smartTagPr/>{}</w:smartTag><w:customXml w:element=\"e\">{}</w:customXml><w:permStart w:id=\"1\" w:edGrp=\"everyone\"/>{}<w:permEnd w:id=\"1\"/>",
            r("Paris"),
            r(" Rome"),
            r("!")
        ))
    ));
    assert_eq!(texts(&r), ["Paris Rome!", "Block"]);
    assert!(codes(&r).contains(&"permission"));
}

#[test]
fn alt_chunks_are_opaque_blocks() {
    let r = read_body(&format!("{}<w:altChunk r:id=\"rIdAlt\"/>", p(&r("a"))));
    assert!(r.document.sections[0].blocks.iter().any(
        |b| matches!(b, Block::Opaque(o) if o.label == "Imported content" && o.xml.contains("rIdAlt"))
    ));
    assert!(codes(&r).contains(&"altChunk"));
}

#[test]
fn unknown_elements_are_reported_with_their_location() {
    let r = read_body(&p("<w:r><w:dayLong/></w:r>"));
    let w = &r.warnings[0];
    assert_eq!(w.code.as_str(), "element.unknown");
    assert_eq!(w.part, "/word/document.xml");
    assert_eq!(w.path, "/w:document/w:body[1]/w:p[1]/w:r[1]/w:dayLong[1]");
}

// ------------------------------------------------------------------ sections

fn hdr(name: &'static str, id: &'static str, text: &str, footer: bool) -> ExtraPart {
    let (root, kind, rt) = if footer {
        ("w:ftr", "footer", rel_type::FOOTER)
    } else {
        ("w:hdr", "header", rel_type::HEADER)
    };
    ExtraPart {
        name,
        content_type: if footer {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
        } else {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
        },
        rel_type: Some(rt),
        rel_id: Some(id),
        content: part(
            root,
            &format!("<w:p><w:r><w:t>{text} {kind}</w:t></w:r></w:p>"),
        )
        .into_bytes(),
    }
}

#[test]
fn paragraph_section_breaks_split_the_body_and_header_parts_are_shared() {
    let r = read_build(Build {
        body: format!(
            "{}{}<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH\"/><w:footerReference w:type=\"first\" r:id=\"rIdF\"/><w:titlePg/><w:pgSz w:w=\"15840\" w:h=\"12240\"/><w:vAlign w:val=\"center\"/><w:docGrid w:linePitch=\"360\"/></w:sectPr>",
            p_pr(
                &r("one"),
                "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH\"/><w:type w:val=\"continuous\"/><w:pgSz w:w=\"8.5in\" w:h=\"11in\"/><w:cols w:num=\"3\" w:space=\"0.5in\"/></w:sectPr>"
            ),
            p(&r("two"))
        ),
        parts: vec![
            hdr("/word/header1.xml", "rIdH", "Head", false),
            hdr("/word/footer1.xml", "rIdF", "Foot", true),
        ],
        ..Build::default()
    });
    let doc = &r.document;
    assert_eq!(doc.sections.len(), 2);
    let s0 = &doc.sections[0].props;
    assert_eq!(s0.break_type, SectionBreak::Continuous);
    assert_eq!(
        (
            s0.page_size.width,
            s0.page_size.height,
            s0.page_size.orientation
        ),
        (12240, 15840, Orientation::Portrait)
    );
    assert_eq!(
        (s0.columns.count, s0.columns.space, s0.columns.equal_width),
        (3, 720, true)
    );
    let s1 = &doc.sections[1].props;
    assert!(s1.title_page);
    assert_eq!(s1.v_align, Some(PageVAlign::Center));
    assert_eq!(s1.page_size.orientation, Orientation::Landscape);
    assert_eq!(s1.doc_grid.as_ref().and_then(|g| g.line_pitch), Some(360));
    let h0 = doc.sections[0].headers.get(HeaderFooterKind::Default);
    assert!(h0.is_some());
    assert_eq!(
        h0, doc.sections[1].headers.default,
        "one shared header story"
    );
    assert_eq!(doc.headers_footers.len(), 2);
    let foot = doc.sections[1].footers.first.expect("first-page footer");
    assert_eq!(r.extras.header_footer_parts[&foot], "/word/footer1.xml");
    assert_eq!(
        doc.paragraphs_in(kiln_doc::Story::HeaderFooter(1))[0].text(),
        "Foot footer"
    );
}

#[test]
fn unmodelled_section_properties_are_kept_and_missing_header_parts_warn() {
    let r = read_body(&format!(
        "{}<w:sectPr><w:headerReference w:type=\"even\" r:id=\"rIdGone\"/><w:paperSrc w:first=\"7\"/><w:footnotePr><w:numFmt w:val=\"lowerRoman\"/><w:numRestart w:val=\"eachPage\"/></w:footnotePr><w:rtlGutter/></w:sectPr>",
        p(&r("x"))
    ));
    let s = &r.document.sections[0];
    let x = &r.extras.sections[&s.id];
    assert_eq!(x.xml, ["<w:paperSrc w:first=\"7\"/>"]);
    let fp = x.footnote_pr.as_ref().expect("footnotePr");
    assert_eq!(fp.format, Some(NumberFormat::LowerRoman));
    assert_eq!(fp.restart.as_deref(), Some("eachPage"));
    assert!(x.rtl_gutter);
    assert!(codes(&r).contains(&"part.missing"));
}

#[test]
fn custom_column_widths() {
    let r = read_body(&format!(
        "{}<w:sectPr><w:cols w:num=\"2\" w:equalWidth=\"0\"><w:col w:w=\"3000\" w:space=\"400\"/><w:col w:w=\"5000\"/></w:cols></w:sectPr>",
        p(&r("x"))
    ));
    let c = &r.document.sections[0].props.columns;
    assert_eq!(
        (c.count, c.space, c.equal_width, c.separator),
        (2, 720, false, false)
    );
    assert_eq!(
        c.cols
            .iter()
            .map(|c| (c.width, c.space))
            .collect::<Vec<_>>(),
        [(3000, 400), (5000, 0)]
    );
}

// ------------------------------------------------------------------ styles, numbering, settings

#[test]
fn table_styles_with_conditional_formatting_and_latent_styles() {
    let r = read_build(Build {
        body: p(&r("x")),
        styles: Some(
            "<w:docDefaults><w:rPrDefault><w:rPr><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults><w:latentStyles w:defLockedState=\"0\" w:count=\"376\"><w:lsdException w:name=\"Normal\" w:qFormat=\"1\"/></w:latentStyles><w:style w:type=\"table\" w:styleId=\"Grid\"><w:name w:val=\"Grid\"/><w:tblPr><w:tblBorders><w:insideH w:val=\"single\" w:sz=\"4\"/></w:tblBorders><w:tblStyleRowBandSize w:val=\"1\"/></w:tblPr><w:tblStylePr w:type=\"firstRow\"><w:rPr><w:b/></w:rPr><w:tcPr><w:shd w:val=\"clear\" w:fill=\"4472C4\"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type=\"bogus\"/></w:style><w:style w:type=\"character\" w:customStyle=\"1\" w:styleId=\"Em\"><w:name w:val=\"Em\"/><w:aliases w:val=\"E1, E2\"/><w:rPr><w:i/></w:rPr></w:style>".to_owned(),
        ),
        ..Build::default()
    });
    let styles = &r.document.styles;
    let grid = styles.get("Grid").expect("Grid");
    let inside = grid
        .tbl_pr
        .as_ref()
        .and_then(|t| t.borders.as_ref())
        .and_then(|b| b.inside_h.clone())
        .expect("insideH");
    assert_eq!(inside.size, Some(4));
    assert_eq!(grid.conditions.len(), 1);
    let first = &grid.conditions[&TableCondition::FirstRow];
    assert_eq!(first.rpr.as_ref().and_then(|r| r.bold), Some(true));
    assert_eq!(
        first
            .tc_pr
            .as_ref()
            .and_then(|t| t.shading.as_ref())
            .and_then(|s| s.fill.clone()),
        Some(DocColor::rgb(0x4472C4))
    );
    let em = styles.get("Em").expect("Em");
    assert!(em.custom);
    assert_eq!(em.aliases, ["E1", "E2"]);
    assert_eq!(styles.doc_defaults.rpr.size, Some(22));
    let sp = styles.doc_defaults.ppr.spacing.expect("spacing");
    assert_eq!((sp.after, sp.line), (Some(160), Some(259)));
    assert!(
        r.extras
            .latent_styles
            .as_deref()
            .is_some_and(|l| l.starts_with("<w:latentStyles "))
    );
    assert!(r.extras.styles_xml["Grid"].starts_with("<w:style "));
}

#[test]
fn level_overrides_legal_numbering_and_suffixes() {
    let r = read_build(Build {
        body: p(&r("x")),
        numbering: Some(
            "<w:numPicBullet w:numPicBulletId=\"0\"><w:pict/></w:numPicBullet><w:abstractNum w:abstractNumId=\"1\"><w:nsid w:val=\"ABCD1234\"/><w:multiLevelType w:val=\"multilevel\"/><w:numStyleLink w:val=\"MyList\"/><w:lvl w:ilvl=\"0\"><w:start w:val=\"3\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/><w:isLgl/><w:suff w:val=\"space\"/><w:lvlJc w:val=\"end\"/></w:lvl><w:lvl w:ilvl=\"1\"><w:numFmt w:val=\"custom\" w:format=\"001, 002, 003, ...\"/><w:lvlText w:null=\"1\"/><w:lvlRestart w:val=\"0\"/></w:lvl></w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:lvl w:ilvl=\"0\"><w:start w:val=\"10\"/><w:numFmt w:val=\"upperLetter\"/><w:lvlText w:val=\"%1)\"/></w:lvl></w:lvlOverride></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"99\"/></w:num>".to_owned(),
        ),
        ..Build::default()
    });
    let n = &r.document.numbering;
    let abs = &n.abstracts[&1];
    assert_eq!(abs.nsid.as_deref(), Some("ABCD1234"));
    assert_eq!(
        abs.multi_level_type,
        Some(kiln_doc::st::MultiLevelType::Multilevel)
    );
    assert_eq!(abs.num_style_link.as_deref(), Some("MyList"));
    let l0 = &abs.levels[0];
    assert_eq!(l0.start, 3);
    assert!(l0.is_legal);
    assert_eq!(l0.suffix, kiln_doc::st::LevelSuffix::Space);
    assert_eq!(l0.alignment, kiln_doc::st::LevelAlignment::Right);
    let l1 = &abs.levels[1];
    assert_eq!(
        (
            l1.start,
            &l1.format,
            l1.custom_format.as_deref(),
            l1.text.as_str(),
            l1.restart_after
        ),
        (
            0,
            &NumberFormat::Custom,
            Some("001, 002, 003, ..."),
            "",
            Some(0)
        )
    );
    let def = n.instances[&1].overrides[0]
        .definition
        .as_ref()
        .expect("definition");
    assert_eq!(
        (def.start, &def.format, def.text.as_str()),
        (10, &NumberFormat::UpperLetter, "%1)")
    );
    assert!(!n.instances.contains_key(&2));
    assert_eq!(r.extras.num_pic_bullets.len(), 1);
}

#[test]
fn settings_are_read() {
    let r = read_build(Build {
        body: p(&r("x")),
        settings: Some(
            "<w:trackRevisions/><w:doNotTrackMoves/><w:defaultTabStop w:val=\"709\"/><w:evenAndOddHeaders/><w:mirrorMargins w:val=\"0\"/><w:updateFields w:val=\"true\"/><w:documentProtection w:edit=\"readOnly\" w:enforcement=\"1\"/><w:footnotePr><w:numFmt w:val=\"chicago\"/></w:footnotePr><w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"14\"/></w:compat><w:themeFontLang w:val=\"de-DE\" w:bidi=\"ar-SA\"/><w:clrSchemeMapping w:bg1=\"light1\" w:t1=\"dark1\" w:bg2=\"dark2\" w:t2=\"light2\"/><w:decimalSymbol w:val=\",\"/><w:listSeparator w:val=\";\"/>".to_owned(),
        ),
        ..Build::default()
    });
    let s = &r.document.settings;
    assert!(s.track_revisions && s.even_and_odd_headers && !s.mirror_margins && s.update_fields);
    assert_eq!(s.default_tab_stop, 709);
    assert_eq!(s.footnotes.format, Some(NumberFormat::Chicago));
    assert_eq!(s.compatibility_mode, 14);
    assert_eq!(s.color_scheme_mapping.bg2, ThemeColorName::Dark2);
    assert_eq!(s.decimal_symbol.as_deref(), Some(","));
    assert_eq!(s.list_separator.as_deref(), Some(";"));
    let x = &r.extras.settings;
    assert_eq!(x.do_not_track_moves, Some(true));
    let prot = x.protection.as_ref().expect("protection");
    assert_eq!(
        (prot.edit.as_deref(), prot.enforced),
        (Some("readOnly"), true)
    );
    let lang = x.theme_font_lang.as_ref().expect("lang");
    assert_eq!(
        (lang.val.as_deref(), lang.bidi.as_deref()),
        (Some("de-DE"), Some("ar-SA"))
    );
    assert!(
        r.extras
            .settings_xml
            .as_deref()
            .is_some_and(|x| x.starts_with("<w:settings "))
    );
}

#[test]
fn a_compat_block_without_compat_setting_is_word_2007() {
    let r = read_build(Build {
        body: p(&r("x")),
        settings: Some("<w:compat><w:useFELayout/></w:compat>".to_owned()),
        ..Build::default()
    });
    assert_eq!(r.document.settings.compatibility_mode, 12);
}

#[test]
fn page_background_colour() {
    let bytes = build_docx(Build {
        body: p(&r("x")),
        ..Build::default()
    });
    let mut pkg = OpcPackage::open(&bytes).expect("open");
    let xml = String::from_utf8(
        pkg.part("/word/document.xml")
            .expect("read")
            .expect("part")
            .into_owned(),
    )
    .expect("utf8")
    .replace("<w:body>", "<w:background w:color=\"FFF2CC\"/><w:body>");
    pkg.set_part("/word/document.xml", xml).expect("set");
    let r = read_docx(&pkg.save().expect("save")).expect("reads");
    assert_eq!(
        r.extras.settings.background_color.as_deref(),
        Some("FFF2CC")
    );
    assert_eq!(
        r.extras.background.as_deref(),
        Some("<w:background w:color=\"FFF2CC\"/>")
    );
}

// ------------------------------------------------------------------ prefixes, ids, extras

#[test]
fn default_namespace_and_unprefixed_attributes_are_accepted() {
    let f = find("foreign-prefixes.docx")
        .and_then(|f| f.read())
        .expect("fixture");
    let r = read_docx(&f).expect("reads");
    r.document.validate().expect("valid");
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.document.comments.len(), 1);
    let (_, marks) = at_text(&r.document, "commented");
    assert!(marks.contains(&&Mark::Comment(r.document.comments[0].id)));
}

#[test]
fn ids_are_unique_and_the_allocator_is_past_them() {
    let f = find("comments.docx")
        .and_then(|f| f.read())
        .expect("fixture");
    let mut r = read_docx(&f).expect("reads");
    r.document.validate().expect("valid");
    let max = r.document.all_ids().into_iter().max().expect("ids");
    assert!(r.document.alloc_id() > max);
    // Every paragraph is reachable by id.
    for p in all_paragraphs(&r.document) {
        assert!(r.document.paragraph(p.id).is_some());
    }
}

#[test]
fn read_documents_serialise_and_load_back() {
    for name in [
        "tables.docx",
        "fields.docx",
        "text-boxes.docx",
        "tracked-changes.docx",
    ] {
        let f = find(name).and_then(|f| f.read()).expect("fixture");
        let r = read_docx(&f).expect("reads");
        let bytes = r.document.to_bytes().expect("encode");
        let back = kiln_doc::Document::from_bytes(&bytes).expect("decode");
        assert_eq!(back, r.document, "{name}");
    }
}
