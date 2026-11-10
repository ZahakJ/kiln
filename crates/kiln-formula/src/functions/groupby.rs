// SPDX-License-Identifier: AGPL-3.0-or-later
//! `GROUPBY` and `PIVOTBY`: summaries of a range by the distinct values of one or more fields,
//! aggregated with a function (`SUM`, `AVERAGE`, `PERCENTOF`, any one-parameter or two-parameter
//! LAMBDA).
//!
//! The data is grouped into a tree, one level per field, outermost first. Each level is sorted
//! (by its items ascending unless `sort_order` says otherwise; value columns sort groups by their
//! aggregate), and totals are added at the depth `total_depth` asks for: 1 a grand total, 2 and
//! more subtotals of the outer levels too, negative values put them above their group. With
//! `field_relationship` 1 (table) the combinations are listed flat with no subtotals.
//!
//! A two-parameter function (`PERCENTOF`) gets the group's values and the values of the reference
//! group: every value for `GROUPBY`, and for `PIVOTBY` the one `relative_to` names.

use std::cmp::Ordering;
use std::sync::Arc;

use rustc_hash::FxHashMap;

use super::support::*;
use crate::engine::{Arg, CallContext, EvalValue, Lambda};
use crate::registry::FunctionSpec;
use crate::values::{Array2D, ErrorValue, Value, lookup_equals, sort_compare, to_bool};

/// The label of grand total rows and columns.
const TOTAL: &str = "Total";

/// An empty cell of the result (labels left out and empty intersections show nothing, not 0).
fn empty() -> Value {
    Value::text("")
}

fn item_hash(v: &Value) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = rustc_hash::FxHasher::default();
    match v {
        Value::Str(s) => {
            1u8.hash(&mut h);
            s.to_lowercase().hash(&mut h);
        }
        Value::Number(n) => {
            2u8.hash(&mut h);
            super::lookup::round15_fast(*n).to_bits().hash(&mut h);
        }
        Value::Bool(b) => {
            3u8.hash(&mut h);
            b.hash(&mut h);
        }
        Value::Error(e) => {
            4u8.hash(&mut h);
            e.code().hash(&mut h);
        }
        _ => 5u8.hash(&mut h),
    }
    h.finish()
}

/// The function argument: one lambda taking one or two parameters (an eta-reduced `SUM` is one).
fn function_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Arc<Lambda>> {
    match value_at(args, i, ctx) {
        EvalValue::Value(Value::Lambda(l)) if l.accepts(1) || l.accepts(2) => Ok(l),
        EvalValue::Value(Value::Error(e)) => Err(e),
        _ => Err(ErrorValue::Value),
    }
}

fn array_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Arc<Array2D>> {
    let v = value_at(args, i, ctx);
    if let EvalValue::Value(Value::Error(e)) = v {
        return Err(e);
    }
    ctx.to_array(v)
}

