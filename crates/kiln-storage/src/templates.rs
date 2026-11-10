// SPDX-License-Identifier: AGPL-3.0-or-later
//! The templates index over `assets/templates` (Kiln Home ▸ New, File ▸ New).
//!
//! The index is `index.json` in the templates folder:
//!
//! ```json
//! { "version": 1, "templates": [
//!   { "id": "write-letter", "kind": "Write", "name": "Studio letter",
//!     "file": "write/letter.kwrite", "thumbnail": "write/letter.png",
//!     "description": "A one-page letter", "featured": true } ] }
//! ```
//!
//! `featured` templates are the ones Kiln Home shows in its strip; the order of the index is the
//! order of the gallery.
//!
//! Paths are relative to the folder. Without an index, [`scan`] lists the native bundles in the
//! folder and its subfolders (a picture with the same name is the thumbnail).

use kiln_common::{KilnError, Result};
use kiln_protocol::DocKind;
#[cfg(not(target_arch = "wasm32"))]
use kiln_protocol::FileFormat;
use serde::{Deserialize, Serialize};

/// The index format this version reads.
pub const INDEX_VERSION: u32 = 1;

/// One template.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Template {
    /// A stable id.
    pub id: String,
    /// The kind of document it makes.
    pub kind: DocKind,
    /// The name shown.
    pub name: String,
    /// The template file, relative to the templates folder.
    pub file: String,
    /// Its picture, relative to the folder.
    #[serde(default)]
    pub thumbnail: Option<String>,
    /// One line about it.
    #[serde(default)]
    pub description: Option<String>,
    /// Shown in Kiln Home's featured strip.
    #[serde(default)]
    pub featured: bool,
}

#[derive(Debug, Deserialize)]
struct Index {
    version: u32,
    #[serde(default)]
    templates: Vec<Template>,
}

/// Reads an index.
///
/// # Errors
///
/// `Format` for an index that does not parse, of a newer version, or with a path that leaves
/// the folder.
pub fn parse(bytes: &[u8]) -> Result<Vec<Template>> {
    let bad = |message: String| KilnError::Format {
        format: "templates index".into(),
        message,
    };
    let index: Index = serde_json::from_slice(bytes).map_err(|e| bad(e.to_string()))?;
    if index.version > INDEX_VERSION {
        return Err(bad(format!(
            "version {} is newer than this version of Kiln reads ({INDEX_VERSION})",
            index.version
        )));
    }
    for t in &index.templates {
        for path in std::iter::once(&t.file).chain(t.thumbnail.iter()) {
            crate::bundle::check_name(path)
                .map_err(|_| bad(format!("{path:?} leaves the folder")))?;
        }
    }
    Ok(index.templates)
}

