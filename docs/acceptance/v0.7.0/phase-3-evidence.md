# Phase 3 acceptance evidence

## AUTO-1

Candidate: 8b514e6. Runner: t70-c1-dev-2. Run 2 of 2. Date: 2026-10-26.

Result: PASS.

Run 1 FAILED because the Codex runner inherited `NO_COLOR=1`: the appearance test expected gold RGB text but terminal color output was disabled. Formatting and strict Clippy had passed. No candidate files changed. The operator appended `unset NO_COLOR` to the shared settings before the remaining offline commands. Run 2 was explicitly approved before execution in [the issue comment](https://github.com/<account>/<repo-1>/issues/248#issuecomment-6428809326). The final row result uses Run 2; Run 1 is retained below.

```text
$ gh run view 40410354801 -R <account>/<repo-1> --json databaseId,headSha,status,conclusion
{"conclusion":"success","databaseId":40410354801,"headSha":"8b514e65f9aab5d579fbe788ed7c482a0ce19e13","status":"completed"}
exit=0
```

```text
$ gh run watch 40410354801 -R <account>/<repo-1> --interval 30 --exit-status
  ✓ Transcript pager budgets
  ✓ Session picker budgets
  ✓ Fleet view budgets
  ✓ Background output and overlay budgets
  ✓ Fleet registry budget
  ✓ Subagent tree budgets
  ✓ Subagent panel budget
  ✓ Keep the report
  ✓ Post Run actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09
  ✓ Complete job

ANNOTATIONS
- "The ubuntu-latest label will migrate to Ubuntu 26 beginning October 19, 2026. For more information, see https://github.com/actions/runner-images/issues/14748"
bench: .github#1

exit=0
```

```text
$ gh api --paginate repos/<account>/<repo-1>/commits/8b514e65f9aab5d579fbe788ed7c482a0ce19e13/check-runs --jq '.check_runs[] | "\(.name) \(.conclusion)"'
bench success
rust-check (ubuntu-latest) success
rust-check (macos-14) success
rust-coverage success
exit=0
```

Run 1 — FAILED (runner environment; `NO_COLOR=1`).

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
---- scrollback_and_streaming_cell_keep_their_theme stdout ----

thread 'scrollback_and_streaming_cell_keep_their_theme' (114403124) panicked at crates/talaria-app/tests/appearance_flow.rs:794:5:
assertion `left == right` failed
  left: Named(Foreground)
 right: Spec(Rgb { r: 255, g: 215, b: 0 })
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace


failures:
    scrollback_and_streaming_cell_keep_their_theme

test result: FAILED. 7 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s

error: test failed, to rerun pass `-p talaria-app --test appearance_flow`
exit=101
```

```text
$ shasum -a 256 target/release/talaria
76d4b99642869d73db0ce578d7b13e675c31ebfe2de3d3fcdcef6b00d2672e6d  target/release/talaria
exit=0
```

Run 2 — PASSED (approved rerun after sourcing settings with `NO_COLOR` unset).

```text
$ cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests talaria_tui

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests talaria_workflow

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ shasum -a 256 target/release/talaria
76d4b99642869d73db0ce578d7b13e675c31ebfe2de3d3fcdcef6b00d2672e6d  target/release/talaria
exit=0
```

## AUTO-2

Candidate: 8b514e6. Runner: t70-c1-dev-2. Run 1 of 1. Date: 2026-10-26.

Result: PASS.

```text
$ cargo test -p talaria-app --test fleet_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 4.24s
     Running tests/fleet_replay.rs (target/debug/deps/fleet_replay-66303ba2bca29e1e)

running 1 test
test recorded_fleet_fixture_replays ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

exit=0
```

```text
$ cargo test -p talaria-app --test peek_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.80s
     Running tests/peek_replay.rs (target/debug/deps/peek_replay-60ba1b63a6707050)

running 1 test
test recorded_peek_fixture_replays ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-app --test launch_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.17s
     Running tests/launch_replay.rs (target/debug/deps/launch_replay-685d95ee0831cc68)

running 1 test
test mock_replies_match_recorded_frames ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s

exit=0
```

```text
$ cargo test -p talaria-app --test subagent_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.03s
     Running tests/subagent_replay.rs (target/debug/deps/subagent_replay-8322d34ba67b7d2e)

running 2 tests
test recorded_control_replays ... ok
test recorded_batch_replays ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

exit=0
```

```text
$ cargo test -p talaria-app --test background_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.73s
     Running tests/background_replay.rs (target/debug/deps/background_replay-fca8e374b7151dd1)

running 1 test
test recorded_background_work_reduces_to_the_observed_result ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-app --test attention_fleet_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.10s
     Running tests/attention_fleet_replay.rs (target/debug/deps/attention_fleet_replay-7b66471ea670be98)

running 1 test
test recorded_fleet_fixture_gives_expected_alerts ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

exit=0
```

```text
$ cargo test -p talaria-app --test model_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.58s
     Running tests/model_replay.rs (target/debug/deps/model_replay-64638c96b5def9fb)

running 1 test
test recorded_switch_reduces_to_expected_notices ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-app --test directory_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.56s
     Running tests/directory_replay.rs (target/debug/deps/directory_replay-540769d00524350c)

running 1 test
test every_recorded_row_reads ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-app --test queue_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.57s
     Running tests/queue_replay.rs (target/debug/deps/queue_replay-4a22c5e3cfc81a95)

running 1 test
test fixture_final_cells ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

exit=0
```

```text
$ cargo test -p talaria-app --test rewind_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.49s
     Running tests/rewind_replay.rs (target/debug/deps/rewind_replay-e0f91b93fa441c4e)

running 1 test
test recorded_rewind_and_rollback_frames_drive_expected_screens ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-app --test context_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.56s
     Running tests/context_replay.rs (target/debug/deps/context_replay-717cadc1177d786e)

running 1 test
test test_39_recorded_replies_parse_and_read ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-app --test automation_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.67s
     Running tests/automation_replay.rs (target/debug/deps/automation_replay-db24b9b8a48da827)

running 1 test
test recorded_commands_reduce_to_expected_rows_and_cells ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-app --test permissions_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.63s
     Running tests/permissions_replay.rs (target/debug/deps/permissions_replay-773af670a3cfecc3)

running 1 test
test recorded_session_reduces_to_expected_state ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-app --test headless_flow mock_completion_matches_recorded_shape
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.48s
     Running tests/headless_flow.rs (target/debug/deps/headless_flow-1a0ca37c57f14bcc)

running 1 test
test mock_completion_matches_recorded_shape ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 7 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-app --test tool_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.96s
     Running tests/tool_replay.rs (target/debug/deps/tool_replay-0d2da53155701aaf)

running 1 test
test recorded_tool_fixtures_render_expected_screens ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-tui --test pager_replay
   Compiling hermes-client v0.7.0-dev (<worktree>/crates/hermes-client)
   Compiling proptest v1.11.0
   Compiling ratatui-crossterm v0.1.2
   Compiling ratatui-macros v0.7.2
   Compiling ratatui v0.30.2
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 11.83s
     Running tests/pager_replay.rs (target/debug/deps/pager_replay-87ef77fc7f446283)

running 1 test
test every_fixture_prompt_is_findable ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

exit=0
```

```text
$ cargo test -p talaria-app --test commands_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.99s
     Running tests/commands_replay.rs (target/debug/deps/commands_replay-0270e3ae085e2327)

running 1 test
test slash_fixture_replays_to_expected_cells ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s

exit=0
```

```text
$ cargo test -p talaria-app --test attachments_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.56s
     Running tests/attachments_replay.rs (target/debug/deps/attachments_replay-31fe560804cac125)

running 1 test
test replays_recorded_mention_and_image ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-app --test side_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.50s
     Running tests/side_replay.rs (target/debug/deps/side_replay-265fbe4b3e4ab4d7)

running 1 test
test recorded_shell_and_btw_reduce_to_expected_cells ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-tui --test checklist_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.07s
     Running tests/checklist_replay.rs (target/debug/deps/checklist_replay-646a4a2270d12a73)

running 2 tests
test hand_built_frames_render_panel_then_block ... ok
test recorded_todo_fixture_renders_panel_then_block ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

exit=0
```

```text
$ cargo test -p talaria-app --test appearance_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.54s
     Running tests/appearance_replay.rs (target/debug/deps/appearance_replay-7931371feebcb28a)

running 1 test
test recorded_describe_skin_resolves ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-app --test attention_replay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 1.41s
     Running tests/attention_replay.rs (target/debug/deps/attention_replay-2a5248bd09a15cf2)

running 4 tests
test masked_request_opens_and_closes_a_need ... ok
test snapshot_prompt_blocks_without_alert ... ok
test approval_fixture_drives_states_and_one_alert ... ok
test local_masked_answer_closes_need_through_update ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s

exit=0
```

## AUTO-3

Candidate: 8b514e6. Runner: t70-c1-dev-2. Run 1 of 1. Date: 2026-10-26.

Result: PASS.

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-core --test fleet_rows --test fleet_queue --test fleet_registry --test fleet_subscribed
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text

running 3 tests
test roster_row_reads_contract_fields ... ok
test outcome_from_turn_status ... ok
test live_items_decode_from_raw_reply ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/fleet_subscribed.rs (target/debug/deps/fleet_subscribed-c0996df8d776a273)

running 1 test
test view_from_session_state ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-app --test fleet_wire --test fleet_foreground --test fleet_app
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
```

```text
test host_scope_refused_falls_back_to_one_profile ... ok
test outcome_probe_uses_epoch_and_tail ... ok
test fleet_link_sends_only_allowed_methods ... ok
test server_requests_are_refused ... ok
test outcome_probe_unknown_cases ... ok
test waiting_session_reads_pending_approvals ... ok
test never_reads_a_session_that_is_not_live ... ok
test sessions_changed_coalesces_and_respects_gaps ... ok
test hot_and_cold_polls_follow_their_intervals ... ok
test reconnect_marks_stale_then_refreshes ... ok
test one_failing_profile_keeps_the_others ... ok
test backstop_rediscovers_profiles ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.96s

exit=0
```

```text
$ cargo test -p talaria-cli --test fleet_cmd
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-cli v0.7.0-dev (<worktree>/crates/talaria-cli)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 3.04s
     Running tests/fleet_cmd.rs (target/debug/deps/fleet_cmd-ac520bf2ec6f86c4)

running 4 tests
test exit_codes_and_messages ... ok
test profile_filter_hides_other_profiles ... ok
test text_and_json_output ... ok
test watch_lines_and_flag_rules ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.83s

exit=0
```

```text
$ cargo test -p talaria-core --test fleet_view_rows --test fleet_view_keys
test fleet_view_invariants ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s

     Running tests/fleet_view_rows.rs (target/debug/deps/fleet_view_rows-88051dcb19e6f4ef)

running 5 tests
test visible_groups_order_counts_and_omit_empty ... ok
test unchanged_snapshot_needs_no_frame ... ok
test row_detail_matches_fleet_list_vocabulary ... ok
test cursor_follows_its_session ... ok
test filter_and_profile_cycle ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-app --test peek_wire --test fleet_view_flow
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.04s
```

```text
test stop_stale_generation_is_reported_once ... ok
test peek_logs_no_ticket_path_or_reply_text ... ok
test reply_to_running_session_is_queued ... ok
test peek_link_sends_only_allowed_methods ... ok
test peek_moves_detach_without_end_reason ... ok
test answer_from_peek_sends_exact_respond ... ok
test stop_denies_then_interrupts_with_generation ... ok
test peek_link_never_refuses_server_requests ... ok
test actions_wait_for_an_opening_peek ... ok
test closing_overlay_detaches_and_closes_link ... ok
test reply_failures_and_resend ... ok
test peek_failures_keep_the_view ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.05s

exit=0
```

```text
$ cargo test -p talaria-tui --test fleet_view_screens
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.91s
     Running tests/fleet_view_screens.rs (target/debug/deps/fleet_view_screens-390342ef8d0ff3c8)

running 6 tests
test untrusted_text_is_sanitized ... ok
test overlay_leaves_nothing_behind ... ok
test narrow_widths_drop_columns ... ok
test groups_and_columns_160x48 ... ok
test empty_loading_scope_and_failure_states ... ok
test peek_panel_tail_card_and_outcome ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-app --test launch_names --test launch_machine --test launch_outcome --test worktree_git --test launch_wire --test launch_tui_flow
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s
```

```text
test refuses_outside_repo_bare_repo_and_no_origin ... ok
test failed_fetch_without_cache_refuses ... ok
test rollback_removes_only_its_own_clean_tree ... ok
test failed_fetch_uses_cached_ref_with_warning ... ok
test tree_starts_at_fresh_origin_not_local_head ... ok
test worktreeinclude_copies_links_and_skips_escapes ... ok
test default_branch_from_ls_remote_when_origin_head_unset ... ok
test record_round_trip ... ok
test branch_has_no_upstream ... ok
test excludes_without_touching_tracked_files ... ok
test name_race_retries_once ... ok
test linked_worktree_launch_uses_main_root ... ok

test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.61s

exit=0
```

```text
$ cargo test -p talaria-tui --test launch_dialog
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.56s
     Running tests/launch_dialog.rs (target/debug/deps/launch_dialog-68fff9971cfcf733)

running 4 tests
test untrusted_text_is_drawn_literally ... ok
test keys_edit_cycle_launch_and_close ... ok
test renders_at_80_and_160 ... ok
test invalid_busy_launching_failed_states ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-cli --test bg_cmd
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-cli v0.7.0-dev (<worktree>/crates/talaria-cli)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.30s
     Running tests/bg_cmd.rs (target/debug/deps/bg_cmd-6de12576ee0290b8)

running 5 tests
test prompt_from_standard_input ... ok
test exit_codes_and_messages_against_mock ... ok
test signal_before_submit_rolls_back ... ok
test usage_errors ... ok
test launch_time_budget ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.41s

exit=0
```

```text
$ cargo test -p hermes-client --test fixture_scan
   Compiling regex v1.13.1
   Compiling proptest v1.11.0
   Compiling tokio-tungstenite v0.26.2
   Compiling hermes-client v0.7.0-dev (<worktree>/crates/hermes-client)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 22.56s
     Running tests/fixture_scan.rs (target/debug/deps/fixture_scan-f9296448db13df4e)

running 4 tests
test no_home_login_name_or_host_name ... ok
test only_allowed_profile_names ... ok
test no_home_directory_shaped_paths ... ok
test no_secret_shaped_strings ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-core --test subagent_tree --test subagent_batch --test commands_local_agents
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test queued_row_expires_after_ten_minutes ... ok
test absent_twice_is_gone_queued_exempt ... ok
test only_spawn_and_start_create_rows ... ok
test terminal_state_is_never_overwritten ... ok
test fields_merge_one_by_one ... ok
test tree_order_and_dangling_parents ... ok
test event_payload_reads_contract_fields ... ok
test counts_and_version ... ok
test activity_keeps_24_and_drops_repeats ... ok
test status_words_map_and_complete_fails_closed ... ok
test retired_ids_never_return ... ok
test invariants::tree_invariants ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.28s

exit=0
```

```text
$ cargo test -p talaria-app --test subagent_wire
test dropped_call_is_unknown ... ok
test switch_and_background_batches ... ok
test delegation_status_missing_shows_unknown ... ok
test a_status_read_started_before_pause_cannot_undo_the_pause_answer ... ok
test elapsed_ticks_only_while_the_open_panel_has_live_rows ... ok
test steer_trims_and_interprets ... ok
test only_allowed_methods_are_sent ... ok
test stop_all_needs_second_press ... ok
test pause_confirms_and_resume_does_not ... ok
test tail_polls_while_open_only ... ok
test stop_on_queued_or_finished_row_sends_nothing ... ok
test poll_only_while_live_or_open ... ok

test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s

exit=0
```

```text
$ cargo test -p talaria-tui --test subagent_view
     Running tests/subagent_view.rs (target/debug/deps/subagent_view-49bccb30d8968dd6)

running 9 tests
test dock_shows_then_clears ... ok
test untrusted_text_is_literal ... ok
test frames_file_matches_contract ... ok
test panel_list_snapshot_160x48 ... ok
test dock_rows_overflow_and_short_terminal ... ok
test panel_keys_drive_modes ... ok
test batch_block_exact_lines ... ok
test panel_detail_tail_and_steer_snapshots ... ok
test widths::no_row_wider_than_terminal ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-core --test background_process --test background_props --test background_bg --test background_commands --test background_clock
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 218.97s
```

```text

running 9 tests
test output::snapshot_merge_follows_the_tail_rules ... ok
test output::closing_the_view_does_not_stop_the_process ... ok
test output::the_live_line_changes_on_the_clock_only ... ok
test output::chunks_for_an_unknown_id_wait ... ok
test output::output_memory_drops_the_least_recent ... ok
test output::the_total_output_cap_keeps_the_selected_buffer ... ok
test output::output_keeps_the_last_quarter_million_characters ... ok
test output::a_second_list_with_a_fresh_request_is_quiet ... ok
test output::snapshot_merge_matches_the_concatenated_tail has been running for over 60 seconds
test output::snapshot_merge_matches_the_concatenated_tail ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 244.84s

exit=0
```

```text
$ cargo test -p talaria-tui --test background_render
running 11 tests
test view::a_closed_overlay_does_not_repaint_on_a_chunk ... ok
test view::a_small_terminal_shows_the_widen_note ... ok
test view::a_wide_row_keeps_the_suffix_until_it_cannot ... ok
test view::overlay_keys_select_refresh_dismiss_and_close ... ok
test view::an_empty_section_has_no_height ... ok
test view::live_rows_use_the_settled_text ... ok
test view::a_result_cell_shows_the_answer_and_the_footer ... ok
test view::reopening_keeps_the_displayed_and_protected_process_together ... ok
test view::hostile_output_cannot_change_the_terminal ... ok
test view::the_overlay_matches_the_worked_layout ... ok
test view::kill_and_stop_need_a_second_press ... ok

test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

```text
$ cargo test -p talaria-app --test background_service
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.62s
     Running tests/background_service.rs (target/debug/deps/background_service-a069f66f9fdffed7)

running 8 tests
test wire::call_failures_map_onto_the_notes ... ok
test a_missing_recovery_closes_both_process_overlay_states ... ok
test a_background_reply_from_an_older_connection_is_dropped ... ok
test unknown_output_uses_current_app_time_without_a_frame_tick ... ok
test wire::an_open_kill_reply_retains_a_scope_stop_warning ... ok
test wire::idle_work_schedules_nothing ... ok
test wire::only_the_foreground_session_is_listed ... ok
test wire::calls_send_only_the_method_keys ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-core --test fleet_attention --test alert_gate
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
```

```text
test same_need_id_alerts_once ... ok
test turn_marks_follow_turns ... ok
test finished_outcomes_and_threshold ... ok
test scopes_all_watched_off ... ok
test labels_title_fallback_profile_prefix_and_cut ... ok
test startup_notice_once_after_quiet_window ... ok
test quiet_window_waits_for_first_listing ... ok
test quiet_window_after_connect_and_reconnect ... ok
test need_phrase_matches_foreground_tracker ... ok
test needs_you_texts_for_every_kind ... ok
test watched_memory_from_foreground_or_subscribed ... ok
test prop_never_alerts_foreground_or_twice_per_need ... ok

test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.57s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-tui --test status_fleet_segments --test status_line --test attention_keys --test fleet_alert_bytes --test keymap_doc
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test model_label_strips_provider_path_and_joins_effort ... ok
test palette_follows_tone_order_and_default_uses_terminal_colours ... ok
test section_is_one_row_and_blank_when_empty ... ok
test folder_abbreviates_home_and_caps_long_paths ... ok
test snapshot_60_columns ... ok
test snapshot_100_columns ... ok
test snapshot_160_columns ... ok
test priority_drops_lowest_first_at_each_width ... ok
test untrusted_text_is_shown_literally ... ok
test last_segment_is_cut_with_an_ellipsis_and_never_overflows ... ok
test prop_widening_never_removes_a_segment ... ok
test prop_visible_segments_are_a_priority_prefix_within_width ... ok

test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-app --test attention_fleet --test config_fleet_attention --test config_doc
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
```

```text
     Running tests/config_doc.rs (target/debug/deps/config_doc-ff518f98aca6a92e)

running 2 tests
test pane_settings_accept_each_option_and_fall_back_independently ... ok
test doc_default_shape_loads_cleanly ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

     Running tests/config_fleet_attention.rs (target/debug/deps/config_fleet_attention-d989829d93dbb878)

running 1 test
test fleet_notification_keys_defaults_and_bad_values ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-core --test review_parse --test review_state --test review_ship --test review_pr --test review_props
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.90s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
     Running tests/review_state.rs (target/debug/deps/review_state-aa2b711bcc4805b3)

running 9 tests
test unchanged_listing_needs_no_frame ... ok
test esc_unwinds_and_ctrl_c_never_acts ... ok
test cursor_follows_path_across_reloads ... ok
test late_draft_ignored_after_esc ... ok
test commit_editor_header_stage_all_and_warnings ... ok
test branch_scope_is_read_only ... ok
test discard_needs_second_key ... ok
test one_mutation_at_a_time ... ok
test caps_cuts_loading_and_empty_states ... ok

test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-app --test review_git --test review_gh --test review_locate --test review_draft --test review_flow --test review_timing
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.58s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.04s
```

```text
running 3 tests
test missing_folder_texts ... ok
test recorded_tree_answers_first_and_a_missing_one_says_so ... ok
test record_then_foreground_then_session_info ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s

     Running tests/review_timing.rs (target/debug/deps/review_timing-be0369ac668ab743)

running 2 tests
test open_to_first_list_frame_200_files ... ok
test keystroke_budget_while_gh_sleeps ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.87s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-tui --test review_render --test fleet_pr_column --test status_pr_segment
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text

running 3 tests
test commit_editor_and_small_window ... ok
test list_and_diff_120x40 ... ok
test sanitized_names_short_and_70x20 ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s

     Running tests/status_pr_segment.rs (target/debug/deps/status_pr_segment-9924de689fc32d2b)

running 1 test
test pr_segment_priority ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-core --test fleet_view_pane
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-core v0.7.0-dev (<worktree>/crates/talaria-core)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.48s
     Running tests/fleet_view_pane.rs (target/debug/deps/fleet_view_pane-7a814ff7421bb057)

running 4 tests
test o_with_no_rows_does_nothing ... ok
test o_on_foreground_row_refuses_with_notice ... ok
test o_on_background_row_emits_open_in_pane ... ok
test o_in_filter_reply_and_confirm_stop_is_not_an_open ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-app --test attach_resolve --test attach_wire --test panes_plan --test panes_exec --test fleet_pane_flow
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
```

```text

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.09s

     Running tests/panes_plan.rs (target/debug/deps/panes_plan-e773c8517d928a34)

running 6 tests
test direction_auto_rule ... ok
test detect_rules ... ok
test attach_command_line_quotes_and_refuses ... ok
test parse_new_pane_ids ... ok
test outcome_texts ... ok
test herdr_and_tmux_command_specs ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p talaria-cli --test attach_cli
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-cli v0.7.0-dev (<worktree>/crates/talaria-cli)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 2.60s
     Running tests/attach_cli.rs (target/debug/deps/attach_cli-659040aa46ef08ba)

running 6 tests
test no_terminal_exits_2_before_network ... ok
test usage_errors_exit_2_with_empty_stdout ... ok
test not_found_and_ambiguous_texts_and_exit_codes ... ok
test empty_attached_session_announces_its_title ... ok
test prefix_resolves_then_opens_like_resume ... ok
test fast_path_lists_nothing_and_resumes ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.31s

exit=0
```

```text
$ cargo test -p talaria-tui --test fleet_pane_screens
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.75s
     Running tests/fleet_pane_screens.rs (target/debug/deps/fleet_pane_screens-5b7ce7d155b65b82)

running 2 tests
test pane_footer_uses_the_bound_key_and_omits_unbound_action ... ok
test footer_hint_and_notices_snapshots ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-app -p talaria-cli
test result: ok. 61 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.12s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.26s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.04s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.71s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.06s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.96s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.67s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.72s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.20s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.14s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.39s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.20s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.62s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.76s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.61s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.23s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.66s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.89s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.08s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.48s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.95s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.68s
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.28s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.62s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.76s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.03s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.52s
```

```text

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests talaria_app

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests talaria_cli

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p talaria-tui
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.72s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.77s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.43s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.55s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.53s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.99s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.23s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.37s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test full_detail_shows_arguments_output_and_whole_diff ... ok
test multi_hunk_and_multi_file_structure ... ok
test unfinished_unknown_and_failed_edit_cells ... ok
test diff_capped_at_40_rows_in_compact_full_in_detail ... ok
test succeeded_shell_preview_and_more_line_with_and_without_hint ... ok
test rows_fit_every_width ... ok

test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s

   Doc-tests talaria_tui

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

Result lines, followed by the final 15 output lines:

```text
$ cargo test --workspace
test result: ok. 48 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 26 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.80s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.34s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.67s
test result: ok. 24 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.46s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.23s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.62s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.41s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.87s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 56.89s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 28.15s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.16s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 33.52s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 50.93s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 63.08s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.36s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.14s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.25s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.22s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.80s
test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
test result: ok. 75 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.01s
test result: ok. 59 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.83s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.49s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.34s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.03s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 21.77s
test result: ok. 61 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.13s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.05s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.27s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.05s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.54s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.13s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.74s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.05s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.96s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.17s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.65s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.83s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.84s
```

```text
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.20s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.08s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.35s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.19s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.62s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.32s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.61s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.68s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.80s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.68s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.94s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.08s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.48s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 30 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.82s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.98s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.92s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.63s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.49s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 245.56s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 31.32s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.05s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.44s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.60s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.76s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.03s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 4 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 3 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 6 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 5 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 2 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.52s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 59 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 248.04s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 215.31s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 234.14s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
```

```text
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.16s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.56s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.06s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.07s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 28 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.97s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.46s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 18 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.15s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 44 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
```

```text
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.30s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 35 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 60.32s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.05s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 11.98s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 3.69s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.39s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.53s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 16 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.01s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.50s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.52s
```

```text
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.50s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.48s
test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.36s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.06s
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.18s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
```

```text

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests talaria_tui

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

   Doc-tests talaria_workflow

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

## AUTO-4

Candidate: 8b514e6. Runner: t70-c1-dev-2. Run 1 of 1. Date: 2026-10-26.

Result: PASS.

The subagent benchmark budget rows all pass. Its `rows_in_order` timing is explicitly an informational row under the source unit specification, section 10; it has no limit or pass/fail verdict and is retained below. The release table also explicitly leaves the live-gateway launch metric unmeasured in this offline row.

The release-binary budget table (from its `os:` line):

```text
os: macos arch: aarch64 cpus: 24
bench wiring: streaming tail and composer only (no status line, no prompt cards)
budget | measured | limit | result
launch to first frame | 5.883 ms (min 5.651, max 6.604) | 100 ms | PASS
  attempt 1: 5.883 ms (min 5.651, max 6.604) PASS
keystroke p95, stream at 1x | 0.299 ms | 16 ms | PASS
  attempt 1: 0.299 ms PASS
keystroke max, stream at 1x | 0.329 ms | 33 ms | PASS
  attempt 1: 0.329 ms PASS
keystroke p95, stream at 10x | 0.551 ms | 16 ms | PASS
  attempt 1: 0.551 ms PASS
keystroke max, stream at 10x | 0.911 ms | 33 ms | PASS
  attempt 1: 0.911 ms PASS
bytes per frame, median (unit choice) | 48.000 B | 2048 B | PASS
bytes per frame, late over early (unit choice) | 1.042  | 1.5  | PASS
idle frames and bytes | 0 frames / 0 bytes | 0  | PASS
idle CPU mean (not enforced) | 0.034 % | - | INFO
check: frame count matches the binary's report | 1633 / 1633 | 1 % | PASS
CPU mean while streaming | 1.310 % | 5 % | PASS
  attempt 1: 1.310 % PASS
CPU peak (1 s window) | 2.702 % | 15 % | PASS
  attempt 1: 2.702 % PASS
worst single frame | 3.999 ms | 50 ms | PASS
  attempt 1: 3.999 ms PASS
keystroke p95, stream at 1x [background-chatty] | 0.070 ms | 16 ms | PASS
  attempt 1: 0.070 ms PASS
keystroke max, stream at 1x [background-chatty] | 0.701 ms | 33 ms | PASS
  attempt 1: 0.701 ms PASS
bytes per frame, median (unit choice) [background-chatty] | 0.000 B | 2048 B | PASS
bytes per frame, late over early (unit choice) [background-chatty] | 0.000  | 1.5  | PASS
idle frames and bytes [background-chatty] | 0 frames / 0 bytes | 0  | PASS
check: frame count matches the binary's report [background-chatty] | 1 / 1 | 1 % | PASS
CPU mean while streaming [background-chatty] | 1.266 % | 5 % | PASS
  attempt 1: 1.266 % PASS
CPU peak (1 s window) [background-chatty] | 1.879 % | 15 % | PASS
  attempt 1: 1.879 % PASS
worst single frame [background-chatty] | 0.206 ms | 50 ms | PASS
  attempt 1: 0.206 ms PASS
keystroke p95, stream at 1x [queue] | 0.289 ms | 16 ms | PASS
  attempt 1: 0.289 ms PASS
keystroke max, stream at 1x [queue] | 0.311 ms | 33 ms | PASS
  attempt 1: 0.311 ms PASS
bytes per frame, median (unit choice) [queue] | 48.000 B | 2048 B | PASS
bytes per frame, late over early (unit choice) [queue] | 1.042  | 1.5  | PASS
idle frames and bytes [queue] | 0 frames / 0 bytes | 0  | PASS
check: frame count matches the binary's report [queue] | 1634 / 1634 | 1 % | PASS
CPU mean while streaming [queue] | 1.642 % | 5 % | PASS
  attempt 1: 1.642 % PASS
CPU peak (1 s window) [queue] | 2.853 % | 15 % | PASS
  attempt 1: 2.853 % PASS
worst single frame [queue] | 4.543 ms | 50 ms | PASS
  attempt 1: 4.543 ms PASS
resident memory at start | 4.358 MB | 40 MB | PASS
resident memory after 2000 cells | 37.945 MB | 100 MB | PASS
check: no queries after start-up | none late | none | PASS
launch to usable composer (live gateway) | not measured | 200 ms | SKIPPED
counts: unanswered queries 50, bytes outside frames 1550, nested markers 0, unterminated frames 0, largest write lag 7.397 ms
```

```text
$ target/release/talaria-bench run --binary target/release/talaria --report "$X/bench.json" > "$X/bench.txt"
exit=0
```

expected exit 1 for zero FAIL lines.

```text
$ grep -cE ' FAIL$' "$X/bench.txt"
0
exit=1
```

```text
$ grep -cE ' PASS$' "$X/bench.txt"
51
exit=0
```

```text
$ cargo test -p talaria-bench --test real_binary release_budgets_hold -- --ignored
   Compiling tokio v1.54.1
   Compiling tempfile v3.27.0
   Compiling gethostname v1.1.0
   Compiling talaria-bench v0.7.0-dev (<worktree>/tools/bench)
   Compiling tokio-tungstenite v0.26.2
   Compiling hermes-client v0.7.0-dev (<worktree>/crates/hermes-client)
    Finished `test` profile [unoptimized + debuginfo] target(s) in 9.27s
     Running tests/real_binary.rs (target/debug/deps/real_binary-90d55d20ec4e13b1)

running 1 test
test release_budgets_hold has been running for over 60 seconds
test release_budgets_hold ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 163.79s

exit=0
```

```text
$ cargo bench -p talaria-core --bench fleet_registry
   Compiling tinytemplate v1.2.1
   Compiling criterion v0.5.1
   Compiling talaria-core v0.7.0-dev (<worktree>/crates/talaria-core)
    Finished `bench` profile [optimized] target(s) in 40.85s
     Running benches/fleet_registry.rs (target/release/deps/fleet_registry-e1a7f8d71ce7a1ed)
Gnuplot not found, using plotters backend
Benchmarking fleet_update_200
Benchmarking fleet_update_200: Warming up for 3.0000 s
Benchmarking fleet_update_200: Collecting 100 samples in estimated 5.2453 s (66k iterations)
Benchmarking fleet_update_200: Analyzing
fleet_update_200        time:   [41.478 µs 42.632 µs 44.658 µs]
Found 8 outliers among 100 measurements (8.00%)
  5 (5.00%) high mild
  3 (3.00%) high severe

exit=0
```

```text
$ jq -e '.median.point_estimate < 1000000' target/criterion/fleet_update_200/new/estimates.json
true
exit=0
```

```text
$ cargo test -p talaria-app --test fleet_wire -- --exact start_returns_at_once
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.26s
     Running tests/fleet_wire.rs (target/debug/deps/fleet_wire-2fa862764675a030)

running 1 test
test start_returns_at_once ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.06s

exit=0
```

```text
$ cargo test -p talaria-app --test fleet_wire -- --exact lists_every_served_profile_with_exact_params
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.15s
     Running tests/fleet_wire.rs (target/debug/deps/fleet_wire-2fa862764675a030)

running 1 test
test lists_every_served_profile_with_exact_params ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 13 filtered out; finished in 0.05s

exit=0
```

```text
$ cargo bench -p talaria-tui --bench fleet_view
   Compiling proptest v1.11.0
   Compiling ratatui-widgets v0.3.2
   Compiling ratatui-crossterm v0.1.2
   Compiling syntect v5.3.0
   Compiling alacritty_terminal v0.26.0
   Compiling vt100 v0.16.2
   Compiling ratatui-macros v0.7.2
   Compiling ratatui v0.30.2
   Compiling talaria-core v0.7.0-dev (<worktree>/crates/talaria-core)
   Compiling hermes-client v0.7.0-dev (<worktree>/crates/hermes-client)
   Compiling talaria-app v0.7.0-dev (<worktree>/crates/talaria-app)
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `bench` profile [optimized] target(s) in 41.59s
     Running benches/fleet_view.rs (target/release/deps/fleet_view-b9c9ff3c03a9a678)
p50=525us p95=694us worst=924us
exit=0
```

```text
$ cargo test -p talaria-app --test fleet_view_flow -- first_frame idle_and_streaming
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.34s
     Running tests/fleet_view_flow.rs (target/debug/deps/fleet_view_flow-6545a272fea9786d)

running 2 tests
test first_frame_within_50ms_without_network ... ok
test idle_and_streaming_frame_counts ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 4 filtered out; finished in 2.06s

exit=0
```

```text
$ cargo bench -p talaria-core --bench subagent_tree
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-core v0.7.0-dev (<worktree>/crates/talaria-core)
    Finished `bench` profile [optimized] target(s) in 0.89s
     Running benches/subagent_tree.rs (target/release/deps/subagent_tree-d0a9ed95d7c0d064)
apply_tool_200_rows median 0us limit 20us pass
reconcile_200_rows median 36us limit 1000us pass
rows_in_order_200_rows median 9us informational
exit=0
```

```text
$ cargo bench -p talaria-tui --bench subagent_panel
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `bench` profile [optimized] target(s) in 0.78s
     Running benches/subagent_panel.rs (target/release/deps/subagent_panel-cc89031afe65abb0)
panel_render_200_rows median 332us limit 4000us pass
exit=0
```

```text
$ cargo bench -p talaria-core --bench background_output
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-core v0.7.0-dev (<worktree>/crates/talaria-core)
    Finished `bench` profile [optimized] target(s) in 0.50s
     Running benches/background_output.rs (target/release/deps/background_output-df12dca4e12a40a2)
background_output chunk_median_us=0 tail_p95_us=200
exit=0
```

```text
$ cargo bench -p talaria-tui --bench background_overlay
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
   Compiling talaria-tui v0.7.0-dev (<worktree>/crates/talaria-tui)
    Finished `bench` profile [optimized] target(s) in 0.80s
     Running benches/background_overlay.rs (target/release/deps/background_overlay-e24708c352cf859c)
background_overlay render_p95_us=49
exit=0
```

```text
$ cargo test -p talaria-app --test launch_tui_flow
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running tests/launch_tui_flow.rs (target/debug/deps/launch_tui_flow-dba55a8482346a19)

running 4 tests
test n_and_w_open_the_dialog_from_the_fleet_view ... ok
test three_launches_at_most ... ok
test launch_runs_off_the_ui_task ... ok
test fleet_is_told_and_row_appears ... ok

test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.80s

exit=0
```

```text
$ cargo test -p talaria-cli --test bg_cmd
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.18s
     Running tests/bg_cmd.rs (target/debug/deps/bg_cmd-6de12576ee0290b8)

running 5 tests
test prompt_from_standard_input ... ok
test exit_codes_and_messages_against_mock ... ok
test signal_before_submit_rolls_back ... ok
test usage_errors ... ok
test launch_time_budget ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.47s

exit=0
```

```text
$ cargo test -p talaria-app --test attention_fleet -- no_new_frame_when_counts_unchanged
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.15s
     Running tests/attention_fleet.rs (target/debug/deps/attention_fleet-e1cc953c0bd22bc8)

running 1 test
test no_new_frame_when_counts_unchanged ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 8 filtered out; finished in 0.02s

exit=0
```

```text
$ cargo test -p talaria-app --test review_timing
warning: crates/talaria-app/Cargo.toml: version requirement `=1.1.8+spec-1.1.0` for dependency `toml` includes semver metadata which will be ignored, removing the metadata is recommended to avoid confusion
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.14s
     Running tests/review_timing.rs (target/debug/deps/review_timing-be0369ac668ab743)

running 2 tests
test open_to_first_list_frame_200_files ... ok
test keystroke_budget_while_gh_sleeps ... ok

test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.90s

exit=0
```

```text
$ cargo test -p talaria-app --test fleet_pane_flow
warning: `talaria-app` (manifest) generated 1 warning
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running tests/fleet_pane_flow.rs (target/debug/deps/fleet_pane_flow-a6ef8e9aadf148b2)

running 7 tests
test no_multiplexer_shows_command_and_runs_nothing ... ok
test outcome_after_overlay_closed_goes_to_main_screen ... ok
test agents_panel_receives_pane_notices_without_writing_under_the_overlay ... ok
test panel_target_uses_supplied_title_and_acknowledges_only_opened ... ok
test slow_split_never_blocks_the_loop ... ok
test o_shows_opening_then_opened_and_acknowledges ... ok
test second_o_while_in_flight_is_refused ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s

exit=0
```

## AUTO-5

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Each command ran on its own, one at a time, with `"$T" fleet list --profile fixture` read before and after each: no NEEDS YOU and no WORKING group before any of them. Six targets ran once and passed; live_background and live_attention failed on run 1 for reasons outside Talaria and passed on run 2, each rerun approved on the issue first (section 8.1 item 2). Long model sentences in BG_APPROVAL lines are cut at 180 characters; only its first line is kept.

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_fleet -- --ignored --nocapture 2>&1 | tee "$X/l1.txt"
SESSIONS_CHANGED_SIBLING: seen
LIVE_FLEET_RESULT: PASSED
LIVE_FLEET_RESULT: PASSED
LIVE_FLEET_RESULT: PASSED
needs_you_p95_ms=1740
LIVE_FLEET_RESULT: PASSED
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 42.22s
SKIPPED count: 0
```

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_fleet_view -- --ignored --nocapture 2>&1 | tee "$X/l2.txt"
LIVE_FLEET_VIEW_RESULT: PASSED (live_answer_approval_from_peek)
peek_open_ms=105
LIVE_FLEET_VIEW_RESULT: PASSED (live_peek_shows_latest_output)
LIVE_FLEET_VIEW_RESULT: PASSED (live_reply_queues_behind_running_turn)
QUEUED_AFTER_STOP: started
LIVE_FLEET_VIEW_RESULT: PASSED (live_stop_from_fleet)
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 72.81s
SKIPPED count: 0
```

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_bg -- --ignored --nocapture 2>&1 | tee "$X/l3.txt"
LIVE_BG_RESULT: PASSED
LIVE_BG_RESULT: PASSED
LIVE_BG_RESULT: PASSED
LIVE_BG_RESULT: PASSED
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.74s
SKIPPED count: 0
```

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_subagents -- --ignored --nocapture 2>&1 | tee "$X/l4.txt"
LIVE_SUBAGENT_RESULT: SKIPPED (TALARIA_LIVE_PAUSE not set)
LIVE_SUBAGENT_RESULT: PASSED
STEER_RESULT: queued
LIVE_SUBAGENT_RESULT: PASSED
LIVE_SUBAGENT_RESULT: PASSED
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 22.45s
SKIPPED count: 1
```

live_background, run 1 of 2: FAILED. Two of its five tests panicked at `crates/talaria-cli/tests/live_background.rs:94` with `open failed: Refused { reason: "invalid_params" }`. Cause, not a Talaria defect: the test names each session's `request_id` after the current millisecond (`format!("live-background-{}", now_ms())`, line 86) and cargo runs the five tests in parallel; two tests that open in the same millisecond share a `request_id`, and the pinned Hermes (`gateway/session_local.py:294`) refuses a repeated `request_id` whose frozen request (here the working folder) differs. Rerun approved on the issue: `rerun approved: AUTO-5: live_background request_id collision between parallel tests`.

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_background -- --ignored --nocapture 2>&1 | tee "$X/l5.txt"
thread 'live_bg_result_arrives' (113911823) panicked at crates/talaria-cli/tests/live_background.rs:94:33:
open failed: Refused { reason: "invalid_params" }
test live_bg_result_arrives ... FAILED
thread 'live_bg_session_listed_report' (113911825) panicked at crates/talaria-cli/tests/live_background.rs:94:33:
open failed: Refused { reason: "invalid_params" }
test live_bg_session_listed_report ... FAILED
BG_APPROVAL: background.complete The command is sitting at a confirmation prompt — `rm -rf` matched the "recursive delete" safety pattern, so the terminal tool returned `pending_ap
LIVE_BACKGROUND_RESULT: PASSED
TERMINAL_OUTPUT_EVENTS: seen
KILL_LATENCY_MS: 149
LIVE_BACKGROUND_RESULT: PASSED
test result: FAILED. 3 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 18.63s
SKIPPED count: 0
```

live_background, run 2 of 2: PASSED. Same command, with `RUST_TEST_THREADS=1` exported in the shell first so the tests run one after another; the command line is unchanged.

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_background -- --ignored --nocapture 2>&1 | tee "$X/l5.txt"
test live_bg_approval_report ... BG_APPROVAL: background.complete The command is pending approval — the terminal returned `status: pending_approval` for the recursive delete, so it
LIVE_BACKGROUND_RESULT: PASSED
test live_bg_session_listed_report ... BG_SESSION_LISTED: live=no stored=yes
test live_process_streams_and_is_killed ... TERMINAL_OUTPUT_EVENTS: seen
KILL_LATENCY_MS: 153
LIVE_BACKGROUND_RESULT: PASSED
test live_stop_all_reports_count ... LIVE_BACKGROUND_RESULT: PASSED
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 46.19s
SKIPPED count: 0
```

live_attention, run 1 of 2: FAILED. `live_background_approval_rings_once_and_counts` saw two desktop alerts where it expects one (`crates/talaria-cli/tests/live_attention.rs:300`). Cause, not a Talaria defect: another seat was running the phase 3 smoke suite on the same gateway at the same time, and its needs-you request (`rm -rf ./talaria-smoke-probe`, sent to the gateway at 18:18:51, two seconds after this test started) is an approval in the same `fixture` fleet that the watching terminal also alerts on; the test's own request followed at 18:19:09. The second alert's text was not captured, so the cause is read from the timing; run 2, with no other traffic on the gateway, rang once. The test panicked before denying its own request; when the tester resumed that session to deny it, the request had already run out of Hermes's confirmation window unanswered (the command was blocked, nothing deleted), and the fleet was quiet again at 18:20:50. Rerun approved on the issue: `rerun approved: AUTO-5: live_attention concurrent smoke approval from another seat`.

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_attention -- --ignored --nocapture 2>&1 | tee "$X/l6.txt"
LIVE_ATTENTION_RESULT: PASSED
thread 'live_background_approval_rings_once_and_counts' (113959754) panicked at crates/talaria-cli/tests/live_attention.rs:300:5:
assertion `left == right` failed: one desktop alert
  left: 2
 right: 1
test live_background_approval_rings_once_and_counts ... FAILED
test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 63.40s
SKIPPED count: 0
```

live_attention, run 2 of 2: PASSED, after the other seat had stopped. The gateway log shows only this test's own two prompts during the run.

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_attention -- --ignored --nocapture 2>&1 | tee "$X/l6.txt"
LIVE_ATTENTION_RESULT: PASSED
LIVE_ATTENTION_RESULT: PASSED
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 71.45s
SKIPPED count: 0
```

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_review -- --ignored --nocapture 2>&1 | tee "$X/l7.txt"
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.45s
SKIPPED count: 0
```

```text
$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_attach_cmd -- --ignored --nocapture 2>&1 | tee "$X/l8.txt"
LIVE_TALARIA_ATTACH_RESULT: PASSED prefix_ms=1063 title_ms=884 fast_ms=122
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 6.34s
SKIPPED count: 0
```

Counts after the last runs: `grep -c SKIPPED` prints 0 for l1, l2, l3, l5, l6, l7 and l8, and 1 for l4 (the pause test, which the operator runs with P3-04 UAT-6).

## AUTO-6

Candidate: 8b514e6. Runner: t70-c1-dev-2. Run 1 of 1. Date: 2026-10-26.

Result: PASS.

Empty non-negated grep scans normally exit 1. The zero-match FAIL count in AUTO-4 also exits 1; neither is a failed budget. All other command statuses are recorded below.

```text
$ cargo run -q -p record-fixture -- scan fixtures/frames
exit=0
```

```text
$ ! git grep -nE 'unsafe *(\{|fn |impl )' -- crates tools
exit=0
```

```text
$ ! git grep -nE '/(Users|home|private|var)/[A-Za-z]' -- crates tools docs/formats | grep -vF 'docs/formats/bg-result-v1.md:8:'
exit=0
```

```text
$ ! git grep -n 'unattended' -- tools/smoke ':!tools/smoke/tests/typed_input.rs' ':!tools/smoke/tests/typed_input2a.rs' ':!tools/smoke/tests/typed_input2b.rs' ':!tools/smoke/tests/typed_input3.rs' ':!tools/smoke/README.md'
exit=0
```

```text
$ git grep -nE 'end_reason|unattended' -- crates/talaria-app/src crates/talaria-cli/src ':!crates/talaria-app/src/bg/' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
exit=1
```

```text
$ ! grep -rlE '"unattended": *true' fixtures/frames
exit=0
```

```text
$ git grep -nE 'session\.(resume|create|detach|interrupt)|prompt\.submit|approval\.respond|config\.set|unattended' -- crates/talaria-app/src/fleet crates/talaria-cli/src/fleet_cmd.rs | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
exit=1
```

```text
$ git grep -nE 'session\.create|session\.mutate|config\.set|"unattended"|"end_reason"|"queued"[[:space:]]*:' -- crates/talaria-app/src/peek.rs crates/talaria-app/src/fleet_actions.rs 'crates/talaria-core/src/fleet_view*' 'crates/talaria-tui/src/fleet_view*' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
exit=1
```

```text
$ grep -vE '^\s*//' crates/talaria-app/src/bg/wire.rs | grep -nE '\b(title|hidden|cwd_explicit|finite|unattended)\b'
exit=1
```

```text
$ git grep -nE -- '--force|force-with-lease|push -f|"\+refs/|prompt\.submit|"unattended"|"finite"' -- crates/talaria-app/src/review/ crates/talaria-core/src/review/
exit=1
```

```text
$ git grep -nE '"(include_hidden|end_reason|unattended|title)"' -- crates/talaria-app/src/attach.rs crates/talaria-app/src/panes.rs crates/talaria-cli/src/attach_cmd.rs
exit=1
```

```text
$ ! git grep -hoE -e '--profile[ =]+"?[A-Za-z0-9_-]+' -- crates tools | sed -E 's/.*[ =]"?//' | grep -vxE 'default|fixture|nosuchprofile|nosuch|to|the'
exit=0
```

```text
$ target/release/talaria --help | grep -cE -- '--bg|--worktree'
6
exit=0
```

```text
$ target/release/talaria attach --help | head -3
Resume a Hermes session by its id, id prefix, or title.

Usage: talaria attach [OPTIONS] <SESSION>
exit=0
```

```text
$ grep -c 'talaria.fleet.v1' docs/formats/fleet-list-v1.md; grep -c 'talaria.bg.v1' docs/formats/bg-result-v1.md
3
2
exit=0
```

## AUTO-7

Candidate: 8b514e6. Runner: t70-c1-dev-2. Run 1 of 1. Date: 2026-10-26.

Result: PASS.

The phase 2a and phase 2 gates used Shape B; their absent checker pairs are omitted as directed.

Result lines, followed by the final 15 output lines:

```text
$ cargo test -p smoke
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.95s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 54.82s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.89s
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.07s
test result: ok. 13 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 33.29s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 49.47s
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 63.37s
test result: ok. 9 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 3.50s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.11s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.03s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.20s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 0 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 19 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.09s
test result: ok. 22 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s
test result: ok. 23 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 20 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.04s
test result: ok. 45 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.91s
test result: ok. 60 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.04s
test result: ok. 75 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 5.40s
test result: ok. 59 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.82s
test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.19s
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 13.29s
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 4.21s
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 12.50s
```

```text
test slash_lines_get_no_submit_fallback ... ok
test needs_you_types_only_the_digit_found_on_screen ... ok
test no_dangerous_slash_commands_are_typed ... ok
test every_launch_line_carries_no_ensure ... ok
test only_allowed_keys_are_sent ... ok
test nothing_typed_contains_unattended ... ok

test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 26.81s

   Doc-tests smoke

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s

exit=0
```

```text
$ cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$'
live_full_smoke_in_herdr: test
live_phase2a_smoke_in_herdr: test
live_phase2b_smoke_in_herdr: test
live_phase3_smoke_in_herdr: test
exit=0
```

```text
$ cargo run -q -p smoke -- run --help
Run the smoke test

Usage: smoke run [OPTIONS]

Options:
      --talaria <TALARIA>              The binary under test; the default is the workspace build
      --herdr <HERDR>                  The command line of the terminal multiplexer [default: herdr]
      --profile <PROFILE>              The profile the session runs in [default: fixture]
      --model <MODEL>                  The model the session uses [default: minimax-m3.1-flash]
      --provider <PROVIDER>            The provider the model comes from [default: ollama-cloud]
      --deadline-secs <DEADLINE_SECS>  The whole run, in seconds [default: 300 for phase1, 1800 for phase2a, 1500 for phase2b, 1200 for phase3]
      --step-scale <STEP_SCALE>        The scale of the per-step limits; the deadline and the quiet periods stay [default: 1]
      --report-dir <REPORT_DIR>        Where the failure report is written
      --suite <SUITE>                  Which suite runs: phase1, phase2a, phase2b, or phase3 [default: phase1]
      --tmux <TMUX>                    The tmux command line the phase2a suite pastes through [default: tmux]
  -h, --help                           Print help
exit=0
```

```text
$ sh docs/acceptance/v0.7.0/check-record.test.sh | tail -1
CHECK_RECORD_TESTS: PASSED
exit=0
```

```text
$ sh docs/acceptance/v0.7.0/check-record.sh | tail -1
ACCEPTANCE_RESULT: PASSED (70 of 70 rows PASS, candidate f87640c)
exit=0
```

## AUTO-8

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Run 1 of 2 failed at the step `interrupt`, after `startup`, `plain-answer` and `approval` had passed (exit 1). The reason printed was "the count was not running, or was nearly finished, when Esc was sent". Cause, not a Talaria defect: before counting, the model wrote a sketch of the count in its visible reasoning, with the bare lines 1, 2, 3, ... and 2000. The step reads the highest bare number on screen after the prompt (`tools/smoke/src/screen.rs:198`) and refuses anything above 1500, so it read 2000 while the real count stood at 180. Rerun approved on the issue: `rerun approved: AUTO-8: model printed a 1..2000 sketch before counting`. Run 2 below came right after AUTO-12, with no other live scenario in between. The Herdr window measured 102 columns by 41 rows just before run 2; this suite's steps do not depend on the width.

`$T` is `<tmp>/talaria-wt/talaria-launcher`, a two-line script that sets `HERMES_HOME` to the test Hermes home and runs the release binary as a child, as the README advises for a gateway outside the default Hermes home (a new Herdr tab does not inherit the caller's `HERMES_HOME`). The tester's quiet-fleet check (`"$T" fleet list --profile fixture`: no NEEDS YOU and no WORKING group) ran before and after the run.

```text
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # before
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
$ TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria "$T" > "$X/s1.txt" 2>&1; echo "exit=$?"
talaria: <tmp>/talaria-wt/talaria-launcher
herdr: herdr 0.9.4
PASS startup 4.1 s
PASS plain-answer 11.6 s
NOTE submit fallback: sent Enter (step plain-answer)
PASS approval 11.7 s
PASS interrupt 11.4 s
PASS quit 0.3 s
PASS resume 10.6 s
tab: wC6:t2W
SMOKE_RESULT: PASSED (elapsed 49 s)
exit=0
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # after
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
```

## AUTO-9

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Run 5 of 5 passed; its whole output is below. Runs 1 to 4 failed, each for a cause outside Talaria.

Run 1 of 5 failed at the step `history-search` after 9 steps had passed (exit 1), with the reason "Ctrl+R did not list the earlier prompt within 5 s". Cause, not a Talaria defect: the Herdr window was 102 columns wide (a second terminal client was holding the shared layout at that width). The search box did list the earlier prompt, but its row was cut at the box edge before the words the step looks for. The operator widened the window to 170 by 44.

Run 2 of 5 failed at the step `pane-state` after 12 steps had passed (exit 1), with the reason "fewer than 15 counting lines appeared within 60 s". Cause, not a Talaria defect: the model counted with its code tool (one tool cell, `+1997 lines` folded) instead of writing the numbers as text, as the prompt asks.

Run 3 of 5 failed at the step `paste-editor` after 19 steps had passed (exit 1), with the reason "the reply did not contain the marker from the second paste within 60 s". Cause, not a Talaria defect: in that pane the model never answered. It loaded an agent-authored skill left in the `fixture` profile by earlier live runs (`talaria-fixture-check`), read the smoke suite's own source and used `skill_manage` to add a note to that skill. The operator then removed four such leftover skills and the profile's skills prompt snapshot.

Run 4 of 5 failed at the step `history-search` again, after 9 steps had passed (exit 1), with the same reason as run 1, in a 170 by 44 window with the leftover skills removed. Cause, not a Talaria defect: the step counts visible lines that hold the earlier prompt before Ctrl+R (`tools/smoke/src/steps2a.rs:1094`) and passes only when the count grows once the search box is open. At 44 rows the step 2 prompt (`> Without using any tools, ... cobalt and 3556`) was still on the top row of the visible screen, so the count before was 1; opening the search box at the bottom scrolled the transcript up, and that prompt left the top of the screen just as the box's match row (`... cobalt and 3556  now`) appeared, so the count stayed at 1. The captured pane text shows it: the visible read after the box opened starts at the answer `cobalt-3556`, and the recent read shows the prompt directly above it. The screen before Ctrl+R was not captured; the count before is inferred from that layout. The operator set the Herdr window to 170 by 29 (section 3 asks for at least 100 by 24), so the step 2 prompt is off screen long before this step.

Reruns approved on the issue before runs 2 to 5: `rerun approved: AUTO-9/AUTO-10: Herdr window 102 columns wraps the matched text`, `rerun approved: AUTO-9: model counted with a code tool instead of streaming`, `rerun approved: AUTO-9: leftover agent-authored talaria-fixture-check skill in fixture profile hijacked paste-editor turn (and Run 1 was a 102-column narrow-pane setup error under 8.1 item 2)`, and the lead's approval of run 5 at 170 by 29. Section 8.1 item 2 allows three runs per row. The lead ruled on the issue that runs 1, 3 and 4 were pane-geometry or leftover-skill setup errors of the tester, which item 2 says to clear and rerun, so only run 2 counts toward the three. The tester notes that the 102-column pane of run 1 met section 3's minimum of 100 columns.

`$T` is `<tmp>/talaria-wt/talaria-launcher-default`, a two-line script that sets `HERMES_HOME` to the test Hermes home only when it is unset and runs the release binary as a child, as the README advises for a gateway outside the default Hermes home (a new Herdr tab does not inherit the caller's `HERMES_HOME`). The tester's quiet-fleet check (`"$T" fleet list --profile fixture`: no NEEDS YOU and no WORKING group) ran before and after the run.

```text
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # before
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
$ TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2a --talaria "$T" > "$X/s2.txt" 2>&1; echo "exit=$?"
talaria: <tmp>/talaria-wt/talaria-launcher-default
herdr: herdr 0.9.4
tmux: tmux 3.7c
PASS startup 4.5 s
PASS plain-answer 7.3 s
PASS tool-shell 9.5 s
PASS tool-edit 8.4 s
PASS palette 2.4 s
PASS mention 5.8 s
PASS shell 0.9 s
PASS btw 2.2 s
PASS pager 1.3 s
PASS history-search 0.4 s
PASS editor 2.7 s
PASS theme 0.1 s
PASS pane-state 9.7 s
PASS checklist 55.2 s
PASS quit 0.3 s
PASS resume 10.9 s
PASS tmux-pane 5.2 s
PASS paste-collapse 6.0 s
PASS file-drop 5.0 s
PASS paste-editor 5.2 s
PASS tmux-quit 0.6 s
tab: wC6:t36
SMOKE_RESULT: PASSED (elapsed 143 s)
exit=0
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # after
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
```

## AUTO-10

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 3 of 3. Date: 2026-10-26.

Run 1 of 3 failed at the step `model-refusal` after 4 steps had passed (exit 1), with the reason "the refusal for an unknown model did not appear within 10 s". Cause, not a Talaria defect: the Herdr window was 102 columns wide (a second terminal client was holding the shared layout at that width), so Talaria wrapped the refusal onto two lines and the step, which matches the whole sentence on one line, did not find it although it was on screen. The operator widened the window to 170 by 44. Run 2 of 3 then failed at the step `headless` after 14 steps had passed (exit 1), with the reason that the headless run without a gateway did not print `"status":"not_started"` with exit code 4. Cause, not a Talaria defect: the step runs the binary with `HERMES_HOME` pointing at an empty folder to simulate a missing gateway, but the tester's first launcher script set `HERMES_HOME` unconditionally, so the run reached the live gateway and exited 0. Both reruns were approved on the issue first: `rerun approved: AUTO-9/AUTO-10: Herdr window 102 columns wraps the matched text` and `rerun approved: AUTO-10: tester launcher overrode the headless step HERMES_HOME`. Run 3 below used a launcher that sets `HERMES_HOME` only when it is unset (a new Herdr tab starts with it unset, checked in a probe tab), in a 170-column window.

`$T` is `<tmp>/talaria-wt/talaria-launcher-default`, a two-line script that sets `HERMES_HOME` to the test Hermes home only when it is unset and runs the release binary as a child, as the README advises for a gateway outside the default Hermes home (a new Herdr tab does not inherit the caller's `HERMES_HOME`). The tester's quiet-fleet check (`"$T" fleet list --profile fixture`: no NEEDS YOU and no WORKING group) ran before and after the run.

```text
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # before
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
$ TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2b --talaria "$T" > "$X/s3.txt" 2>&1; echo "exit=$?"
talaria: <tmp>/talaria-wt/talaria-launcher-default
herdr: herdr 0.9.4
PASS startup 4.1 s
PASS context-empty 0.0 s
PASS permission 0.6 s
PASS model-picker 1.5 s
PASS model-refusal 0.3 s
PASS rewind 15.9 s
PASS plain-answer 7.6 s
PASS steer-idle 4.5 s
PASS context 0.3 s
PASS hermes-commands 0.3 s
PASS rename-export 0.3 s
PASS queue 36.0 s
PASS cd 0.3 s
PASS quit 0.3 s
PASS headless 2.2 s
tab: wC6:t32
SMOKE_RESULT: PASSED (elapsed 74 s)
exit=0
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # after
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
```

## AUTO-11

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Run 1 of 1. The Herdr window measured 102 columns by 41 rows during this run; the phase 3 steps do not depend on a wider tab.

`$T` is the release binary `<worktree>/target/release/talaria` (the phase 3 suite exports `HERMES_HOME` into its tab itself). The tester's quiet-fleet check (`"$T" fleet list --profile fixture`: no NEEDS YOU and no WORKING group) ran before and after the run.

```text
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # before
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
$ TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase3 --talaria "$T" > "$X/s4.txt" 2>&1; echo "exit=$?"
talaria: <worktree>/target/release/talaria
herdr: herdr 0.9.4
PASS fleet-quiet 3.0 s
PASS bg-launch 0.6 s
PASS worktree-launch 0.8 s
PASS fleet-list 6.1 s
PASS attach 2.8 s
PASS startup 2.5 s
PASS plain-answer 4.4 s
PASS local-commands 0.9 s
PASS fleet-view 0.7 s
PASS open-in-pane 1.0 s
PASS quit 0.3 s
PASS needs-you 18.6 s
tab: wC6:t2T
SMOKE_RESULT: PASSED (elapsed 42 s)
exit=0
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # after
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
```

## AUTO-12

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Run 1 of 1, straight after AUTO-11 with only the quiet-fleet check between them. The Herdr window was 102 columns by 41 rows. Both tab lists bracket the whole block of AUTO-8 run 1 to AUTO-12; the tmux count was taken after AUTO-12.

`$T` is the release binary `<worktree>/target/release/talaria` (the phase 3 suite exports `HERMES_HOME` into its tab itself). The tester's quiet-fleet check (`"$T" fleet list --profile fixture`: no NEEDS YOU and no WORKING group) ran before and after the run.

```text
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # before
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
$ TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase3 --talaria "$T" > "$X/s5.txt" 2>&1; echo "exit=$?"
talaria: <worktree>/target/release/talaria
herdr: herdr 0.9.4
PASS fleet-quiet 3.0 s
PASS bg-launch 0.6 s
PASS worktree-launch 1.1 s
PASS fleet-list 6.1 s
PASS attach 2.3 s
PASS startup 2.5 s
PASS plain-answer 4.6 s
PASS local-commands 0.9 s
PASS fleet-view 0.7 s
PASS open-in-pane 1.0 s
PASS quit 0.3 s
PASS needs-you 18.9 s
tab: wC6:t2V
SMOKE_RESULT: PASSED (elapsed 42 s)
exit=0
$ herdr tab list --workspace "$HERDR_WORKSPACE_ID"   # after
wC6:t3 t70-c1-dev-1
wC6:t4 t70-c1-dev-2
wC6:t5 t70-c1-dev-3
wC6:t6 t70-c1-dev-4
wC6:t1 t70-c1-test-1
wC6:t2 t70-c1-test-2
wC6:tV t70-c1-test-3
$ tmux ls 2>&1 | grep -c talaria-smoke
0
```

## P3-01 UAT-1

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Pane A is a shell; pane B runs `"$T" --profile fixture --no-ensure` (change 2) in the scratch folder, each in a Herdr tab of its own (170 by 29: a split would make either pane narrower than 100 columns). Pane B sent `Reply with the single word: ready` and replied `ready`; its session is the row `Respond with ready #5`. The other 199 rows are idle sessions of earlier runs, cut by section 5.3. Every row is on `fixture`; no other profile name appears.

```text
$ "$T" fleet list --profile fixture; echo "exit=$?"
IDLE (200)
  fixture  Respond with ready #5                     minimax-m3.1-flash  idle  2m
  ... (199 more rows, all '  fixture ... minimax-m3.1-flash  idle', cut: earlier runs' sessions)
exit=0
$ # rows by profile: 200 fixture
```

## P3-01 UAT-2

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The tester's card time is when its 0.5 s poll of pane B first saw `Approval needed`; Esc was sent 2 s later. Both latencies are well inside the 6 s the scenario allows. The model's denied-command lines in pane B are its own reply text.

```text
# pane A
$ "$T" fleet list --watch --profile fixture --for 120
18:56:11.782 fixture Respond with ready #5 idle -> working
18:56:16.741 fixture Respond with ready #5 working -> needs you
18:56:16.741 needs you: fixture Respond with ready #5 approval: rm -rf ./talaria-fleet-probe
18:56:18.445 fixture Respond with ready #5 needs you -> working
18:56:18.445 cleared: fixture Respond with ready #5
18:56:20.480 fixture Respond with ready #5 working -> done

# pane B (visible)
ready
> Run this exact shell command: rm -rf ./talaria-fleet-probe
⏺ Shell(rm -rf ./talaria-fleet-probe)  ✗ exit -1: You denied this command — it did not run. · 5.7s
  ⎿  You denied this command — it did not run.
? rm -rf ./talaria-fleet-probe → deny
The command was blocked — you denied it, so rm -rf ./talaria-fleet-probe did not run. Nothing was deleted. Let me know if you'd like to proceed differently.
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                                                               │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ …/<tmp>/tmp.ibwvaJZmls │ ctx 1%

# timings (tester clock)
card seen 18:56:15 -> needs you printed 18:56:16.741 (about 1.7 s); Esc sent 18:56:17 -> cleared printed 18:56:18.445 (about 1.4 s)
$ ls -d talaria-fleet-probe   # in pane A, after the watch ended
talaria-fleet-probe
```

## P3-01 UAT-3

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Pane B was started fresh for this run (a new fixture session), sent the prompt and showed its card. Pane A's kill (`pkill -TERM -f -x "$T --profile fixture --no-ensure"`, the scenario's pattern with changes 1 and 2) and the first listing came 3 s after the card; pane B was then cleared, the session resumed, and Esc was pressed only once both `Resumed` and the redrawn card were on screen. The second listing shows the session `WORKING` on its reply to the denial and no `NEEDS YOU` group; the quiet-fleet check 19 s later read `IDLE (200)`.

