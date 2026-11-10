// SPDX-License-Identifier: AGPL-3.0-or-later
//! The user dictionary (Add to Dictionary): one value of the app storage's `Dictionary` area,
//! the words one per line in UTF-8, sorted, at most [`MAX_WORDS`].

/// The key of the user dictionary.
pub const KEY: &str = "user";
/// The most words kept.
pub const MAX_WORDS: usize = 10_000;

/// The words of a stored dictionary (blank lines dropped).
#[must_use]
pub fn decode(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    normalise(text.lines().map(str::to_owned).collect())
}

/// The stored form of a list of words.
#[must_use]
pub fn encode(words: &[String]) -> Vec<u8> {
    normalise(words.to_vec()).join("\n").into_bytes()
}

/// Adds a word; returns whether it was new.
pub fn add(words: &mut Vec<String>, word: &str) -> bool {
    let word = word.trim();
    if word.is_empty() || words.iter().any(|w| w == word) || words.len() >= MAX_WORDS {
        return false;
    }
    words.push(word.to_owned());
    words.sort();
    true
}

fn normalise(mut words: Vec<String>) -> Vec<String> {
    for w in &mut words {
        *w = w.trim().to_owned();
    }
    words.retain(|w| !w.is_empty());
    words.sort();
    words.dedup();
    words.truncate(MAX_WORDS);
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_round_trip_sorted_and_unique() {
        let mut words = decode(b"kiln\n\nglaze\r\nkiln\n  bisque  ");
        assert_eq!(words, ["bisque", "glaze", "kiln"]);
        assert!(add(&mut words, "Ünïcode"));
        assert!(!add(&mut words, "kiln") && !add(&mut words, "  "));
        assert_eq!(decode(&encode(&words)), words);
    }
}
