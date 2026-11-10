// SPDX-License-Identifier: AGPL-3.0-or-later
//! The desktop host: what the core asks of the outside world, answered natively so
//! no request reaches a webview, whichever window is open.
//!
//! - Files through `std::fs` ([`NativeHost`]: atomic writes, stamps).
//! - The Open and Save As dialogs of the window in front, with filters for its app in the
//!   interface language (`tauri-plugin-dialog`).
//! - The clipboard with its plain text and HTML flavours (`arboard`).
//! - Links through the system's handlers, for the schemes in [`EXTERNAL_SCHEMES`] only, and local
//!   files Kiln can hand over safely ([`may_open_file`]): the PDF to print, a document a link points
//!   at (`tauri-plugin-opener`).
//! - Print: the core writes a temporary PDF that the system's viewer opens and prints.
//! - A second window on a document (Presenter View's audience window).
//!
//! The session wraps it in `StoredHost` (preferences, recent files, autosave journals and versions
//! in the app data directory, file watching) and `NetHost` (sharing).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use kiln_common::ErrorKind;
use kiln_protocol::{ErrorInfo, FileFilter, HostOp, HostReply, RequestId};
use kiln_session::{Deliver, Host, NativeHost};
use tauri::{AppHandle, Manager, Runtime};

use crate::shell::Shell;

/// The URL schemes links may open with.
pub const EXTERNAL_SCHEMES: &[&str] = &["http", "https", "mailto"];

/// Whether `url` may be opened in the system's handler.
#[must_use]
pub fn may_open_url(url: &str) -> bool {
    let Some((scheme, rest)) = url.split_once(':') else {
        return false;
    };
    let scheme = scheme.to_ascii_lowercase();
    if !EXTERNAL_SCHEMES.contains(&scheme.as_str()) {
        return false;
    }
    match scheme.as_str() {
        "mailto" => !rest.trim().is_empty(),
        // A host after `//`, and no spaces or control characters anywhere.
        _ => {
            rest.starts_with("//")
                && rest.len() > 2
                && !url.chars().any(|c| c.is_whitespace() || c.is_control())
        }
    }
}

/// File types a link to a local file may open in their own application: documents, pictures,
/// media and text. Programs, scripts, installers and shortcuts never are, whatever a document says.
const SAFE_FILE_TYPES: &[&str] = &[
    "pdf", "docx", "dotx", "docm", "doc", "odt", "rtf", "txt", "md", "csv", "xlsx", "xltx", "xlsm",
    "xls", "ods", "pptx", "potx", "pptm", "ppsx", "ppt", "odp", "kwrite", "kgrid", "kdeck", "png",
    "jpg", "jpeg", "gif", "bmp", "webp", "svg", "tif", "tiff", "mp3", "wav", "ogg", "oga", "m4a",
    "flac", "mp4", "m4v", "webm", "mov", "mkv", "avi", "zip",
];

/// Whether a local file may be handed to the system to open: an existing regular file of a type in
/// the list above (the PDF print writes is one), never an executable one.
#[must_use]
pub fn may_open_file(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    if !SAFE_FILE_TYPES.contains(&ext.to_ascii_lowercase().as_str()) {
        return false;
    }
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o111 != 0 {
            return false;
        }
    }
    true
}

fn failed(kind: ErrorKind, message: impl Into<String>) -> HostReply {
    HostReply::Failed(ErrorInfo {
        kind,
        message: message.into(),
    })
}

/// The families of the Open dialog, each with the shell's message key.
const FAMILIES: &[(&str, &str, &[&str])] = &[
    ("write", "wordDocuments", &["docx", "dotx", "docm", "dotm"]),
    ("grid", "excelWorkbooks", &["xlsx", "xltx", "xlsm", "csv"]),
    (
        "deck",
        "powerpointPresentations",
        &["pptx", "potx", "pptm", "ppsx"],
    ),
    ("", "opendocumentFiles", &["odt", "ods", "odp"]),
    ("", "kilnDocuments", &["kwrite", "kgrid", "kdeck"]),
    ("write", "textFiles", &["txt", "md", "html", "htm"]),
];

