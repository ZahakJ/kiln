// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recalculation benchmark for the performance budgets B8 and B9 (PLAN.md §3.9).
//!
//! - **B8** (`full_recalc_ms`): a workbook of 1,000,008 formula cells — arithmetic, IF/AND/OR,
//!   row SUMs, a sliding 10-row SUM window and an INDEX lookup into a 1,000-row table standing in
//!   for VLOOKUP — fully recalculated on a 4-thread pool. Median of 3 runs.
//! - **B9** (`incremental_ms_p95`): a 100,008-formula workbook of the same shape; 200 single-cell
//!   edits of an input, each followed by a recalculation; the 95th percentile of edit + recalc.
//!
//! Run with `cargo run --release -p kiln-formula --example recalc_bench`.

use std::sync::Arc;
use std::time::Instant;

use kiln_formula::engine::number_arg;
use kiln_formula::engine::{Area, Arg, CallContext, Engine, EngineOptions, EvalValue, Reference};
use kiln_formula::registry::{FunctionRegistry, FunctionSpec, FunctionTable};
use kiln_formula::values::{ErrorValue, Value};

/// `INDEX(range, row, [col])` returning a reference, as the function library will.
fn index(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> EvalValue {
    let base = ctx.force(&mut args[0]);
    let r = number_arg(args.get_mut(1), ctx, Some(1.0));
    let c = number_arg(args.get_mut(2), ctx, Some(1.0));
    let (Ok(r), Ok(c)) = (r, c) else {
        return EvalValue::from(ErrorValue::Value);
    };
    let (r, c) = (r.trunc() as i64, c.trunc() as i64);
    let EvalValue::Ref(re) = base else {
        let v = ctx.to_value(base);
        return match v {
            Value::Array(a) if r >= 1 && c >= 1 => {
                EvalValue::Value(a.get(r as usize - 1, c as usize - 1).clone())
            }
            _ => EvalValue::from(ErrorValue::Ref),
        };
    };
    let Some(a) = re.single().copied() else {
        return EvalValue::from(ErrorValue::Ref);
    };
    let (h, w) = a.dims();
    if r < 1 || c < 1 || r as usize > h || c as usize > w {
        return EvalValue::from(ErrorValue::Ref);
    }
    EvalValue::Ref(Reference::area(Area::cell(
        a.sheet,
        a.bounds.r0 + (r - 1) as u32,
        a.bounds.c0 + (c - 1) as u32,
    )))
}

fn registry() -> Arc<dyn FunctionRegistry> {
    let mut t = FunctionTable::with_core();
    t.register(FunctionSpec::new("INDEX", 2, 3, index));
    Arc::new(t)
}

/// Formula columns B..J for row `r` (1-based).
fn row_formulas(r: u32) -> [String; 9] {
    let window = if r >= 10 {
        format!("=SUM(G{}:G{r})", r - 9)
    } else {
        format!("=SUM(G1:G{r})")
    };
    [
        format!("=A{r}*2+1"),
        format!("=IF(B{r}>1000,B{r}-1000,B{r}+0.5)"),
        format!("=SUM(A{r}:C{r})"),
        format!("=INDEX(Data!$B$1:$B$1000,A{r}+1)"),
        format!("=E{r}*C{r}-D{r}"),
        format!("=IF(AND(F{r}>0,E{r}<500),F{r},-F{r})"),
        window,
        format!("=H{r}/2+D{r}"),
        format!("=IF(OR(I{r}<0,G{r}>100),1,0)*I{r}"),
    ]
}

fn build(rows: u32) -> Engine {
    let mut e = Engine::with_options(EngineOptions {
        registry: Some(registry()),
        clock: Some(Arc::new(|| 45306.5)),
        ..EngineOptions::default()
    });
    e.add_sheet("Model").expect("sheet");
    e.add_sheet("Data").expect("sheet");
    for i in 0..1000u32 {
        e.set_value("Data", i, 0, f64::from(i)).expect("data");
        e.set_value("Data", i, 1, f64::from((i * 7919) % 1000) / 4.0)
            .expect("data");
    }
    for r in 1..=rows {
        e.set_value("Model", r - 1, 0, f64::from((r * 37) % 1000))
            .expect("input");
        for (i, f) in row_formulas(r).iter().enumerate() {
            e.set_formula("Model", r - 1, 1 + i as u32, f)
                .expect("formula");
        }
    }
    e
}

fn rss_mb() -> f64 {
    std::fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("VmRSS:"))
                .and_then(|l| l.split_whitespace().nth(1)?.parse::<f64>().ok())
        })
        .map_or(0.0, |kb| kb / 1024.0)
}

