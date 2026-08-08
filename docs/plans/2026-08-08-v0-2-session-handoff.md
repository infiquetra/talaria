---
title: Handoff into v0.2 — what shipped, what it does not prove, and what will bite
type: handoff
status: active
date: 2026-08-08
origin: docs/plans/2026-08-07-v0-1-release-and-install-plan.md
---

# Handoff into v0.2

Written 2026-08-08, at `main` = `9509641`, immediately after v0.1.0 was released. This is for a
session that starts cold: it says where the repository actually is, what the v0.1 verdict does and
does not license anyone to claim, which defects are known and already diagnosed, and which repo-specific
traps have already cost a session once.

**It does not choose v0.2's scope.** That is the first real decision of the next session, and the
candidates are set out at the end with the evidence behind each rather than a recommendation dressed
as a plan.

## Where the repository is

- `main` at `9509641`, clean, no open pull requests, no branches other than `main` local or remote.
- **v0.1.0 is released** — <https://github.com/infiquetra/talaria/releases/tag/v0.1.0>. Install is
  `uv tool install git+https://github.com/infiquetra/talaria@v0.1.0`, verified end to end into a
  clean prefix under a scrubbed environment.
- A second release exists, `v0.1.0-rc1`, correctly flagged prerelease. It is the dry run of the
  release workflow, kept deliberately as evidence that the workflow worked before it was trusted.
  Its wheel and the v0.1.0 wheel have **identical sha256 digests**, so the build is reproducible
  across independent continuous-integration runs.
- Full project check run at this commit and green: `ruff check` clean, `mypy --strict` clean across
  122 source files, **1361 passed / 1 skipped in 145s**, `bandit` clean, `git diff --check` clean.
  One green run is not proof — see the intermittent failure in *What v0.1 does not prove*.

## Read these first, in this order

1. **[CLAUDE.md](../../CLAUDE.md)** and **[AGENTS.md](../../AGENTS.md)** — the repo's own rules.
2. **The five architecture decision records** in
   [platform-specs/04-architecture/adrs/](../../platform-specs/04-architecture/adrs/). Four are
   settled; ADR-0005 (Textual as the presentation layer) is `proposed` on the validation gate's pass
   verdict. These are constraints, not history — see *Invariants* below.
3. **[The v0.1 daily-driver verdict](../analysis/2026-08-02-v0-1-daily-driver-verdict.md)** — the
   gating document. Its fenced ` ```gate ` block currently reads `verdict: READY`,
   `review-by: 2026-09-06`, with no blocking conditions. **Read the evidence table, not the
   verdict.** READY is shorthand for "every row of that table is measured or met, across the
   requirements it was scoped to cover" and nothing wider.
4. **[QUEUED.md](../engineering-journal/QUEUED.md)** and
   **[DECISIONS.md](../engineering-journal/DECISIONS.md)** — the deferred work and the durable
   decisions. DECISIONS carries several entries whose form is "we decided X, and here is what would
   make us revisit"; check the revisit conditions before re-opening anything.

## What v0.1 does *not* prove

These six are published in the release notes and the changelog, so they are already public. They are
repeated here because they are the honest frame for scoping v0.2 — each one is a place where a
confident claim would be wrong.

1. **No person has driven the interface on Linux.** The suite runs there in continuous integration,
   pseudo-terminal and process-surface tests included. Nobody has used it there.
2. **No run on either platform has used a real terminal emulator.** Every measurement behind the
   verdict was taken against a bare pseudo-terminal.
3. **Gateway method compatibility is proven at the top level only** — a response's own key set and
   each value's kind, not nested payloads. The same measurement that cleared that row found two
   pinned shapes wrong.
4. **`terminal.read.respond` has no runtime evidence at all.** It was taken out of scope for runtime
   evidence on 2026-08-07 with its condition named in the verdict; it stays required for
   compatibility.
5. **A credential the shell exported before Talaria started stays visible** in the process
   environment for the life of the process. Talaria adds none of its own and refuses an endpoint
   carrying one — but it cannot remove what it inherited. See the prohibition under *Invariants*.
6. **The test suite fails intermittently** — twelve runs green in thirteen, undiagnosed.

## Invariants — the rules that will bite

Every one of these has a document behind it. They are collected here because each is the kind of
constraint a reasonable change violates by accident.

**Architecture.**

- **ADR-0001** — Talaria is a standalone client that dials a gateway it did not launch, possibly on
  another host. Any design that reads a file on the local machine to discover gateway state is
  wrong: it silently returns *this* machine's answer when the gateway is remote. Discovery goes over
  the transport.
- **ADR-0002** — the domain core never imports the terminal framework. `talaria/domain/` must not
  import Textual, directly or transitively.
- **ADR-0003** — Hermes's own terminal interface is *documentation of behavior*, not a source tree to
  translate. Read it to learn what correct looks like; do not port it file by file.

**The TypeScript tree under `src/` is not dead code.** It is three files holding the frozen reference
recorder that `tests/recorder/test_equivalence.py` asserts the Python recorder is equivalent to,
across the credential redaction boundary. It is run in a `tsx` subprocess on every suite run.
`TALARIA_REQUIRE_TS_BRIDGE=1` turns a missing toolchain into a failure rather than a silent skip.
**Do not extend it, do not port it, and do not delete it without first saying what replaces the
redaction equivalence guarantee.** This tree was nearly deleted as "superseded bootstrap" on
2026-08-07 — three documents said it was, and the import graph said otherwise.

**Credentials.**

- Never reproduce a credential value in any file, test fixture, commit message or plan text. Use
  canary literals, described categorically.
- A credential is never written to a frame log, transcript export, diagnostic record or status
  payload.
- **Do not write anywhere that requirement R1's environment clause is now met**, technically or
  otherwise, and do not widen R1's wording so the command-line half satisfies it. The failing half is
  asserted by a test that asserts the *failure*; if Talaria ever does scrub its inherited
  environment, that test goes red and somebody removes it on purpose. This prohibition is recorded in
  both QUEUED.md and DECISIONS.md.

**Recording corpora (R29/R11).** Never committed and never cited by local path — cite by sha256
digest and count. Two digest namespaces exist and must not be conflated:
`talaria-live-v1-<n>f-<hash>` names a **single** recording; `talaria-live-corpus-v1-<n>f-<hash>`
names the **aggregate**.

**Public repository.** No private operational context and no secrets. In particular (R12) no operator
profile name, profile path, or other operator-specific inventory from `GET /api/profiles` in any
committed fixture, document or commit message — profile names in tests are synthetic.

**No attribution lines** in commits or generated content, of any kind.

**The engineering journal ships in the same commit as the change.** A non-obvious fix gets a dated
LEARNINGS entry with evidence, mechanism and a generalizable rule; a convention or tooling decision
gets a DECISIONS entry with rationale, rejected alternatives and a revisit condition.

## Known defects, already diagnosed

All three were found by driving the interface live rather than by reading code, and each has its
mechanism identified in QUEUED.md. None is speculative. **A filing note:** the second and third are
marked P1 but sit physically in QUEUED.md's `## P0` section — read the stated priority, not the
heading they fall under.

