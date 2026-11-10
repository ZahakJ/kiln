// SPDX-License-Identifier: AGPL-3.0-or-later
//! The OpenDocument fidelity adapter (`kiln-io-odf`): `.odt`, `.ods` and `.odp` by the
//! package's media type.
//!
//! - `snapshot`: the same model facts as the Office adapters measure for the same models —
//!   [`kiln_io_docx::snapshot_facts`] for text documents, [`kiln_io_xlsx::snapshot_facts`] and
//!   [`kiln_io_xlsx::snapshot_cells`] for spreadsheets, [`kiln_io_odf::deck_facts`] for
//!   presentations (shapes with their resolved positions, fills and text formatting) — and the
//!   plain text (every story, every cell, every slide and its notes).
//! - `roundtrip`: read → write. ODF packages are written whole from the model, so every file of
//!   the output counts as regenerated; there are no untouched parts to compare.

use kiln_doc::query::PlainTextOptions;
use kiln_io_odf::{OdfKind, Package, WriteOptions};

use crate::fidelity::{CellFact, Roundtrip, RoundtripOutput, Snapshot};
use crate::fixtures::FixtureFormat;

/// The ODF adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Odf;

fn kind(bytes: &[u8]) -> Result<OdfKind, String> {
    Package::open(bytes)
        .map_err(|e| e.to_string())?
        .kind
        .ok_or_else(|| "not an OpenDocument file".to_owned())
}

impl Roundtrip for Odf {
    fn format(&self) -> FixtureFormat {
        FixtureFormat::Odf
    }

    fn name(&self) -> &'static str {
        "kiln-io-odf"
    }

    fn snapshot(&self, bytes: &[u8]) -> Result<Snapshot, String> {
        match kind(bytes)? {
            OdfKind::Text => {
                let read = kiln_io_odf::read_odt(bytes).map_err(|e| e.to_string())?;
                let doc = &read.document;
                Ok(Snapshot {
                    facts: kiln_io_docx::snapshot_facts(doc),
                    text: doc.plain_text(PlainTextOptions {
                        include_headers_footers: true,
                        include_notes: true,
                        accept_changes: false,
                        list_labels: true,
                    }),
                    cells: Default::default(),
                })
            }
            OdfKind::Spreadsheet => {
                let read = kiln_io_odf::read_ods(bytes).map_err(|e| e.to_string())?;
                let wb = &read.workbook;
                let cells: std::collections::BTreeMap<String, CellFact> =
                    kiln_io_xlsx::snapshot_cells(wb)
                        .into_iter()
                        .map(|(k, (value, formula))| (k, CellFact { value, formula }))
                        .collect();
                let text = cells
                    .values()
                    .filter(|c| c.formula.is_none())
                    .map(|c| c.value.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(Snapshot {
                    facts: kiln_io_xlsx::snapshot_facts(wb),
                    text,
                    cells,
                })
            }
            OdfKind::Presentation => {
                let read = kiln_io_odf::read_odp(bytes).map_err(|e| e.to_string())?;
                Ok(Snapshot {
                    facts: kiln_io_odf::deck_facts(&read.presentation),
                    text: kiln_io_odf::deck_text(&read.presentation),
                    cells: Default::default(),
                })
            }
        }
    }

    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String> {
        let options = WriteOptions::default();
        let out = match kind(bytes)? {
            OdfKind::Text => {
                let read = kiln_io_odf::read_odt(bytes).map_err(|e| e.to_string())?;
                kiln_io_odf::write_odt(&read.document, &options)
            }
            OdfKind::Spreadsheet => {
                let read = kiln_io_odf::read_ods(bytes).map_err(|e| e.to_string())?;
                kiln_io_odf::write_ods(
                    &read.workbook,
                    &WriteOptions {
                        properties: Some(read.properties),
                        charts: read.charts,
                        ..options
                    },
                )
            }
            OdfKind::Presentation => {
                let read = kiln_io_odf::read_odp(bytes).map_err(|e| e.to_string())?;
                kiln_io_odf::write_odp(
                    &read.presentation,
                    &WriteOptions {
                        properties: Some(read.properties),
                        ..options
                    },
                )
            }
        }
        .map_err(|e| e.to_string())?;
        let regenerated = Package::open(&out)
            .map(|p| p.files.iter().map(|(n, _)| format!("/{n}")).collect())
            .unwrap_or_default();
        Ok(RoundtripOutput {
            bytes: out,
            regenerated,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> Vec<u8> {
        std::fs::read(crate::fixtures::fixtures_dir().join(rel)).expect("fixture")
    }

    #[test]
    fn every_kind_snapshots_and_round_trips() {
        for rel in [
            "odf/features/tables.odt",
            "odf/features/formulas.ods",
            "odf/features/placeholders.odp",
        ] {
            let bytes = fixture(rel);
            let a = Odf.snapshot(&bytes).expect("snapshot");
            assert!(!a.facts.is_empty() && !a.text.trim().is_empty(), "{rel}");
            let out = Odf.roundtrip(&bytes).expect("roundtrip");
            assert!(out.regenerated.iter().any(|p| p == "/content.xml"), "{rel}");
            let b = Odf.snapshot(&out.bytes).expect("re-read");
            assert_eq!(crate::fidelity::text_score(&a.text, &b.text), 1.0, "{rel}");
            let s = crate::fidelity::overlap(
                a.facts.iter().map(String::as_str),
                b.facts.iter().map(String::as_str),
            );
            assert!(s >= 0.9, "{rel}: {s}");
            if rel.ends_with(".ods") {
                let (values, formulas) = crate::fidelity::cell_scores(&a.cells, &b.cells);
                assert_eq!(values, Some(1.0), "{rel}");
                assert_eq!(formulas, Some(1.0), "{rel}");
            }
        }
        assert!(Odf.snapshot(b"not a zip").is_err());
        assert_eq!(Odf.format(), FixtureFormat::Odf);
    }
}
