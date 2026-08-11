---
title: v0.3 unit B3 — a keypress that did something is distinguishable from one that did not
type: plan
status: proposed
date: 2026-08-11
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: B3
---

# Unit B3 — a keypress that did something is distinguishable from one that did not

The charter states this unit in one line (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:120`),
so the planning work includes establishing the scope, not only the fix. The release theme at its
sharpest: **Talaria does not confirm what it just did.**

An operator presses a key and cannot tell which of four different things happened. They are not the
same case and this plan does not treat them as one:

1. **A key Talaria never received** — eaten by the desktop before the app saw it. Talaria cannot
   confirm what it never saw; this case belongs to unit A4, which re-decides the function-key row
   (charter `:98-101`) so the keys land on bindings the desktop does not eat.
2. **A key Talaria received and deliberately declined** — already covered: every declined keypress
   in the current bindings says why, out loud, through the composer notice bar (census below).
3. **A key Talaria acted on where the effect is invisible** — the defect this unit repairs.
4. **A key that acted correctly and visibly, which needs no feedback at all.** Adding a
   confirmation here is noise on a release that is separately trying to make the transcript
   quieter (unit B4 just shipped the flood fix).

**The gaps are four, enumerated from the bindings, not generalized.** F1 with nothing outstanding,
F5 when already following (shared with the `end` key), F2 when there are no sub-agents, and the
session-picker landing that changes nothing — the last one handed to this unit by name from unit
B5's merged plan.

## Mechanism — verified by reading, at `main` = `fcab675`

The app binds eleven keys (`talaria/ui/app.py:761-790`). A census of every binding — plus the
picker row selection that unit B5's KTD3b hands over — with the feedback each one has today:

| Binding            | Action method                            | Feedback today                                                                                          | B3    |
| ------------------ | ---------------------------------------- | ------------------------------------------------------------------------------------------------------- | ----- |
| `ctrl+q`           | Textual's `quit`                         | visible — the app exits                                                                                 | no    |
| `f1`               | `action_jump_to_prompt` (`app.py:1409`)  | declined by a modal: notice (`app.py:1425-1427`); nothing outstanding: **silent** (`app.py:1428`)       | yes   |
| `f8`               | `action_toggle_pause` (`app.py:1391`)    | notice in both modes — the replay pacing state (`app.py:1395`, text from `_pacing_notice` at `:1366-1374`), or the live refusal (`_pacing_refused_live` at `:1376-1390`) | no    |
| `f9`               | `action_slow_down` (`app.py:1403`)       | same; the notice re-states the speed even at the `MIN_SPEED` clamp (`talaria/replay/controls.py:31`)    | no    |
| `f10`              | `action_speed_up` (`app.py:1397`)        | same, at `MAX_SPEED`/unbounded (`controls.py:32`, `:37`)                                                | no    |
| `f2`               | `action_toggle_agents` (`app.py:1430`)   | visible when rows exist; **invisible when the region is empty** (`talaria/ui/agents.py:119-122`)        | yes   |
| `f3`               | `action_toggle_palette` (`app.py:2769`)  | visible both ways — the region shows or hides, and shows at least its header line                       | no    |
| `f4`               | `action_interrupt` (`app.py:1436`)       | notice in both modes — replay refusal (`app.py:1443-1445`) or the gateway's outcome (`app.py:1660`)     | no    |
| `f5`               | `action_follow_bottom` (`app.py:1433`)   | visible when scrolled up; **silent when already following**                                             | yes   |
| `f6`               | `action_toggle_picker` (`app.py:2774`)   | dialog opens, or a named refusal — two on the models path (`app.py:2809`, `:2812`), four across both pickers (`:2809`, `:2812`, `:2818`, `:2821`) | no    |
| `f7`               | `action_toggle_profiles` (`app.py:2777`) | same shape (`app.py:2818-2821`)                                                                         | no    |
| (picker row enter) | `switch_session` (`app.py:3700`)         | dialog refuses the marked current row inline; the unmarked-current landing is **silent** (`app.py:3307`) | yes   |

**The four silent paths, cited exactly:**

1. **F1 with nothing outstanding.** `action_jump_to_prompt` declines out loud while a modal holds
   the screen (`app.py:1425-1427`, constant `JUMP_BLOCKED_BY_MODAL` at `app.py:278`), then
   delegates to `focus_first_unanswered()` (`app.py:1428`) and discards its return value. That
   method is documented "A no-op — returns `False`, moves nothing — when no card is mounted"
   (`talaria/ui/prompts.py:995`, returning `False` at `prompts.py:1017`). The keypress lands,
   nothing moves, nothing is said. This is the shape of charter item E2's ambiguity
   (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:52-54`) for the jump key.
