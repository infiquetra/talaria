# How Hermes signals reach Talaria

This document is the map a builder should use before adding a notice, a prompt, or an approval card. It records which Hermes signals are written onto the socket Talaria opens, which path carries each one, and what the current client does with the bytes.

## 1. Purpose and scope

The facts below were read from source. They were not observed by running a live gateway. Talaria is the tree at commit `ee5c4881` on `main`. Hermes was read at v0.22.4 and at v0.23.0 of the public repository NousResearch/hermes-agent. Where a behavior starts between those two tags, the note says so. One extra check, called out where it matters, also looked at v0.22.7.

The document covers the interactive session connection: discovery, the ticket, the WebSocket, and the events and server requests that connection can receive. It does not cover the Hypertext Transfer Protocol (HTTP) run Application Programming Interface (API), messaging platforms, or the dashboard page.

A few names are used throughout.

- The session authority is the canonical session owner under `gateway/`. Talaria creates and resumes sessions there.
- The older terminal server is the code under `tui_gateway/`. It still accepts the same WebSocket and still owns some methods.
- A viewer is a socket that has attached to one session. Attach happens on canonical `session.create` and `session.resume`, not merely on connecting.
- JavaScript Object Notation Remote Procedure Call (JSON-RPC) is the frame format on the session socket. The control socket uses a different, one-line identify message.

## 2. How Talaria connects

Talaria attaches to a gateway that is already running. If discovery is allowed to start one, and no gateway answers, Talaria runs `hermes gateway ensure`. Passing `--no-ensure` skips that start. The usual path is attach.

### Discovery

Discovery does not speak JSON-RPC. It connects to a Unix socket named `gateway.sock` in the Hermes home (`crates/hermes-client/src/discovery.rs:1016`). The first message is one newline-terminated JSON object: protocol 1, verb `identify`, id 1 (`discovery.rs:1166`). The reply's `api_origin` becomes the address of the session socket (`discovery.rs:153`).

### Ticket and WebSocket

Talaria then mints an interactive ticket scoped to one profile (`crates/talaria-app/src/connection.rs:106`). It opens a Transmission Control Protocol (TCP) connection to the host and port in `api_origin`. An `https` origin is refused (`crates/hermes-client/src/connect.rs:127`). The WebSocket Uniform Resource Locator (URL) is that origin with the `http` prefix rewritten to `ws`, plus `/api/ws` (`connect.rs:44`).

The client offers two subprotocols: `hermes-gateway-v1`, and `hermes-gateway-ticket.` followed by the ticket (`connect.rs:144`). The server must accept `hermes-gateway-v1` (`connect.rs:164`). Hermes redeems an interactive ticket, binds the connection to that profile's session authority, and hands the socket to `tui_gateway.ws.handle_ws` with that subprotocol (`gateway/run_api.py:480` and `gateway/run_api.py:485` at Hermes v0.23.0).

### Ready, capabilities, describe

The handshake reads frames until it sees an event of type `gateway.ready` whose payload has `session_authority` set to true (`connect.rs:511`). Any other ready payload fails the handshake. Hermes writes that flag when the socket has an authority connection (`tui_gateway/ws.py:531` at Hermes v0.22.4; the v0.23.0 handler still sets the flag when the authority connection exists).

Next, JSON-RPC id 1 calls the older method `client.capabilities` with `server_requests: true` (`connect.rs:531`). A timeout is treated as "no capabilities". Any other error fails the handshake.

Then JSON-RPC id 2 calls the canonical method `runtime.describe` with an empty parameter object (`connect.rs:550`). A result that lacks `session_create` fails the handshake (`connect.rs:571`).

That is the only `runtime.describe` on the normal open path. `ConnectionService::open` keeps the describe result from the handshake (`connection.rs:182`). A second call exists only on `open_with_dial`, which asks the link client for `runtime.describe` again after the link starts (`connection.rs:160`).

