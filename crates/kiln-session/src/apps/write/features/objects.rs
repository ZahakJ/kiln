// SPDX-License-Identifier: AGPL-3.0-or-later
//! Pictures, shapes and text boxes: selecting one (a click on it, the Selection pane), Position
//! presets, Wrap Text, moving and resizing by dragging (re-anchored to the paragraph it is
//! dropped beside, as in Word), the Layout dialog, Bring Forward / Send Backward, Rotate and
//! Flip, Align to the margins, and Insert ▸ Text Box and Shapes (with text).
//!
//! An object is selected when the selection is exactly its one placeholder character.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::props::{DocColor, ParagraphProps};
use kiln_doc::st::ThemeColorName;
use kiln_doc::st::{AlignH, AlignV, RelativeFromH, RelativeFromV, WrapSide, WrapType};
use kiln_doc::{
    Anchor, Block, Crop, Drawing, DrawingKind, EditKind, Fragment, InlineObject, Inlines, Op,
    Paragraph, Placement, Pos, PosH, PosV, Range, RunProps, Transaction,
};
use kiln_doc_layout::page::Item;
use kiln_protocol::commands::write_features::{
    ObjectAlign, ObjectInfo, ObjectLayoutSpec, PictureCrop, PositionPreset, RelativeTo, RotateOp,
    TextBoxDesign, WrapMode, ZOrder,
};
use kiln_protocol::geom::Rect;

use super::super::WriteDoc;

/// EMU per CSS pixel (96 per inch).
const EMU_PX: f64 = 9525.0;
/// EMU per point.
const EMU_PT: f64 = 12700.0;
/// Word's default distance of text from a floating object's sides (0.125 in).
const DIST_LR: i64 = 114_300;
/// A z-order above Word's usual `relativeHeight` values.
const Z_BASE: i64 = 251_658_240;
/// Rotation units per degree (`a:xfrm/@rot`).
const ROT_DEG: i32 = 60_000;

#[allow(clippy::cast_possible_truncation)]
fn emu_px(px: f32) -> i64 {
    (f64::from(px) * EMU_PX).round() as i64
}

#[allow(clippy::cast_possible_truncation)]
fn emu_pt(pt: f32) -> i64 {
    (f64::from(pt) * EMU_PT).round() as i64
}

#[allow(clippy::cast_precision_loss)]
fn pt_of(emu: i64) -> f32 {
    (emu as f64 / EMU_PT) as f32
}

fn wrap_mode(p: &Placement) -> WrapMode {
    match p {
        Placement::Inline => WrapMode::InLine,
        Placement::Anchor(a) => match a.wrap {
            WrapType::Inline => WrapMode::InLine,
            WrapType::Square => WrapMode::Square,
            WrapType::Tight => WrapMode::Tight,
            WrapType::Through => WrapMode::Through,
            WrapType::TopAndBottom => WrapMode::TopAndBottom,
            WrapType::Behind => WrapMode::Behind,
            WrapType::InFront | WrapType::None => WrapMode::InFront,
        },
    }
}

fn kind_name(d: &Drawing) -> &'static str {
    match &d.kind {
        DrawingKind::Picture { .. } => "Picture",
        DrawingKind::Shape { .. } if d.text_box.is_some() => "TextBox",
        DrawingKind::Shape { .. } => "Shape",
        DrawingKind::Chart { .. } => "Chart",
        DrawingKind::Group => "Group",
        DrawingKind::Other => "Other",
    }
}

/// A floating anchor with Word's defaults for a new or newly wrapped object.
fn default_anchor(wrap: WrapType, z: i64) -> Anchor {
    Anchor {
        wrap,
        wrap_side: matches!(wrap, WrapType::Square | WrapType::Tight | WrapType::Through)
            .then_some(WrapSide::BothSides),
        relative_h: RelativeFromH::Column,
        h: PosH::Offset(0),
        relative_v: RelativeFromV::Paragraph,
        v: PosV::Offset(0),
        dist_top: 0,
        dist_bottom: 0,
        dist_left: DIST_LR,
        dist_right: DIST_LR,
        allow_overlap: true,
        layout_in_cell: true,
        locked: false,
        z_order: z,
        wrap_polygon_xml: None,
    }
}

fn wrap_type(w: WrapMode) -> WrapType {
    match w {
        WrapMode::InLine => WrapType::Inline,
        WrapMode::Square => WrapType::Square,
        WrapMode::Tight => WrapType::Tight,
        WrapMode::Through => WrapType::Through,
        WrapMode::TopAndBottom => WrapType::TopAndBottom,
        WrapMode::Behind => WrapType::Behind,
        WrapMode::InFront => WrapType::InFront,
    }
}

/// How a new floating object looks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Look {
    /// A preset shape in the theme's accent colour (the shape style's references).
    Shape,
    /// A text box of the Insert ▸ Text Box gallery.
    TextBox(TextBoxDesign),
}

