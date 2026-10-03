#!/bin/sh
# Packages Tauri's .deb payload as a thin AppImage. The application and icons
# are embedded, while GTK/WebKitGTK and their helpers come from the target
# system as one ABI-coherent runtime stack.
set -eu

if [ "$#" -ne 5 ]; then
  echo "usage: $0 <tauri.deb> <canonical binary> <appimagetool> <runtime> <output.AppImage>" >&2
  exit 2
fi

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
deb=$(realpath "$1")
canonical_binary=$(realpath "$2")
packer=$(realpath "$3")
runtime=$(realpath "$4")
marker_patcher=$repo_root/scripts/patch-tauri-bundle-marker.sh
case $5 in
  /*) output=$5 ;;
  *) output=$PWD/$5 ;;
esac

work=$(mktemp -d "${TMPDIR:-/tmp}/pulse-appimage.XXXXXX")
trap 'rm -rf -- "$work"' EXIT HUP INT TERM
appdir=$work/PULSE.AppDir

dpkg-deb --extract "$deb" "$appdir"
install -m 755 "$repo_root/scripts/appimage/AppRun" "$appdir/AppRun"

# Tauri restores the canonical executable after building each package. Use
# that known UNK state rather than depending on the marker in an intermediate
# DEB payload, then patch only the AppImage copy at the token's exact offset.
pulse=$appdir/usr/bin/pulse
canonical_hash=$(sha256sum "$canonical_binary" | cut -d' ' -f1)
install -m 755 "$canonical_binary" "$pulse"
"$marker_patcher" "$pulse" UNK APP
if [ "$(sha256sum "$canonical_binary" | cut -d' ' -f1)" != "$canonical_hash" ]; then
  echo "Canonical executable was modified while packaging the AppImage" >&2
  exit 1
fi

desktop=usr/share/applications/PULSE.desktop
icon=usr/share/icons/hicolor/256x256@2/apps/pulse.png
if [ ! -f "$appdir/$desktop" ] || [ ! -f "$appdir/$icon" ]; then
  echo "Tauri .deb payload is missing its desktop file or release icon" >&2
  exit 1
fi

ln -s "$desktop" "$appdir/PULSE.desktop"
ln -s "$icon" "$appdir/pulse.png"
ln -s pulse.png "$appdir/.DirIcon"

mkdir -p "$(dirname -- "$output")"
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 \
  "$packer" --runtime-file "$runtime" "$appdir" "$output"

if [ ! -s "$output" ]; then
  echo "AppImage packer did not create $output" >&2
  exit 1
fi
