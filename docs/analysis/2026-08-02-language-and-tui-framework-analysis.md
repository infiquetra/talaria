# Language and TUI framework analysis

Status: `active`
Authority: `reference` — **this is not a decision.** No ADR has been written on the stack, and this
document does not constitute one. It is the evidence a decision would be made from.
Date: 2026-08-02
Evidence revision: Hermes Agent `7f4d15515` (2026-08-01)

Candidates evaluated: **Rust/ratatui**, **Python/Textual**, **TypeScript/Ink**, **Go/Bubble Tea**.

---

## Read this first: the evidence status

This analysis was produced by four independent frames working in parallel. Their claims about Rust,
Go, Python, Textual, ratatui, Bubble Tea, upstream Ink, and the JSON-RPC library ecosystems were
read from those projects' own repositories and package registries, and are unaffected by anything
below.

**Their claims about Hermes were read from a checkout six weeks stale** — `f5382752f` (2026-06-21)
rather than the `7f4d15515` (2026-08-01) that is installed and running. Every Hermes-derived number
and citation has since been re-verified. Here is what moved.

### Corrected measurements

| What                                                     | Analysis assumed |     Verified at `7f4d15515` |   Change |
| -------------------------------------------------------- | ---------------: | --------------------------: | -------: |
| `ui-tui/src/app/createGatewayEventHandler.ts`            |        945 lines |                   **1,419** |     +50% |
| `ui-tui/src/__tests__/createGatewayEventHandler.test.ts` |      1,601 lines |                   **1,984** |     +24% |
| `ui-tui/packages/hermes-ink/` (the Ink replacement)      |     27,823 lines |                  **29,291** |      +5% |
| the hand-written Yoga layout port inside it              |      2,326 lines |                   **2,751** |     +18% |
| gateway JSON-RPC methods                                 |              ~90 |                     **130** |     +44% |
| `ui-tui/src` overall                                     |    ~70,200 lines | **58,581 across 277 files** | see note |

Note on the last row: 70,200 counted a different subset. At `7f4d15515` the whole `ui-tui/`
workspace including `packages/` is 88,744 lines across 429 TypeScript files; `ui-tui/src` alone is
58,581 across 277.

### Corrected citations

| Claim                                                 | Analysis cited                          | Verified at `7f4d15515`        |
| ----------------------------------------------------- | --------------------------------------- | ------------------------------ |
| launcher admits only `<HERMES_TUI_DIR>/dist/entry.js` | `hermes_cli/main.py:1653`, `:1685-1690` | `hermes_cli/main.py:1975-1979` |
| the bundle runs as a blocking foreground child        | `main.py:2046`, `:2047`                 | `main.py:2393`                 |
| `HERMES_HOME` is fixed per process before imports     | `main.py:337`, `:501`                   | `main.py:505-512`              |
| the standalone attach path                            | `hermes_cli/web_server.py:11518`        | `web_server.py:15609`          |
| per-call profile override                             | `tui_gateway/server.py:678`, `:670-677` | `tui_gateway/server.py:1252`   |

### Claims that were re-verified and held unchanged

- The launcher accepts a Node.js bundle and nothing else, with a **three-element argv**; everything
  else travels by environment variable.
- `HERMES_HOME` is resolved at import by every spawned process. `main.py:505-512` states it
  outright: "Many modules cache HERMES_HOME at import time (module-level constants)."
- Hermes serves the full gateway over WebSocket at `/api/ws`, so a non-Node client reaches the
  complete protocol surface as a peer.
- **Hermes does not use Ink.** `ui-tui/package.json:33` aliases the dependency —
  `"ink": "npm:@hermes/ink@0.0.1"` — so the package named `ink` in that tree is Hermes's own fork.
  This is harder evidence for the analysis's central anti-Ink argument than what it originally cited.
- The profiling comments driving that fork are still in the tree and still explicit:
  `hermes-ink/src/ink/stringWidth.ts:281` — "CPU profile (Apr 2026) showed stringWidth dominating at
  21% of total"; `hermes-ink/src/utils/sliceAnsi.ts:16` — "sliceAnsi at 18% total."
- `hermes-ink/src/ink/terminal.ts:67` implements DEC mode 2026 (synchronized output) detection,
  including the tmux case.

### Does the staleness change the conclusion?

**No, and the direction of the error is worth stating.** Every corrected number moves the _protocol
reuse_ argument, which is the single strongest argument for staying on TypeScript. The reusable asset
is 3,403 lines (handler plus tests), not the 2,546 the analysis credited.

