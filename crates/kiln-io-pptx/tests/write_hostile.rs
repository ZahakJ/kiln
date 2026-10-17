// SPDX-License-Identifier: AGPL-3.0-or-later
//! The writer under pressure: text and names XML cannot hold, missing media and charts, links to
//! slides that are gone, kept XML that does not parse, deep groups on a small stack, notes
//! without a notes master, empty presentations, macros saved to a format without them, and
//! random deletions from every fixture (proptest). Nothing panics; every output reads back.

mod read_support;

use kiln_common::NodeId;
use kiln_deck::Presentation;
use kiln_deck::geom::{ChildSpace, Rect, Transform};
use kiln_deck::shape::{ChartRef, Group, Picture, Shape, ShapeKind, ShapeProps};
use kiln_deck::text::TextBody;
use kiln_io_pptx::{PptxKind, WriteOptions, WriteSource, read_pptx, rewrite_pptx, write_pptx};
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions};
use proptest::prelude::*;

fn slide_with(p: &mut Presentation, shapes: Vec<Shape>) {
    let s = &mut p.slides[0];
    s.shapes = shapes;
    p.refresh_id_counter();
}

fn issues_of(p: &Presentation) -> Vec<String> {
    let out = write_pptx(p, None, &WriteOptions::default()).expect("writes");
    read_pptx(&out.bytes).expect("reads back");
    out.issues.into_iter().map(|i| i.code).collect()
}

#[test]
fn text_and_names_xml_cannot_hold_are_cleaned() {
    let mut p = Presentation::new();
    let id = p.alloc_id();
    let mut s = Shape::text_box(
        id,
        Rect::new(0, 0, 100, 100),
        "nul\u{0} bell\u{7} ffff\u{FFFF} emoji 🦀 <&>\"'",
    );
    s.name = "name\u{1}with <control> & \"quotes\"".into();
    s.alt_text = Some("\u{FFFE}".into());
    slide_with(&mut p, vec![s]);
    let out = write_pptx(&p, None, &WriteOptions::default()).expect("writes");
    let back = read_pptx(&out.bytes).expect("reads back");
    let shape = &back.presentation.slides[0].shapes[0];
    let text = kiln_io_pptx::body_text(shape.text().expect("text"));
    assert!(text.contains("emoji 🦀 <&>\"'"), "{text:?}");
    assert!(text.contains("bell"), "{text:?}");
    assert!(
        shape.name.contains("<control> & \"quotes\""),
        "{:?}",
        shape.name
    );
}

#[test]
fn missing_media_and_charts_are_reported_not_fatal() {
    let mut p = Presentation::new();
    let pic = Shape::new(
        p.alloc_id(),
        "Picture",
        Some(Transform::from_rect(Rect::new(0, 0, 10, 10))),
        ShapeKind::Picture(Picture {
            media: "/ppt/media/gone.png".into(),
            ..Picture::default()
        }),
    );
    let chart = Shape::new(
        p.alloc_id(),
        "Chart",
        Some(Transform::from_rect(Rect::new(0, 0, 10, 10))),
        ShapeKind::Chart(ChartRef {
            part: "/ppt/charts/gone.xml".into(),
        }),
    );
    // A media key that is not a part name is stored under a new name.
    p.parts.insert(
        "my picture".into(),
        kiln_deck::Part {
            content_type: "image/png".into(),
            bytes: vec![0x89, b'P', b'N', b'G'],
        },
    );
    let named = Shape::new(
        p.alloc_id(),
        "Named",
        Some(Transform::from_rect(Rect::new(0, 0, 10, 10))),
        ShapeKind::Picture(Picture {
            media: "my picture".into(),
            ..Picture::default()
        }),
    );
    let mut linked = Shape::text_box(p.alloc_id(), Rect::new(0, 0, 10, 10), "jump");
    linked.link = Some("#slide:999999".into());
    slide_with(&mut p, vec![pic, chart, named, linked]);
    let issues = issues_of(&p);
    assert!(issues.contains(&"image.missing".to_owned()), "{issues:?}");
    assert!(issues.contains(&"chart.missing".to_owned()), "{issues:?}");
    assert!(issues.contains(&"link.broken".to_owned()), "{issues:?}");
    let out = write_pptx(&p, None, &WriteOptions::default()).expect("writes");
    let pkg = OpcPackage::open(&out.bytes).expect("opens");
    assert!(pkg.has("/ppt/media/image1.png"));
    let back = read_pptx(&out.bytes).expect("reads");
    // The chart is left out; the other three shapes are there.
    assert_eq!(back.presentation.slides[0].shapes.len(), 3);
}

