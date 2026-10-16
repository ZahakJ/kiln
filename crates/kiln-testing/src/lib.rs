// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-testing
//!
//! Test support for the Kiln core (PLAN.md §5), shared by the format crates' tests, `kiln-cli`,
//! `kiln-bench` and `cargo xtask fidelity`:
//!
//! - [`fixtures`]: discovery of the fixture corpus under `fixtures/` (features, manual, large and
//!   the downloaded external files) and the corpus manifest `parity/corpus.json`.
//! - [`oracle`]: LibreOffice headless through `tools/oracle-soffice.sh` (recalculating profile,
//!   `--convert-to pdf|csv|…`) and poppler's `pdftoppm` for page images.
//! - [`image`]: PNG loading and saving, perceptual pixel diffs and the page-by-page visual score.
//! - [`fidelity`]: the round-trip fidelity scorer (`structural`, `text`, `visual`, `values`,
//!   `formulas`, `oracle_opens`, `byte_identical_untouched`) over the [`Roundtrip`] adapters that
//!   format crates register in [`formats`], plus the OPC passthrough baseline every package gets.
//! - [`conformance`]: the formula conformance CSV runner (`conformance/*.csv`) as a library that
//!   any evaluator can be plugged into.
//!
//! Nothing here is shipped: the crate is a dependency of tests and developer tools only.
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod conformance;
pub mod fidelity;
pub mod fixtures;
pub mod formats;
pub mod image;
pub mod oracle;

pub use fidelity::{Metric, Metrics, Roundtrip, RoundtripOutput, Snapshot};
pub use fixtures::{Fixture, FixtureFormat, FixtureKind};

use std::path::{Path, PathBuf};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The repository root.
///
/// `KILN_REPO_ROOT` wins when set; otherwise the nearest ancestor of the current directory that
/// holds both `Cargo.toml` and `fixtures/`, and finally the checkout this crate was compiled from.
#[must_use]
pub fn repo_root() -> PathBuf {
    if let Some(root) = std::env::var_os("KILN_REPO_ROOT") {
        return PathBuf::from(root);
    }
    let looks_like_root = |p: &Path| p.join("Cargo.toml").is_file() && p.join("fixtures").is_dir();
    if let Ok(cwd) = std::env::current_dir()
        && let Some(found) = cwd.ancestors().find(|p| looks_like_root(p))
    {
        return found.to_path_buf();
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }

    #[test]
    fn finds_the_repository_root() {
        let root = super::repo_root();
        assert!(root.join("fixtures").is_dir(), "{}", root.display());
        assert!(root.join("parity/corpus.json").is_file());
    }
}
