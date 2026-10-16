# Round-trip fidelity

Written by `cargo xtask fidelity` (`kiln-cli fidelity`, scorer in `crates/kiln-testing/src/fidelity/`). Each file is read, written back without edits and read again; the metrics and thresholds are those of PLAN.md §3.10 and §5, per file from `parity/corpus.json` (the §3.10 release targets for files it does not list). Do not edit by hand: rerun the command.

- Formats: docx, xlsx, pptx
- Oracle: LibreOffice (`/usr/bin/soffice`) through `tools/oracle-soffice.sh`; pages rendered by `pdftoppm` at 60 dpi
- Result: **pass**

## Summary

No formats registered: no reader/writer has registered a `Roundtrip` adapter in `crates/kiln-testing/src/formats/mod.rs` yet. The `.docx` adapter arrives with WP-62/WP-65, `.xlsx` with WP-70 and `.pptx` with WP-77/WP-79; until then only the OPC passthrough baseline below is measured.

## OPC passthrough baseline

Every package opened and saved by `kiln-ooxml` without edits (what `kiln-cli repack` does): every part, `.rels` parts included, must come out byte-identical (PLAN.md §3.1 principle 4), and `[Content_Types].xml` must declare the same content types (it is parsed on open, so it is compared by its declarations and not counted among the parts below).

| Fixtures | Packages | Parts | Identical parts | Result |
|---|---|---|---|---|
| `docx/features` | 19 | 325 | 325 | pass |
| `docx/manual` | 2 | 10 | 10 | pass |
| `xlsx/features` | 20 | 196 | 196 | pass |
| `xlsx/manual` | 1 | 4 | 4 | pass |
| `pptx/features` | 19 | 900 | 900 | pass |
