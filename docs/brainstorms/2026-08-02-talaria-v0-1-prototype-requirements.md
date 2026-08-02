---
date: 2026-08-02
topic: talaria-v0-1-prototype
maturity: requirements-ready
source: docs/ideation/2026-08-02-talaria-product-shape-ideation.md — survivors 5, 14, 17, 18, 19, 28
---

# Talaria v0.1 prototype — requirements

## Summary

Talaria v0.1 runs one Hermes session end to end in Python: you type, it streams, you answer whatever
it asks of you, you can stop it. Delegated sub-agents stay visible while the parent session keeps
talking. It is built replay-first, so the terminal framework is judged before any transport work
depends on the answer.

This document fixes the product and safety contract. It is input to `/plan`, not direct authorization
for `/work`: the implementation plan must close every planning obligation below and preserve every
requirement-to-verification mapping before source work begins.

## Problem Frame

The operator already runs Hermes's terminal UI daily and describes it as "not completely awful" —
the complaint is specific, not general. Two pain points were named directly. The composer has no
visual boundary, so operator input begins wherever the transcript happened to stop; this is verified
at Hermes `7f4d15515`, where the composer component and the layout that places it contain no border
declarations at all. And the status rule takes twenty fixed fields, so it can never show anything its
author did not anticipate — no pull-request state, no cloud profile, no language environment.

Underneath both sits a larger gap. Hermes's own status surface is a fixed component, and its agents
view is modal: opening it replaces the transcript. An operator who wants to know what delegated work
is doing must leave the conversation to find out. The strongest available evidence that this matters
is that the operator already wrote a multi-row status line for a different agent harness whose one
irreplaceable field is a marker showing which sub-agent is active — written, by its own comment,
because nothing else in that interface says you are inside one. Sub-agent visibility is the gap
somebody has already spent real effort working around.

The cost of doing nothing is not that the work becomes impossible. It is that delegated work fails
quietly. A blocked sub-agent waits to be discovered rather than announcing itself, and discovery
happens whenever the operator next thinks to look.

## Key Decisions

**v0.1 is a daily driver, not a demonstrator.** The target is that the operator does real Hermes work
in Talaria rather than keeping it beside the tool they actually use. This is the decision every other
one below answers to, and it sets a hard floor: any surface whose absence hangs a session is in scope
regardless of cost.

**One session in view; the session-registry surface is deferred.** v0.1 shows a single session, the
way Hermes does today. The gateway registers twenty-one `session.*` methods including `session.list`
and `session.active_list`, so a registry is a rendering choice available whenever it is wanted rather
than a capability that has to be sourced from elsewhere. This is a surface deferral, not permission
to hard-code a singleton below the projection boundary; deferring it must foreclose nothing. This
supersedes ideation survivor 5 for v0.1 only.

**The sub-agent surface is in v0.1, and it is deliberately small.** Without it, v0.1 is Hermes with a
border, and a version the operator does not want to open is a version that never gets a successor.
It ships as rows inside the focused session and a count — not a designed pane — because unlike the
rest of v0.1 there is nothing to re-encode: Hermes's equivalent is modal, so this part is invention
rather than translation, and invention is where estimates fail.

**Built replay-first.** Milestone 1 drives the entire interface from recorded protocol frames with no
gateway connection; milestone 2 swaps the frame source for a live socket. This makes the terminal
framework validation gate and the prototype the same build rather than two pieces of work, makes
every rendering behavior deterministically testable against identical input, and surfaces a framework
failure before any transport work has been spent on it.

**The status line ships as an external command contract in v0.1.** The operator chose this over
rendering internally, accepting that the payload's shape becomes a v0.1 commitment. The upside is
that it is the strongest available test of ADR-0002: an import check proves the domain core does not
import the terminal framework, but only a second consumer — here, a separate process that may not
even be written in Python — proves the core's state is genuinely usable outside it.

**The recorder is re-encoded in Python before the interface work.** The operator chose this over
continuing to record with the existing TypeScript recorder. It ends the repository's mid-transition
state sooner and makes re-capture cheap when a Hermes upgrade drifts the protocol. It also produces a
first Python commit that is entirely domain-side, below ADR-0002's projection boundary, with no
framework involved — which is the layer ADR-0002 wants built first anyway. The accepted risk is that
redaction is the worst possible place for a re-encode bug, and requirements below convert that risk
into tests rather than leaving it as care.

