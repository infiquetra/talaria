# ADR-0006: Block rendering is bounded by work and height

Status: `proposed`
Date: 2026-08-09
Deciders: operator
Affected components: `talaria/ui/transcript.py`, `talaria/ui/blocks.py` (new), `talaria/ui/markdown.py`,
`talaria/replay/gate.py`, the domain projection's entry-scoped surface, ADR-0005 decisions 3 and 7

_Drafted by unit U1 of the
[v0.2 block-markdown plan](../../../docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md),
ahead of any widget work, because that plan's R13 requires the target to precede the
implementation. This record states the claim the later units (U2–U6) are built and gated against;
it does not itself certify that the claim holds under the built widgets — that certification is
U6's replay gate, and this ADR is `proposed` rather than `accepted` until that gate runs green
under the restated claim below._

## Context

Talaria's transcript pane currently renders every entry as flat, line-styled text — one
`TranscriptLine` widget per source line, mounted and condensed under the count-based cap ADR-0005
recorded as decision 3. That cap is sound for its own shape: it counts widgets, and a line-styled
pane mounts exactly one widget per line, so widget count and line count are the same number.

The v0.2 block-markdown plan changes that shape. Committed assistant and reasoning entries now
mount as Textual `Markdown` documents — a widget tree with headings, fenced code, lists, block
quotes, and tables, each construct its own subtree. A table is the decisive case: Textual's
`Markdown` widget mounts one widget per table cell (`_markdown.py:668`, probed against the
installed Textual 8.2.8), so a single three-line, 601-column table mounts 1,204 descendant
widgets from what would be counted as one top-level block, or three source lines, or one entry.
Neither ADR-0005 decision 3's line count nor a naive top-level-block count bounds this: a
top-level-block cap would let one adversarial table mount unbounded cell widgets while reporting
a block count of one. The restated claim in this ADR replaces "500 line widgets at every instant"
with a two-tier descendant count that a table's cells cannot escape, and adds the three companion
bounds (height, reconcile work, latency) that R13 requires stated together, not as an
afterthought once the widget work is underway.

This ADR is the R13 gate: the four measurable ceilings below, their exact measurement points, and
the fallback that relieves them when a single entry's content cannot be bounded any other way, are
recorded now, before `talaria/ui/blocks.py` or the hybrid pane exist. Unit U3 (the block factory)
is sequenced behind this ADR's review (CR1) for exactly that reason.

## Decision

**The bounded-rendering claim is four measurable ceilings — mounted descendant count, rendered
height, reconcile work, and apply latency — each with a stated measurement point, plus a
per-entry fallback that is the relief valve when an entry's content cannot be bounded by mounting
it as a block document at all.**

### (a) Mounted descendant count — two tiers

All mounted widgets count as descendants, a table's per-cell widgets included, read from
Textual's own tree (`walk_children`) rather than self-reported by any component, sampled at every
50 ms coalescing boundary (the interval Talaria already applies at `COALESCE_INTERVAL`,
`talaria/ui/app.py:167`).

