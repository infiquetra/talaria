# ACT-2: Regression test: an approval Hermes withdraws removes its card

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the reducer arms that already apply these two events, for the Hermes frames those arms correspond to, and for the gap in the existing test. The requirements, the test steps, and the acceptance checks are only in the issue. This page points at them by id.

## The settled arm

`update` (`crates/talaria-core/src/session/reducer.rs:14`) returns immediately when the event's session id is empty or is not the session under test (`reducer.rs:21-23`). A sequence number that is not greater than the cursor is also ignored (`reducer.rs:39-42`).

The name arm at `reducer.rs:515-516` sends `approval.request`, `clarify.request`, `approval.settled`, and `clarify.settled` to `prompts::session` (`crates/talaria-core/src/session/prompts.rs:78`). After that call, a name other than `tool.complete` returns an empty effect list (`reducer.rs:88-91`).

`prompts::session` builds a key only when the payload has both `prompt_id` and `execution_generation` (`prompts.rs:79-86`). If one of them is missing, a `.request` posts the notice `Hermes asked a question this client cannot answer.` and a `.settled` returns without changing a card. REQ-7 leaves that early return alone.

When the name ends in `.settled` and the cell is `PromptState::Open`, the cell becomes `PromptState::Settled(Settlement::Elsewhere)` and `sealed` is set (`prompts.rs:101-103`). `Settlement::Elsewhere` is the arm for someone else, a sweep, or `already_resolved` (`crates/talaria-core/src/session/cell.rs:189`). An `Answering` cell becomes `Settled(Answered)` instead (`prompts.rs:105-107`). REQ-7 does not cover that arm. The new test's request is never answered, so the cell is `Open` when `approval.settled` arrives.

`SessionState::open_prompts` (`crates/talaria-core/src/session/state.rs:300-307`) yields a prompt cell only when its state is `Open` or `Answering`. A settled cell remains in `cells` and is omitted from that iterator. REQ-2 asserts both: the cell is still there, and `open_prompts` is empty.

## The cancelled arm

The same `session` function has a silent arm for `approval.cancelled` and `request.cancel` (`reducer.rs:523-532`). The arm does not read the payload. The name is not pushed onto `unknown_event_types`, and no notice is posted. REQ-3 locks that silence for a payload whose `request_ids` contain the open prompt's id. REQ-4 forbids removing the name from that arm.

`known_but_unmodelled_events_are_silent` (`crates/talaria-core/tests/session_rules.rs:1187`) already applies `approval.cancelled` with `request_ids` `[]` (`session_rules.rs:1215-1224`) and asserts an empty cell list and an empty effect list. It does not start from an open approval. Leave that test as it is.

## What the neighbour test does not lock

`settled_closes_only_matching_prompt_cancels_change_nothing` (`session_rules.rs:1035`) settles `p1` with `{"prompt_id": "p1", "execution_generation": 1}` (`session_rules.rs:1046-1049`) and then applies `approval.cancelled` whose `request_ids` are `["p9"]` (`session_rules.rs:1051-1060`). The clarify `p2` stays `Open`. That cancelled id is not the approval the test is showing. REQ-3 is the run that uses `p1` while `p1` is still `Open`.

`interrupt_leaves_open_prompts_open` (`session_rules.rs:583`) starts a turn, opens approval `p1`, applies `Event::Interrupt`, and completes the message as interrupted. The prompt stays `Open`. REQ-4 says not to copy that assert into the new test.

`settled_prompt_leaves_with_notice_once` (`crates/talaria-app/tests/prompts_server_requests.rs:709`) already says an app-level `approval.settled` removes the prompt from the open keys. This issue does not edit that file. The verification block runs it so a product edit cannot hide behind the new test.

## How the helpers build the event

`run` (`session_rules.rs:31`) calls `drive::reduce` (`crates/talaria-core/tests/support/drive.rs:11`), which starts from `SessionState::empty` and folds `update`. Each call is a new session. A second event appended after `approval.settled` is not a fresh open card.

`ev` (`crates/talaria-core/tests/support/events.rs:61`) sets the frame session id to `SESSION`, which is `local-session-1` (`events.rs:16`), the replay epoch `epoch-1`, sequence as given, and execution generation 1. The payload is a separate object. `approval_request` (`events.rs:132`) puts `prompt_id`, `execution_generation`, `kind` `approval`, the command `rm -rf .pytest_cache`, a description, and the choices into that payload.

The cancelled payload the neighbour uses, and REQ-3 copies, matches `ApprovalCancelledPayload` in `crates/hermes-contract/vendor/gateway-contract.openrpc.json`: required fields `session_id`, `stored_session_id`, `reason`, `cancelled_count`, and `request_ids`. The reducer never reads them for this name. The test still sends the open prompt id so the lock is not the `p9` case.

## What Hermes publishes

Read at Hermes v0.23.0. `PendingControls.snapshot` publishes `kind + '.settled'` with only `prompt_id` and `execution_generation` when the prompt's generation does not match or the prompt is no longer queued (`gateway/session_pending_controls.py:59-63`). `respond` publishes the same object after it removes the prompt (`gateway/session_pending_controls.py:84-85`).

`_announce_cancelled_gateway_approvals` (`tui_gateway/session_lifecycle.py:563`) is the only function that builds `approval.cancelled`. Its docstring names interrupt, reap, and teardown. At v0.23.0 the callers are `_teardown_session` (`session_lifecycle.py:605`), `_interrupt_session_turn` (`session_lifecycle.py:841-842`, reason `interrupt`, or `ws_orphan_reap` when `orphan` is true), and `_recover_unreachable_compute_host_turn` (`tui_gateway/methods_session_interrupt.py:65`, reason `interrupt`). The broadcast lists that older row's pending approval ids (`session_lifecycle.py:582-590`). `_broadcast_global_event` frames the event with an empty session id (`tui_gateway/server.py:643`, `_event_frame(event, "", payload)`). `gateway/session_authority.py` does not name `approval.cancelled`. The canonical interrupt is not one of these callers. REQ-5 is this split. The test assertions do not change.

The app does not apply a frame whose session id is not the open session (`crates/talaria-app/src/app.rs:8983-8986`). An empty id fails that comparison. The new test never enters `app.rs`. `ev` sets a matching session id so the silent arm is what the assert reaches. That is the case REQ-3 locks. The empty-id drop is the case REQ-5 states and this test does not exercise.

## How the card leaves the screen

The fleet peek collects `frame.state.open_prompts()` and passes that slice to `PromptStack::sync` (`crates/talaria-cli/src/run.rs:3729-3736`, `crates/talaria-tui/src/prompts/stack.rs:161`). REQ-6 says the test stops at `open_prompts`. It does not construct a `PromptStack`.

`prompts` in the test file (`session_rules.rs:75`) returns every prompt cell, including settled ones. `open_prompts` does not. REQ-2 needs both, or a builder can "pass" by deleting the cell.
