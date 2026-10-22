# Hermes Desktop catalog for a terminal client (unit u06-hermes-desktop-catalog)

**Orientation.** This report lists what the Hermes Desktop app offers, where each part lives, which backend calls each part makes, and whether a terminal client needs it. The app is at `<hermes>/apps/desktop`, with a shared package at `<hermes>/apps/shared`. It is Electron (a shell that runs a web app as a desktop program) with a React screen layer. The screens reach the Hermes agent backend in two ways: a WebSocket carrying JSON-RPC (named requests such as `session.create`), and plain HTTP REST routes under `/api/`. Everything below comes from reading source at revision 22063fc099. Nothing was run.

**Conventions.** `desktop/src/...` means `apps/desktop/src/...`. `electron/...` means `apps/desktop/electron/...`. `shared/...` means `apps/shared/src/...`. "RPC" means a named JSON-RPC method. "REST" means an HTTP route under `/api/`. "UI" means user interface.

Verdicts: **core** means a terminal client for everyday coding, or for running agents in the background, needs it. **nice** means useful once the core works. **skip** means it needs a graphical window, duplicates what a terminal already does, or is product marketing, billing or telemetry.

## 1. Short answer

- The gateway contract lists 257 RPC methods, 79 notifications that the backend pushes, and 16 requests the backend makes back to the app (approvals, clarifying questions and similar). Source: `shared/gateway-contract.openrpc.json`.
- The renderer names 181 of the 257 methods in its non-test source. The other 76 are listed in section 7.
- The REST layer has 149 wrapper functions covering 148 distinct route templates.
- I catalogued 89 features: 37 core, 30 nice, 22 skip. Section 6 has the table.
- I catalogued 28 configuration areas and how each is read and written (section 8).
- Source size: `desktop/src` holds 1,748 non-test TypeScript files (415,127 lines). The Bot Mode plugin alone is 88 files (32,429 lines). The screen folder `desktop/src/app` is 638 files (157,960 lines).

## 2. How I checked

1. Revision: `git -C <hermes> rev-parse --short HEAD` returned `22063fc099`. The full commit is `22063fc099ec9ad8357d7f8f6e6248a0400680d3`, dated 2026-10-18T08:02:52-07:00, subject "chore(catalog): review disclosure for hermes-seo". `git status --short` printed nothing.
2. Contract: parsed `shared/gateway-contract.openrpc.json` (OpenRPC 1.3.2): 257 methods, 861 schemas, 79 notifications, 16 server requests.
3. Renderer use: counted quoted method names in every non-test file under `desktop/src` (not the Electron folder).
4. REST: parsed every exported function in `desktop/src/api/*.ts` for its path and verb.
5. Electron bridge: parsed the `window.hermesDesktop` object in `electron/preload.ts` from line 28 (156 top-level members). The renderer uses 130 distinct members.
6. Routes, panes, settings and plugins: read `desktop/src/app/routes.ts`, `desktop/src/app/contrib/surfaces.tsx`, `desktop/src/app/contrib/wiring.tsx`, `desktop/src/app/settings/index.tsx`, `desktop/src/app/settings/constants.ts`, and the plugin registrations.
7. Feature areas: read headers and the key call sites of each area.

Limits: no app launch, no backend call, no network fetch. I did not compare against upstream v0.22.4 (released 2026-10-21). Some early grep output was cut by a tool wrapper, so every count above comes from a full script scan. Anything I could not verify is marked "unverified."

## 3. Architecture facts a terminal client must match or replace

- **Three owners.** `desktop/AGENTS.md` ("What this app is") says Electron owns the machine (processes, files, git, windows, install and update), the renderer owns the screens, and the backend owns sessions, tools, model calls and streaming. It also says Desktop "is not the browser dashboard and it does not embed the TUI" (terminal user interface). A terminal client has no Electron part, so it must do the machine work itself or drop it.
- **Live connection.** For a native connection the client offers two WebSocket subprotocols: `hermes-gateway-v1` and `hermes-gateway-ticket.<ticket>` (`desktop/src/api/client.ts:524-530`). The ticket is one-use and is minted through Electron (`desktop/src/AGENTS.md`, "Native tickets"). How a terminal client should authenticate is **unverified**.
- **Local gateway start.** Desktop does not start the gateway itself. It runs `hermes gateway ensure --json` and reads the endpoint from its output (`electron/local-gateway.ts:6, 122-138`; `desktop/src/AGENTS.md`, "Desktop local execution"). One gateway process serves several profiles (same file). A terminal client could run the same command; whether that has side effects a terminal would not want is **unverified**.
- **Protocol name.** The local adapter is `hermes-gateway-v1` (`desktop/src/api/canonical-protocol.ts:1-2`). Remote connections keep an older protocol (same comment).
- **Sparse config writes.** `PUT /api/config` deep-merges, so a client should send only the keys it changed (`desktop/src/app/settings/appearance-settings.tsx:112-116` comment; `chat-font-setting.tsx:83-86` comment).
- **Slash commands come from the backend.** `commands.catalog` and `complete.slash` return built-in, quick and skill commands, so new backend commands appear without client changes (`desktop/src/AGENTS.md`, "Slash commands").
- **Approvals are a server queue.** `approval.pending`, `approval.received` and `approval.respond` drive them (`desktop/src/store/prompts.ts:78-83`).
- **Electron-only work.** Local git (`electron/git-ipc.ts:42-56`), file read and write, the pseudo-terminal (`electron/terminal-ipc.ts:11`, which uses the `node-pty` library), notifications, the desktop plugin installer, connection secrets (`preload.ts:279-280`), app updates and window control. None of these is a backend method.

## 4. Screens, panes and navigation

**Routing.** The app uses a hash router (`desktop/src/main.tsx:114`). The route list has 12 entries (`desktop/src/app/routes.ts:64-77`). A "workspace page" fills the main pane. An "overlay" is a full-screen card over the app (`routes.ts:163-172` lists the eight overlay views).

| Route | Kind | Screen | Source |
|---|---|---|---|
| `/` and `/<session>` | chat | Chat | `desktop/src/app/contrib/surfaces.tsx:188-218` |
| `/capabilities` | workspace page | Skills, toolsets, connectors, plugins | `desktop/src/app/capabilities/index.tsx:31` |
| `/messaging` | workspace page | Messaging platforms and pairing | `desktop/src/app/messaging/index.tsx` |
| `/artifacts` | workspace page | Images, files and links found in chats | `desktop/src/app/artifacts/index.tsx` |
| `/settings` | overlay | Settings | `desktop/src/app/settings/index.tsx:72-89` |
| `/command-center` | overlay | Sessions, system, usage, maintenance | `desktop/src/app/command-center/index.tsx:48-50` |
| `/cron` | overlay | Scheduled jobs | `desktop/src/app/cron/index.tsx` |
| `/profiles` | overlay | Profiles | `desktop/src/app/profiles/index.tsx` |
| `/agents` | overlay | Subagent tree | `desktop/src/app/agents/index.tsx` |
| `/starmap` | overlay | Learning map | `desktop/src/app/starmap/index.tsx` |
| `/webhooks` | overlay | Webhooks | `desktop/src/app/webhooks/index.tsx` |
| `/session-import` | overlay | Import from Claude Code and Codex | `desktop/src/app/session-import/index.tsx` |
| `/kanban` | plugin route | Kanban board | `desktop/src/plugins/kanban/plugin.tsx:108-111` |

