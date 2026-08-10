# Changelog

What is different for you, release by release. The [engineering
journal](docs/engineering-journal/) answers the other question — why a thing was
done the way it was — and is written for whoever maintains this.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with the usual caveat that a `0.x` line may break anything between releases.

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

[0.2.0]: https://github.com/infiquetra/talaria/releases/tag/v0.2.0
[0.1.0]: https://github.com/infiquetra/talaria/releases/tag/v0.1.0
