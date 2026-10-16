// SPDX-License-Identifier: AGPL-3.0-or-later
//! A small, strict XML tokenizer for the hot path: the content of `sheetData`, which holds
//! nearly all of a large workbook's bytes. It finds tags and text with `memchr`, borrows names,
//! attribute values and text from the part, and resolves references only where they occur, so a
//! million cells scan in a fraction of the time a general reader takes.
//!
//! It is strict where it matters: end tags must match their start tags, attribute values must
//! be quoted, only the five predefined entities and character references are accepted, DTDs are
//! refused, comments and processing instructions are skipped, CDATA is text. Anything else is an
//! error, never a panic.

use std::borrow::Cow;

use memchr::{memchr, memchr3, memmem};

/// A token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Tok<'a> {
    /// A start tag (`empty` for `<x/>`); `attrs` is the raw attribute text.
    Start {
        name: &'a str,
        attrs: &'a str,
        empty: bool,
    },
    /// An end tag.
    End { name: &'a str },
    /// Character data, raw (references unresolved).
    Text(&'a str),
    /// A CDATA section's content (taken literally).
    CData(&'a str),
    /// End of input.
    Eof,
}

/// The local part of a qualified name.
pub(crate) fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

/// The tokenizer over `text[pos..]`.
pub(crate) struct Scanner<'a> {
    text: &'a str,
    pos: usize,
}

fn is_name_end(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>')
}

impl<'a> Scanner<'a> {
    pub(crate) fn new(text: &'a str, pos: usize) -> Scanner<'a> {
        Scanner {
            text,
            pos: pos.min(text.len()),
        }
    }

    /// The byte offset of the next token.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// The next token.
    pub(crate) fn next(&mut self) -> Result<Tok<'a>, String> {
        let b = self.text.as_bytes();
        loop {
            if self.pos >= b.len() {
                return Ok(Tok::Eof);
            }
            if b[self.pos] != b'<' {
                let end = memchr(b'<', &b[self.pos..]).map_or(b.len(), |i| self.pos + i);
                let t = &self.text[self.pos..end];
                self.pos = end;
                return Ok(Tok::Text(t));
            }
            let rest = &b[self.pos..];
            match rest.get(1) {
                Some(b'/') => {
                    let close = memchr(b'>', rest).ok_or("an end tag is not closed")?;
                    let name = self.text[self.pos + 2..self.pos + close].trim_end();
                    if name.is_empty() {
                        return Err("an end tag has no name".into());
                    }
                    self.pos += close + 1;
                    return Ok(Tok::End { name });
                }
                Some(b'!') => {
                    if rest.starts_with(b"<!--") {
                        let end =
                            memmem::find(&rest[4..], b"-->").ok_or("a comment is not closed")?;
                        self.pos += 4 + end + 3;
                        continue;
                    }
                    if rest.starts_with(b"<![CDATA[") {
                        let end = memmem::find(&rest[9..], b"]]>")
                            .ok_or("a CDATA section is not closed")?;
                        let t = &self.text[self.pos + 9..self.pos + 9 + end];
                        self.pos += 9 + end + 3;
                        return Ok(Tok::CData(t));
                    }
                    return Err("DTDs and declarations are not allowed".into());
                }
                Some(b'?') => {
                    let end = memmem::find(rest, b"?>")
                        .ok_or("a processing instruction is not closed")?;
                    self.pos += end + 2;
                    continue;
                }
                Some(_) => {
                    // Find the closing `>` outside quoted attribute values.
                    let mut i = 1usize;
                    let close = loop {
                        let k = i + memchr3(b'>', b'"', b'\'', &rest[i..])
                            .ok_or("a tag is not closed")?;
                        match rest[k] {
                            b'>' => break k,
                            q => {
                                let end = memchr(q, &rest[k + 1..])
                                    .ok_or("an attribute value is not closed")?;
                                i = k + 1 + end + 1;
                            }
                        }
                    };
                    let empty = rest[close - 1] == b'/';
                    let inner_end = if empty { close - 1 } else { close };
                    let name_len = rest[1..inner_end]
                        .iter()
                        .position(|&c| is_name_end(c))
                        .unwrap_or(inner_end - 1);
                    if name_len == 0 {
                        return Err("a tag has no name".into());
                    }
                    let name = &self.text[self.pos + 1..self.pos + 1 + name_len];
                    let attrs = &self.text[self.pos + 1 + name_len..self.pos + inner_end];
                    self.pos += close + 1;
                    return Ok(Tok::Start { name, attrs, empty });
                }
                None => return Err("a tag is not closed".into()),
            }
        }
    }

    /// Skips the content of an element whose start tag was just read (well-formedness checked).
    pub(crate) fn skip(&mut self, name: &str) -> Result<(), String> {
        let mut stack: Vec<&'a str> = Vec::new();
        loop {
            match self.next()? {
                Tok::Start { name: n, empty, .. } => {
                    if !empty {
                        if stack.len() > 256 {
                            return Err("elements nested too deeply".into());
                        }
                        stack.push(n);
                    }
                }
                Tok::End { name: n } => match stack.pop() {
                    Some(open) if open == n => {}
                    Some(open) => return Err(format!("</{n}> closes <{open}>")),
                    None if n == name => return Ok(()),
                    None => return Err(format!("</{n}> closes <{name}>")),
                },
                Tok::Eof => return Err(format!("<{name}> is not closed")),
                Tok::Text(_) | Tok::CData(_) => {}
            }
        }
    }

    /// The text content of an element whose start tag was just read, references resolved and
    /// line ends normalised; nested markup is skipped (its text kept).
    pub(crate) fn text_of(&mut self, name: &str) -> Result<Cow<'a, str>, String> {
        let mut out: Option<Cow<'a, str>> = None;
        let mut stack: Vec<&'a str> = Vec::new();
        loop {
            let piece = match self.next()? {
                Tok::Text(t) => unescape(t)?,
                Tok::CData(t) => normalize_eol(t),
                Tok::Start { name: n, empty, .. } => {
                    if !empty {
                        stack.push(n);
                    }
                    continue;
                }
                Tok::End { name: n } => match stack.pop() {
                    Some(open) if open == n => continue,
                    Some(open) => return Err(format!("</{n}> closes <{open}>")),
                    None if n == name => return Ok(out.unwrap_or(Cow::Borrowed(""))),
                    None => return Err(format!("</{n}> closes <{name}>")),
                },
                Tok::Eof => return Err(format!("<{name}> is not closed")),
            };
            out = Some(match out {
                None => piece,
                Some(prev) => Cow::Owned(prev.into_owned() + &piece),
            });
        }
    }

    /// The rest of an element whose start tag was just read, as a tree (names without
    /// prefixes; text with references resolved).
    pub(crate) fn subtree(
        &mut self,
        name: &str,
        attrs: &str,
    ) -> Result<kiln_ooxml::xml::XmlElement, String> {
        use kiln_ooxml::xml::{XmlElement, XmlNode};
        let element = |n: &str, a: &str| -> Result<XmlElement, String> {
            let mut e = XmlElement::new(local(n));
            for kv in Attrs::new(a) {
                let (k, v) = kv?;
                e.attrs.set(k, unescape_attr(v)?.into_owned());
            }
            Ok(e)
        };
        let mut stack = vec![(name.to_owned(), element(name, attrs)?)];
        loop {
            match self.next()? {
                Tok::Start {
                    name: n,
                    attrs: a,
                    empty,
                } => {
                    let e = element(n, a)?;
                    if empty {
                        if let Some((_, top)) = stack.last_mut() {
                            top.children.push(XmlNode::Element(e));
                        }
                    } else {
                        if stack.len() > 256 {
                            return Err("elements nested too deeply".into());
                        }
                        stack.push((n.to_owned(), e));
                    }
                }
                Tok::End { name: n } => {
                    let Some((open, done)) = stack.pop() else {
                        return Err("unbalanced end tag".into());
                    };
                    if open != n {
                        return Err(format!("</{n}> closes <{open}>"));
                    }
                    match stack.last_mut() {
                        Some((_, top)) => top.children.push(XmlNode::Element(done)),
                        None => return Ok(done),
                    }
                }
                Tok::Text(t) | Tok::CData(t) => {
                    let t = unescape(t)?;
                    if let Some((_, top)) = stack.last_mut() {
                        if let Some(XmlNode::Text(last)) = top.children.last_mut() {
                            last.text.push_str(&t);
                        } else {
                            top.children.push(XmlNode::text(t.into_owned()));
                        }
                    }
                }
                Tok::Eof => return Err(format!("<{name}> is not closed")),
            }
        }
    }
}

