---
title: Talaria v0.2 — block-level markdown and transcript differentiation plan
type: feat
status: active
date: 2026-08-09
origin: docs/brainstorms/2026-08-08-block-markdown-and-transcript-differentiation-requirements.md
reviewed: docs/reviews/2026-08-09-talaria-v0-2-block-markdown-plan-review.md
revision: 2
---

# Talaria v0.2 — block-level markdown and transcript differentiation plan

## Summary

Render assistant and reasoning transcript entries as full block-level markdown — headings, fenced
code, lists, tables, block quotes — with Textual's built-in `Markdown` widget family, streaming
progressively, while every transcript kind group gains a distinct visual channel. The plan restates
the pane's bounded-rendering claim in an architecture decision record **before** any widget work,
splits the domain changes from the presentation changes along ADR-0002's boundary, and closes by
re-running the replay gate over a new feature corpus under the restated claim.

Revision 2 resolves the 2026-08-09 doc-review's findings (three panel reviewers plus a Codex pass;
the review artifact in the frontmatter carries the full ledger). The review's live probes against
the installed Textual 8.2.8 reshaped five decisions: the bounded-rendering ceilings now count
descendants rather than top-level blocks, the streaming rationale cites the public `get_stream`
factory it previously missed, the forgery boundary now covers Textual's click actions and image
rendering (not only the parser), the provisional tail is two documents (assistant and reasoning
can stream in the same turn), and the gate grew a sideband timeline because confirmed-cancel and
disconnect are not wire frames and cannot replay from the frame log alone.

## Problem Frame

The transcript is constant white text; the operator's own words from live driving: "It's difficult
to read and differentiate what I am looking at." The inline-markdown decision
(docs/engineering-journal/DECISIONS.md, "Inline markdown is rendered on agent prose") recorded
"block-level rendering is taken up" as its revisit condition — this plan is that revisit. Today's
rendering is also wrong, not merely flat: the per-line styler has no fence awareness
(talaria/ui/markdown.py:82 matches code spans line-locally), so backticks inside a fence body are
styled as inline code and fence delimiter lines render literal.

The upstream requirements doc (the `origin:` above) is authoritative for the WHAT. Its R-IDs
(R1–R18) are carried forward here unchanged except where the Requirements Amendments section below
records an explicit amendment. The four questions the brainstorm deferred to planning are resolved
as KTD2, KTD5, KTD1's instrumentation, and KTD6 respectively.

## Grounding (what the code says today)

- `TranscriptPane` (talaria/ui/transcript.py:90) is line-indexed with four dependent mechanisms:
  the stable-prefix diff (`_common_prefix`, :270), `DEFAULT_MOUNT_CAP = 500` (:52) with
  `mounted_count` read from `len(self.children)` (:149), `_top`/`condensed_count` as an absolute
  line position (:126, :161), and `_restore_anchor` subtracting evicted widget heights (:302).
- `interface_shows_everything` (talaria/replay/gate.py:226) compares `pane.rendered_lines` against
  the projection window position-by-position and requires mounted + condensed == total; the
  mounted-widget ceiling is 600 (gate.py:69). `content_is_complete` (gate.py:290) is the
  domain-side line-indexed proof and stays untouched (R11a).
- `transcript_view` (talaria/domain/projection.py:261) flattens entries to lines, welds
  presentation prefixes (`· ` for reasoning, the `_ENTRY_PREFIX` map at projection.py:305), and
  appends in-flight streaming text as a provisional `assistant` tail — **the reasoning buffer is
  not projected in-flight at all** (projection.py:276 reads only `state.streaming_text`), which is
  R18's headline gap; `committed_lines` marks the boundary.
- `_on_error` (talaria/domain/state.py:1586) clears `streaming_text`/`reasoning_text` without
  committing them — the R6 defect, a domain transition change. The confirmed-cancel path does
  **not** share the commit defect: `cancel_turn` (state.py:567-598) already commits both buffers
  before clearing — that commit behavior is pinned by regression — but it strips them on the way
  (`.strip()` at :580, `.lstrip()` at :583), so it receives exactly one change: the preservation
  work removes its stripping.
- Content-channel text is stripped on its way into the domain: `coerce_text`
  (talaria/domain/normalize.py:224) strips both ends, `message.interim` uses it (state.py:1435),
  final bodies are left-stripped (state.py:1510), and `cancel_turn` strips both buffers
  (state.py:579-583). Stripping is markdown-hostile — leading indentation is a code block and
  trailing blank lines close constructs — so U2 changes the content channel to exact preservation.
- The transport reports disconnection through one callback shape from four distinct causes:
  credential failure (talaria/transport/source.py:383), failed dial (:404), orderly close (:445),
  and exhausted reconnect (:708). The domain's `set_connection` (state.py:560) records a status
  and nothing else — terminal-versus-transient is not decidable from what the domain sees today,
  which is why KTD7 adds a typed cause.
