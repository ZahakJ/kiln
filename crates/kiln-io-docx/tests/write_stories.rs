// SPDX-License-Identifier: AGPL-3.0-or-later
//! The writer's stories and shared parts (ported from the v1.0 writer's `stories.test.ts`):
//! drawings (kept verbatim, patched when edited, built when new), equations, comments with their
//! threads, notes, bookmarks and hyperlinks, sections with shared headers and footers, styles,
//! numbering and settings.

mod read_support;
mod roundtrip_support;

use kiln_doc::st::{AlignH, RelativeFromH, RelativeFromV, WrapSide, WrapType};
use kiln_doc::{
    Anchor, Block, Comment, CommentMeta, Document, Drawing, DrawingKind, Fragment, InlineObject,
    Inlines, Mark, Paragraph, Placement, PosH, PosV, RunProps,
};
use kiln_io_docx::{DocxRead, WriteOptions, WriteSource, read_docx, write_docx};
use kiln_ooxml::opc::{OpcPackage, rel_type};
use read_support::{Build, ExtraPart, build_docx, image_part, objects, p, p_pr, part, r};
use roundtrip_support::{RoundTrip, differences, round_trip};

const MAIN: &str = "/word/document.xml";

fn same(b: Build) -> RoundTrip {
    let rt = round_trip(&build_docx(b));
    let d = differences(&rt.a, &rt.b);
    assert!(d.is_empty(), "{}", d.join("\n\n"));
    rt
}

fn body(body: String) -> Build {
    Build {
        body,
        ..Build::default()
    }
}

/// Edits the read document with `edit`, writes it and reads it back.
fn edit_and_write(original: &[u8], edit: impl FnOnce(&mut Document)) -> (Vec<u8>, DocxRead) {
    let pkg = OpcPackage::open(original).unwrap();
    let read = kiln_io_docx::read_package(&pkg).unwrap();
    let mut doc = read.document.clone();
    edit(&mut doc);
    doc.validate().expect("edited document is valid");
    let source = WriteSource::from_read(&read).with_package(&pkg);
    let out = write_docx(&doc, Some(&source), &WriteOptions::default()).unwrap();
    let back = read_docx(&out.bytes).unwrap();
    (out.bytes, back)
}

fn part_text(bytes: &[u8], name: &str) -> String {
    let pkg = OpcPackage::open(bytes).unwrap();
    String::from_utf8_lossy(&pkg.part_bytes(name).unwrap()).into_owned()
}

/// Replaces the first drawing of the body with `f(drawing)`.
fn edit_drawing(doc: &mut Document, f: impl FnOnce(&mut Drawing)) {
    for s in &mut doc.sections {
        for b in &mut s.blocks {
            let Block::Paragraph(p) = b else { continue };
            let Some((at, InlineObject::Drawing(d))) = p
                .content
                .objects_with_offsets()
                .find(|(_, o)| matches!(o, InlineObject::Drawing(_)))
                .map(|(i, o)| (i, o.clone()))
            else {
                continue;
            };
            let mut d = d;
            f(&mut d);
            let props = p.content.props_at(at).cloned().unwrap_or_default();
            p.content.remove(at, at + 1).unwrap();
            p.content
                .insert(at, Fragment::object(InlineObject::Drawing(d), props))
                .unwrap();
            return;
        }
    }
    panic!("no drawing");
}

// ------------------------------------------------------------------ drawings

fn inline(graphic: &str, doc_pr: &str) -> String {
    format!(
        "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"914400\" cy=\"457200\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>{doc_pr}<wp:cNvGraphicFramePr/><a:graphic>{graphic}</a:graphic></wp:inline></w:drawing></w:r>"
    )
}

fn pic_graphic() -> String {
    "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"1\" name=\"p\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"rIdImg\"/><a:srcRect l=\"10000\" r=\"5000\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm rot=\"5400000\" flipH=\"1\"><a:off x=\"0\" y=\"0\"/><a:ext cx=\"914400\" cy=\"457200\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData>".to_owned()
}

const DOC_PR: &str = "<wp:docPr id=\"7\" name=\"Picture 7\" descr=\"A chart\" title=\"T\"><a:extLst><a:ext uri=\"{C183D7F6-B498-43B3-948B-1728B52AA6E4}\"><adec:decorative xmlns:adec=\"http://schemas.microsoft.com/office/drawing/2017/decorative\" val=\"1\"/></a:ext></a:extLst></wp:docPr>";

fn picture_build() -> Build {
    Build {
        body: p(&inline(&pic_graphic(), DOC_PR)),
        parts: vec![image_part()],
        ..Build::default()
    }
}

fn drawings(doc: &Document) -> Vec<Drawing> {
    let mut out = Vec::new();
    for o in objects(doc) {
        if let InlineObject::Drawing(d) = o {
            out.push((**d).clone());
        }
    }
    out
}

#[test]
fn unchanged_pictures_are_written_verbatim() {
    let rt = same(picture_build());
    let xml = rt.xml(MAIN);
    let d = &drawings(&rt.a.document)[0];
    assert!(xml.contains(d.xml.as_deref().unwrap()), "kept verbatim");
    assert!(d.decorative && d.flip_h && d.rotation == 5_400_000);
    // The image is written under its original name with the original relationship id.
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    assert_eq!(
        pkg.resolve_relationship(MAIN, "rIdImg").unwrap().as_deref(),
        Some("/word/media/image1.png")
    );
}

