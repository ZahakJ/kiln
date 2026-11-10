// SPDX-License-Identifier: AGPL-3.0-or-later
//! Picture Format ▸ Crop: crop handles on the slide (the black handles crop the frame, a drag
//! inside it moves the picture behind the frame, Esc or a click elsewhere ends cropping), crop
//! to an aspect ratio, Fill and Fit. A crop that reaches outside the picture leaves an empty
//! margin, as in PowerPoint.

use kiln_common::{KilnError, NodeId, Result};
use kiln_deck::{Crop, ShapeKind, Transform};
use kiln_deck_layout::geom::rect_px;
use kiln_deck_layout::select::{Grip, Handle, Modifiers, grip_at, resize};
use kiln_protocol::commands::DeckCommand;
use kiln_protocol::commands::deck_content::DeckCropFit;
use kiln_scene::{DrawOp, FillRule, Paint, PathData};

use super::DeckDoc;
use super::stage::{Drag, DragKind};
use crate::model::Change;

/// A crop as it shows on the slide, or back: `a:srcRect` crops the picture before it is flipped,
/// so on a picture flipped across the frame the file's left crop is the frame's right edge (and
/// its top the bottom).
fn as_shown(c: Crop, frame: &Transform) -> Crop {
    let (l, r) = if frame.flip_h { (c.r, c.l) } else { (c.l, c.r) };
    let (t, b) = if frame.flip_v { (c.b, c.t) } else { (c.t, c.b) };
    Crop { l, t, r, b }
}

/// The whole picture behind a frame cropped by `c`, EMU `(x, y, w, h)`.
#[allow(clippy::cast_precision_loss)]
fn full_of(frame: &Transform, c: Crop) -> (f64, f64, f64, f64) {
    let c = as_shown(c, frame);
    let f = |v: i32| f64::from(v) / 100_000.0;
    let w = frame.cx as f64 / (1.0 - f(c.l) - f(c.r)).max(0.01);
    let h = frame.cy as f64 / (1.0 - f(c.t) - f(c.b)).max(0.01);
    (
        frame.x as f64 - f(c.l) * w,
        frame.y as f64 - f(c.t) * h,
        w,
        h,
    )
}

/// The crop that shows the picture `full` through `frame`.
#[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
fn crop_for((x, y, w, h): (f64, f64, f64, f64), frame: &Transform) -> Crop {
    let f = |v: f64| (v * 100_000.0).round() as i32;
    let (fx, fy) = (frame.x as f64, frame.y as f64);
    let (fw, fh) = (frame.cx as f64, frame.cy as f64);
    let shown = Crop {
        l: f((fx - x) / w.max(1.0)),
        t: f((fy - y) / h.max(1.0)),
        r: f((x + w - fx - fw) / w.max(1.0)),
        b: f((y + h - fy - fh) / h.max(1.0)),
    };
    as_shown(shown, frame)
}

impl DeckDoc {
    /// The picture showing its crop handles.
    pub(crate) fn cropping_picture(&self) -> Option<NodeId> {
        if !self.view.crop {
            return None;
        }
        match self.view.shapes.as_slice() {
            [id] if matches!(
                self.pres.shape(*id).map(|s| &s.kind),
                Some(ShapeKind::Picture(_))
            ) =>
            {
                Some(*id)
            }
            _ => None,
        }
    }

    fn crop_of(&self, id: NodeId) -> Crop {
        match self.pres.shape(id).map(|s| &s.kind) {
            Some(ShapeKind::Picture(p)) => p.crop,
            _ => Crop::default(),
        }
    }

    /// Sets a picture's crop on the model (a preview, no undo step).
    pub(crate) fn preview_crop(&mut self, id: NodeId, crop: Crop) {
        if let Some(s) = self.pres.shape_mut(id)
            && let ShapeKind::Picture(p) = &mut s.kind
        {
            p.crop = crop;
        }
    }

