// SPDX-License-Identifier: AGPL-3.0-or-later
//! The timing harness: warm-up runs, timed iterations, percentiles and peak resident memory.
//!
//! Budgets use this harness rather than `criterion` (PLAN.md §4.2): a budget needs a few honest
//! numbers (p50, p95, peak RSS) from a bounded amount of work, in one process, on CI.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How many times a benchmark runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Config {
    /// Untimed runs first (caches, allocator, lazy initialisation).
    pub warmup: usize,
    /// Timed runs.
    pub iterations: usize,
}

impl Config {
    /// A configuration (at least one timed iteration).
    #[must_use]
    pub const fn new(warmup: usize, iterations: usize) -> Self {
        Self {
            warmup,
            iterations: if iterations == 0 { 1 } else { iterations },
        }
    }

    /// The same configuration with every count divided by `n` (at least one iteration): quick
    /// runs for tests.
    #[must_use]
    pub fn scaled_down(self, n: usize) -> Self {
        let n = n.max(1);
        Self::new(self.warmup / n, (self.iterations / n).max(1))
    }
}

/// Timings of one benchmark (serialised as JSON by `kiln-bench run --json`).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sample {
    /// Benchmark name.
    pub name: String,
    /// Timed iterations.
    pub iterations: usize,
    /// Median, in milliseconds.
    pub p50_ms: f64,
    /// 95th percentile (nearest rank), in milliseconds.
    pub p95_ms: f64,
    /// Mean, in milliseconds.
    pub mean_ms: f64,
    /// Fastest run, in milliseconds.
    pub min_ms: f64,
    /// Slowest run, in milliseconds.
    pub max_ms: f64,
    /// Peak resident set size during set-up and runs, in MiB (Linux only).
    pub peak_rss_mb: Option<f64>,
    /// Further figures a benchmark reports (`cold_ms`, `cells`, …).
    pub extra: BTreeMap<String, f64>,
    /// The share of all CPUs (0 to 1) that other processes kept busy while the benchmark ran
    /// (Linux only): above [`CONTENDED`], its timings say more about the machine than the code.
    #[serde(default)]
    pub contention: Option<f64>,
}

impl Sample {
    /// A figure by name: `p50`, `p95`, `mean`, `min`, `max`, `rss` or an `extra` key.
    #[must_use]
    pub fn metric(&self, name: &str) -> Option<f64> {
        match name {
            "p50" => Some(self.p50_ms),
            "p95" => Some(self.p95_ms),
            "mean" => Some(self.mean_ms),
            "min" => Some(self.min_ms),
            "max" => Some(self.max_ms),
            "rss" => self.peak_rss_mb,
            other => self.extra.get(other).copied(),
        }
    }
}

/// The nearest-rank percentile of sorted values (`p` in 0..=100).
#[must_use]
pub fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let rank = ((p.clamp(0.0, 100.0) / 100.0) * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// Summarises durations into a sample.
#[must_use]
pub fn summarize(name: &str, times: &[Duration], peak_rss_mb: Option<f64>) -> Sample {
    let mut ms: Vec<f64> = times.iter().map(|d| d.as_secs_f64() * 1000.0).collect();
    ms.sort_by(f64::total_cmp);
    #[allow(clippy::cast_precision_loss)]
    let mean = if ms.is_empty() {
        0.0
    } else {
        ms.iter().sum::<f64>() / ms.len() as f64
    };
    Sample {
        name: name.to_owned(),
        iterations: ms.len(),
        p50_ms: percentile(&ms, 50.0),
        p95_ms: percentile(&ms, 95.0),
        mean_ms: mean,
        min_ms: ms.first().copied().unwrap_or(0.0),
        max_ms: ms.last().copied().unwrap_or(0.0),
        peak_rss_mb,
        extra: BTreeMap::new(),
        contention: None,
    }
}

/// Resident set size of this process in KiB (`VmRSS` of `/proc/self/status`), on Linux.
#[must_use]
pub fn current_rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|v| v.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

/// Busy and total jiffies over all CPUs (the `cpu` line of `/proc/stat`), on Linux.
fn cpu_jiffies() -> Option<(u64, u64)> {
    let stat = std::fs::read_to_string("/proc/stat").ok()?;
    let fields: Vec<u64> = stat
        .lines()
        .next()?
        .strip_prefix("cpu ")?
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    // user nice system idle iowait irq softirq steal (guest time is already in user).
    let total: u64 = fields.iter().take(8).sum();
    let idle = fields.get(3)? + fields.get(4).copied().unwrap_or(0);
    Some((total.saturating_sub(idle), total))
}

/// The fraction of all CPUs that were busy during `window` (0 to 1; `None` where `/proc/stat` is
/// unavailable).
#[must_use]
pub fn busy_fraction(window: Duration) -> Option<f64> {
    let (busy0, total0) = cpu_jiffies()?;
    std::thread::sleep(window);
    let (busy1, total1) = cpu_jiffies()?;
    let total = total1.checked_sub(total0).filter(|&t| t > 0)?;
    #[allow(clippy::cast_precision_loss)]
    let fraction = busy1.saturating_sub(busy0) as f64 / total as f64;
    Some(fraction.clamp(0.0, 1.0))
}

/// The [`Sample::contention`] above which a benchmark ran on a busy machine: other processes
/// used more than a quarter of all CPUs (one of four here).
pub const CONTENDED: f64 = 0.25;

/// CPU time of this process (user + system, all threads) in clock ticks, from
/// `/proc/self/stat`, on Linux.
fn self_ticks() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    // Fields after the command name (which may contain spaces) start with the state (field 3);
    // utime and stime are fields 14 and 15.
    let rest = &stat[stat.rfind(')')? + 1..];
    let fields: Vec<&str> = rest.split_whitespace().collect();
    let utime: u64 = fields.get(11)?.parse().ok()?;
    let stime: u64 = fields.get(12)?.parse().ok()?;
    Some(utime + stime)
}

