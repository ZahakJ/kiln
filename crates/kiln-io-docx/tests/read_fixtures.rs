// SPDX-License-Identifier: AGPL-3.0-or-later
//! Structure assertions for each generated feature fixture (`fixtures/docx/features`), plus
//! corpus-wide guarantees: every `.docx` under `fixtures/docx` (and the hand-made OPC edge cases)
//! reads into a valid document, the synthetic corpus reads without diagnostics, and the
//! downloaded real-world files (when present) never crash the reader.

mod read_support;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kiln_doc::st::{
    Alignment, BorderStyle, HeaderFooterKind, HighlightColor, LineRule, NoteKind, NoteType,
    NumberFormat, Orientation, SectionBreak, StyleType, TabAlignment, TabLeader, ThemeColorName,
    ThemeFontRef, UnderlineStyle, VMerge, VertAlign, WrapType,
};
use kiln_doc::{
    Block, ColorValue, DocColor, Document, DrawingKind, Indent, InlineObject, LinkTarget, Mark,
    NodeId, Paragraph, Placement, PosH, PosV, Table,
};
use kiln_io_docx::{DocxError, DocxRead, read_docx};
use kiln_ooxml::drawingml::presets::preset_geometry;
use kiln_testing::fixtures::{FixtureFormat, FixtureKind, find, fixtures_dir, list_in};
use read_support::{all_paragraphs, at_text, objects, paragraph_with};

fn read(name: &str) -> DocxRead {
    let f = find(name).unwrap_or_else(|e| panic!("{e}"));
    let r = read_docx(&f.read().expect("fixture")).expect("reads");
    r.document.validate().expect("valid document");
    r
}

fn docx_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            docx_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "docx") {
            out.push(p);
        }
    }
}

fn tables(blocks: &[Block], out: &mut Vec<Table>) {
    for b in blocks {
        match b {
            Block::Table(t) => {
                out.push(t.clone());
                for row in &t.rows {
                    for cell in &row.cells {
                        tables(&cell.blocks, out);
                    }
                }
            }
            Block::Sdt(s) => tables(&s.blocks, out),
            Block::Field(f) => tables(&f.blocks, out),
            _ => {}
        }
    }
}

fn body_tables(doc: &Document) -> Vec<Table> {
    let mut out = Vec::new();
    for s in &doc.sections {
        tables(&s.blocks, &mut out);
    }
    out
}

fn cell_text(t: &Table, row: usize, cell: usize) -> String {
    t.rows[row].cells[cell]
        .blocks
        .iter()
        .filter_map(Block::as_paragraph)
        .map(Paragraph::text)
        .collect::<Vec<_>>()
        .join("\n")
}

// ------------------------------------------------------------------ corpus

#[test]
fn every_docx_fixture_reads_into_a_valid_document() {
    let mut files = Vec::new();
    docx_files(&fixtures_dir().join("docx"), &mut files);
    docx_files(&fixtures_dir().join("large"), &mut files);
    assert!(files.len() >= 30, "found {} files", files.len());
    for path in &files {
        let bytes = std::fs::read(path).expect("read");
        let r = read_docx(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        r.document
            .validate()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(!r.document.sections.is_empty());
    }
}

#[test]
fn feature_fixtures_read_without_diagnostics() {
    let features = list_in(&fixtures_dir(), FixtureFormat::Docx, FixtureKind::Features);
    assert!(features.len() >= 15);
    for f in &features {
        let r = read_docx(&f.read().expect("fixture")).expect("reads");
        assert!(
            r.warnings.is_empty(),
            "{}: {:#?}",
            f.rel,
            r.warnings
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
    }
}

#[test]
fn hand_made_package_edge_cases_read_or_fail_with_a_typed_error() {
    let mut files = Vec::new();
    docx_files(&fixtures_dir().join("ooxml"), &mut files);
    assert!(!files.is_empty());
    for path in &files {
        let bytes = std::fs::read(path).expect("read");
        match read_docx(&bytes) {
            Ok(r) => r
                .document
                .validate()
                .unwrap_or_else(|e| panic!("{}: {e}", path.display())),
            Err(DocxError::Package(_) | DocxError::NotWord(_)) => {}
        }
    }
}

#[test]
fn external_corpus_never_crashes_the_reader() {
    let external = list_in(&fixtures_dir(), FixtureFormat::Docx, FixtureKind::External);
    if external.is_empty() {
        eprintln!("skipped: no external corpus (bash tools/fetch-corpus.sh)");
        return;
    }
    for f in &external {
        let bytes = f.read().expect("fixture");
        let r = match read_docx(&bytes) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}: {e}", f.rel);
                continue;
            }
        };
        r.document
            .validate()
            .unwrap_or_else(|e| panic!("{}: {e}", f.rel));
        for w in &r.warnings {
            assert!(w.part.starts_with('/') && w.path.starts_with('/'), "{w}");
        }
    }
}

