// SPDX-License-Identifier: AGPL-3.0-or-later
//! Insertions: hyperlinks (and Remove Hyperlink), bookmarks, the date and time (as text or a
//! DATE field), fields from their codes, Update Fields, drop caps and pictures (decoded once to
//! size them; inline, or floating with square wrapping).

use kiln_common::{KilnError, Result};
use kiln_doc::fields::{FieldContext, evaluate_field, format_field_date};
use kiln_doc::st::{AlignH, DropCapKind, RelativeFromH, RelativeFromV, WrapSide, WrapType};
use kiln_doc::{
    Anchor, Drawing, EditKind, Field, Fragment, Hyperlink, InlineObject, LinkTarget, Mark, Media,
    Op, Placement, PosH, PosV, Range, Transaction, edit,
};
use kiln_protocol::commands::DropCapPlacement;

use super::doc_err;
use crate::apps::write::WriteDoc;

/// EMU per pixel at 96 dpi.
const EMU_PER_PX: i64 = 9525;
/// The largest picture the core accepts (Word's limit is far higher; this keeps a browser tab
/// responsive).
const MAX_PICTURE_BYTES: usize = 64 * 1024 * 1024;

/// Whether a URL may become a hyperlink (no `javascript:` or `data:`).
pub(super) fn safe_url(url: &str) -> bool {
    let u = url.trim().to_ascii_lowercase();
    u.starts_with("http://")
        || u.starts_with("https://")
        || u.starts_with("mailto:")
        || u.starts_with("ftp://")
        || u.starts_with("tel:")
        || u.starts_with("file://")
        || u.starts_with('#')
}

/// A bookmark name Word accepts: a letter first, then letters, digits and `_`, up to 40.
fn valid_bookmark(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(char::is_alphabetic)
        && name.chars().count() <= 40
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

fn media_type(name: &str, bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    let sniff = |prefix: &[u8]| bytes.starts_with(prefix);
    Some(match ext.as_str() {
        _ if sniff(b"\x89PNG") => ("png", "image/png"),
        _ if sniff(b"\xFF\xD8\xFF") => ("jpeg", "image/jpeg"),
        _ if sniff(b"GIF8") => ("gif", "image/gif"),
        _ if sniff(b"BM") => ("bmp", "image/bmp"),
        _ if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" => {
            ("webp", "image/webp")
        }
        _ => return None,
    })
}

