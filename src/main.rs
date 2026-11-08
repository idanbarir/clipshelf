//! clipshelf — a simple Windows-style (Super+V) clipboard history for Linux.
//!
//! One binary, one process: the first `clipshelf` started becomes the
//! background service (it watches the clipboard and owns the popup window).
//! Running `clipshelf` again — e.g. from the Super+V shortcut — just tells
//! that service to show the popup (GApplication single-instance over D-Bus).

mod config;
mod history;
mod i18n;
mod paste;
mod tray;
mod watcher;

use gtk::prelude::*;
use gtk::{gdk, gio, glib, pango};
use history::{Entry, Kind};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const APP_ID: &str = "io.github.clipshelf.Clipshelf";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const CSS: &str = "
.clip-row { padding: 8px 6px 8px 10px; }
.clip-meta { font-size: 0.85em; opacity: 0.65; }
.clip-search { margin: 8px; }
.clip-row button { min-height: 24px; min-width: 24px; padding: 2px; }
";

fn print_help() {
    println!(
        "clipshelf {VERSION} — clipboard history for Linux (Super+V)

USAGE:
    clipshelf               show the clipboard history popup
    clipshelf --toggle      show the popup, or hide it if it's open
    clipshelf --background  start the background service without showing anything
    clipshelf --quit        stop the background service
    clipshelf --version

Settings: ~/.config/clipshelf/config.json
History:  ~/.local/share/clipshelf/"
    );
}

fn main() -> glib::ExitCode {
    let args: Vec<String> = std::env::args().collect();
    // Translate flags into plain words so GApplication forwards them untouched.
    let cmd = match args.get(1).map(String::as_str) {
        None | Some("show") | Some("--show") => "show",
        Some("toggle") | Some("--toggle") => "toggle",
        Some("background") | Some("--background") | Some("--daemon") | Some("-b") => "background",
        Some("quit") | Some("--quit") => "quit",
        Some("--version") | Some("-V") => {
            println!("clipshelf {VERSION}");
            return glib::ExitCode::SUCCESS;
        }
        Some("--help") | Some("-h") => {
            print_help();
            return glib::ExitCode::SUCCESS;
        }
        Some(other) => {
            eprintln!("clipshelf: unknown option '{other}'\n");
            print_help();
            return glib::ExitCode::FAILURE;
        }
    };

    let app = gtk::Application::builder()
        .application_id(APP_ID)
        .flags(gio::ApplicationFlags::HANDLES_COMMAND_LINE)
        .build();

    let ui: Rc<RefCell<Option<Rc<Ui>>>> = Rc::default();
    {
        let ui = ui.clone();
        app.connect_startup(move |app| {
            // Keep running in the background after the window is hidden.
            std::mem::forget(app.hold());
            *ui.borrow_mut() = Some(Ui::new(app));
        });
    }
    {
        let ui = ui.clone();
        app.connect_command_line(move |app, cl| {
            let cmd = cl
                .arguments()
                .get(1)
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "show".into());
            let ui = ui.borrow().clone();
            if let Some(ui) = ui {
                match cmd.as_str() {
                    "quit" => app.quit(),
                    "background" => {}
                    "toggle" if ui.win.is_visible() => ui.hide(),
                    _ => ui.show(),
                }
            }
            0
        });
    }
    app.run_with_args(&[args[0].clone(), cmd.to_string()])
}

struct RowInfo {
    row: gtk::ListBoxRow,
    id: String,
    /// Lower-cased text for searching (None for images).
    haystack: Option<String>,
}

struct Ui {
    win: gtk::ApplicationWindow,
    search: gtk::SearchEntry,
    list: gtk::ListBox,
    scroller: gtk::ScrolledWindow,
    rows: RefCell<Vec<RowInfo>>,
    history: RefCell<history::History>,
    config: config::Config,
    clipboard: RefCell<Option<arboard::Clipboard>>,
    paster: paste::Paster,
    was_active: Cell<bool>,
    /// A dialog of ours is open: don't hide the popup when it takes focus.
    dialog_open: Cell<bool>,
    /// When the popup was last hidden (to make a tray click a real toggle).
    hidden_at: Cell<Option<Instant>>,
    t: &'static i18n::Strings,
}

