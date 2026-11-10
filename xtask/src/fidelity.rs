// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask fidelity [--ci] [--format f]...`: the round-trip fidelity report.
//!
//! Runs `kiln-cli fidelity`, which scores every registered format adapter over the fixture corpus
//! with the thresholds of `parity/corpus.json`, runs the OPC passthrough baseline over every
//! package, writes `docs/qa/fidelity.md` and exits 1 when a file misses a threshold. With no
//! adapter registered yet, `--ci` reports "no formats registered" and succeeds. Further options
//! (`--no-oracle`, `--large`, `--jobs`) are passed through.
//!
//! `--differential` scores LibreOffice's round trips next to Kiln's and writes the report's
//! side-by-side section; a plain run keeps that section.

use crate::util;

/// Runs the command.
///
/// # Errors
///
/// Unknown options, a failed build, or a failed fidelity run.
pub fn run(args: &[String]) -> Result<(), String> {
    let mut pass = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--ci" | "--check" => pass.push("--ci".to_owned()),
            "--no-oracle" | "--large" | "--differential" => pass.push(a.clone()),
            "--jobs" | "--only" => {
                let v = it.next().ok_or("--jobs and --only need a value")?;
                pass.push(a.clone());
                pass.push(v.clone());
            }
            "--format" => {
                let f = it.next().ok_or("--format needs docx, xlsx or pptx")?;
                pass.push("--format".into());
                pass.push(f.clone());
            }
            other => {
                return Err(format!(
                    "unknown option '{other}' (usage: fidelity [--ci] [--format docx|xlsx|pptx|odf]... [--no-oracle] [--large] [--jobs n] [--differential])"
                ));
            }
        }
    }
    let mut cmd = util::args(&["run", "-q", "-p", "kiln-cli", "--", "fidelity"]);
    cmd.extend(pass);
    cmd.extend(util::args(&["--report", "docs/qa/fidelity.md"]));
    util::run(&util::cargo(), &cmd)
}

#[cfg(test)]
mod tests {
    #[test]
    fn rejects_unknown_options() {
        assert!(
            super::run(&["--fast".to_owned()])
                .unwrap_err()
                .contains("unknown option")
        );
        assert!(super::run(&["--format".to_owned()]).is_err());
    }
}
