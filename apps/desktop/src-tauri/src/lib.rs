// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Kiln shell for Linux, Windows, macOS and Android (Tauri 2).
//!
//! It shows the web build (`apps/web`) in its windows and runs the core's session on a thread of
//! its own (`kiln_session::SessionHandle`), reached through the `kiln_*` IPC commands
//! ([`bridge`]): events stream over an IPC channel, frames travel as raw postcard bytes. Every
//! window talks to the same core, so a document can be in two windows at once (Presenter View).
//!
//! - [`host`]: files, dialogs, clipboard, links, print and windows for the core; preferences,
//!   recent files, autosave journals and versions live in the app data directory (`StoredHost`).
//! - [`windows`]: a window per document, Kiln Home, Presenter View's audience window, files from
//!   the command line, the desktop environment, a second launch and drops.
//! - [`menu`]: the native menus from `menu.json` (catalog, keymaps, translations).
//! - [`probe`]: `--startup-probe` for budgets B1 and B2.
//! - `android`: the Android app (storage access framework, print, share, Back, system bars);
//!   compiled for Android, and its platform-free part for the tests.
//!
//! The updater is configured against GitHub Releases and stays off until a signing key exists
//! (`plugins.updater.pubkey` in `tauri.conf.json`, see docs/RELEASING.md).
#![forbid(unsafe_code)]

#[cfg(any(target_os = "android", test))]
pub mod android;
pub mod bridge;
pub mod cli;
pub mod files;
pub mod fonts;
#[cfg(desktop)]
pub mod host;
#[cfg(desktop)]
pub mod menu;
pub mod probe;
pub mod shell;
#[cfg(desktop)]
pub mod windows;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

#[cfg(desktop)]
use kiln_session::StoredHost;
use kiln_session::{Host, SessionHandle};
use tauri::Manager;

/// The environment variable that puts Kiln's app data (preferences, recent files, journals,
/// window places) in a directory of its own: tests and portable runs.
pub const DATA_DIR_ENV: &str = "KILN_DATA_DIR";

/// Whether the updater has a key to check releases with (`plugins.updater.pubkey`).
#[must_use]
pub fn updater_enabled(config: &tauri::Config) -> bool {
    config
        .plugins
        .0
        .get("updater")
        .and_then(|u| u.get("pubkey"))
        .and_then(serde_json::Value::as_str)
        .is_some_and(|k| !k.trim().is_empty())
}

/// The session's host: the desktop host with the app storage of `data_dir` around it, or alone
/// when that directory cannot be used (the documents still open and save; nothing is kept).
#[cfg(desktop)]
fn session_host<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    data_dir: Option<PathBuf>,
) -> Box<dyn Host> {
    let inner: Box<dyn Host> = Box::new(host::DesktopHost::new(app.clone()));
    let Some(dir) = data_dir else { return inner };
    match StoredHost::open(inner, &dir) {
        Ok(stored) => Box::new(stored),
        Err(e) => {
            eprintln!("kiln: app storage in {}: {e}", dir.display());
            Box::new(host::DesktopHost::new(app.clone()))
        }
    }
}