impl WriteDoc {
    /// Ctrl+K: links the selection, or inserts `text` (the URL by default) as a link.
    pub(super) fn insert_link(
        &mut self,
        url: &str,
        text: Option<&str>,
        tooltip: Option<&str>,
    ) -> Result<()> {
        let url = url.trim();
        if url.is_empty() || !safe_url(url) {
            return Err(KilnError::InvalidInput(format!(
                "'{url}' is not a web address, e-mail link or #bookmark"
            )));
        }
        let target = match url.strip_prefix('#') {
            Some(b) => LinkTarget::Bookmark(b.to_owned()),
            None => LinkTarget::Url(url.to_owned()),
        };
        let link = Mark::Link(Hyperlink {
            target,
            tooltip: tooltip.map(str::to_owned).filter(|t| !t.is_empty()),
            frame: None,
            history: true,
        });
        self.ensure_style("Hyperlink")?;
        let style = self
            .doc
            .styles
            .contains("Hyperlink")
            .then(|| "Hyperlink".to_owned());
        let (s, e) = self.span();
        if s == e || text.is_some_and(|t| !t.is_empty()) {
            self.delete_selection()?;
            let at = self.head;
            let shown = text.filter(|t| !t.is_empty()).unwrap_or(url);
            let mut props = self.caret_props()?;
            props.style_id.clone_from(&style);
            let fragment = Fragment::text(&shown.replace(['\n', '\r'], " "), props).with_mark(link);
            let len = fragment.len();
            let tx = Transaction::new(vec![Op::InsertInline {
                para: at.para,
                offset: at.offset,
                fragment,
            }])
            .with_kind(EditKind::Structure)
            .with_selection(
                Some(Range::caret(at)),
                Some(Range::caret(kiln_doc::Pos::new(at.para, at.offset + len))),
            );
            return self.commit(tx);
        }
        let mut ops = Vec::new();
        for (id, a, b) in edit::segments(&self.doc, s, e).map_err(doc_err)? {
            if a < b {
                // Replace any link already there.
                let p = self.para(id)?;
                for m in p.content.marks() {
                    if matches!(m.mark, Mark::Link(_)) && m.start < b && a < m.end {
                        ops.push(Op::RemoveMark {
                            para: id,
                            start: m.start,
                            end: m.end,
                            mark: m.mark.clone(),
                        });
                    }
                }
                ops.push(Op::AddMark {
                    para: id,
                    start: a,
                    end: b,
                    mark: link.clone(),
                });
            }
        }
        let fmt = edit::format_runs(&self.doc, s, e, |r| r.style_id.clone_from(&style))
            .map_err(doc_err)?;
        ops.extend(fmt.ops);
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Remove Hyperlink: the links over the selection (or around the caret) go, with the
    /// Hyperlink character style.
    pub(super) fn remove_link(&mut self) -> Result<()> {
        let (s, e) = self.span();
        let mut ops = Vec::new();
        for (id, a, b) in edit::segments(&self.doc, s, e).map_err(doc_err)? {
            let p = self.para(id)?;
            for m in p.content.marks() {
                let touches = if a == b {
                    m.start <= a && a <= m.end
                } else {
                    m.start < b && a < m.end
                };
                if matches!(m.mark, Mark::Link(_)) && touches {
                    ops.push(Op::RemoveMark {
                        para: id,
                        start: m.start,
                        end: m.end,
                        mark: m.mark.clone(),
                    });
                    let mut runs = Vec::new();
                    let mut pos = 0;
                    for r in p.content.runs() {
                        let (x, y) = (pos.max(m.start), (pos + r.len).min(m.end));
                        if x < y {
                            let mut props = r.props.clone();
                            if props.style_id.as_deref() == Some("Hyperlink") {
                                props.style_id = None;
                            }
                            runs.push(kiln_doc::RunSpan::new(y - x, props));
                        }
                        pos += r.len;
                    }
                    ops.push(Op::SetRuns {
                        para: id,
                        offset: m.start,
                        runs,
                    });
                }
            }
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Insert ▸ Bookmark around the selection (a name in use moves to the selection).
    pub(super) fn insert_bookmark(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if !valid_bookmark(name) {
            return Err(KilnError::InvalidInput(format!(
                "'{name}' is not a bookmark name: start with a letter, then letters, digits or _ (up to 40)"
            )));
        }
        let (s, e) = self.span();
        let mut batch = self.batch(EditKind::Structure);
        // Remove an existing bookmark of that name (its end first, then its start).
        for story_para in self
            .doc
            .all_paragraphs()
            .iter()
            .map(|(_, p)| p.id)
            .collect::<Vec<_>>()
        {
            let p = self.para(story_para)?;
            let mut hits: Vec<usize> = p
                .content
                .objects_with_offsets()
                .filter(|(_, o)| {
                    matches!(o, InlineObject::BookmarkStart { name: n } | InlineObject::BookmarkEnd { name: n } if n == name)
                })
                .map(|(off, _)| off)
                .collect();
            hits.sort_unstable_by(|a, b| b.cmp(a));
            for off in hits {
                self.batch_apply(
                    &mut batch,
                    Transaction::new(vec![Op::DeleteInline {
                        para: story_para,
                        offset: off,
                        len: 1,
                    }]),
                )?;
            }
        }
        // The selection may have moved left by removed markers: re-read it.
        let (s2, e2) = (
            self.doc.clamp(s).unwrap_or(s),
            self.doc.clamp(e).unwrap_or(e),
        );
        let props = kiln_doc::RunProps::default();
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::InsertInline {
                para: e2.para,
                offset: e2.offset,
                fragment: Fragment::object(
                    InlineObject::BookmarkEnd {
                        name: name.to_owned(),
                    },
                    props.clone(),
                ),
            }]),
        )?;
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![Op::InsertInline {
                para: s2.para,
                offset: s2.offset,
                fragment: Fragment::object(
                    InlineObject::BookmarkStart {
                        name: name.to_owned(),
                    },
                    props,
                ),
            }]),
        )?;
        let shift = |p: kiln_doc::Pos, extra: usize| kiln_doc::Pos::new(p.para, p.offset + extra);
        let start = shift(s2, 1);
        let end = if e2.para == s2.para { shift(e2, 1) } else { e2 };
        self.batch_commit(batch, Range::new(start, end));
        Ok(())
    }

    /// Ctrl+; and Ctrl+Shift+; : today's date or the time as text.
    pub(super) fn insert_now(&mut self, time: bool) -> Result<()> {
        let picture = if time { "h:mm AM/PM" } else { "M/d/yyyy" };
        let text = format_field_date(picture, &self.now());
        self.insert_text_at_caret(&text)
    }

    /// Insert ▸ Date & Time.
    pub(super) fn insert_date_time(&mut self, format: &str, as_field: bool) -> Result<()> {
        let format = format.trim();
        if format.is_empty() || format.len() > 255 {
            return Err(KilnError::InvalidInput("a date format is needed".into()));
        }
        let text = format_field_date(format, &self.now());
        if !as_field {
            return self.insert_text_at_caret(&text);
        }
        let instr = format!("DATE \\@ \"{}\"", format.replace('"', "'"));
        self.insert_field_object(Field::new(&instr, &text))
    }

    /// Quick Parts ▸ Field from a field code.
    pub(super) fn insert_field(&mut self, code: &str) -> Result<()> {
        let code = code.trim();
        if code.is_empty() || code.len() > 255 {
            return Err(KilnError::InvalidInput(
                "a field code has 1–255 characters".into(),
            ));
        }
        let field = Field::new(code, "");
        let result = self.field_result(&field).unwrap_or_default();
        self.insert_field_object(Field {
            result,
            dirty: true,
            ..field
        })
    }

    fn field_result(&self, field: &Field) -> Option<String> {
        let parsed = field.parsed();
        let pages = self
            .view
            .as_ref()
            .map(|v| i64::try_from(v.layout.pages().len()).unwrap_or(0));
        let page = self.view.as_ref().and_then(|v| {
            v.caret(self.head)
                .and_then(|c| v.layout.pages().get(c.page).map(|p| p.number))
        });
        let doc = &self.doc;
        let text = |name: &str| doc.bookmark_text(name);
        let ctx = FieldContext {
            page,
            num_pages: pages,
            now: Some(self.now()),
            bookmark_text: Some(&text),
            ..FieldContext::default()
        };
        evaluate_field(&parsed, &ctx).or_else(|| {
            let kw = field.instr.split_whitespace().next()?.to_ascii_uppercase();
            let props = &doc.properties;
            match kw.as_str() {
                "AUTHOR" => props.creator.clone(),
                "TITLE" => props.title.clone(),
                "SUBJECT" => props.subject.clone(),
                "FILENAME" => self
                    .file_name
                    .as_ref()
                    .map(|f| f.rsplit(['/', '\\']).next().unwrap_or(f).to_owned()),
                _ => None,
            }
        })
    }

    fn insert_field_object(&mut self, field: Field) -> Result<()> {
        self.delete_selection()?;
        let at = self.head;
        let props = self.caret_props()?;
        let tx = Transaction::new(vec![Op::InsertInline {
            para: at.para,
            offset: at.offset,
            fragment: Fragment::object(InlineObject::Field(field), props),
        }])
        .with_kind(EditKind::Structure)
        .with_selection(
            Some(Range::caret(at)),
            Some(Range::caret(kiln_doc::Pos::new(at.para, at.offset + 1))),
        );
        self.commit(tx)
    }

    fn insert_text_at_caret(&mut self, text: &str) -> Result<()> {
        if self.pending_applies() {
            return self.insert_pending(text);
        }
        self.insert_plain(text)
    }

    /// F9: recomputes the fields of the selection (the whole document when nothing is
    /// selected) that the core can evaluate; page fields follow the layout by themselves.
    pub(super) fn update_fields(&mut self) -> Result<()> {
        let (s, e) = self.span();
        let ids: Vec<kiln_doc::NodeId> = if s == e {
            self.doc
                .all_paragraphs()
                .iter()
                .map(|(_, p)| p.id)
                .collect()
        } else {
            self.selected_paragraphs()?
        };
        let mut ops = Vec::new();
        for id in ids {
            let p = self.para(id)?;
            for (offset, o) in p.content.objects_with_offsets() {
                if let InlineObject::Field(f) = o
                    && !f.locked
                    && let Some(result) = self.field_result(f)
                    && result != f.result
                {
                    ops.push(Op::SetObject {
                        para: id,
                        offset,
                        object: InlineObject::Field(Field {
                            result,
                            dirty: false,
                            ..f.clone()
                        }),
                    });
                }
            }
        }
        let sel = Range::new(self.anchor, self.head);
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Other)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// Insert ▸ Drop Cap.
    pub(super) fn drop_cap(&mut self, placement: Option<DropCapPlacement>) -> Result<()> {
        let (s, _) = self.span();
        let id = s.para;
        let cap = placement.map(|p| kiln_doc::props::DropCap {
            kind: match p {
                DropCapPlacement::Dropped => DropCapKind::Drop,
                DropCapPlacement::InMargin => DropCapKind::Margin,
            },
            lines: 3,
        });
        self.format_para_ids(&[id], move |p, _| p.drop_cap = cap)
    }

    /// Inserts a picture from its bytes: inline at the caret (scaled down to the text width),
    /// or floating with square wrapping.
    pub(super) fn insert_picture(
        &mut self,
        name: &str,
        bytes: &[u8],
        floating: bool,
    ) -> Result<()> {
        if bytes.is_empty() || bytes.len() > MAX_PICTURE_BYTES {
            return Err(KilnError::InvalidInput(
                "a picture of 1 byte to 64 MB is needed".into(),
            ));
        }
        let Some((ext, content_type)) = media_type(name, bytes) else {
            return Err(KilnError::Unsupported(format!(
                "pictures of this type ('{name}'); Kiln inserts PNG, JPEG, GIF, BMP and WebP"
            )));
        };
        let (w, h) = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .ok()
            .and_then(|r| r.into_dimensions().ok())
            .ok_or_else(|| {
                KilnError::InvalidInput(format!("'{name}' is not a readable picture"))
            })?;
        if w == 0 || h == 0 {
            return Err(KilnError::InvalidInput(format!("'{name}' has no pixels")));
        }
        self.delete_selection()?;
        let at = self.head;
        // Scale to the text column, like Word.
        let max_w = self
            .doc
            .section_of(at.para)
            .map_or(9360, |s| s.props.content_width());
        let max_px = i64::from(max_w) * 96 / 1440;
        let (mut pw, mut ph) = (i64::from(w), i64::from(h));
        if pw > max_px {
            ph = ph * max_px / pw;
            pw = max_px;
        }
        let mut n = self.doc.media.len() + 1;
        let part = loop {
            let candidate = format!("word/media/image{n}.{ext}");
            if !self.doc.media.contains_key(&candidate) {
                break candidate;
            }
            n += 1;
        };
        let mut drawing = Drawing::picture(
            self.doc.alloc_id(),
            &part,
            pw.max(1) * EMU_PER_PX,
            ph.max(1) * EMU_PER_PX,
        );
        drawing.name = format!("Picture {n}");
        drawing.description = Some(name.rsplit(['/', '\\']).next().unwrap_or(name).to_owned());
        if floating {
            drawing.placement = Placement::Anchor(Box::new(Anchor {
                wrap: WrapType::Square,
                wrap_side: Some(WrapSide::BothSides),
                relative_h: RelativeFromH::Column,
                h: PosH::Align(AlignH::Right),
                relative_v: RelativeFromV::Paragraph,
                v: PosV::Offset(0),
                dist_top: 0,
                dist_bottom: 0,
                dist_left: 114_300,
                dist_right: 114_300,
                allow_overlap: true,
                layout_in_cell: true,
                locked: false,
                z_order: 251_659_264,
                wrap_polygon_xml: None,
            }));
        }
        let props = self.caret_props()?;
        let mut batch = self.batch(EditKind::Structure);
        self.batch_apply(
            &mut batch,
            Transaction::new(vec![
                Op::PutMedia {
                    name: part,
                    media: Some(Media {
                        content_type: content_type.to_owned(),
                        data: bytes.to_vec(),
                    }),
                },
                Op::InsertInline {
                    para: at.para,
                    offset: at.offset,
                    fragment: Fragment::object(InlineObject::Drawing(Box::new(drawing)), props),
                },
            ]),
        )?;
        self.batch_commit(
            batch,
            Range::caret(kiln_doc::Pos::new(at.para, at.offset + 1)),
        );
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn urls_and_bookmarks() {
        assert!(safe_url("https://kiln.dev") && safe_url("mailto:a@b.c") && safe_url("#intro"));
        assert!(!safe_url("javascript:alert(1)") && !safe_url("data:text/html,x"));
        assert!(valid_bookmark("Intro_2") && !valid_bookmark("2nd") && !valid_bookmark("a b"));
        assert_eq!(
            media_type("x.png", b"\x89PNG\r\n"),
            Some(("png", "image/png"))
        );
        assert_eq!(media_type("x.svg", b"<svg"), None);
    }
}