/// The filters of a dialog the core asked for, as the window of `app` shows them in `lang`:
/// the core's single "every format" filter becomes that filter followed by one per family of
/// formats (the window's app first), each limited to what the core reads; other filters keep
/// their extensions and get their names from `menu.json`.
#[must_use]
pub fn dialog_filters(
    filters: &[FileFilter],
    app: &str,
    shell_string: &dyn Fn(&str) -> String,
) -> Vec<(String, Vec<String>)> {
    let named = |f: &FileFilter| -> String {
        let key = match f.name.as_str() {
            "All supported files" => "allDocuments",
            "Pictures" => "pictures",
            "PDF" => "pdf",
            other => other,
        };
        let s = shell_string(key);
        if s == key { f.name.clone() } else { s }
    };
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for f in filters {
        out.push((named(f), f.extensions.clone()));
        if f.name != "All supported files" {
            continue;
        }
        let mut families: Vec<&(&str, &str, &[&str])> = FAMILIES.iter().collect();
        families.sort_by_key(|(a, _, _)| usize::from(*a != app));
        for (_, key, exts) in families {
            let exts: Vec<String> = exts
                .iter()
                .filter(|e| f.extensions.iter().any(|x| x == *e))
                .map(|e| (*e).to_owned())
                .collect();
            if !exts.is_empty() {
                out.push((shell_string(key), exts));
            }
        }
    }
    out
}

/// A clipboard the system would not open or serve: written words for the window, the system's
/// own text for the log.
fn refused(e: &arboard::Error) -> HostReply {
    eprintln!("kiln: the clipboard failed: {e}");
    failed(
        ErrorKind::Unavailable,
        "The system did not let Kiln use the clipboard. Try again in a moment.",
    )
}

/// The clipboard, opened on first use and kept so what Kiln copies stays on it (X11 and Wayland
/// clipboards are served by the program that owns them). One for the app (managed state).
#[derive(Default)]
pub struct Clipboard(Mutex<Option<arboard::Clipboard>>);

impl Clipboard {
    fn with(&self, f: impl FnOnce(&mut arboard::Clipboard) -> HostReply) -> HostReply {
        let mut guard = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if guard.is_none() {
            match arboard::Clipboard::new() {
                Ok(c) => *guard = Some(c),
                Err(e) => return refused(&e),
            }
        }
        guard.as_mut().map_or_else(
            || failed(ErrorKind::Unavailable, "the clipboard is not available"),
            f,
        )
    }

    /// The clipboard's plain text and HTML (`HostReply::Clipboard`).
    pub fn read(&self) -> HostReply {
        self.with(|c| {
            let text = match c.get_text() {
                Ok(t) => t,
                Err(arboard::Error::ContentNotAvailable) => String::new(),
                Err(e) => return refused(&e),
            };
            let html = c.get().html().ok().filter(|h| !h.is_empty());
            HostReply::Clipboard { text, html }
        })
    }

    /// Puts text, and HTML when given, on the clipboard (`HostReply::Done`).
    pub fn write(&self, text: &str, html: Option<&str>) -> HostReply {
        self.with(|c| {
            let result = match html {
                Some(h) => c.set_html(h, Some(text)),
                None => c.set_text(text),
            };
            result.map_or_else(|e| refused(&e), |()| HostReply::Done)
        })
    }
}

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Clipboard")
    }
}

/// The desktop host.
pub struct DesktopHost<R: Runtime> {
    app: AppHandle<R>,
    files: NativeHost,
    clipboard: Arc<Clipboard>,
    deliver: Option<Deliver>,
}

impl<R: Runtime> std::fmt::Debug for DesktopHost<R> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DesktopHost").finish_non_exhaustive()
    }
}

impl<R: Runtime> DesktopHost<R> {
    /// The host of the app's windows.
    #[must_use]
    pub fn new(app: AppHandle<R>) -> Self {
        let clipboard = app
            .try_state::<Arc<Clipboard>>()
            .map_or_else(Arc::default, |c| Arc::clone(&c));
        Self {
            app,
            files: NativeHost,
            clipboard,
            deliver: None,
        }
    }

    fn shell(&self) -> Arc<Shell> {
        Arc::clone(&self.app.state::<Arc<Shell>>())
    }

    /// The window in front (the one whose dialog this is), else the first one.
    fn front(&self) -> Option<tauri::WebviewWindow<R>> {
        let windows = self.app.webview_windows();
        windows
            .values()
            .find(|w| w.is_focused().unwrap_or(false))
            .or_else(|| windows.get("main"))
            .or_else(|| windows.values().next())
            .cloned()
    }

