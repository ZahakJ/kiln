// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small helpers shared by the xtask commands.

use std::path::PathBuf;
use std::process::Command;

/// The repository root (the parent of `xtask/`).
pub fn repo_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.parent().map_or(manifest.clone(), PathBuf::from)
}

/// The `cargo` executable that launched us (so the pinned toolchain is reused).
pub fn cargo() -> String {
    std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned())
}

/// Runs a command from the repository root, echoing it first; fails with a readable message.
pub fn run(program: &str, args: &[String]) -> Result<(), String> {
    println!("$ {} {}", display_program(program), args.join(" "));
    let status = Command::new(program)
        .args(args)
        .current_dir(repo_root())
        .status()
        .map_err(|e| format!("could not start {program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "`{} {}` failed ({status})",
            display_program(program),
            args.join(" ")
        ))
    }
}

fn display_program(program: &str) -> &str {
    if program.ends_with("cargo") {
        "cargo"
    } else {
        program
    }
}

/// Converts string literals to owned arguments.
pub fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}
