// SPDX-License-Identifier: AGPL-3.0-or-later
//! LibreOffice's round trips, through the oracle.

use std::path::{Path, PathBuf};

use crate::fixtures::{Fixture, FixtureFormat};
use crate::oracle::Oracle;

/// A tool whose round trip is scored next to Kiln's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    /// LibreOffice through OpenDocument: docx → odt → docx.
    LibreOfficeOdf,
    /// LibreOffice opening and saving the format itself: docx → docx.
    LibreOfficeDirect,
}

fn odf_ext(format: FixtureFormat) -> Option<&'static str> {
    match format {
        FixtureFormat::Docx => Some("odt"),
        FixtureFormat::Xlsx => Some("ods"),
        FixtureFormat::Pptx => Some("odp"),
        FixtureFormat::Odf => None,
    }
}

impl Tool {
    /// The identifier in logs and scratch directories.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::LibreOfficeOdf => "lo-odf",
            Self::LibreOfficeDirect => "lo-direct",
        }
    }

    /// The name in the report.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::LibreOfficeOdf => "LibreOffice via ODF",
            Self::LibreOfficeDirect => "LibreOffice direct",
        }
    }

    /// Runs the tool's round trip over a file and returns what it wrote.
    ///
    /// # Errors
    ///
    /// When the tool fails, times out or writes nothing.
    pub fn round_trip(
        self,
        fixture: &Fixture,
        oracle: &Oracle,
        dir: &Path,
    ) -> Result<PathBuf, String> {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        let ext = fixture.format.extension();
        let input = dir.join(format!("input.{ext}"));
        std::fs::copy(&fixture.path, &input).map_err(|e| e.to_string())?;
        let own = oracle.with_default_settings(default_profile(dir));
        match self {
            Self::LibreOfficeOdf => {
                let middle = odf_ext(fixture.format).ok_or("no OpenDocument form")?;
                let odf = own.convert(&input, middle, &dir.join("odf"))?;
                let back = own.convert(&odf, ext, &dir.join("back"))?;
                Ok(rename_stem(back)?)
            }
            Self::LibreOfficeDirect => {
                let back = own.convert(&input, ext, &dir.join("back"))?;
                Ok(rename_stem(back)?)
            }
        }
    }
}

/// One default-settings profile per thread, beside the run's other scratch.
fn default_profile(dir: &Path) -> PathBuf {
    let root = dir.parent().and_then(Path::parent).unwrap_or(dir);
    root.join(format!(
        "profile-defaults-{:?}",
        std::thread::current().id()
    ))
}

/// A converted file under a name of its own, so its page renders do not collide with the input's.
fn rename_stem(path: PathBuf) -> Result<PathBuf, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("bin");
    let new = path.with_file_name(format!("back.{ext}"));
    std::fs::rename(&path, &new).map_err(|e| e.to_string())?;
    Ok(new)
}
