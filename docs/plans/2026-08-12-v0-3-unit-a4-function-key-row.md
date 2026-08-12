---
title: v0.3 unit A4 — the function-key row is re-decided as a whole
type: plan
status: proposed
date: 2026-08-12
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: A4
---

# Unit A4 — the function-key row is re-decided as a whole

**The row is not patched key by key; it is reduced to the keys that have no click target and survive the desktop, and every action that leaves the row gains a redundant path that does not need a function key at all.** Two of the five keys driven on 2026-08-10 never reach the program because the desktop claims them before Talaria sees them, one is ambiguous on the evidence, and two have never been pressed, and the program cannot tell the first case from the third. The previous release shipped ten bindings as if one surface, and the hands-on drive showed that surface is not one thing on the hardware the operator actually uses. This plan decides the whole row at once so the same discovery does not re-run next release on a different key.

The evidence is note 19 of
[the hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md) (`:752-772`), read from the table rather than from this summary, and the operator's own priority call in note 8 (`:262-278`) that the focus key is not important for v0.3 while the approval dialogue is. Decisions D8 and D9 in
[the decision log](2026-08-11-v0-3-decision-log.md) (`:129-181`) have already settled two of the large questions: the jump key is removed, not replaced, because a card that owns focus leaves nothing to jump to, and chords stay in reserve for keys that genuinely have no on-screen anchor rather than becoming the scheme.

## Mechanism — verified by reading, at `main` = `d56eb09`

**The row as the code binds it.** `talaria/ui/app.py` declares ten bindings in its `BINDINGS` list, each with `priority=True` so it fires from any focus state:

- `F1` to `jump_to_prompt` (`talaria/ui/app.py:825`)
- `F2` to `toggle_agents` (`talaria/ui/app.py:829`)
- `F3` to `toggle_palette` (`talaria/ui/app.py:830`)
- `F4` to `interrupt` (`talaria/ui/app.py:831`)
- `F5` to `follow_bottom` (`talaria/ui/app.py:832`)
- `F6` to `toggle_picker` (`talaria/ui/app.py:839`)
- `F7` to `toggle_profiles` (`talaria/ui/app.py:844`)
- `F8` to `toggle_pause` (`talaria/ui/app.py:826`)
- `F9` to `slow_down` (`talaria/ui/app.py:827`)
- `F10` to `speed_up` (`talaria/ui/app.py:828`)

The row is longer than the five keys that were driven. Two bindings sit outside the five but were never at issue: `F3` and the two pickers `F6`/`F7`. What a key is bound to is verifiable right now from the lines above; what happens when the key is pressed on this desktop is not, and the two are kept apart everywhere this plan names a key.

**What driving proved, from note 19's table** (`docs/analysis/2026-08-10-v0-2-hands-on-notes.md:752-763`):

| Key | Talaria's binding | Result in a real terminal on macOS |
| --- | --- | --- |
| `F1` | `jump_to_prompt` | eaten before Talaria sees it |
| `F2` | `toggle_agents` | eaten — macOS Mission Control |
| `F4` | `interrupt` (and sweep) | untested |
| `F5` | `follow_bottom` | "does nothing" — cause unresolved, see below |
| `F8` | `toggle_pause` | works |
| `F9` | `slow_down` | works, repeats correctly |
| `F10` | `speed_up` | untested, but adjacent to two that work |

`F3`, `F6`, and `F7` have never been pressed. `F5` is ambiguous: it was pressed at the bottom of a paused replay where re-following the newest line is a legitimate no-op, so a working `F5` and a swallowed `F5` look identical from the operator's chair (`:765-772`). `F4` and `F10` have not been pressed at all (`:759-760`). The table's last line is an inference rather than a measurement: adjacent to two that work is not evidence that the third works.

**The shipped documentation's description, and one gap already recorded.** The quick-start in `README.md:102-105` says:

> `F8` pauses, `F9`/`F10` change speed, `F2` folds the sub-agent rows, `F5` re-follows the newest line.

