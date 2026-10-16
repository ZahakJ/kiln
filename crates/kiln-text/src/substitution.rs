// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Office font substitution table (PLAN.md §1.2 item 7).
//!
//! Microsoft's fonts cannot be shipped, so documents that name them are laid out with metric-compatible
//! OFL families: identical advance widths mean identical line breaks. Fonts without a compatible open
//! counterpart are mapped to the closest bundled family and reported, so the Compatibility Checker can
//! tell the user what changed.

/// How a requested family was satisfied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SubstitutionKind {
    /// The requested family is installed or bundled: no substitution.
    Exact,
    /// Replaced by a family with identical advance widths (Calibri → Carlito, Arial → Liberation Sans):
    /// line breaks match the original.
    MetricCompatible,
    /// Replaced by a similar family whose widths differ: line breaks may differ.
    Approximate,
    /// Replaced by the suite's UI sans (Aptos and Segoe UI have no open metric-compatible counterpart).
    UiSans,
    /// A symbol font (Symbol, Wingdings, …) that is not available; text falls back to a regular family
    /// and symbol code points may render differently.
    SymbolFallback,
    /// The family is unknown and not installed; the default family for its style was used.
    Missing,
}

impl SubstitutionKind {
    /// True when the substitution can change line breaks, i.e. it should be reported to the user.
    #[must_use]
    pub fn is_lossy(self) -> bool {
        !matches!(self, Self::Exact | Self::MetricCompatible)
    }
}

/// Where the table sends a family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    /// The first of these families that is available.
    Families(&'static [&'static str]),
    /// The UI sans set with `FontStore::set_ui_sans`.
    UiSans,
    /// The default sans family; reported as a symbol-font fallback.
    Symbol,
    /// Whatever the script fallback chain provides (East Asian Office fonts).
    ScriptFallback,
}

/// One row of the table.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rule {
    pub(crate) office: &'static str,
    pub(crate) target: Target,
    pub(crate) kind: SubstitutionKind,
}

const SANS: &[&str] = &["Liberation Sans"];
const SERIF: &[&str] = &["Liberation Serif"];
const MONO: &[&str] = &["Liberation Mono"];
const CARLITO: &[&str] = &["Carlito"];
const CALADEA: &[&str] = &["Caladea"];
const NARROW: &[&str] = &["Liberation Sans Narrow", "Liberation Sans"];

const fn rule(office: &'static str, target: Target, kind: SubstitutionKind) -> Rule {
    Rule {
        office,
        target,
        kind,
    }
}

use SubstitutionKind::{Approximate, MetricCompatible, SymbolFallback};
use Target::{Families, ScriptFallback, Symbol, UiSans};

/// The table. Names compare case-insensitively.
pub(crate) const RULES: &[Rule] = &[
    // Metric-compatible (the reason these families exist).
    rule("Calibri", Families(CARLITO), MetricCompatible),
    rule("Cambria", Families(CALADEA), MetricCompatible),
    rule("Arial", Families(SANS), MetricCompatible),
    rule("Helvetica", Families(SANS), MetricCompatible),
    rule("Arimo", Families(SANS), MetricCompatible),
    rule("Times New Roman", Families(SERIF), MetricCompatible),
    rule("Times", Families(SERIF), MetricCompatible),
    rule("Tinos", Families(SERIF), MetricCompatible),
    rule("Courier New", Families(MONO), MetricCompatible),
    rule("Courier", Families(MONO), MetricCompatible),
    rule("Cousine", Families(MONO), MetricCompatible),
    // Same design family, different weight or optical size.
    rule("Calibri Light", Families(CARLITO), Approximate),
    rule("Cambria Math", Families(CALADEA), Approximate),
    rule("Arial Narrow", Families(NARROW), Approximate),
    rule("Helvetica Neue", Families(SANS), Approximate),
    rule("Arial Unicode MS", Families(SANS), Approximate),
    // Office defaults without an open metric-compatible counterpart.
    rule("Aptos", UiSans, SubstitutionKind::UiSans),
    rule("Aptos Display", UiSans, SubstitutionKind::UiSans),
    rule("Aptos Narrow", UiSans, SubstitutionKind::UiSans),
    rule("Aptos Light", UiSans, SubstitutionKind::UiSans),
    rule("Segoe UI", UiSans, SubstitutionKind::UiSans),
    rule("Segoe UI Light", UiSans, SubstitutionKind::UiSans),
    rule("Segoe UI Semibold", UiSans, SubstitutionKind::UiSans),
    // Common Windows families → closest bundled style.
    rule("Verdana", Families(SANS), Approximate),
    rule("Tahoma", Families(SANS), Approximate),
    rule("Trebuchet MS", Families(SANS), Approximate),
    rule("Century Gothic", Families(SANS), Approximate),
    rule("Franklin Gothic Book", Families(SANS), Approximate),
    rule("Gill Sans MT", Families(SANS), Approximate),
    rule("Lucida Sans Unicode", Families(SANS), Approximate),
    rule("Corbel", Families(CARLITO), Approximate),
    rule("Candara", Families(CARLITO), Approximate),
    rule("Georgia", Families(SERIF), Approximate),
    rule("Garamond", Families(SERIF), Approximate),
    rule("Book Antiqua", Families(SERIF), Approximate),
    rule("Palatino Linotype", Families(SERIF), Approximate),
    rule("Constantia", Families(CALADEA), Approximate),
    rule("Consolas", Families(MONO), Approximate),
    rule("Lucida Console", Families(MONO), Approximate),
    rule("Cascadia Code", Families(MONO), Approximate),
    rule("Cascadia Mono", Families(MONO), Approximate),
    // Symbol fonts: no open equivalent; reported.
    rule("Symbol", Symbol, SymbolFallback),
    rule("Wingdings", Symbol, SymbolFallback),
    rule("Wingdings 2", Symbol, SymbolFallback),
    rule("Wingdings 3", Symbol, SymbolFallback),
    rule("Webdings", Symbol, SymbolFallback),
    rule("Marlett", Symbol, SymbolFallback),
    rule("MT Extra", Symbol, SymbolFallback),
    // East Asian Office fonts: resolved through the CJK script fallback chain.
    rule("MS Mincho", ScriptFallback, Approximate),
    rule("MS Gothic", ScriptFallback, Approximate),
    rule("MS PGothic", ScriptFallback, Approximate),
    rule("Yu Gothic", ScriptFallback, Approximate),
    rule("Yu Mincho", ScriptFallback, Approximate),
    rule("Meiryo", ScriptFallback, Approximate),
    rule("SimSun", ScriptFallback, Approximate),
    rule("SimHei", ScriptFallback, Approximate),
    rule("DengXian", ScriptFallback, Approximate),
    rule("Microsoft YaHei", ScriptFallback, Approximate),
    rule("Microsoft JhengHei", ScriptFallback, Approximate),
    rule("PMingLiU", ScriptFallback, Approximate),
    rule("Malgun Gothic", ScriptFallback, Approximate),
    rule("Batang", ScriptFallback, Approximate),
    // Arabic and Hebrew Office fonts.
    rule("Traditional Arabic", ScriptFallback, Approximate),
    rule("Simplified Arabic", ScriptFallback, Approximate),
    rule("Arabic Typesetting", ScriptFallback, Approximate),
    rule("Sakkal Majalla", ScriptFallback, Approximate),
    rule("David", Families(SANS), Approximate),
    rule("Miriam", Families(SANS), Approximate),
];

