// SPDX-License-Identifier: AGPL-3.0-or-later
//! Saving presentations: `.pptx`, `.potx` and `.pptm` through `kiln-io-pptx` (untouched parts
//! copied byte for byte from the file opened), `.odp` through `kiln-io-odf`, and the
//! Compatibility Checker.

use kiln_common::{KilnError, Result};
use kiln_deck::Presentation;
use kiln_io_pptx::{
    CompatOptions, CompatOrigin as Origin, CompatSeverity as Severity, PptxKind, PptxRead,
    WriteOptions, WriteSource, compat_report, write_pptx,
};
use kiln_ooxml::opc::OpcPackage;
use kiln_protocol::{CompatIssue, CompatOrigin, CompatSeverity, FileFormat};

use super::DeckDoc;

/// What a presentation read from a file keeps for saving.
pub(crate) struct Original {
    /// The file as opened.
    bytes: Vec<u8>,
    /// The reader's side data (its presentation moved into the model).
    read: PptxRead,
}

impl std::fmt::Debug for Original {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Original")
            .field("bytes", &self.bytes.len())
            .field("kind", &self.read.kind)
            .finish_non_exhaustive()
    }
}

impl Original {
    /// The file as opened (kept in a native bundle's `passthrough/`).
    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Splits a read into the presentation and what saving needs beside it.
    pub(crate) fn from_read(mut read: PptxRead, bytes: &[u8]) -> (Presentation, Self) {
        let pres = std::mem::replace(&mut read.presentation, Presentation::empty());
        (
            pres,
            Self {
                bytes: bytes.to_vec(),
                read,
            },
        )
    }

    /// The format the file was read as.
    pub(crate) fn kind(&self) -> PptxKind {
        self.read.kind
    }

    /// The reader's side data.
    pub(crate) fn read_ref(&self) -> &PptxRead {
        &self.read
    }
}

/// The PowerPoint format of a save format.
pub(crate) fn pptx_kind(format: FileFormat) -> Option<PptxKind> {
    match format {
        FileFormat::Pptx => Some(PptxKind::Presentation),
        FileFormat::Potx => Some(PptxKind::Template),
        FileFormat::Pptm => Some(PptxKind::MacroEnabledPresentation),
        FileFormat::Ppsx => Some(PptxKind::Show),
        _ => None,
    }
}

/// `YYYY-MM-DDThh:mm:ssZ` of a Unix time in milliseconds.
pub(crate) fn w3cdtf(unix_ms: u64) -> String {
    let secs = i64::try_from(unix_ms / 1000).unwrap_or(i64::MAX);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3_600,
        (rem % 3_600) / 60,
        rem % 60
    )
}

/// The civil date of a day count since 1970-01-01.
pub(crate) fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

