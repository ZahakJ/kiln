// SPDX-License-Identifier: AGPL-3.0-or-later
//! The function library from the command line.
//!
//! ```text
//! cargo run -p kiln-formula --example functions -- --count
//! cargo run -p kiln-formula --example functions -- --list [FAMILY]
//! cargo run -p kiln-formula --example functions -- --doc XLOOKUP
//! ```

use std::process::ExitCode;

use kiln_formula::functions::{self, DocFamily, FunctionDoc, doc, docs};
use kiln_formula::registry::MAX_ARGS;

const USAGE: &str = "usage: functions --count | --list [FAMILY] | --doc NAME";

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
        _ => {
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}
