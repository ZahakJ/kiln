// SPDX-License-Identifier: AGPL-3.0-or-later
//! Element by element: slides built around one construct each (comments, media, OLE objects,
//! alternate content, ink, transitions in `mc:AlternateContent`, timing with triggers and
//! unsupported effects, actions, fields, equations, ragged tables, SmartArt without a drawing,
//! missing and linked images, deep groups, clamped geometry, unknown placeholders, picture
//! bullets and fills), read through a real package.

mod read_support;

use kiln_deck::animation::{AnimationEffect, EffectClass, EffectStart};
use kiln_deck::shape::{MediaKind, MediaSource, PlaceholderType, ShapeKind};
use kiln_deck::transition::TransitionEffect;
use kiln_deck::{Bullet, Presentation};
use kiln_doc::InlineObject;
use kiln_io_pptx::{PptxError, ShapeSource, WarningCode, body_text, read_pptx};
use kiln_ooxml::drawingml::fill::Fill;
use kiln_ooxml::opc::RelationshipOptions;
use read_support::{deck_with_slide, read_package_bytes, rt, slide_xml, sp};

const PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, 0x49, 0x48, 0x44, 0x52, 0, 0, 0,
    1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1F, 0x15, 0xC4, 0x89, 0, 0, 0, 0x0D, 0x49, 0x44, 0x41, 0x54,
    0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0, 0x1F, 0, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D,
    0x1D, 0, 0, 0, 0, 0x49, 0x45, 0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

fn has(read: &kiln_io_pptx::PptxRead, code: WarningCode) -> bool {
    read.warnings.iter().any(|w| w.code == code)
}

fn first_slide(read: &kiln_io_pptx::PptxRead) -> &kiln_deck::Slide {
    &read.presentation.slides[0]
}

#[test]
fn legacy_and_modern_comments_with_authors_and_replies() {
    let mut pkg = deck_with_slide(
        &slide_xml("", &sp(2, "Box", "<a:p><a:r><a:t>x</a:t></a:r></a:p>"), ""),
        &[
            ("rId7", &rt("comments"), "/ppt/comments/comment1.xml", false),
            (
                "rId8",
                "http://schemas.microsoft.com/office/2018/10/relationships/comments",
                "/ppt/comments/modernComment_1.xml",
                false,
            ),
        ],
    );
    pkg.add_part(
        "/ppt/comments/comment1.xml",
        "application/vnd.openxmlformats-officedocument.presentationml.comments+xml",
        r#"<p:cmLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cm authorId="0" dt="2026-01-02T03:04:05.000" idx="1"><p:pos x="576" y="1152"/><p:text>Legacy note</p:text></p:cm><p:cm authorId="9" dt="2026-01-03T00:00:00.000" idx="2"><p:pos x="0" y="0"/><p:text>Who?</p:text></p:cm></p:cmLst>"#,
    )
    .unwrap();
    pkg.add_part(
        "/ppt/commentAuthors.xml",
        "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml",
        r#"<p:cmAuthorLst xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main"><p:cmAuthor id="0" name="Ana Lima" initials="AL" lastIdx="1" clrIdx="0"/></p:cmAuthorLst>"#,
    )
    .unwrap();
    pkg.add_relationship(
        "/ppt/presentation.xml",
        &rt("commentAuthors"),
        "/ppt/commentAuthors.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    pkg.add_part(
        "/ppt/comments/modernComment_1.xml",
        "application/vnd.ms-powerpoint.comments+xml",
        r#"<p188:cmLst xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:cm id="{A}" authorId="{B}" created="2026-02-01T10:00:00.000" status="resolved"><p188:pos x="1000" y="2000"/><p188:replyLst><p188:reply id="{C}" authorId="{B}" created="2026-02-01T11:00:00.000"><p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Done</a:t></a:r></a:p></p188:txBody></p188:reply></p188:replyLst><p188:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>Line one</a:t></a:r></a:p><a:p><a:r><a:t>Line two</a:t></a:r></a:p></p188:txBody></p188:cm></p188:cmLst>"#,
    )
    .unwrap();
    pkg.add_part(
        "/ppt/authors.xml",
        "application/vnd.ms-powerpoint.authors+xml",
        r#"<p188:authorLst xmlns:p188="http://schemas.microsoft.com/office/powerpoint/2018/8/main"><p188:author id="{B}" name="Ben Ode" initials="BO" userId="ben" providerId="None"/></p188:authorLst>"#,
    )
    .unwrap();
    pkg.add_relationship(
        "/ppt/presentation.xml",
        "http://schemas.microsoft.com/office/2018/10/relationships/authors",
        "/ppt/authors.xml",
        RelationshipOptions::default(),
    )
    .unwrap();
    let read = read_package_bytes(&pkg);
    let c = &first_slide(&read).comments;
    assert_eq!(c.len(), 3);
    assert_eq!(
        (c[0].author.as_str(), c[0].initials.as_str()),
        ("Ana Lima", "AL")
    );
    assert_eq!(c[0].text, "Legacy note");
    assert_eq!(c[0].pos, (914_400, 1_828_800), "576 master units per inch");
    assert_eq!(c[0].created, "2026-01-02T03:04:05.000");
    assert_eq!(c[1].author, "", "unknown author");
    assert!(has(&read, WarningCode::CommentAuthorMissing));
    assert_eq!(c[2].author, "Ben Ode");
    assert_eq!(c[2].text, "Line one\nLine two");
    assert!(c[2].resolved);
    assert_eq!(c[2].pos, (1000, 2000));
    assert_eq!(c[2].replies.len(), 1);
    assert_eq!(c[2].replies[0].text, "Done");
    assert!(read.extras.modern_comments);
    for p in [
        "/ppt/comments/comment1.xml",
        "/ppt/authors.xml",
        "/ppt/commentAuthors.xml",
    ] {
        assert!(!read.passthrough.has(p), "{p} is modelled");
    }
}

