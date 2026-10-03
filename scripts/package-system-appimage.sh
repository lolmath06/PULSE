#!/bin/sh
# Packages Tauri's .deb payload as a thin AppImage. The application and icons
# are embedded, while GTK/WebKitGTK and their helpers come from the target
# system as one ABI-coherent runtime stack.
set -eu

if [ "$#" -ne 4 ]; then
  echo "usage: $0 <tauri.deb> <appimagetool> <runtime> <output.AppImage>" >&2
  exit 2
fi

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
deb=$(realpath "$1")
packer=$(realpath "$2")
runtime=$(realpath "$3")
case $4 in
  /*) output=$4 ;;
  *) output=$PWD/$4 ;;
esac

work=$(mktemp -d "${TMPDIR:-/tmp}/pulse-appimage.XXXXXX")
trap 'rm -rf -- "$work"' EXIT HUP INT TERM
appdir=$work/PULSE.AppDir

dpkg-deb --extract "$deb" "$appdir"
install -m 755 "$repo_root/scripts/appimage/AppRun" "$appdir/AppRun"

# Tauri stamps the bundle type into the executable while creating each
# package. This payload came from the .deb, so update its fixed-width marker
# to the AppImage value before repackaging it.
pulse=$appdir/usr/bin/pulse
if [ "$(grep -aoF '__TAURI_BUNDLE_TYPE_VAR_DEB' "$pulse" | wc -l)" -ne 1 ]; then
  echo "Expected exactly one Tauri DEB bundle marker in $pulse" >&2
  exit 1
fi
sed -i 's/__TAURI_BUNDLE_TYPE_VAR_DEB/__TAURI_BUNDLE_TYPE_VAR_APP/' "$pulse"

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
