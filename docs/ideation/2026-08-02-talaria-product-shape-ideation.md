---
date: 2026-08-02
topic: talaria-product-shape
focus: what Talaria should become — first full-product ideation
scope: broad
repo: talaria
maturity: idea-ready
---

# Ideation: What Talaria Should Become

First ideation run on the whole product. Talaria is currently a small Ink prototype; this document
records what it should grow into, which ideas were rejected, and why.

Eighty-five candidates were generated across six frames. Twenty-eight survived. The survivor count
is deliberately far above the usual five to seven because this is the first pass over the entire
product surface — an MVP gets carved from the top section and the rest are sequenced into later
phases. The bar was not lowered: fifty-seven candidates were cut, and every cut is listed below
with its reason and the condition under which it comes back.

**Public-safe note.** This repository is public. The working ideation record contains live probes
of a local Hermes install — profile names, local ports, local file paths, and machine-specific
measurements. Those are generalized here. Citations to Hermes Agent source are retained as-is
because that repository is public. Nothing load-bearing was dropped; instances were replaced with
the claim they support.

## Grounding Context

**Repo:** Talaria is a public, experimental, Hermes-native terminal UI at prototype stage — a
roughly 26-line Ink application. Its stated purpose is to own the client-side terminal experience,
transport abstractions, and UI state while explicitly not owning Hermes core. The prior design
conversation is recorded in `docs/analysis/2026-08-01-hermes-tui-project-direction.md`, which
established the central architectural decision: bind to the JSON-RPC gateway protocol rather than
calling agent implementation code directly, because the legacy client path is tightly coupled to
Hermes internals and copying that coupling recreates the maintenance problem the project exists to
escape. The engineering journal was empty of relevant prior decisions — this is genuinely the first
pass.

**Named repos:** Hermes Agent (public upstream) supplied the bulk of the direct evidence — the
gateway protocol surface and its roughly ninety JSON-RPC methods, the Ink TUI package and its
hand-written layout engine, the delegation tool and its hard limits, the Kanban subsystem, and the
plugin hook surface. A terminal workspace and pane manager the operator already uses in daily work,
driven over a local socket API, supplied the integration constraints.

**Context-libraries:** None consulted — the topic is client architecture and does not touch an
organization standards library.

## Topic Axes

- **A1 — Fleet observability and steering.** What are my agents doing right now, and how do I
  intervene without losing their context?
- **A2 — Profile and identity.** Which agent am I talking to, and how does the client know?
- **A3 — Work orchestration and routing.** How does a goal become a routed, gated graph of agent
  work with durable evidence?
- **A4 — Single-session craft.** Is the place I live for hours calm, legible, and powerful?
- **A5 — Integration architecture and transports.** Which seams does Talaria stand on, and what
  happens when one is missing?

Axis A2 was drawn too wide when this run began — it originally read "how do I see, compare, diff,
and keep honest dozens of agent identities," which smuggled in an agent-administration scope the
operator later ruled out. Constraint 7 below narrowed it to session handling, and its survivor
count fell from five candidates to one. That collapse is intended, not a coverage gap.

## Constraints Applied

Constraints 1 through 6 were established during seed capture. Constraint 7 was added mid-run by the
operator and applied retroactively as a hard filter.

1. Talaria drives the pane manager; the pane manager stays sole authority for workspace and agent
   lifecycle.
2. Own harness first — upstreamability is not a constraint on design.
3. Scope is one Hermes install and all of its profiles. Not multi-host.
4. Surface existing Hermes machinery before building parallel machinery.
5. Never run a second Kanban dispatcher.
6. Tier routing resolves to a profile name, because model and effort are pre-bound per profile.
7. **Talaria reads agent state; it does not author agent identity.** Profile creation, generation,
   editing, pruning, rollback, and config writes belong outside the TUI. Talaria may select a
   profile, show which profile a number came from, and aggregate work across profiles — that is a
   client knowing what it is connected to, not managing a population.

## Two Open Boundary Questions

These are unsettled and each one moves a material part of the survivor set.

**Q1 — Does answering a blocked agent's question count as driving the agent, or as authoring it?**
Assumed _driving_ throughout this document, in the same category as steering a session or typing a
message. If it is ruled authoring, survivor 1 collapses and takes four others with it.

**Q2 — May Talaria ship Hermes-side plugin code?** Survivor 25 and cut R10 both require it. This is
arguably a larger decoupling violation than any read-only profile view, and it was not in scope when
constraint 7 was stated.

---

## Ranked Survivors

### MVP — proves the seam and is useful on day one

#### 1. One typed queue for everything that needs a person

Collapse every "an agent is waiting on a human" signal in the stack into a single flat list with a
typed answer per row.

Define one `PendingHumanAction` type — reference, source, prompt, and the set of allowed responses —
before building any pane that displays a blocked thing. Seven distinct signals across three systems
fold into one list ordered by wait time: the gateway's five respond methods (approval, clarify,
sudo, secret, terminal-read), Kanban's blocked state, and the pane manager's blocked state.

Five of six frames converged here independently, which is the strongest signal in the run. It is
also the industry's named gap: every permission model in the prior-art sweep gates approval locally
inside the calling session, while Hermes routes all five respond methods through the gateway
centrally — which makes this cheaper to build here than anywhere else. A live probe found several
tasks blocked across different profiles with no terminal surface anywhere reporting it.

The downside is dependency on Q1. If answering a blocked agent is ruled authoring rather than
driving, this idea does not survive in any form.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                     |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` all five respond methods are gateway-central, not session-local. `direct:` live board probe showed multiple blocked tasks across profiles with no terminal surface. `external:` prior-art sweep names central approval as unclaimed opening #3. `external:` Agent Inbox is deliberately flat because at decision time a human wants a stack of yes/no/edit calls, not topology. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                  |
| confidence | 85                                                                                                                                                                                                                                                                                                                                                                                        |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                       |
| axis       | A1                                                                                                                                                                                                                                                                                                                                                                                        |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                |

#### 2. Consume the telemetry exporter that is already running

The fleet feed already exists and terminates in Prometheus instead of in a terminal. One HTTP GET is
the fleet header.

A Kanban telemetry exporter ships with the install and runs continuously, publishing blocked-task
and active-worker counts labeled by profile, plus an epic wall-clock metric computed over the task
dependency graph, refreshed every few seconds.

This is the cheapest possible real fleet number: no adapter, no SQLite ownership, no lock contention
with the live board. A read-only HTTP GET structurally cannot violate constraint 5. It is pure
constraint-4 compliance — surfacing machinery that already exists rather than building new
machinery.

The downside is that it inherits the exporter's refresh interval and label set, so it cannot answer
anything the exporter does not already publish.

| field      | value                                                                                                                                                                                                                      |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` live probe of the exporter's metrics endpoint confirms per-profile blocked and active-worker series. `direct:` process probe confirms it is running against the live board database on a short refresh interval. |
| source     | frame-agent                                                                                                                                                                                                                |
| confidence | 90                                                                                                                                                                                                                         |
| complexity | Low                                                                                                                                                                                                                        |
| axis       | A1                                                                                                                                                                                                                         |
| status     | Unexplored                                                                                                                                                                                                                 |

