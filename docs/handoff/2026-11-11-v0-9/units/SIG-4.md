# SIG-4: Recover waiting sudo, secret and vault prompts after a reconnect

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue says which list entries become cards. This file is the map of the snapshot, the masked-card drop, and the resume call. It restates no requirement and no test. It refers to them by id.

## Where the list is dropped today

`CanonicalSnapshot` (`crates/hermes-contract/src/generated/schemas.rs:2682`) has `prompts` and has no `open_requests`. `OpenRequestEntry` (`schemas.rs:10845`) is `id`, `method`, and `params`. `snapshot_from` (`crates/talaria-app/src/sessions.rs:445`) decodes a `CanonicalSnapshot` and therefore drops a field the struct does not have. `SessionService::recover` (`sessions.rs:404`) calls `session.resume`.

`from_snapshot` (`crates/talaria-core/src/session/snapshot.rs:11`) inserts one `PromptCell` per `snapshot.prompts` entry (`snapshot.rs:116-141`). Kinds are `Approval` and `Clarify` (`cell.rs:176-180`). It does not read `open_requests`.

A dropped socket becomes `LinkEvent::Gap`. The app turns that into `AppEffect::Recover` (`app.rs:9118-9122`), and the executor starts that task at `app.rs:12800`. `LinkEvent::State(Connected)` returns `ConnectionOpened` at `app.rs:8736`. The executor handles it at `app.rs:12772` and calls `PromptService::on_connection_replaced` (`prompts.rs:177`), which drains every masked card and says `lost when the connection dropped`. That drain is earlier than the recover result. `AppEffect` is declared at `app.rs:2008`. [#567](https://github.com/infiquetra/talaria/issues/567) (`TRU-9`) does not move it. `policy_for` (`prompts.rs:30-37`) is `Masked` for `sudo`, `secret`, `vault.code`, and `vault.unlock_prompt`, and `Refuse` for every other name, including `clarify`. `carry_over` (`crates/talaria-core/src/side/mod.rs:139`) copies side work and does not copy prompt cards.

## What Hermes puts on the list

At Hermes v0.23.0, `gateway/session_server_requests.py` `open_requests` walks requests that are still waiting and marked replayable, oldest first, and returns `{id, method, params}`. The docstring names `session.resume` and `session.events.since`. REQ-4 says to re-verify that at the v0.23.1 pin.

## Fleet row

`SubscribedView::from_state` (`crates/talaria-core/src/fleet/subscribed.rs:68`) maps each open prompt to a `NeedSummary`, then extends with masked pairs. `AttentionSnapshot::from` (`crates/talaria-app/src/comforts/mod.rs:26`) does the same match. `NeedSummary` (`crates/talaria-core/src/attention.rs:53`) already has `Sudo`, `Secret`, `VaultCode`, and `VaultUnlock`. REQ-2 adds the `ServerRequest` arm in both. `prompt_lines` (`crates/talaria-tui/src/pager/render.rs:266`) matches `PromptKind`. Its `ServerRequest` arm returns no lines. `describe.rs` (`crates/talaria-core/tests/support/describe.rs:247` and `:280`) matches `PromptKind` too. The test commit adds those arms. `open_prompts` (`state.rs:300`) yields `Open` and `Answering` prompts, so a `ServerRequest` card in `PromptState::Open` is visible to the fleet. `PromptStack::sync` (`crates/talaria-tui/src/prompts/stack.rs:161`) and `answer` (`stack.rs:106`) are the operator card. REQ-2 keeps a `ServerRequest` cell out of that stack and does not build a clarify intent for it. `has_open_prompt` is a field (`fleet_actions.rs:254`, `peek.rs:393`). `needs_answer` is a binding (`run.rs:3132`). REQ-2 also skips `ServerRequest` in `model_card_blocked` (`app.rs:2349`), the peek card flag (`app.rs:14571`), and `switch_notice` (`switch.rs:189`). `foreground_open` (`app.rs:4642`) and the status count (`run.rs:1223`) count every open prompt.

`SessionState::from_snapshot` (`state.rs:193`) is public. `snapshot::from_snapshot` (`snapshot.rs:11`) is `pub(crate)`. The app test uses the public function. `on_connection_replaced` calls `responder.ignore()` (`prompts.rs:187`) before it drops a masked card. REQ-3 does the same.

## Tests already in the tree

`snapshot_json::base` is `crates/talaria-core/tests/support/snapshot_json.rs:22`. `snapshot_json::snapshot` is `snapshot_json.rs:13`. `connection_replaced_drops_masked_cards` is `prompts_server_requests.rs:871` and uses `offer` then `on_connection_replaced`. `sixteen_kinds_match_pinned_contract` is `prompts_server_requests.rs:280`. `refusal_notice_once_per_kind_per_connection` is `prompts_server_requests.rs:545`. `open_requests_teach_the_map` is `crates/hermes-client/tests/redaction_server_requests.rs:263`.
