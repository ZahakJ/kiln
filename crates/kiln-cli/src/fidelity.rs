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

use kiln_testing::differential::{self, Tool};
use kiln_testing::fidelity::{self, Isolation, ModelScore, Options, stamp};
use kiln_testing::fixtures::{self, Corpus, FixtureFormat, FixtureKind};

use crate::CliError;

/// Usage text.
pub const USAGE: &str =
    "kiln-cli fidelity [--ci] [--format docx|xlsx|pptx|odf]... [--report <file.md>]
    [--no-oracle] [--large] [--jobs <n>] [--only <text,…>] [--differential]";

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    ci: bool,
    formats: Vec<FixtureFormat>,
    report: Option<PathBuf>,
    oracle: bool,
    large: bool,
    jobs: usize,
    differential: bool,
    only: Vec<String>,
}

fn parse(args: &[String]) -> Result<Args, CliError> {
    let mut a = Args {
        ci: false,
        formats: Vec::new(),
        report: None,
        oracle: true,
        large: false,
        jobs: 4,
        differential: false,
        only: Vec::new(),
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--ci" => a.ci = true,
            "--no-oracle" => a.oracle = false,
            "--large" => a.large = true,
            "--differential" => a.differential = true,
            "--jobs" => {
                a.jobs = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| CliError::usage("--jobs needs a number", USAGE))?;
            }
            "--only" => {
                let v = it
                    .next()
                    .ok_or_else(|| CliError::usage("--only needs a list", USAGE))?;
                a.only.extend(v.split(',').map(str::to_owned));
            }
            "--format" => {
                let v = it
                    .next()
                    .ok_or_else(|| CliError::usage("--format needs a value", USAGE))?;
                for name in v.split(',') {
                    let f = FixtureFormat::parse(name).ok_or_else(|| {
                        CliError::usage(
                            format!("unknown format '{name}' (docx, xlsx, pptx, odf)"),
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
    if let [mode, format, input, output] = args
        && (mode == "--worker" || mode == "--worker-compare")
    {
        return worker(mode == "--worker-compare", format, input, output);
    }
    if let [flag, path] = args
        && flag == "--check-stamp"
    {
        return check_stamp(path);
    }
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
    let mut files = fixtures::discover_in(&fixtures::fixtures_dir(), &formats, &kinds);
    if !a.only.is_empty() {
        files.retain(|f| a.only.iter().any(|o| f.rel.contains(o.as_str())));
    }
    let exe = std::env::current_exe().map_err(|e| CliError::Failed(e.to_string()))?;
    let options = Options {
        formats: formats.clone(),
        use_oracle: a.oracle,
        scratch: kiln_testing::repo_root()
            .join("target/fidelity")
            .join(std::process::id().to_string()),
        isolation: Some(Isolation {
            exe,
            args: vec!["fidelity".to_owned()],
            timeout: std::time::Duration::from_secs(env_number("KILN_FIDELITY_FILE_TIMEOUT", 300)),
            memory_kib: env_number("KILN_FIDELITY_FILE_MEMORY_KIB", 6 << 20),
        }),
        jobs: a.jobs,
        progress: true,
        ..Options::default()
    };
    let partial = partial_reason(&a, &formats, &files)?;
    if a.differential {
        return run_differential(&a, &files, &options, partial.as_deref());
    }
    let report = fidelity::run(
        &kiln_testing::formats::registered(),
        &files,
        &corpus,
        &options,
    );
    let _ = std::fs::remove_dir_all(&options.scratch);
    let _ = std::fs::remove_dir(kiln_testing::repo_root().join("target/fidelity"));
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
        let old = std::fs::read_to_string(path).ok();
        if !a.formats.is_empty()
            && a.formats.len() < FixtureFormat::ALL.len()
            && let Some(old) = &old
        {
            md = merge_report(old, &md, &a.formats);
        }
        if let Some(old) = &old {
            md = differential::keep_section(old, &md);
        }
        let digest = stamp::digest(&kiln_testing::repo_root());
        md = with_stamps(&md, old.as_deref().unwrap_or_default(), &formats, &digest);
        match (&partial, &old) {
            // A run that did not cover the corpus never replaces a report that did.
            (Some(why), Some(_)) => {
                text = format!(
                    "PARTIAL RUN, NOT THE PUBLISHED RESULT: {why}.\n{text}{} left as it was.\n",
                    path.display()
                );
            }
            (partial, _) => {
                if let Some(why) = partial {
                    md = md.replace(
                        "- Result: **pass**",
                        &format!("- Result: **partial, {why}**"),
                    );
                }
                std::fs::write(path, md)
                    .map_err(|e| CliError::Failed(format!("{}: {e}", path.display())))?;
                text.push_str(&format!("wrote {}\n", path.display()));
            }
        }
    } else if let Some(why) = &partial {
        text = format!("PARTIAL RUN: {why}.\n{text}");
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

/// The report's corpus line, counted from its `## Files` table.
fn corpus_line(md: &str) -> String {
    let rows: Vec<&str> = md
        .lines()
        .skip_while(|l| *l != "## Files")
        .skip(1)
        .take_while(|l| !l.starts_with("## "))
        .filter(|l| l.starts_with("| `"))
        .collect();
    let real = rows
        .iter()
        .filter(|l| l.starts_with("| `external/"))
        .count();
    format!(
        "- Corpus: {} files, {real} real-world and {} generated or made by hand",
        rows.len(),
        rows.len() - real
    )
}

/// Why this run does not stand for the whole corpus, if it does not.
fn partial_reason(
    a: &Args,
    formats: &[FixtureFormat],
    files: &[fixtures::Fixture],
) -> Result<Option<String>, CliError> {
    let manifest = fixtures::fixtures_dir().join("corpus-manifest.json");
    let text = std::fs::read_to_string(&manifest)
        .map_err(|e| CliError::Failed(format!("{}: {e}", manifest.display())))?;
    let manifest: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| CliError::Failed(format!("{}: {e}", manifest.display())))?;
    let listed = manifest["files"].as_array().map_or(0, |entries| {
        entries
            .iter()
            .filter(|e| {
                e["format"]
                    .as_str()
                    .and_then(FixtureFormat::parse)
                    .is_some_and(|f| formats.contains(&f))
            })
            .count()
    });
    let present = files
        .iter()
        .filter(|f| f.kind == FixtureKind::External)
        .count();
    Ok(partial_why(a, listed, present, files.len()))
}

fn partial_why(a: &Args, listed: usize, present: usize, scored: usize) -> Option<String> {
    if a.ci {
        return Some(format!(
            "--ci scored the {scored} committed files, without the downloaded ones"
        ));
    }
    if !a.oracle {
        return Some(format!(
            "--no-oracle left visual and oracle_opens out of {scored} files"
        ));
    }
    if !a.only.is_empty() {
        return Some(format!("--only chose {scored} files"));
    }
    if present < listed {
        return Some(format!(
            "{present} of the {listed} downloaded files are here, so {scored} files were scored"
        ));
    }
    None
}

/// `md` with its stamp line: the digest for `formats`, the other formats' as `old` had them.
fn with_stamps(md: &str, old: &str, formats: &[FixtureFormat], digest: &str) -> String {
    let previous = stamps_of(old);
    let line = FixtureFormat::ALL
        .iter()
        .filter_map(|f| {
            if formats.contains(f) {
                Some(format!("{f}={digest}"))
            } else {
                previous
                    .iter()
                    .find(|(n, _)| *n == f.to_string())
                    .map(|(n, d)| format!("{n}={d}"))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    let line = format!("- Stamps: {line}");
    match md.lines().position(|l| l.starts_with("- Result: ")) {
        Some(at) => {
            let mut lines: Vec<&str> = md
                .lines()
                .filter(|l| !l.starts_with("- Stamps: "))
                .collect();
            let at = at.min(lines.len());
            lines.insert(at, &line);
            lines.join("\n") + "\n"
        }
        None => md.to_owned(),
    }
}

fn stamps_of(md: &str) -> Vec<(String, String)> {
    md.lines()
        .find_map(|l| l.strip_prefix("- Stamps: "))
        .map(|l| {
            l.split(", ")
                .filter_map(|p| p.split_once('='))
                .map(|(n, d)| (n.to_owned(), d.to_owned()))
                .collect()
        })
        .unwrap_or_default()
}

/// What in a report does not match `digest`: every format and the side-by-side section must.
fn stale_parts(md: &str, digest: &str) -> Vec<String> {
    let stamps = stamps_of(md);
    let mut stale: Vec<String> = FixtureFormat::ALL
        .iter()
        .map(ToString::to_string)
        .filter(|f| !stamps.iter().any(|(n, d)| n == f && d == digest))
        .collect();
    let section = md.lines().skip_while(|l| !l.starts_with("## Side by side"));
    let wanted = format!("- Stamp: `{digest}`");
    if !section.take(4).any(|l| l == wanted) {
        stale.push("the side-by-side section".to_owned());
    }
    stale
}

/// `--check-stamp <report>`: fails when the report was not produced by these inputs.
fn check_stamp(path: &str) -> Result<String, CliError> {
    let md = std::fs::read_to_string(path).map_err(|e| CliError::Failed(format!("{path}: {e}")))?;
    let digest = stamp::digest(&kiln_testing::repo_root());
    let stale = stale_parts(&md, &digest);
    if stale.is_empty() {
        Ok(format!("{path} matches its inputs ({digest})"))
    } else {
        Err(CliError::Failed(format!(
            "{path} is stale for {}: run `cargo xtask fidelity` and `cargo xtask fidelity --differential`",
            stale.join(", ")
        )))
    }
}

fn env_number(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

/// One file's reader and writer, called by the run in a child process per file: the score goes to
/// standard output as JSON and the written file to `output`. With `compare`, `output` is another
/// tool's file to compare with the input.
fn worker(compare: bool, format: &str, input: &str, output: &str) -> Result<String, CliError> {
    let format = FixtureFormat::parse(format)
        .ok_or_else(|| CliError::Failed(format!("unknown format {format}")))?;
    let adapters = kiln_testing::formats::registered();
    let adapter = adapters
        .iter()
        .find(|a| a.format() == format)
        .ok_or_else(|| CliError::Failed(format!("no adapter for {format}")))?;
    let read = |p: &str| std::fs::read(p).map_err(|e| CliError::Failed(format!("{p}: {e}")));
    let bytes = read(input)?;
    let score: ModelScore = if compare {
        fidelity::score_written(adapter.as_ref(), &bytes, read(output)?, None)
    } else {
        let score = fidelity::score_model(adapter.as_ref(), &bytes);
        if let Some(written) = &score.output {
            std::fs::write(output, written)
                .map_err(|e| CliError::Failed(format!("{output}: {e}")))?;
        }
        score
    };
    Ok(score.to_json())
}

/// `--differential`: Kiln and the other tools over the corpus, written into the report's
/// side-by-side section.
fn run_differential(
    a: &Args,
    files: &[fixtures::Fixture],
    options: &Options,
    partial: Option<&str>,
) -> Result<String, CliError> {
    let tools = [Tool::LibreOfficeOdf, Tool::LibreOfficeDirect];
    let result = differential::run(&kiln_testing::formats::registered(), files, &tools, options)
        .map_err(CliError::Failed)?;
    let _ = std::fs::remove_dir_all(&options.scratch);
    let command = "cargo xtask fidelity --differential";
    let digest = stamp::digest(&kiln_testing::repo_root());
    let section = result.to_markdown(command, &digest);
    let mut text = String::new();
    for f in [
        FixtureFormat::Docx,
        FixtureFormat::Xlsx,
        FixtureFormat::Pptx,
    ] {
        for t in &tools {
            let (w, n) = result.wins(f, *t);
            text.push_str(&format!(
                "{f}: Kiln at least as good as {}: {w} of {n}\n",
                t.label()
            ));
        }
    }
    if let Some(path) = &a.report {
        let old = std::fs::read_to_string(path).unwrap_or_default();
        if let Some(why) = partial.filter(|_| !old.is_empty()) {
            text = format!(
                "PARTIAL RUN, NOT THE PUBLISHED RESULT: {why}.\n{text}{} left as it was.\n",
                path.display()
            );
        } else {
            std::fs::write(path, differential::replace_section(&old, &section))
                .map_err(|e| CliError::Failed(format!("{}: {e}", path.display())))?;
            text.push_str(&format!("wrote {}\n", path.display()));
        }
    }
    if result.passes_the_docx_bar() {
        Ok(text.trim_end().to_owned())
    } else {
        Err(CliError::Failed(format!(
            "{}\nKiln is below the 80 % bar against LibreOffice on docx",
            text.trim_end()
        )))
    }
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
        if is_row {
            // Emitted with the table's separator line below.
            continue;
        }
        out.push_str(line);
        out.push('\n');
        // The rows of a table follow its separator: the previous report's rows of the other
        // formats and this run's rows (a table this run left empty keeps the previous rows).
        if !line.starts_with("|---") || !emitted.insert(heading.clone()) {
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
            } else if l.starts_with("- Corpus: ") {
                corpus_line(&out)
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

    fn args(flags: &[&str]) -> Args {
        parse(&flags.iter().map(|f| (*f).to_owned()).collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn a_run_that_covers_less_says_so() {
        assert_eq!(partial_why(&args(&[]), 150, 150, 336), None);
        let why = partial_why(&args(&[]), 150, 149, 335).unwrap();
        assert!(why.contains("149 of the 150"), "{why}");
        assert!(partial_why(&args(&["--ci"]), 150, 0, 186).is_some());
        assert!(partial_why(&args(&["--no-oracle"]), 150, 150, 336).is_some());
        assert!(partial_why(&args(&["--only", "a"]), 150, 150, 1).is_some());
    }

    #[test]
    fn stamps_tie_a_report_to_its_inputs() {
        let md = "# R\n\n- Result: **pass**\n\n## Summary\n\n<!-- differential:start -->\n## Side by side\n\n- Stamp: `aa`\n\n";
        let all = FixtureFormat::ALL;
        let stamped = with_stamps(md, "", &all, "aa");
        assert!(stale_parts(&stamped, "aa").is_empty(), "{stamped}");
        assert_eq!(stale_parts(&stamped, "bb").len(), 5);
        // A run of one format stamps that format.
        let one = with_stamps(&stamped, &stamped, &[FixtureFormat::Docx], "bb");
        assert_eq!(stale_parts(&one, "bb").len(), 4);
        assert!(stamps_of(&one).contains(&("xlsx".to_owned(), "aa".to_owned())));
    }

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
        assert!(parse(&["--format".into(), "rtf".into()]).is_err());
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

    #[test]
    fn a_table_the_run_leaves_empty_keeps_the_other_formats_rows() {
        let old = "# R\n\n## Baseline\n\n| Dir | N |\n|---|---|\n| `docx/features` | 19 |\n| `pptx/features` | 19 |\n\n## End\n";
        let new = "# R\n\n## Baseline\n\n| Dir | N |\n|---|---|\n\n## End\n";
        let m = merge_report(old, new, &[FixtureFormat::Odf]);
        assert!(
            m.contains("|---|---|\n| `docx/features` | 19 |\n| `pptx/features` | 19 |\n\n## End"),
            "{m}"
        );
    }
}
