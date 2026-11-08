//! UI translations.
//!
//! To add a language: copy one of the tables below, translate the strings,
//! and add it to `LANGS`. Corrections from native speakers are very welcome.
//! Sentences avoid putting a number next to a noun, so no plural rules are needed.

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
    // tray menu
    pub tray_show: &'static str,
    pub tray_language: &'static str,
    pub tray_auto: &'static str,
    pub tray_settings: &'static str,
    pub tray_quit: &'static str,
}

static EN: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Search…",
    clear_all: "Clear all",
    clear_all_tip: "Delete all items except pinned ones",
    confirm_title: "Clear the clipboard history?",
    confirm_body: "Items to be deleted: {n}\nPinned items will be kept.",
    cancel: "Cancel",
    delete_all: "Delete",
    pin: "Pin",
    delete: "Delete",
    empty: "Nothing copied yet.\nEverything you copy will show up here.",
    image: "Image",
    tray_show: "Show clipboard history",
    tray_language: "Language",
    tray_auto: "Automatic (system language)",
    tray_settings: "Open settings file",
    tray_quit: "Quit ClipShelf",
};

static HE: Strings = Strings {
    rtl: true,
    title: "ClipShelf",
    search: "חיפוש…",
    clear_all: "ניקוי הכול",
    clear_all_tip: "מחיקת כל הפריטים חוץ מהנעוצים",
    confirm_title: "למחוק את היסטוריית ההעתקות?",
    confirm_body: "פריטים שיימחקו: {n}\nפריטים נעוצים יישמרו.",
    cancel: "ביטול",
    delete_all: "מחיקה",
    pin: "נעיצה",
    delete: "מחיקה",
    empty: "עדיין לא הועתק דבר.\nכל מה שתעתיקו יופיע כאן.",
    image: "תמונה",
    tray_show: "הצגת היסטוריית ההעתקות",
    tray_language: "שפה",
    tray_auto: "אוטומטי (שפת המערכת)",
    tray_settings: "פתיחת קובץ ההגדרות",
    tray_quit: "יציאה מ־ClipShelf",
};

static AR: Strings = Strings {
    rtl: true,
    title: "ClipShelf",
    search: "بحث…",
    clear_all: "مسح الكل",
    clear_all_tip: "حذف كل العناصر ما عدا المثبّتة",
    confirm_title: "مسح سجل الحافظة؟",
    confirm_body: "العناصر التي ستُحذف: {n}\nستُحفظ العناصر المثبّتة.",
    cancel: "إلغاء",
    delete_all: "حذف",
    pin: "تثبيت",
    delete: "حذف",
    empty: "لم يُنسخ شيء بعد.\nكل ما تنسخه سيظهر هنا.",
    image: "صورة",
    tray_show: "عرض سجل الحافظة",
    tray_language: "اللغة",
    tray_auto: "تلقائي (لغة النظام)",
    tray_settings: "فتح ملف الإعدادات",
    tray_quit: "إنهاء ClipShelf",
};

static DE: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Suchen…",
    clear_all: "Alle löschen",
    clear_all_tip: "Alle Einträge außer angehefteten löschen",
    confirm_title: "Zwischenablage-Verlauf löschen?",
    confirm_body: "Zu löschende Einträge: {n}\nAngeheftete Einträge bleiben erhalten.",
    cancel: "Abbrechen",
    delete_all: "Löschen",
    pin: "Anheften",
    delete: "Löschen",
    empty: "Noch nichts kopiert.\nAlles, was Sie kopieren, erscheint hier.",
    image: "Bild",
    tray_show: "Zwischenablage-Verlauf anzeigen",
    tray_language: "Sprache",
    tray_auto: "Automatisch (Systemsprache)",
    tray_settings: "Einstellungsdatei öffnen",
    tray_quit: "ClipShelf beenden",
};

static ES: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Buscar…",
    clear_all: "Borrar todo",
    clear_all_tip: "Eliminar todos los elementos excepto los fijados",
    confirm_title: "¿Borrar el historial del portapapeles?",
    confirm_body: "Elementos que se eliminarán: {n}\nLos elementos fijados se conservarán.",
    cancel: "Cancelar",
    delete_all: "Eliminar",
    pin: "Fijar",
    delete: "Eliminar",
    empty: "Aún no se ha copiado nada.\nTodo lo que copies aparecerá aquí.",
    image: "Imagen",
    tray_show: "Mostrar el historial del portapapeles",
    tray_language: "Idioma",
    tray_auto: "Automático (idioma del sistema)",
    tray_settings: "Abrir el archivo de configuración",
    tray_quit: "Salir de ClipShelf",
};

