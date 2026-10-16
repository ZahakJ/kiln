# kiln-formula

Kiln Grid's spreadsheet formula engine (PLAN.md §3.4, §3.8, §3.9): the tokenizer and Pratt parser, references
and their rewriting, the value model, the dependency graph with incremental recalculation by topological levels
(independent cells evaluated in parallel), dynamic arrays, Excel number formats, and the function library: 497
Excel worksheet functions with their documentation (`functions`). The library plugs in through the
`FunctionRegistry`: `Engine::new()` knows only the core set (`SUM`, `IF`, `AND`, `OR`, `NOT`, `TRUE`, `FALSE`),
`Engine::with_options(kiln_formula::functions::engine_options())` knows everything.

```rust
use kiln_formula::{Engine, Value};

let mut engine = Engine::new();
engine.add_sheet("Sheet1").unwrap();
engine.set_value("Sheet1", 0, 0, 2.0).unwrap();
engine.set_formula("Sheet1", 0, 1, "=A1*21").unwrap();
engine.recalc();
assert_eq!(engine.value("Sheet1", 0, 1).unwrap(), Value::Number(42.0));
```

## Parser — `parser`

- **Tokens**: numbers, text, logicals, every error literal (`#DIV/0!` … `#SPILL!`, `#CALC!`), A1 references
  (`A1`, `$A$1`, `A1:B2`, `A:A`, `1:1`), sheet qualifiers (`Sheet1!A1`, `'My Sheet'!A1:B2`, 3-D `Sheet1:Sheet3!A1`,
  external `[1]Sheet1!A1`, `Sheet1!#REF!`), names and sheet-scoped names, functions (with `_xlfn.` prefixes),
  structured references (`Table1[[#Headers],[Col]]`, `[@Qty]`, `T[@[Unit Price]]`), operators including `%`, `&`,
  comparisons, range `:`, union `,`, intersection (space), implicit intersection `@` and the spill operator `#`, and
  array constants. R1C1 notation (`R[-1]C`, `R2C3`, `C[1]`) resolves against an anchor cell. The tokenizer never
  fails: unreadable input becomes `Invalid` tokens so the formula bar can still colour it.
- **AST** (`Node`): byte spans on every node; `parse` reports `ParseError { message, span }`. Formulas longer than
  Excel's 8,192 characters, nesting deeper than 256 levels or trees deeper than 1,024 are refused, so no input can
  exhaust the stack (checked on 2 MB threads in debug builds).
- **Printer**: `print` produces canonical text (no spaces except intersections, upper-case functions, normalised
  areas, shortest round-trip number literals); `parse(print(ast)) == ast` for every formula of the conformance suite.

## References — `refs`

`shift_formula` (insert/delete rows and columns, `#REF!` for deleted references, areas clamped at the sheet edge),
`copy_formula` (relative references move), `move_formula` (cut and paste: references into the block follow it,
references to the cells it lands on become `#REF!`), `rename_sheet_in_formula` (quoting when needed),
`remove_sheet_in_formula`, `to_r1c1` / `from_r1c1`, and `toggle_absolute` (the F4 cycle `A1 → $A$1 → A$1 → $A1`
over the references at the caret or in the selection, on unfinished formulas too). Rewrites splice the changed
references into the original text, so the user's spacing and line breaks survive.

## Values — `values`

`Value` = `Blank | Number | Str | Bool | Error | Array | Lambda` (text and arrays reference-counted, so values are
cheap to clone and cross threads). Excel's coercions (`"1,234"`, `"$5"`, `"(5)"`, `"50%"`, `"1 1/2"`, en-US dates
and times as text), comparison rules (numbers < text < logicals, case-insensitive accent-sensitive text, blanks take
the other operand's type, 15-significant-digit "almost equal"), `excel_round`, `number_to_text` (what `=A1&""`
shows), and date serials in the 1900 system (with the Lotus 1900-02-29) and the 1904 system.

## Engine — `engine`

- **Compiled formulas**: each formula compiles to an `Expr` with sheet and function names resolved and relative
  references stored as offsets, so a formula filled down a million rows is one shared compiled tree (like Excel's
  shared formulas).
