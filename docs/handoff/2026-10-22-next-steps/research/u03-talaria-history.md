# u03 — Talaria's recorded history: what a rewrite must keep, avoid, and why

This report reads the history of Talaria, an experimental third-party terminal client for Hermes Agent, kept at `<talaria>`. It supports a handoff document on rebuilding Talaria as "Claude Code for Hermes". All Talaria citations are at commit `cafc640` (origin/main, confirmed with `git fetch` on 2026-10-22; the last content commit is `101a951`, 2026-09-16, and `cafc640` is the merge of PR #187 on 2026-09-25). Hermes citations name their revision. The installed Hermes checkout is `22063fc09` (2026-10-17).

Labels used below:
- **verified**: I read it from the named file or command output in this session.
- **recorded**: Talaria's own documents state it; I checked the quote, not the underlying fact.
- **hypothesis**: my inference, not checked.

Acronyms used throughout:
- **ADR**: architecture decision record, under `platform-specs/04-architecture/adrs/`.
- **RR-nn**: a numbered rule in the Hermes reconciliation-rule catalogue, `docs/analysis/2026-08-02-hermes-reconciliation-rules.md`.
- **KTD**: "key technical decision", a numbered decision inside a Talaria plan.
- **MoA**: Mixture of Agents.
- **PTY**: pseudo-terminal.
- **Herdr (a terminal multiplexer, like tmux)**: the operator's tool for running agent sessions in panes.

---

## 1. Timeline, 2026-08-01 to 2026-09-16, and where the effort went

### 1.1 What each release added (from `CHANGELOG.md` and `docs/releases/`)

| Release | Date | What it added (CHANGELOG wording condensed) |
|---|---|---|
| (bootstrap) | 08-01 to 08-02 | Project direction doc. A TypeScript recorder and redaction module. Four competing language and framework analyses, then ADRs 0001 to 0004 (08-02) and ADR-0005 (08-03, after the Textual validation gate). |
| v0.1.0 | 08-08 | "Talaria dials a Hermes gateway it did not launch, runs a session in a terminal interface, and can record and replay everything it saw" (`CHANGELOG.md:490`). Also the model picker, the profile picker, slash commands and paste collapse. It shipped behind a 91 KB daily-driver verdict document. |
| v0.2.0 | 08-10 | "The interface becomes answerable, `--resume` means what it says, and the transcript renders real markdown" (`CHANGELOG.md:434`): approval and prompt cards, the `/sessions` picker, and block markdown behind ADR-0006. |
| v0.3.0 | 08-13 | "Talaria confirms what it just did" (`CHANGELOG.md:345`): keypress feedback, `Up` history, a `/` command palette, prompt cards that take the caret, and the function-key row re-decided. |
| v0.4.0 | 08-19 | "Talaria becomes a fleet client. One running instance now dials every configured profile … one queue that answers … what is waiting on me?" (`CHANGELOG.md:237-240`). |
| v0.5.0 | 09-01 | Themes (four built-ins, 58 semantic tokens), a Visual Studio Code theme importer, a bottom status bar, a right inspector, a read-only diff viewer, and real-terminal (PTY) acceptance. Its record says "BLOCKED … 41 pass, 1 blocked" (`CHANGELOG.md:164-171`). |
| v0.6.0 | 09-04 (never tagged) | "answering ten manual-test findings" (`CHANGELOG.md:132-136`): gateway command compatibility, Herdr-safe keys (`Ctrl+O`, `Ctrl+S`), single-Enter picker, diagnostics moved to the inspector, a Homebrew theme, marketplace theme import, a multirow script-driven status bar, and credential hardening. `CHANGELOG.md:98-101` says it "landed on `main` but was never tagged or published". |
| v0.6.1 | 09-06 (tag dated 09-09) | File and image attachments, MoA presentation, configuration views, `session.usage` handling, the launch directory adopted as the session's working directory, and the theme lifecycle. A 23-row live-test gate (`docs/analysis/2026-09-05-v0-6-1-daily-driver-verdict.md`). |
| v0.6.2 to v0.6.5 | all 09-16 | Four releases in one day, all "configuration residuals": Talaria-owned settings and surgical writes (0.6.2); live `/config` target switching, one-shot reveal and Start/Stop (0.6.3); schema remount after a target switch (0.6.4); typed schema load outcomes (0.6.5). Each has its own one- to four-row gate document. |

Tags (verified, `git tag --sort=creatordate`): v0.1.0-rc1 and v0.1.0 on 08-08; v0.2.0 08-10; v0.3.0 08-13; v0.4.0 08-19; v0.5.0 09-01; v0.6.1 09-09; v0.6.2 through v0.6.5 all on 09-16. There is no v0.6.0 tag.

Commits per day (verified, `git log --format=%ad --date=short | uniq -c`):
- The work came in bursts.
- 2026-08-31 alone had 145 commits, and 09-16 had 99.
- No commits landed from 08-20 to 08-29.

### 1.2 Effort split: product code against process and acceptance machinery

Method (verified): I summed lines added in every non-merge commit, grouped by path. The script is at `<scratch>/research/churn.py`. There are 504 non-merge commits out of 791 in total, and 150 of 154 pull requests were merged. Every issue and pull request is authored by the operator's account, apart from Dependabot (verified with `gh`).

| Path group | Lines added | Share | Commits touching it |
|---|---:|---:|---:|
| `tests/` | 95,859 | 29.4% | 255 |
| `docs/acceptance/` (receipts, captures, manifests) | 75,145 | 23.1% | 70 |
| **`talaria/` (the product)** | **66,399** | **20.4%** | 213 |
| `docs/` process (plans, reviews, code-reviews, work-sessions, brainstorms, ideation, evidence) | 43,762 | 13.4% | 101 |
| `scripts/acceptance/` (receipt validators, PTY driver, release-record tests) | 16,729 | 5.1% | 64 |
| `docs/` other (analysis, user docs, releases, ADRs) | 12,903 | 4.0% | 135 |
| `docs/engineering-journal/` | 8,841 | 2.7% | 177 |
| TypeScript `src/` | 3,372 | 1.0% | 4 |

**About 44% of everything written was process and acceptance machinery.** That is acceptance records, the acceptance scripts, process documents and the journal: 144,477 of about 325,870 lines added. The product package was 20%. Tests were 29%, and part of that is release-record tests: `tests/docs/test_v0*_release_record.py` and its siblings come to 1,595 lines.

The same split by release window (author date, so approximate: branch commits can fall outside their release's window):

| Window | Non-merge commits | Largest shares of lines added |
|---|---:|---|
| v0.1.0 (08-01 to 08-08) | 105 | tests 38%, product 27%, process docs 13% |
| v0.2.0 (08-09 to 08-10) | 63 | tests 40%, product 32%, process docs 18% |
| v0.3.0 (08-11 to 08-13) | 40 | **process docs 42%**, tests 35%, product 15% |
| v0.4.0 (08-14 to 08-19) | 15 | tests 44%, product 37% |
| v0.5.0 (08-20 to 09-01) | 99 | **docs/acceptance 48%, process docs 24%**, tests 9%, **product 8%** |
| v0.6.0 (09-02 to 09-04) | 29 | tests 49%, product 28% |
| v0.6.1 (09-05 to 09-09) | 84 | **docs/acceptance 52%**, tests 25%, scripts/acceptance 10%, **product 9%** |
| v0.6.2 to v0.6.5 (09-10 to 09-30) | 69 | tests 39%, product 32%, scripts/acceptance 14%, docs/acceptance 12% |

What the current tree holds (verified, `git ls-files | xargs cat | wc -l`):

| Path | Files | Lines |
|---|---:|---:|
| `talaria/` | 87 | 61,002 |
| `tests/` | 169 | 92,988 |
| `scripts/acceptance/` | 23 | 15,752 |
| `docs/` | 2,406 | 571,901 |
| …of which `docs/acceptance/` | 2,234 | 510,788 |

`docs/acceptance/` is 1,004 JSON files, 582 PNG files, 364 ANSI captures, 199 text files and 56 JSONL files. 1,473 of them belong to v0.5.0 and 693 to v0.6.1. Line counts overstate the prose here because many of these are machine captures, but every one of them is acceptance output.

Commit subject scopes back this up (verified, from conventional-commit scopes):
- `acceptance` is the most common scope, with 89 commits.
- `ui` comes next with 32, then `release` 27, `cfg` 19, `journal` 16, `gate` 15 and `plan` 13.
- Only 2 commits are `perf` and only 2 are `refactor`.

---

## 2. The operator's own complaints and wishes

These are quoted verbatim where Talaria recorded them verbatim. The main source is `docs/analysis/2026-08-10-v0-2-hands-on-notes.md`, whose template says "the operator's note verbatim" (line 128).

**Readability and visual hierarchy**
- "It's difficult to read and differentiate what I am looking at." (`docs/brainstorms/2026-08-08-block-markdown-and-transcript-differentiation-requirements.md:18-19`)
- "the overal test box is a bit busy, multiple colors, etc... its sort of hard to follow, needs more spacing, formatting etc... Tables are great, code is very readable … Just some parts are hard to differentiate. I only noticed when pulling up other sessions with more real content." (hands-on notes, Note 11, around line 318)
- On the `caret:` status row: "I as the user had no idea why it was there or even what caret corresponded to. to be frank, I think it might just be prudent for the focus to always go back to the input box." (Note 4)
- "the code started to be duplicated after the table" (Note 14)

**Behave like the tools I already use**
- "I expected it to work like the Hermes TUI. `/models` would open up a dialog picker of some sort, not just a list I now need to pick a number." (`docs/engineering-journal/QUEUED.md:335-336`, repeated at `DECISIONS.md:1733-1735`)
- "the input box will need some work, helpful things like: up arrow to go to previous commands, type "/" allows scrolling/filtering on all possible slash commands like most other TUI's like this." (Note 12; v0.3 delivered both)
- On F1: "I tried F1, and it actually appears to be a hotkey for macos usage … This is what claude code does: [Jump to bottom (click) ↓] … That might be the better way to go about this … I would like to see the "ask" dialogue, that is more important than focus jump." (Note 8)
- "after making the selection and then re-opening the modal with `/models` the current model isn't what is currently selected in the model list" (`LEARNINGS.md:2147`, author line)

**Terminal-native behavior**
- "I don't seem to be able to copy/past text. Doesn't seem to allow the terminal being used to "come through" … this is a herdr session, I should be able to to just select and copy. That doesn't work. ctrl+c does work though" (Note 2).
  - The notes record that the failure was confined to the Talaria pane.
  - It was never fixed. Unit A3 in v0.3 was "blocked on that capture" (`docs/plans/2026-08-11-v0-3-decision-log.md:688-705`).
  - I found no later record of a resolution (verified by `grep` over CHANGELOG, journal and docs).
- "mouse is a bit off … note where the highlight is... is a number lines up from where I clicked." (Note 13)

**Approvals and asks**
- On the approval card: "1. its pretty cramped and needs some UX consideration. 2. I can only select with the mouse, the cursor is not focused inside the approval card 3. … have a "notes" option. Such that I can add notes to the choice, as I might agree with the choice but with caveats." (Note 9)
  - Keyboard access was fixed in v0.3. The QUEUED entry for it is closed on 08-16 (`QUEUED.md:110`).
  - The "notes" option appears nowhere in QUEUED (verified by `grep`).

**Things that worked**
- "in general the session switching worked pretty well actually." (Note 10)
- Tables and code blocks were praised twice (Notes 11 and 17).

**Original goals** (the project direction, `docs/analysis/2026-08-01-hermes-tui-project-direction.md:11-21`):
- "less flicker and more stable rendering"
- "a clearer status bar and run-state model"
- "a separate, useful view of active sub-agents"
- "configuration views closer to a desktop application while remaining terminal-native"
- "workflows that feel closer to modern coding-agent tools"
- "first-class access to … MoA, delegation, skills, tools, and Kanban"

**Rulings recorded as the operator's**
- "the operator declined a smaller first cut" for block markdown (the brainstorm, Key Decisions).
- Multi-profile fleet now, not staged ("Operator ruling (multi-profile now)", `DECISIONS.md:873-876`).
- "the operator's rule that no live test is waived is enforced by the machine" (v0.6.1 verdict, "How a row clears").
- The operator overrode an unsatisfiable review-receipt gate on 08-02 ("v0.1 proceeds without further independent-review ceremony", `DECISIONS.md:2448-2460`). The learning recorded there: "A process gate whose verifier was never implemented becomes a permanent block that looks like diligence."
- The directory complaint behind v0.6.1 item 12: "the agent reporting one directory while Talaria displayed another" (`DECISIONS.md:7-30`). The operator chose option (c), adopt the launch directory.

**Lists that exist only as paraphrase**
- The ten v0.6.0 manual-test findings F1 to F10 are restated as a ledger in issue #118's body.
- The eighteen v0.6.1 interface-feedback items are referenced in issue #139 ("the operator's 2026-09-04 interface feedback (18 numbered items)").
- Neither issue body is verbatim operator text. They are agent-written restatements.

**Not in the recorded history:** "slow" and "not clean" (from this task's brief). I found no recorded performance complaint from the operator (verified, `grep -i 'slow|sluggish|lag'` over the journal, analysis and release notes). Section 6 says what the record does show about speed.

---

## 3. Hard-won technical lessons a rewrite must keep

Each rule gives its source. "Pin" means the Hermes revision the lesson was measured at. Lessons measured at an older Hermes need re-checking against `22063fc09`; I re-checked two of them in this session and say so.

### 3.1 Hermes protocol and gateway behavior

1. **Read the server's emit sites, not one client's handler list.** The Hermes TUI handled 2 of the 4 `.expire` events the gateway emits. (`LEARNINGS.md:3270`; RR-27; pin `7f4d15515`)
2. **Approvals were keyless, and now carry a request id.**
   - At `7f4d15515`, `approval.request` had no `request_id` and resolved by session key (RR-28).
   - v0.5.0 found that "Hermes 0.21.0 assigns request identifiers to live approvals" (`docs/releases/v0.5.0.md:24-28`).
   - At `22063fc09`, `approval.respond` passes `request_id=params.get("request_id")` and falls back to the durable id (`tui_gateway/methods_prompt.py:1246-1260`, **verified**).
   - Key approvals and clarifications by `session_id`, `prompt_id` and `execution_generation`, the three fields the request event carries. Every response echoes all three. Do not key them by `request_id`, and treat RR-28 as obsolete.
3. **A prompt expiry or answer applies only to the prompt whose `session_id`, `prompt_id` and `execution_generation` all match.** Refuse a respond that matches no outstanding prompt before it reaches the socket. (RR-26, RR-29)
4. **`thinking.delta` is the spinner/status line; `reasoning.delta` is the model's reasoning.** Routing both to one buffer discarded whole reasoning blocks. (`LEARNINGS.md:2403`; `DECISIONS.md:2087`)
5. **Append streaming deltas, and never assign `payload.rendered`.**
   - `rendered` is an incremental Rich-ANSI fragment.
   - Final text prefers raw `text`.
   - Strip already-shown segments so replies do not open twice.
   - (RR-15, RR-16, RR-17, RR-18, RR-19)
6. **Turn outcome comes from `message.complete.status` (`complete`, `error` or `interrupted`), never from the event's arrival.** Synthesized error text is never assistant prose. (`LEARNINGS.md:5-21`)
7. **Interrupt is a latch.** All later writes for that turn are suppressed, a trace is always left, a late `message.complete` cannot un-cancel, and only a new `message.start` clears it. (RR-11 to RR-14)
8. **Sub-agent rows are keyed by server `subagent_id`.**
   - Terminal states are never overwritten by stale starts.
   - Late events never create rows.
   - Partial payloads never null fields.
   - Rows are ordered by spawn position.
   - (RR-03, RR-04, RR-06, RR-07, RR-08, RR-09, RR-34)
9. **Merge usage field-wise and accept key aliases** (`input`/`output`, `prompt`/`completion`). Never default a missing field to an observed zero. (`LEARNINGS.md:489-510`; RR-23)
10. **Unknown event types must be surfaced, but once per connection, not once per frame.** One ordinary turn carried 26 `platforms.changed` events, each drawn as a red row. (`LEARNINGS.md:1845`, corollary; RR-24; hands-on Note 17 table)
11. **A session has two ids.**
    - Resume by the durable `stored_session_id`; the runtime id is refused with code 4007 after the process is gone.
    - `session.most_recent` is global, so any webhook or cron session can "win" `--resume`.
    - (`LEARNINGS.md:1891-1908`; pin around 08-09)
12. **Session events go only to the transport that owns the session.**
    - Create, resume, activate and `prompt.submit` all rebind ownership: "attach is steal".
    - The displaced client gets silence, with no close or notice.
    - `sessions.changed` is the only broadcast, with a 2-second floor.
    - (`docs/analysis/2026-08-17-v0-4-topology-verification.md:93-114, 243-270`; pin `91a545ab1`)
    - Re-verify this against the current "session authority" design (see 4.4).
13. **A late frame from a replaced connection updates history, never liveness.** Use a per-connection generation counter, because JSON-RPC ids and epochs restart on reconnect. (`LEARNINGS.md:1502`; `LEARNINGS.md:1396`)
14. **The gateway's `waiting` status does not cover approvals,** because approvals live in `tools/approval.py` queues. An approval-blocked session reports `working`. (`LEARNINGS.md:1291`; pin `91a545ab1`)
15. **`command.dispatch` refuses most of the catalogue with code 4018.** Ordinary commands go through `slash.exec`. Read refusal paths, not only success bodies. (`LEARNINGS.md:2638`; `DECISIONS.md:2254`)
16. **`/model` via `slash.exec` changes the running session only.** `GET /api/model/options` reports the profile default for new sessions. Name both and show both. (`LEARNINGS.md:2147`; `DECISIONS.md:1781`)
17. **"The gateway" was three processes: the terminal gateway, `GatewayRunner`, and the dashboard.**
    - The WebSocket at `/api/ws` lives in the dashboard.
    - The terminal gateway publishes no capability endpoint, so probe for capabilities.
    - Kanban reaches it as notifications only, not as a queryable board.
    - (`LEARNINGS.md:3464-3492`; pin `7f4d15515`)
18. **A running Python process executes the revision it imported at start, not the checkout.**
    - Four dashboards nine days old refused the handshake with HTTP 500 (`QUEUED.md:1386`).
    - The checkout `7095e23eb` differed from the serving `91a545ab1` by 19 methods (`LEARNINGS.md:1464`).
    - Compare process start time with the revision before citing source.
19. **A stub built from your reading of the source agrees with your reading.** Talaria "had never connected to a Hermes gateway" when its first verdict was written; the stub "cannot … disagree with the transcription". (`docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md:20-45`; `LEARNINGS.md:2109`) Use live recordings as fixtures.
20. **Top-level shape comparison misses nested drift.** `compare_shape` reads a restructured payload as present. (`QUEUED.md:1126`)
21. **The profiles listing has no endpoint field** (`GET /api/profiles`, 37 rows, 14 keys, no URL). Talaria had to keep its own `[profiles.endpoints]` map. (`DECISIONS.md:2910`; measured 08-06)
22. **The clarify bridge sends `multi_select: true` only when it is true.** `approval.respond` takes an `all` flag the Hermes TUI never sends. (`QUEUED.md:1225, 1234`)
23. **Decline is an empty answer on this wire.** Without it, a sudo prompt sits out its full 120 seconds. (`QUEUED.md:756`)

### 3.2 Terminal knowledge

24. **macOS eats `F1` and `F2` before the application sees them, and a program cannot detect that.** Never make a function key the primary binding. (`DECISIONS.md:1018-1036`; `docs/terminal-ui.md:238-239`)
25. **Herdr captures `Ctrl+B` when nested.**
    - The inspector moved to `Ctrl+O`.
    - Every global chord must be configurable because "no single default survives every terminal multiplexer".
    - (`docs/terminal-ui.md:64-65, 256-257`; `docs/configuration.md:122`)
26. **`Ctrl+S` as turn-cancel collides with XON/XOFF flow control.**
    - Talaria chose it with "no silent substitution" and a `[keys]` override.
    - Live cross-context validation was never completed (`docs/releases/v0.6.0.md:25-28`; v0.6.0 verdict row 4).
    - Treat it as unproven.
27. **Keep `Ctrl+C` out of interrupt.** Users press it out of habit to copy. Label cancel-turn beside quit so the two cannot be confused. (`docs/terminal-ui.md:258-261`)
28. **Mouse reporting breaks the terminal's own select-to-copy.**
    - Recorded as a hypothesis: Textual turns mouse reporting on by default (hands-on Notes 2 and 13, follow-ups).
    - Never resolved.
    - A rewrite should decide mouse capture deliberately and keep native selection working.
29. **`PageUp` and `Home` only stopped follow mode and never scrolled.** Reading back during a stream was mouse-only. (`QUEUED.md:565`, open P1)
30. **Live terminals find what headless gates cannot.** "Two hours in a real terminal found seven defects that 24-of-24, six review rounds and 1,700 tests could not." (`LEARNINGS.md:1845-1853`) Drive every release in a real emulator and a real multiplexer.
31. **Negotiate synchronized output (mode 2026); do not assume it.**
    - Bubble Tea v2 negotiates it.
    - The inspected ratatui core and crossterm backend do not, so an application on them owns that path.
    - (`docs/analysis/2026-08-02-language-and-tui-framework-analysis-final.md:143-147, 186-189`)
32. **Bound streaming transcript work by measured cost, not by line count.**
    - Textual's `Markdown` widget mounts one widget per table cell: one 601-column table came to 1,204 descendants.
    - An open fence's reparse window grows with the block.
    - A streaming table crosses 50 ms per apply at about 340 rows.
    - (ADR-0006 `:16-40, 148-160`; `QUEUED.md:1012`)
33. **Render untrusted text literally.**
    - Bypass markup parsers.
    - Replace control, bidirectional-override and zero-width characters with visible stand-ins.
    - Never interpret ANSI from status scripts.
    - (ADR-0005 decision 7; `LEARNINGS.md:2761, 2803`; `LEARNINGS.md:2315`, the status-line disagreement with Qwen Code)
34. **Focus rules matter more than key maps.**
    - A new prompt takes the caret only when the composer is empty.
    - When a control is taken away, the caret returns to the composer.
    - Dialogs that must be operated need modal focus.
    - (`DECISIONS.md:2848, 2105, 1709`)

The following are specific to Textual, the terminal framework (`textual`). They are relevant only if the rewrite keeps it:
- A `Paste` event inserts twice (`LEARNINGS.md:3240`).
- Overriding a handler does not replace the one it overrides (`:2668`).
- Assigning `self._closing` hangs Pilot teardown (`:3226`).
- Timers tick into teardown (`:432`, issue #158).
- `TextArea` backspace is async (`:1621`).

### 3.3 Credentials and redaction

35. **The WebSocket credential is a URL query parameter, the only form Hermes reads.** HTTP uses `Authorization: Bearer` plus `X-Hermes-Session-Token`; Hermes calls Bearer "legacy". (`DECISIONS.md:1841-1860`; pin `863e31318`)
36. **The four blocking bridges carry plaintext secrets on ordinary client-to-server frames.**
    - `sudo.respond` carries `password`.
    - `secret.respond` carries `value`.
    - `clarify.respond` carries `answer`.
    - `terminal.read` is the fourth bridge.
    - Build the redactor before anything writes frames.
    - (`LEARNINGS.md:3439-3462, 3416-3437`)
37. **Key-name redaction misses values.** A URL with `?token=` under an innocent key leaked. Redact URL credentials by position, and record redactions derived from bytes that actually changed. (`LEARNINGS.md:3166, 3561`; `DECISIONS.md:2686`)
38. **The credential file is TOML and must be `0600`.** Mode is checked before the file is opened. Strip whitespace, because `echo` appends a newline that the gateway's constant-time compare rejects. (`DECISIONS.md:2760-2772`)
39. **A credential exported in the shell stays visible in the process environment.** No client change can fix that. Prefer the file or a keychain. (`QUEUED.md:457`, closed as out of Talaria's scope; `docs/releases/v0.6.0.md:18-19`)
40. **Status-script children get an allowlisted environment, with four provider credential names denied even when allowlisted.** Sweep the process group, because `setsid` children escape. (v0.6.0 verdict row 11; `DECISIONS.md:2216`; `QUEUED.md:1307`)
41. **Remote (non-loopback) gateways always require auth.**
    - `--insecure` is ignored.
    - Hermes offers a native-app flow (RFC 8252: an authorization code exchanged with a proof key, PKCE, against a loopback redirect) at `/auth/native/authorize` and `/auth/native/token`.
    - (`QUEUED.md:797-805`; pin `7f4d15515`)
42. **Admin HTTP bodies that carry secrets are redacted by route before recording.** (v0.6.2 CHANGELOG `:68-72`)

### 3.4 Process lessons worth keeping, in small doses

- **Verify against the running system, not a checkout or an option-space document.** Every source claim was once read against a checkout six weeks stale (`LEARNINGS.md:3416`). A stale options document re-opened a shipped decision (`LEARNINGS.md:89-116`).
- **A test must be run against the pre-fix code before it is trusted.** "An assertion that cannot fail is the dominant defect shape in this suite" (`LEARNINGS.md:3514`; also `:24-60`).

---

## 4. Decisions a rewrite should revisit

### 4.1 ADR-0004: Talaria is a Python client (2026-08-02)

What it rested on (`adrs/0004-talaria-is-a-python-client.md`, Context and Decision):
- Greenfield.
- Agent-authored code, so legibility and fast typed checks matter.
- Surrounding repositories are Python.
- Hermes core is Python.
- A native executable was declared not a requirement on 2026-08-02.

What rejected the alternatives:
- Rust with ratatui "leaves the most application infrastructure to Talaria, requires maintainer comfort with Rust async and terminal internals that this project has not established."
- Go with Bubble Tea v2 was rejected only because a small self-contained binary was not wanted.

Its own "Revisit when":
- A native executable becomes a requirement.
- Textual fails and no Python alternative clears.

Why revisit:
- The operator has now said they are open to Rust. That changes the maintainer-comfort premise.
- The "shared schemas with Hermes" benefit was never used: ADR-0001 and ADR-0003 forbid importing Hermes, and the code re-encodes protocol by hand.
- **Hypothesis:** the alignment argument bought little in practice.
- The analyses still stand on two points:
  - Bubble Tea v2 negotiates synchronized output out of the box.
  - Ratatui leaves the async runtime, command bus and compound widgets to the application (`…-analysis-final.md:180-192`).
- A Rust rewrite should budget for building that infrastructure.

### 4.2 ADR-0005: Textual is the presentation layer (2026-08-03)

Why revisit:
- The streaming transcript fought the widget-tree model from v0.2 onward. One table cell is one widget.
- That fight produced ADR-0006, a 25 KB document of four ceilings, a fallback trigger and a branch-hold mechanism.
- It left open performance items:
  - `QUEUED.md:965`: the block-markdown gate misses its table-apply ceiling.
  - `QUEUED.md:1012`: table hitch at about 340 rows, 54 to 59 ms per apply.
  - `QUEUED.md:1039`: a monster entry mounts every row.
  - `docs/releases/v0.2.0.md:16`: a one-time demotion costs 100 to 290 ms.
- The terminal's own select-to-copy never worked in the Talaria pane (section 2).
- ADR-0005's own revisit trigger was "a real terminal host or tmux exercises a surface the headless gate could not". That trigger fired on 08-10 (`LEARNINGS.md:1845`).
- **Hypothesis:** a renderer that writes finished transcript into native scrollback and keeps only the live region interactive would remove both the mount-bounding machinery and the copy problem. It needs a prototype.

### 4.3 ADR-0001: a standalone client dialing a gateway it did not launch

Still sound in principle. It is the shape the operator wants: an always-running backend that the client attaches to.

What needs re-deciding:
- **What it dials.** In August, `/api/ws` lived only in the dashboard process. A stale dashboard produced HTTP 500 that Talaria "cannot say so" about (`QUEUED.md:1386`).
- **Whether the "attach is steal" ownership model still holds.**
  - Multi-agent background work, with several sessions running and a client watching, depends on it.
  - At `22063fc09`, Hermes has a "session authority" that owns every session.
  - Older handlers survive as a "legacy sidecar". Tickets carry capabilities such as `worker:adopt` and can be bound to a secondary profile (`tui_gateway/ws_legacy_fallback.py:1-12, 26-31`, **verified**).
  - That is a different protocol shape from the one Talaria decoded. The protocol unit should settle it.

### 4.4 The per-profile multi-gateway design (v0.4)

The decision:
- `DECISIONS.md:873-893` dials every configured profile endpoint concurrently.
- Its rationale was that session events go only to the owning transport, and "sessions in other profile-gateway processes are invisible to any single connection regardless".
- Its revisit condition: "Hermes ships a cross-process or broadcast event surface, or a listing that names a session's profile."
- It rests on ADR-0001's open question, which was answered at the time with "one connection per profile" (`adrs/0001…:97-133`).

It cost a great deal:
- Connection sets, per-connection epochs and generations, seam boards, probes and frame logs.
- A `[profiles.endpoints]` configuration map the operator must write by hand (`DECISIONS.md:2910`).
- About 11,000 product lines in one release (`talaria/` grew from 28,096 to 39,215 lines between v0.3.0 and v0.4.0, verified).

Current evidence that the premise changed:
- **Verified:** the only Hermes WebSocket host process running today is one dashboard, `hermes_cli.main -p default dashboard --port 8765 --open-profile <work-profile>` (pid <pid>, started 2026-10-18, from `ps`). In August the topology document recorded four profile dashboards.
- **Verified:** the gateway now carries a per-session `profile_home` (`tui_gateway/compute_host.py:343-363, 416`).
- **Verified:** there is a per-connection profile binding (`ws_legacy_fallback.py:9-11, 69-71`).
- **Verified:** there is a "multi-profile processes" code path (`tui_gateway/entry.py:194-196`).
- **Unverified:** whether one connection now sees every profile's sessions. That needs a live probe.

**Recommendation:** assume one connection to one backend. Design the client so that a second connection is an exception path, not the architecture.

### 4.5 Other decisions worth reopening

- **ADR-0003: re-encode behavior rather than port code.**
  - Keep the principle.
  - Hand re-encoding of a protocol that moves this fast did not scale (section 6).
  - If Hermes publishes a machine-readable schema, generate decoders from it. Whether it does is unverified, and belongs to the protocol unit.
- **ADR-0006: bounded block rendering.** It falls away with a different renderer.
- **The per-release acceptance contract.**
  - Digest-bound receipts, a version-agnostic verifier, a gate document per version, and the rule that no live test may be waived.
  - `LEARNINGS.md:683` records that receipts bound to a harness commit "forbid squash-merging the release branch".
  - `LEARNINGS.md:610` records that "any edit under scripts/acceptance permanently reds the v0.5.0 receipt re-verification test on that branch".
  - A rewrite should keep the one thing that worked, a person driving a real terminal, and drop most of the rest.
- **The function-key "replay pacing" row and the record/replay product surface.**
  - These served the evidence apparatus more than daily use.
  - **Hypothesis:** keep the recorder as a debugging and fixture tool, and drop replay as a user mode.

---

## 5. Top 20 open product wishes in `QUEUED.md` (not process)

"Open" means the heading is not struck through and has no CLOSED marker, verified against `grep -n '^###' QUEUED.md`. The ranking by user value is my judgment, labelled as such. Items QUEUED still lists but that later releases at least partly delivered are marked.

| # | Wish | QUEUED line | Priority there | Note |
|---:|---|---:|---|---|
| 1 | `PageUp`/`Home` should actually scroll; reading back during a stream is mouse-only | 565 | P1 | Operator's stated reading habit |
| 2 | Decline a blocking prompt (sudo, secret) without waiting for it to expire | 756 | P1 | Empty answer releases the 120-second wait |
| 3 | Attach to a remote, authenticated gateway (native PKCE flow) | 797 | P1 | Needed for an always-running backend on another host |
| 4 | Two queued approvals with long commands: the second is unreadable; needs a layout redesign | 1212 | P2 | Matches operator Note 9, "cramped" |
| 5 | A card whose control is on screen is titled "answer below — scroll" | 835 | P1 | Misleading prompt chrome |
| 6 | A control-only card mounting into a full prompt region is never recomputed | 889 | P1 | Prompt reachability |
| 7 | Honour clarify `multi_select` | 1225 | P2 | |
| 8 | Send `approval.respond`'s `all` flag so "always" really stops repeats | 1234 | P2 | |
| 9 | A partial reply was not committed when the credential failed mid-stream (data loss, seen once) | 660 | P1 | Diagnose first |
| 10 | Bound the domain transcript, not just mounted widgets (memory in long sessions) | 1243 | P2 | 0.23 MB per 1,000 frames measured (ADR-0005 Revisit) |
| 11 | Streaming table hitches past about 340 rows | 1012 | P2 | Rendering speed |
| 12 | A monster line-rendered entry mounts every row before folding | 1039 | P2 | Rendering speed |
| 13 | Block-markdown gate misses its steady-state table ceiling | 965 | P2 | Rendering speed |
| 14 | Sub-agent monitor | 789 | P1 | Partly delivered: `Ctrl+G` sub-agent rows. A full monitor is still open, and it is core to the multi-agent goal |
| 15 | Deterministic Kanban adapter (board query, not just notifications) | 1287 | P2 | Hermes-native extra |
| 16 | Say why a gateway handshake failed (stale dashboard, HTTP 500) | 1386 | P2 | |
| 17 | Command entry is a listing, not completion; three catalogue fields unread | 1157 | P2 | Partly delivered: v0.3 `/` prefix filter and v0.6.1 sectioned menu |
| 18 | Desktop-like configuration views | 1612 | P3 | Largely delivered in v0.6.1 to v0.6.5 `/config`. Operator now wants "very user-friendly" backend configuration, which goes beyond that |
| 19 | Textual's own command palette leaks onto `Ctrl+P` | 1071 | P2 | Framework-specific |
| 20 | Agent working directory in the status line, and mid-session directory switching via `session.cwd.set` | 1641, 1655 | P3 | |

Also on the list:
- QUEUED still lists "Add MoA progress and fallback rendering" (`:1280`), but v0.6.1 delivered it (issue #148).
- The platform matrix is one operating system and two terminal hosts (`:528`): Linux and real emulators are unexercised.

Operator wishes that never reached QUEUED (from the v0.2 notes):
- Select-to-copy through the pane (Note 2).
- A "notes" field on approval and ask answers (Note 9).
- A clickable "Jump to bottom" like Claude Code (Note 8).

---

## 6. Diagnosis: why Talaria ended up slow and not clean

Each point names a mechanism and its evidence. A cause I could not measure is labelled.

**1. Acceptance ceremony consumed most of the effort in the later releases.**
- Process and acceptance artifacts took about 44% of lines added over the project. The product package took 20% (section 1.2).
- In the v0.5.0 and v0.6.1 windows, product code was 8% and 9% of lines added. `docs/acceptance/` was 48% and 52%.
- Each release ran a multi-role orchestration contract. Issue #118's body defines Controller, Architect, worker, Reviewer, Investigator and Tester lanes. It also defines tiered plan approval, wave merges, board-transition rules and pre-launch gates.
- v0.6.1 had 23 digest-bound live receipts. Four "configuration residuals" releases shipped on one day, each with its own gate document and release-record test.
- The apparatus generated its own defects. "Four things in one release looked like verification and performed none" (`LEARNINGS.md:24`). Receipts forbade squash merges (`:683`). Editing acceptance scripts turned an old release's test red (`:610`).
- 96 of 224 LEARNINGS headings are about verification, tests, gates or receipts (keyword count, verified).
- Meanwhile two hours of real use found seven defects that the apparatus missed (`LEARNINGS.md:1845`).
- One investigation "spent over a million input tokens … and produced nothing that survived checking" (`docs/plans/2026-08-11-v0-3-decision-log.md`, around line 700).

**2. Hand-written decoding of a fast-moving protocol.**
- Talaria pinned and re-encoded Hermes behavior by reading source. Code and test citations still point mostly at `7f4d15515`: 41 of 51 backticked revision citations in `talaria/` and `tests/` (verified with `git grep`).
- Hermes moved about 21,400 commits from Talaria's last pin `3236f4405` (2026-09-14) to `22063fc09` (2026-10-17) (verified, `git rev-list --count`).
- Over the same span, `tui_gateway/` changed in 106 files, +15,880 and −4,373 lines (verified, `git diff --stat`).
- Drift showed up repeatedly as user-visible bugs:
  - `platforms.changed` and `session.usage` shown as unknown events.
  - Usage meters showing zero.
  - Approvals gaining request ids.
  - A "session authority" replacing legacy handlers.
- Shape checks look only at the top level (`QUEUED.md:1126`).
- There is no capability endpoint on the terminal gateway to ask (`LEARNINGS.md:3464`).
- **Hypothesis:** a rewrite without generated or schema-checked decoders, and a live-recorded fixture loop, will repeat this.

**3. A monolithic application module.**
- `talaria/ui/app.py` grew 3,214 → 4,337 → 4,603 → 6,350 → 6,684 → 7,815 → 8,486 lines from v0.1.0 to v0.6.5 (verified, per tag).
- It has 245 indented `def`s today. 96 commits touched it.
- `QUEUED.md:938-962` records `TalariaApp` at "roughly 5,534 lines and 181 methods in one class". It says "all six feature children edited this shared file, so the run could not parallelize those edits structurally". The mitigation was a coordinator-enforced "shared-surface lease", not a fix.
- `talaria/domain/state.py` is the second hotspot at 4,795 lines.

**4. A transcript model that fights the framework.**
- One widget per line, then per markdown block, with cell-level widget explosion. That required ADR-0006's four ceilings and several fallback paths, and still left the three rendering-speed items open (section 5, rows 11 to 13).
- The 50 ms coalescing tick bounds render rate by construction (ADR-0005 decision 2).
- **Measured in this session:** importing `talaria.ui.app` takes about 0.32 to 0.40 s warm (`python -X importtime`; three runs: 0.79, 0.40, 0.37 s). So cold start is not the main cost.
- **Unverified:** where the operator's "slow" actually comes from. Candidates are transcript mounting and reflow, the coalescing tick, and Markdown reparse. Profiling a real session would settle it.

**5. Architecture sized for a Hermes topology that changed.**
- v0.4's per-profile connection fleet added about 11,000 product lines (section 4.4).
- It was built on the then-true fact that each profile ran its own gateway process.
- Current evidence suggests one backend serves all profiles. Most of that code may now be dead weight (**hypothesis**, pending the protocol unit's live check).

**6. Features chosen by the evidence machinery's needs rather than the operator's.**
- Record and replay, replay pacing keys F8 to F10, the frame-log format, receipt schemas, and a TypeScript reference recorder kept only to prove redaction equivalence.
- `CLAUDE.md` describes the reference recorder as "three files … that `tests/recorder/test_equivalence.py` asserts the Python recorder is equivalent to".
- Meanwhile select-to-copy, `PageUp`, and decline-a-prompt stayed open.

**7. Stale-source reasoning recurred despite rules against it.**
- A six-week-stale checkout (`LEARNINGS.md:3416`).
- Checkout versus running process (`:1464`).
- A stale option-space document (`:89`).
- The project's thoroughness did not protect it from reading the wrong source. It made wrong readings look authoritative.

---

## Sources read

- **Talaria at `cafc640`:**
  - `platform-specs/04-architecture/adrs/0001` to `0006`
  - `docs/analysis/`: the direction, reconsideration and final language analyses, the reconciliation rules, the v0.1 verdict, the Textual gate, the v0.2 hands-on notes, the v0.4 topology verification, the conformance audit headings, the v0.6 to v0.6.5 verdicts
  - `docs/engineering-journal/` (headings of all three files, plus about 40 sampled entries)
  - `CHANGELOG.md`, `docs/releases/v0.5.0.md`, `docs/releases/v0.6.0.md`
  - `docs/terminal-ui.md`, `docs/configuration.md`
  - `docs/plans/2026-08-11-v0-3-orchestration-charter.md`, `docs/plans/2026-08-11-v0-3-decision-log.md`
  - git history, tags and per-path churn
- **GitHub:** `gh issue list` (34 issues, all by the operator's account), issue #118 body, issue #139 body (sampled), and `gh pr list` (154 pull requests, 150 merged).
- **Hermes at `22063fc09`:**
  - `tui_gateway/methods_prompt.py:1246-1260`
  - `tui_gateway/ws_legacy_fallback.py:1-80`
  - `tui_gateway/compute_host.py:343-416`
  - `tui_gateway/entry.py:188-196`
  - `git rev-list` and `git diff --stat` from `3236f4405`
  - `ps` of running Hermes processes, with credentials redacted