impl Ui {
    fn new(app: &gtk::Application) -> Rc<Self> {
        let config = config::Config::load();
        let t = i18n::strings(&config.language);
        // Lay the window out to match the chosen language, not just the system locale.
        gtk::Widget::set_default_direction(if t.rtl {
            gtk::TextDirection::Rtl
        } else {
            gtk::TextDirection::Ltr
        });
        let dir = history::data_dir();
        let history = history::History::load(dir.clone(), config.max_items);

        if let Some(display) = gdk::Display::default() {
            let provider = gtk::CssProvider::new();
            provider.load_from_data(CSS);
            #[allow(deprecated)]
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }

        let win = gtk::ApplicationWindow::builder()
            .application(app)
            .title(t.title)
            .default_width(420)
            .default_height(560)
            .hide_on_close(true)
            .icon_name("edit-paste")
            .build();

        let header = gtk::HeaderBar::new();
        // Red, labelled, and on the opposite side from the window's close button,
        // so it can't be mistaken for "close".
        let clear_btn = gtk::Button::with_label(t.clear_all);
        clear_btn.add_css_class("destructive-action");
        clear_btn.set_tooltip_text(Some(t.clear_all_tip));
        header.pack_start(&clear_btn);
        win.set_titlebar(Some(&header));

        let search = gtk::SearchEntry::new();
        search.set_property("placeholder-text", t.search);
        search.add_css_class("clip-search");
        search.set_key_capture_widget(Some(&win));

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::Browse);
        list.set_activate_on_single_click(true);
        let placeholder = gtk::Label::new(Some(t.empty));
        placeholder.set_justify(gtk::Justification::Center);
        placeholder.add_css_class("dim-label");
        placeholder.set_margin_top(48);
        placeholder.set_margin_start(24);
        placeholder.set_margin_end(24);
        list.set_placeholder(Some(&placeholder));

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vexpand(true)
            .child(&list)
            .build();

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.append(&search);
        vbox.append(&scroller);
        win.set_child(Some(&vbox));

        let paster = paste::Paster::new(&config.paste_keys, config.paste_command.clone());

        let ui = Rc::new(Ui {
            win,
            search,
            list,
            scroller,
            rows: RefCell::new(Vec::new()),
            history: RefCell::new(history),
            clipboard: RefCell::new(arboard::Clipboard::new().ok()),
            paster,
            config,
            was_active: Cell::new(false),
            dialog_open: Cell::new(false),
            hidden_at: Cell::new(None),
            t,
        });

        // --- signals -------------------------------------------------------
        {
            let w = Rc::downgrade(&ui);
            clear_btn.connect_clicked(move |_| {
                if let Some(ui) = w.upgrade() {
                    ui.confirm_clear();
                }
            });
        }
        {
            let w = Rc::downgrade(&ui);
            ui.list.connect_row_activated(move |_, row| {
                if let Some(ui) = w.upgrade() {
                    if let Some(id) = ui.id_of(row) {
                        ui.paste(&id);
                    }
                }
            });
        }
        {
            let w = Rc::downgrade(&ui);
            ui.search.connect_search_changed(move |_| {
                if let Some(ui) = w.upgrade() {
                    ui.apply_filter();
                }
            });
        }
        {
            // Enter in the search box pastes the first match.
            let w = Rc::downgrade(&ui);
            ui.search.connect_activate(move |_| {
                if let Some(ui) = w.upgrade() {
                    if let Some(row) = ui.first_visible_row() {
                        if let Some(id) = ui.id_of(&row) {
                            ui.paste(&id);
                        }
                    }
                }
            });
        }
        {
            let key = gtk::EventControllerKey::new();
            key.set_propagation_phase(gtk::PropagationPhase::Capture);
            let w = Rc::downgrade(&ui);
            key.connect_key_pressed(move |_, keyval, _, _| {
                let Some(ui) = w.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let focus = gtk::prelude::GtkWindowExt::focus(&ui.win);
                let in_search = focus
                    .as_ref()
                    .map(|f| f.is_ancestor(&ui.search) || f == ui.search.upcast_ref::<gtk::Widget>())
                    .unwrap_or(false);
                match keyval {
                    gdk::Key::Escape => {
                        ui.hide();
                        glib::Propagation::Stop
                    }
                    gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => {
                        // Paste the highlighted item (or the first match).
                        let row = ui.list.selected_row().filter(|r| r.is_visible()).or_else(|| ui.first_visible_row());
                        if let Some(id) = row.and_then(|r| ui.id_of(&r)) {
                            ui.paste(&id);
                        }
                        glib::Propagation::Stop
                    }
                    gdk::Key::Down if in_search => {
                        if let Some(row) = ui.first_visible_row() {
                            ui.list.select_row(Some(&row));
                            row.grab_focus();
                        }
                        glib::Propagation::Stop
                    }
                    gdk::Key::Delete | gdk::Key::KP_Delete if !in_search => {
                        if let Some(row) = ui.list.selected_row() {
                            if let Some(id) = ui.id_of(&row) {
                                ui.delete(&id);
                            }
                        }
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                }
            });
            ui.win.add_controller(key);
        }
        {
            // Close when the user clicks somewhere else, like Windows does.
            let w = Rc::downgrade(&ui);
            ui.win.connect_is_active_notify(move |win| {
                let Some(ui) = w.upgrade() else { return };
                if win.is_active() {
                    ui.was_active.set(true);
                } else if ui.was_active.replace(false)
                    && ui.config.hide_on_focus_loss
                    && !ui.dialog_open.get()
                {
                    ui.hide();
                }
            });
        }

        // --- clipboard watcher ---------------------------------------------
        let (tx, rx) = mpsc::channel::<Entry>();
        watcher::spawn(dir, ui.config.ignore_passwords, tx);
        {
            let w = Rc::downgrade(&ui);
            glib::timeout_add_local(Duration::from_millis(200), move || {
                let Some(ui) = w.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                let mut changed = false;
                while let Ok(e) = rx.try_recv() {
                    changed |= ui.history.borrow_mut().add(e);
                }
                if changed {
                    ui.history.borrow().save();
                    if ui.win.is_visible() {
                        ui.refresh();
                    }
                }
                glib::ControlFlow::Continue
            });
        }

        // --- tray icon -------------------------------------------------------
        if ui.config.tray_icon {
            let (ttx, trx) = mpsc::channel::<tray::Cmd>();
            tray::spawn(ttx, t, ui.config.language.clone());
            let w = Rc::downgrade(&ui);
            glib::timeout_add_local(Duration::from_millis(50), move || {
                let Some(ui) = w.upgrade() else {
                    return glib::ControlFlow::Break;
                };
                while let Ok(cmd) = trx.try_recv() {
                    ui.tray_command(cmd);
                }
                glib::ControlFlow::Continue
            });
        }

        ui
    }