**Gateway-owned slash commands are gateway-driven, not re-implemented.** `commands.catalog` is
described in Hermes's own source as registry-backed slash metadata for the terminal UI, and
`command.dispatch` takes a name, an argument, and a session. The catalogue also contains a small set
of official-client-local commands that another client cannot assume are dispatchable. Talaria asks
what exists, verifies the callable subset, handles the gateway's structured result shapes generically,
and keeps its own local controls explicit. The count of commands in Hermes's tree is not a measure of
work Talaria has to do.

## Actors

- A1. **The operator** — one person, running Hermes agents against their own repositories, who has
  Hermes's terminal UI available as the alternative at all times.
- A2. **The Hermes terminal gateway** — an authenticated WebSocket attach surface exposed by an
  already-running Hermes process that Talaria dials and does not launch, per ADR-0001. It is the
  source of session state, sub-agent state, the slash catalogue, the approval path, and the four
  blocking bridges.
- A3. **The status line command** — an operator-owned executable, in any language, that receives a
  payload and returns rows to render. It is untrusted and may fail, be slow, or be absent.
- A4. **Delegated sub-agents** — work the focused session spawned, whose state Talaria reads and never
  authors.

## Requirements

**The session loop**

- R1. Talaria connects over the authenticated attach path to a Hermes terminal gateway that is already
  running and never launches one. Attach credentials are acquired through a channel that does not
  place them in command-line arguments, shell history, or process listings.
- R2. Exactly one session is in view. At startup Talaria can create a new session, resume a stored
  human-facing session, or honour an explicit session target; after that selection it offers no
  session switcher in v0.1.
- R3. The operator can submit a prompt and watch the response stream into the transcript.
- R4. The operator can cancel a turn that is in flight, and the transcript shows that it was cancelled
  rather than that it ended. A late completion event cannot overwrite the cancelled terminal
  state.
- R5. An event type Talaria does not recognize is surfaced visibly by type and never silently
  discarded. A malformed frame is surfaced as a protocol error without rendering untrusted raw
  bytes. The gateway defines far more than the shipping client uses, so unknown events are
  expected traffic, not a defect.
- R6. Transcript content renders as readable plain text. Markdown, diff, and reasoning-block
  presentation are out of scope, but their content is never dropped.

**Blocking prompts**

- R7. Talaria completes the gateway approval path and all four blocking bridges without leaving the
  transcript: approval, clarification, secret, sudo, and terminal-read. The four human-facing
  prompts are answered in place; terminal-read returns the serialized terminal view required by
  the gateway contract.
- R8. An unanswered human-facing prompt is visible and keyed by its gateway `request_id` for as long
  as it is outstanding. An expiry clears the active control but leaves a persistent transcript
  indication; a late response cannot be attached to a different request. A session waiting on the
  operator never looks like a session that is working.
- R9. Credential-bearing or potentially sensitive response values are never written to a Talaria-side
  frame log, transcript export, diagnostic record, or status-command payload. This includes
  `sudo.respond` passwords, `secret.respond` values, `clarify.respond` answers,
  `terminal.read.respond` text, attach-URL credentials, and credential-shaped fields on methods
  the deny-set does not know. Attach credentials also stay out of command-line arguments, shell
  history, and process listings. A redacted frame retains an explicit redaction marker, and a
  response is sent only for its matching session and `request_id`.

**The composer**

- R10. The composer occupies a bounded, visibly delimited region. Where operator input begins and ends
  is unambiguous while the transcript above it is streaming.
- R11. Text editing is provided by the terminal framework rather than hand-written. Correct behavior
  under bracketed paste, wide and combining characters, and operating-system input methods is a
  framework property Talaria verifies, not a component Talaria authors.
- R12. The composer accepts multi-line input, and the distinction between submitting and inserting a
  newline is explicit and discoverable.
