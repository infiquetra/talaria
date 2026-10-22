# Prior art: third-party clients, terminal UIs, and orchestration tools for Hermes Agent

Data retrieved from GitHub on 2026-10-22 (final build 2026-10-22 05:31 UTC). Hermes Agent is Nous Research's open-source agent (repository NousResearch/hermes-agent). This report lists the third-party clients, terminal user interfaces (TUIs), and orchestration tools I could find, with the evidence for each row.

**Scope note.** This is the prior-art unit for a planned "Claude Code for Hermes" terminal client. Star counts and push dates were read from the GitHub API on the retrieval date and will move. Nothing was installed, run, or changed in Hermes or in the Talaria repository.

## Bottom line

- **Third-party clients are numerous, but terminal-native ones are few.** The keyword and name searches retrieved 2,680 repositories whose name or description mentions Hermes. The curated tables below list 235 projects; the remaining repositories are in the TSV file named under Files.
- **Most-starred third-party terminal app:** joeynyc/hermes-hud (934 stars, Python), a monitor that reads the ~/.hermes folder, not a chat client. **Most-starred interactive third-party terminal client:** liftaris/herm (301 stars, TypeScript, built with the OpenTUI framework).
- **Closest Rust terminal client:** inaki/talaria (0 stars). It calls itself an unofficial native terminal client for Hermes and shares this project's name, so the names need to be kept apart. A second Rust option is 0xNyk/hermes-tui (1 star), which adds a native screen on top of the existing agent process.
- **The largest third-party clients are web or desktop apps.** The most-starred Hermes-specific ones are nesquena/hermes-webui (19,092 stars) and fathah/hermes-desktop (14,448 stars). Both are community projects. The official desktop app is in the Nous repository at apps/desktop.
- **Orchestration is crowded, mostly as web dashboards and team templates.** The terminal-first ones are few: juli-gonzalez/mux, agentik-os/AGK-TUI, yechang1450/open-tui-orchestrator, and shubhsaxena2020/hermes-fleet-ctrl. Each has few or no stars.
- **Nous already ships most of the surfaces a client would need** in the installed checkout: a desktop app, a terminal UI (hermes --tui), a web dashboard, an Agent Client Protocol (ACP) server for editors, a Kanban board, and an agent-to-agent (A2A) plugin. My inference, from this evidence and not a measured result: a new terminal client competes with the first-party TUI, so the difference has to be in behavior and speed, not in existence.
- **Toad** (batrachianai/toad, 3,484 stars, licensed under AGPL-3.0, the GNU Affero General Public License, version 3) is the largest general terminal UI for ACP agents that I found, measured by stars. Its README does not name Hermes, so whether it can run Hermes is unverified.

## How the search was done

- **Repository search with the GitHub CLI.** 30 keyword searches, including 'hermes-agent tui', 'hermes agent client', 'hermes tui', 'nous hermes agent cli', 'hermes acp', 'hermes gateway client', and topic searches for hermes-agent, hermes, nous-research and nous-hermes. I asked for at most 100 results per query. The raw results, before filtering, held 2,888 unique repositories.
- **Name and description searches sorted by stars.** 20 queries of the form 'hermes <term> in:name,description', covering tui, client, ui, dashboard, desktop, acp, orchestrator, multi-agent, kanban, workflow, mobile, workbench, swarm, fleet, team, control, terminal, harness, subagent and gateway. Each returned up to 100 results.
- **Match counts from the GitHub search API.** 20 name-and-description count queries and 20 phrase-and-readme count queries. Totals ran from 59 to 1,532 for name-and-description queries. These totals include unrelated 'Hermes' projects, so I did not enumerate them.
- **Repository details.** Stars, language, and last push came from the saved search results or, for repositories not in those results, from a direct API call. I saved 75 non-empty README files and read them in part (openings, plus keyword counts for the longer ones).
- **Local checkout.** I searched the plugin catalog (690 entries at revision 22063fc09), the official docs under website/docs, optional-skills, plugins, apps, ui-tui, web, acp_adapter and tui_gateway. I did not run any Hermes process.
- **Upstream check.** The upstream main branch has 763 catalog entries; the installed checkout has 690. Of the 74 upstream-only entries, I listed 9 that bear most directly on orchestration, subagents, Kanban or model routing. The rest (themes, memory panes, office tools and similar) are not listed. One local entry (codex-account-switch) is absent upstream.
- **Cross-checks against curated lists.** The Tools, Integrations and Multi-Agent sections of 0xNyk/awesome-hermes-agent, and the Workspaces, Multi-Agent and Tools sections of the Hermes Atlas (ksimback/hermes-ecosystem, file ECOSYSTEM.md). Their star counts are stale; see the conflicts section.
- **Web search.** Five queries. The results were mostly aggregator and blog pages, so I used them only to find leads, not to establish star counts or facts.

**Basis codes used in every table.** R = I read the repository README. D = I used only the GitHub description returned by the API. C = the plugin catalog entry. F = first-party docs or code in the installed checkout. "R (partial)" means I counted or sampled mentions without reading the whole README or code.

**Terms used in the tables.** TUI = terminal user interface. Ink = a React renderer for terminals (the first-party TUI uses it). Textual, Bubble Tea, ratatui and OpenTUI are TUI frameworks for Python, Go, Rust and TypeScript. Electron and Tauri are desktop app frameworks. ACP = Agent Client Protocol, the standard that editors and terminals use to talk to coding agents. MCP = Model Context Protocol. SSH = Secure Shell. PWA = progressive web app. SSE = server-sent events. JSON-RPC = JSON remote procedure call. CLI = command-line interface.

## First-party baseline (what Nous already ships)

These are not third-party projects. They set the baseline any new client has to meet. Paths are in the installed checkout at revision 22063fc09. Its git description is v0.22.2 canary, 60 commits after that tag. The latest upstream release is v0.22.4, published 2026-10-21 (release page checked via the API).

| Component | Location in the checkout | What it is | Evidence |
|---|---|---|---|
| Hermes Desktop (native app) | apps/desktop/ | Electron 40.10.2 app with React 19.2.7 and Vite 8.2.0 (versions from its package.json). Described as the native desktop shell for Hermes Agent. | apps/desktop/README.md; apps/desktop/package.json; website/docs/user-guide/desktop.md:9 |
| Hermes TUI (terminal UI) | ui-tui/ | React and Ink terminal UI started with 'hermes --tui'. Its docs say the client now connects over an authenticated WebSocket with single-use tickets and does not start the gateway module directly. | ui-tui/README.md; ui-tui/CANONICAL_GATEWAY.md:1-14 |
| Web dashboard | web/ | Browser dashboard for configuration, API keys and active sessions. Vite, React 19 and Tailwind CSS v4. Default port 9119 per the Docker docs. | web/README.md; website/docs/user-guide/docker.md:138 |
| ACP adapter (editor protocol) | acp_adapter/ | Agent Client Protocol server so editors can drive Hermes. Documented setups for VS Code, Zed and JetBrains. | acp_adapter/ listing; website/docs/user-guide/features/acp.md:218, 241, 266 |
| Kanban board (multi-agent) | plugins/kanban/ | Dashboard tab described as a multi-agent collaboration board with comment threads and per-profile running state. | plugins/kanban/dashboard/manifest.json; website/docs/user-guide/features/kanban.md |
| Agent-to-agent (A2A) plugin | plugins/platforms/a2a/ | A2A protocol v1.0 client and server. Tools include a2a_call and a2a_orchestrate. | plugins/platforms/a2a/plugin.yaml |
| Bot Mode (named bots and rooms) | website/docs/user-guide/bot-mode.md | Turns Hermes profiles into a roster of named bots with routines and group chats. Built into the desktop app and on by default. | bot-mode.md front matter and intro |
| Buzz integration (documented) | website/docs/integrations/buzz.md | Nous documents three ways to connect Hermes to Buzz, a third-party workspace for people and agents. | buzz.md comparison table |