    /// A press in crop mode: a crop handle, or inside the frame to move the picture. `None`
    /// ends cropping.
    pub(crate) fn crop_grip(&self, x: f32, y: f32) -> Option<DragKind> {
        let id = self.cropping_picture()?;
        let frame = self.xfrm_of(id)?;
        let crop = self.crop_of(id);
        match grip_at(&frame, x, y, self.zoom, false)? {
            Grip::Size(h) => Some(DragKind::Crop {
                handle: Some(h),
                crop,
            }),
            Grip::Body => Some(DragKind::Crop { handle: None, crop }),
            Grip::Rotate => None,
        }
    }

    /// Previews a crop drag.
    pub(crate) fn crop_drag(&mut self, d: &Drag, dx: f32, dy: f32, m: Modifiers) {
        let DragKind::Crop { handle, crop } = d.kind else {
            return;
        };
        let Some((id, frame)) = d.originals.first().copied() else {
            return;
        };
        let full = full_of(&frame, crop);
        let (xfrm, crop) = match handle {
            Some(h) => {
                let f = resize(&frame, h, dx, dy, m);
                (f, crop_for(full, &f))
            }
            None => {
                #[allow(clippy::cast_precision_loss)]
                let (ex, ey) = (
                    kiln_deck_layout::geom::emu(f64::from(dx)) as f64,
                    kiln_deck_layout::geom::emu(f64::from(dy)) as f64,
                );
                (
                    frame,
                    crop_for((full.0 + ex, full.1 + ey, full.2, full.3), &frame),
                )
            }
        };
        self.preview_crop(id, crop);
        self.preview(d.slide, &[(id, xfrm)]);
    }

    /// Ends a crop drag as one undo step.
    pub(crate) fn crop_done(&mut self, d: &Drag) -> Result<Change> {
        let Some((id, _)) = d.originals.first().copied() else {
            return Ok(Change::None);
        };
        let (xfrm, crop) = (self.xfrm_of(id), self.crop_of(id));
        self.restore(d);
        let Some(xfrm) = xfrm else {
            return Ok(Change::None);
        };
        self.set_crop_and_frame(id, crop, xfrm)
    }

    fn set_crop_and_frame(&mut self, id: NodeId, crop: Crop, xfrm: Transform) -> Result<Change> {
        let label = "deck.picture.crop";
        self.begin_group();
        let a = self.apply_with(label, |p| kiln_deck::edit::set_crop(p, id, crop));
        let b = self.apply_with(label, |p| kiln_deck::edit::set_transform(p, id, xfrm));
        self.end_group();
        a?;
        b?;
        Ok(Change::Content)
    }

    /// Crop mode and Crop to an aspect ratio, Fill and Fit.
    pub(crate) fn crop_command(&mut self, cmd: &DeckCommand) -> Result<Change> {
        let label = cmd.meta().id;
        let id = self.selected_picture(label)?;
        match cmd {
            DeckCommand::CropMode { on } => {
                self.end_text();
                self.view.shapes = vec![id];
                self.view.crop = *on;
                self.epoch += 1;
                Ok(Change::Selection)
            }
            DeckCommand::CropTo { fit } => {
                let frame = self
                    .xfrm_of(id)
                    .ok_or_else(|| KilnError::unavailable(label, "the picture has no frame"))?;
                let crop = self.crop_of(id);
                let (xfrm, crop) = match fit {
                    DeckCropFit::Aspect { width, height } => {
                        if *width == 0 || *height == 0 {
                            return Err(KilnError::InvalidInput("an aspect ratio of zero".into()));
                        }
                        // The largest box of that ratio in the frame, centred.
                        let ratio = f64::from(*width) / f64::from(*height);
                        #[allow(clippy::cast_precision_loss)]
                        let (fw, fh) = (frame.cx as f64, frame.cy as f64);
                        let (w, h) = if fw / fh.max(1.0) > ratio {
                            (fh * ratio, fh)
                        } else {
                            (fw, fw / ratio)
                        };
                        #[allow(clippy::cast_possible_truncation)]
                        let f = Transform {
                            x: frame.x + ((fw - w) / 2.0).round() as i64,
                            y: frame.y + ((fh - h) / 2.0).round() as i64,
                            cx: w.round() as i64,
                            cy: h.round() as i64,
                            ..frame
                        };
                        (f, crop_for(full_of(&frame, crop), &f))
                    }
                    DeckCropFit::Fill | DeckCropFit::Fit => {
                        let media = match self.pres.shape(id).map(|s| &s.kind) {
                            Some(ShapeKind::Picture(p)) => p.media.clone(),
                            _ => String::new(),
                        };
                        let (iw, ih) = super::features::designer::image_size(&self.pres, &media)
                            .ok_or_else(|| {
                                KilnError::unavailable(label, "the picture's image cannot be read")
                            })?;
                        let crop = if matches!(fit, DeckCropFit::Fill) {
                            super::features::designer::fill_crop((iw, ih), frame.cx, frame.cy)
                        } else {
                            fit_crop((iw, ih), frame.cx, frame.cy)
                        };
                        (frame, crop)
                    }
                };
                self.set_crop_and_frame(id, crop, xfrm)
            }
            _ => Ok(Change::None),
        }
    }

