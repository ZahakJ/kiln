// SPDX-License-Identifier: AGPL-3.0-or-later
//! `render` for presentations: one PNG per slide (`--page` picks slides), drawn by
//! `kiln-deck-layout` exactly as the stage and the thumbnails show it, without the editing
//! view's placeholder prompts. Hidden slides are rendered too, as PowerPoint exports them.

use std::path::Path;

use kiln_deck::Presentation;
use kiln_deck_layout::render::{Options, Renderer};
use kiln_deck_layout::scene::slide_delta;
use kiln_raster::{Compositor, MAX_SIDE, PaintOptions, Rgba, View};
use kiln_scene::{Bounds, Palette};

use crate::pages::{PageOptions, RenderedPage};

/// Whether `render` treats a file as a presentation.
#[must_use]
pub fn is_presentation(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("pptx" | "pptm" | "potx" | "potm" | "ppsx" | "ppsm" | "odp" | "kdeck")
    )
}

fn read(path: &Path, bytes: &[u8]) -> Result<Presentation, String> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext == "kdeck" {
        let bundle = kiln_storage::Bundle::from_bytes(bytes).map_err(|e| e.to_string())?;
        return Presentation::from_bytes(&bundle.model).map_err(|e| e.to_string());
    }
    if ext == "odp" {
        return kiln_io_odf::read_odp(bytes)
            .map(|r| r.presentation)
            .map_err(|e| e.to_string());
    }
    kiln_io_pptx::read_pptx(bytes)
        .map(|r| r.presentation)
        .map_err(|e| e.to_string())
}

/// Renders slides of a presentation ("page" N is slide N).
///
/// # Errors
///
/// Unreadable files, slide numbers outside the presentation and raster errors.
pub fn render_presentation(
    path: &Path,
    options: &PageOptions,
) -> Result<Vec<RenderedPage>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let pres = read(path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    render(&pres, options)
}

/// Renders slides of a presentation already read.
///
/// # Errors
///
/// Slide numbers outside the presentation and raster errors.
pub fn render(pres: &Presentation, options: &PageOptions) -> Result<Vec<RenderedPage>, String> {
    let count = pres.slides.len();
    if count == 0 {
        return Err("the presentation has no slides".into());
    }
    let wanted: Vec<usize> = match &options.pages {
        None => (1..=count).collect(),
        Some(list) => {
            for &n in list {
                if n == 0 || n > count {
                    return Err(format!(
                        "slide {n} does not exist (the presentation has {count} slide{})",
                        if count == 1 { "" } else { "s" }
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
    .for_app("deck");
    let mut renderer = Renderer::new(
        kiln_deck_layout::fonts::default_fonts(),
        kiln_deck_layout::fonts::default_fonts(),
    );
    let mut out = Vec::with_capacity(wanted.len());
    for n in wanted {
        let slide = pres.slides[n - 1].id;
        let delta = slide_delta(&mut renderer, pres, slide, dpr, &Options::default());
        let mut c = Compositor::new();
        c.apply(&delta).map_err(|e| e.to_string())?;
        let max = MAX_SIDE as f32 / dpr;
        let rect = Bounds::new(0.0, 0.0, delta.extent.w.min(max), delta.extent.h.min(max));
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
                    scroll_x: 0.0,
                    scroll_y: 0.0,
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
    fn renders_one_png_per_slide() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/pptx/features/preset-shapes.pptx");
        assert!(is_presentation(&path));
        let pages = render_presentation(&path, &PageOptions::default()).expect("renders");
        assert_eq!(pages.len(), 5);
        assert!(
            pages
                .iter()
                .all(|p| p.image.width == 1280 || p.image.width == 960)
        );
        let one = render_presentation(
            &path,
            &PageOptions {
                pages: Some(vec![2]),
                ..PageOptions::default()
            },
        )
        .expect("renders slide 2");
        assert_eq!(one.len(), 1);
        assert_eq!(one[0].number, 2);
    }
}
