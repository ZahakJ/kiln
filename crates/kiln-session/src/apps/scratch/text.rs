// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text helpers of the scratch document: line splitting, decoding and grapheme/word boundaries.

use kiln_common::{KilnError, Result};
use kiln_protocol::FileFormat;
use unicode_segmentation::UnicodeSegmentation;

/// Splits text into lines on `\r\n`, `\r` and `\n`; always at least one (possibly empty) line.
pub(super) fn split_lines(text: &str) -> Vec<String> {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    normalised.split('\n').map(str::to_owned).collect()
}

/// Decodes a text file: UTF-8 (optional BOM) or UTF-16 with a BOM.
pub(super) fn decode(bytes: &[u8], format: FileFormat) -> Result<String> {
    let err = |message: &str| KilnError::Format {
        format: format.extension().to_owned(),
        message: message.to_owned(),
    };
    let utf16 = |rest: &[u8], be: bool| -> Result<String> {
        if !rest.len().is_multiple_of(2) {
            return Err(err("odd number of bytes in UTF-16 text"));
        }
        let units = rest.as_chunks::<2>().0.iter().map(|c| {
            if be {
                u16::from_be_bytes(*c)
            } else {
                u16::from_le_bytes(*c)
            }
        });
        char::decode_utf16(units)
            .collect::<Result<String, _>>()
            .map_err(|_| err("invalid UTF-16 text"))
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => {
            String::from_utf8(rest.to_vec()).map_err(|_| err("invalid UTF-8 text"))
        }
        [0xFF, 0xFE, rest @ ..] => utf16(rest, false),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, true),
        _ => String::from_utf8(bytes.to_vec())
            .map_err(|_| err("the file is not UTF-8 text (other encodings are not supported yet)")),
    }
}

/// The grapheme boundary at or before `byte`.
pub(super) fn grapheme_floor(text: &str, byte: usize) -> usize {
    let byte = byte.min(text.len());
    text.grapheme_indices(true)
        .map(|(i, _)| i)
        .take_while(|i| *i <= byte)
        .last()
        .unwrap_or(0)
        .max(if byte == text.len() { byte } else { 0 })
}

/// The grapheme boundary before `byte` (0 stays 0).
pub(super) fn prev_grapheme(text: &str, byte: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(i, _)| i)
        .take_while(|i| *i < byte)
        .last()
        .unwrap_or(0)
}

/// The grapheme boundary after `byte` (the end stays the end).
pub(super) fn next_grapheme(text: &str, byte: usize) -> usize {
    text.grapheme_indices(true)
        .map(|(i, _)| i)
        .find(|i| *i > byte)
        .unwrap_or(text.len())
}

fn is_space(segment: &str) -> bool {
    segment.chars().all(char::is_whitespace)
}

/// Ctrl+Left: the start of the word before `byte` (skipping spaces), or 0.
pub(super) fn prev_word_start(text: &str, byte: usize) -> usize {
    text.split_word_bound_indices()
        .rev()
        .find(|(i, s)| *i < byte && !is_space(s))
        .map_or(0, |(i, _)| i)
}

/// Ctrl+Right: the start of the next word after `byte` (past the spaces), or the end.
pub(super) fn next_word_start(text: &str, byte: usize) -> usize {
    text.split_word_bound_indices()
        .find(|(i, s)| *i > byte && !is_space(s))
        .map_or(text.len(), |(i, _)| i)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_every_kind_of_line_break() {
        assert_eq!(split_lines(""), vec![""]);
        assert_eq!(split_lines("a\r\nb\rc\nd"), vec!["a", "b", "c", "d"]);
        assert_eq!(split_lines("a\n"), vec!["a", ""]);
    }

    #[test]
    fn decodes_utf8_and_utf16() {
        assert_eq!(decode(b"\xEF\xBB\xBFhi", FileFormat::Txt).unwrap(), "hi");
        assert_eq!(decode(b"\xFF\xFEh\0i\0", FileFormat::Txt).unwrap(), "hi");
        assert_eq!(decode(b"\xFE\xFF\0h\0i", FileFormat::Txt).unwrap(), "hi");
        assert!(decode(b"\xFF\xFEh", FileFormat::Txt).is_err());
        assert!(decode(b"\xFF\xFE\x00\xD8", FileFormat::Txt).is_err()); // lone surrogate
        let e = decode(b"caf\xE9", FileFormat::Md).unwrap_err();
        assert!(e.to_string().starts_with("md:"));
        assert!(decode(b"\xEF\xBB\xBF\xFF", FileFormat::Txt).is_err());
    }

    #[test]
    fn grapheme_boundaries_keep_clusters_whole() {
        let t = "e\u{301}👍🏽x"; // e + combining acute, thumbs up + skin tone, x
        assert_eq!(next_grapheme(t, 0), 3);
        assert_eq!(next_grapheme(t, 3), 11);
        assert_eq!(prev_grapheme(t, 11), 3);
        assert_eq!(prev_grapheme(t, 0), 0);
        assert_eq!(next_grapheme(t, t.len()), t.len());
        assert_eq!(grapheme_floor(t, 1), 0);
        assert_eq!(grapheme_floor(t, 5), 3);
        assert_eq!(grapheme_floor(t, t.len()), t.len());
        assert_eq!(grapheme_floor("", 0), 0);
    }

    #[test]
    fn word_motions_skip_spaces() {
        let t = "Hello brave  new world";
        assert_eq!(next_word_start(t, 0), 6);
        assert_eq!(next_word_start(t, 6), 13);
        assert_eq!(next_word_start(t, 17), t.len());
        assert_eq!(prev_word_start(t, t.len()), 17);
        assert_eq!(prev_word_start(t, 13), 6);
        assert_eq!(prev_word_start(t, 3), 0);
    }
}
