#!/usr/bin/env bash
# PULSE overlay bridge (GNOME Shell 45) — user-level install / uninstall.
#
# PROTOTYPE (Phase 11.5A), NOT PHYSICALLY PROVEN.
#
# Touches exactly one directory:
#   ~/.local/share/gnome-shell/extensions/pulse-overlay@jamby/
# No sudo, no system schema, no other extension, no GNOME setting except this
# extension's own entry in the enabled list (via `gnome-extensions`).
#
#   ./install.sh check      validate the sources (writes nothing)
#   ./install.sh install    copy + compile the private schema (then log out/in once)
#   ./install.sh enable     gnome-extensions enable
#   ./install.sh disable    gnome-extensions disable
#   ./install.sh uninstall  disable, then remove the directory — only if this
#                           script created it (marker file)
set -euo pipefail

UUID="pulse-overlay@jamby"
SOURCE="$(cd "$(dirname "${BASH_SOURCE[0]}")/$UUID" && pwd)"
TARGET="${XDG_DATA_HOME:-$HOME/.local/share}/gnome-shell/extensions/$UUID"
MARKER=".installed-by-pulse"
FILES=(metadata.json extension.js lib.js)
SCHEMA="schemas/org.gnome.shell.extensions.pulse-overlay.gschema.xml"

check() {
  python3 -c 'import json,sys; json.load(open(sys.argv[1]))' "$SOURCE/metadata.json"
  glib-compile-schemas --strict --dry-run "$SOURCE/schemas"
  for file in "${FILES[@]}" "$SCHEMA"; do
    [[ -f "$SOURCE/$file" ]] || { echo "missing: $file" >&2; exit 1; }
  done
  echo "sources OK: $SOURCE"
}

ours() { [[ -f "$TARGET/$MARKER" ]]; }

case "${1:-}" in
  check)
    check
    ;;
  install)
    check
    if [[ -e "$TARGET" ]] && ! ours; then
      echo "REFUSING: $TARGET exists and was not installed by this script." >&2
      echo "Inspect it yourself; nothing was changed." >&2
      exit 1
    fi
    mkdir -p "$TARGET/schemas"
    for file in "${FILES[@]}"; do install -m 0644 "$SOURCE/$file" "$TARGET/$file"; done
    install -m 0644 "$SOURCE/$SCHEMA" "$TARGET/$SCHEMA"
    glib-compile-schemas --strict "$TARGET/schemas"
    date -Is > "$TARGET/$MARKER"
    echo "Installed to $TARGET"
    echo "GNOME Shell 45 on Wayland only discovers new extensions at login:"
    echo "log out and back in once, then run: $0 enable"
    ;;
  enable)
    gnome-extensions enable "$UUID"
    gnome-extensions info "$UUID" | sed -n '1,/State/p'
    ;;
  disable)
    gnome-extensions disable "$UUID" || true
    ;;
  uninstall)
    gnome-extensions disable "$UUID" 2>/dev/null || true
    if [[ ! -e "$TARGET" ]]; then
      echo "Not installed: $TARGET"
    elif ours; then
      rm -r -- "$TARGET"
      echo "Removed $TARGET"
    else
      echo "REFUSING: $TARGET was not installed by this script; left untouched." >&2
      exit 1
    fi
    ;;
  *)
    sed -n '2,17p' "$0"
    exit 2
    ;;
esac
