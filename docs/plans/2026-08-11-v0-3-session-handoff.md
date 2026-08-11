---
title: Handoff into v0.3 — what v0.2 shipped, what two hours in a real terminal found, and what to decide first
type: handoff
status: active
date: 2026-08-11
origin: docs/analysis/2026-08-10-v0-2-hands-on-notes.md
---

# Handoff into v0.3

Written 2026-08-11, at `main` = `06dc858`, immediately after v0.2.0 was released. This is for a
session that starts cold. It says where the repository actually is, what v0.2's gates do and do not
license anyone to claim, which defects are known with their mechanism already identified, which are
merely sighted, and which loose ends the v0.2 release itself left behind.

**It does not choose v0.3's scope.** That is the first real decision of the next session, and the
candidates are set out at the end with the evidence behind each rather than a recommendation dressed
as a plan.

**The one thing to carry over if nothing else is read:** v0.2 passed a 24-of-24 replay gate, six
external review rounds and roughly 1,700 tests, and then about two hours of driving it by hand in a
real terminal produced seven defects — including one that makes v0.2's own headline feature unusable
by keyboard. The full account is in
[the hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md), and the general lesson is
recorded in [LEARNINGS.md](../engineering-journal/LEARNINGS.md) under 2026-08-10.

## Where the repository is

- `main` at `1cd176a`, clean, no open pull requests.
- **One branch besides `main` remains, and it must not be deleted.** `outcome/talaria-v0-2` — local
  and remote — carries the v0.2 outcome specification at `docs/outcomes/talaria-v0-2/outcome-spec.json`
  and is deliberately never merged to `main`. The specification lives nowhere else, so deleting the
  branch destroys it. Every other v0.2 branch was deleted on 2026-08-11 after each was re-verified as
  an ancestor of `main` immediately beforehand.
- **The tag `evidence/block-markdown-gate` is not a release tag and must not be deleted either.** It
  preserves the build history that
  [the gate results](../analysis/2026-08-09-block-markdown-gate-results.md) cite by hash — the
  confirming runs `67589a9`, `4498bec` and `2e96324`. That work reached `main` through pull request 49
  as a *different* set of commits, so none of the cited hashes is reachable from `main`, and the branch
  that held them is gone. Without the tag, the evidence behind a published release would cite commits
  nobody can check out.
- **v0.2.0 is released** — <https://github.com/infiquetra/talaria/releases/tag/v0.2.0>, carrying
  `talaria-0.2.0-py3-none-any.whl` and `talaria-0.2.0.tar.gz`, both built by continuous integration
  from the tagged tree. Install is unchanged from v0.1:
  `uv tool install git+https://github.com/infiquetra/talaria@v0.2.0`. The name `talaria` on the Python
  Package Index belongs to an unrelated project and the name request stays deliberately deferred.
- **The tag points at `06dc858`, not at the release merge `d925891`.** Those two commits differ by
  documentation only — 789 lines of hands-on notes, two journal entries, and five italic markers in
  ADR-0006 — with zero changes under `talaria/`. `d925891` was chosen against because its Validate
  run is red; see *Loose ends* below for why that went unnoticed for two merges.
- Validate is green on `06dc858`, on both the pull-request run and the push-to-`main` run.
- Full project check run locally at this commit and green: `ruff check` clean, `mypy` clean,
  **1708 passed / 1 skipped**, `bandit` clean, `git diff --check` clean. One green run is not proof —
  the v0.1-era intermittent suite failure has not recurred, but neither has it been diagnosed.
- **Five architecture decision records are `accepted`; ADR-0006 is still `proposed`** even though its
  own stated acceptance condition has been met. See *Loose ends*.

## Read these first, in this order

1. **[CLAUDE.md](../../CLAUDE.md)** and **[AGENTS.md](../../AGENTS.md)** — the repo's own rules.
2. **[The v0.2 hands-on notes](../analysis/2026-08-10-v0-2-hands-on-notes.md)** — nineteen operator
   notes from the first hand-driven session, and a sorted candidate list at the top. Everything in
   the three findings sections below is drawn from it. Read the sorted section; dip into the numbered
   notes only for a finding you intend to act on.
