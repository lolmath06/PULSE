#!/bin/sh
# Patches the one Tauri bundle-state token in an executable, using the same
# fixed-width replacement model as tauri-bundler's patch_binary function.
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <binary> <source marker suffix> <target marker suffix>" >&2
  exit 2
fi

binary=$(realpath "$1")
case $2 in
  UNK | DEB | RPM | APP) ;;
  *) echo "Unsupported source bundle marker: $2" >&2; exit 2 ;;
esac
case $3 in
  UNK | DEB | RPM | APP) ;;
  *) echo "Unsupported target bundle marker: $3" >&2; exit 2 ;;
esac
source_marker=__TAURI_BUNDLE_TYPE_VAR_$2
target_marker=__TAURI_BUNDLE_TYPE_VAR_$3

report_marker_counts() {
  label=$1
  printf '%s markers:' "$label"
  for suffix in UNK DEB RPM APP; do
    count=$(LC_ALL=C grep -aoF "__TAURI_BUNDLE_TYPE_VAR_$suffix" "$binary" \
      | wc -l | tr -d '[:space:]')
    printf ' %s=%s' "$suffix" "$count"
  done
  printf '\n'
}

if [ "${#source_marker}" -ne "${#target_marker}" ]; then
  echo "Tauri bundle markers must have identical lengths" >&2
  exit 1
fi

source_count=$(LC_ALL=C grep -aoF "$source_marker" "$binary" | wc -l | tr -d '[:space:]')
report_marker_counts before
if [ "$source_count" -ne 1 ]; then
  echo "Expected exactly one $source_marker token in $binary; found $source_count" >&2
  exit 1
fi

before_size=$(wc -c < "$binary")
marker_offset=$(LC_ALL=C grep -aboF "$source_marker" "$binary" | cut -d: -f1)
printf '%s' "$target_marker" \
  | dd of="$binary" bs=1 seek="$marker_offset" conv=notrunc status=none

after_size=$(wc -c < "$binary")
if [ "$after_size" -ne "$before_size" ]; then
  echo "Tauri bundle marker patch changed the executable size" >&2
  exit 1
fi
if LC_ALL=C grep -aqF "$source_marker" "$binary"; then
  echo "Source Tauri bundle marker remains after patching $binary" >&2
  exit 1
fi
patched_marker=$(dd if="$binary" bs=1 skip="$marker_offset" \
  count="${#target_marker}" status=none)
if [ "$patched_marker" != "$target_marker" ]; then
  echo "Tauri bundle marker was not patched at the intended offset" >&2
  exit 1
fi

report_marker_counts after
echo "Patched Tauri bundle marker $2 -> $3 at byte $marker_offset in $binary"
