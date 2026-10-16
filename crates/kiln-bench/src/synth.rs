// SPDX-License-Identifier: AGPL-3.0-or-later
//! Deterministic synthetic inputs for the benchmarks: a small random generator, word streams and
//! a large SpreadsheetML package built in memory with `kiln-ooxml` (so CI needs no Python and no
//! fixture files). The on-disk large fixtures (`fixtures/large/`, 300-page `.docx`, 1M-cell
//! `.xlsx`, 200-slide `.pptx`) come from `tools/make-fixtures.py --synthetic-large`.

use std::fmt::Write as _;

use kiln_ooxml::opc::{OpcPackage, RelationshipOptions, content_type, rel_type};

/// A 64-bit linear congruential generator (Knuth's MMIX constants): deterministic everywhere,
/// no dependency, plenty for test data.
#[derive(Debug, Clone)]
pub struct Lcg(u64);

impl Lcg {
    /// A generator from a seed.
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    /// The next raw value (the high 31 bits of the state).
    pub fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(clippy::cast_possible_truncation)]
        let v = (self.0 >> 33) as u32;
        v
    }

    /// A value in `0..n` (`n > 0`).
    pub fn below(&mut self, n: usize) -> usize {
        self.next_u32() as usize % n.max(1)
    }

    /// `n` words drawn from `vocabulary` (whitespace-separated) joined by `sep`.
    pub fn words(&mut self, vocabulary: &str, n: usize, sep: &str) -> String {
        let list: Vec<&str> = vocabulary.split_whitespace().collect();
        if list.is_empty() {
            return String::new();
        }
        (0..n)
            .map(|_| list[self.below(list.len())])
            .collect::<Vec<_>>()
            .join(sep)
    }
}

/// A1 column letters for a 0-based column.
#[must_use]
pub fn column_name(mut col: u32) -> String {
    let mut s = Vec::new();
    loop {
        #[allow(clippy::cast_possible_truncation)]
        s.push(b'A' + (col % 26) as u8);
        if col < 26 {
            break;
        }
        col = col / 26 - 1;
    }
    s.reverse();
    String::from_utf8(s).unwrap_or_default()
}

/// The XML of one worksheet: `rows × cols` cells, numbers in the first `cols − formula_cols`
/// columns and formulas (`=A1*2+B1`-style, with cached values) in the rest.
#[must_use]
pub fn sheet_xml(rows: u32, cols: u32, formula_cols: u32) -> String {
    let mut rng = Lcg::new(0x00c0_ffee);
    let mut xml = String::with_capacity(rows as usize * cols as usize * 40);
    xml.push_str(concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        "\n",
        r#"<worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#
    ));
    let names: Vec<String> = (0..cols).map(column_name).collect();
    let values = cols.saturating_sub(formula_cols).max(1);
    for r in 1..=rows {
        let _ = write!(xml, r#"<row r="{r}">"#);
        for c in 0..cols {
            let name = &names[c as usize];
            if c < values {
                let v = rng.below(100_000);
                let _ = write!(xml, r#"<c r="{name}{r}"><v>{v}</v></c>"#);
            } else {
                let a = &names[0];
                let b = &names[(c - 1) as usize];
                let _ = write!(
                    xml,
                    r#"<c r="{name}{r}"><f>{a}{r}*2+{b}{r}</f><v>0</v></c>"#
                );
            }
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    xml
}

/// A complete `.xlsx` package with one worksheet of `rows × cols` cells.
///
/// # Errors
///
/// Only if the OPC writer fails (it does not for this input).
pub fn xlsx(rows: u32, cols: u32, formula_cols: u32) -> Result<Vec<u8>, String> {
    let e = |e: kiln_ooxml::opc::OpcError| e.to_string();
    let mut pkg = OpcPackage::new();
    let workbook = concat!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#,
        "\n",
        r#"<workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#
    );
    pkg.add_part(
        "/xl/workbook.xml",
        content_type::XLSX_WORKBOOK,
        workbook.as_bytes().to_vec(),
    )
    .map_err(e)?;
    pkg.add_part(
        "/xl/worksheets/sheet1.xml",
        content_type::XLSX_WORKSHEET,
        sheet_xml(rows, cols, formula_cols).into_bytes(),
    )
    .map_err(e)?;
    let id1 = RelationshipOptions {
        id: Some("rId1"),
        external: false,
    };
    pkg.add_relationship("/", rel_type::OFFICE_DOCUMENT, "/xl/workbook.xml", id1)
        .map_err(e)?;
    pkg.add_relationship(
        "/xl/workbook.xml",
        rel_type::WORKSHEET,
        "/xl/worksheets/sheet1.xml",
        id1,
    )
    .map_err(e)?;
    pkg.save().map_err(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_is_deterministic() {
        let (mut a, mut b) = (Lcg::new(1), Lcg::new(1));
        let x: Vec<u32> = (0..5).map(|_| a.next_u32()).collect();
        let y: Vec<u32> = (0..5).map(|_| b.next_u32()).collect();
        assert_eq!(x, y);
        assert!(Lcg::new(2).below(10) < 10);
        assert_eq!(Lcg::new(3).words("a", 3, "-"), "a-a-a");
        assert_eq!(Lcg::new(3).words(" ", 3, "-"), "");
    }

    #[test]
    fn column_names_follow_excel() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
    }

    #[test]
    fn builds_a_valid_package() {
        let bytes = xlsx(20, 4, 2).unwrap();
        assert_eq!(bytes, xlsx(20, 4, 2).unwrap(), "deterministic");
        let pkg = OpcPackage::open(&bytes).unwrap();
        assert_eq!(
            pkg.main_document_part().unwrap().as_deref(),
            Some("/xl/workbook.xml")
        );
        let sheet = pkg.part_bytes("/xl/worksheets/sheet1.xml").unwrap();
        let text = String::from_utf8_lossy(&sheet);
        assert_eq!(text.matches("<c ").count(), 80);
        assert_eq!(text.matches("<f>").count(), 40);
        assert!(text.contains(r#"<c r="D20"><f>A20*2+C20</f>"#));
    }
}
