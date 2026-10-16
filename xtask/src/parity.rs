// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask parity`: the parity tracker (PLAN.md §7.2).
//!
//! ```text
//! cargo xtask parity                     validate, then regenerate PARITY.md, docs/parity-badge.json
//!                                        and the computed statuses in parity/functions.json
//! cargo xtask parity --check             validate and fail if any generated file is stale (CI)
//! cargo xtask parity --verify [--dry-run] [--app <app>] [--row <id>]...
//!                                        resolve and run the verify ids of every done/better row
//! cargo xtask parity --snapshot <tag>    record the statuses at a release tag (parity/releases/)
//! ```
//!
//! Inputs: `parity/parity.json` (the rows), `parity/functions.json` (every Excel worksheet
//! function), `parity/corpus.json` (fidelity fixtures and their thresholds), `parity/schema.json`
//! (a JSON Schema for all of them), `conformance/functions-implemented.json` (written by the
//! formula library; a function is `done` exactly when it is implemented there) and
//! `parity/releases/<tag>.json` (statuses at each release; PARITY.md lists the flips since the
//! newest one). `parity/README.md` documents the row shape and every verify id form.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;
use serde_json::Value;

use crate::util;

const PARITY: &str = "parity/parity.json";
const SCHEMA: &str = "parity/schema.json";
const FUNCTIONS: &str = "parity/functions.json";
const CORPUS: &str = "parity/corpus.json";
const RELEASES: &str = "parity/releases";
const IMPLEMENTED: &str = "conformance/functions-implemented.json";
const REPORT: &str = "PARITY.md";
const BADGE: &str = "docs/parity-badge.json";
const FIDELITY_REPORT: &str = "docs/qa/fidelity.md";

/// The apps in report order, with their display names.
const APPS: &[(&str, &str)] = &[
    ("write", "Kiln Write"),
    ("grid", "Kiln Grid"),
    ("deck", "Kiln Deck"),
    ("notes", "Kiln Notes"),
    ("suite", "Suite"),
];
const TIERS: &[&str] = &["critical", "commonly-missed", "niche"];
/// Fixture formats whose `fixtures/<format>/features/` directory the corpus must list in full.
const CORPUS_FORMATS: &[&str] = &["docx", "xlsx", "pptx"];

// ---------------------------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------------------------

/// What `cargo xtask parity` was asked to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    /// Validate and write the generated files.
    Generate,
    /// Validate and report stale generated files without writing.
    Check,
    /// Resolve and run verify ids.
    Verify {
        /// Print the commands instead of running them.
        dry_run: bool,
        /// Only rows of these apps (all when empty).
        apps: Vec<String>,
        /// Only these rows (all when empty).
        rows: Vec<String>,
    },
    /// Record the current statuses as release `tag`.
    Snapshot(String),
}

/// Parses `parity` options.
pub fn parse(args: &[String]) -> Result<Mode, String> {
    let mut mode = None;
    let mut dry_run = false;
    let mut apps = Vec::new();
    let mut rows = Vec::new();
    let mut iter = args.iter();
    let set = |m: Mode, mode: &mut Option<Mode>| -> Result<(), String> {
        if mode.is_some() {
            return Err("choose one of --check, --verify and --snapshot".into());
        }
        *mode = Some(m);
        Ok(())
    };
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--check" | "--ci" => set(Mode::Check, &mut mode)?,
            "--verify" => set(
                Mode::Verify {
                    dry_run: false,
                    apps: Vec::new(),
                    rows: Vec::new(),
                },
                &mut mode,
            )?,
            "--snapshot" => {
                let tag = iter.next().ok_or("--snapshot needs a tag such as v0.2.0")?;
                set(Mode::Snapshot(tag.clone()), &mut mode)?;
            }
            "--dry-run" => dry_run = true,
            "--app" => apps.push(iter.next().ok_or("--app needs an app name")?.clone()),
            "--row" => rows.push(iter.next().ok_or("--row needs a row id")?.clone()),
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    let mode = mode.unwrap_or(Mode::Generate);
    match mode {
        Mode::Verify { .. } => {
            for app in &apps {
                if !APPS.iter().any(|(a, _)| a == app) {
                    return Err(format!("unknown app '{app}'"));
                }
            }
            Ok(Mode::Verify {
                dry_run,
                apps,
                rows,
            })
        }
        other => {
            if dry_run || !apps.is_empty() || !rows.is_empty() {
                return Err("--dry-run, --app and --row only apply to --verify".into());
            }
            Ok(other)
        }
    }
}

/// Runs `cargo xtask parity`.
pub fn run(args: &[String]) -> Result<(), String> {
    let mode = parse(args)?;
    let root = util::repo_root();
    let tracker = Tracker::load(&root)?;
    match mode {
        Mode::Generate => {
            let outputs = tracker.outputs()?;
            let mut written = 0;
            for (rel, text) in &outputs {
                let path = root.join(rel);
                if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
                    write_file(&path, text)?;
                    println!("wrote {rel}");
                    written += 1;
                }
            }
            if written == 0 {
                println!("parity: generated files are up to date");
            }
            tracker.print_summary();
            Ok(())
        }
        Mode::Check => {
            let stale: Vec<String> = tracker
                .outputs()?
                .into_iter()
                .filter(|(rel, text)| {
                    std::fs::read_to_string(root.join(rel)).ok().as_deref() != Some(text.as_str())
                })
                .map(|(rel, _)| rel)
                .collect();
            tracker.print_summary();
            if stale.is_empty() {
                println!("parity --check: tracker valid, generated files current");
                Ok(())
            } else {
                Err(format!(
                    "stale: {} (run `cargo xtask parity` and commit the result)",
                    stale.join(", ")
                ))
            }
        }
        Mode::Verify {
            dry_run,
            apps,
            rows,
        } => verify(&root, &tracker, dry_run, &apps, &rows),
        Mode::Snapshot(tag) => {
            if !tag_regex().is_match(&tag) {
                return Err(format!("'{tag}' is not a release tag such as v0.2.0"));
            }
            let path = root.join(RELEASES).join(format!("{tag}.json"));
            if path.exists() {
                return Err(format!("{} already exists", rel_display(&root, &path)));
            }
            let snapshot = tracker.snapshot_json(&tag);
            write_file(&path, &snapshot)?;
            println!("wrote {}", rel_display(&root, &path));
            // The new snapshot becomes the baseline: regenerate the report against it.
            let tracker = Tracker::load(&root)?;
            for (rel, text) in tracker.outputs()? {
                write_file(&root.join(&rel), &text)?;
                println!("wrote {rel}");
            }
            Ok(())
        }
    }
}

fn write_file(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

fn rel_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
}

fn tag_regex() -> Regex {
    Regex::new(r"^v[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$").expect("static regex")
}

// ---------------------------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------------------------

/// One parity row (PLAN.md §7.2).
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Dotted, unique, stable id.
    pub id: String,
    /// `write|grid|deck|notes|suite`.
    pub app: String,
    /// Ribbon path or subsystem.
    pub area: String,
    /// What the feature is.
    pub feature: String,
    /// The Office name/location, or "—" for rows that exist to be better than Office.
    pub office: String,
    /// `critical|commonly-missed|niche`.
    pub tier: String,
    /// `P0|P1|P2|out`.
    pub priority: String,
    /// `missing|partial|done|better`.
    pub status: String,
    /// The delivering work package.
    pub wp: Option<String>,
    /// Verification ids.
    pub verify: Vec<String>,
    /// Free text (required for better rows).
    pub notes: String,
}

impl Row {
    fn from_json(v: &Value) -> Row {
        let s = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Row {
            id: s("id"),
            app: s("app"),
            area: s("area"),
            feature: s("feature"),
            office: s("office"),
            tier: s("tier"),
            priority: s("priority"),
            status: s("status"),
            wp: v.get("wp").and_then(Value::as_str).map(str::to_owned),
            verify: v
                .get("verify")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default(),
            notes: s("notes"),
        }
    }

    fn in_scope(&self) -> bool {
        self.priority != "out"
    }

    /// Rows that exist to be better than Office (§7.2).
    fn is_better_row(&self) -> bool {
        self.office == "—"
    }
}

/// One Excel worksheet function of `parity/functions.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Function {
    /// Upper-case name, e.g. `XLOOKUP`.
    pub name: String,
    /// Function family (one family row in parity.json each).
    pub family: String,
    /// Excel version that introduced it.
    pub since: String,
    /// `planned|done|out` as stored.
    pub status: String,
    /// Why it is out, or other remarks.
    pub notes: Option<String>,
}

impl Function {
    fn from_json(v: &Value) -> Function {
        let s = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned()
        };
        Function {
            name: s("name"),
            family: s("family"),
            since: s("since"),
            status: s("status"),
            notes: v.get("notes").and_then(Value::as_str).map(str::to_owned),
        }
    }
}

/// One fixture of `parity/corpus.json`.
#[derive(Debug, Clone, PartialEq)]
pub struct CorpusEntry {
    /// Path under `fixtures/`.
    pub file: String,
    /// Metric thresholds (ratios) and boolean checks.
    pub checks: BTreeMap<String, Value>,
    /// Parity rows the fixture exercises.
    pub rows: Vec<String>,
}

/// Statuses recorded at a release tag.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// Release tag, e.g. `v0.2.0`.
    pub tag: String,
    /// Tracker date at the tag.
    pub date: String,
    /// Overall score at the tag.
    pub score: f64,
    /// Row id → status.
    pub statuses: BTreeMap<String, String>,
}

/// Weights of §7.2.
#[derive(Debug, Clone, PartialEq)]
pub struct Weights {
    tier: BTreeMap<String, f64>,
    status: BTreeMap<String, f64>,
}

impl Weights {
    fn from_json(v: &Value) -> Weights {
        let map = |k: &str| -> BTreeMap<String, f64> {
            v.get(k)
                .and_then(Value::as_object)
                .map(|o| {
                    o.iter()
                        .map(|(k, v)| (k.clone(), v.as_f64().unwrap_or(0.0)))
                        .collect()
                })
                .unwrap_or_default()
        };
        Weights {
            tier: map("tier"),
            status: map("status"),
        }
    }
}

/// Everything `cargo xtask parity` reads, validated.
#[derive(Debug)]
pub struct Tracker {
    updated: String,
    weights: Weights,
    rows: Vec<Row>,
    functions: Vec<Function>,
    /// Names implemented according to conformance/functions-implemented.json, if it exists.
    implemented: Option<BTreeSet<String>>,
    corpus: Vec<CorpusEntry>,
    snapshot: Option<Snapshot>,
}

