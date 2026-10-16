// SPDX-License-Identifier: AGPL-3.0-or-later
//! LibreOffice headless as a test oracle (PLAN.md §4.5), through `tools/oracle-soffice.sh`.
//!
//! The script runs LibreOffice with a dedicated profile that recalculates OOXML and ODF formulas
//! on load (so `.xlsx → .csv` yields values LibreOffice computed, not cached ones) and serialises
//! conversions with `flock`. Page images come from poppler's `pdftoppm`.
//!
//! The oracle is optional: CI runners and fresh machines usually lack LibreOffice, so callers ask
//! [`Oracle::locate`] and skip oracle metrics when it reports [`Unavailable`].

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Why the oracle cannot run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable(pub String);

impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Default time a single conversion may take before it is killed.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(180);

/// A located oracle.
#[derive(Debug, Clone)]
pub struct Oracle {
    script: PathBuf,
    soffice: PathBuf,
    pdftoppm: Option<PathBuf>,
    timeout: Duration,
}

fn on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

/// The finished run of a child process.
#[derive(Debug)]
struct Finished {
    success: bool,
    stdout: String,
    stderr: String,
}

fn drain(mut r: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = r.read_to_end(&mut buf);
        String::from_utf8_lossy(&buf).into_owned()
    })
}

/// Runs a command with a deadline; the child is killed (by its handle, never by name) when the
/// deadline passes.
fn run_with_timeout(mut cmd: Command, timeout: Duration) -> Result<Finished, String> {
    let shown = format!("{cmd:?}");
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("could not start {shown}: {e}"))?;
    let out = child.stdout.take().map(drain);
    let err = child.stderr.take().map(drain);
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{shown} timed out after {} s", timeout.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(e) => return Err(format!("{shown}: {e}")),
        }
    };
    let join = |h: Option<std::thread::JoinHandle<String>>| {
        h.and_then(|h| h.join().ok()).unwrap_or_default()
    };
    Ok(Finished {
        success: status.success(),
        stdout: join(out),
        stderr: join(err),
    })
}

impl Oracle {
    /// Finds the script (`<repo>/tools/oracle-soffice.sh`), LibreOffice (`$SOFFICE`, else
    /// `/usr/bin/soffice`, else `soffice` on `PATH`) and `pdftoppm`.
    ///
    /// # Errors
    ///
    /// [`Unavailable`] when the script or LibreOffice is missing.
    pub fn locate() -> Result<Self, Unavailable> {
        Self::locate_in(&crate::repo_root())
    }

    /// [`Oracle::locate`] for another checkout.
    ///
    /// # Errors
    ///
    /// [`Unavailable`] when the script or LibreOffice is missing.
    pub fn locate_in(repo: &Path) -> Result<Self, Unavailable> {
        let script = repo.join("tools/oracle-soffice.sh");
        if !script.is_file() {
            return Err(Unavailable(format!("{} is missing", script.display())));
        }
        let soffice = std::env::var_os("SOFFICE")
            .map(PathBuf::from)
            .filter(|p| p.is_file())
            .or_else(|| Some(PathBuf::from("/usr/bin/soffice")).filter(|p| p.is_file()))
            .or_else(|| on_path("soffice"))
            .ok_or_else(|| Unavailable("LibreOffice (soffice) is not installed".into()))?;
        let timeout = std::env::var("KILN_ORACLE_TIMEOUT")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .map_or(DEFAULT_TIMEOUT, Duration::from_secs);
        Ok(Self {
            script,
            soffice,
            pdftoppm: on_path("pdftoppm"),
            timeout,
        })
    }

    /// The LibreOffice executable in use.
    #[must_use]
    pub fn soffice(&self) -> &Path {
        &self.soffice
    }

    /// Whether page images can be made (`pdftoppm` is installed).
    #[must_use]
    pub fn can_render_pages(&self) -> bool {
        self.pdftoppm.is_some()
    }

