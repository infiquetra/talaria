# 06 — Talaria Workflows: Claude-Code-style orchestration on every Hermes model

Status: `proposed`

Evidence: [u15](research/u15-hermes-orchestration.md) (what Hermes can orchestrate today),
[u09](research/u09-prior-art-multiagent-acp.md) §2 (workflow prior art), the live attach probe in
[03-hermes-today.md](03-hermes-today.md) §2, and Claude Code's own workflow tool contract. Research
reports are cited by number; for example, u15 is research/u15-hermes-orchestration.md, the Hermes
orchestration survey.

## The goal

In Claude Code, "ultracode" means the agent writes a short JavaScript program that fans work out to
many subagents — each with its own model and effort — runs them in parallel or as a pipeline, checks
their results against schemas, verifies them adversarially and synthesizes an answer. A journal lets
an interrupted run resume without paying for finished agents again, and a progress view shows every
agent live. The limitation is that every worker is a Claude model.

Talaria Workflows is the same power with Hermes's whole model inventory behind it: a reviewer on one
provider, a second reviewer on another, a cheap surveyor on a free model, a judge on the strongest
model available. On the test machine that inventory was 14 authenticated providers and 295 models.

**This combination does not exist anywhere today.** Claude Code has the orchestration shape but only
Claude workers; Hermes has per-task provider routing (kanban cards, Mixture of Agents presets,
`session.create`) but no deterministic script runner, and its fast delegation tool cannot pick a model
per child. ([u09](research/u09-prior-art-multiagent-acp.md) §0, [u15](research/u15-hermes-orchestration.md) headline answers)

## Design decisions

1. **The script API is Claude Code's, unchanged where possible.** `agent()`, `parallel()`,
   `pipeline()`, `phase()`, `log()`, `args`, `budget`, `workflow()`, and `export const meta`. The
   operator's instincts and existing scripts transfer, and every model that has seen Claude Code's
   workflow documentation can write these scripts. Hermes-specific options are additions.
2. **Every workflow agent is an ordinary Hermes session.** Created on the canonical gateway with an
   explicit model, provider and reasoning effort, hidden from human session pickers
   (`source: "tool"`), and visible in Talaria's fleet view with its own transcript. Nothing about a
   workflow agent is a black box.
3. **The runner is a separate, detachable process; the terminal UI is only a viewer.**
   `talaria workflow run script.js` runs headless and writes an append-only journal. With `--detach`
   it outlives the terminal UI that started it. The terminal UI discovers runs from a runs directory
   and attaches to watch or steer them. Because every agent's prompt is a durable Hermes admission,
   even a crashed runner loses only *progress*, never *work*: running agents finish inside the
   gateway, and `talaria workflow resume <run>` re-attaches to them.
4. **Models come from a staffing table, not from scripts.** Scripts say `tier: 'judge'` (or name a
   model outright). A per-user staffing file maps tiers to Hermes provider, model and effort. Changing
   providers is one edit, and every workflow follows. This is the operator's existing tiering practice,
   made executable.
5. **The agent writes and launches workflows, as in Claude Code.** Talaria installs a Hermes skill
   ("talaria-workflows") that teaches any Hermes session the script API, the staffing tiers and the
   `talaria workflow` commands. A lead session can then author a script and start it with its
   ordinary terminal tool. "Ultracode mode" in Talaria is a session toggle that tells the lead session
   to orchestrate substantive tasks this way. No Hermes change is needed for any of this.

## Script API (version 1)

Identical to Claude Code's workflow tool unless a row says otherwise.