// ------------------------------------------------------------------ numbering.docx

fn labels(r: &DocxRead) -> HashMap<String, String> {
    let labels = r.document.list_labels();
    r.document
        .body_paragraphs()
        .into_iter()
        .filter_map(|p| labels.get(&p.id).map(|l| (p.text(), l.text.clone())))
        .collect()
}

#[test]
fn numbering_reads_abstract_definitions_and_instances() {
    let r = read("numbering.docx");
    let n = &r.document.numbering;
    assert!(n.instances.len() >= 6);
    let restart = &n.instances[&60];
    assert_eq!(restart.overrides.len(), 1);
    assert_eq!(restart.overrides[0].level, 0);
    assert_eq!(restart.overrides[0].start_override, Some(1));
    let abs = &n.abstracts[&40];
    assert_eq!(
        abs.multi_level_type,
        Some(kiln_doc::st::MultiLevelType::HybridMultilevel)
    );
    let l0 = &abs.levels[0];
    assert_eq!(
        l0.ppr.as_ref().and_then(|p| p.indent),
        Some(Indent {
            left: Some(720),
            hanging: Some(360),
            ..Indent::default()
        })
    );
    assert_eq!(
        l0.rpr
            .as_ref()
            .and_then(|r| r.fonts.as_ref())
            .and_then(|f| f.ascii.as_deref()),
        Some("Symbol")
    );
    assert_eq!(n.abstracts[&44].levels[2].text, "%1.%2.%3");
}

#[test]
fn numbering_labels_lists_as_word_does() {
    let l = labels(&read("numbering.docx"));
    for (text, label) in [
        ("decimal item 1", "1."),
        ("decimal item 2", "2."),
        ("decimal item 3", "3."),
        ("decimal item 2.1", "1."),
        ("lowerLetter item 1", "a)"),
        ("lowerLetter item 3", "c)"),
        ("upperRoman item 2", "II."),
        ("upperRoman item 3", "III."),
        ("legal item 2.1", "2.1"),
        ("legal level 3", "3.1.1"),
        ("bullet item 1", "•"),
        ("bullet item 1.1", "o"),
        ("bullet level 3", "▪"),
        ("Continues the decimal list", "4."),
        ("Restarts at one", "1."),
        ("Then two", "2."),
    ] {
        assert_eq!(l.get(text).map(String::as_str), Some(label), "{text}");
    }
}

// ------------------------------------------------------------------ tables.docx

#[test]
fn tables_read_grid_style_and_text() {
    let r = read("tables.docx");
    let t = &body_tables(&r.document)[0];
    assert_eq!(t.props.style_id.as_deref(), Some("TableGrid"));
    assert_eq!((t.rows.len(), t.grid_width()), (4, 4));
    assert_eq!(cell_text(t, 0, 0), "Header 1");
    assert_eq!(cell_text(t, 3, 3), "R4C4");
}

#[test]
fn tables_keep_grid_spans_and_vertical_merges() {
    let r = read("tables.docx");
    let t = &body_tables(&r.document)[1];
    assert_eq!((t.rows.len(), t.grid_width()), (5, 4));
    let merged = &t.rows[1].cells[0];
    assert_eq!(merged.grid_span, 3);
    assert_eq!(cell_text(t, 1, 0), "Merged across three columns");
    let map = t.cell_map();
    let down = map
        .iter()
        .find(|s| {
            t.rows[s.row].cells[s.cell]
                .blocks
                .iter()
                .filter_map(Block::as_paragraph)
                .any(|p| p.text() == "Merged down three rows")
        })
        .expect("merged cell");
    assert_eq!(down.row_span, 3);
    assert_eq!(
        t.rows[down.row].cells[down.cell].v_merge,
        Some(VMerge::Restart)
    );
    // Every row covers the whole grid.
    for (i, row) in t.rows.iter().enumerate() {
        let width: u32 = row.props.grid_before.unwrap_or(0)
            + row.props.grid_after.unwrap_or(0)
            + row.cells.iter().map(|c| c.grid_span).sum::<u32>();
        assert_eq!(width, 4, "row {i}");
    }
    assert_eq!(
        t.props.alignment,
        Some(kiln_doc::st::TableAlignment::Center)
    );
    let look = t.props.look.expect("look");
    assert!(look.first_row && look.no_v_band);
}