#### 3. Probe every seam at startup and name what is missing

Render a persistent banner at launch naming exactly which seams are absent and which features are
therefore off.

The critical finding: **"the gateway" is two processes.** The TUI gateway module has zero Kanban
references and does not import the gateway package at all; the Kanban dispatcher lives in
`GatewayRunner` (`gateway/run.py`) alongside the HTTP API adapter. Connecting to the TUI gateway
yields zero board visibility. Talaria's own code must name them separately from day one.

This kills a phase that was already planned. The direction document's "add a Kanban adapter behind
the gateway" step would have discovered the split at implementation time instead of now. Without
this probe a client models "gateway connected" as one boolean and renders an empty board as _zero
work_ when the truth is _no dispatcher running_ — a fabricated zero, which is the failure mode the
project's own validation rules forbid.

The downside is startup latency and banner noise if the probe set grows without discipline.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` the TUI gateway server module contains no Kanban references and no import of the gateway package. `direct:` the Kanban dispatcher is constructed in `GatewayRunner` in `gateway/run.py`. `direct:` a live process probe found no `GatewayRunner` running at all, so the HTTP API was not serving and the dispatcher was not ticking. `direct:` the capabilities endpoint already publishes per-feature booleans, several of them false. `direct:` `AGENTS.md:41-46` already requires capability gating. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| confidence | 92                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| complexity | Low                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| axis       | A5                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |

#### 4. An append-only local event log as Talaria's first write

Write every normalized event observed — gateway, Kanban watch, pane-manager subscription — to one
append-only, hash-chained local log. Nothing pre-aggregated; every rollup derived on read.

Hermes has already conceded that durability here is the client's job. The delegation event types for
task-spawned, completed, and failed are defined but documented as reserved and not currently
emitted; completed subagents vanish from the active-subagent listing; there is no server-side
durable history; and the existing spawn-tree save call is already the _client_ persisting a tree it
assembled itself. Meanwhile subagent completion events already carry token and cost rollups, files
read and written, and an output tail — only retention is missing.

This is the flywheel: the longer it runs, the more it can answer, with no further Hermes work. It is
also the precondition for the cost half of the telemetry seed, because a burn number that vanishes
when the child finishes is a gauge that resets at the end of the trip, not telemetry. Render "gap:
not observed 14:02–14:31" rather than a clean-looking hole.

The downside is that it is Talaria's first durable state, and durable state is the thing that later
disagrees with reality.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` `DelegateEvent` spawn/complete/fail members are defined but marked reserved and not emitted. `direct:` completed subagents disappear from the active listing; no server-side history exists. `direct:` `spawn_tree.save` is already a client-side persistence call. `direct:` subagent completion events already carry cost, file, and output-tail payloads. `external:` the saga plugin's run-fact ledger is exactly this shape, with chain verification catching mutation, reorder, and deletion. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| confidence | 88                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| axis       | A1                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                    |

#### 5. Make the session registry, not the session, the root object

Core state is a registry keyed by profile and session id over one long-lived gateway connection.
Profile becomes a per-session selector rather than a process-level identity.

The mechanism is already shipped and unused by any terminal client. App-global remote mode lets one
connection create and resume sessions against any profile, with the Hermes home directory rebound
per turn; `session.create` and `session.resume` both take a profile parameter. The CLI structurally
cannot do this — the profile flag is intercepted before argument parsing and sets the home directory
process-wide.

This is selecting which agent to talk to, not managing a population, so it is clean under constraint 7. It is free now and unrecoverable later: model one session as the root object and every
multi-profile feature afterwards requires unwinding the state tree.

The downside is that a registry is more machinery than a single-session prototype needs on day one,
and the payoff is entirely in options it preserves rather than features it delivers.

| field      | value                                                                                                                                                                                                                                                                                                                                                                |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` app-global remote mode in the gateway server module. `direct:` both session create and resume accept a profile parameter resolved through a profile-home lookup, with the home directory rebound per turn. `direct:` the CLI intercepts the profile flag before argparse and sets the home directory process-wide, so it cannot hold two profiles at once. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                             |
| confidence | 85                                                                                                                                                                                                                                                                                                                                                                   |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                  |
| axis       | A2                                                                                                                                                                                                                                                                                                                                                                   |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                           |

#### 6. Never render stale as fresh; every value carries source and age

Every count, row, and cell carries the timestamp of the last event that changed it and the source it
came from. If a source drops, the display says "stale since 14:02" — never a frozen number, never a
fabricated zero.

The board pane is driven by one long-lived watch subscription plus a snapshot at attach, never a CLI
poll on a timer. Each CLI invocation costs a full Python process start, so a one-second refresh is
one interpreter launch per second per pane.

The likely failure of a fleet console is not crashing. It is confidently rendering a number nobody
is updating. Provenance cannot be retrofitted into a display people have already learned to trust.

The downside is visual weight: every cell carrying age and source is more chrome than a clean
dashboard, and the discipline has to hold everywhere or it is worthless.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| basis      | `direct:` each Kanban CLI invocation costs a full interpreter start; `kanban watch --kinds` already includes completed, blocked, gave-up, crashed, and timed-out. `direct:` a recorded prior failure in the operator's own tooling — a scene controller was retired because "every extra guarantee it added was another way to disagree with live state." `external:` saga's fleet-doctor reserves a distinct exit code for incomplete proof so incomplete can never be reported as clean. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| confidence | 88                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| axis       | A5                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 |

#### 7. Fire and observe — no optimistic updates, ever

No user action updates Talaria's model directly. Every mutation sends the call, renders an explicit
_requested_ state with a visible age, and changes displayed state only when the corresponding event
arrives or the reference re-resolves.

Three outcomes, not two: acknowledged, delivered-but-not-yet-observed, and never delivered. The
middle state must be earned by observed behaviour change, not by a transport acknowledgment.

This matters because subagent interrupt is soft and cooperative — a flag checked at iteration
boundaries that cannot hard-kill. Right now "I pressed stop and nothing happened" is
indistinguishable from "I pressed stop and it hasn't reached a boundary yet." Only the second is an
acceptable thing to show someone.

The downside is that it must be set before the first mutating verb ships; it cannot be adopted
halfway, and it makes every action feel slower than an optimistic UI would.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` `subagent.interrupt` sets a cooperative flag checked at iteration boundaries and cannot hard-kill. `direct:` the embedded dispatcher ticks on its own interval regardless of the operator, and pane-manager state changes when any pane anywhere does. `external:` restaurant expediting requires an explicit verbal "heard" so silence and receipt stay distinguishable — and it degrades exactly when the acknowledgment becomes reflexive. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| confidence | 85                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                                                     |
| axis       | A1                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                                              |

