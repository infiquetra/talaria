# ADR-0011: Workflows run in a detachable headless runner

Status: `accepted`
Date: 2026-10-22
Accepted: 2026-10-23
Deciders: operator
Supersedes: none
Affected components: the workflow runner, the fleet view, the CLI

## Context

The operator wants Claude-Code-style workflows ("ultracode") in which each agent can use any model
Hermes is configured with, and agents that keep working while the operator does something else. Hermes
has per-session model choice (`session.create`), durable admissions, and per-task models in kanban, but
no deterministic script runner, and its delegation tool cannot choose a model per child.

## Decision

1. Workflows are JavaScript scripts with Claude Code's workflow API (`agent`, `parallel`, `pipeline`,
   `phase`, `log`, `args`, `budget`, `workflow`) plus Hermes options (`tier`, `provider`, `profile`,
   `toolsets`, `skills`, `isolation: 'worktree'`).
2. `talaria workflow run` executes a script in a headless runner process that writes an append-only
   journal; `--detach` lets it outlive the terminal UI. The terminal UI watches runs; it does not own
   them.
3. Each `agent()` call is an ordinary hidden Hermes session created with an explicit model, provider
   and effort, and an idempotent `request_id` and `submission_id`, so a crashed runner resumes without
   duplicating work.
4. Models are resolved through a staffing table of tiers, checked against `model.options` at the start
   of every run.

## Rejected alternatives

- **Run scripts inside the terminal UI.** Closing the window would stop orchestration.
- **Wait for a Hermes workflow runner.** Nothing exists upstream; a working client-side runner is the
  best way to design the API worth proposing.
- **Kanban only.** Durable, but static graphs, a 60-second dispatch tick, and results passed as files.
  Kept as a later "compile to kanban" option.

## Consequences

- Workflows work on today's Hermes with no upstream change.
- A runner killed mid-run loses progress, not work; `talaria workflow resume` re-attaches.

## Revisit when

- Hermes ships a backend workflow runner, or accepts per-child models in `delegate_task`.
