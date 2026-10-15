/**
 * Static precedent extraction: which cells, ranges and names a formula reads.
 *
 * Defined names are expanded (a formula using `Sales` depends on what `Sales` refers to) and also
 * recorded so redefining the name re-binds its users. Structured references resolve through the
 * host's tables. Dynamic references (OFFSET, INDIRECT) cannot be known statically; those functions
 * are volatile, so their cells recalculate every time.
 */
import { type Node, type SheetPrefix, walk } from '../parser/ast.ts';
import { addressBounds } from '../parser/references.ts';
import { ErrorValue } from '../values/errors.ts';
import type { Area } from '../values/types.ts';
import { getFunction, normalizeFunctionName } from './functions/registry.ts';
import type { Precedents } from './graph.ts';
import type { TableDef } from './host.ts';
import { resolveStructured } from './structured.ts';

export interface PrecedentHost {
  sheetId(name: string): number | undefined;
  sheetsBetween(a: number, b: number): number[] | undefined;
  lookupName(name: string, sheet: number): { ast: Node; scopeSheet: number | null } | undefined;
  getTable(name: string): TableDef | undefined;
  getTableAt(sheet: number, row: number, col: number): TableDef | undefined;
}

const MAX_NAME_DEPTH = 16;

export function addressBoundsOf(node: Extract<Node, { type: 'ref' }>, sheet: number): Area {
  return { sheet, ...addressBounds(node.address) };
}

export function collectPrecedents(
  ast: Node,
  sheet: number,
  row: number,
  col: number,
  host: PrecedentHost,
): Precedents {
  const p: Precedents = {
    cells: [],
    ranges: [],
    names: [],
    volatile: false,
    usesTables: false,
    unresolvedSheets: false,
  };
  const seenNames = new Set<string>();

  const sheetsOf = (prefix: SheetPrefix | undefined, ctxSheet: number): number[] | undefined => {
    if (!prefix) return [ctxSheet];
    if (prefix.workbook !== undefined) return [];
    const a = host.sheetId(prefix.sheet);
    if (a === undefined) return undefined;
    if (prefix.sheet2 === undefined) return [a];
    const b = host.sheetId(prefix.sheet2);
    if (b === undefined) return undefined;
    return host.sheetsBetween(a, b);
  };

  const addTable = (
    t: TableDef | undefined,
    specials: readonly string[] = [],
    thisRow = false,
    columns: readonly string[] | null = null,
  ) => {
    p.usesTables = true;
    if (!t) return;
    const a = resolveStructured(t, specials, thisRow, columns, { sheet, row });
    if (a instanceof ErrorValue) return;
    if (a.r0 === a.r1 && a.c0 === a.c1) p.cells.push({ sheet: a.sheet, row: a.r0, col: a.c0 });
    else p.ranges.push(a);
  };

  const addName = (name: string, prefix: SheetPrefix | undefined, ctxSheet: number, depth: number) => {
    p.names.push(name);
    let scopeSheet = ctxSheet;
    if (prefix) {
      const id = host.sheetId(prefix.sheet);
      if (id === undefined) {
        p.unresolvedSheets = true;
        return;
      }
      scopeSheet = id;
    }
    const key = `${scopeSheet}|${name.toUpperCase()}`;
    if (seenNames.has(key)) return;
    seenNames.add(key);
    const def = host.lookupName(name, scopeSheet);
    if (def) {
      if (depth < MAX_NAME_DEPTH) visit(def.ast, def.scopeSheet ?? ctxSheet, depth + 1);
      return;
    }
    if (!prefix) {
      const t = host.getTable(name);
      if (t) addTable(t);
    }
  };

  const visit = (root: Node, ctxSheet: number, depth: number): void => {
    walk(root, (n) => {
      switch (n.type) {
        case 'ref': {
          const sheets = sheetsOf(n.prefix, ctxSheet);
          if (!sheets) {
            p.unresolvedSheets = true;
            return false;
          }
          const b = addressBounds(n.address);
          for (const s of sheets) {
            if (n.address.kind === 'cell') p.cells.push({ sheet: s, row: b.r0, col: b.c0 });
            else p.ranges.push({ sheet: s, ...b });
          }
          return false;
        }
        case 'name':
          addName(n.name, n.prefix, ctxSheet, depth);
          return false;
        case 'structured':
          addTable(
            n.table ? host.getTable(n.table) : host.getTableAt(sheet, row, col),
            n.specials,
            n.thisRow,
            n.columns,
          );
          return false;
        case 'call': {
          const desc = getFunction(n.name);
          if (desc?.volatile) p.volatile = true;
          if (!desc) addName(normalizeFunctionName(n.name), undefined, ctxSheet, depth);
          return undefined;
        }
        default:
          return undefined;
      }
    });
  };

  visit(ast, sheet, 0);
  return p;
}
