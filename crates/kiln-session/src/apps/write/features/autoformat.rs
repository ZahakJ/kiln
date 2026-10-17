// SPDX-License-Identifier: AGPL-3.0-or-later
//! AutoFormat As You Type: Kiln's Markdown shortcuts and Word's smart typography, applied right
//! after the character that completes them, each as its own undo step (Ctrl+Z after an
//! AutoFormat brings back what was typed, as in Word).
//!
//! - Paragraphs (Markdown shortcuts, on the space after the marker at the start of a
//!   paragraph): `# `…`###### ` headings, `- `, `* `, `+ ` bullets, `1. `/`1) ` numbering, `> `
//!   quotes; after Enter: `---`, `***`, `___` become a horizontal rule (a bottom border), and
//!   ```` ``` ```` (with an optional language) a code block.
//! - Inline (on the closing marker): `**bold**`, `__bold__`, `*italic*`, `_italic_`,
//!   `` `code` ``, `~~strikethrough~~`. Markers inside words (`snake_case`) are left alone.
//! - Typography: "straight" quotes → “smart” quotes (apostrophes too), `--` between words → em
//!   dash, ` - ` and ` -- ` between words → en dash, `...` → ellipsis, 1/2 1/4 3/4 (and thirds, eighths) →
//!   fraction characters. Not inside code.

use kiln_doc::props::Border;
use kiln_doc::{EditKind, Fragment, Op, Pos, Range, RunProps, RunSpan, Transaction};
use kiln_protocol::WriteCommand;
use kiln_protocol::commands::EditCommand;

use super::super::WriteDoc;
use super::blocks::{code_run_props, is_code};

/// Characters a Markdown inline marker may follow (or precede) at a word boundary.
fn boundary(c: Option<char>) -> bool {
    c.is_none_or(|c| c.is_whitespace() || "([{\"'“‘—–-/".contains(c))
}

/// The fraction character for a typed fraction.
fn fraction(s: &str) -> Option<char> {
    Some(match s {
        "1/2" => '½',
        "1/4" => '¼',
        "3/4" => '¾',
        "1/3" => '⅓',
        "2/3" => '⅔',
        "1/8" => '⅛',
        "3/8" => '⅜',
        "5/8" => '⅝',
        "7/8" => '⅞',
        _ => return None,
    })
}

/// A recognised inline span: the opener's start, the content's range, the closer's end, and
/// what to do with the content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InlineKind {
    Bold,
    Italic,
    Code,
    Strike,
}

/// Finds an inline Markdown span that ends at `end` (the caret, right after the closing marker)
/// in `chars`: `(open_start, open_len, close_len, kind)`.
pub(super) fn inline_span(chars: &[char], end: usize) -> Option<(usize, usize, usize, InlineKind)> {
    let at = |i: usize| chars.get(i).copied();
    if end == 0 || end > chars.len() {
        return None;
    }
    let last = chars[end - 1];
    let candidates: &[(&str, InlineKind)] = match last {
        '*' => &[("**", InlineKind::Bold), ("*", InlineKind::Italic)],
        '_' => &[("__", InlineKind::Bold), ("_", InlineKind::Italic)],
        '`' => &[("`", InlineKind::Code)],
        '~' => &[("~~", InlineKind::Strike)],
        _ => return None,
    };
    for &(marker, kind) in candidates {
        let m: Vec<char> = marker.chars().collect();
        let n = m.len();
        if end < n * 2 + 1 || chars[end - n..end] != m[..] {
            continue;
        }
        // `***` is not `*` closing an italic: the closer must not be longer than its marker.
        if end > n && at(end - n - 1) == Some(m[0]) {
            continue;
        }
        let content_end = end - n;
        // The content does not end with a space.
        if at(content_end - 1).is_none_or(char::is_whitespace) {
            continue;
        }
        // Search back for the opener on this line of text.
        let mut i = content_end;
        while i >= n {
            let start = i - n;
            if chars[start..i] == m[..]
                && i < content_end
                && start.checked_sub(1).is_none_or(|b| chars[b] != m[0])
                && boundary(start.checked_sub(1).map(|b| chars[b]))
                && at(i).is_some_and(|c| !c.is_whitespace() && c != m[0])
            {
                // Code may hold anything; other spans need a closer at a word end.
                if kind != InlineKind::Code && chars[i..content_end].contains(&'`') {
                    break;
                }
                return Some((start, n, n, kind));
            }
            if chars[start + n - 1] == '\n' || chars[start + n - 1] == '\u{FFFC}' {
                break;
            }
            i -= 1;
        }
    }
    None
}

