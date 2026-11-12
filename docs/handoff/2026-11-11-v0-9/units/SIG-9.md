# SIG-9: Print a review summary into the conversation

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

The issue states REQ-1 through REQ-3, the one new test, and the stop conditions. This note maps those ids onto the current tree. It does not add a requirement. Line numbers below are from Talaria `main` at the draft, and from Hermes tag v0.23.0, unless a line is named at tag v0.22.4. REQ-3 names the facts its re-verify covers. This issue does not wait on [#544](https://github.com/infiquetra/talaria/issues/544) (`PIN-3`), so the pin at start is Hermes v0.22.4.

## What the reducer does today

`update` (`crates/talaria-core/src/session/reducer.rs:14`) returns immediately when the session id is empty or is not the open session (`reducer.rs:21-22`). `review.summary` is a generated notification (`crates/hermes-contract/src/events.rs:233`). `ReviewSummaryPayload` has one field, `text: String` (`crates/hermes-contract/src/generated/schemas.rs:14641-14643`). The schema comment names the server background-review callback (`schemas.rs:14639`).

A decoded event has `unknown == false`. The empty arm (`reducer.rs:523-532`) matches `_ if !unknown`, so the event stores nothing. It does not reach the diagnostic arm. `ev` (`crates/talaria-core/tests/support/events.rs:61`) calls `EventFrame::from_params`. A payload that call refuses is delivered as an unknown notification whose type name is still `review.summary`, and today that can store the diagnostic notice. REQ-1's arm matches the name before that fall-through.

`SessionState::notice` (`state.rs:362-374`) appends a cell and does not trim. `NoticeKind::System` exists (`cell.rs:157`). A snapshot row whose type is `system` already calls `state.notice(NoticeKind::System, text, None)` (`snapshot.rs:107-109`). REQ-1 does not change that path. The new cell stays in the open session. REQ-1 does not make it survive a reconnect or a resume. `remove_whitespace` (`text.rs:29-30`) deletes every Unicode whitespace character. REQ-1 uses `str::trim` instead.

`AlertKind::Done` is raised from `AttentionTracker::observe` when a turn ends (`attention.rs:186-200`). REQ-2 does not call `observe`.

## Hermes v0.23.0

`gateway/run_turn_runner.py` `_make_bg_review_callbacks`: for a local viewer, `send` waits while `execution_generation` is still the turn's generation, with `deadline = time.monotonic() + 120` (`run_turn_runner.py:427-430`), then `deliver` publishes `{"text": str(message)}` as `review.summary` (`run_turn_runner.py:402`). The comment at `run_turn_runner.py:397-398` says the publish is not generation-fenced, because the review finishes after the claim has settled. The comment at `run_turn_runner.py:424` says a late summary still reaches viewers. REQ-3 is this wait and this publish. Talaria does not implement the wait.

`gateway/session_refine.py:57-60` publishes the same event from `/refine` on a local session. Line 70 says an empty summary draws nothing. The module docstring says messaging sessions keep their own path (`session_refine.py:8`).

`ui-tui/src/app/createGatewayEventHandler.ts:1675-1687` trims `text` and calls `sys(text)` only when the trimmed string is non-empty. The comment says the Python side already formats the message. REQ-1 stores that string. It does not add a prefix.

The draft pin file `crates/hermes-contract/PINNED` records tag `v0.22.4` and commit `4d04303818b61bf3aa09278af48356315515965b`. That is the release when this issue starts. REQ-3 names the `git show` check against the commit in that file. At that commit the same behaviour is present at different lines. `run_turn_runner.py` publishes `{"text": str(message)}` at line 361, and the comment at lines 356-357 says the frame is not generation-fenced. `send` holds on the review thread while `execution_generation` equals the turn's generation, with the deadline at line 386 (`time.monotonic() + 120`), then calls `deliver` at line 390. `session_refine.py` publishes the same payload at line 59, and line 66 says an empty summary draws nothing. `createGatewayEventHandler.ts` trims `text` at line 1570 and calls `sys` only when the trimmed text is non-empty. A line number that differs from the v0.23.0 lines above is not a disagreement. REQ-3's re-verify compares the behaviour it marks.

## Why the new test fails first

`notice`, `NoticeKind::System`, and `ev` already exist. `review_summary_is_a_system_line` compiles. The non-empty assertions fail because the empty arm stores nothing. The all-white-space payload already stores nothing. A payload `from_params` refuses currently stores a diagnostic notice, so the empty-cell assertion on null and missing `text` also fails before the change.

No file under `fixtures/frames/` contains the string `review.summary`. The seven replay goldens do not change. `known_but_unmodelled_events_are_silent` (`session_rules.rs:1187`) does not feed this name. `snapshot_open_prompts_come_from_prompts_not_pending` (`session_snapshot.rs:130`) does not read a review summary.

## Edges, by requirement

| Case | Result | Id |
| --- | --- | --- |
| Non-empty `text` | One `NoticeKind::System` cell, detail `None`, no effect, status unchanged | REQ-1 |
| Leading and trailing white space | `str::trim` only | REQ-1 |
| Empty after trim, or `text` missing, null, or not a string | No cell and no diagnostic notice | REQ-1 |
| Second event, same or different text | Another cell | REQ-1 |
| Other session id, or empty session id | Open session unchanged | existing guard |
| Bell, desktop alert, speech, need, blocked seat | None | REQ-2 |
| A fact REQ-3 marks re-verify differs at the pin | Stop | REQ-3 |
