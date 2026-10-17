// SPDX-License-Identifier: AGPL-3.0-or-later
//! The Write document's view: its layout (`kiln-doc-layout`), kept up to date with the edits
//! since the last frame, the Scene of the pages near the viewport, the overlay (selection,
//! caret, IME composition, touch handles) and the accessibility tree of the visible pages ± 1.

use kiln_common::NodeId;
use kiln_doc::{Document, Op, Pos, Transaction};
use kiln_doc_layout::{Change as LayoutChange, DocLayout, DocScene, StoryKind, View, geometry};
use kiln_protocol::{ViewMode, Viewport};
use kiln_scene::{Bounds, ColorRole, DrawOp, HandleSide, LineStyle, Paint};

use crate::frame::FrameRequest;

/// Visual diameter of a touch handle (its hit target is 44 × 44 dp in the surface).
pub const HANDLE_SIZE: f32 = 22.0;
/// Pages laid out before the first frame; the rest follows on later frames.
const FIRST_PAGES: usize = 2;
/// Pages a frame lays out while the layout is incomplete.
pub const PAGES_PER_FRAME: usize = 40;

/// The layout and scene of an open Write document.
#[derive(Debug)]
pub struct WriteView {
    /// The layout.
    pub layout: DocLayout,
    /// The scene.
    pub scene: DocScene,
    /// Edits not yet laid out (`None` = everything).
    pending: Option<Vec<NodeId>>,
    /// The last frame request.
    pub request: Option<FrameRequest>,
}

/// The layout view for a viewport mode.
#[must_use]
pub fn view_for(vp: &Viewport) -> View {
    let zoom = if vp.zoom > 0.0 { vp.zoom } else { 1.0 };
    match vp.mode {
        ViewMode::WebLayout => View::Web {
            width: (vp.width_px / zoom).max(200.0),
        },
        ViewMode::ReadMode => View::Read {
            width: ((vp.width_px / zoom) / 2.0 - 48.0).clamp(320.0, 820.0),
            height: (vp.height_px / zoom - 64.0).max(320.0),
        },
        ViewMode::Draft | ViewMode::Outline => View::Draft,
        _ => View::Print,
    }
}

/// Nodes a transaction touches (for incremental layout).
#[must_use]
pub fn touched(tx: &Transaction) -> Vec<NodeId> {
    let mut out = Vec::new();
    for op in &tx.ops {
        if let Some(p) = op.paragraph() {
            out.push(p);
        }
        match op {
            Op::SplitParagraph { new_para, .. } => out.push(*new_para),
            Op::MergeParagraphs { second, .. } => out.push(*second),
            Op::InsertBlocks { container, .. } | Op::RemoveBlocks { container, .. } => {
                out.push(*container);
            }
            Op::SetTableProps { table, .. }
            | Op::SetTableGrid { table, .. }
            | Op::InsertRows { table, .. }
            | Op::RemoveRows { table, .. } => out.push(*table),
            Op::SetRow { row, .. } | Op::InsertCells { row, .. } | Op::RemoveCells { row, .. } => {
                out.push(*row);
            }
            Op::SetCell { cell, .. } => out.push(*cell),
            _ => {}
        }
    }
    out
}

/// Whether a transaction changes more than paragraphs and tables (styles, sections, lists,
/// settings): the layout starts over.
#[must_use]
pub fn is_global(tx: &Transaction) -> bool {
    tx.ops.iter().any(|op| {
        op.paragraph().is_none()
            && !matches!(
                op,
                Op::InsertBlocks { .. }
                    | Op::RemoveBlocks { .. }
                    | Op::SetTableProps { .. }
                    | Op::SetTableGrid { .. }
                    | Op::InsertRows { .. }
                    | Op::RemoveRows { .. }
                    | Op::SetRow { .. }
                    | Op::InsertCells { .. }
                    | Op::RemoveCells { .. }
                    | Op::SetCell { .. }
            )
    })
}

impl WriteView {
    /// A view laid out up to its first pages.
    #[must_use]
    pub fn new(
        doc: &Document,
        file_name: Option<String>,
        now: kiln_doc::fields::FieldDateTime,
    ) -> Self {
        // The same fonts as the scratch surface: the bundled set natively, Carlito in the web core.
        let mut layout = DocLayout::new(crate::apps::scratch::fonts::store());
        let mut options = layout.options().clone();
        options.file_name = file_name;
        options.now = Some(now);
        layout.set_options(options);
        layout.layout_until(doc, FIRST_PAGES);
        Self {
            layout,
            scene: DocScene::new(),
            pending: Some(Vec::new()),
            request: None,
        }
    }

    /// Records an edit for the next layout update.
    pub fn note(&mut self, tx: &Transaction) {
        if is_global(tx) {
            self.pending = None;
            return;
        }
        if let Some(p) = &mut self.pending {
            p.extend(touched(tx));
        }
    }

