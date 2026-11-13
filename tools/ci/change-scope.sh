#!/bin/sh
# Decide whether a pull request changes only documents that no check reads.
# Usage: change-scope.sh <base-ref>
# Prints one line, `docs_only=true` or `docs_only=false`. Any doubt answers false,
# so an unreadable base, an empty change, or one path outside the list runs every check.
set -u
base=${1:-}
answer=false
if [ -n "$base" ] && git rev-parse -q --verify "$base^{commit}" > /dev/null 2>&1; then
  # --no-renames lists both sides of a move, so moving a source file into the list
  # still shows the source path it left.
  if paths=$(git diff --name-only --no-renames "$base...HEAD" 2> /dev/null) && [ -n "$paths" ]; then
    answer=true
    while IFS= read -r path; do
      case $path in
        docs/handoff/2026-11-11-v0-9/*) ;;
        *)
          answer=false
          break
          ;;
      esac
    done << EOF
$paths
EOF
  fi
fi
printf 'docs_only=%s\n' "$answer"
