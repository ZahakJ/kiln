// SPDX-License-Identifier: AGPL-3.0-or-later
//! Content real-world decks carry that an unedited round trip must keep. From files of the
//! fidelity corpus.

use kiln_io_pptx::{WriteOptions, rewrite_pptx};
use kiln_ooxml::opc::OpcPackage;

#[test]
fn run_effects_outlines_and_underline_fills_are_kept() {
    let path = format!(
        "{}/../../fixtures/pptx/features/text-effects.pptx",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).expect("fixture");
    let written = rewrite_pptx(&bytes, &WriteOptions::default()).expect("round trip");
    let pkg = OpcPackage::open(&written.bytes).expect("package");
    let xml = pkg.part_bytes("/ppt/slides/slide1.xml").expect("slide");
    let xml = String::from_utf8_lossy(&xml);
    for tag in [
        "<a:ln w=\"9525\">",
        "<a:outerShdw",
        "<a:uFill>",
        "<a:sym typeface=\"Wingdings\"/>",
        "<a:effectLst/>",
        "<a:uLnTx/>",
        "<a:uFillTx/>",
    ] {
        assert!(xml.contains(tag), "{tag}: {xml}");
    }
    // The schema's order inside a:rPr.
    let (outline, fill, shadow, ufill, latin, sym) = (
        xml.find("<a:ln ").unwrap(),
        xml.find("<a:solidFill><a:srgbClr val=\"C2481C\"").unwrap(),
        xml.find("<a:outerShdw").unwrap(),
        xml.find("<a:uFill>").unwrap(),
        xml.find("<a:latin").unwrap(),
        xml.find("<a:sym").unwrap(),
    );
    assert!(outline < fill && fill < shadow && shadow < ufill && ufill < latin && latin < sym);
}

#[test]
fn run_effects_survive_when_the_model_merges_the_runs() {
    // Two runs that differ only in XML the text model does not hold are one span to it.
    let path = format!(
        "{}/../../fixtures/pptx/features/text-effects.pptx",
        env!("CARGO_MANIFEST_DIR")
    );
    let mut pkg = OpcPackage::open(&std::fs::read(path).expect("fixture")).expect("package");
    let slide = String::from_utf8(
        pkg.part_bytes("/ppt/slides/slide1.xml")
            .expect("slide")
            .to_vec(),
    )
    .expect("utf-8");
    let shadow = "<a:rPr lang=\"en-US\" sz=\"2400\"><a:effectLst><a:outerShdw blurRad=\"38100\" dist=\"38100\" dir=\"2700000\" algn=\"tl\"><a:srgbClr val=\"FF0000\"/></a:outerShdw></a:effectLst></a:rPr>";
    let paragraph = format!(
        "<a:p><a:r>{shadow}<a:t>Hello </a:t></a:r><a:r>{shadow}<a:t>world</a:t></a:r><a:r><a:rPr lang=\"en-US\" sz=\"2400\"/><a:t> plain</a:t></a:r></a:p>"
    );
    let start = slide.find("<a:p>").expect("paragraph");
    let end = start + slide[start..].find("</a:p>").expect("end") + "</a:p>".len();
    let changed = format!("{}{paragraph}{}", &slide[..start], &slide[end..]);
    pkg.set_part("/ppt/slides/slide1.xml", changed.into_bytes())
        .expect("part");
    let bytes = pkg.save().expect("save");
    let written = rewrite_pptx(&bytes, &WriteOptions::default()).expect("round trip");
    let out = OpcPackage::open(&written.bytes).expect("package");
    let xml = out.part_bytes("/ppt/slides/slide1.xml").expect("slide");
    let xml = String::from_utf8_lossy(&xml);
    assert_eq!(xml.matches("<a:outerShdw").count(), 3, "{xml}");
    assert!(
        xml.contains("<a:t>Hello </a:t>") && xml.contains("<a:t> plain</a:t>"),
        "{xml}"
    );
}