/// Measures how busy other processes keep the machine between [`ContentionMeter::start`] and
/// [`ContentionMeter::finish`]: all CPUs' busy ticks minus this process's own.
#[derive(Debug, Clone, Copy)]
pub struct ContentionMeter {
    start: Option<(u64, u64, u64)>,
}

impl ContentionMeter {
    /// Starts measuring.
    #[must_use]
    pub fn start() -> Self {
        Self {
            start: cpu_jiffies()
                .zip(self_ticks())
                .map(|((busy, total), own)| (busy, total, own)),
        }
    }

    /// The share of all CPUs (0 to 1) that other processes used since the start (`None` where
    /// `/proc` is unavailable or no time passed).
    #[must_use]
    pub fn finish(self) -> Option<f64> {
        let (busy0, total0, own0) = self.start?;
        let (busy1, total1) = cpu_jiffies()?;
        let own1 = self_ticks()?;
        let total = total1.checked_sub(total0).filter(|&t| t > 0)?;
        let others = busy1
            .saturating_sub(busy0)
            .saturating_sub(own1.saturating_sub(own0));
        #[allow(clippy::cast_precision_loss)]
        let share = others as f64 / total as f64;
        Some(share.clamp(0.0, 1.0))
    }
}

/// How [`wait_for_quiet`] ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quiet {
    /// The busy fraction of the last one-second window.
    pub busy: f64,
    /// Whether it fell to the limit (otherwise the wait timed out).
    pub quiet: bool,
    /// How long the wait took.
    pub waited: Duration,
}

/// Waits, for at most `max`, until fewer than `busy_limit` of all CPUs are busy over a one-second
/// window: other processes (a build, another agent's tests) make timings meaningless, so a
/// benchmark that missed a budget is measured again once the machine has calmed down. `None`
/// where the load cannot be read (then nothing is waited for).
#[must_use]
pub fn wait_for_quiet(max: Duration, busy_limit: f64) -> Option<Quiet> {
    let start = Instant::now();
    loop {
        let busy = busy_fraction(Duration::from_secs(1))?;
        let waited = start.elapsed();
        if busy <= busy_limit || waited >= max {
            return Some(Quiet {
                busy,
                quiet: busy <= busy_limit,
                waited,
            });
        }
    }
}

/// Samples the resident set size on a background thread until dropped, keeping the peak.
#[derive(Debug)]
pub struct RssSampler {
    stop: Arc<AtomicBool>,
    peak: Arc<AtomicU64>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl RssSampler {
    /// Starts sampling every `every` (when `/proc` is readable).
    #[must_use]
    pub fn start(every: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let peak = Arc::new(AtomicU64::new(current_rss_kib().unwrap_or(0)));
        let handle = current_rss_kib().is_some().then(|| {
            let (stop, peak) = (Arc::clone(&stop), Arc::clone(&peak));
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    if let Some(kib) = current_rss_kib() {
                        peak.fetch_max(kib, Ordering::Relaxed);
                    }
                    std::thread::sleep(every);
                }
            })
        });
        Self { stop, peak, handle }
    }

    /// Stops sampling and returns the peak in MiB (`None` where `/proc` is unavailable).
    #[must_use]
    pub fn finish(mut self) -> Option<f64> {
        self.halt();
        let kib = self.peak.load(Ordering::Relaxed);
        #[allow(clippy::cast_precision_loss)]
        let mb = kib as f64 / 1024.0;
        (kib > 0).then_some(mb)
    }

    fn halt(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        if let Some(kib) = current_rss_kib() {
            self.peak.fetch_max(kib, Ordering::Relaxed);
        }
    }
}

