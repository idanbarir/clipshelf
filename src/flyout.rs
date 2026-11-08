//! "Flyout" mode: the history opens as a small borderless panel in the corner
//! next to the system tray, not shown in the taskbar — like Windows' Win+V.
//!
//! Wayland doesn't let applications position their windows or hide them from
//! the taskbar, so in this mode ClipShelf's windows use the X11 backend
//! (XWayland on Wayland sessions), where both are possible.

use gdk4_x11::X11Surface;
use gtk::prelude::*;
use gtk::{gdk, glib};
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask,
};

use std::cell::Cell;

const MARGIN: i32 = 10;

thread_local! {
    /// Last known position of the tray icon (from a click on it).
    static ANCHOR: Cell<Option<(i32, i32)>> = Cell::new(load_anchor());
}

fn anchor_file() -> std::path::PathBuf {
    crate::history::data_dir().join("tray-position")
}

fn load_anchor() -> Option<(i32, i32)> {
    let s = std::fs::read_to_string(anchor_file()).ok()?;
    let mut it = s.split_whitespace().map(|v| v.parse::<i32>());
    match (it.next(), it.next()) {
        (Some(Ok(x)), Some(Ok(y))) => Some((x, y)),
        _ => None,
    }
}

/// Remembers where the tray icon is, so the panel opens right above it —
/// also when opened with the keyboard shortcut, and after a restart.
pub fn set_anchor(x: i32, y: i32) {
    if ANCHOR.with(|a| a.get()) != Some((x, y)) {
        ANCHOR.with(|a| a.set(Some((x, y))));
        let _ = std::fs::write(anchor_file(), format!("{x} {y}\n"));
    }
}

/// Call before GTK starts.
pub fn use_x11_backend() {
    if std::env::var_os("DISPLAY").is_some() && std::env::var_os("GDK_BACKEND").is_none() {
        std::env::set_var("GDK_BACKEND", "x11");
    }
}

fn xsurface(win: &gtk::ApplicationWindow) -> Option<X11Surface> {
    win.surface()?.downcast::<X11Surface>().ok()
}

/// Turns the main window into a flyout. Returns false if not running on X11.
pub fn setup(win: &gtk::ApplicationWindow) -> bool {
    if !gdk::Display::default()
        .map(|d| d.type_().name().contains("X11"))
        .unwrap_or(false)
    {
        return false;
    }
    win.set_decorated(false);
    win.set_resizable(false);
    win.add_css_class("clip-flyout");
    win.connect_realize(|w| {
        if let Some(s) = xsurface(w) {
            s.set_skip_taskbar_hint(true);
            s.set_skip_pager_hint(true);
        }
    });
    // Appear already in place: invisible until moved to the corner.
    win.connect_map(|w| {
        let w2 = w.clone();
        glib::idle_add_local_once(move || {
            place(&w2);
            let w3 = w2.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(30), move || w3.set_opacity(1.0));
        });
    });
    true
}

fn intern(conn: &impl Connection, name: &str) -> Option<u32> {
    Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
}

/// Moves the window to the corner of the current monitor next to the panel
/// (bottom-right for a bottom taskbar, top-right for a top bar) and keeps it on top.
fn place(win: &gtk::ApplicationWindow) -> Option<()> {
    let xid = xsurface(win)?.xid() as u32;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;

    // The tray icon's monitor if we know where it is, else the one under the
    // mouse pointer.
    let anchor = ANCHOR.with(|a| a.get());
    let ptr = conn.query_pointer(root).ok()?.reply().ok()?;
    let (px, py) = anchor.unwrap_or((ptr.root_x as i32, ptr.root_y as i32));
    let display = WidgetExt::display(win);
    let monitors = display.monitors();
    let mut geos = Vec::new();
    for i in 0..monitors.n_items() {
        if let Some(m) = monitors.item(i).and_then(|o| o.downcast::<gdk::Monitor>().ok()) {
            geos.push(m.geometry());
        }
    }
    let mon = geos
        .iter()
        .find(|g| px >= g.x() && px < g.x() + g.width() && py >= g.y() && py < g.y() + g.height())
        .or(geos.first())
        .copied()?;

    // Work area = screen minus panels (set by the window manager).
    let (mut wa_y, mut wa_bottom) = (mon.y(), mon.y() + mon.height());
    if let Some(atom) = intern(&conn, "_NET_WORKAREA") {
        if let Ok(Ok(r)) = conn.get_property(false, root, atom, AtomEnum::CARDINAL, 0, 4).map(|c| c.reply()) {
            if let Some(v) = r.value32().map(|it| it.collect::<Vec<u32>>()) {
                if v.len() == 4 {
                    wa_y = v[1] as i32;
                    wa_bottom = (v[1] + v[3]) as i32;
                }
            }
        }
    }
    let mon_bottom = mon.y() + mon.height();
    let bottom_panel = if wa_bottom > mon.y() && wa_bottom < mon_bottom { mon_bottom - wa_bottom } else { 0 };
    let top_panel = if wa_y > mon.y() && wa_y < mon_bottom { wa_y - mon.y() } else { 0 };

    // The panel has a fixed size; the allocation may not be final yet.
    let (dw, dh) = win.default_size();
    let (w, h) = (win.width().max(dw).max(200), win.height().max(dh).max(200));
    let right = mon.x() + mon.width() - w - MARGIN;
    let x = match anchor {
        // centred over the tray icon, kept on screen
        Some((ax, ay)) if ax >= mon.x() && ax < mon.x() + mon.width() && ay >= mon.y() && ay < mon_bottom => {
            (ax - w / 2).clamp(mon.x() + MARGIN, right)
        }
        _ => right,
    };
    let y = if top_panel > bottom_panel {
        mon.y() + top_panel + MARGIN
    } else {
        mon_bottom - bottom_panel - h - MARGIN
    };
    conn.configure_window(xid, &ConfigureWindowAux::new().x(x).y(y)).ok()?;

    // Keep above other windows (_NET_WM_STATE_ADD, _NET_WM_STATE_ABOVE).
    if let (Some(state), Some(above)) = (intern(&conn, "_NET_WM_STATE"), intern(&conn, "_NET_WM_STATE_ABOVE")) {
        let ev = ClientMessageEvent::new(32, xid, state, [1, above, 0, 1, 0]);
        let _ = conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
    }
    // Ask for keyboard focus explicitly (the panel was opened on purpose,
    // by the user's shortcut or tray click) and raise it.
    let _ = conn.configure_window(xid, &ConfigureWindowAux::new().stack_mode(x11rb::protocol::xproto::StackMode::ABOVE));
    let _ = conn.set_input_focus(x11rb::protocol::xproto::InputFocus::PARENT, xid, x11rb::CURRENT_TIME);
    if let Some(active) = intern(&conn, "_NET_ACTIVE_WINDOW") {
        // source indication 2 = "pager/user request", which window managers honour
        let ev = ClientMessageEvent::new(32, xid, active, [2, 0, 0, 0, 0]);
        let _ = conn.send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            ev,
        );
    }
    conn.flush().ok()?;
    Some(())
}
