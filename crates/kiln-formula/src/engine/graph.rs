// SPDX-License-Identifier: AGPL-3.0-or-later
//! The dependency graph.
//!
//! Nodes are formula cells. A precedent that is a single cell is an edge from that cell's key; a
//! precedent range is one range node stored in a per-sheet interval tree, so a formula over `A:A`
//! costs one entry and "who depends on C7?" is a stab query. Dynamic-array anchors also claim their
//! spill rectangle so cells reading inside a spill depend on the anchor.

use std::sync::Arc;

use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use super::rect_index::RectIndex;
use super::types::{Area, CellKey, SheetId, cell_key, unpack_key};
use crate::refs::Bounds;

/// Index of a formula cell in the engine's arena.
pub(crate) type FormulaId = u32;

/// What a formula reads, found statically.
#[derive(Debug, Default, Clone)]
pub(crate) struct Precedents {
    pub(crate) cells: Vec<CellKey>,
    pub(crate) ranges: Vec<Area>,
    /// Upper-cased names (defined names, tables, named lambdas) the formula uses.
    pub(crate) names: Vec<String>,
    pub(crate) volatile: bool,
    pub(crate) uses_tables: bool,
    pub(crate) unresolved_sheets: bool,
}

/// The graph entries one formula holds (kept on the formula so it can be removed).
#[derive(Debug, Default, Clone)]
pub(crate) struct Registration {
    pub(crate) cells: Box<[CellKey]>,
    pub(crate) ranges: Box<[u32]>,
    pub(crate) names: Box<[Arc<str>]>,
}

#[derive(Debug, Clone)]
struct RangeNode {
    sheet: SheetId,
    rect: Bounds,
    dependents: Vec<FormulaId>,
}

#[derive(Debug, Default)]
pub(crate) struct Graph {
    cell_deps: FxHashMap<CellKey, SmallVec<[FormulaId; 2]>>,
    ranges: Vec<Option<RangeNode>>,
    range_free: Vec<u32>,
    range_by_key: FxHashMap<(SheetId, Bounds), u32>,
    range_index: FxHashMap<SheetId, RectIndex>,
    name_deps: FxHashMap<Arc<str>, Vec<FormulaId>>,
    claims: FxHashMap<SheetId, RectIndex>,
    claim_count: usize,
}

impl Graph {
    pub(crate) fn range_node_count(&self) -> usize {
        self.range_by_key.len()
    }

    pub(crate) fn cell_edge_count(&self) -> usize {
        self.cell_deps.values().map(SmallVec::len).sum()
    }

    pub(crate) fn has_claims(&self) -> bool {
        self.claim_count > 0
    }

    /// Registers a formula's precedents.
    pub(crate) fn add(&mut self, fc: FormulaId, p: &Precedents) -> Registration {
        let mut cells: Vec<CellKey> = p.cells.clone();
        cells.sort_unstable();
        cells.dedup();
        for &k in &cells {
            self.cell_deps.entry(k).or_default().push(fc);
        }
        let mut areas = p.ranges.clone();
        areas.sort_unstable_by_key(|a| {
            (a.sheet, a.bounds.r0, a.bounds.c0, a.bounds.r1, a.bounds.c1)
        });
        areas.dedup();
        let mut ranges = Vec::with_capacity(areas.len());
        for a in areas {
            let id = match self.range_by_key.get(&(a.sheet, a.bounds)) {
                Some(&id) => id,
                None => {
                    let node = RangeNode {
                        sheet: a.sheet,
                        rect: a.bounds,
                        dependents: Vec::new(),
                    };
                    let id = match self.range_free.pop() {
                        Some(i) => {
                            self.ranges[i as usize] = Some(node);
                            i
                        }
                        None => {
                            self.ranges.push(Some(node));
                            (self.ranges.len() - 1) as u32
                        }
                    };
                    self.range_by_key.insert((a.sheet, a.bounds), id);
                    self.range_index
                        .entry(a.sheet)
                        .or_default()
                        .insert(a.bounds, id);
                    id
                }
            };
            if let Some(node) = self.ranges[id as usize].as_mut() {
                node.dependents.push(fc);
            }
            ranges.push(id);
        }
        let mut names: Vec<Arc<str>> = Vec::with_capacity(p.names.len());
        let mut sorted = p.names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        for n in sorted {
            let key: Arc<str> = match self.name_deps.get_key_value(n.as_str()) {
                Some((k, _)) => k.clone(),
                None => Arc::from(n.as_str()),
            };
            self.name_deps.entry(key.clone()).or_default().push(fc);
            names.push(key);
        }
        Registration {
            cells: cells.into_boxed_slice(),
            ranges: ranges.into_boxed_slice(),
            names: names.into_boxed_slice(),
        }
    }