### Phase 2 — the harness starts to exist

#### 8. Spawn work as sessions, not as delegated subagents

Make the delegation primitive "another session in another pane" rather than a `delegate_task` child.
This is the single highest-leverage reframe in the run.

The operator's ask — reach a running teammate's session the way you reach the main one — already
exists for _sessions_ and is structurally unavailable for _delegated children_. `session.steer` is
already a gateway method. Steering a subagent is not available at all. And `DELEGATE_BLOCKED_TOOLS`
strips clarify from every child, so a delegated child structurally cannot ask a question, which
means it can never participate in survivor 1's inbox.

This converts the run's biggest named weakness from "needs upstream work" into "use the other
existing verb." Every spawned unit becomes addressable, resumable, tailable, and answerable on day
one. Because app-global remote mode exists, session-per-unit does not mean process-per-unit.

The downside is that it walks away from the delegation subsystem entirely, including whatever
Hermes builds there later, and sessions carry more weight per unit of work than subagents do.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` `session.steer` exists as a gateway method; no subagent steering method exists. `direct:` `DELEGATE_BLOCKED_TOOLS` strips delegate, clarify, memory, send-message, and execute-code from every child. `direct:` `MAX_DEPTH = 1`, at most three concurrent children, and delegation pause is process-global. `direct:` app-global remote mode decouples session count from process count. `direct:` the operator's own seed already said "your main session is creating, updating, and deleting sessions" — the same conclusion reached by intuition. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| confidence | 82                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| axis       | A3                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     |

#### 9. Three named steering verbs, and make redirect possible

Bind queue, steer, and interrupt distinctly, and apply all three to both the main session and any
running child.

Queue lands after the current turn. Steer injects at the next safe boundary. Interrupt stops now.
Anything with process-global blast radius states that before firing — delegation pause is
process-global, so calming one session silently stops spawning for all of them.

The prior art is settled here: opencode names the three explicitly, Amp binds them to graduated
urgency, and Devin's steer command appends to the last tool result. Claude Code is the
counter-example, where escape is destructive and the non-destructive priority interrupt is a known
unresolved request.

The downside is keybinding budget in a terminal that already has few free chords.

| field      | value                                                                                                                                                                                                                                                                                                                                                                   |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `external:` opencode names queue, steer, and interrupt explicitly; Amp binds them to graduated urgency; Devin's steer appends to the last tool result. `external:` Claude Code's escape is destructive and its non-destructive interrupt is an open request. `direct:` delegation pause is process-global, so it affects sessions the operator did not intend to touch. |
| source     | frame-agent                                                                                                                                                                                                                                                                                                                                                             |
| confidence | 80                                                                                                                                                                                                                                                                                                                                                                      |
| complexity | Low                                                                                                                                                                                                                                                                                                                                                                     |
| axis       | A1                                                                                                                                                                                                                                                                                                                                                                      |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                              |

#### 10. Own the join key; resolve everything else live

One reference type — source, kind, identifier — with the rule that Talaria stores only the reference
and resolves live. One exception, and it is Talaria's only durable relational asset.

That exception is the join: which Hermes session runs in which pane, under which profile, working
which Kanban task, in which worktree. Written at spawn time, treated as a continuously re-validated
_claim_, and rendered as unknown rather than as fact when validation fails.

No authority holds this relationship. The pane manager stores an opaque agent-session value it never
reads and has no notion of task relationships. So recording it does not duplicate an authority's
state — which is the non-paralysing reading of constraint 1, and the precondition for nearly every
other fleet feature. It makes "drive the pane manager, don't mirror it" a type rather than a
discipline.

The downside is that a claim which silently goes wrong is worse than no claim, so the re-validation
path has to be as well built as the write path.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                               |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` the pane manager's agent-session field is an opaque value it never interprets, and it models no task relationships at all. `direct:` a recorded prior failure — resume metadata proved identity, not client liveness, so a pane can carry correct identity while running a bare shell. `reasoned:` constraint 1 forbids duplicating an authority's state; it does not forbid recording a relationship no authority holds. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                            |
| confidence | 78                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| axis       | A5                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                          |

#### 11. Adopt Kanban as the workflow engine of record; saga concepts become projections

An outcome is a board or tagged subgraph, a leaf is a task, dependencies are Kanban links, the
backend is the assigned profile, a human gate is a Kanban block, and evidence is task comments and
run rows. Talaria owns no dependency store, no scheduler, and no second dispatcher.

The central realization of the run: the harness the operator wants substantially already exists
inside Hermes as the Kanban subsystem, and it is headless. Kanban self-describes as a durable
SQLite-backed task board shared across profiles, where tasks are claimed atomically, can depend on
other tasks, and are executed by a named profile in an isolated workspace. It already has link and
unlink, atomic claim, reassign and reclaim, model assignment, block and unblock and promote, a
dry-run dispatch, and a swarm command taking workers, a verifier, and a synthesizer.

The seed asked to generalize the operator's plugin suite into the product. The compounding version
is noticing that saga's outcome graph and Kanban's task graph are the same shape, and Kanban's is
durable, atomic, multi-profile, and already dispatching. Saga's outputs get read, never
reimplemented.

The downside is a large one-way commitment: a parallel graph store would mean owning a second
scheduler forever, but adopting Kanban means Talaria's workflow model is bounded by Kanban's.

| field      | value                                                                                                                                                                                                                                                                                                                                                                    |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| basis      | `direct:` Kanban's own description of itself as a durable cross-profile board with atomic claims and task dependencies. `direct:` the existing command surface covers link, claim, reassign, set-model, block, promote, dry-run dispatch, and swarm. `direct:` saga derives maturity from directory location and never stores it as state; its ledger is already a file. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                 |
| confidence | 75                                                                                                                                                                                                                                                                                                                                                                       |
| complexity | High                                                                                                                                                                                                                                                                                                                                                                     |
| axis       | A3                                                                                                                                                                                                                                                                                                                                                                       |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                               |

#### 12. Talaria never dispatches; it edits the board and shows the next tick

Every operator action is a state edit. A permanently visible pane runs a dry-run dispatch with a
countdown showing exactly what the dispatcher will do on its next tick.

In-gateway dispatch is on by default on a fixed tick, guarded by a cross-process advisory lock
because concurrent dispatchers double-reclaim and corrupt database index pages. The dry-run command
already exists with max and failure-limit flags.

This turns the load-bearing single-dispatcher hazard into a visible feature, and removes the "did my
keystroke do anything" gap using a command that already ships.

The downside is that the operator sees intent rather than action, which feels indirect until the
tick lands.

