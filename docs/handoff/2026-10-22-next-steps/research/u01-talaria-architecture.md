# u01: An architectural assessment of Talaria's Python package

**What this is.** Talaria is an experimental terminal client for Hermes Agent, written in Python with the Textual terminal framework. It lives at `<talaria>`. This report assesses its Python package (`talaria/`) for a team deciding whether to keep building on it or rewrite it, possibly in Rust. The goal is "Claude Code for Hermes".

**Revisions read.** I read Talaria at commit `cafc640` ("Merge pull request #187", 2026-09-25). After `git fetch`, the local `main` matched `origin/main` exactly (`git rev-parse HEAD origin/main` returned `cafc640fc24f0af34e6741de4d21e4c3dd65845c` twice). I compared it with the installed Hermes at `<hermes>`, revision `22063fc09` (2026-10-17). Unless stated otherwise, every `talaria/...:line` citation is at `cafc640`, and every Hermes citation is at `22063fc09`.

**A correction to the brief.** The brief says Talaria's last commit was 2026-09-16. It was 2026-09-25, a single commit (`git log --format=%ad --date=short | sort | uniq -c` showed one commit that day). The last heavy day was 2026-09-16, with 99 commits. The history runs from 2026-08-01 to 2026-09-25 and holds 791 commits.

**How to read the labels.** **Measured** means I ran a command, and the command is shown. **Source-verified** means I read the code but did not run it against a live gateway. **Judgment** marks a grade or an opinion. **Hypothesis** marks anything I could not verify.

---

## 0. The short answer

Treat Talaria as a **specification and a parts bin for a rewrite, not as a foundation to evolve.** The reasons, in order of weight:

1. **Its protocol layer is out of date with Hermes, and the most important path is affected.** In Hermes, approvals, clarifications, and sudo and secret prompts now travel as server-to-client JSON-RPC requests. A client must first announce that it can answer them, with `client.capabilities {server_requests: true}`. Talaria never sends that announcement; a grep for `client.capabilities` in `talaria/` returns nothing. It handles only the older `approval.request` event and `*.respond` method pair. At the installed Hermes, an approval sent to a client that has not announced the capability is **withdrawn**, with the message "the attached client cannot answer approval requests (update the Hermes app)" (`tui_gateway/server.py:835-866`). This is source-verified, not exercised live. Hermes has also gained a typed contract registry (`tui_gateway/contracts/`), a `gateway.capabilities` method (`tui_gateway/methods_voice.py:442`), and an authority-backed canonical protocol called "hermes-gateway-v1" (`tui_gateway/contracts/canonical.py:1-8`). Talaria's whole compatibility system was built on the premise that "the terminal gateway publishes no capability endpoint" (`talaria/domain/compat.py:3`). That premise is no longer true.
2. **The user interface is a single 8,486-line controller class.** `TalariaApp` (`talaria/ui/app.py:1384-8486`) has 226 methods and a constructor with roughly 37 lines of parameters. It imports and drives the transport, authentication, and settings HTTP clients directly. The framework-independent domain core is real and enforced, but nothing sits between it and Textual. Every feature has therefore landed in `app.py`, which has been changed in 85 commits, more than any other file.
3. **About two thirds of the package is comments and docstrings, and most of the repository is process.** Only 32,455 of the package's 61,003 lines (53%) are code. Docstrings account for 13,914 lines and comments for 7,081 (measured). In-code references to plan codes are dense: 623 references to KTD (Key Technical Decision) numbers, 617 to R (requirement) numbers, 380 to U (implementation unit) numbers, and 110 to AE (acceptance example) numbers. The tracked `docs/` tree holds 2,406 files and 191 MB, of which 195 MB on disk is acceptance evidence. There are 15,653 lines of acceptance scripts and 7,281 lines of tests that check documents.
4. **The parts are good.** The things worth keeping are ideas, file formats, and test corpora, not modules: the pure reducer, record and replay, redaction rules, bounded rendering ceilings, honest "unknown outcome" semantics for remote calls, and the startup probe that never mutates anything. All of them carry over into any language.

---

## 1. Module map

Sizes are total lines, then **code-only lines**, meaning lines that are not comments, docstrings, or blank. I measured them with a tokenizer and AST script (`scratchpad/research/tools/loc.py`, run on `talaria/` at `cafc640`).

| Unit | Total | Code only | Code % | Purpose |
|---|---:|---:|---:|---|
| `talaria/domain/` (23 files) | 16,273 | 7,570 | 47% | Framework-free core: wire decode, the reducer, the fleet registry, the needs-you queue, projections, commands, settings contracts, the compatibility baseline, redaction policy |
| `talaria/ui/` (26 files) | 23,959 | 13,535 | 56% | Textual presentation: `TalariaApp` plus about 25 widgets and screens |
| `talaria/transport/` (13 files) | 8,651 | 4,465 | 52% | WebSocket dial, the JSON-RPC correlator, the frame-source seam, the multi-gateway set, credentials and authentication, admin and settings HTTP clients, the compatibility probe |
| `talaria/replay/` (6 files) | 4,614 | 2,197 | 48% | Replay frame source and controls, **plus the framework validation gate harness** (`gate.py` 2,526, `stress.py` 707, `workloads.py` 614) |
| `talaria/recorder/` (5 files) | 1,343 | 573 | 43% | Redaction boundary, frame-log v1/v2 writer and reader, the `talaria record` command |
| `talaria/themes/` (6 files) | 1,756 | 1,380 | 79% | Framework-free theme tokens, built-ins, storage, host palette, marketplace |
| `talaria/status/` (4 files) | 1,181 | 575 | 49% | The external status-bar script runner and its contract |
| `talaria/config.py` | 1,862 | 1,360 | 73% | Settings precedence chain, plus byte-preserving TOML writers (60 top-level defs and classes) |
| `talaria/cli.py` | 1,251 | 750 | 60% | Argument parsing (`record`, `replay`, `refresh-credential`, `theme import/search/fetch`, `gate`), and `build_live_app` (`cli.py:739`) |
| `talaria/text.py` | 110 | 49 | 45% | Shared text helpers |
| **Total** | **61,003** | **32,455** | **53%** | |

