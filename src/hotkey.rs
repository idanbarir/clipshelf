//! Layout-independent Super+V, on every desktop.
//!
//! Desktop shortcut systems match keys through the active keyboard layout, and
//! on some setups (e.g. GNOME on Wayland with Hebrew or Arabic active) letter
//! shortcuts simply never fire. This small system service works one level
//! lower, on physical keys, like keyd or kmonad do:
//!
//!  * it takes exclusive access to the keyboards (EVIOCGRAB),
//!  * forwards every event unchanged to one virtual keyboard (uinput),
//!  * except Super+V, which it swallows and turns into a signal (SIGUSR1)
//!    to the user's running ClipShelf.
//!
//! Safety:
//!  * if anything goes wrong (a write fails, the virtual keyboard can't be
//!    created) the process exits, and the kernel gives the keyboards back;
//!  * holding Backspace + Escape + Enter together exits immediately;
//!  * `--timeout N` runs for N seconds only (used for the trial run during
//!    installation);
//!  * keyboards are only taken over when no key is held down, so nothing
//!    gets stuck.

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ---- key codes (linux/input-event-codes.h) ----------------------------------
pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_REL: u16 = 0x02;
pub const EV_ABS: u16 = 0x03;
pub const EV_LED: u16 = 0x11;
pub const SYN_REPORT: u16 = 0;

pub const KEY_ESC: u16 = 1;
pub const KEY_BACKSPACE: u16 = 14;
pub const KEY_ENTER: u16 = 28;
pub const KEY_LEFTCTRL: u16 = 29;
pub const KEY_A: u16 = 30;
pub const KEY_LEFTSHIFT: u16 = 42;
pub const KEY_V: u16 = 47;
pub const KEY_RIGHTSHIFT: u16 = 54;
pub const KEY_LEFTALT: u16 = 56;
pub const KEY_SPACE: u16 = 57;
pub const KEY_RIGHTCTRL: u16 = 97;
pub const KEY_RIGHTALT: u16 = 100;
pub const KEY_LEFTMETA: u16 = 125;
pub const KEY_RIGHTMETA: u16 = 126;
/// Sent while Super is held so that releasing Super isn't seen as a lone
/// "Super tap" (which opens the Activities overview / app launcher).
pub const KEY_F24: u16 = 194;

const KEY_MAX: u16 = 0x2ff;
const BTN_FIRST: u16 = 0x100;
const BTN_LAST: u16 = 0x15f;
const LED_MAX: u16 = 0x0f;

const VIRTUAL_NAME: &str = "ClipShelf virtual keyboard";

// ---- the pure decision logic (unit-tested) ---------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ev {
    pub type_: u16,
    pub code: u16,
    pub value: i32,
}

impl Ev {
    pub const fn key(code: u16, value: i32) -> Self {
        Ev { type_: EV_KEY, code, value }
    }
    pub const fn syn() -> Self {
        Ev { type_: EV_SYN, code: SYN_REPORT, value: 0 }
    }
}

#[derive(Default)]
pub struct Filter {
    pressed: HashSet<u16>,
    /// The V of a Super+V is being swallowed until it's released.
    swallowing: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct Outcome {
    pub emit: Vec<Ev>,
    pub trigger: bool,
    pub emergency_exit: bool,
}

impl Filter {
    fn held(&self, a: u16, b: u16) -> bool {
        self.pressed.contains(&a) || self.pressed.contains(&b)
    }

