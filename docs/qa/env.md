# Development environment (verified)

Facts verified by WP-01 on 2026-10-15 on the development machine. `bash tools/verify-env.sh` re-checks
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
| Playwright | 1.64.1, Chromium 1248 (Chrome 156) and headless shell 1248 | `~/.cache/ms-playwright`, installed by `pnpm --filter @officina/e2e exec playwright install chromium` (run by `tools/setup-node.sh` once the workspace is installed). No extra system libraries were needed: the smoke e2e passes headless without `install-deps`. |
| Java | OpenJDK 21.0.12 | `/usr/lib/jvm/java-21-openjdk-amd64` (system package) |
| Android SDK | cmdline-tools 22.0, platform-tools, platforms;android-36, build-tools;36.0.0 | `~/Android`, by `tools/setup-android.sh` (idempotent; persists `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_SDK_ROOT` and `PATH` in `~/.bashrc` and `~/.profile`) |
| LibreOffice | 24.2.7.2 | `/usr/bin/soffice`, test oracle only (`tools/oracle-soffice.sh`) |
| Python | 3.12.3 | system |
| git / gh | 2.43.0 / 2.45.0 | system |
| Other | `pdftoppm`, ImageMagick `convert`, `ffmpeg`, `zip`/`unzip` | system |
| Fonts | Liberation, Carlito, Caladea | system-wide |

## pnpm 12 behaviour worth knowing

- **Build scripts are opt-in.** `pnpm install` fails with `ERR_PNPM_IGNORED_BUILDS` when a new dependency has
  an install script. Approve it with `pnpm approve-builds <pkg>`, which records it under `allowBuilds:` in
  `pnpm-workspace.yaml` (currently only `esbuild`). Electron (WP-34) will need the same.
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

Gradle (used from WP-35) runs on the JVM, which prefers IPv4, so the Gradle wrapper download is not affected.

## Test oracles

- `tools/oracle-soffice.sh <in> <fmt> <outdir>` converts with LibreOffice headless using the profile
  `/tmp/officina-lo-profile` (override: `OFFICINA_LO_PROFILE`). On first run it seeds
  `user/registrymodifications.xcu` with `OOXMLRecalcMode = 0` and `ODFRecalcMode = 0` (always recalculate).
  Verified: an `.xlsx` whose `A3 = A1*A2` carries a stale cached value of 999 converts to CSV as `6`.
  Conversions are serialised with `flock` on `<profile>.lock` because a LibreOffice profile cannot be used
  by two processes at once.
