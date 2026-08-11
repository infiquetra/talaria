---
title: v0.3 diagnosis checklist — the four undiagnosed items, and what each one decides
type: checklist
status: ready-for-operator
date: 2026-08-11
source: docs/plans/2026-08-11-v0-3-orchestration-charter.md (units E1–E4)
---

# Operator checklist — the v0.3 diagnosis pass

This is the deliverable of the charter's precondition. **No agent can execute it.** Every item lives
in the seam that produced every v0.2 defect in the first place: a real terminal, on a real desktop,
driven by a real person.

**Reduced 2026-08-11, before it was ever run.** The first draft opened with a probe for what claims
`F1`. That probe is now optional, because the answer stopped deciding anything: the operator's own
notes already record `F1` and `F2` as eaten by the desktop and record an explicit priority — the
focus key is not important for v0.3 — and the charter's spine A has been reframed away from making a
jump key arrive. Step 1 is kept as a ten-second confirmation, marked optional, and nothing waits on
it. **What remains is about fifteen minutes.**

A step that produces a **negative** result is a completed step. The failed reproduction already
recorded on 2026-08-10 is worth more than silence, and it is what makes step 4 sharper this time.

## Preconditions

- Talaria v0.2.0 as installed, or a build of `main` — the two are identical under `talaria/`.
- A live Hermes gateway for steps 1, 3 and 4. Steps 2 and the first half of step 3 need only an
  existing recording and `talaria replay`.
- A throwaway session for steps 3 and 4, discardable afterwards.
- For step 1 and step 3, an outstanding approval, which means `approvals.mode` set to `manual` on the
  profile in play. **Restore it afterwards and verify by re-reading the file** — this is the same
  dance the 2026-08-10 drive performed and correctly recorded.

## What to hand back, for every step

- **Verbatim on-screen text, never a paraphrase.** The wording is frequently the finding.
- **Recordings cited by digest and frame count, never by path** — the repository's own rule (R29).
- **Nothing operator-specific.** No profile name, host name, or file path. This repository is public.

## Steps

### 1. What claims `F1` — OPTIONAL, ten seconds, nothing waits on it (unit E1)

**Run this only if you are at the keyboard anyway.** The charter no longer depends on the answer:
spine A has moved from "make the jump key arrive" to "the card is answerable where your hands already
are", so the cause of the `F1` loss changes no estimate. What the measurement still buys is a clean
line in the record — the notes currently say the key "appears to be" a desktop hotkey, and this turns
an appearance into a measurement.

The binding is not the suspect: `talaria/ui/app.py:770` binds `F1` to the jump with `priority=True`,
the identical form used by `F8`, `F9` and `F10` on the three lines below, and `F8` and `F9` were both
driven successfully in the same session. So this step asks a narrower question — **does the keypress
ever reach the terminal at all?**

**1a. Outside Talaria, in the same terminal emulator**, run `cat -v`, press `F1` once, then press
`ctrl-c`. Record exactly what appeared: `^[OP`, `^[[11~`, something else, or **nothing at all**.

**1b. Repeat 1a holding `fn`** — `fn`+`F1`. Record what appeared, separately from 1a.

**1c. Record the desktop's own setting.** In System Settings → Keyboard, whether "Use F1, F2, etc.
keys as standard function keys" is on or off, and whether any per-application override exists for
this terminal under Keyboard Shortcuts → Function Keys. On a Mac keyboard `F1` is brightness-down
unless that setting is on or `fn` is held, and that is the leading suspect.

**1d. Only if 1a or 1b produced bytes:** launch Talaria against the live gateway with an approval
outstanding and press whichever form produced them. Record whether focus moved to the card.

**What each outcome means, so the result is not over-read:**

| Result | What it means | What it settles |
| --- | --- | --- |
| No bytes in 1a or 1b | The desktop owns the key; it never reaches any program | Confirms the notes, and confirms the whole function-key row is the right thing to re-decide (unit A4) |
| Bytes in 1b only | The key arrives only with `fn`; the default desktop mapping claims the bare press | Same conclusion, with the mechanism named exactly |
| Bytes in 1a, and 1d moves focus | The key works, and something else went wrong on 2026-08-10 | Worth knowing, changes nothing — the operator has already ruled the jump key not worth keeping |
| Bytes in 1a, and 1d does **not** move focus | The loss is inside Talaria, not the desktop | A real Talaria defect that would otherwise go unrecorded; file it separately, still outside spine A |

Also record the terminal emulator's name and version. The repository's platform matrix already lists
terminal hosts, so this belongs in the evidence.

### 2. Whether `F5` is alive (unit E2)

Thirty seconds, no gateway needed. `F5` was pressed on 2026-08-10 at the bottom of a paused replay,
where "follow the newest line" is a legitimate no-op — so the observation proved nothing either way.

Replay any existing recording, **scroll up** until the newest line is off screen, then press `F5`.
Record whether the view returns to the newest line.

### 3. `F4` and `F10`, pressed deliberately (unit E3)

Neither key has ever been pressed. `F10` is safe; `F4` is not, and the order below reflects that.

**3a. `F10` under replay.** Press it and record what changes on screen — the shipped documentation
says `F9` and `F10` change replay speed.

**3b. `F4` in a throwaway live session, knowingly.** `F4` is bound to interrupt
(`talaria/ui/app.py:776`), and it sweeps outstanding prompts only *after* the interrupt is confirmed
(`:1666`). **It stops a running turn.** Submit a prompt long enough that it is still streaming, leave
one prompt outstanding if you can, then press `F4` once.

Record the sequence **in order**: whether the turn stopped, whether a confirmation appeared and what
it said verbatim, and whether outstanding prompts were swept afterwards. That ordering is the whole
finding — the published release notes describe only the second half.

### 4. The duplicated-content sighting, one more attempt (unit E4)

Sighted twice in different sessions. The recorded reproduction attempt **failed**: streamed deltas
measured 1,950 characters against `message.complete`'s 1,948, and each heading appeared exactly once.
That negative result narrows this attempt to one specific shape.

Under `talaria --record`, drive **one turn in which the model speaks, calls a tool, and then continues
speaking** — the shape the failed attempt did not cover. Watch for a repeated heading, paragraph, or
block.

Hand back the recording's digest and frame count, and say plainly which happened: duplication seen, or
not seen. **Not seen is a result**, and it is the one that should be recorded loudest, because two
negative attempts against different shapes start to argue the sighting was something else.

## After the run

Hand the captures back and the charter's units get sized against them. Nothing in spines A, B or C is
estimated on a plausible reading of the code before this checklist has been run — and a checklist that
has not been run is not progress against the items it covers.
