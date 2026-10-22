# 04 — Prior art: what already exists, what to take from it

Status: `proposed`

Research reports are cited by number; for example, u11 is research/u11-language-framework.md, the
language and framework evaluation.

Evidence: [u08](research/u08-prior-art-coding-tuis.md) (coding-agent terminal UIs),
[u09](research/u09-prior-art-multiagent-acp.md) (background-agent managers, workflow systems, the
Agent Client Protocol), [u10](research/u10-prior-art-hermes-ecosystem.md) (third-party Hermes
clients), [u12](research/u12-claude-code-parity.json) (Claude Code's feature inventory). Repository
facts were read with the GitHub API on 2026-10-22.

## Verdict

**Nothing that exists today is "Claude Code for Hermes", and nothing is a drop-in base for it.**

- **Third-party Hermes terminal clients exist, but none targets the current backend or Claude Code
  parity.** The ecosystem search ([u10](research/u10-prior-art-hermes-ecosystem.md)) found 29 terminal
  projects built for Hermes; most are monitors, launchers or remote wrappers. The most complete is
  **Herm** (`liftaris/herm`, MIT, TypeScript on OpenTUI and Bun, 301 stars, about 1,250 commits, almost
  all by one author; last stable release 1.10.0 on 2026-07-10, last prerelease and push 2026-07-29):
  chat on the left, and a sidebar with tabs for model and profile state, sessions, context, agents,
  analytics, skills, cron, toolsets, config, environment, memory and kanban. It is the closest existing picture of "Hermes Desktop in a terminal" and is worth
  ten minutes of the operator's time to judge that layout. It predates Hermes v0.22: it attaches to a
  dashboard or spawns a local gateway subprocess, the two paths Hermes now treats as legacy, and it
  reads some state straight from the Hermes home directory. Two small Rust clients exist as well —
  `inaki/talaria` (ratatui, 0 stars, same name as this project, speaks the old standard-input/output
  protocol) and `0xNyk/hermes-tui` (1 star) — plus a read-only monitor with 934 stars
  (`joeynyc/hermes-hud`). The large third-party clients are web and desktop apps
  (`nesquena/hermes-webui`, 19,093 stars; `fathah/hermes-desktop`, 14,448 stars).
- **Name collision.** `talaria` is taken on crates.io (an unrelated ring-buffer crate), on npm, and on
  the Python Package Index, and another Hermes client on GitHub uses the name. Publish binaries
  from GitHub releases and a Homebrew tap, or pick a distinct package name.
- **The closest existing thing is first-party: Hermes's own terminal UI.** It already has a live
  subagent dock with steer and stop, a queue for typed follow-ups, and a main-screen mode. It is Node
  with a 27,000-line fork of the Ink renderer, alternate-screen by default, and took 3 to 4 seconds
  to a usable prompt. ([03-hermes-today.md](03-hermes-today.md) §5)
- **Any Agent Client Protocol (ACP) terminal client can drive Hermes today with zero code** (Hermes is
  listed on the ACP website's agents page but is not in the machine-readable ACP Registry, so a
  registry-driven client needs it added by hand; `hermes acp` attaches to the same always-running
  gateway). That gets chat, tools, permissions, sessions and model switching. It loses everything that makes Hermes different:
  subagent steering, background processes, profiles, configuration, cron, kanban, queue editing. ACP's
  new (unstable) subagent design deliberately forbids a client from steering a subagent.
- **Forking a big agent UI is a trap.** OpenAI Codex's terminal UI is about 17 MB of Rust in 1,139
  files with commits every day; OpenCode is mid-rewrite to its 2.x line; Crush's licence (FSL-1.1-MIT)
  forbids a competing fork for now; Toad is AGPL-3.0 and has had no commits since 2026-05-26; Claude
  Code, Amp, Cursor, Factory Droid, Grok Build and Muse Code are closed. Kilo Code shows that a
  wholesale fork of OpenCode works, and also what it costs: tracking a giant upstream forever.
- **So: copy patterns, borrow small permissively licensed pieces with attribution, and build a client
  that is Hermes-native.** That is the one ground no generic wrapper can occupy. Three of the eleven
  multi-agent managers surveyed died or pivoted in 2026 (Crystal, Vibe Kanban, Omnara); the survivors
  are first-party or thin layers over terminals.

## Candidates at a glance

| Tool | Stack | Screen model | Client/server split | Why it is not the base |
|---|---|---|---|---|
| Claude Code | TypeScript, React with Anthropic's own cell-diffing renderer | Main screen and native scrollback by default; optional full-screen | Local supervisor for background sessions; no public client protocol | Closed; Claude-only workers |
| OpenAI Codex CLI | Rust, ratatui + crossterm, forked terminal | **Inline viewport; finished history written into native scrollback** | Terminal UI is a client of the `app-server` JSON-RPC daemon | Huge, daily churn, Codex semantics everywhere. **Best code to learn from** (Apache-2.0) |
| OpenCode | TypeScript on Bun, OpenTUI (Zig core) + SolidJS | Alternate screen; `opencode mini` uses a split-footer inline mode | **Always-on background service with thin attachable clients** (`opencode service`, `--server URL`, `pair`) | Its own protocol, mid-rewrite. **Best architecture to copy** |
| Crush | Go, Bubble Tea v2 | Alternate screen | Unix-socket server with multi-client tests | FSL licence |
| Gemini CLI, Qwen Code | TypeScript, Ink | Alternate screen (Gemini) | Qwen Code has an experimental `qwen serve` daemon | Own agents; Qwen Code is the closest open agent to "Claude Code with many models", but it is not a Hermes client |
| Pi, Kimi Code | TypeScript, pi-tui | Main screen with scrollback, or alternate screen | RPC mode over standard input/output | Own agents; pi-tui is a small MIT library worth reading |
| Toad | Python, Textual | Full-screen | Generic ACP client | AGPL, dormant, ACP-only |
| Nori CLI | Rust (a fork of Codex's terminal UI) | Inline | ACP host crate | Proof that Codex's UI can be rewired onto another protocol; small project |
| Hermes's own terminal UI | TypeScript, forked Ink | Alternate screen; `--native` main-screen mode | Viewer of the gateway | The bar to clear, not a base: upstream owns it |
| Herm (`liftaris/herm`) | TypeScript on Bun, OpenTUI | Full screen with sidebar and tabs | Dashboard attach or a spawned local gateway (both legacy since v0.22) | One author, stale since July, about 3 MB of TypeScript including tests; the best reference for an operator-console layout |

## Patterns to take

1. **Finished history goes into native scrollback; only a small live region redraws.** Codex
   (`codex-rs/tui/src/insert_history.rs`), Claude Code's default renderer, pi-tui's main-screen mode
   and OpenTUI's split-footer all converge here. Native search, selection and copy keep working, and
   redraw cost stops growing with transcript length. This is the single biggest fix for "slow" and
   "not clean".
2. **Synchronized output plus cell-level diffing**, negotiated at startup rather than assumed
   (multiplexers such as tmux before 3.6 do not support it).
3. **An optional full-screen view with virtualized history**, for reading back a long session with a
   pager, as Claude Code's full-screen mode and Codex's transcript overlay do.
4. **A server-first split with thin attachable clients.** Hermes already has the server half (the
   gateway); copy the client ergonomics from OpenCode and Claude Code: attach and detach, background
   launch from the shell, and pairing links.
5. **An agent dashboard grouped by state.** Claude Code's agent view: rows grouped as needs-input,
   working, idle, done, failed; a one-line summary per row written by a cheap model; peek and reply
   without attaching; a pull-request label coloured by check status; terminal notifications on
   needs-input, done and failed. Herdr (a terminal multiplexer, like tmux) and agent-deck use the same
   small state vocabulary.
6. **Worktree discipline for background work.** Isolate before the first edit, branch from freshly
   fetched `origin`, end with a pushed branch and a draft pull request, never merge or push to main
   without the operator. (Claude Code agent view, Conductor, claude-squad.)
7. **A generated, versioned protocol schema.** Codex's app-server emits TypeScript and JSON Schema per
   version; OpenCode publishes OpenAPI. Hermes already publishes OpenRPC; use it.
8. **The workflow runtime's user experience** from Claude Code: script-held orchestration, per-agent
   model and effort, concurrency and total-agent caps, deterministic replay, and a drill-down progress
   view with pause, stop and restart. See [06-workflows.md](06-workflows.md).
9. **Conversational configuration.** Crush ships a skill for configuring itself; Kimi Code configures
   Model Context Protocol (MCP) servers through a command. For Hermes, an agent session with a
   configuration skill can propose a
   change, and the client shows it as a reviewable diff before applying it with `config.set`.
10. **UI extension points**, not only agent extension points: Claude Code mods, Pi extensions and
    Amp's plugin API let users add panels and status segments. Talaria already has one good example:
    the external status-line command.

## What not to copy

- **ACP as the main transport.** It would throw away the Hermes features the operator asked for.
  Model Talaria's internal event vocabulary on ACP's names (tool call and update, plan, permission
  options, config options, usage) so an ACP adapter for *non-Hermes* agents is a mapping exercise
  later, if the operator wants Claude Code or Codex sessions in the same fleet view.
- **Alternate screen by default.** Every tool that started there either added a main-screen mode
  (Hermes `--native`, OpenCode `mini`) or kept the main screen as its default (Claude Code, Codex).
- **A client-side process supervisor.** Hermes's gateway already supervises sessions, delegations,
  cron and kanban. Talaria shows them; it does not host them.

## Claude Code parity checklist

The full inventory — 238 features, 119 slash commands, 46 built-in tools and 33 hook events, each
labelled "ui" (the client must draw it or take the input), "agent" (the backend does the work) or
"both", with documentation links — is in [u12](research/u12-claude-code-parity.json). The table below
is the subset a daily driver needs, mapped to what Hermes provides and to the roadmap phase in
[07-roadmap-and-work-units.md](07-roadmap-and-work-units.md) that delivers it. "Client" means Talaria
must build it; a Hermes method name means the backend already does the work.

| Claude Code feature | Hermes provides | Phase |
|---|---|---|
| Streaming output; Esc and Ctrl+C interrupt | `message.*` events; `session.interrupt` | 1 |
| Multi-line composer, `/clear` | Client; a new session | 1 |
| Status line | Client (keep Talaria's external-command contract) | 1 |
| Permission prompts | `approval.request` events, `approval.respond`; `clarify.*`; `sudo`/`secret` requests | 1 |
| Resume and continue sessions | `session.list`, `session.resume` | 1 |
| Tool-call cells and a transcript view (Ctrl+O) | `tool.start`, `tool.complete` (summaries, diffs); a pager overlay in the client | 2 |
| Queued messages, send now, take back | Durable queue on `prompt.submit`; `prompt.steer_queued`, `prompt.replace_queued`, `prompt.cancel` | 2 |
| `@` file mentions, slash menu, custom commands, skills | `complete.path`, `complete.slash`, `commands.catalog`, `slash.exec` | 2 |
| Image paste, large-paste collapse | `image.attach_bytes`, `clipboard.paste`, `paste.collapse` | 2 |
| `!` shell mode | `shell.exec` | 2 |
| `/btw` side questions | `prompt.btw` | 2 |
| Task checklist | `todo.updated` | 2 |
| Rewind and checkpoints, `/diff` | `rollback.*`, `rollback.session_diff`, `session.mutate` rewind | 2 |
| Model picker, effort, fast mode | `model.options`, `session.mutate` model and reasoning; Hermes `/fast` | 2 |
| Context view, compaction | `session.context_breakdown` (owner-served), `session.mutate {operation: "compress"}` | 2 |
| Cost and usage | The `session.usage` event, `session.stats`, `usage.bars` | 2 |
| Name, branch and export sessions | `session.mutate` rename and branch; `session.export` (v0.22.3+) | 2 |
| Plan mode | Partial: Hermes `/plan` writes a plan to `.hermes/plans/` without executing; approval modes `manual`/`smart`/`off`. A true read-only planning mode is an upstream gap | 2 + upstream |
| External editor (Ctrl+G), history search (Ctrl+R), keybindings, suspend (Ctrl+Z) | Client | 2 |
| Themes | Hermes skins arrive in `gateway.ready`; client applies them | 2 |
| Bell and notifications when done or waiting | Client | 2 |
| Headless print mode (`-p`) | Client over `session.create` + `prompt.submit` + `prompt.receipt` | 2 |
| `/loop`, `/goal` | Hermes's own `/loop` and `/goal` (`session.automation`, `session.control`) | 2 (pass-through) |
| Agent view, background sessions (`--bg`), attach | Client fleet view over `session.list` and events; sessions run in the gateway | 3 |
| Background tasks list, background shells | `process.list/kill/stop`, `agent.terminal.output`, `prompt.background` | 3 |
| Subagents, background subagents, stop all | `delegate_task` (one child model per profile), `subagent.*`, `delegation.pause` | 3 |
| Git worktrees (`-w`), pull-request status in the footer | Client (git and `gh`), session working directory | 3 |
| Settings screen, hierarchy | Hermes config schema, `config.get/set`; Talaria's own settings file | 4 |
| MCP servers, plugins, hooks, sandboxed terminal backends | `mcp.*`, `plugins.manage`, Hermes hooks, terminal backends | 4 |
| Instruction files and memory (CLAUDE.md, auto memory) | Hermes context files and memory providers | 4 (configure) |
| Model fallback chains | Hermes fallback providers | 4 |
| Scheduled routines | `cron.manage`, canonical `cron.*` | 4 |
| Dynamic workflows, progress view, per-agent model, saved workflows | Talaria Workflows ([06-workflows.md](06-workflows.md)) | 5 |
| Ultracode | Talaria's ultracode toggle plus the `talaria-workflows` skill | 5 |
| Vim editing mode | Client | 6 |
| Streaming JSON input and output for scripts | Client | 6 |
| Artifacts, cloud sessions, Remote Control, IDE integrations | Out of scope (Anthropic services; Hermes has its own ACP adapter for editors) | — |
