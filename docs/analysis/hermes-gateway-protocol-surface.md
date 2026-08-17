# Hermes gateway protocol surface

Status: `active`
Authority: `reference`
Source: [Hermes Agent](https://github.com/NousResearch/hermes-agent) at `7f4d15515` (2026-08-01);
re-pinned additions at `7095e23eb` (2026-08-17, v0.4 U1) in the dated section at the end

Derived by reading Hermes's own terminal UI (`ui-tui/`) and gateway (`tui_gateway/`) rather than by
observation. This is reference data, not a decision — nothing here is canon until an ADR promotes it.

**Pin the revision when you re-derive this.** An earlier reading of a six-week-old checkout produced
a materially different and wrong picture; see the engineering journal.

## Why read the client at all

Talaria's recorder attaches and listens, so it observes one direction. A protocol has two. Hermes's
`ui-tui/` is a working client that sends the other half, which makes it the only available
documentation of traffic Talaria cannot yet see.

This is not a small program: `ui-tui/src` is 58,581 lines across 277 TypeScript files, and the whole
`ui-tui/` workspace including `packages/hermes-ink` is 88,744 across 429. The parts that carry
protocol knowledge are much smaller.

| file                                   | lines | what it is                             |
| -------------------------------------- | ----: | -------------------------------------- |
| `src/app/createGatewayEventHandler.ts` |  1419 | inbound event → UI state mapping       |
| `src/gatewayClient.ts`                 |   794 | transport, request/response, reconnect |
| `src/gatewayTypes.ts`                  |   741 | the typed protocol contract            |

## Two transports, one dispatch surface

`gatewayClient.ts` sends over **either** a WebSocket or a subprocess's stdin, choosing at runtime and
framing identically (`{id, jsonrpc, method, params}`). The standalone attach path and the
`hermes --tui` bundle path are the same protocol over different pipes.

## Inbound: 44 event types

All arrive as `{"method": "event", "params": {"type": ..., "payload": ...}}`.

| family            | types                                                                                                                                                                  |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| assistant output  | `message.start` `message.delta` `message.interim` `message.complete` `thinking.delta` `reasoning.delta` `reasoning.available`                                          |
| tools             | `tool.start` `tool.progress` `tool.generating` `tool.complete`                                                                                                         |
| subagents         | `subagent.start` `subagent.progress` `subagent.thinking` `subagent.tool` `subagent.complete` `subagent.spawn_requested`                                                |
| blocking prompts  | `approval.request` `clarify.request` `secret.request` `secret.expire` `sudo.request` `sudo.expire`                                                                     |
| gateway lifecycle | `gateway.ready` `gateway.protocol_error` `gateway.start_timeout` `gateway.stderr`                                                                                      |
| mixture-of-agents | `moa.phase` `moa.progress` `moa.aggregating` `moa.reference`                                                                                                           |
| session and shell | `session.info` `status.update` `background.complete` `review.summary` `error`                                                                                          |
| ambient           | `notification.show` `notification.clear` `skin.changed` `reaction` `voice.status` `voice.transcript` `wake.detected` `browser.progress` `billing.step_up.verification` |

Two observations that matter for Talaria specifically:

- **Sub-agent visibility is already a first-class event family**, not something Talaria must synthesize
  from transcript scraping. Six event types carry it. This is the project's stated differentiator and
  the protocol already serves it.
- **The blocking prompts are request/response round trips split across directions.** The gateway sends
  `sudo.request` inbound with an empty payload; the client answers `sudo.respond` outbound carrying the
  plaintext password. Every credential on this protocol travels in the direction a listener cannot see.

## Outbound: what the shipping client sends

The gateway defines **130** methods. `ui-tui` calls **32** of them, so the shipping client exercises
roughly a quarter of the surface.

```
clipboard.paste     command.dispatch    delegation.pause    delegation.status
image.attach        image.detach        input.detect_drop   learning.delete
learning.detail     learning.edit       learning.frames     model.disconnect
model.options       model.save_key      paste.collapse      pet.gallery
pet.select          plugins.manage      prompt.submit       session.activate
session.active_list session.create      session.delete      session.interrupt
session.list        session.resume      session.steer       shell.exec
skills.manage       slash.exec          subagent.interrupt  system.battery
```

`model.save_key` carries `params.api_key`. It is not one of the four blocking bridges, which is the
evidence that credentials on this protocol are not confined to the methods named for them.

## Handshake

`gateway.ready` arrives immediately on connect, carrying the full skin and `change_events: true`.
That boolean is **hardcoded**, not negotiated — it announces that this backend broadcasts
`pet.changed` / `cron.changed` / `sessions.changed` so clients can demote polling. It is the only
capability-shaped value the terminal gateway publishes, and since it is constant it cannot be used to
detect anything. Terminal-gateway capabilities must still be inferred by probing.

## What a first iteration actually needs

Rendering a usable conversation needs a small subset: `message.*` for assistant text, `tool.*` for
what it is doing, `gateway.ready` for connect, and `error`. Everything else degrades to ignored.

The blocking prompts are the first hard requirement beyond display, because ignoring them hangs the
agent rather than merely showing less — and answering them is what puts credentials on the wire.

## Re-pin at `7095e23eb` (2026-08-17, recorded by v0.4 unit U1)

Everything above stands as written at `7f4d15515`. The v0.4 fleet turn re-pinned the read to
`7095e23eb`, the running install's checkout, and drove the load-bearing additions live where the
serving process allowed. Full live evidence, including which claims are wire-verified versus
source-only, is in
[2026-08-17-v0-4-topology-verification.md](2026-08-17-v0-4-topology-verification.md). The surface
deltas:

- **154 registered methods** (was 130 at the old pin; 135 at the revision the live processes run).
  The delta from the old pin is exactly 24 added and none removed. The additions v0.4 builds on:
  `approval.pending` (per-session pending-approval snapshot — **side-effecting**: it warms the
  session's agent build via `_sess`, `tui_gateway/server.py:2500`), `approval.received` (client
  ack), and a third GUI blocking bridge, `mcp.setup.respond`. Other additions (`profiles.*`,
  `mcp.servers.*`, `image.generate`, `session.set_hidden`, `session.workspace.move`,
  `subagent.steer`, `wake.feed`, `mcp.catalog`, `preview.read.respond`, `window.read.respond`) are
  outside Talaria's needs.
- **`session.active_list` is NOT new — it was registered at the old pin too**
  (`tui_gateway/methods_session.py` at `7f4d15515`), and its status vocabulary
  `waiting | starting | working | idle` is unchanged: `_session_live_status` in
  `tui_gateway/server.py` is byte-identical at `7f4d15515`, `91a545ab1` and `7095e23eb`. This
  corrects the v0.4 plan's KTD4, which lists the roster method alongside `approval.pending` as
  something "the old pin does not have"; only `approval.pending` is genuinely new. The line above
  listing the 32 methods the shipping terminal client calls already included the roster method, and
  the enumeration confirms it. Probing it as a capability stays harmless — a probe of an
  always-present method always succeeds — but **the roster must not be gated behind a
  version check**, because there is no version of this gateway Talaria dials that lacks it.
- **Session events are transport-scoped, not broadcast.** `write_json`
  (`server.py:1637`) routes any event frame carrying a `session_id` to the one transport stored on
  that session; only session-less events fan out to all peers via `_broadcast_global_event`
  (`server.py:1691`) — the broadcast set that matters is `sessions.changed` (empty payload,
  signature watcher, ~2 s coalescence) and `session.reclaimed` (`{session_id, stored_session_id,
  reason}`, only for the three backend reap reasons — an explicit `session.close` does not emit
  it). Verified live on two concurrent connections: a session created and driven on one produced
  zero session-scoped frames on the other.
- **Four calls rebind that transport: `session.create`, `session.resume`, `session.activate` — and
  `prompt.submit`** (`methods_prompt.py:337-341`). Attach *and submit* are steal; the displaced
  client receives nothing, mid-turn included (verified live in the one leg that measured the
  displaced connection's frame count). That the `*.respond` bridges do **not** rebind, and resolve
  purely by `request_id` with no transport or session check, is **source-derived, not live-verified**
  — every non-approval bridge delegates to `_respond` (`server.py:11629-11641`), which looks up
  `params["request_id"]` in the `_pending` registry and never resolves a session; and no `*.respond`
  handler appears among the writers of `session["transport"]` (`methods_prompt.py:341`,
  `server.py:8664`, `:8089`, `:1194`, `compute_host.py:530`, `:632`). The live run answered every
  prompt from a connection that had just activated the session, so it never exercised a respond from
  a non-owning connection.
- **`session.activate`'s reply hydrates `pending_approval` and `pending_clarify`** when they exist
  (`server.py:8708-8711`) — and nothing else: a pending sudo/secret is not in the payload. New in
  `7095e23eb`; not yet live on this machine's serving processes (see the caveat below).
- **Approvals gained a keyed identity.** Every queued approval entry synthesizes a `request_id`
  (`tools/approval.py:2596`) carried by the request event, the pending snapshot, and an optional
  aimed `approval.respond`. Pre-pin gateways accept and silently ignore the parameter. Approval
  rows carry **no start stamp** — wait ages have no authoritative start time on this protocol.
- **Probing absence works by error code:** an absent method answers `-32601` (`unknown method: …`);
  a present method with a bad or missing session answers `4001` (`session not found`). Verified
  live for `approval.pending`, which is absent on the wire this machine serves today.

**Caveat that earned its own rule: the checkout is not the process.** At recording time the serving
processes predated the checkout's advance to `7095e23eb` and execute `91a545ab1` — which lacks
`approval.pending`, the activate-reply hydration, the approval `request_id`, and `mcp.setup`. A
re-derivation of this document must pin the revision the *process* imported (process start time
versus checkout history), not the revision the working tree shows.
