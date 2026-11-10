// SPDX-License-Identifier: AGPL-3.0-or-later
//! Right-to-left sheets in the session: `fixtures/xlsx/rtl/sales.xlsx` opens
//! mirrored, Page Layout ▸ Sheet Right-to-Left toggles the direction and the file keeps it, the
//! arrow keys follow the screen as in Excel, clicks land through the mirror, and the overlay and
//! the in-cell editor are where the mirrored cells are.

use kiln_protocol::commands::{
    GridCommand as G, GridDirection as D, GridHit, GridQuery, GridQueryResult, GridState,
};
use kiln_protocol::geom::Point;
use kiln_protocol::{Command, FileFormat, Query, QueryResult, Viewport};
use kiln_scene::DrawOp;

use super::GridDoc;
use crate::frame::{FrameRequest, FrameSource};
use crate::model::DocModel;

fn open() -> GridDoc {
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/xlsx/rtl/sales.xlsx");
    let bytes = std::fs::read(path).expect("fixture");
    let read = kiln_io_xlsx::read_xlsx(&bytes).expect("reads");
    let mut d = GridDoc::new(
        read.workbook,
        None,
        read.warnings,
        Some("sales.xlsx".into()),
    );
    frame(&mut d);
    d
}

fn viewport() -> Viewport {
    Viewport {
        width_px: 1000.0,
        height_px: 600.0,
        scroll: Point::new(0.0, 0.0),
        zoom: 1.0,
        dpr: 1.0,
        ..Viewport::default()
    }
}

fn frame(d: &mut GridDoc) {
    let vp = viewport();
    let req = FrameRequest::for_viewport(&vp).expect("request");
    d.prepare(&req).expect("frame");
}

fn run(d: &mut GridDoc, c: G) {
    d.execute(&Command::Grid(c.clone()))
        .unwrap_or_else(|e| panic!("{c:?}: {e}"));
}

fn state(d: &GridDoc) -> GridState {
    match d.app_query(&Query::Grid(GridQuery::State)).expect("state") {
        QueryResult::Grid(GridQueryResult::State(s)) => *s,
        other => panic!("{other:?}"),
    }
}

fn hit(d: &GridDoc, x: f32, y: f32) -> GridHit {
    match d
        .app_query(&Query::Grid(GridQuery::HitTest {
            x,
            y,
            coarse: false,
        }))
        .expect("hit")
    {
        QueryResult::Grid(GridQueryResult::Hit(h)) => h,
        other => panic!("{other:?}"),
    }
}

fn active(d: &GridDoc) -> (u32, u32) {
    let a = d.view().sel.active;
    (a.row, a.col)
}

fn at(d: &mut GridDoc, row: u32, col: u32) {
    run(
        d,
        G::SelectCell {
            row,
            col,
            extend: false,
            add: false,
        },
    );
}

#[test]
fn the_sheet_opens_right_to_left_and_the_command_toggles_it() {
    let mut d = open();
    assert!(state(&d).layout.rtl);
    run(&mut d, G::SheetRightToLeft);
    assert!(!state(&d).layout.rtl);
    // The file keeps the direction it was given.
    let saved = d.save(FileFormat::Xlsx).expect("saves");
    let back = kiln_io_xlsx::read_xlsx(&saved).expect("reads back");
    assert!(!back.workbook.sheets()[0].meta().view.right_to_left);
    run(&mut d, G::SheetRightToLeft);
    assert!(state(&d).layout.rtl);
    let saved = d.save(FileFormat::Xlsx).expect("saves");
    let xml = kiln_io_xlsx::read_xlsx(&saved).expect("reads back");
    assert!(xml.workbook.sheets()[0].meta().view.right_to_left);
    // The other sheet runs left to right.
    assert!(!xml.workbook.sheets()[1].meta().view.right_to_left);
}

