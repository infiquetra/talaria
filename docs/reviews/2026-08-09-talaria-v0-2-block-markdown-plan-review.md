---
title: Doc-review — the v0.2 block-markdown plan and execution spec
type: review
date: 2026-08-09
target: docs/plans/2026-08-09-talaria-v0-2-block-markdown-plan.md
also_reviewed: docs/plans/2026-08-09-talaria-v0-2-block-markdown-spec.json
reviewed_revision: main @ ae8e36d (plan revision 1); fixes produced revision 2, hardened to 94b2507 across eleven Codex confirm rounds
origin_requirements: docs/brainstorms/2026-08-08-block-markdown-and-transcript-differentiation-requirements.md
blocked: false
outcome: all findings fixed in place (plan revision 2 + spec revision); eleven Codex confirm rounds converged to PASS at 94b2507; three operator-vetoable requirements amendments recorded (RA1, RA2, RA3)
---

# Doc-review — the v0.2 block-markdown plan and execution spec

**Verdict: the plan as first written was not implementation-ready — thirty-five distinct defects,
three of them execution-stopping — and every one is resolved in revision 2, at the cost of three
explicit requirements amendments the operator may veto.** The reviewed revision was `main` at
commit `ae8e36d`. Four reviewers ran: three panel lenses (citation verification, adversarial
readiness, requirements coverage) plus the operator's Codex engine, whose live probes against the
installed Textual 8.2.8 produced the review's most consequential findings. The Codex reply is
archived verbatim in the machine-local saga evidence, sha256
`86150cd955eff7f4c5c5f374cca9e8d14775aa0d45bcaa2cc1223b6e3dec92cb`. After the thirty-five fixes,
eleven scoped Codex confirm rounds hardened the revision further (see the convergence section
below); the eleventh returned PASS with no new findings at commit `94b2507`.

## The three operator-vetoable amendments

- **RA1 — underscore emphasis and strikethrough enter scope.** The parser preset ships both;
  disabling underscore emphasis would disable asterisk emphasis too (one `emphasis` rule covers
  both). The plan's deferral was written for the regex styler this feature retires. Veto path:
  demand rule-level disabling — costs asterisk emphasis as well.
- **RA2 — R3's table reachability is met by wrapping cells, not focusing them.** The stock widget
  fails R3 today (non-focusable ellipsis-plus-tooltip cells, live-probed at 80 columns). Cell
  focus would build a data-grid caret model inside the transcript and collide with the
  answerability focus order. Veto path: demand per-cell keyboard focus — a materially larger U4.
- **RA3 — on fallen-back entries only, on-screen visibility narrows to the clipped rows plus a
  banner.** Recorded during the confirm rounds, not the initial review: the non-wrapping fallback
  for degenerate content (a 100,000-character line, past-ceiling estimates) clips at the viewport,
  and no presentation of such a line shows its tail. R11's projection-to-screen visibility is
  satisfied for that entry class by one painted row per projected line plus a banner naming the
  clip and cause; the gate proves banner presence and exact row count, not clipped-cell
  reachability; the content stays byte-exact in the terminal-read buffer. Veto path: demand a
  keyboard expand/inspect affordance now — U4 grows the per-entry horizontal-navigation machinery
  RA2 declined; it is otherwise queued follow-up work.

## Findings and dispositions

All thirty-five are **fixed** in plan revision 2 and the revised spec. Sources: A = citations
panel, B = readiness panel, C = coverage panel, X = Codex. Merged where lenses found the same
defect.

