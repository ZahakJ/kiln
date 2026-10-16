// SPDX-License-Identifier: AGPL-3.0-or-later
//! Commands whose implementation belongs to a later work package.
//!
//! In CI mode (`--ci` or `--check`) they succeed with an explicit "skipped" line so the pipeline can
//! already call them; run interactively they fail, so nobody mistakes a placeholder for a result.
//! The owning work package replaces the entry with a real module (`xtask/src/<name>.rs`).

use std::process::ExitCode;

/// A not-yet-implemented xtask and the work package that implements it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    /// Subcommand name.
    pub name: &'static str,
    /// Work package that implements it (PLAN.md §11).
    pub owner: &'static str,
}

const PENDING: &[Pending] = &[
    Pending {
        name: "protocol",
        owner: "WP-52",
    },
    Pending {
        name: "wasm",
        owner: "WP-52",
    },
    Pending {
        name: "fidelity",
        owner: "WP-60",
    },
    Pending {
        name: "budgets",
        owner: "WP-60",
    },
    Pending {
        name: "parity",
        owner: "WP-59",
    },
];

/// Finds a pending command by name.
pub fn lookup(name: &str) -> Option<Pending> {
    PENDING.iter().copied().find(|p| p.name == name)
}

/// True when the protocol exporter exists (used by `verify`).
pub fn protocol_implemented() -> bool {
    lookup("protocol").is_none()
}

/// The line printed for a pending command.
pub fn message(task: Pending) -> String {
    format!("skipped: not implemented yet ({})", task.owner)
}

/// Whether the arguments request CI/check mode.
pub fn ci_mode(args: &[String]) -> bool {
    args.iter().any(|a| a == "--ci" || a == "--check")
}

/// Runs a pending command: exit 0 with a skipped line in CI mode, exit 1 otherwise.
pub fn run(task: Pending, args: &[String]) -> ExitCode {
    if ci_mode(args) {
        println!("xtask {}: {}", task.name, message(task));
        ExitCode::SUCCESS
    } else {
        eprintln!(
            "xtask {}: {} — pass --ci or --check to treat this as a skip",
            task.name,
            message(task)
        );
        ExitCode::FAILURE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_pending_command_names_its_owner() {
        for name in ["protocol", "wasm", "fidelity", "budgets", "parity"] {
            let task = lookup(name).expect("pending command");
            assert!(task.owner.starts_with("WP-"));
            assert_eq!(
                message(task),
                format!("skipped: not implemented yet ({})", task.owner)
            );
        }
        assert!(lookup("verify").is_none());
        assert!(!protocol_implemented());
    }

    #[test]
    fn ci_mode_needs_an_explicit_flag() {
        assert!(ci_mode(&["--ci".to_owned()]));
        assert!(ci_mode(&["--release".to_owned(), "--check".to_owned()]));
        assert!(!ci_mode(&[]));
        assert!(!ci_mode(&["--release".to_owned()]));
    }
}