The two largest files on their own (measured): `ui/app.py` has 8,486 lines and 4,038 code lines (48%). `domain/state.py` has 4,795 lines and 1,898 code lines (40%), because 1,689 of its lines are docstrings.

**Dependencies** (`pyproject.toml`): `textual>=8.2.8,<9`, `websockets>=13,<16`, `pygments`. Three runtime dependencies is a genuinely lean set. The environment runs Textual 8.2.8 on Python 3.12.11 (measured with `.venv/bin/python -c "import textual,sys; ..."`).

**Package boundaries.** No module outside `ui/` and `replay/` imports `textual` or `rich`. I checked with `grep -rnE '^\s*(from|import) (textual|rich)' talaria`. The only hits outside `ui/` are `replay/gate.py:51-52,507,1971` and `replay/workloads.py:94`, and `gate.py` reaches into a **private** Textual module, `textual.widgets._markdown`. The boundary tests pass (measured: `pytest tests/test_framework_boundaries.py tests/domain/test_boundary.py` gave 7 passed in 0.69 s). The boundary runs only one way, however: `ui/app.py` imports from 11 transport modules, including `SettingsClient`, `GatedAuthSession`, `write_connection_tokens`, and `probe_seams`. So the UI does own I/O orchestration, despite AGENTS.md's rule that "the UI ... never holds protocol or session state".

### Key types

- **Transport:** `FrameRecord` and the `FrameSource` protocol (`transport/source.py:63,80`), `LiveSource` (`:266`), `RpcCorrelator` and `RpcOutcome` (`transport/rpc.py:177,106`), `AttachTarget`, `AttachSuccess`, `AttachFailure` (`transport/attach.py:282-371`), `ConnectionSet` and `TaggedFrame` (`transport/connection_set.py`), `LoopbackTokenProvider` (`transport/credentials.py:324`), and `GatedAuthSession` (`transport/gated_auth.py`).
- **Domain:** `GatewayEvent`, `ProtocolErrorFrame`, `UnknownEventFrame`, `NonEventFrame` (`domain/decode.py:177-220`), `SessionState` (`domain/state.py:131`, about 39 annotated fields), `FleetState` (`domain/state.py:3074`), `RegistryRow` (`domain/registry.py`, a fixed-size per-session summary), `NeedsYouQueue` (`domain/queue.py`), `Snapshot` and `TranscriptView` (`domain/projection.py:457,70`), `CommandEntry` (`domain/commands.py`), `ConfigTarget` and `SettingsWorkspaceView` (`domain/settings.py:117,349`), and `MethodBaseline` (`domain/compat.py`).

### Data flow: from a gateway frame to pixels (source-verified)

1. **Dial.** `cli.build_live_app` (`cli.py:739`) builds a `LiveSource` (`cli.py:891`), or a `ConnectionSet` for several gateways (`cli.py:957`). The source dials `ws://127.0.0.1:9119/api/ws?token=…` through `transport.attach.attach` (`attach.py:453`) using `websockets_dialer` (`:132`).
2. **Receive, record, correlate.** `LiveSource._read_loop` (`source.py:790`) awaits `connection.recv()` (`:801`) and hands the raw text to `_ingest` (`:813`). `_ingest` first writes the raw frame through the recorder (`_record`, `:818`, which applies redaction inside `recorder/framelog.py`). It then runs `json.loads` (`:823`), lets the correlator resolve any JSON-RPC reply against its pending future (`self.correlator.resolve(...)`, `:849`), wraps the frame in a `FrameRecord` (`:851`), and enqueues it with backpressure (`_enqueue`, `:861`).
3. **Pump.** `TalariaApp._pump` (`app.py:2238`) runs `async for` over the source. It calls `ingest(record, profile=…, epoch=…)` for tagged multi-gateway frames, or `ingest(record)` otherwise.
4. **Decode and normalize.** `TalariaApp.ingest` (`app.py:2347`) calls `normalize_frame` (`domain/normalize.py:104`), a pass-through to `decode_frame` (`domain/decode.py:254`). `decode_frame` sorts each frame into one of four kinds: `GatewayEvent`, a typed but unknown event, a protocol error that withholds the content, or `NonEventFrame` (a request or a response).
5. **Route and reduce.** `route_frame` (`domain/state.py:3603`) sends frames for the focused session to `apply_frame` (`state.py:1718`, called at `:3648/3656/3666`). It sends frames for other sessions to fixed-size registry rows (`_apply_event_to_row`, `:3682`). `apply_frame` dispatches through the `_HANDLERS` table (`state.py:2975-3012`) to about 40 pure `_on_*` functions, and each returns a new immutable `SessionState`. The app then sets `self._dirty = True`.
6. **Coalesced projection.** A 50 ms timer (`COALESCE_INTERVAL`, `app.py:338`) runs `_render_tick` (`app.py:2457`), then `render_snapshot`, then `_render_snapshot_locked` (`app.py:2518`). That calls `project(...)` (`domain/projection.py:794`). `project` rebuilds the transcript, sub-agent, prompt, attachment, status, and Mixture of Agents views, then marks each region "changed" by comparing it for equality with the previous `Snapshot`.
7. **Widgets.** Only the regions that changed are applied: `TranscriptPane.apply` (`ui/transcript.py:1027`, called at `app.py:2555`), `agents.apply` (`ui/agents.py:189`, `app.py:2559`), `prompts.apply` (`ui/prompts.py:1157`, `app.py:2567`), and `inspector.apply` (`ui/inspector.py:527`, `app.py:2591`). The transcript pane mounts one `TranscriptLine` widget per line for most entry kinds, and one Textual `Markdown` document per assistant or reasoning entry (`ui/transcript.py:1-13`).

