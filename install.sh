#!/usr/bin/env bash
# clipshelf installer — installs for the current user only (no root needed,
# except for the optional Wayland auto-paste permission).
set -euo pipefail
cd "$(dirname "$0")"

BIN_DIR="$HOME/.local/bin"
APP_DIR="$HOME/.local/share/applications"
AUTOSTART_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/autostart"
EXE="$BIN_DIR/clipshelf"

say() { printf '\033[1m%s\033[0m\n' "$*"; }

# 1. Binary: use the prebuilt one from a release archive, or build from source.
if [ -x ./clipshelf ] && [ ! -d ./src ]; then
    BIN=./clipshelf
else
    if ! command -v cargo >/dev/null; then
        echo "Rust is not installed. Either download a ready-made release from GitHub,"
        echo "or install Rust:  curl --proto '=https' -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
    if ! pkg-config --exists gtk4 2>/dev/null; then
        echo "Missing build dependencies. Run:"
        echo "  sudo apt install build-essential pkg-config libgtk-4-dev"
        exit 1
    fi
    say "Building clipshelf…"
    cargo build --release
    BIN=target/release/clipshelf
fi

# Stop a running copy before replacing it.
if [ -x "$EXE" ]; then "$EXE" --quit >/dev/null 2>&1 || true; sleep 0.3; fi

say "Installing to $EXE"
install -Dm755 "$BIN" "$EXE"

# 2. Menu entry + autostart at login.
mkdir -p "$APP_DIR" "$AUTOSTART_DIR"
sed "s|^Exec=.*|Exec=$EXE|" data/clipshelf.desktop > "$APP_DIR/io.github.clipshelf.Clipshelf.desktop"
update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
sed "s|^Exec=.*|Exec=$EXE --background|" data/clipshelf-autostart.desktop > "$AUTOSTART_DIR/clipshelf.desktop"

# 3. Super+V keyboard shortcut (GNOME, Zorin, Ubuntu, Pop!_OS…).
MK=org.gnome.settings-daemon.plugins.media-keys
KB_BASE=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings
SCHEMAS=$(gsettings list-schemas 2>/dev/null || true)

# add_shortcut <id> <binding>
add_shortcut() {
    local path="$KB_BASE/$1/" current new
    current=$(gsettings get $MK custom-keybindings)
    new=$(python3 - "$current" "$path" <<'PY'
import ast, sys
cur = sys.argv[1].strip()
if cur.startswith('@as'):
    cur = cur[3:].strip()
lst = ast.literal_eval(cur) if cur else []
if sys.argv[2] not in lst:
    lst.append(sys.argv[2])
print(str(lst))
PY
)
    gsettings set $MK custom-keybindings "$new"
    local S="$MK.custom-keybinding:$path"
    gsettings set "$S" name "ClipShelf ($2)"
    gsettings set "$S" command "$EXE --toggle"
    gsettings set "$S" binding "$2"
}

if command -v gsettings >/dev/null && grep -qx "$MK" <<<"$SCHEMAS"; then
    say "Setting the Super+V shortcut"
    add_shortcut clipshelf '<Super>v'
    # GNOME matches custom shortcuts by the letter the key types in the
    # *current* layout, so with a Hebrew or Arabic layout active Super+V
    # types ה / ر instead. Register the same physical key for those layouts too.
    add_shortcut clipshelf-he '<Super>hebrew_he'
    add_shortcut clipshelf-ar '<Super>Arabic_ra'
    add_shortcut clipshelf-ru '<Super>Cyrillic_em'

    # GNOME uses Super+V for the notification list by default; keep Super+M for that.
    if grep -qx org.gnome.shell.keybindings <<<"$SCHEMAS"; then
        tray=$(gsettings get org.gnome.shell.keybindings toggle-message-tray)
        newtray=$(python3 - "$tray" <<'PY'
import ast, sys
cur = sys.argv[1].strip()
if cur.startswith('@as'):
    cur = cur[3:].strip()
lst = [k for k in (ast.literal_eval(cur) if cur else []) if k.lower() != '<super>v']
print(str(lst) if lst else "['<Super>m']")
PY
)
        gsettings set org.gnome.shell.keybindings toggle-message-tray "$newtray"
    fi
else
    echo "Could not set a shortcut automatically on this desktop."
    echo "Add one yourself in your keyboard settings that runs:  $EXE --toggle"
fi

# 4. Optional: auto-paste on Wayland needs permission to create a virtual keyboard.
if [ "${XDG_SESSION_TYPE:-}" = "wayland" ] && [ ! -f /etc/udev/rules.d/70-clipshelf-uinput.rules ]; then
    echo
    say "You are on a Wayland session."
    echo "To paste automatically into the window you were using (like Windows does),"
    echo "clipshelf needs permission to act as a virtual keyboard (/dev/uinput)."
    echo "Without it, choosing an item copies it and you press Ctrl+V yourself."
    read -rp "Allow it? This asks for your password once. [Y/n] " ans || ans=n
    if [[ ! "$ans" =~ ^[Nn] ]]; then
        echo 'KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput"' \
            | sudo tee /etc/udev/rules.d/70-clipshelf-uinput.rules >/dev/null
        echo uinput | sudo tee /etc/modules-load.d/clipshelf-uinput.conf >/dev/null
        sudo modprobe uinput || true
        sudo udevadm control --reload-rules
        sudo udevadm trigger --sysname-match=uinput || true
        echo "Done. (If auto-paste doesn't work right away, log out and back in.)"
    fi
fi

# 5. Start it now.
nohup "$EXE" --background >/dev/null 2>&1 &
disown || true

echo
say "ClipShelf is installed and running. Press Super+V, or click its icon next to the keyboard-language indicator."
