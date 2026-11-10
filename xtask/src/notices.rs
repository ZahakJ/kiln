// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask notices [--check]`: regenerates `THIRD-PARTY-NOTICES.md`.
//!
//! The file lists every third-party component that ships: npm production packages and bundled
//! assets (from `node tools/check-licenses.mjs --notices`) and the Rust crates linked into the
//! shipped binaries (normal dependencies of every workspace member except `xtask`, from
//! `cargo metadata`), each with its licence text when the package carries one.
//! With `--check` nothing is written; the command fails when the committed file is stale.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::util;

const FILE: &str = "THIRD-PARTY-NOTICES.md";

/// One Rust crate in the notices.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Crate {
    /// Crate name.
    pub name: String,
    /// Version.
    pub version: String,
    /// SPDX licence expression (or `see licence file`).
    pub license: String,
    /// Repository URL, if declared.
    pub repository: Option<String>,
    /// Directory holding the crate sources.
    pub dir: String,
}

/// The font files the binaries and the web build ship: those `kiln-text` embeds
/// (`crates/kiln-text/src/bundled.rs`, `font!("dir", "file")`, the desktop and Android builds'
/// only fonts: they fall back to the platform's own for CJK and emoji), and every font a glob of
/// `apps/web/src/worker.ts` takes in (`assets/fonts/{carlito,…}/*.ttf`, `noto/*.{ttf,otf}`,
/// `noto/cjk/*.otf`: the document fonts, the fallbacks, the CJK font's slices).
fn shipped_fonts(root: &Path) -> Result<BTreeSet<String>, String> {
    let mut out = BTreeSet::new();
    let bundled = fs::read_to_string(root.join("crates/kiln-text/src/bundled.rs"))
        .map_err(|e| format!("cannot read bundled.rs: {e}"))?;
    for line in bundled.lines() {
        let Some(rest) = line.trim().strip_prefix("font!(") else {
            continue;
        };
        if let Some(file) = rest.split('"').nth(3) {
            out.insert(file.to_owned());
        }
    }
    let worker = fs::read_to_string(root.join("apps/web/src/worker.ts"))
        .map_err(|e| format!("cannot read worker.ts: {e}"))?;
    let mut globs = 0;
    for glob in font_globs(&worker) {
        globs += 1;
        let (dirs, file) = glob.rsplit_once('/').unwrap_or(("", glob.as_str()));
        for dir in expand(dirs) {
            let Ok(rd) = fs::read_dir(root.join("assets/fonts").join(&dir)) else {
                return Err(format!(
                    "worker.ts registers fonts from a missing directory: {dir}"
                ));
            };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if expand(file).iter().any(|f| wildcard(f, &name)) {
                    out.insert(name);
                }
            }
        }
    }
    if globs == 0 {
        return Err("worker.ts names no font glob under assets/fonts/".into());
    }
    Ok(out)
}

/// Every `assets/fonts/…` glob in a source file's string literals, the part after `assets/fonts/`.
fn font_globs(source: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = source;
    while let Some(at) = rest.find("assets/fonts/") {
        let tail = &rest[at + "assets/fonts/".len()..];
        let end = tail.find(['\'', '"', '`']).unwrap_or(tail.len());
        let glob = &tail[..end];
        if glob.contains('*') {
            out.push(glob.to_owned());
        }
        rest = &tail[end..];
    }
    out
}

/// Brace expansion of one level: `{a,b}/*.ttf` gives `a/*.ttf` and `b/*.ttf`.
fn expand(glob: &str) -> Vec<String> {
    match (glob.find('{'), glob.find('}')) {
        (Some(open), Some(close)) if open < close => glob[open + 1..close]
            .split(',')
            .map(|alt| format!("{}{}{}", &glob[..open], alt.trim(), &glob[close + 1..]))
            .collect(),
        _ => vec![glob.to_owned()],
    }
}

/// Whether `name` matches `pattern`, where `*` stands for any run of characters.
fn wildcard(pattern: &str, name: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == name;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    if !name.starts_with(first) || !name[first.len()..].ends_with(last) {
        return false;
    }
    let mut at = first.len();
    let end = name.len() - last.len();
    for part in &parts[1..parts.len() - 1] {
        match name[at..end].find(part) {
            Some(i) => at += i + part.len(),
            None => return false,
        }
    }
    true
}