**Outbound calls** go through `LiveSource.call` (`source.py:749`), which wraps `RpcCorrelator` and is used through a `LiveDispatcher` protocol (`app.py:1026`). There are 16 `dispatcher.call(` sites in `app.py`.

**Measured cost of the domain half.** The domain half is cheap. I ran a synthetic benchmark (`scratchpad/research/tools/bench_projection.py`) that builds a `SessionState` of N committed assistant entries, each with 21 lines and 40 streamed deltas:

```
entries=  100 lines=  2100 project()=   0.19 ms  mean apply_frame=  14.1 us
entries=  500 lines= 10500 project()=   0.87 ms  mean apply_frame=  13.4 us
entries= 1000 lines= 21000 project()=   1.84 ms  mean apply_frame=  13.5 us
entries= 2000 lines= 42000 project()=   3.58 ms  mean apply_frame=  13.9 us
```

`project()` does grow linearly, because it re-flattens the whole transcript every dirty tick (`projection.py:476-526`), and `SessionState.transcript` has no upper bound. Still, at 42,000 lines it costs 3.6 ms against a 50 ms tick. **Whatever makes Talaria feel slow is therefore not in the domain reducer or the projection.** The project's own recorded measurements point at the Textual widget layer. The confirming gate run for ADR-0006 (architecture decision record 6, on bounded block rendering) recorded a steady-state p99 apply latency of **44.0 ms** for a streaming one-column table and **19.5 ms** for a growing code fence, against a 50 ms ceiling. The first, pre-fix runs recorded 764 ms and 17,697 ms (`docs/analysis/2026-08-09-block-markdown-gate-results.md:34-36`). Those are recorded figures; I did not re-run them. Import cost is moderate: `import talaria.ui.app` took 0.54-0.68 s of wall time across three runs, and `-X importtime` shows about 218 ms cumulative for `talaria.cli` (measured). **Hypothesis, not measured:** the perceived slowness comes from Textual `Markdown` widget mounting and reflow, plus the one-widget-per-line transcript and the 50 ms coalescing cadence. I did not profile a live session.

---

## 2. The Hermes interfaces Talaria uses

### Endpoints

- **WebSocket:** `ws://127.0.0.1:9119/api/ws`, the default (`transport/credentials.py:150`). It authenticates with the query parameter `?token=` on loopback, or with a single-use `?ticket=` minted from `/api/auth/ws-ticket` on a gated (remote) dashboard (`transport/gated_auth.py`). The wire protocol is newline-delimited JSON-RPC (`transport/attach.py:1-8`). Hermes still serves `/api/ws` (`hermes_cli/gateway_client.py:30`, `hermes_cli/shared_session_attach.py:83`).
- **Dashboard HTTP routes**, 70 distinct paths found with `grep -rnoE '"/(api|v1)/[^"]*"' talaria`:
  - Model and profile administration: `/api/model/info`, `/api/model/options`, `/api/model/set`, `/api/profiles` (`transport/admin.py`).
  - Core settings: `/api/config`, `/api/config/schema`, `/api/env`, `/api/env/reveal`, `/api/status`, `/api/gateway/{start,stop,restart}`, `/api/actions/gateway-restart/status`, `/api/rpc` (`transport/settings.py`).
  - Authentication: `/api/auth/providers`, `/api/auth/me`, `/api/auth/ws-ticket`, `/api/health` (`transport/gated_auth.py`).
  - About 50 more routes for skills, the MCP (Model Context Protocol) servers and catalog, cron, pairing, messaging onboarding, local models, provider OAuth and custom endpoints, voice, and tools (`transport/settings_surfaces.py`). **Only 2 of this module's roughly 120 methods are reachable from the app** (`surfaces.reset_profile` at `app.py:6244` and `surfaces.wake` at `:6258`). No file under `tests/` or `scripts/` references `settings_surfaces` (`grep -rln settings_surfaces tests scripts` returned nothing). It arrived in one commit, `4c6805c` (2026-09-16).
  - Talaria does **not** use the Hermes API server's `GET /v1/capabilities`. A grep for `v1/capabilities` found nothing.

