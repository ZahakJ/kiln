// SPDX-License-Identifier: AGPL-3.0-or-later
//! Number formats (`ST_NumberFormat`) for list labels, page numbers and field `\*` switches, and
//! the mapping of symbol-font bullets to Unicode.

use crate::st::NumberFormat;

const ROMAN: [(i64, &str); 13] = [
    (1000, "M"),
    (900, "CM"),
    (500, "D"),
    (400, "CD"),
    (100, "C"),
    (90, "XC"),
    (50, "L"),
    (40, "XL"),
    (10, "X"),
    (9, "IX"),
    (5, "V"),
    (4, "IV"),
    (1, "I"),
];

/// Values beyond this render as decimal in the formats that repeat symbols, so no input can
/// allocate without bound.
const MAX_REPEAT_VALUE: i64 = 100_000;

/// Upper-case Roman numerals. Zero and negatives render as decimal; values from 4000 repeat `M`.
#[must_use]
pub fn to_roman(n: i64) -> String {
    if n <= 0 || n > MAX_REPEAT_VALUE {
        return n.to_string();
    }
    let mut rest = n;
    let mut out = String::new();
    for (v, s) in ROMAN {
        while rest >= v {
            out.push_str(s);
            rest -= v;
        }
    }
    out
}

/// Word's alphabetic numbering: A…Z, then AA…ZZ, AAA… (the letter repeats; it is not the
/// spreadsheet-column sequence). Zero and negatives render as empty.
#[must_use]
pub fn to_letters(n: i64, alphabet: &str) -> String {
    let letters: Vec<char> = alphabet.chars().collect();
    if n <= 0 || letters.is_empty() {
        return String::new();
    }
    if n > MAX_REPEAT_VALUE {
        return n.to_string();
    }
    let len = i64::try_from(letters.len()).unwrap_or(1);
    let idx = usize::try_from((n - 1) % len).unwrap_or(0);
    let repeat = usize::try_from((n - 1) / len + 1).unwrap_or(1);
    letters
        .get(idx)
        .map(|c| c.to_string().repeat(repeat))
        .unwrap_or_default()
}

const LATIN: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZ";

const ONES: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const SCALES: [(i64, &str); 6] = [
    (1_000_000_000_000_000_000, "quintillion"),
    (1_000_000_000_000_000, "quadrillion"),
    (1_000_000_000_000, "trillion"),
    (1_000_000_000, "billion"),
    (1_000_000, "million"),
    (1000, "thousand"),
];

/// English cardinal text as Word writes it (`121` → `one hundred twenty-one`).
#[must_use]
pub fn to_cardinal_text(n: i64) -> String {
    if n < 0 {
        return format!(
            "minus {}",
            to_cardinal_text(n.checked_neg().unwrap_or(i64::MAX))
        );
    }
    let idx = |v: i64| usize::try_from(v).unwrap_or(0);
    if n < 20 {
        return ONES[idx(n)].to_owned();
    }
    if n < 100 {
        let t = TENS[idx(n / 10)];
        return if n % 10 == 0 {
            t.to_owned()
        } else {
            format!("{t}-{}", ONES[idx(n % 10)])
        };
    }
    if n < 1000 {
        let head = format!("{} hundred", ONES[idx(n / 100)]);
        return if n % 100 == 0 {
            head
        } else {
            format!("{head} {}", to_cardinal_text(n % 100))
        };
    }
    for (scale, word) in SCALES {
        if n >= scale {
            let head = format!("{} {word}", to_cardinal_text(n / scale));
            let rest = n % scale;
            return if rest == 0 {
                head
            } else {
                format!("{head} {}", to_cardinal_text(rest))
            };
        }
    }
    n.to_string()
}

