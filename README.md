<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="docs/media/banner.png">
    <source media="(prefers-color-scheme: light)" srcset="docs/media/banner-light.png">
    <img src="docs/media/banner.png" alt="Kiln: where work gets made. An office suite with a little fire in it." width="820">
  </picture>
</p>

<p align="center">
  <a href="https://github.com/ZahakJ/kiln/actions/workflows/ci.yml"><img src="https://github.com/ZahakJ/kiln/actions/workflows/ci.yml/badge.svg" alt="CI"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-AGPL--3.0--or--later-555" alt="AGPL-3.0-or-later"></a>
</p>

Kiln is an open-source office suite in development: a word processor (**Kiln
Write**), a spreadsheet (**Kiln Grid**), a presentation editor (**Kiln Deck**)
and a notebook (**Kiln Notes**), opened from **Kiln Home**. The aim is a suite
that opens and saves Word, Excel and PowerPoint files faithfully, keeps the
layout Office users know (ribbon, File menu, status bar, Office shortcuts) and
runs on Linux, Windows, macOS, Android and in the browser.

**Status.** Kiln is in active development on a native Rust core; there is no
release yet (v0.1.0 is planned once all 49 work packages land). Working today:

- **Engines:** text shaping and line breaking, the Write layout engine with
  pagination, tables and floats, a formula engine with 484+ Excel functions,
  charts, and PDF export;
- **Files:** Word, Excel and PowerPoint read and write with round-trip fidelity
  tests, plus CSV, TSV and JSON;
- **Apps:** the Kiln interface (ribbons, Backstage, Kiln Home), the Kiln Write
  editor, and the Kiln Grid surface and editor core; the Deck and Notes
  editors, collaboration, the desktop and Android apps come next.

Progress, remaining work, decisions and how the build is run are in
[docs/STATUS.md](docs/STATUS.md); feature parity with Office is tracked row by
row in [parity/parity.json](parity/parity.json).

The plan, the work packages and their order are in [PLAN.md](PLAN.md).

## How it is built

Everything that computes (document models, layout, formulas, file formats,
collaboration, storage, PDF) is Rust, in one Cargo workspace of `kiln-*`
crates. Documents are laid out in Rust and painted onto a canvas from a display
list, so pages look the same on every platform and in the PDF. The same core
runs natively in the Tauri shell and as WebAssembly in a Web Worker for the web
app. The interface around the document (ribbon, dialogs, panes) is a thin
TypeScript and React layer with no business logic. [DESIGN.md](docs/DESIGN.md)
is the map of the code; the decisions are recorded in [docs/adr](docs/adr).

## Building

Rust 1.99.1 (pinned in `rust-toolchain.toml`), Node 24 and pnpm (pinned in
`package.json`). The setup scripts install everything in user space.

```sh
git clone https://github.com/ZahakJ/kiln && cd kiln
bash tools/setup-rust.sh && bash tools/setup-node.sh && bash tools/verify-env.sh
pnpm install
cargo xtask verify       # rustfmt, Clippy with warnings denied, cargo-nextest
pnpm verify              # Biome, TypeScript, Vitest
pnpm dev                 # the web app on http://localhost:5173
pnpm e2e                 # Playwright end-to-end tests in headless Chromium
cargo run -p kiln-cli -- --version
```

The desktop shell needs the WebKitGTK development libraries on Linux
(`libwebkit2gtk-4.1-dev` and friends, listed in [docs/qa/env.md](docs/qa/env.md));
then `pnpm tauri dev` opens it. Release bundles for Linux, Windows, macOS and
Android are built by the release workflow from a version tag
([RELEASING.md](docs/RELEASING.md)).

## Documentation

| | |
|---|---|
| [STATUS.md](docs/STATUS.md) | Build progress, remaining work, decisions, how to resume |
| [PLAN.md](PLAN.md) | The implementation plan: product, architecture, budgets, work packages |
| [DESIGN.md](docs/DESIGN.md) | Architecture: crates, packages, runtime, threads, rendering |
| [FORMATS.md](docs/FORMATS.md) | What each file-format crate covers and the fidelity targets |
| [SHORTCUTS.md](docs/SHORTCUTS.md) | The Office keyboard shortcuts Kiln implements |
| [BRAND.md](brand/BRAND.md) | The brand: mark, app icons, colour, type, voice |
| [Design](docs/design/README.md) | The brand kit in the code, the mockups and which work package implements each |
| [CONTRIBUTING.md](CONTRIBUTING.md) | Development loop, tests, code style |
| [SECURITY.md](SECURITY.md) | Reporting a vulnerability |
| [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) | Third-party components and their licences |

## Licence

GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE).
Every dependency is under a permissive licence (MIT, Apache-2.0, BSD, ISC,
MPL-2.0, Zlib, Unicode or OFL), checked in CI by `cargo deny` and
`pnpm licenses:check`.
