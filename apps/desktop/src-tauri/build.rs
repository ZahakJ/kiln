// SPDX-License-Identifier: AGPL-3.0-or-later
//! Generates the Tauri context and, from the app manifest, one `allow-<command>` permission per
//! `kiln_*` IPC command, so `capabilities/*.json` can grant exactly these commands (PLAN.md §3.5).
fn main() {
    let manifest = tauri_build::AppManifest::new().commands(&[
        "kiln_open",
        "kiln_new",
        "kiln_execute",
        "kiln_query",
        "kiln_viewport",
        "kiln_frame",
        "kiln_a11y",
        "kiln_subscribe",
        "kiln_host_reply",
        "kiln_startup_probe",
    ]);
    if let Err(e) = tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest)) {
        // A build script reports failure by panicking; the message is what Cargo shows.
        panic!("tauri-build failed: {e:#}");
    }
}
