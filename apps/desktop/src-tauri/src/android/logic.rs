// SPDX-License-Identifier: AGPL-3.0-or-later
//! The parts of the Android shell that need no Android, so they are tested on every platform:
//! document handles for content URIs, MIME types for the pickers and for sending, the print
//! job's name, the routes documents open on and the system bars' colours.

use kiln_protocol::{DocKind, FileFilter};

use crate::files::ASSOCIATIONS;

/// What separates a content URI from the name Kiln shows for its document in a handle.
pub const NAME_MARK: &str = "#/";

/// The name a document gets when its provider gives none.
pub const UNNAMED: &str = "Document";

/// Whether `path` is a document behind a content URI (the storage access framework, another
/// app's share) rather than a file.
#[must_use]
pub fn is_content(path: &str) -> bool {
    path.starts_with("content://")
}

/// A name that is safe as the last segment of a handle: no separators, no control characters.
#[must_use]
pub fn safe_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if c == '/' || c == '\\' || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        UNNAMED.to_owned()
    } else {
        trimmed.to_owned()
    }
}

/// The handle of a document behind a content URI: the URI, [`NAME_MARK`] and its name, so the
/// core finds the document's name and format at the end of the path as it does for a file.
#[must_use]
pub fn handle(uri: &str, name: &str) -> String {
    format!("{uri}{NAME_MARK}{}", safe_name(name))
}

/// The content URI of a handle (a handle without a name is a URI already).
#[must_use]
pub fn uri_of(handle: &str) -> &str {
    if !is_content(handle) {
        return handle;
    }
    handle.rfind(NAME_MARK).map_or(handle, |i| &handle[..i])
}

/// The name in a handle.
#[must_use]
pub fn name_of(handle: &str) -> Option<&str> {
    if !is_content(handle) {
        return None;
    }
    handle
        .rfind(NAME_MARK)
        .map(|i| &handle[i + NAME_MARK.len()..])
}

/// MIME types of the formats the core reads or writes besides the associated documents.
const OTHER_TYPES: &[(&str, &str)] = &[
    ("dotm", "application/vnd.ms-word.template.macroEnabled.12"),
    (
        "ppsx",
        "application/vnd.openxmlformats-officedocument.presentationml.slideshow",
    ),
    ("txt", "text/plain"),
    ("md", "text/markdown"),
    ("html", "text/html"),
    ("htm", "text/html"),
    ("pdf", "application/pdf"),
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("bmp", "image/bmp"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
    ("tif", "image/tiff"),
    ("tiff", "image/tiff"),
    ("emf", "image/emf"),
    ("wmf", "image/wmf"),
    ("mp3", "audio/mpeg"),
    ("wav", "audio/wav"),
    ("m4a", "audio/mp4"),
    ("mp4", "video/mp4"),
    ("webm", "video/webm"),
    ("mov", "video/quicktime"),
];

/// Formats providers often do not know: they list them as plain bytes or as ZIP archives.
const UNKNOWN_TO_PROVIDERS: &[&str] = &["kwrite", "kgrid", "kdeck", "md"];

/// The MIME type of a file extension (any case, with or without the dot).
#[must_use]
pub fn mime_of_extension(ext: &str) -> &'static str {
    let ext = ext.strip_prefix('.').unwrap_or(ext).to_ascii_lowercase();
    ASSOCIATIONS
        .iter()
        .map(|a| (a.ext, a.mime))
        .chain(OTHER_TYPES.iter().copied())
        .find(|(e, _)| *e == ext)
        .map_or("application/octet-stream", |(_, m)| m)
}

/// The MIME type of a file or handle, from the extension of its name.
#[must_use]
pub fn mime_of_name(name: &str) -> &'static str {
    let last = name.rsplit(['/', '\\']).next().unwrap_or(name);
    last.rsplit_once('.')
        .map_or("application/octet-stream", |(_, ext)| {
            mime_of_extension(ext)
        })
}

/// The extension Kiln gives a document whose name has none, from the MIME type another app gave.
#[must_use]
pub fn extension_of_mime(mime: &str) -> Option<&'static str> {
    let mime = mime.split(';').next().unwrap_or(mime).trim();
    if mime.eq_ignore_ascii_case("text/comma-separated-values") {
        return Some("csv");
    }
    ASSOCIATIONS
        .iter()
        .map(|a| (a.ext, a.mime))
        .chain(OTHER_TYPES.iter().copied())
        .find(|(_, m)| m.eq_ignore_ascii_case(mime))
        .map(|(e, _)| e)
}

