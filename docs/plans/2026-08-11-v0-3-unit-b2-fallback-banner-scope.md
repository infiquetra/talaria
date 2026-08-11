---
title: v0.3 unit B2 — the fallback banner names what is hidden, and the two markers share one scope
type: plan
status: proposed
date: 2026-08-11
charter: docs/plans/2026-08-11-v0-3-orchestration-charter.md
unit: B2
---

# Unit B2 — the fallback banner names what is hidden, and the two markers share one scope

A two-number screen defect and the clearest instance of the release theme that is not B5: **a number
on screen that means the opposite of what its label promises, sitting beside a second number that
counts a different population while looking like the same kind of thing.**

**The finding, restated from the charter.** The fallback banner prints the *retained* row count under
the word "clipped", proven live by the number falling from 499 to 494 as more was hidden. The
condensed marker at the top of the pane counts folded rows pane-wide; the banner counts rows within
one entry. The charter has already decided the two ship as one change, because neither, fixed alone,
answers the operator's question: **how much am I not seeing, and where is this entry hiding it?**

## Mechanism — verified by reading, at `main` = `fcab675`

Two markers on one screen, two counts, one of them backwards.

### The condensed marker already names its scope and already counts hidden lines

`CONDENSED_TEMPLATE` (`talaria/ui/transcript.py:125`) reads
`"── {count} earlier lines condensed (still readable by the agent)"`. It is rendered by
`_render_condensed` (`transcript.py:1681-1692`), mounted at the pane's top (`before=0`, `:1690`), with
`count = self.condensed_count` (`:1687`). `condensed_count` is `self._top + self._tail_top`
(`:723-730`) — the monotone committed prefix plus the assistant tail's folded head rows, i.e. the
rows folded out of the top of the visible window **across the whole pane**. "Earlier" names the scope
(above the window, pane-wide) and the number is genuinely the count of lines not shown. **The
condensed marker is not the defect.** It is the reference the banner is measured against.

### The fallback banner prints the retained count under "clipped"

`FALLBACK_BANNER_TEMPLATE` (`transcript.py:130-133`):

```
── entry too large to render as markdown ({lines} lines clipped at the viewport edge, full content still readable by the agent) ──
```

The `{lines}` is bound to `len(widgets)` at the one construction site, `transcript.py:1206`
(`_fallback_banner(len(widgets))`). `widgets` is built at `:1203-1205` from `source_lines` **after**
the cap slice at `:1201-1202` (`source_lines = source_lines[len(source_lines) - allowed:]`), so the
number is the count of rows **kept** on screen — `allowed = max(1, max_rows - 1)` for a fallback
unit (`:1200`), i.e. `DEFAULT_MOUNT_CAP` (500, `:118`) minus one banner row. The same retained count
is re-bound at every refresh site: the tail-growth path (`:1438`), the committed partial fold
(`:1557`), the tail fold (`:1570`), and the retarget path (`:1130`) all call
`_banner_text(len(unit.lines))` or `_banner_text(len(tail.lines))` — always the surviving widget
count, never a loss.

The template's phrase "at the viewport edge" is why the author shipped a retained count: each
retained row is rendered `no_wrap` (`:1204`) and its right edge is cut at the viewport, so "N lines
clipped at the viewport edge" is technically true of the rows on screen. It is the reading that is
wrong. "499 lines clipped" reads as "499 lines were removed", and the arithmetic compounds the
misreading: a 600-row entry keeps 499 and drops 101, and the banner shows the one number that is not
the loss.

**The live evidence is on the record, not inferred.** The hands-on notes capture the banner as it
rendered for a 600-line `print` (`docs/analysis/2026-08-10-v0-2-hands-on-notes.md:210-224`):
`── entry too large to render as markdown (499 lines clipped at the viewport edge, full content still readable by the agent) ──`.
The notes' source trace (`:472-502`) reads the same call chain this plan does. And the observation
that settles it — the count *fell* from 499 to 494 after one more turn pushed five rows into the fold
(`:535-542`): if the number meant "removed", it could only rise as more was hidden. It fell because it
is the retained count and retention shrank. That inversion is the defect, observed, not asserted.

### The two markers count different populations, and one of them counts a different quantity

The pane on that session showed both markers at once: `── 317 earlier lines condensed ...` at the top
(`:498-502`) and `499 lines clipped ...` at the bottom (`:220`). The notes' sharper reading
(`:503-521`):

| On screen | Value | Scope | Reads as |
| --- | --- | --- | --- |
| Top marker | 317 | whole pane, all earlier entries | "this entry lost 317 lines" |
| Bottom banner | 499 | this entry's retained rows | "this entry lost 499 lines" |

