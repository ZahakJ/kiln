// SPDX-License-Identifier: AGPL-3.0-or-later
//! Frame production (PLAN.md §3.6): `Frame{viewport} → SceneDelta`.
//!
//! A document model that paints implements [`FrameSource`]: given the regions a frame needs, it
//! brings its [`Scene`] up to date (tile slots for the whole document, tiles built for the send
//! region, the overlay) and hands out glyph-atlas uploads. [`FrameState`] (one per open document)
//! keeps the [`DeltaTracker`] that mirrors the painter's cache and turns the scene into the
//! smallest [`SceneDelta`].
//!
//! Regions, in document pixels: the *visible* rectangle is the viewport; the *send* region adds
//! one viewport on every side (`prefetch_margin = 1 viewport`), so scrolling paints from cached
//! tiles while Rust prepares the next ones; the *keep* region adds one and a half viewports, and
//! the painter drops tiles outside it (bounded memory without thrashing at the edge).

mod glyphs;

pub use glyphs::GlyphCache;

use kiln_common::Result;
use kiln_protocol::Viewport;
use kiln_scene::{AtlasUpdate, Bounds, DeltaTracker, Scene, SceneDelta};

/// Viewports added on every side of the visible rectangle for prefetching.
pub const PREFETCH_MARGIN: f32 = 1.0;
/// Viewports added on every side of the visible rectangle for keeping painted tiles.
pub const KEEP_MARGIN: f32 = 1.5;

/// What a frame needs from a [`FrameSource`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameRequest {
    /// The viewport.
    pub viewport: Viewport,
    /// Visible document rectangle.
    pub visible: Bounds,
    /// Region whose tiles must be built.
    pub send: Bounds,
    /// Region whose tiles the painter keeps.
    pub keep: Bounds,
    /// Device pixels per document pixel.
    pub scale: f32,
}

impl FrameRequest {
    /// The request for a viewport (`None` until the surface has been measured).
    #[must_use]
    pub fn for_viewport(viewport: &Viewport) -> Option<Self> {
        if !viewport.is_valid() || viewport.width_px <= 0.0 || viewport.height_px <= 0.0 {
            return None;
        }
        let w = viewport.width_px / viewport.zoom;
        let h = viewport.height_px / viewport.zoom;
        let visible = Bounds::new(viewport.scroll.x, viewport.scroll.y, w, h);
        Some(Self {
            viewport: *viewport,
            visible,
            send: visible.inflate(w * PREFETCH_MARGIN, h * PREFETCH_MARGIN),
            keep: visible.inflate(w * KEEP_MARGIN, h * KEEP_MARGIN),
            scale: viewport.zoom * viewport.dpr,
        })
    }
}

/// A document model that produces scenes.
pub trait FrameSource {
    /// Brings the scene up to date for `request` (lays out what changed, builds the tiles in
    /// `request.send`, refreshes the overlay).
    ///
    /// # Errors
    ///
    /// When the scene cannot be produced (never for the scratch document).
    fn prepare(&mut self, request: &FrameRequest) -> Result<()>;

    /// The scene of the last [`FrameSource::prepare`].
    fn scene(&self) -> &Scene;

    /// Glyph-atlas pixels the painter lacks: every page with `all` (after a reset), otherwise the
    /// pages changed since the last call.
    fn atlas_updates(&mut self, all: bool) -> Vec<AtlasUpdate>;

    /// Size of the whole surface before any viewport is known (for a first, empty frame).
    fn extent(&self) -> Bounds;
}

/// Per-document frame bookkeeping.
#[derive(Debug, Default)]
pub struct FrameState {
    tracker: DeltaTracker,
}

impl FrameState {
    /// Fresh state: the next frame resets the painter.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The painter lost its cache: the next frame resets it and resends everything.
    pub fn reset(&mut self) {
        self.tracker.reset();
    }

    /// Tiles the painter holds, as far as the core knows.
    #[must_use]
    pub fn cached_tiles(&self) -> usize {
        self.tracker.cached_tiles()
    }

    /// The next frame for `viewport`. Without a source (documents that do not paint yet) or
    /// without a measured viewport, the delta carries only the revision and the extent.
    ///
    /// # Errors
    ///
    /// What [`FrameSource::prepare`] reports.
    pub fn produce(
        &mut self,
        source: Option<&mut dyn FrameSource>,
        viewport: &Viewport,
        revision: u64,
    ) -> Result<SceneDelta> {
        let Some(source) = source else {
            return Ok(SceneDelta {
                revision,
                ..SceneDelta::default()
            });
        };
        let Some(request) = FrameRequest::for_viewport(viewport) else {
            return Ok(SceneDelta {
                revision,
                extent: source.extent(),
                ..SceneDelta::default()
            });
        };
        source.prepare(&request)?;
        let mut delta = self
            .tracker
            .delta(source.scene(), &request.send, &request.keep, revision);
        delta.atlas = source.atlas_updates(delta.reset);
        Ok(delta)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_protocol::geom::Point;

    #[test]
    fn regions_follow_the_viewport_zoom_and_dpr() {
        let vp = Viewport {
            width_px: 1000.0,
            height_px: 800.0,
            scroll: Point::new(10.0, 2000.0),
            zoom: 2.0,
            dpr: 1.5,
            ..Viewport::default()
        };
        let r = FrameRequest::for_viewport(&vp).unwrap();
        assert_eq!(r.visible, Bounds::new(10.0, 2000.0, 500.0, 400.0));
        assert_eq!(r.send, Bounds::new(-490.0, 1600.0, 1500.0, 1200.0));
        assert_eq!(r.keep, Bounds::new(-740.0, 1400.0, 2000.0, 1600.0));
        assert!((r.scale - 3.0).abs() < 1e-6);
        assert!(FrameRequest::for_viewport(&Viewport::default()).is_none());
    }

    #[test]
    fn documents_without_a_source_get_revision_only_frames() {
        let mut f = FrameState::new();
        let d = f.produce(None, &Viewport::default(), 7).unwrap();
        assert_eq!(d.revision, 7);
        assert!(d.tiles.is_empty() && !d.reset);
    }
}
