# @officina/formula

Officina's own spreadsheet formula engine: tokenizer, Pratt parser, AST with source spans,
reference rewriting, the value model, a dependency graph with incremental recalculation, dynamic
arrays, LAMBDA, and the function registry.

## Engine

```ts
import { Engine } from '@officina/formula';

const engine = new Engine(); // in-memory host; pass { host } to use your own cell storage
engine.addSheet('Sheet1');
engine.setCell('Sheet1', 0, 0, { value: 2 }); // A1
engine.setCell('Sheet1', 0, 1, { formula: '=SEQUENCE(A1*2)' }); // B1 spills into B1:B4
const { changed, circular } = engine.recalc(); // incremental
engine.getValue('Sheet1', 3, 1); // 4
engine.getSpillRange('Sheet1', 0, 1); // { row: 0, col: 1, row1: 3, col1: 1, blocked: false }
engine.explain('=IF(A1>1,"big","small")'); // step tree for the Formula Lab
```

- **Host interface.** Literal values live in an `EngineHost` (`getCellValue`, optional `setCellValue`,
  `getUsedBounds`, `getTable`/`getTableAt` for structured references, `applyStructuralEdit`). The engine
  owns formulas and their results. `MemoryHost` is a complete in-memory host.
- **API.** `setCell(sheet,row,col,{formula|value,legacy})`, `clearCell`, `cellChanged`, `insertRows/Cols`,
  `deleteRows/Cols`, `moveRange`, `addSheet`, `renameSheet`, `removeSheet`, `moveSheet`,
  `defineName`/`removeName` (workbook or sheet scope), `tablesChanged`, `recalc()` / `recalcAll()`,
  `getValue`, `getResult`, `getFormula`, `getSpillRange`, `isCircular`, `circularCells`,
  `evaluate(formula, at?)`, `explain(formula, at?)`, `setIterative({ maxIterations, maxChange })`.
- **Dependency graph.** Single-cell precedents are edges; each distinct range is one node in a per-sheet
  interval tree, so `SUM(A:A)` costs one node. Dirty cells are ordered with Kahn's algorithm; what is
  left (cycles and their dependents) goes through Tarjan's SCCs: cycles show 0 with a circular flag, or
  iterate when iterative calculation is on. Volatile functions (NOW, TODAY, OFFSET, INDIRECT) recalculate
  every time.
- **Dynamic arrays.** Array results spill; a non-empty cell, another formula or another spill in the way
  gives `#SPILL!`, which clears itself when the obstruction goes. `A1#` refers to a spill range. Formulas
  set with `legacy: true` use implicit intersection instead.

## Parser and references

`tokenize` (never throws; for syntax highlighting), `parse` (throws `FormulaParseError` with a span),
`print` (canonical text; `parse(print(ast))` round-trips), A1 and R1C1 (`toR1C1`, `fromR1C1`).
Reference rewriting keeps the user's spacing: `shiftFormula` (insert/delete rows/columns, `#REF!` on
deletion), `copyFormula` (relative adjustment), `moveFormula` (cut/paste), `renameSheetInFormula`,
`toggleAbsolute` (the F4 cycle). The grammar covers absolute/mixed references, quoted and 3-D sheet
references, whole rows/columns, structured references (`[#All]`, `[#Headers]`, `[#Data]`, `[#Totals]`,
`[@Col]`, column ranges), names, array constants, Excel operator precedence (unary minus before `%` and
`^`, left-associative `^`), intersection (space), union (comma in parentheses), `@` and `#`.

## Values

`ErrorValue`/`Errors`, `BLANK`, `MISSING`, `Array2D`, `Lambda`, `RefValue`. Coercion follows Excel
(en-US): numeric text with `$`, thousands separators, `%`, accounting parentheses and en-US dates/times;
comparisons are case-insensitive with numbers < text < logicals and 15-significant-digit equality
(`=0.1+0.2=0.3` is TRUE); `numberToText` matches Excel's General conversion; date serials follow the
1900 system (with the Lotus 1900-02-29) or the 1904 system.

## Functions

`registerFunction({ name, minArgs, maxArgs, lazy, lift, volatile, impl, docs })` adds a function; `lazy`
argument indices arrive unevaluated, `lift` indices are applied element-wise over arrays. The engine
ships a native core set: SUM, AVERAGE, COUNT, COUNTA, MIN, MAX, IF, IFS, AND, OR, NOT, IFERROR, ROUND,
ABS, INT, MOD, LEN, LEFT, RIGHT, MID, CONCAT, TEXTJOIN, VLOOKUP, XLOOKUP, INDEX, MATCH, ROW, COLUMN,
OFFSET, INDIRECT, TODAY, NOW, DATE, SEQUENCE, FILTER, SORT, UNIQUE, TRANSPOSE, LET, LAMBDA, ISOMITTED,
MAP, REDUCE, SCAN, BYROW, BYCOL and MAKEARRAY. The full library is added by WP-09.

Scripts: `pnpm lint`, `pnpm typecheck`, `pnpm test` (700+ tests), `pnpm bench` (a 100,000-cell
dependent chain recalculates in about 0.2 s; the target is under 1.5 s).

Licence: AGPL-3.0-or-later.