After `gateway.ready` is written, Hermes registers the socket for broadcasts to every live connection (`tui_gateway/ws.py` registers the transport once the ready frame has been sent; at Hermes v0.22.4 this is `tui_gateway/ws.py:546`).

### What the client calls

Canonical methods go to the session authority. Older methods are served on the same socket only as a fallback: the authority answers "unknown method", the older server still has the method, and the fallback allows it (`tui_gateway/ws.py:388` at Hermes v0.23.0). Session verbs stay on the authority.

Examples the client really calls:

| Method | Kind | Call site |
| --- | --- | --- |
| `runtime.describe` | canonical | `crates/hermes-client/src/connect.rs:550` |
| `session.create` | canonical | `crates/talaria-app/src/sessions.rs:299` |
| `session.resume` | canonical | `crates/talaria-app/src/sessions.rs:270` |
| `prompt.submit` | canonical | `crates/talaria-app/src/sessions.rs:337` |
| `session.interrupt` | canonical | `crates/talaria-app/src/sessions.rs:356` |
| `approval.respond` | canonical | `crates/talaria-app/src/prompts.rs:244` |
| `client.capabilities` | older | `crates/hermes-client/src/connect.rs:531` |
| `slash.exec` | older | `crates/talaria-app/src/commands.rs:245` |
| `command.dispatch` | older | `crates/talaria-app/src/commands.rs:295` |
| `voice.record` | older | `crates/talaria-app/src/voice.rs:198` |
| `process.kill` | older | `crates/talaria-app/src/background.rs:150` |

`session.create` sends `source` `"tui"` unless the caller set another (`crates/talaria-app/src/launch.rs:78`).

## 3. The three delivery paths

Hermes has three writers. Only two of them put bytes on the socket Talaria opened, and they do it for different reasons.

The session event stream is `SessionEvents.publish` (`gateway/session_events.py:89` at Hermes v0.23.0). The frame's session id is the real session id. `fanout.write` delivers it only to sockets that have attached that session (`gateway/session_events.py:101`). Attach stores the WebSocket and calls `fanout.attach` (`gateway/session_authority.py:383` and `gateway/session_authority.py:387` at Hermes v0.23.0). A socket that has connected, but has not created or resumed the session, does not receive these frames.

The older per-session emit is `_emit` (`tui_gateway/server.py:593` at Hermes v0.23.0). It calls `write_json`, which writes to `_sessions[sid]["transport"]` when that older session table has a transport for the id (`tui_gateway/server.py:581`). Otherwise it writes the transport bound to the current call, or standard output (`tui_gateway/server.py:584`). Canonical `session.create` does not insert a row in `_sessions`. A frame built only by `_emit` does not reach Talaria.

The older broadcast is `_broadcast_global_event` (`tui_gateway/server.py:632` at Hermes v0.23.0). It writes one frame to every socket in `_live_transports`, and Talaria's socket is in that set. The frame's session id is the empty string (`tui_gateway/server.py:643`). The payload may still carry a session id. Talaria drops a frame whose session id is empty before the session reducer (`crates/talaria-app/src/app.rs:8947`).

| Path | Who receives it | Session id on the frame | Reaches Talaria? |
| --- | --- | --- | --- |
| Session event stream (`event_stream.publish`) | Sockets attached to that session | The real session id | Yes, after create or resume |
| Older per-session emit (`_emit`) | The older session table's transport, or the call-bound transport | The id `_emit` was given | No, for a session Talaria created |
| Older broadcast (`_broadcast_global_event`) | Every live WebSocket | Empty string | Yes, then Talaria drops it |

An empty session id is not the open session, and it is not a retained session. `link_event` returns without calling the reducer (`crates/talaria-app/src/app.rs:8954`). The voice handler also requires a session id that matches an open or retained session (`crates/talaria-app/src/app.rs:16813`). If an empty id did reach the reducer, `update` would return immediately (`crates/talaria-core/src/session/reducer.rs:21`).

## 4. Signal by signal

### Notices (`notification.show` and `notification.clear`)

