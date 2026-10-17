// SPDX-License-Identifier: AGPL-3.0-or-later
//! Slides with their notes pages, comments, transitions and animations.

use std::collections::HashMap;

use kiln_common::NodeId;
use kiln_deck::shape::{PlaceholderType, ShapeKind};
use kiln_deck::slide::{Comment, CommentReply, Notes, Slide};
use kiln_deck::text::TextBody;
use kiln_ooxml::drawingml::theme::Theme;
use kiln_ooxml::opc::rel_type_matches;
use kiln_ooxml::xml::XmlElement;

use super::Reader;
use super::context::PartCx;
use super::masters::{color_context, color_override};
use super::motion::find_transition;
use crate::common::kind::rel;
use crate::common::warnings::{Severity, WarningCode};
use crate::common::xml::local;

/// Legacy comment positions are in master units, 576 per inch.
const EMU_PER_MASTER_UNIT: f64 = 914_400.0 / 576.0;

/// A comment author: name and initials.
pub(crate) type Authors = HashMap<String, (String, String)>;

/// The notes master's theme and colour map, for notes pages' colours.
pub(crate) type NotesTheme = Option<(Theme, kiln_deck::ColorMapping)>;

impl Reader<'_> {
    /// Reads a slide whose id was allocated up front.
    pub fn read_slide(
        &mut self,
        part: &str,
        id: NodeId,
        notes_theme: &NotesTheme,
        legacy_authors: &Authors,
        modern_authors: &Authors,
    ) -> Option<Slide> {
        let doc = self.part_xml(part, "the presentation")?;
        let root = &doc.root;
        if root.name != "p:sld" {
            self.warn(
                WarningCode::PartUnusable,
                Severity::Warning,
                format!("a slide part holds a {}", root.name),
                part,
                "",
            );
            return None;
        }
        self.consume(part);
        let rels = PartCx::new(self, part, &root.attrs, None).rels;
        let layout = rels
            .all()
            .iter()
            .find(|r| !r.is_external() && rel_type_matches(&r.rel_type, rel::SLIDE_LAYOUT))
            .and_then(|r| rels.resolve(r))
            .and_then(|l| self.layout_for(&l));
        let layout = match layout {
            Some(l) => l,
            None => {
                self.warn(
                    WarningCode::PartMissing,
                    Severity::Warning,
                    "the slide's layout is missing; the first layout is used",
                    part,
                    "",
                );
                self.pres.masters.first()?.layouts.first()?.id
            }
        };
        let color_map = color_override(root);
        let ctx = self.pres.layout(layout).map(|(m, l)| {
            color_context(&m.theme, &color_map.or(l.color_map).unwrap_or(m.color_map))
        });
        // `mc:AlternateContent` at the root holds the transition in PowerPoint 2010+ files.
        let transition_el = find_transition(root);
        let mut modelled = vec!["p:clrMapOvr", "p:transition", "p:timing"];
        if transition_el.is_some_and(|(_, k)| k.name == "mc:AlternateContent") {
            modelled.push("mc:AlternateContent");
        }
        let like = self.read_slide_like(part, root, id, ctx, &modelled);
        // Other mc:AlternateContent blocks at the root (not the transition) are kept.
        if let Some((_, kept)) = transition_el {
            for c in root.elements() {
                if c.name == "mc:AlternateContent" && !std::ptr::eq(c, kept) {
                    self.extras
                        .push_owner_xml(id, crate::common::xml::fragment(c, &root.attrs));
                }
            }
        }
        let transition =
            transition_el.map(|(t, kept)| self.read_transition(part, t, kept, id, &root.attrs));
        let timeline = root
            .child("p:timing")
            .map(|t| self.read_timing(part, t, id, &like.cx.ids, &root.attrs))
            .unwrap_or_default();
        self.extras.part_names.insert(id, part.to_owned());

        let notes = rels
            .all()
            .iter()
            .find(|r| !r.is_external() && rel_type_matches(&r.rel_type, rel::NOTES_SLIDE))
            .and_then(|r| rels.resolve(r))
            .and_then(|n| self.read_notes(&n, id, notes_theme));
        let mut comments = Vec::new();
        for r in rels.all() {
            if r.is_external() {
                continue;
            }
            let Some(target) = rels.resolve(r) else {
                continue;
            };
            if rel_type_matches(&r.rel_type, rel::COMMENTS) {
                self.read_legacy_comments(&target, legacy_authors, &mut comments);
            } else if rel_type_matches(&r.rel_type, rel::MODERN_COMMENTS) {
                self.read_modern_comments(&target, modern_authors, &mut comments);
            }
        }
        Some(Slide {
            id,
            layout,
            name: like.name,
            shapes: like.shapes,
            background: like.background,
            color_map,
            show_master_shapes: root.attr_bool("showMasterSp").unwrap_or(true),
            hidden: !root.attr_bool("show").unwrap_or(true),
            transition,
            timeline,
            notes,
            comments,
        })
    }

    /// The layout of a slide: read already, or read now from a master that did not list it.
    fn layout_for(&mut self, layout_part: &str) -> Option<NodeId> {
        if let Some(id) = self.layout_ids.get(layout_part) {
            return Some(*id);
        }
        let master_part = self
            .pkg
            .related_part(layout_part, rel::SLIDE_MASTER)
            .ok()
            .flatten()?;
        let master = self
            .extras
            .part_names
            .iter()
            .find(|(id, p)| **p == master_part && self.pres.master(**id).is_some())
            .map(|(id, _)| *id)?;
        self.read_layout(layout_part, master, None)
    }

    fn read_notes(&mut self, part: &str, slide: NodeId, theme: &NotesTheme) -> Option<Notes> {
        let doc = self.part_xml(part, "a slide")?;
        let root = &doc.root;
        if root.name != "p:notes" {
            return None;
        }
        self.consume(part);
        self.extras.notes_parts.insert(slide, part.to_owned());
        let ctx = theme
            .as_ref()
            .map(|(t, map)| color_context(t, &color_override(root).unwrap_or(*map)));
        // The notes page has no id in the model; its kept XML is filed under one of its own.
        let key = self.pres.alloc_id();
        self.extras.notes_keys.insert(slide, key);
        let like = self.read_slide_like(part, root, key, ctx, &["p:clrMapOvr"]);
        let mut text = None;
        let mut shapes = Vec::new();
        for s in like.shapes {
            let is_body = s
                .placeholder
                .is_some_and(|p| p.kind == PlaceholderType::Body);
            if is_body && text.is_none() {
                text = Some(match s.kind {
                    ShapeKind::Geometry(a) => a.text.unwrap_or_default(),
                    _ => TextBody::new(),
                });
                continue;
            }
            shapes.push(s);
        }
        Some(Notes {
            text: text.unwrap_or_default(),
            shapes,
        })
    }

    fn read_legacy_comments(&mut self, part: &str, authors: &Authors, out: &mut Vec<Comment>) {
        let Some(doc) = self.part_xml(part, "a slide") else {
            return;
        };
        self.consume(part);
        for cm in doc.root.children_named("p:cm") {
            let author_id = cm.attr("authorId").unwrap_or_default();
            let (author, initials) = self.author(authors, author_id, part);
            let pos = cm.child("p:pos").map_or((0, 0), |p| {
                let f = |a: &str| {
                    let v = p.attr_num(a).unwrap_or(0.0) * EMU_PER_MASTER_UNIT;
                    kiln_deck::geom::round_emu(v).clamp(0, kiln_deck::geom::MAX_COORD)
                };
                (f("x"), f("y"))
            });
            out.push(Comment {
                id: self.pres.alloc_id(),
                author,
                initials,
                created: cm.attr("dt").unwrap_or_default().to_owned(),
                text: cm.child("p:text").map(XmlElement::text).unwrap_or_default(),
                shape: None,
                pos,
                replies: Vec::new(),
                resolved: false,
            });
        }
    }

    fn read_modern_comments(&mut self, part: &str, authors: &Authors, out: &mut Vec<Comment>) {
        let Some(doc) = self.part_xml(part, "a slide") else {
            return;
        };
        self.consume(part);
        self.extras.modern_comments = true;
        for cm in doc.root.elements().filter(|e| local(e) == "cm") {
            let (author, initials) =
                self.author(authors, cm.attr("authorId").unwrap_or_default(), part);
            let pos = cm
                .elements()
                .find(|e| local(e) == "pos")
                .map_or((0, 0), |p| {
                    let f = |a: &str| {
                        p.attr_int(a)
                            .unwrap_or(0)
                            .clamp(0, kiln_deck::geom::MAX_COORD)
                    };
                    (f("x"), f("y"))
                });
            let mut replies = Vec::new();
            if let Some(list) = cm.elements().find(|e| local(e) == "replyLst") {
                for r in list.elements().filter(|e| local(e) == "reply") {
                    let (author, _) =
                        self.author(authors, r.attr("authorId").unwrap_or_default(), part);
                    replies.push(CommentReply {
                        id: self.pres.alloc_id(),
                        author,
                        created: r.attr("created").unwrap_or_default().to_owned(),
                        text: body_text(r),
                    });
                }
            }
            out.push(Comment {
                id: self.pres.alloc_id(),
                author,
                initials,
                created: cm.attr("created").unwrap_or_default().to_owned(),
                text: body_text(cm),
                shape: None,
                pos,
                replies,
                resolved: cm.attr("status") == Some("resolved"),
            });
        }
    }

    fn author(&mut self, authors: &Authors, id: &str, part: &str) -> (String, String) {
        if let Some(a) = authors.get(id) {
            return a.clone();
        }
        self.warn(
            WarningCode::CommentAuthorMissing,
            Severity::Warning,
            format!("a comment's author {id:?} is not listed"),
            part,
            "",
        );
        (String::new(), String::new())
    }

    /// Legacy authors (`p:cmAuthorLst`, keyed by number) or modern ones (`p188:authorLst`,
    /// keyed by GUID).
    pub fn read_authors(&mut self, part: &str) -> Authors {
        let mut out = Authors::new();
        let Some(doc) = self.part_xml(part, "the presentation") else {
            return out;
        };
        self.consume(part);
        for a in doc
            .root
            .elements()
            .filter(|e| matches!(local(e), "cmAuthor" | "author"))
        {
            let Some(id) = a.attr("id") else { continue };
            out.insert(
                id.to_owned(),
                (
                    a.attr("name").unwrap_or_default().to_owned(),
                    a.attr("initials").unwrap_or_default().to_owned(),
                ),
            );
        }
        out
    }
}

/// The plain text of a modern comment's `p188:txBody` (paragraphs joined by newlines).
fn body_text(cm: &XmlElement) -> String {
    let Some(body) = cm.elements().find(|e| local(e) == "txBody") else {
        return String::new();
    };
    body.children_named("a:p")
        .map(|p| {
            p.elements()
                .map(|r| match r.name.as_str() {
                    "a:r" | "a:fld" => r.child("a:t").map(XmlElement::text).unwrap_or_default(),
                    "a:br" => "\n".to_owned(),
                    _ => String::new(),
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
