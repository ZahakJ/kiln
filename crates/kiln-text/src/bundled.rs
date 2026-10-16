// SPDX-License-Identifier: AGPL-3.0-or-later
//! The OFL document fonts compiled into the binary (feature `bundled-fonts`). Sources and checksums:
//! `assets/fonts/SOURCES.md`.

macro_rules! font {
    ($dir:literal, $file:literal) => {
        (
            $file,
            include_bytes!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../assets/fonts/",
                $dir,
                "/",
                $file
            ))
            .as_slice(),
        )
    };
}

/// File name and bytes of every bundled font.
pub(crate) const FONTS: &[(&str, &[u8])] = &[
    font!("liberation", "LiberationMono-BoldItalic.ttf"),
    font!("liberation", "LiberationMono-Bold.ttf"),
    font!("liberation", "LiberationMono-Italic.ttf"),
    font!("liberation", "LiberationMono-Regular.ttf"),
    font!("liberation", "LiberationSans-BoldItalic.ttf"),
    font!("liberation", "LiberationSans-Bold.ttf"),
    font!("liberation", "LiberationSans-Italic.ttf"),
    font!("liberation", "LiberationSans-Regular.ttf"),
    font!("liberation", "LiberationSerif-BoldItalic.ttf"),
    font!("liberation", "LiberationSerif-Bold.ttf"),
    font!("liberation", "LiberationSerif-Italic.ttf"),
    font!("liberation", "LiberationSerif-Regular.ttf"),
    font!("carlito", "Carlito-BoldItalic.ttf"),
    font!("carlito", "Carlito-Bold.ttf"),
    font!("carlito", "Carlito-Italic.ttf"),
    font!("carlito", "Carlito-Regular.ttf"),
    font!("caladea", "Caladea-BoldItalic.ttf"),
    font!("caladea", "Caladea-Bold.ttf"),
    font!("caladea", "Caladea-Italic.ttf"),
    font!("caladea", "Caladea-Regular.ttf"),
];
