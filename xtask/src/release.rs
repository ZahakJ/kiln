// SPDX-License-Identifier: AGPL-3.0-or-later
//! The apps of the release on the user-facing surfaces.
//!
//! v0.1 ships Kiln Write, Grid and Deck; Kiln Notes follows after the release (it is built and
//! tested, and hidden by `RELEASE_APPS` in `packages/commands/src/types.ts`). Nothing a user or
//! visitor reads may advertise it: the interface messages (`packages/i18n/locales/*.json`), the
//! manual (`docs/manual`), the README, the landing page (`docs/index.html`), the package
//! descriptions (`package.json`, the desktop bundle's `tauri.conf.json`) and the issue forms
//! (`.github/ISSUE_TEMPLATE`). [`offenders`] finds
//! Kiln Notes named as an app there; speaker notes, the Notes pane, notes pages and a cell's notes
//! are features of the other apps and are not matched. The test below runs in `cargo xtask verify`.

use std::path::{Path, PathBuf};

/// Kiln Notes as an app, in English, German and Arabic (lower case).
const PATTERNS: &[&str] = &[
    "kiln notes",
    "notebook",
    "onenote",
    "notizbuch",
    "notes app",
    "write, grid, deck and notes",
    "write, grid, deck, notes",
    "دفتر الملاحظات",
    "دفاتر الملاحظات",
];

/// The files to check, relative to the repository root.
fn surfaces(root: &Path) -> Vec<PathBuf> {
    let mut out = vec![
        root.join("README.md"),
        root.join("docs/index.html"),
        root.join("package.json"),
        root.join("apps/desktop/src-tauri/tauri.conf.json"),
    ];
    for locale in ["en", "de", "ar"] {
        out.push(root.join(format!("packages/i18n/locales/{locale}.json")));
    }
    let mut dirs = vec![
        root.join("docs/manual"),
        root.join(".github/ISSUE_TEMPLATE"),
    ];
    while let Some(dir) = dirs.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                dirs.push(path);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("md" | "html" | "yml")
            ) {
                out.push(path);
            }
        }
    }
    out.sort();
    out
}

/// Every line of the user-facing surfaces that names Kiln Notes as an app: `path:line: text`.
pub fn offenders(root: &Path) -> Vec<String> {
    let mut found = Vec::new();
    for path in surfaces(root) {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let rel = path
            .strip_prefix(root)
            .unwrap_or(&path)
            .display()
            .to_string();
        for (i, line) in text.lines().enumerate() {
            let lower = line.to_lowercase();
            if PATTERNS.iter().any(|p| lower.contains(p)) {
                found.push(format!("{rel}:{}: {}", i + 1, line.trim()));
            }
        }
    }
    found
}

/// `cargo xtask release`: fails when a user-facing surface names Kiln Notes as an app.
pub fn run(args: &[String]) -> Result<(), String> {
    if let Some(a) = args.first() {
        return Err(format!("unknown option '{a}'"));
    }
    let root = crate::util::repo_root();
    let found = offenders(&root);
    if found.is_empty() {
        println!("release: the user-facing surfaces name only Kiln Write, Grid and Deck");
        Ok(())
    } else {
        Err(format!(
            "Kiln Notes follows after v0.1; these lines name it as an app:\n{}",
            found.join("\n")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
    }

    #[test]
    fn nothing_user_facing_advertises_kiln_notes_in_this_release() {
        let found = offenders(&root());
        assert!(
            found.is_empty(),
            "Kiln Notes follows after v0.1; these lines name it as an app:\n{}",
            found.join("\n")
        );
    }

    #[test]
    fn the_check_reads_the_surfaces_and_tells_features_from_the_app() {
        let files = surfaces(&root());
        assert!(files.iter().any(|f| f.ends_with("docs/manual/README.md")));
        assert!(
            files
                .iter()
                .any(|f| f.ends_with("src-tauri/tauri.conf.json"))
        );
        assert!(
            files
                .iter()
                .any(|f| f.ends_with("ISSUE_TEMPLATE/bug_report.yml"))
        );
        assert!(
            files
                .iter()
                .any(|f| f.ends_with("packages/i18n/locales/ar.json"))
        );
        let dir = std::env::temp_dir().join(format!("kiln-release-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("docs/manual")).unwrap();
        std::fs::write(
            dir.join("docs/manual/deck.md"),
            "Speaker notes, the Notes pane and Notes Page view.\n",
        )
        .unwrap();
        std::fs::write(dir.join("README.md"), "Kiln Write, Grid, Deck and Notes.\n").unwrap();
        let found = offenders(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(found, ["README.md:1: Kiln Write, Grid, Deck and Notes."]);
    }
}
