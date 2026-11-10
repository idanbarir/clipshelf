# ClipShelf

**A simple clipboard history for Linux that works like Windows' Win+V.**

Press **Super+V**, see everything you copied — text *and* images — and click an item to paste it.
That's it. No tray icon to configure, no settings maze.

- 📋 Remembers the last 100 things you copied (text and images)
- 🖱️ Click (or Enter) pastes straight into the window you were working in
- 🔎 Just start typing to search
- 📜 Compact list (about six items at a glance), with a pulsing arrow when there are more below
- 📌 Pin items you use often — they survive "Clear all" and never fall off the list
- 🧯 "Clear all" asks for confirmation first, so nothing disappears by a mis-click
- 🗑️ Delete single items (trash button or the Delete key)
- 🔐 Skips passwords copied from KeePassXC and other password managers
- 🪟 Opens as a small panel in the corner by the system tray, not in the taskbar — like Windows.
  Drag it by its title to any spot (e.g. right above the tray icon) and it opens there from then on
- 🖱️ Tray icon next to the keyboard-language indicator: click to open, right-click for more
- ⚙️ All settings in the ☰ menu — no config files needed
- 🌍 Interface in 13 languages (follows your system language, or pick one from the tray menu), with proper right-to-left support for Hebrew and Arabic
- 🪶 Small native GTK 4 app written in Rust (~1 MB), no background polling — it reacts to clipboard events

## Install

### Ubuntu, Zorin OS, Linux Mint, Pop!_OS, Debian (recommended)

1. Download `clipshelf_…_amd64.deb` from the [Releases](https://github.com/idanbarir/clipshelf/releases) page.
2. Double-click it to install it with your software center, or run
   `sudo apt install ./clipshelf_*_amd64.deb` in the folder you downloaded it to.
3. Open **ClipShelf** once from your applications menu.

> Upgrading from an older version that was installed with `install.sh`? Run
> `~/.local/bin/clipshelf --quit; /usr/bin/clipshelf --background &` once after
> installing the package. ClipShelf then removes the old copy from your home
> folder and points the shortcuts at the new one. Your history and settings are kept.

The first time it runs, ClipShelf sets itself up for your user: it starts
automatically at every login and registers the **Super+V** shortcut (and
**Ctrl+Super+V** as a backup) on GNOME-based desktops. GNOME normally uses
Super+V for the notification list; that moves to **Super+M**. The package also
lets ClipShelf paste for you on Wayland (it allows the logged-in user to create
a temporary virtual keyboard through `/dev/uinput`).

### Other distributions: ready-made archive

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
- sets the **Super+V** shortcut on GNOME-based desktops (Zorin OS, Ubuntu, Pop!_OS, Fedora Workstation…).
  GNOME normally uses Super+V for the notification list; that stays available on **Super+M**,
- on Wayland, optionally asks for permission to paste automatically (see below).

On other desktops (KDE, XFCE, Cinnamon…), add a keyboard shortcut yourself that runs
`~/.local/bin/clipshelf --toggle`.

To remove it: `./uninstall.sh` (add `--purge` to also delete your history and settings).

## Using it

| Action | How |
|---|---|
| Open the history | **Super+V** (backup: **Ctrl+Super+V**, see Troubleshooting), or click the tray icon |
| Paste an item | Click it, or move with ↑/↓ and press **Enter** |
| Search | Just start typing |
| Pin / unpin | 📌 button |
| Delete an item | 🗑️ button, or **Delete** |
| Clear everything except pinned | red **Clear all** button in the title bar (asks first) |
| Close | **Esc**, **✕**, Super+V again, or click anywhere else |
| Settings, About, language, quit | **☰** button, or right-click the tray icon |

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

Click **☰ → Settings** in the ClipShelf panel (or right-click the tray icon → Settings…):

| Setting | Meaning |
|---|---|
| Language | Follow the system language, or pick one of 13 languages |
| Items to keep | 25 – 500 (pinned items are always kept) |
| Paste automatically | Paste into the window you were working in as soon as you choose an item. Off = only copy it |
| Paste using | Ctrl+V for most programs, Ctrl+Shift+V for terminals, or Shift+Insert |
| Close when clicking elsewhere | Like Windows |
| Don't save passwords | Skip items copied from KeePassXC and other password managers |
| Small panel next to the system tray | On (default): a borderless panel, not shown in the taskbar. Off: a regular window |
| Show icon in the system tray | |

Changes apply immediately (language, panel and tray-icon changes restart ClipShelf in the background).

Advanced: everything is stored in `~/.config/clipshelf/config.json` (☰ → Settings → Open settings file), which also has `paste_delay_ms`.
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
- **UI**: GTK 4 (`src/main.rs`), so text shaping and right-to-left layout come from Pango.
- **Panel mode**: Wayland doesn't let apps position windows or hide them from the taskbar, so in panel
  mode the window uses the X11 backend (XWayland), where it can (`src/flyout.rs`). With fractional display
  scaling on GNOME 46 it may look slightly less sharp; switch the setting off for a regular window.
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

**Super+V types a letter (e.g. ה, ر or м) instead of opening ClipShelf — only while typing in a text
field with a non-Latin layout.** This is a bug in IBus (the input-method service GNOME uses): before
version 1.5.31 it treats Super+letter as text to type, so *no* Super+letter shortcut works there
(GNOME's own Super+A neither). It was fixed upstream in May 2024
([ibus commit 48c4a9c](https://github.com/ibus/ibus/commit/48c4a9c900421b9e77be399a4792974f25a80be6)),
but Ubuntu 24.04 and distributions based on it (Zorin OS 18, Linux Mint 22, Pop!_OS 24.04…) still ship
IBus 1.5.29. Until they update, use the backup shortcut **Ctrl+Super+V**, which IBus lets through, or
click the tray icon. Check your version with `ibus version`.