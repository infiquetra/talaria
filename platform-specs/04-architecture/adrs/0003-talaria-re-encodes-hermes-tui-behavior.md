# ADR-0003: Talaria re-encodes the Hermes terminal UI's behavior rather than porting its code

Status: `accepted`
Date: 2026-08-02
Deciders: operator
Affected components: protocol normalization, domain state, terminal UI, feature scope, tests
Evidence revision: Hermes Agent [`7f4d15515`](https://github.com/NousResearch/hermes-agent/tree/7f4d155159e2a5d4098bb2f27d3fccb01ff84c3d) (2026-08-01)

## Context

Two positions on the relationship to Hermes's existing terminal UI have been held in this project at
different times, and the difference is not cosmetic.

The first was a port: take the shipping terminal UI and translate it into the chosen stack. The
attraction is obvious — the behavior already works, and translation is mechanical enough to delegate.

The second is the one that survived: **evaluate the existing terminal UI feature by feature and
decide what earns a place, then re-encode what does.** The operator's position is that the current
Hermes terminal UI "is not completely awful" — it needs adjustments in places, and it needs
significant additions that diverge from a conventional agent-harness interface — and that the right
question is which features to re-implement, not how to translate a source tree.

Evidence gathered on 2026-08-02 supports the second position, and it did so against the expectation
of the person gathering it. A full read of
[`ui-tui/src/app/createGatewayEventHandler.ts`](https://github.com/NousResearch/hermes-agent/blob/7f4d155159e2a5d4098bb2f27d3fccb01ff84c3d/ui-tui/src/app/createGatewayEventHandler.ts)
(1,419 lines) was run to test whether the file's reconciliation and error-recovery content exceeded
40% — the threshold set in advance for treating Hermes's terminal UI as a substantial reuse asset. It
measured roughly 22% of code lines. The bar was not cleared, and the reasons matter more than the
number:

- **The reusable protocol content is rules, not machinery.** The session cross-talk guard is three
  lines (`:720-722`). Preserving a terminal sub-agent status against a late event is four
  (`:609-612`). Normalizing an unknown sub-agent status to a safe fallback is nine (`:374-382`).
  Flushing an abandoned clarify prompt into the transcript with a de-duplication set is fourteen
  (`:413-426`). Each of these cost Hermes a bug to discover and costs roughly a line to re-encode. The
  value is in knowing the rule exists, and that value transfers to any language.
- **A large part of the file is not protocol at all.** Roughly 310 lines are terminal theme and
  background detection — an OSC-11 background probe, the finding that xterm.js-based hosts and tmux
  both report `#000000` regardless of the real background, an OSC-10 foreground tiebreaker for that
  case, and a platform-specific last resort. That is valuable knowledge about terminals. It is not
  knowledge about Hermes.
- **The densest engineering in the file is a cost of its framework, not an asset.** A forced redraw
  scheduled 40 milliseconds after a theme change (`:101`) exists because of observed tearing in the
  renderer's diff cache. Deferring a configuration fetch out of construction avoids tripping React's
  re-render guard in embedded terminals. These are repairs to Ink. Porting them would import the
  problem along with the fix.

Two things in the Hermes terminal UI are genuinely portable in bulk rather than as rules: the typed
protocol contract in `gatewayTypes.ts` (741 lines), which is effectively a schema and can be consumed
by any language, and the accumulated terminal knowledge described above.

## Decision

**Talaria treats Hermes's terminal UI as documentation of behavior, not as a source tree to
translate.** No file is ported. Hermes's React and Ink state shape is not reproduced. Hermes
implementation modules are not imported, which the standalone-client boundary already forbids and
which a language change makes impossible in any case.

What Talaria takes from it, deliberately and by name:

1. **The protocol contract** — event and method shapes, treated as a schema to be re-expressed in
   Talaria's own typed models at the wire boundary.
2. **The reconciliation rules** — the specific defensive behaviors listed above and the others found
   alongside them, re-encoded in Talaria's normalization layer with their own tests. A rule that is
   not re-encoded is a bug Hermes has already fixed and Talaria will ship.
3. **The hard-won terminal knowledge** — background and theme detection, and comparable findings
   about how terminals actually behave, checked against what the selected presentation layer already
   solves before being re-encoded.

What Talaria explicitly does not take: the component structure, the state shape, the framework
workarounds, and the feature set as a default.

**The mechanism is an inventory, not a judgment call per file.** Before implementation of a given
surface begins, the corresponding Hermes terminal UI features are enumerated with an explicit
verdict — keep as is, keep with changes, or drop — and the verdict is recorded. This makes Talaria's
feature set a decision the project made rather than a residue of what was convenient to translate.
The inventory is derived from Hermes at a pinned revision, and the revision is recorded with it.

## Rejected alternatives

**Port the terminal UI into the chosen stack.** Mechanically faithful and easy to delegate. Rejected
because `ui-tui/src` is 58,581 lines across 277 files, the large majority of which is neither protocol
nor behavior Talaria wants; because porting carries the framework's repairs into a codebase that does
not have that framework's problems; and because it would make Talaria's feature set an accident of
translation at exactly the moment the project is trying to decide that feature set on purpose.

**Start clean and treat Hermes as prior art only.** Cheapest to execute and produces the tidiest
code. Rejected because the reconciliation rules are individually small, individually invisible, and
each one represents a defect Hermes has already found. Discovering them a second time in production
is the most expensive way to obtain them.

**Depend on Hermes's terminal UI packages directly.** Rejected by the standalone-client boundary in
ADR-0001, and made moot by the language decision in ADR-0004. Hermes's `hermes-ink` package is
private to that project and is not a stable interface.

## Consequences

**Easier.** Talaria's feature set becomes a product decision with a record, which is what makes the
"adjustments and additions" framing actionable. Each reconciliation rule costs roughly a line plus a
test, so the useful content of the largest handler file is a small amount of work rather than a
migration. The language decision stops interacting with this question at all — the rules transfer
regardless of stack.

**Harder.** Re-encoding means re-testing; nothing arrives already proven. The inventory must be
systematic rather than opportunistic, because the failure mode is silent — a missed rule produces a
defect months later that Hermes fixed years earlier, and nothing in the codebase points at the
omission. Reading Hermes requires pinning a revision every time, since an earlier reading of a
six-week-old checkout in this project produced a materially wrong picture and had to be corrected
after publication.

**Stale as a result.** Any suggestion in the project's earlier direction document that Talaria begins
as a port of the existing terminal UI no longer holds.

## Revisit when

Talaria and Hermes's terminal UI converge closely enough that a specific surface is genuinely
identical in both, and translating it would be cheaper than re-deriving it. That would be a
per-surface exception rather than a reversal, and it would still exclude the framework-specific
repairs.

## Open, and deliberately not decided here

The reconciliation logic in `ui-tui/src/app/turnController.ts` (1,092 lines) has not been read, only
its call surface. If its engine is less annotated than the handler that delegates to it, part of the
reuse argument recovers there, and the rule catalogue this ADR depends on is incomplete until it is
read.
