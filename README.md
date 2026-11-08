# ClipShelf

**A simple clipboard history for Linux that works like Windows' Win+V.**

Press **Super+V**, see everything you copied — text *and* images — and click an item to paste it.
That's it. No tray icon to configure, no settings maze.

- 📋 Remembers the last 100 things you copied (text and images)
- 🖱️ Click (or Enter) pastes straight into the window you were working in
- 🔎 Just start typing to search
- 📌 Pin items you use often — they survive "Clear all" and never fall off the list
- 🧯 "Clear all" asks for confirmation first, so nothing disappears by a mis-click
- 🗑️ Delete single items (trash button or the Delete key)
- 🔐 Skips passwords copied from KeePassXC and other password managers
- 🖱️ Tray icon next to the keyboard-language indicator: click to open, right-click for more
- ⚙️ Simple Settings window (☰ button) — no config files needed
- 🌍 Interface in 13 languages (follows your system language, or pick one from the tray menu), with proper right-to-left support for Hebrew and Arabic
- 🪶 Small native GTK 4 app written in Rust (~1 MB), no background polling — it reacts to clipboard events

## Install

### Option A: ready-made release (no compiling)

1. Download the latest `clipshelf-…-linux-x86_64.tar.gz` from the [Releases](https://github.com/idanbarir/clipshelf/releases) page.
2. Extract it, open a terminal in the extracted folder, and run:

   ```bash
   ./install.sh
   ```

### Option B: build from source

```bash
sudo apt install build-essential pkg-config libgtk-4-dev git
curl --proto '=https' -sSf https://sh.rustup.rs | sh     # if you don't have Rust yet
git clone https://github.com/idanbarir/clipshelf.git
cd clipshelf
./install.sh
```

The installer (for the current user only, in `~/.local/bin`):

- installs the program and starts it automatically at login,
- sets up **Super+V** so that it works in every keyboard layout and on every desktop (see below),
- on Wayland, optionally asks for permission to paste automatically (see below).

On other desktops (KDE, XFCE, Cinnamon…), add a keyboard shortcut yourself that runs
`~/.local/bin/clipshelf --toggle`.

To remove it: `./uninstall.sh` (add `--purge` to also delete your history and settings).

## Using it

| Action | How |
|---|---|
| Open the history | **Super+V**, or click the ClipShelf icon in the system tray |
| Paste an item | Click it, or move with ↑/↓ and press **Enter** |
| Search | Just start typing |
| Pin / unpin | 📌 button |
| Delete an item | 🗑️ button, or **Delete** |
| Clear everything except pinned | red **Clear all** button in the title bar (asks first) |
| Close | **Esc**, Super+V again, or click anywhere else |
| Settings, language, quit | **☰** button in the window, or right-click the tray icon |

## How Super+V works

Desktop shortcut systems match keys through the *active keyboard layout*, and on some setups
(for example GNOME on Wayland with a Hebrew or Arabic layout active) letter shortcuts never fire —
Super+V just types ה or ر. So ClipShelf comes with a small **keyboard service** that works one level
lower, on the physical keys, like keyd or kmonad do:

- it passes every key through unchanged to a virtual keyboard, except Super+V, which opens ClipShelf;
- it works on any desktop (GNOME, KDE, Cinnamon, XFCE…), on X11 and Wayland, in every layout;
- it works alongside keyd and similar tools.

The installer asks before installing it (it needs your password once), then runs a **30-second trial**:
if you don't confirm that everything works, it switches itself off and nothing is enabled. In that case
the installer falls back to a regular desktop shortcut.

Safety: if the service ever fails, the kernel gives the keyboard straight back. Emergency stop at any
time: hold **Backspace + Escape + Enter**. Service status: `systemctl status clipshelf-hotkey`.

## Auto-paste on X11 and Wayland

After you choose an item, clipshelf puts it on the clipboard, closes, and presses Ctrl+V for you
in the window you came from.

- **X11 sessions** (e.g. "Zorin Desktop on Xorg"): works out of the box.
- **Wayland sessions**: Wayland doesn't let apps type into other apps, so clipshelf uses a virtual
  keyboard (`/dev/uinput`). The installer offers to grant this permission (one udev rule). If you
  say no, the item is still put on the clipboard — just press Ctrl+V yourself.

Terminals use Ctrl+Shift+V for pasting; if you mostly paste into a terminal, change `paste_keys`
(below).

## Settings

Click the **☰** button in the ClipShelf window (or right-click the tray icon) and choose **Settings**:

| Setting | Meaning |
|---|---|
| Language | Follow the system language, or pick one of 13 languages |
| Items to keep | How many items to remember (pinned items are always kept) |
| Paste automatically | Paste into the window you were working in as soon as you choose an item. Off = only copy it |
| Paste using | Ctrl+V for most programs, Ctrl+Shift+V for terminals, or Shift+Insert |
| Paste delay | Raise it if a paste sometimes doesn't arrive |
| Close when clicking elsewhere | Like Windows |
| Don't save passwords | Skip items copied from KeePassXC and other password managers |
| Show icon in the system tray | |

Changes are saved immediately. Language and tray-icon changes apply when you close the Settings window.

Advanced: everything is stored in `~/.config/clipshelf/config.json` (the Settings window has a button to open it).
The file also has `paste_command`, for using your own paste tool, e.g. `"ydotool key 29:1 47:1 47:0 29:0"`.

History is stored in `~/.local/share/clipshelf/` (`history.json` plus an `images/` folder) and
never leaves your computer.

## Command line

```
clipshelf               show the history popup
clipshelf --toggle      show it, or hide it if it's open (used by the shortcut)
clipshelf --background  start the background service without showing anything
clipshelf --quit        stop the background service
```

## How it works (for contributors)

- **One process.** The first `clipshelf` becomes the background service; later invocations just
  tell it to show the popup (GApplication single-instance over D-Bus).
- **Watching the clipboard**: `src/watcher.rs` listens for X11 XFixes selection-owner events.
  On Wayland sessions this goes through XWayland, which GNOME and KDE keep in sync with the
  Wayland clipboard. It asks the owner for its formats (`TARGETS`) first, so it can skip
  password-manager secrets and copied files, and only fetches images when there's no text.
- **Setting the clipboard**: [`arboard`](https://crates.io/crates/arboard) (the service stays
  alive, so it keeps owning the clipboard).
- **Pasting**: `src/paste.rs` — XTest on X11, a `/dev/uinput` virtual keyboard on Wayland.
- **Super+V service**: `src/hotkey.rs` — a root system service (`clipshelf --hotkey-daemon`) that grabs
  keyboards with EVIOCGRAB, forwards events to a uinput device and signals the app with SIGUSR1.
  The key-handling logic is unit-tested (`cargo test`).
- **UI**: GTK 4 (`src/main.rs`), so text shaping and right-to-left layout come from Pango.
- **Tray icon**: StatusNotifierItem via [`ksni`](https://crates.io/crates/ksni) (`src/tray.rs`). On plain GNOME it needs the AppIndicator extension (Zorin and Ubuntu ship it).

Contributions are welcome. **Translations:** each language is a short table of strings in `src/i18n.rs`. The current translations were written carefully but not all were reviewed by native speakers — if something sounds off in your language, please open an issue or a pull request.

## License

[MIT](LICENSE) — free to use, change and share.

---

## בעברית

‏**clipshelf** הוא מנהל לוח העתקה פשוט ללינוקס שמתנהג כמו Win+V בווינדוס: לוחצים **Super+V**,
רואים את כל מה שהעתקתם (טקסט ותמונות), ולחיצה על פריט מדביקה אותו בחלון שבו עבדתם.

‏התקנה: מורידים את הקובץ מעמוד ה־Releases, פורסים אותו, ומריצים בטרמינל מתוך התיקייה: `./install.sh`

‏אפשר לחפש פשוט על ידי הקלדה, לנעוץ פריטים (📌), למחוק פריט (🗑️ או Delete), ולסגור ב־Esc.
‏עברית וערבית מוצגות כראוי (מימין לשמאל), והממשק מוצג בעברית כשהמערכת בעברית.

## Troubleshooting

**Super+V types a letter (e.g. ה or ر) instead of opening ClipShelf.** The keyboard service is not
installed or not running. Re-run `./install.sh` and accept the keyboard service, or check
`systemctl status clipshelf-hotkey`.