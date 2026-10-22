# u13: hands-on drive of Talaria 0.6.5 against live Hermes, compared with Hermes's own terminal UI

Date of run: 2026-10-22, about 00:45 to 01:25 local time. This is a read-only evaluation of two clients attached to the same live Hermes backend on this Mac.

## 0. What was driven, and how

- Talaria: the installed binary `~/.local/bin/talaria`, version 0.6.5. The repository checkout is at commit cafc640 (2026-09-25). I did not verify that the installed binary was built from that commit.
- Hermes backend: the dashboard already running on 127.0.0.1:8765 (process <pid>: `hermes -p default dashboard --port 8765 --open-profile <work-profile>`). The installed Hermes source is at revision 22063fc099.
- Hermes's own terminal UI: `hermes --tui` and `hermes --tui --native`. `hermes --help` lists both flags.
- Which profile answered: all sessions from both clients were written to the <work-profile> profile's database (`~/.hermes/profiles/<profile>/state.db`), and `~/.hermes/active_profile` contains `<work-profile>`. The live default model was the work profile's configured default; the evaluation switched each client to a cheap model (below).
- Model for the evaluation: `minimax-m3.1-flash` via Ollama Cloud (the preferred cheap model; the model picker listed it as `minimax-m3.1-flash`). In Talaria I switched with the F11 picker. In Hermes I used the launch flags `-m minimax-m3.1-flash --provider ollama-cloud`.
- Environment: a tab named `talaria-eval` in the Herdr (a terminal multiplexer, like tmux) workspace `<workspace>`, with two panes, each 85 columns by 44 rows. One pane (`<pane>`) ran Talaria and the other (`<pane>`) ran Hermes. The tab has been closed.
- Throwaway project: `<scratch>/eval-project` with `calc.py` (an `average()` that divides by `len(values) + 1`) and `test_calc.py` (one failing test, `test_average`).
- Isolated Talaria configuration: `TALARIA_CONFIG_DIR=.../scratchpad/talaria-eval-config`. `talaria refresh-credential --from http://127.0.0.1:8765/` wrote a 54 byte mode 0600 `credentials` file into that directory. The real `~/.talaria/credentials` still has its September 16 modification date.
- Evidence: text captures of the panes are in `<scratch>/research/hands-on/` (78 files, named `talaria-NN-*.txt` and `hermes-NN-*.txt`). Captures of the Talaria settings view contained partially masked secrets, so I scrubbed those values from every capture file.
- Ground truth for timings and for what the backend actually received: read-only queries of the <work-profile> `state.db` (message timestamps) and `~/.hermes/profiles/<profile>/logs/agent.log`.

Method limits, stated up front: every number below is a single run (n=1) polled at 0.1 to 0.2 second intervals through `herdr pane read`. The backend is a shared cloud model, and its latency varies. For example, the same kind of trivial question took 19 seconds in one Hermes run and 63 seconds in another (Hermes native mode, section 3 task 2). So timings that depend on the model are comparisons of the clients only where I say so. Panes were 85 columns wide, so layout findings are for a narrow pane unless I say I also checked 168 columns.

## 1. Safety record

| Check | Result |
|---|---|
| `shasum ~/.hermes/config.yaml` before and after | (identical before and after) |
| `shasum ~/.hermes/profiles/<profile>/config.yaml` before and after | (identical before and after) |
| `diff` of the before and after hash files | "Files are identical" (files `hashes-before.txt` and `hashes-after.txt` in the capture folder) |
| Global default model | unchanged: the work profile's file still has its original default model and provider. Talaria's switch printed "(session only — add --global to persist)". |
| Forbidden commands | I did not run `hermes config set`, the global `hermes model` picker, `hermes update`, or anything that stops or restarts Hermes. |
| Files under `~/.hermes` | I only read them (sqlite opened with `-readonly`, log files read). Hermes itself wrote sessions and logs there during normal use, which is unavoidable. |

One slip to disclose: while checking environment variables I ran an `env | grep -i hermes` that printed some `HERMES_*` environment values into my own tool output. Nothing was written to a file or into this report.

## 2. Setup friction found before any task

