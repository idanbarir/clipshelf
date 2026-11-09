//! clipshelf — a simple Windows-style (Super+V) clipboard history for Linux.
//!
//! One binary, one process: the first `clipshelf` started becomes the
//! background service (it watches the clipboard and owns the popup window).
//! Running `clipshelf` again — e.g. from the Super+V shortcut — just tells
//! that service to show the popup (GApplication single-instance over D-Bus).

mod config;
mod flyout;
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
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::mpsc;
use std::time::{Duration, Instant};

const APP_ID: &str = "io.github.idanbarir.ClipShelf";
const VERSION: &str = env!("CARGO_PKG_VERSION");

const CSS: &str = "
.clip-row { padding: 4px 4px 4px 10px; min-height: 50px; }
.clip-text { font-size: 0.95em; }
.clip-more { background-color: alpha(currentColor, 0.12); border-radius: 999px; padding: 4px;
             animation: clip-pulse 1.4s ease-in-out infinite; }
@keyframes clip-pulse { 0% { opacity: 0.9; margin-bottom: 8px; } 50% { opacity: 0.35; margin-bottom: 3px; } 100% { opacity: 0.9; margin-bottom: 8px; } }
.clip-meta { font-size: 0.85em; opacity: 0.65; }
.clip-search { margin: 8px; }
.clip-row button { min-height: 24px; min-width: 24px; padding: 2px; }
.clip-flyout { border: 1px solid alpha(currentColor, 0.25); border-radius: 10px; }
.clip-flyout headerbar { border-radius: 10px 10px 0 0; min-height: 38px; }
window.clip-cloak { background: transparent; border-color: transparent; box-shadow: none; }
window.clip-cloak > * { opacity: 0; }
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

    // Flyout mode needs the X11 backend (see src/flyout.rs).
    if config::Config::load().popup_style == "flyout" {
        flyout::use_x11_backend();
    }

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
                    "toggle" if ui.is_shown() => ui.hide(),
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
    config: RefCell<config::Config>,
    clipboard: RefCell<Option<arboard::Clipboard>>,
    paster: RefCell<paste::Paster>,
    ignore_passwords: Arc<AtomicBool>,
    settings_win: RefCell<Option<gtk::Window>>,
    /// Language or tray setting changed: restart when the settings window closes.
    restart_needed: Cell<bool>,
    was_active: Cell<bool>,
    /// Panel mode: the window is minimized instead of hidden, so it never has
    /// to be placed again (see show()/hide()).
    flyout: bool,
    /// The history is currently open.
    shown: Cell<bool>,
    /// A dialog of ours is open: don't hide the popup when it takes focus.
    dialog_open: Cell<bool>,
    /// The ☰ menu is open.
    menu_open: Cell<bool>,
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
            .icon_name(APP_ID)
            .build();

        let header = gtk::HeaderBar::new();
        // Red, labelled, and on the opposite side from the window's close button,
        // so it can't be mistaken for "close".
        let clear_btn = gtk::Button::with_label(t.clear_all);
        clear_btn.add_css_class("destructive-action");
        clear_btn.set_tooltip_text(Some(t.clear_all_tip));
        header.pack_start(&clear_btn);
        let menu = gio::Menu::new();
        menu.append(Some(t.settings), Some("win.settings"));
        menu.append(Some(t.about_menu), Some("win.about"));
        menu.append(Some(t.tray_quit), Some("win.quit"));
        let menu_btn = gtk::MenuButton::new();
        menu_btn.set_icon_name("open-menu-symbolic");
        menu_btn.set_menu_model(Some(&menu));
        menu_btn.set_tooltip_text(Some(t.settings));
        // ✕ for the panel (a regular window has its own title-bar buttons).
        let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("flat");
        close_btn.set_tooltip_text(Some(t.close));
        header.pack_end(&close_btn);
        header.pack_end(&menu_btn);
        let is_flyout = config.popup_style == "flyout" && flyout::setup(&win);
        if is_flyout {
            // Set up the window's drawing in advance (takes ~0.5 s the first
            // time), so the first Super+V opens instantly.
            // No window frame: the bar becomes part of the panel.
            header.set_show_title_buttons(false);
            win.set_default_size(380, 480);
        } else {
            close_btn.set_visible(false);
            win.set_titlebar(Some(&header));
        }

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

        // A gently pulsing arrow at the bottom while there are more items below.
        let more = gtk::Image::from_icon_name("go-down-symbolic");
        more.add_css_class("clip-more");
        more.set_halign(gtk::Align::Center);
        more.set_valign(gtk::Align::End);
        more.set_margin_bottom(8);
        more.set_can_target(false);
        more.set_visible(false);
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&scroller));
        overlay.add_overlay(&more);
        {
            let adj = scroller.vadjustment();
            let m = more.clone();
            let update = move |a: &gtk::Adjustment| {
                m.set_visible(a.value() + a.page_size() < a.upper() - 4.0);
            };
            let u2 = update.clone();
            adj.connect_value_changed(move |a| update(a));
            adj.connect_changed(move |a| u2(a));
        }

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        if is_flyout {
            vbox.append(&header);
        }
        vbox.append(&search);
        vbox.append(&overlay);
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
            paster: RefCell::new(paster),
            ignore_passwords: Arc::new(AtomicBool::new(config.ignore_passwords)),
            settings_win: RefCell::new(None),
            restart_needed: Cell::new(false),
            config: RefCell::new(config),
            was_active: Cell::new(false),
            flyout: is_flyout,
            shown: Cell::new(false),
            dialog_open: Cell::new(false),
            menu_open: Cell::new(false),
            hidden_at: Cell::new(None),
            t,
        });

        // --- signals -------------------------------------------------------
        {
            let w = Rc::downgrade(&ui);
            close_btn.connect_clicked(move |_| {
                if let Some(ui) = w.upgrade() {
                    ui.hide();
                }
            });
        }
        if ui.flyout {
            // On every map: make sure it is in its corner (normally it already
            // is, thanks to the position hint), then reveal it a few frames
            // later — or hide it again (the invisible setup run at startup).
            let w = Rc::downgrade(&ui);
            ui.win.connect_map(move |_| {
                let w = w.clone();
                glib::idle_add_local_once(move || {
                    let Some(ui) = w.upgrade() else { return };
                    flyout::place(&ui.win);
                    let w = Rc::downgrade(&ui);
                    glib::timeout_add_local_once(Duration::from_millis(40), move || {
                        let Some(ui) = w.upgrade() else { return };
                        if ui.shown.get() {
                            flyout::uncloak(&ui.win);
                        } else {
                            ui.win.set_visible(false);
                        }
                    });
                });
            });
            // Run it once at startup, invisibly, so the window manager knows
            // its position before the first Super+V.
            let w = Rc::downgrade(&ui);
            glib::idle_add_local_once(move || {
                if let Some(ui) = w.upgrade() {
                    if !ui.win.is_visible() {
                        flyout::cloak(&ui.win);
                        ui.win.present();
                    }
                }
            });
        }
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
                    return;
                }
                if !ui.is_shown() || !ui.config.borrow().hide_on_focus_loss {
                    return;
                }
                // Focus can flicker briefly (menus, dialogs): check again shortly.
                let w2 = Rc::downgrade(&ui);
                glib::timeout_add_local_once(Duration::from_millis(150), move || {
                    if let Some(ui) = w2.upgrade() {
                        if ui.is_shown()
                            && !ui.win.is_active()
                            && !ui.dialog_open.get()
                            && !ui.menu_open.get()
                        {
                            ui.hide();
                        }
                    }
                });
            });
        }

        {
            // The menu's popover takes focus from the window on some systems:
            // don't treat that as "clicked elsewhere".
            if let Some(pop) = menu_btn.popover() {
                let w = Rc::downgrade(&ui);
                pop.connect_visible_notify(move |p| {
                    if let Some(ui) = w.upgrade() {
                        ui.menu_open.set(p.is_visible());
                    }
                });
            }
        }
        {
            let a = gio::SimpleAction::new("settings", None);
            let w = Rc::downgrade(&ui);
            a.connect_activate(move |_, _| {
                if let Some(ui) = w.upgrade() {
                    ui.open_settings();
                }
            });
            ui.win.add_action(&a);
            let ab = gio::SimpleAction::new("about", None);
            let w = Rc::downgrade(&ui);
            ab.connect_activate(move |_, _| {
                if let Some(ui) = w.upgrade() {
                    ui.open_about();
                }
            });
            ui.win.add_action(&ab);
            let q = gio::SimpleAction::new("quit", None);
            let w = Rc::downgrade(&ui);
            q.connect_activate(move |_, _| {
                if let Some(ui) = w.upgrade() {
                    if let Some(app) = ui.win.application() {
                        app.quit();
                    }
                }
            });
            ui.win.add_action(&q);
        }

        // --- clipboard watcher ---------------------------------------------
        let (tx, rx) = mpsc::channel::<Entry>();
        watcher::spawn(dir, ui.ignore_passwords.clone(), tx);
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
                    if ui.is_shown() {
                        ui.refresh();
                    }
                }
                glib::ControlFlow::Continue
            });
        }

        // --- tray icon -------------------------------------------------------
        if ui.config.borrow().tray_icon {
            let (ttx, trx) = mpsc::channel::<tray::Cmd>();
            tray::spawn(ttx, t, ui.config.borrow().language.clone());
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
                if self.is_shown() {
                    self.hide();
                } else if !just_hidden {
                    if let Some(tok) = token {
                        self.win.set_startup_id(&tok);
                    }
                    self.show();
                }
            }
            tray::Cmd::Anchor(x, y) => flyout::set_anchor(x, y),
            tray::Cmd::ClearAll => {
                self.show();
                self.confirm_clear();
            }
            tray::Cmd::Settings => self.open_settings(),
            tray::Cmd::Language(code) => {
                let mut cfg = self.config.borrow().clone();
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

    // ---- settings window --------------------------------------------------

    fn open_settings(self: &Rc<Self>) {
        if let Some(w) = self.settings_win.borrow().as_ref() {
            w.present();
            return;
        }
        self.hide();
        let t = self.t;
        let cfg = self.config.borrow().clone();

        let win = gtk::Window::builder()
            .title(format!("{} · ClipShelf", t.settings))
            .default_width(520)
            .resizable(false)
            .icon_name("edit-paste")
            .build();
        if let Some(app) = self.win.application() {
            win.set_application(Some(&app));
        }

        let grid = gtk::Grid::new();
        grid.set_row_spacing(18);
        grid.set_column_spacing(24);
        grid.set_margin_top(20);
        grid.set_margin_bottom(20);
        grid.set_margin_start(24);
        grid.set_margin_end(24);
        let row = Cell::new(0);
        let add = |title: &str, hint: Option<&str>, widget: &gtk::Widget| -> gtk::Box {
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
            texts.set_hexpand(true);
            let l = gtk::Label::new(Some(title));
            l.set_xalign(0.0);
            l.set_wrap(true);
            texts.append(&l);
            if let Some(h) = hint {
                let hl = gtk::Label::new(Some(h));
                hl.set_xalign(0.0);
                hl.set_wrap(true);
                hl.set_max_width_chars(42);
                hl.add_css_class("clip-meta");
                texts.append(&hl);
            }
            widget.set_valign(gtk::Align::Center);
            widget.set_halign(gtk::Align::End);
            grid.attach(&texts, 0, row.get(), 1, 1);
            grid.attach(widget, 1, row.get(), 1, 1);
            row.set(row.get() + 1);
            texts
        };

        // Language
        let mut names: Vec<&str> = vec![t.tray_auto];
        names.extend(i18n::LANGS.iter().map(|(_, n, _)| *n));
        let lang = gtk::DropDown::from_strings(&names);
        let sel = i18n::LANGS
            .iter()
            .position(|(c, _, _)| *c == cfg.language)
            .map(|i| i + 1)
            .unwrap_or(0);
        lang.set_selected(sel as u32);
        add(t.tray_language, None, lang.upcast_ref());

        // Items to keep
        let max = gtk::SpinButton::with_range(10.0, 1000.0, 10.0);
        max.set_value(cfg.max_items as f64);
        add(t.s_max_items, Some(t.s_max_items_hint), max.upcast_ref());

        // Auto-paste + how
        let auto = gtk::Switch::new();
        auto.set_active(cfg.auto_paste);
        add(t.s_auto_paste, Some(t.s_auto_paste_hint), auto.upcast_ref());

        let keys = gtk::DropDown::from_strings(&[t.s_keys_default, t.s_keys_terminal, "Shift+Insert"]);
        keys.set_selected(match cfg.paste_keys.replace(' ', "").to_lowercase().as_str() {
            "ctrl+shift+v" | "shift+ctrl+v" => 1,
            "shift+insert" => 2,
            _ => 0,
        });
        let keys_row = add(t.s_paste_keys, None, keys.upcast_ref());

        let delay = gtk::SpinButton::with_range(50.0, 2000.0, 50.0);
        delay.set_value(cfg.paste_delay_ms as f64);
        let delay_row = add(t.s_delay, Some(t.s_delay_hint), delay.upcast_ref());

        // Others
        let focus = gtk::Switch::new();
        focus.set_active(cfg.hide_on_focus_loss);
        add(t.s_hide_focus, None, focus.upcast_ref());

        let pw = gtk::Switch::new();
        pw.set_active(cfg.ignore_passwords);
        add(t.s_passwords, Some(t.s_passwords_hint), pw.upcast_ref());

        let fly_sw = gtk::Switch::new();
        fly_sw.set_active(cfg.popup_style == "flyout");
        add(t.s_flyout, None, fly_sw.upcast_ref());

        let tray_sw = gtk::Switch::new();
        tray_sw.set_active(cfg.tray_icon);
        add(t.s_tray, None, tray_sw.upcast_ref());

        // auto-paste options only matter when auto-paste is on
        let set_sens = {
            let (keys, delay, kr, dr) = (keys.clone(), delay.clone(), keys_row.clone(), delay_row.clone());
            move |on: bool| {
                for w in [keys.upcast_ref::<gtk::Widget>(), delay.upcast_ref(), kr.upcast_ref(), dr.upcast_ref()] {
                    w.set_sensitive(on);
                }
            }
        };
        set_sens(cfg.auto_paste);

        // --- live updates ---
        let w = Rc::downgrade(self);
        let edit = move |f: &dyn Fn(&Rc<Ui>, &mut config::Config)| {
            if let Some(ui) = w.upgrade() {
                let mut c = ui.config.borrow().clone();
                f(&ui, &mut c);
                c.save();
                *ui.config.borrow_mut() = c;
            }
        };
        let edit = Rc::new(edit);
        {
            let e = edit.clone();
            lang.connect_selected_notify(move |d| {
                let i = d.selected() as usize;
                e(&|ui, c| {
                    c.language = if i == 0 { "auto".into() } else { i18n::LANGS[i - 1].0.into() };
                    ui.restart_needed.set(true);
                });
            });
        }
        {
            let e = edit.clone();
            max.connect_value_changed(move |s| {
                let n = s.value() as usize;
                e(&|ui, c| {
                    c.max_items = n;
                    ui.history.borrow_mut().set_max(n);
                    ui.history.borrow().save();
                });
            });
        }
        {
            let e = edit.clone();
            auto.connect_active_notify(move |s| {
                let on = s.is_active();
                set_sens(on);
                e(&|_, c| c.auto_paste = on);
            });
        }
        {
            let e = edit.clone();
            keys.connect_selected_notify(move |d| {
                let k = ["ctrl+v", "ctrl+shift+v", "shift+insert"][(d.selected() as usize).min(2)];
                e(&|ui, c| {
                    c.paste_keys = k.into();
                    *ui.paster.borrow_mut() = paste::Paster::new(k, c.paste_command.clone());
                });
            });
        }
        {
            let e = edit.clone();
            delay.connect_value_changed(move |s| {
                let ms = s.value() as u64;
                e(&|_, c| c.paste_delay_ms = ms);
            });
        }
        {
            let e = edit.clone();
            focus.connect_active_notify(move |s| {
                let on = s.is_active();
                e(&|_, c| c.hide_on_focus_loss = on);
            });
        }
        {
            let e = edit.clone();
            pw.connect_active_notify(move |s| {
                let on = s.is_active();
                e(&|ui, c| {
                    c.ignore_passwords = on;
                    ui.ignore_passwords.store(on, Ordering::Relaxed);
                });
            });
        }
        {
            let e = edit.clone();
            fly_sw.connect_active_notify(move |s| {
                let on = s.is_active();
                e(&|ui, c| {
                    c.popup_style = if on { "flyout".into() } else { "window".into() };
                    ui.restart_needed.set(true);
                });
            });
        }
        {
            let e = edit.clone();
            tray_sw.connect_active_notify(move |s| {
                let on = s.is_active();
                e(&|ui, c| {
                    c.tray_icon = on;
                    ui.restart_needed.set(true);
                });
            });
        }

        let note = gtk::Label::new(Some(t.s_apply_note));
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.add_css_class("clip-meta");

        let file_btn = gtk::Button::with_label(t.tray_settings);
        file_btn.add_css_class("flat");
        file_btn.set_halign(gtk::Align::Start);
        file_btn.connect_clicked(|_| open_config_file());

        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
        vbox.append(&grid);
        let foot = gtk::Box::new(gtk::Orientation::Vertical, 6);
        foot.set_margin_start(24);
        foot.set_margin_end(24);
        foot.set_margin_bottom(16);
        foot.append(&note);
        foot.append(&file_btn);
        vbox.append(&foot);
        win.set_child(Some(&vbox));

        let w = Rc::downgrade(self);
        win.connect_close_request(move |_| {
            if let Some(ui) = w.upgrade() {
                ui.settings_win.borrow_mut().take();
                if ui.restart_needed.replace(false) {
                    ui.restart();
                }
            }
            glib::Propagation::Proceed
        });
        let key = gtk::EventControllerKey::new();
        let ww = win.clone();
        key.connect_key_pressed(move |_, k, _, _| {
            if k == gdk::Key::Escape {
                ww.close();
                return glib::Propagation::Stop;
            }
            glib::Propagation::Proceed
        });
        win.add_controller(key);

        *self.settings_win.borrow_mut() = Some(win.clone());
        win.present();
    }

    fn show(self: &Rc<Self>) {
        self.search.set_text("");
        self.refresh();
        self.shown.set(true);
        if self.flyout {
            // Invisible until confirmed in its corner (map handler).
            flyout::cloak(&self.win);
        }
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

    fn is_shown(&self) -> bool {
        if self.flyout {
            self.shown.get()
        } else {
            self.win.is_visible()
        }
    }

    fn hide(&self) {
        if !self.is_shown() {
            return;
        }
        self.hidden_at.set(Some(Instant::now()));
        self.was_active.set(false);
        self.shown.set(false);
        if self.flyout {
            flyout::cloak(&self.win);
        }
        self.win.set_visible(false);
    }

    fn open_about(&self) {
        self.hide();
        let about = gtk::AboutDialog::builder()
            .program_name("ClipShelf")
            .logo_icon_name(APP_ID)
            .version(VERSION)
            .comments(self.t.about_comment)
            .copyright("© 2026 Idan Barir")
            .license_type(gtk::License::MitX11)
            .website("https://github.com/idanbarir/clipshelf")
            .website_label("github.com/idanbarir/clipshelf")
            .authors(vec!["Idan Barir".to_string()])
            .build();
        about.add_credit_section("Built with", &["Claude (Anthropic)", "GTK 4 & gtk-rs", "Rust"]);
        if let Some(app) = self.win.application() {
            about.set_application(Some(&app));
        }
        about.present();
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

        // Compact rows of (almost) equal height, like Windows: at most two
        // lines of text, small image thumbnails.
        let (content, haystack): (gtk::Widget, Option<String>) = match e.kind {
            Kind::Text => {
                let full = e.text.as_deref().unwrap_or("");
                let preview: String = full.trim().chars().take(300).collect();
                // Line breaks shown as ⏎ so long items stay within two lines.
                let preview = preview
                    .replace('\t', "    ")
                    .lines()
                    .map(str::trim_end)
                    .filter(|l| !l.is_empty())
                    .collect::<Vec<_>>()
                    .join("  ⏎  ");
                let label = gtk::Label::new(Some(&preview));
                label.set_xalign(0.0);
                label.set_wrap(true);
                label.set_wrap_mode(pango::WrapMode::WordChar);
                label.set_lines(2);
                label.set_ellipsize(pango::EllipsizeMode::End);
                label.set_hexpand(true);
                label.set_valign(gtk::Align::Center);
                label.set_width_chars(10);
                label.set_max_width_chars(60);
                label.add_css_class("clip-text");
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
                let hb = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                hb.set_hexpand(true);
                let thumb = e.thumb_path(&dir);
                let path = if thumb.exists() { thumb } else { e.image_path(&dir) };
                let pic = gtk::Picture::for_filename(&path);
                pic.set_can_shrink(true);
                pic.set_valign(gtk::Align::Center);
                let scale = f64::min(110.0 / e.width as f64, 44.0 / e.height as f64).min(1.0);
                pic.set_size_request(
                    ((e.width as f64 * scale) as i32).max(16),
                    ((e.height as f64 * scale) as i32).max(16),
                );
                let meta = gtk::Label::new(Some(&format!("{}\n{} × {}", self.t.image, e.width, e.height)));
                meta.set_xalign(0.0);
                meta.set_valign(gtk::Align::Center);
                meta.add_css_class("clip-meta");
                hb.append(&pic);
                hb.append(&meta);
                (hb.upcast(), None)
            }
        };

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        buttons.set_valign(gtk::Align::Center);
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
        let (auto_paste, delay) = {
            let c = self.config.borrow();
            (c.auto_paste, c.paste_delay_ms)
        };
        if ok && auto_paste {
            let w = Rc::downgrade(self);
            glib::timeout_add_local_once(Duration::from_millis(delay), move || {
                if let Some(ui) = w.upgrade() {
                    ui.paster.borrow().paste();
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

/// Opens config.json in the user's text editor (for advanced options).
fn open_config_file() {
    let path = history::config_dir().join("config.json");
    if !path.exists() {
        config::Config::default().save();
    }
    let file = gio::File::for_path(&path);
    let ok = gio::AppInfo::default_for_type("text/plain", false)
        .map(|app| app.launch(&[file.clone()], None::<&gio::AppLaunchContext>).is_ok())
        .unwrap_or(false);
    if !ok {
        let _ = gio::AppInfo::launch_default_for_uri(&file.uri(), None::<&gio::AppLaunchContext>);
    }
}
