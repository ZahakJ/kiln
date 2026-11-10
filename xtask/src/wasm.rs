// SPDX-License-Identifier: AGPL-3.0-or-later
//! `cargo xtask wasm [--release] [--size] [--app <app>]`: builds the web cores (budget B12).
//!
//! The web runs one WebAssembly module per app ([`APPS`]: Write, Grid, Deck) and loads only the
//! one the open document needs, so B12 holds per module. For each app:
//!
//! 1. `cargo build --target wasm32-unknown-unknown -p kiln-bridge-wasm --no-default-features
//!    --features <app>` (`--profile wasm-release` with `--release`, the dev profile otherwise);
//! 2. `wasm-bindgen --target web` into `apps/web/src/wasm/<app>/` (gitignored), which the Worker
//!    of that app loads (the web build content-hashes the module);
//! 3. with `--release`, `wasm-opt -Os` in place;
//! 4. prints `wasm gzip: <n> KB (<app>)` and, with `--release`, fails when a module reaches
//!    [`BUDGET_KB`] (B12).
//!
//! the collaboration module, `kiln-collab-wasm`, is built the same way into
//! `apps/web/src/wasm/collab/`; the page loads it only when a document is shared or joined, so it
//! has a budget of its own ([`COLLAB_BUDGET_KB`]) and B12 stays the cores'.
//!
//! the threads artefact. Grid's recalculation evaluates the independent formulas of each
//! topological level on a rayon pool (`kiln-formula`'s `parallel`); on the web that pool is a set
//! of Workers sharing the module's memory (wasm-bindgen-rayon), which needs a module built with
//! atomics and a cross-origin isolated page. [`THREADS_APPS`] get a second module,
//! `kiln-bridge-wasm --features <app>,threads`, built with `+atomics,+bulk-memory` and a shared,
//! imported memory into `apps/web/src/wasm/<app>-threads/`, and the Worker picks it when
//! `crossOriginIsolated` is true. The standard library has to be rebuilt with atomics
//! (`-Z build-std`), so this build sets `RUSTC_BOOTSTRAP=1` on the pinned stable toolchain (with
//! its `rust-src` component) and uses its own target directory (`target/wasm-threads`), so the
//! two builds do not invalidate each other. Write and Deck run no rayon work on the web (PDF
//! export's image encoding is the only other rayon user, and it stays sequential there), so they
//! have one module each. The threads module has the same budget B12.
//!
//! `--journal-check` builds check cores (`kiln-session`'s `journal-check`: each journal write is
//! replayed onto a shadow copy of the workbook and compared); release cores never have it.
//! `--out-dir <dir>` puts the modules there instead (`tools/check-site.sh` builds check cores
//! beside the release ones).
//! `--size` only measures the existing output. `--app` builds or measures one module (`collab`
//! for the collaboration module). `--ci` is accepted for symmetry with the other tasks. The `wasm-bindgen` CLI must match the
//! `wasm-bindgen` crate in `Cargo.lock` exactly. Desktop and Android keep the full native core.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::util;

/// Where the bindings go, relative to the repository root (one directory per app).
pub const OUT_DIR: &str = "apps/web/src/wasm";
/// The output name (`<app>/<name>.js`, `<app>/<name>_bg.wasm`).
pub const OUT_NAME: &str = "kiln_bridge_wasm";
/// The per-app web cores: each is `kiln-bridge-wasm` with only that app's cargo feature.
pub const APPS: [&str; 3] = ["write", "grid", "deck"];
/// The apps with a second, multi-threaded module (`<app>-threads/`): those whose core runs rayon
/// work on the web.
pub const THREADS_APPS: [&str; 1] = ["grid"];
/// The suffix of a threads module's directory.
pub const THREADS_SUFFIX: &str = "-threads";
/// Rust flags of the threads build: the target's own `getrandom` flag (`.cargo/config.toml`,
/// which `RUSTFLAGS` replaces), atomics, and a shared memory the page's Workers import.
const THREADS_RUSTFLAGS: &str = "--cfg getrandom_backend=\"wasm_js\" \
    -C target-feature=+atomics,+bulk-memory,+mutable-globals \
    -C link-arg=--shared-memory -C link-arg=--max-memory=4294967296 -C link-arg=--import-memory \
    -C link-arg=--export=__wasm_init_tls -C link-arg=--export=__tls_size \
    -C link-arg=--export=__tls_align -C link-arg=--export=__tls_base";
