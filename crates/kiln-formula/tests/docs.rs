// SPDX-License-Identifier: AGPL-3.0-or-later
//! The function documentation (`crates/kiln-formula/docs/*.json`): every registered function has
//! exactly one entry and every entry a function, the entries are well formed, and every worked
//! example evaluates to the result it states (to its displayed precision).

mod conformance_support;

use std::collections::BTreeSet;
use std::sync::Arc;

use conformance_support::{NOW, compare};
use kiln_formula::engine::{Engine, EngineOptions};
use kiln_formula::functions::{self, DocFamily, doc, docs};
use kiln_formula::registry::{DocsHandle, MAX_ARGS};

const SINCE: [&str; 8] = [
    "2007", "2010", "2013", "2016", "2019", "2021", "2024", "365",
];

#[test]
fn every_function_is_documented_and_every_doc_has_a_function() {
    let names: BTreeSet<&str> = functions::function_names().into_iter().collect();
    assert!(names.len() >= 484, "{} functions", names.len());
    let missing: Vec<&str> = names.iter().copied().filter(|n| doc(n).is_none()).collect();
    assert!(missing.is_empty(), "undocumented: {missing:?}");
    let orphans: Vec<&str> = docs()
        .iter()
        .map(|d| d.name.as_str())
        .filter(|n| !names.contains(n))
        .collect();
    assert!(
        orphans.is_empty(),
        "documented but not registered: {orphans:?}"
    );
    let mut seen = BTreeSet::new();
    let dupes: Vec<&str> = docs()
        .iter()
        .map(|d| d.name.as_str())
        .filter(|n| !seen.insert(*n))
        .collect();
    assert!(dupes.is_empty(), "documented twice: {dupes:?}");
    assert_eq!(docs().len(), names.len());
    println!(
        "{} functions, {} documentation entries",
        names.len(),
        docs().len()
    );
}

#[test]
fn every_spec_points_at_its_documentation() {
    for spec in functions::all_specs() {
        match spec.docs {
            DocsHandle::Key(k) => {
                assert_eq!(k, spec.name, "{} has the docs key {k}", spec.name);
                assert!(doc(k).is_some(), "{k}: no entry");
            }
            _ => panic!("{} is not documented through the docs JSON", spec.name),
        }
    }
}

#[test]
fn every_entry_is_well_formed() {
    let specs = functions::all_specs();
    for family in DocFamily::ALL {
        assert!(
            docs().iter().any(|d| d.family == family),
            "{} has no entries",
            family.file_stem()
        );
    }
    for d in docs() {
        let at = format!("{}/{}", d.family.file_stem(), d.name);
        assert!(d.summary.len() > 10, "{at}: summary too short");
        assert!(
            d.syntax.starts_with(&format!("{}(", d.name)),
            "{at}: syntax {}",
            d.syntax
        );
        assert!(!d.examples.is_empty(), "{at}: no example");
        assert!(SINCE.contains(&d.since.as_str()), "{at}: since {}", d.since);
        for a in &d.args {
            assert!(!a.name.is_empty(), "{at}: unnamed argument");
            assert!(!a.description.is_empty(), "{at}: {} undescribed", a.name);
        }
        let spec = specs.iter().find(|s| s.name == d.name);
        if let Some(s) = spec
            && s.max_args < MAX_ARGS
        {
            let required = d.args.iter().filter(|a| !a.optional).count();
            assert!(
                required <= usize::from(s.max_args),
                "{at}: {required} required arguments, at most {}",
                s.max_args
            );
        }
    }
}

#[test]
fn xlookup_has_a_signature_and_examples() {
    let x = doc("xlookup").expect("XLOOKUP");
    assert!(x.syntax.starts_with("XLOOKUP(lookup_value"));
    assert!(!x.examples.is_empty());
    assert_eq!(doc("_xlfn.XLOOKUP").map(|d| &d.name), Some(&x.name));
}

#[test]
fn every_example_evaluates_as_documented() {
    let mut engine = Engine::with_options(EngineOptions {
        clock: Some(Arc::new(|| NOW)),
        ..functions::engine_options()
    });
    let sheet = engine.add_sheet("Sheet1").expect("Sheet1");
    let (mut total, mut failures) = (0usize, Vec::new());
    for d in docs() {
        for x in &d.examples {
            total += 1;
            let result = match engine.evaluate_at(&x.formula, sheet, 0, 0, false) {
                Ok(v) => compare(&v, &x.result, true),
                Err(e) => Err(format!("does not parse: {e}")),
            };
            if let Err(msg) = result {
                failures.push(format!("{}: {} → {msg}", d.name, x.formula));
            }
        }
    }
    println!(
        "{total} documented examples evaluated, {} failed",
        failures.len()
    );
    assert!(total >= docs().len());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
