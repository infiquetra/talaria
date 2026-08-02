# ADR-0001: Talaria is a standalone client, not a TUI bundle Hermes launches

Status: `accepted`
Date: 2026-08-02
Deciders: operator
Affected components: transports, core lifetime, packaging, language choice

## Context

Two committed documents chose opposite answers to this question, a day apart, and neither recorded
that it was overturning the other.

`docs/analysis/2026-08-01-hermes-tui-project-direction.md:137-155` defines three local workflow
states and calls state 2 "the preferred compatibility mode for UI-only work":

```
HERMES_TUI_DIR=<talaria>/ui-tui hermes --tui
```

`docs/ideation/2026-08-02-talaria-product-shape-ideation.md` chose the opposite relationship three
separate times — survivors 15, 17, and 28 — without engaging the earlier document.

That silence had a measurable cost. Because implementation is delegated to agents, an agent reading
`docs/analysis/` builds toward the bundle; an agent reading `docs/ideation/` builds toward
standalone. Both documents are committed and both look authoritative. The ideation run that produced
this ADR spent a dedicated reconciliation document resolving exactly this after two agents reached
opposite conclusions from real source.

**What the launcher actually does**, verified in the Hermes source rather than inferred:

- `hermes_cli/main.py:1685-1690` accepts only `<HERMES_TUI_DIR>/dist/entry.js` and execs it as
  `node --expose-gc <path>`. The mode admits a Node.js bundle and nothing else.
- `main.py:2046` runs it as a **blocking foreground child** via `subprocess.call`, then exits with
  the child's code.
- The argv is **fixed at three elements**. Everything else travels by environment variable, so
  `talaria --headless --json` is unreachable through that path.
- Exit code 42 is **reserved** by the launcher to mean "the terminal UI requested an update."

One inference that was made during analysis and turned out to be **wrong** is recorded here so it is
not repeated: bundle mode does _not_ restrict Talaria to one profile. `tui_gateway/server.py:678`
(`_profile_home`) is an ungated per-call override used by `session.create` and `session.resume`, so
a bundled client could still open sessions against other profiles. The incompatibility is about
process lifetime, not profile reach.

## Decision

**Talaria is a standalone process.** It owns its own lifetime and dials a Hermes gateway it did not
launch, over WebSocket. It is not built as, and does not aim to be loadable as, a `HERMES_TUI_DIR`
bundle.

This is not a new integration shape. Hermes already serves and tests it: `hermes_cli/web_server.py:11518`
mounts `@app.websocket("/api/ws")` and hands the socket to `tui_gateway.ws.handle_ws`, driving the
same dispatch surface the bundled terminal UI uses over stdio, and `ui-tui/src/gatewayClient.ts:38`
has a `resolveGatewayAttachUrl()` seam with tests covering attach, reconnect, and URL rotation.

## Rejected alternatives

**The bundle relationship (compatibility mode 2).** It buys the cleanest upstream-contribution path
and a zero-install story. It costs three survivors that survived critique: 17 (a headless core with
the terminal as one client — whose own proof obligation, "a headless mode answers the same questions
as JSON," is literally unreachable through a fixed three-element argv), 28 (publish status into
surfaces Talaria does not own, whose premise is that Talaria is _not_ the focused window), and 15 in
weakened form (own panes that outlive Talaria, where the lifetime relationship inverts).

**Both modes, via a headless core with two front ends.** Conceivable, and the core/renderer split
this ADR implies makes it cheaper later. Rejected for now as a load-bearing architecture commitment
taken before a single socket had been opened.

## Consequences

**Easier.** The language field opens — a bundle would have forced TypeScript, and standalone admits
any language that can hold a WebSocket. Process lifetime becomes Talaria's own, which is the
precondition for the headless core and for owning panes. Talaria can hold N gateway connections.

**Harder.** Talaria must ship its own install story rather than inheriting Hermes's. A gateway must
already be running, and the attach path is authenticated (`_ws_auth_ok`, `web_server.py:11146`) and
in practice depends on the Hermes dashboard process.

**Stale as a result.** `README.md` and `AGENTS.md` still commit to upstreamability as a design
constraint. This ADR does not forbid upstream contribution; it removes it as a constraint on
architecture. Those two files are corrected in the same commit.

## Revisit when

A concrete upstream-contribution opportunity appears that requires being loadable by an unmodified
`hermes --tui`, **or** the standalone attach path proves unreliable in daily use — specifically if
requiring a running dashboard turns out to be a worse dependency than being a child process.

## Open, and deliberately not decided here

Whether a session created against profile B through one connection resolves B's model, skills, and
memory or the launch profile's. `tui_gateway/server.py:670-677` claims the per-call override rebinds
them; `hermes_cli/web_server.py:~11181` says a profile-scoped chat must spawn its own gateway
subprocess. If the answer is "the launch profile's," Talaria needs one connection per profile. That
is a half-day probe and it does not block this decision.
