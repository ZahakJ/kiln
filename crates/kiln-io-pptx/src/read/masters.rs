// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slide masters with their themes and text styles, slide layouts and the notes master; the
//! parts of a slide-like root (`p:cSld` name, background, shape tree) shared with slides.

use kiln_common::NodeId;
use kiln_deck::dml::serde_xml::normalize_theme;
use kiln_deck::shape::Shape;
use kiln_deck::slide::{
    Background, ColorMapping, HeaderFooter, Layout, LayoutKind, Master, NotesMaster,
};
use kiln_deck::text::ListStyle;
use kiln_ooxml::drawingml::color::{ColorContext, ColorMap, DrawingColor, parse_color_child};
use kiln_ooxml::drawingml::fill::find_fill;
use kiln_ooxml::drawingml::theme::{Theme, office_theme, parse_theme, theme_color_context};
use kiln_ooxml::opc::rel_type;
use kiln_ooxml::xml::XmlElement;

use super::Reader;
use super::context::PartCx;
use crate::common::kind::rel;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::{attr_u32, fragment};

/// What the common part of a slide, layout, master or notes page holds.
pub(crate) struct SlideLike {
    pub name: Option<String>,
    pub background: Option<Background>,
    pub shapes: Vec<Shape>,
    pub cx: PartCx,
}

/// `p:clrMap` (or `a:overrideClrMapping`) → the model's colour map.
pub(crate) fn color_mapping(e: &XmlElement) -> ColorMapping {
    ColorMapping::from_ooxml(&ColorMap::parse(e))
}

/// `p:clrMapOvr`: `None` for `a:masterClrMapping`.
pub(crate) fn color_override(root: &XmlElement) -> Option<ColorMapping> {
    root.child("p:clrMapOvr")?
        .child("a:overrideClrMapping")
        .map(color_mapping)
}

/// A colour context for a theme and colour map.
pub(crate) fn color_context(theme: &Theme, map: &ColorMapping) -> ColorContext {
    theme_color_context(theme, map.to_ooxml(), None)
}