#[test]
fn video_with_poster_trim_and_linked_audio() {
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
    pkg.add_part(
        "/ppt/media/clip.mp4",
        "video/mp4",
        b"not really a video".to_vec(),
    )
    .unwrap();
    let read = read_package_bytes(&pkg);
    let s = first_slide(&read);
    let ShapeKind::Media(v) = &s.shapes[0].kind else {
        panic!("{:?}", s.shapes[0].kind)
    };
    assert_eq!(v.kind, MediaKind::Video);
    assert_eq!(
        v.source,
        MediaSource::Embedded("/ppt/media/clip.mp4".into())
    );
    assert_eq!(v.poster.as_deref(), Some("/ppt/media/poster.png"));
    assert_eq!((v.trim_start_ms, v.trim_end_ms), (1500, 250));
    assert_eq!(s.shapes[0].link.as_deref(), Some("ppaction://media"));
    assert_eq!(
        read.presentation.parts["/ppt/media/clip.mp4"].content_type,
        "video/mp4"
    );
    let ShapeKind::Media(a) = &s.shapes[1].kind else {
        panic!()
    };
    assert_eq!(a.kind, MediaKind::Audio);
    assert_eq!(
        a.source,
        MediaSource::Linked("file:///C:/music/song.mp3".into())
    );
    assert!(has(&read, WarningCode::MediaLinked));
}

