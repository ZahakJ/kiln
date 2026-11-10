// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sort order for Data › Sort: multiple keys, custom lists, case sensitivity, colour sorts and
//! locale-aware collation (ICU4X, with the `collation` feature).
//!
//! Values compare like Excel: numbers < text < logicals < errors, blanks always last in either
//! direction; text ignores case unless the sort is case-sensitive, where lowercase sorts before
//! uppercase. Ties keep their original order (the sort is stable).

use std::cmp::Ordering;
use std::ops::RangeInclusive;

use kiln_formula::SheetId;
use kiln_formula::values::{compare_text, sort_compare};

use crate::color::Color;
use crate::commands::SortKey;
use crate::meta::SortBy;
use crate::value::CellValue;
use crate::workbook::Workbook;

/// Compares text: Excel's order, or a locale's collation.
pub struct TextOrder {
    #[cfg(feature = "collation")]
    collator: Option<icu_collator::CollatorBorrowed<'static>>,
    case_sensitive: bool,
}

impl std::fmt::Debug for TextOrder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextOrder")
            .field("case_sensitive", &self.case_sensitive)
            .finish_non_exhaustive()
    }
}

impl TextOrder {
    /// Excel's order (or `locale`'s collation when given and supported).
    pub fn new(locale: Option<&str>, case_sensitive: bool) -> TextOrder {
        #[cfg(feature = "collation")]
        {
            use icu_collator::options::{CollatorOptions, Strength};
            use icu_collator::preferences::CollationCaseFirst;
            let collator = locale.and_then(|tag| {
                let loc = icu_locale_core::Locale::try_from_str(tag).ok()?;
                let mut prefs = icu_collator::CollatorPreferences::from(&loc);
                let mut options = CollatorOptions::default();
                if case_sensitive {
                    prefs.case_first = Some(CollationCaseFirst::Lower);
                    options.strength = Some(Strength::Tertiary);
                } else {
                    options.strength = Some(Strength::Secondary);
                }
                icu_collator::Collator::try_new(prefs, options).ok()
            });
            TextOrder {
                collator,
                case_sensitive,
            }
        }
        #[cfg(not(feature = "collation"))]
        {
            let _ = locale;
            TextOrder { case_sensitive }
        }
    }

    /// Compares two texts.
    pub fn compare(&self, a: &str, b: &str) -> Ordering {
        #[cfg(feature = "collation")]
        if let Some(c) = &self.collator {
            return c.compare(a, b);
        }
        let primary = compare_text(a, b);
        if primary != Ordering::Equal || !self.case_sensitive || a == b {
            return primary;
        }
        // Excel's case-sensitive order puts lowercase first.
        for (x, y) in a.chars().zip(b.chars()) {
            if x != y {
                return match (x.is_lowercase(), y.is_lowercase()) {
                    (true, false) => Ordering::Less,
                    (false, true) => Ordering::Greater,
                    _ => x.cmp(&y),
                };
            }
        }
        a.len().cmp(&b.len())
    }
}

/// Excel's comparison of two cell values for sorting, blanks (and empty text) last.
pub fn compare_cells(a: &CellValue, b: &CellValue, text: &TextOrder) -> Ordering {
    let (ab, bb) = (a.is_empty_text_or_blank(), b.is_empty_text_or_blank());
    if ab || bb {
        return ab.cmp(&bb);
    }
    match (a.as_text(), b.as_text()) {
        (Some(x), Some(y)) => text.compare(x, y),
        _ => sort_compare(&a.to_value(), &b.to_value()),
    }
}

fn list_index(list: &[String], v: &CellValue) -> Option<usize> {
    let t = v.as_text()?;
    list.iter()
        .position(|x| x.to_uppercase() == t.to_uppercase())
}

