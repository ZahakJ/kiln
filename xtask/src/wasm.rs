// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask wasm [--release] [--size]`: builds the web core (PLAN.md §3.5, budget B12).
//!
//! 1. `cargo build --target wasm32-unknown-unknown -p kiln-bridge-wasm` (`--profile wasm-release`
//!    with `--release`, the dev profile otherwise);
//! 2. `wasm-bindgen --target web` into `apps/web/src/wasm/` (gitignored), which the Worker loads;
//! 3. with `--release`, `wasm-opt -Os` in place;
//! 4. prints `wasm gzip: <n> KB` and, with `--release`, fails above 4,500 KB (B12).
//!
//! `--size` only measures the existing output. `--ci` is accepted for symmetry with the other
//! tasks. The `wasm-bindgen` CLI must match the `wasm-bindgen` crate in `Cargo.lock` exactly.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util;

/// Where the bindings go, relative to the repository root.
pub const OUT_DIR: &str = "apps/web/src/wasm";
/// The output name (`<name>.js`, `<name>_bg.wasm`).
pub const OUT_NAME: &str = "kiln_bridge_wasm";
/// Budget B12: the gzipped core must stay below this many kilobytes (1 KB = 1,000 bytes).
pub const BUDGET_KB: u64 = 4_500;

/// Options of the task.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Optimised build with the size assertion.
    pub release: bool,
    /// Only measure the existing output.
    pub size_only: bool,
}

/// Parses the options.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    for a in args {
        match a.as_str() {
            "--release" => o.release = true,
            "--size" => o.size_only = true,
            "--ci" => {}
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(o)
}

/// The `wasm-bindgen` version pinned in `Cargo.lock`.
pub fn locked_wasm_bindgen(lock: &str) -> Option<String> {
    let mut lines = lock.lines();
    while let Some(line) = lines.next() {
        if line.trim() == "name = \"wasm-bindgen\"" {
            let version = lines.next()?.trim();
            return version
                .strip_prefix("version = \"")
                .and_then(|v| v.strip_suffix('"'))
                .map(str::to_owned);
        }
    }
    None
}

/// Kilobytes (1,000 bytes), rounded up.
pub fn kb(bytes: u64) -> u64 {
    bytes.div_ceil(1_000)
}

