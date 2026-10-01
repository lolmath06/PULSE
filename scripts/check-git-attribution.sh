#!/bin/sh
# Fails if any commit to be published attributes authorship to Claude,
# Anthropic, Codex, OpenAI or ChatGPT: an author/committer identity, or an
# attribution trailer (Co-Authored-By, Generated-By, Assisted-By,
# Signed-off-by, ...).
# Ordinary file content is never scanned.
#
#   scripts/check-git-attribution.sh            # every commit reachable from HEAD
#   scripts/check-git-attribution.sh --all      # every local branch and tag
#   scripts/check-git-attribution.sh A..B       # a range, e.g. origin/main..HEAD
set -eu

[ "$#" -eq 0 ] && set -- HEAD
ai_pattern='claude|anthropic|codex|openai|chatgpt'
attribution_trailer_pattern='^([[:alnum:]-]*-by|author|committer|co-author)[[:space:]]*:'
found=0

for commit in $(git rev-list "$@"); do
  identities=$(git cat-file commit "$commit" | awk '
    /^$/ { exit }
    /^(author|committer) / { print }
  ')
  trailers=$(git log -1 --format=%B "$commit" |
    git interpret-trailers --parse |
    grep -iE "$attribution_trailer_pattern" || true)
  hits=$(printf '%s\n%s\n' "$identities" "$trailers" |
    grep -iE "$ai_pattern" || true)
  if [ -n "$hits" ]; then
    found=1
    echo "attribution in $(git log -1 --format='%h %s' "$commit"):"
    echo "$hits" | sed 's/^/  /'
  fi
done

if [ "$found" -ne 0 ]; then
  echo "Prohibited AI attribution found. Rewrite these commits before publishing." >&2
  exit 1
fi
echo "No Claude/Anthropic/Codex/OpenAI/ChatGPT attribution in $(git rev-list --count "$@") commit(s)."
