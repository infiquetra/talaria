# u09: Prior art for background agents, many-model workflows, and the Agent Client Protocol

**What this is.** A research note for the Talaria next-steps handoff. Talaria (repository
`<talaria>`, `origin/main` at `cafc640`) is a third-party Python
and Textual terminal client for Hermes Agent. The operator wants it to become "Claude Code for
Hermes", including two things this unit covers: (a) several agents working in the background while
they keep working, and (b) Claude-Code-style "ultracode" workflows in which every agent can use any
model Hermes is configured with. This note surveys what other tools already do for both, and
decides what Talaria should do about the Agent Client Protocol (ACP, Zed's editor-to-agent
protocol, agentclientprotocol.com).

**Sources and revisions.** Hermes claims cite the installed checkout
`<hermes>` at revision `22063fc09` (committed 2026-10-18), unless they
say `v0.22.4` (the newest upstream release, 2026-10-21, read with `gh api`). ACP claims cite
`github.com/agentclientprotocol/agent-client-protocol` at its default branch as fetched on
2026-10-22 (copies saved under
`<scratch>/research/u09/acp/`).
Claude Code claims cite the official docs at `code.claude.com/docs/en/*.md` fetched 2026-10-22
(copies under `.../research/u09/ccdocs/`). Repository facts (stars, licence, last push) come from
`gh api repos/<owner>/<repo>` on 2026-10-22. Anything not checked against a source is labelled
**unverified** or **hypothesis**.

---

## 0. Bottom line (read this if nothing else)

1. **Background agents.** Every serious tool converged on the same four-part loop: dispatch a
   prompt into an isolated workspace (almost always a git worktree on its own branch), show one
   row per agent with a small fixed set of states (working, needs input, idle, done, failed),
   let the operator peek and reply without attaching, then review a diff and merge or open a pull
   request. Claude Code's *agent view* is the most complete and best-documented version of this
   loop and is the pattern to copy. Hermes already owns the hard part (a long-running gateway
   daemon that keeps sessions, delegations and kanban workers alive without a client), so Talaria
   needs to build the dashboard, not the supervisor.
2. **Many-model workflows.** Of the systems surveyed, Hermes itself already has the richest
   per-step *provider* choice: kanban cards take a per-task model **and provider**, and Mixture of
   Agents presets take explicit provider/model pairs per slot. Hermes's `delegate_task` is the gap:
   it has **no per-task model parameter**; every child in a batch runs on one global
   `delegation.model`. Claude Code workflows have the right *orchestration* shape (deterministic
   script, `agent()` / `parallel()` / `pipeline()` / `phase()`, schema-checked outputs, a resumable
   record of finished agents, a progress view), but their workers are Claude sessions only.
   "Ultracode for Hermes" = Claude Code's workflow shape + Hermes's per-task provider routing. That
   combination does not exist anywhere today (as far as this survey found).
3. **ACP verdict: not the main transport; a deferred, optional adapter; borrow its vocabulary
   now.** Hermes's own ACP server is a *viewer* that dials the same gateway WebSocket (`/api/ws`)
   that Talaria already speaks natively, and projects a subset of it. Using ACP to talk to Hermes
   would therefore throw away exactly the Hermes-specific features the operator wants (subagent
   steering, kanban, profiles, configuration, cron). ACP's new subagent support is still
   *unstable*, deliberately forbids the client from prompting or steering a subagent, and Hermes
   does not implement it. ACP is valuable for one thing Talaria might want later: driving
   *non-Hermes* agents (Claude Code, Codex, Gemini CLI, OpenCode) in the same dashboard. Design
   Talaria's domain events so they map cleanly onto ACP's `session/update` vocabulary, and revisit
   an ACP client transport when the subagents proposal is stable.

---

## 1. Multi-agent session managers and front ends

### 1.1 Survey table