1. First launch could not connect. After `talaria refresh-credential --from http://127.0.0.1:8765/`, the credential file holds only a `token` line (54 bytes). It does not record the dashboard address. Talaria therefore dialled its built-in default `ws://127.0.0.1:9119/api/ws` and the composer showed "disconnected from the gateway · ws://127.0.0.1:9119/api/ws: [Errno 61] Connec…" (capture `talaria-01-launch.txt`). I fixed it by exporting `TALARIA_GATEWAY_URL=ws://127.0.0.1:8765/api/ws`. The README says the credential file may hold an optional `url` key, but `refresh-credential --from <url>` did not write one. Evidence: `README.md` "Supplying the gateway credential" section (checkout cafc640), and the 54 byte file size.
2. There is no first-run guidance on screen for this case. The user sees a one-line error inside the composer.
3. The Talaria docs describe a `[connections.<id>]` table with `url` and `auth` for naming a dashboard (`docs/configuration.md` lines 91 and 134 to 136). I used the environment variable instead; I did not test the `[connections]` route. Unverified whether it would have been easier.

## 3. Task by task, Talaria beside Hermes's terminal UI

Times marked "(db)" come from message timestamps in the <work-profile> `state.db`. Times marked "(poll)" come from polling the pane.

### Task 1: launch to a usable prompt

| | Talaria | Hermes `--tui` |
|---|---|---|
| First frame with a composer | 1.16 s and 1.24 s in two runs (poll) | 3.69 s (poll) |
| Model name shown | 1.69 s | on the first screen (banner by 4.14 s) |
| Welcome banner or notices complete | 3.36 s | 4.14 s |
| `--resume` of an existing session (history on screen) | 0.56 s first frame, 3.23 s history and model restored | 3.65 s (with `--resume 'Describe project contents'`) |
| `--tui --native` | not applicable | prompt at 3.03 s, banner replaced by a one-line prompt |

Talaria is clearly faster to a usable composer. Part of that advantage is that it attaches to an already running gateway, whereas Hermes starts its own agent process; the numbers do not separate those causes. What is not fast is that at launch Talaria prints a compatibility warning every time (see problem 9).

### Task 2: ask a question about the project ("What does this project contain? Answer in two sentences.")

- Talaria: 37.7 s from submit to final answer (db: user message at 583.1, answer at 620.8, seconds since 1792644000). The answer was correct. The first tool call, `terminal ls -la`, sat for about 26 seconds between the assistant message (586.6) and its result (612.9). I could not tell whether that was the backend's first terminal start-up in that session or something in the client.
- Hermes `--tui`: 19.1 s (db: 1207.3 to 1226.4). It ran five tool calls, shown as a tree with durations: `Terminal("ls -la + 2 commands") (4.3s)`, `Search Files("*.md") (0.9s)`, `Read File("calc.py") (0.0s)`, and so on. Its answer noticed an uncommitted change in the working tree, because by then Talaria's earlier fix had modified `calc.py`.
- Hermes `--tui --native`, a different question ("What is in this directory?"): 63.5 s by screen changes, including a `Search Files("*")` step that took 12.3 s. This shows the backend alone can be three times slower than the 19 s run, so the 37.7 s versus 19.1 s gap is not a safe client comparison.
- Display difference: Talaria shows each tool as one line (`$ Tool/Subagent ⏺ terminal ls -la`, then a second line `⏺ terminal ✓`) with no output and no duration. The long `ls` path is cut with an ellipsis. Hermes shows the tool name, a compact argument summary and the duration.

### Task 3: fix the failing test (observe tool calls, diffs, approvals)

- Talaria: 12.2 s end to end (db: 646.7 to 658.8). It ran pytest, patched `calc.py:9`, and re-ran the tests; `git diff --stat` afterwards showed one changed line, and `pytest` then reported 2 passed. The diff shown in the transcript contained literal escape sequences as text, for example `␛[38;2;255;255;255;48;2;127;23;21m-    return total / (len(values) + 1)␛[0m` (capture `talaria-10b-fix-after.txt`). The colour codes are printed instead of applied. No approval prompt appeared for the patch or for running pytest.
- Hermes: 18.8 s (db: 1259.5 to 1278.3). It showed a clean unified diff with `-` and `+` lines, a Thinking token count, a tool tree with duration, and a Σ token total. It also spent two extra tool calls investigating why the file looked already fixed (I had reset the file with `git checkout` between the two clients), which inflates its time. No approval prompt appeared for the patch or pytest.
- Approval for a dangerous command (extra test, section 4 problem 1): Talaria blocked it and could not ask me. Hermes's own UI showed a proper approval dialog.

### Task 4: interrupt a long answer