A notice is an out-of-band line, not the model's reply. On a local-viewer turn the agent's notice callback is `_local_notice_callback_sync` (`gateway/run_turn_runner.py:482` at Hermes v0.23.0). A local-viewer turn is one whose source platform is the local platform (`gateway/run_turn_progress.py:1022`). Talaria's sessions are local: the authority stores `platform` as local (`gateway/session_local_recovery.py:45` at Hermes v0.23.0).

The callback publishes `notification.show` with the notice's text, level, kind, time to live, key, and id (`gateway/run_turn_progress.py:923`). It publishes nothing when the turn is muted, the status adapter is absent, the run is no longer current, or the text is empty (`gateway/run_turn_progress.py:921` and `gateway/run_turn_progress.py:933`). A clear publishes `notification.clear` with `{"key": ...}` (`gateway/run_turn_progress.py:931`).

Credit keys are diagnostic at every level (`gateway/warning_notifications.py:18`). A diagnostic notice is skipped when warning notifications are suppressed for that platform (`gateway/warning_notifications.py:72`). The other callback, used when the turn is not a local viewer, drops `credits.window_limit` and every key that starts with `credits.window.` (`gateway/run_turn_runner.py:364`). Talaria does not use that callback.

The older terminal server also emits notices with `_emit` (`tui_gateway/agent_callbacks.py:130` at Hermes v0.23.0). That copy does not reach Talaria. The publishers below are the session-stream ones.

| Key | Level | Kind | What triggers it | What must already be true |
| --- | --- | --- | --- | --- |
| `credits.usage` | `info` at 50 percent, `warn` at 75 and 90 | `sticky` | The highest usage band changes | Local-viewer callback, status adapter live, text non-empty, warnings not suppressed |
| `credits.grant_spent` | `info` | `sticky` | Included grant is spent and top-up remains | Same, and the one-shot grant gate |
| `credits.depleted` | `error` | `sticky` | Paid access is gone and the model is not free | Same |
| `credits.restored` | `success` | `ttl`, 8000 milliseconds | Paid access returns after depletion | Same |
| `credits.window_limit` | `error` | `sticky` | A usage window is used up | Same. Not dropped on the local-viewer path |
| `credits.window_payg` | `warn` | `sticky` | The turn is now spending top-up credits | Same |
| `login:` plus eight hex characters | `info` | `sticky` | A sign-in link, code, or result for a local session | Source platform is local and the session is live (`gateway/slash_commands_login.py:45`) |
| `plugin-message:` plus plugin id plus 12 hex characters | `info` | `sticky` | A plugin posts text into a live local session | The local session lookup succeeds (`gateway/plugin_messages.py:101`) |
| `sessions.auto_prune` | `warn` | `sticky` | Low disk and auto-prune is still undecided | Platform is local, policy source is `tui` or `gui`, and a notice is due (`gateway/run_housekeeping_disk.py:87`) |

Credit kind is `sticky` except `credits.restored`, which is `ttl` (`agent/credits_tracker.py:162` at Hermes v0.23.0). The disk payload is `notice_payload` (`hermes_cli/prune_advisor.py:315`). Resume schedules the disk notice off the reply path (`gateway/session_controls.py:712` at Hermes v0.23.0).

Talaria decodes both event names (`crates/hermes-contract/src/events.rs:215`). The session reducer has no arm for them. A known name that is not matched is ignored (`crates/talaria-core/src/session/reducer.rs:523`). Nothing is drawn.

### Approval request and approval settled

`approval.request` is how an attached viewer learns that a command is waiting for a choice. `PendingControls.register` publishes it on the session stream with the real session id (`gateway/session_pending_controls.py:38` at Hermes v0.23.0). The payload includes `kind` `approval`, `prompt_id`, `execution_generation`, the redacted command, the description, and the choices.

`approval.settled` is published in two cases. `respond` publishes `kind + '.settled'` when the viewer answers (`gateway/session_pending_controls.py:84`). `snapshot` publishes the same name when a prompt's generation does not match, or the prompt is no longer in the queue (`gateway/session_pending_controls.py:62`). The settled payload is only `prompt_id` and `execution_generation`. There is no field that says denied, cancelled, or answered.