- **Tier one — the folded window.** Every committed entry document and line widget except the
  newest entry and the live streaming tails stays at or under **600 descendants**. This is the
  ceiling the replay gate enforces (`gate.py:69`, unchanged number, now a descendant count rather
  than a widget-per-line count). The fold rule that keeps this tier under 600 reads the *measured*
  descendant counts of already-mounted entries — fallback banner widgets included, since a banner
  is a mounted widget like any other and the fold rule does not get to pretend otherwise — and
  estimates an *incoming* entry's descendant cost with a **construct-aware estimate**: top-level
  parser tokens plus table cells (rows × columns, read from the source's delimiter row) plus
  per-construct container overhead. The container overhead is calibrated against the installed
  widget's actual mounting and pinned by a test, because a top-level-token count alone
  undercounts: the probed 601-column table above mounts 1,204 descendants against a much smaller
  naive token count.
- **Tier two — the newest entry and each live tail, a per-entry ceiling with a two-condition
  trigger.** The entry currently being written to — the newest committed entry, and each of the
  two live streaming tails (assistant, reasoning) — falls back to line rendering when either
  condition holds:
  - its descendant count, or the construct-aware estimate above, exceeds **1,200** (the gate's
    1,000-row table workload measures 1,003 descendants, so 1,200 clears it with headroom without
    being loose enough to admit the 601-column probe, which the calibrated estimate must exceed);
  - **or** its estimated wrapped rows exceed **`mount_cap` (500)**, where estimated wrapped rows
    is `sum over source lines of max(1, ceil(cell_len(line) / content_width))` and `cell_len` is
    Textual's own terminal display-cell width measure — never Python's `len()`. A probed
    37,000-character line of double-width CJK characters paints 949 rows; `len()` estimates 475,
    undercounting by roughly half. A source-line count is not a substitute for either condition: a
    probed single 100,000-character source line mounts as **one descendant** while rendering 1,353
    wrapped rows, and a probed 10,000-line open fence is one block mounting two descendants at
    10,004 rows — neither the descendant condition nor a naive source-line condition alone sees
    either shape; the two conditions together do.

**The fallback renders non-wrapping: one widget per projected line, wrap off.** Each widget paints
exactly one row and clips at the viewport edge; a banner widget on the entry names the clip and
its cause. The full line is always present in the terminal-read buffer regardless of what the
screen clips — the screen clips, the buffer does not, and RA3 below states exactly what that
narrows on screen.

Wrapping was rejected twice over, both probed against the installed widget, not assumed: a single
wrapping fallback widget painting the 100,000-character mega-line renders 1,283 rows on its own —
the same cap violation the fallback exists to avoid — and pre-splitting into hard-wrapped
fragments turns that one line into roughly 1,283 widgets (again its own cap violation), breaks
the pane's `rendered_lines` reconstruction (fragments are not projected source lines), forces
fragment-aware condensation, and re-wraps differently on every resize. Non-wrapping keeps painted
rows, widgets, and projected lines equal to each other for the entry's own content, so none of
those four costs recur.

**Banner rows are charged to the fold arithmetic, never to a fixed headroom.** A fallen-back
entry's accounted row span is *projected lines + 1*, and the fold rule counts that accounted span
against `mount_cap` (500) — the same cap that bounds line-rendered content today. The 100-widget
margin between the 500-row cap and the 600-descendant gate ceiling pays only for **fixed-count**
chrome — the single condensation banner — never for a per-entry cost, because a per-entry cost
measured against a fixed margin is unbounded: 302 one-line fallen-back entries would leave 301 of
them in the folded tier at two widgets each, 602 widgets, whose 301 projected lines never trip a
lines-only fold rule — an aggregate-ceiling violation a lines-only count cannot see. Charging the
banner row into the accounted-row arithmetic folds that same shape at 500 accounted rows, mounting
at most 501 widgets, inside the 600-descendant ceiling. Banner rows are **chrome, not content** —
counted for the cap and the fold arithmetic, but never entering the
`condensed_count + lines-accounted-for == total` content identity the gate proves.

**The banner-preserving odd-cut rule.** Because a fallen-back entry's accounted span includes its
banner row, a fold target can land *inside* that span. The cut folds content rows only: a
partially retained fallen-back entry keeps **exactly one** banner row, its painted rows are
*retained projected lines + 1*, and only its content rows enter the condensed-line arithmetic. A
cut that would retain zero content rows rounds **forward**, folding the whole entry, banner
included. A banner never stands alone in either direction — no bannerless clipped rows, no orphan
banner.

The exact-row formula: **painted rows == projected lines + one banner row per fallen-back entry**,
or, over a partially retained span, **retained projected lines + 1**. A block-rendered newest
entry is the one exception to folding: it mounts whole, because its size is already bounded by the
two-condition trigger above, and any residual overage is recorded as a high-water figure rather
than folded away.

The observation point is deliberate, not "every instant" as ADR-0005 decision 3 stated it:
`Markdown.update` parses off the message pump and mounts in batches of 200, removing old blocks
only once the first new batch is ready, so a mid-update tree transiently holds both old and new
blocks and block counts are unknowable before the parse inside `update` completes. "At every
instant" is therefore not an enforceable claim against the widget's own mechanics. The ceilings
here are enforced at the point Talaria controls — before each apply — and verified at the 50 ms
coalescing boundaries, which is what the `TranscriptPane.apply` measurement in (d) below clocks.

### (b) Rendered height

Bounded by the same two tiers as (a): the folded window's height follows from its accounted-row
arithmetic exactly as it does today, and a single block-rendered entry's height is bounded by the
two-condition trigger — the wrapped-rows condition specifically is what bounds tall-but-few-blocks
shapes (the mega-line, the double-width line) that a descendant count alone cannot see, since
those shapes mount as very few descendants while painting many rows. The gate records the tallest
mounted entry document as a high-water figure at every boundary.

### (c) Reconcile work

Measured as **parser-input bytes per coalescing boundary** — the reparse window
`Markdown.append` actually processes, from the last unfinished top-level block to the end of the
new text. This is not the same number as bytes handed to `append`: a probe of five constant
500-byte appends into an open fence produced parser inputs of 499, 999, 1,499, 1,999, and 2,499
bytes — the reparse window grows with the unfinished block by construction, so "bytes appended"
understates the work `append` actually does. The proportionality claim is that parser input per
boundary is proportional to the unfinished block plus the new bytes, never proportional to the
whole document; for a single ever-growing unfinished block (an open fence that never closes) the
window necessarily grows with it, and the enforceable bound on that growth is the latency ceiling
in (d) — the per-entry fallback in (a) is what ends the growth once the entry trips the
two-condition trigger. High-water parser-input bytes are recorded per run.

### (d) Latency

Under the plan's adversarial workloads, p99 per-boundary apply latency stays under the 50 ms
coalescing interval. The workloads, exactly:

- a growing unclosed fence fed 100 lines per boundary up to 10,000 lines;
- a growing table fed 10 rows per boundary up to 1,000 rows;
- a single unbroken line grown 5,000 characters per boundary up to 100,000 characters (the
  wrapped-rows degenerate shape).

The clock is `time.monotonic()` wrapped around `TranscriptPane.apply`. The first 10 boundaries of
each workload are warm-up and excluded from the sample; the reported quantile is the 99th
percentile of the remaining samples. High-water instrumentation — peak descendant count per tier,
peak parser-input bytes, peak apply milliseconds, tallest entry document — is exposed the way
`peak_mounted` is exposed today (`transcript.py:130`).

### The KTD8 fallback trigger and its branch-hold mechanism

Separately from the per-entry fallback in (a), there is a whole-feature fallback: reverting to the
existing styled-line-run renderer (`talaria/ui/markdown.py`, today's per-line inline styler)
instead of the block-document widget family. That fallback is **gate-triggered only** — it is
invoked exclusively on a red restated gate (U6) with the measured evidence attached, and choosing
it is itself a recorded requirements amendment to R4 in `docs/engineering-journal/DECISIONS.md`,
never a silent swap made mid-implementation. No fallback code is written ahead of that evidence;
`talaria/ui/markdown.py` is retained in the tree as the styling half this fallback would need, but
it is retired from the live rendering path by unit U4 regardless of which way the gate lands.

The branch-hold mechanism is what makes that decision point reachable without leaving `main` in an
unknown state: units U4, U5, and U6 land on one feature branch, and that branch merges to `main`
only after U6's replay gate runs green under the claim this ADR states and CR6 (the cross-review
gate on U6) passes. A red gate therefore leaves `main` untouched, and the fallback decision —
amend R4 and take the styled-line-run fallback, or keep iterating on the branch against the
measured evidence — is an explicit operator gate with numbers attached, not an implementation
default reached by drift. Units U1 through U3 (this ADR, the domain changes, and the block
factory) merge independently of that branch and are useful regardless of which way the gate
eventually lands.

### Amendments to ADR-0005

ADR-0005's decision 3 and decision 7 are amended by this ADR. Nothing else in ADR-0005 changes.

**Decision 3 (superseded).** ADR-0005 stated: "The transcript mounts at most 500 line widgets plus
one condensed block, at every instant and not merely once an update settles." That single-tier,
line-widget, every-instant claim is superseded by the two-tier descendant model in this ADR's
clause (a). Restated in this ADR's terms: **500 remains `DEFAULT_MOUNT_CAP`** — it is now the line
cap for line-rendered surfaces (ordinary line-kind entries and any block entry that has fallen
back to line rendering) and the threshold the wrapped-rows condition of the per-entry trigger
compares against — while **600 is the folded-window descendant ceiling** the replay gate enforces,
counted over the entry's actual mounted widget tree rather than assumed to be one widget per line.
The "at every instant" enforcement claim is also superseded: enforcement is at the coalescing
boundary Talaria controls (before each apply), for the reason clause (a) states —
`Markdown.update`'s own batched mounting makes a stronger claim unverifiable against the widget's
mechanics.

**Decision 7 (amended, not broken).** ADR-0005 stated: "Untrusted text reaches the screen only
through `talaria.ui.literal.literal_text`, which bypasses Rich's markup parser and replaces
obeyable control characters with visible stand-ins." That boundary gains a second lawful channel
under this ADR's KTD4: untrusted text may additionally reach the screen through the
KTD4-configured parser (`MarkdownIt("gfm-like")`, reconfigured with `html=False`, `linkify=False`,
and a pinned exact enabled rule set) and its rendering hooks (a document subclass that renders
links styled but with no action metadata installed, so `Markdown.LinkClicked` never fires, and
renders images as `alt (target)` text with nothing fetched) — **and nothing else**. Defang
(`talaria/ui/literal.py`) still runs before parse, at parity with today's boundary, so C0 control
characters, bidi overrides, and zero-width characters never reach the parser either channel. The
amendment widens the set of lawful renderers from one to two; it does not weaken what either
renderer is permitted to do with untrusted bytes.

### Requirements amendments (RA1, RA2, RA3)

Three findings from the 2026-08-09 plan review could not be resolved inside the original
requirement text and are recorded here as explicit, operator-vetoable amendments, mirrored in
`docs/engineering-journal/DECISIONS.md`.

**RA1 — Underscore emphasis and strikethrough enter scope.** This amends the v0.1 inline-markdown
decision's deferral and R9's grammar allowlist. The deferral existed because the v0.1 per-line
styler would have needed a new regex for underscore emphasis; with a real parser the construct
comes free, and turning it off would require disabling markdown-it's `emphasis` rule wholesale,
which also disables asterisk emphasis — the rule does not distinguish by source character.
Strikethrough (`~~text~~`) is a GitHub-Flavoured Markdown staple in the same position. Both now
render and both join the R9 allowlist; U3's allowlist pin test captures the exact enabled parser
rule set so any future preset drift is a loud test failure, not a silent scope change.

**RA2 — R3's table reachability is met by cell wrapping, not cell focus.** R3 exists so table
content is usable without a mouse; the intent survives, the letter changes. The stock `Markdown`
widget fails R3 as written via ellipsis-plus-hover-tooltip cells. Making cells focusable would
turn the transcript into a data-grid widget — new bindings, a caret model inside entries, and a
collision with the answerability focus order from the v0.2 answerability plan. Instead, the block
factory's table subclass styles cells to wrap, with an explicit column-allocation rule, because
wrapping alone does not survive Textual's `grid-columns: auto` layout — a live five-column probe
at 80 columns assigned data widths `[0, 0, 58, 1, 1]`, starving four of five columns entirely. The
rule: for a table of N columns in the table's actual inner content width W (measured from the
table content box, since the `Markdown` document pads two columns on each side — an 80-column pane
probes to 76 content columns), available content width `A = W − (3N + 1)`; per-column cap
`CAP = max(8, floor(A / N))`; per-column minimum `m_i = min(longest_word_i, CAP)`. If
`sum(m_i) ≤ A`, every column gets `m_i` and the remainder is distributed proportionally to content
length by the largest-remainder method, ties to the leftmost column — including the all-empty-cell
case, where remainder is split equally with the same leftmost tie rule. If `sum(m_i) > A`, every
column gets the equal split `floor(A / N)` with a hard floor of 3 cells, and an overlong word
character-wraps. The amended R3 reads: "table content is fully legible at 80 columns without a
mouse; no construct depends on hover or focus to show its content." AE4's assertion is written
against this amended text and asserts actual painted cell content, not just applied style.

