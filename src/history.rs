//! Clipboard history: in-memory list + JSON persistence + image files on disk.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Image,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Entry {
    /// Content hash (hex). Identical content always gets the same id.
    pub id: String,
    pub kind: Kind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default)]
    pub width: u32,
    #[serde(default)]
    pub height: u32,
    #[serde(default)]
    pub pinned: bool,
    /// Unix time (seconds) of the last copy.
    #[serde(default)]
    pub time: u64,
}

impl Entry {
    pub fn image_path(&self, dir: &Path) -> PathBuf {
        image_path(dir, &self.id)
    }
    pub fn thumb_path(&self, dir: &Path) -> PathBuf {
        thumb_path(dir, &self.id)
    }
}

pub fn image_path(dir: &Path, id: &str) -> PathBuf {
    dir.join("images").join(format!("{id}.png"))
}

pub fn thumb_path(dir: &Path, id: &str) -> PathBuf {
    dir.join("images").join(format!("{id}_thumb.png"))
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 64-bit FNV-1a, enough to tell clipboard contents apart.
pub fn hash(parts: &[&[u8]]) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for p in parts {
        for b in *p {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        // separator so ("ab","c") != ("a","bc")
        h ^= 0xff;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{h:016x}")
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("clipshelf")
}

pub fn config_dir() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("clipshelf")
}

pub struct History {
    pub entries: Vec<Entry>,
    pub dir: PathBuf,
    max: usize,
}

impl History {
    pub fn load(dir: PathBuf, max: usize) -> Self {
        let _ = fs::create_dir_all(dir.join("images"));
        let mut entries: Vec<Entry> = fs::read(dir.join("history.json"))
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        entries.retain(|e| e.kind == Kind::Text || e.image_path(&dir).exists());
        let mut h = History { entries, dir, max: max.max(1) };
        h.trim();
        h.remove_orphan_files();
        h
    }

    pub fn save(&self) {
        let path = self.dir.join("history.json");
        let tmp = self.dir.join("history.json.tmp");
        if let Ok(data) = serde_json::to_vec(&self.entries) {
            if fs::write(&tmp, data).is_ok() {
                let _ = fs::rename(&tmp, &path);
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.id == id)
    }

    /// Adds a new entry at the top (or moves an identical one there).
    /// Returns true if the list changed.
    pub fn add(&mut self, entry: Entry) -> bool {
        if let Some(pos) = self.entries.iter().position(|e| e.id == entry.id) {
            if pos == 0 {
                return false;
            }
            let mut old = self.entries.remove(pos);
            old.time = entry.time;
            self.entries.insert(0, old);
        } else {
            self.entries.insert(0, entry);
        }
        self.trim();
        true
    }

    pub fn remove(&mut self, id: &str) {
        if let Some(pos) = self.entries.iter().position(|e| e.id == id) {
            let e = self.entries.remove(pos);
            self.delete_files(&e);
        }
    }

    pub fn set_pinned(&mut self, id: &str, pinned: bool) {
        if let Some(e) = self.entries.iter_mut().find(|e| e.id == id) {
            e.pinned = pinned;
        }
        self.trim();
    }

    /// Removes everything except pinned items (like Windows' "Clear all").
    pub fn clear_unpinned(&mut self) {
        let removed: Vec<Entry> = self.entries.iter().filter(|e| !e.pinned).cloned().collect();
        self.entries.retain(|e| e.pinned);
        for e in &removed {
            self.delete_files(e);
        }
    }

    fn trim(&mut self) {
        let max = self.max;
        let mut count = 0;
        let mut removed = Vec::new();
        self.entries.retain(|e| {
            if e.pinned {
                return true;
            }
            count += 1;
            if count > max {
                removed.push(e.clone());
                false
            } else {
                true
            }
        });
        for e in &removed {
            self.delete_files(e);
        }
    }

    fn delete_files(&self, e: &Entry) {
        if e.kind == Kind::Image {
            let _ = fs::remove_file(e.image_path(&self.dir));
            let _ = fs::remove_file(e.thumb_path(&self.dir));
        }
    }

    fn remove_orphan_files(&self) {
        let keep: HashSet<&str> = self
            .entries
            .iter()
            .filter(|e| e.kind == Kind::Image)
            .map(|e| e.id.as_str())
            .collect();
        if let Ok(rd) = fs::read_dir(self.dir.join("images")) {
            for f in rd.flatten() {
                let name = f.file_name().to_string_lossy().into_owned();
                let id = name.trim_end_matches(".png").trim_end_matches("_thumb");
                if !keep.contains(id) {
                    let _ = fs::remove_file(f.path());
                }
            }
        }
    }
}