/// Budget B12: each gzipped module the web loads to open a document stays below
/// this many kilobytes (1 KB = 1,000 bytes). The single core reached 5,423 KB with every app in
/// it; split per app, each module is back under the budget (docs/qa/perf.md, B12).
pub const BUDGET_KB: u64 = 4_500;
/// The collaboration module: `kiln-collab-wasm`, bound as `collab/kiln_collab_wasm.js`.
pub const COLLAB: &str = "collab";
/// Its output name.
pub const COLLAB_OUT_NAME: &str = "kiln_collab_wasm";
/// Its budget: the gzipped module stays below this many kilobytes (docs/adr/0004-crdt.md).
pub const COLLAB_BUDGET_KB: u64 = 2_000;

/// Options of the task.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Optimised build with the size assertion.
    pub release: bool,
    /// Only measure the existing output.
    pub size_only: bool,
    /// Only this app's module (one of [`APPS`]); every app when `None`.
    pub app: Option<&'static str>,
    /// Check cores: `kiln-session`'s `journal-check` (each journal write replayed onto a shadow
    /// copy and compared), for the e2e runs that want it; the release cores never have it.
    pub journal_check: bool,
    /// Where the modules go instead of [`OUT_DIR`] (relative to the repository root): check
    /// cores are built beside the release ones, not over them.
    pub out_dir: Option<&'static str>,
}

/// Parses the options.
pub fn parse(args: &[String]) -> Result<Options, String> {
    let mut o = Options::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--release" => o.release = true,
            "--size" => o.size_only = true,
            "--journal-check" => o.journal_check = true,
            "--out-dir" => {
                let dir = it.next().ok_or("--out-dir needs a directory")?;
                o.out_dir = Some(Box::leak(dir.clone().into_boxed_str()));
            }
            "--ci" => {}
            "--app" => {
                let name = it
                    .next()
                    .ok_or("--app needs one of write, grid, deck, grid-threads, collab")?;
                let app = modules()
                    .into_iter()
                    .find(|a| *a == name.as_str())
                    .ok_or_else(|| {
                        format!("unknown app '{name}' (write, grid, deck, grid-threads or collab)")
                    })?;
                o.app = Some(app);
            }
            other => return Err(format!("unknown option '{other}'")),
        }
    }
    Ok(o)
}

/// Every module the task builds, in order: the per-app cores, the threads modules
/// (`grid-threads`) and the collaboration module.
pub fn modules() -> Vec<&'static str> {
    const THREADS_DIRS: [&str; THREADS_APPS.len()] = ["grid-threads"];
    APPS.into_iter()
        .chain(THREADS_DIRS)
        .chain([COLLAB])
        .collect()
}