- **Dependency graph**: single-cell precedents are hash edges; every distinct range is one node in a per-sheet
  interval tree, so `SUM(A:A)` costs one entry and "who reads C7?" is a stab query. Spill areas are claims in the
  same kind of tree. Names, tables and named lambdas are tracked so redefining them recalculates their users.
- **Recalculation**: dirty marking, then Kahn's algorithm by topological levels; levels of 128 cells or more are
  evaluated on the current rayon pool against a read-only snapshot (cells that read anything still pending are
  redone serially, so results are deterministic and identical to serial evaluation). What Kahn cannot order goes to
  Tarjan's strongly connected components: cycles show 0 and are flagged (`is_circular`, `circular_cells`, as Excel
  does) unless iterative calculation is enabled (`IterativeOptions`: maximum iterations and change).
- **Semantics**: lazy arguments (`ArgMask`) for `IF` and the library's `IFS`/`CHOOSE`/`LET`/`LAMBDA`; array lifting
  (`lift` mask) and broadcasting (`#N/A` outside a smaller operand); dynamic arrays with spilling, `#SPILL!` for
  values, formulas, other spills or the sheet edge in the way, and the `A1#` operator; implicit intersection for
  legacy formulas (`CellInput::Formula { legacy: true }`) and `@`; volatile functions; defined names with workbook
  and sheet scope; tables and structured references through the host; 3-D references in workbook order; structural
  edits, range moves, sheet renames, removals and reordering.
- **Formula Lab** (`Engine::explain`, `explain_at`): evaluates a formula once with tracing and returns an
  `ExplainStep` tree — every AST node with its source span, label, value (references dereferenced and shown as
  `A1:A3 → {1;2;3}`), whether and how often it was evaluated (the untaken `IF` arm: never; a `MAP` lambda body:
  once per item) and the function's description.
- **Host**: literal values live in an `EngineHost` (the sheet model); `MemoryHost` is the in-memory one.
- **Limits**: `#NUM!` instead of a crash for runaway lambda recursion (`MAX_LAMBDA_DEPTH`) or evaluation depth.

## Function registry — `registry`

`FunctionSpec { name, min_args, max_args, volatile, lazy, lift, implementation, docs }` and the
`FunctionRegistry` trait; `FunctionTable` is the standard registry (`with_core()`, `register`, `unregister`).
Implementations receive `&mut [Arg]` and a `CallContext` that can force lazy arguments, evaluate expressions in a
lexical `Scope`, dereference and intersect references, make and invoke lambdas, resolve `INDIRECT` text, and query
sheets, tables and row visibility — enough for the library (WP-57) to add every family.

## Function library — `functions`