/// Attributes of a start tag's raw attribute text.
pub(crate) struct Attrs<'a> {
    s: &'a str,
    pos: usize,
}

impl<'a> Attrs<'a> {
    pub(crate) fn new(s: &'a str) -> Attrs<'a> {
        Attrs { s, pos: 0 }
    }
}

impl<'a> Iterator for Attrs<'a> {
    /// (name, raw value).
    type Item = Result<(&'a str, &'a str), String>;

    fn next(&mut self) -> Option<Self::Item> {
        let b = self.s.as_bytes();
        while self.pos < b.len() && b[self.pos].is_ascii_whitespace() {
            self.pos += 1;
        }
        if self.pos >= b.len() {
            return None;
        }
        let start = self.pos;
        let eq = match memchr(b'=', &b[start..]) {
            Some(i) => start + i,
            None => {
                self.pos = b.len();
                return Some(Err("an attribute has no value".into()));
            }
        };
        let name = self.s[start..eq].trim();
        let mut i = eq + 1;
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        let Some(&q) = b.get(i) else {
            self.pos = b.len();
            return Some(Err("an attribute has no value".into()));
        };
        if q != b'"' && q != b'\'' {
            self.pos = b.len();
            return Some(Err("an attribute value is not quoted".into()));
        }
        let Some(end) = memchr(q, &b[i + 1..]) else {
            self.pos = b.len();
            return Some(Err("an attribute value is not closed".into()));
        };
        let value = &self.s[i + 1..i + 1 + end];
        self.pos = i + 1 + end + 1;
        if name.is_empty() || name.contains(|c: char| c.is_whitespace()) {
            return Some(Err("an attribute has no name".into()));
        }
        Some(Ok((name, value)))
    }
}

fn normalize_eol(t: &str) -> Cow<'_, str> {
    if !t.contains('\r') {
        return Cow::Borrowed(t);
    }
    Cow::Owned(t.replace("\r\n", "\n").replace('\r', "\n"))
}

