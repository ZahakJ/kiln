// SPDX-License-Identifier: AGPL-3.0-or-later
//! The font store: bundled OFL fonts, optional system fonts, family resolution through the
//! substitution table, script fallback chains, interned font instances and the metrics cache.

use std::collections::HashMap;
use std::sync::Arc;

use parley::FontContext;
use parley::fontique::{
    Attributes, Blob, Collection, CollectionOptions, FallbackKey, FontStyle, FontWeight, FontWidth,
    GenericFamily, QueryFamily, Script, SourceCache,
};
use skrifa::MetadataProvider;
use skrifa::instance::{LocationRef, Size};

use crate::metrics::{MetricsCache, ScaledMetrics, UnitMetrics};
use crate::substitution::{self, SubstitutionKind, Target};

/// Identifies one font instance (font data + face index + variation coordinates + synthesis) inside
/// a [`FontStore`]. Ids are dense, start at 0 and stay valid for the store's lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FontId(pub u32);

/// A font instance referenced by glyph runs.
#[derive(Clone)]
pub struct FontInstance {
    /// The font file (shared, cheap to clone).
    pub data: Blob<u8>,
    /// Face index inside a collection file.
    pub index: u32,
    /// Normalised variation coordinates (2.14 fixed point), empty for static fonts.
    pub coords: Vec<i16>,
    /// Synthetic bold requested (the family has no bold face).
    pub embolden: bool,
    /// Synthetic oblique angle in degrees, when the family has no italic face.
    pub skew: Option<f32>,
    /// Family name recorded in the font.
    pub family: String,
}

impl std::fmt::Debug for FontInstance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontInstance")
            .field("family", &self.family)
            .field("index", &self.index)
            .field("bytes", &self.data.len())
            .field("coords", &self.coords)
            .field("embolden", &self.embolden)
            .field("skew", &self.skew)
            .finish()
    }
}

impl FontInstance {
    /// The raw font file.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.data.data()
    }
}

/// How a [`FontStore`] is populated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontStoreOptions {
    /// Register the fonts compiled in with the `bundled-fonts` feature.
    pub bundled: bool,
    /// Discover the platform's fonts (needs the `system-fonts` feature; ignored otherwise).
    pub system: bool,
    /// Directories whose fonts are loaded at start-up (e.g. `assets/fonts` when the fonts are not
    /// compiled in).
    #[cfg(not(target_family = "wasm"))]
    pub directories: Vec<std::path::PathBuf>,
}

impl Default for FontStoreOptions {
    fn default() -> Self {
        Self {
            bundled: true,
            system: cfg!(feature = "system-fonts"),
            #[cfg(not(target_family = "wasm"))]
            directories: Vec::new(),
        }
    }
}

/// The outcome of resolving a document's family name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedFamily {
    /// The name the document asked for.
    pub requested: String,
    /// The family that will be used.
    pub family: String,
    /// How the request was satisfied.
    pub kind: SubstitutionKind,
}

/// Script fallback chains installed by default. Only families that exist are used.
const DEFAULT_FALLBACKS: &[([u8; 4], &[&str])] = &[
    (
        *b"Latn",
        &["Liberation Sans", "Carlito", "DejaVu Sans", "Noto Sans"],
    ),
    (*b"Grek", &["Liberation Sans", "DejaVu Sans", "Noto Sans"]),
    (*b"Cyrl", &["Liberation Sans", "DejaVu Sans", "Noto Sans"]),
    (
        *b"Hebr",
        &["Liberation Sans", "Noto Sans Hebrew", "Arial", "David"],
    ),
    (
        *b"Arab",
        &[
            "Noto Naskh Arabic",
            "Noto Sans Arabic",
            "Arial",
            "Tahoma",
            "Segoe UI",
            "Geeza Pro",
            "DejaVu Sans",
        ],
    ),
    (
        *b"Hani",
        &[
            "Noto Sans CJK SC",
            "Noto Sans SC",
            "Source Han Sans SC",
            "Microsoft YaHei",
            "PingFang SC",
            "Noto Serif CJK SC",
            "Droid Sans Fallback",
        ],
    ),
    (
        *b"Hira",
        &[
            "Noto Sans CJK JP",
            "Noto Sans JP",
            "Yu Gothic",
            "Hiragino Sans",
            "Meiryo",
            "Droid Sans Fallback",
        ],
    ),
    (
        *b"Kana",
        &[
            "Noto Sans CJK JP",
            "Noto Sans JP",
            "Yu Gothic",
            "Hiragino Sans",
            "Meiryo",
            "Droid Sans Fallback",
        ],
    ),
    (
        *b"Hang",
        &[
            "Noto Sans CJK KR",
            "Noto Sans KR",
            "Malgun Gothic",
            "Apple SD Gothic Neo",
            "Droid Sans Fallback",
        ],
    ),
    (*b"Thai", &["Noto Sans Thai", "Tahoma", "Leelawadee UI"]),
    (*b"Deva", &["Noto Sans Devanagari", "Nirmala UI", "Mangal"]),
    (
        *b"Zsye",
        &["Noto Color Emoji", "Segoe UI Emoji", "Apple Color Emoji"],
    ),
];

