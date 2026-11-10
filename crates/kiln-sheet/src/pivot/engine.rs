// SPDX-License-Identifier: AGPL-3.0-or-later
//! Aggregation: the records that pass the filters grouped into a tree of items per axis, with an
//! accumulator for every pair of row and column nodes (leaves, subtotals and the grand totals),
//! and the value a report cell shows, "Show Values As" included.

use std::cmp::Ordering;

use kiln_formula::values::ErrorValue;
use rustc_hash::FxHashMap;

use super::cache::{ItemKey, PivotCache};
use super::{
    AxisItem, BaseItem, PivotFilterOp, PivotFunction, PivotLabelFilter, PivotSort, PivotTable,
    PivotValueFilter, ShowAs,
};
use crate::value::CellValue;

/// Running statistics of the values that fall into one report cell; any summary function and
/// any subtotal follow from it, and two of them merge.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Acc {
    /// Non-empty values.
    pub count: u32,
    /// Numbers.
    pub nums: u32,
    pub sum: f64,
    mean: f64,
    m2: f64,
    pub min: f64,
    pub max: f64,
    product: f64,
    /// The first error value met (sums and averages show it, as Excel does).
    error: Option<ErrorValue>,
}

impl Default for Acc {
    fn default() -> Acc {
        Acc {
            count: 0,
            nums: 0,
            sum: 0.0,
            mean: 0.0,
            m2: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            product: 1.0,
            error: None,
        }
    }
}

impl Acc {
    pub fn add(&mut self, v: &CellValue) {
        match v {
            CellValue::Blank => {}
            CellValue::Number(n) => {
                self.count += 1;
                self.nums += 1;
                self.sum += n;
                let d = n - self.mean;
                self.mean += d / f64::from(self.nums);
                self.m2 += d * (n - self.mean);
                self.min = self.min.min(*n);
                self.max = self.max.max(*n);
                self.product *= n;
            }
            CellValue::Error(e) => {
                self.count += 1;
                self.error.get_or_insert(*e);
            }
            other => {
                if !other.is_empty_text_or_blank() {
                    self.count += 1;
                }
            }
        }
    }

    #[cfg(test)]
    pub fn merge(&mut self, o: &Acc) {
        if o.count == 0 && o.error.is_none() {
            return;
        }
        let n = f64::from(self.nums);
        let m = f64::from(o.nums);
        if o.nums > 0 {
            let d = o.mean - self.mean;
            let total = n + m;
            self.m2 += o.m2 + d * d * n * m / total;
            self.mean += d * m / total;
        }
        self.count += o.count;
        self.nums += o.nums;
        self.sum += o.sum;
        self.min = self.min.min(o.min);
        self.max = self.max.max(o.max);
        self.product *= o.product;
        if self.error.is_none() {
            self.error = o.error;
        }
    }

    /// The summary as a number (or an error).
    pub fn result(&self, f: PivotFunction) -> Result<f64, ErrorValue> {
        if !matches!(f, PivotFunction::Count | PivotFunction::CountNums)
            && let Some(e) = self.error
        {
            return Err(e);
        }
        let n = f64::from(self.nums);
        Ok(match f {
            PivotFunction::Sum => self.sum,
            PivotFunction::Count => f64::from(self.count),
            PivotFunction::CountNums => n,
            PivotFunction::Average => {
                if self.nums == 0 {
                    return Err(ErrorValue::Div0);
                }
                self.sum / n
            }
            PivotFunction::Max => {
                if self.nums == 0 {
                    0.0
                } else {
                    self.max
                }
            }
            PivotFunction::Min => {
                if self.nums == 0 {
                    0.0
                } else {
                    self.min
                }
            }
            PivotFunction::Product => {
                if self.nums == 0 {
                    0.0
                } else {
                    self.product
                }
            }
            PivotFunction::Var | PivotFunction::StdDev => {
                if self.nums < 2 {
                    return Err(ErrorValue::Div0);
                }
                let v = (self.m2 / (n - 1.0)).max(0.0);
                if f == PivotFunction::Var { v } else { v.sqrt() }
            }
            PivotFunction::VarP | PivotFunction::StdDevP => {
                if self.nums == 0 {
                    return Err(ErrorValue::Div0);
                }
                let v = (self.m2 / n).max(0.0);
                if f == PivotFunction::VarP {
                    v
                } else {
                    v.sqrt()
                }
            }
        })
    }
}