#[test]
fn edited_pictures_are_patched() {
    let (bytes, back) = edit_and_write(&build_docx(picture_build()), |doc| {
        edit_drawing(doc, |d| {
            d.width = 1_828_800;
            d.description = Some("Edited".into());
            d.title = None;
            d.decorative = false;
            d.rotation = 0;
            d.flip_h = false;
            d.kind = DrawingKind::Picture {
                media: "/word/media/image1.png".into(),
                crop: None,
            };
        });
    });
    let d = &drawings(&back.document)[0];
    assert_eq!(d.width, 1_828_800);
    assert_eq!(d.description.as_deref(), Some("Edited"));
    assert_eq!(d.title, None);
    assert!(!d.decorative && !d.flip_h && d.rotation == 0);
    assert!(matches!(&d.kind, DrawingKind::Picture { crop: None, .. }));
    let xml = part_text(&bytes, MAIN);
    assert!(
        xml.contains("<a:ext cx=\"1828800\" cy=\"457200\"/>"),
        "{xml}"
    );
    assert!(!xml.contains("a:srcRect"));
}

#[test]
fn floating_pictures_keep_position_distances_and_every_wrap() {
    let mut body_xml = String::new();
    for (i, wrap) in [
        "<wp:wrapSquare wrapText=\"left\"/>",
        "<wp:wrapTight wrapText=\"bothSides\"><wp:wrapPolygon edited=\"0\"><wp:start x=\"0\" y=\"0\"/><wp:lineTo x=\"0\" y=\"21600\"/></wp:wrapPolygon></wp:wrapTight>",
        "<wp:wrapThrough wrapText=\"right\"><wp:wrapPolygon edited=\"1\"><wp:start x=\"0\" y=\"0\"/></wp:wrapPolygon></wp:wrapThrough>",
        "<wp:wrapTopAndBottom/>",
        "<wp:wrapNone/>",
    ]
    .iter()
    .enumerate()
    {
        body_xml.push_str(&p(&format!(
            "<w:r><w:drawing><wp:anchor distT=\"10\" distB=\"20\" distL=\"30\" distR=\"40\" simplePos=\"0\" relativeHeight=\"{}\" behindDoc=\"{}\" locked=\"1\" layoutInCell=\"0\" allowOverlap=\"0\"><wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"margin\"><wp:align>center</wp:align></wp:positionH><wp:positionV relativeFrom=\"line\"><wp:posOffset>-12700</wp:posOffset></wp:positionV><wp:extent cx=\"914400\" cy=\"457200\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/>{wrap}<wp:docPr id=\"{}\" name=\"a\"/><wp:cNvGraphicFramePr/><a:graphic>{}</a:graphic></wp:anchor></w:drawing></w:r>",
            100 + i,
            i % 2,
            20 + i,
            pic_graphic()
        )));
    }
    let rt = same(Build {
        body: body_xml,
        parts: vec![image_part()],
        ..Build::default()
    });
    let ds = drawings(&rt.b.document);
    assert_eq!(ds.len(), 5);
    let Placement::Anchor(a) = &ds[0].placement else {
        panic!("anchor")
    };
    assert_eq!(
        (a.wrap, a.wrap_side),
        (WrapType::Square, Some(WrapSide::Left))
    );
    assert_eq!(a.h, PosH::Align(AlignH::Center));
    assert_eq!(a.v, PosV::Offset(-12_700));
    assert!(a.locked && !a.layout_in_cell && !a.allow_overlap);
}

#[test]
fn moving_a_picture_inline_and_back_rebuilds_its_placement() {
    let anchor = Anchor {
        wrap: WrapType::Tight,
        wrap_side: Some(WrapSide::Largest),
        relative_h: RelativeFromH::Page,
        h: PosH::Offset(457_200),
        relative_v: RelativeFromV::Paragraph,
        v: PosV::Offset(0),
        dist_top: 0,
        dist_bottom: 0,
        dist_left: 114_300,
        dist_right: 114_300,
        allow_overlap: true,
        layout_in_cell: true,
        locked: false,
        z_order: 251_659_264,
        wrap_polygon_xml: None,
    };
    let floating = anchor.clone();
    let (bytes, back) = edit_and_write(&build_docx(picture_build()), move |doc| {
        edit_drawing(doc, |d| d.placement = Placement::Anchor(Box::new(floating)));
    });
    let d = &drawings(&back.document)[0];
    let Placement::Anchor(a) = &d.placement else {
        panic!("anchored")
    };
    assert_eq!(
        (a.wrap, a.h, a.relative_h),
        (anchor.wrap, anchor.h, anchor.relative_h)
    );
    assert!(a.wrap_polygon_xml.is_some(), "a tight wrap gets a polygon");
    // The picture itself and its alt text survive the move.
    assert_eq!(d.description.as_deref(), Some("A chart"));
    let (_, inline_again) = edit_and_write(&bytes, |doc| {
        edit_drawing(doc, |d| d.placement = Placement::Inline);
    });
    assert_eq!(
        drawings(&inline_again.document)[0].placement,
        Placement::Inline
    );
}

#[test]
fn new_drawings_get_ids_above_the_kept_ones_and_pictures_get_media() {
    let original = build_docx(picture_build());
    let (bytes, back) = edit_and_write(&original, |doc| {
        let id = doc.alloc_id();
        let pic = Drawing::picture(id, "/word/media/image1.png", 100, 200);
        let pid = doc.alloc_id();
        let mut para = Paragraph::new(pid);
        para.content = Inlines::from_fragment(Fragment::object(
            InlineObject::Drawing(Box::new(pic)),
            RunProps::default(),
        ))
        .unwrap();
        doc.sections[0].blocks.push(Block::Paragraph(para));
    });
    let ds = drawings(&back.document);
    assert_eq!(ds.len(), 2);
    let xml = part_text(&bytes, MAIN);
    assert!(
        xml.contains("<wp:docPr id=\"8\" name=\"Picture 8\"/>"),
        "{xml}"
    );
    // One relationship serves both pictures of the same image.
    let pkg = OpcPackage::open(&bytes).unwrap();
    let images = pkg
        .relationships(MAIN)
        .unwrap()
        .by_type(rel_type::IMAGE)
        .count();
    assert_eq!(images, 1);
}

