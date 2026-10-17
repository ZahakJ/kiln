// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `.pptx` fidelity adapter (`kiln-io-pptx`): the read half (WP-77).
//!
//! - `snapshot`: the reader's canonical facts ([`kiln_io_pptx::snapshot_facts`]: slide size,
//!   masters and layouts, and per slide its layout, flags, background, shapes with kind, name,
//!   transform, placeholder, geometry, fill, table shape, chart and media, transition, animation
//!   effects, notes, comments; sections) plus the structure of every chart
//!   ([`kiln_charts::package::package_chart_facts`]), and the text of every slide, its notes and
//!   comments ([`kiln_io_pptx::plain_text`]).
//! - `roundtrip`: needs the writer (WP-79), which replaces the refusal below with read → write.
//!   Until then the adapter is not in [`crate::formats::registered`], so `cargo xtask fidelity`
//!   does not report every deck as failed; the snapshot is tested here and by `kiln-io-pptx`.

use kiln_io_pptx::{plain_text, read_pptx, snapshot_facts};
use kiln_ooxml::opc::OpcPackage;

use crate::fidelity::{Roundtrip, RoundtripOutput, Snapshot};
use crate::fixtures::FixtureFormat;

/// The `.pptx` adapter.
#[derive(Debug, Clone, Copy, Default)]
pub struct Pptx;

impl Roundtrip for Pptx {
    fn format(&self) -> FixtureFormat {
        FixtureFormat::Pptx
    }

    fn name(&self) -> &'static str {
        "kiln-io-pptx"
    }

    fn snapshot(&self, bytes: &[u8]) -> Result<Snapshot, String> {
        let read = read_pptx(bytes).map_err(|e| e.to_string())?;
        let mut facts = snapshot_facts(&read);
        if let Ok(pkg) = OpcPackage::open(bytes) {
            facts.extend(kiln_charts::package::package_chart_facts(&pkg));
        }
        Ok(Snapshot {
            facts,
            text: plain_text(&read),
            cells: Default::default(),
        })
    }

    fn roundtrip(&self, _bytes: &[u8]) -> Result<RoundtripOutput, String> {
        Err("the .pptx writer is not built yet (WP-79)".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> Vec<u8> {
        std::fs::read(crate::fixtures::fixtures_dir().join(rel)).expect("fixture")
    }

    #[test]
    fn snapshots_hold_facts_text_and_charts() {
        let s = Pptx
            .snapshot(&fixture("pptx/features/charts.pptx"))
            .unwrap();
        assert!(
            s.facts.iter().any(|f| f.contains("/chart=/ppt/charts/")),
            "{:?}",
            s.facts
        );
        assert!(s.facts.iter().any(|f| f.starts_with("/ppt/charts/chart")));
        assert!(s.cells.is_empty());
        let t = Pptx.snapshot(&fixture("pptx/features/notes.pptx")).unwrap();
        assert!(!t.text.trim().is_empty());
        // Two reads of one file agree exactly.
        let again = Pptx.snapshot(&fixture("pptx/features/notes.pptx")).unwrap();
        assert_eq!(t, again);
        assert!(Pptx.snapshot(b"not a zip").is_err());
        assert_eq!(Pptx.format(), FixtureFormat::Pptx);
        assert!(Pptx.roundtrip(b"").unwrap_err().contains("WP-79"));
    }
}
