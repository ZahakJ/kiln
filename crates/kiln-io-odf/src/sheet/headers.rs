// SPDX-License-Identifier: AGPL-3.0-or-later
//! Sheet headers and footers: the `style:header`/`style:footer` of a sheet's master page (with
//! `style:region-left`, `-center`, `-right`) ⇄ Excel's `&L…&C…&R…` codes.
//!
//! | Excel | ODF |
//! |---|---|
//! | `&P`, `&N` | `text:page-number`, `text:page-count` |
//! | `&D`, `&T` | `text:date`, `text:time` |
//! | `&A` | `text:sheet-name` |
//! | `&F`, `&Z&F` | `text:file-name` (`name-and-extension`, `full`) |
//! | `&&` | `&` |
//! | font, size and style codes (`&"Arial,Bold"`, `&12`, `&B`) | not kept |
//!
//! The odd pages' header is `style:header`, the even pages' `style:header-left`, the first page's
//! `style:header-first`.

use kiln_ooxml::xml::{XmlElement, XmlNode};
use kiln_sheet::meta::HeaderFooter;

use crate::el::El;
use crate::xml::kid;

/// The Excel code of the text of a region (its paragraphs joined by line breaks).
fn region_code(e: &XmlElement) -> String {
    fn walk(e: &XmlElement, out: &mut String) {
        for n in &e.children {
            match n {
                XmlNode::Text(t) => {
                    for c in t.text.chars() {
                        if c == '&' {
                            out.push_str("&&");
                        } else if !c.is_control() {
                            out.push(c);
                        }
                    }
                }
                XmlNode::Element(c) => match c.name.as_str() {
                    "text:page-number" => out.push_str("&P"),
                    "text:page-count" => out.push_str("&N"),
                    "text:date" => out.push_str("&D"),
                    "text:time" => out.push_str("&T"),
                    "text:sheet-name" => out.push_str("&A"),
                    "text:file-name" => {
                        if c.attr("text:display") == Some("full") {
                            out.push_str("&Z");
                        }
                        out.push_str("&F");
                    }
                    "text:title" => out.push_str("&F"),
                    "text:s" => {
                        let n = c
                            .attr("text:c")
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1)
                            .clamp(1, 100);
                        out.push_str(&" ".repeat(n));
                    }
                    "text:tab" => out.push(' '),
                    "text:line-break" => out.push('\n'),
                    _ => walk(c, out),
                },
                _ => {}
            }
        }
    }
    let mut parts = Vec::new();
    for p in e
        .elements()
        .filter(|p| p.name == "text:p" || p.name == "text:h")
    {
        let mut s = String::new();
        walk(p, &mut s);
        parts.push(s);
    }
    parts.join("\n")
}

/// The Excel code of a header or footer element, `None` when it is hidden or empty.
fn code_of(e: &XmlElement) -> Option<String> {
    if e.attr("style:display") == Some("false") {
        return None;
    }
    let mut out = String::new();
    let regions = [
        ("style:region-left", "&L"),
        ("style:region-center", "&C"),
        ("style:region-right", "&R"),
    ];
    if regions.iter().any(|(n, _)| kid(e, n).is_some()) {
        for (name, code) in regions {
            if let Some(r) = kid(e, name) {
                let t = region_code(r);
                if !t.is_empty() {
                    out.push_str(code);
                    out.push_str(&t);
                }
            }
        }
    } else {
        // A header without regions shows its text centred.
        let t = region_code(e);
        if !t.is_empty() {
            out.push_str("&C");
            out.push_str(&t);
        }
    }
    (!out.is_empty()).then_some(out)
}

/// The headers and footers of a sheet's master page.
#[must_use]
pub fn read(mp: &XmlElement) -> HeaderFooter {
    let get = |n: &str| kid(mp, n).and_then(code_of);
    let even_header = get("style:header-left");
    let even_footer = get("style:footer-left");
    let first_header = get("style:header-first").or_else(|| get("loext:header-first"));
    let first_footer = get("style:footer-first").or_else(|| get("loext:footer-first"));
    HeaderFooter {
        odd_header: get("style:header"),
        odd_footer: get("style:footer"),
        different_odd_even: even_header.is_some() || even_footer.is_some(),
        different_first: first_header.is_some() || first_footer.is_some(),
        even_header,
        even_footer,
        first_header,
        first_footer,
        scale_with_doc: true,
        align_with_margins: true,
    }
}