    /// The crop overlay: the whole picture's outline with the cropped-away part dimmed, and
    /// the black crop handles on the frame.
    pub(crate) fn crop_overlay(&self, ops: &mut Vec<DrawOp>, z: f32) {
        let Some(id) = self.cropping_picture() else {
            return;
        };
        let Some(frame) = self.xfrm_of(id) else {
            return;
        };
        let (x, y, w, h) = full_of(&frame, self.crop_of(id));
        #[allow(clippy::cast_possible_truncation)]
        let full = rect_px(&Transform {
            x: x.round() as i64,
            y: y.round() as i64,
            cx: w.round() as i64,
            cy: h.round() as i64,
            ..frame
        });
        let r = rect_px(&frame);
        // The cropped-away part of the picture, faint (the frame's part is the slide's own).
        let media = match self.pres.shape(id).map(|s| &s.kind) {
            Some(ShapeKind::Picture(p)) => p.media.clone(),
            _ => String::new(),
        };
        let ghost = self.renderer.borrow_mut().pictures.get(&self.pres, &media);
        if let Some(img) = ghost.filter(|_| frame.rot == 0 && !frame.flip_h && !frame.flip_v) {
            #[allow(clippy::cast_precision_loss)]
            let src = kiln_scene::Bounds::new(0.0, 0.0, img.width as f32, img.height as f32);
            ops.push(DrawOp::Image {
                image: img.id,
                src,
                dst: full,
                opacity: 0.45,
            });
        }
        let mut dim = PathData::rect(full);
        dim.extend(&PathData::rect(r));
        ops.push(DrawOp::Path {
            path: dim,
            fill: Some(Paint::Rgba(0x8080_8040)),
            stroke: None,
            rule: FillRule::EvenOdd,
        });
        ops.push(DrawOp::Path {
            path: PathData::rect(full),
            fill: None,
            stroke: Some(super::view::stroke(
                Paint::Rgba(0x4040_40ff),
                1.0 / z,
                vec![4.0 / z, 3.0 / z],
            )),
            rule: FillRule::NonZero,
        });
        // An L at each corner, a bar at each side's middle.
        let (len, t) = (16.0 / z, 5.0 / z);
        let mut bars = PathData::new();
        for h in Handle::ALL {
            let (fx, fy) = h.fraction();
            let (px, py) = (r.x + fx * r.w, r.y + fy * r.h);
            let (sx, sy) = h.sides();
            let (ix, iy) = (-f32::from(sx), -f32::from(sy));
            let bar = |p: &mut PathData, x0: f32, y0: f32, x1: f32, y1: f32| {
                p.extend(&PathData::rect(kiln_scene::Bounds::from_corners(
                    x0.min(x1),
                    y0.min(y1),
                    x0.max(x1),
                    y0.max(y1),
                )));
            };
            if sx != 0 && sy != 0 {
                bar(&mut bars, px, py, px + ix * len, py + iy * t);
                bar(&mut bars, px, py, px + ix * t, py + iy * len);
            } else if sx != 0 {
                bar(&mut bars, px, py - len / 2.0, px + ix * t, py + len / 2.0);
            } else {
                bar(&mut bars, px - len / 2.0, py, px + len / 2.0, py + iy * t);
            }
        }
        ops.push(DrawOp::Path {
            path: bars,
            fill: Some(Paint::Rgba(0x1010_10ff)),
            stroke: Some(super::view::stroke(
                Paint::Rgba(0xffff_ffff),
                0.75 / z,
                Vec::new(),
            )),
            rule: FillRule::NonZero,
        });
    }
}