/// One node of an axis tree: the root (the grand total), a group, or an item at the last level.
#[derive(Debug, Clone)]
pub(crate) struct Node {
    pub parent: u32,
    /// The item (index into the level field's cache items); unused at the root.
    pub item: u32,
    /// 0 at the root; `k` for the k-th field level.
    pub depth: u8,
    /// Children in display order.
    pub children: Vec<u32>,
}

/// The items of one axis as a tree.
#[derive(Debug, Clone)]
pub(crate) struct Tree {
    /// The source fields of the levels (Σ Values left out).
    pub levels: Vec<usize>,
    pub nodes: Vec<Node>,
    /// (parent, item) → node.
    index: FxHashMap<(u32, u32), u32>,
}

impl Tree {
    fn new(levels: Vec<usize>) -> Tree {
        Tree {
            levels,
            nodes: vec![Node {
                parent: 0,
                item: 0,
                depth: 0,
                children: Vec::new(),
            }],
            index: FxHashMap::default(),
        }
    }

    fn child(&mut self, parent: u32, item: u32) -> u32 {
        if let Some(&n) = self.index.get(&(parent, item)) {
            return n;
        }
        let id = u32::try_from(self.nodes.len()).unwrap_or(u32::MAX);
        let depth = self.nodes[parent as usize].depth + 1;
        self.nodes.push(Node {
            parent,
            item,
            depth,
            children: Vec::new(),
        });
        self.nodes[parent as usize].children.push(id);
        self.index.insert((parent, item), id);
        id
    }

    /// The path of a record: the root, then one node per level.
    fn chain(&mut self, cache: &PivotCache, record: usize, out: &mut Vec<u32>) {
        out.clear();
        out.push(0);
        let mut at = 0;
        for &f in &self.levels.clone() {
            let item = cache.fields[f].index.get(record).copied().unwrap_or(0);
            at = self.child(at, item);
            out.push(at);
        }
    }

    /// The node's items from the first level down.
    pub fn path(&self, mut node: u32) -> Vec<u32> {
        let mut out = Vec::new();
        while node != 0 {
            let n = &self.nodes[node as usize];
            out.push(n.item);
            node = n.parent;
        }
        out.reverse();
        out
    }

    /// The node of a path, if the report has it.
    pub fn find(&self, path: &[u32]) -> Option<u32> {
        let mut at = 0;
        for &item in path {
            at = *self.index.get(&(at, item))?;
        }
        Some(at)
    }

    /// The ancestor of a node at a depth (the node itself at its own depth).
    pub fn ancestor(&self, mut node: u32, depth: u8) -> u32 {
        while self.nodes[node as usize].depth > depth {
            node = self.nodes[node as usize].parent;
        }
        node
    }
}

/// The order of items: numbers, then text (without case), logicals, errors; blanks last.
pub fn compare_items(a: &CellValue, b: &CellValue) -> Ordering {
    fn rank(v: &CellValue) -> u8 {
        match v {
            CellValue::Number(_) => 0,
            CellValue::Text(_) | CellValue::RichText(_) if !v.is_empty_text_or_blank() => 1,
            CellValue::Bool(_) => 2,
            CellValue::Error(_) => 3,
            _ => 4,
        }
    }
    match (a, b) {
        (CellValue::Number(x), CellValue::Number(y)) => x.total_cmp(y),
        (CellValue::Bool(x), CellValue::Bool(y)) => x.cmp(y),
        _ if rank(a) == 1 && rank(b) == 1 => {
            let x = a.as_text().unwrap_or_default().to_lowercase();
            let y = b.as_text().unwrap_or_default().to_lowercase();
            x.cmp(&y)
        }
        _ => rank(a).cmp(&rank(b)),
    }
}