/// The name a document handed to Kiln is opened under: the provider's name, with the extension
/// of its MIME type when the name has no extension Kiln knows.
#[must_use]
pub fn document_name(name: &str, mime: &str) -> String {
    let name = safe_name(name);
    if kiln_protocol::FileFormat::from_path(&name).is_some() {
        return name;
    }
    match extension_of_mime(mime) {
        Some(ext) => format!("{name}.{ext}"),
        None => name,
    }
}

/// The MIME types the document picker offers for the core's filters: each extension's type, and
/// for formats providers do not know (Kiln's own, Markdown) also plain bytes and ZIP archives.
#[must_use]
pub fn picker_mimes(filters: &[FileFilter]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |m: &str| {
        if !out.iter().any(|x| x == m) {
            out.push(m.to_owned());
        }
    };
    for f in filters {
        for ext in &f.extensions {
            let ext = ext.to_ascii_lowercase();
            push(mime_of_extension(&ext));
            if ext == "csv" {
                push("text/comma-separated-values");
            }
            if UNKNOWN_TO_PROVIDERS.contains(&ext.as_str()) {
                push("application/octet-stream");
                push("application/zip");
            }
        }
    }
    out
}

/// The MIME type of the file a Save As dialog creates: the first filter's first extension, else
/// the suggested name's.
#[must_use]
pub fn save_mime(suggested_name: &str, filters: &[FileFilter]) -> &'static str {
    filters
        .first()
        .and_then(|f| f.extensions.first())
        .map_or_else(|| mime_of_name(suggested_name), |e| mime_of_extension(e))
}

/// The name a print job shows (in the print dialog and the printer's queue): the document's name,
/// from the PDF the core writes (`<name>-<milliseconds>.pdf`, `kiln_session::print::write_temp`).
#[must_use]
pub fn print_job_name(path: &str) -> String {
    let file = path.rsplit(['/', '\\']).next().unwrap_or(path);
    let stem = file
        .strip_suffix(".pdf")
        .or_else(|| file.strip_suffix(".PDF"))
        .unwrap_or(file);
    let stem = match stem.rsplit_once('-') {
        Some((name, stamp))
            if !name.is_empty()
                && stamp.len() >= 10
                && stamp.bytes().all(|b| b.is_ascii_digit()) =>
        {
            name
        }
        _ => stem,
    };
    let stem = stem.trim();
    if stem.is_empty() {
        UNNAMED.to_owned()
    } else {
        stem.to_owned()
    }
}

/// Whether a file Kiln wrote may be printed: a PDF in the core's print folder.
#[must_use]
pub fn is_print_file(path: &str) -> bool {
    let p = std::path::Path::new(path);
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
        && p.parent()
            .and_then(|d| d.file_name())
            .is_some_and(|d| d == "kiln-print")
}

/// The route a document opens on (`#/write?doc=3`).
#[must_use]
pub fn doc_route(kind: DocKind, doc: u32) -> String {
    let app = match kind {
        DocKind::Grid => "grid",
        DocKind::Deck => "deck",
        DocKind::Scratch | DocKind::Write | DocKind::Notes => "write",
    };
    format!("#/{app}?doc={doc}")
}

/// A `#RRGGBB` colour (the page's `theme-color`), normalised to upper case.
#[must_use]
pub fn bar_color(color: &str) -> Option<String> {
    let hex = color.trim().strip_prefix('#')?;
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| format!("#{}", hex.to_ascii_uppercase()))
}

/// Whether a `#RRGGBB` colour is light, so the system bars draw dark icons on it.
#[must_use]
pub fn is_light(color: &str) -> bool {
    let Some(hex) = bar_color(color) else {
        return true;
    };
    let channel = |i: usize| f64::from(u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0)) / 255.0;
    let linear = |c: f64| {
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance =
        0.2126 * linear(channel(1)) + 0.7152 * linear(channel(3)) + 0.0722 * linear(channel(5));
    luminance > 0.4
}

