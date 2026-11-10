// SPDX-License-Identifier: AGPL-3.0-or-later
//! The native bundles `.kwrite`, `.kgrid` and `.kdeck`.
//!
//! A bundle is a zip archive:
//!
//! | Entry | What |
//! |---|---|
//! | `manifest.json` | format name, bundle version, kind, generator, dates, the entries below |
//! | `model.bin` | the model (the app's own versioned postcard: `KDOC`, `KGRD`, `KDCK`) |
//! | `crdt.bin` | the CRDT of a shared document (Write and Deck) |
//! | `versions/index.json`, `versions/<id>.bin` | version history: what each version is, and its model |
//! | `media/<name>` | pictures and other media the model refers to by name |
//! | `passthrough/<name>` | what the model does not cover, kept to write the original format again |
//!
//! The manifest's `version` is [`BUNDLE_VERSION`]; a reader refuses a bundle with a higher
//! one, naming both, so a file from a newer Kiln is never misread. Readers check sizes before
//! inflating anything (entry count, entry and total size, the compression ratio of every entry
//! and of the whole archive, with the totals of the Office readers' package limits) and refuse
//! entry names that leave the archive.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use kiln_common::{KilnError, Result};
use kiln_protocol::DocKind;
use kiln_protocol::collab::VersionEntry;
use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, ZipArchive, ZipWriter};

/// The bundle format this version of Kiln writes, and the newest it reads.
pub const BUNDLE_VERSION: u32 = 1;
/// The manifest's `format`.
pub const FORMAT_NAME: &str = "kiln.bundle";

const MANIFEST: &str = "manifest.json";
const MODEL: &str = "model.bin";
const CRDT: &str = "crdt.bin";
const VERSION_INDEX: &str = "versions/index.json";
const VERSIONS: &str = "versions/";
const MEDIA: &str = "media/";
const PASSTHROUGH: &str = "passthrough/";

/// Most entries a bundle may have (as many as an Office package may).
const MAX_ENTRIES: usize = 65_536;
/// Largest entry, uncompressed.
const MAX_ENTRY: u64 = 1 << 30;
/// Largest total, uncompressed (the Office readers' package limit).
const MAX_TOTAL: u64 = 2 << 30;
/// No entry, and not the archive as a whole, may inflate more than this many times.
const MAX_RATIO: u64 = 200;
/// Entries this small inflate to little whatever their ratio (a short manifest).
const RATIO_FREE: u64 = 4 << 10;

/// `manifest.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Always [`FORMAT_NAME`].
    pub format: String,
    /// The bundle version ([`BUNDLE_VERSION`] when written by this Kiln).
    pub version: u32,
    /// The kind of document.
    pub kind: DocKind,
    /// What wrote it (`Kiln 0.1.0`).
    pub generator: String,
    /// The document's title.
    #[serde(default)]
    pub title: Option<String>,
    /// When the document was created, in milliseconds since the Unix epoch.
    #[serde(default)]
    pub created_ms: u64,
    /// When it was written.
    #[serde(default)]
    pub modified_ms: u64,
    /// Whether `crdt.bin` is there.
    #[serde(default)]
    pub crdt: bool,
    /// How many versions `versions/` holds.
    #[serde(default)]
    pub versions: usize,
    /// The entries under `media/`.
    #[serde(default)]
    pub media: Vec<String>,
    /// The entries under `passthrough/`.
    #[serde(default)]
    pub passthrough: Vec<String>,
}

/// One version kept in a bundle: what it is and the model it shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredVersion {
    /// What the Version History pane lists.
    pub entry: VersionEntry,
    /// The model of that version (the bytes of `model.bin` then).
    pub model: Vec<u8>,
}

/// A bundle in memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The kind of document.
    pub kind: DocKind,
    /// The title.
    pub title: Option<String>,
    /// When the document was created.
    pub created_ms: u64,
    /// When the bundle was written.
    pub modified_ms: u64,
    /// `model.bin`.
    pub model: Vec<u8>,
    /// `crdt.bin`.
    pub crdt: Option<Vec<u8>>,
    /// The version history, oldest first.
    pub versions: Vec<StoredVersion>,
    /// `media/*`, by name.
    pub media: BTreeMap<String, Vec<u8>>,
    /// `passthrough/*`, by name.
    pub passthrough: BTreeMap<String, Vec<u8>>,
}