2. **F5 when already following.** `action_follow_bottom` calls `self.transcript.follow_bottom()`
   (`app.py:1433-1434`), which sets `follow = True` and scrolls to the end
   (`talaria/ui/transcript.py:1717-1719`). When `follow` is already true, nothing changes and
   nothing is said — precisely the observation charter E2 records: "pressed at the bottom of a
   paused replay, where re-following the newest line is a legitimate no-op, so the observation is
   ambiguous rather than negative." The `end` key reaches the identical call through the app's raw
   key handler (`app.py:4316-4317`), so it shares the gap and must share the fix.
3. **F2 with no sub-agents.** `action_toggle_agents` awaits `toggle_collapsed()`
   (`talaria/ui/agents.py:194-199`), which flips the `collapsed` flag. With no rows, the region
   stays `display: none` — it only renders when `-populated`, and that class is set from
   `bool(view.rows)` (`agents.py:119-122`, `:158-161`). The flag flip is real but invisible: the
   operator cannot tell the keypress registered, nor see the collapsed state it sets for the next
   fan-out.
4. **The landing that changes nothing.** `_land_session` seeds history only when the focus moved —
   `if previously_focused != raw:` (`app.py:3307`, comparison value captured at `:3301`). The
   other branch has no body at all: the transcript is retained (correctly — seeding re-appended
   the same history a second time), and nothing is announced. Unit B5's merged plan names this
   branch and hands the feedback need to this unit — see KTD4.

Two non-binding key paths were examined. One stays out of scope and the other is out of scope for a
different reason than this plan first gave.

**`pageup` and `home` perform no scroll at all, and the first draft of this plan said they did.**
`app.py:4314-4315` calls only `self.transcript.hold_anchor()`, which sets `self.follow = False`
(`talaria/ui/transcript.py:1714-1715`) and nothing else. Their sibling in the same handler is the
contrast that proves it: `end` reaches `follow_bottom()`, which calls `scroll_end`
(`transcript.py:1717-1719`). The transcript pane has no page-key handling of its own and is never
focused — the app's own comment at `app.py:4311-4313` says the composer holds focus — so nothing
scrolls the view. The only test that presses `pageup`, `test_end_and_pageup_toggle_the_anchor`
(`tests/ui/test_transcript_bounds.py:159-169`), asserts only that the flag flipped.

By this unit's own standard those keys are silent, and at the bottom of a paused replay they are
silent forever, because no new content will arrive to reveal that the anchor is held. **They are
excluded anyway, on different ground: this is a navigation gap, not a feedback gap.** A notice
saying "scroll position held" would confirm a keypress whose advertised effect never happens, which
is a worse outcome than the silence — it would make a broken control look like a working one. The
missing scroll belongs to spine A's mis-aimed-input family alongside unit A3, and is filed as its
own defect rather than papered over here. Mouse scroll up reaches the same `hold_anchor`
(`app.py:4306-4307`) but is genuinely fine, because the scroll wheel moves the view itself.

Every key inside the modal picker is already answered —
movement highlights, typing filters, and an unselectable row is refused in the dialog's own
refusal line (`talaria/ui/dialog.py:389-404`, prefix at `dialog.py:70`) — which is the standard
this unit asks the app bindings to meet.

