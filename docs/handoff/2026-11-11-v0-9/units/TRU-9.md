# TRU-9: Move the snapshot-rebuild path out of app.rs into its own module

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue says what to move. This file is the map of the functions, the call sites that stay, and the tests that already cover them. It restates no requirement and no test. It refers to them by id.

## The seven items

All of these are in `crates/talaria-app/src/app.rs` today.

| Item | Line | Visibility | What it does |
| --- | --- | --- | --- |
| `RebuildMode` | `app.rs:2747` | `pub enum` | `AfterGap`, or `AfterRewind { notice }` |
| `rebuild_foreground` | `app.rs:2757` | `pub fn` | Matches the mode and calls one of the two rebuilds |
| `rebuild_after_gap` | `app.rs:2768` | private | Decodes a `CanonicalSnapshot` through `OpenedSession::from_snapshot` (`sessions.rs:66`) and calls `recovered` |
| `rebuild_after_rewind` | `app.rs:2783` | private | Same decode, then `carry_over` (`crates/talaria-core/src/side/mod.rs:139`) and `final_prefix` |
| `recovered` | `app.rs:9635` | private | Installs the opened session, or the not-found path |
| `recover_retained` | `app.rs:9732` | private | Same for a session that is not the foreground one |
| `final_prefix` | `app.rs:9795` | private | Counts leading final cells |

`carry_over` is imported in `app.rs` from `talaria_core::side` (`app.rs:76-78`). Call sites that stay in `app.rs` also use it (`app.rs:9197` and `app.rs:9213`), so that import stays. The moved functions need their own import.

## Call sites that stay one line

- `AppMessage::BackgroundRecovered` (`app.rs:3525-3530`) calls `recovered` when the session is open and `recover_retained` otherwise.
- `AppMessage::Recovered` (`app.rs:4140`) calls `recovered`.
- `rewind_input` (`app.rs:2945`) handles `RewindEffect::Rebuild` with one `rebuild_foreground` call (`app.rs:2963-2967`).
- `resume_header` (`app.rs:9830`) calls `final_prefix` (`app.rs:9795`). That function moves. The call stays.

`apply` (`app.rs:11546`) applies one message and runs its effects. It does not call the seven functions. REQ-2 leaves it untouched.

The moved functions call private helpers of `app`. A sibling module cannot see those. REQ-2 names each one that must become `pub(crate)` and each field that is already public. `notice_lines` is `app.rs:2345`. `retained_sessions` is `app.rs:832`. `cd_ledger` is `app.rs:957`. `folder` is `app.rs:959`. `REFRESH_FAILED_NOTICE` is `app.rs:153`.

`SessionService::recover` (`sessions.rs:404`) calls `self.snapshot("session.resume", ...)`. That string is the method name. The moved code does not call `recover` or `snapshot_from` (`sessions.rs:445`). REQ-3 leaves both unchanged.

## Re-export

`crates/talaria-app/tests/rewind_service.rs:18-20` imports `RebuildMode` and `rebuild_foreground` from `talaria_app::app`. REQ-1 re-exports those two names from `app.rs` so that import still compiles. The other five functions are not imported by tests.

`crates/talaria-app/src/lib.rs` lists `pub mod queue;` then `pub mod review;` (`lib.rs:41-42`). `pub mod rebuild;` goes between them. Without that line the new file is not part of the crate.

## Tests that already cover the path

None of these files is edited.

- `session_flow.rs:1426` `gap_resumes_rebuilds_resyncs_without_duplicates`. AC-1.
- `rewind_service.rs:425` `rewind_rebuild_preserves_pending_side_work_and_request_ids`. AC-1.
- `rewind_service.rs:443` `rewind_rebuild_marks_cells_without_reprint`. AC-1.
- `rewind_replay.rs:54` `recorded_rewind_and_rollback_frames_drive_expected_screens`. It loads `fixtures/frames/rewind-and-rollback.jsonl` and calls `rewind_turns` and `restore_summary`. It does not call the seven functions. AC-1.
- `prompts_props.rs:328` `every_server_request_answered_at_most_once`. AC-2. This is the guard that the prompt-card path did not move.

## What else must not move

`answer_server_request` is at `app.rs:10000`. `slash_line` stays. The prompt types stay in `crates/talaria-app/src/prompts.rs`.