Earlier runs, none counted toward the three-run limit by the lead's ruling on the issue (`rerun approved: P3-01 UAT-3: ...`, issue comment 6429862467). Run 1: the tester's wait matched the killed Talaria's card still drawn in pane B, so Esc reached the shell before the resumed Talaria drew its card; while that card was up, an attached Herdr client focused pane B's tab (Herdr server log, 18:59:59) and, by the lead's reading of that log, the key `3` (always) was pressed there; the card recorded `→ always`, which approved the command and saved `recursive delete` to the fixture profile's command allowlist. Runs 2 and 3: with that allowlist entry Hermes ran the command without asking, so no card appeared and the probe folder was deleted. The operator removed the allowlist entry before this run, and the tester recreated `talaria-fleet-probe`.

```text
# pane A
$ pkill -TERM -f -x "$T --profile fixture --no-ensure"; echo "pkill=$?"; "$T" fleet list --profile fixture | head -4
pkill=0
NEEDS YOU (1)
  fixture  Remove talaria-fleet-probe directory #11  minimax-m3.1-flash  approval: rm -rf ./talaria-fleet-probe  now
IDLE (199)
  fixture  Remove talaria-fleet-probe directory #10  minimax-m3.1-flash  idle                                    29m
$ "$T" fleet list --profile fixture | head -3; ls -d talaria-fleet-probe
WORKING (1)
  fixture  Remove talaria-fleet-probe directory #11  minimax-m3.1-flash  working  now
IDLE (199)
talaria-fleet-probe

# pane B: the restored card after the resume (visible)
| jq -r '.needs_you[0].session_id')" --profile fixture --no-ensure
── Resumed: Remove talaria-fleet-probe directory #11 ──
> Run this exact shell command: rm -rf ./talaria-fleet-probe
? rm -rf ./talaria-fleet-probe
Approval needed
$ rm -rf ./talaria-fleet-probe
recursive delete
Hermes denies this after 60 s without an answer.
› 1. once
  2. session
  3. always
  4. deny

# pane B after Esc
? rm -rf ./talaria-fleet-probe → deny
⏺ Shell(rm -rf ./talaria-fleet-probe)  ✗ exit -1: You denied this command — it did not run. · 9.7s
  ⎿  You denied this command — it did not run.
```

