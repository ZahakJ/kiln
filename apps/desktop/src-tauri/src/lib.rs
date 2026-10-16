// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Kiln shell for Linux, Windows, macOS and Android (Tauri 2).
//!
//! It opens one window on the web build (`apps/web`), runs the core's session on its own thread
//! (`kiln_session::SessionHandle` with the native host) and exposes it to the webview through the
//! `kiln_*` IPC commands of PLAN.md §3.5 ([`bridge`]); events stream over an IPC channel and frames
//! travel as raw postcard bytes. `--startup-probe` measures the time from process start to the first
//! finished page load, prints `startup-ms=<n>` (and `rss-kb=<n>` where `/proc` exists) and exits;
//! CI uses it for budgets B1 and B2 (PLAN.md §3.9). WP-88 completes the desktop host, menus and
//! file associations.
#![forbid(unsafe_code)]

pub mod bridge;

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use kiln_session::{NativeHost, SessionHandle};
use tauri::webview::PageLoadEvent;

/// Command-line options understood by the shell.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Measure startup, print it and exit.
    pub startup_probe: bool,
}

impl Options {
    /// Parses the process arguments (without the program name). Unknown arguments are ignored so
    /// that file paths passed by the desktop environment do not abort startup.
    pub fn parse<I: IntoIterator<Item = String>>(args: I) -> Self {
        let mut options = Self::default();
        for arg in args {
            if arg == "--startup-probe" {
                options.startup_probe = true;
            }
        }
        options
    }
}

/// The probe report line, e.g. `startup-ms=412`.
#[must_use]
pub fn probe_line(elapsed_ms: u128) -> String {
    format!("startup-ms={elapsed_ms}")
}

/// Resident set size of this process in KiB (Linux and Android only).
#[must_use]
pub fn rss_kib() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    parse_vm_rss(&status)
}

fn parse_vm_rss(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|l| l.strip_prefix("VmRSS:"))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|n| n.parse().ok())
}

/// Builds and runs the application.
///
/// # Panics
///
/// Panics when the Tauri runtime cannot start (no display, missing WebView); there is no
/// meaningful recovery for a GUI shell in that state.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let started = Instant::now();
    let options = Options::parse(std::env::args().skip(1));
    let reported = AtomicBool::new(false);

    let (session, _session_thread) = match SessionHandle::spawn(Box::new(NativeHost)) {
        Ok(started_session) => started_session,
        Err(e) => {
            eprintln!("kiln: {e}");
            std::process::exit(1);
        }
    };

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init());
    #[cfg(desktop)]
    let builder = builder.plugin(tauri_plugin_window_state::Builder::default().build());

    builder
        .manage(bridge::Kiln { session, started })
        .invoke_handler(tauri::generate_handler![
            bridge::kiln_open,
            bridge::kiln_new,
            bridge::kiln_execute,
            bridge::kiln_query,
            bridge::kiln_viewport,
            bridge::kiln_frame,
            bridge::kiln_a11y,
            bridge::kiln_subscribe,
            bridge::kiln_host_reply,
            bridge::kiln_startup_probe,
        ])
        .on_page_load(move |webview, payload| {
            if options.startup_probe
                && payload.event() == PageLoadEvent::Finished
                && !reported.swap(true, Ordering::SeqCst)
            {
                println!("{}", probe_line(started.elapsed().as_millis()));
                if let Some(kib) = rss_kib() {
                    println!("rss-kb={kib}");
                }
                tauri::Manager::app_handle(webview).exit(0);
            }
        })
        .run(tauri::generate_context!())
        .expect("failed to start the Kiln window");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_probe_flag_and_ignores_files() {
        let opts = Options::parse(["report.docx".to_owned(), "--startup-probe".to_owned()]);
        assert!(opts.startup_probe);
        assert!(!Options::parse(Vec::<String>::new()).startup_probe);
    }

    #[test]
    fn formats_the_probe_line_and_reads_rss() {
        assert_eq!(probe_line(412), "startup-ms=412");
        assert_eq!(
            parse_vm_rss("Name:\tkiln\nVmRSS:\t  123456 kB\n"),
            Some(123_456)
        );
        assert_eq!(parse_vm_rss("Name:\tkiln\n"), None);
    }
}