static FR: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Rechercher…",
    clear_all: "Tout effacer",
    clear_all_tip: "Supprimer tous les éléments sauf ceux épinglés",
    confirm_title: "Effacer l’historique du presse-papiers ?",
    confirm_body: "Éléments à supprimer : {n}\nLes éléments épinglés seront conservés.",
    cancel: "Annuler",
    delete_all: "Supprimer",
    pin: "Épingler",
    delete: "Supprimer",
    empty: "Rien n’a encore été copié.\nTout ce que vous copiez apparaîtra ici.",
    image: "Image",
    tray_show: "Afficher l’historique du presse-papiers",
    tray_language: "Langue",
    tray_auto: "Automatique (langue du système)",
    tray_settings: "Ouvrir le fichier de configuration",
    tray_quit: "Quitter ClipShelf",
};

static IT: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Cerca…",
    clear_all: "Cancella tutto",
    clear_all_tip: "Elimina tutti gli elementi tranne quelli fissati",
    confirm_title: "Cancellare la cronologia degli appunti?",
    confirm_body: "Elementi da eliminare: {n}\nGli elementi fissati verranno mantenuti.",
    cancel: "Annulla",
    delete_all: "Elimina",
    pin: "Fissa",
    delete: "Elimina",
    empty: "Non è ancora stato copiato nulla.\nTutto ciò che copi apparirà qui.",
    image: "Immagine",
    tray_show: "Mostra la cronologia degli appunti",
    tray_language: "Lingua",
    tray_auto: "Automatica (lingua di sistema)",
    tray_settings: "Apri il file delle impostazioni",
    tray_quit: "Esci da ClipShelf",
};

static NL: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Zoeken…",
    clear_all: "Alles wissen",
    clear_all_tip: "Alle items verwijderen behalve vastgezette",
    confirm_title: "Klembordgeschiedenis wissen?",
    confirm_body: "Te verwijderen items: {n}\nVastgezette items blijven behouden.",
    cancel: "Annuleren",
    delete_all: "Verwijderen",
    pin: "Vastzetten",
    delete: "Verwijderen",
    empty: "Nog niets gekopieerd.\nAlles wat je kopieert, verschijnt hier.",
    image: "Afbeelding",
    tray_show: "Klembordgeschiedenis tonen",
    tray_language: "Taal",
    tray_auto: "Automatisch (systeemtaal)",
    tray_settings: "Instellingenbestand openen",
    tray_quit: "ClipShelf afsluiten",
};

static PL: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Szukaj…",
    clear_all: "Wyczyść wszystko",
    clear_all_tip: "Usuń wszystkie elementy oprócz przypiętych",
    confirm_title: "Wyczyścić historię schowka?",
    confirm_body: "Elementy do usunięcia: {n}\nPrzypięte elementy zostaną zachowane.",
    cancel: "Anuluj",
    delete_all: "Usuń",
    pin: "Przypnij",
    delete: "Usuń",
    empty: "Nic jeszcze nie skopiowano.\nWszystko, co skopiujesz, pojawi się tutaj.",
    image: "Obraz",
    tray_show: "Pokaż historię schowka",
    tray_language: "Język",
    tray_auto: "Automatycznie (język systemu)",
    tray_settings: "Otwórz plik ustawień",
    tray_quit: "Zakończ ClipShelf",
};

// Worded to read naturally in both European and Brazilian Portuguese.
static PT: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Pesquisar…",
    clear_all: "Limpar tudo",
    clear_all_tip: "Apagar todos os itens exceto os fixados",
    confirm_title: "Limpar o histórico da área de transferência?",
    confirm_body: "Itens a apagar: {n}\nOs itens fixados serão mantidos.",
    cancel: "Cancelar",
    delete_all: "Apagar",
    pin: "Fixar",
    delete: "Apagar",
    empty: "Nada foi copiado ainda.\nTudo o que copiar aparecerá aqui.",
    image: "Imagem",
    tray_show: "Mostrar o histórico da área de transferência",
    tray_language: "Idioma",
    tray_auto: "Automático (idioma do sistema)",
    tray_settings: "Abrir as configurações",
    tray_quit: "Sair do ClipShelf",
};

