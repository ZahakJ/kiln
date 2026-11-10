// SPDX-License-Identifier: AGPL-3.0-or-later
//! Charts on Grid sheets (the model and editor, hosted by Grid): the charts read from the
//! file, inserted from a range, selected, moved, resized, edited with the `Chart*` commands and
//! deleted, with undo; their caches follow the cells; a save writes the charts that changed.
//!
//! A chart lives in two places: its anchor in the sheet's drawings (`kiln-sheet` meta, so
//! inserting, moving and deleting are model commands that shift with rows and columns and undo
//! with everything else) and its `ChartSpace` here, by part name. Edits of a chart's content are
//! steps of the session's history that keep the chart before and after.

use std::cell::RefCell;

use kiln_charts::edit::HostContext;
use kiln_charts::model::{ChartSpace, PartFormat};
use kiln_charts::text::TextMeasurer;
use kiln_common::{KilnError, Result};
use kiln_ooxml::drawingml::theme::Theme;
use kiln_protocol::SuiteCommand;
use kiln_protocol::commands::{ChartMoveTarget, GridDrawing, GridHit};
use kiln_scene::{Bounds, DrawOp};
use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{DrawingAnchor, DrawingKind};
use kiln_sheet::{Range, SheetId};
use kiln_sheet_layout::{ChartLook, ChartTile, anchor_for, anchor_rect, new_chart_anchor};
use rustc_hash::{FxHashMap, FxHashSet};

use super::GridDoc;
use crate::model::Change;

/// A chart edit in the history: the chart before and after.
#[derive(Debug, Clone)]
pub(crate) struct ChartEdit {
    pub part: String,
    pub before: ChartSpace,
    pub after: ChartSpace,
}

/// The charts of a workbook.
#[derive(Debug, Default)]
pub(crate) struct Charts {
    /// By part name (`/xl/charts/chart1.xml`).
    parts: FxHashMap<String, ChartSpace>,
    /// Bumped on every change of a chart, for its tile.
    versions: FxHashMap<String, u64>,
    next_version: u64,
    /// Charts to write on save (edited or new).
    changed: FxHashSet<String>,
    /// Charts whose caches were filled from the cells when the file opened (the file had none, or
    /// stale ones) but are not written yet: an untouched chart saves as read, and the first change
    /// of cells writes it with the caches it shows.
    shown_fresh: FxHashSet<String>,
    /// The workbook's DrawingML theme (accents and fonts of charts).
    theme: Option<Theme>,
    /// The app's dark theme.
    pub(crate) dark: bool,
    /// The selected chart: its anchor id on the active sheet.
    pub(crate) selected: Option<String>,
    /// Lays charts out for the chrome's state (hit tests, axis bounds).
    text: RefCell<Option<TextMeasurer>>,
}

impl Charts {
    /// The charts of a file read (every chart part that parses; the others are kept as read).
    pub(crate) fn from_source(source: Option<&kiln_io_xlsx::XlsxSource>) -> Self {
        let mut c = Self::default();
        let Some(src) = source else {
            return c;
        };
        for (name, chart) in src.charts() {
            if let Ok(chart) = chart {
                c.parts.insert(name, chart);
            }
        }
        let pkg = src.package();
        c.theme = pkg
            .part_names()
            .find(|n| {
                pkg.content_type(n)
                    == Some("application/vnd.openxmlformats-officedocument.theme+xml")
            })
            .and_then(|n| pkg.part(n).ok().flatten())
            .and_then(|b| kiln_ooxml::drawingml::theme::parse_theme_bytes(&b).ok());
        c
    }

    fn bump(&mut self, part: &str) {
        self.next_version += 1;
        self.versions.insert(part.to_owned(), self.next_version);
    }

    fn set(&mut self, part: &str, chart: ChartSpace) {
        self.parts.insert(part.to_owned(), chart);
        self.changed.insert(part.to_owned());
        self.bump(part);
    }

    /// Charts read from a file without an OPC source (`.ods`): all of them are written
    /// on save.
    #[cfg_attr(not(feature = "odf"), allow(dead_code))]
    pub(crate) fn load(&mut self, parts: Vec<(String, ChartSpace)>) {
        for (name, chart) in parts {
            self.set(&name, chart);
        }
    }

