// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask protocol [--check]`: generates the TypeScript side of `kiln-protocol`
//! (PLAN.md §3.5).
//!
//! Writes, from the Rust types:
//!
//! * `packages/protocol/src/generated/*.ts` — ts-rs declarations and `index.ts`;
//! * `packages/protocol/src/generated/catalog.json` — every `CommandMeta`;
//! * `packages/protocol/src/generated/schema.json` — the serde data model for the postcard decoder;
//! * `crates/kiln-protocol/tests/vectors/<Type>.bin` + `.json` — postcard test vectors.
//!
//! With `--check` nothing is written: the command lists every file that differs from (or is
//! missing in, or is stale in) the committed tree and exits 1 if there is any drift.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::util;

/// The generated TypeScript directory, relative to the repository root.
pub const GENERATED_DIR: &str = "packages/protocol/src/generated";
/// The postcard vectors directory, relative to the repository root.
pub const VECTORS_DIR: &str = "crates/kiln-protocol/tests/vectors";

/// One output directory and the exact set of files it must contain.
struct Output {
    dir: PathBuf,
    /// File extensions this generator owns in `dir`; other files are left alone.
    owned_extensions: &'static [&'static str],
    files: BTreeMap<String, Vec<u8>>,
}

fn outputs(root: &Path) -> Result<Vec<Output>, String> {
    use kiln_protocol::schema;
    let mut generated: BTreeMap<String, Vec<u8>> = schema::typescript_files()?
        .into_iter()
        .map(|(name, text)| (name, text.into_bytes()))
        .collect();
    generated.insert("catalog.json".into(), schema::catalog_json()?.into_bytes());
    generated.insert("schema.json".into(), schema::schema_json()?.into_bytes());

    let mut vectors = BTreeMap::new();
    for v in schema::vectors()? {
        vectors.insert(format!("{}.bin", v.name), v.postcard);
        vectors.insert(format!("{}.json", v.name), v.json.into_bytes());
    }
    Ok(vec![
        Output {
            dir: root.join(GENERATED_DIR),
            owned_extensions: &["ts", "json"],
            files: generated,
        },
        Output {
            dir: root.join(VECTORS_DIR),
            owned_extensions: &["bin", "json"],
            files: vectors,
        },
    ])
}

/// Files in `dir` with one of the owned extensions.
fn existing(dir: &Path, owned: &[&str]) -> Result<Vec<String>, String> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let ext = Path::new(&name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        if entry.path().is_file() && owned.contains(&ext) {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// A difference between the generated and the committed files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Drift {
    /// A file that should exist does not.
    Missing(PathBuf),
    /// A file differs.
    Changed(PathBuf),
    /// A file exists that the generator no longer produces.
    Stale(PathBuf),
}

fn diff(out: &Output) -> Result<Vec<Drift>, String> {
    let mut drift = Vec::new();
    for (name, bytes) in &out.files {
        let path = out.dir.join(name);
        match std::fs::read(&path) {
            Ok(current) if current == *bytes => {}
            Ok(_) => drift.push(Drift::Changed(path)),
            Err(_) => drift.push(Drift::Missing(path)),
        }
    }
    for name in existing(&out.dir, out.owned_extensions)? {
        if !out.files.contains_key(&name) {
            drift.push(Drift::Stale(out.dir.join(name)));
        }
    }
    Ok(drift)
}

/// Runs `cargo xtask protocol [--check]`.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut check = false;
    for arg in args {
        match arg.as_str() {
            "--check" => check = true,
            "--ci" => {}
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    let root = util::repo_root();
    let outputs = outputs(&root)?;
    let mut drift = Vec::new();
    for out in &outputs {
        drift.extend(diff(out)?);
    }
    let rel = |p: &Path| p.strip_prefix(&root).unwrap_or(p).display().to_string();
    if check {
        if drift.is_empty() {
            let count: usize = outputs.iter().map(|o| o.files.len()).sum();
            println!("protocol: {count} generated files are up to date");
            return Ok(());
        }
        for d in &drift {
            match d {
                Drift::Missing(p) => eprintln!("  missing  {}", rel(p)),
                Drift::Changed(p) => eprintln!("  changed  {}", rel(p)),
                Drift::Stale(p) => eprintln!("  stale    {}", rel(p)),
            }
        }
        return Err(format!(
            "{} generated file(s) differ from kiln-protocol; run `cargo xtask protocol` and commit the result",
            drift.len()
        ));
    }
    for out in &outputs {
        std::fs::create_dir_all(&out.dir).map_err(|e| format!("{}: {e}", out.dir.display()))?;
    }
    for d in &drift {
        if let Drift::Stale(p) = d {
            std::fs::remove_file(p).map_err(|e| format!("{}: {e}", p.display()))?;
        }
    }
    for out in &outputs {
        for (name, bytes) in &out.files {
            let path = out.dir.join(name);
            if !matches!(std::fs::read(&path), Ok(ref cur) if cur == bytes) {
                std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
            }
        }
    }
    let count: usize = outputs.iter().map(|o| o.files.len()).sum();
    println!(
        "protocol: {count} files generated ({} updated) in {GENERATED_DIR} and {VECTORS_DIR}",
        drift.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_reports_missing_changed_and_stale_files() {
        let dir = std::env::temp_dir().join(format!("kiln-xtask-protocol-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Same.ts"), "a").unwrap();
        std::fs::write(dir.join("Changed.ts"), "old").unwrap();
        std::fs::write(dir.join("Old.ts"), "x").unwrap();
        std::fs::write(dir.join("README.md"), "not ours").unwrap();
        let out = Output {
            dir: dir.clone(),
            owned_extensions: &["ts"],
            files: BTreeMap::from([
                ("Same.ts".to_owned(), b"a".to_vec()),
                ("Changed.ts".to_owned(), b"new".to_vec()),
                ("New.ts".to_owned(), b"n".to_vec()),
            ]),
        };
        let mut drift = diff(&out).unwrap();
        drift.sort_by_key(|d| format!("{d:?}"));
        assert_eq!(
            drift,
            vec![
                Drift::Changed(dir.join("Changed.ts")),
                Drift::Missing(dir.join("New.ts")),
                Drift::Stale(dir.join("Old.ts")),
            ]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn rejects_unknown_options() {
        assert!(run(&util::args(&["--frobnicate"])).is_err());
    }
}
