// SPDX-License-Identifier: AGPL-3.0-or-later
//! Function documentation: signature, argument descriptions, examples and the Excel version that
//! introduced the function, from `crates/kiln-formula/docs/<FAMILY>.json` (compiled in). Used by
//! autocompletion, the Insert Function dialog and the Formula Lab; every library function has
//! exactly one entry (`tests/docs.rs`).

use std::sync::OnceLock;

use rustc_hash::FxHashMap;
use serde::Deserialize;

use crate::registry::FunctionCategory;

/// A documentation file, in the order the Insert Function dialog lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DocFamily {
    /// Financial.
    Financial,
    /// Logical.
    Logical,
    /// Text.
    Text,
    /// Date & Time.
    DateTime,
    /// Lookup & Reference.
    Lookup,
    /// Math & Trig.
    Math,
    /// Statistical.
    Statistical,
    /// Engineering.
    Engineering,
    /// Information.
    Information,
    /// Database.
    Database,
    /// Web.
    Web,
    /// Compatibility (pre-2010 names).
    Compatibility,
}

impl DocFamily {
    /// Every family, in dialog order.
    pub const ALL: [DocFamily; 12] = [
        DocFamily::Financial,
        DocFamily::Logical,
        DocFamily::Text,
        DocFamily::DateTime,
        DocFamily::Lookup,
        DocFamily::Math,
        DocFamily::Statistical,
        DocFamily::Engineering,
        DocFamily::Information,
        DocFamily::Database,
        DocFamily::Web,
        DocFamily::Compatibility,
    ];

    /// The file stem (`MATH` for `docs/MATH.json`).
    pub const fn file_stem(self) -> &'static str {
        match self {
            DocFamily::Financial => "FINANCIAL",
            DocFamily::Logical => "LOGICAL",
            DocFamily::Text => "TEXT",
            DocFamily::DateTime => "DATETIME",
            DocFamily::Lookup => "LOOKUP",
            DocFamily::Math => "MATH",
            DocFamily::Statistical => "STATISTICAL",
            DocFamily::Engineering => "ENGINEERING",
            DocFamily::Information => "INFORMATION",
            DocFamily::Database => "DATABASE",
            DocFamily::Web => "WEB",
            DocFamily::Compatibility => "COMPATIBILITY",
        }
    }

    const fn json(self) -> &'static str {
        match self {
            DocFamily::Financial => include_str!("../../docs/FINANCIAL.json"),
            DocFamily::Logical => include_str!("../../docs/LOGICAL.json"),
            DocFamily::Text => include_str!("../../docs/TEXT.json"),
            DocFamily::DateTime => include_str!("../../docs/DATETIME.json"),
            DocFamily::Lookup => include_str!("../../docs/LOOKUP.json"),
            DocFamily::Math => include_str!("../../docs/MATH.json"),
            DocFamily::Statistical => include_str!("../../docs/STATISTICAL.json"),
            DocFamily::Engineering => include_str!("../../docs/ENGINEERING.json"),
            DocFamily::Information => include_str!("../../docs/INFORMATION.json"),
            DocFamily::Database => include_str!("../../docs/DATABASE.json"),
            DocFamily::Web => include_str!("../../docs/WEB.json"),
            DocFamily::Compatibility => include_str!("../../docs/COMPATIBILITY.json"),
        }
    }
}

/// One argument of a function's signature.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ArgDocEntry {
    /// Name as shown in the signature (`lookup_value`).
    pub name: String,
    /// What it is.
    pub description: String,
    /// May be omitted.
    #[serde(default)]
    pub optional: bool,
    /// May repeat (`number2, …`).
    #[serde(default)]
    pub repeating: bool,
}

/// A worked example: a formula and the result it shows.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ExampleEntry {
    /// The formula, with `=`.
    pub formula: String,
    /// The result as a formula literal (`6`, `"text"`, `TRUE`, `#N/A`, `{1,2;3,4}`).
    pub result: String,
}

