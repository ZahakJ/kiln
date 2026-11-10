// SPDX-License-Identifier: AGPL-3.0-or-later
//! The accessibility tree of the visible grid: a `grid` with the column header
//! row (column headers named by their letters), then one row per visible row (its row header,
//! named by its number, then its cells), each cell named by its address and carrying its
//! displayed value; the active cell is the tree's focus and its value and formula are announced.
//!
//! **Node ids** carry the position, so the mirror can set `aria-rowindex`/`aria-colindex`
//! without another field: a cell is `CELL_BASE + row × 2^15 + col`, a row `ROW_BASE + row`,
//! the header row [`HEADER_ROW`], a column header `HEADER_BASE + col`, a row header
//! `ROW_HEADER_BASE + row`, the grid [`GRID`]. All stay below 2^53 (exact JavaScript numbers). The grid's `text` is `"<rows> <cols>"`, the sheet's
//! size for `aria-rowcount`/`aria-colcount`.

use kiln_common::NodeId;
use kiln_protocol::geom::Rect;
use kiln_protocol::{A11yNode, A11yRole, A11yTree};
use kiln_sheet::address::cell_a1;
use kiln_sheet::{CellPos, MAX_COLS, MAX_ROWS};

/// Row headers: `ROW_HEADER_BASE + row`.
pub const ROW_HEADER_BASE: u64 = 1 << 36;
/// The grid node.
pub const GRID: u64 = 1 << 37;
/// The column header row.
pub const HEADER_ROW: u64 = 1 << 38;
/// Header cells: `HEADER_BASE + col`.
pub const HEADER_BASE: u64 = (1 << 38) + 1;
/// Rows: `ROW_BASE + row`.
pub const ROW_BASE: u64 = 1 << 39;
/// Cells: `CELL_BASE + row × 2^15 + col`.
pub const CELL_BASE: u64 = 1 << 40;

/// The id of a cell node.
#[must_use]
pub fn cell_id(row: u32, col: u32) -> NodeId {
    NodeId(CELL_BASE + (u64::from(row) << 15) + u64::from(col))
}

/// One visible cell for the tree.
#[derive(Debug, Clone, PartialEq)]
pub struct A11yCell {
    /// Position.
    pub at: CellPos,
    /// Displayed text.
    pub text: String,
    /// Bounds in sheet pixels.
    pub bounds: Rect,
}

