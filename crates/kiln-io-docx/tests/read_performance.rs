// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reader throughput on a synthetic 300-page document (PLAN.md §3.9 B3: a 300-page `.docx`
//! shows its first page within 400 ms in the release build; parsing is the first step of that
//! budget). The limit here is for the unoptimised test build on a shared CI runner, so it is
//! generous; `cargo test --release` reads the same document in a fraction of it.

mod read_support;

use std::fmt::Write as _;
use std::time::Instant;

use kiln_io_docx::read_docx;
use read_support::{Build, build_docx};

fn big_body(pages: usize) -> String {
    let mut s = String::new();
    for page in 0..pages {
        let _ = write!(
            s,
            "<w:p><w:pPr><w:pStyle w:val=\"Heading1\"/></w:pPr><w:r><w:t>Chapter {}</w:t></w:r></w:p>",
            page + 1
        );
        for i in 0..8 {
            let _ = write!(
                s,
                "<w:p w14:paraId=\"{:08X}\"><w:pPr><w:spacing w:after=\"160\"/></w:pPr><w:r><w:rPr><w:b/><w:noProof/></w:rPr><w:t xml:space=\"preserve\">Lorem ipsum dolor sit amet, </w:t></w:r><w:r><w:rPr><w:i/><w:color w:val=\"1F3864\"/></w:rPr><w:t xml:space=\"preserve\">consectetur adipiscing elit, sed do eiusmod tempor </w:t></w:r><w:r><w:t>incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco.</w:t></w:r></w:p>",
                page * 100 + i
            );
        }
        if page % 5 == 0 {
            let cell = "<w:tc><w:tcPr><w:tcW w:w=\"2000\" w:type=\"dxa\"/></w:tcPr><w:p><w:r><w:t>cell</w:t></w:r></w:p></w:tc>";
            let row = format!("<w:tr>{}</w:tr>", cell.repeat(4));
            let _ = write!(
                s,
                "<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/></w:tblPr><w:tblGrid>{}</w:tblGrid>{}</w:tbl>",
                "<w:gridCol w:w=\"2000\"/>".repeat(4),
                row.repeat(6)
            );
        }
    }
    s
}

#[test]
fn reads_a_300_page_document_well_within_the_budget() {
    let bytes = build_docx(Build {
        body: big_body(300),
        ..Build::default()
    });
    let _ = read_docx(&bytes).expect("warm-up");
    let mut best = f64::INFINITY;
    let mut blocks = 0;
    for _ in 0..3 {
        let t0 = Instant::now();
        let r = read_docx(&bytes).expect("reads");
        best = best.min(t0.elapsed().as_secs_f64() * 1000.0);
        blocks = r.document.sections[0].blocks.len();
    }
    assert_eq!(blocks, 300 * 9 + 60);
    eprintln!("300 pages read in {best:.0} ms (best of 3)");
    let limit = if cfg!(debug_assertions) {
        1500.0
    } else {
        150.0
    };
    assert!(best < limit, "{best:.0} ms ≥ {limit} ms");
}