/// English ordinal text (`21` → `twenty-first`).
#[must_use]
pub fn to_ordinal_text(n: i64) -> String {
    let card = to_cardinal_text(n);
    let split = card
        .rfind(|c: char| !c.is_ascii_lowercase())
        .map_or(0, |i| i + 1);
    let (head, last) = card.split_at(split);
    let ord = match last {
        "one" => "first".to_owned(),
        "two" => "second".to_owned(),
        "three" => "third".to_owned(),
        "five" => "fifth".to_owned(),
        "eight" => "eighth".to_owned(),
        "nine" => "ninth".to_owned(),
        "twelve" => "twelfth".to_owned(),
        w if w.ends_with('y') => format!("{}ieth", &w[..w.len() - 1]),
        w => format!("{w}th"),
    };
    format!("{head}{ord}")
}

/// An English ordinal with suffix (`1st`, `2nd`, `11th`, `23rd`).
#[must_use]
pub fn to_ordinal(n: i64) -> String {
    let abs = n.unsigned_abs();
    let suffix = match (abs % 100, abs % 10) {
        (11..=13, _) => "th",
        (_, 1) => "st",
        (_, 2) => "nd",
        (_, 3) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// Chicago Manual of Style note symbols: *, †, ‡, §, then doubled, tripled…
#[must_use]
pub fn to_chicago(n: i64) -> String {
    to_letters(n, "*†‡§")
}

const RUSSIAN: &str = "абвгдежзиклмнопрстуфхцчшщэюя";
const ARABIC_ALPHA: &str = "أبتثجحخدذرزسشصضطظعغفقكلمنهوي";
const ARABIC_ABJAD: &str = "أبجدهوزحطيكلمنسعفصقرشتثخذضظغ";
const HEBREW2: &str = "אבגדהוזחטיכלמנסעפצקרשת";
const IROHA: &str = "いろはにほへとちりぬるをわかよたれそつねならむうゐのおくやまけふこえてあさきゆめみしゑひもせす";
const AIUEO: &str =
    "アイウエオカキクケコサシスセソタチツテトナニヌネノハヒフヘホマミムメモヤユヨラリルレロワヲン";
const CJK_DIGITS: [char; 10] = ['〇', '一', '二', '三', '四', '五', '六', '七', '八', '九'];

fn map_digits(n: i64, digits: &[char; 10]) -> String {
    n.to_string()
        .chars()
        .map(|c| c.to_digit(10).map_or(c, |d| digits[d as usize]))
        .collect()
}

/// Japanese/Chinese counting (`23` → `二十三`); values from 10,000 use digit-by-digit form.
fn to_cjk_counting(n: i64) -> String {
    if n <= 0 || n >= 10_000 {
        return map_digits(n, &CJK_DIGITS);
    }
    let mut rest = n;
    let mut out = String::new();
    for (v, s) in [(1000, '千'), (100, '百'), (10, '十')] {
        let d = rest / v;
        if d > 0 {
            if d > 1 {
                out.push(CJK_DIGITS[usize::try_from(d).unwrap_or(0)]);
            }
            out.push(s);
        }
        rest %= v;
    }
    if rest > 0 {
        out.push(CJK_DIGITS[usize::try_from(rest).unwrap_or(0)]);
    }
    out
}

/// Hebrew numerals (gematria) for `hebrew1`, 1–999.
fn to_hebrew(n: i64) -> String {
    if !(1..1000).contains(&n) {
        return n.to_string();
    }
    const HUNDREDS: [&str; 10] = ["", "ק", "ר", "ש", "ת", "תק", "תר", "תש", "תת", "תתק"];
    const TENS_H: [&str; 10] = ["", "י", "כ", "ל", "מ", "נ", "ס", "ע", "פ", "צ"];
    const ONES_H: [&str; 10] = ["", "א", "ב", "ג", "ד", "ה", "ו", "ז", "ח", "ט"];
    let i = |v: i64| usize::try_from(v).unwrap_or(0);
    let h = HUNDREDS[i(n / 100)];
    let rem = n % 100;
    match rem {
        15 => format!("{h}טו"),
        16 => format!("{h}טז"),
        _ => format!("{h}{}{}", TENS_H[i(rem / 10)], ONES_H[i(rem % 10)]),
    }
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn enclosed(value: i64, first: u32, fallback: impl FnOnce() -> String) -> String {
    if (1..=20).contains(&value) {
        let off = u32::try_from(value - 1).unwrap_or(0);
        char::from_u32(first + off).map_or_else(fallback, |c| c.to_string())
    } else {
        fallback()
    }
}

/// Formats one counter value. `bullet` and `none` give an empty string; formats the model cannot
/// render give decimal.
#[must_use]
pub fn format_number(value: i64, format: &NumberFormat) -> String {
    match format {
        NumberFormat::Decimal | NumberFormat::DecimalHalfWidth | NumberFormat::Custom => {
            value.to_string()
        }
        NumberFormat::DecimalZero => {
            if (0..10).contains(&value) {
                format!("0{value}")
            } else {
                value.to_string()
            }
        }
        NumberFormat::UpperRoman => to_roman(value),
        NumberFormat::LowerRoman => to_roman(value).to_lowercase(),
        NumberFormat::UpperLetter => to_letters(value, LATIN),
        NumberFormat::LowerLetter => to_letters(value, LATIN).to_lowercase(),
        NumberFormat::Ordinal => to_ordinal(value),
        NumberFormat::CardinalText => capitalise(&to_cardinal_text(value)),
        NumberFormat::OrdinalText => capitalise(&to_ordinal_text(value)),
        NumberFormat::Hex => {
            if value < 0 {
                value.to_string()
            } else {
                format!("{value:X}")
            }
        }
        NumberFormat::Chicago => to_chicago(value),
        NumberFormat::Bullet | NumberFormat::None => String::new(),
        NumberFormat::DecimalEnclosedCircle => enclosed(value, 0x2460, || value.to_string()),
        NumberFormat::DecimalEnclosedParen => enclosed(value, 0x2474, || format!("({value})")),
        NumberFormat::DecimalEnclosedFullstop => enclosed(value, 0x2488, || format!("{value}.")),
        NumberFormat::DecimalFullWidth => map_digits(
            value,
            &['０', '１', '２', '３', '４', '５', '６', '７', '８', '９'],
        ),
        NumberFormat::NumberInDash => format!("- {value} -"),
        NumberFormat::RussianLower => to_letters(value, RUSSIAN),
        NumberFormat::RussianUpper => to_letters(value, RUSSIAN).to_uppercase(),
        NumberFormat::Hebrew1 => to_hebrew(value),
        NumberFormat::Hebrew2 => to_letters(value, HEBREW2),
        NumberFormat::ArabicAlpha => to_letters(value, ARABIC_ALPHA),
        NumberFormat::ArabicAbjad => to_letters(value, ARABIC_ABJAD),
        NumberFormat::HindiNumbers => {
            map_digits(value, &['०', '१', '२', '३', '४', '५', '६', '७', '८', '९'])
        }
        NumberFormat::ThaiNumbers => {
            map_digits(value, &['๐', '๑', '๒', '๓', '๔', '๕', '๖', '๗', '๘', '๙'])
        }
        NumberFormat::IdeographDigital => map_digits(value, &CJK_DIGITS),
        NumberFormat::JapaneseCounting
        | NumberFormat::ChineseCounting
        | NumberFormat::ChineseCountingThousand => to_cjk_counting(value),
        NumberFormat::Aiueo | NumberFormat::AiueoFullWidth => to_letters(value, AIUEO),
        NumberFormat::Iroha | NumberFormat::IrohaFullWidth => to_letters(value, IROHA),
        NumberFormat::Other(_) => value.to_string(),
    }
}

/// `w:numFmt w:val="custom" w:format="001, 002, 003, …"`: zero-padded decimal by the width of the
/// first token.
#[must_use]
pub fn format_custom(value: i64, pattern: &str) -> String {
    let first = pattern.split(',').next().unwrap_or("").trim();
    let digits = if !first.is_empty() && first.chars().all(|c| c.is_ascii_digit()) {
        first.chars().count().min(32)
    } else {
        1
    };
    format!("{value:0digits$}")
}

fn symbol_bullet(code: u32) -> Option<char> {
    Some(match code {
        183 => '•',
        168 => '♦',
        216 => '⇒',
        174 => '→',
        45 => '−',
        111 => 'ο',
        167 => '♣',
        169 => '♥',
        170 => '♠',
        _ => return None,
    })
}

fn wingdings_bullet(code: u32) -> Option<char> {
    Some(match code {
        167 => '▪',
        168 => '◻',
        110 => '■',
        113 => '❑',
        117 => '◆',
        118 => '❖',
        216 => '➢',
        252 => '✓',
        251 => '✗',
        161 => '○',
        159 => '•',
        119 => '⬥',
        224 | 232 => '➔',
        108 => '●',
        175 => '✪',
        70 => '☞',
        _ => return None,
    })
}

/// Converts a bullet's text (as stored in `w:lvlText`) to renderable Unicode. Word stores bullets
/// as Private Use Area code points of the Symbol and Wingdings fonts (`U+F0B7` in Symbol is •).
#[must_use]
pub fn bullet_display_text(text: &str, font: Option<&str>) -> String {
    let font = font.unwrap_or("").to_lowercase();
    let font = font
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();
    let table: Option<fn(u32) -> Option<char>> = if font == "symbol" {
        Some(symbol_bullet)
    } else if font.starts_with("wingdings") {
        Some(wingdings_bullet)
    } else {
        None
    };
    text.chars()
        .map(|ch| {
            let cp = u32::from(ch);
            let pua = (0xF000..=0xF0FF).contains(&cp);
            let low = if pua { cp - 0xF000 } else { cp };
            match table {
                Some(t) => t(low).unwrap_or(if pua { '•' } else { ch }),
                None if pua => '•',
                None => ch,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roman_numerals() {
        for (n, r) in [
            (1, "I"),
            (4, "IV"),
            (9, "IX"),
            (14, "XIV"),
            (40, "XL"),
            (90, "XC"),
            (400, "CD"),
            (1994, "MCMXCIV"),
            (3999, "MMMCMXCIX"),
        ] {
            assert_eq!(to_roman(n), r, "{n}");
        }
        assert_eq!(to_roman(0), "0");
        assert_eq!(to_roman(-3), "-3");
        assert_eq!(to_roman(i64::MAX), i64::MAX.to_string());
    }

    #[test]
    fn letters_repeat_like_word() {
        for (n, s) in [(1, "A"), (26, "Z"), (27, "AA"), (28, "BB"), (53, "AAA")] {
            assert_eq!(to_letters(n, LATIN), s, "{n}");
        }
        assert_eq!(to_letters(0, LATIN), "");
        assert_eq!(to_letters(5, ""), "");
    }

    #[test]
    fn cardinal_and_ordinal_text() {
        assert_eq!(to_cardinal_text(0), "zero");
        assert_eq!(to_cardinal_text(21), "twenty-one");
        assert_eq!(to_cardinal_text(1205), "one thousand two hundred five");
        assert_eq!(to_cardinal_text(-7), "minus seven");
        assert_eq!(to_cardinal_text(3_000_000), "three million");
        assert!(to_cardinal_text(i64::MIN).starts_with("minus"));
        assert_eq!(to_ordinal_text(12), "twelfth");
        assert_eq!(to_ordinal_text(20), "twentieth");
        assert_eq!(to_ordinal_text(103), "one hundred third");
        assert_eq!(to_ordinal_text(21), "twenty-first");
    }

    #[test]
    fn ordinal_suffixes_hex_chicago_enclosed_full_width() {
        let got: Vec<String> = [1, 2, 3, 4, 11, 12, 13, 21, 22, 101, 111]
            .iter()
            .map(|n| format_number(*n, &NumberFormat::Ordinal))
            .collect();
        assert_eq!(
            got,
            [
                "1st", "2nd", "3rd", "4th", "11th", "12th", "13th", "21st", "22nd", "101st",
                "111th"
            ]
        );
        assert_eq!(format_number(255, &NumberFormat::Hex), "FF");
        let chicago: Vec<String> = [1, 2, 3, 4, 5, 9]
            .iter()
            .map(|n| format_number(*n, &NumberFormat::Chicago))
            .collect();
        assert_eq!(chicago, ["*", "†", "‡", "§", "**", "***"]);
        assert_eq!(format_number(3, &NumberFormat::DecimalEnclosedCircle), "③");
        assert_eq!(
            format_number(21, &NumberFormat::DecimalEnclosedCircle),
            "21"
        );
        assert_eq!(
            format_number(21, &NumberFormat::DecimalEnclosedParen),
            "(21)"
        );
        assert_eq!(
            format_number(2, &NumberFormat::DecimalEnclosedFullstop),
            "⒉"
        );
        assert_eq!(format_number(12, &NumberFormat::DecimalFullWidth), "１２");
        assert_eq!(format_number(5, &NumberFormat::DecimalZero), "05");
        assert_eq!(format_number(3, &NumberFormat::NumberInDash), "- 3 -");
        assert_eq!(format_number(3, &NumberFormat::CardinalText), "Three");
        assert_eq!(format_number(3, &NumberFormat::OrdinalText), "Third");
        assert_eq!(format_number(3, &NumberFormat::Bullet), "");
    }

    #[test]
    fn non_latin_formats() {
        assert_eq!(format_number(3, &NumberFormat::RussianLower), "в");
        assert_eq!(format_number(3, &NumberFormat::RussianUpper), "В");
        assert_eq!(format_number(15, &NumberFormat::Hebrew1), "טו");
        assert_eq!(format_number(16, &NumberFormat::Hebrew1), "טז");
        assert_eq!(format_number(123, &NumberFormat::Hebrew1), "קכג");
        assert_eq!(format_number(2, &NumberFormat::Hebrew2), "ב");
        assert_eq!(format_number(23, &NumberFormat::JapaneseCounting), "二十三");
        assert_eq!(format_number(10, &NumberFormat::ChineseCounting), "十");
        assert_eq!(
            format_number(1111, &NumberFormat::ChineseCounting),
            "千百十一"
        );
        assert_eq!(format_number(42, &NumberFormat::HindiNumbers), "४२");
        assert_eq!(format_number(42, &NumberFormat::ThaiNumbers), "๔๒");
        assert_eq!(
            format_number(105, &NumberFormat::IdeographDigital),
            "一〇五"
        );
        assert_eq!(format_number(2, &NumberFormat::ArabicAlpha), "ب");
        assert_eq!(format_number(3, &NumberFormat::ArabicAbjad), "ج");
        assert_eq!(format_number(1, &NumberFormat::Aiueo), "ア");
        assert_eq!(format_number(1, &NumberFormat::Iroha), "い");
        assert_eq!(format_number(7, &NumberFormat::parse("unknownFormat")), "7");
    }

    #[test]
    fn custom_formats_pad_to_the_first_token() {
        assert_eq!(format_custom(1, "001, 002, 003, ..."), "001");
        assert_eq!(format_custom(12, "0001"), "0012");
        assert_eq!(format_custom(7, "a, b"), "7");
    }

    #[test]
    fn bullet_glyph_mapping() {
        assert_eq!(bullet_display_text("\u{F0B7}", Some("Symbol")), "•");
        assert_eq!(bullet_display_text("\u{F0A7}", Some("Wingdings")), "▪");
        assert_eq!(bullet_display_text("\u{F0D8}", Some("Wingdings")), "➢");
        assert_eq!(bullet_display_text("\u{F0FC}", Some("Wingdings")), "✓");
        assert_eq!(bullet_display_text("–", Some("Calibri")), "–");
        assert_eq!(bullet_display_text("\u{F0B7}", None), "•");
        assert_eq!(bullet_display_text("\u{F001}", Some("Wingdings 2")), "•");
        assert_eq!(bullet_display_text("o", Some("Courier New")), "o");
    }
}