## P3-01 UAT-4

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Pane B (the session `Remove talaria-fleet-probe directory #11`, idle after UAT-3) was sent the prompt; `/quit` went in 8 s later while the count was running, and Talaria printed its resume hint. Every row event in the file belongs to that one session; the first `["idle",null]` is the watch's opening snapshot of it.

```text
$ "$T" fleet list --watch --json --profile fixture --for 120 > watch.jsonl
watch-exit=0
$ jq -c 'select(.kind=="row") | [.row.state, .row.outcome]' watch.jsonl
["idle",null]
["working",null]
["done","completed"]
["done","completed"]
$ jq -r 'select(.kind=="row") | .row.title' watch.jsonl | sort | uniq -c
      4 Remove talaria-fleet-probe directory #11
```

## P3-01 UAT-5

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The tester added `time` (to show the 2 s limit) and `echo "ls-exit=$?"`; the command is otherwise as written. `ls -A` printed nothing, so no gateway was started in the empty Hermes home.

```text
$ H="$(mktemp -d)"; time HERMES_HOME="$H" "$T" fleet list; echo "exit=$?"; ls -A "$H"; echo "ls-exit=$?"
talaria fleet: no Hermes gateway is running for this Hermes home. Start one with "hermes gateway ensure", or start talaria.
real    0m0.006s
user    0m0.002s
sys     0m0.003s
exit=1
ls-exit=0
```

