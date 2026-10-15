# Contributing to Officina

Officina is an office suite written in TypeScript: one React web core that runs
in the browser, inside Electron on the desktop and inside Capacitor on Android.
The code lives in a pnpm workspace: applications in `apps/`, libraries in
`packages/`, end-to-end tests in `e2e/`. [docs/DESIGN.md](docs/DESIGN.md) maps
the packages and explains how the pieces fit together; read it before changing
anything substantial.

Security problems do not belong in an issue or a pull request. See
[SECURITY.md](SECURITY.md).

## Prerequisites

- **Node 24** (the exact version is in `.nvmrc`). `tools/setup-node.sh` installs
  it in your home directory without root.
- **pnpm**, the version pinned in the `packageManager` field of `package.json`.
  With Corepack: `corepack enable`.
- **Chromium for Playwright**, for the end-to-end tests only:
  `pnpm --filter @officina/e2e exec playwright install chromium`.
- For the Android build only: JDK 21 and the Android command-line tools.
  `tools/setup-android.sh` installs both under `~/Android`.
- For fidelity tests only: LibreOffice (`soffice`) on the `PATH`. It is a test
  oracle and is never bundled.

`tools/verify-env.sh` checks all of the above and says what is missing.

## The development loop

```sh
pnpm install
pnpm dev          # the web app with hot reload on http://localhost:5173
```

Every application is a route of the web app (`/write`, `/grid`, `/deck`,
`/notes`; the hub is `/`), so nearly all work happens in a browser tab. The
desktop and Android shells only add platform glue: file dialogs, menus,
printing and intents.

Before you push:

```sh
pnpm verify       # = turbo run lint typecheck test, across every package
pnpm e2e          # Playwright, headless Chromium; writes screenshots to e2e/screenshots/
```

CI runs the same commands plus `pnpm licenses:check`, on every push to `main`
and on every pull request.

## Tests

- **Unit tests** use Vitest and sit next to the code (`src/**/*.test.ts`). A
  change in behaviour comes with a test that fails without it.
- **End-to-end tests** in `e2e/specs/` drive the real application in Chromium.
  User-visible changes take a screenshot in the light and dark themes. Look at
  the screenshots before you open the pull request; a reviewer will.
- **Fidelity tests** (`pnpm fidelity`) round-trip the fixture corpus of
  `.docx`, `.xlsx` and `.pptx` files and compare the result with LibreOffice and
  with the original. A change to a reader or writer must not lower a score in
  [FORMATS.md](docs/FORMATS.md).
- **Formula conformance** cases are CSV files in
  `packages/formula/test/conformance/`; a new or changed function adds cases
  there.

## Code style

- [Biome](https://biomejs.dev) formats and lints everything: `pnpm format`
  rewrites, `pnpm lint` checks. There is no separate Prettier or ESLint.
- TypeScript is strict. No `any` without a comment saying why.
- Comments explain constraints and decisions, not what the next line does.
- Every user-facing string goes through `@officina/i18n`, uses the product
  names (Scriba, Tabula, Scena, Notula, Atrium) and British spelling, and keeps
  Office's name for a command where Office has one.
- New third-party dependencies are pinned once in the `catalog:` of
  `pnpm-workspace.yaml` and must pass `pnpm licenses:check` (permissive licences
  only; see the allow-list in `tools/check-licenses.mjs`).

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
[`good first issue`](https://github.com/ZahakJ/officina/labels/good%20first%20issue)
are small and say where the code is. The parity tracker,
[PARITY.md](PARITY.md), lists Office features that are missing or partial,
each with the test that will prove it done.