/// Kept DrawingML for a new floating shape or text box: the writer patches size, position and
/// wrapping from the model, and the layout reads the fill and outline from it.
fn shape_xml(id: u64, name: &str, preset: &str, look_of: Look, cx: i64, cy: i64) -> String {
    let text_box = look_of != Look::Shape;
    let look = match look_of {
        Look::Shape => String::new(),
        Look::TextBox(TextBoxDesign::Simple) => "<a:solidFill><a:schemeClr val=\"lt1\"/></a:solidFill><a:ln w=\"6350\"><a:solidFill><a:prstClr val=\"black\"/></a:solidFill></a:ln>".to_owned(),
        Look::TextBox(TextBoxDesign::Sidebar) => "<a:solidFill><a:schemeClr val=\"accent1\"><a:lumMod val=\"20000\"/><a:lumOff val=\"80000\"/></a:schemeClr></a:solidFill><a:ln><a:noFill/></a:ln>".to_owned(),
        Look::TextBox(TextBoxDesign::Quote) => "<a:noFill/><a:ln w=\"19050\"><a:solidFill><a:schemeClr val=\"accent1\"/></a:solidFill></a:ln>".to_owned(),
    };
    let style = if text_box {
        String::new()
    } else {
        "<wps:style><a:lnRef idx=\"2\"><a:schemeClr val=\"accent1\"><a:shade val=\"50000\"/></a:schemeClr></a:lnRef><a:fillRef idx=\"1\"><a:schemeClr val=\"accent1\"/></a:fillRef><a:effectRef idx=\"0\"><a:schemeClr val=\"accent1\"/></a:effectRef><a:fontRef idx=\"minor\"><a:schemeClr val=\"lt1\"/></a:fontRef></wps:style>".to_owned()
    };
    let txbx = "<wps:txbx><w:txbxContent><w:p/></w:txbxContent></wps:txbx>";
    let cnv = if text_box {
        "<wps:cNvSpPr txBox=\"1\"/>"
    } else {
        "<wps:cNvSpPr/>"
    };
    let anchor_v = match look_of {
        Look::TextBox(TextBoxDesign::Simple | TextBoxDesign::Sidebar) => "t",
        Look::Shape | Look::TextBox(TextBoxDesign::Quote) => "ctr",
    };
    format!(
        "<w:drawing><wp:anchor distT=\"0\" distB=\"0\" distL=\"{DIST_LR}\" distR=\"{DIST_LR}\" simplePos=\"0\" relativeHeight=\"{z}\" behindDoc=\"0\" locked=\"0\" layoutInCell=\"1\" allowOverlap=\"1\"><wp:simplePos x=\"0\" y=\"0\"/><wp:positionH relativeFrom=\"column\"><wp:posOffset>0</wp:posOffset></wp:positionH><wp:positionV relativeFrom=\"paragraph\"><wp:posOffset>0</wp:posOffset></wp:positionV><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:effectExtent l=\"0\" t=\"0\" r=\"0\" b=\"0\"/><wp:wrapSquare wrapText=\"bothSides\"/><wp:docPr id=\"{id}\" name=\"{name}\"/><wp:cNvGraphicFramePr/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.microsoft.com/office/word/2010/wordprocessingShape\"><wps:wsp>{cnv}<wps:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"{preset}\"><a:avLst/></a:prstGeom>{look}</wps:spPr>{style}{txbx}<wps:bodyPr rot=\"0\" vert=\"horz\" wrap=\"square\" lIns=\"91440\" tIns=\"45720\" rIns=\"91440\" bIns=\"45720\" anchor=\"{anchor_v}\" anchorCtr=\"0\"><a:noAutofit/></wps:bodyPr></wps:wsp></a:graphicData></a:graphic></wp:anchor></w:drawing>",
        z = Z_BASE,
        name = name.replace(['"', '<', '>', '&'], "")
    )
}

impl WriteDoc {
    /// Where a drawing is: (paragraph, offset, drawing).
    pub(in crate::apps::write) fn find_drawing(
        &self,
        id: NodeId,
    ) -> Option<(NodeId, usize, Drawing)> {
        for (_, p) in self.doc.all_paragraphs() {
            for (off, o) in p.content.objects_with_offsets() {
                if let InlineObject::Drawing(d) = o
                    && d.id == id
                {
                    return Some((p.id, off, (**d).clone()));
                }
            }
        }
        None
    }

    /// The selected object: the selection is exactly one drawing's placeholder.
    pub(in crate::apps::write) fn selected_object(&self) -> Option<(NodeId, usize, Drawing)> {
        let (s, e) = self.span();
        if s.para != e.para || e.offset != s.offset + 1 {
            return None;
        }
        let p = self.doc.paragraph(s.para)?;
        match p.content.object_at(s.offset)? {
            InlineObject::Drawing(d) => Some((s.para, s.offset, (**d).clone())),
            _ => None,
        }
    }

    /// The object under a document point (front layer first, then inline, then behind).
    pub(in crate::apps::write) fn object_at(&self, x: f32, y: f32) -> Option<NodeId> {
        let v = self.view.as_ref()?;
        let page_i = v.layout.page_at(x, y)?;
        let page = v.layout.pages().get(page_i)?;
        let (px, py) = (x - page.origin.0, y - page.origin.1);
        let hit = |items: &[Item]| {
            items.iter().rev().find_map(|it| match it {
                Item::Drawing(d) if d.rect.contains(px, py) => Some(d.drawing.id),
                _ => None,
            })
        };
        hit(&page.front)
            .or_else(|| hit(&page.items))
            .or_else(|| hit(&page.behind))
    }

    /// Selects the object under a point.
    pub(in crate::apps::write) fn select_object_at(&mut self, x: f32, y: f32) -> Result<()> {
        match self.object_at(x, y) {
            Some(id) => self.select_object(id),
            None => Ok(()),
        }
    }

    /// Selects an object by id.
    pub(in crate::apps::write) fn select_object(&mut self, id: NodeId) -> Result<()> {
        let (para, off, _) = self
            .find_drawing(id)
            .ok_or_else(|| KilnError::InvalidInput(format!("no object {id}")))?;
        self.editor.header_footer = None;
        self.set_selection(Pos::new(para, off), Pos::new(para, off + 1));
        // A plain click selects this object alone.
        self.office.multi = crate::apps::write::office::MultiSelection::default();
        Ok(())
    }

    /// The rectangle of a drawing on its page, in document coordinates.
    pub(in crate::apps::write) fn drawing_rect(&self, id: NodeId) -> Option<Rect> {
        let v = self.view.as_ref()?;
        for page in v.layout.pages() {
            for it in page.front.iter().chain(&page.items).chain(&page.behind) {
                if let Item::Drawing(d) = it
                    && d.drawing.id == id
                {
                    let r = d.rect;
                    return Some(Rect::new(
                        r.x + page.origin.0,
                        r.y + page.origin.1,
                        r.w,
                        r.h,
                    ));
                }
            }
        }
        None
    }