- R13. A large paste is collapsed through the gateway rather than inlined: Talaria sends the text,
  receives a placeholder and a path, and inserts the placeholder. The pasted content is not
  re-rendered into the transcript. If collapse is unavailable or fails, the original text remains
  in the composer, no partial prompt is submitted, and the missing capability is visible.

**Sub-agent visibility**

- R14. Delegated sub-agents of the focused session are visible while the parent session is still
  streaming. Viewing them never replaces the transcript.
- R15. A sub-agent can be interrupted without leaving the conversation.
- R16. When sub-agent detail is collapsed, a count remains visible, so the operator always knows
  whether delegated work exists.
- R17. Talaria reads sub-agent state and never authors it.

**The status line**

- R18. Talaria runs an operator-supplied executable on an interval, without interpolating session data
  into a shell command, hands it a payload describing current session state, and renders the
  newline-separated rows it returns.
- R19. The payload carries a version identifier from its first commit, because an external consumer
  depends on its shape. The v1 schema, serialization, delivery mechanism, interval, timeout, and
  output limit are fixed in the implementation plan before the first consumer is written.
- R20. The payload is a projection of domain state, carries no terminal-framework types or
  credential-bearing values, and the child receives a sanitized environment rather than
  Talaria's provider credentials.
- R21. A status command that fails, hangs, overlaps its next interval, produces nothing, emits invalid
  text, exceeds its output limit, or does not exist degrades visibly and never blocks rendering,
  breaks the session loop, or crashes Talaria. At most one invocation is active at a time.
- R22. The command chooses a variable number of rows rather than a fixed constant, within a documented
  viewport-safe bound. Output is rendered as literal text; ANSI and terminal-control sequences
  are never interpreted.

**Slash commands**

- R23. The gateway-owned command set is retrieved from the gateway rather than compiled into Talaria.
  Catalogue entries that depend on an official-client-local handler are not presented as working
  until their dispatch path has been proved; an unsupported entry degrades visibly.
- R24. Gateway-owned commands are dispatched by name and their structured `exec`, `plugin`, `send`,
  `skill`, `alias`, and `prefill` results are handled generically. A bundle uses the `send` result
  shape. Model-facing skill or bundle scaffolding is never rendered when the gateway supplies a
  display projection. No gateway-owned command receives a bespoke interface in v0.1;
  Talaria-local controls are a separate explicit set.

**Recording and replay**

- R25. A Python recorder writes frame logs conforming to the existing format contract at
  `docs/formats/frame-log.md`, which is versioned and declares itself the authority. Each recording
  owns one new file with exactly one header; the recorder never overwrites a corpus or appends a
  second header to an existing one. Create, write, flush, and close failures are surfaced, and a
  failed recording is never reported as successful.
- R26. Every inbound and outbound raw frame passes through redaction before it can reach disk rather
  than being scrubbed afterward. An unparseable payload is recorded as a marked hole while its raw
  bytes are withheld; its bounded diagnostic is categorical and never quotes any part of the raw
  payload.
- R27. The Python recorder's redaction behavior is verified against the cases already covered by the
  TypeScript recorder's suite and re-verified against the method registrations and real key names
  of the Hermes revision being recorded. The tests fail on both under-redaction and destructive
  over-redaction.
- R28. The TypeScript and Python recorders produce contract-equivalent frame logs for equivalent
  receive-only input, which is their shared executable boundary. The TypeScript command is
  receive-only, so outbound equivalence is not claimed: outbound Python recording is proved with
  synthetic credential-bearing frames and an end-to-end send-path test before daily-driver status.
- R29. Recorded corpora stay outside version control. Only small hand-authored synthetic frames are
  committed as fixtures.
- R30. Milestone 1 drives the entire interface from a frame log with no gateway connection present.
- R31. Replacing the frame source with a live socket changes nothing above the transport boundary.

**The architectural boundary**

- R32. No domain module imports the terminal framework, enforced by a check that fails the domain
  package's own test run.
- R33. Strict type checking, linting, and tests ship with the first Python commit rather than being
  added afterward.

**Daily-driver gates**