    fn tray_command(self: &Rc<Self>, cmd: tray::Cmd) {
        match cmd {
            tray::Cmd::Toggle(token) => {
                // Clicking the tray icon takes focus away from the popup, which
                // hides it a moment before the click arrives: treat that as "close".
                let just_hidden = self
                    .hidden_at
                    .get()
                    .map(|t| t.elapsed() < Duration::from_millis(400))
                    .unwrap_or(false);
                if self.win.is_visible() {
                    self.hide();
                } else if !just_hidden {
                    if let Some(tok) = token {
                        self.win.set_startup_id(&tok);
                    }
                    self.show();
                }
            }
            tray::Cmd::ClearAll => {
                self.show();
                self.confirm_clear();
            }
            tray::Cmd::Settings => {
                let path = history::config_dir().join("config.json");
                self.config.save(); // make sure it exists
                let file = gio::File::for_path(&path);
                let app = gio::AppInfo::default_for_type("text/plain", false);
                let ok = match app {
                    Some(app) => app.launch(&[file.clone()], None::<&gio::AppLaunchContext>).is_ok(),
                    None => false,
                };
                if !ok {
                    let _ = gio::AppInfo::launch_default_for_uri(&file.uri(), None::<&gio::AppLaunchContext>);
                }
            }
            tray::Cmd::Language(code) => {
                let mut cfg = self.config.clone();
                cfg.language = code;
                cfg.save();
                self.restart();
            }
            tray::Cmd::Quit => {
                if let Some(app) = self.win.application() {
                    app.quit();
                }
            }
        }
    }

