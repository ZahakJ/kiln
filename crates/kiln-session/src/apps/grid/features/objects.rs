// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pictures and shapes on Grid sheets: Insert ▸ Pictures and Insert ▸ Shapes, their tiles and
//! images for the surface, the Picture Format and Shape Format commands (size, crop, alt text,
//! fill, outline, text, placement, order) and their state for the chrome. They are drawing
//! anchors of the sheet model like charts, so moving, sizing, deleting and undo go through the
//! same commands; selecting, moving and sizing them is `charts.rs`'s.

use std::cell::RefCell;
use std::sync::Arc;

use kiln_common::{KilnError, Result};
use kiln_deck_layout::picture::{Decoded, decode};
use kiln_protocol::commands::GridCommand;
use kiln_protocol::commands::grid_features::{
    GridArrange, GridDrawingKind, GridObjectFormat, GridObjectInfo, GridPlacement,
};
use kiln_scene::{Bounds, DrawOp, ImageId, ImageUpload, Paint};
use kiln_sheet::SheetId;
use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{Anchor, DrawingAnchor, DrawingKind, DrawingLook, EditAs};
use kiln_sheet_layout::objects::{NEW_SHAPE_SIZE, ObjectKind, ObjectTile, shape_pad};
use kiln_sheet_layout::{SheetGeometry, anchor_for, anchor_rect};
use rustc_hash::{FxHashMap, FxHashSet};

use super::super::GridDoc;
use crate::model::Change;

/// EMU per point.
const EMU_PER_PT: f32 = 12_700.0;

/// A picture's image, decoded for the surface.
#[derive(Debug, Clone)]
struct Image {
    decoded: Option<Arc<Decoded>>,
    /// The file's own size in pixels (before any downscale for drawing).
    natural: (u32, u32),
}

/// The pictures of a workbook: images added in Kiln (saved with the workbook) and the decoded
/// images of every picture shown, with what the surface has been sent.
#[derive(Debug, Default)]
pub(crate) struct Objects {
    /// Images added in Kiln, by media part name.
    media: FxHashMap<String, Vec<u8>>,
    images: RefCell<FxHashMap<String, Image>>,
    next_image: RefCell<u32>,
    sent: FxHashSet<u32>,
}

/// Where one object of the active sheet is: drawing number, tile rectangle, padding, version and
/// what it draws.
pub(crate) type ObjectPlace = (u32, Bounds, f32, u64, ObjectKind);

impl Objects {
    /// the images added in Kiln, back from a native bundle.
    pub(crate) fn restore_media(&mut self, media: impl IntoIterator<Item = (String, Vec<u8>)>) {
        self.media.extend(media);
    }

    /// The images added in Kiln, for a save.
    pub(crate) fn media(&self) -> Vec<(String, Vec<u8>)> {
        let mut out: Vec<(String, Vec<u8>)> = self
            .media
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }

    /// The tiles of the places of [`GridDoc::object_places`].
    pub(crate) fn tiles<'a>(
        &self,
        places: &[ObjectPlace],
        drawings: &'a [DrawingAnchor],
    ) -> Vec<ObjectTile<'a>> {
        places
            .iter()
            .filter_map(|&(index, rect, pad, version, kind)| {
                Some(ObjectTile {
                    index,
                    rect,
                    pad,
                    version,
                    kind,
                    look: &drawings.get(usize::try_from(index).ok()?)?.look,
                })
            })
            .collect()
    }
}

/// What a drawing kind is for the chrome.
pub(crate) fn chrome_kind(kind: DrawingKind) -> GridDrawingKind {
    match kind {
        DrawingKind::Picture => GridDrawingKind::Picture,
        DrawingKind::Connector => GridDrawingKind::Line,
        DrawingKind::Chart => GridDrawingKind::Chart,
        _ => GridDrawingKind::Shape,
    }
}

/// A drawing Kiln draws as a picture or shape.
pub(crate) fn is_object(kind: DrawingKind) -> bool {
    matches!(
        kind,
        DrawingKind::Picture | DrawingKind::Shape | DrawingKind::Connector
    )
}