### JSON-RPC methods Talaria calls

From the call sites and constants: `session.create`, `session.resume`, `session.most_recent`, `session.list`, `session.active_list` (`app.py:569-723`). `prompt.submit` (`state.py:1068`). `session.interrupt` (`app.py:350`). `subagent.interrupt`, `slash.exec`, `command.dispatch`, `commands.catalog`, `paste.collapse` (`domain/commands.py:114-137`). `approval.respond`, `clarify.respond`, `sudo.respond`, `secret.respond`, `terminal.read.respond` (`ui/prompts.py:125`). `approval.pending` (`app.py:723`). `file.attach`, `image.attach_bytes`, `image.detach` (`transport/attachments.py`). `config.set` (`domain/settings_catalog.py`). The read-only startup probes add `spawn_tree.list`, `agents.list`, and `delegation.status` (`domain/compat.py:120-425`).

### Events Talaria handles

The vocabulary is `KNOWN_EVENT_TYPES` (`domain/decode.py:132`), the union of three provenance-tagged sets (`decode.py:60-135`). Events with real handlers (`state.py:2975-3012`):
- Messages: `message.start/delta/interim/complete`, `thinking.delta`, `reasoning.delta/available`.
- Mixture of Agents: `moa.progress/reference/phase/aggregating`.
- Tools: `tool.start/complete`, with `tool.progress` and `tool.generating` explicitly **ignored**.
- Sub-agents: `subagent.spawn_requested/start/thinking/tool/progress/complete`.
- Prompts: `{approval,clarify,secret,sudo,terminal.read}.request` and the `*.expire` events.
- Session and gateway: `session.info`, `session.usage`, `gateway.ready`, `gateway.protocol_error`, `status.update`.

Fleet routing also uses `sessions.changed` as a poll hint (`decode.py:136-145`). Unknown types are surfaced once per type rather than dropped (`state.py:1773-1793`).

### Authentication: the dashboard session token

Hermes mints `_SESSION_TOKEN` at server start, from `HERMES_DASHBOARD_SESSION_TOKEN` or `secrets.token_urlsafe(32)` (`hermes_cli/web_server.py:233-237`). It injects the token into the single-page app's HTML as `window.__HERMES_SESSION_TOKEN__=` (`hermes_cli/web_server_dashboard.py:139,172`). `talaria refresh-credential` downloads that HTML and **scrapes the token with a regular expression**, `__HERMES_SESSION_TOKEN__\s*=\s*"([^"]+)"` (`transport/refresh.py:73,224`). It then rewrites `~/.talaria/credentials` (mode 0600) in place, without disturbing other bytes (`refresh.py:365-665`). Because the token dies with the dashboard process, this has to be repeated after every dashboard restart (README "Quick start"). Credentials are acquired on every dial, never cached (`credentials.py:1-14`). Gated remote dashboards use PKCE (Proof Key for Code Exchange, an OAuth flow) or a password, then a Bearer token, then a per-dial ticket (`transport/gated_auth.py`). In total, credentials and authentication take about 3,240 lines: `credentials.py` 721, `refresh.py` 690, `gated_auth.py` 533, `attach.py` 488 (largely URL credential scrubbing), plus redaction 813.

### Capability discovery

Talaria keeps a hand-written baseline of each required method's request fixture and response shape, pinned to Hermes **`7f4d15515` (2026-08-01)** (`domain/compat.py:1-30`; 27 citations of that revision across `talaria/`). Each method is classed as read-only, evidence-only, or presence-only. At startup, `transport/compat_check.py` (761 lines) calls only the read-only methods, plus `approval.pending` with no parameters, and compares their shapes. It deliberately never calls a mutating method to learn whether it exists (`compat_check.py:1-12`, `compat.py:430-466`). The revision the brief names, `3236f4405`, appears only inside acceptance-evidence text files under `docs/acceptance/v0.6.{2,3,4}/` (`git grep -l 3236f440`), not in any source file.

**Drift since then** (measured): Hermes is now **36,115 commits** past `7f4d15515` and **21,434 commits** past `3236f4405` (`git rev-list --count`). `tui_gateway/` alone has changed in 106 files (+15,880/−4,373) since `3236f4405`. The changes that matter most for Talaria:

