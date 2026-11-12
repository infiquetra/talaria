# SIG-7: A connection request is shown as a waiting request and can be answered

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the issue says what the card stores, which answer it sends, and which tests prove that. This file is the map of the reducer, the snapshot field, the prompt stack, and the need list. It restates no requirement and no test. It refers to them by id.

## Where the event is dropped today

`connection.request` and `connection.update` are decoded notifications (`crates/hermes-contract/src/events.rs:195-196`). `update` (`crates/talaria-core/src/session/reducer.rs:14`) returns immediately when the session id is empty or is not the open session (`reducer.rs:21-22`). `approval.request` and `clarify.request` go to `prompts::session` (`reducer.rs:515-516`). The known-but-unmodelled arm (`reducer.rs:523-532`) is empty, so both connection names hit it and store nothing.

`SessionState` (`state.rs:104-129`) has no connection field. Its other fields are `pub(crate)` except `queue`. `from_snapshot` (`snapshot.rs:11`) is `pub(crate)`. `SessionState::from_snapshot` (`state.rs:193`) is public and calls it. The prompt loop is `snapshot.rs:116-141`. It does not read `pending_connection`. The module comment says `update` reads no clock (`session/mod.rs:4`). `open_prompts` is `state.rs:300`.

`PromptKind` is only `Approval` and `Clarify` (`cell.rs:176-180`). A connection card is not one of those. `PromptCell` (`cell.rs:204-205`) is `Clone`, `Debug`, and `PartialEq`, with public fields. REQ-1's card follows that. `ConnectionOperationTarget` is `PartialEq` and not `Eq` (`crates/hermes-contract/src/generated/schemas.rs:4283`).

## The generated wire

On the draft tree the pin file is Hermes v0.22.4 and these structs are already generated. This issue starts after `PIN-3`, so the builder's pin is Hermes v0.23.1. REQ-3 is the re-verify.

`CanonicalSnapshot.pending_connection` is `Option<ConnectionRequestPayload>` (`crates/hermes-contract/src/generated/schemas.rs:2693`). The next field, `prompts`, is a `Vec` (`crates/hermes-contract/src/generated/schemas.rs:2694`), so a snapshot sets an array. `ConnectionRequestPayload` (`crates/hermes-contract/src/generated/schemas.rs:4357-4366`) requires `deadline_at`, `op_id`, `seq`, `targets`, and `timeout_seconds`, and optionally carries `display` and `tool_call_id`. There is no `owner` and no `settled` field on the request.

`ConnectionOperationTarget` (`crates/hermes-contract/src/generated/schemas.rs:4284`) has `connect_url: Option<String>` (`crates/hermes-contract/src/generated/schemas.rs:4297`) and `required_env: Option<Vec<ConnectionTargetEnvField>>` (`crates/hermes-contract/src/generated/schemas.rs:4327`). `ConnectionTargetEnvField` (`crates/hermes-contract/src/generated/schemas.rs:4522-4528`) has `default`, `name`, optional `prompt`, `required`, and `secret`. Required target fields used by the tests are `action`, `kind`, `name`, and `state` (`crates/hermes-contract/src/generated/schemas.rs:4285`, `crates/hermes-contract/src/generated/schemas.rs:4316`, `crates/hermes-contract/src/generated/schemas.rs:4319`, `crates/hermes-contract/src/generated/schemas.rs:4340`).

`ConnectionRespondParams` (`crates/hermes-contract/src/generated/schemas.rs:4375-4380`) requires `op_id`, `owner`, and `result`, and optionally `profile`. `ConnectionAnswer` (`crates/hermes-contract/src/generated/schemas.rs:4110-4116`) has optional `said`, optional `settled_by`, and `targets`. `ConnectionAnswerTarget` (`crates/hermes-contract/src/generated/schemas.rs:4176-4184`) has optional `env`, optional `detail`, `name`, and `status`. Status serializes as `approved` or `skipped` (`crates/hermes-contract/src/generated/schemas.rs:4136-4140`). `ConnectionSettleReason` serializes `continue`, `deadline`, `interrupt`, and `all_resolved` (`crates/hermes-contract/src/generated/schemas.rs:4411-4418`). `ConnectionRespondResult` is `settled` and `status` (`crates/hermes-contract/src/generated/schemas.rs:4389-4391`).

`ConnectionUpdatePayload` (`crates/hermes-contract/src/generated/schemas.rs:4667-4688`) includes `settled`, `op_id`, `owner`, `seq`, `deadline_at`, `targets`, and optional `settled_by`.