    fn info_of(&self, para: NodeId, offset: usize, d: &Drawing, selected: bool) -> ObjectInfo {
        let in_selection = selected || self.selected_objects().contains(&d.id);
        ObjectInfo {
            id: d.id,
            name: if d.name.is_empty() {
                format!("{} {}", kind_name(d), d.id.0)
            } else {
                d.name.clone()
            },
            kind: kind_name(d).to_owned(),
            wrap: wrap_mode(&d.placement),
            para,
            offset: u32::try_from(offset).unwrap_or(u32::MAX),
            rect: self.drawing_rect(d.id),
            width: pt_of(d.width),
            height: pt_of(d.height),
            #[allow(clippy::cast_precision_loss)]
            rotation: d.rotation as f32 / ROT_DEG as f32,
            z: match &d.placement {
                Placement::Anchor(a) => a.z_order,
                Placement::Inline => 0,
            },
            alt_text: d.description.clone(),
            decorative: d.decorative,
            selected,
            crop: match &d.kind {
                DrawingKind::Picture { crop, .. } => Some(crop_percent(crop.unwrap_or_default())),
                _ => None,
            },
            shape: super::shapes::shape_look(d),
            hidden: d.hidden,
            in_selection,
        }
    }

    /// The selected object for the ribbon's contextual tabs and the object frame.
    pub(super) fn selected_object_info(&self) -> Option<ObjectInfo> {
        let (para, off, d) = self.selected_object()?;
        Some(self.info_of(para, off, &d, true))
    }

    /// The Selection pane: every drawing of the body, front first.
    pub(in crate::apps::write) fn object_infos(&self) -> Vec<ObjectInfo> {
        let selected = self.selected_object().map(|o| o.2.id);
        let mut out = Vec::new();
        for p in self.doc.body_paragraphs() {
            for (off, o) in p.content.objects_with_offsets() {
                if let InlineObject::Drawing(d) = o {
                    out.push(self.info_of(p.id, off, d, selected == Some(d.id)));
                }
            }
        }
        out.sort_by_key(|o| std::cmp::Reverse(o.z));
        out
    }

    /// Replaces the selected (or given) object's drawing.
    pub(in crate::apps::write) fn update_object(
        &mut self,
        id: Option<NodeId>,
        f: impl FnOnce(&mut Self, &mut Drawing, NodeId) -> Result<()>,
    ) -> Result<()> {
        let (para, offset, mut d) = match id {
            Some(i) => self
                .find_drawing(i)
                .ok_or_else(|| KilnError::InvalidInput(format!("no object {i}")))?,
            None => self.selected_object().ok_or_else(|| {
                KilnError::InvalidInput("select a picture, shape or text box first".into())
            })?,
        };
        let before = d.clone();
        f(self, &mut d, para)?;
        if d == before {
            return Ok(());
        }
        let sel = Range::new(Pos::new(para, offset), Pos::new(para, offset + 1));
        self.commit(
            Transaction::new(vec![Op::SetObject {
                para,
                offset,
                object: InlineObject::Drawing(Box::new(d)),
            }])
            .with_kind(EditKind::Format)
            .with_selection(Some(self.selection_range()), Some(sel)),
        )
    }

    /// The highest z-order of the document's floating objects.
    fn max_z(&self) -> i64 {
        let mut z = Z_BASE;
        for (_, p) in self.doc.all_paragraphs() {
            for o in p.content.objects() {
                if let InlineObject::Drawing(d) = o
                    && let Placement::Anchor(a) = &d.placement
                {
                    z = z.max(a.z_order);
                }
            }
        }
        z
    }

    /// An anchor for an inline object that starts floating where it is on the page.
    fn float_here(&self, d: &Drawing, para: NodeId, wrap: WrapType) -> Anchor {
        let mut a = default_anchor(wrap, self.max_z() + 1024);
        if let (Some(r), Some(v)) = (self.drawing_rect(d.id), self.view.as_ref())
            && let Some(page_i) = v.layout.page_at(r.x + 1.0, r.y + 1.0)
            && let Some(page) = v.layout.pages().get(page_i)
        {
            let col_x = page.origin.0 + page.body.x;
            a.h = PosH::Offset(emu_px(r.x - col_x));
            let para_top = v
                .layout
                .lines_of(para)
                .iter()
                .find(|l| l.page as usize == page_i)
                .and_then(|l| v.layout.line(*l))
                .map_or(r.y, |l| l.rect().y + page.origin.1);
            a.v = PosV::Offset(emu_px(r.y - para_top));
        }
        a
    }

    /// Wrap Text.
    pub(super) fn object_wrap(&mut self, wrap: WrapMode) -> Result<()> {
        self.update_object(None, |doc, d, para| {
            let t = wrap_type(wrap);
            d.placement = match (&d.placement, wrap) {
                (_, WrapMode::InLine) => Placement::Inline,
                (Placement::Inline, _) => Placement::Anchor(Box::new(doc.float_here(d, para, t))),
                (Placement::Anchor(a), _) => {
                    let mut a = (**a).clone();
                    a.wrap = t;
                    a.wrap_side =
                        matches!(t, WrapType::Square | WrapType::Tight | WrapType::Through)
                            .then_some(a.wrap_side.unwrap_or(WrapSide::BothSides));
                    Placement::Anchor(Box::new(a))
                }
            };
            Ok(())
        })
    }