/// The app of a threads module's directory (`grid-threads` → `grid`).
fn threads_app(dir: &str) -> Option<&str> {
    dir.strip_suffix(THREADS_SUFFIX)
        .filter(|app| THREADS_APPS.contains(app))
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

/// The sizes of one built module: (raw, gzip) in bytes.
fn measure(wasm: &Path) -> Result<(u64, u64), String> {
    let bytes = std::fs::read(wasm)
        .map_err(|e| format!("{}: {e} (run `cargo xtask wasm` first)", wasm.display()))?;
    Ok((bytes.len() as u64, gzip_size(&bytes)?))
}

/// Prints each module's size and, for a release build, checks it against its budget.
fn report(out_dir: &Path, apps: &[&str], release: bool) -> Result<(), String> {
    let mut over = Vec::new();
    for app in apps {
        let (raw, gz) = measure(&module_path(out_dir, app))?;
        println!("wasm raw: {} KB ({app})", kb(raw));
        println!("wasm gzip: {} KB ({app})", kb(gz));
        let (budget, what) = if *app == COLLAB {
            (COLLAB_BUDGET_KB, "the collaboration module's budget")
        } else {
            (BUDGET_KB, "budget B12")
        };
        if release && kb(gz) >= budget {
            over.push(format!(
                "{what}: the gzipped {app} module is {} KB, the limit is {budget} KB",
                kb(gz)
            ));
        }
    }
    if over.is_empty() {
        Ok(())
    } else {
        Err(format!("wasm size exceeded: {}", over.join("; ")))
    }
}

/// The bound module of an app (or of the collaboration module).
fn module_path(out_dir: &Path, app: &str) -> PathBuf {
    let name = if app == COLLAB {
        COLLAB_OUT_NAME
    } else {
        OUT_NAME
    };
    out_dir.join(app).join(format!("{name}_bg.wasm"))
}

/// Removes the files of the single core that came before the per-app modules, so neither the dev
/// server nor a reader of the directory mistakes them for current output.
fn remove_single_core(out_dir: &Path) {
    for suffix in [".js", ".d.ts", "_bg.wasm", "_bg.wasm.d.ts"] {
        let _ = std::fs::remove_file(out_dir.join(format!("{OUT_NAME}{suffix}")));
    }
}

/// Runs the task.
pub fn run(args: &[String]) -> Result<(), String> {
    let opts = parse(args)?;
    let root = util::repo_root();
    let out_dir = root.join(opts.out_dir.unwrap_or(OUT_DIR));
    let apps: Vec<&str> = opts.app.map_or_else(modules, |a| vec![a]);
    if opts.size_only {
        return report(&out_dir, &apps, opts.release);
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

    let target_dir =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let profile_dir = if opts.release {
        "wasm-release"
    } else {
        "debug"
    };
    let artefact = |name: &str| {
        target_dir
            .join("wasm32-unknown-unknown")
            .join(profile_dir)
            .join(format!("{name}.wasm"))
    };
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    remove_single_core(&out_dir);

    // One app at a time: each build overwrites the same artefact in the target directory, which
    // is bound into the app's own directory before the next build.
    for app in &apps {
        if *app == COLLAB {
            println!("xtask wasm: building the collaboration module");
            build_collab(&artefact(COLLAB_OUT_NAME), &out_dir, opts.release)?;
        } else if let Some(base) = threads_app(app) {
            println!("xtask wasm: building the {base} core with threads");
            let module = target_dir
                .join("wasm-threads")
                .join("wasm32-unknown-unknown")
                .join(profile_dir)
                .join(format!("{OUT_NAME}.wasm"));
            build_threads(&module, &target_dir, &out_dir, base, opts)?;
        } else {
            println!("xtask wasm: building the {app} core");
            build_app(&artefact(OUT_NAME), &out_dir, app, opts)?;
        }
    }
    report(&out_dir, &apps, opts.release)
}

/// The cargo features of a module: the app's, and the journal check for check cores.
fn with_check(features: &str, opts: Options) -> String {
    if opts.journal_check {
        format!("{features},journal-check")
    } else {
        features.to_owned()
    }
}

/// Builds, binds and (for a release) optimises one app's module.
fn build_app(module: &Path, out_dir: &Path, app: &str, opts: Options) -> Result<(), String> {
    let release = opts.release;
    let features = with_check(app, opts);
    let mut build = util::args(&[
        "build",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "kiln-bridge-wasm",
        "--no-default-features",
        "--features",
        &features,
    ]);
    if release {
        build.extend(util::args(&["--profile", "wasm-release"]));
    }
    util::run(&util::cargo(), &build)?;
    bind(module, out_dir, app, OUT_NAME, release)
}

/// Builds, binds and (for a release) optimises an app's threads module: atomics, a
/// shared memory and a standard library rebuilt with them, in `target/wasm-threads`.
fn build_threads(
    module: &Path,
    target_dir: &Path,
    out_dir: &Path,
    app: &str,
    opts: Options,
) -> Result<(), String> {
    let release = opts.release;
    let features = with_check(&format!("{app},threads"), opts);
    let mut build = util::args(&[
        "build",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "kiln-bridge-wasm",
        "--no-default-features",
        "--features",
        &features,
        "-Z",
        "build-std=panic_abort,std",
    ]);
    if release {
        build.extend(util::args(&["--profile", "wasm-release"]));
    }
    let rustflags = THREADS_RUSTFLAGS
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    println!("$ RUSTC_BOOTSTRAP=1 cargo {}", build.join(" "));
    let status = Command::new(util::cargo())
        .args(&build)
        .current_dir(util::repo_root())
        // `-Z build-std` on the pinned stable toolchain (it ships `rust-src`).
        .env("RUSTC_BOOTSTRAP", "1")
        .env("RUSTFLAGS", rustflags)
        .env("CARGO_TARGET_DIR", target_dir.join("wasm-threads"))
        .status()
        .map_err(|e| format!("could not start cargo: {e}"))?;
    if !status.success() {
        return Err(format!("the {app} threads build failed ({status})"));
    }
    let dir = format!("{app}{THREADS_SUFFIX}");
    bind(module, out_dir, &dir, OUT_NAME, release)?;
    // wasm-bindgen-rayon's pool Workers import the bindings as the package directory
    // (`snippets/<crate>/src/workerHelpers.js` imports `../../..`); this is that entry.
    std::fs::write(
        out_dir.join(&dir).join("index.js"),
        "// Generated by `cargo xtask wasm`: the entry wasm-bindgen-rayon's Workers import.\n\
         export * from './kiln_bridge_wasm.js';\n\
         export { default } from './kiln_bridge_wasm.js';\n",
    )
    .map_err(|e| e.to_string())
}

/// Builds, binds and (for a release) optimises the collaboration module.
fn build_collab(module: &Path, out_dir: &Path, release: bool) -> Result<(), String> {
    let mut build = util::args(&[
        "build",
        "--target",
        "wasm32-unknown-unknown",
        "-p",
        "kiln-collab-wasm",
    ]);
    if release {
        build.extend(util::args(&["--profile", "wasm-release"]));
    }
    util::run(&util::cargo(), &build)?;
    bind(module, out_dir, COLLAB, COLLAB_OUT_NAME, release)
}

/// `wasm-bindgen --target web` into `<out_dir>/<dir>/`, then `wasm-opt -Os` for a release.
fn bind(module: &Path, out_dir: &Path, dir: &str, name: &str, release: bool) -> Result<(), String> {
    let app_dir = out_dir.join(dir);
    std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
    util::run(
        "wasm-bindgen",
        &[
            "--target".to_owned(),
            "web".to_owned(),
            "--out-dir".to_owned(),
            app_dir.display().to_string(),
            "--out-name".to_owned(),
            name.to_owned(),
            module.display().to_string(),
        ],
    )?;

    if release {
        let wasm_out = module_path(out_dir, dir);
        // Rust's wasm32 target enables these proposals by default; wasm-opt must accept them.
        let mut opt = util::args(&[
            "-Os",
            // The threads modules use atomics and a shared memory.
            "--enable-threads",
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options() {
        assert_eq!(parse(&[]), Ok(Options::default()));
        let o = parse(&util::args(&["--release", "--ci"])).unwrap();
        assert!(o.release && !o.size_only && o.app.is_none());
        assert!(parse(&util::args(&["--size"])).unwrap().size_only);
        assert!(parse(&util::args(&["--debug"])).is_err());
        assert_eq!(
            parse(&util::args(&["--app", "grid"])).unwrap().app,
            Some("grid")
        );
        assert!(parse(&util::args(&["--app", "notes"])).is_err());
        assert_eq!(
            parse(&util::args(&["--app", "collab"])).unwrap().app,
            Some("collab")
        );
        assert!(parse(&util::args(&["--app"])).is_err());
        assert_eq!(
            parse(&util::args(&["--app", "grid-threads"])).unwrap().app,
            Some("grid-threads")
        );
        assert!(parse(&util::args(&["--app", "write-threads"])).is_err());
        let check = parse(&util::args(&["--release", "--journal-check"])).unwrap();
        assert!(check.journal_check && check.release);
        assert_eq!(with_check("grid", check), "grid,journal-check");
        assert_eq!(
            with_check("grid,threads", Options::default()),
            "grid,threads"
        );
        let dir = parse(&util::args(&["--out-dir", "target/wasm-check"])).unwrap();
        assert_eq!(dir.out_dir, Some("target/wasm-check"));
    }

    #[test]
    fn the_threads_modules_follow_the_cores() {
        assert_eq!(
            modules(),
            ["write", "grid", "deck", "grid-threads", "collab"]
        );
        assert_eq!(threads_app("grid-threads"), Some("grid"));
        assert_eq!(threads_app("deck-threads"), None);
        assert_eq!(threads_app("grid"), None);
    }

    #[test]
    fn finds_the_locked_wasm_bindgen_version() {
        let lock = "[[package]]\nname = \"wasm-bindgen-macro\"\nversion = \"9.9.9\"\n\n[[package]]\nname = \"wasm-bindgen\"\nversion = \"0.2.129\"\n";
        assert_eq!(locked_wasm_bindgen(lock).as_deref(), Some("0.2.129"));
        assert_eq!(locked_wasm_bindgen("[[package]]\nname = \"x\"\n"), None);
    }

    #[test]
    fn each_app_binds_into_its_own_directory() {
        let out = Path::new("out");
        assert_eq!(
            module_path(out, "deck"),
            Path::new("out/deck/kiln_bridge_wasm_bg.wasm")
        );
        assert_eq!(
            module_path(out, "collab"),
            Path::new("out/collab/kiln_collab_wasm_bg.wasm")
        );
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