/// Whether text matches a pattern with `*` and `?` (case-insensitive).
fn wildcard(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Whether an item passes a label filter.
pub(crate) fn label_passes(f: &PivotLabelFilter, item: &CellValue) -> bool {
    use PivotFilterOp as O;
    let label = item.display_general();
    let num = |s: &str| s.trim().parse::<f64>().ok();
    let cmp = |s: &str| -> Ordering {
        match (item.as_number(), num(s)) {
            (Some(a), Some(b)) => a.total_cmp(&b),
            _ => label.to_lowercase().cmp(&s.to_lowercase()),
        }
    };
    match f.op {
        O::Equal => wildcard(&f.value, &label),
        O::NotEqual => !wildcard(&f.value, &label),
        O::BeginsWith => wildcard(&format!("{}*", f.value), &label),
        O::NotBeginsWith => !wildcard(&format!("{}*", f.value), &label),
        O::EndsWith => wildcard(&format!("*{}", f.value), &label),
        O::NotEndsWith => !wildcard(&format!("*{}", f.value), &label),
        O::Contains => wildcard(&format!("*{}*", f.value), &label),
        O::NotContains => !wildcard(&format!("*{}*", f.value), &label),
        O::GreaterThan => cmp(&f.value) == Ordering::Greater,
        O::GreaterThanOrEqual => cmp(&f.value) != Ordering::Less,
        O::LessThan => cmp(&f.value) == Ordering::Less,
        O::LessThanOrEqual => cmp(&f.value) != Ordering::Greater,
        O::Between => cmp(&f.value) != Ordering::Less && cmp(&f.value2) != Ordering::Greater,
        O::NotBetween => cmp(&f.value) == Ordering::Less || cmp(&f.value2) == Ordering::Greater,
    }
}

fn number_passes(op: PivotFilterOp, x: f64, a: f64, b: f64) -> bool {
    use PivotFilterOp as O;
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    match op {
        O::Equal => x == a,
        O::NotEqual => x != a,
        O::GreaterThan => x > a,
        O::GreaterThanOrEqual => x >= a,
        O::LessThan => x < a,
        O::LessThanOrEqual => x <= a,
        O::Between => (lo..=hi).contains(&x),
        O::NotBetween => !(lo..=hi).contains(&x),
        _ => true,
    }
}

/// The groups, totals and accumulators of a PivotTable over its cache.
pub(crate) struct Summary<'a> {
    pub table: &'a PivotTable,
    pub rows: Tree,
    pub cols: Tree,
    /// (row node, column node) → slot in `accs` (one accumulator per value field there).
    slots: FxHashMap<(u32, u32), u32>,
    accs: Vec<Acc>,
    /// Records that pass every filter.
    pub included: Vec<bool>,
}

fn fields_of(axis: &[AxisItem]) -> Vec<usize> {
    axis.iter()
        .filter_map(|a| match a {
            AxisItem::Field(f) => Some(*f),
            AxisItem::Values => None,
        })
        .collect()
}

