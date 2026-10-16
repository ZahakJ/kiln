// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask verify`: the gate every work package runs before committing (PLAN.md §4.7).
//!
//! Runs `cargo fmt --all --check`, `cargo clippy … --all-targets -- -D warnings`,
//! `cargo nextest run …` and, once the exporter exists, `cargo xtask protocol --check`.
//! Scope: `--workspace` (default; the Tauri shell `kiln-desktop` is excluded because it needs the
//! WebKitGTK system libraries, CI checks it separately) or `-p <crate>` repeated.

use crate::{pending, util};

/// The packages to lint and test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Every workspace member except `kiln-desktop`.
    Workspace,
    /// The listed packages.
    Packages(Vec<String>),
}

impl Scope {
    /// Cargo arguments selecting the scope.
    pub fn cargo_args(&self) -> Vec<String> {
        match self {
            Scope::Workspace => util::args(&["--workspace", "--exclude", "kiln-desktop"]),
            Scope::Packages(list) => list
                .iter()
                .flat_map(|p| ["-p".to_owned(), p.clone()])
                .collect(),
        }
    }
}

/// Parses `verify` options.
pub fn parse(args: &[String]) -> Result<Scope, String> {
    let mut packages = Vec::new();
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--workspace" | "--ci" => {}
            "-p" | "--package" => {
                let name = iter.next().ok_or("-p needs a crate name")?;
                packages.push(name.clone());
            }
            other => {
                if let Some(name) = other.strip_prefix("--package=") {
                    packages.push(name.to_owned());
                } else {
                    return Err(format!("unknown option '{other}'"));
                }
            }
        }
    }
    Ok(if packages.is_empty() {
        Scope::Workspace
    } else {
        Scope::Packages(packages)
    })
}

/// Runs the verification steps in order and stops at the first failure.
pub fn run(args: &[String]) -> Result<(), String> {
    let scope = parse(args)?;
    let cargo = util::cargo();
    util::run(&cargo, &util::args(&["fmt", "--all", "--check"]))?;

    let mut clippy = util::args(&["clippy"]);
    clippy.extend(scope.cargo_args());
    clippy.extend(util::args(&["--all-targets", "--", "-D", "warnings"]));
    util::run(&cargo, &clippy)?;

    let mut nextest = util::args(&["nextest", "run", "--no-tests=warn"]);
    nextest.extend(scope.cargo_args());
    util::run(&cargo, &nextest)?;

    if pending::protocol_implemented() {
        util::run(&cargo, &util::args(&["xtask", "protocol", "--check"]))?;
    } else {
        println!("xtask protocol --check: skipped: not implemented yet (WP-52)");
    }
    println!("verify: all checks passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_the_workspace_without_the_desktop_shell() {
        assert_eq!(parse(&[]), Ok(Scope::Workspace));
        assert_eq!(
            Scope::Workspace.cargo_args(),
            vec!["--workspace", "--exclude", "kiln-desktop"]
        );
    }

    #[test]
    fn accepts_a_package_list() {
        let args = util::args(&["-p", "kiln-text", "--package=kiln-scene"]);
        let scope = parse(&args).expect("valid");
        assert_eq!(
            scope,
            Scope::Packages(vec!["kiln-text".into(), "kiln-scene".into()])
        );
        assert_eq!(
            scope.cargo_args(),
            vec!["-p", "kiln-text", "-p", "kiln-scene"]
        );
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(parse(&util::args(&["--frobnicate"])).is_err());
        assert!(parse(&util::args(&["-p"])).is_err());
    }
}
