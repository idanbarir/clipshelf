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
    /// Interface language: "auto" (follow the system), "en" or "he".
    pub language: String,
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
        }
    }
}

impl Config {
    pub fn load() -> Self {
        let dir = crate::history::config_dir();
        let path = dir.join("config.json");
        match fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                eprintln!("clipshelf: invalid {}: {e} — using defaults", path.display());
                Config::default()
            }),
            Err(_) => {
                let cfg = Config::default();
                let _ = fs::create_dir_all(&dir);
                if let Ok(s) = serde_json::to_string_pretty(&cfg) {
                    let _ = fs::write(&path, s);
                }
                cfg
            }
        }
    }
}
