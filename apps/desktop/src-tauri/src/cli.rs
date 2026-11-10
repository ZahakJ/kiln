// SPDX-License-Identifier: AGPL-3.0-or-later
//! The command line: `kiln [options] [file…]`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// What `kiln --help` prints.
pub const USAGE: &str = "\
Usage: kiln [options] [file...]

Opens each file in a window of its own: Word, Excel and PowerPoint documents and templates,
CSV, OpenDocument (.odt, .ods, .odp) and Kiln documents (.kwrite, .kgrid, .kdeck). With no
file, Kiln Home opens. When Kiln is already running, the files open there.

Options:
  --new-instance    run apart from a Kiln that is already running
  --startup-probe   measure the time to the first paint of Kiln Home and the memory in use,
                    print them (startup-ms=, pss-mb=) and exit
  --version         print the version and exit
  --help            print this help and exit
";

/// Command-line options understood by the shell.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Options {
    /// Measure startup, print it and exit.
    pub startup_probe: bool,
    /// Run apart from a running Kiln (tests, the startup probe).
    pub new_instance: bool,
    /// Print the version and exit.
    pub version: bool,
    /// Print the help and exit.
    pub help: bool,
    /// Files to open, as absolute paths.
    pub files: Vec<PathBuf>,
}

impl Options {
    /// Parses the process arguments (without the program name); relative paths are taken from
    /// `cwd`. Unknown options are ignored (desktop environments and macOS add their own).
    pub fn parse<I, S>(args: I, cwd: &Path) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<OsString>,
    {
        let mut options = Self::default();
        let mut only_files = false;
        for arg in args {
            let arg: OsString = arg.into();
            let text = arg.to_string_lossy();
            if !only_files && text.starts_with('-') {
                match text.as_ref() {
                    "--" => only_files = true,
                    "--startup-probe" => options.startup_probe = true,
                    "--new-instance" => options.new_instance = true,
                    "--version" | "-V" => options.version = true,
                    "--help" | "-h" => options.help = true,
                    _ => {}
                }
                continue;
            }
            // Non-UTF-8 paths are kept as they are; URIs and the rest go through `resolve_arg`.
            match arg.to_str() {
                Some(s) => {
                    if let Some(p) = crate::files::resolve_arg(s, cwd) {
                        options.files.push(p);
                    }
                }
                None => {
                    let p = PathBuf::from(&arg);
                    options
                        .files
                        .push(if p.is_absolute() { p } else { cwd.join(p) });
                }
            }
        }
        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_options_and_files() {
        let cwd = Path::new("/data/zo");
        let o = Options::parse(
            [
                "report.docx",
                "--startup-probe",
                "-psn_0_1234",
                "file:///tmp/b.xlsx",
                "--",
                "--odd-name.pptx",
            ],
            cwd,
        );
        assert!(o.startup_probe);
        assert!(!o.new_instance);
        assert_eq!(
            o.files,
            [
                PathBuf::from("/data/zo/report.docx"),
                PathBuf::from("/tmp/b.xlsx"),
                PathBuf::from("/data/zo/--odd-name.pptx"),
            ]
        );
        let o = Options::parse(["--version", "--new-instance"], cwd);
        assert!(o.version && o.new_instance && !o.help);
        assert!(Options::parse(["-h"], cwd).help);
        assert_eq!(
            Options::parse(Vec::<String>::new(), cwd),
            Options::default()
        );
    }
}