static RU: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Поиск…",
    clear_all: "Очистить всё",
    clear_all_tip: "Удалить все элементы, кроме закреплённых",
    confirm_title: "Очистить историю буфера обмена?",
    confirm_body: "Будет удалено элементов: {n}\nЗакреплённые элементы сохранятся.",
    cancel: "Отмена",
    delete_all: "Удалить",
    pin: "Закрепить",
    delete: "Удалить",
    empty: "Пока ничего не скопировано.\nВсё, что вы скопируете, появится здесь.",
    image: "Изображение",
    tray_show: "Показать историю буфера обмена",
    tray_language: "Язык",
    tray_auto: "Автоматически (язык системы)",
    tray_settings: "Открыть файл настроек",
    tray_quit: "Выйти из ClipShelf",
};

static TR: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Ara…",
    clear_all: "Tümünü temizle",
    clear_all_tip: "Sabitlenenler dışındaki tüm öğeleri sil",
    confirm_title: "Pano geçmişi temizlensin mi?",
    confirm_body: "Silinecek öğe sayısı: {n}\nSabitlenen öğeler korunacak.",
    cancel: "İptal",
    delete_all: "Sil",
    pin: "Sabitle",
    delete: "Sil",
    empty: "Henüz bir şey kopyalanmadı.\nKopyaladığınız her şey burada görünecek.",
    image: "Görsel",
    tray_show: "Pano geçmişini göster",
    tray_language: "Dil",
    tray_auto: "Otomatik (sistem dili)",
    tray_settings: "Ayarlar dosyasını aç",
    tray_quit: "ClipShelf'ten çık",
};

static UK: Strings = Strings {
    rtl: false,
    title: "ClipShelf",
    search: "Пошук…",
    clear_all: "Очистити все",
    clear_all_tip: "Видалити всі елементи, крім закріплених",
    confirm_title: "Очистити історію буфера обміну?",
    confirm_body: "Буде видалено елементів: {n}\nЗакріплені елементи буде збережено.",
    cancel: "Скасувати",
    delete_all: "Видалити",
    pin: "Закріпити",
    delete: "Видалити",
    empty: "Ще нічого не скопійовано.\nУсе, що ви скопіюєте, з'явиться тут.",
    image: "Зображення",
    tray_show: "Показати історію буфера обміну",
    tray_language: "Мова",
    tray_auto: "Автоматично (мова системи)",
    tray_settings: "Відкрити файл налаштувань",
    tray_quit: "Вийти з ClipShelf",
};

/// (code, name in its own language, strings)
pub static LANGS: &[(&str, &str, &Strings)] = &[
    ("en", "English", &EN),
    ("ar", "العربية", &AR),
    ("de", "Deutsch", &DE),
    ("es", "Español", &ES),
    ("fr", "Français", &FR),
    ("he", "עברית", &HE),
    ("it", "Italiano", &IT),
    ("nl", "Nederlands", &NL),
    ("pl", "Polski", &PL),
    ("pt", "Português", &PT),
    ("ru", "Русский", &RU),
    ("tr", "Türkçe", &TR),
    ("uk", "Українська", &UK),
];

fn system_lang() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            if !v.is_empty() && v != "C" && v != "POSIX" {
                return v;
            }
        }
    }
    String::new()
}

/// `setting` is the config value: "auto" follows the system language,
/// otherwise a language code ("en", "he", "de"…). Unknown languages fall back to English.
pub fn strings(setting: &str) -> &'static Strings {
    let l = match setting.trim().to_lowercase().as_str() {
        "" | "auto" => system_lang().to_lowercase(),
        other => other.to_string(),
    };
    let code: String = l.chars().take(2).collect();
    let code = if code == "iw" { "he".to_string() } else { code };
    LANGS
        .iter()
        .find(|(c, _, _)| *c == code)
        .map(|(_, _, s)| *s)
        .unwrap_or(&EN)
}
