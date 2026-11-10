// SPDX-License-Identifier: AGPL-3.0-or-later
//! What the shell knows about its windows and the interface: each window's app, document and
//! title as its page reports them (`kiln_window_context`), the interface language, and the menus
//! and messages of `menu.json` (generated from the command catalog, the keymaps and the locales by
//! `pnpm --filter @kiln/commands desktop:menu`).

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Deserialize;

/// A message in each interface language.
pub type Localized = BTreeMap<String, String>;

/// One item of a menu in `menu.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum MenuEntry {
    /// A command of the frame (`cmd:<id>`) or one of the shell's own actions (`native:<name>`).
    Item {
        /// The item's id.
        id: String,
        /// Its label in each language.
        label: Localized,
        /// The accelerator (`CmdOrCtrl+S`).
        accelerator: Option<String>,
        /// Arguments of the command.
        #[serde(default)]
        args: Option<serde_json::Value>,
    },
    /// A separator.
    Separator {
        /// Always `true`.
        separator: bool,
    },
}

/// One menu of the menu bar.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MenuSpec {
    /// Its title in each language.
    pub title: Localized,
    /// Its items.
    pub items: Vec<MenuEntry>,
}

/// `menu.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MenuFile {
    /// The interface languages, English first.
    pub locales: Vec<String>,
    /// The shell's own messages.
    pub strings: BTreeMap<String, Localized>,
    /// The menus of Kiln Home (`home`) and of each editor.
    pub menus: BTreeMap<String, Vec<MenuSpec>>,
}

impl MenuFile {
    /// The menus built into the shell.
    ///
    /// # Panics
    ///
    /// Never at run time: a unit test parses the same text.
    #[must_use]
    pub fn builtin() -> Self {
        serde_json::from_str(include_str!("../menu.json")).expect("menu.json is valid")
    }

    /// A message of the shell in `lang`, English when it has no translation.
    #[must_use]
    pub fn string(&self, key: &str, lang: &str) -> String {
        self.strings
            .get(key)
            .map(|m| localized(m, lang))
            .unwrap_or_else(|| key.to_owned())
    }
}

/// A message in `lang`, else in English.
#[must_use]
pub fn localized(m: &Localized, lang: &str) -> String {
    m.get(lang)
        .or_else(|| m.get("en"))
        .cloned()
        .unwrap_or_default()
}

/// What a window's page shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowContext {
    /// `home`, `write`, `grid`, `deck` or `present`.
    pub app: String,
    /// The document it shows.
    pub doc: Option<u64>,
    /// Whether that document has unsaved changes.
    pub dirty: bool,
}

