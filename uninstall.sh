#!/usr/bin/env bash
# Removes clipshelf. Add --purge to also delete your history and settings.
set -uo pipefail

EXE="$HOME/.local/bin/clipshelf"
[ -x "$EXE" ] && "$EXE" --quit >/dev/null 2>&1
rm -f "$EXE" \
      "$HOME/.local/share/applications/io.github.clipshelf.Clipshelf.desktop" \
      "$HOME/.local/share/applications/io.github.idanbarir.ClipShelf.desktop" \
      "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/clipshelf.desktop"

MK=org.gnome.settings-daemon.plugins.media-keys
KB_BASE=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings
SCHEMAS=$(gsettings list-schemas 2>/dev/null || true)

# remove_from_list <schema> <key> <item-substring>
remove_from_list() {
    local cur new
    cur=$(gsettings get "$1" "$2")
    new=$(python3 - "$cur" "$3" <<'PY'
import ast, sys
cur = sys.argv[1].strip()
if cur.startswith('@as'):
    cur = cur[3:].strip()
lst = [p for p in (ast.literal_eval(cur) if cur else []) if sys.argv[2] not in p]
print(str(lst) if lst else "@as []")
PY
)
    gsettings set "$1" "$2" "$new"
}

if command -v gsettings >/dev/null && grep -qx "$MK" <<<"$SCHEMAS"; then
    remove_from_list $MK custom-keybindings "$KB_BASE/clipshelf"
    for id in clipshelf clipshelf-alt clipshelf-he clipshelf-ar clipshelf-ru; do
        gsettings reset-recursively "$MK.custom-keybinding:$KB_BASE/$id/" 2>/dev/null || true
    done
    gsettings reset org.gnome.shell.keybindings toggle-message-tray 2>/dev/null || true
fi
if grep -qx org.gnome.shell <<<"$SCHEMAS"; then
    remove_from_list org.gnome.shell favorite-apps io.github.clipshelf.Clipshelf.desktop
    remove_from_list org.gnome.shell favorite-apps io.github.idanbarir.ClipShelf.desktop
fi

if [ -f /etc/systemd/system/clipshelf-hotkey.service ]; then
    echo "Removing the Super+V keyboard service (asks for your password)…"
    sudo systemctl disable --now clipshelf-hotkey 2>/dev/null
    sudo rm -f /etc/systemd/system/clipshelf-hotkey.service
    sudo rm -rf /usr/local/libexec/clipshelf /etc/modules-load.d/clipshelf-uinput.conf
    sudo systemctl daemon-reload
    Q=/etc/libinput/local-overrides.quirks
    if sudo grep -qs 'ClipShelf virtual keyboard' "$Q"; then
        sudo python3 - "$Q" <<'PY'
import sys, re
p = sys.argv[1]
s = open(p).read()
s = re.sub(r'\n?\[ClipShelf virtual keyboard\]\nMatchName=ClipShelf virtual keyboard\nAttrKeyboardIntegration=internal\n', '\n', s)
open(p, 'w').write(s.rstrip('\n') + '\n' if s.strip() else '')
PY
    fi
fi

if [ -f /etc/udev/rules.d/70-clipshelf-uinput.rules ]; then
    echo "Removing the auto-paste permission (asks for your password)…"
    sudo rm -f /etc/udev/rules.d/70-clipshelf-uinput.rules /etc/modules-load.d/clipshelf-uinput.conf
    sudo udevadm control --reload-rules
fi

if [ "${1:-}" = "--purge" ]; then
    rm -rf "${XDG_DATA_HOME:-$HOME/.local/share}/clipshelf" "${XDG_CONFIG_HOME:-$HOME/.config}/clipshelf"
    echo "History and settings deleted."
fi
echo "ClipShelf removed."
