// SPDX-License-Identifier: AGPL-3.0-or-later
//! The "Blank document" template: Word 2016–2021 `Normal.dotm` defaults, because that is what most
//! `.docx` files in the wild were written against. Body text is the minor theme font (Calibri,
//! rendered with the metric-compatible Carlito) at 11 pt, 1.08 line spacing (`259` auto), 8 pt
//! after; headings use the major theme font (Calibri Light) in shades of Accent 1. Files keep
//! their own values; only new documents use these.

use crate::model::{Block, Document, Paragraph, Section};
use crate::numbering::Numbering;
use crate::props::{
    Border, CellMargins, DocColor, Indent, Lang, ParagraphBorders, ParagraphProps, RunFonts,
    RunProps, Spacing, TabStop, TableBorders, TableProps, Underline,
};
use crate::section::{HeaderFooterRefs, Paper, SectionProps};
use crate::settings::{DocProperties, Settings};
use crate::st::{
    Alignment, BorderStyle, LineRule, StyleType, TabAlignment, ThemeColorName as T, ThemeFontRef,
    UnderlineStyle, VertAlign,
};
use crate::style::{DocDefaults, Style, StyleSheet};
use crate::theme::DocTheme;

fn major() -> RunFonts {
    RunFonts {
        ascii_theme: Some(ThemeFontRef::MajorHAnsi),
        east_asia_theme: Some(ThemeFontRef::MajorEastAsia),
        h_ansi_theme: Some(ThemeFontRef::MajorHAnsi),
        cs_theme: Some(ThemeFontRef::MajorBidi),
        ..RunFonts::default()
    }
}

fn minor() -> RunFonts {
    RunFonts {
        ascii_theme: Some(ThemeFontRef::MinorHAnsi),
        east_asia_theme: Some(ThemeFontRef::MinorHAnsi),
        h_ansi_theme: Some(ThemeFontRef::MinorHAnsi),
        cs_theme: Some(ThemeFontRef::MinorBidi),
        ..RunFonts::default()
    }
}

fn spacing(before: Option<i32>, after: Option<i32>, line: Option<i32>) -> Spacing {
    Spacing {
        before,
        after,
        line,
        line_rule: line.map(|_| LineRule::Auto),
        ..Spacing::default()
    }
}

fn single_line_no_after() -> ParagraphProps {
    ParagraphProps {
        spacing: Some(spacing(None, Some(0), Some(240))),
        ..ParagraphProps::default()
    }
}

fn style(id: &str, kind: StyleType, name: &str, priority: u32) -> Style {
    Style {
        ui_priority: Some(priority),
        ..Style::new(id, kind, name)
    }
}

fn para_style(id: &str, name: &str, priority: u32) -> Style {
    Style {
        based_on: Some("Normal".into()),
        next: Some("Normal".into()),
        ..style(id, StyleType::Paragraph, name, priority)
    }
}

fn char_style(id: &str, name: &str, priority: u32, rpr: RunProps) -> Style {
    Style {
        based_on: Some("DefaultParagraphFont".into()),
        rpr: Some(rpr),
        ..style(id, StyleType::Character, name, priority)
    }
}

fn linked_char(id: &str, name: &str, para: &str, rpr: Option<RunProps>, priority: u32) -> Style {
    Style {
        based_on: Some("DefaultParagraphFont".into()),
        link: Some(para.into()),
        rpr,
        ..style(id, StyleType::Character, name, priority)
    }
}

fn hidden(mut s: Style, semi_hidden: bool) -> Style {
    s.semi_hidden = semi_hidden;
    s.unhide_when_used = true;
    s
}

fn quick(mut s: Style) -> Style {
    s.quick_style = true;
    s
}

fn italic() -> RunProps {
    RunProps {
        italic: Some(true),
        italic_cs: Some(true),
        ..RunProps::default()
    }
}

fn bold() -> RunProps {
    RunProps {
        bold: Some(true),
        bold_cs: Some(true),
        ..RunProps::default()
    }
}

