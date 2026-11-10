// SPDX-License-Identifier: AGPL-3.0-or-later
//! Test helpers: building small DOCX packages from XML snippets, finding content in a read
//! document, and a plain-text view that follows mammoth's text model (the independent oracle).
#![allow(dead_code)]

use kiln_doc::st::VMerge;
use kiln_doc::{Block, Document, InlineObject, Mark, Paragraph, RunProps};
use kiln_io_docx::{DocxRead, read_docx, symbol_code_to_unicode};
use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, content_type, rel_type};

pub const NS_DECLS: &str = concat!(
    r#"xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" "#,
    r#"xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" "#,
    r#"xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" "#,
    r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" "#,
    r#"xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture" "#,
    r#"xmlns:wps="http://schemas.microsoft.com/office/word/2010/wordprocessingShape" "#,
    r#"xmlns:mc="http://schemas.openxmlformats.org/markup-compatibility/2006" "#,
    r#"xmlns:m="http://schemas.openxmlformats.org/officeDocument/2006/math" "#,
    r#"xmlns:v="urn:schemas-microsoft-com:vml" "#,
    r#"xmlns:o="urn:schemas-microsoft-com:office:office" "#,
    r#"xmlns:w14="http://schemas.microsoft.com/office/word/2010/wordml" "#,
    r#"xmlns:w15="http://schemas.microsoft.com/office/word/2012/wordml" "#,
    r#"xmlns:c="http://schemas.openxmlformats.org/drawingml/2006/chart""#,
);

/// Wraps XML as a complete part with the test namespace declarations.
pub fn part(root: &str, inner: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<{root} {NS_DECLS}>{inner}</{root}>"
    )
}

/// An extra part of a test package.
pub struct ExtraPart {
    pub name: &'static str,
    pub content_type: &'static str,
    /// Relationship type from the main document (`None` for parts reached otherwise).
    pub rel_type: Option<&'static str>,
    /// A fixed relationship id.
    pub rel_id: Option<&'static str>,
    pub content: Vec<u8>,
}

/// What a test package holds.
#[derive(Default)]
pub struct Build {
    pub body: String,
    pub styles: Option<String>,
    pub numbering: Option<String>,
    pub settings: Option<String>,
    pub parts: Vec<ExtraPart>,
    /// Extra relationships of the main part: (id, type, target, external).
    pub rels: Vec<(&'static str, &'static str, &'static str, bool)>,
}

pub const PNG: [u8; 12] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0];

pub fn image_part() -> ExtraPart {
    ExtraPart {
        name: "/word/media/image1.png",
        content_type: "image/png",
        rel_type: Some(rel_type::IMAGE),
        rel_id: Some("rIdImg"),
        content: PNG.to_vec(),
    }
}

/// Builds a .docx from XML snippets.
pub fn build_docx(b: Build) -> Vec<u8> {
    let mut pkg = OpcPackage::new();
    let main = "/word/document.xml";
    pkg.add_part(
        main,
        content_type::DOCX_DOCUMENT,
        part("w:document", &format!("<w:body>{}</w:body>", b.body)),
    )
    .expect("main part");
    pkg.add_relationship(
        "/",
        rel_type::OFFICE_DOCUMENT,
        main,
        RelationshipOptions::default(),
    )
    .expect("rel");
    let add = |pkg: &mut OpcPackage, p: ExtraPart| {
        pkg.add_part(p.name, p.content_type, p.content)
            .expect("part");
        if let Some(t) = p.rel_type {
            pkg.add_relationship(
                main,
                t,
                p.name,
                RelationshipOptions {
                    id: p.rel_id,
                    external: false,
                },
            )
            .expect("rel");
        }
    };
    for (name, ct, rt, root, xml) in [
        (
            "/word/styles.xml",
            content_type::DOCX_STYLES,
            rel_type::STYLES,
            "w:styles",
            b.styles,
        ),
        (
            "/word/numbering.xml",
            content_type::DOCX_NUMBERING,
            rel_type::NUMBERING,
            "w:numbering",
            b.numbering,
        ),
        (
            "/word/settings.xml",
            content_type::DOCX_SETTINGS,
            rel_type::SETTINGS,
            "w:settings",
            b.settings,
        ),
    ] {
        if let Some(xml) = xml {
            add(
                &mut pkg,
                ExtraPart {
                    name,
                    content_type: ct,
                    rel_type: Some(rt),
                    rel_id: None,
                    content: part(root, &xml).into_bytes(),
                },
            );
        }
    }
    for p in b.parts {
        add(&mut pkg, p);
    }
    for (id, t, target, external) in b.rels {
        let target = if external {
            target.to_owned()
        } else {
            format!("/word/{target}")
        };
        pkg.add_relationship(
            main,
            t,
            &target,
            RelationshipOptions {
                id: Some(id),
                external,
            },
        )
        .expect("rel");
    }
    pkg.save().expect("save")
}

/// Reads a package built from a body snippet, asserting the result is a valid document.
pub fn read_body(body: &str) -> DocxRead {
    read_build(Build {
        body: body.to_owned(),
        ..Build::default()
    })
}

/// Reads a built package, asserting the result is a valid document.
pub fn read_build(b: Build) -> DocxRead {
    let r = read_docx(&build_docx(b)).expect("reads");
    r.document.validate().expect("valid document");
    r
}

pub fn p(inner: &str) -> String {
    format!("<w:p>{inner}</w:p>")
}

