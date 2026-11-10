// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln` — the Kiln desktop application (Tauri 2).
// Do not open a console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

fn main() {
    kiln_desktop::run();
}
