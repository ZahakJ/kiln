# @officina/sheet-model

The Tabula (Grid) workbook model: workbook and sheets, the sparse cell store, cell styles, merges, defined names, the undo/command log and the number-format bridge (numfmt).

**Public API.** `PACKAGE_NAME` (the package's npm name). The package is consumed from source
(`exports` points at `src/index.ts`) through the `@officina/*` TypeScript paths and Vite aliases; there is
no per-package build step. Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`.

**Status.** Scaffold created by WP-01; the implementation lands in the work packages listed in PLAN.md §6.

Licence: AGPL-3.0-or-later.
