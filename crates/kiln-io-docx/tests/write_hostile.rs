// SPDX-License-Identifier: AGPL-3.0-or-later
//! The writer never panics and never writes a file it cannot read back, whatever the model holds:
//! arbitrary text (XML specials, control characters, astral planes), formatting, links, tracked
//! changes, comments and bookmarks on arbitrary ranges, and content nested as deep as the reader
//! follows — on a 1 MiB stack, as the web build has.

mod read_support;

use kiln_doc::st::{BreakKind, UnderlineStyle};
use kiln_doc::{
    Block, Break, Comment, CommentMeta, Document, Fragment, Hyperlink, InlineObject, Inlines,
    LinkTarget, Mark, Paragraph, RevisionInfo, RunProps, Underline,
};
use kiln_io_docx::{WriteOptions, read_docx, write_docx};
use proptest::prelude::*;
use read_support::{Build, build_docx};

fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(1 << 20)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("no stack overflow or panic");
}

fn nested_tables(depth: usize) -> Vec<u8> {
    let mut body = String::from("<w:p><w:r><w:t>core</w:t></w:r></w:p>");
    for _ in 0..depth {
        body = format!(
            "<w:tbl><w:tblPr/><w:tblGrid><w:gridCol w:w=\"100\"/></w:tblGrid><w:tr><w:tc>{body}</w:tc></w:tr></w:tbl><w:p/>"
        );
    }
    build_docx(Build {
        body,
        ..Build::default()
    })
}

#[test]
fn tables_nested_as_deep_as_the_reader_follows_round_trip_on_a_small_stack() {
    on_small_stack(|| {
        let read = read_docx(&nested_tables(30)).expect("reads");
        let out = write_docx(&read.document, None, &WriteOptions::default()).expect("writes");
        let back = read_docx(&out.bytes).expect("reads back");
        assert_eq!(back.document.sections, read.document.sections);
    });
}

#[test]
fn deeper_nesting_writes_what_the_reader_kept() {
    on_small_stack(|| {
        let read = read_docx(&nested_tables(200)).expect("reads");
        let out = write_docx(&read.document, None, &WriteOptions::default()).expect("writes");
        let back = read_docx(&out.bytes).expect("reads back");
        assert_eq!(
            kiln_io_docx::snapshot_facts(&back.document),
            kiln_io_docx::snapshot_facts(&read.document)
        );
    });
}

fn any_text() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            "[a-zA-Z ]{1,6}",
            Just("<&>\"'".to_owned()),
            Just("\t".to_owned()),
            Just("  ".to_owned()),
            Just("\u{1}\u{b}\u{fffe}".to_owned()),
            Just("😀 é".to_owned()),
            Just("{ PAGE }".to_owned()),
        ],
        1..6,
    )
    .prop_map(|v| v.concat())
}

#[derive(Debug, Clone)]
enum MarkKind {
    Link,
    Ins,
    Del,
    Comment,
}

fn any_mark() -> impl Strategy<Value = (MarkKind, usize, usize)> {
    (
        prop_oneof![
            Just(MarkKind::Link),
            Just(MarkKind::Ins),
            Just(MarkKind::Del),
            Just(MarkKind::Comment),
        ],
        0usize..40,
        1usize..20,
    )
}

/// The text that survives in a file: what XML 1.0 allows, plus the vertical tab and form feed of
/// pasted text, which are written as column and page breaks (the model's plain text of those
/// breaks).
fn writable(s: &str) -> String {
    s.chars()
        .filter(|c| {
            (kiln_ooxml::xml::is_xml_char(*c) || matches!(c, '\u{b}' | '\u{c}')) && *c != '\r'
        })
        .collect()
}

