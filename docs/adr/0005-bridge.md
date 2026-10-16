# ADR 0005: The bridge — one typed protocol, two transports

- **Status:** accepted
- **Date:** 2026-10-15
- **Work packages:** WP-50 (decision, condensed from PLAN.md §3.5), WP-52 (implementation)

## Context

The core is Rust; the chrome is TypeScript/React and must stay thin (PLAN.md §3.7). The same core
runs natively inside the Tauri shell (desktop, Android) and as WebAssembly in a browser. Both need
one contract that cannot drift between the two languages, cheap transport for bulk data (frames,
file bytes), and a way for the core to use the outside world (files, dialogs, clipboard) without
depending on any shell.

## Decision

### One protocol crate, generated TypeScript

Every type crossing between Rust and TypeScript lives in **`kiln-protocol`** with `serde` and
`ts_rs::TS` derives: `Command` (an enum of per-app sub-enums `Suite | Write | Grid | Deck | Notes`,
each in its own file `commands/<app>.rs` with its `CommandMeta` table, so app work packages never
edit the same file), `CommandMeta`/`CommandInfo`, `Event`, `ExecResult`, `Query`/`QueryResult`,
`Viewport`, `Selection`, `StatusBar`, `DocSummary`, `A11yTree`, `HostRequest`/`HostReply`,
`ErrorInfo`, and the Scene types (a placeholder `SceneDelta` until WP-54 re-exports `kiln-scene`).

`cargo xtask protocol` writes, from those types:

| Output | What |
|---|---|
| `packages/protocol/src/generated/*.ts` | one ts-rs declaration per type, `index.ts`, `PROTOCOL_VERSION` |
| `packages/protocol/src/generated/catalog.json` | every `CommandMeta` (id, label key, Key Tip, group, tier, apps, availability) |
| `packages/protocol/src/generated/schema.json` | the serde data model of every type, traced with `serde-reflection` |
| `crates/kiln-protocol/tests/vectors/<Type>.bin` + `.json` | postcard test vectors with their JSON form |

`cargo xtask protocol --check` (CI, `cargo xtask verify`) fails on any missing, changed or stale
file. 64-bit integers are exported as `number`: ids and revisions stay below 2^53 by construction
and the decoder rejects larger values.

Command tables are written with the `command_table!` macro, which generates both the static table
and an exhaustive `meta()` `match`, so a command without metadata does not compile. Ids are
`<app>.<group>.<name>` (`suite.file.save`); label keys are `cmd.<id>`.

### Encodings

Small messages (commands, results, queries, events, viewports) are JSON in serde's default
(externally tagged) shape: `{"Suite":"Undo"}`, `{"Write":{"InsertText":{"text":"a"}}}`. Frames and
other bulk payloads are **postcard**. `packages/protocol/src/postcard.ts` decodes postcard by
walking `schema.json`, producing exactly the value serde_json would produce, so the generated types
describe both transports; Vitest decodes every Rust-produced vector and compares it with its JSON.
Postcard was chosen over bincode/CBOR for its compact varints, stable 1.x wire format and
`no_std`-friendly encoder; the schema-driven decoder avoids hand-written TypeScript decoders that
would drift.

### The session facade

`kiln-session` is the only API the shells use. A `Session` keeps the open documents; `execute`
validates every command against its `CommandMeta` (the document's app is in `apps`, and
`availability` holds: `Document`, `Editable`, `CanUndo`, `CanRedo`, `Selection`) before
dispatching it to the document's model (`DocModel`, registered per app in `apps/mod.rs`). Each
model keeps an undo history (`UndoStack`: 1,000 steps / 64 MB, typing coalesced per word, state
ids so undoing back to the saved state is clean). Events queue until `poll()`.

