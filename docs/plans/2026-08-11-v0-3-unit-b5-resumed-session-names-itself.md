---
title: v0.3 unit B5 — a resumed session names itself on arrival
type: plan
status: proposed
date: 2026-08-11
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: B5
---

# Unit B5 — a resumed session names itself on arrival

The cheapest unit in spine B, and the clearest instance of the release theme: **Talaria does not
confirm what it just did.**

**The finding.** `--resume` resumes the *gateway's* most recent session, not the operator's. Recorded
in [the v0.3 handoff](2026-08-11-v0-3-session-handoff.md): "correct per `session.most_recent` and
silently surprising on a gateway shared with automation."

**The behaviour is correct and stays correct.** This unit changes nothing about which session is
chosen. It changes only that the operator is told which one arrived.

## Mechanism — verified by reading, at `main` = `5cd51f1`

`--resume` is deliberately two calls, and the docstring at `talaria/ui/app.py:3128` says why:
`session.most_recent` is the read-only method that names the target, `session.resume` is the mutating
one that opens it, and splitting them is what lets "no previous session" be reported rather than
quietly becoming a new conversation.

1. `talaria/ui/app.py:3187-3200` — the resume branch calls `session.most_recent`, reads `session_id`
   out of the reply, and on a null result reports `NO_SESSION_TO_RESUME` **and appends a transcript
   line saying so**, via `record_local_note` at `:3196-3198`.
2. `talaria/ui/app.py:3214-3220` — the success path dispatches `session.resume` inside the landing
   barrier and hands the outcome to `_land_session`.
3. `_land_session` (`talaria/ui/app.py:3248`) reads **both identities** out of the reply: the runtime
   `session_id` that events are stamped with, and the durable id resolved at `:3290-3294` from
   `session_key`, `resumed`, or `stored_session_id` — the one the session picker names a session by.
4. **Nothing announces either of them.** The transcript seeds with the session's history and the
   operator is given no statement of what they are looking at.

**So the asymmetry is the defect, and it is visible in one function.** The path where resume finds
*nothing* writes the operator a line. The path where it finds *something* — the one where identity
actually matters, because there was a choice and the gateway made it — writes nothing at all.

## Key technical decisions

### KTD1 — the announcement is a local note, not a new mechanism

`record_local_note` (`talaria/domain/state.py:861-869`) exists for exactly this: "a Talaria-authored
system line", kept deliberately distinct from `_apply_system_line` so that "the two sources of a
system entry cannot be confused in review" — that one renders text the *gateway* sent, this one
renders text Talaria wrote about its own transport.

The `NO_SESSION_TO_RESUME` path already uses it, in the same resume flow — that call sits at
`app.py:3196-3198` and this change lands in `_land_session` from `:3248`. This unit adds a second
caller, not a second mechanism.

**Rejected — a composer notice instead of a transcript row.** A notice is transient. The question
"which session am I in?" is asked minutes after arrival, when the notice is long gone, and the
transcript is the durable surface.

### KTD2 — it names the durable id, because that is the one the operator can act on

Both identities are available at land time and they are not interchangeable. The runtime `session_id`
drives event correlation and means nothing outside the process. The durable key — `session_key`,
`resumed`, or `stored_session_id` — is, in `_land_session`'s own words, "what the picker names a
session by and what a later resume asks for".

An operator who wants to check what they just landed on, or resume it deliberately next time, needs
the durable one.

**Rejected — naming the title instead.** `session_title` (`talaria/domain/state.py:102`) is `None` at
land time — `focus_session` resets it to `None` on every switch (`state.py:478`) — and is filled later
by a **`session.info`** event, folded by `_on_session_info` (`state.py:2269-2293`), which is registered
for `"session.info"` at `state.py:2395`. Waiting for it would make the announcement arrive at an
unpredictable moment, or not at all for a session that never gets one.

**`session.title` is not the title source, despite the name.** It is a recognized-but-*unhandled*
event type — present in `_OBSERVED_ON_A_LIVE_GATEWAY` (`talaria/domain/decode.py:110-111`) and absent
from the handler registry — so Talaria surfaces it by name and never folds it into `session_title`.
Stated here because an implementer hunting for a `session.title` handler will not find one, and
adding one would be a behaviour change beyond this unit.

**The durable id may be absent.** `_land_session` already tolerates that — it falls through three
keys. When all three are absent the note names the runtime id and says that is what it is, rather
than printing nothing or printing an unlabelled string.

### KTD3 — every resume landing announces, not only `--resume`

`_land_session` is reached by three routes, and all three arrive at the same function carrying only an
`RpcOutcome`:

```
--new         --> session.create (app.py:3178-3184) --> _land_session
--resume      --> session.most_recent + session.resume (app.py:3214-3220) --> _land_session
picker switch --> session.resume (switch_session, app.py:3700-3722) --> _land_session
```

The last two are cases where the operator ends up looking at a session whose identity someone else
chose. Announcing in `_land_session` covers both and puts the change at one site.

**`--new` does not announce**, and the discrimination is by method, not by a threaded flag.
`RpcOutcome` carries the method it was a reply to (`talaria/transport/rpc.py:108`), so
`_land_session` announces when `outcome.method` is `RESUME_METHOD` and stays silent when it is
`CREATE_METHOD`. **Rejected — threading a mode flag from `open_session`'s call sites**, which would
add a parameter to carry information the outcome already holds, and would not cover `switch_session`
without a third call site learning about it.

A session the operator just created has no identity question — they know what it is because they asked
for it. Adding a row there is noise on the release that is separately trying to make the transcript
less busy.

