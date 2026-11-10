// SPDX-License-Identifier: AGPL-3.0-or-later
//! The encoding of a plain text file (Word's File Conversion dialog): what the detection found,
//! the file read in each other encoding, reading it again in one, and the encoding a save writes.

use kiln_common::{KilnError, Result};
use kiln_doc::{Op, Transaction};
use kiln_protocol::commands::write_office::{TextEncodingChoice, TextEncodingInfo};

use super::super::textio::{self, Encoding};
use super::super::{Pos, WriteDoc};

/// How much of the file each choice previews.
const PREVIEW_CHARS: usize = 400;

impl WriteDoc {
    /// The File Conversion data (`None`: the document is not a text file).
    pub(super) fn text_encoding_info(&self) -> Option<TextEncodingInfo> {
        let format = self.text_format?;
        let unrepresentable = textio::unrepresentable(&self.doc, format.encoding, 20)
            .into_iter()
            .map(String::from)
            .collect();
        let Some(source) = &self.text_source else {
            return Some(TextEncodingInfo {
                encoding: format.encoding.name().to_owned(),
                confident: true,
                choices: Vec::new(),
                unrepresentable,
            });
        };
        let d = &source.detection;
        let head = &source.bytes[..source.bytes.len().min(PREVIEW_CHARS * 4)];
        let choices = d
            .ranked
            .iter()
            .map(|&e| TextEncodingChoice {
                name: e.name().to_owned(),
                title: e.title().to_owned(),
                preview: textio::decode_as(head, e)
                    .chars()
                    .take(PREVIEW_CHARS)
                    .collect(),
            })
            .collect();
        Some(TextEncodingInfo {
            encoding: format.encoding.name().to_owned(),
            confident: d.confident,
            choices,
            unrepresentable,
        })
    }

    /// Sets the encoding of a text file: `reload` reads the opened file again in it (one undo
    /// step); otherwise the next save writes it.
    pub(super) fn set_text_encoding(&mut self, name: &str, reload: bool) -> Result<()> {
        let encoding = Encoding::from_name(name)
            .ok_or_else(|| KilnError::InvalidInput(format!("unknown text encoding {name:?}")))?;
        let mut format = self.text_format.unwrap_or_default();
        // A byte order mark stays when the family stays.
        let encoding = match (format.encoding, encoding) {
            (Encoding::Utf8 { bom }, Encoding::Utf8 { .. }) => Encoding::Utf8 { bom },
            (Encoding::Utf16Le { bom }, Encoding::Utf16Le { .. }) => Encoding::Utf16Le { bom },
            (Encoding::Utf16Be { bom }, Encoding::Utf16Be { .. }) => Encoding::Utf16Be { bom },
            (_, e) => e,
        };
        if !reload {
            format.encoding = encoding;
            self.text_format = Some(format);
            return Ok(());
        }
        let Some(source) = &self.text_source else {
            return Err(KilnError::Unavailable {
                command: "write.file.text_encoding".into(),
                reason: "the document did not open from a text file".into(),
            });
        };
        let text = textio::decode_as(&source.bytes, encoding);
        let section = self
            .doc
            .sections
            .first()
            .ok_or_else(|| KilnError::Internal("a document without a section".into()))?;
        let (container, count) = (section.id, section.blocks.len());
        let doc = &mut self.doc;
        let blocks = textio::paragraphs(&text, || doc.alloc_id());
        let first = blocks.first().map(kiln_doc::Block::id);
        // The new lines first: a container keeps at least one block.
        let added = blocks.len();
        let mut ops = vec![Op::InsertBlocks {
            container,
            index: 0,
            blocks,
        }];
        if count > 0 {
            ops.push(Op::RemoveBlocks {
                container,
                index: added,
                count,
            });
        }
        let caret = first.map(|id| kiln_doc::Range::caret(Pos::new(id, 0)));
        self.commit(
            Transaction::new(ops)
                .with_kind(kiln_doc::EditKind::Structure)
                .with_selection(None, caret),
        )?;
        format.encoding = encoding;
        self.text_format = Some(format);
        if let Some(s) = &mut self.text_source {
            s.detection.encoding = encoding;
            s.detection.confident = true;
        }
        Ok(())
    }
}
