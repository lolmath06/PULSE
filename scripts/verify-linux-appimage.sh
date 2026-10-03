#!/bin/sh
# Verifies the release AppImage's intentional system-runtime contract and the
# GLIBC ceiling of every ELF file shipped in it.
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <PULSE.AppImage> <canonical binary> <maximum GLIBC version>" >&2
  exit 2
fi

repo_root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
appimage=$(realpath "$1")
canonical_binary=$(realpath "$2")
maximum=$3
work=$(mktemp -d "${TMPDIR:-/tmp}/pulse-appimage-verify.XXXXXX")
trap 'rm -rf -- "$work"' EXIT HUP INT TERM

if ! file "$appimage" | grep -q 'ELF'; then
  echo "AppImage runtime is not an ELF executable" >&2
  exit 1
fi
if readelf --program-headers "$appimage" | grep -q ' INTERP '; then
  echo "AppImage runtime must be statically linked, but it has an interpreter" >&2
  exit 1
fi
if readelf --dynamic "$appimage" 2>/dev/null | grep -q '(NEEDED)'; then
  echo "AppImage runtime must be statically linked, but it has shared dependencies" >&2
  exit 1
fi
echo "AppImage runtime is static."

(cd "$work" && "$appimage" --appimage-extract >/dev/null)
appdir=$work/squashfs-root
pulse=$appdir/usr/bin/pulse

if [ ! -x "$appdir/AppRun" ] || [ ! -x "$pulse" ]; then
  echo "AppImage is missing AppRun or usr/bin/pulse" >&2
  exit 1
fi

# Recreate Tauri's fixed-width UNK -> APP patch from the canonical executable.
# Exact equality proves that APP is the active bundle state and that no DEB or
# RPM state was carried over, without confusing compiler-emitted string
# literals with the token Tauri actually patches.
expected_pulse=$work/expected-pulse
cp "$canonical_binary" "$expected_pulse"
"$repo_root/scripts/patch-tauri-bundle-marker.sh" "$expected_pulse" UNK APP
if ! cmp -s "$expected_pulse" "$pulse"; then
  echo "usr/bin/pulse is not the canonical executable with only the Tauri APP marker patch" >&2
  exit 1
fi
echo "AppImage executable exactly matches the canonical binary with its APP marker."

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
