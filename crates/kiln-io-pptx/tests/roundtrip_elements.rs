// SPDX-License-Identifier: AGPL-3.0-or-later
//! Construct by construct through read → write → read, on slides built around one construct
//! each: clips with their `p14:media` extension, OLE objects in alternate content, ink and
//! unknown graphics kept as XML, shapes read from alternate content, actions, fields, breaks and
//! equations, legacy and threaded comments. What the model approximates comes back exactly as it
//! was read, its relationships included; LibreOffice opens every result.

mod read_support;

use kiln_deck::shape::{MediaKind, MediaSource, ShapeKind};
use kiln_doc::InlineObject;
use kiln_io_pptx::{
    PptxRead, ShapeSource, WriteOptions, WriteSource, body_text, read_pptx, write_pptx,
};
use kiln_ooxml::opc::OpcPackage;
use read_support::{deck_with_slide, rt, slide_xml, sp};

const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44, 0x41, 0x54,
    0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D,
    0x1D, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Reads a package, writes it with its source and reads the result; LibreOffice must open it.
fn round_trip(pkg: &OpcPackage) -> (PptxRead, PptxRead, OpcPackage) {
    let bytes = pkg.save().expect("save");
    let original = OpcPackage::open(&bytes).expect("opens");
    let a = kiln_io_pptx::read_package(&original).expect("reads");
    let source = WriteSource::from_read(&a).with_package(&original);
    let out = write_pptx(&a.presentation, Some(&source), &WriteOptions::default()).expect("writes");
    assert!(
        out.issues.is_empty(),
        "the writer reported {:?}",
        out.issues
    );
    let b = read_pptx(&out.bytes).expect("reads back");
    if let Ok(oracle) = kiln_testing::oracle::Oracle::locate() {
        let dir = std::env::temp_dir().join(format!(
            "kiln-pptx-rt-{}-{}",
            std::process::id(),
            out.bytes.len()
        ));
        std::fs::create_dir_all(&dir).expect("scratch");
        let file = dir.join("deck.pptx");
        std::fs::write(&file, &out.bytes).expect("write");
        assert!(
            oracle.opens(&file, &dir),
            "LibreOffice cannot open the result"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
    (a, b, OpcPackage::open(&out.bytes).expect("output opens"))
}

fn slide1(pkg: &OpcPackage) -> String {
    String::from_utf8(
        pkg.part("/ppt/slides/slide1.xml")
            .expect("readable")
            .expect("present")
            .into_owned(),
    )
    .expect("utf-8")
}

#[test]
fn clips_keep_their_media_extension_poster_and_trim() {
    let video = r#"<p:pic><p:nvPicPr><p:cNvPr id="4" name="Clip"><a:hlinkClick r:id="" action="ppaction://media"/></p:cNvPr><p:cNvPicPr/><p:nvPr><a:videoFile r:link="rId11"/><p:extLst><p:ext uri="{DAA4B4D4-6D71-4841-9C94-3DE7FCFB9230}"><p14:media r:embed="rId12"><p14:trim st="1500" end="250"/></p14:media></p:ext></p:extLst></p:nvPr></p:nvPicPr><p:blipFill><a:blip r:embed="rId10"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="100" cy="100"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic>"#;
    let audio = r#"<p:pic><p:nvPicPr><p:cNvPr id="5" name="Sound"/><p:cNvPicPr/><p:nvPr><a:audioFile r:link="rId13"/></p:nvPr></p:nvPicPr><p:blipFill><a:blip r:embed="rId10"/></p:blipFill><p:spPr/></p:pic>"#;
    let mut pkg = deck_with_slide(
        &slide_xml("", &format!("{video}{audio}"), ""),
        &[
            ("rId10", &rt("image"), "/ppt/media/poster.png", false),
            ("rId11", &rt("video"), "/ppt/media/clip.mp4", false),
            (
                "rId12",
                "http://schemas.microsoft.com/office/2007/relationships/media",
                "/ppt/media/clip.mp4",
                false,
            ),
            ("rId13", &rt("audio"), "file:///C:/music/song.mp3", true),
        ],
    );
    pkg.add_part("/ppt/media/poster.png", "image/png", PNG.to_vec())
        .unwrap();
    pkg.add_part("/ppt/media/clip.mp4", "video/mp4", b"video".to_vec())
        .unwrap();
    let (a, b, out) = round_trip(&pkg);
    let (sa, sb) = (&a.presentation.slides[0], &b.presentation.slides[0]);
    assert_eq!(sa.shapes.len(), sb.shapes.len());
    for (x, y) in sa.shapes.iter().zip(&sb.shapes) {
        assert_eq!(x.kind, y.kind, "{}", x.name);
        assert_eq!(x.link, y.link, "{}", x.name);
    }
    let ShapeKind::Media(v) = &sb.shapes[0].kind else {
        panic!()
    };
    assert_eq!((v.trim_start_ms, v.trim_end_ms), (1500, 250));
    assert_eq!(v.kind, MediaKind::Video);
    assert_eq!(
        v.source,
        MediaSource::Embedded("/ppt/media/clip.mp4".into())
    );
    // One p14:media extension, not two.
    let xml = slide1(&out);
    assert_eq!(xml.matches("<p14:media").count(), 1, "{xml}");
    assert!(out.has("/ppt/media/clip.mp4"));
}

#[test]
fn ole_objects_keep_their_frames_and_embeddings() {
    let frame = r#"<mc:AlternateContent><mc:Choice xmlns:v="urn:schemas-microsoft-com:vml" Requires="v"><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="6" name="Object 5"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="10" y="20"/><a:ext cx="300" cy="400"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/presentationml/2006/ole"><p:oleObj spid="_x0000_s1026" name="Worksheet" r:id="rId20" imgW="1" imgH="1" progId="Excel.Sheet.12"><p:embed/></p:oleObj></a:graphicData></a:graphic></p:graphicFrame></mc:Choice><mc:Fallback><p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="6" name="Object 5"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="10" y="20"/><a:ext cx="300" cy="400"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/presentationml/2006/ole"><p:oleObj name="Worksheet" r:id="rId20" imgW="1" imgH="1" progId="Excel.Sheet.12"><p:embed/><p:pic><p:nvPicPr><p:cNvPr id="0" name=""/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed="rId21"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x="10" y="20"/><a:ext cx="300" cy="400"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr></p:pic></p:oleObj></a:graphicData></a:graphic></p:graphicFrame></mc:Fallback></mc:AlternateContent>"#;
    let mut pkg = deck_with_slide(
        &slide_xml("", frame, ""),
        &[
            (
                "rId20",
                &rt("package"),
                "/ppt/embeddings/Sheet1.xlsx",
                false,
            ),
            ("rId21", &rt("image"), "/ppt/media/preview.png", false),
        ],
    );
    pkg.add_part(
        "/ppt/embeddings/Sheet1.xlsx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        b"PK".to_vec(),
    )
    .unwrap();
    pkg.add_part("/ppt/media/preview.png", "image/png", PNG.to_vec())
        .unwrap();
    let (_, b, out) = round_trip(&pkg);
    let o = &b.presentation.slides[0].shapes[0];
    assert_eq!(o.name, "Object 5");
    assert!(matches!(
        b.extras.source_of(o.id),
        Some(ShapeSource::Ole { xml }) if xml.contains("Excel.Sheet.12")
    ));
    // The frame's relationships lead where they did.
    let rels = out.relationships("/ppt/slides/slide1.xml").unwrap();
    let target = |id: &str| rels.get(id).and_then(|r| rels.resolve(r));
    assert_eq!(
        target("rId20").as_deref(),
        Some("/ppt/embeddings/Sheet1.xlsx")
    );
    assert_eq!(target("rId21").as_deref(), Some("/ppt/media/preview.png"));
    assert!(out.has("/ppt/embeddings/Sheet1.xlsx"));
}

#[test]
fn moving_an_ole_object_moves_its_kept_frame() {
    let read = {
        let frame = r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="6" name="Object 5"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="10" y="20"/><a:ext cx="300" cy="400"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/presentationml/2006/ole"><p:oleObj name="Worksheet" r:id="rId20" progId="Excel.Sheet.12"><p:embed/></p:oleObj></a:graphicData></a:graphic></p:graphicFrame>"#;
        let mut pkg = deck_with_slide(
            &slide_xml("", frame, ""),
            &[(
                "rId20",
                &rt("package"),
                "/ppt/embeddings/Sheet1.xlsx",
                false,
            )],
        );
        pkg.add_part(
            "/ppt/embeddings/Sheet1.xlsx",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            b"PK".to_vec(),
        )
        .unwrap();
        pkg.save().expect("save")
    };
    let pkg = OpcPackage::open(&read).expect("opens");
    let a = kiln_io_pptx::read_package(&pkg).expect("reads");
    let mut pres = a.presentation.clone();
    let shape = &mut pres.slides[0].shapes[0];
    let mut x = shape.xfrm.expect("placed");
    x.x = 5_000;
    x.cy = 800;
    shape.xfrm = Some(x);
    shape.name = "Budget".into();
    let source = WriteSource::from_read(&a).with_package(&pkg);
    let out = write_pptx(&pres, Some(&source), &WriteOptions::default()).expect("writes");
    let b = read_pptx(&out.bytes).expect("reads");
    let s = &b.presentation.slides[0].shapes[0];
    assert_eq!(s.name, "Budget");
    assert_eq!(s.xfrm.map(|x| (x.x, x.cy)), Some((5_000, 800)));
    assert!(b.extras.is_ole(s.id));
}

#[test]
fn ink_unknown_graphics_and_alternate_content_stay_in_place() {
    let alternate = format!(
        r#"<mc:AlternateContent><mc:Choice Requires="p14">{}</mc:Choice><mc:Fallback>{}</mc:Fallback></mc:AlternateContent>"#,
        sp(7, "New", "<a:p><a:r><a:t>choice</a:t></a:r></a:p>"),
        sp(7, "Old", "<a:p><a:r><a:t>fallback</a:t></a:r></a:p>")
    );
    let tree = format!(
        "{}{alternate}{}{}{}",
        sp(2, "Before", "<a:p><a:r><a:t>first</a:t></a:r></a:p>"),
        r#"<mc:AlternateContent><mc:Choice xmlns:zz="urn:zz" Requires="zz"><zz:thing/></mc:Choice></mc:AlternateContent>"#,
        r#"<p:contentPart r:id="rId30" p14:bwMode="auto"/>"#,
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="Mystery"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="1" cy="1"/></p:xfrm><a:graphic><a:graphicData uri="urn:mystery"><x:y xmlns:x="urn:x"/></a:graphicData></a:graphic></p:graphicFrame>"#
    );
    let mut pkg = deck_with_slide(
        &slide_xml("", &tree, ""),
        &[(
            "rId30",
            "http://schemas.microsoft.com/office/2011/relationships/inkAction",
            "/ppt/ink/ink1.xml",
            false,
        )],
    );
    pkg.add_part(
        "/ppt/ink/ink1.xml",
        "application/inkml+xml",
        b"<ink/>".to_vec(),
    )
    .unwrap();
    let (a, b, out) = round_trip(&pkg);
    let names = |r: &PptxRead| -> Vec<String> {
        r.presentation.slides[0]
            .shapes
            .iter()
            .map(|s| s.name.clone())
            .collect()
    };
    assert_eq!(names(&a), names(&b));
    let opaque = |r: &PptxRead| -> Vec<(usize, bool)> {
        let id = r.presentation.slides[0].id;
        r.extras
            .opaque_shapes
            .iter()
            .filter(|o| o.parent == id)
            .map(|o| (o.index, o.xml.contains("contentPart")))
            .collect()
    };
    assert_eq!(opaque(&a), opaque(&b));
    let xml = slide1(&out);
    // The shape read from the choice goes back inside its alternate content, fallback and all.
    assert!(xml.contains("<mc:Fallback>"), "{xml}");
    assert!(xml.contains(">fallback<"), "{xml}");
    let rels = out.relationships("/ppt/slides/slide1.xml").unwrap();
    assert!(rels.get("rId30").is_some());
    assert!(out.has("/ppt/ink/ink1.xml"));
}

#[test]
fn actions_fields_breaks_and_equations_round_trip() {
    let body = r#"<a:p><a:r><a:rPr lang="en-US" b="1"><a:hlinkClick r:id="" action="ppaction://hlinkshowjump?jump=nextslide" tooltip="Next"/></a:rPr><a:t>Next</a:t></a:r><a:br><a:rPr lang="en-US"/></a:br><a:fld id="{1}" type="slidenum"><a:rPr lang="en-US"/><a:t>‹#›</a:t></a:fld><a:r><a:t>a_x000B_b</a:t></a:r></a:p><a:p><mc:AlternateContent><mc:Choice Requires="a14"><a14:m><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara></a14:m></mc:Choice><mc:Fallback><a:r><a:t>[x]</a:t></a:r></mc:Fallback></mc:AlternateContent><a:endParaRPr lang="en-US" sz="2000"/></a:p>"#;
    let (a, b, _) = round_trip(&deck_with_slide(&slide_xml("", &sp(2, "T", body), ""), &[]));
    let (ta, tb) = (
        a.presentation.slides[0].shapes[0].text().unwrap(),
        b.presentation.slides[0].shapes[0].text().unwrap(),
    );
    assert_eq!(body_text(ta), body_text(tb));
    for (pa, pb) in ta.paragraphs().zip(tb.paragraphs()) {
        assert_eq!(pa.content.runs(), pb.content.runs());
        assert_eq!(pa.content.marks(), pb.content.marks());
        assert_eq!(pa.mark_props, pb.mark_props);
        let (oa, ob) = (pa.content.objects(), pb.content.objects());
        assert_eq!(oa.len(), ob.len());
        for (x, y) in oa.iter().zip(ob) {
            match (x, y) {
                (InlineObject::Equation(e), InlineObject::Equation(f)) => {
                    assert_eq!(e.display, f.display);
                    let strip = |s: &str| {
                        kiln_ooxml::xml::parse_xml_str(s)
                            .map(|d| d.root.text())
                            .unwrap_or_default()
                    };
                    assert_eq!(strip(&e.omml), strip(&f.omml));
                }
                _ => assert_eq!(x, y),
            }
        }
    }
}

#[test]
fn legacy_and_threaded_comments_round_trip() {
    let mut pkg = deck_with_slide(
        &slide_xml("", &sp(2, "Box", "<a:p><a:r><a:t>x</a:t></a:r></a:p>"), ""),
        &[(
            "rId8",
            "http://schemas.microsoft.com/office/2018/10/relationships/comments",
            "/ppt/comments/modernComment_1.xml",
            false,
        )],
    );
    pkg.add_part(
        "/ppt/comments/modernComment_1.xml",
        "application/vnd.ms-powerpoint.comments+xml",
        r#"<p188:cmLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:cm id="{A}" authorId="{B}" created="2026-01-04T05:06:07.000" status="resolved"><p188:pos x="100" y="200"/><p188:replyLst><p188:reply id="{C}" authorId="{B}" created="2026-01-05T00:00:00.000"><p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Done</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst><p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Modern</a:t></a:r></a:p><a:p><a:r><a:t>note</a:t></a:r></a:p></p188:txBody></p188:cm></p188:cmLst>"#,
    )
    .unwrap();
    pkg.add_part(
        "/ppt/authors.xml",
        "application/vnd.ms-powerpoint.authors+xml",
        r#"<p188:authorLst xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:author id="{B}" name="Bea Ruiz" initials="BR" userId="bea" providerId="None"/></p188:authorLst>"#,
    )
    .unwrap();
    pkg.add_relationship(
        "/ppt/presentation.xml",
        "http://schemas.microsoft.com/office/2018/10/relationships/authors",
        "/ppt/authors.xml",
        kiln_ooxml::opc::RelationshipOptions::default(),
    )
    .unwrap();
    let (a, b, out) = round_trip(&pkg);
    assert_eq!(
        a.presentation.slides[0].comments.len(),
        b.presentation.slides[0].comments.len()
    );
    for (x, y) in a.presentation.slides[0]
        .comments
        .iter()
        .zip(&b.presentation.slides[0].comments)
    {
        assert_eq!(
            (
                &x.author,
                &x.initials,
                &x.created,
                &x.text,
                x.pos,
                x.resolved
            ),
            (
                &y.author,
                &y.initials,
                &y.created,
                &y.text,
                y.pos,
                y.resolved
            )
        );
        assert_eq!(x.replies.len(), y.replies.len());
        assert_eq!(x.replies[0].text, y.replies[0].text);
        assert_eq!(x.replies[0].author, y.replies[0].author);
    }
    assert!(b.extras.modern_comments);
    // The slide links its comments through its extension list.
    let xml = slide1(&out);
    assert!(xml.contains("p188:commentRel"), "{xml}");
    assert!(out.has("/ppt/comments/modernComment_1.xml"));
}
