// SPDX-License-Identifier: AGPL-3.0-or-later
//! The visible part of a document surface.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::geom::Point;

/// A document view, named as in Office.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ViewMode {
    /// Write: pages as printed (default).
    PrintLayout,
    /// Write: no pages, text wraps to the window.
    WebLayout,
    /// Write: Read Mode (columns, no editing chrome).
    ReadMode,
    /// Write: Outline.
    Outline,
    /// Write: Draft.
    Draft,
    /// Grid: Normal (default).
    Normal,
    /// Grid: Page Layout.
    PageLayout,
    /// Grid: Page Break Preview.
    PageBreakPreview,
    /// Deck: Normal with thumbnails (default).
    SlideNormal,
    /// Deck: Slide Sorter.
    SlideSorter,
    /// Deck: Notes Page.
    NotesPage,
    /// Deck: Reading View.
    ReadingView,
}

/// What a surface shows: its size, scroll position, zoom and device pixel ratio.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct Viewport {
    /// Width of the surface in CSS pixels.
    pub width_px: f32,
    /// Height of the surface in CSS pixels.
    pub height_px: f32,
    /// Scroll offset of the surface's top-left corner, in document CSS pixels at zoom 1.
    pub scroll: Point,
    /// Zoom factor (1.0 = 100 %).
    pub zoom: f32,
    /// Device pixels per CSS pixel.
    pub dpr: f32,
    /// The view.
    pub mode: ViewMode,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            width_px: 0.0,
            height_px: 0.0,
            scroll: Point::default(),
            zoom: 1.0,
            dpr: 1.0,
            mode: ViewMode::PrintLayout,
        }
    }
}

impl Viewport {
    /// Whether every number is finite and in range (positive zoom and DPR, non-negative size).
    #[must_use]
    pub fn is_valid(&self) -> bool {
        let finite = [
            self.width_px,
            self.height_px,
            self.scroll.x,
            self.scroll.y,
            self.zoom,
            self.dpr,
        ]
        .iter()
        .all(|v| v.is_finite());
        finite && self.width_px >= 0.0 && self.height_px >= 0.0 && self.zoom > 0.0 && self.dpr > 0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_numbers() {
        assert!(Viewport::default().is_valid());
        let vp = Viewport {
            width_px: 1440.0,
            height_px: 900.0,
            dpr: 2.0,
            ..Viewport::default()
        };
        assert!(vp.is_valid());
        assert!(!Viewport { zoom: 0.0, ..vp }.is_valid());
        assert!(
            !Viewport {
                dpr: f32::NAN,
                ..vp
            }
            .is_valid()
        );
        assert!(
            !Viewport {
                width_px: -1.0,
                ..vp
            }
            .is_valid()
        );
        assert!(
            !Viewport {
                scroll: Point::new(f32::INFINITY, 0.0),
                ..vp
            }
            .is_valid()
        );
    }
}
