// SPDX-License-Identifier: AGPL-3.0-or-later
//! sRGB colours with alpha.

use std::fmt;
use std::str::FromStr;

/// An sRGB colour with straight (not premultiplied) alpha, 8 bits per channel.
///
/// Serialises as a lowercase hex string: `#rrggbb` when opaque, `#rrggbbaa` otherwise. Parsing
/// accepts `#rgb`, `#rrggbb` and `#rrggbbaa`, with or without the `#` (OOXML writes `2F5496`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Color {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (255 = opaque).
    pub a: u8,
}

/// Why a colour string could not be parsed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid colour {input:?}: expected #rgb, #rrggbb or #rrggbbaa")]
pub struct ParseColorError {
    input: String,
}

impl Color {
    /// Opaque black.
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    /// Opaque white.
    pub const WHITE: Color = Color::rgb(255, 255, 255);
    /// Fully transparent black.
    pub const TRANSPARENT: Color = Color::rgba(0, 0, 0, 0);

    /// An opaque colour.
    #[must_use]
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// A colour with alpha.
    #[must_use]
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// The same colour with another alpha.
    #[must_use]
    pub const fn with_alpha(self, a: u8) -> Self {
        Self { a, ..self }
    }

    /// True when alpha is 255.
    #[must_use]
    pub const fn is_opaque(self) -> bool {
        self.a == 255
    }

    /// Packs into `0xRRGGBBAA`.
    #[must_use]
    pub const fn to_u32(self) -> u32 {
        u32::from_be_bytes([self.r, self.g, self.b, self.a])
    }

    /// Unpacks `0xRRGGBBAA`.
    #[must_use]
    pub const fn from_u32(v: u32) -> Self {
        let [r, g, b, a] = v.to_be_bytes();
        Self { r, g, b, a }
    }

    /// Parses `#rgb`, `#rrggbb` or `#rrggbbaa` (the `#` is optional).
    ///
    /// # Errors
    ///
    /// Returns [`ParseColorError`] for any other length or a non-hex digit.
    pub fn parse_hex(input: &str) -> Result<Self, ParseColorError> {
        let err = || ParseColorError {
            input: input.to_owned(),
        };
        let hex = input.strip_prefix('#').unwrap_or(input);
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(err());
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).map_err(|_| err());
        match hex.len() {
            3 => {
                let nib = |i: usize| u8::from_str_radix(&hex[i..=i], 16).map(|n| n * 17);
                Ok(Self::rgb(
                    nib(0).map_err(|_| err())?,
                    nib(1).map_err(|_| err())?,
                    nib(2).map_err(|_| err())?,
                ))
            }
            6 => Ok(Self::rgb(byte(0)?, byte(2)?, byte(4)?)),
            8 => Ok(Self::rgba(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
            _ => Err(err()),
        }
    }

    /// Formats as `#rrggbb` (opaque) or `#rrggbbaa`.
    #[must_use]
    pub fn to_hex(self) -> String {
        if self.is_opaque() {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl FromStr for Color {
    type Err = ParseColorError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_hex(s)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Color {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_hex())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for Color {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct HexVisitor;
        impl serde::de::Visitor<'_> for HexVisitor {
            type Value = Color;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a colour string such as #2f5496")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Color, E> {
                Color::parse_hex(v).map_err(E::custom)
            }
        }
        deserializer.deserialize_str(HexVisitor)
    }
}

#[cfg(feature = "ts")]
impl ts_rs::TS for Color {
    type WithoutGenerics = Self;
    type OptionInnerType = Self;
    fn name(_: &ts_rs::Config) -> String {
        "string".to_owned()
    }
    fn inline(cfg: &ts_rs::Config) -> String {
        Self::name(cfg)
    }
    fn decl(_: &ts_rs::Config) -> String {
        "type Color = string;".to_owned()
    }
    fn decl_concrete(cfg: &ts_rs::Config) -> String {
        Self::decl(cfg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_office_and_css_forms() {
        assert_eq!(Color::parse_hex("2F5496"), Ok(Color::rgb(0x2f, 0x54, 0x96)));
        assert_eq!(
            Color::parse_hex("#2f5496"),
            Ok(Color::rgb(0x2f, 0x54, 0x96))
        );
        assert_eq!(Color::parse_hex("#fff"), Ok(Color::WHITE));
        assert_eq!(
            Color::parse_hex("#11223380"),
            Ok(Color::rgba(0x11, 0x22, 0x33, 0x80))
        );
        assert_eq!("#000".parse::<Color>(), Ok(Color::BLACK));
    }

    #[test]
    fn rejects_malformed_strings() {
        for bad in [
            "", "#", "#12", "#12345", "#1234567", "zzzzzz", "#ééé", "+12345",
        ] {
            assert!(Color::parse_hex(bad).is_err(), "{bad} should be rejected");
        }
        let msg = Color::parse_hex("nope").unwrap_err().to_string();
        assert!(msg.contains("nope"));
    }

    #[test]
    fn formats_round_trip() {
        for c in [
            Color::BLACK,
            Color::WHITE,
            Color::TRANSPARENT,
            Color::rgba(1, 2, 3, 4),
        ] {
            assert_eq!(Color::parse_hex(&c.to_hex()), Ok(c));
            assert_eq!(Color::from_u32(c.to_u32()), c);
        }
        assert_eq!(Color::rgb(0x2f, 0x54, 0x96).to_string(), "#2f5496");
        assert_eq!(Color::WHITE.with_alpha(0).to_hex(), "#ffffff00");
        assert_eq!(Color::rgb(1, 2, 3).to_u32(), 0x010203ff);
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serialises_as_a_hex_string() {
        let json = serde_json::to_string(&Color::rgb(0x2f, 0x54, 0x96)).unwrap();
        assert_eq!(json, "\"#2f5496\"");
        let back: Color = serde_json::from_str("\"#2F549680\"").unwrap();
        assert_eq!(back, Color::rgba(0x2f, 0x54, 0x96, 0x80));
        assert!(serde_json::from_str::<Color>("\"blue\"").is_err());
    }
}