`Owner` is an untagged `SessionOwner` or `AccountOwner` (`crates/hermes-contract/src/generated/schemas.rs:10940-10942`). `SessionOwner` (`crates/hermes-contract/src/generated/schemas.rs:16528-16531`) has `session_id` and `type_` renamed to `type`. `SessionOwnerType::Session` serializes as `session` (`crates/hermes-contract/src/generated/schemas.rs:16551-16553`). REQ-3 sends `SessionOwner` only.

`connection_respond` is `hermes_contract::methods::legacy::connection_respond` (`crates/hermes-contract/src/generated/methods.rs:333`). Its `NAME` is `connection.respond` (`crates/hermes-contract/src/generated/methods.rs:334`). It is not under `methods::canonical`, where `approval_respond` and `clarify_respond` sit. The profile list already contains that string (`crates/hermes-client/src/remote/profile_methods.rs:31`). The redactor already withholds `env` on that method (`crates/hermes-client/src/redact.rs:47`).

## Hermes behaviour, read at v0.23.0 and v0.23.1

REQ-3 states the re-verify and says the pin at start is Hermes v0.23.1. The files below were read at Hermes v0.23.0 and read again at the Hermes v0.23.1 commit (`3b421578588243f920886e6fe79a6faa87ea026e`). Every line cited "at v0.23.0" below is at the same line at v0.23.1. Tag v0.22.4 is the draft tree's pin, not the pin this issue builds on.

An unsettled `connection.update` can arrive before `connection.request`. At v0.23.1, `drive_operation` runs the connector kind's `prepare` step and only then publishes the request (`tools/connectors/run.py:152-155`). A connector's `prepare` can move targets, for example `tools/connectors/managed.py:297-298`. Each move calls the change hook (`tools/connectors/operation.py:185-189`). The local desktop hook publishes `connection.update` on the owning session with the full snapshot, `settled` false while `settled_at` is unset, and owner type `session` (`gateway/local_desktop_ui.py:139-151`). That snapshot holds `op_id`, `seq`, `deadline_at`, and `targets` (`tools/connectors/operation.py:221-229`), which is everything `ConnectionCard` stores. So REQ-1 lets an unsettled update create the card when none exists. The request built afterwards has a `seq` at least as high, so it either changes nothing or replaces the card's fields. An update for an account-owned operation is a global event with owner type `account` (`tui_gateway/methods_connectors.py:415-416`). It carries no session id, so REQ-1's session check ignores it.

`ConnectionRequestPayload.display` is an optional `ConnectionDisplay`, whose only field is `suggested`, a list of connector slugs (`crates/hermes-contract/src/generated/schemas.rs:4193-4194`). Hermes says a display card answered by words typed in the composer settles like Continue (`tui_gateway/contracts/connectors_operation.py:285` at v0.23.1). The sheet's decisions about what the card draws do not mention `display`. REQ-4 therefore does not store or draw it, and no answer sends `said`. Test 3 passes a `display` object through the snapshot and checks that the stored card holds only the four fields and that no frame carries `said`.

`tools/connectors/operation.py` says every frame carries the seq of the snapshot it was built from, so a renderer that keeps the highest seq per op can drop a frame that arrives after a newer one. The comment matches at both tags (v0.23.0 `operation.py:105-107`). `OPERATION_DEADLINE_SECONDS` is `600.0` at v0.23.0 (`operation.py:19`). At tag v0.22.4 it is `300.0` (`operation.py:18`). That older tag is not the pin when this issue starts. `__post_init__` sets `deadline_at` from that constant when it is unset. `request_payload` copies `op_id`, `seq`, `deadline_at`, `timeout_seconds` equal to that constant, `targets`, and optional `tool_call_id`, at both tags. REQ-4 uses `deadline_at`, not `timeout_seconds`.

`tools/connectors/run.py` allows one open operation per session (`run.py:124-133` at both tags). `_watch` (`run.py:167-177` at both tags) settles `interrupt` when the turn is interrupted and `deadline` when `time.time()` passes `deadline_at`.

`gateway/local_desktop_ui.py` publishes `connection.update` with `settled` taken from `settled_at` and `owner` `{type: session, session_id}` (`local_desktop_ui.py:149-151` at both tags). A local-viewer turn sets `connection_callback` to publish `connection.request` (`local_desktop_ui.py:278` at v0.23.0, `local_desktop_ui.py:276` at v0.22.4).

`gateway/session_connections.py` `pending_connection` returns the live operation's `request_payload()` or `None` (`session_connections.py:30-34` at both tags). `connection.respond` returns `status` `ok` and `settled` (`session_connections.py:87-97` at both tags). The handler drives a session-owned operation (`session_connections.py:21-23` at both tags). Try again is `_retry`, which calls `connectors.connect` when `reconnect` is set. The out-of-scope list forbids that call and forbids offering Try again.