## P3-01 UAT-6

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The message names a count (2) and no profile.

```text
$ "$T" fleet list --profile nosuch; echo "exit=$?"
talaria fleet: profile "nosuch" is not served by this gateway (2 profiles are served; run without --profile to see them).
exit=2
```

## P3-02 UAT-1

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Pane B sent the prompt, showed the turn working 3 s later and quit at 12 s; its id prefix `local-029e8a56` was the filter. Pane A's tester-timed latency from space to the first panel line containing `sleep 30` was 0.57 s (that line is the session's prompt). The 201-session count includes pane A's own session.

```text
# pane A after /fleet, / and B id prefix, Enter
Fleet · 201 sessions · filter "local-029e8a56"
WORKING (1)
›  ◐  Run sleep and echo command …  minimax-m3.1-flash  working                              now
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane

# pane A 3 s after space (the "sleep 30" line appeared 0.57 s after space)
Fleet · 201 sessions · filter "local-029e8a56"
WORKING (1)
›  ◐  Run sleep and echo command …  minimax-m3.1-flash  working                              now
── peek ──
◐ Run sleep and echo command #10 · fixture · minimax-m3.1-flash · working now
> Run this exact shell command: sleep 30 && echo peek-ok
The user wants me to run an exact shell command. Let me do that.
✻ working
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane

# pane A 40 s after B prompt
Fleet · 201 sessions · filter "local-029e8a56"
DONE (1)
›  ✓  Run sleep and echo command …  minimax-m3.1-flash  completed                            now
── peek ──
✓ Run sleep and echo command #10 · fixture · minimax-m3.1-flash · completed now
> Run this exact shell command: sleep 30 && echo peek-ok
The user wants me to run an exact shell command. Let me do that.
⏺ Shell(sleep 30 && echo peek-ok)  ✓ · 33s
  ⎿  peek-ok
Exit code 0, output: peek-ok
The user wants me to run an exact shell command. Let me do that.
✓ completed
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane
```

## P3-02 UAT-2

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Pane A's fleet view was opened before pane B quit, so the reply started 3 s after the quit (inside the 10 s the scenario allows). Pane A was read every 0.3 s from 28 s after B's prompt: the first turn's `peek-ok` showed at +37 s, then the panel moved to the queued turn. The peek panel shows the newest turn, so `peek-ok` is on screen for about a second before the queued turn replaces it; that is why a single read at 45 s cannot show it.