- The installed Textual is 8.2.8 (.venv, version-pinned in uv.lock). `Markdown` is exported;
  `MarkdownStream` (_markdown.py:41) is not exported, but the **public** classmethod
  `Markdown.get_stream` (:1155) returns one — its documented purpose is coalescing when append
  rates approach 20 per second. `Markdown.append` (:1445) is public and reparses from the last
  unfinished top-level block; `Markdown.update` (:1376) is the full-document replace path, parsing
  off the message pump and mounting in batches of 200 with the old blocks removed only when the
  first new batch is ready — so a mid-update tree transiently holds old and new blocks, and block
  counts are unknowable before the parse inside `update` runs. Blocks carry `source_range` line
  spans (:210, :1483) — a parser line span, not a proof of rendered content: a probe of
  `"a\n\n# h\n"` yields ranges (0,1) and (2,3) with the blank line owned by neither, and an empty
  document yields no blocks at all. The widget is a vertical container of `MarkdownBlock` children
  (:962), and a table mounts **one widget per cell** (:668) — a top-level block count therefore
  bounds neither descendants nor height. `open_links` defaults True (:1035) and only gates
  `app.open_url` in `on_markdown_link_clicked` (:1194); the `@click` action metadata is installed
  unconditionally for links (:342), so `Markdown.LinkClicked` is emitted regardless — probed live.
  Images render alt text and silently drop the target (:346). Table cells are non-focusable
  `Static` widgets with ellipsis and hover tooltips (:629, :668) — R3's keyboard reachability
  fails in the stock widget today. `MarkdownIt("gfm-like")` defaults `html=True`, `linkify=True`;
  with `html=False` a `<script>` tag parses to a visible inline text token (verified live), and
  the preset also enables horizontal rules, underscore emphasis, strikethrough, autolinks, and
  images — see the Requirements Amendments section for which of those are in scope.

## Requirements Amendments

Two review findings could not be fixed inside the original requirement text; each is an explicit,
operator-vetoable amendment recorded here and mirrored in DECISIONS.md by U1.

**RA1 — Underscore emphasis and strikethrough enter scope (amends the inline decision's deferral
and R9's grammar list).** The brainstorm deferred underscore emphasis because the v0.1 inline
styler would have needed a new regex; with a real parser the construct comes free, and disabling
it would require switching off markdown-it's `emphasis` rule, which kills asterisk emphasis too —
the rule does not distinguish source characters. Strikethrough (`~~`) is a GitHub-Flavoured
Markdown staple in the same position. Both render; both join the R9 allowlist; the allowlist pin
test (U3) captures the exact enabled rule set so any future preset drift fails loudly.

**RA2 — R3's table reachability is met by wrapping, not cell focus (amends R3's letter, keeps its
intent).** R3 exists so table content is usable without a mouse. The stock widget fails it via
ellipsis-plus-hover-tooltip cells. Making cells focusable would turn the transcript into a data
grid — new bindings, a caret model inside entries, and a collision with U8's answerability focus
order. Instead: the block factory styles table cells to wrap (no ellipsis, no tooltip dependence)
**with a column-allocation rule**, because wrapping alone does not survive Textual's
`grid-columns: auto` layout — a live five-column probe at 80 columns assigned data widths
[0, 0, 58, 1, 1], starving four columns entirely. The rule, exactly — for a table of N columns in
the table's **actual inner content width W** — not the pane width: the `Markdown` document adds
two columns of padding each side (probed: an 80-column pane gives `MarkdownTableContent` 76
columns), so W is measured from the table content box (or the padding is explicitly removed and
then W equals the pane content width; the factory picks one and the test pins it). Proportional
remainders are distributed by the **largest-remainder method, ties to the leftmost column** —
deterministic integer arithmetic, no float residue. Available content width `A = W − (3N + 1)`
(one cell-padding
column each side of every cell plus one gutter per boundary); per-column cap
`CAP = max(8, floor(A / N))`; each column's minimum `m_i = min(longest_word_i, CAP)`. If
`sum(m_i) ≤ A`, every column gets its `m_i` and the remainder is distributed proportionally to
each column's total content length. If `sum(m_i) > A`, every column gets the equal split
`floor(A / N)` with a hard floor of 3 cells; a word longer than its column character-wraps. The
factory's table subclass computes these widths and sets them explicitly rather than trusting the
auto layout. Every cell's full content is then readable at 80 columns by ordinary transcript
scrolling — no hover, no mouse, no per-cell focus — and the test asserts the **actual painted
content** of every cell (the five-column probe case included), not just the style.
The amended R3 reads: "table content is fully legible at 80 columns without a mouse; no construct
depends on hover or focus to show its content." AE4's assertion follows the amended text.

## Key Technical Decisions

**KTD1 — The bounded-rendering claim is restated as four measurable ceilings, recorded in
ADR-0006 before implementation:**

