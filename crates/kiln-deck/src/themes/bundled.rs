// SPDX-License-Identifier: AGPL-3.0-or-later
//! The twelve bundled themes (`assets/themes/*.kdeck-theme`), compiled in so every host —
//! including the web build — has them without file access.

use super::ThemeFile;

/// A bundled theme file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BundledTheme {
    /// The theme id (the file stem).
    pub id: &'static str,
    /// The file's text.
    pub source: &'static str,
}

macro_rules! bundled {
    ($($id:literal),+ $(,)?) => {
        /// The bundled themes in gallery order.
        pub const BUNDLED: &[BundledTheme] = &[
            $(BundledTheme {
                id: $id,
                source: include_str!(concat!("../../../../assets/themes/", $id, ".kdeck-theme")),
            }),+
        ];
    };
}

bundled!(
    "kiln",
    "tenmoku",
    "cobalt",
    "jade",
    "saffron",
    "celadon",
    "shino",
    "raku",
    "porcelain",
    "slate",
    "terracotta",
    "ash",
);

/// Every bundled theme that parses (all of them; tested).
#[must_use]
pub fn bundled() -> Vec<ThemeFile> {
    BUNDLED
        .iter()
        .filter_map(|b| ThemeFile::parse(b.source).ok())
        .collect()
}

/// A bundled theme by id.
#[must_use]
pub fn bundled_theme(id: &str) -> Option<ThemeFile> {
    BUNDLED
        .iter()
        .find(|b| b.id == id)
        .and_then(|b| ThemeFile::parse(b.source).ok())
}