- R34. Before the interface reports daily-driver ready, Talaria verifies every gateway method required
  by R1 through R31 against a pinned compatibility baseline. The terminal gateway publishes no
  capability endpoint, so startup invokes only methods proved read-only and side-effect-free;
  mutating and request/response methods are verified by pinned source/schema evidence and isolated
  acceptance tests, never called merely to discover whether they exist. A missing or incompatible
  required method is named and blocks the v0.1 daily-driver verdict; graceful degradation is
  allowed only for a surface this document explicitly marks optional.
- R35. Authentication failure, initial connection failure, disconnect, and reconnect are distinct,
  visible states. Reconnect reconciles the focused session and outstanding prompt state without
  duplicating transcript entries, and an RPC whose outcome was lost with the transport is never
  reported as successful.
- R36. Normal exit and failure teardown restore the terminal, stop Talaria-owned child processes, and
  resolve local waiters. Talaria does not terminate the gateway or implicitly interrupt/delete
  the Hermes session it does not own.
- R37. Protocol normalization is deterministic for malformed, duplicate, missing, late, reordered,
  and cross-session events. Events for another session cannot mutate the focused session; a late
  live event cannot overwrite terminal state or resurrect an entity whose start was missed. The
  reconciliation-rule catalogue required by ADR-0003 is complete at a pinned Hermes revision
  before this layer is implemented.
- R38. Streaming updates are coalesced at a deliberate frame boundary, completed transcript content is
  cached, and mounted widgets and memory stay bounded as history grows. Following the bottom and
  reading while scrolled away both preserve their anchors. The validation plan records the corpus,
  thresholds, and measurements; subjective smoothness is not a pass condition.
- R39. A clean `uv tool install` produces an isolated `talaria` command without modifying the official
  Hermes installation. v0.1 claims support only for Python, operating-system, terminal-host, and
  tmux combinations actually exercised and recorded by the validation gate.
- R40. Replay supports pause, resume, and controllable speed. It exercises the external status command
  against replayed domain state, while controls that would send a gateway mutation are visibly
  inert or deterministically simulated and never open a live connection.

## Key Flows

- F1. **First run.** _Trigger:_ the operator launches Talaria with a gateway already running.
  Talaria authenticates, verifies the pinned compatibility surface without invoking a mutating method
  for discovery, applies the documented startup precedence for an explicit target, a stored session,
  or a new session, renders the transcript, starts the status command, and puts the cursor in the
  composer. Authentication failure or an absent required capability is named and Talaria does not
  report ready. **Covers R1, R2, R18, R23, R34.**

- F2. **A turn that needs the operator.** _Trigger:_ the operator submits a prompt and the agent opens
  the approval path or one of the four blocking bridges. A human-facing prompt appears in place and
  remains tied to its request until answered or expired; terminal-read returns through the same
  correlation boundary without creating a human overlay. An expiry remains visible in the transcript,
  and a late answer cannot resolve a newer request. **Covers R3, R7, R8, R9.**

- F3. **A turn that delegates.** _Trigger:_ the focused session spawns sub-agents. Rows for the
  delegated work appear while the parent continues streaming. The operator can interrupt one of them
  and stay in the conversation. A terminal row remains terminal when a late live event arrives, and
  another session's event cannot change the focused rows. **Covers R14, R15, R16, R17, R37.**

- F4. **A large paste.** _Trigger:_ the operator pastes several hundred lines into the composer. The
  framework delivers it as a single paste event, Talaria collapses it through the gateway, and a
  one-line placeholder appears in the composer in place of the text. If collapse fails, the original
  input remains editable and nothing partial is submitted. **Covers R11, R13.**

- F5. **Replay.** _Trigger:_ the operator replays a recorded frame log with no gateway running. The
  full interface and external status projection render from the file, with pause and controllable
  speed, and no network present. A control that would mutate Hermes is visibly inert or simulated.
  **Covers R18, R30, R31, R40.**

- F6. **Transport loss.** _Trigger:_ the authenticated socket closes during a streamed turn or while a
  request is awaiting a response. Talaria marks the disconnect, does not claim an unknown RPC outcome,
  reconnects when possible, and reconciles the focused session, transcript, sub-agents, and outstanding
  prompts without duplication. **Covers R4, R8, R35, R37.**

