// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-spell
//!
//! Spell checking for Kiln on [`spellbook`] 0.4.2 (Hunspell-compatible) with the bundled en-US
//! dictionary (`assets/dictionaries/en-US`, SCOWL licence; sources in
//! `assets/dictionaries/SOURCES.md`).
//!
//! - **Words** ([`words`]): Unicode word boundaries (UAX #29) over a paragraph's text, with Word's
//!   default exceptions: words with digits, words in UPPERCASE, single letters, Internet and file
//!   addresses, and the `U+FFFC` placeholders of inline objects are not checked.
//! - **Checking** ([`Speller::misspellings`], [`Speller::is_correct`]): the dictionary, then the
//!   user's dictionary (Add to Dictionary) and the session's Ignore All list. Curly apostrophes
//!   count as straight ones, so "don’t" is a word.
//! - **Languages** ([`checks_language`]): only English (United States) has a dictionary; text
//!   marked as another language is not checked (it is not flagged as misspelled either).
//! - **Suggestions** ([`Speller::suggest`]): spellbook's Hunspell suggestions, in the case of the
//!   word (a capitalised word gets capitalised suggestions).
//!
//! The dictionary is parsed once per process, on first use ([`dictionary`]); the web core gets
//! its files from the page ([`load_dictionary`]) instead of carrying them in the binary.
//!
//! ```
//! let speller = kiln_spell::Speller::new();
//! let found = speller.misspellings("Fire the kiln, teh glaze sets.");
//! assert_eq!(found.len(), 1);
//! assert_eq!(found[0].word, "teh");
//! assert!(speller.suggest("teh").iter().any(|s| s == "the"));
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::collections::BTreeSet;
use std::sync::OnceLock;

use unicode_segmentation::UnicodeSegmentation;

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The en-US affix file (native builds embed it; the web core loads it with
/// [`load_dictionary`] so it stays out of the WebAssembly binary, budget B12).
#[cfg(not(target_arch = "wasm32"))]
const EN_US_AFF: &str = include_str!("../../../assets/dictionaries/en-US/en_US.aff");
/// The en-US word list.
#[cfg(not(target_arch = "wasm32"))]
const EN_US_DIC: &str = include_str!("../../../assets/dictionaries/en-US/en_US.dic");

/// Longest word the speller looks at; longer runs of letters are not words (and suggestions for
/// them would be slow).
pub const MAX_WORD_CHARS: usize = 48;
/// Most suggestions returned for one word (Word shows up to five in its context menu).
pub const MAX_SUGGESTIONS: usize = 6;
/// Longest word a user dictionary accepts.
pub const MAX_USER_WORD_CHARS: usize = 64;

static DICT: OnceLock<Option<spellbook::Dictionary>> = OnceLock::new();

/// The en-US dictionary: natively the bundled one, parsed on first use; in the web core the one
/// [`load_dictionary`] was given. `None` before that (or if the files could not be parsed, a
/// build defect the unit tests catch), in which case nothing is flagged.
#[must_use]
pub fn dictionary() -> Option<&'static spellbook::Dictionary> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        DICT.get_or_init(|| spellbook::Dictionary::new(EN_US_AFF, EN_US_DIC).ok())
            .as_ref()
    }
    #[cfg(target_arch = "wasm32")]
    {
        DICT.get().and_then(Option::as_ref)
    }
}

/// Whether the dictionary is available (the web core needs [`load_dictionary`] first).
#[must_use]
pub fn has_dictionary() -> bool {
    dictionary().is_some()
}

/// Gives the speller the en-US dictionary's files (the web core fetches them as static assets).
/// Returns `Ok(true)` when they were loaded now, `Ok(false)` when a dictionary was already
/// there (native builds embed it).
///
/// # Errors
///
/// A message when the files are not a Hunspell dictionary.
pub fn load_dictionary(aff: &str, dic: &str) -> Result<bool, String> {
    if dictionary().is_some() {
        return Ok(false);
    }
    let parsed = spellbook::Dictionary::new(aff, dic).map_err(|e| e.to_string())?;
    Ok(DICT.set(Some(parsed)).is_ok())
}

