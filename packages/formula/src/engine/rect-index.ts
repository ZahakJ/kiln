/**
 * A dynamic interval tree over rectangles: a treap keyed by the first row, augmented with the
 * maximum last row of each subtree. Row-interval queries prune whole subtrees; columns are checked
 * per hit. Used for range precedents (one entry per distinct range, so `SUM(A:A)` is a single
 * node, not a million edges) and for spill areas.
 */

export interface Rect {
  r0: number;
  c0: number;
  r1: number;
  c1: number;
}

interface TreapNode<T> {
  entry: T & Rect;
  id: number;
  prio: number;
  maxR1: number;
  left: TreapNode<T> | null;
  right: TreapNode<T> | null;
}

let nextId = 1;
const ids = new WeakMap<object, number>();

function idOf(o: object): number {
  let id = ids.get(o);
  if (id === undefined) {
    id = nextId++;
    ids.set(o, id);
  }
  return id;
}

export class RectIndex<T extends Rect> {
  private root: TreapNode<T> | null = null;
  private count = 0;

  get size(): number {
    return this.count;
  }

  insert(entry: T): void {
    const node: TreapNode<T> = {
      entry,
      id: idOf(entry),
      prio: Math.random(),
      maxR1: entry.r1,
      left: null,
      right: null,
    };
    this.root = this.insertAt(this.root, node);
    this.count++;
  }

  /** Removes an entry (by identity). Its rectangle must not have changed since insertion. */
  remove(entry: T): boolean {
    const before = this.count;
    this.root = this.removeAt(this.root, entry.r0, idOf(entry));
    return this.count < before;
  }

  /** Calls `cb` for every entry containing (row, col). */
  stab(row: number, col: number, cb: (e: T) => void): void {
    this.query(this.root, row, row, col, col, cb);
  }

  /** Calls `cb` for every entry overlapping the rectangle. */
  overlaps(r: Rect, cb: (e: T) => void): void {
    this.query(this.root, r.r0, r.r1, r.c0, r.c1, cb);
  }

  /** All entries (in row order). */
  entries(): T[] {
    const out: T[] = [];
    const visit = (n: TreapNode<T> | null) => {
      if (!n) return;
      visit(n.left);
      out.push(n.entry);
      visit(n.right);
    };
    visit(this.root);
    return out;
  }

  private query(
    n: TreapNode<T> | null,
    r0: number,
    r1: number,
    c0: number,
    c1: number,
    cb: (e: T) => void,
  ): void {
    while (n) {
      if (n.maxR1 < r0) return;
      if (n.left) this.query(n.left, r0, r1, c0, c1, cb);
      const e = n.entry;
      if (e.r0 > r1) return; // right subtree starts even later
      if (e.r1 >= r0 && e.c0 <= c1 && e.c1 >= c0) cb(e);
      n = n.right;
    }
  }

  private static update<T>(n: TreapNode<T>): void {
    let m = n.entry.r1;
    if (n.left && n.left.maxR1 > m) m = n.left.maxR1;
    if (n.right && n.right.maxR1 > m) m = n.right.maxR1;
    n.maxR1 = m;
  }

  private static less<T>(a: TreapNode<T>, r0: number, id: number): boolean {
    return a.entry.r0 < r0 || (a.entry.r0 === r0 && a.id < id);
  }

  private insertAt(n: TreapNode<T> | null, node: TreapNode<T>): TreapNode<T> {
    if (!n) return node;
    if (RectIndex.less(node, n.entry.r0, n.id)) {
      n.left = this.insertAt(n.left, node);
      if (n.left.prio > n.prio) n = this.rotateRight(n);
    } else {
      n.right = this.insertAt(n.right, node);
      if (n.right.prio > n.prio) n = this.rotateLeft(n);
    }
    RectIndex.update(n);
    return n;
  }

  private removeAt(n: TreapNode<T> | null, r0: number, id: number): TreapNode<T> | null {
    if (!n) return null;
    if (n.id === id) {
      this.count--;
      return this.merge(n.left, n.right);
    }
    if (n.entry.r0 > r0 || (n.entry.r0 === r0 && n.id > id)) n.left = this.removeAt(n.left, r0, id);
    else n.right = this.removeAt(n.right, r0, id);
    RectIndex.update(n);
    return n;
  }

  private merge(a: TreapNode<T> | null, b: TreapNode<T> | null): TreapNode<T> | null {
    if (!a) return b;
    if (!b) return a;
    if (a.prio > b.prio) {
      a.right = this.merge(a.right, b);
      RectIndex.update(a);
      return a;
    }
    b.left = this.merge(a, b.left);
    RectIndex.update(b);
    return b;
  }

  private rotateRight(n: TreapNode<T>): TreapNode<T> {
    const l = n.left as TreapNode<T>;
    n.left = l.right;
    l.right = n;
    RectIndex.update(n);
    RectIndex.update(l);
    return l;
  }

  private rotateLeft(n: TreapNode<T>): TreapNode<T> {
    const r = n.right as TreapNode<T>;
    n.right = r.left;
    r.left = n;
    RectIndex.update(n);
    RectIndex.update(r);
    return r;
  }
}