impl Bundle {
    /// A bundle with a model and nothing else.
    #[must_use]
    pub fn new(kind: DocKind, model: Vec<u8>) -> Self {
        Self {
            kind,
            title: None,
            created_ms: 0,
            modified_ms: 0,
            model,
            crdt: None,
            versions: Vec::new(),
            media: BTreeMap::new(),
            passthrough: BTreeMap::new(),
        }
    }

    /// The archive.
    ///
    /// # Errors
    ///
    /// `Format` for an entry name that cannot be stored (see [`check_name`]), `Internal` when
    /// the archive cannot be written.
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        self.to_bytes_with(true)
    }

    /// The archive, its entries stored rather than deflated when `deflate` is false (an
    /// autosave checkpoint, written often and read rarely, is quicker to write so).
    ///
    /// # Errors
    ///
    /// As [`Bundle::to_bytes`].
    pub fn to_bytes_with(&self, deflate: bool) -> Result<Vec<u8>> {
        let manifest = Manifest {
            format: FORMAT_NAME.into(),
            version: BUNDLE_VERSION,
            kind: self.kind,
            generator: format!("Kiln {}", crate::VERSION),
            title: self.title.clone(),
            created_ms: self.created_ms,
            modified_ms: self.modified_ms,
            crdt: self.crdt.is_some(),
            versions: self.versions.len(),
            media: self.media.keys().cloned().collect(),
            passthrough: self.passthrough.keys().cloned().collect(),
        };
        let mut zip = ZipWriter::new(Cursor::new(Vec::with_capacity(
            self.model.len() + (16 << 10),
        )));
        let manifest = serde_json::to_vec_pretty(&manifest).map_err(internal)?;
        put(&mut zip, MANIFEST, &manifest, deflate)?;
        put(&mut zip, MODEL, &self.model, deflate)?;
        if let Some(crdt) = &self.crdt {
            put(&mut zip, CRDT, crdt, deflate)?;
        }
        if !self.versions.is_empty() {
            let index: Vec<&VersionEntry> = self.versions.iter().map(|v| &v.entry).collect();
            put(
                &mut zip,
                VERSION_INDEX,
                &serde_json::to_vec_pretty(&index).map_err(internal)?,
                deflate,
            )?;
            for v in &self.versions {
                check_id(&v.entry.id)?;
                put(
                    &mut zip,
                    &format!("{VERSIONS}{}.bin", v.entry.id),
                    &v.model,
                    deflate,
                )?;
            }
        }
        for (name, bytes) in &self.media {
            check_name(name)?;
            // Pictures are compressed already.
            put(
                &mut zip,
                &format!("{MEDIA}{name}"),
                bytes,
                deflate && !is_compressed(name),
            )?;
        }
        for (name, bytes) in &self.passthrough {
            check_name(name)?;
            put(
                &mut zip,
                &format!("{PASSTHROUGH}{name}"),
                bytes,
                deflate && !is_compressed(name),
            )?;
        }
        Ok(zip.finish().map_err(internal)?.into_inner())
    }

    /// Reads a bundle.
    ///
    /// # Errors
    ///
    /// `Format` for bytes that are not a Kiln bundle, a bundle of a newer version (the message
    /// says which), an archive over the size limits, a missing model or a damaged entry.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let mut zip = ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| format_error(format!("not a Kiln bundle ({e})")))?;
        check_limits(&mut zip)?;
        let manifest = read_manifest(&mut zip)?;
        let model = read_entry(&mut zip, MODEL)?
            .ok_or_else(|| format_error("the bundle has no model.bin".into()))?;
        let crdt = read_entry(&mut zip, CRDT)?;
        let mut versions = Vec::new();
        if let Some(index) = read_entry(&mut zip, VERSION_INDEX)? {
            let entries: Vec<VersionEntry> = serde_json::from_slice(&index)
                .map_err(|e| format_error(format!("versions/index.json: {e}")))?;
            for entry in entries {
                check_id(&entry.id)?;
                let name = format!("{VERSIONS}{}.bin", entry.id);
                let model = read_entry(&mut zip, &name)?
                    .ok_or_else(|| format_error(format!("the bundle has no {name}")))?;
                versions.push(StoredVersion { entry, model });
            }
        }
        let mut media = BTreeMap::new();
        let mut passthrough = BTreeMap::new();
        let names: Vec<String> = zip
            .file_names()
            .map(|n| n.map(std::borrow::Cow::into_owned))
            .collect::<std::result::Result<_, _>>()
            .map_err(|e| format_error(e.to_string()))?;
        for name in names {
            let (dir, rest) = if let Some(rest) = name.strip_prefix(MEDIA) {
                (&mut media, rest)
            } else if let Some(rest) = name.strip_prefix(PASSTHROUGH) {
                (&mut passthrough, rest)
            } else {
                continue;
            };
            if rest.is_empty() || rest.ends_with('/') {
                continue;
            }
            check_name(rest)?;
            if let Some(bytes) = read_entry(&mut zip, &name)? {
                dir.insert(rest.to_owned(), bytes);
            }
        }
        Ok(Self {
            kind: manifest.kind,
            title: manifest.title,
            created_ms: manifest.created_ms,
            modified_ms: manifest.modified_ms,
            model,
            crdt,
            versions,
            media,
            passthrough,
        })
    }
}