/// Key of an interned font instance.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct InstanceKey {
    blob: u64,
    index: u32,
    coords: Vec<i16>,
    embolden: bool,
    skew: Option<u32>,
}

/// Fonts, family resolution and metrics. One per application (or per worker thread).
pub struct FontStore {
    pub(crate) cx: FontContext,
    instances: Vec<FontInstance>,
    instance_ids: HashMap<InstanceKey, FontId>,
    pub(crate) metrics: MetricsCache,
    resolved: HashMap<String, ResolvedFamily>,
    ui_sans: String,
    system: bool,
}

impl std::fmt::Debug for FontStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontStore")
            .field("instances", &self.instances.len())
            .field("ui_sans", &self.ui_sans)
            .field("system", &self.system)
            .finish_non_exhaustive()
    }
}

impl Default for FontStore {
    fn default() -> Self {
        Self::new(&FontStoreOptions::default())
    }
}

impl FontStore {
    /// A store populated according to `options`.
    #[must_use]
    pub fn new(options: &FontStoreOptions) -> Self {
        let system = options.system && cfg!(feature = "system-fonts");
        let collection = Collection::new(CollectionOptions {
            shared: false,
            system_fonts: system,
        });
        let mut store = Self {
            cx: FontContext {
                collection,
                source_cache: SourceCache::default(),
            },
            instances: Vec::new(),
            instance_ids: HashMap::new(),
            metrics: MetricsCache::default(),
            resolved: HashMap::new(),
            ui_sans: "Liberation Sans".to_owned(),
            system,
        };
        if options.bundled {
            store.register_bundled();
        }
        #[cfg(not(target_family = "wasm"))]
        for dir in &options.directories {
            store.load_directory(dir);
        }
        store.install_generic_families();
        store.install_default_fallbacks();
        store
    }

    /// Bundled fonts only: deterministic on every platform (tests, PDF export, the web build).
    #[must_use]
    pub fn bundled() -> Self {
        Self::new(&FontStoreOptions {
            bundled: true,
            system: false,
            #[cfg(not(target_family = "wasm"))]
            directories: Vec::new(),
        })
    }

    /// Whether system fonts are being used.
    #[must_use]
    pub fn uses_system_fonts(&self) -> bool {
        self.system
    }

    fn register_bundled(&mut self) {
        #[cfg(feature = "bundled-fonts")]
        for (_, bytes) in crate::bundled::FONTS {
            let blob = Blob::new(Arc::new(*bytes));
            self.cx.collection.register_fonts(blob, None);
        }
    }

    /// Registers fonts from memory (a TTF/OTF/TTC file). Returns the family names it added.
    pub fn add_font_data(&mut self, data: Vec<u8>) -> Vec<String> {
        let blob = Blob::new(Arc::new(data));
        let added = self.cx.collection.register_fonts(blob, None);
        self.resolved.clear();
        let mut names = Vec::new();
        for (id, _) in added {
            if let Some(name) = self.cx.collection.family_name(id)
                && !names.iter().any(|n: &String| n == name)
            {
                names.push(name.to_owned());
            }
        }
        names
    }

    /// Loads every font file in `dir` (recursively). Returns the number of families afterwards.
    #[cfg(not(target_family = "wasm"))]
    pub fn load_directory(&mut self, dir: impl AsRef<std::path::Path>) -> usize {
        self.cx.collection.load_fonts_from_paths([dir.as_ref()]);
        self.resolved.clear();
        self.cx.collection.family_names().count()
    }

    /// All family names known to the store (bundled, added and, if enabled, system).
    pub fn family_names(&mut self) -> Vec<String> {
        let mut names: Vec<String> = self
            .cx
            .collection
            .family_names()
            .map(str::to_owned)
            .collect();
        names.sort_unstable();
        names.dedup();
        names
    }