/// Whether text in a language (a BCP 47 tag such as `en-US`, as in `w:lang`) is checked. `None`
/// (no language set anywhere) is checked as English (United States), Word's default. Only
/// `en-US` (and the bare `en`) has a dictionary; every other language is left unchecked rather
/// than flagged.
#[must_use]
pub fn checks_language(lang: Option<&str>) -> bool {
    match lang {
        None => true,
        Some(tag) => {
            let t = tag.trim().replace('_', "-").to_ascii_lowercase();
            t.is_empty() || t == "en" || t == "en-us"
        }
    }
}

/// A word of a text: char offsets and the word as checked (curly apostrophes made straight).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// First char.
    pub start: usize,
    /// One past the last char.
    pub end: usize,
    /// The word as the speller sees it.
    pub word: String,
}

/// A misspelled word.
pub type Misspelling = Word;

/// Char ranges of addresses (URLs, e-mail addresses, file paths) in a text: whitespace-separated
/// tokens with `://`, `@`, `www.` or a backslash.
fn address_spans(text: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start = None;
    let mut token = String::new();
    let chars: Vec<char> = text.chars().collect();
    for (i, c) in chars.iter().copied().chain(std::iter::once(' ')).enumerate() {
        if c.is_whitespace() || c == '\u{FFFC}' {
            if let Some(s) = start.take() {
                let lower = token.to_ascii_lowercase();
                if lower.contains("://")
                    || lower.starts_with("www.")
                    || (lower.contains('@') && lower.contains('.'))
                    || lower.contains('\\')
                    || lower.starts_with("mailto:")
                {
                    out.push((s, i));
                }
                token.clear();
            }
        } else {
            if start.is_none() {
                start = Some(i);
            }
            token.push(c);
        }
    }
    out
}

/// The words of a text that the speller checks, in order. Skipped (as in Word's defaults):
/// words with digits or underscores, words in UPPERCASE, single letters, addresses, and words
/// longer than [`MAX_WORD_CHARS`]. Leading and trailing apostrophes are not part of a word.
#[must_use]
pub fn words(text: &str) -> Vec<Word> {
    let addresses = address_spans(text);
    let mut out = Vec::new();
    // Byte → char offsets as we go (split_word_bound_indices gives bytes).
    let mut char_at = 0usize;
    let mut byte_at = 0usize;
    for (byte, w) in text.split_word_bound_indices() {
        char_at += text[byte_at..byte].chars().count();
        byte_at = byte;
        let n = w.chars().count();
        let start = char_at;
        if !w.chars().any(char::is_alphabetic) {
            continue;
        }
        // Trim apostrophes at the ends ('quoted' words); keep inner ones (don't).
        let is_apos = |c: char| c == '\'' || c == '\u{2019}' || c == '\u{2018}';
        let lead = w.chars().take_while(|c| is_apos(*c)).count();
        let trail = w.chars().rev().take_while(|c| is_apos(*c)).count();
        if lead + trail >= n {
            continue;
        }
        let core: String = w.chars().skip(lead).take(n - lead - trail).collect();
        let (s, e) = (start + lead, start + n - trail);
        let len = e - s;
        if !(2..=MAX_WORD_CHARS).contains(&len) {
            continue;
        }
        if core.chars().any(|c| c.is_numeric() || c == '_') {
            continue;
        }
        let letters: Vec<char> = core.chars().filter(|c| c.is_alphabetic()).collect();
        if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
            continue;
        }
        if addresses.iter().any(|&(a, b)| s >= a && e <= b) {
            continue;
        }
        out.push(Word {
            start: s,
            end: e,
            word: core.replace(['\u{2019}', '\u{2018}'], "'"),
        });
    }
    out
}

