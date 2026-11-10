// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli eval '=formula'`: evaluates one formula with `kiln-formula` on an empty workbook and
//! prints the value as a cell shows it (arrays as `{1,2;3,4}`). A formula error such as `#DIV/0!`
//! is a value and exits 0; a formula that does not parse exits 1.

use kiln_formula::values::{Value, display_text};
use kiln_formula::{Engine, ErrorValue};

use crate::CliError;

/// Usage text.
pub const USAGE: &str = "kiln-cli eval '=formula'";

fn scalar(v: &Value, quote_text: bool) -> String {
    match v {
        Value::Str(s) if quote_text => format!("\"{}\"", s.replace('"', "\"\"")),
        Value::Blank if quote_text => String::new(),
        Value::Lambda(_) => ErrorValue::Calc.code().to_owned(),
        other => display_text(other),
    }
}

/// How a value prints.
#[must_use]
pub fn show(v: &Value) -> String {
    match v {
        Value::Array(a) => {
            let rows: Vec<String> = (0..a.rows())
                .map(|r| {
                    (0..a.cols())
                        .map(|c| scalar(a.get(r, c), true))
                        .collect::<Vec<_>>()
                        .join(",")
                })
                .collect();
            format!("{{{}}}", rows.join(";"))
        }
        other => scalar(other, false),
    }
}

/// Runs the subcommand.
///
/// # Errors
///
/// Usage errors and formulas that do not parse.
pub fn run(args: &[String]) -> Result<String, CliError> {
    let [formula] = args else {
        return Err(CliError::usage(
            "eval takes exactly one formula (quote it for the shell)",
            USAGE,
        ));
    };
    let text = if formula.trim_start().starts_with('=') {
        formula.clone()
    } else {
        format!("={formula}")
    };
    let mut engine = Engine::new();
    engine
        .add_sheet("Sheet1")
        .map_err(|e| CliError::Failed(e.to_string()))?;
    let value = engine
        .evaluate(&text)
        .map_err(|e| CliError::Failed(format!("{text}: {e}")))?;
    Ok(show(&value))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eval(f: &str) -> Result<String, CliError> {
        run(&[f.to_owned()])
    }

    #[test]
    fn evaluates_formulas() {
        assert_eq!(eval("=1+2").unwrap(), "3");
        assert_eq!(eval("SUM(1,2,3)/4").unwrap(), "1.5");
        assert_eq!(eval("=\"a\"&\"b\"").unwrap(), "ab");
        assert_eq!(eval("=IF(1>2,\"x\",TRUE)").unwrap(), "TRUE");
        assert_eq!(eval("=1/0").unwrap(), "#DIV/0!");
        assert_eq!(eval("={1,2;3,\"x\"}").unwrap(), "{1,2;3,\"x\"}");
        assert_eq!(eval("=0.1+0.2").unwrap(), "0.3");
    }

    #[test]
    fn reports_bad_formulas_and_usage() {
        assert!(matches!(eval("=1+"), Err(CliError::Failed(_))));
        assert!(matches!(run(&[]), Err(CliError::Usage(_))));
        assert!(matches!(
            run(&["=1".into(), "=2".into()]),
            Err(CliError::Usage(_))
        ));
    }
}