#[derive(Debug, Deserialize)]
struct RawDoc {
    name: String,
    category: String,
    summary: String,
    syntax: String,
    #[serde(default)]
    args: Vec<ArgDocEntry>,
    #[serde(default)]
    examples: Vec<ExampleEntry>,
    #[serde(default)]
    since: String,
}

/// The documentation of one function.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDoc {
    /// Upper-case name.
    pub name: String,
    /// The documentation file it comes from.
    pub family: DocFamily,
    /// Category in the Insert Function dialog.
    pub category: FunctionCategory,
    /// The category as Excel names it (`Lookup & Reference`).
    pub category_name: String,
    /// One sentence.
    pub summary: String,
    /// The signature, e.g. `XLOOKUP(lookup_value, lookup_array, return_array, …)`.
    pub syntax: String,
    /// Arguments.
    pub args: Vec<ArgDocEntry>,
    /// Examples that evaluate as stated (checked by `tests/docs.rs`).
    pub examples: Vec<ExampleEntry>,
    /// The Excel version that introduced the function (`2007`, `2010` … `365`).
    pub since: String,
}

/// The [`FunctionCategory`] for an Excel category name.
pub fn category_from_name(name: &str) -> Option<FunctionCategory> {
    Some(match name {
        "Math & Trig" => FunctionCategory::MathTrig,
        "Statistical" => FunctionCategory::Statistical,
        "Logical" => FunctionCategory::Logical,
        "Text" => FunctionCategory::Text,
        "Date & Time" => FunctionCategory::DateTime,
        "Lookup & Reference" => FunctionCategory::Lookup,
        "Information" => FunctionCategory::Information,
        "Financial" => FunctionCategory::Financial,
        "Engineering" => FunctionCategory::Engineering,
        "Database" => FunctionCategory::Database,
        "Web" => FunctionCategory::Web,
        "Compatibility" => FunctionCategory::Compatibility,
        "Dynamic Array" => FunctionCategory::DynamicArray,
        _ => return None,
    })
}

struct Catalog {
    entries: Vec<FunctionDoc>,
    by_name: FxHashMap<String, usize>,
}

fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        let mut entries = Vec::with_capacity(500);
        for family in DocFamily::ALL {
            // The files are compiled in and checked by tests/docs.rs; a malformed file would
            // only drop its entries, never panic.
            let raw: Vec<RawDoc> = serde_json::from_str(family.json()).unwrap_or_default();
            for r in raw {
                let Some(category) = category_from_name(&r.category) else {
                    continue;
                };
                entries.push(FunctionDoc {
                    name: r.name.to_uppercase(),
                    family,
                    category,
                    category_name: r.category,
                    summary: r.summary,
                    syntax: r.syntax,
                    args: r.args,
                    examples: r.examples,
                    since: r.since,
                });
            }
        }
        let by_name = entries
            .iter()
            .enumerate()
            .map(|(i, e)| (e.name.clone(), i))
            .collect();
        Catalog { entries, by_name }
    })
}

/// Every documentation entry, family by family in dialog order.
pub fn docs() -> &'static [FunctionDoc] {
    &catalog().entries
}

/// The documentation of a function (case-insensitive, `_xlfn.` prefixes allowed).
pub fn doc(name: &str) -> Option<&'static FunctionDoc> {
    let c = catalog();
    let key = crate::registry::normalize_function_name(name);
    c.by_name.get(&key).map(|&i| &c.entries[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_parses() {
        for family in DocFamily::ALL {
            let raw: Result<Vec<RawDoc>, _> = serde_json::from_str(family.json());
            assert!(raw.is_ok(), "{}: {:?}", family.file_stem(), raw.err());
        }
        assert!(docs().len() >= 484);
        let x = doc("_xlfn.xlookup").map(|d| d.syntax.as_str());
        assert!(x.is_some_and(|s| s.starts_with("XLOOKUP(")));
    }
}
