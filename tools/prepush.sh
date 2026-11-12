#!/usr/bin/env bash
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

bench=false
if [[ $# -eq 1 && $1 == --bench ]]; then
    bench=true
elif [[ $# -ne 0 ]]; then
    echo "usage: tools/prepush.sh [--bench]" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "prepush: missing cargo" >&2
    exit 1
fi
if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    echo "prepush: missing cargo-llvm-cov" >&2
    exit 1
fi
if [[ $bench == true ]] && ! command -v jq >/dev/null 2>&1; then
    echo "prepush: missing jq" >&2
    exit 1
fi

echo "prepush: Format"
cargo fmt --check
echo "prepush: Clippy"
cargo clippy --all-targets -- -D warnings
echo "prepush: Keystroke budget"
cargo test -p talaria-app --test review_timing -- --exact keystroke_budget_while_gh_sleeps --list | grep -Fx 'keystroke_budget_while_gh_sleeps: test'
cargo test -p talaria-app --test review_timing -- --exact keystroke_budget_while_gh_sleeps --include-ignored
echo "prepush: Doc tests"
cargo test --workspace --doc
echo "prepush: Coverage"
cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80
echo "prepush: Clippy ban canary"
cargo test -p patterns-check --test boundaries -- --ignored clippy_catches_each_banned_interface
echo "prepush: Whitespace"
git diff --check origin/main...HEAD
echo "prepush: Conflict markers"
! git grep -nE '^(<{7}|={7}|>{7})( |$)' -- . || (echo "conflict marker" && false)

if [[ $bench == true ]]; then
    echo "prepush: Bench build"
    cargo build --release -p talaria-cli -p talaria-bench
    echo "prepush: Bench measure"
    target/release/talaria-bench run --binary target/release/talaria --report bench-report.json
    echo "prepush: Bench pager"
    cargo bench -p talaria-tui --bench pager
    echo "prepush: Bench session picker"
    cargo bench -p talaria-tui --bench session_picker
    echo "prepush: Bench fleet view"
    cargo bench -p talaria-tui --bench fleet_view
    echo "prepush: Bench background output"
    cargo bench -p talaria-core --bench background_output
    echo "prepush: Bench background overlay"
    cargo bench -p talaria-tui --bench background_overlay
    echo "prepush: Bench fleet registry"
    cargo bench -p talaria-core --bench fleet_registry
    jq -e '.median.point_estimate < 1000000' target/criterion/fleet_update_200/new/estimates.json
    echo "prepush: Bench subagent tree"
    cargo bench -p talaria-core --bench subagent_tree
    echo "prepush: Bench subagent panel"
    cargo bench -p talaria-tui --bench subagent_panel
fi
