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
    // Arabic, symbols and mathematics.
    font!("noto", "NotoNaskhArabic-Regular.ttf"),
    font!("noto", "NotoNaskhArabic-Bold.ttf"),
    font!("noto", "NotoSansSymbols2-Regular.ttf"),
    font!("noto", "NotoSansMath-Regular.ttf"),
];

/// The bytes of a bundled font by its file name (`Carlito-Regular.ttf`), for a shell that serves
/// the same fonts to its interface instead of shipping them twice.
#[must_use]
pub fn bundled_font(file: &str) -> Option<&'static [u8]> {
    FONTS
        .iter()
        .find(|(name, _)| *name == file)
        .map(|(_, bytes)| *bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_bundled_fonts_by_file_name() {
        assert!(super::bundled_font("Carlito-Regular.ttf").is_some_and(|b| b.len() > 100_000));
        assert!(super::bundled_font("Arial.ttf").is_none());
    }
}