/// The manifest of a bundle without reading the rest (what kind of document it holds).
///
/// # Errors
///
/// As [`Bundle::from_bytes`] for the manifest.
pub fn manifest(bytes: &[u8]) -> Result<Manifest> {
    let mut zip = ZipArchive::new(Cursor::new(bytes))
        .map_err(|e| format_error(format!("not a Kiln bundle ({e})")))?;
    check_limits(&mut zip)?;
    read_manifest(&mut zip)
}

fn read_manifest(zip: &mut ZipArchive<Cursor<&[u8]>>) -> Result<Manifest> {
    let bytes = read_entry(zip, MANIFEST)?
        .ok_or_else(|| format_error("not a Kiln bundle (no manifest.json)".into()))?;
    // The version first: a newer manifest may not parse as this one.
    let value: serde_json::Value =
        serde_json::from_slice(&bytes).map_err(|e| format_error(format!("manifest.json: {e}")))?;
    if value.get("format").and_then(serde_json::Value::as_str) != Some(FORMAT_NAME) {
        return Err(format_error(
            "not a Kiln bundle (manifest.json names another format)".into(),
        ));
    }
    let version = value
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or_else(|| format_error("manifest.json has no version".into()))?;
    if version > u64::from(BUNDLE_VERSION) {
        return Err(newer(version));
    }
    serde_json::from_value(value).map_err(|e| format_error(format!("manifest.json: {e}")))
}

/// The error for a bundle written by a newer Kiln.
fn newer(version: u64) -> KilnError {
    format_error(format!(
        "this file was made by a newer version of Kiln (bundle version {version}); this version \
         opens bundles up to version {BUNDLE_VERSION}. Update Kiln to open it"
    ))
}

fn check_limits(zip: &mut ZipArchive<Cursor<&[u8]>>) -> Result<()> {
    if zip.len() > MAX_ENTRIES {
        return Err(format_error(format!(
            "the bundle has {} entries (at most {MAX_ENTRIES})",
            zip.len()
        )));
    }
    let (mut total, mut packed): (u64, u64) = (0, 0);
    for i in 0..zip.len() {
        let f = zip
            .by_index_raw(i)
            .map_err(|e| format_error(e.to_string()))?;
        let (size, compressed) = (f.size(), f.compressed_size());
        let name = || {
            f.name()
                .map(std::borrow::Cow::into_owned)
                .unwrap_or_default()
        };
        if size > MAX_ENTRY {
            return Err(format_error(format!("{} is too large", name())));
        }
        if size > RATIO_FREE && size > MAX_RATIO.saturating_mul(compressed) {
            return Err(format_error(format!("{} inflates too far", name())));
        }
        total = total.saturating_add(size);
        packed = packed.saturating_add(compressed);
        if total > MAX_TOTAL {
            return Err(format_error("the bundle is too large".into()));
        }
    }
    if total > RATIO_FREE && total > MAX_RATIO.saturating_mul(packed) {
        return Err(format_error("the bundle inflates too far".into()));
    }
    Ok(())
}

fn read_entry(zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>> {
    let mut f = match zip.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(format_error(format!("{name}: {e}"))),
    };
    let declared = f.size();
    let mut out = Vec::with_capacity(usize::try_from(declared.min(MAX_ENTRY)).unwrap_or(0));
    // Never trust the declared size: read at most one byte more than it.
    (&mut f)
        .take(declared.saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|e| format_error(format!("{name}: {e}")))?;
    if out.len() as u64 != declared {
        return Err(format_error(format!("{name} is damaged")));
    }
    Ok(Some(out))
}