    /// Position ▾.
    pub(super) fn object_position(&mut self, preset: PositionPreset) -> Result<()> {
        if preset == PositionPreset::InLine {
            return self.object_wrap(WrapMode::InLine);
        }
        let (h, v) = match preset {
            PositionPreset::TopLeft => (AlignH::Left, AlignV::Top),
            PositionPreset::TopCenter => (AlignH::Center, AlignV::Top),
            PositionPreset::TopRight => (AlignH::Right, AlignV::Top),
            PositionPreset::MiddleLeft => (AlignH::Left, AlignV::Center),
            PositionPreset::MiddleCenter => (AlignH::Center, AlignV::Center),
            PositionPreset::MiddleRight => (AlignH::Right, AlignV::Center),
            PositionPreset::BottomLeft => (AlignH::Left, AlignV::Bottom),
            PositionPreset::BottomCenter => (AlignH::Center, AlignV::Bottom),
            PositionPreset::BottomRight | PositionPreset::InLine => (AlignH::Right, AlignV::Bottom),
        };
        self.update_object(None, |doc, d, _| {
            let mut a = match &d.placement {
                Placement::Anchor(a) => (**a).clone(),
                Placement::Inline => default_anchor(WrapType::Square, doc.max_z() + 1024),
            };
            a.wrap = WrapType::Square;
            a.wrap_side = Some(WrapSide::BothSides);
            a.relative_h = RelativeFromH::Margin;
            a.h = PosH::Align(h);
            a.relative_v = RelativeFromV::Margin;
            a.v = PosV::Align(v);
            d.placement = Placement::Anchor(Box::new(a));
            Ok(())
        })
    }

    /// Points a floating anchor at a rectangle of the page (document coordinates, `size` its
    /// width and height): horizontally from the page edge, vertically from the body paragraph
    /// beside its top (Word re-anchors a dropped object there). Returns that paragraph
    /// (`para` when none is beside it).
    pub(in crate::apps::write) fn anchor_at(
        &mut self,
        a: &mut Anchor,
        rect: Rect,
        size: (f32, f32),
        para: NodeId,
    ) -> Result<NodeId> {
        let (w, h) = size;
        let view = self.view_synced();
        let layout = &view.layout;
        let page_i = layout
            .page_at(rect.x + w / 2.0, rect.y + h / 2.0)
            .or_else(|| layout.page_at(rect.x, rect.y))
            .unwrap_or(0);
        let Some(page) = layout.pages().get(page_i) else {
            return Err(KilnError::InvalidInput(
                "the document is not laid out".into(),
            ));
        };
        let (ox, oy) = page.origin;
        let probe_y = rect
            .y
            .clamp(oy + page.body.y, oy + page.body.y + page.body.h - 1.0);
        let hit = view.hit(ox + page.body.x + 1.0, probe_y);
        let mut new_para = hit.map_or(para, |h| h.pos.para);
        let mut para_top = layout
            .lines_of(new_para)
            .iter()
            .find(|l| l.page as usize == page_i)
            .and_then(|l| layout.line(*l))
            .map(|l| l.rect().y + oy);
        if self.doc.paragraph(new_para).is_none() {
            new_para = para;
            para_top = None;
        }
        a.relative_h = RelativeFromH::Page;
        a.h = PosH::Offset(emu_px(rect.x - ox));
        match para_top {
            Some(top) => {
                a.relative_v = RelativeFromV::Paragraph;
                a.v = PosV::Offset(emu_px(rect.y - top));
            }
            None => {
                a.relative_v = RelativeFromV::Page;
                a.v = PosV::Offset(emu_px(rect.y - oy));
            }
        }
        Ok(new_para)
    }

