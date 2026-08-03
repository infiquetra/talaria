# ADR-0005: Textual is Talaria's presentation layer

Status: `accepted`
Date: 2026-08-03
Deciders: operator
Affected components: `talaria/ui/`, the composer, the transcript pane, the status region, the test
strategy for every screen-facing surface

_Drafted by unit U5 of the [v0.1 prototype plan](../../../docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md)
on a pass verdict, as the plan and DECISIONS.md's revisit-when both require. Accepted by the operator
on 2026-08-03, on the third gate run of that day — see the Context section for why the first two do
not support this decision._

## Context

ADR-0004 settled the language as Python and deliberately left the terminal framework open. It did
more than defer: it recorded that every analysis in the chain had treated Textual as the only Python
candidate, so the framework had been _assumed_ rather than chosen, and it made a measured validation
gate a precondition of committing to one. The plan's KTD12 named `prompt_toolkit` as the fallback,
and U4 assessed it against the same criteria before any measurement was taken — so a failure had
somewhere to go that was not improvisation.

The gate ran three times on 2026-08-03, and only the third run supports this decision.

The first reported `pass` and was worthless: four of its seven measurements were structurally
incapable of failing, and the decisive one compared the projection against itself, so an interface
rendering _nothing at all_ passed with zero content loss. The repaired gate — thirteen checks, each
verified to be capable of failing — returned `fail` on three real defects, all of them in Talaria's
own reconciliation code rather than in the framework. The third run, after those fixes, passed all
thirteen. Its measurements, corpus identities, exercised matrix, the full three-verdict sequence,
and explicit non-claims are in
[Textual validation gate results](../../../docs/analysis/2026-08-03-textual-validation-gate-results.md).

This history is in the ADR rather than only in the results document because it is the reason to
believe the number. A gate that has never failed is not evidence that the thing it measures is
sound; it is evidence about the gate. This one has now failed on real defects and passed after they
were fixed, which is the only sequence that makes the `pass` mean anything.

Two design points that were open questions in ADR-0002 and ADR-0004 also got answers, and they are
the substance of this decision rather than the headline:

- ADR-0002 left the view-model shape open and assigned it to "the first vertical slice's re-render
  cost evidence." U3 chose immutable snapshots with an explicit `changed` set because it had to ship
  the projection first; U5 measured the cost of that choice. It is not the bottleneck. At maximum
  replay speed the domain reducer folds 53,516 frames in 2.3 seconds, allocating one snapshot per
  flush rather than per frame, because rendering is decoupled onto a coalescing tick.
- ADR-0004 warned that the presentation layer would have to own transcript virtualization
  explicitly, since no candidate supplies unbounded rich-widget scrolling. That turned out to be
  true, and cheaper in lines than in care: about 120 lines of diff-and-condense in
  `talaria/ui/transcript.py`, holding 501 mounted widgets across a corpus that produced 4,454 lines
  — and the site of all three defects the repaired gate found. Owning virtualization explicitly
  means owning its correctness explicitly too, which the ADR-0004 warning did not spell out.

## Decision

**Textual 8.2.8 is Talaria's presentation layer for v0.1.** Concretely:

1. `talaria/ui/` is the only package permitted to import a terminal framework (ADR-0002 unchanged).
   Every widget consumes projection view models and holds presentation state exclusively.
2. Streaming renders on a ~50ms coalescing tick, not per token (KTD14). The domain reducer runs at
   frame rate; the screen does not.
3. The transcript mounts at most 500 line widgets plus one condensed block, at every instant and
   not merely once an update settles. Everything above the cap is condensed _before_ it is mounted,
   never after.
4. The domain publishes the boundary between settled and provisional transcript lines
   (`TranscriptView.committed_lines`); the renderer never infers it from two snapshots agreeing.
   A renderer that infers it desynchronizes the first time an entry commits mid-stream.
5. The composer is Textual's `TextArea` configured as a plain-text chat editor — `language=None`,
   `soft_wrap=True`, `show_line_numbers=False` — with Enter bound to submit and Ctrl+J to newline
   (KTD4).
6. Screen-facing behaviour is tested through `run_test()` and `Pilot`, headless, in the ordinary
   `pytest` run. There is no separate UI test tier.
7. Untrusted text reaches the screen only through `talaria.ui.literal.literal_text`, which bypasses
   Rich's markup parser and replaces obeyable control characters with visible stand-ins.

## Rejected alternatives

**`prompt_toolkit`.** Assessed in U4 to plausibility depth against the same five criteria the gate
measures, and found plausible on all five. It stays the recorded fallback and nothing about it has
been disproved. It is not selected because the gate it would have replaced was passed: switching now
would trade a measured framework for an assessed one.

**`urwid`.** Recorded in U4 as the secondary candidate. Not assessed further; nothing about this
decision changes that.

**Deferring the choice further.** Rejected because the deferral had a purpose that is now served.
ADR-0004 deferred in order to buy evidence, and the evidence exists; carrying an open framework
question past the point where it has been measured only makes every subsequent unit conditional.

## Consequences

**Easier.** Layout, focus, scrolling, bindings, and a headless test driver come from the framework.
The Pilot suites run in the same `pytest` invocation as the domain tests, on macOS 3.12 and 3.13 in
CI, with no terminal attached — which is what makes screen behaviour a normal part of the check
pipeline rather than a manual pass.

**Harder.** Subclassing `App` means sharing a namespace with a large private framework surface, and
a collision does not present as a collision. Two names in U5 collided (`_flush`, `_closing`); the
`_closing` one made every Pilot test hang at teardown with a traceback that named nothing in this
repository. `tests/ui/test_app_shadowing.py` now fails the build on any new collision. Expect the
same class of trap when adding screens, and expect Textual's event-delivery details to matter — a
`Paste` posted to a widget rather than to the app inserts twice.

**Pinned, and deliberately so.** `textual>=8.2.8,<9` in `pyproject.toml`. The gate measured 8.2.8;
a major-version move invalidates the measurement rather than merely risking it, and re-running
`talaria gate` is the cheap way to re-establish it.

**Unchanged.** The domain core still imports nothing from this package or any framework, and
`tests/domain/test_boundary.py` still fails the domain run if it ever does. This decision buys a
renderer, not a coupling.

## Revisit when

- Textual 9 is adopted, or the pin is widened for any reason. Re-run `talaria gate` and record the
  numbers alongside the old ones; that is a measurement, not a discussion.
- A real terminal host or tmux exercises a surface the headless gate could not — bracketed paste,
  IME composition, or wide-character measurement through a live emulator. The gate explicitly does
  not claim these, and U10's acceptance run is where they get evidence.
- The recorded steady-state memory slope (0.23 MB per 1,000 frames, from an unevicted domain
  transcript — about 232 MB extrapolated to a million-frame session, inside the 300 MB ceiling but
  not comfortably) starts to matter in a real session. That is a transcript-eviction decision rather
  than a framework one, and QUEUED.md already carries it.
