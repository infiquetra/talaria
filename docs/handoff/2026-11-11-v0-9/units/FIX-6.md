# FIX-6: A partial reply is lost when a credential fails mid-stream

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

The issue states REQ-1 and REQ-2, the one new test, and the stop conditions. This note maps those ids onto the current tree. It does not add a requirement. Line numbers are from Talaria `main` at the draft, and from Hermes tag v0.23.0, unless a line is named at tag v0.22.4. REQ-2 names the facts its re-verify covers. This issue does not wait on `PIN-3`, so the pin at start is Hermes v0.22.4.

## What the reducer does today

`update` passes `raw["payload"]` into `session` (`crates/talaria-core/src/session/reducer.rs:80-83`). `message.complete` calls `complete` (`reducer.rs:462`, defined at `reducer.rs:571`). A finished turn returns immediately (`reducer.rs:573-575`). The interrupt latch seals and returns before the status match (`reducer.rs:576-582`). Status `error` is `TurnOutcome::Failed` with `unknown` false (`reducer.rs:587`). Any other status string is `Failed` with `unknown` true (`reducer.rs:588`).

The failed arm (`reducer.rs:604-619`) builds one error notice and never reads `partial`. The notice text is the fixed unrecognised-status sentence when `unknown` is true. Otherwise it is `nonblank` of `error`, then `text`, then `failure_reason`, then "Hermes could not finish the turn." (`nonblank` is `reducer.rs:754-756`). `error_surface` is copied when it is an object. A `warning` is a second notice (`reducer.rs:622-624`). `seal_all` then runs (`reducer.rs:625`) and the turn becomes `Finished` (`reducer.rs:626`). REQ-1 pushes the new assistant cell before that error notice. The warning notice, when the payload has one, and `seal_all` stay after it. The order check reads `cells()` (`state.rs:198`), because `assistants` and `notices` each drop the other kind.

`message.delta` appends onto the open assistant cell, or creates one (`reducer.rs:377-398`). A payload `text` of JSON null calls `seal_assistant` (`state.rs:376-381`), which sets `sealed` and clears the open index. The cell stays. `begin_turn` (`state.rs:420-427`) sets `turn_start` (`state.rs:109`) to the length of `cells` after sealing, so cells before that index belong to earlier turns. `add_assistant` (`reducer.rs:552-569`) records the open index only when the new cell is not sealed.

`complete_text` (`reducer.rs:637-680`) runs only for `TurnOutcome::Completed`. REQ-1 does not change it. A search of `crates/talaria-core/src/session` finds no use of the payload key `partial`. The word in `session_rules.rs:379` is delta text, not the flag.

`MessageCompletePayload.partial` is `Option<bool>` (`crates/hermes-contract/src/generated/schemas.rs:10085`). The doc comment names `prompt_turn._complete_turn_payload` (`schemas.rs:10073`). `events.rs:205` maps the event name. The tests read cells through `assistants` (`session_rules.rs:35`), `assistant_text` (`session_rules.rs:46`), and `notices` (`session_rules.rs:86`). `run` is `drive::reduce` (`session_rules.rs:31`). `start` is seq 1 (`session_rules.rs:108`). `ev` is `crates/talaria-core/tests/support/events.rs:61`.

## Hermes v0.23.0

`_result_status` (`tui_gateway/prompt_turn.py:313-316`) returns `error` when `result.error` is set and the result is not interrupted. `_turn_outcome` (`prompt_turn.py:328-329`) substitutes the failure account when `final_response` is empty and the result is failed or partial. `_complete_turn_payload` starts the payload as `{"text": raw, "status": status, ...}` (`prompt_turn.py:940`). `has_partial_text` (`prompt_turn.py:967-969`) requires `result.partial`, a `final_response` string that is non-empty after stripping, and that string different from the error string. On status `error` the payload sets `error` (`prompt_turn.py:984`) and, when `has_partial_text`, sets `partial` to true (`prompt_turn.py:988`). The comment at `prompt_turn.py:986` says the desktop client uses the flag to tell retained answer text from error copy. REQ-2 is this flag. Talaria does not compare `text` with `error` itself.

The draft pin file `crates/hermes-contract/PINNED` records tag `v0.22.4` and commit `4d04303818b61bf3aa09278af48356315515965b`. That is the release when this issue starts. At that commit the same behaviour is present at different lines: `_result_status` at line 311, the empty-response substitution at lines 326-327, the payload `text` at line 918, `has_partial_text` at lines 945-947, `error` at line 962, the desktop comment at line 964, and `partial = True` at line 966. A line number that differs from the v0.23.0 lines is not a disagreement. REQ-2's re-verify compares the behaviour it marks.

`_fail_inflight_turn` (`tui_gateway/session_history.py:447-450` at v0.23.0) keeps an in-flight turn replayable after a disconnect. REQ-1 does not implement that path. The issue leaves reconnect out.

## Why the settled rule looks like this

The loss a new assertion can show is a complete that carries the answer only in `text` with `partial` true. `seal_all` already keeps a delta (`state.rs:376-381`), so asserting that a delta survives an error complete passes before any change. Hermes sets the flag only for retained answer text that is not the error string (`prompt_turn.py:967-988`). The existing test `error_status_makes_a_notice_and_never_assistant_text` (`session_rules.rs:376`) forbids copying `text` on an error that has no `partial` field. `duplicate_message_complete_is_ignored` (`session_rules.rs:443`) depends on the finished-turn return.

Sealing a delta clears the open index and leaves the cell. Treating "already open" as that index alone would add a second cell after a null delta. `turn_start` is the bound that still sees the sealed cell. Cells before it are an earlier turn and must not block the new answer.

## Edges, by requirement

| Case | Result | Id |
| --- | --- | --- |
| `error`, `partial` true, non-blank `text`, no assistant cell in this turn | One new assistant cell, sealed, then the error notice, then the warning notice when the payload has one, then `seal_all` | REQ-1 |
| This turn already has an assistant cell, sealed or open | That text stays, no added cell | REQ-1 |
| `partial` missing, false, or not a boolean | `text` is not copied | REQ-1 |
| `text` missing, not a string, or only white space | No added cell | REQ-1 |
| Unrecognised status, or the interrupt latch | Today's path, `partial` unread | REQ-1 |
| Turn already finished | Return before `partial` | REQ-1 |
| A fact REQ-2 marks re-verify differs at the pin | Stop | REQ-2 |
