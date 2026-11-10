// SPDX-License-Identifier: AGPL-3.0-or-later
//! The interface's document and brand fonts, served from the ones the native core already embeds
//! (`kiln_text::bundled_font`, the brand fonts of `kiln-deck-layout`), so the executable carries
//! each font once. [`FontAssets`] answers `/fonts/<file>` on the app's own origin and passes every
//! other path to the embedded frontend; `apps/desktop/scripts/frontend.mjs` points the
//! interface's `@font-face` rules there and leaves those files out of the frontend.

use std::borrow::Cow;

use tauri::utils::assets::{AssetKey, AssetsIter, CspHash};
use tauri::{Assets, Runtime};

/// The path prefix of the fonts (`/fonts/Carlito-Regular.ttf`).
pub const PREFIX: &str = "/fonts/";

/// A font the core embeds, by file name.
#[must_use]
pub fn font(file: &str) -> Option<&'static [u8]> {
    kiln_text::bundled_font(file).or_else(|| {
        kiln_deck_layout::fonts::BRAND_FONTS
            .iter()
            .find(|(name, _)| *name == file)
            .map(|(_, bytes)| *bytes)
    })
}

/// No assets: the placeholder while the embedded ones are taken out to be wrapped.
#[derive(Debug, Clone, Copy)]
pub struct NoAssets;

impl<R: Runtime> Assets<R> for NoAssets {
    fn get(&self, _key: &AssetKey) -> Option<Cow<'_, [u8]>> {
        None
    }

    fn iter(&self) -> Box<AssetsIter<'_>> {
        Box::new(std::iter::empty())
    }

    fn csp_hashes(&self, _html_path: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        Box::new(std::iter::empty())
    }
}

/// The embedded frontend with the core's fonts under [`PREFIX`].
pub struct FontAssets<R: Runtime> {
    inner: Box<dyn Assets<R>>,
}

impl<R: Runtime> std::fmt::Debug for FontAssets<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FontAssets")
    }
}

impl<R: Runtime> FontAssets<R> {
    /// Wraps the frontend.
    #[must_use]
    pub fn new(inner: Box<dyn Assets<R>>) -> Self {
        Self { inner }
    }
}

impl<R: Runtime> Assets<R> for FontAssets<R> {
    fn setup(&self, app: &tauri::App<R>) {
        self.inner.setup(app);
    }

    fn get(&self, key: &AssetKey) -> Option<Cow<'_, [u8]>> {
        if let Some(file) = key.as_ref().strip_prefix(PREFIX)
            && let Some(bytes) = font(file)
        {
            return Some(Cow::Borrowed(bytes));
        }
        self.inner.get(key)
    }

    fn iter(&self) -> Box<AssetsIter<'_>> {
        self.inner.iter()
    }

    fn csp_hashes(&self, html_path: &AssetKey) -> Box<dyn Iterator<Item = CspHash<'_>> + '_> {
        self.inner.csp_hashes(html_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_the_fonts_the_core_embeds_and_the_frontend_points_at() {
        assert!(font("Carlito-Regular.ttf").is_some_and(|b| b.len() > 100_000));
        assert!(font("Figtree-VF.ttf").is_some());
        assert!(font("NotoNaskhArabic-Bold.ttf").is_some());
        assert!(font("../Cargo.toml").is_none());
        let script = include_str!("../../scripts/frontend.mjs");
        let listed = script
            .split('\'')
            .filter(|s| s.ends_with(".ttf") && !s.contains('/'))
            .collect::<Vec<_>>();
        assert_eq!(listed.len(), 26);
        for name in listed {
            assert!(font(name).is_some(), "{name}");
        }
        assert!(script.contains("'/fonts/'"));
    }
}