### P0 — a blocking prompt cannot be answered without guessing how many times to press `tab`

An approval expired while its answer was being aimed, twice in one session. One `tab` from a sudo
card put focus on the composer and the typed answer appeared in plain text where a chat message goes
— a canary, but a real password would have been one `enter` from the transcript. The tab distance to
a control varied between **3 and 7** in the same session, because it depends on what else is on
screen. The focus styles exist and are only legible once you already know which row to look at;
locating a control took an ANSI-level dump of the screen.

This is the same shape as the picker defect closed on 2026-08-07: the machine does the right thing
when driven correctly, and nothing tells the operator what correct is.

### P1 — `--resume` reattaches to the session and discards its entire conversation

Measured, not inferred. The gateway's `session.resume` reply carried `message_count = 3`, a
`messages` array of three entries, and `messages_omitted = False` — it withheld nothing. **Talaria
rendered an empty transcript.** `TalariaApp._land_session` reads exactly one field out of the reply,
`session_id`, and returns; `messages` and `message_count` appear nowhere in the package outside the
compatibility baseline.

Row 19's acceptance run graded `--resume` **pass**, and that grade is correct for what it measured —
which session each startup path lands in. Nobody asked whether the conversation appeared. Fix
`messages_omitted` in the same change: when true the gateway sends `messages: []` while
`message_count` still reports the real length, so a withheld history would otherwise render as a
complete short one.

The design question is not reading the reply. It is projecting the history into the same transcript
state the event stream feeds.

### P1 — `absent_capability` blames the gateway's version for a mistyped profile name

`GET /api/model/options` returns 200; the same path with `?profile=no-such-profile` returns **404**.
The admin client maps any 404 on an admin path to `absent_capability` — "this gateway does not serve
/api/model/options; it predates the admin model API" — about a gateway that plainly does serve it.
The message names a cause the operator cannot act on and hides the one they can, and it fires on the
most likely operator error. Small fix: only a 404 on the *bare* path is evidence of an absent
capability.

## Release mechanics, now that they exist

- **The version has one source.** `talaria/__init__.py` holds the literal; `pyproject.toml` declares
  `dynamic = ["version"]` with `[tool.hatch.version] path`. `talaria --version` and the distribution
  metadata cannot disagree. `tests/test_packaging.py` pins this.
- **Cutting a release is: bump that literal, update `CHANGELOG.md`, write
  `docs/releases/vX.Y.Z.md`, tag, push the tag.** `.github/workflows/release.yml` does the rest —
  it checks the tag against the package version, runs the full gate, builds, verifies the built
  artifact installs and reports its version, and creates the release with the notes file as its body.
- **The workflow refuses to publish over a red gate.** `scripts/gate_verdict.py` reads the fenced
  gate block by `id`, not by path. It exits `0` for "read it, verdict matches", `1` for "read it,
  wrong verdict", and `2` for "could not determine". **Only `1` is overridable**, and only via
  `workflow_dispatch` with `allow_red_gate` *and* a typed reason, both of which land in the run
  record. A gate that cannot be read is never waved through — that state means the check has
  silently stopped checking anything, which is the opposite problem from a check that disagrees.