/// Normalises a word for the user dictionary and the ignore list: trimmed, straight apostrophes.
fn normal(word: &str) -> String {
    word.trim().replace(['\u{2019}', '\u{2018}'], "'")
}

/// Why a word cannot go into the user dictionary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WordError {
    /// Empty, or only spaces.
    Empty,
    /// Longer than [`MAX_USER_WORD_CHARS`].
    TooLong,
    /// Contains whitespace or a control character.
    NotAWord,
}

impl std::fmt::Display for WordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "a word is needed",
            Self::TooLong => "the word is too long for the dictionary",
            Self::NotAWord => "a dictionary word has no spaces or control characters",
        })
    }
}

impl std::error::Error for WordError {}

/// Checks a word for the user dictionary.
///
/// # Errors
///
/// [`WordError`] for an empty, overlong or multi-word entry.
pub fn valid_user_word(word: &str) -> Result<String, WordError> {
    let w = normal(word);
    if w.is_empty() {
        return Err(WordError::Empty);
    }
    if w.chars().count() > MAX_USER_WORD_CHARS {
        return Err(WordError::TooLong);
    }
    if w.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(WordError::NotAWord);
    }
    Ok(w)
}

/// A speller: the bundled dictionary, the user's dictionary and an Ignore All list.
#[derive(Debug, Clone, Default)]
pub struct Speller {
    /// Words the user added (Add to Dictionary), exact and as typed.
    user: BTreeSet<String>,
    /// Lower-cased user words: a word added in lower case is also correct capitalised.
    user_folded: BTreeSet<String>,
    /// Ignore All, for the session.
    ignored: BTreeSet<String>,
}