## Key technical decisions

### KTD1 — the feedback rides the composer notice, not a transcript row

The composer notice is the surface the codebase already uses for every keypress refusal it has:
`JUMP_BLOCKED_BY_MODAL` (`app.py:278`, shown at `app.py:1426`), the live-pacing refusal
(`LIVE_HAS_NO_REPLAY_CLOCK` at `talaria/domain/commands.py:426-428`, shown through
`_pacing_refused_live` at `app.py:1376-1390`), the replay mutation refusal (`_refuse_mutation` at
`app.py:1443-1445`), and the picker-open refusals (`app.py:2809-2821`). A no-op keypress is the
same kind of fact — transient, about this moment, not about the session.

Unit B5 makes the mirror-image call and says so: it chose the transcript for the durable identity
fact of which session arrived, and explicitly **rejected** the composer notice for it because "the
question 'which session am I in?' is asked minutes after arrival, when the notice is long gone"
(unit B5 plan, KTD1,
`docs/plans/2026-08-11-v0-3-unit-b5-resumed-session-names-itself.md:46-59`). The two calls are
consistent, not contradictory: durable facts go to the durable surface, transient facts to the
transient one. B5's KTD3b names this unit "the right home for transient feedback" — this is that
home.

**Rejected — a transcript row instead.** The row would be durable feedback for a fact that is not
durable: "your keypress did nothing" is not part of the session's story. The cost is concrete:
repeated presses stack rows; a transcript this release is separately quietening (unit B4) grows by
one line per stray F5; and the row answers the wrong question — "what happened in this session"
rather than "did my key register". The notice's own cost is stated rather than ignored: the bar is
one shared line, and a notice overwrites whatever was there. Every existing refusal already pays
that price, and a no-op confirmation is the least valuable message the bar can hold — losing it to
a later message loses nothing.

### KTD2 — the scope is the four enumerated silent paths, and nothing else

The census table above is the scope. A rule that covers "every action" is a rule nobody can
verify; this plan names four code sites and each gets its feedback at the site where its silence
lives:

- `action_jump_to_prompt` (`app.py:1409`): read the `bool` that `focus_first_unanswered()` already
  returns and notice when it is `False`.
- `action_follow_bottom` (`app.py:1433`): notice when `self.transcript.follow` is already true
  before the call. The `end` branch of the app's raw key handler (`app.py:4316-4317`) must reach
  the **same** rule rather than growing a second copy — the `_pacing_notice` docstring states why
  two renderings of one fact drift (`app.py:1366-1374`), and the fix is to route both entry points
  through one method.
- `action_toggle_agents` (`app.py:1430`): notice when the region has no rows to show or hide. The
  read needs a public way to ask `AgentRows` whether it is populated — the check exists
  (`bool(view.rows)` at `agents.py:161`) but is private to `apply`; the plan adds a small public
  read rather than reaching into `_view`.
- `_land_session`'s retain branch (`app.py:3307`): the branch gains a body — see KTD4.

Every other binding already confirms itself (table above) and is not touched. The toggle still
flips its flag when empty, the jump still moves nothing, the retain branch still re-seeds nothing:
**this unit changes no behaviour, only silence.** That is the scope discipline the finding's
one-sentence charter demands.

### KTD3 — a declined keypress names its reason, in the house register

