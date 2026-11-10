// SPDX-License-Identifier: AGPL-3.0-or-later
//! The native application menu (desktop only), built at startup and on every change of a
//! window's app or of the interface language from `menu.json` (the command catalog, the keymaps
//! and the translations, see [`crate::shell`]).
//!
//! Items with a `cmd:` id run the command in the frame of the window they belong to (the page
//! hears `kiln://menu`); `native:` items are the shell's own. On Linux and Windows each window has
//! the menu bar of its app; macOS has one menu bar, which follows the window in front and starts
//! with the application menu. In Arabic the GTK menu bar runs right to left.

use tauri::menu::{AboutMetadataBuilder, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

use crate::shell::{MenuEntry, Shell, localized};

/// The event a window's page hears when one of its menu's commands is picked.
pub const MENU_EVENT: &str = "kiln://menu";

/// What `kiln://menu` carries.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct MenuCommand {
    /// The command id (`suite.file.save`).
    pub command: String,
    /// Its arguments.
    pub args: Option<serde_json::Value>,
}

/// The menu context of an app (`present` windows have no menu).
#[must_use]
pub fn context(app: &str) -> Option<&'static str> {
    match app {
        "write" => Some("write"),
        "grid" => Some("grid"),
        "deck" => Some("deck"),
        "present" => None,
        _ => Some("home"),
    }
}

/// Builds the menu bar of `ctx` (`home`, `write`, `grid`, `deck`) in `lang`.
///
/// # Errors
///
/// When the platform refuses a menu item.
pub fn build<R: Runtime>(
    app: &AppHandle<R>,
    shell: &Shell,
    ctx: &str,
    lang: &str,
) -> tauri::Result<Menu<R>> {
    let menu = Menu::new(app)?;
    #[cfg(target_os = "macos")]
    menu.append(&app_menu(app, shell, lang)?)?;
    for spec in shell.menu.menus.get(ctx).into_iter().flatten() {
        let submenu = Submenu::new(app, localized(&spec.title, lang), true)?;
        for entry in &spec.items {
            match entry {
                MenuEntry::Separator { .. } => {
                    submenu.append(&PredefinedMenuItem::separator(app)?)?
                }
                MenuEntry::Item {
                    id,
                    label,
                    accelerator,
                    ..
                } => {
                    // macOS has About and Quit in the application menu.
                    if cfg!(target_os = "macos")
                        && matches!(id.as_str(), "native:about" | "native:quit")
                    {
                        continue;
                    }
                    let item = MenuItem::with_id(
                        app,
                        id.as_str(),
                        localized(label, lang),
                        true,
                        accelerator.as_deref(),
                    )?;
                    submenu.append(&item as &dyn IsMenuItem<R>)?;
                }
            }
        }
        menu.append(&submenu)?;
    }
    Ok(menu)
}

/// macOS: the application menu (About, Services, Hide, Quit).
#[cfg(target_os = "macos")]
fn app_menu<R: Runtime>(
    app: &AppHandle<R>,
    shell: &Shell,
    lang: &str,
) -> tauri::Result<Submenu<R>> {
    let s = |k: &str| shell.menu.string(k, lang);
    let about = about_metadata(app);
    Submenu::with_items(
        app,
        "Kiln",
        true,
        &[
            &PredefinedMenuItem::about(app, Some(&s("about")), Some(about))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, Some(&s("services")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, Some(&s("hide")))?,
            &PredefinedMenuItem::hide_others(app, Some(&s("hideOthers")))?,
            &PredefinedMenuItem::show_all(app, Some(&s("showAll")))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, Some(&s("quit")))?,
        ],
    )
}

/// What About Kiln shows.
pub fn about_metadata<R: Runtime>(app: &AppHandle<R>) -> tauri::menu::AboutMetadata<'_> {
    let info = app.package_info();
    AboutMetadataBuilder::new()
        .name(Some("Kiln"))
        .version(Some(info.version.to_string()))
        .comments(Some("Write, Grid and Deck. Where work gets made."))
        .copyright(Some("Copyright the Kiln contributors"))
        .license(Some("AGPL-3.0-or-later"))
        .website(Some("https://github.com/ZahakJ/kiln"))
        .website_label(Some("github.com/ZahakJ/kiln"))
        .authors(Some(vec!["The Kiln contributors".into()]))
        .icon(app.default_window_icon().cloned())
        .build()
}

