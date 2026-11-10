// SPDX-License-Identifier: AGPL-3.0-or-later
//! The side-by-side section of `docs/qa/fidelity.md`. It sits between two markers so that a plain
//! fidelity run keeps it. Its tables use `Kind` and `File` headers: the parity contract reserves
//! `Format` for the summary table.

use std::fmt::Write as _;

use super::{COMPARED, Outcome, Tool, at_least_as_good, tool_lead};
use crate::fidelity::Metric;
use crate::fixtures::{Fixture, FixtureFormat, FixtureKind};

/// Opens the section.
pub const START: &str = "<!-- differential:start -->";
/// Closes the section.
pub const END: &str = "<!-- differential:end -->";

/// A lead on any metric above this makes a file one a tool clearly wins.
pub const CLEAR_LEAD: f64 = 0.02;

/// One file across the tools.
#[derive(Debug, Clone, PartialEq)]
pub struct FileRow {
    /// The file.
    pub fixture: Fixture,
    /// Kiln's result.
    pub kiln: Outcome,
    /// The other tools' results, by [`Tool::id`].
    pub others: Vec<(String, Outcome)>,
    /// Pages in Kiln's own PDF.
    pub kiln_pages: Option<usize>,
    /// Pages in LibreOffice's PDF of the original.
    pub libreoffice_pages: Option<usize>,
}

impl FileRow {
    /// One line for the run log.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut s = format!("kiln {}", cell(&self.kiln));
        for (id, o) in &self.others {
            let _ = write!(s, " | {id} {}", cell(o));
        }
        s
    }

    fn other(&self, tool: Tool) -> Option<&Outcome> {
        self.others
            .iter()
            .find(|(id, _)| id == tool.id())
            .map(|(_, o)| o)
    }
}

/// The whole comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct Differential {
    /// The tools that took part.
    pub tools: Vec<Tool>,
    /// One row per file.
    pub rows: Vec<FileRow>,
}

fn cell(o: &Outcome) -> String {
    match o {
        Outcome::Failed(_) => "failed".to_owned(),
        Outcome::Scored(m) => COMPARED
            .iter()
            .filter_map(|c| m.get(*c).map(|v| format!("{v:.3}")))
            .collect::<Vec<_>>()
            .join(" "),
    }
}

impl Differential {
    fn rows_of(&self, format: FixtureFormat) -> Vec<&FileRow> {
        self.rows
            .iter()
            .filter(|r| r.fixture.format == format)
            .collect()
    }

    /// Files of `format` where Kiln is at least as good as `tool`, and all of them.
    #[must_use]
    pub fn wins(&self, format: FixtureFormat, tool: Tool) -> (usize, usize) {
        let rows = self.rows_of(format);
        let wins = rows
            .iter()
            .filter(|r| r.other(tool).is_some_and(|o| at_least_as_good(&r.kiln, o)))
            .count();
        (wins, rows.len())
    }

    /// Whether Kiln is at least as good as LibreOffice's ODF round trip on 80 % of docx files.
    #[must_use]
    pub fn passes_the_docx_bar(&self) -> bool {
        let (w, n) = self.wins(FixtureFormat::Docx, Tool::LibreOfficeOdf);
        n > 0 && w * 5 >= n * 4
    }

    /// The files a tool clearly beats Kiln on: (file, tool, metric, Kiln's result, the tool's).
    #[must_use]
    pub fn clear_losses(&self) -> Vec<(String, &'static str, String)> {
        let mut out = Vec::new();
        for r in &self.rows {
            for t in &self.tools {
                let Some(o) = r.other(*t) else { continue };
                if matches!((&r.kiln, o), (Outcome::Failed(_), Outcome::Scored(_))) {
                    out.push((r.fixture.rel.clone(), t.label(), "Kiln failed".to_owned()));
                }
                let (lead, m) = tool_lead(&r.kiln, o);
                if lead > CLEAR_LEAD
                    && let Some(m) = m
                {
                    out.push((r.fixture.rel.clone(), t.label(), format!("{m} +{lead:.3}")));
                }
            }
        }
        out
    }