The desktop store `applyOperationStatus` changes nothing when `op_id` differs or `seq` is not higher (`apps/desktop/src/store/connection-request.ts:316-318`). REQ-1 follows that for a non-settled frame. REQ-1 also removes the card on `settled: true` for the same `op_id` even when `seq` is not higher.

## The answer path

`Effect::RespondApproval` is the pattern (`effect.rs:33-38`). REQ-3 adds `RespondConnection` beside it. `PromptService::execute` (`crates/talaria-app/src/prompts.rs:240-272`) matches `RespondApproval` and `RespondClarify`, then a wildcard that returns an empty `Executed`. The new arm has to sit before that wildcard. `Executed` (`prompts.rs:54-58`) is `events` and `notices`. REQ-3 leaves the card up when that call errors and does not tell the stack. REQ-4 marks the card sent for 5 seconds, measured with the `now` passed to `show_connection`. While elapsed time is under 5 seconds, keys send nothing. At 5 seconds the mark has lapsed, and a key sends an intent while the card is shown.

`route` (`app.rs:6184-6204`) sends `RespondApproval` as `AppEffect::Respond`. `sessions.rs:358-368` refuses that group as `not_a_session_call`. `queue.rs:125-138` refuses it as `not_a_queue_call`. `side.rs:69-79` returns an empty event list. REQ-6 adds the new variant to those four groups. `effect_line` (`tests/support/describe.rs:47-139`) is exhaustive. The test commit adds its arm. The developer commit does not edit that file.

`UserAction::Approve` (`app.rs:6758-6768`) reduces `Event::AnswerApproval` and calls `route`. A connection answer is not a reducer event. REQ-4's `UserAction::Connect` arm builds the effect and calls `route` directly.

`prompts_server_requests.rs` builds a `Rig` (`prompts_server_requests.rs:108-126`) with `MockGateway`, `SESSION_ID`, and `PromptService`. `MockGateway::requests` (`tests/support/mock_gateway.rs:1450`) filters seen frames by method. `Seen.params` is the request params (`mock_gateway.rs:349-350`). `push_result` (`mock_gateway.rs:1436`) scripts the next result.

## The prompt stack

`PromptStack::sync` (`crates/talaria-tui/src/prompts/stack.rs:161-220`) rebuilds `cards` from masked views and prompt cells only. `Identity` is `Prompt` or `Masked` (`stack.rs:19-22`). `Content` is the same pair (`stack.rs:24-27`). `Card::session` (`stack.rs:59-64`) matches `Content`. REQ-4 adds a connection identity and keeps it across that rebuild without changing the parameter list. `ready` is 250 milliseconds (`stack.rs:222-224`). `handle_key` ignores keys when the composer is not empty (`stack.rs:228`). Ctrl+C (`stack.rs:244-261`) may attach an intent for a masked or clarify card and always sets `interrupt_requested`. Esc on an approval with no `deny` choice hides the card and sends nothing (`stack.rs:340-347`). REQ-4 must not do that for this card.

`PromptIntent` (`prompts/mod.rs:18-33`) is `Approve`, `Clarify`, and `Masked`. Its `Debug` impl (`mod.rs:35-52`) is exhaustive. `prompt_actions` (`crates/talaria-cli/src/run.rs:1983-2012`) is exhaustive. `card_answer` (`run.rs:707-715`) becomes `fn card_answer(intent: PromptIntent) -> Option<CardAnswer>`. `PromptIntent::Connect` returns `None`. The three existing arms return `Some`. The caller at `run.rs:756-758` pushes `UserAction::FleetAnswer` only for `Some`. `show_connection` is never called for a peek stack, so that `None` cannot be reached and has no test. `configure_prompt_actions` (`run.rs:1967`) matches only `Clarify`, so it does not need an arm.

`TuiScreen::sync_prompts` (`run.rs:1934-1945`) calls `sync` for the foreground session. `unix_now` (`run.rs:841-844`) is Unix seconds. `View.now_ms` (`app.rs:429`) is elapsed application milliseconds, not Unix time. REQ-4 passes `unix_now()`, not `now_ms`. `sync_configure_prompts` (`run.rs:1948`) is the configure stack and is out of scope.

