// SPDX-License-Identifier: AGPL-3.0-or-later
//! Text fidelity against an independent reader: for every generated feature fixture, the body
//! text we read is compared with mammoth's raw text (`fixtures/docx/read/<name>.mammoth.txt`,
//! made by `node packages/testing/scripts/mammoth-docx-text.mjs`), paragraph by paragraph.
//!
//! mammoth's text model differs from Word's in documented ways; the comparison follows it rather
//! than hiding differences (see `read_support::mammoth_model_paragraphs`): line breaks produce no
//! text, tabs are kept, deleted text is omitted, and headers, footers, notes and text boxes are
//! not body text. mammoth ignores `w:fldSimple`, so the oracle was given the file with simple
//! fields unwrapped into their result runs — the content Word displays.

mod read_support;

use std::path::Path;

use kiln_io_docx::read_docx;
use kiln_testing::fixtures::{FixtureFormat, FixtureKind, fixtures_dir, list_in};
use read_support::{mammoth_model_paragraphs, normalise};

/// mammoth's raw text split into paragraphs (it ends every paragraph with a blank line).
fn mammoth_paragraphs(text: &str) -> Vec<String> {
    let mut parts: Vec<String> = text.split("\n\n").map(str::to_owned).collect();
    if parts.last().is_some_and(String::is_empty) {
        parts.pop();
    }
    parts
}

/// `2·LCS / (|a| + |b|)` over paragraphs: 1 when the lists are equal.
fn similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let mut prev = vec![0usize; b.len() + 1];
    for x in a {
        let mut cur = vec![0usize; b.len() + 1];
        for (j, y) in b.iter().enumerate() {
            cur[j + 1] = if x == y {
                prev[j] + 1
            } else {
                prev[j + 1].max(cur[j])
            };
        }
        prev = cur;
    }
    #[allow(clippy::cast_precision_loss)]
    let lcs = prev[b.len()] as f64;
    #[allow(clippy::cast_precision_loss)]
    let total = (a.len() + b.len()) as f64;
    2.0 * lcs / total
}

/// The paragraphs outside one longest common subsequence, in document order: `ours:` lines are
/// only in the model, `mammoth:` lines only in the oracle. Following the alignment (rather than
/// pairing by index) keeps one inserted or missing paragraph from mispairing everything after it.
fn unmatched(a: &[String], b: &[String]) -> Vec<String> {
    // lcs[i][j] = LCS length of a[i..] and b[j..].
    let mut lcs = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] {
                lcs[i + 1][j + 1] + 1
            } else {
                lcs[i + 1][j].max(lcs[i][j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j == b.len() || (i < a.len() && lcs[i + 1][j] >= lcs[i][j + 1]) {
            out.push(format!("  ours:    #{i} {:?}", a[i]));
            i += 1;
        } else {
            out.push(format!("  mammoth: #{j} {:?}", b[j]));
            j += 1;
        }
    }
    out
}

#[test]
fn body_text_matches_mammoth_for_every_feature_fixture() {
    let fixtures = list_in(&fixtures_dir(), FixtureFormat::Docx, FixtureKind::Features);
    assert!(
        fixtures.len() >= 15,
        "the generated feature corpus is present"
    );
    let oracle_dir = fixtures_dir().join("docx/read");
    let mut report = Vec::new();
    let mut failures = Vec::new();
    for f in &fixtures {
        let name = f.file_name().trim_end_matches(".docx");
        let oracle = oracle_dir.join(format!("{name}.mammoth.txt"));
        let theirs = std::fs::read_to_string(&oracle).unwrap_or_else(|e| {
            panic!(
                "{}: {e} (run node packages/testing/scripts/mammoth-docx-text.mjs)",
                Path::new(&oracle).display()
            )
        });
        let theirs: Vec<String> = mammoth_paragraphs(&theirs)
            .iter()
            .map(|s| normalise(s))
            .collect();
        assert!(
            !theirs.concat().trim().is_empty(),
            "{name}: the oracle has text"
        );
        let read = read_docx(&f.read().expect("fixture")).expect("reads");
        let ours: Vec<String> = mammoth_model_paragraphs(&read.document)
            .iter()
            .map(|s| normalise(s))
            .collect();
        let score = similarity(&ours, &theirs);
        report.push(format!("{name:<20} {score:.4}"));
        if score < 0.99 {
            let diff: Vec<String> = unmatched(&ours, &theirs).into_iter().take(10).collect();
            failures.push(format!(
                "{name}: {score:.4} ({} vs {} paragraphs)\n{}",
                ours.len(),
                theirs.len(),
                diff.join("\n")
            ));
        }
    }
    println!("mammoth text similarity\n{}", report.join("\n"));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn similarity_is_a_paragraph_lcs_ratio() {
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    assert!((similarity(&s(&["a", "b"]), &s(&["a", "b"])) - 1.0).abs() < 1e-12);
    assert!((similarity(&s(&["a", "b"]), &s(&["a", "c"])) - 0.5).abs() < 1e-12);
    assert!((similarity(&[], &[]) - 1.0).abs() < 1e-12);
    assert_eq!(mammoth_paragraphs("a\n\nb\n\n"), ["a", "b"]);
}

#[test]
fn unmatched_follows_the_alignment() {
    let s = |v: &[&str]| v.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
    // One missing paragraph is reported once; the rest stays aligned.
    assert_eq!(
        unmatched(&s(&["a", "c", "d"]), &s(&["a", "b", "c", "d"])),
        ["  mammoth: #1 \"b\""]
    );
    assert_eq!(
        unmatched(&s(&["a", "x", "b"]), &s(&["a", "b"])),
        ["  ours:    #1 \"x\""]
    );
    assert!(unmatched(&s(&["a"]), &s(&["a"])).is_empty());
}
