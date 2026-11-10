// SPDX-License-Identifier: AGPL-3.0-or-later
//! Paste Markdown: Markdown from chat tools, editors and READMEs becomes styled Write content —
//! headings, paragraphs, bulleted and numbered lists (nested by indentation), quotes, code blocks
//! (coloured), horizontal rules, tables (GitHub's pipe tables) and inline bold, italic, code,
//! strikethrough and links.
//!
//! The Markdown is turned into HTML here and pasted through the editor's sanitising HTML import
//! (one mapping to Word's structures for both), then code blocks and rules get Kiln's
//! formatting.

use std::collections::HashSet;
use std::sync::LazyLock;

use kiln_common::{KilnError, NodeId, Result};
use kiln_doc::props::{Border, ParagraphBorders};
use kiln_doc::st::BorderStyle;
use kiln_doc::{DocColor, EditKind, Op, ParagraphProps, Pos, Transaction};
use kiln_protocol::commands::{ClipboardData, EditCommand, PasteMode};
use regex::Regex;

use super::super::WriteDoc;
use super::blocks::CODE_STYLE;

/// Pasted Markdown above this size is refused (the HTML import caps its input too).
const MAX_MARKDOWN: usize = 2 * 1024 * 1024;
/// Marks a horizontal rule's paragraph through the HTML import.
const RULE: &str = "\u{2063}kiln-rule\u{2063}";

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

static CODE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").expect("valid regex"));
static IMAGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"!\[([^\]]*)\]\(([^)\s]+)[^)]*\)").expect("valid regex"));
static LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\(([^)\s]+)[^)]*\)").expect("valid regex"));
static BOLD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\*\*([^\s*](?:[^*]*[^\s*])?)\*\*|__([^\s_](?:[^_]*[^\s_])?)__")
        .expect("valid regex")
});
static ITALIC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[^\w*])\*([^\s*](?:[^*]*[^\s*])?)\*|(^|[^\w_])_([^\s_](?:[^_]*[^\s_])?)_")
        .expect("valid regex")
});
static STRIKE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~~([^~]+)~~").expect("valid regex"));
static LIST: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\s*)([-*+]|\d{1,9}[.)])\s+(.*)$").expect("valid regex"));
static RULE_LINE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s{0,3}(?:(?:\*\s*){3,}|(?:-\s*){3,}|(?:_\s*){3,})$").expect("valid regex")
});
static TABLE_SEP: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\|?\s*:?-{1,}:?\s*(\|\s*:?-{1,}:?\s*)*\|?\s*$").expect("valid regex")
});

