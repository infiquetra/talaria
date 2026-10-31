#!/bin/sh
# Release runner checks. Each case prints one ok line or one failure line.
set -u

HERE=$(CDPATH='' cd -- "$(dirname "$0")" && pwd)
ROOT=$(CDPATH='' cd -- "$HERE/../../.." && pwd)
RC=$ROOT/tools/release/rc.sh
SCAN=$ROOT/tools/release/doc-scan.sh
GATES=$ROOT/tools/release/gates.tsv

NRAN=0
NFAIL=0
CASE=
REPO=
OUT=
EC=
OUTSIDE=

fail() {
  printf 'CASE %s: FAILED: %s\n' "$CASE" "$1"
  NFAIL=$((NFAIL + 1))
}

begin() {
  CASE=$1
  NRAN=$((NRAN + 1))
}

ok() {
  printf 'CASE %s: ok\n' "$CASE"
}

init_repo() {
  REPO=$(mktemp -d)
  git -C "$REPO" init -q
  git -C "$REPO" config user.email test@example.invalid
  git -C "$REPO" config user.name test
  # Test-supplied gate definitions are fixture inputs, not changes made by a gate.
  printf '%s\n' '/tools/release/gates.tsv' >> "$REPO/.git/info/exclude"
  printf 'seed\n' > "$REPO/seed.txt"
  git -C "$REPO" add seed.txt
  git -C "$REPO" commit -q -m seed
}

run_rc() {
  OUT=$(cd "$REPO" && "$RC" "$@" 2>&1)
  EC=$?
}

have() {
  printf '%s\n' "$OUT" | grep -F -q -- "$1"
}

one_line() {
  n=$(printf '%s\n' "$OUT" | grep -c .)
  [ "$n" -eq 1 ]
}

no_gate() {
  ! printf '%s\n' "$OUT" | grep -F -q 'GATE '
}

write_gates() {
  mkdir -p "$REPO/tools/release"
  printf '%s\n' "$1" > "$REPO/tools/release/gates.tsv"
}

pass_row() {
  printf '%s\tyes\t%s\ttrue' "$1" "$2"
}

case_run_all_pass() {
  begin run-all-pass
  init_repo
  write_gates "$(printf '%s\n' \
    "$(pass_row a ci)" \
    "$(pass_row b ci)" \
    "$(pass_row c ci)")"
  run_rc run
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if ! have 'GATE a PASS' || ! have 'GATE b PASS' || ! have 'GATE c PASS'; then
    fail "missing pass lines"
    return
  fi
  if ! have 'RC_RESULT: PASSED (3 of 3 required gates pass'; then
    fail "missing passing result"
    return
  fi
  if [ "$EC" -ne 0 ]; then
    fail "wrong exit code"
    return
  fi
  ok
}

case_run_failure_continues() {
  begin run-failure-continues-and-fails-the-run
  init_repo
  write_gates "$(printf '%s\n' \
    "$(pass_row a ci)" \
    "$(printf 'b\tyes\tci\techo boom; exit 3')" \
    "$(pass_row c ci)")"
  run_rc run
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if ! have 'GATE b FAIL' || ! have '(exit 3)'; then
    fail "missing exit 3 failure"
    return
  fi
  if ! have 'GATE c PASS'; then
    fail "third gate did not run"
    return
  fi
  if ! have 'RC_RESULT: FAILED (2 of 3 required gates pass, 1 failed'; then
    fail "missing failed count"
    return
  fi
  if [ "$EC" -ne 1 ]; then
    fail "wrong exit code"
    return
  fi
  ok
}

case_run_skip() {
  begin run-skip-exit-77-is-not-a-pass
  init_repo
  write_gates "$(printf 's\tyes\tci\techo needs gateway; exit 77')"
  run_rc run
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if ! have 'GATE s SKIP (needs gateway)'; then
    fail "missing skip reason"
    return
  fi
  if ! have '1 skipped'; then
    fail "skip was counted as a pass"
    return
  fi
  if [ "$EC" -ne 1 ]; then
    fail "wrong exit code"
    return
  fi
  ok
}

case_run_informational() {
  begin run-informational-gate-cannot-fail-the-run
  init_repo
  write_gates "$(printf '%s\n' \
    "$(pass_row a ci)" \
    "$(printf 'info\tno\tci\techo bad; exit 1')")"
  run_rc run
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if ! have ' (informational)'; then
    fail "missing informational mark"
    return
  fi
  if ! have 'RC_RESULT: PASSED (1 of 1 required gates pass, 1 informational gates not passing'; then
    fail "informational gate failed the run"
    return
  fi
  if [ "$EC" -ne 0 ]; then
    fail "wrong exit code"
    return
  fi
  ok
}