But the same correction produces a finding the original analysis could not have had: **that asset
grew roughly 50% in six weeks.** The analysis hypothesized "an ongoing drift-tracking tax" on
protocol reuse. Six weeks of drift is now measured rather than assumed — about +80 lines per week on
the handler alone. A bigger asset is a better one-time gift and a worse ongoing dependency, and
whatever gets ported is a snapshot of a moving target.

**A full re-run of the four frames is not warranted.** Roughly four-fifths of the scoring never
touched Hermes. Of the fifth that did, every substantive finding held and only the magnitudes moved —
and they moved in a way the analysis already anticipated in prose. If you want one anyway, the frames
worth re-running are the two stack surveys, not the requirements frame, whose gates are derived from
Talaria's own survivors rather than from Hermes source.

---

## The framing question, which decides this before any stack is scored

The requirements frame set a **swing rule** ahead of all scoring: settle whether Talaria is a process
Hermes launches or a process of its own, **before reading either stack survey.** Its reasoning:

> Choosing the Hermes-launched relationship forces the language. Choosing a language does not force
> the relationship.

[ADR-0001](../../platform-specs/04-architecture/adrs/0001-talaria-is-a-standalone-client.md) fired
that rule on 2026-08-02: **Talaria is a standalone process.** Under the analysis's own procedure, the
compatibility-mode criterion therefore scores zero, and everything else applies as written.

This matters more than any other single input, because compatibility mode is **the only row in
either scorecard that changes between framings.** Every other row is identical. TypeScript/Ink's
decisive advantage was that it could inhabit `HERMES_TUI_DIR` natively and no other candidate could.
ADR-0001 removed the mode from the product.

That is the finding in one sentence: **the stack was inherited from a bootstrap commit under a
framing the project has since abandoned, and nobody re-opened it.**

---

## Hard constraints — gates, not scores

Ten constraints were derived by walking all 28 survivors of the product-shape ideation. A stack that
fails one is out regardless of how it scores. Discriminating power is the frame's own judgment of
whether the constraint actually separates candidates.

| #   | Constraint                                                                                                                     | Discriminates |
| --- | ------------------------------------------------------------------------------------------------------------------------------ | ------------- |
| H1  | JSON-RPC over stdio **and** WebSocket, one long-lived connection, many concurrent sessions, out-of-band event frames           | low           |
| H2  | A framed protocol over a Unix domain socket, with a version check at connect                                                   | low           |
| H3  | Run headless, no controlling terminal, process lifetime independent of any terminal window                                     | **high**      |
| H4  | Be drivable end to end from a synthetic frame source, at controllable speed, with two renderers consuming the identical source | **high**      |
| H5  | Render a pinned region beside a high-rate scrolling region without flicker and without repainting unchanged cells              | **high**      |
| H6  | Do not foreclose appending the transcript to native scrollback while repainting only pinned regions                            | medium-high   |
| H7  | Reach Kanban deterministically, without a model choosing to call a board tool, without owning the board database               | medium        |
| H8  | Consume JSON Schema 2020-12 and emit typed bindings whose drift fails the build, not the runtime                               | low-medium    |
| H9  | Append to and verify a hash-chained log durably, including recovery from a torn write                                          | low           |
| H10 | Deliver plain Escape distinctly from Alt-prefixed sequences, three non-colliding steering chords, large paste as one event     | medium        |

Two of the four high-discrimination gates — H3 and H4 — are about **separability from the
framework**, not about the framework's quality. That is the shape of this decision.

---

## Weighting

Applies only among stacks that clear all ten gates.

| Rank | Criterion                                                       | Weight |
| ---: | --------------------------------------------------------------- | -----: |
|    1 | Operator debugging fluency under fatigue                        |     25 |
|    2 | Time to the first hard, load-bearing milestone                  |     20 |
|    3 | Rendering headroom at the fleet size that actually exists       |     15 |
|    4 | Measured cost of the three-transport integration                |     12 |
|    5 | Kanban reach quality without violating the repository guardrail |     10 |
|    6 | Inner-loop iteration speed — edit to observed change            |      8 |
|    7 | Protocol-knowledge reuse from the existing Hermes terminal tree |      5 |
|    8 | Distribution and packaging maturity                             |      3 |
|    9 | MCP server maturity in-stack                                    |      2 |

**Criterion 1 is a quarter of the scale, and its definition carries the weight.** Fluency here means
_debugging fluency under fatigue_, not syntax familiarity. What will actually break is a long-lived
multiplexed duplex socket, a detached process with no terminal, three transports in one process, and
a renderer under a ~16ms budget — async, cross-process, protocol-level failures, the class where the
gap between "I can read this" and "I can find this at 11pm" is widest.