The reducer matches both names and calls `prompts::session` (`crates/talaria-core/src/session/reducer.rs:515`). A request inserts an open approval card (`crates/talaria-core/src/session/prompts.rs:118`). A settled event seals an open card as settled elsewhere (`prompts.rs:97`).

The client answers with canonical `approval.respond` (`crates/talaria-app/src/prompts.rs:244`).

### Request cancel

`request.cancel` tells every viewer that a server request (sudo, secret, or a clarify batch) is no longer waiting. The canonical waiter publishes it on the session stream (`gateway/session_server_requests.py` at Hermes v0.23.0). The reason is `timeout` when the wait expires, `interrupted` when the turn is interrupted, and `resolved` when a viewer answered. The session id is the real session id. If no viewer is attached, the waiter does not write a frame.

The older server has its own `_emit("request.cancel", ...)` (`tui_gateway/server_requests.py:165` at Hermes v0.23.0). That copy does not reach Talaria.

For a frame that passed the session-id gates, Talaria pushes `CancelRequest` (`crates/talaria-app/src/app.rs:9003`). `on_request_cancel` removes a masked card with that id (`crates/talaria-app/src/prompts.rs:148`). The notice says the card timed out, was withdrawn because the turn was interrupted, or was answered in another window. The reducer then ignores the event name (`reducer.rs:531`). An empty session id never reaches this handler.

### Sudo, secret, and clarify

These are questions the gateway asks the viewer while a tool runs. They are not approval cards.

Sudo calls `ask_viewer` with method `sudo` on the session event stream (`gateway/session_sudo_prompt.py:26` at Hermes v0.23.0). Secret does the same with method `secret` (`gateway/session_secret_prompt.py:43`). A clarify batch calls `ask_viewer_result` with method `clarify` (`gateway/session_clarify_batch.py:29`). All three write a JSON-RPC request, not an event, onto the fanout of viewers attached to that session. The parameters include the real session id.

Clarify also exists as events. `register_clarify` publishes `clarify.request` on the session stream (`gateway/session_pending_controls.py:51`). A settled clarify uses the same `kind + '.settled'` path as approval. Secret and sudo are not published as those events. The pending-controls header says they stay on the server-request path.

Talaria keeps sudo and secret as masked prompts (`crates/talaria-app/src/prompts.rs:96`). Any other server-request method, including `clarify`, is answered with JSON-RPC error `-32601` and one notice that Talaria does not support it (`prompts.rs:116`). The `clarify.request` event is a different frame. The reducer handles it and draws a question card (`reducer.rs:515`, `prompts.rs:118`). `request.cancel` closes the masked sudo or secret card. It does not close a clarify event card.

A withdrawal or a timeout of sudo, secret, or the clarify server request is the `request.cancel` event in the previous subsection.

### The open-requests list

`open_requests` is not an event. It is a field on the canonical replies `session.resume` and `session.events.since`. Each entry is a server request that is still waiting: id, method, and parameters.

At Hermes v0.22.4 the field is absent. Resume returns prompts, pending work, and todo state, and does not include `open_requests`. Events-since returns only the replay dict.

At Hermes v0.23.0 both replies include it (`gateway/session_controls.py:709` and `gateway/session_controls.py:757`). The same field is present at v0.22.7 (`gateway/session_controls.py:703` and `gateway/session_controls.py:751` at that tag). So a gateway older than v0.22.7 does not send the list. v0.22.7 and v0.23.0 do.

Talaria decodes canonical resume into `CanonicalSnapshot`, which has `prompts` and no `open_requests` (`crates/hermes-contract/src/generated/schemas.rs:2682`). The struct does not deny unknown fields, so the extra field is ignored. Canonical events-since uses `CanonicalEventsResult`, which also lacks the field (`schemas.rs:1994`). No code under `crates/talaria-app/src` or `crates/talaria-core/src` reads `open_requests`. Snapshot restore walks `prompts` only.