fn resolve_entity(e: &str) -> Result<char, String> {
    Ok(match e {
        "lt" => '<',
        "gt" => '>',
        "amp" => '&',
        "quot" => '"',
        "apos" => '\'',
        _ => {
            let code = if let Some(h) = e.strip_prefix("#x").or_else(|| e.strip_prefix("#X")) {
                u32::from_str_radix(h, 16).ok()
            } else if let Some(d) = e.strip_prefix('#') {
                d.parse::<u32>().ok()
            } else {
                return Err(format!("unknown entity &{e};"));
            };
            code.and_then(char::from_u32)
                .filter(|c| *c != '\0')
                .ok_or_else(|| format!("bad character reference &{e};"))?
        }
    })
}

fn unescape_with(t: &str, attr: bool) -> Result<Cow<'_, str>, String> {
    let needs = |c: u8| c == b'&' || c == b'\r' || (attr && (c == b'\n' || c == b'\t'));
    if !t.bytes().any(needs) {
        return Ok(Cow::Borrowed(t));
    }
    let mut out = String::with_capacity(t.len());
    let mut chars = t.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '&' => {
                let rest = &t[i + 1..];
                let end = rest.find(';').ok_or("an entity reference is not closed")?;
                out.push(resolve_entity(&rest[..end])?);
                for _ in 0..=end {
                    chars.next();
                }
            }
            '\r' => {
                if chars.peek().is_some_and(|(_, n)| *n == '\n') {
                    chars.next();
                }
                out.push(if attr { ' ' } else { '\n' });
            }
            '\n' | '\t' if attr => out.push(' '),
            c => out.push(c),
        }
    }
    Ok(Cow::Owned(out))
}

