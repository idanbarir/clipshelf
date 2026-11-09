//! Per-user setup, done by the app itself the first time it runs (and again
//! whenever the program moves, e.g. from ~/.local/bin to /usr/bin):
//!   * start at login (~/.config/autostart),
//!   * the Super+V and Ctrl+Super+V keyboard shortcuts (GNOME and relatives),
//!   * cleanup of files left by the older install.sh-based installation.
//!
//! A system package (.deb) can't do these, because they live in each user's
//! own settings — so the package only installs files, and the app takes care
//! of the rest when it starts.

use gtk::gio;
use gtk::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

const SETUP_VERSION: u32 = 1;
const MK: &str = "org.gnome.settings-daemon.plugins.media-keys";
const KB_SCHEMA: &str = "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding";
const KB_BASE: &str = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings";

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

fn marker() -> PathBuf {
    crate::history::data_dir().join("setup-done")
}

pub fn run() {
    if std::env::var_os("CLIPSHELF_NO_SETUP").is_some() {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    // Never register a development build from a cargo target directory.
    if exe.components().any(|c| c.as_os_str() == "target") {
        return;
    }
    let stamp = format!("{SETUP_VERSION}:{}", exe.display());
    if fs::read_to_string(marker()).ok().as_deref().map(str::trim) == Some(stamp.as_str()) {
        return;
    }
    let exe_s = exe.to_string_lossy().into_owned();
    if exe.starts_with("/usr") {
        remove_user_install(&exe);
    }
    autostart(&exe_s);
    shortcuts(&exe_s);
    if let Some(d) = marker().parent() {
        let _ = fs::create_dir_all(d);
    }
    let _ = fs::write(marker(), stamp + "\n");
}

/// The autostart entry. If the user has turned autostart off in their
/// settings, only the program path is updated and it stays off.
fn autostart(exe: &str) {
    let dir = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("autostart");
    let path = dir.join("clipshelf.desktop");
    let exec = format!("Exec={exe} --background");
    let text = match fs::read_to_string(&path) {
        Ok(old) => old
            .lines()
            .map(|l| if l.starts_with("Exec=") { exec.as_str() } else { l })
            .collect::<Vec<_>>()
            .join("\n")
            + "\n",
        Err(_) => format!(
            "[Desktop Entry]\nType=Application\nName=ClipShelf (background)\n{exec}\n\
             Icon=io.github.idanbarir.ClipShelf\nTerminal=false\nNoDisplay=true\n\
             X-GNOME-Autostart-enabled=true\nX-GNOME-Autostart-Delay=2\n"
        ),
    };
    let _ = fs::create_dir_all(&dir);
    let _ = fs::write(path, text);
}

fn schema(id: &str) -> bool {
    gio::SettingsSchemaSource::default()
        .and_then(|s| s.lookup(id, true))
        .is_some()
}

/// Super+V, plus Ctrl+Super+V as a backup: IBus before 1.5.31 (Ubuntu 24.04,
/// Zorin 18, …) types the letter instead of passing Super+letter shortcuts
/// through while a non-Latin layout is active in a text field, but it does let
/// Ctrl combinations through.
fn shortcuts(exe: &str) {
    if !schema(MK) || !schema(KB_SCHEMA) {
        return;
    }
    let mk = gio::Settings::new(MK);
    let mut list: Vec<String> = mk
        .strv("custom-keybindings")
        .iter()
        .map(|s| s.to_string())
        .collect();
    // Shortcuts added by older versions.
    for id in ["clipshelf-he", "clipshelf-ar", "clipshelf-ru"] {
        let p = format!("{KB_BASE}/{id}/");
        if list.contains(&p) {
            list.retain(|x| x != &p);
            let s = gio::Settings::with_path(KB_SCHEMA, &p);
            for k in ["name", "command", "binding"] {
                s.reset(k);
            }
        }
    }
    for (id, name, binding) in [
        ("clipshelf", "ClipShelf", "<Super>v"),
        ("clipshelf-alt", "ClipShelf (backup shortcut)", "<Control><Super>v"),
    ] {
        let p = format!("{KB_BASE}/{id}/");
        if !list.contains(&p) {
            list.push(p.clone());
        }
        let s = gio::Settings::with_path(KB_SCHEMA, &p);
        let _ = s.set_string("name", name);
        let _ = s.set_string("command", &format!("{exe} --toggle"));
        let _ = s.set_string("binding", binding);
    }
    let refs: Vec<&str> = list.iter().map(String::as_str).collect();
    let _ = mk.set_strv("custom-keybindings", refs.as_slice());

    // GNOME uses Super+V for the notification list; move that to Super+M.
    const SHELL: &str = "org.gnome.shell.keybindings";
    if schema(SHELL) {
        let sh = gio::Settings::new(SHELL);
        let cur: Vec<String> = sh
            .strv("toggle-message-tray")
            .iter()
            .map(|s| s.to_string())
            .collect();
        if cur.iter().any(|k| k.eq_ignore_ascii_case("<super>v")) {
            let mut new: Vec<&str> = cur
                .iter()
                .filter(|k| !k.eq_ignore_ascii_case("<super>v"))
                .map(String::as_str)
                .collect();
            if new.is_empty() {
                new.push("<Super>m");
            }
            let _ = sh.set_strv("toggle-message-tray", new.as_slice());
        }
    }
    gio::Settings::sync();
}

/// Now installed system-wide: remove the copy the old install.sh put in the
/// home folder, so the menu and the shortcuts don't point at two programs.
fn remove_user_install(exe: &Path) {
    let h = home();
    let old_bin = h.join(".local/bin/clipshelf");
    if old_bin != exe {
        let _ = fs::remove_file(old_bin);
    }
    for f in [
        ".local/share/applications/io.github.idanbarir.ClipShelf.desktop",
        ".local/share/icons/hicolor/scalable/apps/io.github.idanbarir.ClipShelf.svg",
        ".local/share/icons/hicolor/symbolic/apps/io.github.idanbarir.ClipShelf-symbolic.svg",
    ] {
        let _ = fs::remove_file(h.join(f));
    }
}
