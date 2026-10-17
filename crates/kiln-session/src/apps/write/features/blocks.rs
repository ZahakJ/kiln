// SPDX-License-Identifier: AGPL-3.0-or-later
//! Kiln's blocks and document-level options:
//!
//! - **Code blocks**: paragraphs in Kiln's *Code* style (monospace, shaded, joined) with syntax
//!   colouring as direct run colours, so a saved `.docx` stays coloured in Word. A line is
//!   coloured when Enter ends it (and a whole block when it is inserted or pasted); Enter on an
//!   empty last line leaves the block. Inline `code` uses the same font on a light shading.
//! - **Callouts** (Note, Tip, Warning, Important): a shaded paragraph with a coloured bar and a
//!   bold label, which Word shows as plain paragraph formatting.
//! - **Cover pages**: six designs in a "Cover Pages" building block before the first page
//!   (inserting another replaces it), filled from the document's title, subject and author.
//! - Document properties, the word-count goal (a custom property, so it travels with the file),
//!   automatic hyphenation.

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::props::{Border, Indent, ParagraphBorders, Shading, Spacing};
use kiln_doc::settings::{CustomProperty, PropertyValue};
use kiln_doc::st::{Alignment, BorderStyle, LineRule, SdtKind, StyleType};
use kiln_doc::{
    Block, DocColor, EditKind, Fragment, Inlines, Op, Paragraph, ParagraphProps, Pos, Range,
    RunFonts, RunProps, RunSpan, SdtBlock, SdtInfo, Style, Transaction,
};
use kiln_protocol::commands::write_features::{CalloutKind, CoverDesign, PropertiesEdit};

use super::super::WriteDoc;
use super::missing_styles;

/// Kiln's code paragraph style.
pub(super) const CODE_STYLE: &str = "KilnCode";
/// The code font (Word's; Liberation Mono stands in where it is missing).
const CODE_FONT: &str = "Consolas";
/// The word-count goal's custom property.
const GOAL_PROPERTY: &str = "KilnWordCountGoal";

fn doc_err(e: kiln_doc::DocError) -> KilnError {
    KilnError::InvalidInput(e.to_string())
}

fn fonts(name: &str) -> RunFonts {
    RunFonts {
        ascii: Some(name.to_owned()),
        h_ansi: Some(name.to_owned()),
        cs: Some(name.to_owned()),
        east_asia: None,
        ..RunFonts::default()
    }
}

/// Inline code: the code font on a light shading.
#[must_use]
pub(super) fn code_run_props(base: &RunProps) -> RunProps {
    RunProps {
        fonts: Some(fonts(CODE_FONT)),
        shading: Some(Shading::fill(DocColor::rgb(0xF1_EE_EA))),
        ..base.clone()
    }
}

/// Whether run formatting is code (inline code or a code block's).
#[must_use]
pub(super) fn is_code(props: &RunProps) -> bool {
    props
        .fonts
        .as_ref()
        .and_then(|f| f.ascii.as_deref())
        .is_some_and(|f| f.eq_ignore_ascii_case(CODE_FONT) || f.eq_ignore_ascii_case("Courier New"))
}

/// Kiln's Code paragraph style.
fn code_style() -> Style {
    let mut s = Style::new(CODE_STYLE, StyleType::Paragraph, "Code");
    s.based_on = Some("Normal".into());
    s.custom = true;
    s.quick_style = true;
    s.ui_priority = Some(40);
    s.ppr = Some(ParagraphProps {
        spacing: Some(Spacing {
            before: Some(0),
            after: Some(0),
            line: Some(240),
            line_rule: Some(LineRule::Auto),
            ..Spacing::default()
        }),
        indent: Some(Indent {
            left: Some(144),
            right: Some(144),
            ..Indent::default()
        }),
        contextual_spacing: Some(true),
        shading: Some(Shading::fill(DocColor::rgb(0xF6_F4_F1))),
        borders: Some(ParagraphBorders {
            left: Some(Border {
                space: Some(6),
                color: Some(DocColor::rgb(0xD9_D3_CB)),
                ..Border::new(BorderStyle::Single, 18)
            }),
            ..ParagraphBorders::default()
        }),
        ..ParagraphProps::default()
    });
    s.rpr = Some(RunProps {
        fonts: Some(fonts(CODE_FONT)),
        size: Some(19),
        size_cs: Some(19),
        ..RunProps::default()
    });
    s
}