3. **[The previous handoff](2026-08-08-v0-2-session-handoff.md)** — its *Invariants* and *Traps*
   sections are still current and are not repeated here in full. Read them; they are the rules a
   reasonable change violates by accident.
4. **The six architecture decision records** in
   [platform-specs/04-architecture/adrs/](../../platform-specs/04-architecture/adrs/). These are
   constraints, not history.
5. **[QUEUED.md](../engineering-journal/QUEUED.md)** and
   **[DECISIONS.md](../engineering-journal/DECISIONS.md)** — the deferred work and the durable
   decisions. QUEUED's `## P0` section opens with the two entries this handoff expands on.

## What v0.2 does *not* prove

Three of these are published in the release notes. The fourth corrects one of them, and the
correction matters more than the original.

1. **No person has driven the interface on Linux.** Unchanged from v0.1. The suite runs there in
   continuous integration, pseudo-terminal and process-surface tests included. Nobody has used it
   there.
2. **"No run on either platform has used a real terminal emulator" is now out of date, and only
   half-superseded.** The shipped v0.2.0 notes still carry that sentence, because the drive happened
   after they were written. As of 2026-08-10 the interface *has* been driven by hand in a real
   terminal emulator, on macOS, for roughly two hours — and that session is where every defect below
   came from. Linux remains untouched by a human, and the macOS drive covered the six features it set
   out to demonstrate rather than the whole surface: `F4` and `F10` were never pressed at all.
3. **A still-open streaming table re-renders wholesale per append** and crosses the 50 ms apply
   ceiling at roughly 340 rows until the 500-row fallback demotes it. Recorded verbatim in the gate
   results as `block_phase_peak_ms`, with the early-demotion fix queued.
4. **An ambiguous approval outcome settles and latches** rather than restoring the card, because the
   approval wire carries no request id and a restored card could be a zombie no keystroke can kill.
   Over-latching self-heals through gateway expiry.

Also still standing from v0.1, and worth re-reading in the previous handoff before touching anything
near it: a credential the shell exported before Talaria started stays visible in the process
environment for the life of the process, and **nothing may be written anywhere claiming R1's
environment clause is met.**

## The finding that should shape v0.3

Two things came out of the hands-on drive that are larger than any single defect.

**Every defect below survived the full verification apparatus, because all of them live in the seam
that apparatus explicitly does not cover:** a real terminal, on a real desktop, driven by a real
person. Both v0.1 and v0.2 record "no run on either platform has used a real terminal emulator" as a
known limitation. Two hours of hands-on driving is what that limitation was worth. The generalizable
form is in LEARNINGS.md: a gate measures the claim it was built to measure, and a claim about a
person cannot be measured without one.

**Four separate findings are one problem: Talaria does not confirm what it just did.** A status row
nobody can interpret (`caret:`), a number without its scope (the fallback banner), a card advertising
keys that do nothing (`enter select · esc decline`), and a keypress indistinguishable from a dead one
(`F1`). That is a tighter release theme than "readability", and it absorbs most of the transcript
legibility complaint as a consequence rather than as a separate project.

## Defects — diagnosed, with the mechanism named

Seven, all from the hands-on drive. The first is the only P0.

### P0 — the approval card has no keyboard path on macOS

**This is v0.2's own headline feature, unusable by keyboard on the only platform anyone drives.** A
prompt card never takes focus when it mounts unless it is input-backed —
`talaria/ui/prompts.py:1171` reads `if focus_new and isinstance(card.action_widget, Input)` — and the
comment directly above states the consequence plainly: every other kind "is reachable exclusively
through the jump". An approval card is button-backed. The jump is `F1`, and `F1` never arrives: it
was pressed repeatedly against a live approval and nothing moved. The card meanwhile prints
`enter select · esc decline`; both were tested with focus in the composer and neither did anything.
The card was answerable only with the mouse.

