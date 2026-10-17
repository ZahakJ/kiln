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
    let mut kinds = vec![
        FixtureKind::Features,
        FixtureKind::Manual,
        FixtureKind::Charts,
    ];
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
        let mut md = report.to_markdown();
        // `--format x` rewrites only the rows of x: the other formats' rows stay as they were.
        if !a.formats.is_empty()
            && a.formats.len() < FixtureFormat::ALL.len()
            && let Ok(old) = std::fs::read_to_string(path)
        {
            md = merge_report(&old, &md, &a.formats);
        }
        std::fs::write(path, md)
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

/// The format a table row of the report belongs to: its first cell is a format (`| xlsx |`), a
/// fixture path (`` | `xlsx/features/a.xlsx` | ``, `` | `large/sheet-1m.xlsx` | ``) or a fixture
/// directory (`` | `external/xlsx` | ``).
fn row_format(line: &str) -> Option<FixtureFormat> {
    let first = line
        .trim()
        .strip_prefix('|')?
        .split('|')
        .next()?
        .trim()
        .trim_matches('`');
    if let Some(f) = FixtureFormat::parse(first) {
        return Some(f);
    }
    if let Some(f) = std::path::Path::new(first)
        .extension()
        .and_then(|e| e.to_str())
        .and_then(FixtureFormat::parse)
    {
        return Some(f);
    }
    first.split('/').find_map(FixtureFormat::parse)
}

/// The report written for some formats merged into the previous report: every table keeps the
/// previous rows of the other formats and takes the new rows of `formats` (in the order of
/// [`FixtureFormat::ALL`]); the header lists every format and the result is the merged one.
#[must_use]
pub fn merge_report(old: &str, new: &str, formats: &[FixtureFormat]) -> String {
    /// The rows of each table, by the heading the table is under.
    fn tables(md: &str) -> Vec<(String, Vec<String>)> {
        let mut out: Vec<(String, Vec<String>)> = Vec::new();
        let mut heading = String::new();
        for line in md.lines() {
            if line.starts_with("## ") {
                heading = line.to_owned();
            } else if line.starts_with('|')
                && !line.starts_with("|---")
                && row_format(line).is_some()
            {
                match out.iter_mut().find(|(h, _)| *h == heading) {
                    Some((_, rows)) => rows.push(line.to_owned()),
                    None => out.push((heading.clone(), vec![line.to_owned()])),
                }
            }
        }
        out
    }
    let old_tables = tables(old);
    let new_tables = tables(new);
    let rank = |l: &str| {
        row_format(l)
            .and_then(|f| FixtureFormat::ALL.iter().position(|x| *x == f))
            .unwrap_or(usize::MAX)
    };
    let mut kept_formats: Vec<FixtureFormat> = Vec::new();
    let mut out = String::new();
    let mut heading = String::new();
    let mut emitted = std::collections::HashSet::new();
    let mut failed = false;
    for line in new.lines() {
        if line.starts_with("## ") {
            heading = line.to_owned();
        }
        let is_row =
            line.starts_with('|') && !line.starts_with("|---") && row_format(line).is_some();
        if !is_row {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if !emitted.insert(heading.clone()) {
            continue;
        }
        let mut rows: Vec<String> = old_tables
            .iter()
            .find(|(h, _)| *h == heading)
            .map(|(_, r)| {
                r.iter()
                    .filter(|l| row_format(l).is_some_and(|f| !formats.contains(&f)))
                    .cloned()
                    .collect()
            })
            .unwrap_or_default();
        rows.extend(
            new_tables
                .iter()
                .find(|(h, _)| *h == heading)
                .map(|(_, r)| r.clone())
                .unwrap_or_default(),
        );
        rows.sort_by_key(|l| rank(l));
        for r in rows {
            if let Some(f) = row_format(&r)
                && !kept_formats.contains(&f)
            {
                kept_formats.push(f);
            }
            if r.contains("| FAIL |") || r.contains("| fail |") {
                failed = true;
            }
            out.push_str(&r);
            out.push('\n');
        }
    }
    kept_formats.sort_by_key(|f| FixtureFormat::ALL.iter().position(|x| x == f));
    let names: Vec<String> = kept_formats.iter().map(ToString::to_string).collect();
    out.lines()
        .map(|l| {
            if l.starts_with("- Formats: ") {
                format!("- Formats: {}", names.join(", "))
            } else if l.starts_with("- Result: ") {
                format!("- Result: **{}**", if failed { "FAIL" } else { "pass" })
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
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

    #[test]
    fn a_format_run_rewrites_only_its_own_rows() {
        let old = "# R\n\n- Formats: docx, xlsx, pptx\n- Result: **pass**\n\n## Summary\n\n| Format | A |\n|---|---|\n| docx | 1 |\n| xlsx | 0.5 |\n| pptx | 1 |\n\n## Files\n\n| File | Result |\n|---|---|\n| `docx/features/a.docx` | pass |\n| `xlsx/features/b.xlsx` | FAIL |\n| `external/pptx/c.pptx` | pass |\n";
        let new = "# R\n\n- Formats: xlsx\n- Result: **pass**\n\n## Summary\n\n| Format | A |\n|---|---|\n| xlsx | 1 |\n\n## Files\n\n| File | Result |\n|---|---|\n| `xlsx/charts/bar.xlsx` | pass |\n| `xlsx/features/b.xlsx` | pass |\n";
        let m = merge_report(old, new, &[FixtureFormat::Xlsx]);
        assert!(m.contains("- Formats: docx, xlsx, pptx"));
        assert!(m.contains("- Result: **pass**"));
        assert!(m.contains("| docx | 1 |\n| xlsx | 1 |\n| pptx | 1 |"));
        assert!(m.contains("| `docx/features/a.docx` | pass |\n| `xlsx/charts/bar.xlsx` | pass |\n| `xlsx/features/b.xlsx` | pass |\n| `external/pptx/c.pptx` | pass |"));
        assert!(!m.contains("FAIL"));
        assert_eq!(
            row_format("| `large/sheet-1m.xlsx` | pass |"),
            Some(FixtureFormat::Xlsx)
        );
        assert_eq!(
            row_format("| `external/docx` | 10 |"),
            Some(FixtureFormat::Docx)
        );
        assert_eq!(row_format("| File | Result |"), None);
    }
}