impl Speller {
    /// A speller with an empty user dictionary.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the user dictionary (the words the UI keeps in the user's preferences); invalid
    /// entries are skipped.
    pub fn set_user_words<I, S>(&mut self, words: I)
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.user.clear();
        self.user_folded.clear();
        for w in words {
            if let Ok(w) = valid_user_word(w.as_ref()) {
                self.user_folded.insert(w.to_lowercase());
                self.user.insert(w);
            }
        }
    }

    /// Add to Dictionary.
    ///
    /// # Errors
    ///
    /// [`WordError`] for an entry that is not a word.
    pub fn add_user_word(&mut self, word: &str) -> Result<bool, WordError> {
        let w = valid_user_word(word)?;
        self.user_folded.insert(w.to_lowercase());
        Ok(self.user.insert(w))
    }

    /// Removes a word from the user dictionary.
    pub fn remove_user_word(&mut self, word: &str) -> bool {
        let w = normal(word);
        let removed = self.user.remove(&w);
        if removed {
            self.user_folded = self.user.iter().map(|u| u.to_lowercase()).collect();
        }
        removed
    }

    /// The user dictionary, sorted.
    #[must_use]
    pub fn user_words(&self) -> Vec<String> {
        self.user.iter().cloned().collect()
    }

    /// Ignore All: the word is correct for the rest of the session.
    pub fn ignore_all(&mut self, word: &str) -> bool {
        let w = normal(word);
        !w.is_empty() && self.ignored.insert(w)
    }

    /// The Ignore All list.
    #[must_use]
    pub fn ignored(&self) -> Vec<String> {
        self.ignored.iter().cloned().collect()
    }

    /// Whether one word is spelled correctly (dictionary, user dictionary or ignored).
    #[must_use]
    pub fn is_correct(&self, word: &str) -> bool {
        let w = normal(word);
        if w.is_empty() || self.ignored.contains(&w) || self.user.contains(&w) {
            return true;
        }
        // A user word typed in lower case also covers its capitalised and upper-case forms
        // (Hunspell's rule for dictionary words).
        let lower = w.to_lowercase();
        let capitalised_or_upper = w.chars().next().is_some_and(char::is_uppercase)
            && (w.chars().skip(1).all(|c| !c.is_uppercase())
                || w.chars().all(|c| !c.is_lowercase()));
        if capitalised_or_upper && self.user_folded.contains(&lower) && self.user.contains(&lower) {
            return true;
        }
        match dictionary() {
            Some(d) => d.check(&w),
            None => true,
        }
    }

    /// The misspelled words of a text, in order.
    #[must_use]
    pub fn misspellings(&self, text: &str) -> Vec<Misspelling> {
        words(text)
            .into_iter()
            .filter(|w| !self.is_correct(&w.word))
            .collect()
    }

    /// Up to [`MAX_SUGGESTIONS`] corrections for a word, best first, without duplicates.
    #[must_use]
    pub fn suggest(&self, word: &str) -> Vec<String> {
        let w = normal(word);
        if w.is_empty() || w.chars().count() > MAX_WORD_CHARS {
            return Vec::new();
        }
        let Some(d) = dictionary() else {
            return Vec::new();
        };
        // A capitalised word ("Teh" at a sentence start) gets the suggestions of its lower-case
        // form, capitalised: Hunspell ranks those better.
        let chars: Vec<char> = w.chars().collect();
        let capitalised = chars.first().is_some_and(|c| c.is_uppercase())
            && chars.iter().skip(1).all(|c| !c.is_uppercase());
        let base = if capitalised { w.to_lowercase() } else { w.clone() };
        let recase = |s: String| -> String {
            if !capitalised {
                return s;
            }
            let mut c = s.chars();
            c.next().map_or(s.clone(), |f| f.to_uppercase().chain(c).collect())
        };
        let mut found = Vec::new();
        // Two letters typed in the wrong order is the commonest typo: such fixes come first.
        let base_chars: Vec<char> = base.chars().collect();
        for i in 0..base_chars.len().saturating_sub(1) {
            let mut swapped = base_chars.clone();
            swapped.swap(i, i + 1);
            let s: String = swapped.into_iter().collect();
            if s != base && d.check(&s) && !found.contains(&s) {
                found.push(s);
            }
        }
        let mut hunspell = Vec::new();
        d.suggest(&base, &mut hunspell);
        found.extend(hunspell);
        let mut out: Vec<String> = Vec::new();
        for s in found.into_iter().map(recase) {
            if !out.contains(&s) && s != w {
                out.push(s);
            }
            if out.len() == MAX_SUGGESTIONS {
                break;
            }
        }
        // User words close to the misspelling (one edit away) come first: they are the
        // vocabulary the user cares about.
        let lower = w.to_lowercase();
        let mut extra: Vec<String> = self
            .user
            .iter()
            .filter(|u| edit_distance(&u.to_lowercase(), &lower) == 1)
            .cloned()
            .collect();
        extra.retain(|e| !out.contains(e));
        extra.extend(out);
        extra.truncate(MAX_SUGGESTIONS);
        extra
    }
}

