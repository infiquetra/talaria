#!/bin/sh
# Run release gates against one frozen commit, or check a proposed tag difference.
set -u

rc_error() { printf 'rc.sh: %s\n' "$1" >&2; exit 2; }
rc_usage() {
  printf '%s\n' 'usage: rc.sh run [--class ci|live|operator|all] [--only id]... [--gates file] [--evidence file] [--summary file]'
  printf '%s\n' '       rc.sh allowed-diff candidate tag-commit'
  printf '%s\n' '       rc.sh --list'
  exit 2
}
[ "$#" -gt 0 ] || rc_usage
rc_command=$1
shift
case $rc_command in
  run|allowed-diff|--list) ;;
  *) rc_usage ;;
esac
rc_root=$(git rev-parse --show-toplevel 2>/dev/null) || rc_error 'not inside a git repository'
rc_root=$(CDPATH='' cd -- "$rc_root" && pwd -P) || rc_error 'cannot resolve repository root'
cd "$rc_root" || rc_error 'cannot enter repository'

if [ "$rc_command" = --list ]; then
  [ "$#" -eq 0 ] || rc_usage
  [ -r tools/release/gates.tsv ] || rc_error 'gates file not found'
  cat tools/release/gates.tsv
  exit $?
fi

if [ "$rc_command" = allowed-diff ]; then
  [ "$#" -eq 2 ] || rc_error 'allowed-diff needs two commits'
  exec python3 - "$1" "$2" <<'PY'
import re
import subprocess
import sys


def git(*args, check=True):
    return subprocess.run(['git', *args], stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, check=check).stdout


def error(message):
    print('rc.sh: ' + message, file=sys.stderr)
    raise SystemExit(2)


try:
    before = git('rev-parse', '--verify', '-q', sys.argv[1] + '^{commit}').decode().strip()
    after = git('rev-parse', '--verify', '-q', sys.argv[2] + '^{commit}').decode().strip()
except subprocess.CalledProcessError:
    error('both arguments must resolve to commits')
