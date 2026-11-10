// SPDX-License-Identifier: AGPL-3.0-or-later
//! `BAHTTEXT`: an amount written out in Thai, in baht and satang.
//!
//! The amount is rounded to the satang (two decimals). Each group of six digits is read with the
//! place words สิบ, ร้อย, พัน, หมื่น, แสน, with ยี่สิบ for twenty and เอ็ด for a final one after
//! a tens digit; groups are joined by ล้าน (million). Whole amounts end in บาทถ้วน, amounts with
//! satang in …บาท…สตางค์ (or only …สตางค์ below one baht); negative amounts start with ลบ.

const DIGITS: [&str; 10] = [
    "ศูนย์",
    "หนึ่ง",
    "สอง",
    "สาม",
    "สี่",
    "ห้า",
    "หก",
    "เจ็ด",
    "แปด",
    "เก้า",
];
const TEN: &str = "สิบ";
const TWENTY: &str = "ยี่";
const ONE_AFTER_TENS: &str = "เอ็ด";
/// 10², 10³, 10⁴, 10⁵.
const PLACES: [&str; 4] = ["ร้อย", "พัน", "หมื่น", "แสน"];
const MILLION: &str = "ล้าน";
const BAHT: &str = "บาท";
const EXACT: &str = "ถ้วน";
const SATANG: &str = "สตางค์";
const MINUS: &str = "ลบ";

/// Reads 1 ≤ `n` ≤ 999,999.
fn block(out: &mut String, mut n: u64) {
    for (pow, word) in [
        (100_000, PLACES[3]),
        (10_000, PLACES[2]),
        (1_000, PLACES[1]),
        (100, PLACES[0]),
    ] {
        if n >= pow {
            out.push_str(DIGITS[(n / pow) as usize]);
            out.push_str(word);
            n %= pow;
        }
    }
    if n > 0 {
        let (tens, ones) = (n / 10, n % 10);
        if tens >= 3 {
            out.push_str(DIGITS[tens as usize]);
        } else if tens == 2 {
            out.push_str(TWENTY);
        }
        if tens >= 1 {
            out.push_str(TEN);
        }
        if tens > 0 && ones == 1 {
            out.push_str(ONE_AFTER_TENS);
        } else if ones > 0 {
            out.push_str(DIGITS[ones as usize]);
        }
    }
}

/// The Thai text of an amount (`None` when it overflows in satang).
pub(crate) fn baht_text(x: f64) -> Option<String> {
    let minus = x < 0.0;
    // Rounded to the satang the way ROUND does (1.005 is 1.01).
    let satang_total = (crate::values::excel_round(x.abs(), 2.0) * 100.0).round();
    if !satang_total.is_finite() {
        return None;
    }
    let mut baht = (satang_total / 100.0).floor();
    let satang = (satang_total - baht * 100.0).clamp(0.0, 99.0) as u64;
    let mut text = String::new();
    if baht == 0.0 {
        if satang == 0 {
            text.push_str(DIGITS[0]);
        }
    } else {
        // Groups of six digits from the right; a group is prefixed by ล้าน when a higher group
        // follows it.
        while baht > 0.0 {
            let higher = (baht / 1e6).floor();
            let group = (baht - higher * 1e6).clamp(0.0, 999_999.0) as u64;
            let mut piece = String::new();
            if higher > 0.0 {
                piece.push_str(MILLION);
            }
            if group > 0 {
                block(&mut piece, group);
            }
            text.insert_str(0, &piece);
            baht = higher;
        }
    }
    if !text.is_empty() {
        text.push_str(BAHT);
    }
    if satang == 0 {
        text.push_str(EXACT);
    } else {
        block(&mut text, satang);
        text.push_str(SATANG);
    }
    if minus {
        text.insert_str(0, MINUS);
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microsoft_example_and_place_rules() {
        assert_eq!(
            baht_text(1234.0).as_deref(),
            Some("หนึ่งพันสองร้อยสามสิบสี่บาทถ้วน")
        );
        assert_eq!(baht_text(0.0).as_deref(), Some("ศูนย์บาทถ้วน"));
        assert_eq!(baht_text(11.0).as_deref(), Some("สิบเอ็ดบาทถ้วน"));
        assert_eq!(baht_text(21.0).as_deref(), Some("ยี่สิบเอ็ดบาทถ้วน"));
        assert_eq!(baht_text(10.0).as_deref(), Some("สิบบาทถ้วน"));
        assert_eq!(baht_text(1.0).as_deref(), Some("หนึ่งบาทถ้วน"));
        assert_eq!(baht_text(101.0).as_deref(), Some("หนึ่งร้อยหนึ่งบาทถ้วน"));
        assert_eq!(baht_text(0.5).as_deref(), Some("ห้าสิบสตางค์"));
        assert_eq!(baht_text(1.25).as_deref(), Some("หนึ่งบาทยี่สิบห้าสตางค์"));
        assert_eq!(baht_text(-2.0).as_deref(), Some("ลบสองบาทถ้วน"));
        assert_eq!(baht_text(1_000_000.0).as_deref(), Some("หนึ่งล้านบาทถ้วน"));
        assert_eq!(baht_text(2_000_001.0).as_deref(), Some("สองล้านหนึ่งบาทถ้วน"));
        assert_eq!(baht_text(0.004).as_deref(), Some("ศูนย์บาทถ้วน"));
        assert_eq!(baht_text(0.005).as_deref(), Some("หนึ่งสตางค์"));
        assert_eq!(baht_text(1.005).as_deref(), Some("หนึ่งบาทหนึ่งสตางค์"));
    }

    #[test]
    fn huge_amounts_terminate() {
        assert!(baht_text(1e300).is_some_and(|t| t.ends_with("บาทถ้วน")));
        assert_eq!(baht_text(f64::MAX), None);
    }
}