The older result type `SessionEventsSinceResult` does require `open_requests`. That is not the type the canonical method uses.

### Replay gap

`session.replay_gap` means this socket's event subscription ended because its backlog overflowed. The fan-out builds the frame with the real session id and a payload of `replay_epoch` and `latest_seq` (`gateway/session_events.py:99` at Hermes v0.23.0). The authority then drops that socket's subscription (`gateway/session_authority.py:405`). A later resume attaches again.

This is not the broadcast path. The session id is real, so the frame can pass Talaria's session-id gate. The reducer treats the name as a signal to resume and returns (`crates/talaria-core/src/session/reducer.rs:25`).

### Voice interrupted

`voice.interrupted` is emitted only by the older voice module, `_voice_emit("voice.interrupted")` (`tui_gateway/methods_voice.py:323` at Hermes v0.23.0). `_voice_emit` binds the transport saved when `voice.record` last started, then calls `_emit`. Talaria calls `voice.record` with a session id (`crates/talaria-app/src/voice.rs:198`). That call is an older method, served on this socket by the fallback. The canonical session is not in the older session table, so the frame is written to the bound socket, with that session id.

Talaria handles it when the session id matches an open or retained session (`crates/talaria-app/src/app.rs:16828`). `voice_event` maps the name to an interrupted voice event (`crates/talaria-app/src/voice.rs:329`). Before any `voice.record`, the stored session id is empty and the frame does not match.

### Mixture of Agents progress

Mixture of Agents (MoA) progress uses four event names: `moa.reference`, `moa.progress`, `moa.phase`, and `moa.aggregating` (`gateway/local_viewer_events.py:28` at Hermes v0.23.0).

On a local-viewer turn the progress callback calls `publish_progress`. That function publishes these four names through `publish_execution`, which writes the session stream with the real session id, but only when reasoning display is on, or when the turn is a classic command-line turn (`gateway/local_viewer_events.py:144`). Talaria is not a classic command-line turn. Its policy platform is `tui`, not `cli` (`gateway/session_policy.py:85` and `gateway/session_policy.py:489`). For policy source `tui` or `gui`, reasoning display defaults to true when the session has not set the flag (`gateway/local_viewer_events.py:81`). A session that has turned reasoning display off publishes nothing.

The older server emits the same four names with `_emit`. Those copies do not reach Talaria.

Talaria decodes all four as known notifications (`crates/hermes-contract/src/events.rs:210`). The reducer has no arm for them, so a frame that arrives is ignored (`reducer.rs:523`).

### Tool-output risk

`tool.output_risk` does not reach Talaria.

The agent can report that event name to the progress callback. On an authority turn the callback is `publish_progress`, which returns without a frame unless the name is a sub-agent event, `tool.progress`, `reasoning.available`, or one of the four MoA names (`gateway/local_viewer_events.py:129`). `tool.output_risk` is none of those.

The only builder of the frame is `_progress_output_risk` in the older server, which calls `_emit` (`tui_gateway/tool_progress.py:395` at Hermes v0.23.0). That uses the older session table. Canonical create does not put Talaria's socket there.

Talaria knows the name (`crates/hermes-contract/src/events.rs:258`). If a frame arrived with the open session id, the reducer would ignore it.

### Reclaimed session

`session.reclaimed` says the older terminal server unloaded a session the client did not ask to close. The only emitter is `_announce_session_reclaimed` (`tui_gateway/session_lifecycle.py:549` at Hermes v0.23.0). It runs only for end reasons `idle_timeout`, `lru_evict`, and `ws_orphan_reap` (`tui_gateway/session_lifecycle.py:546`). It broadcasts, so the frame session id is empty. The payload's `session_id` is the older row's runtime id, eight hex characters (`tui_gateway/methods_session.py:65`). The payload's `stored_session_id` is that row's session key, which is the session id argument of the older `session.resume` (`tui_gateway/methods_session.py:1046`).

