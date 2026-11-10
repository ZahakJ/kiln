# ADR 0006: Shells and threading

- **Status:** accepted (desktop shell completed 2026-10-29; Android app 2026-10-30)
- **Date:** 2026-10-15

## Decision: Tauri 2 for desktop and Android, WebAssembly for the web

`apps/desktop/src-tauri` (crate `kiln-desktop`, executable `kiln`, identifier `io.github.zahakj.kiln`) is
the shell for Linux, Windows, macOS and, through Tauri 2 mobile, Android. The web/PWA build runs the same
core compiled to WebAssembly. Electron and Capacitor are dropped: with documents laid out in Rust
(ADR 0003) the WebView only runs the chrome and the canvas painter, so per-OS WebView differences no longer
affect documents, and Tauri gives a much smaller download and memory footprint (budgets B1, B2 and B13).

## Threading, native

- One **session thread** owns `kiln-session` and every document: a single writer, no locks on models,
  commands processed in order from a `flume` channel.
- A **rayon pool** (CPU count − 1, at least 2) does the data-parallel work the session thread hands out:
  formula recalculation by independent strongly connected components in topological levels, line breaking
  for pages not yet visible, part-level OOXML parsing, image decoding, glyph rasterisation batches and
  fidelity scoring.
- A **tokio runtime** (multi-threaded, 2 workers) does IO: autosave journal writes, file watching,
  collaboration sockets, update checks.
- The Tauri main thread runs only the WebView and the IPC handlers, which forward to the session thread
  and await replies. Long operations report progress events.

## Threading, web

The whole core runs in one dedicated Web Worker, so parsing a 20 MB workbook never blocks the chrome.
`rayon` work is sequential unless the `threads` build is active. Long operations are chunked
(`Session::step()` with an 8 ms budget) so frame requests interleave and the surface keeps painting.

## Painter and memory

The painter runs on the WebView's main thread (decode `SceneDelta`, update tile canvases, composite in
`requestAnimationFrame`; target 4 ms per frame at 1440 × 900 @2×). `kiln-session` evicts layout far from
the viewport, drops unused glyph atlas pages after 30 s, caps the undo log at 1,000 transactions or 64 MB
and streams file writes.

## The desktop shell

- **One core, many windows.** The `kiln` process runs one session thread for every window: Kiln
  Home, a window per document (`#/write?doc=3`), and Presenter View's audience window
  (`HostOp::OpenWindow`, `#/present?doc=3&view=audience`). Each webview subscribes to the
  session's events under its own label, so both windows of a slide show follow `Event::Show`
  without the relay the web needs between tabs.
- **The host is native.** `DesktopHost` answers every `HostOp` in the Rust process: files through
  `std::fs` (atomic writes), the Open and Save As dialogs of the window in front with filters for
  its app in the interface language (`tauri-plugin-dialog`), the clipboard with its text and HTML
  flavours (`arboard`), links through the system's handler for `http`, `https` and `mailto` only,
  local files only of document, picture and media types, print as a temporary PDF opened in the
  system's viewer, and second windows. `StoredHost` wraps it with the app storage in the app data
  directory (preferences, recent files, autosave journals, versions, file watching), and
  `NetHost` adds the sharing sockets and timers. No host request reaches a webview, so two
  windows never answer the same request.
- **Menus from the catalog.** `pnpm --filter @kiln/commands desktop:menu` writes
  `apps/desktop/src-tauri/menu.json` from the command catalog, the keymaps and the locales: the
  menus of Home, Write, Grid and Deck with each command's title in English, German and Arabic and
  the shortcut the editors bind to it as the accelerator. The shell gives each window the menu
  bar of the app its page reports (`kiln_window_context`) in the interface language; on macOS the
  menu bar follows the window in front and starts with the application menu. A picked command
  runs in that window's frame (`kiln://menu`), the same path as its key; in a text field of the
  chrome the editing items act on the field. The GTK menu bar runs right to left in Arabic and
  leaves F10 to Key Tips.