    fn dialog(&self, id: RequestId, op: &HostOp, deliver: Deliver) {
        use tauri_plugin_dialog::DialogExt;
        let shell = self.shell();
        let window = self.front();
        let app = window
            .as_ref()
            .and_then(|w| shell.context(w.label()))
            .map_or_else(|| "home".to_owned(), |c| c.app);
        let string = |k: &str| shell.string(k);
        let mut builder = self.app.dialog().file();
        if let Some(w) = &window {
            builder = builder.set_parent(w);
        }
        let reply = move |path: Option<tauri_plugin_dialog::FilePath>| {
            let r = match path.map(tauri_plugin_dialog::FilePath::into_path) {
                None => HostReply::Cancelled,
                Some(Ok(p)) => HostReply::Path(p.to_string_lossy().into_owned()),
                Some(Err(e)) => failed(ErrorKind::Io, e.to_string()),
            };
            deliver(id, r);
        };
        match op {
            HostOp::PickOpen { filters } => {
                for (name, exts) in dialog_filters(filters, &app, &string) {
                    let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                    builder = builder.add_filter(name, &exts);
                }
                builder.pick_file(reply);
            }
            HostOp::PickSave {
                suggested_name,
                filters,
            } => {
                for (name, exts) in dialog_filters(filters, &app, &string) {
                    let exts: Vec<&str> = exts.iter().map(String::as_str).collect();
                    builder = builder.add_filter(name, &exts);
                }
                builder.set_file_name(suggested_name).save_file(reply);
            }
            _ => {}
        }
    }

    fn open_external(&self, url: &str) -> HostReply {
        use tauri_plugin_opener::OpenerExt;
        if !may_open_url(url) {
            return failed(
                ErrorKind::InvalidInput,
                "Kiln opens web pages and e-mail addresses only",
            );
        }
        self.app.opener().open_url(url, None::<&str>).map_or_else(
            |e| failed(ErrorKind::Io, e.to_string()),
            |()| HostReply::Done,
        )
    }

    fn open_file(&self, path: &Path) -> HostReply {
        use tauri_plugin_opener::OpenerExt;
        if !may_open_file(path) {
            return failed(
                ErrorKind::InvalidInput,
                "Kiln does not open this kind of file from a document",
            );
        }
        self.app
            .opener()
            .open_path(path.to_string_lossy(), None::<&str>)
            .map_or_else(
                |e| failed(ErrorKind::Io, e.to_string()),
                |()| HostReply::Done,
            )
    }

    /// Writes a PDF to the temporary folder and opens it in the system's viewer to print.
    fn print_pdf(&self, name: &str, bytes: &[u8]) -> HostReply {
        let dir = std::env::temp_dir().join("kiln-print");
        let safe: String = name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_.".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let file = dir.join(if safe.to_ascii_lowercase().ends_with(".pdf") {
            safe
        } else {
            format!("{safe}.pdf")
        });
        if let Err(e) = std::fs::create_dir_all(&dir).and_then(|()| std::fs::write(&file, bytes)) {
            return failed(ErrorKind::Io, e.to_string());
        }
        self.open_file(&file)
    }
}

impl<R: Runtime> Host for DesktopHost<R> {
    fn request(&mut self, op: &HostOp) -> Option<HostReply> {
        match op {
            HostOp::ReadFile { .. } | HostOp::WriteFile { .. } | HostOp::StatFile { .. } => {
                self.files.request(op)
            }
            HostOp::OpenExternal { url } if !may_open_url(url) => Some(self.open_external(url)),
            // `StoredHost` keeps the recent files and preferences; without app storage there are
            // none to list, and preferences live in the page.
            HostOp::Recents => Some(HostReply::Recents(Vec::new())),
            HostOp::StorePref { .. } | HostOp::LoadPref { .. } => Some(failed(
                ErrorKind::Unsupported,
                "preferences need the app storage",
            )),
            HostOp::WatchFile { .. } | HostOp::UnwatchFile { .. } => Some(failed(
                ErrorKind::Unsupported,
                "file watching needs the app storage",
            )),
            _ => None,
        }
    }

