// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text summaries of a layout for snapshot tests: the page count and the first and last words of
//! every page's body (what a reader compares when checking pagination against Word).

use std::fmt::Write;

use crate::engine::DocLayout;
use crate::page::{Page, StoryKind};

/// The words of a page's body text in reading order.
#[must_use]
pub fn page_words(page: &Page) -> Vec<String> {
    let mut words = Vec::new();
    for l in page.body_lines() {
        if l.story != StoryKind::Body {
            continue;
        }
        let Some(info) = l.para.lines.get(l.line as usize) else {
            continue;
        };
        let text = l.para.src.map.text();
        let start = info
            .bytes
            .start
            .max(l.para.src.label_len.min(info.bytes.end));
        let s = text.get(start..info.bytes.end).unwrap_or("");
        words.extend(
            s.split(|c: char| c.is_whitespace() || c == '\u{200B}')
                .filter(|w| !w.is_empty())
                .map(str::to_owned),
        );
    }
    words
}

/// `pages: N`, then one line per page: its number and the first and last words of its body.
#[must_use]
pub fn summary(layout: &DocLayout) -> String {
    let mut out = String::new();
    let pages = layout.pages();
    let _ = writeln!(out, "pages: {}", pages.len());
    for (i, p) in pages.iter().enumerate() {
        let words = page_words(p);
        let first = words.first().map_or("", String::as_str);
        let last = words.last().map_or("", String::as_str);
        let _ = writeln!(
            out,
            "{:>3} [s{} #{}{}] {} … {} ({} words)",
            i + 1,
            p.section + 1,
            p.number_text,
            if p.blank { " blank" } else { "" },
            first,
            last,
            words.len()
        );
    }
    out
}