| field      | value                                                                                                                                                                                                                                                     |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` in-gateway dispatch defaults on with a fixed tick interval and a cross-process advisory lock documented as preventing double-reclaim and index-page corruption. `direct:` `dispatch --dry-run` with max and failure-limit flags already exists. |
| source     | frame-agent                                                                                                                                                                                                                                               |
| confidence | 82                                                                                                                                                                                                                                                        |
| complexity | Low                                                                                                                                                                                                                                                       |
| axis       | A3                                                                                                                                                                                                                                                        |
| status     | Unexplored                                                                                                                                                                                                                                                |

#### 13. Budget attention; nominal agents get a count, not a row

Agents running nominally collapse into a count. Only blocked, stalled-past-heartbeat, or asking
agents get a row. The top tier is perceptually distinct rather than more frequent, and volume is
suppressed at the source rather than formatted better.

Includes an explicit operator-set "not attending this" category that stops an agent consuming
attention budget at all.

The evidence for this is unusually strong across domains. Between 72 and 99 percent of clinical
monitor alarms are false or non-actionable; ninety-eight alarm-related adverse events were logged
over a three-year period, eighty of them fatal; and per-case threshold customization cut
high-priority alarms by 43 percent. Span of control for knowledge work runs five to ten, with seven
to eight the sweet spot, and air traffic control caps a sector at eighteen aircraft counting only
those on frequency. Teams converge on three to five concurrent agents because review bandwidth binds
first — a console that makes twenty agents visible does not make twenty agents reviewable. The live
board bears this out: a handful of blocked tasks inside a population of dozens.

The downside, and the reason it must be a rule from the start: retrofitting quiet onto a noisy
channel does not work, because by then the channel is already discredited.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                           |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` live board probe — a small handful of blocked tasks against a much larger completed population across dozens of profiles. `external:` clinical alarm-fatigue literature on false-alarm rates, adverse events, and the measured effect of per-case thresholds. `external:` span-of-control research and the FAA sector cap. `external:` observed convergence on three to five concurrent agents in agent-team tooling. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                        |
| confidence | 88                                                                                                                                                                                                                                                                                                                                                                                                                              |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                             |
| axis       | A1                                                                                                                                                                                                                                                                                                                                                                                                                              |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                      |

#### 14. Conversation is the shell; one exception strip; the fleet is a drill-down

Keep conversation primary at full width. Permanent screen space goes only to an exception count —
how many need you, how many stalled, how many blocked — with no per-agent rows at rest. The full
cockpit is a drill-down reached by a key and dismissed with escape.

This is the run's answer to the layout question the operator explicitly asked the frames to argue.
The observed pain is that the board is invisible unless you open a separate browser window, which
makes the friction _discovery_, not detail. A mode switch does not fix "I didn't know" — you have to
already suspect something is wrong to enter it. Only an always-visible exception signal changes the
class of the problem.

The prior art supports the drill-down: k9s is a drill-down stack with a command palette, enter to
drill, escape to back out, and contextual hotkeys; Temporal and Argo keep the graph primary and
surface workers on drill-down rather than unifying board and roster visually.

**This is only viable if survivor 13 holds.** An exception strip that fills with routine events is
just a second mode with worse ergonomics.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` the operator's stated pain is that the board requires a separate browser window to see at all. `direct:` live board probe — only a few items needed attention; everything else was nominal. `external:` k9s drill-down stack conventions. `external:` Temporal and Argo keep graph primary with workers on drill-down. `reasoned:` a mode switch cannot fix a discovery problem, because entering the mode requires already suspecting the problem. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| confidence | 72                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| axis       | A4                                                                                                                                                                                                                                                                                                                                                                                                                                                            |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                                                    |

#### 15. Own the launch spec; drive the pane manager's socket, not its built-in launcher

Compose the pane manager's pane, worktree, and workspace verbs with launch specifications Talaria
owns rather than using its built-in agent integrations.

Agent-name slug and human-readable workspace label stay separate typed fields. Pane presence is
never read as agent liveness. A claimed Kanban task materializes a pane in a worktree-backed
workspace and is torn down at terminal state.

Three recorded prior failures drive this. The built-in integrations invoke the plain native binary
with no wrapper hook, so flag translation has to be redone per agent kind — a problem that was hit
twice. Conflating the agent slug with the workspace label caused a real bug. And resume metadata
proved identity rather than liveness. Separately, the layout verbs are socket-only with no CLI
equivalent, so driving the socket is strictly more capable than driving the binary.

The downside is a deeper coupling to the socket protocol than the CLI would require.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                                                 |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` three recorded failures in the operator's prior tooling — per-agent-kind flag translation hit twice, slug-versus-label conflation causing a real bug, and resume metadata proving identity rather than liveness. `direct:` the layout apply, export, and split-ratio verbs are socket-only with no CLI surface. `external:` one worktree per agent is the universal isolation primitive across the entire prior-art survey. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                                              |
| confidence | 80                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| axis       | A5                                                                                                                                                                                                                                                                                                                                                                                                                                    |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                                            |

#### 16. Generate the pane-manager client from the pane manager's own schema

The pane manager publishes a JSON Schema description of its full method surface. Generate the
bindings from it and verify the protocol version at connect, so a protocol bump becomes
regenerate-and-typecheck rather than a runtime error in front of the operator.

**One caveat carried forward honestly:** frame 3 could not independently reproduce the full method
enumeration — its probe returned a small set of envelope schemas rather than a method table. The
generation approach survives; the specific method count needs one confirming probe before it is
quoted as fact anywhere.

The downside is a build-time code generation step in a project that currently has none.

| field      | value                                                                                                                                                                                                                                     |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` the pane manager exposes a JSON Schema draft 2020-12 description of its API and a protocol version at connect. `direct:` a second independent probe did not reproduce the full method enumeration, so the count is unconfirmed. |
| source     | frame-agent                                                                                                                                                                                                                               |
| confidence | 70                                                                                                                                                                                                                                        |
| complexity | Low                                                                                                                                                                                                                                       |
| axis       | A5                                                                                                                                                                                                                                        |
| status     | Unexplored                                                                                                                                                                                                                                |

#### 17. Split a headless core from the terminal

A core module owns transports, the ledger, the join table, the intent queue, and a typed command
bus. The terminal is one client of it. Prove the split is real by making a headless mode answer the
same questions as JSON.

The forcing evidence is a live process probe: the TUI gateway is a **child of the TUI launch**, not
a shared daemon. So a naive "connect to the gateway" design binds fleet lifetime to one terminal
window — while survivor 14 makes Talaria responsible for panes that outlive it. The gateway's stdio
and WebSocket transports already share one dispatch path, documented as serving non-Ink clients.

The downside is up-front architecture cost on a prototype that could ship something visible sooner
without it.

| field      | value                                                                                                                                                                                                                                                                                                                                                                                                       |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` live process probe shows the TUI launch owning both the Ink entry point and the TUI gateway as child processes, so the gateway is per-instance rather than a shared daemon. `direct:` the gateway's stdio and WebSocket transports share one dispatch path documented as serving non-Ink clients. `external:` opencode's client/server split is the closest architectural analogue in the survey. |
| source     | combined                                                                                                                                                                                                                                                                                                                                                                                                    |
| confidence | 80                                                                                                                                                                                                                                                                                                                                                                                                          |
| complexity | High                                                                                                                                                                                                                                                                                                                                                                                                        |
| axis       | A5                                                                                                                                                                                                                                                                                                                                                                                                          |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                                                                                  |

