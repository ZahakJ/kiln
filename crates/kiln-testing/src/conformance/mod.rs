// SPDX-License-Identifier: AGPL-3.0-or-later
//! The formula conformance runner as a library (PLAN.md §5).
//!
//! `conformance/*.csv` (and subdirectories such as `conformance/core/`) hold rows
//! `formula,expected,type,notes`: a formula, the value Excel (or the LibreOffice oracle) gives, the
//! value's type (`number`, `date`, `text`, `boolean`, `error`, `array`, or `skip`) and free notes
//! (the word `rounded` compares numbers to the precision the expected literal was written with).
//! Every row is evaluated next to the shared `Data` sheet of `conformance/fixture.json`.
//!
//! The runner is independent of the engine: callers convert their values to [`Cell`] and pass an
//! evaluator closure to [`run`]. Comparison is type-aware, as in the v1.0 runner: numbers to 1e-9
//! relative (1e-14 absolute near zero), errors by code, text and logicals exactly, arrays element
//! by element; a non-array row compares the top-left value of an array result (what one cell
//! shows).

use std::collections::BTreeSet;
use std::fmt;
use std::path::Path;

/// The expected type of a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// A number.
    Number,
    /// A date serial (compared as a number).
    Date,
    /// Text (the expected column holds the bare text).
    Text,
    /// `TRUE` or `FALSE`.
    Boolean,
    /// An error code such as `#N/A`.
    Error,
    /// An array literal `{1,2;3,4}`.
    Array,
    /// A row that is kept for reference but not evaluated.
    Skip,
}

impl Kind {
    /// Parses the `type` column.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "number" => Self::Number,
            "date" => Self::Date,
            "text" => Self::Text,
            "boolean" => Self::Boolean,
            "error" => Self::Error,
            "array" => Self::Array,
            "skip" => Self::Skip,
            _ => return None,
        })
    }
}

/// One row of a conformance CSV.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Case {
    /// The CSV file name.
    pub file: String,
    /// 1-based line number (the header is line 1).
    pub line: usize,
    /// The formula, with its `=`.
    pub formula: String,
    /// The expected value as written.
    pub expected: String,
    /// The expected type.
    pub kind: Kind,
    /// Notes.
    pub notes: String,
}

impl Case {
    /// Whether numbers compare to the precision the expected literal was written with.
    #[must_use]
    pub fn rounded(&self) -> bool {
        self.notes
            .split(|c: char| !c.is_alphanumeric())
            .any(|w| w == "rounded")
    }

    /// `file:line` for messages.
    #[must_use]
    pub fn label(&self) -> String {
        format!("{}:{}", self.file, self.line)
    }
}

/// A value as the runner compares it.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    /// An empty cell.
    Blank,
    /// A number.
    Number(f64),
    /// Text.
    Text(String),
    /// A logical.
    Bool(bool),
    /// An error, by its code (`#DIV/0!`).
    Error(String),
    /// A rectangular array, row by row.
    Array(Vec<Vec<Cell>>),
}

impl fmt::Display for Cell {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Blank => f.write_str("(blank)"),
            Self::Number(n) => write!(f, "{n}"),
            Self::Text(s) => write!(f, "\"{}\"", s.replace('"', "\"\"")),
            Self::Bool(b) => f.write_str(if *b { "TRUE" } else { "FALSE" }),
            Self::Error(e) => f.write_str(e),
            Self::Array(rows) => {
                f.write_str("{")?;
                for (i, r) in rows.iter().enumerate() {
                    if i > 0 {
                        f.write_str(";")?;
                    }
                    for (j, c) in r.iter().enumerate() {
                        if j > 0 {
                            f.write_str(",")?;
                        }
                        write!(f, "{c}")?;
                    }
                }
                f.write_str("}")
            }
        }
    }
}

