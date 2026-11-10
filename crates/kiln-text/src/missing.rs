// SPDX-License-Identifier: AGPL-3.0-or-later
//! Characters laid out with no font that has them (drawn as `.notdef`), by kind. The web core
//! fetches its Arabic, symbol, mathematics, CJK and emoji fonts only when a document needs them:
//! it reads these flags after each request ([`take_missing`]) and registers the font that covers
//! them. Chinese, Japanese and Korean come in slices of a font, so the characters themselves are
//! kept too ([`take_missing_chars`]): the page fetches the slices that have them.
//!
//! An emoji is told by its Unicode properties and sequences (UTS #51), not by its block: a
//! character whose `Emoji_Presentation` is Yes (✅, ⭐, 😀), a flag's regional indicators, a keycap
//! (`1️⃣`), a variation selector 16 asking for emoji presentation (`❤️`), a tag sequence and a ZWJ
//! sequence all need the emoji font.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

/// Arabic script (Noto Naskh Arabic covers it).
pub const MISSING_ARABIC: u32 = 1;
/// Symbols and pictographs (Noto Sans Symbols 2).
pub const MISSING_SYMBOLS: u32 = 2;
/// Mathematical operators and alphanumerics (Noto Sans Math).
pub const MISSING_MATH: u32 = 4;
/// Anything else.
pub const MISSING_OTHER: u32 = 8;
/// Chinese, Japanese and Korean: Han, kana, Hangul, bopomofo, CJK punctuation, fullwidth forms
/// (a subset of Noto Sans CJK SC).
pub const MISSING_CJK: u32 = 16;
/// Emoji (Noto Emoji).
pub const MISSING_EMOJI: u32 = 32;

static MISSING: AtomicU32 = AtomicU32::new(0);

/// The CJK characters missed since the last [`take_missing_chars`], at most [`MAX_CHARS`].
static CHARS: Mutex<Vec<u32>> = Mutex::new(Vec::new());
const MAX_CHARS: usize = 8192;

/// Unicode 17's `Emoji_Presentation` (emoji-data.txt, 2025-07-25), merged ranges.
const EMOJI_PRESENTATION: &[(u32, u32)] = &[
    (0x231A, 0x231B),
    (0x23E9, 0x23EC),
    (0x23F0, 0x23F0),
    (0x23F3, 0x23F3),
    (0x25FD, 0x25FE),
    (0x2614, 0x2615),
    (0x2648, 0x2653),
    (0x267F, 0x267F),
    (0x2693, 0x2693),
    (0x26A1, 0x26A1),
    (0x26AA, 0x26AB),
    (0x26BD, 0x26BE),
    (0x26C4, 0x26C5),
    (0x26CE, 0x26CE),
    (0x26D4, 0x26D4),
    (0x26EA, 0x26EA),
    (0x26F2, 0x26F3),
    (0x26F5, 0x26F5),
    (0x26FA, 0x26FA),
    (0x26FD, 0x26FD),
    (0x2705, 0x2705),
    (0x270A, 0x270B),
    (0x2728, 0x2728),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2795, 0x2797),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x1F004, 0x1F004),
    (0x1F0CF, 0x1F0CF),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F1E6, 0x1F1FF),
    (0x1F201, 0x1F201),
    (0x1F21A, 0x1F21A),
    (0x1F22F, 0x1F22F),
    (0x1F232, 0x1F236),
    (0x1F238, 0x1F23A),
    (0x1F250, 0x1F251),
    (0x1F300, 0x1F320),
    (0x1F32D, 0x1F335),
    (0x1F337, 0x1F37C),
    (0x1F37E, 0x1F393),
    (0x1F3A0, 0x1F3CA),
    (0x1F3CF, 0x1F3D3),
    (0x1F3E0, 0x1F3F0),
    (0x1F3F4, 0x1F3F4),
    (0x1F3F8, 0x1F43E),
    (0x1F440, 0x1F440),
    (0x1F442, 0x1F4FC),
    (0x1F4FF, 0x1F53D),
    (0x1F54B, 0x1F54E),
    (0x1F550, 0x1F567),
    (0x1F57A, 0x1F57A),
    (0x1F595, 0x1F596),
    (0x1F5A4, 0x1F5A4),
    (0x1F5FB, 0x1F64F),
    (0x1F680, 0x1F6C5),
    (0x1F6CC, 0x1F6CC),
    (0x1F6D0, 0x1F6D2),
    (0x1F6D5, 0x1F6D8),
    (0x1F6DC, 0x1F6DF),
    (0x1F6EB, 0x1F6EC),
    (0x1F6F4, 0x1F6FC),
    (0x1F7E0, 0x1F7EB),
    (0x1F7F0, 0x1F7F0),
    (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945),
    (0x1F947, 0x1F9FF),
    (0x1FA70, 0x1FA7C),
    (0x1FA80, 0x1FA8A),
    (0x1FA8E, 0x1FAC6),
    (0x1FAC8, 0x1FAC8),
    (0x1FACD, 0x1FADC),
    (0x1FADF, 0x1FAEA),
    (0x1FAEF, 0x1FAF8),
];