    fn request_later(&mut self, id: RequestId, op: &HostOp) -> bool {
        let Some(deliver) = self.deliver.clone() else {
            return false;
        };
        match op {
            HostOp::PickOpen { .. } | HostOp::PickSave { .. } => {
                self.dialog(id, op, deliver);
                true
            }
            HostOp::ClipboardRead | HostOp::ClipboardWrite { .. } => {
                let clipboard = Arc::clone(&self.clipboard);
                let op = op.clone();
                std::thread::spawn(move || {
                    let reply = match &op {
                        HostOp::ClipboardWrite { text, html } => {
                            clipboard.write(text, html.as_deref())
                        }
                        _ => clipboard.read(),
                    };
                    deliver(id, reply);
                });
                true
            }
            HostOp::OpenExternal { url } => {
                deliver(id, self.open_external(url));
                true
            }
            HostOp::OpenFile { path } => {
                deliver(id, self.open_file(&PathBuf::from(path)));
                true
            }
            HostOp::PrintPdf { name, bytes } => {
                deliver(id, self.print_pdf(name, bytes));
                true
            }
            HostOp::OpenWindow { route, title } => {
                let app = self.app.clone();
                let (route, title) = (route.clone(), title.clone());
                let run = self.app.run_on_main_thread(move || {
                    let reply = crate::windows::open(&app, &route, &title).map_or_else(
                        |e| failed(ErrorKind::Internal, e.to_string()),
                        |_| HostReply::Done,
                    );
                    deliver(id, reply);
                });
                if let (Err(e), Some(d)) = (run, &self.deliver) {
                    d(id, failed(ErrorKind::Internal, e.to_string()));
                }
                true
            }
            _ => false,
        }
    }

    fn attach(&mut self, deliver: Deliver) {
        self.deliver = Some(deliver);
    }

    fn now_ms(&self) -> u64 {
        self.files.now_ms()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_web_and_mail_links() {
        assert!(may_open_url("https://example.com/a?b=c"));
        assert!(may_open_url("HTTP://example.com"));
        assert!(may_open_url("mailto:zo@example.com"));
        assert!(!may_open_url("mailto:"));
        assert!(!may_open_url("https://"));
        assert!(!may_open_url("https://exa mple.com"));
        assert!(!may_open_url("file:///etc/passwd"));
        assert!(!may_open_url("javascript:alert(1)"));
        assert!(!may_open_url("smb://server/share"));
        assert!(!may_open_url("example.com"));
    }

    #[test]
    fn hands_over_documents_but_never_programs() {
        let dir = std::env::temp_dir().join(format!("kiln-desktop-host-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let pdf = dir.join("Report.PDF");
        std::fs::write(&pdf, b"%PDF-1.7").unwrap();
        assert!(may_open_file(&pdf));
        let sh = dir.join("run.sh");
        std::fs::write(&sh, b"#!/bin/sh").unwrap();
        assert!(!may_open_file(&sh));
        let exe = dir.join("setup.exe");
        std::fs::write(&exe, b"MZ").unwrap();
        assert!(!may_open_file(&exe));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let png = dir.join("trick.png");
            std::fs::write(&png, b"x").unwrap();
            std::fs::set_permissions(&png, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(!may_open_file(&png));
        }
        assert!(!may_open_file(&dir.join("missing.docx")));
        assert!(!may_open_file(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn open_dialogs_list_the_apps_formats_first_in_the_interface_language() {
        let all = FileFilter {
            name: "All supported files".into(),
            extensions: [
                "docx", "dotx", "odt", "xlsx", "csv", "pptx", "kdeck", "txt", "md",
            ]
            .into_iter()
            .map(str::to_owned)
            .collect(),
        };
        let shell = Shell::new();
        shell.set_language("de");
        let s = |k: &str| shell.string(k);
        let f = dialog_filters(std::slice::from_ref(&all), "grid", &s);
        let names: Vec<&str> = f.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            [
                "Alle Dokumente, die Kiln öffnet",
                "Excel-Arbeitsmappen",
                "Word-Dokumente",
                "PowerPoint-Präsentationen",
                "OpenDocument-Dateien",
                "Kiln-Dokumente",
                "Text und Markdown",
            ]
        );
        assert_eq!(f[1].1, ["xlsx", "csv"]);
        assert_eq!(f[3].1, ["pptx"]);
        let f = dialog_filters(&[all], "deck", &s);
        assert_eq!(f[1].0, "PowerPoint-Präsentationen");
        // Save As: the formats by their names.
        let save = [FileFilter {
            name: "Docx".into(),
            extensions: vec!["docx".into()],
        }];
        assert_eq!(
            dialog_filters(&save, "write", &s),
            [("Word-Dokument".to_owned(), vec!["docx".to_owned()])]
        );
        let pictures = [FileFilter {
            name: "Pictures".into(),
            extensions: vec!["png".into()],
        }];
        assert_eq!(dialog_filters(&pictures, "write", &s)[0].0, "Bilder");
        let other = [FileFilter {
            name: "Something new".into(),
            extensions: vec!["x".into()],
        }];
        assert_eq!(dialog_filters(&other, "write", &s)[0].0, "Something new");
    }
}
