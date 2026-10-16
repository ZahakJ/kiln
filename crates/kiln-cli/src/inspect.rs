// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli inspect <file> [--xml <part>] [--docx] [--json]`: the parts of an Office (OPC)
//! package with their content types and sizes, and the relationships of every source part;
//! `--xml` prints one part's XML indented, exactly as stored (prefixes are not normalised);
//! `--docx` reads a Word document with `kiln-io-docx` and prints its heading outline, counts and
//! import diagnostics.

use std::fmt::Write as _;
use std::path::PathBuf;

use kiln_ooxml::opc::{CONTENT_TYPES_PART, OpcPackage};
use kiln_ooxml::xml::{
    ParseOptions, XmlDocument, XmlElement, XmlNode, escape_attr, escape_text, parse_xml_with,
};
use serde_json::json;

use crate::CliError;

/// Usage text.
pub const USAGE: &str = "kiln-cli inspect <file> [--xml <part>] [--docx] [--json]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Options {
    input: PathBuf,
    xml: Option<String>,
    docx: bool,
    json: bool,
}

fn parse(args: &[String]) -> Result<Options, CliError> {
    let mut input = None;
    let mut xml = None;
    let mut json = false;
    let mut docx = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--xml" => {
                xml = Some(
                    it.next()
                        .cloned()
                        .ok_or_else(|| CliError::usage("--xml needs a part name", USAGE))?,
                );
            }
            "--json" => json = true,
            "--docx" => docx = true,
            s if s.starts_with('-') => {
                return Err(CliError::usage(format!("unknown option '{s}'"), USAGE));
            }
            s if input.is_none() => input = Some(PathBuf::from(s)),
            s => return Err(CliError::usage(format!("unexpected argument '{s}'"), USAGE)),
        }
    }
    Ok(Options {
        input: input.ok_or_else(|| CliError::usage("a file is required", USAGE))?,
        xml,
        docx,
        json,
    })
}

/// A part name with the leading slash OPC uses.
fn part_name(name: &str) -> String {
    if name.starts_with('/') {
        name.to_owned()
    } else {
        format!("/{name}")
    }
}

fn human_size(n: usize) -> String {
    #[allow(clippy::cast_precision_loss)]
    let f = n as f64;
    if n < 1024 {
        format!("{n} B")
    } else if f < 1024.0 * 1024.0 {
        format!("{:.1} KiB", f / 1024.0)
    } else {
        format!("{:.1} MiB", f / (1024.0 * 1024.0))
    }
}

/// Short form of a relationship type URI (its last path segment).
fn short_type(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap_or(uri)
}

fn open(path: &std::path::Path) -> Result<OpcPackage, CliError> {
    let bytes =
        std::fs::read(path).map_err(|e| CliError::Failed(format!("{}: {e}", path.display())))?;
    OpcPackage::open(&bytes).map_err(|e| CliError::Failed(format!("{}: {e}", path.display())))
}

/// Indents an element tree, keeping text-only elements on one line and mixed content as is.
fn pretty_element(out: &mut String, e: &XmlElement, depth: usize) {
    let pad = "  ".repeat(depth);
    let _ = write!(out, "{pad}<{}", e.name);
    for (k, v) in e.attrs.iter() {
        let _ = write!(out, " {k}=\"{}\"", escape_attr(v));
    }
    if e.children.is_empty() {
        out.push_str("/>\n");
        return;
    }
    let only_text = e.children.iter().all(|c| matches!(c, XmlNode::Text(_)));
    if only_text {
        out.push('>');
        for c in &e.children {
            if let XmlNode::Text(t) = c {
                push_text(out, t);
            }
        }
        let _ = writeln!(out, "</{}>", e.name);
        return;
    }
    out.push_str(">\n");
    for c in &e.children {
        pretty_node(out, c, depth + 1);
    }
    let _ = writeln!(out, "{pad}</{}>", e.name);
}