    /// Drag to move or resize: the new rectangle in document coordinates.
    pub(in crate::apps::write) fn object_transform(
        &mut self,
        id: NodeId,
        rect: Rect,
    ) -> Result<()> {
        if !(rect.width.is_finite()
            && rect.height.is_finite()
            && rect.x.is_finite()
            && rect.y.is_finite())
        {
            return Err(KilnError::InvalidInput(
                "the object's rectangle is not a number".into(),
            ));
        }
        let (para, offset, mut d) = self
            .find_drawing(id)
            .ok_or_else(|| KilnError::InvalidInput(format!("no object {id}")))?;
        let w = rect.width.clamp(4.0, 5000.0);
        let h = rect.height.clamp(4.0, 5000.0);
        d.width = emu_px(w);
        d.height = emu_px(h);
        let mut target = (para, offset);
        if let Placement::Anchor(a) = &d.placement {
            let mut a = (**a).clone();
            let new_para = self.anchor_at(&mut a, rect, (w, h), para)?;
            d.placement = Placement::Anchor(Box::new(a));
            if new_para != para {
                target = (new_para, 0);
            }
        }
        let object = InlineObject::Drawing(Box::new(d));
        let props = self.props_at(Pos::new(para, offset + 1));
        let ops = if target.0 == para {
            vec![Op::SetObject {
                para,
                offset,
                object,
            }]
        } else {
            vec![
                Op::DeleteInline {
                    para,
                    offset,
                    len: 1,
                },
                Op::InsertInline {
                    para: target.0,
                    offset: target.1,
                    fragment: Fragment::object(object, props),
                },
            ]
        };
        let sel = Range::new(
            Pos::new(target.0, target.1),
            Pos::new(target.0, target.1 + 1),
        );
        // Moving an object is not a tracked change in Kiln (Word records it as a move).
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(self.selection_range()), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    /// The Layout dialog.
    pub(super) fn object_layout(
        &mut self,
        id: Option<NodeId>,
        spec: &ObjectLayoutSpec,
    ) -> Result<()> {
        let finite = |v: Option<f32>| v.is_none_or(f32::is_finite);
        if ![
            spec.x,
            spec.y,
            spec.width,
            spec.height,
            spec.rotation,
            spec.distance,
        ]
        .into_iter()
        .all(finite)
        {
            return Err(KilnError::InvalidInput(
                "layout values must be numbers".into(),
            ));
        }
        if spec.width.is_some_and(|w| w <= 0.0) || spec.height.is_some_and(|h| h <= 0.0) {
            return Err(KilnError::InvalidInput(
                "width and height must be positive".into(),
            ));
        }
        if let Some(n) = &spec.name
            && (n.trim().is_empty() || n.chars().count() > 255)
        {
            return Err(KilnError::InvalidInput(
                "a name of 1–255 characters is needed".into(),
            ));
        }
        let spec = spec.clone();
        self.update_object(id, move |doc, d, para| {
            if let Some(w) = spec.wrap {
                let t = wrap_type(w);
                d.placement = match (&d.placement, w) {
                    (_, WrapMode::InLine) => Placement::Inline,
                    (Placement::Inline, _) => {
                        Placement::Anchor(Box::new(doc.float_here(d, para, t)))
                    }
                    (Placement::Anchor(a), _) => {
                        let mut a = (**a).clone();
                        a.wrap = t;
                        Placement::Anchor(Box::new(a))
                    }
                };
            }
            if let Placement::Anchor(a) = &mut d.placement {
                if let Some(x) = spec.x {
                    a.h = PosH::Offset(emu_pt(x));
                    a.relative_h = match spec.x_from.unwrap_or(RelativeTo::Page) {
                        RelativeTo::Page => RelativeFromH::Page,
                        RelativeTo::Margin => RelativeFromH::Margin,
                        RelativeTo::ColumnOrParagraph => RelativeFromH::Column,
                    };
                }
                if let Some(y) = spec.y {
                    a.v = PosV::Offset(emu_pt(y));
                    a.relative_v = match spec.y_from.unwrap_or(RelativeTo::Page) {
                        RelativeTo::Page => RelativeFromV::Page,
                        RelativeTo::Margin => RelativeFromV::Margin,
                        RelativeTo::ColumnOrParagraph => RelativeFromV::Paragraph,
                    };
                }
                if let Some(move_with) = spec.move_with_text
                    && spec.y.is_none()
                {
                    a.relative_v = if move_with {
                        RelativeFromV::Paragraph
                    } else {
                        RelativeFromV::Page
                    };
                }
                if let Some(dist) = spec.distance {
                    let e = emu_pt(dist.max(0.0));
                    a.dist_top = e;
                    a.dist_bottom = e;
                    a.dist_left = e;
                    a.dist_right = e;
                }
                if let Some(o) = spec.allow_overlap {
                    a.allow_overlap = o;
                }
            }
            if let Some(w) = spec.width {
                d.width = emu_pt(w);
            }
            if let Some(h) = spec.height {
                d.height = emu_pt(h);
            }
            if let Some(r) = spec.rotation {
                #[allow(clippy::cast_possible_truncation)]
                let units = (f64::from(r.rem_euclid(360.0)) * f64::from(ROT_DEG)).round() as i32;
                d.rotation = units;
            }
            if let Some(n) = &spec.name {
                d.name = n.trim().to_owned();
            }
            if let Some(t) = &spec.alt_text {
                d.description = (!t.is_empty()).then(|| t.clone());
            }
            if let Some(dec) = spec.decorative {
                d.decorative = dec;
            }
            Ok(())
        })
    }

    /// Bring Forward / Send Backward.
    pub(super) fn object_order(&mut self, order: ZOrder) -> Result<()> {
        let Some((_, _, d)) = self.selected_object() else {
            return Err(KilnError::InvalidInput(
                "select a picture, shape or text box first".into(),
            ));
        };
        let Placement::Anchor(mine) = &d.placement else {
            return Err(KilnError::InvalidInput(
                "objects in line with text have no stacking order: choose a text wrapping first"
                    .into(),
            ));
        };
        let z = mine.z_order;
        let mut others: Vec<i64> = Vec::new();
        for (_, p) in self.doc.all_paragraphs() {
            for o in p.content.objects() {
                if let InlineObject::Drawing(x) = o
                    && x.id != d.id
                    && let Placement::Anchor(a) = &x.placement
                {
                    others.push(a.z_order);
                }
            }
        }
        let new_z = match order {
            ZOrder::ToFront => others.iter().max().map_or(z, |m| (*m).max(z) + 1024),
            ZOrder::ToBack => others
                .iter()
                .min()
                .map_or(z, |m| ((*m).min(z) - 1024).max(0)),
            ZOrder::Forward => others
                .iter()
                .filter(|o| **o >= z)
                .min()
                .map_or(z, |next| next + 1),
            ZOrder::Backward => others
                .iter()
                .filter(|o| **o <= z)
                .max()
                .map_or(z, |prev| (prev - 1).max(0)),
        };
        self.update_object(None, |_, d, _| {
            if let Placement::Anchor(a) = &mut d.placement {
                a.z_order = new_z.min(i64::from(i32::MAX));
            }
            Ok(())
        })
    }

    /// Rotate ▾.
    pub(super) fn object_rotate(&mut self, op: RotateOp) -> Result<()> {
        self.update_object(None, |_, d, _| {
            let full = 360 * ROT_DEG;
            match op {
                RotateOp::Right90 => d.rotation = (d.rotation + 90 * ROT_DEG).rem_euclid(full),
                RotateOp::Left90 => d.rotation = (d.rotation - 90 * ROT_DEG).rem_euclid(full),
                RotateOp::FlipVertical => d.flip_v = !d.flip_v,
                RotateOp::FlipHorizontal => d.flip_h = !d.flip_h,
            }
            Ok(())
        })
    }

    /// Align ▾ to the margins (Word's default for one object) or to the page.
    pub(in crate::apps::write) fn object_align(
        &mut self,
        align: ObjectAlign,
        relative_to: Option<RelativeTo>,
    ) -> Result<()> {
        let page = relative_to == Some(RelativeTo::Page);
        if relative_to == Some(RelativeTo::ColumnOrParagraph) {
            return Err(KilnError::InvalidInput(
                "objects align to the page or the margins".into(),
            ));
        }
        self.update_object(None, |doc, d, para| {
            if matches!(d.placement, Placement::Inline) {
                d.placement =
                    Placement::Anchor(Box::new(doc.float_here(d, para, WrapType::Square)));
            }
            if let Placement::Anchor(a) = &mut d.placement {
                match align {
                    ObjectAlign::Left
                    | ObjectAlign::Center
                    | ObjectAlign::Right
                    | ObjectAlign::DistributeHorizontally => {
                        a.relative_h = if page {
                            RelativeFromH::Page
                        } else {
                            RelativeFromH::Margin
                        };
                        a.h = PosH::Align(match align {
                            ObjectAlign::Left => AlignH::Left,
                            ObjectAlign::Right => AlignH::Right,
                            _ => AlignH::Center,
                        });
                    }
                    ObjectAlign::Top
                    | ObjectAlign::Middle
                    | ObjectAlign::Bottom
                    | ObjectAlign::DistributeVertically => {
                        a.relative_v = if page {
                            RelativeFromV::Page
                        } else {
                            RelativeFromV::Margin
                        };
                        a.v = PosV::Align(match align {
                            ObjectAlign::Top => AlignV::Top,
                            ObjectAlign::Bottom => AlignV::Bottom,
                            _ => AlignV::Center,
                        });
                    }
                }
            }
            Ok(())
        })
    }

    /// The next free number for an object name ("Text Box 3").
    fn next_object_number(&self, prefix: &str) -> usize {
        let mut n = 1;
        for (_, p) in self.doc.all_paragraphs() {
            for o in p.content.objects() {
                if let InlineObject::Drawing(d) = o
                    && let Some(rest) = d.name.strip_prefix(prefix)
                    && let Ok(k) = rest.trim().parse::<usize>()
                {
                    n = n.max(k + 1);
                }
            }
        }
        n
    }

    /// Inserts a floating shape beside the caret's paragraph, or in a rectangle drawn on the
    /// page (document coordinates), and selects it.
    #[allow(clippy::too_many_arguments)]
    fn insert_floating_shape(
        &mut self,
        preset: &str,
        name: String,
        text: Option<&str>,
        look: Look,
        size: (i64, i64),
        rect: Option<Rect>,
    ) -> Result<()> {
        let (s, _) = self.span();
        if matches!(self.doc.story_of(s.para), Some(kiln_doc::Story::Comment(_))) {
            return Err(KilnError::InvalidInput(
                "comments cannot hold shapes".into(),
            ));
        }
        if let Some(r) = rect
            && !(r.x.is_finite() && r.y.is_finite() && r.width.is_finite() && r.height.is_finite())
        {
            return Err(KilnError::InvalidInput(
                "the drawn rectangle is not a number".into(),
            ));
        }
        let text_box = look != Look::Shape;
        let id = self.doc.alloc_id();
        let blocks = if text_box || text.is_some() {
            let mut p = Paragraph::new(self.doc.alloc_id());
            let (props, run) = design_text(look);
            p.props = props;
            if let Some(t) = text.filter(|t| !t.is_empty()) {
                p.content =
                    Inlines::from_fragment(Fragment::text(&t.replace(['\r', '\n'], " "), run))
                        .map_err(|e| KilnError::InvalidInput(e.to_string()))?;
            }
            Some(vec![Block::Paragraph(p)])
        } else {
            None
        };
        let mut anchor = default_anchor(WrapType::Square, self.max_z() + 1024);
        match look {
            Look::TextBox(TextBoxDesign::Sidebar) => {
                anchor.relative_h = RelativeFromH::Margin;
                anchor.h = PosH::Align(AlignH::Right);
                anchor.relative_v = RelativeFromV::Margin;
                anchor.v = PosV::Align(AlignV::Top);
            }
            Look::TextBox(TextBoxDesign::Quote) => {
                anchor.wrap = WrapType::TopAndBottom;
                anchor.wrap_side = None;
                anchor.dist_top = DIST_LR;
                anchor.dist_bottom = DIST_LR;
                anchor.relative_h = RelativeFromH::Margin;
                anchor.h = PosH::Align(AlignH::Center);
            }
            Look::Shape | Look::TextBox(TextBoxDesign::Simple) => {
                anchor.relative_h = RelativeFromH::Column;
                anchor.h = PosH::Align(AlignH::Right);
            }
        }
        let mut size = size;
        let mut at = Pos::new(s.para, s.offset);
        if let Some(r) = rect {
            let (w, h) = (
                r.width.abs().clamp(4.0, 5000.0),
                r.height.abs().clamp(4.0, 5000.0),
            );
            // A drag up or to the left draws the same rectangle.
            let r = Rect::new(r.x.min(r.x + r.width), r.y.min(r.y + r.height), w, h);
            size = (emu_px(w), emu_px(h));
            let para = self.anchor_at(&mut anchor, r, (w, h), s.para)?;
            if para != s.para {
                at = Pos::new(para, 0);
            }
        }
        let d = Drawing {
            id,
            kind: DrawingKind::Shape {
                preset: preset.to_owned(),
            },
            width: size.0,
            height: size.1,
            placement: Placement::Anchor(Box::new(anchor)),
            name: name.clone(),
            description: None,
            title: None,
            decorative: false,
            hidden: false,
            rotation: 0,
            flip_h: false,
            flip_v: false,
            text_box: blocks,
            xml: Some(shape_xml(id.0, &name, preset, look, size.0, size.1)),
            group: None,
        };
        let props = self.props_at(at);
        self.commit(
            Transaction::new(vec![Op::InsertInline {
                para: at.para,
                offset: at.offset,
                fragment: Fragment::object(InlineObject::Drawing(Box::new(d)), props),
            }])
            .with_kind(EditKind::Structure)
            .with_selection(
                Some(self.selection_range()),
                Some(Range::new(at, Pos::new(at.para, at.offset + 1))),
            ),
        )
    }

    /// Insert ▸ Text Box: a gallery design beside the caret or in a drawn rectangle.
    pub(super) fn insert_text_box(
        &mut self,
        text: Option<&str>,
        design: TextBoxDesign,
        rect: Option<Rect>,
    ) -> Result<()> {
        if text.is_some_and(|t| t.chars().count() > 10_000) {
            return Err(KilnError::InvalidInput(
                "the text box text is too long".into(),
            ));
        }
        let n = self.next_object_number("Text Box");
        let placeholder = match design {
            TextBoxDesign::Simple => None,
            TextBoxDesign::Sidebar => {
                Some("Use a sidebar for an aside, a list of key points or a short story.")
            }
            TextBoxDesign::Quote => {
                Some("\u{201C}Use a pull quote to draw attention to a key line.\u{201D}")
            }
        };
        let size = match design {
            TextBoxDesign::Simple => (2_514_600, 914_400),
            TextBoxDesign::Sidebar => (1_828_800, 4_114_800),
            TextBoxDesign::Quote => (4_114_800, 822_960),
        };
        self.insert_floating_shape(
            "rect",
            format!("Text Box {n}"),
            text.or(placeholder),
            Look::TextBox(design),
            size,
            rect,
        )
    }

    /// Insert ▸ Shapes: a preset beside the caret or in a drawn rectangle.
    pub(super) fn insert_shape(
        &mut self,
        preset: &str,
        text: Option<&str>,
        rect: Option<Rect>,
    ) -> Result<()> {
        if !kiln_ooxml::drawingml::presets::is_preset_shape_type(preset) {
            return Err(KilnError::InvalidInput(format!(
                "'{preset}' is not a DrawingML preset shape"
            )));
        }
        if text.is_some_and(|t| t.chars().count() > 10_000) {
            return Err(KilnError::InvalidInput("the shape text is too long".into()));
        }
        let n = self.next_object_number(&format!("{preset} "));
        let name = format!("{} {n}", preset);
        let size = if matches!(preset, "line" | "straightConnector1") {
            (1_371_600, 0)
        } else {
            (1_371_600, 914_400)
        };
        self.insert_floating_shape(
            preset,
            name,
            text,
            Look::Shape,
            (size.0, size.1.max(9525)),
            rect,
        )
    }

    // ── Picture Format ──────────────────────────────────────────────────────────────────────

    /// The selected picture: (paragraph, offset, drawing, media part, crop).
    fn selected_picture(&self) -> Result<(NodeId, usize, Drawing, String, Crop)> {
        let (para, off, d) = self
            .selected_object()
            .ok_or_else(|| KilnError::InvalidInput("select a picture first".into()))?;
        match &d.kind {
            DrawingKind::Picture { media, crop } => {
                let (m, c) = (media.clone(), crop.unwrap_or_default());
                Ok((para, off, d, m, c))
            }
            _ => Err(KilnError::InvalidInput(
                "the selected object is not a picture".into(),
            )),
        }
    }

    /// The pixel size of a media part.
    fn media_pixels(&self, media: &str) -> Option<(u32, u32)> {
        let m = self.doc.media.get(media)?;
        image::ImageReader::new(std::io::Cursor::new(&m.data))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()
            .filter(|(w, h)| *w > 0 && *h > 0)
    }

    /// Replaces the selected picture's drawing (one undo step, the picture stays selected).
    fn set_picture(
        &mut self,
        para: NodeId,
        offset: usize,
        d: Drawing,
        mut ops: Vec<Op>,
    ) -> Result<()> {
        ops.push(Op::SetObject {
            para,
            offset,
            object: InlineObject::Drawing(Box::new(d)),
        });
        let sel = Range::new(Pos::new(para, offset), Pos::new(para, offset + 1));
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(self.selection_range()), Some(sel)),
        )
    }

    /// Crop: the share cut from each side; the picture keeps its scale, so its box shrinks or
    /// grows with the visible part (as in Word).
    pub(super) fn crop_picture(&mut self, crop: PictureCrop) -> Result<()> {
        let sides = [crop.left, crop.top, crop.right, crop.bottom];
        if sides
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=99.0).contains(v))
            || crop.left + crop.right >= 99.5
            || crop.top + crop.bottom >= 99.5
        {
            return Err(KilnError::InvalidInput(
                "crop each side by 0 to 99 per cent, leaving some of the picture".into(),
            ));
        }
        let (para, off, mut d, media, old) = self.selected_picture()?;
        let new = crop_of(crop);
        resize_for_crop(&mut d, old, new);
        d.kind = DrawingKind::Picture {
            media,
            crop: (new != Crop::default()).then_some(new),
        };
        self.set_picture(para, off, d, Vec::new())
    }

    /// Crop ▸ Aspect Ratio: the largest centred part of the whole picture with that shape.
    pub(super) fn crop_to_aspect(&mut self, width: u16, height: u16) -> Result<()> {
        if width == 0 || height == 0 || width > 100 || height > 100 {
            return Err(KilnError::InvalidInput(
                "an aspect ratio of 1 to 100 by 1 to 100 is needed".into(),
            ));
        }
        let (para, off, mut d, media, old) = self.selected_picture()?;
        // The size the whole picture shows at now.
        let (full_w, full_h) = full_size(&d, old);
        let want = f64::from(width) / f64::from(height);
        let have = full_w / full_h;
        let mut new = Crop::default();
        if have > want {
            let keep = want / have;
            let cut = to_crop_units((1.0 - keep) / 2.0);
            new.left = cut;
            new.right = cut;
        } else if have < want {
            let keep = have / want;
            let cut = to_crop_units((1.0 - keep) / 2.0);
            new.top = cut;
            new.bottom = cut;
        }
        resize_for_crop(&mut d, old, new);
        d.kind = DrawingKind::Picture {
            media,
            crop: (new != Crop::default()).then_some(new),
        };
        self.set_picture(para, off, d, Vec::new())
    }

    /// Reset Picture: no cropping, rotation or flips; with `size`, the picture's own size
    /// (at 96 pixels per inch, no wider than the text column, as Insert ▸ Pictures sizes it).
    pub(super) fn reset_picture(&mut self, size: bool) -> Result<()> {
        let (para, off, mut d, media, old) = self.selected_picture()?;
        let (full_w, full_h) = full_size(&d, old);
        #[allow(clippy::cast_possible_truncation)]
        {
            d.width = full_w.round().max(1.0) as i64;
            d.height = full_h.round().max(1.0) as i64;
        }
        if size && let Some((w, h)) = self.media_pixels(&media) {
            let (w, h) = self.fit_column(para, w, h);
            d.width = w;
            d.height = h;
        }
        d.rotation = 0;
        d.flip_h = false;
        d.flip_v = false;
        d.kind = DrawingKind::Picture { media, crop: None };
        self.set_picture(para, off, d, Vec::new())
    }

    /// A picture of `w` × `h` pixels at 96 per inch, scaled down to the text column (EMU).
    fn fit_column(&self, para: NodeId, w: u32, h: u32) -> (i64, i64) {
        let max_w = self
            .doc
            .section_of(para)
            .map_or(9360, |s| s.props.content_width());
        let max_px = i64::from(max_w) * 96 / 1440;
        let (mut pw, mut ph) = (i64::from(w), i64::from(h));
        if pw > max_px {
            ph = (ph * max_px / pw).max(1);
            pw = max_px;
        }
        (pw.max(1) * 9525, ph.max(1) * 9525)
    }

    /// Change Picture: another image in the selected picture's place, as wide as the old one
    /// shows, with the new image's proportions.
    pub(super) fn change_picture(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let (para, off, mut d, _, _) = self.selected_picture()?;
        let (ext, content_type, (w, h)) = crate::apps::write::commands::picture_media(name, bytes)?;
        let mut n = self.doc.media.len() + 1;
        let part = loop {
            let candidate = format!("word/media/image{n}.{ext}");
            if !self.doc.media.contains_key(&candidate) {
                break candidate;
            }
            n += 1;
        };
        #[allow(clippy::cast_possible_truncation)]
        {
            d.height = ((d.width as f64) * f64::from(h) / f64::from(w))
                .round()
                .max(1.0) as i64;
        }
        d.kind = DrawingKind::Picture {
            media: part.clone(),
            crop: None,
        };
        let ops = vec![Op::PutMedia {
            name: part,
            media: Some(kiln_doc::Media {
                content_type: content_type.to_owned(),
                data: bytes.to_vec(),
            }),
        }];
        self.set_picture(para, off, d, ops)
    }
}

