// SPDX-License-Identifier: AGPL-3.0-or-later
//! Recalculation budgets B8 and B9 (PLAN.md §3.9) through `kiln-formula`'s public API.
//!
//! The workbook has one input column (`A`) and ten formula columns per row that mix arithmetic,
//! `IF`, `AND`, row `SUM`s and a sliding ten-row `SUM` window, so recalculation exercises the
//! dependency graph's single-cell edges, range nodes and topological levels:
//!
//! ```text
//! B =A*2+1            C =IF(B>100,B-100,B+100)   D =SUM(A:C)        E =AND(B>C,D>0)
//! F =IF(E,D,-D)       G =SUM(F[r-9]:F[r])        H =B+C*D-G         I =IF(H>0,H/2,0-H)
//! J =G+I              K =SUM(B:J)
//! ```
//!
//! - `recalc-full` (B8): 100,000 rows = 1,000,000 formula cells, `recalc_all` on a 4-thread pool.
//! - `recalc-incremental` (B9): 10,000 rows = 100,000 formula cells; each run edits one input
//!   and recalculates (the edit is part of the timing, as a user would see it).
//!
//! The budget scenario names a VLOOKUP chain; lookups join the workbook when the function library
//! (WP-57) registers them.

use std::time::Instant;

use kiln_formula::{Engine, Value};

use super::RunOptions;
use crate::harness::{Config, Sample, measure};
use crate::synth::Lcg;

/// Formula cells per row.
const PER_ROW: u32 = 10;

fn formulas(r: u32) -> [String; PER_ROW as usize] {
    let r0 = r.saturating_sub(9).max(1);
    [
        format!("=A{r}*2+1"),
        format!("=IF(B{r}>100,B{r}-100,B{r}+100)"),
        format!("=SUM(A{r}:C{r})"),
        format!("=AND(B{r}>C{r},D{r}>0)"),
        format!("=IF(E{r},D{r},-D{r})"),
        format!("=SUM(F{r0}:F{r})"),
        format!("=B{r}+C{r}*D{r}-G{r}"),
        format!("=IF(H{r}>0,H{r}/2,0-H{r})"),
        format!("=G{r}+I{r}"),
        format!("=SUM(B{r}:J{r})"),
    ]
}

/// Builds the workbook with `rows` rows (`rows × 10` formulas).
///
/// # Errors
///
/// When the engine rejects a cell (it does not for this input).
pub fn workbook(rows: u32) -> Result<Engine, String> {
    let mut engine = Engine::new();
    engine.add_sheet("Data").map_err(|e| e.to_string())?;
    let mut rng = Lcg::new(0xbeef);
    for r in 1..=rows {
        #[allow(clippy::cast_precision_loss)]
        let v = rng.below(1000) as f64;
        engine
            .set_value("Data", r - 1, 0, v)
            .map_err(|e| e.to_string())?;
        for (i, f) in formulas(r).iter().enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let col = 1 + i as u32;
            engine
                .set_formula("Data", r - 1, col, f)
                .map_err(|e| format!("{f}: {e}"))?;
        }
    }
    Ok(engine)
}

fn pool() -> Result<rayon::ThreadPool, String> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(4)
        .build()
        .map_err(|e| e.to_string())
}

fn check(engine: &Engine, rows: u32) -> Result<(), String> {
    // Row 1, input a: B = 2a+1, K = SUM(B:J) must be a finite number.
    let k = engine
        .value("Data", rows - 1, PER_ROW)
        .map_err(|e| e.to_string())?;
    match k {
        Value::Number(n) if n.is_finite() => Ok(()),
        other => Err(format!("K{rows} = {other:?}, expected a number")),
    }
}

/// B8.
pub fn full(options: &RunOptions) -> Result<Sample, String> {
    let (rows, config) = if options.quick {
        (500, Config::new(0, 2))
    } else {
        (100_000, Config::new(1, 20))
    };
    let pool = pool()?;
    let t = Instant::now();
    let mut engine = workbook(rows)?;
    let build_ms = t.elapsed().as_secs_f64() * 1000.0;
    let first = pool.install(|| engine.recalc_all());
    if first.evaluated < (rows * PER_ROW) as usize {
        return Err(format!(
            "only {} of {} formulas evaluated",
            first.evaluated,
            rows * PER_ROW
        ));
    }
    check(&engine, rows)?;
    let mut sample = measure("recalc-full", config, || {
        let r = pool.install(|| engine.recalc_all());
        if r.evaluated == 0 {
            return Err("nothing was recalculated".into());
        }
        Ok(())
    })?;
    sample.extra.insert("build_ms".into(), build_ms);
    sample
        .extra
        .insert("formulas".into(), f64::from(rows * PER_ROW));
    sample.extra.insert("threads".into(), 4.0);
    Ok(sample)
}

/// B9.
pub fn incremental(options: &RunOptions) -> Result<Sample, String> {
    let (rows, config) = if options.quick {
        (200, Config::new(1, 5))
    } else {
        (10_000, Config::new(20, 200))
    };
    let pool = pool()?;
    let mut engine = workbook(rows)?;
    pool.install(|| engine.recalc_all());
    let mut rng = Lcg::new(0x1d17);
    let mut evaluated = 0usize;
    let mut sample = measure("recalc-incremental", config, || {
        #[allow(clippy::cast_possible_truncation)]
        let row = rng.below(rows as usize) as u32;
        #[allow(clippy::cast_precision_loss)]
        let v = rng.below(1000) as f64 + 0.5;
        engine
            .set_value("Data", row, 0, v)
            .map_err(|e| e.to_string())?;
        let r = pool.install(|| engine.recalc());
        if r.evaluated == 0 {
            return Err("the edit recalculated nothing".into());
        }
        evaluated += r.evaluated;
        Ok(())
    })?;
    check(&engine, rows)?;
    #[allow(clippy::cast_precision_loss)]
    sample.extra.insert(
        "evaluated_per_edit".into(),
        evaluated as f64 / (config.warmup + config.iterations) as f64,
    );
    sample
        .extra
        .insert("formulas".into(), f64::from(rows * PER_ROW));
    Ok(sample)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workbook_computes_expected_values() {
        let mut e = workbook(12).unwrap();
        e.recalc_all();
        let a = match e.value("Data", 11, 0).unwrap() {
            Value::Number(n) => n,
            other => panic!("{other:?}"),
        };
        assert_eq!(
            e.value("Data", 11, 1).unwrap(),
            Value::Number(a * 2.0 + 1.0)
        );
        // E is a logical.
        assert!(matches!(e.value("Data", 11, 4).unwrap(), Value::Bool(_)));
        check(&e, 12).unwrap();
        // An edit recalculates the row and the window below it, not the whole sheet.
        e.set_value("Data", 0, 0, 7.0).unwrap();
        let r = e.recalc();
        assert!(r.evaluated >= 10 && r.evaluated < 120, "{}", r.evaluated);
    }
}