    /// Whether a family with this name exists (case-insensitive, as fontique matches names).
    pub fn has_family(&mut self, name: &str) -> bool {
        self.cx.collection.family_id(name).is_some()
    }

    /// The family used in place of Aptos and Segoe UI (the UI sans of the brand kit, set by the
    /// design tokens). Defaults to Liberation Sans.
    #[must_use]
    pub fn ui_sans(&self) -> &str {
        &self.ui_sans
    }

    /// Sets the UI sans family (see [`FontStore::ui_sans`]).
    pub fn set_ui_sans(&mut self, family: impl Into<String>) {
        self.ui_sans = family.into();
        self.resolved.clear();
    }

    fn install_generic_families(&mut self) {
        for (generic, name) in [
            (GenericFamily::SansSerif, "Liberation Sans"),
            (GenericFamily::Serif, "Liberation Serif"),
            (GenericFamily::Monospace, "Liberation Mono"),
            (GenericFamily::SystemUi, "Liberation Sans"),
        ] {
            if let Some(id) = self.cx.collection.family_id(name) {
                self.cx
                    .collection
                    .set_generic_families(generic, std::iter::once(id));
            }
        }
    }

    fn install_default_fallbacks(&mut self) {
        for (script, families) in DEFAULT_FALLBACKS {
            self.set_script_fallbacks(*script, families);
        }
    }

    /// Sets the fallback chain for an ISO 15924 script tag (e.g. `*b"Arab"`), keeping only families
    /// that exist. Returns how many were found; when none exist the chain is left unchanged (system
    /// fallback, if enabled, still applies).
    pub fn set_script_fallbacks(&mut self, script: [u8; 4], families: &[&str]) -> usize {
        let ids: Vec<_> = families
            .iter()
            .filter_map(|name| self.cx.collection.family_id(name))
            .collect();
        if !ids.is_empty() {
            let key = FallbackKey::new(Script::from_bytes(script), None);
            self.cx.collection.set_fallbacks(key, ids.iter().copied());
        }
        ids.len()
    }

    /// The fallback chain currently used for a script.
    pub fn script_fallbacks(&mut self, script: [u8; 4]) -> Vec<String> {
        let key = FallbackKey::new(Script::from_bytes(script), None);
        let ids: Vec<_> = self.cx.collection.fallback_families(key).collect();
        ids.into_iter()
            .filter_map(|id| self.cx.collection.family_name(id).map(str::to_owned))
            .collect()
    }

    /// Resolves a document's family name: the family itself if it exists, else the substitution
    /// table, else a bundled default chosen from the name's style.
    pub fn resolve(&mut self, requested: &str) -> ResolvedFamily {
        if let Some(r) = self.resolved.get(requested) {
            return r.clone();
        }
        let resolved = self.resolve_uncached(requested);
        self.resolved.insert(requested.to_owned(), resolved.clone());
        resolved
    }

    /// Like [`FontStore::resolve`], without cloning the result.
    pub fn resolve_ref(&mut self, requested: &str) -> &ResolvedFamily {
        if !self.resolved.contains_key(requested) {
            let resolved = self.resolve_uncached(requested);
            self.resolved.insert(requested.to_owned(), resolved);
        }
        &self.resolved[requested]
    }

    fn resolve_uncached(&mut self, requested: &str) -> ResolvedFamily {
        let name = requested.trim();
        let make = |family: &str, kind| ResolvedFamily {
            requested: requested.to_owned(),
            family: family.to_owned(),
            kind,
        };
        if !name.is_empty() && self.has_family(name) {
            return make(name, SubstitutionKind::Exact);
        }
        if let Some(rule) = substitution::lookup(name) {
            match rule.target {
                Target::Families(candidates) => {
                    if let Some(found) = candidates.iter().find(|c| self.has_family(c)) {
                        return make(found, rule.kind);
                    }
                }
                Target::UiSans => {
                    let ui = self.ui_sans.clone();
                    if self.has_family(&ui) {
                        return make(&ui, rule.kind);
                    }
                }
                Target::Symbol | Target::ScriptFallback => {
                    let family = self.available_default(substitution::StyleGuess::Sans);
                    return make(&family, rule.kind);
                }
            }
        }
        let guess = substitution::guess_style(name);
        let family = self.available_default(guess);
        make(&family, SubstitutionKind::Missing)
    }

