// SPDX-License-Identifier: AGPL-3.0-or-later
//! `parse(print(ast))` round-trips: a curated corpus covering every construct of the grammar, a
//! seeded random generator (property-style), and every formula of the conformance suite
//! (`conformance/**/*.csv`, more than 4,500 formulas).

use std::path::{Path, PathBuf};

use kiln_formula::parser::{ParseOptions, PrintOptions, parse, print};
use kiln_formula::refs::column_to_letters;

const CURATED: &[&str] = &[
    // literals
    "=1",
    "=0.5",
    "=.25",
    "=1E+21",
    "=1.5E-7",
    "=123456789012345",
    "=\"text\"",
    "=\"\"",
    "=\"quote \"\" inside\"",
    "=TRUE",
    "=FALSE",
    "=#N/A",
    "=#DIV/0!",
    "=#REF!",
    "=#NAME?",
    "=#NULL!",
    "=#NUM!",
    "=#VALUE!",
    "=#SPILL!",
    "=#CALC!",
    // references
    "=A1",
    "=$A$1",
    "=A$1",
    "=$A1",
    "=XFD1048576",
    "=A1:B2",
    "=$A$1:$B$2",
    "=A:A",
    "=$A:$C",
    "=1:1",
    "=$2:$10",
    "=Sheet1!A1",
    "=Sheet1!$A$1:$C$9",
    "='My Sheet'!A1",
    "='It''s'!B2",
    "='2024'!A1",
    "='A1'!B2",
    "='R1C1'!B2",
    "=Sheet1:Sheet3!A1",
    "='Q 1:Q 4'!B2:C3",
    "=Jan:Dec!A:A",
    "=[1]Sheet1!A1",
    "='[Book 1.xlsx]Data'!A1",
    "=Sheet1!#REF!",
    "=Données!A1",
    "=Sheet1!Total",
    "='Sheet 2'!Sales",
    // names and structured references
    "=Sales",
    "=TaxRate*2",
    "=_total",
    "=Revenue.Q1",
    "=Table1",
    "=Table1[]",
    "=Table1[Amount]",
    "=Table1[[Unit Price]]",
    "=Table1[#All]",
    "=Table1[#Data]",
    "=Table1[#Headers]",
    "=Table1[#Totals]",
    "=Table1[[#Headers],[Amount]]",
    "=Table1[[#Headers],[#Data],[Jan]:[Mar]]",
    "=Table1[[#Data],[#Totals]]",
    "=Table1[[Jan]:[Mar]]",
    "=Table1[@]",
    "=Table1[@Amount]",
    "=Table1[@[Unit Price]]",
    "=Table1[@[Jan]:[Mar]]",
    "=Table1[[#This Row],[Amount]]",
    "=[@Qty]*[@Price]",
    "=[@[Unit Price]]",
    "=[Amount]",
    "=Table1[[Col'#1]]",
    "=Table1[[It''s]]",
    "=Table1[[a'[b']]]",
    "=SUM(Table1[Amount])",
    "=COUNTA(Table1[[#Headers],[#Data],[A]])",
    // array constants
    "={1,2,3}",
    "={1;2;3}",
    "={1,2;3,4}",
    "={-1,2.5;\"a\",TRUE}",
    "={#N/A,\"x\";FALSE,0}",
    "=SUM({1,2;3,4})",
    // operators
    "=1+2",
    "=1-2",
    "=2*3",
    "=6/3",
    "=2^3",
    "=2^3^2",
    "=-2^2",
    "=2^-1",
    "=5%",
    "=-5%",
    "=50%*A1",
    "=\"a\"&\"b\"",
    "=A1&B1&C1",
    "=1=1",
    "=1<>2",
    "=1<2",
    "=1>2",
    "=1<=2",
    "=1>=2",
    "=--A1",
    "=+A1",
    "=-(-A1)",
    "=(1+2)*3",
    "=((1))",
    "=A1:B2 B1:C3",
    "=(A1,B2,C3)",
    "=SUM((A1:A3,C1:C3))",
    "=A1:A3:C2",
    "=A1:INDEX(B:B,5)",
    "=@A1:A10",
    "=@Table1[Amount]",
    "=A1#",
    "='My Sheet'!B2#",
    "=SUM(A1#)",
    "=COUNT(Sheet2!A1#)",
    // functions
    "=NOW()",
    "=TODAY()",
    "=PI()",
    "=SUM(1,2,3)",
    "=SUM(A1:A10,B1:B10)",
    "=IF(A1,,2)",
    "=IF(A1>0,\"pos\",IF(A1<0,\"neg\",\"zero\"))",
    "=IFERROR(1/0,\"\")",
    "=VLOOKUP(A1,Sheet2!A:C,3,FALSE)",
    "=XLOOKUP(A1,B:B,C:C,\"none\",0,-1)",
    "=INDEX(A1:C10,MATCH(D1,B1:B10,0),3)",
    "=SUMPRODUCT((A1:A10>5)*(B1:B10))",
    "=TEXTJOIN(\", \",TRUE,A1:A5)",
    "=LOG10(100)",
    "=ATAN2(1,1)",
    "=DEC2BIN(10)",
    "=_xlfn.XLOOKUP(1,A:A,B:B)",
    "=_xlfn._xlws.SORT(A1:A10)",
    "=STDEV.S(A1:A10)",
    "=F.DIST.RT(1,2,3)",
    "=LET(x,2,y,x*3,x+y)",
    "=LAMBDA(x,x*2)(21)",
    "=LAMBDA(a,b,a+b)(1,2)",
    "=LAMBDA(x,LAMBDA(y,x+y))(1)(2)",
    "=MAP(A1:A3,LAMBDA(v,v*2))",
    "=REDUCE(0,A1:A10,LAMBDA(acc,v,acc+v))",
    "=SCAN(,A1:A3,LAMBDA(a,b,a+b))",
    "=MAKEARRAY(3,3,LAMBDA(r,c,r*c))",
    "=BYROW(A1:C3,LAMBDA(r,SUM(r)))",
    "=SEQUENCE(10,2,0,0.5)",
    "=FILTER(A1:B10,B1:B10>5,\"none\")",
    "=SORT(UNIQUE(A1:A100),1,-1)",
    "=TRANSPOSE({1,2,3})",
    "=OFFSET(A1,1,1,2,2)",
    "=INDIRECT(\"Sheet2!A\"&ROW())",
    "=SUM(INDIRECT(\"A1:A\"&B1))",
    "=TRUE()",
    "=IF(TRUE(),FALSE(),1)",
    "=CONCAT(\"a\",,\"b\")",
    "=SUM(Jan:Dec!B2)",
    "=MyLambda(1,2)",
    "=DATE(2024,1,15)+TIME(12,0,0)",
];

