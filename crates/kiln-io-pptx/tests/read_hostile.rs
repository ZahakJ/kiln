// SPDX-License-Identifier: AGPL-3.0-or-later
//! Presentations are untrusted input (PLAN.md §3.12): the reader must never panic and never
//! exhaust the stack, whatever the bytes. Deep nesting is read on a 1 MiB stack (the web
//! build's default) in an unoptimised build; mutated parts must give a valid model or an error.

mod read_support;

use kiln_io_pptx::{ReadOptions, read_pptx, read_pptx_with};
use kiln_ooxml::opc::OpcLimits;
use proptest::prelude::*;
use read_support::{deck_with_slide, package, slide_xml, sp};

/// Runs `f` on a thread with a small stack, as the wasm build has.
fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("no stack overflow or panic");
}

fn nested(open: &str, close: &str, inner: &str, depth: usize) -> String {
    let mut s = String::new();
    for _ in 0..depth {
        s.push_str(open);
    }
    s.push_str(inner);
    for _ in 0..depth {
        s.push_str(close);
    }
    s
}

fn read_slide(tree: String, tail: String) {
    on_small_stack(move || {
        let pkg = deck_with_slide(&slide_xml("", &tree, &tail), &[]);
        let read = read_pptx(&pkg.save().unwrap()).expect("the deck opens");
        read.presentation.validate().expect("valid");
    });
}

#[test]
fn deeply_nested_groups_are_bounded() {
    let open = r#"<p:grpSp><p:nvGrpSpPr><p:cNvPr id="9" name="G"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>"#;
    for depth in [10, 200, 1_000, 2_100] {
        read_slide(
            nested(open, "</p:grpSp>", &sp(2, "Leaf", "<a:p/>"), depth),
            String::new(),
        );
    }
}

#[test]
fn deeply_nested_timing_and_alternate_content_are_bounded() {
    let open = r#"<p:par><p:cTn id="3"><p:childTnLst>"#;
    let close = "</p:childTnLst></p:cTn></p:par>";
    for depth in [10, 300, 650] {
        let inner = nested(open, close, "", depth);
        let timing = format!(
            r#"<p:timing><p:tnLst><p:par><p:cTn id="1" nodeType="tmRoot"><p:childTnLst><p:seq><p:cTn id="2" nodeType="mainSeq"><p:childTnLst>{inner}</p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>"#
        );
        read_slide(sp(2, "A", "<a:p/>"), timing);
    }
    let ac_open = r#"<mc:AlternateContent><mc:Choice Requires="a14">"#;
    let ac_close = "</mc:Choice></mc:AlternateContent>";
    for depth in [3, 50, 600] {
        let body = format!(
            "<a:p>{}</a:p>",
            nested(ac_open, ac_close, "<a:r><a:t>x</a:t></a:r>", depth)
        );
        read_slide(sp(2, "A", &body), String::new());
    }
}

#[test]
fn zip_limits_are_applied() {
    let bytes = std::fs::read(read_support::fixtures().join("pptx/features/charts.pptx")).unwrap();
    let tight = OpcLimits {
        max_entries: 5,
        ..OpcLimits::default()
    };
    assert!(
        read_pptx_with(
            &bytes,
            ReadOptions {
                limits: Some(tight)
            }
        )
        .is_err()
    );
    assert!(read_pptx_with(&bytes, ReadOptions::default()).is_ok());
}

const SOURCES: [(&str, &str); 6] = [
    ("pptx/features/placeholders.pptx", "/ppt/slides/slide5.xml"),
    ("pptx/features/animations.pptx", "/ppt/slides/slide1.xml"),
    ("pptx/features/tables.pptx", "/ppt/slides/slide1.xml"),
    ("pptx/features/groups.pptx", "/ppt/slides/slide1.xml"),
    (
        "pptx/features/placeholders.pptx",
        "/ppt/slideMasters/slideMaster1.xml",
    ),
    ("pptx/features/sections.pptx", "/ppt/presentation.xml"),
];

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    /// Cutting, repeating and splicing pieces of real parts never panics; what opens is valid.
    #[test]
    fn mutated_parts_never_panic(
        source in 0..SOURCES.len(),
        cut in 0.0f64..1.0,
        len in 0usize..400,
        insert in prop::sample::select(vec![
            "", "<p:sp/>", "<a:p><a:r><a:t>x", "</p:spTree>", "<p:grpSp><p:grpSpPr/>",
            "<a:tbl><a:tr h=\"-5\"><a:tc/></a:tr></a:tbl>", "<a:off x=\"-99999999999999999999\"/>",
            "<p:ph type=\"nope\" idx=\"-1\"/>", "&amp;&#0;", "<p:cTn presetClass=\"emph\" presetID=\"8\"/>",
            "<a:fld type=\"slidenum\"/>", "<p14:sectionLst><p14:section name=\"x\"/></p14:sectionLst>",
        ]),
    ) {
        let (fixture, part) = SOURCES[source];
        let mut pkg = package(fixture);
        let xml = String::from_utf8(pkg.part_bytes(part).unwrap().into_owned()).unwrap();
        let mut at = ((xml.len() as f64) * cut) as usize;
        while !xml.is_char_boundary(at) {
            at -= 1;
        }
        let mut end = (at + len).min(xml.len());
        while !xml.is_char_boundary(end) {
            end -= 1;
        }
        let mutated = format!("{}{insert}{}", &xml[..at], &xml[end..]);
        pkg.set_part(part, mutated.into_bytes()).unwrap();
        let bytes = pkg.save().unwrap();
        if let Ok(read) = read_pptx(&bytes) {
            prop_assert!(read.presentation.validate().is_ok());
        }
    }
}