"Only that it was seen" is not enough: the operator who pressed F1 wants to know there is nothing
to jump to, not merely that Talaria is listening — the second fact is the one they doubted. The
existing refusals set the register: full lowercase sentences that name the reason, sometimes with
the consequence ("close the picker first, then jump to the prompt", "governs a recorded replay's
pacing, and this session is live — nothing changed"). New lines follow it; exact wording settles
in implementation against the constants they sit beside, but the shape is decided here:

- F1, nothing outstanding: names that no prompt is waiting — the absence *is* the answer.
- F5/`end`, already following: names that the newest line is already followed.
- F2, no rows: names that there are no sub-agents to show or hide.
- retain branch: reuses the picker's own words — see KTD4.

**Rejected — one shared "nothing happened" string.** It would satisfy "distinguishable from one
that did something" and fail the operator's actual question, which differs per key; and a shared
string read back later says nothing about which key was pressed.

### KTD4 — the B5 KTD3b case is covered, split along the line B5 drew

Unit B5's merged plan hands this unit a case: "The operator who picks the row they are already on
still deserves to know the keypress registered — that is unit B3 ... and it is the right home for
transient feedback" (B5 plan, KTD3b,
`docs/plans/2026-08-11-v0-3-unit-b5-resumed-session-names-itself.md:122-139`). Reading the current
tree shows the case splits in two, and the plan covers both halves:

**The marked row already confirms itself, in the dialog.** The session picker marks the current
row and makes it unselectable — `selectable=not row.is_current`, refusal
`SESSION_ALREADY_FOCUSED = "already the focused session"` (`talaria/ui/picker.py:570`,
`:615-630`) — and pressing enter on it shows that refusal in the dialog's own refusal line
(`dialog.py:389-404`). That feedback shipped in v0.2 and satisfies the KTD3b case for the row the
picker recognizes; this unit asserts it still works (AE6) rather than rebuilding it.

**The unmarked row lands silently, and that is the half this unit repairs.** The marking compares
the listing's ids against `state.session_key` (`picker.py:564`, fed from
`self.state.session_key or ""` at `app.py:3646`), and the durable key can be absent —
`_land_session` itself tolerates a reply carrying none of the three keys it falls through
(`app.py:3290-3294`, passed as `None` at `:3305`) — so the focused session's row can be
unmarked, chosen, and resumed. The gateway then answers with the session already focused,
`_land_session` takes the retain branch (`app.py:3307`), the dialog closes, the caret returns
to the composer, and nothing is said. That branch gains the feedback: the same words the dialog
uses for the marked row, reusing the `SESSION_ALREADY_FOCUSED` constant so one fact has one
voice on both surfaces.

The two halves compose with unit B5's implementation when it merges: B5 adds a transcript row in
the **seed** branch (focus moved), this unit adds a transient notice in the **retain** branch
(nothing moved).

**The invariant the composition depends on, stated rather than assumed.** B5's row must be appended
*after* the `if previously_focused != raw:` line — inside the seed branch — and this unit's notice
*inside* the retain branch. Only then are the two mutually exclusive. This is worth pinning because
B5's own KTD3a expresses its insertion window as "after `land_session` and before `seed_history`", a
range that spans that `if`: an implementer placing the row one line early would fire it on the retain
branch as well, double-firing with this unit's notice and contradicting B5's own KTD3b and AE2a.

**Verified, not predicted.** B5 was implemented while this plan was under review, and the row landed
inside the seed branch — `feat/v0-3-unit-b5`, commit `28665d2`, guarded by `if outcome.method ==
RESUME_METHOD:` within the moved-focus block. The invariant holds today. It is written down anyway,
because the next change to either function is where it would quietly stop holding.

### KTD5 — feedback is tied to the action method, never to the key, because spine A is rewriting the key set

Unit A4 re-decides the whole function-key row (charter `:98-101`), and decision D9 has already
settled the shape: the card takes focus on mount, "**the jump key is not replaced, it is
removed**", and A4 re-homes the keys with no on-screen anchor — the sub-agent toggle and the
replay controls (`docs/plans/2026-08-11-v0-3-decision-log.md:144-163`). This plan therefore
assumes **nothing about which keys will carry these actions**, and puts every notice inside the
action method the binding invokes. Three consequences, stated so the merge is not a surprise:

- If A1/A4 removes `action_jump_to_prompt` — D9 says the jump goes away once the card owns focus —
  the F1 notice goes with it. That is the correct outcome, not lost work: the need (a jump with
  nothing to jump to) dies with the key. Until then, F1 confirms itself.
