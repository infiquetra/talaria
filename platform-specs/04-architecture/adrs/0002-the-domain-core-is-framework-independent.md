# ADR-0002: The domain core is framework-independent and the terminal UI is a projection

Status: `accepted`
Date: 2026-08-02
Deciders: operator
Affected components: transport, protocol normalization, domain state, record and replay, terminal UI, tests

## Context

Talaria has not selected its presentation layer. The engineering journal records a Textual-first
validation gate, and that gate is the mechanism by which the layer will be chosen.

**A renderer gate is meaningless if domain truth lives inside the renderer being compared.** If the
transcript's state, the reconciliation rules, and the command surface live in widget instances, then
swapping the presentation layer means rewriting the application, and the comparison the gate is meant
to perform cannot be performed at all. This is not a stylistic preference. It is the precondition
that makes the pending decision reversible.

Three independent passes over the candidate stacks reached this requirement separately, without
coordinating on it:

- the independent analysis names it as requirement 3, "separation between protocol state and
  presentation";
- the reconsidered analysis names it as Textual downside 6, "Textual must remain a projection";
- the product ideation's survivor 17 asks for a headless core with the terminal as one client, which
  is unreachable if the terminal owns the state.

There is a second reason, specific to how this project will be built. Talaria will be written
predominantly by coding agents. The default shape of framework-first terminal UI code — state in
components, effects driving transport, protocol handling inside callbacks — is the shape an agent
will produce unless the repository says otherwise. A convention that is not canon and not enforced
will erode within a handful of commits.

## Decision

**Talaria's domain core has no dependency on its UI framework, in either direction of knowledge.**

The pipeline is:

```text
raw gateway frames -> normalized events -> domain state -> render projection
```

Everything to the left of `render projection` is plain code with no import of the presentation
framework: the transport, protocol parsing and validation, normalized event types, reconciliation
rules, domain state and its transitions, the command surface, clocks, and the record and replay path.

The presentation layer consumes immutable view models or explicit state transitions. Framework
callbacks may **request** domain commands; they do not **define** domain truth. Widget instances hold
presentation concerns only — scroll position, focus, selection, layout — never protocol or session
state.

This boundary is enforced by a check, not by intention: the domain packages must fail their own test
run if they import the presentation framework. The specific mechanism is chosen with the language,
but the check ships in the same commit as the first domain module.

## Rejected alternatives

**Let the framework own application state.** This is the standard shape for terminal UI applications,
it is faster to write, and it is what most examples in every candidate framework's documentation
demonstrate. It is rejected because it forecloses two things this project has already committed to:
the validation gate that selects the presentation layer, and the headless core that answers the same
questions as the terminal. Choosing it would mean the framework selection becomes irreversible on the
day the first screen is written.

**A formal ports-and-adapters layering with an interface for every boundary.** Rejected as ceremony
disproportionate to the project's size. The requirement here is one directional constraint — the
domain does not import the framework — not a full hexagonal architecture. Adding interfaces where
there is exactly one implementation would produce indirection without producing options.

**Convention without enforcement.** Rejected because the code is predominantly agent-authored. An
unenforced convention documented in `AGENTS.md` is a convention that survives until the first agent
that has not read it, which in practice means it does not survive.

## Consequences

**Easier.** The presentation-layer gate becomes a real comparison rather than a rewrite. Replay tests
run against domain state with no terminal and no framework, which is the cheapest and fastest test
layer in the project. The headless core stops being a future refactor and becomes a property the code
already has. If the selected framework is later abandoned, the loss is bounded to the projection.

**Harder.** There is more indirection early than a small terminal UI appears to need, and the first
few screens will feel over-engineered relative to their size. Every agent working in this repository
must be told the boundary exists, which means it belongs in `AGENTS.md` and not only here. The
enforcement check is additional infrastructure that must be written before it is needed rather than
after it is missed.

## Revisit when

This is expected to hold for the life of the project; it is the cheapest architectural commitment
available and it costs almost nothing to keep. Revisit only if the enforced boundary is found to
require duplicating a substantial view model on every state change with no corresponding benefit — in
which case the correct response is to narrow where the boundary is drawn, not to remove it.

## Open, and deliberately not decided here

The shape of the view model handed across the boundary — immutable snapshots, explicit transition
events, or a hybrid where the transcript is snapshotted and everything else is transitioned. That is
a question the first vertical slice should answer with evidence about re-render cost, not a question
to settle in advance.
