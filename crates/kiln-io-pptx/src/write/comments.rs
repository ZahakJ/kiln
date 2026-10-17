// SPDX-License-Identifier: AGPL-3.0-or-later
//! Comments: legacy comment parts (`p:cmLst`, one per slide) with `commentAuthors.xml`, or
//! modern threaded comments (`p188:cmLst` with replies and resolved state) with `authors.xml`.
//! Files read with modern comments, and comments with replies or a resolved state, are written
//! as modern comments (legacy comments have neither).

use std::collections::BTreeMap;

use kiln_deck::slide::Comment;
use kiln_deck::{Presentation, Slide};
use kiln_ooxml::xml::XmlElement;

use super::xml::{NS_P188, NS_PC, encode_text, stable_guid};

/// Legacy comment positions are in master units, 576 per inch.
const EMU_PER_MASTER_UNIT: f64 = 914_400.0 / 576.0;

/// The legacy comment part content type.
pub(crate) const LEGACY_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.comments+xml";
/// The legacy authors part content type.
pub(crate) const LEGACY_AUTHORS_CT: &str =
    "application/vnd.openxmlformats-officedocument.presentationml.commentAuthors+xml";
/// The modern comment part content type.
pub(crate) const MODERN_CT: &str = "application/vnd.ms-powerpoint.comments+xml";
/// The modern authors part content type.
pub(crate) const MODERN_AUTHORS_CT: &str = "application/vnd.ms-powerpoint.authors+xml";

/// Whether the presentation's comments are written as modern comments.
pub(crate) fn use_modern(pres: &Presentation, read_modern: bool) -> bool {
    read_modern
        || pres
            .slides
            .iter()
            .flat_map(|s| &s.comments)
            .any(|c| c.resolved || !c.replies.is_empty())
}

/// One comment author.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Author {
    pub name: String,
    pub initials: String,
    /// Legacy: the number of comments (the last index used).
    pub count: u32,
}

/// The authors of every comment and reply, in order of first appearance.
pub(crate) fn authors(pres: &Presentation) -> Vec<Author> {
    let mut out: Vec<Author> = Vec::new();
    let mut add = |name: &str, initials: &str| match out.iter_mut().find(|a| a.name == name) {
        Some(a) => {
            a.count += 1;
            if a.initials.is_empty() {
                a.initials = initials.to_owned();
            }
        }
        None => out.push(Author {
            name: name.to_owned(),
            initials: initials.to_owned(),
            count: 1,
        }),
    };
    for s in &pres.slides {
        for c in &s.comments {
            add(&c.author, &c.initials);
            for r in &c.replies {
                add(&r.author, "");
            }
        }
    }
    out
}

fn author_index(authors: &[Author], name: &str) -> usize {
    authors.iter().position(|a| a.name == name).unwrap_or(0)
}

fn author_guid(name: &str) -> String {
    stable_guid(&format!("author:{name}"))
}

/// `p:cmAuthorLst`.
pub(crate) fn legacy_authors(authors: &[Author]) -> XmlElement {
    super::xml::part_root("p:cmAuthorLst").with_children(authors.iter().enumerate().map(
        |(i, a)| {
            XmlElement::new("p:cmAuthor")
                .with_attr("id", i)
                .with_attr("name", a.name.as_str())
                .with_attr("initials", a.initials.as_str())
                .with_attr("lastIdx", a.count)
                .with_attr("clrIdx", i % 8)
        },
    ))
}

/// `p188:authorLst`.
pub(crate) fn modern_authors(authors: &[Author]) -> XmlElement {
    super::xml::part_root("p188:authorLst")
        .with_attr("xmlns:p188", NS_P188)
        .with_children(authors.iter().map(|a| {
            XmlElement::new("p188:author")
                .with_attr("id", author_guid(&a.name))
                .with_attr("name", a.name.as_str())
                .with_attr("initials", a.initials.as_str())
                .with_attr("userId", a.name.as_str())
                .with_attr("providerId", "None")
        }))
}

/// `p:cmLst` of a slide. `next_idx` numbers each author's comments across the presentation.
pub(crate) fn legacy_comments(
    slide: &Slide,
    authors: &[Author],
    next_idx: &mut BTreeMap<usize, u32>,
) -> XmlElement {
    let to_units = |emu: i64| {
        let v = (emu as f64 / EMU_PER_MASTER_UNIT).round();
        // Positions are clamped to the DrawingML range on read, so this fits.
        v as i64
    };
    super::xml::part_root("p:cmLst").with_children(slide.comments.iter().map(|c| {
        let a = author_index(authors, &c.author);
        let idx = next_idx.entry(a).or_insert(0);
        *idx += 1;
        XmlElement::new("p:cm")
            .with_attr("authorId", a)
            .with_opt_attr("dt", (!c.created.is_empty()).then_some(c.created.as_str()))
            .with_attr("idx", *idx)
            .with_child(
                XmlElement::new("p:pos")
                    .with_attr("x", to_units(c.pos.0))
                    .with_attr("y", to_units(c.pos.1)),
            )
            .with_child(XmlElement::new("p:text").with_text(c.text.as_str()))
    }))
}