/// The session's host on Android: content URIs, the storage access framework, print and the
/// clipboard around the app storage of `data_dir`.
#[cfg(target_os = "android")]
fn session_host<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    data_dir: Option<PathBuf>,
) -> Box<dyn Host> {
    android::session_host(app, data_dir)
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
    let cwd = std::env::current_dir().unwrap_or_default();
    let options = cli::Options::parse(std::env::args_os().skip(1), &cwd);
    if options.help {
        print!("{}", cli::USAGE);
        return;
    }
    if options.version {
        println!("Kiln {}", env!("CARGO_PKG_VERSION"));
        return;
    }

    let mut context = tauri::generate_context!();
    if let Some(dir) = std::env::var_os(DATA_DIR_ENV).filter(|d| !d.is_empty()) {
        context.config_mut().app.app_directories_override = Some(
            tauri::utils::config::AppDirectoriesOverride::Root(PathBuf::from(dir)),
        );
    }
    let updater = updater_enabled(context.config());
    // The interface's fonts come from the core's (`/fonts/…`, see `fonts`).
    let embedded = context.set_assets(Box::new(fonts::NoAssets));
    context.set_assets(Box::new(fonts::FontAssets::new(embedded)));

    let builder = tauri::Builder::default();
    // A second launch hands its files to the running Kiln (first plugin, as the plugin asks).
    #[cfg(desktop)]
    let builder = if options.new_instance || options.startup_probe {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, cwd| {
            windows::second_instance(app, argv, cwd);
        }))
    };
    let builder = builder
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init());
    #[cfg(desktop)]
    let builder = builder.plugin(
        tauri_plugin_window_state::Builder::default()
            .map_label(|label| match label.split_once('-') {
                Some(("show", _)) => "show",
                Some(_) => "document",
                None => label,
            })
            .build(),
    );
    #[cfg(desktop)]
    let builder = if updater {
        builder.plugin(tauri_plugin_updater::Builder::new().build())
    } else {
        builder
    };
    #[cfg(mobile)]
    let _ = updater;
    #[cfg(target_os = "android")]
    let builder = builder.plugin(android::plugin::init());

    let shell = Arc::new(shell::Shell::new());
    #[cfg(desktop)]
    let builder = builder.manage(Arc::new(host::Clipboard::default()));
    let probe = options.startup_probe;
    let files = options.files;
    let builder = builder.manage(Arc::clone(&shell)).setup(move |app| {
        let handle = app.handle().clone();
        let data_dir = app.path().app_data_dir().ok();
        let (session, _thread) = SessionHandle::spawn(session_host(&handle, data_dir))
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
        app.manage(bridge::Kiln {
            session,
            started,
            probe,
            painted: AtomicBool::new(false),
        });
        #[cfg(desktop)]
        {
            menu::init();
            app.on_menu_event(|app, event| menu::on_event(app, event.id().as_ref()));
            let title = shell.string("home");
            let main = windows::open(&handle, "#/home", &title)?;
            if !files.is_empty() {
                tauri::async_runtime::spawn(windows::open_paths(handle.clone(), files, Some(main)));
            }
            if probe {
                // A page that never paints still ends the probe, with a failure.
                std::thread::spawn(move || {
                    std::thread::sleep(probe::TIMEOUT);
                    println!("startup-ms=timeout");
                    handle.exit(1);
                });
            }
        }
        #[cfg(target_os = "android")]
        {
            let _ = (&shell, &files);
            android::setup(&handle)?;
        }
        Ok(())
    });
    #[cfg(desktop)]
    let builder = builder.on_window_event(windows::on_window_event);

    builder
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
            bridge::kiln_home_painted,
            bridge::kiln_window_context,
            bridge::kiln_close_window,
            bridge::kiln_open_paths,
            bridge::kiln_open_route,
            bridge::kiln_clipboard_text,
            bridge::kiln_share,
            bridge::kiln_leave,
        ])
        .build(context)
        .expect("failed to start the Kiln window")
        .run(|app, event| {
            // The last window closed: Kiln ends only when no document with unsaved changes is
            // left without a window (New or Open in a window, a window closed some other way).
            #[cfg(desktop)]
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = &event
            {
                api.prevent_exit();
                let app = app.clone();
                tauri::async_runtime::spawn(async move {
                    if windows::exit_guard(&app).await {
                        app.exit(0);
                    }
                });
            }
            #[cfg(mobile)]
            let _ = (app, event);
        });
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_updater_waits_for_a_signing_key() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("tauri.conf.json");
        let updater = &conf["plugins"]["updater"];
        assert_eq!(
            updater["pubkey"], "",
            "the release key is added with docs/RELEASING.md"
        );
        assert!(
            updater["endpoints"][0]
                .as_str()
                .is_some_and(|e| e.starts_with("https://github.com/ZahakJ/kiln/releases/"))
        );
        assert_eq!(conf["bundle"]["createUpdaterArtifacts"], false);
    }

    #[test]
    fn the_capabilities_grant_the_shell_commands() {
        let caps = include_str!("../capabilities/default.json");
        for cmd in [
            "allow-kiln-home-painted",
            "allow-kiln-window-context",
            "allow-kiln-close-window",
            "allow-kiln-open-paths",
            "allow-kiln-open-route",
            "allow-kiln-clipboard-text",
        ] {
            assert!(caps.contains(cmd), "{cmd}");
        }
    }

    #[test]
    fn android_grants_sharing_and_leaving_to_its_one_window() {
        let caps: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/mobile.json")).expect("mobile.json");
        assert_eq!(caps["platforms"], serde_json::json!(["android"]));
        assert_eq!(caps["windows"], serde_json::json!(["main"]));
        let granted: Vec<&str> = caps["permissions"]
            .as_array()
            .expect("permissions")
            .iter()
            .filter_map(serde_json::Value::as_str)
            .collect();
        assert_eq!(granted, ["allow-kiln-share", "allow-kiln-leave"]);
        let android: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.android.conf.json")).expect("conf");
        assert_eq!(android["identifier"], "io.github.zahakj.kiln");
        assert_eq!(android["bundle"]["android"]["minSdkVersion"], 26);
        // The manifest has its own intent filters (VIEW, EDIT, SEND); the CLI adds none.
        assert!(
            android["bundle"]
                .as_object()
                .is_some_and(|b| b.get("fileAssociations") == Some(&serde_json::Value::Null))
        );
    }
}
