# ADR-0007: Talaria attaches to the Hermes session authority as a viewer

Status: `accepted`
Date: 2026-10-22
Accepted: 2026-10-23
Deciders: operator
Supersedes: the transport half of ADR-0001 (the per-profile gateway fleet and dashboard token scraping)
Affected components: transport, connection management, credentials, the fleet view
Evidence revision: Hermes Agent `22063fc09` (2026-10-17); live probe 2026-10-22

## Context

ADR-0001 settled that Talaria is a standalone client dialing a gateway it did not launch. That still
holds. What it dialled has changed underneath it. In August, the only WebSocket was inside a
`hermes dashboard` process, each profile ran its own gateway, and the client had to scrape a session
token out of the dashboard's HTML. Talaria v0.4 built a fleet of per-profile connections on top of
that topology.

Since Hermes v0.22.3 (2026-10-21), one supervised gateway per machine owns every session of every
profile. Every first-party client attaches to it as a viewer over a WebSocket speaking
`hermes-gateway-v1`, authenticated with a single-use ticket minted on the owner-only Unix socket
`$HERMES_HOME/gateway.sock`. Sessions are durable admissions that run with no client attached; events
are sequenced and replayable after a reconnect. The standalone dashboard socket Talaria uses today has
no session authority, and Hermes now withdraws approval prompts from clients that do not declare they
can answer them.

A probe on 2026-10-22 verified the attach path end to end from a third-party process: identify, ticket,
upgrade, `gateway.ready`, `runtime.describe`, `session.list`, `model.options`, `profiles.list`, with a
warm attach of about 30 ms.

## Decision

1. Talaria finds the gateway with the control socket's `identify` verb (or `hermes gateway ensure
   --json`), mints a ticket with `session-ticket` (purpose `interactive`, scope `host` when the fleet
   view needs every profile), and dials `<api_origin>/api/ws` with the subprotocols
   `hermes-gateway-v1` and `hermes-gateway-ticket.<ticket>`.
2. One connection per Talaria process. A second connection is an exception path (for example a remote
   machine), not the architecture.
3. Talaria sends `client.capabilities {server_requests: true}` on every connection. It answers every
   server request at once, so none waits for a timeout. Approvals, clarifications, sudo prompts and
   secret prompts get real answers. `vault.code` and `vault.unlock_prompt` get the masked-input card.
   Every other request in the `x-server-requests` list of the gateway contract gets a JSON-RPC error
   (the standard reply for a failed call) that says this client does not support it. The client shows
   one notice per connection.
4. Dashboard REST is used only for what has no RPC (the config schema, `.env`), authenticated with the
   gateway-ticket header.
5. Remote machines are a later, separate decision (the gated dashboard with the native-app sign-in
   flow), taken after an end-to-end test.

## Rejected alternatives

- **Keep the standalone dashboard socket.** No session authority, no durable admissions, approvals
  withdrawn, and upstream is routing around it.
- **Spawn the standard-input/output gateway.** Hermes refuses it while a gateway owns the profile.
- **The Agent Client Protocol (`hermes acp`).** Works, but loses subagent control, profiles,
  configuration, cron, kanban and queue editing. Kept as a possible adapter for non-Hermes agents.
- **The OpenAI-compatible HTTP API server.** Good for scripted runs; too thin for an interactive client.

## Consequences

- The per-profile connection set, the `[profiles.endpoints]` map, token scraping and the startup seam
  probe are deleted, not ported.
- "Always running" comes for free: closing Talaria never stops work.
- Talaria depends on a private control-socket verb that Hermes documents only in code. Mitigation: pin
  the Hermes release (v0.22.3 or later), test the attach path in continuous integration against that
  release, and propose a documented bootstrap command upstream.

## Revisit when

- Hermes documents a different supported bootstrap for third-party clients.
- Talaria needs to drive a gateway on another machine as a daily path.
