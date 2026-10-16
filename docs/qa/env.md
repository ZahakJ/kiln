# Development environment (verified)

Facts verified by WP-01 and refreshed by WP-50 on 2026-10-15 on the development machine. `bash tools/verify-env.sh` re-checks
the required items and exits non-zero with a message for each missing one.

## Machine

| Item | Value |
|---|---|
| OS | Ubuntu 24.04.5 LTS, x86_64 |
| CPU / RAM | 4 CPUs, 7 GB RAM (limit test parallelism: `vitest` `maxWorkers: 2`, Playwright `workers: 2`) |
| Root | not available; everything below is installed in user space |
| Network | IPv4 works; **IPv6 is routed but broken** (`curl -6 https://dl.google.com` times out). Tools that only try IPv6 hang (see Android). |

## Toolchain

| Tool | Version | Location / how installed |
|---|---|---|
| Node.js | v24.22.0 | `~/.local/opt/node-v24.22.0-linux-x64`, symlinked as `~/.local/opt/node` by `tools/setup-node.sh` (SHA-256 verified against nodejs.org `SHASUMS256.txt`). The system Node 18.19.1 at `/usr/bin/node` is too old and must be shadowed: `export PATH="$HOME/.local/opt/node/bin:$PATH"` (also appended to `~/.bashrc` and `~/.profile`). |
| pnpm | 12.11.2 | `npm install -g pnpm@12.11.2` into the user-space Node; pinned by `packageManager` in `package.json`. |
| Playwright | 1.64.1, Chromium 1248 (Chrome 156) and headless shell 1248 | `~/.cache/ms-playwright`, installed by `pnpm --filter @kiln/e2e exec playwright install chromium` (run by `tools/setup-node.sh` once the workspace is installed). No extra system libraries were needed: the smoke e2e passes headless without `install-deps`. |
| Java | OpenJDK 21.0.12 | `/usr/lib/jvm/java-21-openjdk-amd64` (system package) |
| Android SDK | cmdline-tools 22.0, platform-tools, platforms;android-36, build-tools;36.0.0 | `~/Android`, by `tools/setup-android.sh` (idempotent; persists `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_SDK_ROOT` and `PATH` in `~/.bashrc` and `~/.profile`) |
| LibreOffice | 24.2.7.2 | `/usr/bin/soffice`, test oracle only (`tools/oracle-soffice.sh`) |
| Python | 3.12.3 | system |
| git / gh | 2.43.0 / 2.45.0 | system |
| Other | `pdftoppm`, ImageMagick `convert`, `ffmpeg`, `zip`/`unzip` | system |
| Fonts | Liberation, Carlito, Caladea | system-wide |

## Rust toolchain (WP-50)

Installed by `tools/setup-rust.sh` (PLAN.md §4.1), all in user space, every download with `curl -4`:

| Tool | Version | Location / how installed |
|---|---|---|
| rustup + Rust | 1.99.1 (profile minimal, rustfmt, clippy, rust-src) | `~/.rustup`, `~/.cargo/bin`; `PATH` line appended to `~/.bashrc` and `~/.profile`; `rust-toolchain.toml` pins the channel, components and targets |
| Rust targets | `wasm32-unknown-unknown`, `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android` | rustup |
| cargo-nextest | 0.9.148 | prebuilt binary from get.nexte.st |
| sccache | 0.18.0 | prebuilt musl binary from GitHub releases; `.cargo/config.toml` sets it as `rustc-wrapper` (the setup script removes that line if sccache is missing) |
| wasm-opt | binaryen version_133 | prebuilt release unpacked to `~/.local/opt/binaryen`, symlinked into `~/.cargo/bin` (the binary needs its `../lib`) |
| wasm-bindgen-cli | 0.2.129 | `cargo install --locked`; must equal the `wasm-bindgen` crate in `Cargo.lock` (verify-env checks) |
| cargo-deny | 0.20.2 | `cargo install --locked` |
| cargo-about | 0.9.2 | `cargo install --locked --features cli` (without the `cli` feature the crate installs no binary; the PLAN.md command omits it) |

`.cargo/config.toml` limits Cargo to 3 jobs; dependencies build at `opt-level = 2` once and our crates stay
fast to compile. Two agents share `target/`; Cargo's own lock serialises their builds.