/// Fails when a shipped font has no entry in a `NOTICES.json` (its licence would be missing
/// from `THIRD-PARTY-NOTICES.md`).
fn fonts_have_notices(root: &Path) -> Result<(), String> {
    let mut noticed = BTreeSet::new();
    for pkg in fs::read_dir(root.join("packages"))
        .map_err(|e| e.to_string())?
        .flatten()
    {
        for dir in ["fonts", "assets"] {
            let file = pkg.path().join(dir).join("NOTICES.json");
            let Ok(text) = fs::read_to_string(&file) else {
                continue;
            };
            let v: Value =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", file.display()))?;
            for asset in v["assets"].as_array().into_iter().flatten() {
                for f in asset["files"].as_array().into_iter().flatten() {
                    if let Some(name) = f.as_str().and_then(|p| p.rsplit('/').next()) {
                        // A pattern (`NotoSansCJKsc-*.otf`) covers the files it matches.
                        noticed.insert(name.to_owned());
                    }
                }
            }
        }
    }
    let missing: Vec<String> = shipped_fonts(root)?
        .into_iter()
        .filter(|f| !noticed.iter().any(|n| wildcard(n, f)))
        .collect();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "fonts that ship without a notice (add them to a NOTICES.json): {}",
            missing.join(", ")
        ))
    }
}

/// Runs the command.
pub fn run(args: &[String]) -> Result<(), String> {
    let check = args.iter().any(|a| a == "--check" || a == "--ci");
    let root = util::repo_root();
    fonts_have_notices(&root)?;
    let npm = npm_notices(&root)?;
    let crates = shipped_crates(&root)?;
    let text = render(&npm, &crates);
    let path = root.join(FILE);
    if check {
        let current = fs::read_to_string(&path).unwrap_or_default();
        if current == text {
            println!(
                "notices: {FILE} is up to date ({} Rust crates)",
                crates.len()
            );
            Ok(())
        } else {
            Err(format!(
                "{FILE} is stale; run `cargo xtask notices` and commit the result"
            ))
        }
    } else {
        fs::write(&path, text).map_err(|e| format!("cannot write {FILE}: {e}"))?;
        println!("notices: wrote {FILE} ({} Rust crates)", crates.len());
        Ok(())
    }
}

fn npm_notices(root: &Path) -> Result<String, String> {
    let out = Command::new("node")
        .args(["tools/check-licenses.mjs", "--notices"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run node: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "node tools/check-licenses.mjs --notices failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    String::from_utf8(out.stdout).map_err(|e| format!("npm notices are not UTF-8: {e}"))
}

fn shipped_crates(root: &Path) -> Result<Vec<Crate>, String> {
    let out = Command::new(util::cargo())
        .args(["metadata", "--format-version", "1", "--locked"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let meta: Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("bad cargo metadata: {e}"))?;
    Ok(collect(&meta))
}

/// Walks the resolve graph from every workspace member except `xtask`, following only normal
/// (non-dev, non-build) dependencies, and returns the third-party packages reached.
pub fn collect(meta: &Value) -> Vec<Crate> {
    let empty = Vec::new();
    let packages = meta["packages"].as_array().unwrap_or(&empty);
    let by_id: BTreeMap<&str, &Value> = packages
        .iter()
        .filter_map(|p| Some((p["id"].as_str()?, p)))
        .collect();
    let members: BTreeSet<&str> = meta["workspace_members"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(Value::as_str)
        .collect();
    let nodes: BTreeMap<&str, &Value> = meta["resolve"]["nodes"]
        .as_array()
        .unwrap_or(&empty)
        .iter()
        .filter_map(|n| Some((n["id"].as_str()?, n)))
        .collect();

    let mut queue: VecDeque<&str> = members
        .iter()
        .copied()
        .filter(|id| by_id.get(id).and_then(|p| p["name"].as_str()) != Some("xtask"))
        .collect();
    let mut seen: BTreeSet<&str> = queue.iter().copied().collect();
    while let Some(id) = queue.pop_front() {
        let Some(node) = nodes.get(id) else { continue };
        for dep in node["deps"].as_array().unwrap_or(&empty) {
            let normal = dep["dep_kinds"]
                .as_array()
                .unwrap_or(&empty)
                .iter()
                .any(|k| k["kind"].is_null());
            if let (true, Some(dep_id)) = (normal, dep["pkg"].as_str())
                && seen.insert(dep_id)
            {
                queue.push_back(dep_id);
            }
        }
    }

    let mut crates: Vec<Crate> = seen
        .iter()
        .filter(|id| !members.contains(*id))
        .filter_map(|id| by_id.get(id))
        .map(|p| Crate {
            name: p["name"].as_str().unwrap_or_default().to_owned(),
            version: p["version"].as_str().unwrap_or_default().to_owned(),
            license: p["license"]
                .as_str()
                .unwrap_or("see licence file")
                .to_owned(),
            repository: p["repository"].as_str().map(str::to_owned),
            dir: Path::new(p["manifest_path"].as_str().unwrap_or_default())
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
        })
        .collect();
    crates.sort();
    crates.dedup_by(|a, b| a.name == b.name && a.version == b.version);
    crates
}

fn licence_texts(dir: &str) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<_> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                let n = n.to_ascii_lowercase();
                n.starts_with("license") || n.starts_with("licence") || n.starts_with("copying")
            })
        })
        .collect();
    files.sort();
    files
        .iter()
        .filter_map(|p| fs::read_to_string(p).ok())
        .map(|t| t.trim().to_owned())
        .collect()
}