Both numbers are correct against their own definitions and both invite the same wrong reading. The
top marker counts hidden rows pane-wide; the banner counts *shown* rows entry-scoped. An operator
cannot compose them into an answer to "how much am I not seeing".

### Why the change is rendering-only, and where the boundary is

The banner, the condensed marker, `condensed_count`, the cap slice, and both row splitters
(`_welded_entry_lines` at `transcript.py:567-579`, `_welded_tail_lines` at `:552-564`) all live in
`talaria/ui/transcript.py`, a presentation module. The domain core never imports the terminal
framework (ADR-0002, `accepted`; enforced by check per `AGENTS.md:60`). This plan changes nothing a
domain module owns: no state transition, no projection rule, no record shape. The fix is confined to
what the banner's text says, which is the presentation layer's own statement about its own
rendering. The ADR-0002 boundary is not approached, let alone crossed.

## Key technical decisions

### KTD1 — the banner reports what is hidden, and names the total

The banner prints the **hidden count** — the entry's total projected rows minus the retained mounted
rows — together with the total: `"{hidden} of {total} lines of this entry hidden"`. For the hands-on
600-row entry that is `"101 of 600 lines of this entry hidden"`.

Three properties decide it:

1. **It answers the operator's question.** "How much am I not seeing" is a loss question, and the
   number that answers it is the loss. The retained count answers "how much can I read here", which
   is a secondary question the operator did not ask — the live 499→494 inversion shows they read the
   number as a loss even while it was a retention.
2. **It fixes the inversion.** The hidden count only ever rises as more is hidden (101 → 106 when
   five more rows fold), which is the direction "hidden" promises.
3. **The total makes the retained count derivable.** The hands-on notes' own completeness criterion
   — "neither is 600, neither is 101" (`:518-519`) — wants the total on screen. `"101 of 600 lines
   of this entry hidden"` carries hidden *and* total in one phrase, so retained (499) is arithmetic
   the operator can do.

**Rejected — "showing the last 499 of 600 lines".** This was the notes' own first suggestion
(`:494-496`) and it is the strongest rejected alternative. It is strictly informative about what is
on screen, but it keeps the two markers counting *opposite* quantities — the top marker counts
hidden rows pane-wide, the banner would count shown rows entry-scoped — which is exactly the
incommensurability B2 exists to remove. Once the banner counts hidden rows like the top marker does,
the two numbers compose (KTD3); a retained-count banner can never compose with a hidden-count marker.
The retained count is still recoverable from the chosen wording, which drains most of the force from
"strictly more informative".

**Rejected — hidden count without the total.** It answers the loss question but not the scale
question ("how long is this entry"), which the notes name as part of the operator's actual want
(`:518-519`). The total costs one placeholder and makes the retained count derivable.

**Rejected — printing both counts explicitly.** Redundant: hidden-plus-total already yields retained.
A longer sentence buys nothing and this release is separately trying to quiet the transcript.

### KTD2 — the label drops "clipped at the viewport edge" and names the entry scope

The word "clipped" is doing double duty and one of its jobs is the defect. It correctly names the
horizontal cut of each retained `no_wrap` row, but the number being printed is the vertical loss, and
an operator reads "clipped" as "removed". The new wording says "hidden" — the plain word for "not
shown" — and adds the scope word "of this entry" so the banner's extent is stated, not implied. The
two surviving clauses stay: the fallback cause ("entry too large to render as markdown") and the
reassurance ("full content still readable by the agent"), both part of the RA3 contract the banner's
own comment at `transcript.py:127-129` describes.

The count clause is always present, including `"0 of N lines of this entry hidden"`. A fallback can
demote on the *descendant* trigger alone — an entry with few rows but over 1,200 descendants
(`trips_fallback_trigger`, `transcript.py:435-442`) keeps every row, hidden count zero — and "0 of N
lines of this entry hidden" is truthful there and even useful: it tells the operator the whole entry
is on screen, only line-rendered. One template, no conditional, and every mounted banner carries a
deterministic, assertable count clause. The exact sentence is settled at implementation against the
constants' register, following `NO_SESSION_TO_RESUME`-style precedent, but the semantics — hidden
count, total, entry scope, the two surviving clauses — are decided here.

**Rejected — keeping "clipped at the viewport edge" with the new number.** The hidden rows are the
entry's head, folded away; they are not "at the viewport edge" at all. The phrase describes the
horizontal clip of the rows that remain, which is not the number now being printed. A label that
names one axis while the number counts another re-installs the same wrongness in new clothes.