- F7. **Exit.** _Trigger:_ the operator exits normally or Talaria fails. Talaria restores the terminal,
  stops its status child, releases its own waiters, and leaves the external gateway and Hermes session
  running unless the operator separately requested a gateway action. **Covers R21, R36.**

## Acceptance Examples

- AE1. **When the status command exits non-zero, hangs past its interval, overlaps the next tick,
  produces no output, or violates the planned output contract**, the status region names the failure
  and the session loop continues unaffected. Talaria does not leak credentials to the child, interpret
  its output as terminal control, run two copies concurrently, or retry in a way that compounds the
  delay. **Covers R18, R19, R20, R21, R22.**

- AE2. **When input contains an unknown event, malformed frame, duplicate, reordering, late terminal
  update, missing start, or another session's event**, normalization produces the same deterministic
  state on every replay: unknown types and protocol errors are visible, raw unparseable bytes are not
  rendered or reflected in parser diagnostics, terminal state is not reversed, and the focused session
  is not cross-contaminated.
  **Covers R4, R5, R26, R37.**

- AE3. **When approval, clarification, secret, sudo, or terminal-read traffic crosses the socket while
  recording is active**, no sensitive answer, terminal buffer, attach credential, or credential-shaped
  unknown field appears in a frame log, transcript export, diagnostic record, status payload, or child
  environment; the attach credential is also absent from command-line arguments, shell history, and
  process listings. Each withheld value leaves a marker, and ordinary token-accounting fields survive.
  **Covers R7, R9, R20, R26, R27, R28.**

- AE4. **When the composer receives multiline input, bracketed paste, wide or combining characters,
  and an operating-system input method**, the selected framework widget preserves the text and the
  documented submit-versus-newline binding remains discoverable. **Covers R10, R11, R12.**

- AE5. **When the terminal repeatedly shrinks and grows during a long stream**, the transcript reflows,
  mounted history and memory remain within the plan's measured bounds, following the bottom still
  follows, reading away from the bottom holds its anchor, the composer keeps its boundary, and all
  transcript content remains readable and complete in the plain-text presentation.
  **Covers R6, R10, R38.**

- AE6. **When the same receive-only input is recorded by the TypeScript and Python recorders**, the
  resulting frame logs are contract-equivalent. A separate Python send-path test proves outbound
  recording and redaction; the result does not claim the TypeScript command recorded traffic it never
  sends. Production corpora remain outside version control, and any committed comparison fixtures are
  small and synthetic. **Covers R25, R26, R27, R28, R29.**

- AE7. **When pinned source/schema evidence or a side-effect-free startup check shows one required
  method absent or incompatible**, Talaria names that method and does not show a daily-driver-ready
  state; it never invokes a mutating method as a capability probe. Absence of a method explicitly
  classified as optional degrades only that surface. **Covers R23, R24, R34.**

- AE8. **When the socket closes after an RPC is sent but before its response arrives**, Talaria marks
  the outcome unknown rather than successful. After reconnect, the focused transcript and outstanding
  prompt each appear once. **Covers R8, R35.**

- AE9. **When the catalogue contains ordinary gateway, plugin, skill or bundle, alias, and prefill
  commands plus an official-client-local entry**, Talaria dispatches the callable entries generically,
  handles the six structured result shapes, renders the safe display projection instead of model
  scaffolding, and marks the unproved local entry unsupported. **Covers R23, R24.**

- AE10. **When installed into a clean supported environment and then exited from both normal and
  failure paths**, the `talaria` command launches without changing the official Hermes installation,
  repository checks pass, terminal state is restored, Talaria-owned children stop, and the external
  gateway remains alive. **Covers R32, R33, R36, R39.**

- AE11. **When replay is paused and a mutation control is invoked**, replay does not open a socket or
  mutate Hermes; resuming at another speed continues deterministically from the same corpus.
  **Covers R30, R31, R40.**

- AE12. **When startup is given an explicit session target, a stored session choice, or a request for a
  new session**, the documented precedence selects exactly one and no switcher appears afterward.
  **Covers R2.**

- AE13. **When `paste.collapse` fails**, the complete original paste remains in the composer, no
  placeholder is inserted, and no partial prompt is sent. **Covers R13.**