#[test]
fn real_decks_keep_their_run_effects() {
    let dir = format!(
        "{}/../../fixtures/external/pptx",
        env!("CARGO_MANIFEST_DIR")
    );
    let Ok(files) = std::fs::read_dir(dir) else {
        return;
    };
    let mut lost = Vec::new();
    for f in files.flatten() {
        let bytes = std::fs::read(f.path()).expect("deck");
        let Ok(written) = rewrite_pptx(&bytes, &WriteOptions::default()) else {
            continue;
        };
        let (a, b) = (
            OpcPackage::open(&bytes).expect("package"),
            OpcPackage::open(&written.bytes).expect("package"),
        );
        // Over the deck: the writer may number slide parts differently.
        let count = |pkg: &OpcPackage, tag: &str| -> usize {
            pkg.all_part_names()
                .filter(|n| n.starts_with("/ppt/slides/slide"))
                .filter_map(|n| pkg.part_bytes(n).ok())
                .map(|x| String::from_utf8_lossy(&x).matches(tag).count())
                .sum()
        };
        for tag in ["<a:outerShdw", "<a:uFillTx", "<a:uFill>", "<a:uLnTx"] {
            let (n, m) = (count(&a, tag), count(&b, tag));
            if n != m {
                lost.push(format!("{:?} {tag}: {n} -> {m}", f.file_name()));
            }
        }
    }
    assert!(lost.is_empty(), "{}", lost.join("\n"));
}

/// Saves a deck as a session does: `edit` through a transaction, the parts it touched rewritten.
fn saved_after(
    bytes: &[u8],
    edit: impl FnOnce(&mut kiln_deck::Presentation) -> kiln_deck::Transaction,
) -> OpcPackage {
    let pkg = OpcPackage::open(bytes).expect("package");
    let read = kiln_io_pptx::read_package(&pkg).expect("reads");
    let mut pres = read.presentation.clone();
    let tx = edit(&mut pres);
    pres.apply(tx.clone()).expect("edit applies");
    let mut touched = kiln_io_pptx::TouchedParts::new();
    touched.record(&pres, &tx);
    let source = kiln_io_pptx::WriteSource::from_read(&read).with_package(&pkg);
    let written = kiln_io_pptx::write_pptx(
        &pres,
        Some(&source),
        &WriteOptions {
            preserve: Some(&touched),
            ..WriteOptions::default()
        },
    )
    .expect("writes");
    OpcPackage::open(&written.bytes).expect("package")
}

/// A shape's first paragraph ends here, after `extra` is typed: the position at its end.
fn end_of_first_paragraph(
    pres: &kiln_deck::Presentation,
    shape: kiln_common::NodeId,
) -> kiln_doc::Pos {
    let body = pres
        .slides
        .iter()
        .flat_map(|s| s.shapes.iter())
        .find(|s| s.id == shape)
        .and_then(|s| s.text())
        .expect("body");
    let p = body.paragraphs().next().expect("paragraph");
    kiln_doc::Pos::new(p.id, p.content.len())
}

/// The first shape on the first slide with text containing an effect, by id.
fn shape_with_text(pres: &kiln_deck::Presentation, wanted: &str) -> kiln_common::NodeId {
    pres.slides
        .iter()
        .flat_map(|s| s.shapes.iter())
        .find(|s| {
            s.text()
                .is_some_and(|t| kiln_io_pptx::body_text(t).contains(wanted))
        })
        .map(|s| s.id)
        .expect("the shape")
}

#[test]
fn typing_in_a_paragraph_keeps_the_effects_of_its_runs() {
    let path = format!(
        "{}/../../fixtures/pptx/features/text-effects.pptx",
        env!("CARGO_MANIFEST_DIR")
    );
    let bytes = std::fs::read(path).expect("fixture");
    let out = saved_after(&bytes, |p| {
        let shape = shape_with_text(p, "Shadow and outline");
        let at = end_of_first_paragraph(p, shape);
        kiln_deck::edit::insert_text(p, kiln_deck::ops::TextTarget::Shape(shape), at, " more")
            .expect("edit")
    });
    let xml = out.part_bytes("/ppt/slides/slide1.xml").expect("slide");
    let xml = String::from_utf8_lossy(&xml);
    assert!(xml.contains("<a:outerShdw"), "{xml}");
    assert!(
        xml.contains("<a:uFill>") && xml.contains("<a:uLnTx/>"),
        "{xml}"
    );
    assert!(
        xml.contains("Shadow and outline") && xml.contains(" more"),
        "{xml}"
    );
}

