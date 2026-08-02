# Hermes TUI project direction

Status: `draft`
Authority: `reference`
Date: 2026-08-01

## Purpose

This document records the public-safe outcome of the initial design conversation that led to Talaria. It captures the problem, evidence gathered from the public Hermes source layout, alternatives considered, and the current direction. Runtime model changes, local credentials, private paths, and other session-only details are intentionally excluded.

## The starting problem

The current Hermes terminal experience is powerful but not yet the ideal daily engineering interface for this project. The desired experience has:

- less flicker and more stable rendering;
- a clearer status bar and run-state model;
- a more deliberate composer and workflow surface;
- a separate, useful view of active sub-agents;
- configuration views closer to a desktop application while remaining terminal-native;
- workflows that feel closer to modern coding-agent tools; and
- first-class access to Hermes-specific capabilities such as Mixture of Agents (MoA), delegation, skills, tools, and Kanban.

The constraint is equally important: the result should remain useful as a daily TUI while being structured so valuable changes can become focused upstream Hermes contributions.

## What was learned from Hermes

### The old direct-client path is tightly coupled

The legacy CLI/TUI path calls agent implementation code directly. Building a new client by copying that coupling would make the new project sensitive to internal Python refactors and would recreate the maintenance problem the project is trying to escape.

### The modern TUI has a protocol seam

The Hermes repository contains an Ink/React TUI under `ui-tui/` and a Python `tui_gateway/` that exposes a JSON-RPC surface over local stdio and WebSocket transports. The existing TUI launches the gateway and communicates through that protocol rather than rendering the agent loop itself.

That seam is the most useful starting point for a new client. It already covers Hermes-native operations such as session lifecycle, prompt submission, approvals, configuration, tools, skills, commands, and delegation-related state.

### Hermes also exposes an external API

The API server provides a more stable boundary for an independently installed client. The relevant public surfaces include:

- OpenAI-compatible chat completions;
- asynchronous runs and streamed run events;
- persistent sessions and history;
- approvals and cancellation;
- capability discovery;
- model/provider options; and
- skills and toolsets.

The OpenAI-compatible endpoint alone is not enough for a full Hermes TUI, but the broader run/session API is a useful primary transport.

### MoA execution and MoA observability are different

Hermes models Mixture of Agents as a virtual provider/preset. A client using the API can request MoA execution. The richer Hermes-native TUI protocol also contains MoA-specific progress concepts, such as advisor references, phases, progress, and aggregation.

Therefore, API-only does not necessarily lose MoA execution, but it can lose the detailed progress experience. Talaria should preserve that distinction in its architecture rather than claiming that a generic chat stream is equivalent to full MoA telemetry.

### Kanban is a control-plane surface, not ordinary chat

Hermes exposes Kanban operations as tools and has a human-facing Kanban CLI, but the API route surface is not the same thing as a deterministic board API. A model deciding to call a Kanban tool is not a substitute for a TUI reading and mutating board state through typed operations.

Talaria should use a dedicated Kanban adapter. A local CLI adapter may be the first implementation; a structured gateway or HTTP adapter could follow if the upstream boundary supports it. Direct SQLite ownership should be avoided unless the project explicitly takes responsibility for dispatcher and concurrency semantics.

## Alternatives considered

### 1. Build only against `/v1/chat/completions`

**Strengths:** simple, portable, and easy to point at local or remote Hermes instances.

**Problems:** insufficient access to rich lifecycle state, MoA progress, delegation topology, configuration operations, and deterministic Kanban behavior.

**Disposition:** useful as one transport, not sufficient as the whole architecture.

### 2. Fork the existing Hermes TUI wholesale

**Strengths:** fastest access to an already functional UI, existing types, and gateway integration.

**Problems:** the current TUI is large and carries rendering and UX complexity that may make flicker and workflow changes harder to isolate. A wholesale fork also makes it easy to drift from upstream without clarifying which changes are meant to be contributed back.

**Disposition:** reuse protocol knowledge and selected components where valuable, but do not make a blind copy the product architecture.

### 3. Build a thin, fresh client against Hermes contracts

**Strengths:** clearer ownership, smaller experiments, explicit capability discovery, easier tests, and a natural path to focused upstream PRs.

**Problems:** more initial integration work and the need to recreate UI behavior that the current TUI already has.

**Disposition:** selected. This is the Talaria direction.

## Settled direction

Talaria will be **API-first, but not API-only**:

```text
                    Talaria
                       │
          transport and capability layer
                       │
     ┌─────────────────┼─────────────────┐
     │                 │                 │
 Hermes API       TUI gateway       Kanban adapter
 runs/sessions   rich Hermes state  typed board access
```