    /// Lays out pending edits. Without `complete` the re-flow stops a few pages past the
    /// viewport (later pages follow on later frames, [`PAGES_PER_FRAME`] at a time); with it the
    /// whole document is laid out.
    pub fn sync(&mut self, doc: &Document, complete: bool) {
        let limit = if complete {
            None
        } else {
            Some(self.visible_pages().1 + 3)
        };
        match self.pending.take() {
            None => self.layout.update_until(doc, &LayoutChange::All, limit),
            Some(ids) if !ids.is_empty() => {
                self.layout
                    .update_until(doc, &LayoutChange::Nodes(ids), limit);
            }
            Some(_) => {}
        }
        self.pending = Some(Vec::new());
        if !self.layout.is_complete() {
            self.layout.resume(
                doc,
                if complete {
                    None
                } else {
                    Some(PAGES_PER_FRAME)
                },
            );
        }
    }

    /// Brings the layout and scene up to date for a frame.
    pub fn prepare(&mut self, doc: &Document, req: &FrameRequest, overlay: Vec<DrawOp>) {
        let view = view_for(&req.viewport);
        if view != self.layout.view() {
            self.layout.set_view(view);
            self.pending = None;
        }
        // Frames lay out what is near the viewport first; the rest follows frame by frame.
        self.request = Some(*req);
        self.sync(doc, false);
        self.scene.prepare(
            &mut self.layout,
            doc,
            req.scale,
            &req.send,
            &req.keep,
            overlay,
        );
    }

    /// The caret bar at a position.
    #[must_use]
    pub fn caret(&self, pos: Pos) -> Option<geometry::CaretGeom> {
        geometry::caret_in(&self.layout, pos)
    }

    /// The overlay for a selection.
    #[must_use]
    pub fn overlay(
        &self,
        doc: &Document,
        anchor: Pos,
        head: Pos,
        composition: Option<(Pos, usize)>,
    ) -> Vec<DrawOp> {
        let mut ops = Vec::new();
        if anchor != head {
            let rects = geometry::selection_rects(&self.layout, doc, anchor, head);
            ops.push(DrawOp::SelectionRects {
                rects,
                paint: Paint::Role(ColorRole::Selection),
            });
        }
        if let Some((at, len)) = composition {
            let end = Pos::new(at.para, at.offset + len);
            for r in geometry::selection_rects(&self.layout, doc, at, end) {
                ops.push(DrawOp::Underline {
                    x: r.x,
                    y: r.bottom() - 2.0,
                    width: r.w,
                    thickness: 1.0,
                    paint: Paint::Role(ColorRole::Composition),
                    style: LineStyle::Solid,
                });
            }
        }
        if anchor == head
            && let Some(c) = self.caret(head)
        {
            ops.push(DrawOp::Caret {
                rect: c.rect,
                paint: Paint::Role(ColorRole::Caret),
                spark: true,
            });
            // Word's second caret where left-to-right and right-to-left text meet.
            if let Some(second) = c.secondary {
                ops.push(DrawOp::Caret {
                    rect: Bounds::new(second.x, second.y, second.w, second.h * 0.5),
                    paint: Paint::Role(ColorRole::Caret),
                    spark: false,
                });
            }
        }
        let handle = |p: Pos, side: HandleSide| {
            self.caret(p).map(|c| DrawOp::Handle {
                x: c.rect.x + c.rect.w / 2.0,
                y: c.rect.bottom(),
                size: HANDLE_SIZE,
                side,
                paint: Paint::Role(ColorRole::Caret),
            })
        };
        if anchor == head {
            ops.extend(handle(head, HandleSide::Caret));
        } else {
            let (s, e) = match doc.compare(anchor, head) {
                Some(std::cmp::Ordering::Greater) => (head, anchor),
                _ => (anchor, head),
            };
            ops.extend(handle(s, HandleSide::Start));
            ops.extend(handle(e, HandleSide::End));
        }
        ops
    }

    /// Visible pages ± 1 (all pages before the first frame).
    #[must_use]
    pub fn visible_pages(&self) -> (usize, usize) {
        let n = self.layout.pages().len();
        let Some(req) = &self.request else {
            return (0, n.saturating_sub(1));
        };
        let v = req.visible;
        let pages: Vec<usize> = (0..n)
            .filter(|&i| self.layout.page_rect(i).is_some_and(|r| r.intersects(&v)))
            .collect();
        match (pages.first(), pages.last()) {
            (Some(a), Some(b)) => (a.saturating_sub(1), (b + 1).min(n.saturating_sub(1))),
            _ => (0, n.saturating_sub(1).min(1)),
        }
    }

    /// The position under a document point in the body.
    #[must_use]
    pub fn hit(&self, x: f32, y: f32) -> Option<geometry::Hit> {
        geometry::hit_test(&self.layout, x, y, Some(StoryKind::Body))
    }
}

/// A date-time for DATE and TIME fields from the host's clock (Unix milliseconds, UTC).
/// `wasm32-unknown-unknown` has no clock of its own (`SystemTime::now` panics there), so the
/// session passes the host's (WP-64).
#[must_use]
pub fn date_time(now_ms: u64) -> kiln_doc::fields::FieldDateTime {
    let secs = now_ms / 1000;
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm), UTC.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    kiln_doc::fields::FieldDateTime {
        year: i32::try_from(y).unwrap_or(1970),
        month: u8::try_from(m).unwrap_or(1),
        day: u8::try_from(d).unwrap_or(1),
        hour: u8::try_from(rem / 3600).unwrap_or(0),
        minute: u8::try_from((rem % 3600) / 60).unwrap_or(0),
        second: u8::try_from(rem % 60).unwrap_or(0),
    }
}