    /// Decides what to do with one key event. `app_running` says whether
    /// there is a ClipShelf to open; if not, Super+V passes through untouched.
    pub fn key(&mut self, code: u16, value: i32, app_running: impl FnOnce() -> bool) -> Outcome {
        match value {
            1 => {
                self.pressed.insert(code);
            }
            0 => {
                self.pressed.remove(&code);
            }
            _ => {}
        }
        let emergency_exit = [KEY_BACKSPACE, KEY_ESC, KEY_ENTER]
            .iter()
            .all(|k| self.pressed.contains(k));

        let pass = Outcome { emit: vec![Ev::key(code, value)], trigger: false, emergency_exit };

        if code != KEY_V {
            return pass;
        }
        if self.swallowing {
            if value == 0 {
                self.swallowing = false;
            }
            return Outcome { emit: vec![], trigger: false, emergency_exit };
        }
        let super_only = self.held(KEY_LEFTMETA, KEY_RIGHTMETA)
            && !self.held(KEY_LEFTCTRL, KEY_RIGHTCTRL)
            && !self.held(KEY_LEFTALT, KEY_RIGHTALT)
            && !self.held(KEY_LEFTSHIFT, KEY_RIGHTSHIFT);
        if value == 1 && super_only && app_running() {
            self.swallowing = true;
            return Outcome {
                emit: vec![Ev::key(KEY_F24, 1), Ev::syn(), Ev::key(KEY_F24, 0), Ev::syn()],
                trigger: true,
                emergency_exit,
            };
        }
        pass
    }
}

// ---- ioctl plumbing --------------------------------------------------------

const fn ioc(dir: u64, ty: u8, nr: u8, size: usize) -> libc::c_ulong {
    ((dir << 30) | ((size as u64) << 16) | ((ty as u64) << 8) | nr as u64) as libc::c_ulong
}
const fn eviocgname(len: usize) -> libc::c_ulong {
    ioc(2, b'E', 0x06, len)
}
const fn eviocgbit(ev: u8, len: usize) -> libc::c_ulong {
    ioc(2, b'E', 0x20 + ev, len)
}
const fn eviocgkey(len: usize) -> libc::c_ulong {
    ioc(2, b'E', 0x18, len)
}
const EVIOCGRAB: libc::c_ulong = ioc(1, b'E', 0x90, 4);
const UI_SET_EVBIT: libc::c_ulong = ioc(1, b'U', 100, 4);
const UI_SET_KEYBIT: libc::c_ulong = ioc(1, b'U', 101, 4);
const UI_SET_LEDBIT: libc::c_ulong = ioc(1, b'U', 105, 4);
const UI_DEV_SETUP: libc::c_ulong = 0x405c_5503;
const UI_DEV_CREATE: libc::c_ulong = 0x5501;
const UI_DEV_DESTROY: libc::c_ulong = 0x5502;

#[repr(C)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}
const EV_SIZE: usize = std::mem::size_of::<InputEvent>();

#[repr(C)]
struct UinputSetup {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
    name: [u8; 80],
    ff_effects_max: u32,
}

fn check(r: libc::c_int) -> io::Result<()> {
    if r < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

fn write_ev(mut f: &File, e: Ev) -> io::Result<()> {
    let ev = InputEvent {
        time: libc::timeval { tv_sec: 0, tv_usec: 0 },
        type_: e.type_,
        code: e.code,
        value: e.value,
    };
    let bytes = unsafe { std::slice::from_raw_parts(&ev as *const InputEvent as *const u8, EV_SIZE) };
    f.write_all(bytes)
}

/// Reads every pending event (non-blocking fd).
fn read_evs(mut f: &File) -> io::Result<Vec<Ev>> {
    let mut out = Vec::new();
    let mut buf = [0u8; EV_SIZE * 64];
    loop {
        match f.read(&mut buf) {
            Ok(0) => return Err(io::Error::from_raw_os_error(libc::ENODEV)),
            Ok(n) => {
                for chunk in buf[..n].chunks_exact(EV_SIZE) {
                    let ev: InputEvent = unsafe { std::ptr::read_unaligned(chunk.as_ptr() as *const InputEvent) };
                    out.push(Ev { type_: ev.type_, code: ev.code, value: ev.value });
                }
                if n < buf.len() {
                    return Ok(out);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => return Ok(out),
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
}

fn bit(bits: &[u8], n: u16) -> bool {
    let n = n as usize;
    bits.get(n / 8).map(|b| b & (1 << (n % 8)) != 0).unwrap_or(false)
}

/// Is this a real keyboard we should take over? (pure, unit-tested)
pub fn is_keyboard(name: &str, ev_bits: &[u8], key_bits: &[u8]) -> bool {
    if name.to_lowercase().starts_with("clipshelf") {
        return false; // our own devices
    }
    bit(ev_bits, EV_KEY)
        && !bit(ev_bits, EV_REL) // keyboard+mouse combo devices are left alone
        && !bit(ev_bits, EV_ABS)
        && [KEY_A, KEY_V, KEY_SPACE, KEY_ENTER].iter().all(|k| bit(key_bits, *k))
}

// ---- devices ----------------------------------------------------------------

struct VirtualKeyboard {
    file: File,
}

impl VirtualKeyboard {
    fn new() -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")?;
        let fd = file.as_raw_fd();
        unsafe {
            check(libc::ioctl(fd, UI_SET_EVBIT as _, EV_KEY as libc::c_int))?;
            check(libc::ioctl(fd, UI_SET_EVBIT as _, EV_LED as libc::c_int))?;
            check(libc::ioctl(fd, UI_SET_EVBIT as _, EV_SYN as libc::c_int))?;
            // All keyboard keys, but no mouse/joystick buttons (those would
            // make the desktop treat it as a pointer device).
            for k in 1..=KEY_MAX {
                if !(BTN_FIRST..=BTN_LAST).contains(&k) {
                    check(libc::ioctl(fd, UI_SET_KEYBIT as _, k as libc::c_int))?;
                }
            }
            for l in 0..=LED_MAX.min(10) {
                check(libc::ioctl(fd, UI_SET_LEDBIT as _, l as libc::c_int))?;
            }
            let mut setup = UinputSetup {
                bustype: 0x06, // BUS_VIRTUAL
                vendor: 0x1d6b,
                product: 0xc15f,
                version: 1,
                name: [0; 80],
                ff_effects_max: 0,
            };
            setup.name[..VIRTUAL_NAME.len()].copy_from_slice(VIRTUAL_NAME.as_bytes());
            check(libc::ioctl(fd, UI_DEV_SETUP as _, &setup as *const UinputSetup))?;
            check(libc::ioctl(fd, UI_DEV_CREATE as _))?;
        }
        Ok(VirtualKeyboard { file })
    }
}

impl Drop for VirtualKeyboard {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY as _);
        }
    }
}

struct Keyboard {
    path: PathBuf,
    file: File,
    name: String,
}

impl Drop for Keyboard {
    fn drop(&mut self) {
        unsafe {
            libc::ioctl(self.file.as_raw_fd(), EVIOCGRAB as _, 0 as libc::c_int);
        }
    }
}

fn device_name(fd: i32) -> String {
    let mut buf = [0u8; 256];
    let n = unsafe { libc::ioctl(fd, eviocgname(buf.len()) as _, buf.as_mut_ptr()) };
    if n <= 0 {
        return String::new();
    }
    let s = &buf[..(n as usize).min(buf.len())];
    String::from_utf8_lossy(s.split(|b| *b == 0).next().unwrap_or(&[])).into_owned()
}

/// Opens and grabs a keyboard, if it is one and no key is currently held.
fn try_grab(path: &PathBuf) -> Option<Keyboard> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let fd = file.as_raw_fd();
    let mut ev_bits = [0u8; 4];
    let mut key_bits = [0u8; (KEY_MAX as usize + 8) / 8];
    unsafe {
        if libc::ioctl(fd, eviocgbit(0, ev_bits.len()) as _, ev_bits.as_mut_ptr()) < 0 {
            return None;
        }
        if libc::ioctl(fd, eviocgbit(EV_KEY as u8, key_bits.len()) as _, key_bits.as_mut_ptr()) < 0 {
            return None;
        }
    }
    let name = device_name(fd);
    if !is_keyboard(&name, &ev_bits, &key_bits) {
        return None;
    }
    // Never take over a keyboard while a key is down: its release would be
    // lost and the key would stay "stuck" for the desktop.
    let mut down = [0u8; (KEY_MAX as usize + 8) / 8];
    unsafe {
        if libc::ioctl(fd, eviocgkey(down.len()) as _, down.as_mut_ptr()) < 0 || down.iter().any(|b| *b != 0) {
            return None;
        }
        // Fails with EBUSY if another tool (e.g. keyd) already owns it; we then
        // see its keys through that tool's virtual keyboard instead.
        if libc::ioctl(fd, EVIOCGRAB as _, 1 as libc::c_int) < 0 {
            return None;
        }
    }
    Some(Keyboard { path: path.clone(), file, name })
}

// ---- telling ClipShelf --------------------------------------------------------

/// PIDs of running ClipShelf apps (not root's, not this service).
fn app_pids() -> Vec<i32> {
    let me = std::process::id() as i32;
    let mut pids = Vec::new();
    let Ok(rd) = fs::read_dir("/proc") else { return pids };
    for e in rd.flatten() {
        let Ok(pid) = e.file_name().to_string_lossy().parse::<i32>() else { continue };
        if pid == me {
            continue;
        }
        let comm = fs::read_to_string(e.path().join("comm")).unwrap_or_default();
        if comm.trim() != "clipshelf" {
            continue;
        }
        let status = fs::read_to_string(e.path().join("status")).unwrap_or_default();
        let uid = status
            .lines()
            .find(|l| l.starts_with("Uid:"))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|u| u.parse::<u32>().ok());
        if matches!(uid, Some(u) if u != 0) {
            pids.push(pid);
        }
    }
    pids
}

