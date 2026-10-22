# u04 — Hermes client-integration map for a new terminal client

**Scope.** This is a map of every surface a third-party terminal client (a rewritten Talaria, "Claude Code for Hermes") can use to drive the installed Hermes Agent backend. It reads the Hermes checkout at `<hermes>`, revision `22063fc099ec9ad8357d7f8f6e6248a0400680d3` (authored 2026-10-17, committed 2026-10-18 08:02 -0700, `git describe`: `v0.22.2+canary.20261018T065527Z-60-g22063fc099`). Every `file:line` below is at that revision unless it says otherwise. Talaria was read at commit `cafc640` (2026-09-25). Today is 2026-10-22.

**Evidence labels.** "Verified" means read in code at the revision above, or observed with a command whose output is quoted. "Hypothesis" or "unverified" means I could not confirm it and it should be checked before anyone relies on it. I did not create sessions or send prompts to the live gateway (that would write to its database and spend model tokens), so every claim about runtime behaviour of turns is code-derived, not live-tested.

---

## 0. The one-paragraph answer

Hermes 0.22 has consolidated on **one long-running gateway process per machine that owns every session for every profile** (the "session authority"), and every first-party client — the Ink terminal UI, the Electron Desktop app, the ACP editor adapter, the classic CLI — is now a *viewer* that attaches to it over a WebSocket speaking the **native `hermes-gateway-v1` protocol**. The old model, where a client spawns its own `tui_gateway` Python process over stdio, is retired: the stdio server now refuses to start when a gateway owns the profile home (`tui_gateway/entry.py:273-295`), and on this machine a launchd-supervised gateway owns 26 profile homes. A new client should therefore build on the native gateway WebSocket, with the shared method contract (`apps/shared/src/gateway-contract.openrpc.json`) as its type source. The contract is good enough to generate most of a typed client, but not all of it: the canonical-only events (approval and clarify prompts, replay gaps) are hand-typed outside the generated file, the main mutation verb (`session.mutate`) has an untyped `operation`/`payload`, no error codes are declared, and the document's `info.version` has stayed `"1"` through 219 commits in five weeks. Sessions do keep running with no client attached (durable admission queue in `state.db`, no interrupt on detach). ACP is a workable but lossy fallback: it has no subagent view, no background-process control, no configuration management, and one profile per process.

---

## 1. Transports a third-party client can use

### 1.1 Overview table

| # | Transport | How to reach it | Auth | Profiles per process | Contract | Recommended? |
|---|---|---|---|---|---|---|
| A | **Native gateway WebSocket, `hermes-gateway-v1`** (the "canonical" transport) | Gateway's own listener: `ws://127.0.0.1:<random port>/api/ws`, port from control-socket `identify` (`api_origin`) or `hermes gateway ensure --json` | One-use 30 s ticket minted on the owner-only Unix control socket `$HERMES_HOME/gateway.sock` (verb `session-ticket`), presented as WebSocket subprotocol `hermes-gateway-ticket.<ticket>` beside `hermes-gateway-v1`; loopback peers only | All profiles the multiplexer serves (26 here); ticket binds one profile, `scope: host` grants all | `x-canonical-methods` (47) + `methods` (257) via legacy fallback; generated | **Yes — primary** |
| B | Dashboard / `hermes serve` WebSocket `/api/ws` (newline JSON-RPC, same frames as stdio) | `hermes dashboard` (default port 9119) or headless `hermes serve`; also mounted lazily on the gateway's own listener | Loopback: legacy `?token=` (in-process session token). Gated (public bind or `dashboard.public_url`): `?ticket=` from `POST /api/auth/ws-ticket`, `?internal=`, or `?token=<user session access token>`; native-app OAuth PKCE at `/auth/native/authorize` → `/auth/native/token` | Host-scoped: may name any served profile (`chat_ws.py:726-728`) | `methods` (257); generated | Only as the **remote** path, and only when the listener is the gateway's own (so the socket gets the session authority) |
| C | stdio JSON-RPC by spawning `python -m tui_gateway.entry` | Spawn the process; newline JSON-RPC on stdin/stdout | None (process ownership) | One (launch home) | `methods` (257) | **No** — refuses beside a live gateway |
| D | HTTP API server (OpenAI-compatible + `/v1/runs`) | Messaging-platform adapter `api_server` inside the gateway; default `http://localhost:8642` | Bearer `API_SERVER_KEY` (≥16 chars, required even on loopback) | `/p/<profile>/…` prefix mirrors under multiplexing | `/v1/capabilities` self-description; no OpenRPC | For headless automation only |
| E | ACP over stdio (`hermes acp`) | Editor spawns `hermes acp`; it attaches to the gateway as a viewer | ACP `authenticate` (terminal setup) + its own gateway ticket | One per process (`hermes -p X acp`) | Agent Client Protocol (external spec) | Fallback / compatibility only |

### 1.2 A — Native gateway WebSocket (`hermes-gateway-v1`)

**What it is.** The gateway process (`hermes gateway run`, supervised by launchd here) binds a loopback HTTP/WS listener at startup: `gateway/run_runtime.py:439` calls `start_gateway_api(runner)` whose defaults are `host="127.0.0.1", port=0` (`gateway/run_api.py:138`). The module docstring calls it "the native session WebSocket (ticketed `/api/ws` — every local CLI/TUI/Desktop session)" (`gateway/run_api.py:1-12`). The protocol name is the constant `CANONICAL_GATEWAY_PROTOCOL = 'hermes-gateway-v1'` (`gateway/session_contract.py:9`).

**How to locate it (verified live).** The control socket answers a one-line JSON `identify` verb (`gateway/control_socket.py:1-7`: "ONE request per connection — one JSON line in, one out"). I sent `{"verb":"identify","id":1,"protocol":1}` to `~/.hermes/gateway.sock` and got (trimmed):

```
{'api_origin': 'http://127.0.0.1:61399', 'authority_epoch': 1,
 'capabilities': ['session-authority-v1', 'durable-admission-v1', 'event-replay-v1'],
 'code_sha': '22063fc099ec9ad8357d7f8f6e6248a0400680d3', 'code_version': '0.22.2',
 'runtime_protocol': 1, 'state': 'ready', 'pid': 90149, 'supervisor': 'launchd'}
served_profiles: 26 parked: 0
```

The machine-readable, credential-free way to find or start it is `hermes gateway ensure --json` (`hermes_cli/gateway_runtime_cli.py:1-40`; exit codes 0 ready, 3 incompatible, 4 authorization, 5 deadline, 6 draining, 7 conflict — `hermes_cli/subcommands/gateway.py:115-124`). Desktop uses exactly this: "Normal local primary and pooled launches invoke `hermes gateway ensure --json`, consume its credential-free endpoint, and authenticate WebSockets with fresh private control tickets" (`apps/desktop/src/AGENTS.md:6-12`). I did not run `ensure` because it may start a gateway.

**Authentication (local).** The client mints a ticket on the control socket with verb `session-ticket` and params `{profile_id, instance_id, purpose: "interactive"}` (optional `scope`, `subject`); the gateway checks the Unix peer's identity, checks `instance_id`, and answers `{"ticket", "expires_in_seconds": 30, "instance_id", "profile_id", "runtime_protocol": 1}` (`gateway/control_socket.py:335-361`). The client then dials `<api_origin>/api/ws` with subprotocols `["hermes-gateway-v1", "hermes-gateway-ticket.<ticket>"]` (`hermes_cli/gateway_client.py:25-33`). The listener accepts that path only from `127.0.0.1`/`::1`, only without an `Origin` header, redeems the ticket, binds the socket to that profile's authority, and hands it to `handle_ws` with the canonical subprotocol (`gateway/run_api.py:330-376`). The reference implementations are 120 lines of Python each: `ui-tui/scripts/gateway_bootstrap.py:44-104` (what Ink runs) and `hermes_cli/gateway_client.py:152-240` (`connect_gateway`, used by ACP and the classic CLI).

**Authentication (remote, another machine).** The ticketed native path is loopback-only by design (`run_api.py:336`). A remote client has three options, all verified only in code:
1. `HERMES_TUI_GATEWAY_URL=ws(s)://…` — `connect_gateway` dials an explicit URL with no ticket subprotocol and "no local fallback" (`hermes_cli/gateway_client.py:199-210`); the credential must ride in the URL and is judged by the dashboard gate (B below). Ink honours the same variable (`ui-tui/src/gatewayClient.ts:56-59`) and, when the listener's `gateway.ready` carries `session_authority: true`, switches itself to the canonical protocol (`gatewayClient.ts:190-196`, `adoptCanonicalAttach`).
2. A gated dashboard listener (non-loopback bind or `dashboard.public_url`) with an auth provider: mint a single-use 30 s WS ticket via `POST /api/auth/ws-ticket` (`hermes_cli/dashboard_auth/routes.py:556-564`), or obtain bearer tokens through the RFC 8252 native-app flow `GET /auth/native/authorize` (PKCE S256, loopback redirect) → `POST /auth/native/token` (`routes.py:289-337`, `routes.py:574-591`, `hermes_cli/dashboard_auth/native_flow.py:1-12`) and pass `?token=` on the upgrade (`hermes_cli/web_server_chat.py:288-330`).
3. Desktop's SSH mode runs a remote `hermes serve --isolated --ssh-session-token-file …` (`hermes_cli/subcommands/dashboard.py:50-58`); details of how Desktop tunnels it were not read (unverified).

Note: the gateway's own listener always binds 127.0.0.1 (`run_api.py:138`, `run_runtime.py:439`), so option 2 against the gateway (rather than a separate `hermes serve`) needs a reverse proxy or tunnel in front of it. Hypothesis: that is what `dashboard.public_url` is for; I did not trace the deployment docs.

