# u15 — Multi-agent workflows ("ultracode") on Hermes

**What this is.** A read-only survey of the installed Hermes Agent at `<hermes>`, revision `22063fc099` (commit date 2026-10-18 08:02 -0700, "chore(catalog): review disclosure for hermes-seo"). It asks one question for the Talaria next-steps handoff: how could a Claude-Code-style workflow engine run on Hermes, so that a deterministic script fans work out to many agents, each on its own model and effort, then verifies and synthesizes, with a resumable journal and a live progress tree?

**Why it matters now.** The operator wants Talaria to become "Claude Code for Hermes", including workflows that can use every model Hermes is configured with, and agents that keep working after the operator walks away. The answer depends on which Hermes primitives exist today, and which side (client or backend) can hold the orchestration.

**Talaria reference point.** Talaria `HEAD` is `cafc640` (commit date 2026-09-25 17:08 -0400, a merge of PR #187; the brief said 2026-09-16, but `git log -1` in the Talaria repo shows 2026-09-25). Talaria last pinned Hermes revision `3236f4405` (2026-09-14). The installed Hermes is 21,434 commits past that pin (`git rev-list --count 3236f4405..HEAD`), and upstream v0.22.4 is 1,338 commits past the installed revision (GitHub compare API `22063fc099...v0.22.4` reported `ahead_by: 1338`).

Evidence convention: `path:line` refers to the installed Hermes at `22063fc099` unless it says Talaria. "Unverified" or "hypothesis" marks anything I could not confirm from a current source.

---

## Headline answers

1. **`delegate_task` cannot pick a model, provider, toolset, or effort per child.** Its schema has only `goal`, `context`, `output_schema`, `images`, and `group` per task (`tools/delegate_tool.py:706-776`). Every child in a call uses one route: `delegation.model` / `delegation.provider` from `config.yaml`, otherwise the parent's model (`tools/delegate_tool.py:647-648`, `tools/delegate_tool_config.py:425-444`). This is still true in upstream v0.22.4 (I fetched its `tools/delegate_tool.py` through the GitHub API and found no `model`, `provider`, or `toolsets` property).
2. **Per-agent model choice exists elsewhere in Hermes, in four places:**
   - `session.create` takes `model`, `provider`, and `reasoning_effort` on the dashboard sidecar (`tui_gateway/contracts/sessions.py:118-139`). The gateway's own session authority takes those plus `toolsets`, `cwd`, `skills`, `max_turns`, and more (`gateway/session_policy.py:14-18`).
   - Kanban tasks carry `model_override`, `provider_override`, and `reasoning_effort` (`hermes_cli/kanban_db.py:1183-1185`), and the gateway applies them when it runs the card (`gateway/session_kanban.py:52-54`).
   - The plugin-only Public Subagent Lifecycle API takes `model` and `provider` for each launch (`agent/subagent_lifecycle.py:49-61`).
   - Plugin LLM calls (`ctx.llm`) can pick a provider and model once the operator opts in.
3. **Several independent top-level sessions, each on its own model, can run in parallel on one backend today.** The gateway runs each turn on an unbounded thread pool, one live turn per session. The only admission cap is `max_concurrent_sessions`, which is unset by default (`gateway/run.py:4062-4072`). The gateway's durable "admission" queue keeps an accepted prompt running after every viewer disconnects (developer doc `website/docs/developer-guide/gateway-local-sessions.md:28-33`).
4. **A client can list the whole configured model inventory with the `model.options` RPC.** The same data is at REST `GET /api/model/options`. It returns provider rows with models, `authenticated`, `auth_type`, per-model pricing strings, capabilities, rate-limit state, and subscription usage windows (`tui_gateway/contracts/config_free_tier_control.py:443-566`). Mixture of Agents presets appear as a virtual `moa` provider.
5. **Hermes already ships the backend half of a workflow engine. It lacks the deterministic-script layer.** Kanban is a durable DAG: tasks with parents, per-task models, idempotency keys, retries, and a gateway-hosted dispatcher. `hermes kanban swarm` builds the exact shape "N parallel workers, then a verifier, then a synthesizer" (`hermes_cli/kanban_swarm.py:1-13`). Hermes also ships its own model-driven version of the pattern as the optional `dynamic-workflow` skill. What is missing is a durable, client-independent runner that executes a *script*: loops, conditionals, schema-forced results, a journal, and per-agent model choice in one place.

**Recommendation in one paragraph.** Build "Talaria Workflows" in three steps:
- **Step 1, client-side.** A script engine that runs in the client and maps each `agent()` call to one hidden gateway session, created with an explicit model, provider, and effort. Each prompt is submitted as a durable admission and keyed in a local journal, so a crashed or closed client can resume and re-attach without re-running finished work. This works on today's Hermes with no upstream change.
- **Step 2, backend-side.** A "compile to Kanban" path for workflows whose shape is a static graph, so they make progress with the client closed.
- **Step 3, upstream.** Propose a backend workflow runner to Hermes: a parentless, durable launch API with a per-unit model, plus `workflow.*` events. Then the full script semantics survive client exit.

Pre-mortem for this plan: step 1 is most likely to fail at resume, because a script that is not deterministic on replay produces different journal keys. See section 4.9.

---

## 1. Orchestration primitives that exist today

### 1.1 `delegate_task` (the model-facing subagent tool)

**Parameters** (`tools/delegate_tool.py:696-776`):

| Field | Meaning | Evidence |
|---|---|---|
| `tasks[]` (min 1) | one subagent per entry, run in parallel | `:711-754` |
| `tasks[].goal` (required), `context` | self-contained brief; the child sees nothing of the parent conversation | `:718-728` |
| `tasks[].output_schema` | a JSON Schema the child's final answer must match. The parent validates it and allows **one bounded correction retry**. The result gains `schema_valid` and `schema_errors`, and the raw text is kept as `summary` | `:729-735`; doc `website/docs/user-guide/features/delegation.md:55-78` |
| `tasks[].images` | up to 8 paths or URLs | `:736-742` |
| `tasks[].group` | result-delivery bucket. Only shown to the model when `delegation.independent_completions` is on | `:743-750`, `:680-685` |
| `action` | `spawn` (default), `list`, `steer`, `stop` | `:756-764` |
| `subagent_id`, `message` | targets for `steer` and `stop` | `:765-770` |

**Fields it does not have.** There is no per-task `model`, `provider`, `toolsets`, `reasoning_effort`, or `max_iterations`:
- The schema comment says the legacy `role` is accepted but "ignored: capability is depth-derived" (`:707-710`). The code confirms it: `effective_role = "orchestrator" if _get_orchestrator_enabled() and child_depth < max_spawn else "leaf"` (`tools/delegate_tool.py:234`).
- A caller-supplied `max_iterations` is ignored in favour of `delegation.max_iterations` (`:529-535`).
- Children's toolsets are always inherited from the parent (`toolsets=None  # always inherit the parent's toolsets`, `tools/delegate_tool.py:439`). Comment at `:96`: "there is no model-facing toolsets argument".
- `acp_command` and `acp_args` exist on tasks, but they are stripped from model input (`_MODEL_HIDDEN_TASK_FIELDS`, `:790-796`). They are reachable from trusted config only.

**Model routing for children:**
- One route per profile: `delegation.model`, `delegation.provider`, `delegation.base_url`, `delegation.api_key`, `delegation.api_mode` (`tools/delegate_tool_config.py:425-444`). The tool description tells the model: "Children inherit the parent model unless pinned via delegation.provider / delegation.model in config.yaml" (`tools/delegate_tool.py:647-648`).
- An internal-only `credentials_cfg` lets Python callers route one call differently. `/review` uses it to route to `auxiliary.review` (`tools/delegate_tool.py:537-541`). It is not reachable from the model or from RPC.
- Live config on this machine (read with the Hermes venv's Python; secret-looking keys redacted): `delegation.model` and `delegation.provider` are empty, so children inherit the main model. The main model is `muse-spark-1.3-contributor` on provider `opencode-go`.

**Limits** (all in `tools/delegate_tool_config.py`):

| Knob | Default | Notes |
|---|---|---|
| `delegation.max_concurrent_children` (or env `DELEGATION_MAX_CONCURRENT_CHILDREN`) | 10 | Floor 1, no ceiling (`:17`, `:93-108`). A larger batch is a tool error, not truncation (doc `delegation.md:211`). |
| background-delegation cap | equals `max_concurrent_children` | When full, a new async dispatch is **rejected, not queued** (`:121-132`). |
| `delegation.max_spawn_depth` | 1 (flat) | Floor 1, no ceiling (`:21`, `:151-163`). Depth check at `tools/delegate_tool.py:519-526`. |
| `delegation.oneshot_max_children` | 2 | Total children allowed in a finite one-shot session (`:85-90`). |
| `delegation.max_iterations` | 250 | `tools/delegate_tool.py:105`. |
| `delegation.child_timeout_seconds` | none | Stall detection by heartbeat instead (`:24-27`, `:139-149`). This machine sets 600. |
| `delegation.worktree_isolation` | false | Gives each child its own git worktree (`:115-119`). |
| `delegation.independent_completions` | false | Delivers results per task or per group instead of one message per call (`:110-113`). |

**Children never get** `delegate_task` (when they are leaves), `clarify`, `memory`, `send_message`, or `cronjob_manage`, and never the `kanban` toolset (`tools/delegate_tool_toolsets.py:14-22`, `:55-64`, `:121`).

**How children report back:**
- Only the child's final summary returns, with optional schema validation.
- Top-level delegations always run in the background. "Dispatch returns live transcript paths and results re-enter as a new message when subagents finish ... only BETWEEN your turns" (`tools/delegate_tool.py:613-620`, `:780-787`).
- Completions are written to `state.db` (table `async_delegations`) before delivery, and survive a restart for up to 48 hours (`tools/async_delegation.py:47-56`).
- **Running children do not survive owner exit.** A child whose owner process dies is recorded as outcome "unknown" (`tools/async_delegation.py:282-300`; doc `delegation.md:219-233`). "/stop, /new, or process exit halts running subagents (whole tree)" (`tools/delegate_tool.py:627-628`).
- Each child has a live transcript at `cache/delegation/live/<id>/task-<n>.log` (`tools/delegate_tool.py:561-580`).

**How a client observes and steers children** (the dashboard sidecar RPC, `tui_gateway`):
- Events `subagent.spawn_requested`, `subagent.start`, `subagent.progress`, `subagent.thinking`, `subagent.tool`, and `subagent.complete`. Payload fields include `subagent_id`, `parent_id`, `child_session_id`, `delegation_id`, `depth`, `model`, `toolsets`, input, output, and reasoning tokens, `api_calls`, `files_read`, `files_written`, `status`, `summary`, and `duration_seconds` (`tui_gateway/contracts/events.py:574-612`). That is enough to draw a live spawn tree.
- RPCs: `subagent.list`, `subagent.interrupt`, `subagent.tail` (the last 16 KB of a child's transcript) (`tui_gateway/contracts/profiles_vault_complete_foreign_subagents.py:673-700`); `subagent.steer` (`tui_gateway/contracts/billing_delegation_pets.py:401-419`); `delegation.status`, which returns the active tree, the pause flag, and the limits (`:376-385`); `delegation.pause`, a global switch that blocks new spawns (`:388-397`).
- `spawn_tree.save`, `spawn_tree.list`, and `spawn_tree.load` persist finished tree snapshots (`tui_gateway/contracts/sessions.py:644-703`).

### 1.2 Public Subagent Lifecycle API (plugins only)

`agent/subagent_lifecycle.py` (472 lines; added 2026-07-12 in `1865fb5fcd`). Doc: `website/docs/developer-guide/subagent-lifecycle-api.md`.

- `SubagentLaunchRequest` takes `goal`, `context`, `role`, **`model`**, **`provider`**, `allowed_toolsets`, `blocked_tools`, `working_directory`, `parent_session_id`, `correlation_id`, `metadata`, and `timeout_seconds` (`agent/subagent_lifecycle.py:49-61`). Provider precedence is request, then delegation config, then parent (doc `:75-84`).
- The handle supports `status`, `wait`, `cancel`, `steer`, `result`, and `reconnect`.
- **Limits that matter for workflows:**
  - It needs an active agent turn as parent: "No active Hermes parent session is available" (`agent/subagent_lifecycle.py:280`).
  - Children are in-process threads and do not survive a restart; `reconnect` returns `RECONNECT_UNAVAILABLE` (doc `:57-63`).
  - Terminal results are kept for one hour (`_TERMINAL_RETENTION_SECONDS = 3_600`, `:29`).
  - `blocked_tools`, `working_directory`, and per-launch timeouts are rejected for now (doc `:65-71`).
  - There is no `output_schema` field.

### 1.3 Kanban (durable multi-agent board)

Doc: `website/docs/user-guide/features/kanban.md` (1,469 lines). Code: `hermes_cli/kanban_*.py`, `tools/kanban_tools.py`, `gateway/session_kanban.py`.

- **Tasks.** A task is a row in a durable SQLite board. Its fields include assignee profile, status, `parents` (a dependency graph), `idempotency_key`, `max_runtime_seconds`, `max_retries`, `model_override`, `provider_override`, `reasoning_effort`, `goal_mode`, `completion_contract`, and `skills` (`hermes_cli/kanban_db.py:1180-1201`).
- **The model-facing `kanban_create` tool also takes `model`, `provider`, `parents`, and `idempotency_key`** (`tools/kanban_tools.py:1113-1145`).
- **Execution.** The dispatcher runs inside the gateway (`kanban.dispatch_in_gateway: true`, ticking every 60 s; `gateway/kanban_watchers.py:231`).
  - At this revision the spawned subprocess is only a waiter. `_worker_argv` returns `python -m hermes_cli.kanban_worker_client` ("The profile owner executes; this subprocess only waits for its receipt", `hermes_cli/kanban_db_dispatch.py:2745-2748`), and that client "submit[s] one claim to its profile owner; never run[s] an agent" (`hermes_cli/kanban_worker_client.py:1`).
  - The owner turns the claim into an ordinary durable admission, with the task's model, provider, and reasoning applied (`gateway/session_kanban.py:1`, `:52-58`).
- **Concurrency:** `kanban.max_in_progress` and `kanban.max_in_progress_per_profile`, both unlimited by default (doc `kanban.md:1009-1018`).
- **Swarm helper:** `hermes kanban swarm "<goal>" --workers a,b,c --verifier r --synthesizer w` creates a root "blackboard" card (shared context stored as JSON comments), N parallel worker cards, a verifier gated on all workers, and a synthesizer gated on the verifier. It commits the graph atomically (`hermes_cli/kanban_swarm.py:1-13`; doc `kanban.md:1060-1070`).
- **Observability.** REST under `/api/plugins/kanban/`: board, tasks, links, `POST /dispatch` (nudges the dispatcher so it skips the 60 s wait), `GET /workers/active`, and `GET /runs/{id}`. A WebSocket at `/events?since=<id>` tails the append-only `task_events` table (doc `kanban.md:871-896`, `:1047-1058`).
- **Security note.** The dashboard's HTTP auth middleware skips `/api/plugins/`, so the kanban REST surface is unauthenticated on localhost (doc `kanban.md:913-919`).
- **Caveat.** The installed Kanban doc says "a kanban task executes in an isolated managed worker ... that worker is not offered `delegate_task`" (`kanban.md:122`). Upstream commit `bf2b3713ab` (2026-10-20, in v0.22.4) is titled "fix(managed-worker): delegate_task works in Kanban workers through owner-reserved children". I saw the commit title only, not its diff.

### 1.4 Mixture of Agents (MoA)

- A virtual provider `moa` whose "models" are named presets. Each preset has reference models (advisors without tools) and an aggregator that acts and is billed for the run. `fanout` is one of `user_turn` (default), `per_iteration`, or `every_n:N` (doc `website/docs/user-guide/features/mixture-of-agents.md:9-140`). Code: `agent/moa_loop.py`, `hermes_cli/inventory.py:1036` (`_moa_provider_row`).
- Events `moa.reference`, `moa.aggregating`, `moa.progress`, and `moa.phase` (`tui_gateway/contracts/events.py:615-647`).
- Live config: preset `default` has 5 references (`openai-codex:gpt-6-sol`, `ollama-cloud:deepseek-v4-flash:0731`, `ollama-cloud:kimi-k3`, `ollama-cloud:nemotron-3-ultra`, `gemini:gemini-3.6-flash`), aggregator `openai-codex:gpt-6-sol`, `fanout: user_turn`.
- **Relevance:** MoA is an ensemble *inside one agent turn*, not a workflow. A workflow engine can still use a MoA preset as an agent's model (`provider: moa, model: <preset>`), because it is a normal provider row.

### 1.5 Other primitives checked

| Primitive | What it is | Fit for workflows | Evidence |
|---|---|---|---|
| `/goal` | A judge model re-prompts the **same** session until done (a "Ralph loop") | A per-agent "keep going" mode; no fan-out | doc `goals.md:7-31`; RPC `goal.pause`, `goal.resume`, `goal.clear`, `goal.unwait` (`config_free_tier_control.py:699-705`) |
| `/loop` | Scheduled re-runs within a session | Not a workflow | `loop.pause`, `loop.resume`, `loop.stop` (same file) |
| `cron` | Scheduled jobs. The gateway authority has durable `cron.submit`, `cron.status`, `cron.cancel`, and `cron.recover` | A scheduled trigger for a workflow | `tui_gateway/contracts/canonical.py:355-398` |
| `prompt.background` | "Run a task on a fresh agent in the background". The answer arrives as a `background.complete` event. Params are `text` only, so no model choice | Weak; no model, no schema | `tui_gateway/contracts/prompt_voice.py:223-235` |
| `execute_code` | Python script that calls Hermes tools over RPC. **Cannot call `delegate_task`** | A deterministic layer inside an agent; cannot fan out | doc `code-execution.md:9-21`, `:153` |
| Hosted rooms (`groups.*`) | Group chat where members are **profiles**; turn-based discussion across local profile owners | A "council" feature, not a fan-out engine | `tui_gateway/contracts/groups_bot_relay.py:1-8`, `:181-196`; doc `developer-guide/hosted-profile-owners.md` |
| `agents.list` | **Background terminal processes**, not agents (rows hold `command`, `status`, `uptime`) | Misleading name; not an agent roster | `tui_gateway/contracts/tools_commands.py:70-85` |
| "foreign subagents" contract file | Despite the file name, `session.foreign.*` browses and imports **Claude Code / Codex session histories**. `subagent.*` in the same file is the delegate_task roster | Import of Claude Code history only | `tui_gateway/contracts/profiles_vault_complete_foreign_subagents.py:1-8`, `:562-632` |
| `ctx.llm` (plugin LLM access) | Single completions and structured (JSON Schema) calls with gated `provider` and `model` overrides | Good for cheap "judge" or "verify" steps inside a backend plugin | doc `developer-guide/plugin-llm-access.md:209-300`, `:393-470` |
| `dynamic-workflow` skill (optional) | Hermes's own "plan-in-code fan-outs, adversarial verification, waves": `execute_code` does the deterministic part, `delegate_task` batches do the judgment part, and results go to files | **Direct prior art.** Model-driven, so no per-unit model and no durability ("Work that must outlive this process ... use `cronjob` or the kanban swarm instead") | `optional-skills/autonomous-ai-agents/dynamic-workflow/SKILL.md:1-120` |
| "workflow" / "pipeline" names | `hermes_cli/kanban_workflow.py` is Kanban column definitions. `plugins/teams_pipeline` is a Microsoft Teams meeting pipeline. Neither is an orchestration engine | none | file headers |

---

## 2. Many concurrent top-level sessions, each with its own model, on one backend

**Yes, through two different session hosts.** Which one Talaria reaches depends on the WebSocket it dials.

### 2.1 Dashboard sidecar (`tui_gateway`, what Talaria uses today)

- Talaria dials the dashboard's `/api/ws`, which "hands the socket to `tui_gateway.ws.handle_ws`" (Talaria `talaria/transport/attach.py:1-9` at `cafc640`). In the installed Hermes, the dashboard route `/api/ws` is described as the "JSON-RPC sidecar for the Chat tab" and calls `tui_gateway.ws.handle_ws` (`hermes_cli/web_routers/chat_ws.py:696-735`). A dashboard is running on this machine (`hermes_cli.main -p default dashboard --port 8765`, seen with `ps`).
- `session.create` accepts `model`, `provider`, `reasoning_effort`, `fast`, `service_tier`, `hidden`, `close_on_disconnect`, `parent_session_id`, and `idempotency_key` (`tui_gateway/contracts/sessions.py:118-150`). The developer doc says a provider/model pair the provider cannot serve is refused up front with JSON-RPC `-32602` plus suggestions (`developer-guide/programmatic-integration.md:62-64`).
- **Execution model:** each `prompt.submit` starts a daemon `threading.Thread` for the turn (`tui_gateway/methods_prompt.py:880-887`). Long RPC handlers share a pool of `HERMES_TUI_RPC_POOL_WORKERS` workers, default 8 (`tui_gateway/server.py:180-202`). With `dashboard.turn_isolation` (default false, `hermes_cli/config_defaults.py:1020`), turns run in a compute-host child process on a thread pool of the same default size (`tui_gateway/compute_host.py:66-74`, `:542-546`).
- **Per-session process cost:** a slash-command worker subprocess is spawned lazily on first `slash.exec` (`tui_gateway/methods_tools.py:1063-1080`), not per session.
- **I found no session-count cap in the sidecar.** This is a negative finding from grepping `tui_gateway/*.py` for `max_sessions` and similar; I did not read every file, so treat it as unverified.
- **On client disconnect:** a running turn keeps going while it shows activity (`ws_orphan_activity_stale_s`, default 600 s), then it is interrupted and reaped. An idle orphaned session is reaped after `ws_orphan_reap_grace_s`, default 20 s. Sessions with live background delegations are kept (`tui_gateway/server.py:131-162`; `tui_gateway/session_lifecycle.py:975-1000`). So on the sidecar, **a session's work survives a short disconnect, but it is not a durable queue.**
- **Model switch mid-session:** `config.set {key: "model", session_id, value, confirm_expensive_model}` (`tui_gateway/methods_config_set.py:125-165`), or `command.dispatch "/model …"` (doc `programmatic-integration.md:234-243`). An "expensive model" confirmation gate can return `confirm_required` (`tui_gateway/model_switch.py:261`). A workflow engine has to pass `confirm_expensive_model: true` deliberately, or set the model at create time.

### 2.2 Gateway session authority (new since Talaria's pin)

- The gateway process now serves its own native `/api/ws` ("every local CLI/TUI/Desktop session", `gateway/run_api.py:1-12`), backed by a canonical "session authority". The doc `developer-guide/gateway-local-sessions.md` first appears in history at `0190774d73` (2026-10-08), after Talaria's 2026-09-14 pin. The gateway is running on this machine (`ps` shows a ~3.5-day-old process logging to `~/.hermes/logs/gateway.error.log`).
- **Launch fields frozen at creation** (`gateway/session_policy.py:14-18`): `request_id`, `source`, `cwd`, `model`, `toolsets`, `provider`, `base_url`, `reasoning`, `max_turns`, `ignore_rules`, `api_key`, `editor`, `yolo`, `safe_mode`, `ignore_user_config`, `skills`, `checkpoints`, `accept_hooks`, `pass_session_id`, `follow_profile_config`, `service_tier`, `cwd_explicit`, `run_budget` (a time budget in seconds, not money; `:252-255`), `verbose`, `row_cwd`, `client_env`. `runtime.describe` exposes which of these are accepted (doc `:117-120`).
- **Durable admissions:**
  - `prompt.submit({session_id, submission_id, text, unattended, finite, …})` returns an `AdmissionStatus` (`queued`, `started`, `unknown`, or `terminal`) (`tui_gateway/contracts/canonical_sessions.py:196-210`, `:295`; `canonical.py:24-36`).
  - `prompt.receipt({session_id, admission_id, include_result})` returns the structured turn result and usage once terminal (`canonical.py:39-56`).
  - "Closing every viewer leaves the execution and its pending approval alive" (doc `:28-33`).
  - Never-started queued inputs resume after a restart. Started-but-interrupted ones become `unknown` and are never replayed (doc `:121-126`).
  - `request_id` makes `session.create` idempotent, including after the daemon dies (doc `:17-27`).
- **Concurrency:**
  - "Actual bound: one live turn per session ... `max_concurrent_sessions` is the only admission cap and is unset by default" (`gateway/run.py:4062-4072`). This machine has it unset (`config.yaml` has no `max_concurrent_sessions` at the top level or under `gateway`).
  - The OpenAI-compatible API server has its own `gateway.api_server.max_concurrent_runs`, default 10 (`gateway/platforms/api_server.py:1287-1289`).
  - The doc cites a loopback test with three simultaneous sessions holding distinct requested models (doc `:164-170`; `tests/gateway/test_session_policy.py`). I did not run it.
- **Process model:** sessions are threads in the one gateway process per profile (or per multiplexer). There is no process per session (`gateway/run.py:4062-4072`).

### 2.3 Real limits a workflow engine will hit

- **Provider rate limits and subscription windows.** These are the binding constraint, not Hermes. `model.options` exposes them per provider: `limit` gives `scope: account|models` and `resets_at`, and `usage.windows` / `usage.accounts` give `used_percent`, `resets_at`, and `state: ready|limited|unknown|unavailable` (`config_free_tier_control.py:474-527`).
- **Memory and CPU of the single owner process.** Not measured. Treat as a hypothesis to measure before raising widths above about 10.
- **Background delegation inside an agent** is capped at `max_concurrent_children`, and the extra is rejected rather than queued (section 1.1).

---

## 3. Enumerating "every model Hermes is configured to use"

- **RPC `model.options`** (sidecar) takes `{profile?, session_id?, explicit_only, include_unconfigured, refresh}` and returns `{providers: [...], model, provider}` (`tui_gateway/contracts/config_free_tier_control.py:443-566`). Each provider row (`ModelOptionProvider`, `:530-556`) has:
  - `slug`, `name`, `models[]`, `total_models`, `is_current`, `is_user_defined`, `source`, `aliases`, `api_url`
  - **`auth_type`, `authenticated`, `key_env`, `warning`**
  - `featured_models`, `unavailable_models`
  - **`capabilities{model: {fast, ultrafast, reasoning, can_disable_reasoning}}`**
  - **`pricing{model: {input, output, cache, free, discount_percent…}}`**. These are *formatted $/Mtok strings*, with `""` meaning unknown (`:451-462`). They are not numbers.
  - `pricing_pending`, `free_tier`, **`limit`**, **`usage`**
- **The same payload over REST:** dashboard `GET /api/model/options` (header `X-Hermes-Session-Token`) and API-server `GET /api/model/options` (bearer key). Passing `refresh=1` busts the cache and probes saved custom providers (doc `programmatic-integration.md:158-184`). The shared builder is `hermes_cli/inventory.py:75` (`build_models_payload`) and `:226` (`build_model_options_payload`).
- **MoA presets** appear as provider `moa` (`hermes_cli/inventory.py:1036`).
- **Not checked live.** I did not call `model.options` against the running dashboard, because that needs the session token and the unit is read-only. I also did not run the builder in-process, because it may write pricing caches. So I cannot say how many providers or models this machine reports. Config shows no `providers:` or `custom_providers:` entries in `~/.hermes/config.yaml`, but other providers (`openai-codex`, `ollama-cloud`, `gemini`, `opencode-go`) are referenced by the MoA preset and the main model. Their credentials live outside `config.yaml` (hypothesis: env or credential pools).
- **Credential state per account:** `ProviderUsageAccount.state` (`:497-516`), and `model.save_key` / `model.disconnect` to add or remove keys.
- **Cost after the fact:** the `Usage` result carries `cost_usd` and `cost_status` (`tui_gateway/contracts/common.py:43-44`). Session list rows carry `actual_cost_usd` and `estimated_cost_usd` (`:125-126`). `session.usage` events stream tokens, `api_calls`, `rate_limit`, `model`, and `provider` mid-turn (`events.py:285-309`). `subagent.complete` carries per-child token counts (`events.py:574-603`).

A workflow engine's "model roster" is therefore `model.options` filtered to `authenticated == true`, minus `unavailable_models`, annotated with parsed pricing and live `limit` / `usage`. It should re-read the roster on each run, because limits change.

---

## 4. Design sketch: "Talaria Workflows"

### 4.1 Where the script runs: three options

| | A. Client-side engine (Talaria process) | B. Compile to Kanban (backend graph) | C. Backend workflow runner (Hermes plugin or core) |
|---|---|---|---|
| Works on today's Hermes? | **Yes** | **Yes** | **No.** Needs upstream changes (4.8) |
| Script semantics (loops, conditionals, retries on validation failure, dynamic fan-out width) | Full | Static graph only. Dynamic choices need an orchestrator *agent* to call `kanban_create` | Full |
| Per-agent model / provider / effort | Yes, through gateway `session.create` | Yes, `model_override`, `provider_override`, `reasoning_effort` per card | Yes, if the runner uses gateway admissions; the plugin lifecycle API has model and provider but needs a parent turn |
| Survives client exit | Running agents finish (gateway admissions are durable). **No new step starts until the client returns** | **Yes, fully** (dispatcher in gateway) | **Yes, fully** |
| Survives backend restart | Queued admissions replay; started ones become `unknown` | Yes (reclaim and retry) | Depends on the journal design |
| Schema-forced results | Client validates and sends a correction turn | Not built in. `completion_contract` is PR-acceptance oriented (`hermes_cli/kanban_pr_acceptance.py`); results are free text | Can mirror `delegate_task`'s `output_schema` |
| Latency per step | Low (event-driven) | Dispatcher tick of 60 s, which `POST /dispatch` can shorten | Low |
| Progress UI | Journal plus live session events | `task_events` WebSocket | New `workflow.*` events |
| Fits "always-running agent" | Partly | **Yes** | **Yes** |

**Recommendation:** build A first, add B as an "unattended" execution mode for static-shape workflows, and pursue C upstream. Reasoning:
- A gives full Claude-Code-style script semantics on today's Hermes. Because each agent call is a durable gateway admission, the client being closed costs *progress*, not *work*.
- B covers the operator's "send it and walk away" case without upstream changes. Its cost is that the script must be a static graph, or the dynamic part must be delegated to an orchestrator agent, which loses determinism.
- C is the right end state, but it depends on Nous accepting a new subsystem. Prototyping A on the client first is the cheapest way to settle the API worth proposing.

### 4.2 Script language

- **Python** is the recommendation if Talaria stays Python (ADR-0004), and it is still a strong pick if the client moves to Rust: the same script runs unchanged in option C, because Hermes plugins are Python. A Rust client would run the engine as a `uv run` subprocess speaking JSON lines (this is a design proposal, not something I measured).
- **TypeScript/JS** matches Claude Code's workflow scripts (the prior art the operator wants). It can be embedded in Rust with a JS engine (hypothesis; not evaluated here), but it does not port to a Hermes plugin.
- **Starlark** is deterministic by construction, which makes resume safe, but authors would find it unfamiliar and it cannot do I/O.

Whichever language is chosen, the API should be tiny and replay-safe: `agent()`, `parallel()`, `pipeline()`, `judge()`, `log()`, and `budget()`, with no wall-clock reads or randomness except through engine-provided, journaled helpers.

### 4.3 Mapping the API onto Hermes calls (option A, gateway authority)

```
agent(prompt, model=, provider=, effort=, toolsets=, cwd=, schema=, skills=, max_turns=, key=)
  1. key := explicit key or hash(script_id, call_path, prompt, model, provider, effort, toolsets, schema)
  2. journal hit (terminal)        -> return the cached result
     journal hit (admission known) -> prompt.receipt(session_id, admission_id, include_result=true); re-attach
  3. session.create({request_id: "wf:<run>:<key>", source: "tool", hidden: true, model, provider,
                     reasoning: effort, toolsets, cwd, skills, max_turns})        # idempotent on request_id
  4. prompt.submit({session_id, submission_id: "wf:<run>:<key>:0", text: brief(prompt, schema),
                    unattended: true, finite: true})                              # durable admission
  5. session.resume(session_id) to stream events into the progress tree (optional; multi-viewer)
  6. on a terminal receipt: validate against the schema; if invalid, ONE correction turn
     (submission_id ...:1) carrying the validator errors verbatim (the delegate_task pattern)
  7. journal.append({key, session_id, admission_ids, status, result, schema_valid, usage, cost_usd})

parallel(thunks, width=)      -> bounded semaphore over agent() calls (4.7)
pipeline(stages)              -> sequential agent() calls, each fed the previous stage's validated result
judge(question, evidence, schema) -> agent() on a cheap model with a forced schema (verify / refute step)
```

[Correction, 2026-10-22: `unattended: true` auto-approves every dangerous command in the turn; the handoff's runner never sends it. See [06-workflows.md](../06-workflows.md).]

Notes:
- `source: "tool"` keeps workflow sessions out of human session pickers ("`tool` ... keep[s] their own stored label so human pickers hide them", `gateway-local-sessions.md:68-73`), and avoids the coding-posture default ("never for `source: tool`", `:76-79`). That means **toolsets must be passed explicitly**.
- For the dashboard sidecar as a fallback target, steps 3–4 become `session.create {model, provider, reasoning_effort, hidden, idempotency_key}` plus sidecar `prompt.submit`, with completion read from `message.complete`. This loses durable admissions: work is reaped about 600 s after the client leaves (section 2.1).
- A nested `delegate_task` inside a workflow agent still works. Its children inherit that session's model, and it streams `subagent.*` events that the progress tree can nest under the agent.

### 4.4 Schema-forced structured results

- Hermes has no per-turn `response_format` on `prompt.submit` (I checked `CanonicalSubmitParams`, `canonical_sessions.py:196-210`; there is no schema field).
- **Option A therefore enforces schemas client-side:** put the schema in the brief ("return ONLY the JSON value"), validate with `jsonschema`, and allow one correction turn. This is the same contract `delegate_task` uses (`tools/delegate_tool.py:729-735`; doc `delegation.md:55-78`).
- Keep the raw text as well, so that a failed validation still yields evidence.
- For short, tool-free judge steps, a backend plugin could use `ctx.llm.complete_structured(json_schema=…)`, which requests `response_format` natively (doc `plugin-llm-access.md:242-280`).

### 4.5 Resume journal

- One append-only JSONL file per run, under Talaria's data directory, plus a run manifest: script hash, inputs, the roster snapshot from `model.options`, and Hermes `runtime.describe` identity and epoch.
- **Resume = re-execute the script.** Each `agent()` call whose key has a terminal record returns the stored value. A call with a non-terminal admission re-attaches through `prompt.receipt`. A call with `unknown` status (started, then the owner restarted) is surfaced to the operator, never silently re-run. This mirrors Hermes's own "never replay unknown work" rule (`gateway-local-sessions.md:121-126`).
- The idempotency keys (`request_id`, `submission_id`) mean a resumed client cannot create duplicate sessions or prompts even if it crashed between the RPC and the journal write.

### 4.6 Progress UI

- **Tree:** run → stage (`pipeline`/`parallel`) → agent (session) → nested subagents (`subagent.*` events from that session, which carry `subagent_id`, `parent_id`, and `depth`, so they nest).
- **Per node:** model and provider, state (`queued`/`started`/`terminal`/`unknown`, from the admission), tokens and cost (`session.usage` / `Usage.cost_usd`), last tool (`tool.start` events), and the live tail (`subagent.tail` for delegated children).
- **The tree must render from the journal alone**, so it works after the client reconnects and before live events arrive.
- **For option B**, render the tree from the kanban `task_events` WebSocket and `GET /tasks/:id`.

### 4.7 Concurrency caps

- **Engine-level global cap**, defaulting to 10 to match Hermes's own defaults (`max_concurrent_children`, `api_server.max_concurrent_runs`).
- **Per-provider caps** derived from `model.options` `limit` and `usage`. Skip or queue units for a provider whose state is `limited` until `resets_at`.
- **On a rate-limit failure:** halve the provider's width for the rest of the run and retry the failed unit once. This mirrors the operator's own fleet policy in `~/.claude/CLAUDE.md`. The engine must recognise the failure from the turn result: the API server reports `turn_exit_reason`, and provider errors appear as `failed`. I did not confirm the exact error shape on the canonical receipt, so that part is a hypothesis.
- **Respect Hermes-side caps:** `max_concurrent_sessions` if the operator sets it (the gateway rejects new sessions at the cap, `gateway/run_inbound.py:1281`); `kanban.max_in_progress` in option B.

### 4.8 Cost tracking

- **Before a run:** estimate cost from the `model.options` pricing strings. This means parsing "$x/Mtok" text, where `""` is unknown and `"free"` is free. Show the estimate as a range, and say "unknown" for unpriced models; never show 0.
- **During a run:** accumulate `cost_usd` per session from the `session.usage` RPC result, which carries `cost_usd` and `cost_status` (`common.py:43-44`). For subscription providers, show `usage.windows[].used_percent` instead of dollars.
- **Budget guard:** `budget(usd=…)` stops admitting new units once actual spend plus in-flight estimates would cross the limit. Hermes's `run_budget` is a time budget, not money (`gateway/session_policy.py:252-255`), so a money budget has to be enforced client-side.
- **MoA:** charge the aggregator's provider (doc `mixture-of-agents.md:13-15`).

### 4.9 Pre-mortem: how this most likely fails

1. **Non-deterministic scripts break resume.** If `call_path` or prompt text depends on timestamps, dict ordering of live data, or a model's free-text output fed straight into a later prompt, replay produces new keys and re-runs paid work. Mitigation: journal every input to a key, and expose `now()`, `random()`, and fan-out-width decisions only through journaled helpers.
2. **Talaria is attached to the dashboard sidecar, not the gateway authority.** The durability story in option A holds only on the gateway's native `/api/ws`. Talaria must add a second attach target and authentication path: gateway bootstrap tickets, which I did not investigate here.
3. **Hermes churn.** The gateway authority is weeks old. Installed `22063fc099` is 1,338 commits behind v0.22.4, and seven commits to `gateway/session_policy.py` landed between 2026-10-18 and 2026-10-20 alone (GitHub commits API). Pin a revision at v0.22.3 or later and gate on `runtime.describe` capabilities.
4. **Provider limits, not Hermes, cap throughput.** A 10-wide fan-out on one subscription provider will hit account windows. The roster-aware per-provider scheduler is not optional.

### 4.10 Upstream Hermes changes that option C (backend-side workflows) would need

1. **A parentless, durable launch API.** The lifecycle API (`agent/subagent_lifecycle.py`) needs an active parent turn, keeps results for one hour, and cannot reconnect after a restart. A workflow runner needs units launched as gateway admissions with per-unit `model`, `provider`, `reasoning`, `toolsets`, and `cwd`. The ingredients exist in `gateway/session_policy.py` and `gateway/session_kanban.py`; there is no public entry point.
2. **A runner host and a journal table.** An owner-side runner (plugin or core) that executes the script on the gateway, with an append-only `workflow_runs` / `workflow_steps` table in `state.db`, keyed like admissions, with "unknown" semantics on restart.
3. **RPC and events.** `workflow.start`, `workflow.status`, `workflow.cancel`, and `workflow.resume`, plus `workflow.step.*` events, so any client (Talaria, Desktop) can render the tree and steer steps.
4. **Schema on admissions.** An `output_schema` on `prompt.submit` (or on the runner's unit), with the same one-correction-turn contract `delegate_task` already implements, and the validated JSON stored with the receipt (`prompt.receipt` already has a `result` slot, `canonical.py:47-53`).
5. **Model-facing per-task routing in `delegate_task`** (optional, but it closes the gap for model-driven fan-outs). Add per-task `model`, `provider`, and `reasoning_effort`, restricted to an operator allowlist the way `ctx.llm` gates overrides (`plugin-llm-access.md:393-470`). Today `delegate_task` uses one route per profile.
6. **Provider-aware admission control.** A per-provider concurrency semaphore in the owner, fed by the same limit and usage data `model.options` shows, so backend-side runs back off on 429s without each client reimplementing it.
7. **Docs fixes found in passing:**
   - `delegation.md:536-550` still documents `role="orchestrator"` as the opt-in, but the code ignores `role` and derives it from depth (`tools/delegate_tool.py:231-234`).
   - `kanban.md:11` says "every worker is a full OS process", but at this revision the worker subprocess only submits the claim, and the agent runs inside the profile owner (`hermes_cli/kanban_worker_client.py:1`, `gateway/session_kanban.py:1`).

### 4.11 Option B detail: compile to Kanban

- **Mapping:**
  - `agent()` becomes a card with `model_override`, `provider_override`, `reasoning_effort`, `skills`, a workspace (`worktree:` or `dir:`), and an `idempotency_key` of `wf:<run>:<key>`.
  - `pipeline()` becomes `parents` links.
  - `parallel()` becomes sibling cards.
  - verify-then-synthesize is exactly `hermes kanban swarm`.
- **Results:** have each card write JSON to a declared artifact (`kanban_complete(artifacts=[...])`), because scratch workspaces are deleted on completion (`kanban.md:135`). Validate the JSON in the next card or in the client.
- **Limits:**
  - Dynamic control flow needs an orchestrator profile calling `kanban_create`, which means a model, not the script, is deciding.
  - There is a 60 s dispatch tick unless the client nudges `POST /dispatch`.
  - The board is shared across all profiles on the host.
  - The kanban REST API is unauthenticated on localhost (`kanban.md:913-919`).
- **Requirement:** the gateway must be running (`dispatch_in_gateway`). The installed config has `kanban.dispatch_in_gateway: true`.

---

## 5. Prior art to borrow from

- **Hermes `dynamic-workflow` skill:** two-layer split (a deterministic `execute_code` layer plus a judgment `delegate_task` layer), a manifest per run, outputs to files rather than summaries, and "adversarial convergence" (independent attempts, then refuters, capped at 3 rounds) (`SKILL.md:47-120`). These are good defaults for Talaria's `judge()` and refute patterns.
- **Hermes Kanban Swarm:** blackboard, then workers, then verifier, then synthesizer, committed atomically (`hermes_cli/kanban_swarm.py`).
- **Claude Code workflows** (from the brief and the operator's `~/.claude/CLAUDE.md`, "Model & effort tiering"): `agent()` takes `{model, effort}` per call, fan-out goes through a bounded pool rather than a bare `parallel([...])`, and there is a resumable journal and a live progress tree. I did not inspect Claude Code's implementation in this unit.

---

## 6. Things I could not verify

- The live `model.options` output on this machine: how many providers and models it reports, which are authenticated, and which have prices. Not called (read-only rule; it needs the dashboard token).
- Whether the gateway's native `/api/ws` accepts Talaria's existing credential, and how bootstrap tickets are obtained. Not investigated.
- Throughput and memory at about 10 or more concurrent sessions in one gateway process. Not measured.
- The exact shape of a provider rate-limit failure on a canonical `prompt.receipt`. Not observed.
- The content of upstream commit `bf2b3713ab` beyond its title.
- That the sidecar has no session-count cap. This is a grep-based negative finding only.