/// Splits CSV text into rows of fields (RFC 4180 quoting, `\r\n` or `\n` line ends).
#[must_use]
pub fn read_csv(text: &str) -> Vec<Vec<String>> {
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

/// The header every conformance CSV starts with.
pub const HEADER: &str = "formula,expected,type,notes";

/// Parses one conformance CSV.
///
/// # Errors
///
/// A wrong header, a row with too few columns or an unknown type, with the line.
pub fn parse_cases(file: &str, text: &str) -> Result<Vec<Case>, String> {
    let rows = read_csv(text);
    let header = rows.first().map(|r| r.join(",")).unwrap_or_default();
    if header != HEADER {
        return Err(format!(
            "{file}: the header must be `{HEADER}`, not `{header}`"
        ));
    }
    let mut out = Vec::new();
    for (i, r) in rows.iter().enumerate().skip(1) {
        let line = i + 1;
        if r.iter().all(String::is_empty) {
            continue;
        }
        if r.len() < 3 {
            return Err(format!(
                "{file}:{line}: expected 4 columns, found {}",
                r.len()
            ));
        }
        let kind =
            Kind::parse(&r[2]).ok_or_else(|| format!("{file}:{line}: unknown type `{}`", r[2]))?;
        out.push(Case {
            file: file.to_owned(),
            line,
            formula: r[0].clone(),
            expected: r[1].clone(),
            kind,
            notes: r.get(3).cloned().unwrap_or_default(),
        });
    }
    Ok(out)
}

/// Every case of the `*.csv` files directly in `dir`, in file name order.
///
/// # Errors
///
/// I/O and parse errors.
pub fn load_dir(dir: &Path) -> Result<Vec<Case>, String> {
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "csv"))
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let name = f
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if name == "oracle-exclusions.csv" {
            continue;
        }
        let text = std::fs::read_to_string(&f).map_err(|e| format!("{}: {e}", f.display()))?;
        out.extend(parse_cases(&name, &text)?);
    }
    Ok(out)
}

/// The formulas of `oracle-exclusions.csv` (`formula,libreoffice,reason`): rows where the
/// LibreOffice oracle is known to disagree with Excel.
///
/// # Errors
///
/// I/O errors or a wrong header.
pub fn load_exclusions(path: &Path) -> Result<BTreeSet<String>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let rows = read_csv(&text);
    if rows.first().and_then(|r| r.first()).map(String::as_str) != Some("formula") {
        return Err(format!(
            "{}: the first column must be `formula`",
            path.display()
        ));
    }
    Ok(rows
        .into_iter()
        .skip(1)
        .filter_map(|r| r.into_iter().next())
        .filter(|f| !f.is_empty())
        .collect())
}

/// Splits an array literal's body into element texts, marking row ends.
fn split_elements(body: &str) -> Vec<(String, bool)> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_str = false;
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '"' {
            if in_str && chars.peek() == Some(&'"') {
                cur.push_str("\"\"");
                chars.next();
                continue;
            }
            in_str = !in_str;
            cur.push(ch);
        } else if !in_str && (ch == ',' || ch == ';') {
            out.push((cur.trim().to_owned(), ch == ';'));
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    out.push((cur.trim().to_owned(), true));
    out
}

fn expected_rows(text: &str) -> (Vec<Vec<String>>, bool) {
    let t = text.trim();
    if !(t.starts_with('{') && t.ends_with('}') && t.len() >= 2) {
        return (vec![vec![t.to_owned()]], false);
    }
    let mut rows: Vec<Vec<String>> = vec![Vec::new()];
    for (el, row_end) in split_elements(&t[1..t.len() - 1]) {
        if let Some(r) = rows.last_mut() {
            r.push(el);
        }
        if row_end {
            rows.push(Vec::new());
        }
    }
    rows.pop();
    (rows, true)
}

/// Tolerance implied by how many digits a number literal was written with.
fn display_tolerance(lit: &str) -> f64 {
    let t = lit.trim().trim_start_matches(['+', '-']);
    let (mant, exp) = match t.split_once(['e', 'E']) {
        Some((m, e)) => (m, e.parse::<i32>().unwrap_or(0)),
        None => (t, 0),
    };
    let (int, frac) = mant.split_once('.').unwrap_or((mant, ""));
    let sig = format!("{}{frac}", int.trim_start_matches('0'))
        .trim_start_matches('0')
        .len();
    let v: f64 = lit.trim().parse().unwrap_or(0.0);
    if sig >= 15 {
        return v.abs() * 1e-13;
    }
    let digits = i32::try_from(frac.len()).unwrap_or(i32::MAX);
    0.5 * 10f64.powi(exp.saturating_sub(digits)) * (1.0 + 1e-9)
}