/// Builds the tree for the visible rows and columns.
#[must_use]
pub fn grid_tree(
    sheet_name: &str,
    title: &str,
    rows: &[u32],
    cols: &[u32],
    cells: &[A11yCell],
    active: CellPos,
    announce: Option<String>,
) -> A11yTree {
    let mut nodes = Vec::with_capacity(cells.len() + rows.len() + cols.len() + 3);
    let row_ids: Vec<NodeId> = rows
        .iter()
        .map(|&r| NodeId(ROW_BASE + u64::from(r)))
        .collect();
    let mut grid_children = vec![NodeId(HEADER_ROW)];
    grid_children.extend(row_ids.iter().copied());
    nodes.push(A11yNode {
        id: NodeId(GRID),
        role: A11yRole::Grid,
        name: Some(format!("{title} – {sheet_name}")),
        text: Some(format!("{MAX_ROWS} {MAX_COLS}")),
        bounds: None,
        level: None,
        children: grid_children,
        objects: None,
        revisions: None,
    });
    nodes.push(A11yNode {
        id: NodeId(HEADER_ROW),
        role: A11yRole::Row,
        name: None,
        text: None,
        bounds: None,
        level: None,
        children: cols
            .iter()
            .map(|&c| NodeId(HEADER_BASE + u64::from(c)))
            .collect(),
        objects: None,
        revisions: None,
    });
    for &c in cols {
        let letters = cell_a1(0, c, false);
        let letters = letters
            .trim_end_matches(|ch: char| ch.is_ascii_digit())
            .to_owned();
        nodes.push(A11yNode {
            id: NodeId(HEADER_BASE + u64::from(c)),
            role: A11yRole::ColumnHeader,
            name: Some(letters.clone()),
            text: Some(letters),
            bounds: None,
            level: None,
            children: Vec::new(),
            objects: None,
            revisions: None,
        });
    }
    for (&r, id) in rows.iter().zip(&row_ids) {
        let number = (u64::from(r) + 1).to_string();
        let header = NodeId(ROW_HEADER_BASE + u64::from(r));
        let mut children = vec![header];
        children.extend(cols.iter().map(|&c| cell_id(r, c)));
        nodes.push(A11yNode {
            id: *id,
            role: A11yRole::Row,
            name: Some(number.clone()),
            text: None,
            bounds: None,
            level: None,
            children,
            objects: None,
            revisions: None,
        });
        nodes.push(A11yNode {
            id: header,
            role: A11yRole::RowHeader,
            name: Some(number.clone()),
            text: Some(number),
            bounds: None,
            level: None,
            children: Vec::new(),
            objects: None,
            revisions: None,
        });
        for &c in cols {
            let cell = cells.iter().find(|x| x.at.row == r && x.at.col == c);
            nodes.push(A11yNode {
                id: cell_id(r, c),
                role: A11yRole::GridCell,
                name: Some(cell_a1(r, c, false)),
                text: Some(cell.map(|x| x.text.clone()).unwrap_or_default()),
                bounds: cell.map(|x| x.bounds),
                level: None,
                children: Vec::new(),
                objects: None,
                revisions: None,
            });
        }
    }
    A11yTree {
        nodes,
        active: Some(cell_id(active.row, active.col)),
        announce,
        caret: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_encode_positions_below_2_pow_53() {
        let id = cell_id(1_048_575, 16_383).0;
        assert!(id < (1 << 53));
        assert_eq!((id - CELL_BASE) >> 15, 1_048_575);
        assert_eq!((id - CELL_BASE) & 0x7FFF, 16_383);
        let tree = grid_tree(
            "Budget",
            "Studio budget",
            &[0, 1],
            &[0, 2],
            &[A11yCell {
                at: CellPos::new(1, 2),
                text: "7,950".into(),
                bounds: Rect::new(128.0, 20.0, 64.0, 20.0),
            }],
            CellPos::new(1, 2),
            Some("C2, 7,950".into()),
        );
        assert_eq!(tree.nodes[0].role, A11yRole::Grid);
        assert_eq!(tree.active, Some(cell_id(1, 2)));
        let c2 = tree.nodes.iter().find(|n| n.id == cell_id(1, 2)).unwrap();
        assert_eq!(c2.name.as_deref(), Some("C2"));
        assert_eq!(c2.text.as_deref(), Some("7,950"));
        let header = tree
            .nodes
            .iter()
            .find(|n| n.id == NodeId(HEADER_BASE + 2))
            .unwrap();
        assert_eq!(header.name.as_deref(), Some("C"));
    }

    #[test]
    fn every_visible_cell_and_header_has_a_named_node_once() {
        let rows: Vec<u32> = (0..40).chain([1_048_575]).collect();
        let cols: Vec<u32> = (0..26).chain([16_383]).collect();
        let cells: Vec<A11yCell> = rows
            .iter()
            .step_by(3)
            .flat_map(|&r| {
                cols.iter().step_by(2).map(move |&c| A11yCell {
                    at: CellPos::new(r, c),
                    text: format!("{r}:{c}"),
                    bounds: Rect::new(0.0, 0.0, 64.0, 20.0),
                })
            })
            .collect();
        let tree = grid_tree("S", "Book", &rows, &cols, &cells, CellPos::new(0, 0), None);
        let mut ids = std::collections::HashSet::new();
        assert!(tree.nodes.iter().all(|n| ids.insert(n.id)), "unique ids");
        let by_id: std::collections::HashMap<_, _> = tree.nodes.iter().map(|n| (n.id, n)).collect();
        for n in &tree.nodes {
            assert!(
                n.children.iter().all(|c| by_id.contains_key(c)),
                "children exist"
            );
        }
        for &r in &rows {
            let row = by_id[&NodeId(ROW_BASE + u64::from(r))];
            assert_eq!(row.children.len(), cols.len() + 1);
            let header = by_id[&row.children[0]];
            assert_eq!(header.role, A11yRole::RowHeader);
            assert_eq!(header.name.as_deref(), Some((r + 1).to_string().as_str()));
            for &c in &cols {
                let cell = by_id[&cell_id(r, c)];
                assert_eq!(cell.role, A11yRole::GridCell);
                assert_eq!(cell.name.as_deref(), Some(cell_a1(r, c, false).as_str()));
                let want = cells
                    .iter()
                    .find(|x| x.at == CellPos::new(r, c))
                    .map(|x| x.text.as_str());
                assert_eq!(cell.text.as_deref(), Some(want.unwrap_or("")));
            }
        }
        for &c in &cols {
            let h = by_id[&NodeId(HEADER_BASE + u64::from(c))];
            assert_eq!(h.role, A11yRole::ColumnHeader);
            assert!(h.name.as_deref().is_some_and(|n| !n.is_empty()));
        }
        assert_eq!(
            by_id[&NodeId(HEADER_BASE + 16_383)].name.as_deref(),
            Some("XFD")
        );
        const { assert!(ROW_HEADER_BASE + 1_048_576 < GRID) };
    }
}
