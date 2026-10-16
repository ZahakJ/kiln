// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-bridge-wasm
//!
//! The WebAssembly bridge: `wasm-bindgen` exports of `kiln-session` for the web build, run inside a
//! Web Worker.
//!
//! **Status:** scaffold created by WP-50. The crate builds and is linted and tested in CI, but
//! it holds no engine logic yet; WP-52 (PLAN.md §11) implements it.
//!
//! `#![forbid(unsafe_code)]` is deliberately absent: `#[wasm_bindgen]` expands to `unsafe` glue
//! (ABI conversions) in this crate. Hand-written `unsafe` is still not allowed here; every
//! exception must carry a `// SAFETY:` comment and be approved in review.
#![warn(missing_docs)]

use wasm_bindgen::prelude::wasm_bindgen;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the suite version, so the web host can check that it loaded the expected core.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    VERSION.to_owned()
}

#[cfg(test)]
mod tests {
    #[test]
    fn exported_version_is_the_suite_version() {
        assert_eq!(super::version(), kiln_common::VERSION);
    }
}