fn scalar_matches(actual: &Cell, lit: &str, rounded: bool) -> bool {
    let lit = lit.trim();
    if lit.len() >= 2 && lit.starts_with('"') && lit.ends_with('"') {
        let want = lit[1..lit.len() - 1].replace("\"\"", "\"");
        return matches!(actual, Cell::Text(s) if *s == want);
    }
    if lit.eq_ignore_ascii_case("TRUE") {
        return *actual == Cell::Bool(true);
    }
    if lit.eq_ignore_ascii_case("FALSE") {
        return *actual == Cell::Bool(false);
    }
    if lit.starts_with('#') {
        return matches!(actual, Cell::Error(e) if e.eq_ignore_ascii_case(lit));
    }
    if lit.is_empty() {
        return matches!(actual, Cell::Blank)
            || *actual == Cell::Number(0.0)
            || matches!(actual, Cell::Text(s) if s.is_empty());
    }
    let Ok(want) = lit.parse::<f64>() else {
        return false;
    };
    let Cell::Number(got) = actual else {
        return false;
    };
    let tol = if rounded {
        display_tolerance(lit)
    } else {
        (want.abs() * 1e-9).max(1e-14)
    };
    (got - want).abs() <= tol
}

/// Compares an actual value with a case's expectation.
///
/// # Errors
///
/// A message saying what was expected and what came out.
pub fn compare(actual: &Cell, case: &Case) -> Result<(), String> {
    let expected = if case.kind == Kind::Text {
        format!("\"{}\"", case.expected.replace('"', "\"\""))
    } else {
        case.expected.clone()
    };
    let rounded = case.rounded();
    // One cell shows the top-left value of an array result (what the oracle saw).
    let top_left;
    let actual = match actual {
        Cell::Array(rows) if case.kind != Kind::Array => {
            top_left = rows
                .first()
                .and_then(|r| r.first())
                .cloned()
                .unwrap_or(Cell::Blank);
            &top_left
        }
        other => other,
    };
    let (rows, is_array) = expected_rows(&expected);
    if !is_array {
        return if !matches!(actual, Cell::Array(_)) && scalar_matches(actual, &rows[0][0], rounded)
        {
            Ok(())
        } else {
            Err(format!("expected {expected}, got {actual}"))
        };
    }
    let single;
    let got: &[Vec<Cell>] = match actual {
        Cell::Array(r) => r,
        other => {
            single = vec![vec![other.clone()]];
            &single
        }
    };
    let (r, c) = (rows.len(), rows.first().map_or(0, Vec::len));
    if got.len() != r || got.iter().any(|row| row.len() != c) {
        return Err(format!("expected {expected} ({r}×{c}), got {actual}"));
    }
    for (row, got_row) in rows.iter().zip(got) {
        for (lit, cell) in row.iter().zip(got_row) {
            if !scalar_matches(cell, lit, rounded) {
                return Err(format!("expected {expected}, got {actual}"));
            }
        }
    }
    Ok(())
}

/// A failed row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    /// `file:line`.
    pub label: String,
    /// The formula.
    pub formula: String,
    /// What went wrong.
    pub message: String,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}: {}", self.label, self.formula, self.message)
    }
}

/// The outcome of [`run`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Rows evaluated.
    pub total: usize,
    /// Rows skipped (`skip` type, or excluded by the caller's filter).
    pub skipped: usize,
    /// Failed rows.
    pub failures: Vec<Failure>,
}

impl Outcome {
    /// Rows that passed.
    #[must_use]
    pub fn passed(&self) -> usize {
        self.total - self.failures.len()
    }