- **(a) Mounted descendant count — two tiers.** All mounted widgets are counted as descendants (a
  table's per-cell widgets included), read from Textual's own tree (`walk_children`), never
  self-reported, at every 50 ms coalescing boundary. Tier one: the **folded window** — every
  committed entry document and line widget except the newest entry and the live tails — stays
  ≤ 600 descendants, enforced by KTD2's fold rule (fold decisions read the measured descendant
  counts of already-mounted entries; an incoming entry's **construct-aware estimate** counts
  top-level tokens **plus table cells** — rows × columns read from the source's delimiter row —
  because a probed three-line, 601-column table mounts 1,204 descendants, so a top-level token
  count alone is not conservative). Tier two: the newest entry and each live tail carry a
  **per-entry ceiling with a two-condition trigger** — the entry falls back to line rendering
  when its descendant count (or construct-aware estimate) exceeds **1,200** (the gate's 1,000-row
  table workload measures 1,003 descendants, fitting with headroom) **or** when its **estimated
  wrapped rows** exceed **`mount_cap` (500)**. Estimated wrapped rows are width-aware by
  construction: `sum over source lines of max(1, ceil(len(line) / content_width))` — a source-line
  count is not the metric, because a probed single 100,000-character source line mounts as **one
  descendant** yet renders 1,353 wrapped rows, and neither a descendant nor a source-line
  condition would fire. The estimate covers both probed degenerate shapes: the 10,000-line open
  fence (one block, two descendants, 10,004 rows) and the mega-line. A line-rendered entry is
  **not** exempt from folding: it is
  ordinary line content under today's cap machinery, `desired_top` may land inside it (exactly as
  it does today for a 4,000-line tool dump), and the banner accounts its folded lines — so the
  fallback can never itself mount more than the line cap allows. The entry-level fallback both
  restores the count bound and ends the growing-reparse work of clause (c). The boundary
  observation point is deliberate:
  `Markdown.update` parses off the pump and mounts in batches of 200, transiently holding old and
  new blocks, so "at every instant" is not enforceable against the widget's own mechanics; the
  ceilings are enforced at the point talaria controls — before each apply — and verified at
  boundaries.
- **(b) Rendered height.** Bounded by the same two tiers: the folded window's height follows from
  its line arithmetic as today, and a single block-rendered entry's height is bounded by the
  two-condition trigger of (a) — the width-aware wrapped-rows condition is what bounds the tall-but-few-blocks
  shapes the descendant count cannot see. The gate measures the tallest mounted entry document and
  records it as a high-water figure.
- **(c) Reconcile work.** Per-boundary parse work is measured as **parser-input bytes** — the
  reparse window `Markdown.append` actually processes, from the last unfinished top-level block to
  the end (probed fact: five constant 500-byte appends into an open fence produce parser inputs of
  499, 999, 1,499, 1,999, 2,499 bytes — the window grows with the unfinished block, which is
  inherent to append's reparse semantics, so "bytes handed to append" understates work and is not
  the metric). The proportionality claim: parser input per boundary is proportional to the
  unfinished block plus new bytes, never O(document); for a single ever-growing unfinished block
  the window grows with it by construction — the latency ceiling (d) is the enforceable bound
  there, and the per-entry fallback of (a) is the relief valve that ends the growth. Recorded
  high-water.
- **(d) Latency.** Under the R14 adversarial workloads, p99 per-boundary apply latency stays under
  the 50 ms coalescing interval. The workloads, exactly: a growing unclosed fence fed 100 lines
  per boundary to 10,000 lines; a growing table fed 10 rows per boundary to 1,000 rows; a single
  unbroken line grown 5,000 characters per boundary to 100,000 characters (the wrapped-rows
  degenerate shape). Clock:
  `time.monotonic()` around `TranscriptPane.apply`. The first 10 boundaries are warm-up and
  excluded; the quantile is the 99th percentile of the remaining samples. High-water
  instrumentation (peak descendant count per tier, peak parser-input bytes, peak apply
  milliseconds, tallest entry document) is exposed the way `peak_mounted` is today
  (transcript.py:130).

Rationale: R13 requires the target to precede the implementation, and requires work **and height**
bounded, not only count; a top-level block count alone lets one table mount unbounded cell widgets,
which is why the count ceiling is descendants — and a single-tier descendant ceiling is
incompatible with the gate's own 1,000-row workload once the newest entry mounts whole, which is
why there are two tiers and an entry-level fallback rather than one number.

**KTD2 — The pane goes hybrid: block documents for committed assistant/reasoning entries, line
widgets for everything else, two live tail documents for the streams — with a defined fold rule.**
Each committed assistant or reasoning entry mounts one `Markdown` document (entry = parser document
is exactly R15's isolation boundary); all other kinds keep one `TranscriptLine` per line; the
in-flight assistant and reasoning streams render in **two independently keyed live `Markdown` tail
documents** — the domain holds both buffers simultaneously (state.py:106) and neither may steal
the other's progressive rendering (R18). On commit, each tail's final source becomes its committed
entry's document without a full-pane rebuild, keyed by the entry identity KTD6 publishes.

Two mounting rules precede condensation. A committed assistant/reasoning entry (or a tail state)
whose parse yields **zero blocks** — empty, whitespace-only, newline-only — line-renders: a
zero-block document mounts at height zero (probed), which would silently drop the blank rows a
line widget shows today. And an entry that trips KTD1(a)'s two-condition trigger (descendants or
construct-aware estimate > 1,200, or estimated wrapped rows > `mount_cap`) line-renders with a
banner note.

