# @officina/tokens

Design tokens of the "Horizon" design language: CSS custom properties (`--of-*`) for the light, dark and high-contrast themes, the same tokens as a typed TS object and a Tailwind 4 `@theme` block, plus the UI fonts (Inter Variable, JetBrains Mono) and the bundled document fonts.

**Public API.** `PACKAGE_NAME` (the package's npm name). The package is consumed from source
(`exports` points at `src/index.ts`) through the `@officina/*` TypeScript paths and Vite aliases; there is
no per-package build step. Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`.

**Status.** Scaffold created by WP-01; the implementation lands in the work packages listed in PLAN.md §6.

Licence: AGPL-3.0-or-later.