#[test]
fn kept_xml_that_does_not_parse_is_kept_as_written() {
    let bytes =
        std::fs::read(read_support::fixtures().join("pptx/features/groups.pptx")).expect("fixture");
    let pkg = OpcPackage::open(&bytes).expect("opens");
    let mut read = kiln_io_pptx::read_package(&pkg).expect("reads");
    let slide = read.presentation.slides[0].id;
    let first = read.presentation.slides[0].shapes[0].id;
    // Garbage in every kind of kept XML: it is left out (and reported), the transition and the
    // timing are generated from the model, and the file reads back.
    read.extras
        .shape_xml
        .insert(first, vec!["<a:scene3d>".into()]);
    read.extras.timing.insert(slide, "<p:timing".into());
    read.extras.transitions.insert(slide, "not xml".into());
    read.extras.presentation_xml.push("<p:kinsoku".into());
    let mut pres = read.presentation.clone();
    pres.slides[0].transition = Some(kiln_deck::Transition::new(
        kiln_deck::TransitionEffect::Fade {
            through_black: true,
        },
    ));
    let source = WriteSource::from_read(&read).with_package(&pkg);
    let out = write_pptx(&pres, Some(&source), &WriteOptions::default()).expect("writes");
    assert!(
        out.issues.iter().any(|i| i.code == "xml.invalid"),
        "{:?}",
        out.issues
    );
    let back = read_pptx(&out.bytes).expect("reads back");
    assert!(
        back.warnings
            .iter()
            .all(|w| w.code != kiln_io_pptx::WarningCode::PartInvalidXml)
    );
    assert_eq!(
        back.presentation.slides[0]
            .transition
            .as_ref()
            .map(|t| t.effect),
        Some(kiln_deck::TransitionEffect::Fade {
            through_black: true
        })
    );
}

#[test]
fn deep_groups_write_on_a_small_stack() {
    let handle = std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(|| {
            let mut p = Presentation::new();
            let r = Rect::new(0, 0, 1000, 1000);
            let mut inner = Shape::text_box(p.alloc_id(), r, "deep");
            for _ in 0..64 {
                inner = Shape::new(
                    p.alloc_id(),
                    "Group",
                    Some(Transform::from_rect(r)),
                    ShapeKind::Group(Group {
                        child_space: ChildSpace::identity(r),
                        children: vec![inner],
                        props: ShapeProps::default(),
                    }),
                );
            }
            slide_with(&mut p, vec![inner]);
            let out = write_pptx(&p, None, &WriteOptions::default()).expect("writes");
            out.bytes.len()
        })
        .expect("thread");
    assert!(handle.join().expect("no stack overflow") > 0);
}

#[test]
fn notes_without_a_notes_master_get_one() {
    let mut p = Presentation::new();
    p.notes_master = None;
    p.slides[0].notes = Some(kiln_deck::Notes {
        text: TextBody::from_text("Remember this"),
        shapes: Vec::new(),
    });
    let out = write_pptx(&p, None, &WriteOptions::default()).expect("writes");
    let back = read_pptx(&out.bytes).expect("reads");
    assert!(back.presentation.notes_master.is_some());
    assert_eq!(back.presentation.slides[0].notes_text(), "Remember this");
    let pkg = OpcPackage::open(&out.bytes).expect("opens");
    let nm = pkg
        .related_part(
            "/ppt/presentation.xml",
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster",
        )
        .unwrap()
        .expect("a notes master");
    assert!(
        pkg.related_part(
            &nm,
            "http://schemas.openxmlformats.org/officeDocument/2006/relationships/theme"
        )
        .unwrap()
        .is_some()
    );
}

#[test]
fn an_empty_presentation_is_a_valid_file() {
    let mut p = Presentation::new();
    p.slides.clear();
    let out = write_pptx(&p, None, &WriteOptions::default()).expect("writes");
    let back = read_pptx(&out.bytes).expect("reads");
    assert!(back.presentation.slides.is_empty());
    assert_eq!(back.presentation.masters.len(), 1);
}