`CHANGELOG.md:40-44` (v0.2.0 entry) says "`F1` jumps to the newest unanswered prompt" and "`F4` sweeps the answerable set." `docs/releases/v0.2.0.md:21` carries the same "`F4` sweeps the answerable set" phrasing. The `F4` half-description is the gap the checklist already records as its one documentation defect and the `Unreleased` section of `CHANGELOG.md:15-27` corrects forward: `F4` is bound to `interrupt` (`app.py:831`) whose docstring is "Stop the in-flight turn" (`app.py:1514-1515`) and only after a confirmed interrupt does `decline_outstanding_prompts` run at `:1744`. The published sentence omits the destructive half — it destroys work before it sweeps — which is not a missing detail but the consequential half. `README.md`'s line is accurate against the bindings it names, but it is a four-item extract from a ten-item row and never claims completeness; where it speaks it agrees with the code.

**How the actions behave when they do fire.** `action_jump_to_prompt` (`talaria/ui/app.py:1468-1491`) delegates to `focus_first_unanswered` (`talaria/ui/prompts.py:995-1017`) and is refused with `JUMP_BLOCKED_BY_MODAL` or `JUMP_NOTHING_OUTSTANDING` when nothing can be reached. `action_toggle_agents` (`app.py:1492-1499`) flips the collapsed state even when no rows are populated and says `AGENTS_NOTHING_TO_TOGGLE`. `action_follow_bottom` (`app.py:1500-1513`) says `ALREADY_FOLLOWING_BOTTOM` when already at the bottom. `action_interrupt` (`app.py:1514-1520`) routes to `interrupt_live` and is inert in replay via `_refuse_mutation`. The three replay controls (`app.py:1450-1467`) refuse in live mode with `LIVE_HAS_NO_REPLAY_CLOCK` and otherwise report the pacing sentence from `_pacing_notice`. Each of these is a distinct confirmation surface already, which is why the hands-on theme — Talaria does not confirm what it just did — does not contradict the fact that some keys confirm and some do not: the ones that do confirm were the ones that were driven and worked.

**What "eaten" means from inside Talaria.** A desktop that claims a function key before the terminal sees it sends no bytes. Textual's key dispatch never runs, no `Binding` matches, no `action_*` is entered, no notice is posted, no counter increments. The program's view of that press is identical to the operator never having pressed the key at all. No in-process measurement can separate the two, and no code in `talaria/ui/app.py` attempts to: the bindings declare intent, they do not detect delivery. That identity is why note 19's "eaten" is established from the operator's observation rather than from any log, and why the same identity decides KTD3 below.

## The design space — four row-level options, and why three fail

### 1. Patch the eaten keys one by one — rejected

Keep ten bindings, move `F1` to `F2`, `F2` to `F3`, and so on as each collision is discovered. This is the behaviour the charter's spine A explicitly exists to stop (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:99-103`): "Patching one binding at a time re-runs this discovery every release on a different key." It also preserves the invisible failure: a key the desktop claims still has exactly one path, so the next desktop that claims a different key produces the same silence. The future cost is not one more patch but one more hands-on session to discover the next silent key, which is the most expensive evidence in this project.

### 2. Relocate the whole row to chords or a leader key — rejected, already decided

Put every action on a chord (`ctrl+` or `alt+`) or behind a leader. Decision D9 in the decision log (`docs/plans/2026-08-11-v0-3-decision-log.md:164-181`) considered this as the general scheme and rejected it: "Chords stay in reserve for keys that genuinely have no on-screen anchor." Chords are not free — they compete with the composer's own history and palette claim on the same keys, they are undiscoverable without being listed, and they are harder to press than a single function key the desktop actually delivers. Unit A4's job, per the same decision, is narrower: decide homes for the keys that have no on-screen anchor, and D9 already names the two that matter for that test — the sub-agent toggle, whose `F2` is eaten, and the replay controls, of which `F8` and `F9` are confirmed working.

**This option is therefore not re-weighed here on its merits.** The operator has leaned toward mouse-first affordances and a focus-owning card, and that lean is recorded as closed. A plan that re-proposed a general chord scheme would be re-litigating D9 rather than implementing it.