    /// Removes a formula's precedents.
    pub(crate) fn remove(&mut self, fc: FormulaId, reg: &Registration) {
        for k in reg.cells.iter() {
            if let Some(list) = self.cell_deps.get_mut(k) {
                if let Some(i) = list.iter().position(|&x| x == fc) {
                    list.swap_remove(i);
                }
                if list.is_empty() {
                    self.cell_deps.remove(k);
                }
            }
        }
        for &id in reg.ranges.iter() {
            let Some(node) = self.ranges.get_mut(id as usize).and_then(Option::as_mut) else {
                continue;
            };
            if let Some(i) = node.dependents.iter().position(|&x| x == fc) {
                node.dependents.swap_remove(i);
            }
            if node.dependents.is_empty() {
                let (sheet, rect) = (node.sheet, node.rect);
                self.ranges[id as usize] = None;
                self.range_free.push(id);
                self.range_by_key.remove(&(sheet, rect));
                if let Some(idx) = self.range_index.get_mut(&sheet) {
                    idx.remove(rect.r0, id);
                }
            }
        }
        for n in reg.names.iter() {
            if let Some(list) = self.name_deps.get_mut(n) {
                if let Some(i) = list.iter().position(|&x| x == fc) {
                    list.swap_remove(i);
                }
                if list.is_empty() {
                    self.name_deps.remove(n);
                }
            }
        }
    }

    /// Drops every edge and claim (before a full rebuild).
    pub(crate) fn clear(&mut self) {
        *self = Graph::default();
    }

    pub(crate) fn formulas_using_name(&self, upper_name: &str) -> Vec<FormulaId> {
        self.name_deps.get(upper_name).cloned().unwrap_or_default()
    }

    /// Replaces the spill claim of the anchor `fc` on `sheet`.
    pub(crate) fn set_claim(
        &mut self,
        sheet: SheetId,
        fc: FormulaId,
        old: Option<Bounds>,
        new: Option<Bounds>,
    ) {
        if let Some(o) = old
            && let Some(idx) = self.claims.get_mut(&sheet)
            && idx.remove(o.r0, fc)
        {
            self.claim_count -= 1;
        }
        if let Some(n) = new {
            self.claims.entry(sheet).or_default().insert(n, fc);
            self.claim_count += 1;
        }
    }

    /// Anchors whose spill claim covers a cell, other than an anchor at that cell itself.
    pub(crate) fn claims_at(
        &self,
        sheet: SheetId,
        row: u32,
        col: u32,
        cb: &mut impl FnMut(FormulaId, &Bounds),
    ) {
        if self.claim_count == 0 {
            return;
        }
        if let Some(idx) = self.claims.get(&sheet) {
            idx.stab(row, col, &mut |id, rect| {
                if rect.r0 != row || rect.c0 != col {
                    cb(id, rect);
                }
            });
        }
    }

    /// Anchors whose claim overlaps a rectangle.
    pub(crate) fn claims_overlapping(
        &self,
        sheet: SheetId,
        rect: &Bounds,
        cb: &mut impl FnMut(FormulaId, &Bounds),
    ) {
        if self.claim_count == 0 {
            return;
        }
        if let Some(idx) = self.claims.get(&sheet) {
            idx.overlaps(rect, cb);
        }
    }

    /// Formulas that must recalculate when the value at (sheet, row, col) changes.
    pub(crate) fn dependents_at(
        &self,
        sheet: SheetId,
        row: u32,
        col: u32,
        out: &mut Vec<FormulaId>,
    ) {
        if let Some(list) = self.cell_deps.get(&cell_key(sheet, row, col)) {
            out.extend_from_slice(list);
        }
        if let Some(idx) = self.range_index.get(&sheet)
            && idx.len() > 0
        {
            idx.stab(row, col, &mut |id, _| {
                if let Some(node) = self.ranges.get(id as usize).and_then(Option::as_ref) {
                    out.extend_from_slice(&node.dependents);
                }
            });
        }
        self.claims_at(sheet, row, col, &mut |id, _| out.push(id));
    }

    /// Formulas reading any cell of a rectangle (single-cell edges and overlapping ranges).
    pub(crate) fn dependents_in_rect(
        &self,
        sheet: SheetId,
        rect: &Bounds,
        out: &mut Vec<FormulaId>,
    ) {
        if rect.cell_count() <= self.cell_deps.len() as u64 {
            for r in rect.r0..=rect.r1 {
                for c in rect.c0..=rect.c1 {
                    if let Some(list) = self.cell_deps.get(&cell_key(sheet, r, c)) {
                        out.extend_from_slice(list);
                    }
                }
            }
        } else {
            for (&k, list) in &self.cell_deps {
                let (s, r, c) = unpack_key(k);
                if s == sheet && rect.contains(r, c) {
                    out.extend_from_slice(list);
                }
            }
        }
        if let Some(idx) = self.range_index.get(&sheet)
            && idx.len() > 0
        {
            idx.overlaps(rect, &mut |id, _| {
                if let Some(node) = self.ranges.get(id as usize).and_then(Option::as_ref) {
                    out.extend_from_slice(&node.dependents);
                }
            });
        }
    }

    /// Direct dependents of a formula cell: readers of its cell and of its spill area.
    pub(crate) fn dependents_of(
        &self,
        sheet: SheetId,
        row: u32,
        col: u32,
        claim: Option<&Bounds>,
        out: &mut Vec<FormulaId>,
    ) {
        self.dependents_at(sheet, row, col, out);
        if let Some(c) = claim {
            self.dependents_in_rect(sheet, c, out);
        }
    }
}
