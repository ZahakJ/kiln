// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Grid surface: the view of each sheet (selection, scroll), the frames (tiles for every
//! pane and header band, the overlay with the selection and the in-cell editor) and the
//! accessibility tree of the visible cells.

use kiln_common::Result;
use kiln_protocol::A11yTree;
use kiln_protocol::geom::Rect;
use kiln_scene::{AtlasUpdate, Bounds, Scene};
use kiln_sheet::meta::{Selection as SheetSelection, SheetView as SheetViewMeta};
use kiln_sheet::{CellPos, MAX_COLS, MAX_ROWS, Range};
use kiln_sheet_layout::a11y::{A11yCell, grid_tree};
use kiln_sheet_layout::overlay::{
    OverlayInput, editor_ops, formula_refs, layout_editor, overlay_ops,
};
use kiln_sheet_layout::{
    GridScene, HeaderMarks, SceneRequest, SheetGeometry, Snap, View, geometry_of, view_options,
};

use super::GridDoc;
use crate::frame::{FrameRequest, FrameSource};

/// The selection of a sheet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sel {
    /// Ranges in selection order; the last holds the active cell.
    pub ranges: Vec<Range>,
    /// The active cell.
    pub active: CellPos,
}

impl Default for Sel {
    fn default() -> Self {
        Self {
            ranges: vec![Range::cell(0, 0)],
            active: CellPos::new(0, 0),
        }
    }
}

/// The view of one sheet.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ViewState {
    /// Selection.
    pub sel: Sel,
    /// The moving corner of the last range (Shift+arrows extend from it).
    pub head: CellPos,
}

impl ViewState {
    /// The view a file saved.
    #[must_use]
    pub fn from_meta(meta: &SheetViewMeta) -> Self {
        let SheetSelection { ranges, active } = &meta.selection;
        let ranges: Vec<Range> = ranges
            .iter()
            .filter(|r| r.r1 < MAX_ROWS && r.c1 < MAX_COLS)
            .copied()
            .collect();
        let active = if active.row < MAX_ROWS && active.col < MAX_COLS {
            *active
        } else {
            CellPos::new(0, 0)
        };
        let mut sel = Sel {
            ranges: if ranges.is_empty() {
                vec![Range::cell(active.row, active.col)]
            } else {
                ranges
            },
            active,
        };
        // The active cell's range goes last.
        if let Some(i) = sel
            .ranges
            .iter()
            .position(|r| r.contains(active.row, active.col))
        {
            let r = sel.ranges.remove(i);
            sel.ranges.push(r);
        } else {
            sel.ranges = vec![Range::cell(active.row, active.col)];
        }
        let last = sel.ranges[sel.ranges.len() - 1];
        let head = if last.r0 == active.row && last.c0 == active.col {
            CellPos::new(last.r1, last.c1)
        } else {
            CellPos::new(last.r0, last.c0)
        };
        Self { sel, head }
    }
}

impl GridDoc {
    /// The geometry of the active sheet (built on demand).
    pub(crate) fn geometry(&mut self) -> &SheetGeometry {
        if self.geo.is_none() {
            let headings = self
                .active_sheet()
                .is_none_or(|(_, s)| s.meta().view.show_headings);
            let g = self.active_sheet().map(|(_, s)| geometry_of(s, headings));
            self.geo = g.or_else(|| {
                Some(SheetGeometry {
                    rows: kiln_sheet_layout::AxisIndex::new(MAX_ROWS, 20.0),
                    cols: kiln_sheet_layout::AxisIndex::new(MAX_COLS, 64.0),
                    freeze_rows: 0,
                    freeze_cols: 0,
                    headings,
                    merges: Vec::new(),
                })
            });
        }
        // Set above.
        self.geo
            .as_ref()
            .unwrap_or_else(|| unreachable!("geometry is built above"))
    }

    /// The geometry without building it (queries are `&self`).
    pub(crate) fn geometry_ref(&self) -> SheetGeometry {
        match &self.geo {
            Some(g) => g.clone(),
            None => self
                .active_sheet()
                .map(|(_, s)| geometry_of(s, s.meta().view.show_headings))
                .unwrap_or_else(|| SheetGeometry {
                    rows: kiln_sheet_layout::AxisIndex::new(MAX_ROWS, 20.0),
                    cols: kiln_sheet_layout::AxisIndex::new(MAX_COLS, 64.0),
                    freeze_rows: 0,
                    freeze_cols: 0,
                    headings: true,
                    merges: Vec::new(),
                }),
        }
    }

    /// The row header width for the last view.
    pub(crate) fn header_w(&self, geo: &SheetGeometry) -> f32 {
        let used = self
            .active_sheet()
            .and_then(|(_, s)| s.cells().used_range(false))
            .map_or(0, |r| r.r1);
        geo.header_w(geo.last_visible_row(&self.last_view).max(used).max(999))
    }