#[test]
fn the_arrows_follow_the_screen_as_in_excel() {
    let mut d = open();
    at(&mut d, 3, 1);
    // Right goes towards column A (to the right on the screen), Left away from it.
    run(
        &mut d,
        G::Move {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(active(&d), (3, 0));
    run(
        &mut d,
        G::Move {
            dir: D::Left,
            extend: false,
        },
    );
    run(
        &mut d,
        G::Move {
            dir: D::Left,
            extend: false,
        },
    );
    assert_eq!(active(&d), (3, 2));
    // Shift+Left extends away from column A.
    run(
        &mut d,
        G::Move {
            dir: D::Left,
            extend: true,
        },
    );
    let r = *d.view().sel.ranges.last().expect("range");
    assert_eq!((r.c0, r.c1), (2, 3));
    // Ctrl+Left jumps to the end of the data, away from column A.
    at(&mut d, 3, 0);
    run(
        &mut d,
        G::Jump {
            dir: D::Left,
            extend: false,
        },
    );
    assert_eq!(active(&d), (3, 5));
    run(
        &mut d,
        G::Jump {
            dir: D::Right,
            extend: false,
        },
    );
    assert_eq!(active(&d), (3, 0));
    // Tab still goes to the next column.
    run(
        &mut d,
        G::Advance {
            backward: false,
            across: true,
        },
    );
    assert_eq!(active(&d), (3, 1));
}

#[test]
fn clicks_land_through_the_mirror() {
    let d = open();
    let l = state(&d).layout;
    let w = viewport().width_px;
    // The corner and the row numbers are at the right edge.
    assert_eq!(hit(&d, w - 4.0, 4.0), GridHit::Corner);
    assert!(matches!(
        hit(&d, w - 4.0, l.header_h + 10.0),
        GridHit::RowHeader { row: 0 }
    ));
    // Just left of the row numbers: column A (frozen), then the scrolled columns.
    let x = w - l.header_w - 5.0;
    assert!(matches!(
        hit(&d, x, l.header_h + l.frozen_h + 5.0),
        GridHit::Cell { col: 0, .. } | GridHit::SelectionBorder { col: 0, .. }
    ));
    let x = w - l.header_w - l.frozen_w - 5.0;
    assert!(matches!(
        hit(&d, x, l.header_h + l.frozen_h + 5.0),
        GridHit::Cell { col: 1, .. }
    ));
    // A column border's start is the column's right (start) edge on the surface.
    let (cx, _) = d.geometry_ref().cols.span(1);
    let left = w - l.header_w - l.frozen_w;
    let border = left - d.geometry_ref().cols.span(1).1 as f32 + 1.0;
    match hit(&d, border, l.header_h / 2.0) {
        GridHit::ColumnBorder { col, start, .. } => {
            assert_eq!(col, 1);
            assert!((start - left).abs() < 1.0, "{start} vs {left} ({cx})");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_selection_and_the_editor_are_where_the_mirrored_cells_are() {
    let mut d = open();
    at(&mut d, 3, 1);
    frame(&mut d);
    // The selection frame is drawn in the mirrored sheet: cell B3 spans sheet x from −(A + B)
    // to −A.
    let geo = d.geometry_ref();
    let cell = geo.cell_rect(3, 1);
    let frames: Vec<kiln_scene::Bounds> = d
        .scene()
        .overlay
        .iter()
        .filter_map(|op| match op {
            DrawOp::StrokeRect { rect, .. } => Some(*rect),
            _ => None,
        })
        .collect();
    assert!(
        frames
            .iter()
            .any(|r| (r.x + cell.right()).abs() < 3.0 && (r.right() + cell.x).abs() < 3.0),
        "{frames:?} for {cell:?}"
    );
    // The in-cell editor opens over B3, at the right of the surface.
    run(
        &mut d,
        G::BeginEdit {
            text: "مرحبا".into(),
        },
    );
    frame(&mut d);
    let ed = state(&d).editor.expect("editing");
    let l = state(&d).layout;
    let w = viewport().width_px;
    let right = w - l.header_w - l.frozen_w;
    assert!(
        (ed.rect.x + ed.rect.width - right).abs() < 1.0,
        "{:?} ends at {right}",
        ed.rect
    );
}
