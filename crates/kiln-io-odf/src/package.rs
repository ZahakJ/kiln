// SPDX-License-Identifier: AGPL-3.0-or-later
//! ODF packages (ODF 1.3 Part 2): a zip with `mimetype` first and stored, `META-INF/manifest.xml`
//! listing every file with its media type, and the XML streams `content.xml`, `styles.xml`,
//! `meta.xml` and `settings.xml`.
//!
//! Reading enforces the same zip-bomb limits as the OOXML reader before anything is inflated,
//! refuses encrypted entries (ODF encryption is declared in the manifest) and accepts packages
//! without a `mimetype` entry by falling back to the manifest's root media type. Writing puts
//! `mimetype` first and uncompressed (so `file(1)` and other tools recognise the package),
//! deflates the XML streams, stores already-compressed media, and stamps every entry with the
//! DOS epoch so output is reproducible.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, DateTime, System, ZipArchive, ZipWriter};

use crate::OdfError;
use crate::xml::{XmlOut, kids, parse_part};

/// The three document kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum OdfKind {
    /// `.odt`: a text document.
    Text,
    /// `.ods`: a spreadsheet.
    Spreadsheet,
    /// `.odp`: a presentation.
    Presentation,
}

impl OdfKind {
    /// Every kind.
    pub const ALL: [OdfKind; 3] = [Self::Text, Self::Spreadsheet, Self::Presentation];

    /// The package media type (the content of `mimetype`).
    #[must_use]
    pub const fn media_type(self) -> &'static str {
        match self {
            Self::Text => "application/vnd.oasis.opendocument.text",
            Self::Spreadsheet => "application/vnd.oasis.opendocument.spreadsheet",
            Self::Presentation => "application/vnd.oasis.opendocument.presentation",
        }
    }

    /// The file extension, without the dot.
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Text => "odt",
            Self::Spreadsheet => "ods",
            Self::Presentation => "odp",
        }
    }

    /// The kind of a media type; templates (`…-template`) count as their document kind.
    #[must_use]
    pub fn from_media_type(media: &str) -> Option<Self> {
        let m = media.trim().trim_end_matches("-template");
        Self::ALL.into_iter().find(|k| k.media_type() == m)
    }

    /// The kind of a file extension (`odt`, `ott`, `ods`, `ots`, `odp`, `otp`; any case).
    #[must_use]
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "odt" | "ott" => Some(Self::Text),
            "ods" | "ots" => Some(Self::Spreadsheet),
            "odp" | "otp" => Some(Self::Presentation),
            _ => None,
        }
    }

    /// The `office:body` child holding the content.
    #[must_use]
    pub const fn body_element(self) -> &'static str {
        match self {
            Self::Text => "office:text",
            Self::Spreadsheet => "office:spreadsheet",
            Self::Presentation => "office:presentation",
        }
    }
}

impl std::fmt::Display for OdfKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.extension())
    }
}

/// Limits enforced before inflating (the defaults match `kiln_ooxml::opc::OpcLimits`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// Maximum number of entries.
    pub max_entries: usize,
    /// Maximum declared size of one entry.
    pub max_entry_size: u64,
    /// Maximum declared size of all entries.
    pub max_total_size: u64,
    /// Maximum compression ratio of an entry larger than `ratio_threshold`.
    pub max_compression_ratio: u64,
    /// Entries smaller than this are exempt from the ratio check.
    pub ratio_threshold: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_entries: 65_536,
            max_entry_size: 2 << 30,
            max_total_size: 2 << 30,
            max_compression_ratio: 1000,
            ratio_threshold: 1 << 20,
        }
    }
}

/// An opened package: every file inflated, by its path inside the package.
#[derive(Debug, Clone, Default)]
pub struct Package {
    /// Files in package order (`content.xml`, `Pictures/1.png`, …), directories left out.
    pub files: Vec<(String, Vec<u8>)>,
    /// Media types from the manifest, by path.
    pub media_types: BTreeMap<String, String>,
    /// The document kind.
    pub kind: Option<OdfKind>,
    /// The root media type as declared (`mimetype`, or the manifest's `/` entry).
    pub media_type: String,
}

impl Package {
    /// Opens a package with the default limits.
    ///
    /// # Errors
    ///
    /// [`OdfError::NotZip`], [`OdfError::Corrupt`], [`OdfError::ZipBomb`],
    /// [`OdfError::Encrypted`], or [`OdfError::NotOdf`] when neither `mimetype` nor the manifest
    /// names an ODF document.
    pub fn open(bytes: &[u8]) -> Result<Self, OdfError> {
        Self::open_with(bytes, Limits::default())
    }