### Hermes API transport

Use the API for:

- normal conversations;
- streamed responses;
- persistent sessions and history;
- approvals;
- cancellation;
- model/provider selection;
- remote Hermes connections; and
- version/capability discovery.

### Hermes gateway transport

Use the gateway when available for:

- detailed MoA progress;
- delegation and spawn-tree visualization;
- richer sub-agent controls;
- slash-command dispatch; and
- Hermes-specific configuration operations.

The gateway must remain behind an adapter so the rest of the UI does not depend on its transport details.

### Kanban adapter

Expose board state and mutations through explicit typed operations. Do not make the board pane depend on the model's willingness to issue a tool call.

## Development and contribution model

Talaria is a public repository under Infiquetra, named for Hermes's winged sandals. The name is intended to be memorable while keeping the relationship to Hermes clear. It is not presented as an official Hermes distribution.

The local workflow should support three states:

1. **Official Hermes:** the installed Hermes TUI remains untouched and launchable.
2. **Talaria UI against official Hermes:** a built Talaria TUI can be selected through Hermes's external TUI bundle override. This is the preferred compatibility mode for UI-only work.
3. **Full Talaria/Hermes development:** a separate Talaria checkout and Python environment run the Talaria source UI with the matching development gateway/core when protocol or core behavior changes are required.

This separation keeps the installed version from being overwritten and makes the distinction between UI changes and protocol changes explicit.

The concrete launcher distinction is:

```text
HERMES_TUI_DIR=<talaria>/ui-tui hermes --tui
    built Talaria UI, official Hermes Python/gateway

HERMES_HOME=<isolated-home> <talaria-venv>/bin/hermes --tui --dev
    Talaria source UI, matching Talaria development gateway/core
```

`--dev` and `HERMES_TUI_DIR` are separate modes in the current Hermes launcher. The first mode is the compatibility path for UI-only changes; the second is the source path for protocol or core changes. `HERMES_HOME` isolates sessions and configuration when the development core must not touch the normal Hermes state.

## Proposed implementation sequence

### Phase 0: repository and runtime proof

- Keep the repository standards and public-safe documentation in place.
- Make the minimal TUI shell build, run, and test.
- Establish a command that launches Talaria without shadowing the official `hermes` executable.

### Phase 1: protocol discovery

- Implement transport interfaces for the Hermes API and gateway.
- Add capability/version discovery.
- Add fixture-driven event parsing and graceful unknown-event handling.
- Verify session creation, resume, prompt submission, streaming, cancellation, and approvals.

### Phase 2: core workflow UI

- Build a stable screen model rather than rendering directly from transport callbacks.
- Add a composer, transcript, status bar, and explicit busy/approval states.
- Add keyboard behavior and terminal resize tests.

### Phase 3: agent and model visibility

- Add the sub-agent monitor pane and spawn-tree state.
- Add model/provider selection.
- Add MoA progress when the gateway advertises the relevant capability, with a useful API fallback.

### Phase 4: Hermes control plane

- Add skills, tools, commands, and configuration views through adapters.
- Add a deterministic Kanban adapter.
- Define fallback behavior when an optional capability is unavailable.

### Phase 5: upstream contribution

- Separate generally useful changes from Talaria-specific experiments.
- Add focused tests and public documentation.
- Open small upstream Hermes PRs at the narrowest useful boundary.

## Acceptance criteria for the first real milestone

The first useful milestone is not feature parity. It is a trustworthy loop:

- Talaria installs and runs independently.
- The official Hermes TUI remains launchable without changing files in the official installation.
- Talaria can discover the backend capabilities it is connected to.
- A user can create/resume a session, submit a prompt, observe streamed output, and exit safely.
- Unknown or unsupported events do not crash the UI.
- The screen is driven by testable state transitions rather than transport callbacks directly mutating terminal output.
- The repository's documented checks pass.

## Open questions

These remain design questions rather than hidden assumptions:

1. Which API and gateway capabilities are stable enough to treat as versioned Talaria contracts?
2. Should the first remote mode use the API server exclusively, with the gateway reserved for local mode?
3. Which subset of the current Hermes TUI's protocol types should be reused, regenerated, or redefined?
4. What is the smallest Kanban adapter that is deterministic without taking ownership of the board database?
5. Which improvements belong in upstream Hermes directly, and which are Talaria-specific experiments?

## Public-safe boundary

This summary intentionally omits private machine paths, credentials, internal hostnames, private operational details, and session-only routing information. It records only the project reasoning and public Hermes integration facts necessary to understand Talaria.