    /// One line for logs.
    #[must_use]
    pub fn summary(&self) -> String {
        format!(
            "{} rows, {} passed, {} failed, {} skipped",
            self.total,
            self.passed(),
            self.failures.len(),
            self.skipped
        )
    }
}

/// Evaluates every case that `include` accepts with `eval` and compares the results.
pub fn run(
    cases: &[Case],
    mut include: impl FnMut(&Case) -> bool,
    mut eval: impl FnMut(&str) -> Result<Cell, String>,
) -> Outcome {
    let mut out = Outcome::default();
    for case in cases {
        if case.kind == Kind::Skip || !include(case) {
            out.skipped += 1;
            continue;
        }
        out.total += 1;
        let result = eval(&case.formula).and_then(|v| compare(&v, case));
        if let Err(message) = result {
            out.failures.push(Failure {
                label: case.label(),
                formula: case.formula.clone(),
                message,
            });
        }
    }
    out
}

/// A cell of the shared fixture sheet (`conformance/fixture.json`).
#[derive(Debug, Clone, PartialEq)]
pub enum FixtureValue {
    /// A formula (with its `=`).
    Formula(String),
    /// Literal text (a leading apostrophe in the JSON is removed).
    Text(String),
    /// A number.
    Number(f64),
    /// A logical.
    Bool(bool),
    /// An error literal.
    Error(String),
}

/// The shared sheet every row is evaluated next to.
#[derive(Debug, Clone, PartialEq)]
pub struct FixtureSheet {
    /// The sheet name (`Data`).
    pub sheet: String,
    /// Cells by A1 address, in file order.
    pub cells: Vec<(String, FixtureValue)>,
}