fn push_text(out: &mut String, t: &kiln_ooxml::xml::XmlText) {
    if t.cdata {
        let _ = write!(
            out,
            "<![CDATA[{}]]>",
            t.text.replace("]]>", "]]]]><![CDATA[>")
        );
    } else {
        out.push_str(&escape_text(&t.text));
    }
}

fn pretty_node(out: &mut String, n: &XmlNode, depth: usize) {
    let pad = "  ".repeat(depth);
    match n {
        XmlNode::Element(e) => pretty_element(out, e, depth),
        XmlNode::Text(t) => {
            out.push_str(&pad);
            push_text(out, t);
            out.push('\n');
        }
        XmlNode::Comment(c) => {
            let _ = writeln!(out, "{pad}<!--{c}-->");
        }
        XmlNode::Pi(p) => {
            let _ = writeln!(out, "{pad}<?{} {}?>", p.target, p.content);
        }
    }
}

/// The document indented two spaces per level, with its declaration.
fn pretty(doc: &XmlDocument) -> String {
    let mut out = String::new();
    if let Some(d) = &doc.declaration {
        let _ = write!(out, "<?xml version=\"{}\"", d.version);
        if let Some(enc) = &d.encoding {
            let _ = write!(out, " encoding=\"{enc}\"");
        }
        if let Some(sa) = &d.standalone {
            let _ = write!(out, " standalone=\"{sa}\"");
        }
        out.push_str("?>\n");
    }
    for n in &doc.prolog {
        pretty_node(&mut out, n, 0);
    }
    pretty_element(&mut out, &doc.root, 0);
    out
}

fn show_xml(pkg: &OpcPackage, name: &str) -> Result<String, CliError> {
    let wanted = part_name(name);
    let bytes = if wanted.eq_ignore_ascii_case(CONTENT_TYPES_PART) {
        pkg.content_types().to_bytes()
    } else {
        pkg.part(&wanted)
            .map_err(|e| CliError::Failed(e.to_string()))?
            .ok_or_else(|| {
                CliError::Failed(format!(
                    "no part {wanted} (run `kiln-cli inspect <file>` to list the parts)"
                ))
            })?
            .into_owned()
    };
    let options = ParseOptions {
        normalize_prefixes: false,
        drop_inter_element_whitespace: true,
        ..ParseOptions::default()
    };
    let doc = parse_xml_with(&bytes, options)
        .map_err(|e| CliError::Failed(format!("{wanted} is not well-formed XML: {e}")))?;
    Ok(pretty(&doc).trim_end().to_owned())
}

struct PartRow {
    name: String,
    content_type: String,
    size: usize,
}

fn rows(pkg: &OpcPackage) -> Result<Vec<PartRow>, CliError> {
    let mut out = vec![PartRow {
        name: CONTENT_TYPES_PART.to_owned(),
        content_type: "(content types)".to_owned(),
        size: pkg.content_types().to_bytes().len(),
    }];
    for name in pkg.all_part_names() {
        let size = pkg
            .part_bytes(name)
            .map_err(|e| CliError::Failed(e.to_string()))?
            .len();
        out.push(PartRow {
            name: name.to_owned(),
            content_type: pkg.content_type(name).unwrap_or("(none)").to_owned(),
            size,
        });
    }
    Ok(out)
}

