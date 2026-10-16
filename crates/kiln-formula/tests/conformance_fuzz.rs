// SPDX-License-Identifier: AGPL-3.0-or-later
//! No library function panics or runs away on hostile input: every registered function is called
//! with every arity it accepts (up to four extra optional arguments) and with arguments drawn from
//! a pool of edge cases — extreme and non-finite-producing numbers, empty and huge text, errors,
//! blanks, logicals, arrays of mixed types, ranges, whole columns and lambdas. Each call must
//! return a value (any value, errors included) without panicking.

mod conformance_support;

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::{Duration, Instant};

use kiln_formula::functions;
use kiln_formula::registry::MAX_ARGS;

const POOL: &[&str] = &[
    "0",
    "-1",
    "1",
    "2.5",
    "-0.5",
    "1E+308",
    "-1E+308",
    "1E-308",
    "4294967296",
    "1E+15",
    "2958466",
    "45306.5",
    "\"\"",
    "\"abc\"",
    "\"1\"",
    "\"-3.5E+2\"",
    "\"2024-02-29\"",
    "\"1+2i\"",
    "\">5\"",
    "\"a*\"",
    "\"[\"",
    "REPT(\"x\",32767)",
    "TRUE",
    "FALSE",
    "#N/A",
    "#DIV/0!",
    "{1,2;3,4}",
    "{1,\"a\";TRUE,#N/A}",
    "{0}",
    "{-1,0,1}",
    "Data!A1:A10",
    "Data!B1:B10",
    "Data!R1:T5",
    "Data!G1:G4",
    "Data!H1:L7",
    "Data!X1:X5",
    "Data!A:A",
    "(Data!A1:A2,Data!C1:C2)",
    "LAMBDA(x,x)",
    "LAMBDA(a,b,a+b)",
    "",
];

/// A small deterministic generator (SplitMix64), so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() % xs.len() as u64) as usize]
    }
}

#[test]
fn no_function_panics_on_hostile_arguments() {
    let mut engine = conformance_support::make_engine();
    let calc = engine.sheet_id("Calc").expect("Calc");
    // KILN_FUZZ_ROUNDS and KILN_FUZZ_SEED widen the search locally (CI runs the default).
    let rounds: usize = std::env::var("KILN_FUZZ_ROUNDS")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(6);
    let seed: u64 = std::env::var("KILN_FUZZ_SEED")
        .ok()
        .and_then(|r| r.parse().ok())
        .unwrap_or(0x4B49_4C4E);
    let mut rng = Rng(seed);
    let (mut calls, mut panics, mut slow) = (0usize, Vec::new(), Vec::new());
    // KILN_FUZZ_LOG=<file> writes each formula before it is evaluated (to find a hang).
    let mut log = std::env::var_os("KILN_FUZZ_LOG").and_then(|p| std::fs::File::create(p).ok());
    for spec in functions::all_specs() {
        let max = if spec.max_args == MAX_ARGS {
            usize::from(spec.min_args) + 4
        } else {
            usize::from(spec.max_args).min(usize::from(spec.min_args) + 4)
        };
        for arity in usize::from(spec.min_args)..=max {
            for _ in 0..rounds {
                let args: Vec<&str> = (0..arity).map(|_| rng.pick(POOL)).collect();
                let formula = format!("={}({})", spec.name, args.join(","));
                calls += 1;
                if let Some(log) = &mut log {
                    use std::io::Write as _;
                    let _ = writeln!(log, "{formula}");
                    let _ = log.flush();
                }
                let start = Instant::now();
                let result = catch_unwind(AssertUnwindSafe(|| {
                    engine.evaluate_at(&formula, calc, 0, 0, false)
                }));
                let took = start.elapsed();
                if result.is_err() {
                    panics.push(formula.clone());
                    // The engine may be left mid-evaluation: start over with a fresh one.
                    engine = conformance_support::make_engine();
                }
                // Unbounded work shows up as minutes; the largest arrays a function may build
                // (16,777,216 cells, as in v1.0) take a few seconds in an unoptimised test build.
                if took > Duration::from_secs(20) {
                    slow.push(format!("{formula} took {took:?}"));
                }
            }
        }
    }
    println!(
        "{calls} hostile calls, {} panics, {} slow",
        panics.len(),
        slow.len()
    );
    assert!(panics.is_empty(), "panicked:\n{}", panics.join("\n"));
    assert!(slow.is_empty(), "slow:\n{}", slow.join("\n"));
}
