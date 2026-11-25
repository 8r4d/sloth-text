#!/bin/sh
# Installs Sloth Writer for you alone (no sudo): the program in
# ~/.local/bin, with its icon and menu entry. Run it from the folder it
# came in. Run it again to update; `./install.sh --remove` takes it away.
set -e
here=$(cd "$(dirname "$0")" && pwd)
bin="$HOME/.local/bin"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
icon="$data/icons/hicolor/512x512/apps/sloth-writer.png"
desktop="$data/applications/sloth-writer.desktop"

if [ "$1" = "--remove" ]; then
    rm -f "$bin/sloth-writer" "$icon" "$desktop"
    echo "Sloth Writer removed. (Your projects, settings and backups are left alone.)"
    exit 0
fi

mkdir -p "$bin" "$(dirname "$icon")" "$(dirname "$desktop")"
install -m 755 "$here/sloth-writer" "$bin/sloth-writer"
install -m 644 "$here/sloth-writer.png" "$icon"
install -m 644 "$here/sloth-writer.desktop" "$desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$data/applications" 2>/dev/null || true
echo "Sloth Writer is installed: find it in your applications menu, or run sloth-writer."
case ":$PATH:" in *":$bin:"*) ;; *) echo "(Add $bin to your PATH to run it by name.)" ;; esac