/// The lines (`lines`, rows or with `by_columns` columns) in sorted order.
#[allow(clippy::too_many_arguments)]
pub fn sort_order(
    wb: &Workbook,
    sheet: SheetId,
    lines: RangeInclusive<u32>,
    keys: &[SortKey],
    case_sensitive: bool,
    by_columns: bool,
    locale: Option<&str>,
) -> Vec<u32> {
    let text = TextOrder::new(locale, case_sensitive);
    let theme = wb.theme().clone();
    let resolve = |c: Option<Color>, auto: u32| c.map_or(auto, |c| c.resolve(&theme, auto));
    // Precompute every key's value per line.
    let lines: Vec<u32> = lines.collect();
    let columns: Vec<Vec<KeyValue>> = keys
        .iter()
        .map(|k| {
            lines
                .iter()
                .map(|&line| {
                    let (row, col) = if by_columns {
                        (k.index, line)
                    } else {
                        (line, k.index)
                    };
                    match k.sort_by {
                        SortBy::Value | SortBy::Icon => KeyValue::Value(wb.value(sheet, row, col)),
                        SortBy::CellColor => {
                            let st = wb.sheet(sheet).map_or(0, |s| s.style_at(row, col));
                            KeyValue::Color(
                                wb.styles()
                                    .fill_of(st)
                                    .main_color()
                                    .map(|c| c.resolve(&theme, 0xFFFFFF)),
                            )
                        }
                        SortBy::FontColor => {
                            let st = wb.sheet(sheet).map_or(0, |s| s.style_at(row, col));
                            KeyValue::Color(Some(resolve(wb.styles().font_of(st).color, 0)))
                        }
                    }
                })
                .collect()
        })
        .collect();
    let mut idx: Vec<usize> = (0..lines.len()).collect();
    idx.sort_by(|&x, &y| {
        for (k, key) in keys.iter().enumerate() {
            let c = compare_key(key, &columns[k][x], &columns[k][y], &text, &theme);
            if c != Ordering::Equal {
                return c;
            }
        }
        Ordering::Equal
    });
    idx.into_iter().map(|i| lines[i]).collect()
}

enum KeyValue {
    Value(CellValue),
    Color(Option<u32>),
}

fn compare_key(
    key: &SortKey,
    a: &KeyValue,
    b: &KeyValue,
    text: &TextOrder,
    theme: &crate::Theme,
) -> Ordering {
    match (a, b) {
        (KeyValue::Value(a), KeyValue::Value(b)) => {
            let (ab, bb) = (a.is_empty_text_or_blank(), b.is_empty_text_or_blank());
            if ab || bb {
                // Blanks sort last in either direction.
                return ab.cmp(&bb);
            }
            let c = match &key.custom_list {
                Some(list) => match (list_index(list, a), list_index(list, b)) {
                    (Some(i), Some(j)) => i.cmp(&j),
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => compare_cells(a, b, text),
                },
                None => compare_cells(a, b, text),
            };
            if key.descending { c.reverse() } else { c }
        }
        (KeyValue::Color(a), KeyValue::Color(b)) => {
            // The chosen colour goes on top (bottom when descending); others keep their order.
            let target = key.color.map(|c| c.resolve(theme, 0xFFFFFF));
            let hit = |c: &Option<u32>| target.is_some() && *c == target;
            let c = hit(b).cmp(&hit(a));
            if key.descending { c.reverse() } else { c }
        }
        _ => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn orders_like_excel() {
        let t = TextOrder::new(None, false);
        let mut v = [
            CellValue::Blank,
            CellValue::text("b"),
            CellValue::Bool(true),
            CellValue::Number(10.0),
            CellValue::text("A"),
            CellValue::Error(kiln_formula::values::ErrorValue::NA),
            CellValue::Number(-1.0),
            CellValue::text(""),
        ];
        v.sort_by(|a, b| compare_cells(a, b, &t));
        let shown: Vec<String> = v.iter().map(CellValue::display_general).collect();
        assert_eq!(shown, vec!["-1", "10", "A", "b", "TRUE", "#N/A", "", ""]);
    }

    #[test]
    fn case_sensitive_puts_lowercase_first() {
        let t = TextOrder::new(None, true);
        assert_eq!(t.compare("apple", "Apple"), Ordering::Less);
        assert_eq!(t.compare("Apple", "apple"), Ordering::Greater);
        assert_eq!(t.compare("apple", "banana"), Ordering::Less);
        let i = TextOrder::new(None, false);
        assert_eq!(i.compare("apple", "APPLE"), Ordering::Equal);
    }

    #[cfg(feature = "collation")]
    #[test]
    fn collates_by_locale() {
        // Swedish sorts Ä after Z; German sorts it with A.
        let sv = TextOrder::new(Some("sv"), false);
        let de = TextOrder::new(Some("de"), false);
        assert_eq!(sv.compare("Äpple", "Zebra"), Ordering::Greater);
        assert_eq!(de.compare("Äpfel", "Zebra"), Ordering::Less);
        let unknown = TextOrder::new(Some("not a locale!"), false);
        assert_eq!(unknown.compare("b", "A"), Ordering::Greater);
    }
}