case_run_words() {
  begin run-result-word-by-selection
  init_repo
  write_gates "$(printf '%s\n' \
    "$(pass_row a ci)" \
    "$(pass_row b live)" \
    "$(pass_row c operator)")"
  run_rc run
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if ! have 'RC_RESULT: PASSED'; then
    fail "missing RC_RESULT word"
    return
  fi
  run_rc run --class ci
  if ! have 'RC_RESULT_CI:'; then
    fail "missing RC_RESULT_CI word"
    return
  fi
  run_rc run --class live
  if ! have 'RC_RESULT_LIVE:'; then
    fail "missing RC_RESULT_LIVE word"
    return
  fi
  run_rc run --class operator
  if ! have 'RC_RESULT_OPERATOR:'; then
    fail "missing RC_RESULT_OPERATOR word"
    return
  fi
  run_rc run --only a
  if ! have 'RC_PARTIAL:'; then
    fail "missing RC_PARTIAL word"
    return
  fi
  copy=$OUTSIDE/gates-copy.tsv
  cp "$REPO/tools/release/gates.tsv" "$copy"
  run_rc run --gates "$copy"
  if ! have 'RC_PARTIAL:'; then
    fail "gates file did not force a partial word"
    return
  fi
  ok
}

case_run_dirty() {
  begin run-refuses-dirty-tree
  init_repo
  write_gates "$(printf 'a\tyes\tci\ttouch marker')"
  printf 'extra\n' > "$REPO/extra.txt"
  run_rc run
  if [ "$EC" -ne 2 ]; then
    fail "wrong exit code"
    return
  fi
  if ! have 'rc.sh: the working tree is not clean; commit or stash first'; then
    fail "missing dirty-tree sentence"
    return
  fi
  if ! no_gate; then
    fail "a gate ran on a dirty tree"
    return
  fi
  if [ -e "$REPO/marker" ]; then
    fail "gate marker exists"
    return
  fi
  rm -f "$REPO/extra.txt"
  printf 'changed\n' >> "$REPO/seed.txt"
  run_rc run
  if [ "$EC" -ne 2 ] || ! have 'rc.sh: the working tree is not clean; commit or stash first'; then
    fail "modified file was not refused"
    return
  fi
  if [ -e "$REPO/marker" ]; then
    fail "gate marker exists"
    return
  fi
  ok
}

case_run_tree_change() {
  begin run-gate-that-changes-the-tree-fails
  init_repo
  printf 'tracked\n' > "$REPO/tracked.txt"
  git -C "$REPO" add tracked.txt
  git -C "$REPO" commit -q -m tracked
  write_gates "$(printf '%s\n' \
    "$(printf 'dirty\tyes\tci\tprintf x >> tracked.txt')" \
    "$(printf 'next\tyes\tci\ttouch marker-next')")"
  run_rc run
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if ! have 'GATE dirty FAIL' || ! have '(changed the working tree)'; then
    fail "missing tree-change failure"
    return
  fi
  if ! have 'rc.sh: gate dirty changed the working tree; stopping'; then
    fail "missing stopping line"
    return
  fi
  if have 'GATE next'; then
    fail "next gate ran after a tree change"
    return
  fi
  if ! have 'RC_RESULT: FAILED' || [ "$EC" -ne 1 ]; then
    fail "wrong exit code"
    return
  fi
  if [ -e "$REPO/marker-next" ]; then
    fail "next gate marker exists"
    return
  fi
  ok
}

expect_usage() {
  want=$1
  why=$2
  if [ "$EC" -ne 2 ]; then
    fail "wrong exit code"
    return 1
  fi
  if ! have "$want"; then
    fail "$why"
    return 1
  fi
  if ! one_line || ! no_gate; then
    fail "usage output was not one plain line"
    return 1
  fi
  return 0
}

case_run_usage() {
  begin run-usage-errors-exit-2
  init_repo
  rm -rf "$REPO/tools"
  run_rc run
  expect_usage 'rc.sh: gates file not found' 'missing gates sentence' || return
  write_gates "$(pass_row a ci)"
  run_rc run --class nosuch
  expect_usage 'rc.sh: unknown class nosuch' 'missing unknown class sentence' || return
  run_rc run --only nosuch
  expect_usage 'rc.sh: unknown gate nosuch' 'missing unknown gate sentence' || return
  write_gates "$(printf 'a\tyes\tci')"
  run_rc run
  expect_usage 'rc.sh: gates file line 1:' 'missing field-count sentence' || return
  write_gates "$(printf '%s\n' "$(pass_row a ci)" "$(pass_row a ci)")"
  run_rc run
  expect_usage 'rc.sh: gates file line 2:' 'missing duplicate sentence' || return
  write_gates "$(printf '%s\n' "$(pass_row a ci)" "$(pass_row b live)")"
  run_rc run --class live --only a
  expect_usage 'rc.sh: no gate selected' 'missing empty selection sentence' || return
  bare=$(mktemp -d)
  OUT=$(cd "$bare" && "$RC" run 2>&1)
  EC=$?
  expect_usage 'rc.sh: not inside a git repository' 'missing outside-git sentence' || return
  init_repo
  write_gates "$(pass_row a ci)"
  mkdir -p "$REPO/inside"
  run_rc run --evidence "$REPO/inside/evidence.md"
  expect_usage 'rc.sh: refusing to write evidence inside the repository' 'missing evidence refusal' || return
  ok
}