Condensation over mixed units, exactly: `desired_top` is computed in projected lines as today
(transcript.py:223); when it lands inside a **block** entry's line span, it rounds **up** to the
next unit boundary — the cap prefers evicting more over keeping a cap-buster — with one
exception: a block-rendered newest entry is mounted whole (its size is already bounded by the
two-condition trigger, and the residual overage is recorded in the high-water instrumentation).
A **line-rendered** entry — including a fallen-back newest entry — is ordinary line content:
`desired_top` may land inside it and fold its head, exactly as today. The ceiling claim in
KTD1(a) is stated over these rules: folded window ≤ 600 descendants, plus at most one
block-rendered newest entry and the tails, each bounded by the two-condition trigger. The banner
keeps counting **lines**; the entry line spans KTD6 publishes are what keep
`condensed_count + lines-accounted-for == total` computable over folded block units.

Rationale: this confines variable-height widgets to entry-scoped regions, keeps the stable-prefix
diff meaningful for line-rendered kinds, and gives the gate a per-entry region to prove ownership
over (R11b).

**KTD3 — Streaming drives public `Markdown.append` at the 50 ms boundary, with the fragment and
the replace signal both derived from the domain, not guessed; replacement drives
`Markdown.update`.** The corrected API grounding: Textual 8.2.8 does expose a public streaming
surface — `Markdown.get_stream` (:1155) — whose documented purpose is coalescing bursts above
~20 appends per second. Talaria already coalesces at a 50 ms boundary
(`COALESCE_INTERVAL`, talaria/ui/app.py:167) and serializes applies on the pump, so the factory's
coalescing duplicates what the app provides; direct `append` at the boundary is chosen on that
comparison, not because no public API exists.

The pane receives snapshots (`TranscriptView`), not deltas, so U2 publishes what append needs:
each provisional tail carries a **stream generation counter** that increments on any replacement
(`message.interim`, any replace-wins path) and the tail's raw text. Same generation → the pane
appends the suffix beyond what it last applied; changed generation → `update()` with the full
authoritative text. Append-only cannot express replacement, and prefix-guessing is exactly the
invented discriminator this decision exists to forbid. A pin test asserts the observed
append/update semantics against Textual 8.2.8 and fails loudly on upgrade drift.

**KTD4 — The forgery boundary is the parser configuration plus the rendering hooks, and each
forbidden channel gets a proving test.** The block factory builds
`Markdown(parser_factory=…, open_links=False)` where the factory returns `MarkdownIt("gfm-like")`
reconfigured with `html=False` and `linkify=False` — and pins the **exact enabled rule set** (the
allowlist upper bound: a snapshot test of the parser's active rules, so a preset drift or an
accidentally enabled rule fails the pin). Defang runs before parse at parity with today's boundary
(talaria/ui/literal.py), so C0 controls, bidi overrides, and zero-width characters never reach the
parser.

Parser configuration alone is not the whole boundary — Textual installs `@click` action metadata
for links unconditionally and emits `Markdown.LinkClicked` regardless of `open_links` (:342,
:1194), and renders images as alt text with the target silently dropped (:346). So the factory's
document subclass renders links **styled but carrying no action metadata** (nothing is emitted,
nothing is opened — a test asserts no `LinkClicked` message fires on click) and renders images as
`alt (target)` text per R10 ("their alt text and target render as text"). HTML literalizes to
visible text (never disappears — R10), bare URLs stay text, Rich console markup is never parsed
over gateway bytes (the widget renders from tokens, not markup). ADR-0005's `literal_text`-only
rule is amended by ADR-0006, not silently broken.

**KTD5 — Kind differentiation is per-widget CSS classes by kind group plus theme variables from
the existing theme's vocabulary, with the channel carried by background tint and a gutter marker,
not text colour.** The twelve `TranscriptKind` members map to the five groups R7 fixes; both
`TranscriptLine` and entry documents carry a `transcript--group-*` class; assertions read computed
styles (AE5), and a fence's syntax colours cannot erase a background/gutter channel (R8). Styling
changes no widget's height, measured against the same screen with styling disabled. Kind styling
uses the existing theme's variable vocabulary — no new theme variables are introduced (the
brainstorm's constraint, carried forward verbatim). Rationale: classes compose with the widget
family's own styles where text-colour overrides would fight the highlighter.

**KTD6 — The projection grows an entry-scoped surface carrying entry identity; the flattened line
buffer keeps its decorated lines exactly as today.** `TranscriptView` adds per-entry records —
**a stable entry id (monotonic per session, stable under condensation)**, kind, raw body,
committed flag, line span — plus the two provisional tails (raw text and stream generation each).
The reasoning `·` weld becomes decoration applied only to line-rendered surfaces (R18: a
first-line heading in a reasoning stream must be valid markdown). `terminal_read` (KTD10 of v0.1)
and `content_is_complete` (R11a) read the unchanged decorated line buffer, so the agent-facing
read and the domain proof move zero bytes. The entry id is what the tail-to-committed handoff
(KTD2) keys on. Rationale: two consumers, two surfaces — restating the line buffer would break the
v0.1 pin `tests/domain/test_projection.py::test_every_transcript_entry_survives_into_the_line_buffer`,
which must keep passing verbatim.

