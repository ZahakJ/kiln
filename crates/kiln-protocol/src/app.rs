// SPDX-License-Identifier: AGPL-3.0-or-later
//! Apps, document kinds and file formats.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A Kiln app. Commands declare which apps they belong to (`CommandMeta::apps`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum App {
    /// Kiln Write (documents).
    Write,
    /// Kiln Grid (spreadsheets).
    Grid,
    /// Kiln Deck (presentations).
    Deck,
    /// Kiln Notes (notebooks).
    Notes,
    /// Kiln Home (start screen, no document).
    Home,
}

impl App {
    /// Every app, in display order.
    pub const ALL: [App; 5] = [App::Write, App::Grid, App::Deck, App::Notes, App::Home];

    const fn bit(self) -> u8 {
        match self {
            App::Write => 1,
            App::Grid => 2,
            App::Deck => 4,
            App::Notes => 8,
            App::Home => 16,
        }
    }
}

/// A set of [`App`]s, usable in `const` command tables.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppMask(u8);

impl AppMask {
    /// No app.
    pub const NONE: AppMask = AppMask(0);
    /// Kiln Write.
    pub const WRITE: AppMask = AppMask(App::Write.bit());
    /// Kiln Grid.
    pub const GRID: AppMask = AppMask(App::Grid.bit());
    /// Kiln Deck.
    pub const DECK: AppMask = AppMask(App::Deck.bit());
    /// Kiln Notes.
    pub const NOTES: AppMask = AppMask(App::Notes.bit());
    /// Kiln Home.
    pub const HOME: AppMask = AppMask(App::Home.bit());
    /// Every app that edits a document (Write, Grid, Deck, Notes).
    pub const DOCUMENTS: AppMask = AppMask(1 | 2 | 4 | 8);
    /// Every app including Home.
    pub const ALL: AppMask = AppMask(1 | 2 | 4 | 8 | 16);

    /// The union of two masks.
    #[must_use]
    pub const fn union(self, other: AppMask) -> AppMask {
        AppMask(self.0 | other.0)
    }

    /// Whether `app` is in the set.
    #[must_use]
    pub const fn contains(self, app: App) -> bool {
        self.0 & app.bit() != 0
    }

    /// The apps in the set, in [`App::ALL`] order.
    #[must_use]
    pub fn apps(self) -> Vec<App> {
        App::ALL.into_iter().filter(|a| self.contains(*a)).collect()
    }
}

/// The kind of an open document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, TS)]
pub enum DocKind {
    /// Plain paragraphs of text without layout: the document that proves the bridge loop before
    /// the Write engine exists. Belongs to Kiln Write.
    Scratch,
    /// A word-processing document (Kiln Write).
    Write,
    /// A workbook (Kiln Grid).
    Grid,
    /// A presentation (Kiln Deck).
    Deck,
    /// A notebook page (Kiln Notes).
    Notes,
}

impl DocKind {
    /// The app that edits documents of this kind.
    #[must_use]
    pub const fn app(self) -> App {
        match self {
            DocKind::Scratch | DocKind::Write => App::Write,
            DocKind::Grid => App::Grid,
            DocKind::Deck => App::Deck,
            DocKind::Notes => App::Notes,
        }
    }

    /// The title of a new, unsaved document of this kind (Office numbers them: "Document1").
    #[must_use]
    pub const fn untitled_stem(self) -> &'static str {
        match self {
            DocKind::Scratch => "Scratch",
            DocKind::Write => "Document",
            DocKind::Grid => "Book",
            DocKind::Deck => "Presentation",
            DocKind::Notes => "Page",
        }
    }
}

/// A file format Kiln reads or writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum FileFormat {
    /// Plain text (UTF-8).
    Txt,
    /// Markdown.
    Md,
    /// Word document.
    Docx,
    /// Word template.
    Dotx,
    /// Word macro-enabled document (macros preserved, never run).
    Docm,
    /// Word macro-enabled template (macros preserved, never run).
    Dotm,
    /// A web page (export).
    Html,
    /// OpenDocument text.
    Odt,
    /// Excel workbook.
    Xlsx,
    /// Excel template.
    Xltx,
    /// Excel macro-enabled workbook (macros preserved, never run).
    Xlsm,
    /// Comma-separated values.
    Csv,
    /// OpenDocument spreadsheet.
    Ods,
    /// PowerPoint presentation.
    Pptx,
    /// PowerPoint template.
    Potx,
    /// PowerPoint macro-enabled presentation (macros preserved, never run).
    Pptm,
    /// PowerPoint show (opens as a slide show).
    Ppsx,
    /// OpenDocument presentation.
    Odp,
    /// PDF (export only).
    Pdf,
    /// Kiln Write document (native bundle with its version history).
    KWrite,
    /// Kiln Grid workbook (native bundle with its version history).
    KGrid,
    /// Kiln Deck presentation (native bundle with its version history).
    KDeck,
}

impl FileFormat {
    /// Every format.
    pub const ALL: [FileFormat; 22] = [
        FileFormat::Txt,
        FileFormat::Md,
        FileFormat::Docx,
        FileFormat::Dotx,
        FileFormat::Docm,
        FileFormat::Dotm,
        FileFormat::Html,
        FileFormat::Odt,
        FileFormat::Xlsx,
        FileFormat::Xltx,
        FileFormat::Xlsm,
        FileFormat::Csv,
        FileFormat::Ods,
        FileFormat::Pptx,
        FileFormat::Potx,
        FileFormat::Pptm,
        FileFormat::Ppsx,
        FileFormat::Odp,
        FileFormat::Pdf,
        FileFormat::KWrite,
        FileFormat::KGrid,
        FileFormat::KDeck,
    ];

