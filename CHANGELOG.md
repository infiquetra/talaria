# Changelog

What is different for you, release by release. The [engineering
journal](docs/engineering-journal/) answers the other question — why a thing was
done the way it was — and is written for whoever maintains this.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
Versioning follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
with the usual caveat that a `0.x` line may break anything between releases.

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

[0.1.0]: https://github.com/infiquetra/talaria/releases/tag/v0.1.0