497 functions in every Excel family: the 484 of the v1.0 TypeScript library, ported with identical names,
arities, volatility and lazy/lifted arguments (`tests/conformance_registry.rs` checks them against a snapshot of the
v1.0 registry), and 13 that v1.0 lacked (`AMORDEGRC`, `AMORLINC`, `ODDFPRICE`, `ODDFYIELD`, `ODDLPRICE`, `ODDLYIELD`,
`EUROCONVERT`, `ASC`, `JIS`, `DBCS`, `BAHTTEXT`, `PHONETIC`, `PERCENTOF`): Math & Trig (including `SUBTOTAL`, `AGGREGATE`, `SUMPRODUCT`, matrices, `SEQUENCE`, `RANDARRAY`),
conditional aggregates (`COUNTIFS`, `SUMIFS`, `MAXIFS` … with Excel's criteria and wildcards), Statistical with
every distribution and its inverse, regression (`LINEST`, `LOGEST`, `FORECAST`), and tests; Logical including
`LET`, `LAMBDA`, `MAP`, `REDUCE`, `SCAN`, `MAKEARRAY`, `BYROW`, `BYCOL`; Information; Text including `TEXT`
(through `numfmt`), `TEXTSPLIT`, `TEXTBEFORE`, `TEXTAFTER` and `REGEX*` (`fancy-regex`, with look-around and
back-references and a backtracking limit); Date & Time including `NETWORKDAYS.INTL`, `WORKDAY.INTL`, `DATEDIF`,
`YEARFRAC`; Lookup & Reference including `XLOOKUP`, `XMATCH`, `FILTER`, `SORT`, `SORTBY`, `UNIQUE`, `TAKE`,
`DROP`, `HSTACK`, `VSTACK`, `TOCOL`, `TOROW`, `WRAPROWS`, `WRAPCOLS`, `EXPAND`, `OFFSET`, `INDIRECT`; Financial
(time value of money, cash flows, depreciation including the French `AMORLINC`/`AMORDEGRC`, bonds with odd first
and last periods, bills, `EUROCONVERT`; `rust_decimal` for `DOLLARDE`/`DOLLARFR`);
Engineering including `CONVERT`, base conversions, bitwise operations, complex numbers and Bessel functions;
Database (`DSUM` … `DGET`); and `ENCODEURL`.

```rust
use kiln_formula::{Engine, Value, functions};

let mut engine = Engine::with_options(functions::engine_options());
let sheet = engine.add_sheet("Sheet1").unwrap();
let v = engine.evaluate_at("=XLOOKUP(5,{1,3,5},{\"a\",\"b\",\"c\"})", sheet, 0, 0, false).unwrap();
assert_eq!(v, Value::from("c"));
let doc = functions::doc("XLOOKUP").unwrap();
assert!(doc.syntax.starts_with("XLOOKUP(lookup_value"));
```

- **Documentation**: `crates/kiln-formula/docs/<FAMILY>.json` (compiled in with `include_str!`) gives each
  function its category, summary, signature, argument descriptions, the Excel version that introduced it and
  worked examples; `functions::docs()` lists them in Insert Function order and `functions::doc(name)` finds one
  (case-insensitive, `_xlfn.` allowed). Autocompletion, the Insert Function dialog and the Formula Lab read it.
- **Conformance**: `tests/conformance.rs` evaluates every row of `conformance/curated-*.csv` and
  `conformance/generated-*.csv` (5,450 rows: Microsoft's documented examples, curated Excel behaviours including
  279 dynamic-array and LAMBDA rows, and LibreOffice-oracle rows) with 0 failures; `tests/docs.rs` checks that
  every function has exactly one documentation entry and that all 890 documented examples evaluate as stated;
  `tests/conformance_fuzz.rs` calls every function at every arity with hostile arguments and requires an answer
  without a panic or a stall, and requires every cumulative distribution function to return a probability. Known differences from Excel: `docs/formula-deviations.md`.
- **Parity**: `conformance/functions-implemented.json` (kept current by `tests/conformance_registry.rs`; rewrite it
  with `KILN_BLESS=1`) tells `cargo xtask parity` which Excel functions are done.
- `cargo run -p kiln-formula --example functions -- --count | --list [FAMILY] | --doc XLOOKUP` prints the library
  from the command line; `--eval FORMULA…` evaluates formulas with it (every digit of numbers shown).

## Number formats — `numfmt`

`NumberFormat::parse(code).format(&value, &FormatOptions)`: sections and conditions, colours (`color()`), literal
and escaped text, `_` skips and `*` fills, thousands separators and scaling commas, percent, scientific, vulgar
fractions, dates and times with elapsed `[h]`/`[mm]`/`[ss]`, sub-seconds, `AM/PM` and `A/P`, Buddhist years, the
Hijri calendar, locale and currency codes (`[$€-409]`), and `General` with Excel's 11-character rule. Rounding is
decimal, as in Excel. Locale data is English for now (other locale codes use the English names).

## Tests and benchmarks

- `cargo nextest run -p kiln-formula`: unit tests plus `tests/conformance*.rs` and `tests/docs.rs` (the function
  library, above), `tests/parser_*.rs` (tokenizer, parser, rewriting,
  round-trips of every conformance formula), `tests/engine_*.rs` (recalculation, spills, structural edits, LET and
  LAMBDA through the registry, core functions and operators, stack limits, the Formula Lab tree,
  `conformance/core/*.csv`) and
  `tests/numfmt_conformance.rs` (`conformance/numfmt/*.csv`, regenerated with `node tools/gen-numfmt-cases.mjs`
  from the `numfmt` oracle, a dev-only dependency).
- `cargo run --release -p kiln-formula --example recalc_bench` measures B8 (full recalculation of 1,000,008
  formulas on 4 threads; `THREADS=n` to vary) and B9 (single-edit incremental recalculation in a 100,008-formula
  workbook, 95th percentile), and exits with status 1 when either budget is missed.
- `cargo build --target wasm32-unknown-unknown -p kiln-formula --no-default-features` builds the single-threaded
  web variant.
