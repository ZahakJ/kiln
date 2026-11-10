// SPDX-License-Identifier: AGPL-3.0-or-later
//! `--startup-probe` (budgets B1 and B2): the time from the start of the process to
//! the first paint of Kiln Home (the page reports it with `kiln_home_painted`), and the memory of
//! Kiln at idle with Home open: the proportional set of the `kiln` process and of the WebKit
//! processes it started, summed (`pss-mb`, shared pages counted once: budget B2 as amended
//! 2026-10-30, < 300 MB), and their resident sets (`rss-mb`). Printed as `key=value` lines with a
//! line per budget (`b1=met|over`, `b2=met|over`); the process then exits.

use std::time::{Duration, Instant};

/// How long Kiln Home stays open before the memory is read (idle).
pub const IDLE: Duration = Duration::from_millis(1500);

/// How long the probe waits for Kiln Home before it gives up.
pub const TIMEOUT: Duration = Duration::from_secs(60);

/// Milliseconds since the process started: from the kernel's start time of the process where
/// `/proc` has it (so loading the executable and its libraries counts), else since `started`.
#[must_use]
pub fn elapsed_ms(started: Instant) -> u64 {
    process_age_ms()
        .unwrap_or_else(|| u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX))
}

/// Linux: `/proc/uptime` minus the process's start time (`/proc/self/stat` field 22, in clock
/// ticks of 1/100 s, which is `USER_HZ` on every Linux architecture Kiln ships for).
fn process_age_ms() -> Option<u64> {
    let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
    let uptime = std::fs::read_to_string("/proc/uptime").ok()?;
    let start_ticks = stat_field(&stat, 22)?.parse::<u64>().ok()?;
    let up_s: f64 = uptime.split_whitespace().next()?.parse().ok()?;
    // Whole milliseconds; uptime is at most centuries, well inside f64's exact range.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let up_ms = (up_s * 1000.0).round() as u64;
    up_ms.checked_sub(start_ticks * 10)
}

/// Field `n` (1-based, as in proc(5)) of a `/proc/<pid>/stat` line; the command name in field 2
/// may contain spaces and parentheses, so fields are counted after its closing parenthesis.
#[must_use]
pub fn stat_field(stat: &str, n: usize) -> Option<&str> {
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(n.checked_sub(3)?)
}

/// A `kB` line of `/proc/<pid>/status` or `smaps_rollup` (`VmRSS:`, `Pss:`).
#[must_use]
pub fn kb_line(text: &str, key: &str) -> Option<u64> {
    text.lines()
        .find_map(|l| l.strip_prefix(key))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

/// This process and every process it started (WebKit's web and network processes), by pid.
#[must_use]
pub fn process_tree() -> Vec<u32> {
    let me = std::process::id();
    let Ok(dir) = std::fs::read_dir("/proc") else {
        return vec![me];
    };
    let parents: Vec<(u32, u32)> = dir
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str()?.parse::<u32>().ok())
        .filter_map(|pid| {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
            Some((pid, stat_field(&stat, 4)?.parse().ok()?))
        })
        .collect();
    let mut tree = vec![me];
    let mut i = 0;
    while i < tree.len() {
        let parent = tree[i];
        tree.extend(
            parents
                .iter()
                .filter(|(_, pp)| *pp == parent)
                .map(|(p, _)| *p),
        );
        i += 1;
    }
    tree
}

/// Memory of a set of processes in KiB: (resident set, proportional set).
#[must_use]
pub fn memory_kib(pids: &[u32]) -> (u64, u64) {
    let mut rss = 0;
    let mut pss = 0;
    for pid in pids {
        if let Ok(s) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
            rss += kb_line(&s, "VmRSS:").unwrap_or(0);
        }
        if let Ok(s) = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")) {
            pss += kb_line(&s, "Pss:").unwrap_or(0);
        }
    }
    (rss, pss)
}

/// B1: milliseconds to Kiln Home's first paint.
pub const B1_BUDGET_MS: u64 = 700;

/// B2: MB of memory at rest with Home open, PSS summed over every process (amended 2026-10-30).
pub const B2_BUDGET_PSS_MB: u64 = 300;

/// The probe's report: the measures, then each budget and whether it is met.
#[must_use]
pub fn report(startup_ms: u64, pids: usize, rss_kib: u64, pss_kib: u64) -> String {
    let mb = |k: u64| k.div_ceil(1024);
    let verdict = |ok: bool| if ok { "met" } else { "over" };
    format!(
        "startup-ms={startup_ms}\npss-mb={pss}\nrss-mb={rss}\nrss-kb={rss_kib}\nprocesses={pids}\n\
         b1={} (startup-ms < {B1_BUDGET_MS})\nb2={} (pss-mb < {B2_BUDGET_PSS_MB})",
        verdict(startup_ms < B1_BUDGET_MS),
        verdict(mb(pss_kib) < B2_BUDGET_PSS_MB),
        pss = mb(pss_kib),
        rss = mb(rss_kib),
    )
}

/// Resident set size of this process alone in KiB (Linux and Android).
#[must_use]
pub fn own_rss_kib() -> Option<u64> {
    kb_line(
        &std::fs::read_to_string("/proc/self/status").ok()?,
        "VmRSS:",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_proc_files() {
        let stat = "1234 (kiln (x) y) S 1 1234 1234 0 -1 4194560 0 0 0 0 0 0 0 0 20 0 9 0 5550 0";
        assert_eq!(stat_field(stat, 3), Some("S"));
        assert_eq!(stat_field(stat, 4), Some("1"));
        assert_eq!(stat_field(stat, 22), Some("5550"));
        assert_eq!(stat_field(stat, 2), None);
        assert_eq!(
            kb_line("Name:\tkiln\nVmRSS:\t  123456 kB\n", "VmRSS:"),
            Some(123_456)
        );
        assert_eq!(kb_line("Rss: 10 kB\nPss: 7 kB\n", "Pss:"), Some(7));
        assert_eq!(kb_line("Name:\tkiln\n", "VmRSS:"), None);
    }

    #[test]
    fn measures_this_process() {
        let started = Instant::now();
        let ms = elapsed_ms(started);
        assert!(ms < 3_600_000, "{ms}");
        let tree = process_tree();
        assert_eq!(tree.first(), Some(&std::process::id()));
        let (rss, pss) = memory_kib(&tree);
        if cfg!(target_os = "linux") {
            assert!(rss > 0 && pss > 0 && pss <= rss + 1024, "{rss} {pss}");
            assert!(own_rss_kib().is_some());
        }
        assert_eq!(
            report(412, 3, 102_400, 81_000),
            "startup-ms=412\npss-mb=80\nrss-mb=100\nrss-kb=102400\nprocesses=3\n\
             b1=met (startup-ms < 700)\nb2=met (pss-mb < 300)"
        );
        let over = report(860, 3, 700_000, 400_000);
        assert!(
            over.contains("b1=over") && over.contains("b2=over"),
            "{over}"
        );
    }
}