#[test]
fn ole_objects_show_their_preview_and_keep_their_xml() {
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
    let read = read_package_bytes(&pkg);
    let s = first_slide(&read);
    assert_eq!(s.shapes.len(), 1);
    let o = &s.shapes[0];
    assert_eq!(o.name, "Object 5");
    let ShapeKind::Picture(p) = &o.kind else {
        panic!()
    };
    assert_eq!(p.media, "/ppt/media/preview.png");
    assert_eq!(o.xfrm.unwrap().cx, 300);
    let Some(ShapeSource::Ole { xml }) = read.extras.source_of(o.id) else {
        panic!("{:?}", read.extras.source_of(o.id))
    };
    assert!(xml.starts_with("<mc:AlternateContent"), "{xml}");
    assert!(xml.contains("Excel.Sheet.12"));
    assert_eq!(kiln_io_pptx::shape_kind(o, &read.extras), "ole");
    // The embedded workbook stays in the package with the slide's relationship to it.
    assert!(read.passthrough.has("/ppt/embeddings/Sheet1.xlsx"));
    assert!(
        read.passthrough
            .relationships()
            .iter()
            .any(|r| r.source == "/ppt/slides/slide1.xml" && r.rel.id == "rId20")
    );
    assert!(has(&read, WarningCode::OleObject));
}

#[test]
fn alternate_content_ink_and_unknown_graphics() {
    // Newer content with a fallback: the choice is read when Kiln knows it.
    let alternate = format!(
        r#"<mc:AlternateContent><mc:Choice Requires="p14">{}</mc:Choice><mc:Fallback>{}</mc:Fallback></mc:AlternateContent>"#,
        sp(7, "New", "<a:p><a:r><a:t>choice</a:t></a:r></a:p>"),
        sp(7, "Old", "<a:p><a:r><a:t>fallback</a:t></a:r></a:p>")
    );
    let tree = format!(
        "{alternate}{}{}{}",
        // A choice nobody knows, without fallback: kept.
        r#"<mc:AlternateContent><mc:Choice xmlns:zz="urn:zz" Requires="zz"><zz:thing/></mc:Choice></mc:AlternateContent>"#,
        r#"<p:contentPart r:id="rId30" p14:bwMode="auto"/>"#,
        r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="9" name="Mystery"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="1" cy="1"/></p:xfrm><a:graphic><a:graphicData uri="urn:mystery"><x:y xmlns:x="urn:x"/></a:graphicData></a:graphic></p:graphicFrame>"#
    );
    let read = read_package_bytes(&deck_with_slide(&slide_xml("", &tree, ""), &[]));
    let s = first_slide(&read);
    assert_eq!(s.shapes.len(), 1);
    assert_eq!(s.shapes[0].name, "New");
    assert!(matches!(
        read.extras.source_of(s.shapes[0].id),
        Some(ShapeSource::AlternateContent { .. })
    ));
    let opaque: Vec<_> = read
        .extras
        .opaque_shapes
        .iter()
        .filter(|o| o.parent == s.id)
        .collect();
    assert_eq!(opaque.len(), 3);
    assert!(opaque.iter().all(|o| o.index == 1));
    assert!(opaque[1].xml.contains("contentPart"));
    assert!(
        opaque[0].xml.contains("xmlns:zz=\"urn:zz\""),
        "{}",
        opaque[0].xml
    );
    assert!(has(&read, WarningCode::ContentPart));
    assert!(has(&read, WarningCode::ShapeUnknown));
}