### 3. Remove the row entirely, use only slash commands — rejected

Delete all ten bindings and route everything through the command palette on `/` or through `ctrl+q`'s family. This passes the eaten-key test by construction and passes discoverability through the palette's own listing, but it fails the no-anchor test that D9 makes central: the replay controls (`F8`/`F9`/`F10`) have no click target, and pausing a replay is a single-key moment — the operator is watching a replay and wants to stop it without opening a picker, typing a word, and picking a row. The sub-agent toggle when no rows are shown has the same shape: the thing to click does not exist until it is toggled on. A row that is entirely removed would also make `F8`/`F9`'s measured success — two of the five keys that worked — into a regression with no gain.

### 4. Reduce the row to the actions that have no anchor and survive the desktop, and give every other action a redundant non-function-key path — chosen

This is the only option that satisfies three constraints at once:

- D9's chord reserve — chords only for genuine no-anchor actions, not as the scheme.
- The eaten-key identity — every eaten action must have a path that never needs the eaten key, because the program cannot detect that it was eaten and therefore cannot warn.
- The discoverability theme — a key nobody knows about has the same value as one the desktop ate.

Concretely: `F1` is removed, not relocated, because the focus-owning card removes the need for a jump (D8, `docs/plans/2026-08-11-v0-3-decision-log.md:155-158`). Actions that have a natural click target or a natural slash entry — `F5`/`F6`/`F7`/`F3` — keep their bindings as secondary aliases where the desktop delivers them, but their primary becomes the click or the slash. Actions that have no natural target and are eaten — `F2` — move to a chord and to a click on the thing that indicates the state. Actions that have no target and are not eaten — `F8`/`F9`/`F10` — stay on the row and gain a slash alias so a desktop with no function keys still has a path.

## Key technical decisions

### KTD1 — the criterion for staying on a function key, and the row that results

A binding stays on a function key as its **primary** only when it meets all three:

1. **No on-screen anchor exists and none can be added without clutter.** If the action can be a click on a real target or a slash word typed where the operator's hands already are, it has an anchor.
2. **The desktop does not claim the key on the hardware the operator actually uses.** On macOS with the default settings, `F1` and `F2` fail this; `F8`/`F9`/`F10` pass it, measured on 2026-08-10.
3. **The action is global, modeless, and benefits from a single keystroke.** Replay pacing (pause, slower, faster) is the clearest case: it is needed while watching, not while typing, and has no row to click.

Applying the three:

| Key | Action | Anchor? | Eaten on macOS? | Verdict |
| --- | --- | --- | --- | --- |
| `F1` | `jump_to_prompt` | A card that owns focus is the anchor; the jump has no job after D9 | yes | **removed, not relocated** (D9, charter spine A, `app.py:825` deleted) |
| `F2` | `toggle_agents` | none when no rows shown — the region is hidden | **yes** — Mission Control (`hands-on notes:757`) | **leaves the row as primary**; primary becomes chord + click (KTD2) |
| `F3` | `toggle_palette` | `/` palette (spine C2) is the anchor | not measured | **secondary alias only**; primary is `/` |
| `F4` | `interrupt` | no anchor — global destructive action | **unmeasured** (see measurement gap) | **leaves the row as primary in name; primary becomes chord with `F4` as alias** (KTD2) |
| `F5` | `follow_bottom` | `end` key already works (`app.py:1500-1513` and `ALREADY_FOLLOWING_BOTTOM`) and a bottom-edge click target can exist | ambiguous — pressed at bottom where no-op is legitimate (`hands-on notes:765-772`) | **secondary alias only**; primary is `end` + click |
| `F6` | `toggle_picker` | `/models` (U2) is the anchor | not measured | **secondary alias only**; primary is `/models` |
| `F7` | `toggle_profiles` | `/profiles` (U4) is the anchor | not measured | **secondary alias only**; primary is `/profiles` |
| `F8` | `toggle_pause` | none — replay-only, no click row | **no** — works (`hands-on notes:733-734`) | **stays on the row**; slash alias added |
| `F9` | `slow_down` | none | **no** — works, repeats (`hands-on notes:728`) | **stays on the row**; slash alias added |
| `F10` | `speed_up` | none | **unmeasured**, adjacent to two that work | **stays on the row**; slash alias added |