if subprocess.run(['git', 'merge-base', '--is-ancestor', before, after], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode:
    error('candidate is not an ancestor of the tag commit')

paths = [p.decode('utf-8', 'surrogateescape') for p in git('diff', '--no-renames', '--name-only', '-z', before, after).split(b'\0') if p]
version = re.compile(r'version\s*=\s*"[^"]*"')
version_line = re.compile(r'^\s*version\s*=\s*"[^"]*"\s*(?:#.*)?$')
lock_version_line = re.compile(r'^\s*version\s*=\s*"[^"]*"\s*$')
path_line = re.compile(r'\bpath\s*=\s*"')
table_line = re.compile(r'^\s*(\[\[?[^\]]+\]\]?)\s*(?:#.*)?$')
hunk = re.compile(r'^@@ -(\d+)(?:,\d+)? \+(\d+)(?:,\d+)? @@')


def content(commit, path):
    try:
        return git('show', commit + ':' + path).decode('utf-8', 'surrogateescape').splitlines()
    except subprocess.CalledProcessError:
        return []


def changes(path):
    diff = git('diff', '--no-renames', '--no-ext-diff', '--no-color', '--unified=0', before, after, '--', path).decode('utf-8', 'surrogateescape').splitlines()
    if any(line.startswith(('old mode ', 'new mode ', 'new file mode ', 'deleted file mode ', 'Binary files ', 'GIT binary patch')) for line in diff):
        return None
    old_line = new_line = 0
    removed, added = [], []
    for line in diff:
        m = hunk.match(line)
        if m:
            old_line, new_line = int(m.group(1)), int(m.group(2))
        elif line.startswith(('---', '+++', '\\')):
            continue
        elif line.startswith('-'):
            removed.append((old_line, line[1:]))
            old_line += 1
        elif line.startswith('+'):
            added.append((new_line, line[1:]))
            new_line += 1
        elif line.startswith(' '):
            old_line += 1
            new_line += 1
    return removed, added


def section(lines, number):
    table, begin = '', 0
    for i, line in enumerate(lines[:number], 1):
        match = table_line.match(line)
        if match:
            table, begin = re.sub(r'[\s\"\']', '', match.group(1)), i
    end = len(lines)
    for i in range(begin, len(lines)):
        if table_line.match(lines[i]):
            end = i
            break
    return table, lines[begin:end]


def dependency_kind(table):
    parts = table.strip('[]').split('.')
    names = ('dependencies', 'dev-dependencies', 'build-dependencies')
    if parts[0] in names:
        end = 1
    elif len(parts) >= 2 and parts[:2] == ['workspace', 'dependencies']:
        end = 2
    elif parts[0] == 'target':
        end = next((index + 1 for index, part in enumerate(parts[1:], 1) if part in names), 0)
        if end == 0:
            return None
    else:
        return None
    if len(parts) == end:
        return 'base'
    if len(parts) == end + 1:
        return 'expanded'
    return None


def allowed_manifest_side(lines, entries):
    masked = []
    for number, line in entries:
        table, body = section(lines, number)
        package_version = table in ('[package]', '[workspace.package]') and version_line.match(line)
        inline_path = dependency_kind(table) == 'base' and path_line.search(line) and version.search(line)
        expanded_path = dependency_kind(table) == 'expanded' and version_line.match(line) and any(path_line.search(x) for x in body)
        if not (package_version or inline_path or expanded_path):
            return None
        masked.append(version.sub('version = "X"', line))
    return sorted(masked)


def manifest_ok(path):
    diff = changes(path)
    if diff is None:
        return False
    removed, added = diff
    if not removed or not added:
        return False
    old = allowed_manifest_side(content(before, path), removed)
    new = allowed_manifest_side(content(after, path), added)
    return old is not None and new is not None and old == new


def local_lock_side(lines, entries):
    for number, line in entries:
        if not lock_version_line.match(line) or not 1 <= number <= len(lines):
            return False
        start = number - 1
        while start > 0 and lines[start].strip() and lines[start].strip() != '[[package]]':
            start -= 1
        if lines[start].strip() != '[[package]]':
            return False
        end = start + 1
        while end < len(lines) and lines[end].strip():
            end += 1
        if any(re.match(r'^\s*source\s*=', item) for item in lines[start:end]):
            return False
    return True


def lock_ok(path):
    diff = changes(path)
    if diff is None:
        return False
    removed, added = diff
    return bool(removed) and len(removed) == len(added) and local_lock_side(content(before, path), removed) and local_lock_side(content(after, path), added)


bad = []
for path in paths:
    if path in ('CHANGELOG.md', 'README.md', 'llms.txt') or path.startswith('docs/'):
        continue
    if path == 'Cargo.lock' and lock_ok(path):
        continue
    if (path == 'Cargo.toml' or re.fullmatch(r'(?:crates|tools)/[^/]+/Cargo\.toml', path)) and manifest_ok(path):
        continue
    bad.append(path)
if bad:
    print('not allowed:')
    for path in bad:
        print('  ' + path)
    raise SystemExit(1)
print('ALLOWED_DIFF: OK (%d files)' % len(paths))
PY
fi

rc_class=all
rc_gates=tools/release/gates.tsv
rc_evidence=
rc_summary=
rc_partial=0
rc_tmp=$(mktemp -d) || rc_error 'cannot create temporary directory'
trap 'rm -rf "$rc_tmp"' EXIT HUP INT TERM
: > "$rc_tmp/only"
while [ "$#" -gt 0 ]; do
  rc_option=$1
  shift
  case $rc_option in
    --class|--only|--gates|--evidence|--summary)
      [ "$#" -gt 0 ] || rc_error "missing value for $rc_option"
      rc_value=$1
      shift
      case $rc_option in
        --class) rc_class=$rc_value ;;
        --only) printf '%s\n' "$rc_value" >> "$rc_tmp/only"; rc_partial=1 ;;
        --gates) rc_gates=$rc_value; rc_partial=1 ;;
        --evidence) rc_evidence=$rc_value ;;
        --summary) rc_summary=$rc_value ;;
      esac ;;
    *) rc_error "unknown option $rc_option" ;;
  esac