- **Server-to-client requests** (`tui_gateway/server_requests.py`, added in `ebe8cda8ea` on 2026-09-13, so already present at `3236f4405`). Its docstring says: "no paired `*.request` notification / `*.respond` method, no per-kind `*.expire`" (`tui_gateway/server_requests.py:1-35`). Talaria's decoder sorts such a frame (one with a `method` other than `"event"`) as `NonEventFrame(kind="request")` (`decode.py:278-279`), and `apply_frame` discards it (`state.py:1722-1723`). Since `f9d178f78e` (2026-09-16), a client that has not sent `client.capabilities` never receives these requests, and approvals are withdrawn instead (`tui_gateway/server.py:835-866`, `methods_voice.py:450-458`). **Hypothesis, strongly supported by the source:** Talaria's approval, clarify, sudo, and secret prompts do not work against the installed Hermes. They would have stalled against `3236f4405`, and they are withdrawn against `22063fc09`. The v0.6.2 to v0.6.5 acceptance runs were about configuration, so they would not have exercised this path.
- **A typed contract registry** (`tui_gateway/contracts/`, 24 modules of Pydantic models, added in `d4840f9236`, 2026-09-14). `gateway.capabilities` (`methods_voice.py:442`) and `client.capabilities` (`:450`) now exist. A rewrite could generate its client types from these models instead of hand-maintaining a baseline.
- **A "session authority" and canonical protocol** (`tui_gateway/contracts/canonical.py:1-8`, `tui_gateway/ws_legacy_fallback.py:1-30`). Session verbs have moved behind an authority, and legacy handlers are a fallback. I did not determine which Talaria calls still reach legacy handlers; that is unverified and belongs to the protocol unit.

---

## 3. Feature inventory

Line counts are the files listed plus the matching section of `app.py` where one exists. Section boundaries come from the `# ──` headers in `app.py`. Grades are **judgment**: 5 means a clean, minimal design that fits its purpose; 1 means accretion that would be cheaper to rewrite than to fix. "Completeness" describes what the code does, not live behavior unless stated.

| Feature | Where | Lines | Completeness | Grade | Why |
|---|---|---:|---|:-:|---|
| Transcript | `ui/transcript.py`, `ui/blocks.py`, `ui/markdown.py` (kept but "retired from the live rendering path", `markdown.py:3`), `ui/literal.py`, `domain/history.py` | 3,360 | Streams and block-renders Markdown. Tool calls show as **one line each** (`state.py:2218-2263`), with no expandable tool output, and `tool.progress` is ignored. Only about the last 500 lines stay mounted (`DEFAULT_MOUNT_CAP`, `transcript.py:105`). Older lines collapse into "── N earlier lines condensed (still readable by the agent)" (`:112`), and I found no in-app way to read them. | 2 | A large share of the code fights Textual `Markdown` costs: two-tier descendant budgets, fallback banners, ring recycling. The module docstring runs 60+ lines of unit history. |
| Composer | `ui/composer.py`, `domain/composer_history.py`, `ui/focus.py`, app section of 74 lines | ~1,120 | Multi-line input, history, large-paste collapse through `paste.collapse` (`app.py:8339-8427`) | 3 | Reasonable. The focus-ownership rules are spread across modules. |
| Slash palette | `ui/palette.py`, `domain/commands.py` | 2,165 | Gateway catalogue (`commands.catalog`) plus 15 local commands (`commands.py:648-791`), filter-as-you-type, provenance badges | 3 | Works, but the docstring still says "Deliberately not a fuzzy-search overlay" before describing the later filter mode (`palette.py:1-30`): layered amendments. |
| Model and profile pickers, session picker | `ui/picker.py`, `ui/dialog.py`, `domain/selection.py`, `transport/admin.py`, `domain/models_catalog.py`, `domain/session_list.py`, plus app sections (163 model, 92 profile, 626 session, 271 default-model write) | ~4,660 | Model selection writes the profile's default model through `/api/model/set`, with a two-step confirmation. **Unverified:** whether a per-session model switch exists outside the gateway's own `/model` slash command. | 3 | The pure `selection.py` is clean. The default-model write takes 271 lines of the app. |
| Inspector and diff viewer | `ui/inspector.py`, `ui/diff_viewer.py`, `domain/changes.py` | 2,366 | Responsive right-hand panel; read-only unified diffs taken from `tool.complete.inline_diff` (`state.py:2252-2262`) | 3 | Bounded and read-only, which is sensible. There is no apply, revert, or file view. |
| Needs-you queue | `domain/queue.py`, `ui/needs_you.py`, `domain/registry.py`, fleet half of `state.py` (`:3074-4795`) | ~4,050 | Oldest-first queue across every session on every gateway, answered by `/needs` | 3 | Solid domain model (stable item identity, bounded rows). It sits on the superseded prompt protocol (section 2). |
| Approvals and blocking prompts | `ui/prompts.py`, app sections (934 prompts, 127 terminal-read), prompt handlers in `state.py` (`:1176-1717`, `:2281-2560`) | ~2,850 | Approve, deny, deny-all, clarify, sudo, secret, terminal-read, with honest lost-outcome lines | 2 | Careful semantics, but built entirely on `*.request` events and `*.respond` calls, which Hermes has replaced (section 2). |
| Attachments | `transport/attachments.py`, `domain/attachments.py`, `ui/attach.py`, part of an 871-line app section that also holds theme handling | ~900 + app | `/attach` and paste; files and images through `file.attach` and `image.attach_bytes` | 3 | Clean outcome types. Handling is split across four places. |
| Themes, marketplace, import | `themes/*`, `ui/theme.py`, `ui/theme_import.py` | 3,393 | Built-ins, host palette inheritance, VS Code theme import, marketplace search and fetch, reload | 3 | Well separated and framework-free, but 3,400 lines for theming is out of proportion to a coding client's core work. |
| Status-bar scripts | `status/*`, `ui/status_bar.py`, `ui/status_region.py` | 1,996 | Runs a user command as a child process; payload on stdin; environment deny-list; limits; responsive bar | 4 | Clear contract, documented in `docs/formats/status-line.md`. Worth keeping. |
| Settings and configuration workspace | `ui/settings_workspace.py`, `settings_widgets.py`, `settings_overlays.py`, `config_view.py`, `domain/settings*.py`, `transport/settings.py`, `transport/settings_surfaces.py`, app `/config` section of 643 lines | 6,456 | Schema-driven `/config` with target switching, save, discard, one-shot reveal, gateway start/stop. Most of the "Desktop-parity" surface (skills, MCP, cron, OAuth, local models) is **not reachable**; see section 2. | 2 | Driving the editor from `/api/config/schema` is the right idea. Five releases (v0.6.1 to v0.6.5) went to residual fixes in this one area, and 972 lines of `settings_surfaces.py` are untested and unused. |
| Record and replay | `recorder/*`, `replay/source.py`, `replay/controls.py` | 2,110 | `--record`, `talaria record`, `talaria replay` with pause, speed, and visibly disabled controls; frame log v1/v2 specified in `docs/formats/frame-log.md` | 5 | The best part of the codebase. See section 5. |
| Framework validation gate | `replay/gate.py`, `stress.py`, `workloads.py` | 3,847 | Replays synthetic workloads through the real app and enforces widget, height, and latency ceilings | 3 | A valuable instrument, but it ships inside the product package and imports a private Textual module. |
| Multiple gateways at once | `transport/connection_set.py`, `FleetState` | 1,045 + fleet | One `LiveSource` per configured profile or connection; tagged frames; per-connection epochs | 3 | Sound design. Only the **focused** session keeps a transcript; other sessions are fixed-size summary rows (`registry.py:1-30`), and switching focus means a `session.resume`. |
| Mixture of Agents progress | `decode.py:441-528`, `normalize.py:244-385`, `state.py:2810-2961`, `projection.moa_view` | ~450 | Progress, references, phases, aggregation, with a fallback | 3 | Proportionate. |
| Sub-agent rows | `ui/agents.py`, `state.py:2561-2714` | ~385 | Read-only rows (id, name, status), fold toggle, interrupt one sub-agent. Talaria **refuses to author** sub-agent state (`compat.py:466-472`). | 2 | Thin relative to the goal of several agents running in the background. There is no spawn or delegation control and no per-agent transcript. |
| Startup compatibility probe and seam board | `domain/compat.py`, `transport/compat_check.py`, app startup section of 1,028 lines | 1,902 + 1,028 | Probes without side effects; names the missing seams | 2 | A principled idea, but pinned to `7f4d15515` and now superseded by `gateway.capabilities` and the typed contracts. |
| Credentials and authentication | see section 2 | ~3,240 | Loopback token scrape and refresh; gated PKCE, Bearer, and ticket | 3 | Careful, but the regex scrape is fragile by nature, and 3,000+ lines is a lot for a client. |
| Launch working directory | `domain/workdir.py` | 85 | Requests a working directory and reports whether it was adopted | 4 | Small and honest. |

