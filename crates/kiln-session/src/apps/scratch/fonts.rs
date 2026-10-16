// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fonts of the scratch document.
//!
//! Its text is Calibri 11 pt, which the substitution table maps to Carlito (metric-compatible).
//! Natively the full bundled OFL set is available (Liberation for fallback, no system fonts, so
//! renderings are identical on every machine). The web core embeds Carlito Regular only: the
//! whole set would not fit the 4.5 MB WebAssembly budget (B12), and Latin text lays out and
//! rasterises identically either way, which the painter-parity test relies on.

use kiln_text::FontStore;
#[cfg(target_arch = "wasm32")]
use kiln_text::FontStoreOptions;

/// The font store for a scratch document.
#[must_use]
pub fn store() -> FontStore {
    #[cfg(not(target_arch = "wasm32"))]
    {
        FontStore::bundled()
    }
    #[cfg(target_arch = "wasm32")]
    {
        static CARLITO: &[u8] =
            include_bytes!("../../../../../assets/fonts/carlito/Carlito-Regular.ttf");
        let mut fonts = FontStore::new(&FontStoreOptions {
            bundled: false,
            system: false,
        });
        fonts.add_font_data(CARLITO.to_vec());
        fonts
    }
}
