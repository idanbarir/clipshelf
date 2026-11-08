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
- 🌍 Proper right-to-left support: Hebrew and Arabic are shown correctly, and the UI is in English or Hebrew
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
- sets the **Super+V** shortcut on GNOME-based desktops (Zorin OS, Ubuntu, Pop!_OS, Fedora Workstation…).
  GNOME normally uses Super+V for the notification list; that stays available on **Super+M**.
  The shortcut also works while a Hebrew, Arabic or Russian keyboard layout is active,
- offers to pin ClipShelf to the taskbar/dock, so you can also open it with a click,
- on Wayland, optionally asks for permission to paste automatically (see below).

On other desktops (KDE, XFCE, Cinnamon…), add a keyboard shortcut yourself that runs
`~/.local/bin/clipshelf --toggle`.

To remove it: `./uninstall.sh` (add `--purge` to also delete your history and settings).

## Using it

| Action | How |
|---|---|
| Open the history | **Super+V**, or click the ClipShelf icon on the taskbar |
| Paste an item | Click it, or move with ↑/↓ and press **Enter** |
| Search | Just start typing |
| Pin / unpin | 📌 button |
| Delete an item | 🗑️ button, or **Delete** |
| Clear everything except pinned | red **Clear all** button in the title bar (asks first) |
| Close | **Esc**, Super+V again, or click anywhere else |

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

`~/.config/clipshelf/config.json` is created on first run:

```json
{
  "max_items": 100,
  "auto_paste": true,
  "paste_keys": "ctrl+v",
  "paste_delay_ms": 200,
  "paste_command": null,
  "hide_on_focus_loss": true,
  "ignore_passwords": true,
  "language": "auto"
}
```

| Setting | Meaning |
|---|---|
| `max_items` | How many (unpinned) items to keep |
| `auto_paste` | `false` = only copy the item, don't press Ctrl+V |
| `paste_keys` | `"ctrl+v"`, `"ctrl+shift+v"` or `"shift+insert"` |
| `paste_delay_ms` | Wait before pasting, so focus returns to your window. Raise it if pastes get lost |
| `paste_command` | Use your own command to paste, e.g. `"ydotool key 29:1 47:1 47:0 29:0"` |
| `hide_on_focus_loss` | Close the popup when you click elsewhere |
| `ignore_passwords` | Don't record items that password managers mark as secret |
| `language` | `"auto"` follows your system language (English if it isn't translated yet), or force `"en"` / `"he"` |

Restart after editing: `clipshelf --quit && clipshelf --background &`

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

Contributions and translations are welcome — a translation is just a table of 12 short strings in `src/i18n.rs`.

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

**Super+V types a letter (e.g. ה) instead of opening ClipShelf.** GNOME matches custom shortcuts by
the letter a key produces in the *current* layout. The installer registers Super+V for Latin, Hebrew,
Arabic and Russian layouts; for another layout, add a shortcut in Settings → Keyboard that runs
`~/.local/bin/clipshelf --toggle` while that layout is active.
