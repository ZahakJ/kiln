# @kiln/desktop

The Kiln shell for Linux, Windows, macOS and Android, built with Tauri 2. The Rust crate
`kiln-desktop` lives in `src-tauri/` and produces the `kiln` executable; the user interface is the
web build in `apps/web`. Run `pnpm --filter @kiln/desktop tauri dev` for a development window
(needs the WebKitGTK libraries listed in `docs/qa/env.md` on Linux) and
`target/debug/kiln --startup-probe` to measure startup time and memory (budgets B1 and B2).
