// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-cli
//!
//! The `kiln-cli` command line (installed as `kiln` by the Linux packages): the same core the
//! apps use, driven from a shell or an agent (PLAN.md §3.1 principle 2).
//!
//! ```text
//! kiln-cli inspect <file> [--xml <part>] [--json]     parts, content types, sizes, relationships
//! kiln-cli repack <in> <out>                          passthrough round trip, identical parts
//! kiln-cli render <file> --png out/page-%03d.png      pages to PNG [--page N] [--dpi 96] [--dark]
//! kiln-cli render --scratch <file.txt> --png <out>    one viewport of a scratch document
//! kiln-cli control                                    JSON control channel over stdio
//! kiln-cli eval '=formula'                            evaluate a formula
//! kiln-cli fidelity [--ci] [--format f]               round-trip fidelity of the fixture corpus
//! kiln-cli version                                    version, protocol and target
//! ```
//!
//! Exit status: 0 success, 1 failure, 2 usage error.
#![forbid(unsafe_code)]

mod control;
mod eval;
mod fidelity;
mod inspect;
mod pages;
mod render;
mod repack;

use std::fmt;
use std::process::ExitCode;

/// Why a command did not succeed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CliError {
    /// The arguments are wrong (exit status 2).
    Usage(String),
    /// The command ran and failed (exit status 1).
    Failed(String),
}

impl CliError {
    /// A usage error with the subcommand's usage appended.
    pub fn usage(message: impl Into<String>, usage: &str) -> Self {
        Self::Usage(format!("{}\n\nusage: {usage}", message.into()))
    }

    /// The exit status for this error.
    #[must_use]
    pub const fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            Self::Failed(_) => 1,
        }
    }
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(m) | Self::Failed(m) => f.write_str(m),
        }
    }
}

fn usage() -> String {
    format!(
        "{}\n\nUsage:\n  {}\n  {}\n  {}\n  {}\n  {}\n  {}\n  kiln-cli version\n  kiln-cli help\n\nExit status: 0 success, 1 failure, 2 usage error. Manual: docs/manual/cli.md",
        kiln_common::version_line(),
        inspect::USAGE,
        repack::USAGE,
        render::USAGE.replace('\n', "\n  "),
        control::USAGE,
        eval::USAGE,
        fidelity::USAGE.replace('\n', "\n  "),
    )
}

fn version() -> String {
    let info = kiln_session::Session::core_info();
    format!(
        "{}\nprotocol {}\ntarget {}",
        kiln_common::version_line(),
        info.protocol,
        info.target
    )
}

/// Runs a command (arguments without the program name).
fn run(args: &[String]) -> Result<String, CliError> {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        None => Err(CliError::Usage(format!(
            "kiln-cli: no command given\n\n{}",
            usage()
        ))),
        Some("--help" | "-h" | "help") => Ok(usage()),
        Some("--version" | "-V" | "version") => {
            if rest.is_empty() {
                Ok(version())
            } else {
                Err(CliError::usage(
                    "version takes no arguments",
                    "kiln-cli version",
                ))
            }
        }
        Some("inspect") => inspect::run(rest),
        Some("repack") => repack::run(rest),
        Some("render") => render::run(rest),
        Some("control") => control::run(rest),
        Some("eval") => eval::run(rest),
        Some("fidelity") => fidelity::run(rest),
        Some(other) => Err(CliError::Usage(format!(
            "kiln-cli: unknown command '{other}'\n\n{}",
            usage()
        ))),
    }
}

/// Writes the result and a newline to stdout, returning I/O errors (such as a closed pipe) rather
/// than panicking as `println!` does.
fn write_stdout(text: &str) -> std::io::Result<()> {
    use std::io::Write as _;
    let mut out = std::io::stdout().lock();
    out.write_all(text.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().cloned().unwrap_or_default();
    match run(&args) {
        Ok(text) => {
            if text.is_empty() {
                return ExitCode::SUCCESS;
            }
            match write_stdout(&text) {
                Ok(()) => ExitCode::SUCCESS,
                // The reader went away (`kiln-cli … | head`): nothing more to say, like `cat`.
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("kiln-cli {command}: writing stdout: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        Err(e) => {
            let message = e.to_string();
            if message.starts_with("kiln-cli") {
                eprintln!("{message}");
            } else {
                eprintln!("kiln-cli {command}: {message}");
            }
            ExitCode::from(e.exit_code())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CliError, run};

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn prints_the_version_and_help() {
        let v = run(&args(&["version"])).unwrap();
        assert!(v.starts_with("kiln 0.1.0\nprotocol "), "{v}");
        assert_eq!(run(&args(&["--version"])).unwrap(), v);
        let help = run(&args(&["help"])).unwrap();
        let none = run(&[]).unwrap_err();
        assert_eq!(none.exit_code(), 2);
        assert!(none.to_string().contains("no command given"));
        assert!(none.to_string().contains(&help));
        for cmd in ["inspect", "repack", "render", "control", "eval", "fidelity"] {
            assert!(help.contains(&format!("kiln-cli {cmd}")), "{cmd}");
        }
        assert_eq!(run(&args(&["--help"])).unwrap(), help);
        assert_eq!(run(&args(&["version", "x"])).unwrap_err().exit_code(), 2);
    }

    #[test]
    fn unknown_commands_are_usage_errors() {
        let e = run(&args(&["frobnicate"])).unwrap_err();
        assert!(matches!(e, CliError::Usage(_)));
        assert_eq!(e.exit_code(), 2);
        assert!(e.to_string().contains("unknown command 'frobnicate'"));
        assert_eq!(CliError::Failed("x".into()).exit_code(), 1);
    }
}