    /// The scrollable size of the cells: the used range plus a screen, and at least a screen
    /// past the scroll position (Excel's scroll bars grow as you go).
    pub(crate) fn content_extent(&self, geo: &SheetGeometry, view: &View) -> (f32, f32) {
        let used = self
            .active_sheet()
            .and_then(|(_, s)| s.cells().used_range(false))
            .unwrap_or_else(|| Range::cell(0, 0));
        let z = view.zoom.max(0.01);
        let (vw, vh) = (view.width / z, view.height / z);
        let total_w = geo.cols.total() as f32;
        let total_h = geo.rows.total() as f32;
        let sel = self.view().sel;
        let active = geo.cell_rect(sel.active.row, sel.active.col);
        let w = (geo.cols.offset(used.c1 + 1) as f32 + vw)
            .max(view.scroll_x + 2.0 * vw)
            .max(active.right() + vw)
            .min(total_w);
        let h = (geo.rows.offset(used.r1 + 1) as f32 + vh)
            .max(view.scroll_y + 2.0 * vh)
            .max(active.bottom() + vh)
            .min(total_h);
        (w, h)
    }

    fn overlay_input(&self) -> OverlayInput {
        let v = self.view();
        let sheet_name = self
            .active_sheet()
            .map(|(_, s)| s.name().to_owned())
            .unwrap_or_default();
        let refs = self
            .edit
            .as_ref()
            .map(|e| {
                formula_refs(&e.text, &sheet_name)
                    .into_iter()
                    .filter_map(|r| r.range.map(|g| (g, r.color)))
                    .collect()
            })
            .unwrap_or_default();
        let sid = self.sid().ok();
        let spill = sid.and_then(|sid| {
            self.workbook
                .spill_range(sid, v.sel.active.row, v.sel.active.col)
                .filter(|s| !s.blocked)
                .map(|s| Range::from_bounds(s.bounds))
                .filter(|r| !r.is_single_cell())
        });
        // A selected chart takes the selection's place (Excel hides the cell cursor).
        let chart = self.charts.selected.is_some();
        OverlayInput {
            ranges: if chart {
                Vec::new()
            } else {
                v.sel.ranges.clone()
            },
            active: v.sel.active,
            fill_handle: self.edit.is_none() && !chart,
            copy: self.copy.filter(|c| Some(c.sheet) == sid).map(|c| c.range),
            fill_preview: self.fill_preview,
            spill: if self.edit.is_none() { spill } else { None },
            refs,
            editing: self.edit.as_ref().is_some_and(|e| e.pointing()),
        }
    }

    /// Lays out the in-cell editor (sheet pixels), for the overlay and for the chrome's popovers.
    pub(crate) fn editor_layout(&mut self) -> Option<kiln_sheet_layout::overlay::Editor> {
        let edit = self.edit.clone()?;
        let geo = self.geometry().clone();
        let sheet_name = self
            .active_sheet()
            .map(|(_, s)| s.name().to_owned())
            .unwrap_or_default();
        let (_, sheet) = self.active_sheet()?;
        let style = sheet.style_at(edit.row, edit.col);
        let colors = kiln_sheet_layout::look::Colors {
            theme: self.workbook.theme(),
        };
        let look = kiln_sheet_layout::look::resolve(
            colors,
            &self.workbook.styles().get(style),
            None,
            None,
        );
        let cell = geo.display_rect(edit.row, edit.col);
        let view = self.last_view;
        let hw = self.header_w(&geo);
        let z = view.zoom.max(0.01);
        // The editor may grow to the right edge of the surface.
        let (sx, _) = geo.to_surface(&view, cell.x, cell.y, hw);
        let max_right = cell.x + ((view.width - sx) / z).max(cell.w);
        let refs = formula_refs(&edit.text, &sheet_name);
        let (start, end) = edit.byte_selection();
        let scene = self
            .scene
            .get_or_insert_with(|| GridScene::new(crate::apps::scratch::fonts::store()));
        Some(layout_editor(
            scene.text_engine(),
            &edit.display_text(),
            (start, end),
            cell,
            &look,
            look.align.wrap_text,
            max_right,
            &refs,
        ))
    }

