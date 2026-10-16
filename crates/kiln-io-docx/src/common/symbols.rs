// SPDX-License-Identifier: AGPL-3.0-or-later
//! `w:sym` characters live in symbol fonts (Symbol, Wingdings, Webdings, …) that are not
//! available on every platform. The model keeps the original font and code
//! ([`kiln_doc::Symbol`]) so the writer restores the `w:sym` element; these functions give the
//! Unicode equivalent for display, search and plain text.

use super::dingbats::FONTS;

/// The `w:char` code of a `w:sym` (`F04A`, `004A`, `4A`) as a number.
#[must_use]
pub fn parse_symbol_code(char_hex: &str) -> Option<u32> {
    let code = char_hex.trim();
    if code.is_empty() || code.len() > 6 || !code.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(code, 16).ok()
}

fn lookup(font: &str, code: u32) -> Option<char> {
    let font = font.trim().to_lowercase();
    let (_, table) = FONTS.iter().find(|(name, _)| *name == font)?;
    let index = usize::try_from(code.checked_sub(32)?).ok()?;
    let cp = *table.get(index)?;
    if cp == 0 { None } else { char::from_u32(cp) }
}

/// The Unicode equivalent of a symbol-font character, or `None` when the font or character has
/// no known equivalent. Word stores symbol-font code points in the private use area `F0xx`,
/// which map to the font's 8-bit code `xx`.
#[must_use]
pub fn symbol_code_to_unicode(font: &str, code: u32) -> Option<char> {
    lookup(font, code).or_else(|| {
        if (0xF000..=0xF0FF).contains(&code) {
            lookup(font, code - 0xF000)
        } else {
            None
        }
    })
}

/// [`symbol_code_to_unicode`] for the hexadecimal `w:char` spelling.
#[must_use]
pub fn symbol_to_unicode(font: &str, char_hex: &str) -> Option<char> {
    symbol_code_to_unicode(font, parse_symbol_code(char_hex)?)
}

/// The text a `w:sym` displays: the Unicode equivalent, else the raw code point when it is a
/// character outside the private use area, else nothing.
#[must_use]
pub fn symbol_text(font: &str, code: u32) -> String {
    if let Some(c) = symbol_code_to_unicode(font, code) {
        return c.to_string();
    }
    match char::from_u32(code) {
        Some(c) if code > 0 && !(0xE000..=0xF8FF).contains(&code) => c.to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_symbol_and_wingdings_private_use_codes() {
        assert_eq!(symbol_to_unicode("Symbol", "F061"), Some('α'));
        assert_eq!(symbol_to_unicode("Symbol", "61"), Some('α'));
        assert_eq!(symbol_to_unicode("Wingdings", "F04A"), Some('☺'));
        assert_eq!(symbol_to_unicode("wingdings", "f0fc"), Some('✓'));
        assert_eq!(symbol_to_unicode("Wingdings 2", "F050"), Some('\u{1F5F8}'));
        assert_eq!(symbol_to_unicode("Unknown Font", "F041"), None);
        assert_eq!(symbol_to_unicode("Symbol", "zz"), None);
        assert_eq!(symbol_to_unicode("Symbol", "1F"), None);
        assert_eq!(symbol_to_unicode("Symbol", "1234567"), None);
    }

    #[test]
    fn falls_back_to_the_code_point_outside_the_private_use_area() {
        assert_eq!(symbol_text("Marlett", 0xF031), "");
        assert_eq!(symbol_text("Marlett", 0x2603), "☃");
        assert_eq!(symbol_text("Symbol", 0xF061), "α");
        assert_eq!(symbol_text("Symbol", 0), "");
        assert_eq!(parse_symbol_code("F04A"), Some(0xF04A));
    }
}