fn sized(half_points: u32) -> RunProps {
    RunProps {
        size: Some(half_points),
        size_cs: Some(half_points),
        ..RunProps::default()
    }
}

/// Word's built-in styles of a new document, in Word's file order.
#[must_use]
pub fn default_styles() -> Vec<Style> {
    let accent1 = |cached: u32, shade: u8| DocColor::theme(cached, T::Accent1).with_shade(shade);
    let heading_rpr = |cached: u32, shade: u8, size: Option<u32>, italic_on: bool| RunProps {
        fonts: Some(major()),
        color: Some(accent1(cached, shade)),
        size,
        size_cs: size,
        italic: italic_on.then_some(true),
        italic_cs: italic_on.then_some(true),
        ..RunProps::default()
    };
    let h_rpr = [
        heading_rpr(0x2F5496, 0xBF, Some(32), false),
        heading_rpr(0x2F5496, 0xBF, Some(26), false),
        heading_rpr(0x1F3763, 0x7F, Some(24), false),
        heading_rpr(0x2F5496, 0xBF, None, true),
        heading_rpr(0x2F5496, 0xBF, None, false),
        heading_rpr(0x1F3763, 0x7F, None, false),
    ];
    let heading = |n: u8| {
        let level = n - 1;
        let ppr = if n == 1 {
            ParagraphProps {
                keep_next: Some(true),
                keep_lines: Some(true),
                spacing: Some(spacing(Some(240), Some(0), None)),
                outline_level: Some(0),
                ..ParagraphProps::default()
            }
        } else {
            ParagraphProps {
                keep_next: Some(true),
                keep_lines: Some(true),
                spacing: Some(spacing(Some(40), Some(0), None)),
                outline_level: Some(level),
                ..ParagraphProps::default()
            }
        };
        let mut s = quick(Style {
            link: Some(format!("Heading{n}Char")),
            ppr: Some(ppr),
            rpr: Some(h_rpr[usize::from(level)].clone()),
            ..para_style(&format!("Heading{n}"), &format!("heading {n}"), 9)
        });
        s.unhide_when_used = n > 1;
        s
    };
    let title_rpr = RunProps {
        fonts: Some(major()),
        spacing: Some(-10),
        kern: Some(28),
        ..sized(56)
    };
    let text1_tint = |cached: u32, tint: u8| DocColor::theme(cached, T::Text1).with_tint(tint);
    let subtitle_rpr = RunProps {
        fonts: Some(RunFonts {
            east_asia_theme: Some(ThemeFontRef::MinorEastAsia),
            ..RunFonts::default()
        }),
        color: Some(text1_tint(0x5A5A5A, 0xA5)),
        spacing: Some(15),
        ..RunProps::default()
    };
    let quote_rpr = RunProps {
        color: Some(text1_tint(0x404040, 0xBF)),
        ..italic()
    };
    let accent = DocColor::theme(0x4472C4, T::Accent1);
    let hf_tabs = vec![
        TabStop::new(4680, TabAlignment::Center),
        TabStop::new(9360, TabAlignment::Right),
    ];
    let grid_border = Border {
        style: BorderStyle::Single,
        size: Some(4),
        space: Some(0),
        color: Some(DocColor::auto()),
        ..Border::default()
    };
    let toc = |n: i32| Style {
        unhide_when_used: true,
        ppr: Some(ParagraphProps {
            spacing: Some(spacing(None, Some(100), None)),
            indent: (n > 1).then_some(Indent {
                left: Some((n - 1) * 220),
                ..Indent::default()
            }),
            ..ParagraphProps::default()
        }),
        ..para_style(&format!("TOC{n}"), &format!("toc {n}"), 39)
    };
    let link_rpr = |cached: u32, theme: T| RunProps {
        color: Some(DocColor::theme(cached, theme)),
        underline: Some(Underline::new(UnderlineStyle::Single)),
        ..RunProps::default()
    };
    let footnote_text_rpr = sized(20);

    let mut styles = vec![quick(Style {
        is_default: true,
        ..style("Normal", StyleType::Paragraph, "Normal", 0)
    })];
    styles.extend((1..=6).map(heading));
    styles.extend([
        hidden(
            Style {
                is_default: true,
                ..style(
                    "DefaultParagraphFont",
                    StyleType::Character,
                    "Default Paragraph Font",
                    1,
                )
            },
            true,
        ),
        hidden(
            Style {
                is_default: true,
                tbl_pr: Some(TableProps {
                    indent: Some(0),
                    cell_margins: Some(CellMargins {
                        top: Some(0),
                        left: Some(108),
                        bottom: Some(0),
                        right: Some(108),
                    }),
                    ..TableProps::default()
                }),
                ..style("TableNormal", StyleType::Table, "Normal Table", 99)
            },
            true,
        ),
        hidden(
            Style {
                is_default: true,
                ..style("NoList", StyleType::Numbering, "No List", 99)
            },
            true,
        ),
    ]);
    for n in 1..=6u8 {
        styles.push(linked_char(
            &format!("Heading{n}Char"),
            &format!("Heading {n} Char"),
            &format!("Heading{n}"),
            Some(h_rpr[usize::from(n - 1)].clone()),
            9,
        ));
    }
    styles.extend([
        quick(Style {
            link: Some("TitleChar".into()),
            ppr: Some(ParagraphProps {
                spacing: Some(spacing(None, Some(0), Some(240))),
                contextual_spacing: Some(true),
                ..ParagraphProps::default()
            }),
            rpr: Some(title_rpr.clone()),
            ..para_style("Title", "Title", 10)
        }),
        linked_char("TitleChar", "Title Char", "Title", Some(title_rpr), 10),
        quick(Style {
            link: Some("SubtitleChar".into()),
            ppr: Some(ParagraphProps {
                spacing: Some(spacing(None, Some(160), None)),
                ..ParagraphProps::default()
            }),
            rpr: Some(subtitle_rpr.clone()),
            ..para_style("Subtitle", "Subtitle", 11)
        }),
        linked_char(
            "SubtitleChar",
            "Subtitle Char",
            "Subtitle",
            Some(subtitle_rpr),
            11,
        ),
        quick(char_style(
            "SubtleEmphasis",
            "Subtle Emphasis",
            19,
            RunProps {
                color: Some(text1_tint(0x404040, 0xBF)),
                ..italic()
            },
        )),
        quick(char_style("Emphasis", "Emphasis", 20, italic())),
        quick(char_style(
            "IntenseEmphasis",
            "Intense Emphasis",
            21,
            RunProps {
                color: Some(accent.clone()),
                ..italic()
            },
        )),
        quick(char_style("Strong", "Strong", 22, bold())),
        quick(Style {
            link: Some("QuoteChar".into()),
            ppr: Some(ParagraphProps {
                spacing: Some(spacing(Some(200), Some(160), None)),
                indent: Some(Indent {
                    left: Some(864),
                    right: Some(864),
                    ..Indent::default()
                }),
                alignment: Some(Alignment::Center),
                ..ParagraphProps::default()
            }),
            rpr: Some(quote_rpr.clone()),
            ..para_style("Quote", "Quote", 29)
        }),
        linked_char("QuoteChar", "Quote Char", "Quote", Some(quote_rpr), 29),
        quick(Style {
            ppr: Some(ParagraphProps {
                borders: Some(ParagraphBorders {
                    top: Some(Border {
                        space: Some(10),
                        color: Some(accent.clone()),
                        ..Border::new(BorderStyle::Single, 4)
                    }),
                    bottom: Some(Border {
                        space: Some(10),
                        color: Some(accent.clone()),
                        ..Border::new(BorderStyle::Single, 4)
                    }),
                    ..ParagraphBorders::default()
                }),
                spacing: Some(spacing(Some(360), Some(360), None)),
                indent: Some(Indent {
                    left: Some(864),
                    right: Some(864),
                    ..Indent::default()
                }),
                alignment: Some(Alignment::Center),
                ..ParagraphProps::default()
            }),
            rpr: Some(RunProps {
                color: Some(accent.clone()),
                ..italic()
            }),
            ..para_style("IntenseQuote", "Intense Quote", 30)
        }),
        quick(char_style(
            "SubtleReference",
            "Subtle Reference",
            31,
            RunProps {
                small_caps: Some(true),
                color: Some(text1_tint(0x5A5A5A, 0xA5)),
                ..RunProps::default()
            },
        )),
        quick(char_style(
            "IntenseReference",
            "Intense Reference",
            32,
            RunProps {
                small_caps: Some(true),
                color: Some(accent),
                spacing: Some(5),
                ..bold()
            },
        )),
        quick(char_style(
            "BookTitle",
            "Book Title",
            33,
            RunProps {
                italic: Some(true),
                italic_cs: Some(true),
                spacing: Some(5),
                ..bold()
            },
        )),
        quick(Style {
            next: None,
            ppr: Some(ParagraphProps {
                indent: Some(Indent {
                    left: Some(720),
                    ..Indent::default()
                }),
                contextual_spacing: Some(true),
                ..ParagraphProps::default()
            }),
            ..para_style("ListParagraph", "List Paragraph", 34)
        }),
        quick(hidden(
            Style {
                ppr: Some(ParagraphProps {
                    spacing: Some(spacing(None, Some(200), Some(240))),
                    ..ParagraphProps::default()
                }),
                rpr: Some(RunProps {
                    color: Some(DocColor::theme(0x44546A, T::Text2)),
                    ..RunProps {
                        italic: Some(true),
                        italic_cs: Some(true),
                        ..sized(18)
                    }
                }),
                ..para_style("Caption", "caption", 35)
            },
            false,
        )),
        Style {
            based_on: Some("TableNormal".into()),
            ppr: Some(single_line_no_after()),
            tbl_pr: Some(TableProps {
                borders: Some(TableBorders {
                    top: Some(grid_border.clone()),
                    left: Some(grid_border.clone()),
                    bottom: Some(grid_border.clone()),
                    right: Some(grid_border.clone()),
                    inside_h: Some(grid_border.clone()),
                    inside_v: Some(grid_border),
                }),
                ..TableProps::default()
            }),
            ..style("TableGrid", StyleType::Table, "Table Grid", 39)
        },
        quick(hidden(
            Style {
                based_on: Some("Heading1".into()),
                ppr: Some(ParagraphProps {
                    outline_level: Some(9),
                    ..ParagraphProps::default()
                }),
                ..para_style("TOCHeading", "TOC Heading", 39)
            },
            false,
        )),
        toc(1),
        toc(2),
        toc(3),
        hidden(
            Style {
                next: None,
                link: Some("FootnoteTextChar".into()),
                ppr: Some(single_line_no_after()),
                rpr: Some(footnote_text_rpr.clone()),
                ..para_style("FootnoteText", "footnote text", 99)
            },
            true,
        ),
        Style {
            semi_hidden: true,
            ..linked_char(
                "FootnoteTextChar",
                "Footnote Text Char",
                "FootnoteText",
                Some(footnote_text_rpr.clone()),
                99,
            )
        },
        hidden(
            char_style(
                "FootnoteReference",
                "footnote reference",
                99,
                RunProps {
                    vert_align: Some(VertAlign::Superscript),
                    ..RunProps::default()
                },
            ),
            true,
        ),
        hidden(
            Style {
                next: None,
                ppr: Some(single_line_no_after()),
                rpr: Some(footnote_text_rpr),
                ..para_style("EndnoteText", "endnote text", 99)
            },
            true,
        ),
        hidden(
            char_style(
                "EndnoteReference",
                "endnote reference",
                99,
                RunProps {
                    vert_align: Some(VertAlign::Superscript),
                    ..RunProps::default()
                },
            ),
            true,
        ),
        hidden(
            Style {
                next: None,
                link: Some("HeaderChar".into()),
                ppr: Some(ParagraphProps {
                    tabs: Some(hf_tabs.clone()),
                    ..single_line_no_after()
                }),
                ..para_style("Header", "header", 99)
            },
            false,
        ),
        linked_char("HeaderChar", "Header Char", "Header", None, 99),
        hidden(
            Style {
                next: None,
                link: Some("FooterChar".into()),
                ppr: Some(ParagraphProps {
                    tabs: Some(hf_tabs),
                    ..single_line_no_after()
                }),
                ..para_style("Footer", "footer", 99)
            },
            false,
        ),
        linked_char("FooterChar", "Footer Char", "Footer", None, 99),
        hidden(
            char_style(
                "Hyperlink",
                "Hyperlink",
                99,
                link_rpr(0x0563C1, T::Hyperlink),
            ),
            false,
        ),
        hidden(
            char_style(
                "FollowedHyperlink",
                "FollowedHyperlink",
                99,
                link_rpr(0x954F72, T::FollowedHyperlink),
            ),
            true,
        ),
        quick(Style {
            ppr: Some(single_line_no_after()),
            ..style("NoSpacing", StyleType::Paragraph, "No Spacing", 1)
        }),
    ]);
    styles
}