**RA3 — On fallen-back entries only, on-screen visibility narrows to the clipped row plus a
banner.** This amends R11's projection-to-screen visibility requirement and R3's legibility
requirement, scoped exclusively to entries that have tripped the per-entry fallback in clause (a)
above. R11 requires every projected source region represented visibly; a clipped row of a
100,000-character line genuinely does not show its tail on screen, and no presentation of that
shape does. The amendment: for a fallen-back entry, projection-to-screen visibility is satisfied
by one painted row per projected line **plus a banner that names the clip and the fallback
cause**. The replay gate proves the banner is present and the row count is exact — it does not
claim clipped-cell reachability. The full content stays byte-exact in the terminal-read buffer and
in any recording, regardless of what the screen clips. A keyboard expand or inspect affordance for
fallen-back entries is queued follow-up work, out of this plan's scope — building per-entry
horizontal navigation for adversarial content is the same data-grid machinery RA2 declines above.
The veto path, if this narrowing is unacceptable: demand the expand affordance now, which grows a
new interaction model and its focus-order integration with the answerability spine as a
consequence.

## Rejected alternatives

**A single-tier descendant ceiling with no per-entry fallback.** Rejected because it is
incompatible with the gate's own 1,000-row table workload: once the newest, block-rendered entry
mounts whole (which it must, to render progressively as it streams), a single number cannot
distinguish "the folded window is too large" from "the one entry currently being written is too
large" — they need different remedies (fold more of the window versus fall this one entry back to
line rendering). Two tiers with an entry-level fallback is the minimum shape that answers both.

