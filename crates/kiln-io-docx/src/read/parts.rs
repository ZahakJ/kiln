// SPDX-License-Identifier: AGPL-3.0-or-later
//! Secondary stories: headers and footers, footnotes and endnotes, comments (with Word 2013+
//! threads from `commentsExtended.xml`) and comment authors (`people.xml`).
//!
//! Notes and comments are read in two phases: first every id is mapped to a model node (so any
//! story, including a footnote, can refer to any note or comment), then the bodies are read.

use std::collections::HashMap;

use kiln_doc::model::{Comment, CommentMeta, HeaderFooter, HeaderFooterPart, Note};
use kiln_doc::st::{NoteKind, NoteType};
use kiln_doc::{Block, NodeId};
use kiln_ooxml::opc::rel_type;
use kiln_ooxml::xml::{XmlDocument, XmlElement};

use super::context::ReadContext;
use super::story::{BlockSink, StoryOptions, StoryReader};
use crate::common::extras::Person;
use crate::common::units::parse_on_off;
use crate::common::xml::{XmlPath, attr, attr_of, stepped};

/// Reads the blocks of one story container.
fn read_story(
    ctx: &mut ReadContext<'_>,
    part: &str,
    doc: &XmlDocument,
    container: &XmlElement,
    path: XmlPath,
    note: Option<(NodeId, NoteKind)>,
) -> Vec<Block> {
    let options = StoryOptions {
        part: part.to_owned(),
        scope: doc.root.attrs.clone(),
        path,
        note,
    };
    let mut story = StoryReader::new(ctx, options);
    let mut sink = BlockSink::default();
    story.read_blocks(container, &mut sink);
    let mut blocks = sink.into_blocks(story.ctx);
    story.finish_story(&mut blocks);
    blocks
}

/// Reads a header or footer part once; several sections may reference it.
pub(crate) fn read_header_footer(
    ctx: &mut ReadContext<'_>,
    part: &str,
    kind: HeaderFooterPart,
    cache: &mut HashMap<String, NodeId>,
) -> Option<NodeId> {
    if let Some(id) = cache.get(part) {
        return Some(*id);
    }
    let doc = ctx.load_part(part)?;
    ctx.consume(part);
    let path = XmlPath::new(&doc.root.name);
    let blocks = read_story(ctx, part, &doc, &doc.root, path, None);
    let id = ctx.doc.alloc_id();
    ctx.doc.headers_footers.push(HeaderFooter {
        id,
        part: kind,
        blocks,
    });
    ctx.extras.header_footer_parts.insert(id, part.to_owned());
    cache.insert(part.to_owned(), id);
    Some(id)
}

/// A loaded notes or comments part whose ids are mapped, ready for its bodies to be read.
pub(crate) struct Prepared {
    part: String,
    doc: XmlDocument,
    /// `(element index among the root's children, node)`.
    items: Vec<(usize, NodeId)>,
}

fn note_type(v: Option<&str>) -> NoteType {
    v.and_then(NoteType::parse).unwrap_or_default()
}

/// Loads `footnotes.xml` or `endnotes.xml` and maps its ids to nodes.
pub(crate) fn prepare_notes(
    ctx: &mut ReadContext<'_>,
    main: &str,
    kind: NoteKind,
) -> Option<Prepared> {
    let rel = match kind {
        NoteKind::Footnote => rel_type::FOOTNOTES,
        NoteKind::Endnote => rel_type::ENDNOTES,
    };
    let part = ctx.related_part(main, rel)?;
    let doc = ctx.load_part(&part)?;
    ctx.consume(&part);
    let name = match kind {
        NoteKind::Footnote => "w:footnote",
        NoteKind::Endnote => "w:endnote",
    };
    let mut items = Vec::new();
    for (i, el) in doc.root.elements().enumerate() {
        if el.name != name {
            continue;
        }
        let node = ctx.doc.alloc_id();
        let id = attr(el, "w:id").unwrap_or_default().to_owned();
        if note_type(attr(el, "w:type")) == NoteType::Normal {
            ctx.note_ids.entry((kind, id.clone())).or_insert(node);
        }
        ctx.extras.note_ids.insert(node, id);
        items.push((i, node));
    }
    Some(Prepared { part, doc, items })
}

/// Reads the note bodies of a prepared notes part.
pub(crate) fn read_notes(ctx: &mut ReadContext<'_>, prepared: &Prepared, kind: NoteKind) {
    let children = stepped(&prepared.doc.root);
    for (index, node) in &prepared.items {
        let Some((el, step)) = children.get(*index) else {
            continue;
        };
        let mut path = XmlPath::new(&prepared.doc.root.name);
        path.push(step.clone());
        let blocks = read_story(
            ctx,
            &prepared.part,
            &prepared.doc,
            el,
            path,
            Some((*node, kind)),
        );
        ctx.doc.notes.push(Note {
            id: *node,
            kind,
            note_type: note_type(attr(el, "w:type")),
            blocks,
        });
    }
}