**KTD7 — Terminal paths commit partial buffers in the domain, with the terminal-versus-transient
distinction made decidable by a typed cause.** Today the transport reports four distinct causes
through one callback shape (source.py:383, :404, :445, :708) and the domain records only a status
— the distinction the requirement needs does not exist in the events. So: the transport layer
gains a typed end-of-stream cause seam, and the domain gains a transition that consumes it.
Terminal causes — `auth_failed`, `dial_failed`, `orderly_close`, `reconnect_exhausted`, operator
teardown — commit `streaming_text` and `reasoning_text` as entries before clearing (R6), as does
`_on_error` (state.py:1586). A transient reconnect that resumes the same response commits nothing
and must not duplicate; the existing segment/interim machinery is the dedupe backstop, and a
scenario pins it. `cancel_turn` already commits both buffers (state.py:580-585) — the commit
behavior is pinned by regression, and its one change is that the preservation work removes its
stripping of those buffers. Content-channel strings are preserved **exactly** end-to-end
(no stripping — leading indentation is a code block; trailing blank lines close constructs);
`coerce_text`'s stripping remains for diagnostic channels only. The typed cause is wired
end-to-end: the transport's cause seam reaches the domain through the same live path that today
connects `LiveSource.bind` to `note_connection_state` (talaria/ui/app.py:1455-1467, wired at
talaria/cli.py:453), so U2 owns those two wiring files and an end-to-end live-source test — the
domain transition must have a specified live call path, not just a seam. This is a
domain-plus-transport-plus-wiring change under ADR-0002's direction rule — the domain still
imports no framework.

**KTD8 — The styled-line-run fallback is gate-triggered, amends R4 explicitly, and the work is
branch-held until the gate is green.** The fallback (projection-tagged line runs styled by the
existing pane) is invoked only on a red restated gate with the measured evidence attached, and
taking it is a recorded requirements amendment in DECISIONS.md — never a silent swap. Until that
evidence exists, no fallback code is written. The reachability mechanism: U4, U5, and U6 land on
one feature branch that merges to `main` only after U6's gate runs green and CR6 passes — so a red
gate leaves `main` untouched and the fallback decision point (operator gate, numbers attached)
chooses between amending R4 and iterating on the branch. U1–U3 merge independently; they are
useful under either outcome.

## Requirement traceability

Every R-ID and acceptance example, and the unit(s) that discharge it. The spec's unit prompts
carry the same lists; a unit naming an R-ID here covers its substance in scope and scenarios.

| Requirement | Discharged by |
| --- | --- |
| R1 all five constructs render | U4 (render + per-construct oracles), U6 (corpus + gate oracles) |
| R2 bounded fence, no inline code-span inside | U4 |
| R3 tables legible at 80 columns without a mouse (as amended by RA2) | U4 |
| R4 built-in widget family, glue only | U3 (factory is configuration + subclassed rendering hooks, no parser/assembly engine), U4; CR3/CR4 verify |
| R5 progressive structure | U4, U6 |
| R6 early-ending turn commits partials | U2, U6 |
| R7 twelve kinds to five groups | U5 |
| R8 kind styling composes, no height change | U5 |
| R9 forgery allowlist (as amended by RA1) | U3 |
| R10 HTML literalized; links inert; images not fetched, alt + target as text | U3 |
| R11a `content_is_complete` intact, not restated | U6 (explicit guard: the function and its callers are untouched; the v0.1 pin passes verbatim) |
| R11b projection-to-screen region proof | U6 |
| R12 replay determinism, normalized | U6 |
| R13 ADR precedes implementation | U1 (record), spec graph (U3 gated on CR1) |
| R14 adversarial corpus incl. open-fence and mid-table termination | U2 (domain legs), U6 (gate legs) |
| R15 parser isolation per entry | U3 |
| R16 replacement semantics, all three clauses | U2 (domain boundary), U4 (render-exactly-once, terminal-path awaits writes, stale stream vs removed widget) |
| R17 scroll anchoring | U4 |
| R18 provisional reasoning projected and rendered progressively | U2 (projection), U4 (rendering), U6 (overlap corpus case) |

AE1→U4, AE2→U2+U6, AE3→U3, AE4→U4 (amended per RA2), AE5→U5, AE6→U6, AE7→U4.

## Dependency order

The spec graph in the companion execution spec is authoritative for execution order. The shape:
U1 (the ADR) starts alone; U2 (domain) starts in parallel with U1 — the stated exception to
ADR-first, safe because U2 is domain-only and touches no widget; U3 (the block factory) waits for
CR1 (the ADR review) because `talaria/ui/blocks.py` is widget work and R13 places the record
before it; U4 (the hybrid pane) needs CR1, CR2, and CR3; U5 (kind styling) edits the same files as
U4 and follows CR4; U6 closes after CR5. Each build unit is cross-reviewed by the Codex engine
before its dependents run, and **a review that returns findings blocks its dependents until the
findings are fixed and re-reviewed — dependents consume the verdict, not the completion**. CR6
gates the outcome leaf's completion event. U4–U6 land on the KTD8 feature branch; U1–U3 merge
independently. The executable workflow file is **re-emitted from the revised spec at dispatch
time** — the previously committed revision-1 emission was deleted with this revision so a stale
graph cannot be launched by habit.