Overlays are mounted at `desktop/src/app/contrib/wiring.tsx:1343-1411`.

**Sidebar top navigation** (`desktop/src/app/chat/sidebar/index.tsx:209-250`): new session, Capabilities, Messaging, Artifacts and Cron. Artifacts and Cron show only in Advanced mode (tiers at lines 239 and 247). The Kanban plugin adds a row (`desktop/src/plugins/kanban/plugin.tsx:125-128`).

**Panes.** The layout registry holds pane ids: sessions, bots (Bot Mode), workspace, chat tiles, terminal, review, files and HUD. The default arrangement groups review, files and terminal (`desktop/src/app/contrib/layout-presets.ts:27, 46, 61, 66`). The review pane id is set at `desktop/src/app/contrib/controller.tsx:273`. The terminal is mounted at `desktop/src/app/contrib/wiring.tsx:1180`. The Bot Mode pane is registered at `desktop/src/plugins/hermes-bots/plugin.tsx:443-447`.

**Command palette.** About 40 entries (`desktop/src/app/command-palette/index.tsx`: navigation at 863-921, command center at 956-1007, capabilities at 1160-1181).

**Interface tiers.** Advanced (the default) and Simple. Simple hides the file browser, review pane and terminal, collapses reasoning into one "Thought for N s" line, shows product summaries of tool calls, and shows the status bar only when a free-tier sign-in or onboarding needs it (`desktop/src/store/interface-mode.ts:106-125`). The choice is a local preference under the key `hermes.desktop.interfaceMode.v1` (`interface-mode.ts:44`). Mode changes what is shown, not what Hermes can do (`interface-mode.ts:9-12`).

## 5. Settings structure

- **Views** (`desktop/src/app/settings/index.tsx:72-89`): nine configuration sections, then providers, gateway, connections (an old name that now redirects to gateway), keybinds, keys, vault, notifications, billing, sessions, plugins and about.
- **Configuration sections** (`desktop/src/app/settings/constants.ts:710-867`): Model, Chat, Appearance, Workspace, Safety, Browser, Memory and Context, Voice, Advanced.
- **Subpages**: Model (main, fallbacks, auxiliary, MoA for Mixture of Agents). Chat (behavior, attachments). Workspace (projects, shell, files). Safety (approvals, privacy, checkpoints). Browser (profile, network). Memory (persistent, context). Voice (conversation, transcription, speech). Advanced (desktop, runtime, tools, terminal, delegation, output). Gateway (connection, devices, managed updates). Keybinds (shortcuts, HUD gesture, screen capture). Notifications (alerts, sounds). Sessions (archived, default folder). Vault (credentials, sources). About (updates, uninstall). Source: `config-subpages.ts:14-82`, `other-subpages.ts`, `appearance-subpages.ts`.
- **Mounts** (`settings/index.tsx`): config 543, providers 551, keys 560, billing 562, plugins 564, vault 572, sessions 574.

## 6. Feature catalog

Each row gives the feature, its location, the RPC methods or REST routes it calls, what it does, and its verdict. Call names are from the source cited. Scratch extracts are listed in section 11.

### 6.1 Chat and sessions

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F01 | Session sidebar | `desktop/src/app/chat/sidebar/` (index.tsx, sessions-section.tsx, session-row.tsx) | REST: GET `/api/profiles/sessions/sidebar`, GET `/api/profiles/sessions`, GET `/api/sessions/search`, PATCH `/api/sessions/{id}` (archive, pin, unread, rename), DELETE `/api/sessions/{id}`. RPC: `session.title` (session-actions-menu.tsx:87), `session.archive` (session-row.tsx:473), `session.branch` family | Lists chats across profiles and gateways, searches, archives, renames, deletes, branches | core: the session list is the main navigation |
| F02 | Chat tiles | `desktop/src/app/chat/session-tile.tsx`, `tile-zone-host.ts`, `session-tile-actions.ts` | RPC `session.resume` (session-tile.tsx:167), `session.interrupt`, `session.steer`, `session.redirect` (session-tile-actions.ts:383, 410, 467) | Several chats on screen at once, each with its own run | core: parallel runs are the goal |
| F03 | Transcript and streaming | `desktop/src/app/chat/index.tsx`, `transcript-window.ts`, `transcript-backfill.ts:609` | REST GET `/api/sessions/{id}` and `/messages`. Events `message.start`, `message.delta`, `message.complete`, `tool.start`, `tool.complete`, `reasoning.delta`, `thinking.delta`, `session.info` | Shows streamed answers, tool calls and reasoning | core |
| F04 | Create or resume a chat | `desktop/src/app/session/hooks/use-session-actions/` (session-create-request.ts) | RPC `session.create` (sent with source desktop, per `desktop/src/AGENTS.md`), `session.resume`, `session.activate` | Starts a chat in a working folder under a profile, or reopens one | core |
| F05 | Import Claude Code and Codex sessions | `desktop/src/app/session-import/index.tsx`, `api.ts:4-44` | RPC `session.foreign.list`, `session.foreign.preview`, `session.foreign.import` (called through a template string at api.ts:44) | Lists past Claude Code and Codex sessions and imports one | nice: a one-time migration |

