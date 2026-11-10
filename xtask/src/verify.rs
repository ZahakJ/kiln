// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask verify`: the gate every change runs before committing.
//!
//! Runs `cargo fmt --all --check`, `cargo clippy … --all-targets -- -D warnings`,
//! `cargo nextest run …`, `cargo test --doc` for the crates whose doc tests guard something
//! ([`DOC_TEST_CRATES`]: nextest runs no doc tests), `cargo xtask release` and, once the exporter exists,
//! `cargo xtask protocol --check`.
//! Scope: `--workspace` (default; the Tauri shell `kiln-desktop` is excluded because it needs the
//! WebKitGTK system libraries, CI checks it separately) or `-p <crate>` repeated.

use crate::{pending, util};

/// The crates whose doc tests are a guard (`compile_fail` tests that a bypass does not
/// compile), run with `cargo test --doc`; `every_compile_fail_crate_is_listed` keeps it whole.
pub const DOC_TEST_CRATES: &[&str] = &["kiln-session"];

/// The file of the planted bypasses of Kiln Grid's ledger (feature `ownership-plants`): each line
/// marked `// PLANT B<n>` must fail to compile. Doc tests are another crate, so they cannot show
/// what this crate's own modules can reach; this builds the crate itself with the plants in.
pub const OWNERSHIP_PLANTS: &str = "crates/kiln-session/src/apps/grid/structure.rs";

/// The planted lines of [`OWNERSHIP_PLANTS`]: (line, plant).
pub fn plant_lines(source: &str) -> Vec<(usize, String)> {
    source
        .lines()
        .enumerate()
        .filter_map(|(i, l)| {
            let at = l.find("// PLANT ")?;
            Some((i + 1, l[at + 9..].trim().to_owned()))
        })
        .collect()
}

/// The planted lines no error was reported on, from `cargo check --message-format short` output.
pub fn plants_that_compiled(plants: &[(usize, String)], output: &str) -> Vec<String> {
    let file = OWNERSHIP_PLANTS.trim_start_matches("crates/");
    plants
        .iter()
        .filter(|(line, _)| {
            !output.lines().any(|o| {
                o.contains(": error")
                    && o.split(':')
                        .nth(1)
                        .and_then(|n| n.trim().parse::<usize>().ok())
                        == Some(*line)
                    && o.contains(file)
            })
        })
        .map(|(line, name)| format!("{name} (line {line})"))
        .collect()
}

/// Builds `kiln-session` with the planted bypasses: it must fail, at every planted line.
fn check_ownership_plants(cargo: &str) -> Result<(), String> {
    let root = util::repo_root();
    let source = std::fs::read_to_string(root.join(OWNERSHIP_PLANTS))
        .map_err(|e| format!("{OWNERSHIP_PLANTS}: {e}"))?;
    let plants = plant_lines(&source);
    if plants.is_empty() {
        return Err(format!("{OWNERSHIP_PLANTS} has no planted lines"));
    }
    let args = util::args(&[
        "check",
        "-p",
        "kiln-session",
        "--lib",
        "--features",
        "ownership-plants",
        "--message-format",
        "short",
    ]);
    println!("$ cargo {} (each planted bypass must fail)", args.join(" "));
    let out = std::process::Command::new(cargo)
        .args(&args)
        .current_dir(&root)
        .output()
        .map_err(|e| format!("could not start cargo: {e}"))?;
    let text = String::from_utf8_lossy(&out.stderr);
    for l in text.lines().filter(|l| l.contains(": error")) {
        println!("  {l}");
    }
    if out.status.success() {
        return Err("the planted bypasses of the Grid ledger compiled".to_owned());
    }
    let open = plants_that_compiled(&plants, &text);
    if !open.is_empty() {
        return Err(format!(
            "planted bypasses of the Grid ledger with no error: {}",
            open.join(", ")
        ));
    }
    println!(
        "ownership plants: {} planted bypasses, each fails to compile",
        plants.len()
    );
    Ok(())
}

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

    /// Whether the scope takes in `krate`.
    pub fn includes(&self, krate: &str) -> bool {
        match self {
            Scope::Workspace => krate != "kiln-desktop",
            Scope::Packages(list) => list.iter().any(|p| p == krate),
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

    for krate in DOC_TEST_CRATES {
        if scope.includes(krate) {
            util::run(&cargo, &util::args(&["test", "--doc", "-p", krate]))?;
        }
    }
    if scope.includes("kiln-session") {
        check_ownership_plants(&cargo)?;
    }

    // the manual, README, landing page and messages name only the apps of the release.
    crate::release::run(&[])?;

    if pending::protocol_implemented() {
        util::run(&cargo, &util::args(&["xtask", "protocol", "--check"]))?;
    } else {
        println!("xtask protocol --check: skipped: not implemented yet");
    }
    println!("verify: all checks passed");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_planted_line_without_an_error_is_named() {
        let source = "a\nlet x = y; // PLANT B3\nb\nlet z = w; // PLANT B4\n";
        let plants = plant_lines(source);
        assert_eq!(plants, vec![(2, "B3".to_owned()), (4, "B4".to_owned())]);
        let output = "crates/kiln-session/src/apps/grid/structure.rs:2:9: error[E0624]: private";
        assert_eq!(
            plants_that_compiled(&plants, output),
            vec!["B4 (line 4)".to_owned()]
        );
    }

    /// Every crate with a `compile_fail` doc test is in [`DOC_TEST_CRATES`].
    #[test]
    fn every_compile_fail_crate_is_listed() {
        fn has(dir: &std::path::Path) -> bool {
            std::fs::read_dir(dir)
                .into_iter()
                .flatten()
                .flatten()
                .any(|e| {
                    let p = e.path();
                    if p.is_dir() {
                        has(&p)
                    } else {
                        p.extension().is_some_and(|x| x == "rs")
                            && std::fs::read_to_string(&p)
                                .is_ok_and(|t| t.contains("```compile_fail"))
                    }
                })
        }
        let crates = util::repo_root().join("crates");
        for e in std::fs::read_dir(&crates).unwrap().flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if has(&e.path().join("src")) {
                assert!(DOC_TEST_CRATES.contains(&name.as_str()), "{name}");
            }
        }
    }

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