Related Nous repositories, with live API data:

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [NousResearch/hermes-agent](https://github.com/NousResearch/hermes-agent) | Hermes Agent core. Hosts every first-party surface in the table above. | Python | 256,009 | 2026-10-22 | Reference point, not third-party prior art. | F |
| [NousResearch/hermes-official-plugins](https://github.com/NousResearch/hermes-official-plugins) | Official plugin repository (Nous). The local catalog has 14 official-tier entries. | Python | 363 | 2026-10-22 | Reference point, not third-party prior art. | F |
| [NousResearch/hermes-paperclip-adapter](https://github.com/NousResearch/hermes-paperclip-adapter) | Nous adapter that runs Hermes as a managed employee in Paperclip (an external agent-operations platform). | TypeScript | 1,995 | 2026-04-04 | Reference point, not third-party prior art. | R |
| [NousResearch/Hermes-Bot-Mode](https://github.com/NousResearch/Hermes-Bot-Mode) (archived) | Earlier home of Bot Mode for the desktop app. Archived; the docs now describe Bot Mode inside the desktop app. | JavaScript | 669 | 2026-08-17 | Reference point, not third-party prior art. | D+F |

## Third-party terminal UIs built for Hermes

29 rows. The terminal UIs that act as chat clients are the ones to compare against the first-party TUI. Most of the rest are monitors, widgets, remote wrappers or launchers.

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [inaki/talaria](https://github.com/inaki/talaria) | Unofficial native Rust terminal client (ratatui) for Hermes. The README says it speaks the same stdio JSON-RPC 2.0 protocol as hermes --tui and is not a Nous Research product. | Rust | 0 | 2026-10-03 | Closest prior art to a Rust terminal client. Same name as this project (infiquetra/talaria). Check its protocol against the current WebSocket gateway. | R |
| [0xNyk/hermes-tui](https://github.com/0xNyk/hermes-tui) | Rust second screen for the Ink TUI. Rust draws the screen and the Python agent keeps running; it speaks newline-delimited JSON-RPC. Opt-in, not a replacement for hermes --tui. | Rust | 1 | 2026-09-05 | Shows a split (native screen over the existing agent process) that a new client could reuse. | R |
| [1deat0r/Hades-Agent](https://github.com/1deat0r/Hades-Agent) | Native Rust terminal interface that aims for exact behavioral and visual parity with the Hermes TUI. The README says AI agents built it with a human prompter. | Python | 0 | 2026-08-05 | Parity-focused Rust clone. GitHub lists the language as Python while the README says Rust; unresolved. | R |
| [DevvGwardo/hermes-tui](https://github.com/DevvGwardo/hermes-tui) | Standalone terminal UI client built with Go and the Bubble Tea framework. | Go | 0 | 2026-10-01 | Go option. No stars. | R |
| [KenKinCOM/hermes-tui](https://github.com/KenKinCOM/hermes-tui) | Textual (Python TUI framework) client that connects to the Hermes gateway. Three columns: session list, chat with input, and global status. README is in Chinese. | Python | 0 | 2026-06-04 | Session-list layout in Python. No stars. | R |
| [liftaris/herm](https://github.com/liftaris/herm) | Terminal UI built with OpenTUI (a TypeScript TUI framework). Sidebar and tabs cover model and profile state, sessions, agents, analytics, skills, cron, toolsets, config, memory and kanban. | TypeScript | 301 | 2026-07-29 | Longest feature list in its README among third-party terminal clients found. | R |
| [MasterDenis/herm](https://github.com/MasterDenis/herm) | Same README opening as liftaris/herm. The relationship is unverified; it may be a copy. | none listed | 0 | 2026-06-01 | Probable duplicate with little independent value. | R |
| [joeynyc/hermes-hud](https://github.com/joeynyc/hermes-hud) | Python terminal monitor that reads the ~/.hermes folder and shows conversations, skills, memory, tool use and mistakes. The README calls it a consciousness monitor. | Python | 934 | 2026-07-05 | Monitor, not a chat client. Most-starred third-party terminal app found. | R |
| [mudrii/hermesd](https://github.com/mudrii/hermesd) | Python real-time TUI monitoring dashboard for Hermes. Screenshots use synthetic data. | Python | 119 | 2026-10-22 | Read-only monitoring. Recently active. | R |
| [GumbyEnder/hermes-tui-hud](https://github.com/GumbyEnder/hermes-tui-hud) | Terminal operator console for Hermes. The README says it uses only what Hermes and Hermes Web UI already expose. | Python | 49 | 2026-04-26 | Operator console. Relevant to a live status panel. | R |
| [LehaoLin/hermes-gate](https://github.com/LehaoLin/hermes-gate) | Python terminal UI for managing Hermes tmux sessions on remote cloud servers, with auto-reconnect and detach. | Python | 28 | 2026-04-21 | Remote session UI. Relevant to an always-running backend that survives dropped connections. | R |
| [kzndk/hermachy](https://github.com/kzndk/hermachy) | Renders the native Hermes TUI locally while the agent runs remotely over OpenSSH. Commands run on the local machine or on the Hermes host. | JavaScript | 0 | 2026-10-09 | Local-remote split for a terminal client. No stars. | R |
| [shubhsaxena2020/hermes-fleet-ctrl](https://github.com/shubhsaxena2020/hermes-fleet-ctrl) | TypeScript real-time TUI plus a WebSocket and server-sent events control plane for a fleet of tmux-backed Hermes sessions on one server. | TypeScript | 1 | 2026-08-30 | Fleet-control terminal UI for background sessions. | R |
| [marcosleu123-a11y/Hermes_Agents](https://github.com/marcosleu123-a11y/Hermes_Agents) | Self-described 'my own Hermes TUI' in Python. The README has 18 Hermes mentions; the code was not inspected. | Python | 0 | 2026-06-28 | Depth unverified. No stars. | R (partial) |
| [itsPremkumar/hermes-cli-tui](https://github.com/itsPremkumar/hermes-cli-tui) | Python 'Hermes CLI - Fleet Management TUI'. The repository has no README. | Python | 0 | 2026-09-08 | Unverified. No README. | D |
| [Zwx-tech/orbit-ai](https://github.com/Zwx-tech/orbit-ai) | Go TUI for managing a local Hermes setup. | Go | 0 | 2026-06-03 | Depth unverified. No stars. | D |
| [achappell/hermes-relay-tui](https://github.com/achappell/hermes-relay-tui) | Textual terminal UI for authenticated Hermes voice sessions: typed text, microphone turns and streamed audio. | Python | 1 | 2026-10-22 | Voice-focused terminal client. Shows a terminal client can carry audio. | R |
| [RapidWebs-Enterprise/hermes-help](https://github.com/RapidWebs-Enterprise/hermes-help) | Python configuration CLI with a TUI wizard that validates, queries, diffs and exports Hermes configuration. | Python | 1 | 2026-07-02 | Configuration from the terminal, which the operator asked for. | D |
| [Tranquil-Flow/hermes-neurovision](https://github.com/Tranquil-Flow/hermes-neurovision) | Python terminal visualizer with 100+ animated themes that react to agent activity. | Python | 67 | 2026-03-16 | Decorative overlay, not a client. Listed in the curated Hermes Atlas as a tool. | D |
| [iko-soy/bg-agents-panel](https://github.com/iko-soy/bg-agents-panel) | Live TUI panel that shows background agents and processes for Hermes. | Python | 0 | 2026-07-11 | Background-work monitor. | D |
| [edrethardo/hermes-i3](https://github.com/edrethardo/hermes-i3) | Tiling terminal panes in a browser tab. Each pane runs hermes --tui and it is served as a dashboard page. | Python | 0 | 2026-09-18 | Multi-pane layout, web-hosted. | D |
| [lEWFkRAD/hermes-tui-web](https://github.com/lEWFkRAD/hermes-tui-web) | Serves the Hermes TUI as a mobile-friendly web page over Tailscale (a private network service). | JavaScript | 0 | 2026-08-04 | Remote access route to the TUI. | D |
| [lsaether/hermes-remote-control](https://github.com/lsaether/hermes-remote-control) | TypeScript mobile-friendly remote control for Hermes TUI sessions. | TypeScript | 0 | 2026-06-02 | Remote control client. | D |
| [Konor11/hermes-remote-gateway](https://github.com/Konor11/hermes-remote-gateway) | Hermes CLI plugin that connects the native TUI to a remote gateway through a local WebSocket proxy. | Python | 1 | 2026-10-03 | Shows the remote attach route for the first-party TUI. | D |
| [lsaether/acp-tui](https://github.com/lsaether/acp-tui) | Minimal terminal debugger for any ACP (Agent Client Protocol) server over WebSocket. It shows raw protocol frames. The description says it pairs with hermes-bridge for Hermes sessions. | Python | 1 | 2026-06-10 | Generic ACP terminal client. Useful as a test harness for an ACP server. | R |
| [almafazi/hermes-tui-extension](https://github.com/almafazi/hermes-tui-extension) | VS Code extension that opens a sidebar terminal running hermes --tui. | TypeScript | 0 | 2026-04-20 | Editor-embedded TUI. | R |
| [ElChacoVeloz/omarchy-hermes-modal](https://github.com/ElChacoVeloz/omarchy-hermes-modal) | Drop-down Hermes TUI modal for the Omarchy desktop environment. | QML | 0 | 2026-08-25 | Launcher-style overlay. | D |
| [mbot11/omarchy-hermes-deck](https://github.com/mbot11/omarchy-hermes-deck) | Companion for Omarchy that shows status, sessions, usage, kanban and controls, plus a drop-down TUI modal. | Python | 2 | 2026-09-22 | Status panel plus terminal modal. | D |
| [batrachianai/toad](https://github.com/batrachianai/toad) | Terminal user interface for coding agents (Claude, Gemini, Codex and others) that supports ACP-compatible agents. The README does not name Hermes. License AGPL-3.0. | Python | 3,484 | 2026-05-26 | Largest general terminal host for ACP agents found, by stars. Whether it can launch hermes acp is unverified. | R |

**Multi-tool terminal UIs** (Hermes is one of several supported agents)

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [juli-gonzalez/mux](https://github.com/juli-gonzalez/mux) | Local orchestrator with a TUI dashboard for Claude Code, Codex, OpenCode, Pi, OpenClaw and Hermes. | Python | 0 | 2026-08-18 | Terminal multi-agent dashboard that includes Hermes. No stars. | R |
| [agentik-os/AGK-TUI](https://github.com/agentik-os/AGK-TUI) | Terminal control plane for durable Hermes, Codex, Claude Code, OpenCode and OpenRouter terminal sessions. The README refers to Hermes 52 times and to a larger platform. | Python | 1 | 2026-09-05 | Heavy Hermes coupling inside a larger platform. | R |
| [yechang1450/open-tui-orchestrator](https://github.com/yechang1450/open-tui-orchestrator) | Windows-only orchestrator for agent CLIs with parallel TUI windows. Adapters cover Codex, pi, opencode, Kimi Code, Claude Code, OpenClaw and Hermes Agent. | JavaScript | 0 | 2026-10-19 | Parallel-window orchestration. Windows only. | R |
| [bvolpato/omnisession](https://github.com/bvolpato/omnisession) | Local CLI and terminal picker that finds and continues sessions from many coding agents. The README mentions Hermes 5 times. | Rust | 6 | 2026-10-21 | Session-continuation picker across agents. | R |
| [luoyuctl/agenttrace](https://github.com/luoyuctl/agenttrace) | Rust terminal UI and CLI that audits coding-agent sessions for cost, tokens, latency and failures. The README mentions Hermes once. | Rust | 148 | 2026-10-19 | Rust TUI prior art for session audit, not Hermes-specific. | R |

## TUI extensions, themes, and widgets (not clients)

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [joeynyc/hermes-skins](https://github.com/joeynyc/hermes-skins) | Python community skins (visual themes) for the Hermes CLI. | Python | 617 | 2026-06-28 | Theme prior art. | D |
| [bchop-studio/hermes-skins-pack](https://github.com/bchop-studio/hermes-skins-pack) | 101 CLI and TUI skin themes for Hermes. | HTML | 85 | 2026-09-29 | Theme prior art. | D |
| [devnyks/hermes-session-loop](https://github.com/devnyks/hermes-session-loop) | Claude Code-style recurring prompts for Hermes TUI sessions. | Python | 0 | 2026-08-10 | Directly echoes a Claude Code feature. No stars. | D |
| [devnyks/hermes-scrollboost](https://github.com/devnyks/hermes-scrollboost) | Smooth transcript scrolling for the Hermes TUI. | Python | 0 | 2026-08-14 | TUI enhancement. No stars. | D |
| [himanusia/hermes-tui-quota](https://github.com/himanusia/hermes-tui-quota) | Provider quota widget for the Hermes TUI. | JavaScript | 0 | 2026-10-19 | Widget. No stars. | D |
| [E-R-Butch/hermes-opencode-go-widget](https://github.com/E-R-Butch/hermes-opencode-go-widget) | Usage card for the Hermes TUI showing OpenCode Go usage windows. | JavaScript | 0 | 2026-09-23 | Widget. No stars. | D |
| [clearmud/hermes-chat-bubble](https://github.com/clearmud/hermes-chat-bubble) | Floating TUI chat bubble plugin for the Hermes dashboard. | Shell | 32 | 2026-05-30 | Dashboard widget. | D |
| [NaimioChan/hermes-bubbles](https://github.com/NaimioChan/hermes-bubbles) | Bubble-style chat page for the Hermes web dashboard that replaces the built-in terminal chat. | TypeScript | 4 | 2026-10-21 | Web chat alternative. | D |
| [v-y-archive/hermes-tui-distraction-free](https://github.com/v-y-archive/hermes-tui-distraction-free) | Plugin that strips emoji and the pet from the TUI. | Python | 2 | 2026-08-22 | TUI setting plugin. | D |
| [OLDBAI213/hermes-tui-reverse-study](https://github.com/OLDBAI213/hermes-tui-reverse-study) (archived) | Reverse-engineering study of the Hermes TUI architecture, design and display system. Archived. | none listed | 0 | 2026-07-15 | Architecture notes on the first-party TUI. | D |
| [OLDBAI213/hermes-tui-zh](https://github.com/OLDBAI213/hermes-tui-zh) | Chinese localization extension for the TUI. | TypeScript | 0 | 2026-08-11 | Localization. | D |
| [osamahbeig/hermes-skin-lab](https://github.com/osamahbeig/hermes-skin-lab) | Browser workbench for designing TUI skins. | JavaScript | 1 | 2026-07-04 | Skin design tool. | D |
| [2NN300HU/braille-painter](https://github.com/2NN300HU/braille-painter) | Braille editor for TUI hero banners. | TypeScript | 0 | 2026-08-25 | Banner art tool. | D |

## Third-party desktop apps

18 rows.

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [fathah/hermes-desktop](https://github.com/fathah/hermes-desktop) | Community native desktop app (Electron, TypeScript) for installing, configuring and chatting with Hermes. The README calls the app 'Hermes One', says it is community maintained, and says it runs the official install script. | TypeScript | 14,448 | 2026-10-13 | Largest Hermes-specific desktop client by stars in this pool. Not a Nous product. | R |
| [dodo-reach/hermes-desktop](https://github.com/dodo-reach/hermes-desktop) | Native macOS companion for Hermes, used on the local Mac or over SSH. Covers sessions, workflows, Kanban, files, usage, skills and cron. | Swift | 2,032 | 2026-06-19 | Pure-SSH model for a remote backend. | R |
| [nickvasilescu/hermes-desktop-os1](https://github.com/nickvasilescu/hermes-desktop-os1) | Native macOS Hermes workspace for Orgo cloud computers and SSH hosts. | Swift | 529 | 2026-05-09 | Remote-workspace desktop app. | D |
| [RedWoodOG/Hermes-Desktop](https://github.com/RedWoodOG/Hermes-Desktop) | C# desktop app titled 'Hermes Desktop Updated'. The README mentions Hermes 40 times. The code was not inspected. | C# | 177 | 2026-05-30 | Windows-oriented desktop client. Depth unverified. | R (partial) |
| [acegraphx/hermes-desktop-win](https://github.com/acegraphx/hermes-desktop-win) | Native Windows (WPF, a C# UI framework) port of Hermes Desktop. SSH-based workspace. | C# | 69 | 2026-05-21 | Windows port of the desktop app. | D |
| [meespace/hermes-desktop-community](https://github.com/meespace/hermes-desktop-community) | Community port of Hermes Desktop from Electron to Tauri (a Rust desktop framework). | TypeScript | 36 | 2026-07-24 | Lighter-weight desktop port. | D |
| [hermes-webui/hermes-desktop-rust](https://github.com/hermes-webui/hermes-desktop-rust) | Tauri 2 desktop app that runs the hermes-webui interface on Windows, Linux and macOS, with an SSH tunnel setup. | Rust | 50 | 2026-09-24 | Desktop wrapper around a web client. | R |
| [Eynzof/Hermes-CN-Desktop](https://github.com/Eynzof/Hermes-CN-Desktop) | Windows-first Hermes desktop app built with Tauri, TypeScript and Rust. Isolates the Hermes core. | TypeScript | 1,709 | 2026-09-21 | Desktop app with an isolated core. The repository name uses 'CN'; the description does not explain it. | D |
| [Mahiruxia/hermes-forge](https://github.com/Mahiruxia/hermes-forge) | Local-first desktop client built with Electron, React and TypeScript. The README says it is a community project, not the official client. | TypeScript | 140 | 2026-09-17 | Community desktop client. | R |
| [AlopecicNovice/hermes-agent-client](https://github.com/AlopecicNovice/hermes-agent-client) | Windows native desktop client built with Tauri v2. | Rust | 0 | 2026-05-02 | Windows desktop client. No stars. | D |
| [aivrar/portable-hermes-agent](https://github.com/aivrar/portable-hermes-agent) | Portable Windows desktop bundle with a GUI, local models and a large tool set. | Python | 237 | 2026-10-21 | Bundled desktop distribution. | D |
| [sir1st/hermes-desktop](https://github.com/sir1st/hermes-desktop) | Moved to EKKOLearnAI/hermes-web-ui; the repository is kept for historical releases. | JavaScript | 73 | 2026-06-01 | Superseded. | D |
| [NiuN101/Hermes-Auto](https://github.com/NiuN101/Hermes-Auto) | Windows GUI launcher for Hermes in WSL (Windows Subsystem for Linux) with one-click model switching. | Python | 1 | 2026-06-13 | Launcher, not a chat client. | D |
| [Adolanium/hermes-terminal](https://github.com/Adolanium/hermes-terminal) | Runs the Hermes TUI as a workspace tab in Hermes Desktop. | JavaScript | 2 | 2026-10-17 | Terminal tab inside a desktop client. | D |
| [abundantbeing/hermes-browser-extension](https://github.com/abundantbeing/hermes-browser-extension) | Browser side panel that connects web context to the local Hermes runtime. | JavaScript | 1,697 | 2026-10-20 | Browser client. | D |
| [CorsenAI/hermes-connector](https://github.com/CorsenAI/hermes-connector) | Chrome extension that gives Hermes exact sessions and user-selected tabs, local-first. | Python | 28 | 2026-10-20 | Browser client. Small. | D |
| [dannyshmueli/obsidian-hermes-console](https://github.com/dannyshmueli/obsidian-hermes-console) | Runs Hermes inside Obsidian (a notes app) with selected-note context. | TypeScript | 65 | 2026-07-24 | Editor-embedded client. | D |
| [iamlukethedev/Herald-OS](https://github.com/iamlukethedev/Herald-OS) | Agent-native operating system with Hermes as the interface. The description says it is independent and not affiliated with Nous Research. | TypeScript | 541 | 2026-10-20 | Broad scope. Not a terminal client. | D |

## Third-party web UIs, dashboards, and control panels

22 rows.

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [nesquena/hermes-webui](https://github.com/nesquena/hermes-webui) | Lightweight dark-themed web UI for Hermes. The README claims full parity with the CLI. Python backend, vanilla JavaScript front end, no build step. | Python | 19,092 | 2026-10-22 | Most-starred Hermes-specific client in this pool. A baseline for feature parity. | R |
| [outsourc-e/hermes-workspace](https://github.com/outsourc-e/hermes-workspace) | Web workspace with chat, terminal, memory, skills and an inspector. The README says it runs on the unmodified upstream install. | JavaScript | 6,738 | 2026-10-19 | Broad web workspace with an embedded terminal. | R |
| [EKKOLearnAI/ekko-studio](https://github.com/EKKOLearnAI/ekko-studio) | Local-first AI workspace for multi-agent chat, coding and visual workflows, on desktop and web. The README names Hermes Agent alongside its own Ekko Agent and 14 coding-agent integrations. The EKKOLearnAI/hermes-studio and hermes-web-ui names redirect here. | TypeScript | 11,504 | 2026-10-22 | Large multi-agent workspace with Hermes as the lead integration. | R+D |
| [JPeetz/Hermes-Studio](https://github.com/JPeetz/Hermes-Studio) | Web UI and dashboard for Hermes: chat, memory, skills, terminal, approvals, multi-agent crews and a cron manager. | TypeScript | 390 | 2026-07-03 | Multi-agent web UI with a terminal panel. | R |
| [xaspx/hermes-control-interface](https://github.com/xaspx/hermes-control-interface) | Self-hosted web dashboard with chat, browser terminals (xterm.js), files, cron, token analytics, MCP servers and swarm pipelines. | JavaScript | 900 | 2026-06-28 | Web control surface with a terminal. | R |
| [joeynyc/hermes-hudui](https://github.com/joeynyc/hermes-hudui) | Python web UI consciousness monitor for Hermes. | Python | 1,847 | 2026-10-11 | Monitor on the web. | D |
| [agent37-platform/minions](https://github.com/agent37-platform/minions) | Mission Control for Hermes Agent: tracks long-running jobs so the operator does not need to watch terminal sessions. | TypeScript | 657 | 2026-08-15 | Background-job tracking for delegated work. | R |
| [sharbelxyz/hermes-agent-mission-control](https://github.com/sharbelxyz/hermes-agent-mission-control) | Self-hostable dashboard template ('Hermy HQ') that pairs with a local Hermes over a database. Dispatches work, reviews plans and approves side effects. | TypeScript | 453 | 2026-07-26 | Dispatch and approval dashboard. | R |
| [shannhk/hermes-agent-control-room](https://github.com/shannhk/hermes-agent-control-room) | Control-room template for managing Hermes agents, from one agent up to specialist teams and orchestrated workflows. | Shell | 732 | 2026-05-16 | Team-level dashboard template. | D |
| [praveen-ks-2001/hermes-agent-template](https://github.com/praveen-ks-2001/hermes-agent-template) | One-click Railway deployment template with a web admin dashboard for provider and channel setup and gateway supervision. | Python | 324 | 2026-10-21 | Hosted deployment with an admin panel. | D |
| [przbadu/hermes-ui](https://github.com/przbadu/hermes-ui) | Web UI for Hermes, installable as a PWA (progressive web app), extracted from Hermes Desktop. | TypeScript | 142 | 2026-08-19 | Mobile-capable web client. | D |
| [pyrate-llama/hermes-ui](https://github.com/pyrate-llama/hermes-ui) | Command center for Hermes: chat, steer, browse files, manage skills and monitor. | HTML | 203 | 2026-06-30 | Web command center. | D |
| [lotsoftick/hermes_client](https://github.com/lotsoftick/hermes_client) | Web chat interface that manages several Hermes profiles as separate agents, with streaming replies. | TypeScript | 233 | 2026-06-10 | Multi-profile web chat. | R |
| [Kori-x/hermes-dashboard](https://github.com/Kori-x/hermes-dashboard) | Dashboard for monitoring and managing Hermes sessions, tool use and activity in real time. | TypeScript | 66 | 2026-04-08 | Monitoring dashboard. | D |
| [sanchomuzax/hermes-webui](https://github.com/sanchomuzax/hermes-webui) | Process monitoring and configuration dashboard for Hermes. Not the same project as nesquena/hermes-webui. | Python | 114 | 2026-03-24 | Monitoring dashboard. Name collision. | D |
| [chrisryugj/hermes-dashboard](https://github.com/chrisryugj/hermes-dashboard) | Web dashboard for the Hermes gateway: configuration, MCP, cron and model management without the CLI. | HTML | 40 | 2026-07-30 | Configuration dashboard. | D |
| [Itsme23476/jarvis-hermes-dashboard](https://github.com/Itsme23476/jarvis-hermes-dashboard) | Voice-enabled mission-control dashboard for Hermes. | Python | 55 | 2026-08-08 | Voice dashboard. | D |
| [Euraika-Labs/pan-ui](https://github.com/Euraika-Labs/pan-ui) | Self-hosted AI workspace for Hermes: chat, skills, extensions, memory, profiles and runtime controls. | TypeScript | 99 | 2026-07-10 | Web workspace. | D |
| [reallygood83/hermes-for-web](https://github.com/reallygood83/hermes-for-web) | Web workbench for Hermes with setup packs, artifacts and memory-aware onboarding. | Python | 83 | 2026-04-06 | Web workbench. | D |
| [filipj9/Hermes-Control-Deck](https://github.com/filipj9/Hermes-Control-Deck) | CLI-first mobile PWA control deck for Hermes WebUI and the local Codex CLI. | JavaScript | 137 | 2026-10-02 | Mobile control surface. | D |
| [stasstepv/hermes-pwa](https://github.com/stasstepv/hermes-pwa) | Unofficial mobile PWA that installs as a plugin inside the Hermes dashboard. | TypeScript | 33 | 2026-09-20 | Mobile web client. | D |
| [Drstone0007/agentos-mission-control](https://github.com/Drstone0007/agentos-mission-control) | Mission-control dashboard for Hermes AgentOS with a live feed and five tabs. | HTML | 1 | 2026-06-22 | Dashboard. Small. | D |

## Mobile and device clients

31 rows.

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [uzairansaruzi/hermex](https://github.com/uzairansaruzi/hermex) | Native iPhone app for a Hermes agent. | Swift | 1,545 | 2026-10-22 | Most-starred Hermes-specific native mobile app in this pool. | D |
| [kaishi00/hermes-conduit](https://github.com/kaishi00/hermes-conduit) | Native SwiftUI iOS client for Hermes Agent. | Swift | 382 | 2026-10-22 | Mobile client. | D |
| [rusty4444/hermes-android](https://github.com/rusty4444/hermes-android) | Android client that chats with Hermes sessions over local Wi-Fi or a private network. | Dart | 428 | 2026-10-22 | Mobile client. | D |
| [Hy4ri/hermes-mobile](https://github.com/Hy4ri/hermes-mobile) | Native Android app for Hermes Agent. | Kotlin | 319 | 2026-10-21 | Mobile client. | D |
| [Codename-11/hermes-relay](https://github.com/Codename-11/hermes-relay) | Native Android companion with streaming chat, voice and agent management, plus a single-binary CLI that gives the agent access to machines the operator pairs. | Kotlin | 368 | 2026-10-20 | Mobile client plus a machine-side CLI. | R |
| [Giton22/herald](https://github.com/Giton22/herald) | Unofficial Android client for Hermes Agent. | Kotlin | 128 | 2026-10-20 | Mobile client. | D |
| [xP3ta/hermes-console](https://github.com/xP3ta/hermes-console) | Private native Android app for a self-hosted Hermes, with chat that shows the agent's work. | Dart | 107 | 2026-10-21 | Mobile client. | D |
| [omarqaterge/hermes-mobile-app](https://github.com/omarqaterge/hermes-mobile-app) | Native Android app that runs Hermes on the phone itself through Termux (a Linux environment for Android). | TypeScript | 82 | 2026-10-21 | On-device Hermes. | D |
| [CodeUpdaterBot/Hermes-Mobile-App](https://github.com/CodeUpdaterBot/Hermes-Mobile-App) | Mobile app for Hermes built around bots, running on the operator's own computer. | TypeScript | 81 | 2026-10-17 | Mobile client. | D |
| [goncharik/hermes-mobile](https://github.com/goncharik/hermes-mobile) | Mobile companion for Hermes Agent (Swift). | Swift | 80 | 2026-10-12 | Mobile client. | D |
| [goosehouse-llc/redde](https://github.com/goosehouse-llc/redde) | iPhone, iPad and CarPlay client for Hermes or any OpenAI-compatible model server. | Swift | 77 | 2026-10-21 | Mobile client that also works with other model servers. | R |
| [unsupportedpastels/mercury](https://github.com/unsupportedpastels/mercury) | Free, open-source mobile clients for Hermes on Android and iOS. Unofficial. | Kotlin | 55 | 2026-10-21 | Mobile client. | D |
| [sesaloy/hermes-mobile](https://github.com/sesaloy/hermes-mobile) | Self-contained ARM64 Android app that runs Hermes locally. | Python | 49 | 2026-10-02 | On-device Hermes. | D |
| [jeromeleeqy-hash/Hermes-Android](https://github.com/jeromeleeqy-hash/Hermes-Android) | Android mobile client for a self-hosted Hermes gateway. | Kotlin | 46 | 2026-10-09 | Mobile client. | D |
| [adebnar/hermes-android](https://github.com/adebnar/hermes-android) | Native Android client for the Hermes gateway with chat plus management of sessions, models, cron and usage. | Kotlin | 41 | 2026-10-03 | Mobile client with management screens. | D |
| [shilp26/hermes-go](https://github.com/shilp26/hermes-go) | Mobile client for self-hosted Hermes, with a site and release assets. | HTML | 41 | 2026-10-18 | Mobile client. | D |
| [AIowa-LLC/hermes-fleet](https://github.com/AIowa-LLC/hermes-fleet) | Native iPhone control plane for user-owned Hermes deployments. It connects directly to gateways the user selects. | Swift | 24 | 2026-10-22 | Fleet control on a phone. | R |
| [demi-hl/hermes-ios-app](https://github.com/demi-hl/hermes-ios-app) | Desktop and mobile cockpit for a local Hermes: chat, repos, fleet, kanban, terminal and editor. | TypeScript | 26 | 2026-06-25 | Broad cockpit client. | D |
| [2winter-dev/iHermes](https://github.com/2winter-dev/iHermes) | Mobile-first Hermes client built with Expo (a React Native toolkit). | TypeScript | 30 | 2026-05-24 | Mobile client. | D |
| [AxelFooley/hermes-ios](https://github.com/AxelFooley/hermes-ios) | Native SwiftUI client described as the Hermes TUI rebuilt for iPhone and iPad. | Swift | 0 | 2026-08-25 | Mobile rebuild of the TUI idea. No stars. | D |
| [ComputerByte/hermex-android](https://github.com/ComputerByte/hermex-android) | Native Android (Jetpack Compose) client for a self-hosted hermes-webui server. | Kotlin | 83 | 2026-07-20 | Mobile client for the web UI. | D |
| [researchoors/hermes-native](https://github.com/researchoors/hermes-native) | Native macOS and iOS client for the Hermes gateway over WebSocket JSON-RPC. The repository description claims full TUI parity. | Swift | 2 | 2026-07-28 | Native client on the same gateway protocol the TUI uses. | R |
| [liamlai88/hermes-agent-clients](https://github.com/liamlai88/hermes-agent-clients) | macOS and iOS clients for the self-hosted Hermes API server. | Swift | 4 | 2026-05-13 | Native Apple clients. | D |
| [cryptobredda/hermes-agent-clients](https://github.com/cryptobredda/hermes-agent-clients) | Mobile and desktop clients for self-hosted Hermes servers. | TypeScript | 0 | 2026-08-22 | Clients for both mobile and desktop. | D |
| [gravol/hermexv2](https://github.com/gravol/hermexv2) | Native client experiment that copies the Hermes WebUI look and feel. | Kotlin | 0 | 2026-09-12 | Experimental. | D |
| [Spacetrek-Collective/hermes-ai](https://github.com/Spacetrek-Collective/hermes-ai) | Hermes client with a Live2D animated character. | TypeScript | 1 | 2026-06-12 | Novelty client. | D |
| [charleypeng/iHermes](https://github.com/charleypeng/iHermes) | Client for remote gateway and SSH access. | none listed | 0 | 2026-08-12 | Remote client. | D |
| [neurologicby/voice-hermes-gateway-client](https://github.com/neurologicby/voice-hermes-gateway-client) | Voice client for Hermes on Windows. | Python | 0 | 2026-09-14 | Voice-only client. | D |
| [Adolanium/hermes-gadget-sdk](https://github.com/Adolanium/hermes-gadget-sdk) | SDK for voice and text devices for Hermes, with ESP32 (small microcontroller) firmware, a Linux client and a desktop simulator. | C++ | 427 | 2026-10-22 | Device client, not a terminal client. | D |
| [rapidreview-io/merv-hermes-client](https://github.com/rapidreview-io/merv-hermes-client) (archived) | Generated Hermes client for another product called Merv. Archived. | Python | 0 | 2026-10-16 | Archived. Not a general client. | D |
| [faizalardhi16/gather](https://github.com/faizalardhi16/gather) | Requirement-gathering chat UI bound to Hermes. | TypeScript | 0 | 2026-08-16 | Single-purpose chat UI. | D |

## ACP, gateway, editor, and launcher bridges

24 rows. ACP is the protocol the first-party editor setups use. A bridge in this table either adapts that protocol or launches a session.

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [atrium-cloud/hermes-acp](https://github.com/atrium-cloud/hermes-acp) | ACP v1 adapter for Hermes that uses the tui_gateway backend. | TypeScript | 0 | 2026-10-10 | Protocol adapter for editor and terminal clients. | R |
| [dismantl/hermes-acp-relay](https://github.com/dismantl/hermes-acp-relay) | WebSocket relay and stdio bridge that exposes the ACP adapter over a private network, without changing upstream Hermes. | Python | 0 | 2026-10-22 | Remote access path for ACP clients. | R |
| [djasdh/hermes-acp-http](https://github.com/djasdh/hermes-acp-http) | Streamable HTTP and WebSocket transport for Hermes ACP. | Rust | 1 | 2026-08-17 | Transport option for ACP clients. Small. | D |
| [VoixKz/hermes-acp-sdk](https://github.com/VoixKz/hermes-acp-sdk) | Python SDK that drives Hermes from any Python app over ACP, with streamed events. | Python | 0 | 2026-09-25 | Library for building a client. | D |
| [Occa-Labs/adapter-hermes](https://github.com/Occa-Labs/adapter-hermes) | Runtime adapter that drives a remote 'hermes acp' process over SSH. | TypeScript | 0 | 2026-05-26 | Remote-process adapter. | D |
| [mjhankin-svg/t3code-hermes](https://github.com/mjhankin-svg/t3code-hermes) | T3 Code app with a first-class Hermes ACP provider for isolated homelab development. | TypeScript | 0 | 2026-07-16 | Hermes as a provider inside another coding app. | D |
| [simonfunk/faiden](https://github.com/simonfunk/faiden) | Early local-first macOS terminal built around durable threads. It can start a new Hermes ACP session. Written in Rust. | Rust | 0 | 2026-09-09 | Terminal app that already hosts Hermes over ACP. Closest to a terminal-first design found. | R |
| [mvdbastos/hermes-acp-agents](https://github.com/mvdbastos/hermes-acp-agents) | Third-party ACP agent backend plugins that let Hermes call Claude Code, Codex CLI and GitHub Copilot. | Python | 2 | 2026-07-28 | Hermes delegating to other coding agents. | D |
| [HashVal/hermes-acp-plugin](https://github.com/HashVal/hermes-acp-plugin) | Runs Claude Code and Codex from Hermes gateway chats through ACP. | Python | 0 | 2026-10-09 | Hermes delegating to Claude Code. | D |
| [memoverflow/hermes-acp-bridge](https://github.com/memoverflow/hermes-acp-bridge) | WebSocket bridge that connects Hermes to Claude Code and Codex CLI. | Python | 0 | 2026-05-14 | Hermes delegating to Claude Code. | D |
| [trojan-lord/opencode-hermes-acp](https://github.com/trojan-lord/opencode-hermes-acp) | Drives OpenCode as Hermes's default model provider over ACP. | Python | 1 | 2026-10-17 | Provider bridge. | D |
| [agentic-control-plane/hermes-acp-plugin](https://github.com/agentic-control-plane/hermes-acp-plugin) | Policy check, audit and veto for every tool call made through ACP. | Python | 2 | 2026-10-07 | Governance layer. | D |
| [WOOWTECH/hermes-paseo-plugin](https://github.com/WOOWTECH/hermes-paseo-plugin) | Paseo setup, encrypted remote pairing and Hermes ACP conversations. | Python | 0 | 2026-10-12 | Remote pairing with a Paseo client. | D |
| [rifandyzv/hermes-acp-bridge](https://github.com/rifandyzv/hermes-acp-bridge) | ACP bridge (no description given). | TypeScript | 0 | 2026-04-26 | Unverified. | D |
| [gad0n3/hermes-acp-launcher](https://github.com/gad0n3/hermes-acp-launcher) | Minimal wrapper for launching Hermes ACP for the ACP registry. | Shell | 0 | 2026-07-12 | Launcher. | D |
| [patrick-lai/hermes-acp-plugins](https://github.com/patrick-lai/hermes-acp-plugins) | Hermes ACP plugins (no description given). | Python | 0 | 2026-09-01 | Unverified. | D |
| [jove-rina/rina-hermes-acp](https://github.com/jove-rina/rina-hermes-acp) | No description given. 4 stars. | TypeScript | 4 | 2026-07-02 | Unverified. | D |
| [Joseph19820124/hermes-acp-architecture-analysis](https://github.com/Joseph19820124/hermes-acp-architecture-analysis) | Written analysis of the Hermes ACP call chain and architecture. | none listed | 0 | 2026-04-27 | Reading material on how ACP is wired. | D |
| [0xNyk/hermes-buzz](https://github.com/0xNyk/hermes-buzz) | Starter kit that combines Hermes with Buzz and its ACP onboarding. | Shell | 23 | 2026-08-25 | Buzz integration kit. | D |
| [r0b0tlab/hermes-buzz-shared-profile](https://github.com/r0b0tlab/hermes-buzz-shared-profile) | macOS skill that shares one writable Hermes profile across Buzz and ACP surfaces. | Python | 52 | 2026-07-30 | Profile sharing across clients. | D |
| [block/buzz](https://github.com/block/buzz) | Block's open-source, self-hostable workspace where people and agents share channels (the description reads 'A hive mind communication platform'). Nous documents three ways to connect Hermes to it. | Rust | 36,236 | 2026-10-22 | Third-party workspace with an official Hermes integration. Relevant to multi-channel orchestration. | D+F |
| [paperclipai/paperclip](https://github.com/paperclipai/paperclip) | Open-source app for managing agents at work (its own description). Nous publishes a Hermes adapter for it. | TypeScript | 105,760 | 2026-10-22 | General agent-operations platform. The Hermes part is the adapter. | D+F |
| [AIAtHome-Adam/vibelaunch](https://github.com/AIAtHome-Adam/vibelaunch) | Windows session manager with presets for Claude, Codex, Hermes and other tools. | PowerShell | 0 | 2026-07-27 | Launcher for several agent sessions. | D |
| [CarloCamacho/noctalia-agent-harness](https://github.com/CarloCamacho/noctalia-agent-harness) | Plugin for the Noctalia desktop shell: a chat bar that pops out into a Pi or Hermes TUI. | Luau | 0 | 2026-09-08 | Launcher for the terminal UI. | D |

## Orchestration, multi-agent, Kanban, and Claude Code bridges (Hermes-specific)

36 rows. The most direct matches for 'several agents I can send work to and check on' are 100yenadmin/hermes-teammates, xuyang-liu16/hermes-code-bridge, and forcewake/hermes-conductor.

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [rlaope/oh-my-hermes](https://github.com/rlaope/oh-my-hermes) | Packages for Hermes: skills, a coding harness, long-term memory, model-optimized subagents and bot-team workflows. | Python | 3,541 | 2026-10-22 | Largest orchestration package by stars. | D |
| [witt3rd/oh-my-hermes](https://github.com/witt3rd/oh-my-hermes) | Multi-agent orchestration skills for Hermes, inspired by oh-my-claudecode. | Python | 369 | 2026-09-27 | Orchestration skills. | D |
| [Salomondiei08/oh-my-hermes](https://github.com/Salomondiei08/oh-my-hermes) | Opinionated workflow layer for building, shipping and operating apps with Hermes. | Shell | 877 | 2026-09-22 | Workflow layer. | D |
| [tonbistudio/hermes-multi-agent-workflow](https://github.com/tonbistudio/hermes-multi-agent-workflow) | Multi-agent workflow for Hermes (the README heading). No description given. | Python | 486 | 2026-06-01 | Depth unverified. | R (partial) |
| [magnus919/hermes-profiles](https://github.com/magnus919/hermes-profiles) | Curated Hermes profiles for specialist agent swarms. | Python | 336 | 2026-06-27 | Profile presets for teams of agents. | D |
| [magnus919/hermes-council](https://github.com/magnus919/hermes-council) | Multi-agent debate system where expert agents argue a question in rounds. | Python | 55 | 2026-07-20 | Debate pattern. | D |
| [42-evey/hermes-plugins](https://github.com/42-evey/hermes-plugins) | Plugins for goal management, an inter-agent bridge, model selection and cost control. | Python | 531 | 2026-07-18 | Inter-agent messaging. | D |
| [100yenadmin/hermes-teammates](https://github.com/100yenadmin/hermes-teammates) | Named subagent teammates, each with its own instructions and model. The agent assigns work, checks on it and steers it mid-task. | Python | 88 | 2026-10-20 | Closest to 'several agents I can send work to and check on'. | R |
| [DevvGwardo/brain-mcp](https://github.com/DevvGwardo/brain-mcp) | Multi-agent coordination layer for Hermes with a SQLite store (a file-based database) and an MCP (Model Context Protocol) interface. | TypeScript | 8 | 2026-07-25 | Shared-memory coordination. Small. | D |
| [xuyang-liu16/hermes-code-bridge](https://github.com/xuyang-liu16/hermes-code-bridge) | Makes Hermes a command center that routes work to Codex, Kimi Code, Claude Code, OpenCode and Gemini CLI, then returns diffs and test results. | Python | 41 | 2026-07-22 | Direct example of Hermes orchestrating coding CLIs. | R |
| [forcewake/hermes-conductor](https://github.com/forcewake/hermes-conductor) | One Hermes conductor for many coding CLIs, with worktree lanes and verification gates. GitHub does not list a primary language. | none listed | 79 | 2026-08-25 | Multi-harness orchestration patterns. | R |
| [RodrigoSiliunas/hermes-claude-bridge](https://github.com/RodrigoSiliunas/hermes-claude-bridge) | Bridge from Hermes to the Claude Code CLI for delegated coding tasks. | Python | 1 | 2026-06-21 | Hermes delegating to Claude Code. Early stage. | D |
| [AlexAI-MCP/hermes-CCC](https://github.com/AlexAI-MCP/hermes-CCC) | Hermes Agent ported to the Claude Code Channel, with 46 native skills (the README mentions Hermes 28 times). | PowerShell | 134 | 2026-04-08 | Reverse direction: Hermes inside Claude Code. | R |
| [kristianvast/hermes-claude-auth](https://github.com/kristianvast/hermes-claude-auth) | Plugin for hermes-agent described as a Claude Code OAuth bypass (OAuth is a login standard). | Python | 399 | 2026-08-18 | Route for Claude model access. Policy implications not reviewed. | D |
| [mlennie/hermes-mcp](https://github.com/mlennie/hermes-mcp) | MCP server that lets Claude Desktop or another MCP client delegate tasks to a local Hermes. | Python | 67 | 2026-09-29 | Reverse direction: Hermes as a tool server. | D |
| [zaycruz/hermes-opencode-plugin](https://github.com/zaycruz/hermes-opencode-plugin) | OpenCode integration plugin that dispatches coding tasks to a multi-agent harness. | Python | 62 | 2026-10-01 | Delegation to OpenCode. | D |
| [smfworks/hermes-ai-team](https://github.com/smfworks/hermes-ai-team) | Builds an AI team of Hermes agents with memory, skills, a Kanban board and nightly research. | Python | 69 | 2026-10-04 | Team setup with Kanban. | D |
| [Ardha-Eco-System/RUDR9](https://github.com/Ardha-Eco-System/RUDR9) | One command sets up a nine-role AI engineering team with Kanban coordination. | Shell | 52 | 2026-07-15 | Preset engineering team. | D |
| [italeex/hermes-swarm-kit](https://github.com/italeex/hermes-swarm-kit) | Roles, a TUI, an approval gate and an agent board for a team of AI agents on Hermes. | Python | 0 | 2026-10-20 | Team kit with a terminal UI component. | D |
| [amirghm/hermes-kanban](https://github.com/amirghm/hermes-kanban) | Lightweight SQLite-backed Kanban board for Hermes with drag and drop, which detects agents automatically. | HTML | 51 | 2026-06-21 | Kanban board. | D |
| [GumbyEnder/hermes-kanban](https://github.com/GumbyEnder/hermes-kanban) | Obsidian plugin plus Hermes skill that turns Hermes into a project executor on Kanban boards. | Python | 277 | 2026-07-30 | Kanban in a notes app. | D |
| [Abruptive/Ankh.md](https://github.com/Abruptive/Ankh.md) | Multi-agent swarm framework. Its description calls it a Hermes Agent (Nous Research) 'launch exclusive'. | Shell | 75 | 2026-03-15 | Swarm framework tied to Hermes. Depth unverified. | D |
| [supermodeltools/bigiron](https://github.com/supermodeltools/bigiron) (archived) | AI-native software lifecycle built on Hermes with a code graph, gated at each phase. Archived. | Shell | 12 | 2026-04-02 | Lifecycle pipeline. Archived. | D |
| [howdymary/hermes-agent-metaharness](https://github.com/howdymary/hermes-agent-metaharness) | Meta-harness implementation for Hermes agent orchestration. | Python | 119 | 2026-07-11 | Harness layer. | D |
| [erenciracioglu-dotcom/hermes-self-evolution](https://github.com/erenciracioglu-dotcom/hermes-self-evolution) | Self-evolving harness with evolution, critic, verifier and gardener roles for Hermes. | Shell | 62 | 2026-07-27 | Harness with review roles. | D |
| [InfiniteWhispers/HermesAgent-MultiModel](https://github.com/InfiniteWhispers/HermesAgent-MultiModel) | Local multi-model agent framework with mixture-of-agents orchestration (several models answer and a combiner merges them). Its description says it runs Hermes with Ollama (a local model server). The README mentions Hermes 16 times. | none listed | 60 | 2026-07-13 | Multi-model routing. Relation to upstream code not verified. | R (partial) |
| [r0b0tlab/hermes-concurrent-agents](https://github.com/r0b0tlab/hermes-concurrent-agents) | Runs concurrent Hermes worker processes on shared-memory graphics processor (GPU) hardware. | Python | 83 | 2026-07-18 | Parallel workers. Hardware-specific. | D |
| [Sahil-SS9/hermes-project-stewardship](https://github.com/Sahil-SS9/hermes-project-stewardship) | Durable project ownership for Hermes agent fleets, with verified cycles and bounded initiatives. | JavaScript | 72 | 2026-10-19 | Fleet governance. | D |
| [dandacompany/deskrpg](https://github.com/dandacompany/deskrpg) | Self-hosted 3D virtual office where Hermes profiles act as staff who chat and hold meetings. | TypeScript | 188 | 2026-10-19 | Visual multi-agent office. | D |
| [JuniorXcoder/hermes-virtual-office](https://github.com/JuniorXcoder/hermes-virtual-office) | 3D office for an agent fleet with desks per division and meetings. | TypeScript | 135 | 2026-10-09 | Visual multi-agent office. | D |
| [magnusfroste/hermeshotel](https://github.com/magnusfroste/hermeshotel) | Multi-agent Hermes platform with a TUI control panel. | Python | 2 | 2026-05-15 | Includes a terminal control panel. | D |
| [linke-ai/hermes-agent-team](https://github.com/linke-ai/hermes-agent-team) | Local multi-agent team collaboration web system built on Hermes. | Python | 185 | 2026-05-20 | Team web app. | D |
| [Naroh091/hermes-war-room](https://github.com/Naroh091/hermes-war-room) | Browser dashboard on top of Hermes's multi-profile delegation and Kanban systems. | Vue | 356 | 2026-09-18 | Visual orchestration console. | R |
| [solomon2773/nora](https://github.com/solomon2773/nora) | Self-hosted control plane for OpenClaw and Hermes agent fleets on Docker or Kubernetes, with REST, CLI and MCP access. | TypeScript | 59 | 2026-10-21 | Fleet control plane with a CLI. | D |
| [agent37-platform/gateway](https://github.com/agent37-platform/gateway) | Single web API that routes requests to Hermes, OpenClaw, Claude Code or Codex on an instance. | TypeScript | 16 | 2026-10-10 | HTTP gateway for any client. | D |
| [simonyos/hermes-plugin-claude-code-tui](https://github.com/simonyos/hermes-plugin-claude-code-tui) | Plugin that supervises Claude Code TUI workers in tmux through an MCP agent bus. | Python | 0 | 2026-06-05 | Hermes supervising Claude Code terminals. | D |

## Multi-tool apps that include Hermes among other agents

These are not built only for Hermes. They are listed because they can run Hermes, or show its sessions, alongside other agents.

| Repository | What it is | Language | Stars | Last push | Relevance to a Claude Code-style terminal client | Basis |
|---|---|---|---:|---|---|---|
| [farion1231/cc-switch](https://github.com/farion1231/cc-switch) | Desktop manager for Claude Code, Codex, OpenCode, Hermes and other tools. It switches API providers and manages MCP servers, skills and prompts. | Rust | 147,361 | 2026-10-21 | Configuration-manager prior art across agents. Large. | R |
| [iOfficeAI/AionUi](https://github.com/iOfficeAI/AionUi) | 24/7 desktop cowork app for Hermes, Claude Code, Codex, OpenCode and 20+ other agents. | TypeScript | 33,686 | 2026-10-19 | Multi-agent desktop that includes Hermes. | R |
| [Waishnav/devspace](https://github.com/Waishnav/devspace) | Minimal coding-agent harness on MCP for ChatGPT, Claude, Hermes and others. | TypeScript | 5,289 | 2026-10-12 | Harness that includes Hermes. | D |
| [LiteLLM-Labs/litellm-agent-control-plane](https://github.com/LiteLLM-Labs/litellm-agent-control-plane) | One place to call OpenCode, Hermes, Claude Managed Agents, Cursor Agents and DeepAgents. | Rust | 1,319 | 2026-10-20 | Control plane that includes Hermes. | D |
| [freestylefly/wesight](https://github.com/freestylefly/wesight) | Desktop control console for local agents. It installs or reuses Claude Code, Codex, Hermes and other tools. | TypeScript | 955 | 2026-10-01 | Multi-agent desktop including Hermes. | R |
| [Orkas-AI/Orkas](https://github.com/Orkas-AI/Orkas) | Local-first multi-agent desktop app with a Commander that runs Claude Code, Codex, OpenCode, OpenClaw and Hermes sessions. | JavaScript | 2,146 | 2026-10-22 | Multi-agent desktop including Hermes. | R |
| [sno-ai/sno-station](https://github.com/sno-ai/sno-station) | Squad of agents (Codex, Claude Code, Cursor, Hermes, OpenClaw) that share an encrypted memory. | TypeScript | 765 | 2026-10-21 | Shared-memory squad including Hermes. | D |
| [mertkayacs/reevesagents](https://github.com/mertkayacs/reevesagents) | Local workspace for AI coding CLIs, including Hermes, with MCP, CLI and TUI or web surfaces. | TypeScript | 109 | 2026-10-21 | Multi-CLI workspace that includes Hermes. | R |
| [cogwheel0/conduit](https://github.com/cogwheel0/conduit) | Native iOS and Android client for Open WebUI, OpenAI-compatible servers, Ollama and Hermes. | Dart | 2,356 | 2026-10-18 | Mobile client that includes Hermes. | D |
| [p697/clawket](https://github.com/p697/clawket) | Open-source iOS and Android companion for OpenClaw and Hermes Agent. | TypeScript | 349 | 2026-10-16 | Mobile companion for two agents. | R |
| [qingchencloud/clawpanel](https://github.com/qingchencloud/clawpanel) | Multi-engine management panel for OpenClaw and Hermes, as a Tauri v2 desktop app. | JavaScript | 2,936 | 2026-10-20 | Desktop management panel that includes Hermes. | D |
| [itq5/OpenClaw-Admin](https://github.com/itq5/OpenClaw-Admin) | Vue web admin for OpenClaw and Hermes gateways. | Vue | 868 | 2026-04-27 | Web admin that includes Hermes. | D |
| [SebastianElvis/clawpier](https://github.com/SebastianElvis/clawpier) | Desktop app for managing sandboxed OpenClaw and Hermes instances through Docker. | TypeScript | 70 | 2026-04-10 | Desktop manager that includes Hermes. | D |
| [basionwang-bot/HermesPet](https://github.com/basionwang-bot/HermesPet) | macOS desktop companion that runs seven kinds of AI terminals in one observable control surface. | TypeScript | 597 | 2026-10-15 | Desktop companion that includes Hermes. | R |
| [rainnoon/oc-claw](https://github.com/rainnoon/oc-claw) | Desktop pet that monitors coding agents, including Hermes Agent on macOS. | Rust | 360 | 2026-06-17 | Status monitor. | R |
| [VasiHemanth/tokentelemetry](https://github.com/VasiHemanth/tokentelemetry) | Local observability dashboard for coding agents, including Hermes (28 README mentions). | Python | 383 | 2026-10-20 | Telemetry dashboard. | R |
| [Socialpranker/agentburn](https://github.com/Socialpranker/agentburn) | Local spend profiler for Claude Code, OpenClaw and Hermes. | Python | 113 | 2026-10-19 | Cost analysis. | D |
| [ydyangdan/UniPet](https://github.com/ydyangdan/UniPet) | Desktop pet that shows status for AI coding agents, including Hermes. | JavaScript | 18 | 2026-06-10 | Status layer. | D |

## Plugin catalog entries (UI, dashboards, orchestration, providers)

39 rows. The catalog column says where each entry came from: LOCAL means it is in the installed catalog; UPSTREAM means it exists only in the current upstream catalog (74 such entries in total; I listed the ones relevant here). Plugin stars come from their own repositories.

| Plugin (catalog id) | Repository | What it does (catalog text, summarized) | Relevance to a Claude Code-style terminal client | Catalog source | Stars | Last push | Language |
|---|---|---|---|---|---:|---|---|
| hermes-workflows | [jacobhausler/hermes-workflows](https://github.com/jacobhausler/hermes-workflows) | Agent-owned workflow graphs. A background runner executes a JSON graph of agent nodes with fan-outs and human or machine gates. | Workflow runner inside Hermes. Closest to the 'ultracode' workflows the operator described. | LOCAL | 1 | 2026-10-22 | Python |
| agora | [yzy806806/agora](https://github.com/yzy806806/agora) | Self-driving multi-agent development team. Role-based workers (architect, developer, reviewer, tester) are each a real Hermes profile. | Role-based team of Hermes profiles. | LOCAL | 23 | 2026-10-12 | Python |
| crew | [macd2/Crew](https://github.com/macd2/Crew) | One /crew message becomes verified work on a kanban board. A coordinator owns each card until its proof command exits 0. | Verification-gated Kanban in chat. | LOCAL | 3 | 2026-10-19 | Python |
| agent-batch | [raaaas/hermes-agent-batch](https://github.com/raaaas/hermes-agent-batch) | Parallel AI task orchestrator. A task list pasted into the desktop panel is phased into dependency-ordered groups. | Batch fan-out inside the desktop app. | LOCAL | 12 | 2026-10-08 | Python |
| storm-fusion-research-pipeline | [bmtrnavsky/storm-fusion-research-pipeline](https://github.com/bmtrnavsky/storm-fusion-research-pipeline) | Human-gated research with several perspective seats and one judge. | Structured multi-agent review. | LOCAL | 5 | 2026-10-12 | Python |
| nerve | [keeltrace/hermes-nerve](https://github.com/keeltrace/hermes-nerve) | Supervisory layer for Hermes with evidence-backed Kanban supervision and context governance. | Supervisor for background work. | LOCAL | 44 | 2026-10-05 | Python |
| protean-subagent-viewer | [aska-digital/protean-subagent-viewer](https://github.com/aska-digital/protean-subagent-viewer) | Read-only desktop pane for live and past delegated subagent workers. | Observes subagents. Read-only. | LOCAL | 0 | 2026-10-13 | JavaScript |
| protean-model-sync | [aska-digital/protean-model-sync](https://github.com/aska-digital/protean-model-sync) | Fleet-wide provider and model presets with CLI and desktop support. | Model presets across agents. | LOCAL | 1 | 2026-10-13 | Python |
| iowap-hermes-integration | [iowap-org/iowap-hermes-integration](https://github.com/iowap-org/iowap-hermes-integration) | Fleet control for the desktop: status chip, fleet pane and fleet page. | Fleet control UI. | LOCAL | 0 | 2026-10-15 | Python |
| operations-hermes-desktop-plugin | [c-pompa/operations-hermes-desktop-plugin](https://github.com/c-pompa/operations-hermes-desktop-plugin) | Desktop page for running a fleet: topology, model traffic, handoffs and traces. | Fleet operations page. | LOCAL | 1 | 2026-10-21 | Python |
| hicortex-hermes-plugin | [gamaze-labs/hicortex-hermes-plugin](https://github.com/gamaze-labs/hicortex-hermes-plugin) | Shared fleet memory: each connected agent learns from the others. | Shared memory across agents. | LOCAL | 1 | 2026-10-05 | Python |
| hermes-fleet-connector | [tony-simons-aiowa/hermes-fleet-connector](https://github.com/tony-simons-aiowa/hermes-fleet-connector) | Owner-approved phone pairing and read-only live operations reporting. | Phone view of fleet state. | LOCAL | 0 | 2026-10-17 | Python |
| myswarm | [tomkubik/hermes-myswarm](https://github.com/tomkubik/hermes-myswarm) | Discovers, visualizes, sanitizes, exports and imports community swarm configurations. | Swarm config sharing. | LOCAL | 0 | 2026-10-18 | Python |
| hermes-kanban-gantt | [e-is/hermes-kanban-gantt](https://github.com/e-is/hermes-kanban-gantt) | Gantt timeline view of the Hermes Kanban boards on a desktop page. | Kanban timeline view. | LOCAL | 10 | 2026-10-19 | JavaScript |
| hermes-kanban-graph | [Senci/hermes-kanban-graph](https://github.com/Senci/hermes-kanban-graph) | Live read-only dependency graph of the Kanban boards in Hermes Desktop. | Kanban dependency view. | LOCAL | 2 | 2026-10-20 | JavaScript |
| hermes-kanban-live-gantt | [ningfangbin/hermes-kanban-live-gantt](https://github.com/ningfangbin/hermes-kanban-live-gantt) | Live, now-centered Gantt timeline for the Kanban boards. | Kanban timeline view. | LOCAL | 1 | 2026-09-30 | JavaScript |
| hermes-monitor | [mr-3mm3/hermes-monitor](https://github.com/mr-3mm3/hermes-monitor) | Kanban worker health in the desktop status bar, with stall and loop detection. | Worker health for background jobs. | LOCAL | 1 | 2026-10-18 | Python |
| hermes-worker-monitor | [djmango/hermes-worker-monitor](https://github.com/djmango/hermes-worker-monitor) | Kanban board groups and worker health in the desktop footer. | Worker health for background jobs. | LOCAL | 0 | 2026-10-08 | Python |
| hermes-quest | [ap08960695/hermes-quest](https://github.com/ap08960695/hermes-quest) | Read-only pixel-art view of Kanban and worker activity, with a web dashboard tab. | Visual status view. | LOCAL | 1 | 2026-10-18 | JavaScript |
| prompt-queue | [vectorforge22/prompt-queue](https://github.com/vectorforge22/prompt-queue) | Kanban-style prompt queue that plays cards one at a time into new or existing sessions. | Queued prompts for several sessions. | LOCAL | 1 | 2026-10-18 | JavaScript |
| tailgate | [c0mrade/tailgate](https://github.com/c0mrade/tailgate) | Per-job progress updates in chat for jobs Hermes hands to other machines. | Progress tracking for remote jobs. | LOCAL | 0 | 2026-09-19 | Python |
| session-mirror | [rrrrnmtsu/hermes-session-mirror](https://github.com/rrrrnmtsu/hermes-session-mirror) | Mirrors Desktop, CLI, TUI and ACP conversations into Discord. | Cross-surface mirror. | LOCAL | 0 | 2026-10-14 | Python |
| hermes-workbench | [sxuff/hermes-workbench](https://github.com/sxuff/hermes-workbench) | The Hermes desktop app served in a browser through the dashboard. | Browser delivery of the desktop UI. | LOCAL | 11 | 2026-09-24 | TypeScript |
| hermes-mobile | [geniok1980/hermes-mobile-adapter](https://github.com/geniok1980/hermes-mobile-adapter) | WebSocket platform for your own mobile, desktop or web app that talks to the agent directly. | Protocol for building your own client. | LOCAL | 4 | 2026-10-18 | Python |
| hermes-t3-control | [thetedotgg/hermes-t3-control](https://github.com/thetedotgg/hermes-t3-control) | Bounded tools to list, read, create and control local T3 coding threads. | Control of another coding app. | LOCAL | 1 | 2026-09-13 | Python |
| devin | [Adolanium/hermes-devin](https://github.com/Adolanium/hermes-devin) | Delegates coding tasks to the Devin CLI, locally or in Devin Cloud, with progress cards in Hermes Desktop. | Delegation to another coding agent. | LOCAL | 1 | 2026-10-17 | Python |
| junie-acp | [JetBrains/junie](https://github.com/JetBrains/junie) | JetBrains Junie coding agent as a Hermes model provider over ACP. | Provider bridge. | LOCAL | 493 | 2026-10-21 | Shell |
| kimchi-acp-provider | [getkimchi/kimchi-hermes](https://github.com/getkimchi/kimchi-hermes) | The Kimchi coding harness as a Hermes provider over ACP stdio. | Provider bridge. | LOCAL | 1 | 2026-10-11 | Python |
| grok-acp | [nicolasdmolina/hermes-grok-acp](https://github.com/nicolasdmolina/hermes-grok-acp) | xAI Grok subscription as a Hermes model provider, driven over ACP. | Provider bridge. | LOCAL | 0 | 2026-09-24 | Python |
| kiro-acp | [tobenwarrior/hermes-kiro-provider](https://github.com/tobenwarrior/hermes-kiro-provider) | Kiro CLI (AWS) as a Hermes model provider over ACP. | Provider bridge. | LOCAL | 0 | 2026-09-17 | Python |
| cursor-acp | [neerazz/hermes-cursor-acp](https://github.com/neerazz/hermes-cursor-acp) | Cursor subscription models inside Hermes through the Cursor CLI's ACP server. | Provider bridge. | UPSTREAM | 1 | 2026-10-20 | Python |
| agent-station | [thealps01-netizen/agent-station](https://github.com/thealps01-netizen/agent-station) | Read-only Hermes Desktop page for the focused chat: main agent and subagents, background commands and a replay of file edits. | Subagent observation view. | UPSTREAM | 4 | 2026-10-18 | JavaScript |
| orquehelx | [TicoraX/OrqueHelx](https://github.com/TicoraX/OrqueHelx) | Per-call delegation: the model sends a subagent to a named route, and each route pins a provider. | Per-task model routing for subagents. | UPSTREAM | 0 | 2026-10-19 | Python |
| hermes-bot-hq | [RohanRJ389/hermes-bot-hq](https://github.com/RohanRJ389/hermes-bot-hq) | Desktop fleet page with a per-bot home for status, metrics, alerts, tasks and routines. | Fleet view in the desktop app. | UPSTREAM | 19 | 2026-10-21 | JavaScript |
| hermes-kanban-stream | [mbgulden/hermes-kanban-stream](https://github.com/mbgulden/hermes-kanban-stream) | Streams Kanban worker activity to a Telegram ops channel, one edit-in-place message per task. | Remote status stream. | UPSTREAM | 0 | 2026-10-20 | Python |
| git-tasks | [meviusisback/git-tasks](https://github.com/meviusisback/git-tasks) | Multi-repo GitHub issues and pull requests in a Tasks view for Hermes Desktop. | Task view in the desktop app. | UPSTREAM | 0 | 2026-10-18 | JavaScript |
| hermes-board | [skk2010/hermes-board-plugin](https://github.com/skk2010/hermes-board-plugin) | Miro-like board that the agent draws with its own tools and the user edits in a desktop pane. | Shared diagram surface. | UPSTREAM | 1 | 2026-10-20 | Python |
| model-picker | [aean0x/hermes-model-picker](https://github.com/aean0x/hermes-model-picker) | Per-turn cost classifier that rewrites the model for each request across three tiers. | Per-turn model routing. | UPSTREAM | 0 | 2026-10-20 | Python |
| mcp-serve (official) | [NousResearch/hermes-official-plugins](https://github.com/NousResearch/hermes-official-plugins) | Runs Hermes as an MCP server so other MCP clients can list conversations and poll events. | First-party route for other clients to read Hermes. | UPSTREAM | 363 | 2026-10-22 | Python |

## Excluded: name collisions, not Hermes tools, or claims not supported by the repository

Listed so a reader can see what the searches returned and why it was dropped.

| Repository | Why it is listed | Basis |
|---|---|---|
| [facebook/hermes](https://github.com/facebook/hermes) | JavaScript engine for React Native. Not Hermes Agent. | D |
| [reactnativecn/react-native-update](https://github.com/reactnativecn/react-native-update) | Over-the-air updates for React Native that mention the Hermes JavaScript engine. | D |
| [P1sec/hermes-dec](https://github.com/P1sec/hermes-dec) | Reverse-engineering tool for React Native Hermes bytecode. | D |
| [HermesApp/Hermes](https://github.com/HermesApp/Hermes) | macOS Pandora client (2020). Name collision. | D |
| [Xiaofei-it/Hermes](https://github.com/Xiaofei-it/Hermes) | Android inter-process communication framework. Name collision. | D |
| [Barkhausen-Institut/hermespy](https://github.com/Barkhausen-Institut/hermespy) | Radio simulator. Name collision. | D |
| [codyd51/Hermes](https://github.com/codyd51/Hermes) | iOS tweak. Name collision. | D |
| [MythicAgents/hermes](https://github.com/MythicAgents/hermes) | Swift macOS agent. The description does not mention Hermes Agent. | D |
| [Skeletal-Group/Hermes](https://github.com/Skeletal-Group/Hermes) | Timing-channel research code. Name collision. | D |
| [LZ9/Hermes](https://github.com/LZ9/Hermes) | Android client for the MQTT (a messaging protocol) messaging protocol and WebSocket. Name collision. | D |
| [EggUncle/Hermes](https://github.com/EggUncle/Hermes) | Android Binder data printing. Name collision. | D |
| [xaelophone/hermes](https://github.com/xaelophone/hermes) | Markdown editor. Name collision. | D |
| [Ap0ll02/hermes](https://github.com/Ap0ll02/hermes) | Pacman TUI wrapper. Name collision. | D |
| [Mario-SO/hermes](https://github.com/Mario-SO/hermes) | Peer-to-peer file-sharing TUI. Name collision. | D |
| [PranithMahanti/Hermes-TUI](https://github.com/PranithMahanti/Hermes-TUI) | Terminal chat messenger for a university network, per its README (last push 2025-05-22). Not Hermes Agent. | R |
| [papysans/HERMES](https://github.com/papysans/HERMES) | Telegram remote control for the OpenCode TUI. Name reuse. | D |
| [1ilkhamov/opencode-hermes-multiagent](https://github.com/1ilkhamov/opencode-hermes-multiagent) | 17-agent system built for OpenCode and named Hermes, per its README. Not a Hermes Agent tool. | R |
| [morriszdweck/mosaic](https://github.com/morriszdweck/mosaic) | Terminal agent described as Hermes-style and based on OpenCode. Not built for Hermes Agent. | D |
| [3495857712-lang/Hermes--Tui](https://github.com/3495857712-lang/Hermes--Tui) | README says it is 'inspired by' Hermes Agent's self-evolution loop; it does not say it runs on Hermes. | R |
| [joelachance/hypervisor-archive](https://github.com/joelachance/hypervisor-archive) | Description mentions Hermes; README has zero Hermes mentions. | R |
| [builderz-labs/mission-control](https://github.com/builderz-labs/mission-control) | Listed as a Hermes orchestrator in a curated list; README has zero Hermes mentions. | R |
| [ruvnet/ruflo](https://github.com/ruvnet/ruflo) | Surfaced by search; README has zero Hermes mentions. | R |
| [dyedd/lens](https://github.com/dyedd/lens) | Description says it supports Hermes Agent; README has zero Hermes mentions. | R |
| [wk42worldworld/cybercode](https://github.com/wk42worldworld/cybercode) | Description claims Hermes self-evolution; README has zero Hermes mentions. | R |
| [swarmclawai/swarmclaw](https://github.com/swarmclawai/swarmclaw) | Hermes appears only as a provider logo in the README. | R |
| [kerpopule/hermes-jev-skills](https://github.com/kerpopule/hermes-jev-skills) | Skill pack for model routing and memory. A skill pack, not a client. | R |
| [Tommy-yw/RunbookHermes](https://github.com/Tommy-yw/RunbookHermes) | Hermes-native operations agent for incident response. A domain application, not a client. | D |
| [UfukNode/Noustiny](https://github.com/UfukNode/Noustiny) | Video-creation pipeline on top of Hermes. A domain application. | D |
| [Zyrexnn/Cybermes](https://github.com/Zyrexnn/Cybermes) | Security agent framework powered by Hermes. A domain application. | D |
| [bigph00t/hermescraft](https://github.com/bigph00t/hermescraft) | Minecraft companion built on Hermes. A domain application. | D |
| [Infisical/agent-vault](https://github.com/Infisical/agent-vault) | Credential proxy that lists Hermes among agents. Not a client. | D |
| [NVIDIA/NemoClaw](https://github.com/NVIDIA/NemoClaw) | Sandbox runner for agents including Hermes. Not a client. | D |
| [garrytan/gbrain](https://github.com/garrytan/gbrain) | Memory layer for OpenClaw and Hermes. Not a client. | D |
| [thedotmack/claude-mem](https://github.com/thedotmack/claude-mem) | Persistent memory for many agents. Not Hermes-specific. | D |
| [kreem007/-kreem07](https://github.com/kreem007/-kreem07) | Unrelated repository with a non-project description. Not inspected further. | D |
| [nshkrdotcom/HackTUI-Hermes-Jido](https://github.com/nshkrdotcom/HackTUI-Hermes-Jido) | Elixir security operations platform. Relation to Hermes Agent not verified. | D |
| [Mybiznes754/HackTUI-Hermes-Jido](https://github.com/Mybiznes754/HackTUI-Hermes-Jido) | Elixir security operations platform. Relation to Hermes Agent not verified. | D |

## Unverified: plausible Hermes clients I could not confirm

| Repository | Why it is listed | Basis |
|---|---|---|
| [gordiontenor/HermesMobile30.07](https://github.com/gordiontenor/HermesMobile30.07) | Described only as 'Hermes Agent Client'. The README has zero Hermes mentions. | D |
| [Felix-Forever/hermes-agent-desktop](https://github.com/Felix-Forever/hermes-agent-desktop) | Claims a ground-up rebuild of the Hermes experience with a multi-agent system. The README shows no link to a Hermes gateway or CLI, so it is unclear whether it uses Hermes at all. | R |
| [pnganga813-commits/Aether-desktop-orchestrator](https://github.com/pnganga813-commits/Aether-desktop-orchestrator) | Marketing-style README. The features it lists (OpenAI and Claude API keys) show no Hermes integration. | R |
| [Etschmia/hermes-gateway-client](https://github.com/Etschmia/hermes-gateway-client) | TypeScript gateway client with no description. Not inspected. | D |
| [tonbistudio/hermes-desktop-plugins](https://github.com/tonbistudio/hermes-desktop-plugins) | No description. Not inspected. | D |
| [dev-toolings/tui-hermes-console](https://github.com/dev-toolings/tui-hermes-console) | No description and no README. | D |
| [itsPremkumar/hermes-cli-tui](https://github.com/itsPremkumar/hermes-cli-tui) | No README (listed in the TUI table as well). | D |
| [areu01or00/Hermes-Agent-Mobile-Client](https://github.com/areu01or00/Hermes-Agent-Mobile-Client) | No description. Not inspected. | D |
| [amirghm/hermes-agent-mobile](https://github.com/amirghm/hermes-agent-mobile) | Install script for mobile. Not inspected. | D |
| [achappell/homebrew-hermes-relay](https://github.com/achappell/homebrew-hermes-relay) | Homebrew tap for a 'Hermes Streaming TUI'. Not inspected. | D |
| [HenWorks/Hermes-agent-android-PC-companion-app](https://github.com/HenWorks/Hermes-agent-android-PC-companion-app) | PC-side companion for an Android app, with mesh and handoff features. Not inspected. | D |

## Searches that returned nothing, or nothing relevant

| Search | Result | Tool |
|---|---|---|
| GitHub repository search: 'nous hermes agent cli' | 0 repositories | gh search repos |
| GitHub repository search: 'agent client protocol hermes' | 0 repositories | gh search repos |
| GitHub repository search: 'toad agent client protocol terminal' | 0 repositories (Toad itself was found by its repository name) | gh search repos |
| GitHub repository search: 'claude-for-hermes' | No repository with that name; only unrelated fuzzy matches | gh search repos |
| Local search for hermes-workspace or outsourc-e in docs, catalog and optional skills | 0 files | grep |
| Local search for nesquena or hermes-webui in docs, catalog and optional skills | 0 files | grep |
| Local search for fathah or hermes-desktop-companion in docs, catalog and optional skills | 0 files | grep |
| Local search for Ekko or EKKOLearn in docs, catalog and optional skills | 0 files | grep |
| Local search for Toad or batrachian in the official docs | 0 files in the official docs. One false match, a case-insensitive hit inside the word 'AutoAdd' in a Lambda Labs reference file, is not relevant | grep |
| Web search: 'Hermes Agent Nous Research third-party terminal TUI client open source' | Results returned, but none names a third-party terminal client for Hermes | WebSearch |
| Web search: 'Claude Code for Hermes' or 'Hermes Agent' with Claude Code style terminal client | Results returned, but none describes a Claude Code-style terminal client for Hermes | WebSearch |
| Web search: community desktop app alternative to Hermes Desktop | Results returned, but none names a separate community alternative (they cover the official app or general substitutes) | WebSearch |

Every other repository-search query returned at least one result. The count queries are in counts.tsv and totals_nd.tsv, and the repository results are in the raw JSON files named under Files.

## Coverage limits

- **Not exhaustive.** GitHub keyword search matches the word 'Hermes' in many unrelated projects, such as the React Native Hermes engine. The name-and-description pool has 2,680 repositories whose name or description contains 'hermes', and the match counts run into the hundreds or thousands for the broad queries. I could not enumerate every match.
- **README-only mentions are not enumerated.** A repository can mention Hermes only in its README. The phrase search for 'Hermes TUI' across names, descriptions and READMEs matched 442 repositories; I did not enumerate them.
- **Each query returns at most 100 results.** The name-and-description searches kept the top 100 by stars; the first keyword searches used GitHub's default order. Projects beyond those 100 were not seen.
- **Descriptions are not behavior.** For the rows marked D, I know what the repository says it does, not whether the code works. I did not run any third-party code.
- **The local catalog is stale.** The installed catalog has 690 entries; upstream main has 763. I checked both and recorded the difference, but I did not review the other upstream changes in the repository.

## Conflicting or stale sources

- **Star counts in curated lists are stale.** The Hermes Atlas (ksimback/hermes-ecosystem) lists outsourc-e/hermes-workspace at 830 stars and fathah/hermes-desktop at 65. The GitHub API on 2026-10-22 reports 6,738 and 14,448. A web aggregator lists nesquena/hermes-webui at about 3.1k stars; the API reports 19,092. I used the API throughout.
- **Name confusion for 'Hermes Desktop'.** One article credits fathah with the app and says it is not an official release. A headline calls the official app 'Official Desktop App'. The installed checkout confirms the first-party app is in apps/desktop. The community app is fathah/hermes-desktop, whose README calls it 'Hermes One'.
- **ACP feature request is stale.** Nous issue NousResearch/hermes-agent#569 (opened 2026-03-07, still open) asks for an ACP server mode for Zed, JetBrains, Neovim and Toad. The installed checkout already has an ACP adapter, and the docs describe VS Code, Zed and JetBrains setups. The issue seems stale or partly done; I did not test the setups.
- **TUI protocol changed.** inaki/talaria's README says it speaks stdio JSON-RPC like hermes --tui. The installed TUI's own design doc (ui-tui/CANONICAL_GATEWAY.md) says the TUI now connects over an authenticated WebSocket with single-use tickets. Third-party TUIs built on the older path may need changes. This is unverified for each project.
- **Language mismatch.** 1deat0r/Hades-Agent is listed as Python by GitHub; its README says Rust. Unresolved.
- **Name collision.** inaki/talaria (Rust TUI for Hermes) and this project are both called Talaria. Any public naming decision should account for it.

## What this means for a Claude Code-style terminal client

These points come from the tables above. They are not a recommendation.

- **Terminal-native prior art is thin.** Among third-party terminal UIs, the most-starred interactive client has 301 stars. The Rust clients have 0 to 1 stars (inaki/talaria, 0xNyk/hermes-tui, Hades-Agent). The Go client has 0 (DevvGwardo/hermes-tui). Excluding monitors, the Python terminal client with the most stars is LehaoLin/hermes-gate (28 stars).
- **The first-party TUI is the reference.** Several third-party clients say they speak the same JSON-RPC protocol family as the first-party TUI (inaki/talaria and 0xNyk/hermes-tui say so). The first-party TUI's connection method changed (see the conflicts section), so any new client has to track it.
- **Hermes-driving-Claude-Code bridges exist, in several forms.** xuyang-liu16/hermes-code-bridge (41 stars), forcewake/hermes-conductor (79), AlexAI-MCP/hermes-CCC (134, Hermes ported into Claude Code), RodrigoSiliunas/hermes-claude-bridge (1), and several ACP bridges with 0 to 2 stars.
- **Terminal-first orchestration is rare.** Most orchestration tools are web dashboards. The terminal-first ones (mux, AGK-TUI, open-tui-orchestrator, hermes-fleet-ctrl, bg-agents-panel) are small.
- **Toad is the one to test first.** It is the largest general terminal UI for ACP agents found, measured by stars. Whether it can launch hermes acp is a test to run, not a fact I have.

## Files

- This report: <scratch>/research/u10-prior-art-hermes-ecosystem.md
- Full list of 2,680 Hermes-named repositories in a tab-separated (TSV) file, with heuristic category labels (labels are my pattern match, not verified): <scratch>/research/hermes_named_repos_2026-10-22.tsv
- Count queries (text, one query per line with its total): <scratch>/research/counts.tsv and <scratch>/research/totals_nd.tsv
- Raw search results (JavaScript Object Notation, JSON): <scratch>/research/union_all.json (merged), and the per-query files in <scratch>/research/nd and <scratch>/research/raw_*.json
- Plugin catalog diff: <scratch>/research/upstream_only.txt (upstream-only) and <scratch>/research/local_only.txt (local-only)
- README files saved for review: <scratch>/research/readmes
- Keyword-filtered catalog descriptions: <scratch>/research/catalog_strict2.txt
- Metadata cache used by this report: <scratch>/research/build/meta_cache.json