| Function | Behaviour |
|---|---|
| `export const meta = { name, description, phases }` | A pure literal, as in Claude Code. |
| `agent(prompt, opts)` | Runs one agent to completion and returns its final text, or a validated object when `schema` is given, or `null` if it failed after retries or was skipped. |
| `opts.label`, `opts.phase`, `opts.schema` | As in Claude Code. |
| `opts.tier` | **Talaria.** A key in the staffing table, for example `survey`, `builder`, `judge`. |
| `opts.model`, `opts.provider`, `opts.effort` | An explicit Hermes model (overrides `tier`). `effort` maps to Hermes's per-session reasoning effort. |
| `opts.profile` | **Talaria.** The Hermes profile that hosts the agent (its credentials, skills, memory). Default: the run's profile. |
| `opts.toolsets`, `opts.skills` | **Talaria.** Restrict or add Hermes toolsets and skills for this agent. Workflow sessions must name toolsets explicitly, because `source: "tool"` sessions do not get the coding defaults. |
| `opts.isolation: 'worktree'` | Run in a fresh git worktree; the runner creates it and passes it as the session's working directory. |
| `opts.maxTurns` | Hermes's per-session turn limit. |
| `parallel(thunks)` | A barrier, as in Claude Code. Width is bounded by the run's pool (below). |
| `pipeline(items, ...stages)` | No barrier between stages, as in Claude Code. |
| `phase(title)`, `log(message)`, `args`, `budget` | As in Claude Code. `budget` can carry a **money** limit as well as tokens (Hermes's own `run_budget` is time, not money). |
| `workflow(nameOrRef, args)` | Runs a saved workflow inline, one level deep. |

Rules carried over for deterministic resume: no `Date.now()`, no `Math.random()`, no argument-less
`new Date()`, no filesystem or network access. The runner provides journaled `now()` and `random()`
helpers for scripts that genuinely need them.

## How a call maps onto Hermes

```text
agent(prompt, opts)
  key      = hash(run_id, call_path, prompt, resolved model/provider/effort/toolsets/skills, schema)
  journal  has a terminal record for key      → return the stored result
           has an admission for key           → prompt.receipt(...) and re-attach
  session  = session.create {request_id: "wf:<run>:<key>", source: "tool", model, provider,
                             reasoning: effort, toolsets, skills, cwd, max_turns}     (idempotent)
  receipt  = prompt.submit {session_id, submission_id: "wf:<run>:<key>:0",
                            text: brief(prompt, schema), finite: true}       (idempotent)
  stream   the session's events into the progress tree (subagents nest under it)
  on terminal receipt: validate against schema; if invalid, one correction turn
                       (submission_id ...:1) quoting the validator's errors
  journal.append {key, session_id, admission_ids, status, result, schema_valid, usage, cost}
```

Never send `unattended: true` on this call. In Hermes it means "auto-approve every dangerous command
in this turn" (`tools/approval.py`, `_yolo_active`); it does not mean "no viewer is attached". An agent
that should be able to act without approvals gets that from an explicit approval policy that the
operator chooses for its workflow and that is shown in the run manifest.

Both `request_id` and `submission_id` are idempotency keys on the gateway, so a runner that crashes
between a call and its journal write cannot create duplicate sessions or prompts. A call whose
admission became `unknown` (it had started when the gateway restarted) is shown to the operator and
never silently re-run; this mirrors Hermes's own rule. ([u15](research/u15-hermes-orchestration.md) §4.3, §4.5)

## The staffing table

```toml
# ~/.talaria/staffing.toml — tiers used by workflows; edit to change providers everywhere
[tiers.survey]          # read-only sweeps, counting, listing
model = "minimax-m3.1-flash"
provider = "ollama-cloud"

[tiers.builder]         # implement a settled unit, including its tests
model = "<a strong coding model>"
provider = "<its provider>"
effort = "high"

[tiers.judge]           # design, adversarial review, final synthesis
model = "<the strongest model available>"
provider = "<its provider>"
effort = "high"

[limits]
max_in_flight = 10      # halve on the first rate-limit error, retry the failed unit once
```

At the start of every run the runner reads `model.options` and records a roster snapshot in the run
manifest: which providers are authenticated, which models are unavailable, each provider's rate-limit
state and subscription usage window. It refuses to start if a tier names a model that is not
authenticated, and it schedules per provider: a provider whose usage state is `limited` gets no new
units until its reset time. Cost is estimated before the run from the pricing strings in
`model.options` (shown as a range, and "unknown" for unpriced models, never zero) and accumulated
during the run from each session's usage events.

## The progress view

A tree: run → phase → agent → that agent's own subagents (Hermes's `subagent.*` events carry parent
links and depth, so they nest). Each node shows its model and provider, its state (queued, running,
waiting on you, done, failed, unknown), tokens and cost, its last tool call, and a live tail. Keys:
open the agent's session in the main view, steer it, stop it (`session.interrupt`), restart it, pause
the run, stop the run. The tree must render from the journal alone, so it works the moment the
terminal UI reattaches and before live events arrive.

