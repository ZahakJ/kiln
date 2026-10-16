# kiln-session

The facade over the whole core and the only API the shells use (PLAN.md §3.3, `docs/adr/0005-bridge.md`).

- `Session`: open documents (`new_document`, `open`), `execute` with validation against each command's `CommandMeta` (app and availability), `query`, `set_viewport`, `frame` (an empty `SceneDelta` with the revision until WP-54), `a11y`, `poll` for events and `host_reply` for deferred host requests. Open and Save/Save As run as host-request continuations, so they work over both synchronous and asynchronous hosts.
- `Host`: the outside world. `NativeHost` (desktop) reads and writes files with atomic replace and defers dialogs, clipboard and preferences to the UI; `DeferredHost` (web) defers everything.
- `SessionHandle` (native only): the session on its own thread behind a flume channel with one-shot replies, typed async methods for the Tauri IPC handlers, blocking `call`, non-blocking event subscriptions per document or for all (`subscribe`, `subscribe_as` to replace an owner's earlier stream; events nobody received yet, such as an early host request, go to the next subscriber); a panicking job fails alone, and a document it was changing is marked damaged (read-only, Save asks for a new location).
- `undo::UndoStack`: bounded history (1,000 steps, 64 MB) with coalescing and saved-state ids.
- `apps/`: the registry of document models (`DocModel`, `AppFactory`). `apps/scratch` is a plain-paragraph document (grapheme-aware editing, word motions, selections, coalesced undo, UTF-8/UTF-16 text files) that proves the loop until the Write engine exists. App work packages add one line each to `apps/mod.rs`.