impl WriteDoc {
    /// Runs AutoFormat after typed text (best effort: a failed AutoFormat leaves the text as
    /// typed).
    pub(in crate::apps::write) fn after_typing(&mut self, text: &str) {
        if self.composition.is_some() || self.anchor != self.head {
            return;
        }
        let o = self.features.autoformat.0;
        let result = if text == "\n" || text == "\r\n" || text == "\r" {
            self.after_enter(o.markdown)
        } else {
            let mut chars = text.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) => self.after_char(c),
                _ => Ok(()),
            }
        };
        if let Err(e) = result {
            // AutoFormat never blocks typing; the command log shows what was skipped.
            self.editor
                .log
                .push_back(kiln_protocol::commands::CommandLogEntry {
                    id: "write.options.autoformat".into(),
                    outcome: format!("skipped: {e}"),
                });
        }
    }

    fn after_char(&mut self, c: char) -> kiln_common::Result<()> {
        let o = self.features.autoformat.0;
        let at = self.head;
        let chars: Vec<char> = self.para_text(at.para).chars().collect();
        let off = at.offset.min(chars.len());
        let props = self.props_at(at);
        let code = is_code(&props);
        if o.markdown && c == ' ' && self.paragraph_shortcut(&chars, off)? {
            return Ok(());
        }
        if o.markdown
            && ((!code && matches!(c, '*' | '_' | '~')) || c == '`')
            && let Some((start, open, close, kind)) = inline_span(&chars, off)
        {
            return self.apply_inline(at.para, start, open, off, close, kind);
        }
        // Not in code, nor in inline code being typed (an open backtick before the caret).
        if code || chars[..off].iter().filter(|c| **c == '`').count() % 2 == 1 {
            return Ok(());
        }
        let before = |k: usize| off.checked_sub(k).and_then(|i| chars.get(i).copied());
        // Smart quotes.
        if o.smart_quotes && (c == '"' || c == '\'') && off > 0 {
            let prev = before(2);
            let opening = prev.is_none_or(|p| p.is_whitespace() || "([{—–-/\u{FFFC}".contains(p));
            let curly = match (c, opening) {
                ('"', true) => '“',
                ('"', false) => '”',
                ('\'', true) => '‘',
                _ => '’',
            };
            return self.replace_before(at, 1, &curly.to_string(), 0);
        }
        // Ellipsis.
        if o.ellipsis && c == '.' && before(2) == Some('.') && before(3) == Some('.') {
            return self.replace_before(at, 3, "…", 0);
        }
        // Dashes: word--w → word—w, word - w → word – w (on the first character after them).
        if o.dashes && !c.is_whitespace() && c != '-' {
            if before(2) == Some('-')
                && before(3) == Some('-')
                && before(4).is_some_and(|p| !p.is_whitespace() && p != '-')
            {
                return self.replace_before(at, 3, "—", 1);
            }
            if before(2) == Some(' ')
                && before(3) == Some('-')
                && before(4) == Some(' ')
                && before(5).is_some_and(|p| !p.is_whitespace())
            {
                return self.replace_before(at, 3, "–", 2);
            }
            // word -- word → word – word (Word's spaced double hyphen).
            if before(2) == Some(' ')
                && before(3) == Some('-')
                && before(4) == Some('-')
                && before(5) == Some(' ')
                && before(6).is_some_and(|p| !p.is_whitespace())
            {
                return self.replace_before(at, 4, "–", 2);
            }
        }
        // Fractions, after the character that ends the number.
        if o.fractions && (c.is_whitespace() || ",.;:!?)".contains(c)) && off >= 4 {
            let token: String = chars[off - 4..off - 1].iter().collect();
            if let Some(f) = fraction(&token)
                && boundary(before(5))
            {
                return self.replace_before(at, 4, &f.to_string(), 1);
            }
        }
        Ok(())
    }

    /// Replaces the `len` chars before the caret, keeping the last `keep` of them, with
    /// `with` (the replaced chars' formatting) as one undo step; the caret stays after `keep`.
    fn replace_before(
        &mut self,
        at: Pos,
        len: usize,
        with: &str,
        keep: usize,
    ) -> kiln_common::Result<()> {
        let start = at.offset - len;
        let n = len - keep;
        let props = self.props_at(Pos::new(at.para, start + 1));
        let new_len = with.chars().count();
        let after = Pos::new(at.para, start + new_len + keep);
        self.commit(
            Transaction::new(vec![
                Op::DeleteInline {
                    para: at.para,
                    offset: start,
                    len: n,
                },
                Op::InsertInline {
                    para: at.para,
                    offset: start,
                    fragment: Fragment::text(with, props),
                },
            ])
            .with_kind(EditKind::Other)
            .with_selection(Some(Range::caret(at)), Some(Range::caret(after))),
        )
    }

    /// `# `, `- `, `1. `, `> ` at the start of a paragraph.
    fn paragraph_shortcut(&mut self, chars: &[char], off: usize) -> kiln_common::Result<bool> {
        let prefix: String = chars[..off].iter().collect();
        enum Do {
            Style(String),
            Bullets,
            Numbering,
        }
        let action = match prefix.as_str() {
            "# " | "## " | "### " | "#### " | "##### " | "###### " => {
                Do::Style(format!("Heading{}", prefix.trim_end().len()))
            }
            "- " | "* " | "+ " => Do::Bullets,
            "1. " | "1) " => Do::Numbering,
            "> " => Do::Style("Quote".into()),
            _ => return Ok(false),
        };
        let at = self.head;
        // In a list already (the bullets would toggle it off) or a table cell's text: leave it.
        let p = self.paragraph(at.para)?;
        if p.props.numbering.is_some() && !matches!(action, Do::Style(_)) {
            return Ok(false);
        }
        let start = Pos::new(at.para, 0);
        let len = self.states.len();
        self.commit(
            Transaction::new(vec![Op::DeleteInline {
                para: at.para,
                offset: 0,
                len: off,
            }])
            .with_kind(EditKind::Other)
            .with_selection(Some(Range::caret(at)), Some(Range::caret(start))),
        )?;
        match action {
            Do::Style(id) => {
                self.run_editor(&WriteCommand::ApplyStyle { style_id: id })?;
            }
            Do::Bullets => {
                self.run_edit(&EditCommand::Bullets { glyph: None })?;
            }
            Do::Numbering => {
                self.run_edit(&EditCommand::Numbering { style: None })?;
            }
        }
        // Ctrl+Z brings the typed marker back in one step (as in Word).
        self.group_undo_since(len);
        Ok(true)
    }

    /// `**bold**` and friends: formats the content and removes the markers in one step.
    fn apply_inline(
        &mut self,
        para: kiln_common::NodeId,
        start: usize,
        open: usize,
        end: usize,
        close: usize,
        kind: InlineKind,
    ) -> kiln_common::Result<()> {
        let content = (start + open, end - close);
        let p = self.paragraph(para)?;
        let mut runs: Vec<RunSpan> = Vec::new();
        let mut at = 0usize;
        for r in p.content.runs() {
            let (rs, re) = (at, at + r.len);
            at = re;
            let (a, b) = (rs.max(content.0), re.min(content.1));
            if a >= b {
                continue;
            }
            let mut props = r.props.clone();
            match kind {
                InlineKind::Bold => props.bold = Some(true),
                InlineKind::Italic => props.italic = Some(true),
                InlineKind::Strike => props.strike = Some(true),
                InlineKind::Code => props = code_run_props(&props),
            }
            runs.push(RunSpan::new(b - a, props));
        }
        let caret = Pos::new(para, end - close - open);
        self.commit(
            Transaction::new(vec![
                Op::DeleteInline {
                    para,
                    offset: end - close,
                    len: close,
                },
                Op::SetRuns {
                    para,
                    offset: content.0,
                    runs,
                },
                Op::DeleteInline {
                    para,
                    offset: start,
                    len: open,
                },
            ])
            .with_kind(EditKind::Format)
            .with_selection(
                Some(Range::caret(Pos::new(para, end))),
                Some(Range::caret(caret)),
            ),
        )?;
        // Text typed next is plain again (Word ends the formatting at the marker).
        let plain = {
            let mut r = self.props_at(caret);
            match kind {
                InlineKind::Bold => r.bold = None,
                InlineKind::Italic => r.italic = None,
                InlineKind::Strike => r.strike = None,
                InlineKind::Code => r = RunProps::default(),
            }
            r
        };
        self.editor.pending = Some((caret, plain));
        Ok(())
    }

    /// After Enter: code lines are coloured; `---` rules and ```` ``` ```` code blocks in the
    /// paragraph just ended.
    fn after_enter(&mut self, markdown: bool) -> kiln_common::Result<()> {
        let at = self.head;
        let order = self.story_paragraphs(at.para);
        let Some(i) = order.iter().position(|p| *p == at.para) else {
            return Ok(());
        };
        let Some(prev) = i.checked_sub(1).map(|k| order[k]) else {
            return Ok(());
        };
        // Code blocks colour the line just ended (and an empty last line leaves the block).
        if self.code_after_enter(prev)? || !markdown {
            return Ok(());
        }
        let text = self.para_text(prev);
        let t = text.trim();
        let len = text.chars().count();
        if matches!(t, "---" | "***" | "___") {
            let p = self.paragraph(prev)?;
            let mut props = p.props.clone();
            let mut borders = props.borders.clone().unwrap_or_default();
            borders.bottom = Some(Border {
                space: Some(1),
                color: Some(kiln_doc::DocColor::auto()),
                ..Border::new(kiln_doc::st::BorderStyle::Single, 12)
            });
            props.borders = Some(borders);
            return self.commit(
                Transaction::new(vec![
                    Op::DeleteInline {
                        para: prev,
                        offset: 0,
                        len,
                    },
                    Op::SetParagraphProps {
                        para: prev,
                        props: Box::new(props),
                    },
                ])
                .with_kind(EditKind::Other)
                .with_selection(Some(Range::caret(at)), Some(Range::caret(at))),
            );
        }
        if let Some(lang) = t.strip_prefix("```")
            && !lang.contains('`')
            && lang
                .chars()
                .all(|c| c.is_alphanumeric() || "+#-_".contains(c))
            && self.para_len(at.para) == 0
        {
            let lang = (!lang.is_empty()).then(|| lang.to_owned());
            return self.code_block_from(prev, at.para, len, lang.as_deref());
        }
        Ok(())
    }
}

