// SPDX-License-Identifier: AGPL-3.0-or-later
//! Plain text files in Kiln Write (`.txt`): the encoding is detected when a file opens (a byte
//! order mark; UTF-16 without a mark when every other byte is zero; UTF-8 when the bytes are
//! valid UTF-8; else the likeliest of Windows-1252, Windows-1251, Shift-JIS and GB18030, scored on
//! how plausible the decoded text is) and kept with the line endings, so a text file saved again
//! comes back in the encoding it had. When the scores are close the detection is not
//! [`Detection::confident`] and the editor asks (Word's File Conversion dialog). Characters the
//! encoding cannot hold are saved as `?`, after a warning. A document that was not a text file
//! is saved as UTF-8 with Windows line endings, as Word does.
//!
//! Each line becomes a paragraph without space after it; a save writes every paragraph as a line
//! (list labels included, deleted text left out, table cells separated by tabs).

use kiln_doc::query::PlainTextOptions;
use kiln_doc::{Block, Document, Fragment, Inlines, Paragraph, RunProps};

/// A text encoding Kiln reads and writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Encoding {
    /// UTF-8, with or without a byte order mark.
    Utf8 {
        /// The file started with `EF BB BF`.
        bom: bool,
    },
    /// UTF-16 little endian (Windows' "Unicode").
    Utf16Le {
        /// The file started with `FF FE`.
        bom: bool,
    },
    /// UTF-16 big endian.
    Utf16Be {
        /// The file started with `FE FF`.
        bom: bool,
    },
    /// Windows-1252 (Western European, Windows' "ANSI").
    Windows1252,
    /// Windows-1251 (Cyrillic).
    Windows1251,
    /// Shift-JIS (Japanese).
    ShiftJis,
    /// GB18030 (Simplified Chinese, a superset of GBK and GB2312).
    Gb18030,
}

/// The encodings of the File Conversion list, in its order.
pub(crate) const CHOICES: [Encoding; 7] = [
    Encoding::Utf8 { bom: false },
    Encoding::Utf16Le { bom: false },
    Encoding::Utf16Be { bom: false },
    Encoding::Windows1252,
    Encoding::Windows1251,
    Encoding::ShiftJis,
    Encoding::Gb18030,
];

impl Encoding {
    /// The WHATWG name.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 { .. } => "utf-8",
            Encoding::Utf16Le { .. } => "utf-16le",
            Encoding::Utf16Be { .. } => "utf-16be",
            Encoding::Windows1252 => "windows-1252",
            Encoding::Windows1251 => "windows-1251",
            Encoding::ShiftJis => "shift_jis",
            Encoding::Gb18030 => "gb18030",
        }
    }

    /// Word's name for it.
    pub(crate) fn title(self) -> &'static str {
        match self {
            Encoding::Utf8 { .. } => "Unicode (UTF-8)",
            Encoding::Utf16Le { .. } => "Unicode",
            Encoding::Utf16Be { .. } => "Unicode (Big-Endian)",
            Encoding::Windows1252 => "Western European (Windows)",
            Encoding::Windows1251 => "Cyrillic (Windows)",
            Encoding::ShiftJis => "Japanese (Shift-JIS)",
            Encoding::Gb18030 => "Chinese Simplified (GB18030)",
        }
    }

    /// The encoding with a WHATWG name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        CHOICES
            .into_iter()
            .find(|e| e.name().eq_ignore_ascii_case(name))
    }

    fn legacy(self) -> Option<&'static encoding_rs::Encoding> {
        match self {
            Encoding::Windows1252 => Some(encoding_rs::WINDOWS_1252),
            Encoding::Windows1251 => Some(encoding_rs::WINDOWS_1251),
            Encoding::ShiftJis => Some(encoding_rs::SHIFT_JIS),
            Encoding::Gb18030 => Some(encoding_rs::GB18030),
            _ => None,
        }
    }
}

/// How a text file was written: its encoding and line ending.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TextFormat {
    /// The encoding.
    pub encoding: Encoding,
    /// The line ending (`"\r\n"`, `"\n"` or `"\r"`).
    pub newline: &'static str,
}

impl Default for TextFormat {
    fn default() -> Self {
        Self {
            encoding: Encoding::Utf8 { bom: false },
            newline: "\r\n",
        }
    }
}

