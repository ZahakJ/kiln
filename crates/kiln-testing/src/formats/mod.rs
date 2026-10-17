// SPDX-License-Identifier: AGPL-3.0-or-later
//! The registry of fidelity adapters: one [`Roundtrip`](crate::Roundtrip) implementation per
//! format reader/writer pair.
//!
//! This file is append-only and shared (PLAN.md §4.7): the work package that lands a format adds
//! its adapter as `formats/<format>.rs` (a `pub mod` line below) and one `Box::new(..)` line to
//! [`registered`], for example
//!
//! ```text
//! pub mod docx;                              // WP-62/WP-65: kiln-io-docx
//! ...
//!     Box::new(docx::Docx),                  // WP-62/WP-65
//! ```
//!
//! The adapter lives here rather than in the format crate so that `kiln-testing` depends on the
//! format crates and not the other way round.

use crate::fidelity::Roundtrip;

pub mod docx; // WP-62/WP-65: kiln-io-docx
pub mod pptx;
pub mod xlsx; // WP-70: kiln-io-xlsx // WP-77: kiln-io-pptx read half; WP-79 adds the writer and the registration below

/// Every registered adapter, in registration order. Empty until the first format lands, in which
/// case `cargo xtask fidelity` reports "no formats registered" and measures only the OPC
/// passthrough baseline.
#[must_use]
pub fn registered() -> Vec<Box<dyn Roundtrip>> {
    vec![
        Box::new(docx::Docx), // WP-62/WP-65
        Box::new(xlsx::Xlsx), // WP-70
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
