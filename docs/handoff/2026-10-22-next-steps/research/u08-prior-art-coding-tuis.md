# u08 — Prior art: coding-agent terminal UIs, and whether one can become "Claude Code for Hermes"

Research date: 2026-10-22. Read-only survey. Scope: decide whether an existing coding-agent terminal UI already does what the operator wants from Talaria (a "Claude Code for Hermes"), or could serve as the base for it, and which design patterns are worth copying.

Revisions read:
- Hermes Agent installed checkout `<hermes>` at `22063fc099` (2026-10-18 commit date; the brief calls it 22063fc09, v0.22.2 canary).
- Talaria `<talaria>` at `cafc640` (merge of PR #187; `git log -1` commit date 2026-09-25).
- openai/codex `main` at `342ed69a1` (2026-10-22); latest release `rust-v0.163.0` (2026-10-15).
- anomalyco/opencode (`sst/opencode` now redirects there) `dev` at `248bfbe11` (2026-10-22); latest GitHub release `v1.19.3` (2026-10-16).
- Other repositories were read at their default branch on 2026-10-22 through `gh api`.

Evidence conventions: `file:line@rev` for local code, `gh api` reads cited by repo path, URLs for web sources. Anything not verified is labelled **unverified** or **hypothesis**.

---

## 1. Verdict in plain English

1. **Nothing third-party is "Claude Code for Hermes" today.** GitHub searches turned up mobile apps, desktop apps, chat GUIs, Hermes Desktop plugins and a VS Code side panel. None is a coding-focused terminal client that drives Hermes and exposes Hermes-only features. The closest thing that exists is **Hermes's own first-party terminal UI** (`hermes --tui`, React + a forked Ink, in `ui-tui/`). It already has a live sub-agent dock with steer and stop controls. The second closest is any generic Agent Client Protocol (ACP) terminal client (Toad, Nori CLI, Martty, acpx) pointed at `hermes acp`. That route gets chat, tools, permissions, sessions and model switching, but none of Hermes's extras.
2. **No existing UI is a drop-in base.** Every realistic candidate is either:
   - tied to its own agent's protocol (Codex, OpenCode, Crush, Claude Code), or
   - a generic ACP client, where ACP is too narrow for background-agent rosters, sub-agent trees, cron, or editing Hermes configuration.

   The best *code* to borrow is Codex's terminal layer (Rust, ratatui, finished history written into native scrollback). The best *architecture* to copy is OpenCode 2's always-on background service with thin attachable clients. The cheapest *experiment* is Toad or Nori CLI over `hermes acp`, which needs zero code. That experiment would mainly tell us what ACP cannot carry.
3. **Pre-mortem.** The most likely way a fork fails is upstream churn. Codex's terminal UI is roughly 17 MB of Rust across 1,139 files and gets commits daily. OpenCode is rewriting into a 2.x line. A fork that rewires either one to Hermes would be rebasing against a moving target forever. Copying patterns is safer than forking code.

---

## 2. Candidate survey

Repository metadata comes from `gh api repos/<owner>/<repo>`, read 2026-10-22: stars, last push, licence, main language.

| Tool | Repo (stars, last push, licence, language) | UI stack | Screen model |
|---|---|---|---|
| Claude Code | anthropics/claude-code (152,776; 2026-10-22; no licence, proprietary; repo is issues and plugins only) | React with Anthropic's own cell-diffing renderer (replaced Ink's renderer, Jan 2026, secondary source) | Classic: main screen and native scrollback. Fullscreen: alternate screen, virtualised, mouse; now the default for many new users |
| OpenAI Codex CLI | openai/codex (130,349; 2026-10-22; Apache-2.0; Rust) | ratatui + crossterm, forked custom terminal | **Inline viewport.** Finished history is written into native scrollback; only the bottom area is redrawn |
| OpenCode | anomalyco/opencode (219,806; 2026-10-22; MIT; TypeScript) | OpenTUI (Zig core over Bun FFI) + SolidJS | Main TUI on alternate screen (OpenTUI default). `opencode mini` uses "split-footer" inline mode |
| Crush | charmbracelet/crush (28,744; 2026-10-22; **FSL-1.1-MIT**, source-available; Go) | Bubble Tea v2, Lip Gloss v2, Ultraviolet | Alternate screen |
| Gemini CLI | google-gemini/gemini-cli (107,490; 2026-10-22; Apache-2.0; TS) | Ink fork `npm:@jrichman/ink@6.6.9`, React 19 | Alternate buffer by default since v0.15 (Google blog) |
| Qwen Code | QwenLM/qwen-code (28,638; 2026-10-22; Apache-2.0; TS) | Ink 7.0.3, React 19 | Not verified |
| Kimi Code CLI | MoonshotAI/kimi-code (7,958; 2026-10-21; MIT; TS). Old Python kimi-cli archived | **pi-tui**, vendored as `packages/pi-tui` | pi-tui main-screen renderer (inferred from the dependency) |
| Toad | batrachianai/toad (3,484; **2026-05-26**; AGPL-3.0 plus commercial; Python) | **Textual 8.2.7**, same framework family as Talaria | Full-screen Textual app |
| Pi | earendil-works/pi, formerly badlogic/pi-mono (117,010; 2026-10-21; MIT; TS) | pi-tui, its own framework | Both: `TuiMainScreen` keeps scrollback; `TuiAltScreen` is optional |
| Goose | aaif-goose/goose (55,519; 2026-10-22; Apache-2.0; Rust) | CLI is line-mode (rustyline, cliclack, bat), not a full TUI. Desktop is Electron | Line-mode. Its ACP TUI was **deprecated** |
| Aider | Aider-AI/aider (49,690; 2026-05-22; Apache-2.0; Python) | prompt_toolkit line mode | Main screen. Last release v0.86.0 (2025-08-09) |
| Amp | closed source | Not stated | "Neo" rewrite, May 2026 |
| Kilo Code CLI | Kilo-Org/kilocode (27,698; 2026-10-22; MIT; TS) | Fork of OpenCode's TUI (`@opencode-ai/tui`, OpenTUI) | Same as OpenCode |
| Cline CLI | cline/cline (70,697; 2026-10-22; Apache-2.0; TS) | `@cline/cli` 3.0.73 on OpenTUI + React | Not verified |
| Factory Droid | closed source | Not found | Not found |
| Warp | warpdotdev/warp (65,624; 2026-10-22; AGPL-3.0; Rust) | A terminal emulator, not a TUI | n/a |
| Cursor CLI | closed source (`cursor-agent` 2026.10.19 installed) | Bundled JS containing `react-reconciler`, so React-based; whether it is Ink is unverified | Not verified |
| Grok Build | closed source (`grok` 1.0.62 installed) | Binary contains `ratatui-0.29.0`, `crossterm-0.28.1`, `ratatui-inline/src/terminal.rs` | Probably inline ratatui (**hypothesis** from strings). Press describes "fullscreen, mouse-aware" |
| Muse Code (Meta) | closed source (`muse` 1.4.11 installed) | String search of an older binary found no framework signature: **unverified** | Unverified |

