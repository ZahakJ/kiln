// SPDX-License-Identifier: AGPL-3.0-or-later
//! Small helpers shared by the xtask commands.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// The repository root (the parent of `xtask/`), resolved at run time.
///
/// The compile-time `CARGO_MANIFEST_DIR` is only the last resort: a binary cached in a shared
/// target directory may have been built from another checkout (a worktree since deleted), and
/// cargo does not rebuild it just because it is run from here.
pub fn repo_root() -> PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(find_repo_root).clone()
}

fn find_repo_root() -> PathBuf {
    // `cargo run` (the `cargo xtask` alias) sets this for the checkout it was run in.
    let at_run_time = std::env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.join("Cargo.toml").is_file())
        .and_then(|dir| dir.parent().map(Path::to_path_buf));
    at_run_time
        .or_else(workspace_of_current_dir)
        .unwrap_or_else(|| {
            let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            manifest.parent().map_or(manifest.clone(), PathBuf::from)
        })
}

/// The workspace containing the current directory (`cargo locate-project --workspace`).
fn workspace_of_current_dir() -> Option<PathBuf> {
    let out = Command::new(cargo())
        .args(["locate-project", "--workspace", "--message-format", "plain"])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    let manifest = String::from_utf8(out.stdout).ok()?;
    Path::new(manifest.trim()).parent().map(Path::to_path_buf)
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