### KTD3a — the insertion point is pinned, because two nearby choices are silently wrong

`land_session` **clears the transcript on a real switch** (`talaria/domain/state.py:569`). A note
appended before that call is wiped on the picker-switch path — and passes the first-landing case, so
the mistake ships looking correct.

The append goes **after** the confirmed and refusal early returns (`app.py:3268-3283`), **after**
`land_session` (`app.py:3302-3306`), and **before** `seed_history` (`app.py:3314`). That is the only
window where the note survives the clear and still precedes the history.

### KTD3b — a landing that changes nothing announces nothing

`_land_session` seeds history only when the focus actually moved — `if previously_focused != raw:`
(`app.py:3307`). The other branch, reachable by choosing the already-focused row in the picker, keeps
the transcript exactly as it is, deliberately, because seeding into it re-appended the same history a
second time (`app.py:3308-3312`).

**That branch does not announce.** A row reading "resumed session X" when the operator never left
session X confirms nothing, and repeated picker presses would each add one to a transcript this
release is separately trying to quieten.

**The real need this exposes belongs to another unit.** The operator who picks the row they are
already on still deserves to know the keypress registered — that is unit B3, "a keypress that did
something is distinguishable from one that did not", and it is the right home for transient feedback.
Naming it here rather than solving it here keeps B5 to one mechanism.

A reconnect does not reach this branch at all: reconnects append a `note_reconnect`
(`app.py:1519-1544`) rather than re-landing through `open_session`.

### KTD4 — the wording states the choice, not just the identifier

A bare identifier is not an announcement. The line says that this session was *resumed* and which one
it is, so a reader who did not expect a shared gateway's most-recent session learns both facts at
once. Exact wording is settled in implementation against the existing constants near
`NO_SESSION_TO_RESUME`, following their register.

## Risk this unit must clear

**The replay gate cannot see this change at all, which is a stronger reason than the one this plan
first gave.** The gate constructs `TalariaApp(..., mode="replay")` with no startup selection
(`talaria/replay/gate.py:1217` and `:1433`), and `begin_live_startup` short-circuits when `startup` is
`None` (`talaria/ui/app.py:3053`). So `open_session` never runs under the gate, the announcement row
is absent from **both** the replay projection and the replayed pane, and `content_is_complete` and
`interface_shows_everything` (`gate.py:1380` and `:1382`) have nothing to disagree about.

This plan originally argued that both sides "move together". That is true of the live path and was the
wrong reason here: under the gate neither side moves. Same conclusion, sounder mechanism, corrected on
review rather than left as a lucky prediction.

**The live-versus-replay equivalence test is unaffected for the same reason.**
`tests/transport/test_source_equivalence.py:264-304` compares a live run against a replay of the same
frames, which is exactly the shape this change could have broken — but its corpus contains no landing
and neither application receives a startup selection.

**The landing barrier is the ordering hazard.** `_land_session` runs inside `_landing`
(`talaria/ui/app.py:3225`), which holds inbound frames and folds them in arrival order on exit.
The note must precede the seeded history rather than landing in the middle of it — the announcement of
what arrived belongs before what arrived. KTD3a pins the exact window; AE3 asserts the order.

**Four exact-transcript tests change, and they are named rather than discovered.** All four are in
`tests/transport/test_session_startup.py` and assert the transcript's full contents on a resume path:
`test_resume_puts_the_resumed_conversation_on_the_rendered_screen` (`:602-607`),
`test_an_event_racing_the_resume_reply_lands_after_the_seeded_history` (`:703-713`), the
`_land_session`-inside-`_landing` barrier assertion (`:770`), and
`test_a_resume_that_withheld_its_history_says_so_on_screen` (`:797`). Each is updated with a comment
naming this plan, not quietly edited. `tests/ui/test_sessions.py` survives untouched because its
transcript assertions filter by content (`:569-577`).

## Acceptance evidence

- **AE1.** A confirmed `--resume` landing appends exactly **one** system row naming the resumed
  session by its durable id.
- **AE2.** A picker-driven switch to a **different** session appends the same row; a `--new` session
  appends **none**. Both halves are asserted — the silence is a requirement, not an omission.
- **AE2a.** A picker-driven switch to the session **already focused** appends **none**, and the
  transcript it was already showing is unchanged. This is KTD3b's retain branch.
- **AE3.** The row appears **before** the seeded history in the transcript, not interleaved with it.
- **AE4.** A resume reply carrying no durable id still announces, naming the runtime id and labelling
  it as such. No silent path.
- **AE5.** An unconfirmed or refused landing appends **nothing** — the existing failure paths are
  unchanged and still report through `_report_startup_failure` and `switch_refusal`.
- **AE6.** The replay gate runs green over the existing gate corpus, with both `content_is_complete`
  and `interface_shows_everything` true. The corpus is named by digest and frame count, never by path.
- **AE7.** The project check is clean: `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check`.

**Acceptance for a person, per the charter's evidence rule 2:** on a gateway that has more than one
session, the operator runs `--resume` and can say which session they are in without asking. That is
operator-only and is not claimed on test evidence.

## Verification

```bash
uv sync --all-groups
uv run pytest tests/domain/ tests/ui/ tests/replay/ tests/transport/ -q
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched.

## What this unit does not do

- **It does not change which session `--resume` resolves to.** That behaviour is correct per
  `session.most_recent` and re-deciding it is a different unit with its own evidence.
- **It does not add a session switcher, a `--session` flag, or any way to name a target.** Those are
  fleet-axis work, deferred by decision under the charter's unit L4.
- **It does not render the title.** The title arrives on its own event when the gateway sends one.