/// Inline Markdown → HTML.
fn inline(text: &str) -> String {
    // Code spans first, kept out of the other rules.
    let mut codes: Vec<String> = Vec::new();
    let protected = CODE.replace_all(text, |c: &regex::Captures<'_>| {
        codes.push(format!("<code>{}</code>", escape(&c[1])));
        format!("\u{E000}{}\u{E001}", codes.len() - 1)
    });
    let mut s = escape(&protected);
    s = IMAGE.replace_all(&s, "$1").into_owned();
    s = LINK
        .replace_all(&s, |c: &regex::Captures<'_>| {
            let url = &c[2];
            let safe = url.starts_with("http://")
                || url.starts_with("https://")
                || url.starts_with("mailto:")
                || url.starts_with('#');
            if safe {
                format!("<a href=\"{url}\">{}</a>", &c[1])
            } else {
                c[1].to_owned()
            }
        })
        .into_owned();
    s = BOLD
        .replace_all(&s, |c: &regex::Captures<'_>| {
            format!(
                "<strong>{}</strong>",
                c.get(1).or_else(|| c.get(2)).map_or("", |m| m.as_str())
            )
        })
        .into_owned();
    s = ITALIC
        .replace_all(&s, |c: &regex::Captures<'_>| {
            let (pre, body) = match (c.get(1), c.get(2)) {
                (Some(p), Some(b)) => (p.as_str(), b.as_str()),
                _ => (
                    c.get(3).map_or("", |m| m.as_str()),
                    c.get(4).map_or("", |m| m.as_str()),
                ),
            };
            format!("{pre}<em>{body}</em>")
        })
        .into_owned();
    s = STRIKE.replace_all(&s, "<del>$1</del>").into_owned();
    // Restore the code spans.
    let mut out = String::with_capacity(s.len());
    let mut rest = s.as_str();
    while let Some(i) = rest.find('\u{E000}') {
        out.push_str(&rest[..i]);
        let after = &rest[i + '\u{E000}'.len_utf8()..];
        match after.find('\u{E001}') {
            Some(j) => {
                let n: usize = after[..j].parse().unwrap_or(usize::MAX);
                out.push_str(codes.get(n).map_or("", String::as_str));
                rest = &after[j + '\u{E001}'.len_utf8()..];
            }
            None => {
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

fn table_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_owned()).collect()
}

/// Markdown → HTML for the editor's HTML import.
#[must_use]
pub(super) fn markdown_to_html(md: &str) -> String {
    let lines: Vec<&str> = md.lines().collect();
    let mut out = String::new();
    let mut i = 0;
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, out: &mut String| {
        if !para.is_empty() {
            out.push_str(&format!("<p>{}</p>", inline(&para.join(" "))));
            para.clear();
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let t = line.trim();
        if t.is_empty() {
            flush(&mut para, &mut out);
            i += 1;
            continue;
        }
        // Fenced code.
        if t.starts_with("```") || t.starts_with("~~~") {
            flush(&mut para, &mut out);
            let fence = &t[..3];
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with(fence) {
                code.push(escape(lines[i]));
                i += 1;
            }
            i += 1;
            for l in code {
                out.push_str(&format!(
                    "<pre>{}</pre>",
                    if l.is_empty() { " ".into() } else { l }
                ));
            }
            continue;
        }
        // Headings.
        let hashes = t.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            flush(&mut para, &mut out);
            let text = t[hashes..].trim().trim_end_matches('#').trim();
            out.push_str(&format!("<h{hashes}>{}</h{hashes}>", inline(text)));
            i += 1;
            continue;
        }
        // Rules.
        if RULE_LINE.is_match(line) {
            flush(&mut para, &mut out);
            out.push_str(&format!("<p>{RULE}</p>"));
            i += 1;
            continue;
        }
        // Tables.
        if t.contains('|')
            && lines
                .get(i + 1)
                .is_some_and(|n| TABLE_SEP.is_match(n) && n.contains('-'))
        {
            flush(&mut para, &mut out);
            let head = table_cells(t);
            out.push_str("<table><tr>");
            for c in &head {
                out.push_str(&format!("<th>{}</th>", inline(c)));
            }
            out.push_str("</tr>");
            i += 2;
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                out.push_str("<tr>");
                let mut cells = table_cells(lines[i]);
                cells.resize(head.len().max(1), String::new());
                for c in &cells {
                    out.push_str(&format!("<td>{}</td>", inline(c)));
                }
                out.push_str("</tr>");
                i += 1;
            }
            out.push_str("</table>");
            continue;
        }
        // Quotes.
        if t.starts_with('>') {
            flush(&mut para, &mut out);
            let mut quote = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                quote.push(lines[i].trim_start()[1..].trim().to_owned());
                i += 1;
            }
            for chunk in quote.split(String::is_empty) {
                if !chunk.is_empty() {
                    out.push_str(&format!(
                        "<blockquote>{}</blockquote>",
                        inline(&chunk.join(" "))
                    ));
                }
            }
            continue;
        }
        // Lists.
        if LIST.is_match(line) {
            flush(&mut para, &mut out);
            let mut stack: Vec<(usize, bool)> = Vec::new();
            while i < lines.len() {
                let Some(c) = LIST.captures(lines[i]) else {
                    // A continuation line of the item.
                    if !lines[i].trim().is_empty() && lines[i].starts_with(' ') {
                        out.push_str(&format!(" {}", inline(lines[i].trim())));
                        i += 1;
                        continue;
                    }
                    break;
                };
                let indent = c[1].replace('\t', "    ").chars().count();
                let ordered = c[2].chars().next().is_some_and(|x| x.is_ascii_digit());
                let level = indent / 2;
                while stack.last().is_some_and(|(l, _)| *l > level) {
                    let (_, o) = stack.pop().unwrap_or((0, false));
                    out.push_str(if o { "</li></ol>" } else { "</li></ul>" });
                }
                match stack.last() {
                    Some((l, o)) if *l == level && *o == ordered => out.push_str("</li>"),
                    Some((l, o)) if *l == level => {
                        out.push_str(if *o { "</li></ol>" } else { "</li></ul>" });
                        stack.pop();
                        out.push_str(if ordered { "<ol>" } else { "<ul>" });
                        stack.push((level, ordered));
                    }
                    _ => {
                        if stack.len() < 9 {
                            out.push_str(if ordered { "<ol>" } else { "<ul>" });
                            stack.push((level, ordered));
                        } else {
                            out.push_str("</li>");
                        }
                    }
                }
                let mut text = c[3].to_owned();
                // Task lists show their box.
                if let Some(rest) = text.strip_prefix("[ ] ") {
                    text = format!("☐ {rest}");
                } else if let Some(rest) = text
                    .strip_prefix("[x] ")
                    .or_else(|| text.strip_prefix("[X] "))
                {
                    text = format!("☒ {rest}");
                }
                out.push_str(&format!("<li>{}", inline(&text)));
                i += 1;
            }
            while let Some((_, o)) = stack.pop() {
                out.push_str(if o { "</li></ol>" } else { "</li></ul>" });
            }
            continue;
        }
        para.push(t.to_owned());
        i += 1;
    }
    flush(&mut para, &mut out);
    out
}

