# ADR-0004: Talaria is a Python client

Status: `accepted`
Date: 2026-08-02
Deciders: operator
Affected components: language, toolchain, packaging and installation, continuous integration, every source file

## Context

Four analysis documents compared candidate stacks. Their recommendations moved — TypeScript with
OpenTUI, then Python with Textual — and the reason they moved is that the weighting changed, not that
the evidence did. All four are preserved in `docs/analysis/` as provenance.

The reconsidered analysis introduced four constraints that had not previously been weighted:

1. Talaria is greenfield, so migrating its current implementation is not a real cost.
2. It will be built predominantly by coding agents, so framework legibility and the quality of the
   automated verification loop matter more than they would for a small team writing by hand.
3. The surrounding Infiquetra repositories are predominantly Python.
4. Hermes core is Python, which makes Python the lowest-friction language for shared schemas,
   fixtures, and diagnostics.

**Three of those four are arguments about the language, not about any particular presentation
framework.** That exposed an incoherence in the recommendation as first written: it selected Python
with Textual, but named Go with Bubble Tea as the fallback if Textual failed — which would have
discarded all three language arguments in response to a framework failure. Either the alignment
constraints are load-bearing, in which case a framework failure should not change the language, or
they are not, in which case they should not have selected the language in the first place.

The reconsidered analysis left one question open as the tiebreaker, and recorded it as the empty
seventh item on the operator's constraint list:

> Must Talaria be a small, independently copyable native executable that runs without Python, `uv`, an
> installer, or extracted runtime files?

**The operator answered no on 2026-08-02.** That removes Go's only advantage that Python cannot
match. Talaria's audience is developers already running Hermes, Homebrew, `uv`, and agent
command-line tools, and an ordinary installed command is sufficient for them.

The current TypeScript source is 866 lines across 8 files, of which 85 lines are the Ink shell. It is
repository-bootstrap code and carries no weight in this decision.

## Decision

**Talaria is written in Python.**

This decision covers the language, the toolchain, and the installation contract. It does **not**
select the presentation layer; that remains subject to the validation gate recorded in the
engineering journal, and Textual is the first candidate through it.

The following ship from the first Python commit rather than being added later:

- complete type hints on public and domain interfaces, with a strict type checker in the check run;
- `ruff` for formatting and linting;
- typed models — dataclasses or Pydantic — for protocol messages at the wire boundary, with no
  untyped dictionaries crossing the normalized-event boundary;
- explicit handling of unknown event variants, so an unrecognized event is surfaced rather than
  silently dropped;
- fixture-driven tests covering duplicate, late, missing, reordered, and malformed events.

These gates are heavier than they would be for hand-written Python. That is deliberate: the code is
predominantly agent-authored, and these checks move the most consequential failures into a fast,
machine-visible loop.

Distribution is an ordinary console-script entry point installed with `uv tool install`, producing a
`talaria` command on the user's path. A bundled single-file artifact is not a goal.

**The fallback, if the presentation layer fails its gate, is another Python path — not another
language.** This follows directly from the constraints above: a framework failure is not evidence
against the language that selected it.

## Rejected alternatives

**Go with Bubble Tea v2.** Genuinely strong: a current cell-diff renderer, negotiated synchronized
output, a clean concurrency model for several long-lived transports, and the simplest cross-platform
release story in the candidate set. Rejected because its distinguishing advantage over Python is the
small self-contained executable, which the operator has now declared not to be a requirement, and
because selecting it would trade away alignment with every surrounding repository and with Hermes
core to buy something the product does not need.

**TypeScript with OpenTUI.** The strongest candidate under the earlier weighting, which valued
preserving Talaria's existing TypeScript. That weighting was wrong for a greenfield repository whose
implementation is 866 lines of bootstrap. Set against a pre-1.0 API, a Bun-first operational path,
platform-specific native packages, and a higher risk of agents reproducing stale APIs, the remaining
case does not survive.

**Rust with ratatui.** Retains the clearest low-level correctness contract in the set — an explicit
cell buffer, a diff model, and an in-memory test backend. Rejected because it leaves the most
application infrastructure to Talaria, requires maintainer comfort with Rust async and terminal
internals that this project has not established, and answers a question — exact cell-buffer equality
as a release gate — that Talaria has not asked.

**TypeScript with Ink.** Rejected as the product foundation. The late evidence pass corrected several
stale claims about it: Ink 7.1.1 does support alternate-screen rendering, synchronized-update
envelopes, incremental line rendering, and bracketed paste. It is rejected anyway, because Talaria
would still own transcript virtualization, compound widgets, and a stronger replay test surface, and
because its TypeScript alignment now has negative organizational value.

## Consequences

**Easier.** One toolchain — `uv`, `ruff`, a strict type checker, `pytest` — shared with the
surrounding repositories. Protocol fixtures and diagnostics can be compared directly against Hermes's
canonical Python behavior. Less context switching for maintainers and for agents. Installation is a
single documented command with no build step for the user.

**Harder.** The presentation layer must own transcript virtualization explicitly, since no candidate
supplies unbounded rich-widget scrolling for free. Streaming updates must be coalesced at a frame
boundary rather than rendered per token. Dynamic typing means the gates above are load-bearing rather
than stylistic. Distribution ships a managed runtime rather than a binary.

**Newly harder, and a direct consequence of this decision: there is no evaluated Python fallback.**
Every analysis in the chain treated Textual as the only Python candidate. If Textual fails its gate
on transcript cost or pseudo-terminal correctness, the alternatives within Python have not been
assessed by anyone. Identifying and assessing at least one is now a prerequisite of the gate rather
than a contingency after it.

**Stale as a result.** The TypeScript source tree, `package.json`, `tsconfig.json`,
`vitest.config.ts`, and every reference to `npm run check` as the project's check command. These
remain in place only until the Python tree replaces them; no new behavior is added to them. The
redaction rules and the frame-log format are contracts rather than code and are re-encoded rather
than re-derived — the redaction rule set in particular was corrected against a running Hermes
instance and that correction must survive the language change.

## Revisit when

A small, self-contained native executable becomes a genuine product requirement — for example if
Talaria is to be distributed to users who do not already have a managed Python toolchain. That is the
condition the operator answered no to on 2026-08-02, and it is the only one that would reopen the
language.

Also revisit if Textual fails its gate and no Python alternative clears it either. That would be
evidence that the presentation requirement, not the alignment argument, should be deciding the
language.

## Open, and deliberately not decided here

The presentation layer. Textual 8.2.8 is the first candidate through the validation gate recorded in
the engineering journal; the gate's result, the validated version, the supported Python window, and
the transcript strategy belong in a subsequent ADR.

The supported platform matrix. Nothing in the project has yet stated which operating systems and
architectures are supported, and the installation contract cannot be fully specified without it.