- **Files from outside.** The bundles register `.docx`, `.dotx`, `.docm`, `.odt`, `.kwrite`,
  `.xlsx`, `.xltx`, `.xlsm`, `.csv`, `.ods`, `.kgrid`, `.pptx`, `.potx`, `.pptm`, `.odp` and
  `.kdeck`, with a document icon per app on Linux. `kiln <file>…`, Open With, files dropped on a
  window and a second launch (single instance: the running Kiln opens them) all open each file
  in a window of its own, or in a Kiln Home window that has nothing else open.
- **Closing.** A document window's close button asks its page to close the document first (the
  usual question about unsaved changes), then the window closes; closing the document with
  Ctrl+W closes its window unless it is the last one, which goes back to Kiln Home.
- **Window places** are kept per kind of window (`main`, `document`, `show`) by
  `tauri-plugin-window-state`; the audience's window opens full screen on the other display
  when there is one.
- **Updates.** `tauri-plugin-updater` is configured against GitHub Releases and registered only
  when `plugins.updater.pubkey` holds a key (docs/RELEASING.md).
- **Startup probe.** `kiln --startup-probe` prints the time from the start of the process to
  Kiln Home's first paint and the memory of the process and the WebKit processes it started.
- **Frontend.** The executable embeds `apps/desktop/dist`: the web build without the
  WebAssembly cores and source maps, which only the browser uses.

## The Android app

- **The same crate.** `kiln-desktop` builds as `libkiln_desktop.so` for Android (application
  id `io.github.zahakj.kiln`, Android 8.0 / API 26 and later, target API 36) in the Gradle
  project `gen/android`, which is kept in the repository: `pnpm tauri android init` only adds
  files that are missing, so the project is edited in place. The desktop's windows, menus,
  clipboard crate, single instance and updater stay out of the Android build; what only
  Android does is in `src/android` and two Kotlin files, `MainActivity.kt` and
  `KilnPlugin.kt` (an app plugin the Rust side calls, `kiln-android`).
- **One window.** Kiln Home and each document share the window; a document opens on its
  app's route in it, as in a browser tab. A route the page asks for in a window of its own
  (`kiln_open_route`) is shown in that window.
- **The host is native, as on the desktop.** `AndroidHost` wraps `StoredHost` (app storage
  in the app's data folder) and answers what Android does differently. Documents behind
  content URIs are read and written through `tauri-plugin-fs`, which opens them with
  Android's content resolver; Kiln names them with a handle, the URI followed by `#/` and the
  document's name, so the core reads the name and the format from the path as for a file.
  Their stamp (size and date) comes from the provider; providers do not report changes, so
  the stamp before each save stands in for file watching. Open and Save As are the storage
  access framework's dialogs (`ACTION_OPEN_DOCUMENT`, `ACTION_CREATE_DOCUMENT`) with lasting
  access, so Recent files reopen documents after a restart. The clipboard is Android's, with
  HTML. Print hands the PDF the core writes (`HostOp::OpenFile` on a file of the print
  folder) to `PrintManager`, named after the document; the temporary folder is the app's
  cache (`TMPDIR`, set by the activity), which a `FileProvider` shares for printing and
  sending. Links open for `http`, `https` and `mailto` only. Presenter View's second window
  is answered with a message.
- **Files from other apps.** The manifest takes the sixteen document types the desktop
  registers for VIEW and EDIT (by MIME type, and by extension where a file manager gives
  none) and SEND. The Tauri CLI's own file-association filters are switched off for Android
  (`bundle.fileAssociations: null` in `tauri.android.conf.json`). A document handed over
  for reading only opens without its place, so Save asks where.
- **The window.** Edge to edge: the system bars' and the keyboard's insets pad the page
  (`windowSoftInputMode` is `adjustResize` as well), and the bars take the colour of the
  page's title bar, its `theme-color` from the tokens, reported with the window's context;
  the theme's colours start the same (`values/colors.xml`, checked against the tokens).
- **Back** goes to the page (the app plugin's `back-button` event; the WebView's history is
  not used): it closes the topmost sheet, dialog, menu or Backstage with Escape, then the
  document (`suite.file.close`), and on Kiln Home puts Kiln in the background
  (`kiln_leave`). A hardware keyboard's shortcuts go to the page first.
- **Signing.** The release APK is signed with the key the environment names
  (`KILN_KEYSTORE`, docs/RELEASING.md); only with `KILN_KEYSTORE` unset does Gradle use its
  debug key, and a tagged release stops without the release key.