case_run_evidence() {
  begin run-evidence-is-scrubbed-and-tailed
  init_repo
  # The gate, rather than this test shell, expands these variables.
  # shellcheck disable=SC2016
  cmd='seq 1 97; pwd; printf "%s\n" "$HOME"; printf "%s\n" "${TMPDIR:-/tmp}"'
  write_gates "$(printf 'g\tyes\tci\t%s' "$cmd")"
  ev=$OUTSIDE/evidence.md
  run_rc run --evidence "$ev"
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if [ ! -f "$ev" ]; then
    fail "evidence was not written"
    return
  fi
  evtext=$(cat "$ev")
  if ! printf '%s\n' "$evtext" | grep -F -x -q '61'; then
    fail "evidence is missing the tailed lines"
    return
  fi
  if ! printf '%s\n' "$evtext" | grep -F -x -q '97'; then
    fail "evidence is missing the tailed lines"
    return
  fi
  if printf '%s\n' "$evtext" | grep -F -x -q '60'; then
    fail "evidence kept a line before the tail"
    return
  fi
  if ! printf '%s\n' "$evtext" | grep -F -q '<repo>' \
    || ! printf '%s\n' "$evtext" | grep -F -q '<home>' \
    || ! printf '%s\n' "$evtext" | grep -F -q '<tmp>'; then
    fail "evidence is missing scrubbed placeholders"
    return
  fi
  home_now=${HOME:-}
  tmp_now=${TMPDIR:-$(printf '/%s' tmp)}
  repo_real=$(cd "$REPO" && pwd -P)
  if [ -n "$home_now" ] && printf '%s\n' "$evtext" | grep -F -q -- "$home_now"; then
    fail "evidence still has a home path"
    return
  fi
  if printf '%s\n' "$evtext" | grep -F -q -- "$REPO" \
    || printf '%s\n' "$evtext" | grep -F -q -- "$repo_real"; then
    fail "evidence still has the repository path"
    return
  fi
  if printf '%s\n' "$evtext" | grep -F -q -- "$tmp_now"; then
    fail "evidence still has the temporary path"
    return
  fi
  for piece in Users home private; do
    prefix=$(printf '/%s' "$piece")
    if printf '%s\n' "$evtext" | grep -F -q -- "$prefix"; then
      fail "evidence still matches a home or private path"
      return
    fi
  done
  folder=$(printf '/%s/%s' var folders)
  if printf '%s\n' "$evtext" | grep -F -q -- "$folder"; then
    fail "evidence still matches a temporary folder"
    return
  fi
  ok
}

case_run_summary() {
  begin run-summary-table
  init_repo
  write_gates "$(printf '%s\n' \
    "$(pass_row a ci)" \
    "$(printf 'b\tyes\tci\techo needs gateway; exit 77')")"
  sum=$OUTSIDE/summary.md
  run_rc run --summary "$sum"
  if ! have 'GATE '; then
    fail "no GATE lines"
    return
  fi
  if [ ! -f "$sum" ]; then
    fail "summary table was not written"
    return
  fi
  if ! grep -F -x -q '| gate | required | class | result | seconds |' "$sum"; then
    fail "summary header is missing"
    return
  fi
  if ! grep -F -x -q '|---|---|---|---|---|' "$sum"; then
    fail "summary separator is missing"
    return
  fi
  rows=$(grep -c '^| ' "$sum")
  if [ "$rows" -ne 3 ]; then
    fail "summary does not have one row per gate"
    return
  fi
  if ! grep -F '| b |' "$sum" | grep -F -q ' - |'; then
    fail "skip row does not show a dash"
    return
  fi
  run_rc run --summary "$sum"
  heads=$(grep -c '| gate | required | class | result | seconds |' "$sum")
  if [ "$heads" -ne 1 ]; then
    fail "summary was appended instead of replaced"
    return
  fi
  ok
}

expect_allowed_ok() {
  text=$1
  if ! have "ALLOWED_DIFF: OK ($text"; then
    fail "missing ALLOWED_DIFF line"
    return 1
  fi
  if [ "$EC" -ne 0 ]; then
    fail "wrong exit code"
    return 1
  fi
  return 0
}

expect_not_allowed() {
  path=$1
  if ! have 'not allowed:'; then
    fail "missing not allowed line"
    return 1
  fi
  if ! have "  $path"; then
    fail "missing refused path"
    return 1
  fi
  if [ "$EC" -ne 1 ]; then
    fail "wrong exit code"
    return 1
  fi
  return 0
}