/// A crop that fits all of an image of `img` pixels in a box of `cx` × `cy` EMU, centred, with
/// empty margins (negative crops) on the short sides.
fn fit_crop((iw, ih): (u32, u32), cx: i64, cy: i64) -> Crop {
    let img = f64::from(iw.max(1)) / f64::from(ih.max(1));
    #[allow(clippy::cast_precision_loss)]
    let area = cx.max(1) as f64 / cy.max(1) as f64;
    // The image shown is `keep` of the box along one axis; the margins are negative crops
    // relative to the image.
    #[allow(clippy::cast_possible_truncation)]
    let pad = |keep: f64| (-(1.0 / keep - 1.0) / 2.0 * 100_000.0).round() as i32;
    if img > area {
        let c = pad(area / img);
        Crop {
            l: 0,
            t: c,
            r: 0,
            b: c,
        }
    } else {
        let c = pad(img / area);
        Crop {
            l: c,
            t: 0,
            r: c,
            b: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_crops_the_picture_behind_it() {
        let frame = Transform::new(1000, 1000, 8000, 6000);
        let full = full_of(&frame, Crop::default());
        let smaller = Transform::new(2000, 1000, 7000, 3000);
        let c = crop_for(full, &smaller);
        assert_eq!((c.l, c.t, c.r, c.b), (12_500, 0, 0, 50_000));
        let back = full_of(&smaller, c);
        assert!((back.0 - 1000.0).abs() < 1.0 && (back.2 - 8000.0).abs() < 1.0);
    }

    #[test]
    fn a_flipped_picture_is_cropped_on_the_side_that_shows() {
        // Flipped left to right, the file's left crop (20 %) is cut off at the frame's right.
        let mut frame = Transform::new(1000, 1000, 8000, 6000);
        frame.flip_h = true;
        let crop = Crop {
            l: 20_000,
            ..Crop::default()
        };
        let full = full_of(&frame, crop);
        assert!(
            (full.0 - 1000.0).abs() < 1.0 && (full.2 - 10_000.0).abs() < 1.0,
            "{full:?}"
        );
        // Dragging the frame's left edge in crops the picture's right side, as it shows.
        let smaller = Transform {
            x: 2000,
            cx: 7000,
            ..frame
        };
        let c = crop_for(full, &smaller);
        assert_eq!((c.l, c.t, c.r, c.b), (20_000, 0, 10_000, 0));
        // Upside down too: the file's top crop is the frame's bottom.
        frame.flip_h = false;
        frame.flip_v = true;
        let crop = Crop {
            t: 25_000,
            ..Crop::default()
        };
        let full = full_of(&frame, crop);
        assert!(
            (full.1 - 1000.0).abs() < 1.0 && (full.3 - 8000.0).abs() < 1.0,
            "{full:?}"
        );
        let lower = Transform {
            y: 2000,
            cy: 5000,
            ..frame
        };
        let c = crop_for(full, &lower);
        assert_eq!((c.l, c.t, c.r, c.b), (0, 25_000, 0, 12_500));
    }

    #[test]
    fn fit_leaves_margins_fill_crops() {
        // A 2:1 image in a square: Fit pads top and bottom by half the image's height.
        let c = fit_crop((200, 100), 1000, 1000);
        assert_eq!((c.t, c.b, c.l), (-50_000, -50_000, 0));
        let c = super::super::features::designer::fill_crop((200, 100), 1000, 1000);
        assert_eq!((c.l, c.r, c.t), (25_000, 25_000, 0));
    }
}