**Missing for "Claude Code for Hermes"** (source-verified absences, from the `_HANDLERS` table, the local commands, and the module list):
- No expandable tool calls or live tool output.
- No `@`-file mentions or file picker.
- No permission or plan modes.
- No reading of the full transcript once it is condensed.
- No spawning of background agents, or any multi-agent workflow authoring.
- No Kanban adapter (the README says so).
- No per-session transcripts for background sessions.

---

## 4. Accidental complexity

All figures are measured with the commands shown, at `cafc640`.

1. **`TalariaApp` is the whole application layer.** It has 8,486 lines, 226 methods, about 95 `self.` attributes assigned in its first 690 lines, 74 module-level constants, and 11 transport imports. It was changed in 85 of 791 commits (`git log --format= --name-only -- talaria | sort | uniq -c`). The biggest sections are the live startup sequence (1,028 lines, `app.py:4731-5759`), blocking prompts (934, `:3259-4193`), attachments plus themes (871, `:6402-7273`), `/config` (643, `:5759-6402`), and the session picker (626, `:7273-7899`). **The cause, as far as the code shows:** ADR-0002 (the framework-independent domain) produced a pure core and a thin widget layer, but no application-service layer for controllers, use cases, and I/O orchestration. Each feature's orchestration therefore went into the only stateful object available. Even the module docstrings say so: "`TalariaApp` — the Textual shell, and the thing the gate measures" (`app.py:1`).
2. **`domain/state.py` holds two reducers in one file.** Its 4,795 lines are only 1,898 code lines. It contains the focused-session reducer (`:131-3073`) and the whole fleet registry and router (`:3074-4795`). In-code amendment narratives such as "U4's amendment to that note… U6's amendment: the move is done" (`state.py:3025-3060`) and "Three earlier drafts of this comment were wrong" (`app.py:2422-2440`) show that the code itself has become the change log.
3. **Comments and docstrings outweigh code in the core.** Package-wide, 34% of lines are comments or docstrings: 7,081 + 13,914 = 20,995 of 61,003. In `domain/`, code is 47%, and in `state.py` it is 40%. Planning codes cited in code: KTD 623, R 617, U 380, AE 110. Citations of Hermes `file:line` locations: 251. Those citations point at a Hermes revision that is now 36,115 commits old.
4. **Process machinery is larger than many features.** Measured:
   - `scripts/acceptance/`: 15,461 lines in 23 Python files. `v050_receipt.py` alone is 5,484 lines.
   - `tests/docs/`: 7,281 lines of tests that check documents. `test_v061_lineage.py` alone is 5,205 lines and 210 KB.
   - `docs/`: 2,406 tracked files and about 191 MB tracked (summed with `git ls-files docs | ... wc -c`). `docs/acceptance/` is 195 MB on disk (`du -sh`), with 2,234 files. v0.5.0 holds 678 JSON, 409 PNG, and 364 ANSI files.
   - Planning prose: 171 Markdown files with 52,569 lines outside `docs/acceptance/`. That breaks down as plans 12,903, the engineering journal 8,465, analysis 6,414, code reviews 3,817, reviews 1,861, work sessions 1,510, and brainstorms 1,243.
   - 289 of 791 commit subjects (37%) mention acceptance, receipt, evidence, verdict, gate, or record (`git log --format=%s | grep -ciE ...`). Docs paths were touched 4,859 times, against 594 touches of `talaria/` paths. Many of the docs touches are bulk evidence additions.
   - The v0.6.1 daily-driver verdict lists 23 rows, each with an identical 40-word status cell (`docs/analysis/2026-09-05-v0-6-1-daily-driver-verdict.md`).
