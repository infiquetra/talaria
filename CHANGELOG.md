# Changelog

What is different for you, release by release. The [engineering
journal](docs/engineering-journal/) answers the other question — why a thing was
done the way it was — and is written for whoever maintains this.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with the usual caveat that a `0.x` line may break anything between releases.

## [Unreleased]

## [0.5.0] — 2026-09-01

Talaria v0.5.0 makes the terminal interface themeable and adds persistent bottom-of-screen
context, an inspector, and a bounded read-only view of session-reported changes. The
[v0.5.0 acceptance run](docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md) was
executed on 2026-08-31. **BLOCKED**: 43 of 43 expected checklist/tester slots are covered. The
evidence set separately contains 44 current receipts (42 item and 2 install). The one-receipt overlap
is checklist item 1 for talaria-t2, which has both an item receipt and an install receipt. Item
verdicts are 41 pass, 1 blocked, and 0 fail. Item 24 is the only blocker and is terminally
unreachable because the gateway always assigns a request identifier.

### Added

- **Four built-in themes and 58 semantic tokens.** Refined Default, Dark Green Terminal, Neutral
  Dark, and Accessible High Contrast cover the application, transcript identities, status bar,
  inspector, diff, and syntax classes. `/theme` previews without writing, `Escape` restores the
  prior theme, `Enter` selects for the process, and `/theme save [user|repository]` explicitly
  persists a selection by changing only `[theme]`.
- **A bounded Visual Studio Code theme importer.**
  `talaria theme import FILE [--name NAME] [--json]` accepts a documented allowlist of workbench
  colors and syntax scopes, reports unsupported input and Refined Default fallbacks, atomically
  replaces a deterministic user-theme JSON file, and writes nothing on malformed or empty input.
  `--json` writes one versioned machine-readable report to standard output; its fields are defined
  in the [Visual Studio Code theme import format](docs/formats/vscode-theme-import.md).
- **A configurable true-bottom status bar.** Seven reorderable/hideable segments show launch
  directory and branch, held agent/context/task state, connection, and version in one non-wrapping
  row. Fixed breakpoints shorten and drop lower-priority segments while retaining a literal
  connection glyph. `/bar` toggles segments for the process without writing configuration.
- **A right inspector from held session state.** `Ctrl+B` and `/inspector` expose Tasks, Context,
  Changed files, and Operation details. It docks from 120 columns, becomes a non-reflowing overlay
  below that width, preserves the requested state across resize, and renders honest empty sections.
- **A full-terminal, read-only diff viewer.** `/diffs` or `Enter` on an inspector file opens
  session-reported unified diff text. Side-by-side mode requires 112 columns and falls back to
  unified without losing the preference; file/hunk navigation, syntax treatment, intraline spans,
  long lines, and rendered work are bounded. No working-tree mutation path exists.
- **Configuration for the new surfaces.** `theme.name`, status-bar segment/cap rows, and
  `ui.reduced_motion` join the additive startup snapshot with visible fallback notices and no
  external-file live reload.

### Changed

- **The interface uses semantic theme colors throughout.** Six transcript groups retain literal
  labels and fixed gutters as well as their own fills, so identity does not depend on color.
  Connection, queue, approval, agent, selection, and diff states likewise keep words or glyphs.
- **Focus no longer changes screen height.** The composer changes its existing border title and
  color, and other controls use reserved gutters or fixed borders. Moving focus cannot add a row or
  move the transcript viewport.
- **The needs-you summary now feeds `task_progress`.** The queue count remains visible in the
  default bottom bar while `/needs` continues to provide the full detail.
- **Reduced motion and stable scroll are explicit policies.** `ui.reduced_motion = true` freezes
  decorative frames and makes routed scrolling immediate. Layout, status, theme, inspector, and
  streaming updates preserve an unpinned reader's held transcript anchor; `End` or `F5` follows the
  bottom again.

### Fixed

- Invalid status intervals, width caps, segment lists, and commands now produce visible startup
  notices and bounded fallbacks instead of handing bad values to the runner or silently removing
  the status-command region.

### Known limitations

- The [v0.5.0 acceptance verdict](docs/acceptance/2026-08-30-talaria-v0-5-0-live-acceptance-results.md)
  is **BLOCKED** on item 24 alone. The gateway always assigns a request identifier, so the keyless
  approval state required by that item is terminally unreachable without simulation.
