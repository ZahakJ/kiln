// SPDX-License-Identifier: AGPL-3.0-or-later
//! The writer's element mapping on hand-written packages (ported from the v1.0 writer's
//! `elements.test.ts`): runs, paragraphs, tracked changes, fields, content controls and tables
//! come back exactly as read, and the XML follows the schema order Word requires.

mod read_support;
mod roundtrip_support;

use kiln_doc::fields::FieldForm;
use kiln_doc::st::VMerge;
use kiln_doc::{Block, InlineObject, Mark};
use read_support::{Build, at_text, build_docx, objects, p, p_pr, paragraph_with, r, r_pr};
use roundtrip_support::{RoundTrip, differences, differences_but_warnings, round_trip};

/// Round-trips a body the reader had to repair: the repaired model survives, the repair
/// diagnostic does not come back.
fn repaired(body: &str) -> RoundTrip {
    let rt = round_trip(&build_docx(Build {
        body: body.to_owned(),
        ..Build::default()
    }));
    let d = differences_but_warnings(&rt.a, &rt.b);
    assert!(d.is_empty(), "{}", d.join("\n\n"));
    assert!(rt.b.warnings.len() < rt.a.warnings.len());
    rt
}

/// Round-trips a body and asserts the model and extras survive.
fn same(body: &str) -> RoundTrip {
    let rt = round_trip(&build_docx(Build {
        body: body.to_owned(),
        ..Build::default()
    }));
    let d = differences(&rt.a, &rt.b);
    assert!(d.is_empty(), "{}", d.join("\n\n"));
    rt
}

fn document(rt: &RoundTrip) -> String {
    rt.xml("/word/document.xml")
}

fn index(xml: &str, needles: &[&str]) {
    let mut at = 0;
    for n in needles {
        match xml[at..].find(n) {
            Some(i) => at += i + n.len(),
            None => panic!("{n} not found in order in {xml}"),
        }
    }
}

// ------------------------------------------------------------------ runs

#[test]
fn every_modelled_run_property_comes_back() {
    let rpr = concat!(
        r#"<w:rStyle w:val="Emphasis"/><w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="MS Mincho" w:cs="Arial" w:asciiTheme="minorHAnsi"/>"#,
        r#"<w:b/><w:bCs/><w:i w:val="0"/><w:iCs/><w:caps/><w:smallCaps w:val="0"/><w:strike/><w:dstrike w:val="0"/>"#,
        r#"<w:outline/><w:shadow/><w:emboss/><w:imprint/><w:vanish/>"#,
        r#"<w:color w:val="2F5496" w:themeColor="accent1" w:themeShade="BF"/><w:spacing w:val="20"/><w:w w:val="150"/>"#,
        r#"<w:kern w:val="28"/><w:position w:val="-4"/><w:sz w:val="28"/><w:szCs w:val="26"/><w:highlight w:val="yellow"/>"#,
        r#"<w:u w:val="dotDash" w:color="FF0000"/><w:bdr w:val="single" w:sz="4" w:space="1" w:color="auto"/>"#,
        r#"<w:shd w:val="pct25" w:color="auto" w:fill="FFFF00" w:themeFill="accent2" w:themeFillTint="33"/>"#,
        r#"<w:vertAlign w:val="superscript"/><w:rtl/><w:lang w:val="en-GB" w:eastAsia="ja-JP" w:bidi="ar-SA"/>"#,
    );
    let rt = same(&p(&r_pr("Styled", rpr)));
    let (props, _) = at_text(&rt.b.document, "Styled");
    assert_eq!(props.bold, Some(true));
    assert_eq!(props.italic, Some(false));
    assert_eq!(props.scale, Some(150));
    assert_eq!(props.position, Some(-4));
    let xml = document(&rt);
    // CT_RPr order.
    index(
        &xml,
        &[
            "w:rStyle",
            "w:rFonts",
            "<w:b/>",
            "<w:bCs/>",
            "<w:i ",
            "w:caps",
            "w:strike",
            "w:outline",
            "w:vanish",
            "w:color",
            "w:spacing",
            "<w:w ",
            "w:kern",
            "w:position",
            "<w:sz ",
            "w:szCs",
            "w:highlight",
            "<w:u ",
            "w:bdr",
            "w:shd",
            "w:vertAlign",
            "<w:rtl/>",
            "w:lang",
        ],
    );
}

