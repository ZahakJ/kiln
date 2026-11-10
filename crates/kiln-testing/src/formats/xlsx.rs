// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `.xlsx` fidelity adapter (`kiln-io-xlsx`).
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
        let read_cells = snapshot_cells(wb);
        let text = read_cells
            .values()
            .filter(|(_, formula)| formula.is_none())
            .map(|(value, _)| value.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        // Values as the file stores them, never as anything recalculated them, and with cells
        // the reader may not have seen.
        let stored = crate::fidelity::stored::stored_cells(bytes).unwrap_or_default();
        let mut cells: std::collections::BTreeMap<String, CellFact> = read_cells
            .into_iter()
            .map(|(k, (_, formula))| {
                let value = stored.get(&k).cloned().unwrap_or_default();
                (k, CellFact { value, formula })
            })
            .collect();
        for (k, value) in stored {
            cells.entry(k).or_insert(CellFact {
                value,
                formula: None,
            });
        }
        // Charts are scored structurally: their series, types and axes as facts.
        let mut facts = snapshot_facts(wb);
        facts.extend(kiln_charts::package::package_chart_facts(
            read.source.package(),
        ));
        Ok(Snapshot { facts, text, cells })
    }

    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String> {
        let written = rewrite_xlsx(bytes, &WriteOptions::default()).map_err(|e| e.to_string())?;
        let mut regenerated = written.regenerated;
        // The calculation chain is a cache Excel rebuilds; anything else left out is a loss.
        let chain = written
            .dropped
            .iter()
            .filter(|d| d.eq_ignore_ascii_case("/xl/calcChain.xml"))
            .cloned();
        regenerated.extend(chain);
        regenerated.extend(crate::fidelity::package::excused_drops(
            bytes,
            &written.dropped,
        ));
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
    fn values_compare_what_the_files_store_not_a_recalculation() {
        // Two files with one formula and different stored results: a score that recalculated both
        // sides would call them equal.
        let with = |v: &str| {
            let mut pkg = OpcPackage::open(&fixture("xlsx/manual/minimal.xlsx")).unwrap();
            let sheet = format!(
                "<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData><row r=\"1\"><c r=\"A1\"><f>1+1</f><v>{v}</v></c></row></sheetData></worksheet>"
            );
            pkg.set_part("/xl/worksheets/sheet1.xml", sheet.into_bytes())
                .unwrap();
            pkg.save().unwrap()
        };
        let (a, b) = (
            Xlsx.snapshot(&with("5")).unwrap(),
            Xlsx.snapshot(&with("2")).unwrap(),
        );
        let (values, _) = crate::fidelity::cell_scores(&a.cells, &b.cells);
        assert_eq!(values, Some(0.0));
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
