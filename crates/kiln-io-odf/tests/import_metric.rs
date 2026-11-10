// SPDX-License-Identifier: AGPL-3.0-or-later
//! An import metric independent of Kiln's own ODF writer: every `odf/features/X.od?` was made by
//! LibreOffice from `docx|xlsx|pptx/features/X`, so the ODF reader's result is compared with the
//! Office reader's result for the source. Text by word overlap; structure by the share of facts
//! that apply to both formats (headings, list labels, tables, notes and pictures for text; cell
//! values, formulas, number formats, merges, rules and sheet settings for spreadsheets; the deck
//! facts for presentations).
//!
//! `KILN_ODF_METRIC=1` prints every file's scores and differences.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use kiln_doc::query::PlainTextOptions;

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures")
}

fn pairs(ext: &str, source: &str) -> Vec<(PathBuf, PathBuf)> {
    let mut v: Vec<(PathBuf, PathBuf)> = std::fs::read_dir(fixtures().join("odf/features"))
        .expect("fixtures/odf/features")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == ext))
        .filter_map(|p| {
            let src = fixtures()
                .join(source)
                .join("features")
                .join(p.with_extension(source).file_name()?);
            src.exists().then_some((p, src))
        })
        .collect();
    v.sort();
    v
}

/// Shared share of two multisets (1.0 when both are empty), and what differs.
fn overlap(a: &[String], b: &[String]) -> (f64, Vec<String>) {
    let mut m: HashMap<&str, i64> = HashMap::new();
    for x in a {
        *m.entry(x.as_str()).or_insert(0) += 1;
    }
    let mut common = 0usize;
    let mut extra = Vec::new();
    for y in b {
        match m.get_mut(y.as_str()) {
            Some(n) if *n > 0 => {
                *n -= 1;
                common += 1;
            }
            _ => extra.push(format!("+ {y}")),
        }
    }
    for (k, n) in m {
        for _ in 0..n.max(0) {
            extra.push(format!("- {k}"));
        }
    }
    extra.sort();
    let d = a.len().max(b.len());
    (
        if d == 0 {
            1.0
        } else {
            common as f64 / d as f64
        },
        extra,
    )
}

fn words(s: &str) -> Vec<String> {
    s.split_whitespace()
        .map(|w| w.trim_matches(|c: char| c == '\u{fffc}').to_owned())
        .filter(|w| !w.is_empty())
        .collect()
}

fn verbose() -> bool {
    std::env::var_os("KILN_ODF_METRIC").is_some()
}

struct Score {
    file: String,
    text: f64,
    facts: f64,
    diff: Vec<String>,
}

