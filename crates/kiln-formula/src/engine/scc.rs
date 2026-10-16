// SPDX-License-Identifier: AGPL-3.0-or-later
//! Iterative Tarjan strongly-connected components over the cells Kahn's algorithm could not
//! order (cycles and everything downstream of them).

use rustc_hash::FxHashMap;

use super::graph::FormulaId;

/// Components of the subgraph induced by `nodes`, in topological order (a component comes before
/// everything that depends on it). `edges(n)` lists the dependents of `n`.
pub(crate) fn tarjan<'a>(
    nodes: &[FormulaId],
    edges: impl Fn(FormulaId) -> &'a [FormulaId],
) -> Vec<Vec<FormulaId>> {
    #[derive(Clone, Copy)]
    struct Info {
        index: u32,
        low: u32,
        on_stack: bool,
    }
    const UNVISITED: u32 = u32::MAX;
    let mut info: FxHashMap<FormulaId, Info> = nodes
        .iter()
        .map(|&n| {
            (
                n,
                Info {
                    index: UNVISITED,
                    low: 0,
                    on_stack: false,
                },
            )
        })
        .collect();
    let mut next_index = 0u32;
    let mut stack: Vec<FormulaId> = Vec::new();
    let mut out: Vec<Vec<FormulaId>> = Vec::new();
    let mut work: Vec<(FormulaId, usize)> = Vec::new();
    for &root in nodes {
        if info[&root].index != UNVISITED {
            continue;
        }
        if let Some(i) = info.get_mut(&root) {
            *i = Info {
                index: next_index,
                low: next_index,
                on_stack: true,
            };
        }
        next_index += 1;
        stack.push(root);
        work.push((root, 0));
        while let Some(&mut (v, ref mut edge)) = work.last_mut() {
            let list = edges(v);
            if *edge < list.len() {
                let w = list[*edge];
                *edge += 1;
                let Some(wi) = info.get(&w).copied() else {
                    continue;
                };
                if wi.index == UNVISITED {
                    if let Some(i) = info.get_mut(&w) {
                        *i = Info {
                            index: next_index,
                            low: next_index,
                            on_stack: true,
                        };
                    }
                    next_index += 1;
                    stack.push(w);
                    work.push((w, 0));
                } else if wi.on_stack {
                    let vi = info[&v];
                    if wi.index < vi.low
                        && let Some(i) = info.get_mut(&v)
                    {
                        i.low = wi.index;
                    }
                }
                continue;
            }
            work.pop();
            let vi = info[&v];
            if let Some(&(parent, _)) = work.last() {
                let pl = info[&parent].low;
                if vi.low < pl
                    && let Some(i) = info.get_mut(&parent)
                {
                    i.low = vi.low;
                }
            }
            if vi.low == vi.index {
                let mut comp = Vec::new();
                while let Some(w) = stack.pop() {
                    if let Some(i) = info.get_mut(&w) {
                        i.on_stack = false;
                    }
                    comp.push(w);
                    if w == v {
                        break;
                    }
                }
                out.push(comp);
            }
        }
    }
    // Tarjan emits sinks first; edges point from precedent to dependent, so reverse.
    out.reverse();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_cycles_in_topological_order() {
        // 1 → 2 → 3 → 2, 3 → 4; 5 alone.
        let adj: FxHashMap<u32, Vec<u32>> = [
            (1, vec![2]),
            (2, vec![3]),
            (3, vec![2, 4]),
            (4, vec![]),
            (5, vec![]),
        ]
        .into_iter()
        .collect();
        let comps = tarjan(&[1, 2, 3, 4, 5], |n| adj[&n].as_slice());
        let pos = |x: u32| comps.iter().position(|c| c.contains(&x)).unwrap_or(99);
        assert!(pos(1) < pos(2));
        assert_eq!(pos(2), pos(3));
        assert!(pos(3) < pos(4));
        assert_eq!(comps.iter().map(Vec::len).sum::<usize>(), 5);
    }
}
