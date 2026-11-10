// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask` — repository automation for Kiln.
//!
//! ```text
//! cargo xtask verify [--workspace | -p <crate>…]   fmt --check, clippy -D warnings, nextest, protocol --check
//! cargo xtask protocol [--check]                    ts-rs export of kiln-protocol
//! cargo xtask wasm [--release] [--size] [--app <a>] per-app wasm32 builds + wasm-bindgen + wasm-opt
//! cargo xtask fidelity [--ci] [--format f]          round-trip fidelity over fixtures/
//! cargo xtask budgets [--ci]                        performance budgets
//! cargo xtask parity [--check | --verify | --snapshot <tag>]  parity/parity.json → PARITY.md
//! cargo xtask notices [--check]                     THIRD-PARTY-NOTICES.md from Cargo and pnpm metadata
//! cargo xtask release                               the user-facing surfaces name only the apps of the release
//! cargo xtask site [--check]                        the landing page's numbers and the function list
//! ```
#![forbid(unsafe_code)]

mod budgets;
mod fidelity;
mod notices;
mod parity;
mod pending;
mod protocol;
mod release;
mod site;
mod util;
mod verify;
mod wasm;

use std::process::ExitCode;

const USAGE: &str =
    "usage: cargo xtask <verify|protocol|wasm|fidelity|budgets|parity|notices|release|site> [options]

  verify [--workspace | -p <crate>...]  fmt --check, clippy -D warnings, nextest, protocol --check
  protocol [--check]                    generate the TypeScript protocol
  wasm [--release] [--size] [--app <a>] build the web cores, one per app
  fidelity [--ci] [--format f]...       round-trip fidelity report → docs/qa/fidelity.md
  budgets [--ci]                        performance budgets → docs/qa/perf.md
  parity [--check | --verify [--dry-run] [--app <app>] [--row <id>] | --snapshot <tag>]
                                        validate the parity tracker, regenerate PARITY.md
  notices [--check]                     regenerate THIRD-PARTY-NOTICES.md
  release                               check the manual, README, landing page and interface
                                        messages name only the apps of the release
  site [--check]                        the landing page's numbers, docs/data/*.json and the
                                        manual's function list";

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
        "fidelity" => fidelity::run(&rest),
        "budgets" => budgets::run(&rest),
        "parity" => parity::run(&rest),
        "protocol" => protocol::run(&rest),
        "wasm" => wasm::run(&rest),
        "release" => release::run(&rest),
        "site" => site::run(&rest),
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
