# u16: Stack bake-off — the same Hermes client built in Rust and in TypeScript

Date: 2026-10-22. Machine: the operator's Mac (arm64, 24 cores), heavily loaded by other agent
sessions during the runs (one builder recorded a one-minute load average of about 20). Hermes:
revision `22063fc09`, gateway running. This unit is the "head-to-head agent spike" that
[u11](u11-language-framework.md) §8 named as the evidence that would settle Rust against TypeScript.

## Plain-English answer

- **All four agents built a working client.** Each one attached through the ticket path, streamed
  replies, showed tool cells, answered approvals, interrupted turns, detached cleanly, wrote finished
  output into native scrollback, and recorded and replayed frames. None needed human help.
- **TypeScript with OpenTUI was easier to build.** It needed 3 fixes at the Sonnet tier against
  11 for Rust. Nearly all of Rust's extra fixes were in one place, the inline terminal layer. At the
  Haiku tier the counts were close: 6 for TypeScript and 5 for Rust.
- **Rust was much lighter and faster at runtime.** It held about 5 MB of memory against 104 to
  122 MB, shipped a 2.2–2.3 MB binary against 74.9 MB, and reached `gateway.ready` in about 15 ms
  against about 300 ms.
- **Every builder fell into the same protocol traps.** They are listed in section 5 and copied
  into [03-hermes-today.md](../03-hermes-today.md) §2.

## 1. Method

One specification went to four builders running in parallel in one workflow, each in its own
scratch folder:

| Builder | Model and effort | Stack offered |
|---|---|---|
| rust-sonnet | Sonnet 5.5, high | Rust 1.98, ratatui 0.30, crossterm 0.29, tokio, tokio-tungstenite, serde_json |
| ts-sonnet | Sonnet 5.5, high | TypeScript on Bun 1.3.14, `@opentui/core` 0.6.x in split-footer or main-screen mode |
| rust-haiku | Haiku 5.5, xhigh | as rust-sonnet |
| ts-haiku | Haiku 5.5, xhigh | as ts-sonnet |

The specification asked for a program called `slice` that does five things:

1. Attaches through the gateway's control socket, a single-use ticket and the `hermes-gateway-v1`
   WebSocket. A working Python probe of exactly these steps was supplied.
2. Creates a session on the free model `minimax-m3.1-flash` (provider `ollama-cloud`) in a
   throwaway project.
3. Streams replies inline, with only a small live region redrawn.
4. Handles tool cells with coloured diffs, approvals (keys 1–4 or arrows and Enter), interrupt with
   Ctrl+C, and detach on exit.
5. Records frames with `--record` and replays them with `--replay`.

The builders had to produce offline tests (frame decoding, a full-turn state machine and a render
snapshot) and a live run inside a pseudo-terminal harness. The live run asked one question, then ran
`rm -rf .pytest_cache` behind an approval. They also had to report measurements and log every fix
in a file, one line per failure that needed an edit.

The rules set a 120-minute time box per builder. They forbade changes outside the scratch folder and
any change to Hermes, its configuration or its processes. The TypeScript builders were allowed to
vendor Hermes's MIT-licensed client files. The Rust builders were pointed at ratatui's inline viewport
and `insert_before`, or at porting OpenAI Codex's `insert_history` technique.

The whole workflow took 22 minutes. Each builder finished well inside its time box.

## 2. Results

