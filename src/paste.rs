//! Sending the paste keystroke (Ctrl+V) to the window that had focus.
//!
//! Methods, in order:
//!  1. a user-defined command (config `paste_command`)
//!  2. X11 session: XTest (no setup needed)
//!  3. a virtual keyboard through /dev/uinput (works on Wayland; needs the
//!     udev rule installed by install.sh)
//!  4. XTest anyway (reaches XWayland apps on Wayland)

use std::error::Error;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::process::Command;
use std::thread;
use std::time::Duration;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{self, ConnectionExt as _};
use x11rb::protocol::xtest::ConnectionExt as _;

#[derive(Clone, Copy, Debug)]
enum K {
    Ctrl,
    Shift,
    V,
    Insert,
}

impl K {
    fn evdev(self) -> u16 {
        match self {
            K::Ctrl => 29,    // KEY_LEFTCTRL
            K::Shift => 42,   // KEY_LEFTSHIFT
            K::V => 47,       // KEY_V
            K::Insert => 110, // KEY_INSERT
        }
    }
    fn keysym(self) -> u32 {
        match self {
            K::Ctrl => 0xffe3,   // Control_L
            K::Shift => 0xffe1,  // Shift_L
            K::V => 0x0076,      // v
            K::Insert => 0xff63, // Insert
        }
    }
}

fn parse_keys(s: &str) -> Vec<K> {
    match s.trim().to_lowercase().replace(' ', "").as_str() {
        "ctrl+shift+v" | "shift+ctrl+v" => vec![K::Ctrl, K::Shift, K::V],
        "shift+insert" => vec![K::Shift, K::Insert],
        _ => vec![K::Ctrl, K::V],
    }
}

pub struct Paster {
    keys: Vec<K>,
    command: Option<String>,
    wayland: bool,
    uinput: Option<Uinput>,
}

impl Paster {
    pub fn new(keys: &str, command: Option<String>) -> Self {
        let wayland = std::env::var("XDG_SESSION_TYPE").map(|v| v == "wayland").unwrap_or(false)
            || std::env::var_os("WAYLAND_DISPLAY").is_some();
        // Create the virtual keyboard up front: the compositor needs a moment
        // to pick up a new input device before it accepts events from it.
        let uinput = if wayland { Uinput::new().ok() } else { None };
        Paster {
            keys: parse_keys(keys),
            command: command.filter(|c| !c.trim().is_empty()),
            wayland,
            uinput,
        }
    }

    pub fn paste(&self) {
        if let Some(cmd) = &self.command {
            if let Err(e) = Command::new("sh").arg("-c").arg(cmd).status() {
                eprintln!("clipshelf: paste_command failed: {e}");
            }
            return;
        }
        if !self.wayland && xtest(&self.keys).is_ok() {
            return;
        }
        if let Some(u) = &self.uinput {
            if u.combo(&self.keys).is_ok() {
                return;
            }
        }
        if let Err(e) = xtest(&self.keys) {
            eprintln!("clipshelf: could not send paste keys: {e}");
        }
    }
}

fn xtest(keys: &[K]) -> Result<(), Box<dyn Error>> {
    let (conn, screen) = x11rb::connect(None)?;
    let setup = conn.setup();
    let root = setup.roots[screen].root;
    let (min, max) = (setup.min_keycode, setup.max_keycode);
    let map = conn.get_keyboard_mapping(min, max - min + 1)?.reply()?;
    let per = map.keysyms_per_keycode.max(1) as usize;
    let mut codes = Vec::new();
    for k in keys {
        let pos = map
            .keysyms
            .chunks(per)
            .position(|c| c.contains(&k.keysym()))
            .ok_or("keysym not in keymap")?;
        codes.push(min + pos as u8);
    }
    for c in &codes {
        conn.xtest_fake_input(xproto::KEY_PRESS_EVENT, *c, x11rb::CURRENT_TIME, root, 0, 0, 0)?;
    }
    for c in codes.iter().rev() {
        conn.xtest_fake_input(xproto::KEY_RELEASE_EVENT, *c, x11rb::CURRENT_TIME, root, 0, 0, 0)?;
    }
    conn.get_input_focus()?.reply()?; // round-trip so the events are processed
    Ok(())
}

// ---- /dev/uinput virtual keyboard -------------------------------------------

const UI_SET_EVBIT: libc::c_ulong = 0x4004_5564;
const UI_SET_KEYBIT: libc::c_ulong = 0x4004_5565;
const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;
const EV_SYN: u16 = 0;
const EV_KEY: u16 = 1;
const BUS_VIRTUAL: u16 = 0x06;

#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}

struct Uinput {
    file: File,
}

impl Uinput {
    fn new() -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        let check = |r: libc::c_int| {
            if r < 0 {
                Err(std::io::Error::last_os_error())
            } else {
                Ok(())
            }
        };
        unsafe {
            check(libc::ioctl(fd, UI_SET_EVBIT as _, EV_KEY as libc::c_int))?;
            for k in [K::Ctrl, K::Shift, K::V, K::Insert] {
                check(libc::ioctl(fd, UI_SET_KEYBIT as _, k.evdev() as libc::c_int))?;
            }
            let mut setup = UinputSetup {
                id: InputId { bustype: BUS_VIRTUAL, vendor: 0x1d6b, product: 0xc11b, version: 1 },
                name: [0; 80],
                ff_effects_max: 0,
            };
            let name = b"clipshelf virtual keyboard";
            setup.name[..name.len()].copy_from_slice(name);
            check(libc::ioctl(fd, UI_DEV_SETUP as _, &setup as *const UinputSetup))?;
            check(libc::ioctl(fd, UI_DEV_CREATE as _))?;
        }
        Ok(Uinput { file })
    }

    fn emit(&self, type_: u16, code: u16, value: i32) -> std::io::Result<()> {
        let ev = InputEvent {
            time: libc::timeval { tv_sec: 0, tv_usec: 0 },
            type_,
            code,
            value,
        };
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &ev as *const InputEvent as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };
        (&self.file).write_all(bytes)
    }

    fn combo(&self, keys: &[K]) -> std::io::Result<()> {
        for k in keys {
            self.emit(EV_KEY, k.evdev(), 1)?;
            self.emit(EV_SYN, 0, 0)?;
            thread::sleep(Duration::from_millis(8));
        }
        for k in keys.iter().rev() {
            self.emit(EV_KEY, k.evdev(), 0)?;
            self.emit(EV_SYN, 0, 0)?;
            thread::sleep(Duration::from_millis(8));
        }
        Ok(())
    }
}

impl Drop for Uinput {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY as _);
        }
    }
}