#### 18. Record protocol traffic; turn the renderer question into a measurement

A record mode writes raw JSON-RPC frames to disk and a replay harness re-drives the UI from them at
controllable speed.

The most expensive open decision in the project — stay on Ink, inherit the existing Ink package's
27,823 lines, or leave the stack entirely — is currently an argument between four frames that
disagree. With a recorder it becomes a measurement: replay the identical ten minutes into two
renderers and compare. It also turns protocol drift on a Hermes upgrade into a replay rather than a
discovery.

Given survivor 4's event log this is a small addition rather than a separate system.

The downside is that it produces evidence rather than progress, and it is easy to defer forever.

| field      | value                                                                                                                                                                                                                                                                                                                             |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `reasoned:` four frames reached incompatible conclusions on the renderer question from the same evidence, which is the signature of a question that should be measured rather than argued. `direct:` survivor 4's append-only log already captures normalized events, so raw frame capture is an increment on existing machinery. |
| source     | frame-agent                                                                                                                                                                                                                                                                                                                       |
| confidence | 85                                                                                                                                                                                                                                                                                                                                |
| complexity | Low                                                                                                                                                                                                                                                                                                                               |
| axis       | A4                                                                                                                                                                                                                                                                                                                                |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                        |

#### 19. Inherit the gateway event handler, not the renderer

Port the existing gateway event handler — 945 lines mapping every gateway event to state — and defer
the Ink rendering package, which is 27,823 lines including a 2,326-line hand-written Yoga layout
port.

The event handler is hard-won protocol knowledge with an increasing-value curve. The rendering
engine is an engine, with a permanent coupling cost.

This exists to prevent the default failure: rejecting the whole 70,200-line existing TUI tree as
"the old thing" and re-deriving the protocol mapping event by event in production.

The downside is inheriting the event handler's assumptions about the state shape it maps into.

| field      | value                                                                                                                                                                                                        |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| basis      | `direct:` line counts measured against the public Hermes tree — 945 lines for the gateway event handler, 27,823 for the Ink package including a 2,326-line Yoga port, 70,200 for the full existing TUI tree. |
| source     | frame-agent                                                                                                                                                                                                  |
| confidence | 82                                                                                                                                                                                                           |
| complexity | Med                                                                                                                                                                                                          |
| axis       | A4                                                                                                                                                                                                           |
| status     | Unexplored                                                                                                                                                                                                   |

### Later phases

#### 20. Route to a role, not a profile name

One typed, visible, editable table mapping role and capability to profile, read by tier selection,
delegation, dispatch, and swarm.

Because Hermes pre-binds model and effort per profile, every tier decision necessarily resolves to a
profile name — so a resolver table is the only shape a tier concept can take here. The profile names
in use are already roles rather than identities; the directory name is the only thing making them
look otherwise.

The downside is one more piece of configuration to keep honest.

| field      | value                                                                                                                                                                                                                                                                                                                       |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` the tier router's own documentation states that model and effort are pre-bound per profile. `direct:` `hermes profile describe` is documented as used by the Kanban orchestrator, so routing already runs off free-text descriptions only a model reads. `direct:` the swarm command takes profiles positionally. |
| source     | combined                                                                                                                                                                                                                                                                                                                    |
| confidence | 75                                                                                                                                                                                                                                                                                                                          |
| complexity | Low                                                                                                                                                                                                                                                                                                                         |
| axis       | A3                                                                                                                                                                                                                                                                                                                          |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                  |

#### 21. Expose Talaria over MCP so the agent drives the same verbs as the keyboard

Publish the derived state and command bus as MCP tools, so one integration serves two callers:
Hermes sessions and Claude Code.

This resolves the "merge Claude Code features in" seed sideways. Rather than porting Claude Code
features into a Hermes client, it lets Claude Code be a _client of Talaria_.

One distinction must be stated explicitly or a reviewer will misread it: `AGENTS.md:41-46` forbids
Kanban _operations_ depending on a model choosing to call a board tool. This runs the opposite
direction — Talaria offers tools to the agent while its own reads stay deterministic.

The downside is a public API surface that constrains internal refactoring once anything depends on
it.

| field      | value                                                                                                                                                                                                                                        |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` Hermes both consumes MCP and serves as an MCP server. `direct:` the operator's seed asks that the main session create and destroy sessions. `direct:` `AGENTS.md:41-46` constrains the inverse dependency direction, not this one. |
| source     | combined                                                                                                                                                                                                                                     |
| confidence | 65                                                                                                                                                                                                                                           |
| complexity | Med                                                                                                                                                                                                                                          |
| axis       | A3                                                                                                                                                                                                                                           |
| status     | Unexplored                                                                                                                                                                                                                                   |

#### 22. The re-entry digest, and operator-authored wake conditions

What finished, what failed, what is blocked on you, and how many are still fine as a count —
computed from the event log rather than replayed from a feed. Paired with wake conditions the
operator authors in a plain file, so alarm policy is a user artifact rather than product
configuration.

The alarm research is clear that the most effective single intervention was letting thresholds be
set per case, by the person with the context. The naval analogue is standing night orders plus a
structured watch turnover brief.

The downside is that an unmaintained wake-condition file degrades silently into either noise or
silence.

| field      | value                                                                                                                                                                                                                                                                                                                                        |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `external:` alarm-fatigue research identifies per-case operator-set thresholds as the highest-yield intervention. `external:` ship's night orders and structured clinical handoff protocols. `direct:` `kanban watch --kinds` already emits exactly the exception kinds required, and the pane manager publishes a large set of event kinds. |
| source     | combined                                                                                                                                                                                                                                                                                                                                     |
| confidence | 70                                                                                                                                                                                                                                                                                                                                           |
| complexity | Med                                                                                                                                                                                                                                                                                                                                          |
| axis       | A1                                                                                                                                                                                                                                                                                                                                           |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                   |

#### 23. Merged colour-coded tail with interest-managed fidelity

Never attach to one agent — interleave N tails into a single stream, prefixed and coloured by
profile, narrowed by regex filter.

Fidelity is a budget, not a per-source toggle: the focused agent gets a full-rate tail, the adjacent
ring gets state transitions only, and the rest gets a periodic poll. Talaria _declines to subscribe_
rather than filtering after receipt, so the cost is never paid in the first place.