/// Character data with references resolved and line ends normalised.
pub(crate) fn unescape(t: &str) -> Result<Cow<'_, str>, String> {
    unescape_with(t, false)
}

/// An attribute value, normalised as XML 1.0 says (references resolved, white space → spaces).
pub(crate) fn unescape_attr(t: &str) -> Result<Cow<'_, str>, String> {
    unescape_with(t, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(s: &str) -> Result<Vec<Tok<'_>>, String> {
        let mut sc = Scanner::new(s, 0);
        let mut v = Vec::new();
        loop {
            let t = sc.next()?;
            if t == Tok::Eof {
                return Ok(v);
            }
            v.push(t);
        }
    }

    #[test]
    fn tokens_and_attributes() {
        let t = toks(r#"<row r="1" a='x>y'><c r="A1"/><!-- c --><?pi x?><v>1 &amp; 2</v><![CDATA[<raw>]]></row>"#)
            .expect("tokens");
        assert_eq!(
            t[0],
            Tok::Start {
                name: "row",
                attrs: r#" r="1" a='x>y'"#,
                empty: false
            }
        );
        let attrs: Vec<_> = Attrs::new(r#" r="1" a='x>y'"#)
            .collect::<Result<_, _>>()
            .expect("attrs");
        assert_eq!(attrs, vec![("r", "1"), ("a", "x>y")]);
        assert!(matches!(
            t[1],
            Tok::Start {
                name: "c",
                empty: true,
                ..
            }
        ));
        assert_eq!(t[3], Tok::Text("1 &amp; 2"));
        assert_eq!(t[5], Tok::CData("<raw>"));
        assert_eq!(unescape("1 &amp; &#x41;&#66;").expect("text"), "1 & AB");
        assert_eq!(unescape_attr("a&#10;b\tc").expect("attr"), "a\nb c");
        assert_eq!(unescape("a\r\nb\rc").expect("eol"), "a\nb\nc");
    }

    #[test]
    fn malformed_input_is_an_error() {
        for bad in [
            "<a",
            "<a b=\"x>",
            "<!DOCTYPE x>",
            "<a b=x/>",
            "<!-- open",
            "<a>&nope;</a>",
            "<a>&#0;</a>",
            "</>",
        ] {
            let mut sc = Scanner::new(bad, 0);
            let mut failed = false;
            for _ in 0..10 {
                match sc.next() {
                    Err(_) => {
                        failed = true;
                        break;
                    }
                    Ok(Tok::Text(t)) => {
                        if unescape(t).is_err() {
                            failed = true;
                            break;
                        }
                    }
                    Ok(Tok::Start { attrs, .. }) => {
                        if Attrs::new(attrs).any(|a| a.is_err()) {
                            failed = true;
                            break;
                        }
                    }
                    Ok(Tok::Eof) => break,
                    Ok(_) => {}
                }
            }
            assert!(failed, "{bad}");
        }
        let mut sc = Scanner::new("<v>1</c>", 0);
        let _ = sc.next();
        assert!(sc.text_of("v").is_err());
        let mut sc = Scanner::new("<is><r><t>a</t></r></is>", 0);
        let _ = sc.next();
        let e = sc.subtree("is", "").expect("tree");
        assert_eq!(e.text(), "a");
    }
}
