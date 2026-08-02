# Learnings - talaria

> Empirical findings, mechanisms, fixes, validations, and generalizable rules. Keep newest entries first.

## 2026-08-02

### The gateway carries plaintext credentials on ordinary frames, so the recorder needed its deny-set on day one

**Author.** ADR-backlog ideation run

**Context.** Three separate analyses independently recommended building a raw protocol-frame recorder first, because a recorded corpus is language-neutral and settles the renderer question by measurement instead of argument. That recommendation was right, and following it naively would have written credentials to disk.

**Evidence.** From `tui_gateway/server.py`, read directly:

```
@method("sudo.respond")   -> _respond(rid, params, "password")
@method("secret.respond") -> _respond(rid, params, "value")
```

The plaintext sudo password arrives as `params["password"]` on an ordinary client-to-server JSON-RPC frame — the same connection a recorder captures. The request side is safe: `sudo.request` is emitted with an empty payload, so the exposure is entirely in the direction Talaria _writes_, which is exactly the direction "fire and observe" (survivor 7) requires Talaria to record. A grep of `tui_gateway/*.py` found no existing frame logging anywhere: Talaria's recorder would be the first thing that writes these frames to disk.

**Mechanism.** The danger compounds with the planned append-only hash-chained ledger (survivor 4). **A hash chain cannot be redacted — that is the entire point of a hash chain.** Removing a record afterwards either breaks verification for every record after it or requires re-chaining the tail, destroying the tamper-evidence that was the reason to chain. So the decision about what may be written is made once, at the first write, permanently. The window closes at the first session in which the operator answers a sudo prompt.

**Fix.** The redaction boundary was written before the recorder and before the socket client — `src/record/redact.ts`, with an explicit deny-set keyed by method plus a key-name net (`password|secret|token|credential|api_key|...`) that catches credentials on methods the deny-set has never heard of. Withheld values are recorded as first-class `redactions` entries with a path and a reason, so a reader sees a marked hole rather than clean-looking data. The connection URL's query token is stripped too. Format contract in [docs/formats/frame-log.md](../formats/frame-log.md).

**Validation.** Fifteen unit tests in `src/record/redact.test.ts`, plus an end-to-end run against a synthetic gateway emitting four distinct credential canaries — a `sudo.respond` password, a `secret.respond` value, an `api_key` on an unknown method, and a token in the connection URL. All four were absent from the resulting corpus; 7 frames recorded, 3 values withheld, 1 unparseable frame recorded as a hole.

**Generalizable rule.** Before recording a protocol, read what the protocol carries in the direction you will be writing. Build the redaction boundary before the thing that writes, not after — for append-only or tamper-evident storage there is no "clean it up later," only "delete it all."

### "The gateway" is three processes, and the one you want depends on what you are asking for

**Author.** First full-product ideation run; corrected the same day by the ADR-backlog run.

**Correction.** This entry was first published saying **two** processes. That was wrong, and the error mattered: it under-applied its own generalizable rule by stopping at the first two it found. There are three, and the third is the one that carries the standalone integration path.

**Context.** The project direction assumed a phase that adds a typed Kanban adapter behind the TUI gateway. That phase would not have worked, and the failure would only have surfaced at implementation time.

**Evidence.** All verified directly against the Hermes fork on 2026-08-02.

1. `tui_gateway/server.py` — the terminal gateway. `grep -c -i kanban` returns `0`, and it has zero `from gateway` or `import gateway` statements. It speaks the session and conversation protocol and knows nothing about the board.
2. `gateway/run.py:2409` — `class GatewayRunner(GatewayAuthorizationMixin, GatewayKanbanWatchersMixin, GatewaySlashCommandsMixin)`, which starts the Kanban dispatcher watcher (gated by `kanban.dispatch_in_gateway`) and hosts the HTTP API platform adapter.
3. `hermes_cli/web_server.py` — the dashboard, started by `hermes dashboard`. It mounts `@app.websocket("/api/ws")` at line 11518 and hands the socket to `handle_ws` from `tui_gateway.ws` **inside its own process**, gated by `_ws_auth_ok` (line 11146) and `_ws_request_is_allowed`.

**Mechanism.** These are separate programs that share a name, not layers of one server. Connecting to the terminal gateway yields exactly zero board visibility no matter what the client asks for. And the "attach to a gateway you did not launch" path that [ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) depends on lives in the **third** process — so it carries a dependency the first two do not: the dashboard must be running, and the connection is authenticated.

**And there is no capability endpoint to ask.** The product-shape ideation's survivor 3 cites `/v1/capabilities` as evidence that per-feature booleans are already published. That endpoint is real but it is on the **API server** — `gateway/platforms/api_server.py:4252` registers `GET /v1/capabilities`, inside `GatewayRunner`. A grep of `tui_gateway/*.py` for `capabilit` returns exactly one hit, a keyword argument at `server.py:9562`, not an endpoint. **The terminal gateway publishes no capability surface at all**, so its capabilities cannot be asked for and must be inferred by probing. This is the same error one process further along: evidence gathered from one program, attributed to another.

**Fix.** Talaria names the seams separately in its own code and probes each independently at launch, rather than modelling "gateway connected" as one boolean. Sized against the original two-process version, survivor 3's probe set was one seam short; the real set is at least four, counting the pane manager's Unix socket. And the probe cannot rely on a capability endpoint for the terminal gateway, because there isn't one.

**Validation.** Every claim above re-verified by direct read on 2026-08-02. A separate live process probe found the terminal gateway and `GatewayRunner` both absent while four dashboards were running — which is exactly the case that makes the distinction matter: a client modelling one boolean renders an empty board as _zero work_ when the truth is _no dispatcher running_.

**Generalizable rule.** Before building an adapter behind a service, confirm the service actually imports the subsystem you intend to reach through it. A shared name is not a shared process — and having found two, keep counting.

## 2026-08-01

### Hermes already exposes two useful client boundaries

**Author.** Project bootstrap

**Context.** The project needed to decide whether a new TUI should call only an OpenAI-compatible endpoint, import Hermes internals, or use the existing Hermes TUI architecture.

**Evidence.** Public Hermes source areas include `ui-tui/`, `tui_gateway/`, and `gateway/platforms/api_server.py`.

**Mechanism.** The API server is the better portable run/session boundary, while the TUI gateway carries richer Hermes-native control-plane behavior. They solve different integration problems.

**Fix.** Talaria uses an API-first direction with an optional gateway adapter and typed adapters for non-chat surfaces.

**Validation.** The direction is documented in [the project analysis](../analysis/2026-08-01-hermes-tui-project-direction.md), with the initial prototype checks passing once dependencies are installed.

**Generalizable rule.** Do not mistake a generic chat protocol for a complete agent control-plane contract.