impl Reader<'_> {
    /// Reads `p:cSld` and records the root's and `p:cSld`'s unmodelled children for `owner`.
    pub fn read_slide_like(
        &mut self,
        part: &str,
        root: &XmlElement,
        owner: NodeId,
        color: Option<ColorContext>,
        modelled: &[&str],
    ) -> SlideLike {
        let mut cx = PartCx::new(self, part, &root.attrs, color);
        let csld = root.child("p:cSld");
        let name = csld
            .and_then(|c| c.attr("name"))
            .filter(|s| !s.is_empty())
            .map(str::to_owned);
        let mut background = None;
        let mut shapes = Vec::new();
        if let Some(csld) = csld {
            for c in csld.elements() {
                match c.name.as_str() {
                    "p:bg" => background = self.read_background(&cx, c),
                    "p:spTree" => shapes = self.read_tree(&mut cx, c, owner, 0),
                    _ => self.extras.push_owner_xml(owner, fragment(c, &root.attrs)),
                }
            }
        }
        for c in root.elements() {
            if c.name != "p:cSld" && !modelled.contains(&c.name.as_str()) {
                self.extras.push_owner_xml(owner, fragment(c, &root.attrs));
            }
        }
        self.resolve_connections(&cx, &mut shapes);
        SlideLike {
            name,
            background,
            shapes,
            cx,
        }
    }

    /// Reads `p:bg`.
    pub fn read_background(&mut self, cx: &PartCx, bg: &XmlElement) -> Option<Background> {
        if let Some(pr) = bg.child("p:bgPr") {
            let mut fill = find_fill(pr)?;
            self.fix_fill(cx, &mut fill, "background");
            return Some(Background::Fill {
                fill,
                shade_to_title: pr.attr_bool("shadeToTitle").unwrap_or(false),
            });
        }
        let r = bg.child("p:bgRef")?;
        Some(Background::Theme {
            idx: attr_u32(r, "idx").unwrap_or(1001),
            color: parse_color_child(Some(r)),
        })
    }

    /// The theme of a master or notes master part (Office's theme when it has none).
    fn read_theme(&mut self, part: &str, consume: bool) -> (Theme, Option<String>) {
        let cx_rels = self.pkg.related_part(part, rel_type::THEME).ok().flatten();
        let Some(theme_part) = cx_rels else {
            return (office_theme(), None);
        };
        let Some(doc) = self.part_xml(&theme_part, part) else {
            return (office_theme(), None);
        };
        match parse_theme(&doc.root) {
            Ok(mut t) => {
                normalize_theme(&mut t);
                if consume {
                    self.consume(&theme_part);
                }
                (t, Some(theme_part))
            }
            Err(e) => {
                self.warn(
                    WarningCode::PartInvalidXml,
                    Severity::Warning,
                    format!("the theme is not readable ({e}); Office's theme is used"),
                    &theme_part,
                    "",
                );
                (office_theme(), None)
            }
        }
    }

    /// Reads a slide master and the layouts it lists. `None` when the part is unusable.
    pub fn read_master(&mut self, part: &str, list_id: Option<u32>) -> Option<NodeId> {
        let doc = self.part_xml(part, "the presentation")?;
        let root = &doc.root;
        if root.name != "p:sldMaster" {
            self.warn(
                WarningCode::PartUnusable,
                Severity::Warning,
                format!("a slide master part holds a {}", root.name),
                part,
                "",
            );
            return None;
        }
        self.consume(part);
        let id = self.pres.alloc_id();
        let (theme, theme_part) = self.read_theme(part, true);
        let color_map = root
            .child("p:clrMap")
            .map(color_mapping)
            .unwrap_or_default();
        let ctx = color_context(&theme, &color_map);
        let like = self.read_slide_like(
            part,
            root,
            id,
            Some(ctx),
            &["p:clrMap", "p:sldLayoutIdLst", "p:txStyles", "p:hf"],
        );
        let styles = root.child("p:txStyles");
        let style = |r: &mut Self, n: &str| -> ListStyle {
            styles
                .and_then(|s| s.child(n))
                .map(|e| r.read_list_style(&like.cx, e))
                .unwrap_or_default()
        };
        let title_style = style(self, "p:titleStyle");
        let body_style = style(self, "p:bodyStyle");
        let other_style = style(self, "p:otherStyle");
        let master = Master {
            id,
            name: like.name.clone().unwrap_or_else(|| theme.name.clone()),
            theme,
            color_map,
            background: like.background.clone().unwrap_or(Background::Theme {
                idx: 1001,
                color: Some(DrawingColor::scheme("bg1")),
            }),
            shapes: like.shapes,
            title_style,
            body_style,
            other_style,
            layouts: Vec::new(),
            header_footer: read_hf(root.child("p:hf")),
            preserve: root.attr_bool("preserve").unwrap_or(false),
        };
        self.extras.part_names.insert(id, part.to_owned());
        if let Some(t) = theme_part {
            self.extras.theme_parts.insert(id, t);
        }
        if let Some(l) = list_id {
            self.extras.list_ids.insert(id, l);
        }
        self.pres.masters.push(master);
        // The layouts, in the master's order.
        let rels = like.cx.rels.clone();
        if let Some(list) = root.child("p:sldLayoutIdLst") {
            for l in list.children_named("p:sldLayoutId") {
                let Some(target) = l
                    .attr("r:id")
                    .and_then(|rid| rels.get(rid))
                    .filter(|r| !r.is_external())
                    .and_then(|r| rels.resolve(r))
                else {
                    continue;
                };
                if self.layout_ids.contains_key(&target) {
                    continue;
                }
                self.read_layout(&target, id, attr_u32(l, "id"));
            }
        }
        // Layouts related to the master but missing from its list still belong to it.
        for r in rels.all() {
            if r.is_external() || !kiln_ooxml::opc::rel_type_matches(&r.rel_type, rel::SLIDE_LAYOUT)
            {
                continue;
            }
            if let Some(target) = rels.resolve(r)
                && !self.layout_ids.contains_key(&target)
            {
                self.read_layout(&target, id, None);
            }
        }
        Some(id)
    }

    /// Reads a layout into its master.
    pub fn read_layout(
        &mut self,
        part: &str,
        master: NodeId,
        list_id: Option<u32>,
    ) -> Option<NodeId> {
        let doc = self.part_xml(part, "a slide master")?;
        let root = &doc.root;
        if root.name != "p:sldLayout" {
            self.warn(
                WarningCode::PartUnusable,
                Severity::Warning,
                format!("a slide layout part holds a {}", root.name),
                part,
                "",
            );
            return None;
        }
        self.consume(part);
        let id = self.pres.alloc_id();
        self.layout_ids.insert(part.to_owned(), id);
        let (theme, master_map) = {
            let m = self.pres.master(master)?;
            (m.theme.clone(), m.color_map)
        };
        let color_map = color_override(root);
        let ctx = color_context(&theme, &color_map.unwrap_or(master_map));
        let like = self.read_slide_like(part, root, id, Some(ctx), &["p:clrMapOvr"]);
        let kind = LayoutKind::from_ooxml(root.attr("type"));
        let layout = Layout {
            id,
            name: like.name.unwrap_or_else(|| kind.display_name().to_owned()),
            kind,
            shapes: like.shapes,
            background: like.background,
            color_map,
            show_master_shapes: root.attr_bool("showMasterSp").unwrap_or(true),
            preserve: root.attr_bool("preserve").unwrap_or(false),
        };
        self.extras.part_names.insert(id, part.to_owned());
        if let Some(l) = list_id {
            self.extras.list_ids.insert(id, l);
        }
        self.pres.master_mut(master)?.layouts.push(layout);
        Some(id)
    }

    /// Reads the notes master; its theme stays in the passthrough (the model keeps one theme
    /// per slide master) and colours resolve against it.
    pub fn read_notes_master(&mut self, part: &str) -> Option<(Theme, ColorMapping)> {
        let doc = self.part_xml(part, "the presentation")?;
        let root = &doc.root;
        if root.name != "p:notesMaster" {
            self.warn(
                WarningCode::PartUnusable,
                Severity::Warning,
                format!("the notes master part holds a {}", root.name),
                part,
                "",
            );
            return None;
        }
        self.consume(part);
        let id = self.pres.alloc_id();
        let (theme, _) = self.read_theme(part, false);
        let color_map = root
            .child("p:clrMap")
            .map(color_mapping)
            .unwrap_or_default();
        let ctx = color_context(&theme, &color_map);
        let like = self.read_slide_like(
            part,
            root,
            id,
            Some(ctx),
            &["p:clrMap", "p:notesStyle", "p:hf"],
        );
        let notes_style = root
            .child("p:notesStyle")
            .map(|e| self.read_list_style(&like.cx, e))
            .unwrap_or_default();
        if let Some(hf) = root.child("p:hf") {
            self.extras.push_owner_xml(id, fragment(hf, &root.attrs));
        }
        self.pres.notes_master = Some(NotesMaster {
            id,
            shapes: like.shapes,
            background: like.background.unwrap_or(Background::Theme {
                idx: 1001,
                color: Some(DrawingColor::scheme("bg1")),
            }),
            notes_style,
            color_map,
        });
        self.extras.part_names.insert(id, part.to_owned());
        Some((theme, color_map))
    }

    /// A master with one blank layout for a file without a usable master (PowerPoint refuses
    /// such files; Kiln opens what it can).
    pub fn fallback_master(&mut self) {
        let id = self.pres.alloc_id();
        let layout = Layout::new(self.pres.alloc_id(), LayoutKind::Blank);
        self.pres.masters.push(Master {
            id,
            name: "Office Theme".to_owned(),
            theme: office_theme(),
            color_map: ColorMapping::default(),
            background: Background::Theme {
                idx: 1001,
                color: Some(DrawingColor::scheme("bg1")),
            },
            shapes: Vec::new(),
            title_style: ListStyle::default(),
            body_style: ListStyle::default(),
            other_style: ListStyle::default(),
            layouts: vec![layout],
            header_footer: HeaderFooter::default(),
            preserve: false,
        });
    }
}

/// `p:hf` (every flag on when the element is absent).
fn read_hf(hf: Option<&XmlElement>) -> HeaderFooter {
    let Some(hf) = hf else {
        return HeaderFooter::default();
    };
    let f = |n: &str| hf.attr_bool(n).unwrap_or(true);
    HeaderFooter {
        slide_number: f("sldNum"),
        header: f("hdr"),
        footer: f("ftr"),
        date: f("dt"),
    }
}