| # | Pri | Source | Defect (compressed) | Disposition in revision 2 |
| --- | --- | --- | --- | --- |
| F1 | P0 | B | U4 breaks three suites outside every unit's file list (`rendered_lines` one-line-one-widget assertions) | U4 owns the three files; `rendered_lines` restated as content reconstruction from line spans |
| F2 | P0 | B, X | "Under the cap at every instant" unenforceable (batched thread-parsed mounting; count unknowable pre-parse) | Ceiling observed at 50 ms boundaries; enforcement moved to the pre-apply fold rule; ADR records why |
| F3 | P0 | B, X | Fold-whole-units vs line cap: no tie-breaker, no oversized-entry rule | Round-up rule + newest-entry-always-mounted with recorded overage (KTD2) |
| F4 | P1 | B | Snapshot-driven pane has no append fragment or replace discriminator | Stream generation counter published by U2; suffix-append on same generation, `update()` on change (KTD3) |
| F5 | P1 | B, X | Terminal-vs-transient disconnect undecidable from today's events | Typed end-of-stream cause seam; four transport sites enumerated; dedupe backstop pinned (KTD7) |
| F6 | P1 | B, X | No height ceiling; count metric bounds nothing (a table mounts one widget per cell) | Four ceilings: descendant count, height via fold rule, work in source bytes, latency with exact workloads (KTD1) |
| F7 | P1 | B, C, X | R18 provisional reasoning projected nowhere; one tail can't hold two streams | Two independently keyed tails, both projected (U2) and rendered (U4); overlap corpus case (U6) |
| F8 | P1 | B | Inline styler orphaned — cited as the defect, owned by no unit | U4 owns `talaria/ui/markdown.py`; retired from live path, retained as the KTD8 fallback's styling half |
| F9 | P1 | B | KTD8 fallback unreachable; no rollback story | Branch-hold: U4–U6 on `feat/v0-2-block-markdown-build`, merged only on green gate + CR6 pass |
| F10 | P1 | C | Spec U4 dropped the fence/inline-code-span assertion | Restored in spec U4 prompt and returns |
| F11 | P1 | C | R16 clauses 2–3 (terminal path awaits writes; stale stream vs removed widget) tested nowhere | U4 scenarios + returns |
| F12 | P1 | C, X | R10 image clause discharged nowhere; Textual drops the target and emits LinkClicked despite `open_links=False` (live-probed) | Rendering hooks join the forgery boundary: no action metadata, no LinkClicked, images render `alt (target)` (KTD4, U3) |
| F13 | P1 | C, X | Parser silently ships underscore emphasis + strikethrough; no allowlist upper bound | RA1 amendment + exact enabled-rule-set pin test (U3) |
| F14 | P2 | A, B, C, X | `_on_error` cited at state.py:1181; actual :1586 | Corrected in plan and spec |
| F15 | P2 | A | U2 names nonexistent `tests/domain/test_state.py` | Corrected to `test_transcript_state.py` + `test_turn_lifecycle.py` |
| F16 | P2 | A, C, X | Nonexistent `docs/measurements/`; literal `2026-08-XX` placeholder | `docs/analysis/2026-08-09-block-markdown-gate-results.md`, beside the existing gate record |
| F17 | P2 | B | `cancel_turn` already commits partials; KTD7 listed it as a change | Regression-test-only, stated in Grounding, KTD7, and spec U2 |
| F18 | P2 | B | ADR-0005 decision 3 superseded but U1 amends only decision 7; 500-vs-600 unstated | U1 amends both; 500 = pane enforcement cap, 600 = gate ceiling, both in descendants |
| F19 | P2 | B | U2 changes stress-corpus projections; nothing re-baselines until U6 | Expected re-baseline noted in U2, landed in U6 |
| F20 | P2 | C | Spec U6 widened to all of R11, including the half that forbids restating | R11a guard: `content_is_complete` untouched, v0.1 pin verbatim, in prompt and returns |
| F21 | P2 | C | U6 name-drops R17 with no scenario | R17 removed from U6's list; U4 owns it |
| F22 | P2 | C | R14's mid-table early termination missing | Domain leg in U2, gate leg in U6 |
| F23 | P2 | C | Spec U2 dropped the `message.interim` boundary scenario | Restored with the generation-bump requirement |
| F24 | P2 | C | CR units gate on completion, not verdict; no review checklist; CR6 gates nothing | Verdict-gating stated in description and every CR prompt; CR checklists carry unit returns; CR6 gates leaf completion and the branch merge |
| F25 | P2 | C, X | R3 named by no unit; R4 discharged by no unit; traceability claim false | Full R1–R18 + AE1–AE7 traceability table added; R3→U4 (as amended), R4→U3/U4 with glue-only note |
| F26 | P3 | A, B, C, X | Projection citation drifts (:260→:261, :301→:305) | Corrected |
| F27 | P1 | C, X | U3 (widget work) not gated on U1's ADR — violates R13's ADR-first sentence | U3 depends on CR1; U2's parallelism recorded as the stated domain-only exception |
| F28 | P3 | C | "Existing theme's vocabulary" constraint dropped | Restored in KTD5 with a stylesheet assertion in U5 |
| F29 | P3 | C | U5 tiered effort medium while editing U4's files, no basis | Raised to high |
| F30 | P1 | X | Gate corpus cannot replay confirmed-cancel or disconnect (not wire frames; NonEventFrame ignored; callbacks unrecorded) | Deterministic sideband timeline, scoped to exactly two action kinds (U6) |
| F31 | P1 | X | R3 fails in the stock widget today; plan asserted an outcome with no implementation decision | RA2 amendment: wrapping, asserted at 80 columns (U4) |
| F32 | P1 | X | Region-ownership proof impossible under condensation and too weak (blank lines unowned; empty doc has no blocks; ranges don't prove content) | Two-part proof: mounted-window ownership with blank-line accounting + condensed-range accounting + semantic comparison + mutation tests (U6) |
| F33 | P1 | X | R1 constructs without correctness oracles (headings/lists/quotes could flatten to paragraphs and pass) | Per-construct class+geometry oracles, each proven to fail when flattened (U4, U6) |
| F34 | P1 | X | Normalization strips markdown-significant whitespace (indented code committed by cancel becomes prose) | Exact content preservation with a whitespace scenario set; stripping stays diagnostic-only (KTD7, U2) |
| F35 | P2 | X | KTD3's rationale rejected the wrong API boundary — `Markdown.get_stream` is public | Corrected evidence in Grounding and KTD3; direct append kept on the corrected comparison; pin covers `get_stream` |

## The Codex confirm-fix convergence

After the thirty-five findings were fixed, the revision went through eleven scoped Codex confirm
rounds — each one a fresh read of the pushed branch with live probes against the installed
Textual 8.2.8 and, in the later rounds, executable re-runs of the plan's own fold arithmetic.
Rounds one through ten each returned FAIL with progressively narrower findings; every finding was
fixed on the branch and re-confirmed; round eleven returned PASS with no new findings at commit
`94b2507`. The commit chain: `da304cb` (revision 2) → `9a2c63e` → `3889fd3` → `4f2a6b4` →
`6ad94b2` → `1dab934` → `1aca0e6` → `94ac876` → `23b2d84` → `18802f2` → `94b2507`.

The rounds' most consequential catches, in arc order:

- **Counting defects the initial review missed**: the descendant estimate had to count table
  cells and per-construct containers (a probed three-line 601-column table mounts 1,204
  descendants); reconcile work had to be measured in parser-input bytes (append's reparse window
  grows with the unfinished block); wrapped-row estimates had to use display cells, never
  `len()` (37,000 double-width characters paint 949 rows where a character count says 475).
- **The wrapping fallback was itself a cap violation**: a single wrapping fallback widget paints
  1,283 rows for the 100,000-character line, and pre-split hard-wrap fragments break the
  reconstruction — which forced the non-wrapping one-widget-per-projected-line fallback and,
  because a clipped row genuinely hides its tail, produced amendment RA3.
- **The banner budget was arithmetically unsound twice over**: paying one banner per fallen-back
  entry from a fixed 100-widget margin fails at 302 one-line entries (602 widgets in a window
  whose 301 projected lines never trigger a lines-only fold), so banner rows are now charged to
  the fold arithmetic itself; and the charge rule then needed an odd-cut definition (a partially
  retained entry keeps exactly one banner; a cut retaining zero content rows rounds forward — a
  banner never stands alone), with both arms pinned as regressions after Codex showed an
  always-round-forward implementation would pass the first fixture alone.

Every round's pane reply is archived verbatim in the machine-local saga evidence
(never committed), cited here by sha256:

| Round | Verdict | sha256 |
| --- | --- | --- |
| 1 | FAIL (7) | `db170aa2bc6f5b72079eecc4730a877ad31dab7baabfca1f5f7b72fa5123d7f2` |
| 2 | FAIL | `a0f58cefa036540be4c935e08707d11a77b5affbf4a43a0ddb15b1bfe013967d` |
| 3 | FAIL | `37b5f4073dc161e3cf30436c20f54572e0d4130ddfe6c293f7c87ce9e7b89c66` |
| 4 | FAIL | `542901585122ad68eafb9af542cc9077eae0d39a7a85edad46d66dcb105b84a1` |
| 5 | FAIL | `d41a90e5201556a4d8aaaa65bb3b1b566eb06874877dc08997d2ada3ad52e4ed` |
| 6 | FAIL | `06efac235fe77631b28d06bc90c07eff908e4a163801f6467b938e2f5a2b1b16` |
| 7 | FAIL (RA3 propagation) | `316dfa2e0be1781c35cbb5b91d630323558d3c178e1f99571d94fd3d0860e348` |
| 8 | FAIL (banner budget counterexample) | `743f7e6b0a7600f34141b60c9fc39263bfbd55669e2281fcfbc1650750a6bd19` |
| 9 | FAIL (odd-cut undefined) | `c2fba485804c3536db3865c935a63a8c201d5fa7dd92c8f7eeae5a26af542d42` |
| 10 | FAIL (partial-retention arm unpinned) | `22edf44b661ecd138efc8580a79f7cb226cac9001e6afb1b2d7cbbe951423588` |
| 11 | **PASS** | `7ed314966154d35623169f931b440a9ed61bc188d0a6e00364d6122bdbdb3527` |

## What the review verified as true

The citation lens confirmed every behavioral claim in the Grounding section against the source and
the installed Textual 8.2.8 — the plan's factual base was sound; its defects were decisions left
open and coverage gaps, which is exactly the failure mode a doc-review exists to catch before
autonomous execution invents the answers. The coverage lens confirmed the dependency order, the
concurrency cap of three, and homes for all seven acceptance examples. Codex's regression run of
the adjacent suites (47 tests) was green, confirming the findings were plan gaps, not baseline
failures.

## Residual risk

- The sideband timeline (F30's fix) is new replay machinery specified at the plan level; its
  design is bounded to two action kinds, but it is the revision's largest new surface and CR6
  reviews it against exactly that boundary.
- RA1, RA2, and RA3 are judgment calls made under review evidence; all three are recorded in the
  plan's Requirements Amendments section and mirrored to DECISIONS.md by U1, and any can be
  vetoed before the build gate opens. The operator accepted RA1 and RA2 during the review; RA3
  arose later, in the confirm rounds, and awaits the operator's accept-or-veto at the build gate.
- The fold arithmetic (accounted rows, the banner charge rule, the odd-cut arms) is the
  revision's subtlest surface — it survived executable re-runs in confirm rounds nine through
  eleven, and its regressions (aggregate-ceiling, odd-cut, partial-retention) exist precisely
  because plausible implementations pass without them.