/// The ops that add Kiln's Code style when the document lacks it.
pub(super) fn code_style_ops(doc: &kiln_doc::Document) -> Vec<Op> {
    let mut ops = missing_styles(doc, &["Normal"]);
    if !doc.styles.contains(CODE_STYLE) {
        ops.push(Op::PutStyle {
            style: Box::new(code_style()),
            index: doc.styles.len() + ops.len(),
        });
    }
    ops
}

/// A syntax token class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Token {
    Keyword,
    Str,
    Comment,
    Number,
    Type,
}

/// Keywords of the common languages (one set: a code block's language is not stored).
const KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "as",
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "def",
    "default",
    "defer",
    "del",
    "do",
    "elif",
    "else",
    "enum",
    "except",
    "export",
    "extends",
    "extern",
    "false",
    "final",
    "finally",
    "fn",
    "for",
    "from",
    "func",
    "function",
    "go",
    "if",
    "impl",
    "implements",
    "import",
    "in",
    "interface",
    "is",
    "lambda",
    "let",
    "loop",
    "match",
    "mod",
    "module",
    "mut",
    "namespace",
    "new",
    "nil",
    "None",
    "not",
    "null",
    "or",
    "package",
    "pass",
    "private",
    "protected",
    "pub",
    "public",
    "raise",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "switch",
    "this",
    "throw",
    "trait",
    "true",
    "True",
    "False",
    "try",
    "type",
    "typeof",
    "undefined",
    "unsafe",
    "use",
    "var",
    "void",
    "where",
    "while",
    "with",
    "yield",
    "select",
    "from",
    "where",
    "insert",
    "update",
    "delete",
    "int",
    "float",
    "double",
    "char",
    "bool",
    "string",
    "str",
    "u8",
    "u32",
    "u64",
    "i32",
    "i64",
    "usize",
    "f32",
    "f64",
];

/// Colours a line of code: `(start, end, token)` in chars.
#[must_use]
pub(super) fn highlight(line: &str) -> Vec<(usize, usize, Token)> {
    let c: Vec<char> = line.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    let first_text = c.iter().position(|x| !x.is_whitespace()).unwrap_or(c.len());
    while i < c.len() {
        let ch = c[i];
        // Comments to the end of the line: //, -- (SQL, Lua), # at the line start or before a
        // space (Python, shell; not Rust's #[attributes] or CSS colours).
        let hash_comment = ch == '#'
            && (i == first_text && c.get(i + 1) != Some(&'[') && c.get(i + 1) != Some(&'!')
                || c.get(i + 1) == Some(&' '));
        if (ch == '/' && c.get(i + 1) == Some(&'/'))
            || (ch == '-' && c.get(i + 1) == Some(&'-') && i == first_text)
            || hash_comment
        {
            out.push((i, c.len(), Token::Comment));
            break;
        }
        if ch == '/' && c.get(i + 1) == Some(&'*') {
            let end = (i + 2..c.len().saturating_sub(1))
                .find(|&k| c[k] == '*' && c[k + 1] == '/')
                .map_or(c.len(), |k| k + 2);
            out.push((i, end, Token::Comment));
            i = end;
            continue;
        }
        if ch == '"' || ch == '\'' || ch == '`' {
            // A Rust lifetime ('a) is not a string.
            if ch == '\''
                && c.get(i + 1).is_some_and(|x| x.is_alphabetic())
                && c.get(i + 2).is_some_and(|x| *x != '\'')
                && i > 0
                && c[i - 1] == '&'
            {
                i += 1;
                continue;
            }
            let mut k = i + 1;
            while k < c.len() && c[k] != ch {
                if c[k] == '\\' {
                    k += 1;
                }
                k += 1;
            }
            let end = (k + 1).min(c.len());
            out.push((i, end, Token::Str));
            i = end;
            continue;
        }
        if ch.is_ascii_digit() && (i == 0 || !(c[i - 1].is_alphanumeric() || c[i - 1] == '_')) {
            let mut k = i;
            while k < c.len() && (c[k].is_ascii_alphanumeric() || c[k] == '.' || c[k] == '_') {
                k += 1;
            }
            out.push((i, k, Token::Number));
            i = k;
            continue;
        }
        if ch.is_alphabetic() || ch == '_' {
            let mut k = i;
            while k < c.len() && (c[k].is_alphanumeric() || c[k] == '_') {
                k += 1;
            }
            let word: String = c[i..k].iter().collect();
            if KEYWORDS.contains(&word.as_str()) {
                out.push((i, k, Token::Keyword));
            } else if ch.is_uppercase() && word.chars().any(char::is_lowercase) {
                out.push((i, k, Token::Type));
            }
            i = k;
            continue;
        }
        i += 1;
    }
    out
}

