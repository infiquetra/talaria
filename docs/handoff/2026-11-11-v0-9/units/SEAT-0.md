# SEAT-0: One pre-push script that runs what continuous integration runs on a pull request

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue says what to build. This file is the map of the workflow commands the script copies, and why a few continuous-integration steps are not in the script. It restates no requirement and no test. It refers to them by id.

## What the Ubuntu pull-request job runs

`.github/workflows/ci.yml` starts on every pull request (`ci.yml:3-6`). The job `rust-check` is a matrix of `macos-14` and `ubuntu-latest` (`ci.yml:26-32`). The Ubuntu steps that are product checks, in file order, are:

| Step name in the workflow | Line | Command the script copies |
| --- | --- | --- |
| Format | `ci.yml:63-64` | `cargo fmt --check` (REQ-2 step 1) |
| Clippy | `ci.yml:65-66` | `cargo clippy --all-targets -- -D warnings` (REQ-2 step 2) |
| Test | `ci.yml:67-69` | `cargo test --workspace`, and only when `matrix.os != 'ubuntu-latest'`. The script does not run this step. Decision: the script runs the Ubuntu leg on every operating system. The short loop in the guides still names this command. |
| Check the native Linux keystroke budget | `ci.yml:70-76` | The `--list` pipe and the `--include-ignored` invocation (REQ-2 step 3). The script runs this on every operating system. |
| Test native Linux workspace doctests | `ci.yml:77-103` | `cargo test --workspace --doc` (`ci.yml:85`) is REQ-2 step 4. The list-file Python is not copied. |
| Measure line coverage (floor 80%) | `ci.yml:104-106` | `cargo llvm-cov --workspace --ignore-filename-regex /generated/ --fail-under-lines 80` (REQ-2 step 5) |
| Clippy ban canary | `ci.yml:107-108` | `cargo test -p patterns-check --test boundaries -- --ignored clippy_catches_each_banned_interface` (REQ-2 step 6) |
| Whitespace | `ci.yml:109-110` | `git diff --check origin/main...HEAD` (REQ-2 step 7). The checkout uses `fetch-depth: 0` so `origin/main` exists (`ci.yml:38-39`). |
| Conflict markers | `ci.yml:111-113` | `! git grep -nE '^(<{7}|={7}|>{7})( |$)' -- . || (echo "conflict marker" && false)` (REQ-2 step 8) |

## How the script starts

The script is bash, with `set -euo pipefail`, and it is executable. Its first action after that is `cd "$(git rev-parse --show-toplevel)"`. It then checks that `cargo` and `cargo-llvm-cov` are on `PATH`, and that `jq` is on `PATH` when `--bench` was given. A missing tool prints `prepush: missing <name>` and the script exits. It does not install `cargo-llvm-cov` or `jq`. The workflow installs `cargo-llvm-cov` in the Ubuntu job (`ci.yml:44-48`). That install step is not copied.

The drift test normalizes a workflow before comparing. A backslash at the end of a line is removed and the next line is joined with one space. A folded `run: >` block is joined into one line the same way. Runs of spaces collapse to one space. A command matches when the normalized text contains the command string as a substring. The bench measure command is three physical lines in `bench.yml:44-47` and one command after that join. The keystroke `--list` line and the `grep -Fx` line in `ci.yml:74-75` are two substrings after the backslash is dropped. The doc command is the substring `cargo test --workspace --doc` of `ci.yml:85`, which continues `| tee "$result_file"`. The script does not copy the `| tee`. `ci_workflow_runs_every_baseline_command` (`repository.rs:125`) matches some lines as a whole line. This test does not.

## Steps the script does not run

These `run:` steps are on the Ubuntu job and are not in the closed list the issue names (REQ-2). They are scaffolding around the checks.

- Toolchain install, `rustup toolchain install` and `rustup show active-toolchain` (`ci.yml:41-43`).
- Cache-policy unit tests, `python3 -I -m unittest discover -s .github/actions/rust-cache -p 'test_*.py'` (`ci.yml:49-51`).
- Package unit tests, `python3 -I -m unittest discover -s tools/package/tests -t tools/package` (`ci.yml:114-115`).
- The doctest bookkeeping Python that reads `$RUNNER_TEMP` (`ci.yml:86-102`). REQ-2 step 4 is only `cargo test --workspace --doc`.
- Write the LCOV report, `cargo llvm-cov report --lcov --output-path lcov.info` (`ci.yml:118-120`). That step is `if: always()` so it still runs when the floor fails. The script stops at the first failure, so it does not run this report.
- The `rust-coverage` job (`ci.yml:129-140`) runs `test "$RUST_CHECK_RESULT" = success`. That restates the matrix result. It is not a local check.

## Benchmark job

`.github/workflows/bench.yml` runs on every pull request (`bench.yml:2-3`). Job `bench` (`bench.yml:26-29`). REQ-3 copies these `run:` commands and no others:

- `cargo build --release -p talaria-cli -p talaria-bench` (`bench.yml:41-42`)
- `target/release/talaria-bench run --binary target/release/talaria --report bench-report.json` (`bench.yml:44-47`). The workflow hard-codes `target/release`, not a custom target directory.
- `cargo bench -p talaria-tui --bench pager` (`bench.yml:49`)
- `cargo bench -p talaria-tui --bench session_picker` (`bench.yml:51`)
- `cargo bench -p talaria-tui --bench fleet_view` (`bench.yml:53`)
- `cargo bench -p talaria-core --bench background_output` and `cargo bench -p talaria-tui --bench background_overlay` (`bench.yml:55-57`)
- `cargo bench -p talaria-core --bench fleet_registry` and `jq -e '.median.point_estimate < 1000000' target/criterion/fleet_update_200/new/estimates.json` (`bench.yml:58-61`)
- `cargo bench -p talaria-core --bench subagent_tree` (`bench.yml:63`)
- `cargo bench -p talaria-tui --bench subagent_panel` (`bench.yml:65`)

The artifact upload (`bench.yml:66-71`) is `if: always()` and is not a check.

## Where the agreement test lives

`crates/talaria-cli/tests/repository.rs` already reads workflow files from the repository root. `workspace_root` (`repository.rs:4-6`) and `read_repo_file` (`repository.rs:88-92`) are the helpers. `ci_workflow_runs_every_baseline_command` (`repository.rs:125`) already asserts that `ci.yml` still contains `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test --workspace`, the coverage command, and `git diff --check origin/main...HEAD` (`repository.rs:39-45`). The new test is a sibling of that function. It does not edit `BASELINE_COMMANDS`. `saga_profile_declares_the_project_check` (`repository.rs:117`) asserts the parsed `.saga-profile.json` equals `expected_profile` (`repository.rs:47`). REQ-4 does not change that file.

## Guides

The Checks section of `AGENTS.md` is at `AGENTS.md:47-62`. The Checks section of `CLAUDE.md` is at `CLAUDE.md:14-29`. Both already show the three-command block and the coverage command. REQ-4 adds the pre-push sentence and leaves those blocks in place. `CLAUDE.md:37` tells a writer to update the engineering journal. This issue's file list does not include that journal.
