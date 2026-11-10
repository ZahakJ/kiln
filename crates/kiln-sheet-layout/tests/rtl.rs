// SPDX-License-Identifier: AGPL-3.0-or-later
//! Right-to-left sheets (`sheetView rightToLeft`): golden renders of
//! `fixtures/xlsx/rtl/sales.xlsx` (`fixtures/golden/rtl/sales-0-{light,dark}.png`, checked against
//! LibreOffice's rendering, see the README there; `KILN_UPDATE_GOLDEN=1` rewrites them), and the
//! mirrored geometry: column A and the row headers at the right, hit testing through the mirror,
//! the frozen columns at the right, text that keeps its reading order.

use std::path::{Path, PathBuf};

use kiln_io_xlsx::read_xlsx;
use kiln_raster::{PaintOptions, View as RasterView, golden, render_delta};
use kiln_scene::{DrawOp, Palette};
use kiln_sheet::Workbook;
use kiln_sheet_layout::{
    HitTarget, Pane, RenderOptions, SheetGeometry, View, geometry_of, render_sheet,
};
use kiln_text::FontStore;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn workbook() -> Workbook {
    let bytes = std::fs::read(repo().join("fixtures/xlsx/rtl/sales.xlsx")).expect("fixture");
    read_xlsx(&bytes).expect("reads").workbook
}

fn render(wb: &mut Workbook, sheet: usize, palette: &Palette) -> kiln_raster::Rgba {
    let r =
        render_sheet(wb, sheet, FontStore::bundled(), &RenderOptions::default()).expect("sheet");
    let rect = r.rect;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (
        rect.w.min(1600.0).round() as u32,
        rect.h.min(1200.0).round() as u32,
    );
    render_delta(
        &r.delta,
        &RasterView {
            width: w,
            height: h,
            scroll_x: rect.x,
            scroll_y: rect.y,
            options: PaintOptions {
                caret_visible: false,
                touch: false,
            },
        },
        palette,
    )
    .expect("renders")
}