/// Levenshtein distance (small words only; used to rank user words).
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut prev: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut cur = vec![i + 1; b.len() + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            cur[j + 1] = (prev[j] + cost).min(prev[j + 1] + 1).min(cur[j] + 1);
        }
        prev = cur;
    }
    prev[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }

    #[test]
    fn the_bundled_dictionary_parses() {
        let d = dictionary().expect("the en-US dictionary parses");
        assert!(d.check("the"));
        assert!(d.check("kiln"));
        assert!(!d.check("teh"));
    }

    #[test]
    fn flags_teh_and_not_the() {
        let s = Speller::new();
        let found = s.misspellings("teh cat sat on the mat");
        assert_eq!(
            found,
            vec![Word {
                start: 0,
                end: 3,
                word: "teh".into()
            }]
        );
        assert!(s.is_correct("the"));
        assert!(!s.is_correct("teh"));
        assert!(s.misspellings("The quick brown fox jumps over the lazy dog.").is_empty());
    }

    #[test]
    fn offsets_are_chars_not_bytes() {
        let s = Speller::new();
        // "Zoë" is not in en_US; the offsets after it count chars.
        let found = s.misspellings("Café — teh end");
        let teh = found.iter().find(|w| w.word == "teh").expect("teh flagged");
        assert_eq!((teh.start, teh.end), (7, 10));
    }

    #[test]
    fn word_skips_follow_words_defaults() {
        let w: Vec<String> = words("NASA flew 3D x y42 snake_case at https://kiln.example/teh and teh@kiln.org or C:\\teh\\dir 'quoted'")
            .into_iter()
            .map(|w| w.word)
            .collect();
        assert_eq!(w, vec!["flew", "at", "and", "or", "quoted"]);
    }

    #[test]
    fn apostrophes_and_objects() {
        let s = Speller::new();
        assert!(s.misspellings("don’t won't it's").is_empty());
        // Inline objects are U+FFFC placeholders and split words.
        let found = words("glaze\u{FFFC}fire");
        assert_eq!(found.len(), 2);
        assert_eq!((found[1].start, found[1].end), (6, 10));
    }

    #[test]
    fn suggestions_rank_the_obvious_fix() {
        let s = Speller::new();
        let sug = s.suggest("teh");
        assert!(sug.len() <= MAX_SUGGESTIONS);
        assert!(sug.iter().any(|x| x == "the"), "{sug:?}");
        let cap = s.suggest("Teh");
        assert!(cap.iter().any(|x| x == "The"), "{cap:?}");
        assert!(s.suggest("").is_empty());
    }

    #[test]
    fn user_dictionary_and_ignore_all() {
        let mut s = Speller::new();
        assert!(!s.is_correct("kilnware"));
        assert_eq!(s.add_user_word("kilnware"), Ok(true));
        assert_eq!(s.add_user_word("kilnware"), Ok(false));
        assert!(s.is_correct("kilnware"));
        assert!(s.is_correct("Kilnware"), "a lower-case user word covers its capitalised form");
        assert!(s.misspellings("kilnware glazes").is_empty());
        assert_eq!(s.user_words(), vec!["kilnware".to_owned()]);
        assert!(s.suggest("kilnwar").first().is_some_and(|w| w == "kilnware"));
        assert!(s.remove_user_word("kilnware"));
        assert!(!s.is_correct("kilnware"));

        assert!(s.ignore_all("Zahakian"));
        assert!(s.is_correct("Zahakian"));
        assert_eq!(s.ignored(), vec!["Zahakian".to_owned()]);

        s.set_user_words(["Kiln’s", "bad word", ""]);
        assert_eq!(s.user_words(), vec!["Kiln's".to_owned()]);
        assert_eq!(s.add_user_word("two words"), Err(WordError::NotAWord));
        assert_eq!(s.add_user_word("  "), Err(WordError::Empty));
        assert_eq!(s.add_user_word(&"a".repeat(65)), Err(WordError::TooLong));
    }

    #[test]
    fn only_english_us_is_checked() {
        assert!(checks_language(None));
        assert!(checks_language(Some("en-US")));
        assert!(checks_language(Some("en_us")));
        assert!(checks_language(Some("en")));
        assert!(!checks_language(Some("de-DE")));
        assert!(!checks_language(Some("en-GB")), "no en-GB dictionary: unchecked, not wrong");
    }

    #[test]
    fn loading_again_is_a_no_op_natively() {
        assert!(has_dictionary());
        assert_eq!(load_dictionary("", ""), Ok(false));
    }

    #[test]
    fn edit_distances() {
        assert_eq!(edit_distance("kiln", "kiln"), 0);
        assert_eq!(edit_distance("kiln", "kilns"), 1);
        assert_eq!(edit_distance("teh", "the"), 2);
    }
}