### KTD3 — the two markers share one quantity, at two named scopes

The charter's phrase "the two markers share one scope" is not attainable literally — the condensed
marker is a pane-top widget reporting the whole transcript's folded prefix, and the banner is
attached to one entry; neither can become the other. What is attainable, and what the phrase means
operationally, is **one shared definition with two named extents**: both markers count hidden lines,
and each label says which extent it covers. The condensed marker already reads "N earlier lines
condensed" — "earlier" is its scope word, "condensed" is its quantity word. The banner becomes "N of
M lines of this entry hidden" — "of this entry" is its scope word, "hidden" its quantity word. Two
labels that are clearly distinct in scope and consistent in quantity.

The composition property is real, not cosmetic, for the populations that share the line-buffer
identity. A fallback committed entry's hidden head rows are folded into `_top` by `_compute_new_top`
(`transcript.py:1593-1677`: the fallback branch at `:1668-1673` returns `start + (count - retained)`,
which `_condense` lands in `_top` at `:1582`), and a fallback assistant tail's hidden rows land in
`_tail_top` (`:1560-1562` and `:1581`). So at a settled checkpoint, the banner's hidden count is a
subset of the condensed marker's count, and the operator can decompose: "317 lines are above the
window; 101 of them are this entry's hidden head; the other 216 are earlier entries." The notes'
sharpest complaint — that neither number tells the operator where the entry starts — is answered
without a new position display, because the banner's total and the subset relationship locate the
entry relative to the fold.