| Tool | Form | State (2026-10-22) | Dispatch | Monitor | Review and merge | Isolation |
|---|---|---|---|---|---|---|
| Claude Code agent view (`claude agents`) | Terminal screen inside Claude Code | Research preview (agent-view.md:20) | Input at the bottom of the screen; `claude --bg` from the shell (agent-view.md:491) | Rows grouped by state; Haiku-class one-line summary per row; peek panel (`Space`) with reply; terminal notification on needs-input / finish / fail (agent-view.md:129-233) | Pull request label on the row, coloured by check status (agent-view.md:174-199); session commits and pushes, opens draft PR, never merges or pushes to main (agent-view.md:591-596) | Session moves into a git worktree under `.claude/worktrees/` before its first edit (agent-view.md:567) |
| Claude Code agent teams | Lead session + teammate sessions | Experimental, off by default, `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1` (code.claude.com/docs/en/agent-teams) | Natural language to the lead; per-teammate model can be named in the prompt | Agent panel below the prompt; in-process or tmux / iTerm2 split panes | Shared task list with dependencies and file-locked claiming; mailbox JSON files per agent | None: "Agent teams don't isolate teammates in worktrees" (agents.md) |
| Claude Code background subagents | Subagents inside one session | Default for subagents spawned from the main conversation (sub-agents page) | Claude spawns; `Ctrl+B` moves a foreground subagent to background | `/tasks` list; permission prompts surface in main session, naming the subagent | Result arrives as a completion notification in a later turn | Optional `isolation: worktree` per subagent |
| claude-squad (`smtg-ai/claude-squad`, Go, AGPL-3.0, 8,644 stars, pushed 2026-08-20) | TUI | Active | `n` new, `N` new with prompt | List of instances; preview tab and diff tab | `s` commit and push; `c` checkout (commits and pauses); `r` resume | tmux session per agent + git worktree per task (README lines 152-153) |
| Conductor (conductor.build) | macOS desktop app, closed source | Active | `Cmd+N` new workspace | Per-workspace terminal and diff (docs did not describe a central status view) | "review the diff, open a pull request, merge, and archive the workspace" | "Conductor creates these separate checkouts with Git worktrees"; new branch always from latest `origin` (conductor.build/docs/concepts/workspaces-and-branches) |
| Crystal (`stravu/crystal`, MIT) | Electron desktop | **Deprecated February 2026**, replaced by Nimbalyst (README) | n/a | n/a | n/a | Git worktrees (repo description) |
| Vibe Kanban (`BloopAI/vibe-kanban`, Rust + TS, Apache-2.0, 28,425 stars) | Local web app | **Sunsetting**: company behind it (bloop) shut down, announced 2026-04-10; repo continues as community project (README line 19; WebSearch of vibekanban.com/blog/shutdown, page itself failed to fetch with an SSL error) | Kanban issue → workspace | Board columns | Inline diff comments sent back to the agent; PR creation and merge | "each workspace gives an agent a branch, a terminal, and a dev server" |
| Sculptor (`imbue-ai/sculptor`, Python, MIT, 240 stars, pushed 2026-10-21) | Desktop app | Active, "experimental research preview" | Workspace = isolated copy of the repo; prompt an agent; add more agents to a workspace | (docs pages listed, not read) | "Review the changes when it's done and merge back to main" | Worktrees ("Workspaces: isolated worktrees of your repo") |
| Omnara (`omnara-ai/omnara`, Go, Apache-2.0) | **Pivoted**: now "an open-source managed agent platform … self-hostable alternative to Claude Managed Agents" (README) | Active | API / CLI `npx omnara agents launch --file agent.yaml` | `omnara agents chat` | n/a | Managed sandboxes ("machine pools") |
| agent-deck (`asheshgoplani/agent-deck`, Go, MIT, 1,138 stars, pushed 2026-10-21) | TUI + web mode | Active | `n` new; `o` send a message (busy Claude sessions queue it); `agent-deck session send --message-file` | Status icons Running / Waiting / Idle / Error by "smart polling"; waiting sessions in the tmux status bar; web "Fleet board" with *semantic* status columns set by `agent-deck session annotate --hint status=needs-input` (README 359-376, 1236-1268) | `agent-deck worktree finish` merges branch, removes worktree, deletes session (README 378-386) | Optional worktree per session; "Conductor" = a persistent agent that watches other sessions, auto-answers when confident, escalates to the human, optionally via Telegram/Slack (README 606-612) |
| Herdr (`herdrdev/herdr`, Rust, Apache-2.0, 45,239 stars, pushed 2026-10-21) — the operator's multiplexer | Terminal multiplexer ("the runtime your coding agents live on") | Active | Panes; agents can spawn panes and prompt each other via CLI and socket API | Every pane marked working / blocked / idle | n/a (does not touch git) | None; owns terminals, not checkouts |
| OpenCode (`anomalyco/opencode`, MIT, 219,806 stars) | TUI + headless server | Active | `POST /session/:id/prompt_async` returns 204 immediately | SSE `/event`; `GET /session/:id/children` | `POST /session/:id/fork` | None built in (not verified further) |
| Codex cloud | Cloud tasks (web, desktop, mobile, CLI) | Active | "Start a task from your terminal, or list recent cloud chats and check their status" | Task list | "Inspect changed files and check results, request follow-up changes, and commit or open a pull request" (learn.chatgpt.com/docs/cloud.md line 19) | Cloud container per task. Best-of-N `--attempts 1-4` is reported by third-party blogs only: **unverified** against OpenAI docs |
| Toad (`batrachianai/toad`, Python + Textual, AGPL-3.0, 3,484 stars) | TUI, generic ACP client | **Quiet**: last release v0.6.20 on 2026-05-26, last push same day | Launch agents from an agent store | "Concurrent sessions … Hit ctrl+s to show the current state of all your agents in one screen" (README ~line 114-118) | n/a | None |