The prior art is stern, which merges N pod logs into one colour-coded stream — a primitive the
survey found no agent tool provides. The frame budget argument is direct: roughly 16 milliseconds
per frame at 60fps, and many concurrent agent tails is exactly the case that blows it.

The downside is that a merged stream is harder to read than a single one when you actually do want
to follow one agent closely.

| field      | value                                                                                                                                                                                                                                   |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `external:` stern merges N pod logs into one colour-coded prefixed stream, and the prior-art sweep found no agent tool doing this. `external:` interest management and level-of-detail as standard practice under a fixed frame budget. |
| source     | combined                                                                                                                                                                                                                                |
| confidence | 68                                                                                                                                                                                                                                      |
| complexity | Med                                                                                                                                                                                                                                     |
| axis       | A1                                                                                                                                                                                                                                      |
| status     | Unexplored                                                                                                                                                                                                                              |

#### 24. You never author a workflow; you approve a generated one

The operator states a goal, the machine proposes a routed graph, and Talaria is the gated review
screen where it is edited and approved before anything dispatches.

Both halves already exist: the Kanban decompose command turns a triage task into a graph routed to
specialist profiles by description, and the swarm command produces a workers-then-verifier-then-
synthesizer shape.

This is Claude Code's plan mode applied to orchestration instead of file edits. It also makes the
operator's "workflow node is a Kanban card" intuition literally true — the generated nodes _are_
cards.

The downside is that review quality on a generated graph is hard to sustain; approval degrades into
rubber-stamping exactly the way alarm acknowledgment does.

| field      | value                                                                                                                                                                                               |
| ---------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` `kanban decompose` already routes a decomposed graph to specialist profiles using profile descriptions. `direct:` `kanban swarm` already expresses the worker/verifier/synthesizer shape. |
| source     | frame-agent                                                                                                                                                                                         |
| confidence | 70                                                                                                                                                                                                  |
| complexity | Med                                                                                                                                                                                                 |
| axis       | A3                                                                                                                                                                                                  |
| status     | Unexplored                                                                                                                                                                                          |

#### 25. One harness-enforced gate seam — contingent on Q2

The `pre_gateway_dispatch` hook can skip or rewrite an inbound message, making it the only real veto
point in the stack. The approval-request hooks are explicitly observe-only.

Build the seam once and every future precondition becomes a predicate rather than a plumbing
project.

**Contingent on Q2**, because it requires shipping Hermes-side plugin code. The honest limit is that
it enforces _mechanical_ preconditions only — it cannot make a reviewer's self-assessed score
honest, only recorded, visible, and non-editable.

| field      | value                                                                                                                                             |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` `pre_gateway_dispatch` can skip or rewrite an inbound message; the pre- and post-approval-request hooks are documented as observe-only. |
| source     | frame-agent                                                                                                                                       |
| confidence | 55                                                                                                                                                |
| complexity | Med                                                                                                                                               |
| axis       | A3                                                                                                                                                |
| status     | Unexplored                                                                                                                                        |

#### 26. Make the concurrency ceiling real, with a visible queue behind it

Render the concurrency limit as an editable number with a queue behind it, setting the existing
dispatcher's bounds rather than starting a rival loop.

The operator's own concurrency rule currently lives in a markdown file a model is asked to obey. A
limit enforced by prose is not a limit. Claude Code's workflow runtime is the counter-example: it
exposes total, spent, and remaining budget, and the agent spawn call _throws_ at exhaustion —
enforced at the call site rather than documented.

The Kanban dispatch command already expresses a bounded pool through its max and failure-limit
flags.

The downside is that the ceiling only binds work Talaria routes; anything dispatched another way
still escapes it.

| field      | value                                                                                                                                                                                                                                                                                              |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `external:` Claude Code workflow budgets are enforced at the call site by throwing at exhaustion rather than by documentation. `direct:` `kanban dispatch` already accepts max and failure-limit bounds. `reasoned:` a limit that exists only as prose a model is asked to follow is not enforced. |
| source     | frame-agent                                                                                                                                                                                                                                                                                        |
| confidence | 70                                                                                                                                                                                                                                                                                                 |
| complexity | Low                                                                                                                                                                                                                                                                                                |
| axis       | A3                                                                                                                                                                                                                                                                                                 |
| status     | Unexplored                                                                                                                                                                                                                                                                                         |

#### 27. Falsifiable interrupt tiers that auto-demote

Every interrupt records whether the operator acted within a window. A class whose action rate falls
below threshold auto-demotes itself and says so. Escalation back up is deliberate and manual.

This is the mechanism none of the eight researched domains has. Every intervention the alarm
literature names is a _static authoring_ decision made before the alarm fires — but actionability is
by definition only observable after the operator responds or does not. A system with no correction
term drifts into the same regime however carefully its tiers were authored.

The asymmetry between auto-demote and manual-escalate follows from failure costs: an over-quiet
channel is recoverable, an over-loud one that trained the operator to ignore it is not.

It is cheap here specifically because Talaria sees both the alarm and every subsequent operator
action in one process.

The downside is that it is reasoned rather than observed — no surveyed system does this, so there is
no evidence it works in practice.

| field      | value                                                                                                                                                                                                                                                                                                                                                   |
| ---------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `reasoned:` every intervention named in the alarm literature is a static pre-authoring decision, while actionability is only observable after the fact; a control system with no correction term drifts regardless of how well it was initially tuned. The demote/escalate asymmetry follows from the relative recoverability of the two failure modes. |
| source     | frame-agent                                                                                                                                                                                                                                                                                                                                             |
| confidence | 60                                                                                                                                                                                                                                                                                                                                                      |
| complexity | Med                                                                                                                                                                                                                                                                                                                                                     |
| axis       | A1                                                                                                                                                                                                                                                                                                                                                      |
| status     | Unexplored                                                                                                                                                                                                                                                                                                                                              |

#### 28. Make the whole status one line, and emit it where Talaria isn't

Publish the fleet's exception state into surfaces Talaria does not own — a pane-manager metadata
token, the terminal title, a shell prompt segment.

This is the cheapest possible version of the whole product. It ships before any pane exists, and it
pushes state _to_ the pane manager rather than duplicating the pane manager's state locally.

The reasoning is that Talaria will not be the focused window most of the time. A console that only
works when you are looking at it does not solve "several cards were blocked and nobody noticed."

The downside is severe compression — one line cannot carry enough to act on, only enough to make you
look.

| field      | value                                                                                                                                                                                                                                                                                                          |
| ---------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| basis      | `direct:` the pane manager exposes a bounded per-pane string-token map settable from the CLI, and its passive layer already scrapes terminal titles with per-agent regex rules. `reasoned:` the originating pain is a discovery failure that occurs precisely when the operator is not looking at the console. |
| source     | frame-agent                                                                                                                                                                                                                                                                                                    |
| confidence | 78                                                                                                                                                                                                                                                                                                             |
| complexity | Low                                                                                                                                                                                                                                                                                                            |
| axis       | A5                                                                                                                                                                                                                                                                                                             |
| status     | Unexplored                                                                                                                                                                                                                                                                                                     |

