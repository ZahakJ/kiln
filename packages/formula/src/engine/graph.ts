/**
 * The dependency graph.
 *
 * Nodes are formula cells. A precedent that is a single cell is an edge from that cell key; a
 * precedent range is one {@link RangeNode} stored in a per-sheet interval tree, so a formula over
 * `A:A` costs one entry and "who depends on C7?" is a stab query. Dynamic-array anchors also claim
 * their spill rectangle so cells reading inside a spill depend on the anchor.
 *
 * Recalculation marks dirty cells transitively, orders the dirty subgraph with Kahn's algorithm, and
 * hands what Kahn cannot order (cycles and everything downstream of them) to Tarjan's strongly
 * connected components so cycles can be flagged (or iterated) and the rest evaluated in order.
 */
import type { Node } from '../parser/ast.ts';
import type { Area, Value } from '../values/types.ts';
import { BLANK } from '../values/types.ts';
import { cellKey } from './host.ts';
import { type Rect, RectIndex } from './rect-index.ts';

export interface RangeNode extends Rect {
  sheet: number;
  key: string;
  dependents: Set<FormulaCell>;
}

export interface SpillClaim extends Rect {
  anchor: FormulaCell;
}

export class FormulaCell {
  /** Formula text including the leading `=`. */
  text: string;
  ast: Node;
  legacy: boolean;
  /** Result: a scalar, or the full array for a dynamic-array anchor. */
  value: Value = BLANK;
  /** Spill state of an array result. */
  spill: 'none' | 'ok' | 'blocked' = 'none';
  claim: SpillClaim | null = null;
  dirty = false;
  evaluating = false;
  volatile = false;
  circular = false;
  // graph bookkeeping
  precCells: number[] = [];
  precRanges: RangeNode[] = [];
  names: string[] = [];
  usesTables = false;
  /** Unresolved sheet names referenced (bound when such a sheet appears). */
  unresolvedSheets = false;
  // scratch for Kahn/Tarjan
  indeg = 0;
  out: FormulaCell[] | null = null;
  index = -1;
  low = 0;
  onStack = false;

  constructor(
    public sheet: number,
    public row: number,
    public col: number,
    text: string,
    ast: Node,
    legacy: boolean,
  ) {
    this.text = text;
    this.ast = ast;
    this.legacy = legacy;
  }

  get key(): number {
    return cellKey(this.sheet, this.row, this.col);
  }
}

export interface Precedents {
  cells: { sheet: number; row: number; col: number }[];
  ranges: Area[];
  names: string[];
  volatile: boolean;
  usesTables: boolean;
  unresolvedSheets: boolean;
}

export class DependencyGraph {
  private readonly cellDeps = new Map<number, Set<FormulaCell>>();
  private readonly ranges = new Map<string, RangeNode>();
  private readonly rangeIndex = new Map<number, RectIndex<RangeNode>>();
  private readonly nameDeps = new Map<string, Set<FormulaCell>>();
  readonly claims = new Map<number, RectIndex<SpillClaim>>();

  /** Number of distinct range nodes (for diagnostics and tests). */
  get rangeNodeCount(): number {
    return this.ranges.size;
  }

  /** Number of single-cell edges. */
  get cellEdgeCount(): number {
    let n = 0;
    for (const s of this.cellDeps.values()) n += s.size;
    return n;
  }

  add(fc: FormulaCell, p: Precedents): void {
    fc.volatile = p.volatile;
    fc.usesTables = p.usesTables;
    fc.unresolvedSheets = p.unresolvedSheets;
    for (const c of p.cells) {
      const k = cellKey(c.sheet, c.row, c.col);
      let set = this.cellDeps.get(k);
      if (!set) {
        set = new Set();
        this.cellDeps.set(k, set);
      }
      set.add(fc);
      fc.precCells.push(k);
    }
    for (const a of p.ranges) {
      const key = `${a.sheet}:${a.r0}:${a.c0}:${a.r1}:${a.c1}`;
      let node = this.ranges.get(key);
      if (!node) {
        node = { sheet: a.sheet, r0: a.r0, c0: a.c0, r1: a.r1, c1: a.c1, key, dependents: new Set() };
        this.ranges.set(key, node);
        let idx = this.rangeIndex.get(a.sheet);
        if (!idx) {
          idx = new RectIndex();
          this.rangeIndex.set(a.sheet, idx);
        }
        idx.insert(node);
      }
      node.dependents.add(fc);
      fc.precRanges.push(node);
    }
    for (const n of p.names) {
      const u = n.toUpperCase();
      let set = this.nameDeps.get(u);
      if (!set) {
        set = new Set();
        this.nameDeps.set(u, set);
      }
      set.add(fc);
      fc.names.push(u);
    }
  }