fn listing(pkg: &OpcPackage, path: &std::path::Path) -> Result<String, CliError> {
    let rows = rows(pkg)?;
    let total: usize = rows.iter().map(|r| r.size).sum();
    let mut out = String::new();
    let main = pkg
        .main_document_part()
        .ok()
        .flatten()
        .unwrap_or_else(|| "(none)".into());
    let _ = writeln!(
        out,
        "{}: {} parts, {} uncompressed, main part {main}\n",
        path.display(),
        rows.len(),
        human_size(total)
    );
    let w = rows.iter().map(|r| r.name.len()).max().unwrap_or(4).max(4);
    let _ = writeln!(out, "{:<w$}  {:>10}  CONTENT TYPE", "PART", "SIZE");
    for r in &rows {
        let _ = writeln!(
            out,
            "{:<w$}  {:>10}  {}",
            r.name,
            human_size(r.size),
            r.content_type
        );
    }
    out.push_str("\nRELATIONSHIPS\n");
    for source in pkg.relationship_sources() {
        let rels = pkg
            .relationships(&source)
            .map_err(|e| CliError::Failed(e.to_string()))?;
        let _ = writeln!(out, "{source}");
        for r in rels.all() {
            let target = if r.is_external() {
                format!("{} (external)", r.target)
            } else {
                rels.resolve(r).unwrap_or_else(|| r.target.clone())
            };
            let _ = writeln!(
                out,
                "  {:<7} {:<22} -> {target}",
                r.id,
                short_type(&r.rel_type)
            );
        }
    }
    Ok(out.trim_end().to_owned())
}

fn listing_json(pkg: &OpcPackage) -> Result<String, CliError> {
    let parts: Vec<_> = rows(pkg)?
        .into_iter()
        .map(|r| json!({"name": r.name, "content_type": r.content_type, "size": r.size}))
        .collect();
    let mut rels = Vec::new();
    for source in pkg.relationship_sources() {
        let list = pkg
            .relationships(&source)
            .map_err(|e| CliError::Failed(e.to_string()))?;
        for r in list.all() {
            rels.push(json!({
                "source": source,
                "id": r.id,
                "type": r.rel_type,
                "target": r.target,
                "resolved": if r.is_external() { None } else { list.resolve(r) },
                "external": r.is_external(),
            }));
        }
    }
    let main = pkg.main_document_part().ok().flatten();
    let doc = json!({"main": main, "parts": parts, "relationships": rels});
    serde_json::to_string_pretty(&doc).map_err(|e| CliError::Failed(e.to_string()))
}

/// `--docx`: the document as Kiln Write reads it.
fn docx_summary(path: &std::path::Path, as_json: bool) -> Result<String, CliError> {
    let bytes =
        std::fs::read(path).map_err(|e| CliError::Failed(format!("{}: {e}", path.display())))?;
    let read = kiln_io_docx::read_docx(&bytes)
        .map_err(|e| CliError::Failed(format!("{}: {e}", path.display())))?;
    let s = kiln_io_docx::Summary::of(&read);
    if as_json {
        let doc = json!({
            "format": s.kind,
            "outline": s.outline.iter().map(|e| json!({"level": e.level + 1, "text": e.text})).collect::<Vec<_>>(),
            "counts": {
                "sections": s.sections, "paragraphs": s.paragraphs, "tables": s.tables,
                "list_paragraphs": s.list_paragraphs, "headers_footers": s.headers_footers,
                "footnotes": s.footnotes, "endnotes": s.endnotes, "comments": s.comments,
                "revisions": s.revisions, "pictures": s.pictures, "drawings": s.drawings,
                "fields": s.fields, "bookmarks": s.bookmarks, "equations": s.equations,
                "styles": s.styles, "lists": s.lists, "words": s.words,
                "passthrough_parts": s.passthrough_parts,
            },
            "warnings": read.warnings.iter().map(|w| json!({
                "code": w.code.as_str(), "severity": w.severity.as_str(),
                "message": w.message, "part": w.part, "path": w.path,
            })).collect::<Vec<_>>(),
        });
        return serde_json::to_string_pretty(&doc).map_err(|e| CliError::Failed(e.to_string()));
    }
    let mut out = format!("{}\n\n{}", path.display(), s.to_text());
    if !read.warnings.is_empty() {
        out.push_str("\nDiagnostics\n");
        for w in &read.warnings {
            let _ = writeln!(out, "  {w}");
        }
    }
    Ok(out.trim_end().to_owned())
}

