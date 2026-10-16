// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text encodings of delimited files: detection on import (byte order marks, UTF-16 without a
//! mark, valid UTF-8, else Windows-1252 — what Excel assumes for "CSV" on Western systems) and
//! encoding on export.

use std::borrow::Cow;

/// A text encoding of a delimited file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Encoding {
    /// UTF-8 without a byte order mark.
    #[default]
    Utf8,
    /// UTF-8 with a byte order mark (Excel's "CSV UTF-8").
    Utf8Bom,
    /// UTF-16 little-endian with a byte order mark (Excel's "Unicode Text").
    Utf16Le,
    /// UTF-16 big-endian with a byte order mark.
    Utf16Be,
    /// Windows-1252 (Excel's "CSV" on Western Windows).
    Windows1252,
}

impl Encoding {
    /// The name shown to people (`UTF-8`, `UTF-16 LE`, `Windows-1252`).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf8Bom => "UTF-8 with BOM",
            Encoding::Utf16Le => "UTF-16 LE",
            Encoding::Utf16Be => "UTF-16 BE",
            Encoding::Windows1252 => "Windows-1252",
        }
    }
}

/// Guesses whether BOM-less bytes are UTF-16 from where the zero bytes fall (ASCII-heavy text
/// has a zero in every other byte).
fn sniff_utf16(bytes: &[u8]) -> Option<Encoding> {
    let n = bytes.len().min(4096) & !1;
    if n < 4 {
        return None;
    }
    let (mut even, mut odd) = (0usize, 0usize);
    for (i, b) in bytes[..n].iter().enumerate() {
        if *b == 0 {
            if i % 2 == 0 {
                even += 1;
            } else {
                odd += 1;
            }
        }
    }
    let half = n / 2;
    if odd * 10 >= half * 4 && even * 10 < half {
        Some(Encoding::Utf16Le)
    } else if even * 10 >= half * 4 && odd * 10 < half {
        Some(Encoding::Utf16Be)
    } else {
        None
    }
}

/// Detects the encoding of a file.
#[must_use]
pub fn detect(bytes: &[u8]) -> Encoding {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return Encoding::Utf8Bom;
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return Encoding::Utf16Le;
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return Encoding::Utf16Be;
    }
    if let Some(e) = sniff_utf16(bytes) {
        return e;
    }
    if std::str::from_utf8(bytes).is_ok() {
        Encoding::Utf8
    } else {
        Encoding::Windows1252
    }
}

/// Decodes bytes in an encoding (byte order marks removed; invalid sequences become U+FFFD).
#[must_use]
pub fn decode(bytes: &[u8], encoding: Encoding) -> Cow<'_, str> {
    match encoding {
        Encoding::Utf8 | Encoding::Utf8Bom => {
            let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
            String::from_utf8_lossy(body)
        }
        Encoding::Utf16Le => {
            let body = bytes.strip_prefix(&[0xFF, 0xFE][..]).unwrap_or(bytes);
            Cow::Owned(
                encoding_rs::UTF_16LE
                    .decode_without_bom_handling(body)
                    .0
                    .into_owned(),
            )
        }
        Encoding::Utf16Be => {
            let body = bytes.strip_prefix(&[0xFE, 0xFF][..]).unwrap_or(bytes);
            Cow::Owned(
                encoding_rs::UTF_16BE
                    .decode_without_bom_handling(body)
                    .0
                    .into_owned(),
            )
        }
        Encoding::Windows1252 => Cow::Owned(
            encoding_rs::WINDOWS_1252
                .decode_without_bom_handling(bytes)
                .0
                .into_owned(),
        ),
    }
}

/// Encodes text (with the byte order mark the encoding calls for). Characters Windows-1252
/// cannot hold become `?`, as Excel writes them.
#[must_use]
pub fn encode(text: &str, encoding: Encoding) -> Vec<u8> {
    match encoding {
        Encoding::Utf8 => text.as_bytes().to_vec(),
        Encoding::Utf8Bom => {
            let mut v = vec![0xEF, 0xBB, 0xBF];
            v.extend_from_slice(text.as_bytes());
            v
        }
        Encoding::Utf16Le => {
            let mut v = vec![0xFF, 0xFE];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_le_bytes());
            }
            v
        }
        Encoding::Utf16Be => {
            let mut v = vec![0xFE, 0xFF];
            for u in text.encode_utf16() {
                v.extend_from_slice(&u.to_be_bytes());
            }
            v
        }
        Encoding::Windows1252 => {
            let mut out = Vec::with_capacity(text.len());
            let mut buf = [0u8; 4];
            for c in text.chars() {
                let s = c.encode_utf8(&mut buf);
                let (bytes, _, unmappable) = encoding_rs::WINDOWS_1252.encode(s);
                if unmappable {
                    out.push(b'?');
                } else {
                    out.extend_from_slice(&bytes);
                }
            }
            out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_round_trips_every_encoding() {
        let text = "Name;Città\r\nZoë;€5\r\n";
        for e in [
            Encoding::Utf8,
            Encoding::Utf8Bom,
            Encoding::Utf16Le,
            Encoding::Utf16Be,
            Encoding::Windows1252,
        ] {
            let bytes = encode(text, e);
            assert_eq!(detect(&bytes), e, "{e:?}");
            assert_eq!(decode(&bytes, e), text, "{e:?}");
        }
        // BOM-less UTF-16 is recognised from its zero bytes.
        let le: Vec<u8> = "a,b\r\n1,2\r\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        assert_eq!(detect(&le), Encoding::Utf16Le);
        assert_eq!(encode("日本", Encoding::Windows1252), b"??");
    }
}
