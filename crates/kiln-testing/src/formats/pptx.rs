// SPDX-License-Identifier: AGPL-3.0-or-later
//! The `.pptx` fidelity adapter (`kiln-io-pptx`: reader WP-77, writer WP-79).
//!
//! - `snapshot`: the reader's canonical facts ([`kiln_io_pptx::snapshot_facts`]: slide size,
//!   masters and layouts, and per slide its layout, flags, background, shapes with kind, name,
//!   transform, placeholder, geometry, fill, table shape, chart and media, transition, animation
//!   effects, notes, comments; sections) plus the structure of every chart
//!   ([`kiln_charts::package::package_chart_facts`]), and the text of every slide, its notes and
//!   comments ([`kiln_io_pptx::plain_text`]).
//! - `roundtrip`: read → write with every model part written from the model (masters, layouts,
//!   slides, notes, comments, the presentation, properties; nothing is copied because it was
//!   not edited). The writer reports the parts it produced, the relationships parts it changed
//!   and `[Content_Types].xml`; everything else (media, charts, passthrough parts and their
//!   relationships, an unchanged theme) must come out byte-identical.

use kiln_io_pptx::{WriteOptions, plain_text, read_pptx, rewrite_pptx, snapshot_facts};
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

    fn roundtrip(&self, bytes: &[u8]) -> Result<RoundtripOutput, String> {
        let written = rewrite_pptx(bytes, &WriteOptions::default()).map_err(|e| e.to_string())?;
        let mut regenerated = written.regenerated;
        // Parts the writer deliberately left out are its decision too: the byte check reports
        // them as dropped otherwise.
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
        assert!(Pptx.roundtrip(b"not a zip").is_err());
    }

    #[test]
    fn round_trips_keep_facts_text_and_untouched_parts() {
        use crate::fidelity::package::compare_packages;
        for rel in [
            "pptx/features/charts.pptx",
            "pptx/features/notes.pptx",
            "pptx/features/animations.pptx",
        ] {
            let bytes = fixture(rel);
            let out = Pptx.roundtrip(&bytes).expect("roundtrip");
            let (a, b) = (
                Pptx.snapshot(&bytes).expect("a"),
                Pptx.snapshot(&out.bytes).expect("b"),
            );
            assert_eq!(
                crate::fidelity::overlap(
                    a.facts.iter().map(String::as_str),
                    b.facts.iter().map(String::as_str),
                ),
                1.0,
                "{rel}"
            );
            assert_eq!(a.text, b.text, "{rel}");
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