- AE14. **When a sub-agent reaches a terminal state and a late progress event follows**, its row remains
  terminal; interrupting a live sub-agent does not replace the parent transcript. **Covers R14, R15,
  R16, R17, R37.**

- AE15. **When the recorder cannot create, write, flush, or close its output**, the recording command
  fails visibly and never reports the partial attempt as successful. No unparseable payload fragment
  appears in either the frame log or its diagnostic. **Covers R25, R26.**

- AE16. **When a controlled live socket emits the same ordered raw frames as a replay corpus**, both
  frame sources produce identical normalized domain and view-model transitions above the transport
  boundary. The live path separately meets the plan's measured streaming, backpressure, disconnect,
  and reconnect criteria rather than inheriting replay's timing result. **Covers R31, R35, R37, R38.**

## Requirement Traceability

The implementation plan must assign every row below to concrete units and executable evidence; a
range in this table is compression, not permission to omit an individual requirement.

| Requirements | Primary evidence in this document                              |
| ------------ | -------------------------------------------------------------- |
| R1–R6        | F1, F6; AE2, AE8, AE12                                         |
| R7–R9        | F2; AE3, AE8                                                   |
| R10–R13      | F4; AE4, AE5, AE13                                             |
| R14–R17      | F3; AE14                                                       |
| R18–R22      | F1, F5, F7; AE1, AE3                                           |
| R23–R24      | F1; AE7, AE9                                                   |
| R25–R31      | F5; AE2, AE3, AE6, AE11, AE15, AE16                            |
| R32–R33      | AE10 and the first-Python-commit check gate                    |
| R34–R40      | F1, F3, F5, F6, F7; AE2, AE5, AE7, AE8, AE10, AE11, AE14, AE16 |

## Scope Boundaries

Out of v0.1:

- The session registry and any session switcher. Deferred, not rejected; `session.list` makes it cheap
  later.
- Theme selection and terminal colour detection, despite the colour-detection code being the
  highest-value read in the Hermes tree.
- Every Hermes product overlay except the four human-facing blocking prompts. Terminal-read is handled
  as a gateway bridge without a fifth operator overlay.
- Billing, subscription, virtual pets, journey, and the skills and plugins hubs. These are Hermes
  product surfaces, not terminal-UI capability, and they are outside Talaria's identity rather than
  deferred.
- Markdown, diff, and reasoning-block presentation polish.
- Pane-manager integration. It remains a target to be aware of, not a v0.1 feature, and Talaria never
  re-implements what it does.
- File and image attachment, prompt history cycling, and slash-command completion popups. All cheap,
  all deferred.
- A merged view across multiple sessions or sources.

## Dependencies and Assumptions

- **The standalone attach path is authenticated and dashboard-backed.** ADR-0001 records that the
  WebSocket Talaria needs is exposed by the Hermes dashboard process and rejects unauthenticated
  requests. Startup and reconnect validation must exercise that actual seam; "a gateway is running"
  is not sufficient evidence.
- **No Python alternative to the first-choice terminal framework has been assessed.** Every candidate
  in the analysis chain except one was in another language, so settling the language left the fallback
  set unevaluated. If milestone 1 fails, there is currently nowhere to go. This is tracked as a P0 in
  `docs/engineering-journal/QUEUED.md` and is a prerequisite of the gate rather than a contingency
  after it.
- **The framework's own text widgets are assumed adequate for a chat composer.** The single-line
  widget is too narrow and the multi-line one is built as a code editor with syntax awareness; a chat
  composer is neither. Which one backs R10 through R12 is a planning decision, but milestone 1 must
  stress whichever is chosen, because a failure there has no cheap remedy.
- **The reconciliation-rule catalogue ADR-0003 depends on is incomplete.** Hermes's turn controller has
  not been read at a pinned revision, only its call surface. A rule discovered after the normalization
  layer is written is a defect found late. Tracked as a P1 in the engineering journal.
- **Replay timing is not live timing.** An interface can render a frame log cleanly and still stutter
  against a socket. Milestone 2 verifies this rather than inheriting milestone 1's result.
- **Collapsed pastes are written to Hermes's home directory, not Talaria's.** This is correct rather
  than a boundary violation: the agent must be able to open the path, so Hermes owns it.