fn put(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    name: &str,
    bytes: &[u8],
    deflate: bool,
) -> Result<()> {
    let base = SimpleFileOptions::default()
        .last_modified_time(DateTime::default())
        .large_file(bytes.len() as u64 >= u64::from(u32::MAX));
    let opts = if deflate {
        base.compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6))
    } else {
        base.compression_method(CompressionMethod::Stored)
    };
    zip.start_file(name, opts).map_err(internal)?;
    zip.write_all(bytes).map_err(internal)
}

/// Checks a `media/` or `passthrough/` entry name: relative, `/`-separated, no `.` or `..`
/// segments, no backslashes or control characters, at most 255 bytes.
///
/// # Errors
///
/// `Format` naming the entry.
pub fn check_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.len() > 255
        || name.starts_with('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
        || name
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..");
    if bad {
        Err(format_error(format!("{name:?} is not a valid entry name")))
    } else {
        Ok(())
    }
}

/// Version ids: 1–64 letters, digits, `-` and `_`.
fn check_id(id: &str) -> Result<()> {
    let ok = !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(format_error(format!("{id:?} is not a valid version id")))
    }
}

fn is_compressed(name: &str) -> bool {
    let ext = name.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "webp"
            | "mp4"
            | "m4a"
            | "mp3"
            | "zip"
            | "docx"
            | "xlsx"
            | "pptx"
            | "docm"
            | "xlsm"
            | "pptm"
            | "dotx"
            | "xltx"
            | "potx"
            | "ppsx"
    )
}

fn format_error(message: String) -> KilnError {
    KilnError::Format {
        format: "Kiln bundle".into(),
        message,
    }
}