fn notify_app() {
    for pid in app_pids() {
        unsafe {
            libc::kill(pid, libc::SIGUSR1);
        }
    }
}

// ---- main loop ----------------------------------------------------------------

pub fn run_daemon(timeout: Option<Duration>) -> i32 {
    if unsafe { libc::geteuid() } != 0 {
        eprintln!("clipshelf: the keyboard-shortcut service must run as root (it is installed as a system service)");
        return 1;
    }
    let out = match VirtualKeyboard::new() {
        Ok(v) => v,
        Err(e) => {
            eprintln!("clipshelf-hotkey: cannot create the virtual keyboard: {e}");
            return 1;
        }
    };
    // Let the desktop notice the new device before keys arrive on it.
    std::thread::sleep(Duration::from_millis(300));

    let start = Instant::now();
    let mut keyboards: Vec<Keyboard> = Vec::new();
    let mut filter = Filter::default();
    let mut last_scan: Option<Instant> = None;

    loop {
        if let Some(t) = timeout {
            if start.elapsed() >= t {
                eprintln!("clipshelf-hotkey: trial time is over, releasing the keyboard");
                return 0;
            }
        }
        // (Re)scan for keyboards every 2 s: hot-plugging, resume from sleep…
        if last_scan.map(|t| t.elapsed() >= Duration::from_secs(2)).unwrap_or(true) {
            last_scan = Some(Instant::now());
            if let Ok(rd) = fs::read_dir("/dev/input") {
                let mut paths: Vec<PathBuf> = rd
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| p.file_name().map(|n| n.to_string_lossy().starts_with("event")).unwrap_or(false))
                    .collect();
                paths.sort();
                for p in paths {
                    if keyboards.iter().any(|k| k.path == p) {
                        continue;
                    }
                    if let Some(k) = try_grab(&p) {
                        eprintln!("clipshelf-hotkey: using {} ({})", k.name, k.path.display());
                        keyboards.push(k);
                    }
                }
            }
        }

        let mut fds: Vec<libc::pollfd> = keyboards
            .iter()
            .map(|k| libc::pollfd { fd: k.file.as_raw_fd(), events: libc::POLLIN, revents: 0 })
            .collect();
        fds.push(libc::pollfd { fd: out.file.as_raw_fd(), events: libc::POLLIN, revents: 0 });
        let r = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 500) };
        if r < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            eprintln!("clipshelf-hotkey: poll failed: {e}");
            return 1;
        }

        let mut gone = Vec::new();
        for (i, k) in keyboards.iter().enumerate() {
            let re = fds[i].revents;
            if re & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                gone.push(i);
                continue;
            }
            if re & libc::POLLIN == 0 {
                continue;
            }
            let evs = match read_evs(&k.file) {
                Ok(v) => v,
                Err(_) => {
                    gone.push(i);
                    continue;
                }
            };
            for ev in evs {
                let emit = match ev.type_ {
                    EV_KEY => {
                        let o = filter.key(ev.code, ev.value, || !app_pids().is_empty());
                        if o.emergency_exit {
                            eprintln!("clipshelf-hotkey: emergency exit (Backspace+Escape+Enter)");
                            return 0;
                        }
                        if o.trigger {
                            notify_app();
                        }
                        o.emit
                    }
                    EV_SYN => vec![ev],
                    _ => vec![], // MSC scancodes etc. aren't needed
                };
                for e in emit {
                    if let Err(err) = write_ev(&out.file, e) {
                        eprintln!("clipshelf-hotkey: cannot write to the virtual keyboard: {err}");
                        return 1; // keyboards are released on exit
                    }
                }
            }
        }
        for i in gone.into_iter().rev() {
            let k = keyboards.remove(i);
            eprintln!("clipshelf-hotkey: {} went away", k.name);
        }

        // Keyboard LEDs (Caps Lock, Num Lock): the desktop sets them on our
        // virtual keyboard; pass them on to the real ones.
        if fds.last().map(|f| f.revents & libc::POLLIN != 0).unwrap_or(false) {
            if let Ok(evs) = read_evs(&out.file) {
                for ev in evs.into_iter().filter(|e| e.type_ == EV_LED) {
                    for k in &keyboards {
                        let _ = write_ev(&k.file, ev);
                        let _ = write_ev(&k.file, Ev::syn());
                    }
                }
            }
        }
    }
}

