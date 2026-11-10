// SPDX-License-Identifier: AGPL-3.0-or-later
//! The fonts of the documents' layout.
//!
//! Text in Calibri, Kiln's default, maps to Carlito (metric-compatible) through the substitution
//! table. Natively the full bundled OFL set is available (Liberation for fallback; the document
//! fonts are never the system's, so renderings are identical on every machine), and the
//! platform's own Chinese, Japanese, Korean and emoji fonts are the fallback for text that no
//! bundled font has (`FontStore::add_system_fallbacks`). The web core embeds no font: the page
//! fetches the metric-compatible set (Carlito, Caladea, Liberation) as static assets (cached by
//! the browser and the service worker) and gives them to the core with [`register`] before the
//! first document opens, which keeps the WebAssembly binary inside its budget (B12). Latin text lays out and rasterises identically
//! either way, which the painter-parity test relies on.

use std::sync::{Arc, Mutex};

use kiln_text::FontStore;
#[cfg(target_arch = "wasm32")]
use kiln_text::FontStoreOptions;

/// Fonts the host registered at run time (the web core's document fonts).
static REGISTERED: Mutex<Vec<Arc<[u8]>>> = Mutex::new(Vec::new());

/// Most fonts a host may register (a guard against a page registering in a loop): the document
/// fonts, the Arabic, symbol, mathematics and emoji fallbacks and every slice of the CJK font.
const MAX_REGISTERED: usize = 160;

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

/// How many fonts the host has registered (they only ever add up).
#[must_use]
pub fn registered_count() -> usize {
    REGISTERED.lock().map(|f| f.len()).unwrap_or(0)
}

/// The fonts registered after the first `n`.
#[must_use]
pub fn registered_after(n: usize) -> Vec<Arc<[u8]>> {
    REGISTERED
        .lock()
        .map(|f| f.get(n..).map(<[Arc<[u8]>]>::to_vec).unwrap_or_default())
        .unwrap_or_default()
}

fn registered() -> Vec<Arc<[u8]>> {
    REGISTERED.lock().map(|f| f.clone()).unwrap_or_default()
}

/// The font store for a document.
#[must_use]
pub fn store() -> FontStore {
    // Natively the platform's own CJK and emoji fonts are the fallback for text no bundled font
    // has (the web core fetches slices of Noto Sans CJK and Noto Emoji instead).
    #[cfg(not(target_arch = "wasm32"))]
    let mut fonts = {
        let mut f = FontStore::bundled();
        f.add_system_fallbacks();
        f
    };
    #[cfg(target_arch = "wasm32")]
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: false,
        system: false,
    });
    let registered = registered();
    for f in &registered {
        fonts.add_font_data(f.to_vec());
    }
    if !registered.is_empty() {
        // The fallback chains were installed before these families existed.
        fonts.refresh_fallbacks();
    }
    fonts
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    /// The desktop and Android store: Chinese, Japanese, Korean and emoji lay out with the
    /// platform's own fonts (fontconfig's here), with nothing missing.
    #[test]
    fn the_native_store_lays_out_cjk_and_emoji_with_the_platforms_fonts() {
        let mut fonts = super::store();
        let found: Vec<String> = kiln_text::SYSTEM_FALLBACKS
            .iter()
            .filter(|f| fonts.has_family(f))
            .map(|f| (*f).to_owned())
            .collect();
        println!("NATIVE fallbacks found: {found:?}");
        // A machine with no CJK font at all (a bare build server) has nothing to fall back to.
        if found.is_empty() {
            println!("NATIVE: this machine has none of the platform fallback fonts");
            return;
        }
        let mut engine = kiln_text::TextEngine::new(fonts);
        let _ = kiln_text::take_missing();
        let layout = engine.layout(&kiln_text::ParagraphSpec::plain(
            "日本語 中文 한국어 かな 😀🎉 ✅ 🇯🇵",
            kiln_text::SpanStyle::new("Calibri", 11.0),
            1.0e6,
        ));
        let boxes = layout
            .lines
            .iter()
            .flat_map(|l| l.runs.iter().flat_map(|r| r.glyphs.iter()))
            .filter(|g| g.id == 0)
            .count();
        let kinds = kiln_text::take_missing();
        println!("NATIVE missing kinds {kinds}, boxes {boxes}");
        assert_eq!(kinds, 0);
        assert_eq!(boxes, 0);
    }
}