5. **The test suite is large relative to the code.** `tests/` holds 91,227 lines in 153 files, with 2,980 test functions (`grep -rhoE '^\s*(async )?def test_'`). That is 1.50 test lines per package line, or **2.8 test lines per code line**. The largest suites are `tests/ui` (36,214), `tests/domain` (17,309), and `tests/transport` (16,789). Many of these tests pin implementation details. One example is the epoch argument test that `app.py:2409-2421` describes. Heavy pinning of this kind makes refactoring expensive, which is part of why accretion won over restructuring. This is a judgment.
6. **Compatibility probing against a stale pin.** There are 1,902 lines of baseline and probe code, plus a 1,028-line startup sequence, all against `7f4d15515`. The cause is ADR-era reasoning that the gateway "publishes no capability endpoint". That reasoning was true on 2026-08-01 and false by 2026-09-14 (section 2).
7. **Credential handling** totals about 3,240 lines across the scrape, refresh, gated PKCE and ticket flow, URL credential scrubbing, and route-aware HTTP redaction. The cause is Hermes's design: an in-memory token injected into HTML, plus a separate gated mode. The scraping approach is inherently brittle.
8. **Dead or near-dead code.** `transport/settings_surfaces.py` has 972 lines, about 120 methods, 2 reachable from the app, and no tests. `ui/markdown.py` (206 lines) is "retired from the live rendering path" and kept as a fallback (`markdown.py:3-16`). `src/` holds 589 lines of TypeScript that exist only so a Python test can compare against them, and that test requires a Node toolchain in CI (`AGENTS.md`).
9. **Over-investment in peripheral features.** Theming (3,393 lines) and configuration (6,456 lines) together exceed the transcript, composer, and palette combined (6,645 lines).

---

## 5. What is genuinely good, and worth carrying into any rewrite

1. **A framework-independent domain core with a pure reducer.** `apply_frame(state, decoded) -> SessionState` (`domain/state.py:1718`) is a pure function over frozen dataclasses, dispatched through a table (`:2975`). It costs about 14 µs per frame (measured). The boundary is enforced by tests (`tests/domain/test_boundary.py`, `tests/test_framework_boundaries.py`; 7 passed). In Rust, the same idea is an Elm or Redux style `update(state, event)` with a separate `view(state)`.
2. **Record and replay as the primary test seam.** There is one narrow `FrameSource` protocol, an async iterator plus `close()` (`transport/source.py:63-95`). Live and replay share every line above it. The frame log is a JSON Lines format specified independently of any language (`docs/formats/frame-log.md`, 232 lines). It covers versions 1 and 2, with frames tagged by connection for multi-gateway runs (`recorder/framelog.py:13-35`). Replay disables any control that would change something on the gateway (`replay/controls.py`). Recorded corpora exist in `corpora/` (1.4 MB). **Carry over the format and the corpora as-is.** They are a ready-made regression oracle for a rewrite.
3. **Redaction before disk.** A deny-set lists the methods whose responses carry secrets (`recorder/redact.py:192`). Key-name patterns are matched in both squashed and camel-normalized forms. URL query keys `token`, `ticket`, and `internal` are always withheld. Each redaction is recorded rather than silently applied (`recorder/redact.py:1-60`). On top of that sits a route-aware HTTP redaction policy (`domain/redaction.py`).
4. **Honest remote-call outcomes.** If the socket drops mid-call, the result is `unknown`, never success (`transport/rpc.py:1-30`). Replies are epoch-qualified, so a reply from a replaced socket is never applied to the current one. Every dial returns an outcome instead of raising (`transport/attach.py`). Any rewrite should keep these semantics.
5. **Coalesced, change-marked projection.** Snapshots are immutable and name the regions that changed (`domain/projection.py:794-830`), and widgets update only those regions on a fixed cadence (`app.py:2457-2600`). Combine this with a bounded transcript in state, and re-flatten only the tail.
6. **Bounded rendering as stated, measurable ceilings.** ADR-0006 fixes four ceilings: descendant count, rendered height, reconcile work, and apply latency. Each has a measurement point and a fallback per entry. A gate enforces them (`platform-specs/04-architecture/adrs/0006-...md`, `replay/gate.py`). The specific numbers are Textual-specific. The discipline of stating ceilings first and gating on them is not.
7. **A startup probe that never mutates.** Only calls proved to be read-only, or bare presence checks, are allowed on the wire (`domain/compat.py:430-466`). Replace the hand-pinned baseline with Hermes's `gateway.capabilities` and the generated contracts, but keep the rule.
8. **The fleet model.** It uses fixed-size `RegistryRow` summaries with three distinct states (live, stale, never-observed), ages computed from frame time (`domain/registry.py:1-30`), and a needs-you queue with stable item identity (`domain/queue.py`). This is the right abstraction for "several agents working in the background". Extend it with a bounded per-session transcript cache.
9. **The status-line child-process contract** (`status/contract.py`, `docs/formats/status-line.md`): stdin payload, environment deny boundary, limits.
10. **Reconciliation knowledge.** The rules for re-encoding Hermes terminal behavior are written down: `docs/analysis/2026-08-02-hermes-reconciliation-rules.md` (175 lines), `hermes-tui-feature-inventory.md` (199 lines), and `hermes-gateway-protocol-surface.md` (156 lines). Examples in code include never assigning `payload.rendered` because it is incremental (`state.py:1913-1930`), and flushing an abandoned clarify on `tool.complete` (`state.py:2227-2240`). These need re-checking against `22063fc09`, but the method of recording each rule with its source revision is sound.
11. **A lean dependency footprint**: three runtime dependencies, plus explicit sdist allow-listing (`pyproject.toml`).

