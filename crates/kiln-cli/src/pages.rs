// SPDX-License-Identifier: AGPL-3.0-or-later
//! Page rendering shared by `render <file>` and the control channel's `render`.
//!
//! Pages are found in the Scene itself, so every document kind that paints pages works without
//! a per-app hook: a page is a content-layer tile whose top-level ops cast the theme's page shadow
//! (`DrawOp::Shadow` with `ShadowStyle::Page`, PLAN.md §3.6); the shadow's caster rectangle is the
//! page. The document is first scanned at a low scale to find the pages, then each wanted page is
//! framed exactly (viewport = page rectangle, device pixel ratio = dpi / 96) and composited by
//! `kiln-raster` without the caret.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use kiln_common::DocId;
use kiln_protocol::geom::Point;
use kiln_protocol::{ViewMode, Viewport};
use kiln_raster::{Compositor, MAX_SIDE, PaintOptions, Rgba, View};
use kiln_scene::{Bounds, DrawOp, Palette, SceneDelta, ShadowStyle, TileId};
use kiln_session::Session;

/// Allowed resolutions.
pub const DPI_RANGE: std::ops::RangeInclusive<u32> = 24..=600;

/// Scale of the page-finding pass (cheap glyphs; only the page rectangles matter).
const SCAN_DPR: f32 = 0.25;
/// Height of one scan step in document pixels.
const SCAN_STEP: f32 = 4096.0;

/// What to render.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOptions {
    /// Resolution (96 = one device pixel per CSS pixel).
    pub dpi: u32,
    /// Dark theme instead of light.
    pub dark: bool,
    /// 1-based page numbers to render (`None` = every page).
    pub pages: Option<Vec<usize>>,
}

impl Default for PageOptions {
    fn default() -> Self {
        Self {
            dpi: 96,
            dark: false,
            pages: None,
        }
    }
}

/// One rendered page.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedPage {
    /// 1-based page number.
    pub number: usize,
    /// The page rectangle in document pixels.
    pub rect: Bounds,
    /// The pixels.
    pub image: Rgba,
}

fn viewport(rect: Bounds, dpr: f32) -> Viewport {
    Viewport {
        width_px: rect.w.max(1.0),
        height_px: rect.h.max(1.0),
        scroll: Point::new(rect.x, rect.y),
        zoom: 1.0,
        dpr,
        mode: ViewMode::PrintLayout,
    }
}

/// Page rectangles cast by top-level page shadows of `delta`'s tiles, keyed by tile.
fn pages_in(delta: &SceneDelta, found: &mut BTreeMap<TileId, Bounds>) {
    for tile in &delta.tiles {
        let mut depth = 0usize;
        for op in &tile.ops {
            match op {
                DrawOp::PushTransform { .. } => depth += 1,
                DrawOp::PopTransform => depth = depth.saturating_sub(1),
                DrawOp::Shadow(s) if depth == 0 && s.style == ShadowStyle::Page => {
                    found.insert(
                        tile.id,
                        Bounds::new(
                            s.rect.x + tile.rect.x,
                            s.rect.y + tile.rect.y,
                            s.rect.w,
                            s.rect.h,
                        ),
                    );
                }
                _ => {}
            }
        }
    }
}

/// Finds every page of a document, in reading order (top to bottom, then left to right).
///
/// # Errors
///
/// Session errors, and documents without pages.
pub fn find_pages(session: &mut Session, doc: DocId) -> Result<Vec<Bounds>, String> {
    let e = |e: kiln_common::KilnError| e.to_string();
    // A first frame without a measured viewport reports the document's extent.
    session
        .set_viewport(doc, viewport(Bounds::new(0.0, 0.0, 1.0, 1.0), SCAN_DPR))
        .map_err(e)?;
    let first = session.frame(doc).map_err(e)?;
    let extent = first.extent;
    let mut found = BTreeMap::new();
    pages_in(&first, &mut found);
    let mut y = extent.y;
    let bottom = extent.y + extent.h;
    while y < bottom {
        let rect = Bounds::new(extent.x, y, extent.w.max(1.0), SCAN_STEP);
        session
            .set_viewport(doc, viewport(rect, SCAN_DPR))
            .map_err(e)?;
        let delta = session.frame(doc).map_err(e)?;
        pages_in(&delta, &mut found);
        y += SCAN_STEP;
    }
    let mut pages: Vec<Bounds> = found.into_values().collect();
    pages.sort_by(|a, b| a.y.total_cmp(&b.y).then(a.x.total_cmp(&b.x)));
    pages.dedup_by(|a, b| (a.x - b.x).abs() < 0.5 && (a.y - b.y).abs() < 0.5);
    if pages.is_empty() {
        return Err("the document has no pages to render".into());
    }
    Ok(pages)
}