#[test]
fn transitions_inside_alternate_content_and_timing() {
    let tree = format!(
        "{}{}{}",
        sp(
            2,
            "A",
            "<a:p><a:r><a:t>a</a:t></a:r></a:p><a:p><a:r><a:t>b</a:t></a:r></a:p>"
        ),
        sp(3, "B", "<a:p><a:r><a:t>b</a:t></a:r></a:p>"),
        sp(4, "Button", "<a:p><a:r><a:t>go</a:t></a:r></a:p>")
    );
    let effect = |id: u32, class: &str, preset: u32, node: &str, spid: u32, extra: &str| {
        format!(
            r#"<p:par><p:cTn id="{id}" presetID="{preset}" presetClass="{class}" presetSubtype="0" fill="hold" nodeType="{node}"><p:stCondLst><p:cond delay="250"/></p:stCondLst><p:childTnLst><p:set><p:cBhvr><p:cTn id="{}" dur="1" fill="hold"/><p:tgtEl><p:spTgt spid="{spid}"/></p:tgtEl></p:cBhvr></p:set>{extra}</p:childTnLst></p:cTn></p:par>"#,
            id + 100
        )
    };
    let spin = r#"<p:animRot by="-10800000"><p:cBhvr><p:cTn id="300" dur="2000" fill="hold"/><p:tgtEl><p:spTgt spid="3"/></p:tgtEl></p:cBhvr></p:animRot>"#;
    let main = format!(
        r#"<p:par><p:cTn id="3" fill="hold"><p:stCondLst><p:cond delay="indefinite"/></p:stCondLst><p:childTnLst><p:par><p:cTn id="4" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>{}{}</p:childTnLst></p:cTn></p:par><p:par><p:cTn id="5" fill="hold"><p:stCondLst><p:cond delay="1"/></p:stCondLst><p:childTnLst>{}{}{}</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>"#,
        effect(10, "entr", 10, "clickEffect", 2, ""),
        effect(11, "emph", 8, "withEffect", 3, spin),
        effect(
            12,
            "path",
            0,
            "afterEffect",
            2,
            r#"<p:animMotion origin="layout" path="M 0 0 L 0.25 0.33 E"/>"#
        ),
        effect(13, "exit", 10, "afterEffect", 99, ""),
        effect(14, "entr", 2, "afterEffect", 2, ""),
    );
    let timing = format!(
        r#"<p:timing><p:tnLst><p:par><p:cTn id="1" dur="indefinite" restart="never" nodeType="tmRoot"><p:childTnLst><p:seq concurrent="1" nextAc="seek"><p:cTn id="2" dur="indefinite" nodeType="mainSeq"><p:childTnLst>{main}</p:childTnLst></p:cTn></p:seq><p:seq concurrent="1" nextAc="seek"><p:cTn id="20" restart="whenNotActive" fill="hold" evtFilter="cancelBubble" nodeType="interactiveSeq"><p:stCondLst><p:cond evt="onClick" delay="0"><p:tgtEl><p:spTgt spid="4"/></p:tgtEl></p:cond></p:stCondLst><p:childTnLst><p:par><p:cTn id="21" fill="hold"><p:stCondLst><p:cond delay="0"/></p:stCondLst><p:childTnLst>{}</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst><p:bldLst><p:bldP spid="2" grpId="0" build="p"/></p:bldLst></p:timing>"#,
        effect(22, "entr", 1, "clickEffect", 3, "")
    );
    let transition = r#"<mc:AlternateContent><mc:Choice Requires="p14"><p:transition spd="slow" p14:dur="1600" advClick="0" advTm="5000"><p14:reveal dir="r"/></p:transition></mc:Choice><mc:Fallback><p:transition spd="slow" advClick="0" advTm="5000"><p:fade/></p:transition></mc:Fallback></mc:AlternateContent>"#;
    let read = read_package_bytes(&deck_with_slide(
        &slide_xml("", &tree, &format!("{transition}{timing}")),
        &[],
    ));
    let s = first_slide(&read);
    let t = s.transition.as_ref().unwrap();
    assert!(matches!(t.effect, TransitionEffect::Reveal { .. }));
    assert_eq!(t.duration_ms, 1600);
    assert!(!t.advance_on_click);
    assert_eq!(t.advance_after_ms, Some(5000));
    assert!(t.original.is_none());
    assert!(read.extras.transitions[&s.id].starts_with("<mc:AlternateContent"));
    // The transition's block is modelled, not kept as an unknown root child.
    assert!(
        read.extras
            .owner_xml
            .get(&s.id)
            .is_none_or(|v| v.iter().all(|x| !x.contains("transition")))
    );

    let seq = &s.timeline.sequence;
    assert_eq!(
        seq.len(),
        4,
        "the effect on a missing shape is skipped: {seq:?}"
    );
    assert!(has(&read, WarningCode::TimingUnreadable));
    assert_eq!(seq[0].effect, AnimationEffect::Fade);
    assert_eq!(seq[0].start, EffectStart::OnClick);
    assert_eq!(seq[0].delay_ms, 250);
    assert_eq!(seq[1].effect, AnimationEffect::Spin { degrees: -180 });
    assert_eq!(seq[1].start, EffectStart::WithPrevious);
    assert_eq!(seq[1].duration_ms, 2000);
    let AnimationEffect::Other { class, xml, .. } = &seq[2].effect else {
        panic!("{:?}", seq[2].effect)
    };
    assert_eq!(*class, EffectClass::Path);
    assert!(xml.as_deref().unwrap().contains("animMotion"));
    assert!(has(&read, WarningCode::AnimationUnsupported));
    assert!(matches!(seq[3].effect, AnimationEffect::FlyIn { .. }));
    assert_eq!(seq[3].start, EffectStart::AfterPrevious);
    assert_eq!(s.timeline.triggers.len(), 1);
    assert_eq!(s.timeline.triggers[0].trigger, s.shapes[2].id);
    assert_eq!(
        s.timeline.triggers[0].effects[0].effect,
        AnimationEffect::Appear
    );
    assert!(read.extras.timing[&s.id].contains("bldLst"));
}

