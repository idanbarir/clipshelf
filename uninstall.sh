#!/usr/bin/env bash
# Removes clipshelf. Add --purge to also delete your history and settings.
set -uo pipefail

EXE="$HOME/.local/bin/clipshelf"
[ -x "$EXE" ] && "$EXE" --quit >/dev/null 2>&1
rm -f "$EXE" \
      "$HOME/.local/share/applications/io.github.clipshelf.Clipshelf.desktop" \
      "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/clipshelf.desktop"

MK=org.gnome.settings-daemon.plugins.media-keys
KB_PATH=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/clipshelf/
SCHEMAS=$(gsettings list-schemas 2>/dev/null || true)
if command -v gsettings >/dev/null && grep -qx "$MK" <<<"$SCHEMAS"; then
    current=$(gsettings get $MK custom-keybindings)
    new=$(python3 - "$current" "$KB_PATH" <<'PY'
import ast, sys
cur = sys.argv[1].strip()
if cur.startswith('@as'):
    cur = cur[3:].strip()
lst = [p for p in (ast.literal_eval(cur) if cur else []) if p != sys.argv[2]]
print(str(lst) if lst else "@as []")
PY
)
    gsettings set $MK custom-keybindings "$new"
    gsettings reset-recursively "$MK.custom-keybinding:$KB_PATH" 2>/dev/null || true
    gsettings reset org.gnome.shell.keybindings toggle-message-tray 2>/dev/null || true
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
echo "clipshelf removed."