The core reaches the outside world only through `trait Host`: each `HostOp` (read/write file,
pick open/save, recents, store/load preference, clipboard read/write, open URL) is answered either
at once (the desktop's `NativeHost` reads and writes files) or later: the session then emits
`Event::HostRequest` and resumes when the shell calls `host_reply`. Multi-step operations (Open:
dialog → read → parse; Save As: dialog → serialise → write → mark saved) are continuations keyed by
request id, so the same core code runs over a synchronous native host and an asynchronous browser.

### Transports

- **Tauri (desktop and Android).** `SessionHandle` runs the session on its own thread (flume
  channel, one-shot replies). A panicking job fails alone and is reported as an `Error` event; if
  it was changing a document, that document may be half-edited, so it is marked *damaged*:
  read-only, no further edits or undo, and Save asks for a new location rather than overwriting
  the original (Save As, queries and rendering still work, so the user can keep a copy).
  Subscriptions (`subscribe(doc)`, `subscribe_as(owner, doc)`) never block the caller; each
  follows one document or all, plus the session-wide events. An event no subscriber received
  (for example a host request raised before the webview subscribed) is kept, host requests
  always and up to 256 others, and handed to the next matching subscriber, so no host request is
  left pending forever.
  `kiln-desktop` registers a fixed set of commands — `kiln_open` (file bytes as the raw request
  body, the `OpenHint` as a percent-encoded `kiln-hint` header), `kiln_new`, `kiln_execute`,
  `kiln_query`, `kiln_viewport`, `kiln_frame` (raw postcard bytes as `ipc::Response`), `kiln_a11y`,
  `kiln_subscribe(doc, on_event)` (the events of `doc`, or of all documents for `null`, over an
  ordered `ipc::Channel`; keyed by webview label and `doc`, so a reloaded webview replaces its old
  stream instead of adding one), `kiln_host_reply` and
  `kiln_startup_probe`. The app manifest in `build.rs` generates one permission per command and
  `capabilities/*.json` grant exactly those plus the fs, dialog, opener, process and (desktop)
  window-state plugins. The CSP forbids remote scripts.
- **WebAssembly (web/PWA).** `kiln-bridge-wasm` exports `KilnSession` (`open`, `newDocument`,
  `execute`, `query`, `setViewport`, `frame`, `a11y`, `poll`, `hostReply`; JSON strings in and out,
  frames as `Uint8Array`, errors thrown as `Error`s whose message is `ErrorInfo` JSON).
  Deviation from the work package text: `poll()` returns one JSON string holding the event array,
  not `Uint8Array[]`. Events are small messages, the rule above sends small messages as JSON, and
  one string crosses the wasm boundary in one copy where an array of buffers would need one per
  event plus a postcard decode on the TypeScript side. Frames never travel in polled events: the
  session does not emit `Event::Frame`, and the Worker calls `frame()`, which returns postcard
  bytes. If a later work package streams frames as events, `poll` must switch to binary.
  `apps/web/src/worker.ts` owns it in a dedicated Worker; `WasmWorkerClient` speaks to the Worker
  with `postMessage`, transferring file and frame buffers. The default build is single-threaded;
  the `threads` feature (`wasm-bindgen-rayon`, needs nightly `+atomics`) is wired but off.
  `cargo xtask wasm --release` builds, binds, optimises (`wasm-opt -Os`) and asserts budget B12
  (gzip < 4.5 MB; 131 KB at WP-52).
- **One client interface.** Both implement `KilnClient` (`ping`, `execute`, `query`, `open`, `new`,
  `setViewport`, `frame`, `a11y`, `subscribe`, `hostReply`, `dispose`); the page picks
  `TauriClient` when it runs in the Tauri shell (`__TAURI_INTERNALS__`), otherwise boots the
  Worker. Host requests are answered on the main thread: `WebHost` (File System Access API, an
  `<input type=file>` fallback, the Origin Private File System for saving, `localStorage` prefs,
  async clipboard) or `TauriHost` (dialog and opener plugins; files never leave Rust).
- **Control channel.** `kiln-cli control` will speak newline-delimited JSON over stdio with the same
  `Command` enum (WP-95).

## Consequences

- Adding a command means one variant, one table row and `cargo xtask protocol`; the TypeScript
  types, catalog, schema and vectors follow, and CI catches anything stale.
- The UI never computes document state: it renders `DocSummary`, `Selection` and `StatusBar` from
  events and queries.
- Bulk data never goes through JSON; the decoder costs one schema walk per frame.

## Why ts-rs

`specta`/`tauri-specta` 2 has been a release candidate since 2024; `ts-rs` 12 is stable, and typed
command wrappers are trivial once the `Command` enum is generated. ts-rs gives declarations only, so
the postcard schema comes from `serde-reflection` 0.6 (MIT/Apache-2.0), which traces the exact serde
data model that postcard encodes.
