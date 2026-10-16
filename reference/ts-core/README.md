# reference/ts-core

Read-only oracle for the Rust port; not built; deleted by WP-97.

These are the TypeScript engines of the v1.0 plan (WP-05, WP-07, WP-09, WP-10, WP-11, WP-16, plus the
archived WP-13 DOCX writer and WP-18 XLSX reader from the `archive/ts-core-wip` branch), moved here by
WP-50 when the core moved to Rust (PLAN.md §3.13). Porting agents read the implementation and its tests
side by side with the Rust crate they are writing:

| Directory | Ported to |
|---|---|
| `ooxml/` | `crates/kiln-ooxml` (WP-55) |
| `formula/` | `crates/kiln-formula` (WP-56, WP-57) |
| `doc-model/` | `crates/kiln-doc` (WP-61) |
| `io-docx/` (reader, `src/write` writer, `test/`) | `crates/kiln-io-docx` (WP-62, WP-65) |
| `sheet-model/` | `crates/kiln-sheet` (WP-69) |
| `io-xlsx/` | `crates/kiln-io-xlsx` (WP-70) |
| `testing/` (fidelity scorer, manual-fixture generator) | `crates/kiln-testing` (WP-60) |

Rules: nothing here is part of the pnpm workspace, Biome, TypeScript project references or CI, and no
code outside this directory may import it. Package names use the `@kiln-ref/*` scope so they can never
be confused with the live `@kiln/*` packages. Fix bugs in the Rust port, not here. The formula
conformance CSVs that used to live in `formula/test/conformance/` are now the shared oracle in
`/conformance`, and the fixtures in `/fixtures`.
