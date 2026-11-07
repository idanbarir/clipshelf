//! Background thread that watches the clipboard.
//!
//! It uses the X11 XFixes extension to get an event every time the CLIPBOARD
//! selection changes owner (no polling). On GNOME/KDE Wayland sessions this goes
//! through XWayland, which mirrors the Wayland clipboard, so it works there too.

use crate::history::{self, Entry, Kind};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};
use x11rb::connection::Connection;
use x11rb::protocol::xfixes::{ConnectionExt as _, SelectionEvent, SelectionEventMask};
use x11rb::protocol::xproto::{AtomEnum, ConnectionExt as _, CreateWindowAux, WindowClass};
use x11rb::protocol::Event;
use x11rb::rust_connection::RustConnection;

/// Texts larger than this are not stored (1 MiB).
const MAX_TEXT: usize = 1 << 20;
const THUMB_W: u32 = 360;
const THUMB_H: u32 = 140;

pub fn spawn(dir: PathBuf, ignore_passwords: bool, tx: Sender<Entry>) {
    thread::Builder::new()
        .name("clipboard-watcher".into())
        .spawn(move || loop {
            if let Err(e) = run(&dir, ignore_passwords, &tx) {
                eprintln!("clipshelf: clipboard watcher error: {e}; retrying");
            }
            thread::sleep(Duration::from_secs(2));
        })
        .expect("spawn watcher thread");
}

struct Atoms {
    clipboard: u32,
    targets: u32,
    prop: u32,
}

fn intern(conn: &RustConnection, name: &str) -> Result<u32, Box<dyn Error>> {
    Ok(conn.intern_atom(false, name.as_bytes())?.reply()?.atom)
}

fn run(dir: &Path, ignore_passwords: bool, tx: &Sender<Entry>) -> Result<(), Box<dyn Error>> {
    let (conn, screen_num) = x11rb::connect(None)?;
    let root = conn.setup().roots[screen_num].root;
    let win = conn.generate_id()?;
    conn.create_window(
        x11rb::COPY_DEPTH_FROM_PARENT,
        win,
        root,
        0,
        0,
        1,
        1,
        0,
        WindowClass::INPUT_OUTPUT,
        0,
        &CreateWindowAux::new(),
    )?;
    conn.xfixes_query_version(5, 0)?.reply()?;
    let atoms = Atoms {
        clipboard: intern(&conn, "CLIPBOARD")?,
        targets: intern(&conn, "TARGETS")?,
        prop: intern(&conn, "CLIPSHELF_TARGETS")?,
    };
    conn.xfixes_select_selection_input(win, atoms.clipboard, SelectionEventMask::SET_SELECTION_OWNER)?;
    conn.flush()?;

    let mut cb = arboard::Clipboard::new()?;
    let mut last: Option<String> = None;

    // Whatever is on the clipboard right now.
    capture(&conn, win, &atoms, &mut cb, dir, ignore_passwords, &mut last, tx);

    loop {
        let ev = conn.wait_for_event()?;
        if let Event::XfixesSelectionNotify(n) = ev {
            if n.subtype == SelectionEvent::SET_SELECTION_OWNER && n.owner != x11rb::NONE {
                // Give the new owner a moment to be ready to answer requests.
                thread::sleep(Duration::from_millis(50));
                capture(&conn, win, &atoms, &mut cb, dir, ignore_passwords, &mut last, tx);
            }
        }
    }
}

/// Asks the clipboard owner which formats it offers.
fn targets(conn: &RustConnection, win: u32, a: &Atoms) -> Result<Vec<String>, Box<dyn Error>> {
    conn.convert_selection(win, a.clipboard, a.targets, a.prop, x11rb::CURRENT_TIME)?;
    conn.flush()?;
    let deadline = Instant::now() + Duration::from_millis(800);
    loop {
        match conn.poll_for_event()? {
            Some(Event::SelectionNotify(n)) if n.requestor == win && n.selection == a.clipboard => {
                if n.property == x11rb::NONE {
                    return Ok(Vec::new());
                }
                break;
            }
            Some(_) => {}
            None => {
                if Instant::now() > deadline {
                    return Ok(Vec::new());
                }
                thread::sleep(Duration::from_millis(5));
            }
        }
    }
    let reply = conn
        .get_property(true, win, a.prop, AtomEnum::ATOM, 0, 4096)?
        .reply()?;
    let atoms: Vec<u32> = reply.value32().map(|it| it.collect()).unwrap_or_default();
    let cookies: Vec<_> = atoms
        .iter()
        .filter_map(|at| conn.get_atom_name(*at).ok())
        .collect();
    Ok(cookies
        .into_iter()
        .filter_map(|c| c.reply().ok())
        .map(|r| String::from_utf8_lossy(&r.name).into_owned())
        .collect())
}

#[allow(clippy::too_many_arguments)]
fn capture(
    conn: &RustConnection,
    win: u32,
    atoms: &Atoms,
    cb: &mut arboard::Clipboard,
    dir: &Path,
    ignore_passwords: bool,
    last: &mut Option<String>,
    tx: &Sender<Entry>,
) {
    let names = targets(conn, win, atoms).unwrap_or_default();
    let has = |n: &str| names.iter().any(|x| x == n);

    // Password managers (KeePassXC, KDE) mark secrets with this target.
    if ignore_passwords && has("x-kde-passwordManagerHint") {
        return;
    }
    // Copied files in a file manager: not useful as history (Windows skips them too).
    if has("x-special/gnome-copied-files") || has("application/x-kde-cutselection") {
        return;
    }

    let has_text = names.is_empty()
        || names
            .iter()
            .any(|n| n == "UTF8_STRING" || n == "STRING" || n == "TEXT" || n.starts_with("text/plain"));
    let has_image = names.iter().any(|n| n.starts_with("image/"));

    let entry = if has_text {
        match cb.get_text() {
            Ok(t) if !t.trim().is_empty() && t.len() <= MAX_TEXT => {
                let id = history::hash(&[b"t", t.as_bytes()]);
                Some(Entry {
                    id,
                    kind: Kind::Text,
                    text: Some(t),
                    width: 0,
                    height: 0,
                    pinned: false,
                    time: history::now(),
                })
            }
            _ => None,
        }
    } else if has_image {
        cb.get_image().ok().and_then(|img| image_entry(dir, img))
    } else {
        None
    };

    if let Some(e) = entry {
        if last.as_deref() == Some(e.id.as_str()) {
            return;
        }
        *last = Some(e.id.clone());
        let _ = tx.send(e);
    }
}

fn image_entry(dir: &Path, img: arboard::ImageData) -> Option<Entry> {
    let (w, h) = (img.width as u32, img.height as u32);
    if w == 0 || h == 0 {
        return None;
    }
    let id = history::hash(&[b"i", &w.to_le_bytes(), &h.to_le_bytes(), &img.bytes]);
    let path = history::image_path(dir, &id);
    if !path.exists() {
        let rgba = image::RgbaImage::from_raw(w, h, img.bytes.into_owned())?;
        rgba.save(&path).ok()?;
        let scale = f64::min(THUMB_W as f64 / w as f64, THUMB_H as f64 / h as f64).min(1.0);
        let (tw, th) = (((w as f64 * scale) as u32).max(1), ((h as f64 * scale) as u32).max(1));
        let thumb = image::imageops::thumbnail(&rgba, tw, th);
        let _ = thumb.save(history::thumb_path(dir, &id));
    }
    Some(Entry {
        id,
        kind: Kind::Image,
        text: None,
        width: w,
        height: h,
        pinned: false,
        time: history::now(),
    })
}