fn token_props(t: Token) -> RunProps {
    let (rgb, italic) = match t {
        Token::Keyword => (0x008B_3FA8, false),
        Token::Str => (0x002E_7D32, false),
        Token::Comment => (0x007A_746E, true),
        Token::Number => (0x00B5_541C, false),
        Token::Type => (0x001F_5F99, false),
    };
    RunProps {
        color: Some(DocColor::rgb(rgb)),
        italic: italic.then_some(true),
        ..RunProps::default()
    }
}

/// Run spans that colour a code line of `len` chars.
fn code_runs(text: &str, len: usize) -> Vec<RunSpan> {
    let mut runs = Vec::new();
    let mut at = 0;
    for (s, e, t) in highlight(text) {
        let (s, e) = (s.min(len), e.min(len));
        if s > at {
            runs.push(RunSpan::new(s - at, RunProps::default()));
        }
        if e > s {
            runs.push(RunSpan::new(e - s, token_props(t)));
        }
        at = e.max(at);
    }
    if len > at {
        runs.push(RunSpan::new(len - at, RunProps::default()));
    }
    runs
}

fn callout_look(kind: CalloutKind) -> (&'static str, u32, u32) {
    match kind {
        CalloutKind::Note => ("Note", 0x31_59_C6, 0xEC_F1_FB),
        CalloutKind::Tip => ("Tip", 0x0F_7A_5E, 0xE8_F5_EF),
        CalloutKind::Warning => ("Warning", 0xA8_6B_00, 0xFF_F5_DC),
        CalloutKind::Important => ("Important", 0xB8_3A_26, 0xFC_EC_E8),
    }
}