fn utf16(bytes: &[u8], le: bool) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|&c| {
        if le {
            u16::from_le_bytes(c)
        } else {
            u16::from_be_bytes(c)
        }
    });
    char::decode_utf16(units)
        .map(|r| r.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// Whether bytes without a byte order mark look like UTF-16: mostly ASCII text where every
/// other byte is zero. Returns little endian (`Some(true)`) or big endian (`Some(false)`).
fn looks_utf16(bytes: &[u8]) -> Option<bool> {
    if bytes.len() < 4 || !bytes.len().is_multiple_of(2) {
        return None;
    }
    let sample = &bytes[..bytes.len().min(4096)];
    let pairs = sample.len() / 2;
    let (mut odd_zero, mut even_zero) = (0, 0);
    for c in sample.as_chunks::<2>().0 {
        if c[0] == 0 {
            even_zero += 1;
        }
        if c[1] == 0 {
            odd_zero += 1;
        }
    }
    // At least 40 % of the units are ASCII in the expected half and none in the other.
    if odd_zero * 10 >= pairs * 4 && even_zero * 20 <= pairs {
        Some(true)
    } else if even_zero * 10 >= pairs * 4 && odd_zero * 20 <= pairs {
        Some(false)
    } else {
        None
    }
}

/// Decodes bytes in an encoding (its byte order mark removed; invalid sequences become U+FFFD).
pub(crate) fn decode_as(bytes: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Utf8 { .. } => {
            let body = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
            String::from_utf8_lossy(body).into_owned()
        }
        Encoding::Utf16Le { .. } => utf16(bytes.strip_prefix(&[0xFF, 0xFE]).unwrap_or(bytes), true),
        Encoding::Utf16Be { .. } => {
            utf16(bytes.strip_prefix(&[0xFE, 0xFF]).unwrap_or(bytes), false)
        }
        legacy => legacy
            .legacy()
            .map(|e| e.decode_without_bom_handling(bytes).0.into_owned())
            .unwrap_or_default(),
    }
}

/// What the detection found.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Detection {
    /// The likeliest encoding.
    pub encoding: Encoding,
    /// Sure enough not to ask.
    pub confident: bool,
    /// Every encoding the bytes decode in, the likeliest first.
    pub ranked: Vec<Encoding>,
}

/// How plausible a decoded text is, per non-ASCII character (higher is likelier): letters of one
/// script in words of that script score, mixtures, controls, private use and replacement
/// characters cost.
fn plausibility(text: &str, encoding: Encoding) -> f32 {
    let chars: Vec<char> = text.chars().collect();
    let (mut score, mut n) = (0.0_f32, 0_u32);
    for (i, &c) in chars.iter().enumerate() {
        if c.is_ascii() {
            continue;
        }
        n += 1;
        let u = u32::from(c);
        let prev = i.checked_sub(1).map(|j| chars[j]);
        let next = chars.get(i + 1).copied();
        let ascii_letter = |o: Option<char>| o.is_some_and(|c| c.is_ascii_alphabetic());
        let cyr = |o: Option<char>| o.is_some_and(|c| ('\u{400}'..='\u{4FF}').contains(&c));
        score += match u {
            0xFFFD | 0x80..=0x9F | 0xE000..=0xF8FF => -4.0,
            // Kana: Japanese.
            0x3040..=0x30FF => 2.0,
            // Common ideographs.
            0x4E00..=0x9FFF => 1.0,
            // CJK punctuation and full-width forms.
            0x3000..=0x303F | 0xFF01..=0xFF5E => 1.0,
            // Half-width katakana: rare in real text, frequent in misread Chinese.
            0xFF61..=0xFF9F => -1.0,
            // Rare ideographs and Hangul from GB18030's four-byte area.
            0x3400..=0x4DBF | 0xAC00..=0xD7AF | 0x10000.. => -2.0,
            0x400..=0x4FF => {
                // Cyrillic in Cyrillic words; mixed with Latin letters it is misread Latin, and
                // capitals inside words are misread double-byte text.
                if c.is_uppercase() && prev.is_some_and(char::is_lowercase) {
                    -1.5
                } else if ascii_letter(prev) || ascii_letter(next) {
                    -1.0
                } else if cyr(prev) || cyr(next) {
                    1.0
                } else {
                    0.3
                }
            }
            0xC0..=0xFF => {
                // Accented Latin among ASCII letters; runs of them are misread Cyrillic.
                let latin1 =
                    |o: Option<char>| o.is_some_and(|c| ('\u{C0}'..='\u{FF}').contains(&c));
                if latin1(prev) && latin1(next) {
                    -1.0
                } else if ascii_letter(prev) || ascii_letter(next) {
                    1.0
                } else {
                    0.3
                }
            }
            // Typographic punctuation and symbols (– “ ” € ™ …).
            0x2000..=0x206F | 0x20AC | 0x2122 | 0xA0..=0xBF => 0.5,
            _ => -0.5,
        };
    }
    if n == 0 {
        return 1.0;
    }
    // Shift-JIS and GB18030 read each other's bytes; a small bias settles near ties the way
    // they are met in practice (Windows-1252 first, as Word assumes).
    let bias = match encoding {
        Encoding::Windows1252 => 0.05,
        _ => 0.0,
    };
    #[allow(clippy::cast_precision_loss)]
    let avg = score / n as f32;
    avg + bias
}

