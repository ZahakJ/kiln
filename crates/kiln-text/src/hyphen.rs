// SPDX-License-Identifier: AGPL-3.0-or-later
//! Automatic hyphenation with Knuth–Liang patterns (en-US and German 1996, from hyph-utf8 via the
//! `hyphenation` crate; licences in `data/hyphenation/README.md`).

/// Hyphenation dictionaries, loaded on first use.
#[derive(Debug, Default)]
pub(crate) struct Hyphenators {
    #[cfg(feature = "hyphenation")]
    en: Option<Option<hyphenation::Standard>>,
    #[cfg(feature = "hyphenation")]
    de: Option<Option<hyphenation::Standard>>,
}

/// Languages with built-in patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HyphLang {
    EnglishUs,
    German,
}

impl HyphLang {
    /// Picks patterns for a BCP 47 tag; untagged text is treated as English.
    pub(crate) fn from_tag(tag: Option<&str>) -> Option<Self> {
        let Some(tag) = tag else {
            return Some(Self::EnglishUs);
        };
        let primary = tag.split(['-', '_']).next().unwrap_or("");
        if primary.eq_ignore_ascii_case("en") {
            Some(Self::EnglishUs)
        } else if primary.eq_ignore_ascii_case("de") {
            Some(Self::German)
        } else {
            None
        }
    }
}

impl Hyphenators {
    /// Byte offsets inside `word` where it may be hyphenated (ascending). Empty when the language
    /// has no patterns or the feature is disabled.
    #[cfg_attr(not(feature = "hyphenation"), allow(clippy::unused_self))]
    pub(crate) fn points(&mut self, word: &str, lang: HyphLang) -> Vec<usize> {
        #[cfg(feature = "hyphenation")]
        {
            use hyphenation::{Hyphenator, Language, Load, Standard};
            let (slot, bytes, language): (_, &[u8], _) = match lang {
                HyphLang::EnglishUs => (
                    &mut self.en,
                    include_bytes!("../data/hyphenation/en-us.standard.bincode"),
                    Language::EnglishUS,
                ),
                HyphLang::German => (
                    &mut self.de,
                    include_bytes!("../data/hyphenation/de-1996.standard.bincode"),
                    Language::German1996,
                ),
            };
            let dict = slot.get_or_insert_with(|| {
                let mut reader = bytes;
                Standard::from_reader(language, &mut reader).ok()
            });
            match dict {
                Some(d) => d.hyphenate(word).breaks,
                None => Vec::new(),
            }
        }
        #[cfg(not(feature = "hyphenation"))]
        {
            let _ = (word, lang);
            Vec::new()
        }
    }
}

#[cfg(all(test, feature = "hyphenation"))]
mod tests {
    use super::*;

    #[test]
    fn english_and_german_patterns_load_and_hyphenate() {
        let mut h = Hyphenators::default();
        let en = h.points("hyphenation", HyphLang::EnglishUs);
        assert_eq!(en, vec![2, 6, 7], "hy-phen-a-tion");
        let de = h.points("Silbentrennung", HyphLang::German);
        assert!(de.contains(&6), "Silben-trennung: {de:?}");
        assert_eq!(HyphLang::from_tag(Some("de-DE")), Some(HyphLang::German));
        assert_eq!(HyphLang::from_tag(Some("ar")), None);
    }
}
