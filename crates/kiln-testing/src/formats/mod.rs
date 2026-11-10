// SPDX-License-Identifier: AGPL-3.0-or-later
//! The registry of fidelity adapters: one [`Roundtrip`](crate::Roundtrip) implementation per
//! format reader/writer pair.
//!
//! The change that lands a format adds
//! its adapter as `formats/<format>.rs` (a `pub mod` line below) and one `Box::new(..)` line to
//! [`registered`], for example
//!
//! ```text
//! pub mod docx;                              // kiln-io-docx
//! ...
//!     Box::new(docx::Docx),
//! ```
//!
//! The adapter lives here rather than in the format crate so that `kiln-testing` depends on the
//! format crates and not the other way round.

use crate::fidelity::Roundtrip;

pub mod docx; // kiln-io-docx
pub mod odf; // kiln-io-odf
pub mod pptx; // kiln-io-pptx
pub mod xlsx; // kiln-io-xlsx

/// Every registered adapter, in registration order. Empty until the first format lands, in which
/// case `cargo xtask fidelity` reports "no formats registered" and measures only the OPC
/// passthrough baseline.
#[must_use]
pub fn registered() -> Vec<Box<dyn Roundtrip>> {
    vec![
        Box::new(docx::Docx),
        Box::new(xlsx::Xlsx),
        Box::new(pptx::Pptx),
        Box::new(odf::Odf),
    ]
}

#[cfg(test)]
mod tests {
    #[test]
    fn adapters_have_distinct_formats_and_names() {
        let all = super::registered();
        let mut keys: Vec<_> = all.iter().map(|a| (a.format(), a.name())).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), all.len());
    }
}