| | Talaria | Hermes |
|---|---|---|
| Key | Ctrl+S (documented as the interrupt chord) | Ctrl+C |
| Latency from key to "interrupted" on screen | 0.23 s (poll) | 0.12 s (poll) |
| Ctrl+C | did nothing visible in 45 s while the model kept streaming (changes continued), so a habitual Ctrl+C does not stop a turn; I did not see any hint printed | interrupts |

Both clients also recorded the interruption: Talaria's backend session shows the assistant turn "Your request was not processed. Send it again if you still want me to carry it out." and Hermes printed `Operation interrupted: waiting for model response (9.1s elapsed).`

### Task 5: model picker, session list and resume, slash palette

- Model picker. Talaria (F11 or `/models`): a two level list of 12 configured providers, then a numbered model list with type to filter. Typing "minimax" filtered to four models. Selection gave no immediate feedback: the status bar still read "ChatGPT o…-6.1-sol" for about 3 to 5 seconds, then a transcript line "Model switched: minimax-m3.1-flash ... (session only — add --global to persist)" appeared. Model rows are numbered by a global index (for example 71 to 82 within one provider), which means nothing to the reader. Hermes (`/model`): a three step picker listing about 53 providers including ones marked "(no key)" and "(needs setup)", with a visible "persist: session · ^g toggle" line. Hermes is noisier but states plainly whether the choice is saved. Talaria's `/models` has a third argument form that can set a model as the profile default; I did not use it.
- Session list. Talaria `/sessions`: the picker took 1.54 s and 2.09 s to appear in two warm runs (poll), and about 5 s the first time (estimate from my first look, not timed). Every row carries the text "never observed · no lifecycle poll yet", followed by a title and a raw identifier such as `04027596-1216-424b-bd87-d4713438d2ff · 2 msgs`. Rows come from other projects too (for example "Dispatch Pilot-repo webhook", "Process dispatch record #48"); the list is not scoped to the working directory. Hermes `/resume`: appeared in 0.07 s (poll), shows a `+ new` row, the current session, an age ("today", "4d ago", "35d ago"), a message count and a title, and keyboard hints (Enter switch, Ctrl+D close, Ctrl+N new). The Hermes list also spans projects.
- A naming collision: in Talaria the palette entry `/resume` is a local command meaning "Release the replay clock (F8)". The gateway's own `/resume` is reported as "unavailable — name taken by built-in; use /skill resume". So typing the obvious word to resume a session does something unrelated in a live session.
- Slash palette. Talaria: typing `/` shows an 8 line notice block ("commands: 164 from the gateway · 15 local · 3 unsupported" and a four-command collision message) followed by a window of only 5 command rows at a time. The filter result was visibly stale for about a second (typing `sess` first showed unrelated commands). After I closed pickers with Esc, the old palette rows stayed on screen for the rest of the session, occupying about 13 rows in the 44 row pane (captures `talaria-15-second-prompt-1.txt` and later). Hermes: a floating bordered popup with three line entries, scrollable, including `/bg`, `/btw`, `/stop`, `/agents`.

### Task 6: second prompt while the first is running; background work

- Talaria: a second prompt typed 7 to 9 seconds into a running turn is sent to Hermes as an "OUT-OF-BAND USER MESSAGE" injected into the running turn (the database stores it as a user message starting with that tag). The agent answered "Four" about 2 seconds later (1517.9 to 1519.9) while it moved the long loop to a background process. The screen shows the new `> You  Also: what is 2+2?` line immediately but no label saying it was queued or injected.
- Hermes `--tui`: the second prompt was shown under a heading "queued (1)" with the text, and ran after the first turn finished. The answer "Four" appeared once the 30 second loop was done. It is explicit and predictable.
- Subagents in the background, in both clients, work: I sent "What is 3+3?" while two delegated subagents were still running in Talaria and got "Six" in 4.2 s (db: 991.6 to 995.8). The subagents then finished and reported back as an "ASYNC DELEGATION BATCH COMPLETE" message. So you can keep typing while delegated work continues. This is a backend feature that Talaria exposes correctly.
- `/bg` (run a prompt in a separate background session): in Talaria it printed "Background task #1 started ... Task ID: bg_011500_169bf2 ... results will appear when done." The Hermes agent log shows the task started at 01:15:01 and finished at 01:15:31. Talaria never showed a result: I checked 135 seconds later, after pressing F5 to follow, and in the inspector. The same command in Hermes's UI printed the result `[bg bg_44b9bc] Counted 1 through 5 ... BGDONE2` after 51.8 s. Why Talaria missed it is unverified; the checkout does contain a decoder entry for `background.complete` (`talaria/domain/decode.py:41`), so the event name is known to it.

