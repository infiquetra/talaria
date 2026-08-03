# ADR-0005: Textual is Talaria's presentation layer

Status: `proposed`
Date: 2026-08-03
Deciders: operator
Affected components: `talaria/ui/`, the composer, the transcript pane, the status region, the test
strategy for every screen-facing surface

_Drafted by unit U5 of the [v0.1 prototype plan](../../../docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md)
on a pass verdict, as the plan and DECISIONS.md's revisit-when both require. It is `proposed` rather
than `accepted` because acceptance is the operator's, not the executor's._

## Context

ADR-0004 settled the language as Python and deliberately left the terminal framework open. It did
more than defer: it recorded that every analysis in the chain had treated Textual as the only Python
candidate, so the framework had been _assumed_ rather than chosen, and it made a measured validation
gate a precondition of committing to one. The plan's KTD12 named `prompt_toolkit` as the fallback,
and U4 assessed it against the same criteria before any measurement was taken — so a failure had
somewhere to go that was not improvisation.

The gate ran on 2026-08-03 and passed all ten threshold checks. Its measurements, corpus identities,
exercised matrix, and explicit non-claims are in
[Textual validation gate results](../../../docs/analysis/2026-08-03-textual-validation-gate-results.md).

Two design points that were open questions in ADR-0002 and ADR-0004 also got answers, and they are
the substance of this decision rather than the headline:

- ADR-0002 left the view-model shape open and assigned it to "the first vertical slice's re-render
  cost evidence." U3 chose immutable snapshots with an explicit `changed` set because it had to ship
  the projection first; U5 measured the cost of that choice. It is not the bottleneck. At maximum
  replay speed the domain reducer folds 53,516 frames in 1.8 seconds, allocating one snapshot per
  flush rather than per frame, because rendering is decoupled onto a coalescing tick.
- ADR-0004 warned that the presentation layer would have to own transcript virtualization
  explicitly, since no candidate supplies unbounded rich-widget scrolling. That turned out to be
  true and cheap: about 120 lines of diff-and-condense in `talaria/ui/transcript.py`, holding a
  steady 501 mounted widgets across a corpus that produced 4,454 lines.

## Decision

**Textual 8.2.8 is Talaria's presentation layer for v0.1.** Concretely:

1. `talaria/ui/` is the only package permitted to import a terminal framework (ADR-0002 unchanged).
   Every widget consumes projection view models and holds presentation state exclusively.
2. Streaming renders on a ~50ms coalescing tick, not per token (KTD14). The domain reducer runs at
   frame rate; the screen does not.
3. The transcript mounts at most 500 line widgets plus one condensed block, and a backlog larger
   than the cap is condensed _before_ it is mounted rather than after.
4. The composer is Textual's `TextArea` configured as a plain-text chat editor — `language=None`,
   `soft_wrap=True`, `show_line_numbers=False` — with Enter bound to submit and Ctrl+J to newline
   (KTD4).
5. Screen-facing behaviour is tested through `run_test()` and `Pilot`, headless, in the ordinary
   `pytest` run. There is no separate UI test tier.
6. Untrusted text reaches the screen only through `talaria.ui.literal.literal_text`, which bypasses
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
- The recorded memory slope (0.33 MB per 1,000 frames, from an unevicted domain transcript) starts
  to matter in a real session. That is a transcript-eviction decision rather than a framework one,
  and QUEUED.md already carries it.