/// Whether a character shows as an emoji by default (`Emoji_Presentation=Yes`).
#[must_use]
pub fn is_emoji_presentation(c: char) -> bool {
    let c = u32::from(c);
    EMOJI_PRESENTATION
        .binary_search_by(|&(lo, hi)| {
            if hi < c {
                std::cmp::Ordering::Less
            } else if lo > c {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// Whether a cluster of text is an emoji (UTS #51): see the module documentation.
#[must_use]
pub fn is_emoji_cluster(cluster: &str) -> bool {
    let mut chars = cluster.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let rest: Vec<char> = chars.collect();
    if rest.contains(&'\u{FE0E}') {
        return false;
    }
    let regional = |c: char| ('\u{1F1E6}'..='\u{1F1FF}').contains(&c);
    regional(first)
        || is_emoji_presentation(first)
        || rest
            .iter()
            .any(|&c| matches!(c, '\u{FE0F}' | '\u{20E3}' | '\u{E0020}'..='\u{E007F}'))
        || (rest.contains(&'\u{200D}') && rest.iter().any(|&c| is_emoji_presentation(c)))
}

/// The kind of a character, for [`MISSING`].
#[must_use]
pub fn missing_kind(c: char) -> u32 {
    match u32::from(c) {
        0x0600..=0x06FF | 0x0750..=0x077F | 0x0870..=0x08FF | 0xFB50..=0xFDFF | 0xFE70..=0xFEFF => {
            MISSING_ARABIC
        }
        0x2200..=0x22FF | 0x27C0..=0x27EF | 0x2980..=0x2AFF | 0x1D400..=0x1D7FF => MISSING_MATH,
        0x1100..=0x11FF
        | 0x2E80..=0x2FDF
        | 0x3000..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA960..=0xA97F
        | 0xAC00..=0xD7FF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFFEF
        | 0x20000..=0x3134F => MISSING_CJK,
        0x1F300..=0x1F5FF | 0x1F600..=0x1F64F | 0x1F680..=0x1F6FF | 0x1F900..=0x1FAFF => {
            MISSING_EMOJI
        }
        0x2190..=0x21FF
        | 0x2300..=0x23FF
        | 0x2460..=0x27BF
        | 0x27F0..=0x297F
        | 0x2B00..=0x2BFF
        | 0x1F000..=0x1FBFF => MISSING_SYMBOLS,
        _ => MISSING_OTHER,
    }
}

/// The kind of a cluster laid out with no font: an emoji's, or its first character's.
#[must_use]
pub fn missing_kind_of(cluster: &str) -> u32 {
    if is_emoji_cluster(cluster) {
        return MISSING_EMOJI;
    }
    cluster.chars().next().map_or(MISSING_OTHER, missing_kind)
}

pub(crate) fn note(cluster: &str) {
    let kind = missing_kind_of(cluster);
    MISSING.fetch_or(kind, Ordering::Relaxed);
    if kind == MISSING_CJK
        && let Ok(mut chars) = CHARS.lock()
    {
        for c in cluster.chars().map(u32::from) {
            if chars.len() < MAX_CHARS && !chars.contains(&c) {
                chars.push(c);
            }
        }
    }
}

/// The kinds of characters laid out without a font since the last call, and clears them.
pub fn take_missing() -> u32 {
    MISSING.swap(0, Ordering::Relaxed)
}

/// The Chinese, Japanese and Korean characters laid out without a font since the last call (at
/// most 8,192), and clears them: the web core fetches the slices of its CJK font that have them.
pub fn take_missing_chars() -> Vec<u32> {
    CHARS
        .lock()
        .map(|mut c| std::mem::take(&mut *c))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emoji_are_told_by_their_properties_and_sequences() {
        for e in [
            "✅",
            "⭐",
            "⭐\u{FE0F}",
            "😀",
            "🇯🇵",
            "🇬🇧",
            "1\u{FE0F}\u{20E3}",
            "#\u{20E3}",
            "❤\u{FE0F}",
            "👨\u{200D}👩\u{200D}👧",
            "🏴\u{E0067}\u{E0062}\u{E0073}\u{E0063}\u{E0074}\u{E007F}",
        ] {
            assert_eq!(missing_kind_of(e), MISSING_EMOJI, "{e:?}");
        }
        for t in ["☃", "✓", "⭐\u{FE0E}", "❤", "1", "漢"] {
            assert_ne!(missing_kind_of(t), MISSING_EMOJI, "{t:?}");
        }
    }
}