impl WriteDoc {
    /// Paste Markdown.
    pub(super) fn paste_markdown(&mut self, text: &str) -> Result<()> {
        if text.trim().is_empty() {
            return Err(KilnError::InvalidInput(
                "there is no Markdown to paste".into(),
            ));
        }
        if text.len() > MAX_MARKDOWN {
            return Err(KilnError::InvalidInput(
                "the Markdown is too large to paste (2 MB at most)".into(),
            ));
        }
        let html = markdown_to_html(text);
        let steps = self.states.len();
        let before: HashSet<NodeId> = self
            .doc
            .all_paragraphs()
            .iter()
            .map(|(_, p)| p.id)
            .collect();
        self.run_edit(&EditCommand::PasteAs {
            mode: PasteMode::Html,
            data: Some(ClipboardData {
                text: text.to_owned(),
                html: Some(html),
                kiln: None,
            }),
        })?;
        // Kiln's code blocks and rules for what the import made of them.
        let new: Vec<NodeId> = self
            .doc
            .all_paragraphs()
            .iter()
            .map(|(_, p)| p.id)
            .filter(|id| !before.contains(id))
            .collect();
        let mut ops = Vec::new();
        let mut code = Vec::new();
        let mut styles_needed = false;
        for id in &new {
            let Some(p) = self.doc.paragraph(*id) else {
                continue;
            };
            let text = p.content.rope().to_string();
            if text.trim() == RULE {
                let mut props = p.props.clone();
                let mut b = props.borders.clone().unwrap_or_default();
                b.bottom = Some(Border {
                    space: Some(1),
                    color: Some(DocColor::auto()),
                    ..Border::new(BorderStyle::Single, 12)
                });
                props.borders = Some(ParagraphBorders { ..b });
                ops.push(Op::DeleteInline {
                    para: *id,
                    offset: 0,
                    len: p.content.len(),
                });
                ops.push(Op::SetParagraphProps {
                    para: *id,
                    props: Box::new(props),
                });
                continue;
            }
            let all_mono = !p.content.runs().is_empty()
                && p.content.runs().iter().all(|r| {
                    r.props
                        .fonts
                        .as_ref()
                        .and_then(|f| f.ascii.as_deref())
                        .is_some_and(|f| f.eq_ignore_ascii_case("Courier New"))
                });
            if all_mono && p.props.numbering.is_none() {
                styles_needed = true;
                ops.push(Op::SetRuns {
                    para: *id,
                    offset: 0,
                    runs: vec![kiln_doc::RunSpan::new(
                        p.content.len(),
                        kiln_doc::RunProps::default(),
                    )],
                });
                ops.push(Op::SetParagraphProps {
                    para: *id,
                    props: Box::new(ParagraphProps {
                        style_id: Some(CODE_STYLE.into()),
                        ..ParagraphProps::default()
                    }),
                });
                code.push(*id);
            }
        }
        // Tables get Word's Grid Table 4 (a tinted header row, banded rows), as a styled table.
        let mut tables: Vec<(NodeId, NodeId)> = Vec::new();
        for id in &new {
            if let Some((t, _, _)) = self.doc.enclosing_cell(*id)
                && !tables.iter().any(|(x, _)| *x == t.id)
            {
                tables.push((t.id, *id));
            }
        }
        if !ops.is_empty() {
            if styles_needed {
                ops.splice(0..0, super::blocks::code_style_ops(&self.doc));
            }
            let sel = self.selection_range();
            self.commit(
                Transaction::new(ops)
                    .with_kind(EditKind::Format)
                    .with_selection(Some(sel), Some(sel)),
            )?;
            self.highlight_code(&code)?;
        }
        if !tables.is_empty() {
            let (anchor, head) = (self.anchor, self.head);
            for (_, first) in &tables {
                self.set_selection(Pos::new(*first, 0), Pos::new(*first, 0));
                self.run_edit(&EditCommand::TableStyle {
                    style: TABLE_STYLE.into(),
                })?;
            }
            self.set_selection(anchor, head);
        }
        // One undo step for the whole paste.
        self.group_undo_since(steps);
        Ok(())
    }
}

