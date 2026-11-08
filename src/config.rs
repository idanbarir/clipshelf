//! User settings, stored in ~/.config/clipshelf/config.json.

use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    /// How many unpinned items to keep.
    pub max_items: usize,
    /// Paste into the previously focused window right after choosing an item.
    /// When false, the item is only placed on the clipboard.
    pub auto_paste: bool,
    /// Key combination used for auto-paste: "ctrl+v", "ctrl+shift+v" or "shift+insert".
    pub paste_keys: String,
    /// Delay (ms) between closing the popup and sending the paste keys,
    /// so focus can return to the previous window.
    pub paste_delay_ms: u64,
    /// Optional shell command that performs the paste instead of the built-in
    /// method, e.g. "ydotool key 29:1 47:1 47:0 29:0".
    pub paste_command: Option<String>,
    /// Close the popup when it loses focus (like Windows).
    pub hide_on_focus_loss: bool,
    /// Ignore content marked as secret by password managers (KeePassXC etc.).
    pub ignore_passwords: bool,
    /// Interface language: "auto" (follow the system) or a code like "en", "he", "de".
    pub language: String,
    /// Show an icon in the system tray (next to the keyboard-language indicator).
    pub tray_icon: bool,
    /// "flyout": a small borderless panel next to the system tray, not in the
    /// taskbar (like Windows). "window": a regular window.
    pub popup_style: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            max_items: 100,
            auto_paste: true,
            paste_keys: "ctrl+v".into(),
            paste_delay_ms: 200,
            paste_command: None,
            hide_on_focus_loss: true,
            ignore_passwords: true,
            language: "auto".into(),
            tray_icon: true,
            popup_style: "flyout".into(),
        }
    }
}

impl Config {
    fn path() -> std::path::PathBuf {
        crate::history::config_dir().join("config.json")
    }

    pub fn load() -> Self {
        let path = Self::path();
        let existing = fs::read_to_string(&path).ok();
        let cfg = match &existing {
            Some(text) => match serde_json::from_str(text) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("clipshelf: invalid {}: {e} — using defaults", path.display());
                    return Config::default();
                }
            },
            None => Config::default(),
        };
        // Write the file back so settings added in newer versions show up in it
        // (the user's own values are kept).
        if let Ok(s) = serde_json::to_string_pretty(&cfg) {
            if existing.as_deref().map(str::trim) != Some(s.trim()) {
                cfg.save();
            }
        }
        cfg
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = fs::write(&path, s + "\n");
        }
    }
}