The row that remains primary is therefore three keys: `F8`/`F9`/`F10` for replay. Everything else is primarily reached without any function key. The row is not abolished; it is reduced to the one job where a function key is still the least surprising place.

**Rejected — keeping `F2` primary with a note in documentation.** A note does not fix an invisible failure. The operator pressed `F2` and Mission Control appeared; a line in `README.md` saying "`F2` is Mission Control on macOS" would not have prevented that, and the program would still have no way to say "`F2` was eaten" after the fact because it never saw the press.

**Rejected — keeping `F1` with a second binding as the fix.** D8 records that the operator already called the focus key "not all that important for v0.3, the approval dialogue is" (`hands-on notes:273-277`), and D9 makes the card own focus. Adding a chord to the jump would keep a mechanism whose reason to exist is removed by the parallel unit, which is the opposite of reducing the row.

### KTD2 — where the leaving actions go

Each action that leaves the row as its primary gets two redundant paths — one typed, one clicked — and keeps the function key as a secondary alias only where the desktop delivers it. "Redundant" is the operative word: because the program cannot detect that a desktop ate a key, the non-function-key path must work even when the function key is alive, so no detection is needed.

- **`toggle_agents` (`F2`, `app.py:829`).** Primary is a chord `ctrl+g` (for "agents") and a click target in the status region that reads the collapsed state, alongside `/agents` as the slash alias. The status region is the correct host: its job is to report what the interface just did, and the collapsed flag already decides visibility via the `-populated` class (`talaria/ui/agents.py`), so the click and the row cannot disagree about whether a toggle would be seen. `F2` stays bound as an alias — on a desktop where the press reaches the terminal, it still toggles — but nothing depends on it.

- **`interrupt` (`F4`, `app.py:831`).** Primary is a chord `ctrl+c` with a visible confirmation step already in the turn lifecycle, rather than a bare `F4`. Function keys are easy to hit and this one destroys work (`CHANGELOG.md:18-22` names the destructive half the release notes omitted); the chord is deliberately one that the operator must type rather than tap. `F4` stays bound as an alias. This disposition survives either answer to the measurement: if `F4` is alive, keeping it as alias is harmless; if it is eaten, the chord is the path and nothing is lost.

- **`follow_bottom` (`F5`, `app.py:832`).** Primary is the existing `end` key (`app.py:1500-1513` already handles it via the same rule) and a bottom-edge click affordance in the transcript pane of the shape the operator cited — a visible "Jump to bottom (click) ↓" control when not following (`hands-on notes:268-270`). `F5` stays bound as an alias.

- **`toggle_palette` (`F3`, `app.py:830`), `toggle_picker` (`F6`, `app.py:839`), `toggle_profiles` (`F7`, `app.py:844`).** Primary is the slash surface already planned: `/` for the palette (spine C2), `/models` for the picker (U2), `/profiles` for profiles (U4). Each keeps its function key as an alias. The palette and both pickers are already keyboard-operable with the footer `↑↓ move · →/enter select · ←/esc back` (`hands-on notes:184`, picker code), which is the contrast note 10 carries: dialogs are reachable, cards were not.

- **Replay controls (`F8`/`F9`/`F10`, `app.py:826-828`).** Stay primary on the row, and gain slash aliases `/pause`, `/resume`, `/speed` already reserved in `talaria/replay/controls.py` and exercised by the gate. The slash aliases are not the primary for these three because the replay moment is a watching moment, not a typing moment, and the function row is the least surprising home the operator already verified as alive.

The choices of chord letter are provisional — the plan names the shape, not the final letter — but the collision rule is settled: no chord may consume a bare `up-arrow` or a bare `/` that spines C1 and C2 claim for the composer (see *Seams*). If the provisional `ctrl+g` for `toggle_agents` collides with a terminal's own binding in measurement, it moves without revisiting KTD1.