#[test]
fn unmodelled_run_properties_are_kept_in_schema_order() {
    let rt = same(&p(&r_pr(
        "Effects",
        r#"<w:rFonts w:hint="eastAsia"/><w:b/><w:noProof/><w:webHidden/><w:em w:val="dot"/><w14:ligatures w14:val="standard"/><w14:textOutline w14:w="9525"><w14:noFill/></w14:textOutline>"#,
    )));
    let xml = document(&rt);
    index(
        &xml,
        &[
            "<w:rFonts w:hint=\"eastAsia\"/>",
            "<w:b/>",
            "<w:noProof/>",
            "<w:webHidden/>",
            "<w:em w:val=\"dot\"/>",
            "<w14:ligatures",
            "<w14:textOutline",
        ],
    );
    assert!(xml.contains("xmlns:w14="), "the w14 namespace is declared");
}

#[test]
fn breaks_tabs_symbols_and_special_hyphens_come_back() {
    let rt = same(&p(concat!(
        "<w:r><w:t>a</w:t><w:tab/><w:t>b</w:t><w:br/><w:t>c</w:t><w:br w:type=\"page\"/><w:br w:type=\"column\"/><w:br w:clear=\"all\"/><w:cr/></w:r>",
        "<w:r><w:noBreakHyphen/><w:softHyphen/><w:sym w:font=\"Wingdings\" w:char=\"F0E8\"/></w:r>"
    )));
    let xml = document(&rt);
    assert!(xml.contains("<w:tab/>"));
    assert!(xml.contains("<w:br w:type=\"page\"/>"));
    assert!(xml.contains("<w:br w:type=\"column\"/>"));
    assert!(xml.contains("<w:br w:clear=\"all\"/>"));
    assert!(xml.contains("<w:sym w:font=\"Wingdings\" w:char=\"F0E8\"/>"));
    // Text, tab, breaks and symbols of one formatting share a run.
    assert_eq!(xml.matches("<w:r>").count(), 1, "{xml}");
}

#[test]
fn significant_spaces_are_preserved_and_equal_runs_merged() {
    let rt = same(&p(&format!(
        "{}{}{}",
        r(" leading"),
        r("and  double"),
        r("trailing ")
    )));
    let xml = document(&rt);
    assert!(
        xml.contains("<w:t xml:space=\"preserve\"> leadingand  doubletrailing </w:t>"),
        "{xml}"
    );
}

// ------------------------------------------------------------------ paragraphs

#[test]
fn every_modelled_paragraph_property_comes_back() {
    let ppr = concat!(
        r#"<w:pStyle w:val="Heading1"/><w:keepNext/><w:keepLines w:val="0"/><w:pageBreakBefore/><w:widowControl w:val="0"/>"#,
        r#"<w:numPr><w:ilvl w:val="1"/><w:numId w:val="3"/></w:numPr><w:suppressLineNumbers/>"#,
        r#"<w:pBdr><w:top w:val="single" w:sz="4" w:space="1" w:color="auto"/><w:between w:val="double" w:sz="6" w:space="0" w:color="FF0000"/></w:pBdr>"#,
        r#"<w:shd w:val="clear" w:color="auto" w:fill="D9E2F3"/><w:tabs><w:tab w:val="center" w:pos="4680"/><w:tab w:val="right" w:leader="dot" w:pos="9360"/></w:tabs>"#,
        r#"<w:suppressAutoHyphens/><w:bidi/><w:snapToGrid w:val="0"/><w:spacing w:before="240" w:after="120" w:line="360" w:lineRule="auto" w:beforeAutospacing="1"/>"#,
        r#"<w:ind w:left="720" w:right="360" w:hanging="360"/><w:contextualSpacing/><w:mirrorIndents/><w:jc w:val="both"/>"#,
        r#"<w:textAlignment w:val="center"/><w:outlineLvl w:val="0"/>"#,
    );
    let rt = same(&p_pr(&r("Para"), ppr));
    let para = paragraph_with(&rt.b.document, "Para");
    assert_eq!(para.props.outline_level, Some(0));
    index(
        &document(&rt),
        &[
            "w:pStyle",
            "w:keepNext",
            "w:keepLines",
            "w:pageBreakBefore",
            "w:widowControl",
            "w:numPr",
            "w:suppressLineNumbers",
            "w:pBdr",
            "w:shd",
            "w:tabs",
            "w:suppressAutoHyphens",
            "<w:bidi/>",
            "w:snapToGrid",
            "w:spacing",
            "w:ind",
            "w:contextualSpacing",
            "w:mirrorIndents",
            "<w:jc ",
            "w:textAlignment",
            "w:outlineLvl",
        ],
    );
}

