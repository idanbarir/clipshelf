//! Minimal UI translations (English / Hebrew). Add a language by adding a table.

pub struct Strings {
    pub title: &'static str,
    pub search: &'static str,
    pub clear_all: &'static str,
    pub pin: &'static str,
    pub delete: &'static str,
    pub empty: &'static str,
    pub image: &'static str,
}

static EN: Strings = Strings {
    title: "Clipboard",
    search: "Search…",
    clear_all: "Clear all (pinned items are kept)",
    pin: "Pin",
    delete: "Delete",
    empty: "Nothing copied yet.\nEverything you copy will show up here.",
    image: "Image",
};

static HE: Strings = Strings {
    title: "לוח העתקה",
    search: "חיפוש…",
    clear_all: "ניקוי הכול (פריטים נעוצים יישמרו)",
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

pub fn strings() -> &'static Strings {
    let l = lang();
    if l.starts_with("he") || l.starts_with("iw") {
        &HE
    } else {
        &EN
    }
}