/// A comment or reply body (`p188:txBody`): one paragraph per line.
fn modern_body(text: &str) -> XmlElement {
    XmlElement::new("p188:txBody")
        .with_child(XmlElement::new("a:bodyPr"))
        .with_child(XmlElement::new("a:lstStyle"))
        .with_children(text.split('\n').map(|line| {
            let p = XmlElement::new("a:p");
            if line.is_empty() {
                p
            } else {
                p.with_child(
                    XmlElement::new("a:r")
                        .with_child(XmlElement::new("a:t").with_text(encode_text(line))),
                )
            }
        }))
}

/// `p188:cmLst` of a slide. `slide_id` is its `p:sldId/@id`, `creation_id` its
/// `p14:creationId` (comments are anchored to both).
pub(crate) fn modern_comments(slide: &Slide, slide_id: u32, creation_id: u32) -> XmlElement {
    let comment = |c: &Comment| {
        let replies = (!c.replies.is_empty()).then(|| {
            XmlElement::new("p188:replyLst").with_children(c.replies.iter().map(|r| {
                XmlElement::new("p188:reply")
                    .with_attr("id", stable_guid(&format!("reply:{}", r.id.0)))
                    .with_attr("authorId", author_guid(&r.author))
                    .with_opt_attr(
                        "created",
                        (!r.created.is_empty()).then_some(r.created.as_str()),
                    )
                    .with_child(modern_body(&r.text))
            }))
        });
        XmlElement::new("p188:cm")
            .with_attr("id", stable_guid(&format!("comment:{}", c.id.0)))
            .with_attr("authorId", author_guid(&c.author))
            .with_opt_attr("status", c.resolved.then_some("resolved"))
            .with_opt_attr(
                "created",
                (!c.created.is_empty()).then_some(c.created.as_str()),
            )
            .with_child(
                XmlElement::new("pc:sldMkLst")
                    .with_attr("xmlns:pc", NS_PC)
                    .with_child(XmlElement::new("pc:docMk"))
                    .with_child(
                        XmlElement::new("pc:sldMk")
                            .with_attr("cId", creation_id)
                            .with_attr("sldId", slide_id),
                    ),
            )
            .with_child(
                XmlElement::new("p188:pos")
                    .with_attr("x", c.pos.0)
                    .with_attr("y", c.pos.1),
            )
            .with_opt_child(replies)
            .with_child(modern_body(&c.text))
    };
    super::xml::part_root("p188:cmLst")
        .with_attr("xmlns:p188", NS_P188)
        .with_children(slide.comments.iter().map(comment))
}

/// EMU from master units (for tests of the inverse).
#[cfg(test)]
fn from_units(v: i64) -> i64 {
    kiln_deck::geom::round_emu(v as f64 * EMU_PER_MASTER_UNIT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_common::NodeId;
    use kiln_deck::slide::CommentReply;

    fn comment(id: u64, author: &str, replies: usize) -> Comment {
        Comment {
            id: NodeId(id),
            author: author.into(),
            initials: author[..1].into(),
            created: "2026-01-02T03:04:05.000".into(),
            text: "Line one\nLine two".into(),
            shape: None,
            pos: (from_units(576), from_units(1152)),
            replies: (0..replies)
                .map(|i| CommentReply {
                    id: NodeId(id * 10 + i as u64),
                    author: "Bea".into(),
                    created: String::new(),
                    text: "ok".into(),
                })
                .collect(),
            resolved: false,
        }
    }

    #[test]
    fn authors_are_collected_in_order() {
        let mut p = Presentation::new();
        let layout = p.masters[0].layouts[0].id;
        let mut s = Slide::new(p.alloc_id(), layout);
        s.comments = vec![comment(90, "Ana", 1), comment(91, "Ana", 0)];
        p.slides.push(s);
        let a = authors(&p);
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].name, "Ana");
        assert_eq!(a[0].count, 2);
        assert_eq!(a[1].name, "Bea");
        assert!(use_modern(&p, false));
        let mut idx = BTreeMap::new();
        let x = legacy_comments(p.slides.last().unwrap(), &a, &mut idx);
        let pos = x.first_element().unwrap().child("p:pos").unwrap();
        assert_eq!(pos.attr("x"), Some("576"));
        assert_eq!(pos.attr("y"), Some("1152"));
        let m = modern_comments(p.slides.last().unwrap(), 256, 0);
        assert_eq!(m.elements().count(), 2);
        let first = m.first_element().unwrap();
        assert!(first.child("p188:replyLst").is_some());
        assert_eq!(
            first
                .child("p188:txBody")
                .unwrap()
                .children_named("a:p")
                .count(),
            2
        );
    }
}
