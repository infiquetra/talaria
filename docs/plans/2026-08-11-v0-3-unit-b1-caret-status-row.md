---
title: v0.3 unit B1 — the caret status row says something an operator can interpret, or it goes away
type: plan
status: proposed
date: 2026-08-11
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: B1
---

# Unit B1 — the caret status row says something an operator can interpret, or it goes away

**The row is removed, and the requirement it served is replaced, not dropped.** There is no narrower
snap-back rule to find: the only rule that returns focus to the composer without breaking the
deliberate moves is the take-away rule already implemented, and this plan says so from the code
rather than inheriting the charter's "A narrower rule may work" as an open question. What remains is
the reword-versus-remove decision, and removal wins because the row's one load-bearing case — the
invisible transcript-pane focus — is better served by a transient notice that fires when a printable
key is about to be discarded than by a continuous row the operator could not read.

The operator's complaint is recorded in
[the hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md), note 4 (`:184-209`): driving v0.2,
they hit a bottom row reading `caret: transcript` and said:

> its the wording I think and more importantly... I as the user had no idea why it was there or even
> what caret corresponded to. to be frank, I think it might just be prudent for the focus to always
> go back to the input box.

That is a design question, not a defect — the row is doing exactly what requirement R5 built it to
do. The analysis that follows the note draws the same line: "caret" is developer vocabulary, the
`key: value` shape reads like debug output, and "A status row nobody can interpret is not a status
row." (hands-on notes `:454-459`). It also names the tension this plan resolves: focus moves away
from the composer on purpose, so an unconditional snap-back breaks the answerability spine
(`:464-471`).

## The requirement the row serves

R5 is written down in
[the v0.2 answerability plan](2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md),
at `:53-55`, as:

> **R5.** Whenever the caret is not in the composer, the status region names what holds it.

The mechanism decision behind it is KTD5 in the same plan (`:151-159`): a **dedicated** fixed-height
non-wrapping slot inside `StatusRegion`, mounted unconditionally so writing into it never changes the
region's height. The plan that implemented it, unit U3 (`:288-312`), watches focus changes at the app
level and writes `caret: prompts` / `caret: transcript` / `caret: agents` otherwise, empty when the
composer holds the caret. A plan that removes the row must say what happens to R5; this plan replaces
the requirement rather than silently dropping it.

## Mechanism — verified by reading, at `main` = `ec9abfd`

**The slot.** `talaria/ui/status_region.py` reserves a dedicated `Static` with `classes="status--caret"`
(`:44-59`, `:68-72`), mounted unconditionally at `height: 1` so its presence never changes
`StatusRegion`'s height. `set_caret` (`:88-102`) writes `f"caret: {location}"` only when `location` is
non-empty and clears the slot otherwise. The slot is always present, which means the status region
always renders a first row — blank when the composer holds the caret, `caret: X` otherwise.

**The wiring.** `talaria/ui/app.py:4130-4139` maps the three region ids to their location words;
`_caret_location` (`:4141-4149`) walks `focused.ancestors_with_self` and returns the first matching
word, or `""` for the composer and for nothing-focused; `_refresh_caret_slot` (`:4151-4170`) writes it
into the slot, and is called from `on_descendant_focus` and `on_descendant_blur` (`:4172-4176`).

**Every away-state, enumerated from the focus moves that exist today.** Focus leaves the composer only
in these cases, and each is either deliberate or by design:

1. **A prompt card's control.** The F1 jump (`action_jump_to_prompt` at `app.py:1426-1445`)
   delegates to `focus_first_unanswered` (`talaria/ui/prompts.py:995-1017`), which lands on the card's
   action widget. The jump is deliberate — the code's own KTD1 says "a keypress this explicit is
   allowed to move the caret even mid-word" (`prompts.py:1006-1008`). An input-backed card also
   auto-focuses at mount, guarded by `focus_new=not self.composer.text.strip()` (`app.py:1356`,
   `prompts.py:1171-1172`).