`F1` is not mis-bound. `talaria/ui/app.py:770` binds it to `jump_to_prompt` with `priority=True` —
the identical form used by `F8`, `F9` and `F10` on the three lines below, and `F8` and `F9` were both
driven successfully in the same session. Function keys as a class reach the application. What
specifically claims `F1` is not yet established, and it decides the size of the fix: an alternate
binding on a surviving key is nearly free, whereas "the desktop owns our whole hotkey row" is a
redesign.

Full entry in [QUEUED.md](../engineering-journal/QUEUED.md) under `## P0`.

### Mouse selection lands several rows off

A double-click landed several rows above the clicked line. This is separately why the terminal's own
select-and-copy does not reach through the Talaria pane, while working normally in every other pane
of the same terminal — so it is Talaria's behaviour, not the terminal's. Suspect: the mixed-height
widget layout v0.2 introduced. Undiagnosed, and it matters more than it looks, because the mouse is
currently the *only* working path to an approval card.

### `platforms.changed` floods the transcript

Twenty-six rows in a single turn. The one-line fix is adding it to `_OBSERVED_ON_A_LIVE_GATEWAY` in
`talaria/domain/decode.py:110`, a set whose own comment already names this exact job: "the place any
future live capture should add to". Worth deciding at the same time whether repeated unknown events
should coalesce rather than each taking a row.

### The fallback banner reports the retained count under the word "clipped"

The banner names the number of lines *kept*, phrased as though it were the number *hidden*. Proven
live: as more content arrived and more was hidden, the number **fell**, 499 to 494. Suggested
wording: "showing the last 499 of 600 lines".

### The condensed marker and the fallback banner are measured against different scopes

The condensed marker counts pane-wide (`self._top + self._tail_top`); the banner counts within the
entry. Neither answers the question an operator actually has, which is "where does this entry start
and how long is it". Fix the two as one change, not separately.

### The shipped release notes describe `F4` by half

Both `docs/releases/v0.2.0.md:21` and `CHANGELOG.md:26` say `F4` "sweeps the answerable set". It
first **interrupts the in-flight turn** (`app.py:776` binds it to `action_interrupt`, which sweeps at
`:1666` only after the interrupt is confirmed). The omitted half is the destructive one. Both files
are published; see *Loose ends* for the decision this needs.

### `F2` is eaten by macOS Mission Control

Confirmed by the operator. `F2` is the sub-agent monitor toggle.

## Undiagnosed — evidence needed before these can be called anything

Do not schedule these as fixes. Schedule the evidence.

- **Duplicated rendered content.** Sighted twice, in different sessions. One reproduction attempt is
  recorded and **failed** — the streamed deltas measured 1,950 characters against `message.complete`'s
  1,948, and each heading appeared exactly once. That is a real negative result and it narrows the
  next attempt: try a turn where the model speaks, calls a tool, then continues.
- **What specifically eats `F1`.** macOS Help, the brightness key row, or the host terminal. This
  decides whether the P0 fix is one alternate binding or something larger.
- **Whether `F5` is alive.** It was pressed at the bottom of a paused replay, where its action — follow
  bottom — is a legitimate no-op. Scroll up and press it again. This is a thirty-second test.
- **`F4` and `F10` have never been pressed.** Note that `F4` interrupts before it sweeps, so test it
  deliberately rather than casually.

## Design questions — not defects, and not to be argued as if they were

- **Should focus always return to the composer?** The operator's own proposal, after finding the
  `caret:` row uninterpretable. The tension is real: `F1` and `F4` move focus deliberately, so an
  unconditional snap-back breaks the answerability spine. A narrower rule may work.
- **Should an answer carry notes or caveats?** The operator's framing: "I might agree with the choice
  but with caveats." Their own open question is whether this belongs on an approval or only on a
  clarify. Check first whether `approval.respond` can carry free text at all — if it cannot, the
  question is moot at the wire before it is a design question.