    /// The default family for a style when it is loaded; otherwise the first loaded family of
    /// the Latin fallback chain, or any loaded family. A store with fewer fonts (the web core
    /// embeds Carlito only) must not resolve to a family it does not have: text in that family,
    /// such as a bullet in Symbol, would find no font for characters of no particular script.
    fn available_default(&mut self, style: substitution::StyleGuess) -> String {
        let preferred = substitution::default_family(style);
        if self.has_family(preferred) {
            return preferred.to_owned();
        }
        let latin = DEFAULT_FALLBACKS
            .iter()
            .find(|(script, _)| script == b"Latn")
            .map_or(&[][..], |(_, families)| *families);
        if let Some(found) = latin.iter().find(|f| self.has_family(f)) {
            return (*found).to_owned();
        }
        self.family_names()
            .into_iter()
            .next()
            .unwrap_or_else(|| preferred.to_owned())
    }

    /// The font instance behind an id.
    #[must_use]
    pub fn font(&self, id: FontId) -> Option<&FontInstance> {
        self.instances.get(id.0 as usize)
    }

    /// Number of interned font instances.
    #[must_use]
    pub fn font_count(&self) -> usize {
        self.instances.len()
    }

    /// Interns a font instance and returns its id.
    pub(crate) fn intern(
        &mut self,
        data: &Blob<u8>,
        index: u32,
        coords: &[i16],
        embolden: bool,
        skew: Option<f32>,
    ) -> FontId {
        let key = InstanceKey {
            blob: data.id(),
            index,
            coords: coords.to_vec(),
            embolden,
            skew: skew.map(f32::to_bits),
        };
        if let Some(id) = self.instance_ids.get(&key) {
            return *id;
        }
        let id = FontId(u32::try_from(self.instances.len()).unwrap_or(u32::MAX));
        let unit = UnitMetrics::read(data.data(), index);
        let family = skrifa::FontRef::from_index(data.data(), index)
            .ok()
            .and_then(|f| {
                f.localized_strings(skrifa::string::StringId::FAMILY_NAME)
                    .english_or_first()
                    .map(|s| s.chars().collect::<String>())
            })
            .unwrap_or_default();
        self.metrics.insert_font(id.0, unit);
        self.instances.push(FontInstance {
            data: data.clone(),
            index,
            coords: coords.to_vec(),
            embolden,
            skew,
            family,
        });
        self.instance_ids.insert(key, id);
        id
    }

    /// The first font of a family for the given weight and style (used to size empty lines).
    pub fn primary_font(&mut self, family: &str, weight: u16, italic: bool) -> Option<FontId> {
        let attrs = Attributes::new(
            FontWidth::NORMAL,
            if italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            FontWeight::new(f32::from(weight)),
        );
        let found = {
            let mut query = self.cx.collection.query(&mut self.cx.source_cache);
            query.set_families([
                QueryFamily::Named(family),
                QueryFamily::Generic(GenericFamily::SansSerif),
            ]);
            query.set_attributes(attrs);
            query.first_available_font()
        }?;
        Some(self.intern(
            &found.blob,
            found.index,
            &[],
            found.synthesis.embolden(),
            found.synthesis.skew(),
        ))
    }

    /// Unscaled metrics of a font.
    #[must_use]
    pub fn unit_metrics(&self, id: FontId) -> Option<&UnitMetrics> {
        self.metrics.units(id.0)
    }

    /// Metrics of a font at a size in px (cached per (font, size)).
    pub fn metrics(&mut self, id: FontId, size: f32) -> ScaledMetrics {
        self.metrics.scaled(id.0, size)
    }

    /// Number of (font, size) entries in the metrics cache.
    #[must_use]
    pub fn metrics_cache_len(&self) -> usize {
        self.metrics.scaled_len()
    }

    /// The nominal glyph for a character and its advance at `size` px, if the font maps it.
    #[must_use]
    pub fn glyph_for_char(&self, id: FontId, ch: char, size: f32) -> Option<(u32, f32)> {
        let inst = self.font(id)?;
        let font = skrifa::FontRef::from_index(inst.bytes(), inst.index).ok()?;
        let gid = font.charmap().map(ch)?;
        let coords = skrifa_coords(&inst.coords);
        let advance = font
            .glyph_metrics(Size::new(size), LocationRef::new(&coords))
            .advance_width(gid)?;
        Some((gid.to_u32(), advance))
    }
}

/// Converts 2.14 fixed-point coordinates to skrifa's representation.
pub(crate) fn skrifa_coords(coords: &[i16]) -> Vec<skrifa::instance::NormalizedCoord> {
    coords
        .iter()
        .map(|c| skrifa::instance::NormalizedCoord::from_bits(*c))
        .collect()
}