## Tauri Linux libraries: not installed (WP-50)

The one root step of PLAN.md §4.1,
`pkexec apt-get install -y libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev patchelf file`,
was attempted twice on 2026-10-15 (120 s and 300 s waits). The polkit authentication prompt was not
answered in either window (`pkexec` exited on the `timeout`), so the packages are **not installed**;
`pkg-config` itself is also missing. Consequences:

- `cargo check -p kiln-desktop` fails locally in `gobject-sys`'s build script ("The pkg-config command
  could not be found"). Every other crate builds; `kiln-desktop` is outside the workspace's
  `default-members`, and `cargo xtask verify` excludes it.
- CI installs the libraries with `apt-get` and checks, lints and smoke-tests `kiln-desktop` (`ci.yml` jobs
  `rust` and `desktop-smoke`); the release workflow builds the desktop bundles.
- To build the shell locally, run the command above (or `sudo apt-get install …`) once; nothing else
  changes.

## Android NDK (WP-50)

`tools/setup-android.sh` now also installs `ndk;29.0.14206865` (NDK r29) into
`~/Android/ndk/29.0.14206865` and persists `NDK_HOME` in `~/.bashrc` and `~/.profile`. Together with the
Rust Android targets this is what `pnpm tauri android build` needs (WP-89).

## Python oracle environment

The fixture generator and the Python oracle use `~/.venvs/kiln` (`openpyxl` 3.1.5, `python-docx` 1.1.2,
`python-pptx` 1.0.2), created by WP-50 with the same package versions as the v1.0 environment.

## pnpm 12 behaviour worth knowing

- **Build scripts are opt-in.** `pnpm install` fails with `ERR_PNPM_IGNORED_BUILDS` when a new dependency has
  an install script. Approve it with `pnpm approve-builds <pkg>`, which records it under `allowBuilds:` in
  `pnpm-workspace.yaml` (currently only `esbuild`). Tauri's CLI ships as a prebuilt npm binary and needs no approval.
- **Minimum release age.** pnpm refuses versions published very recently unless they are listed under
  `minimumReleaseAgeExclude:` in `pnpm-workspace.yaml`; it adds the entries itself on the first install.
- `pnpm licenses list` needs `--recursive` to see workspace packages (`tools/check-licenses.mjs` passes it).
- With `node-linker=hoisted`, a dependency used by only one workspace project may still be installed under
  that project's own `node_modules` (e.g. `e2e/node_modules/@playwright/test`), so run its binaries through
  `pnpm --filter <project> exec`.

## Android command-line tools: deviation from PLAN.md §4.4

PLAN.md pins `commandlinetools-linux-16111833`. In that build `sdkmanager` is a deprecated shim that
downloads and runs the new native "Android CLI" (`~/.android/bin/android-cli`). Its downloader only opens
IPv6 connections (observed with `ss -tn state syn-sent`), so on this machine `sdkmanager` hangs forever with
no output, and neither `-Djava.net.preferIPv4Stack=true` nor `JAVA_TOOL_OPTIONS` changes that.
`tools/setup-android.sh` therefore installs **cmdline-tools 22.0 (build 15859902)**, the last Java-based
`sdkmanager`, with `SDKMANAGER_OPTS=-Djava.net.preferIPv4Stack=true`. It installs the identical SDK packages
(`platform-tools`, `platforms;android-36`, `build-tools;36.0.0`). Override with
`CMDLINE_TOOLS_BUILD=<build> CMDLINE_TOOLS_REVISION=<rev>` on a host with working IPv6.

Gradle (used by the Tauri Android build from WP-89) runs on the JVM, which prefers IPv4, so the Gradle wrapper download is not affected.

## Test oracles

- `tools/oracle-soffice.sh <in> <fmt> <outdir>` converts with LibreOffice headless using the profile
  `/tmp/kiln-lo-profile` (override: `KILN_LO_PROFILE`). On first run it seeds
  `user/registrymodifications.xcu` with `OOXMLRecalcMode = 0` and `ODFRecalcMode = 0` (always recalculate).
  Verified: an `.xlsx` whose `A3 = A1*A2` carries a stale cached value of 999 converts to CSV as `6`.
  Conversions are serialised with `flock` on `<profile>.lock` because a LibreOffice profile cannot be used
  by two processes at once.
