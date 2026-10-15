# ADR 0001: Technology stack and licence

- **Status:** accepted
- **Date:** 2026-10-15
- **Work package:** WP-01 (source: PLAN.md §0, §1.1a, §3, §4)

## Context

Officina is an office suite (Scriba for documents, Tabula for spreadsheets, Scena for presentations,
Notula for notes, Atrium as the start screen) that must open, edit and save real Microsoft Office files,
feel familiar to Office users, and run on Linux, Windows, macOS, Android and the web. It is built by a
small team of agents working concurrently on a 4-CPU, 7 GB machine without root, Rust or Docker.

## Decisions

### One TypeScript + React core in a pnpm + Turborepo monorepo

- **TypeScript 5.9.3** (pinned; the Go-based 7.x compiler is not yet supported by all tooling), **React 19**,
  **Vite 8** (Rolldown) with `@vitejs/plugin-react` 6 and **Tailwind CSS 4** over our own design tokens.
- **pnpm 12** workspaces (`apps/*`, `packages/*`, `e2e`) with every third-party version declared once in
  the `catalog:` of `pnpm-workspace.yaml`; packages reference `"catalog:"` and never pin their own versions.
  `node-linker=hoisted` because electron-builder and Capacitor's Gradle copy step need a flat `node_modules`.
- **Turborepo 2** runs `lint`, `typecheck`, `test` (cached), `build` (depends on `^build`), `e2e` (depends on
  the web build) and the uncached `release:*` tasks.
- Packages are consumed **from source**: each `package.json` exports `./src/index.ts`, TypeScript resolves
  `@officina/*` through `paths` in `tsconfig.base.json`, and Vite through matching aliases. There is no
  per-package build step; only apps run `vite build`.
- **Biome 2** replaces ESLint + Prettier (recommended rules, 2 spaces, single quotes, 110 columns,
  organised imports). **Vitest 5** for unit tests (jsdom for UI packages), **Playwright 1.64** with headless
  Chromium for end-to-end tests and screenshots that are reviewed visually.
- **Node 24 LTS** (24.22.0) is installed in user space under `~/.local/opt`; the system Node 18 is too old
  for Vite 8, Vitest 5 and Capacitor 8.

### Shells

- **Desktop: Electron 44** via `electron-vite`, packaged by `electron-builder` (AppImage and deb locally;
  NSIS and dmg in GitHub Actions). Chosen over **Tauri 2** because no Rust toolchain is available, Tauri's
  Android path needs the NDK and Rust targets, and Tauri uses a different WebView per OS (WebKitGTK on Linux),
  which would make WYSIWYG pagination and print output differ between platforms. Electron ships the same
  Chromium everywhere; the cost (about 100 MB larger installers, about 150 MB more RAM) is accepted.
- **Android: Capacitor 8** (Java 21, Android SDK platform 36 and build-tools 36.0.0 in `~/Android`, Gradle
  wrapper). One web core is the only way to ship three editors on four platforms; native concerns (file
  intents, share sheet, SAF access, status bar, keyboard insets) are handled by Capacitor plugins, and the
  mobile UX is adapted in CSS/React rather than re-implemented in Kotlin or Flutter.
- **Web:** an installable PWA from the same build.

### Editors and engines

- **Write:** TipTap 3 / ProseMirror with our own pagination layer. Lexical was rejected because
  TipTap/ProseMirror has more mature table support and Yjs bindings.
- **Grid:** a custom canvas grid and **our own formula engine** (`@officina/formula`). HyperFormula was not
  adopted (see the licence section) and fast-formula-parser is unmaintained.
- **Deck:** a custom DOM/SVG slide stage with TipTap text bodies (Konva and Fabric were rejected in favour of
  DOM/SVG, which keeps text editable and accessible).
- **File formats:** our own OOXML layer (`@officina/ooxml`: OPC container over `fflate`, XML through
  `fast-xml-parser`) with opaque-part passthrough for round-trip fidelity. JSZip was rejected (dual MIT/GPL,
  slower than fflate); the `xlsx` package on npm (stale SheetJS CE 0.18.5 with known CVEs) was rejected.
- **Collaboration, undo, offline:** Yjs with `y-prosemirror`, `y-indexeddb` and Hocuspocus.
- **Charts:** ECharts 6. **PDF:** Chromium print plus `pdf-lib`; `pdfjs-dist` for previews and tests.
- **Test oracles (never shipped):** LibreOffice 24.2 headless (`tools/oracle-soffice.sh`, profile set to
  recalculate on load), ExcelJS, mammoth, docx, pptxgenjs and docx-preview as dev dependencies.

## Licence

Officina is licensed under the **GNU Affero General Public License v3.0 or later**.

1. The suite ships a collaboration server and a PWA, so it can be offered as a hosted service. The AGPL keeps
   hosted forks open; ONLYOFFICE, Collabora and Euro-Office chose the same licence for the same reason.
2. Every runtime dependency is permissive (MIT, Apache-2.0, BSD, ISC, 0BSD, Zlib), file-level copyleft
   (MPL-2.0) or a font licence (OFL-1.1), all of which may be combined into an AGPL work.
   `tools/check-licenses.mjs` enforces this on every production dependency in CI by evaluating each SPDX
   expression against the allow-list `MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, 0BSD, MPL-2.0,
   OFL-1.1, CC0-1.0, Unlicense, Python-2.0, BlueOak-1.0.0, Zlib` plus the verbatim expressions
   `(MIT AND BSD)`, `(MIT OR Apache-2.0)`, `(MPL-2.0 OR Apache-2.0)` and `(MIT OR GPL-3.0-or-later)`.
   Anything else, including `GPL-2.0-only`, SSPL, BUSL, `CC-BY-NC-*` and unknown licences, fails the check.
   `Zlib` was added to the PLAN.md list in WP-01 because `pako` (a dependency of `pdf-lib`) is
   `(MIT AND Zlib)`; the zlib licence is permissive and GPL-compatible.
3. GPL-3.0-only code such as HyperFormula could legally be combined with an AGPL-3.0 work (GPLv3 §13). We
   still build our own formula engine, for control over Excel semantics and so that `@officina/formula`
   could later be relicensed permissively. HyperFormula and Univer's Apache-2.0 formula engine remain
   admissible fallbacks.
4. LibreOffice (MPL-2.0) is used only as a development-time test oracle and is never bundled.
5. `THIRD-PARTY-NOTICES.md` is generated by `pnpm licenses:notices`.

## Consequences

- Contributors need Node 24 and pnpm 12 (`tools/setup-node.sh`); `tools/verify-env.sh` checks the
  environment.
- Adding a dependency means adding it to the catalog first; the licence gate runs in CI.
- Larger desktop installers than a Tauri build, in exchange for identical rendering on every desktop OS.