/// mulberry32, as in the TypeScript generator, so the corpus matches v1.0's.
struct Rng(u32);

impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
    fn int(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() * (hi - lo + 1) as f64).floor() as i64
    }
    fn pick<'a>(&mut self, xs: &[&'a str]) -> &'a str {
        xs[(self.next() * xs.len() as f64).floor() as usize]
    }
}

const FUNCS: &[&str] = &[
    "SUM", "IF", "MAX", "CONCAT", "INDEX", "XLOOKUP", "LEN", "ROUND", "F.DIST", "LOG10",
];
const NAMES: &[&str] = &["Sales", "TaxRate", "_x", "Revenue.Q1", "λ"];
const SHEETS: &[&str] = &["Sheet1", "My Sheet", "O'Brien", "Data2"];
const BIN: &[&str] = &[
    "+", "-", "*", "/", "^", "&", "=", "<>", "<", ">", "<=", ">=",
];
const ERRORS: &[&str] = &["#N/A", "#DIV/0!", "#VALUE!", "#REF!"];

fn quote_sheet(s: &str) -> String {
    let plain = s
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.');
    if plain {
        s.to_owned()
    } else {
        format!("'{}'", s.replace('\'', "''"))
    }
}

struct Gen(Rng);

impl Gen {
    fn cell(&mut self) -> String {
        let r = &mut self.0;
        let ca = if r.next() < 0.3 { "$" } else { "" };
        let col = column_to_letters(r.int(0, 700) as u32);
        let ra = if r.next() < 0.3 { "$" } else { "" };
        let row = r.int(1, 5000);
        format!("{ca}{col}{ra}{row}")
    }
    fn prefix(&mut self) -> String {
        let x = self.0.next();
        if x < 0.6 {
            return String::new();
        }
        if x < 0.85 {
            let s = self.0.pick(SHEETS);
            return format!("{}!", quote_sheet(s));
        }
        let a = self.0.pick(SHEETS);
        let b = self.0.pick(SHEETS);
        format!("{}!", quote_sheet(&format!("{a}:{b}")))
    }
    fn reference(&mut self) -> String {
        let x = self.0.next();
        if x < 0.45 {
            let p = self.prefix();
            return p + &self.cell();
        }
        if x < 0.8 {
            let p = self.prefix();
            let a = self.cell();
            let b = self.cell();
            return format!("{p}{a}:{b}");
        }
        if x < 0.9 {
            let p = self.prefix();
            let a = column_to_letters(self.0.int(0, 30) as u32);
            let b = column_to_letters(self.0.int(0, 30) as u32);
            return format!("{p}{a}:{b}");
        }
        let p = self.prefix();
        let a = self.0.int(1, 99);
        let b = self.0.int(1, 99);
        format!("{p}{a}:{b}")
    }
    fn num(&mut self) -> String {
        let x = self.0.next();
        if x < 0.5 {
            return self.0.int(0, 1000).to_string();
        }
        if x < 0.8 {
            let v = self.0.int(0, 10000) as f64 / 100.0;
            return kiln_formula::values::shortest_number_text(v);
        }
        if x < 0.9 {
            let a = self.0.int(1, 9);
            let b = self.0.int(21, 99);
            return format!("{a}E+{b}");
        }
        let a = self.0.int(1, 9);
        let b = self.0.int(7, 99);
        format!("{a}E-{b}")
    }
    fn atom(&mut self, d: i32) -> String {
        let x = self.0.next();
        if x < 0.15 {
            return self.num();
        }
        if x < 0.22 {
            let s = self.0.pick(&["a", "b c", "say \"\"hi\"\"", ""]);
            return format!("\"{s}\"");
        }
        if x < 0.25 {
            return self.0.pick(&["TRUE", "FALSE"]).to_owned();
        }
        if x < 0.28 {
            return self.0.pick(ERRORS).to_owned();
        }
        if x < 0.5 {
            return self.reference();
        }
        if x < 0.55 {
            return self.0.pick(NAMES).to_owned();
        }
        if x < 0.6 {
            return self
                .0
                .pick(&[
                    "T[Amount]",
                    "[@Qty]",
                    "T[[#Headers],[A]:[B]]",
                    "T[#All]",
                    "T[@[Unit Price]]",
                ])
                .to_owned();
        }
        if x < 0.63 {
            let n = self.0.int(-5, 5);
            return format!("{{{n},\"x\";TRUE,#N/A}}");
        }
        if x < 0.66 {
            return format!("{}#", self.cell());
        }
        if d <= 0 {
            return self.num();
        }
        if x < 0.75 {
            let n = self.0.int(0, 4);
            let mut args = Vec::new();
            for _ in 0..n {
                if self.0.next() < 0.15 {
                    args.push(String::new());
                } else {
                    args.push(self.expr(d - 1));
                }
            }
            let f = self.0.pick(FUNCS);
            return format!("{f}({})", args.join(","));
        }
        if x < 0.82 {
            return format!("({})", self.expr(d - 1));
        }
        if x < 0.86 {
            return format!("-{}", self.atom(d - 1));
        }
        if x < 0.88 {
            return format!("@{}", self.reference());
        }
        if x < 0.9 {
            return format!("{}%", self.atom(d - 1));
        }
        if x < 0.93 {
            let a = self.reference();
            let b = self.reference();
            return format!("({a},{b})");
        }
        if x < 0.96 {
            let a = self.reference();
            let b = self.reference();
            return format!("{a} {b}");
        }
        let body = self.expr(d - 1);
        let arg = self.expr(d - 1);
        format!("LAMBDA(x,{body})({arg})")
    }
    fn expr(&mut self, d: i32) -> String {
        let mut s = self.atom(d);
        let n = self.0.int(0, 3);
        for _ in 0..n {
            let op = self.0.pick(BIN);
            s.push_str(op);
            s.push_str(&self.atom(d - 1));
        }
        s
    }
}

