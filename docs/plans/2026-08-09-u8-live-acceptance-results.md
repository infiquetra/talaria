---
title: U8 live acceptance run — results
type: results
status: passed
date: 2026-08-09
covers: R1, R2, R3, R4, R5, R6, R7, R12
executes: docs/plans/2026-08-09-u8-acceptance-checklist.md
targets: docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md#u8-the-live-acceptance-run
supersedes: the blocked verdict this file carried earlier on 2026-08-09
---

# U8 live acceptance run — results

## The verdict

**All seven legs pass.** R1 through R7 were driven live on 2026-08-09 against a running Hermes
gateway, with the operator present, on throwaway sessions created for this run, with every frame
recorded, redaction-checked, and cited by digest below. The earlier blocked verdict this file
carried is superseded: both of its unblock conditions were met before any key was sent — U1–U7
merged to `main` via pull request #45 (merge commit `529928c`), and a dedicated throwaway pane was
created fresh for this run by the driver (a new tab of its own, so it is by construction not one of
the operator's working panes; its machine-local identity lives in saga state, not in this public
document).

## Run setup

- Code under test: `main` at `529928c` ("Merge pull request #45 — the answerability spine").
- Gateway: the configured endpoint in the credential file (`ws://127.0.0.1:8765/api/ws`), serving
  the local Hermes profile the operator designated for this machine. The credential postdated the
  server's start, so no refresh was needed.
- Sessions: two throwaway sessions created by this run — session A
  (`20260809_084412_b08629`, created by a bare `talaria --record` launch) and session B
  (`20260809_085422_e794c1a1`, created the same way for leg 7). Both were left idle and
  prompt-free at the end of the run; the one background process a canary started was killed by the
  agent before the run closed.
- Recording discipline (R12): every launch ran with `--record`. All five recordings were scanned
  for the gateway credential token and for any unredacted `password` field before being cited
  here; all five came back clean.

| recording | role | sha256 |
|---|---|---|
| `2026-08-09T12-44-11-662Z.jsonl` | session A: legs 1–5 (canaries, clarify, F1, focus walk, interrupt sweep, sudo decline) | `1762e6a9aecafd8a6d6667a299006d2120b8aaa280313257147e195524906772` |
| `2026-08-09T12-55-38-698Z.jsonl` | the `--resume` most-recent observation (see observation 1) | `3934ee302d4c317a31f0d76794d5a11479c77365035094b8c6ffbdaa998b2acf` |
| `2026-08-09T12-56-39-967Z.jsonl` | the runtime-id refusal observation (see observation 2) | `a1583ff308ff5f421ea426b4266d25fc96d7ee854c159951d5484af11ffb80df` |
| `2026-08-09T12-57-26-563Z.jsonl` | leg 6: durable-id resume of session A | `5e1ad954785701088e7cd285e3ef095156f7e0d9c8d5650535284e07f1222878` |
| `2026-08-09T12-57-52-811Z.jsonl` | leg 7: session B launch and the `/sessions` switch back to A | `de321a1dc0c1f8d057e76f8153c91d9aead08b7c465061572094bfeb9ded74ce` |

## Legs

1. **R1 — pass.** With a clarify card outstanding (see observation 4 for why clarify stood in for
   an approval), one `F1` press moved the caret from the composer to the card's control: the
   status region read `caret: prompts` on the next read, and the card carried its hint line
   (`enter select · esc decline`) naming its operating keys. F1 was also re-exercised later on the
   sudo card before leg 3's decline, with the same result.
2. **R2 — pass.** The ANSI screen read of the focused card shows a card-level background tint
   (`rgb(65,47,23)` across the whole card, against the interface's default `rgb(18,18,18)`
   ground), while the focused `Yes` control separately carried reverse video — the card is
   distinguished by more than reverse video alone.
3. **R3 — pass.** A genuine gateway sudo prompt ("sudo password required") was raised by a canary
   (`sudo -k true`; the leg exists to decline, and no password was ever typed). With the card
   focused, `escape` cleared it and the transcript read "— sudo declined". The recording carries
   the wire frame: `sudo.respond {"request_id": "cde7a8d1", "password": "[redacted]"}` with an
   `ok` reply — the sudo kind's own method, never the approval `deny` shape. One nuance: the
   recorder redacts the `password` field by name regardless of content (correct — a recording must
   never distinguish an empty from a non-empty value there), so the field's *emptiness* is pinned
   by the headless test `tests/ui/test_prompts.py::test_escape_on_a_sudo_control_sends_an_empty_password_and_clears_it`
   (asserting `password: ""` on the wire), not by the recording.
