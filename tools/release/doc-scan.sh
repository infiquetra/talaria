#!/bin/sh
# Report locations of potentially private material without printing the matches.
set -u
if [ "$#" -gt 1 ]; then
  printf '%s\n' 'doc-scan.sh: expected at most one repository root' >&2
  exit 2
fi
scan_root=${1:-.}
if ! scan_root=$(git -C "$scan_root" rev-parse --show-toplevel 2>/dev/null); then
  printf '%s\n' 'doc-scan.sh: not inside a git repository' >&2
  exit 2
fi
scan_tmp=$(mktemp -d) || exit 2
trap 'rm -rf "$scan_tmp"' EXIT HUP INT TERM
if ! git -C "$scan_root" ls-files -- '*.md' '*.txt' > "$scan_tmp/files"; then
  printf '%s\n' 'doc-scan.sh: cannot list tracked documents' >&2
  exit 2
fi
scan_home=${HOME:-}
scan_login=${USER:-${LOGNAME:-}}
scan_host=$(hostname -s 2>/dev/null || :)
for scan_item in home login host; do
  case $scan_item in
    home) scan_value=$scan_home ;;
    login) scan_value=$scan_login ;;
    host) scan_value=$scan_host ;;
  esac
  if [ "${#scan_value}" -lt 4 ]; then
    printf 'note: %s not checked (unset or under four characters)\n' "$scan_item"
  fi
done
scan_one() {
  scan_rule=$1
  scan_expr=$2
  scan_mode=$3
  if [ "$scan_mode" = fixed ]; then
    grep -nF -- "$scan_expr" "$scan_root/$scan_file" 2>/dev/null > "$scan_tmp/matches" || :
  else
    grep -nE -- "$scan_expr" "$scan_root/$scan_file" 2>/dev/null > "$scan_tmp/matches" || :
  fi
  while IFS=: read -r scan_line scan_rest; do
    : "$scan_rest"
    [ -n "$scan_line" ] || continue
    printf '%s:%s %s\n' "$scan_file" "$scan_line" "$scan_rule"
    printf '%s\n' hit >> "$scan_tmp/hits"
  done < "$scan_tmp/matches"
}
: > "$scan_tmp/hits"
scan_count=0
while IFS= read -r scan_file; do
  [ -n "$scan_file" ] || continue
  [ -f "$scan_root/$scan_file" ] || continue
  scan_count=$((scan_count + 1))
  scan_one home-path '/(Users|home)/[A-Za-z0-9._-]' regex
  scan_one tmp-path '/(private/)?var/folders/' regex
  scan_one sk-key 'sk-[A-Za-z0-9_-]{20,}' regex
  scan_one url-userinfo '://[^/[:space:]:@]+:[^/[:space:]@]+@' regex
  for scan_value in "$scan_home" "$scan_login" "$scan_host"; do
    if [ "${#scan_value}" -ge 4 ]; then
      scan_one forbidden-literal "$scan_value" fixed
    fi
  done
done < "$scan_tmp/files"
if [ -s "$scan_tmp/hits" ]; then
  exit 1
fi
printf 'DOC_SCAN: OK (%s files)\n' "$scan_count"