/// Reads `conformance/fixture.json`.
///
/// # Errors
///
/// I/O errors and values of unexpected shape.
pub fn load_fixture_sheet(path: &Path) -> Result<FixtureSheet, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: serde_json::Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let sheet = v["sheet"]
        .as_str()
        .ok_or("fixture.json: `sheet` must be a string")?
        .to_owned();
    let cells = v["cells"]
        .as_object()
        .ok_or("fixture.json: `cells` must be an object")?;
    let mut out = Vec::with_capacity(cells.len());
    for (addr, v) in cells {
        let value = match v {
            serde_json::Value::String(s) if s.starts_with('=') => FixtureValue::Formula(s.clone()),
            serde_json::Value::String(s) => {
                FixtureValue::Text(s.strip_prefix('\'').unwrap_or(s).to_owned())
            }
            serde_json::Value::Bool(b) => FixtureValue::Bool(*b),
            serde_json::Value::Number(n) => FixtureValue::Number(
                n.as_f64()
                    .ok_or_else(|| format!("fixture.json: {addr}: number out of range"))?,
            ),
            serde_json::Value::Object(o) => FixtureValue::Error(
                o.get("error")
                    .and_then(|c| c.as_str())
                    .ok_or_else(|| format!("fixture.json: {addr}: expected {{\"error\": code}}"))?
                    .to_owned(),
            ),
            other => return Err(format!("fixture.json: {addr}: unexpected value {other}")),
        };
        out.push((addr.clone(), value));
    }
    Ok(FixtureSheet { sheet, cells: out })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case(formula: &str, expected: &str, kind: Kind, notes: &str) -> Case {
        Case {
            file: "t.csv".into(),
            line: 2,
            formula: formula.into(),
            expected: expected.into(),
            kind,
            notes: notes.into(),
        }
    }

    #[test]
    fn reads_quoted_csv() {
        let rows = read_csv("a,\"b,c\",\"d\"\"e\"\r\nx,,y\n");
        assert_eq!(rows, [vec!["a", "b,c", "d\"e"], vec!["x", "", "y"]]);
        assert_eq!(read_csv("last,row"), [vec!["last", "row"]]);
    }

    #[test]
    fn parses_cases_and_reports_bad_rows() {
        let cases = parse_cases(
            "m.csv",
            "formula,expected,type,notes\n=1+1,2,number,ok\n\n=X(),,skip,\n",
        )
        .unwrap();
        assert_eq!(cases.len(), 2);
        assert_eq!(cases[0].label(), "m.csv:2");
        assert_eq!(cases[1].kind, Kind::Skip);
        assert!(
            parse_cases("m.csv", "f,e\n")
                .unwrap_err()
                .contains("header")
        );
        assert!(
            parse_cases("m.csv", "formula,expected,type,notes\n=1,1,int,\n")
                .unwrap_err()
                .contains("m.csv:2")
        );
    }

    #[test]
    fn compares_like_the_v1_runner() {
        let n = |v| Cell::Number(v);
        assert!(compare(&n(2.0), &case("=1+1", "2", Kind::Number, "")).is_ok());
        assert!(compare(&n(2.000_000_001), &case("=x", "2", Kind::Number, "")).is_ok());
        assert!(compare(&n(2.01), &case("=x", "2", Kind::Number, "")).is_err());
        // `rounded` uses the literal's precision.
        assert!(compare(&n(1.234_56), &case("=x", "1.2346", Kind::Number, "rounded")).is_ok());
        assert!(compare(&n(1.234_56), &case("=x", "1.2346", Kind::Number, "")).is_err());
        assert!(
            compare(
                &Cell::Text("a\"b".into()),
                &case("=x", "a\"b", Kind::Text, "")
            )
            .is_ok()
        );
        assert!(compare(&Cell::Text("1".into()), &case("=x", "1", Kind::Number, "")).is_err());
        assert!(compare(&Cell::Bool(true), &case("=x", "TRUE", Kind::Boolean, "")).is_ok());
        assert!(
            compare(
                &Cell::Error("#N/A".into()),
                &case("=x", "#N/A", Kind::Error, "")
            )
            .is_ok()
        );
        assert!(compare(&Cell::Blank, &case("=x", "", Kind::Number, "")).is_ok());
        let arr = Cell::Array(vec![
            vec![n(1.0), n(2.0)],
            vec![n(3.0), Cell::Text("x".into())],
        ]);
        assert!(compare(&arr, &case("=x", "{1,2;3,\"x\"}", Kind::Array, "")).is_ok());
        assert!(compare(&arr, &case("=x", "{1,2;3,4}", Kind::Array, "")).is_err());
        assert!(compare(&arr, &case("=x", "{1,2}", Kind::Array, "")).is_err());
        // A single cell shows the top-left value.
        assert!(compare(&arr, &case("=x", "1", Kind::Number, "")).is_ok());
        assert_eq!(arr.to_string(), "{1,2;3,\"x\"}");
    }

    #[test]
    fn runs_cases_through_an_evaluator() {
        let cases = vec![
            case("=1+1", "2", Kind::Number, ""),
            case("=BAD(", "1", Kind::Number, ""),
            case("=SKIP()", "", Kind::Skip, ""),
            case("=EXCLUDED()", "1", Kind::Number, ""),
        ];
        let out = run(
            &cases,
            |c| c.formula != "=EXCLUDED()",
            |f| match f {
                "=1+1" => Ok(Cell::Number(2.0)),
                _ => Err("parse error".into()),
            },
        );
        assert_eq!((out.total, out.skipped, out.passed()), (2, 2, 1));
        assert_eq!(out.failures[0].to_string(), "t.csv:2 =BAD(: parse error");
        assert_eq!(out.summary(), "2 rows, 1 passed, 1 failed, 2 skipped");
    }

    #[test]
    fn loads_the_repository_suite() {
        let dir = crate::repo_root().join("conformance");
        let cases = load_dir(&dir).unwrap();
        assert!(cases.len() > 3000, "{} cases", cases.len());
        let core = load_dir(&dir.join("core")).unwrap();
        assert!(core.len() >= 150);
        let excl = load_exclusions(&dir.join("oracle-exclusions.csv")).unwrap();
        assert!(excl.contains("=LN(-1)"));
        let sheet = load_fixture_sheet(&dir.join("fixture.json")).unwrap();
        assert_eq!(sheet.sheet, "Data");
        assert!(
            sheet
                .cells
                .iter()
                .any(|(a, v)| a == "A1" && *v == FixtureValue::Number(10.0))
        );
    }
}