#[test]
fn the_right_to_left_sheet_matches_its_goldens() {
    let mut failures = Vec::new();
    for (theme, palette) in [
        ("light", Palette::light().for_app("grid")),
        ("dark", Palette::dark().for_app("grid")),
    ] {
        let img = render(&mut workbook(), 0, &palette);
        let path = repo().join(format!("fixtures/golden/rtl/sales-0-{theme}.png"));
        if let Err(e) = golden::check(&img, &path) {
            failures.push(format!("{theme}: {e}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_sheet_reads_its_direction_and_the_second_sheet_runs_left_to_right() {
    let wb = workbook();
    assert!(geometry_of(&wb.sheets()[0], true).rtl);
    assert!(!geometry_of(&wb.sheets()[1], true).rtl);
}

fn view() -> View {
    View {
        width: 800.0,
        height: 600.0,
        scroll_x: 0.0,
        scroll_y: 0.0,
        zoom: 1.0,
    }
}

fn geo() -> SheetGeometry {
    let mut wb = Workbook::new();
    let _ = &mut wb;
    let mut g = geometry_of(&wb.sheets()[0], true);
    g.rtl = true;
    g
}

#[test]
fn hit_testing_follows_the_mirror() {
    let g = geo();
    let v = view();
    let hw = g.header_w(30);
    // The corner and the row headers are at the right.
    assert_eq!(
        g.hit_test(&v, 800.0 - 5.0, 5.0, hw, None),
        Some(HitTarget::Corner)
    );
    assert!(matches!(
        g.hit_test(&v, 800.0 - 5.0, 34.0, hw, None),
        Some(HitTarget::RowHeader { row: 0 })
    ));
    // Column A is just left of the row headers, B left of A.
    let a = 800.0 - hw - 10.0;
    assert_eq!(
        g.hit_test(&v, a, 40.0, hw, None),
        Some(HitTarget::Cell { row: 0, col: 0 })
    );
    assert_eq!(
        g.hit_test(&v, a - 64.0, 40.0, hw, None),
        Some(HitTarget::Cell { row: 0, col: 1 })
    );
    assert!(matches!(
        g.hit_test(&v, a - 64.0, 10.0, hw, None),
        Some(HitTarget::ColumnHeader { col: 1 })
    ));
    // The border between A and B is A's left edge: dragging it resizes A.
    assert!(matches!(
        g.hit_test(&v, 800.0 - hw - 64.0 + 1.0, 10.0, hw, None),
        Some(HitTarget::ColumnBorder { col: 0 })
    ));
    // The fill handle at the bottom-left corner of the selection.
    let sel = kiln_sheet::Range::new(0, 0, 1, 1);
    let r = g.surface_rect(&v, g.range_rect(&sel), hw);
    assert_eq!(
        g.hit_test(&v, r.x, r.bottom(), hw, Some(&sel)),
        Some(HitTarget::FillHandle)
    );
    // Surface and sheet positions map both ways.
    let (sx, sy) = g.to_surface(&v, 100.0, 30.0, hw);
    let (_, x, y) = g.to_sheet(&v, sx, sy, hw).expect("in a pane");
    assert!((x - 100.0).abs() < 0.01 && (y - 30.0).abs() < 0.01);
    assert!((sx - (800.0 - hw - 100.0)).abs() < 0.01);
}

#[test]
fn frozen_columns_stay_at_the_right() {
    let mut g = geo();
    g.freeze_cols = 2;
    g.freeze_rows = 1;
    let v = View {
        scroll_x: 640.0,
        ..view()
    };
    let hw = g.header_w(30);
    let (_, _, frozen) = g.pane(&v, Pane::TopLeft, hw);
    assert!((frozen.right() - (800.0 - hw)).abs() < 0.01, "{frozen:?}");
    assert!((frozen.w - 128.0).abs() < 0.01);
    let (sx, _, scrolled) = g.pane(&v, Pane::BottomRight, hw);
    assert!(scrolled.right() <= frozen.x + 0.01);
    assert_eq!(scrolled.x, 0.0);
    assert_eq!(sx, 640.0);
    // A point just left of the frozen columns is the first scrolled column.
    assert_eq!(
        g.hit_test(&v, frozen.x - 5.0, 40.0, hw, None),
        Some(HitTarget::Cell { row: 0, col: 10 })
    );
}

#[test]
fn text_keeps_its_reading_order_in_mirrored_tiles() {
    // Every glyph run of the Arabic product column reads right to left inside its cell, and the
    // header letters read A, B, … from the right.
    let mut wb = workbook();
    let r =
        render_sheet(&mut wb, 0, FontStore::bundled(), &RenderOptions::default()).expect("sheet");
    let headers = r
        .delta
        .tiles
        .iter()
        .find(|t| t.id.layer == kiln_sheet_layout::scene::COL_HEADER_LAYER)
        .expect("column headers");
    assert!(
        headers.rect.right() <= 0.0,
        "mirrored to the left of the origin"
    );
    let xs: Vec<i32> = headers
        .ops
        .iter()
        .filter_map(|op| match op {
            DrawOp::Glyphs(g) => g.quads.get(4).copied(),
            _ => None,
        })
        .collect();
    assert!(xs.len() >= 6);
    assert!(xs.windows(2).all(|w| w[1] < w[0]), "A at the right: {xs:?}");
}

#[test]
fn fonts_added_later_draw_the_arabic_text() {
    use kiln_sheet_layout::{ChartLook, GridScene, HeaderMarks, SceneRequest, view_options};
    use kiln_text::FontStoreOptions;
    // The web core starts without an Arabic font and is given one when a sheet needs it.
    let font = |p: &str| std::fs::read(repo().join(p)).expect("font");
    let mut wb = workbook();
    let mut fonts = FontStore::new(&FontStoreOptions {
        bundled: false,
        system: false,
        ..FontStoreOptions::default()
    });
    fonts.add_font_data(font("assets/fonts/liberation/LiberationSans-Regular.ttf"));
    fonts.refresh_fallbacks();
    let mut scene = GridScene::new(fonts);
    let geo = geometry_of(&wb.sheets()[0], true);
    let opts = view_options(&wb, &wb.sheets()[0]);
    let marks = HeaderMarks::default();
    let frame = |scene: &mut GridScene, wb: &mut Workbook| {
        let _ = kiln_text::take_missing();
        scene.prepare(
            wb,
            0,
            &geo,
            SceneRequest {
                view: view(),
                scale: 1.0,
                header_w: 44.0,
                extent: (4000.0, 4000.0),
                marks: &marks,
                all_selected: false,
                opts: &opts,
                overlay: Vec::new(),
                charts: &[],
                chart_look: ChartLook::default(),
                objects: &[],
            },
        );
        kiln_text::take_missing() & kiln_text::MISSING_ARABIC
    };
    assert_ne!(frame(&mut scene, &mut wb), 0, "Arabic without a font");
    scene.add_fonts(&[std::sync::Arc::from(font(
        "assets/fonts/noto/NotoNaskhArabic-Regular.ttf",
    ))]);
    assert_eq!(
        frame(&mut scene, &mut wb),
        0,
        "every tile drawn again with the font"
    );
}
