#!/bin/sh
# Fails if any commit to be published attributes authorship to Claude or
# Anthropic: an author/committer identity, or an attribution trailer
# (Co-Authored-By, Generated-By, Assisted-By, Signed-off-by, ...).
# Ordinary file content is never scanned.
#
#   scripts/check-git-attribution.sh            # every commit reachable from HEAD
#   scripts/check-git-attribution.sh --all      # every local branch and tag
#   scripts/check-git-attribution.sh A..B       # a range, e.g. origin/main..HEAD
set -eu

[ "$#" -eq 0 ] && set -- HEAD
pattern='claude|anthropic'
found=0

for commit in $(git rev-list "$@"); do
  hits=$(git cat-file commit "$commit" | awk '
    /^$/ { body = 1; next }
    !body && /^(author|committer) / { print; next }
    body && tolower($0) ~ /^[a-z-]*(authored|generated|assisted|signed-off)[a-z-]*-?by:/ { print }
  ' | grep -iE "$pattern" || true)
  if [ -n "$hits" ]; then
    found=1
    echo "attribution in $(git log -1 --format='%h %s' "$commit"):"
    echo "$hits" | sed 's/^/  /'
  fi
done

if [ "$found" -ne 0 ]; then
  echo "Claude/Anthropic attribution found. Rewrite these commits before publishing." >&2
  exit 1
fi
echo "No Claude/Anthropic attribution in $(git rev-list --count "$@") commit(s)."