`_teardown_session` is the only caller (`tui_gateway/session_lifecycle.py:601`). It walks a dict from the older `_sessions` table. Canonical create and resume never insert that dict. The authority's own unload, described in the next section, does not emit `session.reclaimed`.

The event describes a session of the older terminal server. It cannot name a session Talaria holds. The payload `session_id` is an eight-character runtime id. Talaria stores `local-` plus a sha256 (`hermes_state_keys.py:71` at Hermes v0.23.0). Those strings are not the same. The payload `stored_session_id` equals an id Talaria stores only if some other client passed that `local-...` id to the older `session.resume`. Talaria never does that, and without the older row the event is not emitted for the session Talaria attached.

The bytes can still arrive, because broadcast writes every live socket. Talaria then drops the empty session id (`crates/talaria-app/src/app.rs:8954`).

### Cancelled approval

`approval.cancelled` says the older terminal server is dropping gateway approvals for one of its rows. The only emitter is `_announce_cancelled_gateway_approvals` (`tui_gateway/session_lifecycle.py:563` at Hermes v0.23.0), called from the same teardown (line 605). It broadcasts, so the frame session id is empty. `request_ids` are the pending approval request ids queued under that older row's session key.

Talaria's approval card uses a `prompt_id` registered under the canonical route, shaped `namespace:local:dm:local-...` (`gateway/session.py:697` and `gateway/run_turn_runner.py:714` at Hermes v0.23.0). The older teardown lists the queue for the session id the older client passed to `session.resume`, not for that route. A lookup on `local-...` does not see entries stored under the route. The `request_ids` are not the prompt ids Talaria is showing.

The event describes a session of the older terminal server. It cannot name an approval Talaria is showing. The frame is still written to every live socket, and Talaria drops it because the session id is empty (`crates/talaria-app/src/app.rs:8954`). The reducer would also ignore the name (`reducer.rs:530`).

What closes Talaria's card is `approval.settled`, described above, not `approval.cancelled`.

### What the client does today

| Signal | On the socket? | What Talaria does |
| --- | --- | --- |
| `notification.show`, `notification.clear` | Yes, session stream | Ignores the known name |
| `approval.request`, `approval.settled` | Yes, session stream | Draws or seals the card |
| `clarify.request`, `clarify.settled` | Yes, session stream | Draws or seals the question |
| `request.cancel` | Yes, session stream | Closes a masked sudo or secret card |
| sudo, secret | Yes, server request on the fan-out | Masked prompt |
| clarify as a method | Yes, server request on the fan-out | Refuses with `-32601` |
| `open_requests` | On the reply from Hermes v0.22.7 on | Generated types ignore the field |
| `session.replay_gap` | Yes, session stream, on overflow | Marks the session as needing resume |
| `voice.interrupted` | Yes, after `voice.record` on this socket | Voice handler takes it |
| MoA progress | Yes, when reasoning display is on | Ignores the known name |
| `tool.output_risk` | No | Nothing arrives |
| `session.reclaimed`, `approval.cancelled` | Yes, as an empty-id broadcast | Dropped before the reducer |

## 5. What happens when a session Talaria holds goes away

Talaria's session stays in memory while a viewer is attached. Idle unload refuses to delete a session that still has subscribers (`gateway/session_retirement.py:24` at Hermes v0.23.0). The older reaper's idle timeout, least-recently-used eviction, and orphan reap are not this path. Those three names exist only as the reasons for `session.reclaimed`.