/// Renders pages of an open document.
///
/// # Errors
///
/// Session and raster errors, page numbers outside the document, a resolution outside
/// [`DPI_RANGE`] or pages too large for one bitmap.
pub fn render(
    session: &mut Session,
    doc: DocId,
    options: &PageOptions,
) -> Result<Vec<RenderedPage>, String> {
    if !DPI_RANGE.contains(&options.dpi) {
        return Err(format!(
            "--dpi must be between {} and {}",
            DPI_RANGE.start(),
            DPI_RANGE.end()
        ));
    }
    let pages = find_pages(session, doc)?;
    let wanted: Vec<usize> = match &options.pages {
        None => (1..=pages.len()).collect(),
        Some(list) => {
            for &n in list {
                if n == 0 || n > pages.len() {
                    return Err(format!(
                        "page {n} does not exist (the document has {} page{})",
                        pages.len(),
                        if pages.len() == 1 { "" } else { "s" }
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
    };
    let mut compositor = Compositor::new();
    let mut out = Vec::with_capacity(wanted.len());
    for n in wanted {
        let rect = pages[n - 1];
        session
            .set_viewport(doc, viewport(rect, dpr))
            .map_err(|e| e.to_string())?;
        let delta = session.frame(doc).map_err(|e| e.to_string())?;
        compositor.apply(&delta).map_err(|e| e.to_string())?;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (w, h) = (
            (rect.w * dpr).round().max(1.0) as u32,
            (rect.h * dpr).round().max(1.0) as u32,
        );
        if w > MAX_SIDE || h > MAX_SIDE {
            return Err(format!(
                "page {n} would be {w}×{h} pixels at {} dpi (at most {MAX_SIDE} per side)",
                options.dpi
            ));
        }
        let view = View {
            width: w,
            height: h,
            scroll_x: rect.x,
            scroll_y: rect.y,
            options: PaintOptions {
                caret_visible: false,
                touch: false,
            },
        };
        let pixmap = compositor
            .render(&view, &palette)
            .map_err(|e| e.to_string())?;
        out.push(RenderedPage {
            number: n,
            rect,
            image: Rgba::from_pixmap(&pixmap),
        });
    }
    Ok(out)
}

/// Expands a printf-style page pattern: `%d`, or `%0Nd` with a width (`page-%03d.png`).
/// `%%` is a literal percent sign. Returns `None` when the pattern has no page placeholder.
#[must_use]
pub fn expand_pattern(pattern: &str, page: usize) -> Option<String> {
    let mut out = String::with_capacity(pattern.len() + 4);
    let mut chars = pattern.chars().peekable();
    let mut used = false;
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            out.push('%');
            continue;
        }
        let mut spec = String::new();
        while let Some(&d) = chars.peek() {
            if d.is_ascii_digit() {
                spec.push(d);
                chars.next();
            } else {
                break;
            }
        }
        if chars.peek() == Some(&'d') && spec.len() <= 2 {
            chars.next();
            let width: usize = spec.parse().unwrap_or(0);
            if spec.starts_with('0') {
                out.push_str(&format!("{page:0width$}"));
            } else {
                out.push_str(&format!("{page:width$}"));
            }
            used = true;
        } else {
            out.push('%');
            out.push_str(&spec);
        }
    }
    used.then_some(out)
}

/// Where page `page` goes: the pattern expanded, the path itself for a single page without a
/// placeholder, or `page-NNN.png` inside a directory (an existing one, or a path ending in `/`).
///
/// # Errors
///
/// Several pages and a plain file name.
pub fn output_path(target: &str, page: usize, pages: usize) -> Result<PathBuf, String> {
    if let Some(p) = expand_pattern(target, page) {
        return Ok(PathBuf::from(p));
    }
    let path = Path::new(target);
    if target.ends_with('/') || path.is_dir() {
        return Ok(path.join(format!("page-{page:03}.png")));
    }
    if pages == 1 {
        return Ok(path.to_path_buf());
    }
    Err(format!(
        "{pages} pages to write but '{target}' names one file: use a pattern such as out/page-%03d.png, a directory, or --page N"
    ))
}

/// Writes rendered pages to `target` (see [`output_path`]), creating directories, and returns
/// the paths.
///
/// # Errors
///
/// Naming and I/O errors.
pub fn write_pages(pages: &[RenderedPage], target: &str) -> Result<Vec<PathBuf>, String> {
    let mut written = Vec::with_capacity(pages.len());
    for p in pages {
        let path = output_path(target, p.number, pages.len())?;
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        let png = p.image.encode_png()?;
        std::fs::write(&path, png).map_err(|e| format!("{}: {e}", path.display()))?;
        written.push(path);
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_printf_patterns() {
        assert_eq!(
            expand_pattern("p-%03d.png", 7).as_deref(),
            Some("p-007.png")
        );
        assert_eq!(expand_pattern("p%d.png", 12).as_deref(), Some("p12.png"));
        assert_eq!(expand_pattern("p%3d", 5).as_deref(), Some("p  5"));
        assert_eq!(expand_pattern("100%%-%d", 2).as_deref(), Some("100%-2"));
        assert_eq!(expand_pattern("plain.png", 1), None);
        assert_eq!(expand_pattern("odd%s.png", 1), None);
        assert_eq!(expand_pattern("odd%123d.png", 1), None);
    }

    #[test]
    fn chooses_output_paths() {
        assert_eq!(output_path("a.png", 1, 1).unwrap(), PathBuf::from("a.png"));
        assert!(output_path("a.png", 1, 2).unwrap_err().contains("pattern"));
        assert_eq!(
            output_path("out/", 3, 9).unwrap(),
            PathBuf::from("out/page-003.png")
        );
        assert_eq!(
            output_path("o/p-%02d.png", 3, 9).unwrap(),
            PathBuf::from("o/p-03.png")
        );
    }
}
