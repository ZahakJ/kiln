// SPDX-License-Identifier: AGPL-3.0-or-later
//! AutoCorrect as you type (File ▸ Options ▸ Proofing ▸ AutoCorrect Options): the replace list
//! (common typos and symbols such as `(c)`, which the chrome holds and sends with the user's
//! changes), capitalising the first letter of sentences and the names of days, and correcting
//! TWo INitial CApitals. Each correction is its own undo step: Ctrl+Z right after it brings back
//! what was typed, as in Word.

use std::collections::HashMap;

use kiln_protocol::commands::write_office::AutoCorrectEntry;

use super::super::WriteDoc;

/// The replace list, by what is typed.
#[derive(Debug, Default)]
pub(in crate::apps::write) struct AutoCorrectList {
    /// Words (letters, digits, apostrophes): replaced when the word ends.
    words: HashMap<String, String>,
    /// Entries ending in a symbol (`(c)`, `-->`): replaced as soon as their last character is
    /// typed, longest first.
    symbols: Vec<(Vec<char>, String)>,
}

impl AutoCorrectList {
    /// The list from the chrome's entries (empty or overlong entries are left out).
    pub(super) fn new(entries: &[AutoCorrectEntry]) -> Self {
        let mut out = Self::default();
        for e in entries {
            let from = e.from.trim();
            if from.is_empty() || from.chars().count() > 64 || e.to.chars().count() > 255 {
                continue;
            }
            if from.chars().last().is_some_and(word_char) {
                out.words.insert(from.to_owned(), e.to.clone());
            } else {
                out.symbols.push((from.chars().collect(), e.to.clone()));
            }
        }
        out.symbols.sort_by_key(|(f, _)| std::cmp::Reverse(f.len()));
        out
    }
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '\''
}

/// Words a sentence does not end after.
const ABBREVIATIONS: [&str; 22] = [
    "e.g", "i.e", "etc", "vs", "mr", "mrs", "ms", "dr", "prof", "st", "no", "fig", "approx", "cf",
    "al", "jr", "sr", "inc", "ltd", "co", "p", "pp",
];

const DAYS: [&str; 7] = [
    "monday",
    "tuesday",
    "wednesday",
    "thursday",
    "friday",
    "saturday",
    "sunday",
];

/// The replacement with the typed word's capitalisation (`Teh` → `The`, `TEH` → `THE`).
fn with_case(typed: &str, to: &str) -> String {
    let letters: Vec<char> = typed.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) {
        return to.to_uppercase();
    }
    if letters.first().is_some_and(|c| c.is_uppercase()) {
        let mut cs = to.chars();
        return cs.next().map_or_else(String::new, |f| {
            f.to_uppercase().chain(cs).collect::<String>()
        });
    }
    to.to_owned()
}

fn capitalised(word: &str) -> String {
    let mut cs = word.chars();
    cs.next()
        .map_or_else(String::new, |f| f.to_uppercase().chain(cs).collect())
}

/// Whether the word starting at `start` begins a sentence: the paragraph starts there, or the
/// text before it ends with `.`, `!` or `?` (not after an abbreviation or a number) and spaces.
fn starts_sentence(chars: &[char], start: usize) -> bool {
    let mut i = start;
    while i > 0 && matches!(chars[i - 1], ' ' | '\u{A0}' | '"' | '“' | '(' | '\'' | '‘') {
        i -= 1;
    }
    if i == 0 {
        return true;
    }
    if i == start {
        // No space before the word: inside a word or after a symbol.
        return false;
    }
    let end = chars[i - 1];
    if !matches!(end, '.' | '!' | '?') {
        return false;
    }
    if end == '.' {
        // The word before the full stop.
        let mut j = i - 1;
        while j > 0 && (chars[j - 1].is_alphanumeric() || chars[j - 1] == '.') {
            j -= 1;
        }
        let prev: String = chars[j..i - 1].iter().collect::<String>().to_lowercase();
        if prev.is_empty()
            || ABBREVIATIONS.contains(&prev.as_str())
            || (prev.chars().count() == 1 && prev.chars().all(char::is_alphabetic))
        {
            return false;
        }
        // A number ends a sentence ("fired to cone 6. The"), as in Word, except where it numbers
        // the paragraph ("6. the") or follows an abbreviation ("No. 6. the", "p. 6. the").
        if prev.chars().all(|c| c.is_ascii_digit()) {
            let mut k = j;
            while k > 0 && matches!(chars[k - 1], ' ' | '\u{A0}' | '\t') {
                k -= 1;
            }
            if k == 0 {
                return false;
            }
            if chars[k - 1] == '.' {
                let mut m = k - 1;
                while m > 0 && chars[m - 1].is_alphabetic() {
                    m -= 1;
                }
                let abbr: String = chars[m..k - 1].iter().collect::<String>().to_lowercase();
                if ABBREVIATIONS.contains(&abbr.as_str()) {
                    return false;
                }
            }
        }
    }
    true
}

/// Whether the text before `end` (back to the last space) is a web address, an email address or
/// a path: AutoCorrect leaves those alone (`www.teh.com`, `teh@kiln.org`, `C:\teh\`).
fn in_address(chars: &[char], end: usize) -> bool {
    let mut s = end;
    while s > 0 && !chars[s - 1].is_whitespace() {
        s -= 1;
    }
    let token: String = chars[s..end].iter().collect::<String>().to_lowercase();
    token.contains("://")
        || token.starts_with("www.")
        || token.contains('@')
        || token.contains('/')
        || token.contains('\\')
}