    fn mean(rows: &[&FileRow], pick: impl Fn(&FileRow) -> Option<Outcome>, m: Metric) -> String {
        let (mut sum, mut n, mut failed) = (0.0, 0usize, 0usize);
        for r in rows {
            match pick(r) {
                Some(Outcome::Scored(x)) => {
                    if let Some(v) = x.get(m) {
                        sum += v;
                        n += 1;
                    }
                }
                Some(Outcome::Failed(_)) => failed += 1,
                None => {}
            }
        }
        if n + failed == 0 {
            return "—".to_owned();
        }
        // A failed file scores 0 in the mean.
        #[allow(clippy::cast_precision_loss)]
        let mean = sum / (n + failed) as f64;
        format!("{mean:.3}")
    }

    /// The Markdown section, markers included.
    #[must_use]
    pub fn to_markdown(&self, command: &str, stamp: &str) -> String {
        let mut md = format!("{START}\n## Side by side\n\n- Stamp: `{stamp}`\n\n");
        let _ = writeln!(
            md,
            "Written by `{command}`. Each tool takes the same input files and every output goes through the same comparison: Kiln's reader gives `structural`, `text` and `formulas`, the XML of each file gives `values` (the values it stores, not a recalculation) and `raw_elements`, and LibreOffice renders original and output at 60 dpi for `visual`, both from one path that is the same in every run and checkout (a document can show its own path in a field). Nothing renders in Office. `structural` counts facts in Kiln's own model, so a tool that restructures a file without changing how it looks (new style ids, runs split differently) loses points there; `text` depends on what that model reads; `values`, `visual` and `raw_elements` do not depend on it. LibreOffice's round trips run with its default settings.\n"
        );
        md.push_str(
            "The rule, fixed before the runs: Kiln counts as at least as good as a tool on a file only if it is not below the tool on any of those metrics. A file a tool fails on is a loss for that tool, Kiln included. Every file of the corpus counts. In the means, a failed file scores 0.\n\n",
        );
        md.push_str(
            "LibreOffice runs two ways: through OpenDocument (`docx → odt → docx`), the round trip the release bar names, and directly (`docx → docx`), what a user gets from open and save.\n\n",
        );
        md.push_str("ONLYOFFICE and the Craft apps were not run.\n\n");
        let formats = [
            FixtureFormat::Docx,
            FixtureFormat::Xlsx,
            FixtureFormat::Pptx,
        ];
        md.push_str("### Corpus\n\n| Kind | Files | Real-world | Generated |\n|---|---|---|---|\n");
        for f in formats {
            let rows = self.rows_of(f);
            let real = rows
                .iter()
                .filter(|r| r.fixture.kind == FixtureKind::External)
                .count();
            let _ = writeln!(
                md,
                "| {f} files | {} | {real} | {} |",
                rows.len(),
                rows.len() - real
            );
        }
        md.push_str(
            "\n### Kiln against each tool\n\n| Kind | Tool | Kiln at least as good | Share | structural | text | visual | raw_elements | Failed files |\n|---|---|---|---|---|---|---|---|---|\n",
        );
        for f in formats {
            let rows = self.rows_of(f);
            let means = |pick: &dyn Fn(&FileRow) -> Option<Outcome>| {
                [
                    Metric::Structural,
                    Metric::Text,
                    Metric::Visual,
                    Metric::RawElements,
                ]
                .map(|m| Self::mean(&rows, pick, m))
                .join(" | ")
            };
            let kiln_failed = rows
                .iter()
                .filter(|r| matches!(r.kiln, Outcome::Failed(_)))
                .count();
            let _ = writeln!(
                md,
                "| {f} files | Kiln | — | — | {} | {kiln_failed} |",
                means(&|r| Some(r.kiln.clone()))
            );
            for t in &self.tools {
                let (w, n) = self.wins(f, *t);
                #[allow(clippy::cast_precision_loss)]
                let share = if n == 0 {
                    0.0
                } else {
                    100.0 * w as f64 / n as f64
                };
                let failed = rows
                    .iter()
                    .filter(|r| matches!(r.other(*t), Some(Outcome::Failed(_))))
                    .count();
                let _ = writeln!(
                    md,
                    "| {f} files | {} | {w} of {n} | {share:.1} % | {} | {failed} |",
                    t.label(),
                    means(&|r| r.other(*t).cloned())
                );
            }
        }
        md.push_str("\n### Page counts\n\nKiln's own PDF against LibreOffice's PDF of the same original, every sheet of a workbook included, with Kiln's bundled metric-compatible fonts available to both.\n\n| Kind | Compared | Equal | Within one page | Largest gap |\n|---|---|---|---|---|\n");
        for f in formats {
            let pairs: Vec<(usize, usize)> = self
                .rows_of(f)
                .iter()
                .filter_map(|r| Some((r.kiln_pages?, r.libreoffice_pages?)))
                .collect();
            let equal = pairs.iter().filter(|(a, b)| a == b).count();
            let near = pairs.iter().filter(|(a, b)| a.abs_diff(*b) <= 1).count();
            let gap = pairs.iter().map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0);
            let _ = writeln!(
                md,
                "| {f} files | {} | {equal} | {near} | {gap} |",
                pairs.len()
            );
        }
        let losses = self.clear_losses();
        let _ = write!(
            md,
            "\n### Files a tool clearly beats Kiln on\n\nA lead above {CLEAR_LEAD} on any metric: {} entries.\n\n",
            losses.len()
        );
        if !losses.is_empty() {
            md.push_str("| File | Tool | Lead |\n|---|---|---|\n");
            for (file, tool, what) in &losses {
                let _ = writeln!(md, "| `{file}` | {tool} | {what} |");
            }
            md.push('\n');
        }
        md.push_str("### Every file\n\nScores in the order structural, text, visual, then values and formulas for workbooks, then raw_elements.\n\n| File | Kiln |");
        for t in &self.tools {
            let _ = write!(md, " {} |", t.label());
        }
        md.push_str(" Pages Kiln | Pages LibreOffice |\n|---|---|");
        md.push_str(&"---|".repeat(self.tools.len() + 2));
        md.push('\n');
        let pages = |p: Option<usize>| p.map_or_else(|| "—".to_owned(), |n| n.to_string());
        for r in &self.rows {
            let _ = write!(md, "| `{}` | {} |", r.fixture.rel, cell(&r.kiln));
            for t in &self.tools {
                let _ = write!(
                    md,
                    " {} |",
                    r.other(*t).map_or_else(|| "—".to_owned(), cell)
                );
            }
            let _ = writeln!(
                md,
                " {} | {} |",
                pages(r.kiln_pages),
                pages(r.libreoffice_pages)
            );
        }
        let _ = writeln!(md, "\n{END}");
        md
    }
}