Approvals raised by workflow agents go into the same "needs you" queue as every other session. The
safest default for unattended runs is a read-only toolset for review and research workflows, and
worktree isolation plus an explicit approval policy for workflows that edit code. An approval nobody
answers is denied after Hermes's `approvals.timeout` (default 300 s), and the workflow treats that
agent as failed.

## Built-in workflows to ship first

1. **review-branch** — three reviewers on three different providers, each with a different lens
   (correctness, security, simplicity), then an adversarial verifier per finding, then a synthesis.
2. **research** — a multi-modal sweep (code search, web search, documentation) on cheap models, then
   deep reads, then a synthesis on the judge tier.
3. **fix-failing-tests** — find failing tests, fix each in its own worktree on the builder tier, verify
   by running the tests, then summarize.
4. **ultracode** — understand, design (a judge panel of independent approaches), implement, review:
   the shape the operator already uses in Claude Code.

## Later: three ways to go further

1. **Compile static graphs to kanban.** A script whose shape is a fixed graph can be compiled to kanban
   cards (`agent` → card with model and provider overrides; `pipeline` → parent links; `parallel` →
   sibling cards). Kanban's dispatcher runs inside the gateway, so the work proceeds with no runner at
   all. Costs: a 60-second dispatch tick (shortened by nudging `POST /dispatch`), results passed as
   files, and the kanban REST routes are unauthenticated on localhost. ([u15](research/u15-hermes-orchestration.md) §4.11)
2. **Propose a workflow runner to Hermes upstream.** A parentless, durable launch API with per-unit
   model, provider, reasoning, toolsets and working directory; a `workflow_runs` journal table;
   `workflow.start/status/cancel/resume` methods and `workflow.step.*` events; and an `output_schema`
   on admissions with the same one-correction contract `delegate_task` already implements. Prototype
   the client-side runner first; it is the cheapest way to settle which API is worth proposing.
3. **Per-child models in `delegate_task`.** A small upstream change (per-task `model`, `provider`,
   `reasoning_effort`, restricted to an operator allow-list) would give model-driven fan-outs the same
   power. ([u15](research/u15-hermes-orchestration.md) §4.10)

## Pre-mortem: how this most likely fails

1. **Scripts that are not deterministic break resume.** If a prompt or a call path depends on a clock,
   on unordered live data, or on free text fed straight into a later prompt, replay produces new keys
   and pays for finished work again. Mitigation: journal every input to a key, and expose clock and
   randomness only through journaled helpers.
2. **Provider limits, not Hermes, cap throughput.** A ten-wide fan-out on one subscription provider
   hits its window quickly. The roster-aware, per-provider scheduler is not optional.
3. **Unattended approvals stall.** An agent that hits an approval with nobody watching waits five
   minutes and is denied. Choose toolsets and policies per workflow, and surface approvals loudly.
4. **The gateway's session policy is weeks old and changing.** Sixteen commits landed on
   `gateway/session_policy.py` on main between 2026-10-18 and 2026-10-20 (GitHub commits API,
   committer dates). Gate the runner on
   `runtime.describe` capabilities and the generated contract of the pinned Hermes release.
