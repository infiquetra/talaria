# Learnings - talaria

> Empirical findings, mechanisms, fixes, validations, and generalizable rules. Keep newest entries first.

## 2026-08-02

### "The gateway" is two processes, and Kanban lives in the other one

**Author.** First full-product ideation run

**Context.** The project direction assumed a phase that adds a typed Kanban adapter behind the TUI gateway. That phase would not have worked, and the failure would only have surfaced at implementation time.

**Evidence.** In the public Hermes tree, `grep -c -i kanban tui_gateway/server.py` returns `0`, and the same file has zero `from gateway` or `import gateway` statements. The Kanban dispatcher belongs to a different process: `gateway/run.py:2409` declares `class GatewayRunner(GatewayAuthorizationMixin, GatewayKanbanWatchersMixin, GatewaySlashCommandsMixin)`, and that class starts the dispatcher watcher task, gated by `kanban.dispatch_in_gateway`.

**Mechanism.** The TUI gateway and the API gateway are separate programs that share a name, not layers of one server. The TUI gateway speaks the session and conversation protocol; the Kanban dispatcher and the HTTP API adapter live in `GatewayRunner`. Connecting a client to the TUI gateway therefore yields exactly zero board visibility, no matter what the client asks for.

**Fix.** Talaria's own code names the two seams separately from the start and probes each independently at launch, rather than modelling "gateway connected" as one boolean. This is survivor 3 in [the product-shape ideation](../ideation/2026-08-02-talaria-product-shape-ideation.md).

**Validation.** Verified directly against the Hermes fork on 2026-08-02 by the greps above. A separate live process probe found neither gateway running, which is the case that makes the distinction matter: a client modelling one boolean renders an empty board as _zero work_ when the truth is _no dispatcher running_.

**Generalizable rule.** Before building an adapter behind a service, confirm the service actually imports the subsystem you intend to reach through it. A shared name is not a shared process.

## 2026-08-01

### Hermes already exposes two useful client boundaries

**Author.** Project bootstrap

**Context.** The project needed to decide whether a new TUI should call only an OpenAI-compatible endpoint, import Hermes internals, or use the existing Hermes TUI architecture.

**Evidence.** Public Hermes source areas include `ui-tui/`, `tui_gateway/`, and `gateway/platforms/api_server.py`.

**Mechanism.** The API server is the better portable run/session boundary, while the TUI gateway carries richer Hermes-native control-plane behavior. They solve different integration problems.

**Fix.** Talaria uses an API-first direction with an optional gateway adapter and typed adapters for non-chat surfaces.

**Validation.** The direction is documented in [the project analysis](../analysis/2026-08-01-hermes-tui-project-direction.md), with the initial prototype checks passing once dependencies are installed.

**Generalizable rule.** Do not mistake a generic chat protocol for a complete agent control-plane contract.
