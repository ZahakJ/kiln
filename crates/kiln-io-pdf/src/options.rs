// SPDX-License-Identifier: AGPL-3.0-or-later
//! What to print and how: the page selection, copies, paper, scale, pages per sheet, colour and
//! the accessibility switches. The same options drive the PDF and the print preview, so the
//! preview always shows exactly what the PDF will contain.

use kiln_common::{KilnError, Result};

/// Which pages to print.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PageSelection {
    /// Every page.
    #[default]
    All,
    /// Only the pages listed, as 1-based page positions, in the order given (Word's "Pages"
    /// box: `1-3, 5, 8-`).
    Ranges(Vec<PageRange>),
}

/// An inclusive range of 1-based page positions; `end: None` runs to the last page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRange {
    /// First page (1-based).
    pub start: usize,
    /// Last page (1-based, inclusive); `None` = the last page of the document.
    pub end: Option<usize>,
}

impl PageSelection {
    /// Parses Word's page box: numbers and ranges separated by commas or semicolons
    /// (`1-3, 5, 8-`, `4–6`, `-3`). An empty or blank text selects every page.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for anything else (letters, zero, a range that runs backwards).
    pub fn parse(text: &str) -> Result<Self> {
        let text = text.trim();
        if text.is_empty() {
            return Ok(Self::All);
        }
        let bad = |part: &str| {
            KilnError::InvalidInput(format!(
                "\"{part}\" is not a page or page range (try 1-3, 5)"
            ))
        };
        let number = |s: &str, part: &str| -> Result<usize> {
            let n: usize = s.trim().parse().map_err(|_| bad(part))?;
            if n == 0 { Err(bad(part)) } else { Ok(n) }
        };
        let mut out = Vec::new();
        for part in text.split([',', ';']) {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            let dash = part.find(['-', '\u{2013}', '\u{2014}']);
            let range = match dash {
                None => {
                    let n = number(part, part)?;
                    PageRange {
                        start: n,
                        end: Some(n),
                    }
                }
                Some(i) => {
                    let dash_len = part[i..].chars().next().map_or(1, char::len_utf8);
                    let (a, b) = (part[..i].trim(), part[i + dash_len..].trim());
                    let start = if a.is_empty() { 1 } else { number(a, part)? };
                    let end = if b.is_empty() {
                        None
                    } else {
                        Some(number(b, part)?)
                    };
                    if end.is_some_and(|e| e < start) {
                        return Err(bad(part));
                    }
                    PageRange { start, end }
                }
            };
            out.push(range);
        }
        if out.is_empty() {
            return Ok(Self::All);
        }
        Ok(Self::Ranges(out))
    }

    /// The 0-based page indexes selected from a document of `count` pages, in print order.
    /// Pages past the end are ignored.
    #[must_use]
    pub fn resolve(&self, count: usize) -> Vec<usize> {
        match self {
            Self::All => (0..count).collect(),
            Self::Ranges(ranges) => {
                let mut out = Vec::new();
                for r in ranges {
                    let end = r.end.unwrap_or(count).min(count);
                    if r.start > end {
                        continue;
                    }
                    out.extend(r.start - 1..end);
                }
                out
            }
        }
    }
}

/// Odd or even pages only (Word's "Print All Pages / Only Odd / Only Even").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PageParity {
    /// Every selected page.
    #[default]
    All,
    /// Odd page positions (1, 3, 5…).
    Odd,
    /// Even page positions.
    Even,
}

/// A paper size in points (portrait).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    /// Width, pt.
    pub width: f32,
    /// Height, pt.
    pub height: f32,
}

impl Paper {
    /// US Letter, 8.5 × 11 in.
    pub const LETTER: Paper = Paper {
        width: 612.0,
        height: 792.0,
    };
    /// US Legal, 8.5 × 14 in.
    pub const LEGAL: Paper = Paper {
        width: 612.0,
        height: 1008.0,
    };
    /// A3, 297 × 420 mm.
    pub const A3: Paper = Paper {
        width: 841.89,
        height: 1190.55,
    };
    /// A4, 210 × 297 mm.
    pub const A4: Paper = Paper {
        width: 595.28,
        height: 841.89,
    };
    /// A5, 148 × 210 mm.
    pub const A5: Paper = Paper {
        width: 419.53,
        height: 595.28,
    };

    /// The same paper turned to landscape.
    #[must_use]
    pub fn landscape(self) -> Paper {
        Paper {
            width: self.width.max(self.height),
            height: self.width.min(self.height),
        }
    }
}

/// How the printed pages are sized on the paper.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum Scaling {
    /// Each page at its own size (the paper is the page), times `PdfOptions::scale`.
    #[default]
    Actual,
    /// Every page scaled to fit this paper, centred (Word's "Scale to Paper Size").
    FitTo(Paper),
}