    /// Opens a package with limits.
    ///
    /// # Errors
    ///
    /// As [`Package::open`].
    pub fn open_with(bytes: &[u8], limits: Limits) -> Result<Self, OdfError> {
        if bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]) {
            return Err(OdfError::Encrypted);
        }
        if bytes.len() < 4 || !bytes.starts_with(b"PK") {
            return Err(OdfError::NotZip);
        }
        let mut archive = ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| OdfError::Corrupt(format!("zip: {e}")))?;
        let mut total = 0u64;
        let mut count = 0usize;
        let mut pkg = Package::default();
        for i in 0..archive.len() {
            let entry = archive
                .by_index(i)
                .map_err(|e| OdfError::Corrupt(format!("zip entry {i}: {e}")))?;
            let name = entry.name().to_owned();
            if entry.is_dir() || name.ends_with('/') {
                continue;
            }
            count += 1;
            if count > limits.max_entries {
                return Err(OdfError::ZipBomb(format!(
                    "more than {} entries",
                    limits.max_entries
                )));
            }
            let size = entry.size();
            if size > limits.max_entry_size {
                return Err(OdfError::ZipBomb(format!(
                    "{name} declares {size} bytes (limit {})",
                    limits.max_entry_size
                )));
            }
            total = total.saturating_add(size);
            if total > limits.max_total_size {
                return Err(OdfError::ZipBomb(format!(
                    "the package expands beyond {} bytes",
                    limits.max_total_size
                )));
            }
            let compressed = entry.compressed_size().max(1);
            if size > limits.ratio_threshold
                && size > limits.max_compression_ratio.saturating_mul(compressed)
            {
                return Err(OdfError::ZipBomb(format!(
                    "{name} has a suspicious compression ratio"
                )));
            }
            if entry.encrypted() {
                return Err(OdfError::Encrypted);
            }
            if pkg.files.iter().any(|(n, _)| *n == name) {
                return Err(OdfError::Corrupt(format!("{name} appears twice")));
            }
            let capacity = usize::try_from(size.min(64 << 20)).unwrap_or(0);
            let mut buf = Vec::with_capacity(capacity);
            entry
                .take(size.saturating_add(1))
                .read_to_end(&mut buf)
                .map_err(|e| OdfError::Corrupt(format!("{name}: {e}")))?;
            if buf.len() as u64 > size {
                return Err(OdfError::ZipBomb(format!(
                    "{name} inflates beyond its declared size"
                )));
            }
            pkg.files.push((name, buf));
        }
        if let Some(m) = pkg.file("META-INF/manifest.xml") {
            let root = parse_part("META-INF/manifest.xml", m)?;
            for e in kids(&root, "manifest:file-entry") {
                if e.elements().any(|c| c.name == "manifest:encryption-data") {
                    return Err(OdfError::Encrypted);
                }
                if let (Some(path), Some(media)) =
                    (e.attr("manifest:full-path"), e.attr("manifest:media-type"))
                {
                    pkg.media_types.insert(path.to_owned(), media.to_owned());
                }
            }
        }
        let declared = pkg
            .file("mimetype")
            .map(|m| String::from_utf8_lossy(m).trim().to_owned())
            .filter(|m| !m.is_empty())
            .or_else(|| pkg.media_types.get("/").cloned())
            .unwrap_or_default();
        pkg.kind = OdfKind::from_media_type(&declared);
        pkg.media_type = declared;
        if pkg.kind.is_none() {
            return Err(OdfError::NotOdf(if pkg.media_type.is_empty() {
                "no mimetype and no manifest root entry".to_owned()
            } else {
                format!("media type {}", pkg.media_type)
            }));
        }
        Ok(pkg)
    }

    /// A file's bytes.
    #[must_use]
    pub fn file(&self, path: &str) -> Option<&[u8]> {
        self.files
            .iter()
            .find(|(n, _)| n == path)
            .map(|(_, b)| b.as_slice())
    }

    /// The media type the manifest declares for a file, or one guessed from its extension.
    #[must_use]
    pub fn media_type_of(&self, path: &str) -> String {
        self.media_types
            .get(path)
            .cloned()
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| media_type_for(path).to_owned())
    }
}

/// The media type of a file from its extension (images, XML, fallback octet stream).
#[must_use]
pub fn media_type_for(path: &str) -> &'static str {
    let ext = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match ext.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" | "jpe" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tif" | "tiff" => "image/tiff",
        "svg" => "image/svg+xml",
        "emf" => "image/x-emf",
        "wmf" => "image/x-wmf",
        "webp" => "image/webp",
        "xml" => "text/xml",
        "rdf" => "application/rdf+xml",
        "mp4" => "video/mp4",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        _ => "application/octet-stream",
    }
}

/// The extension for an image media type.
#[must_use]
pub fn extension_for(media: &str) -> &'static str {
    match media {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tif",
        "image/svg+xml" => "svg",
        "image/x-emf" | "image/emf" => "emf",
        "image/x-wmf" | "image/wmf" => "wmf",
        "image/webp" => "webp",
        _ => "bin",
    }
}

/// Builds a package.
#[derive(Debug)]
pub struct PackageWriter {
    kind: OdfKind,
    files: Vec<(String, String, Vec<u8>)>,
}

impl PackageWriter {
    /// A package of a kind.
    #[must_use]
    pub fn new(kind: OdfKind) -> Self {
        Self {
            kind,
            files: Vec::new(),
        }
    }

    /// Adds a file (a later file with the same path replaces the earlier one).
    pub fn add(&mut self, path: &str, media_type: &str, bytes: Vec<u8>) {
        self.files.retain(|(p, _, _)| p != path);
        self.files
            .push((path.to_owned(), media_type.to_owned(), bytes));
    }