| What happened | What the attached viewer is written |
| --- | --- |
| Last viewer detaches with an end reason | The socket is detached from the fan-out first (`gateway/session_authority.py:421`). `close_viewer_session` stores the close and interrupts delegations (`gateway/session_viewer_close.py:79`). It does not publish an event. The leaving socket is written nothing. |
| The socket drops, detach with no end reason | Same detach. At v0.23.0 a plain detach may arm an orphan close (`gateway/session_authority.py:438`). v0.22.4 has no `orphaned` argument. No `session.reclaimed`. |
| Idle unload | `retire_idle` deletes the in-memory session and publishes nothing (`gateway/session_retirement.py:56`). It does not run while this viewer is attached. |
| Gateway restart, or the process exits | No `session.reclaimed`. The module comment says a missing session is the state after a restart (`gateway/session_retirement.py:6`). A turn the restart interrupted can later carry a warning on `message.complete` (`gateway/session_settlement_recovery.py:125`). If the process has exited, the socket is gone. |
| `session.interrupt`, or the agent turn ends | `interrupt` publishes no event (`gateway/session_authority.py:934`). When the turn settles, the viewer gets `message.complete` on the real session id (`gateway/session_settlement_recovery.py:122`). The same completion calls `snapshot` with generation `None`, which publishes `approval.settled` for each still-pending approval (`gateway/session_pending_controls.py:59`). |
| The fan-out backlog overflows | `session.replay_gap` with the real session id (`gateway/session_events.py:99`). The subscription is removed. Resume attaches again. |

`session.interrupt` does not emit `approval.cancelled`. The approval wait treats the interrupt as a withdrawal, not as a choice the user made (`tools/approval_gateway_wait.py:251` at Hermes v0.23.0). The card closes later, when settlement publishes `approval.settled` with `prompt_id` and `execution_generation` and no resolution field.

## 6. What a builder must not assume

1. An event name in the contract does not mean the event reaches Talaria. `tool.output_risk`, `session.reclaimed`, and `approval.cancelled` are real names. The first is never written to this socket. The other two are broadcasts Talaria drops.

2. An empty session id means the older server broadcast the frame to every socket. Talaria will not apply it to the open session. Do not read a session id out of the payload to recover it. The client does not.

3. A frame from `_emit` is not a frame from the session stream. Copying an older terminal handler into a feature will not deliver it to a session Talaria created.

4. `approval.settled` does not say how the prompt ended. The payload is `prompt_id` and `execution_generation`. An interrupt, a timeout, and another window answering can all look the same.

5. `approval.cancelled` is not the interrupt signal. Interrupt settles the canonical card with `approval.settled`.

6. `open_requests` on a canonical reply is absent before Hermes v0.22.7. From v0.22.7 on, the field is on the wire and Talaria's generated snapshot type ignores it. Pending prompts on the snapshot are `prompts`, not `open_requests`.

7. A clarify server request is refused. The question card is the `clarify.request` event. Sudo and secret are server requests, and they are the ones `request.cancel` closes.

8. Credit and usage-window notices are on the session stream for a local viewer, and Talaria ignores `notification.show`. Suppressing warning notifications, or a dead status adapter, means Hermes never sends them. A builder who only watches the older `tui_gateway` callback will miss the copy that actually arrives, and will still see nothing in the client until the reducer grows an arm.

## 7. Not verified

These points were not settled by reading, or were not observed on a live gateway.

- No live gateway was run. A zero count in the recorded frame logs does not prove a signal never arrives. The checked logs have no `notification.show`, `notification.clear`, `session.reclaimed`, `approval.cancelled`, `request.cancel`, `tool.output_risk`, `voice.interrupted`, or `moa.reference` / `moa.progress` / `moa.phase` / `moa.aggregating` line. They do contain `approval.request`, `approval.settled`, and one `clarify.request` (`fixtures/frames/approval.jsonl`, `fixtures/frames/clarification.jsonl`).
- One reading of `ConnectionService::open` treated the handshake `runtime.describe` as a second call. The source does not. The handshake calls it once. Only `open_with_dial` calls it again (`crates/talaria-app/src/connection.rs:160` and `connection.rs:182`).
- At the installed tip, `gateway/plugin_command_choice.py` also calls `ask_viewer_result`. The body of that call was not read. Whether it sends another `clarify` server request, and on which sessions, is not settled.
- Whether a second client, using the older `session.resume` with a `local-...` id, could make `stored_session_id` on `session.reclaimed` equal an id Talaria also holds was read as a possible argument to that old method. It was not executed. Talaria itself never inserts the older row.
- Reasoning display defaults to on for policy source `tui` when the session flag is unset. A live session that has stored the flag as off was not observed. In that case MoA events are not published.
- The exact text of each credit notice depends on account state in the credit tracker. The keys, levels, and kinds above are what the source builds. A live account was not queried.
- `session.replay_gap` is what the fan-out writes on overflow. A live overflow was not captured. The reducer marks the session as needing resume and returns. What the screen shows after that was not traced past that return.