/// Builds the whole notices file from the npm part and the Rust crates.
pub fn render(npm: &str, crates: &[Crate]) -> String {
    let mut out = String::new();
    out.push_str("# Third-party notices\n\n");
    out.push_str(
        "Kiln is licensed under the GNU Affero General Public License v3.0 or later. It includes the\n\
         third-party software listed below, each under its own licence. This file is generated by\n\
         `cargo xtask notices` (npm part: `node tools/check-licenses.mjs --notices`; Rust part:\n\
         `cargo metadata`); do not edit it by hand.\n\n",
    );
    out.push_str("## Rust crates\n\n| Crate | Version | Licence |\n|---|---|---|\n");
    for c in crates {
        out.push_str(&format!("| {} | {} | {} |\n", c.name, c.version, c.license));
    }
    out.push('\n');
    // Many crates ship byte-identical licence files (the Apache-2.0 text above all), so every
    // distinct text is printed once and each crate refers to the texts that apply to it.
    let mut texts: Vec<String> = Vec::new();
    let mut details = String::new();
    for c in crates {
        details.push_str(&format!(
            "### {} {}\n\n- Licence: {}\n",
            c.name, c.version, c.license
        ));
        if let Some(repo) = &c.repository {
            details.push_str(&format!("- Repository: {repo}\n"));
        }
        let refs: Vec<String> = licence_texts(&c.dir)
            .into_iter()
            .map(|text| {
                let index = texts.iter().position(|t| *t == text).unwrap_or_else(|| {
                    texts.push(text);
                    texts.len() - 1
                });
                format!("[text {}](#rust-licence-text-{})", index + 1, index + 1)
            })
            .collect();
        if !refs.is_empty() {
            details.push_str(&format!("- Licence files: {}\n", refs.join(", ")));
        }
        details.push('\n');
    }
    out.push_str(&details);
    out.push_str("## Licence texts of the Rust crates\n\n");
    for (i, text) in texts.iter().enumerate() {
        out.push_str(&format!("### Rust licence text {}\n\n```text\n", i + 1));
        out.push_str(&text.replace("```", "'''"));
        out.push_str("\n```\n\n");
    }
    out.push_str("## npm packages and bundled assets\n\n");
    // The npm script prints its own title and preamble; keep everything from its table on and
    // demote its headings one level so they sit under this section.
    let body = npm.find("| Package |").map_or(npm, |i| &npm[i..]);
    for line in body.lines() {
        if let Some(rest) = line.strip_prefix("## ") {
            out.push_str("### ");
            out.push_str(rest);
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    out.push_str(
        "\n## Test data\n\n\
         `fixtures/pptx/read/external/*.json` are dumps of test decks, text included, made with\n\
         python-pptx. Each one's source and licence is in `fixtures/corpus-manifest.json` (`derived`).\n\n\
         - Apache POI test data (`test-data/`), Apache License 2.0, https://poi.apache.org/\n\
         - LibreOffice test data, Mozilla Public License 2.0, https://www.libreoffice.org/\n\n\
         The decks and other files they come from are downloaded by `tools/fetch-corpus.sh` and are\n\
         not part of this repository.\n",
    );
    format!("{}\n", out.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_font_glob_of_the_worker_is_read() {
        let source = "glob('../../../assets/fonts/{carlito,liberation}/*.ttf', x);\n\
                      glob('../../../assets/fonts/noto/*.{ttf,otf}');\n\
                      glob(\"../../../assets/fonts/noto/cjk/*.otf\");\n\
                      const ONE = '../../../assets/fonts/carlito/Carlito-Regular.ttf';";
        assert_eq!(
            font_globs(source),
            [
                "{carlito,liberation}/*.ttf",
                "noto/*.{ttf,otf}",
                "noto/cjk/*.otf"
            ]
        );
        assert_eq!(expand("{a,b}/x"), ["a/x", "b/x"]);
        assert!(wildcard("NotoSansCJKsc-*.otf", "NotoSansCJKsc-07.otf"));
        assert!(!wildcard("NotoSansCJKsc-*.otf", "NotoEmoji-Regular.ttf"));
        assert!(expand("*.{ttf,otf}").iter().any(|f| wildcard(f, "A.otf")));
    }

    /// The fonts of this tree: the CJK slices and Noto Emoji are among those shipped, and each
    /// has its notice.
    #[test]
    fn the_shipped_fonts_include_the_slices_and_emoji_and_all_have_notices() {
        let root = util::repo_root();
        let fonts = shipped_fonts(&root).unwrap();
        assert!(fonts.contains("NotoEmoji-Regular.ttf"), "{fonts:?}");
        assert!(
            fonts.iter().any(|f| f.starts_with("NotoSansCJKsc-")),
            "{fonts:?}"
        );
        fonts_have_notices(&root).unwrap();
    }

    fn meta() -> Value {
        serde_json::json!({
            "workspace_members": ["app 0.1.0", "xtask 0.1.0"],
            "packages": [
                {"id": "app 0.1.0", "name": "app", "version": "0.1.0", "license": "AGPL-3.0-or-later", "manifest_path": "/w/app/Cargo.toml"},
                {"id": "xtask 0.1.0", "name": "xtask", "version": "0.1.0", "manifest_path": "/w/xtask/Cargo.toml"},
                {"id": "serde 1", "name": "serde", "version": "1.0.0", "license": "MIT OR Apache-2.0", "repository": "https://example.org/serde", "manifest_path": "/r/serde/Cargo.toml"},
                {"id": "proptest 1", "name": "proptest", "version": "1.0.0", "license": "MIT", "manifest_path": "/r/proptest/Cargo.toml"},
                {"id": "cc 1", "name": "cc", "version": "1.0.0", "license": "MIT", "manifest_path": "/r/cc/Cargo.toml"},
                {"id": "json 1", "name": "serde_json", "version": "1.0.0", "license": "MIT", "manifest_path": "/r/json/Cargo.toml"}
            ],
            "resolve": {"nodes": [
                {"id": "app 0.1.0", "deps": [
                    {"pkg": "serde 1", "dep_kinds": [{"kind": null}]},
                    {"pkg": "proptest 1", "dep_kinds": [{"kind": "dev"}]},
                    {"pkg": "cc 1", "dep_kinds": [{"kind": "build"}]}
                ]},
                {"id": "xtask 0.1.0", "deps": [{"pkg": "json 1", "dep_kinds": [{"kind": null}]}]},
                {"id": "serde 1", "deps": []}
            ]}
        })
    }

    #[test]
    fn collects_only_shipped_normal_dependencies() {
        let crates = collect(&meta());
        let names: Vec<_> = crates.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["serde"]);
        assert_eq!(crates[0].license, "MIT OR Apache-2.0");
    }

    #[test]
    fn prints_each_distinct_licence_text_once() {
        let base = std::env::temp_dir().join(format!("kiln-notices-test-{}", std::process::id()));
        let crates: Vec<Crate> = ["a", "b"]
            .iter()
            .map(|name| {
                let dir = base.join(name);
                fs::create_dir_all(&dir).expect("temp dir");
                fs::write(dir.join("LICENSE-MIT"), "MIT licence text\n").expect("write");
                Crate {
                    name: (*name).to_owned(),
                    version: "1.0.0".to_owned(),
                    license: "MIT".to_owned(),
                    repository: None,
                    dir: dir.display().to_string(),
                }
            })
            .collect();
        let text = render("", &crates);
        let _ = fs::remove_dir_all(&base);
        assert_eq!(text.matches("MIT licence text").count(), 1);
        assert_eq!(text.matches("[text 1](#rust-licence-text-1)").count(), 2);
        assert!(text.contains("### Rust licence text 1"));
    }

    #[test]
    fn renders_rust_table_and_npm_part() {
        let crates = collect(&meta());
        let npm = "# Third-party notices\n\npreamble\n\n| Package | Version | Licence |\n|---|---|---|\n| react | 19 | MIT |\n\n## react 19\n\n- Licence: MIT\n";
        let text = render(npm, &crates);
        assert!(text.starts_with("# Third-party notices\n"));
        assert!(text.contains("| serde | 1.0.0 | MIT OR Apache-2.0 |"));
        assert!(text.contains("- Repository: https://example.org/serde"));
        assert!(text.contains("| react | 19 | MIT |"));
        assert!(text.contains("### react 19"));
        assert!(!text.contains("preamble"));
        assert!(text.contains("- Licence: MIT\n"));
        assert!(text.ends_with("not part of this repository.\n"));
    }
}