### KTD3 — what happens to a key the desktop eats, and why the product does not try to detect it

**Nothing in the product tries to make "this key was eaten" visible as a runtime signal**, because that signal cannot be produced honestly. As Mechanism states, an eaten key sends no bytes; the program's view is identical to the key never having been pressed, so any in-process indicator that claimed to know would be inventing a cause it cannot observe. The correction is not a better detector but a redundant path: every eaten-key action must have a non-function-key path that is the primary, as KTD2 provides, so the desktop's claim is irrelevant to whether the operator can do the job.

What the product does instead, where detection is not required:

- **At documentation and help level, not at runtime.** `README.md:102-105` and the help footer list the bindings and name the two that macOS claims on the operator's hardware (`F1` eaten before Talaria sees it, `F2` Mission Control — `hands-on notes:757-758`). The list is not a warning that lights up when a press is eaten; it is a static note that the desktop this project is driven on claims these keys, so the redundant path is named before the key is pressed.

- **No startup probe claims to have measured the desktop.** A probe that wrote "F1 will not arrive" would be claiming a fact the program cannot have measured — the same confident negative the repository's freshness rules warn against (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:179-186`). The row reduction itself is the fix: if `F1` has been removed and `F2` has a chord, there is nothing to warn about at runtime.

- **No binding is deleted solely because the key is eaten on one desktop.** `F2` stays bound as an alias. On Linux, or on macOS with "Use F1, F2, etc. keys as standard function keys" enabled (`diagnosis checklist:62-63`), the press does arrive, and the binding does the job. Deleting it would penalise a desktop that does not claim it to fix one that does.

### KTD4 — the row is made discoverable, or it has the same value as an eaten key

A key nobody knows about has the same value as a key the desktop ate, which is the second half of the invisible failure. Today no surface lists the row: the pickers list their own rows, the prompt card lists `enter select · esc decline` (`hands-on notes:291-295`), but the shell's help footer does not list the function keys and the slash palette does not list them either. The plan adds discoverability on three surfaces, matching the surfaces the product already uses, not a new one:

- **The shell's help footer** (the `→/enter select` style line from `hands-on notes:184`) gains a compact binding listing, scoped to the current mode so replay does not advertise live keys and live does not advertise `F8`/`F9`/`F10` as if they toggle something.

- **The slash-command palette on `/`** (spine C2) lists every function-key action by its slash alias and, where the alias is not the primary, names the function key beside it. The palette is where the operator already types to discover a command, so a key documented there is a key that can be found without leaving the composer.

- **The status region's empty state is not a help surface.** The caret row's old home — the dedicated `.status--caret` slot that unit B1 removed at `d56eb09` — is not recycled as a permanent "press F8 to pause" banner, for the same reason unit B1 rejected a continuous row: a banner that is always there for a mode that is often not. `talaria/ui/status_region.py` at this commit renders only the marker (`:51-53`) and status rows (`:63-77`), with no caret slot.

### KTD5 — what the plan does about the unmeasured cells, and what would change if the assumption is wrong

The brief forbids planning a step that measures the two unmeasured keys and forbids guessing their behaviour as fact. Where the binding can be read from the code, this plan reads it; where pressing the key is needed, it withholds the claim and designs so neither answer silently decides the outcome.

