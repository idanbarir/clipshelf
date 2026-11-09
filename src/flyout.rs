//! "Flyout" mode: the history opens as a small borderless panel in the corner
//! next to the system tray, not shown in the taskbar — like Windows' Win+V.
//!
//! Wayland doesn't let applications position their windows or hide them from
//! the taskbar, so in this mode ClipShelf's windows use the X11 backend
//! (XWayland on Wayland sessions), where both are possible.

use gdk4_x11::X11Surface;
use gtk::prelude::*;
use gtk::gdk;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{
    AtomEnum, ClientMessageEvent, ConfigureWindowAux, ConnectionExt as _, EventMask,
};

use std::cell::Cell;

const MARGIN: i32 = 10;

thread_local! {
    /// Last known position of the tray icon (from a click on it).
    static ANCHOR: Cell<Option<(i32, i32)>> = Cell::new(load_anchor());
    /// Where the user dragged the panel (top-left corner), remembered.
    static SAVED: Cell<Option<(i32, i32)>> = Cell::new(load_pos(&saved_file()));
    /// Where ClipShelf last put it.
    static LAST_PLACED: Cell<Option<(i32, i32)>> = const { Cell::new(None) };
}

fn saved_file() -> std::path::PathBuf {
    crate::history::data_dir().join("panel-position")
}

fn load_pos(path: &std::path::Path) -> Option<(i32, i32)> {
    let s = std::fs::read_to_string(path).ok()?;
    let mut it = s.split_whitespace().map(|v| v.parse::<i32>());
    match (it.next(), it.next()) {
        (Some(Ok(x)), Some(Ok(y))) => Some((x, y)),
        _ => None,
    }
}

fn anchor_file() -> std::path::PathBuf {
    crate::history::data_dir().join("tray-position")
}

fn load_anchor() -> Option<(i32, i32)> {
    load_pos(&anchor_file())
}

/// Remembers where the tray icon is, so the panel opens right above it —
/// also when opened with the keyboard shortcut, and after a restart.
pub fn set_anchor(x: i32, y: i32) {
    dbg(&format!("anchor {x},{y}"));
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
fn dbg(what: &str) {
    if std::env::var_os("CLIPSHELF_DEBUG").is_some() {
        thread_local!(static T0: std::time::Instant = std::time::Instant::now());
        T0.with(|t| eprintln!("[flyout {:>5} ms] {what}", t.elapsed().as_millis()));
    }
}

/// Makes the panel fully transparent (drawn by GTK itself, so it works with
/// every window manager).
pub fn cloak(win: &gtk::ApplicationWindow) {
    dbg("cloak");
    win.add_css_class("clip-cloak");
}

pub fn uncloak(win: &gtk::ApplicationWindow) {
    dbg("uncloak");
    win.remove_css_class("clip-cloak");
}

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
            // A utility window (like a tool palette): GNOME Shell doesn't play
            // its window-open animation for these (only for normal windows and
            // dialogs, js/ui/windowManager.js), and Mutter places them exactly
            // at their declared position. Tiling extensions leave them alone.
            // GTK writes _NET_WM_WINDOW_TYPE_NORMAL when it creates the
            // surface, but Xlib buffers it: flush GTK's queue first so our
            // value can't be overwritten by a late NORMAL.
            gtk::prelude::WidgetExt::display(w).sync();
            set_utility_type(s.xid() as u32);
        }
    });
    true
}

fn intern(conn: &impl Connection, name: &str) -> Option<u32> {
    Some(conn.intern_atom(false, name.as_bytes()).ok()?.reply().ok()?.atom)
}

/// Where the panel should be: where the user last dragged it, else the
/// corner next to the panel/system tray (bottom-right for a bottom taskbar,
/// top-right for a top bar) of the monitor under the pointer.
fn target(win: &gtk::ApplicationWindow, conn: &impl Connection, root: u32) -> Option<(i32, i32)> {
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
    // A position the user chose by dragging wins, if it is still on a screen.
    if let Some((sx, sy)) = SAVED.with(|s| s.get()) {
        if let Some(g) = geos.iter().find(|g| sx >= g.x() - w / 2 && sx < g.x() + g.width() - w / 2 && sy >= g.y() && sy < g.y() + g.height() - 40) {
            let cx = sx.clamp(g.x(), g.x() + g.width() - w);
            let cy = sy.clamp(g.y(), g.y() + g.height() - h);
            return Some((cx, cy));
        }
    }
    Some((x, y))
}

