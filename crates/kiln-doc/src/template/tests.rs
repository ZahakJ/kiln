// SPDX-License-Identifier: AGPL-3.0-or-later
//! Ported from `reference/ts-core/doc-model/test/template.test.ts`: the Word 2016–2021 defaults
//! of new documents. Module `styles::builtin` holds the built-in style checks the parity tracker
//! points at (`unit:kiln-doc/styles/builtin`).

use super::*;
use crate::resolve::{
    DocContext, ResolvedParagraph, ResolvedRun, resolve_paragraph, resolve_text_run,
};
use crate::section::Paper;
use crate::st::Orientation;

fn para(doc: &Document, style: &str) -> ResolvedParagraph {
    resolve_paragraph(
        &doc.context(),
        &ParagraphProps {
            style_id: Some(style.into()),
            ..ParagraphProps::default()
        },
        None,
    )
}

fn run(doc: &Document, para_style: Option<&str>, char_style: Option<&str>) -> ResolvedRun {
    resolve_text_run(
        &doc.context(),
        &ParagraphProps {
            style_id: para_style.map(str::to_owned),
            ..ParagraphProps::default()
        },
        &RunProps {
            style_id: char_style.map(str::to_owned),
            ..RunProps::default()
        },
        None,
    )
}

#[test]
fn creates_one_letter_section_with_one_empty_normal_paragraph() -> Result<(), crate::DocError> {
    let d = Document::new();
    assert_eq!(d.sections.len(), 1);
    let s = &d.sections[0];
    assert_eq!(
        (
            s.props.page_size.width,
            s.props.page_size.height,
            s.props.page_size.orientation
        ),
        (12240, 15840, Orientation::Portrait)
    );
    assert_eq!(
        (
            s.props.margins.top,
            s.props.margins.left,
            s.props.margins.header,
            s.props.margins.footer
        ),
        (1440, 1440, 720, 720)
    );
    assert_eq!(s.blocks.len(), 1);
    assert!(matches!(&s.blocks[0], Block::Paragraph(p) if p.content.is_empty()));
    d.validate()?;
    Ok(())
}

#[test]
fn uses_a4_outside_letter_regions() {
    let with = |locale: &str| {
        Document::with_options(&NewDocumentOptions {
            locale: Some(locale.into()),
            paper: None,
        })
    };
    assert_eq!(with("de-DE").sections[0].props.page_size.width, 11906);
    assert_eq!(with("en-GB").sections[0].props.page_size.height, 16838);
    assert_eq!(with("en-CA").sections[0].props.page_size.width, 12240);
    assert_eq!(
        with("de-DE")
            .styles
            .doc_defaults
            .rpr
            .lang
            .and_then(|l| l.val)
            .as_deref(),
        Some("de-DE")
    );
    let a5 = Document::with_options(&NewDocumentOptions {
        locale: None,
        paper: Some(Paper::A5),
    });
    assert_eq!(a5.sections[0].props.page_size.width, 8391);
    assert_eq!(Document::default(), Document::new());
}

#[test]
fn normal_is_calibri_11_with_1_08_lines_and_8_pt_after() {
    let d = Document::new();
    let p = resolve_paragraph(&d.context(), &ParagraphProps::default(), None);
    assert_eq!(
        (
            p.spacing.after,
            p.spacing.line,
            p.spacing.line_rule,
            p.spacing.before
        ),
        (160, 259, LineRule::Auto, 0)
    );
    assert!((f64::from(p.spacing.line) / 240.0 - 1.08).abs() < 0.005);
    let r = run(&d, None, None);
    assert_eq!(r.size, 22);
    assert_eq!(r.fonts.ascii, "Calibri");
    assert_eq!(r.color.to_string(), "auto");
}

#[test]
fn context_without_numbering_still_resolves() {
    let d = Document::new();
    let ctx = DocContext::new(&d.styles, &d.theme);
    assert_eq!(
        resolve_text_run(&ctx, &ParagraphProps::default(), &RunProps::default(), None).size,
        22
    );
}

mod styles {
    pub(super) mod builtin {
        use super::super::*;

        #[test]
        fn headings_use_the_major_font_in_accent_1_shades() {
            let d = Document::new();
            for (id, size, color, before, outline) in [
                ("Heading1", 32, "2F5496", 240, 0),
                ("Heading2", 26, "2F5496", 40, 1),
                ("Heading3", 24, "1F3763", 40, 2),
            ] {
                let p = para(&d, id);
                let r = run(&d, Some(id), None);
                assert_eq!(r.size, size, "{id}");
                assert_eq!(r.color.to_string(), color, "{id}");
                assert_eq!(r.fonts.ascii, "Calibri Light", "{id}");
                assert_eq!((p.spacing.before, p.spacing.after), (before, 0), "{id}");
                assert_eq!(p.outline_level, outline, "{id}");
                assert!(p.keep_next && p.keep_lines, "{id}");
                assert_eq!(d.styles.next_style_id(id), "Normal");
            }
            assert!(run(&d, Some("Heading4"), None).italic);
            assert_eq!(para(&d, "Heading6").outline_level, 5);
        }