  remove(fc: FormulaCell, keepClaim = false): void {
    for (const k of fc.precCells) {
      const set = this.cellDeps.get(k);
      if (set) {
        set.delete(fc);
        if (set.size === 0) this.cellDeps.delete(k);
      }
    }
    for (const node of fc.precRanges) {
      node.dependents.delete(fc);
      if (node.dependents.size === 0) {
        this.ranges.delete(node.key);
        this.rangeIndex.get(node.sheet)?.remove(node);
      }
    }
    for (const n of fc.names) {
      const set = this.nameDeps.get(n);
      if (set) {
        set.delete(fc);
        if (set.size === 0) this.nameDeps.delete(n);
      }
    }
    fc.precCells = [];
    fc.precRanges = [];
    fc.names = [];
    if (!keepClaim) this.setClaim(fc, null);
  }

  formulasUsingName(name: string): FormulaCell[] {
    return [...(this.nameDeps.get(name.toUpperCase()) ?? [])];
  }

  /** Replaces the spill claim of an anchor. */
  setClaim(fc: FormulaCell, rect: Rect | null): void {
    if (fc.claim) {
      this.claims.get(fc.sheet)?.remove(fc.claim);
      fc.claim = null;
    }
    if (rect) {
      const claim: SpillClaim = { r0: rect.r0, c0: rect.c0, r1: rect.r1, c1: rect.c1, anchor: fc };
      let idx = this.claims.get(fc.sheet);
      if (!idx) {
        idx = new RectIndex();
        this.claims.set(fc.sheet, idx);
      }
      idx.insert(claim);
      fc.claim = claim;
    }
  }

  /** Anchors whose spill claim covers a cell (other than an anchor at that cell itself). */
  claimsAt(sheet: number, row: number, col: number, cb: (c: SpillClaim) => void): void {
    const idx = this.claims.get(sheet);
    if (!idx || idx.size === 0) return;
    idx.stab(row, col, (c) => {
      if (c.anchor.row !== row || c.anchor.col !== col) cb(c);
    });
  }

  claimsOverlapping(sheet: number, rect: Rect, cb: (c: SpillClaim) => void): void {
    const idx = this.claims.get(sheet);
    if (!idx || idx.size === 0) return;
    idx.overlaps(rect, cb);
  }

  /** Formulas that must recalculate when the value at (sheet,row,col) changes. */
  dependentsAt(sheet: number, row: number, col: number, out: FormulaCell[]): void {
    const set = this.cellDeps.get(cellKey(sheet, row, col));
    if (set) for (const d of set) out.push(d);
    const idx = this.rangeIndex.get(sheet);
    if (idx && idx.size > 0) {
      idx.stab(row, col, (node) => {
        for (const d of node.dependents) out.push(d);
      });
    }
    this.claimsAt(sheet, row, col, (c) => out.push(c.anchor));
  }

  /** Formulas reading any cell of a rectangle (single-cell edges and overlapping ranges). */
  dependentsInRect(sheet: number, rect: Rect, out: FormulaCell[]): void {
    const cells = (rect.r1 - rect.r0 + 1) * (rect.c1 - rect.c0 + 1);
    if (cells <= this.cellDeps.size) {
      for (let r = rect.r0; r <= rect.r1; r++) {
        for (let c = rect.c0; c <= rect.c1; c++) {
          const set = this.cellDeps.get(cellKey(sheet, r, c));
          if (set) for (const d of set) out.push(d);
        }
      }
    } else {
      const lo = cellKey(sheet, rect.r0, 0);
      const hi = cellKey(sheet, rect.r1, 16_383);
      for (const [k, set] of this.cellDeps) {
        if (k < lo || k > hi) continue;
        const col = k % 16_384;
        if (col < rect.c0 || col > rect.c1) continue;
        for (const d of set) out.push(d);
      }
    }
    const idx = this.rangeIndex.get(sheet);
    if (idx && idx.size > 0) {
      idx.overlaps(rect, (node) => {
        for (const d of node.dependents) out.push(d);
      });
    }
  }

  /** Direct dependents of a formula cell: readers of its cell and of its spill area. */
  dependentsOf(fc: FormulaCell, out: FormulaCell[]): void {
    this.dependentsAt(fc.sheet, fc.row, fc.col, out);
    const claim = fc.claim;
    if (claim) {
      this.dependentsInRect(fc.sheet, claim, out);
    }
  }
}
