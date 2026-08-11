---
title: v0.3 unit B4 — the unknown-event flood, and the latch that stops the next one
type: plan
status: proposed
date: 2026-08-11
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: B4
---

# Unit B4 — `platforms.changed` floods the transcript

The smallest fully-diagnosed unit in v0.3, and the first through the lifecycle, so the loop is proved
on work whose failure mode is cosmetic rather than on the release's hardest change.

**The finding.** Driving v0.2 by hand on 2026-08-10 produced **twenty-six rows in a single turn**,
each reading `unknown event type: platforms.changed`. Recorded as defect 3 in
[the hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md) and carried into
[the v0.3 handoff](2026-08-11-v0-3-session-handoff.md).

## Mechanism — verified by reading, not inferred

Three files, one path, and every line below was checked in the tree at `main` = `6946af9`.

1. `talaria/domain/decode.py:248` — an event whose type is outside `KNOWN_EVENT_TYPES` becomes an
   `UnknownEventFrame` rather than being dropped. This is deliberate and required: R5 says an
   unrecognized type is **surfaced by name**, because the gateway registers far more methods than any
   client uses, so unknown events are expected traffic rather than a defect.
2. `talaria/domain/state.py:1416` — `_apply_unknown_event` appends a transcript entry for **every
   occurrence**. It already keeps a deduplicated tuple of the *types* seen
   (`state.unknown_event_types`), but the row is appended whether or not the type is new.
3. `platforms.changed` is in none of the three sets that make up `KNOWN_EVENT_TYPES`
   (`decode.py:60-116`), so a gateway that emits it twenty-six times in a turn produces twenty-six
   rows.

**So there are two defects here, not one**, and the second is the one worth the unit's time: a type
Talaria does not know floods the transcript in proportion to how often the gateway emits it. Fixing
only the first repairs today's symptom and leaves the mechanism armed for the next unknown type — and
`_OBSERVED_ON_A_LIVE_GATEWAY`'s own comment says plainly that it expects there to be a next one,
describing itself as "the place any future live capture should add to".

## Key technical decisions

### KTD1 — `platforms.changed` joins `_OBSERVED_ON_A_LIVE_GATEWAY`, not either other set

The three sets are kept apart on **provenance**, and that separation is load-bearing rather than
tidiness. `_HANDLED_BY_HERMES_TUI` and `_UNHANDLED_BY_HERMES_TUI` were derived by reading Hermes at a
pinned revision; `_OBSERVED_ON_A_LIVE_GATEWAY` holds what a running gateway was seen emitting that the
reading missed. `platforms.changed` was found exactly that way — by attaching to a live gateway and
watching it arrive — so it belongs to the third set, and the docstring gains its date and provenance
the way the existing three entries carry theirs.

**Rejected:** adding it to the reading-derived sets, which would misrepresent how it was found and
quietly erode the one honest record that a source reading alone did not converge.

### KTD2 — it is named in `AMBIENT_IGNORED_EVENTS`, not left to the fall-through

Once a type is known, `_apply_event` (`state.py:1440-1449`) looks for a handler, then for
`SYSTEM_LINE_EVENTS`, then for `AMBIENT_IGNORED_EVENTS`, and otherwise **returns the state unchanged
anyway**. So the narrow fix appears to work without touching anything else.

Rely on that fall-through and the tree records no intent: a later reader cannot tell "we decided this
event is ambient" from "nobody got to it". `AMBIENT_IGNORED_EVENTS` exists to carry exactly that
distinction, and `tests/domain/test_reconciliation.py:305` already asserts the shape with the comment
"these are known, just not rendered". `platforms.changed` is named there.

### KTD3 — an unknown type is announced once per session, not once per occurrence

The latch already exists in this codebase, one function above the defect. `_apply_protocol_error`
(`state.py:1398`) keeps `protocol_noise_announced` so that "a noisy connection announces itself once
instead of on every frame", re-encoding Hermes's own `protocolWarned` behaviour. Unknown events get
the same treatment, with one difference that matters: for a protocol error the *announcement* is
latched and the per-frame rows continue, whereas for an unknown event **the row is the announcement**,
so the latch is on the row itself, per type.

