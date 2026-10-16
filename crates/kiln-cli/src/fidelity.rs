// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli fidelity`: runs the round-trip fidelity scorer of `kiln-testing` over the fixture
//! corpus and optionally writes the Markdown report (`cargo xtask fidelity` writes
//! `docs/qa/fidelity.md` through this command).
//!
//! - `--ci`: the committed corpus only (features and manual fixtures), and an empty adapter
//!   registry is accepted (exit 0 with "no formats registered").
//! - Otherwise the downloaded `external/` files are scored too when present, and an empty
//!   registry exits 1 so a placeholder is never mistaken for a result.
//!
//! Exit status 1 when a file misses a threshold of `parity/corpus.json` or a package does not
//! survive the passthrough baseline.

use std::path::PathBuf;

use kiln_testing::fidelity::{self, Options};
use kiln_testing::fixtures::{self, Corpus, FixtureFormat, FixtureKind};

use crate::CliError;

/// Usage text.
pub const USAGE: &str = "kiln-cli fidelity [--ci] [--format docx|xlsx|pptx]... [--report <file.md>]
    [--no-oracle] [--large]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    ci: bool,
    formats: Vec<FixtureFormat>,
    report: Option<PathBuf>,
    oracle: bool,
    large: bool,
}

fn parse(args: &[String]) -> Result<Args, CliError> {
    let mut a = Args {
        ci: false,
        formats: Vec::new(),
        report: None,
        oracle: true,
        large: false,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ci" => a.ci = true,
            "--no-oracle" => a.oracle = false,
            "--large" => a.large = true,
            "--format" => {
                let v = it
                    .next()
                    .ok_or_else(|| CliError::usage("--format needs a value", USAGE))?;
                for name in v.split(',') {
                    let f = FixtureFormat::parse(name).ok_or_else(|| {
                        CliError::usage(
                            format!("unknown format '{name}' (docx, xlsx, pptx)"),
                            USAGE,
                        )
                    })?;
                    if !a.formats.contains(&f) {
                        a.formats.push(f);
                    }
                }
            }
            "--report" => {
                a.report =
                    Some(PathBuf::from(it.next().ok_or_else(|| {
                        CliError::usage("--report needs a file", USAGE)
                    })?));
            }
            other => return Err(CliError::usage(format!("unknown option '{other}'"), USAGE)),
        }
    }
    Ok(a)
}

/// Runs the subcommand.
///
/// # Errors
///
/// Usage errors, an unreadable corpus manifest, failed files, and (outside `--ci`) an empty
/// registry.
pub fn run(args: &[String]) -> Result<String, CliError> {
    let a = parse(args)?;
    let corpus = Corpus::load_repo().map_err(CliError::Failed)?;
    let mut kinds = vec![FixtureKind::Features, FixtureKind::Manual];
    if !a.ci {
        kinds.push(FixtureKind::External);
    }
    if a.large {
        kinds.push(FixtureKind::Large);
    }
    let formats = if a.formats.is_empty() {
        FixtureFormat::ALL.to_vec()
    } else {
        a.formats.clone()
    };
    let files = fixtures::discover_in(&fixtures::fixtures_dir(), &formats, &kinds);
    let options = Options {
        formats: formats.clone(),
        use_oracle: a.oracle,
        ..Options::default()
    };
    let report = fidelity::run(
        &kiln_testing::formats::registered(),
        &files,
        &corpus,
        &options,
    );
    let _ = std::fs::remove_dir_all(&options.scratch);
    let mut text = report.to_text();
    if let Some(path) = &a.report {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir)
                .map_err(|e| CliError::Failed(format!("{}: {e}", dir.display())))?;
        }
        std::fs::write(path, report.to_markdown())
            .map_err(|e| CliError::Failed(format!("{}: {e}", path.display())))?;
        text.push_str(&format!("wrote {}\n", path.display()));
    }
    let text = text.trim_end().to_owned();
    if !report.passed() {
        return Err(CliError::Failed(text));
    }
    if report.no_formats_registered() && !a.ci {
        return Err(CliError::Failed(format!(
            "{text}\nno formats registered: nothing to score yet (pass --ci to accept)"
        )));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_arguments() {
        let a = parse(&[
            "--ci".into(),
            "--format".into(),
            "docx,XLSX".into(),
            "--format".into(),
            "docx".into(),
            "--report".into(),
            "r.md".into(),
            "--no-oracle".into(),
        ])
        .unwrap();
        assert!(a.ci && !a.oracle && !a.large);
        assert_eq!(a.formats, [FixtureFormat::Docx, FixtureFormat::Xlsx]);
        assert_eq!(a.report, Some(PathBuf::from("r.md")));
        assert!(parse(&["--format".into(), "odt".into()]).is_err());
        assert!(parse(&["--report".into()]).is_err());
        assert!(parse(&["--frob".into()]).is_err());
    }

    #[test]
    fn ci_mode_accepts_the_empty_registry_and_writes_the_report() {
        // A format whose adapter has not landed yet (none once every format is registered).
        let registered: Vec<FixtureFormat> = kiln_testing::formats::registered()
            .iter()
            .map(|a| a.format())
            .collect();
        let Some(format) = FixtureFormat::ALL
            .into_iter()
            .find(|f| !registered.contains(f))
        else {
            return;
        };
        let name = format.to_string();
        let path = std::env::temp_dir().join(format!("kiln-fidelity-{}.md", std::process::id()));
        let out = run(&[
            "--ci".into(),
            "--no-oracle".into(),
            "--format".into(),
            name.clone(),
            "--report".into(),
            path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        assert!(out.contains("no formats registered"), "{out}");
        assert!(out.contains("passthrough baseline"));
        let md = std::fs::read_to_string(&path).unwrap();
        assert!(md.contains("No formats registered"));
        assert!(md.contains(&format!("| `{name}/features` |")), "{md}");
        let _ = std::fs::remove_file(&path);
        // Interactively, an empty registry is not a result.
        let err = run(&["--no-oracle".into(), "--format".into(), name]).unwrap_err();
        assert!(err.to_string().contains("pass --ci"));
    }
}
