#!/bin/sh
# Verifies the release AppImage's intentional system-runtime contract and the
# GLIBC ceiling of every ELF file shipped in it.
set -eu

if [ "$#" -ne 2 ]; then
  echo "usage: $0 <PULSE.AppImage> <maximum GLIBC version>" >&2
  exit 2
fi

appimage=$(realpath "$1")
maximum=$2
work=$(mktemp -d "${TMPDIR:-/tmp}/pulse-appimage-verify.XXXXXX")
trap 'rm -rf -- "$work"' EXIT HUP INT TERM

(cd "$work" && "$appimage" --appimage-extract >/dev/null)
appdir=$work/squashfs-root
pulse=$appdir/usr/bin/pulse

if [ ! -x "$appdir/AppRun" ] || [ ! -x "$pulse" ]; then
  echo "AppImage is missing AppRun or usr/bin/pulse" >&2
  exit 1
fi

if [ "$(grep -aoF '__TAURI_BUNDLE_TYPE_VAR_APP' "$pulse" | wc -l)" -ne 1 ]; then
  echo "usr/bin/pulse does not contain exactly one Tauri AppImage bundle marker" >&2
  exit 1
fi
if grep -aEq '__TAURI_BUNDLE_TYPE_VAR_(UNK|DEB|RPM)' "$pulse"; then
  echo "usr/bin/pulse contains a stale Tauri bundle marker" >&2
  exit 1
fi

bundled_runtime=$(find "$appdir" \( -type f -o -type l \) \
  \( -name '*.so' -o -name '*.so.*' -o -name 'WebKitWebProcess' \
     -o -name 'WebKitNetworkProcess' -o -name 'WebKitGPUProcess' \) -print)
if [ -n "$bundled_runtime" ]; then
  echo "AppImage must use the target system's coherent GTK/WebKit runtime; found:" >&2
  echo "$bundled_runtime" >&2
  exit 1
fi

if grep -Eq '^[[:space:]]*(export[[:space:]]+)?(LD_LIBRARY_PATH|GST_PLUGIN_(SYSTEM_)?PATH)=' "$appdir/AppRun"; then
  echo "AppRun must not override system library or GStreamer search paths" >&2
  exit 1
fi

for library in libwebkit2gtk-4.1.so.0 libjavascriptcoregtk-4.1.so.0 libgtk-3.so.0 libglib-2.0.so.0; do
  if ! readelf -d "$pulse" | grep -Fq "Shared library: [$library]"; then
    echo "usr/bin/pulse no longer declares the expected system dependency $library" >&2
    exit 1
  fi
done

elfs=$work/elfs
printf '%s\n' "$appimage" > "$elfs"
find "$appdir" -type f -exec sh -c '
  for file do
    if file "$file" | grep -q "ELF"; then
      printf "%s\n" "$file"
    fi
  done
' sh {} + >> "$elfs"

while IFS= read -r elf; do
  highest=$(readelf --version-info "$elf" 2>/dev/null \
    | grep -oE 'GLIBC_[0-9]+(\.[0-9]+)+' \
    | sort -Vu \
    | tail -n 1 || true)
  if [ -z "$highest" ]; then
    if ! readelf --program-headers "$elf" | grep -q ' INTERP '; then
      echo "$elf: no dynamic GLIBC requirement"
      continue
    fi
    echo "No required GLIBC version found in dynamically linked ELF $elf" >&2
    exit 1
  fi
  echo "$elf: highest required GLIBC version is $highest"
  if [ "$(printf '%s\n' "$maximum" "$highest" | sort -V | tail -n 1)" != "$maximum" ]; then
    echo "$elf requires $highest, above the $maximum release baseline" >&2
    exit 1
  fi
done < "$elfs"

echo "AppImage contains no bundled GTK/WebKit runtime and satisfies $maximum."