/// Detects the encoding of a text file.
pub(crate) fn detect(bytes: &[u8]) -> Detection {
    let sure = |encoding: Encoding| {
        let mut ranked = vec![encoding];
        ranked.extend(CHOICES.into_iter().filter(|e| e.name() != encoding.name()));
        Detection {
            encoding,
            confident: true,
            ranked,
        }
    };
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return sure(Encoding::Utf8 { bom: true });
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        return sure(Encoding::Utf16Le { bom: true });
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        return sure(Encoding::Utf16Be { bom: true });
    }
    if let Some(le) = looks_utf16(bytes) {
        return sure(if le {
            Encoding::Utf16Le { bom: false }
        } else {
            Encoding::Utf16Be { bom: false }
        });
    }
    if std::str::from_utf8(bytes).is_ok() {
        return sure(Encoding::Utf8 { bom: false });
    }
    let sample = &bytes[..bytes.len().min(64 * 1024)];
    let mut scored: Vec<(Encoding, f32)> = [
        Encoding::Windows1252,
        Encoding::Windows1251,
        Encoding::ShiftJis,
        Encoding::Gb18030,
    ]
    .into_iter()
    .filter_map(|e| {
        let enc = e.legacy()?;
        let (text, had_errors) = enc.decode_without_bom_handling(sample);
        // A multi-byte encoding the bytes are not valid in is out (the sample may cut the
        // last character: tolerate one error at its very end).
        let errors = text.matches('\u{FFFD}').count();
        let cut = sample.len() < bytes.len() && errors == 1 && text.ends_with('\u{FFFD}');
        if had_errors && matches!(e, Encoding::ShiftJis | Encoding::Gb18030) && !cut {
            return None;
        }
        Some((e, plausibility(&text, e)))
    })
    .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    let best = scored.first().map_or(-1.0, |s| s.1);
    let second = scored.get(1).map_or(-10.0, |s| s.1);
    let encoding = scored.first().map_or(Encoding::Windows1252, |s| s.0);
    let mut ranked: Vec<Encoding> = scored.iter().map(|s| s.0).collect();
    let rest: Vec<Encoding> = CHOICES
        .into_iter()
        .filter(|e| !ranked.contains(e))
        .collect();
    ranked.extend(rest);
    Detection {
        encoding,
        confident: best >= 0.5 && best - second >= 0.35,
        ranked,
    }
}

/// Decodes a text file, detecting its encoding.
pub(crate) fn decode(bytes: &[u8]) -> (String, Encoding) {
    let d = detect(bytes);
    (decode_as(bytes, d.encoding), d.encoding)
}

/// The line ending used most in `text` (Windows' when there is none).
fn newline_of(text: &str) -> &'static str {
    let crlf = text.matches("\r\n").count();
    let cr = text.matches('\r').count() - crlf;
    let lf = text.matches('\n').count() - crlf;
    if lf > crlf && lf >= cr {
        "\n"
    } else if cr > crlf && cr > lf {
        "\r"
    } else {
        "\r\n"
    }
}

/// A Write document from a text file, and how the file was written.
#[cfg(test)]
pub(crate) fn read(bytes: &[u8]) -> (Document, TextFormat) {
    let (text, encoding) = decode(bytes);
    from_text(&text, encoding)
}

/// A Write document from a text file read in an encoding.
pub(crate) fn read_as(bytes: &[u8], encoding: Encoding) -> (Document, TextFormat) {
    from_text(&decode_as(bytes, encoding), encoding)
}

/// The lines of a text file as paragraphs (ids from `id`).
pub(crate) fn paragraphs(text: &str, mut id: impl FnMut() -> kiln_common::NodeId) -> Vec<Block> {
    let normalised = text.replace("\r\n", "\n").replace('\r', "\n");
    // A final line ending does not start another paragraph.
    let body = normalised.strip_suffix('\n').unwrap_or(&normalised);
    let mut blocks = Vec::new();
    for line in body.split('\n') {
        let mut p = Paragraph::new(id());
        let clean: String = line
            .chars()
            .filter(|c| *c == '\t' || !c.is_control())
            .collect();
        if !clean.is_empty()
            && let Ok(content) = Inlines::from_fragment(Fragment::text(&clean, RunProps::default()))
        {
            p.content = content;
        }
        blocks.push(Block::Paragraph(p));
    }
    blocks
}

