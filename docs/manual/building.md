# Building from source

Kiln is a Rust workspace (the core, the command-line tool and the desktop shell) and a pnpm
workspace (the interface, shared by the desktop app and the web app). This page builds the
desktop app on Linux and the Android app, and runs their checks; [Releasing](../RELEASING.md)
covers the release workflow that builds every platform.

## What you need

- Rust 1.99.1 (`rustup`; `rust-toolchain.toml` selects it) and `cargo-nextest`.
- Node 24 and pnpm, the version named in `package.json` (`corepack enable`).
- The WebKitGTK and GTK development libraries, OpenSSL and the AppImage tools. On Debian and
  Ubuntu:

  ```sh
  sudo apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
    librsvg2-dev libxdo-dev libssl-dev patchelf file
  ```

  On Arch Linux: `pacman -S --needed webkit2gtk-4.1 gtk3 libayatana-appindicator librsvg xdotool openssl patchelf file`.
- For the bundle checks, `dpkg` (`dpkg-deb`); for the desktop smoke test, `xvfb`, python-xlib
  and WebKitWebDriver (`xvfb python3-xlib webkitgtk-webdriver` on Ubuntu 26.04, with
  `webkit2gtk-driver` in place of the last on older Debian and Ubuntu releases; `xorg-server-xvfb
  python-xlib webkitgtk-6.0` on Arch Linux), and `cargo install tauri-driver --version 2.1.0 --locked`.

## The desktop app

```sh
pnpm install --frozen-lockfile
pnpm --filter @kiln/desktop tauri dev              # a development window on the Vite server
pnpm --filter @kiln/desktop tauri build --bundles appimage,deb -- --profile dist
```

`tauri build` first writes the native menus (`pnpm --filter @kiln/commands desktop:menu`, which
turns the command catalog, the keymaps and the translations into
`apps/desktop/src-tauri/menu.json`), builds the web interface, and copies it without the
browser's WebAssembly cores to `apps/desktop/dist`, which the executable embeds. The `dist`
profile links with full link-time optimisation; the bundles land in `target/dist/bundle/`:

| Bundle | Path |
|---|---|
| AppImage | `target/dist/bundle/appimage/Kiln_X.Y.Z_amd64.AppImage` |
| Debian package | `target/dist/bundle/deb/Kiln_X.Y.Z_amd64.deb` |
| Executable | `target/dist/kiln` |

Run `kiln --help` for the command line: `kiln report.docx budget.xlsx` opens each file in a
window of its own, in the Kiln that is already running if there is one. `--new-instance` runs a
Kiln of its own, and `KILN_DATA_DIR=<folder>` keeps its settings, recent files, autosave journals
and window places in that folder instead of the user's.

## Checks

```sh
cargo nextest run -p kiln-desktop                  # the shell's unit tests
bash tools/verify-bundles.sh                       # the AppImage and the .deb
xvfb-run -a target/dist/kiln --startup-probe       # start-up time and memory
xvfb-run -a pnpm --filter @kiln/e2e tauri-smoke    # the desktop smoke test
```

- `tools/verify-bundles.sh` reads the package with `dpkg-deb -I`, unpacks both bundles, runs the
  binary inside the AppImage, checks that the desktop file opens files (`kiln %F`) and registers
  all sixteen document types, that the `.deb` installs the MIME database entry for `.kwrite`,
  `.kgrid` and `.kdeck` and an icon for every document type, and that the bundles are within the
  size budgets (AppImage under 130 MB, `.deb` under 30 MB).
- `--startup-probe` starts Kiln, waits for Kiln Home's first paint, and prints the time since the
  process started (`startup-ms`, budget B1: under 700 ms) and, after a moment at rest, the memory
  of Kiln and the WebKit processes it started: `pss-mb`, which counts shared memory once (budget
  B2: under 300 MB), and `rss-mb`. The last two lines say whether each budget is met
  (`b1=met`, `b2=over`).
- The smoke test drives the built app through `tauri-driver`: it opens a `.docx` from the command
  line, types, saves with Ctrl+S and checks the file on disk; then it starts Presenter View on a
  presentation and checks that the audience's window follows the presenter and closes with the
  show. `KILN_BIN` picks the binary (default: the newest of `target/dist`, `target/release` and
  `target/debug`); screenshots go to `e2e/screenshots/tauri/`.

## Android

The Android app is the same crate (`kiln-desktop`, built as `libkiln_desktop.so`) in the Gradle
project `apps/desktop/src-tauri/gen/android`, which is kept in the repository.

What it needs, all in user space:

- Java 21 for Gradle (`JAVA_HOME`);
- the Android SDK (`ANDROID_HOME`) with platform 36 and build-tools 36.0.0, and an NDK, r27 or
  later (`NDK_HOME`);
- the Rust targets `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`
  and `i686-linux-android`.

`bash tools/setup-android.sh` installs whatever of these is missing (`--check` only reports, and
`--persist` writes the variables to `~/.profile`), and prints the variables to export. Then:

```sh
pnpm tauri android build --apk --target aarch64 --split-per-abi   # arm64-v8a
bash tools/verify-apk.sh                                           # the newest APK it built
```

The APK is
`apps/desktop/src-tauri/gen/android/app/build/outputs/apk/arm64/release/app-arm64-release.apk`;
`--target armv7` and `--target x86_64` build the other two (`arm`, `x86_64`). Gradle runs with
2 GB of heap, two workers and no daemon (`gen/android/gradle.properties`), so it fits next to
the Rust build on a small machine.

The release APK is signed with the key named by the environment. With `KILN_KEYSTORE` unset it
is signed with the debug key Gradle makes in `~/.android` (it installs, but cannot update an
install signed with the release key); with `KILN_KEYSTORE` naming a file that is not there, or
without `KILN_KEYSTORE_PASSWORD`, the build stops:

| Variable | |
|---|---|
| `KILN_KEYSTORE` | the keystore file (`~/.kiln-keys/release.jks`) |
| `KILN_KEYSTORE_PASSWORD` | its password |
| `KILN_KEY_ALIAS` | the key's alias, `kiln` when not set |
| `KILN_KEY_PASSWORD` | the key's password, the keystore's when not set |

[Releasing](../RELEASING.md#signing) makes the key.

`tools/verify-apk.sh [--release] [apk]` checks an APK without a device (`--release` fails an
APK signed with the debug key): with `aapt2 dump badging` the
package (`io.github.zahakj.kiln`), the label, the SDK levels, the version and the native code;
with `aapt2 dump xmltree` that VIEW, EDIT and SEND take every document type the desktop
registers, by MIME type and by extension, `adjustResize` and the FileProvider; with `apksigner`
the signature and whose key it is; with `unzip -l` the library, the web interface inside it
and the adaptive launcher icon; and the size, under 35 MB (B13).

`pnpm tauri android init` regenerates only the files missing from `gen/android`; the generated
project's build outputs, `tauri.properties`, the Gradle files the CLI writes and the copied
libraries are ignored by Git.

## The rest of the workspace

```sh
cargo xtask verify            # format, Clippy, tests, the protocol and the other Rust checks
pnpm verify                   # lint, types, unit tests and licences of the interface
cargo xtask wasm --release    # the WebAssembly cores of the web app
pnpm --filter @kiln/web build # the web app
pnpm e2e                      # the Playwright suite against the web app
```

## Icons

The document icons the Linux packages install (a page with the app's tile) are made from the
brand kit by `bash apps/desktop/src-tauri/icons/filetypes/build.sh` (needs `rsvg-convert`).
