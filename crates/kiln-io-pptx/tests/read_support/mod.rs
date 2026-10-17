// SPDX-License-Identifier: AGPL-3.0-or-later
//! Shared helpers of the reader's integration tests: fixture paths, the python-pptx view of a
//! read presentation, and packages built from a fixture with replaced or added parts.
#![allow(dead_code)]

use std::path::{Path, PathBuf};

use kiln_deck::shape::{Shape, ShapeKind};
use kiln_io_pptx::{PptxExtras, PptxRead, body_text, read_pptx, shape_kind};
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions};
use serde_json::{Value, json};

/// The fixtures root.
pub fn fixtures() -> PathBuf {
    kiln_testing::fixtures::fixtures_dir()
}

/// Every `.pptx` under a fixtures subdirectory (recursively), sorted.
pub fn decks(dir: &str) -> Vec<PathBuf> {
    fn walk(d: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(d) else {
            return;
        };
        for e in entries.flatten() {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "pptx") {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    walk(&fixtures().join(dir), &mut out);
    out.sort();
    out
}

/// Reads a fixture by path relative to the fixtures root.
pub fn read_fixture(rel: &str) -> PptxRead {
    let bytes = std::fs::read(fixtures().join(rel)).expect("fixture");
    read_pptx(&bytes).unwrap_or_else(|e| panic!("{rel}: {e}"))
}

fn shape_view(s: &Shape, extras: &PptxExtras) -> Value {
    let kind = shape_kind(s, extras);
    let mut v = json!({"kind": kind, "name": s.name});
    match &s.kind {
        ShapeKind::Group(g) if kind == "group" => {
            v["children"] =
                Value::Array(g.children.iter().map(|c| shape_view(c, extras)).collect());
        }
        ShapeKind::Table(t) => {
            v["cells"] = Value::Array(
                t.rows
                    .iter()
                    .map(|r| {
                        Value::Array(r.cells.iter().map(|c| json!(body_text(&c.text))).collect())
                    })
                    .collect(),
            );
        }
        ShapeKind::Geometry(a) => {
            v["text"] = json!(a.text.as_ref().map(body_text).unwrap_or_default());
        }
        _ => {}
    }
    v
}

/// The read presentation in the shape of `tools/make-fixtures.py --dump-pptx`. Shapes read from
/// `mc:AlternateContent` are left out (python-pptx does not look inside it).
pub fn oracle_view(read: &PptxRead) -> Value {
    let p = &read.presentation;
    let x = &read.extras;
    let visible = |s: &&Shape| {
        !matches!(
            x.source_of(s.id),
            Some(kiln_io_pptx::ShapeSource::AlternateContent { .. })
        )
    };
    let slides: Vec<Value> = p
        .slides
        .iter()
        .map(|s| {
            let shapes: Vec<&Shape> = s.shapes.iter().filter(visible).collect();
            json!({
                "layout": p.slide_layout(s).map(|(_, l)| l.name.clone()).unwrap_or_default(),
                "shape_count": shapes.len(),
                "shapes": shapes.iter().map(|sh| shape_view(sh, x)).collect::<Vec<_>>(),
                "notes": s.notes.as_ref().map(|n| body_text(&n.text)),
            })
        })
        .collect();
    json!({
        "slide_size": [p.slide_size.cx, p.slide_size.cy],
        "slide_count": p.slides.len(),
        "masters": p.masters.iter().map(|m| json!({
            "layouts": m.layouts.iter().map(|l| l.name.clone()).collect::<Vec<_>>()
        })).collect::<Vec<_>>(),
        "slides": slides,
    })
}

/// The oracle dump of a fixture, if one was generated.
pub fn oracle_dump(deck: &Path) -> Option<Value> {
    let name = format!("{}.json", deck.file_name()?.to_string_lossy());
    let under_external = deck.components().any(|c| c.as_os_str() == "external");
    let dir = fixtures().join("pptx/read");
    let path = if under_external {
        dir.join("external").join(name)
    } else {
        dir.join(name)
    };
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// A package from a fixture, to edit and read back.
pub fn package(rel: &str) -> OpcPackage {
    OpcPackage::open(&std::fs::read(fixtures().join(rel)).expect("fixture")).expect("package")
}

/// The namespaces a slide part declares.
pub const NS: &str = concat!(
    r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
    r#"xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:p14="http://schemas.microsoft.com/office/powerpoint/2010/main" "#,
    r#"xmlns:a14="http://schemas.microsoft.com/office/drawing/2010/main" "#,
    r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math""#
);

/// A whole slide part: `tree` inside `p:spTree`, `tail` after `p:clrMapOvr` (transition,
/// timing), `attrs` on `p:sld`.
pub fn slide_xml(attrs: &str, tree: &str, tail: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><p:sld {NS} {attrs}><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{tree}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{tail}</p:sld>"#
    )
}

/// A text shape.
pub fn sp(id: u32, name: &str, body: &str) -> String {
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x="100" y="200"/><a:ext cx="3000" cy="4000"/></a:xfrm><a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody></p:sp>"#
    )
}

/// Replaces slide 1 of `aspect-16x9.pptx` (a two-slide deck on the Title Only layout) with a
/// slide part, adding relationships `(id, type, target, external)` from it.
pub fn deck_with_slide(xml: &str, rels: &[(&str, &str, &str, bool)]) -> OpcPackage {
    let mut pkg = package("pptx/features/aspect-16x9.pptx");
    pkg.set_part("/ppt/slides/slide1.xml", xml.as_bytes().to_vec())
        .expect("slide part");
    for (id, t, target, external) in rels {
        pkg.add_relationship(
            "/ppt/slides/slide1.xml",
            t,
            target,
            RelationshipOptions {
                id: Some(id),
                external: *external,
            },
        )
        .expect("relationship");
    }
    pkg
}

/// Saves and reads a package.
pub fn read_package_bytes(pkg: &OpcPackage) -> PptxRead {
    read_pptx(&pkg.save().expect("save")).expect("read")
}

/// The relationship type URI for a short PresentationML/DrawingML name.
pub fn rt(name: &str) -> String {
    format!("http://schemas.openxmlformats.org/officeDocument/2006/relationships/{name}")
}