impl WriteDoc {
    /// Colours the code lines among `paras` (not a tracked change).
    pub(super) fn highlight_code(&mut self, paras: &[NodeId]) -> Result<()> {
        let mut ops = Vec::new();
        for id in paras {
            let Some(p) = self.doc.paragraph(*id) else {
                continue;
            };
            if p.props.style_id.as_deref() != Some(CODE_STYLE) {
                continue;
            }
            let text = p.content.rope().to_string();
            let len = p.content.len();
            if len == 0 {
                continue;
            }
            let runs = code_runs(&text, len);
            if runs.as_slice() != p.content.runs() {
                ops.push(Op::SetRuns {
                    para: *id,
                    offset: 0,
                    runs,
                });
            }
        }
        if ops.is_empty() {
            return Ok(());
        }
        let sel = self.selection_range();
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Format)
                .with_selection(Some(sel), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    /// Insert ▸ Code Block: the caret's empty paragraph becomes one, else a new one follows
    /// the caret's paragraph.
    pub(super) fn insert_code_block(&mut self, language: Option<&str>) -> Result<()> {
        if language.is_some_and(|l| l.chars().count() > 40) {
            return Err(KilnError::InvalidInput(
                "a language name is at most 40 characters".into(),
            ));
        }
        let (s, _) = self.span();
        let mut ops = code_style_ops(&self.doc);
        let empty = self.para_len(s.para) == 0;
        let target = if empty {
            let p = self.paragraph(s.para)?;
            ops.push(Op::SetParagraphProps {
                para: s.para,
                props: Box::new(ParagraphProps {
                    style_id: Some(CODE_STYLE.into()),
                    ..p.props.clone()
                }),
            });
            s.para
        } else {
            let (container, index) = self
                .doc
                .parent_of(s.para)
                .ok_or_else(|| KilnError::InvalidInput("no place for a code block".into()))?;
            let mut p = Paragraph::new(self.doc.alloc_id());
            p.props.style_id = Some(CODE_STYLE.into());
            let id = p.id;
            ops.push(Op::InsertBlocks {
                container,
                index: index + 1,
                blocks: vec![Block::Paragraph(p)],
            });
            id
        };
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Structure)
                .with_selection(
                    Some(self.selection_range()),
                    Some(Range::caret(Pos::new(target, 0))),
                ),
        )
    }

    /// ```` ```lang ```` + Enter: the paragraph becomes an empty code line with the caret in it.
    pub(super) fn code_block_from(
        &mut self,
        prev: NodeId,
        next: NodeId,
        len: usize,
        _language: Option<&str>,
    ) -> Result<()> {
        let mut ops = code_style_ops(&self.doc);
        let p = self.paragraph(prev)?;
        ops.push(Op::DeleteInline {
            para: prev,
            offset: 0,
            len,
        });
        ops.push(Op::SetParagraphProps {
            para: prev,
            props: Box::new(ParagraphProps {
                style_id: Some(CODE_STYLE.into()),
                numbering: None,
                ..p.props.clone()
            }),
        });
        ops.push(Op::MergeParagraphs {
            first: prev,
            second: next,
        });
        let at = self.head;
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Other)
                .with_selection(
                    Some(Range::caret(at)),
                    Some(Range::caret(Pos::new(prev, 0))),
                ),
        )
    }

    /// After Enter in a code block: colour the line just ended; Enter on an empty last line
    /// leaves the block.
    pub(super) fn code_after_enter(&mut self, prev: NodeId) -> Result<bool> {
        let at = self.head;
        let is_code_para = |d: &Self, id: NodeId| {
            d.doc
                .paragraph(id)
                .is_some_and(|p| p.props.style_id.as_deref() == Some(CODE_STYLE))
        };
        if !is_code_para(self, prev) {
            return Ok(false);
        }
        if self.para_len(prev) == 0 && self.para_len(at.para) == 0 && is_code_para(self, at.para) {
            let p = self.paragraph(at.para)?;
            let props = ParagraphProps {
                style_id: Some("Normal".into()),
                ..p.props.clone()
            };
            let ops = vec![
                Op::MergeParagraphs {
                    first: prev,
                    second: at.para,
                },
                Op::SetParagraphProps {
                    para: prev,
                    props: Box::new(props),
                },
            ];
            self.commit(
                Transaction::new(ops)
                    .with_kind(EditKind::Other)
                    .with_selection(
                        Some(Range::caret(at)),
                        Some(Range::caret(Pos::new(prev, 0))),
                    ),
            )?;
            return Ok(true);
        }
        self.highlight_code(&[prev])?;
        Ok(true)
    }

    /// Insert ▸ Callout.
    pub(super) fn insert_callout(&mut self, kind: CalloutKind) -> Result<()> {
        let (s, _) = self.span();
        let (label, bar, fill) = callout_look(kind);
        let props = ParagraphProps {
            shading: Some(Shading::fill(DocColor::rgb(fill))),
            borders: Some(ParagraphBorders {
                left: Some(Border {
                    space: Some(8),
                    color: Some(DocColor::rgb(bar)),
                    ..Border::new(BorderStyle::Single, 24)
                }),
                ..ParagraphBorders::default()
            }),
            indent: Some(Indent {
                left: Some(216),
                right: Some(144),
                ..Indent::default()
            }),
            spacing: Some(Spacing {
                before: Some(120),
                after: Some(120),
                ..Spacing::default()
            }),
            ..ParagraphProps::default()
        };
        let mut p = Paragraph::new(self.doc.alloc_id());
        p.props = props;
        let mut c = Inlines::new();
        c.insert(
            0,
            Fragment::text(
                label,
                RunProps {
                    bold: Some(true),
                    color: Some(DocColor::rgb(bar)),
                    ..RunProps::default()
                },
            ),
        )
        .map_err(doc_err)?;
        c.insert(
            label.chars().count(),
            Fragment::text(" ", RunProps::default()),
        )
        .map_err(doc_err)?;
        p.content = c;
        let caret = Pos::new(p.id, label.chars().count() + 1);
        let (container, index) = self
            .doc
            .parent_of(s.para)
            .ok_or_else(|| KilnError::InvalidInput("no place for a callout".into()))?;
        let empty = self.para_len(s.para) == 0;
        let mut ops = vec![Op::InsertBlocks {
            container,
            index: if empty { index } else { index + 1 },
            blocks: vec![Block::Paragraph(p)],
        }];
        if empty && self.doc.container(container).map_or(0, Vec::len) > 1 {
            ops.push(Op::RemoveBlocks {
                container,
                index: index + 1,
                count: 1,
            });
        }
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Structure)
                .with_selection(Some(self.selection_range()), Some(Range::caret(caret))),
        )
    }

    /// Insert ▸ Cover Page.
    #[allow(clippy::too_many_lines)]
    pub(super) fn insert_cover_page(&mut self, design: CoverDesign) -> Result<()> {
        let props = self.doc.properties.clone();
        let title = props
            .title
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Document title".into());
        let subtitle = props
            .subject
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| "Document subtitle".into());
        let author = props
            .creator
            .clone()
            .filter(|t| !t.trim().is_empty())
            .unwrap_or_else(|| self.features.review.author.clone());
        let now = self.now();
        let date = kiln_doc::fields::format_field_date("d MMMM yyyy", &now);
        let mut blocks: Vec<Block> = Vec::new();
        let para = |blocks: &mut Vec<Block>,
                    doc: &mut kiln_doc::Document,
                    text: &str,
                    run: RunProps,
                    pp: ParagraphProps|
         -> Result<()> {
            let mut p = Paragraph::new(doc.alloc_id());
            p.props = pp;
            if !text.is_empty() {
                p.content = Inlines::from_fragment(Fragment::text(text, run)).map_err(doc_err)?;
            }
            blocks.push(Block::Paragraph(p));
            Ok(())
        };
        let serif = |size: u32, bold: bool, color: u32| RunProps {
            fonts: Some(fonts("Cambria")),
            size: Some(size),
            bold: bold.then_some(true),
            color: Some(DocColor::rgb(color)),
            ..RunProps::default()
        };
        let sans = |size: u32, color: u32| RunProps {
            fonts: Some(fonts("Calibri")),
            size: Some(size),
            color: Some(DocColor::rgb(color)),
            ..RunProps::default()
        };
        let space = |before: i32, after: i32| Spacing {
            before: Some(before),
            after: Some(after),
            ..Spacing::default()
        };
        // A band of colour from page edge to page edge: the shading reaches out through
        // borders of the fill's colour (shading covers a border's space), so the text keeps a
        // margin inside the band. `pad_top`/`pad_bottom` (points) pad the band vertically.
        let (margin_l, margin_r) = self.doc.sections.first().map_or((1440, 1440), |s| {
            (s.props.margins.left, s.props.margins.right)
        });
        const BAND_SPACE: u32 = 31; // points, Word's largest border space
        let band_border = |fill: u32, space: u32| Border {
            space: Some(space),
            color: Some(DocColor::rgb(fill)),
            ..Border::new(BorderStyle::Single, 4)
        };
        // The border's space and half-point width, in twips.
        let reach = i32::try_from(BAND_SPACE * 20 + 10).unwrap_or(630);
        let shaded = |fill: u32, pad_top: u32, pad_bottom: u32, align: Alignment| ParagraphProps {
            shading: Some(Shading::fill(DocColor::rgb(fill))),
            spacing: Some(space(0, 0)),
            alignment: Some(align),
            indent: Some(Indent {
                left: Some(reach - margin_l),
                right: Some(reach - margin_r),
                ..Indent::default()
            }),
            borders: Some(ParagraphBorders {
                left: Some(band_border(fill, BAND_SPACE)),
                right: Some(band_border(fill, BAND_SPACE)),
                top: (pad_top > 0).then(|| band_border(fill, pad_top.min(BAND_SPACE))),
                bottom: (pad_bottom > 0).then(|| band_border(fill, pad_bottom.min(BAND_SPACE))),
                ..ParagraphBorders::default()
            }),
            ..ParagraphProps::default()
        };
        // One shaded paragraph holding the whole band: lines separated by line breaks (`None`
        // is an empty line in `blank`'s size), so the band has no seams between paragraphs.
        let band = |doc: &mut kiln_doc::Document,
                    fill: u32,
                    lines: &[Option<(&str, RunProps)>],
                    blank: u32|
         -> Result<Block> {
            let mut p = Paragraph::new(doc.alloc_id());
            p.props = shaded(fill, BAND_SPACE, BAND_SPACE, Alignment::Left);
            let blank_run = RunProps {
                size: Some(blank),
                ..RunProps::default()
            };
            let mut at = 0usize;
            for (i, line) in lines.iter().enumerate() {
                if i > 0 {
                    let props = match line {
                        Some((_, run)) => run.clone(),
                        None => blank_run.clone(),
                    };
                    p.content
                        .insert(
                            at,
                            Fragment::object(
                                kiln_doc::InlineObject::Break(kiln_doc::Break {
                                    kind: kiln_doc::st::BreakKind::TextWrapping,
                                    clear: None,
                                }),
                                props,
                            ),
                        )
                        .map_err(doc_err)?;
                    at += 1;
                }
                if let Some((text, run)) = line {
                    p.content
                        .insert(at, Fragment::text(text, run.clone()))
                        .map_err(doc_err)?;
                    at += text.chars().count();
                }
            }
            Ok(Block::Paragraph(p))
        };
        let plain = |before: i32, after: i32, align: Alignment| ParagraphProps {
            spacing: Some(space(before, after)),
            alignment: Some(align),
            ..ParagraphProps::default()
        };
        let rule = |pp: ParagraphProps, color: u32, top: bool| ParagraphProps {
            borders: Some(if top {
                ParagraphBorders {
                    top: Some(Border {
                        space: Some(12),
                        color: Some(DocColor::rgb(color)),
                        ..Border::new(BorderStyle::Single, 12)
                    }),
                    ..ParagraphBorders::default()
                }
            } else {
                ParagraphBorders {
                    bottom: Some(Border {
                        space: Some(12),
                        color: Some(DocColor::rgb(color)),
                        ..Border::new(BorderStyle::Single, 12)
                    }),
                    ..ParagraphBorders::default()
                }
            }),
            ..pp
        };
        let doc = &mut self.doc;
        match design {
            CoverDesign::Kiln => {
                para(
                    &mut blocks,
                    doc,
                    "",
                    RunProps::default(),
                    plain(2880, 0, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &title,
                    serif(88, false, 0x2A_20_1A),
                    plain(0, 240, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &subtitle,
                    sans(32, 0xC2_4E_14),
                    plain(0, 2880, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &author,
                    sans(24, 0x4A_40_38),
                    plain(0, 60, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &date,
                    sans(22, 0x7A_70_66),
                    plain(0, 0, Alignment::Left),
                )?;
            }
            CoverDesign::Banded => {
                para(
                    &mut blocks,
                    doc,
                    "",
                    RunProps::default(),
                    plain(1440, 0, Alignment::Left),
                )?;
                let b = band(
                    doc,
                    0x31_59_C6,
                    &[
                        Some((title.as_str(), serif(72, true, 0xFF_FF_FF))),
                        Some((subtitle.as_str(), sans(30, 0xE6_EC_FA))),
                    ],
                    24,
                )?;
                blocks.push(b);
                para(
                    &mut blocks,
                    doc,
                    &author,
                    sans(24, 0x2A_20_1A),
                    plain(4320, 60, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &date,
                    sans(22, 0x6A_60_58),
                    plain(0, 0, Alignment::Left),
                )?;
            }
            CoverDesign::Ledger => {
                para(
                    &mut blocks,
                    doc,
                    &title,
                    serif(64, false, 0x1F_2A_44),
                    rule(plain(1440, 240, Alignment::Left), 0x1F_2A_44, false),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &subtitle,
                    sans(28, 0x4A_55_6E),
                    plain(120, 4320, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &author,
                    sans(24, 0x1F_2A_44),
                    plain(0, 60, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &date,
                    sans(22, 0x6A_70_80),
                    plain(0, 0, Alignment::Left),
                )?;
            }
            CoverDesign::Classic => {
                para(
                    &mut blocks,
                    doc,
                    &title,
                    serif(60, true, 0x20_20_20),
                    rule(plain(2880, 120, Alignment::Center), 0x20_20_20, true),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &subtitle,
                    serif(28, false, 0x40_40_40),
                    rule(plain(0, 4320, Alignment::Center), 0x20_20_20, false),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &author,
                    sans(24, 0x30_30_30),
                    plain(0, 60, Alignment::Center),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &date,
                    sans(22, 0x60_60_60),
                    plain(0, 0, Alignment::Center),
                )?;
            }
            CoverDesign::Slab => {
                let b = band(
                    doc,
                    0x2A_20_1A,
                    &[
                        None,
                        None,
                        None,
                        Some((title.as_str(), serif(76, false, 0xFF_F4_E8))),
                        Some((subtitle.as_str(), sans(30, 0xFF_9A_5C))),
                        None,
                    ],
                    56,
                )?;
                blocks.push(b);
                para(
                    &mut blocks,
                    doc,
                    &author,
                    sans(24, 0x2A_20_1A),
                    plain(2160, 60, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &date,
                    sans(22, 0x6A_60_58),
                    plain(0, 0, Alignment::Left),
                )?;
            }
            CoverDesign::Quiet => {
                para(
                    &mut blocks,
                    doc,
                    &title,
                    serif(56, false, 0x2A_20_1A),
                    plain(8640, 120, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &subtitle,
                    sans(26, 0x6A_60_58),
                    plain(0, 360, Alignment::Left),
                )?;
                para(
                    &mut blocks,
                    doc,
                    &format!("{author} · {date}"),
                    sans(22, 0x6A_60_58),
                    plain(0, 0, Alignment::Left),
                )?;
            }
        }
        // The page ends after the cover: the last paragraph carries a page break.
        if let Some(Block::Paragraph(last)) = blocks.last_mut() {
            let at = last.content.len();
            last.content
                .insert(
                    at,
                    Fragment::object(
                        kiln_doc::InlineObject::Break(kiln_doc::Break {
                            kind: kiln_doc::st::BreakKind::Page,
                            clear: None,
                        }),
                        RunProps::default(),
                    ),
                )
                .map_err(doc_err)?;
        }
        let sdt = SdtBlock {
            id: self.doc.alloc_id(),
            info: SdtInfo {
                kind: SdtKind::DocPartObj,
                doc_part_gallery: Some("Cover Pages".into()),
                tag: Some(format!("kiln:cover:{design:?}").to_lowercase()),
                alias: Some("Cover Page".into()),
                ..SdtInfo::default()
            },
            blocks,
        };
        let Some(section) = self.doc.sections.first() else {
            return Err(KilnError::InvalidInput("the document has no body".into()));
        };
        let section_id = section.id;
        let existing = section.blocks.iter().position(|b| {
            matches!(b, Block::Sdt(s) if s.info.doc_part_gallery.as_deref() == Some("Cover Pages"))
        });
        let mut ops = vec![Op::InsertBlocks {
            container: section_id,
            index: 0,
            blocks: vec![Block::Sdt(sdt)],
        }];
        if let Some(i) = existing {
            ops.push(Op::RemoveBlocks {
                container: section_id,
                index: i + 1,
                count: 1,
            });
        }
        let sel = self.selection_range();
        self.commit(
            Transaction::new(ops)
                .with_kind(EditKind::Structure)
                .with_selection(Some(sel), Some(sel)),
        )
    }

    /// File ▸ Info ▸ Properties.
    pub(super) fn set_properties(&mut self, edit: &PropertiesEdit) -> Result<()> {
        let mut p = self.doc.properties.clone();
        let set = |field: &mut Option<String>, v: &Option<String>| -> Result<()> {
            if let Some(v) = v {
                if v.chars().count() > 1024 {
                    return Err(KilnError::InvalidInput(
                        "a property holds at most 1,024 characters".into(),
                    ));
                }
                *field = (!v.trim().is_empty()).then(|| v.trim().to_owned());
            }
            Ok(())
        };
        set(&mut p.title, &edit.title)?;
        set(&mut p.subject, &edit.subject)?;
        set(&mut p.creator, &edit.author)?;
        set(&mut p.keywords, &edit.keywords)?;
        set(&mut p.description, &edit.description)?;
        set(&mut p.category, &edit.category)?;
        set(&mut p.content_status, &edit.status)?;
        set(&mut p.company, &edit.company)?;
        set(&mut p.manager, &edit.manager)?;
        if p == self.doc.properties {
            return Ok(());
        }
        let sel = self.selection_range();
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(vec![Op::SetProperties {
                properties: Box::new(p),
            }])
            .with_kind(EditKind::Other)
            .with_selection(Some(sel), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    /// The word-count goal (0: none).
    pub(super) fn word_goal(&self) -> u32 {
        self.doc
            .properties
            .custom
            .iter()
            .find(|c| c.name == GOAL_PROPERTY)
            .and_then(|c| match &c.value {
                PropertyValue::Integer(n) => u32::try_from(*n).ok(),
                PropertyValue::Text(t) | PropertyValue::Real(t) => t.trim().parse().ok(),
                _ => None,
            })
            .unwrap_or(0)
    }

    /// Sets the word-count goal (saved with the document as a custom property).
    pub(super) fn set_word_goal(&mut self, words: u32) -> Result<()> {
        if words > 10_000_000 {
            return Err(KilnError::InvalidInput(
                "a goal of at most 10,000,000 words".into(),
            ));
        }
        if words == self.word_goal() {
            return Ok(());
        }
        let mut p = self.doc.properties.clone();
        p.custom.retain(|c| c.name != GOAL_PROPERTY);
        if words > 0 {
            p.custom.push(CustomProperty {
                name: GOAL_PROPERTY.into(),
                value: PropertyValue::Integer(i64::from(words)),
            });
        }
        let sel = self.selection_range();
        self.features.untracked = true;
        let r = self.commit(
            Transaction::new(vec![Op::SetProperties {
                properties: Box::new(p),
            }])
            .with_kind(EditKind::Other)
            .with_selection(Some(sel), Some(sel)),
        );
        self.features.untracked = false;
        r
    }

    /// Layout ▸ Hyphenation ▸ Automatic.
    pub(super) fn toggle_hyphenation(&mut self) -> Result<()> {
        let mut settings = self.doc.settings.clone();
        settings.auto_hyphenation = !settings.auto_hyphenation;
        let sel = self.selection_range();
        self.commit(
            Transaction::new(vec![Op::SetSettings {
                settings: Box::new(settings),
            }])
            .with_kind(EditKind::Format)
            .with_selection(Some(sel), Some(sel)),
        )
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn highlights_common_code() {
        let t = highlight("let x = \"glaze\"; // fire it");
        assert!(t.contains(&(0, 3, Token::Keyword)));
        assert!(t.contains(&(8, 15, Token::Str)));
        assert!(t.iter().any(|(s, _, k)| *k == Token::Comment && *s == 17));
        let py = highlight("# kiln.py\nimport os");
        assert_eq!(py[0].2, Token::Comment);
        let rust = highlight("#[derive(Debug)] struct Kiln(u32);");
        assert!(rust.iter().all(|(_, _, k)| *k != Token::Comment));
        assert!(rust.iter().any(|(_, _, k)| *k == Token::Type));
        assert!(
            highlight("x = 0x1F + 42")
                .iter()
                .filter(|(_, _, k)| *k == Token::Number)
                .count()
                == 2
        );
        assert!(
            highlight("color: #fff;")
                .iter()
                .all(|(_, _, k)| *k != Token::Comment)
        );
    }

    #[test]
    fn code_runs_cover_the_line() {
        let text = "fn main() {}";
        let runs = code_runs(text, text.chars().count());
        assert_eq!(
            runs.iter().map(|r| r.len).sum::<usize>(),
            text.chars().count()
        );
        assert!(runs[0].props.color.is_some(), "fn is a keyword");
    }

    #[test]
    fn code_formatting_is_recognised() {
        assert!(is_code(&code_run_props(&RunProps::default())));
        assert!(!is_code(&RunProps::default()));
    }
}