#[test]
fn unmodelled_paragraph_properties_keep_their_place_without_duplicates() {
    let rt = same(&p_pr(
        &r("Kinsoku"),
        r#"<w:pStyle w:val="Normal"/><w:framePr w:w="2000" w:h="400" w:hAnchor="page" w:vAnchor="text"/><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr><w:kinsoku w:val="0"/><w:adjustRightInd w:val="0"/><w:jc w:val="left"/><w:cnfStyle w:val="100000000000"/><w:rPr><w:b/><w:noProof/></w:rPr>"#,
    ));
    let xml = document(&rt);
    assert_eq!(xml.matches("w:numPr").count(), 2, "one element: {xml}");
    index(
        &xml,
        &[
            "w:pStyle",
            "w:framePr",
            "w:numPr",
            "w:kinsoku",
            "w:adjustRightInd",
            "<w:jc ",
            "w:cnfStyle",
            "<w:rPr><w:b/><w:noProof/></w:rPr>",
        ],
    );
}

#[test]
fn drop_caps_come_back_as_frames() {
    let rt = same(&p_pr(
        &r("D"),
        r#"<w:framePr w:dropCap="drop" w:lines="3" w:wrap="around" w:vAnchor="text" w:hAnchor="text"/>"#,
    ));
    let para = paragraph_with(&rt.b.document, "D");
    assert_eq!(para.props.drop_cap.map(|d| d.lines), Some(3));
}

#[test]
fn paragraph_mark_revisions_and_property_changes_come_back() {
    let rt = same(&p_pr(
        &r("Changed"),
        concat!(
            r#"<w:jc w:val="center"/><w:rPr><w:ins w:id="7" w:author="Ed" w:date="2026-01-01T00:00:00Z"/><w:b/><w:rPrChange w:id="8" w:author="Ed"><w:rPr><w:i/></w:rPr></w:rPrChange></w:rPr>"#,
            r#"<w:pPrChange w:id="9" w:author="Ed" w:date="2026-01-01T00:00:00Z"><w:pPr><w:jc w:val="right"/></w:pPr></w:pPrChange>"#
        ),
    ));
    let para = paragraph_with(&rt.b.document, "Changed");
    assert!(para.revision.mark.is_some());
    assert!(para.revision.props_change.is_some());
    assert!(para.revision.mark_props_change.is_some());
}

