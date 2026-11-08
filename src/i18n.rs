//! Minimal UI translations (English / Hebrew). Add a language by adding a table.

pub struct Strings {
    /// Right-to-left interface layout.
    pub rtl: bool,
    pub title: &'static str,
    pub search: &'static str,
    pub clear_all: &'static str,
    pub clear_all_tip: &'static str,
    pub confirm_title: &'static str,
    /// "{n}" is replaced with the number of items.
    pub confirm_body: &'static str,
    pub cancel: &'static str,
    pub delete_all: &'static str,
    pub pin: &'static str,
    pub delete: &'static str,
    pub empty: &'static str,
    pub image: &'static str,
}

static EN: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Search…",
    clear_all: "Clear all",
    clear_all_tip: "Delete all items except pinned ones",
    confirm_title: "Clear the clipboard history?",
    confirm_body: "{n} items will be deleted permanently.\nPinned items are kept.",
    cancel: "Cancel",
    delete_all: "Delete",
    pin: "Pin",
    delete: "Delete",
    empty: "Nothing copied yet.\nEverything you copy will show up here.",
    image: "Image",
};

static HE: Strings = Strings {
    rtl: true,
    title: "ClipShelf",
    search: "חיפוש…",
    clear_all: "ניקוי הכול",
    clear_all_tip: "מחיקת כל הפריטים חוץ מהנעוצים",
    confirm_title: "למחוק את היסטוריית ההעתקות?",
    confirm_body: "{n} פריטים יימחקו לצמיתות.\nפריטים נעוצים יישמרו.",
    cancel: "ביטול",
    delete_all: "מחיקה",
    pin: "נעיצה",
    delete: "מחיקה",
    empty: "עדיין לא הועתק דבר.\nכל מה שתעתיקו יופיע כאן.",
    image: "תמונה",
};

fn lang() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    String::new()
}

/// `setting` is the config value: "auto" follows the system language,
/// otherwise a language code ("en", "he"). Unknown languages fall back to English.
pub fn strings(setting: &str) -> &'static Strings {
    let l = match setting.trim().to_lowercase().as_str() {
        "" | "auto" => lang(),
        other => other.to_string(),
    };
    if l.starts_with("he") || l.starts_with("iw") {
        &HE
    } else {
        &EN
    }
}