/// Runs the subcommand.
///
/// # Errors
///
/// Usage errors, unreadable files and missing parts.
pub fn run(args: &[String]) -> Result<String, CliError> {
    let o = parse(args)?;
    if o.docx {
        if o.xml.is_some() {
            return Err(CliError::usage(
                "--docx and --xml cannot be combined",
                USAGE,
            ));
        }
        return docx_summary(&o.input, o.json);
    }
    let pkg = open(&o.input)?;
    match (&o.xml, o.json) {
        (Some(part), _) => show_xml(&pkg, part),
        (None, true) => listing_json(&pkg),
        (None, false) => listing(&pkg, &o.input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> String {
        kiln_testing::fixtures::fixtures_dir()
            .join(rel)
            .to_string_lossy()
            .into_owned()
    }

    #[test]
    fn lists_parts_types_sizes_and_relationships() {
        let out = run(&[fixture("docx/features/tables.docx")]).unwrap();
        assert!(out.contains("/word/document.xml"), "{out}");
        assert!(out.contains(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"
        ));
        assert!(out.contains("/_rels/.rels"));
        assert!(out.contains("RELATIONSHIPS"));
        assert!(out.contains("officeDocument"));
        assert!(out.contains("main part /word/document.xml"));
    }

    #[test]
    fn prints_json() {
        let out = run(&[fixture("xlsx/features/styles.xlsx"), "--json".into()]).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["main"], "/xl/workbook.xml");
        assert!(v["parts"].as_array().unwrap().len() > 5);
        assert!(
            v["relationships"]
                .as_array()
                .unwrap()
                .iter()
                .any(|r| r["source"] == "/")
        );
    }

    #[test]
    fn pretty_prints_one_part_as_stored() {
        let out = run(&[
            fixture("docx/features/tables.docx"),
            "--xml".into(),
            "word/document.xml".into(),
        ])
        .unwrap();
        assert!(out.starts_with("<?xml version=\"1.0\""), "{out}");
        assert!(out.contains("\n<w:document"));
        assert!(out.contains("\n  <w:body>"));
        assert!(out.lines().any(|l| l.trim_start().starts_with("<w:tbl>")));
        let ct = run(&[
            fixture("docx/features/tables.docx"),
            "--xml".into(),
            "[Content_Types].xml".into(),
        ])
        .unwrap();
        assert!(ct.contains("<Types"));
    }

    #[test]
    fn docx_prints_the_outline_and_counts() {
        let out = run(&["--docx".into(), fixture("docx/features/long-text.docx")]).unwrap();
        assert!(out.contains("Outline\n"), "{out}");
        assert!(out.contains("  paragraphs "), "{out}");
        assert!(out.contains("  tables "), "{out}");
        let json = run(&[
            fixture("docx/features/tables.docx"),
            "--docx".into(),
            "--json".into(),
        ])
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["format"], "docx");
        assert_eq!(v["counts"]["tables"], 4);
        assert!(matches!(
            run(&[fixture("xlsx/features/styles.xlsx"), "--docx".into()]),
            Err(CliError::Failed(_))
        ));
        assert!(matches!(
            run(&["a.docx".into(), "--docx".into(), "--xml".into(), "x".into()]),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn reports_bad_input() {
        assert!(matches!(run(&[]), Err(CliError::Usage(_))));
        assert!(matches!(
            run(&["a.docx".into(), "--frob".into()]),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            run(&["/nonexistent/a.docx".into()]),
            Err(CliError::Failed(_))
        ));
        let missing = run(&[
            fixture("docx/features/tables.docx"),
            "--xml".into(),
            "/nope.xml".into(),
        ])
        .unwrap_err();
        assert!(missing.to_string().contains("no part /nope.xml"));
        let not_zip = run(&[fixture("docx/text/oracle.json")]).unwrap_err();
        assert!(matches!(not_zip, CliError::Failed(_)));
    }
}