#[test]
fn actions_fields_breaks_and_equations() {
    let body = r#"<a:p><a:r><a:rPr lang="en-US" b="1"><a:hlinkClick r:id="" action="ppaction://hlinkshowjump?jump=nextslide" tooltip="Next"/></a:rPr><a:t>Next</a:t></a:r><a:br><a:rPr lang="en-US"/></a:br><a:fld id="{1}" type="slidenum"><a:rPr lang="en-US"/><a:t>‹#›</a:t></a:fld><a:r><a:t>a_x000B_b</a:t></a:r></a:p><a:p><mc:AlternateContent><mc:Choice Requires="a14"><a14:m><m:oMathPara><m:oMath><m:r><m:t>x</m:t></m:r></m:oMath></m:oMathPara></a14:m></mc:Choice><mc:Fallback><a:r><a:t>[x]</a:t></a:r></mc:Fallback></mc:AlternateContent><a:endParaRPr lang="en-US" sz="2000"/></a:p>"#;
    let read = read_package_bytes(&deck_with_slide(&slide_xml("", &sp(2, "T", body), ""), &[]));
    let t = first_slide(&read).shapes[0].text().unwrap();
    assert_eq!(body_text(t), "Next\u{b}‹#›a\u{b}b\n");
    let paras: Vec<_> = t.paragraphs().collect();
    let objects = paras[0].content.objects();
    assert!(matches!(&objects[1], InlineObject::Field(f) if f.instr == "slidenum"));
    assert!(matches!(objects[0], InlineObject::Break(_)));
    let mark = &paras[0].content.marks()[0];
    let kiln_doc::Mark::Link(h) = &mark.mark else {
        panic!()
    };
    assert_eq!(
        h.target,
        kiln_doc::LinkTarget::Url("ppaction://hlinkshowjump?jump=nextslide".into())
    );
    assert_eq!(h.tooltip.as_deref(), Some("Next"));
    assert_eq!((mark.start, mark.end), (0, 4));
    assert!(
        matches!(&paras[1].content.objects()[0], InlineObject::Equation(e) if e.display && e.omml.contains("m:oMath"))
    );
    assert_eq!(paras[1].mark_props.size, Some(40), "20 pt");
    assert_eq!(paras[0].content.runs()[0].props.bold, Some(true));
}

