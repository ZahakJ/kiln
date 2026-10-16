// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fixture access shared by the integration tests.
#![allow(dead_code)]

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};

/// The repository's `fixtures/` directory.
pub fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

/// The bytes of a fixture, by path relative to `fixtures/`.
pub fn fixture(rel: &str) -> Vec<u8> {
    let path = fixtures_dir().join(rel);
    std::fs::read(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
}

fn files_with_ext(dir: &Path, ext: &str) -> Vec<String> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<String> = read
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(&format!(".{ext}")))
        .collect();
    out.sort();
    out
}

/// The generated feature fixtures of one format (`docx/features/styles.docx`, …).
pub fn feature_fixtures(format: &str) -> Vec<String> {
    files_with_ext(&fixtures_dir().join(format).join("features"), format)
        .into_iter()
        .map(|f| format!("{format}/features/{f}"))
        .collect()
}

/// Every feature fixture of the three formats.
pub fn all_feature_fixtures() -> Vec<String> {
    ["docx", "xlsx", "pptx"]
        .iter()
        .flat_map(|f| feature_fixtures(f))
        .collect()
}

/// Real-world files downloaded by `tools/fetch-corpus.sh` (absent on fresh clones and CI).
pub fn external_fixtures() -> Vec<String> {
    ["docx", "xlsx", "pptx"]
        .iter()
        .flat_map(|f| {
            files_with_ext(&fixtures_dir().join("external").join(f), f)
                .into_iter()
                .map(move |n| format!("external/{f}/{n}"))
        })
        .collect()
}

/// A zip entry: name, inflated bytes, raw (compressed) bytes.
pub struct ZipEntry {
    pub name: String,
    pub data: Vec<u8>,
    pub raw: Vec<u8>,
}

/// Every non-directory entry of a zip, in archive order.
pub fn unzip(bytes: &[u8]) -> Vec<ZipEntry> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a zip");
    let mut out = Vec::new();
    for i in 0..archive.len() {
        let (name, data) = {
            let mut f = archive.by_index(i).expect("entry");
            if f.is_dir() {
                continue;
            }
            let mut data = Vec::new();
            f.read_to_end(&mut data).expect("inflate");
            (f.name().expect("name").into_owned(), data)
        };
        let mut raw = Vec::new();
        archive
            .by_index_raw(i)
            .expect("raw entry")
            .read_to_end(&mut raw)
            .expect("raw read");
        out.push(ZipEntry { name, data, raw });
    }
    out
}

/// Builds a zip in memory from `(name, bytes, deflate?)` entries.
pub fn make_zip(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, data, deflate) in entries {
        let opts = if *deflate {
            SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .compression_level(Some(9))
        } else {
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored)
        };
        w.start_file(*name, opts).expect("start");
        w.write_all(data).expect("write");
    }
    w.finish().expect("finish").into_inner()
}