/// Every text in the slides, whitespace aside, as characters: run boundaries do not matter.
fn slide_characters(pkg: &OpcPackage) -> std::collections::BTreeMap<char, usize> {
    let mut out = std::collections::BTreeMap::new();
    for n in pkg
        .all_part_names()
        .filter(|n| n.starts_with("/ppt/slides/slide"))
    {
        let Ok(bytes) = pkg.part_bytes(n) else {
            continue;
        };
        let Ok(doc) = kiln_ooxml::xml::parse_xml(&bytes) else {
            continue;
        };
        for t in doc.root.descendants("a:t") {
            for c in t.text().chars().filter(|c| !c.is_whitespace()) {
                *out.entry(c).or_insert(0) += 1;
            }
        }
    }
    out
}

/// `a:rPr` children in the schema's order, and how many of `tag` the slides hold.
fn run_props_in_order(pkg: &OpcPackage) -> Result<(), String> {
    const ORDER: [&str; 13] = [
        "a:ln",
        "a:fill",
        "a:effect",
        "a:highlight",
        "a:uLn",
        "a:uFill",
        "a:latin",
        "a:ea",
        "a:cs",
        "a:sym",
        "a:hlinkClick",
        "a:hlinkMouseOver",
        "a:extLst",
    ];
    let rank = |name: &str| -> Option<usize> {
        let group = match name {
            "a:noFill" | "a:solidFill" | "a:gradFill" | "a:blipFill" | "a:pattFill"
            | "a:grpFill" => "a:fill",
            "a:effectLst" | "a:effectDag" => "a:effect",
            "a:uLnTx" | "a:uLn" => "a:uLn",
            "a:uFillTx" | "a:uFill" => "a:uFill",
            other => other,
        };
        ORDER.iter().position(|o| *o == group)
    };
    for n in pkg
        .all_part_names()
        .filter(|n| n.starts_with("/ppt/slides/slide"))
    {
        let Ok(bytes) = pkg.part_bytes(n) else {
            continue;
        };
        let Ok(doc) = kiln_ooxml::xml::parse_xml(&bytes) else {
            continue;
        };
        for tag in ["a:rPr", "a:endParaRPr"] {
            for r in doc.root.descendants(tag) {
                let ranks: Vec<usize> = r.elements().filter_map(|e| rank(&e.name)).collect();
                if ranks.windows(2).any(|w| w[0] > w[1]) {
                    return Err(format!("{n}: {r:?}"));
                }
            }
        }
    }
    Ok(())
}

#[test]
fn edited_real_decks_keep_every_text_and_effect_in_schema_order() {
    for name in ["poi-stress021", "poi-smartart-simple", "poi-aptia"] {
        let path = format!(
            "{}/../../fixtures/external/pptx/{name}.pptx",
            env!("CARGO_MANIFEST_DIR")
        );
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        let original = OpcPackage::open(&bytes).expect("package");
        let (mut want, typed) = (slide_characters(&original), " Kiln");
        for c in typed.chars().filter(|c| !c.is_whitespace()) {
            *want.entry(c).or_insert(0) += 1;
        }
        let out = saved_after(&bytes, |p| {
            let shape = p
                .slides
                .iter()
                .flat_map(|s| s.shapes.iter())
                .find(|s| {
                    s.text()
                        .is_some_and(|t| !kiln_io_pptx::body_text(t).trim().is_empty())
                })
                .map(|s| s.id)
                .expect("a text");
            let at = end_of_first_paragraph(p, shape);
            kiln_deck::edit::insert_text(p, kiln_deck::ops::TextTarget::Shape(shape), at, typed)
                .expect("edit")
        });
        assert_eq!(slide_characters(&out), want, "{name}: the slides' text");
        run_props_in_order(&out).unwrap_or_else(|e| panic!("{name}: {e}"));
        let count = |pkg: &OpcPackage, tag: &str| -> usize {
            pkg.all_part_names()
                .filter(|n| n.starts_with("/ppt/slides/slide"))
                .filter_map(|n| pkg.part_bytes(n).ok())
                .map(|x| String::from_utf8_lossy(&x).matches(tag).count())
                .sum()
        };
        for tag in ["<a:outerShdw", "<a:uFillTx", "<a:uFill>", "<a:uLnTx"] {
            assert_eq!(count(&out, tag), count(&original, tag), "{name}: {tag}");
        }
    }
}