- **`--resume` resumes the gateway's most recent session, not yours.** Correct per `session.most_recent`
  and silently surprising on a gateway shared with automation. Cheapest fix is naming the resumed
  session on arrival, which is also an instance of the release theme above.
- **The composer needs the conventions every comparable interface has** — up-arrow history, and a
  filterable slash-command palette on `/`.
- **The approval card is cramped**, and **the transcript is visually busy and hard to differentiate.**
  The second only became visible against real transcripts with real content; against the synthetic
  content of every previous test it looked fine. The operator's own verdict on the parts that work:
  tables "great", code "very readable".

## Not Talaria's to fix

Recorded so nobody re-investigates them.

- The 256k context window reported for `muse-spark-1.2-contributor` — the gateway's model catalogue,
  not Talaria's reading of it.
- Approvals not firing during the walk — the operator's Hermes profile had `approvals.mode` set to
  `off`. Flipped to `manual` for the test and restored afterwards, verified by re-reading the file.
- An expired approval defaults to **deny** — the safe default, working as intended.

## Loose ends left by the v0.2 release itself

Four, all verified against the live repository on 2026-08-11 rather than inferred. The first is
**closed**; it is kept here because the rule it produced outlives it.

### ~~The v0.2.0 release has no wheel and no source distribution attached~~ — CLOSED 2026-08-11

The release was created by hand with `gh release create` about a minute before the tag-triggered
Release workflow reached its final step, and that step then failed with "a release with the same tag
name already exists: v0.2.0". `v0.1.0` carried both distributions; `v0.2.0` carried no assets at all.

The failure was confined to that one step. Steps 1 through 15 of the workflow all passed on the
tagged tree — the tag agreed with the package version, ruff, mypy, pytest and bandit ran green on
continuous integration, the gate block read READY, the distributions built, and the built artifact
installed clean into a fresh environment and reported its version. The release was validated and
merely undelivered.

**Repaired the same day** by deleting the GitHub release — never the tag, which stayed at `465649e4`
throughout — and re-running the failed workflow run, so the attached
`talaria-0.2.0-py3-none-any.whl` and `talaria-0.2.0.tar.gz` are the ones continuous integration built
from the tagged tree, matching how `v0.1.0` got its assets. The recreated release was checked
byte-for-byte against a backup taken before the delete: identical body hash, title, and prerelease
flag.

**The rule this produced: this repository publishes releases from tags via
`.github/workflows/release.yml`. Push the tag and let it run; never create the release by hand — and
after any release, check the assets rather than the release page, because a validated-but-undelivered
release looks completely normal from the outside.**

### `main` was red for two consecutive merges, and nothing stopped it

Branch protection on `main` requires exactly two checks: `python-check (3.12)` and
`python-check (3.13)`. The Validate workflow also runs a Node job named `check` — `npm run typecheck
&& npm test && npm run format:check` — and that job is **not required**, so a failure in it does not
block a merge. It failed on a Prettier formatting violation in ADR-0006, introduced during the
block-markdown build and merged twice: once with the block-markdown work (`05ecaa6`) and once with
the v0.2.0 release itself (`d925891`). It was fixed in `5211a8c`.

The decision this needs is whether `check` and the `install` jobs join the required set. The argument
for: a formatting job that cannot block anything is a job that will silently drift again, and it
already did, straight through a release. The argument against: `check` covers the TypeScript
reference recorder, whose failure mode is narrow. Either answer is defensible; leaving it undecided
is what produced two red merges.

### ADR-0006 is still `proposed`, and its acceptance condition has been met

The record states its own condition in its opening note: it is "`proposed` rather than `accepted`
until that gate runs green under the restated claim". That gate ran green — 24 of 24, confirmed
across three runs ending at `2e96324`, published in
[the gate results](../analysis/2026-08-09-block-markdown-gate-results.md). Either flip the status to
`accepted` or record why it is being held open; a record whose stated condition is satisfied while
its status disagrees is worse than either.

### The `F4` half-description is public