#[test]
fn tables_read_shading_widths_and_nested_tables() {
    let r = read("tables.docx");
    let all = body_tables(&r.document);
    let t = &all[2];
    let first = &t.rows[0].cells[0];
    assert_eq!(
        first.props.shading.as_ref().and_then(|s| s.fill.clone()),
        Some(DocColor::rgb(0xFFF2CC))
    );
    assert_eq!(first.props.width, Some(kiln_doc::TableWidth::dxa(1701)));
    assert_eq!(t.grid.len(), 3);
    // The nested table is the fourth table found (depth first).
    assert_eq!(all.len(), 4);
    assert_eq!(all[3].grid_width(), 2);
}

// ------------------------------------------------------------------ sections.docx

#[test]
fn sections_read_break_types_sizes_and_orientation() {
    let r = read("sections.docx");
    let s: Vec<_> = r.document.sections.iter().map(|s| &s.props).collect();
    assert_eq!(s.len(), 4);
    assert_eq!(
        s.iter().map(|p| p.break_type).collect::<Vec<_>>(),
        [
            SectionBreak::NextPage,
            SectionBreak::NextPage,
            SectionBreak::Continuous,
            SectionBreak::OddPage
        ]
    );
    assert_eq!(
        (
            s[0].page_size.width,
            s[0].page_size.height,
            s[0].page_size.orientation
        ),
        (11906, 16838, Orientation::Portrait)
    );
    assert_eq!(s[1].page_size.orientation, Orientation::Landscape);
    assert_eq!(s[1].margins.left, 850);
    assert_eq!(
        (
            s[2].columns.count,
            s[2].columns.space,
            s[2].columns.separator
        ),
        (2, 720, true)
    );
    assert_eq!(s[3].margins.gutter, 567);
}

#[test]
fn sections_map_header_references_by_kind() {
    let r = read("sections.docx");
    let doc = &r.document;
    let hf_text = |id: Option<NodeId>| -> String {
        let id = id.expect("reference");
        let h = doc
            .headers_footers
            .iter()
            .find(|h| h.id == id)
            .expect("story");
        h.blocks
            .iter()
            .filter_map(Block::as_paragraph)
            .map(Paragraph::text)
            .collect()
    };
    let first = &doc.sections[0];
    assert_eq!(hf_text(first.headers.default), "Default header (section 1)");
    assert_eq!(hf_text(first.headers.first), "First-page header");
    assert_eq!(hf_text(first.headers.even), "Even-page header");
    assert!(first.props.title_page);
    assert!(doc.settings.even_and_odd_headers);
    assert_eq!(
        hf_text(doc.sections[1].headers.get(HeaderFooterKind::Default)),
        "Landscape section header"
    );
}

// ------------------------------------------------------------------ headers-footers.docx