/// One region's paragraphs from Excel code text (`&P`, `&&`, …; formatting codes dropped).
fn region_el(name: &str, code: &str) -> El {
    let mut region = El::new(name);
    for line in code.split('\n') {
        let mut p = El::new("text:p");
        let mut text = String::new();
        let mut chars = line.chars().peekable();
        let flush = |p: &mut El, text: &mut String| {
            if !text.is_empty() {
                p.children.push(El::text_node(std::mem::take(text)));
            }
        };
        while let Some(c) = chars.next() {
            if c != '&' {
                text.push(c);
                continue;
            }
            match chars.next() {
                Some('&') => text.push('&'),
                Some(f @ ('P' | 'N' | 'D' | 'T' | 'A' | 'F' | 'Z')) => {
                    flush(&mut p, &mut text);
                    let el = match f {
                        'P' => El::new("text:page-number").with_text("1"),
                        'N' => El::new("text:page-count").with_text("1"),
                        'D' => El::new("text:date").with_text(""),
                        'T' => El::new("text:time").with_text(""),
                        'A' => El::new("text:sheet-name").with_text("???"),
                        'Z' => {
                            // `&Z&F`: the full path.
                            if chars.peek() == Some(&'&') {
                                chars.next();
                                chars.next();
                            }
                            El::new("text:file-name")
                                .with("text:display", "full")
                                .with_text("???")
                        }
                        _ => El::new("text:file-name")
                            .with("text:display", "name-and-extension")
                            .with_text("???"),
                    };
                    p.children.push(el);
                }
                Some('"') => {
                    // A font name: up to the closing quote.
                    for c in chars.by_ref() {
                        if c == '"' {
                            break;
                        }
                    }
                }
                Some(d) if d.is_ascii_digit() => {
                    // A font size.
                    while chars.peek().is_some_and(char::is_ascii_digit) {
                        chars.next();
                    }
                }
                Some('K') => {
                    // A colour: six hex digits or a theme code.
                    for _ in 0..6 {
                        chars.next();
                    }
                }
                // Bold, italic, underline, strike, super/subscript, picture: not kept.
                _ => {}
            }
        }
        flush(&mut p, &mut text);
        region.children.push(p);
    }
    region
}

/// A header or footer element from Excel code (`None` for an empty code).
fn element(name: &str, code: Option<&str>) -> Option<El> {
    let code = code.filter(|c| !c.is_empty())?;
    let mut el = El::new(name);
    // Split at the section codes (`&&` is a literal ampersand, not a code).
    let mut sections: [String; 3] = Default::default();
    let mut cur = 1usize;
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.peek().copied() {
                Some('L') => {
                    chars.next();
                    cur = 0;
                    continue;
                }
                Some('C') => {
                    chars.next();
                    cur = 1;
                    continue;
                }
                Some('R') => {
                    chars.next();
                    cur = 2;
                    continue;
                }
                Some('&') => {
                    chars.next();
                    sections[cur].push_str("&&");
                    continue;
                }
                _ => {}
            }
        }
        sections[cur].push(c);
    }
    for (i, name) in [
        "style:region-left",
        "style:region-center",
        "style:region-right",
    ]
    .iter()
    .enumerate()
    {
        if !sections[i].is_empty() {
            el.children.push(region_el(name, &sections[i]));
        }
    }
    (!el.children.is_empty()).then_some(el)
}

/// The header and footer elements of a master page.
#[must_use]
pub fn write(hf: &HeaderFooter) -> Vec<El> {
    let mut out = Vec::new();
    let odd_even = hf.different_odd_even;
    let first = hf.different_first;
    for (name, code) in [
        ("style:header", hf.odd_header.as_deref()),
        (
            "style:header-left",
            odd_even.then_some(hf.even_header.as_deref()).flatten(),
        ),
        (
            "style:header-first",
            first.then_some(hf.first_header.as_deref()).flatten(),
        ),
        ("style:footer", hf.odd_footer.as_deref()),
        (
            "style:footer-left",
            odd_even.then_some(hf.even_footer.as_deref()).flatten(),
        ),
        (
            "style:footer-first",
            first.then_some(hf.first_footer.as_deref()).flatten(),
        ),
    ] {
        if let Some(e) = element(name, code) {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::{XmlOut, parse_part};

    fn round_trip(hf: &HeaderFooter) -> HeaderFooter {
        let mut mp = El::new("style:master-page").with("style:name", "P");
        mp.children.extend(write(hf));
        let mut w = XmlOut::new();
        w.root("office:master-styles", &["office", "style", "text"]);
        mp.write(&mut w);
        let root = parse_part("styles.xml", &w.finish()).unwrap();
        read(kid(&root, "style:master-page").unwrap())
    }

    #[test]
    fn codes_round_trip() {
        let hf = HeaderFooter {
            odd_header: Some("&C&F — &A".into()),
            odd_footer: Some("&LConfidential &&Co&RPage &P of &N".into()),
            even_header: Some("&L&D &T".into()),
            different_odd_even: true,
            first_footer: Some("&C&Z&F".into()),
            different_first: true,
            scale_with_doc: true,
            align_with_margins: true,
            ..HeaderFooter::default()
        };
        assert_eq!(round_trip(&hf), hf);
    }

    #[test]
    fn formatting_codes_are_dropped() {
        let hf = HeaderFooter {
            odd_header: Some("&C&\"Arial,Bold\"&14&BTitle&B".into()),
            scale_with_doc: true,
            align_with_margins: true,
            ..HeaderFooter::default()
        };
        assert_eq!(round_trip(&hf).odd_header.as_deref(), Some("&CTitle"));
    }
}