pub fn p_pr(inner: &str, ppr: &str) -> String {
    format!("<w:p><w:pPr>{ppr}</w:pPr>{inner}</w:p>")
}

pub fn r(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

pub fn r_pr(text: &str, rpr: &str) -> String {
    format!("<w:r><w:rPr>{rpr}</w:rPr><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

pub fn codes(r: &DocxRead) -> Vec<&'static str> {
    r.warnings.iter().map(|w| w.code.as_str()).collect()
}

fn collect_paragraphs<'a>(blocks: &'a [Block], out: &mut Vec<&'a Paragraph>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                out.push(p);
                for o in p.content.objects() {
                    if let Some(tb) = o.blocks() {
                        collect_paragraphs(tb, out);
                    }
                }
            }
            Block::Table(t) => {
                for row in &t.rows {
                    for cell in &row.cells {
                        collect_paragraphs(&cell.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => collect_paragraphs(&s.blocks, out),
            Block::Field(f) => collect_paragraphs(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

/// Every paragraph of every story, in story order (body, headers/footers, notes, comments).
pub fn all_paragraphs(doc: &Document) -> Vec<&Paragraph> {
    let mut out = Vec::new();
    for story in doc.stories() {
        if let Some(blocks) = doc.story_blocks(story) {
            collect_paragraphs(blocks, &mut out);
        }
    }
    out
}

/// Body paragraphs (with tables, content controls, field blocks and text boxes).
pub fn body_paragraphs(doc: &Document) -> Vec<&Paragraph> {
    let mut out = Vec::new();
    for s in &doc.sections {
        collect_paragraphs(&s.blocks, &mut out);
    }
    out
}

/// The first paragraph whose text contains `needle`.
pub fn paragraph_with<'a>(doc: &'a Document, needle: &str) -> &'a Paragraph {
    all_paragraphs(doc)
        .into_iter()
        .find(|p| p.text().contains(needle))
        .unwrap_or_else(|| panic!("no paragraph containing {needle:?}"))
}

/// The run properties and marks at the first character of the first occurrence of `needle`.
pub fn at_text<'a>(doc: &'a Document, needle: &str) -> (&'a RunProps, Vec<&'a Mark>) {
    for p in all_paragraphs(doc) {
        let text: String = p.content.raw_text();
        if let Some(byte) = text.find(needle) {
            let offset = text[..byte].chars().count();
            let props = p.content.props_at(offset).expect("props");
            let marks = p
                .content
                .marks()
                .iter()
                .filter(|m| m.start <= offset && offset < m.end)
                .map(|m| &m.mark)
                .collect();
            return (props, marks);
        }
    }
    panic!("no text {needle:?}")
}

/// Every inline object of every story.
pub fn objects(doc: &Document) -> Vec<&InlineObject> {
    all_paragraphs(doc)
        .into_iter()
        .flat_map(|p| p.content.objects().iter())
        .collect()
}

// ------------------------------------------------------------------ mammoth's text model

fn mammoth_inline(p: &Paragraph) -> String {
    let deleted: Vec<std::ops::Range<usize>> = p
        .content
        .marks()
        .iter()
        .filter(|m| matches!(m.mark, Mark::Deletion(_) | Mark::MoveFrom(_)))
        .map(|m| m.start..m.end)
        .collect();
    let mut s = String::new();
    let mut object = 0;
    for (i, c) in p.content.raw_text().chars().enumerate() {
        let skip = deleted.iter().any(|r| r.contains(&i));
        if c == kiln_doc::OBJECT_CHAR {
            let o = &p.content.objects()[object];
            object += 1;
            if skip {
                continue;
            }
            match o {
                InlineObject::Field(f) => s.push_str(&f.result),
                InlineObject::Symbol(sym) => {
                    if let Some(ch) = symbol_code_to_unicode(&sym.font, sym.code) {
                        s.push(ch);
                    }
                }
                InlineObject::NoteRef(_) => {}
                _ => {}
            }
        } else if !skip {
            s.push(c);
        }
    }
    s
}

fn mammoth_blocks(blocks: &[Block], out: &mut Vec<String>) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => out.push(mammoth_inline(p)),
            Block::Table(t) => {
                for row in &t.rows {
                    // mammoth merges vertically merged cells into the cell that starts them.
                    for cell in row
                        .cells
                        .iter()
                        .filter(|c| c.v_merge != Some(VMerge::Continue))
                    {
                        mammoth_blocks(&cell.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => mammoth_blocks(&s.blocks, out),
            Block::Field(f) => mammoth_blocks(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

/// Body text following mammoth's raw-text model: one entry per paragraph of the main story
/// (table cells paragraph by paragraph, generated tables included), deleted text and line
/// breaks omitted, tabs kept; headers, footers, notes and text boxes excluded.
pub fn mammoth_model_paragraphs(doc: &Document) -> Vec<String> {
    let mut out = Vec::new();
    for s in &doc.sections {
        mammoth_blocks(&s.blocks, &mut out);
    }
    out
}

/// Whitespace normalisation of the comparison: runs of spaces and NBSPs collapse, trailing
/// spaces go.
pub fn normalise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = false;
    for c in s.chars() {
        if c == ' ' || c == '\u{00A0}' {
            if !space {
                out.push(' ');
            }
            space = true;
        } else {
            out.push(c);
            space = false;
        }
    }
    out.trim_end_matches(' ').to_owned()
}