**Profiles.** One gateway process serves every profile: `identify` lists 26 `served_profiles` (verified live). A ticket names one profile; a `scope: "host"` grant (the Desktop primary socket and the dashboard gate) may pass `profile` on any call and be routed to that profile's sibling authority (`gateway/session_controls.py:133-160`). Release notes for v0.22.0: "One backend and one gateway for every profile" (`gh api repos/NousResearch/hermes-agent/releases/tags/v0.22.0`, body line 33).

**What the socket serves.** `AuthorityConnection.handlers()` (`gateway/session_controls.py:162-226`) serves the canonical verbs (`session.create/resume/list/info/mutate/detach/events.since`, `prompt.submit/receipt/cancel/replace_queued/steer_queued/resolve_unknown`, `approval.respond`, `clarify.respond`, `runtime.describe`, …) plus several legacy-named verbs it re-implements owner-side (subagent control, processes, rollback, shell, config, model options, background and btw prompts). For any method the authority answers `-32601` for, `tui_gateway/ws.py:350-362` falls back to the legacy handler **if** the connection holds the full interactive grant and the method is not a session/prompt/message verb (except six read-only allowlisted ones) and not one of five process-global verbs (`tui_gateway/ws_legacy_fallback.py:27-67`). In practice the native socket reaches config, MCP, skills, tools, plugins, connectors, cron (`cron.manage`), projects, handoff, vault, voice, attachments and completions through that fallback. It does **not** reach these 16 legacy session verbs (computed from the contract and the handler tables; each has a canonical replacement, mostly `session.mutate` operations or `session.stats`/`session.info`/`session.resume`): `session.activate, session.archive, session.branch, session.branch_stored, session.branch_whole, session.close, session.compress, session.cwd.set, session.delete, session.history, session.set_hidden, session.status, session.title, session.undo, session.usage, session.workspace.move`; nor `agents.list` (process-global, refused).

**Readiness handshake.** The first frame is an `event` `gateway.ready` with `skin`, `change_events: true`, `heartbeat: true`, `replay_epoch`, and `session_authority: true` on the canonical path (`tui_gateway/ws.py:456-462`). Canonical clients then call `runtime.describe`, which returns `instance_id`, `profile_id`, `authority_epoch`, capability strings `durable-admission-v1, event-replay-v1, local-cli-create-v1, acp-editor-policy-v1, acp-session-mcp-v1`, the accepted `session.create` sources and parameters, and the skin (`gateway/session_controls.py:477-487`). Liveness: `gateway.ping` is answered on the read loop (`ws.py`), and the shared TS client pings every 15 s with a 45 s deadline (`apps/shared/src/json-rpc-channel.ts:154-155`).

**Stability.** The method/event shapes are generated from Pydantic models (section 4). The transport has a protocol name (`hermes-gateway-v1`), a control protocol version (`CONTROL_PROTOCOL_VERSION = 1`, `control_socket.py:31`), `runtime_protocol: 1`, and capability strings. There is no published compatibility promise for third parties that I could find; `tui_gateway/AGENTS.md` describes the three first-party consumers (Ink, Desktop, dashboard) only (`tui_gateway/AGENTS.md:3-5`).

### 1.3 B — Dashboard / `hermes serve` WebSocket

`hermes dashboard` and `hermes serve` share `cmd_dashboard → start_server`; `serve` is headless ("what the desktop app and remote backends run", `hermes_cli/subcommands/dashboard.py:1-6`), default `--port 9119 --host 127.0.0.1` (`dashboard.py:18-19`). A second invocation attaches to the existing host backend instead of binding another port ("Exactly ONE backend runs per host and multiplexes every profile", `hermes_cli/main_dashboard.py:873-895`). `/api/ws` is `hermes_cli/web_routers/chat_ws.py:702-735`. Its auth gate (`hermes_cli/web_server_chat.py:200-330`): a native `X-Hermes-Gateway-Ticket` header (local Desktop), else in gated mode `?internal=`, a ticket (query or `hermes-gateway-ticket.` subprotocol), or `?token=` verified as a user session access token; in loopback mode the legacy `?token=` session token.

The important subtlety: whether this socket is canonical depends on which process serves it. When the dashboard app is mounted inside the gateway (`GatewayDashboardMount._bind_state`, `gateway/run_api.py:62-72`) `app.state.session_authority` is set and `/api/ws` builds an `AuthorityConnection` with a host-scoped identity (`chat_ws.py:716-728`). In a separately launched `hermes dashboard` process, `session_authority` is `None` and requests go to the legacy in-process `server.dispatch` (`tui_gateway/ws.py:339-353`). Talaria today attaches to this legacy dashboard path (`talaria/transport/attach.py:5-9`). Whether legacy session verbs in a standalone dashboard still create working sessions beside a live gateway is **unverified**; a standalone dashboard already forwards `session.mutate` to the gateway owner because its own writes lose the ownership lock (`hermes_cli/web_server_sessions.py:189-240`), which suggests the legacy path is no longer a safe place to run sessions.

The dashboard also exposes REST: I counted 295 decorated routes across `hermes_cli/web_routers/*.py` and `dashboard_auth/*.py` (`grep -rhoE "@[a-z_]*router\.(get|post|put|patch|delete|websocket)\(" … | wc -l` → 295). Relevant to a client: `GET /api/config`, `GET /api/config/schema` (field schema + category order), `PUT /api/config`, `GET/PUT/DELETE /api/env`, `POST /api/env/reveal` (`hermes_cli/web_routers/config_env.py:80,99,117,259,328,1067,1085`), `/api/providers/*` (OAuth flows, custom endpoints), `/api/profiles/*` (16 routes), `/api/cron/*`, `/api/mcp/*`, `/api/skills/*`, and the kanban plugin router under `/api/plugins/kanban/` (`plugins/kanban/dashboard`: `/board`, `/tasks`, `/runs/{id}`, …). Local REST calls to the gateway-hosted dashboard authenticate with the `X-Hermes-Gateway-Ticket` header (`hermes_cli/dashboard_auth/native_http.py:1-16`).

### 1.4 C — stdio JSON-RPC (`tui_gateway.entry`)

Newline-delimited JSON-RPC peer-to-peer over stdio, documented in `tui_gateway/AGENTS.md:19-34`. **Retired for products:** "No product surface launches this standalone stdio server any more: Ink and Desktop attach to the profile's gateway over WS … it must be the home's only owner: the gateway's `gateway.lock` reservation refuses it beside a live gateway" (`tui_gateway/entry.py:273-283`); `main()` exits 1 with "refused: … is owned by a running gateway; attach to it instead" (`entry.py:291-295`). Since the live gateway serves all 26 homes on this machine, a client that spawns stdio would be refused for every profile. It remains useful only for isolated test homes (`apps/desktop/e2e/real-session-builder.ts`, per the same docstring).

### 1.5 D — HTTP API server

`gateway/platforms/api_server.py:1-6`: "OpenAI-compatible API server platform adapter (aiohttp). Serves /v1/chat/completions, /v1/responses, /v1/models, /v1/capabilities, /api/sessions, /v1/runs, /api/jobs and /health*". Route table `api_server.py:1645-1692` plus `/v1/runs` family (`api_server_runs.py:241-249`: `POST /v1/runs`, `GET /v1/runs/{id}`, `GET /v1/runs/{id}/events` (SSE with `Last-Event-ID`/`last_seq` resume and a `replay.truncated` marker, `api_server_runs.py:1202-1260`), `POST …/approval`, `…/clarify`, `…/steer`, `…/resolve-unknown`, `…/stop`). `GET /v1/capabilities` self-describes features and endpoints (`api_server.py:2453-2497`), declaring `admin_config_rw: False`, `jobs_admin: False`, `audio_api: False` (`api_server.py:67-76`). Auth is bearer `API_SERVER_KEY`, required even on loopback and rejected if under 16 characters (`api_server.py:4407-4436`). Under multiplexing every route is mirrored at `/p/<profile>/…` (`api_server.py:4472-4478`). On this machine it is **not enabled** (verified: no listener on 8642, `API_SERVER_KEY` absent from `~/.hermes/.env`, `config.yaml` only sets `platforms.api_server.max_concurrent_runs: 10`). It is a good fit for scripted "fire a run and stream it" use and for cross-machine peers (`hermes peer`, `hermes_cli/subcommands/peer.py:286,326`), not for a full interactive client (no subagent tree, no config, no slash catalog).

### 1.6 E — ACP over stdio (`hermes acp`)

`hermes acp` (`hermes_cli/subcommands/acp.py`) starts `acp_adapter/entry.py:main`, which builds `HermesACPAgent()`; with no session manager argument that constructor returns `GatewayACPAgent` (`acp_adapter/server.py:254-258`), "ACP projection of the profile gateway; never constructs an execution owner" (`acp_adapter/gateway_server.py:1`). It dials the gateway with `connect_gateway` (`gateway_server.py:17`), i.e. transport A underneath. See section 5 for features and losses.

---
## 2. Method, event and server-request catalog by capability area

**Source.** Parsed with Python from `apps/shared/src/gateway-contract.openrpc.json` at the base revision (OpenRPC 1.3.2, 1.1 MB): 257 `methods`, 47 `x-canonical-methods`, 16 `x-server-requests`, 79 `x-notifications`, 861 component schemas. The full name lists are saved beside this report as `u04-methods.tsv`, `u04-canonical.tsv`, `u04-srq.tsv`, `u04-notif.tsv`.