4. **R4 — pass.** With the clarify card outstanding, `F4` sent the interrupt; the card stayed on
   screen until the gateway's confirmed reply rendered (`*[interrupted]*`), and only then swept,
   with the honest notice "clarify not declined — the gateway had already stopped waiting — the
   answer was discarded". Nothing queued behind it. ("Confirmed" here is the gateway-confirmed
   reply, per `interrupt_live`'s contract — only a confirmed interrupt declines the turn's
   outstanding prompts.)
5. **R5 — pass.** Walking focus through the prompt card, the composer, and the transcript: the
   status region read `caret: prompts` and `caret: transcript` at those stops, and rendered an
   empty (but still one-row) slot at the composer stop — which is the documented design
   (`talaria/ui/status_region.py:88-93`: the slot names where *else* the caret went; the composer
   is the caret's home). Across all three states the compose box's top border stayed on the same
   screen row (row 37 of a 40-row read): no widget height changed and no row entered or left the
   body stack.
6. **R6 — pass.** Relaunching with `--session 20260809_084412_b08629` rendered all sixteen prior
   messages as committed transcript entries — user rows, terminal tool rows, the clarify
   question, the interrupt marker, and the sudo sequence — before any new live event.
   `messages_omitted` was not exercised (sixteen messages is below any omission threshold), so the
   omission-naming half of the expectation remains covered by its headless pin only.
7. **R7 — pass.** With sessions A and B both live, `/sessions` opened the picker: rows are listed
   by durable id, the focused session's row (session B, `20260809_085422_e794c1a1`) carried the
   highlight, type-to-filter on a fragment of A's durable id ranked A first, and selecting it
   issued `session.resume {"session_id": "20260809_084412_b08629"}` on the wire — the durable id,
   through the same landing path as startup — after which A's history rendered exactly as in
   leg 6.

## Observations (not leg failures)

1. **`--resume` raced a background session.** Between quitting session A and relaunching,
   a webhook-spawned agent session became `session.most_recent`, and `--resume` landed there.
   The run exited it immediately without typing anything into it. This is the gateway's
   most-recent semantics, not a talaria defect — but on a machine where background automations
   spawn sessions, `--resume` is effectively nondeterministic, and `--session <durable-id>` is
   the dependable form. Recording `3934ee30…` above.
2. **`session.resume` wants the durable id, not the runtime id.** Resuming by the runtime id the
   create reply had returned (`07e299c5`) was refused by the gateway with code 4007 ("session not
   found"), rendered honestly by the app; the durable `stored_session_id` worked. The `--session`
   help text says "attach to an explicit session id" without saying which id; a docs touch-up
   naming the durable id is queued. Recording `a1583ff3…` above.
3. **An empty prompt dock can still take focus.** After the leg-4 sweep removed the only card,
   one Tab stop still reported `caret: prompts` with no card on screen. Cosmetic, but it hands
   the caret to a region with nothing to operate; queued as a follow-up.
4. **No approval-kind card could be provoked on this profile.** The profile runs smart approval
   (an auxiliary reviewer auto-approves low-risk commands), so both canary shell commands ran
   without raising an interactive approval; the checklist's leg 1 explicitly allows "approval or
   clarify", and clarify/sudo cards carried the run. Consequence: the approval answer and
   deny-all paths remain pinned by their headless tests only — no live approval card has been
   driven. If a live approval drive is wanted, it needs a profile or command class the smart
   approver declines to auto-approve.
5. **Two gateway event kinds are unknown to talaria.** `platforms.changed` and
   `agent.terminal.output` rendered as "! unknown event type" notices — the designed
   unknown-event path doing its job, and a queued candidate for teaching talaria the new kinds.

## Safety envelope: held

Throwaway sessions only; the one `--resume` mis-landing was exited without input. No credential,
real or canary, was ever typed — the sudo leg declined. Canary commands granted nothing
(`touch`/`rm` of a scratch file, `sudo -k true` declined). All recordings were redaction-checked
before citation. The operator was present throughout. The run closed what it opened: the canary
file was removed by its paired canary, the background pty was killed, both throwaway sessions were
left idle with no prompt waiting, and the throwaway pane was closed after the run.
