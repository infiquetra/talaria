---
date: 2026-08-08
topic: block-markdown-and-transcript-differentiation
maturity: requirements-ready
---

# Block-level markdown and transcript differentiation — requirements

## Summary

Talaria's transcript renders assistant and reasoning output as full block-level markdown — headings,
fenced code, lists, tables, block quotes — using Textual's built-in Markdown widget family, with
structure appearing progressively while a reply streams. Alongside it, every transcript line kind
gains a distinct visual treatment, so an operator can tell at a glance what they are looking at.

## Problem Frame

The transcript today is constant white text, line after line. The operator's own words, from live
daily driving: "It's difficult to read and differentiate what I am looking at." The request is not
typographic fidelity for its own sake — it is visual hierarchy, and block markdown is the chosen
first step of a broader output-enhancement arc.

The inline-markdown decision (DECISIONS.md, "Inline markdown is rendered on agent prose; block
markdown stays out of scope") anticipated exactly this moment: its recorded revisit condition is
"block-level rendering is taken up." Today's rendering is also subtly wrong, not merely flat — the
per-line styler has no fence awareness, so backtick pairs inside a code block are styled as inline
code spans, and fence delimiter lines render literal. No test anywhere renders a multi-line fence.

An adversarial review of this document's first draft (external engine, read-only, empirical — it
imported the vendored framework and drove the parser live) found one blocking contradiction and
several acceptance gaps; this revision incorporates its surviving findings. Two of its facts bind
everything below: the vendored Textual does **not** export a public streaming widget
(`MarkdownStream` is defined in `_markdown.py` but absent from `textual.widgets`), and its default
parser enables HTML and link-opening — inline HTML silently disappears, block HTML mounts nothing,
and links auto-open URLs.

## Key Decisions

**The full construct set ships together.** Headings, fenced code, lists, tables, and block quotes
are all required — the operator declined a smaller first cut. The original Large effort grading
stands.

**Rendering adopts Textual's built-in Markdown widget family; nothing is hand-rolled.** The v0.1
feature inventory's verdict on Hermes's 1,344-line markdown machinery was "the single largest
candidate for being replaced by a built-in rather than re-encoded," and ADR-0003 already forbids
porting it. The operator chose the built-in over a styled-line-run alternative.

**Streaming is fully progressive, stated falsifiably.** Structure appears as deltas arrive;
reinterpretation flicker on an unclosed fence is an accepted cost. Because Talaria coalesces paints
on a 50 ms boundary, "fully progressive" cannot mean "every delta is visible"; R5 states the
measurable condition. The commit-time-only shape suggested in the queue entry is rejected by
operator choice.

**Acceptance is the restated, re-run gate — v0.1's own standard — over a new feature corpus.** "One
line, one widget" is replaced by a block-aware bounded-rendering claim recorded in an architecture
decision record before implementation, the replay gate grows checks stated against the new claim,
and the gate re-runs green. The existing corpora contain no markdown and every generated turn
completes, so a green re-run of them alone proves nothing — R14 mandates the corpus this feature
needs.

**The styled-line-run approach is the named fallback, and taking it amends R4.** If the restated
gate cannot go green under the widget family, the recorded escape is rendering blocks as tagged,
styled line runs: the projection tags each line with block context (fence + language, heading
level, quote depth, list marker, table row) and the existing one-line-one-widget pane styles from
the tag — every line-indexed invariant survives, at a fidelity ceiling (aligned text instead of a
table grid, per-line highlighting). This is the analogue of v0.1 holding `prompt_toolkit` against
the Textual gate. R4 mandates the widget family, so invoking the fallback is an explicit
requirements amendment, not a silent swap.

**Kind differentiation rides in the same work.** The differentiation problem is only partly a
markdown problem; per-kind visual treatment attacks the rest and shares the styling surface.

**Syntax highlighting inside fences ships by default.** The built-in fence widget always invokes
its highlighter; keeping it is the zero-cost path, and turning it off would be extra work with no
requirement behind it.

## Requirements

**Block rendering**