    /// Quits and starts again in the background (used after changing the language).
    fn restart(&self) {
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new("sh")
                .arg("-c")
                .arg("sleep 0.8; exec \"$0\" --background")
                .arg(exe)
                .spawn();
        }
        if let Some(app) = self.win.application() {
            app.quit();
        }
    }

    fn show(self: &Rc<Self>) {
        self.search.set_text("");
        self.refresh();
        self.win.present();
        self.scroller.vadjustment().set_value(0.0);
        if let Some(row) = self.first_visible_row() {
            self.list.select_row(Some(&row));
            row.grab_focus();
        }
        // Focus again once the window is actually mapped.
        let w = Rc::downgrade(self);
        glib::timeout_add_local_once(Duration::from_millis(60), move || {
            if let Some(ui) = w.upgrade() {
                if let Some(row) = ui.first_visible_row() {
                    ui.list.select_row(Some(&row));
                    row.grab_focus();
                }
            }
        });
    }

    fn hide(&self) {
        if self.win.is_visible() {
            self.hidden_at.set(Some(Instant::now()));
        }
        self.was_active.set(false);
        self.win.set_visible(false);
    }

    fn id_of(&self, row: &gtk::ListBoxRow) -> Option<String> {
        self.rows
            .borrow()
            .iter()
            .find(|r| &r.row == row)
            .map(|r| r.id.clone())
    }

    fn first_visible_row(&self) -> Option<gtk::ListBoxRow> {
        self.rows
            .borrow()
            .iter()
            .find(|r| r.row.is_visible())
            .map(|r| r.row.clone())
    }

    fn refresh(self: &Rc<Self>) {
        // Remove only our rows (first_child() would also catch the placeholder label).
        for r in self.rows.borrow_mut().drain(..) {
            self.list.remove(&r.row);
        }
        let entries = self.history.borrow().entries.clone();
        let mut rows = Vec::with_capacity(entries.len());
        for e in &entries {
            let info = self.build_row(e);
            self.list.append(&info.row);
            rows.push(info);
        }
        *self.rows.borrow_mut() = rows;
        self.apply_filter();
    }

    fn apply_filter(&self) {
        let q = self.search.text().to_lowercase();
        let q = q.trim();
        for r in self.rows.borrow().iter() {
            let visible = q.is_empty()
                || r.haystack.as_deref().map(|h| h.contains(q)).unwrap_or(false);
            r.row.set_visible(visible);
        }
        if !q.is_empty() {
            if let Some(row) = self.first_visible_row() {
                self.list.select_row(Some(&row));
            }
        }
    }

    fn build_row(self: &Rc<Self>, e: &Entry) -> RowInfo {
        let dir = self.history.borrow().dir.clone();
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        hbox.add_css_class("clip-row");

        let (content, haystack): (gtk::Widget, Option<String>) = match e.kind {
            Kind::Text => {
                let full = e.text.as_deref().unwrap_or("");
                let preview: String = full.trim().chars().take(600).collect();
                let preview = preview.replace('\t', "    ");
                let label = gtk::Label::new(Some(&preview));
                label.set_xalign(0.0);
                label.set_wrap(true);
                label.set_wrap_mode(pango::WrapMode::WordChar);
                label.set_lines(4);
                label.set_ellipsize(pango::EllipsizeMode::End);
                label.set_hexpand(true);
                label.set_width_chars(10);
                label.set_max_width_chars(60);
                // Right-align Hebrew/Arabic text, left-align Latin text,
                // whatever the UI language is.
                label.set_direction(if starts_rtl(full) {
                    gtk::TextDirection::Rtl
                } else {
                    gtk::TextDirection::Ltr
                });
                (label.upcast(), Some(full.to_lowercase()))
            }
            Kind::Image => {
                let vb = gtk::Box::new(gtk::Orientation::Vertical, 4);
                vb.set_hexpand(true);
                let thumb = e.thumb_path(&dir);
                let path = if thumb.exists() { thumb } else { e.image_path(&dir) };
                let pic = gtk::Picture::for_filename(&path);
                pic.set_can_shrink(true);
                pic.set_halign(gtk::Align::Start);
                let scale = f64::min(360.0 / e.width as f64, 140.0 / e.height as f64).min(1.0);
                pic.set_size_request(-1, ((e.height as f64 * scale) as i32).max(16));
                let meta = gtk::Label::new(Some(&format!("{} · {} × {}", self.t.image, e.width, e.height)));
                meta.set_xalign(0.0);
                meta.add_css_class("clip-meta");
                vb.append(&pic);
                vb.append(&meta);
                (vb.upcast(), None)
            }
        };

        let buttons = gtk::Box::new(gtk::Orientation::Vertical, 2);
        buttons.set_valign(gtk::Align::Start);
        let pin = gtk::ToggleButton::new();
        pin.set_icon_name("view-pin-symbolic");
        pin.set_active(e.pinned);
        pin.add_css_class("flat");
        pin.set_tooltip_text(Some(self.t.pin));
        pin.set_focus_on_click(false);
        let del = gtk::Button::from_icon_name("user-trash-symbolic");
        del.add_css_class("flat");
        del.set_tooltip_text(Some(self.t.delete));
        del.set_focus_on_click(false);
        buttons.append(&pin);
        buttons.append(&del);

        {
            let w = Rc::downgrade(self);
            let id = e.id.clone();
            pin.connect_toggled(move |b| {
                if let Some(ui) = w.upgrade() {
                    ui.history.borrow_mut().set_pinned(&id, b.is_active());
                    ui.history.borrow().save();
                }
            });
        }
        {
            let w = Rc::downgrade(self);
            let id = e.id.clone();
            del.connect_clicked(move |_| {
                if let Some(ui) = w.upgrade() {
                    ui.delete(&id);
                }
            });
        }

        hbox.append(&content);
        hbox.append(&buttons);
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&hbox));
        RowInfo { row, id: e.id.clone(), haystack }
    }

    /// "Clear all" asks first, so a mis-click can't wipe the history.
    fn confirm_clear(self: &Rc<Self>) {
        let n = self.history.borrow().entries.iter().filter(|e| !e.pinned).count();
        if n == 0 {
            return;
        }
        #[allow(deprecated)]
        let dlg = gtk::MessageDialog::builder()
            .transient_for(&self.win)
            .modal(true)
            .message_type(gtk::MessageType::Warning)
            .text(self.t.confirm_title)
            .secondary_text(self.t.confirm_body.replace("{n}", &n.to_string()))
            .build();
        #[allow(deprecated)]
        {
            dlg.add_button(self.t.cancel, gtk::ResponseType::Cancel);
            let del = dlg.add_button(self.t.delete_all, gtk::ResponseType::Accept);
            del.add_css_class("destructive-action");
            dlg.set_default_response(gtk::ResponseType::Cancel);
        }
        self.dialog_open.set(true);
        let w = Rc::downgrade(self);
        #[allow(deprecated)]
        dlg.connect_response(move |d, resp| {
            d.close();
            let Some(ui) = w.upgrade() else { return };
            ui.dialog_open.set(false);
            if resp == gtk::ResponseType::Accept {
                ui.history.borrow_mut().clear_unpinned();
                ui.history.borrow().save();
                ui.refresh();
            }
            ui.win.present();
        });
        dlg.present();
    }

    fn delete(&self, id: &str) {
        self.history.borrow_mut().remove(id);
        self.history.borrow().save();
        let mut rows = self.rows.borrow_mut();
        if let Some(pos) = rows.iter().position(|r| r.id == id) {
            let r = rows.remove(pos);
            self.list.remove(&r.row);
            // keep keyboard focus in the list
            let next = rows.get(pos).or_else(|| rows.last()).map(|r| r.row.clone());
            drop(rows);
            if let Some(next) = next {
                self.list.select_row(Some(&next));
                next.grab_focus();
            }
        }
    }

    /// Puts the item on the clipboard, closes the popup and pastes it.
    fn paste(self: &Rc<Self>, id: &str) {
        let Some(e) = self.history.borrow().get(id).cloned() else { return };
        let dir = self.history.borrow().dir.clone();
        let ok = {
            let mut cb = self.clipboard.borrow_mut();
            if cb.is_none() {
                *cb = arboard::Clipboard::new().ok();
            }
            match cb.as_mut() {
                None => false,
                Some(cb) => match e.kind {
                    Kind::Text => cb.set_text(e.text.clone().unwrap_or_default()).is_ok(),
                    Kind::Image => match image::open(e.image_path(&dir)) {
                        Ok(img) => {
                            let rgba = img.to_rgba8();
                            let (w, h) = rgba.dimensions();
                            cb.set_image(arboard::ImageData {
                                width: w as usize,
                                height: h as usize,
                                bytes: Cow::Owned(rgba.into_raw()),
                            })
                            .is_ok()
                        }
                        Err(_) => false,
                    },
                },
            }
        };
        if !ok {
            eprintln!("clipshelf: could not set the clipboard");
        }
        self.hide();
        if ok && self.config.auto_paste {
            let w = Rc::downgrade(self);
            glib::timeout_add_local_once(Duration::from_millis(self.config.paste_delay_ms), move || {
                if let Some(ui) = w.upgrade() {
                    ui.paster.paste();
                }
            });
        }
    }
}

/// True if the first letter with a strong direction is Hebrew/Arabic/etc.
fn starts_rtl(s: &str) -> bool {
    for c in s.chars() {
        let u = c as u32;
        if (0x0590..=0x08FF).contains(&u) || (0xFB1D..=0xFDFF).contains(&u) || (0xFE70..=0xFEFF).contains(&u) {
            return true;
        }
        if c.is_alphabetic() {
            return false;
        }
    }
    false
}