/// What AutoCorrect does with the word `word` that just ended (`None`: nothing).
pub(super) fn correct_word(
    list: &AutoCorrectList,
    o: &kiln_protocol::commands::write_features::AutoFormatOptions,
    chars: &[char],
    start: usize,
    word: &str,
) -> Option<String> {
    let on = |v: Option<bool>| v.unwrap_or(true);
    let mut out = word.to_owned();
    let mut changed = false;
    if on(o.replace_text) {
        let lower = word.to_lowercase();
        if let Some(to) = list.words.get(word).or_else(|| list.words.get(&lower)) {
            out = with_case(word, to);
            changed = true;
        }
    }
    let letters = || out.chars().filter(|c| c.is_alphabetic()).count();
    if !changed && on(o.two_initial_caps) && letters() >= 3 {
        let cs: Vec<char> = out.chars().collect();
        if cs.len() >= 3
            && cs[0].is_uppercase()
            && cs[1].is_uppercase()
            && cs[2..].iter().all(|c| c.is_lowercase())
        {
            out = std::iter::once(cs[0])
                .chain(cs[1].to_lowercase())
                .chain(cs[2..].iter().copied())
                .collect();
            changed = true;
        }
    }
    if on(o.capitalize_days) && DAYS.contains(&out.as_str()) {
        out = capitalised(&out);
        changed = true;
    }
    if on(o.capitalize_sentences)
        && out.chars().next().is_some_and(char::is_lowercase)
        && out.chars().all(word_char)
        && starts_sentence(chars, start)
    {
        out = capitalised(&out);
        changed = true;
    }
    (changed && out != word).then_some(out)
}

impl WriteDoc {
    /// AutoCorrect after a typed character: `true` when it changed the text.
    pub(in crate::apps::write) fn autocorrect_after(
        &mut self,
        c: char,
        chars: &[char],
        off: usize,
    ) -> kiln_common::Result<bool> {
        let o = self.features.autoformat.0;
        let at = self.head;
        let list = &self.office.autocorrect;
        // Nothing inside a web or email address or a path.
        if off >= 1 && in_address(chars, off - 1) {
            return Ok(false);
        }
        // Symbols as soon as they are complete: `(c)` → ©, `-->` → →.
        if o.replace_text.unwrap_or(true) && !word_char(c) {
            for (from, to) in &list.symbols {
                let n = from.len();
                if off >= n && chars[off - n..off] == from[..] {
                    let to = to.clone();
                    self.replace_before(at, n, &to, 0)?;
                    return Ok(true);
                }
            }
        }
        // Words, when the character after them ends them.
        if !(c.is_whitespace() || ".,;:!?)]}".contains(c)) || off < 2 {
            return Ok(false);
        }
        let end = off - 1;
        let mut start = end;
        while start > 0 && word_char(chars[start - 1]) {
            start -= 1;
        }
        if start == end {
            return Ok(false);
        }
        let word: String = chars[start..end].iter().collect();
        let word = word.trim_matches('\'').to_owned();
        if word.is_empty() || chars[start] == '\'' {
            return Ok(false);
        }
        let Some(with) = correct_word(&self.office.autocorrect, &o, chars, start, &word) else {
            return Ok(false);
        };
        self.replace_before(at, off - start, &format!("{with}{c}"), 0)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_protocol::commands::write_features::AutoFormatOptions;

    fn list() -> AutoCorrectList {
        AutoCorrectList::new(&[
            AutoCorrectEntry {
                from: "teh".into(),
                to: "the".into(),
            },
            AutoCorrectEntry {
                from: "i".into(),
                to: "I".into(),
            },
            AutoCorrectEntry {
                from: "(c)".into(),
                to: "©".into(),
            },
        ])
    }

    fn fix(text: &str) -> Option<String> {
        let chars: Vec<char> = text.chars().collect();
        let mut start = chars.len();
        while start > 0 && word_char(chars[start - 1]) {
            start -= 1;
        }
        let word: String = chars[start..].iter().collect();
        correct_word(&list(), &AutoFormatOptions::default(), &chars, start, &word)
    }

    #[test]
    fn typos_capitals_days_and_sentences() {
        assert_eq!(fix("see teh"), Some("the".into()));
        assert_eq!(fix("Teh"), Some("The".into()));
        assert_eq!(fix("so i"), Some("I".into()));
        assert_eq!(fix("on monday"), Some("Monday".into()));
        assert_eq!(fix("the THursday"), Some("Thursday".into()));
        assert_eq!(fix("a kiln"), None);
        assert_eq!(fix("It is hot. the"), Some("The".into()));
        assert_eq!(
            fix("kiln"),
            Some("Kiln".into()),
            "a paragraph starts a sentence"
        );
        assert_eq!(fix("see e.g. this"), None, "not after an abbreviation");
        assert_eq!(
            fix("fired to cone 6. the"),
            Some("The".into()),
            "a number ends a sentence"
        );
        assert_eq!(fix("6. the"), None, "not after a paragraph number");
        assert_eq!(
            fix("see No. 6. the"),
            None,
            "not after a numbered abbreviation"
        );
        assert_eq!(fix("KILN"), None);
        assert!(list().symbols.iter().any(|(f, _)| f.len() == 3));
    }

    #[test]
    fn addresses_and_paths_are_left_alone() {
        let at = |t: &str| {
            let cs: Vec<char> = t.chars().collect();
            in_address(&cs, cs.len())
        };
        assert!(at("visit www.teh"));
        assert!(at("https://teh.example.org/teh"));
        assert!(at("mail teh@kiln"));
        assert!(at("C:\\Users\\teh"));
        assert!(at("/usr/share/teh"));
        assert!(!at("see teh"));
        assert!(!at("it is teh"));
    }
}