| | rust-sonnet | ts-sonnet | rust-haiku | ts-haiku |
|---|---|---|---|---|
| Attach, stream, tool cells, approval, interrupt, scrollback, replay | all work | all work | all work | all work |
| Offline tests passing | 10 | 17 | 15 | 20 |
| Wall-clock minutes (from the builder's start stamp) | about 20 | about 13 | about 20 | about 20 |
| Fixes logged | 11 | 3 | 5 | 6 |
| Compile errors | 1 | 0 (strict `tsc` clean) | 0 | 0 |
| Lines written: source + tests | 1,496 + 284 | 1,182 + 221 | 1,820 in total | 1,214 + 271 |
| Clean release build | 13.4 s | 0.23 s | 9.1 s | 0.15–0.20 s (warm) |
| Binary | 2,210,976 bytes | 74,872,418 bytes | 2,269,568 bytes | 74,872,418 bytes |
| Program start to `gateway.ready` | 12–26 ms (6 runs, in-process clock) | 271–533 ms, median about 300 (6 runs) | 22.8 ms | 271–350 ms, median 279 (9 runs) |
| Submit to first streamed character, no tool | 2.5 s | 1.36–1.52 s | not isolated | 1.59 s |
| Resident memory after a turn | 2.8–5.5 MB | 104–114 MB | 5.5 MB | 111–122 MB |

Submit-to-first-character times are dominated by the model and are not a client measure. ts-haiku
measured submit to the first gateway event at 3.3 to 6.3 ms, so the client's own overhead is small
in both stacks.

Neither TypeScript builder used its permission to vendor Hermes's client. Both wrote their own
channel of about 140 lines.

## 3. Fixes, grouped by cause

Builders counted slightly differently: rust-sonnet counted design changes found on inspection, and
the Haiku builders separated harness bugs. The groups below come from each builder's own log.

| Cause | rust-sonnet | ts-sonnet | rust-haiku | ts-haiku |
|---|---|---|---|---|
| Protocol: the contract or the specification was wrong | 3 | 1 | 3 | 3 |
| Terminal library or language | 6 | 0 | 1 (not reproduced) | 0 |
| Application logic | 1 | 1 | 0 | 1 |
| Tests or test harness | 1 | 1 | 1 | 2 |
| **Total** | **11** | **3** | **5** | **6** |

The six Rust terminal and language fixes in rust-sonnet were:

1. A borrow-checker error, two mutable borrows in terminal restore.
2. A panic when crossterm's event stream was created without a terminal (plain mode).
3. and 4. ratatui's inline viewport queries the cursor position, and the query timed out while
   crossterm's key reader held the input. Two attempts were needed. The fix was a fixed viewport
   that the program moves itself, so no query happens after start-up.
5. ratatui's `insert_before` scrolls with `CSI S`, which the test emulator ignored and which did
   not demonstrably feed scrollback. It was replaced with Codex-style newline scrolling inside a
   scroll region anchored at the top.
6. The first line was lost when the live region started on screen row 0.

rust-haiku's one unconfirmed failure was the same cursor-query race: the client died once just after
attaching, and reordering start-up stopped it recurring.

The specification itself steered both Rust builders towards `Viewport::Inline` and `insert_before`,
the two interfaces behind most of this friction. A patterns guide that names the working approach
should remove most of it. That is a hypothesis, which unit P0-04 of the roadmap tests.

## 4. What was not verified

- **Real terminals.** Every scrollback check used the `pyte` emulator in a pseudo-terminal. None ran
  in iTerm2, Terminal.app or Ghostty.
- **Live-run gaps in every client.** No client was tested live for terminal resize, a second Ctrl+C
  during a turn, or the "session" and "always" approval choices.
- **Diff colouring.** ts-sonnet verified it offline only. ts-haiku confirmed the colour codes for
  user lines in raw bytes but not for diff lines.
- **Unexplained exits.** Three early rust-sonnet runs ended with SIGTERM about one second after
  launch, and two early ts-haiku runs exited about 1.5 to 2 seconds in. None recurred in later runs,
  and no cause was found.
- **Typing while streaming.** Keystroke latency and CPU use while streaming were not measured for
  any client.

## 5. Protocol traps every builder met

Each item was observed against the live gateway at `22063fc09`.

1. `prompt.submit` is refused with `invalid_params` (code 4001, no field named) unless it carries
   `submission_id` or `input_id`. The contract marks both as optional. The check is in
   `gateway/session_controls.py`.
2. `prompt.submit` returns an admission receipt (`status: queued`). The turn itself arrives as
   events.
3. `session.create` returns a full snapshot that includes:
   - `subscription_id`, which `session.detach` needs;
   - `execution_generation`, which starts at 0 and rises with each turn. Events carry it, and
     `session.interrupt` and `approval.respond` need the current value.

   Events flow after `session.create` with no separate subscribe call.
4. Progress text arrives as `thinking.delta`, often with empty text. `reasoning.delta` never
   appeared with this model. Events missing from the specification also arrive: `agent.step`,
   `agent.print`, `tool.generating`, `session.info`, `sessions.changed` and `platforms.changed`.
5. Approval choices arrive in the order `once, deny, session, always`, so keys must be mapped by the
   payload's order. The payload carries `prompt_id`, `execution_generation`, `command`, `description`
   and `timeout_s` (60). On the canonical socket approvals arrived as events. No server-to-client
   approval request was seen.
6. `session.interrupt` does not settle a pending approval. The gateway waits out the 60-second
   prompt timeout, so deny first and then interrupt. An interrupted turn ends later as
   `message.complete` with status `interrupted` and outcome `cancelled`.
7. `tool.complete` reports failure with `is_error`, a boolean, and its `result` is a JSON string.
   `context` is preview text that can read like `rm -rf .pytest_cache + 1 command`. The completion
   may omit the command, so carry it from `tool.start` by `tool_id`. One builder saw `duration_s`;
   another found it absent in every payload and timed tools on the client.
8. `inline_diff` arrives already coloured with ANSI escapes, under a `┊ review diff` header, with
   `a/path → b/path` lines instead of unified-diff headers. Strip escapes before classifying lines,
   and never print it raw.
9. `session.usage` nests its numbers under `usage` (`input`, `output`, `total`).
10. The first `message.delta` after a tool call often starts with blank lines, and
    `message.complete` repeats the whole reply text.
11. The `session.detach` reply carries `session_id` and `subscription_id`, as the create snapshot
    does, but has no `stored_session_id`. A reducer that recognises snapshots by those two fields
    will mistake it for one.
12. Every `session.create` leaves a session row in the profile. Two builders reported 11 and about
    22 rows from their runs. Run tests in a throwaway profile.

## 6. Side effects

- The builders' sessions were created in the default Hermes profile and were not deleted.
- No Hermes file, configuration or process was changed.
- No ticket, token or key was printed or recorded. The builders checked this by scanning their
  recordings.

## 7. Kept for reference

The two Sonnet clients are in [../reference-slices/](../reference-slices/). Their recorded fixtures
were scrubbed of machine-specific data before being added. The Haiku clients and every builder's raw
evidence stayed in the session's scratch folder and are not kept.