`PromptSection::lines` (`stack.rs:473`) is private and matches `Content`. REQ-4 names public `connection_remaining_seconds` and `connection_lines` on `PromptStack`. Painted rows are read by `draw` (`prompt_cards.rs:226`), which calls public `section()` (`stack.rs:385`) and `render`. Existing calls pass width 80 and 24 rows. There is no existing remaining-seconds helper to copy. The test commit's `intent_kind` (`prompt_cards.rs:166-172`) is exhaustive. `apply_intent` (`tests/support/convert.rs:20-36`) is exhaustive. `live_fleet_view.rs:523-527` is exhaustive. Those three gain arms in the test commit.

## The need list

`NeedKey` (`attention.rs:44-49`) is `Prompt` or `Masked`. `NeedSummary` (`attention.rs:53-71`) has no connection variant. `pane` (`attention.rs:284-288`) is blocked when any need is open. `summary_text` (`attention.rs:325-348`) is exhaustive. `observe` formats `needs you: {message}` (`attention.rs:215-227`). Speech is off in `all_channels` (`tests/attention.rs:15-21`), so the alert text is that sentence. `opened` is `tests/attention.rs:39`.

`AttentionSnapshot::from` (`comforts/mod.rs:26-49`) maps open prompts and masked requests. `needs` is private. `activities_for` (`comforts/mod.rs:59-72`) is public and emits `NeedOpened` and `NeedClosed`. `SubscribedView::from_state` (`fleet/subscribed.rs:68-91`) maps the same two lists onto `NeedSlot` (`fleet/queue.rs:7-15`). An approval need copies `choices`, `execution_generation`, and `timeout_s` from the prompt (`subscribed.rs:81-83`). A masked need uses empty `choices`, `execution_generation: None`, and `timeout_s: None` (`subscribed.rs:89-91`). REQ-5's connection need has none of those prompt fields, and it uses that empty fill.

Exhaustive `NeedSummary` matches the developer commit edits: `summary_text` (`attention.rs:325`), `need_phrase` (`fleet_attention.rs:444-468`), `need_text` (`fleet_view/detail.rs:51-73`), and both matches in `fleet_cmd.rs` (`fleet_cmd.rs:370-392` and `fleet_cmd.rs:612-622`). `slot_json` (`fleet_cmd.rs:601-608`) is the exhaustive `NeedSlot` match. `fleet_view_rows.rs:51-57` uses `NeedDetail::Known(_)`, so it compiles without an arm. `fleet_view_screens.rs:149-165` is exhaustive. The test commit adds the connection arm there.

`app.rs:14074-14076` takes `NeedSlot::Prompt` and otherwise refuses. A connection slot hits that refusal. REQ-5 leaves it. `app.rs:14085-14115` matches `Question` and `Approval` and then `_`, so it compiles without a connection arm.

`AttentionTracker::seat_state` is not in this tree. `SEAT-1` adds it. The ledger names are `talaria_core::SeatState { Idle, Working, Blocked(Option<BlockKind>), Done, Error }` and `talaria_core::BlockKind { Permission, Question, Auth }`. REQ-5 adds the `Connection` arm only.

## The overlay and the fixtures

`connectors_on_message` (`app.rs:3108-3133`) returns when the overlay is closed. When it is open, `connection.update` becomes `ConnectorsInput::ConnectionUpdate` with `op_id` and `owner_is_account`. REQ-6 does not change that function. The session card closes in the reducer.

`crates/hermes-client/tests/fixture_scan.rs` and `crates/hermes-client/tests/fixtures_replay.rs` expect 33 files under `fixtures/frames` (`crates/hermes-client/tests/fixture_scan.rs:36`, `crates/hermes-client/tests/fixtures_replay.rs:118`). REQ tests build JSON in the test.

`keybinding_tables_equal_default_key_map` is `crates/talaria-cli/tests/docs.rs:161`. A new key action with no docs row fails it. This issue adds no action.

## The live scenario

UAT-1 needs a real connector that the model asks to connect. Connectors are the operator's own configuration, so the issue names none. The operator's comment on the issue supplies the connector's name, the prompt, and a line saying it is not yet connected. While the seat is blocked, `hc send` is refused, so the answer uses `hc keys`.

`known_but_unmodelled_events_are_silent` is `session_rules.rs:1187`. It does not feed `connection.request`. `snapshot_open_prompts_come_from_prompts_not_pending` is `session_snapshot.rs:130`. `ev` (`tests/support/events.rs:61`) takes the envelope seq as its first argument. The reducer returns before any connection rule when that seq is present and at or below the cursor (`reducer.rs:39-42`). Payload `seq` is a different field. `run` is `session_rules.rs:31`. `snapshot_json::snapshot` is `snapshot_json.rs:13` and `base` is `snapshot_json.rs:22`. The session id `ev` uses is `local-session-1` (`events.rs:16`).