    /// The accessibility tree of the visible cells.
    pub(crate) fn a11y_tree(&self, title: &str) -> A11yTree {
        let geo = self.geometry_ref();
        let Some((_, sheet)) = self.active_sheet() else {
            return A11yTree::default();
        };
        let sid = sheet.id();
        let view = self.last_view;
        let hw = self.header_w(&geo);
        let mut rows: Vec<u32> = Vec::new();
        let mut cols: Vec<u32> = Vec::new();
        let regions = if view.width > 0.0 && view.height > 0.0 {
            geo.visible_regions(&view, hw)
        } else {
            vec![(
                kiln_sheet_layout::Pane::BottomRight,
                Bounds::new(0.0, 0.0, 1024.0, 600.0),
            )]
        };
        for (_, b) in regions {
            let (r0, r1) = geo.rows.indices_in(f64::from(b.y), f64::from(b.bottom()));
            let (c0, c1) = geo.cols.indices_in(f64::from(b.x), f64::from(b.right()));
            rows.extend((r0..=r1.min(r0 + 120)).filter(|&r| !geo.rows.is_hidden(r)));
            cols.extend((c0..=c1.min(c0 + 40)).filter(|&c| !geo.cols.is_hidden(c)));
        }
        rows.sort_unstable();
        rows.dedup();
        cols.sort_unstable();
        cols.dedup();
        let mut cells = Vec::new();
        for &r in &rows {
            for &c in &cols {
                let text = self.workbook.display_text(sid, r, c);
                if text.is_empty() {
                    continue;
                }
                let b = geo.cell_rect(r, c);
                cells.push(A11yCell {
                    at: CellPos::new(r, c),
                    text,
                    bounds: Rect::new(b.x, b.y, b.w, b.h),
                });
            }
        }
        let v = self.view();
        let active = v.sel.active;
        let addr = kiln_sheet::address::cell_a1(active.row, active.col, false);
        let shown = self.workbook.display_text(sid, active.row, active.col);
        let formula = self.workbook.formula(sid, active.row, active.col);
        let mut announce = addr;
        if !shown.is_empty() {
            announce.push_str(", ");
            announce.push_str(&shown);
        }
        if let Some(f) = formula {
            announce.push_str(", formula ");
            if !f.starts_with('=') {
                announce.push('=');
            }
            announce.push_str(&f);
        }
        grid_tree(
            sheet.name(),
            title,
            &rows,
            &cols,
            &cells,
            active,
            Some(announce),
        )
    }
}

impl FrameSource for GridDoc {
    fn prepare(&mut self, request: &FrameRequest) -> Result<()> {
        let vp = request.viewport;
        self.geometry();
        let geo = self.geo.clone().unwrap_or_else(|| self.geometry_ref());
        let (sx, sy) = geo.clamp_scroll(vp.scroll.x, vp.scroll.y);
        let view = View {
            width: vp.width_px,
            height: vp.height_px,
            scroll_x: sx,
            scroll_y: sy,
            zoom: vp.zoom,
        };
        self.last_view = view;
        let hw = self.header_w(&geo);
        let extent = self.content_extent(&geo, &view);
        let v = self.view();
        let marks = HeaderMarks {
            ranges: if self.charts.selected.is_some() {
                Vec::new()
            } else {
                v.sel.ranges.clone()
            },
        };
        let all = v
            .sel
            .ranges
            .iter()
            .any(|r| r.is_whole_cols() && r.is_whole_rows());
        let snap = Snap(request.scale);
        let mut overlay = overlay_ops(&geo, &self.overlay_input(), snap);
        overlay.extend(self.chart_overlay(&geo));
        let places = self.chart_places(&geo);
        let editor = self.editor_layout();
        let Some((index, sheet)) = self.active_sheet() else {
            return Ok(());
        };
        let opts = view_options(&self.workbook, sheet);
        let scene = self
            .scene
            .get_or_insert_with(|| GridScene::new(crate::apps::scratch::fonts::store()));
        if let Some(ed) = &editor {
            // The glyphs are rasterised at the frame's scale (set by `prepare` below on a new
            // scale; set it first so the editor's glyphs match).
            let (glyphs, text) = scene.glyphs_and_text();
            glyphs.set_scale(request.scale);
            editor_ops(ed, glyphs, text, &mut overlay);
        }
        let charts = self.charts.tiles(&places);
        scene.prepare(
            &mut self.workbook,
            index,
            &geo,
            SceneRequest {
                view,
                scale: request.scale,
                header_w: hw,
                extent,
                marks: &marks,
                all_selected: all,
                opts: &opts,
                overlay,
                charts: &charts,
                chart_look: self.charts.look(),
            },
        );
        Ok(())
    }

    fn scene(&self) -> &Scene {
        static EMPTY: std::sync::OnceLock<Scene> = std::sync::OnceLock::new();
        match &self.scene {
            Some(s) => s.scene(),
            None => EMPTY.get_or_init(|| {
                Scene::new(
                    1.0,
                    Bounds::ZERO,
                    kiln_scene::Paint::Role(kiln_scene::ColorRole::Page),
                )
            }),
        }
    }

    fn atlas_updates(&mut self, all: bool) -> Vec<AtlasUpdate> {
        self.scene
            .as_mut()
            .map(|s| s.atlas_updates(all))
            .unwrap_or_default()
    }

    fn extent(&self) -> Bounds {
        let geo = self.geometry_ref();
        let hw = self.header_w(&geo);
        let (w, h) = self.content_extent(
            &geo,
            &View {
                width: 1440.0,
                height: 900.0,
                ..self.last_view
            },
        );
        Bounds::new(-hw, -geo.header_h(), w + hw, h + geo.header_h())
    }

    fn extra_regions(&self, _request: &FrameRequest) -> Vec<(Bounds, Bounds)> {
        self.scene
            .as_ref()
            .map(|s| s.regions().iter().map(|r| (r.send, r.keep)).collect())
            .unwrap_or_default()
    }
}