    /// Whether a path is taken.
    #[must_use]
    pub fn has(&self, path: &str) -> bool {
        self.files.iter().any(|(p, _, _)| p == path)
    }

    /// The paths written so far.
    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.files.iter().map(|(p, _, _)| p.as_str())
    }

    fn manifest(&self) -> Vec<u8> {
        let mut w = XmlOut::new();
        w.root("manifest:manifest", &["manifest"])
            .attr("manifest:version", "1.3");
        w.start("manifest:file-entry")
            .attr("manifest:full-path", "/")
            .attr("manifest:version", "1.3")
            .attr("manifest:media-type", self.kind.media_type())
            .end();
        for (path, media, _) in &self.files {
            w.start("manifest:file-entry")
                .attr("manifest:full-path", path)
                .attr("manifest:media-type", media)
                .end();
        }
        w.finish()
    }

    /// Writes the zip.
    ///
    /// # Errors
    ///
    /// [`OdfError::Write`] when the archive cannot be written.
    pub fn finish(self) -> Result<Vec<u8>, OdfError> {
        let werr = |e: zip::result::ZipError| OdfError::Write(e.to_string());
        let ioerr = |e: std::io::Error| OdfError::Write(e.to_string());
        let size: usize = self.files.iter().map(|(_, _, b)| b.len()).sum();
        let mut zip = ZipWriter::new(Cursor::new(Vec::with_capacity(size / 3 + 4096)));
        let base = SimpleFileOptions::default()
            .last_modified_time(DateTime::default())
            .system(System::Dos);
        let stored = base.compression_method(CompressionMethod::Stored);
        let deflated = base
            .compression_method(CompressionMethod::Deflated)
            .compression_level(Some(6));
        zip.start_file("mimetype", stored).map_err(werr)?;
        zip.write_all(self.kind.media_type().as_bytes())
            .map_err(ioerr)?;
        let manifest = self.manifest();
        for (path, media, bytes) in &self.files {
            let compressed_already = media.starts_with("image/")
                && !media.ends_with("svg+xml")
                && !media.ends_with("bmp")
                && !media.ends_with("emf")
                && !media.ends_with("wmf");
            let opts = if compressed_already {
                stored
            } else {
                deflated
            }
            .large_file(bytes.len() as u64 >= u64::from(u32::MAX));
            zip.start_file(path.as_str(), opts).map_err(werr)?;
            zip.write_all(bytes).map_err(ioerr)?;
        }
        zip.start_file("META-INF/manifest.xml", deflated)
            .map_err(werr)?;
        zip.write_all(&manifest).map_err(ioerr)?;
        Ok(zip.finish().map_err(werr)?.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn written_packages_open_with_mimetype_first_and_stored() {
        let mut w = PackageWriter::new(OdfKind::Text);
        w.add("content.xml", "text/xml", b"<a/>".to_vec());
        w.add("Pictures/1.png", "image/png", vec![1, 2, 3]);
        let bytes = w.finish().unwrap();
        // The first local header names `mimetype`, stored (method 0), right after the header.
        assert_eq!(&bytes[0..4], b"PK\x03\x04");
        assert_eq!(u16::from_le_bytes([bytes[8], bytes[9]]), 0);
        assert_eq!(&bytes[30..38], b"mimetype");
        assert_eq!(
            &bytes[38..38 + 39],
            b"application/vnd.oasis.opendocument.text"
        );
        let p = Package::open(&bytes).unwrap();
        assert_eq!(p.kind, Some(OdfKind::Text));
        assert_eq!(p.file("content.xml"), Some(&b"<a/>"[..]));
        assert_eq!(p.media_type_of("Pictures/1.png"), "image/png");
        assert_eq!(p.media_types.get("/").map(String::as_str), Some(OdfKind::Text.media_type()));
        // Deterministic.
        let mut w2 = PackageWriter::new(OdfKind::Text);
        w2.add("content.xml", "text/xml", b"<a/>".to_vec());
        w2.add("Pictures/1.png", "image/png", vec![1, 2, 3]);
        assert_eq!(w2.finish().unwrap(), bytes);
    }

    #[test]
    fn non_packages_are_refused() {
        assert!(matches!(Package::open(b"hello"), Err(OdfError::NotZip)));
        assert!(matches!(
            Package::open(&[0xd0, 0xcf, 0x11, 0xe0, 0, 0]),
            Err(OdfError::Encrypted)
        ));
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        zip.start_file("mimetype", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"application/zip").unwrap();
        let bytes = zip.finish().unwrap().into_inner();
        assert!(matches!(Package::open(&bytes), Err(OdfError::NotOdf(_))));
    }

    #[test]
    fn kinds_parse_extensions_and_templates() {
        assert_eq!(OdfKind::from_extension("OTT"), Some(OdfKind::Text));
        assert_eq!(
            OdfKind::from_media_type("application/vnd.oasis.opendocument.spreadsheet-template"),
            Some(OdfKind::Spreadsheet)
        );
        assert_eq!(OdfKind::from_extension("docx"), None);
    }
}
