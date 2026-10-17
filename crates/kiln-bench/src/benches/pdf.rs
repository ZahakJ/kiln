// SPDX-License-Identifier: AGPL-3.0-or-later
//! PDF export benchmark (WP-67), budget B11: `pdf-export-300p` exports
//! `fixtures/large/doc-300p.docx` (300 pages of headings, text, tables and pictures) the way
//! File ▸ Export ▸ PDF does on an open document: lay it out for print and write the tagged PDF
//! with subset fonts, pictures, bookmarks and page labels. Reading the `.docx` is not timed (the
//! document is open). Extra figures: `export_ms` (the PDF writer alone, p95), `pages`, `pdf_kb`.
//!
//! In quick mode it uses `fixtures/docx/features/long-text.docx`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use kiln_io_pdf::write::{WritePrint, print_layout};
use kiln_io_pdf::{PdfOptions, export};

use crate::harness::{Config, Sample, measure};
use crate::scenarios::RunOptions;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn input(options: &RunOptions) -> Result<Vec<u8>, String> {
    let rel = if options.quick {
        "fixtures/docx/features/long-text.docx"
    } else {
        "fixtures/large/doc-300p.docx"
    };
    let p = repo().join(rel);
    if !p.exists() && !options.quick {
        let status = std::process::Command::new("python3")
            .args([
                "-I",
                "tools/make-fixtures.py",
                "--synthetic-large",
                "--only",
                "docx",
            ])
            .current_dir(repo())
            .status()
            .map_err(|e| format!("python3 tools/make-fixtures.py: {e}"))?;
        if !status.success() {
            return Err(format!("tools/make-fixtures.py failed ({status})"));
        }
    }
    std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))
}

fn p95(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        clippy::cast_precision_loss
    )]
    let i = ((0.95 * v.len() as f64).ceil() as usize).clamp(1, v.len().max(1)) - 1;
    v.get(i).copied().unwrap_or(0.0)
}

/// `pdf-export-300p`.
pub fn export_300p(options: &RunOptions) -> Result<Sample, String> {
    let bytes = input(options)?;
    let doc = kiln_io_docx::read_docx(&bytes)
        .map_err(|e| format!("doc-300p.docx: {e:?}"))?
        .document;
    let config = if options.quick {
        Config::new(0, 2)
    } else {
        Config::new(1, 8)
    };
    let mut pages = 0usize;
    let mut size = 0usize;
    let mut writer = Vec::new();
    let mut sample = measure("pdf-export-300p", config, || {
        let layout = print_layout(&doc, kiln_doc_layout::default_fonts(), true, None);
        let t = Instant::now();
        let pdf = export(
            &WritePrint::new(&doc, &layout, Some("doc-300p".into())),
            &PdfOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        writer.push(t.elapsed().as_secs_f64() * 1000.0);
        if !pdf.starts_with(b"%PDF-") {
            return Err("the export is not a PDF".into());
        }
        pages = layout.pages().len();
        size = pdf.len();
        Ok(())
    })?;
    if !options.quick && pages < 250 {
        return Err(format!("doc-300p.docx printed {pages} pages"));
    }
    sample.extra.insert("export_ms".into(), p95(&mut writer));
    #[allow(clippy::cast_precision_loss)]
    {
        sample.extra.insert("pages".into(), pages as f64);
        sample.extra.insert("pdf_kb".into(), size as f64 / 1024.0);
    }
    Ok(sample)
}