Earlier runs, all mistakes of the tester (section 8.1 item 2: clear, rerun, say so), none touching Talaria's result: run 1, an Esc closed only the peek panel after UAT-1, so the typed `/fleet` went into the still-open overlay's filter and the reply text opened the worktree dialog (it refused to launch outside a repository; nothing started); run 2, the Enter after the pasted `/fleet` became a new line in the composer and the 10 s start window passed; run 3, the reply was queued and the queued turn ran, and the log shows the first turn's `sleep 30` completing in 33.7 s before it, but the single read at 45 s showed only the queued turn; run 4, the tester's wait matched `peek-ok` in the session's title and stopped sampling early.

```text
# pane A just after enter (reply sent 3 s after B quit)
Fleet · 201 sessions · filter "local-c7374f75"
WORKING (1)
›  ◐  Run sleep and echo command …  minimax-m3.1-flash  working                              now
── peek ──
◐ Run sleep and echo command #12 · fixture · minimax-m3.1-flash · working now
> Run this exact shell command: sleep 30 && echo peek-ok
✻ working
Queued for "Run sleep and echo command #12": it runs after the current turn.
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane

# pane A at +37 s: the first turn finished, not interrupted
Fleet · 201 sessions · filter "local-c7374f75"
WORKING (1)
›  ◐  Run sleep and echo command …  minimax-m3.1-flash  ヽ(>∀<☆)☆ synthesizing...            now
── peek ──
◐ Run sleep and echo command #12 · fixture · minimax-m3.1-flash · working now
> Run this exact shell command: sleep 30 && echo peek-ok
⏺ Shell(sleep 30 && echo peek-ok)  ✓ · 34s
  ⎿  peek-ok
✻ ヽ(>∀<☆)☆ synthesizing...
Queued for "Run sleep and echo command #12": it runs after the current turn.
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane

# pane A at +50 s: the queued turn
Fleet · 201 sessions · filter "local-c7374f75"
DONE (1)
›  ✓  Run sleep and echo command …  minimax-m3.1-flash  completed                            now
── peek ──
✓ Run sleep and echo command #12 · fixture · minimax-m3.1-flash · completed now
> Now reply with the single word: queued-ok
The user asked to reply with the single word "queued-ok". This is analogous to the code word / token case.
Just reply with the single word.
queued-ok
✓ completed
Queued for "Run sleep and echo command #12": it runs after the current turn.
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane
```

## P3-02 UAT-3

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Per the lead's ruling recorded on the issue (comment 6429947908, a specification correction under section 8), pane B's Talaria was ended with `pkill -TERM` once its card showed, instead of typing `/quit` into the focused card (Enter there confirms `1. once`). The pattern was the scenario's, limited to pane B's shell: `pkill -TERM -P <pane B shell pid> -f -x "$T --profile fixture --no-ensure"`, because pane A runs the same command line. The deny digit was pressed 47 s after B's card, inside the fixture profile's 60 s approval timeout. The digit took the row out of `NEEDS YOU` in 0.03 s; the session then went on working on its reply to the denial. Pane A's status line shows its own folder and model and no other session's title or switch notice; pane A's own session has no prompt yet, so it has no title to show.

Earlier attempts, mistakes of the tester (section 8.1 item 2): the first `pkill` was not limited to pane B and also ended pane A's Talaria (that request was denied by Hermes's 60 s timeout, nothing deleted); in this run `a` and the first `4` were sent as pasted text, which the card ignores, before `a` and `4` were sent as key presses.

```text
# pane A, fleet view filtered to B (detail before the digit)
Fleet · 201 sessions · filter "local-94ad4589"
NEEDS YOU (1)
›  ●  Remove talaria-peek-probe d…  minimax-m3.1-flash  approval: rm -rf ./talaria-peek-pr…  now
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane

# pane A after space and a (answer mode), before the digit
Fleet · 201 sessions · filter "local-94ad4589"
NEEDS YOU (1)
›  ●  Remove talaria-peek-probe d…  minimax-m3.1-flash  approval: rm -rf ./talaria-peek-pr…  now
── peek ──
● Remove talaria-peek-probe directory #6 · fixture · minimax-m3.1-flash · needs you now
Approval needed
$ rm -rf ./talaria-peek-probe
recursive delete
Hermes denies this after 60 s without an answer.
› 1. once
  2. session
  3. always
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop
✻ working
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list

# pane A 0.03 s after the digit 4
Fleet · 201 sessions · filter "local-94ad4589"
WORKING (1)
›  ◐  Remove talaria-peek-probe d…  minimax-m3.1-flash  ヽ(>∀<☆)☆ processing...              now
── peek ──

# pane A 4 s later
? rm -rf ./talaria-peek-probe → deny
⏺ Shell(rm -rf ./talaria-peek-probe)  ✗ exit -1: You denied this command — it did not run. · 51s
  ⎿  You denied this command — it did not run.
$ ls -d talaria-peek-probe   # in the scratch folder
talaria-peek-probe

# pane A status line with the overlay closed
 minimax-m3.1-flash │ approvals: manual │ …/<tmp>/tmp.pTRgPfKej7 │ ✓ 1 done
```

## P3-02 UAT-4

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

At 70 s the only lines holding `too-late` are the session's prompt and the model's reasoning quoting it; the panel holds no command cell and no output line, and the Hermes log shows the terminal tool ended with an error after 9.9 s and the turn ended `interrupted_by_user`. The tester reads the scenario's "no line contains too-late" as no output of the command.

```text
# pane A after the first x
Fleet · 201 sessions · filter "local-1350093b"
WORKING (1)
›  ◐  Run delayed shell command #2  minimax-m3.1-flash  working                              now
── peek ──
◐ Run delayed shell command #2 · fixture · minimax-m3.1-flash · working now
> Run this exact shell command: sleep 60 && echo too-late
The user asks to run an exact shell command: sleep 60 && echo too-late. This seems like a test of whether
the agent waits. Let me just run it. Foreground max timeout is 600s, so 60s sleep is fine. But should I
run it in background? The command takes 60 seconds. I can run it in the foreground with a timeout of ~90s.
Let me run it.
Wait — is there a concern about blocking? The instruction says "Run this exact shell command and tell me
its exit code, call the terminal tool once with that exact command only". The user here just says "Run
this exact shell command: sleep 60 && echo too-late". So I should call terminal once with that exact
command. Foreground with timeout set generously (e.g., 120s).
Note the "too-late" name hints at something about the timeout — maybe the intent is that a 60s sleep
exceeds some limit. Let's just run it.
✻ working
Stop "Run delayed shell command #2"? Its running turn and background subagents stop and its open approval…
enter open · space peek · r reply · x stop · v review · / filter · esc close · o new pane

# pane A 0.48 s after the second x
Fleet · 201 sessions · filter "local-1350093b"
›  ✓  Run delayed shell command #2  minimax-m3.1-flash  interrupted                          now
── peek ──
✓ Run delayed shell command #2 · fixture · minimax-m3.1-flash · interrupted now
■ interrupted

# pane A 70 s later: every line holding too-late
> Run this exact shell command: sleep 60 && echo too-late
The user asks to run an exact shell command: sleep 60 && echo too-late. This seems like a test of whether
this exact shell command: sleep 60 && echo too-late". So I should call terminal once with that exact
Note the "too-late" name hints at something about the timeout — maybe the intent is that a 60s sleep
# count of command cells and result lines in the panel:
0
```

## P3-02 UAT-5

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 3 of 3. Date: 2026-10-26.

Result: PASS

Pane A was a new Herdr tab of 169 by 29 running a fresh `"$T" --profile fixture --no-ensure`; the fleet view was filtered to UAT-1's finished session (`local-029e8a56`) and Enter pressed on it. The status line ends with the session's folder and title (`… · Run sleep and echo command #10`). In the reopened view B's row carries ` (here)` and pane A's earlier session is the newest `(untitled)` row, marked `·`.

Earlier runs, approved for rerun on the issue (comment 6430117033): runs 1 and 2 ran in a pane 106 columns wide, because other attached terminal clients held the shared Herdr layout at that width; at that width Talaria leaves the folder and title out of its status line, so the status-line check could not be seen. Everything else held in both. A tab made before the operator's resize kept its 106-column terminal, so this run used a new tab.

```text
# pane A after enter on B row (overlay closed)
Now showing "Run sleep and echo command #10". Left "untitled session"; it keeps running in Hermes. /resume returns to it.
── Resumed: Run sleep and echo command #10 ──
> Run this exact shell command: sleep 30 && echo peek-ok
The user wants me to run an exact shell command. Let me do that.
⏺ Shell(sleep 30 && echo peek-ok)
Exit code 0, output: peek-ok
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                                                                               │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ …/<tmp>/tmp.pTRgPfKej7 │ <tmp>/tmp.pTRgPfKej7 · Run sleep and echo command #10

# pane A after /fleet again
Fleet · 201 sessions
IDLE (201)
›  ·  (untitled)                    fixture       minimax-m3.1-flash  idle                                                                                  now
   ·  (untitled)                    fixture       minimax-m3.1-flash  idle                                                                                  2m
   ·  Run delayed shell command #4  fixture       minimax-m3.1-flash  idle                                                                                  3m
   ·  Run delayed shell command #3  fixture       minimax-m3.1-flash  idle                                                                                  5m
   ·  Run delayed shell command #2  fixture       minimax-m3.1-flash  idle                                                                                  8m
   ·  Remove talaria-peek-probe d…  fixture       minimax-m3.1-flash  idle                                                                                  10m
   ·  Run sleep and echo c… (here)  fixture       minimax-m3.1-flash  idle                                                                                  21m
```

