// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `.docx` fidelity adapter (`kiln-io-docx`, WP-62 reader + WP-65 writer).
//!
//! - `snapshot`: the reader's canonical model facts ([`kiln_io_docx::snapshot_facts`]: sections,
//!   paragraphs with their styles, list references and run formatting, tables with merges,
//!   marks, objects, headers and footers, notes, comments, styles, list levels) and the plain
//!   text of every story (headers and footers, body with list labels, notes, comments).
//! - `roundtrip`: read → write with every model part regenerated (no part is copied because it
//!   was not edited: that would measure nothing). The writer reports the parts it produced,
//!   the relationships parts it changed and `[Content_Types].xml`; everything else (media,
//!   passthrough parts and their relationships, an unchanged theme) must come out
//!   byte-identical.

use kiln_doc::Block;
use kiln_doc::query::PlainTextOptions;
use kiln_io_docx::{WriteOptions, read_docx, rewrite_docx, snapshot_facts};

use crate::fidelity::{Roundtrip, RoundtripOutput, Snapshot};
use crate::fixtures::FixtureFormat;

/// The `.docx` adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Docx;

fn comment_text(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Paragraph(p) => {
                out.push_str(&p.content.plain_text(false));
                out.push('\n');
            }
            Block::Table(t) => {
                for r in &t.rows {
                    for c in &r.cells {
                        comment_text(&c.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => comment_text(&s.blocks, out),
            Block::Field(f) => comment_text(&f.blocks, out),
            Block::Opaque(_) => {}
        }
    }
}

impl Roundtrip for Docx {
    fn format(&self) -> FixtureFormat {
        FixtureFormat::Docx
    }

    fn name(&self) -> &'static str {
        "kiln-io-docx"
    }

    fn snapshot(&self, bytes: &[u8]) -> Result<Snapshot, String> {
        let read = read_docx(bytes).map_err(|e| e.to_string())?;
        let doc = &read.document;
        let mut text = doc.plain_text(PlainTextOptions {
            include_headers_footers: true,
            include_notes: true,
            accept_changes: false,
            list_labels: true,
        });
        for c in &doc.comments {
            text.push('\n');
            comment_text(&c.blocks, &mut text);
        }
        Ok(Snapshot {
            facts: snapshot_facts(doc),
            text,
            cells: Default::default(),
        })
    }

    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String> {
        let written = rewrite_docx(bytes, &WriteOptions::default()).map_err(|e| e.to_string())?;
        let mut regenerated = written.regenerated;
        // Parts the writer deliberately left out (none on an unedited round trip) are its
        // decision too: the byte check reports them as dropped otherwise.
        regenerated.extend(written.dropped);
        Ok(RoundtripOutput {
            bytes: written.bytes,
            regenerated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fidelity::package::compare_packages;
    use kiln_ooxml::opc::OpcPackage;

    fn fixture(rel: &str) -> Vec<u8> {
        std::fs::read(crate::fixtures::fixtures_dir().join(rel)).expect("fixture")
    }

    #[test]
    fn snapshots_hold_facts_and_text() {
        let s = Docx
            .snapshot(&fixture("docx/features/tables.docx"))
            .unwrap();
        assert!(s.facts.iter().any(|f| f.contains("/tbl@")));
        assert!(!s.text.trim().is_empty());
        assert!(s.cells.is_empty());
        assert!(Docx.snapshot(b"not a zip").is_err());
    }

    #[test]
    fn round_trips_keep_facts_text_and_untouched_parts() {
        for rel in [
            "docx/features/comments.docx",
            "docx/features/images-inline.docx",
            "docx/features/sections.docx",
        ] {
            let bytes = fixture(rel);
            let out = Docx.roundtrip(&bytes).unwrap();
            let (a, b) = (
                Docx.snapshot(&bytes).unwrap(),
                Docx.snapshot(&out.bytes).unwrap(),
            );
            assert_eq!(crate::fidelity::text_score(&a.text, &b.text), 1.0, "{rel}");
            let structural = crate::fidelity::overlap(
                a.facts.iter().map(String::as_str),
                b.facts.iter().map(String::as_str),
            );
            assert_eq!(structural, 1.0, "{rel}");
            let cmp = compare_packages(
                &OpcPackage::open(&bytes).unwrap(),
                &OpcPackage::open(&out.bytes).unwrap(),
                &out.regenerated,
            );
            assert!(
                cmp.differing.is_empty() && cmp.missing.is_empty(),
                "{rel}: {cmp:?}"
            );
            // Media and passthrough parts are compared, not listed as regenerated.
            assert!(cmp.identical > 3, "{rel}: {cmp:?}");
        }
    }
}