fn main() {
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(
            std::env::var("THREADS")
                .ok()
                .and_then(|t| t.parse().ok())
                .unwrap_or(4),
        )
        .stack_size(8 << 20)
        .build()
        .expect("thread pool");

    // B8: full recalculation of 1M formulas.
    let t = Instant::now();
    let mut big = build(111_112);
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    let formulas = big.formula_count();
    let stats = big.graph_stats();
    let first = pool.install(|| {
        let t = Instant::now();
        big.recalc();
        t.elapsed().as_secs_f64() * 1000.0
    });
    let mut runs: Vec<f64> = (0..3)
        .map(|_| {
            pool.install(|| {
                let t = Instant::now();
                let r = big.recalc_all();
                assert_eq!(r.evaluated, formulas);
                t.elapsed().as_secs_f64() * 1000.0
            })
        })
        .collect();
    runs.sort_by(f64::total_cmp);
    let full_ms = runs[1];
    let check = big.value("Model", 50_000, 9).expect("value");
    assert!(
        matches!(check, Value::Number(_)),
        "unexpected result {check:?}"
    );
    let rss_big = rss_mb();
    // For information: single edits in the million-formula workbook.
    let mut big_samples: Vec<f64> = (0..50u32)
        .map(|i| {
            pool.install(|| {
                let t = Instant::now();
                big.set_value("Model", (i * 2203) % 111_112, 0, f64::from(i % 997))
                    .expect("edit");
                big.recalc();
                t.elapsed().as_secs_f64() * 1000.0
            })
        })
        .collect();
    big_samples.sort_by(f64::total_cmp);
    let big_p95 = big_samples[47];
    drop(big);

    // B9: single-edit incremental recalculation in a 100k-formula workbook.
    let mut small = build(11_112);
    pool.install(|| small.recalc());
    let mut samples = Vec::with_capacity(200);
    let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
    let mut evaluated = 0usize;
    for _ in 0..200 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let row = (seed % 11_112) as u32;
        let value = (seed % 997) as f64;
        let ms = pool.install(|| {
            let t = Instant::now();
            small.set_value("Model", row, 0, value).expect("edit");
            let r = small.recalc();
            evaluated += r.evaluated;
            t.elapsed().as_secs_f64() * 1000.0
        });
        samples.push(ms);
    }
    samples.sort_by(f64::total_cmp);
    let p50 = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95).div_ceil(100) - 1];

    println!(
        "formulas={formulas} range_nodes={} compiled={}",
        stats.range_nodes, stats.compiled
    );
    println!("build_ms={build_ms:.0} first_recalc_ms={first:.1} rss_mb={rss_big:.0}");
    println!("full_recalc_ms={full_ms:.1}");
    println!("incremental_1m_ms_p95={big_p95:.3}");
    println!(
        "incremental_formulas={} avg_evaluated={:.1} incremental_ms_p50={p50:.3}",
        small.formula_count(),
        evaluated as f64 / 200.0
    );
    println!("incremental_ms_p95={p95:.3}");
    let ok = full_ms < 2000.0 && p95 < 2.0;
    println!(
        "budgets B8<2000ms B9<2ms: {}",
        if ok { "PASS" } else { "FAIL" }
    );
    if !ok {
        std::process::exit(1);
    }
}