- The `growing-one-column-table` steady-state p99 `TranscriptPane.apply` latency exceeds its 50 ms
  ceiling. Its run-to-run spread makes it unsuitable as a release gate; the measured debt and revisit
  condition are recorded in [QUEUED.md](docs/engineering-journal/QUEUED.md#the-shipped-block-markdown-gate-misses-its-steady-state-table-apply-ceiling).
- The v0.4.0 limitations remain unchanged: no person has driven Talaria on Linux; no run has used a
  real terminal emulator; gateway compatibility checks only top-level response shapes;
  `terminal.read.respond` has no runtime evidence; and the intermittent full-suite failure remains
  under investigation.

## [0.4.0] — 2026-08-19

Talaria becomes a fleet client. One running instance now dials every
configured profile, tracks every session those gateways can see, and keeps
one queue that answers the operator's standing question: what is waiting on
me? The scope comes out of driving v0.3 across several concurrent
workstreams, where a terminal showing one session answers the wrong
question. Twelve pull requests: eight code units, a live-gateway analysis
that preceded them, a mid-turn replan that slotted the foreign-wait slice,
a closing review-and-QA pass over the whole release, and the acceptance
record. The deepest unit — the needs-you queue — took twelve adversarial
review rounds plus a bounded confirmation, and its hardest finding
corrected the project's own review rules. The unit-by-unit record is in
[the fleet-turn plan](docs/plans/2026-08-16-talaria-v0-4-fleet-turn-plan.md);
the live-acceptance record is in
[the acceptance results](docs/plans/2026-08-19-v0-4-live-acceptance-results.md).

### Added

- **The fleet.** `build_live_app` assembles a `ConnectionSet` — one
  connection per configured profile endpoint, plus the credential file's
  own gateway — and hands it to the app as frame source, ensurer and
  fleet. Each connection carries its own credential, correlator, epoch,
  reconnect schedule and probe state, and every frame is routed by the
  connection it crossed. Failure is per connection: one gateway down never
  stalls another's dial or frame flow.
- **A registry of every session the fleet can see.** A background event
  updates its own row instead of being discarded, an event from an unknown
  session creates one, and the focused transcript does not move. Rows
  seeded from the roster enter never-observed — named as such, never faked
  as idle or aged zero — and ages render as "waiting at least the observed
  span" rather than inventing a start. A row is retired only when both
  roster sweeps of the same epoch omit it and it is not focused, queued or
  holding an in-flight answer. A 256-row per-connection bound evicts
  oldest-first among unprotected rows, and the eviction is named on a
  visible notice, never a silent drop.
- **The needs-you queue.** Every outstanding human-facing blocking prompt
  from every registry session becomes one typed item in one flat,
  wait-age-ordered queue — fed by events where Talaria drives and by polls
  where it does not, with one identity per wait so attaching mid-wait
  never doubles an item. The queue is derived on every read, never stored,
  so an expiry removes the prompt and its item in the same reduction. An
  item the queue declines to carry is named on its row; an empty queue on
  a fleet that could not be fully asked says so instead of saying "none".
- **One reserved summary row, and `/needs`.** The summary occupies a
  single screen row reserved from first mount and cannot grow; it names
  the count, then the oldest item's source, age and session. `/needs`
  opens a drill-down that navigates across connections and answers
  approvals inline through the same function the prompt card uses.
  Answering is a descent into named choices — never an empty choice,
  because the gateway reads any resolved non-deny answer, an empty one
  included, as approved.
- **Foreign waits reach the queue.** A session someone else owns, waiting
  on an approval on a gateway Talaria is merely connected to, appears —
  fed by a per-connection `approval.pending` poll on a wall-clock cadence
  with change-hint coalescing. On a gateway that does not register the
  method, the queue names the disabled feature rather than going silent.
- **Confirm before steal.** Moving focus to a session another live client
  is driving asks first, in a dedicated two-row modal rather than the
  picker — the picker gives every printable key to its filter, and a
  stray keystroke must not answer a question about taking somebody's
  session. The confirmation fires only for a row the latest roster sweep
  saw live elsewhere; everything else resumes dialog-free.
- **Seam probes, per connection.** Startup asks each gateway what its
  install can do, and absence is proved rather than inferred: a
  method-not-found on a parameterized call is re-asked bare, and only a
  second refusal claims absence. Every named absence names the feature it
  turns off. `approval.pending` is probed with no session id at all, so
  the probe can never warm a session's agent build.
- **Fleet recording and replay.** A multi-connection `--record` run writes
  a version-2 frame log — its connections in the header, a profile on
  every frame — and the reader stops on a version it does not know rather
  than misreading it. Replay reproduces registry, queue and focused
  projection deterministically, ages included, and the validation gate
  gains seven fleet checkpoints at two speeds. A single-connection
  recording stays byte-identical version 1.
- **Per-profile credentials.** A named profile resolves its own
  `[profiles.<name>].token` and nothing else, and
  `refresh-credential --profile <name>` writes it. Two profiles whose
  endpoints parse equal
  share a connection only when their tokens are byte-identical; otherwise
  the launch refuses loudly and names both.

### Changed

- **`/profiles` ensures instead of drop-switching.** Selecting a profile
  brings its connection up and makes it the home for the next create or
  resume, leaving every other socket alone. Since v0.1 it had dialed the
  selected gateway in place of the current one.
- **Stale is named with its span.** Every derived value renders stale-since
  with its source and age. Ages come from the frame clock and freeze when
  frames stop rather than drifting with the wall clock, and an item whose
  connection broke reports its age as a floor measured to the break, with
  the blind span named separately.

### Fixed

- **`refresh-credential` verifies its edit instead of trusting its
  scanner.** Adversarial review found four distinct ways a line-wise TOML
  editor misreads structure — a table header carrying a trailing comment,
  an array-of-tables header, a quoted key containing a bracket, and a
  header-shaped line inside a multi-line string — each able to destroy one
  profile's token or leave a stale one authoritative while the command
  reported success. The guarantee is now a verification gate: parse the
  document before and after, require the result to equal the original with
  exactly one key set, and otherwise refuse with the file untouched. Two
  exotic shapes the old writer handled are now refused by design — a
  refusal that names the problem is a strictly better failure than a
  silent write.

## [0.3.0] — 2026-08-13

Talaria confirms what it just did. The release comes out of driving v0.2 by
hand and finding that four defects which looked unrelated were all the same
problem: a keypress that did nothing looked exactly like a keypress that
worked. Alongside that, the composer gains the two conventions every
comparable interface already has — history on `Up`, and a slash-command
palette. Nine units; two shipped clean, three needed one repair round, and
four needed a further round after an independent check found the first
report had claimed more than the work delivered. The session-by-session
record is in [the decision register](docs/plans/2026-08-11-v0-3-decision-log.md).

### Added

- **`Up` recalls what you sent.** An in-memory composer history bounded at
  100 entries. `Down` walks back toward the newest and then restores the
  draft you were part-way through typing, caret at the end. A multi-line
  draft recalls only at the caret boundary you are moving through, so
  editing a paragraph still works. Slash commands and lines the gateway
  failed to deliver are held; empty lines and refused submissions are not.
  Never written to disk.
- **`/` opens a filterable command palette** over Talaria's own eight
  commands and the gateway's, Talaria's first. Filtering is by prefix,
  case-insensitively. `Up` and `Down` move the selection and scroll it into
  view, `Enter` inserts the canonical name with a trailing space and sends
  nothing, `Escape` closes it and keeps what you typed, and moving the caret
  closes it too. `F3` still browses the whole list.
- **An outstanding prompt card takes the caret by itself** when the composer
  is empty, so the answerable thing is where your hands already are. The
  first card of a batch takes focus; a later card in the same batch does not
  steal it from the one you are answering.
- **A resumed session names itself on arrival**, by its durable id, placed so
  it survives the transcript clear and introduces the history it precedes. A
  reply carrying no durable id names the runtime id and says so.
- **Four silent keypresses now confirm themselves** on the composer notice,
  each at the site of its silence — re-following when already at the bottom
  among them.
- **Typing where text cannot go says typing is paused**, names the region,
  and leads with the way back so the sentence survives 80 columns. Latched
  per focus-hold, cleared when the caret leaves the region. The keystroke is
  still discarded; the silence is what changed.
- **`/agents`** — a slash primary for the sub-agent rows, which had only a
  function key before.

### Changed

- **Every action the desktop can eat has a non-function-key primary.**
  `ctrl+g` toggles sub-agent rows, `ctrl+c` interrupts, `end` re-follows the
  newest line. `F2`, `F4` and `F5` remain as aliases where the desktop
  delivers them; `F3`, `F6` and `F7` remain beside their slash primaries;
  `F8`, `F9` and `F10` stay primary for replay pacing. The help footer is
  scoped to the running mode, so a live session no longer advertises replay
  keys.
- **The fallback banner reports what is hidden, not what is retained.** It
  printed a retained line count under the word "clipped" — a number a reader
  takes as a loss, visibly falling as more was hidden. It now names the
  hidden count and the total in one clause, at a named scope, so the banner
  and the condensed marker count the same quantity.
- **An unknown gateway event announces once per type, per connection**, with
  repeats counted rather than reprinted, and an unknown event belonging to a
  background session no longer writes into the foreground transcript.

### Removed

- **`F1`'s jump to the newest unanswered prompt**, with its action and
  constants. The card owns focus now, so the jump has no job. On macOS the
  key never reached Talaria at all: the desktop ate it, and an eaten key
  sends no bytes, so the program could not report the loss.
- **The caret status row**, with its slot, its CSS and the walk that fed it.
  It named where the caret was — unreadably in the one case that mattered,
  redundantly everywhere else. The discard notice above covers the case the
  row existed for.

### Fixed

- **`F4`'s published description was missing its destructive half, and this
  corrects it forward.** Both [the v0.2.0 release notes](docs/releases/v0.2.0.md)
  and the v0.2.0 entry below say `F4` "sweeps the answerable set" and stop
  there. What `F4` is actually bound to is `interrupt` — it **cancels the
  in-flight turn**, and only _then_, and only when the gateway confirms the
  cancellation, does it decline that turn's outstanding prompts. An interrupt
  whose outcome is unknown declines nothing, deliberately, because the turn may
  still be alive.

  So the omitted half is the one that destroys work, and a reader who pressed
  `F4` expecting a sweep would have stopped their own turn. **The shipped v0.2.0
  notes are left as they were published** — a released artifact says what it
  said — and this entry is the correction of record.

## [0.2.0] — 2026-08-10

The interface becomes answerable, `--resume` means what it says, and the
transcript renders real markdown. Two legs, each merged behind its own
measured gate: the answerability spine (41 external-review findings fixed,
every fix pinned by a revert-checked test) and the block-markdown build
(a 24-of-24 full-scale replay gate, its figures published in
[the gate results](docs/analysis/2026-08-09-block-markdown-gate-results.md)).

### Added

- **Answerable prompts.** `F1` jumps to the newest unanswered prompt
  (modal-aware), every prompt kind carries its own hint line and focus tint,
  `Escape` declines per kind — an approval sends an explicit `deny`, because
  the gateway treats any resolved non-deny choice, an empty one included, as
  approved — and `F4` sweeps the answerable set. Typed text survives a
  refused decline.
- **A caret slot** — a dedicated status-region row that always names where
  the caret went, geometry-invariant across states.
- **`--resume` renders history.** A resumed session's prior conversation is
  decoded and on screen, content-preserving: a row the decoder cannot read
  surfaces as unreadable rather than vanishing, and history the gateway
  withheld is named, counted where possible, never silently blank.
- **`/sessions`** — a picker over the gateway's sessions; selecting one
  switches the running interface to it, with the outgoing session's
  transcript cleanly replaced, not merged.
- **Block-level markdown.** A committed assistant or reasoning entry renders
  as a real markdown document — headings, fenced code, lists, tables, block
  quotes — while both in-flight streams render progressively in their own
  live documents at the same 50 ms cadence as before. Every other kind keeps
  line rendering, now with per-kind visual differentiation. Degenerate
  content (a 100,000-character line, a 601-column table, a 10,000-line
  fence) falls back to bounded, clipped line rendering behind a one-row
  banner instead of stalling the interface.

### Changed

- **v0.1's "markdown presentation is out of scope" rule (R6) is amended**:
  presentation is now in scope; the guarantee that content is never dropped
  is unchanged and still pinned by the same projection test.
- **The bounded-rendering claim is restated** for a block-rendering pane:
  [ADR-0006](platform-specs/04-architecture/adrs/0006-block-rendering-is-bounded-by-work-and-height.md)
  bounds rendering by work and height (two-tier widget ceilings, a
  steady-state 50 ms p99 apply-latency ceiling) in place of v0.1's
  one-line-one-widget claim, and the replay gate proves the restated claim
  at full scale — 24 checks, mid-stream ownership and progressive
  reachability included.
- **A profile-carrying 404 is disambiguated honestly** — a bare probe that
  refuses redirects and never lets the credential leave the origin — instead
  of being read optimistically.
- **An ambiguous approval outcome settles and latches** rather than
  restoring the card: the wire carries no request id, so a restored card
  could be a zombie no keystroke can kill; over-latching self-heals through
  gateway expiry. Recorded in the engineering journal with its revisit
  condition.

## [0.1.0] — 2026-08-08

The first release. Talaria dials a Hermes gateway it did not launch, runs a
session in a terminal interface, and can record and replay everything it saw.

**Read the limits before relying on it.** They are listed in the [release
notes](docs/releases/v0.1.0.md) and set out row by row in
[the v0.1 daily-driver verdict](docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md).
The short version: nobody has driven the interface on Linux, and no run on
either platform has used a real terminal emulator rather than a bare
pseudo-terminal.

### Added

- **A live session.** A bare `talaria` dials the configured gateway, opens a
  session and streams turns to completion. `--session <id>` attaches to a
  specific session, `--resume` picks up the most recent one, and asking for both
  is a usage error raised before anything is dialed.
- **Sub-agent visibility.** Delegated work renders as its own rows rather than
  being flattened into the transcript. `F2` folds them, `F5` re-follows the
  newest line.
- **`/models`** — a picker over the models the connected Hermes offers. With no
  argument it opens or closes; with an index it switches the running session;
  with `<n> default` it writes that model as the connected profile's default
  instead, behind a confirmation for an expensive model.
- **`/profiles`** — a picker over the profiles the gateway knows about, showing
  each one's configured model and whether its gateway is running. Selecting one
  dials that profile's gateway. It never rewrites Hermes's own active-profile
  setting, which by Hermes's documentation would not move the session in front
  of you anyway.
- **Recording.** `--record` on a live session, or `talaria record` with no
  interface at all, writes every frame to a frame log with credentials redacted
  on the way out.
- **Replay.** `talaria replay <recording>` drives the entire interface from a
  frame log with no socket open anywhere in the process. `F8` pauses, `F9`/`F10`
  change speed. Controls that would change something on a gateway are visibly
  inert.
- **`talaria refresh-credential`** — rewrites the credential file from a running
  Hermes dashboard's session token, which the dashboard mints at start-up and
  keeps in memory. Needed again after every dashboard restart. The token is
  never printed.
- **`talaria gate`** — re-runs the framework validation gate against a recording
  and prints its measurements as JSON. Exits non-zero on a fail verdict.
- **`talaria --version`**, reporting the same string the distribution metadata
  carries.
- **A status line** driven by a configured command, and paste collapsing with
  configurable byte and line thresholds.

### Security

- A credential is never written to a frame log, a transcript export, a
  diagnostic record or a status payload, and never appears in Talaria's own
  command line or environment. The Python recorder's redaction is asserted
  equivalent to a frozen TypeScript reference implementation on every run.
- An endpoint carrying a credential in its query string, userinfo or fragment is
  **refused** rather than quietly stripped, whether it arrives on the command
  line or in `TALARIA_GATEWAY_URL`.
- The credential file is written `0600`.
- **Not fixed, and not fixable by Talaria:** a credential exported into the
  environment by your shell before Talaria starts is inherited and stays
  readable for the life of the process. Use the credential file.

### Known limitations

- No person has driven the interface on Linux. The suite runs there in
  continuous integration; nobody has used it there.
- No run on either platform has used a real terminal emulator. Every measurement
  behind the v0.1 verdict was taken against a bare pseudo-terminal.
- Gateway method compatibility is checked at the top level of each response
  only — the key set and each value's kind, not nested payloads.
- `terminal.read.respond` has no runtime evidence at all.
- The test suite fails intermittently, undiagnosed: twelve runs green in
  thirteen.

### Not published to the Python Package Index

Install from a release tag. The name `talaria` on PyPI belongs to an unrelated
content management system whose last upload was 2010-06-19, so
`uv tool install talaria` gets you that project rather than this one.

[Unreleased]: https://github.com/infiquetra/talaria/compare/v0.5.0...HEAD
[0.5.0]: https://github.com/infiquetra/talaria/releases/tag/v0.5.0
[0.4.0]: https://github.com/infiquetra/talaria/releases/tag/v0.4.0
[0.3.0]: https://github.com/infiquetra/talaria/releases/tag/v0.3.0
[0.2.0]: https://github.com/infiquetra/talaria/releases/tag/v0.2.0
[0.1.0]: https://github.com/infiquetra/talaria/releases/tag/v0.1.0