- **The source distribution is an allow-list, not a deny-list**, in `[tool.hatch.build.targets.sdist]`.
  See *Traps*.

## Traps this repository has already sprung

**Hatchling's default sdist selection is a deny-list over an open set.** It ships "everything version
control does not *ignore*" — and untracked-but-unignored is a third state every machine-local scratch
file passes through. Building v0.1.0 locally put a never-committed, never-ignored settings file into
the tarball. Invisible on continuous integration forever, because CI always builds from a clean clone.
The `include` allow-list and a test now prevent it; the general rule is that any "everything except"
rule over a set you do not control will eventually include something you never chose.

**A shell `case` glob is not a regex.** The release workflow's first tag validation was
`case "$tag" in v[0-9]*.[0-9]*.[0-9]*)`, which looks strict and passes `v1.2.3; rm -rf /` — `*`
matches spaces, semicolons and newlines. It is now a bash whole-string regex, tested against ten
inputs.

**A workflow-level `permissions:` block elevates above the organization's
`default_workflow_permissions: read`.** Verified empirically by the rc1 dry run creating a release,
rather than argued from the settings page.

**Do not run `ruff format` in this repository.** The tree is not format-clean and the check is
`ruff check .` only. One run churned fifty unrelated files.

**Machine-specific gotchas** — a stale global install shadowing the repository build, a Hermes
restart invalidating the credential, and tooling that aggregates command output when the detail is
the point — are recorded in this project's session memory rather than here, because they are facts
about the operator's machine and this repository is public.

## Candidate scope for v0.2 — evidence, not a recommendation

Presented as an option space. The right first move is to pick a spine and let the rest follow, not to
take all of them.

**A. Make the interface answerable.** The P0 focus defect, plus two items that are the same defect
seen from other angles: the P1 "let the operator decline a blocking prompt without waiting for it to
expire" (Small) and the P2 "nothing on screen says where the caret is when it is not in the composer"
(Small, but it reopens a settled layout decision — anything that adds or removes a row is what must
*not* come back).

This is the only candidate that fixes something which can *lose an operator's data* — a password
typed into a transcript. It is also the one whose absence blocks recommending Talaria to a second
person.

**B. Make `--resume` mean what it says.** The P1 above. Smallest coherent feature-shaped change with a
real design question inside it (projecting history into transcript state), and it closes the gap most
likely to be noticed first by anyone who installs v0.1.0.

**C. Block-level markdown — headings, fenced code, lists, tables, block quotes.** P2, Large. **This is
the one item in the queue recorded as explicitly requested by the operator**, and it carries the
operator's own sequencing: *take it up once the defect list from the core build is clear.* Read that
as ordering candidate A (and probably B) ahead of it rather than as a reason to defer it
indefinitely.

Three things make it larger than "more of the same". The inline half already shipped
(`talaria/ui/markdown.py`: emphasis and code spans, with the decision beside it explaining why it
stopped there); this is the half whose unit is a **block**, not a line. It is a **requirement change**
— R6 puts markdown presentation out of scope for v0.1 while requiring content is never dropped, and
`tests/domain/test_projection.py::test_every_transcript_entry_survives_into_the_line_buffer` enforces
the half that stays. And it **reopens the measured rendering gate**: `interface_shows_everything` in
`talaria/replay/gate.py` compares mounted lines against the projection window position by position,
which "one renderable spanning many lines" breaks.

**Do not start this by writing widgets.** Start by deciding what replaces "one line, one widget" as
the bounded-rendering claim, and get that into an architecture decision record. The separate and
harder half is streaming: a fence arrives one delta at a time, so until the closer lands the correct
rendering is genuinely ambiguous. The queue's suggested shape — render blocks only on *committed*
entries, leave the provisional tail inline — is offered there as a shape, not a decision.

**D. Close the evidence gaps the verdict names.** Drive the interface on Linux; run under a real
terminal emulator; widen compatibility checking below the top level (`compare_shape` is queued
separately for exactly this). This converts three of the six published limits into measurements. It
is the prerequisite for ever publishing to a package index, and it is unglamorous.

**Not in scope unless deliberately chosen:** the Python Package Index name. That was researched,
drafted, and deferred on 2026-08-08 — see
[the plan](2026-08-08-pypi-name-request.md) and the DECISIONS entry. It reopens only when the name is
settled *and* there is intent to publish, and the name may well change before then.

## What this handoff deliberately does not do

- **It does not re-open the v0.1 gate.** No row is re-graded and the verdict does not move. If v0.2
  work clears a row, that is restated in *both* the evidence table and the gate block — the suite
  enforces their agreement — with a `Clears: v0-1-daily-driver#row-<n>` backlink.
- **It does not pre-authorize anything.** The previous handoff of this kind
  ([2026-08-02](2026-08-02-talaria-v0-1-work-launch.md)) was an answer sheet for an unattended run
  and carried standing authorizations for push, pull request and merge. This one carries none; it is
  context for a session that starts with a person in it.