fn shape_alternate(text: &str) -> String {
    format!(
        "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"114300\" distR=\"114300\" simplePos=\"0\" relativeHeight=\"5\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"column\"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx=\"1000\" cy=\"2000\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:wrapSquare wrapText=\"bothSides\"/><wp:docPr id=\"3\" name=\"Text Box 3\"/><wp:cNvGraphicFramePr/><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp><wps:cNvSpPr txBox=\"1\"/><wps:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"1000\" cy=\"2000\"/></a:xfrm><a:prstGeom prst=\"ellipse\"><a:avLst/></a:prstGeom><a:solidFill><a:srgbClr val=\"DEEBF7\"/></a:solidFill></wps:spPr><wps:txbx><w:txbxContent><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:txbxContent></wps:txbx><wps:bodyPr/></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing></mc:Choice><mc:Fallback><w:pict><v:oval style=\"width:1pt;height:2pt\"><v:textbox><w:txbxContent><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:txbxContent></v:textbox></v:oval></w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    )
}

#[test]
fn unchanged_shapes_keep_their_alternate_content() {
    let rt = same(body(p(&shape_alternate("inside"))));
    let xml = rt.xml(MAIN);
    assert!(xml.contains("<mc:AlternateContent><mc:Choice Requires=\"wps\">"));
    assert!(xml.contains("<v:oval"));
    assert!(xml.contains("<a:solidFill><a:srgbClr val=\"DEEBF7\"/></a:solidFill>"));
}

#[test]
fn text_box_content_is_written_from_the_model() {
    let (bytes, back) = edit_and_write(&build_docx(body(p(&shape_alternate("before")))), |doc| {
        edit_drawing(doc, |d| {
            let tb = d.text_box.as_mut().expect("text box");
            let Block::Paragraph(p) = &mut tb[0] else {
                panic!("paragraph")
            };
            let len = p.content.len();
            p.content.remove(0, len).unwrap();
            p.content
                .insert(0, Fragment::text("after", RunProps::default()))
                .unwrap();
        });
    });
    let xml = part_text(&bytes, MAIN);
    assert_eq!(
        xml.matches(">after<").count(),
        2,
        "choice and fallback: {xml}"
    );
    assert!(!xml.contains("before"));
    assert!(
        xml.contains("<mc:Fallback>"),
        "only the text changed: the fallback stays"
    );
    let d = &drawings(&back.document)[0];
    let tb = d.text_box.as_ref().unwrap();
    assert_eq!(tb[0].as_paragraph().unwrap().text(), "after");
}

#[test]
fn edited_shapes_are_patched_and_lose_their_stale_fallback() {
    let (bytes, back) = edit_and_write(&build_docx(body(p(&shape_alternate("x")))), |doc| {
        edit_drawing(doc, |d| {
            d.kind = DrawingKind::Shape {
                preset: "roundRect".into(),
            };
            d.width = 5000;
        });
    });
    let xml = part_text(&bytes, MAIN);
    assert!(!xml.contains("mc:AlternateContent"), "{xml}");
    assert!(xml.contains("prst=\"roundRect\""));
    assert!(
        xml.contains("<a:solidFill><a:srgbClr val=\"DEEBF7\"/></a:solidFill>"),
        "fills survive"
    );
    let d = &drawings(&back.document)[0];
    assert_eq!(
        d.kind,
        DrawingKind::Shape {
            preset: "roundRect".into()
        }
    );
    assert_eq!(d.width, 5000);
}

#[test]
fn shapes_made_in_the_editor_are_drawingml_shapes_with_text() {
    let mut doc = Document::new();
    let id = doc.alloc_id();
    let tid = doc.alloc_id();
    let mut inner = Paragraph::new(tid);
    inner.content = Inlines::from_fragment(Fragment::text("Note", RunProps::default())).unwrap();
    let shape = Drawing {
        kind: DrawingKind::Shape {
            preset: "wedgeRectCallout".into(),
        },
        text_box: Some(vec![Block::Paragraph(inner)]),
        ..Drawing::picture(id, "", 1_000_000, 500_000)
    };
    let Block::Paragraph(p) = &mut doc.sections[0].blocks[0] else {
        panic!("paragraph")
    };
    p.content = Inlines::from_fragment(Fragment::object(
        InlineObject::Drawing(Box::new(shape)),
        RunProps::default(),
    ))
    .unwrap();
    let out = write_docx(&doc, None, &WriteOptions::default()).unwrap();
    let back = read_docx(&out.bytes).unwrap();
    let d = &drawings(&back.document)[0];
    assert_eq!(
        d.kind,
        DrawingKind::Shape {
            preset: "wedgeRectCallout".into()
        }
    );
    assert_eq!(
        d.text_box.as_ref().unwrap()[0]
            .as_paragraph()
            .unwrap()
            .text(),
        "Note"
    );
}

#[test]
fn charts_keep_their_parts_and_relationship_ids() {
    let chart = ExtraPart {
        name: "/word/charts/chart1.xml",
        content_type: "application/vnd.openxmlformats-officedocument.drawingml.chart+xml",
        rel_type: Some(rel_type::CHART),
        rel_id: Some("rIdChart"),
        content:
            b"<c:chartSpace xmlns:c=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"/>"
                .to_vec(),
    };
    let original = build_docx(Build {
        body: p(&inline(
            "<a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/chart\"><c:chart r:id=\"rIdChart\"/></a:graphicData>",
            "<wp:docPr id=\"1\" name=\"Chart 1\"/>",
        )),
        parts: vec![chart],
        ..Build::default()
    });
    let rt = round_trip(&original);
    assert!(differences(&rt.a, &rt.b).is_empty());
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    assert_eq!(
        pkg.resolve_relationship(MAIN, "rIdChart")
            .unwrap()
            .as_deref(),
        Some("/word/charts/chart1.xml")
    );
    assert_eq!(
        pkg.part("/word/charts/chart1.xml").unwrap(),
        OpcPackage::open(&original)
            .unwrap()
            .part("/word/charts/chart1.xml")
            .unwrap()
    );
    assert_eq!(rt.b.charts.len(), 1);
}

#[test]
fn vml_pictures_text_boxes_ole_objects_smart_art_and_groups_survive() {
    let rt = same(Build {
        body: p(concat!(
            "<w:r><w:pict><v:shape style=\"width:72pt;height:0.5in\"><v:imagedata r:id=\"rIdImg\" o:title=\"Logo\"/></v:shape></w:pict></w:r>",
            "<w:r><w:pict><v:rect style=\"width:100pt;height:50pt\"><v:textbox><w:txbxContent><w:p><w:r><w:t>Boxed</w:t></w:r></w:p></w:txbxContent></v:textbox></v:rect></w:pict></w:r>",
            "<w:r><w:pict><v:line/></w:pict></w:r>",
            "<w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/><wp:docPr id=\"4\" name=\"d\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\"/></a:graphic></wp:inline></w:drawing></w:r>",
            "<w:r><w:drawing><wp:inline><wp:extent cx=\"1\" cy=\"1\"/><wp:docPr id=\"5\" name=\"g\"/><a:graphic><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingGroup\"/></a:graphic></wp:inline></w:drawing></w:r>"
        )),
        parts: vec![image_part()],
        ..Build::default()
    });
    let xml = rt.xml(MAIN);
    assert!(xml.contains("<v:imagedata r:id=\"rIdImg\" o:title=\"Logo\"/>"));
    assert!(xml.contains("<v:line/>"));
}

#[test]
fn edited_vml_pictures_become_drawingml() {
    let original = build_docx(Build {
        body: p(
            "<w:r><w:pict><v:shape style=\"width:72pt;height:0.5in\"><v:imagedata r:id=\"rIdImg\" o:title=\"Logo\"/></v:shape></w:pict></w:r>",
        ),
        parts: vec![image_part()],
        ..Build::default()
    });
    let (bytes, back) = edit_and_write(&original, |doc| edit_drawing(doc, |d| d.width *= 2));
    let xml = part_text(&bytes, MAIN);
    assert!(
        xml.contains("<w:drawing>") && !xml.contains("<w:pict>"),
        "{xml}"
    );
    let d = &drawings(&back.document)[0];
    assert_eq!(d.width, 1_828_800);
    assert!(matches!(&d.kind, DrawingKind::Picture { .. }));
}

#[test]
fn equations_are_written_as_office_math() {
    let rt = same(body(format!(
        "{}{}",
        p(&format!(
            "{}<m:oMath><m:r><m:t>mc</m:t></m:r><m:sSup><m:e><m:r><m:t>2</m:t></m:r></m:e></m:sSup></m:oMath>",
            r("E = ")
        )),
        p("<m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara>")
    )));
    let xml = rt.xml(MAIN);
    assert!(xml.contains("<m:oMathPara><m:oMath>"));
    assert!(xml.contains("xmlns:m="));
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

fn threaded() -> Build {
    Build {
        body: p(&format!(
            "<w:commentRangeStart w:id=\"1\"/>{}<w:commentRangeEnd w:id=\"1\"/><w:r><w:commentReference w:id=\"1\"/></w:r><w:r><w:commentReference w:id=\"2\"/></w:r>",
            r("discussed")
        )),
        parts: vec![
            comments_part(
                "<w:comment w:id=\"1\" w:author=\"Ann\" w:date=\"2026-01-01T00:00:00Z\" w:initials=\"A\"><w:p w14:paraId=\"00000001\"><w:r><w:t>Why?</w:t></w:r></w:p></w:comment><w:comment w:id=\"2\" w:author=\"Bo\"><w:p w14:paraId=\"00000002\"><w:r><w:t>Because.</w:t></w:r></w:p></w:comment>",
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
    }
}

#[test]
fn threads_resolved_state_people_and_ranges_come_back() {
    let rt = same(threaded());
    let c = &rt.b.document.comments;
    assert!(c[0].meta.resolved);
    assert_eq!(c[1].meta.parent, Some(c[0].id));
    let xml = rt.xml(MAIN);
    assert!(xml.contains("<w:commentRangeStart w:id=\"1\"/>"));
    assert!(
        xml.contains("<w:commentRangeEnd w:id=\"1\"/><w:r><w:commentReference w:id=\"1\"/></w:r>"),
        "{xml}"
    );
    let comments = rt.xml("/word/comments.xml");
    assert!(comments.contains("<w:annotationRef/>"));
    assert!(comments.contains("w:initials=\"A\""));
}

#[test]
fn comment_ranges_spanning_paragraphs_come_back() {
    same(Build {
        body: format!(
            "<w:commentRangeStart w:id=\"7\"/>{}{}{}",
            p(&r("first")),
            p(&format!("{}<w:commentRangeEnd w:id=\"7\"/>", r("second"))),
            p(&r("third"))
        ),
        parts: vec![comments_part(
            "<w:comment w:id=\"7\" w:author=\"A\"><w:p><w:r><w:t>Spans</w:t></w:r></w:p></w:comment>",
        )],
        ..Build::default()
    });
}

#[test]
fn comments_added_in_the_editor_get_ids_threads_and_people() {
    let (bytes, back) = edit_and_write(&build_docx(threaded()), |doc| {
        let cid = doc.alloc_id();
        let pid = doc.alloc_id();
        let mut body_p = Paragraph::new(pid);
        body_p.content =
            Inlines::from_fragment(Fragment::text("reply", RunProps::default())).unwrap();
        let parent = doc.comments[0].id;
        doc.comments.push(Comment {
            id: cid,
            meta: CommentMeta {
                author: "Kiln User".into(),
                initials: Some("KU".into()),
                date: Some("2026-10-16T10:00:00Z".into()),
                parent: Some(parent),
                resolved: false,
            },
            blocks: vec![Block::Paragraph(body_p)],
        });
        // Anchor it on the body text too.
        let Block::Paragraph(p) = &mut doc.sections[0].blocks[0] else {
            panic!("paragraph")
        };
        p.content.add_mark(0, 3, Mark::Comment(cid)).unwrap();
    });
    let c = &back.document.comments;
    assert_eq!(c.len(), 3);
    assert_eq!(c[2].meta.author, "Kiln User");
    assert_eq!(c[2].meta.parent, Some(c[0].id));
    assert!(back.extras.people.iter().any(|p| p.author == "Kiln User"));
    let comments = part_text(&bytes, "/word/comments.xml");
    assert!(
        comments.contains("w:id=\"3\""),
        "a fresh id above the others: {comments}"
    );
}

// ------------------------------------------------------------------ notes, bookmarks, links

#[test]
fn footnotes_and_endnotes_keep_separators_references_and_custom_marks() {
    let notes = |name: &'static str, root: &str, el: &str, rt: &'static str| {
        ExtraPart {
        name,
        content_type: if el == "w:footnote" {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footnotes+xml"
        } else {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.endnotes+xml"
        },
        rel_type: Some(rt),
        rel_id: None,
        content: part(
            root,
            &format!(
                "<{el} w:type=\"separator\" w:id=\"-1\"><w:p><w:pPr><w:spacing w:after=\"0\"/></w:pPr><w:r><w:separator/></w:r></w:p></{el}><{el} w:type=\"continuationSeparator\" w:id=\"0\"><w:p><w:r><w:continuationSeparator/></w:r></w:p></{el}><{el} w:id=\"4\"><w:p><w:r><w:rPr><w:vertAlign w:val=\"superscript\"/></w:rPr><w:{}Ref/></w:r><w:r><w:t xml:space=\"preserve\"> Note four</w:t></w:r></w:p></{el}>",
                &el[2..]
            ),
        )
        .into_bytes(),
    }
    };
    let rt = same(Build {
        body: p(
            "<w:r><w:footnoteReference w:customMarkFollows=\"1\" w:id=\"4\"/><w:t>*</w:t></w:r><w:r><w:endnoteReference w:id=\"4\"/></w:r>",
        ),
        settings: Some(
            "<w:footnotePr><w:footnote w:id=\"-1\"/><w:footnote w:id=\"0\"/></w:footnotePr><w:endnotePr><w:endnote w:id=\"-1\"/><w:endnote w:id=\"0\"/></w:endnotePr>".to_owned(),
        ),
        parts: vec![
            notes("/word/footnotes.xml", "w:footnotes", "w:footnote", rel_type::FOOTNOTES),
            notes("/word/endnotes.xml", "w:endnotes", "w:endnote", rel_type::ENDNOTES),
        ],
        ..Build::default()
    });
    let f = rt.xml("/word/footnotes.xml");
    assert!(f.contains("<w:r><w:separator/></w:r>") && f.contains("<w:continuationSeparator/>"));
    assert!(f.contains("<w:footnoteRef/>"));
    let body_xml = rt.xml(MAIN);
    assert!(
        body_xml
            .contains("<w:footnoteReference w:customMarkFollows=\"1\" w:id=\"4\"/><w:t>*</w:t>")
    );
    let settings = rt.xml("/word/settings.xml");
    assert!(settings.contains(
        "<w:footnotePr><w:footnote w:id=\"-1\"/><w:footnote w:id=\"0\"/></w:footnotePr>"
    ));
}

#[test]
fn notes_added_in_kiln_get_separators() {
    let mut doc = Document::new();
    let nid = doc.alloc_id();
    let np = doc.alloc_id();
    let mut note_p = Paragraph::new(np);
    note_p.content = Inlines::from_fragment(Fragment::text("A note", RunProps::default())).unwrap();
    doc.notes.push(kiln_doc::Note {
        id: nid,
        kind: kiln_doc::st::NoteKind::Footnote,
        note_type: kiln_doc::st::NoteType::Normal,
        blocks: vec![Block::Paragraph(note_p)],
    });
    let Block::Paragraph(p) = &mut doc.sections[0].blocks[0] else {
        panic!("paragraph")
    };
    p.content = Inlines::from_fragment(Fragment::object(
        InlineObject::NoteRef(kiln_doc::NoteRef {
            note: nid,
            kind: kiln_doc::st::NoteKind::Footnote,
            custom_mark: None,
        }),
        RunProps::default(),
    ))
    .unwrap();
    let out = write_docx(&doc, None, &WriteOptions::default()).unwrap();
    let back = read_docx(&out.bytes).unwrap();
    let types: Vec<_> = back.document.notes.iter().map(|n| n.note_type).collect();
    assert_eq!(
        types,
        [
            kiln_doc::st::NoteType::Separator,
            kiln_doc::st::NoteType::ContinuationSeparator,
            kiln_doc::st::NoteType::Normal
        ]
    );
    let settings = part_text(&out.bytes, "/word/settings.xml");
    assert!(
        settings.contains(
            "<w:footnotePr><w:footnote w:id=\"-1\"/><w:footnote w:id=\"0\"/></w:footnotePr>"
        ),
        "{settings}"
    );
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
}

#[test]
fn bookmarks_inside_a_hyperlink_do_not_split_it() {
    let rt = same(Build {
        body: p(&format!(
            "<w:hyperlink w:anchor=\"target\">{}<w:bookmarkStart w:id=\"0\" w:name=\"mid\"/>{}<w:bookmarkEnd w:id=\"0\"/>{}</w:hyperlink><w:bookmarkStart w:id=\"1\" w:name=\"target\"/>{}<w:bookmarkEnd w:id=\"1\"/>",
            r("one "),
            r("two"),
            r(" three"),
            r("here")
        )),
        ..Build::default()
    });
    let xml = rt.xml(MAIN);
    assert_eq!(xml.matches("<w:hyperlink").count(), 1, "{xml}");
}

#[test]
fn hyperlinks_share_one_relationship_per_url() {
    let rt = same(Build {
        body: p(&format!(
            "<w:hyperlink r:id=\"rIdL\" w:tooltip=\"Go\" w:tgtFrame=\"_blank\" w:history=\"1\">{}</w:hyperlink>{}<w:hyperlink r:id=\"rIdL\">{}</w:hyperlink>",
            r("first"),
            r(" and "),
            r("second")
        )),
        rels: vec![(
            "rIdL",
            rel_type::HYPERLINK,
            "https://example.org/p?q=1&x=2",
            true,
        )],
        ..Build::default()
    });
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    let rels = pkg.relationships(MAIN).unwrap();
    assert_eq!(rels.by_type(rel_type::HYPERLINK).count(), 1);
    assert_eq!(
        rels.get("rIdL").unwrap().target,
        "https://example.org/p?q=1&x=2"
    );
}

#[test]
fn imported_chunks_keep_their_parts() {
    let chunk = ExtraPart {
        name: "/word/afchunk.htm",
        content_type: "text/html",
        rel_type: Some(
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/aFChunk",
        ),
        rel_id: Some("rIdAlt"),
        content: b"<html><body>Imported</body></html>".to_vec(),
    };
    let rt = same(Build {
        body: format!("{}<w:altChunk r:id=\"rIdAlt\"/>{}", p(&r("a")), p(&r("b"))),
        parts: vec![chunk],
        ..Build::default()
    });
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    assert_eq!(
        pkg.resolve_relationship(MAIN, "rIdAlt").unwrap().as_deref(),
        Some("/word/afchunk.htm")
    );
}

// ------------------------------------------------------------------ sections

fn hdr(name: &'static str, id: &'static str, inner: &str, footer: bool) -> ExtraPart {
    ExtraPart {
        name,
        content_type: if footer {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.footer+xml"
        } else {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml"
        },
        rel_type: Some(if footer {
            rel_type::FOOTER
        } else {
            rel_type::HEADER
        }),
        rel_id: Some(id),
        content: part(if footer { "w:ftr" } else { "w:hdr" }, inner).into_bytes(),
    }
}

#[test]
fn sections_with_breaks_and_shared_headers_and_footers() {
    let rt = same(Build {
        body: format!(
            "{}{}<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH\"/><w:footerReference w:type=\"first\" r:id=\"rIdF\"/><w:titlePg/><w:pgSz w:w=\"15840\" w:h=\"12240\" w:orient=\"landscape\"/><w:pgMar w:top=\"720\" w:right=\"720\" w:bottom=\"720\" w:left=\"720\" w:header=\"360\" w:footer=\"360\" w:gutter=\"0\"/><w:vAlign w:val=\"center\"/><w:docGrid w:linePitch=\"360\"/></w:sectPr>",
            p_pr(
                &r("one"),
                "<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH\"/><w:type w:val=\"continuous\"/><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:cols w:num=\"3\" w:sep=\"1\" w:space=\"360\"/><w:lnNumType w:countBy=\"5\" w:restart=\"continuous\"/><w:pgNumType w:fmt=\"lowerRoman\" w:start=\"3\"/></w:sectPr>"
            ),
            p(&r("two"))
        ),
        parts: vec![
            hdr(
                "/word/header1.xml",
                "rIdH",
                "<w:p><w:r><w:t>Head</w:t></w:r></w:p>",
                false,
            ),
            hdr(
                "/word/footer1.xml",
                "rIdF",
                "<w:p><w:r><w:t>Foot</w:t></w:r></w:p>",
                true,
            ),
        ],
        ..Build::default()
    });
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    assert_eq!(
        pkg.related_parts(MAIN, rel_type::HEADER).unwrap(),
        ["/word/header1.xml"],
        "one part for the shared header"
    );
    assert_eq!(rt.b.document.sections.len(), 2);
}

#[test]
fn pictures_in_headers_get_relationships_from_the_header_part() {
    let header = ExtraPart {
        name: "/word/header1.xml",
        content_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.header+xml",
        rel_type: Some(rel_type::HEADER),
        rel_id: Some("rIdH"),
        content: part(
            "w:hdr",
            &p(&inline(
                &pic_graphic(),
                "<wp:docPr id=\"1\" name=\"Logo\"/>",
            )),
        )
        .into_bytes(),
    };
    let mut original = OpcPackage::open(&build_docx(Build {
        body: format!(
            "{}<w:sectPr><w:headerReference w:type=\"default\" r:id=\"rIdH\"/></w:sectPr>",
            p(&r("body"))
        ),
        parts: vec![header],
        ..Build::default()
    }))
    .unwrap();
    original
        .add_part(
            "/word/media/image1.png",
            "image/png",
            read_support::PNG.to_vec(),
        )
        .unwrap();
    original
        .add_relationship(
            "/word/header1.xml",
            rel_type::IMAGE,
            "/word/media/image1.png",
            kiln_ooxml::opc::RelationshipOptions {
                id: Some("rIdImg"),
                external: false,
            },
        )
        .unwrap();
    let rt = round_trip(&original.save().unwrap());
    assert!(differences(&rt.a, &rt.b).is_empty());
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    assert_eq!(
        pkg.resolve_relationship("/word/header1.xml", "rIdImg")
            .unwrap()
            .as_deref(),
        Some("/word/media/image1.png")
    );
}

#[test]
fn every_modelled_section_property_comes_back() {
    same(body(format!(
        "{}<w:sectPr w:rsidR=\"00A1B2C3\"><w:footnotePr><w:pos w:val=\"beneathText\"/><w:numFmt w:val=\"lowerRoman\"/><w:numStart w:val=\"2\"/><w:numRestart w:val=\"eachPage\"/></w:footnotePr><w:type w:val=\"evenPage\"/><w:pgSz w:w=\"11906\" w:h=\"16838\" w:code=\"9\"/><w:pgMar w:top=\"1417\" w:right=\"1134\" w:bottom=\"1134\" w:left=\"1134\" w:header=\"708\" w:footer=\"708\" w:gutter=\"567\"/><w:paperSrc w:first=\"7\"/><w:pgBorders w:offsetFrom=\"page\"><w:top w:val=\"double\" w:sz=\"6\" w:space=\"24\" w:color=\"auto\"/></w:pgBorders><w:lnNumType w:countBy=\"1\" w:start=\"1\" w:distance=\"360\"/><w:pgNumType w:fmt=\"upperRoman\" w:start=\"1\" w:chapStyle=\"1\"/><w:cols w:num=\"2\" w:equalWidth=\"0\"><w:col w:w=\"3000\" w:space=\"400\"/><w:col w:w=\"5000\"/></w:cols><w:formProt/><w:vAlign w:val=\"bottom\"/><w:noEndnote/><w:titlePg/><w:textDirection w:val=\"tbRl\"/><w:bidi/><w:rtlGutter/><w:docGrid w:type=\"lines\" w:linePitch=\"312\" w:charSpace=\"-2\"/></w:sectPr>",
        p(&r("x"))
    )));
}

// ------------------------------------------------------------------ styles, numbering, settings

#[test]
fn table_styles_conditions_aliases_and_latent_styles_come_back() {
    let rt = same(Build {
        body: p(&r("x")),
        styles: Some(
            "<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:asciiTheme=\"minorHAnsi\" w:eastAsiaTheme=\"minorEastAsia\" w:hAnsiTheme=\"minorHAnsi\" w:cstheme=\"minorBidi\"/><w:sz w:val=\"22\"/><w:lang w:val=\"en-US\" w:eastAsia=\"en-US\" w:bidi=\"ar-SA\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"160\" w:line=\"259\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults><w:latentStyles w:defLockedState=\"0\" w:count=\"376\"><w:lsdException w:name=\"Normal\" w:qFormat=\"1\"/></w:latentStyles><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/><w:qFormat/><w:rsid w:val=\"00AB12CD\"/></w:style><w:style w:type=\"table\" w:styleId=\"Grid\"><w:name w:val=\"Grid\"/><w:basedOn w:val=\"TableNormal\"/><w:uiPriority w:val=\"39\"/><w:tblPr><w:tblBorders><w:insideH w:val=\"single\" w:sz=\"4\"/></w:tblBorders><w:tblStyleRowBandSize w:val=\"1\"/></w:tblPr><w:tblStylePr w:type=\"firstRow\"><w:rPr><w:b/><w14:ligatures w14:val=\"standard\"/></w:rPr><w:tcPr><w:shd w:val=\"clear\" w:fill=\"4472C4\"/></w:tcPr></w:tblStylePr><w:tblStylePr w:type=\"band1Horz\"><w:tcPr><w:shd w:val=\"clear\" w:fill=\"D9E2F3\"/></w:tcPr></w:tblStylePr></w:style><w:style w:type=\"character\" w:customStyle=\"1\" w:styleId=\"Em\"><w:name w:val=\"Em\"/><w:aliases w:val=\"E1,E2\"/><w:link w:val=\"Normal\"/><w:semiHidden/><w:unhideWhenUsed/><w:locked/><w:rPr><w:i/><w:noProof/></w:rPr></w:style>".to_owned(),
        ),
        ..Build::default()
    });
    let styles = rt.xml("/word/styles.xml");
    assert!(styles.contains("<w:rsid w:val=\"00AB12CD\"/>"));
    assert!(styles.contains("<w14:ligatures w14:val=\"standard\"/>"));
    assert!(styles.contains("<w:noProof/>"));
    assert!(styles.contains("<w:latentStyles w:defLockedState=\"0\" w:count=\"376\">"));
}

#[test]
fn numbering_definitions_overrides_and_picture_bullets_come_back() {
    let built = build_docx(Build {
        body: p(&r("x")),
        numbering: Some(
            "<w:numPicBullet w:numPicBulletId=\"0\"><w:pict><v:shape style=\"width:9pt;height:9pt\"><v:imagedata r:id=\"rIdImg\" o:title=\"bullet\"/></v:shape></w:pict></w:numPicBullet><w:abstractNum w:abstractNumId=\"1\" w15:restartNumberingAfterBreak=\"0\"><w:nsid w:val=\"ABCD1234\"/><w:multiLevelType w:val=\"multilevel\"/><w:tmpl w:val=\"0409001F\"/><w:lvl w:ilvl=\"0\" w:tplc=\"0409000F\"><w:start w:val=\"3\"/><w:numFmt w:val=\"decimal\"/><w:lvlText w:val=\"%1.\"/><w:isLgl/><w:suff w:val=\"space\"/><w:lvlJc w:val=\"right\"/><w:pPr><w:ind w:left=\"360\" w:hanging=\"360\"/></w:pPr></w:lvl><w:lvl w:ilvl=\"1\" w15:tentative=\"1\"><w:start w:val=\"1\"/><w:numFmt w:val=\"custom\" w:format=\"001, 002, 003, ...\"/><w:lvlText w:val=\"\"/><w:lvlRestart w:val=\"0\"/><w:lvlJc w:val=\"left\"/></w:lvl></w:abstractNum><w:num w:numId=\"1\"><w:abstractNumId w:val=\"1\"/><w:lvlOverride w:ilvl=\"0\"><w:startOverride w:val=\"5\"/><w:lvl w:ilvl=\"0\"><w:start w:val=\"10\"/><w:numFmt w:val=\"upperLetter\"/><w:lvlText w:val=\"%1)\"/><w:lvlJc w:val=\"left\"/></w:lvl></w:lvlOverride></w:num>".to_owned(),
        ),
        parts: vec![image_part()],
        ..Build::default()
    });
    let mut pkg = OpcPackage::open(&built).unwrap();
    pkg.add_relationship(
        "/word/numbering.xml",
        rel_type::IMAGE,
        "/word/media/image1.png",
        kiln_ooxml::opc::RelationshipOptions {
            id: Some("rIdImg"),
            external: false,
        },
    )
    .unwrap();
    let rt = round_trip(&pkg.save().unwrap());
    assert!(differences(&rt.a, &rt.b).is_empty());
    let numbering = rt.xml("/word/numbering.xml");
    assert!(numbering.contains("w:tplc=\"0409000F\""));
    assert!(numbering.contains("w15:tentative=\"1\""));
    assert!(numbering.contains("<w:tmpl w:val=\"0409001F\"/>"));
    assert!(numbering.contains("w15:restartNumberingAfterBreak=\"0\""));
    // The picture bullet's image is related from the numbering part.
    let pkg = OpcPackage::open(&rt.bytes).unwrap();
    assert_eq!(
        pkg.resolve_relationship("/word/numbering.xml", "rIdImg")
            .unwrap()
            .as_deref(),
        Some("/word/media/image1.png")
    );
}

#[test]
fn settings_are_patched_and_everything_else_kept_in_order() {
    let original = build_docx(Build {
        body: p(&r("x")),
        settings: Some(
            "<w:zoom w:percent=\"120\"/><w:trackRevisions/><w:doNotTrackMoves/><w:defaultTabStop w:val=\"709\"/><w:characterSpacingControl w:val=\"doNotCompress\"/><w:evenAndOddHeaders/><w:updateFields w:val=\"true\"/><w:documentProtection w:edit=\"readOnly\" w:enforcement=\"1\" w:hash=\"abc\"/><w:compat><w:compatSetting w:name=\"compatibilityMode\" w:uri=\"http://schemas.microsoft.com/office/word\" w:val=\"14\"/></w:compat><w:rsids><w:rsidRoot w:val=\"00A1\"/></w:rsids><w:themeFontLang w:val=\"de-DE\"/><w:clrSchemeMapping w:bg1=\"light1\" w:t1=\"dark1\" w:bg2=\"light2\" w:t2=\"dark2\" w:accent1=\"accent1\"/><w:decimalSymbol w:val=\",\"/><w:listSeparator w:val=\";\"/>".to_owned(),
        ),
        ..Build::default()
    });
    let rt = round_trip(&original);
    assert!(differences(&rt.a, &rt.b).is_empty());
    let (bytes, back) = edit_and_write(&original, |doc| {
        doc.settings.track_revisions = false;
        doc.settings.default_tab_stop = 720;
        doc.settings.mirror_margins = true;
    });
    let s = &back.document.settings;
    assert!(!s.track_revisions && s.mirror_margins && s.default_tab_stop == 720);
    let xml = part_text(&bytes, "/word/settings.xml");
    assert!(xml.contains("<w:zoom w:percent=\"120\"/>"));
    assert!(xml.contains("w:hash=\"abc\""));
    assert!(xml.contains("<w:rsidRoot w:val=\"00A1\"/>"));
    let at = |n: &str| xml.find(n).unwrap_or_else(|| panic!("{n} in {xml}"));
    assert!(at("w:zoom") < at("w:mirrorMargins"));
    assert!(at("w:mirrorMargins") < at("w:defaultTabStop"));
    assert!(at("w:defaultTabStop") < at("w:characterSpacingControl"));
}

#[test]
fn word_2007_compatibility_stays_without_a_compat_setting() {
    let rt = same(Build {
        body: p(&r("x")),
        settings: Some("<w:compat><w:useFELayout/></w:compat>".to_owned()),
        ..Build::default()
    });
    let xml = rt.xml("/word/settings.xml");
    assert!(!xml.contains("compatibilityMode"), "{xml}");
    assert_eq!(rt.b.document.settings.compatibility_mode, 12);
}

#[test]
fn the_page_background_colour_comes_back() {
    let mut pkg = OpcPackage::open(&build_docx(Build {
        body: p(&r("x")),
        ..Build::default()
    }))
    .unwrap();
    let xml = String::from_utf8(pkg.part_bytes(MAIN).unwrap().into_owned()).unwrap();
    let with_bg = xml.replace("<w:body>", "<w:background w:color=\"FFF2CC\"/><w:body>");
    pkg.set_part(MAIN, with_bg.into_bytes()).unwrap();
    let rt = round_trip(&pkg.save().unwrap());
    assert!(differences(&rt.a, &rt.b).is_empty());
    assert!(
        rt.xml(MAIN)
            .contains("<w:background w:color=\"FFF2CC\"/><w:body>")
    );
    assert_eq!(
        rt.b.extras.settings.background_color.as_deref(),
        Some("FFF2CC")
    );
}
