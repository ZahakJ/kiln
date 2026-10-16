// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-cli
//!
//! The `kiln-cli` command line (installed as `kiln` by the Linux packages): convert, render,
//! inspect, fidelity, benchmarks, the function list and the JSON control channel over stdio.
//!
//! **Status:** scaffold created by WP-50. The crate builds and is linted and tested in CI, but
//! it holds no engine logic yet; WP-60 (PLAN.md §11) implements it.
#![forbid(unsafe_code)]

use std::process::ExitCode;

fn usage() -> String {
    format!(
        "{}\n\nUsage: kiln-cli [--version | --help]\n\nThe convert, render, inspect, fidelity, functions and control\nsubcommands arrive with WP-60 (PLAN.md §11).",
        kiln_common::version_line()
    )
}

/// Decides what to print and the exit status for the given arguments (without the program name).
fn run(args: &[String]) -> (String, bool) {
    match args.first().map(String::as_str) {
        None | Some("--version" | "-V" | "version") => (kiln_common::version_line(), true),
        Some("--help" | "-h" | "help") => (usage(), true),
        Some(other) => (
            format!("kiln-cli: unknown command '{other}'\n\n{}", usage()),
            false,
        ),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (text, ok) = run(&args);
    if ok {
        println!("{text}");
        ExitCode::SUCCESS
    } else {
        eprintln!("{text}");
        ExitCode::from(2)
    }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn prints_the_version_line() {
        assert_eq!(run(&[]), ("kiln 0.1.0".to_owned(), true));
        assert_eq!(run(&["--version".to_owned()]).0, "kiln 0.1.0");
    }

    #[test]
    fn rejects_unknown_commands() {
        let (text, ok) = run(&["frobnicate".to_owned()]);
        assert!(!ok);
        assert!(text.contains("unknown command 'frobnicate'"));
    }
}
