// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-collab-server
//!
//! The self-hosted collaboration relay: an `axum` and `tokio` WebSocket server with `redb`
//! persistence and shared-token authentication.
//!
//! **Status:** scaffold created by WP-50. The crate builds and is linted and tested in CI, but
//! it holds no engine logic yet; WP-82 (PLAN.md §11) implements it.
#![forbid(unsafe_code)]

use std::process::ExitCode;

/// The version of this binary (the suite version).
const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        println!("kiln-collab-server {VERSION}");
        return ExitCode::SUCCESS;
    }
    eprintln!("kiln-collab-server {VERSION}: the relay is not implemented yet (WP-82)");
    ExitCode::FAILURE
}

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