### Task 7: delegate a subtask to a subagent; visibility and steering

- Talaria: after a `delegate_task` call, a status line reads "sub-agents: 2 active · 0 finished (collapsed — ctrl+g/F2 to expand)". Expanded, each row shows a state, an elapsed timer (for example "running 15.4s") and the goal text wrapped over several lines. The status bar counter reads "task 0/2". The inspector overlay (Ctrl+O) shows a "delegate_task · 2 tasks" entry and, in the 85 column pane, wraps its text into a 34 column strip. The source (`talaria/ui/agents.py` lines 13 and 83) says Enter on a focused running row sends `subagent.interrupt`. I could not reach a focused row with Tab in the pane; Tab only cycled the composer's focus state, and I could not see a focus highlight in a text capture. So steering is "interrupt only, and I could not discover how". There is no tail of a child's output, no detail view and no way to send it a message.
- Hermes `/agents`: a spawn tree with a header "d1 · 2 agents · 2 tools · caps d1/10", a timeline bar per child, and keys "Enter/t tail · d detail · e steer · x stop". The default view also lists "2 live agents · Ctrl+T expand" under the transcript. Hermes is clearly ahead on subagent observation and control.

### Task 8: configuration and settings views

- Talaria `/config` (Ctrl+K showed nothing within 2.5 s; I did not wait longer for it): the settings workspace appeared roughly 7 to 9 seconds after `/config` was submitted. It rendered as a run-on block of text: the heading "Hermes profile · Host · Environment" ran straight into environment variable names and values, wrapped in two columns of free flowing text, with "Save / Discard / Stop" at the bottom. It looked the same at 85 and at 168 columns (captures `talaria-19c-config.txt` and `talaria-21-config-wide.txt`). I could not navigate sections with Tab. It listed about 60 environment variables, including several dozen `HERMES_PROFILE_API_KEY_*` entries, with each secret shown masked to its first four and last four characters (for example `sk-c…49c3` style); I scrubbed these from the saved captures. After pressing Esc, the first line of that text stayed inside the composer's helper row and the composer showed "caret elsewhere" (capture `talaria-22-after-unzoom.txt`).
- Hermes `--tui`: `/config` replied "/config is not available on the shared gateway yet". So Hermes's terminal UI offers no configuration screen on this gateway; configuration there is through the CLI and dashboard. The comparison therefore says Talaria attempts something Hermes's UI does not, but the attempt, as it renders today, is not usable.

### Task 9: resize