#[test]
fn para_ids_are_written_with_the_w14_namespace_ignorable() {
    let rt =
        same(r#"<w:p w14:paraId="1A2B3C4D" w14:textId="77777777"><w:r><w:t>x</w:t></w:r></w:p>"#);
    let xml = document(&rt);
    assert!(xml.contains("w14:paraId=\"1A2B3C4D\" w14:textId=\"77777777\""));
    assert!(xml.contains("mc:Ignorable=\"w14"), "{xml}");
}

// ------------------------------------------------------------------ revisions

#[test]
fn insertions_deletions_and_formatting_changes_come_back() {
    let rt = same(&p(concat!(
        r#"<w:ins w:id="1" w:author="A" w:date="2026-01-15T09:30:00Z"><w:r><w:t>new</w:t></w:r></w:ins>"#,
        r#"<w:del w:id="2" w:author="B"><w:r><w:delText xml:space="preserve"> old </w:delText></w:r></w:del>"#,
        r#"<w:r><w:rPr><w:b/><w:rPrChange w:id="3" w:author="C"><w:rPr/></w:rPrChange></w:rPr><w:t>bold</w:t></w:r>"#
    )));
    let xml = document(&rt);
    assert!(xml.contains("<w:delText xml:space=\"preserve\"> old </w:delText>"));
    assert!(
        xml.contains(r#"<w:rPrChange w:id="3" w:author="C"><w:rPr/></w:rPrChange>"#),
        "{xml}"
    );
}

#[test]
fn tracked_moves_are_written_as_deletion_and_insertion() {
    let rt = round_trip(&build_docx(Build {
        body: p(concat!(
            r#"<w:moveFrom w:id="1" w:author="M"><w:r><w:t>moved</w:t></w:r></w:moveFrom>"#,
            r#"<w:moveTo w:id="2" w:author="M"><w:r><w:t>moved</w:t></w:r></w:moveTo>"#
        )),
        ..Build::default()
    }));
    let marks: Vec<&Mark> = paragraph_with(&rt.b.document, "moved")
        .content
        .marks()
        .iter()
        .map(|m| &m.mark)
        .collect();
    assert!(marks.iter().any(|m| matches!(m, Mark::Deletion(_))));
    assert!(marks.iter().any(|m| matches!(m, Mark::Insertion(_))));
    let report = kiln_io_docx::compat_report(&rt.a.document, Default::default());
    assert!(report.iter().any(|i| i.code == "revision.move"));
}

#[test]
fn deleted_field_codes_use_del_instr_text() {
    let rt = same(&p(concat!(
        r#"<w:del w:id="5" w:author="A"><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:delInstrText xml:space="preserve"> PAGE </w:delInstrText></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:delText>3</w:delText></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r></w:del>"#
    )));
    assert!(
        document(&rt).contains("<w:delInstrText xml:space=\"preserve\"> PAGE </w:delInstrText>")
    );
}

#[test]
fn row_revisions_and_cell_revisions_come_back() {
    let rt = same(concat!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid>",
        r#"<w:tr><w:trPr><w:ins w:id="4" w:author="A"/></w:trPr><w:tc><w:tcPr><w:cellIns w:id="5" w:author="A"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc></w:tr>"#,
        "</w:tbl><w:p/>"
    ));
    let xml = document(&rt);
    assert!(
        xml.contains(r#"<w:trPr><w:ins w:id="4" w:author="A"/></w:trPr>"#),
        "{xml}"
    );
    assert!(xml.contains(r#"<w:cellIns w:id="5" w:author="A"/>"#));
}

// ------------------------------------------------------------------ fields

#[test]
fn complex_fields_keep_result_dirty_and_locked_flags() {
    let rt = same(&p(concat!(
        r#"<w:r><w:fldChar w:fldCharType="begin" w:dirty="true" w:fldLock="1"/></w:r><w:r><w:instrText xml:space="preserve"> DATE \@ "d MMMM yyyy" </w:instrText></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:rPr><w:b/></w:rPr><w:t>16 October 2026</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )));
    let f = objects(&rt.b.document)
        .into_iter()
        .find_map(|o| match o {
            InlineObject::Field(f) => Some(f.clone()),
            _ => None,
        })
        .expect("field");
    assert!(f.dirty && f.locked);
    assert_eq!(f.result, "16 October 2026");
}

#[test]
fn nested_fields_in_an_instruction_become_nested_fields() {
    let rt = same(&p(concat!(
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> IF </w:instrText></w:r>"#,
        r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>1</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#,
        r#"<w:r><w:instrText xml:space="preserve"> = 1 "first" "other" </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>first</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
    )));
    let xml = document(&rt);
    assert_eq!(xml.matches("w:fldCharType=\"begin\"").count(), 2, "{xml}");
}

#[test]
fn simple_fields_and_page_numbers_keep_their_form() {
    let rt = same(&p(concat!(
        r#"<w:fldSimple w:instr=" NUMPAGES "><w:r><w:t>4</w:t></w:r></w:fldSimple>"#,
        r#"<w:r><w:pgNum/></w:r>"#
    )));
    let forms: Vec<FieldForm> = objects(&rt.b.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::Field(f) => Some(f.form),
            _ => None,
        })
        .collect();
    assert_eq!(forms, [FieldForm::Simple, FieldForm::Simple]);
    let xml = document(&rt);
    assert!(
        xml.contains("<w:fldSimple w:instr=\" NUMPAGES \">"),
        "{xml}"
    );
    let rt2 = roundtrip_support::round_trip(&rt.bytes);
    assert!(differences(&rt2.a, &rt2.b).is_empty());
    assert!(xml.contains("<w:pgNum/>"));
}

#[test]
fn hyperlink_fields_become_links_and_tables_of_contents_span_paragraphs() {
    let rt = same(&format!(
        "{}{}{}",
        p(concat!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> TOC \o "1-3" \h </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r>"#,
            r#"<w:r><w:t>Intro</w:t></w:r>"#
        )),
        p(r#"<w:r><w:t>Body</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#),
        p(concat!(
            r#"<w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText xml:space="preserve"> HYPERLINK "https://example.com" </w:instrText></w:r>"#,
            r#"<w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>link</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r>"#
        ))
    ));
    let doc = &rt.b.document;
    assert!(matches!(doc.sections[0].blocks[0], Block::Field(_)));
    let (_, marks) = at_text(doc, "link");
    assert!(marks.iter().any(|m| matches!(m, Mark::Link(_))));
}

// ------------------------------------------------------------------ content controls

#[test]
fn block_and_inline_content_controls_wrap_what_they_held() {
    let rt = same(&format!(
        "<w:sdt><w:sdtPr><w:alias w:val=\"Box\"/><w:tag w:val=\"b\"/><w:id w:val=\"7\"/></w:sdtPr><w:sdtContent>{}{}</w:sdtContent></w:sdt>{}",
        p(&r("one")),
        p(&r("two")),
        p(concat!(
            r#"<w:sdt><w:sdtPr><w:id w:val="8"/><w14:checkbox><w14:checked w14:val="1"/></w14:checkbox></w:sdtPr><w:sdtContent><w:r><w:t>☒</w:t></w:r></w:sdtContent></w:sdt>"#,
            r#"<w:sdt><w:sdtPr><w:dropDownList><w:listItem w:displayText="A" w:value="a"/></w:dropDownList></w:sdtPr><w:sdtContent><w:r><w:t>A</w:t></w:r></w:sdtContent></w:sdt>"#
        ))
    ));
    assert!(matches!(&rt.b.document.sections[0].blocks[0], Block::Sdt(s) if s.blocks.len() == 2));
}

#[test]
fn row_and_cell_content_controls_are_rewrapped() {
    same(concat!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/><w:gridCol w:w=\"2000\"/></w:tblGrid>",
        "<w:sdt><w:sdtPr><w:id w:val=\"1\"/><w15:repeatingSection/></w:sdtPr><w:sdtContent>",
        "<w:sdt><w:sdtPr><w:id w:val=\"2\"/><w15:repeatingSectionItem/></w:sdtPr><w:sdtContent>",
        "<w:tr><w:tc><w:p><w:r><w:t>a</w:t></w:r></w:p></w:tc>",
        "<w:sdt><w:sdtPr><w:id w:val=\"3\"/></w:sdtPr><w:sdtContent><w:tc><w:p><w:r><w:t>b</w:t></w:r></w:p></w:tc></w:sdtContent></w:sdt>",
        "</w:tr></w:sdtContent></w:sdt></w:sdtContent></w:sdt>",
        "<w:tr><w:tc><w:p/></w:tc><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>"
    ));
}

// ------------------------------------------------------------------ tables

#[test]
fn table_properties_widths_and_look_come_back() {
    let rt = same(concat!(
        "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblpPr w:leftFromText=\"180\" w:rightFromText=\"180\" w:vertAnchor=\"text\" w:tblpY=\"1\"/>",
        "<w:tblOverlap w:val=\"never\"/><w:bidiVisual/><w:tblStyleRowBandSize w:val=\"1\"/><w:tblW w:w=\"5000\" w:type=\"pct\"/><w:jc w:val=\"center\"/>",
        "<w:tblCellSpacing w:w=\"20\" w:type=\"dxa\"/><w:tblInd w:w=\"10%\" w:type=\"pct\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:space=\"0\" w:color=\"auto\"/><w:insideV w:val=\"nil\"/></w:tblBorders>",
        "<w:shd w:val=\"clear\" w:color=\"auto\" w:fill=\"auto\"/><w:tblLayout w:type=\"fixed\"/><w:tblCellMar><w:left w:w=\"108\" w:type=\"dxa\"/><w:right w:w=\"108\" w:type=\"dxa\"/></w:tblCellMar>",
        "<w:tblLook w:val=\"04A0\" w:firstRow=\"1\" w:lastRow=\"0\" w:firstColumn=\"1\" w:lastColumn=\"0\" w:noHBand=\"0\" w:noVBand=\"1\"/><w:tblCaption w:val=\"Cap\"/><w:tblDescription w:val=\"Desc\"/></w:tblPr>",
        "<w:tblGrid><w:gridCol w:w=\"3000\"/></w:tblGrid><w:tr><w:trPr><w:cantSplit/><w:trHeight w:val=\"400\" w:hRule=\"exact\"/><w:tblHeader/><w:jc w:val=\"right\"/></w:trPr>",
        "<w:tc><w:tcPr><w:tcW w:w=\"3000\" w:type=\"dxa\"/><w:tcBorders><w:tl2br w:val=\"single\" w:sz=\"4\"/></w:tcBorders><w:shd w:val=\"clear\" w:fill=\"FF0000\"/><w:noWrap/>",
        "<w:tcMar><w:top w:w=\"50\" w:type=\"dxa\"/></w:tcMar><w:textDirection w:val=\"btLr\"/><w:tcFitText/><w:vAlign w:val=\"center\"/><w:hideMark/></w:tcPr><w:p><w:r><w:t>x</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>"
    ));
    index(
        &document(&rt),
        &[
            "w:tblStyle",
            "w:tblpPr",
            "w:tblOverlap",
            "w:bidiVisual",
            "w:tblStyleRowBandSize",
            "w:tblW",
            "<w:jc ",
            "w:tblCellSpacing",
            "w:tblInd",
            "w:tblBorders",
            "<w:shd ",
            "w:tblLayout",
            "w:tblCellMar",
            "w:tblLook",
            "w:tblCaption",
            "w:tblDescription",
        ],
    );
}

#[test]
fn merged_cells_keep_grid_spans_and_vertical_merges() {
    let rt = same(concat!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1000\"/><w:gridCol w:w=\"1000\"/><w:gridCol w:w=\"1000\"/></w:tblGrid>",
        "<w:tr><w:tc><w:tcPr><w:gridSpan w:val=\"2\"/></w:tcPr><w:p/></w:tc><w:tc><w:tcPr><w:vMerge w:val=\"restart\"/></w:tcPr><w:p/></w:tc></w:tr>",
        "<w:tr><w:tc><w:p/></w:tc><w:tc><w:p/></w:tc><w:tc><w:tcPr><w:vMerge/></w:tcPr><w:p/></w:tc></w:tr></w:tbl><w:p/>"
    ));
    let Block::Table(t) = &rt.b.document.sections[0].blocks[0] else {
        panic!("table");
    };
    assert_eq!(t.rows[0].cells[0].grid_span, 2);
    assert_eq!(t.rows[1].cells[2].v_merge, Some(VMerge::Continue));
}

#[test]
fn legacy_horizontal_merges_become_one_spanning_cell() {
    let rt = round_trip(&build_docx(Build {
        body: concat!(
            "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1000\"/><w:gridCol w:w=\"1000\"/></w:tblGrid>",
            "<w:tr><w:tc><w:tcPr><w:hMerge w:val=\"restart\"/></w:tcPr><w:p><w:r><w:t>wide</w:t></w:r></w:p></w:tc><w:tc><w:tcPr><w:hMerge/></w:tcPr><w:p/></w:tc></w:tr></w:tbl><w:p/>"
        )
        .to_owned(),
        ..Build::default()
    }));
    let xml = rt.xml("/word/document.xml");
    assert!(xml.contains("<w:gridSpan w:val=\"2\"/>"));
    assert!(!xml.contains("hMerge"));
    // The merged cell's own (empty) content was dropped on reading, so node ids differ.
    let Block::Table(t) = &rt.b.document.sections[0].blocks[0] else {
        panic!("table");
    };
    assert_eq!(t.rows[0].cells.len(), 1);
    assert_eq!(t.rows[0].cells[0].grid_span, 2);
    assert_eq!(
        kiln_io_docx::snapshot_facts(&rt.a.document),
        kiln_io_docx::snapshot_facts(&rt.b.document)
    );
}

#[test]
fn grid_before_after_and_ragged_rows_are_written() {
    let rt = repaired(concat!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"1000\"/><w:gridCol w:w=\"1000\"/><w:gridCol w:w=\"1000\"/></w:tblGrid>",
        "<w:tr><w:trPr><w:gridBefore w:val=\"1\"/><w:wBefore w:w=\"1000\" w:type=\"dxa\"/></w:trPr><w:tc><w:p/></w:tc><w:tc><w:p/></w:tc></w:tr>",
        "<w:tr><w:tc><w:p/></w:tc></w:tr></w:tbl><w:p/>"
    ));
    let xml = document(&rt);
    index(&xml, &["<w:gridBefore w:val=\"1\"/>", "<w:wBefore"]);
    assert!(xml.contains("<w:gridAfter w:val=\"2\"/>"), "{xml}");
}

#[test]
fn nested_tables_end_every_cell_with_a_paragraph() {
    let rt = same(concat!(
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc>",
        "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>inner</w:t></w:r></w:p></w:tc></w:tr></w:tbl><w:p/>",
        "</w:tc></w:tr></w:tbl><w:p/>"
    ));
    assert!(document(&rt).contains("</w:tbl><w:p/></w:tc>"));
}

/// A cell whose last child is a nested table, without the end-of-cell paragraph Word writes
/// (Apache POI's `deep-table-cell.docx` is made of them).
const CELL_ENDING_WITH_A_TABLE: &str = concat!(
    "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"4000\"/></w:tblGrid><w:tr><w:tc>",
    "<w:p><w:r><w:t>outer</w:t></w:r></w:p>",
    "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"2000\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>inner</w:t></w:r></w:p></w:tc></w:tr></w:tbl>",
    "</w:tc></w:tr></w:tbl><w:p/>"
);

/// The outer table's only cell.
fn outer_cell(doc: &mut kiln_doc::Document) -> &mut kiln_doc::Cell {
    let Some(Block::Table(t)) = doc.sections[0].blocks.first_mut() else {
        panic!("table");
    };
    &mut t.rows[0].cells[0]
}

#[test]
fn an_unedited_cell_ending_with_a_table_is_saved_without_an_added_paragraph() {
    // The model still ends the cell with a paragraph, as Word does …
    let rt = same(CELL_ENDING_WITH_A_TABLE);
    let mut doc = rt.a.document.clone();
    assert!(matches!(
        outer_cell(&mut doc).blocks.last(),
        Some(Block::Paragraph(_))
    ));
    assert_eq!(rt.a.extras.cell_end_paragraphs.len(), 1);
    // … but the saved cell ends with the table, as the file had it (LibreOffice lays the two
    // forms out differently).
    let xml = document(&rt);
    assert!(xml.contains("</w:tbl></w:tc>"), "{xml}");
    assert!(!xml.contains("</w:tbl><w:p/></w:tc>"), "{xml}");
}

#[test]
fn an_end_of_cell_paragraph_that_was_typed_in_or_formatted_is_saved() {
    let pkg_bytes = build_docx(Build {
        body: CELL_ENDING_WITH_A_TABLE.to_owned(),
        ..Build::default()
    });
    let pkg = kiln_ooxml::opc::OpcPackage::open(&pkg_bytes).expect("package");
    let read = kiln_io_docx::read_package(&pkg).expect("reads");
    let save = |doc: &kiln_doc::Document| {
        let source = kiln_io_docx::WriteSource::from_read(&read).with_package(&pkg);
        let out =
            kiln_io_docx::write_docx(doc, Some(&source), &Default::default()).expect("writes");
        let back = kiln_io_docx::read_docx(&out.bytes).expect("reads back");
        let pkg = kiln_ooxml::opc::OpcPackage::open(&out.bytes).expect("package");
        let xml = pkg
            .part("/word/document.xml")
            .expect("readable")
            .expect("present");
        (String::from_utf8(xml.into_owned()).expect("UTF-8"), back)
    };

    // Typed in: the paragraph and its text are written.
    let mut typed = read.document.clone();
    if let Some(Block::Paragraph(p)) = outer_cell(&mut typed).blocks.last_mut() {
        p.content =
            kiln_doc::Inlines::from_fragment(kiln_doc::Fragment::text("typed", Default::default()))
                .expect("fragment");
    }
    let (xml, back) = save(&typed);
    assert!(
        xml.contains("</w:tbl><w:p><w:r><w:t>typed</w:t></w:r></w:p></w:tc>"),
        "{xml}"
    );
    assert!(back.extras.cell_end_paragraphs.is_empty());

    // Formatted: the paragraph is written with its properties.
    let mut formatted = read.document.clone();
    if let Some(Block::Paragraph(p)) = outer_cell(&mut formatted).blocks.last_mut() {
        p.props.keep_next = Some(true);
    }
    let (xml, _) = save(&formatted);
    assert!(
        xml.contains("</w:tbl><w:p><w:pPr><w:keepNext/></w:pPr></w:p></w:tc>"),
        "{xml}"
    );

    // The table before it deleted: the cell keeps its paragraphs.
    let mut deleted = read.document.clone();
    let cell = outer_cell(&mut deleted);
    cell.blocks.remove(1);
    let (xml, _) = save(&deleted);
    assert!(
        xml.contains("<w:t>outer</w:t></w:r></w:p><w:p/></w:tc>"),
        "{xml}"
    );

    // Without the reader's extras (a document built in Kiln) every cell ends with a paragraph.
    let out = kiln_io_docx::write_docx(&read.document, None, &Default::default()).expect("writes");
    let pkg = kiln_ooxml::opc::OpcPackage::open(&out.bytes).expect("package");
    let xml = String::from_utf8(
        pkg.part("/word/document.xml")
            .expect("readable")
            .expect("present")
            .into_owned(),
    )
    .expect("UTF-8");
    assert!(xml.contains("</w:tbl><w:p/></w:tc>"), "{xml}");
}

#[test]
fn a_section_break_after_a_table_gets_a_paragraph_of_its_own() {
    let mut doc = kiln_doc::Document::new();
    let mut second = doc.sections[0].clone();
    second.id = doc.alloc_id();
    let pid = doc.alloc_id();
    second.blocks = vec![Block::Paragraph(kiln_doc::Paragraph::new(pid))];
    let tid = doc.alloc_id();
    let (rid, cid, cpid) = (doc.alloc_id(), doc.alloc_id(), doc.alloc_id());
    doc.sections[0].blocks = vec![Block::Table(kiln_doc::Table {
        id: tid,
        props: Default::default(),
        grid: vec![2000],
        rows: vec![kiln_doc::Row {
            id: rid,
            props: Default::default(),
            revision: None,
            cells: vec![kiln_doc::Cell {
                id: cid,
                props: Default::default(),
                grid_span: 1,
                v_merge: None,
                blocks: vec![Block::Paragraph(kiln_doc::Paragraph::new(cpid))],
            }],
        }],
    })];
    doc.sections.push(second);
    doc.validate().expect("valid");
    let out = kiln_io_docx::write_docx(&doc, None, &Default::default()).expect("writes");
    let back = kiln_io_docx::read_docx(&out.bytes).expect("reads");
    assert_eq!(back.document.sections.len(), 2);
    assert!(matches!(
        back.document.sections[0].blocks[0],
        Block::Table(_)
    ));
    assert!(matches!(
        back.document.sections[0].blocks[1],
        Block::Paragraph(_)
    ));
}