        #[test]
        fn title_subtitle_quote_list_paragraph_caption() {
            let d = Document::new();
            let title = run(&d, Some("Title"), None);
            assert_eq!((title.size, title.kern, title.spacing), (56, Some(28), -10));
            assert_eq!(run(&d, Some("Subtitle"), None).color.to_string(), "5A5A5A");
            let quote = para(&d, "Quote");
            assert_eq!(
                (quote.alignment, quote.indent.left, quote.indent.right),
                (Alignment::Center, 864, 864)
            );
            assert!(run(&d, Some("Quote"), None).italic);
            let lp = para(&d, "ListParagraph");
            assert!(lp.contextual_spacing);
            assert_eq!(lp.indent.left, 720);
            let cap = run(&d, Some("Caption"), None);
            assert_eq!(
                (cap.size, cap.italic, cap.color.to_string()),
                (18, true, "44546A".to_owned())
            );
            let iq = para(&d, "IntenseQuote");
            assert_eq!(iq.borders.top.and_then(|b| b.space), Some(10));
        }

        #[test]
        fn table_grid_tocs_notes_headers_footers() {
            let d = Document::new();
            assert_eq!(
                d.styles
                    .get("TableGrid")
                    .and_then(|s| s.tbl_pr.as_ref())
                    .and_then(|t| t.borders.as_ref())
                    .and_then(|b| b.inside_v.as_ref())
                    .map(|b| b.style.clone()),
                Some(BorderStyle::Single)
            );
            let tocs: Vec<i32> = (1..=3)
                .map(|n| para(&d, &format!("TOC{n}")).indent.left)
                .collect();
            assert_eq!(tocs, vec![0, 220, 440]);
            assert_eq!(run(&d, Some("FootnoteText"), None).size, 20);
            let header_tabs: Vec<i32> = para(&d, "Header").tabs.iter().map(|t| t.pos).collect();
            assert_eq!(header_tabs, vec![4680, 9360]);
            assert_eq!(para(&d, "Footer").spacing.after, 0);
            assert_eq!(
                run(&d, None, Some("FootnoteReference")).vert_align,
                VertAlign::Superscript
            );
            assert_eq!(para(&d, "TOCHeading").outline_level, 9);
            assert_eq!(para(&d, "NoSpacing").spacing.after, 0);
        }

        #[test]
        fn character_styles() {
            let d = Document::new();
            let link = run(&d, None, Some("Hyperlink"));
            assert_eq!(link.color.to_string(), "0563C1");
            assert_eq!(
                link.underline.map(|u| u.style),
                Some(UnderlineStyle::Single)
            );
            assert_eq!(
                run(&d, None, Some("FollowedHyperlink")).color.to_string(),
                "954F72"
            );
            assert!(run(&d, None, Some("Strong")).bold);
            assert!(run(&d, None, Some("Emphasis")).italic);
            assert!(!run(&d, Some("Quote"), Some("Emphasis")).italic);
            let ir = run(&d, None, Some("IntenseReference"));
            assert!(ir.bold && ir.small_caps);
            assert_eq!(ir.color.to_string(), "4472C4");
            let bt = run(&d, None, Some("BookTitle"));
            assert!(bt.bold && bt.italic);
            assert!(run(&d, None, Some("SubtleReference")).small_caps);
            assert_eq!(
                run(&d, None, Some("SubtleEmphasis")).color.to_string(),
                "404040"
            );
        }

        #[test]
        fn quick_styles_gallery_is_ordered_like_word() {
            let d = Document::new();
            let names: Vec<String> = d
                .styles
                .quick_styles()
                .iter()
                .map(|s| s.display_name())
                .collect();
            assert_eq!(
                &names[..6],
                [
                    "Normal",
                    "No Spacing",
                    "Heading 1",
                    "Heading 2",
                    "Heading 3",
                    "Heading 4"
                ]
            );
            assert!(names.iter().any(|n| n == "Title"));
            assert!(names.iter().any(|n| n == "Strong"));
            assert!(!names.iter().any(|n| n == "Table Grid"));
        }

        #[test]
        fn one_default_style_per_type() {
            let s = default_style_sheet();
            for (kind, id) in [
                (StyleType::Paragraph, "Normal"),
                (StyleType::Character, "DefaultParagraphFont"),
                (StyleType::Table, "TableNormal"),
                (StyleType::Numbering, "NoList"),
            ] {
                assert_eq!(s.default_style(kind).map(|x| x.id.as_str()), Some(id));
            }
            for st in s.iter() {
                if let Some(link) = &st.link {
                    assert!(s.contains(link), "{} links to missing {link}", st.id);
                }
                if let Some(b) = &st.based_on {
                    assert!(s.contains(b), "{} is based on missing {b}", st.id);
                }
            }
        }

        #[test]
        fn default_style_sheet_snapshot() {
            /// Drops unset (`null`), `false` and empty values so the snapshot shows only what the
            /// template sets.
            fn compact(v: serde_json::Value) -> Option<serde_json::Value> {
                use serde_json::Value;
                match v {
                    Value::Null | Value::Bool(false) => None,
                    Value::Object(map) => {
                        let out: serde_json::Map<String, Value> = map
                            .into_iter()
                            .filter_map(|(k, v)| compact(v).map(|v| (k, v)))
                            .collect();
                        (!out.is_empty()).then_some(Value::Object(out))
                    }
                    Value::Array(items) if items.is_empty() => None,
                    Value::Array(items) => Some(Value::Array(
                        items
                            .into_iter()
                            .map(|i| compact(i).unwrap_or(Value::Null))
                            .collect(),
                    )),
                    other => Some(other),
                }
            }
            let value = serde_json::to_value(default_style_sheet()).unwrap_or_default();
            let json = serde_json::to_string_pretty(&compact(value)).unwrap_or_default();
            insta::assert_snapshot!("default_style_sheet", json);
        }
    }
}
