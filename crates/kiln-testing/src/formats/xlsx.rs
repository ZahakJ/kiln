// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `.xlsx` fidelity adapter (`kiln-io-xlsx`, WP-70).
//!
//! - `snapshot`: the reader's model facts ([`kiln_io_xlsx::snapshot_facts`]: sheets, views, page
//!   setup, protection, merges, conditional formats, validation, hyperlinks, notes, threaded
//!   comments, tables, drawings, row and column properties, defined names, the format of every
//!   formatted cell by content), the text of every cell, and every cell's value and formula
//!   ([`kiln_io_xlsx::snapshot_cells`], spilled values included) after calculation.
//! - `roundtrip`: read → write with the source; every part the model covers is regenerated
//!   (workbook, worksheets, styles, shared strings, and the parts of notes, threaded comments and
//!   tables when they changed), everything else must come out byte-identical. `calcChain.xml`
//!   is dropped on purpose (Excel rebuilds it) and counted with the regenerated parts.

use kiln_io_xlsx::{WriteOptions, read_xlsx, rewrite_xlsx, snapshot_cells, snapshot_facts};

use crate::fidelity::{CellFact, Roundtrip, RoundtripOutput, Snapshot};
use crate::fixtures::FixtureFormat;

/// The `.xlsx` adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Xlsx;

impl Roundtrip for Xlsx {
    fn format(&self) -> FixtureFormat {
        FixtureFormat::Xlsx
    }

    fn name(&self) -> &'static str {
        "kiln-io-xlsx"
    }

    fn snapshot(&self, bytes: &[u8]) -> Result<Snapshot, String> {
        let read = read_xlsx(bytes).map_err(|e| e.to_string())?;
        let wb = &read.workbook;
        let cells: std::collections::BTreeMap<String, CellFact> = snapshot_cells(wb)
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
            facts: snapshot_facts(wb),
            text,
            cells,
        })
    }

    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String> {
        let written = rewrite_xlsx(bytes, &WriteOptions::default()).map_err(|e| e.to_string())?;
        let mut regenerated = written.regenerated;
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
    fn snapshots_hold_facts_cells_and_text() {
        let s = Xlsx
            .snapshot(&fixture("xlsx/features/formulas.xlsx"))
            .expect("snapshot");
        assert!(s.facts.iter().any(|f| f.starts_with("sheet[0]=Formulas")));
        assert!(s.cells.values().filter(|c| c.formula.is_some()).count() > 250);
        assert!(s.text.contains("Statistical"));
        assert!(Xlsx.snapshot(b"not a zip").is_err());
    }

    #[test]
    fn round_trips_keep_facts_cells_and_untouched_parts() {
        for rel in [
            "xlsx/features/charts.xlsx",
            "xlsx/features/comments.xlsx",
            "xlsx/features/tables.xlsx",
        ] {
            let bytes = fixture(rel);
            let out = Xlsx.roundtrip(&bytes).expect("roundtrip");
            let (a, b) = (
                Xlsx.snapshot(&bytes).expect("a"),
                Xlsx.snapshot(&out.bytes).expect("b"),
            );
            assert_eq!(
                crate::fidelity::overlap(
                    a.facts.iter().map(String::as_str),
                    b.facts.iter().map(String::as_str),
                ),
                1.0,
                "{rel}"
            );
            let (values, formulas) = crate::fidelity::cell_scores(&a.cells, &b.cells);
            assert_eq!(values, Some(1.0), "{rel}");
            assert!(formulas.is_none_or(|f| f == 1.0), "{rel}");
            let cmp = compare_packages(
                &OpcPackage::open(&bytes).expect("in"),
                &OpcPackage::open(&out.bytes).expect("out"),
                &out.regenerated,
            );
            assert!(
                cmp.differing.is_empty() && cmp.missing.is_empty(),
                "{rel}: {cmp:?}"
            );
            assert!(cmp.identical >= 3, "{rel}: {cmp:?}");
        }
    }
}
