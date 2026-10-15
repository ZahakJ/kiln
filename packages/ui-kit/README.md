# @officina/ui-kit

Office-familiar UI building blocks shared by every app: Ribbon (Classic and Simplified), Backstage, StatusBar, CommandPalette, task panes, dialogs, the mini toolbar, TransformBox, colour/font pickers and the mobile bottom sheet. Built on Radix primitives, Lucide icons and the Officina Glyphs set.

**Public API.** `PACKAGE_NAME` (the package's npm name). The package is consumed from source
(`exports` points at `src/index.ts`) through the `@officina/*` TypeScript paths and Vite aliases; there is
no per-package build step. Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`.

**Status.** Scaffold created by WP-01; the implementation lands in the work packages listed in PLAN.md §6.

Licence: AGPL-3.0-or-later.