done
case $rc_class in ci|live|operator|all) ;; *) rc_error "unknown class $rc_class" ;; esac
rc_check_output() {
  rc_kind=$1
  rc_path=$2
  [ -n "$rc_path" ] || return 0
  rc_parent=$(dirname -- "$rc_path")
  [ -d "$rc_parent" ] || rc_error "no such directory for $rc_kind"
  rc_parent=$(CDPATH='' cd -- "$rc_parent" && pwd -P) || rc_error "no such directory for $rc_kind"
  case "$rc_parent/" in "$rc_root/"*) rc_error "refusing to write $rc_kind inside the repository" ;; esac
  [ ! -L "$rc_path" ] || rc_error "refusing symbolic link for $rc_kind"
}
rc_check_output evidence "$rc_evidence"
rc_check_output summary "$rc_summary"
[ -f "$rc_gates" ] && [ -r "$rc_gates" ] || rc_error 'gates file not found'
if ! awk -F '\t' '
  /^#/ || /^[[:space:]]*$/ { next }
  { if (NF != 4) problem = "expected four fields"
    else if ($1 == "" || $1 ~ /[[:space:]]/) problem = "invalid id"
    else if ($2 != "yes" && $2 != "no") problem = "invalid required value"
    else if ($3 != "ci" && $3 != "live" && $3 != "operator") problem = "invalid class"
    else if ($4 == "" || $0 ~ /\r/) problem = "invalid command or carriage return"
    else if (seen[$1]++) problem = "duplicate id"
    if (problem != "") { printf "rc.sh: gates file line %d: %s\n", NR, problem > "/dev/stderr"; exit 1 }
  }
' "$rc_gates"; then exit 2; fi
while IFS= read -r rc_id; do
  if ! awk -F '\t' -v id="$rc_id" '$1 == id { found=1 } END { exit !found }' "$rc_gates"; then
    rc_error "unknown gate $rc_id"
  fi
done < "$rc_tmp/only"
: > "$rc_tmp/selected"
rc_tab=$(printf '\t')
while IFS="$rc_tab" read -r rc_id rc_required rc_gate_class rc_gate_command; do
  case $rc_id in ''|'#'*) continue ;; esac
  [ "$rc_class" = all ] || [ "$rc_class" = "$rc_gate_class" ] || continue
  if [ -s "$rc_tmp/only" ] && ! grep -F -x -q -- "$rc_id" "$rc_tmp/only"; then continue; fi
  printf '%s\t%s\t%s\t%s\n' "$rc_id" "$rc_required" "$rc_gate_class" "$rc_gate_command" >> "$rc_tmp/selected"
done < "$rc_gates"
[ -s "$rc_tmp/selected" ] || rc_error 'no gate selected'
[ -z "$(git status --porcelain)" ] || rc_error 'the working tree is not clean; commit or stash first'
rc_head=$(git rev-parse HEAD 2>/dev/null) || rc_error 'cannot resolve HEAD'
rc_short=$(printf '%.7s' "$rc_head")
if [ "$rc_partial" -eq 1 ]; then rc_word=RC_PARTIAL
else
  case $rc_class in
    all) rc_word=RC_RESULT ;;
    ci) rc_word=RC_RESULT_CI ;;
    live) rc_word=RC_RESULT_LIVE ;;
    operator) rc_word=RC_RESULT_OPERATOR ;;
  esac
