/**
 * Iterative Tarjan strongly-connected components over the cells Kahn's algorithm could not order.
 * Returns components in topological order (a component comes before everything that depends on it).
 */
import type { FormulaCell } from './graph.ts';

export function tarjan(nodes: FormulaCell[]): FormulaCell[][] {
  const inSet = new Set(nodes);
  let index = 0;
  const stack: FormulaCell[] = [];
  const out: FormulaCell[][] = [];
  for (const n of nodes) n.index = -1;
  for (const root of nodes) {
    if (root.index !== -1) continue;
    const work: { node: FormulaCell; edge: number }[] = [{ node: root, edge: 0 }];
    root.index = root.low = index++;
    stack.push(root);
    root.onStack = true;
    while (work.length > 0) {
      const frame = work[work.length - 1] as { node: FormulaCell; edge: number };
      const v = frame.node;
      const edges = v.out ?? [];
      if (frame.edge < edges.length) {
        const w = edges[frame.edge++] as FormulaCell;
        if (!inSet.has(w)) continue;
        if (w.index === -1) {
          w.index = w.low = index++;
          stack.push(w);
          w.onStack = true;
          work.push({ node: w, edge: 0 });
        } else if (w.onStack && w.index < v.low) {
          v.low = w.index;
        }
        continue;
      }
      work.pop();
      const parent = work[work.length - 1];
      if (parent && v.low < parent.node.low) parent.node.low = v.low;
      if (v.low === v.index) {
        const comp: FormulaCell[] = [];
        let w: FormulaCell;
        do {
          w = stack.pop() as FormulaCell;
          w.onStack = false;
          comp.push(w);
        } while (w !== v);
        out.push(comp);
      }
    }
  }
  // Tarjan emits sinks first; edges point from precedent to dependent, so reverse.
  return out.reverse();
}