/// Gives `window` the menu bar of its app in the interface language (Linux and Windows), or makes
/// it the application's menu bar while the window is in front (macOS). Presentation windows have
/// none.
pub fn apply<R: Runtime>(window: &WebviewWindow<R>, shell: &Shell, app_name: &str) {
    let lang = shell.language();
    let app = window.app_handle();
    let Some(ctx) = context(app_name) else {
        #[cfg(not(target_os = "macos"))]
        let _ = window.remove_menu();
        return;
    };
    let Ok(menu) = build(app, shell, ctx, &lang) else {
        return;
    };
    #[cfg(target_os = "macos")]
    {
        if window.is_focused().unwrap_or(true) {
            let _ = app.set_menu(menu);
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_menu(menu);
        #[cfg(target_os = "linux")]
        linux::mirror(window, lang == "ar");
    }
}

/// Runs a picked menu item: a command goes to the page of the window in front, the shell's own
/// actions run here.
pub fn on_event<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let windows = app.webview_windows();
    let window = windows
        .values()
        .find(|w| w.is_focused().unwrap_or(false))
        .or_else(|| windows.get(crate::windows::MAIN))
        .or_else(|| windows.values().next())
        .cloned();
    if let Some(command) = id.strip_prefix("cmd:") {
        let Some(w) = window else { return };
        let shell = app.state::<std::sync::Arc<Shell>>();
        let ctx = shell.context(w.label()).map(|c| c.app).unwrap_or_default();
        let args = context(&ctx).and_then(|c| item_args(&shell, c, id));
        let _ = app.emit_to(
            w.label(),
            MENU_EVENT,
            MenuCommand {
                command: command.to_owned(),
                args,
            },
        );
        return;
    }
    let Some(action) = id.strip_prefix("native:") else {
        return;
    };
    match action {
        "new-window" => {
            let _ = crate::windows::open(app, "#/home", "Kiln");
        }
        "new-write" | "new-grid" | "new-deck" => {
            if let Some(w) = window {
                let route = format!("#/{}?new={}", &action[4..], crate::windows::nonce());
                crate::windows::navigate(&w, &route);
            }
        }
        "close-window" => {
            if let Some(w) = window {
                let _ = w.close();
            }
        }
        "minimize" => {
            if let Some(w) = window {
                let _ = w.minimize();
            }
        }
        "fullscreen" => {
            if let Some(w) = window {
                let full = w.is_fullscreen().unwrap_or(false);
                let _ = w.set_fullscreen(!full);
            }
        }
        "quit" => crate::windows::quit(app),
        "about" => about(app, window.as_ref()),
        "manual" => open_link(
            app,
            "https://github.com/ZahakJ/kiln/blob/main/docs/manual/README.md",
        ),
        "report" => open_link(app, "https://github.com/ZahakJ/kiln/issues/new/choose"),
        _ => {}
    }
}

fn item_args(shell: &Shell, ctx: &str, id: &str) -> Option<serde_json::Value> {
    shell
        .menu
        .menus
        .get(ctx)?
        .iter()
        .flat_map(|m| &m.items)
        .find_map(|e| match e {
            MenuEntry::Item { id: i, args, .. } if i == id => args.clone(),
            _ => None,
        })
}

fn open_link<R: Runtime>(app: &AppHandle<R>, url: &str) {
    use tauri_plugin_opener::OpenerExt;
    let _ = app.opener().open_url(url, None::<&str>);
}

/// About Kiln on Linux and Windows (macOS has the system's panel in the application menu).
fn about<R: Runtime>(app: &AppHandle<R>, window: Option<&WebviewWindow<R>>) {
    #[cfg(target_os = "linux")]
    if let Some(w) = window {
        linux::about(w, &about_metadata(app));
        return;
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri_plugin_dialog::DialogExt;
        let info = app.package_info();
        let mut dialog = app
            .dialog()
            .message(format!(
                "Kiln {}\nWrite, Grid and Deck. Where work gets made.\n\nCopyright the Kiln contributors.\nAGPL-3.0-or-later. github.com/ZahakJ/kiln",
                info.version
            ))
            .title("Kiln");
        if let Some(w) = window {
            dialog = dialog.parent(w);
        }
        dialog.show(|_| ());
    }
    let _ = (app, window);
}

#[cfg(target_os = "linux")]
mod linux {
    use gtk::prelude::*;
    use tauri::{Runtime, WebviewWindow};

