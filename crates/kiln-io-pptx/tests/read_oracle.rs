// SPDX-License-Identifier: AGPL-3.0-or-later
//! The python-pptx oracle: every deck of the corpus read by Kiln gives the slides, layouts,
//! shapes (kind, name, nesting), text per shape and table cell, and notes that python-pptx
//! reports (`fixtures/pptx/read/*.pptx.json`, written by `tools/make-fixtures.py --dump-pptx`).

mod read_support;

use read_support::{decks, oracle_dump, oracle_view};
use serde_json::Value;

/// The first difference between two JSON values, as a path.
fn diff(path: &str, a: &Value, b: &Value) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                let p = format!("{path}.{k}");
                match y.get(k) {
                    Some(w) => {
                        if let Some(d) = diff(&p, v, w) {
                            return Some(d);
                        }
                    }
                    None => return Some(format!("{p}: missing in the oracle")),
                }
            }
            y.keys()
                .find(|k| !x.contains_key(*k))
                .map(|k| format!("{path}.{k}: missing in Kiln's view"))
        }
        (Value::Array(x), Value::Array(y)) => {
            for (i, (v, w)) in x.iter().zip(y).enumerate() {
                if let Some(d) = diff(&format!("{path}[{i}]"), v, w) {
                    return Some(d);
                }
            }
            (x.len() != y.len()).then(|| format!("{path}: {} items, oracle {}", x.len(), y.len()))
        }
        _ => (a != b).then(|| format!("{path}: kiln {a} != oracle {b}")),
    }
}

fn check(dir: &str, require_dump: bool) -> usize {
    let mut checked = 0;
    for deck in decks(dir) {
        let rel = deck.display().to_string();
        let bytes = std::fs::read(&deck).unwrap();
        let read = kiln_io_pptx::read_pptx(&bytes).unwrap_or_else(|e| panic!("{rel}: {e}"));
        read.presentation
            .validate()
            .unwrap_or_else(|e| panic!("{rel}: {e}"));
        let Some(dump) = oracle_dump(&deck) else {
            assert!(
                !require_dump,
                "{rel} has no oracle dump: run tools/make-fixtures.py --dump-pptx"
            );
            continue;
        };
        let ours = oracle_view(&read);
        if let Some(d) = diff("$", &ours, &dump) {
            panic!("{rel}: {d}");
        }
        checked += 1;
    }
    checked
}

#[test]
fn feature_and_manual_decks_match_python_pptx() {
    let n = check("pptx/features", true) + check("pptx/manual", true);
    assert!(n >= 19, "only {n} decks compared");
}

#[test]
fn external_decks_match_python_pptx() {
    // Not an acceptance criterion (the external corpus is not ours, and only there after
    // `tools/fetch-corpus.sh`), but every deck with a dump matches too.
    let present = read_support::decks("external/pptx")
        .iter()
        .filter(|d| oracle_dump(d).is_some())
        .count();
    assert_eq!(check("external/pptx", false), present);
}

#[test]
fn diffs_name_the_first_difference() {
    let a: Value = serde_json::json!({"s": [1, {"t": "x"}]});
    let b: Value = serde_json::json!({"s": [1, {"t": "y"}]});
    assert_eq!(
        diff("$", &a, &b).unwrap(),
        "$.s[1].t: kiln \"x\" != oracle \"y\""
    );
    assert!(diff("$", &a, &a).is_none());
    let c: Value = serde_json::json!({"s": [1]});
    assert!(diff("$", &a, &c).unwrap().contains("items"));
}
