// SPDX-License-Identifier: AGPL-3.0-or-later
//! A dynamic interval tree over rectangles: a treap keyed by the first row, augmented with the
//! largest last row of each subtree. Row-interval queries prune whole subtrees; columns are checked
//! per hit. Used for range precedents (one entry per distinct range, so `SUM(A:A)` is one node,
//! not a million edges) and for spill areas.

use crate::refs::Bounds;

const NIL: u32 = u32::MAX;

#[derive(Debug, Clone)]
struct TNode {
    rect: Bounds,
    id: u32,
    prio: u32,
    max_r1: u32,
    left: u32,
    right: u32,
}

/// Rectangles with a `u32` payload id (unique within the index).
#[derive(Debug, Clone)]
pub(crate) struct RectIndex {
    nodes: Vec<TNode>,
    free: Vec<u32>,
    root: u32,
    count: usize,
    rng: u64,
}

impl Default for RectIndex {
    fn default() -> RectIndex {
        RectIndex {
            nodes: Vec::new(),
            free: Vec::new(),
            root: NIL,
            count: 0,
            rng: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

impl RectIndex {
    /// Number of entries.
    pub(crate) fn len(&self) -> usize {
        self.count
    }

    fn next_prio(&mut self) -> u32 {
        // xorshift64*: deterministic, so recalculation order never depends on randomness.
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        (x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    fn less(&self, a: &TNode, r0: u32, id: u32) -> bool {
        a.rect.r0 < r0 || (a.rect.r0 == r0 && a.id < id)
    }

    fn update(&mut self, n: u32) {
        let (l, r, own) = {
            let node = &self.nodes[n as usize];
            (node.left, node.right, node.rect.r1)
        };
        let mut m = own;
        if l != NIL {
            m = m.max(self.nodes[l as usize].max_r1);
        }
        if r != NIL {
            m = m.max(self.nodes[r as usize].max_r1);
        }
        self.nodes[n as usize].max_r1 = m;
    }

    /// Adds an entry.
    pub(crate) fn insert(&mut self, rect: Bounds, id: u32) {
        let prio = self.next_prio();
        let node = TNode {
            rect,
            id,
            prio,
            max_r1: rect.r1,
            left: NIL,
            right: NIL,
        };
        let idx = match self.free.pop() {
            Some(i) => {
                self.nodes[i as usize] = node;
                i
            }
            None => {
                self.nodes.push(node);
                (self.nodes.len() - 1) as u32
            }
        };
        self.root = self.insert_at(self.root, idx);
        self.count += 1;
    }

    fn insert_at(&mut self, n: u32, new: u32) -> u32 {
        if n == NIL {
            return new;
        }
        let (r0, id) = {
            let x = &self.nodes[new as usize];
            (x.rect.r0, x.id)
        };
        let go_left = {
            let cur = &self.nodes[n as usize];
            !(self.less(cur, r0, id))
        };
        let mut n = n;
        if go_left {
            let l = self.insert_at(self.nodes[n as usize].left, new);
            self.nodes[n as usize].left = l;
            if self.nodes[l as usize].prio > self.nodes[n as usize].prio {
                n = self.rotate_right(n);
            }
        } else {
            let r = self.insert_at(self.nodes[n as usize].right, new);
            self.nodes[n as usize].right = r;
            if self.nodes[r as usize].prio > self.nodes[n as usize].prio {
                n = self.rotate_left(n);
            }
        }
        self.update(n);
        n
    }

    /// Removes the entry with this first row and id. Returns whether it was found.
    pub(crate) fn remove(&mut self, r0: u32, id: u32) -> bool {
        let before = self.count;
        self.root = self.remove_at(self.root, r0, id);
        self.count < before
    }

    fn remove_at(&mut self, n: u32, r0: u32, id: u32) -> u32 {
        if n == NIL {
            return NIL;
        }
        let (nid, nr0, left, right) = {
            let x = &self.nodes[n as usize];
            (x.id, x.rect.r0, x.left, x.right)
        };
        if nid == id && nr0 == r0 {
            self.count -= 1;
            self.free.push(n);
            return self.merge(left, right);
        }
        if nr0 > r0 || (nr0 == r0 && nid > id) {
            let l = self.remove_at(left, r0, id);
            self.nodes[n as usize].left = l;
        } else {
            let r = self.remove_at(right, r0, id);
            self.nodes[n as usize].right = r;
        }
        self.update(n);
        n
    }

    fn merge(&mut self, a: u32, b: u32) -> u32 {
        if a == NIL {
            return b;
        }
        if b == NIL {
            return a;
        }
        if self.nodes[a as usize].prio > self.nodes[b as usize].prio {
            let r = self.merge(self.nodes[a as usize].right, b);
            self.nodes[a as usize].right = r;
            self.update(a);
            a
        } else {
            let l = self.merge(a, self.nodes[b as usize].left);
            self.nodes[b as usize].left = l;
            self.update(b);
            b
        }
    }

    fn rotate_right(&mut self, n: u32) -> u32 {
        let l = self.nodes[n as usize].left;
        self.nodes[n as usize].left = self.nodes[l as usize].right;
        self.nodes[l as usize].right = n;
        self.update(n);
        self.update(l);
        l
    }

    fn rotate_left(&mut self, n: u32) -> u32 {
        let r = self.nodes[n as usize].right;
        self.nodes[n as usize].right = self.nodes[r as usize].left;
        self.nodes[r as usize].left = n;
        self.update(n);
        self.update(r);
        r
    }

    /// Calls `cb(id, rect)` for every entry containing (row, col).
    pub(crate) fn stab(&self, row: u32, col: u32, cb: &mut impl FnMut(u32, &Bounds)) {
        self.query(self.root, row, row, col, col, cb);
    }

    /// Calls `cb(id, rect)` for every entry overlapping `r`.
    pub(crate) fn overlaps(&self, r: &Bounds, cb: &mut impl FnMut(u32, &Bounds)) {
        self.query(self.root, r.r0, r.r1, r.c0, r.c1, cb);
    }

    fn query(
        &self,
        mut n: u32,
        r0: u32,
        r1: u32,
        c0: u32,
        c1: u32,
        cb: &mut impl FnMut(u32, &Bounds),
    ) {
        while n != NIL {
            let node = &self.nodes[n as usize];
            if node.max_r1 < r0 {
                return;
            }
            if node.left != NIL {
                self.query(node.left, r0, r1, c0, c1, cb);
            }
            let e = &node.rect;
            if e.r0 > r1 {
                return;
            }
            if e.r1 >= r0 && e.c0 <= c1 && e.c1 >= c0 {
                cb(node.id, e);
            }
            n = node.right;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self, lo: u32, hi: u32) -> u32 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            lo + ((self.0 >> 33) % u64::from(hi - lo + 1)) as u32
        }
    }

    #[test]
    fn stab_and_overlap_match_brute_force() {
        let mut rng = Lcg(42);
        let mut idx = RectIndex::default();
        let mut live: Vec<(u32, Bounds)> = Vec::new();
        for i in 0..2000u32 {
            let r0 = rng.next(0, 1000);
            let c0 = rng.next(0, 50);
            let b = Bounds {
                r0,
                c0,
                r1: r0 + rng.next(0, 200),
                c1: c0 + rng.next(0, 10),
            };
            idx.insert(b, i);
            live.push((i, b));
            if rng.next(0, 9) < 3 {
                let v = rng.next(0, live.len() as u32 - 1) as usize;
                let (id, b) = live.remove(v);
                assert!(idx.remove(b.r0, id));
            }
        }
        assert_eq!(idx.len(), live.len());
        for _ in 0..300 {
            let row = rng.next(0, 1200);
            let col = rng.next(0, 60);
            let mut got = Vec::new();
            idx.stab(row, col, &mut |id, _| got.push(id));
            got.sort_unstable();
            let mut want: Vec<u32> = live
                .iter()
                .filter(|(_, b)| b.contains(row, col))
                .map(|(i, _)| *i)
                .collect();
            want.sort_unstable();
            assert_eq!(got, want);
            let q = Bounds {
                r0: row,
                r1: row + rng.next(0, 50),
                c0: col,
                c1: col + rng.next(0, 5),
            };
            let mut got2 = Vec::new();
            idx.overlaps(&q, &mut |id, _| got2.push(id));
            got2.sort_unstable();
            let mut want2: Vec<u32> = live
                .iter()
                .filter(|(_, b)| b.overlaps(&q))
                .map(|(i, _)| *i)
                .collect();
            want2.sort_unstable();
            assert_eq!(got2, want2);
        }
    }

    #[test]
    fn removing_an_unknown_entry_is_a_no_op() {
        let mut idx = RectIndex::default();
        let b = Bounds {
            r0: 0,
            c0: 0,
            r1: 1,
            c1: 1,
        };
        idx.insert(b, 1);
        assert!(!idx.remove(0, 2));
        assert_eq!(idx.len(), 1);
    }
}