impl Drop for RssSampler {
    fn drop(&mut self) {
        self.halt();
    }
}

/// Runs `body` `config.warmup` times untimed, then `config.iterations` times timed.
///
/// # Errors
///
/// The first error `body` returns.
pub fn measure(
    name: &str,
    config: Config,
    mut body: impl FnMut() -> Result<(), String>,
) -> Result<Sample, String> {
    for _ in 0..config.warmup {
        body()?;
    }
    let mut times = Vec::with_capacity(config.iterations);
    for _ in 0..config.iterations {
        let t = Instant::now();
        body()?;
        times.push(t.elapsed());
    }
    Ok(summarize(name, &times, None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentiles_use_the_nearest_rank() {
        let v: Vec<f64> = (1..=20).map(f64::from).collect();
        assert_eq!(percentile(&v, 50.0), 10.0);
        assert_eq!(percentile(&v, 95.0), 19.0);
        assert_eq!(percentile(&v, 100.0), 20.0);
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&[7.0], 95.0), 7.0);
        assert_eq!(percentile(&[], 95.0), 0.0);
    }

    #[test]
    fn summarizes_timings() {
        let times: Vec<Duration> = [3, 1, 2]
            .iter()
            .map(|&n| Duration::from_millis(n))
            .collect();
        let s = summarize("x", &times, Some(10.0));
        assert_eq!(s.iterations, 3);
        assert!((s.p50_ms - 2.0).abs() < 1e-9);
        assert!((s.p95_ms - 3.0).abs() < 1e-9);
        assert!((s.mean_ms - 2.0).abs() < 1e-9);
        assert_eq!((s.min_ms, s.max_ms), (1.0, 3.0));
        assert_eq!(s.metric("rss"), Some(10.0));
        assert_eq!(s.metric("nope"), None);
    }

    #[test]
    fn measures_warmup_and_iterations() {
        let mut calls = 0;
        let s = measure("count", Config::new(2, 5), || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(calls, 7);
        assert_eq!(s.iterations, 5);
        let err = measure("fail", Config::new(0, 3), || Err("boom".into()));
        assert_eq!(err.unwrap_err(), "boom");
        assert_eq!(Config::new(4, 20).scaled_down(10), Config::new(0, 2));
        assert_eq!(Config::new(1, 0).iterations, 1);
    }

    #[test]
    fn measures_contention_without_counting_itself() {
        let meter = ContentionMeter::start();
        // Busy work of our own must not count as contention.
        let mut x = 0u64;
        let t = Instant::now();
        while t.elapsed() < Duration::from_millis(300) {
            x = std::hint::black_box(x.wrapping_mul(31).wrapping_add(7));
        }
        let share = meter.finish();
        if cfg!(target_os = "linux") {
            let share = share.unwrap();
            assert!((0.0..=1.0).contains(&share), "{share}");
            assert!(self_ticks().unwrap() > 0);
        }
    }

    #[test]
    fn reads_the_machine_load_on_linux() {
        let busy = busy_fraction(Duration::from_millis(50));
        let quiet = wait_for_quiet(Duration::ZERO, 1.0);
        if cfg!(target_os = "linux") {
            let busy = busy.unwrap();
            assert!((0.0..=1.0).contains(&busy), "{busy}");
            // A limit of 100 % is always met, after one window.
            let q = quiet.unwrap();
            assert!(q.quiet && q.waited >= Duration::from_millis(900), "{q:?}");
            // An impossible limit times out after the first window.
            let q = wait_for_quiet(Duration::ZERO, -1.0).unwrap();
            assert!(!q.quiet);
        }
    }

    #[test]
    fn samples_resident_memory_on_linux() {
        let sampler = RssSampler::start(Duration::from_millis(1));
        let big = vec![1u8; 32 << 20];
        std::thread::sleep(Duration::from_millis(20));
        let peak = sampler.finish();
        assert_eq!(big.iter().map(|&b| usize::from(b)).sum::<usize>(), 32 << 20);
        if cfg!(target_os = "linux") {
            assert!(peak.unwrap() >= 32.0, "{peak:?}");
            assert!(current_rss_kib().unwrap() > 0);
        }
    }
}