Both `docs/releases/v0.2.0.md:21` and `CHANGELOG.md:26` are published and describe a destructive key
as though it were only additive. Decide deliberately between correcting them in place — which edits a
shipped release's notes — and correcting forward in v0.3's changelog entry. This handoff takes no
position; it only insists the choice be made rather than forgotten.

## Invariants — what changed, and what did not

**The previous handoff's *Invariants* and *Traps* sections are still current in full.** Read them
there. Three things changed or are worth restating because v0.3 is likely to walk into them.

- **ADR-0006 now governs transcript rendering.** "One line, one widget" is no longer the bounded
  rendering claim; the replacement is bounded by work and height, and `interface_shows_everything` in
  `talaria/replay/gate.py` was restated against it. Anything touching the transcript pane must read
  ADR-0006 before it reads the code.
- **The TypeScript tree under `src/` is still not dead code**, and its description has changed since
  the v0.1 handoff: the bootstrap was removed on 2026-08-07, and what remains is the three-file
  reference recorder that `tests/recorder/test_equivalence.py` asserts the Python recorder is
  equivalent to across the credential redaction boundary. Do not extend it, do not port it, and do
  not delete it without first saying what replaces the redaction equivalence guarantee.
- **This repository is public.** No operator profile name, profile path, or other operator-specific
  inventory in any committed fixture, document or commit message — R12, and it has already been
  violated once by a document written in the same session that produced this handoff. Machine-specific
  facts belong in session memory, not here.

## Candidate scope for v0.3 — evidence, not a recommendation

An option space. The right first move is to pick a spine and let the rest follow.

**A. Make the answerability spine actually reachable.** The P0 keyboard path, plus the mis-aimed
mouse — because the mouse is the fallback the P0 currently depends on, so fixing one without the
other leaves the feature resting on a known defect. This is the only candidate that repairs something
v0.2 already claimed to ship, and the claim is in the published release notes.

Sequencing note: the cheap diagnosis of what eats `F1` comes first and may collapse this from a
redesign into a one-line rebinding. Do that before estimating.

**B. Talaria confirms what it just did.** The theme above, as a release: the `caret:` row, the
fallback banner's wording and scope, the card's hint line matching what the keys actually do, and
some acknowledgement that a keypress was received. It absorbs four findings that look unrelated, and
it converts the transcript-legibility complaint from an open-ended project into a set of specific
answers. It overlaps candidate A rather than competing with it — the card's lying hint line belongs
to both.

**C. The composer conventions.** Up-arrow history and a filterable slash-command palette. Smallest
coherent feature-shaped candidate, entirely additive, no gate reopened, and the operator named both
unprompted while driving. Weakest claim on urgency; strongest claim on being finishable.

**D. Close the evidence gaps.** Drive the interface on Linux by hand; widen compatibility checking
below the top level, where `compare_shape` is queued for exactly this. One of the six published
limits has just been half-closed for free by the macOS drive — the honest way to close the rest is
the same way, which is by hand.

**E. A diagnosis pass, as a prerequisite rather than a release.** The four undiagnosed items are
cheap and two of them gate estimates elsewhere: what eats `F1` sizes candidate A, and the duplicated
content could be anything from a rendering defect to nothing at all. Half a day here makes every
other estimate honest. Consider it a precondition on whichever spine is chosen, not a candidate
competing with them.

**Not in scope unless deliberately chosen:** the Python Package Index name, deferred on 2026-08-08 and
unchanged — it reopens only when the name is settled *and* there is intent to publish.

## What this handoff deliberately does not do

- **It does not schedule anything.** Nothing above is committed work; the sorted list it draws from
  says so explicitly, and priorities in QUEUED.md are the author's judgement, not a decision.
- **It does not re-open v0.2's gates.** The block-markdown gate's 24-of-24 result stands and no figure
  in it is re-graded. What the hands-on drive found is not a contradiction of that gate — it is the
  seam beside it.
- **It does not pre-authorize anything.** No standing permission to push, open a pull request, merge,
  tag, or publish. This is context for a session that starts with a person in it.
