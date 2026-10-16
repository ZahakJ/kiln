# Parity tracker

The machine-readable list of what people coming from Microsoft Office would miss in Kiln, and of
the places where Kiln sets out to be better (PLAN.md §7). `cargo xtask parity` validates it and
generates [PARITY.md](../PARITY.md) and the README badge ([docs/parity-badge.json](../docs/parity-badge.json)).

| File | What it holds | Edited by |
|---|---|---|
| `parity.json` | One row per feature: Office location, would-miss tier, priority, status, delivering work package, verify ids | every work package that changes a status (append-only for new rows) |
| `functions.json` | Every Excel worksheet function (523 names from Microsoft's public list), its family, the Excel version that introduced it and its status | by hand for `out` decisions; `cargo xtask parity` computes `done` |
| `corpus.json` | Every fixture under `fixtures/{docx,xlsx,pptx}/features` with the fidelity checks it must pass and the rows it exercises | the work package that adds a fixture |
| `schema.json` | JSON Schema (2020-12 subset) for all of the above and for release snapshots | WP-59 and later tracker changes |
| `releases/<tag>.json` | Every row's status at a release tag; PARITY.md lists flips since the newest | `cargo xtask parity --snapshot <tag>` at release time |

## Commands

```sh
cargo xtask parity                  # validate, then write PARITY.md, the badge and computed function statuses
cargo xtask parity --check          # CI: validate and fail if any generated file is stale
cargo xtask parity --verify         # run the tests behind every done, better and partial row
cargo xtask parity --verify --dry-run --app grid --row grid.view.window.freeze-panes
cargo xtask parity --snapshot v0.1.0
```

`PARITY.md`, `docs/parity-badge.json` and the `status` fields of `functions.json` are generated:
never merge them by hand. On a conflict take either side and run `cargo xtask parity`.

## Rows

```json
{"id": "write.home.clipboard.format-painter", "app": "write", "area": "Home ▸ Clipboard",
 "feature": "Format Painter (single click, and double-click to keep painting)",
 "office": "Word ▸ Home ▸ Clipboard ▸ Format Painter", "tier": "critical", "priority": "P0",
 "status": "missing", "wp": "WP-66",
 "verify": ["e2e:write-features:format-painter", "unit:kiln-session/write/format_painter"], "notes": ""}
```

- `id` is dotted, unique and stable, and starts with the app. Never rename an id; add a new row
  and leave the old one if the scope really changed.
- `tier` is the importance to a switcher: `critical` (absence sends people back within a week),
  `commonly-missed` (used weekly by many, or by whole professions), `niche`.
- `priority` is the delivery order and is tied to the tier: critical rows are `P0`,
  commonly-missed rows `P0` or `P1`, niche rows `P2` or `out`. `out` rows stay `missing`, have no
  work package and are excluded from the score; they exist so the scope decision is visible.
- `status` is `missing`, `partial`, `done` or `better`. A row is `done` only when every automated
  verify id exists and passes; `better` additionally needs a `notes` sentence saying what is
  better than Office. `partial` needs at least one passing automated verify id (the delivered
  part) and a note saying what is missing. Rows whose tests do not exist yet stay `missing`.
- `office: "—"` marks rows that exist to be better than Office: they are `commonly-missed` and can
  only become `better`.
- Each function family has one row `grid.formulas.functions.<family>`; it may be `done` only when
  every planned function of the family is done in `functions.json`.

The schema enforces the tier, priority, status and verify rules; `cargo xtask parity` also checks
unique ids, known work packages, family consistency and the corpus against `fixtures/`.

## Verify ids

| Form | Resolves to | Passes when |
|---|---|---|
| `e2e:<spec>:<slug>` | `e2e/specs/<spec>.spec.ts`, the `test(...)` whose title slugifies to `<slug>` (lower case, other characters collapsed to `-`), run with `playwright test --grep` | the test exists, is not skipped and passes |
| `unit:<crate>/<path>` | `crates/<crate>` (or `xtask`): `cargo nextest run -p <crate> --no-tests=fail -E <filter>`. `<path>` is a module path with `/` for `::` and `-` for `_`, matched in order; a leading `tests/<binary>/` selects an integration test binary | at least one test matches and all pass |
| `unit:<package>/<path>` | `packages/<package>` or `apps/<package>`: `vitest run -t <pattern>`, path segments matched in order, any case | at least one test matches and all pass |
| `conformance:<csv>[#FUNCTION]` | `conformance/<csv>` and the runner `crates/kiln-formula/tests/conformance.rs` | the function is implemented, the CSV has non-excluded rows calling it, and the whole conformance suite passes with 0 failures |
| `fidelity:<format>:<metric>>=<value>` | the summary table of `docs/qa/fidelity.md` (written by `cargo xtask fidelity`): a table with a `Format` column and one column per metric (`structural`, `text`, `visual`, `values`, `formulas`, `oracle_opens`, `byte_identical_untouched`), one row per format; ratios, percentages or yes/no | the reported value (the lowest, if several tables report it) is at least `<value>` |
| `manual:docs/qa/<file>.md#<anchor>` | a documented manual check | never counts as verification; allowed on niche rows only |

## Functions

`functions.json` lists every Excel worksheet function from Microsoft's public alphabetical list
(retrieved 2026-10-16), plus COPILOT and FIELDVALUE, which that list omits. `since` is the Excel
version that introduced the function (`2007` means 2007 or earlier, `365` means Microsoft 365
only). Status is `planned`, `out` (with the reason in `notes`) or `done`; `done` is computed from
`conformance/functions-implemented.json`, which the formula library writes (`{"NAME":
"implemented" | "planned"}`): a function is done exactly when it is implemented there.

## Corpus

Each entry names a fixture under `fixtures/` and the checks of PLAN.md §2A.3 it must meet
(`structural`, `text`, `visual`, `values`, `formulas` as ratios; `oracle_opens` and
`byte_identical_untouched` as booleans) and, optionally, the parity rows it exercises. Every file
in `fixtures/{docx,xlsx,pptx}/features` must be listed; add a fixture and its entry in the same
commit, and add real-world files that break before fixing them.