## Implementation Units

### U1. ADR-0006 — the block-aware bounded-rendering claim

**Goal:** Record the restated claim, its instrumentation, and its ceilings before any
implementation lands on it (R13; KTD1, KTD8).

**Files:** platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md
(new), docs/engineering-journal/DECISIONS.md (KTD mirror, RA1/RA2 amendments).

**Scope:** The four ceilings of KTD1 with their exact measurement points (the boundary-observed
descendant count and why "every instant" is not the enforcement point, the height-via-fold-rule
argument, the work metric, the latency workloads/clock/warm-up/quantile); the amendments to
ADR-0005 — **both** decision 3 (the 500-line-widget cap "at every instant", superseded by the
two-tier model; state that 500 stays `DEFAULT_MOUNT_CAP` — the line cap for line-rendered
surfaces and the wrapped-rows trigger threshold — while 600 is the folded-window **descendant**
ceiling the gate enforces) and decision 7 (the `literal_text`-only rule — untrusted text may
additionally reach the screen through the KTD4-configured parser and rendering hooks and nothing
else); the RA1 and RA2 requirements amendments; the fallback trigger (KTD8), its branch-hold
mechanism, and what evidence invokes it.

**Test expectation:** none — architecture record; U6 makes every clause executable.

### U2. Domain: terminal-path commits, exact content preservation, and the entry-scoped projection

**Goal:** Commit partial buffers on terminal paths under a typed cause, preserve content-channel
text exactly, and publish entry identity, raw bodies, and both provisional tails beside the
decorated line buffer (R6, R14, R16, R18; KTD6, KTD7).

**Files:** talaria/domain/state.py, talaria/domain/projection.py, talaria/domain/normalize.py,
talaria/transport/source.py, talaria/ui/app.py (the `note_connection_state` wiring only),
talaria/cli.py (the `LiveSource.bind` wiring only), tests/domain/test_transcript_state.py,
tests/domain/test_turn_lifecycle.py, tests/domain/test_projection.py,
tests/transport/test_source.py. (U4 also edits talaria/ui/app.py; the units are sequential —
U4 waits for CR2 — so there is no concurrent edit.)

**Scope note:** the stress corpus's projected line counts change when error paths start committing
partials (the generated corpus carries interleaved malformed frames, gate.py:337-341); the
re-baseline lands in U6's gate run — expected, not a regression.

**Test scenarios:** an error arriving mid-fence commits the partial streaming and reasoning
buffers as entries, then clears (AE2's error leg); each typed terminal cause (`auth_failed`,
`dial_failed`, `orderly_close`, `reconnect_exhausted`) commits likewise; a transient reconnect
resuming the same response produces the content exactly once (the dedupe scenario); `cancel_turn`
keeps committing both buffers (the commit pinned by regression) and stops stripping them (its one
change); the typed cause travels the live path end-to-end (`LiveSource.bind` →
`note_connection_state` → the domain transition, exercised against a live source in the
transport suite); a cancellation arriving
mid-table (after the header row) commits the partial table (R14's mid-table leg — the parser
reinterprets an unterminated table as a paragraph, which is exactly why the case exists);
`message.interim` replacement respects the committed/interim boundary and increments the stream
generation; content preservation — leading indentation survives **byte-for-byte** (a four-space
indented body commits as an indented code block — four spaces, because GitHub-Flavoured Markdown
parses two-space indentation as a paragraph), trailing blank lines survive,
whitespace-only bodies survive, an open fence survives, across interim, final-replacement, error,
cancellation, and disconnect paths; the entry-scoped view carries a stable entry id, a reasoning
body with **no** `·` weld while the flattened line buffer keeps it, and an in-flight reasoning
tail with its own generation (R18's projection half — today projection.py:276 projects only the
assistant buffer); the v0.1 pin
`test_every_transcript_entry_survives_into_the_line_buffer` passes unmodified.

### U3. The block factory and its forgery proof

**Goal:** One constructor for safely configured entry documents — parser configuration plus inert
rendering hooks — with a test per forbidden channel (R4, R9, R10, R15; KTD3, KTD4).

**Files:** talaria/ui/blocks.py (new), tests/ui/test_blocks.py (new).

**Scope note (R4):** the factory is configuration and subclassed rendering hooks over the built-in
widget family — no markdown parsing or block-assembly engine is written here; CR3 verifies the
glue-only claim.

