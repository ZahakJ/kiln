// SPDX-License-Identifier: AGPL-3.0-or-later
//! Golden renders of the styled `fixtures/xlsx/features` workbooks (`fixtures/golden/grid/*.png`):
//! every sheet's used range with its headers, in the light and dark themes.
//!
//! `KILN_UPDATE_GOLDEN=1 cargo nextest run -p kiln-sheet-layout --test golden` rewrites them;
//! look at the images before committing. `KILN_GRID_PREVIEW=<dir>` also writes 2× renders there.

use std::path::{Path, PathBuf};

use kiln_io_xlsx::read_xlsx;
use kiln_raster::{PaintOptions, View, golden, render_delta};
use kiln_scene::Palette;
use kiln_sheet_layout::{RenderOptions, render_sheet};
use kiln_text::FontStore;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// The fixtures with a golden image, and the sheets of each.
const FIXTURES: &[(&str, &[usize])] = &[
    ("styles", &[0]),
    ("number-formats", &[0]),
    ("merges", &[0]),
    ("conditional-formatting", &[0]),
    ("rich-text", &[0]),
    ("hidden-rows-cols", &[0]),
    ("freeze-panes", &[0]),
    ("tables", &[0]),
    ("hyperlinks", &[0]),
    ("comments", &[0]),
];

fn render(name: &str, sheet: usize, scale: f32, palette: &Palette) -> kiln_raster::Rgba {
    let path = repo().join(format!("fixtures/xlsx/features/{name}.xlsx"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut read = read_xlsx(&bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
    let r = render_sheet(
        &mut read.workbook,
        sheet,
        FontStore::bundled(),
        &RenderOptions {
            scale,
            // Keep goldens small: at most 40 rows and 16 columns.
            range: None,
            ..RenderOptions::default()
        },
    )
    .unwrap_or_else(|| panic!("{name}: no sheet {sheet}"));
    let rect = r.rect;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (w, h) = (
        ((rect.w.min(1600.0)) * scale).round() as u32,
        ((rect.h.min(1200.0)) * scale).round() as u32,
    );
    render_delta(
        &r.delta,
        &View {
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
    .unwrap_or_else(|e| panic!("{name}: {e}"))
}

#[test]
fn fixture_sheets_match_their_goldens() {
    let preview = std::env::var_os("KILN_GRID_PREVIEW").map(PathBuf::from);
    let mut failures = Vec::new();
    for (name, sheets) in FIXTURES {
        for &sheet in *sheets {
            for (theme, palette) in [
                ("light", Palette::light().for_app("grid")),
                ("dark", Palette::dark().for_app("grid")),
            ] {
                let img = render(name, sheet, 1.0, &palette);
                let golden_path =
                    repo().join(format!("fixtures/golden/grid/{name}-{sheet}-{theme}.png"));
                if let Err(e) = golden::check(&img, &golden_path) {
                    failures.push(format!("{name} sheet {sheet} ({theme}): {e}"));
                }
                if let Some(dir) = &preview {
                    let _ = std::fs::create_dir_all(dir);
                    let big = render(name, sheet, 2.0, &palette);
                    let _ = std::fs::write(
                        dir.join(format!("{name}-{sheet}-{theme}@2x.png")),
                        big.encode_png().unwrap_or_default(),
                    );
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