/// Loads `comments.xml` and maps its ids to nodes.
pub(crate) fn prepare_comments(ctx: &mut ReadContext<'_>, main: &str) -> Option<Prepared> {
    let part = ctx.related_part(main, rel_type::COMMENTS)?;
    let doc = ctx.load_part(&part)?;
    ctx.consume(&part);
    let mut items = Vec::new();
    for (i, el) in doc.root.elements().enumerate() {
        if el.name != "w:comment" {
            continue;
        }
        let id = attr(el, "w:id").unwrap_or_default().to_owned();
        if ctx.comment_ids.contains_key(&id) {
            continue; // a duplicate id: Word keeps the first comment
        }
        let node = ctx.doc.alloc_id();
        ctx.comment_ids.insert(id.clone(), node);
        ctx.extras.comment_ids.insert(node, id);
        items.push((i, node));
    }
    Some(Prepared { part, doc, items })
}

/// The `w14:paraId` of a comment's last paragraph (the key `commentsExtended.xml` uses).
fn last_para_id(comment: &XmlElement) -> Option<String> {
    comment
        .descendants("w:p")
        .last()
        .and_then(|p| p.attr("w14:paraId"))
        .map(str::to_owned)
}

/// Reads the comment bodies and threads (`commentsExtended.xml`).
pub(crate) fn read_comments(ctx: &mut ReadContext<'_>, main: &str, prepared: &Prepared) {
    // Threads: paraId → (parent paraId, done).
    let mut ext: HashMap<String, (Option<String>, bool)> = HashMap::new();
    if let Some(ext_part) = ctx.related_part(main, rel_type::COMMENTS_EXTENDED)
        && let Some(ext_doc) = ctx.load_part(&ext_part)
    {
        ctx.consume(&ext_part);
        for ce in ext_doc.root.children_named("w15:commentEx") {
            if let Some(para) = ce.attr("w15:paraId") {
                ext.insert(
                    para.to_owned(),
                    (
                        ce.attr("w15:paraIdParent").map(str::to_owned),
                        parse_on_off(ce.attr("w15:done"), false),
                    ),
                );
            }
        }
    }
    let children = stepped(&prepared.doc.root);
    let mut by_para: HashMap<String, NodeId> = HashMap::new();
    let mut paras: Vec<Option<String>> = Vec::new();
    for (index, node) in &prepared.items {
        let Some((el, step)) = children.get(*index) else {
            continue;
        };
        let mut path = XmlPath::new(&prepared.doc.root.name);
        path.push(step.clone());
        let blocks = read_story(ctx, &prepared.part, &prepared.doc, el, path, None);
        let para = last_para_id(el);
        if let Some(p) = &para {
            by_para.insert(p.clone(), *node);
        }
        paras.push(para);
        ctx.doc.comments.push(Comment {
            id: *node,
            meta: CommentMeta {
                author: attr(el, "w:author").unwrap_or_default().to_owned(),
                initials: attr(el, "w:initials").map(str::to_owned),
                date: attr(el, "w:date").map(str::to_owned),
                parent: None,
                resolved: false,
            },
            blocks,
        });
    }
    let start = ctx.doc.comments.len() - paras.len();
    for (k, para) in paras.into_iter().enumerate() {
        let Some((parent, done)) = para.and_then(|p| ext.get(&p).cloned()) else {
            continue;
        };
        let comment = &mut ctx.doc.comments[start + k];
        comment.meta.resolved = done;
        if let Some(parent) = parent.and_then(|p| by_para.get(&p).copied())
            && parent != comment.id
        {
            comment.meta.parent = Some(parent);
        }
    }
}

/// `people.xml`: comment authors with their identity provider.
pub(crate) fn read_people(ctx: &mut ReadContext<'_>, main: &str) -> Vec<Person> {
    let Some(part) = ctx.related_part(main, rel_type::PEOPLE) else {
        return Vec::new();
    };
    let Some(doc) = ctx.load_part(&part) else {
        return Vec::new();
    };
    ctx.consume(&part);
    doc.root
        .children_named("w15:person")
        .map(|p| {
            let info = p.child("w15:presenceInfo");
            Person {
                author: p.attr("w15:author").unwrap_or_default().to_owned(),
                provider_id: attr_of(info, "w15:providerId").map(str::to_owned),
                user_id: attr_of(info, "w15:userId").map(str::to_owned),
            }
        })
        .collect()
}