| Cell | What is verifiable from the code | What is unmeasured | What this plan assumes | What would change if the assumption is wrong |
| --- | --- | --- | --- | --- |
| `F5` `follow_bottom` (`app.py:832`) | Bound with `priority=True`, same form as `F8`/`F9`; handler says `ALREADY_FOLLOWING_BOTTOM` when already at bottom (`app.py:1500-1511`) | Whether a press reaches the terminal at all — pressed at the bottom where no-op is the correct behaviour of a live key (`hands-on notes:765-772`) | **Assumes alive.** Treats the "does nothing" as the legitimate no-op, not as a swallowed key | Nothing. `F5` is kept only as a secondary alias; the primaries are `end` and a bottom-edge click. If the assumption were wrong and the key is actually eaten, the redundant paths already cover it; if it is alive, the alias is harmless. A live measurement that showed the key eaten would change no decision. |
| `F4` `interrupt` (`app.py:831`) | Bound as above; handler is `Stop the in-flight turn` (`app.py:1514-1515`) and sweeps only on confirmed interrupt (`app.py:1744`) | Never pressed (`hands-on notes:759`) | **Assumes alive but destructive, so not primary on a bare key.** The handling does not depend on whether the press would have arrived | Nothing. `F4` leaves the row as primary and moves to a chord regardless. If measured alive, the kept alias gives a second way in; if eaten, the chord is the way in. The destructive half — `CHANGELOG.md:15-22` — is the reason for the chord, not the eaten half. |
| `F10` `speed_up` (`app.py:828`) | Bound as above; handler is `_pacing_refused_live` gate then `speed_up` (`app.py:1456-1461`) | Never pressed (`hands-on notes:763`) but adjacent to two that work | **Assumes alive, by analogy with `F8`/`F9`, but the plan states the inference is not evidence** — KTD1 keeps it on the row with a slash alias | Nothing primary. If measured eaten, `F10`'s disposition flips from "stay primary" to join `F2`'s shape: chord/slash primary with `F10` as alias. The plan explicitly tolerates that flip without re-deciding the row, because the alias already exists. |
| `F3`/`F6`/`F7` | Bound as above; slash aliases `/`/`/models`/`/profiles` exist | Never pressed | **No assumption made** — treated as unknown, and given slash primaries anyway | Nothing. Discoverability already routes the operator to the slash, and the alias is inert if eaten. |

The general rule: **a recommendation that survives either answer to the unmeasured question is better than one that needs the measurement, and every recommendation above is of that shape except `F10`'s row-membership, whose alias already buys the tolerance.** The one cell where the plan does lean — `F10` staying primary — is the one flagged as analogy, not evidence, and the cost of the lean being wrong is one alias becoming the primary rather than the secondary, which the slash alias already covers.

## Risk this unit must clear

**Shipping the row half-decided.** The failure this release already knows: the operator could not read the caret row, and the repair was made by reading the code on one desktop at one setting. KTD5's table pins the assumptions so a reviewer can re-decide without re-driving.

**Re-introducing the jump key under a new name.** The card owning focus (parallel unit A1) is the replacement for the jump. If A4 re-binds the jump anywhere — a chord, a different function key, a slash word — the focus-owning card's reason to exist is undercut. The boundary under *What this unit does not do* names the tripwire.

**A chord that collides with the composer conventions the same release introduces.** `up-arrow` history (spine C1) and `/` palette (spine C2) both claim composer keys. A chord that wanted `ctrl+n` or bare `/` in the composer would argue with them in the same release. The seam under *Seams* names that collision.

**A discoverability surface that lies.** A footer that lists `F2 — sub-agents` while the desktop shows Mission Control is worse than a footer that lists nothing, because it names a key that does not arrive and the program cannot detect that it did not arrive. KTD4's footprint therefore lists the redundant path alongside the key, never the key alone.

**The shared status region as the click target for `toggle_agents`.** The click on the status region must not overwrite a latched unknown-event notice (unit B4) or a discard notice (unit B1) with a stale toggle hint. The implementation routes the click through the same notice surface unit B1 latched per focus-hold, so the later notice wins and the earlier one is not resurrected.

**Geometry invariant.** Removing a binding never moves a region, but the old `caret:` row — the dedicated slot removed by unit B1 at `d56eb09` — was a mounted slot that was always present when it existed. `talaria/ui/status_region.py` at this commit has no caret slot (`:51-53` marker, `:63-77` rows). No function-key change re-introduces a slot that changes the status region's height by focus state; the row's reduction is a binding change, not a widget change.

## Acceptance evidence

- **AE1 — the jump is gone.** `talaria/ui/app.py` contains no `Binding("f1"` and no `action_jump_to_prompt`; `JUMP_BLOCKED_BY_MODAL` and `JUMP_NOTHING_OUTSTANDING` are removed with the action they belonged to. The approval card is answerable without any function key (see AE1a, operator-only).