### 2.1 Per-candidate detail

**Claude Code** (proprietary; 2.1.308 installed)
- Rendering:
  - In the classic renderer, finished output lives in native scrollback.
  - Anthropic rewrote the renderer to diff terminal cells, keeping React and dropping Ink's renderer. Secondary coverage: https://letsdatascience.com/news/anthropic-rewrites-claude-code-terminal-rendering-76034b52 and https://steipete.me/posts/2025/signature-flicker.
  - The opt-in, now often default, **fullscreen** renderer uses the alternate screen. It renders only visible messages, so "memory stays constant regardless of conversation length". It adds mouse support, and `Ctrl+o` then `[` writes the transcript back into native scrollback (https://code.claude.com/docs/en/fullscreen).
  - Background sessions attached with `claude attach` always render fullscreen.
- Client/server split: there is no public client protocol. Background sessions run under a local **supervisor** process (`claude daemon status|logs|stop`). Each session is its own process, and attach and detach never stop it (https://code.claude.com/docs/en/agent-view). The Agent SDK embeds the agent loop; it is not a remote protocol for a foreign UI.
- Foreign agents: no. Other models are reachable only through an Anthropic-compatible gateway. This machine runs Claude Code that way: live processes use `--settings .../provider-settings/ollama-cloud-...`.
- Multi-agent features:
  - Subagents with a per-agent model.
  - Agent teams (experimental; teammates in-process or in tmux panes; a mailbox at `~/.claude/teams/{team}/inboxes/*.json`) (https://code.claude.com/docs/en/agent-teams).
  - **Dynamic workflows / ultracode**: JavaScript scripts using `agent()`, `parallel()` and `pipeline()`. Up to 16 concurrent agents by default (configurable to 256), up to 1,000 agents per run. Runs can be resumed within a session and watched in a `/workflows` progress view (https://code.claude.com/docs/en/workflows.md).
- Extensibility: hooks, skills, plugins, "mods" (interface extensions), MCP.
- Memory: measured RSS of live `claude` processes on this machine was **348–659 MB** (`ps -axo pid,rss,...`, 2026-10-22; 18 processes, uneven workloads and uptimes, so not a controlled benchmark).

**OpenAI Codex CLI** (Apache-2.0; 0.163.0 installed)
- Rendering:
  - `codex-rs/tui/src/insert_history.rs:1-5@342ed69a1`: "Inserts finalized history rows into terminal scrollback. Codex uses the terminal scrollback itself for finalized chat history, so inserting a history cell is an escape-sequence operation rather than a normal ratatui render."
  - Lines 51-54 of the same file handle Zellij and unreliable scroll regions.
  - `custom_terminal.rs` is a forked ratatui terminal. Its test at `:1084` covers "leaving_alternate_screen_repaints_restored_inline_viewport", so overlays use the alternate screen temporarily and then return to the inline viewport.
- Client/server split: the terminal UI is a client of **app-server**.
  - `codex-rs/tui/Cargo.toml` depends on `codex-app-server-client`, `-daemon` and `-protocol`.
  - `app_server_connection.rs:31` calls `RemoteAppServerClient::connect_local_daemon`.
  - Protocol: JSON-RPC 2.0-style without the `jsonrpc` header. Transports are stdio (JSONL), WebSocket (experimental), and WebSocket over a Unix socket. Primitives are thread, turn and item. Methods include `thread/start|resume|fork|list`, `turn/start`, `turn/steer` and `model/list`. TypeScript or JSON Schema can be generated per version. "The app-server command and WebSocket transport are experimental and aren't supported for production workloads." Some features are gated behind an `experimentalApi` flag or limited to first-party hosts (https://learn.chatgpt.com/docs/app-server; `codex-rs/app-server/README.md`).
  - Live on this machine: `codex app-server daemon` processes and `codex app-server --listen u...` at 373 MB RSS; terminal UI client processes at 74–142 MB RSS.
- Foreign agents: not by design. The UI speaks Codex's thread/turn/item protocol. But **Nori CLI** (tilework-tech/nori-cli, Apache-2.0, 190 stars, pushed 2026-10-13) is a Codex fork. Its `NOTICE` says "includes software developed by OpenAI… OpenAI Codex". It adds an `acp-host` crate and drives Claude, Codex, Gemini "or any ACP compatible agent". That is direct evidence the Codex terminal UI *can* be rewired onto ACP.
- Multiple models: `[model_providers.<id>]` with `base_url`, `env_key` and `wire_api` (https://learn.chatgpt.com/docs/config-file/config-advanced). Built-in `ollama` and `lmstudio` providers.
- Agents: subagents and custom agent TOML files. `tui/src/multi_agents.rs:1-5` provides a `/subagents` picker and fast-switch keys. Per-agent model routing had a reported regression (community thread, July 2026, **unverified**).
- Size: `codex-rs/tui/src` holds **1,139 `.rs` files, 17.2 MB** (`gh api .../git/trees/342ed69a1?recursive=1`). The UI also depends on about 50 `codex-*` crates (Cargo.toml lines 25–80).

**OpenCode** (MIT; GitHub `v1.19.3`; v2.1.2 installed locally from a pre-release channel; npm dist-tags show `tui-v2` and `beta` channels)
- Rendering:
  - OpenTUI's `ScreenMode = "alternate-screen" | "main-screen" | "split-footer"`, defaulting to the alternate screen (`anomalyco/opentui packages/core/src/renderer.ts:240-245, 373`).
  - `opencode mini` uses the split-footer mode. `packages/tui/src/mini/runtime.lifecycle.ts:1-4`: "Lifecycle management for the split-footer renderer… writes the entry splash to scrollback".
- Client/server split: the strongest of any candidate.
  - v1: `opencode serve` runs a headless server with an OpenAPI 3.1 spec at `/doc` and Server-Sent Events at `/event`. The terminal UI "is the client that talks to the server". Optional HTTP basic auth (https://opencode.ai/docs/server/).
  - v2.1.2 (local `opencode --help`): `--standalone` means "Run with a private server instead of the background service". `--server URL` connects elsewhere. `opencode service start|stop|status` manages the background server. `opencode pair` prints "one-time links to connect a browser or app".
  - Measured: `opencode serve --service` was running at 469 MB RSS; `opencode service status` returned `http://127.0.0.1:49374`.
  - Repo layout: `packages/{protocol,client,server,sdk,tui,cli}`. `@opencode/tui` depends on `@opencode/client`, but **also on `@opencode/core`** (from package.json), so the UI is not purely a protocol client.
  - `packages/protocol/src/groups` has 31 groups, including agent, config, mcp, model, permission, provider, pty, session, skill, vcs and worktree.
- Foreign agents: no; it speaks its own protocol. It *exposes* itself over ACP (`opencode acp`; "Some built-in slash commands like /undo and /redo are currently unsupported", https://opencode.ai/docs/acp/).
- Agents: primary agents (Build, Plan) and subagents (General, Explore, Scout), with a per-agent `model`. Child sessions are navigable with leader+arrow keys (https://opencode.ai/docs/agents/). The docs do not describe background or parallel scheduling.
- Extensibility: plugins, MCP, custom tools and commands, AGENTS.md.
- Size: `packages/tui/src` holds 329 files, 2.25 MB of TS/TSX.

**Crush** (Charm)
- Licence: FSL-1.1-MIT (`gh api repos/charmbracelet/crush/license`). Source-available with a non-compete clause; it converts to MIT after the FSL period. This rules out a competing fork during that window.
- Rendering: Bubble Tea v2.1.0 (go.mod); `internal/ui/model/ui.go:3805` sets `v.AltScreen = true`.
- Split: there is a real client/server split.
  - `internal/{server,client,backend,proto}`. The server listens on a per-user Unix socket (named pipe on Windows) (`internal/server/server.go:20-66`).
  - It has `multiclient_test.go`.
  - It has an `internal/herdr` package.
- Features: multiple providers through Catwalk (`charm.land/catwalk v0.52.49`), LSP, MCP over stdio/http/sse, hooks, skills, sessions, switching models mid-session (README lines 17–20, 401–418). No ACP listing (https://agentclientprotocol.com/get-started/agents).

**Gemini CLI / Qwen Code**
- Both are Ink + React.
- Gemini CLI originated the ACP integration with Zed and is listed as an ACP agent.
- Qwen Code (started as a Gemini CLI derivative, **unverified** from its README) now claims Claude Code parity: "SubAgents, Agent Teams, Dynamic Workflows", "Multi-protocol (OpenAI / Anthropic / Gemini / Qwen + any provider)", and "Daemon Mode — `qwen serve` (multi-client shared agent)", which is HTTP + Server-Sent Events, experimental (QwenLM/qwen-code README lines 107, 138–157).
- So Qwen Code is the closest *open-source agent* to "Claude Code with many models and an always-on daemon". It is not a Hermes client.

**Kimi Code CLI**
- MIT, TS. Its terminal UI "is built on top of pi-tui" (README line 122), and the repo vendors `packages/pi-tui`.
- Has an ACP server (`kimi acp`), subagents, hooks, plugins, a `remote-control` package.
- Claims "TUI is ready in milliseconds" (vendor claim, not measured).

**Toad** (Will McGugan, Batrachian AI)
- Licence: AGPL-3.0 plus commercial (README:306).
- Stack: Python ≥3.14, `textual[syntax]==8.2.7`, `textual-speedups` (pyproject).
- Activity: last commit `dd4f90e` on **2026-05-26**, release v0.6.20. Nearly five months without a commit. Issues require an approved Discussions post first (README:296).
- Protocol: its own ACP client implementation (`src/toad/acp/{agent,api,protocol,...}.py`).
- Agent catalogue (`src/toad/data/agents/*.toml`): 21 agents (Claude, Codex/OpenAI, OpenCode, Gemini, Goose, Kimi, Grok, Cursor, …). **No Hermes entry.**
- Roadmap (README:283–292): "Expose model selection (waiting on ACP update)" is unchecked; "Multiple agents" and "Sessions (resume)" are checked.
- Rendering: partial-region refresh claimed (i-programmer, https://www.i-programmer.info/news/90-tools/18619-toad-pythonic-universal-terminal-interface-for-ai-agents.html). The app is full-screen.

**Pi**
- MIT, TS.
- `@earendil-works/pi-tui` 1.4.0: "differential rendering and synchronized output", with "`TuiMainScreen` renders into the main terminal buffer and preserves terminal scrollback" and "`TuiAltScreen` … when stopped… prints the complete final document" (packages/tui/README.md lines 3–15, 57–62). Uses CSI 2026 synchronized output.
- RPC mode: "long-lived subprocess controlled through JSON records on stdin and stdout… for… custom user interfaces" (packages/coding-agent/docs/rpc.md).
- Deliberately "skips features like sub-agents and plan mode" (coding-agent README:19). Extensions supply them.
- Listed as an ACP agent through the `pi-acp` adapter.

**Goose**
- Apache-2.0, Rust.
- `ui/text/README.md`: "goose ACP TUI — Deprecated… use… Any ACP based TUI from https://agentclientprotocol.com/get-started/clients#cli-and-tui."
- The CLI is line-mode (rustyline, cliclack). Desktop is Electron. `ui/goose-acp-client` is a TS ACP client library.

**Amp**
- Closed source. "Neo" rewrite (May 6, 2026): "remote-controllable, compaction-first, plugin-powered, and much faster".
- Vendor-reported figures for a thread of about 5,000 messages: idle memory 1,814 MB to 540 MB, mean CPU 84.1% to 17.4% (https://ampcode.com/news/neo).
- Not an ACP agent.

**Kilo Code CLI**
- Forked the whole of OpenCode in Feb 2026 (https://blog.kilo.ai/p/kilo-cli). The repo carries `packages/opencode` and `packages/tui` (`@opencode-ai/tui` on OpenTUI).
- This is evidence the OpenCode stack can be forked wholesale. It also shows the cost: Kilo now tracks a large upstream.

**Cline CLI**
- `apps/cli` (`@cline/cli` 3.0.73) on `@opentui/core` + `@opentui/react`.
- `cline --acp` exposes an ACP server.
- Parallel agents means one process per tmux pane, with no shared scheduler (https://cline.bot/blog/introducing-cline-cli-2-0; devops.com coverage).

**Factory Droid, Cursor CLI, Grok Build, Muse Code**
- All closed source. Not forkable.
- Factory Droid: `customModels` bring-your-own-key in `~/.factory/settings.json` (third-party guides).
- Cursor CLI: Auto model default; subagent logs (https://cursor.com/docs/cli/changelog).
- Grok Build: up to 8 parallel subagents in worktrees; skills, plugins, hooks, MCP (secondary coverage, https://mer.vin/2026/05/grok-build-cli-xai-terminal-coding-agent-with-plan-mode-subagents-and-headless-ci/).
- Muse Code: beta 2026-08-05; persistent background subagents (secondary, https://www.mindstudio.ai/blog/meta-muse-code-spark-coding-agent). Locally, `~/.local/share/muse/runtime/muse/ms-*.sock` Unix sockets exist, which suggests a local service process (**hypothesis**).
- Factory Droid, Cursor and Muse Spark Code are listed as ACP agents; Grok and Amp are not.

**Warp**
- The client went open source under AGPL-3.0 (May 2026; https://www.warp.dev/blog/warp-is-now-open-source).
- It is a terminal *emulator* with first-class hosting of Claude Code, Codex, OpenCode and Gemini CLI: notifications, inline review, remote control.
- Not a base for a terminal UI. Relevant only as a host Talaria should behave well inside.

### 2.2 Other ACP terminal clients found (from https://agentclientprotocol.com/get-started/clients)

| Client | Repo facts (gh api) | Note |
|---|---|---|
| Nori CLI | tilework-tech/nori-cli, 190★, 2026-10-13, Apache-2.0, Rust | **Codex terminal UI fork with an `acp-host` crate.** Bring-your-own ACP agent through `[[agents]]` in `~/.nori/cli/config.toml` |
| Martty | openma-ai/Martty, 80★, 2026-10-17, MIT, Rust | ratatui ACP client, defaults to DeepSeek Harness; ACP registry with `/harness <id>` switching |
| acpx | openclaw/acpx, 3,375★, 2026-10-21, MIT, TS | Headless CLI for stateful ACP sessions |
| hydra-acp | smagnuso/hydra-acp, 38★, 2026-10-22, MIT, TS | "Multi-client ACP session daemon: spawn coding agents, share live sessions across terminals, editors…" |
| Raven | EverMind-AI/Raven, 5,540★, 2026-10-22, Apache-2.0, Python | Splits a task into a DAG and runs nodes on built-in agents or launched ACP agents. Prior art for multi-agent workflows over ACP |
| Intelligent Terminal | microsoft/intelligent-terminal, 2,071★, C++ | Windows Terminal fork that drives ACP CLIs |

**Hermes Agent is on the official ACP agent list** (https://agentclientprotocol.com/get-started/agents), so every client in this table can drive Hermes today without code changes.

---

## 3. Question 1: does a "Claude Code for Hermes" already exist?

**No third-party one.** Evidence: `gh search repos` on 2026-10-22 for "hermes-agent tui", "hermes agent client", "hermes-agent acp", "hermes-agent desktop", "hermes agent terminal", "hermes-agent gui", "hermes ratatui", "hermes textual agent", "hermes-agent cli client", "hermes opencode", "hermes toad", "nousresearch hermes tui" and "hermes-agent claude code". The results fall into these groups:

- **Skins and add-ons for Hermes's own terminal UI**, all 0–1★: osamahbeig/hermes-skin-lab, mdgld/hermes-skins, E-R-Butch/hermes-opencode-go-widget, devnyks/hermes-session-loop.
- **Mobile and desktop chat clients**:
  - liamlai88/hermes-agent-clients (Swift, 4★)
  - acegraphx/hermes-desktop-win (WPF over SSH, 69★)
  - synthalorian/hermes-wingman (Flutter, 13★)
  - AlopecicNovice/hermes-agent-client (Tauri, 0★)
  - jeromeleeqy-hash/Hermes-Desktop (Kotlin, 5★)
- **ACP bridges and editor panels**:
  - 1919884213/hermes-panel (VS Code over ACP stdio, 1★)
  - lsaether/hermes-bridge (WebSocket to ACP, 1★)
  - xnzone/hermes-agent-acp-bridge (Rust, OpenAI-compatible HTTP, 4★)
  - Stoltemberg/buzz (Buzz Desktop with Hermes ACP runtime, 0★)
- **Integrations in the other direction**: zaycruz/hermes-opencode-plugin (62★) dispatches tasks *from* Hermes *to* OpenCode.
- **A name collision**: Ruddickmg/hermes.nvim is a generic "ACP client for Neovim", not specific to Hermes Agent.

None of these is a coding-focused terminal client. infiquetra/talaria itself shows 0★ and is public (`gh api repos/infiquetra/talaria`).

**The real "closest existing thing" is first-party.** Hermes ships its own terminal UI:
- `ui-tui/README.md@22063fc099`: "React + Ink terminal UI for Hermes. TypeScript owns the screen. Python owns sessions, tools, model calls, and most command logic." It spawns `python -m tui_gateway.entry` and speaks "newline-delimited JSON-RPC over stdio".
- It ships a forked Ink (`ui-tui/packages/hermes-ink`) and runs on the alternate screen (`ui-tui/src/entry.tsx:49`; `hermes-ink/src/ink/ink.tsx:676 enterAlternateScreen`).
- It has a "Live agents" dock: roster, transcript tail, steering form, stop and subtree-stop. These are backed by `subagent.list`, `subagent.tail` and `subagent.steer` RPCs (`ui-tui/README.md:72-90`).
- The gateway exposes a far richer method set than ACP. Grep of `tui_gateway/*.py` finds `subagent.*` (start, steer, tail, interrupt, …), `session.*` (steer, branch, compress, undo, usage, workspace.move, foreign.import, …), `config.get|set|show|yaml`, `model.options|save_key`, `cron.manage` and `agents.list`.
- Hermes Desktop (`apps/desktop`) is Electron 40.10.2 + React 19.2.7 (package.json).

So the operator's target, a Claude-Code-grade terminal client over Hermes's full backend, is partly built inside Hermes upstream. Talaria (Python + Textual, attaching to the same `tui_gateway` over WebSocket per `talaria/transport/attach.py:1-8@cafc640`) is the only independent attempt found.

**The generic ACP route works but is narrow.** `hermes acp` (`acp_adapter/entry.py:202`) constructs `HermesACPAgent()` with no session manager. That returns `GatewayACPAgent` (`acp_adapter/server.py:254-258`), "ACP projection of the profile gateway; never constructs an execution owner" (`acp_adapter/gateway_server.py:1`). So an ACP client attaches to the **always-running gateway**, which is exactly the persistence the operator wants.

What that agent offers:
- Advertised: `load_session`, image prompts, session fork, list and resume (`server.py:534-556`).
- Implemented: `set_session_model` (`gateway_server.py:441`), cancel, slash commands (`_run_slash`), and answers to permission and clarify prompts.

What it does not carry:
- Mode changes and config writes are refused. `set_session_mode` and `set_config_option` raise `acp_edit_policy_mutation_unavailable` for anything but the default (`gateway_server.py:540-552`).
- No sub-agent roster or steering methods.
- No configuration editing.
- No cron.

Toad would not even surface model switching ("waiting on ACP update").

---

## 4. Question 2: which UI codebases are realistic to fork or adapt

Ranked by fit for the operator's stated goals: fast, clean, background agents, many models, full Hermes configuration, Claude-Code-style workflows.

| Option | What it would take | Cost | Risk | Verdict |
|---|---|---|---|---|
| **A. Toad (or Nori CLI, Martty) over `hermes acp`, unchanged** | Add a Hermes agent TOML to Toad, or `[[agents]]` to Nori. Zero code | Hours | Toad: AGPL, dormant since 2026-05-26, Python ≥3.14, no model picker. Nori and Martty: small projects. All are capped by ACP: no sub-agent roster, config, cron, or Hermes slash surfaces beyond `_run_slash` | **Do it as a one-day probe**, to measure what ACP loses and to compare speed (Textual-based Toad against Talaria). Not a base |
| **B. Fork the Codex terminal UI and replace app-server-client with a Hermes `tui_gateway` client** | Rewire about 50 `codex-*` crate dependencies; map Codex's thread/turn/item model onto Hermes sessions, events and subagents. Nori CLI shows a Codex→ACP rewire is feasible | Very high: 17.2 MB / 1,139 Rust files in `tui/src` alone; upstream commits daily (main `342ed69a1` dated the day of this research) | Permanent rebase burden. Deep Codex semantics (sandbox, approvals presets, Codex Desktop extras) leak everywhere | **Do not fork wholesale.** Copy specific modules under Apache-2.0 with attribution: `insert_history.rs`, `custom_terminal.rs`, `live_wrap.rs`, `markdown_stream.rs` (file names verified in the directory listing; contents of the last two not read) |
| **C. Fork Nori CLI** (already Codex with an ACP host) | Add Hermes-specific ACP extensions or a second `tui_gateway` transport | High (inherits B's size) but the ACP plumbing exists | Small upstream (190★), two levels of fork drift | Worth reading `nori-rs/acp-host` (19 files) as a reference for a Rust ACP host. Not a base |
| **D. OpenCode's TUI against a Hermes adapter** | Either (i) a shim server that implements OpenCode's protocol (31 groups including pty, vcs, worktree, provider) on top of Hermes, or (ii) fork `packages/tui` and replace `@opencode/client`. The TUI also imports `@opencode/core` | High. 2.25 MB TS in `packages/tui/src`; Bun + Zig toolchain | Semantic mismatch with Hermes sessions. OpenCode is mid-rewrite to 2.x (local v2.1.2 against GitHub release v1.19.3), so the protocol is moving. Measured `opencode serve --service` RSS was 469 MB | **Copy the architecture, not the code.** Kilo shows a wholesale fork is possible but means owning a giant tree |
| **E. Build a new client on pi-tui (TS) or OpenTUI's split-footer mode, speaking `tui_gateway` JSON-RPC** | New client of modest size; pi-tui is a small MIT library already reused by Kimi Code | Medium | It duplicates what Hermes's own `ui-tui` already does (also TS over `tui_gateway`). Two TS clients for one backend | Reasonable if the operator wants TS. Otherwise see F/G |
| **F. Contribute to, or fork, Hermes's own `ui-tui`** | It already speaks the full gateway protocol and has the live-agents dock | Low to medium | It is Ink-based on the alternate screen, the same family whose flicker and memory reputation drove Claude Code to write its own renderer and add virtualisation. The operator called Hermes Desktop bloated; `ui-tui` is not that, but it is Node | **Strong option to evaluate.** It is the only existing client with Hermes-specific features. Measure it against Talaria before deciding (see the measurement gap below) |
| **G. Rust rewrite from scratch, ratatui inline viewport, speaking `tui_gateway` (or ACP plus Hermes `_meta` extensions)** | New client; borrow Codex's inline-history technique | Medium to high, but bounded and owned | Must reimplement markdown streaming, diff rendering, input editor. Codex modules can be lifted under Apache-2.0 | Best long-term fit for "fast and clean" **if** the Hermes protocol surface is stable enough to target. Rust TUIs measured here: Codex client 74–142 MB RSS against Claude Code's 348–659 MB (uncontrolled snapshot) |
| Not candidates | Crush (FSL non-compete), Claude Code, Amp, Cursor, Factory Droid, Grok Build, Muse Code (closed); Aider (line-mode; last commit 2026-05-22; last release 2025-08-09); Goose CLI (line-mode; its own TUI deprecated); Warp (an emulator) | — | — | — |

---

## 5. Question 3: design patterns to steal

1. **Finished history goes into native scrollback; only a small live region is redrawn** (Codex: `insert_history.rs:1-5`, viewport diagram at `:201`, Zellij-safe strategy at `:51-54`).
   - Native Cmd+F, selection and copy keep working.
   - Redraw cost no longer grows with transcript length.
   - Grok Build appears to use the same idea (`ratatui-inline` in the binary, hypothesis).
   - OpenTUI's `split-footer` mode and pi-tui's `TuiMainScreen` are the TS equivalents.
2. **Two renderers, user-switchable, sharing one component tree.**
   - Claude Code offers classic (main screen, scrollback) and fullscreen (alternate screen, virtualised, mouse), switched mid-session with `/tui` and carrying the session across a relaunch.
   - It also gives a way back to native scrollback (`Ctrl+o` then `[`).
   - pi-tui does the same at library level: one `TUI` interface, `TuiMainScreen` and `TuiAltScreen`.
3. **Virtualise the transcript.** Render only visible messages, so memory stays flat ("memory stays constant regardless of conversation length", Claude Code fullscreen docs). Amp's rewrite reports idle memory falling from 1,814 MB to 540 MB on a thread of about 5,000 messages (vendor figure).
4. **Synchronized output (CSI ?2026) plus cell-level diffing.** pi-tui uses CSI 2026 for atomic updates. Claude Code probes for synchronized-output support at startup and notes tmux ≤3.6 lacks it.
5. **A server-first split with an always-on background service and thin attachable clients.**
   - OpenCode 2: `opencode service`, `--server URL`, `--standalone`, `pair` links.
   - Claude Code: supervisor plus `claude attach`, sessions survive terminal close, idle processes reaped after about an hour but resumable from disk.
   - Crush: Unix-socket server with multi-client tests.
   - Qwen Code: `qwen serve`.
   - Hermes already has this shape (gateway plus `tui_gateway`). The pattern to copy is the **client ergonomics**: attach/detach, a list of running sessions grouped by state (Claude Code's agent view groups by "Needs input / Working / Completed"), and peek-and-reply without attaching.
6. **A generated, versioned protocol schema.** Codex app-server emits TypeScript or JSON Schema per version. OpenCode publishes OpenAPI 3.1 at `/doc`. A Hermes client should pin against a generated schema, not hand-mirrored method names.
7. **The workflow-runtime user experience** (Claude Code dynamic workflows):
   - Script-held orchestration (`agent`, `parallel`, `pipeline`, `phase`).
   - Per-agent model and effort.
   - A concurrency cap and a total-agent cap.
   - Deterministic replay for resume (`Date.now()` and `Math.random()` throw).
   - A `/workflows` drill-down: phase, then agent, then tool calls, with `p` pause, `x` stop, `r` restart.

   Raven's DAG-over-ACP-agents design is open-source prior art for running each node on any agent or model.
8. **Agent roster plus steering.** The `/subagents` picker and fast-switch keys (Codex `multi_agents.rs`), the agent panel with Enter to view and message a teammate (Claude Code), and Hermes's own live-agents dock (`ui-tui/README.md:72-90`) all work this way. Hermes already has the RPCs (`subagent.steer`, `subagent.tail`).
9. **Conversational configuration.** Kimi Code configures MCP through `/mcp-config`. Crush ships "a builtin skill for configuring itself" (README:250). This is a cheap route to "very user-friendly configuration of the Hermes backend" on top of `config.get` and `config.set`.
10. **Plugin points in the UI, not just the agent.** Claude Code "mods" (draw in the interface, react to events). Amp's plugin API (`amp.registerTool`, UI elements). Pi extensions render custom components through `ctx.ui` even in RPC mode.

---

## 6. Measurements taken (and their limits)

- **Resident memory** (`ps -axo pid,rss,etime,comm,args`, 2026-10-22). Different workloads and uptimes, so this is **not** a controlled comparison:

  | Process | RSS |
  |---|---|
  | `claude` (18 processes) | 348–659 MB |
  | `codex` terminal UI clients | 74–142 MB |
  | `codex app-server --listen` | 373 MB |
  | `opencode serve --service` | 469 MB |
  | `talaria` (3 min uptime) | 84 MB |
  | `talaria gate` benchmark run | 225 MB |
  | Hermes desktop Python backend | 420 MB |

- **Process boot as a startup proxy** (best of 3, wall clock including about 20 ms of timing overhead). This is **not** time to first frame:

  | Command | Time |
  |---|---|
  | `claude --version` | 0.041 s |
  | `codex --version` | 0.040 s |
  | `opencode --version` | 0.085 s |
  | `qwen --version` | 0.081 s |
  | `gemini --version` | 0.895 s |
  | `talaria --help` | 0.132 s |
  | `hermes --version` | 1.684 s |

- **Not measured:** time to first interactive frame, frame time while streaming, and memory on a long transcript, for any client. This is the key gap before choosing between options F and G.

---

## 7. Open questions

- How much of the operator's "slow, not clean" complaint about Talaria comes from Textual's full-screen model versus Talaria's own code? Talaria is 2.65 MB of Python source (`find talaria -name '*.py' | xargs cat | wc -c`), about four times Toad's 678 KB. A side-by-side of Toad over `hermes acp` against Talaria would separate the two.
- Will Hermes upstream stabilise `tui_gateway` as a public, versioned contract, or is ACP plus `_meta` extensions the sanctioned third-party surface? (`gateway_server.py` refuses mode and config mutations today.)
- Is Hermes's own `ui-tui` close enough that contributing upstream beats an independent client?
- Toad's licence (AGPL) against Talaria's (MIT, `LICENSE`, `pyproject.toml:11`): borrowing Toad code would force relicensing.