case_allowed_docs() {
  begin allowed-diff-accepts-documents
  init_repo
  base=$(git -C "$REPO" rev-parse HEAD)
  mkdir -p "$REPO/docs"
  printf 'd\n' > "$REPO/docs/x.md"
  printf 'c\n' > "$REPO/CHANGELOG.md"
  printf 'r\n' > "$REPO/README.md"
  printf 'l\n' > "$REPO/llms.txt"
  git -C "$REPO" add docs/x.md CHANGELOG.md README.md llms.txt
  git -C "$REPO" commit -q -m docs
  tag=$(git -C "$REPO" rev-parse HEAD)
  run_rc allowed-diff "$base" "$tag"
  expect_allowed_ok '4 files)' || return
  ok
}

write_version_tree() {
  ver=$1
  lockver=$2
  mkdir -p "$REPO/crates/a" "$REPO/crates/c"
  cat > "$REPO/Cargo.toml" <<EOF
[workspace]
members = ["crates/a", "crates/c"]

[workspace.package]
version = "$ver"
EOF
  cat > "$REPO/crates/a/Cargo.toml" <<EOF
[package]
name = "a"
version.workspace = true

[dependencies]
b = { path = "../b", version = "$ver" }
EOF
  cat > "$REPO/crates/c/Cargo.toml" <<EOF
[package]
name = "c"
version.workspace = true

[dependencies.b]
path = "../b"
version = "$ver"
EOF
  cat > "$REPO/Cargo.lock" <<EOF
[[package]]
name = "a"
version = "$lockver"

[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
EOF
}

case_allowed_versions() {
  begin allowed-diff-accepts-version-lines
  init_repo
  write_version_tree "0.1.0" "0.1.0"
  git -C "$REPO" add Cargo.toml Cargo.lock crates
  git -C "$REPO" commit -q -m base
  base=$(git -C "$REPO" rev-parse HEAD)
  write_version_tree "0.2.0" "0.2.0"
  git -C "$REPO" add Cargo.toml Cargo.lock crates
  git -C "$REPO" commit -q -m bump
  tag=$(git -C "$REPO" rev-parse HEAD)
  run_rc allowed-diff "$base" "$tag"
  expect_allowed_ok '' || return
  ok
}

reject_pair() {
  path=$1
  init_repo
  mkdir -p "$REPO/$(dirname "$path")"
  printf '%s\n' "$2" > "$REPO/$path"
  git -C "$REPO" add -A
  git -C "$REPO" commit -q -m base
  base=$(git -C "$REPO" rev-parse HEAD)
  printf '%s\n' "$3" > "$REPO/$path"
  git -C "$REPO" add -A
  git -C "$REPO" commit -q -m change
  tag=$(git -C "$REPO" rev-parse HEAD)
  run_rc allowed-diff "$base" "$tag"
  expect_not_allowed "$path"
}

case_allowed_rejects_deps() {
  begin allowed-diff-rejects-dependency-changes
  reject_pair crates/a/Cargo.toml \
    "$(printf '%s\n' '[dependencies]' 'serde = "1"')" \
    "$(printf '%s\n' '[dependencies]' 'serde = "2"')" || return
  reject_pair crates/a/Cargo.toml \
    "$(printf '%s\n' '[dependencies.serde]' 'version = "1"')" \
    "$(printf '%s\n' '[dependencies.serde]' 'version = "2"')" || return
  reject_pair Cargo.toml \
    "$(printf '%s\n' '[workspace.dependencies]' 'serde = "1"')" \
    "$(printf '%s\n' '[workspace.dependencies]' 'serde = "2"')" || return
  reject_pair Cargo.toml \
    "$(printf '%s\n' '[package.metadata.dependencies]' 'local = { path = "../local", version = "1" }')" \
    "$(printf '%s\n' '[package.metadata.dependencies]' 'local = { path = "../local", version = "2" }')" || return
  lock_reg='[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"'
  lock_reg2='[[package]]
name = "serde"
version = "2.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"'
  reject_pair Cargo.lock "$lock_reg" "$lock_reg2" || return
  lock_sum='[[package]]
name = "serde"
version = "1.0.0"
source = "registry+https://github.com/rust-lang/crates.io-index"
checksum = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"'
  reject_pair Cargo.lock "$lock_reg" "$lock_sum" || return
  lock_two='[[package]]
name = "local"
version = "1"
version = "1"'
  lock_one='[[package]]
name = "local"
version = "2"'
  reject_pair Cargo.lock "$lock_two" "$lock_one" || return
  ok
}

case_allowed_rejects_source() {
  begin allowed-diff-rejects-source-and-workflow-changes
  reject_pair src/main.rs 'fn main() {}' 'fn main() { let _ = 1; }' || return
  reject_pair .github/workflows/x.yml 'name: a' 'name: b' || return
  reject_pair fixtures/a.json '{}' '{ }' || return
  init_repo
  printf '[package]\nversion = "1"\n' > "$REPO/Cargo.toml"
  git -C "$REPO" add Cargo.toml
  git -C "$REPO" commit -q -m manifest
  base=$(git -C "$REPO" rev-parse HEAD)
  chmod +x "$REPO/Cargo.toml"
  git -C "$REPO" add Cargo.toml
  git -C "$REPO" commit -q -m mode
  tag=$(git -C "$REPO" rev-parse HEAD)
  run_rc allowed-diff "$base" "$tag"
  expect_not_allowed Cargo.toml || return
  ok
}

case_allowed_usage() {
  begin allowed-diff-usage-errors-exit-2
  init_repo
  head=$(git -C "$REPO" rev-parse HEAD)
  run_rc allowed-diff not-a-commit "$head"
  if [ "$EC" -ne 2 ]; then
    fail "wrong exit code"
    return
  fi
  if ! have 'rc.sh:' || ! one_line; then
    fail "usage output was not one plain line"
    return
  fi
  printf 'other\n' > "$REPO/other.txt"
  git -C "$REPO" add other.txt
  git -C "$REPO" commit -q -m other
  later=$(git -C "$REPO" rev-parse HEAD)
  run_rc allowed-diff "$later" "$head"
  if [ "$EC" -ne 2 ]; then
    fail "wrong exit code"
    return
  fi
  if ! have 'rc.sh:' || ! one_line; then
    fail "ancestor refusal was not one plain line"
    return
  fi
  ok
}

need_id() {
  id=$1
  if ! awk -F '\t' -v id="$id" 'BEGIN { found = 0 } $1 == id { found = 1 } END { exit found ? 0 : 1 }' "$GATES"; then
    fail "missing gate ids"
    return 1
  fi
  return 0
}

case_gates_shape() {
  begin gates-file-shape
  if [ ! -f "$GATES" ]; then
    fail "missing gate ids"
    return
  fi
  ids='fmt
clippy
tests
release-build
replay-fixtures
repo-scan
doc-scan
contract-check
contract-vendor
contract-pinned-drift
contract-latest-drift
bench
no-registry-publish
no-unattended-flag
no-attribution-lines
python-era-removed
release-tooling-tests
smoke-phase1
smoke-phase2a
smoke-phase2b
smoke-phase3
smoke-phase4a
smoke-phase4b
smoke-phase5
live-tests
live-pause'
  oldifs=$IFS
  IFS='
'
  for id in $ids; do
    IFS=$oldifs
    need_id "$id" || return
    IFS='
'
  done
  IFS=$oldifs
  shape=$(awk -F '\t' '
    /^#/ || $0 ~ /^[[:space:]]*$/ { next }
    { n++; if (NF != 4) bad = bad " fields"
      if ($2 != "yes" && $2 != "no") bad = bad " required"
      if ($3 != "ci" && $3 != "live" && $3 != "operator") bad = bad " class"
      if ($4 == "") bad = bad " command"
      if ($0 ~ /\r/) bad = bad " cr"
      if (seen[$1]++) bad = bad " duplicate" }
    END { if (n == 0) bad = bad " empty"; printf "%s", bad }
  ' "$GATES")
  if [ -n "$shape" ]; then
    fail "gates file shape $shape"
    return
  fi
  build=$(awk -F '\t' '$1 == "release-build" { print $4 }' "$GATES")
  if ! printf '%s\n' "$build" | grep -F -q -- '--bins --examples'; then
    fail "release build is missing bins and examples"
    return
  fi
  check=$(awk -F '\t' '$1 == "contract-check" { print $4 }' "$GATES")
  if ! printf '%s\n' "$check" | grep -F -q 'CONTRACT_GEN_REQUIRE_TYPIFY=1'; then
    fail "contract check is missing the typify requirement"
    return
  fi
  locked=$(awk -F '\t' '
    /^#/ || NF != 4 { next }
    { cmd = $4
      n = split(cmd, words, " ")
      i = 1
      while (i <= n) {
        if (words[i] == "cargo" && i < n) {
          subc = words[i + 1]
          if (subc == "build" || subc == "test" || subc == "clippy" || subc == "run" || subc == "metadata") {
            if (index(cmd, "--locked") == 0) print "unlocked"
          }
        }
        i++
      }
    }
  ' "$GATES")
  if [ -n "$locked" ]; then
    fail "a resolving cargo call is missing --locked"
    return
  fi
  live=$(awk -F '\t' '$1 == "live-tests" { print $4 }' "$GATES")
  if ! printf '%s\n' "$live" | grep -F -q 'live_pause_blocks_new_spawns'; then
    fail "live tests do not skip the pause test"
    return
  fi
  pause=$(awk -F '\t' '$1 == "live-pause" { print $4 }' "$GATES")
  if ! printf '%s\n' "$pause" | grep -F -q 'TALARIA_LIVE_PAUSE=1'; then
    fail "live pause does not set its switch"
    return
  fi
  flag=$(awk -F '\t' '$1 == "no-unattended-flag" { print $4 }' "$GATES")
  if ! printf '%s\n' "$flag" | grep -F -q '/**'; then
    fail "flag pathspecs are missing the deep glob"
    return
  fi
  ok
}

case_gates_smoke() {
  begin gates-cover-every-smoke-suite
  for id in smoke-phase1 smoke-phase2a smoke-phase2b smoke-phase3 smoke-phase4a smoke-phase4b smoke-phase5; do
    need_id "$id" || return
  done
  help=$(cd "$ROOT" && cargo run -q --locked -p smoke -- run --help 2>&1)
  suites=$(printf '%s\n' "$help" | grep -oE 'phase[0-9]+[a-z]*' | sort -u)
  if [ -z "$suites" ]; then
    fail "smoke help has no suite list"
    return
  fi
  gates=$(awk -F '\t' '$1 ~ /^smoke-/ { sub(/^smoke-/, "", $1); print $1 }' "$GATES" | sort -u)
  if [ "$suites" != "$gates" ]; then
    fail "smoke suites and gate ids differ"
    return
  fi
  ok
}

case_gates_paths() {
  begin gates-name-real-packages-and-paths
  need_id fmt || return
  meta=$(cd "$ROOT" && cargo metadata --no-deps --format-version 1)
  pkgs=$(printf '%s\n' "$meta" | python3 -I -c 'import json,sys; d=json.load(sys.stdin); print("\n".join(sorted(p["name"] for p in d["packages"])))')
  tests=$(printf '%s\n' "$meta" | python3 -I -c 'import json,sys; d=json.load(sys.stdin); names=set();
[names.add(t["name"]) for p in d["packages"] for t in p.get("targets", []) if "test" in t.get("kind", [])];
print("\n".join(sorted(names)))')
  bins=$(printf '%s\n' "$meta" | python3 -I -c 'import json,sys; d=json.load(sys.stdin); names=set();
[names.add(t["name"]) for p in d["packages"] for t in p.get("targets", []) if "bin" in t.get("kind", [])];
print("\n".join(sorted(names)))')
  cmds=$(awk -F '\t' 'NF == 4 && $1 !~ /^#/ { print $4 }' "$GATES")
  for name in $(printf '%s\n' "$cmds" | grep -oE -- '-p [a-z0-9_-]+' | awk '{ print $2 }' | sort -u); do
    if ! printf '%s\n' "$pkgs" | grep -F -x -q -- "$name"; then
      fail "unknown package $name"
      return
    fi
  done
  for name in $(printf '%s\n' "$cmds" | grep -oE -- '--test [a-z0-9_-]+' | awk '{ print $2 }' | sort -u); do
    if ! printf '%s\n' "$tests" | grep -F -x -q -- "$name"; then
      fail "unknown test target $name"
      return
    fi
  done
  for path in $(printf '%s\n' "$cmds" | grep -oE '(tools|crates)/[A-Za-z0-9_./-]+' | sort -u); do
    if [ ! -e "$ROOT/$path" ]; then
      fail "missing path $path"
      return
    fi
  done
  for bin in $(printf '%s\n' "$cmds" | grep -oE 'target/release/[A-Za-z0-9_-]+' | sed 's#target/release/##' | sort -u); do
    if ! printf '%s\n' "$bins" | grep -F -x -q -- "$bin"; then
      fail "unknown release binary $bin"
      return
    fi
  done
  ok
}

gate_cmd() {
  awk -F '\t' -v id="$1" '$1 == id { print $4 }' "$GATES"
}

case_gate_publish() {
  begin gate-no-registry-publish-detects-a-crate-without-publish-false
  cmd=$(gate_cmd no-registry-publish)
  if [ -z "$cmd" ]; then
    fail "missing gate ids"
    return
  fi
  work=$(mktemp -d)
  mkdir -p "$work/a" "$work/b"
  cat > "$work/Cargo.toml" <<'EOF'
[workspace]
members = ["a", "b"]
resolver = "2"

[workspace.package]
version = "0.0.0"
edition = "2021"
publish = false
EOF
  cat > "$work/a/Cargo.toml" <<'EOF'
[package]
name = "crate-a"
version.workspace = true
edition.workspace = true
publish.workspace = true
EOF
  printf 'pub fn a() {}\n' > "$work/a/lib.rs"
  printf '[lib]\npath = "lib.rs"\n' >> "$work/a/Cargo.toml"
  cat > "$work/b/Cargo.toml" <<'EOF'
[package]
name = "crate-b"
version.workspace = true
edition.workspace = true
publish = false
EOF
  printf 'pub fn b() {}\n' > "$work/b/lib.rs"
  printf '[lib]\npath = "lib.rs"\n' >> "$work/b/Cargo.toml"
  OUT=$(cd "$work" && sh -c "$cmd" 2>&1)
  EC=$?
  if [ "$EC" -ne 0 ]; then
    fail "inherited publish false did not pass"
    return
  fi
  cat > "$work/b/Cargo.toml" <<'EOF'
[package]
name = "crate-b"
version.workspace = true
edition.workspace = true
EOF
  printf '[lib]\npath = "lib.rs"\n' >> "$work/b/Cargo.toml"
  OUT=$(cd "$work" && sh -c "$cmd" 2>&1)
  EC=$?
  if [ "$EC" -eq 0 ] || ! have 'crate-b'; then
    fail "missing publish key was not named"
    return
  fi
  ok
}

plant_flag() {
  body=$1
  R=$(mktemp -d)
  git -C "$R" init -q
  git -C "$R" config user.email test@example.invalid
  git -C "$R" config user.name test
  mkdir -p "$R/crates/a/src"
  printf '%s\n' "$body" > "$R/crates/a/src/lib.rs"
  git -C "$R" add -A
  git -C "$R" commit -q -m plant
  OUT=$(cd "$R" && sh -c "$cmd" 2>&1)
  EC=$?
}

case_gate_flag() {
  begin gate-no-unattended-flag-detects-a-true-value
  cmd=$(gate_cmd no-unattended-flag)
  if [ -z "$cmd" ]; then
    fail "missing gate ids"
    return
  fi
  flag=$(printf '%s%s' un attended)
  plant_flag "$(printf 'let f = Frame { %s: Some(true) };' "$flag")"
  if [ "$EC" -eq 0 ]; then
    fail "optional true form was accepted"
    return
  fi
  plant_flag "$(printf 'params.%s = true;' "$flag")"
  if [ "$EC" -eq 0 ]; then
    fail "assignment true form was accepted"
    return
  fi
  plant_flag "$(printf 'let p = Params::builder().%s(true);' "$flag")"
  if [ "$EC" -eq 0 ]; then
    fail "builder true form was accepted"
    return
  fi
  plant_flag "$(printf 'json!({"%s": true})' "$flag")"
  if [ "$EC" -eq 0 ]; then
    fail "json true form was accepted"
    return
  fi
  plant_flag "$(printf '    %s: true,' "$flag")"
  if [ "$EC" -eq 0 ]; then
    fail "field true form was accepted"
    return
  fi
  plant_flag "// never send it as true here"
  if [ "$EC" -ne 0 ]; then
    fail "a comment was rejected"
    return
  fi
  plant_flag "$(printf 'assert!(!text.contains("%s"));' "$flag")"
  if [ "$EC" -ne 0 ]; then
    fail "an absence assertion was rejected"
    return
  fi
  plant_flag "$(printf 'let %s = false;' "$flag")"
  if [ "$EC" -ne 0 ]; then
    fail "a false value was rejected"
    return
  fi
  ok
}

case_gate_trailer() {
  begin gate-no-attribution-lines-detects-a-trailer
  cmd=$(gate_cmd no-attribution-lines)
  if [ -z "$cmd" ]; then
    fail "missing gate ids"
    return
  fi
  trailer=$(printf '%s%s: person@example.invalid' 'Co-auth' 'ored-by')
  R=$(mktemp -d)
  git -C "$R" init -q
  git -C "$R" config user.email test@example.invalid
  git -C "$R" config user.name test
  printf 'a\n' > "$R/a.txt"
  git -C "$R" add a.txt
  git -C "$R" commit -q -m base
  git -C "$R" tag -a python-final -m tag
  printf 'b\n' > "$R/b.txt"
  git -C "$R" add b.txt
  git -C "$R" commit -q -m 'feat: x' -m "$trailer"
  OUT=$(cd "$R" && sh -c "$cmd" 2>&1)
  EC=$?
  if [ "$EC" -eq 0 ]; then
    fail "a trailer was accepted"
    return
  fi
  git -C "$R" reset -q --hard HEAD^
  printf 'c\n' > "$R/c.txt"
  git -C "$R" add c.txt
  git -C "$R" commit -q -m 'feat: x'
  OUT=$(cd "$R" && sh -c "$cmd" 2>&1)
  EC=$?
  if [ "$EC" -ne 0 ]; then
    fail "a plain message was rejected"
    return
  fi
  S=$(mktemp -d)
  git -C "$S" init -q
  git -C "$S" config user.email test@example.invalid
  git -C "$S" config user.name test
  printf 'd\n' > "$S/d.txt"
  git -C "$S" add d.txt
  git -C "$S" commit -q -m plain
  OUT=$(cd "$S" && sh -c "$cmd" 2>&1)
  EC=$?
  if [ "$EC" -eq 0 ]; then
    fail "a missing tag was accepted"
    return
  fi
  ok
}

case_gate_scan() {
  begin gate-doc-scan-finds-planted-samples
  clean=$(mktemp -d)
  git -C "$clean" init -q
  git -C "$clean" config user.email test@example.invalid
  git -C "$clean" config user.name test
  mkdir -p "$clean/docs"
  printf 'hello\n' > "$clean/docs/ok.md"
  git -C "$clean" add docs/ok.md
  git -C "$clean" commit -q -m clean
  OUT=$(cd "$clean" && env HOME="$OUTSIDE" USER=scan LOGNAME=scan "$SCAN" 2>&1)
  EC=$?
  if ! have 'DOC_SCAN: OK (1 files)' || [ "$EC" -ne 0 ]; then
    fail "missing DOC_SCAN line"
    return
  fi
  dirty=$(mktemp -d)
  git -C "$dirty" init -q
  git -C "$dirty" config user.email test@example.invalid
  git -C "$dirty" config user.name test
  mkdir -p "$dirty/a/b"
  key=$(printf 'sk-%s' "$(printf 'a%.0s' $(seq 1 24))")
  path=$(printf '/%s/%s/x' Users someone)
  printf '%s\n%s\n' "$key" "$path" > "$dirty/a/b/c.md"
  git -C "$dirty" add a/b/c.md
  git -C "$dirty" commit -q -m dirty
  OUT=$(cd "$dirty" && env HOME="$OUTSIDE" USER=scan LOGNAME=scan "$SCAN" 2>&1)
  EC=$?
  if [ "$EC" -ne 1 ]; then
    fail "planted samples did not fail"
    return
  fi
  if ! have 'a/b/c.md:1 sk-key' || ! have 'a/b/c.md:2 home-path'; then
    fail "missing sample rule lines"
    return
  fi
  if have "$key" || have "$path"; then
    fail "output repeated planted text"
    return
  fi
  ok
}

case_workflow() {
  begin workflow-file-shape
  wf=$ROOT/.github/workflows/release-candidate.yml
  if [ ! -f "$wf" ]; then
    fail "missing workflow"
    return
  fi
  for text in \
    'tools/release/**' \
    '.github/workflows/release-candidate.yml' \
    'workflow_dispatch' \
    'contents: read' \
    'rc-tools:' \
    'rc-linux:' \
    'fetch-depth: 0' \
    'RC_TESTS_BASE' \
    'GH_TOKEN' \
    'if: always()' \
    'timeout-minutes: 90'
  do
    if ! grep -F -q -- "$text" "$wf"; then
      fail "workflow is missing $text"
      return
    fi
  done
  if ! awk '
    /^  rc-tools:/ { mode = "tools" }
    /^  rc-linux:/ { mode = "linux" }
    /^  [A-Za-z0-9_-]+:/ && $0 !~ /^  rc-tools:/ && $0 !~ /^  rc-linux:/ { mode = "" }
    mode == "tools" && /timeout-minutes:/ { tools = 1 }
    mode == "linux" && /timeout-minutes: 90/ { linux = 1 }
    END { exit (tools && linux) ? 0 : 1 }
  ' "$wf"; then
    fail "a job is missing its timeout"
    return
  fi
  if ! awk '
    /uses:/ { if ($0 !~ /@[0-9a-fA-F]{40}([^0-9a-fA-F]|$)/) bad = 1 }
    END { exit bad ? 1 : 0 }
  ' "$wf"; then
    fail "an action is not pinned to 40 hex"
    return
  fi
  if git -C "$ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    base=${RC_TESTS_BASE:-origin/main}
    if ! git -C "$ROOT" rev-parse -q --verify "$base" >/dev/null 2>&1; then
      fail "set RC_TESTS_BASE"
      return
    fi
    mb=$(git -C "$ROOT" merge-base HEAD "$base")
    if ! git -C "$ROOT" diff --quiet "$mb" HEAD -- .github/workflows/ci.yml; then
      fail "ci.yml changed since the merge base"
      return
    fi
  else
    # A standalone copy has no history; compare the exact original workflow bytes.
    digest=$(python3 -I -c 'import hashlib,sys; print(hashlib.sha256(open(sys.argv[1],"rb").read()).hexdigest())' "$ROOT/.github/workflows/ci.yml")
    if [ "$digest" != 89521401838a52f5a61300095a61d2cf94f035d5265235c1c5ede380a52c2fc2 ]; then
      fail "ci.yml differs from the standalone-copy baseline"
      return
    fi
  fi
  ok
}

OUTSIDE=$(mktemp -d)

case_run_all_pass
case_run_failure_continues
case_run_skip
case_run_informational
case_run_words
case_run_dirty
case_run_tree_change
case_run_usage
case_run_evidence
case_run_summary
case_allowed_docs
case_allowed_versions
case_allowed_rejects_deps
case_allowed_rejects_source
case_allowed_usage
case_gates_shape
case_gates_smoke
case_gates_paths
case_gate_publish
case_gate_flag
case_gate_trailer
case_gate_scan
case_workflow

if [ "$NRAN" -ne 23 ]; then
  printf 'RC_TESTS: FAILED (%s failed)\n' "$NFAIL"
  exit 1
fi
if [ "$NFAIL" -eq 0 ]; then
  printf 'RC_TESTS: PASSED (23 cases)\n'
  exit 0
fi
printf 'RC_TESTS: FAILED (%s failed)\n' "$NFAIL"
exit 1