2. **The transcript pane.** Tab or a click lands the caret on `TranscriptPane`, a `VerticalScroll`
   (`talaria/ui/transcript.py:629`). This pane has **no focus styling** — its `DEFAULT_CSS`
   (`transcript.py:632-654`) contains no `:focus` rule, so nothing on the pane itself says it holds
   the caret. The test suite states this directly: "The transcript pane gives no visual sign of
   holding the caret on its own" (`tests/ui/test_focus_returns.py:186-188`).
3. **A sub-agent row.** A click (or tab) lands on an `AgentRow` with `can_focus = self.interruptible`
   (`talaria/ui/agents.py:98`). Unlike the transcript, an interruptible row **is** visibly focused —
   `AgentRow.-interruptible:focus { background: $accent 20%; }` (`agents.py:127-128`).
4. **The prompt region container itself.** `PromptRegion` is a `VerticalScroll`
   (`talaria/ui/prompts.py:947`), and `VerticalScroll` is focusable — `talaria/ui/focus.py:9-14`
   states it plainly ("Both regions are `VerticalScroll`, which sets `can_focus = True` so arrow keys
   scroll — a scroll container therefore accepts the caret, discards every printable key sent to it,
   and draws no caret anywhere"). The test suite says so too: the `test_answering_an_approval_by_clicking_hands_the_caret_back` docstring (`tests/ui/test_focus_returns.py:64-68`) — "Textual hands the caret to the enclosing `PromptRegion` here, which is focusable so that arrow keys scroll it and which silently drops every printable key it is given." A probe walking the focus chain from the composer shows `tab 1: TranscriptPane#transcript`, `tab 2: PromptRegion#prompts`, `tab 3: Button#choice-0`. With focus on `PromptRegion`, pressing `z` reaches `TalariaApp.on_key` and the composer text stays empty: the key is discarded. Two tabs from the composer is all it takes.

Case 1 is announced by the card itself: a focused prompt control is visibly focused (R2,
`docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md:35-36`), and decision D9
makes the card own focus when it mounts and name its keys
(`docs/plans/2026-08-11-v0-3-decision-log.md:162-181`). Case 3 is announced by the row's own tint.
**Cases 2 and 4 — the transcript pane and the prompts container — are the away-states with no
announcement of their own, and case 2 is the exact state the operator met and could not read.**

**The return path.** When a control that holds the caret is taken away — a card is answered or
declined (`prompts.py:1187-1188`), a sub-agent row stops being interruptible (`agents.py:96-97`), or
F2 collapses every row (`agents.py:191-192`) — the region posts `CaretReleased`
(`talaria/ui/focus.py:37-46`), and the app answers it by focusing the composer
(`app.py:4180-4195`). This is the narrower rule the charter wonders about, and it already exists: the
caret goes back to the composer whenever Talaria takes a control away, and operator focus moves are
untouched (focus.py:23-26). A deliberate tab-into-the-transcript is left alone across renders — that
is the pinned invariant `test_a_deliberate_focus_move_is_left_alone` asserts
(`tests/ui/test_focus_returns.py:155-181`).

## The design space — the four options, and why three fail

### 1. The operator's proposal: focus always returns to the composer — rejected

An unconditional snap-back breaks the F1 jump: the jump's whole point is to land the caret on a
card's control so the next keystroke answers it (`prompts.py:1006-1008`), and a snap-back would undo
the landing. The jump is pinned by two tests that assert the caret stays on the card after F1
(`test_focus_returns.py:252-280`, `:384-419`). It would also break the deliberate tab into the
transcript, pinned by `test_a_deliberate_focus_move_is_left_alone` (`:155-181`). The charter records
the same rejection (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:112-115`), and the handoff's
design-question entry repeats it (`docs/plans/2026-08-11-v0-3-session-handoff.md:197-199`).

**A correction to the hands-on analysis, read from the code.** The analysis says "F1 jumps to the
newest unanswered prompt and F4 sweeps the answerable set, and both work by putting focus on a prompt
so the next keystroke answers it" (hands-on notes `:465-467`). That sentence is half-wrong. F1 does
not jump to the newest — it jumps to the *oldest* outstanding card's control in mount order
(`talaria/ui/prompts.py:995-1003` docstring "Jump the caret to the oldest outstanding card's
control"; `talaria/ui/app.py:1426-1427` "Move the caret to the oldest unanswered prompt's control";
`prompts.py:1152-1157` mounts cards at explicit positions because "Order is the one thing an
approval queue must not lie about"). The second half is also wrong: `action_interrupt`
(`app.py:1453-1458`) runs `interrupt_live`, whose docstring describes exactly two actions — cancel
the in-flight turn and, only when confirmed, decline its outstanding prompts (`app.py:1622-1638`) —
and the decline removes the cards, which posts `CaretReleased` and returns the caret to the composer
(`prompts.py:1187-1188`). **F4 never holds the caret on a prompt for the next keystroke; its focus
outcome is already the composer.** The snap-back tension is F1 and the deliberate pane moves, not
F4. The 2026-08-11 orchestration charter (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:112-115`)
and the 2026-08-11 session handoff (`docs/plans/2026-08-11-v0-3-session-handoff.md:197-199`) both
repeat the same "F1 and F4 move focus deliberately" misattribution, so this correction refutes those
two documents as well.

### 2. A narrower snap-back rule — none exists beyond the one already implemented

Two narrower candidates were put on the table and both fail against the deliberate moves:

**"return focus to the composer whenever nothing is waiting to be answered"** — the analysis's own sketch
(`hands-on notes:468-469`). This breaks the deliberate tab-into-the-transcript when no prompt is
outstanding: the operator tabs to scroll the transcript and is dragged back to the composer. It breaks
a deliberate click on a sub-agent row for the same reason. To spare those moves the rule must exclude
the operator's own tab and click — which is exactly the predicate the existing `CaretReleased` uses
(a control *was taken away*). There is nothing left for a new rule to add.

**"Snap back on the first printable key in a pane that discards keys."** This is the narrowest viable
descendant of the operator's proposal, and it misdirects keys from the answerability spine. A
printable key on a focused card control must **not** snap focus away — today it leaves the card
focused so the next `enter` answers it (that is the whole shape of
`test_answering_via_the_f1_jumped_to_control_hands_the_caret_back`, `test_focus_returns.py:384-419`).
A rule that snaps focus off the card would turn an intended "answer
this approval next" into "typed a message into the composer instead", a new failure the interface does
not have today. Excluding card controls leaves the rule covering only the panes — which is the scope
of the event-driven notice this plan recommends instead, minus the focus mutation and the key
re-dispatch a snap-back requires.

**The finding, stated plainly.** The only rule that returns the caret to the composer without breaking
a deliberate move is the take-away rule (`CaretReleased`), and it is implemented and pinned by tests.
Any additional snap-back either breaks F1, breaks the deliberate pane moves, or misdirects keys from a
focused card. **The substance of this unit is therefore not finding a rule; it is deciding what the
status row does given that the away-states are all deliberate or self-announcing.**

### 3. Reword the row — rejected

Rewording fixes the first half of the operator's complaint and not the second. The complaint at
hands-on notes `:205-206` — "I as the user had no idea why it was there or even what caret
corresponded to" — raises two questions the operator could not answer: what "caret" referred to and
why the row was there at all, and the second is intrinsic to a *continuous* row that announces operator-caused states. The operator
who tabbed into the transcript knows they tabbed; a row that says so, in whatever words, interrupts
their workflow to confirm something they did on purpose. The row's cases where the information is
not already visible are the transcript pane (case 2) and the prompts container (case 4) — both show
no focused control and no tint — and in the transcript case the row appears from the moment
of the deliberate tab until the operator tabs back — a long, quiet announcement that carried no
consequence in the words that were on screen, which is precisely why the operator met it and could not
use it.

Rewording also keeps the slot's permanent cost. R5's no-height-change constraint forces the slot to
exist unconditionally (`status_region.py:44-54`), so the status region always renders a blank first
row above the status command's rows (`talaria/status/contract.py:46` sets the row limit at eight).
When focus is in the composer — the common case — that row is blank, and the operator has no way to
know the blank is a placeholder for a message that is almost never there.

**The moment the row's information matters is an event, not a state.** A printable key is about to be
discarded because focus sits in a pane that accepts no text. The right surface for that moment is a
transient notice, not a row that was already there for the ten minutes before it.

### 4. Remove the row, and make the requirement event-driven — chosen

The replacement is a transient composer notice that fires when a printable key would otherwise be
silently discarded because focus sits in a pane that accepts no text. It is the same transient-feedback
surface unit B3 built for no-op keypresses
(`docs/plans/2026-08-11-v0-3-unit-b3-keypress-feedback.md:110-135`), latched per focus-hold the way
unit B4's unknown-event latch is latched per connection
(`docs/plans/2026-08-11-v0-3-unit-b4-unknown-event-flood-plan.md:66-100`). It fires at the moment of
consequence, says what is happening and how to return, and is silent otherwise. The operator's
proposal — that focus always returns to the input box (paraphrasing hands-on notes:206-208 "for the
focus to always go back to the input box") — is realised in its safe form: the operator
is never left silently in a state where typing does not reach the box; the moment they type, the
interface says so.

## Key technical decisions

### KTD1 — the row is removed, and R5 is replaced by an event-driven guarantee

Delete the `.status--caret` slot (`status_region.py:44-59`, `:68-72`), `set_caret`
(`:88-102`), `caret_text` (`:82-86`), the `_CARET_REGION_IDS` mapping and `_caret_location`
(`app.py:4130-4149`), and `_refresh_caret_slot` (`app.py:4151-4170`). The `on_descendant_focus` /
`on_descendant_blur` handlers (`:4172-4176`) do not die with the row — KTD4 gives them a new job.
R5's replacement, written the way the original was:

> **R5'.** When a printable key or a paste would otherwise be silently discarded because the caret is
> in a region that accepts no text (KTD5's predicate — transcript, agents, or the prompts container
> outside a card), the composer notice names the fact and how to return.

**Rejected — removing the row and stopping there.** That ships the operator's exact failure with the
cryptic row removed and nothing in its place: focus in the transcript pane still discards every
printable key, now with no signal of any kind. The brief's rule that removal must pin what replaces
R5 is the guard against exactly this, and AE2–AE5 below are that guard.

**Rejected — re-deciding where focus goes.** No new snap-back is added (KTD2), the deliberate moves
are untouched, and the focus chain is not changed. Those are navigation decisions that belong to spine
A's mis-aimed-input family, not to the status row's fate.

### KTD2 — the replacement is a latched composer notice on a discarded printable key or paste

The notice rides the existing composer notice (`_notice` at `app.py:2599-2608`, the same surface every
keypress refusal uses and unit B3 reuses for its no-op confirmations), and it never moves focus. The
predicate reuses the region classification the row already computed, re-scoped in KTD5: a printable
key reaches the app's `on_key` handler (`app.py:4342-4349`) only when the focused widget does not
consume it, so the handler sees printable keys from the transcript pane, from agent rows, and from the
prompts container, but **not** from the composer (the text area consumes them) and **not** from a
card's input (same). The notice fires when the focused widget is a no-text region (KTD5's refined
predicate) and the input is either a printable key or a paste. A paste is handled the same way: Textual
routes `Paste` to the focused widget exactly like a key (via `self.focused._forward_event(event)`),
and nothing in the chain handles it when focus is away from a text area — `TranscriptPane` and
`PromptRegion` have no paste handler and `TalariaApp` defines no `on_paste` — so posting
`events.Paste("PASTED BODY")` with focus on `TranscriptPane` or `PromptRegion` leaves the composer
text unchanged and the transcript unchanged: the paste vanishes with no signal, measured in the probe.
A paste is typing's bigger sibling, and the same hazard class this unit exists to remove ("the
operator is never left silently in a state where typing does not reach the box" — R5'); R5' as written
cannot cover a paste because a paste is not a printable key, so the plan extends the trigger to cover
it with the same notice and the same latch.

**The latch, so the notice is not spam.** Announce once per focus-hold. Store the region word that
announced; while the caret stays in that region, further printable keys or pastes show nothing more.
The latch clears whenever the caret **leaves the announced region**, not only when it returns to the
composer. The composer-only clear fails on a reachable path: focus on `TranscriptPane` → `f1` lands on
`Button#choice-0` → `shift+tab` lands on `PromptRegion#prompts` → `shift+tab` lands back on
`TranscriptPane`. The composer is never focused on this path. Layout order makes it structural:
`compose` yields `#transcript`, `#agents`, `#prompts` before `#composer` (`talaria/ui/app.py:1025-1031`),
so backward tabbing from any card control reaches the transcript without wrapping through the composer.
F1 is a `priority` binding that fires from any focus state (`talaria/ui/app.py:787`
`Binding("f1", "jump_to_prompt", "answer", priority=True)`). Without the widened condition the
sequence tab-into-transcript, type (notice fires, latch = transcript) → F1 to the card → shift+tab
back to the transcript → type yields **no notice**, because the latch still holds "transcript" and the
plan's old rule ("while the caret stays in that region, further printable keys show nothing more" —
and it never left, per the latch's only composer clear-point) suppresses it. This is the exact failure
the plan's own risk section names ("A latch that never clears would silently lose the warning after
the first focus-hold"), reached despite the composer clear-point. The repurposed
`on_descendant_focus`/`on_descendant_blur` handlers already fire on every focus change and
`_caret_location()` (`app.py:4141-4149`) already says where the caret is now, so KTD4's "only
reliable clear-point" argument survives unchanged — only the condition widens. This is unit B4's
pattern — announce once, count the repeats — applied at the UI layer to a state (which widget holds
focus) that is presentation concern, not domain truth, so ADR-0002's boundary is untouched
(`platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md:36-51`).

**The triggering input is discarded, not rescued.** The notice fires *as* the key or paste is
discarded; it never moves focus and never re-dispatches the input into the composer. The operator's
first character (or the entire pasted body) in each focus-hold is lost, announced after the fact, and
that loss is accepted — re-dispatching into an unfocused composer opens its own questions and would
be a behaviour change this plan has not decided. An implementer must not "fix" the notice by inserting
the character.

**The wording names the fact, the place, and the way back**, in the house register unit B3 KTD3 sets
(`docs/plans/2026-08-11-v0-3-unit-b3-keypress-feedback.md:161-177`): full lowercase sentences that name
the reason, sometimes with the consequence. Two sentences, shaped like "typing is paused — the
transcript pane holds the focus; press tab to return to the message box." Exact wording settles in
implementation against the constants it sits beside, but the shape — a claim that typing is not
reaching the box, the region word, the way back — is decided here and is what makes it interpretable
where "caret: transcript" was not. **The way back must survive truncation.** The composer notice bar is
`height: 1`, `text-wrap: nowrap`, `text-overflow: ellipsis` (`talaria/ui/composer.py:146-160`), and
that CSS carries an explicit warning: "the row is *routinely* too narrow for the line… the operative
clause is often past column 60 — and a plain one-row Static clips it with nothing on screen to say it
clipped." The sample above is ~96 characters: clipped at any terminal under ~100 columns, and at 80
columns the operator loses roughly everything from "return to the message box" onward. The way-back
clause is the one piece the original operator lacked; a shape that systematically ellipsizes it on
common widths undercuts the plan's central claim. The plan therefore constrains the wording: the way
back must survive truncation — either lead with it ("press tab to return to the message box — typing
is paused while the transcript pane holds the focus") or give the wording a column budget the bar
meets at the narrowest supported terminal. One sentence in the plan; the wording still settles in
implementation.

**Rejected — a transcript row instead.** A durable row for a transient fact is the mistake unit B3
rejected for the same reason (`docs/plans/2026-08-11-v0-3-unit-b3-keypress-feedback.md:128-129`):
"your keypress did nothing" is not part of the session's story, and the transcript is separately being
made quieter.

**Rejected — focus styling on the transcript pane instead of a notice.** Styling would make the
away-state visible at the source, which unit U3 explicitly deferred as a follow-up "if the marker
proves insufficient"
(`docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md:300-301`), and the marker
has now proven insufficient. But styling announces the *state*, not the *consequence* — a highlighted
pane does not say that typing is paused. The operator's failure was typing into a pane that discards
keys; only a notice at the moment of a discarded key states that consequence. Styling remains the
deferred U3 follow-up and is deliberately out of this unit's scope (see "What this unit does not do").

### KTD3 — the notice covers the no-text regions and never the card controls

The no-text regions are the transcript pane, the agent rows, and the prompt region container when
no card holds the caret — the places a printable key or a paste can arrive and mean nothing. Card
controls are excluded by the predicate, not by construction: in Textual 8.2.8 `Button` binds only
`enter` and the dispatch forwards every unhandled key from the focused widget up to the app
(`talaria/ui/app.py:4130-4139` ancestor walk; measured: with focus on `Button#choice-0`, pressing `q`
reached `TalariaApp.on_key`), so the "unhandled printable key" test alone does not exclude cards. The
exclusion comes from the region classification — the notice fires only when the focused widget
classifies as a no-text region (transcript, agents, or the prompts container outside a card), and a
focused card control classifies as `prompts` and is therefore answer-affordant. The reason for the
exclusion is the answerability spine, not convenience: a key on a focused card is an
operator acting on the card (or about to press `enter` on it), and moving that focus — or announcing
that typing is paused while a card is the thing being answered — would be wrong. The card announces
itself under D9 by naming its keys (`docs/plans/2026-08-11-v0-3-decision-log.md:162-181`), and a
focused card control is visibly focused (R2).

**The excluded case stated, not hidden.** A printable key on a focused approval button is silently
discarded today and stays silent under this plan. That is acceptable because the card is visible, is
focused, and names its keys — the operator has been told what to do with it — which is exactly the
quality the transcript pane and the prompts container lack.

### KTD4 — the take-away return path is kept unchanged, and the focus handlers gain a new job

`CaretReleased` (`focus.py:37-46`) and the app's answer (`app.py:4180-4195`) are the correct narrow
rule and are not re-implemented. The existing tests that pin them survive untouched. The
`on_descendant_focus` / `on_descendant_blur` handlers (`app.py:4172-4176`) are repurposed to clear
the notice latch whenever the caret leaves the announced region — the handlers already fire on every
focus change and `_caret_location()` (`app.py:4141-4149`) already says where the caret is now, so
this is the reliable clear-point (KTD2's widened condition). The earlier "only when the caret returns
to the composer" was too narrow for the transcript → F1 → PromptRegion → transcript re-entry that
`compose`'s order (`app.py:1025-1031`) and the priority F1 binding (`app.py:787`) make reachable, but
the mechanism survives unchanged — only the condition widens. Printable keys never reach `on_key`
while the composer is focused, so the latch still cannot be cleared lazily from the notice itself,
which is why a focus handler is needed at all. This keeps the removal free of dead code: the handlers
stop writing a row and start clearing a latch, and they do it on every region exit rather than only
on composer entry.

**Rejected — leaving the handlers as dead code or deleting them.** Deleting them removes the only
reliable reset point for the latch; keeping them to write a deleted row is dead code. Repurposing is
both.

### KTD5 — the region classification survives, re-scoped from naming to classifying

The mapping of ancestor ids to region words (`app.py:4130-4139`) is not deleted wholesale: the notice
needs to know whether the focused widget is in a no-text region, or in the prompts region but not
inside a card — the ancestor walk (`app.py:4141-4149` over `focused.ancestors_with_self`) already
distinguishes the two (a card is a `PromptCard` ancestor; the container is the region itself).
Transcript and agents are always no-text; prompts is answer-affordant only when the focused widget
is inside a card and is no-text when the container itself holds the caret. The plan keeps the
classification and re-scopes it — implementation may rename the map and the `_caret_location` read,
but the ancestor walk that classifies a focused widget's region stays, because it is the exact
predicate the notice runs on.

## Risk this unit must clear

**Removal with the replacement skipped.** The failure this release already knows: the operator could
not read the row, so a future implementer reads this plan as "remove the row" and stops, shipping the
silent-loss hazard with no signal. AE2 pins the replacement as a present-and-working behaviour, so the
"nothing happens" items (the row never appears) are balanced by a "something happens" item (the notice
does).

**The shared composer notice bar.** The discard notice overwrites whatever the bar held, and unit B3's
notices overwrite it back. That is the accepted cost of every notice on the shared bar — unit B3
states it in its own risk section
(`docs/plans/2026-08-11-v0-3-unit-b3-keypress-feedback.md:262-265`) — and the discard notice is the
least valuable message the bar can hold losing to a later message loses nothing, the same argument B3
KTD1 makes.

**The merge with unit B3 in `app.py` and in `tests/ui/test_focus_returns.py`.** B3's plan edits three action methods and adds a branch to
`_land_session`, and routes the `end` key in the app's raw key handler through the F5 rule
(`docs/plans/2026-08-11-v0-3-unit-b3-keypress-feedback.md:137-159`). This unit edits `on_key`
(`app.py:4342-4349`) at the printable-key fall-through, a branch B3 does not touch. The two changes
meet in the same handler on disjoint branches, and neither depends on the other's — the conflict is
merge order only, and it is named here so neither plan treats `on_key` as its private file. The two
units also both edit `tests/ui/test_focus_returns.py` in disjoint hunks (B3 amends
`test_f1_with_nothing_outstanding_is_a_no_op` to assert `JUMP_NOTHING_OUTSTANDING`; B1 removes
`test_tabbing_into_the_transcript_names_it_in_the_caret_slot` and
`test_f1_jump_names_the_prompts_region_in_the_caret_slot`), with no genuine collision —
again merge order only. The boundary is mutual: B3's plan states it "does not take a position on
the caret status row" and names this unit's surface
(`docs/plans/2026-08-11-v0-3-unit-b3-keypress-feedback.md:358-359`), and this plan returns the same
courtesy by changing none of B3's four silent-path sites.

**The latch must re-announce after a return, and must not spam within a hold.** A latch that never
clears would silently lose the warning after the first focus-hold; one that clears on every key would
be noisier than the row it replaces; and a latch that clears only on composer return still loses the
warning on the reachable `transcript → F1 → PromptRegion → transcript` re-entry (KTD2). AE3 asserts
all three: no spam within a hold, re-announce after composer return, and re-announce after the
composer-free re-entry.

**Sequencing with spine A.** The removal is safe because the prompts away-state is announced by the
card: R2 styling today, D9's focus-owning card when A1 lands. Between this unit's merge and A1's, an
F1 jump lands on a visibly focused control (`test_focus_returns.py:239-249` asserts the focused
button), so removing the redundant "caret: prompts" text loses no information the operator could see.
The plan does not depend on A1 having landed; it depends on R2, which shipped in v0.2.

**The geometry invariant.** Removing a mounted slot cannot move a region the slot occupied, but the
old R5 falsifier asserted the invariant for a reason, and the removal asserts it once more
(`tests/ui/test_status_region.py:135-174`) so a regression that re-introduces a focus-dependent row
fails loudly.

**What breaks, named rather than discovered.** The caret-slot tests change:
`test_tabbing_into_the_transcript_names_it_in_the_caret_slot` (`test_focus_returns.py:184-208`) and
`test_f1_jump_names_the_prompts_region_in_the_caret_slot`
(`:252-280`) asserted the row's content and are removed with a comment naming this plan; the U3
section of `test_status_region.py` (`:99-174`) is rewritten around the latch-clearing handlers; and
`test_a_deliberate_focus_move_is_left_alone` (`test_focus_returns.py:155-181`) and
`test_answering_via_the_f1_jumped_to_control_hands_the_caret_back` (`:384-419`) survive **unchanged**
as the deliberate-move and take-away pins. `status_region.py`'s `row_texts` and `marker_text`
properties are untouched — only the caret member disappears.

## Acceptance evidence

- **AE1.** The caret row never appears: tabbing into the transcript, tabbing a second time onto the
  prompts container (`PromptRegion#prompts`), an F1 jump onto a card, and a click on a sub-agent row
  each leave the status region with **no** `caret:`-prefixed text, and the region's rendered row set
  is identical to the composer-focused row set. The row's absence is asserted as a presence-of-text
  check, not by the string "caret" not existing anywhere in the tree.
- **AE2.** R5' is live for keys and pastes: with the caret in the transcript pane — and separately with
  the caret on the prompts container — pressing one printable key shows exactly **one** composer notice
  naming that typing is not reaching the message box, naming the region, and naming the way back with
  the way-back clause surviving at 80 columns; the focus does **not** move and the triggering key is
  discarded, not re-dispatched (F7). A paste into either no-text region is treated the same way — one
  notice, no transcript row, composer's text unchanged, focus does not move — and a second paste in the
  same hold shows no additional notice (F3).
- **AE3.** The notice is latched per focus-hold: a second printable key (or a second paste) in the same
  hold shows **no** additional notice; returning the caret to the composer and tabbing back into the
  transcript resets the latch, and so does the composer-free re-entry `transcript → F1 → shift+tab
  (PromptRegion) → shift+tab (transcript)` — a fresh first key or paste after either return announces
  again (F2). All three asserted.
- **AE4.** The notice does **not** fire where the away-state is announced already: after an F1 jump to
  an approval card, a printable key shows **no** notice and the card keeps the caret — the
  answerability spine is untouched (this is KTD3's exclusion, asserted).
- **AE5.** The notice does **not** fire while the composer holds the caret: ordinary typing shows **no**
  notice, and the composer's text is unchanged. The silence is a requirement, not an omission.
- **AE6.** The take-away return path is unchanged: answering a card by tab, by click, or after an F1
  jump, collapsing the sub-agent rows with F2, and a sub-agent finishing each return the caret to the
  composer with **no** notice — the `CaretReleased` mechanism and its existing tests are untouched.
- **AE7.** The deliberate moves are unchanged: tab-into-the-transcript keeps the caret there across
  renders (`test_a_deliberate_focus_move_is_left_alone` survives unchanged), and the F1 jump still
  lands and stays on the card's control.
- **AE8.** The status region's geometry is invariant across every focus state after the removal — the
  old R5 falsifier, re-asserted so a future focus-dependent row fails loudly.
- **AE9.** The replay gate runs green over the existing gate corpus, with both `content_is_complete`
  and `interface_shows_everything` true. The corpus is named by digest and frame count, never by
  path. The notice writes no transcript row and the gate drives no focus away from the composer, for
  the same reason unit B3 KTD6 gives — the change has no surface under the gate.
- **AE10.** The project check is clean: `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check`.

**Acceptance for a person, per the charter's evidence rule 2**
(`docs/plans/2026-08-11-v0-3-orchestration-charter.md:182-186`): driving the app, the operator never
sees a `caret:` row; when they tab into the transcript and type, the notice tells them typing is
paused and how to return; and the interface never leaves them typing into a pane that discards the
text with nothing said. That is operator-only and is not claimed on test evidence.

## Verification

```bash
uv sync --all-groups
uv run pytest tests/ui/ tests/domain/ tests/replay/ -q
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched.

## What this unit does not do

- **It does not re-decide any key binding.** F1, F4, F5, F2, and tab are untouched. The function-key
  row is spine A's unit A4, under D9
  (`docs/plans/2026-08-11-v0-3-decision-log.md:128-181`); this plan does not re-litigate it.
- **It does not change where focus goes.** No snap-back is added; the deliberate moves and the
  `CaretReleased` take-away rule are unchanged. The operator's proposal that focus always returns
  to the input box is realised only as the notice that fires when a key would otherwise be lost.
- **It does not change the focus chain.** The transcript pane stays focusable and the agent rows stay
  clickable. Making the panes unfocusable so the hazard cannot exist is a navigation decision in spine
  A's mis-aimed-input family and is deliberately out of scope here.
- **It does not add focus styling to the transcript pane.** That is unit U3's deferred follow-up
  (`docs/plans/2026-08-08-talaria-v0-2-answerability-and-session-story-plan.md:300-301`), and it
  announces a state, not a consequence; the notice announces the consequence and is this plan's
  replacement for R5.
- **It does not touch the domain core.** The notice is presentation feedback, the latch is
  presentation state, and ADR-0002's boundary holds.
- **It does not implement D9's focus-owning card.** That is unit A1's work; this plan only depends on
  the card being visible and focusable, which R2 already guarantees.