fn generated(seed: u32, count: usize) -> Vec<String> {
    let mut g = Gen(Rng(seed));
    (0..count).map(|_| format!("={}", g.expr(3))).collect()
}

/// Asserts `parse(print(parse(src))) == parse(src)` (ignoring spans) and that printing is stable.
fn round_trip(src: &str) -> Result<(), String> {
    let opts = ParseOptions::default();
    let ast1 = parse(src, &opts).map_err(|e| format!("{src}: {e}"))?;
    let printed = print(&ast1, &PrintOptions::formula());
    let ast2 = parse(&printed, &opts).map_err(|e| format!("{src} → {printed}: {e}"))?;
    if ast2.without_spans() != ast1.without_spans() {
        return Err(format!("{src} → {printed}: different tree"));
    }
    let again = print(&ast2, &PrintOptions::formula());
    if again != printed {
        return Err(format!("{src} → {printed} → {again}"));
    }
    Ok(())
}

#[test]
fn curated_and_generated_formulas_round_trip() {
    let gen_list = generated(20_261_015, 420);
    let unique: std::collections::BTreeSet<&String> = gen_list.iter().collect();
    assert!(unique.len() > 400, "generated formulas are diverse");
    let all: Vec<String> = CURATED
        .iter()
        .map(|s| (*s).to_owned())
        .chain(gen_list)
        .collect();
    assert!(all.len() >= 300);
    assert!(all.iter().any(|f| f.contains("[#Headers]")));
    assert!(all.iter().any(|f| f.contains("Sheet1:Sheet3!")));
    assert!(all.iter().any(|f| f.contains('{')));
    assert!(all.iter().any(|f| f.contains("'My Sheet'!")));
    let failures: Vec<String> = all.iter().filter_map(|f| round_trip(f).err()).collect();
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn prints_canonical_text() {
    let pr = |s: &str| {
        parse(s, &ParseOptions::default())
            .map(|n| print(&n, &PrintOptions::default()))
            .unwrap_or_default()
    };
    assert_eq!(
        parse("= sum( a1 , 2 )", &ParseOptions::default())
            .map(|n| print(&n, &PrintOptions::formula()))
            .ok()
            .as_deref(),
        Some("=SUM(A1,2)")
    );
    assert_eq!(pr("=Table1[[#This Row],[Amount]]"), "Table1[@Amount]");
    assert_eq!(pr("=T[[#Headers],[Col'#1]]"), "T[[#Headers],[Col'#1]]");
    assert_eq!(pr("=T[[A]]"), "T[A]");
    assert_eq!(pr("='Sheet1'!A1"), "Sheet1!A1");
    assert_eq!(pr("=1e21"), "1E+21");
    assert_eq!(pr("=0.0000001"), "1E-7");
}

/// Minimal RFC 4180 reader (quoted fields, doubled quotes, embedded newlines).
fn read_csv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if in_q {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_q = false;
                }
            } else {
                cur.push(ch);
            }
        } else if ch == '"' {
            in_q = true;
        } else if ch == ',' {
            row.push(std::mem::take(&mut cur));
        } else if ch == '\n' {
            row.push(std::mem::take(&mut cur));
            rows.push(std::mem::take(&mut row));
        } else if ch != '\r' {
            cur.push(ch);
        }
    }
    if !cur.is_empty() || !row.is_empty() {
        row.push(cur);
        rows.push(row);
    }
    rows
}

fn csv_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            csv_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "csv") {
            out.push(p);
        }
    }
}

#[test]
fn every_conformance_formula_round_trips() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance");
    let mut files = Vec::new();
    csv_files(&dir, &mut files);
    let mut formulas = Vec::new();
    for f in &files {
        let Ok(text) = std::fs::read_to_string(f) else {
            continue;
        };
        let rows = read_csv(&text);
        let Some(header) = rows.first() else { continue };
        let Some(col) = header.iter().position(|h| h == "formula") else {
            continue;
        };
        for r in &rows[1..] {
            if let Some(cell) = r.get(col)
                && cell.starts_with('=')
            {
                formulas.push(cell.clone());
            }
        }
    }
    assert!(
        formulas.len() >= 4500,
        "only {} conformance formulas found",
        formulas.len()
    );
    let failures: Vec<String> = formulas
        .iter()
        .filter_map(|f| round_trip(f).err())
        .collect();
    assert!(
        failures.is_empty(),
        "{} of {} formulas failed: {:#?}",
        failures.len(),
        formulas.len(),
        &failures[..failures.len().min(20)]
    );
    println!("round-tripped {} conformance formulas", formulas.len());
}
