// SPDX-License-Identifier: AGPL-3.0-or-later
//! Full-width and half-width forms for `ASC` (to half width) and `JIS`/`DBCS` (to full width),
//! following the Japanese (code page 932) conventions Excel uses, as LibreOffice's Excel-compatible
//! "like ASC" / "like JIS" transliterations do:
//!
//! - printable ASCII ↔ U+FF01–FF5E, except that the apostrophe, quotation mark, grave accent and
//!   backslash become `’`, `”`, `‘` and `￥` (the forms code page 932 has), and back;
//! - katakana ↔ half-width katakana (U+FF61–FF9F), where a voiced kana (`ガ`, `パ`, `ヴ`, `ヷ`) is
//!   one full-width character and two half-width ones (`ｶﾞ`, `ﾊﾟ`, `ｳﾞ`, `ﾜﾞ`);
//! - spaces, hiragana, kanji and everything else are left alone.

/// Full-width katakana and punctuation for the half-width forms U+FF61–U+FF9F, in order.
const HALF_KANA_TO_FULL: [char; 63] = [
    '。', '「', '」', '、', '・', 'ヲ', 'ァ', 'ィ', 'ゥ', 'ェ', 'ォ', 'ャ', 'ュ', 'ョ', 'ッ', 'ー',
    'ア', 'イ', 'ウ', 'エ', 'オ', 'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ',
    'チ', 'ツ', 'テ', 'ト', 'ナ', 'ニ', 'ヌ', 'ネ', 'ノ', 'ハ', 'ヒ', 'フ', 'ヘ', 'ホ', 'マ', 'ミ',
    'ム', 'メ', 'モ', 'ヤ', 'ユ', 'ヨ', 'ラ', 'リ', 'ル', 'レ', 'ロ', 'ワ', 'ン', '゛', '゜',
];

/// ASCII characters whose full-width form is not the U+FF01–FF5E one.
const SPECIAL_FULL: [(char, char); 4] = [('\'', '’'), ('"', '”'), ('`', '‘'), ('\\', '￥')];

const DAKUTEN_HALF: char = '\u{FF9E}';
const HANDAKUTEN_HALF: char = '\u{FF9F}';

/// The voiced (`゛`) form of a full-width katakana, if it has one.
fn voiced(c: char) -> Option<char> {
    match c {
        'ウ' => Some('ヴ'),
        'ワ' => Some('ヷ'),
        'ヲ' => Some('ヺ'),
        'カ'..='ト' | 'ハ'..='ホ' => {
            let base = voicing_base(c)?;
            char::from_u32(base as u32 + 1)
        }
        _ => None,
    }
}

/// The semi-voiced (`゜`) form of a full-width katakana (`ハ` row only).
fn semi_voiced(c: char) -> Option<char> {
    match c {
        'ハ' | 'ヒ' | 'フ' | 'ヘ' | 'ホ' => char::from_u32(c as u32 + 2),
        _ => None,
    }
}

/// `c` when it is an unvoiced kana of the カ, サ, タ or ハ rows (whose voiced form follows it).
fn voicing_base(c: char) -> Option<char> {
    const BASES: [char; 20] = [
        'カ', 'キ', 'ク', 'ケ', 'コ', 'サ', 'シ', 'ス', 'セ', 'ソ', 'タ', 'チ', 'ツ', 'テ', 'ト',
        'ハ', 'ヒ', 'フ', 'ヘ', 'ホ',
    ];
    BASES.contains(&c).then_some(c)
}