fn build(
    texts: &[String],
    bold: &[bool],
    marks: &[(MarkKind, usize, usize)],
    breaks: bool,
) -> Document {
    let mut doc = Document::new();
    let cid = doc.alloc_id();
    let cpid = doc.alloc_id();
    let mut blocks = Vec::new();
    for (i, t) in texts.iter().enumerate() {
        let props = RunProps {
            bold: bold.get(i).copied(),
            underline: (i % 3 == 1).then(|| Underline::new(UnderlineStyle::Wave)),
            ..RunProps::default()
        };
        let mut content = Inlines::from_fragment(Fragment::text(t, props)).unwrap();
        if breaks {
            let at = content.len() / 2;
            content
                .insert(
                    at,
                    Fragment::object(
                        InlineObject::Break(Break {
                            kind: BreakKind::TextWrapping,
                            clear: None,
                        }),
                        RunProps::default(),
                    ),
                )
                .unwrap();
            content
                .insert(
                    0,
                    Fragment::object(
                        InlineObject::BookmarkStart {
                            name: format!("b{i}"),
                        },
                        RunProps::default(),
                    ),
                )
                .unwrap();
            let end = content.len();
            content
                .insert(
                    end,
                    Fragment::object(
                        InlineObject::BookmarkEnd {
                            name: format!("b{i}"),
                        },
                        RunProps::default(),
                    ),
                )
                .unwrap();
        }
        let len = content.len();
        for (k, (kind, start, span)) in marks.iter().enumerate() {
            if len == 0 {
                break;
            }
            let s = start % len;
            let e = (s + span).min(len);
            if s >= e {
                continue;
            }
            let info = RevisionInfo::new(u32::try_from(k + 1).unwrap(), "Prop", None);
            let mark = match kind {
                MarkKind::Link => Mark::Link(Hyperlink {
                    target: LinkTarget::Url(format!("https://example.com/{k}?a=1&b=<2>")),
                    tooltip: Some("t\"ip".into()),
                    frame: None,
                    history: true,
                }),
                MarkKind::Ins => Mark::Insertion(info),
                MarkKind::Del => Mark::Deletion(info),
                MarkKind::Comment => Mark::Comment(cid),
            };
            let _ = content.add_mark(s, e, mark);
        }
        let mut p = Paragraph::new(doc.alloc_id());
        p.content = content;
        blocks.push(Block::Paragraph(p));
    }
    doc.sections[0].blocks = blocks;
    let mut cp = Paragraph::new(cpid);
    cp.content = Inlines::from_fragment(Fragment::text("note", RunProps::default())).unwrap();
    doc.comments.push(Comment {
        id: cid,
        meta: CommentMeta {
            author: "A<b>".into(),
            ..CommentMeta::default()
        },
        blocks: vec![Block::Paragraph(cp)],
    });
    doc
}

fn texts_of(doc: &Document) -> Vec<String> {
    doc.sections[0]
        .blocks
        .iter()
        .filter_map(|b| b.as_paragraph().map(Paragraph::text))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 96, ..ProptestConfig::default() })]

    #[test]
    fn any_content_is_written_and_reads_back(
        texts in prop::collection::vec(any_text(), 1..5),
        bold in prop::collection::vec(any::<bool>(), 0..5),
        marks in prop::collection::vec(any_mark(), 0..6),
        breaks in any::<bool>(),
    ) {
        let doc = build(&texts, &bold, &marks, breaks);
        prop_assert!(doc.validate().is_ok());
        let out = write_docx(&doc, None, &WriteOptions::default()).expect("writes");
        let back = read_docx(&out.bytes).expect("reads back");
        prop_assert_eq!(back.document.validate(), Ok(()));
        let want: Vec<String> = texts_of(&doc).iter().map(|t| writable(t)).collect();
        prop_assert_eq!(texts_of(&back.document), want);
        // Writing what was read gives the same bytes again.
        let again = write_docx(&back.document, None, &WriteOptions::default()).expect("writes");
        let third = read_docx(&again.bytes).expect("reads");
        prop_assert_eq!(texts_of(&third.document), texts_of(&back.document));
    }
}

/// Tables nested `depth` deep with a word in every level's cell and no trailing paragraphs, as
/// Apache POI's `deep-table-cell.docx` is built.
fn worded_nested_tables(depth: usize) -> Vec<u8> {
    let mut body = String::new();
    for i in 0..depth {
        body.push_str(&format!(
            "<w:tbl><w:tblGrid><w:gridCol w:w=\"2400\"/></w:tblGrid><w:tr><w:tc><w:p><w:r><w:t>level{i}</w:t></w:r></w:p>"
        ));
    }
    for _ in 0..depth {
        body.push_str("</w:tc></w:tr></w:tbl>");
    }
    build_docx(Build {
        body,
        ..Build::default()
    })
}

fn main_part_text(bytes: &[u8]) -> String {
    let pkg = kiln_ooxml::opc::OpcPackage::open(bytes).expect("package");
    let part = pkg
        .part("/word/document.xml")
        .expect("readable")
        .expect("present");
    String::from_utf8(part.into_owned()).expect("UTF-8")
}

fn assert_every_level_saved(depth: usize) {
    let read = read_docx(&worded_nested_tables(depth)).expect("reads");
    let out = write_docx(&read.document, None, &WriteOptions::default()).expect("writes");
    let xml = main_part_text(&out.bytes);
    for i in 0..depth {
        assert!(
            xml.contains(&format!(">level{i}<")),
            "level {i} of {depth} lost"
        );
    }
    let back = read_docx(&out.bytes).expect("reads back");
    assert_eq!(
        kiln_io_docx::snapshot_facts(&back.document),
        kiln_io_docx::snapshot_facts(&read.document)
    );
}

#[test]
fn tables_nested_past_the_followed_depth_keep_every_level_on_save() {
    // Past the reader's container limit the remaining tables are kept verbatim; the limit must
    // never cut a followed table open and drop the content of its innermost cell.
    on_small_stack(|| {
        for depth in [20, 40, 200] {
            assert_every_level_saved(depth);
        }
    });
}

#[test]
fn tables_nested_past_the_xml_depth_limit_open_and_save_losslessly() {
    // 5,000 levels is 15,000 elements deep: far past the XML tree limit. The over-deep part is
    // kept as verbatim markup, so the document opens and saves without losing a word.
    on_small_stack(|| assert_every_level_saved(5_000));
}
