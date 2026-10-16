// SPDX-License-Identifier: AGPL-3.0-or-later
//! Part-by-part comparison of OPC packages, and the passthrough repack (open + save without
//! edits) that every `.docx`/`.xlsx`/`.pptx` must survive byte for byte.

use std::time::{Duration, Instant};

use kiln_ooxml::opc::OpcPackage;

/// How two packages compare part by part (parts include `.rels` parts).
///
/// `[Content_Types].xml` is not counted among the byte-compared parts: `kiln-ooxml` parses it on
/// open and does not keep its source bytes, so it is compared by what it declares (the default
/// and override content types) and reported in [`PartComparison::content_types_equal`]. Whole
/// output files are compared byte for byte by [`repack`] ([`Repack::zip_identical`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PartComparison {
    /// Parts of the first package (`[Content_Types].xml` excluded).
    pub parts: usize,
    /// Parts of the first package that the second holds with identical (inflated) bytes.
    pub identical: usize,
    /// Parts whose bytes differ.
    pub differing: Vec<String>,
    /// Parts of the first package the second lacks.
    pub missing: Vec<String>,
    /// Parts only the second package has.
    pub added: Vec<String>,
    /// Whether both packages declare the same content types (`None` when `[Content_Types].xml`
    /// was skipped). When they differ, `[Content_Types].xml` is also listed in `differing`.
    pub content_types_equal: Option<bool>,
}

impl PartComparison {
    /// True when every part of the first package is identical in the second, nothing was added
    /// and the declared content types are the same.
    #[must_use]
    pub fn all_identical(&self) -> bool {
        self.identical == self.parts
            && self.added.is_empty()
            && self.content_types_equal != Some(false)
    }
}

/// The bytes of `[Content_Types].xml` (not listed by `all_part_names`).
const CONTENT_TYPES: &str = "/[Content_Types].xml";

fn content_types_bytes(pkg: &OpcPackage) -> Vec<u8> {
    pkg.content_types().to_bytes()
}

/// Compares two packages part by part. Parts listed in `skip` (case-insensitive part names,
/// such as the parts a writer regenerates) are left out of the comparison.
#[must_use]
pub fn compare_packages(a: &OpcPackage, b: &OpcPackage, skip: &[String]) -> PartComparison {
    let skipped = |name: &str| skip.iter().any(|s| s.eq_ignore_ascii_case(name));
    let mut cmp = PartComparison::default();
    if !skipped(CONTENT_TYPES) {
        // Both sides are re-serialised the same way, so this compares the declared types, not
        // the source bytes (see the type's documentation).
        let equal = content_types_bytes(a) == content_types_bytes(b);
        cmp.content_types_equal = Some(equal);
        if !equal {
            cmp.differing.push(CONTENT_TYPES.to_owned());
        }
    }
    for name in a.all_part_names() {
        if skipped(name) {
            continue;
        }
        cmp.parts += 1;
        let left = a.part(name);
        let right = b.part(name);
        match (left, right) {
            (Ok(Some(x)), Ok(Some(y))) if x == y => cmp.identical += 1,
            (_, Ok(None)) => cmp.missing.push(name.to_owned()),
            _ => cmp.differing.push(name.to_owned()),
        }
    }
    cmp.added = b
        .all_part_names()
        .filter(|n| !a.has(n) && !skipped(n))
        .map(str::to_owned)
        .collect();
    cmp
}

/// The result of [`repack`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repack {
    /// The saved package.
    pub bytes: Vec<u8>,
    /// Input against output.
    pub comparison: PartComparison,
    /// Time to open and save.
    pub elapsed: Duration,
    /// Whether the zip entries came out byte-identical too (same compressed bytes, order and
    /// names), not just the inflated parts.
    pub zip_identical: bool,
}

/// Opens a package and saves it without edits, then compares input and output.
///
/// # Errors
///
/// When the input is not an OPC package, or the output cannot be re-read.
pub fn repack(bytes: &[u8]) -> Result<Repack, String> {
    let start = Instant::now();
    let pkg = OpcPackage::open(bytes).map_err(|e| e.to_string())?;
    let saved = pkg.save().map_err(|e| e.to_string())?;
    let elapsed = start.elapsed();
    let again = OpcPackage::open(&saved).map_err(|e| format!("the repacked package: {e}"))?;
    let comparison = compare_packages(&pkg, &again, &[]);
    Ok(Repack {
        zip_identical: saved == bytes,
        bytes: saved,
        comparison,
        elapsed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(rel: &str) -> Vec<u8> {
        std::fs::read(crate::fixtures::fixtures_dir().join(rel)).unwrap()
    }

    #[test]
    fn repacking_a_fixture_keeps_every_part() {
        let r = repack(&fixture("xlsx/features/styles.xlsx")).unwrap();
        assert!(r.comparison.all_identical(), "{:?}", r.comparison);
        assert!(r.comparison.parts > 5);
        assert_eq!(r.comparison.identical, r.comparison.parts);
        assert_eq!(r.comparison.content_types_equal, Some(true));
        assert!(r.zip_identical);
    }

    #[test]
    fn detects_changed_missing_and_added_parts() {
        let bytes = fixture("docx/features/tables.docx");
        let a = OpcPackage::open(&bytes).unwrap();
        let mut b = OpcPackage::open(&bytes).unwrap();
        b.set_part("/word/document.xml", b"<w:document/>".to_vec())
            .unwrap();
        b.add_part(
            "/custom/extra.bin",
            "application/octet-stream",
            vec![1, 2, 3],
        )
        .unwrap();
        let removable = a
            .all_part_names()
            .find(|n| n.ends_with("settings.xml"))
            .unwrap()
            .to_owned();
        b.remove_part(&removable, true).unwrap();
        let c = compare_packages(&a, &b, &[]);
        assert!(!c.all_identical());
        assert!(c.differing.iter().any(|p| p == "/word/document.xml"));
        assert_eq!(c.added, ["/custom/extra.bin"]);
        assert!(c.missing.contains(&removable));
        // Skipping the regenerated part leaves the others to compare.
        let skip = vec!["/WORD/document.xml".to_owned()];
        let c2 = compare_packages(&a, &b, &skip);
        assert!(!c2.differing.iter().any(|p| p == "/word/document.xml"));
        assert_eq!(c2.parts + 1, c.parts);
        // Removing a part drops its override: the declared content types differ.
        assert_eq!(c.content_types_equal, Some(false));
        assert!(c.differing.iter().any(|p| p == CONTENT_TYPES));
        let c3 = compare_packages(&a, &b, &[CONTENT_TYPES.to_owned()]);
        assert_eq!(c3.content_types_equal, None);
    }

    #[test]
    fn non_packages_are_errors() {
        assert!(repack(b"not a zip").is_err());
    }
}