## 8. Three more events that reach Talaria

Traced on 2026-11-11 by reading Hermes source at the tag v0.23.0. None was observed on a live gateway. Talaria decodes all three and, before v0.9.0, ignores them. File paths are in the Hermes repository.

### Connection request (`connection.request`)

- **Who sends it.** A turn on a session a local viewer holds installs a callback that publishes the event on the session's stream: `agent.connection_callback = lambda payload: publish("connection.request", …)` (`gateway/local_desktop_ui.py:278`). The tool is `manage_connections`. A hosted connect or reconnect, and an MCP (Model Context Protocol) install, enable or authorize, start a connection operation whenever that callback is set.
- **It blocks a tool.** The tool thread stays in `_watch` until the operation settles, the turn is interrupted, or the deadline passes (`tools/connectors/run.py:167-176`). The deadline is 600 seconds from the operation's creation (`OPERATION_DEADLINE_SECONDS`, `tools/connectors/operation.py:20`), and the payload carries it as `deadline_at` and `timeout_seconds`. A client that shows nothing leaves the operator looking at a turn that does not move for up to ten minutes.
- **Payload.** `op_id`, `seq`, `deadline_at`, `timeout_seconds` and `targets` are required; `tool_call_id` and `display` are optional (`ConnectionRequestPayload` in the published contract).
- **How a client answers.** `connection.respond` with `owner`, `op_id` and `result`. `result` holds a list of targets, each `approved` or `skipped`, and may carry `settled_by: "continue"`, which settles the operation even when a target is still open.
- **How it ends.** Hermes publishes `connection.update` with `settled: true` when the operation settles, whatever the reason (an answer, the deadline, an interrupt). After that the next snapshot's `pending_connection` is null.
- **After a reconnect.** A resume returns the live operation's request as `pending_connection` in the snapshot. It is not an entry in the open-requests list.
- One operation may be open per session.

### Printed lines (`agent.print`)

- **Who sends it.** The same kind of turn installs a print function that publishes each printed line: `publish("agent.print", {"text": …})` (`gateway/local_viewer_events.py:40-41`). It is installed only for a turn on a session a local viewer holds that is not muting notification replies (`gateway/run_turn_runner.py:1351-1354`).
- **What it carries.** One field, `text`. The contract describes it as sub-agent completion, compaction, error and budget lines. Two recordings in this repository contain it: an interruption line and a delegation line.
- **Nothing waits on it.** A line whose turn has already ended is not delivered.

### Review summary (`review.summary`)

- **Who sends it.** Two publishers reach a session Talaria created: the background review that follows a turn, and `/refine` (`gateway/session_refine.py:57-60`). Both publish on the session's event stream.
- **What it carries.** One field, `text`, already formatted by Hermes as a summary of what the review saved.
- **Nothing waits on it.** Hermes's own terminal client prints the trimmed text as a system line and prints nothing when it is empty.

### Why these reach a session Talaria created

Each of the three is sent only on a "local viewer turn". Every session that `session.create` mints has a local route: the session entry is built with `platform=Platform.LOCAL` whatever `source` label the client sent (`gateway/session_local.py:369-375`). A turn on a local route whose session the gateway's session authority holds is a local viewer turn (`gateway/run_turn_progress.py:1022-1028`, `gateway/run_turn_runner.py:199-205`). The sudo and secret requests use the same condition.

### One event that stays ignored

`billing.step_up.verification` reaches only a client that called the billing step-up method. Talaria never calls it.