impl DeckDoc {
    /// Serialises the presentation.
    pub(crate) fn write(&self, format: FileFormat, now_ms: Option<u64>) -> Result<Vec<u8>> {
        let failed = |e: String| KilnError::Format {
            format: format.extension().to_owned(),
            message: e,
        };
        #[cfg(feature = "odf")]
        if format == FileFormat::Odp {
            return kiln_io_odf::write_odp(
                &self.pres,
                &kiln_io_odf::WriteOptions {
                    properties: self.odf_properties.clone(),
                    ..kiln_io_odf::WriteOptions::default()
                },
            )
            .map_err(|e| failed(e.to_string()));
        }
        let kind = pptx_kind(format).ok_or_else(|| {
            KilnError::Unsupported(format!(
                "saving Kiln Deck presentations as .{}",
                format.extension()
            ))
        })?;
        let now = now_ms.map(w3cdtf);
        // Set Up Slide Show changed: `presProps.xml` gets the new `p:showPr`; the guides
        // changed: `viewProps.xml` gets the new `p:guideLst`.
        let settings_changed = super::show::settings::saved(&self.show_settings)
            != super::show::settings::saved(&self.show_settings_read);
        let empty = kiln_ooxml::passthrough::PassthroughSet::new();
        let base = self
            .original
            .as_ref()
            .map_or(&empty, |o| &o.read.passthrough);
        let main = self
            .original
            .as_ref()
            .map_or("/ppt/presentation.xml", |o| o.read.main_part.as_str());
        let mut patched: Option<kiln_ooxml::passthrough::PassthroughSet> = None;
        if settings_changed {
            patched = Some(super::show::settings::apply(
                patched.as_ref().unwrap_or(base),
                &self.show_settings,
                main,
            ));
        }
        if self.guides != self.guides_read {
            patched = Some(super::views::apply(
                patched.as_ref().unwrap_or(base),
                &self.guides,
                main,
            ));
        }
        let written = match &self.original {
            Some(o) => {
                let pkg = OpcPackage::open(&o.bytes).ok();
                let mut source = WriteSource::from_read(&o.read);
                if let Some(p) = &patched {
                    source.passthrough = p;
                }
                let source = match &pkg {
                    Some(p) => source.with_package(p),
                    None => source,
                };
                write_pptx(
                    &self.pres,
                    Some(&source),
                    &WriteOptions {
                        kind: Some(kind),
                        now,
                        preserve: pkg.as_ref().map(|_| &self.touched),
                        ..WriteOptions::default()
                    },
                )
            }
            None => {
                let extras = kiln_io_pptx::PptxExtras::default();
                let properties = kiln_ooxml::props::DocumentProperties::default();
                let source = patched.as_ref().map(|p| WriteSource {
                    passthrough: p,
                    extras: &extras,
                    properties: &properties,
                    main_part: "/ppt/presentation.xml",
                    kind,
                    package: None,
                });
                write_pptx(
                    &self.pres,
                    source.as_ref(),
                    &WriteOptions {
                        kind: Some(kind),
                        now,
                        ..WriteOptions::default()
                    },
                )
            }
        };
        let bytes = written
            .map(|w| w.bytes)
            .map_err(|e| failed(e.to_string()))?;
        match &self.props {
            Some(p) => p.patch_package(&bytes).map_err(failed),
            None => Ok(bytes),
        }
    }

    /// the document properties: those set in Kiln, else those of the file read.
    pub(crate) fn user_props(&self) -> crate::props::UserProps {
        self.props.clone().unwrap_or_else(|| {
            self.original
                .as_ref()
                .map(|o| crate::props::UserProps::from_ooxml(&o.read.properties))
                .unwrap_or_default()
        })
    }

    /// The Compatibility Checker for a save as `format`.
    pub(crate) fn compat(&self, format: FileFormat) -> Vec<CompatIssue> {
        #[cfg(feature = "odf")]
        if format == FileFormat::Odp {
            return self
                .odf_warnings
                .iter()
                .map(|w| CompatIssue {
                    code: "odf.open".into(),
                    severity: CompatSeverity::Info,
                    message: w.clone(),
                    count: 1,
                    origin: CompatOrigin::Open,
                })
                .collect();
        }
        let target = pptx_kind(format);
        let (source, extras, warnings) = match &self.original {
            Some(o) => (
                Some(o.kind()),
                Some(&o.read.extras),
                o.read.warnings.as_slice(),
            ),
            None => (None, None, &[][..]),
        };
        compat_report(
            &self.pres,
            CompatOptions {
                target,
                source,
                extras,
                warnings,
            },
        )
        .into_iter()
        .map(|i| CompatIssue {
            code: i.code,
            severity: match i.severity {
                Severity::Loss => CompatSeverity::Loss,
                Severity::Change => CompatSeverity::Change,
                Severity::Info => CompatSeverity::Info,
            },
            message: i.message,
            count: u32::try_from(i.count).unwrap_or(u32::MAX),
            origin: match i.origin {
                Origin::Save => CompatOrigin::Save,
                Origin::Open => CompatOrigin::Open,
            },
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_w3cdtf() {
        assert_eq!(w3cdtf(0), "1970-01-01T00:00:00Z");
        assert_eq!(w3cdtf(1_792_000_000_000), "2026-10-14T17:46:40Z");
    }
}
