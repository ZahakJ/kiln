// SPDX-License-Identifier: AGPL-3.0-or-later
//! What ties the published report to what produced it: a digest of the scorer, the readers and
//! writers it measures, the thresholds, the committed fixtures and the corpus manifest. It is
//! taken from the files themselves, so it means the same in any checkout of them.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Trees and files a report depends on, relative to the repository root.
const INPUTS: &[&str] = &[
    "crates/kiln-testing/src",
    "crates/kiln-ooxml/src",
    "crates/kiln-io-docx/src",
    "crates/kiln-io-xlsx/src",
    "crates/kiln-io-pptx/src",
    "crates/kiln-io-odf/src",
    "crates/kiln-doc/src",
    "crates/kiln-doc-layout/src",
    "crates/kiln-deck/src",
    "crates/kiln-sheet/src",
    "crates/kiln-formula/src",
    "crates/kiln-charts/src",
    "crates/kiln-cli/src/fidelity.rs",
    "tools/oracle-soffice.sh",
    "parity/corpus.json",
    "fixtures/corpus-manifest.json",
    "fixtures/docx/features",
    "fixtures/docx/manual",
    "fixtures/xlsx/features",
    "fixtures/xlsx/manual",
    "fixtures/xlsx/charts",
    "fixtures/pptx/features",
    "fixtures/pptx/manual",
    "fixtures/odf/features",
    "fixtures/odf/manual",
];

fn files_under(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
    } else if let Ok(entries) = std::fs::read_dir(path) {
        for e in entries.flatten() {
            files_under(&e.path(), out);
        }
    }
}

/// The digest of a checkout's fidelity inputs, as hex.
#[must_use]
pub fn digest(root: &Path) -> String {
    let mut files = Vec::new();
    for input in INPUTS {
        files_under(&root.join(input), &mut files);
    }
    let mut keyed: Vec<(String, PathBuf)> = files
        .into_iter()
        .filter_map(|p| {
            let rel = p.strip_prefix(root).ok()?.to_str()?.replace('\\', "/");
            Some((rel, p))
        })
        .collect();
    keyed.sort();
    let mut hash = Sha256::new();
    for (rel, path) in keyed {
        hash.update(rel.as_bytes());
        hash.update([0]);
        if let Ok(bytes) = std::fs::read(&path) {
            hash.update((bytes.len() as u64).to_le_bytes());
            hash.update(&bytes);
        }
    }
    let mut out = String::new();
    for b in &hash.finalize()[..16] {
        let _ = write!(out, "{b:02x}");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_digest_follows_the_inputs_and_only_them() {
        let dir = std::env::temp_dir().join(format!("kiln-stamp-{}", std::process::id()));
        let file = dir.join("crates/kiln-testing/src/a.rs");
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "one").unwrap();
        let first = digest(&dir);
        assert_eq!(first.len(), 32);
        std::fs::write(dir.join("README.md"), "not an input").unwrap();
        assert_eq!(digest(&dir), first);
        std::fs::write(&file, "two").unwrap();
        assert_ne!(digest(&dir), first);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
