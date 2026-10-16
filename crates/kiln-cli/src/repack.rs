// SPDX-License-Identifier: AGPL-3.0-or-later
//! `kiln-cli repack <in> <out>`: opens an Office package and saves it without edits through
//! `kiln-ooxml`'s passthrough, then reports how many parts came out byte-identical. Exits 1 when a
//! part changed, so it doubles as a check that a file survives Kiln untouched.

use std::fmt::Write as _;
use std::path::PathBuf;

use kiln_testing::fidelity::package;

use crate::CliError;

/// Usage text.
pub const USAGE: &str = "kiln-cli repack <in> <out>";

/// Runs the subcommand.
///
/// # Errors
///
/// Usage errors, unreadable input, unwritable output, and parts that did not survive.
pub fn run(args: &[String]) -> Result<String, CliError> {
    let (input, output) = match args {
        [i, o] if !i.starts_with('-') && !o.starts_with('-') => {
            (PathBuf::from(i), PathBuf::from(o))
        }
        _ => {
            return Err(CliError::usage(
                "repack takes an input and an output file",
                USAGE,
            ));
        }
    };
    let bytes =
        std::fs::read(&input).map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    let r = package::repack(&bytes)
        .map_err(|e| CliError::Failed(format!("{}: {e}", input.display())))?;
    std::fs::write(&output, &r.bytes)
        .map_err(|e| CliError::Failed(format!("{}: {e}", output.display())))?;
    let c = &r.comparison;
    let mut out = format!(
        "{} -> {} ({} -> {} bytes, {:.1} ms)\nidentical parts: {}/{}",
        input.display(),
        output.display(),
        bytes.len(),
        r.bytes.len(),
        r.elapsed.as_secs_f64() * 1000.0,
        c.identical,
        c.parts
    );
    if r.zip_identical {
        out.push_str("\nthe output file is byte-identical to the input");
    }
    for (what, list) in [
        ("changed", &c.differing),
        ("missing", &c.missing),
        ("added", &c.added),
    ] {
        for p in list {
            let _ = write!(out, "\n{what}: {p}");
        }
    }
    if c.all_identical() {
        Ok(out)
    } else {
        Err(CliError::Failed(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repacks_a_fixture_with_every_part_identical() {
        let input = kiln_testing::fixtures::fixtures_dir().join("xlsx/features/styles.xlsx");
        let out_path =
            std::env::temp_dir().join(format!("kiln-cli-repack-{}.xlsx", std::process::id()));
        let text = run(&[
            input.to_string_lossy().into_owned(),
            out_path.to_string_lossy().into_owned(),
        ])
        .unwrap();
        let line = text.lines().nth(1).unwrap();
        let (n, total) = line
            .strip_prefix("identical parts: ")
            .unwrap()
            .split_once('/')
            .unwrap();
        assert_eq!(n, total);
        assert!(n.parse::<usize>().unwrap() > 5);
        assert!(out_path.is_file());
        let _ = std::fs::remove_file(&out_path);
    }

    #[test]
    fn rejects_bad_arguments_and_files() {
        assert!(matches!(run(&[]), Err(CliError::Usage(_))));
        assert!(matches!(
            run(&["a".into(), "--b".into()]),
            Err(CliError::Usage(_))
        ));
        assert!(matches!(
            run(&["/nonexistent.xlsx".into(), "/tmp/x.xlsx".into()]),
            Err(CliError::Failed(_))
        ));
    }
}