/// An optional integer argument (`None` when omitted or left empty).
fn opt_int(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Option<i64>> {
    if omitted(args, i) {
        return Ok(None);
    }
    let x = num(args, i, ctx, 0.0)?;
    Ok(Some(x.trunc() as i64))
}

/// `sort_order`: one or more 1-based column numbers, negative for descending.
fn sort_arg(args: &mut [Arg<'_>], i: usize, ctx: &mut dyn CallContext) -> R<Option<Vec<i64>>> {
    if omitted(args, i) {
        return Ok(None);
    }
    let a = array_arg(args, i, ctx)?;
    let mut out = Vec::with_capacity(a.len());
    for v in a.values() {
        let n = number(v, ctx)?.trunc() as i64;
        if n == 0 {
            return Err(ErrorValue::Value);
        }
        out.push(n);
    }
    Ok(Some(out))
}

/// The prepared data: keys per field and values per value column, for the rows kept.
struct Data {
    /// `keys[field][row]`.
    keys: Vec<Vec<Value>>,
    /// `vals[column][row]`.
    vals: Vec<Vec<Value>>,
    /// Header names of the fields and of the value columns.
    key_names: Vec<Value>,
    val_names: Vec<Value>,
    /// The data rows kept by the filter.
    rows: Vec<usize>,
    show_headers: bool,
}

fn columns(a: &Array2D, skip: usize) -> Vec<Vec<Value>> {
    (0..a.cols())
        .map(|c| (skip..a.rows()).map(|r| a.get(r, c).clone()).collect())
        .collect()
}

/// Reads the field and value arrays, the header mode and the filter.
///
/// `field_headers`: omitted is automatic (headers when the first value is text and the second a
/// number, shown when there is more than one field or value column), 0 none, 1 present but not
/// shown, 2 none but generated, 3 present and shown.
fn prepare(
    fields: &[&Array2D],
    values: &Array2D,
    headers: Option<i64>,
    filter: Option<&Array2D>,
) -> R<(Data, Vec<usize>)> {
    let n = values.rows();
    if fields.iter().any(|f| f.rows() != n) {
        return Err(ErrorValue::Value);
    }
    let has = match headers {
        None => {
            n >= 2
                && matches!(values.get(0, 0), Value::Str(_))
                && matches!(values.get(1, 0), Value::Number(_))
        }
        Some(0 | 2) => false,
        Some(1 | 3) => true,
        Some(_) => return Err(ErrorValue::Value),
    };
    let field_cols: usize = fields.iter().map(|f| f.cols()).sum();
    let show = match headers {
        None => has && (field_cols > fields.len() || values.cols() > 1),
        Some(h) => h >= 2,
    };
    let skip = usize::from(has);
    let mut keys = Vec::new();
    let mut key_names = Vec::new();
    // `splits[i]` is where the columns of fields[i] start in `keys`.
    let mut splits = Vec::with_capacity(fields.len());
    for (fi, f) in fields.iter().enumerate() {
        splits.push(keys.len());
        for c in 0..f.cols() {
            key_names.push(if has {
                f.get(0, c).clone()
            } else if fields.len() > 1 {
                Value::text(format!(
                    "{} Field {}",
                    if fi == 0 { "Row" } else { "Column" },
                    c + 1
                ))
            } else {
                Value::text(format!("Row Field {}", c + 1))
            });
        }
        keys.extend(columns(f, skip));
    }
    let val_names = (0..values.cols())
        .map(|c| {
            if has {
                values.get(0, c).clone()
            } else {
                Value::text(format!("Value {}", c + 1))
            }
        })
        .collect();
    let vals = columns(values, skip);
    let count = n - skip;
    let rows: Vec<usize> = match filter {
        None => (0..count).collect(),
        Some(f) => {
            let flat: Vec<&Value> = if f.cols() == 1 {
                (0..f.rows()).map(|r| f.get(r, 0)).collect()
            } else if f.rows() == 1 {
                (0..f.cols()).map(|c| f.get(0, c)).collect()
            } else {
                return Err(ErrorValue::Value);
            };
            let flat = match flat.len() {
                l if l == n => &flat[skip..],
                l if l == count => &flat[..],
                _ => return Err(ErrorValue::Value),
            };
            let mut keep = Vec::new();
            for (r, v) in flat.iter().enumerate() {
                let on = match v {
                    Value::Error(e) => return Err(*e),
                    Value::Blank => false,
                    other => to_bool(other).map_err(|_| ErrorValue::Value)?,
                };
                if on {
                    keep.push(r);
                }
            }
            keep
        }
    };
    if rows.is_empty() {
        return Err(ErrorValue::Calc);
    }
    if count == 0 {
        return Err(ErrorValue::Calc);
    }
    Ok((
        Data {
            keys,
            vals,
            key_names,
            val_names,
            rows,
            show_headers: show,
        },
        splits,
    ))
}

/// A group: its item at its level and the data rows under it.
struct Node {
    item: Value,
    rows: Vec<usize>,
    children: Vec<Node>,
}

/// Splits `rows` by the distinct values of `keys`, in order of first appearance.
fn split(keys: &[Value], rows: &[usize]) -> Vec<Node> {
    const END: usize = usize::MAX;
    let mut heads: FxHashMap<u64, usize> = FxHashMap::default();
    let mut next: Vec<usize> = Vec::new();
    let mut out: Vec<Node> = Vec::new();
    for &r in rows {
        let v = &keys[r];
        let h = item_hash(v);
        let mut g = heads.get(&h).copied().unwrap_or(END);
        while g != END && !lookup_equals(&out[g].item, v) {
            g = next[g];
        }
        if g == END {
            next.push(heads.insert(h, out.len()).unwrap_or(END));
            out.push(Node {
                item: v.clone(),
                rows: vec![r],
                children: Vec::new(),
            });
        } else {
            out[g].rows.push(r);
        }
    }
    out
}

fn build(keys: &[Vec<Value>], rows: &[usize], level: usize) -> Vec<Node> {
    let mut nodes = split(&keys[level], rows);
    if level + 1 < keys.len() {
        for n in &mut nodes {
            n.children = build(keys, &n.rows, level + 1);
        }
    }
    nodes
}

/// Aggregates value column `v` over `rows`; `reference` is the second argument of a
/// two-parameter function.
fn aggregate(
    ctx: &mut dyn CallContext,
    f: &Lambda,
    vals: &[Value],
    rows: &[usize],
    reference: &[usize],
) -> Value {
    if rows.is_empty() {
        return empty();
    }
    let col = |rs: &[usize]| {
        Array2D::column(rs.iter().map(|&r| vals[r].clone()).collect()).map(Value::array)
    };
    let Some(sub) = col(rows) else {
        return Value::Error(ErrorValue::Calc);
    };
    let mut call = vec![EvalValue::Value(sub)];
    if !f.accepts(1) {
        let Some(all) = col(reference) else {
            return Value::Error(ErrorValue::Calc);
        };
        call.push(EvalValue::Value(all));
    }
    match ctx.invoke(f, call) {
        EvalValue::Value(Value::Array(a)) if a.len() == 1 => a.get(0, 0).clone(),
        EvalValue::Value(Value::Array(_) | Value::Lambda(_)) => Value::Error(ErrorValue::Calc),
        r @ EvalValue::Ref(_) => match ctx.to_value(r) {
            Value::Array(a) if a.len() == 1 => a.get(0, 0).clone(),
            Value::Array(_) => Value::Error(ErrorValue::Calc),
            v => v,
        },
        EvalValue::Value(v) => v,
        EvalValue::Missing => Value::Blank,
    }
}

/// Sorts each level of a tree. A key `s` sorts by output column `|s|` (descending when negative):
/// a field column sorts the level it belongs to by item, a value column sorts every level by the
/// groups' aggregates of that column (`agg`).
fn sort_tree(
    nodes: &mut [Node],
    level: usize,
    fields: usize,
    order: &[i64],
    agg: &mut dyn FnMut(&[usize], usize) -> Value,
) {
    // Aggregates needed for sorting this level, computed once per node.
    let value_keys: Vec<usize> = order
        .iter()
        .filter_map(|&s| {
            let c = s.unsigned_abs() as usize - 1;
            (c >= fields).then(|| c - fields)
        })
        .collect();
    let cached: Vec<Vec<Value>> = nodes
        .iter()
        .map(|n| value_keys.iter().map(|&v| agg(&n.rows, v)).collect())
        .collect();
    let mut idx: Vec<usize> = (0..nodes.len()).collect();
    idx.sort_by(|&a, &b| {
        let mut vk = 0;
        for &s in order {
            let c = s.unsigned_abs() as usize - 1;
            let ord = if c < fields {
                if c != level {
                    continue;
                }
                sort_compare(&nodes[a].item, &nodes[b].item)
            } else {
                let o = sort_compare(&cached[a][vk], &cached[b][vk]);
                vk += 1;
                o
            };
            let ord = if s < 0 { ord.reverse() } else { ord };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    });
    let mut taken: Vec<Option<Node>> = nodes
        .iter_mut()
        .map(|n| {
            Some(Node {
                item: std::mem::replace(&mut n.item, Value::Blank),
                rows: std::mem::take(&mut n.rows),
                children: std::mem::take(&mut n.children),
            })
        })
        .collect();
    for (slot, i) in nodes.iter_mut().zip(idx) {
        if let Some(n) = taken[i].take() {
            *slot = n;
        }
    }
    for n in nodes.iter_mut() {
        sort_tree(&mut n.children, level + 1, fields, order, agg);
    }
}

/// A row (or column) of the output: its labels per field, its data rows and the rows of its
/// parent group (the reference of `PERCENTOF` relative to the parent).
struct Line {
    labels: Vec<Value>,
    rows: Vec<usize>,
    parent: Vec<usize>,
}

/// Lists the lines of a tree with totals at `depth` (0 none, 1 the grand total, `d` subtotals of
/// the `d − 1` outer levels; negative above their groups).
fn lines(roots: &[Node], fields: usize, depth: i64, all: &[usize]) -> Vec<Line> {
    fn walk(
        nodes: &[Node],
        path: &mut Vec<Value>,
        parent: &[usize],
        fields: usize,
        depth: i64,
        out: &mut Vec<Line>,
    ) {
        let sub_levels = (depth.unsigned_abs() as usize).saturating_sub(1);
        let top = depth < 0;
        for n in nodes {
            path.push(n.item.clone());
            if n.children.is_empty() {
                let mut labels = path.clone();
                labels.resize(fields, empty());
                out.push(Line {
                    labels,
                    rows: n.rows.clone(),
                    parent: parent.to_vec(),
                });
            } else {
                let subtotal = path.len() <= sub_levels;
                let mut labels = path.clone();
                labels.resize(fields, empty());
                let line = Line {
                    labels,
                    rows: n.rows.clone(),
                    parent: parent.to_vec(),
                };
                if subtotal && top {
                    out.push(line);
                    walk(&n.children, path, &n.rows, fields, depth, out);
                } else {
                    walk(&n.children, path, &n.rows, fields, depth, out);
                    if subtotal {
                        out.push(line);
                    }
                }
            }
            path.pop();
        }
    }
    let mut out = Vec::new();
    let grand = || {
        let mut labels = vec![empty(); fields.max(1)];
        labels[0] = Value::text(TOTAL);
        Line {
            labels,
            rows: all.to_vec(),
            parent: all.to_vec(),
        }
    };
    if depth < 0 {
        out.push(grand());
    }
    walk(roots, &mut Vec::new(), all, fields, depth, &mut out);
    if depth > 0 {
        out.push(grand());
    }
    out
}

/// Flat combinations of every field (`field_relationship` 1), with an optional grand total.
fn table_lines(
    data: &Data,
    fields: usize,
    order: &[i64],
    depth: i64,
    agg: &mut dyn FnMut(&[usize], usize) -> Value,
) -> Vec<Line> {
    const END: usize = usize::MAX;
    let mut heads: FxHashMap<u64, usize> = FxHashMap::default();
    let mut next: Vec<usize> = Vec::new();
    let mut groups: Vec<Line> = Vec::new();
    for &r in &data.rows {
        let key: Vec<&Value> = (0..fields).map(|f| &data.keys[f][r]).collect();
        let h = key
            .iter()
            .fold(0u64, |acc, v| acc.rotate_left(7) ^ item_hash(v));
        let mut g = heads.get(&h).copied().unwrap_or(END);
        while g != END
            && !groups[g]
                .labels
                .iter()
                .zip(&key)
                .all(|(a, b)| lookup_equals(a, b))
        {
            g = next[g];
        }
        if g == END {
            next.push(heads.insert(h, groups.len()).unwrap_or(END));
            groups.push(Line {
                labels: key.into_iter().cloned().collect(),
                rows: vec![r],
                parent: data.rows.clone(),
            });
        } else {
            groups[g].rows.push(r);
        }
    }
    let cached: Vec<Vec<Value>> = groups
        .iter()
        .map(|g| {
            order
                .iter()
                .filter_map(|&s| {
                    let c = s.unsigned_abs() as usize - 1;
                    (c >= fields).then(|| agg(&g.rows, c - fields))
                })
                .collect()
        })
        .collect();
    let mut idx: Vec<usize> = (0..groups.len()).collect();
    idx.sort_by(|&a, &b| {
        let mut vk = 0;
        for &s in order {
            let c = s.unsigned_abs() as usize - 1;
            let ord = if c < fields {
                sort_compare(&groups[a].labels[c], &groups[b].labels[c])
            } else {
                let o = sort_compare(&cached[a][vk], &cached[b][vk]);
                vk += 1;
                o
            };
            let ord = if s < 0 { ord.reverse() } else { ord };
            if ord != Ordering::Equal {
                return ord;
            }
        }
        Ordering::Equal
    });
    let mut slots: Vec<Option<Line>> = groups.into_iter().map(Some).collect();
    let mut out: Vec<Line> = idx.into_iter().filter_map(|i| slots[i].take()).collect();
    if depth != 0 {
        let mut labels = vec![empty(); fields];
        labels[0] = Value::text(TOTAL);
        let grand = Line {
            labels,
            rows: data.rows.clone(),
            parent: data.rows.clone(),
        };
        if depth < 0 {
            out.insert(0, grand);
        } else {
            out.push(grand);
        }
    }
    out
}

fn check_order(order: &[i64], columns: usize) -> R<()> {
    if order.iter().any(|s| s.unsigned_abs() as usize > columns) {
        Err(ErrorValue::Value)
    } else {
        Ok(())
    }
}

/// The automatic total depth: the grand total and subtotals of every outer level.
fn auto_depth(fields: usize) -> i64 {
    fields.max(1) as i64
}

fn check_depth(d: i64) -> R<i64> {
    if d.unsigned_abs() > 64 {
        Err(ErrorValue::Value)
    } else {
        Ok(d)
    }
}

/// Builds the lines of one axis.
fn axis_lines(
    data: &Data,
    range: std::ops::Range<usize>,
    depth: i64,
    order: Option<Vec<i64>>,
    table: bool,
    agg: &mut dyn FnMut(&[usize], usize) -> Value,
) -> R<Vec<Line>> {
    let fields = range.len();
    let order = order.unwrap_or_else(|| (1..=fields as i64).collect());
    check_order(&order, fields + data.vals.len())?;
    if table {
        // Sorting keys that name field columns refer to this axis's own fields.
        let keys = &data.keys[range.clone()];
        let sub = Data {
            keys: keys.to_vec(),
            vals: Vec::new(),
            key_names: Vec::new(),
            val_names: Vec::new(),
            rows: data.rows.clone(),
            show_headers: false,
        };
        return Ok(table_lines(&sub, fields, &order, depth, agg));
    }
    let keys = &data.keys[range];
    let mut roots = build(keys, &data.rows, 0);
    sort_tree(&mut roots, 0, fields, &order, agg);
    Ok(lines(&roots, fields, depth, &data.rows))
}

fn groupby(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let fields = array_arg(args, 0, ctx)?;
    let values = array_arg(args, 1, ctx)?;
    let f = function_arg(args, 2, ctx)?;
    let headers = opt_int(args, 3, ctx)?;
    let depth = opt_int(args, 4, ctx)?;
    let order = sort_arg(args, 5, ctx)?;
    let filter = if omitted(args, 6) {
        None
    } else {
        Some(array_arg(args, 6, ctx)?)
    };
    let relationship = opt_int(args, 7, ctx)?.unwrap_or(0);
    if !(0..=1).contains(&relationship) {
        return Err(ErrorValue::Value);
    }
    let (data, _) = prepare(&[&fields], &values, headers, filter.as_deref())?;
    let k = data.keys.len();
    let m = data.vals.len();
    let table = relationship == 1;
    let depth = check_depth(depth.unwrap_or(if table { 1 } else { auto_depth(k) }))?;
    let all = data.rows.clone();
    let mut agg = |rows: &[usize], v: usize| aggregate(ctx, &f, &data.vals[v], rows, &all);
    let lines = axis_lines(&data, 0..k, depth, order, table, &mut agg)?;
    let mut out: Vec<Vec<Value>> = Vec::with_capacity(lines.len() + 1);
    if data.show_headers {
        let mut h = data.key_names.clone();
        h.extend(data.val_names.iter().cloned());
        out.push(h);
    }
    for line in &lines {
        let mut row = line.labels.clone();
        for v in 0..m {
            row.push(agg(&line.rows, v));
        }
        out.push(row);
    }
    check_size(&out)?;
    from_rows(out)
}

fn check_size(out: &[Vec<Value>]) -> R<()> {
    let cells = out.len().saturating_mul(out.first().map_or(0, Vec::len));
    if cells > MAX_CELLS {
        Err(ErrorValue::Num)
    } else {
        Ok(())
    }
}

fn pivotby(args: &mut [Arg<'_>], ctx: &mut dyn CallContext) -> R<Value> {
    let row_fields = array_arg(args, 0, ctx)?;
    let col_fields = array_arg(args, 1, ctx)?;
    let values = array_arg(args, 2, ctx)?;
    let f = function_arg(args, 3, ctx)?;
    let headers = opt_int(args, 4, ctx)?;
    let row_depth = opt_int(args, 5, ctx)?;
    let row_order = sort_arg(args, 6, ctx)?;
    let col_depth = opt_int(args, 7, ctx)?;
    let col_order = sort_arg(args, 8, ctx)?;
    let filter = if omitted(args, 9) {
        None
    } else {
        Some(array_arg(args, 9, ctx)?)
    };
    let relative = opt_int(args, 10, ctx)?.unwrap_or(0);
    if !(0..=4).contains(&relative) {
        return Err(ErrorValue::Value);
    }
    let (data, splits) = prepare(
        &[&row_fields, &col_fields],
        &values,
        headers,
        filter.as_deref(),
    )?;
    let k = splits[1];
    let kc = data.keys.len() - k;
    let m = data.vals.len();
    let row_depth = check_depth(row_depth.unwrap_or(auto_depth(k)))?;
    let col_depth = check_depth(col_depth.unwrap_or(auto_depth(kc)))?;
    let all = data.rows.clone();
    let mut agg = |rows: &[usize], v: usize| aggregate(ctx, &f, &data.vals[v], rows, &all);
    let row_lines = axis_lines(&data, 0..k, row_depth, row_order, false, &mut agg)?;
    // Column sort keys count columns as the column fields first, then the value columns.
    let col_lines = axis_lines(&data, k..k + kc, col_depth, col_order, false, &mut agg)?;
    drop(agg);
    let n = data.keys.first().map_or(0, Vec::len);
    let width = k + col_lines.len() * m;
    let header_rows = kc + usize::from(m > 1);
    let mut out: Vec<Vec<Value>> = Vec::with_capacity(header_rows + row_lines.len());
    for h in 0..header_rows {
        let mut line = vec![empty(); width];
        if data.show_headers && h + 1 == header_rows {
            for (i, name) in data.key_names[..k].iter().enumerate() {
                line[i] = name.clone();
            }
        }
        for (ci, col) in col_lines.iter().enumerate() {
            for v in 0..m {
                let at = k + ci * m + v;
                line[at] = if h < kc {
                    col.labels.get(h).cloned().unwrap_or_else(empty)
                } else {
                    data.val_names[v].clone()
                };
            }
        }
        out.push(line);
    }
    // Rows of each column line, as a membership mark per data row.
    let marks: Vec<Vec<bool>> = col_lines
        .iter()
        .map(|c| {
            let mut mark = vec![false; n];
            for &r in &c.rows {
                mark[r] = true;
            }
            mark
        })
        .collect();
    for rl in &row_lines {
        let mut line = rl.labels.clone();
        line.resize(k, empty());
        for (ci, cl) in col_lines.iter().enumerate() {
            let cell: Vec<usize> = rl.rows.iter().copied().filter(|&r| marks[ci][r]).collect();
            // 0 the column's total, 1 the row's, 2 everything, 3 and 4 the parent column or row
            // group within this cell's row or column.
            let reference: &[usize] = match relative {
                0 => &cl.rows,
                1 => &rl.rows,
                2 => &all,
                3 => &cl.parent,
                _ => &rl.parent,
            };
            let parent_cell: Vec<usize>;
            let reference = match relative {
                3 | 4 => {
                    let (other, mine) = if relative == 3 {
                        (&rl.rows, reference)
                    } else {
                        (&cl.rows, reference)
                    };
                    let mut mark = vec![false; n];
                    for &r in other {
                        mark[r] = true;
                    }
                    parent_cell = mine.iter().copied().filter(|&r| mark[r]).collect();
                    &parent_cell[..]
                }
                _ => reference,
            };
            for v in 0..m {
                line.push(aggregate(ctx, &f, &data.vals[v], &cell, reference));
            }
        }
        out.push(line);
    }
    check_size(&out)?;
    from_rows(out)
}

/// The `GROUPBY` and `PIVOTBY` specs.
pub(crate) fn specs() -> Vec<FunctionSpec> {
    vec![
        spec("GROUPBY", 3, 8, imp!(groupby)),
        spec("PIVOTBY", 4, 11, imp!(pivotby)),
    ]
}