/// The table style pasted Markdown tables get.
const TABLE_STYLE: &str = "GridTable4-Accent1";

#[cfg(test)]
mod unit {
    use super::*;

    #[test]
    fn blocks_become_html() {
        let html = markdown_to_html(
            "# Kiln\n\nSome **bold** and *it* with `co*de*` and [a link](https://kiln.example).\n\n- one\n- two\n  - nested\n1. first\n\n> quoted\n\n---\n\n```rust\nfn main() {}\n```\n\n| a | b |\n|---|:-:|\n| 1 | 2 |\n",
        );
        assert!(html.contains("<h1>Kiln</h1>"));
        assert!(html.contains("<strong>bold</strong>"));
        assert!(html.contains("<em>it</em>"));
        assert!(html.contains("<code>co*de*</code>"), "{html}");
        assert!(html.contains("<a href=\"https://kiln.example\">a link</a>"));
        assert!(
            html.contains("<ul><li>one</li><li>two<ul><li>nested</li></ul></li></ul>"),
            "{html}"
        );
        assert!(html.contains("<ol><li>first</li></ol>"));
        assert!(html.contains("<blockquote>quoted</blockquote>"));
        assert!(html.contains(RULE));
        assert!(html.contains("<pre>fn main() {}</pre>"));
        assert!(
            html.contains(
                "<table><tr><th>a</th><th>b</th></tr><tr><td>1</td><td>2</td></tr></table>"
            )
        );
    }

    #[test]
    fn unsafe_links_and_html_are_neutralised() {
        let html =
            markdown_to_html("[x](javascript:alert(1)) <script>bad</script> snake_case_name");
        assert!(!html.contains("javascript"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(
            !html.contains("<em>"),
            "intraword underscores are not emphasis: {html}"
        );
    }
}