#[test]
fn macros_go_only_into_macro_enabled_formats() {
    let mut pkg = read_support::package("pptx/features/aspect-16x9.pptx");
    pkg.set_content_type(
        "/ppt/presentation.xml",
        PptxKind::MacroEnabledPresentation.content_type(),
    )
    .expect("content type");
    pkg.add_part(
        "/ppt/vbaProject.bin",
        "application/vnd.ms-office.vbaProject",
        b"vba".to_vec(),
    )
    .expect("part");
    pkg.add_relationship(
        "/ppt/presentation.xml",
        "http://schemas.microsoft.com/office/2006/relationships/vbaProject",
        "/ppt/vbaProject.bin",
        RelationshipOptions::default(),
    )
    .expect("relationship");
    let bytes = pkg.save().expect("save");
    let keep = rewrite_pptx(&bytes, &WriteOptions::default()).expect("pptm");
    let kept = OpcPackage::open(&keep.bytes).expect("opens");
    assert!(kept.has("/ppt/vbaProject.bin"));
    let drop = rewrite_pptx(
        &bytes,
        &WriteOptions {
            kind: Some(PptxKind::Presentation),
            ..WriteOptions::default()
        },
    )
    .expect("pptx");
    let dropped = OpcPackage::open(&drop.bytes).expect("opens");
    assert!(!dropped.has("/ppt/vbaProject.bin"));
    assert!(
        dropped
            .relationships("/ppt/presentation.xml")
            .unwrap()
            .all()
            .iter()
            .all(|r| !r.rel_type.contains("vbaProject"))
    );
    assert_eq!(
        dropped.content_type("/ppt/presentation.xml"),
        Some(PptxKind::Presentation.content_type())
    );
    assert!(read_pptx(&drop.bytes).is_ok());
}

/// Every fixture with slides and shapes removed at random still writes a file that reads back.
fn fixture_paths() -> Vec<std::path::PathBuf> {
    read_support::decks("pptx/features")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]
    #[test]
    fn random_deletions_write_files_that_read_back(
        which in 0usize..64,
        drops in proptest::collection::vec(0usize..64, 0..6),
    ) {
        let paths = fixture_paths();
        let path = &paths[which % paths.len()];
        let bytes = std::fs::read(path).expect("fixture");
        let pkg = OpcPackage::open(&bytes).expect("opens");
        let read = kiln_io_pptx::read_package(&pkg).expect("reads");
        let mut pres = read.presentation.clone();
        for d in drops {
            if d % 3 == 0 && pres.slides.len() > 1 {
                let i = d % pres.slides.len();
                let id = pres.slides[i].id;
                let tx = kiln_deck::edit::delete_slides(&mut pres, &[id]).expect("delete");
                pres.apply(tx).expect("applies");
            } else if let Some(s) = pres.slides.get(d % pres.slides.len().max(1)) {
                if s.shapes.is_empty() {
                    continue;
                }
                let id = s.shapes[d % s.shapes.len()].id;
                let tx = kiln_deck::edit::delete_shapes(&mut pres, &[id]).expect("delete");
                pres.apply(tx).expect("applies");
            }
        }
        let source = WriteSource::from_read(&read).with_package(&pkg);
        let out = write_pptx(&pres, Some(&source), &WriteOptions::default()).expect("writes");
        let back = read_pptx(&out.bytes).expect("reads back");
        prop_assert_eq!(back.presentation.slides.len(), pres.slides.len());
        for (a, b) in pres.slides.iter().zip(&back.presentation.slides) {
            prop_assert_eq!(a.shapes.len(), b.shapes.len());
            prop_assert_eq!(a.timeline.sequence.len(), b.timeline.sequence.len());
        }
    }
}

#[test]
fn ids_of_new_shapes_never_clash_with_kept_xml() {
    // A SmartArt frame and an OLE object keep their XML; new shapes beside them get fresh ids.
    let read = read_support::read_fixture("pptx/features/groups.pptx");
    let mut pres = read.presentation.clone();
    let extra = Shape::text_box(pres.alloc_id(), Rect::new(0, 0, 10, 10), "new");
    let slide = pres.slides[0].id;
    pres.slide_mut(slide).expect("slide").shapes.push(extra);
    let out = write_pptx(
        &pres,
        Some(&WriteSource::from_read(&read)),
        &WriteOptions::default(),
    )
    .expect("writes");
    let pkg = OpcPackage::open(&out.bytes).expect("opens");
    let xml = String::from_utf8(
        pkg.part("/ppt/slides/slide1.xml")
            .unwrap()
            .unwrap()
            .into_owned(),
    )
    .unwrap();
    let mut ids: Vec<&str> = xml
        .match_indices("<p:cNvPr id=\"")
        .map(|(i, m)| {
            let rest = &xml[i + m.len()..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect();
    let n = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), n, "duplicate shape ids in {xml}");
    let _ = NodeId(0);
}