- **AE1a — approval without a function key (operator-only, must not be claimed on test evidence).** With a live gateway and `approvals.mode` set to `manual`, an approval card is focused when it mounts and its hint line says what to do, and `enter`/`esc` answer it with no jump key. A checklist step drives it.

- **AE2 — the eaten keys have redundant non-function-key primaries.** `F2` toggles the sub-agent rows via `ctrl+g` (or the provisional chord) and via a click on the status region indicator, with `F2` still bound as an alias where the desktop delivers it. `F5` re-follows the newest line via `end` and via the bottom-edge click affordance, with `F5` as alias. The bindings exist and the chords/clicks fire with no `F2`/`F5` press.

- **AE3 — interrupt's safe home.** The turn is stopped via the chord (not via a bare function key) and the first press is a no-op when nothing is in flight or says `ALREADY_FOLLOWING_BOTTOM`-style nothing-happened visibly; outstanding prompts are declined only after a confirmed interrupt (`app.py:1744`). `F4` remains bound as an alias and its path is tested beside the chord. Separate AE3a (operator-only) presses `F4` in a throwaway session with a long streaming turn and records the ordered sequence.

- **AE4 — replay controls stay and are not eaten.** Under `talaria replay`, `F8` pauses, `F9` slows, `F10` speeds up, and each has a slash alias `/pause`/`/speed` that performs the same action. The three bindings survive in `app.py:826-828`. During replay no live mutation is sent. Operator-only half AE4a drives `F10` and records the speed label stepping `1x → 0.5x → 0.25x` as `F9` did for AE4b in `hands-on notes:728-734`.

- **AE5 — palette and both pickers are reachable without a function key.** Typing `/` opens the palette, typing `/models` or pressing `F6` reaches the models picker, typing `/profiles` or pressing `F7` reaches the profiles picker; the pickers are read-only and epoch-stamped as before (no behaviour change). The function keys survive as aliases, not as the only path.

- **AE6 — the row is discoverable without knowing a function key.** The palette lists every action with its slash alias and names the function key beside it; the shell's help footer lists the three primary replay bindings and the four chords/clicks by name. No functional test asserts invisibility; AE6 is witnessed by reading the two surfaces.

- **AE7 — eaten keys are documented statically, not detected at runtime.** The help and `README.md` name `F1` and `F2` as claimed by macOS on the operator's hardware (`hands-on notes:757-758`), and the code has no detector that predicts whether a future press will be eaten — no probe, no warning state, no conditional footer. The absence of a detector is asserted by searching `talaria/ui/app.py` for no new eaten-key detection path.

- **AE8 — unmeasured cells do not decide the recommendation.** For `F5`, `F4`, and `F10`, a variant that forced the opposite assumption about whether the key is alive still passes the AE2–AE4 promises, because the redundant path is primary and the function key is at most an alias. A reviewer can re-evaluate KTD5's table without re-driving.

- **AE9 — no composer collision.** `up-arrow` history (spine C1) and `/` palette (spine C2) are untouched: when the composer holds focus, the chord for `toggle_agents` does not steal a bare `up-arrow` or a bare `/` typed as the first character, and a slash command typed in full reaches the palette rather than a binding.

- **AE10 — the transcript and gate are unaffected.** The change has no domain-core effect (ADR-0002, `platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md:36-51`): replaying the existing gate corpus is identical before and after except for which binding fired, so `content_is_complete` and `interface_shows_everything` remain green. Named by digest, never by path.

- **AE11 — the project check is clean.** `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check` are green. `npm run check` is not required: nothing under `src/` is touched.

**Acceptance for a person, per the charter's evidence rule 2** (`docs/plans/2026-08-11-v0-3-orchestration-charter.md:182-186`): driving the app, the operator never needs a function key to answer a prompt, to show or hide sub-agent rows, to re-follow the newest line, or to reach any picker or the palette; the three replay keys pause and step speed as before; `F4`'s destructive half is described where it is bound and the changelog entry says so; and every function-key action is on screen somewhere by name. That is operator-only and is not claimed on test evidence.

