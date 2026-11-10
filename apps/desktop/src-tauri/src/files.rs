// SPDX-License-Identifier: AGPL-3.0-or-later
//! The documents Kiln opens from the desktop: the file associations (the same list as
//! `tauri.conf.json` and the MIME types of `bundle/linux/kiln.xml`), which app each one opens in,
//! and the paths given on the command line, by the desktop environment or by a drop on a window.

use std::path::{Path, PathBuf};

/// An app of the suite that edits documents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum App {
    /// Kiln Write.
    Write,
    /// Kiln Grid.
    Grid,
    /// Kiln Deck.
    Deck,
}

impl App {
    /// The route segment of the app (`#/write`).
    #[must_use]
    pub const fn route(self) -> &'static str {
        match self {
            Self::Write => "write",
            Self::Grid => "grid",
            Self::Deck => "deck",
        }
    }
}

/// One file association.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Association {
    /// The extension, lower case, without the dot.
    pub ext: &'static str,
    /// The app that opens it.
    pub app: App,
    /// Its MIME type.
    pub mime: &'static str,
}

const fn assoc(ext: &'static str, app: App, mime: &'static str) -> Association {
    Association { ext, app, mime }
}

/// Every file type Kiln registers with the desktop environment.
pub const ASSOCIATIONS: &[Association] = &[
    assoc(
        "docx",
        App::Write,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    ),
    assoc(
        "dotx",
        App::Write,
        "application/vnd.openxmlformats-officedocument.wordprocessingml.template",
    ),
    assoc(
        "docm",
        App::Write,
        "application/vnd.ms-word.document.macroEnabled.12",
    ),
    assoc("odt", App::Write, "application/vnd.oasis.opendocument.text"),
    assoc("kwrite", App::Write, "application/x-kiln-write"),
    assoc(
        "xlsx",
        App::Grid,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    ),
    assoc(
        "xltx",
        App::Grid,
        "application/vnd.openxmlformats-officedocument.spreadsheetml.template",
    ),
    assoc(
        "xlsm",
        App::Grid,
        "application/vnd.ms-excel.sheet.macroEnabled.12",
    ),
    assoc("csv", App::Grid, "text/csv"),
    assoc(
        "ods",
        App::Grid,
        "application/vnd.oasis.opendocument.spreadsheet",
    ),
    assoc("kgrid", App::Grid, "application/x-kiln-grid"),
    assoc(
        "pptx",
        App::Deck,
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    ),
    assoc(
        "potx",
        App::Deck,
        "application/vnd.openxmlformats-officedocument.presentationml.template",
    ),
    assoc(
        "pptm",
        App::Deck,
        "application/vnd.ms-powerpoint.presentation.macroEnabled.12",
    ),
    assoc(
        "odp",
        App::Deck,
        "application/vnd.oasis.opendocument.presentation",
    ),
    assoc("kdeck", App::Deck, "application/x-kiln-deck"),
];

/// The association of a file, by its extension (any case).
#[must_use]
pub fn association(path: &Path) -> Option<&'static Association> {
    let ext = path.extension()?.to_str()?.to_ascii_lowercase();
    ASSOCIATIONS.iter().find(|a| a.ext == ext)
}

/// Whether Kiln opens this file as a document (an associated type).
#[must_use]
pub fn opens(path: &Path) -> bool {
    association(path).is_some()
}

