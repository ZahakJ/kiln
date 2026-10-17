// SPDX-License-Identifier: AGPL-3.0-or-later
//! # kiln-sheet-layout
//!
//! Kiln Grid's surface (PLAN.md §3.6 "Grid specifics"): sheet geometry and painting into the
//! Scene, so the grid, `kiln render` PNGs and print previews draw the same pixels.
//!
//! - [`axis`]: row and column offsets over up to 1,048,576 rows in O(log n) — a Fenwick tree
//!   over blocks of sizes, hidden rows and columns at size 0, live resizing.
//! - [`geom`]: cell and range rectangles, merges, the four panes of frozen views (each a
//!   viewport onto one sheet space, sharing one tile cache), row and column headers, hit testing
//!   (cells, headers, resize borders, the fill handle, the corner) and scrolling cells into view.
//! - [`look`] and [`display`]: a cell's format resolved for painting (table styles under the
//!   cell format, conditional formats over it, themed automatic colours for dark mode) and its
//!   displayed text (General sized to the column, `#####`, Accounting's fill).
//! - [`paint`]: fills (solid, the 17 patterns, gradients), gridlines, borders with Excel's
//!   precedence ([`border`]) and all 13 line styles, text with alignment, indent, wrap, shrink to
//!   fit, rotation and overflow into empty neighbours, merged cells, data bars and icon sets
//!   ([`icons`]), hyperlink styling, note, comment and error-check markers.
//! - [`headers`]: row and column headers with the selection highlight, and the corner.
//! - [`scene`]: 256 px tiles for every region a frame shows, with versions so only changed tiles
//!   are repainted and sent.
//! - [`overlay`]: the selection, fill handle, copy marquee, fill preview, spill range, the
//!   colours of a formula's references and the in-cell editor.
//! - [`a11y`]: the accessibility tree of the visible grid.
//! - [`render`]: an area of a sheet as one self-contained Scene delta (PNGs, golden tests).
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod a11y;
pub mod axis;
pub mod border;
pub mod celltext;
pub mod display;
pub mod geom;
pub mod glyphs;
pub mod headers;
pub mod icons;
pub mod look;
pub mod overlay;
pub mod paint;
pub mod render;
pub mod scene;

pub use axis::AxisIndex;
pub use geom::{HitTarget, Pane, SheetGeometry, Snap, View};
pub use headers::{HeaderMarks, col_letters};
pub use paint::ViewOptions;
pub use render::{RenderOptions, SheetRender, geometry_of, render_sheet, view_options};
pub use scene::{GridScene, Region, RegionKind, SceneRequest};

/// The version of this crate. Every Kiln crate shares the suite version (`[workspace.package]`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(test)]
mod tests {
    #[test]
    fn crate_version_is_the_suite_version() {
        assert_eq!(super::VERSION, kiln_common::VERSION);
    }
}
