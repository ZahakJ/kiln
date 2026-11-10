// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text positions as Excel counts them: in UTF-16 code units (an emoji outside the Basic
//! Multilingual Plane has length 2), with a fast path for ASCII.

/// Length in UTF-16 code units.
pub(crate) fn len16(s: &str) -> usize {
    if s.is_ascii() {
        s.len()
    } else {
        s.encode_utf16().count()
    }
}

/// The text between UTF-16 positions `start..end` (clamped to the text).
pub(crate) fn slice16(s: &str, start: usize, end: usize) -> String {
    if s.is_ascii() {
        let (a, b) = (start.min(s.len()), end.min(s.len()));
        return if a < b {
            s[a..b].to_owned()
        } else {
            String::new()
        };
    }
    let units: Vec<u16> = s.encode_utf16().collect();
    let (a, b) = (start.min(units.len()), end.min(units.len()));
    if a >= b {
        return String::new();
    }
    String::from_utf16_lossy(&units[a..b])
}

/// The text from UTF-16 position `start` on.
pub(crate) fn from16(s: &str, start: usize) -> String {
    slice16(s, start, usize::MAX)
}

/// UTF-16 position of the first occurrence of `needle` at or after UTF-16 position `from` (a
/// position inside a surrogate pair starts the search at the next character, as JavaScript's
/// `indexOf` effectively does).
pub(crate) fn find16(hay: &str, needle: &str, from: usize) -> Option<usize> {
    if hay.is_ascii() {
        if from > hay.len() {
            return None;
        }
        return hay[from..].find(needle).map(|i| i + from);
    }
    let byte = byte_at16(hay, from).or_else(|| byte_at16(hay, from + 1))?;
    let i = hay[byte..].find(needle)? + byte;
    Some(len16(&hay[..i]))
}

/// Byte offset of a UTF-16 position (`None` beyond the end or inside a surrogate pair).
pub(crate) fn byte_at16(s: &str, pos: usize) -> Option<usize> {
    if s.is_ascii() {
        return (pos <= s.len()).then_some(pos);
    }
    let mut units = 0;
    for (b, c) in s.char_indices() {
        if units == pos {
            return Some(b);
        }
        if units > pos {
            return None;
        }
        units += c.len_utf16();
    }
    (units == pos).then_some(s.len())
}

/// UTF-16 position of a character index.
pub(crate) fn char_to16(s: &str, chars: usize) -> usize {
    s.chars().take(chars).map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf16_positions() {
        assert_eq!(len16("abc"), 3);
        assert_eq!(len16("a😀b"), 4);
        assert_eq!(slice16("a😀b", 1, 3), "😀");
        assert_eq!(slice16("héllo", 1, 3), "él");
        assert_eq!(slice16("abc", 2, 10), "c");
        assert_eq!(slice16("abc", 5, 10), "");
        assert_eq!(find16("a😀b😀", "😀", 2), Some(4));
        assert_eq!(find16("abcabc", "c", 3), Some(5));
        assert_eq!(find16("abc", "", 3), Some(3));
        assert_eq!(find16("abc", "a", 4), None);
        assert_eq!(char_to16("😀x", 1), 2);
    }
}