**A top-level-block or source-line count as the mounting metric.** Rejected on three separate
probed counter-examples: the 601-column table (top-level blocks undercount because cells are the
real cost), the 100,000-character mega-line (one source line, one descendant, but 1,353 rendered
rows — a source-line count sees neither the height nor the row-painting cost), and the
10,000-line open fence (one block, two descendants, 10,004 rows — the same blind spot). No single
one of {descendant count, source-line count, character count} covers all three; the construct-aware
estimate plus the display-cell-width wrapped-rows estimate is the minimum pair that does.

**A wrapping fallback, and a pre-split hard-wrapped-fragment fallback.** Both probed and rejected;
see clause (a) above for the two measured failure modes (1,283 rows from one wrapping widget;
~1,283 widgets and a broken `rendered_lines` reconstruction from pre-split fragments).

**Charging fallback banner rows to a fixed headroom instead of the fold arithmetic.** Rejected
because a fixed margin against a per-entry, unbounded-count cost is itself unbounded — the
302-fallen-back-entry probe in clause (a) demonstrates the exact shape that breaks a fixed-margin
rule while the accounted-row charge holds it inside the ceiling.

**Reverting to the styled-line-run renderer now, ahead of any gate evidence.** Rejected as
premature: KTD8's whole-feature fallback exists precisely because the block-widget family's
coexistence with the pane's mechanics at acceptable cost is an assumption, not yet a measurement.
Taking the fallback before U6's gate runs would spend the fallback on a guess instead of evidence,
and would make R4's amendment a default rather than a decision.

