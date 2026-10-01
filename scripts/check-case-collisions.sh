#!/bin/sh
# Fails if two Git-tracked paths differ only by letter case. Linux and Fedora
# keep both; Windows (and macOS by default) are case-insensitive, so a
# checkout there breaks, merges them, or resolves imports to the wrong file.
#
# Two kinds are checked:
#   - paths (files and directories): src/Design/a.ts beside src/design/b.ts;
#   - module names: source files whose paths differ only by case once the
#     extension is dropped — src/design/LookContext.tsx and
#     src/design/lookContext.ts. An import of "@/design/LookContext" tries
#     ".ts" first and, on Windows, lands on lookContext.ts.
#
#   scripts/check-case-collisions.sh
set -eu

git ls-files -z | tr '\0' '\n' | awk '
  function check(kind, name,    key) {
    key = kind SUBSEP tolower(name)
    if (!(key in first)) {
      first[key] = name
    } else if (first[key] != name && !((key, name) in reported)) {
      reported[key, name] = 1
      printf "case collision (%s): %s <-> %s\n", kind, first[key], name
      bad = 1
    }
  }
  {
    n = split($0, part, "/")
    prefix = ""
    for (i = 1; i <= n; i++) {
      prefix = (i == 1) ? part[1] : prefix "/" part[i]
      check("path", prefix)
    }
    stem = $0
    if (sub(/(\.d)?\.(ts|tsx|mts|cts|js|jsx|mjs|cjs)$/, "", stem)) check("module", stem)
  }
  END {
    if (bad) {
      print "Tracked paths must differ by more than letter case (Windows is case-insensitive)." > "/dev/stderr"
      exit 1
    }
    printf "No case-insensitive path or module collisions in %d tracked files.\n", NR
  }
'