### 6.2 Composer

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F06 | Send a prompt | `desktop/src/app/session/hooks/use-prompt-actions/index.ts` | RPC `prompt.submit` | Sends the message | core |
| F07 | Background prompt | `use-prompt-actions/slash.ts:749-778` | RPC `prompt.background` (returns a task id) | Runs a prompt as a background task while the user keeps working | core: central to running several agents |
| F08 | Side question (/btw) | `slash.ts:702-726` | RPC `prompt.btw`; event `btw.complete` | Asks a side question without changing the main turn | nice |
| F09 | Slash commands | `desktop/src/app/chat/composer/hooks/use-slash-completions.ts:59, 96`; `slash.ts`; `desktop/src/lib/desktop-slash-registry.json` | RPC `commands.catalog`, `complete.slash`, `slash.exec`, `command.dispatch` (fallback) | Lists and runs commands | core. The registry holds 129 commands. 91 have no disposition (shown on desktop). 27 are marked terminal-only (for example /clear, /exit, /quit, /config, /cron, /plugins, /toolsets, /update, /restart, /reload, /copy, /history, /paste, /image, /verbose). 8 are advanced (/curator, /fast, /insights, /kanban, /reload-mcp, /reload-skills and two underscore aliases). /model is hidden, /login is settings-owned, /voice is composer-voice. "Terminal-only" is one of the disposition values named in `desktop/src/AGENTS.md` lines 39-41. Reading "no value means shown" from null is inferred, not checked against the Python registry |
| F10 | @ file completion | `use-at-completions.ts:123`; `path-refs.ts:4` | RPC `complete.path` | Completes file and folder paths | core |
| F11 | Attachments | `use-prompt-actions/index.ts:170-195`; `desktop/src/app/chat/hooks/use-composer-actions.ts:843` | RPC `image.attach_bytes`, `image.attach`, `file.attach`, `image.detach` | Attaches images and files to the next prompt | core |
| F12 | Model pill and switch | `desktop/src/app/chat/composer/model-pill.tsx:41`; `desktop/src/app/session/hooks/use-model-controls.ts:360-364` | RPC `model.options`; `config.set` with key `model` and value `<model> --provider <provider>`, plus ` --session` when it applies to this chat only | Shows and changes the model | core |
| F13 | Reasoning effort | `desktop/src/app/shell/reasoning-menu-panel.tsx`; `desktop/src/lib/reasoning-step.ts:66` | RPC `config.set` (reasoning) | Sets how hard the model thinks | core |
| F14 | Approval mode | `desktop/src/app/shell/approval-mode-menu.tsx`; `desktop/src/store/approval-mode.ts:3, 69, 95` | RPC `config.get` and `config.set` with key `approvals.mode`. Values: manual, smart, off | Sets when commands need approval | core |
| F15 | Busy-input behavior | `desktop/src/app/session/hooks/use-busy-input-mode.ts:59`; `use-hermes-config.ts:155` | RPC `config.get` (`display.busy_input_mode`) | Sets what typing during a run does. Exact values **unverified** | nice |
| F16 | Context usage and compression | `desktop/src/app/shell/hooks/use-context-breakdown.ts:18, 31`; `context-usage-panel.tsx` | RPC `session.context_breakdown`, `session.compress` | Shows what fills the context window and shrinks it | core |
| F17 | Working folder change | `desktop/src/app/shell/hooks/use-change-workspace.ts:22` | RPC `session.cwd.set` | Moves a chat to another folder | core |
| F18 | Stop, steer, redirect | `session-tile-actions.ts:383-467`; `slash.ts:665-690` | RPC `session.interrupt`; `session.steer` (adds a note the model reads at its next tool step); `session.redirect`; `process.stop` | Stops a run, or adds a note while it runs | core |
| F19 | Rollback and undo | Slash registry marks `/rollback` and `/undo` as shown (`desktop-slash-registry.json`); route is the generic slash path (`desktop/src/AGENTS.md`) | Slash `slash.exec`. Rollback RPCs `rollback.list`, `rollback.diff`, `rollback.restore` have **no** direct desktop call (literal search); `session.undo` likewise | Restores files or conversation to an earlier point. Backend behavior **unverified** | core: parity with Claude Code's rewind |
| F20 | Status stack above the composer | `desktop/src/app/chat/composer/status-stack/` (index.tsx, use-subagent-snapshot.ts, subagent-transcript.tsx) | RPC `subagent.list` (use-subagent-snapshot.ts:42), `subagent.tail` (subagent-transcript.tsx:37), `process.list` (`desktop/src/store/composer-status.ts:422`), `process.kill` (composer-status.ts:482) | Shows subagents and running background processes; refreshes every 5 seconds as a fallback | core |
| F21 | Handoff to a messaging platform | `use-prompt-actions/index.ts:567-635`; `slash.ts:1122-1130` | RPC `handoff.request` (platform, session id), `handoff.state` (polled), `handoff.fail` | Moves the chat to a platform such as Telegram | nice |

### 6.3 Approvals and prompts

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F22 | Command approval | `desktop/src/store/prompts.ts:78-83, 256, 283, 357, 388, 402`; server request `approval` | RPC `approval.pending`, `approval.received`, `approval.respond` (330-second timeout, prompts.ts:357) | Asks the user to approve a dangerous command | core |
| F23 | Clarifying questions | Server request `clarify` (the contract says 1 to 5 questions); `desktop/src/api/client.ts:401-406` | RPC `clarify.lock`, `clarify.respond` | Asks the user questions the agent needs answered | core |
| F24 | Masked prompts (sudo, secret, vault) | Server requests `sudo`, `secret`, `display.install.sudo`, `vault.unlock_prompt`, `vault.code`, `vault.save_login` (`server-requests.ts`) | Server requests | Asks for a password or secret without showing it | nice: needed when tools ask for passwords |
| F25 | Agent-driven UI requests | `desktop/src/app/session/hooks/use-message-stream/gateway-event/server-requests.ts:111-115, 773-780` | Server requests `preview.read`, `preview.act`, `window.read`, `theme.apply`, `theme.pick`, `tour`, `setup_beat`, `terminal.read` | The agent reads or drives the app's windows | skip: drives graphical panes. `terminal.read` (read the user's terminal) would be nice |

### 6.4 Models and providers

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F26 | Model picker | `desktop/src/app/shell/model-menu-panel.tsx`, `model-catalog-menu.tsx`, `use-model-menu-controller.ts`; `desktop/src/api/models.ts:21-124` | RPC `model.options`; REST GET `/api/model/options` (models.ts:59), GET `/api/model/info` (models.ts:21), GET `/api/model/recommended-default` (models.ts:75), POST `/api/model/set` (models.ts:124) | Lists providers and models, and sets one | core |
| F27 | Provider keys | `desktop/src/app/settings/keys-settings.tsx`; `env-credentials.tsx:83, 116, 165, 185` | REST GET `/api/env`, PUT `/api/env`, DELETE `/api/env`, POST `/api/env/reveal` | Stores, clears and reveals API keys | core |
| F28 | Custom model endpoints | `desktop/src/app/settings/custom-endpoints-settings.tsx:134-305` | REST GET and POST `/api/providers/custom-endpoints`; POST `.../validate`; POST `.../{id}/activate`; DELETE `.../{id}` | Adds, checks, activates and removes custom endpoints | core |
| F29 | Provider sign-in (OAuth) | `desktop/src/app/settings/providers-settings.tsx:435-510`; `desktop/src/store/onboarding.ts`; `desktop/src/store/free-tier-sign-in.ts:4, 331, 385` | REST GET `/api/providers/oauth`; POST `.../{id}/start`; POST `.../{id}/submit`; GET `.../{id}/poll/{session}`; DELETE `.../sessions/{session}`; DELETE `/api/providers/oauth/{id}` | Signs in to a provider account and disconnects it | core: needed by sign-in-only providers |
| F30 | Model roles | `desktop/src/app/settings/model-settings.tsx:448-951` | REST GET `/api/model/auxiliary`; GET and PUT `/api/model/moa`; PUT `/api/config`; POST `/api/model/set` | Chooses the main model, helper-job models, MoA and fallbacks | core: the main model is needed; the rest is tuning |

