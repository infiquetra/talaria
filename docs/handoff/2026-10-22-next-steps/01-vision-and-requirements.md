# 01 — Vision and requirements

Status: `proposed`
Date: 2026-10-22
Audience: the operator, and any coding agent picking up Talaria after this date

## The one-sentence product

> **Talaria is Claude Code for Hermes:** a terminal client in which you code exactly as you would in
> Claude Code, with Hermes's extras on top — any model Hermes is configured for, an agent backend
> that keeps running when the client closes, several agents you can hand work to and leave running
> while you do something else, full and friendly configuration of the Hermes backend, and
> Claude-Code-style multi-agent workflows in which every agent can run on a different Hermes model.

Hermes's own Desktop app is the closest existing picture of the feature set, and the operator likes
it; what Talaria removes is the bloat. Hermes's own terminal UI is the baseline Talaria has to beat
on speed and cleanliness, not just on features.

## What the operator said, restated as testable requirements

The operator's brief (2026-10-22) had ten points. Each requirement below names how a reviewer can
tell whether a build meets it. Numbers marked *target* are this document's proposal, not something
the operator stated; they are there so "fast" and "clean" can fail a test.

| Id | Requirement | How to check it |
|---|---|---|
| R1 | **Fast.** Launch-to-usable-composer, keystroke echo, and streaming must feel instant and stay instant in a long session. | *Target:* under 200 ms from launch to a usable composer with a running backend; keystroke echo under 16 ms; no dropped input while a reply streams; idle CPU near 0%; resident memory under 100 MB after a 2,000-message session. Measured by the benchmark harness in work unit P1-11. |
| R2 | **Clean.** The transcript reads like Claude Code's: finished output lives in the terminal's own scrollback, so native search, selection and copy work; no flicker; one status line; no permanent panels. Overlays (pickers, fleet view, settings, diff review) appear only when asked for and leave nothing behind. | Side-by-side review against Claude Code on the same task; a screenshot test for each overlay closing cleanly. |
| R3 | **Coding parity with Claude Code.** Everything a developer uses daily in Claude Code has an equivalent: streaming replies, tool-call cells, diffs, approvals and permission modes, plan mode, interrupt, queue and steer, `@` file mentions, image paste, `!` shell, slash and custom commands, skills, model and effort switching, context usage and compaction, cost, resume and continue, checkpoints and rewind, status line, keybinding customisation, vim mode, a headless `-p` mode for scripts. | The parity checklist in [04-prior-art.md](04-prior-art.md#claude-code-parity-checklist), each row marked done with a test or a recorded manual check. |
| R4 | **Many models.** Any session, subagent or workflow step can run on any model Hermes is configured for; switching is per session and never silently changes a global default. | Switch a live session between two providers; confirm the backend's default model is unchanged afterwards. |
| R5 | **Always-running backend.** Agent work belongs to Hermes, not to the client process. Closing Talaria never stops a turn; reopening it shows what happened while it was closed, including questions still waiting for an answer. | Start a long turn, quit Talaria, wait, relaunch: the turn finished (or is still running) and the transcript is complete. |
| R6 | **A fleet of agents.** The operator can send prompts to several sessions, leave them working, and keep coding in another. A fleet view shows each session's state (working, waiting on you, done, failed), its model and cost; a single "needs you" queue collects every pending approval or question across sessions; any session can be opened, steered, interrupted or closed from the fleet view. | Three sessions on three models, each given a task; operator answers an approval in one without leaving the session they are typing in. |
| R7 | **Configure Hermes fully, in a very friendly way.** Providers and keys, models, profiles, Model Context Protocol (MCP) servers, plugins, skills, connectors, cron jobs, messaging platforms, memory, tools and toolsets — all readable and editable from Talaria, with validation, masked secrets, and a clear "applies now" or "needs restart" label. Built from the backend's own schemas so new Hermes settings appear without a client release. | Add a provider key, add an MCP server, change a profile's model, create a cron job — all without leaving Talaria or opening a YAML file. |
| R8 | **Workflows across Hermes's models.** A Claude-Code-style workflow engine: a deterministic script fans work out to many agents, each on a model chosen from Hermes's configured inventory, with parallel and pipeline stages, schema-checked results, a live progress tree, cost limits, and resume after interruption. | Run the bundled "review this branch" workflow with three reviewers on three different providers; kill Talaria halfway; resume; the finished agents are not re-run. |
| R9 | **Keeps up with Hermes.** Hermes ships thousands of merged changes per release. Talaria targets Hermes's stable release channel, generates its protocol types from the contract Hermes publishes, and detects contract drift in continuous integration rather than in the operator's terminal. | A scheduled CI job regenerates types against the newest Hermes stable release and fails loudly on a breaking change. |
| R10 | **Buildable by moderately capable coding agents.** Small work units with explicit inputs, outputs and tests; strong compile-time feedback; deterministic replay tests from recorded sessions; no unit that needs the whole system in its head. | Each unit in [07-roadmap-and-work-units.md](07-roadmap-and-work-units.md) names its files, interfaces and acceptance tests, and can be done without reading other units. |

## Non-goals

- **No agent logic in the client.** The agent loop, tools, memory, skills, delegation, scheduling
  and model calls belong to Hermes. If Talaria needs something Hermes does not offer, the fix is a
  Hermes change (Hermes merges outside contributions at a very high rate), not a client-side
  imitation.
- **Not a Hermes fork and not a graphical app.** Talaria stays a terminal program. Hermes Desktop
  already exists for people who want a window.
- **No release ceremony heavier than the product.** Verification must be proportionate: automated
  tests, a recorded-session replay suite, and a short smoke script per release, run by a person or by
  an agent in Herdr (unit P1-13; Herdr is defined in the glossary below). See
  [02-current-state.md](02-current-state.md) for why this is called out.

## Glossary

- **Backend** — the long-running Hermes process that hosts sessions, runs agents and serves clients.
  Since Hermes v0.22.0 (2026-10-13) there is one per machine, serving every profile. Since v0.22.3
  (2026-10-21), the first stable release to ship the session authority, it also owns every session and
  clients attach to it as viewers. Talaria's minimum supported Hermes is v0.22.3.
- **Session** — one Hermes conversation with its own transcript, model and working directory.
- **Subagent** — a child agent a session starts with Hermes's delegation tool; it belongs to its
  parent session.
- **Fleet** — all sessions the operator has open across profiles, plus their subagents.
- **Workflow** — a script that orchestrates many agents deterministically; see
  [06-workflows.md](06-workflows.md).
- **Contract** — the machine-readable description of every method, event and server-to-client
  request the Hermes backend speaks, generated by Hermes from its own type declarations.
- **Herdr** — a terminal multiplexer for running coding agents in panes, comparable to tmux. It is the
  operator's daily environment, so Talaria's live checks run inside it; tmux is the fallback for anyone
  without it.
