# Contributing to Kiln

Kiln is an office suite with a native Rust core and a thin TypeScript + React
presentation layer. The core is a Cargo workspace (`crates/kiln-*`, `xtask/`);
it runs natively in the Tauri 2 shell (`apps/desktop/src-tauri`, desktop and
Android) and as WebAssembly in the web app (`apps/web`). The presentation
packages are a pnpm workspace (`packages/*`, `apps/*`, `e2e/`).
[docs/DESIGN.md](docs/DESIGN.md) maps the crates and packages and explains how
the pieces fit together; read it before changing anything substantial.

Security problems do not belong in an issue or a pull request. See
[SECURITY.md](SECURITY.md).

## Prerequisites

- **Rust 1.99.1** with rustfmt, Clippy, the wasm32 and Android targets,
  cargo-nextest, sccache, wasm-bindgen-cli, wasm-opt, cargo-deny and
  cargo-about. `tools/setup-rust.sh` installs all of it in your home directory
  without root; `rust-toolchain.toml` pins the toolchain.
- **Node 24** (the exact version is in `.nvmrc`). `tools/setup-node.sh` installs
  it in your home directory without root.
- **pnpm**, the version pinned in the `packageManager` field of `package.json`.
  With Corepack: `corepack enable`.
- **Chromium for Playwright**, for the end-to-end tests only:
  `pnpm --filter @kiln/e2e exec playwright install chromium`.
- For the desktop shell only, on Linux: the WebKitGTK development libraries
  (`libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev
  librsvg2-dev libxdo-dev libssl-dev patchelf file`). The core builds without
  them.
- For the Android build only: JDK 21, the Android command-line tools and NDK
  r29. `tools/setup-android.sh` installs them under `~/Android`.
- For fidelity tests only: LibreOffice (`soffice`) on the `PATH`. It is a test
  oracle and is never bundled.

`tools/verify-env.sh` checks all of the above and says what is missing.

## The development loop

```sh
pnpm install
pnpm dev          # the web app with hot reload on http://localhost:5173
cargo nextest run -p kiln-formula   # iterate on one crate
```

Every application is a route of the web app (`/write`, `/grid`, `/deck`,
`/notes`; Kiln Home is `/`), so most interface work happens in a browser tab.
Engine work happens in the crates, tested from Rust and driven without a GUI
through `kiln-cli`. The desktop and Android shells only add platform glue: file
dialogs, menus, printing and intents.

Before you push:

```sh
cargo xtask verify   # rustfmt --check, Clippy -D warnings, nextest (add -p <crate> to narrow)
pnpm verify          # = turbo run lint typecheck test, across every package
pnpm e2e             # Playwright, headless Chromium; writes screenshots to e2e/screenshots/
```

CI runs the same commands plus `cargo deny check`, the WebAssembly build, the
desktop start-up probe and `pnpm licenses:check`, on every push to `main` and on
every pull request.

## The thinness rule

Anything that computes belongs in Rust. Code in `packages/*` and `apps/*` may
render, keep UI state, check the shape of form input, turn DOM events into
commands and paint scenes. It may not parse or write a document format or
compute layout, formulas, number formats, sorting, filtering, search results or
spelling. Biome rejects imports of document-format and formula libraries there;
reviewers check the rest.

## Tests

- **Rust tests** run with cargo-nextest: unit tests next to the code, integration
  tests in `crates/<name>/tests/`, `insta` snapshots and `proptest` properties
  where they fit. A change in behaviour comes with a test that fails without it.
- **TypeScript unit tests** use Vitest and sit next to the code
  (`src/**/*.test.ts`).
- **End-to-end tests** in `e2e/specs/` drive the real application in Chromium.
  User-visible changes take a screenshot in the light and dark themes. Look at
  the screenshots before you open the pull request; a reviewer will.
- **Fidelity tests** (`cargo xtask fidelity`) round-trip the fixture corpus of
  `.docx`, `.xlsx` and `.pptx` files and compare the result with LibreOffice and
  with the original. A change to a reader or writer must not lower a score in
  [FORMATS.md](docs/FORMATS.md).
- **Formula conformance** cases are CSV files in `conformance/`; a new or
  changed function adds cases there.

## Code style

- Rust: `cargo fmt`, Clippy with warnings denied, `#![forbid(unsafe_code)]` in
  every core crate, no panics on user input (return errors), and dependency
  versions pinned once in `[workspace.dependencies]` of the root `Cargo.toml`.
- [Biome](https://biomejs.dev) formats and lints the TypeScript: `pnpm format`
  rewrites, `pnpm lint` checks. There is no separate Prettier or ESLint.
- TypeScript is strict. No `any` without a comment saying why.
- Comments explain constraints and decisions, not what the next line does.
- Every user-facing string goes through the i18n layer, uses the product
  names (Kiln Write, Kiln Grid, Kiln Deck, Kiln Notes, Kiln Home) and British spelling, and keeps
  Office's name for a command where Office has one.
- New third-party dependencies are pinned once (Rust in `Cargo.toml`, npm in the
  `catalog:` of `pnpm-workspace.yaml`) and must pass `cargo deny check` and
  `pnpm licenses:check` (permissive licences only; the allow-list is in
  `deny.toml` and `tools/check-licenses.mjs`). `cargo xtask notices`
  regenerates THIRD-PARTY-NOTICES.md.

## Commits and pull requests

- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org):
  `feat(grid): sort by colour`, `fix(io-docx): keep empty sections`,
  `docs:`, `test:`, `build:`, `ci:`, `chore:`.
- Sign off every commit (`git commit -s`). The sign-off certifies the
  [Developer Certificate of Origin](https://developercertificate.org): that you
  wrote the change, or otherwise have the right to submit it under this
  project's licence. There is no CLA.
- Contributions are accepted under the GNU Affero General Public License,
  version 3 or later, the licence of the project.
- One concern per pull request. Add a line to the `Unreleased` section of
  [CHANGELOG.md](CHANGELOG.md) for anything a user would notice.
- Use your GitHub noreply address as the commit e-mail
  (`ID+username@users.noreply.github.com`, shown on
  <https://github.com/settings/emails>) unless you want your address public.

## Where to start

Issues labelled
[`good first issue`](https://github.com/ZahakJ/kiln/labels/good%20first%20issue)
are small and say where the code is. Once the parity tracker exists (PARITY.md, from WP-59) it lists the Office
features that are missing or partial, each with the test that will prove it
done.