/// The shell's state, shared by the IPC commands, the menus and the host.
#[derive(Debug)]
pub struct Shell {
    /// The menus and messages.
    pub menu: MenuFile,
    language: Mutex<String>,
    /// Whether a page has said which language the interface is in.
    language_known: std::sync::atomic::AtomicBool,
    windows: Mutex<BTreeMap<String, WindowContext>>,
    /// Windows whose page has closed their document and may now close.
    agreed: Mutex<Vec<String>>,
    /// Windows opened on an unsaved document at exit: they ask about it once the page is ready.
    ask_close: Mutex<Vec<String>>,
    next: AtomicU32,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Shell {
    /// The shell with the built-in menus, in English until a page reports its language.
    #[must_use]
    pub fn new() -> Self {
        Self {
            menu: MenuFile::builtin(),
            language: Mutex::new("en".into()),
            language_known: std::sync::atomic::AtomicBool::new(false),
            windows: Mutex::default(),
            agreed: Mutex::default(),
            ask_close: Mutex::default(),
            next: AtomicU32::new(1),
        }
    }

    /// The interface language (`en`, `de`, `ar`).
    #[must_use]
    pub fn language(&self) -> String {
        lock(&self.language).clone()
    }

    /// Sets the interface language; `true` when it changed.
    pub fn set_language(&self, lang: &str) -> bool {
        self.language_known.store(true, Ordering::Relaxed);
        let known = self.menu.locales.iter().any(|l| l == lang);
        let lang = if known { lang } else { "en" };
        let mut current = lock(&self.language);
        if *current == lang {
            return false;
        }
        lang.clone_into(&mut current);
        true
    }

    /// Whether a page has reported the interface language yet (at startup it has not).
    #[must_use]
    pub fn language_known(&self) -> bool {
        self.language_known.load(Ordering::Relaxed)
    }

    /// A message of the shell in the interface language.
    #[must_use]
    pub fn string(&self, key: &str) -> String {
        self.menu.string(key, &self.language())
    }

    /// What window `label` shows, as its page last said.
    #[must_use]
    pub fn context(&self, label: &str) -> Option<WindowContext> {
        lock(&self.windows).get(label).cloned()
    }

    /// Records what window `label` shows; returns the previous app.
    pub fn set_context(&self, label: &str, ctx: WindowContext) -> Option<String> {
        lock(&self.windows)
            .insert(label.to_owned(), ctx)
            .map(|c| c.app)
    }

    /// Forgets a closed window.
    pub fn forget(&self, label: &str) {
        lock(&self.windows).remove(label);
        lock(&self.agreed).retain(|l| l != label);
        lock(&self.ask_close).retain(|l| l != label);
    }

    /// The page of window `label` has closed its document: the window may close.
    pub fn agree_close(&self, label: &str) {
        let mut agreed = lock(&self.agreed);
        if !agreed.iter().any(|l| l == label) {
            agreed.push(label.to_owned());
        }
    }

    /// Whether window `label` may close now (once: the next request asks the page again).
    pub fn take_agreed(&self, label: &str) -> bool {
        let mut agreed = lock(&self.agreed);
        let was = agreed.iter().any(|l| l == label);
        agreed.retain(|l| l != label);
        was
    }

    /// Window `label` asks about its document's changes as soon as its page shows it.
    pub fn ask_close_when_ready(&self, label: &str) {
        let mut ask = lock(&self.ask_close);
        if !ask.iter().any(|l| l == label) {
            ask.push(label.to_owned());
        }
    }

    /// Whether window `label`, whose page now shows a document, was to ask about it (once).
    pub fn take_ask_close(&self, label: &str) -> bool {
        let mut ask = lock(&self.ask_close);
        let was = ask.iter().any(|l| l == label);
        ask.retain(|l| l != label);
        was
    }

    /// A new window label with the given prefix (`doc-3`, `show-4`).
    #[must_use]
    pub fn next_label(&self, prefix: &str) -> String {
        format!("{prefix}-{}", self.next.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for Shell {
    fn default() -> Self {
        Self::new()
    }
}

/// The app a route shows: `#/write?doc=3` → `write`, `#/present…` → `present`, else `home`.
#[must_use]
pub fn route_app(route: &str) -> &str {
    let path = route.trim_start_matches('#').trim_start_matches('/');
    let name = path.split(['?', '/']).next().unwrap_or("");
    match name {
        "write" | "grid" | "deck" | "present" => name,
        _ => "home",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_generated_menus_in_every_language() {
        let m = MenuFile::builtin();
        assert_eq!(m.locales.first().map(String::as_str), Some("en"));
        for app in ["home", "write", "grid", "deck"] {
            let menus = &m.menus[app];
            assert!(menus.len() >= 4, "{app}");
            assert_eq!(localized(&menus[0].title, "de"), "Datei");
            assert_eq!(localized(&menus[0].title, "ar"), "ملف");
        }
        assert_eq!(m.string("quit", "de"), "Kiln beenden");
        assert_eq!(m.string("quit", "xx"), "Quit Kiln");
        assert_eq!(m.string("no-such-key", "en"), "no-such-key");
        let save = m.menus["write"][0].items.iter().find_map(|e| match e {
            MenuEntry::Item {
                id, accelerator, ..
            } if id == "cmd:suite.file.save" => accelerator.clone(),
            _ => None,
        });
        assert_eq!(save.as_deref(), Some("CmdOrCtrl+S"));
    }

    #[test]
    fn keeps_each_windows_context_and_language() {
        let shell = Shell::new();
        assert_eq!(shell.language(), "en");
        assert!(!shell.language_known());
        assert!(shell.set_language("ar"));
        assert!(shell.language_known());
        assert!(!shell.set_language("ar"));
        assert_eq!(shell.string("help"), "تعليمات");
        assert!(shell.set_language("tlh"));
        assert_eq!(shell.language(), "en");
        let ctx = WindowContext {
            app: "write".into(),
            doc: Some(3),
            dirty: true,
        };
        assert_eq!(shell.set_context("doc-1", ctx.clone()), None);
        assert_eq!(shell.context("doc-1"), Some(ctx));
        assert!(!shell.take_ask_close("doc-1"));
        shell.ask_close_when_ready("doc-1");
        assert!(shell.take_ask_close("doc-1"));
        assert!(!shell.take_ask_close("doc-1"));
        assert!(!shell.take_agreed("doc-1"));
        shell.agree_close("doc-1");
        shell.agree_close("doc-1");
        assert!(shell.take_agreed("doc-1"));
        assert!(!shell.take_ask_close("doc-1"));
        shell.ask_close_when_ready("doc-1");
        assert!(shell.take_ask_close("doc-1"));
        assert!(!shell.take_ask_close("doc-1"));
        assert!(!shell.take_agreed("doc-1"));
        shell.forget("doc-1");
        assert_eq!(shell.context("doc-1"), None);
        assert_ne!(shell.next_label("doc"), shell.next_label("doc"));
    }

    #[test]
    fn tells_the_app_of_a_route() {
        assert_eq!(route_app("#/write?doc=3"), "write");
        assert_eq!(route_app("#/present?doc=3&view=audience"), "present");
        assert_eq!(route_app("#/home"), "home");
        assert_eq!(route_app(""), "home");
        assert_eq!(route_app("#/frame"), "home");
    }
}
