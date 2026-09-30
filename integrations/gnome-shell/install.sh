#!/usr/bin/env bash
# PULSE Overlay Bridge (GNOME Shell 45) — user-level install / update /
# uninstall. See docs/overlay/gnome-bridge.md.
#
# Touches exactly one directory:
#   ~/.local/share/gnome-shell/extensions/pulse-overlay@jamby/
# No sudo, no system schema, no other extension, no GNOME setting except this
# extension's own entry in the enabled list (via `gnome-extensions`).
#
#   ./install.sh check      validate the sources (writes nothing)
#   ./install.sh status     what is installed and what GNOME Shell runs (writes nothing)
#   ./install.sh install    copy + compile the private schema (then log out/in once)
#   ./install.sh update     same as install, over a copy this script installed
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

version_of() {
  python3 -c 'import json,sys; print(json.load(open(sys.argv[1])).get("version","?"))' "$1" 2>/dev/null || echo "?"
}

case "${1:-}" in
  check)
    check
    ;;
  status)
    echo "bundled:   v$(version_of "$SOURCE/metadata.json")  ($SOURCE)"
    if [[ -f "$TARGET/metadata.json" ]]; then
      echo "installed: v$(version_of "$TARGET/metadata.json")  ($TARGET)$(ours && echo ', by this script')"
    else
      echo "installed: no"
    fi
    if command -v gnome-extensions >/dev/null; then
      LC_ALL=C gnome-extensions info "$UUID" 2>/dev/null | sed -n 's/^ *\(State\|Version\): */running \1: /p' ||
        echo "running:   GNOME Shell does not know it yet (log out and back in after installing)"
    fi
    ;;
  install|update)
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
    echo "Installed v$(version_of "$TARGET/metadata.json") to $TARGET"
    echo "GNOME Shell 45 on Wayland loads extension code only at login:"
    echo "log out and back in once, then enable it (PULSE → Overlays, or: $0 enable)."
    ;;
  enable)
    gnome-extensions enable "$UUID"
    LC_ALL=C gnome-extensions info "$UUID" | sed -n '1,/State/p'
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
    sed -n '2,19p' "$0"
    exit 2
    ;;
esac
