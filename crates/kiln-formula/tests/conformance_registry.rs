// SPDX-License-Identifier: AGPL-3.0-or-later
//! The library keeps the v1.0 TypeScript registry's contract: the same 484 names, arities,
//! volatility, lazy arguments and lifted arguments. `conformance_registry_v1.json` is a snapshot of
//! the v1.0 `listFunctions()` (masks as arithmetic runs `[start, step, count]`, `"all"` for every
//! position; a variadic maximum is 255).
//!
//! `ArgMask` lists positions below 64 one by one; the criteria of `COUNTIFS`, `SUMIFS`,
//! `AVERAGEIFS`, `MAXIFS` and `MINIFS` beyond that are lifted by the implementations themselves,
//! which `criteria_beyond_position_64_are_lifted` checks by evaluation.

mod conformance_support;

use std::collections::BTreeMap;

use kiln_formula::functions;
use kiln_formula::registry::ArgMask;
use serde_json::Value as Json;

fn positions(runs: &Json) -> Vec<usize> {
    let mut out = Vec::new();
    for r in runs.as_array().expect("runs") {
        let r: Vec<usize> = r
            .as_array()
            .expect("run")
            .iter()
            .map(|n| n.as_u64().expect("index") as usize)
            .collect();
        let [start, step, count] = r[..] else {
            panic!("bad run {r:?}")
        };
        out.extend((0..count).map(|k| start + k * step));
    }
    out
}

/// Functions whose lifted criteria beyond position 63 are handled by the implementation.
const LIFT_TAIL: [&str; 5] = ["AVERAGEIFS", "COUNTIFS", "MAXIFS", "MINIFS", "SUMIFS"];

fn mask_matches(name: &str, what: &str, mask: ArgMask, want: &Json, max: usize) -> Vec<String> {
    let all = want.as_str() == Some("all");
    let listed = if all { Vec::new() } else { positions(want) };
    let checked = if what == "lifted" && LIFT_TAIL.contains(&name) {
        max.min(64)
    } else {
        max
    };
    (0..checked)
        .filter(|&i| mask.contains(i) != (all || listed.contains(&i)))
        .map(|i| {
            format!(
                "{name}: argument {i} is {}{what} in v1.0",
                if mask.contains(i) { "not " } else { "" }
            )
        })
        .collect()
}

#[test]
fn registry_matches_the_v1_snapshot() {
    let snapshot: Vec<Json> =
        serde_json::from_str(include_str!("conformance_registry_v1.json")).expect("snapshot");
    let specs: BTreeMap<&str, _> = functions::all_specs()
        .into_iter()
        .map(|s| (s.name, s))
        .collect();
    let mut problems = Vec::new();
    for want in &snapshot {
        let name = want["name"].as_str().expect("name");
        let Some(s) = specs.get(name) else {
            problems.push(format!("{name}: missing"));
            continue;
        };
        let (min, max) = (
            want["min"].as_u64().expect("min"),
            want["max"].as_u64().expect("max"),
        );
        if u64::from(s.min_args) != min || u64::from(s.max_args) != max {
            problems.push(format!(
                "{name}: arity {}..{}, v1.0 {min}..{max}",
                s.min_args, s.max_args
            ));
        }
        if s.volatile != want["volatile"].as_bool().expect("volatile") {
            problems.push(format!("{name}: volatility differs"));
        }
        let max = usize::from(s.max_args);
        problems.extend(mask_matches(name, "lazy", s.lazy, &want["lazy"], max));
        problems.extend(mask_matches(name, "lifted", s.lift, &want["lift"], max));
    }
    for name in specs.keys() {
        if !snapshot.iter().any(|w| w["name"].as_str() == Some(name)) {
            problems.push(format!("{name}: not in v1.0"));
        }
    }
    println!(
        "{} functions compared with the v1.0 registry",
        snapshot.len()
    );
    assert_eq!(snapshot.len(), 484);
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn criteria_beyond_position_64_are_lifted() {
    let mut engine = conformance_support::make_engine();
    let calc = engine.sheet_id("Calc").expect("Calc");
    // 33 pairs: the last criterion is argument 65 (0-based) of COUNTIFS, 66 of SUMIFS.
    let pairs = "Data!A1:A10,\">0\",".repeat(32);
    for (formula, want) in [
        (
            format!("=COUNTIFS({pairs}Data!A1:A10,{{\">10\",\">50\"}})"),
            [9.0, 5.0],
        ),
        (
            format!("=SUMIFS(Data!A1:A10,{pairs}Data!A1:A10,{{\">10\",\">50\"}})"),
            [540.0, 400.0],
        ),
        (
            format!("=MAXIFS(Data!A1:A10,{pairs}Data!A1:A10,{{\"<50\",\"<20\"}})"),
            [40.0, 10.0],
        ),
    ] {
        let v = engine
            .evaluate_at(&formula, calc, 0, 0, false)
            .expect("parses");
        conformance_support::compare(&v, &format!("{{{},{}}}", want[0], want[1]), false)
            .unwrap_or_else(|e| panic!("{}: {e}", &formula[..9]));
    }
}

/// `conformance/functions-implemented.json`, read by `cargo xtask parity`: every library function
/// is `implemented`; every function the parity tracker plans (`parity/functions.json`, status
/// `planned`) that the library lacks is `planned`. Run with `KILN_BLESS=1` to rewrite it.
#[test]
fn functions_implemented_json_is_current() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let tracker: Vec<Json> = serde_json::from_str(
        &std::fs::read_to_string(root.join("parity/functions.json"))
            .expect("parity/functions.json"),
    )
    .expect("parity/functions.json");
    let implemented = functions::function_names();
    let mut want: BTreeMap<String, &str> = implemented
        .iter()
        .map(|n| ((*n).to_owned(), "implemented"))
        .collect();
    for f in &tracker {
        let name = f["name"].as_str().expect("name");
        if f["status"].as_str() == Some("planned") {
            want.entry(name.to_owned()).or_insert("planned");
        }
    }
    let untracked: Vec<&&str> = implemented
        .iter()
        .filter(|n| !tracker.iter().any(|f| f["name"].as_str() == Some(**n)))
        .collect();
    assert!(
        untracked.is_empty(),
        "add these to parity/functions.json: {untracked:?}"
    );
    let mut text = String::from("{\n");
    let last = want.len().saturating_sub(1);
    for (i, (name, status)) in want.iter().enumerate() {
        let comma = if i == last { "" } else { "," };
        text.push_str(&format!("  \"{name}\": \"{status}\"{comma}\n"));
    }
    text.push_str("}\n");
    let path = root.join("conformance/functions-implemented.json");
    if std::env::var_os("KILN_BLESS").is_some() {
        std::fs::write(&path, &text).expect("write functions-implemented.json");
    }
    let on_disk = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        on_disk == text,
        "conformance/functions-implemented.json is stale: run KILN_BLESS=1 cargo nextest run -p kiln-formula --test conformance_registry"
    );
}