#[test]
fn headers_and_footers_read_tabs_and_page_fields() {
    let r = read("headers-footers.docx");
    let doc = &r.document;
    let s = &doc.sections[0];
    let header = doc
        .headers_footers
        .iter()
        .find(|h| Some(h.id) == s.headers.default)
        .expect("header");
    assert_eq!(
        header.blocks[0]
            .as_paragraph()
            .map(Paragraph::text)
            .as_deref(),
        Some("Officina fixtures\tHeaders and footers\tConfidential")
    );
    let footer = doc
        .headers_footers
        .iter()
        .find(|h| Some(h.id) == s.footers.default)
        .expect("footer");
    let p = footer.blocks[0].as_paragraph().expect("paragraph");
    let fields: Vec<(String, String)> = p
        .content
        .objects()
        .iter()
        .filter_map(|o| match o {
            InlineObject::Field(f) => Some((f.instr.clone(), f.result.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        fields,
        [
            ("PAGE".to_owned(), "1".to_owned()),
            ("NUMPAGES".to_owned(), "3".to_owned())
        ]
    );
    assert_eq!(p.props.alignment, Some(Alignment::Center));
}

// ------------------------------------------------------------------ images

#[test]
fn inline_images_carry_media_sizes_and_content_types() {
    let r = read("images-inline.docx");
    let pics: Vec<_> = objects(&r.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::Drawing(d) => Some(d),
            _ => None,
        })
        .collect();
    assert_eq!(pics.len(), 4);
    assert_eq!(pics[0].placement, Placement::Inline);
    assert_eq!(pics[0].width, 2_880_000);
    let mut types = Vec::new();
    for d in &pics {
        let DrawingKind::Picture { media, .. } = &d.kind else {
            panic!("a picture")
        };
        let m = r.document.media.get(media).expect("media");
        types.push(m.content_type.clone());
        assert!(d.height > 0);
        assert!(!r.passthrough.has(media), "media is modelled");
    }
    assert!(types.iter().any(|t| t == "image/png"));
    assert!(types.iter().any(|t| t == "image/jpeg"));
    let DrawingKind::Picture { media, .. } = &pics[0].kind else {
        panic!()
    };
    assert_eq!(
        &r.document.media[media].data[..4],
        &[0x89, 0x50, 0x4e, 0x47]
    );
}

#[test]
fn anchored_images_read_every_wrap_mode_and_position() {
    let r = read("images-anchored.docx");
    let anchors: Vec<_> = objects(&r.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::Drawing(d) => Some(d),
            _ => None,
        })
        .collect();
    let wraps: Vec<WrapType> = anchors
        .iter()
        .map(|d| match &d.placement {
            Placement::Anchor(a) => a.wrap,
            Placement::Inline => WrapType::Inline,
        })
        .collect();
    assert_eq!(
        wraps,
        [
            WrapType::Square,
            WrapType::Tight,
            WrapType::Through,
            WrapType::TopAndBottom,
            WrapType::Behind,
            WrapType::InFront
        ]
    );
    let Placement::Anchor(a) = &anchors[1].placement else {
        panic!("anchored")
    };
    assert_eq!(a.relative_h, kiln_doc::st::RelativeFromH::Column);
    assert_eq!(a.relative_v, kiln_doc::st::RelativeFromV::Paragraph);
    assert_eq!(a.h, PosH::Offset(914_400));
    assert_eq!(a.v, PosV::Offset(0));
    assert_eq!(a.dist_left, 114_300);
    assert!(a.allow_overlap && a.layout_in_cell);
    assert_eq!(a.z_order, 251_659_264 + 101);
    assert_eq!(anchors[1].width, 1_828_800);
}

// ------------------------------------------------------------------ text-boxes.docx

#[test]
fn text_boxes_read_shapes_with_their_text() {
    let r = read("text-boxes.docx");
    let shapes: Vec<_> = objects(&r.document)
        .into_iter()
        .filter_map(|o| match o {
            InlineObject::Drawing(d) => match &d.kind {
                DrawingKind::Shape { preset } => Some((preset.clone(), d)),
                _ => None,
            },
            _ => None,
        })
        .collect();
    assert_eq!(
        shapes.iter().map(|(p, _)| p.as_str()).collect::<Vec<_>>(),
        ["rect", "roundRect", "ellipse"]
    );
    let d = shapes[0].1;
    assert_eq!((d.width, d.height), (2_286_000, 914_400));
    let text: String = d
        .text_box
        .as_ref()
        .expect("text box")
        .iter()
        .filter_map(Block::as_paragraph)
        .map(Paragraph::text)
        .collect();
    assert!(text.contains("Text box 1: rect"), "{text}");
    let xml = d.xml.as_deref().expect("kept XML");
    assert!(xml.contains("DEEBF7") && xml.contains("12700"));
    let el = kiln_io_docx::common::xml::parse_fragment(xml).expect("fragment");
    assert!(el.name == "w:drawing" || el.name == "mc:AlternateContent");
    // The presets the shapes use are ones the geometry engine knows.
    for (p, _) in &shapes {
        assert!(preset_geometry(p, 100.0, 100.0, &[]).is_ok(), "{p}");
    }
}

// ------------------------------------------------------------------ footnotes.docx

#[test]
fn footnotes_and_endnotes_with_references() {
    let r = read("footnotes.docx");
    let doc = &r.document;
    let ids = |kind: NoteKind| -> Vec<String> {
        doc.notes
            .iter()
            .filter(|n| n.kind == kind && n.note_type == NoteType::Normal)
            .map(|n| r.extras.note_ids[&n.id].clone())
            .collect()
    };
    assert_eq!(ids(NoteKind::Footnote), ["1", "2", "3"]);
    assert_eq!(ids(NoteKind::Endnote), ["1", "2"]);
    let first = doc
        .notes
        .iter()
        .find(|n| n.kind == NoteKind::Footnote && n.note_type == NoteType::Normal)
        .expect("note");
    let text: String = first
        .blocks
        .iter()
        .filter_map(Block::as_paragraph)
        .map(|p| p.content.plain_text(false))
        .collect();
    assert!(text.starts_with(" Footnote 1: "), "{text:?}");
    // The note's own reference mark points back at it.
    let own = first
        .blocks
        .iter()
        .filter_map(Block::as_paragraph)
        .flat_map(|p| p.content.objects())
        .find_map(|o| match o {
            InlineObject::NoteRef(n) => Some(n.note),
            _ => None,
        });
    assert_eq!(own, Some(first.id));
    let refs: Vec<String> = doc
        .body_paragraphs()
        .into_iter()
        .flat_map(|p| p.content.objects())
        .filter_map(|o| match o {
            InlineObject::NoteRef(n) => Some(format!("{}:{}", n.kind, r.extras.note_ids[&n.note])),
            _ => None,
        })
        .collect();
    assert_eq!(
        refs,
        [
            "footnote:1",
            "footnote:2",
            "footnote:3",
            "endnote:1",
            "endnote:2"
        ]
    );
    // Separator notes are kept (for the writer) but marked as such.
    assert!(doc.notes.iter().any(|n| n.note_type == NoteType::Separator));
}

// ------------------------------------------------------------------ fields.docx

#[test]
fn toc_fields_become_field_blocks() {
    let r = read("fields.docx");
    let fb: Vec<_> = r.document.sections[0]
        .blocks
        .iter()
        .filter_map(|b| match b {
            Block::Field(f) => Some(f),
            Block::Sdt(s) => s.blocks.iter().find_map(|b| match b {
                Block::Field(f) => Some(f),
                _ => None,
            }),
            _ => None,
        })
        .collect();
    assert_eq!(fb.len(), 1);
    assert_eq!(fb[0].field.instr, "TOC \\o \"1-3\" \\h \\z \\u");
    assert_eq!(fb[0].blocks.len(), 3);
    assert_eq!(
        fb[0].blocks[0]
            .as_paragraph()
            .map(Paragraph::text)
            .as_deref(),
        Some("Chapter One\t2")
    );
}

#[test]
fn complex_and_simple_fields_keep_instructions_and_cached_results() {
    let r = read("fields.docx");
    let fields: Vec<(String, String)> = r
        .document
        .body_paragraphs()
        .into_iter()
        .filter(|p| r.document.parent_of(p.id).is_some())
        .flat_map(|p| p.content.objects())
        .filter_map(|o| match o {
            InlineObject::Field(f) if !f.instr.starts_with("PAGEREF") => {
                Some((f.instr.clone(), f.result.clone()))
            }
            _ => None,
        })
        .collect();
    let expect = [
        ("PAGE", "1"),
        ("NUMPAGES", "1"),
        ("DATE \\@ \"yyyy-MM-dd\"", "2026-01-15"),
        ("REF chapter_one \\h", "Chapter One"),
        ("AUTHOR", "Officina Fixtures"),
    ];
    assert_eq!(
        fields,
        expect
            .iter()
            .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
            .collect::<Vec<_>>()
    );
}

#[test]
fn bookmarks_surround_the_referenced_heading() {
    let r = read("fields.docx");
    let heading = r
        .document
        .body_paragraphs()
        .into_iter()
        .find(|p| p.props.style_id.as_deref() == Some("Heading1") && p.text() == "Chapter One")
        .expect("heading");
    let objs = heading.content.objects();
    assert!(matches!(&objs[0], InlineObject::BookmarkStart { name } if name == "chapter_one"));
    assert!(
        matches!(objs.last(), Some(InlineObject::BookmarkEnd { name }) if name == "chapter_one")
    );
    assert!(heading.content.raw_text().starts_with('\u{FFFC}'));
    assert_eq!(
        r.document.bookmark_text("chapter_one").as_deref(),
        Some("Chapter One")
    );
}

// ------------------------------------------------------------------ comments.docx

#[test]
fn comments_carry_authors_dates_and_initials_and_mark_their_ranges() {
    let r = read("comments.docx");
    let doc = &r.document;
    assert_eq!(doc.comments.len(), 3);
    let c0 = &doc.comments[0];
    assert_eq!(r.extras.comment_ids[&c0.id], "0");
    assert_eq!(c0.meta.author, "Reviewer 1");
    assert_eq!(c0.meta.initials.as_deref(), Some("R1"));
    assert_eq!(c0.meta.date.as_deref(), Some("2026-01-11T10:00:00Z"));
    let c1_text: String = doc.comments[1]
        .blocks
        .iter()
        .filter_map(Block::as_paragraph)
        .map(Paragraph::text)
        .collect();
    assert!(c1_text.starts_with("Comment number 2: "), "{c1_text}");
    let (_, marks) = at_text(doc, "commented text 2");
    assert!(marks.contains(&&Mark::Comment(doc.comments[1].id)));
}

// ------------------------------------------------------------------ tracked-changes.docx

#[test]
fn insertions_and_deletions_are_revision_marks() {
    let r = read("tracked-changes.docx");
    let doc = &r.document;
    let info = |id| kiln_doc::RevisionInfo::new(id, "Editor", Some("2026-01-15T09:30:00Z"));
    let (_, m) = at_text(doc, "Inserted words.");
    assert!(m.contains(&&Mark::Insertion(info(1))), "{m:?}");
    let (_, m) = at_text(doc, "Deleted words.");
    assert!(m.contains(&&Mark::Deletion(info(2))), "{m:?}");
    let (_, m) = at_text(doc, "Unchanged end.");
    assert!(!m.iter().any(|m| m.revision().is_some()));
}

#[test]
fn format_and_paragraph_changes_are_recorded() {
    let r = read("tracked-changes.docx");
    let doc = &r.document;
    let (props, marks) = at_text(doc, "made bold");
    assert_eq!(props.bold, Some(true));
    assert!(marks.iter().any(
        |m| matches!(m, Mark::FormatChange { info, .. } if info.id == 3 && info.author == "Editor")
    ));
    let p = paragraph_with(doc, "Paragraph property change");
    assert_eq!(p.props.alignment, Some(Alignment::Center));
    let change = p.revision.props_change.as_ref().expect("pPrChange");
    assert_eq!((change.info.id, change.info.author.as_str()), (4, "Editor"));
    assert!(doc.settings.track_revisions);
    assert!(doc.revisions().len() >= 4);
}

// ------------------------------------------------------------------ rtl.docx

#[test]
fn rtl_paragraphs_and_runs() {
    let r = read("rtl.docx");
    let doc = &r.document;
    let p = paragraph_with(doc, "هذه فقرة");
    assert_eq!(p.props.bidi, Some(true));
    assert_eq!(p.props.alignment, Some(Alignment::Right));
    let (props, _) = at_text(doc, "זוהי");
    assert_eq!(props.rtl, Some(true));
    assert_eq!(
        props.fonts.as_ref().and_then(|f| f.cs.as_deref()),
        Some("Arial")
    );
    assert_eq!(paragraph_with(doc, "left-to-right").props.bidi, None);
}

// ------------------------------------------------------------------ hyperlinks.docx

#[test]
fn hyperlinks_resolve_relationships_and_anchors() {
    let r = read("hyperlinks.docx");
    let doc = &r.document;
    let link = |needle: &str| -> LinkTarget {
        at_text(doc, needle)
            .1
            .into_iter()
            .find_map(|m| match m {
                Mark::Link(l) => Some(l.target.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no link on {needle}"))
    };
    assert_eq!(
        link("https://example.org/"),
        LinkTarget::Url("https://example.org/".into())
    );
    assert_eq!(
        link("someone@example.org"),
        LinkTarget::Url("mailto:someone@example.org".into())
    );
    assert_eq!(
        link("jump to target"),
        LinkTarget::Bookmark("target".into())
    );
    assert!(r.document.bookmarks().iter().any(|b| b.name == "target"));
    // Modelled hyperlink relationships are not passthrough.
    assert!(
        !r.passthrough
            .relationships()
            .iter()
            .any(|p| p.rel.rel_type.ends_with("/hyperlink"))
    );
}

// ------------------------------------------------------------------ page-setup.docx

#[test]
fn page_setup_reads_size_margins_numbering_and_borders() {
    let r = read("page-setup.docx");
    let s = &r.document.sections[0];
    assert_eq!(
        (s.props.page_size.width, s.props.page_size.height),
        (12240, 20160)
    );
    let m = s.props.margins;
    assert_eq!(
        (m.left, m.right, m.header, m.footer),
        (1803, 1803, 709, 709)
    );
    let ln = s.props.line_numbering.expect("line numbering");
    assert_eq!(ln.count_by, 5);
    assert_eq!(ln.restart, kiln_doc::st::LineNumberRestart::NewPage);
    let pn = s.props.page_numbering.as_ref().expect("page numbering");
    assert_eq!(pn.start, Some(3));
    assert_eq!(pn.format, Some(NumberFormat::LowerRoman));
    let pb = r.extras.sections[&s.id]
        .page_borders
        .as_ref()
        .expect("page borders");
    assert_eq!(pb.offset_from.as_deref(), Some("page"));
    let top = pb.top.as_ref().expect("top");
    assert_eq!(top.style, BorderStyle::Single);
    assert_eq!((top.size, top.space), (Some(12), Some(24)));
    assert_eq!(
        top.color.as_ref().map(|c| c.val),
        Some(ColorValue::hex(0x4472C4))
    );
}

// ------------------------------------------------------------------ breaks.docx

#[test]
fn breaks_tabs_and_special_characters() {
    let r = read("breaks.docx");
    let doc = &r.document;
    let breaks = |p: &Paragraph, kind| {
        p.content
            .objects()
            .iter()
            .filter(|o| matches!(o, InlineObject::Break(b) if b.kind == kind))
            .count()
    };
    use kiln_doc::st::BreakKind;
    assert_eq!(
        breaks(paragraph_with(doc, "Line one"), BreakKind::TextWrapping),
        1
    );
    let pages: usize = doc
        .body_paragraphs()
        .into_iter()
        .map(|p| breaks(p, BreakKind::Page))
        .sum();
    assert_eq!(pages, 1);
    assert_eq!(
        paragraph_with(doc, "Tabs:")
            .content
            .raw_text()
            .matches('\t')
            .count(),
        3
    );
    assert!(
        paragraph_with(doc, "Special characters")
            .text()
            .contains("soft\u{00AD}hyphen")
    );
}

#[test]
fn symbol_characters_keep_font_and_code_and_map_to_unicode() {
    let r = read("breaks.docx");
    let p = paragraph_with(&r.document, "Symbol characters");
    let syms: Vec<_> = p
        .content
        .objects()
        .iter()
        .filter_map(|o| match o {
            InlineObject::Symbol(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(syms.len(), 2);
    assert_eq!((syms[0].font.as_str(), syms[0].code), ("Wingdings", 0xF04A));
    let text: String = syms
        .iter()
        .filter_map(|s| kiln_io_docx::symbol_code_to_unicode(&s.font, s.code))
        .collect();
    assert_eq!(text, "☺α");
}

// ------------------------------------------------------------------ styles.docx

#[test]
fn style_sheet_with_based_on_chains_and_doc_defaults() {
    let r = read("styles.docx");
    let styles = &r.document.styles;
    assert_eq!(
        styles.get("Heading1").and_then(|s| s.based_on.as_deref()),
        Some("Normal")
    );
    let chain: Vec<&str> = styles
        .chain(Some("Heading2"))
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert!(
        chain.contains(&"Normal") && chain.contains(&"Heading2"),
        "{chain:?}"
    );
    assert_eq!(
        styles
            .default_style(StyleType::Paragraph)
            .map(|s| s.id.as_str()),
        Some("Normal")
    );
    assert_eq!(
        styles
            .doc_defaults
            .rpr
            .fonts
            .as_ref()
            .and_then(|f| f.ascii_theme),
        Some(ThemeFontRef::MinorHAnsi)
    );
    assert!(styles.get("Title").is_some_and(|s| s.quick_style));
}

#[test]
fn character_formatting_is_read_into_run_properties() {
    let r = read("styles.docx");
    let d = &r.document;
    let props = |t: &str| at_text(d, t).0.clone();
    let p = props("bold italic");
    assert_eq!((p.bold, p.italic), (Some(true), Some(true)));
    assert_eq!(props("double strike").dstrike, Some(true));
    assert_eq!(props("small caps").small_caps, Some(true));
    assert_eq!(
        props("superscript").vert_align,
        Some(VertAlign::Superscript)
    );
    assert_eq!(
        props("underline double").underline.map(|u| u.style),
        Some(UnderlineStyle::Double)
    );
    assert_eq!(
        props("underline wavy").underline.map(|u| u.style),
        Some(UnderlineStyle::Wave)
    );
    assert_eq!(props("36pt").size, Some(72));
    assert_eq!(
        props("in Georgia").fonts.and_then(|f| f.ascii),
        Some("Georgia".to_owned())
    );
    assert_eq!(props("#C00000").color, Some(DocColor::rgb(0xC00000)));
    assert_eq!(
        props("theme accent_1").color.and_then(|c| c.theme_color),
        Some(ThemeColorName::Accent1)
    );
    assert_eq!(props("yellow").highlight, Some(HighlightColor::Yellow));
    assert_eq!(props("spacing 60").spacing, Some(60));
    assert_eq!(props("hidden").hidden, Some(true));
}

// ------------------------------------------------------------------ paragraphs.docx

#[test]
fn paragraph_alignment_indents_and_spacing() {
    let r = read("paragraphs.docx");
    let d = &r.document;
    let pp = |t: &str| paragraph_with(d, t).props.clone();
    assert_eq!(pp("[CENTER]").alignment, Some(Alignment::Center));
    assert_eq!(pp("[JUSTIFY]").alignment, Some(Alignment::Both));
    assert_eq!(
        pp("[left indent 2 cm]").indent,
        Some(Indent {
            left: Some(1134),
            ..Indent::default()
        })
    );
    assert_eq!(
        pp("[hanging 1.27 cm]").indent,
        Some(Indent {
            left: Some(720),
            hanging: Some(720),
            ..Indent::default()
        })
    );
    let sp = pp("[space before 18").spacing.expect("spacing");
    assert_eq!((sp.before, sp.after), (Some(360), Some(120)));
    let line = |t: &str| {
        let s = pp(t).spacing.expect("spacing");
        (s.line, s.line_rule)
    };
    assert_eq!(
        line("line spacing double"),
        (Some(480), Some(LineRule::Auto))
    );
    assert_eq!(
        line("line spacing exactly"),
        (Some(400), Some(LineRule::Exact))
    );
    assert_eq!(
        line("line spacing at least"),
        (Some(480), Some(LineRule::AtLeast))
    );
}

#[test]
fn paragraph_keep_options_borders_shading_tabs() {
    let r = read("paragraphs.docx");
    let d = &r.document;
    let pp = |t: &str| paragraph_with(d, t).props.clone();
    assert_eq!(pp("Keep with next").keep_next, Some(true));
    assert_eq!(pp("Widow control off").widow_control, Some(false));
    let boxed = pp("Boxed paragraph");
    let bottom = boxed.borders.and_then(|b| b.bottom).expect("bottom border");
    assert_eq!((bottom.style, bottom.size), (BorderStyle::Double, Some(6)));
    assert_eq!(
        boxed.shading.and_then(|s| s.fill),
        Some(DocColor::rgb(0xDEEAF6))
    );
    let tabs = pp("Left").tabs.expect("tabs");
    assert_eq!(
        tabs.iter().map(|t| t.align).collect::<Vec<_>>(),
        [
            TabAlignment::Center,
            TabAlignment::Right,
            TabAlignment::Decimal
        ]
    );
    assert_eq!(tabs[0].leader, Some(TabLeader::Dot));
    assert_eq!(pp("Page break before").page_break_before, Some(true));
}

// ------------------------------------------------------------------ properties.docx

#[test]
fn core_and_custom_document_properties() {
    let r = read("properties.docx");
    let p = &r.document.properties;
    assert_eq!(p.title.as_deref(), Some("Document properties"));
    assert_eq!(p.subject.as_deref(), Some("Fixture subject"));
    assert_eq!(p.creator.as_deref(), Some("Officina Fixtures"));
    assert_eq!(p.category.as_deref(), Some("Testing"));
    let names: Vec<&str> = p.custom.iter().map(|c| c.name.as_str()).collect();
    for n in ["Project", "Build", "Ratio", "Reviewed"] {
        assert!(names.contains(&n), "{n}");
    }
    assert_eq!(
        p.custom
            .iter()
            .find(|c| c.name == "Build")
            .map(|c| &c.value),
        Some(&kiln_doc::settings::PropertyValue::Integer(42))
    );
}

// ------------------------------------------------------------------ long-text.docx

#[test]
fn long_documents_keep_every_heading() {
    let r = read("long-text.docx");
    let count = |style: &str| {
        r.document
            .body_paragraphs()
            .into_iter()
            .filter(|p| p.props.style_id.as_deref() == Some(style))
            .count()
    };
    assert_eq!(count("Heading2"), 15);
    assert_eq!(count("Heading1"), 5);
    assert_eq!(r.document.outline().len(), 20);
}

// ------------------------------------------------------------------ passthrough

#[test]
fn unmodelled_parts_are_passthrough() {
    let r = read("styles.docx");
    let names: Vec<&str> = r.passthrough.parts().map(|p| p.name.as_str()).collect();
    for n in [
        "/customXml/item1.xml",
        "/word/fontTable.xml",
        "/word/webSettings.xml",
    ] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    for n in [
        "/word/document.xml",
        "/word/styles.xml",
        "/docProps/core.xml",
    ] {
        assert!(!names.contains(&n), "{n}");
    }
}

#[test]
fn every_paragraph_of_every_story_is_reachable() {
    for name in ["comments.docx", "footnotes.docx", "headers-footers.docx"] {
        let r = read(name);
        assert!(all_paragraphs(&r.document).len() > r.document.body_paragraphs().len());
    }
}