The state to latch against is already there and already deduplicated — `state.unknown_event_types`.
The change is to append only when the type is new, and to count the repeats:

```python
def _apply_unknown_event(state: SessionState, frame: UnknownEventFrame) -> SessionState:
    if frame.type in state.unknown_event_types:
        return replace(state, unknown_event_repeats=state.unknown_event_repeats + 1)
    return _append(
        replace(state, unknown_event_types=(*state.unknown_event_types, frame.type)),
        "unknown-event",
        frame.text,
    )
```

**R5 still holds in full.** The requirement is that an unrecognized type is surfaced by name rather
than dropped; it is surfaced by name, once, and the recurrence is counted rather than discarded.

**Rejected — rewriting the existing row to carry a count.** The domain transcript is strictly
append-only and entry text is immutable, which the 2026-08-03 reconciliation work established by
measurement and which the transcript pane's whole prefix-matching design depends on. A counter that
edits a committed entry would reopen that, for a cosmetic gain.

**Rejected — a periodic summary line.** More machinery than the finding warrants, and it puts a second
kind of row on a transcript this release is separately trying to make less busy.

### KTD4 — the repeat count is state, not a rendered line

`unknown_event_repeats` is a new integer on `SessionState`, in the same family as
`protocol_error_count` and `cross_session_events_ignored`, both of which are counted without being
rendered per occurrence. Nothing in this unit renders it. It exists so that a future diagnostic can
answer "how much did we suppress" without the suppression having thrown the answer away.

## Risk this unit must clear, named before it is built

**The replay gate's `interface_shows_everything` check must stay green, and the reason it will is
worth stating rather than assuming.** That check compares what the pane rendered against what the
domain projected. Both sides derive from the same `SessionState`, so a row the projection no longer
contains is a row the pane is no longer expected to show — the two move together. This is a
prediction, and AE4 below is what turns it into evidence.

The second-order risk is a test that asserts the *old* behaviour. `tests/domain/test_normalize.py:65`
asserts `unknown_event_types == ("cauldron.bubbled",)` after an unknown event, which the change
preserves. Any test asserting one row per occurrence is the behaviour this unit deliberately changes,
and it is updated with a comment naming this plan rather than quietly edited.

## Acceptance evidence

- **AE1.** A feed containing `platforms.changed` produces **zero** unknown-event rows, and
  `state.unknown_event_types` does not contain it — it is known now, not merely quiet.
- **AE2.** A feed containing the same *genuinely* unknown type twenty-six times produces **exactly one**
  transcript row naming that type, and `unknown_event_repeats == 25`.
- **AE3.** A feed containing two distinct unknown types produces **two** rows, one per type, in arrival
  order. The latch is per type, not global.
- **AE4.** The replay gate runs green over the existing gate corpus — `uv run talaria gate --corpus
  <recording> --deltas 50000` exits zero, and `interface_shows_everything` is true.
- **AE5.** The project check is clean: `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check`.

AE2 and AE3 are new tests. AE1 needs a fixture carrying `platforms.changed`; the existing
`tests/domain/` fixtures show the shape and no live capture is required to build one.

## Verification

```bash
uv sync --all-groups
uv run pytest tests/domain/ tests/replay/ -q
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched.

## What this unit does not do

- **It does not decide whether `platforms.changed` should be rendered as something useful.** It is
  ambient by this plan's own KTD2. If it later turns out to carry something an operator wants, that is
  a different unit with its own evidence.
- **It does not touch the transcript pane.** The change is entirely in the domain layer, which is what
  keeps it clear of ADR-0006 and of the mixed-height layout that unit A3 is separately diagnosing.
- **It does not surface the repeat count anywhere.** KTD4 records the count; rendering it is not part
  of this unit and needs a reason of its own.