**Criterion 7 is deliberately low, and the frame is emphatic about why.** Survivor 19's entire
evidence basis was a line count. Nobody opened the file. The risk it manages is a _knowledge_ risk,
and knowledge transfers by reading, in any language — only a literal copy-paste port is
language-locked. The same protocol knowledge also exists in Python already, on the emitting side.
The frame's instruction: **"Do not score this criterion from the line count."** Its ceiling is 15,
reachable only if someone reads the handler and finds that reconciliation and error-recovery logic
exceed 40% of it. **That read has never been done** — and the file is now 50% larger than when the
instruction was written.

**Criterion 8 is deliberately low** because `QUEUED.md` files independent distribution under "Maybe,"
and there are zero users. The distribution fact that matters today is the inner loop, which is
criterion 6 at nearly three times the weight.

---

## Findings by stack

### TypeScript / Ink — the incumbent

**For.** The only candidate that could inhabit `HERMES_TUI_DIR` (now moot). `vscode-jsonrpc` 9.0.1 is
the JSON-RPC layer under the Language Server Protocol — a decade of exactly this traffic shape —
plus `ws` and standard-library Unix sockets. `npm i -g talaria` is one command and the `bin` entry
already exists. Healthiest framework governance of the JavaScript options. The headless core tests
well in vitest, as Hermes's own 1,984-line handler suite demonstrates.

**Against, and it is structural.** Talaria needs a scrollable viewport, a table, a tree, a docked
strip, and a diffing renderer that holds up under a merged high-frequency tail. **Ink ships none of
them.** It has no cell buffer, no dirty-region tracking, and no synchronized output in v5.

The decisive evidence is what two better-resourced teams did about it. Nous Research replaced Ink's
rendering half while keeping React — 29,291 lines, including a hand-written 2,751-line Yoga layout
port — and aliased the package name so `ink` resolves to their fork. Anthropic shipped a from-scratch
renderer for Claude Code across a long run of point releases. **A solo operator should not plan to be
the third team to do this.**

### Python / Textual

**For.** Dirty-region tracking and partial paints verified in Textual's compositor, plus DEC 2026
probing in its Linux driver. Per-widget `asyncio.Queue` and `Task`, with queue depth exposed as a
backpressure signal. `run_test(headless=True)` and `Pilot` make the core/renderer split the
framework's own test mechanism, so the H3/H4 gates come close to free. Ships every widget Talaria
needs. The operator is Python-fluent and the machine running Talaria already runs a Python Hermes.

**Against.** The JSON-RPC client ecosystem is dead — `jsonrpcclient` last released 2023-02,
`jsonrpcserver` 2022-09 — so you hand-roll. Distribution is materially worse than `npm i -g`. Bus
factor of one: 90 of 91 commits are a single author, and there is no visible succession plan.
Region-level diffing is coarser than cell-level and nobody has benchmarked it for Talaria's tail.

### Go / Bubble Tea

**For.** Highest score on its frame's rubric. Bubble Tea v2 emits DEC mode 2026 synchronized output
**by default** and diffs cells — the anti-flicker protocol shipped on the default path, which is
directly what the README's "less flicker" promise needs. `jsonrpc2.NewPlainObjectStream` already
implements Hermes's exact newline framing over any `io.ReadWriteCloser`, covering all three
transports with one connection type. `program.Send` is safe from any goroutine, so the N-transports-
into-one-UI problem disappears. Single static binary, and GoReleaser can still publish an npm package
so `npm i -g talaria` survives. Roughly one week to productivity from a garbage-collected background.

**Against.** A new language for a solo side project. Genuine type-safety regression from TypeScript —
no sum types, no exhaustiveness checking, `nil` — which costs vigilance permanently. `teatest` is
still under `x/exp/` with only a pseudo-version for v2 and an open proposal to replace it. Cannot be
merged upstream into Hermes's TypeScript tree.

### Rust / ratatui

**For.** The best rendering architecture on paper: direct cell-buffer writes, no per-frame string
allocation. `TestBackend` is stable and `insta`'s snapshot review workflow is excellent — the one
category Rust wins outright. 1.2 MB binary against Go's 7.0 MB.