/// The templates of a folder: its index, or the bundles in it.
///
/// # Errors
///
/// `Io` when the folder cannot be read (a missing folder has no templates), `Format` for a bad
/// index.
#[cfg(not(target_arch = "wasm32"))]
pub fn scan(dir: &std::path::Path) -> Result<Vec<Template>> {
    match std::fs::read(dir.join("index.json")) {
        Ok(bytes) => return parse(&bytes),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let mut out = Vec::new();
    let mut dirs = vec![(dir.to_path_buf(), String::new())];
    while let Some((d, rel)) = dirs.pop() {
        let entries = match std::fs::read_dir(&d) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        let mut names: Vec<(String, bool)> = entries
            .filter_map(std::result::Result::ok)
            .map(|e| {
                let dir = e.file_type().is_ok_and(|t| t.is_dir());
                (e.file_name().to_string_lossy().into_owned(), dir)
            })
            .collect();
        names.sort();
        for (name, is_dir) in &names {
            let path = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}/{name}")
            };
            if *is_dir {
                dirs.push((d.join(name), path));
                continue;
            }
            let Some(kind) = FileFormat::from_path(name).and_then(bundle_kind) else {
                continue;
            };
            let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
            let png = format!("{stem}.png");
            out.push(Template {
                id: path.replace(['/', '.'], "-"),
                kind,
                name: stem.to_owned(),
                file: path,
                thumbnail: names.iter().any(|(n, _)| *n == png).then(|| {
                    if rel.is_empty() {
                        png.clone()
                    } else {
                        format!("{rel}/{png}")
                    }
                }),
                description: None,
                featured: false,
            });
        }
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[cfg(not(target_arch = "wasm32"))]
fn bundle_kind(f: FileFormat) -> Option<DocKind> {
    match f {
        FileFormat::KWrite => Some(DocKind::Write),
        FileFormat::KGrid => Some(DocKind::Grid),
        FileFormat::KDeck => Some(DocKind::Deck),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_index_is_read_and_checked() {
        let ok = br#"{"version":1,"templates":[{"id":"w","kind":"Write","name":"Letter",
            "file":"write/letter.kwrite","thumbnail":"write/letter.png"}]}"#;
        let t = parse(ok).unwrap();
        assert_eq!(t[0].kind, DocKind::Write);
        assert_eq!(t[0].description, None);
        let escape = br#"{"version":1,"templates":[{"id":"w","kind":"Write","name":"L",
            "file":"../etc/passwd"}]}"#;
        assert!(parse(escape).is_err());
        assert!(
            parse(br#"{"version":2}"#)
                .unwrap_err()
                .to_string()
                .contains("newer")
        );
        assert!(parse(br#"{"version":1}"#).unwrap().is_empty());
    }

    #[test]
    fn a_folder_without_an_index_lists_its_bundles() {
        let dir = std::env::temp_dir().join(format!("kiln-templates-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("grid")).unwrap();
        std::fs::write(dir.join("grid/Budget.kgrid"), b"x").unwrap();
        std::fs::write(dir.join("grid/Budget.png"), b"x").unwrap();
        std::fs::write(dir.join("Letter.kwrite"), b"x").unwrap();
        std::fs::write(dir.join("notes.txt"), b"x").unwrap();
        let t = scan(&dir).unwrap();
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(t.len(), 2);
        assert_eq!(
            (t[0].kind, t[0].file.as_str(), t[0].thumbnail.as_deref()),
            (DocKind::Write, "Letter.kwrite", None)
        );
        assert_eq!(t[1].thumbnail.as_deref(), Some("grid/Budget.png"));
        assert!(scan(&dir).unwrap().is_empty(), "a missing folder has none");
    }

    /// The templates Kiln ships: `assets/templates/index.json` lists twelve per app, in
    /// that folder, each a bundle of its kind without version history and with a PNG thumbnail.
    #[test]
    fn the_shipped_templates_are_twelve_per_app_with_thumbnails() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/templates");
        let list = scan(&dir).expect("the index reads");
        for kind in [DocKind::Write, DocKind::Grid, DocKind::Deck] {
            assert_eq!(
                list.iter().filter(|t| t.kind == kind).count(),
                12,
                "{kind:?}"
            );
        }
        assert_eq!(list.len(), 36);
        let ids: std::collections::BTreeSet<&str> = list.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids.len(), list.len(), "ids are unique");
        assert_eq!(
            list.iter().filter(|t| t.featured).count(),
            8,
            "eight featured templates"
        );
        for t in &list {
            assert!(
                !t.name.trim().is_empty()
                    && t.description.as_deref().is_some_and(|d| !d.is_empty())
            );
            let bytes = std::fs::read(dir.join(&t.file)).expect("the bundle is there");
            let bundle = crate::Bundle::from_bytes(&bytes).expect("a bundle");
            assert_eq!(bundle.kind, t.kind, "{}", t.file);
            assert!(
                bundle.versions.is_empty(),
                "{} carries no version history",
                t.file
            );
            let thumb = t.thumbnail.as_deref().expect("a thumbnail");
            let png = std::fs::read(dir.join(thumb)).expect("the thumbnail is there");
            assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"), "{thumb} is a PNG");
        }
    }
}