/// Declares (x, y) as the window's own position (USPosition/PPosition in
/// WM_NORMAL_HINTS) and moves it there. Window managers honour a declared
/// position when the window is mapped — Mutter even before "center new
/// windows" (mutter src/core/place.c) — so it appears right there.
fn set_position(conn: &impl Connection, xid: u32, x: i32, y: i32) -> Option<()> {
    if let (Some(hints_atom), Some(size_hints)) = (intern(conn, "WM_NORMAL_HINTS"), intern(conn, "WM_SIZE_HINTS")) {
        let mut v: Vec<u32> = conn
            .get_property(false, xid, hints_atom, size_hints, 0, 18)
            .ok()
            .and_then(|c| c.reply().ok())
            .and_then(|r| r.value32().map(|it| it.collect()))
            .unwrap_or_default();
        v.resize(18, 0);
        v[0] |= 1 | 4; // USPosition | PPosition
        v[1] = x as u32;
        v[2] = y as u32;
        let _ = x11rb::wrapper::ConnectionExt::change_property32(
            conn,
            x11rb::protocol::xproto::PropMode::REPLACE,
            xid,
            hints_atom,
            size_hints,
            &v,
        );
    }
    conn.configure_window(xid, &ConfigureWindowAux::new().x(x).y(y)).ok()?;
    LAST_PLACED.with(|l| l.set(Some((x, y))));
    conn.flush().ok()
}

/// _NET_WM_WINDOW_TYPE = UTILITY (see setup()).
fn set_utility_type(xid: u32) -> Option<()> {
    let (conn, _) = x11rb::connect(None).ok()?;
    let (ty, util) = (intern(&conn, "_NET_WM_WINDOW_TYPE")?, intern(&conn, "_NET_WM_WINDOW_TYPE_UTILITY")?);
    x11rb::wrapper::ConnectionExt::change_property32(
        &conn,
        x11rb::protocol::xproto::PropMode::REPLACE,
        xid,
        ty,
        AtomEnum::ATOM,
        &[util],
    )
    .ok()?;
    // Round trip: the property is on the server before GTK maps the window.
    let r = conn.get_input_focus().ok()?.reply().ok().map(|_| ());
    r
}

/// Call BEFORE showing the (hidden) panel: puts the hidden window where it
/// must appear, so the window manager maps it there directly.
pub fn preposition(win: &gtk::ApplicationWindow) -> Option<()> {
    dbg("preposition");
    let xid = xsurface(win)?.xid() as u32;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let (x, y) = target(win, &conn, root)?;
    gtk::prelude::WidgetExt::display(win).sync();
    set_utility_type(xid);
    set_position(&conn, xid, x, y)
}

/// Call before hiding: if the user dragged the panel somewhere else,
/// remember that spot for next time.
pub fn remember(win: &gtk::ApplicationWindow) -> Option<()> {
    let xid = xsurface(win)?.xid() as u32;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let t = conn.translate_coordinates(xid, root, 0, 0).ok()?.reply().ok()?;
    let (x, y) = (t.dst_x as i32, t.dst_y as i32);
    let placed = LAST_PLACED.with(|l| l.get())?;
    if (x - placed.0).abs() > 8 || (y - placed.1).abs() > 8 {
        dbg(&format!("user moved the panel to {x},{y}"));
        SAVED.with(|s| s.set(Some((x, y))));
        LAST_PLACED.with(|l| l.set(Some((x, y))));
        let _ = std::fs::write(saved_file(), format!("{x} {y}\n"));
    }
    Some(())
}

/// Call after the panel is mapped: makes sure it is in place (waiting until
/// the window manager confirms), keeps it on top and gives it the focus.
pub fn place(win: &gtk::ApplicationWindow) -> Option<()> {
    dbg("place");
    let xid = xsurface(win)?.xid() as u32;
    let (conn, screen) = x11rb::connect(None).ok()?;
    let root = conn.setup().roots[screen].root;
    let (x, y) = target(win, &conn, root)?;
    set_position(&conn, xid, x, y)?;
    // Wait (max ~150 ms) until the window manager has actually moved it.
    let start = std::time::Instant::now();
    while start.elapsed() < std::time::Duration::from_millis(150) {
        if let Ok(Ok(t)) = conn.translate_coordinates(xid, root, 0, 0).map(|c| c.reply()) {
            if (t.dst_x as i32 - x).abs() <= 2 && (t.dst_y as i32 - y).abs() <= 2 {
                dbg("moved");
                break;
            }
            dbg(&format!("waiting: at {},{} want {},{}", t.dst_x, t.dst_y, x, y));
        }
        std::thread::sleep(std::time::Duration::from_millis(4));
    }

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