/// Word 2016+ `w:docDefaults`: minor theme font, 11 pt, en-US; 8 pt after, 1.08 line spacing.
#[must_use]
pub fn default_doc_defaults() -> DocDefaults {
    DocDefaults {
        rpr: RunProps {
            fonts: Some(minor()),
            lang: Some(Lang {
                val: Some("en-US".into()),
                east_asia: Some("en-US".into()),
                bidi: Some("ar-SA".into()),
            }),
            ..sized(22)
        },
        ppr: ParagraphProps {
            spacing: Some(spacing(None, Some(160), Some(259))),
            ..ParagraphProps::default()
        },
    }
}

/// The style sheet of a new document.
#[must_use]
pub fn default_style_sheet() -> StyleSheet {
    StyleSheet::new(default_doc_defaults(), default_styles())
}

/// Options for [`Document::with_options`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NewDocumentOptions {
    /// BCP 47 locale of the text (default `en-US`); sets the language and, without `paper`, the
    /// paper size.
    pub locale: Option<String>,
    /// Paper size (default from the locale: Letter for US/CA/MX/…, A4 elsewhere).
    pub paper: Option<Paper>,
}

impl Document {
    /// A new blank document: en-US, Letter, Word's default styles, one empty paragraph.
    #[must_use]
    pub fn new() -> Self {
        Self::with_options(&NewDocumentOptions::default())
    }

    /// A new blank document for a locale and paper.
    #[must_use]
    pub fn with_options(options: &NewDocumentOptions) -> Self {
        let locale = options.locale.clone().unwrap_or_else(|| "en-US".to_owned());
        let paper = options.paper.unwrap_or_else(|| Paper::for_locale(&locale));
        let mut styles = default_style_sheet();
        if locale != "en-US"
            && let Some(lang) = styles.doc_defaults.rpr.lang.as_mut()
        {
            lang.val = Some(locale);
        }
        let mut doc = Document::empty();
        doc.styles = styles;
        doc.numbering = Numbering::new();
        doc.settings = Settings::default();
        doc.properties = DocProperties::default();
        doc.theme = DocTheme::office();
        let para = Paragraph::new(doc.alloc_id());
        let section = Section {
            id: doc.alloc_id(),
            props: SectionProps::new(paper),
            headers: HeaderFooterRefs::default(),
            footers: HeaderFooterRefs::default(),
            blocks: vec![Block::Paragraph(para)],
        };
        doc.sections.push(section);
        doc
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