## Planning Closure Obligations

These are HOW decisions, not optional questions. `/plan` must resolve each from cited evidence and
record the verification that can falsify it; an implementer must not choose a default ad hoc.

- PC1. Select the framework widget behind R10 through R12, document submit-versus-newline bindings,
  and bind the choice to the composer cases in AE4 and the framework gate.
- PC2. Freeze the status contract required by R19: version-1 fields, serialization and delivery,
  executable invocation, interval, timeout, overlap policy, environment, output limit, and row bound.
- PC3. Define the smallest sub-agent row projection and collapsed count, including the fields and
  terminal-state precedence required by R14 through R17 and R37.
- PC4. Compare the Python shape against `docs/formats/frame-log.md` before implementation and define
  the recorder-equivalence relation used by R28 and AE6, including which run/host observation fields
  are normalized and which sequence, direction, frame, redaction, and parse-error semantics must
  remain equal. If a second implementation requires a contract change, update the authoritative
  format and version first; otherwise record why version 1 is sufficient.
- PC5. Define deterministic startup precedence for explicit target, stored-session resume, and new
  session creation without introducing the deferred registry or switcher.
- PC6. Inventory the callable gateway catalogue separately from official-client-local commands and
  name the minimal Talaria-local control set needed for composer, replay, and exit behavior.
- PC7. Pin the Hermes compatibility revision, validation corpus, measurable renderer thresholds, and
  supported Python/platform/terminal matrix that satisfy R34, R38, and R39. Classify every required
  gateway method by its safe compatibility evidence — read-only startup check or isolated acceptance
  test — because the terminal gateway has no capability endpoint and mutating methods are not probes.
- PC8. Identify and assess at least one Python presentation-layer fallback against the same validation
  criteria before the first-choice framework gate runs, so a failed gate has an evaluated next step.
- PC9. Define the terminal-read projection and failure contract: how `start` and `count` select the
  current Talaria terminal view, how the response serializes `total_lines`, `start`, `end`,
  `viewport_rows`, `cursor_row`, and `text`, and how an unavailable or timed-out view fails visibly
  without fabricating a buffer or leaking its contents to Talaria-side records.
- PC10. Define gateway credential acquisition, storage, rotation, and reconnect behavior while
  preserving R1 and R9: no attach token in command-line arguments, shell history, process listings,
  logs, diagnostics, exports, recordings, or status-command state.

## Sources and Research

- Architecture decisions in `platform-specs/04-architecture/adrs/` — ADR-0001 through ADR-0004 are
  settled and constrain everything above.
- `docs/analysis/2026-08-02-hermes-tui-feature-inventory.md` — surface-by-surface verdicts on Hermes's
  terminal UI. Its slash-command section overstates the work: the catalogue is gateway-supplied.
- `docs/analysis/hermes-gateway-protocol-surface.md` — the gateway defines 130 methods at Hermes
  `7f4d15515` and exposes a substantially broader surface than Talaria v0.1 requires. The requirements
  above name the pinned subset they rely on rather than inheriting the reference's client-call count,
  which measures Hermes's shipping client at the same pin rather than anything Talaria needs.
- `docs/formats/frame-log.md` — the frame log format contract, version 1, written to survive the
  language change.
- `src/record/` — the TypeScript recorder and its redaction suite, whose deny-set is keyed to Hermes's
  actual method registrations. Superseded as code, retained as the reference behavior R27 tests
  against.
- `docs/ideation/2026-08-02-talaria-product-shape-ideation.md` — survivors 14, 17, 18 and 28 are
  load-bearing here. Survivor 19 is superseded: ADR-0003 replaced porting with re-encoding, and its
  line count was measured against a different tree.
- Gateway methods relied on, verified at Hermes `7f4d15515`: `session.*` for the session loop,
  `spawn_tree.list`, `agents.list`, `delegation.status` and `subagent.interrupt` for sub-agent
  visibility, `commands.catalog` and `command.dispatch` for slash commands, and `paste.collapse` for
  large pastes.
- The status-line pattern is prior art from another agent harness, which runs an operator-supplied
  command, hands it a payload of session state, and renders the rows it returns. The pattern is the
  input here, not any particular implementation of it.