### 6.5 Subagents and background work

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F31 | Subagent tree (Agents overlay) | `desktop/src/app/agents/index.tsx:84-105`; `desktop/src/store/subagents.ts:54, 194-213` | Events `subagent.start`, `subagent.progress`, `subagent.thinking`, `subagent.tool`, `subagent.complete`, `subagent.spawn_requested`; RPC `subagent.list`, `subagent.tail`. Desktop never calls `subagent.interrupt` or `subagent.steer` | Shows the tree of spawned subagents with status and progress | core: read side only. Interrupt and steer are in the contract but unused by Desktop; their behavior is **unverified** |
| F32 | Background processes | `desktop/src/store/composer-status.ts:52-61, 422, 482, 519`; `slash.ts:690` | RPC `process.list`, `process.kill`, `process.stop` | Lists and stops background commands the agent started | core |
| F33 | Agent terminal output tab | `desktop/src/app/right-sidebar/terminal/agent-terminal-stream.ts`; `desktop/src/app/session/hooks/use-message-stream/gateway-event/desktop-bridge.ts:11` | Event `agent.terminal.output` | Read-only live output of commands the agent runs | nice |
| F34 | Bot Mode roster | `desktop/src/plugins/hermes-bots/` (88 files; `plugin.tsx:111, 438-447, 500-505`); `desktop/src/AGENTS.md` ("Bot Mode") | RPC `profiles.list`, `profiles.create`, `profiles.configure`, `profiles.describe`, `profiles.get_asset`, `profiles.set_asset`, `session.list` (finds a bot's chat by title), `prompt.submit`, `groups.*` (group chats), `bot_relay.*` (relays), `cron.manage`, `skills.manage`, `mcp.*`, `image.generate` (avatars) | Each bot is a profile with one persistent chat, plus group chats and relays | nice. The identity rule is worth copying: a bot is a profile plus one hidden chat titled "Bot Chat" (documented in `desktop/src/AGENTS.md`). The rest is heavy |
| F35 | Bot Screen | `desktop/src/plugins/hermes-bots/screen-pane.tsx` (header lines 1-13), `screen-connection.ts`, `screen-state.ts` | RPC `display.status`, `display.observe`, `display.start`, `display.switchSandboxImage`, `display.lease.acquire`, `display.lease.release`, `display.install`, `display.thumbnail`. A remote-desktop viewer (noVNC, a browser client for VNC) over a WebSocket speaking RFB (remote framebuffer protocol) | Live view of a bot's headless desktop, with take-over and hand-back | skip: graphical |

### 6.6 Workspace, git and files

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F36 | Review pane | `desktop/src/app/right-sidebar/review/` (index.tsx, ship-bar.tsx:41-171, file-tree.tsx, churn-bar.tsx, scope-row.tsx); `desktop/src/lib/desktop-git.ts`; `electron/git-ipc.ts:42-56` | Electron IPC `hermes:git:*` locally; a REST facade for remote gateways | Lists changed files for a chosen scope, stages, unstages, reverts, commits, and opens or creates a pull request when the GitHub CLI (`gh`) is ready | core: implement with git and gh |
| F37 | Inline code diffs in chat | Appearance setting `hideCodeDiffs` (`desktop/src/app/settings/settings-manifest.ts`) | None | Shows file edits as diffs in the transcript | core |
| F38 | Files pane | `desktop/src/app/right-sidebar/index.tsx:127-290`; `files/tree.tsx`; `files/use-project-tree.ts`; Electron file IPC (`preload.ts`: `readDir`, `renamePath`, `writeTextFile`, `trashPath`, `revealPath`, `openDir`) | Local file IPC | Browses and edits the working folder | nice |
| F39 | Projects | `desktop/src/store/projects.ts` (1,722 lines); `desktop/src/app/chat/sidebar/projects/`; `sidebar/index.tsx:983` | RPC `projects.list`, `projects.tree`, `projects.create`, `projects.update`, `projects.delete`, `projects.add_folder`, `projects.record_repos`, `projects.discover_repos`, `projects.set_active`, `projects.project_sessions`; `session.workspace.move` | Groups chats under project folders and finds git repositories | nice. The per-chat working folder (F17) is core |
| F40 | Your terminal pane | `desktop/src/app/right-sidebar/terminal/` (about 20 files); `wiring.tsx:1180`; `electron/terminal-ipc.ts:11`; `preload.ts:507-520` (`terminal.start`, `attach`, `write`, `resize`, `cwd`, `dispose`) | Electron pseudo-terminal, shown in xterm.js (a browser terminal library) | Runs a real shell in the working folder | skip: the terminal client is already a terminal |
| F41 | Browser pane and window | `desktop/src/app/chat/browser-popout-shell.tsx:1-16`; `desktop/src/app/chat/right-rail/preview*.ts(x)`; `preload.ts:56` (`openBrowserWindow`); RPC `browser.manage`; event `browser.progress` | Electron web view with an address bar | Shows web pages inside the app | skip: graphical |
| F42 | Preview pane | `desktop/src/app/chat/right-rail/` (23 preview files); `desktop/src/app/session/hooks/use-preview-routing.ts:73, 178-184` | RPC `preview.restart`; events `preview.open`, `preview.close`, `preview.restart.progress`, `preview.restart.complete`; server requests `preview.read`, `preview.act` | Shows a local page or file with console and annotation tools | skip: graphical |
| F43 | Artifacts page | `desktop/src/app/artifacts/index.tsx:139-142`; `artifact-utils.ts` | REST GET `/api/profiles/sessions`; GET `/api/sessions/{id}/messages` | Collects images, files and links from chats across profiles, with filters | nice |

### 6.7 Capabilities

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F44 | Skills | `desktop/src/app/capabilities/skills/` (skills-tab.tsx:92, 158, 224, 241; skill-detail.tsx:39; official-skill-detail.tsx:33); `desktop/src/api/skills.ts:15-169` | REST GET `/api/skills`; GET `/api/skills/content?name=`; PUT `/api/skills/toggle`; GET `/api/skills/hub/official`, `/sources`, `/search`, `/preview`, `/scan`; POST `/api/skills/hub/install`, `/uninstall`, `/update`; GET, PUT, DELETE `/api/learning/node` | Lists, reads, enables and disables skills, and installs them from the hub | core |
| F45 | Toolsets | `desktop/src/app/capabilities/toolsets/` (toolsets-tab.tsx:92); `desktop/src/app/settings/toolset-config-panel.tsx:148-699`; `desktop/src/api/toolsets.ts:18-100` | REST GET `/api/tools/toolsets`; PUT `/api/tools/toolsets/{name}`; GET `.../config`, `.../models`; PUT `.../model`, `.../provider`; POST `.../post-setup` | Turns toolsets on or off, and sets each one's model and provider | core |
| F46 | Terminal backend | `desktop/src/app/settings/terminal-backend-panel.tsx:65, 99`; `desktop/src/api/toolsets.ts:113-125` | REST GET `/api/tools/terminal/backends`; PUT `/api/tools/terminal/backend` | Chooses where agent shell commands run | nice |
| F47 | Computer-use permissions | `desktop/src/app/settings/computer-use-panel.tsx:73, 93, 115`; `desktop/src/api/toolsets.ts:132-145` | REST GET `/api/tools/computer-use/status`; POST `/api/tools/computer-use/permissions/grant`; GET `/api/actions/{name}/status` | Grants macOS permissions so the agent can control the screen | skip: graphical permission flow |
| F48 | MCP servers | `desktop/src/app/capabilities/mcp/` (use-mcp-servers.ts:106, 230; use-mcp-probes.ts:52; mcp-editor.tsx; mcp-logs.tsx); `connectors/data/rpc.ts:176-180`; `desktop/src/moved-tabs.ts` (MCP lives in Connectors) | REST GET, POST, PUT and DELETE `/api/mcp/servers`; PUT `/api/mcp/servers/{name}/enabled`; POST `/api/mcp/servers/{name}/test`; GET `/api/mcp/catalog`; POST `/api/mcp/catalog/install`. RPC `mcp.servers.list`, `mcp.servers.status`, `mcp.servers.set_api_key`, `reload.mcp` | Adds, edits, tests, enables and installs MCP (Model Context Protocol) tool servers | core |
| F49 | Connectors | `desktop/src/app/capabilities/connectors/` (45 files; connectors-tab.tsx; derive.ts:233, 244) | RPC `connectors.catalog`, `connectors.list`, `connectors.accounts`, `connectors.accounts.rename`, `connectors.accounts.remove`, `connectors.connect`, `connectors.operation.status`, `connectors.operation.wake`, `connectors.policy.get`, `connectors.policy.set`, `connectors.tools`; `connection.respond` | Connects outside accounts as tools. "Hosted" connectors are managed by the provider; "local" ones come from the catalog, a plugin or a custom entry | nice |
| F50 | Plugins | `desktop/src/app/capabilities/plugins/plugins-tab.tsx:93`; `desktop/src/app/settings/plugin-settings.tsx:88, 395`; `desktop/src/lib/plugin-catalog.ts:95-203` | RPC `plugins.manage`, `plugins.catalog.lookup`; Electron `installDesktopPlugin`, `removeDesktopPlugin`, `reconcileDesktopPlugins`, `probePluginRepo` | Browses the catalog, installs from a repository, enables and disables plugins | nice |
| F51 | Memory | `desktop/src/app/settings/memory/` (provider-config-panel.tsx:33, 59; connect.tsx:34, 75); `desktop/src/app/command-center/maintenance.tsx:72-183`; `desktop/src/api/system.ts:61-141` | REST GET `/api/memory`; POST `/api/memory/reset`; GET and PUT `/api/memory/providers/{p}/config?surface=declared`; POST `.../oauth/start`; GET `.../oauth/status`; GET `/api/curator`; PUT `/api/curator/paused`; POST `/api/curator/run` | Shows memory status, edits memory provider settings, resets memory, runs the curator | core |
| F52 | Learning map (Starmap overlay) | `desktop/src/app/starmap/index.tsx` (57 lines); `star-map.tsx`; `desktop/src/store/starmap.ts`; `desktop/src/app/learning/` | REST GET `/api/learning/graph`; GET, PUT, DELETE `/api/learning/node` | Visual map of what Hermes has learned, along a time axis | skip: visual. Node edits could be nice later |

### 6.8 Profiles, gateway and messaging

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F53 | Profiles page | `desktop/src/app/profiles/index.tsx:292-318`; `create-profile-dialog.tsx:80-84`; `delete-profile-dialog.tsx:72`; `rename-profile-dialog.tsx:98` | REST GET and POST `/api/profiles`; PATCH and DELETE `/api/profiles/{name}`; GET and PUT `/api/profiles/{name}/soul`; GET `/api/profiles/{name}/setup-command`; POST `/api/profiles/{name}/export`; POST `/api/profiles/import` | Lists, creates, renames and deletes profiles; edits the soul file; exports and imports archives | core: profiles keep agent identities apart. Create and rename can wait |
| F54 | Profile switcher | `desktop/src/app/chat/sidebar/profile-switcher.tsx:767, 788`; `profile-scope.ts` | REST GET and PUT `/api/profiles/{name}/soul`; sidebar listings | Moves between profiles | core |
| F55 | Gateway connection | `desktop/src/app/settings/gateway-settings.tsx:179-266, 578`; `connections-registry.tsx`; Electron calls at `preload.ts:273-316` | Electron IPC `getConnectionConfig`, `saveConnectionConfig`, `applyConnectionConfig`, `testConnectionConfig`, `probeConnectionConfig`, `oauthLoginConnectionConfig`, `sshConfigHosts`, `sshResolveHost`, `cloud` | Chooses and tests the connection to the backend, including SSH hosts | core: a terminal client needs its own version |
| F56 | Gateway restart | `desktop/src/api/system.ts:149-151`; used by `command-center/index.tsx:314` and the webhooks page | REST POST `/api/gateway/restart` | Restarts the backend gateway | core |
| F57 | Messaging platforms | `desktop/src/app/messaging/index.tsx:215-544`; `desktop/src/api/messaging.ts:19-47` | REST GET `/api/messaging/platforms`; PUT `/api/messaging/platforms/{id}`; POST `.../{id}/test` | Configures and tests chat platforms | nice |
| F58 | Pairing | `messaging/index.tsx:246, 519, 544`; `messaging.ts:115-145` | REST GET `/api/pairing`; POST `/api/pairing/approve`; POST `/api/pairing/revoke` | Approves or revokes users who pair with the bot | nice |
| F59 | Telegram QR setup | `desktop/src/app/messaging/telegram-qr-setup.tsx:106-240`; `messaging.ts:63-102` | REST POST `/api/messaging/telegram/onboarding/start`; GET `.../{pairing_id}`; POST `.../apply`; DELETE `.../{pairing_id}` | Creates a Telegram bot through a QR-code flow | nice |
| F60 | Webhooks | `desktop/src/app/webhooks/index.tsx:143-352`; `messaging.ts:158-196` | REST GET and POST `/api/webhooks`; POST `/api/webhooks/enable`; PUT `/api/webhooks/{name}/enabled`; DELETE `/api/webhooks/{name}` | Lists, creates, enables, disables and deletes webhooks | nice |

### 6.9 Automation

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F61 | Cron jobs | `desktop/src/app/cron/index.tsx:426-1002`; `cron-actions.ts:27, 94`; `desktop/src/app/chat/sidebar/cron-jobs-section.tsx:265-414`; `desktop/src/api/cron.ts:31-158` | REST GET and POST `/api/cron/jobs`; PUT and DELETE `/api/cron/jobs/{id}`; POST `.../pause`, `.../resume`, `.../trigger`; GET `.../runs`; GET `/api/cron/delivery-targets`; GET `/api/cron/blueprints`; POST `/api/cron/blueprints/instantiate` | Schedules agent runs, with pause, resume, trigger and run history | nice: useful once the backend runs unattended |
| F62 | Kanban board (plugin) | `desktop/src/plugins/kanban/plugin.tsx:82-128`; `api.ts:126-509` | REST under `/api/plugins/kanban/`: GET `/boards`, `/board`, `/profiles`, `/projects`, `/orchestration`; POST `/boards`, `/boards/import`, `/tasks`, `/tasks/bulk`, `/estimate`, `/dispatch`; PUT `/orchestration`; PATCH `/boards/{slug}`; DELETE `/boards/{slug}` (archives); socket `/events` | A task board for agents, with bulk edits, estimates, orchestration settings and a dispatch action | nice: the nearest thing to a task queue for several agents. What "dispatch" starts is **unverified** |

### 6.10 Voice

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F63 | Speech input and spoken replies | `desktop/src/app/chat/composer/hooks/use-composer-voice.ts`; `desktop/src/app/chat/session-tile.tsx:241`; `desktop/src/store/voice-live.ts` | REST POST `/api/audio/transcribe`, `/api/audio/speak`, `/api/audio/tts-lease`, `/api/audio/stt-lease`; GET `/api/audio/elevenlabs/voices` | Records speech, turns it into text (STT, speech-to-text), and speaks replies (TTS, text-to-speech) | nice |
| F64 | GPT-Live conversation | `desktop/src/lib/voice-live.ts` (header lines 1-20); `desktop/src/app/chat/composer/hooks/use-voice-live-conversation.ts`; `voice-live.ts:74` (`config.set voice.voice_chat_mode`) | Session created through the gateway (method not traced). WebRTC (web real-time communication) audio to an OpenAI realtime model, per the vendor link in the header | Full-duplex voice that hands real work to Hermes | nice: depends on an outside vendor. Runtime **unverified** |
| F65 | Wake word | `desktop/src/store/wake-word.ts:270-398`; `slash.ts:1078-1114` | RPC `wake.start`, `wake.stop`, `wake.status`, `wake.resume`, `wake.pause`; event `wake.detected` | Listens for a spoken wake phrase to start voice input | nice: needs a microphone. Not traced end to end |

### 6.11 Operations and updates

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F66 | Command Center | `desktop/src/app/command-center/index.tsx:48-50` (sections), `:218, 242, 314, 319`; `maintenance.tsx` | REST GET `/api/status`, GET `/api/analytics/usage`, POST `/api/gateway/restart`, POST `/api/hermes/update`, GET `/api/actions/{name}/status`; plus F51 routes | A dashboard for sessions, system status, usage and maintenance | nice |
| F67 | Diagnostics | `desktop/src/api/system.ts:293-317`; `maintenance.tsx:147` | REST POST `/api/ops/doctor`, `/api/ops/security-audit`, `/api/ops/backup`, `/api/ops/debug-share` | Health check, security audit, backup, and a shareable debug bundle | nice: a terminal client could offer the same checks as commands (CLI equivalence **unverified**) |
| F68 | Hermes runtime update | `desktop/src/api/system.ts:157-170`; `command-center/index.tsx:314` | REST POST `/api/hermes/update`; GET `/api/hermes/update/check` | Checks for and applies a Hermes update | nice |
| F69 | Desktop app update | `desktop/src/app/settings/about-settings.tsx:61`; `gateway-settings.tsx:233, 1515`; `electron/updater/`; `preload.ts:666` | Electron bridge only | Checks for and installs the desktop app | skip: applies only to the Electron app |
| F70 | Onboarding and setup checks | `desktop/src/store/onboarding.ts` | RPC `onboarding.start`, `onboarding.skip`, `onboarding.state`, `onboarding.set_run`, `setup.status`, `setup.runtime_check` | First-run flow and environment checks | nice: keep the environment check as a doctor command |
| F71 | Shared metrics (opt-in telemetry) | `desktop/src/app/settings/shared-metrics-settings.tsx`; `config-settings.tsx:496`; `desktop/src/store/shared-metrics.ts:14-17` | RPC `shared_metrics.status`, `shared_metrics.set`, `shared_metrics.desktop_*` | Sends usage data if the user opted in | skip. Honor the two switches (`enabled`, `send`) |

### 6.12 Settings and personal preferences

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F72 | Appearance | `desktop/src/app/settings/appearance-settings.tsx`; `settings-manifest.ts:21-115` | Local atoms. Writes `PUT /api/config` for `display.resume_last_session` (appearance-settings.tsx:112), `display.language` (`desktop/src/i18n/context.tsx:43-48`), `terminal.font_family` (terminal-font-setting.tsx:86-100), `desktop.font_family` (chat-font-setting.tsx:24, 80-94) | Themes, fonts, density, layout, bubbles, pet, language | skip for the visual parts. Keep the language and resume-last-session keys |
| F73 | Chat behavior settings | `desktop/src/app/settings/config-settings.tsx`; `config-subpages.ts:19-30` | `PUT /api/config` for `display.personality`, `timezone`, `display.show_reasoning`, `agent.image_input_mode` | Personality, time zone, reasoning display, image input | nice |
| F74 | Sessions settings | `desktop/src/app/settings/sessions-settings.tsx:82, 275, 324-340` | PATCH `/api/sessions/{id}` (archived); `PUT /api/config` for `sessions.auto_archive` and `sessions.auto_archive_days`; Electron default-folder calls (`preload.ts:436-440`) | Restores archived chats, sets auto-archive, sets the default folder for new chats | nice |
| F75 | Vault | `desktop/src/app/settings/vault-settings.tsx:42-371` | RPC `vault.sources`, `vault.source.set`, `vault.lock`, `vault.unlock`, `vault.list`, `vault.add`, `vault.remove` | Stores login credentials, and locks and unlocks them | nice |
| F76 | Billing and free tier | `desktop/src/app/settings/billing/api.ts:148-173`; `desktop/src/store/free-tier-sign-in.ts` | RPC `billing.state`, `billing.charge`, `billing.charge_status`, `billing.step_up`, `billing.auto_reload`, `subscription.state`, `subscription.preview`, `subscription.change`, `subscription.resume`, `free_tier.status`, `free_tier.provision`, `free_tier.ack_notice` | Plan, payment and free-tier state for the Nous account | skip: product billing. The Nous sign-in stays as F29 |
| F77 | Notifications and sounds | `desktop/src/app/settings/notifications-settings.tsx` (no backend call found); Electron `notify` | Local | Desktop notifications and sounds | skip |
| F78 | Keybinds, HUD gesture, screen capture | `keybind-settings.tsx`, `hud-modifier-settings.tsx`, `screenshot-settings.tsx` (no backend call found) | Local and Electron | Keyboard shortcuts, the HUD gesture, screenshot keys | skip |
| F79 | Interface tiers | `desktop/src/store/interface-mode.ts:1-35, 44, 106-132` | Local | Hides developer panes and tool payloads by default | nice: a short default view is worth copying |
| F80 | Command palette | `desktop/src/app/command-palette/index.tsx` (about 40 commands) | Calls the screens above | Searches and runs commands and settings | core |
| F81 | Tours and tips | `desktop/src/store/tours.ts:25`; `desktop/src/store/tips.ts:90` (config keys `display.in_app_tours`, `display.in_app_tips`) | `config.set` | Walk-throughs and hints | skip |

### 6.13 Desktop-only and decorative surfaces

| # | Feature | Where | Calls | What it does | Verdict and reason |
|---|---|---|---|---|---|
| F82 | Quick entry window | `desktop/src/app/quick-entry/quick-entry-app.tsx` (237 lines); `desktop/src/app/contrib/hooks/use-quick-entry-bridge.ts`; `preload.ts:229` | Electron global shortcut and IPC | Sends a prompt into a chosen chat from anywhere | skip: a terminal is already the launcher |
| F83 | HUD (floating transcript) | `desktop/src/app/hud/hud-shell.tsx` (369 lines); `preload.ts:123, 180` | Electron window, always on top | Floating transcript and composer | skip: graphical |
| F84 | Pet, pet overlay, pet generation | `desktop/src/app/pet-overlay/`; `desktop/src/app/pet-generate/`; `desktop/src/app/settings/pet-settings.tsx:49` | RPC `pet.info`, `pet.gallery`, `pet.generate`, `pet.hatch`, `pet.select`, event `pet.changed` | A desktop mascot window and its generator | skip: decorative |
| F85 | Wake indicator window | `desktop/src/app/wake-indicator/wake-indicator-app.tsx` (42 lines); `preload.ts:79` | Electron window | Shows whether wake-word listening is on | skip: follows F65 |
| F86 | Radio plugin | `desktop/src/plugins/radio/plugin.js` (698 lines; stations at lines 12-18) | External audio streams | Internet radio in the status bar | skip: bloat with external streams |
| F87 | Hello-runtime plugin | `desktop/src/plugins/hello-runtime/plugin.runtime.js` (121 lines) | None | A sample plugin for developers | skip |
| F88 | Reactions | `desktop/src/store/reactions-enabled.ts:35` (config key `display.message_reactions`); events `message.reaction`, `reaction` | `config.set` | Emoji reactions on messages | skip: decorative |
| F89 | Window chrome | `preload.ts:30, 386, 395, 447, 690` (translucency, tray, zoom, themes) | Electron | Transparency, tray icon, zoom, themes and skins | skip |

**Tally:** 89 features. Core 37 (F01–F04, F06, F07, F09–F14, F16–F20, F22, F23, F26–F32, F36, F37, F44, F45, F48, F51, F53–F56, F80). Nice 30 (F05, F08, F15, F21, F24, F33, F34, F38, F39, F43, F46, F49, F50, F57–F59, F61–F68 except F66's overlap, F70, F73–F75, F79). Skip 22 (F25, F35, F40, F41, F42, F47, F52, F69, F71, F72, F76–F78, F81–F89).

## 7. Gateway methods the desktop does not call, and notifications it does not name

**Methods with no quoted use in renderer source (76 of 257).** `browser.controller.detach`, `browser.controller.heartbeat`, `browser.controller.register`, `browser.controller.result`, `client.capabilities`, `clipboard.paste`, `command.resolve`, `config.show`, `delegation.pause`, `delegation.status`, `display.stop`, `gateway.capabilities`, `groups.demote`, `groups.disband`, `groups.peer.invite`, `groups.peer.register`, `groups.peer.revoke`, `groups.promote`, `groups.rename`, `groups.replica_state`, `groups.replicate`, `input.detect_drop`, `insights.get`, `learning.delete`, `learning.detail`, `learning.edit`, `learning.frames`, `machine.facts`, `mcp.servers.oauth.callback`, `mcp.servers.oauth.cancel`, `mcp.servers.oauth.poll`, `mcp.servers.oauth.start`, `mcp.servers.remove`, `model.disconnect`, `model.save_key`, `onboarding.set_run`, `paste.collapse`, `pet.cells`, `plugins.list`, `project.facts`, `projects.archive`, `projects.for_cwd`, `projects.get`, `projects.remove_folder`, `projects.set_primary`, `request.answer`, `rollback.diff`, `rollback.list`, `rollback.restore`, `session.delete`, `session.events.stats`, `session.foreign.import`, `session.foreign.list`, `session.foreign.preview`, `session.most_recent`, `session.set_hidden`, `session.undo`, `shell.exec`, `skills.reload`, `spawn_tree.list`, `spawn_tree.load`, `spawn_tree.save`, `subagent.interrupt`, `subagent.steer`, `subscription.upgrade`, `system.battery`, `terminal.resize`, `tools.configure`, `tools.list`, `tools.show`, `toolsets.list`, `usage.bars`, `verification.status`, `voice.record`, `voice.toggle`, `voice.tts`.

Notes: `session.foreign.*` is used through a template string (api.ts:44), so it is used. The learning methods are unused because the learning map calls REST instead. The MCP sign-in is handled by Electron. Voice runs over REST and WebRTC. The rest may be used by other Hermes clients, which I did not check.

**Notifications with no literal handler in renderer (10 of 79).** `agent.print`, `browser.controller.cancel`, `browser.controller.command`, `session.resume_progress`, `skills.changed`, `tool.output_risk`, `voice.interrupted`, `voice.partial`, `voice.status`, `voice.transcript`. Whether a generic handler covers them is **unverified**.

**Server requests** (16): all have literal handlers in `server-requests.ts`.

## 8. Configuration: how each area is read and written

**General mechanics (verified in source):**
- Read the whole config: GET `/api/config` (`desktop/src/api/config.ts:156-163`; hook `desktop/src/app/hooks/use-config-record.ts:91`). Defaults: GET `/api/config/defaults` (config.ts:165). Field schema: GET `/api/config/schema` (config.ts:172; used by config-settings.tsx:141).
- Write the config: PUT `/api/config` with body `{config}` (`saveHermesConfig` / `saveHermesConfigRecord`, config.ts:179-200). The server deep-merges, so partial patches are normal.
- Live settings: RPC `config.get` and `config.set` with a key and value (approvals.mode, model, reasoning, voice.voice_chat_mode, display toggles).
- Secrets: GET, PUT and DELETE `/api/env`, and POST `/api/env/reveal` (config.ts:204-241).
- Electron-owned settings: connection profiles, default folder, desktop plugins, secret-storage encryption, window and theme settings.

| # | Area | Where in Desktop | Read | Write | Needed for full terminal configuration? |
|---|---|---|---|---|---|
| C01 | Providers and sign-in | Settings "providers" | GET `/api/providers/oauth`; GET `/api/env` | POST `/api/providers/oauth/{id}/start`, `.../submit`; GET `.../poll/{session}`; DELETE `.../sessions/{session}`; DELETE `/api/providers/oauth/{id}`; POST and DELETE `/api/providers/custom-endpoints` | Yes: sign-in is the only way some providers connect |
| C02 | Provider keys | Settings "keys" | GET `/api/env` | PUT `/api/env` (key, value); DELETE `/api/env`; POST `/api/env/reveal` | Yes |
| C03 | Models | Settings Model section | GET `/api/config`, `/api/config/schema`, `/api/model/info`, `/api/model/auxiliary`, `/api/model/moa`, `/api/model/recommended-default`; RPC `model.options` | PUT `/api/config`; POST `/api/model/set`; PUT `/api/model/moa`; RPC `config.set` key `model` | Yes |
| C04 | Profiles | Route `/profiles` | GET `/api/profiles`; GET `/api/profiles/{name}/soul` | POST `/api/profiles`; PATCH and DELETE `/api/profiles/{name}`; PUT `.../soul`; POST `.../export`; POST `/api/profiles/import` | Yes for listing and switching; the rest can wait |
| C05 | Gateway connection | Settings "gateway" | Electron `getConnectionConfig`, `probeConnectionConfig`, `sshConfigHosts`, `getSecretStorageEncryption` | Electron `saveConnectionConfig`, `applyConnectionConfig`, `testConnectionConfig`, `oauthLoginConnectionConfig`, `setSecretStorageEncryption`; POST `/api/gateway/restart` | Yes, but the terminal client keeps its own settings |
| C06 | Messaging platforms and pairing | Route `/messaging` | GET `/api/messaging/platforms`; GET `/api/pairing` | PUT `/api/messaging/platforms/{id}`; POST `.../{id}/test`; POST `/api/pairing/approve` and `/revoke`; Telegram onboarding routes | Nice |
| C07 | Webhooks | Route `/webhooks` | GET `/api/webhooks` | POST `/api/webhooks`; POST `/api/webhooks/enable`; PUT `.../{name}/enabled`; DELETE `/api/webhooks/{name}` | Nice |
| C08 | Memory | Settings "memory" and provider panels | GET `/api/memory`; GET `/api/memory/providers/{p}/config?surface=declared`; GET `/api/config` | PUT `/api/memory/providers/{p}/config?surface=declared`; PUT `/api/config`; POST `/api/memory/reset`; POST `/api/curator/run`; PUT `/api/curator/paused` | Yes |
| C09 | Toolsets | Capabilities "toolsets" | GET `/api/tools/toolsets`; GET `.../{name}/config`, `.../models` | PUT `/api/tools/toolsets/{name}`; PUT `.../model`, `.../provider`; POST `.../post-setup`; PUT `/api/config` (toolsets key) | Yes |
| C10 | Terminal backend | Advanced "terminal" and toolset detail | GET `/api/tools/terminal/backends` | PUT `/api/tools/terminal/backend` | Nice |
| C11 | MCP servers | Capabilities "connectors" | GET `/api/mcp/servers`; GET `/api/mcp/catalog`; RPC `mcp.servers.list`, `mcp.servers.status` | POST, PUT, DELETE `/api/mcp/servers`; POST `.../{name}/test`; PUT `.../{name}/enabled`; POST `/api/mcp/catalog/install`; RPC `mcp.servers.set_api_key`, `reload.mcp` | Yes |
| C12 | Plugins | Settings "plugins"; Capabilities "plugins" | RPC `plugins.manage`; `plugins.catalog.lookup` | RPC `plugins.manage` (enable, disable); Electron `installDesktopPlugin`, `removeDesktopPlugin` | Nice |
| C13 | Skills | Capabilities "skills" | GET `/api/skills`; GET `/api/skills/content?name=` | PUT `/api/skills/toggle`; POST `/api/skills/hub/install`, `/uninstall`, `/update` | Yes |
| C14 | Cron | Route `/cron` | GET `/api/cron/jobs`; GET `/api/cron/blueprints`; GET `/api/cron/delivery-targets` | POST, PUT, DELETE `/api/cron/jobs...`; POST `.../pause`, `.../resume`, `.../trigger`; POST `/api/cron/blueprints/instantiate` | Nice |
| C15 | Appearance | Settings "appearance" | Local atoms; `GET /api/config` for four keys | Local; `PUT /api/config` for `display.resume_last_session`, `display.language`, `terminal.font_family`, `desktop.font_family` | No for visuals. The four keys are optional |
| C16 | Chat behavior | Settings "chat" | GET `/api/config` | PUT `/api/config` (`display.personality`, `timezone`, `display.show_reasoning`, `agent.image_input_mode`) | Nice |
| C17 | Workspace | Settings "workspace" | GET `/api/config` (`terminal.cwd`, `desktop.repo_scan_*`, `code_execution.mode`, `terminal.persistent_shell`, `terminal.env_passthrough`, `file_read_max_chars`) | PUT `/api/config` | Yes for the working folder; nice for the rest |
| C18 | Safety | Settings "safety" | GET `/api/config` (`approvals.*`, `command_allowlist`, `security.*`, `checkpoints.*`); RPC `config.get` `approvals.mode` | PUT `/api/config`; RPC `config.set` `approvals.mode` | Yes, for the approval mode |
| C19 | Browser | Settings "browser" | GET `/api/config` (`browser.use_real_profile`, `browser.allow_private_urls`, `browser.auto_local_for_private_urls`) | PUT `/api/config` (browser-real-profile-panel.tsx:70) | Nice |
| C20 | Voice | Settings "voice" | GET `/api/config`, `/api/config/schema`; GET `/api/audio/elevenlabs/voices` | PUT `/api/config` (voice-provider-fields.tsx:100); RPC `config.set` `voice.voice_chat_mode` | Nice |
| C21 | Advanced | Settings "advanced" | GET `/api/config`, `/api/config/defaults` (`agent.max_turns`, `agent.api_max_retries`, `agent.service_tier`, `delegation.*`, `tool_output.*`, `updates.*`) | PUT `/api/config` | Nice. The schema makes the page generic |
| C22 | Vault | Settings "vault" | RPC `vault.list`, `vault.sources` | RPC `vault.add`, `vault.remove`, `vault.lock`, `vault.unlock`, `vault.source.set` | Nice |
| C23 | Sessions | Settings "sessions" | REST GET `/api/profiles/sessions` (archived list through a store) | PATCH `/api/sessions/{id}` (archived); PUT `/api/config` (`sessions.auto_archive`, `auto_archive_days`); Electron default folder | Nice |
| C24 | Local models | Settings providers, behind the `localModels` launch flag (`providers-settings.tsx:216, 602`; `preload.ts:33`) | GET `/api/local-models/status`, `/hardware`, `/catalog`, `/jobs` | POST `/api/local-models/runtime/install`, `/quickstart`, `/download`, `/activate`, `/eject`, `/server`, `/sideload`; DELETE `/api/local-models/models/{id}`; POST download pause and resume | Nice |
| C25 | About and updates | Settings "about" | Electron `getVersion`; GET `/api/hermes/update/check` | POST `/api/hermes/update`; Electron `relaunchApp`, `uninstall` | Nice |
| C26 | Notifications and keybinds | Settings "notifications", "keybinds" | Local | Local and Electron | No |
| C27 | Billing | Settings "billing" | RPC `billing.state`, `subscription.state` | RPC `billing.charge`, `billing.auto_reload`, `subscription.change`, `subscription.resume` | No |
| C28 | Shared metrics | Inside config (`config-settings.tsx:496`) | RPC `shared_metrics.status` | RPC `shared_metrics.set` | No. Keep the two switches visible |

## 9. Verdict tally and what to build first

- **Core (37):** the session list, tiles, transcript, sessions, prompts, background prompts, slash commands, @ completion, attachments, model switch, reasoning effort, approval mode, context and compression, working folder, stop, steer and redirect, rollback and undo, the status stack, approvals, clarifying questions, model picker, provider keys, custom endpoints, sign-in, model roles, subagent tree, background processes, review pane, inline diffs, skills, toolsets, MCP servers, memory, profiles, profile switch, gateway connection, gateway restart, and the command palette.
- **Build first:** resume and create chats, send prompts and stream answers, interrupt and steer, background prompts, the process list, the subagent list, approvals and clarifying questions, the model switch, keys and sign-in, git review with commit and pull request, skills, toolsets, MCP servers, profiles, and the config editor (`GET /api/config`, `PUT /api/config`, `GET /api/config/schema`).
- **Build second:** cron, Kanban, messaging and webhooks, connectors, plugins, local models, voice, the agent terminal stream, and handoff.
- **Do not port:** quick entry, HUD, pet, Bot Screen, browser and preview panes, radio, starmap, billing, telemetry, the appearance editor, and the keybind editor.
- **Caution on subagents:** Desktop lists and tails subagents but never interrupts or steers them. The contract defines both. Their behavior is **unverified**, so test before relying on them.

## 10. Open questions and unverified items

1. How a terminal client authenticates to the gateway without Electron-minted tickets (`desktop/src/api/client.ts:524-530`; `desktop/src/AGENTS.md`). **Unverified.**
2. What the Kanban dispatch action starts (`plugins/kanban/api.ts:506`). Not traced.
3. Runtime behavior of every feature. Nothing was run.
4. Whether the 76 uncalled methods are used by other Hermes clients. Not checked.
5. Whether the 10 unnamed notifications are handled generically. Not checked.
6. Wake word and GPT-Live end to end, including the session-creation method. Not traced.
7. Differences in upstream v0.22.4 (2026-10-21). Not fetched or compared.
8. Exact values of `display.busy_input_mode`. Not traced.
9. Whether the `hermes gateway ensure --json` output is stable across versions. Not checked.

## 11. Evidence files

Raw extracts from this run are in the scratch folder `<scratch>/research/`. Key files: `<scratch>/research/openrpc-methods.txt` (257 methods with parameters), `<scratch>/research/rpc-usage.txt` (renderer references per method), `<scratch>/research/api-wrappers.txt` (REST wrappers), and `<scratch>/research/feature-map.txt` (RPC and REST use by area). The folder also has per-area call-site lists, notification and server-request lists, and the Python scripts that produced them.

**Note on the report file.** The task asked for this report to be written to a markdown file. The subagent rules forbid writing report `.md` files and require the findings to be returned in the final message, so no file was written. This message is the complete report.