fn internal(e: impl std::fmt::Display) -> KilnError {
    KilnError::Internal(format!("writing a bundle: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_protocol::collab::VersionOrigin;

    fn sample() -> Bundle {
        let mut b = Bundle::new(DocKind::Write, b"KDOC\x01\x00model".to_vec());
        b.title = Some("Plan".into());
        b.created_ms = 1_792_000_000_000;
        b.modified_ms = 1_792_000_100_000;
        b.crdt = Some(vec![1, 2, 3]);
        b.versions.push(StoredVersion {
            entry: VersionEntry {
                id: "v1".into(),
                name: Some("Draft".into()),
                author: "Ana".into(),
                time_ms: 1_792_000_050_000,
                origin: VersionOrigin::Save,
            },
            model: b"KDOC\x01\x00old".to_vec(),
        });
        b.media
            .insert("image1.png".into(), vec![0x89, b'P', b'N', b'G']);
        b.passthrough
            .insert("original.docx".into(), b"PK\x03\x04".to_vec());
        b.passthrough
            .insert("charts/chart1.xml".into(), b"<c:chartSpace/>".to_vec());
        b
    }

    #[test]
    fn a_bundle_round_trips_with_every_entry() -> Result<()> {
        let b = sample();
        let bytes = b.to_bytes()?;
        assert_eq!(Bundle::from_bytes(&bytes)?, b);
        assert_eq!(
            Bundle::from_bytes(&b.to_bytes_with(false)?)?,
            b,
            "stored, as checkpoints are"
        );
        let m = manifest(&bytes)?;
        assert_eq!(m.version, BUNDLE_VERSION);
        assert_eq!(m.kind, DocKind::Write);
        assert_eq!(m.versions, 1);
        assert!(m.crdt);
        assert_eq!(m.passthrough, vec!["charts/chart1.xml", "original.docx"]);
        assert!(m.generator.starts_with("Kiln "));
        Ok(())
    }

    #[test]
    fn a_bundle_from_a_newer_kiln_is_refused_with_a_clear_message() -> Result<()> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let manifest = serde_json::json!({
            "format": FORMAT_NAME, "version": 7, "kind": "Write", "generator": "Kiln 9.0",
            "something": "new"
        });
        put(&mut zip, MANIFEST, manifest.to_string().as_bytes(), true)?;
        put(&mut zip, MODEL, b"KDOC", true)?;
        let bytes = zip.finish().map_err(internal)?.into_inner();
        let e = Bundle::from_bytes(&bytes).unwrap_err().to_string();
        assert!(e.contains("newer version of Kiln"), "{e}");
        assert!(
            e.contains("bundle version 7") && e.contains("up to version 1"),
            "{e}"
        );
        Ok(())
    }

    #[test]
    fn foreign_and_damaged_archives_are_refused() -> Result<()> {
        assert!(Bundle::from_bytes(b"not a zip").is_err());
        // A zip that is not a bundle (a .docx).
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        put(&mut zip, "[Content_Types].xml", b"<Types/>", true)?;
        let docx = zip.finish().map_err(internal)?.into_inner();
        let e = Bundle::from_bytes(&docx).unwrap_err().to_string();
        assert!(e.contains("no manifest.json"), "{e}");
        // Another format's manifest.
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        put(
            &mut zip,
            MANIFEST,
            br#"{"format":"other","version":1}"#,
            true,
        )?;
        let other = zip.finish().map_err(internal)?.into_inner();
        assert!(Bundle::from_bytes(&other).is_err());
        // No model.
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let m = serde_json::json!({"format": FORMAT_NAME, "version": 1, "kind": "Grid",
            "generator": "Kiln"});
        put(&mut zip, MANIFEST, m.to_string().as_bytes(), true)?;
        let empty = zip.finish().map_err(internal)?.into_inner();
        let e = Bundle::from_bytes(&empty).unwrap_err().to_string();
        assert!(e.contains("no model.bin"), "{e}");
        Ok(())
    }

    #[test]
    fn entry_names_never_leave_the_archive() {
        for bad in [
            "", "../x", "a/../b", "/abs", "a\\b", "a//b", "./a", "a\u{7}",
        ] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
        for good in ["a.png", "charts/chart1.xml", "word/media/image 1.png"] {
            assert!(check_name(good).is_ok(), "{good:?}");
        }
        let mut b = Bundle::new(DocKind::Grid, vec![]);
        b.media.insert("../evil".into(), vec![]);
        assert!(b.to_bytes().is_err());
        assert!(check_id("v-1_a").is_ok() && check_id("v/1").is_err() && check_id("").is_err());
    }

    #[test]
    fn a_zip_bomb_is_refused_before_it_inflates() -> Result<()> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let m = serde_json::json!({"format": FORMAT_NAME, "version": 1, "kind": "Deck",
            "generator": "Kiln"});
        put(&mut zip, MANIFEST, m.to_string().as_bytes(), true)?;
        put(&mut zip, MODEL, &vec![0u8; 8 << 20], true)?;
        let bomb = zip.finish().map_err(internal)?.into_inner();
        let e = Bundle::from_bytes(&bomb).unwrap_err().to_string();
        assert!(e.contains("inflates too far"), "{e}");
        Ok(())
    }

    #[test]
    fn many_small_entries_of_zeros_are_refused_too() -> Result<()> {
        // Each entry is under the old 1 MiB threshold; together they would inflate a thousand
        // times over.
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        let m = serde_json::json!({"format": FORMAT_NAME, "version": 1, "kind": "Deck",
            "generator": "Kiln"});
        put(&mut zip, MANIFEST, m.to_string().as_bytes(), true)?;
        put(&mut zip, MODEL, b"KDCK", true)?;
        for i in 0..64 {
            put(
                &mut zip,
                &format!("{MEDIA}z{i}.bin"),
                &vec![0u8; 512 << 10],
                true,
            )?;
        }
        let bomb = zip.finish().map_err(internal)?.into_inner();
        let e = Bundle::from_bytes(&bomb).unwrap_err().to_string();
        assert!(e.contains("inflates too far"), "{e}");
        assert!(
            manifest(&bomb).is_err(),
            "the manifest alone is not read either"
        );
        Ok(())
    }

    #[test]
    fn real_bundles_stay_within_the_limits() -> Result<()> {
        // A deflated model of text-like content and stored media.
        let words = ["glaze ", "kiln ", "cone ", "ten ", "clay ", "fire "];
        let mut seed = 7u32;
        let mut b = sample();
        b.model = (0..200_000)
            .flat_map(|_| {
                seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12_345);
                words[(seed >> 16) as usize % words.len()].bytes()
            })
            .collect();
        b.media.insert(
            "photo.jpg".into(),
            (0..300_000u32)
                .map(|i| i.wrapping_mul(2_654_435_761).to_le_bytes()[3])
                .collect(),
        );
        assert_eq!(Bundle::from_bytes(&b.to_bytes()?)?, b);
        Ok(())
    }
}