    /// Converts `input` with LibreOffice into `outdir` and returns the output path.
    ///
    /// `format` is anything `soffice --convert-to` accepts: `pdf`, `csv`, `docx`, or a filter
    /// spec such as `csv:Text - txt - csv (StarCalc):44,34,76,1`; the output extension is the
    /// part before the first `:`.
    ///
    /// # Errors
    ///
    /// When LibreOffice fails, times out or writes nothing.
    pub fn convert(&self, input: &Path, format: &str, outdir: &Path) -> Result<PathBuf, String> {
        let ext = format.split(':').next().unwrap_or(format);
        if ext.is_empty() {
            return Err("an empty conversion format".into());
        }
        let stem = input
            .file_stem()
            .ok_or_else(|| format!("{}: no file name", input.display()))?;
        std::fs::create_dir_all(outdir).map_err(|e| format!("{}: {e}", outdir.display()))?;
        let expected = outdir.join(stem).with_extension(ext);
        // A stale file from an earlier run must not pass for this conversion's output.
        let _ = std::fs::remove_file(&expected);
        let mut cmd = Command::new("bash");
        cmd.arg(&self.script)
            .arg(input)
            .arg(format)
            .arg(outdir)
            .env("SOFFICE", &self.soffice);
        let done = run_with_timeout(cmd, self.timeout)?;
        let written = std::fs::metadata(&expected).is_ok_and(|m| m.len() > 0);
        if done.success && written {
            Ok(expected)
        } else {
            Err(format!(
                "LibreOffice could not convert {} to {format}: {}{}",
                input.display(),
                done.stderr.trim(),
                done.stdout.trim()
            ))
        }
    }

    /// Whether LibreOffice opens `input` (it converts to a non-empty PDF in `scratch`).
    #[must_use]
    pub fn opens(&self, input: &Path, scratch: &Path) -> bool {
        self.convert(input, "pdf", scratch).is_ok()
    }

    /// Renders every page of a PDF to PNG files `<outdir>/<prefix>-<n>.png` at `dpi` and returns
    /// them in page order.
    ///
    /// # Errors
    ///
    /// When `pdftoppm` is missing or fails.
    pub fn render_pages(
        &self,
        pdf: &Path,
        dpi: u32,
        outdir: &Path,
        prefix: &str,
    ) -> Result<Vec<PathBuf>, String> {
        let pdftoppm = self
            .pdftoppm
            .as_ref()
            .ok_or_else(|| "pdftoppm (poppler-utils) is not installed".to_owned())?;
        std::fs::create_dir_all(outdir).map_err(|e| format!("{}: {e}", outdir.display()))?;
        let mut cmd = Command::new(pdftoppm);
        cmd.arg("-r")
            .arg(dpi.clamp(10, 600).to_string())
            .arg("-png")
            .arg(pdf)
            .arg(outdir.join(prefix));
        let done = run_with_timeout(cmd, self.timeout)?;
        if !done.success {
            return Err(format!(
                "pdftoppm failed on {}: {}",
                pdf.display(),
                done.stderr.trim()
            ));
        }
        page_files(outdir, prefix)
    }
}

/// The `<prefix>-<n>.png` files `pdftoppm` wrote (it pads `n` to the page count's width), in
/// page order.
///
/// # Errors
///
/// When the directory cannot be read.
pub fn page_files(dir: &Path, prefix: &str) -> Result<Vec<PathBuf>, String> {
    let lead = format!("{prefix}-");
    let mut pages: Vec<(u32, PathBuf)> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            let n = name.strip_prefix(&lead)?.strip_suffix(".png")?;
            Some((n.parse().ok()?, e.path()))
        })
        .collect();
    pages.sort();
    Ok(pages.into_iter().map(|(_, p)| p).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_files_are_ordered_numerically() {
        let dir = std::env::temp_dir().join(format!("kiln-oracle-pages-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for n in ["10", "02", "1"] {
            std::fs::write(dir.join(format!("p-{n}.png")), b"x").unwrap();
        }
        std::fs::write(dir.join("p-x.png"), b"x").unwrap();
        std::fs::write(dir.join("q-3.png"), b"x").unwrap();
        let pages = page_files(&dir, "p").unwrap();
        let names: Vec<_> = pages
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["p-1.png", "p-02.png", "p-10.png"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn timeouts_kill_the_child() {
        let mut cmd = Command::new("sleep");
        cmd.arg("5");
        let err = run_with_timeout(cmd, Duration::from_millis(100)).unwrap_err();
        assert!(err.contains("timed out"), "{err}");
        let ok = run_with_timeout(Command::new("true"), Duration::from_secs(5)).unwrap();
        assert!(ok.success);
    }

    #[test]
    fn a_checkout_without_the_script_is_unavailable() {
        let err = Oracle::locate_in(Path::new("/nonexistent")).unwrap_err();
        assert!(err.to_string().contains("oracle-soffice.sh"));
    }
}
