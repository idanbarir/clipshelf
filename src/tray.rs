//! Icon in the system tray (the area next to the keyboard-language indicator),
//! using the StatusNotifierItem / AppIndicator protocol supported by Zorin,
//! Ubuntu, KDE, Cinnamon, XFCE and others.

use crate::i18n::{self, Strings};
use ksni::menu::{RadioGroup, RadioItem, StandardItem, SubMenu};
use ksni::{MenuItem, ToolTip};
use std::sync::mpsc::Sender;

pub enum Cmd {
    /// Show or hide the popup; carries the Wayland activation token if any.
    Toggle(Option<String>),
    /// Where the tray icon was clicked (screen coordinates), if known.
    Anchor(i32, i32),
    ClearAll,
    Settings,
    Language(String),
    Quit,
}

pub struct ClipTray {
    pub tx: Sender<Cmd>,
    pub t: &'static Strings,
    /// Current "language" setting ("auto", "en", …).
    pub lang: String,
}

impl ClipTray {
    fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
    }
}

impl ksni::Tray for ClipTray {
    fn id(&self) -> String {
        "clipshelf".into()
    }

    fn title(&self) -> String {
        "ClipShelf".into()
    }

    fn icon_name(&self) -> String {
        "io.github.idanbarir.ClipShelf-symbolic".into()
    }

    fn icon_theme_path(&self) -> String {
        // Point the tray at the folder that holds our icon. Without this, the
        // GNOME/Zorin tray looks the name up in the current icon theme only,
        // and on Zorin it then shows a placeholder ("•••") instead.
        let home = std::env::var("HOME").map(|h| format!("{h}/.local/share/icons")).unwrap_or_default();
        let found = [home.as_str(), "/usr/local/share/icons", "/usr/share/icons"]
            .into_iter()
            .find(|d| {
                std::path::Path::new(&format!(
                    "{d}/hicolor/symbolic/apps/io.github.idanbarir.ClipShelf-symbolic.svg"
                ))
                .exists()
            })
            .map(str::to_string)
            .unwrap_or_default();
        found
    }

    fn tool_tip(&self) -> ToolTip {
        ToolTip {
            title: "ClipShelf".into(),
            description: format!("{} (Super+V)", self.t.tray_show),
            ..Default::default()
        }
    }

    /// Left click: open/close the history.
    fn activate(&mut self, x: i32, y: i32) {
        eprintln!("clipshelf: tray icon clicked at {x},{y}");
        // Diagnostics for "open above the tray icon": what the tray reports.
        let _ = std::fs::write(
            crate::history::data_dir().join("tray-last-click"),
            format!("{x} {y}\n"),
        );
        if x > 0 || y > 0 {
            self.send(Cmd::Anchor(x, y));
        }
        self.send(Cmd::Toggle(ksni::current_xdg_activation_token()));
    }

    fn menu(&self) -> Vec<MenuItem<Self>> {
        let selected = if self.lang == "auto" || self.lang.is_empty() {
            0
        } else {
            i18n::LANGS
                .iter()
                .position(|(c, _, _)| *c == self.lang)
                .map(|i| i + 1)
                .unwrap_or(0)
        };
        let mut options = vec![RadioItem {
            label: self.t.tray_auto.into(),
            ..Default::default()
        }];
        options.extend(i18n::LANGS.iter().map(|(_, name, _)| RadioItem {
            label: (*name).into(),
            ..Default::default()
        }));

        vec![
            StandardItem {
                label: format!("{}  (Super+V)", self.t.tray_show),
                icon_name: "edit-paste-symbolic".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Toggle(ksni::current_xdg_activation_token()))),
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: format!("{}…", self.t.clear_all),
                icon_name: "edit-clear-all-symbolic".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::ClearAll)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            SubMenu {
                label: self.t.tray_language.into(),
                icon_name: "preferences-desktop-locale-symbolic".into(),
                submenu: vec![RadioGroup {
                    selected,
                    select: Box::new(|t: &mut Self, idx| {
                        let code = if idx == 0 {
                            "auto".to_string()
                        } else {
                            i18n::LANGS[idx - 1].0.to_string()
                        };
                        if code != t.lang {
                            t.send(Cmd::Language(code));
                        }
                    }),
                    options,
                }
                .into()],
                ..Default::default()
            }
            .into(),
            StandardItem {
                label: format!("{}…", self.t.settings),
                icon_name: "preferences-system-symbolic".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Settings)),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: self.t.tray_quit.into(),
                icon_name: "application-exit-symbolic".into(),
                activate: Box::new(|t: &mut Self| t.send(Cmd::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
}

/// Starts the tray icon on a background thread. Failure (no tray support
/// on this desktop) is not fatal: Super+V keeps working.
pub fn spawn(tx: Sender<Cmd>, t: &'static Strings, lang: String) {
    use ksni::blocking::TrayMethods;
    // At login the panel may start after us: keep waiting for it instead of giving up.
    match (ClipTray { tx, t, lang }).assume_sni_available(true).spawn() {
        Ok(handle) => std::mem::forget(handle),
        Err(e) => eprintln!("clipshelf: no tray icon ({e})"),
    }
}