- R1. Assistant and reasoning entries render headings, fenced code blocks, lists (ordered and
  unordered), tables, and block quotes as visually distinct block structures; all other entry kinds
  continue to render literally.
- R2. Fenced code blocks render as a visually bounded region distinct from prose, and inline
  code-span styling never fires on content inside a fence (fixes the current misrendering).
- R3. A table's full cell content is reachable **without a mouse** at every supported width — via
  wrapping, keyboard-driven scrolling, or an equivalent affordance; a hover-only tooltip does not
  satisfy this. Alignment may degrade at narrow widths. The acceptance matrix includes 80 columns
  and cells holding long unbroken values (URLs, identifiers, code).
- R4. Rendering is driven by Textual's built-in Markdown widget family; no markdown parsing or
  block-assembly engine is written in this repository beyond glue. (The styled-line-run fallback in
  Key Decisions amends this requirement if invoked.)

**Streaming**

- R5. Block structure appears progressively while a reply streams, stated falsifiably: any content
  prefix that survives one render boundary is structurally rendered by the next render boundary.
  Deltas that arrive and are superseded within a single boundary need no observable intermediate
  screen. Progressive rendering applies to reasoning streams as well as assistant prose (see R18).
- R6. A turn that ends early renders all content received. Concretely: on a turn-terminal path —
  confirmed cancellation, gateway error, or terminal disconnect — partial streaming and reasoning
  buffers are **committed to the transcript**, not cleared (today `_on_error` clears both without
  committing, `talaria/domain/state.py:1181` — this is a domain transition change, not
  presentation). A transient disconnect that reconnects and resumes the same response is not a
  terminal path and must not duplicate content; the requirement distinguishes the two explicitly.
- R16. Streaming replacement is defined, not assumed: `message.interim` replaces buffered deltas
  with authoritative text and a final body may differ from the accumulated prefix, while the widget
  streaming interface is append-only — the requirement is that replacement events render the
  authoritative text (replace wins over append), every terminal path stops and awaits pending
  widget writes, and a stale stream can never update a removed widget.

**Kind differentiation**

- R7. Transcript kinds are visually distinguishable by group, with the twelve `TranscriptKind`
  members mapped explicitly: operator (`user`) · assistant prose (`assistant`) · reasoning
  (`reasoning`) · activity (`tool`, `subagent`) · session record (`system`, `prompt`,
  `prompt-expired`, `cancelled`) · faults (`error`, `protocol-error`, `unknown-event`). Each group
  has a named visual channel (colour, dimming, marker); adjacent groups are distinguishable without
  reading content, asserted by computed style or screenshot comparison — not by judgment.