**One boundary, stated rather than hidden:** a fallback *reasoning* tail's hidden rows are not in
`condensed_count` — the reasoning tail has no span in the line buffer by design (R18;
`_compute_new_top`'s docstring at `transcript.py:1631-1634`), so its pre-capped head rows are a
genuinely separate population. The banner on a reasoning tail still says "of this entry hidden",
which is truthful on its own; it simply does not compose with the pane-top number. The label's scope
word is what makes that legible rather than a silent overlap.

**Rejected — forcing one literal scope.** Making the banner report the pane-wide condensed count
would be a category error: the banner sits inside one entry and must name what that entry hid, and a
pane-wide number on an entry-attached row lies about what it is attached to. Making the condensed
marker entry-scoped would break the pane-wide identity the gate's ownership proof reads numerically
(`condensed_count` in `gate.py:796`, `:926`, `:948`, `:957`) and the invariant
`rendered_lines == view.lines[condensed_count:]` that `test_transcript_bounds.py:84` pins. Both
directions fail; distinct labels are the only honest route.

### KTD4 — no new state; the total is derived at each call site, and one site has a trap

The hidden count is always `total − retained`, and both operands are already in scope at every banner
call site. `_MountedUnit.applied_text` holds the full current body — it is never trimmed by
condensation (the partial fold at `transcript.py:1544-1557` pops head *widgets*, not text) — and the
row count of that text is computed by the same splitter the construction path already chooses
(`:1187-1190`): `_welded_tail_lines` for a tail (a string `entry_id`), `_welded_entry_lines` for a
committed entry (an int `entry_id`). Retained is `len(unit.lines)`, already passed everywhere.

The five call sites and the total each must supply:

| Call site | Retained | Total |
| --- | --- | --- |
| Construction, `transcript.py:1206` | `len(widgets)` | `len(source_lines)` pre-slice — in scope at `:1190` |
| Retarget, `:1130` | `len(unit.lines)` | `len(_welded_entry_lines(unit.entry_kind, unit.applied_text))` |
| Tail growth, `:1438` | `len(unit.lines)` | `len(new_lines)` — see the trap below |
| Committed partial fold, `:1557` | `len(unit.lines)` | `count` from `record.line_span`, or `len(_welded_entry_lines(...))` |
| Tail fold, `:1570` | `len(tail.lines)` | `len(_welded_tail_lines("assistant", tail.applied_text))` |

**The growth-site trap.** At `:1438`, `unit.applied_text` is still the *pre-growth* text — it is
updated to `tail.raw_text` at `:1455`, seventeen lines later. A total computed from `unit.applied_text`
there would be one boundary stale, and the hidden count would silently under-report on every
boundary of a growing stream — the exact failure class this unit repairs. The growth site must bind
the total from `new_lines` (`:1407`, `len(_welded_tail_lines(kind, tail.raw_text))`), which is in
scope, or the update at `:1455` must move before the banner refresh. The plan pins the requirement;
implementation picks the cleaner of the two.

**Rejected — caching the total on `_MountedUnit`.** A stored `total_rows` must be updated in lockstep
with every path that changes the body — growth, retarget, adoption — and a missed update reproduces
the stale-count defect with better bookkeeping. Deriving from the already-consistent `applied_text`
(or the in-scope `new_lines` at growth) removes the state that could go stale instead of managing it.

### KTD5 — the fold arithmetic, the trigger, and the identity all stand untouched

This change alters only what `_banner_text` and `_fallback_banner` (`transcript.py:582-626`) print and
what their call sites pass. Nothing else moves: the fallback trigger (`:435-442`), the cap slice
(`:1200-1202`), `_condense` (`:1491-1583`), `_compute_new_top` (`:1593-1677`), `condensed_count`
(`:723-730`), and the rendered-lines identity — which explicitly excludes the banner as chrome
(`:738`, KTD2) — are all read-only to this change. Because the banner stays chrome (never a projected
content line) and stays exactly one painted row (`no_wrap=True` at `:624` plus the `height: 1` rule
at `:643-646`), the one property the replay gate's `fallback_banner_accounting` asserts is preserved
by construction even though the sentence it asserts about is now longer. `test_fallback_banner_paints_exactly_one_row`
(`tests/ui/test_transcript_blocks.py:778`) pins it across pane widths.

## Risk this unit must clear

**The replay gate's two settled-transcript checks are text-agnostic, which is both the easy and the
weak half of the story.** `content_is_complete` (`talaria/replay/gate.py:1023-1047`) walks the
domain's entries and requires each projected line to appear in `view.lines`; the banner is never a
projected line, so the banner's text is invisible to it. `interface_shows_everything`
(`gate.py:996-1020`) composes `ownership_report`, whose banner proof is `fallback_banner_accounting`
(`gate.py:828-896`) — it reads only `.transcript--nowrap`/`.transcript--fallback-banner` classes and
`widget.outer_size.height`, never the banner's text — and whose line-window half
(`rendered_lines == view.lines[condensed_count:]`) excludes the banner by construction. So neither
check can disagree about a wording change, provided the banner stays one row and stays chrome.

**But the gate also cannot see the fallback path at all, and that is the weak half, stated rather
than papered over.** The gate replays the stress corpus twice — unbounded and on the recorded cadence
— and the feature corpus through `measure_replay` (`gate.py:1482-1519`), plus a live corpus only when
one is supplied. The stress corpus's delta fragments are short prose (`talaria/replay/stress.py:66-75`)
and its turns assemble to a few dozen wrapped rows — comfortably enough to *condense* (the corpus
comment at `stress.py:49-51` says condensing happens hundreds of times), but nowhere near the fallback
trigger of 500 wrapped rows or 1,200 descendants — and the feature corpus's constructs are short
fences, tables, quotes, and parser attacks (`stress.py:287-303`, `:380-469`). No corpus entry trips
the trigger, so **no fallback banner ever mounts under the gate**. The gate does exercise the
condensed marker path (the stress corpus drives `condensed_count` and `_render_condensed` hundreds of
times), and this change leaves `condensed_count` and the condensed marker's text untouched, so that
leg is green for the uninteresting reason that it is unchanged. The banner's wording is proven by the
unit tests below, and only by them. A green gate after this change proves the identity still holds;
it does not prove the banner says anything new.

**Two unit tests assert the current banner text, and both must change with a comment naming this
plan, not quietly.** `test_a_growing_fallback_tail_reuses_its_mounted_widgets`
(`tests/ui/test_transcript_blocks.py:805`) asserts `str(len(unit.lines)) in str(unit.banner.render())`
at `:854` — the retained count appears in the banner. Under the new semantics the banner prints the
hidden count and the total, not the retained count; the assertion must be re-expressed on the new
wording. (It may pass by coincidence — with `mount_cap=2000` the total string can contain the
retained length — and that is exactly why it must be rewritten rather than left.) And
`test_a_partial_fold_refreshes_the_banner_count` (`:1398`) asserts `"(1 lines clipped" in
str(unit.banner.render())` at `:1425` — the partial fold refreshes the banner to the new hidden
count, and the assertion text changes with it. Both tests' *intent* survives: the banner follows a
partial fold, and the banner follows a growing tail. `test_the_condensed_block_is_one_widget_no_matter_how_much_it_covers`
(`tests/ui/test_transcript_bounds.py:94`) asserts only that the condensed marker's content contains
`str(pane.condensed_count)` (`:103`); the condensed marker is unchanged, so it survives untouched.

**The gate's own banner tests are text-free.** `test_fallback_banner_accounting_*`
(`tests/replay/test_gate.py:933-978`) drive `fallback_banner_accounting` with duck-typed panes that
carry classes and `outer_size` but no text; they keep passing. The direct-pane fallback tests in
`test_gate.py` (`:1066-1164`) assert `condensed_count`, mounted-widget counts, and the
`fallback_banner_accounting` result — all structural, none reading the banner's text, all unchanged.

**The one property that must survive is the one-row banner.** The new sentence is longer than the
old by the total's digits, and the RA3 contract (`transcript.py:127-129`) explicitly permits the
tail of the sentence to clip at the viewport edge — it is `no_wrap` by construction (`:624`) and
pinned to height 1 by CSS (`:643-646`) and by `test_fallback_banner_paints_exactly_one_row`
(`test_transcript_blocks.py:778`). The gate's `fallback_banner_accounting` fails a banner that paints
more than one row (`gate.py:877-884`); the unchanged construction keeps that from ever becoming
reachable.

## Acceptance evidence

- **AE1.** A fallback banner reports the hidden count and the total: for the hands-on 600-row entry
  with 499 retained, the rendered banner reads `"101 of 600 lines of this entry hidden"`, and the
  words "clipped" and "at the viewport edge" do not appear. **The absence is asserted** — the old
  label is a requirement to disappear, not an omission.
- **AE2.** The live inversion is gone: a fold that trims retained rows from 499 to 494 raises the
  hidden count from 101 to 106. `test_a_partial_fold_refreshes_the_banner_count` is updated to assert
  the rising hidden count on the new wording, not the falling retained count.
- **AE3.** The banner's hidden count and the condensed marker's count are the same quantity: at a
  settled checkpoint a committed fallback entry's hidden rows are a subset of `condensed_count`, so
  the banner's number never exceeds the top marker's and the labels' scope words distinguish them.
  Asserted by a test that mounts a fallback entry over a condensed prefix and checks
  `banner_hidden <= pane.condensed_count` at a quiescent instant.
- **AE4.** The growth path's hidden count is not stale: `test_a_growing_fallback_tail_reuses_its_mounted_widgets`
  is updated to assert the post-growth total and hidden count on the new wording, exercising the
  KTD4 trap (the total must come from the new text).
- **AE5.** A descendant-triggered fallback that retains every row still renders a banner with a
  truthful `"0 of N lines of this entry hidden"` clause and the fallback cause. The silent case — a
  banner appearing without a count — is asserted not to happen.
- **AE6.** The banner still paints exactly one row at every pane width: `test_fallback_banner_paints_exactly_one_row`
  stays green unmodified, and the `test_fallback_banner_accounting_*` duck-typed gate tests stay green.
- **AE7.** The replay gate runs green over the existing gate corpus, with `content_is_complete` and
  `interface_shows_everything` both true — the condensed identity holds because `condensed_count` is
  unchanged, and the fallback banner does not mount under the gate at all, so the gate's green
  verdict is necessary but not sufficient evidence for this change. The banner's wording is proven by
  AE1-AE6.
- **AE8.** The project check is clean: `ruff`, `mypy`, `pytest`, `bandit`, `git diff --check`.

**Acceptance for a person, per the charter's evidence rule 2:** on a real gateway, the operator
re-runs the hands-on step 2b (`print exactly 600 lines`), sees the banner read `"101 of 600 lines of
this entry hidden"` beside a top marker reading `"N earlier lines condensed"` with `N >= 101`, then
lets one more turn arrive and sees both numbers rise — and can state, without scrolling, that the
entry is 600 lines long, 101 lines of its head are hidden, and the hidden head sits at the top of the
fold. That is operator-only and is not claimed on test evidence.

## Verification

```bash
uv sync --all-groups
uv run pytest tests/ui/test_transcript_blocks.py tests/ui/test_transcript_bounds.py tests/replay/test_gate.py -q
uv run ruff check .
uv run mypy
uv run pytest
uv run bandit -r talaria -q
git diff --check
```

`npm run check` is not required: nothing under `src/` is touched.

## What this unit does not do

- **It does not change which rows are hidden.** The cap, the fold arithmetic, and the retention
  policy are correct and untouched; only the banner's statement about them changes.
- **It does not change the condensed marker's number or wording.** `condensed_count`, its
  pane-wide scope, and the `"N earlier lines condensed"` sentence all stand.
- **It does not add a position display.** Showing "lines 102-600" or an entry-start locator is a
  new feature beyond B2; this unit makes the two existing numbers truthful and comparable, and the
  subset relationship (AE3) locates the entry relative to the fold without a new surface.
- **It does not touch the domain.** The change is confined to `talaria/ui/transcript.py` and the
  tests that assert the banner; ADR-0002's boundary is not approached.
