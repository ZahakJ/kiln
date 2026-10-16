// SPDX-License-Identifier: AGPL-3.0-or-later
//! Machine calibration: a fixed, CPU-bound, single-threaded workload whose time on this machine,
//! divided by its time on the reference machine (`calibration_reference_ms` in
//! `bench/budgets.toml`), is the machine factor that scales the budgets (PLAN.md §3.9).
//!
//! The workload mixes integer hashing, floating-point arithmetic, branches and a small table that
//! stays in cache, so it tracks general single-core speed rather than one unit; it touches no
//! memory beyond 64 KiB, so it does not measure the memory system.

use std::hint::black_box;
use std::time::Instant;

/// Rounds of the workload (≈ 100 ms on the reference machine in a release build).
pub const ROUNDS: u32 = 3_000_000;

fn workload(rounds: u32) -> u64 {
    let mut table = [0u64; 8192];
    let mut h: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut f = 1.0f64;
    for i in 0..rounds {
        h ^= u64::from(i);
        h = h.wrapping_mul(0xff51_afd7_ed55_8ccd).rotate_left(29);
        let slot = (h as usize) & (table.len() - 1);
        table[slot] = table[slot].wrapping_add(h);
        f = if h & 1 == 0 {
            (f * 1.000_000_1).sqrt() + 0.5
        } else {
            f.mul_add(0.999_999_9, 0.25)
        };
        for _ in 0..8 {
            h = h.wrapping_add(table[(h >> 7) as usize & (table.len() - 1)]) ^ (h >> 13);
        }
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fbits = f.to_bits();
    h ^ fbits ^ table.iter().fold(0, |a, b| a ^ b)
}

/// Runs the workload `runs` times and returns the fastest time in milliseconds.
#[must_use]
pub fn calibrate_with(rounds: u32, runs: usize) -> f64 {
    let mut best = f64::INFINITY;
    for _ in 0..runs.max(1) {
        let t = Instant::now();
        black_box(workload(black_box(rounds)));
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
    }
    best
}

/// The calibration time of this machine (best of five), in milliseconds.
#[must_use]
pub fn calibrate() -> f64 {
    calibrate_with(ROUNDS, 5)
}

/// The machine factor: this machine's calibration time over the reference's, never below 1 (a
/// faster machine keeps the budgets as written) and capped at `max` (a much slower machine must
/// not hide a regression).
#[must_use]
pub fn machine_factor(measured_ms: f64, reference_ms: f64, max: f64) -> f64 {
    if !(measured_ms.is_finite() && reference_ms.is_finite()) || reference_ms <= 0.0 {
        return 1.0;
    }
    (measured_ms / reference_ms).clamp(1.0, max.max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_workload_is_deterministic() {
        assert_eq!(workload(1000), workload(1000));
        assert_ne!(workload(1000), workload(1001));
        assert!(calibrate_with(10_000, 2) > 0.0);
    }

    #[test]
    fn the_factor_is_clamped() {
        assert_eq!(machine_factor(100.0, 200.0, 3.0), 1.0);
        assert!((machine_factor(300.0, 200.0, 3.0) - 1.5).abs() < 1e-12);
        assert_eq!(machine_factor(2000.0, 200.0, 3.0), 3.0);
        assert_eq!(machine_factor(f64::NAN, 200.0, 3.0), 1.0);
        assert_eq!(machine_factor(100.0, 0.0, 3.0), 1.0);
    }
}