    /// F10 belongs to Key Tips (as in Office), not to GTK's "open the menu bar".
    pub fn release_f10() {
        if let Some(settings) = gtk::Settings::default() {
            settings.set_property("gtk-menu-bar-accel", "");
        }
    }

    fn set_direction(widget: &gtk::Widget, dir: gtk::TextDirection) {
        widget.set_direction(dir);
        if let Some(item) = widget.downcast_ref::<gtk::MenuItem>()
            && let Some(sub) = item.submenu()
        {
            set_direction(&sub, dir);
        }
        if let Some(container) = widget.downcast_ref::<gtk::Container>() {
            for child in container.children() {
                set_direction(&child, dir);
            }
        }
    }

    /// Runs the window's menu bar (and its menus) right to left for Arabic.
    pub fn mirror<R: Runtime>(window: &WebviewWindow<R>, rtl: bool) {
        let dir = if rtl {
            gtk::TextDirection::Rtl
        } else {
            gtk::TextDirection::Ltr
        };
        let Ok(vbox) = window.default_vbox() else {
            return;
        };
        for child in vbox.children() {
            if child.is::<gtk::MenuBar>() {
                set_direction(&child, dir);
            }
        }
    }

    /// GTK's About dialog, with the Kiln icon.
    pub fn about<R: Runtime>(window: &WebviewWindow<R>, meta: &tauri::menu::AboutMetadata<'_>) {
        let Ok(parent) = window.gtk_window() else {
            return;
        };
        let dialog = gtk::AboutDialog::new();
        dialog.set_transient_for(Some(&parent));
        dialog.set_modal(true);
        dialog.set_program_name(meta.name.as_deref().unwrap_or("Kiln"));
        if let Some(v) = &meta.version {
            dialog.set_version(Some(v));
        }
        if let Some(c) = &meta.comments {
            dialog.set_comments(Some(c));
        }
        if let Some(c) = &meta.copyright {
            dialog.set_copyright(Some(c));
        }
        if let Some(w) = &meta.website {
            dialog.set_website(Some(w));
        }
        if let Some(l) = &meta.website_label {
            dialog.set_website_label(Some(l));
        }
        dialog.set_license_type(gtk::License::Agpl30);
        dialog.set_logo_icon_name(Some("kiln"));
        if let Some(icon) = &meta.icon {
            let bytes = gtk::glib::Bytes::from(icon.rgba());
            let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_bytes(
                &bytes,
                gtk::gdk_pixbuf::Colorspace::Rgb,
                true,
                8,
                i32::try_from(icon.width()).unwrap_or(0),
                i32::try_from(icon.height()).unwrap_or(0),
                i32::try_from(icon.width() * 4).unwrap_or(0),
            );
            let scaled = pixbuf.scale_simple(128, 128, gtk::gdk_pixbuf::InterpType::Bilinear);
            dialog.set_logo(scaled.as_ref().or(Some(&pixbuf)));
        }
        dialog.connect_response(|d, _| d.close());
        dialog.show_all();
    }
}

/// Process-wide menu settings (Linux: F10 for Key Tips).
pub fn init() {
    #[cfg(target_os = "linux")]
    linux::release_f10();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_window_but_the_show_has_a_menu_bar() {
        assert_eq!(context("write"), Some("write"));
        assert_eq!(context("grid"), Some("grid"));
        assert_eq!(context("deck"), Some("deck"));
        assert_eq!(context("home"), Some("home"));
        assert_eq!(context(""), Some("home"));
        assert_eq!(context("present"), None);
    }

    #[test]
    fn menu_items_carry_the_arguments_of_their_binding() {
        let shell = Shell::new();
        // Every command item of every menu bar is in the generated file once per menu bar.
        for ctx in ["home", "write", "grid", "deck"] {
            let mut ids: Vec<&str> = shell.menu.menus[ctx]
                .iter()
                .flat_map(|m| &m.items)
                .filter_map(|e| match e {
                    MenuEntry::Item { id, .. } => Some(id.as_str()),
                    MenuEntry::Separator { .. } => None,
                })
                .collect();
            let n = ids.len();
            ids.sort_unstable();
            ids.dedup();
            assert_eq!(ids.len(), n, "{ctx}: an id twice");
        }
        assert_eq!(item_args(&shell, "write", "cmd:suite.file.save"), None);
    }
}
