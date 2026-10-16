// SPDX-License-Identifier: AGPL-3.0-or-later
//! Every formula cell of the feature fixtures, as Kiln calculates it after reading, against
//! LibreOffice's recalculated values (`tools/oracle-soffice.sh` with the recalculate-on-load
//! profile, every sheet exported to CSV): numbers within 1e-9 (relative to their magnitude),
//! everything else as equal text. Skipped (with a message) where LibreOffice is not installed,
//! as on CI.
//!
//! LibreOffice 24.2 does not evaluate some functions Excel has (it shows `#NAME?`), and a few
//! functions answer by design differently in the two programs; those cells are listed in
//! [`ORACLE_GAPS`] with the reason, and the test fails if a listed cell starts to agree (so the
//! list cannot go stale) or if any other cell disagrees.

use std::collections::BTreeMap;
use std::path::Path;

use kiln_io_xlsx::read_xlsx;
use kiln_sheet::CellValue;
use kiln_testing::oracle::Oracle;

/// Cells whose LibreOffice value is not a valid oracle, with the reason.
const ORACLE_GAPS: &[(&str, &str, &str)] = &[
    // Functions LibreOffice 24.2 does not have (XLOOKUP, XMATCH and the dynamic-array functions
    // arrived in LibreOffice 24.8).
    (
        "formulas.xlsx",
        "Formulas!C192",
        "XLOOKUP: not in LibreOffice 24.2",
    ),
    (
        "formulas.xlsx",
        "Formulas!C193",
        "XLOOKUP: not in LibreOffice 24.2",
    ),
    (
        "formulas.xlsx",
        "Formulas!C194",
        "XMATCH: not in LibreOffice 24.2",
    ),
    (
        "shared-array-formulas.xlsx",
        "Formulas!G1",
        "SORT: not in LibreOffice 24.2",
    ),
    (
        "shared-array-formulas.xlsx",
        "Formulas!H1",
        "UNIQUE: not in LibreOffice 24.2",
    ),
    (
        "shared-array-formulas.xlsx",
        "Formulas!I1",
        "SEQUENCE: not in LibreOffice 24.2",
    ),
    // LibreOffice 24.2 does not map these `_xlfn.` names from OOXML.
    (
        "formulas.xlsx",
        "Formulas!C176",
        "_xlfn.NETWORKDAYS.INTL: not imported by LibreOffice 24.2",
    ),
    (
        "formulas.xlsx",
        "Formulas!C178",
        "_xlfn.WORKDAY.INTL: not imported by LibreOffice 24.2",
    ),
    // The fixture stores these Excel 2010+ functions without their `_xlfn.` prefix; LibreOffice
    // then does not recognise them. Kiln reads them as the functions named (and saves them with
    // the prefix, which makes Excel compute them too).
    (
        "formulas.xlsx",
        "Formulas!C31",
        "AGGREGATE stored without _xlfn.",
    ),
    (
        "formulas.xlsx",
        "Formulas!C257",
        "PDURATION stored without _xlfn.",
    ),
    (
        "formulas.xlsx",
        "Formulas!C258",
        "RRI stored without _xlfn.",
    ),
    // Excel and LibreOffice differ by design; Kiln follows Excel.
    (
        "formulas.xlsx",
        "Formulas!C102",
        "PERCENTRANK.INC: Excel truncates to 3 significant digits (0.315); LibreOffice rounds (0.316)",
    ),
    (
        "formulas.xlsx",
        "Formulas!C233",
        "TYPE of a multi-cell range: Excel returns 64; LibreOffice #VALUE!",
    ),
    (
        "formulas.xlsx",
        "Formulas!C259",
        "CONVERT lbm→kg: Excel uses the exact 0.45359237; LibreOffice an older factor",
    ),
];