## P3-02 UAT-6

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Pane B started the `sleep 60` prompt and quit; pane A's own session is the idle `(here)` row (pane A has shown UAT-1's finished session since UAT-5's first run). The tester reached each row with the fleet view's filter (`/`, the session id's start, enter), clearing the previous filter with Esc first, before the scenario's own `/ zzz-no-match`.

An earlier attempt was spoiled by the tester: `/` reopened the filter with its old text and appended to it, so the filter read `local-...local-...zzz-no-match` and the `x` on pane A's own row never happened. It is not counted (section 8.1 item 2).

```text
# pane A, x on B row
Fleet · 201 sessions · filter "local-859448ce"
WORKING (1)
›  ◐  Run delayed shell command #4  minimax-m3.1-flash  working                              now
Stop "Run delayed shell command #4"? Its running turn and background subagents stop and its open approval…
# then esc
Stop cancelled.

# x on pane A's own idle row
›  ·  Run sleep and echo c… (here)  minimax-m3.1-flash  idle                                 17m
Nothing is running in "Run sleep and echo command #10".

# / zzz-no-match enter
Fleet · 201 sessions · filter "zzz-no-match"
No sessions match "zzz-no-match".
No sessions match "zzz-no-match".

# after ctrl+c (overlay closed, prompt intact)
╭────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ …/<tmp>/tmp.pTRgPfKej7 │ 1 working │ ✓ 3 done

# /fleet reopened
Fleet · 201 sessions
WORKING (1)
›  ◐  Run delayed shell command #4  fixture       minimax-m3.1-flash  working                now
```

## P3-03 UAT-1

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Pane A is a shell in `$S/repo` (a Herdr tab of 169 by 29). The launch carries `--no-ensure` (change 2) and `$T` (change 1). The tester appended `echo snapshot-ready` to the snapshot wait and `echo wait-done` to the last line, to read the pane. The launch returned in 0.088 s (`real`).

```text
$ "$T" fleet list --watch --json --profile fixture --for 120 > "$S/w.jsonl" & until grep -q '"kind":"snapshot"' "$S/w.jsonl"; do sleep 1; done; echo snapshot-ready
[1] 60435
snapshot-ready
                                                                                                                                                               main≡ 
$ time "$T" --bg --no-ensure --output-format json --profile fixture "Count from one to five." | tee "$S/l.json"; echo "exit=${PIPESTATUS[0]}"
{"format":"talaria.bg.v1","status":"launched","session_id":"local-3035123cf34d28e23563f91cf907673dd370cc73bdfb05ce2a82da77f959408a","stored_session_id":"local-3035123cf34d28e23563f91cf907673dd370cc73bdfb05ce2a82da77f959408a","profile":"fixture","admission_id":"8003344f99b445fe808ab84b16fd6f3f","admission_status":"queued","model":"minimax-m3.1-flash","cwd":"<tmp>/tmp.PXxaUfBVo9/repo","worktree":null,"warnings":[],"error":null}
real    0m0.088s
user    0m0.004s
sys     0m0.008s
exit=0
                                                                                                                                                               main≡ 
$ wait; jq -r .session_id "$S/l.json"; grep '"kind":"row"' "$S/w.jsonl" | jq -r 'select(.row.state=="done") | .row.session_id'; echo wait-done
[1]+  Done                       "$T" fleet list --watch --json --profile fixture --for 120 > "$S/w.jsonl"
local-3035123cf34d28e23563f91cf907673dd370cc73bdfb05ce2a82da77f959408a
local-3035123cf34d28e23563f91cf907673dd370cc73bdfb05ce2a82da77f959408a
wait-done
                                                                                                                                                               main≡ 
```

## P3-03 UAT-2

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The scenario's worktree variable `T` is renamed `TW` because `$T` is the binary (a rename under change (1), as section 7.2 does for P3-06). `--no-ensure` is added to the launch (change 2). The polling loop is the tester's way of polling every 5 s (180 s limit); it printed every state it read, and none was `needs_you` or `failed`. `git status --porcelain` and `git diff --stat -- .gitignore` printed nothing (the line before `final-done` is `ls`'s output). The prompt's `main↓1` marker shows `main` still one commit behind `origin`, as set up.

```text
$ git log -1 --format=%s main; "$T" --bg --no-ensure --worktree --name probe-one --profile fixture "Create a file named hello.txt containing the word hi in the current folder. Do nothing else."; echo "exit=$?"; ID=$(git config --get branch.talaria/probe-one.talaria-session); echo "ID=${ID:0:14}"
one
Launched session local-f40dfaad6b8fc0a6051f46f5c0b213915afea9b2c30b644fea0d21d63fe6ad54 in profile fixture.
Folder: <tmp>/tmp.PXxaUfBVo9/repo/.worktrees/talaria-probe-one
Worktree: branch talaria/probe-one from origin/main at a1a92b9eb7d4
Follow it: talaria fleet list --profile fixture
Open it: talaria --resume local-f40dfaad6b8fc0a6051f46f5c0b213915afea9b2c30b644fea0d21d63fe6ad54 --profile fixture
exit=0
ID=local-f40dfaad
                                                                                                                                                              main↓1 
$ TW=.worktrees/talaria-probe-one; i=0; while [ $i -lt 36 ]; do st=$("$T" fleet list --json --profile fixture | jq -r --arg id "$ID" '.rows[] | select(.session_id==$id) | .state'); echo "poll $((i*5))s state=$st hello=$(ls $TW/hello.txt 2>/dev/null)"; if ls $TW/hello.txt >/dev/null 2>&1 && { [ "$st" = done ] || [ "$st" = idle ]; }; then break; fi; sleep 5; i=$((i+1)); done; echo poll-end
poll 0s state=working hello=
poll 5s state=working hello=.worktrees/talaria-probe-one/hello.txt
poll 10s state=idle hello=.worktrees/talaria-probe-one/hello.txt
poll-end
                                                                                                                                                              main↓1 
$ git -C $TW log -1 --format=%s; ls $TW/hello.txt hello.txt; git status --porcelain; git diff --stat -- .gitignore; echo final-done
two
ls: cannot access 'hello.txt': No such file or directory
.worktrees/talaria-probe-one/hello.txt
final-done
                                                                                                                                                              main↓1 
```

## P3-03 UAT-3

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
# pane B: w pressed in the fleet view (the dialog)
›  ✓  Create hello.txt w╭─ New background session in a worktree ───────────────────────────────────────────────────────────────────────────────╮            now
   ✓  Count from one to │ Prompt   ›                                                                                                           │            2m
IDLE (199)              │ Profile    fixture                                                                                     ← → to change │
   ·  (untitled) (here) │ Folder     <tmp>/tmp.PXxaUfBVo9/repo                              │            2m
   ·  (untitled)        │ Model      profile default                                                                                           │            3m
   ·  (untitled)        │ Worktree   work                                                                                                      │            5m
   ·  Run delayed shell │                                                                                                                      │            7m
   ·  Run delayed shell │ enter launch · tab next · shift+tab back · esc close                                                                 │            8m
   ·  Run delayed shell ╰──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯            12m
   ·  Remove talaria-peek-probe d…  fixture       minimax-m3.1-flash  idle                                                                                  14m

# pane B 1 s after Enter (dialog closed 0.48 s, row listed 0.73 s after Enter)
Fleet · 202 sessions
WORKING (1)
   ◐  Write the word ready into n…  fixture       minimax-m3.1-flash  working                                                                               now

# pane B after Esc, then x typed
· theme.name is no longer used: Talaria follows the Hermes skin (see /skin and /theme).
· Launched in fixture: "Write the word ready into notes.txt" (worktree talaria/write-the-word-ready-into-notes-txt)
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > x                                                                                                                                                                   │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ …/<tmp>/tmp.PXxaUfBVo9/repo (main) │ 1 working │ ✓ 2 done

# pane A
$ ls -d .worktrees/talaria-write-the-word-ready-into-notes-txt
.worktrees/talaria-write-the-word-ready-into-notes-txt
```

## P3-03 UAT-4

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The launch carries `--no-ensure` (change 2). Pane B was a Talaria at its composer; it quit, its screen was cleared, and `"$T" --resume <id> --profile fixture --no-ensure` (changes 1 and 2) restored the card; Esc was pressed only once `Resumed` and the card were on screen. The listing loop is the tester's way of listing every 5 s for up to 60 s, stopping at the first `NEEDS YOU (1)`; it prints only the group lines and this scenario's rows.

```text
# pane A
$ mkdir talaria-bg-probe; "$T" --bg --no-ensure --profile fixture "Run this exact shell command: rm -rf ./talaria-bg-probe"; for i in 1 2 ... 12; do echo "--- listing $i $(date +%T)"; "$T" fleet list --profile fixture | grep -E "^[A-Z][A-Z ]+ \(|talaria-bg-probe" | head -4; ...NEEDS YOU (1) && break; sleep 5; done
Launched session local-7176268d1c9320875d3d4e04710241194d788a68f0c16f40f6ee865fabea96df in profile fixture.
Folder: <tmp>/tmp.PXxaUfBVo9/repo
Follow it: talaria fleet list --profile fixture
Open it: talaria --resume local-7176268d1c9320875d3d4e04710241194d788a68f0c16f40f6ee865fabea96df --profile fixture
--- listing 1 20:07:13
WORKING (1)
IDLE (199)
  fixture  Remove talaria-bg-probe directory #11     minimax-m3.1-flash  idle     1h
  fixture  Remove talaria-bg-probe directory #10     minimax-m3.1-flash  idle     8h
--- listing 2 20:07:19
WORKING (1)
  fixture  Remove talaria-bg-probe directory #14     minimax-m3.1-flash  working  now
IDLE (199)
  fixture  Remove talaria-bg-probe directory #11     minimax-m3.1-flash  idle     1h
--- listing 3 20:07:24
NEEDS YOU (1)
  fixture  Remove talaria-bg-probe directory #14     minimax-m3.1-flash  approval: rm -rf ./talaria-bg-probe  now
IDLE (199)
  fixture  Remove talaria-bg-probe directory #11     minimax-m3.1-flash  idle                                 1h
                                                                                                                                                              main↓1 
$ ls -d talaria-bg-probe
talaria-bg-probe
                                                                                                                                                              main↓1 

# pane B after the resume: the restored card
── Resumed: Remove talaria-bg-probe directory #14 ──
> Run this exact shell command: rm -rf ./talaria-bg-probe
The user asked to run an exact shell command: rm -rf ./talaria-bg-probe. This is a destructive command but explicitly requested. Let me just run it and report the exit
? rm -rf ./talaria-bg-probe
Approval needed
$ rm -rf ./talaria-bg-probe
recursive delete
Hermes denies this after 60 s without an answer.
› 1. once
  2. session
  3. always
  4. deny

# pane B after Esc
? rm -rf ./talaria-bg-probe → deny
⏺ Shell(rm -rf ./talaria-bg-probe)  ✗ exit -1: You denied this command — it did not run. · 22s
  ⎿  You denied this command — it did not run.
```

## P3-03 UAT-5

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Both launches carry `$T` and `--no-ensure` (changes 1 and 2); the tester ran each `cd` in a subshell so pane A stays in `$S/repo`. The row count was 200 before and after, as the scenario asks, but 200 is the list's cap (the diagnostics say 1435 more live sessions are not listed), so the count alone cannot show that nothing started. The tester therefore also read the fixture gateway log: no prompt `hi` arrived after 20:08:00, so neither launch started a session.

```text
$ N=$("$T" fleet list --json --profile fixture | jq ".rows | length"); echo "N=$N"; ( cd "$(mktemp -d)" && "$T" --bg --worktree --no-ensure --profile fixture "hi"; echo "exit=$?" ); R="$(mktemp -d)"; git -C "$R" init -q; $G -C "$R" commit -q --allow-empty -m x
N=200
talaria: <tmp>/tmp.MHWqC66Jx1 is not inside a git repository.
Next step: Run talaria --bg without --worktree, or start it from inside a repository.
exit=4
talaria: this repository has no remote named origin.
Next step: Add one with git remote add origin <url>, or run without --worktree.
exit=4
.git
N-after=200
[
  "1435 live sessions in fixture are not in its list"
]
                                                                                                                                                              main↓1 
$ # gateway log (fixture): inbound prompts "hi" since 20:08:00
0
```

## P3-03 UAT-6

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Run in a temporary folder (any folder will do); the tester removed `out.txt` afterwards.

```text
$ "$T" --bg -p "x" > out.txt; echo "exit=$?"; wc -c < out.txt; "$T" --worktree "x"; echo "exit=$?"; "$T" --bg --worktree --name '!!' "x"; echo "exit=$?"
Next step: Run talaria --help.
exit=2
0
talaria: talaria --worktree needs --bg.
Next step: Run talaria --help.
exit=2
talaria: talaria --name needs at least one ASCII letter or digit.
Next step: Run talaria --help.
exit=2
                                                                                                                                                              main↓1 
```

## P3-04 UAT-1

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Pane B is a Herdr tab of 106 by 44 (`tput cols` 106, at least 100 as the common setup asks). No approval appeared. The live region was read every second; the read at 7 s caught the dock with `2 running`. The model's own reply text is left out of the scrollback read after the block.

```text
# pane B live region, read 7 s after prompt B (reads every 1 s)
I'll spawn both subagents in parallel now.
⏺ Delegate(2 tasks: List the files in the current working... | Read the file notes.txt i...)  ✓ · 0.1s
⎇ Subagents · 2 running · Alt+S to open
  ◐ List the files in the current working directory and report the names. · ヽ(>∀<☆)☆ contemplating...
  ◐ Read the file notes.txt in the current working directory and report its full contents. · ( ͡° ͜ʖ ͡°) pon…
╭────────────────────────────────────────────────────────────────────────────────────────────────────────╮

# pane B scrollback after the reply ended
> Use the delegate_task tool to run two subagents in parallel: one lists the files in the current director
y, the other reads notes.txt. Then summarise both results in one sentence.
I'll spawn both subagents in parallel now.
⏺ Delegate(2 tasks: List the files in the current working... | Read the file notes.txt i...)  ✓ · 0.1s
Both subagents are running in the background; results will arrive as a new message when they finish.
⎇ Subagents · 2 finished · 9s
  ✓ Read the file notes.txt in the current working directory and report its full contents. · 9s · 3 tools…
  ✓ List the files in the current working directory and report the names. · 7s · 1 tool · Listed `<tmp>…
· Subagent Tasks Completed: List the files in the current working directory and report the names.; Read th

# pane B live region at the end (no ⎇ Subagents)
The working directory contains a single file, notes.txt, whose contents are the line "alpha beta".
╭────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ …/<tmp>/tmp.ff1I1w439V │ ctx 1%
```

## P3-04 UAT-2

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Before prompt L, a tester mistake sent the word `clear` to this session as a prompt (a shell command typed into the running Talaria); the model answered it and nothing else ran. Prompt L followed, and the dock showed `1 running` before `alt+s`.

```text
# read 1: after alt+s
Subagents · List files and read notes.txt #17                         1 running · 0 finished · spawning on
▸ ◐ Run `sleep 3` ten times, o…  minimax-m3.1-flash             1s  0 tools   (⊙_⊙) reasoning...

# read 2: Enter, 5 s later
Live transcript · "Run `sleep 3` ten times, one terminal command at a time, then reply with the single wo…
=== Hermes subagent live transcript ===
delegation: deleg_291f79d0   task: 0
goal: Run `sleep 3` ten times, one terminal command at a time, then reply with the single word finished.
started: 2026-10-26 20:10:22
(append-only; streams while the subagent runs — tail -f me)
========================================
20:10:22 user     | kickoff: Run `sleep 3` ten times, one terminal command at a time, then reply with the
single word finished. | context: You are a leaf subagent on macOS. You have a terminal tool. Execute the s
hell command `sleep 3` TEN separate times, as TEN distinct terminal tool invocations (one command per call

# read 3: after Esc twice
⎇ Subagents · 1 running · Alt+S to open
  ◐ Run `sleep 3` ten times, one terminal command at a time, then reply with the single word finished. · …
╭────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > Type a message. Enter sends, Shift+Enter adds a line.                                                │
╰────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ …/<tmp>/tmp.ff1I1w439V │ ctx 2%
```

## P3-04 UAT-3

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Try 1 of 3 (the scenario's retry rule was not needed).

```text
# after e (the steer prompt)
Subagents · List files and read notes.txt #17                         1 running · 0 finished · spawning on
▸ ◐ Run `sleep 3` ten times, o…  minimax-m3.1-flash             1s  0 tools   (◔_◔) ruminating...
Steer "Run `sleep 3` ten times, one terminal command at a time, then reply with the single word finished…

# read 0.03 s after Enter
Subagents · List files and read notes.txt #17                         1 running · 0 finished · spawning on
▸ ◐ Run `sleep 3` ten times, o…  minimax-m3.1-flash             8s  2 tools   terminal("sleep 3")
Steer queued for "Run `sleep 3` ten times, one terminal co". It reaches the subagent at its next step.
```

## P3-04 UAT-4

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 2 follows the corrected wording of the scenario (issue #242 and Appendix A, recorded on the issue under section 8): the first read came 0.05 s after `x`, then reads every 0.15 to 0.2 s, then every 5 s to 60 s. The row showed `stopping` at 0.05 s and `■` at 0.24 s (seconds from `x` to `■`: 0.24); the flash read `Stop requested for "`. At 1.21 s the parent's follow-up turn started and the panel pruned the finished batch (`prune_finished` on `message.start`, P3-04's specified behaviour); scrollback holds `⎇ Subagents · 1 finished · 1 stopped · 0s` with the `■` row and Hermes's `status=interrupted`. The parent turn's final status: the model was told its child was interrupted, started a new child that ran to `✓` (49 s), and ended with `done`.

Run 1 (no count against the limit by the lead's ruling, `rerun approved: P3-04 UAT-4`): the first read came 1 s after `x`, after the prune, so the panel showed no row; the stop itself worked (interrupted after 1.49 s, the same scrollback block).

```text
# panel open on the running row, before x
Subagents · Use the delegate_task tool to start exactly one…          1 running · 0 finished · spawning on
▸ ◐ Run the shell command `sle…  minimax-m3.1-flash             0s  0 tools   (⌐■_■) analyzing...

# reads after x (every 0.15-0.2 s; seconds since x)
+0.05s  1 running · 0 finished  ▸ ◐ Run the shell command `sle…  minimax-m3.1-flash             0s  0 tools   stopping
+0.24s  0 running · 1 finished  ▸ ■ Run the shell command `sle…  minimax-m3.1-flash             0s  0 tools   Your request was not proces…
+0.44s  0 running · 1 finished  ▸ ■ Run the shell command `sle…  minimax-m3.1-flash             0s  0 tools   Your request was not proces…
+0.64s  0 running · 1 finished  ▸ ■ Run the shell command `sle…  minimax-m3.1-flash             0s  0 tools   Your request was not proces…
+0.83s  0 running · 1 finished  ▸ ■ Run the shell command `sle…  minimax-m3.1-flash             0s  0 tools   Your request was not proces…
+1.02s  0 running · 1 finished  ▸ ■ Run the shell command `sle…  minimax-m3.1-flash             0s  0 tools   Your request was not proces…
+1.21s  0 running · 0 finished  (no row)
+1.39s  0 running · 0 finished  (no row)
+1.21s .. +3.72s  0 running · 0 finished  (the batch was pruned)
Stop requested for "Run the shell command `sleep 3` ten time".

# scrollback
⎇ Subagents · 1 finished · 1 stopped · 0s
  ■ Run the shell command `sleep 3` ten times, one command at a time (ten separate terminal calls, each r…
The subagent runs in the background. The result comes back as a new message after I end my turn. The user
lls, each running `sleep 3` alone, sequentially). Then reply with the single word: finished  (status=inter
⎇ Subagents · 1 finished · 49s
  ✓ Run the shell command `sleep 3` ten times, one command at a time (ten separate terminal calls, each r…
done
```

## P3-04 UAT-5

Candidate: 8b514e6. Runner: t70-c1-dev-1. Run 3 of 3. Date: 2026-10-26.

Result: PASS

Run 3 follows the corrected wording (issue #242 and Appendix A, recorded on the issue under section 8): a new session; after prompt B the panel was opened as soon as the dock appeared, and `x` was pressed on the `✓` row while its sibling was still running (`1 running · 1 finished`), before the follow-up turn could prune the batch. The flash reads `had already finished.` and the row stays `✓`.

Earlier runs: run 1 opened the panel after the reply, by which time the follow-up turn had pruned the finished batch (`No subagents in this session yet.`), so no `✓` row existed; run 2, the tester waited for a dock text (`1 running · 1 finished`) that the dock does not show, and the batch finished first. Neither counts against the limit (the lead's ruling recorded on the issue; run 2 was the tester's mistake).

```text
# /tasks
Subagents · local-bba8017cde3a5fe152c383bff283cf05b852bed98f8a03fe7…  0 running · 0 finished · spawning on
No subagents in this session yet. They appear when the agent uses delegate_task.

# /agents frobnicate
· Unknown /agents option: frobnicate. Use /agents, /agents pause or /agents resume.

# prompt B; panel open while one child has finished and its sibling still runs
Subagents · Summarize directory files and notes.txt                   1 running · 1 finished · spawning on
▸ ✓ Read the contents of notes…  minimax-m3.1-flash             8s  2 tools   `notes.txt` exists and cont…
  ◐ List all files in the curr…  minimax-m3.1-flash             8s  3 tools   ( ͡° ͜ʖ ͡°) mulling...

# x on the ✓ row
Subagents · Summarize directory files and notes.txt                   1 running · 1 finished · spawning on
▸ ✓ Read the contents of notes…  minimax-m3.1-flash             8s  2 tools   `notes.txt` exists and cont…
  ◐ List all files in the curr…  minimax-m3.1-flash             8s  3 tools   ( ͡° ͜ʖ ͡°) mulling...
"Read the contents of notes.txt in the cu" had already finished.

# scrollback after the reply
⎇ Subagents · 2 finished · 10s
```

## P3-04 UAT-6

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431843691

```text
Initial alt+s header: Subagents · spawning on
After /agents pause then n: Subagents · spawning on (panel stays open)
After p then y: Subagents · spawning paused for all sessions
Prompt B sent while paused: 0 "⎇ Subagents" lines in live region or scrollback; reply notes spawning is paused (PAUSE_MENTIONED: yes)
After /agents resume: New subagents are allowed again.
Final alt+s header read-back: Subagents · spawning on (PAUSE_FINAL: paused=false)

$ TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_PAUSE=1 cargo test -p talaria-app --test live_subagents live_pause -- --ignored --nocapture
running 1 test
PAUSE_MENTIONED: yes
PAUSE_FINAL: paused=false
LIVE_SUBAGENT_RESULT: PASSED
test live_pause_blocks_new_spawns ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 6.24s
```

## P3-05 UAT-1

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 was invalid: Enter arrived before pasted text settled, leaving the command in the composer. After manual submission, a completion matcher mistook the startup notice for the result footer and closed the pane early. Captures are preserved. The temporary runner was corrected before Run 2. The lead [approved the rerun and required this two-run history](https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652). Run 2 used a 213-column, 34-row terminal; both read sources showed the test keystroke. The running row and draft were captured about 0.4 seconds after Enter, then again at two seconds. The result was observed 3.57 seconds after Enter with the draft preserved.

```text
Within 2 seconds:
· Background task bg_adfd05 started. Its answer appears here when it finishes; it is not part of the conversation.
◌ bg · Reply with exactly: bg-ok · running · 0s
│ > still typing
At two seconds:
◌ bg · Reply with exactly: bg-ok · running · 1s
│ > still typing
At completion:
bg · Reply with exactly: bg-ok
  bg-ok
  background task bg_adfd05 · not part of the conversation
│ > still typing
Running-row count after completion: 0
Enter-to-result observation: 3.571 seconds
Cleanup tick process count: 0
TALARIA_EXIT:0
```

The fleet was quiet before and after in its returned 200-session view. The temporary tab was closed and the owned lock released between this row and XF-7.

## P3-05 UAT-2

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The prerequisite heading was present before the live lock was acquired. The terminal measured 169 columns by 29 rows; both read sources displayed the test keystroke. The exact prescribed prompt started the process without an approval. The two overlay reads were 3.27 seconds apart; the second kill key followed the confirmation within 0.3 seconds. The transcript returned after Esc.

```text
Before /processes:
⚙ sh tick.sh · 2s · last: tick 1 · /processes
pgrep -fx 'sh tick.sh': exit=0; count=1
First overlay read:
Background processes · 1 running · 0 finished
▸ ● running              6s   sh tick.sh
tick 2␊tick 3␊tick 4␊tick 5
Second overlay read:
Background processes · 1 running · 0 finished
▸ ● running              9s   sh tick.sh
tick 2␊tick 3␊tick 4␊tick 5␊tick 6␊tick 7␊tick 8
First x:
Press x again within 3 s to kill: sh tick.sh
After second x and 5 seconds:
Background processes · 0 running · 1 finished
▸ ■ killed         ended 4s   sh tick.sh
Killed. Hermes will not tell the agent about this process's exit.
After Esc:
■ sh tick.sh · killed · ended 5s ago · /processes
pgrep -fx 'sh tick.sh': exit=1; count=0
```

UAT-5 followed immediately in this same session under the same lock. The fleet checks before the pair and after cleanup showed no NEEDS YOU or WORKING group. The listing reported its 200-session cap, so these checks cover the returned registry view. The temporary tab was closed and the lock released after the pair.

## P3-05 UAT-3

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The terminal measured 169 columns by 29 rows. The prescribed process prompt was sent twice; both processes were running in the overlay before the count prompt. The count prompt was followed by /stop while the status showed a working turn. At 5.3 seconds after submission, the killed-process row was present. The retained transcript contains exactly one Interrupted notice and no numbers after it.

```text
Before counting:
Background processes · 2 running · 0 finished
pgrep -fx 'sh tick.sh' | wc -l: 2
> Count slowly from one to fifty, one number per line
Status before /stop: 1 working
After /stop:
· Interrupted
· Stopped 2 background processes.
■ sh tick.sh · killed · ended 5s ago · /processes
pgrep -fx 'sh tick.sh': exit=1; count=0
Cleanup tick process count: 0
TALARIA_EXIT:0
```

The fleet was checked before and after under this scenario's own lock, with no NEEDS YOU or WORKING group in its returned 200-session view. The temporary tab was closed and the owned lock released.

## P3-05 UAT-4

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

PASS. Run 1 was a tester mistake: /bg was typed before connection settled and an extra Enter was sent during recovery. Run 2 used a fresh session and waited for the connected composer. Pane size: 106 columns, 44 rows. The fixture fleet listing before and after showed no NEEDS YOU or WORKING group, but was truncated at 200 sessions.

```text
/bg: Usage: /bg <prompt> — runs the prompt on a fresh agent in the background; the answer appears here when it finishes.
No Background task notice appeared.
/processes: Background processes · 0 running · 0 finished
No background processes in this session.
x: No background processes in this session.
S: No background processes are running.
Esc: overlay closed; composer visible.
pgrep -fx 'sh tick.sh' | wc -l: 0
```

## P3-05 UAT-5

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

This continued UAT-2 in the same session and lock: the killed overlay reopened 1.03 seconds after UAT-2's Esc capture, well within 30 seconds. The killed process had ended about six seconds earlier. The final read was 0.27 seconds after Esc, within the five-second limit.

```text
Reopened /processes:
Background processes · 0 running · 1 finished
▸ ■ killed         ended 6s   sh tick.sh
After x:
This process has already ended.
Killing note count: 0
After d:
Background processes · 0 running · 0 finished
No background processes in this session.
After Esc:
Process row count above composer: 0
Composer: Type a message. Enter sends, Shift+Enter adds a line.
pgrep -fx 'sh tick.sh': exit=1; count=0
Cleanup tick process count: 0
TALARIA_EXIT:0
```

The before/after quiet-fleet checks and tab/lock cleanup are those of the chained UAT-2/UAT-5 pair above.

## P3-06 UAT-1

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
frozen candidate 8b514e6; binary checksum 76d4b99642869d73db0ce578d7b13e675c31ebfe2de3d3fcdcef6b00d2672e6d
quiet-fleet: no NEEDS YOU or WORKING group
Scratch folder variable renamed TD, retaining T as frozen binary (section 7.2).
quiet-fleet: no NEEDS YOU or WORKING group
b1: read-source visible verified
b1: terminal 106 columns x 44 rows
c1: read-source visible verified
c1: terminal 106 columns x 44 rows
C session id from fleet JSON: local-8fa2830aea6fd49d644561d669ec6ca2765fe51dab5aad2b0c7da5ff487bfc17
│ > draft-kept                                                                                           │
 minimax-m3.1-flash │ approvals: manual │ …/attention-scratch │ ⚑ 1 needs you
raw bell+needs-you notification count=1; elapsed from C card=30.02s
PASS: exactly one bell followed by needs-you notification
PASS: composer draft kept
Alert was present at the first planned read, 30.02s after C card; exact earlier onset was not sampled. C was terminated through its owned shell parent, not typed /quit on the focused card.
raw notification bytes: <BEL><ESC>]9;Talaria: Remove talaria-attention-probe …: needs you: approval: rm -rf ./talaria-attention-probe<BEL>
```

## P3-06 UAT-2

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
after jump, selected row and peek:
›  ●  Remove talaria-attention-pr…  fixture       minimax-m3.1-flash  approval: rm -rf ./t…  now
   ·  (untitled) (here)             fixture       minimax-m3.1-flash  idle                   now
● Remove talaria-attention-probe directory #18 · fixture · minimax-m3.1-flash · needs you now
$ rm -rf ./talaria-attention-probe
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop
after id filter:
›  ●  Remove talaria-attention-pr…  minimax-m3.1-flash  approval: rm -rf ./talaria-attenti…  now
● Remove talaria-attention-probe directory #18 · fixture · minimax-m3.1-flash · needs you now
$ rm -rf ./talaria-attention-probe
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop
denied using displayed digit 4
›  ●  Remove talaria-attention-pr…  minimax-m3.1-flash  approval: rm -rf ./talaria-attenti…  now
● Remove talaria-attention-probe directory #18 · fixture · minimax-m3.1-flash · needs you now
$ rm -rf ./talaria-attention-probe
  4. deny
1-4 choose · Enter confirm · Esc deny · Tab note · Ctrl+C deny and stop
1-4 choose · Enter confirm · Esc deny · Tab note · ctrl+c back to list
after denial:
│ > draft-kept                                                                                           │
 minimax-m3.1-flash │ approvals: manual │ …/attention-scratch │ 1 working
PASS: needs-you segment cleared
PASS: counter cleared within ten seconds
PASS: ls -d TD/talaria-attention-probe: folder exists
PASS: no renewed approval from denied session
approval answered 34.12s after its card (timeout60s)
quiet-fleet: no NEEDS YOU or WORKING group
[17 unrelated fleet rows omitted under section 5.3.]
ls -d probe (final retained folder): <scratch>/attention-scratch/talaria-attention-probe
```

## P3-06 UAT-3

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
quiet-fleet: no NEEDS YOU or WORKING group
b3: read-source visible verified
b3: terminal 106 columns x 44 rows
PASS: no done segment initially
c3: read-source visible verified
c3: terminal 169 columns x 29 rows
quiet-fleet: no NEEDS YOU or WORKING group
 minimax-m3.1-flash │ approvals: manual │ …/attention-scratch │ ✓ 1 done
unwatched done notification count=0
PASS: never watched session finishes silently
```

## P3-06 UAT-4

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
quiet-fleet: no NEEDS YOU or WORKING group
b4: read-source visible verified
b4: terminal 169 columns x 29 rows
 minimax-m3.1-flash │ approvals: manual │ …/attention-scratch │ ✓ 1 done │ <scratch>/attention-scratch · Run sleep command and echo attention-done
watched done notification count=1; bell+done count=0; seconds after prompt=40.07
PASS: exactly one watched done notification
PASS: done notification has no preceding bell
PASS: done notification arrives at least25s after prompt
PASS: one background done segment
quiet-fleet: no NEEDS YOU or WORKING group
raw notification bytes: <ESC>]9;Talaria: Run attention completion command: done<BEL>
```

## P3-06 UAT-5

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
quiet-fleet: no NEEDS YOU or WORKING group
c5: read-source visible verified
c5: terminal 169 columns x 29 rows
b5: read-source visible verified
b5: terminal 169 columns x 29 rows
· 1 background session needs you; press alt+a to see it.
 minimax-m3.1-flash │ approvals: manual │ …/attention-scratch │ ⚑ 1 needs you
startup notice latency=6.05s; notification bytes count=0; notice line count=1
PASS: startup counts waiting approval
PASS: no startup notification burst
PASS: exactly one startup notice
PASS: startup notice within16s
denied using displayed digit 4
PASS: probe folder preserved after denial
quiet-fleet: no NEEDS YOU or WORKING group
```

## P3-06 UAT-6

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

```text
Run 1: tester used raw cursor-positioning output and typed x before confirming the notice; composer assertion was inconclusive. Original receipt retained. Run 2 approved: https://github.com/infiquetra/talaria/issues/248#issuecomment-6430720001. This capture waits for the notice, then types x and reads visible without --raw; script -F flushes byte counts.
frozen candidate 8b514e6; binary checksum 76d4b99642869d73db0ce578d7b13e675c31ebfe2de3d3fcdcef6b00d2672e6d
quiet-fleet: no NEEDS YOU or WORKING group
b6-final: read-source visible verified
b6-final: terminal 169 columns x 29 rows
· Nothing in the background needs you.
╭───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╮
│ > x                                                                                                                                                                   │
╰───────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────────╯
 minimax-m3.1-flash │ approvals: manual │ …/attention6-capture

BEL before=0; after=0
PASS: empty jump did not ring
PASS: composer receives x; no overlay
PASS: no fleet overlay
quiet-fleet: no NEEDS YOU or WORKING group
```

## P3-07 UAT-1

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS. The two-call input needed a short delay between text and Enter; the first attempt left the text in the composer, which was cleared before the scenario interaction.

```text
Review: ? hello.txt +1 -0
Nothing is staged: this commit will include all 1 changed files on talaria/hello.
Committed 85498af on talaria/hello: feat: add hello
Pushed talaria/hello to origin.
Already up to date on origin.
git log -1 --format=%B: feat: add hello
git branch --list 'talaria/*': talaria/hello
```

## P3-07 UAT-2

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS. The first draft appeared within the 60-second wait; the exact elapsed time was not captured.

```text
Review: M hello.txt +1 -0
Nothing is staged: this commit will include all 1 changed files on talaria/hello.
Drafting a message with minimax-m3.1-flash…
First draft: feat: add world
Second draft: feat: extend hello message with world
```

## P3-07 UAT-3

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844045

```text
Confirmation on shift+p: Open draft pull request from talaria/live-7d7da6da into main (1 commits)?
Notice after Enter: Draft pull request opened: https://github.com/<account>/<repo-1>/pull/346
Fleet row PR column after Esc: #346 draft
$ gh pr view 346 --json isDraft -q .isDraft
true
$ gh pr close 346 --delete-branch
✓ Closed pull request <account>/<repo-1>#346 (live recording)
✓ Deleted branch talaria/live-7d7da6da and switched to branch main

$ TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_review_gh -- --ignored --nocapture
running 1 test
create stdout: https://github.com/<account>/<repo-1>/pull/346
graphql stdout: {"data":{"repository":{"b0":{"nodes":[{"number":346,"state":"OPEN","isDraft":true,"isCrossRepository":false,"url":"https://github.com/<account>/<repo-1>/pull/346","headRefName":"talaria/live-7d7da6da","reviewDecision":null,"commits":{"nodes":[{"commit":{"statusCheckRollup":null}}]}}]}}}}
close stderr: ✓ Closed pull request <account>/<repo-1>#346 (live recording)
✓ Deleted branch talaria/live-7d7da6da and switched to branch main
test draft_pr_round_trip ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 14.14s
```

## P3-07 UAT-4

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

PASS. Both panes were 106 columns by 44 rows. The fixture fleet listing before and after showed no NEEDS YOU or WORKING group; it was truncated at 200 sessions.

```text
/changes: Review · repo · main · 1 file · +1 −0
p: Talaria does not push or open pull requests from main, the default branch.
shift+p: Talaria does not push or open pull requests from main, the default branch.
git -C <tmp>/origin.git branch:
* main
/changes in empty folder: <tmp> is not inside a git repository.
No review opened in the empty folder.
```

## P3-07 UAT-5

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

PASS. Pane size: 106 columns, 44 rows. The fixture fleet listing surrounding this row showed no NEEDS YOU or WORKING group; it was truncated at 200 sessions.

```text
Cursor: ? scratch.txt
shift+x: Delete untracked scratch.txt? Press X again to discard; any other key cancels.
j: Discard cancelled.
Cursor: ? scratch.txt
shift+x, shift+x: Deleted untracked scratch.txt.
ls repo:
scratch2.txt
tracked.txt
```

## P3-08 UAT-1

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 restored both attachments but did not establish fresh timing; changing terminal geometry and lock custody invalidated its acceptance capture. Run 2 waited for the actual empty composer and rename acknowledgement, sent the prescribed counting prompt once, and captured both attaches with fresh native-scrollback boundaries. Both completed within two seconds without a profile flag. Visible and native-scrollback reads were saved for each; the native Resumed header is authoritative after 40 lines scroll above the viewport, as approved in https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652. Both exits and before/after quiet-fleet checks passed.

```text
$T attach --no-ensure 6e1b1323
Enter-to-Resumed: 1.213 seconds
── Resumed: attach-uat-3afb8f ──
> Count from 1 to 40, one number per line.
Counting excerpt: 1 ... 40
exit=0
$T attach --no-ensure attach-uat-3afb8f
Enter-to-Resumed: 1.351 seconds
── Resumed: attach-uat-3afb8f ──
> Count from 1 to 40, one number per line.
Counting excerpt: 1 ... 40
exit=0
```

## P3-08 UAT-2

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 could not create the fixture duplicate because an old zero-message fixture row held the title outside the newest-session window. Its runner then mistook an opened default viewer for an exited negative command and sent test text into that viewer. The operator removed the stale fixture row and restored the prepared default row to zero messages. Run 2 used the fixed tag 9ac246, waited for the rename acknowledgement, and ran all six commands in order at a verified shell, with the expected exit codes. The prepared default session still has zero messages on read-only readback. The viewer in case 3 was quit before the remaining shell commands. Approved by https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652.

```text
Fixture setup: $T --no-ensure --profile fixture
/rename attach-dup-9ac246
Renamed to "attach-dup-9ac246".
/quit; exit=0
$T attach --no-ensure zz-none-9ac246
talaria: no session matches "zz-none-9ac246" in the profiles default, fixture.
exit=1
$T attach --no-ensure attach-dup-9ac246
talaria: 2 sessions match "attach-dup-9ac246":
  fixture  local-9977f35b…  "attach-dup-9ac246"  live  now
  default  local-acc89ab3…  "attach-dup-9ac246"  live  11h
exit=2
$T attach --no-ensure attach-dup-9ac246 --profile fixture
── Resumed: attach-dup-9ac246 ──
exit=0
$T attach --no-ensure "  "
talaria: the session to attach to is empty.
exit=2
$T attach --no-ensure abcd --model x
error: unexpected argument '--model' found   tip: to pass '--model' as a value, use '-- --model' Usage: talaria attach --no-ensure <SESSION> For more information, try '--help'.
exit=2
$T attach --no-ensure abcd --profile <profile>
talaria: the profile <profile> is not served by the gateway (served: default, fixture).
exit=2
Default prepared duplicate read-only readback: message_count=0
Quiet fleet before and after: no NEEDS YOU or WORKING header.
```

## P3-08 UAT-3

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 was blocked by narrow geometry. Run 2 setup diagnostics also exposed that a child shell on the existing Herdr server lacked the isolated Hermes home; an initial checker also used the wrong case for the visible peek marker. These incomplete diagnostic captures were retained and do not contribute to the passing observations. The complete Run 2 capture used a disposable Herdr server whose parent and child shell environments were verified, with a 214-column pane before the split. The application opened exactly one adjacent viewer; both panes stayed above 100 columns, both actual focus fields retained the parent, the peek stayed open, and j moved its cursor. The child resumed within three seconds and returned to a shell after /quit. The disposable server was stopped and removed. Approved by https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652.

```text
Pane list before (id, workspace, tab, focused):
w1:p1 w1 w1:t1 False
w1:p6 w1 w1:t4 True
Pane list after (same fields):
w1:p1 w1 w1:t1 False
w1:p6 w1 w1:t4 True
w1:p7 w1 w1:t4 False
Focus read from pane-list focused and layout.focused_pane_id: parent remains selected
Parent width before split: 214 columns
Split widths: 107, 107 columns
Parent: Opened "attach-uat-3afb8f" in a new Herdr pane.
Parent: ── peek ──
Parent selected row before j: ›  ·  attach-uat-3afb8f             fixture       minimax-m3.1-flash  idle                  11m
Parent selected row after j: ›  ·  (untitled)                    fixture       minimax-m3.1-flash  idle                  17m
o-to-Resumed: 1.591 seconds
Child: ── Resumed: attach-uat-3afb8f ──
Child shell command: '$T' attach 'local-6e1b13236b58e424b7ba71151de73c7528e32953d6f7cd01c6b2b8c7664a13f8' --profile 'fixture'
Child resume hint: talaria --resume local-6e1b13236b58e424b7ba71151de73c7528e32953d6f7cd01c6b2b8c7664a13f8 --profile fixture
Child exit=0
Quiet fleet before and after: no NEEDS YOU or WORKING header.
```

## P3-08 UAT-4

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 delivered /fleet and Enter together and never opened the overlay. Run 2 used the approved text/Enter pacing and actual startup checks. Its initial environment diagnostics found a personal after-split hook moving the new viewer into another window. That hook was disabled only on the disposable server; the isolated Hermes home was set for its child shells. The test terminal was widened to 214 columns so both resulting panes meet section 3's 100-column minimum. The complete capture shows two panes in one window, the original still active, and the resumed counting session in the new pane. Earlier diagnostic captures were retained separately. The server was killed in cleanup. Approved by https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652.

```text
Startup waited for actual empty composer and approvals: manual
/fleet submitted as literal text, 0.4-second delay, then Enter
Waited for Fleet · before j and o
Pane list (pane id, active):
%0 1
%1 0
Pane sizes (pane id, columns, rows):
%0 107 49
%1 106 49
Parent: Opened "attach-uat-3afb8f" in a new tmux pane.
Child: ── Resumed: attach-uat-3afb8f ──
Child counting excerpt: 1 ... 40
o-to-Resumed: 1.533 seconds
Isolated tmux server killed after captures
Quiet fleet before and after: no NEEDS YOU or WORKING header.
```

## P3-08 UAT-5

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 found the outside-multiplexer notice but its downward-only search missed the foreground row. Run 2 used k to reach (here), captured its refusal, and verified that both pane lists contained the same identifiers. The full outside-multiplexer command was captured after widening only the disposable test client so the command was not clipped. Both visible reads and before/after lists were retained; the client exited 0 and its temporary pane was closed. Approved by https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652.

```text
Pane ids before: w1:p1, w1:p8
Not inside Herdr or tmux. To watch "attach-uat-3afb8f" side by side, run this in another terminal: '$T' attach 'local-6e1b13236b58e424b7ba71151de73c7528e32953d6f7cd01c6b2b8c7664a13f8' --profile 'fixture'
After k selected the foreground (here) row:
This session is already open in this pane.
Pane ids after: w1:p1, w1:p8
Pane-id sets unchanged: true
exit=0
Quiet fleet before and after: no NEEDS YOU or WORKING header.
```

## XF-1

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

PASS. Pane size: 106 columns, 44 rows; READ-CHECK appeared in visible. Each of the five palette reads displayed Tab complete. No Enter was sent. The fixture fleet listing immediately before alt+a showed no NEEDS YOU or WORKING group, but was truncated at 200 sessions.

```text
/fleet  Show every live session and peek, reply, stop or open one  talaria
/agents  Show the session's subagents; `pause` and `resume` switch spawning  talaria
/processes  Show this session's background processes, with live output and kill  talaria
/bg  Run a prompt on a fresh agent in the background; the answer appears here  talaria
/changes  Review the foreground session's changed files  talaria
alt+s: No subagents in this session yet. They appear when the agent uses delegate_task.
alt+a: Nothing in the background needs you.
```

## XF-2

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
M2: glm-5.3-flash
launched local-20
launched local-47
launched local-a5
All three ids present; needs_you=1 belongs to second launch; final needs_you=0; failed=0.
NEEDS YOU (1)
›  ●  Run this exact shell comman…  fixture       glm-5.3-flash       approval: rm -rf ./probe                                                              now
● Run this exact shell command: rm -rf ./probe · fixture · glm-5.3-flash · needs you now
$ rm -rf ./probe
  4. deny
Fourth session retained both 40-character draft ends and minimax-m3.1-flash; no attention flag.
probe exists; second launch retains glm-5.3-flash.
launch receipt: {"admission_id": "d8ac34c75cb7452185d4ccfcf3f675c7", "admission_status": "queued", "cwd": "<scratch>/xf-2/p1", "error": null, "format": "talaria.bg.v1", "model": "minimax-m3.1-flash", "profile": "fixture", "session_id": "local-20", "status": "launched", "stored_session_id": "local-20", "warnings": [], "worktree": null}
launch receipt: {"admission_id": "c9d8908079b547aabeac66ee74e9af16", "admission_status": "queued", "cwd": "<scratch>/xf-2/p2", "error": null, "format": "talaria.bg.v1", "model": "glm-5.3-flash", "profile": "fixture", "session_id": "local-47", "status": "launched", "stored_session_id": "local-47", "warnings": [], "worktree": null}
launch receipt: {"admission_id": "e59ed19b0acb41a8bd6a8f147d414a1c", "admission_status": "queued", "cwd": "<scratch>/xf-2/p3", "error": null, "format": "talaria.bg.v1", "model": "minimax-m3.1-flash", "profile": "fixture", "session_id": "local-a5", "status": "launched", "stored_session_id": "local-a5", "warnings": [], "worktree": null}
v3 after denial (fourth session):
│ > abcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefghiabcdefgh   │
│   iabcdefghiabcdefghiabcdefghiabcdefghiab                                                                                                                             │
 minimax-m3.1-flash │ approvals: manual │ …<scratch>/xf-2/p4 (main) │ 1 working │ ✓ 1 done
Reassembled draft: grep first40=1; grep last40=1 (single reassembled line).
ls -d probe (final retained folder): <scratch>/xf-2/p2/probe
```

## XF-3

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 lacked a rename acknowledgement and the model returned only its token without counting. Run 2 waited for the real empty composer and the exact rename acknowledgement before submitting the unchanged prompt. The first authorized model attempt reached line 10; the originating viewer was quit, its resume hint was captured, and the fleet reported idle with that viewer closed. A new viewer attached by title and displayed line 40 and the joined token, then exited 0. No fresh-session model retry was needed. Approved by https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652.

```text
Renamed to "fixture xf3 3158".
Prescribed counting-and-token prompt submitted once in this rerun
Standalone line 10 observed before /quit
Resume hint: talaria --resume local-0d1133966cc8c7353e18e7da02a858492b17549a4909236d73f6b11f559ab30f --profile fixture
Last fleet state with the originating viewer closed: idle
$T attach --no-ensure --profile fixture "fixture xf3 3158"
── Resumed: fixture xf3 3158 ──
40
alder-4864
exit=0
Model attempts in Run 2: 1
Quiet fleet before and after: no NEEDS YOU or WORKING header.
```

## XF-4

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS. Run 1 reported a missing notice while the fleet overlay was open; the specification was corrected to capture the main transcript, and the operator approved this rerun. On Run 2, recent-unwrapped while the overlay was open showed the overlay; closing it with Escape exposed the notice in the visible transcript. The fleet view was then reopened for review, commit and push.

```text
Launched in fixture: "Create hello.txt containing hello" (worktree talaria/create-hello-txt-containing-hello)
Worktree session state: done; hello.txt exists.
Review: ? hello.txt +1 -0
Committed c3b7b5a on talaria/create-hello-txt-containing-hello: feat: add hello
Pushed talaria/create-hello-txt-containing-hello to origin.
origin branches: talaria/create-hello-txt-containing-hello
main checkout status --porcelain: empty
main log -1 --format=%s: one
find hello.txt: <tmp>/repo/.worktrees/talaria-create-hello-txt-containing-hello/hello.txt (one match)
```

The fleet list reported only its 200 newest live sessions; the visible list had no NEEDS YOU or WORKING group at the checks before and between the original rows, after the last original row, and before the rerun. Older hidden sessions cannot be ruled out from that output.

## XF-5

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

```text
NEEDS YOU (1)
  fixture  Run rm -rf ./probe                        minimax-m3.1-flash  approval: rm -rf ./probe  now
JSON needs_you: 1 -> 0; same launched session.
 minimax-m3.1-flash │ approvals: manual │ …<scratch>/xf-5 │ ⚑ 1 needs you
NEEDS YOU (1)
›  ●  Run rm -rf ./probe            fixture       minimax-m3.1-flash  approval: rm -rf ./probe                                                              now
● Run rm -rf ./probe · fixture · minimax-m3.1-flash · needs you now
$ rm -rf ./probe
  4. deny
Four-surface capture windows: before=1.71s after=3.70s.
After denial: text fleet has no NEEDS YOU; own session status has no flag and retains its folder/model.
probe exists.
after denial fleet header: WORKING (1)
own status after denial:  minimax-m3.1-flash │ approvals: manual │ …<scratch>/xf-5 │ ✓ 1 done
ls -d probe (final retained folder): <scratch>/xf-5/probe
```

## XF-6

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The terminal measured 169 columns by 29 rows. Both read sources showed READ-CHECK and the test keystroke; visible was used for the overlay and dock reads. The exact prescribed prompt started one child. All four stated pass predicates hold. After x, the child was pruned from the panel before the five-second poll observed a stopped row; that poll reached its 60-second timeout. Closing the overlay exposed the committed stopped summary in native scrollback. No extra live attempt was made.

```text
a: fleet overlay
IDLE (201)
›  ·  Run sleep commands w… (here)  fixture  minimax-m3.1-flash  idle  now
b: after closing the fleet overlay
⎇ Subagents · 1 running · Alt+S to open
  ◐ Run the shell command `sleep 3` ten times, one command at a time
c: subagent panel
Subagents · Run sleep commands with delegate_task
1 running · 0 finished · spawning on · depth ≤1 · ≤10 at once
▸ ◐ Run the shell command `sleep 3` ten times  minimax-m3.1-flash  6s  1 tool
After x, panel before close:
0 running · 0 finished
No subagents in this session yet. They appear when the agent uses delegate_task.
Stop requested for "Run the shell command `sleep 3` ten time".
d: native scrollback after closing the panel
⎇ Subagents · 1 finished · 1 stopped · 7s
  ■ Run the shell command `sleep 3` ten times, one command at a time
Stopped summary count: 1
WORKING ( / IDLE ( line count in native scrollback: 0
TALARIA_EXIT:0
```

The fleet was quiet before and after in its returned 200-session view. The temporary tab was closed and the owned lock released.

## XF-7

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

Run 1 used token bg-3361. At its two-second capture the answer was already complete, before the fleet overlay opened. The startup notice and result footer also produced two generic phrase matches rather than the original requirement of one. The lead [corrected the timing and footer check and approved Run 2](https://github.com/infiquetra/talaria/issues/248#issuecomment-6430930652): capture within two seconds, open the overlay immediately, and count the result-footer line separately. Run 2 used the unchanged prompt with token bg-5972. Its early capture showed the running row. All 24 overlay reads showed the current-session row, without intervening keys. After the overlay closed, the result cell and its footer appeared once, the token appeared on two lines, no running row remained, and no fleet group lines appeared in native scrollback.

```text
a: within-two-second capture, before /fleet
· Background task bg_072b2c started. Its answer appears here when it finishes; it is not part of the conversation.
◌ bg · Reply with exactly: bg-5972 · running · 0s
o: overlay observation
›  ·  (untitled) (here)  fixture  minimax-m3.1-flash  idle  now
Current-session (here) row present in 24 of 24 reads.
b/c: after closing the overlay
· Background task bg_072b2c started. Its answer appears here when it finishes; it is not part of the conversation.
bg · Reply with exactly: bg-5972
  bg-5972
  background task bg_072b2c · not part of the conversation
grep -c 'bg · Reply with exactly: bg-5972': 1
grep -cw 'bg-5972': 2
grep -c 'background task bg_.* · not part of the conversation': 1
grep -c 'not part of the conversation': 2 (startup notice + result footer)
Running-row count after close: 0
WORKING ( / IDLE ( line count in native scrollback: 0
TALARIA_EXIT:0
```

Run 2's terminal measured 213 columns by 34 rows and both read sources passed READ-CHECK. Each run acquired its own lock and checked the fleet before and after, quiet in its returned 200-session view. Each temporary tab was closed and the owned lock released. Final tick process count was zero. Both runs' raw captures were retained; the evidence block above is Run 2 only.

## XF-8

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

Two full-width viewers attached to the same completed background session. Viewer 1 submitted the prescribed token prompt. Both native-scrollback reads contain the joined token as a whole line; viewer 2 was read immediately after viewer 1. Both clients were quit, shell foreground was checked, and exit=0 was captured. Both temporary panes were closed. Quiet-fleet checks passed before and after. The read-check setup returned READ-CHECK and 169 columns by 29 rows.

```text
Token: kestrel-4829
Viewer 1:
── Resumed: Reply with ready #26 ──
kestrel-4829
exit=0
Viewer 2:
── Resumed: Reply with ready #26 ──
kestrel-4829
exit=0
```

## XF-9

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

All three background sessions reached idle before measurement. The client opened fleet/peek, subagents and processes, then completed the prescribed ready prompt. After the five-second settling wait, the client remained idle for at least 120 seconds. The gateway lock was released for that idle interval; no turn was sent during it. All four limits passed and the visible captures are byte-identical. The temporary client pane was closed, and a final quiet-fleet check passed. KiB means kibibytes; RSS means resident memory; CPU time means processor time.

```text
Three background session terminal states: idle, idle, idle
Sample 0: RSS=24992 KiB; time=0:00.06; seconds=0.06
Sample 1: RSS=27648 KiB; time=0:00.11; seconds=0.11
Sample 2: RSS=29104 KiB; time=0:00.41; seconds=0.41
Elapsed idle seconds: 120.00965249998262
RSS growth: 1456 KiB (limit 4096)
CPU-time growth: 0.30 seconds (limit 1)
Start limit: 40960 KiB; final ceiling: 102400 KiB
diff output: empty
diff exit=0
Final quiet-fleet check: no NEEDS YOU or WORKING header.
```

## XF-10

Candidate: 8b514e6. Runner: agent tester. Run 2 of 2. Date: 2026-10-26.

Result: PASS

```text
Run 1: prescribed third Esc reached the foreground session and interrupted sleep90 after peek and overlay had closed. Original failure receipt retained.
Run 2: two Esc presses per approved specification correction: https://github.com/infiquetra/talaria/issues/248#issuecomment-6430720001.
q0: Reply with the word omega in capital letters.                                                                                                       queued (1) · ↑ select
q1: Reply with the word omega in capital letters.                                                                                                       queued (1) · ↑ select
 minimax-m3.1-flash │ approvals: manual │ …<scratch>/xf-10 │ ⚑ 1 needs you │ 1 working
q2: Reply with the word omega in capital letters.                                                                                                       queued (1) · ↑ select
q2 has no attention flag; probe exists.
FIRST
> Reply with the word omega in capital letters.
OMEGA
omega user line count=1; steer count=0; verified transcript order FIRST < user prompt < OMEGA.
ls -d probe (final retained folder): <scratch>/xf-10/probe
```

## XF-11

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

Result: PASS

The baseline and final launches used only --no-ensure --profile fixture, without model flags. Both status lines named the same default. The completed background session on the supplied second model appeared in both the selected JSON row and text fleet listing. The saved-model file was absent before and after. Both temporary client instances exited and their pane was closed.

```text
Saved default before: minimax-m3.1-flash
Background session model: glm-5.3-flash
Background session state: idle
Matching model lines in the selected JSON row: 1
Matching model lines in fleet text: 3
Saved default after: minimax-m3.1-flash
Saved-model file before: absent
Saved-model file after: absent
Comparison: absent on both sides; no saved-model file created or changed.
Baseline client exit=0
Final client exit=0
Quiet fleet before and after: no NEEDS YOU or WORKING header.
```

## XF-12

Candidate: 8b514e6. Runner: agent tester. Run 1 of 1. Date: 2026-10-26.

PASS. Each empty-home listing produced no output; each command completed in less than one measured second.

```text
fleet list:
talaria fleet: no Hermes gateway is running for this Hermes home. Start one with "hermes gateway ensure", or start talaria.
exit=1 elapsed=0s
ls -A: empty
--bg --no-ensure "hi":
talaria: no Hermes gateway is running for this home.
Next step: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.
exit=4 elapsed=0s
ls -A: empty
attach --no-ensure zz-none:
talaria: no Hermes gateway is running for this home.
Next step: Run `hermes gateway ensure --json` to start the Hermes gateway, then try again.
exit=1 elapsed=0s
ls -A: empty
```

## OP-1

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
OP-1: PASS
Fleet window duration: 45 minutes (>= 30 minutes) on 2026-10-26.
Three background sessions launched (--bg, n, w) on three distinct providers:
1. provider=ollama-cloud model=minimax-m3.1-flash repo=<repo-1> state=working -> done
2. provider=openrouter model=glm-5.2 repo=<repo-1> state=working -> done
3. provider=anthropic model=claude-sonnet-5-5 repo=<repo-2> state=working -> done
Fleet view listed all three concurrently with their models while running; all three reached done.
```

## OP-2

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
OP-2: PASS
Foreground coding session active in <repo-1> for 45 minutes (>= 30 minutes) during the fleet window.
Status line named the foreground session throughout.
Messages >= 200 characters typed while background work ran: 5 (alerts arrived during 3 of them).
Lost, doubled, or reordered characters: 0.
Composer text cleared or replaced by alerts, overlays, or notices: 0.
```

## OP-3

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
OP-3: PASS
Background approval requests answered from the peek panel (alt+a / /fleet -> space -> a -> digit): 4 (>= 3).
Breakdown: 2 deny (verified target paths untouched; denied commands did not run), 2 allow once (verified commands ran once).
Misrouted answers: 0.
Unseen timeouts (300 s): 0.
```

## OP-4

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
OP-4: PASS
Worktree session launched on <repo-1> (GitHub origin); edits confined under .worktrees/.
Main checkout git status --porcelain before: "" (empty)
Main checkout git status --porcelain after: "" (empty)
Reviewed with v, committed, pushed, and opened draft PR via shift+p: https://github.com/<account>/<repo-1>/pull/346
Fleet row PR column: #346 draft (and check status matched live checks).
Closed and branch deleted:
✓ Closed pull request <account>/<repo-1>#346
✓ Deleted branch talaria/live-7d7da6da and switched to branch main
```

## OP-5

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
OP-5: PASS
Needs-you events in fleet window: 4; each rang terminal bell once and raised 1 desktop notification.
Unwatched-session done/failed alerts: 0.
Status-line counter spot checks against talaria fleet list: 3/3 matched.
Failed sessions: failed: none occurred.
```

## OP-6

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
OP-6: PASS
Subagents tailed live in alt+s: 2; steered: 1; stopped: 1.
Background processes watched in /processes and killed (x twice): 1.
Sessions stopped from /fleet: 1.
/bg side prompt results read: 3 (>= 2).
Open in new pane (o): used in Herdr 1 time, used in tmux 1 time.
talaria attach from a second terminal: used 2 times.
```

## OP-7

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
OP-7: PASS
Quit Talaria with 1 session working and 1 waiting for approval (< 240 s), then reopened:
- Startup notice shown: "1 background session needs you; press alt+a to see it."
- Finished sessions listed in fleet; talaria attach showed complete transcript.
Fleet row state spot checks across days: 10/10 matched real session state.
Rows stuck working > 10 s after turn ended: 0.
Repeated, ghost, or garbled rows after 3 terminal resizes in Herdr and 1 in plain terminal: 0.
Resident memory at end of longest session (48 messages, 4 sessions in fleet): ps -o rss= printed 34816 KiB (<= 102400 KiB).
```

## OP-8

Candidate: 8b514e6. Runner: operator. Run 1 of 1. Date: 2026-10-26.

Result: PASS. Operator comment: https://github.com/infiquetra/talaria/issues/248#issuecomment-6431844309

```text
Binary SHA-256: 76d4b99642869d73db0ce578d7b13e675c31ebfe2de3d3fcdcef6b00d2672e6d
Day log: 2026-10-25 (2.5h, Herdr + plain terminal) + 2026-10-26 (3.0h, Herdr + tmux, 45m fleet window) = 5.5h total across 2 days; restarts=0; config changes=0; fallbacks=0.
PAR-1: works | used 8 times on 2026-10-25..2026-10-26 | 4 background launches (--bg, n, w), fleet view opened 9 times, talaria attach from second terminal twice, o in Herdr and tmux
PAR-2: works | used 5 times on 2026-10-25..2026-10-26 | 4 approvals answered from peek panel (2 deny, 2 allow once), 1 reply queued from peek, 2 sessions opened as foreground
PAR-3: works | used 6 times on 2026-10-25..2026-10-26 | needs-you bell and OSC 9 notification 4 times, watched-session done notification 2 times, alt+a jump key 4 times; failed: none occurred
PAR-4: works | used 4 times on 2026-10-25..2026-10-26 | subagent dock and alt+s panel used on both days; 2 children tailed live, 1 child steered, 1 child stopped
PAR-5: works | used 5 times on 2026-10-25..2026-10-26 | /processes used 3 times, 1 background shell process killed via x twice, 3 /bg side prompts completed and rendered
PAR-6: works | used 4 times on 2026-10-25..2026-10-26 | 3 worktree sessions (-w and w in /fleet), review + commit + push via v/c/p, draft pull request #346 opened via shift+p with #346 draft badge and closed cleanly
Defects found: none (D = 0, F = 0, G = 0)
GAPS ACCEPTED: none
PHASE 3 OPERATOR CHECK: PASSED (candidate 8b514e6)
```
