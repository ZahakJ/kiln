// SPDX-License-Identifier: AGPL-3.0-or-later
//! Reads a workbook and writes it back: with its source (the default, a faithful save) or as a
//! new package (`--new`), printing the reader's warnings and the writer's report.
//!
//! ```text
//! cargo run -p kiln-io-xlsx --example rewrite in.xlsx out.xlsx [--new]
//! ```

use kiln_io_xlsx::{WriteOptions, read_xlsx, write_xlsx};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let new = args.iter().any(|a| a == "--new");
    let files: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    let [input, output] = files.as_slice() else {
        eprintln!("usage: rewrite <in.xlsx> <out.xlsx> [--new]");
        std::process::exit(2);
    };
    let run = || -> Result<(), String> {
        let bytes = std::fs::read(input).map_err(|e| format!("{input}: {e}"))?;
        let read = read_xlsx(&bytes).map_err(|e| format!("{input}: {e}"))?;
        for w in &read.warnings {
            println!("warning: {w}");
        }
        let written = write_xlsx(
            &read.workbook,
            (!new).then_some(&read.source),
            &WriteOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        for i in &written.issues {
            println!("issue: {i}");
        }
        println!("regenerated: {}", written.regenerated.join(", "));
        println!("dropped: {}", written.dropped.join(", "));
        std::fs::write(output, &written.bytes).map_err(|e| format!("{output}: {e}"))?;
        println!("wrote {output} ({} bytes)", written.bytes.len());
        Ok(())
    };
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