impl Tracker {
    /// Loads and validates every input; all problems are reported together.
    pub fn load(root: &Path) -> Result<Tracker, String> {
        let schema = read_json(&root.join(SCHEMA))?;
        let mut validator = Validator::new(&schema)?;
        let mut problems = Vec::new();

        let parity = read_json(&root.join(PARITY))?;
        problems.extend(label(PARITY, validator.validate_root(&parity), &parity));
        let functions_json = read_json(&root.join(FUNCTIONS))?;
        problems.extend(label(
            FUNCTIONS,
            validator.validate_def("functionsFile", &functions_json)?,
            &functions_json,
        ));
        let corpus_json = read_json(&root.join(CORPUS))?;
        problems.extend(label(
            CORPUS,
            validator.validate_def("corpusFile", &corpus_json)?,
            &corpus_json,
        ));
        let mut snapshots = Vec::new();
        let releases = root.join(RELEASES);
        if releases.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(&releases)
                .map_err(|e| format!("{}: {e}", releases.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .collect();
            entries.sort();
            for path in entries {
                let rel = rel_display(root, &path);
                let v = read_json(&path)?;
                let errors = validator.validate_def("snapshotFile", &v)?;
                if errors.is_empty() {
                    snapshots.push(snapshot_from_json(&v));
                } else {
                    problems.extend(label(&rel, errors, &v));
                }
            }
        }
        let implemented = read_implemented(root)?;
        if !problems.is_empty() {
            return Err(format!(
                "{} schema problem(s):\n  {}",
                problems.len(),
                problems.join("\n  ")
            ));
        }

        let rows: Vec<Row> = parity["rows"]
            .as_array()
            .map(|a| a.iter().map(Row::from_json).collect())
            .unwrap_or_default();
        let functions: Vec<Function> = functions_json
            .as_array()
            .map(|a| a.iter().map(Function::from_json).collect())
            .unwrap_or_default();
        let corpus = corpus_json["files"]
            .as_array()
            .map(|a| a.iter().map(corpus_from_json).collect())
            .unwrap_or_default();
        snapshots.sort_by_key(|s| version_key(&s.tag));
        let tracker = Tracker {
            updated: parity["updated"].as_str().unwrap_or_default().to_owned(),
            weights: Weights::from_json(&parity["weights"]),
            rows,
            functions,
            implemented,
            corpus,
            snapshot: snapshots.pop(),
        };
        let problems = tracker.semantic_problems(root);
        if problems.is_empty() {
            Ok(tracker)
        } else {
            Err(format!(
                "{} problem(s):\n  {}",
                problems.len(),
                problems.join("\n  ")
            ))
        }
    }

    /// Checks the rules a JSON Schema cannot express.
    fn semantic_problems(&self, root: &Path) -> Vec<String> {
        let mut out = Vec::new();
        // Rows: unique ids, id prefix = app, known work packages.
        let mut ids = BTreeSet::new();
        let known_wps = known_work_packages(root);
        for row in &self.rows {
            if !ids.insert(row.id.as_str()) {
                out.push(format!("{PARITY}: duplicate row id {}", row.id));
            }
            if !row.id.starts_with(&format!("{}.", row.app)) {
                out.push(format!(
                    "{PARITY}: row {} must start with its app '{}.'",
                    row.id, row.app
                ));
            }
            if let (Some(wp), Some(known)) = (&row.wp, &known_wps)
                && !known.contains(wp)
            {
                out.push(format!(
                    "{PARITY}: row {} names {wp}, which no work package file defines",
                    row.id
                ));
            }
        }
        // Functions: unique names, one family row per family, families consistent.
        let mut names = BTreeSet::new();
        for f in &self.functions {
            if !names.insert(f.name.as_str()) {
                out.push(format!("{FUNCTIONS}: duplicate function {}", f.name));
            }
        }
        match &self.implemented {
            Some(implemented) => {
                for name in implemented {
                    if !names.contains(name.as_str()) {
                        out.push(format!(
                            "{IMPLEMENTED}: {name} is not an Excel worksheet function listed in {FUNCTIONS}"
                        ));
                    }
                }
            }
            None => {
                for f in self.functions.iter().filter(|f| f.status == "done") {
                    out.push(format!(
                        "{FUNCTIONS}: {} is done, but {IMPLEMENTED} does not exist to prove it",
                        f.name
                    ));
                }
            }
        }
        let families: BTreeSet<&str> = self.functions.iter().map(|f| f.family.as_str()).collect();
        let family_counts = self.family_counts();
        for family in &families {
            let id = family_row_id(family);
            let Some(row) = self.rows.iter().find(|r| r.id == id) else {
                out.push(format!(
                    "{PARITY}: no family row {id} for the {family} functions"
                ));
                continue;
            };
            let c = family_counts.get(*family).copied().unwrap_or_default();
            let all_out = c.planned + c.done == 0;
            if all_out != (row.priority == "out") {
                out.push(format!(
                    "{PARITY}: row {id} is {} but {} of its {} functions are out of scope",
                    if row.priority == "out" {
                        "out of scope"
                    } else {
                        "in scope"
                    },
                    c.out,
                    c.out + c.planned + c.done
                ));
            }
            match row.status.as_str() {
                "done" | "better" if c.done < c.planned + c.done => out.push(format!(
                    "{PARITY}: row {id} is {} but only {}/{} {family} functions are done",
                    row.status,
                    c.done,
                    c.planned + c.done
                )),
                "partial" if c.done == 0 => out.push(format!(
                    "{PARITY}: row {id} is partial but no {family} function is done"
                )),
                _ => {}
            }
        }
        // Verify ids naming a function must name a listed function.
        for row in &self.rows {
            for id in &row.verify {
                if let Ok(VerifyId::Conformance {
                    function: Some(f), ..
                }) = VerifyId::parse(id)
                    && !names.contains(f.as_str())
                {
                    out.push(format!(
                        "{PARITY}: row {} verifies {f}, which {FUNCTIONS} does not list",
                        row.id
                    ));
                }
            }
        }
        // Corpus: every listed fixture exists, every feature fixture is listed, rows exist.
        let mut listed = BTreeSet::new();
        for entry in &self.corpus {
            if !listed.insert(entry.file.as_str()) {
                out.push(format!("{CORPUS}: {} is listed twice", entry.file));
            }
            if !root.join("fixtures").join(&entry.file).is_file() {
                out.push(format!("{CORPUS}: fixtures/{} does not exist", entry.file));
            }
            for row in &entry.rows {
                if !ids.contains(row.as_str()) {
                    out.push(format!("{CORPUS}: {} names unknown row {row}", entry.file));
                }
            }
        }
        for format in CORPUS_FORMATS {
            for file in feature_fixtures(root, format) {
                if !listed.contains(file.as_str()) {
                    out.push(format!(
                        "{CORPUS}: fixtures/{file} is not listed (add it with its expected checks)"
                    ));
                }
            }
        }
        out
    }

    /// The effective status of a function: `done` exactly when implemented (if the
    /// implementation list exists), otherwise as stored.
    fn function_status<'a>(&self, f: &'a Function) -> &'a str {
        match &self.implemented {
            Some(set) if set.contains(&f.name) => "done",
            Some(_) if f.status == "done" => "planned",
            _ => f.status.as_str(),
        }
    }

    fn family_counts(&self) -> BTreeMap<&str, FamilyCount> {
        let mut out: BTreeMap<&str, FamilyCount> = BTreeMap::new();
        for f in &self.functions {
            let c = out.entry(f.family.as_str()).or_default();
            match self.function_status(f) {
                "done" => c.done += 1,
                "out" => c.out += 1,
                _ => c.planned += 1,
            }
        }
        out
    }

    /// Score over the rows selected by `keep`.
    fn tally(&self, keep: impl Fn(&Row) -> bool) -> Tally {
        let mut t = Tally::default();
        for row in self.rows.iter().filter(|r| keep(r)) {
            if !row.in_scope() {
                t.out += 1;
                continue;
            }
            let tw = self.weights.tier.get(&row.tier).copied().unwrap_or(0.0);
            let sw = self.weights.status.get(&row.status).copied().unwrap_or(0.0);
            t.weight += tw;
            t.earned += tw * sw;
            t.rows += 1;
            match row.status.as_str() {
                "done" => t.done += 1,
                "better" => t.better += 1,
                "partial" => t.partial += 1,
                _ => t.missing += 1,
            }
        }
        t
    }

    /// Every generated file: path relative to the root → content.
    pub fn outputs(&self) -> Result<Vec<(String, String)>, String> {
        Ok(vec![
            (REPORT.to_owned(), self.report()),
            (BADGE.to_owned(), self.badge()),
            (FUNCTIONS.to_owned(), self.functions_json()),
        ])
    }

    fn print_summary(&self) {
        let all = self.tally(|_| true);
        let counts = self.family_counts();
        let (done, total): (usize, usize) = counts
            .values()
            .fold((0, 0), |(d, t), c| (d + c.done, t + c.done + c.planned));
        println!(
            "parity: {} rows ({} in scope), score {} — {} done, {} better, {} partial; functions {done}/{total} done",
            self.rows.len(),
            all.rows,
            percent(all.score()),
            all.done,
            all.better,
            all.partial
        );
    }

    /// `parity/functions.json` with the computed statuses, one entry per line.
    fn functions_json(&self) -> String {
        let mut out = String::from("[\n");
        let mut sorted: Vec<&Function> = self.functions.iter().collect();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        for (i, f) in sorted.iter().enumerate() {
            let mut line = format!(
                "  {{\"name\": {}, \"family\": {}, \"since\": {}, \"status\": {}",
                json_str(&f.name),
                json_str(&f.family),
                json_str(&f.since),
                json_str(self.function_status(f))
            );
            if let Some(notes) = &f.notes {
                let _ = write!(line, ", \"notes\": {}", json_str(notes));
            }
            line.push('}');
            if i + 1 < sorted.len() {
                line.push(',');
            }
            out.push_str(&line);
            out.push('\n');
        }
        out.push_str("]\n");
        out
    }

    /// `docs/parity-badge.json` in the shields.io endpoint schema.
    fn badge(&self) -> String {
        let score = self.tally(|_| true).score();
        let color = match score {
            s if s >= 0.9 => "brightgreen",
            s if s >= 0.75 => "green",
            s if s >= 0.5 => "yellowgreen",
            s if s >= 0.25 => "yellow",
            s if s > 0.0 => "orange",
            _ => "lightgrey",
        };
        format!(
            "{{\n  \"schemaVersion\": 1,\n  \"label\": \"parity\",\n  \"message\": {},\n  \"color\": \"{color}\"\n}}\n",
            json_str(&percent_whole(score))
        )
    }

    fn snapshot_json(&self, tag: &str) -> String {
        let mut out = format!(
            "{{\n  \"tag\": {},\n  \"date\": {},\n  \"score\": {:.4},\n  \"statuses\": {{\n",
            json_str(tag),
            json_str(&self.updated),
            self.tally(|_| true).score()
        );
        let lines: Vec<String> = self
            .rows
            .iter()
            .map(|r| format!("    {}: {}", json_str(&r.id), json_str(&r.status)))
            .collect();
        out.push_str(&lines.join(",\n"));
        out.push_str("\n  }\n}\n");
        out
    }

    /// Status flips since the newest release snapshot (or since the seed, when every row started
    /// `missing`).
    fn flips(&self) -> Vec<Flip> {
        let mut out = Vec::new();
        for row in &self.rows {
            let before = match &self.snapshot {
                Some(s) => s.statuses.get(&row.id).cloned(),
                None => Some("missing".to_owned()),
            };
            if before.as_deref() != Some(row.status.as_str()) {
                out.push(Flip {
                    id: row.id.clone(),
                    feature: row.feature.clone(),
                    from: before,
                    to: Some(row.status.clone()),
                });
            }
        }
        if let Some(s) = &self.snapshot {
            let ids: BTreeSet<&str> = self.rows.iter().map(|r| r.id.as_str()).collect();
            for (id, status) in &s.statuses {
                if !ids.contains(id.as_str()) {
                    out.push(Flip {
                        id: id.clone(),
                        feature: String::new(),
                        from: Some(status.clone()),
                        to: None,
                    });
                }
            }
        }
        out
    }

    /// The text of PARITY.md.
    fn report(&self) -> String {
        let mut md = String::new();
        let all = self.tally(|_| true);
        let _ = writeln!(md, "# Parity with Microsoft Office\n");
        let _ = writeln!(
            md,
            "<!-- Generated by `cargo xtask parity` from parity/parity.json; do not edit by hand. -->\n"
        );
        let _ = writeln!(
            md,
            "This page tracks the features people coming from Microsoft Office would miss, and the ones where Kiln aims to be better. \
Each row has a would-miss tier (critical, commonly-missed, niche), a delivery priority and a status; a row counts as done only when \
its named tests pass (`cargo xtask parity --verify`). The rules are in [PLAN.md §7](PLAN.md) and [parity/README.md](parity/README.md).\n"
        );
        let _ = writeln!(
            md,
            "Tracker updated {}. Overall score **{}** over {} rows in scope ({} out of scope).\n",
            self.updated,
            percent(all.score()),
            all.rows,
            all.out
        );
        let _ = writeln!(
            md,
            "Legend: {} better than Office · {} done · {} partial · {} missing · {} out of scope. \
Score = Σ tier weight × status weight ÷ Σ tier weight over rows in scope, with tier weights critical {}, commonly-missed {}, niche {} \
and status weights missing {}, partial {}, done {}, better {}.\n",
            glyph("better", "P0"),
            glyph("done", "P0"),
            glyph("partial", "P0"),
            glyph("missing", "P0"),
            glyph("missing", "out"),
            num(self.weights.tier.get("critical")),
            num(self.weights.tier.get("commonly-missed")),
            num(self.weights.tier.get("niche")),
            num(self.weights.status.get("missing")),
            num(self.weights.status.get("partial")),
            num(self.weights.status.get("done")),
            num(self.weights.status.get("better")),
        );

        // Scores.
        let _ = writeln!(md, "## Scores\n");
        let _ = writeln!(
            md,
            "| Scope | Score | Rows | Better | Done | Partial | Missing | Out of scope |"
        );
        let _ = writeln!(md, "|---|---:|---:|---:|---:|---:|---:|---:|");
        tally_row(&mut md, "**Overall**", &all);
        for (app, name) in APPS {
            tally_row(&mut md, name, &self.tally(|r| r.app == *app));
        }
        let _ = writeln!(md);
        let _ = writeln!(
            md,
            "| Tier | Score | Rows | Better | Done | Partial | Missing | Out of scope |"
        );
        let _ = writeln!(md, "|---|---:|---:|---:|---:|---:|---:|---:|");
        for tier in TIERS {
            tally_row(&mut md, tier, &self.tally(|r| r.tier == *tier));
        }
        let _ = writeln!(md);
        let _ = writeln!(
            md,
            "| App | Critical | Commonly-missed | Niche |\n|---|---:|---:|---:|"
        );
        for (app, name) in APPS {
            let cells: Vec<String> = TIERS
                .iter()
                .map(|tier| {
                    let t = self.tally(|r| r.app == *app && r.tier == *tier);
                    if t.rows == 0 {
                        "—".to_owned()
                    } else {
                        format!("{} ({})", percent(t.score()), t.rows)
                    }
                })
                .collect();
            let _ = writeln!(md, "| {name} | {} |", cells.join(" | "));
        }
        let _ = writeln!(md);

        // Better than Office.
        let _ = writeln!(md, "## Better than Office\n");
        let _ = writeln!(
            md,
            "Rows that exist to be better than Office (no Office equivalent, `office: \"—\"`) and rows already marked better.\n"
        );
        let _ = writeln!(md, "| | Feature | App | Priority | Why it is better |");
        let _ = writeln!(md, "|---|---|---|---|---|");
        for row in self
            .rows
            .iter()
            .filter(|r| r.is_better_row() || r.status == "better")
        {
            let _ = writeln!(
                md,
                "| {} | {} | {} | {} | {} |",
                glyph(&row.status, &row.priority),
                cell(&row.feature),
                app_name(&row.app),
                row.priority,
                cell(
                    row.notes
                        .strip_prefix("Better than Office: ")
                        .unwrap_or(&row.notes)
                )
            );
        }
        let _ = writeln!(md);

        // Flips.
        let flips = self.flips();
        match &self.snapshot {
            Some(s) => {
                let _ = writeln!(md, "## Status flips since {}\n", s.tag);
                let _ = writeln!(
                    md,
                    "Compared with `parity/releases/{}.json` ({}, score {}).\n",
                    s.tag,
                    s.date,
                    percent(s.score)
                );
            }
            None => {
                let _ = writeln!(md, "## Status flips since the seed\n");
                let _ = writeln!(
                    md,
                    "No release has been tagged yet, so flips are counted from the seed, where every row started missing. \
`cargo xtask parity --snapshot <tag>` records the statuses at each release.\n"
                );
            }
        }
        if flips.is_empty() {
            let _ = writeln!(md, "No status has changed.\n");
        } else {
            let _ = writeln!(md, "| Row | Feature | From | To |\n|---|---|---|---|");
            for f in &flips {
                let _ = writeln!(
                    md,
                    "| `{}` | {} | {} | {} |",
                    f.id,
                    cell(&f.feature),
                    f.from.as_deref().unwrap_or("(new row)"),
                    f.to.as_deref().unwrap_or("(row removed)")
                );
            }
            let _ = writeln!(md);
        }

        // Functions.
        let counts = self.family_counts();
        let _ = writeln!(md, "## Excel worksheet functions\n");
        let (done, planned, out) = counts.values().fold((0, 0, 0), |(d, p, o), c| {
            (d + c.done, p + c.planned, o + c.out)
        });
        let _ = writeln!(
            md,
            "{} functions from Microsoft's public list ([parity/functions.json](parity/functions.json)): {done} done, {planned} planned, {out} out of scope. \
A function is done when the formula library lists it as implemented in `{IMPLEMENTED}`{}; each family row below aggregates its functions.\n",
            self.functions.len(),
            if self.implemented.is_some() {
                ""
            } else {
                " (not written yet)"
            }
        );
        let _ = writeln!(
            md,
            "| Family | Done | Planned | Out | Row |\n|---|---:|---:|---:|---|"
        );
        for (family, c) in &counts {
            let _ = writeln!(
                md,
                "| {family} | {} | {} | {} | `{}` |",
                c.done,
                c.planned,
                c.out,
                family_row_id(family)
            );
        }
        let _ = writeln!(md);
        let outs: Vec<&Function> = self
            .functions
            .iter()
            .filter(|f| self.function_status(f) == "out")
            .collect();
        if !outs.is_empty() {
            let _ = writeln!(
                md,
                "Out of scope:\n\n| Function | Family | Why |\n|---|---|---|"
            );
            for f in outs {
                let _ = writeln!(
                    md,
                    "| {} | {} | {} |",
                    f.name,
                    f.family,
                    cell(f.notes.as_deref().unwrap_or("out of scope"))
                );
            }
            let _ = writeln!(md);
        }

        // Corpus.
        let _ = writeln!(md, "## Fidelity corpus\n");
        let _ = writeln!(
            md,
            "[parity/corpus.json](parity/corpus.json) lists every feature fixture with the checks `cargo xtask fidelity` must meet; results are in `{FIDELITY_REPORT}`.\n"
        );
        let _ = writeln!(md, "| Format | Fixtures | Thresholds |\n|---|---:|---|");
        for format in CORPUS_FORMATS {
            let entries: Vec<&CorpusEntry> = self
                .corpus
                .iter()
                .filter(|e| e.file.starts_with(&format!("{format}/")))
                .collect();
            let mut thresholds: BTreeMap<&str, String> = BTreeMap::new();
            for e in &entries {
                for (k, v) in &e.checks {
                    let text = match v {
                        Value::Bool(true) => "yes".to_owned(),
                        Value::Bool(false) => "no".to_owned(),
                        other => format!("≥ {}", other),
                    };
                    let slot = thresholds.entry(k.as_str()).or_insert_with(|| text.clone());
                    if *slot != text {
                        *slot = "varies".to_owned();
                    }
                }
            }
            let t: Vec<String> = thresholds.iter().map(|(k, v)| format!("{k} {v}")).collect();
            let _ = writeln!(md, "| {format} | {} | {} |", entries.len(), t.join(", "));
        }
        let _ = writeln!(md);

        // Per app.
        for (app, name) in APPS {
            let rows: Vec<&Row> = self.rows.iter().filter(|r| r.app == *app).collect();
            let t = self.tally(|r| r.app == *app);
            let _ = writeln!(md, "## {name}\n");
            let _ = writeln!(
                md,
                "Score {} over {} rows in scope; {} better, {} done, {} partial, {} missing, {} out of scope.\n",
                percent(t.score()),
                t.rows,
                t.better,
                t.done,
                t.partial,
                t.missing,
                t.out
            );
            let mut areas: Vec<&str> = Vec::new();
            for r in &rows {
                if !areas.contains(&r.area.as_str()) {
                    areas.push(&r.area);
                }
            }
            for area in areas {
                let _ = writeln!(md, "### {}\n", area);
                let _ = writeln!(
                    md,
                    "| | Feature | Office | Tier | Priority | WP | Id |\n|---|---|---|---|---|---|---|"
                );
                for r in rows.iter().filter(|r| r.area == area) {
                    let _ = writeln!(
                        md,
                        "| {} | {} | {} | {} | {} | {} | `{}` |",
                        glyph(&r.status, &r.priority),
                        cell(&r.feature),
                        cell(&r.office),
                        r.tier,
                        r.priority,
                        r.wp.as_deref().unwrap_or("—"),
                        r.id
                    );
                }
                let _ = writeln!(md);
            }
        }
        while md.ends_with("\n\n") {
            md.pop();
        }
        md
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct FamilyCount {
    done: usize,
    planned: usize,
    out: usize,
}

/// Weighted score and counts over a set of rows.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Tally {
    weight: f64,
    earned: f64,
    rows: usize,
    out: usize,
    missing: usize,
    partial: usize,
    done: usize,
    better: usize,
}