- If A4 moves `action_toggle_agents` to a chord or a click affordance, the no-rows notice follows
  the action — same method, same feedback, new key.
- If unit A1's click affordance invokes the same actions, it inherits the same feedback for free.

The alternative — keying feedback to the binding — would silently break on whichever merge lands
second, which is exactly the failure the brief names: a plan that depends on bindings another unit
is rewriting.

### KTD6 — the replay gate cannot see this change, because the gate never drives these paths

Checked rather than assumed. The two settled-transcript checks are `content_is_complete`
(`talaria/replay/gate.py:1023`), which walks the domain's committed entries against the rendered
projection, and `interface_shows_everything` (`gate.py:996`), the transcript pane's ownership
proof — called at the settled checkpoint at `gate.py:1380` and `:1382`. Neither reads the composer
notice, and this unit writes nothing into the transcript, so the checks have no surface to
disagree about even in principle.

The path also does not run under the gate. `measure_replay`, the function holding both call
sites, drives no keys at all; the only keypress anywhere in the gate is `enter` inside
`exercise_inert_controls` (`gate.py:1449`, pressed at `gate.py:1460`), which reaches the replay
submit refusal, not any B3 path. The sideband actions are only `confirmed_cancel` and
`typed_disconnect` (`gate.py:1122-1134`). No gate path calls `action_jump_to_prompt`,
`action_follow_bottom`, `action_toggle_agents`, or `switch_session`, and the gate constructs
`TalariaApp(..., mode="replay")` with no startup selection (`gate.py:1217`, `:1433`, `:1454`), so
`open_session` and `_land_session` never run under it either — the same corrected reasoning unit
B5's plan landed on for its own announcement.

## Risk this unit must clear

**The shared notice bar.** A B3 notice overwrites whatever the bar held. This is the accepted cost
of every existing refusal and is cheapest here: the overwritten message is always more recent or
more actionable than "your keypress did nothing", and the `_clear_paste_notice` precedent
(`app.py:4286-4303`) shows the bar's sharing rules are already managed deliberately.

**The spine A merge.** Bounded by construction (KTD5): B3 edits the bodies of three action methods
and adds one branch to `_land_session`; A4 edits the bindings table and may delete
`action_jump_to_prompt` outright. The only overlapping symbol is that method, and the resolution
is decided in advance — if A4 removes it, B3's F1 notice is dropped with it.

**Two entry points, one fact.** F5 and `end` share `follow_bottom`; implementing the rule twice
would drift, the way `_pacing_notice`'s own docstring records two renderings drifting
(`app.py:1366-1374`). One method carries the rule (KTD2).

