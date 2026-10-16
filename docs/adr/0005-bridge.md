# ADR 0005: The bridge — one typed protocol, two transports

- **Status:** accepted
- **Date:** 2026-10-15
- **Work package:** WP-50 (condensed from PLAN.md §3.5)

## Decision

Every type crossing between Rust and TypeScript lives in **`kiln-protocol`** with `serde` and `ts_rs::TS`
derives. `cargo xtask protocol` writes the TypeScript definitions to `packages/protocol/src/generated/`
plus a `catalog.json` of every `CommandMeta`; `cargo xtask protocol --check` fails CI when they are stale.
Both hosts implement one TypeScript `KilnClient` interface.

## Core types

`Command` (an enum of per-app sub-enums `Suite | Write | Grid | Deck | Notes`, each in its own file with
its `CommandMeta` table so app work packages never edit the same file), `CommandMeta` (id, label key,
Key Tip, group, tier, apps, availability), `Event` (`Opened`, `Changed`, `Frame`, `Recovery`, `Collab`,
`Error`, `HostRequest`), `Viewport`, `Selection`, `SceneDelta` and `A11yTree`.

Small messages are JSON (`serde_json`); `SceneDelta`, atlas pages and file bytes use `postcard`, decoded in
TypeScript by a schema-driven decoder tested against vectors produced by Rust.

## Transports

- **Tauri (desktop and Android):** a fixed set of commands — `kiln_open`, `kiln_execute`, `kiln_viewport`,
  `kiln_frame` (binary `ipc::Response`), `kiln_a11y`, `kiln_subscribe` (events over an ordered
  `ipc::Channel`), `kiln_host_reply` and `kiln_startup_probe`. Capabilities grant only these commands and
  the needed plugins to the main window; the CSP forbids remote scripts.
- **WebAssembly (web/PWA):** `kiln-bridge-wasm` exports a `KilnSession` class through `wasm-bindgen`
  0.2.129; a dedicated Worker owns it and `WasmWorkerClient` speaks `KilnClient` over `postMessage` with
  transferable buffers. The default build is single-threaded; a `threads` build (`wasm-bindgen-rayon`) is
  selected at runtime when the page is cross-origin isolated.
- **Control channel:** `kiln-cli control` speaks newline-delimited JSON over stdio with the same `Command`
  enum, for agents, tests and conversions without a GUI.

## Why ts-rs

`specta`/`tauri-specta` 2 has been a release candidate since 2024; `ts-rs` 12 is stable, and typed command
wrappers are trivial once the `Command` enum is generated.

## Status at WP-50

`kiln-protocol` and `kiln-bridge-wasm` are scaffolds (the bridge exports only `version()`); WP-52
implements the protocol, the exporter, the session facade and both bridges.
