// SPDX-License-Identifier: AGPL-3.0-or-later
//! Excel's header and footer codes: `&L`, `&C` and `&R` start the left, centre and right
//! sections; `&P` is the page number, `&N` the page count, `&D` the date, `&T` the time, `&F` the
//! file name, `&Z` the file's folder, `&A` the sheet name, `&G` a picture (not printed), `&&` an
//! ampersand. `&+N`/`&-N` offset the page number. Formatting codes (`&B`, `&I`, `&U`, `&E`,
//! `&S`, `&X`, `&Y`, `&"font,style"`, `&nn`) are read and dropped: the text is printed in the
//! default font.

/// What the codes stand for on one page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Fields<'a> {
    /// Page number.
    pub page: u32,
    /// Pages.
    pub pages: u32,
    /// The date as shown (`18/10/2026`).
    pub date: &'a str,
    /// The time as shown (`14:05`).
    pub time: &'a str,
    /// File name.
    pub file: &'a str,
    /// Folder of the file.
    pub path: &'a str,
    /// Sheet name.
    pub sheet: &'a str,
}

/// The three sections (left, centre, right) of a header or footer with the fields filled in.
#[must_use]
pub fn sections(code: &str, f: &Fields<'_>) -> [String; 3] {
    let mut out = [String::new(), String::new(), String::new()];
    let mut cur = 1usize;
    let mut chars = code.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '&' {
            out[cur].push(c);
            continue;
        }
        let Some(&k) = chars.peek() else {
            break;
        };
        chars.next();
        match k {
            'L' | 'l' => cur = 0,
            'C' | 'c' => cur = 1,
            'R' | 'r' => cur = 2,
            'P' | 'p' => {
                // `&P+2`: an offset after the code.
                let mut page = i64::from(f.page);
                if let Some(&sign) = chars.peek()
                    && (sign == '+' || sign == '-')
                {
                    chars.next();
                    let mut n = String::new();
                    while let Some(&d) = chars.peek() {
                        if !d.is_ascii_digit() {
                            break;
                        }
                        n.push(d);
                        chars.next();
                    }
                    let n: i64 = n.parse().unwrap_or(0);
                    page += if sign == '+' { n } else { -n };
                }
                out[cur].push_str(&page.to_string());
            }
            'N' | 'n' => out[cur].push_str(&f.pages.to_string()),
            'D' | 'd' => out[cur].push_str(f.date),
            'T' | 't' => out[cur].push_str(f.time),
            'F' | 'f' => out[cur].push_str(f.file),
            'Z' | 'z' => out[cur].push_str(f.path),
            'A' | 'a' => out[cur].push_str(f.sheet),
            '&' => out[cur].push('&'),
            '"' => {
                // A font name and style, to the closing quote.
                for d in chars.by_ref() {
                    if d == '"' {
                        break;
                    }
                }
            }
            d if d.is_ascii_digit() => {
                // A font size: the digits that follow too.
                while chars.peek().is_some_and(char::is_ascii_digit) {
                    chars.next();
                }
            }
            'K' | 'k' => {
                // A colour: six hex digits (or a theme colour `K01+000`).
                for _ in 0..6 {
                    if chars
                        .peek()
                        .is_some_and(|c| c.is_ascii_alphanumeric() || *c == '+' || *c == '-')
                    {
                        chars.next();
                    }
                }
            }
            // Formatting and pictures.
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields() -> Fields<'static> {
        Fields {
            page: 2,
            pages: 4,
            date: "18/10/2026",
            time: "09:30",
            file: "print-setup.xlsx",
            path: "/data/ana/",
            sheet: "Print",
        }
    }

    #[test]
    fn fills_in_the_fields_of_each_section() {
        assert_eq!(
            sections("&C&F — &A", &fields()),
            [
                String::new(),
                "print-setup.xlsx — Print".into(),
                String::new()
            ]
        );
        assert_eq!(
            sections("&L&D &T&RPage &P of &N", &fields()),
            [
                "18/10/2026 09:30".into(),
                String::new(),
                "Page 2 of 4".into()
            ]
        );
    }

    #[test]
    fn drops_formatting_codes_and_keeps_ampersands() {
        assert_eq!(
            sections("&\"Arial,Bold\"&14&BR&&D &KFF0000red&P+10", &fields())[1],
            "R&D red12"
        );
    }
}
