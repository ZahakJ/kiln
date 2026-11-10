// SPDX-License-Identifier: AGPL-3.0-or-later
//! PivotTables and sparklines in `.xlsx`: Excel's XML read into the model and the report
//! Kiln lays out equal to the cells Excel saved; unedited PivotTables keep their parts byte for
//! byte; edited ones get a regenerated cache of their own; PivotTables Kiln makes open in
//! LibreOffice, which refreshes them to the same values (skipped where LibreOffice is not
//! installed, as on CI); what the model cannot hold stays a static table with its parts.

use std::io::{Read, Write};
use std::path::Path;
use std::sync::Arc;

use kiln_io_xlsx::{WriteOptions, read_xlsx, rewrite_xlsx, write_xlsx};
use kiln_sheet::commands::MetaField;
use kiln_sheet::pivot::render::render;
use kiln_sheet::pivot::{
    AxisItem, PivotArea, PivotCache, PivotFunction, PivotLayout, PivotSort, PivotSource,
    PivotTable, ShowAs,
};
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId, Workbook};

fn fixture(name: &str) -> Vec<u8> {
    kiln_testing::fixtures::find(name)
        .and_then(|f| f.read())
        .unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn sheet_id(wb: &Workbook, name: &str) -> SheetId {
    wb.sheet_by_name(name)
        .map(kiln_sheet::Sheet::id)
        .expect("sheet")
}

/// The report cells as text, row by row, over a range.
fn grid(wb: &Workbook, sid: SheetId, r: Range) -> Vec<Vec<String>> {
    (r.r0..=r.r1)
        .map(|row| {
            (r.c0..=r.c1)
                .map(|col| match wb.value(sid, row, col) {
                    CellValue::Number(n) => format!("{:.6}", n),
                    v => v.display_general(),
                })
                .collect()
        })
        .collect()
}

#[test]
fn excel_pivot_tables_read_into_the_model() {
    let read = read_xlsx(&fixture("pivot-tables.xlsx")).expect("read");
    assert!(read.warnings.is_empty(), "{:?}", read.warnings);
    let wb = &read.workbook;
    let report = wb.sheet_by_name("Report").expect("Report");
    let p = &report.pivots()[0];
    assert_eq!(p.name, "PivotTable1");
    assert_eq!(
        p.source,
        PivotSource::Range {
            sheet: "Data".into(),
            range: Range::new(0, 0, 24, 5)
        }
    );
    assert_eq!(p.at, CellPos::new(2, 0));
    assert_eq!(
        (p.rows.clone(), p.cols.clone()),
        (vec![AxisItem::Field(0)], vec![AxisItem::Field(2)])
    );
    assert_eq!(p.values[0].name, "Sum of Units");
    assert_eq!(p.options.layout, PivotLayout::Compact);
    assert_eq!(p.options.style.name, "PivotStyleLight16");
    assert_eq!(p.cache.records, 24);
    // The cache follows the file's item order (sorted) for a field without a sort type.
    assert_eq!(
        p.cache.fields[0].items,
        ["East", "North", "South", "West"]
            .map(CellValue::text)
            .to_vec()
    );
    let detail = wb.sheet_by_name("Detail").expect("Detail");
    let d = &detail.pivots()[0];
    assert_eq!(d.pages.len(), 1);
    assert_eq!(d.pages[0].field, 1);
    assert_eq!(d.rows, vec![AxisItem::Field(0), AxisItem::Field(3)]);
    assert_eq!(d.cols, vec![AxisItem::Values]);
    assert_eq!(d.values[1].function, PivotFunction::Average);
    assert_eq!(d.values[1].num_fmt.as_deref(), Some("0.00"));
    assert_eq!(d.values[2].show_as, ShowAs::PercentOfTotal);
    assert_eq!(d.fields[0].hidden, vec![CellValue::text("North")]);
    assert_eq!(d.fields[3].sort, PivotSort::Descending);
    assert_eq!(d.fields[3].sort_by_value, Some(0));
    // Kiln's layout of both reports is the one Excel saved, cell for cell.
    for (sheet, t) in [("Report", p), ("Detail", d)] {
        let sid = sheet_id(wb, sheet);
        let rep = render(t).expect("report");
        let range = t.whole_range().expect("range");
        let saved = grid(wb, sid, range);
        let mut ours = vec![vec![String::new(); saved[0].len()]; saved.len()];
        for c in &rep.cells {
            let v = match &c.value {
                CellValue::Number(n) => format!("{:.6}", n),
                v => v.display_general(),
            };
            ours[(c.row - range.r0) as usize][(c.col - range.c0) as usize] = v;
        }
        assert_eq!(ours, saved, "{sheet}");
    }
}

fn part_text(bytes: &[u8], name: &str) -> Option<String> {
    part(bytes, name).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn part(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(bytes)).ok()?;
    let mut f = z.by_name(name.trim_start_matches('/')).ok()?;
    let mut out = Vec::new();
    f.read_to_end(&mut out).ok()?;
    Some(out)
}

const PARTS: [&str; 4] = [
    "xl/pivotTables/pivotTable1.xml",
    "xl/pivotTables/pivotTable2.xml",
    "xl/pivotCache/pivotCacheDefinition1.xml",
    "xl/pivotCache/pivotCacheRecords1.xml",
];

#[test]
fn unedited_pivot_tables_keep_their_parts() {
    let bytes = fixture("pivot-tables.xlsx");
    let out = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("rewrite");
    for p in PARTS {
        assert!(
            !out.regenerated.iter().any(|r| r.ends_with(p)),
            "{p} was regenerated"
        );
        assert_eq!(part_text(&out.bytes, p), part_text(&bytes, p), "{p}");
    }
    let again = read_xlsx(&out.bytes).expect("read again");
    assert_eq!(
        again
            .workbook
            .sheet_by_name("Detail")
            .map(|s| s.pivots().len()),
        Some(1)
    );
}

/// The model's PivotTable on a sheet, changed by `f`, its report re-laid out and stored.
fn edit(wb: &mut Workbook, sheet: &str, f: impl FnOnce(&mut PivotTable)) {
    let sid = sheet_id(wb, sheet);
    let mut list = wb.sheet(sid).map(|s| s.pivots().to_vec()).expect("pivots");
    f(&mut list[0]);
    list[0].output = render(&list[0]).expect("report").output;
    wb.apply(&Command::SetSheetMeta {
        sheet: sid,
        field: Box::new(MetaField::Pivots(list)),
    })
    .expect("apply");
}

#[test]
fn an_edited_pivot_table_gets_a_cache_of_its_own() {
    let bytes = fixture("pivot-tables.xlsx");
    let mut read = read_xlsx(&bytes).expect("read");
    edit(&mut read.workbook, "Report", |t| {
        t.place(Some(1), PivotArea::Filters, None, None);
        t.at = CellPos::new(3, 0);
        t.options.layout = PivotLayout::Tabular;
    });
    let out =
        write_xlsx(&read.workbook, Some(&read.source), &WriteOptions::default()).expect("write");
    assert!(
        out.regenerated
            .iter()
            .any(|r| r.ends_with("pivotTables/pivotTable1.xml"))
    );
    // PivotTable2 keeps the shared cache as it was; PivotTable1's is new.
    assert_eq!(
        part_text(&out.bytes, "xl/pivotTables/pivotTable2.xml"),
        part_text(&bytes, "xl/pivotTables/pivotTable2.xml")
    );
    assert_eq!(
        part_text(&out.bytes, "xl/pivotCache/pivotCacheDefinition1.xml"),
        part_text(&bytes, "xl/pivotCache/pivotCacheDefinition1.xml")
    );
    let cache2 = part(&out.bytes, "xl/pivotCache/pivotCacheDefinition2.xml").expect("a new cache");
    let text = String::from_utf8(cache2).expect("utf-8");
    assert!(
        text.contains(r#"<worksheetSource ref="A1:F25" sheet="Data"/>"#),
        "{text}"
    );
    assert!(text.contains(r#"refreshOnLoad="1""#));
    let wbx =
        String::from_utf8(part(&out.bytes, "xl/workbook.xml").expect("workbook")).expect("utf-8");
    assert_eq!(wbx.matches("<pivotCache ").count(), 2, "{wbx}");
    let again = read_xlsx(&out.bytes).expect("read again");
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    let p = &again
        .workbook
        .sheet_by_name("Report")
        .expect("Report")
        .pivots()[0];
    assert_eq!(p.pages.len(), 1);
    assert_eq!(p.options.layout, PivotLayout::Tabular);
    assert_eq!(p.cache.records, 24);
    let d = &again
        .workbook
        .sheet_by_name("Detail")
        .expect("Detail")
        .pivots()[0];
    assert_eq!(d.values.len(), 3);
    // Deleting the PivotTable takes its parts and its cache with it.
    let mut read = read_xlsx(&out.bytes).expect("read");
    let sid = sheet_id(&read.workbook, "Report");
    read.workbook
        .apply(&Command::SetSheetMeta {
            sheet: sid,
            field: Box::new(MetaField::Pivots(Vec::new())),
        })
        .expect("apply");
    let out =
        write_xlsx(&read.workbook, Some(&read.source), &WriteOptions::default()).expect("write");
    assert!(part(&out.bytes, "xl/pivotCache/pivotCacheDefinition2.xml").is_none());
    assert!(part(&out.bytes, "xl/pivotTables/pivotTable1.xml").is_none());
    assert!(part(&out.bytes, "xl/pivotTables/pivotTable2.xml").is_some());
}

/// Region, Product, Amount data and a PivotTable over it, made in Kiln.
fn kiln_book() -> Workbook {
    let mut wb = Workbook::new();
    let data = wb.first_sheet().expect("sheet");
    let rows = [
        ("Region", "Product", "Amount"),
        ("East", "Apple", "10"),
        ("West", "Banana", "5"),
        ("East", "Banana", "20"),
        ("West", "Apple", "7"),
        ("East", "Apple", "1"),
        ("North", "Cherry", "12"),
    ];
    let values = rows
        .iter()
        .map(|(a, b, c)| {
            vec![
                Some(CellValue::text(*a)),
                Some(CellValue::text(*b)),
                Some(
                    c.parse::<f64>()
                        .map_or_else(|_| CellValue::text(*c), CellValue::Number),
                ),
            ]
        })
        .collect();
    wb.apply(&Command::SetRange {
        sheet: data,
        row: 0,
        col: 0,
        values,
    })
    .expect("data");
    wb.apply(&Command::AddSheet {
        name: Some("Report".into()),
        index: None,
        id: None,
    })
    .expect("sheet");
    let source = PivotSource::Range {
        sheet: "Sheet1".into(),
        range: Range::new(0, 0, 6, 2),
    };
    let cache = PivotCache::read(&wb, &source).expect("cache");
    let mut t = PivotTable::new("Sales", source, CellPos::new(2, 0), cache);
    t.place(Some(0), PivotArea::Rows, None, None);
    t.place(Some(1), PivotArea::Columns, None, None);
    t.place(Some(2), PivotArea::Values, None, None);
    t.fields[0].sort = PivotSort::Descending;
    let rep = render(&t).expect("report");
    t.output = rep.output.clone();
    let sid = sheet_id(&wb, "Report");
    wb.apply(&Command::SetCells {
        sheet: sid,
        cells: rep
            .cells
            .iter()
            .map(|c| kiln_sheet::commands::CellInput::value(c.row, c.col, c.value.clone()))
            .collect(),
    })
    .expect("cells");
    wb.apply(&Command::SetSheetMeta {
        sheet: sid,
        field: Box::new(MetaField::Pivots(vec![t])),
    })
    .expect("pivot");
    wb
}

#[test]
fn a_kiln_pivot_table_saves_and_opens_again() {
    let wb = kiln_book();
    let out = write_xlsx(&wb, None, &WriteOptions::default()).expect("write");
    let again = read_xlsx(&out.bytes).expect("read");
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    let a = &wb.sheet_by_name("Report").expect("Report").pivots()[0];
    let b = &again
        .workbook
        .sheet_by_name("Report")
        .expect("Report")
        .pivots()[0];
    assert_eq!(a.rows, b.rows);
    assert_eq!(a.cols, b.cols);
    assert_eq!(a.values, b.values);
    assert_eq!(a.fields[0].sort, b.fields[0].sort);
    assert_eq!(a.options, b.options);
    assert_eq!(a.output, b.output);
    assert_eq!(a.cache.records, b.cache.records);
    let sid = sheet_id(&again.workbook, "Report");
    let r = b.whole_range().expect("range");
    assert_eq!(
        grid(&again.workbook, sid, r),
        grid(&wb, sheet_id(&wb, "Report"), r)
    );
}

/// LibreOffice's view of a file through UNO (`tests/oracle/libreoffice-uno.py`), or `None` when
/// LibreOffice is not installed.
fn libreoffice(path: &Path, password: Option<&str>) -> Option<serde_json::Value> {
    if kiln_testing::oracle::Oracle::locate().is_err() {
        eprintln!("LibreOffice is not installed: skipped");
        return None;
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/oracle/libreoffice-uno.py");
    let mut cmd = std::process::Command::new("python3");
    cmd.arg(script).arg(path).stdin(std::process::Stdio::null());
    if let Some(p) = password {
        cmd.arg(p);
    }
    let out = cmd.output().expect("python3");
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        if err.contains("No module named 'uno'") {
            eprintln!("LibreOffice's Python is not available: skipped");
            return None;
        }
        panic!("the LibreOffice oracle failed: {err}");
    }
    Some(serde_json::from_slice(&out.stdout).expect("json"))
}

#[test]
fn libreoffice_refreshes_a_kiln_pivot_table_to_the_same_values() {
    let wb = kiln_book();
    let out = write_xlsx(&wb, None, &WriteOptions::default()).expect("write");
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("pivots-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp");
    let path = dir.join("kiln-pivot.xlsx");
    std::fs::File::create(&path)
        .and_then(|mut f| f.write_all(&out.bytes))
        .expect("file");
    let Some(lo) = libreoffice(&path, None) else {
        return;
    };
    let sheets = lo["sheets"].as_array().expect("sheets");
    let report = sheets
        .iter()
        .find(|s| s["name"] == "Report")
        .expect("Report");
    let cells = &report["pivots"][0]["cells"];
    let rows: Vec<Vec<String>> = cells
        .as_array()
        .expect("cells")
        .iter()
        .map(|r| {
            r.as_array()
                .expect("row")
                .iter()
                .map(|c| c.as_str().unwrap_or("").to_owned())
                .collect()
        })
        .collect();
    // LibreOffice lays out its own captions ("Total Result", the field names); the regions
    // (largest first) and their numbers must agree with Kiln's.
    let body: Vec<(String, Vec<String>)> = rows
        .iter()
        .filter(|r| ["West", "North", "East"].contains(&r[0].as_str()))
        .map(|r| (r[0].clone(), r[1..].to_vec()))
        .collect();
    assert_eq!(
        body.iter().map(|b| b.0.as_str()).collect::<Vec<_>>(),
        ["West", "North", "East"],
        "{rows:?}"
    );
    let sid = sheet_id(&wb, "Report");
    for (label, values) in body {
        let row = (3..10)
            .find(|&r| wb.value(sid, r, 0).display_general() == label)
            .expect("row");
        let ours: Vec<String> = (1..=4)
            .map(|c| wb.value(sid, row, c).display_general())
            .collect();
        assert_eq!(values, ours, "{label}");
    }
}

#[test]
fn what_the_model_cannot_hold_stays_a_static_table() {
    // A calculated field: kept as it is.
    let bytes = fixture("pivot-tables.xlsx");
    let mut z = zip::ZipArchive::new(std::io::Cursor::new(&bytes)).expect("zip");
    let mut out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for i in 0..z.len() {
        let mut f = z.by_index(i).expect("entry");
        let name = f.name().map(|n| n.to_string()).expect("name");
        let mut body = Vec::new();
        f.read_to_end(&mut body).expect("read");
        if name == "xl/pivotCache/pivotCacheDefinition1.xml" {
            body = String::from_utf8(body)
                .expect("utf-8")
                .replace(
                    r#"<cacheField name="Price" numFmtId="0">"#,
                    r#"<cacheField name="Price" numFmtId="0" formula="Units*2" databaseField="0">"#,
                )
                .into_bytes();
        }
        out.start_file(
            name,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored),
        )
        .expect("start");
        out.write_all(&body).expect("write");
    }
    let bytes = out.finish().expect("finish").into_inner();
    let read = read_xlsx(&bytes).expect("read");
    assert_eq!(read.warnings.len(), 2, "{:?}", read.warnings);
    assert!(read.warnings[0].contains("kept as a static table (a calculated field)"));
    assert!(read.workbook.sheets().iter().all(|s| s.pivots().is_empty()));
    let sid = sheet_id(&read.workbook, "Report");
    assert_eq!(
        read.workbook.value(sid, 3, 0).display_general(),
        "Row Labels"
    );
    let out = rewrite_xlsx(&bytes, &WriteOptions::default()).expect("rewrite");
    for p in PARTS {
        assert_eq!(part_text(&out.bytes, p), part_text(&bytes, p), "{p}");
    }
    let wbx =
        String::from_utf8(part(&out.bytes, "xl/workbook.xml").expect("workbook")).expect("utf-8");
    assert!(wbx.contains(r#"<pivotCache cacheId="5""#), "{wbx}");
}

#[test]
fn sparklines_read_and_write_back() {
    let bytes = fixture("sparklines.xlsx");
    let read = read_xlsx(&bytes).expect("read");
    let g = read
        .workbook
        .sheet_by_name("Trends")
        .expect("sheet")
        .meta()
        .sparklines
        .clone();
    let g = &g;
    assert_eq!(g.len(), 3);
    assert_eq!(g[0].kind, kiln_sheet::sparkline::SparklineKind::Line);
    assert!(g[0].markers && g[0].high && g[0].low);
    assert_eq!(
        g[0].colors.series,
        kiln_sheet::Color::theme(4, -0.499_984_740_745_262)
    );
    assert_eq!(g[1].kind, kiln_sheet::sparkline::SparklineKind::Column);
    assert_eq!(g[1].min, kiln_sheet::sparkline::SparkAxis::Group);
    assert_eq!(g[2].kind, kiln_sheet::sparkline::SparklineKind::WinLoss);
    assert_eq!(g[2].sparklines.len(), 6);
    assert_eq!(g[2].sparklines[5].at, CellPos::new(6, 9));
    assert_eq!(g[2].sparklines[5].data, Range::new(6, 1, 6, 6));
    // An edit is written and read back.
    let mut read = read;
    let sid = sheet_id(&read.workbook, "Trends");
    let mut groups = g.clone();
    groups[1].high = true;
    groups[1].colors.high = kiln_sheet::Color::rgb(0x1D_8A60);
    groups.remove(2);
    read.workbook
        .apply(&Command::SetSheetMeta {
            sheet: sid,
            field: Box::new(MetaField::Sparklines(groups.clone())),
        })
        .expect("apply");
    let out =
        write_xlsx(&read.workbook, Some(&read.source), &WriteOptions::default()).expect("write");
    let again = read_xlsx(&out.bytes).expect("again");
    assert_eq!(
        again
            .workbook
            .sheet_by_name("Trends")
            .map(|s| s.meta().sparklines.clone()),
        Some(groups)
    );
}

#[test]
fn the_cache_is_shared_as_an_arc() {
    // Copies of a sheet's metadata (undo steps) share the cache instead of copying its records.
    let read = read_xlsx(&fixture("pivot-tables.xlsx")).expect("read");
    let p = &read
        .workbook
        .sheet_by_name("Report")
        .expect("Report")
        .pivots()[0];
    let copy = p.clone();
    assert!(Arc::ptr_eq(&p.cache, &copy.cache));
}

/// A PivotTable made in Kiln on a sheet before the one holding the file's own PivotTable takes a
/// `cacheId` the file does not use (LibreOffice numbers its caches from 1, as Kiln would).
#[test]
fn a_new_pivot_table_takes_a_cache_id_the_file_does_not_use() {
    let bytes = fixture("pivot-libreoffice.xlsx");
    let mut read = read_xlsx(&bytes).expect("read");
    let wb = &mut read.workbook;
    let data = wb.sheets()[0].name().to_owned();
    wb.apply(&Command::AddSheet {
        name: Some("Kiln".into()),
        index: Some(0),
        id: None,
    })
    .expect("sheet");
    let source = PivotSource::Range {
        sheet: data,
        range: Range::new(0, 0, 3, 1),
    };
    let cache = PivotCache::read(wb, &source).expect("cache");
    let mut t = PivotTable::new("Kiln", source, CellPos::new(2, 0), cache);
    t.place(Some(0), PivotArea::Rows, None, None);
    let sid = sheet_id(wb, "Kiln");
    wb.apply(&Command::SetSheetMeta {
        sheet: sid,
        field: Box::new(MetaField::Pivots(vec![t])),
    })
    .expect("pivot");
    let out = write_xlsx(wb, Some(&read.source), &WriteOptions::default()).expect("write");
    let wbx =
        String::from_utf8(part(&out.bytes, "xl/workbook.xml").expect("workbook")).expect("utf-8");
    let ids: Vec<&str> = wbx
        .split("<pivotCache ")
        .skip(1)
        .filter_map(|p| p.split("cacheId=\"").nth(1)?.split('"').next())
        .collect();
    assert_eq!(ids.len(), 2, "{wbx}");
    assert_ne!(ids[0], ids[1], "{wbx}");
    // Each PivotTable names its own cache.
    let again = read_xlsx(&out.bytes).expect("read again");
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    let kiln = &again.workbook.sheet_by_name("Kiln").expect("Kiln").pivots()[0];
    assert_eq!(kiln.name, "Kiln");
    let theirs: Vec<_> = again
        .workbook
        .sheets()
        .iter()
        .filter(|s| s.name() != "Kiln")
        .flat_map(|s| s.pivots().iter())
        .collect();
    assert_eq!(theirs.len(), 1);
    assert_ne!(theirs[0].cache.fields.len(), 0);
}