- R8. Kind styling composes with block rendering (a reasoning fence carries both treatments — the
  fence widget's own colours must not erase the kind channel) and changes no widget's height by
  itself, measured against the same screen with styling disabled.
- R18. Reasoning streams are projected provisionally and rendered progressively, like assistant
  text. The current projection omits in-flight reasoning entirely and welds a `·` presentation
  prefix onto the text — which makes a first-line heading, fence, or list invalid markdown — so the
  projection must carry raw body and presentation decoration separately. (The mechanism is
  planning's; the capability is required.)

**Safety and standing invariants**

- R9. The forgery guarantee, in allowlist form: gateway bytes may invoke **markdown semantics and
  nothing else**. Permitted: the block and inline grammar this document scopes, styled by Talaria's
  own theme. Forbidden, with tests that prove each: Rich console markup, terminal control and
  escape sequences, Textual actions/links that trigger behavior, and any styling directive outside
  the markdown grammar. Defanging (bidi overrides, zero-width characters, C0 controls) runs before
  parsing at parity with today's boundary, and ADR-0005's rule that untrusted text reaches the
  screen only through `literal_text` is amended, not silently broken.
- R10. HTML in gateway text is **literalized** — rendered visibly as text, never interpreted,
  never silently dropped (the widget default does both: inline HTML disappears, block HTML mounts
  nothing — each is content loss under R11). Links render as styled text; they are never
  auto-opened and no URL is ever fetched (the widget's `open_links` default is disabled). Images
  are not fetched; their alt text and target render as text.
- R15. Parser isolation per entry: every assistant or reasoning entry is its own parser document —
  an unclosed fence or quote in one entry can never absorb a later operator, tool, or system entry
  into agent-authored markdown. This is part of the forgery guarantee, not an implementation
  detail.
- R11. v0.1's content obligation splits into two proofs, both required: (a) the existing
  domain-entry-to-projection preservation check (`content_is_complete`) **stays intact and
  line-indexed** — it is not restated; (b) a new proof covers projection-to-screen for block
  rendering — every projected source region is owned by a mounted, visible block, with
  construct-specific visual assertions, because a Markdown widget can retain its full source while
  rendering nothing.
- R12. Replay determinism, defined: the same recording produces the same **normalized block
  structure** — ordered block classes, source ranges, and semantic content, with runtime-generated
  identifiers excluded — under fixed width, theme, and framework version. The comparison's
  environmental inputs are pinned in the gate.

**Acceptance and the measured gate**

- R13. An architecture decision record states the block-aware bounded-rendering claim **before
  implementation lands on it**: what is bounded (render work and height, not only widget count — a
  descendant tally alone lets one enormous fence consume unbounded work as a single block), where
  it is measured, its high-water instrumentation, and its ceiling. The target precedes the
  implementation rather than being fitted to it.
- R14. The gate re-runs green over a **new deterministic feature corpus** that exercises what this
  feature adds: every block construct; open-fence and mid-table early termination by cancellation,
  error, and disconnect; parser-attack cases (HTML, console markup, control sequences, link
  targets); every kind group; resize including 80 columns; and adversarial streaming workloads —
  a long unclosed fence and a growing table, with parse/update latency thresholds, because the
  widget reparses from the last unfinished block on every append (quadratic total work on a
  growing fence). Progressiveness (R5) is asserted at timed intermediate checkpoints, not only
  after settling — the current gate deliberately asserts nothing mid-stream, so a commit-only
  implementation would otherwise pass.
- R17. Scroll anchoring is part of the claim: streaming reinterpretation, resize, condensation of
  variable-height blocks, and early termination must not move a reader positioned above the
  change or steal follow-bottom state. Accepted fence flicker does not imply accepted scroll
  jumps.

## Acceptance Examples

- AE1. **Covers R2, R5.** A reply streams a fenced block: the opening ` ```python ` arrives, then
  body lines, then the closer. Structure appears at the render boundary after each surviving
  prefix; a backtick pair inside the body is never styled as an inline code span; the closed block
  renders as one bounded region.
- AE2. **Covers R6.** The operator interrupts while a fence is open and the gateway **confirms**
  the cancellation. The partial fence body is committed and rendered — nothing blanks. The same
  holds when the turn ends by gateway error, and when a disconnect is terminal; a transient
  reconnect that resumes the turn renders the content exactly once.
- AE3. **Covers R9, R10.** A recorded frame carries `<script>alert(1)</script>`, a Rich markup
  literal like `[bold red]x[/]`, a raw ANSI escape, and `[click](https://example.com)`. The screen
  shows each as visible text content — the HTML literalized, the Rich markup unparsed, the escape
  defanged, the link styled but inert — and nothing opens, fetches, or executes.
- AE4. **Covers R3.** An assistant reply contains a five-column table with one long unbroken URL
  cell, at 100×30 resized to 80 columns. Every cell's full content is reachable without a mouse;
  alignment may degrade.
- AE5. **Covers R7.** A screen holding one line from each kind group is distinguishable
  group-by-group with content blurred, asserted by computed styles — including a reasoning fence,
  which keeps its kind channel.
- AE6. **Covers R11, R13, R14.** The feature corpus replays under the restated gate: zero content
  loss across both proofs at every checkpoint, the boundedness claim's instrumentation stays under
  its ceiling through the unclosed-fence and growing-table workloads, verdict green.
- AE7. **Covers R16.** A `message.interim` replaces three accumulated deltas with shorter
  authoritative text; the screen shows the authoritative text. The final body differs from the
  accumulated prefix; the final body wins, exactly once.

## Scope Boundaries

- Link activation, URL fetching, and images are explicitly disabled behaviors (R10) — not
  incidental widget defaults. Re-enabling any of them is future work with its own safety case.
- Diff rendering keeps RR-38's stance (plain text, chrome stripped) — not part of this work.
- Theme selection and terminal colour detection stay deferred as recorded in v0.1's scope
  boundaries; kind styling uses the existing theme's vocabulary.
- Turning fence syntax highlighting **off** is out of scope (it ships because the widget ships it).
- No Hermes markdown machinery is read for porting (ADR-0003); its streaming de-duplication design
  may be read for education when planning the progressive path.
- The approved v0.2 answerability plan and its execution spec are untouched; this document feeds
  its own plan.

## Dependencies / Assumptions

- The vendored Textual ships the Markdown widget family, but **does not export a public streaming
  interface** — `MarkdownStream` exists in `_markdown.py` and is absent from `textual.widgets`
  (verified by live import). Planning must choose: pin against the internal class, upgrade Textual
  to a version that exports it, or drive `Markdown.append`/`update` directly — and no non-public
  API is relied on without a pinned version and a test that fails on upgrade.
- Assumption, unverified: the widget family can coexist with the transcript's condensing,
  mount-cap, and scroll-anchor mechanics at acceptable cost. This is precisely what the restated
  gate measures, and the named fallback exists because it may measure false.
- R6 and R18 require domain-layer changes (committing partial buffers on terminal paths;
  projecting provisional reasoning with decoration separated). This work is **not**
  presentation-only, and ADR-0002's boundary (the domain never imports the terminal framework)
  binds those changes.
- Assumption: per-kind styling can be expressed without adding rows or changing widget heights
  (consistent with the caret-marker constraint recorded in the v0.2 plan).

## Outstanding Questions

Deferred to planning — none block `/plan`:

- How committed entries and the streaming tail share widget structure (one widget per entry is the
  R15 floor; whether the streaming tail is its own widget or the last entry's is open), and how
  the condensed-window banner coexists with block widgets.
- The kind-styling mechanism (per-widget classes vs. rendering-time treatment) and its composition
  with the widget family's own styles.
- The exact boundedness instrumentation the ADR commits to (R13 names what it must cover; planning
  proposes the metric and ceiling, the ADR records them).
- The structured projection's shape for R18 (entry identity, raw body, kind, commitment state,
  presentation decoration) — capability required here, design deferred.

## Sources / Research

- `docs/engineering-journal/QUEUED.md` — "Block-level markdown: headings, fenced code, lists,
  tables, block quotes" (the P2 entry this work takes up; its four line-indexed mechanisms and
  streaming-ambiguity analysis).
- `docs/plans/2026-08-08-v0-2-session-handoff.md` — candidate C's framing and sequencing.
- `docs/engineering-journal/DECISIONS.md` — "Inline markdown is rendered on agent prose; block
  markdown stays out of scope" (rationale, the raw-string rejection R9 answers in allowlist form,
  and the revisit condition this brainstorm satisfies).
- `talaria/ui/markdown.py`, `talaria/ui/transcript.py` — the inline styler and the line→widget seam
  as they exist today.
- `talaria/replay/gate.py` — `interface_shows_everything`, `content_is_complete`, the
  mounted-widget ceiling, and the settled-only streaming posture R14 replaces with timed
  checkpoints.
- `talaria/domain/state.py:1181` — `_on_error` clearing partial buffers uncommitted (the R6
  defect); `talaria/domain/projection.py:260,291` — the flattened, prefix-welded projection R18
  replaces.
- `docs/analysis/2026-08-02-hermes-tui-feature-inventory.md` row B1 — the replace-with-built-in
  verdict on Hermes's markdown machinery.
- `docs/analysis/2026-08-02-hermes-reconciliation-rules.md` RR-16, RR-38 — raw text preferred over
  pre-rendered ANSI; diff chrome stripped.
- `platform-specs/04-architecture/adrs/0005-textual-is-talarias-presentation-layer.md` — the
  `literal_text`-only rule R9 amends; ADR-0002 — the domain boundary binding R6/R18.