#[test]
fn ragged_and_empty_tables() {
    let table = |rows: &str| {
        format!(
            r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="5" name="Table 4"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="200" cy="200"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/table"><a:tbl><a:tblPr firstRow="1"/><a:tblGrid><a:gridCol w="100"/><a:gridCol w="100"/></a:tblGrid>{rows}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>"#
        )
    };
    let cell = |t: &str| {
        format!(
            r#"<a:tc><a:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t>{t}</a:t></a:r></a:p></a:txBody><a:tcPr marL="1" anchor="ctr"><a:lnL w="12700"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:lnL><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></a:tcPr></a:tc>"#
        )
    };
    let rows = format!(
        r#"<a:tr h="100">{}</a:tr><a:tr h="100">{}{}{}</a:tr>"#,
        cell("a"),
        cell("b"),
        cell("c"),
        cell("dropped")
    );
    let tree = format!("{}{}", table(&rows), table(""));
    let read = read_package_bytes(&deck_with_slide(&slide_xml("", &tree, ""), &[]));
    let s = first_slide(&read);
    assert_eq!(s.shapes.len(), 1, "the empty table is kept as XML");
    let ShapeKind::Table(t) = &s.shapes[0].kind else {
        panic!()
    };
    assert_eq!(t.rows[0].cells.len(), 2);
    assert_eq!(body_text(&t.rows[0].cells[1].text), "");
    assert_eq!(body_text(&t.rows[1].cells[1].text), "c");
    let c = &t.rows[0].cells[0];
    assert_eq!(c.props.margins[0], Some(1));
    assert!(c.props.borders[0].0.is_some() && c.props.borders[1].0.is_none());
    assert!(matches!(c.props.fill, Some(Fill::Solid(_))));
    assert!(has(&read, WarningCode::TableIrregular));
    assert_eq!(read.extras.opaque_shapes.len(), 1);
}

#[test]
fn smart_art_without_a_drawing_is_an_empty_frame() {
    let frame = r#"<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id="4" name="Diagram 3"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm><a:off x="0" y="0"/><a:ext cx="500" cy="500"/></p:xfrm><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/diagram"><dgm:relIds xmlns:dgm="http://schemas.openxmlformats.org/drawingml/2006/diagram" r:dm="rId40" r:lo="rId41" r:qs="rId42" r:cs="rId43"/></a:graphicData></a:graphic></p:graphicFrame>"#;
    let read = read_package_bytes(&deck_with_slide(&slide_xml("", frame, ""), &[]));
    let s = first_slide(&read);
    assert!(matches!(s.shapes[0].kind, ShapeKind::Picture(_)));
    assert_eq!(
        kiln_io_pptx::shape_kind(&s.shapes[0], &read.extras),
        "diagram"
    );
    assert!(
        read.extras
            .source_of(s.shapes[0].id)
            .unwrap()
            .xml()
            .contains("relIds")
    );
}