fn from_text(text: &str, encoding: Encoding) -> (Document, TextFormat) {
    let newline = newline_of(text);
    let mut doc = Document::new();
    // Lines of a text file sit one under the other, as in Word's plain text view.
    let ppr = &mut doc.styles.doc_defaults.ppr;
    let mut sp = ppr.spacing.unwrap_or_default();
    sp.after = Some(0);
    sp.line = Some(240);
    sp.line_rule = Some(kiln_doc::st::LineRule::Auto);
    ppr.spacing = Some(sp);
    if let Some(normal) = doc.styles.get("Normal").cloned() {
        let mut normal = normal;
        if let Some(s) = normal.ppr.as_mut().and_then(|p| p.spacing.as_mut()) {
            s.after = Some(0);
            s.line = Some(240);
            s.line_rule = Some(kiln_doc::st::LineRule::Auto);
        }
        let _ = doc.styles.add(normal);
    }
    let blocks = paragraphs(text, || doc.alloc_id());
    if let Some(s) = doc.sections.first_mut() {
        s.blocks = blocks;
    }
    doc.refresh_id_counter();
    (doc, TextFormat { encoding, newline })
}

/// The text a save writes.
fn file_text(doc: &Document) -> String {
    let text = doc.plain_text(PlainTextOptions {
        include_headers_footers: false,
        include_notes: false,
        accept_changes: true,
        list_labels: true,
    });
    let text: String = text
        .chars()
        .filter(|c| *c != kiln_doc::OBJECT_CHAR)
        .map(|c| if c == '\u{c}' { '\n' } else { c })
        .collect();
    text
}

