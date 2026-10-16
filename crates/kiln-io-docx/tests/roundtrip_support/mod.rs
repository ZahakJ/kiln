// SPDX-License-Identifier: AGPL-3.0-or-later
//! Round-trip helpers: read → write → read of a package, and a field-by-field comparison of two
//! reads that names what differs.
#![allow(dead_code)]

use std::fmt::Debug;

use kiln_io_docx::{DocxRead, WriteOptions, WriteSource, read_docx, write_docx};
use kiln_ooxml::opc::OpcPackage;

/// The save time the tests stamp (when they stamp one).
pub const SAVE_TIME: &str = "2026-10-16T12:00:00Z";

/// One round trip.
pub struct RoundTrip {
    pub original: Vec<u8>,
    pub a: DocxRead,
    pub bytes: Vec<u8>,
    pub b: DocxRead,
    pub regenerated: Vec<String>,
    pub dropped: Vec<String>,
    pub issues: Vec<kiln_io_docx::WriteIssue>,
}

impl RoundTrip {
    /// The text of a part of the written package.
    pub fn xml(&self, part: &str) -> String {
        let pkg = OpcPackage::open(&self.bytes).expect("written package opens");
        pkg.part(part)
            .ok()
            .flatten()
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default()
    }
}

/// read → write (every part regenerated, the original package at hand) → read.
pub fn round_trip(original: &[u8]) -> RoundTrip {
    round_trip_with(original, &WriteOptions::default())
}

/// [`round_trip`] with options.
pub fn round_trip_with(original: &[u8], options: &WriteOptions<'_>) -> RoundTrip {
    let pkg = OpcPackage::open(original).expect("original opens");
    let a = kiln_io_docx::read_package(&pkg).expect("original reads");
    let source = WriteSource::from_read(&a).with_package(&pkg);
    let written = write_docx(&a.document, Some(&source), options).expect("writes");
    let b = read_docx(&written.bytes).expect("written file reads");
    RoundTrip {
        original: original.to_vec(),
        a,
        bytes: written.bytes,
        b,
        regenerated: written.regenerated,
        dropped: written.dropped,
        issues: written.issues,
    }
}

fn check<T: PartialEq + Debug>(what: &str, a: &T, b: &T, out: &mut Vec<String>) {
    if a != b {
        let (sa, sb) = (format!("{a:#?}"), format!("{b:#?}"));
        // The first differing line, with a little context.
        let la: Vec<&str> = sa.lines().collect();
        let lb: Vec<&str> = sb.lines().collect();
        let i = la
            .iter()
            .zip(lb.iter())
            .position(|(x, y)| x != y)
            .unwrap_or(la.len().min(lb.len()));
        let from = i.saturating_sub(6);
        out.push(format!(
            "{what} differs at line {i}:\n--- before\n{}\n--- after\n{}",
            la[from..(i + 3).min(la.len())].join("\n"),
            lb[from..(i + 3).min(lb.len())].join("\n")
        ));
    }
}

/// What differs between two reads, the reader's diagnostics aside (a file the reader had to
/// repair comes back regular, without the diagnostic).
pub fn differences_but_warnings(a: &DocxRead, b: &DocxRead) -> Vec<String> {
    differences(a, b)
        .into_iter()
        .filter(|d| !d.starts_with("warnings "))
        .collect()
}

/// What differs between two reads (empty when they agree on the model and the extras).
pub fn differences(a: &DocxRead, b: &DocxRead) -> Vec<String> {
    let mut out = Vec::new();
    let (da, db) = (&a.document, &b.document);
    check("sections", &da.sections, &db.sections, &mut out);
    check(
        "headers/footers",
        &da.headers_footers,
        &db.headers_footers,
        &mut out,
    );
    check("notes", &da.notes, &db.notes, &mut out);
    check("comments", &da.comments, &db.comments, &mut out);
    check("styles", &da.styles, &db.styles, &mut out);
    check("numbering", &da.numbering, &db.numbering, &mut out);
    check("settings", &da.settings, &db.settings, &mut out);
    check("properties", &da.properties, &db.properties, &mut out);
    check("theme", &da.theme, &db.theme, &mut out);
    check("media", &da.media, &db.media, &mut out);
    let (xa, xb) = (&a.extras, &b.extras);
    check(
        "extras.paragraphs",
        &xa.paragraphs,
        &xb.paragraphs,
        &mut out,
    );
    check("extras.tables", &xa.tables, &xb.tables, &mut out);
    check("extras.rows", &xa.rows, &xb.rows, &mut out);
    check("extras.cells", &xa.cells, &xb.cells, &mut out);
    check(
        "extras.cell_end_paragraphs",
        &xa.cell_end_paragraphs,
        &xb.cell_end_paragraphs,
        &mut out,
    );
    check("extras.sections", &xa.sections, &xb.sections, &mut out);
    check("extras.note_ids", &xa.note_ids, &xb.note_ids, &mut out);
    check(
        "extras.comment_ids",
        &xa.comment_ids,
        &xb.comment_ids,
        &mut out,
    );
    check("extras.people", &xa.people, &xb.people, &mut out);
    check("extras.settings", &xa.settings, &xb.settings, &mut out);
    check(
        "extras.background",
        &xa.background,
        &xb.background,
        &mut out,
    );
    check(
        "extras.latent_styles",
        &xa.latent_styles,
        &xb.latent_styles,
        &mut out,
    );
    check(
        "extras.num_pic_bullets",
        &xa.num_pic_bullets,
        &xb.num_pic_bullets,
        &mut out,
    );
    check(
        "extras.header_footer_parts",
        &xa.header_footer_parts,
        &xb.header_footer_parts,
        &mut out,
    );
    check(
        "passthrough parts",
        &part_names(a),
        &part_names(b),
        &mut out,
    );
    check("charts", &a.charts.len(), &b.charts.len(), &mut out);
    check("kind", &a.kind, &b.kind, &mut out);
    let warn = |r: &DocxRead| -> Vec<String> {
        let mut w: Vec<String> = r
            .warnings
            .iter()
            .map(|w| format!("{}: {}", w.code.as_str(), w.message))
            .collect();
        w.sort();
        w
    };
    check("warnings", &warn(a), &warn(b), &mut out);
    out
}

fn part_names(r: &DocxRead) -> Vec<String> {
    let mut v: Vec<String> = r.passthrough.parts().map(|p| p.name.clone()).collect();
    v.sort();
    v
}

/// A package's part names (with `.rels` parts).
pub fn package_parts(bytes: &[u8]) -> Vec<String> {
    let pkg = OpcPackage::open(bytes).expect("opens");
    let mut v: Vec<String> = pkg.all_part_names().map(str::to_owned).collect();
    v.sort();
    v
}
