# 03 — What Hermes offers a client today

Status: `proposed`
Evidence: [u04](research/u04-hermes-client-surfaces.md) (client surfaces), [u15](research/u15-hermes-orchestration.md)
(orchestration), [u07](research/u07-hermes-ink-tui.md) (Hermes's own terminal UI),
[u05](research/u05-hermes-release-delta.md) (release notes), [u06](research/u06-hermes-desktop-catalog.md)
(Desktop), and a live attach probe run on 2026-10-22 (section 2). Hermes was read at the installed
revision `22063fc09` (2026-10-17). The newest release is v0.22.4 (2026-10-21), about 1,338 commits
later; re-check anything load-bearing against the Hermes you are building against.

Research reports are cited by number; for example, u11 is research/u11-language-framework.md, the
language and framework evaluation.

## 1. The big change: one backend per machine, and clients are viewers

Hermes v0.22.0 (2026-10-13) made one gateway process per machine serve every profile. The session
authority on top of it came later. It means the gateway owns every session, every first-party client
(the Ink terminal UI, Desktop, the editor adapter for the Agent Client Protocol (ACP), and the classic
command line) attaches as a *viewer* over a WebSocket speaking a protocol named `hermes-gateway-v1`,
and tickets are minted on `$HERMES_HOME/gateway.sock`. It first shipped in a stable release in v0.22.3
(2026-10-21); v0.22.1 shipped it by mistake and v0.22.2 withdrew it. **Talaria's minimum supported
Hermes is v0.22.3.** Closing a viewer never stops its sessions.

This settles three things the old Talaria design had to work around:

- **"Always running" is a backend property now.** A prompt submitted to a session becomes a durable
  "admission" in that profile's database; it runs whether or not any client is attached, and a
  client that reconnects can replay what it missed (`session.events.since`) or ask for a full snapshot
  (`session.resume`). An approval with nobody watching waits for `approvals.timeout` (default 300 s)
  and is then denied, so a background-agent client must surface pending approvals as notifications.
  ([u04](research/u04-hermes-client-surfaces.md) §3)
- **One connection can reach every profile.** The gateway on the test machine serves 26 profiles from
  one process. A connection ticket binds one profile by default; a ticket with `scope: "host"` lets an
  interactive connection select any served profile per call. Talaria's per-profile gateway fleet is
  obsolete.
- **The old attach path is a legacy path.** A standalone `hermes dashboard` still serves a `/api/ws`
  endpoint, which is what Talaria uses today, but that socket has no session authority, and upstream
  is routing work around it. Spawning the old standard-input/output server
  (`python -m tui_gateway.entry`) is refused while a gateway owns the profile.

## 2. How a third-party client attaches (verified live)

The steps below were run by a throwaway script on 2026-10-22 against the running gateway, as an
ordinary same-user process with no Hermes code imported. All of it is local-only by design.

1. **Find the gateway.** Connect to the Unix socket `$HERMES_HOME/gateway.sock`, or to the path
   written in `$HERMES_HOME/gateway.sock.path` when that pointer file exists (normally the socket is
   `~/.hermes/gateway.sock`; Hermes checks that the peer is the same operating-system user). Send one
   JSON line: `{"verb":"identify","id":1,"protocol":1}`. The reply carries `api_origin` (the
   gateway's loopback HTTP address, on a random port), `instance_id`, `served_profiles` (each
   `{profile_id, home}`; the default profile's id is the path of the Hermes home), `parked_profiles`
   (profile name → the reason the gateway is not serving it; a rescan can serve it again),
   `capabilities` (`session-authority-v1`, `durable-admission-v1`, `event-replay-v1`) and
   `code_version`.
   If nothing is listening, `hermes gateway ensure --json` starts or finds the gateway without
   printing credentials (exit codes: 0 ready, 3 incompatible, 4 authorization, 5 deadline, 6 draining,
   7 conflict). Do not start it with `--idle-exit`; that variant stops itself after ten idle minutes.
2. **Mint a ticket.** On a new connection to the same socket, send
   `{"protocol":1,"id":2,"verb":"session-ticket","params":{"profile_id":"<id>","instance_id":"<id>","purpose":"interactive","scope":"profile"|"host"}}`.
   The reply's `ticket` is single-use and valid for 30 seconds. Never log it.
3. **Open the WebSocket.** Dial `<api_origin with http→ws>/api/ws`, offering two subprotocols:
   `hermes-gateway-v1` and `hermes-gateway-ticket.<ticket>`. Send no `Origin` header (the gateway
   refuses browser-origin connections). The server echoes `hermes-gateway-v1`.
4. **Handshake.** The first frame is a JSON-RPC notification with `method: "event"` and
   `params.type: "gateway.ready"`. Every event uses this envelope, with the event name in
   `params.type` and its fields in `params.payload`. The `gateway.ready` payload holds `skin`,
   `heartbeat`, `replay_epoch`, `session_authority: true` and `change_events`. Call
   `runtime.describe` for the owner's identity, epoch, capabilities and the accepted `session.create`
   parameters.

Measured on the warm path: identify 1 to 3 ms, ticket about 1 ms, WebSocket upgrade 17 to 20 ms,
`gateway.ready` about 5 ms, `runtime.describe` 5 to 9 ms. The first connection after the gateway had
been idle took 771 ms to upgrade. Over the same socket, `session.list` answered in about 1 ms,
`profiles.list` in 130 ms, and `model.options` in 1.2 s, reporting 14 authenticated providers with 295
models between them.

Two things to know:

- **The gateway's served profiles and the dashboard's profiles can differ.** On the test machine one
  profile the operator uses daily was served by a standalone dashboard but not by the gateway, and a
  ticket request naming it was refused with "bootstrap binding rejected". A client must show which
  profiles the gateway actually serves and say why one is missing: first look the profile up in
  `parked_profiles`, and if it is not there, report that the gateway never discovered it.
- **Remote machines are a separate problem.** The ticket path is loopback-only. A remote client needs
  a gated dashboard with the native-app sign-in flow (PKCE: an authorization code exchanged with a
  proof key) behind a tunnel; no one has exercised this end to end yet. See
  [u04](research/u04-hermes-client-surfaces.md) §1.2.

Reference implementations to read: `ui-tui/scripts/gateway_bootstrap.py` (about 120 lines; what
Hermes's own terminal UI runs) and `hermes_cli/gateway_client.py` (about 240 lines; `connect_gateway`,
used by the ACP adapter and the classic command line). A Rust and a TypeScript client that did all of
this live are in [reference-slices/](reference-slices/).

**Traps the live gateway set for every builder.** The four bake-off builders all met these on
2026-10-22. Details are in [u16](research/u16-stack-bakeoff.md) §5.

- **`prompt.submit` needs an idempotency key.** It is refused with `invalid_params` unless it
  carries `submission_id` or `input_id`. The contract marks both as optional.
- **`prompt.submit` only admits the prompt.** It answers with a receipt (`status: queued`), and the
  turn arrives as events.
- **Keep two values from `session.create`.**
  - `subscription_id`, which `session.detach` needs.
  - `execution_generation`, which rises with each turn. `session.interrupt` and `approval.respond`
    need the current value.
- **Map approval choices by the payload's order.** They arrive as `once, deny, session, always`.
  On the canonical socket they arrive as events, not as server requests.
- **Interrupting does not answer an open approval.** The gateway waits out the prompt's 60-second
  timeout, so deny first and then interrupt.
- **Never print `inline_diff` raw.** It arrives already coloured with ANSI escapes, under its own
  header. Strip the escapes before classifying `+` and `-` lines.
- **Read tool outcomes carefully.**
  - `tool.complete` reports failure with `is_error`, and its `result` is a JSON string.
  - The command may be missing from the completion, so carry it from `tool.start`.
  - `duration_s` is not always present.
- **Expect events the specification does not list.** Progress arrives as `thinking.delta`, and other
  unlisted events appear. Ignore unknown events gracefully.
- **Every `session.create` leaves a session row in the profile.** Test against a throwaway profile.

## 3. The contract: generated, typed, and moving fast

Hermes declares every method, server-to-client request and event as a Pydantic model and generates
two files from them: `apps/shared/src/gateway-contract.openrpc.json` (OpenRPC 1.3.2, a JSON
description of JSON-RPC services) and `apps/shared/src/gateway-contract.generated.ts` (TypeScript
types). Hermes's own tests fail when either is stale. At `22063fc09` the OpenRPC file holds 257
methods, 47 methods specific to the canonical `hermes-gateway-v1` socket (under the key
`x-canonical-methods`), 16 server-to-client requests (`x-server-requests`), 79 notifications
(`x-notifications`) and 861 schemas.

It is good enough to generate most of a typed client, with known gaps ([u04](research/u04-hermes-client-surfaces.md) §4,
[u11](research/u11-language-framework.md) §3.4):

- Generators ignore `x-` keys, and 18 method names exist in both catalogs with different shapes, so a
  generator must namespace by transport.
- Five canonical events are not in the contract at all — `approval.request`, `approval.settled`,
  `clarify.request`, `clarify.settled`, `session.replay_gap` — and are hand-typed in
  `apps/shared/src/gateway-events.ts`. Three more (`session.updated`, `session.tools_changed`,
  `message.user`) are in neither file.
- `session.mutate` (rename, branch, compress, rewind, model, reasoning effort, and more) has an untyped
  `operation` string and `payload` object.
- No error codes are declared, and `info.version` has been `"1"` through 219 commits in five weeks.
- Rust code generation with `cargo typify` works after a small preprocessing step: drop defaults from
  three schemas and strip `additionalProperties: false`, or every new field Hermes adds breaks
  decoding. TypeScript needs no generation: Hermes's own client files (MIT licence) type-check and run
  unmodified outside the Hermes repository.

**Implication:** pin a Hermes release, regenerate types from that release's contract, gate features
on `runtime.describe` capabilities, and run a scheduled job that regenerates against the newest Hermes
stable release and fails on breaking changes. Hermes now ships tested SemVer (semantic versioning)
releases on a stable channel (since v0.21.6), which makes "target the stable channel" a realistic policy.

## 4. What the backend already does for a "Claude Code for Hermes" client

Everything below is a backend feature reachable from the canonical socket. The client's job is to
present it well. Method names are from the contract; see [u04](research/u04-hermes-client-surfaces.md)
§2 for the full catalog with one-line descriptions.

| Need | Hermes provides |
|---|---|
| Sessions | `session.create` (model, provider, reasoning effort, toolsets, skills, working directory, turn and time budgets, all frozen at creation), `session.resume` (full snapshot), `session.list`, `session.detach`, `session.mutate` (rename, branch, compress, rewind, reset, model, reasoning, workspace move, archive, delete), `session.export` (v0.22.3+) |
| Prompts | `prompt.submit` (a busy session queues into a durable first-in-first-out queue), `prompt.steer_queued`, `prompt.replace_queued`, `prompt.cancel`, `prompt.receipt`, `session.steer` (inject into the next tool result), `session.redirect`, `session.interrupt` |
| Side work | `prompt.background` (a fresh background agent; result arrives as `background.complete`), `prompt.btw` (a side question over a snapshot; `btw.complete`) |
| Streaming | `message.start/delta/interim/complete`, `reasoning.delta`, `tool.start/generating/complete` (with summaries and optional diffs), `tool.progress` (v0.22.3+), `todo.updated`, `status.update`, `session.usage` |
| Questions | Approvals and clarifications as canonical events with `approval.respond` / `clarify.respond`; `sudo` and `secret` as true server-to-client requests |
| Subagents | `subagent.*` events (spawn, start, thinking, tool, progress, complete) with parent links, depth, model and token counts; `subagent.list`, `subagent.tail` (last 16 KB of a child's transcript), `subagent.steer`, `subagent.interrupt`; `delegation.status` and `delegation.pause` |
| Checkpoints | `rollback.list`, `rollback.diff`, `rollback.restore`, `rollback.session_diff` |
| Commands | `commands.catalog`, `slash.exec`, `command.dispatch`, `complete.slash`, `complete.path` (for `@` mentions) |
| Shell | `shell.exec` (the `!command` path, safe commands only) |
| Attachments | `image.attach_bytes`, `file.attach`, `pdf.attach`, `clipboard.paste`, `paste.collapse` |
| Models | `model.options` (every provider with models, authentication state, pricing strings, capabilities, rate limits and subscription usage windows), `model.save_key`, `model.disconnect` |
| Configuration | `config.get`, `config.set`, `config.show`; the dashboard REST routes `GET /api/config/schema` (the field schema that should drive a settings screen), `/api/config`, `/api/env` |
| Profiles | `profiles.list`, `profiles.describe`, `profiles.create`, `profiles.configure` |
| Extensions | `mcp.catalog` and `mcp.servers.*` for Model Context Protocol (MCP) servers (add, remove, test, OAuth), `skills.manage`, `plugins.manage`, `tools.list` and `tools.configure`, `connectors.*` |
| Scheduling | `cron.manage`; canonical `cron.submit/status/cancel/recover`; kanban through its REST routes and the `hermes kanban` command |
| Projects | `projects.*` (folders, active project, repositories, per-project session lanes) |
| Change signals | `sessions.changed`, `skills.changed`, `cron.changed`, `projects.changed` and others tell a client when to refetch |

Gaps that matter for the operator's goals:

- **`delegate_task` cannot choose a model per child.** Every child in a batch uses the profile's
  `delegation.model`. Per-agent model choice exists in `session.create`, in kanban tasks
  (`model_override`, `provider_override`, `reasoning_effort`) and in Mixture of Agents presets.
  ([u15](research/u15-hermes-orchestration.md) §1)
- **There is no workflow runner.** Kanban is a durable task graph with per-task models and a
  gateway-hosted dispatcher, and `hermes kanban swarm` builds a workers-then-verifier-then-synthesizer
  graph, but nothing runs a deterministic script. See [06-workflows.md](06-workflows.md).
- **There is no `output_schema` on `prompt.submit`.** Structured results must be validated client-side
  (the same one-correction-retry contract `delegate_task` already uses).
- **Kanban has no RPC on the canonical socket**, only REST routes that are unauthenticated on
  localhost.

## 5. Hermes's own terminal UI is the bar to clear

`hermes --tui` is React with a forked Ink renderer (27,451 lines of fork), attached to the gateway as a
viewer. It already has: a live-agents dock and a Ctrl+T subagent overlay with tail, detail, steer, stop
and pause; typed follow-ups that queue by default with edit and remove; a four-stage model picker with
reasoning effort and a session-or-persist toggle; Ctrl+G to edit the draft in `$EDITOR`; clipboard
image paste; nine skins with light-terminal detection; and, behind `--native` (undocumented on the
website), a main-screen mode where the terminal owns scrollback and search. Its default is the
alternate screen with a virtualized transcript. It took 3.0 to 3.7 s to a usable prompt in the
hands-on drive. Its own repository lists gaps: secret and sudo prompts and batch clarifications are not
claimed on the canonical path. ([u07](research/u07-hermes-ink-tui.md))

Talaria's reason to exist is to be clearly better than this on speed, cleanliness, multi-agent work,
configuration and workflows — not to match it.

## 6. What changed in Hermes since Talaria's last pin

See [u05](research/u05-hermes-release-delta.md) for the full table with release-note quotes (ten
releases from v2026.9.14 to v0.22.4). The changes that most affect a terminal client:

**Breaking for Talaria as it stands**

- **2026-09-14: prompts became server-to-client requests.** Talaria's compatibility baseline
  (`7f4d15515`, 2026-08-01) predates this change. Its acceptance tests cite `3236f4405` (2026-09-14),
  which already includes it, so Talaria's own records disagree about which protocol it targets. On
  the legacy socket the gateway stopped emitting `approval.request`, `clarify.request`, `secret.request`,
  `sudo.request` and `terminal.read.request` events and stopped accepting the matching `*.respond`
  methods for secret, sudo and terminal-read; it now sends JSON-RPC requests the client must answer.
  Two days later it started withdrawing them from clients that never declared `client.capabilities`.
  On the canonical socket, approvals and clarifications are events again (`approval.request`,
  `clarify.request`) answered with `approval.respond` and `clarify.respond`, while sudo and secret stay
  server-to-client requests.
- **v0.22.0: one gateway per machine** (`gateway.multiplex_profiles` on by default). Talaria's
  per-profile connection fleet no longer matches the backend.
- **v0.22.3: the session authority, the control-socket ticket and the canonical `hermes-gateway-v1`
  methods ship in a stable release for the first time.** No earlier stable release has them.
- **v0.22.0 to v0.22.2 deleted ended sessions inactive for 90 days at startup by default;** v0.22.3
  made that opt-in again. v0.22.1 was built from the wrong commit; do not target it.

**New capabilities a client should use**

- The generated contract (OpenRPC plus TypeScript), and runtime capability strings.
- Live subagent listing, tail, steer and stop (v0.22.0), shown above the composer in Hermes's own
  clients.
- An editable, steerable queue: `/queue` to list, edit, move or remove queued prompts (v0.22.0);
  `prompt.steer_queued` and `prompt.replace_queued` (v0.22.3).
- A model picker that knows subscription plans: favourites, a usage chip before a plan runs out,
  rate-limit lift times, and reasoning effort chosen with the model (v0.21.6 to v0.22.0).
- `hermes --native`, where the terminal owns scrollback (v0.22.0).
- Double-Esc interrupts; Ctrl+C clears the draft; a bell and desktop notification when Hermes waits on
  the user (v0.22.0).
- Device-code login for MCP servers in headless terminals: `hermes mcp login <server> --flow device`
  (v0.22.0).
- A separate model and provider for delegated children, set per profile in the `delegation` config
  block, with its own fallback chain (v0.22.0), and a guided picker for it in Desktop (v0.22.3). This
  is one child model per profile, not a model per child.
- New in v0.22.3/v0.22.4 (absent from the installed canary `22063fc09`): the canonical
  `session.export` method and a live `tool.progress` notification.

**Not breaking, but not identical:** between the installed build and v0.22.4 no method was added or
removed from the 257 legacy methods. Thirteen referenced schemas gained fields, and the canonical
`prompt.submit` result became a union (`AdmissionStatus` or `{voice_stopped: true}`). Every change is
additive, so a decoder that ignores unknown fields and handles the union keeps working. Generate types
from the pinned release, not from `22063fc09`. v0.23.0 is announced in the v0.22.2 and v0.22.3 release
notes but not yet published.

## 7. What the Desktop app has that a terminal client should borrow

See [u06](research/u06-hermes-desktop-catalog.md) for the screen-by-screen catalog with a fit verdict
for each feature. Desktop is Electron with React: 1,748 non-test TypeScript files and 415,127 lines in
its renderer alone. Of 89 features catalogued, **37 are core** for a terminal client, 30 are nice to
have, and 22 should be skipped.

**Core — build these** (grouped; numbers are the catalog's feature ids):

| Group | Desktop features | How Desktop does it |
|---|---|---|
| Sessions and chat | Session sidebar across profiles (F01), several chats live at once (F02), streaming transcript (F03), create and resume (F04) | `session.*` RPC plus REST `/api/profiles/sessions`, `/api/sessions/{id}` |
| Composer | Send (F06), background prompt (F07), slash commands from the backend catalog (F09), `@` completion (F10), attachments (F11), model pill and switch (F12), reasoning effort (F13), approval mode `manual`/`smart`/`off` (F14), context breakdown and compression (F16), change working folder (F17), stop, steer and redirect (F18), rollback and undo (F19) | `prompt.*`, `commands.catalog`, `complete.*`, `image.*`/`file.attach`, `config.set` (`model`, `approvals.mode`), `session.context_breakdown`, `session.mutate` (`compress`, `workspace`; the legacy `session.compress` and `session.cwd.set` that Desktop's catalog lists are refused on the gateway socket), `session.interrupt/steer/redirect` |
| Live work | The status stack above the composer: subagents and background processes (F20), the subagent tree (F31), background processes with kill (F32) | `subagent.list/tail`, `process.list/kill/stop`, `subagent.*` events. Desktop never calls `subagent.steer` or `subagent.interrupt`; test them before relying on them |
| Questions | Command approval (F22), clarifying questions (F23) | `approval.pending/received/respond`, `clarify.*` |
| Models and providers | Model picker (F26), provider keys (F27), custom endpoints (F28), provider sign-in (F29), model roles: main, auxiliary, Mixture of Agents, fallbacks (F30) | `model.options`, REST `/api/env`, `/api/providers/*`, `/api/model/*` |
| Review | Changed files with stage, revert, commit and pull request (F36), inline diffs (F37) | Local git and `gh` (Desktop does this in Electron, so a terminal client does it itself) |
| Capabilities | Skills (F44), toolsets with per-toolset model (F45), MCP servers (F48), memory and its provider (F51) | REST `/api/skills/*`, `/api/tools/toolsets/*`, `/api/mcp/*`, `/api/memory/*` |
| Profiles and backend | Profiles (F53), profile switcher (F54), gateway connection (F55), gateway restart (F56) | REST `/api/profiles/*`, `POST /api/gateway/restart` |
| Navigation | Command palette (F80) | Local |

**Nice, after the core works:** importing Claude Code and Codex sessions (F05), side questions (F08),
handoff to a messaging platform (F21), masked sudo and secret prompts (F24 — needed as soon as tools
ask for passwords), the agent's terminal output stream (F33), projects (F39), connectors (F49),
plugins (F50), messaging platforms, pairing and webhooks (F57 to F60), cron (F61), the kanban board
(F62), voice (F63 to F65), the command center, diagnostics and Hermes updates (F66 to F68), and
Desktop's "Simple" interface tier (F79): a short default view that hides developer detail.

**Skip:** anything that needs a graphical window or duplicates the terminal: the embedded terminal,
browser and preview panes, the HUD (Desktop's heads-up overlay), quick entry, Bot Screen, the pet,
radio, the learning map, billing, telemetry, appearance and keybinding editors, tours and reactions.

**Configuration mechanics worth copying.** `GET /api/config/schema` returns every field with its
category; `GET /api/config/defaults` the defaults; `PUT /api/config` deep-merges, so send only the keys
that changed; live settings go through `config.set`; secrets go through `/api/env` with a separate
one-shot reveal. The catalog lists 28 configuration areas with the exact read and write calls for
each ([u06](research/u06-hermes-desktop-catalog.md) §8). One Desktop idea transfers directly: a "bot" is a profile plus one hidden persistent
chat, which is a good model for named long-lived agents in the fleet view.