/// Options for one PDF export, print or preview.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfOptions {
    /// The pages.
    pub pages: PageSelection,
    /// Odd or even pages only.
    pub parity: PageParity,
    /// Copies (collated: the whole selection, then again). 1–999.
    pub copies: u32,
    /// Paper and fitting.
    pub scaling: Scaling,
    /// Extra zoom applied to the content, 0.1–4.0 (1.0 = 100 %).
    pub scale: f32,
    /// Pages per sheet: 1, 2, 4, 6, 9 or 16 (laid out left to right, top to bottom; two and six
    /// per sheet turn the sheet to landscape, as Word does).
    pub per_sheet: u32,
    /// Print in shades of grey.
    pub grayscale: bool,
    /// Write the logical structure (headings, paragraphs, tables, figures with alt text,
    /// links) so screen readers and reflow work (PDF/UA-1 basics).
    pub tagged: bool,
    /// Write bookmarks (the outline) from the headings.
    pub bookmarks: bool,
    /// Document title (the window title of PDF viewers); `None` uses the printable's own.
    pub title: Option<String>,
}

impl Default for PdfOptions {
    fn default() -> Self {
        Self {
            pages: PageSelection::All,
            parity: PageParity::All,
            copies: 1,
            scaling: Scaling::Actual,
            scale: 1.0,
            per_sheet: 1,
            grayscale: false,
            tagged: true,
            bookmarks: true,
            title: None,
        }
    }
}

/// Pages per sheet Kiln lays out.
pub const PER_SHEET: [u32; 6] = [1, 2, 4, 6, 9, 16];

impl PdfOptions {
    /// Checks the numbers.
    ///
    /// # Errors
    ///
    /// `InvalidInput` for copies outside 1–999, a scale outside 0.1–4.0, a pages-per-sheet
    /// count Kiln does not lay out, or a non-finite or non-positive paper.
    pub fn validate(&self) -> Result<()> {
        if !(1..=999).contains(&self.copies) {
            return Err(KilnError::InvalidInput(format!(
                "{} copies (1 to 999)",
                self.copies
            )));
        }
        if !(self.scale.is_finite() && (0.1..=4.0).contains(&self.scale)) {
            return Err(KilnError::InvalidInput(format!(
                "a scale of {} (10 % to 400 %)",
                self.scale
            )));
        }
        if !PER_SHEET.contains(&self.per_sheet) {
            return Err(KilnError::InvalidInput(format!(
                "{} pages per sheet (1, 2, 4, 6, 9 or 16)",
                self.per_sheet
            )));
        }
        if let Scaling::FitTo(p) = self.scaling
            && !(p.width.is_finite() && p.height.is_finite() && p.width > 1.0 && p.height > 1.0)
        {
            return Err(KilnError::InvalidInput("a paper without a size".into()));
        }
        Ok(())
    }

    /// The 0-based pages printed from a document of `count` pages (selection, then parity).
    #[must_use]
    pub fn pages_of(&self, count: usize) -> Vec<usize> {
        let pages = self.pages.resolve(count);
        match self.parity {
            PageParity::All => pages,
            PageParity::Odd => pages.into_iter().filter(|p| p % 2 == 0).collect(),
            PageParity::Even => pages.into_iter().filter(|p| p % 2 == 1).collect(),
        }
    }

    /// Columns and rows of the pages-per-sheet grid (for portrait pages).
    #[must_use]
    pub fn grid(&self) -> (u32, u32) {
        match self.per_sheet {
            2 => (2, 1),
            4 => (2, 2),
            6 => (3, 2),
            9 => (3, 3),
            16 => (4, 4),
            _ => (1, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_boxes_parse_like_word() {
        assert_eq!(PageSelection::parse("").unwrap(), PageSelection::All);
        assert_eq!(PageSelection::parse("  ").unwrap(), PageSelection::All);
        let s = PageSelection::parse("1-3, 5;8-").unwrap();
        assert_eq!(s.resolve(10), vec![0, 1, 2, 4, 7, 8, 9]);
        assert_eq!(
            PageSelection::parse("4\u{2013}6").unwrap().resolve(10),
            vec![3, 4, 5]
        );
        assert_eq!(PageSelection::parse("-2").unwrap().resolve(10), vec![0, 1]);
        // Past the end: ignored; the order given is kept.
        assert_eq!(
            PageSelection::parse("12, 3, 1").unwrap().resolve(5),
            vec![2, 0]
        );
        for bad in ["0", "a", "3-1", "1-b", "1--3"] {
            assert!(PageSelection::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn options_validate_and_select() {
        let mut o = PdfOptions::default();
        assert!(o.validate().is_ok());
        o.parity = PageParity::Even;
        assert_eq!(o.pages_of(5), vec![1, 3]);
        o.parity = PageParity::Odd;
        assert_eq!(o.pages_of(5), vec![0, 2, 4]);
        o.copies = 0;
        assert!(o.validate().is_err());
        o.copies = 2;
        o.scale = 9.0;
        assert!(o.validate().is_err());
        o.scale = 1.0;
        o.per_sheet = 3;
        assert!(o.validate().is_err());
        o.per_sheet = 6;
        assert_eq!(o.grid(), (3, 2));
        o.scaling = Scaling::FitTo(Paper {
            width: f32::NAN,
            height: 1.0,
        });
        assert!(o.validate().is_err());
        assert_eq!(Paper::A4.landscape().width, Paper::A4.height);
    }
}