impl<'a> Summary<'a> {
    pub fn build(table: &'a PivotTable) -> Summary<'a> {
        let cache = &*table.cache;
        let n = cache.len();
        let mut included = vec![true; n];
        // Items excluded by the field filters (hidden items, a single page item, label filters)
        // of every field the report uses.
        let mut placed: Vec<usize> = fields_of(&table.rows);
        placed.extend(fields_of(&table.cols));
        placed.extend(table.pages.iter().map(|p| p.field));
        for &f in &placed {
            let (Some(field), Some(cf)) = (table.fields.get(f), cache.fields.get(f)) else {
                continue;
            };
            let page_item = table
                .pages
                .iter()
                .find(|p| p.field == f)
                .and_then(|p| p.item.as_ref())
                .map(ItemKey::of);
            let hidden: Vec<ItemKey> = field.hidden.iter().map(ItemKey::of).collect();
            if hidden.is_empty() && page_item.is_none() && field.label_filter.is_none() {
                continue;
            }
            let on_axis = table.rows.contains(&AxisItem::Field(f))
                || table.cols.contains(&AxisItem::Field(f));
            let ok: Vec<bool> = cf
                .items
                .iter()
                .map(|it| {
                    let k = ItemKey::of(it);
                    if hidden.contains(&k) {
                        return false;
                    }
                    if let Some(p) = &page_item
                        && *p != k
                    {
                        return false;
                    }
                    match &field.label_filter {
                        Some(lf) if on_axis => label_passes(lf, it),
                        _ => true,
                    }
                })
                .collect();
            for (r, inc) in included.iter_mut().enumerate() {
                if *inc && !ok.get(cf.index[r] as usize).copied().unwrap_or(true) {
                    *inc = false;
                }
            }
        }
        let mut s = Summary::aggregate(table, included);
        // Value filters, outermost row field first, then the column fields: each keeps the
        // items whose totals pass among their siblings, and the totals follow.
        for (axis_is_rows, f) in fields_of(&table.rows)
            .into_iter()
            .map(|f| (true, f))
            .chain(fields_of(&table.cols).into_iter().map(|f| (false, f)))
        {
            let Some(vf) = table.fields[f].value_filter.clone() else {
                continue;
            };
            if s.apply_value_filter(axis_is_rows, f, &vf) {
                let included = std::mem::take(&mut s.included);
                s = Summary::aggregate(table, included);
            }
        }
        s.sort();
        s
    }

    fn aggregate(table: &'a PivotTable, included: Vec<bool>) -> Summary<'a> {
        let cache = &*table.cache;
        let mut rows = Tree::new(fields_of(&table.rows));
        let mut cols = Tree::new(fields_of(&table.cols));
        let nv = table.values.len().max(1);
        let mut slots: FxHashMap<(u32, u32), u32> = FxHashMap::default();
        let mut accs: Vec<Acc> = Vec::new();
        let (mut rc, mut cc) = (Vec::new(), Vec::new());
        let value_fields: Vec<Option<&super::cache::CacheField>> = table
            .values
            .iter()
            .map(|v| cache.fields.get(v.field))
            .collect();
        for (r, inc) in included.iter().enumerate() {
            if !inc {
                continue;
            }
            rows.chain(cache, r, &mut rc);
            cols.chain(cache, r, &mut cc);
            for &a in &rc {
                for &b in &cc {
                    let next = u32::try_from(accs.len() / nv).unwrap_or(u32::MAX);
                    let slot = *slots.entry((a, b)).or_insert_with(|| {
                        accs.extend(std::iter::repeat_n(Acc::default(), nv));
                        next
                    }) as usize;
                    for (k, vf) in value_fields.iter().enumerate() {
                        if let Some(vf) = vf {
                            accs[slot * nv + k].add(vf.value(r));
                        }
                    }
                }
            }
        }
        Summary {
            table,
            rows,
            cols,
            slots,
            accs,
            included,
        }
    }

    /// Marks the records of the items a value filter removes; whether any were.
    fn apply_value_filter(&mut self, rows: bool, field: usize, vf: &PivotValueFilter) -> bool {
        let tree = if rows { &self.rows } else { &self.cols };
        let Some(level) = tree.levels.iter().position(|&f| f == field) else {
            return false;
        };
        let depth = u8::try_from(level + 1).unwrap_or(u8::MAX);
        let value = match vf {
            PivotValueFilter::Top { value, .. } | PivotValueFilter::Compare { value, .. } => *value,
        };
        let Some(spec) = self.table.values.get(value) else {
            return false;
        };
        let total = |node: u32| -> Option<f64> {
            let acc = if rows {
                self.acc(node, 0, value)
            } else {
                self.acc(0, node, value)
            }?;
            acc.result(spec.function).ok()
        };
        let mut removed: Vec<u32> = Vec::new();
        // Siblings: the nodes at `depth` grouped by parent.
        let parents: Vec<u32> = (0..tree.nodes.len() as u32)
            .filter(|&n| tree.nodes[n as usize].depth + 1 == depth)
            .collect();
        for p in parents {
            let kids = &tree.nodes[p as usize].children;
            match vf {
                PivotValueFilter::Top { count, bottom, .. } => {
                    let mut ranked: Vec<(u32, f64)> = kids
                        .iter()
                        .map(|&k| (k, total(k).unwrap_or(f64::NEG_INFINITY)))
                        .collect();
                    ranked.sort_by(|a, b| {
                        if *bottom {
                            a.1.total_cmp(&b.1)
                        } else {
                            b.1.total_cmp(&a.1)
                        }
                    });
                    removed.extend(ranked.iter().skip(*count as usize).map(|x| x.0));
                }
                PivotValueFilter::Compare { op, a, b, .. } => {
                    removed.extend(
                        kids.iter()
                            .filter(|&&k| !total(k).is_some_and(|x| number_passes(*op, x, *a, *b)))
                            .copied(),
                    );
                }
            }
        }
        if removed.is_empty() {
            return false;
        }
        let cache = &*self.table.cache;
        let removed_paths: Vec<Vec<u32>> = removed.iter().map(|&n| tree.path(n)).collect();
        let levels = tree.levels[..=level].to_vec();
        for (r, inc) in self.included.iter_mut().enumerate() {
            if !*inc {
                continue;
            }
            let path: Vec<u32> = levels
                .iter()
                .map(|&f| cache.fields[f].index.get(r).copied().unwrap_or(0))
                .collect();
            if removed_paths.contains(&path) {
                *inc = false;
            }
        }
        true
    }

    /// Orders every node's children by its level field's sort.
    fn sort(&mut self) {
        for rows in [true, false] {
            let tree = if rows { &self.rows } else { &self.cols };
            let mut orders: Vec<(u32, Vec<u32>)> = Vec::new();
            for (id, node) in tree.nodes.iter().enumerate() {
                if node.children.is_empty() {
                    continue;
                }
                let level = usize::from(node.depth);
                let Some(&f) = tree.levels.get(level) else {
                    continue;
                };
                let field = &self.table.fields[f];
                let items = &self.table.cache.fields[f].items;
                let mut kids = node.children.clone();
                let by_value = field
                    .sort_by_value
                    .and_then(|v| self.table.values.get(v).map(|spec| (v, spec.function)));
                let key = |k: u32| -> Option<f64> {
                    let (v, func) = by_value?;
                    let acc = if rows {
                        self.acc(k, 0, v)
                    } else {
                        self.acc(0, k, v)
                    }?;
                    acc.result(func).ok()
                };
                match (field.sort, by_value) {
                    (PivotSort::Source, _) => {
                        kids.sort_by_key(|&k| tree.nodes[k as usize].item);
                    }
                    (s, Some(_)) => {
                        kids.sort_by(|&a, &b| {
                            let o = key(a)
                                .unwrap_or(f64::NEG_INFINITY)
                                .total_cmp(&key(b).unwrap_or(f64::NEG_INFINITY));
                            if s == PivotSort::Descending {
                                o.reverse()
                            } else {
                                o
                            }
                        });
                    }
                    (s, None) => {
                        kids.sort_by(|&a, &b| {
                            let ia = &items[tree.nodes[a as usize].item as usize];
                            let ib = &items[tree.nodes[b as usize].item as usize];
                            let o = compare_items(ia, ib);
                            // Blanks stay last either way.
                            let blank = |v: &CellValue| v.is_empty_text_or_blank();
                            if s == PivotSort::Descending && !blank(ia) && !blank(ib) {
                                o.reverse()
                            } else {
                                o
                            }
                        });
                    }
                }
                orders.push((u32::try_from(id).unwrap_or(0), kids));
            }
            let tree = if rows { &mut self.rows } else { &mut self.cols };
            for (id, kids) in orders {
                tree.nodes[id as usize].children = kids;
            }
        }
    }

    /// The accumulator of a value at (row node, column node).
    pub fn acc(&self, row: u32, col: u32, value: usize) -> Option<&Acc> {
        let nv = self.table.values.len().max(1);
        let slot = *self.slots.get(&(row, col))? as usize;
        self.accs.get(slot * nv + value)
    }

    /// What a report cell shows for a value at (row node, column node), summarised with
    /// `function` (a subtotal's own function, or the value's).
    pub fn cell(&self, row: u32, col: u32, value: usize, function: PivotFunction) -> CellValue {
        let Some(spec) = self.table.values.get(value) else {
            return CellValue::Blank;
        };
        let raw = |r: u32, c: u32| -> Option<Result<f64, ErrorValue>> {
            Some(self.acc(r, c, value)?.result(function))
        };
        let Some(own) = raw(row, col) else {
            return CellValue::Blank;
        };
        let x = match own {
            Ok(x) => x,
            Err(e) => return CellValue::Error(e),
        };
        let ratio = |den: Option<Result<f64, ErrorValue>>| -> CellValue {
            match den {
                Some(Ok(d)) if d != 0.0 => CellValue::number(x / d),
                Some(Err(e)) => CellValue::Error(e),
                _ => CellValue::Error(ErrorValue::Div0),
            }
        };
        let rn = &self.rows.nodes[row as usize];
        let cn = &self.cols.nodes[col as usize];
        match spec.show_as {
            ShowAs::Normal => CellValue::number(x),
            ShowAs::PercentOfTotal => ratio(raw(0, 0)),
            ShowAs::PercentOfColumn => ratio(raw(0, col)),
            ShowAs::PercentOfRow => ratio(raw(row, 0)),
            ShowAs::PercentOfParentRow => {
                if row == 0 {
                    CellValue::number(1.0)
                } else {
                    ratio(raw(rn.parent, col))
                }
            }
            ShowAs::PercentOfParentColumn => {
                if col == 0 {
                    CellValue::number(1.0)
                } else {
                    ratio(raw(row, cn.parent))
                }
            }
            ShowAs::Index => {
                let g = raw(0, 0);
                let rt = raw(row, 0);
                let ct = raw(0, col);
                match (g, rt, ct) {
                    (Some(Ok(g)), Some(Ok(rt)), Some(Ok(ct))) if rt * ct != 0.0 => {
                        CellValue::number(x * g / (rt * ct))
                    }
                    _ => CellValue::Error(ErrorValue::Div0),
                }
            }
            ShowAs::Percent | ShowAs::Difference | ShowAs::PercentDifference => {
                self.compare_with_base(row, col, value, function, x)
            }
            ShowAs::RunningTotal | ShowAs::PercentRunningTotal => {
                self.running(row, col, value, function, spec.show_as)
            }
            ShowAs::RankAscending | ShowAs::RankDescending => self.rank(
                row,
                col,
                value,
                function,
                spec.show_as == ShowAs::RankDescending,
                x,
            ),
        }
    }

    /// The axis and level of a value's base field: (rows?, depth of the level).
    fn base_level(&self, value: usize) -> Option<(bool, u8)> {
        let f = self.table.values.get(value)?.base_field?;
        if let Some(l) = self.rows.levels.iter().position(|&x| x == f) {
            return Some((true, u8::try_from(l + 1).ok()?));
        }
        let l = self.cols.levels.iter().position(|&x| x == f)?;
        Some((false, u8::try_from(l + 1).ok()?))
    }

    /// The node standing for the base item in place of `node`'s ancestor at `depth`.
    fn base_node(&self, rows: bool, node: u32, depth: u8, base: &BaseItem) -> Option<u32> {
        let tree = if rows { &self.rows } else { &self.cols };
        let at = tree.ancestor(node, depth);
        let parent = tree.nodes[at as usize].parent;
        let sibs = &tree.nodes[parent as usize].children;
        let pos = sibs.iter().position(|&s| s == at)?;
        let target = match base {
            BaseItem::Previous => *sibs.get(pos.checked_sub(1)?)?,
            BaseItem::Next => *sibs.get(pos + 1)?,
            BaseItem::Item(v) => {
                let f = tree.levels[usize::from(depth) - 1];
                let item = self.table.cache.fields[f].find(v)?;
                *sibs
                    .iter()
                    .find(|&&s| tree.nodes[s as usize].item == item)?
            }
        };
        if target == at {
            return Some(node);
        }
        // The same path below the base item.
        let mut path = tree.path(node);
        path[usize::from(depth) - 1] = tree.nodes[target as usize].item;
        tree.find(&path)
    }

    fn compare_with_base(
        &self,
        row: u32,
        col: u32,
        value: usize,
        function: PivotFunction,
        x: f64,
    ) -> CellValue {
        let spec = &self.table.values[value];
        let Some((rows, depth)) = self.base_level(value) else {
            return CellValue::Error(ErrorValue::NA);
        };
        let node = if rows { row } else { col };
        let tree = if rows { &self.rows } else { &self.cols };
        if tree.nodes[node as usize].depth < depth {
            // Totals above the base field's level show nothing.
            return CellValue::Blank;
        }
        let at = tree.ancestor(node, depth);
        let is_base = match &spec.base_item {
            BaseItem::Item(v) => {
                let f = tree.levels[usize::from(depth) - 1];
                self.table.cache.fields[f].find(v) == Some(tree.nodes[at as usize].item)
            }
            _ => false,
        };
        if is_base {
            return match spec.show_as {
                ShowAs::Percent => CellValue::number(1.0),
                _ => CellValue::Blank,
            };
        }
        let Some(b) = self.base_node(rows, node, depth, &spec.base_item) else {
            return match spec.base_item {
                BaseItem::Item(_) => CellValue::Error(ErrorValue::NA),
                _ => CellValue::Blank,
            };
        };
        let base = if rows {
            self.acc(b, col, value)
        } else {
            self.acc(row, b, value)
        };
        let Some(base) = base else {
            return CellValue::Error(ErrorValue::NA);
        };
        match base.result(function) {
            Err(e) => CellValue::Error(e),
            Ok(bv) => match spec.show_as {
                ShowAs::Difference => CellValue::number(x - bv),
                ShowAs::Percent if bv != 0.0 => CellValue::number(x / bv),
                ShowAs::PercentDifference if bv != 0.0 => CellValue::number((x - bv) / bv),
                _ => CellValue::Error(ErrorValue::Div0),
            },
        }
    }

    fn running(
        &self,
        row: u32,
        col: u32,
        value: usize,
        function: PivotFunction,
        how: ShowAs,
    ) -> CellValue {
        let Some((rows, depth)) = self.base_level(value) else {
            return CellValue::Error(ErrorValue::NA);
        };
        let node = if rows { row } else { col };
        let tree = if rows { &self.rows } else { &self.cols };
        if tree.nodes[node as usize].depth < depth {
            return CellValue::Blank;
        }
        let at = tree.ancestor(node, depth);
        let parent = tree.nodes[at as usize].parent;
        let sibs = &tree.nodes[parent as usize].children;
        let path = tree.path(node);
        let mut sum = 0.0;
        let mut whole = 0.0;
        let mut upto = 0.0;
        let mut passed = false;
        for &s in sibs {
            let mut p = path.clone();
            p[usize::from(depth) - 1] = tree.nodes[s as usize].item;
            let v = tree
                .find(&p)
                .and_then(|n| {
                    if rows {
                        self.acc(n, col, value)
                    } else {
                        self.acc(row, n, value)
                    }
                })
                .map(|a| a.result(function));
            match v {
                Some(Err(e)) => return CellValue::Error(e),
                Some(Ok(v)) => {
                    whole += v;
                    if !passed {
                        sum += v;
                    }
                }
                None => {}
            }
            if s == at {
                upto = sum;
                passed = true;
            }
        }
        if how == ShowAs::RunningTotal {
            CellValue::number(upto)
        } else if whole != 0.0 {
            CellValue::number(upto / whole)
        } else {
            CellValue::Error(ErrorValue::Div0)
        }
    }

    fn rank(
        &self,
        row: u32,
        col: u32,
        value: usize,
        function: PivotFunction,
        largest_first: bool,
        x: f64,
    ) -> CellValue {
        let Some((rows, depth)) = self.base_level(value) else {
            return CellValue::Error(ErrorValue::NA);
        };
        let node = if rows { row } else { col };
        let tree = if rows { &self.rows } else { &self.cols };
        if tree.nodes[node as usize].depth < depth {
            return CellValue::Blank;
        }
        let at = tree.ancestor(node, depth);
        let parent = tree.nodes[at as usize].parent;
        let path = tree.path(node);
        let mut better = 0u32;
        for &s in &tree.nodes[parent as usize].children {
            let mut p = path.clone();
            p[usize::from(depth) - 1] = tree.nodes[s as usize].item;
            let v = tree
                .find(&p)
                .and_then(|n| {
                    if rows {
                        self.acc(n, col, value)
                    } else {
                        self.acc(row, n, value)
                    }
                })
                .and_then(|a| a.result(function).ok());
            if let Some(v) = v
                && (if largest_first { v > x } else { v < x })
            {
                better += 1;
            }
        }
        CellValue::number(f64::from(better + 1))
    }

    /// The records behind a report cell (row node, column node), in source order.
    pub fn records(&self, row: u32, col: u32) -> Vec<usize> {
        let cache = &*self.table.cache;
        let rp = self.rows.path(row);
        let cp = self.cols.path(col);
        let rl = &self.rows.levels[..rp.len()];
        let cl = &self.cols.levels[..cp.len()];
        (0..cache.len())
            .filter(|&r| {
                self.included[r]
                    && rl
                        .iter()
                        .zip(&rp)
                        .all(|(&f, &item)| cache.fields[f].index[r] == item)
                    && cl
                        .iter()
                        .zip(&cp)
                        .all(|(&f, &item)| cache.fields[f].index[r] == item)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accumulators_merge_like_one_pass() {
        let xs = [3.0, 1.5, -2.0, 8.0, 4.25];
        let mut one = Acc::default();
        for x in xs {
            one.add(&CellValue::Number(x));
        }
        let mut a = Acc::default();
        let mut b = Acc::default();
        for x in &xs[..2] {
            a.add(&CellValue::Number(*x));
        }
        for x in &xs[2..] {
            b.add(&CellValue::Number(*x));
        }
        a.merge(&b);
        for f in PivotFunction::ALL {
            let (p, q) = (one.result(f), a.result(f));
            match (p, q) {
                (Ok(p), Ok(q)) => assert!((p - q).abs() < 1e-9, "{f:?}: {p} vs {q}"),
                (p, q) => assert_eq!(p, q, "{f:?}"),
            }
        }
        let var = one.result(PivotFunction::Var).unwrap_or(0.0);
        let mean = xs.iter().sum::<f64>() / 5.0;
        let expect = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / 4.0;
        assert!((var - expect).abs() < 1e-12);
    }

    #[test]
    fn text_counts_but_does_not_sum() {
        let mut a = Acc::default();
        a.add(&CellValue::text("x"));
        a.add(&CellValue::Number(2.0));
        a.add(&CellValue::Blank);
        assert_eq!(a.result(PivotFunction::Count), Ok(2.0));
        assert_eq!(a.result(PivotFunction::CountNums), Ok(1.0));
        assert_eq!(a.result(PivotFunction::Sum), Ok(2.0));
        a.add(&CellValue::Error(ErrorValue::NA));
        assert_eq!(a.result(PivotFunction::Sum), Err(ErrorValue::NA));
        assert_eq!(a.result(PivotFunction::Count), Ok(3.0));
    }

    #[test]
    fn wildcards_and_items_order() {
        assert!(wildcard("ea*", "East"));
        assert!(wildcard("*s?", "West"));
        assert!(!wildcard("n*", "East"));
        let mut v = [
            CellValue::Blank,
            CellValue::text("b"),
            CellValue::Number(10.0),
            CellValue::text("A"),
            CellValue::Number(2.0),
            CellValue::Bool(true),
        ];
        v.sort_by(compare_items);
        let shown: Vec<String> = v.iter().map(CellValue::display_general).collect();
        assert_eq!(shown, ["2", "10", "A", "b", "TRUE", ""]);
    }
}