/// Half-width text (`ASC`).
pub(crate) fn to_half_width(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let u = c as u32;
        if let Some(&(half, _)) = SPECIAL_FULL.iter().find(|(_, full)| *full == c) {
            out.push(half);
            continue;
        }
        match u {
            0xFF01..=0xFF5E => out.push(char::from_u32(u - 0xFEE0).unwrap_or(c)),
            _ => {
                if let Some(i) = HALF_KANA_TO_FULL.iter().position(|&f| f == c) {
                    out.push(char::from_u32(0xFF61 + i as u32).unwrap_or(c));
                } else if let Some((base, mark)) = devoice(c) {
                    if let Some(i) = HALF_KANA_TO_FULL.iter().position(|&f| f == base) {
                        out.push(char::from_u32(0xFF61 + i as u32).unwrap_or(base));
                        out.push(mark);
                    } else {
                        out.push(c);
                    }
                } else {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// A voiced or semi-voiced full-width kana as its unvoiced base and the half-width mark.
fn devoice(c: char) -> Option<(char, char)> {
    match c {
        'ヴ' => return Some(('ウ', DAKUTEN_HALF)),
        'ヷ' => return Some(('ワ', DAKUTEN_HALF)),
        'ヺ' => return Some(('ヲ', DAKUTEN_HALF)),
        _ => {}
    }
    let prev = char::from_u32((c as u32).checked_sub(1)?)?;
    if voiced(prev) == Some(c) {
        return Some((prev, DAKUTEN_HALF));
    }
    let prev2 = char::from_u32((c as u32).checked_sub(2)?)?;
    if semi_voiced(prev2) == Some(c) {
        return Some((prev2, HANDAKUTEN_HALF));
    }
    None
}

/// Full-width text (`JIS`, `DBCS`).
pub(crate) fn to_full_width(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        let u = c as u32;
        if let Some(&(_, full)) = SPECIAL_FULL.iter().find(|(half, _)| *half == c) {
            out.push(full);
            continue;
        }
        match u {
            0x21..=0x7E => out.push(char::from_u32(u + 0xFEE0).unwrap_or(c)),
            0xFF61..=0xFF9F => {
                let full = HALF_KANA_TO_FULL[(u - 0xFF61) as usize];
                let combined = match chars.peek() {
                    Some(&DAKUTEN_HALF) => voiced(full),
                    Some(&HANDAKUTEN_HALF) => semi_voiced(full),
                    _ => None,
                };
                if let Some(v) = combined {
                    chars.next();
                    out.push(v);
                } else {
                    out.push(full);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_quotes_and_spaces() {
        assert_eq!(to_half_width("ＥＸＣＥＬ２０２４！"), "EXCEL2024!");
        assert_eq!(to_full_width("Excel 2024!"), "Ｅｘｃｅｌ ２０２４！");
        assert_eq!(to_full_width("~\\'\"`"), "～￥’”‘");
        assert_eq!(to_half_width("～￥’”‘＼＇＂｀"), "~\\'\"`\\'\"`");
        // Spaces keep their width.
        assert_eq!(to_half_width("ア　イ"), "ｱ　ｲ");
        assert_eq!(to_full_width("a b"), "ａ ｂ");
    }

    #[test]
    fn katakana_with_voicing_marks() {
        assert_eq!(
            to_half_width("ガギグゲゴ パピプペポ ヴヷヺ"),
            "ｶﾞｷﾞｸﾞｹﾞｺﾞ ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ ｳﾞﾜﾞｦﾞ"
        );
        assert_eq!(
            to_full_width("ｶﾞｷﾞｸﾞｹﾞｺﾞ ﾊﾟﾋﾟﾌﾟﾍﾟﾎﾟ ｳﾞﾜﾞｦﾞ"),
            "ガギグゲゴ パピプペポ ヴヷヺ"
        );
        assert_eq!(to_half_width("エクセル。「テスト」・ー"), "ｴｸｾﾙ｡｢ﾃｽﾄ｣･ｰ");
        assert_eq!(to_full_width("ｴｸｾﾙ｡｢ﾃｽﾄ｣･ｰ"), "エクセル。「テスト」・ー");
        // A mark that cannot combine stays a separate full-width mark.
        assert_eq!(to_full_width("ｱﾞ"), "ア゛");
        assert_eq!(to_full_width("ｶﾟ"), "カ゜");
    }

    #[test]
    fn everything_else_is_untouched() {
        for s in ["ひらがな", "漢字", "é", "😀", "ヮヰヱヵヶ", ""] {
            assert_eq!(to_half_width(s), s);
        }
        assert_eq!(to_full_width("ひらがな漢字é😀"), "ひらがな漢字é😀");
        assert_eq!(to_half_width("￥１００"), "\\100");
        assert_eq!(to_half_width("￠￡￢￣￤￦“"), "￠￡￢￣￤￦“");
        assert_eq!(to_full_width("¢£¬¯¦₩¥"), "¢£¬¯¦₩¥");
    }

    #[test]
    fn round_trips() {
        let half = "ｱｲｳｴｵｶﾞｻﾞﾀﾞﾊﾞﾊﾟABC xyz 123 ~!";
        assert_eq!(to_half_width(&to_full_width(half)), half);
    }
}
