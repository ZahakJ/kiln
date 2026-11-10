// SPDX-License-Identifier: AGPL-3.0-or-later
//! Delimited text: RFC 4180 records (quoted fields with doubled quotes, line breaks inside
//! quotes, CRLF, LF or CR line endings), tolerant of what spreadsheets meet in practice (a quote
//! inside an unquoted field is a character; text after a closing quote is kept), and delimiter
//! sniffing.

/// How a file is delimited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Dialect {
    /// Field separator (`,`, `;`, tab, `|`).
    pub delimiter: char,
    /// Quote character.
    pub quote: char,
}

impl Default for Dialect {
    fn default() -> Self {
        Dialect {
            delimiter: ',',
            quote: '"',
        }
    }
}

/// Splits text into records.
#[must_use]
pub fn parse_records(text: &str, d: Dialect) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut chars = text.chars().peekable();
    let mut in_quotes = false;
    let mut quoted_field = false;
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if in_quotes {
            if c == d.quote {
                if chars.peek() == Some(&d.quote) {
                    field.push(d.quote);
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }
        if c == d.quote && field.is_empty() && !quoted_field {
            in_quotes = true;
            quoted_field = true;
        } else if c == d.delimiter {
            record.push(std::mem::take(&mut field));
            quoted_field = false;
        } else if c == '\r' || c == '\n' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            record.push(std::mem::take(&mut field));
            records.push(std::mem::take(&mut record));
            quoted_field = false;
            any = false;
        } else {
            field.push(c);
        }
    }
    if any || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

/// Fields per line for a candidate delimiter over the first lines (quotes respected).
fn counts(text: &str, delimiter: char, quote: char, lines: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut n = 0usize;
    let mut in_q = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if out.len() >= lines {
            break;
        }
        if c == quote {
            in_q = !in_q;
        } else if !in_q && c == delimiter {
            n += 1;
        } else if !in_q && (c == '\n' || c == '\r') {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            out.push(n);
            n = 0;
        }
    }
    if n > 0 && out.len() < lines {
        out.push(n);
    }
    out
}

/// Picks the delimiter whose count is most consistent (and highest) over the first 50 lines;
/// files with a `.tsv`/`.tab` extension should pass tab instead of sniffing.
#[must_use]
pub fn sniff(text: &str) -> Dialect {
    let quote = '"';
    let mut best = (',', 0usize, 0usize);
    for d in [',', ';', '\t', '|'] {
        let c = counts(text, d, quote, 50);
        if c.is_empty() {
            continue;
        }
        let first = c[0];
        if first == 0 {
            continue;
        }
        let consistent = c.iter().filter(|&&x| x == first).count();
        // Prefer lines that agree, then more fields.
        if (consistent, first) > (best.1, best.2) {
            best = (d, consistent, first);
        }
    }
    Dialect {
        delimiter: best.0,
        quote,
    }
}

/// Whether a field must be quoted (Excel quotes the delimiter, quotes, line breaks and leading
/// or trailing spaces).
#[must_use]
pub fn needs_quotes(field: &str, d: Dialect) -> bool {
    field.contains(d.delimiter)
        || field.contains(d.quote)
        || field.contains(['\n', '\r'])
        || field.starts_with(' ')
        || field.ends_with(' ')
}

/// Appends a field, quoted when needed.
pub fn push_field(out: &mut String, field: &str, d: Dialect) {
    if needs_quotes(field, d) {
        out.push(d.quote);
        for c in field.chars() {
            if c == d.quote {
                out.push(d.quote);
            }
            out.push(c);
        }
        out.push(d.quote);
    } else {
        out.push_str(field);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn rfc4180_records() {
        let d = Dialect::default();
        let r = parse_records("a,\"b,c\",\"d \"\"q\"\"\"\r\n\"multi\nline\",,x\n", d);
        assert_eq!(
            r,
            vec![
                vec!["a".to_owned(), "b,c".into(), "d \"q\"".into()],
                vec!["multi\nline".into(), String::new(), "x".into()],
            ]
        );
        assert_eq!(parse_records("", d), Vec::<Vec<String>>::new());
        assert_eq!(parse_records("x", d), vec![vec!["x".to_owned()]]);
        // Tolerated: a quote inside an unquoted field, text after a closing quote.
        assert_eq!(
            parse_records("5\" pipe,\"a\"b\r", d),
            vec![vec!["5\" pipe".to_owned(), "ab".into()]]
        );
        // CR-only line endings (old Mac).
        assert_eq!(parse_records("a\rb", d).len(), 2);
    }

    #[test]
    fn sniffs_delimiters() {
        assert_eq!(sniff("a;b;c\n1;2,5;3\n").delimiter, ';');
        assert_eq!(sniff("a\tb\n1\t2\n").delimiter, '\t');
        assert_eq!(sniff("a,b\n\"x;y\",2\n").delimiter, ',');
        assert_eq!(sniff("a|b|c\n1|2|3\n").delimiter, '|');
        assert_eq!(sniff("single\ncolumn\n").delimiter, ',');
    }

    proptest! {
        #[test]
        fn written_fields_read_back(rows in proptest::collection::vec(
            proptest::collection::vec("[ -~\n\"é;,\t]{0,12}", 1..5), 1..6)
        ) {
            for d in [Dialect::default(), Dialect { delimiter: ';', quote: '"' }, Dialect { delimiter: '\t', quote: '"' }] {
                let mut text = String::new();
                for r in &rows {
                    for (i, f) in r.iter().enumerate() {
                        if i > 0 { text.push(d.delimiter); }
                        push_field(&mut text, f, d);
                    }
                    text.push_str("\r\n");
                }
                let back = parse_records(&text, d);
                // A record of one empty field is an empty line: equal to [""] either way.
                prop_assert_eq!(&back, &rows);
            }
        }
    }
}