impl Tally {
    /// Σ tierWeight·statusWeight / Σ tierWeight (0 when empty).
    pub fn score(&self) -> f64 {
        if self.weight > 0.0 {
            self.earned / self.weight
        } else {
            0.0
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Flip {
    id: String,
    feature: String,
    from: Option<String>,
    to: Option<String>,
}

fn tally_row(md: &mut String, name: &str, t: &Tally) {
    let _ = writeln!(
        md,
        "| {name} | {} | {} | {} | {} | {} | {} | {} |",
        percent(t.score()),
        t.rows,
        t.better,
        t.done,
        t.partial,
        t.missing,
        t.out
    );
}

fn family_row_id(family: &str) -> String {
    format!("grid.formulas.functions.{family}")
}

fn app_name(app: &str) -> &str {
    APPS.iter()
        .find(|(a, _)| *a == app)
        .map_or(app, |(_, name)| name)
}

/// Status glyph (no emoji, per the house style).
fn glyph(status: &str, priority: &str) -> &'static str {
    if priority == "out" {
        return "–";
    }
    match status {
        "better" => "★",
        "done" => "●",
        "partial" => "◐",
        _ => "○",
    }
}

/// One decimal, British spacing: `71.4 %`.
fn percent(score: f64) -> String {
    format!("{:.1} %", (score * 1000.0).round() / 10.0)
}

/// Whole percent for the badge, never rounding up to 100 % before it is reached.
fn percent_whole(score: f64) -> String {
    let p = (score * 100.0).floor();
    format!("{p:.0} %")
}

fn num(v: Option<&f64>) -> String {
    let v = v.copied().unwrap_or(0.0);
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

/// Escapes text for a Markdown table cell.
fn cell(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('\n', " ")
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_owned())
}

/// Sort key for release tags: numeric major/minor/patch, then the full text.
fn version_key(tag: &str) -> (Vec<u64>, String) {
    let core = tag.trim_start_matches('v');
    let nums = core
        .split(['.', '-'])
        .take(3)
        .map(|p| p.parse::<u64>().unwrap_or(0))
        .collect();
    (nums, tag.to_owned())
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: invalid JSON: {e}", path.display()))
}

/// `conformance/functions-implemented.json` (WP-57): `{"NAME": "implemented" | "planned"}`;
/// also accepted: a list of names, or either form under a `functions` key.
fn read_implemented(root: &Path) -> Result<Option<BTreeSet<String>>, String> {
    let path = root.join(IMPLEMENTED);
    if !path.exists() {
        return Ok(None);
    }
    let v = read_json(&path)?;
    parse_implemented(&v)
        .map(Some)
        .map_err(|e| format!("{IMPLEMENTED}: {e}"))
}

fn parse_implemented(v: &Value) -> Result<BTreeSet<String>, String> {
    let v = v.get("functions").unwrap_or(v);
    let mut out = BTreeSet::new();
    match v {
        Value::Array(items) => {
            for item in items {
                let name = item
                    .as_str()
                    .or_else(|| item.get("name").and_then(Value::as_str))
                    .ok_or("list entries must be names or objects with a name")?;
                out.insert(name.to_ascii_uppercase());
            }
        }
        Value::Object(map) => {
            for (name, state) in map {
                let implemented = match state {
                    Value::Bool(b) => *b,
                    Value::String(s) => match s.as_str() {
                        "implemented" | "done" => true,
                        "planned" | "missing" | "out" => false,
                        other => return Err(format!("{name}: unknown state '{other}'")),
                    },
                    _ => return Err(format!("{name}: state must be a string or boolean")),
                };
                if implemented {
                    out.insert(name.to_ascii_uppercase());
                }
            }
        }
        _ => return Err("expected an object of name → state or a list of names".into()),
    }
    Ok(out)
}

fn snapshot_from_json(v: &Value) -> Snapshot {
    Snapshot {
        tag: v["tag"].as_str().unwrap_or_default().to_owned(),
        date: v["date"].as_str().unwrap_or_default().to_owned(),
        score: v["score"].as_f64().unwrap_or(0.0),
        statuses: v["statuses"]
            .as_object()
            .map(|o| {
                o.iter()
                    .map(|(k, s)| (k.clone(), s.as_str().unwrap_or_default().to_owned()))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn corpus_from_json(v: &Value) -> CorpusEntry {
    CorpusEntry {
        file: v["file"].as_str().unwrap_or_default().to_owned(),
        checks: v["checks"]
            .as_object()
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
        rows: v
            .get("rows")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_owned)
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// `fixtures/<format>/features/*.<format>`, relative to `fixtures/`, sorted.
fn feature_fixtures(root: &Path, format: &str) -> Vec<String> {
    let dir = root.join("fixtures").join(format).join("features");
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .map(|rd| {
            rd.filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|name| {
                    Path::new(name)
                        .extension()
                        .is_some_and(|x| x.eq_ignore_ascii_case(format))
                })
                .map(|name| format!("{format}/features/{name}"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// Work package ids from `plan/work-packages.json` and `rounds/round-*.json` (§7.3), or `None`
/// when the plan file is absent (e.g. a partial checkout).
fn known_work_packages(root: &Path) -> Option<BTreeSet<String>> {
    let plan = read_json(&root.join("plan/work-packages.json")).ok()?;
    let mut out = BTreeSet::new();
    let mut add = |v: &Value| {
        let list = v
            .as_array()
            .or_else(|| v.get("work_packages").and_then(Value::as_array));
        for wp in list.into_iter().flatten() {
            if let Some(id) = wp.get("id").and_then(Value::as_str) {
                out.insert(id.to_owned());
            }
        }
    };
    add(&plan);
    if let Ok(rd) = std::fs::read_dir(root.join("rounds")) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|x| x == "json")
                && let Ok(v) = read_json(&path)
            {
                add(&v);
            }
        }
    }
    Some(out)
}

/// Prefixes schema errors with the file and names rows by id instead of index.
fn label(file: &str, errors: Vec<String>, doc: &Value) -> Vec<String> {
    let re = Regex::new(r"/rows/([0-9]+)").expect("static regex");
    errors
        .into_iter()
        .map(|e| {
            let named = re.replace_all(&e, |c: &regex::Captures<'_>| {
                let id = c[1]
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| doc["rows"].get(i))
                    .and_then(|r| r.get("id"))
                    .and_then(Value::as_str);
                match id {
                    Some(id) => format!("row {id} "),
                    None => c[0].to_owned(),
                }
            });
            format!("{file}: {}", named.replace(" /", " ").replace(" :", ":"))
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// JSON Schema (the 2020-12 subset parity/schema.json uses)
// ---------------------------------------------------------------------------------------------

/// Validates instances against `parity/schema.json`.
///
/// Supported: `$ref` (local), `type`, `enum`, `const`, `minLength`, `maxLength`, `pattern`,
/// `minimum`, `maximum`, `exclusiveMinimum`, `exclusiveMaximum`, `items`, `minItems`, `maxItems`,
/// `uniqueItems`, `contains`, `properties`, `required`, `additionalProperties`, `propertyNames`,
/// `minProperties`, `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`; annotations are
/// ignored. Any other keyword is an error, so the schema cannot silently promise more than is
/// checked.
#[derive(Debug)]
pub struct Validator<'s> {
    root: &'s Value,
    regexes: HashMap<String, Regex>,
}

const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "$defs",
    "title",
    "description",
    "examples",
    "default",
    "deprecated",
];

impl<'s> Validator<'s> {
    /// A validator for `root`; compiles every pattern up front.
    pub fn new(root: &'s Value) -> Result<Validator<'s>, String> {
        let mut v = Validator {
            root,
            regexes: HashMap::new(),
        };
        v.compile_patterns(root)?;
        Ok(v)
    }

    fn compile_patterns(&mut self, schema: &Value) -> Result<(), String> {
        match schema {
            Value::Object(map) => {
                for (k, v) in map {
                    if k == "pattern" {
                        if let Some(p) = v.as_str() {
                            let re = Regex::new(p)
                                .map_err(|e| format!("{SCHEMA}: bad pattern {p}: {e}"))?;
                            self.regexes.insert(p.to_owned(), re);
                        }
                    } else if k != "const" && k != "enum" && k != "examples" && k != "default" {
                        self.compile_patterns(v)?;
                    }
                }
            }
            Value::Array(items) => {
                for item in items {
                    self.compile_patterns(item)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// Validates against the root schema.
    pub fn validate_root(&mut self, instance: &Value) -> Vec<String> {
        let root = self.root;
        let mut errors = Vec::new();
        self.check(root, instance, "", &mut errors);
        errors
    }

    /// Validates against `#/$defs/<name>`.
    pub fn validate_def(&mut self, name: &str, instance: &Value) -> Result<Vec<String>, String> {
        let schema = self
            .root
            .pointer(&format!("/$defs/{name}"))
            .ok_or_else(|| format!("{SCHEMA}: missing $defs/{name}"))?;
        let mut errors = Vec::new();
        self.check(schema, instance, "", &mut errors);
        Ok(errors)
    }

    fn errors_of(&mut self, schema: &Value, instance: &Value, path: &str) -> Vec<String> {
        let mut e = Vec::new();
        self.check(schema, instance, path, &mut e);
        e
    }

    fn check(&mut self, schema: &Value, inst: &Value, path: &str, errors: &mut Vec<String>) {
        let map = match schema {
            Value::Bool(true) => return,
            Value::Bool(false) => {
                errors.push(format!("{}: not allowed", at(path)));
                return;
            }
            Value::Object(map) => map,
            _ => {
                errors.push(format!("{}: schema is not an object", at(path)));
                return;
            }
        };
        for (key, value) in map {
            match key.as_str() {
                "$ref" => {
                    let target = value
                        .as_str()
                        .and_then(|r| r.strip_prefix('#'))
                        .and_then(|p| self.root.pointer(p));
                    match target {
                        Some(t) => self.check(t, inst, path, errors),
                        None => errors.push(format!("{}: unresolvable $ref {value}", at(path))),
                    }
                }
                "type" => {
                    let ok = match value {
                        Value::String(t) => type_matches(t, inst),
                        Value::Array(ts) => ts
                            .iter()
                            .filter_map(Value::as_str)
                            .any(|t| type_matches(t, inst)),
                        _ => false,
                    };
                    if !ok {
                        errors.push(format!(
                            "{}: expected {}, found {}",
                            at(path),
                            type_list(value),
                            type_name(inst)
                        ));
                    }
                }
                "enum" => {
                    let ok = value
                        .as_array()
                        .is_some_and(|a| a.iter().any(|v| v == inst));
                    if !ok {
                        errors.push(format!("{}: {inst} is not one of {value}", at(path)));
                    }
                }
                "const" => {
                    if value != inst {
                        errors.push(format!("{}: {inst} must be {value}", at(path)));
                    }
                }
                "minLength" | "maxLength" => {
                    if let (Some(s), Some(n)) = (inst.as_str(), value.as_u64()) {
                        let len = s.chars().count() as u64;
                        if key == "minLength" && len < n {
                            errors.push(format!("{}: shorter than {n} characters", at(path)));
                        }
                        if key == "maxLength" && len > n {
                            errors.push(format!("{}: longer than {n} characters", at(path)));
                        }
                    }
                }
                "pattern" => {
                    if let (Some(s), Some(p)) = (inst.as_str(), value.as_str()) {
                        let matched = self.regexes.get(p).is_some_and(|re| re.is_match(s));
                        if !matched {
                            errors.push(format!("{}: {inst} does not match {p}", at(path)));
                        }
                    }
                }
                "minimum" | "maximum" | "exclusiveMinimum" | "exclusiveMaximum" => {
                    if let (Some(x), Some(bound)) = (inst.as_f64(), value.as_f64()) {
                        let ok = match key.as_str() {
                            "minimum" => x >= bound,
                            "maximum" => x <= bound,
                            "exclusiveMinimum" => x > bound,
                            _ => x < bound,
                        };
                        if !ok {
                            errors.push(format!("{}: {x} violates {key} {bound}", at(path)));
                        }
                    }
                }
                "items" => {
                    if let Some(items) = inst.as_array() {
                        for (i, item) in items.iter().enumerate() {
                            self.check(value, item, &format!("{path}/{i}"), errors);
                        }
                    }
                }
                "minItems" | "maxItems" => {
                    if let (Some(items), Some(n)) = (inst.as_array(), value.as_u64()) {
                        let len = items.len() as u64;
                        if key == "minItems" && len < n {
                            errors.push(format!("{}: fewer than {n} items", at(path)));
                        }
                        if key == "maxItems" && len > n {
                            errors.push(format!("{}: more than {n} items", at(path)));
                        }
                    }
                }
                "uniqueItems" => {
                    if let (Some(items), Some(true)) = (inst.as_array(), value.as_bool()) {
                        for (i, a) in items.iter().enumerate() {
                            if items[..i].contains(a) {
                                errors.push(format!("{}: duplicate item {a}", at(path)));
                            }
                        }
                    }
                }
                "contains" => {
                    if let Some(items) = inst.as_array() {
                        let found = items
                            .iter()
                            .any(|item| self.errors_of(value, item, path).is_empty());
                        if !found {
                            errors.push(format!(
                                "{}: no item matches {}",
                                at(path),
                                describe(value, self.root)
                            ));
                        }
                    }
                }
                "properties" => {
                    if let (Some(obj), Some(props)) = (inst.as_object(), value.as_object()) {
                        for (name, sub) in props {
                            if let Some(v) = obj.get(name) {
                                self.check(sub, v, &format!("{path}/{name}"), errors);
                            }
                        }
                    }
                }
                "required" => {
                    if let (Some(obj), Some(req)) = (inst.as_object(), value.as_array()) {
                        for name in req.iter().filter_map(Value::as_str) {
                            if !obj.contains_key(name) {
                                errors.push(format!("{}: missing property \"{name}\"", at(path)));
                            }
                        }
                    }
                }
                "additionalProperties" => {
                    if let Some(obj) = inst.as_object() {
                        let declared = map.get("properties").and_then(Value::as_object);
                        for (name, v) in obj {
                            if declared.is_some_and(|d| d.contains_key(name)) {
                                continue;
                            }
                            if value == &Value::Bool(false) {
                                errors.push(format!("{}: unknown property \"{name}\"", at(path)));
                            } else {
                                self.check(value, v, &format!("{path}/{name}"), errors);
                            }
                        }
                    }
                }
                "propertyNames" => {
                    if let Some(obj) = inst.as_object() {
                        for name in obj.keys() {
                            let key_value = Value::String(name.clone());
                            self.check(value, &key_value, &format!("{path}/{name}"), errors);
                        }
                    }
                }
                "minProperties" => {
                    if let (Some(obj), Some(n)) = (inst.as_object(), value.as_u64())
                        && (obj.len() as u64) < n
                    {
                        errors.push(format!("{}: fewer than {n} properties", at(path)));
                    }
                }
                "allOf" => {
                    for sub in value.as_array().into_iter().flatten() {
                        let sub_errors = self.errors_of(sub, inst, path);
                        if sub_errors.is_empty() {
                            continue;
                        }
                        match sub.get("description").and_then(Value::as_str) {
                            Some(rule) => errors.push(format!(
                                "{}: breaks the rule \"{rule}\" ({})",
                                at(path),
                                sub_errors.join("; ")
                            )),
                            None => errors.extend(sub_errors),
                        }
                    }
                }
                "anyOf" | "oneOf" => {
                    let branches: Vec<Vec<String>> = value
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|sub| self.errors_of(sub, inst, path))
                        .collect();
                    let passing = branches.iter().filter(|e| e.is_empty()).count();
                    if passing == 0 {
                        let closest = branches
                            .iter()
                            .min_by_key(|e| e.len())
                            .map(|e| e.join("; "))
                            .unwrap_or_default();
                        errors.push(format!(
                            "{}: {inst} matches none of the allowed forms (closest: {closest})",
                            at(path)
                        ));
                    } else if key == "oneOf" && passing > 1 {
                        errors.push(format!(
                            "{}: matches {passing} forms where exactly one is allowed",
                            at(path)
                        ));
                    }
                }
                "not" => {
                    if self.errors_of(value, inst, path).is_empty() {
                        errors.push(format!(
                            "{}: {inst} must not match {}",
                            at(path),
                            describe(value, self.root)
                        ));
                    }
                }
                "if" => {
                    let branch = if self.errors_of(value, inst, path).is_empty() {
                        map.get("then")
                    } else {
                        map.get("else")
                    };
                    if let Some(b) = branch {
                        self.check(b, inst, path, errors);
                    }
                }
                "then" | "else" => {}
                k if ANNOTATIONS.contains(&k) => {}
                other => errors.push(format!(
                    "{SCHEMA}: unsupported keyword \"{other}\" (extend xtask/src/parity.rs first)"
                )),
            }
        }
    }
}

fn at(path: &str) -> &str {
    if path.is_empty() { "/" } else { path }
}

fn type_matches(t: &str, v: &Value) -> bool {
    match t {
        "null" => v.is_null(),
        "boolean" => v.is_boolean(),
        "object" => v.is_object(),
        "array" => v.is_array(),
        "string" => v.is_string(),
        "number" => v.is_number(),
        "integer" => v.as_f64().is_some_and(|x| x.fract() == 0.0),
        _ => false,
    }
}

fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

fn type_list(v: &Value) -> String {
    match v {
        Value::Array(a) => a
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(" or "),
        other => other.as_str().unwrap_or("?").to_owned(),
    }
}

/// A short description of a subschema for messages.
fn describe(schema: &Value, root: &Value) -> String {
    if let Some(r) = schema.get("$ref").and_then(Value::as_str) {
        let target = r.strip_prefix('#').and_then(|p| root.pointer(p));
        return match target {
            Some(t) => describe(t, root),
            None => r.to_owned(),
        };
    }
    if let Some(p) = schema.get("pattern").and_then(Value::as_str) {
        return format!("pattern {p}");
    }
    schema.to_string()
}

// ---------------------------------------------------------------------------------------------
// Verify ids
// ---------------------------------------------------------------------------------------------

/// A parsed verify id (PLAN.md §7.2).
#[derive(Debug, Clone, PartialEq)]
pub enum VerifyId {
    /// `e2e:<spec>:<test-title-slug>` → `e2e/specs/<spec>.spec.ts`, Playwright `--grep`.
    E2e {
        /// Spec file stem.
        spec: String,
        /// Slug of the test title.
        slug: String,
    },
    /// `unit:<crate-or-package>/<path>` → nextest filter or Vitest `-t`.
    Unit {
        /// Crate (`crates/<name>`, `xtask`) or package (`packages/<name>`, `apps/<name>`).
        target: String,
        /// Path segments (module path, or `tests/<binary>/…`).
        path: String,
    },
    /// `conformance:<csv>[#FUNCTION]`.
    Conformance {
        /// CSV file under `conformance/`.
        csv: String,
        /// One function, or the whole file.
        function: Option<String>,
    },
    /// `fidelity:<format>:<metric>>=<value>` → `docs/qa/fidelity.md`.
    Fidelity {
        /// docx, xlsx, pptx…
        format: String,
        /// structural, text, visual, values, formulas, oracle_opens, byte_identical_untouched.
        metric: String,
        /// Minimum value.
        min: f64,
    },
    /// `manual:<docs/qa/...md#anchor>` (niche rows only; never counts as verification).
    Manual {
        /// Document path.
        doc: String,
        /// Heading anchor.
        anchor: String,
    },
}

impl VerifyId {
    /// Parses an id (the schema has already checked its shape).
    pub fn parse(id: &str) -> Result<VerifyId, String> {
        let bad = || format!("malformed verify id '{id}'");
        let (kind, rest) = id.split_once(':').ok_or_else(bad)?;
        match kind {
            "e2e" => {
                let (spec, slug) = rest.split_once(':').ok_or_else(bad)?;
                Ok(VerifyId::E2e {
                    spec: spec.to_owned(),
                    slug: slug.to_owned(),
                })
            }
            "unit" => {
                let (target, path) = rest.split_once('/').ok_or_else(bad)?;
                Ok(VerifyId::Unit {
                    target: target.to_owned(),
                    path: path.trim_end_matches('/').to_owned(),
                })
            }
            "conformance" => {
                let (csv, function) = match rest.split_once('#') {
                    Some((c, f)) => (c, Some(f.to_owned())),
                    None => (rest, None),
                };
                Ok(VerifyId::Conformance {
                    csv: csv.to_owned(),
                    function,
                })
            }
            "fidelity" => {
                let (format, cond) = rest.split_once(':').ok_or_else(bad)?;
                let (metric, min) = cond.split_once(">=").ok_or_else(bad)?;
                Ok(VerifyId::Fidelity {
                    format: format.to_owned(),
                    metric: metric.to_owned(),
                    min: min.parse().map_err(|_| bad())?,
                })
            }
            "manual" => {
                let (doc, anchor) = rest.split_once('#').ok_or_else(bad)?;
                Ok(VerifyId::Manual {
                    doc: doc.to_owned(),
                    anchor: anchor.to_owned(),
                })
            }
            _ => Err(bad()),
        }
    }
}

/// How one verify id is checked.
#[derive(Debug, Clone, PartialEq)]
enum Plan {
    /// Run a command from the repository root; success (and, when given, output matching
    /// `expect`) verifies.
    Run {
        program: String,
        args: Vec<String>,
        expect: Option<&'static str>,
    },
    /// Verified (or failed) without running anything.
    Static(Result<String, String>),
    /// The target does not exist yet.
    Missing(String),
}

/// Slug of a test title: lower case, runs of other characters become one `-`.
pub fn slugify(title: &str) -> String {
    let mut out = String::new();
    let mut dash = false;
    for c in title.chars().flat_map(char::to_lowercase) {
        if c.is_ascii_alphanumeric() {
            if dash && !out.is_empty() {
                out.push('-');
            }
            out.push(c);
            dash = false;
        } else {
            dash = true;
        }
    }
    out
}

/// Test titles declared with `test(...)` in a Playwright spec, with whether they are skipped.
fn spec_titles(source: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    for quote in ['\'', '"', '`'] {
        let pattern = format!(
            r#"\btest(?:\.(only|skip|fixme|fail|slow))?\(\s*{q}((?:[^{q}\\]|\\.)*){q}"#,
            q = regex::escape(&quote.to_string())
        );
        let Ok(re) = Regex::new(&pattern) else {
            continue;
        };
        for cap in re.captures_iter(source) {
            let modifier = cap.get(1).map(|m| m.as_str());
            let title = cap.get(2).map_or("", |m| m.as_str()).replace("\\", "");
            out.push((title, matches!(modifier, Some("skip" | "fixme"))));
        }
    }
    out
}

/// A regular expression matching `text` with letters in either case (for Vitest's `-t`, which
/// is case-sensitive).
fn case_insensitive(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        if c.is_ascii_alphabetic() {
            let _ = write!(
                out,
                "[{}{}]",
                c.to_ascii_lowercase(),
                c.to_ascii_uppercase()
            );
        } else {
            out.push_str(&regex::escape(&c.to_string()));
        }
    }
    out
}

/// nextest filterset for a `unit:` path inside one crate.
fn nextest_filter(path: &str) -> String {
    let segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.replace('-', "_"))
        .collect();
    let (binary, rest) = match segments.split_first() {
        Some((first, rest)) if first == "tests" && !rest.is_empty() => {
            (Some(rest[0].clone()), rest[1..].to_vec())
        }
        _ => (None, segments),
    };
    let test = if rest.is_empty() {
        None
    } else {
        let parts: Vec<String> = rest.iter().map(|s| regex::escape(s)).collect();
        Some(format!("test(/{}/)", parts.join("::(?:.*::)?")))
    };
    match (binary, test) {
        (Some(b), Some(t)) => format!("binary(={b}) & {t}"),
        (Some(b), None) => format!("binary(={b})"),
        (None, Some(t)) => t,
        (None, None) => "all()".to_owned(),
    }
}

/// Reads one metric for one format from the fidelity report's Markdown tables.
///
/// Contract with `cargo xtask fidelity` (WP-60): a summary table whose header has a `Format`
/// column and one column per metric (`structural`, `text`, `visual`, `values`, `formulas`,
/// `oracle_opens`, `byte_identical_untouched`; spaces or underscores, any case), with one row
/// per format. Values are ratios (`0.97`), percentages (`97 %`) or yes/no. When several tables
/// match, the lowest value counts.
pub fn fidelity_metric(report: &str, format: &str, metric: &str) -> Option<f64> {
    let norm = |s: &str| {
        s.trim()
            .trim_matches(|c| c == '*' || c == '`' || c == '_')
            .to_ascii_lowercase()
            .replace([' ', '-'], "_")
    };
    let lines: Vec<&str> = report.lines().collect();
    let mut best: Option<f64> = None;
    let cells = |line: &str| -> Vec<String> {
        line.trim()
            .trim_matches('|')
            .split('|')
            .map(|c| c.trim().to_owned())
            .collect()
    };
    let mut i = 0;
    while i + 1 < lines.len() {
        let header = lines[i].trim();
        let sep = lines[i + 1].trim();
        let is_sep =
            sep.starts_with('|') && sep.chars().all(|c| matches!(c, '|' | '-' | ':' | ' '));
        if !(header.starts_with('|') && is_sep) {
            i += 1;
            continue;
        }
        let head: Vec<String> = cells(header).iter().map(|c| norm(c)).collect();
        let format_col = head.iter().position(|c| c == "format");
        let metric_col = head.iter().position(|c| *c == norm(metric));
        let mut j = i + 2;
        while j < lines.len() && lines[j].trim().starts_with('|') {
            if let (Some(fc), Some(mc)) = (format_col, metric_col) {
                let row = cells(lines[j]);
                if row.get(fc).is_some_and(|f| norm(f) == norm(format))
                    && let Some(v) = row.get(mc).and_then(|c| parse_metric(c))
                {
                    best = Some(best.map_or(v, |b: f64| b.min(v)));
                }
            }
            j += 1;
        }
        i = j;
    }
    best
}

fn parse_metric(text: &str) -> Option<f64> {
    let t = text
        .trim()
        .trim_matches(|c| c == '*' || c == '`')
        .trim()
        .to_ascii_lowercase();
    match t.as_str() {
        "yes" | "true" | "✓" | "pass" => return Some(1.0),
        "no" | "false" | "✗" | "fail" => return Some(0.0),
        _ => {}
    }
    let first = t.split_whitespace().next()?;
    if let Some(p) = first.strip_suffix('%') {
        return p.parse::<f64>().ok().map(|x| x / 100.0);
    }
    let x: f64 = first.parse().ok()?;
    if t.contains('%') {
        Some(x / 100.0)
    } else {
        Some(x)
    }
}

/// Rows of a conformance CSV that call `function` and are not excluded by the oracle list.
fn conformance_rows(csv: &str, exclusions: &BTreeSet<String>, function: &str) -> usize {
    let call = Regex::new(&format!(
        r"(?i)(^|[^A-Za-z0-9_.]){}\(",
        regex::escape(function)
    ))
    .expect("escaped regex");
    csv.lines()
        .skip(1)
        .filter_map(first_csv_field)
        .filter(|formula| !exclusions.contains(formula))
        .filter(|formula| call.is_match(formula))
        .count()
}

fn first_csv_field(line: &str) -> Option<String> {
    let line = line.trim_end_matches('\r');
    if line.trim().is_empty() {
        return None;
    }
    if let Some(rest) = line.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    out.push('"');
                    chars.next();
                } else {
                    return Some(out);
                }
            } else {
                out.push(c);
            }
        }
        None
    } else {
        Some(line.split(',').next().unwrap_or_default().to_owned())
    }
}

/// Resolves verify ids to plans, memoising shared runs (one conformance run for all rows).
struct Resolver<'a> {
    root: &'a Path,
    tracker: &'a Tracker,
    fidelity: Option<String>,
    exclusions: BTreeSet<String>,
}

impl<'a> Resolver<'a> {
    fn new(root: &'a Path, tracker: &'a Tracker) -> Resolver<'a> {
        let exclusions = std::fs::read_to_string(root.join("conformance/oracle-exclusions.csv"))
            .map(|t| t.lines().skip(1).filter_map(first_csv_field).collect())
            .unwrap_or_default();
        Resolver {
            root,
            tracker,
            fidelity: std::fs::read_to_string(root.join(FIDELITY_REPORT)).ok(),
            exclusions,
        }
    }

    fn plan(&self, id: &VerifyId) -> Plan {
        match id {
            VerifyId::E2e { spec, slug } => self.plan_e2e(spec, slug),
            VerifyId::Unit { target, path } => self.plan_unit(target, path),
            VerifyId::Conformance { csv, function } => {
                self.plan_conformance(csv, function.as_deref())
            }
            VerifyId::Fidelity {
                format,
                metric,
                min,
            } => self.plan_fidelity(format, metric, *min),
            VerifyId::Manual { doc, .. } => Plan::Static(Err(format!(
                "manual check {doc} does not count as verification"
            ))),
        }
    }

    fn plan_e2e(&self, spec: &str, slug: &str) -> Plan {
        let rel = format!("e2e/specs/{spec}.spec.ts");
        let Ok(source) = std::fs::read_to_string(self.root.join(&rel)) else {
            return Plan::Missing(format!("{rel} does not exist"));
        };
        let titles = spec_titles(&source);
        let Some((title, skipped)) = titles.iter().find(|(t, _)| slugify(t) == slug) else {
            return Plan::Missing(format!("{rel} has no test titled like '{slug}'"));
        };
        if *skipped {
            return Plan::Static(Err(format!("{rel}: test '{title}' is skipped")));
        }
        Plan::Run {
            expect: None,
            program: "pnpm".into(),
            args: util::args(&[
                "--filter",
                "@kiln/e2e",
                "exec",
                "playwright",
                "test",
                &format!("specs/{spec}.spec.ts"),
                "--grep",
                &regex::escape(title),
            ]),
        }
    }

    fn plan_unit(&self, target: &str, path: &str) -> Plan {
        let is_crate = target == "xtask"
            || self
                .root
                .join("crates")
                .join(target)
                .join("Cargo.toml")
                .is_file();
        if is_crate {
            return Plan::Run {
                expect: None,
                program: util::cargo(),
                args: util::args(&[
                    "nextest",
                    "run",
                    "-p",
                    target,
                    "--no-tests=fail",
                    "-E",
                    &nextest_filter(path),
                ]),
            };
        }
        let package = ["packages", "apps"]
            .iter()
            .map(|d| self.root.join(d).join(target).join("package.json"))
            .find(|p| p.is_file());
        let Some(package) = package else {
            return Plan::Missing(format!("no crate or package named {target}"));
        };
        let name = read_json(&package)
            .ok()
            .and_then(|v| v["name"].as_str().map(str::to_owned))
            .unwrap_or_else(|| format!("@kiln/{target}"));
        let pattern: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| {
                s.split(['-', '_'])
                    .map(case_insensitive)
                    .collect::<Vec<_>>()
                    .join("[-_ ]")
            })
            .collect();
        Plan::Run {
            // Vitest exits 0 when -t matches nothing; demand at least one passed test.
            expect: Some(r"Tests\s+[1-9][0-9]* passed"),
            program: "pnpm".into(),
            args: util::args(&[
                "--filter",
                &name,
                "exec",
                "vitest",
                "run",
                "-t",
                &pattern.join(".*"),
            ]),
        }
    }

    fn plan_conformance(&self, csv: &str, function: Option<&str>) -> Plan {
        let rel = format!("conformance/{csv}");
        let Ok(text) = std::fs::read_to_string(self.root.join(&rel)) else {
            return Plan::Missing(format!("{rel} does not exist"));
        };
        if !self
            .root
            .join("crates/kiln-formula/tests/conformance.rs")
            .is_file()
        {
            return Plan::Missing(
                "the conformance runner crates/kiln-formula/tests/conformance.rs does not exist"
                    .into(),
            );
        }
        if let Some(f) = function {
            match &self.tracker.implemented {
                Some(set) if set.contains(f) => {}
                Some(_) => return Plan::Missing(format!("{f} is not implemented ({IMPLEMENTED})")),
                None => return Plan::Missing(format!("{IMPLEMENTED} does not exist")),
            }
            if conformance_rows(&text, &self.exclusions, f) == 0 {
                return Plan::Missing(format!("{rel} has no rows calling {f}"));
            }
        } else if text.lines().skip(1).all(|l| l.trim().is_empty()) {
            return Plan::Missing(format!("{rel} has no rows"));
        }
        // The runner reports 0 failures over every CSV, so one run verifies all rows.
        Plan::Run {
            expect: None,
            program: util::cargo(),
            args: util::args(&[
                "nextest",
                "run",
                "-p",
                "kiln-formula",
                "--test",
                "conformance",
                "--no-tests=fail",
            ]),
        }
    }

    fn plan_fidelity(&self, format: &str, metric: &str, min: f64) -> Plan {
        let Some(report) = &self.fidelity else {
            return Plan::Missing(format!("{FIDELITY_REPORT} does not exist"));
        };
        match fidelity_metric(report, format, metric) {
            None => Plan::Missing(format!("{FIDELITY_REPORT} reports no {format} {metric}")),
            Some(v) if v + 1e-9 >= min => Plan::Static(Ok(format!("{format} {metric} = {v}"))),
            Some(v) => Plan::Static(Err(format!("{format} {metric} = {v} < {min}"))),
        }
    }
}

/// Runs the verification of every selected done, better and partial row.
///
/// A done or better row passes when every automated verify id resolves to an existing target
/// and passes. A partial row passes when at least one of its automated ids does (the delivered
/// part is proven; the rest may not exist yet). Each distinct command runs once.
fn verify(
    root: &Path,
    tracker: &Tracker,
    dry_run: bool,
    apps: &[String],
    only: &[String],
) -> Result<(), String> {
    for id in only {
        if !tracker.rows.iter().any(|r| &r.id == id) {
            return Err(format!("unknown row {id}"));
        }
    }
    let selected: Vec<&Row> = tracker
        .rows
        .iter()
        .filter(|r| apps.is_empty() || apps.contains(&r.app))
        .filter(|r| only.is_empty() || only.contains(&r.id))
        .filter(|r| matches!(r.status.as_str(), "done" | "better" | "partial"))
        .collect();
    let resolver = Resolver::new(root, tracker);
    let mut failures = Vec::new();
    let mut row_plans: Vec<(&Row, Vec<(String, Plan)>)> = Vec::new();
    for row in &selected {
        let mut plans = Vec::new();
        for id in &row.verify {
            match VerifyId::parse(id) {
                Ok(VerifyId::Manual { .. }) => {}
                Ok(v) => plans.push((id.clone(), resolver.plan(&v))),
                Err(e) => failures.push(format!("{}: {e}", row.id)),
            }
        }
        if plans.is_empty() {
            failures.push(format!(
                "{} is {} but names no automated verify id",
                row.id, row.status
            ));
        } else {
            row_plans.push((row, plans));
        }
    }

    let mut commands: Vec<(String, &Plan)> = Vec::new();
    for (_, plans) in &row_plans {
        for (_, plan) in plans {
            if let Plan::Run { program, args, .. } = plan {
                let key = command_line(program, args);
                if !commands.iter().any(|(k, _)| *k == key) {
                    commands.push((key, plan));
                }
            }
        }
    }
    let count = |status: &[&str]| {
        row_plans
            .iter()
            .filter(|(r, _)| status.contains(&r.status.as_str()))
            .count()
    };
    if row_plans.is_empty() {
        println!("parity --verify: no done, better or partial rows to verify");
    } else {
        println!(
            "parity --verify: {} done/better and {} partial row(s), {} command(s) to run",
            count(&["done", "better"]),
            count(&["partial"]),
            commands.len()
        );
    }
    let mut results: BTreeMap<String, Result<(), String>> = BTreeMap::new();
    for (key, plan) in &commands {
        if dry_run {
            println!("would run: {key}");
            continue;
        }
        let Plan::Run {
            program,
            args,
            expect,
        } = plan
        else {
            continue;
        };
        let result = run_captured(root, program, args, *expect);
        match &result {
            Ok(()) => println!("ok: {key}"),
            Err(e) => println!("FAILED: {key}\n{e}"),
        }
        results.insert(key.clone(), result);
    }
    for (row, plans) in &row_plans {
        let outcomes: Vec<(&String, Result<(), String>)> = plans
            .iter()
            .map(|(id, plan)| {
                let outcome = match plan {
                    Plan::Missing(why) => Err(format!("verification missing: {why}")),
                    Plan::Static(r) => r.clone().map(|_| ()),
                    Plan::Run { .. } if dry_run => Ok(()),
                    Plan::Run { program, args, .. } => results
                        .get(&command_line(program, args))
                        .cloned()
                        .unwrap_or_else(|| Err("not run".into()))
                        .map_err(|_| "its tests failed".to_owned()),
                };
                (id, outcome)
            })
            .collect();
        if row.status == "partial" {
            if !outcomes.iter().any(|(_, o)| o.is_ok()) {
                let why: Vec<String> = outcomes
                    .iter()
                    .filter_map(|(id, o)| o.as_ref().err().map(|e| format!("{id}: {e}")))
                    .collect();
                failures.push(format!(
                    "{} is partial but no verify id passes ({}); fix it or set it to missing",
                    row.id,
                    why.join("; ")
                ));
            }
        } else {
            for (id, outcome) in outcomes {
                if let Err(e) = outcome {
                    failures.push(format!("{} ({}) {id}: {e}", row.id, row.status));
                }
            }
        }
    }
    if failures.is_empty() {
        println!("parity --verify: every done, better and partial row is verified");
        Ok(())
    } else {
        Err(format!(
            "{} verification problem(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        ))
    }
}

fn command_line(program: &str, args: &[String]) -> String {
    let program = if program.ends_with("cargo") {
        "cargo"
    } else {
        program
    };
    let quoted: Vec<String> = args
        .iter()
        .map(|a| {
            if a.chars()
                .any(|c| c.is_whitespace() || "()|&[]*?$".contains(c))
            {
                format!("'{a}'")
            } else {
                a.clone()
            }
        })
        .collect();
    format!("{program} {}", quoted.join(" "))
}

fn run_captured(
    root: &Path,
    program: &str,
    args: &[String],
    expect: Option<&str>,
) -> Result<(), String> {
    let out = Command::new(program)
        .args(args)
        .current_dir(root)
        .env("NO_COLOR", "1")
        .output()
        .map_err(|e| format!("could not start {program}: {e}"))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    let ansi = Regex::new(r"\x1b\[[0-9;]*[A-Za-z]").expect("static regex");
    let text = ansi.replace_all(&text, "");
    let tail = || {
        let lines: Vec<&str> = text.lines().collect();
        lines[lines.len().saturating_sub(30)..].join("\n  ")
    };
    if !out.status.success() {
        return Err(format!("  {}\n  ({})", tail(), out.status));
    }
    if let Some(pattern) = expect {
        let re = Regex::new(pattern).map_err(|e| e.to_string())?;
        if !re.is_match(&text) {
            return Err(format!("  {}\n  (no test matched)", tail()));
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn schema() -> Value {
        read_json(&util::repo_root().join(SCHEMA)).expect("schema")
    }

    fn row(over: Value) -> Value {
        let mut base = json!({
            "id": "write.home.font.bold", "app": "write", "area": "Home ▸ Font",
            "feature": "Bold", "office": "Word ▸ Bold", "tier": "critical", "priority": "P0",
            "status": "missing", "wp": "WP-64", "verify": ["e2e:write-editing:bold"], "notes": ""
        });
        for (k, v) in over.as_object().expect("object") {
            base[k] = v.clone();
        }
        base
    }

    fn row_errors(r: Value) -> Vec<String> {
        let s = schema();
        let mut v = Validator::new(&s).expect("validator");
        v.validate_def("row", &r).expect("def")
    }

    #[test]
    fn options_parse() {
        assert_eq!(parse(&[]), Ok(Mode::Generate));
        assert_eq!(parse(&util::args(&["--check"])), Ok(Mode::Check));
        assert_eq!(parse(&util::args(&["--ci"])), Ok(Mode::Check));
        assert_eq!(
            parse(&util::args(&[
                "--verify",
                "--dry-run",
                "--app",
                "grid",
                "--row",
                "grid.x"
            ])),
            Ok(Mode::Verify {
                dry_run: true,
                apps: vec!["grid".into()],
                rows: vec!["grid.x".into()]
            })
        );
        assert_eq!(
            parse(&util::args(&["--snapshot", "v0.2.0"])),
            Ok(Mode::Snapshot("v0.2.0".into()))
        );
        assert!(parse(&util::args(&["--check", "--verify"])).is_err());
        assert!(parse(&util::args(&["--dry-run"])).is_err());
        assert!(parse(&util::args(&["--verify", "--app", "outlook"])).is_err());
        assert!(parse(&util::args(&["--snapshot"])).is_err());
        assert!(parse(&util::args(&["--frobnicate"])).is_err());
    }

    #[test]
    fn a_plain_row_is_valid() {
        assert_eq!(row_errors(row(json!({}))), Vec::<String>::new());
    }

    #[test]
    fn schema_rules_reject_inconsistent_rows() {
        // critical rows are P0.
        assert!(!row_errors(row(json!({"priority": "P1"}))).is_empty());
        // niche rows are P2 or out.
        assert!(!row_errors(row(json!({"tier": "niche"}))).is_empty());
        // done needs an automated verify id.
        let e = row_errors(row(json!({"status": "done", "verify": []})));
        assert!(e.iter().any(|m| m.contains("automated")), "{e:?}");
        // better needs a note; "—" rows cannot be done.
        assert!(!row_errors(row(json!({"status": "better"}))).is_empty());
        let e = row_errors(row(
            json!({"office": "—", "tier": "commonly-missed", "status": "done", "notes": "Better than Office: yes"}),
        ));
        assert!(e.iter().any(|m| m.contains("only be better")), "{e:?}");
        // out rows are missing and have no WP.
        assert!(!row_errors(row(json!({"tier": "niche", "priority": "out"}))).is_empty());
        // manual ids only on niche rows.
        assert!(!row_errors(row(json!({"verify": ["manual:docs/qa/x.md#y"]}))).is_empty());
        // bad verify ids, unknown properties, bad enums.
        assert!(!row_errors(row(json!({"verify": ["playwright:foo"]}))).is_empty());
        assert!(!row_errors(row(json!({"colour": "red"}))).is_empty());
        assert!(!row_errors(row(json!({"status": "finished"}))).is_empty());
        assert!(!row_errors(row(json!({"wp": "WP-x"}))).is_empty());
    }

    #[test]
    fn verify_id_forms_are_accepted() {
        for id in [
            "e2e:write-features:format-painter",
            "unit:kiln-doc/styles/builtin",
            "unit:commands/palette",
            "unit:kiln-formula/tests/conformance",
            "conformance:curated-lookup.csv#XLOOKUP",
            "conformance:generated-date.csv#NETWORKDAYS.INTL",
            "conformance:generated-math.csv",
            "fidelity:docx:visual>=0.90",
            "fidelity:xlsx:byte_identical_untouched>=1.0",
        ] {
            let e = row_errors(row(json!({"verify": [id]})));
            assert!(e.is_empty(), "{id}: {e:?}");
            assert!(VerifyId::parse(id).is_ok());
        }
        assert_eq!(
            VerifyId::parse("fidelity:pptx:visual>=0.85"),
            Ok(VerifyId::Fidelity {
                format: "pptx".into(),
                metric: "visual".into(),
                min: 0.85
            })
        );
        assert_eq!(
            VerifyId::parse("conformance:a.csv#SUM"),
            Ok(VerifyId::Conformance {
                csv: "a.csv".into(),
                function: Some("SUM".into())
            })
        );
    }

    #[test]
    fn validator_supports_its_keywords_and_rejects_others() {
        let s = json!({
            "type": "object",
            "properties": {
                "n": {"type": "integer", "minimum": 1, "exclusiveMaximum": 3},
                "a": {"type": "array", "minItems": 1, "uniqueItems": true, "contains": {"const": 2}},
                "o": {"oneOf": [{"type": "string"}, {"type": "number"}]},
                "m": {"type": "object", "propertyNames": {"pattern": "^x"}, "additionalProperties": {"type": "boolean"}},
                "k": {"not": {"enum": [1, 2]}}
            }
        });
        let mut v = Validator::new(&s).expect("validator");
        assert!(
            v.validate_root(&json!({"n": 2, "a": [1, 2], "o": "s", "m": {"xa": true}, "k": 3}))
                .is_empty()
        );
        assert_eq!(v.validate_root(&json!({"n": 3})).len(), 1);
        assert_eq!(v.validate_root(&json!({"n": 1.5})).len(), 1);
        assert_eq!(v.validate_root(&json!({"a": [1, 1]})).len(), 2);
        assert_eq!(v.validate_root(&json!({"o": true})).len(), 1);
        assert_eq!(v.validate_root(&json!({"m": {"y": 1}})).len(), 2);
        assert_eq!(v.validate_root(&json!({"k": 2})).len(), 1);
        let bad = json!({"type": "string", "format": "email"});
        let mut v = Validator::new(&bad).expect("validator");
        assert!(v.validate_root(&json!("a"))[0].contains("unsupported keyword"));
        assert!(Validator::new(&json!({"pattern": "("})).is_err());
    }

    #[test]
    fn slugs_and_spec_titles() {
        assert_eq!(
            slugify("Format Painter (double-click)"),
            "format-painter-double-click"
        );
        assert_eq!(slugify("  F4 toggles $ refs "), "f4-toggles-refs");
        let src = "test('bold works', async () => {});\n test.skip(\"later one\", () => {});\n test.describe('group', () => {});\n test(`tpl title`, x)";
        let titles = spec_titles(src);
        assert!(titles.contains(&("bold works".into(), false)));
        assert!(titles.contains(&("later one".into(), true)));
        assert!(titles.contains(&("tpl title".into(), false)));
        assert!(!titles.iter().any(|(t, _)| t == "group"));
    }

    #[test]
    fn filters_for_test_runners() {
        assert_eq!(nextest_filter("tests/conformance"), "binary(=conformance)");
        assert_eq!(
            nextest_filter("tests/docs/every-function"),
            "binary(=docs) & test(/every_function/)"
        );
        assert_eq!(
            nextest_filter("write/format-painter"),
            "test(/write::(?:.*::)?format_painter/)"
        );
        assert_eq!(case_insensitive("a-b"), "[aA]\\-[bB]");
    }

    #[test]
    fn fidelity_report_is_read_conservatively() {
        let report = "# Fidelity\n\n| Format | Files | Structural | Visual | Oracle opens |\n|---|---:|---:|---:|---|\n| **docx** | 19 | 0.97 | 92 % | yes |\n| xlsx | 20 | 0.99 | — | no |\n\n| Format | Visual |\n|---|---|\n| docx | 0.91 |\n";
        assert_eq!(fidelity_metric(report, "docx", "structural"), Some(0.97));
        assert_eq!(fidelity_metric(report, "docx", "visual"), Some(0.91));
        assert_eq!(fidelity_metric(report, "docx", "oracle_opens"), Some(1.0));
        assert_eq!(fidelity_metric(report, "xlsx", "oracle_opens"), Some(0.0));
        assert_eq!(fidelity_metric(report, "xlsx", "visual"), None);
        assert_eq!(fidelity_metric(report, "pptx", "visual"), None);
    }

    #[test]
    fn conformance_rows_honour_exclusions_and_word_boundaries() {
        let csv = "formula,expected,type,notes\n\"=SUM(1,2)\",3,number,\n=XSUM(1),1,number,\n=LN(0),x,error,\n\"=IF(SUM(1)=1,\"\"a\"\",\"\"b\"\")\",a,text,\n";
        let excl: BTreeSet<String> = ["=LN(0)".to_owned()].into();
        assert_eq!(conformance_rows(csv, &excl, "SUM"), 2);
        assert_eq!(conformance_rows(csv, &excl, "LN"), 0);
        assert_eq!(
            first_csv_field("\"=A(\"\"x\"\")\",1"),
            Some("=A(\"x\")".into())
        );
    }

    #[test]
    fn implemented_list_forms() {
        let a = parse_implemented(&json!({"SUM": "implemented", "XLOOKUP": "planned", "IF": true}))
            .expect("ok");
        assert_eq!(a, ["IF".to_owned(), "SUM".to_owned()].into());
        let b = parse_implemented(&json!({"functions": ["sum", {"name": "IF"}]})).expect("ok");
        assert_eq!(b, ["IF".to_owned(), "SUM".to_owned()].into());
        assert!(parse_implemented(&json!({"SUM": "maybe"})).is_err());
        assert!(parse_implemented(&json!(3)).is_err());
    }

    #[test]
    fn scores_and_formatting() {
        let t = Tally {
            weight: 7.0,
            earned: 5.0,
            ..Tally::default()
        };
        assert!((t.score() - 5.0 / 7.0).abs() < 1e-12);
        assert_eq!(Tally::default().score(), 0.0);
        assert_eq!(percent(0.71428), "71.4 %");
        assert_eq!(percent_whole(0.999), "99 %");
        assert_eq!(cell("a|b"), "a\\|b");
        assert!(version_key("v0.10.0") > version_key("v0.9.3"));
        assert_eq!(glyph("done", "out"), "–");
        assert_eq!(glyph("better", "P0"), "★");
    }

    /// The committed tracker is valid, meets the WP-59 seed coverage, and its generated files
    /// are current (the same check CI runs).
    #[test]
    fn committed_tracker_is_valid_and_current() {
        let root = util::repo_root();
        let tracker = Tracker::load(&root).expect("tracker valid");
        let count = |app: &str| tracker.rows.iter().filter(|r| r.app == app).count();
        assert!(tracker.rows.len() >= 530);
        assert!(count("write") >= 160 && count("grid") >= 180 && count("deck") >= 120);
        assert!(count("notes") >= 30 && count("suite") >= 40);
        assert!(tracker.rows.iter().any(|r| r.priority == "out"));
        assert!(tracker.functions.len() >= 520);
        for (rel, text) in tracker.outputs().expect("outputs") {
            let on_disk = std::fs::read_to_string(root.join(&rel)).unwrap_or_default();
            assert!(on_disk == text, "{rel} is stale: run `cargo xtask parity`");
        }
    }

    #[test]
    fn scoring_uses_the_documented_formula() {
        let mut t = Tracker {
            updated: "2026-10-16".into(),
            weights: Weights::from_json(&json!({
                "tier": {"critical": 3, "commonly-missed": 2, "niche": 0.5},
                "status": {"missing": 0, "partial": 0.5, "done": 1, "better": 1}
            })),
            rows: Vec::new(),
            functions: Vec::new(),
            implemented: None,
            corpus: Vec::new(),
            snapshot: None,
        };
        let mk = |id: &str, tier: &str, prio: &str, status: &str| {
            Row::from_json(&row(
                json!({"id": id, "tier": tier, "priority": prio, "status": status}),
            ))
        };
        t.rows = vec![
            mk("write.a", "critical", "P0", "done"),
            mk("write.b", "commonly-missed", "P1", "partial"),
            mk("write.c", "niche", "P2", "missing"),
            mk("write.d", "niche", "out", "missing"),
        ];
        let all = t.tally(|_| true);
        // (3·1 + 2·0.5 + 0.5·0) / (3 + 2 + 0.5)
        assert!((all.score() - 4.0 / 5.5).abs() < 1e-12);
        assert_eq!(
            (all.rows, all.out, all.done, all.partial, all.missing),
            (3, 1, 1, 1, 1)
        );
        // Flips since the seed, then since a snapshot.
        assert_eq!(t.flips().len(), 2);
        t.snapshot = Some(Snapshot {
            tag: "v0.1.0".into(),
            date: "2026-10-16".into(),
            score: 0.5,
            statuses: [
                ("write.a".to_owned(), "partial".to_owned()),
                ("write.z".to_owned(), "done".to_owned()),
            ]
            .into(),
        });
        let flips = t.flips();
        let ids: Vec<&str> = flips.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["write.a", "write.b", "write.c", "write.d", "write.z"]
        );
        assert!(t.report().contains("## Status flips since v0.1.0"));
        assert!(t.badge().contains("\"message\": \"72 %\""));
        let snap: Value = serde_json::from_str(&t.snapshot_json("v0.2.0")).expect("json");
        let s = schema();
        let mut v = Validator::new(&s).expect("validator");
        assert!(
            v.validate_def("snapshotFile", &snap)
                .expect("def")
                .is_empty()
        );
    }
}