/// The text of a new text box or shape: paragraph and run formatting of the design.
fn design_text(look: Look) -> (ParagraphProps, RunProps) {
    let mut p = ParagraphProps::default();
    let mut r = RunProps::default();
    match look {
        Look::Shape => p.alignment = Some(kiln_doc::st::Alignment::Center),
        Look::TextBox(TextBoxDesign::Simple | TextBoxDesign::Sidebar) => {}
        Look::TextBox(TextBoxDesign::Quote) => {
            p.alignment = Some(kiln_doc::st::Alignment::Center);
            r.italic = Some(true);
            r.size = Some(28);
            r.color = Some(DocColor::theme(0x4472C4, ThemeColorName::Accent1));
        }
    }
    (p, r)
}

/// Crop units (thousandths of a per cent) from a fraction.
#[allow(clippy::cast_possible_truncation)]
fn to_crop_units(fraction: f64) -> i32 {
    (fraction.clamp(0.0, 0.99) * 100_000.0).round() as i32
}

fn crop_of(c: PictureCrop) -> Crop {
    let u = |v: f32| to_crop_units(f64::from(v) / 100.0);
    Crop {
        left: u(c.left),
        top: u(c.top),
        right: u(c.right),
        bottom: u(c.bottom),
    }
}

#[allow(clippy::cast_precision_loss)]
fn crop_percent(c: Crop) -> PictureCrop {
    let p = |v: i32| v as f32 / 1000.0;
    PictureCrop {
        left: p(c.left),
        top: p(c.top),
        right: p(c.right),
        bottom: p(c.bottom),
    }
}

/// The shown fractions of the picture's width and height under a crop.
fn kept(c: Crop) -> (f64, f64) {
    let f = |v: i32| f64::from(v.clamp(0, 99_000)) / 100_000.0;
    (
        (1.0 - f(c.left) - f(c.right)).max(0.01),
        (1.0 - f(c.top) - f(c.bottom)).max(0.01),
    )
}

/// The size (EMU) the whole picture shows at, given its box and its crop.
#[allow(clippy::cast_precision_loss)]
fn full_size(d: &Drawing, crop: Crop) -> (f64, f64) {
    let (kw, kh) = kept(crop);
    (
        (d.width as f64 / kw).max(1.0),
        (d.height as f64 / kh).max(1.0),
    )
}

/// Resizes a picture's box from one crop to another at the same scale.
#[allow(clippy::cast_possible_truncation)]
fn resize_for_crop(d: &mut Drawing, old: Crop, new: Crop) {
    let (full_w, full_h) = full_size(d, old);
    let (kw, kh) = kept(new);
    d.width = (full_w * kw).round().max(1.0) as i64;
    d.height = (full_h * kh).round().max(1.0) as i64;
}
