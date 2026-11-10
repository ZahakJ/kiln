# @kiln/desktop

The Kiln shell for Linux, Windows, macOS and Android, built with Tauri 2. The Rust crate
`kiln-desktop` lives in `src-tauri/` and produces the `kiln` executable; the user interface is the
web build in `apps/web`, embedded without its WebAssembly cores (`scripts/frontend.mjs` writes
`dist/`).

- `src/bridge.rs`: the `kiln_*` IPC commands and the shell's own
  (`kiln_window_context`, `kiln_close_window`, `kiln_home_painted`, `kiln_open_paths`,
  `kiln_clipboard_text`).
- `src/host.rs`: the native host (files, dialogs, clipboard, links, print, windows), wrapped by
  the app storage in the app data directory.
- `src/windows.rs`: Kiln Home, a window per document, Presenter View's audience window, files
  from the command line, a second launch and drops, closing with unsaved changes.
- `src/menu.rs`, `menu.json`: the native menus (`pnpm --filter @kiln/commands desktop:menu`).
- `src/files.rs`, `bundle/linux/`, `icons/filetypes/`: the file associations, the desktop file,
  the MIME database entry and the document icons.
- `src/probe.rs`: `--startup-probe` (budgets B1 and B2).

```sh
pnpm --filter @kiln/desktop tauri dev
pnpm --filter @kiln/desktop tauri build --bundles appimage,deb -- --profile dist
cargo nextest run -p kiln-desktop
```

`docs/manual/building.md` has the requirements, the bundle checks and the desktop smoke test.