**Test scenarios (tests/ui/test_blocks.py):** the AE3 quartet — `<script>alert(1)</script>`
visible as text, `[bold red]x[/]` unparsed, a raw ANSI escape defanged, `[click](https://example.com)`
styled but inert; the link-inertness test asserts **no `Markdown.LinkClicked` message is emitted**
on click (the stock widget emits it even with `open_links=False` — the factory's rendering hook
must not install the action metadata) and nothing is opened or fetched; an image renders as
`alt (target)` text, nothing fetched (R10's image clause); a bare URL stays text (linkify off);
the allowlist upper bound — a snapshot pin of the parser's exact enabled rule set (RA1's
underscore emphasis and strikethrough are in it; anything else appearing fails the pin); defang
runs before parse (a NUL/bidi payload arrives as control pictures in block content); two entries
with an unclosed fence in the first never absorb the second (R15); the Textual 8.2.8 pin test —
append reparses from the last unfinished block, update replaces, `get_stream` exists and is
public (the KTD3 comparison evidence) — fails on semantic drift.

### U4. The hybrid transcript pane

**Goal:** Mount committed assistant/reasoning entries as block documents, keep line widgets for
every other kind, stream both tails progressively, restate cap, condensation, and anchor over
mixed units, and restate `rendered_lines` so the content claim survives (R1, R2, R3, R5, R16,
R17, R18; KTD1, KTD2, KTD3).

**Files:** talaria/ui/transcript.py, talaria/ui/app.py, talaria/ui/markdown.py,
tests/ui/test_transcript_blocks.py (new), tests/ui/test_transcript_bounds.py,
tests/ui/test_markdown.py, tests/transport/test_session_startup.py,
tests/transport/test_source_equivalence.py.

**Scope notes:** `rendered_lines` is redefined as a content reconstruction — line widgets
contribute their line; block entries contribute their projected source lines via their line spans
— so `pane.rendered_lines == view.lines[pane.condensed_count:]` remains a true **content** claim
under mixed mounting, and the three existing suites that assert it (tests/ui/test_markdown.py:297,
tests/transport/test_session_startup.py:607, tests/transport/test_source_equivalence.py:277,299)
are updated in place, never deleted. The inline styler is retired from the live path by this unit:
`MARKDOWN_KINDS` (transcript.py:68) is exactly the set that moves to block documents, leaving
`inline_markdown` with zero live callers — it is kept in the tree as the KTD8 fallback's styling
half (the fallback amends R4 only if invoked; the module carries a comment saying why it is
retained), and tests/ui/test_markdown.py keeps its parser-level tests while its pane-level
assertions move to the reconstruction. Tables: per RA2, the factory's table style wraps cell
content (no ellipsis, no tooltip dependence); this unit asserts full-content legibility at 80
columns.