---

## Did Not Survive (revivable)

Explicit rejection is the quality mechanism. Cut ideas keep stable ids so they can be revived, which
re-enters them into the same critique with new evidence. Ids are never renumbered on a status change.

| id  | title                                           | summary                                                                                             | reason                                                                                                                    | status   |
| --- | ----------------------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------- | -------- |
| R1  | Profile generation view                         | Read-only view of what the profile generator produced, generation over generation                   | Constraint 7 — read-only, so not authoring, but still a Hermes _administration_ surface                                   | rejected |
| R2  | Generate profiles from one versioned manifest   | Replace hand-maintained profiles with generation from a schema-validated manifest                   | Cut twice: out of scope under constraint 7, and already implemented outside this repo                                     | rejected |
| R3  | Prune unused profiles / utilization column      | Show run counts per profile and prune the unused ones                                               | Pruning a generated profile is a manifest edit plus a regenerate, so it belongs to the generator, not the TUI             | rejected |
| R4  | Price and surface generation rollback           | Make rolling a profile back to a prior generation an operator action                                | Profile authoring, excluded by constraint 7                                                                               | rejected |
| R5  | Run the wholeness gate across the roster        | One health light per profile from the existing wholeness checker                                    | Same class as R1 — administration surface                                                                                 | rejected |
| R6  | Per-profile nominal envelopes                   | Per-profile alerting thresholds so "normal" is defined per agent                                    | Cut as written because it writes threshold declarations into each profile's own config                                    | rejected |
| R7  | Ship no fleet view at all                       | Push all content into the pane manager's own surfaces and build no fleet UI                         | Maximum-compliance reading of constraint 1, but cannot deliver the cross-profile view that is the actual gap              | rejected |
| R8  | Remove modes and panes entirely                 | One resource list, command palette, filter, contextual verb keys — k9s shaped                       | Coherent product, but makes the conversation one resource class among several, against the operator's stated working mode | rejected |
| R9  | Two modes: gather and drive                     | Two control regimes from shepherding theory with a computed dispersion trigger                      | Strongest pro-two-mode argument in the run, but at current fleet size there is nothing to gather                          | rejected |
| R10 | Hermes-side plugin pushing delegation lifecycle | Emit delegation events Hermes does not currently emit, from a plugin                                | Contingent on Q2, and largely obviated if survivor 8 wins                                                                 | rejected |
| R11 | Andon cord — authority envelope plus cheap halt | Let a child at the edge of its authority stop and ask instead of guessing                           | Cut only because survivor 8 dissolves the problem by not using delegated children                                         | rejected |
| R12 | Delete the spawn-tree view                      | Delegation is a three-slot rack, not a tree — render it as slots                                    | Correct on the evidence, but subsumed by survivor 8                                                                       | rejected |
| R13 | Aggregate early-warning score                   | One composite health score per agent instead of four independent signals                            | Blocked on an unanswered question: whether the four telemetry signals get interrupt authority or are pull-only            | rejected |
| R14 | Sterile-cockpit suppression windows             | Suppress non-critical interrupts during declared focus windows                                      | Good mechanism, real false-positive risk of suppressing a genuine emergency                                               | rejected |
| R15 | Root-cause alarm suppression over the task DAG  | Suppress downstream alarms caused by one upstream failure, computed over task links                 | Cut for sequencing only — it needs a fan-out failure to suppress and the board is currently near-idle                     | rejected |
| R16 | Squawk mismatch as the stuck detector           | Correlate outside-the-process agent status against gateway event silence; disagreement is the alert | Cut for sequencing, not merit — it needs survivor 10's join key first                                                     | rejected |
| R17 | Profile currency gates routing                  | Require recent successful runs before a profile is eligible for routing                             | Sits on the constraint-7 boundary — marking a profile ineligible is close to managing the population                      | rejected |
| R18 | Compose the cockpit from pane-manager panes     | Separate OS processes get separate redraw budgets for free                                          | Couples layout deeply to the pane manager and makes Talaria unusable without it                                           | rejected |
| R19 | No conversation renderer in v1                  | Put the existing Hermes TUI in a pane and build only the fleet surfaces                             | Strongest cost argument in the run, but defers the project's own founding premise                                         | rejected |
| R20 | Append transcript to native scrollback          | Repaint only pinned regions; flicker becomes impossible by construction                             | Forecloses in-place editing, such as collapsing a finished tool call                                                      | rejected |
| —   | axis: A2                                        | Only one survivor on the profile and identity axis                                                  | Deliberate gap — constraint 7 narrowed this axis to session handling                                                      | —        |

### Revival conditions

**R1, R5 — revive if** the operator's line is "no writes to agent identity" rather than "no
agent-admin surface at all." Under the first reading these survive as written today. Both frames
rewrote these candidates after constraint 7 landed, and the revised forms are strictly read-only:
one calls the existing wholeness module and renders its answer rather than re-deriving anything,
which is constraint-4 compliance rather than a violation of it. The underlying data is real and
currently unread — an exported capability projection sits against a schema-validated manifest, and
roughly twenty retained generations sit on disk with nothing diffing them.

**R3 — revive if** the utilization _column_ alone is judged in scope, separate from the prune
action. The underlying question is live and worth answering by someone: if most profiles have never
run a task, every fleet feature in this document is being sized for a population that does not
exist. The operator has precedent for acting on that kind of answer — a 4,731-line mode was deleted
after recorded runs showed zero uses.

**R4 — the finding is retained as a warning even though the idea was cut.** The generator's own
documentation claims generations are complete and immutable, and for the _live_ generation that is
false: the current generation is the live Hermes home and accumulates into it, so it grows well
beyond the size of its frozen predecessors. A bare symlink flip back does not restore a
configuration; it swaps in an agent whose sessions and logs are frozen at mint time. Anything
Talaria ever displays about generations must not imply rollback is free.

**R6 — revive immediately if** the envelope lives in Talaria's own config keyed by profile name
rather than in the profile's config. The underlying idea carries the only measured effect size in
the alarm research.

**R7 — revive if** survivor 14's exception strip plus survivor 28's one-line emission prove
sufficient on their own.

**R8 — revive if** survivor 14's drill-down grows enough resource classes that a palette becomes the
natural primary surface.

**R9 — revive when** the fleet is routinely large enough that the exception strip overflows.

**R11 — revive immediately if** survivor 8 is rejected. It names the sharpest concrete Hermes gap in
the run: blocked tools strip clarify from every delegated child, so a child at the edge of its
authority must guess past it.

**R12 — revive if** delegation stays subagent-based rather than session-based.

**R14 — revive after** survivor 13 is built and the top-tier exemption list can be written from
evidence rather than guessed.

**R15 — revive when** swarm and decompose are in routine use.