/// A path from the command line or the desktop environment: `file://` URIs (what `%U` passes)
/// become paths, relative paths are taken from `cwd`. `None` for other URIs.
#[must_use]
pub fn resolve_arg(arg: &str, cwd: &Path) -> Option<PathBuf> {
    if let Some(rest) = arg.strip_prefix("file://") {
        // `file:///data/a/b.docx` or `file://localhost/data/a/b.docx`.
        let path = rest.strip_prefix("localhost").unwrap_or(rest);
        if !path.starts_with('/') {
            return None;
        }
        let decoded = crate::bridge::percent_decode(path).ok()?;
        return Some(PathBuf::from(decoded));
    }
    if arg.contains("://") {
        return None;
    }
    let p = Path::new(arg);
    Some(if p.is_absolute() {
        p.to_path_buf()
    } else {
        cwd.join(p)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_extension_opens_in_its_app() {
        let app = |p: &str| association(Path::new(p)).map(|a| a.app);
        for ext in ["docx", "dotx", "docm", "odt", "kwrite"] {
            assert_eq!(app(&format!("a.{ext}")), Some(App::Write), "{ext}");
        }
        for ext in ["xlsx", "xltx", "xlsm", "csv", "ods", "kgrid"] {
            assert_eq!(app(&format!("a.{ext}")), Some(App::Grid), "{ext}");
        }
        for ext in ["pptx", "potx", "pptm", "odp", "kdeck"] {
            assert_eq!(app(&format!("a.{ext}")), Some(App::Deck), "{ext}");
        }
        assert_eq!(app("Report.DOCX"), Some(App::Write));
        assert!(!opens(Path::new("photo.png")));
        assert!(!opens(Path::new("README")));
        assert_eq!(ASSOCIATIONS.len(), 16);
    }

    #[test]
    fn the_bundle_registers_every_association_with_its_icon() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let listed = conf["bundle"]["fileAssociations"]
            .as_array()
            .expect("fileAssociations");
        let mut exts: Vec<&str> = listed
            .iter()
            .flat_map(|a| a["ext"].as_array().into_iter().flatten())
            .filter_map(serde_json::Value::as_str)
            .collect();
        exts.sort_unstable();
        let mut ours: Vec<&str> = ASSOCIATIONS.iter().map(|a| a.ext).collect();
        ours.sort_unstable();
        assert_eq!(exts, ours);
        for a in listed {
            let ext = a["ext"][0].as_str().unwrap_or_default();
            let ours = ASSOCIATIONS.iter().find(|x| x.ext == ext).expect(ext);
            assert_eq!(a["mimeType"].as_str(), Some(ours.mime), "{ext}");
        }
        // Linux: the MIME database entries and the per-type icons the deb installs.
        let mime = include_str!("../bundle/linux/kiln.xml");
        let files = conf["bundle"]["linux"]["deb"]["files"]
            .as_object()
            .expect("deb files");
        for a in ASSOCIATIONS {
            if a.mime.starts_with("application/x-kiln-") {
                assert!(mime.contains(&format!("type=\"{}\"", a.mime)), "{}", a.mime);
                assert!(
                    mime.contains(&format!("pattern=\"*.{}\"", a.ext)),
                    "{}",
                    a.ext
                );
            }
            let icon = format!(
                "/usr/share/icons/hicolor/256x256/mimetypes/{}.png",
                a.mime.replace('/', "-")
            );
            assert!(files.contains_key(&icon), "{icon}");
        }
        let desktop = include_str!("../bundle/linux/kiln.desktop");
        for a in ASSOCIATIONS {
            assert!(desktop.contains(a.mime), "{}", a.mime);
        }
    }

    #[test]
    fn resolves_command_line_paths_and_file_uris() {
        let cwd = Path::new("/data/zo");
        assert_eq!(
            resolve_arg("report.docx", cwd),
            Some(PathBuf::from("/data/zo/report.docx"))
        );
        assert_eq!(
            resolve_arg("/tmp/a.xlsx", cwd),
            Some(PathBuf::from("/tmp/a.xlsx"))
        );
        assert_eq!(
            resolve_arg("file:///tmp/Zo%C3%AB%20b.pptx", cwd),
            Some(PathBuf::from("/tmp/Zoë b.pptx"))
        );
        assert_eq!(
            resolve_arg("file://localhost/tmp/c.odt", cwd),
            Some(PathBuf::from("/tmp/c.odt"))
        );
        assert_eq!(resolve_arg("https://example.com/a.docx", cwd), None);
    }
}