// ---- tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn run(f: &mut Filter, seq: &[(u16, i32)], app: bool) -> (Vec<Ev>, usize) {
        let mut out = Vec::new();
        let mut triggers = 0;
        for (c, v) in seq {
            let o = f.key(*c, *v, || app);
            out.extend(o.emit);
            if o.trigger {
                triggers += 1;
            }
        }
        (out, triggers)
    }

    #[test]
    fn plain_typing_passes_through() {
        let mut f = Filter::default();
        let seq = [(KEY_A, 1), (KEY_A, 0), (KEY_V, 1), (KEY_V, 2), (KEY_V, 0)];
        let (out, t) = run(&mut f, &seq, true);
        assert_eq!(t, 0);
        assert_eq!(out, seq.iter().map(|(c, v)| Ev::key(*c, *v)).collect::<Vec<_>>());
    }

    #[test]
    fn super_v_is_swallowed_and_triggers() {
        let mut f = Filter::default();
        let (out, t) = run(
            &mut f,
            &[(KEY_LEFTMETA, 1), (KEY_V, 1), (KEY_V, 2), (KEY_V, 2), (KEY_V, 0), (KEY_LEFTMETA, 0)],
            true,
        );
        assert_eq!(t, 1);
        assert!(!out.iter().any(|e| e.code == KEY_V), "no V may reach the desktop: {out:?}");
        // Super press/release still pass, plus the F24 guard in between.
        assert_eq!(out.first(), Some(&Ev::key(KEY_LEFTMETA, 1)));
        assert_eq!(out.last(), Some(&Ev::key(KEY_LEFTMETA, 0)));
        assert!(out.contains(&Ev::key(KEY_F24, 1)) && out.contains(&Ev::key(KEY_F24, 0)));
    }

    #[test]
    fn right_super_works_too() {
        let mut f = Filter::default();
        let (_, t) = run(&mut f, &[(KEY_RIGHTMETA, 1), (KEY_V, 1), (KEY_V, 0), (KEY_RIGHTMETA, 0)], true);
        assert_eq!(t, 1);
    }

    #[test]
    fn super_released_before_v_still_swallows_v_release() {
        let mut f = Filter::default();
        let (out, t) = run(&mut f, &[(KEY_LEFTMETA, 1), (KEY_V, 1), (KEY_LEFTMETA, 0), (KEY_V, 0)], true);
        assert_eq!(t, 1);
        assert!(!out.iter().any(|e| e.code == KEY_V));
        // and the next V is normal again
        let (out, t) = run(&mut f, &[(KEY_V, 1), (KEY_V, 0)], true);
        assert_eq!(t, 0);
        assert_eq!(out, vec![Ev::key(KEY_V, 1), Ev::key(KEY_V, 0)]);
    }

    #[test]
    fn other_modifier_combos_pass_through() {
        for m in [KEY_LEFTCTRL, KEY_LEFTSHIFT, KEY_LEFTALT, KEY_RIGHTALT] {
            let mut f = Filter::default();
            let seq = [(KEY_LEFTMETA, 1), (m, 1), (KEY_V, 1), (KEY_V, 0), (m, 0), (KEY_LEFTMETA, 0)];
            let (out, t) = run(&mut f, &seq, true);
            assert_eq!(t, 0, "modifier {m}");
            assert_eq!(out.len(), seq.len());
        }
        // Ctrl+V (normal paste) is untouched
        let mut f = Filter::default();
        let (out, t) = run(&mut f, &[(KEY_LEFTCTRL, 1), (KEY_V, 1), (KEY_V, 0), (KEY_LEFTCTRL, 0)], true);
        assert_eq!(t, 0);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn without_running_app_super_v_passes_through() {
        let mut f = Filter::default();
        let seq = [(KEY_LEFTMETA, 1), (KEY_V, 1), (KEY_V, 0), (KEY_LEFTMETA, 0)];
        let (out, t) = run(&mut f, &seq, false);
        assert_eq!(t, 0);
        assert_eq!(out.len(), 4);
    }

    #[test]
    fn emergency_combo() {
        let mut f = Filter::default();
        assert!(!f.key(KEY_BACKSPACE, 1, || true).emergency_exit);
        assert!(!f.key(KEY_ESC, 1, || true).emergency_exit);
        assert!(f.key(KEY_ENTER, 1, || true).emergency_exit);
    }

    #[test]
    fn keyboard_detection() {
        let mut ev = [0u8; 4];
        let mut keys = [0u8; 96];
        let set = |b: &mut [u8], n: u16| b[n as usize / 8] |= 1 << (n % 8);
        set(&mut ev, EV_KEY);
        for k in [KEY_A, KEY_V, KEY_SPACE, KEY_ENTER] {
            set(&mut keys, k);
        }
        assert!(is_keyboard("AT Translated Set 2 keyboard", &ev, &keys));
        assert!(is_keyboard("keyd virtual keyboard", &ev, &keys));
        assert!(!is_keyboard("ClipShelf virtual keyboard", &ev, &keys));
        assert!(!is_keyboard("clipshelf virtual keyboard", &ev, &keys));
        // power button / lid switch: no letter keys
        assert!(!is_keyboard("Power Button", &ev, &[0u8; 96]));
        // keyboard+mouse combo
        let mut ev2 = ev;
        set(&mut ev2, EV_REL);
        assert!(!is_keyboard("Logitech combo", &ev2, &keys));
    }

    #[test]
    fn ioctl_numbers() {
        // values from <linux/input.h> / <linux/uinput.h> on x86_64
        assert_eq!(EVIOCGRAB, 0x4004_4590);
        assert_eq!(eviocgname(256), 0x8100_4506);
        assert_eq!(eviocgbit(0, 4), 0x8004_4520);
        assert_eq!(eviocgbit(1, 96), 0x8060_4521);
        assert_eq!(eviocgkey(96), 0x8060_4518);
        assert_eq!(UI_SET_EVBIT, 0x4004_5564);
        assert_eq!(UI_SET_KEYBIT, 0x4004_5565);
        assert_eq!(UI_SET_LEDBIT, 0x4004_5569);
        assert_eq!(EV_SIZE, 24);
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
    }
}