- Talaria: zooming the pane from 85 to 168 columns reflowed in a single frame (0.01 s in my poll). The status bar became richer at the wider width ("cwd: <project path, truncated>│git: master│agent: minimax-….1-flash│context: 75k/? ?%│tasks: 0/0│[ok] connected│v0.6.5").
- Hermes: reflowed to 168 columns with the status line gaining a context bar and rates ("4% │ 8m 9s │ ✓ 33s │ ◎ 98% │ ◷ 4.1s │ ↑ 53 t/s │ voice off │ 2 sessions").
- In both clients the context shows differently: Talaria reports "ctx ?%" and "context: 75k/? ?%" (it does not know this model's window), while Hermes reports "33k/1M".

### Task 10: quit and resume

- Talaria: Ctrl+Q returned to the shell within the 1.5 second wait. `talaria --resume` restored the history and the minimax model (3.23 s). Because the gateway keeps the session, "/quit" is described as "Leave Talaria; the gateway session keeps running".
- Hermes: `/quit` prints a summary (session id, 39 messages, 511,167 tokens). `hermes --tui --resume latest` was refused: "Error: This chat is open in another Hermes window/terminal, and attaching this terminal to it is not available in this build. ... Details: session 20261022_004831_cd8d94 opened by tui 18m ago." The session was held open by my Talaria window, so a second client cannot attach to a session that another client has open. Resuming by title (`--resume 'Describe project contents'`) worked in 3.65 s.

## 4. Side-by-side friction table

| Area | Talaria 0.6.5 | Hermes `--tui` (revision 22063fc099) |
|---|---|---|
| Launch to usable prompt | 1.2 s | 3.0 to 3.7 s |
| Connection setup | needs env var or hand edit; credential file lacks the address; default port 9119 wrong for this dashboard | none (own process) |
| Question about project | 37.7 s (db), one tool call stalled 26 s | 19.1 s (db) |
| Fix failing test | 12.2 s; diff shows raw escape codes | 18.8 s; clean diff |
| Approval for dangerous command | cannot ask; command blocked by Hermes | dialog: Allow once / session / always / Deny |
| Interrupt | Ctrl+S in 0.23 s; Ctrl+C silently ignored | Ctrl+C in 0.12 s |
| Model picker | 12 providers; delayed feedback; confusing global numbering | 53 providers incl. unusable ones; states "persist: session" |
| Session list | 1.5 to 2.1 s warm; "never observed" noise; raw ids | 0.07 s; age, count, title |
| Slash palette | 5 rows at a time under an 8 line notice; stale rows stay on screen | floating popup, 3 line entries |
| Second prompt while busy | injected mid-turn, no label, answered at once | "queued (1)" shown, runs afterwards |
| Background prompt `/bg` | started; result never shown | result shown at 51.8 s |
| Subagent visibility | list with timers; no tail, detail or steer | spawn tree, timeline, tail, detail, steer, stop |
| Settings view | slow, unstructured text, partial secrets, leaves residue | `/config` unavailable on this gateway |
| Resize | instant | instant |
| Quit and resume | works, history in 3.2 s | works, 3.65 s; cannot attach to a session another client holds |
| Context window shown | "?" | 1M |

## 5. Talaria's top ten UX problems, ranked by severity

1. **Talaria cannot answer Hermes's approval or question prompts, so dangerous commands are blocked.** I asked the agent to run `rm -rf .pytest_cache` inside the throwaway project. The agent reported "the approval prompt was withdrawn before it reached you" and the backend recorded: "BLOCKED: Command approval was withdrawn before the user answered (the attached client cannot answer approval requests (update the Hermes app))". The directory still existed afterwards. The same request in Hermes's UI showed the approval dialog. The mechanism, verified in source: Hermes only sends server-to-client requests to a client that sent `client.capabilities {server_requests: true}` (`tui_gateway/server_requests.py` lines 31 to 35 and 137 to 142, and `tui_gateway/methods_voice.py` line 450, Hermes 22063fc099). A search of the Talaria checkout (cafc640) for `client.capabilities` and `server_requests` found nothing in `talaria/`, README or docs. Hermes added this behaviour in commit f9d178f78e on 2026-09-17, three days after the Hermes revision Talaria last pinned (3236f4405, 2026-09-14). The link between the missing advertisement and this block is strongly supported but I did not instrument the connection to prove it. Why it ranks first: any ordinary destructive or networked command, and any clarifying question from the agent, stops working, which defeats the "Claude Code for Hermes" goal.
2. **Background work started with `/bg` never reports back.** The backend finished the task (agent.log, 01:15:31) but Talaria showed nothing after 135 seconds, after following and checking the inspector. Hermes's own UI showed the result. Always-on background agents are a core part of the target product.
3. **The settings workspace is unusable as rendered.** It opens slowly (about 7 to 9 s), appears as run-on text with no visible fields or section navigation, displays masked secrets with four leading and four trailing characters, and leaves residue and a broken focus state after closing. Evidence: captures `talaria-19c-config.txt`, `talaria-21-config-wide.txt`, `talaria-22-after-unzoom.txt`. The operator wants "full and very user-friendly configuration of the Hermes backend from the client"; this is the opposite today. (Caveat: I saw this on one terminal through Herdr; I did not check a different terminal emulator.)
4. **Connecting to a specific Hermes profile or endpoint is manual and mislabelled.** `refresh-credential --from` does not store the address, so the default port 9119 is dialled; every one of the first 11 profile rows I saw in the picker shows "[no endpoint configured]" (capture `talaria-28-profiles.txt`; I did not scroll to the rest of the 26 profile directories); the inspector and picker label the connection "default" (picker row: "default — muse-spark-1.3-contributor") while the live model and database were the <work-profile> profile's (its configured default model). A person who wants the <work-profile> profile's backend has to know to export an address or edit TOML by hand.
5. **Tool and diff presentation lose or corrupt information.** Diffs show escape codes as text; tools show a bare name and a tick with no output or duration; the assistant's reasoning is rendered after the tool lines it relates to, and reasoning text runs together ("Let me run pytest.The bug: ..."); a code block of three review findings was cut off at the pane edge without an indicator (capture `talaria-16b-after-follow.txt`). Hermes shows durations, token counts and clean diffs for the same tasks.
6. **Focus handling is fragile and the interface leaves stale pieces on screen.** Typing is silently discarded when the caret is "elsewhere" (my "rm -rf" prompt was lost once with only a composer helper line saying "typing is paused while the prompts region holds the focus"); that helper line stayed even when typing worked again; Tab cycles through states without a visible indication; closed palettes and pickers leave rows behind; scrolling up with Page Up silently stops following new output until F5, and I saw no indicator that new output was waiting (I did not look for one beyond the scroll bar glyph). Four of seven rapid Backspace presses were dropped once (cause unverified).
7. **The result of sending a prompt while busy is invisible.** Talaria shows the prompt as an ordinary message, though Hermes injected it into the running turn as an out-of-band message. Hermes's UI says "queued (1)". An operator cannot tell whether their second prompt will interrupt, steer or wait.
8. **Subagent observation and control are thin.** Rows with a timer, but no output tail, no detail, no steering message, no stop key I could reach, and an inspector squeezed into a 34 column strip at 85 columns. Hermes `/agents` provides a spawn tree, timeline, tail, detail, steer and stop. The target product's multi-agent workflows depend on this.
9. **Version drift noise on every launch and in normal use.** Every start prints "gateway compatibility check found a gap: commands.catalog: DRIFTED — ... unrecorded key 'plugin_discovery_error' ... 'plugin_commands'", and the composer shows "gateway compatibility: 1 blocking, 13 unverified at runtime". During use, lines such as "Error unknown event type: skills.changed" and "cron.changed" appeared in the transcript. The status bar shows "ctx ?%" because the model's context window is unknown to it. Talaria's last pin (Hermes 3236f4405, 2026-09-14) is behind the running Hermes (22063fc099, 2026-10-17, v0.22.2 canary), so problem 1 and this problem are two faces of the same drift. The installed Hermes is also behind the newest release v0.22.4 (2026-10-21), per the task statement; I did not independently check that.
10. **Session discovery and the command palette are slow and noisy.** The session picker takes 1.5 to 2.1 s (Hermes: 0.07 s) and prints "never observed · no lifecycle poll yet" on every row beside raw identifiers; it is not scoped to the project; `/resume` means "release the replay clock"; the palette shows five rows at a time under an 8 line notice that lists name collisions every time.

## 6. What worked well (so the rewrite does not lose it)

- Fast to a usable composer (1.2 s) and fast resume (3.2 s) with the model preserved.
- Fast, documented interrupt (Ctrl+S, 0.23 s).
- Instant resize and a status bar that adapts to width.
- The backend features are all reachable: parallel subagents (verified concurrent: both terminals started within 4 s of each other), prompts accepted while delegated work runs (3+3 answered in 4.2 s), session-only model switching with a clear message, and a profile picker that lists the profiles by name and model.
- The model picker lists only configured providers (12 against Hermes's 53).

## 7. Open questions and things I did not verify

- Whether Talaria's `[connections.<id>]` route would have avoided the environment variable (not tested).
- Why `/bg` results never rendered, and whether the event arrives at all (the log proves the backend finished; I did not record the websocket frames).
- Whether the 26 second first tool call in Talaria's session was client, backend or first-use start-up (unverified; the same Hermes call took 4.3 s in its own session, but sessions differ).
- Whether Ctrl+K would have opened settings if I had waited longer.
- Keyboard route to per-row subagent interrupt (the source says Enter on a focused row; I could not focus one).
- Rendering in a different terminal emulator, or with a different Textual theme, which could change findings 3, 5 and 6.
- Whether Talaria's installed binary matches checkout cafc640.
- `hermes --tui --native` was only used for one question and one `/quit`; I did not test its picker or subagent views.
- The model was a free cheap model; behaviour with the work profile's configured default model was not tested.

## 8. Reproduction commands (no secrets)

```bash
export TALARIA_CONFIG_DIR=<scratch>/talaria-eval-config
talaria refresh-credential --from http://127.0.0.1:8765/   # writes token only
export TALARIA_GATEWAY_URL=ws://127.0.0.1:8765/api/ws
talaria                      # F11 model picker, /sessions, ctrl+g agents, /config
hermes --tui -m minimax-m3.1-flash --provider ollama-cloud
# approval gap: ask the agent to "run exactly: rm -rf .pytest_cache" in the throwaway project
sqlite3 -readonly ~/.hermes/profiles/<profile>/state.db \
  "select role,substr(content,1,200) from messages where content like 'BLOCKED:%' or content like '%approval was withdrawn%'"
```
