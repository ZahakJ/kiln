/**
 * @officina/formula — Officina's own spreadsheet formula engine.
 *
 * - `parser`: tokenizer, Pratt parser, AST with source spans, printer, reference rewriting
 *   (insert/delete rows and columns, copy, move, rename, A1↔R1C1, F4 toggle).
 * - `values`: the value model (errors, blanks, arrays, lambdas), coercion and comparison rules,
 *   15-digit number handling and date serials.
 * - `engine`: the dependency graph, incremental recalculation, dynamic arrays, the function registry
 *   and the `Engine` API.
 */
export const PACKAGE_NAME = '@officina/formula' as const;

export * from './engine/index.ts';
export * from './parser/index.ts';
export * from './values/index.ts';