### 1.2 How the state is detected (this matters for Talaria)

* **Claude Code agent view** knows state from inside each session process; a separate *supervisor*
  process hosts background sessions so they survive closing the terminal, restarts crashed
  sessions, stops idle unattached ones after about an hour and resumes them from disk on reply
  (agent-view.md:835-849). Script access is `claude agents --json` (agent-view.md:797-812).
* **Herdr** does not scrape the screen for state. The agent reports itself:
  `"$HERDR_BIN_PATH" pane report-agent --state idle|working|blocked --seq <n>`, with `--seq`
  ordering so a stale report cannot overwrite a newer one, and an optional resume command so herdr
  can restart the session after a server restart (herdr.dev/docs/add-herdr-support, fetched
  2026-10-22). Only cleanup is observed ("Herdr notices once the pane is back at its idle shell
  prompt").
* **agent-deck** uses "smart polling" of tmux panes (README line 361) plus explicit semantic
  annotations for the web board.
* **claude-squad** wraps tmux sessions; it reads the pane (mechanism not documented in the README;
  **unverified**).

**Lesson for Talaria.** Talaria does not need any of these detection tricks for Hermes agents: the
Hermes gateway already pushes typed events. The Hermes `tui_gateway` (the JSON-RPC surface behind
`/api/ws` that Talaria already speaks, per Talaria ADR-0001) exposes `subagent.list`,
`subagent.interrupt`, `subagent.tail` (`tui_gateway/methods_subagents.py:53,81,105`),
`subagent.steer`, `delegation.status`, `delegation.pause`, and `spawn_tree.save/list/load`
(`tui_gateway/methods_session.py:2214-2303`), plus emitted events named `subagent.start`,
`subagent.text`, `subagent.thinking`, `subagent.tool`, `subagent.complete` (string search over
`tui_gateway/*.py`). Herdr-style self-report is only relevant if Talaria ever hosts non-Hermes
agents in terminals, and then the right move is to read herdr's socket API rather than reinvent it.

### 1.3 The UX patterns worth copying

1. **Fixed state vocabulary, grouped by state.** Claude Code agent view: Working, Needs input,
   Idle, Completed, Failed, Stopped, with a separate glyph for "process exited but resumable"
   (agent-view.md:131-148). Herdr: working / blocked / idle. agent-deck: Running / Waiting / Idle
   / Error. Everyone puts "needs input" first and loudest.
2. **One-line live summary per row, written by a cheap model.** Claude Code generates row
   summaries with a Haiku-class model at each turn end and every few minutes during long turns,
   and reuses the session's own output between model calls (agent-view.md:164-172). For Hermes this
   maps onto the auxiliary model slot; **hypothesis** that Hermes's existing auxiliary client could
   serve it — not checked.
3. **Peek and reply without attaching.** `Space` opens the exact question or result; replies to a
   working session are queued, not interrupting; `/stop` stops immediately; permission dialogs
   still need a full attach (agent-view.md:201-231). Hermes already has the queueing and steering
   primitives (`subagent.steer`; delegation docs describe `steer` as "queue course-correction text
   into one child … without stopping it", `tools/delegate_tool.py:760`).
4. **Pull request status on the row.** `#1234` coloured yellow / green / purple / grey by checks
   and review (agent-view.md:174-199). Cheap to build with `gh` and very high value.
5. **Worktree before first edit, preserve work before finishing.** Commit and push the branch,
   open a draft PR, never merge or push to main, and let repo instructions override
   (agent-view.md:565-596). Conductor additionally branches from freshly fetched `origin` so a
   stale local checkout cannot seed new work.
6. **Background from foreground.** Claude Code: `←` / `/background` moves the current session to
   the background; `/fork` copies it (agent-view.md:434-470). OpenCode: `prompt_async` + SSE.
7. **A supervisor that outlives every client.** Claude Code built one; Herdr is one; Hermes's
   gateway already is one (delegation children, kanban dispatcher, cron all run inside it —
   `website/docs/user-guide/features/kanban.md` "Dispatcher … Runs inside the gateway by default").
8. **An orchestrator agent that watches the others.** agent-deck's *Conductor* and Claude Code's
   agent-team lead. Hermes's kanban orchestrator profile lane is the native equivalent
   (`kanban-worker-lanes.md` "Orchestrator profile lane", line 115).

### 1.4 What the casualties say

Three of the eleven tools changed course in 2026: Crystal deprecated (February), Vibe Kanban's
company shut down (April; stated reason: mostly free users and no business model), Omnara pivoted
from a Claude-Code companion to a managed-agent platform. Toad has been quiet since May. The
survivors are either first-party (Claude Code, Codex, OpenCode) or thin layers over terminals
(Herdr, agent-deck, claude-squad). **Inference, not verified:** a third-party dashboard that wraps
someone else's agent is fragile when the agent vendor ships the same dashboard. Talaria's defence is
that it is Hermes-native: it can show things (kanban, profiles, provider routing) that no generic
wrapper can.

---

## 2. Multi-model orchestration and workflow systems

### 2.1 Which systems let each step use a different model or provider

| System | Orchestration style | Per-step model | Per-step provider | Durable / resumable | Background | Evidence |
|---|---|---|---|---|---|---|
| Claude Code dynamic workflows (behind `ultracode`) | Deterministic JavaScript: `agent()`, `parallel()`, `pipeline()`, `phase()`, `log()`, `args`; optional JSON schema per agent | Yes: "A model the script names for a stage counts as the per-invocation model" (workflows.md:449) | **No**, Claude models only: "In every approach the workers are Claude sessions" (agents.md:19) | Resumable *within the session*: completed agents return saved results on relaunch; first changed or failed agent and everything after it rerun (workflows.md:370-386). `Date.now()` and `Math.random()` throw so replays are deterministic | Yes; `/workflows` progress view with pause, stop agent, restart agent, save script (workflows.md:82-103) | code.claude.com/docs/en/workflows.md |
| Claude Code subagents / agent teams | Model-driven (Claude decides turn by turn) | Yes: per-invocation `model`, then definition `model`, then `CLAUDE_CODE_SUBAGENT_MODEL`, then session model | No (Claude only) | Teams: no resume of in-process teammates | Yes | sub-agents and agent-teams pages |
| Hermes `delegate_task` | Model-driven fan-out, up to 10 concurrent by default (`delegation.md:45`) | **No**: "`delegate_task` has no per-task model parameter, so every child in a batch runs on the configured delegation model" (`delegation.md:292`); the model-facing schema has only `goal`, `context`, `output_schema`, `images`, `group` per task (`tools/delegate_tool.py:696-765`) | One global `delegation.provider` / `delegation.base_url` for all children (`delegation.md:264-290`) | Kanban doc says "Resumability: None — failed = failed" (`kanban.md:106`) | Yes: top-level delegations run in the background and post back (`delegation.md:11`); live `list` / `steer` / `stop` actions | installed `22063fc09` |
| Hermes kanban | Durable SQLite queue + state machine; dispatcher in the gateway | Yes: `hermes kanban create … --model … --provider …`, `hermes kanban set-model` (`kanban.md:669-683`) | **Yes**, per task | Yes: block / unblock / reclaim on crash; durable rows | Yes: "Fire-and-forget after create" | `kanban.md:97-122` |
| Hermes Mixture of Agents | Fixed per-turn pattern: reference models without tools, then aggregator with tools | Yes, per slot | **Yes**: "explicit provider/model pairs, so you can mix providers" (`mixture-of-agents.md:77`) | n/a (per turn) | n/a | `mixture-of-agents.md:54-100` |
| Hermes `/goal` | Single-session judge loop | Judge model configurable (`goals.md:237`) | Not checked | Persisted (`goals.md:217`) | Continues without re-prompting | `goals.md:24-41` |
| Mixture-of-Agents paper (Wang et al., arXiv 2406.04692, 2024-06-07) | Layers of agents, each taking all outputs of the previous layer | Yes | Yes (open models from several sources) | n/a | n/a | 65.1% AlpacaEval 2.0 with open models only vs 57.5% GPT-4o (arxiv abstract) |
| OpenAI Agents SDK | Code-defined agents, handoffs | Yes: "Within a single workflow, you may want to use different models for each agent" | Yes via LiteLLM / Any-LLM model objects, with a recommendation to keep one API shape per workflow | Not checked | Not checked | openai.github.io/openai-agents-python/models |
| LangGraph | State graph; durable execution with persistence; human-in-the-loop | Nodes are plain functions, so any client per node — **inference**, the overview page does not state it | Same inference | Yes ("resuming from where they left off") | n/a | docs.langchain.com/oss/python/langgraph/overview |
| Goose recipes and subagents | YAML recipes with `sub_recipes` (name, path, values); subagents can run in parallel, default limit 5 | `GOOSE_SUBAGENT_MODEL` / `GOOSE_SUBAGENT_PROVIDER` defaults, "recipe settings take precedence" | Yes at recipe level; per-sub-recipe provider **not documented** on the sub-recipes page | Subagents "exist only for task execution", 5-minute default timeout | Parallel, not background | goose-docs.ai subrecipes and subagents pages |
| Raven (`EverMind-AI/Raven`, Apache-2.0) | Plans a task as a DAG, runs each node on a built-in agent or an ACP agent | Per node agent | Via ACP, CLI or OpenAI-compatible APIs (README line 200) | Not checked | Not checked | ACP clients list line ~53 |

### 2.2 What "ultracode for Hermes" would actually need

The orchestration shape to copy is Claude Code's workflow runtime, because it fixes the problems
that model-driven delegation cannot: the plan lives in a script, intermediate results live in
script variables instead of a context window, and a relaunch replays finished agents from a record
rather than paying for them again. The concrete properties worth copying, all from
`workflows.md`:

* primitives `agent(prompt, {schema, label, model, …})`, `parallel()`, `pipeline()`, `phase()`;
* schema-validated structured output with bounded retries (five by default);
* determinism rules (no clock, no randomness inside the script) so replays line up;
* a concurrency cap (16 by default) and a total-agent cap (1,000 per run);
* "No mid-run user input" — sign-off happens between workflows, not inside one;
* a progress view: phases → agents → agent detail, with pause, stop agent, restart agent.

Hermes already has the routing half: kanban per-task `--model/--provider`, MoA per-slot pairs, and
named profiles each with their own `config.yaml`. What Hermes lacks is a per-call model on the
delegation primitive, which is where a workflow's `agent()` would naturally land. Two ways to close
it, neither verified as feasible:

* **Option A — workflow engine over kanban (no Hermes change).** Each `agent()` becomes a kanban
  card with a model override; `parallel()` becomes sibling cards; `pipeline()` becomes linked
  parent → child cards. Strength: durable across gateway restarts, already per-task provider,
  already visible on the dashboard. Weakness: kanban workers are full OS processes with a
  dispatcher tick (default 60 s, `kanban.md` "Dispatcher" bullet), so fan-out latency is high, and
  workers cannot call `delegate_task` (`kanban.md:122`). **Hypothesis:** acceptable for
  multi-minute coding stages, poor for many small research calls.
* **Option B — add a per-task `model`/`provider` to `delegate_task` upstream.** The plumbing
  already resolves a credential bundle per call (`tools/delegate_tool.py:407-417`,
  `_credential_overrides`), so the change looks local. **Hypothesis**, not tested; it is an
  upstream Hermes contribution, not Talaria work.

Either way the *script runtime* belongs on the Hermes side (gateway or a plugin), not inside
Talaria: the operator wants workflows to keep running when the client closes, and only the gateway
outlives the client.

---

## 3. The Agent Client Protocol

### 3.1 Version and status (verified 2026-10-22)

* Repository `agentclientprotocol/agent-client-protocol` (moved from `zed-industries`; Apache-2.0,
  4,478 stars, pushed 2026-10-22). Latest releases: library `v1.12.0` and **schema `v1.26.0`**,
  both 2026-10-16; a **v2 schema is in alpha** (`schema-v2.0.0-alpha.9`, same day)
  (`gh api …/releases`).
* Wire protocol version is the integer `1` (`docs/protocol/v1/initialization.mdx:37`).
* Transports: **stdio only** is standard. "Streamable HTTP (draft proposal in progress)"
  (`docs/protocol/v1/transports.mdx`); the remote-transport RFD proposes HTTP/2 Streamable HTTP plus
  WebSocket on one `/acp` endpoint (`docs/rfds/streamable-http-websocket-transport.mdx`).
* Python SDK `agent-client-protocol` on PyPI: latest stable `0.12.1`; `1.0.0rc3` on 2026-10-12
  (`curl https://pypi.org/pypi/agent-client-protocol/json`). Apache-2.0.

### 3.2 Features (stable schema v1 vs unstable)

Stable methods (`schema/v1/meta.json`): agent side `initialize`, `authenticate`, `session/new`,
`session/load`, `session/resume`, `session/list`, `session/close`, `session/delete`,
`session/prompt`, `session/cancel`, `session/set_mode`, `session/set_config_option`, `logout`;
client side `session/request_permission`, `session/update`, `fs/read_text_file`,
`fs/write_text_file`, `terminal/create|output|release|wait_for_exit|kill`,
`elicitation/create|complete`; plus `$/cancel_request`.

Stable `session/update` kinds (`schema/v1/schema.json`, `SessionUpdate.oneOf`): user / agent
message chunks, agent thought chunks, `tool_call`, `tool_call_update`, `plan`,
`available_commands_update` (slash commands), `current_mode_update`, `config_option_update`,
`session_info_update`, `usage_update`, `notice`, `compaction_update`, `compaction_summary_chunk`.

Unstable additions (`schema/v1/meta.unstable.json`, `schema.unstable.json`): `session/fork`,
`providers/list|set|disable`, `mcp/message` (MCP over ACP), and session updates `plan_update`,
`plan_removed`, **`subagent_update`, `session_message`, `session_message_chunk`**.

Model selection is now a session *config option* with category `model`
(`docs/protocol/v1/session-config-options.mdx:47-56`); the older `session/set_model` method does
**not** exist in either current schema (grep count 0 in both).

Extensibility: any method beginning with `_` is reserved for custom extensions, `_meta` fields
carry custom data, and custom capabilities should be advertised
(`docs/protocol/v1/extensibility.mdx:8-47,115`). This is the escape hatch a Hermes-specific feature
would have to use.

**MCP pass-through.** `session/new` carries the client's `mcpServers` (stdio, HTTP, SSE transport
types, `session-setup.mdx:380-531`).

### 3.3 Subagents in ACP (the part closest to the operator's vision)

Schema `v1.24.0` (2026-09-30) added "Initial RFD and schema for subagents" as **unstable**
(`schema/v1/CHANGELOG.md`). The RFD (`docs/rfds/subagents.mdx`, 1,258 lines, authors Vadim
Briliantov and Ben Brandt) gives each subagent its own ACP session ID linked to its parent by an
upsert `subagent_update`, so clients can render concurrent work separately (lines 8-26). Two
design decisions matter for Talaria:

* **The client may not drive a subagent.** "Client-initiated methods that modify a child session
  … are disabled unless explicitly enabled … The only mutation capability defined by this RFD is
  `cancel`" — prompting, queueing, **steering**, closing, mode and config changes are all excluded
  (lines 741-760). "The Client cannot prompt, queue, or steer it" (FAQ, line ~1062).
* **Cancelling the parent does not cascade to background children**, explicitly to match Claude
  Code's behaviour (FAQ, "Why doesn't cancelling a parent stop every descendant?").

Hermes's native gateway already allows steering a running child (`subagent.steer`), so even a
fully implemented ACP subagent projection would be *less* capable than what Talaria can do today
over `/api/ws`.

### 3.4 Clients and agents

* **Clients** (`docs/get-started/clients.mdx`): Zed (line 42); JetBrains (line 16); Emacs via
  `agent-shell.el` (line 14); Neovim via CodeCompanion, agentic.nvim, avante.nvim and
  **hermes.nvim** (lines 18-22); Obsidian plugins; VS Code extensions; CLI and TUI clients
  including **Toad**, acpx (headless CLI, MIT, 3,375 stars), Martty (Rust / ratatui), Raven, and
  Microsoft's Intelligent Terminal (lines 44-55). 169 lines of list in total.
* **Agents** (`docs/get-started/agents.mdx`): Claude Agent via the `claude-agent-acp` adapter,
  Codex CLI via `codex-acp`, Gemini CLI, GitHub Copilot, Cursor, Goose, **Hermes Agent** (line 29),
  Junie, Kimi CLI, Mistral Vibe, OpenCode, OpenHands, Pi, Qwen Code and others.
* **ACP Registry** (`https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`, URL
  from `docs/get-started/registry.mdx:1107`): 43 agents; **Hermes is not in it** (its id list has
  no `hermes` entry). Hermes is listed on the docs page but cannot be one-click installed from
  registry-aware clients. Verified 2026-10-22.

### 3.5 Hermes's ACP server as it actually is

* `hermes acp` runs `acp_adapter/entry.py:main`, which serves `HermesACPAgent` with
  `use_unstable_protocol=True` (`entry.py:201-208`). `HermesACPAgent.__new__` returns a
  `GatewayACPAgent` when no session manager is passed (`acp_adapter/server.py:254-257`), i.e. the
  default path is the gateway projection: "ACP projection of the profile gateway; never constructs
  an execution owner" (`acp_adapter/gateway_server.py:1`).
* That projection dials `connect_gateway()` (`gateway_server.py:152`), which opens the owner's
  canonical WebSocket at `…/api/ws` (`hermes_cli/gateway_client.py:24-30,189-215`) and calls
  native RPCs such as `runtime.describe` and `session.resume` (`gateway_server.py:155`, `:178`, `:308`).
  **This is the same socket Talaria dials** (Talaria ADR-0001: "dials a Hermes gateway it did not
  launch, over WebSocket … `@app.websocket("/api/ws")`"). So Hermes-over-ACP is a strict,
  lossy translation of the protocol Talaria already uses.
* Advertised capabilities: `load_session`, image prompts, session `fork`, `list`, `resume`
  (`gateway_server.py:129-138`). Emits plan, available commands, session info, usage, thought
  chunks, tool calls and permission requests (file-level grep of `acp_adapter/*.py`).
* Not implemented (grep of `acp_adapter/*.py`, zero hits): client terminals (`terminal/create`),
  client file system (`fs/read_text_file`, `fs/write_text_file`), `subagent_update`,
  `config_option_update`, `current_mode_update`. Commands run on the gateway host (`acp.md:80`).
* `delegate_task` is rendered as an ordinary tool call (`acp_adapter/tools.py:773`), so children
  are flattened into the parent.
* Toolset "intentionally excludes … messaging delivery and cronjob management" (`acp.md:40`).
* Model switching uses the older unstable `session/set_model` and `SessionModelState`
  (`gateway_server.py:441-460`, `gateway_session_meta.py:67-84`), which the current ACP schema has
  dropped in favour of config options; `set_config_option` returns an empty option list
  (`gateway_server.py:544-552`).
* At `22063fc09`, in gateway mode only the default edit mode could be selected
  (`gateway_server.py:534-542`, raising `acp_edit_policy_mutation_unavailable`). Upstream fixed this
  on 2026-10-19 (commit `8729dd9e7`, "restore Accept Edits / Don't Ask as the session's owner-held
  edit policy"), and that fix is in `v0.22.4` (`gh api …/compare/8729dd9e7...v0.22.4` → `ahead`,
  `behind_by 0`).
* Hermes pins the ACP Python SDK at `agent-client-protocol==0.9.0` (`pyproject.toml:418`; still
  `==0.9.0` on upstream `main`, line 422), a release from 2026-03-26 — seven months and three minor
  versions behind.
* Hermes is also an ACP *client* in one place: the `copilot-acp` provider launches an ACP agent
  command as a model backend, and delegation can carry `acp_command` / `acp_args` from trusted
  config, hidden from the model (`tools/delegate_tool.py:216-217,414-415,790`;
  `agent/auxiliary_client.py:6137`). Behaviour not exercised here.

### 3.6 Gaps that stop ACP from carrying Hermes-specific features

| Hermes feature the operator wants | Native gateway (`/api/ws`) | ACP today | Gap |
|---|---|---|---|
| See subagents separately | `subagent.start/text/thinking/tool/complete` events, `subagent.list`, `subagent.tail`, `spawn_tree.*` | Unstable `subagent_update`; Hermes emits none; delegations flattened into one tool call | Spec unstable + not implemented |
| Steer or stop one subagent | `subagent.steer`, `subagent.interrupt`, `delegation.pause` | RFD allows only `cancel`, by design forbids steering | Design-level gap |
| Profiles | `profiles.create` and per-call profile home (ADR-0001 notes `_profile_home`) | One agent process = one profile (gateway checks `profile_id`, `gateway_server.py:155-157`) | No concept |
| Configuration of the backend | `config.get`, `config.set` (`methods_config.py:287`, `methods_config_set.py:504`), `model.options`, `connectors.*`, `vault.*` | Session-scoped config options only; unstable `providers/*` covers provider endpoints, not Hermes config | No concept beyond session options |
| Cron | `cron.manage` (`methods_tools.py:1302`) | Excluded from the ACP toolset (`acp.md:40`) | No concept |
| Kanban | Gateway-side files reference kanban (`methods_session.py`, `session_lifecycle.py` …) | Kanban tools are ACP tool calls only | No board concept |
| Remote / always-on backend | WebSocket to a long-lived daemon | stdio subprocess; remote transport is a draft RFD | Transport gap |
| Many models per workflow | Kanban per-task model/provider; MoA presets | One `model` config option per session | No per-step concept |

Everything in the right-hand column could be smuggled through `_hermes/...` extension methods and
`_meta`, but then no other ACP client would understand it, which removes the only reason to use
ACP.

---

## 4. Verdict

### 4.1 Reuse — patterns

* **Agent dashboard = Claude Code agent view, fed by Hermes gateway events.** States (needs input
  first), cheap-model row summary, peek and reply with queue semantics, `/stop`, attach, PR label
  coloured by checks, terminal notification on needs-input / done / failed.
* **Worktree discipline** from agent view and Conductor: isolate before first edit, branch from
  freshly fetched `origin`, commit and push and open a draft PR at the end, never merge or push to
  main, and let repo instructions override. Hermes kanban already has `worktree` workspaces
  ("Preserved on completion", `kanban.md` core concepts), so map Talaria's "dispatch to background"
  onto a kanban card with a `worktree:` workspace or onto a gateway session; do not build a
  client-side process supervisor.
* **Semantic status annotations** (agent-deck): let an agent or the operator set
  needs-input / ready-for-review / parked / done independently of process state.
* **Workflow runtime shape** (Claude Code workflows): `agent/parallel/pipeline/phase`, schema
  outputs, deterministic replay of finished agents, concurrency and total caps, a three-level
  progress view with pause / stop / restart, and "save this run as a command". Combine with Hermes
  per-task provider routing (section 2.2).
* **Self-reported agent state with sequence numbers** (Herdr), if Talaria ever shows non-Hermes
  agents. Better still, integrate with Herdr's socket API rather than duplicate it, since the
  operator already runs Herdr.

### 4.2 Reuse — protocols and code

* **Protocol vocabulary, not wire protocol.** Model Talaria's domain events on ACP's names and
  shapes: `tool_call` / `tool_call_update` with kind and status, `plan` entries, permission
  options, `available_commands_update`, config options with categories (`model`, `mode`,
  thought level), `usage_update`, `notice`, and the subagent association model (parent session,
  child session, title, state snapshot). It costs nothing now and makes a later ACP adapter a
  mapping exercise. This is consistent with Talaria's ADR-0002 (framework-free domain core).
* **Code.** Licences decide what can be copied into MIT-licensed Talaria: ACP schema and Python
  SDK (Apache-2.0), Herdr (Apache-2.0), Vibe Kanban (Apache-2.0), agent-deck (MIT), OpenCode (MIT),
  Sculptor (MIT), acpx (MIT) are compatible with attribution. **Toad and claude-squad are
  AGPL-3.0**: read them for ideas, do not copy code. Toad is the closest technical neighbour
  (Python + Textual + ACP + concurrent sessions), which makes it the best reference for Textual
  rendering of agent streams — by reading, not copying.

### 4.3 ACP as a Talaria transport: options

* **Main transport — reject.** Hermes's ACP server is a viewer over the same `/api/ws` gateway
  Talaria already speaks; going through it would remove subagent steering, kanban, profiles,
  configuration and cron, which are the features the operator asked for. It would also tie
  Talaria to a Hermes ACP adapter that pins a seven-month-old SDK and still uses a dropped method.
  And it would put Talaria in direct competition with Toad and a dozen other generic ACP clients
  on their ground instead of on Hermes-native ground.
* **One of several transports — defer.** Worth doing only for the "many agents" half of the
  vision when the agents are *not* Hermes (for example a Claude Code or Codex session beside Hermes
  sessions in one dashboard). Revisit when (a) the ACP subagents proposal leaves unstable, (b) the
  remote HTTP / WebSocket transport is accepted, and (c) the operator actually wants non-Hermes
  agents in Talaria. Until then it is scope without a user.
* **Non-goal for the core — yes, for now.** Write it down as a decision with a "revisit when"
  condition (the three triggers above) rather than leaving it implicit.

**Pre-mortem of this verdict.** The most likely way it fails: the native gateway protocol churns
faster than Talaria can follow (the installed gateway registers 151 `@method` handlers across
`tui_gateway/*.py`, and upstream merged 1,338 commits between `22063fc09` and `v0.22.4`), while ACP
offers a slow-moving, versioned contract. The mitigation is to treat the gateway like a versioned
API anyway — capability discovery (`gateway.capabilities`, `client.capabilities` exist as methods),
recorded contract tests, and the ACP-shaped domain model above — so that if churn wins, a fallback
to ACP for the basic chat path is cheap.

---

## 5. Open questions

1. Does the operator want non-Hermes agents (Claude Code, Codex) inside Talaria at all? That single
   answer decides whether an ACP client transport ever earns its cost.
2. Would Nous accept a per-task `model` / `provider` on `delegate_task`? Without it, "ultracode for
   Hermes" must run on kanban (durable, but slow fan-out) or on profiles.
3. Where should the workflow script runtime live — a Hermes plugin, a gateway method family, or a
   separate daemon? It must outlive the client, which rules out Talaria itself.
4. Is Herdr integration (reading its socket API for non-Hermes panes) more valuable than any
   in-Talaria multi-agent view for non-Hermes agents?

## 6. Things that surprised me

* Hermes's ACP server is not an independent agent loop any more; by default it is a projection
  over the gateway WebSocket Talaria already uses. The Hermes ACP doc still says "ACP sessions are
  tracked by the ACP adapter's in-memory session manager" (`acp.md:361`), which contradicts the
  code (`entry.py:201` "this process is a viewer"; `server.py:254-257`).
* Hermes docs contradict themselves on `delegate_task`: kanban's comparison table says the parent
  "Blocks until child returns" (`kanban.md:104`) while the delegation page says top-level
  delegations "run in the background automatically" (`delegation.md:11`).
* ACP's subagent design explicitly forbids client steering — the one subagent feature Hermes
  already offers natively.
* Hermes is listed on the ACP agents page but absent from the ACP Registry (43 agents).
* Three of the surveyed multi-agent managers died or pivoted in 2026 (Crystal, Vibe Kanban,
  Omnara), and Toad has been quiet since May.
* Herdr's working / blocked / idle state is reported by the agent itself with sequence numbers, not
  inferred from the screen.
* Claude Code workflows already support a per-stage model, but only Claude models; Hermes already
  supports per-task providers, but not in its fast delegation primitive. Neither side has both.
