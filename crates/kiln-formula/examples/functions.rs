// SPDX-License-Identifier: AGPL-3.0-or-later
//! The function library from the command line.
//!
//! ```text
//! cargo run -p kiln-formula --example functions -- --count
//! cargo run -p kiln-formula --example functions -- --list [FAMILY]
//! cargo run -p kiln-formula --example functions -- --doc XLOOKUP
//! cargo run -p kiln-formula --example functions -- --eval '=XLOOKUP(5,{1,3,5},{"a","b","c"})' …
//! ```
//!
//! `--eval` evaluates each formula on an empty sheet with the full library and prints
//! `formula<TAB>result`, the result written as a formula literal (`"c"`, `{1,2;3,4}`, `#N/A`).

use std::process::ExitCode;

use kiln_formula::Engine;
use kiln_formula::Value;
use kiln_formula::functions::{self, DocFamily, FunctionDoc, doc, docs};
use kiln_formula::registry::MAX_ARGS;

const USAGE: &str = "usage: functions --count | --list [FAMILY] | --doc NAME | --eval FORMULA…";

fn print_doc(d: &FunctionDoc) {
    println!("{}", d.syntax);
    println!();
    println!("{}", d.summary);
    println!();
    println!("Category: {} (since Excel {})", d.category_name, d.since);
    if let Some(spec) = functions::all_specs()
        .into_iter()
        .find(|s| s.name == d.name)
    {
        let max = if spec.max_args == MAX_ARGS {
            "255".to_owned()
        } else {
            spec.max_args.to_string()
        };
        let volatile = if spec.volatile { ", volatile" } else { "" };
        println!("Arguments: {} to {max}{volatile}", spec.min_args);
    }
    if !d.args.is_empty() {
        println!();
        let width = d.args.iter().map(|a| a.name.len()).max().unwrap_or(0);
        for a in &d.args {
            let mut tags = Vec::new();
            if a.optional {
                tags.push("optional");
            }
            if a.repeating {
                tags.push("repeating");
            }
            let tags = if tags.is_empty() {
                String::new()
            } else {
                format!(" ({})", tags.join(", "))
            };
            println!("  {:width$}  {}{tags}", a.name, a.description);
        }
    }
    if !d.examples.is_empty() {
        println!();
        println!("Examples:");
        for x in &d.examples {
            println!("  {}  →  {}", x.formula, x.result);
        }
    }
}

/// A value as a formula literal, numbers with every digit (shortest round-trip form).
fn literal(v: &Value) -> String {
    match v {
        Value::Blank => String::new(),
        Value::Number(n) => format!("{n}"),
        Value::Str(s) => format!("\"{}\"", s.replace('"', "\"\"")),
        Value::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_owned(),
        Value::Error(e) => e.to_string(),
        Value::Array(a) => {
            let rows: Vec<String> = (0..a.rows())
                .map(|r| {
                    (0..a.cols())
                        .map(|c| literal(a.get(r, c)))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect();
            format!("{{{}}}", rows.join(";"))
        }
        Value::Lambda(l) => format!("LAMBDA({})", l.params().join(",")),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--count") => {
            println!("{}", functions::function_names().len());
            ExitCode::SUCCESS
        }
        Some("--list") => {
            let family = args.get(1).map(|f| f.to_uppercase());
            if let Some(f) = &family
                && !DocFamily::ALL.iter().any(|x| x.file_stem() == f)
            {
                let names: Vec<&str> = DocFamily::ALL.iter().map(|x| x.file_stem()).collect();
                eprintln!("unknown family {f}; one of {}", names.join(", "));
                return ExitCode::FAILURE;
            }
            for d in docs() {
                if family.as_deref().is_none_or(|f| d.family.file_stem() == f) {
                    println!("{:<20} {:<20} {}", d.name, d.category_name, d.summary);
                }
            }
            ExitCode::SUCCESS
        }
        Some("--doc") => match args.get(1).and_then(|n| doc(n)) {
            Some(d) => {
                print_doc(d);
                ExitCode::SUCCESS
            }
            None => {
                eprintln!(
                    "no function named {}",
                    args.get(1).map_or("(missing)", String::as_str)
                );
                ExitCode::FAILURE
            }
        },
        Some("--eval") if args.len() > 1 => {
            let mut engine = Engine::with_options(functions::engine_options());
            let Ok(sheet) = engine.add_sheet("Sheet1") else {
                return ExitCode::FAILURE;
            };
            let mut ok = true;
            for f in &args[1..] {
                match engine.evaluate_at(f, sheet, 0, 0, false) {
                    Ok(v) => println!("{f}\t{}", literal(&v)),
                    Err(e) => {
                        ok = false;
                        println!("{f}\tparse error: {e}");
                    }
                }
            }
            if ok {
                ExitCode::SUCCESS
            } else {
                ExitCode::FAILURE
            }
        }
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
