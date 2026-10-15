# @officina/ooxml

The Office Open XML core: the OPC zip container (fflate), XML (de)serialisation (fast-xml-parser), relationships, content types, DrawingML, themes and opaque-part passthrough for round-trip fidelity.

**Public API.** `PACKAGE_NAME` (the package's npm name). The package is consumed from source
(`exports` points at `src/index.ts`) through the `@officina/*` TypeScript paths and Vite aliases; there is
no per-package build step. Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test`.

**Status.** Scaffold created by WP-01; the implementation lands in the work packages listed in PLAN.md §6.

Licence: AGPL-3.0-or-later.