    /// The usual file extension, without the dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            FileFormat::Txt => "txt",
            FileFormat::Md => "md",
            FileFormat::Docx => "docx",
            FileFormat::Dotx => "dotx",
            FileFormat::Docm => "docm",
            FileFormat::Dotm => "dotm",
            FileFormat::Html => "html",
            FileFormat::Odt => "odt",
            FileFormat::Xlsx => "xlsx",
            FileFormat::Xltx => "xltx",
            FileFormat::Xlsm => "xlsm",
            FileFormat::Csv => "csv",
            FileFormat::Ods => "ods",
            FileFormat::Pptx => "pptx",
            FileFormat::Potx => "potx",
            FileFormat::Pptm => "pptm",
            FileFormat::Ppsx => "ppsx",
            FileFormat::Odp => "odp",
            FileFormat::Pdf => "pdf",
            FileFormat::KWrite => "kwrite",
            FileFormat::KGrid => "kgrid",
            FileFormat::KDeck => "kdeck",
        }
    }

    /// The native bundle of a kind of document: `.kwrite`, `.kgrid` or `.kdeck`.
    #[must_use]
    pub const fn bundle_of(kind: DocKind) -> Option<Self> {
        match kind {
            DocKind::Write => Some(FileFormat::KWrite),
            DocKind::Grid => Some(FileFormat::KGrid),
            DocKind::Deck => Some(FileFormat::KDeck),
            DocKind::Scratch | DocKind::Notes => None,
        }
    }

    /// Whether this is one of Kiln's native bundles.
    #[must_use]
    pub const fn is_bundle(self) -> bool {
        matches!(
            self,
            FileFormat::KWrite | FileFormat::KGrid | FileFormat::KDeck
        )
    }

    /// The format for a file extension (case-insensitive, with or without the dot).
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        let ext = ext.strip_prefix('.').unwrap_or(ext);
        if ext.eq_ignore_ascii_case("htm") {
            return Some(FileFormat::Html);
        }
        Self::ALL
            .into_iter()
            .find(|f| f.extension().eq_ignore_ascii_case(ext))
    }

    /// The format of a path or file name, from its extension.
    #[must_use]
    pub fn from_path(path: &str) -> Option<Self> {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let (_, ext) = name.rsplit_once('.')?;
        Self::from_extension(ext)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masks_are_sets_of_apps() {
        assert!(AppMask::ALL.contains(App::Home));
        assert!(!AppMask::DOCUMENTS.contains(App::Home));
        assert_eq!(
            AppMask::WRITE.union(AppMask::NOTES).apps(),
            vec![App::Write, App::Notes]
        );
        assert!(AppMask::NONE.apps().is_empty());
        assert_eq!(AppMask::ALL.apps(), App::ALL.to_vec());
        for app in App::ALL {
            assert!(AppMask::ALL.contains(app));
        }
        assert_eq!(AppMask::GRID.apps(), vec![App::Grid]);
        assert_eq!(
            AppMask::DECK.union(AppMask::HOME).apps(),
            vec![App::Deck, App::Home]
        );
    }

    #[test]
    fn doc_kinds_map_to_apps() {
        assert_eq!(DocKind::Scratch.app(), App::Write);
        assert_eq!(DocKind::Grid.app(), App::Grid);
        assert_eq!(DocKind::Deck.app(), App::Deck);
        assert_eq!(DocKind::Notes.app(), App::Notes);
        assert_eq!(DocKind::Write.untitled_stem(), "Document");
        assert_eq!(DocKind::Grid.untitled_stem(), "Book");
    }

    #[test]
    fn formats_from_extensions_and_paths() {
        for f in FileFormat::ALL {
            assert_eq!(FileFormat::from_extension(f.extension()), Some(f));
        }
        assert_eq!(FileFormat::from_extension(".DOCX"), Some(FileFormat::Docx));
        assert_eq!(
            FileFormat::from_path("C:\\Users\\a\\Report.final.XLSX"),
            Some(FileFormat::Xlsx)
        );
        assert_eq!(
            FileFormat::from_path("/data/a/notes.txt"),
            Some(FileFormat::Txt)
        );
        assert_eq!(FileFormat::from_path("/data/a.b/README"), None);
        assert_eq!(FileFormat::from_extension("exe"), None);
    }

    #[test]
    fn every_document_kind_with_an_app_has_a_bundle() {
        assert_eq!(
            FileFormat::bundle_of(DocKind::Write),
            Some(FileFormat::KWrite)
        );
        assert_eq!(
            FileFormat::bundle_of(DocKind::Grid),
            Some(FileFormat::KGrid)
        );
        assert_eq!(
            FileFormat::bundle_of(DocKind::Deck),
            Some(FileFormat::KDeck)
        );
        assert_eq!(FileFormat::bundle_of(DocKind::Notes), None);
        assert!(FileFormat::from_path("/a/Plan.kwrite").is_some_and(FileFormat::is_bundle));
        assert!(!FileFormat::Docx.is_bundle());
    }
}