**Against.** **Ships no synchronized output in any of its four backends**, verified by grep across
the repository, and `ratatui::init()` hands you unbuffered stdout — two traps on the default path.
The leading JSON-RPC crate has no stdio transport and is pre-1.0; you hand-roll roughly 200 lines to
cover the three transports uniformly. Roughly ninety days to fluency from a garbage-collected
background. And the decisive one: **Talaria's core is shared mutable state with aliasing** — an
append-only ledger, a graph-shaped join table, a command bus, N subscriptions, multiple clients —
which is the single shape Rust taxes hardest and returns least on, because the idiomatic answer
(`Arc<Mutex<…>>`) moves the guarantees back to runtime anyway.

**On the OpenAI precedent, which points the other way.** OpenAI moved Codex CLI from TypeScript/Ink
to Rust/ratatui — same starting stack, same product category — and ratatui subsequently took a grant
from OpenAI's Codex Open Source Fund. Two more agent terminal UIs are on ratatui. The frame's
argument that this does not transfer: OpenAI's four stated reasons were zero-dependency install,
native OS sandboxing, no garbage collection in a hot model-calling loop, and an extensible wire
protocol. **Talaria executes nothing** — it is a client talking JSON-RPC to a Hermes gateway that
does the sandboxing — so the reason with the least substitutability is the one that does not apply.
And the agent loop runs in a Python process elsewhere, so the GC argument is about a different
workload. Finally: OpenAI has a team.

---

## The two scorecards

**These are two separate scoring passes with different rubrics, run by different frames. Do not add
or compare the totals across the two tables.** They are reproduced as scored.

**Pass A — Rust and Go, scored against "how much does this stack remove the problem," where 3 is
neutral against staying on Ink:**

| Dimension                         | Rust/ratatui | Go/Bubble Tea |
| --------------------------------- | :----------: | :-----------: |
| Rendering under load              |      3       |       4       |
| Async fit                         |      3       |       5       |
| Protocol-client ecosystem         |      2       |       5       |
| Distribution                      |      5       |       5       |
| Solo-maintainer cost              |      2       |       4       |
| Time-to-basic-TUI                 |      2       |       4       |
| Testability                       |      4       |       3       |
| Headless-core fit                 |      2       |       5       |
| Hermes launch-mode + upstream fit |      2       |       2       |
| **Total**                         | **25 / 45**  |  **37 / 45**  |

**Pass B — Python and TypeScript, scored against "how much work must Talaria do that the ecosystem
does not already do for it":**

| Category                   | Python/Textual | TypeScript/Ink |
| -------------------------- | :------------: | :------------: |
| Rendering under load       |       4        |       2        |
| Async fit                  |       5        |       4        |
| Protocol-client ecosystem  |       2        |       5        |
| Distribution               |       3        |       5        |
| Solo-maintainer cost       |       3        |       3        |
| Time to a basic TUI        |       4        |       3        |
| Testability                |       4        |       3        |
| Headless-core fit          |       5        |       4        |
| Compatibility mode         |       1        |       5        |
| **Total, drop-in framing** |  **31 / 45**   |  **34 / 45**   |
| **Total, fleet framing**   |  **35 / 45**   |  **31 / 45**   |

The fleet-framing totals rescore only the compatibility-mode row, on the convention that "cannot
inhabit a mode the product has abandoned" is not a defect. If you reject that convention, drop the
row from both and compare 30 to 29 — a near-tie. **Either way TypeScript does not lead under the
fleet framing**, and either way the rows are more informative than the sums: the two stacks are
strong in disjoint places.

---

## Traps — how this decision gets scored wrongly

Preserved because they are the most useful part of the analysis for a reviewer.

**Status-quo bias toward the existing code.** Real as a bias, near-worthless as an asset. The
bootstrap was 46 lines: a coloured heading, two lines of static text, and a quit handler. **Counter-
test:** anyone arguing to stay on "we already have working code" must name a behaviour in `src/` that
would have to be rebuilt. At bootstrap there was exactly one — whitespace normalization in
`formatPrompt`. The real status-quo asset is the toolchain — TypeScript config, vitest, Prettier,
CI — which is worth about a day, and should count as about a day.

**Novelty bias toward Rust.** The tell is any argument leading with performance for a workload whose
rate nobody has measured. A live board probe during the ideation run found a near-idle fleet — well
over a hundred completed tasks against single-digit active ones. Delegation is capped at depth 1 and
three concurrent children, and the external evidence is that agent teams converge on three to five
concurrent agents because review bandwidth binds first. **Counter-test:** any performance claim must
state the N and the rate it assumes. A stack that wins at 2,000 lines/second and one that wins at 500
are indistinguishable if the fleet produces 40.