/// What the [`ORACLE_GAPS`] cells (and the arrays the gap formulas spill) must hold, computed by
/// hand from the fixtures' `Data` sheet (A1:A20 = 1…20, B = alpha…, C1 = 2026-01-15 in steps of a
/// week) and the `Formulas` sheet of `shared-array-formulas.xlsx` (A1:A10 = 3,6,9,1,4,7,10,2,5,8),
/// following Excel's documented definitions. These need no LibreOffice, so they also run on CI.
#[derive(Debug, Clone, Copy)]
enum Expect {
    N(f64),
    T(&'static str),
}

const GAP_EXPECTED: &[(&str, &str, Expect)] = &[
    // SUM ignoring errors and hidden rows over 1…20.
    ("formulas.xlsx", "Formulas!C31", Expect::N(210.0)),
    // (7 − 1) / (20 − 1) = 0.31578…, truncated (not rounded) to 3 significant digits.
    ("formulas.xlsx", "Formulas!C102", Expect::N(0.315)),
    // 2026-01-15 … 2026-03-19 is 64 days, 9 of them Sundays (weekend code 11 = Sunday only).
    ("formulas.xlsx", "Formulas!C176", Expect::N(55.0)),
    // 10 working days (Saturday–Sunday weekend) after Thursday 2026-01-15: 2026-01-29.
    ("formulas.xlsx", "Formulas!C178", Expect::N(46_051.0)),
    ("formulas.xlsx", "Formulas!C192", Expect::T("eta")),
    ("formulas.xlsx", "Formulas!C193", Expect::T("none")),
    ("formulas.xlsx", "Formulas!C194", Expect::N(7.0)),
    ("formulas.xlsx", "Formulas!C233", Expect::N(64.0)),
    // ln(2200 / 2000) / ln(1.025).
    (
        "formulas.xlsx",
        "Formulas!C257",
        Expect::N(3.859_866_162_622_662),
    ),
    // (11000 / 10000)^(1/96) − 1.
    (
        "formulas.xlsx",
        "Formulas!C258",
        Expect::N(0.000_993_307_376_291_330_3),
    ),
    // The international avoirdupois pound, exactly.
    ("formulas.xlsx", "Formulas!C259", Expect::N(0.453_592_37)),
    // SORT(A1:A10) spills 1…10 down G.
    ("shared-array-formulas.xlsx", "Formulas!G1", Expect::N(1.0)),
    ("shared-array-formulas.xlsx", "Formulas!G2", Expect::N(2.0)),
    ("shared-array-formulas.xlsx", "Formulas!G5", Expect::N(5.0)),
    (
        "shared-array-formulas.xlsx",
        "Formulas!G10",
        Expect::N(10.0),
    ),
    // UNIQUE(A1:A10) keeps the first-seen order.
    ("shared-array-formulas.xlsx", "Formulas!H1", Expect::N(3.0)),
    ("shared-array-formulas.xlsx", "Formulas!H2", Expect::N(6.0)),
    ("shared-array-formulas.xlsx", "Formulas!H4", Expect::N(1.0)),
    ("shared-array-formulas.xlsx", "Formulas!H10", Expect::N(8.0)),
    // SEQUENCE(5,1,1,2) = 1, 3, 5, 7, 9.
    ("shared-array-formulas.xlsx", "Formulas!I1", Expect::N(1.0)),
    ("shared-array-formulas.xlsx", "Formulas!I3", Expect::N(5.0)),
    ("shared-array-formulas.xlsx", "Formulas!I5", Expect::N(9.0)),
];

/// The cells LibreOffice cannot vouch for are checked against hand-computed values instead, and
/// every listed gap has such a value.
#[test]
fn oracle_gap_cells_match_hand_computed_values() {
    for (file, cell, reason) in ORACLE_GAPS {
        assert!(
            GAP_EXPECTED.iter().any(|(f, c, _)| f == file && c == cell),
            "{file} {cell} ({reason}) has no independent expected value"
        );
    }
    let features = kiln_testing::fixtures::fixtures_dir().join("xlsx/features");
    let mut failures = Vec::new();
    for (file, at, expect) in GAP_EXPECTED {
        let read = read_xlsx(&std::fs::read(features.join(file)).expect("fixture")).expect("read");
        let wb = &read.workbook;
        let (sheet, cell) = at.split_once('!').expect("sheet!cell");
        let s = wb
            .sheets()
            .iter()
            .find(|s| s.name() == sheet)
            .expect("sheet")
            .id();
        let (r, c) = kiln_io_xlsx::text::parse_cell_ref(cell.as_bytes()).expect("cell");
        let ours = wb.value(s, r, c);
        let ok = match (expect, &ours) {
            (Expect::N(x), CellValue::Number(n)) => (x - n).abs() <= 1e-12 * x.abs().max(1.0),
            (Expect::T(t), CellValue::Text(v)) => *t == &**v,
            _ => false,
        };
        if !ok {
            failures.push(format!("{file} {at}: expected {expect:?}, Kiln {ours:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A time of day as LibreOffice writes it (`09:30:00 AM`, `17:05:10`) as a day fraction.
fn parse_lo_time(text: &str) -> Option<f64> {
    let t = text.trim();
    let (clock, pm) = match t.rsplit_once(' ') {
        Some((c, "AM")) => (c, Some(false)),
        Some((c, "PM")) => (c, Some(true)),
        _ => (t, None),
    };
    let mut p = clock.split(':');
    let mut h: f64 = p.next()?.trim().parse().ok()?;
    let mi: f64 = p.next()?.trim().parse().ok()?;
    let s: f64 = p.next().unwrap_or("0").trim().parse().ok()?;
    match pm {
        Some(true) if h < 12.0 => h += 12.0,
        Some(false) if h == 12.0 => h = 0.0,
        _ => {}
    }
    Some((h * 3600.0 + mi * 60.0 + s) / 86_400.0)
}

fn parse_lo_date(text: &str) -> Option<f64> {
    // `MM/DD/YYYY` or `YYYY-MM-DD`, optionally followed by ` HH:MM[:SS]`.
    if !text.contains(['/', '-']) {
        return parse_lo_time(text);
    }
    let (date, time) = text.split_once(' ').unwrap_or((text, ""));
    let (y, m, d) = if let Some((a, rest)) = date.split_once('/') {
        let (b, c) = rest.split_once('/')?;
        (
            c.parse::<i64>().ok()?,
            a.parse::<i64>().ok()?,
            b.parse::<i64>().ok()?,
        )
    } else {
        let mut p = date.split('-');
        let y = p.next()?.parse::<i64>().ok()?;
        let m = p.next()?.parse::<i64>().ok()?;
        let d = p.next()?.parse::<i64>().ok()?;
        (y, m, d)
    };
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1900 {
        return None;
    }
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468 + 25_569;
    let mut serial = days as f64;
    if !time.is_empty() {
        serial += parse_lo_time(time)?;
    }
    Some(serial)
}

/// Whether LibreOffice's text means Kiln's value (`date1904`: dates count from 1904).
fn agrees(ours: &CellValue, lo: &str, date1904: bool) -> bool {
    match ours {
        CellValue::Number(n) => {
            let close = |x: f64| (x - n).abs() <= 1e-9 * n.abs().max(x.abs()).max(1.0);
            let shift = if date1904 { 1_462.0 } else { 0.0 };
            lo.trim().parse::<f64>().is_ok_and(close)
                || lo
                    .trim()
                    .strip_suffix('%')
                    .and_then(|p| p.trim().parse::<f64>().ok())
                    .is_some_and(|p| close(p / 100.0))
                || parse_lo_date(lo.trim()).is_some_and(|d| {
                    // A time alone is not shifted.
                    if d < 1.0 { close(d) } else { close(d - shift) }
                })
        }
        CellValue::Bool(b) => lo == if *b { "TRUE" } else { "FALSE" },
        CellValue::Error(e) => lo == e.code(),
        CellValue::Blank => lo.is_empty(),
        CellValue::Text(t) => lo == &**t,
        CellValue::RichText(r) => lo == &**r.plain(),
    }
}

fn lo_sheets(oracle: &Oracle, file: &Path, dir: &Path) -> BTreeMap<String, Vec<Vec<String>>> {
    let _ = std::fs::remove_dir_all(dir);
    // Every sheet: LibreOffice writes `<stem>-<sheet>.csv` (so `convert` reports no `<stem>.csv`).
    let _ = oracle.convert(
        file,
        "csv:Text - txt - csv (StarCalc):44,34,76,1,,1033,false,true,false,false,false,-1",
        dir,
    );
    let stem = file.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let mut out = BTreeMap::new();
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(sheet) = name
            .strip_prefix(&format!("{stem}-"))
            .and_then(|n| n.strip_suffix(".csv"))
        else {
            continue;
        };
        let bytes = std::fs::read(e.path()).unwrap_or_default();
        let text = String::from_utf8_lossy(&bytes);
        out.insert(
            sheet.to_owned(),
            kiln_io_csv::parse_records(&text, kiln_io_csv::Dialect::default()),
        );
    }
    out
}

#[test]
fn formula_values_match_libreoffice() {
    let oracle = match Oracle::locate() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    let features = kiln_testing::fixtures::fixtures_dir().join("xlsx/features");
    let scratch = std::env::temp_dir().join(format!("kiln-xlsx-lo-{}", std::process::id()));
    let mut files: Vec<_> = std::fs::read_dir(&features)
        .expect("fixtures")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "xlsx"))
        .collect();
    files.sort();
    let mut compared = 0usize;
    let mut failures = Vec::new();
    let mut gaps_seen = Vec::new();
    for f in &files {
        let name = f
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_owned();
        let read = read_xlsx(&std::fs::read(f).expect("read")).expect("parse");
        let wb = &read.workbook;
        let lo = lo_sheets(&oracle, f, &scratch.join(&name));
        for s in wb.sheets() {
            let Some(rows) = lo.get(s.name()) else {
                if s.cells().iter().any(|(_, _, d)| d.kind.is_formula()) {
                    failures.push(format!(
                        "{name}: LibreOffice exported no sheet {}",
                        s.name()
                    ));
                }
                continue;
            };
            for (r, c, d) in s.cells().iter() {
                if !d.kind.is_formula() {
                    continue;
                }
                let ours = wb.value(s.id(), r, c);
                let theirs = rows
                    .get(r as usize)
                    .and_then(|row| row.get(c as usize))
                    .cloned()
                    .unwrap_or_default();
                let at = format!("{}!{}", s.name(), kiln_io_xlsx::text::cell_ref(r, c));
                compared += 1;
                let formula = wb.formula(s.id(), r, c).unwrap_or_default();
                let gap = ORACLE_GAPS
                    .iter()
                    .find(|(file, cell, _)| *file == name && *cell == at);
                match (agrees(&ours, &theirs, wb.props().date1904), gap) {
                    (true, None) => {}
                    (true, Some(_)) => failures.push(format!(
                        "{name} {at}: listed as an oracle gap but now agrees ({ours:?}); remove it"
                    )),
                    (false, Some(_)) => gaps_seen.push(at),
                    (false, None) => failures.push(format!(
                        "{name} {at} ={formula}: Kiln {ours:?}, LibreOffice {theirs:?}"
                    )),
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);
    eprintln!(
        "compared {compared} formula cells; {} documented oracle gaps",
        gaps_seen.len()
    );
    assert!(compared > 300, "only {compared} formula cells compared");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn libreoffice_dates_parse() {
    assert_eq!(parse_lo_date("01/15/2026"), Some(46_037.0));
    assert_eq!(parse_lo_date("2026-01-15"), Some(46_037.0));
    assert_eq!(parse_lo_date("2026-01-15 12:00:00"), Some(46_037.5));
    assert!(agrees(&CellValue::Number(0.12), "12%", false));
    assert!(agrees(&CellValue::Number(44_576.0), "2026-01-16", true));
    assert_eq!(parse_lo_time("09:30:00 AM"), Some(0.395_833_333_333_333_3));
    assert_eq!(parse_lo_time("12:00:00 AM"), Some(0.0));
}

/// `kiln-cli convert book.xlsx book.csv` writes the active sheet as displayed; LibreOffice's own
/// conversion of the workbook (first sheet) must hold the same values: text equal, numbers
/// within 1e-9 — LibreOffice shows the results of `DATE`, `TIME`, `IRR`… in a date, time or
/// percent format the file does not give those cells (Excel applies such formats when a formula
/// is typed, not when a file is opened), so its dates, times and percentages are read back as
/// numbers. The cells of [`ORACLE_GAPS`] are left out.
#[test]
fn csv_export_matches_libreoffice_csv() {
    let oracle = match Oracle::locate() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    };
    let file = kiln_testing::fixtures::fixtures_dir().join("xlsx/features/formulas.xlsx");
    let read = read_xlsx(&std::fs::read(&file).expect("fixture")).expect("read");
    let wb = &read.workbook;
    let sheet = wb.sheets()[wb.props().active_sheet].id();
    let ours = String::from_utf8(kiln_io_csv::write_delimited(
        wb,
        sheet,
        &kiln_io_csv::ExportOptions::default(),
    ))
    .expect("utf-8");
    let dir = std::env::temp_dir().join(format!("kiln-xlsx-csv-{}", std::process::id()));
    let lo_path = oracle.convert(&file, "csv", &dir).expect("LibreOffice csv");
    let lo_bytes = std::fs::read(&lo_path).expect("csv");
    let _ = std::fs::remove_dir_all(&dir);
    let theirs = kiln_io_csv::parse_records(
        &String::from_utf8_lossy(&lo_bytes),
        kiln_io_csv::Dialect::default(),
    );
    let ours = kiln_io_csv::parse_records(&ours, kiln_io_csv::Dialect::default());
    assert_eq!(ours.len(), theirs.len(), "rows");
    let mut compared = 0;
    let mut failures = Vec::new();
    for (r, (a, b)) in ours.iter().zip(&theirs).enumerate() {
        // Every row must be as wide as LibreOffice's, or cells it writes would go unchecked.
        if a.len() != b.len() {
            failures.push(format!(
                "row {}: Kiln wrote {} fields, LibreOffice {}",
                r + 1,
                a.len(),
                b.len()
            ));
        }
        for (c, (x, y)) in a.iter().zip(b).enumerate() {
            let at = format!(
                "Formulas!{}",
                kiln_io_xlsx::text::cell_ref(r as u32, c as u32)
            );
            if ORACLE_GAPS
                .iter()
                .any(|(f, cell, _)| *f == "formulas.xlsx" && *cell == at)
            {
                continue;
            }
            compared += 1;
            let ours_value = match x.parse::<f64>() {
                Ok(n) => CellValue::Number(n),
                Err(_) => CellValue::text(x.as_str()),
            };
            if !agrees(&ours_value, y, false) {
                failures.push(format!("{at}: Kiln {x:?}, LibreOffice {y:?}"));
            }
        }
    }
    assert!(compared > 800, "{compared}");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