fn placement_of(a: &Anchor) -> GridPlacement {
    match a {
        Anchor::TwoCell {
            edit_as: EditAs::TwoCell,
            ..
        } => GridPlacement::MoveAndSize,
        Anchor::TwoCell {
            edit_as: EditAs::OneCell,
            ..
        }
        | Anchor::OneCell { .. } => GridPlacement::Move,
        _ => GridPlacement::Fixed,
    }
}

fn fnv(bytes: &[u8], mut h: u64) -> u64 {
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// A version of a look (for tiles).
fn look_version(look: &DrawingLook) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325;
    for s in [
        look.preset.as_deref().unwrap_or(""),
        look.fill.as_deref().unwrap_or(""),
        look.line.as_deref().unwrap_or(""),
        look.text.as_str(),
        look.text_color.as_deref().unwrap_or(""),
    ] {
        h = fnv(s.as_bytes(), h) ^ 0xff;
    }
    for v in look.crop {
        h = fnv(&v.to_le_bytes(), h);
    }
    h = fnv(&look.line_width.unwrap_or(-1).to_le_bytes(), h);
    fnv(&look.text_size.unwrap_or(0).to_le_bytes(), h)
}

/// The media type extension of an image file name (`png` when it has none Kiln knows).
fn image_ext(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "jpeg",
        Some("gif") => "gif",
        Some("bmp") => "bmp",
        Some("webp") => "webp",
        _ => "png",
    }
}

impl GridDoc {
    /// The bytes of a picture's image: added in Kiln, or in the file's package.
    pub(crate) fn picture_bytes(&self, part: &str) -> Option<Vec<u8>> {
        if let Some(b) = self.features.objects.media.get(part) {
            return Some(b.clone());
        }
        self.source
            .as_ref()?
            .package()
            .part(part)
            .ok()
            .flatten()
            .map(|b| b.to_vec())
    }

    /// The decoded image of a picture (decoded on first use).
    fn image_of(&self, part: &str) -> Image {
        if let Some(i) = self.features.objects.images.borrow().get(part) {
            return i.clone();
        }
        let bytes = self.picture_bytes(part);
        let image = match bytes {
            Some(b) => {
                let mut next = self.features.objects.next_image.borrow_mut();
                *next += 1;
                let decoded = decode(&b, ImageId(*next)).map(Arc::new);
                let natural = image::ImageReader::new(std::io::Cursor::new(&b))
                    .with_guessed_format()
                    .ok()
                    .and_then(|r| r.into_dimensions().ok())
                    .or_else(|| decoded.as_ref().map(|d| (d.width, d.height)))
                    .unwrap_or((0, 0));
                Image { decoded, natural }
            }
            None => Image {
                decoded: None,
                natural: (0, 0),
            },
        };
        self.features
            .objects
            .images
            .borrow_mut()
            .insert(part.to_owned(), image.clone());
        image
    }

