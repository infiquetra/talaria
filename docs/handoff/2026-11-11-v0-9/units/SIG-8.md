# SIG-8: Show the compaction, error and budget lines Hermes prints during a turn

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

The issue states REQ-1 through REQ-4, the tests, and the stop conditions. This note maps those ids onto the current tree. It does not add a requirement. Line numbers are from Talaria `main` at the draft, and from Hermes tag v0.23.0. REQ-3 states the re-verify. This issue does not wait on [#544](https://github.com/infiquetra/talaria/issues/544) (`PIN-3`), so the pin at start is Hermes v0.22.4.

## What the reducer does today

`update` (`crates/talaria-core/src/session/reducer.rs:14`) reads no clock (`reducer.rs:13`). An empty session id, or a session id that is not the open session, returns immediately (`reducer.rs:21-22`). A `seq` less than or equal to the cursor returns immediately (`reducer.rs:40-42`). The `unknown` flag passed into `session` (`reducer.rs:318`) is `matches!(frame.notification, Notification::Unknown(_))` (`reducer.rs:86`). After `session` returns, `update` returns an empty effect list unless the name is `tool.complete` (`reducer.rs:88-92`).

`agent.print` is a generated notification (`crates/hermes-contract/src/events.rs:181`). `AgentPrintPayload` has one field, `text: String` (`crates/hermes-contract/src/generated/schemas.rs:204-206`). The schema comment names subagent completion, compaction, errors, and budget notices (`schemas.rs:202-203`). That comment is not an instruction to draw a status row. REQ-1 forbids a status-row change.

A decoded `agent.print` has `unknown == false`. The empty arm (`reducer.rs:523-532`) matches `_ if !unknown`, so the event stores nothing. It does not reach the diagnostic arm (`reducer.rs:533-548`). The hand-built helper `ev` (`crates/talaria-core/tests/support/events.rs:61`) calls `EventFrame::from_params` (`events.rs:28-31`). A payload that helper refuses uses `Notification::Unknown` (`events.rs:56`), whose type name is still the raw `type`. A missing or null `text` takes that path today and can store the diagnostic notice. REQ-1's arm has to match the name before that fall-through.

`status.update` reads `payload["text"].as_str()` and writes `state.status` (`reducer.rs:495-512`). `error` reads `payload["message"].as_str()` and calls `state.notice` (`reducer.rs:518-521`). REQ-1 follows the `as_str` read and does not follow the status write.

`SessionState::notice` (`state.rs:362-374`) appends `Cell::Notice` and does not trim. `NoticeKind::System` exists (`cell.rs:157`). Its comment says a system row from a snapshot. REQ-1 uses that variant and does not add one. `Cell::is_final` is true for every notice (`cell.rs:40`), so `advance_prefix` (`state.rs:410-417`) can count the new cell once every earlier cell is final.

`remove_whitespace` (`text.rs:29-30`) deletes every Unicode whitespace character. REQ-1 uses `str::trim` instead, so interior spaces stay.

`status()` (`state.rs:280`) and `status_log()` (`state.rs:285`) read the fields at `state.rs:119-120`. REQ-1 leaves both alone. `cells()` is `state.rs:198`. `unknown_event_types()` is `state.rs:332`.

`interrupted_notice` (`reducer.rs:630-634`) writes `NoticeKind::Interrupted` with text `Interrupted`, once. A log that both prints and then completes as interrupted can hold both cells. REQ-1 does not suppress the print.

`AlertKind` is `NeedsYou`, `Done`, and `Failed` (`crates/talaria-core/src/attention.rs:22-28`). `NeedKey` starts at `attention.rs:44`. `AttentionTracker::observe` is `attention.rs:186`. REQ-2 adds no variant and does not call `observe`.

## Hermes v0.23.0

`print_fn` (`gateway/local_viewer_events.py:40-42`) publishes `agent.print` with `{"text": sep.join(args)}`. `sep` comes from the keyword arguments and defaults to a space. `wire` assigns `agent._print_fn = print_fn` (`local_viewer_events.py:42`).

`gateway/run_turn_runner.py:1351` installs that wire only when `local_viewer_turn(self) and not ctx.mute_notification_reply`. The call is `local_viewer_events.wire(self, agent)` at lines 1353-1354. The `finally` block restores it when `local_sinks is not None` (`run_turn_runner.py:1369-1371`). REQ-3 is this pair of facts. The draft pin file `crates/hermes-contract/PINNED` records tag `v0.22.4` and commit `4d04303818b61bf3aa09278af48356315515965b`. That is the release when this issue starts. REQ-3 names the `git show` check against the commit in that file.

## Recordings and the golden

Only two frame logs contain `agent.print`.

`fixtures/frames/queue-move-send-now.jsonl` line 38 is `agent.print` with text `⚡ Interrupted during API call.` Line 41 is `message.complete` with `status` `interrupted`. The recording is not one of the seven replay roles (`crates/talaria-core/tests/session_replay.rs:20-28`). Test 2 reads it with `frames::read_path` (`crates/talaria-core/tests/support/frames.rs:58`) and `drive::reduce` (`tests/support/drive.rs:11`). The session id has to come from the log. `run` in `session_rules.rs:31` always uses `local-session-1` (`events.rs:16`), which is the wrong id for this file.

`fixtures/frames/subagent-batch.jsonl` line 25 is `agent.print` with text `  🔀 [set 1] delegating 2 tasks`. Line 24 is `tool.start`. The current golden `crates/talaria-core/tests/golden/subagent-batch.cells.txt` is five cells and `final_prefix: 5 of 5`, ending `turn: Finished(Completed)`, `prompts: none`, `unknown_event_types: none`. `describe` (`tests/support/describe.rs:18-44`) numbers cells from 1. A notice is `notice[system] "<text>"` (`describe.rs:230-241`). Test 3 inserts the trimmed text as cell 4 and moves the old cells 4 and 5 down by one. `final_prefix` becomes `6 of 6` because a notice is final. The comparison is `every_recorded_fixture_reduces_to_its_golden_cells` (`session_replay.rs:136`).

`subagent_fixture_adds_no_cells_or_unknown_notice` (`session_replay.rs:405-438`) feeds only `subagent.*` events to the empty-cell assertion, and on the full log rejects a notice whose text contains `subagent.`. The trimmed delegation line does not contain that substring.

`known_but_unmodelled_events_are_silent` (`session_rules.rs:1187`) allows every generated notification name in its first loop, then feeds a shorter list that does not include `agent.print`. Its cell assertion stays empty if that list is left as it is.

`output_event` (`crates/talaria-app/tests/workflow_progress_live.rs:201-213`) matches the name `agent.print`. `wait_until_active` returns on that match (`workflow_progress_live.rs:225`). `steer_stop_and_open_against_gateway` calls it (`workflow_progress_live.rs:589`). The helper does not read cells.

## Why the new tests fail first

`SessionState::notice`, `NoticeKind::System`, `ev`, `ev_session` (`events.rs:71`), and `read_path` already exist. Tests 1 and 2 compile. They fail on the assertion that a system notice was stored, because the empty arm stores nothing. The other-session half of test 1 already passes, because of `reducer.rs:21-22`. Test 3 fails on the golden assertion once the test commit has replaced `subagent-batch.cells.txt` and the arm is still absent.

## Edges, by requirement

| Case | Result | Id |
| --- | --- | --- |
| Non-empty `text` | One `NoticeKind::System` cell, detail `None`, no effect, status unchanged | REQ-1 |
| Leading and trailing white space | `str::trim` only | REQ-1 |
| Empty after trim, or `text` missing, null, or not a string | No cell, and no diagnostic notice | REQ-1 |
| Second event, same or different text | Another cell | REQ-1 |
| Interrupted cell or tool cell already stored | The print is still stored | REQ-1, REQ-4 |
| Other session id, or empty session id | Open session unchanged | existing guard; test 1 |
| Bell, desktop alert, speech, need | None | REQ-2 |
| Pin disagrees with the v0.23.0 publisher | Stop | REQ-3 |

The learning file's newest heading, before this issue, is `## 2026-11-04 — Persistent workspace ownership must outlive temporary overlays` (`docs/engineering-journal/LEARNINGS.md:5`). REQ-4 inserts the new heading above that one. The sentence REQ-4 quotes is the whole check. A search for a single word from it is not the check.
