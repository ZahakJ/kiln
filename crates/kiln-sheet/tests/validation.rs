// SPDX-License-Identifier: AGPL-3.0-or-later
//! Data validation evaluation: one test per validation type, blanks, alerts, prompts, dropdown
//! lists and Circle Invalid Data.

mod support;

use kiln_sheet::commands::MetaField;
use kiln_sheet::meta::{DataValidation, ErrorStyle, Operator, ValidationType};
use kiln_sheet::validation::Validity;
use kiln_sheet::{CellPos, CellValue, Command, Range, SheetId};
use support::{Fixture, setup};

fn rule(kind: ValidationType, op: Operator, f1: &str, f2: Option<&str>) -> DataValidation {
    let mut v = DataValidation::new(1, vec![Range::new(0, 0, 9, 0)], kind);
    v.operator = op;
    v.formula1 = Some(f1.into());
    v.formula2 = f2.map(str::to_owned);
    v
}

fn with(f: &mut Fixture, s: SheetId, v: DataValidation) {
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Validations(vec![v])),
    });
}

fn ok(f: &mut Fixture, s: SheetId, v: impl Into<CellValue>) -> bool {
    f.wb.validate(s, 0, 0, &v.into()).is_valid()
}

fn n(x: f64) -> CellValue {
    CellValue::Number(x)
}

#[test]
fn whole_numbers_between_bounds() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    with(
        &mut f,
        s,
        rule(ValidationType::Whole, Operator::Between, "1", Some("10")),
    );
    assert!(ok(&mut f, s, n(5.0)));
    assert!(!ok(&mut f, s, n(5.5)), "not whole");
    assert!(!ok(&mut f, s, n(11.0)));
    assert!(!ok(&mut f, s, "five"));
    assert!(
        ok(&mut f, s, CellValue::Blank),
        "blanks are ignored by default"
    );
}

#[test]
fn decimals_with_formula_bounds() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    f.set(s, "C1", 2.5);
    with(
        &mut f,
        s,
        rule(
            ValidationType::Decimal,
            Operator::GreaterThanOrEqual,
            "$C$1",
            None,
        ),
    );
    assert!(ok(&mut f, s, n(2.5)));
    assert!(!ok(&mut f, s, n(2.4)));
    f.set(s, "C1", 1);
    assert!(ok(&mut f, s, n(2.4)), "criteria follow the referenced cell");
}

#[test]
fn lists_from_literals_and_ranges() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    with(
        &mut f,
        s,
        rule(
            ValidationType::List,
            Operator::Between,
            "\"Yes,No,Maybe\"",
            None,
        ),
    );
    assert!(ok(&mut f, s, "yes"));
    assert!(!ok(&mut f, s, "Perhaps"));
    assert_eq!(
        f.wb.validation_list(s, 3, 0),
        Some(vec![
            CellValue::text("Yes"),
            CellValue::text("No"),
            CellValue::text("Maybe")
        ])
    );
    for (i, v) in ["Red", "Green", "Blue"].iter().enumerate() {
        f.set(s, &format!("E{}", i + 1), *v);
    }
    f.set(s, "E4", 7);
    with(
        &mut f,
        s,
        rule(ValidationType::List, Operator::Between, "$E$1:$E$5", None),
    );
    assert!(ok(&mut f, s, "green"));
    assert!(ok(&mut f, s, n(7.0)));
    assert!(!ok(&mut f, s, "Purple"));
    assert_eq!(f.wb.validation_list(s, 0, 0).map(|l| l.len()), Some(4));
    assert_eq!(f.wb.validation_list(s, 0, 3), None, "no rule there");
}

#[test]
fn dates_and_times() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    with(
        &mut f,
        s,
        rule(
            ValidationType::Date,
            Operator::Between,
            "DATE(2024,1,1)",
            Some("DATE(2024,12,31)"),
        ),
    );
    assert!(ok(&mut f, s, n(45306.0)));
    assert!(!ok(&mut f, s, n(45700.0)));
    with(
        &mut f,
        s,
        rule(
            ValidationType::Date,
            Operator::GreaterThan,
            "\"1/1/2025\"",
            None,
        ),
    );
    assert!(ok(&mut f, s, n(45700.0)));
    assert!(!ok(&mut f, s, n(45306.0)));
    with(
        &mut f,
        s,
        rule(ValidationType::Time, Operator::LessThan, "\"12:00\"", None),
    );
    assert!(ok(&mut f, s, n(0.25)));
    assert!(
        !ok(&mut f, s, n(45306.75)),
        "the time part of a date-time counts"
    );
}