---

## 6. Verdict (judgment)

**Treat the codebase as an executable specification for a rewrite, not as a foundation to evolve.**

Why not evolve it:
- **The center is wrong.** All orchestration lives in one 8,486-line Textual `App` subclass. To get Claude Code-grade features (expandable tools, many concurrent sessions with their own transcripts, workflow authoring), that class would first have to be broken into an application layer. Doing that inside a codebase with 2.8 test lines per code line, many of them pinned to internals, costs about as much as a clean rebuild that reuses the domain ideas. It also carries the same risk.
- **The protocol layer must be redone anyway.** The prompt path, the compatibility baseline, and the event vocabulary all predate Hermes's server-to-client requests, typed contracts, `gateway.capabilities`, and the session authority (section 2). The prompt path is the core of approvals and the needs-you queue, which is Talaria's main differentiator. Rewriting that layer touches the reducer, the queue, the prompts UI, and app orchestration: roughly 7,000+ lines.
- **The rendering substrate is at its limit.** The recorded p99 of 44 ms against a 50 ms ceiling (`block-markdown-gate-results.md:35`) and the 500-line mounted cap with condensation (`transcript.py:105,112`) are symptoms of building a long-scrollback chat transcript out of retained Textual widgets. That is a framework-fit problem, not a tuning problem (judgment; not profiled).
- **The project's process overhead swamps its product.** 191 MB of tracked docs, 15,461 lines of acceptance scripts, 7,281 lines of document tests, and 37% of commits about acceptance and receipts. A rewrite that keeps tests behavioral (driven by record and replay) and keeps documents short would move faster.

What to take into the rewrite, in whatever language:
1. The frame-log format and the recorded corpora, as a regression oracle.
2. The pure reducer and projection architecture, with a real application layer (sessions, connections, prompts, settings services) between domain and UI.
3. The redaction rules and the "unknown outcome" call semantics.
4. Stated rendering ceilings gated in CI.
5. The fleet registry and needs-you queue model, extended with per-session transcript caches.
6. The status-line contract.
7. The reconciliation-rules documents, re-verified against Hermes `22063fc09` or later, with the client protocol generated from `tui_gateway/contracts/` instead of hand-pinned.

**Most likely way this verdict is wrong (pre-mortem):** if the rewrite underestimates how much hard-won edge-case behavior is encoded in the 2,980 tests (reconnect races, stale epochs, prompt expiry, scroll anchoring), it will rediscover those bugs one at a time. Mitigation: before deleting anything, mine `tests/domain` and `tests/transport` (34,098 lines together) for scenarios, and replay the existing corpora through the new client.

---

## Appendix: commands and scripts used

- `python3 -I scratchpad/research/tools/loc.py talaria`: counts code, comment, docstring, and blank lines.
- `.venv/bin/python scratchpad/research/tools/bench_projection.py`: the reducer and projection benchmark.
- `.venv/bin/python -m pytest -q -p no:cacheprovider tests/test_framework_boundaries.py tests/domain/test_boundary.py`: 7 passed in 0.69 s.
- `git -C ~/.hermes/hermes-agent rev-list --count 7f4d15515..HEAD` returned 36115; for `3236f4405..HEAD` it returned 21434.
- `git -C ~/.hermes/hermes-agent merge-base --is-ancestor ebe8cda8ea 3236f4405` succeeded, so the server-requests commit is contained in `3236f4405`.
- Scratch files are under `<scratch>/research/tools/`.