## Consequences

**Easier.** A stated, numbered claim gives U2 through U6 a target to build toward and the replay
gate something falsifiable to check, rather than a felt sense that "the pane should stay bounded."
The construct-aware estimate and the display-cell wrapped-rows estimate are each independently
testable against a fixed probe corpus (the 601-column table, the 37,000-character CJK line, the
100,000-character mega-line, the 10,000-line fence), so a future Textual upgrade that changes
per-cell mounting cost fails a pinned test rather than silently drifting the ceiling.

**Harder.** The two-tier model plus the per-entry fallback plus the banner-charging rule is
materially more bookkeeping than a single line-widget cap. The fold rule now has to estimate an
incoming entry's cost before mounting it, hold that estimate to a calibration pinned against the
installed widget, and reconcile accounted rows (which include banner chrome) against the line
count identity the gate proves (which must not include banner chrome) — two adjacent but distinct
arithmetics that a careless change could conflate.

**Deferred, deliberately.** The keyboard expand/inspect affordance for fallen-back entries (RA3)
is named and vetoable but not built. If the fallback trigger (a) or the whole-feature fallback
(KTD8) fires in practice more often than the probe corpus suggests, that affordance moves from
queued follow-up to the next unit of work.

## Revisit when

- U6's replay gate runs against the claim stated here. A green run is what moves this ADR's status
  from `proposed` to `accepted`; a red run is the KTD8 decision point — amend R4 and take the
  styled-line-run fallback, or keep iterating on the branch against the recorded evidence. Either
  outcome updates this ADR's status field, not its numbers, unless the numbers themselves are what
  the evidence contradicts.
- Textual is upgraded past 8.2.8. The per-cell mounting cost, the batched-mount behaviour of
  `Markdown.update`, and the `append`/`get_stream` semantics this ADR and KTD3 depend on are
  implementation details of a specific version; U3's pin tests are what convert a semantic change
  on upgrade into a loud failure rather than a silently stale ceiling.
- The expand/inspect affordance deferred by RA3 is taken up. That is a scope decision the operator
  makes explicitly, not a consequence of this ADR's numbers changing.
