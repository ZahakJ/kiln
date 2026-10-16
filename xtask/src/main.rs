// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask` — repository automation for Kiln (PLAN.md §3.2, §4.7).
//!
//! ```text
//! cargo xtask verify [--workspace | -p <crate>…]   fmt --check, clippy -D warnings, nextest, protocol --check
//! cargo xtask protocol [--check]                    ts-rs export of kiln-protocol (WP-52)
//! cargo xtask wasm [--release] [--size]             wasm32 build + wasm-bindgen + wasm-opt (WP-52)
//! cargo xtask fidelity [--ci]                       round-trip fidelity over fixtures/ (WP-60)
//! cargo xtask budgets [--ci]                        performance budgets of §3.9 (WP-60)
//! cargo xtask parity [--check]                      parity/parity.json → PARITY.md (WP-59)
//! cargo xtask notices [--check]                     THIRD-PARTY-NOTICES.md from Cargo and pnpm metadata
//! ```
#![forbid(unsafe_code)]

mod notices;
mod pending;
mod protocol;
mod util;
mod verify;
mod wasm;

use std::process::ExitCode;

const USAGE: &str =
    "usage: cargo xtask <verify|protocol|wasm|fidelity|budgets|parity|notices> [options]

  verify [--workspace | -p <crate>...]  fmt --check, clippy -D warnings, nextest, protocol --check
  protocol [--check]                    generate the TypeScript protocol (WP-52)
  wasm [--release] [--size]             build the web core (WP-52)
  fidelity [--ci]                       round-trip fidelity report (WP-60)
  budgets [--ci]                        performance budgets (WP-60)
  parity [--check]                      regenerate PARITY.md (WP-59)
  notices [--check]                     regenerate THIRD-PARTY-NOTICES.md";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let Some(cmd) = args.next() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let rest: Vec<String> = args.collect();
    let result = match cmd.as_str() {
        "verify" => verify::run(&rest),
        "notices" => notices::run(&rest),
        "protocol" => protocol::run(&rest),
        "wasm" => wasm::run(&rest),
        "-h" | "--help" | "help" => {
            println!("{USAGE}");
            Ok(())
        }
        other => match pending::lookup(other) {
            Some(task) => return pending::run(task, &rest),
            None => Err(format!("unknown xtask '{other}'\n\n{USAGE}")),
        },
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask {cmd}: {message}");
            ExitCode::FAILURE
        }
    }
}
