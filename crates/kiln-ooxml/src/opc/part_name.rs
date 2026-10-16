// SPDX-License-Identifier: AGPL-3.0-or-later
//! OPC part names (ECMA-376 Part 2 §6.2.2): absolute, `/`-separated, compared case-insensitively.

/// A part name that breaks the OPC grammar Kiln relies on.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("invalid part name {name:?}: {reason}")]
pub struct PartNameError {
    /// The offending name.
    pub name: String,
    /// What is wrong with it.
    pub reason: &'static str,
}

/// Decodes `%XX` escapes as UTF-8, like JavaScript's `decodeURIComponent`. `None` when an escape is
/// malformed or the bytes are not UTF-8 (callers then keep the text as written).
#[must_use]
pub fn percent_decode(s: &str) -> Option<String> {
    if !s.contains('%') {
        return Some(s.to_owned());
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes.get(i + 1..i + 3)?;
            let hi = (hex[0] as char).to_digit(16)?;
            let lo = (hex[1] as char).to_digit(16)?;
            // Two hex digits always fit a byte.
            out.push(u8::try_from(hi * 16 + lo).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Turns a zip entry name (`word/document.xml`) into a part name (`/word/document.xml`):
/// backslashes become slashes and percent-escapes are decoded where valid.
#[must_use]
pub fn zip_name_to_part_name(zip_name: &str) -> String {
    let n = zip_name.replace('\\', "/");
    let n = percent_decode(&n).unwrap_or(n);
    if n.starts_with('/') {
        n
    } else {
        format!("/{n}")
    }
}

/// Turns a part name back into a zip entry name.
#[must_use]
pub fn part_name_to_zip_name(part_name: &str) -> &str {
    part_name.trim_start_matches('/')
}

/// The case-folded key used for part-name lookups (OPC part names compare case-insensitively).
#[must_use]
pub fn part_key(part_name: &str) -> String {
    if part_name
        .bytes()
        .any(|b| b.is_ascii_uppercase() || b >= 0x80)
    {
        part_name.to_lowercase()
    } else {
        part_name.to_owned()
    }
}

/// True when two part names denote the same part.
#[must_use]
pub fn same_part(a: &str, b: &str) -> bool {
    (a.len() == b.len() && a.eq_ignore_ascii_case(b)) || part_key(a) == part_key(b)
}

/// Validates a part name.
///
/// # Errors
/// [`PartNameError`] for a name that is relative, ends with `/`, has an empty segment, a segment
/// ending with `.`, or a `.`/`..` segment.
pub fn validate_part_name(name: &str) -> Result<(), PartNameError> {
    let err = |reason| {
        Err(PartNameError {
            name: name.to_owned(),
            reason,
        })
    };
    let Some(rest) = name.strip_prefix('/') else {
        return err("part names must start with \"/\"");
    };
    if name.ends_with('/') {
        return err("part names must not end with \"/\"");
    }
    for s in rest.split('/') {
        if s.is_empty() {
            return err("empty segment");
        }
        if s == "." || s == ".." {
            return err("relative segment");
        }
        if s.ends_with('.') {
            return err("segment ends with \".\"");
        }
    }
    Ok(())
}

/// Normalises a part reference as found in the wild (`word\media\..\document.xml`,
/// `/word//styles.xml`, `xl/a%20b.xml`) to a valid part name: slashes, percent-escapes decoded,
/// `.`/`..` resolved, empty segments removed, a leading `/`.
///
/// # Errors
/// [`PartNameError`] when nothing remains or a segment ends with `.`.
pub fn normalize_part_name(name: &str) -> Result<String, PartNameError> {
    let out = resolve_target("/", name);
    validate_part_name(&out)?;
    Ok(out)
}

/// The extension without the dot, lower-cased (`/word/media/image1.PNG` → `png`).
#[must_use]
pub fn extension_of(part_name: &str) -> String {
    let slash = part_name.rfind('/');
    match part_name.rfind('.') {
        Some(dot) if slash.is_none_or(|s| dot > s) => part_name[dot + 1..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// The directory of a part, with a trailing slash (`/word/document.xml` → `/word/`).
#[must_use]
pub fn directory_of(part_name: &str) -> &str {
    match part_name.rfind('/') {
        Some(i) => &part_name[..=i],
        None => "/",
    }
}

/// The last segment of a part name.
#[must_use]
pub fn basename_of(part_name: &str) -> &str {
    match part_name.rfind('/') {
        Some(i) => &part_name[i + 1..],
        None => part_name,
    }
}

/// The relationships part of a source part: `/word/document.xml` → `/word/_rels/document.xml.rels`;
/// the package (`/`) → `/_rels/.rels`.
#[must_use]
pub fn rels_part_name(source_part: &str) -> String {
    if source_part == "/" || source_part.is_empty() {
        return "/_rels/.rels".to_owned();
    }
    format!(
        "{}_rels/{}.rels",
        directory_of(source_part),
        basename_of(source_part)
    )
}

/// The inverse of [`rels_part_name`]; `None` if `rels_part` is not a relationships part.
#[must_use]
pub fn source_part_of_rels(rels_part: &str) -> Option<String> {
    let lower = rels_part.to_ascii_lowercase();
    if !lower.ends_with(".rels") {
        return None;
    }
    let stem = &rels_part[..rels_part.len() - ".rels".len()];
    let slash = stem.rfind('/')?;
    let (dir_with_rels, base) = (&stem[..slash], &stem[slash + 1..]);
    if !dir_with_rels.to_ascii_lowercase().ends_with("_rels") {
        return None;
    }
    let dir = &dir_with_rels[..dir_with_rels.len() - "_rels".len()];
    if !dir.ends_with('/') {
        return None;
    }
    if base.is_empty() {
        return (dir == "/").then(|| "/".to_owned());
    }
    Some(format!("{dir}{base}"))
}

/// True for a relationships part (`…/_rels/….rels`).
#[must_use]
pub fn is_rels_part(name: &str) -> bool {
    source_part_of_rels(name).is_some()
}

/// Resolves a relationship target against its source part. `target` may be relative
/// (`media/image1.png`, `../theme/theme1.xml`) or absolute (`/xl/workbook.xml`); query and fragment
/// are removed, percent-escapes decoded, backslashes treated as slashes.
#[must_use]
pub fn resolve_target(source_part: &str, target: &str) -> String {
    let t = target.split('#').next().unwrap_or("");
    let t = t.split('?').next().unwrap_or("");
    let t = percent_decode(t).unwrap_or_else(|| t.to_owned());
    let t = t.replace('\\', "/");
    let mut out: Vec<&str> = Vec::new();
    let base = if t.starts_with('/') {
        ""
    } else {
        directory_of(if source_part.is_empty() {
            "/"
        } else {
            source_part
        })
    };
    for s in base.split('/').chain(t.split('/')) {
        match s {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    format!("/{}", out.join("/"))
}

/// The shortest relative reference from `source_part` to `target_part`, as Office writes it
/// (`/word/document.xml` → `/word/media/image1.png` gives `media/image1.png`; package relationships
/// use the path without the leading slash).
#[must_use]
pub fn relative_target(source_part: &str, target_part: &str) -> String {
    let from: Vec<&str> = directory_of(if source_part.is_empty() {
        "/"
    } else {
        source_part
    })
    .split('/')
    .filter(|s| !s.is_empty())
    .collect();
    let to: Vec<&str> = target_part.split('/').filter(|s| !s.is_empty()).collect();
    let mut i = 0;
    while i < from.len() && i + 1 < to.len() && from[i] == to[i] {
        i += 1;
    }
    let mut parts: Vec<&str> = vec![".."; from.len() - i];
    parts.extend_from_slice(&to[i..]);
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decoding_matches_decode_uri_component() {
        assert_eq!(percent_decode("a%20b").as_deref(), Some("a b"));
        assert_eq!(percent_decode("caf%C3%A9").as_deref(), Some("café"));
        assert_eq!(percent_decode("100%"), None);
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("%FF"), None);
    }

    #[test]
    fn rels_names_round_trip_case_insensitively() {
        assert_eq!(
            source_part_of_rels("/WORD/_RELS/document.xml.RELS").as_deref(),
            Some("/WORD/document.xml")
        );
        assert_eq!(source_part_of_rels("/word/_rels/.rels"), None);
        assert_eq!(source_part_of_rels("/x.rels"), None);
    }

    #[test]
    fn normalisation_cleans_wild_names() {
        assert_eq!(
            normalize_part_name("word\\media\\..\\document.xml").as_deref(),
            Ok("/word/document.xml")
        );
        assert_eq!(
            normalize_part_name("/word//a%20b.xml").as_deref(),
            Ok("/word/a b.xml")
        );
        assert!(normalize_part_name("../..").is_err());
        assert!(normalize_part_name("/word/doc.").is_err());
    }
}
