// SPDX-License-Identifier: AGPL-3.0-or-later
//! Charts inside Office packages: finding chart parts, reading and writing them, and reaching a
//! chart's embedded workbook. This is the chart part handler `kiln-io-xlsx`, `kiln-io-docx` and
//! `kiln-io-pptx` register for drawing graphic frames (`c:chart` / `cx:chart` references).
//!
//! The format crates own the drawing anchors and relationships; they hand this module the part
//! name a graphic frame points at.

use kiln_common::{KilnError, Result};
use kiln_ooxml::opc::OpcPackage;

use crate::facts::chart_facts;
use crate::model::{ChartSpace, PartFormat};
use crate::read::{format_error, read_chart};
use crate::write::write_chart;

/// Relationship type of an embedded workbook (`c:externalData` → `embeddings/*.xlsx`).
pub const PACKAGE_RELATIONSHIP: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/package";

/// Whether a part's content type is a chart part this crate reads.
#[must_use]
pub fn is_chart_content_type(content_type: &str) -> bool {
    content_type == PartFormat::Classic.content_type()
        || content_type == PartFormat::ChartEx.content_type()
}

/// Every chart part of a package, in part order.
#[must_use]
pub fn chart_parts(pkg: &OpcPackage) -> Vec<String> {
    pkg.part_names()
        .filter(|n| pkg.content_type(n).is_some_and(is_chart_content_type))
        .map(str::to_owned)
        .collect()
}

/// Reads the chart part `name`.
///
/// # Errors
///
/// `NotFound` when the part is missing, `Format` when it is not a chart.
pub fn read_part(pkg: &OpcPackage, name: &str) -> Result<ChartSpace> {
    let bytes = pkg
        .part(name)
        .map_err(|e| format_error(e.to_string()))?
        .ok_or_else(|| KilnError::NotFound {
            what: format!("chart part {name}"),
        })?;
    read_chart(&bytes)
}

/// Writes a chart back to `name`, fixing the part's content type when the chart changed
/// vocabulary (a column chart turned into a treemap is a chartex part). The relationship type from
/// the drawing must then change too: [`PartFormat::relationship_type`] gives it.
///
/// # Errors
///
/// When the package rejects the part.
pub fn write_part(pkg: &mut OpcPackage, name: &str, chart: &ChartSpace) -> Result<()> {
    pkg.set_part(name, write_chart(chart))
        .map_err(|e| format_error(e.to_string()))?;
    let ct = chart.format.content_type();
    if pkg.content_type(name) != Some(ct) {
        pkg.set_content_type(name, ct)
            .map_err(|e| format_error(e.to_string()))?;
    }
    Ok(())
}

/// The embedded workbook of the chart part `name`: its part name and bytes (Write and Deck charts
/// keep their data in `embeddings/Microsoft_Excel_Worksheet*.xlsx`). The host reads it with
/// `kiln-io-xlsx` and refreshes the chart from it with [`crate::data::refresh`].
#[must_use]
pub fn embedded_workbook(
    pkg: &OpcPackage,
    name: &str,
    chart: &ChartSpace,
) -> Option<(String, Vec<u8>)> {
    let ext = chart.external_data.as_ref()?;
    let rels = pkg.relationships(name).ok()?;
    let rel = rels.get(&ext.rel_id)?;
    if rel.is_external() {
        return None;
    }
    let target = rels.resolve(rel)?;
    let bytes = pkg.part(&target).ok()??;
    Some((target, bytes.into_owned()))
}

/// Structural facts of every chart in a package, each prefixed with its part name, for the
/// fidelity scorer's snapshots (`kiln-testing` adapters append them to `Snapshot::facts`).
/// Unreadable chart parts contribute one fact saying so.
#[must_use]
pub fn package_chart_facts(pkg: &OpcPackage) -> Vec<String> {
    let mut out = Vec::new();
    for name in chart_parts(pkg) {
        match read_part(pkg, &name) {
            Ok(cs) => out.extend(chart_facts(&cs).into_iter().map(|f| format!("{name}|{f}"))),
            Err(_) => out.push(format!("{name}|unreadable")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> OpcPackage {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures")
            .join(rel);
        OpcPackage::open(&std::fs::read(p).unwrap()).unwrap()
    }

    #[test]
    fn finds_reads_and_writes_chart_parts() {
        let mut pkg = fixture("pptx/features/charts.pptx");
        let parts = chart_parts(&pkg);
        assert_eq!(parts.len(), 9);
        let cs = read_part(&pkg, &parts[0]).unwrap();
        let (name, bytes) = embedded_workbook(&pkg, &parts[0], &cs).unwrap();
        assert!(name.ends_with(".xlsx"), "{name}");
        assert!(
            OpcPackage::open(&bytes).is_ok(),
            "the embedding is a workbook"
        );
        let mut tree = cs.clone();
        crate::edit::change_type(&mut tree, kiln_protocol::commands::ChartKind::Treemap);
        write_part(&mut pkg, &parts[0], &tree).unwrap();
        assert_eq!(
            pkg.content_type(&parts[0]),
            Some(PartFormat::ChartEx.content_type())
        );
        assert_eq!(
            read_part(&pkg, &parts[0]).unwrap().format,
            PartFormat::ChartEx
        );
        assert!(read_part(&pkg, "/ppt/charts/nope.xml").is_err());
        let facts = package_chart_facts(&fixture("xlsx/charts/treemap.xlsx"));
        assert!(
            facts
                .iter()
                .any(|f| f.starts_with("/xl/charts/chartEx1.xml|group[0]=Hierarchy")),
            "{facts:?}"
        );
    }
}