/// Looks a family up in the table (case-insensitive).
pub(crate) fn lookup(name: &str) -> Option<&'static Rule> {
    let name = name.trim();
    RULES.iter().find(|r| r.office.eq_ignore_ascii_case(name))
}

/// The broad style of an unknown family, guessed from its name, used to pick a default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StyleGuess {
    Sans,
    Serif,
    Mono,
}

/// Guesses whether an unknown family is serif, sans or monospaced from its name.
pub(crate) fn guess_style(name: &str) -> StyleGuess {
    let lower = name.to_ascii_lowercase();
    let has = |needle: &str| lower.contains(needle);
    if has("mono") || has("courier") || has("code") || has("console") || has("typewriter") {
        StyleGuess::Mono
    } else if (has("serif") && !has("sans"))
        || has("roman")
        || has("times")
        || has("mincho")
        || has("song")
        || has("ming")
        || has("antiqua")
        || has("garamond")
        || has("bodoni")
    {
        StyleGuess::Serif
    } else {
        StyleGuess::Sans
    }
}

/// The bundled family used for a style guess.
pub(crate) fn default_family(style: StyleGuess) -> &'static str {
    match style {
        StyleGuess::Sans => "Liberation Sans",
        StyleGuess::Serif => "Liberation Serif",
        StyleGuess::Mono => "Liberation Mono",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_defaults_map_to_metric_compatible_families() {
        let calibri = lookup("calibri").expect("Calibri is in the table");
        assert_eq!(calibri.target, Families(CARLITO));
        assert_eq!(calibri.kind, MetricCompatible);
        assert_eq!(
            lookup("Times New Roman").map(|r| r.kind),
            Some(MetricCompatible)
        );
        assert_eq!(
            lookup(" Courier New ").map(|r| r.target),
            Some(Families(MONO))
        );
        assert_eq!(lookup("Aptos").map(|r| r.target), Some(UiSans));
        assert_eq!(lookup("Wingdings").map(|r| r.kind), Some(SymbolFallback));
        assert!(lookup("Comic Sans Fantasy").is_none());
    }

    #[test]
    fn table_has_no_duplicate_rows() {
        for (i, a) in RULES.iter().enumerate() {
            for b in &RULES[i + 1..] {
                assert!(!a.office.eq_ignore_ascii_case(b.office), "{}", a.office);
            }
        }
    }

    #[test]
    fn unknown_families_are_classified_by_name() {
        assert_eq!(guess_style("Noto Serif Display"), StyleGuess::Serif);
        assert_eq!(guess_style("Noto Sans"), StyleGuess::Sans);
        assert_eq!(guess_style("Fira Code"), StyleGuess::Mono);
        assert_eq!(guess_style("Bookman Old Style"), StyleGuess::Sans);
        assert!(SubstitutionKind::Approximate.is_lossy());
        assert!(!SubstitutionKind::MetricCompatible.is_lossy());
    }
}