**Conflating "the agent is Python" with "the client should be Python."** Half substantiated. The
dismissed half: the project's founding argument is that the legacy path calls agent implementation
code directly, and copying that coupling recreates the maintenance problem Talaria exists to escape.
The substantiated half: the Kanban gap is real and the alternatives are genuinely poor. **The honest
framing, which a scorer must not resolve silently:** a Python client's advantage here is an advantage
at doing precisely the thing the repository's own guardrail forbids. That is the operator's question,
not a point to be quietly awarded.

**Treating line-count reuse as value without reading the lines.** The sharpest trap in the set. The
handler is about 2.4% of `ui-tui/src`. If it were the concentrated crown jewel of protocol knowledge
you would expect it to be denser than that relative to everything around it. Maybe it is. **Nobody
has looked.**

**Optimizing for a fleet scale that does not exist.** The ideation's own revival condition says it
outright: if most profiles have never run a task, every fleet feature is being sized for a population
that does not exist. **Counter-test:** any criterion whose weight depends on more than ten concurrent
agents is speculative until utilization is measured.

**Letting the deferred measurement become an excuse not to decide.** The replay fixture is a JSON
Lines file from a generator script. It needs no recorder, no stack, and no running Hermes. If anyone
says "we cannot choose until we have the record-and-replay harness," this trap has fired.

**Evaluating the terminal framework when five things are being chosen at once.** The decision bundles
the language, the terminal framework, the concurrency model, the code-generation toolchain, and the
packaging story. A survey answering "which terminal framework is nicest" has answered the least
consequential of the five. **Counter-test:** for any claim, ask which of the five it is about.

**Mistaking "I can read this language" for "I can debug this language's async failures at 11pm."**
The most likely way criterion 1 gets scored wrongly — and criterion 1 is a quarter of the scale.

**Silently inheriting one of two documents that disagree.** The most dangerous, because nothing in
the repository signals it. This one already fired once and produced ADR-0001. **Counter-test, which
generalizes:** when two documents bear on a question, check whether the later one _engaged_ the
earlier or merely _postdates_ it. An unnoticed supersession is not a decision.

---

## What this analysis cannot settle

Stated plainly so nobody is handed false precision.

- **Which product Talaria is.** The rubric can price the fleet console against the better
  single-session Hermes terminal UI. It cannot choose between them, and it should not appear to.
- **Whether the operator will enjoy writing it.** Not a soft consideration: the analysis argues
  abandonment, not migration, is the dominant failure mode for a solo unfunded side project — which
  makes enjoyment causally upstream of whether Talaria exists in six months. Entirely unmeasurable
  here. If the probes come back close, this is the tiebreaker and it is the operator's call alone.
- **How much of "operator fluency" is really agent fluency.** A large share of implementation is
  delegated to agents. If most Talaria code is agent-written, criterion 1's 25 points split somewhere
  between the human's debugging fluency and the model's fluency in the target language. Nothing here
  measures that split, and it could move the largest weight on the scale.
- **Whether the result is calm.** The founding goal is "a calmer, more capable workflow." The
  rendering probe measures flicker, frame time, and bytes written. None of those is calm.
- **How fast Hermes's protocol will churn.** Schema-driven codegen bounds the damage. Nothing here
  predicts the rate — though the six-week drift measured above is the first real data point.
- **Three-year ecosystem bets.** Unknowable.

---

## Open, and worth deliberating

1. **Is upstreamability released, or reinstated?** ADR-0001 released it as an architectural
   constraint and `README.md` and `AGENTS.md` were corrected to match. If it is ever reinstated as
   binding, the analysis says insert it at 20 points — near-dispositive, because upstream Hermes is
   TypeScript — and **do not run a comparative survey at all**, because the answer is determined.
2. **Has anyone read the event handler?** Criterion 7 is scored at 5 provisionally and its ceiling is 15. The read has not been done, and the file is now 1,419 lines. This is the single cheapest
   action that could move the scale.
3. **Does the fleet population justify the fleet features?** Utilization across profiles has never
   been measured. It bears on the weighting through the scale trap above.
4. **How much of the implementation will agents write?** Unmeasured, and it bears on the largest
   weight on the scale.

---

## Provenance

Produced by a four-frame ideation run on 2026-08-02: a requirements frame deriving gates and weights
from the 28 survivors of the [product-shape ideation](../ideation/2026-08-02-talaria-product-shape-ideation.md),
two stack-survey frames, and a grounding frame on the launcher/language coupling. The full working
record stays out of this repository per the convention in
[DECISIONS](../engineering-journal/DECISIONS.md); this artifact is the reviewed and scrubbed form,
with all Hermes-derived claims re-verified against `7f4d15515` as described above.