/// Characters of the document an encoding cannot hold (in order of appearance, each once, at
/// most `max`).
pub(crate) fn unrepresentable(doc: &Document, encoding: Encoding, max: usize) -> Vec<char> {
    let Some(enc) = encoding.legacy() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let text = file_text(doc);
    let mut buf = [0_u8; 8];
    for c in text.chars() {
        if c.is_ascii() || out.contains(&c) {
            continue;
        }
        let mut e = enc.new_encoder();
        let (r, _, _) =
            e.encode_from_utf8_without_replacement(c.encode_utf8(&mut [0; 4]), &mut buf, true);
        if matches!(r, encoding_rs::EncoderResult::Unmappable(_)) {
            out.push(c);
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

/// Encodes text in a legacy encoding, characters it cannot hold as `?` (Word's substitution).
fn encode_legacy(text: &str, enc: &'static encoding_rs::Encoding) -> Vec<u8> {
    let mut e = enc.new_encoder();
    let mut out = Vec::with_capacity(text.len() + 16);
    let mut rest = text;
    let mut buf = vec![0_u8; 4096];
    loop {
        let (r, read, written) = e.encode_from_utf8_without_replacement(rest, &mut buf, true);
        out.extend_from_slice(&buf[..written]);
        rest = &rest[read..];
        match r {
            encoding_rs::EncoderResult::InputEmpty => break,
            encoding_rs::EncoderResult::OutputFull => {}
            encoding_rs::EncoderResult::Unmappable(_) => out.push(b'?'),
        }
    }
    out
}

/// The document as a text file, in the encoding it was opened with (characters it cannot hold
/// become `?`; the editor warns first).
pub(crate) fn write(doc: &Document, format: TextFormat) -> Vec<u8> {
    let text = file_text(doc);
    let mut lines: Vec<&str> = text.split('\n').collect();
    while lines.len() > 1 && lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut out = lines.join(format.newline);
    if !out.is_empty() {
        out.push_str(format.newline);
    }
    match format.encoding {
        Encoding::Utf8 { bom } => {
            let mut b = Vec::with_capacity(out.len() + 3);
            if bom {
                b.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            b.extend_from_slice(out.as_bytes());
            b
        }
        Encoding::Utf16Le { bom } | Encoding::Utf16Be { bom } => {
            let le = matches!(format.encoding, Encoding::Utf16Le { .. });
            let mut b = Vec::with_capacity(out.len() * 2 + 2);
            if bom {
                b.extend_from_slice(if le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
            }
            for u in out.encode_utf16() {
                b.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
            }
            b
        }
        legacy => legacy
            .legacy()
            .map(|e| encode_legacy(&out, e))
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(doc: &Document) -> Vec<String> {
        doc.body_paragraphs()
            .iter()
            .map(|p| p.content.plain_text(false))
            .collect()
    }

    #[test]
    fn detects_utf8_utf16_and_windows_1252() {
        let (t, e) = decode("Grüße\r\n".as_bytes());
        assert_eq!(
            (t.as_str(), e),
            ("Grüße\r\n", Encoding::Utf8 { bom: false })
        );
        let (t, e) = decode(&[0xEF, 0xBB, 0xBF, b'a']);
        assert_eq!((t.as_str(), e), ("a", Encoding::Utf8 { bom: true }));
        let le: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("Ünï".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(
            decode(&le),
            ("Ünï".to_owned(), Encoding::Utf16Le { bom: true })
        );
        let be: Vec<u8> = "plain text"
            .encode_utf16()
            .flat_map(u16::to_be_bytes)
            .collect();
        assert_eq!(
            decode(&be),
            ("plain text".to_owned(), Encoding::Utf16Be { bom: false })
        );
        // "café – 5 €" in Windows-1252.
        let ansi = [b'c', b'a', b'f', 0xE9, b' ', 0x96, b' ', b'5', b' ', 0x80];
        assert_eq!(
            decode(&ansi),
            ("café – 5 €".to_owned(), Encoding::Windows1252)
        );
    }

    #[test]
    fn lines_become_paragraphs_and_save_back_in_their_encoding() {
        let src: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain(
                "One\r\n\tTwo\r\n\r\nFour\r\n"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        let (doc, format) = read(&src);
        assert_eq!(lines(&doc), ["One", "\tTwo", "", "Four"]);
        assert_eq!(format.newline, "\r\n");
        assert!(doc.validate().is_ok());
        assert_eq!(write(&doc, format), src);

        let (doc, format) = read(b"a\nb\n");
        assert_eq!(format.newline, "\n");
        assert_eq!(write(&doc, format), b"a\nb\n");
        // Windows-1252 keeps its encoding: a character it cannot hold is saved as `?`.
        let (mut doc, format) = read(&[b'c', b'a', b'f', 0xE9]);
        assert_eq!(format.encoding, Encoding::Windows1252);
        assert_eq!(write(&doc, format), [b'c', b'a', b'f', 0xE9, b'\r', b'\n']);
        if let Some(Block::Paragraph(p)) = doc.sections[0].blocks.first_mut() {
            p.content = Inlines::from_fragment(Fragment::text("café 日本", RunProps::default()))
                .expect("fragment");
        }
        assert_eq!(unrepresentable(&doc, format.encoding, 20), ['日', '本']);
        assert_eq!(write(&doc, format), b"caf\xE9 ??\r\n");
    }

    fn enc(text: &str, e: &'static encoding_rs::Encoding) -> Vec<u8> {
        e.encode(text).0.into_owned()
    }

    #[test]
    fn detects_legacy_encodings_with_confidence() {
        let cases: [(&str, &'static encoding_rs::Encoding, Encoding); 4] = [
            (
                "Le café est très chaud, déjà prêt à côté du four — « voilà ».",
                encoding_rs::WINDOWS_1252,
                Encoding::Windows1252,
            ),
            (
                "Печь для обжига керамики нагревается медленно и равномерно.",
                encoding_rs::WINDOWS_1251,
                Encoding::Windows1251,
            ),
            (
                "陶芸の窯はゆっくりと均一に温まります。釉薬が溶けるまで待ちます。",
                encoding_rs::SHIFT_JIS,
                Encoding::ShiftJis,
            ),
            (
                "陶瓷窑炉缓慢而均匀地升温，釉料熔化后再慢慢冷却。",
                encoding_rs::GB18030,
                Encoding::Gb18030,
            ),
        ];
        for (text, e, want) in cases {
            let bytes = enc(text, e);
            let d = detect(&bytes);
            assert_eq!(d.encoding, want, "{text}");
            assert!(d.confident, "{text}: {d:?}");
            assert_eq!(decode_as(&bytes, d.encoding), text);
            assert_eq!(d.ranked.len(), CHOICES.len());
        }
        // UTF-8 and UTF-16 without marks are sure.
        assert!(detect("Привет".as_bytes()).confident);
        // Two high bytes say little: the editor asks.
        let d = detect(&[b'A', 0xC1, 0xE0, b' ', b'b']);
        assert!(!d.confident, "{d:?}");
    }

    #[test]
    fn a_text_file_has_no_space_after_its_lines() {
        let (doc, _) = read(b"x\ny");
        let p = doc.body_paragraphs()[0];
        let r = doc.resolve_paragraph(p);
        assert_eq!(r.spacing.after, 0);
    }
}