#[test]
fn text_length() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    with(
        &mut f,
        s,
        rule(
            ValidationType::TextLength,
            Operator::LessThanOrEqual,
            "5",
            None,
        ),
    );
    assert!(ok(&mut f, s, "hello"));
    assert!(!ok(&mut f, s, "goodbye"));
    assert!(ok(&mut f, s, n(12345.0)));
    assert!(!ok(&mut f, s, n(123_456.0)));
}

#[test]
fn custom_formulas_see_the_proposed_value() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    with(
        &mut f,
        s,
        rule(
            ValidationType::Custom,
            Operator::Between,
            "ISEVEN(A1)",
            None,
        ),
    );
    assert!(ok(&mut f, s, n(4.0)));
    assert!(!ok(&mut f, s, n(3.0)));
    // Relative to each cell of the range.
    assert!(f.wb.validate(s, 5, 0, &n(8.0)).is_valid());
    assert!(!f.wb.validate(s, 5, 0, &n(9.0)).is_valid());
    // Validation does not change the cell.
    assert!(f.val(s, "A1").is_blank());
    // Unique entries in a column.
    f.set(s, "A2", 4);
    with(
        &mut f,
        s,
        rule(
            ValidationType::Custom,
            Operator::Between,
            "COUNTIF($A$1:$A$10,A1)=1",
            None,
        ),
    );
    assert!(!ok(&mut f, s, n(4.0)));
    assert!(ok(&mut f, s, n(6.0)));
}

#[test]
fn blanks_alerts_and_prompts() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    let mut v = rule(ValidationType::Whole, Operator::GreaterThan, "0", None);
    v.allow_blank = false;
    v.error_style = ErrorStyle::Warning;
    v.error_title = Some("Positive".into());
    v.error = Some("Enter a positive whole number.".into());
    v.prompt_title = Some("Quantity".into());
    v.prompt = Some("Whole units".into());
    with(&mut f, s, v.clone());
    assert!(!ok(&mut f, s, CellValue::Blank));
    match f.wb.validate(s, 0, 0, &n(-1.0)) {
        Validity::Invalid {
            style,
            title,
            message,
        } => {
            assert_eq!(style, ErrorStyle::Warning);
            assert_eq!(title, "Positive");
            assert_eq!(message, "Enter a positive whole number.");
        }
        Validity::Valid => panic!("-1 is not positive"),
    }
    assert_eq!(
        f.wb.validation_prompt(s, 2, 0),
        Some(("Quantity".into(), "Whole units".into()))
    );
    v.show_error_message = false;
    with(&mut f, s, v);
    assert!(ok(&mut f, s, n(-1.0)), "no alert means anything goes");
}

#[test]
fn circles_invalid_data() {
    let mut f = setup(&["Sheet1"]);
    let s = f.id(0);
    for (i, x) in [1, 50, 3, 70].iter().enumerate() {
        f.set(s, &format!("A{}", i + 1), *x);
    }
    with(
        &mut f,
        s,
        rule(ValidationType::Whole, Operator::LessThan, "10", None),
    );
    assert_eq!(
        f.wb.invalid_cells(s, Range::new(0, 0, 100, 3)),
        vec![CellPos::new(1, 0), CellPos::new(3, 0)]
    );
    // The last rule listed for a cell wins.
    let mut other = DataValidation::new(2, vec![Range::cell(1, 0)], ValidationType::Any);
    other.allow_blank = true;
    let base = rule(ValidationType::Whole, Operator::LessThan, "10", None);
    f.exec(Command::SetSheetMeta {
        sheet: s,
        field: Box::new(MetaField::Validations(vec![base, other])),
    });
    assert_eq!(
        f.wb.invalid_cells(s, Range::new(0, 0, 100, 3)),
        vec![CellPos::new(3, 0)]
    );
}
