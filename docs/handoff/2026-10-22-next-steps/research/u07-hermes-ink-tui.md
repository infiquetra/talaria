# Hermes's Ink terminal UI: features, rendering, start-up, limits, and comparison with Talaria

Unit u07-hermes-ink-tui. Research date 2026-10-22.

**Orientation.** Talaria is a third-party terminal client for Hermes Agent, the open-source agent backend from Nous Research. This report describes Hermes's own full-screen terminal user interface (TUI, the screen that `hermes --tui` opens). That TUI is written in TypeScript and uses Ink, a React renderer for terminals, in a forked copy. The operator named it as a model for Talaria's replacement, so the report covers what it does, how it draws the screen, how it starts, what its own documentation says is missing, and how far Talaria matches it.

Checked versions: Hermes at commit 22063fc09 (dated 2026-10-17). Talaria at HEAD cafc640 (dated 2026-09-25).

## 0. How to read this report

- **Hermes evidence.** The checkout is `<hermes>`. `git rev-parse HEAD` returned `22063fc099ec9ad8357d7f8f6e6248a0400680d3`. `git status --short` printed nothing, so the working tree equals that commit. Paths like `ui-tui/src/app/useInputHandlers.ts:761` are relative to that checkout and refer to revision 22063fc09.
- **Talaria evidence.** The checkout is `<talaria>`. HEAD is `cafc640`, a merge commit dated 2026-09-25. The feature commit it merged, `101a951`, is dated 2026-09-16, which matches the brief's "last commit" date. The package version is 0.6.5 (`talaria/__init__.py:3`).
- **Labels.** "Verified" means I read the code or ran a command and saw its output. "Docs" means a documentation page says so and I did not trace it in code. "Unverified" means I could not confirm it or did not look.
- **Not done.** I did not start the TUI, press keys in a real terminal, or time anything. Nothing here is a measured speed or size figure. I did not run `git fetch`, because the brief bars changes to either repository, so I did not look at the upstream v0.22.4 release from 2026-10-21.

## 1. Findings in brief

1. **The Node client owns the screen and local input state.** Node runs the Ink app. A Python gateway owns sessions, tools, and model calls. The two talk over an authenticated WebSocket. Verified: `ui-tui/src/gatewayClient.ts:552-600`, `ui-tui/scripts/gateway_bootstrap.py:42-106`, and `tui_gateway/AGENTS.md` ("Process model").
2. **The Python gateway is not a child process of the TUI.** The TUI asks a small helper to make sure a per-profile gateway is running, receives a one-use ticket, and connects. Closing the TUI only drops its socket. Verified: `ui-tui/src/gatewayClient.ts:958-975` (the kill method logs "detach only") and `ui-tui/CANONICAL_GATEWAY.md:11`. The README's description of a Python child process (`ui-tui/README.md:16`) is out of date.
3. **The default screen is the alternate screen.** The `--native` flag (alias `--tui-native`) switches to the main screen, so the terminal's own scrollback and search work, and mouse tracking starts off. This option is documented only in code and config comments. No page on the website mentions it. Verified: `hermes_cli/_parser.py:210`, `ui-tui/src/config/env.ts:50-77`, and zero matches for `tui_native`, `--native`, and `HERMES_TUI_NATIVE` under `website/docs`.
4. **Flicker control has four parts.** The renderer writes only changed cells. It wraps frames in synchronized-output markers where the terminal supports them. On the alternate screen it mounts only the visible transcript entries, plus a margin. It batches streamed text every 16 milliseconds. Verified: `ui-tui/packages/hermes-ink/src/ink/ink.tsx:1226-1234`, `ui-tui/src/hooks/useVirtualHistory.ts:19-50`, `ui-tui/src/config/timing.ts:1-4`. The README says the transcript uses `<Static>` (a component that renders once and never re-renders). That is wrong: nothing in `ui-tui/src` uses `<Static>`.
5. **Ctrl+T opens the subagent overlay.** Inside it, Enter opens the live transcript tail, not the details view as `ui-tui/README.md:81` says. Verified: `ui-tui/src/app/useInputHandlers.ts:761-763` and `ui-tui/src/components/agentsOverlay.tsx:911-912`.
6. **Ctrl+L redraws the screen.** It does not start a new session, which is what `ui-tui/README.md:161` says. Verified: `ui-tui/src/app/useInputHandlers.ts:810-813`.
7. **Approval prompts take numbers, Enter, arrows, and Esc.** The README's letter keys (`o`, `s`, `a`, `d`, `ui-tui/README.md:191`) are not in the code. Verified: `ui-tui/src/components/prompts.tsx:13` and `:61-88`.
8. **Submitting while the agent is busy queues the message by default in the TUI.** The Python default is `interrupt`, and the TUI overrides it to `queue`. Verified: `ui-tui/src/app/useConfigSync.ts:56` against `hermes_cli/config_defaults.py:845`. A comment at `ui-tui/src/app/useSubmission.ts:275` still calls `interrupt` the default.
9. **The docs say a failed TUI launch falls back to the classic command-line interface. The code exits instead.** Verified: `hermes_cli/main_tui_launch.py:209` (exit status 1 when Node is missing) and `hermes_cli/main.py:1753` (no fallback around the call). The claim is at `website/docs/user-guide/tui.md:310`.
10. **The repository lists its own gaps.** The Ink path does not claim support for secret and sudo prompts (sudo is the system password prompt) or for batch clarifications. The gateway-backed classic command-line interface lists further parity work outstanding. Verified: `ui-tui/CANONICAL_GATEWAY.md:60-76` and `website/docs/developer-guide/gateway-classic-cli.md:88-95`.
11. **Talaria's docs match its code on every key I checked.** Verified: `talaria/config.py:72-79` and `talaria/ui/app.py:1309-1337` against `docs/terminal-ui.md:216-240`. Talaria has no main-screen mode. It calls Textual's `app.run()` without the `inline` option (`talaria/cli.py:698` and `:1072`). Textual's default is the alternate screen (`textual/app.py:2308-2312` in the Textual 8.2.8 package inside Talaria's `.venv`).
12. **The Hermes TUI changed a great deal after Talaria's pinned revision.** Talaria pins 3236f4405 (dated 2026-09-14). Since then, 334 commits touched `ui-tui`, 328 files there changed (+24,596 / -5,996 lines), and 106 files under `tui_gateway` changed (+15,880 / -4,373). Verified with `git rev-list --count` and `git diff --stat` (section 9).

## 2. Architecture in brief

