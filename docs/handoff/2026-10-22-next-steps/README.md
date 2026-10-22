# Talaria next steps — handoff, 2026-10-22

Status: `proposed` — written for the operator to decide on, and for coding agents to execute once they
have.

Talaria is a third-party terminal client for Hermes Agent, the open-source agent backend from Nous
Research. Hermes runs the agents, tools and models; Talaria is the screen and keyboard a developer
codes in. This handoff records what was measured on 2026-10-22 and recommends how to rebuild Talaria
as "Claude Code for Hermes".

**Talaria should be rewritten as a fast, inline, Hermes-native client that attaches to the Hermes
gateway the way Hermes's own clients now do.** The current Python client is slow where it matters
(launch, and typing while a reply streams), cannot answer approval prompts on current Hermes, and has
most of its effort locked in process records rather than product. Hermes itself changed underneath it:
it now publishes a generated contract for its protocol, and since v0.22.3 (2026-10-21) one supervised
gateway per machine owns every session of every profile and keeps agents running with no client
attached. That makes the
operator's goal — "Claude Code for Hermes", with many models, background agents, full configuration and
Claude-Code-style workflows across every configured model — reachable with a much thinner client than
Talaria tried to be.

## The recommendation on one screen

| Question | Answer | Where |
|---|---|---|
| Keep evolving the Python client? | No. Treat it as a specification: keep its ideas and file formats, not its modules | [02-current-state.md](02-current-state.md) |
| Does something already exist that meets the need? | No. Herm (TypeScript, OpenTUI) is the closest third-party Hermes console and is worth a look; Hermes's own terminal UI is the bar to clear; nothing targets Hermes's current gateway with Claude Code parity | [04-prior-art.md](04-prior-art.md) |
| What does Talaria attach to? | The per-machine Hermes gateway's `hermes-gateway-v1` WebSocket, with a ticket from `$HERMES_HOME/gateway.sock` — verified live from a third-party process (about 30 ms warm) | [03-hermes-today.md](03-hermes-today.md) §2 |
| How does it stay current with Hermes? | Generate protocol types from the contract Hermes publishes; pin Hermes stable releases; a scheduled drift job | [05-architecture.md](05-architecture.md), architecture decision record (ADR) draft 0008 |
| How does it get fast and clean? | Inline-first rendering: finished output goes into the terminal's native scrollback once; only a small live region redraws | [05-architecture.md](05-architecture.md), ADR draft 0009 |
| Which language? | Rust with ratatui. In a bake-off, moderate-tier agents built the same working client in Rust and in TypeScript with OpenTUI. TypeScript needed fewer fixes but missed the launch and memory budgets by a wide margin. Rust's extra friction sat in one terminal module. The fallback and the conditions for switching are named | [05-architecture.md](05-architecture.md) §Stack, [ADR draft 0010](adr-drafts/0010-rust-and-ratatui.md) |
| Background agents? | Hermes already runs them; Talaria adds a fleet view grouped by state, a needs-you queue, peek and reply, background launch in a git worktree | [05-architecture.md](05-architecture.md) §The fleet view |
| Configure Hermes from Talaria? | Schema-driven settings from Hermes's own config schema, typed screens for providers, models, Model Context Protocol (MCP) servers, skills, toolsets, profiles and cron, plus conversational configuration | [05-architecture.md](05-architecture.md) §Configuration |
| Ultracode across every model? | Talaria Workflows: Claude Code's script API, each agent a Hermes session on a model chosen from a staffing table, run by a detachable runner with a resumable journal | [06-workflows.md](06-workflows.md) |
| What happens in what order? | Seven phases, each ending in a check the operator runs themselves | [07-roadmap-and-work-units.md](07-roadmap-and-work-units.md) |
| What could go wrong, and what must the operator decide? | Ten risks with mitigations; eight decisions in blocking order; eleven upstream contributions | [08-risks-and-open-questions.md](08-risks-and-open-questions.md) |

## Reading order

1. [01-vision-and-requirements.md](01-vision-and-requirements.md) — the operator's ten points as
   testable requirements R1 to R10.
2. [02-current-state.md](02-current-state.md) — what was measured and found in Talaria 0.6.5.
3. [03-hermes-today.md](03-hermes-today.md) — the backend as it is now, and how to attach to it.
4. [04-prior-art.md](04-prior-art.md) — what exists, what to copy, and the Claude Code parity table.
5. [05-architecture.md](05-architecture.md) — the recommended design, budgets and stack.
6. [06-workflows.md](06-workflows.md) — workflows across every Hermes model.
7. [07-roadmap-and-work-units.md](07-roadmap-and-work-units.md) — the executable plan.
8. [08-risks-and-open-questions.md](08-risks-and-open-questions.md) — pre-mortem and decisions.
9. [adr-drafts/](adr-drafts/) — the decisions above as proposed ADRs.
10. [research/](research/) — the sixteen evidence reports, with the corrections between them.
11. [reference-slices/](reference-slices/) — two working minimal clients from the stack bake-off, in
    Rust and in TypeScript, to read before building.
12. [units/](units/README.md) — the reference specification behind each GitHub issue in the Talaria
    v0.7.0 issue set, indexed by phase.

## For a coding agent starting work

- **Do not start building until the operator has accepted the ADR drafts.** Until then, the accepted
  ADRs in `platform-specs/04-architecture/adrs/` still describe the Python client, and implementation
  changes must not contradict them.
- After acceptance, start at phase 0 in [07-roadmap-and-work-units.md](07-roadmap-and-work-units.md).
  Every unit names what to read first, which files it owns, the interfaces it must provide, and the
  tests that prove it done.
- Each unit is also a GitHub issue under the Objective `Talaria v0.7.0` on the Operations board, as a
  sub-issue of its phase's capability issue. The issue is the contract; its specification in
  [units/](units/README.md) holds the evidence and detail. If the two disagree, the issue wins and
  the disagreement is reported on the issue.
- Verify Hermes facts against the Hermes you are building against, not against this document. Every
  Hermes claim here was read at revision `22063fc09` (2026-10-17) or checked against v0.22.4
  (2026-10-21); Hermes merges thousands of changes per release.
- This repository is public. Keep private hostnames, private profile names, credentials and personal
  paths out of everything you commit.

## How this was produced

On 2026-10-22, fifteen research units ran in parallel (code review of Talaria, performance
measurement, history mining, the Hermes client surfaces, orchestration primitives, the Hermes release
delta, the Desktop catalog, Hermes's own terminal UI, three prior-art surveys, a stack evaluation with
benchmarks and code-generation trials, a Claude Code feature inventory, a hands-on drive of Talaria
beside Hermes's terminal UI against a live backend, and startup timing of installed agent clients).
A live attach probe verified the recommended transport. A stack bake-off then had four
moderate-tier agents build the same working client, two in Rust and two in TypeScript, against the
live gateway. Every report is in [research/](research/), and the bake-off's clients are in
[reference-slices/](reference-slices/).
