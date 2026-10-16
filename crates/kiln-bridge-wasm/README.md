# kiln-bridge-wasm

The WebAssembly bridge (PLAN.md §3.5): `wasm-bindgen` exports of `kiln-session` for the web build, run inside a Web Worker (`apps/web/src/worker.ts`).

`KilnSession` exports `open`, `newDocument`, `execute`, `query`, `setViewport`, `frame`, `a11y`, `poll` and `hostReply`. Small payloads are JSON strings of the `kiln-protocol` types; frames are postcard bytes (`Uint8Array`). Errors are thrown as `Error`s whose message is `ErrorInfo` JSON. `console_error_panic_hook` is installed at start. The logic lives in the plain-Rust `Bridge` type so it is unit-tested natively.

Build with `cargo xtask wasm` (dev) or `cargo xtask wasm --release` (`wasm-release` profile, `wasm-opt -Os`, prints `wasm gzip: <n> KB` and enforces budget B12 < 4,500 KB). Output goes to `apps/web/src/wasm/` (gitignored). The `threads` feature exports `initThreadPool` from `wasm-bindgen-rayon`; it needs a nightly toolchain with `+atomics,+bulk-memory` and is off by default.