- **Node side (draws the screen).** `ui-tui/src/entry.tsx` starts the process. `ui-tui/src/app.tsx` and `ui-tui/src/app/*` hold the state and the key routing. `ui-tui/src/components/*` hold the screens. `ui-tui/packages/hermes-ink/` is a forked copy of Ink, the React renderer for terminals. It includes its own TypeScript port of the Yoga layout engine (Yoga is Facebook's flexbox layout library): `ui-tui/packages/hermes-ink/src/native-ts/yoga-layout/index.ts`.
- **Transport (Node side).** `ui-tui/src/gatewayClient.ts` opens the WebSocket. It sends JSON-RPC requests (JSON remote procedure calls), handles incoming events, reconnects after a drop, and tracks the subagent roster.
- **Python side (owns sessions).** `tui_gateway/` holds the method handlers (`server.py` and `methods_*.py`) and the WebSocket transport (`ws.py`). `tui_gateway/entry.py` is an older stdio transport. It is still in the tree and used by tests and probes, but the Ink client no longer launches it (section 5).
- **Contract.** Each method, server request, and event has a Pydantic model (a Python data-validation library) in `tui_gateway/contracts/`. A generator writes the TypeScript types to `apps/shared/src/gateway-contract.generated.ts`. Source: `tui_gateway/AGENTS.md`, section "Transport".
- **Ownership rule.** "TypeScript owns the screen. Python owns sessions, tools, model calls, and slash-command logic. Never move agent behaviour into the renderer." Source: `tui_gateway/AGENTS.md`, section "Process model".

## 3. Feature list with keybindings

**Conventions.** "Ctrl" means the Control key on Linux and Windows. On macOS a few shortcuts use Cmd instead. The action modifier is defined in `ui-tui/src/lib/platform.ts:12-36`. Ctrl+C stays the interrupt key on macOS (`platform.ts:1-9`). "Docs" marks a website claim I did not trace in code.

### 3.1 Starting, sessions, and exit

| Action | Key or command | Evidence |
|---|---|---|
| Start the TUI | `hermes --tui`, or `HERMES_TUI=1`, or `display.interface: tui` in config. The config default is `cli`. | `hermes_cli/_parser.py:208`; `hermes_cli/main.py:286-297`; `hermes_cli/config_defaults.py:855` |
| Force the classic command-line interface | `hermes --cli` | `hermes_cli/main.py:286-297` |
| Continue or resume | `-c` / `--continue`, `--resume <id, title, or latest>`, `--in <dir>` | Docs: `website/docs/user-guide/tui.md`, "Launch" |
| Auto-resume on launch | Off by default (`display.tui_auto_resume_recent: false`). `HERMES_TUI_RESUME=1` or a session id turns it on. | `hermes_cli/config_defaults.py:861`; docs `tui.md`, "Auto-resume" |
| Session switcher (live sessions in one TUI process) | Ctrl+X, or `/sessions` (alias `/switch`) | `ui-tui/src/app/useInputHandlers.ts:738`; docs `tui.md` |
| Inside the switcher | Up/Down select; Enter switch; Ctrl+N blank session; Ctrl+R refresh; Ctrl+D close the selected session; Tab (docs: choose a model for a new session, not traced); Esc close | `ui-tui/src/components/activeSessionSwitcher.tsx:643-693` |
| New session | `/clear` or `/new`. Asks for confirmation first, unless `HERMES_TUI_NO_CONFIRM=1`. | `ui-tui/README.md` command list; `ui-tui/src/config/env.ts:58`; docs `website/docs/user-guide/security.md:53` |
| Branch a session | `/branch` (alias `/fork`) | `ui-tui/src/app/slash/commands/session.ts:365` (command name entry) |
| Compress context | `/compress` | `ui-tui/src/app/slash/commands/session.ts:292` |
| Quit | `/quit` (alias `/exit`), or Ctrl+D on an empty composer | `ui-tui/README.md` command list; `ui-tui/src/app/useInputHandlers.ts:800-806` |
| Ctrl+C | Clears a typed draft. Interrupts a busy turn (one agent response to one message). Otherwise runs the idle-exit path. | `ui-tui/src/app/useInputHandlers.ts:769-799` |

### 3.2 Typing and editing in the composer

The composer is the text box at the bottom. Its editing code is `ui-tui/src/components/textInput.tsx`, and key routing is in `ui-tui/src/app/useInputHandlers.ts`.

| Action | Key | Evidence |
|---|---|---|
| Submit | Enter | `textInput.tsx:1447` |
| New line | Shift+Enter, Ctrl+Enter, Alt+Enter (on macOS, Option is "meta" and Cmd is "super") | `textInput.tsx:299-305` (`shouldInsertNewlineOnReturn`) |
| Continue a line | Put a backslash at the end of the line, then Enter | `ui-tui/src/app/useSubmission.ts:540` |
| Start or end of line | Home or Ctrl+A; End or Ctrl+E | `textInput.tsx:1468-1469` |
| Delete to start of line | Ctrl+U | `textInput.tsx:1470` |
| Delete to end of line | Ctrl+K (on Linux this overlaps with a global Ctrl+K binding; see section 10) | `textInput.tsx:1471` |
| Delete previous word | Ctrl+W | `textInput.tsx:1472` |
| Edit the draft in an editor | Ctrl+G, or Alt+G (a fallback for VS Code and Cursor, which bind the primary key to Find Next) | `useInputHandlers.ts:822-832`; `ui-tui/src/lib/editor.ts` |
| Paste | Ctrl+V, Cmd+V, or Alt+V. Text first, then a clipboard image if there is no text. | `textInput.tsx:1395-1400`; section 3.13 |
| Esc twice within 500 ms | Clears a typed draft if one exists. Otherwise interrupts a busy turn. Otherwise does nothing. | `useInputHandlers.ts:448-470`; `resolveDoubleEscAction` at `:104-118`; `ui-tui/src/config/timing.ts` (`DOUBLE_ESC_MS`) |
| Scroll the transcript | Shift+Up/Down; PgUp/PgDn; mouse wheel | `useInputHandlers.ts:619`, `:648-660` |
| Recall history or edit queue | Up/Down (queued items first, then history) | `useInputHandlers.ts:684-705` |
| Toggle auto-approve (YOLO mode) | Shift+Tab. Shows a YOLO badge in the status bar. | `useInputHandlers.ts:835-855`; docs `tui.md` "Status line" |
| Run a shell command | `!cmd`. Inline output: `{!cmd}`. | `ui-tui/README.md`, "App model" (docs only, not traced) |

### 3.3 Completions

| Action | Key or input | Evidence |
|---|---|---|
| Slash completion | A draft that starts with `/` asks the gateway (`complete.slash`) | `ui-tui/src/hooks/useCompletion.ts:37-38`, `:60-68` |
| Path completion | A last word that starts with `./`, `../`, `~/`, `/`, or `@` asks the gateway (`complete.path`) | `useCompletion.ts:32`, `:80` |
| Debounce | 60 ms | `useCompletion.ts:177` |
| Apply | Tab | `useInputHandlers.ts:857-862` |
| Move through the list | Up/Down while the list is open | `useInputHandlers.ts:611-614` |
| Display | Floating panel with descriptions | Docs `tui.md`, "Slash commands" |

For a non-local terminal backend, path completion lists directories on that backend, not on the gateway host (`tui_gateway/AGENTS.md`, "Key surfaces" table).

### 3.4 Slash commands

- **Owned by the TUI.** There are 66 command definitions in `ui-tui/src/app/slash/commands/*.ts` (core 25, session 19, ops 14, debug 4, and four more files). Counted with a search for `name:` lines.
- **Everything else goes to Python.** Unknown commands go to `slash.exec`, then `command.dispatch`. The Python side resolves aliases, plugins, and skills (`ui-tui/README.md`, "Commands"; `tui_gateway/AGENTS.md`, "Slash command flow").
- **Named in the README's command list.** Core: `/help`, `/quit` (`/exit`), `/update`, `/clear` (`/new`), `/density`, `/copy`, `/paste`, `/details` (`/detail`), `/statusbar` (`/sb`), `/queue` (`/q`), `/logs`, `/history`, `/save`, `/undo`, `/retry`, `/steer`, `/mouse` (`/scroll`), `/status`, `/title`, `/fortune`, `/redraw`, `/terminal-setup`. Session: `/model`, `/sessions` (`/switch`, `/session`, `/resume`), `/bg`, `/btw`, `/image`, `/personality`, `/compress`, `/branch` (`/fork`), `/voice`, `/skin`, `/indicator`, `/yolo`, `/reasoning`, `/fast`, `/busy`, `/verbose`, `/usage`. Ops: `/stop`, `/reload-mcp` (`/reload_mcp`), `/reload`, `/browser`, `/rollback`, `/agents` (`/tasks`), `/replay`, `/replay-diff`, `/skills`, `/reload-skills`, `/plugins`, `/tools`. Also `/billing`, `/credits`, `/setup`, `/heapdump`, `/mem`.
- **Also in the code.** `/theme` (`ui-tui/src/app/slash/commands/session.ts:518`) sits beside `/skin` (`:559`). The README does not list it, and I did not check how the two differ.

### 3.5 Model picker

| Action | Key or command | Evidence |
|---|---|---|
| Open | `/model`, or Ctrl+O. Ctrl+O works with a draft typed and mid-stream. Picking a model writes the session model, and the next turn uses it. | `session.ts:155`; `useInputHandlers.ts:756-759` and `:765-767` |
| Stages | provider, then key entry, then model, then reasoning effort, plus a disconnect stage | `ui-tui/src/components/modelPicker.tsx:23` (`type Stage`) |
| Filter | Type to filter providers or models | `modelPicker.tsx:211` |
| Clear filter | Ctrl+U | `modelPicker.tsx:515` |
| Disconnect a provider | Ctrl+D on the provider stage (hint: `^d disconnect`) | `modelPicker.tsx:532`; `ui-tui/src/i18n/en/pickers.ts:64` |
| Save scope | Ctrl+G toggles "persist" scope when the picker allows it | `modelPicker.tsx:399`, `:525`; `ui-tui/src/i18n/en/pickers.ts:66-71` |
| Cost hints | Docs say the picker shows cost hints (not traced) | Docs `tui.md`, "Slash commands" |

### 3.6 Subagent panel (Ctrl+T) and the live-work dock

A subagent is a child agent that a parent agent starts to do a task. A live-work dock is the strip above the composer that shows running subagents and background processes.

| Action | Key | Evidence |
|---|---|---|
| Open the subagent overlay | Ctrl+T (the same roster as `/agents`, alias `/tasks`) | `useInputHandlers.ts:761-763`; `agentsOverlay.tsx:825` (Ctrl+T closes it) |
| Toggle the dock between preview and one summary line | Ctrl+R, or F7 where the terminal sends it to the app. Does not change focus. | `useInputHandlers.ts:746-751`; docs `tui.md` |
| Select an agent | Up/Down, or j/k; g/G first and last | `agentsOverlay.tsx:915-927` |
| Open the live transcript tail | Enter (live mode), or `t` | `agentsOverlay.tsx:911-912`, `:833` |
| Open details | Right arrow or `l`, or `d` (Enter opens details only in replay mode) | `agentsOverlay.tsx:911-912`, `:837` |
| Steer a running agent | `e` opens a form. Enter queues guidance. "Queued" means accepted for the next tool boundary, not delivered. | `agentsOverlay.tsx:829`; `ui-tui/src/i18n/en/hubs.ts:149`; docs `website/docs/user-guide/features/delegation.md:508` |
| Stop the selected agent | `x` | `agentsOverlay.tsx:862` |
| Stop it and its children | `X` | `agentsOverlay.tsx:866` |
| Pause spawning | `p` | `agentsOverlay.tsx:858` |
| Step through recent batches | `<` or `[` (older), `>` or `]` (newer) | `agentsOverlay.tsx:850-857` |
| Sort or filter the list | `s` cycles sort order; `f` cycles filter | `agentsOverlay.tsx:931`, `:935` |
| In tail or detail view | h or Left goes back; PgUp/PgDn, Ctrl+U/Ctrl+D, and arrows scroll; g/G jump | `agentsOverlay.tsx:871-903` |
| Leave | Esc goes back to the list, then closes. `q` closes. | `agentsOverlay.tsx:841-849` |
| Recent batches for replay | `/replay` shows the last ten finished subagent batches | `ui-tui/README.md`, file map (`spawnHistoryStore.ts`) |

### 3.7 Approvals and other prompts

The Python side can pause a turn and ask the user something. The TUI shows the question as an overlay.

| Prompt | Keys | Evidence |
|---|---|---|
| Approval (allow a tool call) | Choices are once, session, always, deny, in that order. "Always" is hidden when the backend forbids it. | `ui-tui/src/components/prompts.tsx:13-16`, `:28-38` |
| Approval, choose | Type 1 to 4 to pick by number; Enter picks the highlighted choice; Up/Down move; Esc denies | `prompts.tsx:61-88` |
| Approval, cancel | Ctrl+C cancels the open prompt. The README says this denies; the mapping was not traced. | `ui-tui/src/app/useInputHandlers.ts:521` |
| Clarify (multiple choice or free text) | Up/Down select; Enter confirm; Tab and Shift+Tab switch question; Esc or Ctrl+C cancel | `ui-tui/src/i18n/en/overlays.ts:21-23` |
| Sudo and secret prompts (masked input) | Enter submits; Ctrl+C cancels. The repository says these are not claimed on the canonical gateway path. | `ui-tui/README.md`, "Prompt flows"; `ui-tui/CANONICAL_GATEWAY.md:60-76` |

### 3.8 Queue (typing while the agent is busy)

| Behavior | Detail | Evidence |
|---|---|---|
| Default mode | Typing while busy queues the message. Other modes: `steer` (inject into the running turn) and `interrupt` (stop and redirect). Set with `display.busy_input_mode`. | `ui-tui/src/app/useConfigSync.ts:47-56`; `ui-tui/src/app/interfaces.ts:57`; `ui-tui/src/app/useSubmission.ts:270-276` |
| Preview | Shows up to three queued items, with a line that says how many more | `ui-tui/src/components/queuedMessages.tsx:7` |
| Edit | Up/Down reach queued items before history | `useInputHandlers.ts:684-705` |
| Cancel an edit | Esc | `useInputHandlers.ts:676` |
| Remove an item | Ctrl+X on a highlighted queued item | `useInputHandlers.ts:732-735` |
| Send the next item now | Ctrl+K (also matches the composer's delete-to-end; see section 10) | `useInputHandlers.ts:867-873` |
| Survives a gateway restart | Docs say the queue lives on the gateway and survives a crash (not traced) | Docs `tui.md` line 114 |

### 3.9 Vim-style keys

- **No modal editing.** The composer has no normal or insert mode. A search for vi-mode settings in `ui-tui` found nothing.
- **Pager.** Some overlays use a read-only pager. j/k move one line; b or PgUp page back; Space, Enter, or PgDn page forward (and close at the end); g and G go to top and bottom; q or Esc closes. Evidence: `ui-tui/src/app/useInputHandlers.ts:528-575`.
- **Subagent overlay.** j/k, g/G, h, and l work there too (section 3.6).
- **Real vim.** Ctrl+G opens whatever `$EDITOR` names, so vim or Neovim can be used for long drafts (section 3.2).

### 3.10 Status bar

| Setting | Values or behavior | Evidence |
|---|---|---|
| Position | `display.tui_statusbar`: `top` (default), `bottom`, or `off` | `ui-tui/src/app/uiStore.ts:36`; `ui-tui/src/app/interfaces.ts:44`; `ui-tui/src/app/useConfigSync.ts:30-31` |
| Toggle | `/statusbar` (alias `/sb`) | `ui-tui/src/app/slash/commands/core.ts:653`, `:668` |
| Field filter | `display.status_bar.fields` (a list). An empty list means the default set. | `ui-tui/src/app/useConfigSync.ts:37-46`; `hermes_cli/config_defaults.py:996-997` |
| Width tiers | Below 72 columns the context count is compact. From 72 the context bar shows; from 76 elapsed time; from 80 the compression count; from 84 voice state; from 88 background-task count; from 92 subagent count; from 96 cache-hit rate. Latency and token-rate segments also exist; I did not read their thresholds. | `ui-tui/src/components/appChrome.tsx:341-365` |
| Content | Working directory with git branch; elapsed time per prompt; compression count; background task count; YOLO badge; session title badge; a status word (for example "thinking", "ready", "interrupted") | Docs `tui.md`, "Status line" (the in-code fields are not all traced) |
| Busy indicator style | `display.tui_status_indicator`: `kaomoji` (default), `emoji`, `unicode`, or `ascii` | `hermes_cli/config_defaults.py:919`; docs `tui.md`, "Busy indicator styles" |

### 3.11 Notifications

| Item | Behavior | Evidence |
|---|---|---|
| Gateway notices | The gateway sends `notification.show` (with id, key, kind, level, text, and optional time-to-live) and `notification.clear` (by key). Credit and usage notices are the producers the code comments name. | `ui-tui/src/app/createGatewayEventHandler.ts:1241-1256` |
| Timing | A notice that arrives during a turn waits and shows at turn end. Latest notice wins. Clear only removes a notice with a matching key. | `ui-tui/src/app/turnController.ts:243-262` |
| Bell on turn complete | `display.bell_on_complete` (default off) | `hermes_cli/config_defaults.py:866` |
| Bell on prompt | `display.bell_on_prompt` (default off). Sends the terminal bell character when a prompt opens. | `hermes_cli/config_defaults.py:867`; `ui-tui/src/app/createGatewayEventHandler.ts:702-707` |
| Quiet alerts | Docs say ambient messages are hidden by default and surface as floating alerts only if every panel is hidden (not traced) | Docs `tui.md`, "Default visibility" |

### 3.12 Themes and skins

- **Nine built-in skins.** `default`, `ares`, `mono`, `slate`, `daylight`, `warm-lightmode`, `poseidon`, `sisyphus`, `charizard`. Defined in `hermes_cli/skin_engine.py` at lines 64, 97, 141, 158, 173, 192, 209, 253, and 298.
- **Live preview.** `/skin <name>` changes the theme as you browse (`ui-tui/src/app/slash/commands/session.ts:559`). Docs: `website/docs/user-guide/features/skins.md`.
- **How skins reach the screen.** The skin arrives in the `gateway.ready` event and is merged over `DEFAULT_THEME` (`ui-tui/README.md`, "Theme model"; `ui-tui/src/theme.ts:797`).
- **Light terminals.** Three checks, in order: `HERMES_TUI_THEME` (light, dark, or a hex colour), then `COLORFGBG`, then a background-colour query (OSC 11, a terminal escape sequence). The query is asserted in `ui-tui/packages/hermes-ink/src/ink/osc-response-chain.test.ts:32`. Docs: `tui.md`, "Light-terminal detection".

### 3.13 Image paste and attachments

| Action | How | Evidence |
|---|---|---|
| Paste | Ctrl+V, Cmd+V, or Alt+V: text first, then a clipboard image | `textInput.tsx:1395-1400`; `ui-tui/src/app/useComposerState.ts:306` |
| Attach by path | Type or drop a path. Sends `image.attach` with the path. | `useComposerState.ts:487-514`, `:503` |
| Attach command | `/image <path>` | `ui-tui/src/app/slash/commands/session.ts:252` |
| Clipboard reader | A helper, `ui-tui/scripts/clipboard_image.py`, runs on the Ink host and is copied into the bundle (`scripts/build/tui.mjs:73`). Non-PNG clipboard images use Pillow when it is installed and otherwise depend on ImageMagick, per the helper's comment (`clipboard_image.py:18-19`). | As listed |
| Where images go | Local sessions copy the image into the profile's `cache/images` folder. Remote connections upload the bytes. | `ui-tui/CANONICAL_GATEWAY.md:35-59` |
| Busy turn | An image in an interrupt correction is refused. Use queue mode for an image-bearing turn. | `ui-tui/CANONICAL_GATEWAY.md:35-59` |

### 3.14 Other

- **Mouse.** At start, mouse tracking is on in "all" mode (including hover), except in native mode, on Termux, or when `HERMES_TUI_DISABLE_MOUSE` is set (`ui-tui/src/config/env.ts:52-56`). The setting is `display.mouse_tracking` (`off`, `wheel`, `buttons`, `all`), and `/mouse` switches presets (docs `tui.md`; `ui-tui/README.md` command list). Copy with a selection: Cmd+C on macOS, Ctrl+C elsewhere (`useInputHandlers.ts:710-719`).
- **Voice.** `/voice` (`session.ts:388`). Push-to-talk defaults to Ctrl+B (`ui-tui/src/lib/platform.ts:84-86`). The key is set by `voice.record_key`.
- **Markdown.** Headings, lists, quotes, tables, code blocks with syntax colours, diffs, links (`ui-tui/README.md`, "Rendering"; `ui-tui/src/components/markdown.tsx`; `ui-tui/src/lib/syntax.ts`). LaTeX math is shown as Unicode (`ui-tui/src/lib/mathUnicode.ts`; docs `tui.md`, "LaTeX math rendering").
- **Standing goal.** `/goal` shows a row above the dock (docs `tui.md`, line 108).
- **Background tasks.** `/bg` runs a task in the background. A counter in the status bar shows how many are running (docs `tui.md`). The dock also has a "Processes" block for background terminal commands (docs `tui.md`, line 107).
- **Mascot.** `/pet` (`session.ts:493`). Off by default (`hermes_cli/config_defaults.py:1002-1003`).

## 4. Rendering: alternate screen, `--native`, and flicker

### 4.1 Default: the alternate screen

- At start, unless native or Termux, the process clears the screen and the scrollback with `ESC[2J ESC[H ESC[3J` (`ui-tui/src/entry.tsx:51-55`).
- The whole app sits inside an `AlternateScreen` wrapper, unless `INLINE_MODE` is true (`ui-tui/src/components/appLayout.tsx:594-595`). The wrapper enters DEC private mode 1049, which is the terminal's alternate screen (`packages/hermes-ink/src/ink/components/AlternateScreen.tsx:32`, `:76`). It turns mouse tracking off and leaves the alternate screen when it unmounts (`AlternateScreen.tsx:94`).
- Scrolling happens inside the app's own viewport (section 3.2). When the app exits, the previous terminal content comes back. The docs say "no scrollback clutter after you quit" (docs `tui.md`, line 61). I did not run this.

### 4.2 What `--native` changes

**How it is set.** The flag is `--native` or `--tui-native` (`hermes_cli/_parser.py:210` and `:321`). It is also `display.tui_native` in config (`hermes_cli/config_defaults.py:858`, default `false`). The launcher passes it to the Node process as `HERMES_TUI_NATIVE=1` or `0` (`hermes_cli/main_tui_launch.py:476`). The Node side reads that value at `ui-tui/src/config/env.ts:50`.

**What changes (verified in code):**

1. **No alternate screen.** `INLINE_MODE` becomes true (`env.ts:77`), so the app renders with no `AlternateScreen` wrapper (`appLayout.tsx:594`). The transcript is ordinary document flow. The code says so: "Native mode deliberately leaves the transcript in ordinary document flow. The terminal owns scrollback" (`ui-tui/src/hooks/useVirtualHistory.ts:329-332`).
2. **No virtualization.** Every transcript row is mounted (`useVirtualHistory.ts:329-332` and `:565-567`). Long sessions therefore mount more rows than in alternate-screen mode. The cost is not measured here.
3. **Mouse tracking off at boot.** `resolvedBootMouseEnabled` is false in native mode (`env.ts:52-56`).
4. **Start and exit.** The start writes a newline instead of clearing the screen (`entry.tsx:51-55`). On exit the native frame is cleared (`entry.tsx:43-44`).
5. **Resize.** The main-screen redraw erases only the visible screen, not the scrollback. The code comment says: "No CSI 3J: scrollback is the user's history here" (`packages/hermes-ink/src/ink/ink.tsx:1086-1092`; CSI 3J is the escape code that erases scrollback).
6. **Status bar always at top.** In native mode the status rule is drawn in the top slot, even when the setting says `bottom` (`appLayout.tsx:420` draws the top slot, `:503` the bottom slot, and `:538` returns nothing for the bottom slot in native mode).

**Unverified.** I did not confirm whether PgUp, Shift+Up, the subagent overlay, or `/mouse` behave the same in native mode. The mouse-tracking prop goes only to `AlternateScreen` (`appLayout.tsx:595`), so it may not apply in native mode.

**Documentation gap.** `--native` appears only in the `--help` text (`_parser.py:210`: "use native terminal scrollback and disable mouse tracking") and in a config comment (`config_defaults.py:857`). No website page mentions it.

### 4.3 How flicker is limited

| Mechanism | Detail | Evidence |
|---|---|---|
| Cell-level diff | Each frame is compared with the one before, and only changed cells are written | `packages/hermes-ink/src/ink/ink.tsx:1226-1234` (`this.log.render(prevFrame, frame, altScreenActive, SYNC_OUTPUT_SUPPORTED)`); `packages/hermes-ink/src/ink/log-update.ts` imports the cell-diff helper `diffEach` |
| Synchronized output | Where the terminal supports DEC mode 2026, each frame is bracketed by begin and end markers (BSU and ESU), so the terminal paints it in one step. Markers are skipped where unsupported, because multiplexers such as tmux or Zellij can mishandle them. | `packages/hermes-ink/src/ink/terminal.ts:351`; `packages/hermes-ink/src/ink/termio/dec.ts:23`, `:37`; `ink.tsx:1086-1092` |
| Hardware scrolling | The scroll-region code (DECSTBM, "set top and bottom margins") is used only on the alternate screen | `packages/hermes-ink/src/ink/frame.ts:10`; `packages/hermes-ink/src/ink/log-update.ts:157` |
| Virtualized transcript | Alternate screen only. A margin of 20 items beyond the viewport, at most 120 transcript items mounted, 30 at cold start, and at most 12 new items mounted per commit. | `ui-tui/src/hooks/useVirtualHistory.ts:19`, `:25-26`, `:50` |
| Batched streaming | Streamed text is flushed every 16 ms by default. After a keystroke the interval becomes 80 ms. After a scroll it is at least 96 ms. | `ui-tui/src/config/timing.ts:1-4`; `ui-tui/src/app/turnController.ts:208-215`, `:1047` |
| Resize coalescing | Resize bursts are merged into one reflow per 32 ms | `ui-tui/src/config/timing.ts:14` |
| Static output | None. The README's `<Static>` claim is wrong. | `ui-tui/README.md:129,142` against a zero-match search of `ui-tui/src` |

A code comment near the virtualization constants quotes profiling figures (renderer p99 of 106 ms and about 23,000 live layout nodes during fast paging, `useVirtualHistory.ts:21-24`). I did not reproduce them. p99 means the 99th percentile of frame time.

### 4.4 The renderer

- `@hermes/ink` is a fork of Ink, kept in `ui-tui/packages/hermes-ink` (`ui-tui/README.md`, file map: "forked Ink renderer (local dep)"; `packages/hermes-ink/package.json`, version 0.0.1).
- The production build inlines the fork's source, not its prebuilt bundle (`scripts/build/tui.mjs:44-51`). The build file's comments give the startup-order reasons.
- The renderer file `packages/hermes-ink/src/ink/ink.tsx` runs to 2,850 lines.

## 5. Start-up path

Each step is verified in the code at the revision named above.

1. **Command.** `hermes --tui` is parsed (`hermes_cli/_parser.py:208`). Before argument parsing, `_wants_tui_early` decides the surface. Precedence: `--cli` wins; then `--tui`, `--native`, or `HERMES_TUI=1`; then a terminal check; then `display.interface` (`hermes_cli/main.py:283-297`).
2. **Launcher.** The chat path calls `_launch_tui` (`hermes_cli/main.py:1753`). `_launch_tui` (`hermes_cli/main_tui_launch.py:430`) builds the child environment: working directory (`HERMES_CWD`, `TERMINAL_CWD`, `HERMES_TUI_CWD`), the temporary active-session file (`:466`), `HERMES_TUI_NATIVE` (`:476`), model, provider, toolsets, skills, and query values, and a V8 heap cap through `NODE_OPTIONS` (`:521-524`, default 8192 MB, lowered to fit any cgroup memory limit).
3. **Bundle lookup.** `_make_tui_argv` (`main_tui_launch.py:213-266`) tries, in order:
   a. `HERMES_TUI_DIR/dist/entry.js`, a prebuilt bundle named by the environment;
   b. `hermes_cli/tui_dist/entry.js`, a bundle inside the Python package (`_find_bundled_tui`, `:135-141`);
   c. otherwise, a source build when the bundle is stale (`_tui_need_rebuild`, `:128-132`, which calls `source_product_current` in `hermes_cli/source_build.py:10`).
   For `--dev`, it runs `tsx src/entry.tsx` instead (`:256-262`).
4. **Node.** `_tui_node_bin` (`main_tui_launch.py:195-210`) uses `HERMES_NODE` if set; otherwise the package manager's `node` (the `pm` module, a dependency manager for Hermes's packages). If Node is missing it prints advice and exits with status 1 (`:209`).
5. **Child process.** `subprocess.call([node, "--expose-gc", <entry.js>], cwd, env)` runs the TUI and waits (`main_tui_launch.py:537`). The function's docstring says "Replace current process" (`:437`), which the code does not do. Exit status 42 means the TUI asked for an update, and the launcher runs `hermes update` (`:567`).
6. **Node entry.** `ui-tui/src/entry.tsx` exits with status 0 if stdin is not a terminal (`:19-22`), resets terminal modes (`:26`), creates `GatewayClient` (`:57`), starts it (`:61`), and calls `ink.render(<App gw={gw} />, …)` (`:178`).
7. **Gateway attach.** `GatewayClient.start()` (`ui-tui/src/gatewayClient.ts:718`) uses an explicit `HERMES_TUI_GATEWAY_URL` if set. The docs call that a dashboard detail (`tui.md:295`). Otherwise `startLocalGateway()` (`:552`) runs `bootstrapLocalGateway` (`ui-tui/src/localGatewayBootstrap.ts:36-41`). That runs `ui-tui/scripts/gateway_bootstrap.py` with a 40-second timeout, and the script prints a grant: `{url, protocols, profile_id, instance_id}`. Then `startAttachedGateway()` (`gatewayClient.ts:600`) opens the WebSocket.
8. **Helper script.** `gateway_bootstrap.py` calls `ensure_gateway_runtime(home, timeout=30, idle_exit=True)` on first launch (`gateway_bootstrap.py:72-73`), or discovers an existing one (5-second timeout). On a reconnect it re-ensures the gateway only within a small budget (`:77`). It then asks the gateway for a session ticket over a private control socket (`:83` onward).
9. **Owner process.** If none is running, `spawn_unmanaged_gateway` starts `python -m hermes_cli.main … gateway run --quiet --idle-exit` as a background process (`hermes_cli/gateway_runtime_start.py:46-47`). The `--idle-exit` daemon stops itself after `gateway.unmanaged_idle_exit_seconds`, default 600 seconds (`gateway/run_idle_exit.py:3-6`, `:32`).
10. **Listener.** The gateway's own HTTP and WebSocket listener serves the ticketed `/api/ws` route for local CLI, TUI, and Desktop sessions (`gateway/run_api.py:1-7`, `:258`, `:288`, `:301`, and `:368`, which imports `tui_gateway.ws.handle_ws`). The dashboard also registers `/api/ws` (`hermes_cli/web_routers/chat_ws.py:702-707`).
11. **Reconnect.** Reconnect backoff starts at 1 second and caps at 30 seconds (`gatewayClient.ts:47-48`, schedule at `:335`). Owner recovery allows 3 attempts per 60 seconds (`:105-106`, claim at about `:587`).
12. **Exit.** Quitting calls `kill()`, which only detaches the socket (`gatewayClient.ts:958-975`). The gateway keeps running.

**Is the bundle prebuilt?** It depends on the install.

- **Packaged installs** (Nix builds, and wheels built inside Nix) can ship a prebuilt `tui_dist/entry.js`, or point at one with `HERMES_TUI_DIR`. The packaging guard in `setup.py` says pip, PyPI, and Homebrew are no longer supported distribution methods, and wheels are built only inside a Nix build (`setup.py:1-20`). Docs: `website/docs/user-guide/tui.md`, "External prebuild".
- **Git checkouts** (this one) build on demand. The build is one esbuild bundle (`scripts/build/tui.mjs:35-52`, target `node20` at `:39`), with the forked Ink inlined from source (`:51`). The helper scripts are copied beside it (`:73`). Source-build functions: `hermes_cli/source_build.py:82` (`prepare_launch_dependencies`) and `:92` (`build_source_tui`).
- **This checkout.** `ui-tui/dist/entry.js` is about 4.07 MB and is dated 2026-10-18. Its `hermes-build.json` records platform darwin/arm64 and Node 26.7.0. The `node` on this machine's PATH is v22.22.3 (`node -v`). `hermes_cli/tui_dist` does not exist here.

## 6. Limitations recorded in the repository

| Limitation | Where recorded | Scope note |
|---|---|---|
| Fresh TUI sessions need a gateway that advertises TUI launch support. Older CLI-only runtimes are refused. | `ui-tui/CANONICAL_GATEWAY.md:60-76` | Describes one change set. Read as current only where it matches the code. |
| The Ink client does not claim secret and sudo prompts, or batch clarifications. | `ui-tui/CANONICAL_GATEWAY.md:60-76` | Same. |
| Tool and skill inventories show "unavailable", not lists. | `ui-tui/CANONICAL_GATEWAY.md:60-76` | Same. |
| Model chooser needs an owner-side `model.options` projection. Older owners reject busy controls and remote image uploads. | `ui-tui/CANONICAL_GATEWAY.md:60-76` | Same. |
| Session switch in one view can leave old subscriptions until the socket closes. | `ui-tui/CANONICAL_GATEWAY.md:60-76` | Same. |
| Subagent panels "not claimed" on this path. | `ui-tui/CANONICAL_GATEWAY.md:60-76` | The subagent overlay exists in code (`agentsOverlay.tsx`). Whether it works on the canonical path today is unverified. |
| The gateway-backed classic command-line interface refuses `-w`, `--image`, `--run-budget`, `-v`, `--no-restore-cwd`, and `--create-if-missing` without a name. | `website/docs/developer-guide/gateway-classic-cli.md:73-86` | Classic CLI, not Ink. The docs say the Ink path does not yet carry `-v`. |
| Classic CLI on the gateway path lacks full legacy presentation, slash registry parity, interactive history editing, auto-reconnect, durable client journals, remote login bootstrap, cold-owner resume, and native Windows and macOS quality assurance. | `website/docs/developer-guide/gateway-classic-cli.md:88-95` | Different client from Ink. Ink does reconnect (`gatewayClient.ts:335`). |
| Windows: docs say `hermes --tui` runs on native Windows and WSL2. | `website/docs/user-guide/windows-native.md:122` | Docs claim. Unverified. |
| Native mode is undocumented on the website. | `hermes_cli/_parser.py:210`; `config_defaults.py:857` | Code and config comments only. |
| The `tui_gateway` guide says the TUI "fully replaces" the classic prompt CLI. | `tui_gateway/AGENTS.md:3` | Out of date. The classic CLI is still the default (`config_defaults.py:855`). |
| A reply in flight is lost when a spawned gateway dies. | Docs `website/docs/user-guide/tui.md:301` | Docs claim. |

## 7. Where the docs disagree with the code

Code wins where they conflict. Each row names the claim, where it sits, and what the code does.

| Claim | Where it sits | What the code does | Verdict |
|---|---|---|---|
| "By default the TUI spawns its own in-process gateway." | `website/docs/user-guide/tui.md:293` | Ink attaches to a shared per-profile gateway through `gateway_bootstrap.py` and a WebSocket (`gatewayClient.ts:552-600`; `gateway_bootstrap.py:72-83`). | Stale |
| "The `/api/ws` endpoint exists only inside the dashboard server." | `website/docs/user-guide/tui.md:295` | The gateway's own listener serves it for local CLI, TUI, and Desktop sessions (`gateway/run_api.py:1-7`, `:258`, `:368`). | Contradicted |
| "If the TUI fails to launch … Hermes prints a diagnostic and falls back." | `website/docs/user-guide/tui.md:310` | Missing Node exits with status 1 (`main_tui_launch.py:209`); the call has no fallback (`main.py:1753`). | Contradicted |
| "`Ctrl+G` / `Ctrl+X Ctrl+E` open the input buffer in `$EDITOR`." | `website/docs/user-guide/tui.md:116` | Ctrl+G and Alt+G do. Ctrl+X opens the session switcher (`useInputHandlers.ts:738`). No two-key chord handler found. | Contradicted for Ctrl+X Ctrl+E |
| "`Ctrl+L` starts a new session." | `ui-tui/README.md:161` | Ctrl+L redraws (`useInputHandlers.ts:810-813`). | Contradicted |
| "Approval quick-picks with `o`, `s`, `a`, `d`." | `ui-tui/README.md:191` | Numbers only, plus Enter and arrows (`prompts.tsx:61-88`). | Contradicted |
| "Enter opens its details" (subagent overlay). | `ui-tui/README.md:81` | Enter opens the live tail in live mode (`agentsOverlay.tsx:911-912`). | Contradicted |
| "The transcript is `<Static>` output." | `ui-tui/README.md:129`, `:142` | No `<Static>` in `ui-tui/src`. | Contradicted |
| "`GatewayClient` spawns `python -m tui_gateway.entry`." | `ui-tui/README.md:16`; `tui_gateway/event_publisher.py:3` | No child-process code in `gatewayClient.ts`. Startup goes through `gateway_bootstrap.py`. | Stale |
| "Default busy mode is `interrupt`." | `ui-tui/src/app/useSubmission.ts:275` (comment) | TUI default is `queue` (`useConfigSync.ts:56`). | Stale comment |
| Ctrl+K deletes to end of line. | `ui-tui/README.md:175` | Composer does this (`textInput.tsx:1471`). A global handler also maps Ctrl+K to "send next queued item" (`useInputHandlers.ts:867`). | Partly true; overlap |
| `_launch_tui` "Replace current process with the TUI." | `hermes_cli/main_tui_launch.py:437` | Uses `subprocess.call` and waits (`:537`). | Docstring wrong |
| "The `Alt+K` key retries a submission." | `ui-tui/src/i18n/en/canonical.ts:23`, `:33`, `:41` (user-visible hints) | No key handler for Alt+K found in the TUI code. | Unverified |

## 8. Comparison with Talaria

Columns: Area; Hermes Ink TUI (revision 22063fc09); Talaria (HEAD cafc640). Talaria entries cite its docs (`docs/terminal-ui.md`, `README.md`) unless marked "code". Where a feature is missing from Talaria's docs, the cell says "not in docs". That is not proof the feature is absent from Talaria's code.

| Area | Hermes Ink TUI | Talaria |
|---|---|---|
| Stack | Node, Ink (forked React renderer), Yoga layout port. Python gateway. (section 2) | Python 3.12 and Textual 8.2.8 (`README.md:17`; Textual in `.venv`). ADR-0004 and ADR-0005. |
| Screen mode | Alternate screen by default. `--native` uses the main screen with terminal scrollback (section 4). | Alternate screen only. Calls `app.run()` with no `inline` option (code: `talaria/cli.py:698`, `:1072`). No main-screen mode in docs. |
| Gateway connection | Shared per-profile gateway, started on demand. Ticketed WebSocket. Survives TUI exit. | Dials a configured fleet of gateways over WebSocket (`README.md:65-67`). Credential from a dashboard token file (`README.md`, "Supplying the gateway credential"). |
| Profiles | Hermes profiles each get a gateway. The TUI's components and its own command list have no profile picker. Profile switching was not traced further. | `/profiles` (F7 or F12) lists profiles and dials the selected one (`README.md`, "Switching Hermes profile"; `terminal-ui.md:232`). |
| Sessions | Session switcher for live sessions (Ctrl+X). `/sessions`, `/resume`, `/branch`, `/compress`, `--continue`, `--resume`, `--in`. | `--resume` and `--session` flags (`README.md`, "Quick start"). `/sessions` is in the palette (`terminal-ui.md:168`). Switcher behavior not described. |
| Subagents | Overlay with live tail, detail, steer (queued for next tool boundary), stop, subtree stop, pause, history, sort, filter. Dock above the composer. | Toggle subagent rows with Ctrl+G or F2 (`terminal-ui.md:228`). Tasks section in the inspector (`terminal-ui.md:62-124`). Steer, stop, and pause for subagents are not in docs. |
| Approvals | Numbered choices with Enter and Esc. Overlay blocks other keys. | "Approval card owns focus" (`README.md`, Quick start). Detail not described in `terminal-ui.md`. |
| Work queue while busy | Typed messages queue by default. Edit with Up/Down. Remove with Ctrl+X. Modes: queue, steer, interrupt. | "Needs-you queue" is an attention list for items awaiting the operator, shown as a count in the status bar (`terminal-ui.md:23`, `:85`). Typed follow-ups while busy not described in docs. |
| Model picker | Ctrl+O or `/model`. Provider, key, model, and reasoning stages. Filter by typing. | F11 or `/models` (`terminal-ui.md:231`). Picker detail not described. |
| Status bar | Position top, bottom, or off. Fields chosen with a list. Width tiers (section 3.10). | Bottom bar with seven segments: cwd, git branch, agent and model, context, task progress, connection, version. Fixed drop order by width (`terminal-ui.md:15-60`). |
| Inspector panel | None. Dock and overlays instead. | Right inspector with Tasks, Context, Changed files, Operation details, Diagnostics, and Keys. Ctrl+O toggles it (`terminal-ui.md:62-124`, `:225`). |
| Diff viewing | Inline diffs in tool results (the `tool.complete` event carries `inline_diff`, `ui-tui/README.md`, event table). | Read-only diff viewer, side by side or unified, with hunk and file keys (`terminal-ui.md:125-157`). |
| Command palette | Tab completion for slash commands and paths. Floating panel (section 3.3). | `/` opens a palette with sections, provenance badges, and filter-as-you-type. F3 browses (`terminal-ui.md:158-214`). |
| Attachments | Ctrl+V or Cmd+V pastes a clipboard image. Paths can be dropped or typed. `/image`. | `/attach` with a confirm step. Text, code, and image files. PDF refused. Drag a path onto the terminal (`terminal-ui.md:241-261`). Clipboard image paste not in docs. |
| Settings | `config.yaml`. Startup wizard `/setup`. TUI has no settings screen of its own. | `/config` settings workspace with Stay, Save, and Discard, and scoped writes (`terminal-ui.md:262-330`). |
| Themes | Nine built-in skins, custom skins, light-terminal detection, live `/skin` preview (section 3.12). | Theme picker in the settings workspace (`terminal-ui.md:297`). Theme list in `docs/themes.md` (not read here). |
| Reduced motion | Not found. The busy indicator has four styles. | `ui.reduced_motion` makes spinners static and scrolling immediate (`terminal-ui.md`, "Focus, motion, and scroll"). |
| Mouse | Tracking presets; drag to select; click links. | Not in docs. |
| Keys: quit, interrupt | Ctrl+D quits an empty composer. Ctrl+C interrupts a busy turn or clears the draft (`useInputHandlers.ts:769-800`). | Ctrl+Q quits (`talaria/ui/app.py:1309`). Ctrl+S interrupts the turn (`talaria/config.py:74`). Ctrl+C is not interrupt (`terminal-ui.md:255-259`). |
| Replay and recording | Not found. `/replay` shows finished subagent batches, not session recordings. | `talaria record`, `talaria replay` with F8 pause and F9/F10 speed (`README.md`, Quick start). Playback is a Talaria feature. |
| Voice | `/voice`, push-to-talk on Ctrl+B. | Not in docs. |
| Markdown and math | Markdown with syntax colour. LaTeX rendered as Unicode. | Not in docs. |
| External editor | Ctrl+G opens `$EDITOR`. | Not in docs. |
| Background tasks | `/bg` tasks with a counter; "Processes" block in the dock. | Not in docs. |
| Vim-style keys | j/k and g/G in pager and overlays. No modal editing. | Not in docs. |
| Reconnect and recovery | Automatic reconnect with backoff; owner recovery budget (section 5, step 11). | Connection segment shows states such as `[ok]`, `[..]`, `[x]` (`terminal-ui.md:15-30`). Recovery behavior not described. |

What Talaria's docs describe that Hermes's TUI does not: the right inspector, the read-only diff viewer, the settings workspace with scoped writes, recording and replay, the multi-gateway fleet view with `/profiles`, the responsive status bar with a fixed drop order, and reduced motion.

What Hermes's TUI has that Talaria's docs do not describe: native main-screen mode, the live-work dock, steer, pause, and spawn-history controls, typed follow-ups while busy with edit and remove, voice push-to-talk, LaTeX rendering, the external editor handoff, clipboard image paste, nine skins with light-terminal detection, and mouse presets.

## 9. What changed since Talaria's pin

- Talaria's pinned Hermes revision is `3236f4405`, dated 2026-09-14 (the merge of PR #111038, "fix/local-runtime-cleanup"). `git cat-file -t` confirms the commit exists in this checkout.
- Commits touching `ui-tui` since that pin: `git rev-list --count 3236f4405..HEAD -- ui-tui` returned 334.
- Diff size: `git diff --stat 3236f4405 HEAD -- ui-tui` ended with "328 files changed, 24596 insertions(+), 5996 deletions(-)". For `tui_gateway`: "106 files changed, 15880 insertions(+), 4373 deletions(-)".
- **Hypothesis (not verified):** the ticketed WebSocket start-up (`gateway_bootstrap.py`, `CANONICAL_GATEWAY.md`) is newer than the pin, so any Talaria assumption about the wire protocol, reconnects, or credentials should be re-checked against 22063fc09 before design work. I did not diff the wire contract to confirm this.

## 10. Unverified items and open questions

1. **Ctrl+K on Linux and Windows.** Two handlers match: the composer's delete-to-end (`textInput.tsx:1471`) and the global "send next queued" (`useInputHandlers.ts:867`). Which one wins was not checked by running the TUI.
2. **Alt+K retry.** The user-visible hints name it (`ui-tui/src/i18n/en/canonical.ts`). No handler was found.
3. **Native mode behavior.** Whether PgUp, Shift+Up, the subagent overlay, and `/mouse` behave the same as in alternate-screen mode.
4. **Native mode cost.** Every row is mounted in native mode (section 4.2). Its speed on long sessions was not measured.
5. **Port 9119.** Talaria's docs point at `ws://127.0.0.1:9119/api/ws` for the dashboard. Whether the gateway's own listener uses the same port on a default install was not checked.
6. **Ctrl+C on an approval prompt.** The README says deny. The code cancels the overlay (`useInputHandlers.ts:521`). The mapping was not traced.
7. **Windows.** Docs say native Windows works (`windows-native.md:122`). Not verified.
8. **Docs-only claims not traced:** model picker cost hints; queue survival across a gateway restart; "instant first frame"; "no flicker"; reply lost on a spawned-gateway crash; the subagent overlay on the canonical path.
9. **Upstream.** The v0.22.4 release (2026-10-21) was not inspected, because the brief bars fetching.
10. **Talaria's code.** I checked Talaria's key bindings and version against its code. I did not check its other claims (for example, the settings workspace or the diff viewer) against code. The comparison in section 8 relies on Talaria's docs for those.

## 11. Implications for Talaria's replacement

These are observations from this unit. Design choices belong to the operator.

1. **The wire contract is the hard part.** A replacement client needs the ticketed WebSocket start-up, the JSON-RPC methods and events generated from Pydantic models, and server requests for approvals and clarifications (`tui_gateway/AGENTS.md`, "Transport"). Talaria already uses a WebSocket to a gateway. Whether it can use the same local ticket route is a hypothesis, because that route runs through a private control socket on the same machine.
2. **Two screen modes are a real choice.** Hermes offers the alternate screen and a main-screen mode. Talaria uses Textual's alternate screen, and Textual exposes an inline option (`textual/app.py:2312`). Hypothesis: a main-screen mode would need a different transcript design, because all rows mount there, as Hermes's native mode shows.
3. **Use code, not Hermes's README.** Hermes's `ui-tui/README.md` and the website TUI guide disagree with the code in at least ten places (section 7). Any comparison that cites them should cite the code too.
4. **Hermes's background-agent model is per profile.** One always-running gateway per profile, which the TUI detaches from on exit. Talaria dials a configured fleet. These are different designs. The operator must choose which one a replacement follows.
5. **Subagent control is the closest match to "several agents in the background."** Hermes has steer, stop, pause, and a dock. Talaria's docs describe a Tasks section and a toggle for subagent rows. Whether Talaria can steer or stop subagents is unverified from its docs.

## Appendix: method

- Read-only throughout. No files in either repository were changed. The only writes are the scratch notes and this report, both under `<scratch>/research/`.
- Commands used: `git rev-parse`, `git status --short`, `git log`, `git cat-file -t`, `git rev-list --count`, `git diff --stat`, `ls`, `sed -n`, `rg -n` (ripgrep) searches over the Hermes and Talaria trees, `node -v`, and `cat` on small build and config files.
- Scratch notes: `<scratch>/research/u07-notes.md`.