    /// The pictures and shapes of the active sheet with their drawing numbers.
    pub(crate) fn sheet_objects(&self) -> Vec<(u32, &DrawingAnchor)> {
        self.active_sheet()
            .map(|(_, s)| {
                s.meta()
                    .drawings
                    .iter()
                    .enumerate()
                    .filter(|(_, d)| is_object(d.kind) && !d.hidden)
                    .map(|(i, d)| (u32::try_from(i).unwrap_or(u32::MAX), d))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Where the active sheet's pictures and shapes are drawn.
    pub(crate) fn object_places(&self, geo: &SheetGeometry) -> Vec<ObjectPlace> {
        self.sheet_objects()
            .into_iter()
            .map(|(index, d)| {
                let r = anchor_rect(geo, &d.anchor);
                let (kind, pad) = match d.kind {
                    DrawingKind::Picture => {
                        let img = self.image_of(&d.reference);
                        (
                            ObjectKind::Picture {
                                image: img.decoded.as_ref().map(|x| x.id),
                                size: img.decoded.as_ref().map_or((0, 0), |x| (x.width, x.height)),
                            },
                            0.0,
                        )
                    }
                    DrawingKind::Connector => (ObjectKind::Line, shape_pad(&d.look)),
                    _ => (ObjectKind::Shape, shape_pad(&d.look)),
                };
                let rect = r.inflate(pad, pad);
                let size = (u64::from(r.w.to_bits()) << 32) | u64::from(r.h.to_bits());
                let image = match kind {
                    ObjectKind::Picture { image, .. } => u64::from(image.map_or(0, |i| i.0)),
                    _ => 0,
                };
                (
                    index,
                    rect,
                    pad,
                    look_version(&d.look) ^ size.rotate_left(7) ^ image.rotate_left(29),
                    kind,
                )
            })
            .collect()
    }

    /// The images of the active sheet's pictures the surface does not have yet (all of them
    /// after a reset).
    pub(crate) fn object_images(&mut self, all: bool) -> Vec<ImageUpload> {
        if all {
            self.features.objects.sent.clear();
        }
        let parts: Vec<String> = self
            .sheet_objects()
            .into_iter()
            .filter(|(_, d)| d.kind == DrawingKind::Picture)
            .map(|(_, d)| d.reference.clone())
            .collect();
        let mut out = Vec::new();
        for p in parts {
            let Some(d) = self.image_of(&p).decoded else {
                continue;
            };
            if self.features.objects.sent.insert(d.id.0) {
                out.push(ImageUpload {
                    id: d.id,
                    width: d.width,
                    height: d.height,
                    data: d.rgba.clone(),
                });
            }
        }
        out
    }

    /// The images of the active sheet's pictures, for printing.
    pub(crate) fn print_images(
        &self,
    ) -> FxHashMap<String, Arc<kiln_io_pdf::printable::PrintImage>> {
        let mut out = FxHashMap::default();
        for (_, d) in self.sheet_objects() {
            if d.kind != DrawingKind::Picture || out.contains_key(&d.reference) {
                continue;
            }
            if let Some(img) = self.image_of(&d.reference).decoded {
                out.insert(
                    d.reference.clone(),
                    Arc::new(kiln_io_pdf::printable::PrintImage {
                        id: img.id,
                        width: img.width,
                        height: img.height,
                        rgba: Arc::new(img.rgba.clone()),
                        source: None,
                    }),
                );
            }
        }
        out
    }

    /// Picture and shape commands, or `None` for others.
    pub(crate) fn object_command(&mut self, cmd: &GridCommand) -> Option<Result<Change>> {
        Some(match cmd {
            GridCommand::InsertPicture { name, bytes } => self.insert_picture(name, bytes),
            GridCommand::InsertShape { preset, text } => self.insert_shape(preset, text.as_deref()),
            GridCommand::FormatObject { format } => self.format_object(format),
            GridCommand::ResetPicture { size } => self.reset_picture(*size),
            GridCommand::ArrangeDrawing { arrange } => self.arrange(*arrange),
            _ => return None,
        })
    }

    fn drawings_list(&self, sid: SheetId) -> Vec<DrawingAnchor> {
        self.workbook
            .sheet(sid)
            .map(|s| s.meta().drawings.clone())
            .unwrap_or_default()
    }

    fn put_drawings(
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

    /// A drawing id no drawing of the workbook has.
    fn new_drawing_id(&self) -> String {
        let used: FxHashSet<&str> = self
            .workbook
            .sheets()
            .iter()
            .flat_map(|s| s.meta().drawings.iter().map(|d| d.id.as_str()))
            .collect();
        (1u32..)
            .map(|n| format!("kiln{n}"))
            .find(|id| !used.contains(id.as_str()))
            .unwrap_or_else(|| "kiln".into())
    }

    /// The top-left corner of the active cell in sheet pixels.
    fn active_corner(&self, geo: &SheetGeometry) -> (f32, f32) {
        let a = self.view().sel.active;
        let r = geo.display_rect(a.row, a.col);
        (r.x, r.y)
    }

    /// Adds an object at `rect` (sheet pixels) and selects it.
    fn add_object(
        &mut self,
        label: &'static str,
        kind: DrawingKind,
        reference: String,
        name: String,
        rect: Bounds,
        look: DrawingLook,
    ) -> Result<Change> {
        let sid = self.sid()?;
        let geo = self.geometry_ref();
        let template = Anchor::TwoCell {
            from: kiln_sheet::meta::AnchorPoint {
                row: 0,
                col: 0,
                row_off: 0,
                col_off: 0,
            },
            to: kiln_sheet::meta::AnchorPoint {
                row: 0,
                col: 0,
                row_off: 0,
                col_off: 0,
            },
            edit_as: EditAs::TwoCell,
        };
        let anchor = anchor_for(&geo, rect, &template);
        let id = self.new_drawing_id();
        let mut list = self.drawings_list(sid);
        list.push(DrawingAnchor {
            id: id.clone(),
            kind,
            reference,
            anchor,
            name: Some(name),
            alt_text: None,
            hidden: false,
            locked: true,
            look,
        });
        self.put_drawings(label, sid, list)?;
        self.edit = None;
        self.charts.selected = Some(id);
        self.selection_epoch += 1;
        Ok(Change::Content)
    }

    /// The next free number of an object name (`Picture 3`).
    fn next_name(&self, stem: &str) -> String {
        let n = self
            .workbook
            .sheets()
            .iter()
            .flat_map(|s| s.meta().drawings.iter())
            .filter_map(|d| {
                d.name
                    .as_deref()?
                    .strip_prefix(stem)?
                    .trim()
                    .parse::<u32>()
                    .ok()
            })
            .max()
            .unwrap_or(0);
        format!("{stem} {}", n + 1)
    }

    /// Insert ▸ Pictures.
    fn insert_picture(&mut self, name: &str, bytes: &[u8]) -> Result<Change> {
        let ext = image_ext(name);
        let probe = decode(bytes, ImageId(0)).ok_or_else(|| {
            KilnError::InvalidInput(format!(
                "“{name}” is not a picture Kiln can read (PNG, JPEG, GIF, BMP or WebP)"
            ))
        })?;
        let natural = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok())
            .unwrap_or((probe.width, probe.height));
        // A media part name the file and the other pictures do not use.
        let part = (1u32..)
            .map(|n| format!("/xl/media/image{n}.{ext}"))
            .find(|p| {
                !self.features.objects.media.contains_key(p)
                    && !self.source.as_ref().is_some_and(|s| s.package().has(p))
            })
            .unwrap_or_else(|| format!("/xl/media/image.{ext}"));
        self.log.record(super::super::storage::GridOp::Media {
            part: part.clone(),
            bytes: bytes.to_vec(),
        });
        self.features
            .objects
            .media
            .insert(part.clone(), bytes.to_vec());
        let geo = self.geometry_ref();
        let (x, y) = self.active_corner(&geo);
        // Its own size at 96 dpi, at most two thirds of the window.
        let view = self.last_view;
        let z = view.zoom.max(0.01);
        let max_w = (view.width / z * 0.66).max(96.0);
        let max_h = (view.height / z * 0.66).max(96.0);
        let (w, h) = (natural.0.max(1) as f32, natural.1.max(1) as f32);
        let s = (max_w / w).min(max_h / h).min(1.0);
        let label = self.next_name("Picture");
        self.add_object(
            "grid.insert.picture",
            DrawingKind::Picture,
            part,
            label,
            Bounds::new(x, y, (w * s).max(1.0), (h * s).max(1.0)),
            DrawingLook {
                lock_aspect: true,
                ..DrawingLook::default()
            },
        )
    }

    /// Insert ▸ Shapes (a click places the shape at the active cell at its usual size).
    fn insert_shape(&mut self, preset: &str, text: Option<&str>) -> Result<Change> {
        if preset == "textBox" {
            // Insert ▸ Text Box: a white rectangle with a thin dark outline and dark text.
            let geo = self.geometry_ref();
            let (x, y) = self.active_corner(&geo);
            let label = self.next_name("TextBox");
            return self.add_object(
                "grid.insert.shape",
                DrawingKind::Shape,
                String::new(),
                label,
                Bounds::new(x + 8.0, y + 8.0, 192.0, 72.0),
                DrawingLook {
                    preset: Some("rect".into()),
                    fill: Some("scheme:lt1".into()),
                    line: Some("scheme:tx1".into()),
                    line_width: Some(9_525),
                    text: text.unwrap_or_default().to_owned(),
                    text_color: Some("scheme:tx1".into()),
                    text_size: Some(1100),
                    ..DrawingLook::default()
                },
            );
        }
        if kiln_ooxml::drawingml::presets::preset_geometry(preset, 10.0, 10.0, &[]).is_err() {
            return Err(KilnError::InvalidInput(format!(
                "“{preset}” is not a shape"
            )));
        }
        let line = matches!(
            preset,
            "line" | "straightConnector1" | "bentConnector3" | "curvedConnector3"
        );
        let geo = self.geometry_ref();
        let (x, y) = self.active_corner(&geo);
        let (w, h) = if line { (144.0, 0.0) } else { NEW_SHAPE_SIZE };
        let look = DrawingLook {
            preset: Some(preset.to_owned()),
            fill: (!line).then(|| "scheme:accent1".to_owned()),
            line: Some(if line { "scheme:accent1" } else { "scheme:dk2" }.to_owned()),
            line_width: Some(12_700),
            text: text.unwrap_or_default().to_owned(),
            text_color: (!line).then(|| "scheme:lt1".to_owned()),
            text_size: Some(1100),
            ..DrawingLook::default()
        };
        let stem = if line { "Straight Connector" } else { "Shape" };
        let label = self.next_name(stem);
        self.add_object(
            "grid.insert.shape",
            if line {
                DrawingKind::Connector
            } else {
                DrawingKind::Shape
            },
            String::new(),
            label,
            Bounds::new(x + 8.0, y + 8.0, w, h),
            look,
        )
    }

    /// The selected picture or shape: its position in the sheet's drawings.
    fn selected_object(&self) -> Result<(SheetId, usize)> {
        let sid = self.sid()?;
        let id = self.charts.selected.as_deref();
        let i = self
            .workbook
            .sheet(sid)
            .and_then(|s| {
                s.meta()
                    .drawings
                    .iter()
                    .position(|d| Some(d.id.as_str()) == id && is_object(d.kind))
            })
            .ok_or_else(|| KilnError::InvalidInput("select a picture or shape first".into()))?;
        Ok((sid, i))
    }

    /// Picture Format and Shape Format.
    fn format_object(&mut self, f: &GridObjectFormat) -> Result<Change> {
        let (sid, i) = self.selected_object()?;
        let geo = self.geometry_ref();
        let mut list = self.drawings_list(sid);
        let natural = if list[i].kind == DrawingKind::Picture {
            self.image_of(&list[i].reference).natural
        } else {
            (0, 0)
        };
        let d = &mut list[i];
        let look = &mut d.look;
        if let Some(n) = &f.name {
            let n = n.trim();
            if n.is_empty() {
                return Err(KilnError::InvalidInput("a name cannot be empty".into()));
            }
            d.name = Some(n.to_owned());
        }
        if let Some(a) = &f.alt_text {
            d.alt_text = (!a.is_empty()).then(|| a.clone());
        }
        if let Some(l) = f.lock_aspect {
            look.lock_aspect = l;
        }
        if let Some(p) = &f.preset {
            if kiln_ooxml::drawingml::presets::preset_geometry(p, 10.0, 10.0, &[]).is_err() {
                return Err(KilnError::InvalidInput(format!("“{p}” is not a shape")));
            }
            look.preset = Some(p.clone());
        }
        let colour = |c: &str| -> Result<String> {
            let ok = c == "none"
                || c.starts_with("scheme:")
                || (c.len() == 6 && c.chars().all(|x| x.is_ascii_hexdigit()));
            if ok {
                Ok(c.to_ascii_uppercase()
                    .replace("SCHEME:", "scheme:")
                    .replace("NONE", "none"))
            } else {
                Err(KilnError::InvalidInput(format!("“{c}” is not a colour")))
            }
        };
        if let Some(c) = &f.fill {
            look.fill = Some(colour(c)?);
        }
        if let Some(c) = &f.line {
            look.line = Some(colour(c)?);
        }
        if let Some(w) = f.line_width {
            if !(0.0..=1584.0).contains(&w) {
                return Err(KilnError::InvalidInput(
                    "the weight is 0 to 1584 points".into(),
                ));
            }
            look.line_width = Some((w * EMU_PER_PT).round() as i64);
        }
        if let Some(t) = &f.text {
            look.text = t.replace("\r\n", "\n");
        }
        if let Some(c) = &f.text_color {
            look.text_color = Some(colour(c)?);
        }
        if let Some(s) = f.text_size {
            if !(1.0..=409.0).contains(&s) {
                return Err(KilnError::InvalidInput(
                    "the size is 1 to 409 points".into(),
                ));
            }
            look.text_size = Some((s * 100.0).round() as u32);
        }
        let mut rect = anchor_rect(&geo, &d.anchor);
        if let Some(c) = f.crop {
            if c.iter().any(|v| !v.is_finite() || *v < 0.0)
                || c[0] + c[2] >= 100.0
                || c[1] + c[3] >= 100.0
            {
                return Err(KilnError::InvalidInput(
                    "the crops are percentages that leave part of the picture".into(),
                ));
            }
            let old = look.crop.map(|v| v as f32 / 1000.0);
            // The picture keeps its scale: the frame shrinks or grows with the crop.
            let (fw, fh) = (
                (100.0 - old[0] - old[2]).max(0.01),
                (100.0 - old[1] - old[3]).max(0.01),
            );
            let (nw, nh) = (100.0 - c[0] - c[2], 100.0 - c[1] - c[3]);
            let sx = rect.w / fw;
            let sy = rect.h / fh;
            rect = Bounds::new(
                rect.x + (c[0] - old[0]) * sx,
                rect.y + (c[1] - old[1]) * sy,
                nw * sx,
                nh * sy,
            );
            look.crop = c.map(|v| (v * 1000.0).round() as i32);
        }
        if f.width.is_some() || f.height.is_some() {
            let (mut w, mut h) = (f.width.unwrap_or(rect.w), f.height.unwrap_or(rect.h));
            if !(w.is_finite() && h.is_finite()) || w < 0.0 || h < 0.0 {
                return Err(KilnError::InvalidInput("the size is not a number".into()));
            }
            if look.lock_aspect && rect.w > 0.0 && rect.h > 0.0 {
                let ratio = rect.w / rect.h;
                match (f.width, f.height) {
                    (Some(_), None) => h = w / ratio,
                    (None, Some(_)) => w = h * ratio,
                    _ => {}
                }
            }
            rect = Bounds::new(
                rect.x,
                rect.y,
                w.max(1.0),
                h.max(if d.kind == DrawingKind::Connector {
                    0.0
                } else {
                    1.0
                }),
            );
        }
        let _ = natural;
        d.anchor = anchor_for(&geo, rect, &d.anchor);
        if let Some(p) = f.placement {
            let edit_as = match p {
                GridPlacement::MoveAndSize => EditAs::TwoCell,
                GridPlacement::Move => EditAs::OneCell,
                GridPlacement::Fixed => EditAs::Absolute,
            };
            let base = Anchor::TwoCell {
                from: kiln_sheet::meta::AnchorPoint {
                    row: 0,
                    col: 0,
                    row_off: 0,
                    col_off: 0,
                },
                to: kiln_sheet::meta::AnchorPoint {
                    row: 0,
                    col: 0,
                    row_off: 0,
                    col_off: 0,
                },
                edit_as,
            };
            d.anchor = anchor_for(&geo, rect, &base);
        }
        self.put_drawings("grid.drawing.format", sid, list)?;
        Ok(Change::Content)
    }

    /// Reset Picture: no crop (and with `size`, the picture's own size).
    fn reset_picture(&mut self, size: bool) -> Result<Change> {
        let (sid, i) = self.selected_object()?;
        let geo = self.geometry_ref();
        let mut list = self.drawings_list(sid);
        if list[i].kind != DrawingKind::Picture {
            return Err(KilnError::InvalidInput("select a picture first".into()));
        }
        let natural = self.image_of(&list[i].reference).natural;
        let d = &mut list[i];
        let rect = anchor_rect(&geo, &d.anchor);
        let old = d.look.crop.map(|v| v as f32 / 1000.0);
        let (fw, fh) = (
            (100.0 - old[0] - old[2]).max(0.01),
            (100.0 - old[1] - old[3]).max(0.01),
        );
        let (sx, sy) = (rect.w / fw, rect.h / fh);
        let mut r = Bounds::new(
            rect.x - old[0] * sx,
            rect.y - old[1] * sy,
            100.0 * sx,
            100.0 * sy,
        );
        if size && natural.0 > 0 && natural.1 > 0 {
            r = Bounds::new(
                r.x.max(0.0),
                r.y.max(0.0),
                natural.0 as f32,
                natural.1 as f32,
            );
        }
        d.look.crop = [0; 4];
        d.anchor = anchor_for(&geo, r, &d.anchor);
        self.put_drawings("grid.picture.reset", sid, list)?;
        Ok(Change::Content)
    }

    /// Bring Forward and Send Backward among the sheet's drawings.
    fn arrange(&mut self, arrange: GridArrange) -> Result<Change> {
        let sid = self.sid()?;
        let id = self.charts.selected.clone().ok_or_else(|| {
            KilnError::InvalidInput("select a picture, shape or chart first".into())
        })?;
        let mut list = self.drawings_list(sid);
        let Some(i) = list.iter().position(|d| d.id == id) else {
            return Ok(Change::None);
        };
        let j = match arrange {
            GridArrange::Forward => (i + 1).min(list.len() - 1),
            GridArrange::Front => list.len() - 1,
            GridArrange::Backward => i.saturating_sub(1),
            GridArrange::Back => 0,
        };
        if i == j {
            return Ok(Change::None);
        }
        let d = list.remove(i);
        list.insert(j, d);
        self.put_drawings("grid.drawing.arrange", sid, list)?;
        Ok(Change::Content)
    }

    /// The chrome's state of the selected picture or shape.
    pub(crate) fn object_state(
        &self,
        index: u32,
        d: &DrawingAnchor,
    ) -> kiln_protocol::commands::GridDrawing {
        let geo = self.geometry_ref();
        let r = anchor_rect(&geo, &d.anchor);
        let natural = if d.kind == DrawingKind::Picture {
            self.image_of(&d.reference).natural
        } else {
            (0, 0)
        };
        let look = &d.look;
        kiln_protocol::commands::GridDrawing {
            index,
            name: d
                .name
                .clone()
                .unwrap_or_else(|| format!("Object {}", index + 1)),
            x: r.x,
            y: r.y,
            width: r.w,
            height: r.h,
            kind: chrome_kind(d.kind),
            chart: None,
            object: Some(GridObjectInfo {
                preset: look.preset.clone().unwrap_or_default(),
                fill: look.fill.clone().unwrap_or_default(),
                line: look.line.clone().unwrap_or_default(),
                line_width: look.line_width.map_or(0.0, |w| w as f32 / EMU_PER_PT),
                text: look.text.clone(),
                text_color: look.text_color.clone().unwrap_or_default(),
                text_size: look.text_size.map_or(11.0, |s| s as f32 / 100.0),
                crop: look.crop.map(|v| v as f32 / 1000.0),
                lock_aspect: look.lock_aspect,
                alt_text: d.alt_text.clone().unwrap_or_default(),
                placement: placement_of(&d.anchor),
                natural_width: natural.0 as f32,
                natural_height: natural.1 as f32,
            }),
        }
    }

    /// The selection frame and handles of a picture or shape (sheet pixels).
    pub(crate) fn object_overlay(&self, d: &DrawingAnchor, geo: &SheetGeometry) -> Vec<DrawOp> {
        let r = anchor_rect(geo, &d.anchor);
        let z = self.last_view.zoom.max(0.01);
        let blue = Paint::Rgba(0x2f6fd0ff);
        let mut ops = vec![DrawOp::StrokeRect {
            rect: r,
            paint: blue,
            width: 1.0 / z,
            radius: 0.0,
        }];
        let s = 7.0 / z;
        let (x0, y0, x1, y1) = (r.x, r.y, r.right(), r.bottom());
        let (xm, ym) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let points: Vec<(f32, f32)> = if d.kind == DrawingKind::Connector {
            vec![(x0, y0), (x1, y1)]
        } else {
            vec![
                (x0, y0),
                (xm, y0),
                (x1, y0),
                (x1, ym),
                (x1, y1),
                (xm, y1),
                (x0, y1),
                (x0, ym),
            ]
        };
        for (x, y) in points {
            let h = Bounds::new(x - s / 2.0, y - s / 2.0, s, s);
            ops.push(DrawOp::FillRect {
                rect: h,
                paint: Paint::Rgba(0xffffffff),
                radius: s / 2.0,
            });
            ops.push(DrawOp::StrokeRect {
                rect: h,
                paint: blue,
                width: 1.0 / z,
                radius: s / 2.0,
            });
        }
        ops
    }
}