**R16 — strong revival candidate** as soon as survivor 10 lands. High positive predictive value and
costs nothing to compute.

**R17 — revive as advisory** — annotate a stale profile rather than blocking routing to it.

**R18, R19, R20 — revive on measurement.** All three are held pending survivor 18's record-and-replay
harness. The renderer question was correctly identified by four frames as the one decision that
should not be settled by argument. R20 is the leading candidate if the measurement says diffing is
not enough; R18 if single-process rendering cannot hold; R19 as a schedule fallback if the fleet
surfaces prove more valuable than the conversation work.

## Axis Coverage

| axis                                         | survivors                     | count |
| -------------------------------------------- | ----------------------------- | ----- |
| A1 — fleet observability and steering        | 1, 4, 7, 9, 13, 22, 23, 27    | 8     |
| A2 — profile and identity                    | 5                             | 1     |
| A3 — work orchestration and routing          | 8, 11, 12, 20, 21, 24, 25, 26 | 8     |
| A4 — single-session craft                    | 14, 18, 19                    | 3     |
| A5 — integration architecture and transports | 2, 3, 6, 10, 15, 16, 17, 28   | 8     |

A2's collapse to a single survivor is the direct and intended consequence of constraint 7. A4 is
thin because three of its four strongest candidates are held pending the measurement survivor 18
provides.

## Co-ideation Log

Every operator seed entered the frame agents as a peer to build on, challenge, or combine, and faced
the identical critique. None was rubber-stamped; none was silently dropped.

| source      | entered            | idea / seed                                                                     | outcome                                                                                                                                                                                                                                                |
| ----------- | ------------------ | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| user-seed   | Phase 0            | S1 — pane-manager features as first-class citizens; handle all profiles at once | survived across 5, 15, 16, 28 — reframed from "port its features" to "drive its socket and push state to it"                                                                                                                                           |
| user-seed   | Phase 0            | S2 — merge Claude Code features in                                              | survived as 21, inverted — rather than porting features in, let Claude Code become a client of Talaria over MCP                                                                                                                                        |
| user-seed   | Phase 0            | S3 — mine Hermes Desktop for features                                           | absorbed into the A4 candidates; no standalone survivor, and the clutter the operator named is what survivor 13 exists to prevent                                                                                                                      |
| user-seed   | Phase 0            | S4 — agent management, view, and steering as the primary update                 | survived as 8 and 9. The frames did not file this as a Hermes bug; survivor 8 dissolves it by using sessions, which are already addressable and steerable                                                                                              |
| user-seed   | Phase 0            | S6 — main session creates, updates, and deletes sessions                        | survived as 5, 8, and 21; frame 3 reached the same conclusion independently from the delegation limits                                                                                                                                                 |
| user-seed   | Phase 0            | S7 — the modest alternative: better single-profile TUI plus a good plugin       | cut → R7, and it is the strongest cut in the set. Loses because it cannot deliver the cross-profile view, which is the actual measured gap                                                                                                             |
| user-seed   | Phase 0            | S8 — exhaustive prior-art search                                                | executed; produced the external basis behind 1, 9, 13, 14, 22, 23, 26                                                                                                                                                                                  |
| user-seed   | Phase 0            | S9 — this is a harness, not just a TUI                                          | survived as 11, 12, 24 — with the central reframe that the harness substantially already exists inside Hermes as the Kanban subsystem, headless                                                                                                        |
| user-seed   | Phase 0            | S10 — generalize the plugin suite into the product                              | survived as 11, transformed: saga's outcome graph and Kanban's task graph are the same shape, and Kanban's is durable and already dispatching                                                                                                          |
| user-seed   | Phase 0            | S11 — keep many more survivors than normal                                      | applied to this document — 28 survivors instead of the usual 5 to 7, horizon-tagged                                                                                                                                                                    |
| user-seed   | Phase 0            | S12 — run a Socratic review to pull more seeds                                  | executed; produced seeds S13 through S19                                                                                                                                                                                                               |
| interview   | Phase 0 Socratic   | S13 — the spine exists but is invisible                                         | became constraint 4 and the direct basis for 2, 11, 12, 14                                                                                                                                                                                             |
| interview   | Phase 0 Socratic   | S14 — Talaria drives the pane manager                                           | became constraint 1; shapes 10, 15, 28                                                                                                                                                                                                                 |
| interview   | Phase 0 Socratic   | S15 — own harness first, accept divergence                                      | became constraint 2. Leaves a live contradiction with `README.md:14` and `AGENTS.md:7`, which still commit to upstreamability                                                                                                                          |
| interview   | Phase 0 Socratic   | S16 — the fleet is one Hermes install and all its profiles                      | became constraint 3; scoped 2, 5, 20                                                                                                                                                                                                                   |
| interview   | Phase 0 Socratic   | S17 — conversation primary, but two modes are attractive                        | argued by five frames as the operator asked; resolved to survivor 14, with the two-mode design preserved as R9 with a revival trigger                                                                                                                  |
| interview   | Phase 0 Socratic   | S18 — steering means addressable sessions plus Kanban-shaped workflow control   | survived as 8, 9, 24; the "workflow node is a Kanban card" intuition became literally true                                                                                                                                                             |
| interview   | Phase 0 Socratic   | S19 — all four telemetry signals wanted                                         | survived partially. Frame 5 challenged it directly on alarm-fatigue grounds and won a constraint: survivor 13 caps what may compete for attention, and R13 holds the aggregate-score question pending whether the four signals get interrupt authority |
| user-seed   | mid-run correction | Constraint 7 — Talaria reads agent state, it does not author agent identity     | applied retroactively as a hard filter. Cut R1 through R6 and narrowed axis A2 from five survivors to one                                                                                                                                              |
| frame-agent | Phase 2            | The gateway is two processes and Kanban is in the other one                     | survived as 3 — the single most consequential grounding correction in the run                                                                                                                                                                          |
| frame-agent | Phase 2            | Spawn work as sessions, not delegated children                                  | survived as 8 — the highest-leverage reframe in the run                                                                                                                                                                                                |
| frame-agent | Phase 2            | Measure the renderer question instead of arguing it                             | survived as 18, and holds R18, R19, R20 pending its result                                                                                                                                                                                             |

## Provenance

Run id `c71d6744`, 2026-08-02. Six frames — pain and friction, inversion and removal,
assumption-breaking, leverage and compounding, cross-domain analogy, constraint-flipping — produced
85 candidates, merged into 21 clusters, filtered to 28 survivors and 20 revivable cuts.

Two grounding corrections were found and verified mid-run against a live install, and both are
reflected above: profiles are generated and generation-versioned rather than hand-maintained, and an
earlier inference drawn from a zero-byte database file was wrong because the live database sits at a
different path.

Two claims in this document are explicitly not fully verified and are marked where they appear: the
pane manager's method count in survivor 16, and the extent of the profile population's actual
utilization in R3.