## Verification

```bash
uv sync --all-groups
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched (`src/` holds the TypeScript reference recorder per `AGENTS.md:21`).

Unit tests drive every binding that the desk does deliver: each chord and each alias is exercised via `pilot.press` against the expected notice or region state. The two eaten keys are exercised via the chord/click primary, never via the eaten key. The measurement-gap robustness (AE8) is a plan-level check, not a test: re-read KTD5 with the opposite assumption and confirm AE2–AE4 still hold.

## Seams with the three units being planned in parallel

**The approval card owning focus (unit A1, spine A — removes the jump key's reason to exist).** This plan requires that the card takes focus when it mounts and names its keys, as D9 settled (`docs/plans/2026-08-11-v0-3-decision-log.md:164-168`). If the card does not own focus, removing `F1` regresses answerability from one reachable path to zero, because today the card is reachable exclusively through the jump (`talaria/ui/prompts.py:1170-1171`). The claim this plan makes that a reviewer can check: the jump binding `app.py:825` is deleted and no binding replaces it, so any card that re-introduces a jump is a defect against this unit.

Chords are not the card's fallback: D9 keeps chords for no-anchor actions, and the card now has an anchor — itself — so a chord re-adding the jump would be out of scope for both units.

**Up-arrow history in the composer (spine C1) and a slash-command palette on `/` (spine C2) — both claim composer keys.** If this unit's scheme uses chords or a leader key anywhere near the composer, that is the collision to name rather than discover later. Concretely:

- `up-arrow` becomes history recall in the composer; a chord for `toggle_agents` must not require `up-arrow` as a bare key or as `ctrl+up` that shadows it, because the composer, not the app, must see the arrow first. The provisional `ctrl+g` avoids this.

- Bare `/` becomes the palette. Any chord using `/` as an initial keystroke loses to the palette's own claim, and any leader that started with `/` would argue with typing a slash command in full. The palette's `dependsOn` is the composer's `on_key` handling, so the ordering is: composer consumes `up-arrow`/`/` first, app bindings fire only for what the focused widget does not consume.

- `end` stays the re-follow key (`app.py:1500-1513` already pairs it with `F5`). No chord may take `end` as a chord member — it is a bare scrolling key, not a chord prefix.

Neither seam changes the row's reduction: the composer claims `up-arrow` and `/`, the row claims `F8`/`F9`/`F10`, and the four redundant chords claim `ctrl+` letters that the composer does not use. The claim to check is that no `Binding` in `app.py:816-845` binds a bare `up-arrow` or bare `/`.

## What this unit does not do

- **It does not re-decide where focus goes.** No snap-back is added; the `CaretReleased` take-away path (see unit B1) and the deliberate pane moves are untouched. The operator's proposal that focus always returns to the input box is not re-litigated here.

- **It does not change the domain core.** No change imports the terminal framework into domain state, and no binding adds a domain transition (ADR-0002, `platform-specs/04-architecture/adrs/0002-the-domain-core-is-framework-independent.md:36-51`). Replay controls still route through `ReplayControls` (`app.py:1445-1448`) and `control.attempt` rather than dispatching.

- **It does not plan a measurement step.** Whether `F5` is alive, and what `F4`/`F10` do when pressed, remains operator-only and is not a step in this plan (brief, measurement gap). The plan's dispositions survive either answer.

- **It does not add a live gateway step.** No part of the plan requires `talaria --record` or `talaria replay` against a running gateway; all operator-only acceptance is marked as such.

- **It does not touch `src/`.** The TypeScript reference recorder is not behaviour to extend and not bootstrap to delete (`AGENTS.md:21`).

- **It does not claim the startup probe decides the row.** No detection of an eaten key is added at startup or later; the redundant path is the fix, which is exactly why `F10`'s provisional primary tolerates a wrong guess.

- **It does not reintroduce the jump key under another name.** Removing `F1` is load-bearing for the answerability spine; a slash word or chord that jumped to the card would undercut unit A1, so it is named as out of scope rather than discovered later.
