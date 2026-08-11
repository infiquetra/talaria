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

The `NO_SESSION_TO_RESUME` path already uses it, four lines from where this change goes. This unit
adds a second caller, not a second mechanism.

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
land time and is filled later by a `session.title` event
(`talaria/domain/state.py:2285-2293`). Waiting for it would make the announcement arrive at an
unpredictable moment, or not at all for a session that never gets a title. The title is welcome when
it arrives and is not a substitute for saying something now.

**The durable id may be absent.** `_land_session` already tolerates that — it falls through three
keys. When all three are absent the note names the runtime id and says that is what it is, rather
than printing nothing or printing an unlabelled string.

### KTD3 — every resume landing announces, not only `--resume`

`_land_session` is reached by both startup `--resume` and a picker-driven switch. Both are cases where
the operator ends up looking at a session someone else chose the identity of. Announcing in
`_land_session` covers both and puts the change at one site.

**`--new` does not announce.** A session the operator just created has no identity question — they
know what it is because they asked for it. Adding a row there is noise on the release that is
separately trying to make the transcript less busy.

### KTD4 — the wording states the choice, not just the identifier

A bare identifier is not an announcement. The line says that this session was *resumed* and which one
it is, so a reader who did not expect a shared gateway's most-recent session learns both facts at
once. Exact wording is settled in implementation against the existing constants near
`NO_SESSION_TO_RESUME`, following their register.

## Risk this unit must clear

**The transcript is append-only and the replay gate compares the pane against the domain projection.**
This unit adds a row through the same `_append` path every other system line uses, so both sides of
the gate's settled-transcript checks — `content_is_complete` and `interface_shows_everything`
(`talaria/replay/gate.py:1380` and `:1382`) — move together. A new row is a row the projection
contains and the pane is therefore expected to show.

**The landing barrier is the ordering hazard.** `_land_session` runs inside `_landing`
(`talaria/ui/app.py:3225`), which holds inbound frames and folds them in arrival order on exit.
The note must be appended such that it precedes the seeded history rather than landing in the middle
of it — the announcement of what arrived belongs before what arrived. AE3 asserts the order.

## Acceptance evidence

- **AE1.** A confirmed `--resume` landing appends exactly **one** system row naming the resumed
  session by its durable id.
- **AE2.** A picker-driven switch appends the same row; a `--new` session appends **none**.
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
uv run pytest tests/domain/ tests/ui/ tests/replay/ -q
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
