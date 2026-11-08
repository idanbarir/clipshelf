#!/usr/bin/env bash
# ClipShelf installer. The app itself is installed for the current user only.
# Your password is asked for (once) only for the parts that need the system:
# the Super+V keyboard service and auto-paste on Wayland.
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
# (older versions used a different app ID)
rm -f "$APP_DIR/io.github.clipshelf.Clipshelf.desktop"
sed "s|^Exec=.*|Exec=$EXE|" data/clipshelf.desktop > "$APP_DIR/io.github.idanbarir.ClipShelf.desktop"
update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
sed "s|^Exec=.*|Exec=$EXE --background|" data/clipshelf-autostart.desktop > "$AUTOSTART_DIR/clipshelf.desktop"

# Start the app now (the shortcut trial below needs it running).
nohup "$EXE" --background >/dev/null 2>&1 &
disown || true

# 3. Super+V.
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

# remove_shortcut <id>
remove_shortcut() {
    local path="$KB_BASE/$1/" current new
    current=$(gsettings get $MK custom-keybindings)
    new=$(python3 - "$current" "$path" <<'PY'
import ast, sys
cur = sys.argv[1].strip()
if cur.startswith('@as'):
    cur = cur[3:].strip()
lst = [p for p in (ast.literal_eval(cur) if cur else []) if p != sys.argv[2]]
print(str(lst) if lst else "@as []")
PY
)
    gsettings set $MK custom-keybindings "$new"
    gsettings reset-recursively "$MK.custom-keybinding:$path" 2>/dev/null || true
}

HAS_GNOME_KEYS=0
if command -v gsettings >/dev/null && grep -qx "$MK" <<<"$SCHEMAS"; then HAS_GNOME_KEYS=1; fi

# Shortcuts added by older versions are replaced by what follows.
if [ $HAS_GNOME_KEYS = 1 ]; then
    for id in clipshelf clipshelf-he clipshelf-ar clipshelf-ru; do remove_shortcut $id; done
fi

# 3a. The keyboard-shortcut service: Super+V on physical keys, so it works
#     in every keyboard layout and on every desktop (see src/hotkey.rs).
HOTKEY_BIN=/usr/local/libexec/clipshelf/clipshelf-hotkey
SERVICE=/etc/systemd/system/clipshelf-hotkey.service
QUIRKS=/etc/libinput/local-overrides.quirks
HOTKEY_OK=0

install_service_files() {
    sudo install -Dm755 -o root -g root "$EXE" "$HOTKEY_BIN"
    sudo modprobe uinput 2>/dev/null || true
    echo uinput | sudo tee /etc/modules-load.d/clipshelf-uinput.conf >/dev/null
    # Tell libinput our virtual keyboard is the built-in one, so
    # "disable touchpad while typing" keeps working on laptops.
    if ! sudo grep -qs 'ClipShelf virtual keyboard' "$QUIRKS"; then
        printf '\n[ClipShelf virtual keyboard]\nMatchName=ClipShelf virtual keyboard\nAttrKeyboardIntegration=internal\n' \
            | sudo tee -a "$QUIRKS" >/dev/null
    fi
}

enable_service() {
    sudo tee "$SERVICE" >/dev/null <<UNIT
[Unit]
Description=ClipShelf keyboard shortcut (Super+V in every keyboard layout)
After=systemd-udevd.service

[Service]
ExecStart=$HOTKEY_BIN --hotkey-daemon
Restart=on-failure
RestartSec=2
NoNewPrivileges=yes
ProtectHome=yes
ProtectSystem=strict
PrivateTmp=yes

[Install]
WantedBy=multi-user.target
UNIT
    sudo systemctl daemon-reload
    sudo systemctl enable clipshelf-hotkey >/dev/null 2>&1
    sudo systemctl restart clipshelf-hotkey
}

if ! command -v systemctl >/dev/null; then
    echo "(systemd not found: skipping the keyboard-shortcut service)"
elif [ -f "$SERVICE" ]; then
    # Upgrade: the service was already approved and tested.
    say "Updating the Super+V keyboard service"
    sudo systemctl stop clipshelf-hotkey 2>/dev/null || true
    install_service_files
    enable_service
    HOTKEY_OK=1
else
    echo
    say "Super+V keyboard shortcut"
    echo "Desktop shortcuts often fail with non-Latin keyboard layouts (Hebrew, Arabic,"
    echo "Russian…). ClipShelf can install a small system service that recognises the"
    echo "physical Super+V keys directly, in every layout and on every desktop."
    echo "It passes all other keys through untouched."
    read -rp "Install it? This asks for your password. [Y/n] " ans || ans=n
    if [[ ! "$ans" =~ ^[Nn] ]]; then
        install_service_files
        echo
        say "30-second trial"
        echo "The service is running now, for 30 seconds only. Please:"
        echo "  1. press Super+V (also with your other keyboard layouts) – ClipShelf should open,"
        echo "  2. type a few words anywhere to check your keyboard works normally."
        echo "If something goes wrong, just wait: it switches itself off after 30 seconds."
        echo "(Emergency stop at any time: hold Backspace + Escape + Enter.)"
        sudo "$HOTKEY_BIN" --hotkey-daemon --timeout 30 >/tmp/clipshelf-trial.log 2>&1 &
        TRIAL=$!
        sleep 1
        if read -t 30 -rp "Did it work? Type y and press Enter: " ans && [[ "$ans" =~ ^[Yy] ]]; then
            sudo kill $TRIAL 2>/dev/null || true; wait $TRIAL 2>/dev/null || true
            enable_service
            HOTKEY_OK=1
            say "Super+V service enabled."
        else
            echo
            sudo kill $TRIAL 2>/dev/null || true; wait $TRIAL 2>/dev/null || true
            echo "OK, not enabled. Details: /tmp/clipshelf-trial.log"
        fi
    fi
fi

# 3b. Without the service: the desktop's own shortcut (GNOME-based desktops).
if [ $HOTKEY_OK = 0 ]; then
    if [ $HAS_GNOME_KEYS = 1 ]; then
        say "Setting the Super+V desktop shortcut"
        # The physical V key (keycode 0x37) rather than the letter v.
        add_shortcut clipshelf '<Super>0x37'
        gsettings set "$MK.custom-keybinding:$KB_BASE/clipshelf/" name 'ClipShelf (Super+V)'
        # GNOME uses Super+V for the notification list; keep Super+M for that.
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
        echo "Add a keyboard shortcut in your desktop settings that runs:  $EXE --toggle"
    fi
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

echo
say "ClipShelf is installed and running. Press Super+V, or click its icon next to the keyboard-language indicator."
[ $HOTKEY_OK = 1 ] && echo "(Recommended once: restart the computer, so the touchpad settings pick up the new keyboard service.)" || true
