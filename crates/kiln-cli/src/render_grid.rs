// SPDX-License-Identifier: AGPL-3.0-or-later
//! `render` for workbooks (WP-71): one PNG per visible sheet showing its used range with row and
//! column headers, painted by `kiln-sheet-layout` exactly as the grid shows it.

use std::path::Path;

use kiln_raster::{Compositor, MAX_SIDE, PaintOptions, Rgba, View};
use kiln_scene::{Bounds, Palette};
use kiln_sheet::Workbook;
use kiln_sheet::meta::SheetVisibility;
use kiln_sheet_layout::{RenderOptions, render_sheet};

use crate::pages::{PageOptions, RenderedPage};

/// Whether `render` treats a file as a workbook.
#[must_use]
pub fn is_workbook(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("xlsx" | "xlsm" | "xltx" | "xltm" | "csv" | "tsv")
    )
}

fn read(path: &Path, bytes: &[u8]) -> Result<Workbook, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext == "csv" || ext == "tsv" {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.chars().take(31).collect());
        return kiln_io_csv::read_csv(
            bytes,
            &kiln_io_csv::ImportOptions {
                sheet_name: stem,
                ..kiln_io_csv::ImportOptions::default()
            },
        )
        .map(|i| i.workbook)
        .map_err(|e| e.to_string());
    }
    kiln_io_xlsx::read_xlsx(bytes)
        .map(|r| r.workbook)
        .map_err(|e| e.to_string())
}

/// Renders the visible sheets of a workbook ("page" N is the N-th visible sheet).
///
/// # Errors
///
/// Unreadable files, sheet numbers outside the workbook and raster errors.
pub fn render_workbook(path: &Path, options: &PageOptions) -> Result<Vec<RenderedPage>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut wb = read(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    let visible: Vec<usize> = wb
        .sheets()
        .iter()
        .enumerate()
        .filter(|(_, s)| s.meta().visibility == SheetVisibility::Visible)
        .map(|(i, _)| i)
        .collect();
    if visible.is_empty() {
        return Err(format!(
            "{}: the workbook has no visible sheet",
            path.display()
        ));
    }
    let wanted: Vec<usize> = match &options.pages {
        None => (1..=visible.len()).collect(),
        Some(list) => {
            for &n in list {
                if n == 0 || n > visible.len() {
                    return Err(format!(
                        "sheet {n} does not exist (the workbook has {} visible sheet{})",
                        visible.len(),
                        if visible.len() == 1 { "" } else { "s" }
                    ));
                }
            }
            list.clone()
        }
    };
    #[allow(clippy::cast_precision_loss)]
    let dpr = options.dpi as f32 / 96.0;
    let palette = if options.dark {
        Palette::dark()
    } else {
        Palette::light()
    }
    .for_app("grid");
    let fonts = || kiln_text::FontStore::bundled();
    let mut out = Vec::with_capacity(wanted.len());
    for n in wanted {
        let index = visible[n - 1];
        let r = render_sheet(
            &mut wb,
            index,
            fonts(),
            &RenderOptions {
                scale: dpr,
                ..RenderOptions::default()
            },
        )
        .ok_or_else(|| format!("sheet {n} could not be rendered"))?;
        let mut c = Compositor::new();
        c.apply(&r.delta).map_err(|e| e.to_string())?;
        // Very large used ranges are cut to the largest bitmap (from the top-left corner).
        let max = MAX_SIDE as f32 / dpr;
        let rect = Bounds::new(r.rect.x, r.rect.y, r.rect.w.min(max), r.rect.h.min(max));
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (w, h) = (
            ((rect.w * dpr).round() as u32).clamp(1, MAX_SIDE),
            ((rect.h * dpr).round() as u32).clamp(1, MAX_SIDE),
        );
        let pixmap = c
            .render(
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
                &palette,
            )
            .map_err(|e| e.to_string())?;
        out.push(RenderedPage {
            number: n,
            rect,
            image: Rgba::from_pixmap(&pixmap),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_every_visible_sheet_of_a_fixture() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/xlsx/features/multi-sheet.xlsx");
        assert!(is_workbook(&path));
        let pages = render_workbook(&path, &PageOptions::default()).expect("renders");
        assert!(pages.len() >= 2);
        assert!(
            pages
                .iter()
                .all(|p| p.image.width > 100 && p.image.height > 100)
        );
        let one = render_workbook(
            &path,
            &PageOptions {
                pages: Some(vec![1]),
                dark: true,
                ..PageOptions::default()
            },
        )
        .expect("one sheet");
        assert_eq!(one.len(), 1);
        assert!(
            render_workbook(
                &path,
                &PageOptions {
                    pages: Some(vec![99]),
                    ..PageOptions::default()
                }
            )
            .is_err()
        );
        assert!(!is_workbook(Path::new("a.docx")));
    }
}