/// The URL schemes links in documents may open with (the desktop host's list).
pub const EXTERNAL_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Whether `url` may be opened in the system's handler: web pages and e-mail addresses only.
#[must_use]
pub fn may_open_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    if !EXTERNAL_SCHEMES.contains(&scheme.as_str()) {
        return false;
    }
    if scheme == "mailto" {
        return !rest.trim().is_empty();
    }
    rest.starts_with("//")
        && rest.len() > 2
        && !url.chars().any(|c| c.is_whitespace() || c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;
    use kiln_protocol::FileFormat;

    #[test]
    fn handles_carry_the_name_the_core_reads_the_format_from() {
        let uri = "content://com.android.providers.downloads.documents/document/msf%3A1000000031";
        let h = handle(uri, "Quarterly report.docx");
        assert!(is_content(&h));
        assert_eq!(uri_of(&h), uri);
        assert_eq!(name_of(&h), Some("Quarterly report.docx"));
        assert_eq!(FileFormat::from_path(&h), Some(FileFormat::Docx));
        // A name with separators cannot hide the URI or the extension.
        let h = handle(uri, "a/b\\c.xlsx");
        assert_eq!(uri_of(&h), uri);
        assert_eq!(name_of(&h), Some("a_b_c.xlsx"));
        assert_eq!(FileFormat::from_path(&h), Some(FileFormat::Xlsx));
        assert_eq!(name_of(&handle(uri, "  ")), Some(UNNAMED));
        // Files are not handles.
        assert_eq!(
            uri_of("/data/user/0/x/files/a.docx"),
            "/data/user/0/x/files/a.docx"
        );
        assert_eq!(name_of("/data/a.docx"), None);
        assert_eq!(uri_of(uri), uri);
    }

    #[test]
    fn documents_without_an_extension_get_one_from_their_type() {
        assert_eq!(
            document_name(
                "Budget",
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            ),
            "Budget.xlsx"
        );
        assert_eq!(
            document_name("Budget.xlsx", "application/octet-stream"),
            "Budget.xlsx"
        );
        assert_eq!(
            document_name("data", "text/comma-separated-values"),
            "data.csv"
        );
        assert_eq!(
            document_name("notes", "text/plain; charset=utf-8"),
            "notes.txt"
        );
        assert_eq!(document_name("blob", "application/x-unknown"), "blob");
    }

    #[test]
    fn the_picker_offers_the_types_of_the_core_filters() {
        let all = FileFilter {
            name: "All supported files".into(),
            extensions: ["docx", "CSV", "kgrid", "pptx"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
        };
        let m = picker_mimes(&[all]);
        assert_eq!(
            m,
            [
                "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
                "text/csv",
                "text/comma-separated-values",
                "application/x-kiln-grid",
                "application/octet-stream",
                "application/zip",
                "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            ]
        );
        let pictures = FileFilter {
            name: "Pictures".into(),
            extensions: vec!["png".into(), "jpg".into(), "jpeg".into()],
        };
        assert_eq!(picker_mimes(&[pictures]), ["image/png", "image/jpeg"]);
        let save = [FileFilter {
            name: "Docx".into(),
            extensions: vec!["docx".into()],
        }];
        assert_eq!(
            save_mime("Report.docx", &save),
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        );
        assert_eq!(save_mime("Report.pdf", &[]), "application/pdf");
    }

    #[test]
    fn print_jobs_are_named_after_the_document() {
        assert_eq!(
            print_job_name(
                "/data/user/0/io.github.zahakj.kiln/cache/kiln-print/Quarterly report-1761830000123.pdf"
            ),
            "Quarterly report"
        );
        assert_eq!(print_job_name("/c/kiln-print/Plan-B.pdf"), "Plan-B");
        assert_eq!(
            print_job_name("/c/kiln-print/-1761830000123.pdf"),
            "-1761830000123"
        );
        assert_eq!(print_job_name("/c/kiln-print/.pdf"), UNNAMED);
        assert!(is_print_file("/c/kiln-print/Plan-1761830000123.pdf"));
        assert!(!is_print_file("/c/elsewhere/Plan.pdf"));
        assert!(!is_print_file("/c/kiln-print/run.sh"));
    }

    #[test]
    fn documents_open_on_their_apps_route() {
        assert_eq!(doc_route(DocKind::Write, 3), "#/write?doc=3");
        assert_eq!(doc_route(DocKind::Grid, 4), "#/grid?doc=4");
        assert_eq!(doc_route(DocKind::Deck, 5), "#/deck?doc=5");
    }

    #[test]
    fn the_bars_take_the_title_bars_colour_with_readable_icons() {
        assert_eq!(bar_color(" #f6efe6 ").as_deref(), Some("#F6EFE6"));
        assert_eq!(bar_color("f6efe6"), None);
        assert_eq!(bar_color("#fff"), None);
        assert_eq!(bar_color("#GGGGGG"), None);
        assert!(is_light("#F6EFE6"));
        assert!(!is_light("#17110E"));
        assert!(!is_light("#3159C6"));
    }

    #[test]
    fn links_open_only_for_web_pages_and_mail() {
        assert!(may_open_url("https://example.com/a"));
        assert!(may_open_url("mailto:zo@example.com"));
        assert!(!may_open_url("mailto:"));
        assert!(!may_open_url("file:///sdcard/a.docx"));
        assert!(!may_open_url("intent://scan/#Intent;scheme=zxing;end"));
        assert!(!may_open_url("javascript:alert(1)"));
        assert!(!may_open_url("https://exa mple.com"));
    }

    /// The manifest opens every type the desktop registers, by MIME type and by extension, for
    /// VIEW and EDIT, and takes every one from the share sheet (SEND).
    #[test]
    fn the_manifest_opens_every_associated_document() {
        let manifest = include_str!("../../gen/android/app/src/main/AndroidManifest.xml");
        let filters: Vec<&str> = manifest.split("<intent-filter>").skip(1).collect();
        let with = |action: &str| -> Vec<&str> {
            filters
                .iter()
                .copied()
                .filter(|f| f.contains(&format!("android.intent.action.{action}\"")))
                .collect()
        };
        let (view, edit, send) = (with("VIEW"), with("EDIT"), with("SEND"));
        assert_eq!(view.len(), 2);
        assert_eq!(edit.len(), 2);
        assert_eq!(send.len(), 1);
        for a in ASSOCIATIONS {
            let mime = format!("android:mimeType=\"{}\"", a.mime);
            assert!(view.iter().any(|f| f.contains(&mime)), "VIEW {}", a.mime);
            assert!(edit.iter().any(|f| f.contains(&mime)), "EDIT {}", a.mime);
            assert!(send[0].contains(&mime), "SEND {}", a.mime);
            let ext = format!("android:pathPattern=\".*\\\\.{}\"", a.ext);
            assert!(view.iter().any(|f| f.contains(&ext)), "extension {}", a.ext);
        }
        assert!(manifest.contains("android:windowSoftInputMode=\"adjustResize\""));
        assert!(manifest.contains("android:roundIcon=\"@mipmap/ic_launcher_round\""));
        assert!(!manifest.contains("leanback"));
    }

    /// The window and the system bars start in the colour the page gives them: the tokens'
    /// `bg-surface-2`, light and dark.
    #[test]
    fn the_window_colours_are_the_tokens() {
        let tokens = include_str!("../../../../../packages/tokens/src/generated/kit.ts");
        let bg = |theme: &str| -> String {
            let block = tokens
                .split(&format!("    {theme}: {{"))
                .nth(1)
                .expect("theme in kit.ts");
            let line = block
                .lines()
                .find(|l| l.trim_start().starts_with("bg:"))
                .expect("bg");
            line.split('\'')
                .nth(1)
                .expect("colour")
                .to_ascii_uppercase()
        };
        let light = include_str!("../../gen/android/app/src/main/res/values/colors.xml");
        let dark = include_str!("../../gen/android/app/src/main/res/values-night/colors.xml");
        let window = |xml: &str| -> String {
            let line = xml
                .lines()
                .find(|l| l.contains("name=\"kiln_window\""))
                .expect("kiln_window");
            let argb = line
                .split('>')
                .nth(1)
                .and_then(|s| s.split('<').next())
                .expect("value");
            format!("#{}", &argb[3..]).to_ascii_uppercase()
        };
        assert_eq!(window(light), bg("light"));
        assert_eq!(window(dark), bg("dark"));
    }

    /// The release build is signed with the key in KILN_KEYSTORE, else with the debug key, and
    /// Gradle stays within its memory limit.
    #[test]
    fn gradle_signs_from_the_environment_and_stays_small() {
        let app = include_str!("../../gen/android/app/build.gradle.kts");
        for needle in [
            "System.getenv(\"KILN_KEYSTORE\")",
            "System.getenv(\"KILN_KEYSTORE_PASSWORD\")",
            "KILN_KEY_ALIAS",
            "KILN_KEY_PASSWORD",
            "signingConfigs.findByName(\"release\") ?: signingConfigs.getByName(\"debug\")",
            "applicationId = \"io.github.zahakj.kiln\"",
            "minSdk = 26",
            "targetSdk = 36",
            "is not a file: the release keystore is missing",
            "KILN_KEYSTORE_PASSWORD is empty",
        ] {
            assert!(app.contains(needle), "{needle}");
        }
        let props = include_str!("../../gen/android/gradle.properties");
        for needle in [
            "org.gradle.jvmargs=-Xmx2048m -XX:MaxMetaspaceSize=512m",
            "org.gradle.parallel=false",
            "org.gradle.workers.max=2",
            "org.gradle.daemon=false",
        ] {
            assert!(props.contains(needle), "{needle}");
        }
    }

    /// A tagged release is never signed with the debug key: the workflow stops without the
    /// key's secrets and checks the APKs with `--release`, which fails a debug signer.
    #[test]
    fn a_tagged_release_needs_the_release_key() {
        let workflow = include_str!("../../../../../.github/workflows/release.yml");
        assert!(workflow.contains("a tagged release is never signed with a debug key"));
        assert!(workflow.contains("refs/tags/v*) release=--release"));
        assert!(workflow.contains("bash tools/verify-apk.sh $release"));
        let verify = include_str!("../../../../../tools/verify-apk.sh");
        assert!(verify.contains("a release is signed with the release key"));
    }
}