#[test]
fn missing_linked_and_filled_images_bullets_and_clamped_geometry() {
    let pic = |id: u32, blip: &str| {
        format!(
            r#"<p:pic><p:nvPicPr><p:cNvPr id="{id}" name="P{id}"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill>{blip}</p:blipFill><p:spPr/></p:pic>"#
        )
    };
    let filled = r#"<p:sp><p:nvSpPr><p:cNvPr id="8" name="Filled"/><p:cNvSpPr/><p:nvPr><p:ph type="weird" idx="3"/></p:nvPr></p:nvSpPr><p:spPr><a:xfrm rot="21600000"><a:off x="99999999999999999" y="0"/><a:ext cx="10" cy="10"/></a:xfrm><a:custGeom><a:pathLst><a:path w="10" h="10"><a:moveTo><a:pt x="0" y="0"/></a:moveTo><a:lnTo><a:pt x="10" y="10"/></a:lnTo></a:path></a:pathLst></a:custGeom><a:blipFill><a:blip r:embed="rId50"/><a:stretch/></a:blipFill><a:scene3d><a:camera prst="orthographicFront"/><a:lightRig rig="threePt" dir="t"/></a:scene3d></p:spPr><p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr><a:buBlip><a:blip r:embed="rId50"/></a:buBlip></a:lvl1pPr></a:lstStyle><a:p><a:pPr><a:buBlip><a:blip r:embed="rId50"/></a:buBlip></a:pPr><a:r><a:t>x</a:t></a:r></a:p></p:txBody></p:sp>"#;
    let tree = format!(
        "{}{}{}",
        pic(6, r#"<a:blip r:embed="rId99"/>"#),
        pic(7, r#"<a:blip r:link="rId51"/>"#),
        filled
    );
    let mut pkg = deck_with_slide(
        &slide_xml("", &tree, ""),
        &[
            ("rId50", &rt("image"), "/ppt/media/tex.png", false),
            ("rId51", &rt("image"), "https://example.com/a.png", true),
        ],
    );
    pkg.add_part("/ppt/media/tex.png", "image/png", PNG.to_vec())
        .unwrap();
    let read = read_package_bytes(&pkg);
    assert!(has(&read, WarningCode::ImageMissing));
    assert!(has(&read, WarningCode::ImageLinked));
    assert!(has(&read, WarningCode::PlaceholderUnknown));
    assert!(has(&read, WarningCode::ShapeClamped));
    let s = first_slide(&read);
    let f = &s.shapes[2];
    assert_eq!(f.placeholder.unwrap().kind, PlaceholderType::Object);
    assert_eq!(f.placeholder.unwrap().idx, 3);
    assert!(f.xfrm.unwrap().is_valid());
    assert_eq!(f.xfrm.unwrap().rot, 0);
    let Some(Fill::Blip(b)) = &f.props().unwrap().fill else {
        panic!()
    };
    assert_eq!(b.embed.as_deref(), Some("/ppt/media/tex.png"));
    let t = f.text().unwrap();
    let p = t.paragraphs().next().unwrap();
    assert_eq!(
        t.deck_props(p.id).unwrap().bullet,
        Some(Bullet::Picture("/ppt/media/tex.png".into()))
    );
    assert_eq!(
        t.list_style.level(0).deck.bullet,
        Some(Bullet::Picture("/ppt/media/tex.png".into()))
    );
    assert!(
        read.extras.shape_xml[&f.id]
            .iter()
            .any(|x| x.contains("scene3d"))
    );
    assert_eq!(kiln_io_pptx::shape_kind(f, &read.extras), "placeholder");
}

#[test]
fn groups_nested_too_deeply_are_kept() {
    let mut inner = sp(2, "Leaf", "<a:p/>");
    for i in 0..40 {
        inner = format!(
            r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="{}" name="G{i}"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="10" cy="10"/><a:chOff x="0" y="0"/><a:chExt cx="10" cy="10"/></a:xfrm></p:grpSpPr>{inner}</p:grpSp>"#,
            100 + i
        );
    }
    let read = read_package_bytes(&deck_with_slide(&slide_xml("", &inner, ""), &[]));
    assert!(has(&read, WarningCode::NestingTooDeep));
    assert_eq!(read.extras.opaque_shapes.len(), 1);
    let mut depth = 0;
    let mut s = &first_slide(&read).shapes[0];
    while let Some(c) = s.children().and_then(|c| c.first()) {
        depth += 1;
        s = c;
    }
    assert!(depth >= 30, "{depth}");
}

#[test]
fn slides_on_missing_layouts_and_missing_slides() {
    let mut pkg = deck_with_slide(&slide_xml("", "", ""), &[]);
    pkg.remove_relationship("/ppt/slides/slide1.xml", "rId1")
        .unwrap();
    // A slide list entry pointing at nothing.
    let pres = String::from_utf8(
        pkg.part_bytes("/ppt/presentation.xml")
            .unwrap()
            .into_owned(),
    )
    .unwrap();
    let pres = pres.replacen(
        "<p:sldIdLst>",
        r#"<p:sldIdLst><p:sldId id="999" r:id="rId404"/>"#,
        1,
    );
    pkg.set_part("/ppt/presentation.xml", pres.into_bytes())
        .unwrap();
    let read = read_package_bytes(&pkg);
    assert_eq!(read.presentation.slides.len(), 2);
    assert!(has(&read, WarningCode::SlideSkipped));
    assert!(has(&read, WarningCode::PartMissing));
    let first_layout = read.presentation.masters[0].layouts[0].id;
    assert_eq!(read.presentation.slides[0].layout, first_layout);
}

#[test]
fn inconsistent_sections_are_dropped() {
    let mut pkg = read_support::package("pptx/features/sections.pptx");
    let pres = String::from_utf8(
        pkg.part_bytes("/ppt/presentation.xml")
            .unwrap()
            .into_owned(),
    )
    .unwrap();
    // Swap the first two sections' slide lists: slides then go back to an earlier section.
    let start = pres.find("<p14:sldIdLst>").unwrap();
    let a_end = pres[start..].find("</p14:sldIdLst>").unwrap() + start + "</p14:sldIdLst>".len();
    let b_start = pres[a_end..].find("<p14:sldIdLst>").unwrap() + a_end;
    let b_end =
        pres[b_start..].find("</p14:sldIdLst>").unwrap() + b_start + "</p14:sldIdLst>".len();
    let swapped = format!(
        "{}{}{}{}{}",
        &pres[..start],
        &pres[b_start..b_end],
        &pres[a_end..b_start],
        &pres[start..a_end],
        &pres[b_end..]
    );
    pkg.set_part("/ppt/presentation.xml", swapped.into_bytes())
        .unwrap();
    let read = read_package_bytes(&pkg);
    assert!(read.presentation.sections.is_empty());
    assert!(has(&read, WarningCode::SectionsInconsistent));
}

#[test]
fn packages_that_are_not_presentations_are_refused() {
    assert!(matches!(
        read_pptx(b"not a zip"),
        Err(PptxError::Package(_))
    ));
    let docx = std::fs::read(read_support::fixtures().join("docx/features/tables.docx")).unwrap();
    let e = read_pptx(&docx).unwrap_err();
    assert!(matches!(e, PptxError::NotPresentation(_)), "{e}");
    assert!(e.to_string().contains("wordprocessingml"), "{e}");
    let mut pkg = read_support::package("pptx/features/aspect-16x9.pptx");
    pkg.set_part("/ppt/presentation.xml", b"<p:presentation".to_vec())
        .unwrap();
    assert!(matches!(
        read_pptx(&pkg.save().unwrap()),
        Err(PptxError::NotPresentation(_))
    ));
    let mut pkg = read_support::package("pptx/features/aspect-16x9.pptx");
    pkg.set_part(
        "/ppt/presentation.xml",
        br#"<x:other xmlns:x="urn:x"/>"#.to_vec(),
    )
    .unwrap();
    assert!(matches!(
        read_pptx(&pkg.save().unwrap()),
        Err(PptxError::NotPresentation(_))
    ));
}

#[test]
fn broken_parts_become_diagnostics() {
    // A malformed slide is skipped; a malformed layout falls back; the rest opens.
    let mut pkg = read_support::package("pptx/features/placeholders.pptx");
    pkg.set_part("/ppt/slides/slide2.xml", b"<p:sld><unclosed>".to_vec())
        .unwrap();
    pkg.set_part("/ppt/slideLayouts/slideLayout3.xml", b"garbage".to_vec())
        .unwrap();
    pkg.set_part("/ppt/theme/theme1.xml", b"<nope/>".to_vec())
        .unwrap();
    let read = read_package_bytes(&pkg);
    assert_eq!(read.presentation.slides.len(), 10);
    assert!(has(&read, WarningCode::PartInvalidXml));
    assert_eq!(read.presentation.masters[0].layouts.len(), 10);
    // The slide on the broken layout uses the first layout.
    assert!(
        read.presentation
            .slides
            .iter()
            .all(|s| read.presentation.layout(s.layout).is_some())
    );
    // The model is still sound.
    let back = Presentation::from_bytes(&read.presentation.to_bytes().unwrap()).unwrap();
    assert_eq!(back, read.presentation);
}