    /// Every chart by part name (a save as `.ods` writes them all).
    #[cfg_attr(not(feature = "odf"), allow(dead_code))]
    pub(crate) fn all(&self) -> Vec<(String, ChartSpace)> {
        let mut out: Vec<(String, ChartSpace)> = self
            .parts
            .iter()
            .map(|(p, c)| (p.clone(), c.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The charts a save writes.
    pub(crate) fn to_write(&self) -> Vec<(String, ChartSpace)> {
        let mut out: Vec<(String, ChartSpace)> = self
            .changed
            .iter()
            .filter_map(|p| self.parts.get(p).map(|c| (p.clone(), c.clone())))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// A part name no chart and no part of `source` uses.
    fn fresh_part(&self, format: PartFormat, source: Option<&kiln_io_xlsx::XlsxSource>) -> String {
        let stem = match format {
            PartFormat::Classic => "chart",
            PartFormat::ChartEx => "chartEx",
        };
        (1u32..)
            .map(|n| format!("/xl/charts/{stem}{n}.xml"))
            .find(|p| !self.parts.contains_key(p) && !source.is_some_and(|s| s.package().has(p)))
            .unwrap_or_else(|| format!("/xl/charts/{stem}.xml"))
    }

    /// The tiles of charts placed by [`GridDoc::chart_places`].
    pub(crate) fn tiles<'a>(&'a self, places: &[(u32, Bounds, u64, String)]) -> Vec<ChartTile<'a>> {
        places
            .iter()
            .filter_map(|(index, rect, version, part)| {
                Some(ChartTile {
                    index: *index,
                    rect: *rect,
                    version: *version,
                    chart: self.parts.get(part)?,
                })
            })
            .collect()
    }

    /// A chart by part name.
    pub(crate) fn part(&self, name: &str) -> Option<&ChartSpace> {
        self.parts.get(name)
    }

    /// The workbook's DrawingML theme.
    pub(crate) fn theme(&self) -> Option<&Theme> {
        self.theme.as_ref()
    }

    pub(crate) fn look(&self) -> ChartLook<'_> {
        ChartLook {
            theme: self.theme.as_ref(),
            dark: self.dark,
        }
    }

    /// The chrome's state of a chart drawn at `w` × `h`.
    fn state(&self, chart: &ChartSpace, w: f32, h: f32) -> kiln_protocol::commands::ChartState {
        let colors = self.look().colors();
        let mut text = self.text.borrow_mut();
        let text =
            text.get_or_insert_with(|| TextMeasurer::new(crate::apps::scratch::fonts::store()));
        let layout = kiln_charts::layout::layout_chart(chart, w, h, &colors, text);
        kiln_charts::state::chart_state(chart, &layout, &colors)
    }

    /// The selection outline of a chart at `rect` (sheet pixels), for the overlay.
    fn selection_ops(&self, chart: &ChartSpace, rect: Bounds) -> Vec<DrawOp> {
        let colors = self.look().colors();
        let mut text = self.text.borrow_mut();
        let text =
            text.get_or_insert_with(|| TextMeasurer::new(crate::apps::scratch::fonts::store()));
        let layout = kiln_charts::layout::layout_chart(chart, rect.w, rect.h, &colors, text);
        let mut ops = vec![DrawOp::PushTransform {
            matrix: [1.0, 0.0, 0.0, 1.0, rect.x, rect.y],
        }];
        ops.extend(kiln_charts::render::selection_ops(&layout, None));
        ops.push(DrawOp::PopTransform);
        ops
    }
}

/// The eight sizing handles of a rectangle, clockwise from the top-left corner.
fn handles(r: Bounds) -> [(f32, f32); 8] {
    let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
    let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    [
        (x0, y0),
        (xm, y0),
        (x1, y0),
        (x1, ym),
        (x1, y1),
        (xm, y1),
        (x0, y1),
        (x0, ym),
    ]
}

impl GridDoc {
    /// The charts of the active sheet with their drawing numbers.
    fn sheet_charts(&self) -> Vec<(u32, &DrawingAnchor)> {
        self.active_sheet()
            .map(|(_, s)| {
                s.meta()
                    .drawings
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| d.kind == DrawingKind::Chart && !d.hidden)
                    .filter(|(_, d)| self.charts.parts.contains_key(&d.reference))
                    .map(|(i, d)| (u32::try_from(i).unwrap_or(u32::MAX), d))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The charts, pictures and shapes of the active sheet a click can select.
    fn sheet_drawings(&self) -> Vec<(u32, &DrawingAnchor)> {
        self.active_sheet()
            .map(|(_, s)| {
                s.meta()
                    .drawings
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| !d.hidden)
                    .filter(|(_, d)| {
                        (d.kind == DrawingKind::Chart
                            && self.charts.parts.contains_key(&d.reference))
                            || super::features::objects::is_object(d.kind)
                    })
                    .map(|(i, d)| (u32::try_from(i).unwrap_or(u32::MAX), d))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The selected drawing (chart, picture or shape): its number and anchor.
    fn selected_drawing(&self) -> Option<(u32, &DrawingAnchor)> {
        let id = self.charts.selected.as_deref()?;
        self.sheet_drawings().into_iter().find(|(_, d)| d.id == id)
    }

    /// Where the active sheet's charts are: drawing number, rectangle, version, part.
    pub(crate) fn chart_places(
        &self,
        geo: &kiln_sheet_layout::SheetGeometry,
    ) -> Vec<(u32, Bounds, u64, String)> {
        self.sheet_charts()
            .into_iter()
            .map(|(index, d)| {
                let rect = anchor_rect(geo, &d.anchor);
                let size = (u64::from(rect.w.to_bits()) << 32) | u64::from(rect.h.to_bits());
                let version = self.charts.versions.get(&d.reference).copied().unwrap_or(0);
                (
                    index,
                    rect,
                    version.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ size,
                    d.reference.clone(),
                )
            })
            .collect()
    }

    /// The selected chart: drawing number, anchor and chart.
    fn selected_chart(&self) -> Option<(u32, &DrawingAnchor, &ChartSpace)> {
        let id = self.charts.selected.as_deref()?;
        self.sheet_charts()
            .into_iter()
            .find(|(_, d)| d.id == id)
            .and_then(|(i, d)| Some((i, d, self.charts.parts.get(&d.reference)?)))
    }

    /// The overlay of the selected chart, picture or shape (its frame and handles).
    pub(crate) fn chart_overlay(&self, geo: &kiln_sheet_layout::SheetGeometry) -> Vec<DrawOp> {
        match self.selected_chart() {
            Some((_, d, chart)) => self
                .charts
                .selection_ops(chart, anchor_rect(geo, &d.anchor)),
            None => match self.selected_drawing() {
                Some((_, d)) => self.object_overlay(d, geo),
                None => Vec::new(),
            },
        }
    }

    /// The selected chart for the chrome.
    pub(crate) fn drawing_state(&self) -> Option<GridDrawing> {
        let Some((index, d, chart)) = self.selected_chart() else {
            let (index, d) = self.selected_drawing()?;
            return Some(self.object_state(index, d));
        };
        let geo = self.geometry_ref();
        let r = anchor_rect(&geo, &d.anchor);
        Some(GridDrawing {
            index,
            name: d
                .name
                .clone()
                .unwrap_or_else(|| format!("Chart {}", index + 1)),
            x: r.x,
            y: r.y,
            width: r.w,
            height: r.h,
            kind: kiln_protocol::commands::grid_features::GridDrawingKind::Chart,
            chart: Some(self.charts.state(chart, r.w.max(1.0), r.h.max(1.0))),
            object: None,
        })
    }

    /// What a point of the surface hits among the charts (topmost first), if anything.
    pub(crate) fn hit_chart(&self, x: f32, y: f32) -> Option<GridHit> {
        let geo = self.geometry_ref();
        let hw = self.header_w(&geo);
        let (_, sx, sy) = geo.to_sheet(&self.last_view, x, y, hw)?;
        let z = self.last_view.zoom.max(0.01);
        // Handles: within 5 screen pixels.
        if let Some((index, d)) = self.selected_drawing() {
            let r = anchor_rect(&geo, &d.anchor);
            let reach = 5.0 / z;
            if let Some(h) = handles(r)
                .iter()
                .position(|(hx, hy)| (sx - hx).abs() <= reach && (sy - hy).abs() <= reach)
            {
                return Some(GridHit::DrawingHandle {
                    index,
                    handle: u8::try_from(h).unwrap_or(0),
                });
            }
        }
        // Thin objects (lines) are hit within a few screen pixels.
        let slack = 4.0 / z;
        self.sheet_drawings()
            .into_iter()
            .rev()
            .find(|(_, d)| {
                let r = anchor_rect(&geo, &d.anchor);
                let (sx0, sy0) = if r.w < 2.0 * slack {
                    (slack, 0.0)
                } else {
                    (0.0, 0.0)
                };
                let sy0 = if r.h < 2.0 * slack { slack } else { sy0 };
                sx >= r.x - sx0
                    && sx <= r.right() + sx0
                    && sy >= r.y - sy0
                    && sy <= r.bottom() + sy0
            })
            .map(|(index, _)| GridHit::Drawing { index })
    }

    fn drawings_of(&self, sid: SheetId) -> Vec<DrawingAnchor> {
        self.workbook
            .sheet(sid)
            .map(|s| s.meta().drawings.clone())
            .unwrap_or_default()
    }

    fn set_drawings(
        &mut self,
        label: &'static str,
        sid: SheetId,
        list: Vec<DrawingAnchor>,
    ) -> Result<()> {
        self.exec(
            label,
            kiln_sheet::Command::SetSheetMeta {
                sheet: sid,
                field: Box::new(MetaField::Drawings(list)),
            },
        )?;
        Ok(())
    }

    /// `SelectDrawing`.
    pub(crate) fn select_drawing(&mut self, index: Option<u32>) -> Result<Change> {
        let id = match index {
            None => None,
            Some(i) => Some(
                self.sheet_drawings()
                    .into_iter()
                    .find(|(n, _)| *n == i)
                    .map(|(_, d)| d.id.clone())
                    .ok_or_else(|| {
                        KilnError::InvalidInput(format!("there is no chart {i} on this sheet"))
                    })?,
            ),
        };
        if id.is_some() {
            // Selecting a chart ends an entry and the copy marquee stays.
            self.edit = None;
        }
        self.charts.selected = id;
        self.selection_epoch += 1;
        Ok(Change::Selection)
    }

    /// `PlaceDrawing`: a move or resize, one undo step.
    pub(crate) fn place_drawing(&mut self, index: u32, rect: Bounds) -> Result<Change> {
        if !(rect.x.is_finite() && rect.y.is_finite() && rect.w.is_finite() && rect.h.is_finite()) {
            return Err(KilnError::InvalidInput(
                "the chart's place is not a number".into(),
            ));
        }
        let sid = self.sid()?;
        let geo = self.geometry_ref();
        let mut list = self.drawings_of(sid);
        let i = usize::try_from(index).unwrap_or(usize::MAX);
        let d = list
            .get_mut(i)
            .filter(|d| d.kind == DrawingKind::Chart || super::features::objects::is_object(d.kind))
            .ok_or_else(|| {
                KilnError::InvalidInput(format!("there is no chart {index} on this sheet"))
            })?;
        // Pictures and shapes that keep their proportions follow the side dragged most.
        let rect = if d.look.lock_aspect && d.kind != DrawingKind::Chart {
            let old = anchor_rect(&geo, &d.anchor);
            if old.w > 0.0
                && old.h > 0.0
                && ((rect.w - old.w).abs() > 0.01 || (rect.h - old.h).abs() > 0.01)
            {
                let ratio = old.w / old.h;
                if (rect.w - old.w).abs() / old.w >= (rect.h - old.h).abs() / old.h {
                    Bounds::new(rect.x, rect.y, rect.w, rect.w / ratio)
                } else {
                    Bounds::new(rect.x, rect.y, rect.h * ratio, rect.h)
                }
            } else {
                rect
            }
        } else {
            rect
        };
        let max_w = geo.cols.total() as f32;
        let max_h = geo.rows.total() as f32;
        let r = Bounds::new(
            rect.x.clamp(0.0, (max_w - rect.w.max(1.0)).max(0.0)),
            rect.y.clamp(0.0, (max_h - rect.h.max(1.0)).max(0.0)),
            rect.w.min(max_w),
            rect.h.min(max_h),
        );
        let anchor = anchor_for(&geo, r, &d.anchor);
        if anchor == d.anchor {
            return Ok(Change::None);
        }
        d.anchor = anchor;
        let part = d.reference.clone();
        let chart = d.kind == DrawingKind::Chart;
        self.charts.selected = Some(d.id.clone());
        self.set_drawings("grid.drawing.place", sid, list)?;
        if chart {
            self.charts.bump(&part);
        }
        Ok(Change::Content)
    }

    /// `DeleteDrawing` (and Delete with a chart selected).
    pub(crate) fn delete_drawing(&mut self) -> Result<Change> {
        let Some(id) = self.charts.selected.clone() else {
            return Ok(Change::None);
        };
        let sid = self.sid()?;
        let mut list = self.drawings_of(sid);
        let before = list.len();
        list.retain(|d| d.id != id);
        self.charts.selected = None;
        if list.len() == before {
            return Ok(Change::Selection);
        }
        self.set_drawings("grid.drawing.delete", sid, list)?;
        Ok(Change::Content)
    }

    /// `Theme`: the app's light or dark theme (repaints the charts).
    pub(crate) fn set_dark(&mut self, dark: bool) -> Change {
        if self.charts.dark == dark {
            return Change::None;
        }
        self.charts.dark = dark;
        Change::Selection
    }

    /// The selected range for Insert ▸ Chart: a single cell stands for its region (Excel).
    fn chart_source_range(&self) -> Result<(String, Range)> {
        let (_, sheet) = self
            .active_sheet()
            .ok_or_else(|| KilnError::InvalidInput("there is no sheet".into()))?;
        let name = sheet.name().to_owned();
        let v = self.view();
        let r = v
            .sel
            .ranges
            .last()
            .copied()
            .ok_or_else(|| KilnError::InvalidInput("select the cells to chart first".into()))?;
        let r = if r.is_single_cell() {
            self.current_region(v.sel.active).unwrap_or(r)
        } else {
            r
        };
        // Whole rows and columns chart their used part.
        let used = sheet.cells().used_range(false).unwrap_or(r);
        let r = Range::new(
            r.r0.max(used.r0.min(r.r1)),
            r.c0.max(used.c0.min(r.c1)),
            r.r1.min(used.r1.max(r.r0)),
            r.c1.min(used.c1.max(r.c0)),
        );
        Ok((name, r))
    }

    /// Recommended Charts for the selection, best first (none for a selection without data).
    pub(crate) fn recommended_charts(&self) -> Vec<kiln_protocol::commands::ChartKind> {
        let Ok((sheet, range)) = self.chart_source_range() else {
            return Vec::new();
        };
        kiln_charts::data::analyse_range(&*self.workbook, &sheet, range)
            .map(|shape| kiln_charts::edit::recommend(&*self.workbook, &shape))
            .unwrap_or_default()
    }

    /// Insert ▸ Chart: a chart of the selection in the middle of the view, selected.
    fn insert_chart(&mut self, cmd: &SuiteCommand) -> Result<Change> {
        let sid = self.sid()?;
        let (sheet, range) = self.chart_source_range()?;
        let mut chart = kiln_charts::edit::sample_chart(None);
        kiln_charts::edit::apply(
            &mut chart,
            cmd,
            HostContext {
                data: Some(&*self.workbook),
                selection: Some((&sheet, range)),
            },
        )?;
        let geo = self.geometry_ref();
        let hw = self.header_w(&geo);
        let view = self.last_view;
        let z = view.zoom.max(0.01);
        let (w, h) = kiln_sheet_layout::drawings::NEW_CHART_SIZE;
        let cells_w = (view.width / z - hw - geo.frozen_w()).max(w);
        let cells_h = (view.height / z - geo.header_h() - geo.frozen_h()).max(h);
        let rect = Bounds::new(
            view.scroll_x + ((cells_w - w) / 2.0).max(0.0),
            view.scroll_y + ((cells_h - h) / 2.0).max(0.0),
            w,
            h,
        );
        let part = self.charts.fresh_part(chart.format, self.source.as_ref());
        let mut list = self.drawings_of(sid);
        let n = list.len() + 1;
        let id = (1u32..)
            .map(|k| format!("kiln-chart-{k}"))
            .find(|k| !list.iter().any(|d| &d.id == k))
            .unwrap_or_else(|| "kiln-chart".into());
        list.push(DrawingAnchor {
            id: id.clone(),
            kind: DrawingKind::Chart,
            reference: part.clone(),
            anchor: new_chart_anchor(&geo, rect),
            name: Some(format!("Chart {n}")),
            alt_text: None,
            hidden: false,
            locked: true,
            look: Default::default(),
        });
        self.log
            .record(super::storage::GridOp::chart(&part, &chart));
        self.charts.set(&part, chart);
        self.set_drawings("suite.chart.insert", sid, list)?;
        self.charts.selected = Some(id);
        self.edit = None;
        Ok(Change::Content)
    }

    /// The `Chart*` commands of Chart Design, Format and the Format pane.
    pub(crate) fn chart_command(&mut self, cmd: &SuiteCommand) -> Result<Change> {
        if matches!(cmd, SuiteCommand::ChartInsert { .. }) {
            return self.insert_chart(cmd);
        }
        let Some((_, anchor, chart)) = self.selected_chart() else {
            return Err(KilnError::unavailable(
                cmd.meta().id,
                "select a chart first",
            ));
        };
        let part = anchor.reference.clone();
        let before = chart.clone();
        if let SuiteCommand::ChartMove { to } = cmd {
            return self.move_chart(to);
        }
        let mut after = before.clone();
        let sheet = self
            .active_sheet()
            .map(|(_, s)| s.name().to_owned())
            .unwrap_or_default();
        let range = self.view().sel.ranges.last().copied();
        let changed = kiln_charts::edit::apply(
            &mut after,
            cmd,
            HostContext {
                data: Some(&*self.workbook),
                selection: range.map(|r| (sheet.as_str(), r)),
            },
        )?;
        if !changed || after == before {
            return Ok(Change::None);
        }
        self.charts.set(&part, after.clone());
        self.record_chart_step(
            cmd.meta().id,
            ChartEdit {
                part,
                before,
                after,
            },
        );
        Ok(Change::Content)
    }

    /// Chart Design ▸ Move Chart: to another sheet as an object, or to a new chart sheet placed
    /// before the current sheet (as Excel does).
    fn move_chart(&mut self, to: &ChartMoveTarget) -> Result<Change> {
        let from = self.sid()?;
        let Some(id) = self.charts.selected.clone() else {
            return Ok(Change::None);
        };
        let mut src = self.drawings_of(from);
        let Some(pos) = src.iter().position(|d| d.id == id) else {
            return Ok(Change::None);
        };
        match to {
            ChartMoveTarget::ObjectIn { sheet } => {
                let target = self
                    .workbook
                    .sheets()
                    .iter()
                    .position(|s| s.name().eq_ignore_ascii_case(sheet))
                    .ok_or_else(|| {
                        KilnError::InvalidInput(format!("there is no sheet “{sheet}”"))
                    })?;
                let to_id = self.workbook.sheets()[target].id();
                if to_id == from {
                    return Ok(Change::None);
                }
                if self.workbook.sheets()[target].meta().chart_sheet {
                    return Err(KilnError::InvalidInput(format!(
                        "“{sheet}” is a chart sheet: it shows one chart only"
                    )));
                }
                let mut d = src.remove(pos);
                // From a chart sheet the chart becomes an object of Excel's default size at A1.
                if self
                    .active_sheet()
                    .is_some_and(|(_, s)| s.meta().chart_sheet)
                {
                    d.anchor = kiln_sheet::meta::Anchor::OneCell {
                        from: kiln_sheet::meta::AnchorPoint::default(),
                        ext: (4_572_000, 2_743_200),
                    };
                }
                self.charts.changed.insert(d.reference.clone());
                let mut dst = self.drawings_of(to_id);
                dst.push(d);
                let mut cmds = vec![
                    kiln_sheet::Command::SetSheetMeta {
                        sheet: from,
                        field: Box::new(MetaField::Drawings(src)),
                    },
                    kiln_sheet::Command::SetSheetMeta {
                        sheet: to_id,
                        field: Box::new(MetaField::Drawings(dst)),
                    },
                ];
                // A chart sheet left without its chart goes, as in Excel.
                let emptied = self
                    .active_sheet()
                    .is_some_and(|(_, s)| s.meta().chart_sheet && s.meta().drawings.len() == 1);
                if emptied {
                    cmds.push(kiln_sheet::Command::RemoveSheet { sheet: from });
                }
                self.exec_all("suite.chart.move", cmds)?;
                if let Some(i) = self.workbook.sheets().iter().position(|s| s.id() == to_id) {
                    self.active = i;
                }
            }
            ChartMoveTarget::NewSheet { name } => {
                let name = name.trim();
                kiln_sheet::workbook::check_sheet_name(name)
                    .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
                if self
                    .workbook
                    .sheets()
                    .iter()
                    .any(|s| s.name().eq_ignore_ascii_case(name))
                {
                    return Err(KilnError::InvalidInput(format!(
                        "a sheet called “{name}” already exists: type another name"
                    )));
                }
                let new_id = self
                    .workbook
                    .sheets()
                    .iter()
                    .map(|s| s.id().0)
                    .max()
                    .unwrap_or(0)
                    + 1;
                let index = self.active;
                let mut d = src.remove(pos);
                d.anchor = kiln_sheet::meta::Anchor::Absolute {
                    pos: (0, 0),
                    ext: kiln_sheet::meta::CHART_SHEET_EXT,
                };
                self.charts.changed.insert(d.reference.clone());
                let view = kiln_sheet::meta::SheetView {
                    show_gridlines: false,
                    show_headings: false,
                    ..kiln_sheet::meta::SheetView::default()
                };
                let sheet = kiln_sheet::SheetId(new_id);
                self.exec_all(
                    "suite.chart.move",
                    vec![
                        kiln_sheet::Command::SetSheetMeta {
                            sheet: from,
                            field: Box::new(MetaField::Drawings(src)),
                        },
                        kiln_sheet::Command::AddSheet {
                            name: Some(name.to_owned()),
                            index: Some(index),
                            id: Some(new_id),
                        },
                        kiln_sheet::Command::SetSheetMeta {
                            sheet,
                            field: Box::new(MetaField::ChartSheet(true)),
                        },
                        kiln_sheet::Command::SetSheetMeta {
                            sheet,
                            field: Box::new(MetaField::View(view)),
                        },
                        kiln_sheet::Command::SetSheetMeta {
                            sheet,
                            field: Box::new(MetaField::Drawings(vec![d])),
                        },
                    ],
                )?;
                self.active = index;
            }
        }
        self.charts.selected = Some(id);
        Ok(Change::Content)
    }

    /// Restores a chart from the history (undo and redo of a chart step).
    pub(crate) fn restore_chart(&mut self, part: &str, chart: ChartSpace) {
        self.charts.set(part, chart);
    }

    /// Refreshes the caches of every chart from the cells (after edits and recalculation).
    pub(crate) fn refresh_charts(&mut self) {
        let parts: Vec<String> = self.charts.parts.keys().cloned().collect();
        for p in parts {
            let refreshed = self
                .charts
                .parts
                .get_mut(&p)
                .is_some_and(|c| kiln_charts::data::refresh(c, &*self.workbook));
            let changed = self.charts.shown_fresh.remove(&p) || refreshed;
            if changed {
                self.charts.changed.insert(p.clone());
                self.charts.bump(&p);
            }
        }
    }

    /// Fills the charts' caches from the cells for display, when the file opens: files written
    /// without caches (openpyxl, for one) would otherwise show empty charts until a cell changes.
    /// Nothing is marked for saving, so an untouched chart part saves byte for byte.
    pub(crate) fn show_fresh_charts(&mut self) {
        let parts: Vec<String> = self.charts.parts.keys().cloned().collect();
        for p in parts {
            let refreshed = self
                .charts
                .parts
                .get_mut(&p)
                .is_some_and(|c| kiln_charts::data::refresh(c, &*self.workbook));
            if refreshed {
                self.charts.shown_fresh.insert(p.clone());
                self.charts.bump(&p);
            }
        }
    }

    /// The parts a save writes (edited or new charts), sorted.
    #[cfg(test)]
    pub(crate) fn chart_parts_to_write(&self) -> Vec<String> {
        self.charts.to_write().into_iter().map(|(p, _)| p).collect()
    }

    /// A chart as shown, by part name.
    #[cfg(test)]
    pub(crate) fn chart_part(&self, part: &str) -> Option<&ChartSpace> {
        self.charts.parts.get(part)
    }

    /// Forgets the selection of a chart that is no longer on the active sheet.
    pub(crate) fn validate_chart_selection(&mut self) {
        if let Some(id) = &self.charts.selected
            && !self.sheet_drawings().iter().any(|(_, d)| &d.id == id)
        {
            self.charts.selected = None;
        }
    }
}
