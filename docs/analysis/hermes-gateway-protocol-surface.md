# Hermes gateway protocol surface

Status: `active`
Authority: `reference`
Source: [Hermes Agent](https://github.com/NousResearch/hermes-agent) at `7f4d15515` (2026-08-01)

Derived by reading Hermes's own terminal UI (`ui-tui/`) and gateway (`tui_gateway/`) rather than by
observation. This is reference data, not a decision — nothing here is canon until an ADR promotes it.

**Pin the revision when you re-derive this.** An earlier reading of a six-week-old checkout produced
a materially different and wrong picture; see the engineering journal.

## Why read the client at all

Talaria's recorder attaches and listens, so it observes one direction. A protocol has two. Hermes's
`ui-tui/` is a working client that sends the other half, which makes it the only available
documentation of traffic Talaria cannot yet see.

This is not a small program: 58,581 lines across 277 TypeScript files. The parts that carry protocol
knowledge are much smaller.

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