fn report(kind: &str, scores: &[Score], text_min: f64, facts_min: f64, exceptions: &[(&str, f64)]) {
    let mut failures = Vec::new();
    for s in scores {
        let floor = exceptions
            .iter()
            .find(|(n, _)| *n == s.file)
            .map_or(facts_min, |(_, f)| *f);
        eprintln!("{kind} {}: text {:.3} facts {:.3}", s.file, s.text, s.facts);
        if verbose() {
            for d in s.diff.iter().take(400) {
                eprintln!("    {d}");
            }
        }
        if s.text < text_min || s.facts < floor {
            failures.push(format!(
                "{}: text {:.3} (min {text_min}), facts {:.3} (min {floor})\n{}",
                s.file,
                s.text,
                s.facts,
                s.diff
                    .iter()
                    .take(25)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

// ---------------------------------------------------------------- text documents

fn doc_text(doc: &kiln_doc::Document, headers: bool) -> String {
    doc.plain_text(PlainTextOptions {
        include_headers_footers: headers,
        include_notes: true,
        accept_changes: false,
        list_labels: true,
    })
}

fn tables(blocks: &[kiln_doc::Block], out: &mut Vec<String>) {
    for b in blocks {
        if let kiln_doc::Block::Table(t) = b {
            out.push(format!("table {}x{}", t.rows.len(), t.grid.len()));
            for row in &t.rows {
                for cell in &row.cells {
                    tables(&cell.blocks, out);
                }
            }
        }
    }
}

fn doc_facts(doc: &kiln_doc::Document) -> Vec<String> {
    let mut f = Vec::new();
    for h in doc.outline() {
        f.push(format!("heading {}: {}", h.level, h.text));
    }
    // List labels as plain text shows them (label, tab, text).
    for line in doc_text(doc, false).lines() {
        if let Some((label, rest)) = line.split_once('\t')
            && !label.trim().is_empty()
            && label.chars().count() <= 8
        {
            f.push(format!(
                "list {label}: {}",
                rest.split_whitespace()
                    .take(4)
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
    }
    for s in &doc.sections {
        tables(&s.blocks, &mut f);
    }
    f.push(format!("sections {}", doc.sections.len()));
    // Separator notes are Word's own; ODF has none.
    for n in doc
        .notes
        .iter()
        .filter(|n| n.note_type == kiln_doc::st::NoteType::Normal)
    {
        f.push(format!("note {:?}", n.kind));
    }
    f.push(format!("comments {}", doc.comments.len()));
    let mut drawings = 0;
    for (_, p) in doc.all_paragraphs() {
        for o in p.content.objects() {
            if matches!(o, kiln_doc::InlineObject::Drawing(_)) {
                drawings += 1;
            }
        }
    }
    f.push(format!("drawings {drawings}"));
    for s in &doc.sections {
        let p = &s.props.page_size;
        // Page size in whole millimetres (twips / 56.7).
        f.push(format!(
            "page {}x{}",
            (f64::from(p.width) / 56.7).round(),
            (f64::from(p.height) / 56.7).round()
        ));
        let m = &s.props.margins;
        f.push(format!(
            "margins {} {} {} {}",
            (f64::from(m.top) / 56.7).round(),
            (f64::from(m.bottom) / 56.7).round(),
            (f64::from(m.left) / 56.7).round(),
            (f64::from(m.right) / 56.7).round()
        ));
    }
    f
}

#[test]
fn odt_reads_like_the_docx_it_came_from() {
    let mut scores = Vec::new();
    for (odt, docx) in pairs("odt", "docx") {
        let a = kiln_io_docx::read_docx(&std::fs::read(&docx).unwrap())
            .unwrap()
            .document;
        let b = kiln_io_odf::read_odt(&std::fs::read(&odt).unwrap())
            .unwrap()
            .document;
        let (text, mut diff) = overlap(&words(&doc_text(&a, true)), &words(&doc_text(&b, true)));
        let (facts, d2) = overlap(&doc_facts(&a), &doc_facts(&b));
        diff.truncate(10);
        diff.extend(d2);
        scores.push(Score {
            file: odt.file_name().unwrap().to_string_lossy().into_owned(),
            text,
            facts,
            diff,
        });
    }
    assert!(scores.len() >= 15, "{}", scores.len());
    report("odt", &scores, 0.97, 0.90, ODT_EXCEPTIONS);
}

/// Files whose facts score is below the floor for reasons outside the reader, with the floor
/// they must keep.
const ODT_EXCEPTIONS: &[(&str, f64)] = &[];

// ---------------------------------------------------------------- spreadsheets

/// What LibreOffice changes on its own when it converts `.xlsx` to `.ods`, so the comparison
/// leaves it out: formulas with structured references (rewritten as cell references), fills
/// that are patterns or gradients (blended into one colour).
#[derive(Default)]
struct Skip {
    formulas: std::collections::HashSet<String>,
    fills: std::collections::HashSet<String>,
}

fn skip_of(wb: &kiln_sheet::Workbook) -> Skip {
    let mut s = Skip::default();
    for (k, (_, formula)) in kiln_io_xlsx::snapshot_cells(wb) {
        if formula.is_some_and(|f| f.contains('[')) {
            s.formulas.insert(k);
        }
    }
    let styles = wb.styles();
    for sh in wb.sheets() {
        for (r, c, d) in sh.cells().iter() {
            let solid = matches!(
                styles.fill(styles.xf(d.style).fill),
                kiln_sheet::styles::Fill::Pattern {
                    pattern: kiln_sheet::styles::PatternType::Solid
                        | kiln_sheet::styles::PatternType::None,
                    ..
                }
            );
            if !solid {
                s.fills.insert(format!(
                    "{}!{}",
                    sh.name(),
                    kiln_io_xlsx::text::cell_ref(r, c)
                ));
            }
        }
    }
    s
}

/// A formula as text to compare: no spaces or sheet-name quotes, upper case.
fn formula_key(f: &str) -> String {
    f.replace([' ', '\''], "").to_ascii_uppercase()
}

fn sheet_facts(wb: &kiln_sheet::Workbook, skip: &Skip) -> Vec<String> {
    let mut f = Vec::new();
    let styles = wb.styles();
    for (k, (v, formula)) in kiln_io_xlsx::snapshot_cells(wb) {
        f.push(format!("{k}={v}"));
        if let Some(x) = formula
            && !skip.formulas.contains(&k)
        {
            f.push(format!("{k} formula ={}", formula_key(&x)));
        }
    }
    for s in wb.sheets() {
        let name = s.name();
        f.push(format!("sheet {name} {:?}", s.meta().visibility));
        for (r, c, d) in s.cells().iter() {
            if d.style == 0 {
                continue;
            }
            let at = format!("{name}!{}", kiln_io_xlsx::text::cell_ref(r, c));
            let code = styles.num_fmt(d.style);
            if code != "General" {
                f.push(format!("{at} format {code}"));
            }
            let font = styles.font_of(d.style);
            if font.bold {
                f.push(format!("{at} bold"));
            }
            if font.italic {
                f.push(format!("{at} italic"));
            }
            if !skip.fills.contains(&at)
                && let Some(c) = styles.fill(styles.xf(d.style).fill).main_color()
            {
                f.push(format!("{at} fill {c:?}"));
            }
        }
        let m = s.meta();
        for r in &m.merges {
            f.push(format!("{name} merge {}", r.to_a1(false)));
        }
        for cf in &m.cf {
            let ranges: Vec<String> = cf.ranges.iter().map(|r| r.to_a1(false)).collect();
            for rule in &cf.rules {
                let kind = format!("{:?}", rule.kind);
                let kind = kind.split([' ', '(', '{']).next().unwrap_or("").to_owned();
                let look = rule
                    .kind
                    .dxf()
                    .and_then(|d| styles.dxf(d))
                    .map(|d| {
                        format!(
                            "fill {:?} bold {:?} color {:?}",
                            d.fill
                                .as_ref()
                                .and_then(kiln_sheet::styles::Fill::main_color),
                            d.font.as_ref().and_then(|f| f.bold),
                            d.font.as_ref().and_then(|f| f.color)
                        )
                    })
                    .unwrap_or_default();
                f.push(format!("{name} cf {} {kind} {look}", ranges.join(" ")));
            }
        }
        f.push(format!("{name} validations {}", m.validations.len()));
        for h in &m.hyperlinks {
            // LibreOffice turns a link to a place in the workbook into a link to the source
            // file with that place (`../hyperlinks.xlsx#Target!B5`).
            let (target, location) = match h.target.as_deref().and_then(|t| t.split_once(".xlsx#"))
            {
                Some((_, place)) => (None, Some(place.to_owned())),
                None => (
                    h.target.clone(),
                    h.location
                        .clone()
                        .map(|l| l.trim_start_matches('#').to_owned()),
                ),
            };
            f.push(format!(
                "{name} link {} {target:?} {location:?}",
                h.range.to_a1(false),
            ));
        }
        for n in &m.notes {
            f.push(format!(
                "{name} note {} {}",
                kiln_io_xlsx::text::cell_ref(n.at.row, n.at.col),
                n.text.split_whitespace().collect::<Vec<_>>().join(" ")
            ));
        }
        f.push(format!("{name} freeze {:?}", m.view.freeze));
        let hf = &m.print.header_footer;
        f.push(format!(
            "{name} header {:?} footer {:?}",
            hf.odd_header, hf.odd_footer
        ));
        f.push(format!("{name} drawings {}", m.drawings.len()));
        for (c, p) in s.cols().to_vec() {
            if p.hidden {
                f.push(format!("{name} col {c} hidden"));
            }
        }
        for (r, p) in s.rows().to_vec() {
            if p.hidden {
                f.push(format!("{name} row {r} hidden"));
            }
        }
    }
    for n in wb.names() {
        if !n.name.starts_with("_xlnm") {
            f.push(format!("name {}", n.name));
        }
    }
    f
}

#[test]
fn ods_reads_like_the_xlsx_it_came_from() {
    let mut scores = Vec::new();
    for (ods, xlsx) in pairs("ods", "xlsx") {
        let ra = kiln_io_xlsx::read_xlsx(&std::fs::read(&xlsx).unwrap()).unwrap();
        let charts_a: Vec<(String, kiln_charts::model::ChartSpace)> = ra
            .source
            .charts()
            .into_iter()
            .filter_map(|(n, c)| c.ok().map(|c| (n, c)))
            .collect();
        let a = ra.workbook;
        // Charts without caches show the cells' values (as Grid shows them).
        let charts_a: Vec<_> = charts_a
            .into_iter()
            .map(|(n, mut c)| {
                kiln_charts::data::refresh(&mut c, &a);
                (n, c)
            })
            .collect();
        let rb = kiln_io_odf::read_ods(&std::fs::read(&ods).unwrap()).unwrap();
        let charts_b = rb.charts;
        let b = rb.workbook;
        let cells = |wb: &kiln_sheet::Workbook| -> Vec<String> {
            kiln_io_xlsx::snapshot_cells(wb)
                .into_iter()
                .map(|(k, (v, _))| format!("{k}={v}"))
                .collect()
        };
        let (text, _) = overlap(&cells(&a), &cells(&b));
        let skip = skip_of(&a);
        let mut fa = sheet_facts(&a, &skip);
        fa.extend(chart_facts(&a, &charts_a));
        let mut fb = sheet_facts(&b, &skip);
        fb.extend(chart_facts(&b, &charts_b));
        let (facts, diff) = overlap(&fa, &fb);
        scores.push(Score {
            file: ods.file_name().unwrap().to_string_lossy().into_owned(),
            text,
            facts,
            diff,
        });
    }
    assert!(scores.len() >= 15, "{}", scores.len());
    report("ods", &scores, 0.99, 0.90, ODS_EXCEPTIONS);
}

const ODS_EXCEPTIONS: &[(&str, f64)] = &[
    // The source's chart categories are number references (`c:numRef`) to text cells, which the
    // Office reader refreshes as blanks; LibreOffice reads them as the month names.
    ("charts.ods", 0.85),
];

/// The charts of the sheets' chart drawings, in drawing order.
fn chart_facts(
    wb: &kiln_sheet::Workbook,
    charts: &[(String, kiln_charts::model::ChartSpace)],
) -> Vec<String> {
    let mut f = Vec::new();
    for s in wb.sheets() {
        let anchors = s
            .meta()
            .drawings
            .iter()
            .filter(|d| d.kind == kiln_sheet::meta::DrawingKind::Chart);
        for d in anchors {
            // By the cell the chart is anchored at (the files list drawings in different orders).
            let at = match d.anchor {
                kiln_sheet::meta::Anchor::TwoCell { from, .. }
                | kiln_sheet::meta::Anchor::OneCell { from, .. } => {
                    format!("{},{}", from.row, from.col)
                }
                kiln_sheet::meta::Anchor::Absolute { pos, .. } => format!("{pos:?}"),
            };
            let prefix = format!("{}/chart@{at}", s.name());
            match charts.iter().find(|(n, _)| *n == d.reference) {
                Some((_, cs)) => f.extend(kiln_io_odf::chart::facts(&prefix, cs)),
                None => f.push(format!("{prefix}=missing")),
            }
        }
    }
    f
}

// ---------------------------------------------------------------- presentations

/// The deck facts, compared across formats: layouts as a set (LibreOffice orders its master
/// pages by name), PowerPoint's centred title as the title (ODF has one title class), an
/// inherited fill or outline as none (LibreOffice writes the resolved value, which is none where
/// PowerPoint inherits nothing visible), the content placeholder as a body one (ODF's outline
/// class is both), a straight connector as a line, positions to 0.1 mm (LibreOffice rounds 10 µm down), and no facts for the single
/// empty paragraph of a shape without text (LibreOffice writes no text for it).
fn deck_facts(p: &kiln_deck::Presentation) -> Vec<String> {
    let all = kiln_io_odf::deck_facts(p);
    let empty: std::collections::HashSet<String> = all
        .iter()
        .filter_map(|f| f.strip_suffix("/p0/runs="))
        .filter(|prefix| !all.iter().any(|g| g.starts_with(&format!("{prefix}/p1"))))
        .map(str::to_owned)
        .collect();
    all.into_iter()
        // A PowerPoint table's look comes from its table style; LibreOffice writes it out as
        // cell formatting.
        .filter(|f| !(f.contains("/cell") && (f.contains("/fill=") || f.contains("/borders="))))
        .filter(|f| {
            !empty.iter().any(|p| {
                f.strip_prefix(p.as_str())
                    .is_some_and(|rest| rest == "/p0/runs=" || rest.starts_with("/p0=lvl"))
            })
        })
        .map(|f| {
            if let Some((k, v)) = f.split_once('=') {
                if k.starts_with("layout") && !k.contains('/') {
                    return format!("layout={v}");
                }
                if k.ends_with("/xfrm") {
                    let parts: Vec<String> = v
                        .split(',')
                        .map(|n| {
                            n.parse::<i64>().map_or_else(
                                |_| n.to_owned(),
                                |x| ((x + 6).div_euclid(10)).to_string(),
                            )
                        })
                        .collect();
                    return format!("{k}={}", parts.join(","));
                }
                if (k.ends_with("/fill") || k.ends_with("/line")) && v == "inherit" {
                    return format!("{k}=none");
                }
                if k.ends_with("/ph") {
                    return format!(
                        "{k}={}",
                        v.replace("ctrTitle", "title").replace("obj", "body")
                    );
                }
                if k.ends_with("/geom") && v == "straightConnector1" {
                    return format!("{k}=line");
                }
            }
            f
        })
        .collect()
}

#[test]
fn odp_reads_like_the_pptx_it_came_from() {
    let mut scores = Vec::new();
    for (odp, pptx) in pairs("odp", "pptx") {
        let a = kiln_io_pptx::read_pptx(&std::fs::read(&pptx).unwrap())
            .unwrap()
            .presentation;
        let b = kiln_io_odf::read_odp(&std::fs::read(&odp).unwrap())
            .unwrap()
            .presentation;
        let (text, mut diff) = overlap(
            &words(&kiln_io_odf::deck_text(&a)),
            &words(&kiln_io_odf::deck_text(&b)),
        );
        let (facts, d2) = overlap(&deck_facts(&a), &deck_facts(&b));
        diff.truncate(10);
        diff.extend(d2);
        scores.push(Score {
            file: odp.file_name().unwrap().to_string_lossy().into_owned(),
            text,
            facts,
            diff,
        });
    }
    assert!(scores.len() >= 10, "{}", scores.len());
    report("odp", &scores, 0.97, 0.90, ODP_EXCEPTIONS);
}

const ODP_EXCEPTIONS: &[(&str, f64)] = &[];
