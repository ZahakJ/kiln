// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-raster
//!
//! A CPU painter on `tiny-skia` that turns a Scene into an RGBA bitmap, used by `kiln render`,
//! tests, thumbnails and as a native fallback.
//!
//! **Status:** scaffold created by WP-50. The crate builds and is linted and tested in CI, but
//! it holds no engine logic yet; WP-54 (PLAN.md §11) implements it.
//!
//! `#![forbid(unsafe_code)]` is relaxed to `deny` here (PLAN.md §3.2): handing pixel buffers to
//! callers without a copy may need a narrowly scoped `#[allow(unsafe_code)]` block, which must
//! carry a `// SAFETY:` comment. The scaffold contains no `unsafe` at all.
#![deny(unsafe_code)]
#![warn(missing_docs)]

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
