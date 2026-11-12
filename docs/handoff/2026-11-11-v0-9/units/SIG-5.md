# SIG-5: Answer a recovered prompt, and show the ones that cannot be answered

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: read it for the code that exists today. The requirements, the tests and the acceptance checks are only in the issue.

## What exists today

[#580](https://github.com/infiquetra/talaria/issues/580) (`SIG-4`) adds `PromptKind::ServerRequest`. The card's `prompt_id` is the request id and its `command` is the method. `from_snapshot` inserts one card per non-clarify `open_requests` entry. `PromptStack::sync` (`stack.rs:161`) does not build a card for that kind. `PromptSection::lines` (`stack.rs:473`) titles an `Approval` "Approval needed" and anything else "Question". `draw` in `prompt_cards.rs:226` renders `section()` into a buffer. `sudo_view` (`prompt_cards.rs:137`) is a live masked sudo view, not a `ServerRequest`.

`policy_for` (`prompts.rs:30`) returns `Masked` for `sudo`, `secret`, `vault.code`, and `vault.unlock_prompt`, and `Refuse` for every other name, including `vault.save_login`. `answer_masked` (`prompts.rs:196`) sends `{"value": ...}` on the live `Responder`. `install_recovered` drops that handle, so a recovered card has no `Responder`.

`SubscribedView::from_state` starts at `subscribed.rs:55`. On `main` the match is only `Approval` and `Clarify`. [#580](https://github.com/infiquetra/talaria/issues/580) (`SIG-4`) adds a `ServerRequest` arm and maps every method to a need. `foreground_open` (`app.rs:4638`) counts every open prompt. `AttentionSnapshot::from` (`comforts/mod.rs:26`) matches only `Approval` and `Clarify`. `needs_you_segment` (`segments.rs:520`) prints a count it is given. `next_need` (`fleet_attention.rs:70`) walks background queue items and skips foreground ones. `AttentionTracker::pane` (`attention.rs:284`) returns `PaneState::Blocked` while needs remain. There is no `seat_state`.

`FaultRelay::drop` and `FaultRelay::restore` are `fault_relay.rs:67` and `:81`. `live_reconnect.rs` includes that file with `#[path = "support/fault_relay.rs"]` and marks its tests `#[ignore]`. `request.answer` is in the vendored contract. Its parameters are `id`, `result`, and optional `profile`.

## Names this unit uses

`recovered_limit_seconds` and `talaria_core::session::is_answerable` live in `cell.rs`. The card path, `from_state`, `foreground_open`, and `AttentionSnapshot::from` call `is_answerable`. `policy_for` calls it for `Refuse` and keeps its `MaskedKind` match. `PromptService::answer_recovered(request_id: &str, value: Zeroizing<String>) -> AppEffect` is an associated function with no `self`. It returns `AppEffect::AnswerRecovered { request_id, value }` (`app.rs:2221`). The `UserAction::Masked` arm (`app.rs:6770`) returns that variant when the foreground session has an open `ServerRequest` card with that `prompt_id`, and `AnswerMasked` otherwise. The handler calls `request.answer` in the same shape as `.call::<approval_respond::Marker>` (`prompts.rs:244`), borrowing the value from the `Zeroizing` buffer. The card stays `Open` until success. `Event::ServerRequestAnswered { id }` settles it as `Settlement::Sent` (new, no field). `Elsewhere` is not used for a sent answer. A failed call raises no event. The card stays `Open`, and the notice is Hermes's message or `The answer could not be sent; Hermes may have moved on.` `request.cancel` for an open recovered card raises `Event::ServerRequestCancelled { id }` and settles `Settlement::Elsewhere`. The reason match at `prompts.rs:152` moves into one private function both paths call. The typed value is never stored. The live test polls up to 120 seconds for the sudo request, then runs the drop, restore, resume, and answer with no sleep. `foreground_open` and `AttentionSnapshot::from` each have a test. [#580](https://github.com/infiquetra/talaria/issues/580) (`SIG-4`) adds the `ServerRequest` arm and the `NeedSummary` mapping.

## Hermes facts

Read at Hermes v0.23.1 on 2026-11-11, the release [#544](https://github.com/infiquetra/talaria/issues/544) (`PIN-3`) pins. Re-verify at the commit in `crates/hermes-contract/PINNED` with `git show <commit>:<path>`.

- The limits: `SUDO_TIMEOUT_S = 120` (`gateway/session_sudo_prompt.py`), `SECRET_TIMEOUT_S = 120` (`gateway/session_secret_prompt.py`), and `UNLOCK_TIMEOUT_S = 120`, `SAVE_LOGIN_TIMEOUT_S = 180` and `CODE_TIMEOUT_S = 180` (`gateway/session_vault_prompt.py`). These are the numbers in REQ-1.
- An approval is never in `open_requests`. That list holds the requests sent with `ask_viewer` or `ask_viewer_result` (`gateway/session_server_requests.py`) whose `replay` is true. Their callers under `gateway/` send `sudo`, `secret`, `clarify`, the three vault methods and the desktop interface's own requests. None sends `approval`. An approval reaches a viewer as a prompt and stays a `PromptKind::Approval` card, so no read-only card appears beside it.