/// `new` with the side-by-side section of `old`, when `new` has none.
#[must_use]
pub fn keep_section(old: &str, new: &str) -> String {
    let (Some(a), Some(b)) = (old.find(START), old.find(END)) else {
        return new.to_owned();
    };
    if new.contains(START) || b < a {
        return new.to_owned();
    }
    let section = &old[a..b + END.len()];
    let at = new.find("## OPC passthrough baseline").unwrap_or(new.len());
    format!("{}{section}\n\n{}", &new[..at], &new[at..])
}

/// `old` with its side-by-side section replaced by `section`.
#[must_use]
pub fn replace_section(old: &str, section: &str) -> String {
    let stripped = match (old.find(START), old.find(END)) {
        (Some(a), Some(b)) if b >= a => {
            format!("{}{}", &old[..a], old[b + END.len()..].trim_start())
        }
        _ => old.to_owned(),
    };
    keep_section(section, &stripped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_section_survives_a_plain_run() {
        let old =
            format!("# R\n\n{START}\n## Side by side\n{END}\n\n## OPC passthrough baseline\n");
        let new = "# S\n\n## OPC passthrough baseline\nx\n";
        let m = keep_section(&old, new);
        assert!(m.starts_with("# S\n\n<!-- differential:start -->"));
        assert!(m.ends_with("## OPC passthrough baseline\nx\n"));
        let again = replace_section(&m, &format!("{START}\nnew\n{END}\n"));
        assert!(again.contains("new") && !again.contains("## Side by side"));
    }
}