#[cfg(test)]
mod unit {
    use super::*;

    fn span(s: &str) -> Option<(usize, usize, usize, InlineKind)> {
        let c: Vec<char> = s.chars().collect();
        inline_span(&c, c.len())
    }

    #[test]
    fn inline_markdown_spans() {
        assert_eq!(span("a **kiln**"), Some((2, 2, 2, InlineKind::Bold)));
        assert_eq!(span("*glaze*"), Some((0, 1, 1, InlineKind::Italic)));
        assert_eq!(span("use `cone 10`"), Some((4, 1, 1, InlineKind::Code)));
        assert_eq!(span("~~old~~"), Some((0, 2, 2, InlineKind::Strike)));
        assert_eq!(span("__b__"), Some((0, 2, 2, InlineKind::Bold)));
        // Not spans: inside words, empty, space before the closer, a lone marker.
        assert_eq!(span("snake_case_"), None);
        assert_eq!(span("****"), None);
        assert_eq!(span("** x**"), None);
        assert_eq!(span("a *b *"), None);
        assert_eq!(span("*"), None);
        assert_eq!(span("2*3*"), None);
    }

    #[test]
    fn fractions_and_boundaries() {
        assert_eq!(fraction("1/2"), Some('½'));
        assert_eq!(fraction("7/8"), Some('⅞'));
        assert_eq!(fraction("2/4"), None);
        assert!(boundary(None) && boundary(Some(' ')) && !boundary(Some('a')));
    }
}