**How to read the "Where" column.** `legacy` = declared in `methods` (the `tui_gateway` dispatcher's catalog). `canonical` = declared in `x-canonical-methods` (the native `hermes-gateway-v1` catalog validated by `AuthorityConnection`). A name in both may carry **different** param/result models on each transport (`tui_gateway/AGENTS.md:47-53`; e.g. canonical `session.create` takes `CanonicalCreateParams` and returns `CanonicalSnapshot`). On the native socket a `legacy`-only method is reachable when the authority re-implements it or through the legacy fallback (section 1.2), except the 16 session verbs and `agents.list` listed there.

**Things a "Claude Code for Hermes" client needs that are not obvious from the table:**

- **Session lifecycle on the native socket** is `session.create` / `session.resume` (both return a full `CanonicalSnapshot`: `messages`, `display_messages`, `pending`, `prompts`, `running`, `info`, `revision`, `execution_generation`, `last_sequence`, `replay_epoch`, `subscription_id`) → `prompt.submit` (returns an `AdmissionStatus` receipt: `admission_id`, `sequence`, `status`, `outcome`) → `session.detach`. Branch, compress, rewind (undo), reset, rename, archive, delete, model, reasoning effort, workspace move and import are all `session.mutate {operation, payload, expected_revision, expected_generation, request_id}` (`SessionMutateParams` description in the contract).
- **Per-session model switching**: `session.mutate operation=model` / `operation=reasoning` (`{effort}`), or per-turn `prompt.submit.model_override` (upstream commit `b0bb0d8749` "admit model_override.reasoning on prompt.submit" landed after the base revision). Creation-time policy: `session.create` accepts `model, provider, reasoning, toolsets, skills, run_budget, max_turns, cwd, yolo, checkpoints, safe_mode, service_tier, …` (`CanonicalCreateParams`).
- **Queue / steer / interrupt**: `prompt.submit` on a busy session queues into the durable FIFO; `prompt.steer_queued`, `prompt.replace_queued`, `prompt.cancel` edit the queue; `session.steer` injects text into the next tool result; `session.redirect` redirects the active turn; `session.interrupt` stops it.
- **Approvals and clarify on the native socket are events, not server requests.** The owner publishes `approval.request` / `clarify.request` (with `prompt_id`, `execution_generation`, `choices`, optional `edit` diff and `timeout_s`) and later `approval.settled` / `clarify.settled`; the client answers with `approval.respond` / `clarify.respond` (`gateway/session_pending_controls.py:18-40`). These four event names are **not in the generated contract**; they are hand-typed in `apps/shared/src/gateway-events.ts:33-49` ("Not part of the generated `tui_gateway` contract"). `sudo`, `secret` and the Desktop UI requests remain true server→client requests on the native socket too (`gateway/session_server_requests.py:1-10`).
- **Diffs**: `tool.complete` carries "parsed result, summary, optional diff / todo snapshot"; approval prompts can carry an `edit` proposal; checkpoints are `rollback.list/diff/restore` and canonical `rollback.session_diff`.
- **Subagents** are spawned by the model through the `delegate_task` tool, not by an RPC. The client observes them through `subagent.*` events and polls `subagent.list` / `subagent.tail`, and controls them with `subagent.steer` / `subagent.interrupt` (semantics in `tui_gateway/AGENTS.md:97-125`). `delegate_task`'s per-task fields are `goal, context, output_schema, images, group` (`tools/delegate_tool.py:712` onward) — **there is no per-child model field**; the child model comes from the global `delegation.model` / `delegation.provider` config (`hermes_cli/config_defaults.py:1342-1356`). See section 6.
- **Kanban** has no client RPC beyond the native-owner-only `kanban.run`; the board is reachable over the kanban plugin's dashboard REST router (`/api/plugins/kanban/board`, `/tasks`, `/runs/{id}`, …) and the `hermes kanban` CLI.
- **`.env` management** has no RPC except `reload.env`, `model.save_key`, `model.disconnect`, `mcp.servers.set_api_key`; general `.env` CRUD and the config field schema are dashboard REST (`/api/env`, `/api/config/schema`, section 1.3).

#### Sessions: create, resume, list, branch, compact, rollback, checkpoints

| Method | Where | One-line doc from the contract |
|---|---|---|
| `rollback.diff` | legacy | Diff between a checkpoint and the working tree, with an ANSI rendering sized to the TUI. |
| `rollback.list` | legacy | Checkpoints for the session's cwd; ``enabled: false`` when checkpointing is off. |
| `rollback.restore` | legacy | Restore the working tree (or one file) to a checkpoint by hash or 1-based index. |
| `rollback.session_diff` | canonical | The classic CLI's /diff session: everything changed since the session's earliest retained checkpoint, as data for its terminal renderer. |
| `session.activate` | legacy | Attach the frontend to a live session without closing the previously focused one. |
| `session.active_list` | legacy | Live sessions in this process, insertion order (not a DB browser). |
| `session.archive` | legacy | Set/clear archived (soft-hide, messages kept) on a session + lineage; Desktop PATCH parity. |
| `session.automation` | canonical | The classic CLI's /goal, /subgoal, /loop, /heartbeat manager calls (state writes, the goal judge, quality gates), run by the owner on the session's row. |
| `session.branch` | legacy | Fork a live session into a new stored child that shares the parent's history so far. |
| `session.branch_stored` | legacy | Whole-session branch of a stored parent: the owning backend reads and copies the transcript, which never crosses the wire (a separate method so an older gateway fails loudly, not with an empty branch). |
| `session.branch_whole` | legacy | session.branch of the whole history without echoing the copied transcript back. |
| `session.close` | legacy | Tear down a live session (its stored row stays resumable). |
| `session.compress` | legacy | Manual /compress of an idle session, optionally focused on a topic. |
| `session.context_breakdown` | legacy | Cursor-style split of the context window by category. |
| `session.create` | legacy + canonical | Mint a live session (agent builds after the reply); a DB row appears on the first prompt unless seeded. |
| `session.cwd.set` | legacy | Change a live, idle session's working directory. |
| `session.delete` | legacy | Delete a stored session + transcripts; refused while it is live here. |
| `session.detach` | canonical | Drop this connection's subscription; a stale subscription id answers detached=false. end_reason (snake_case): the viewer is closing the conversation; the last viewer leaving an idle local session ends its row with that reason. |
| `session.foreign.import` | legacy | Import a foreign session into this profile's history (idempotent per origin). |
| `session.foreign.list` | legacy | One page of Claude Code / Codex sessions found on the serving backend. |
| `session.foreign.preview` | legacy | Preview a foreign session's tail before importing it. |
| `session.history` | legacy | The durable display transcript (ancestors included, row ids attached). |
| `session.info` | canonical | Frozen launch policy projection of one local session. |
| `session.list` | legacy + canonical | Human-facing stored sessions, most recent first (sub-agent / kanban sources denied). |
| `session.most_recent` | legacy | Most recent human-facing session; errors fold into a null session_id. |
| `session.mutate` | canonical | Revision-fenced, retry-idempotent session edit (rename, archive, sidebar, branch, delete, rewind, reset, model, reasoning, workspace, compress, import). |
| `session.prune` | canonical | Local operator only: the owner deletes the ended sessions `hermes sessions prune` selected, skipping busy sessions and retained reset owners. |
| `session.resume` | legacy + canonical | Attach to a stored session: reuse it if live here, else lazy / deferred / cold / eager rebuild. |
| `session.save` | legacy | Export the transcript to ~/.hermes/sessions/saved (classic /save). |
| `session.set_hidden` | legacy | Set/clear hidden (out of the default list, still resumable by its owner) on a session + lineage. |
| `session.status` | legacy | Rendered /status text for the session. |
| `session.title` | legacy | Read or set a live session's title; a title set before the row exists is queued. |
| `session.undo` | legacy | Drop the last user turn (and everything after it) from an idle session. |
| `session.workspace.move` | legacy | Re-home a stored session's workspace; git identity is replaced and a live agent follows. |
| `spawn_tree.list` | legacy | Saved spawn-tree snapshots, newest first. |
| `spawn_tree.load` | legacy | Read one saved spawn-tree snapshot (path must be under the spawn-trees root). |
| `spawn_tree.save` | legacy | Persist a finished delegation tree snapshot under the session's spawn-trees dir. |

#### Prompts: submit, queue, steer, interrupt, side questions

| Method | Where | One-line doc from the contract |
|---|---|---|
| `approval.pending` | legacy | Replay the approvals still waiting on this session (reconnect / polling). |
| `approval.received` | legacy | Tell the backend the card is on screen, so its timeout clock starts. |
| `approval.respond` | legacy + canonical | Deliver the user's decision on a dangerous command (falls back to durable identity on a stale sid). |
| `clarify.lock` | legacy + canonical | Lock one answer of a batch clarify request (editable until every question is locked). **Canonical:** Lock one answer of the owner's batch clarify request (editable until every question is locked); only a viewer attached to that session with session:respond may lock. |
| `clarify.respond` | canonical | Generation-fenced answer to a pending clarify prompt (empty answer = skipped). |
| `message.react` | legacy | Set/clear one author's emoji reaction on a message; returns the row's full reaction list. |
| `prompt.background` | legacy | Run a task on a fresh agent in the background; the answer arrives as background.complete. |
| `prompt.btw` | legacy | Side question over a snapshot of the live conversation; the answer arrives as btw.complete. |
| `prompt.cancel` | canonical | Retire a still-queued admission; a started one is unaffected. |
| `prompt.receipt` | canonical | Current receipt of one admission the caller submitted (or controls). |
| `prompt.replace_queued` | canonical | Cancel a still-queued admission and admit the edited prompt as a new admission at the back of the FIFO, in one write bound to the replaced admission. Refusals: admission_started, admission_conflict (the submission_id replaced another admission), queue_full, unsupported_control, not_found. |
| `prompt.resolve_unknown` | canonical | Acknowledge a turn lost across an owner restart; the FIFO behind it resumes. Never requeues the lost input. |
| `prompt.steer_queued` | canonical | Hand a still-queued admission's text to the running turn's agent as a steer. The admission reads 'started' with that turn (pending snapshots mark it 'steering') and never runs on its own; it settles terminal 'steered' when the turn completes with its text, else 'interrupted' (its completion frame says 'cancelled', as an interrupted turn's does); a text the turn never consumed goes back to its queue place. Refusals: admission_started, session_idle, steer_unavailable (no agent to take it: the admission stays queued), unsupported_control, not_found. |
| `prompt.submit` | legacy + canonical | Send a user turn to a live session; busy sessions queue / steer / redirect instead of refusing. |
| `request.answer` | legacy | Answer an open server→client request from a client that never received the frame. |
| `session.interrupt` | legacy + canonical | Stop the running turn (and streaming TTS); retires the crash-recovery marker. |
| `session.redirect` | legacy + canonical | Redirect the active turn (queued for the next turn while the agent is still building). |
| `session.steer` | legacy + canonical | Inject text into the next tool result without interrupting the turn. |

#### Event replay and connection

| Method | Where | One-line doc from the contract |
|---|---|---|
| `client.capabilities` | legacy | What the calling client handles, sent once per connection (after gateway.ready); returns the server→client request methods this backend may send. |
| `command.observe` | canonical | Fire-only: a recognized command the classic CLI or the TUI runs itself; the gateway fires `pre_command` for it in the session's profile after replying (`queued` false: not fired). |
| `gateway.capabilities` | legacy | What THIS build enforces (a client withholds a feature unless advertised). |
| `ping` | legacy | Cheapest liveness probe; answered on the WS reader thread even while every agent is mid-turn. |
| `runtime.describe` | canonical | Owner identity and the canonical capabilities a client may rely on. |
| `session.events.since` | legacy + canonical | Replay events after a seq watermark on WS reconnect; truncated means refetch state. |
| `session.events.stats` | legacy | Replay-buffer occupancy telemetry (ops/debug). |
| `terminal.resize` | legacy | Record the client's column width for server-side rendering. |

#### Goals, loops and heartbeats (session automation)

| Method | Where | One-line doc from the contract |
|---|---|---|
| `session.control` | legacy | Run one allowlisted goal / loop / subgoal / heartbeat action and return the exact resulting snapshot. |
| `session.control.read` | legacy | Stable, allowlisted snapshot of one live session's goal / loop / heartbeat state. |

#### Subagents and delegation

| Method | Where | One-line doc from the contract |
|---|---|---|
| `agents.list` | legacy | Registry-wide background process summary for ``/agents``. |
| `delegation.pause` | legacy | Block/unblock NEW spawns globally (active children keep running); returns the new state. |
| `delegation.status` | legacy | Running subagent tree plus the spawn pause flag and limits. |
| `subagent.interrupt` | legacy | Hard-interrupt one owned child; ``found`` is false when it already finished. |
| `subagent.list` | legacy | Live children owned by this session (other sessions' children never leak). |
| `subagent.steer` | legacy | Queue steering text into a live delegated child owned by this session. |
| `subagent.tail` | legacy | Last 16KB of an owned child's live transcript. |

#### Background processes and shell

| Method | Where | One-line doc from the contract |
|---|---|---|
| `cli.exec` | legacy | Run ``hermes <argv>`` non-interactively and capture its output; ``blocked`` explains a refusal. |
| `process.kill` | legacy | Kill one background process the caller's session owns and return its output snapshot. |
| `process.list` | legacy | Background processes owned by the caller's session (desktop status stack poll). |
| `process.stop` | legacy | ``/stop``: on the shared owner, kill the background processes ``session_id`` owns (required); a standalone sidecar kills every process it holds. Answers the count killed. |
| `shell.exec` | legacy + canonical | Run a safe (non-dangerous) shell command captured for ``!cmd`` / inline substitution. **Canonical:** Ink ``!cmd`` / ``{!cmd}`` on the shared owner: one safe command for a local session, in its frozen launch cwd (``gateway/session_shell.py``). |

#### Models, providers, usage, billing

| Method | Where | One-line doc from the contract |
|---|---|---|
| `billing.auto_reload` | legacy | Enable/disable auto top-up with its threshold and reload amount (billing:manage). |
| `billing.charge` | legacy | Start a one-off top-up charge (billing:manage, idempotent). |
| `billing.charge_status` | legacy | Poll one charge by id. |
| `billing.state` | legacy | Read-only billing view (no scope); the Nous free tier is answered locally without a portal call. |
| `billing.step_up` | legacy | Run the billing:manage device flow; the URL/code arrive via billing.step_up.verification. |
| `free_tier.ack_notice` | legacy | Mark the one-time availability notice as shown on the free-tier identity. |
| `free_tier.challenge_result` | legacy | Report a browser window outcome for the matching pending attempt; mint remains authoritative. |
| `free_tier.claim_nudge` | legacy | Claim the due sign-in offer; true for exactly one caller each time an offer comes due. |
| `free_tier.provision` | legacy | Explicit retry of the free-tier identity mint when the boot bootstrap could not create it. |
| `free_tier.status` | legacy | Pure read of the focused profile's free-tier identity state (no network, no side effects). |
| `insights.get` | legacy | Session/message counts over the last ``days`` for the (optionally scoped) profile store. |
| `llm.oneshot` | legacy | Stateless one-shot LLM completion (titles, ideas) on the session's or the task backend. |
| `model.disconnect` | legacy | Remove every credential (env keys and OAuth state) for a provider. |
| `model.options` | legacy | Provider/model inventory for the picker, layered over the session's live provider when given. |
| `model.save_key` | legacy | Save an API key for a provider and return its refreshed inventory row. |
| `session.stats` | canonical | The session.usage dict on demand: what main's classic CLI read off its agent. |
| `session.usage` | legacy | Token / context / cost counters for the session (+ Nous credit lines when available). |
| `setup.runtime_check` | legacy | Strict provider check through the same runtime resolution the agent uses on session creation. |
| `setup.status` | legacy | Loose provider check: is ANY provider auth state discoverable for the (launch or named) profile. |
| `subscription.change` | legacy | Schedule a downgrade / same-price change or a period-end cancellation. |
| `subscription.preview` | legacy | Chargeless quote of what a plan change would do (billing:manage). |
| `subscription.resume` | legacy | Clear a scheduled downgrade / cancellation (re-enables recurring spend). |
| `subscription.state` | legacy | Current plan, tier catalog and usage for the picker; fail-open when logged out. |
| `subscription.upgrade` | legacy | Prorate, charge and flip the plan (billing:manage, idempotent). |
| `usage.bars` | legacy | Two-bar dollar usage view shared by /usage, /topup and /subscription; fail-open to unavailable. |

#### Profiles

| Method | Where | One-line doc from the contract |
|---|---|---|
| `profiles.configure` | legacy | Editor Save: apply any subset of a profile's sections and report each one. |
| `profiles.create` | legacy | Create a profile (ws twin of POST /api/profiles), mirroring launch credentials by default. |
| `profiles.describe` | legacy | Everything the profile editor shows: soul, model pin, skills, toolsets, MCP servers. |
| `profiles.get_asset` | legacy | A profile asset as a data URL. |
| `profiles.list` | legacy + canonical | Roster of profiles with previews so a client paints without N follow-up calls. |
| `profiles.set_asset` | legacy | Store or clear a profile asset (avatar) atomically. |

#### Configuration, secrets, vault

| Method | Where | One-line doc from the contract |
|---|---|---|
| `config.get` | legacy + canonical | Read one normalised config value (or the whole effective config) the way the UIs render it. |
| `config.set` | legacy + canonical | Change one config key (persisted or session-scoped) and read back the normalised value. |
| `config.show` | legacy | Masked, display-ready config summary (model / agent / environment rows). |
| `onboarding.set_run` | legacy | Write onboarding.run in the root profile's config.yaml; answers the new state. |
| `onboarding.skip` | legacy | Skip setup: end the session's onboarding run and close the card that waits. |
| `onboarding.start` | legacy | Open the onboarding chat in the default profile: create it, write its run record and play the opening. |
| `onboarding.state` | legacy | Whether desktop first-run onboarding is due, and the chat in progress; reads the root profile's config. |
| `reload.env` | legacy | Re-read ~/.hermes/.env (CLI /reload parity); built agents keep their pool until /new. |
| `vault.add` | legacy | Add a login / payment / address item to the local vault. |
| `vault.list` | legacy | Metadata-only listing across the local vault and every unlocked password manager. |
| `vault.lock` | legacy | Forget a manager's session token (every manager when no name is given). |
| `vault.remove` | legacy | Remove a local vault item by id. |
| `vault.source.set` | legacy | Enable or disable an external password manager (disabling also locks it). |
| `vault.sources` | legacy | Status of every login source (local vault + detected password managers). |
| `vault.unlock` | legacy | Unlock a password manager for this session with its master password. |

#### Skills and learning

| Method | Where | One-line doc from the contract |
|---|---|---|
| `learning.delete` | legacy | Archive a skill (restorable via curator) or remove a memory chunk. |
| `learning.detail` | legacy | Node content (SKILL.md or memory chunk) for an edit prefill. |
| `learning.edit` | legacy | Rewrite a node's content (SKILL.md or memory chunk). |
| `learning.frames` | legacy | Pre-render the /journey timeline (frames + legend/summary) so the TUI walks it locally. |
| `skills.manage` | legacy | Skills hub backend: list the profile's skills or search / browse / inspect / install from the hub. |
| `skills.reload` | legacy | Re-scan skill dirs; the pre-rendered ``output`` is what /reload-skills prints. |

#### Tools and toolsets

| Method | Where | One-line doc from the contract |
|---|---|---|
| `tools.configure` | legacy | Persist a toolset / MCP enable-disable change and rebuild the session agent so it takes effect now. |
| `tools.list` | legacy | Every toolset with its resolved tool names, flagged against the session's (or config's) enabled set. |
| `tools.show` | legacy | The /tools listing grouped by toolset, including tools deferred behind the tool_search bridge. |
| `toolsets.list` | legacy | Toolset summaries (no tool names) for the desktop Toolsets tab. |

#### MCP servers

| Method | Where | One-line doc from the contract |
|---|---|---|
| `mcp.catalog` | legacy | Curated MCP presets with per-profile installed/enabled state and the env keys each needs. |
| `mcp.servers.add` | legacy | Add a server to the profile's config from a catalog preset and/or an explicit config. |
| `mcp.servers.list` | legacy | Configured MCP servers for the (scoped) profile, secrets redacted to env-key names. |
| `mcp.servers.oauth.callback` | legacy | Relay a client-captured redirect into a client_redirect_uri flow. |
| `mcp.servers.oauth.cancel` | legacy | Cancel a flow owned by the resolved profile, waking its callback worker. |
| `mcp.servers.oauth.poll` | legacy | Poll a flow; approved persists tokens for the profile and returns the probed tools. |
| `mcp.servers.oauth.start` | legacy | Begin a PKCE OAuth flow; the client opens auth_url and polls mcp.servers.oauth.poll. |
| `mcp.servers.remove` | legacy | Drop a server from the profile's config.yaml. |
| `mcp.servers.set_api_key` | legacy | Store a credential in the profile's .env and reference it from the server config (header or env). |
| `mcp.servers.status` | legacy | Cached runtime state per configured server; never connects, probes, or starts auth. |
| `mcp.servers.test` | legacy | Connect, list tools, disconnect — an OAuth server with no token on disk is reported as not ok. |
| `reload.mcp` | legacy | Tear down and rediscover MCP servers for every live session (prompt cache is invalidated). |

#### Plugins

| Method | Where | One-line doc from the contract |
|---|---|---|
| `plugins.catalog.lookup` | legacy | One curated plugin-catalog entry by name, from the backend's live catalog (cached, with the in-tree fallback and kill list); backs hermes://plugin/install?catalog= deep links. |
| `plugins.list` | legacy | Loaded plugin manager entries (legacy flat view); the Plugins Hub uses plugins.manage list. |
| `plugins.manage` | legacy | Plugins Hub backend: list installed plugins, toggle, git-install, re-pin a catalog install, or remove a user install. |

#### Connectors

| Method | Where | One-line doc from the contract |
|---|---|---|
| `connection.respond` | legacy | Per-target outcomes from the card, and an optional Continue. |
| `connectors.accounts` | legacy | The scoped member's hosted connector accounts, optionally filtered by connector slug. |
| `connectors.accounts.remove` | legacy | Remove one hosted connector account owned by the scoped member. |
| `connectors.accounts.rename` | legacy | Name or rename one hosted connector account owned by the scoped member. |
| `connectors.catalog` | legacy | The hosted connector catalog available to the scoped member. |
| `connectors.connect` | legacy | Start or re-initiate authorization for named connectors on a session or account operation. |
| `connectors.list` | legacy | Connector catalog + connection state for one session or profile account owner. |
| `connectors.operation.status` | legacy | The current snapshot of one open session or account operation. |
| `connectors.operation.wake` | legacy | The browser leg came back (hermes://connections/done): read the accounts now, not at the next tick. |
| `connectors.policy.get` | legacy | Policy layers for the scoped member, from organization to member scope. |
| `connectors.policy.set` | legacy | Apply one scoped member connector or tool-list policy change. |
| `connectors.tools` | legacy | The scoped profile's cached or current tool list for one connector. |

#### Cron and kanban

| Method | Where | One-line doc from the contract |
|---|---|---|
| `cron.cancel` | canonical | Cancel a queued cron admission or latch cancellation on a started one. |
| `cron.manage` | legacy | List/add/remove/pause/resume cron jobs in the (optionally profile-scoped) cron store. |
| `cron.recover` | canonical | Re-observe a firing after a scheduler restart (same request identity). |
| `cron.status` | canonical | Status and, once terminal, the result of one cron admission. |
| `cron.submit` | canonical | Admit one cron firing into the owning profile's durable FIFO. |
| `kanban.run` | canonical | Native-owner only: admit the dispatcher's current claim on a kanban task. |

#### Projects

| Method | Where | One-line doc from the contract |
|---|---|---|
| `project.facts` | legacy | Structured project facts for a cwd so UIs don't re-sniff the workspace. |
| `projects.add_folder` | legacy | Attach a folder to a project (optionally as its primary path). |
| `projects.archive` | legacy | Archive (or with ``restore`` un-archive) a project; answers the full listing. |
| `projects.create` | legacy | Create a project from a name + folders; duplicate primary paths are refused (5063). |
| `projects.delete` | legacy | Delete a project and its folders; answers the full listing. |
| `projects.discover_repos` | legacy | Repos for the desktop overview: scanned-from-disk (cached) ∪ session-derived. |
| `projects.for_cwd` | legacy | Which project (if any) owns a directory, plus the resolved cwd and its git branch. |
| `projects.get` | legacy | One stored project with its folders. |
| `projects.list` | legacy | Every project of the profile (archived included) plus which one is active. |
| `projects.project_sessions` | legacy | Fully hydrated lanes for one project, from the same grouping as projects.tree. |
| `projects.record_repos` | legacy | Persist repo roots found by the client's (desktop-side) scan; return the merged list. |
| `projects.remove_folder` | legacy | Detach a folder from a project. |
| `projects.set_active` | legacy | Switch (or clear) the active project for the profile. |
| `projects.set_primary` | legacy | Make one attached folder the project's primary path. |
| `projects.tree` | legacy | Project → repo → lane overview with counts and a few preview sessions per project. |
| `projects.update` | legacy | Patch a project's display fields; answers the refreshed project. |
| `verification.status` | legacy | Best known verification evidence for a cwd/session; read-only, never runs checks. |

#### Handoff

| Method | Where | One-line doc from the contract |
|---|---|---|
| `handoff.fail` | legacy | Fail a not-yet-claimed handoff (client poll timeout); CAS against the watcher. |
| `handoff.request` | legacy | Queue a handoff to a messaging platform's home channel; the gateway watcher claims it. |
| `handoff.state` | legacy | Poll the handoff row for this session. |

#### Slash commands and completions

| Method | Where | One-line doc from the contract |
|---|---|---|
| `command.dispatch` | legacy | Run a quick/plugin/bundle/skill/built-in slash command and answer a structured directive. |
| `command.resolve` | legacy | Canonical registry command for a name or alias. |
| `commands.catalog` | legacy | Categorized slash metadata (registry, quick, plugin, skill) for completion menus. |
| `complete.path` | legacy | Path / @-reference completions for the composer (files, folders, profiles, plugin providers). |
| `complete.slash` | legacy | Ranked slash-command / skill completions for a ``/`` token. |
| `slash.exec` | legacy | Execute a slash command against the session's slash worker (or a live/plugin shortcut). |

#### Attachments

| Method | Where | One-line doc from the contract |
|---|---|---|
| `clipboard.paste` | legacy | Save the host clipboard image into the session and queue it for the next turn. |
| `file.attach` | legacy | Stage a non-image file into the session workspace and hand back its @file: ref. |
| `image.attach` | legacy | Queue a gateway-visible image file for the next turn. |
| `image.attach_bytes` | legacy + canonical | Queue an image uploaded as base64 (remote client); reply mirrors image.attach. |
| `image.detach` | legacy | Drop a queued image before the turn is sent. |
| `image.generate` | legacy | Generate an image through the tool's provider dispatcher and hand the renderer a data URL. |
| `input.detect_drop` | legacy | Recognise a terminal file drop pasted into the composer and turn it into an attachment. |
| `paste.collapse` | legacy | Spill a large paste to a file and hand back the inline placeholder. |
| `pdf.attach` | legacy | Render a PDF's pages to PNG and queue them as images for the next turn. |

#### Voice and wake word

| Method | Where | One-line doc from the contract |
|---|---|---|
| `voice.record` | legacy | VAD-bounded push-to-talk; the transcript arrives as a voice.transcript event. |
| `voice.toggle` | legacy | /voice parity: report, flip voice mode on/off, or toggle speech output. |
| `voice.tts` | legacy | Speak text through the backend TTS engine (barge-in aware). |
| `wake.feed` | legacy | Push client-captured PCM into the armed detector (mic-less remote backends). |
| `wake.pause` | legacy | Release the mic (e.g. while the desktop's browser captures audio). |
| `wake.resume` | legacy | Reclaim the mic after a pause; no-op if the listener isn't armed. |
| `wake.start` | legacy | Arm the wake-word listener for the calling surface; refusals explain why. |
| `wake.status` | legacy | Everything a client needs to draw the wake-word state and decide whether to (re)arm. |
| `wake.stop` | legacy | Stop this surface's listener; persist also writes wake_word.enabled: false. |

#### Everything else (Desktop-specific, groups, bot relay, pets, display, metrics, browser, i18n)

`a2a.forward`, `bot_relay.deliver`, `bot_relay.outbox.drain`, `bot_relay.reply`, `bot_relay.roster.sync`, `bot_relay.settle`, `browser.controller.detach`, `browser.controller.heartbeat`, `browser.controller.register`, `browser.controller.result`, `browser.manage`, `diagnostics.share_nous`, `display.install`, `display.lease.acquire`, `display.lease.release`, `display.observe`, `display.start`, `display.status`, `display.stop`, `display.switchSandboxImage`, `display.thumbnail`, `groups.approve`, `groups.attachment.download`, `groups.attachment.upload`, `groups.capabilities`, `groups.create`, `groups.demote`, `groups.disband`, `groups.discard`, `groups.list`, `groups.log`, `groups.peer.invite`, `groups.peer.register`, `groups.peer.revoke`, `groups.promote`, `groups.rename`, `groups.replica_state`, `groups.replicate`, `groups.retry`, `groups.send`, `groups.state`, `groups.stop`, `i18n.catalog`, `i18n.languages`, `machine.facts`, `pet.cancel`, `pet.cells`, `pet.disable`, `pet.export`, `pet.gallery`, `pet.generate`, `pet.generate.status`, `pet.hatch`, `pet.info`, `pet.info.meta`, `pet.remove`, `pet.rename`, `pet.scale`, `pet.select`, `pet.thumb`, `preview.restart`, `shared_metrics.desktop_daily`, `shared_metrics.desktop_dislike`, `shared_metrics.desktop_feature_use`, `shared_metrics.desktop_friction`, `shared_metrics.desktop_onboarding`, `shared_metrics.set`, `shared_metrics.slash_command`, `shared_metrics.startup_latency`, `shared_metrics.status`, `shared_metrics.update_run`, `system.battery`, `worker.adopt`, `worker.persist`, `worker.register`

#### Server-to-client requests (`x-server-requests`, all 16)

| Method | Where | One-line doc from the contract |
|---|---|---|
| `approval` | request | A dangerous command awaits the user's decision. |
| `clarify` | request | The clarify tool: ask the user 1-5 questions. |
| `display.install.sudo` | request | Masked sudo password for the Bot Screen package install; app-level (empty session). |
| `preview.act` | request | Click / type / scroll / annotate inside the in-app browser preview. |
| `preview.read` | request | Read the in-app browser preview's text (JSON text answer). |
| `secret` | request | Masked value for a named env var (skills / setup flows). |
| `setup_beat` | request | The onboarding chat's card for one beat; blocks setup_beat until answered (300 s). |
| `sudo` | request | Masked sudo password for the terminal tool. |
| `terminal.read` | request | Read the visible in-app terminal buffer (JSON text answer). |
| `theme.apply` | request | Apply set_theme values in the window showing the session (JSON text answer). |
| `theme.pick` | request | Show an accent or layout picker card in the chat and wait for the pick. |
| `tour` | request | Drive a guided tour highlight in the desktop renderer. |
| `vault.code` | request | A one-time / 2FA code the user reads from their device. |
| `vault.save_login` | request | Save a login for a site the agent is about to fill; the answer is JSON {identifier, password}. |
| `vault.unlock_prompt` | request | Master password to unlock an external password manager for this session. |
| `window.read` | request | Enumerate the native window below the app (JSON text answer). |

#### Notifications (`x-notifications`, 79 at the base revision)

**Streaming: message, reasoning, tools, todo**

| Method | Where | One-line doc from the contract |
|---|---|---|
| `message.complete` | event | The turn ended: final text, usage and outcome. |
| `message.delta` | event | One streamed chunk of the assistant reply (null text: segment boundary). |
| `message.interim` | event | Interim assistant commentary (text beside tool calls) sealed as its own segment. |
| `message.reaction` | event | The agent reacted to a message; paint it live. |
| `message.start` | event | A turn began streaming; an automation turn carries its async-result card. |
| `moa.aggregating` | event | The MoA aggregator started. |
| `moa.phase` | event | MoA phase transition (currently only ``aggregator``). |
| `moa.progress` | event | MoA reference fan-out progress (n/total). |
| `moa.reference` | event | One MoA reference model's output. |
| `reasoning.available` | event | A completed reasoning block (non-streaming providers). |
| `reasoning.delta` | event | One streamed chunk of the model's reasoning. |
| `thinking.delta` | event | Legacy thinking-text chunk (thinking_callback). |
| `todo.updated` | event | Full todo snapshot after a todo tool ran. |
| `tool.complete` | event | A tool call finished: parsed result, summary, optional diff / todo snapshot. |
| `tool.generating` | event | The model is emitting a tool call's arguments. |
| `tool.output_risk` | event | Tool output was classified as risky (prompt-injection / secret findings). |
| `tool.start` | event | A tool call began (stable id + full args). |

**Subagents**

| Method | Where | One-line doc from the contract |
|---|---|---|
| `subagent.complete` | event | A child finished (status + observability rollup). |
| `subagent.progress` | event | Batched tool-name progress from a child. |
| `subagent.spawn_requested` | event | delegate_task accepted a child goal (before the child starts). |
| `subagent.start` | event | A delegated child started running. |
| `subagent.thinking` | event | A child's reasoning chunk. |
| `subagent.tool` | event | A child called a tool. |

**Session state and status**

| Method | Where | One-line doc from the contract |
|---|---|---|
| `approval.cancelled` | event | Pending gateway approvals were dropped by interrupt/reap/teardown; the wait resolved as deny (not a user refusal). |
| `background.complete` | event | A /background side agent finished. |
| `btw.complete` | event | A /btw side question was answered. |
| `error` | event | A session-level failure outside a turn (agent init, model switch, compression, resume). |
| `gateway.ready` | event | First frame of a connection: the resolved skin, the change-event capability and the replay epoch. |
| `notice` | event | Informational one-liner for the session (capabilities refreshed). |
| `request.cancel` | event | The backend withdrew an open server→client request; clear the matching card only. |
| `review.summary` | event | Background review of the last turn finished. |
| `session.control.update` | event | Persisted goal / loop / heartbeat state changed. |
| `session.info` | event | Live session settings snapshot (``server._session_info``); also the ``info`` of create/resume/activate. |
| `session.reclaimed` | event | The backend reclaimed a live session out from under its clients. |
| `session.resume_progress` | event | Deferred resume hydration progress. |
| `session.title` | event | Auto-titling renamed the session (``session_id`` is the stored key). |
| `session.usage` | event | Mid-turn usage tick; message.complete carries the authoritative final usage. |
| `status.update` | event | Transient status line (kind: status, lifecycle, compacting, goal, loop, heartbeat, process, …). |

**Background processes**

| Method | Where | One-line doc from the contract |
|---|---|---|
| `agent.print` | event | A status line the agent printed during a local viewer's turn. |
| `agent.terminal.output` | event | Output chunk from an agent-owned background process. |
| `terminal.close` | event | An agent-owned background process closed. |

**Voice**

| Method | Where | One-line doc from the contract |
|---|---|---|
| `voice.interrupted` | event | Barge-in: the spoken interjection interrupted the turn; no payload. |
| `voice.partial` | event | Live STT text so far while the user is still speaking. |
| `voice.status` | event | Voice recorder state changed. |
| `voice.transcript` | event | A voice capture produced text (or a stop phrase / silence limit). |
| `wake.detected` | event | A wake phrase fired. |

**Change signals (refetch triggers)**

| Method | Where | One-line doc from the contract |
|---|---|---|
| `bot_relay.outbox.pending` | event | A bot-relay outbox envelope is queued; drain it. |
| `cron.changed` | event | cron/jobs.json moved; refetch the cron list. |
| `pairing.changed` | event | Pairing state moved; refetch pairing. |
| `pet.changed` | event | The active pet / its spritesheet changed (watcher). |
| `platforms.changed` | event | gateway_state.json moved; refetch platform status. |
| `projects.changed` | event | projects.db moved; refetch the project list + tree. |
| `sessions.changed` | event | state.db moved; refetch the session list. |
| `skills.changed` | event | A skill root moved (installed/removed elsewhere); refetch commands.catalog from the gateway. |
| `skin.changed` | event | The active skin moved (name switch or live colour edit); repaint from this palette. |

**Connections**

| Method | Where | One-line doc from the contract |
|---|---|---|
| `connection.request` | event | A connection operation opened on this session; the desktop renders its card. |
| `connection.update` | event | One transition or the settlement of an open connection operation. |

**Other notifications:** `billing.step_up.verification`, `browser.controller.cancel`, `browser.controller.command`, `browser.progress`, `display.install.done`, `display.install.log`, `display.lease`, `display.status`, `free_tier.challenge`, `notification.clear`, `notification.show`, `onboarding.ended`, `pane.reveal`, `pet.generate.progress`, `pet.hatch.progress`, `preview.close`, `preview.open`, `preview.restart.complete`, `preview.restart.progress`, `reaction`, `setup.ready`, `tip.show`

---

## 3. Event replay, reconnect, and whether sessions keep running with no client

### 3.1 How replay works

- **Sequencing and ring.** Every event frame gets a per-session monotonic `seq` and lands in a bounded in-memory ring: 512 events and 4 MiB per session, 64 sessions, 64 MiB per process, oldest evicted first; evictions leave a truncation watermark "so a reconnecting client refetches instead of trusting a replay with holes" (`tui_gateway/event_replay.py:1-35`). A process-wide `_REPLAY_EPOCH` lets a client detect a backend restart (`event_replay.py:21-24`). The canonical transport reuses the same ring through `SessionEvents`, adds a per-session `replay_epoch`, and stamps `authority_epoch` / `execution_generation` on frames (`gateway/session_events.py:11-60`).
- **Reconnect call.** `session.events.since {session_id, last_seen|last_sequence, replay_epoch?}` returns `{events, latest_seq, last_sequence, truncated, snapshot_required, replay_epoch, epoch, count}` (`CanonicalEventsResult` in the contract; handler `gateway/session_controls.py:666-672`). If `snapshot_required` is true (epoch changed, ring truncated, cursor ahead) the client must call `session.resume` again for a full snapshot; the shared TS client turns that into a synthetic `session.replay_gap` event (`apps/shared/src/json-rpc-gateway.ts:570-668`). The canonical owner also emits `session.replay_gap` itself when a slow peer's fan-out backlog overflows (`gateway-events.ts:46-49`).
- **Still-open questions after reconnect.** Legacy transport: unanswered server→client requests come back as `open_requests` on `session.resume` / `session.activate` / `session.events.since` (`tui_gateway/server_requests.py:15-16`, `tui_gateway/methods_session.py:2338-2341`). Canonical transport: pending approvals and clarifies are part of the resume snapshot (`pending`, `prompts`) and of the replayable event stream (`approval.request` is published into the ring, `session_pending_controls.py:38`), plus `approval.pending` for polling. **Hypothesis, not live-tested:** canonical `sudo`/`secret` requests are not re-delivered to a viewer that attaches after they were sent — `_ask` writes the frame once to the current fan-out (`gateway/session_server_requests.py:141`) rather than publishing it into the ring, and returns `None` at once when no viewer is attached (`session_server_requests.py:135-136`), so a sudo prompt with nobody watching resolves as "no answer" immediately.
- **Durable transcript.** The ring is an optimisation; the authoritative transcript and admission queue live in each profile's `state.db`, which `session.resume` reads.

### 3.2 Does a session keep running with no client attached?

**Yes on the native gateway — verified in code, not live-tested.**

1. The owner is a separate supervised process. On this machine it is launchd-supervised and has been up 3 days 12 hours (`ps -o etime -p 90149` → `03-12:53:05`; `identify` → `supervisor: launchd`). Clients are viewers: "Closing, switching or evicting a Desktop connection disconnects the viewer; only explicit gateway lifecycle commands stop the owner" (`apps/desktop/src/AGENTS.md:10-12`); `hermes_cli/gateway_client.py:1`: "Closing a viewer never stops its authority."
2. Disconnect only detaches. `AuthorityConnection.close()` detaches each subscription (`session_controls.py:784-799`); `SessionAuthority.detach` removes the subscriber and fan-out, releases editor MCP servers when the last viewer leaves, and ends a row only if the viewer passed `end_reason` or the session is an ACP session — and even then only with an idle FIFO: "a busy FIFO keeps the row open" (`gateway/session_authority.py:298-320`, `gateway/session_acp_lifecycle.py:16-46`). Nothing in that path interrupts a running turn.
3. Work is admitted durably. `prompt.submit` returns an admission receipt from a durable FIFO (`session_authority.py:322-338`; capability `durable-admission-v1`); a turn lost across an owner restart is surfaced through `prompt.resolve_unknown` rather than silently requeued (contract doc for that method).
4. Approvals without a viewer wait `approvals.timeout`, default 300 s (`tools/approval_context.py:304-310`), then deny; clarify likewise times out. So an unattended agent that hits a dangerous command stalls up to five minutes and is then refused — an "always-running agent" client should surface pending approvals as notifications (or the user configures approval policy).
5. Caveat — auto-started gateways exit when idle. A gateway that a client starts itself (Ink: `ensure_gateway_runtime(home, idle_exit=True)`, `ui-tui/scripts/gateway_bootstrap.py:76-77`) runs with `--idle-exit` and stops after `gateway.unmanaged_idle_exit_seconds` (default 600) **only** when there is no client, no messaging adapter, no due cron/kanban work, and no queued/started admission, background process, async delegation, heartbeat/loop or handoff (`gateway/run_idle_exit.py:1-20`). Supervised gateways and `hermes gateway ensure` (Desktop) never arm this. A new client should start the owner with `hermes gateway ensure` or a service install, not with idle-exit.
6. Contrast — legacy transports do not give this. On the legacy WebSocket path, disconnect runs `_close_sessions_for_transport`, which reaps sessions the transport owned or detaches them to a grace-windowed orphan reaper (`tui_gateway/ws.py:551-557`), and stdio dies with its parent.

---

## 4. Is the OpenRPC document good enough to generate a typed client?

**Mostly yes for request/response, no for events and versioning.**

What is solid (verified):
- **Single source of truth.** Every method has a Pydantic `Params` and `Result`, every server request a `Params` + `Result`, every event a `Payload`; params are `extra="forbid"` and `dict[str, Any]` is banned in contracts (`tui_gateway/contracts/base.py:1-20`). `scripts/gen_gateway_contracts.py:296-356` renders both the TypeScript file and the OpenRPC document; `tests/tui_gateway/contracts/test_generated.py:29-33` fails CI when either is stale, and `test_catalog_covers_the_whole_wire` (`:90-96`) checks every registered method, emitted event and sent request has a contract. The dispatcher rejects unknown param keys with error `4000`; the canonical authority with `4001 invalid_params` (`tui_gateway/AGENTS.md:39-41,49-51`).
- **Typing coverage (measured with Python over the document).** Of 861 schemas, 52 are open objects (`additionalProperties` not false) and 2 are fully opaque. All 257 legacy and 47 canonical **params** are closed. Results: 10 of 257 legacy methods return an open model (`config.show, cron.manage, pet.info, process.kill, rollback.restore, session.compress, session.cwd.set, session.save, session.usage, spawn_tree.load`); 9 of 47 canonical (`cron.recover, cron.status, groups.attachment.download, groups.attachment.upload, session.info, session.mutate, worker.adopt, worker.persist, worker.register`). 57 sub-schemas are free-form objects without properties; 7 of 3,891 properties are completely untyped (`JsonValue`). 15 notification payloads are open, 7 of them the empty `ChangeSignalPayload` refetch signals (by design).

What blocks a clean generated client in Rust, Go or TypeScript:
1. **Non-standard extensions.** Canonical methods, server requests and notifications live under `x-canonical-methods`, `x-server-requests`, `x-notifications` (`gen_gateway_contracts.py:336-354`). Off-the-shelf OpenRPC generators ignore `x-` keys, and 19 names exist in both `methods` and `x-canonical-methods`, 18 of them with different param or result schemas (`approval.respond, bot_relay.deliver, config.get, config.set, groups.approve, groups.create, groups.retry, image.attach_bytes, profiles.list, prompt.submit, session.create, session.events.since, session.interrupt, session.list, session.redirect, session.resume, session.steer, shell.exec`), so a generator must namespace by transport. A ~300-line custom generator (JSON Schema → `serde` structs via a tool such as `typify`, plus hand-rolled method/event enums) is realistic; this estimate is a hypothesis.
2. **Canonical-only events are missing.** `approval.request`, `approval.settled`, `clarify.request`, `clarify.settled`, `session.replay_gap` appear 0 times in the OpenRPC document and are hand-typed in `apps/shared/src/gateway-events.ts:33-49`; `session.updated`, `session.tools_changed` and `message.user` (emitted from `gateway/session_deferred_pick.py:124,136`, `gateway/session_local_mcp.py:330`) are in neither the OpenRPC file nor the generated TS. Cause: the completeness test only scans `tui_gateway/*.py` and a few named files for emitters (`test_generated.py:47-76`), not `gateway/session_*.py`.
3. **The event envelope is not modelled.** Events arrive as `{"method":"event","params":{type, session_id, payload, seq, replay_epoch, authority_epoch?, execution_generation?, admission_id?}}`; the document lists payloads per type but declares no discriminated union for the envelope. Replayed events are typed as `CanonicalReplayEvent` whose `payload` is an open object.
4. **`session.mutate` is stringly typed.** `operation` is a plain string and `payload` an open object; validation happens in `hermes_state_mutations.validate_action` (contract description). Branch, compress, rewind, model and reasoning switches — core client actions — therefore have no generated types.
5. **No error model.** Zero methods declare `errors`; `components` contains only `schemas`. Codes in use include `-32601`, `-32700`, `-32603`, `4000` (unknown param key), `4001` (`invalid_params`, `profile_mismatch`, `runtime_draining`, …), `4403`, `5063`, `5300` and string `reason` codes in `error.data` (`gateway/session_controls.py:22-35`, method docs).
6. **18 canonical methods have empty summaries**; 17 of them are shared names whose canonical shapes differ from the legacy ones (`prompt.submit`, `session.create`, `session.resume`, `session.events.since`, `session.interrupt`, `approval.respond`, …; the 18th is canonical-only `bot_relay.settle`), so the only docs a reader finds describe the other transport's shape.
7. **Versioning is nominal.** `info.version` is the literal `"1"` (`gen_gateway_contracts.py:326-327`) and is still `"1"` on upstream `main` after `session.export`, `tool.progress` and 7 schemas were added (diffed against `main` @ `4e58c721c8`). The file has 219 commits between its creation on 2026-09-14 and the base revision on 2026-10-17 (`git log --oneline 22063fc099 -- apps/shared/src/gateway-contract.openrpc.json | wc -l` → 219), about 6.6 a day. Compatibility signalling instead lives in runtime capability strings (`runtime.describe`, control-socket `identify.capabilities`, `gateway.capabilities`) and the shared client's "version-skew notice" on `-32601` (`tui_gateway/ws.py:356-361`). A third-party client must pin a Hermes revision and regenerate on every bump.

---

## 5. The ACP adapter (`hermes acp`)

**What it supports (verified in `acp_adapter/gateway_server.py` at the base revision):**
- `initialize` advertises `load_session`, image prompts, `fork`, `list`, `resume` (`gateway_server.py:129-138`); `new_session`, `load_session`, `resume_session`, `fork_session`, `list_sessions`, `prompt`, `cancel` (`:245-565`).
- Model catalog and per-session model: `set_session_model` with a picker built from `model.options` (`:441`, `acp_adapter/model_catalog.py`); deferred picks that need consent use `request_permission` (`:464-530`).
- Modes = edit approval policy: `default` (ask), `accept_edits`, `dont_ask` (`acp_adapter/server.py:236-251`); `set_session_mode`, `set_config_option` (`gateway_server.py:534-552`). Several post-base upstream commits rework this (section 7).
- Permissions: approvals become ACP `request_permission`, with an edit diff rendered as an ACP edit tool call when the prompt carries `edit` (`gateway_server.py:915-948`).
- Clarify: ACP form **elicitation** when the editor advertises it, else a permission card (`gateway_server.py:884-913`).
- Streaming projection: `message.delta`, `message.interim`, `message.complete`, `reasoning.delta`, `todo.updated` (as an ACP plan), `tool.start` / `tool.complete`, usage and title updates (`gateway_server.py:762-860`).
- Slash commands: a local route table (`/help`, `/steer`, `/queue`, `/title`, `/undo`, `/retry`, `/tools`, `/model`, `/compress`, `/new`, `/clear`; `/branch` redirects to ACP fork) and everything else forwarded to the owner's `slash.exec`, which refuses commands it does not serve on the canonical transport (`acp_adapter/gateway_commands.py:1-10,255-270`).
- Editor MCP servers passed in `session/new` are attached to the owner session (`acp-session-mcp-v1` capability).

**What a client loses with ACP instead of the native gateway:**
- **No subagent view or control.** The projection ignores every `subagent.*` event (`grep -n subagent acp_adapter/gateway_server.py acp_adapter/gateway_reply.py` → no matches) and there is no ACP method for `subagent.list/tail/steer/interrupt` or `spawn_tree.*`.
- No background-process list/kill, no `prompt.background` / `prompt.btw`, no goal/loop/heartbeat (`session.control*`), no queued-prompt editing (`prompt.steer_queued` / `replace_queued` / `cancel`), no `session.steer` except via `/steer`.
- No configuration, `.env`, MCP, skills, tools/toolsets, plugins, connectors, cron, kanban, projects, handoff, vault, voice, usage bars or subscription surfaces.
- One profile per process (`hermes -p X acp`; `connect_gateway` uses `get_hermes_home()`), no cross-profile session list.
- No rollback/checkpoint diffs, no `status.update` / `notification.*` / change signals, no `sudo`/`secret` prompts (hypothesis: the projection only handles approval and clarify).
- Protocol constraints: ACP's turn model is one prompt → one response per session; a "many agents in background" UI would have to multiplex many ACP sessions and still could not see delegated children.

ACP remains valuable as a compatibility path (Zed, VS Code, JetBrains) and as a second reference client for the canonical protocol.

---

## 6. Recommendation and candidate upstream contributions

### 6.1 Which surface to build on

1. **Primary: the native gateway WebSocket (`hermes-gateway-v1`).** Discover and start the owner with `hermes gateway ensure --json` (never with `--idle-exit`), mint tickets on `$HERMES_HOME/gateway.sock` exactly as `ui-tui/scripts/gateway_bootstrap.py:80-104` does, dial `<api_origin>/api/ws` with the two subprotocols, call `runtime.describe`, then drive sessions with the canonical verbs. On the same socket, call legacy-catalog methods (config, MCP, skills, tools, plugins, connectors, cron, projects, attachments, voice, completions, `commands.catalog`, `slash.exec`) through the authority's legacy fallback. This is the only surface that gives: many concurrent sessions per connection across 26 profiles, durable FIFO admission (queue/steer/edit while busy), sessions that outlive the client, replayable events with gap detection, subagent observation and control, and parity with what Desktop and Ink do today.
2. **Secondary on the same listener: dashboard REST** with the `X-Hermes-Gateway-Ticket` header for things with no RPC: `GET /api/config/schema` (drives a "very user-friendly" settings UI), `/api/env`, `/api/providers/*` OAuth flows, kanban plugin routes.
3. **Remote machines:** the gated-dashboard path with the RFC 8252 native-app PKCE flow and `?token=` (or `HERMES_TUI_GATEWAY_URL`-style explicit URL), behind a tunnel. Needs a live test before committing (unverified end to end).
4. **Not:** stdio (refused beside a gateway), the standalone-dashboard legacy `/api/ws` that Talaria uses today (no session authority, unverified session behaviour, being routed around upstream), ACP (lossy; keep as compatibility), the `/v1/runs` HTTP API (good for scripted runs, too thin for an interactive client).
5. **For "ultracode" multi-agent workflows with any model per agent:** do not rely on `delegate_task`, whose children share one configured model. Orchestrate at the client: create N native sessions with `session.create {model, provider, reasoning, toolsets, skills, cwd, run_budget}` (all present in `CanonicalCreateParams`), submit prompts, and watch them on one socket. Each session runs inside the gateway and keeps going when the client is closed. (Design inference from the contract; nobody has built this yet.)

### 6.2 What is missing upstream (candidate contributions)

| # | Gap | Evidence | Suggested change |
|---|---|---|---|
| 1 | Canonical events absent from the generated contract | `gateway-events.ts:33-49`; 0 hits in OpenRPC; completeness test scans only `tui_gateway/` (`test_generated.py:47-76`) | Declare `approval.request/settled`, `clarify.request/settled`, `session.replay_gap`, `session.updated`, `session.tools_changed`, `message.user` in `contracts/events.py`; extend the emitter scan to `gateway/session_*.py` |
| 2 | `session.mutate` untyped | `operation: string`, `payload: object` in `SessionMutateParams` | Discriminated union per operation (`rename`, `branch`, `compress`, `rewind`, `model`, `reasoning`, …) with typed results |
| 3 | No error model | 0 `errors` entries | Declare error codes and `error.data.reason` enum per method |
| 4 | Event envelope not modelled; replay payload open | `CanonicalReplayEvent.payload` open | A `oneOf` envelope discriminated on `type` |
| 5 | Nominal versioning | `info.version: "1"` unchanged while the catalog grew | Content hash or semver in `info.version`, surfaced by `runtime.describe`; a deprecation window policy |
| 6 | 18 canonical methods undocumented | empty `summary` | One-line docs |
| 7 | No public third-party bootstrap contract | ticket minting is a private control-socket verb documented only in code | Document `session-ticket` as stable, or add a `hermes gateway connect --json` that prints a URL + subprotocols with a fresh ticket for same-user callers |
| 8 | Per-child model in delegation | `delegate_task` task fields lack `model`/`provider`; global `delegation.model` only | Optional per-task `model`/`provider`/`reasoning`, validated against configured providers |
| 9 | Kanban has no RPC | only canonical `kanban.run` (owner-only) | `kanban.*` read/write verbs on the canonical transport, mirroring the plugin REST router |
| 10 | Remote terminal auth | native PKCE flow needs a browser and a loopback redirect | A device-code flow for headless/SSH terminals (hypothesis that it does not already exist elsewhere; I found none in `dashboard_auth/`) |
| 11 | Unattended sudo/secret | `_ask` returns `None` with no viewer and does not replay to a later viewer (`session_server_requests.py:135-141`) — hypothesis on impact | Publish open server requests into the replay ring / resume snapshot like approvals |

---

## 7. Upstream changes since the base revision

`gh api repos/NousResearch/hermes-agent/compare/22063fc099…main` → `ahead_by: 1408`, `behind_by: 0` (the API truncates the file list at 300, so I listed commits per path instead). Upstream `main` is `4e58c721c8` (2026-10-22T01:39:02Z); release `v0.22.4` (2026-10-21T18:49:24Z) contains the base revision and is 1,338 commits ahead of it. Commits per path not already in the base (merges included; lists saved as `u04-upstream-*.new.tsv` beside this report): `tui_gateway/` 81, `apps/shared/` 30, `gateway/session_controls.py` 13, `acp_adapter/` 20.

Contract diff, base vs `main` (parsed both OpenRPC files): legacy `methods` unchanged at 257; `x-canonical-methods` 47 → 48 (**added `session.export`**: "The /save transcript rendered for the client to write (the gateway writes nothing)"); `x-server-requests` unchanged at 16; `x-notifications` 79 → 80 (**added `tool.progress`**: "Live progress of a running tool call (MCP notifications/progress)"); schemas 861 → 868; `info.version` still `"1"`.

Relevant commits (subjects quoted):
- Replay and reconnect: `6930e77a1b` "fix(gateway): replay missed events and prompts after a legacy reconnect"; `875577495b` "fix(gateway): re-attach after session.replay_gap instead of failing the turn"; `e6653e1c9f` "perf(tui_gateway): free a torn-down session's event replay ring" (+ three follow-ups); `98159a3200` "feat(gateway): session.resume carries the session's todo snapshot".
- Sessions staying alive: `ff63499e90` "fix(tui_gateway): keep detached sessions alive while a background process will notify them"; `06bcbc2ab7` "hold foreign owners through the post-turn drain; keep owners across agent rebuilds".
- Queue/steer: `936ec14ee4` "fix(tui): queue a Desktop prompt in the gateway owner's FIFO instead of refusing it"; `b2c269f665` "re-admit only a correction the owner marks undelivered"; `b0bb0d8749` "fix(contracts): admit model_override.reasoning on prompt.submit".
- New surface: `8930c3c632` "feat(gateway): session.export returns the /save transcript for the client to write"; `60affe7fe2` "feat(mcp): forward MCP notifications/progress as live tool.progress events"; `e6da913585` merge of PR #138820 "capabilities-catalog" (contents not read).
- Performance of listings: `45e7f7dd22` "the live session listings read the store off the loop, one query per batch"; `2d7f9b5f2a` "stop hydrating a row per session"; `6aca02a7be` "the control-publish lock is per session, so one stalled client can't delay others".
- ACP: a run of edit-mode fixes (`8729dd9e74` "restore Accept Edits / Don't Ask as the session's owner-held edit policy", `64d4862c3d` "one edit-mode table for both ACP servers", `0bfd5800f5` "recover unknown turns in editor").
- Profiles: `70872e7fa6` "the default profile keeps env-only credentials after the in-place multiplex switch".
- From release notes (not traced in code): v0.22.4's plugin catalog adds "mcp-serve (Hermes as an MCP server)" — another possible client surface, unverified.

Implication: the contract moves daily. A client generated from the base revision would miss `session.export` and `tool.progress` four days later, and several reconnect semantics changed in the same window. Pin a Hermes revision, regenerate from that revision's OpenRPC file, and gate on `runtime.describe` capabilities.

---

## 8. Open questions

1. Does a standalone `hermes dashboard` `/api/ws` (no session authority) still create working sessions beside a live gateway, or are they refused? Matters only if anything keeps using that path (Talaria does today).
2. End-to-end remote attach: gated listener + native PKCE + `?token=` against the gateway's own listener behind a tunnel — not exercised.
3. Are canonical `sudo`/`secret` requests really lost for a viewer that attaches late (section 3.1 hypothesis)?
4. What does upstream PR #138820 ("capabilities catalog") add to `apps/shared`?
5. Is there any statement from Hermes maintainers about supporting third-party clients on `hermes-gateway-v1` (compatibility window, deprecation)? None found in `AGENTS.md` files.
