// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fonts for slides: the bundled document fonts plus Kiln's brand families (Fraunces and
//! Figtree, OFL), which the Kiln and Tenmoku themes use. Native builds embed the brand fonts;
//! the web core receives them from the page with every other document font.

use kiln_text::FontStore;

/// The brand fonts' file names and bytes (native builds).
#[cfg(not(target_arch = "wasm32"))]
pub const BRAND_FONTS: &[(&str, &[u8])] = &[
    (
        "Fraunces-VF.ttf",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/fonts/brand/Fraunces/Fraunces-VF.ttf"
        )),
    ),
    (
        "Figtree-VF.ttf",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/fonts/brand/Figtree/Figtree-VF.ttf"
        )),
    ),
];

/// Adds the brand fonts to a store (no-op on the web, where the page registers them).
pub fn add_brand_fonts(store: &mut FontStore) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        for (_, bytes) in BRAND_FONTS {
            store.add_font_data(bytes.to_vec());
        }
        store.refresh_fallbacks();
    }
    #[cfg(target_arch = "wasm32")]
    let _ = store;
}

/// The bundled document fonts and the brand fonts (native builds; tests, the CLI).
#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn default_fonts() -> FontStore {
    let mut s = FontStore::bundled();
    add_brand_fonts(&mut s);
    s
}