**Test scenarios (tests/ui/test_transcript_blocks.py, tests/ui/test_transcript_bounds.py):** AE1 —
a fence streamed opener/body/closer renders structure at each surviving boundary, no inline
code-span styling inside the fence, one bounded region when closed; an interim replacement shows
the authoritative text exactly once (AE7) via the generation counter, never a prefix guess; both
tails live at once — an assistant delta and a reasoning delta in the same turn render in their own
documents, neither stealing the other (R18); commit hands each tail's source to its entry document
keyed by entry id without a pane rebuild; a terminal path stops and awaits pending widget writes
(R16 clause 2); a stale stream write arriving after its widget was condensed away or removed
updates nothing and raises nothing (R16 clause 3 — exactly the mount-cap interaction); the
descendant count under KTD1(a)'s ceiling at every coalescing boundary, including across an
oversized-newest-entry overage (the KTD2 rule); condensation folds whole units with the round-up
rule, a **block-rendered** newest entry survives whole (a fallen-back line-rendered one may fold
under the cap), and the banner's line arithmetic still sums; a reader
scrolled above the stream holds position through reinterpretation, resize, and condensation (R17)
and follow-bottom is never stolen; per-construct render oracles — heading renders as a heading
block, bullet and ordered lists as list blocks, a block quote as a quote block, each asserted by
block class and geometry and each proven to fail when the construct is flattened to a paragraph
(R1's oracle, not just presence); tables render under RA2's bounded-fractional column rule with
every cell's actual painted content asserted at 80 columns
(AE4/R3 as amended).

### U5. Kind differentiation

**Goal:** Give the five kind groups their named visual channels, composed with block rendering
(R7, R8; KTD5).

**Files:** talaria/ui/transcript.py, talaria/ui/app.py (CSS), tests/ui/test_kind_styles.py (new).

**Test scenarios (tests/ui/test_kind_styles.py):** AE5 — one line of each group on screen,
adjacent groups distinguishable by computed style with content ignored; a reasoning fence carries
both the kind channel and fence rendering (R8); per-widget height equal with styling on and off;
styling introduces no new theme variables (the KTD5 constraint, asserted against the stylesheet);
the twelve-member mapping is total (a new `TranscriptKind` member fails the mapping test rather
than silently rendering ungrouped).

### U6. The restated gate, the feature corpus, and the green re-run

**Goal:** Replace `interface_shows_everything`'s one-line-one-widget claim with the two-part
ownership proof, grow the corpus and the sideband timeline this feature needs, and re-run the gate
green (R5, R11a-guard, R11b, R12, R13, R14, R18; AE6).

**Files:** talaria/replay/gate.py, talaria/replay/stress.py, talaria/replay/source.py,
tests/replay/test_gate.py, tests/ui/test_transcript_bounds.py,
docs/analysis/2026-08-09-block-markdown-gate-results.md (new, beside the existing gate record
docs/analysis/2026-08-03-textual-validation-gate-results.md).

**Scope notes:** `content_is_complete` (gate.py:290) and its callers are untouched — the R11a
guard is explicit: the unit's diff must not touch the function, and the v0.1 projection pin
passes verbatim. Confirmed-cancel and terminal disconnect are **not wire frames** — interrupt
replies decode to `NonEventFrame` and are ignored by the reducer (decode.py:176, state.py:1246),
and transport callbacks are not recorded at all — so AE2 at gate level needs a **deterministic
sideband timeline**: a scripted action track (timestamped confirmed-cancel and typed-disconnect
injections, ordered against frame indices) that the replay source applies alongside the frame
log; determinism includes the sideband. The results document pins the environment exactly:
theme `textual-dark`, terminal sizes (including 80 columns), Textual 8.2.8, corpus identities by
sha256, and the reproduction commands.

**Test scenarios (tests/replay/test_gate.py plus the gate's own corpus verdict):** the two-part
ownership proof — (1) mounted-window ownership: every projected line inside the mounted window of
a block entry is covered by that entry's document blocks' `source_range` spans, with the
blank-line accounting rule stated (a probe shows inter-block blank lines are owned by neither
block — the proof accounts them to the entry's span, not to a block), and applied to **both
provisional tail documents at every checkpoint**, not only committed entries. Zero-block sources
get no document at all: an empty, whitespace-only, or newline-only body parses to zero blocks and
the resulting document mounts at **height zero** — probed — so widget presence would claim
visibility R11 doesn't get; instead, an entry (or tail state) whose parse yields no blocks
**line-renders**, preserving its blank rows as visible line widgets exactly as today, and falls
under the existing window comparison rather than the block proof; (2) condensed-range
accounting: folded units are accounted by their line spans in the banner arithmetic — plus the
semantic comparison (defanged source versus rendered content per construct) and mutation tests
(dropped text, a wrong block class, a hidden construct each make the gate fail); construct-specific
visual oracles for every R1 construct (heading, both list types, table grid, fence region, quote)
each proven to fail when flattened; line-rendered kinds keep the existing window comparison;
progressiveness asserted at timed intermediate checkpoints, not only settled (R5/R14), including
the two-tail overlap case (R18); the adversarial workloads — the KTD1(d) fence, table, and
unbroken mega-line, exact
sizes as specified — hold the latency and descendant ceilings with high-water figures recorded;
replay determinism compares normalized block structure (ordered classes, source ranges, semantic
content; runtime identifiers excluded) under the pinned width, theme, and framework version (R12),
sideband included; early termination by cancellation, error, and typed disconnect renders all
received content, driven through the sideband timeline (AE2 at gate level), including the
mid-table case; resize including 80 columns; verdict green over both the existing corpora and the
new feature corpus.

## Scope Boundaries

Carried from the brainstorm unchanged: link activation, URL fetching, and images stay disabled
behaviors (images render alt and target as text — R10); diff rendering keeps RR-38's plain-text
stance; theme selection and terminal colour detection stay deferred, and kind styling uses the
existing theme's vocabulary; turning fence highlighting off is out of scope; no Hermes markdown
machinery is ported (ADR-0003). Amended by this plan's review (see Requirements Amendments):
underscore emphasis and strikethrough are **in** scope (RA1); per-cell table focus is **out** —
table legibility is met by wrapping (RA2). Additionally out of scope: the approved v0.2
answerability plan and its execution spec are untouched; transcript memory eviction (KTD14's
deferred item) is not taken up; the styled-line-run fallback is not built ahead of the evidence
that would invoke it (KTD8 — `talaria/ui/markdown.py` is retained but retired from the live path
as its styling half).

Deferred to follow-up work: exposing the block structure to `terminal_read` (the agent-facing
buffer stays line-indexed); markdown in `tool` or `user` entries (the inline decision's reasoning
still binds).

## Risks

- **The widget family may not coexist with the pane's mechanics at acceptable cost** — this is
  the brainstorm's named assumption, it is exactly what U6 measures, and KTD8's fallback exists
  because it may measure false. The fallback decision point is after U6's first red run, with
  numbers attached, never before — and it is reachable because U4–U6 are branch-held: a red gate
  leaves `main` untouched, and the operator's gate decision chooses between amending R4 (invoking
  the fallback, whose styling half is the retained inline styler) and iterating on the branch.
- **Textual upgrade drift** — append/update/get_stream semantics are implementation details of
  public methods; the U3 pin test converts silent drift into a loud failure.
- **The gate's new claims could be weaker than the old ones** — mitigated by keeping
  `content_is_complete` and the v0.1 projection pin untouched under an explicit guard (R11a), by
  the mutation tests that prove the new proof can fail, and by the review gate on U6 before the
  verdict is trusted.
- **The sideband timeline is new replay machinery** — it is scoped to deterministic injection of
  two action kinds (confirmed-cancel, typed disconnect) ordered against frame indices; anything
  richer is out of scope, and CR6 reviews it against exactly that boundary.