fn tool_output(program: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new(program)
        .args(args)
        .output()
        .map_err(|e| format!("{program} is not installed ({e}); see tools/setup-rust.sh"))?;
    if !out.status.success() {
        return Err(format!("`{program} {}` failed", args.join(" ")));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The size of `bytes` after `gzip -9`.
fn gzip_size(bytes: &[u8]) -> Result<u64, String> {
    let mut child = Command::new("gzip")
        .args(["-9", "-c"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .map_err(|e| format!("gzip is not available: {e}"))?;
    let mut stdin = child.stdin.take().ok_or("gzip stdin")?;
    let data = bytes.to_vec();
    let writer = std::thread::spawn(move || stdin.write_all(&data));
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    writer
        .join()
        .map_err(|_| "gzip writer panicked".to_owned())?
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err("gzip failed".into());
    }
    Ok(out.stdout.len() as u64)
}

/// Waits while another agent runs a heavy build (PLAN.md §4.7); never in CI.
fn wait_for_heavy_builds(root: &Path) {
    let marker = root.join(".agent.heavy");
    if std::env::var_os("CI").is_some() {
        return;
    }
    let mut announced = false;
    while marker.exists() {
        if !announced {
            println!("xtask wasm: another heavy build is running (.agent.heavy); waiting…");
            announced = true;
        }
        std::thread::sleep(std::time::Duration::from_secs(30));
    }
}

fn report(wasm: &Path, release: bool) -> Result<(), String> {
    let bytes = std::fs::read(wasm)
        .map_err(|e| format!("{}: {e} (run `cargo xtask wasm` first)", wasm.display()))?;
    let gz = gzip_size(&bytes)?;
    println!("wasm raw: {} KB", kb(bytes.len() as u64));
    println!("wasm gzip: {} KB", kb(gz));
    if release && kb(gz) >= BUDGET_KB {
        return Err(format!(
            "budget B12 exceeded: the gzipped core is {} KB, the limit is {BUDGET_KB} KB",
            kb(gz)
        ));
    }
    Ok(())
}

/// Runs the task.
pub fn run(args: &[String]) -> Result<(), String> {
    let opts = parse(args)?;
    let root = util::repo_root();
    let out_dir = root.join(OUT_DIR);
    let wasm_out = out_dir.join(format!("{OUT_NAME}_bg.wasm"));
    if opts.size_only {
        return report(&wasm_out, opts.release);
    }

    // The CLI and the crate must agree, or the generated glue does not match the module.
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).map_err(|e| e.to_string())?;
    let locked = locked_wasm_bindgen(&lock).ok_or("wasm-bindgen is not in Cargo.lock")?;
    let cli = tool_output("wasm-bindgen", &["--version"])?;
    if cli.split_whitespace().nth(1) != Some(locked.as_str()) {
        return Err(format!(
            "`{cli}` does not match the wasm-bindgen crate {locked} in Cargo.lock; install it with `cargo install wasm-bindgen-cli --version {locked} --locked`"
        ));
    }

    if opts.release {
        wait_for_heavy_builds(&root);
    }
    let cargo = util::cargo();
    let mut build = util::args(&[
        "build",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "kiln-bridge-wasm",
    ]);
    if opts.release {
        build.extend(util::args(&["--profile", "wasm-release"]));
    }
    util::run(&cargo, &build)?;

    let target_dir =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let profile_dir = if opts.release {
        "wasm-release"
    } else {
        "debug"
    };
    let module = target_dir
        .join("wasm32-unknown-unknown")
        .join(profile_dir)
        .join(format!("{OUT_NAME}.wasm"));

    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    util::run(
        "wasm-bindgen",
        &[
            "--target".to_owned(),
            "web".to_owned(),
            "--out-dir".to_owned(),
            out_dir.display().to_string(),
            "--out-name".to_owned(),
            OUT_NAME.to_owned(),
            module.display().to_string(),
        ],
    )?;

    if opts.release {
        // Rust's wasm32 target enables these proposals by default; wasm-opt must accept them.
        let mut opt = util::args(&[
            "-Os",
            "--enable-bulk-memory",
            "--enable-nontrapping-float-to-int",
            "--enable-sign-ext",
            "--enable-mutable-globals",
            "--enable-reference-types",
            "--enable-multivalue",
        ]);
        opt.extend([
            wasm_out.display().to_string(),
            "-o".to_owned(),
            wasm_out.display().to_string(),
        ]);
        util::run("wasm-opt", &opt)?;
    }
    report(&wasm_out, opts.release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options() {
        assert_eq!(parse(&[]), Ok(Options::default()));
        let o = parse(&util::args(&["--release", "--ci"])).unwrap();
        assert!(o.release && !o.size_only);
        assert!(parse(&util::args(&["--size"])).unwrap().size_only);
        assert!(parse(&util::args(&["--debug"])).is_err());
    }

    #[test]
    fn finds_the_locked_wasm_bindgen_version() {
        let lock = "[[package]]\nname = \"wasm-bindgen-macro\"\nversion = \"9.9.9\"\n\n[[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.129\"\n";
        assert_eq!(locked_wasm_bindgen(lock).as_deref(), Some("0.2.129"));
        assert_eq!(locked_wasm_bindgen("[[package]]\nname = \"x\"\n"), None);
    }

    #[test]
    fn sizes_round_up_to_kilobytes_and_gzip_works() {
        assert_eq!(kb(0), 0);
        assert_eq!(kb(1), 1);
        assert_eq!(kb(4_500_000), 4_500);
        let zeros = vec![0u8; 100_000];
        let gz = gzip_size(&zeros).unwrap();
        assert!(gz > 0 && gz < 1_000);
    }
}
