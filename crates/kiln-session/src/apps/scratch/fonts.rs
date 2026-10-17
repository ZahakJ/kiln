// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fonts of the documents' layout.
//!
//! Text in Calibri, Kiln's default, maps to Carlito (metric-compatible) through the substitution
//! table. Natively the full bundled OFL set is available (Liberation for fallback, no system
//! fonts, so renderings are identical on every machine). The web core embeds no font: the page
//! fetches Carlito Regular as a static asset (cached by the browser and the service worker) and
//! gives it to the core with [`register`] before the first document opens, which keeps the
//! WebAssembly binary inside its budget (B12). Latin text lays out and rasterises identically
//! either way, which the painter-parity test relies on.

use std::sync::{Arc, Mutex};

use kiln_text::FontStore;
#[cfg(target_arch = "wasm32")]
use kiln_text::FontStoreOptions;

/// Fonts the host registered at run time (the web core's document fonts).
static REGISTERED: Mutex<Vec<Arc<[u8]>>> = Mutex::new(Vec::new());

/// Most fonts a host may register (a guard against a page registering in a loop).
const MAX_REGISTERED: usize = 32;

/// Adds a font (TrueType or OpenType bytes) to every font store created from now on. Returns
/// `false` when it was already registered or the limit is reached.
pub fn register(bytes: Vec<u8>) -> bool {
    let Ok(mut fonts) = REGISTERED.lock() else {
        return false;
    };
    if fonts.len() >= MAX_REGISTERED || fonts.iter().any(|f| f[..] == bytes[..]) {
        return false;
    }
    fonts.push(Arc::from(bytes));
    true
}

fn registered() -> Vec<Arc<[u8]>> {
    REGISTERED.lock().map(|f| f.clone()).unwrap_or_default()
}

/// The font store for a document.
#[must_use]
pub fn store() -> FontStore {
    #[cfg(not(target_arch = "wasm32"))]
    let mut fonts = FontStore::bundled();
    #[cfg(target_arch = "wasm32")]
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: false,
        system: false,
    });
    for f in registered() {
        fonts.add_font_data(f.to_vec());
    }
    fonts
}