**Behaviour preservation is the real regression risk.** The unit must not change what the silent
paths *do* — the collapsed flag still flips on an empty F2 (it decides how the next fan-out
arrives, which is somebody's muscle memory even if it is invisible today), the retain branch still
seeds nothing, the jump still moves nothing. AE1 through AE5 assert the unchanged behaviour
alongside the new notice so a fix that accidentally becomes a behaviour change fails loudly.

**What breaks, named rather than discovered.** Two tests encode today's silence and change:
`test_f1_with_nothing_outstanding_is_a_no_op` (`tests/ui/test_focus_returns.py:312-327`), whose
focus-and-text assertions survive but which gains the notice assertion this unit exists for, and
`test_landing_the_already_focused_session_does_not_reseed_history`
(`tests/ui/test_sessions.py:537-579`), whose canary assertion survives and gains the retain-branch
notice half. Both are updated with a comment naming this plan, not quietly edited. Everything
nearby survives untouched, and the reasons are the record: the modal-refusal test
(`tests/ui/test_focus_returns.py:330-380`, notice asserted at `:379`) already expects its notice;
`test_following_the_bottom_and_holding_an_anchor_are_different_states`
(`tests/ui/test_transcript_bounds.py:131-154`) calls `follow_bottom` on the pane, below the app
level where the feedback lives; `test_end_and_pageup_toggle_the_anchor`
(`tests/ui/test_transcript_bounds.py:159-169`) presses `end` only with follow off — the silent
half; `test_the_count_survives_collapsing` (`tests/ui/test_agent_rows.py:53-74`) toggles with rows
present; `test_the_region_hides_itself_when_there_are_no_sub_agents`
(`tests/ui/test_agent_rows.py:93-101`) presses nothing; the domain-level retain test
(`tests/domain/test_transcript_state.py:449`) is below the UI entirely; and the pacing and
interrupt key tests (`tests/replay/test_controls.py:181-193`,
`tests/ui/test_live_wiring.py:317-336`) touch bindings this unit does not change.

## Acceptance evidence

- **AE1.** F1 with nothing outstanding shows exactly one composer notice naming that no prompt is
  waiting, leaves the focus and the composer text exactly as they were, and appends **no**
  transcript row. Presence, two absences, all asserted.
- **AE2.** F1 while a modal picker is open still shows `JUMP_BLOCKED_BY_MODAL` and moves nothing —
  the existing refusal is a regression guard, asserted unchanged.
- **AE3.** F5 (and `end`) while already following shows the notice and `follow` remains true;
  while **not** following, shows **no** notice and `follow` becomes true. Both halves asserted —
  the silence on a visible scroll is a requirement, not an omission.
- **AE4.** F2 with no sub-agent rows shows the notice and the region stays hidden; with rows
  present shows **no** notice and the collapse behaviour is unchanged. Both halves asserted.
- **AE5.** Landing the already-focused session through `switch_session` shows a notice carrying
  the `SESSION_ALREADY_FOCUSED` wording, seeds **no** history (the existing canary holds), and
  appends **no** transcript row. Landing a **different** session shows **no** such notice. Both
  halves asserted; this is the B5 KTD3b handoff, covered per KTD4.
- **AE6.** The picker's inline refusal of the marked current row is unchanged — enter on it still
  shows the dialog's `cannot select — already the focused session` line. The two surfaces agree on
  one fact, and the agreement is asserted.
- **AE7.** The replay gate runs green over the existing gate corpus, with both `content_is_complete`
  and `interface_shows_everything` true (`talaria/replay/gate.py:1380`, `:1382`). The corpus is
  named by digest and frame count, never by path.
- **AE8.** The project check is clean: `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check`.

**Acceptance for a person, per the charter's evidence rule 2**
(`docs/plans/2026-08-11-v0-3-orchestration-charter.md:182-186`): at the bottom of a paused replay —
charter E2's exact ambiguity — the operator presses F5 and can say whether the key registered;
with nothing outstanding, F1 says so instead of being silent. That is operator-only and is not
claimed on test evidence.

## Verification

```bash
uv sync --all-groups
uv run pytest tests/ui/ tests/domain/ tests/transport/ tests/replay/ -q
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched.

## What this unit does not do

- **It does not rebind, remove, or add any key.** The function-key row is unit A4's decision, and
  D9 records the scheme already chosen (`docs/plans/2026-08-11-v0-3-decision-log.md:144-163`).
- **It does not fix keys Talaria never receives.** A key eaten by the desktop cannot be confirmed
  by the app that never saw it; A4 moves those keys where they arrive.
- **It does not add feedback where feedback already exists** — the pacing keys, interrupt, the
  picker-open refusals, the modal picker's own key handling, or a key whose effect is visible.
  Confirmation of a visibly-working key is noise, and the transcript is being quietened elsewhere.
- **It does not change any behaviour of the silent paths.** No flag semantics change, no branch
  re-seeds, no focus moves differently. Feedback only.
- **It does not announce session landings.** That is unit B5's transcript row, a separate
  mechanism in a disjoint branch; this unit's notice lives only where B5's announcement does not.
- **It does not take a position on the caret status row.** That is unit B1's surface and its
  decision.
