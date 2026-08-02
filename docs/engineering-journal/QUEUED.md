# Queued Work - talaria

> Future work with priority, rough effort, and worth-it-when triggers.

## P0

### Prove the Hermes transport seam

**Author.** Project bootstrap
**Priority.** P0
**Effort.** Medium
**Worth it when.** The prototype shell is ready for the first real integration slice.
**Context.** Talaria needs capability discovery, session lifecycle, prompt streaming, cancellation, and approval handling before UI work can be judged against real Hermes behavior.
**Refs.** [Project direction](../analysis/2026-08-01-hermes-tui-project-direction.md)

## P1

### Build the stable screen model

**Author.** Project bootstrap
**Priority.** P1
**Effort.** Medium
**Worth it when.** Transport fixtures exist and event/state transitions can be tested independently of a terminal.
**Context.** Stable rendering and reduced flicker depend on state-driven rendering rather than direct callback-driven writes.

### Add the sub-agent monitor

**Author.** Project bootstrap
**Priority.** P1
**Effort.** Medium
**Worth it when.** The gateway advertises enough delegation or spawn-tree state to make the pane meaningful.
**Context.** Sub-agent visibility is a primary UX goal.

## P2

### Add MoA progress and fallback rendering

**Author.** Project bootstrap
**Priority.** P2
**Effort.** Medium
**Worth it when.** The first transport path is proven and MoA event capability differences are captured in fixtures.

### Add a deterministic Kanban adapter

**Author.** Project bootstrap
**Priority.** P2
**Effort.** Medium to large
**Worth it when.** The board contract and ownership boundary are clear enough to avoid taking accidental responsibility for dispatcher internals.

## P3

### Desktop-like configuration views

**Author.** Project bootstrap
**Priority.** P3
**Effort.** Large
**Worth it when.** The core session workflow is reliable and the gateway configuration methods are capability-described.

## Maybe

### Package Talaria as an independently installable distribution

**Author.** Project bootstrap
**Priority.** Maybe
**Effort.** Medium
**Worth it when.** The client can launch against both local and remote Hermes instances with a stable compatibility story.