fi
rc_required_total=$(awk -F '\t' '$2 == "yes" {n++} END {print n+0}' "$rc_tmp/selected")
rc_pass=0 rc_failed=0 rc_skipped=0 rc_info=0 rc_n=0 rc_stopped=0
: > "$rc_tmp/results"
while IFS="$rc_tab" read -r rc_id rc_required rc_gate_class rc_gate_command; do
  rc_n=$((rc_n + 1))
  if [ "$rc_stopped" -eq 1 ]; then
    printf '%s\t%s\t%s\tNOT RUN\t-\n' "$rc_id" "$rc_required" "$rc_gate_class" >> "$rc_tmp/results"
    continue
  fi
  rc_start=$(date +%s)
  sh -c "$rc_gate_command" > "$rc_tmp/output.$rc_n" 2>&1
  rc_exit=$?
  rc_seconds=$(($(date +%s) - rc_start))
  rc_changed=0
  if [ -n "$(git status --porcelain)" ] || [ "$(git rev-parse HEAD 2>/dev/null)" != "$rc_head" ]; then rc_changed=1; fi
  rc_suffix=
  [ "$rc_required" = yes ] || rc_suffix=' (informational)'
  if [ "$rc_changed" -eq 1 ]; then
    rc_result=FAIL
    rc_stopped=1
    printf 'GATE %s FAIL %s s (changed the working tree)%s\n' "$rc_id" "$rc_seconds" "$rc_suffix"
    printf 'rc.sh: gate %s changed the working tree; stopping\n' "$rc_id"
  elif [ "$rc_exit" -eq 0 ]; then
    rc_result=PASS
    printf 'GATE %s PASS %s s%s\n' "$rc_id" "$rc_seconds" "$rc_suffix"
  elif [ "$rc_exit" -eq 77 ]; then
    rc_result=SKIP
    rc_reason=$(awk 'NF {line=$0} END {if (line != "") print substr(line, 1, 120); else print "no reason given"}' "$rc_tmp/output.$rc_n")
    printf 'GATE %s SKIP (%s)%s\n' "$rc_id" "$rc_reason" "$rc_suffix"
  else
    rc_result=FAIL
    printf 'GATE %s FAIL %s s (exit %s)%s\n' "$rc_id" "$rc_seconds" "$rc_exit" "$rc_suffix"
  fi
  if [ "$rc_required" = yes ]; then
    case $rc_result in PASS) rc_pass=$((rc_pass + 1)) ;; FAIL) rc_failed=$((rc_failed + 1)) ;; SKIP) rc_skipped=$((rc_skipped + 1)) ;; esac
  elif [ "$rc_result" != PASS ]; then rc_info=$((rc_info + 1)); fi
  rc_duration=$rc_seconds
  [ "$rc_result" != SKIP ] || rc_duration=-
  printf '%s\t%s\t%s\t%s\t%s\n' "$rc_id" "$rc_required" "$rc_gate_class" "$rc_result" "$rc_duration" >> "$rc_tmp/results"
done < "$rc_tmp/selected"
if [ -n "$rc_summary" ]; then
  {
    printf '%s\n' '| gate | required | class | result | seconds |' '|---|---|---|---|---|'
    awk -F '\t' '{printf "| %s | %s | %s | %s | %s |\n", $1, $2, $3, $4, $5}' "$rc_tmp/results"
  } > "$rc_summary" || rc_error 'cannot write summary'
fi
if [ -n "$rc_evidence" ]; then
  if ! RC_ROOT="$rc_root" RC_TMP="${TMPDIR:-/tmp}" python3 - "$rc_tmp" "$rc_evidence" <<'PY'
import os
from pathlib import Path
import re
import sys

root = os.environ['RC_ROOT']
tmp = os.environ['RC_TMP']
home = os.environ.get('HOME', '')
replacements = []
for value, marker in ((root, '<repo>'), (home, '<home>'), (tmp, '<tmp>')):
    if value:
        variants = sorted({value, os.path.realpath(value)}, key=len, reverse=True)
        replacements.extend((variant, marker) for variant in variants)
def scrub(text):
    for value, marker in replacements:
        text = text.replace(value, marker)
    text = re.sub(r'/(?:Users|home)/[^/\s]*', '<home>', text)
    return re.sub(r'/' + 'private' + r'/(?=var/|tmp/)', '/', text)
results = Path(sys.argv[1], 'results').read_text().splitlines()
with open(sys.argv[2], 'w', encoding='utf-8') as out:
    for index, row in enumerate(results, 1):
        gate, _, _, result, _ = row.split('\t')
        path = Path(sys.argv[1], 'output.' + str(index))
        text = ''.join(path.read_text(encoding='utf-8', errors='replace').splitlines(keepends=True)[-40:]) if path.exists() else ''
        if not text:
            text = '(no output)\n'
        elif not text.endswith('\n'):
            text += '\n'
        out.write('## %s (%s)\n```text\n%s```\n\n' % (gate, result, scrub(text)))
PY
  then rc_error 'cannot write evidence'; fi
fi
if [ "$rc_pass" -eq "$rc_required_total" ] && [ "$rc_stopped" -eq 0 ]; then
  printf '%s: PASSED (%s of %s required gates pass, %s informational gates not passing, commit %s)\n' "$rc_word" "$rc_pass" "$rc_required_total" "$rc_info" "$rc_short"
  exit 0
fi
printf '%s: FAILED (%s of %s required gates pass, %s failed, %s skipped, %s informational gates not passing, commit %s)\n' "$rc_word" "$rc_pass" "$rc_required_total" "$rc_failed" "$rc_skipped" "$rc_info" "$rc_short"
exit 1
