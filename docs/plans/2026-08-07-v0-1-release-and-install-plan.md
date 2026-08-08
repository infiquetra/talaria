---
title: Releasing v0.1 — how a person gets Talaria, and what a tag asserts
type: chore
status: active
date: 2026-08-07
origin: docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md
---

# Releasing v0.1 — how a person gets Talaria, and what a tag asserts

## Summary

The v0.1 daily-driver gate reached **READY** on 2026-08-07, which answers "is this usable". It does
not answer "how does anyone get it", and today the honest answer is *they cannot*: nothing has ever
been tagged, no release or publish workflow exists, and the README's install instruction points at a
different project entirely.

This plan ships a **git-installable v0.1.0** — a tag and a GitHub Release with built artifacts — and
leaves the Python Package Index (PyPI) for later, deliberately. It also corrects the install
instruction, collapses a version number that currently exists in two places, and shrinks the
superseded TypeScript tree down to the one part of it that is still load-bearing.

## Problem frame

Four things are true on `main` at `1012c05`, all verified rather than recalled.

**1. Nothing has ever been released.** `git tag -l` is empty. The only workflow is `Validate`. The
version `0.1.0` in `pyproject.toml` has never been attached to an artifact anyone could install.

**2. The README tells people to install someone else's package.** It reads:

```bash
uv tool install talaria
```

`talaria` on PyPI is a content management system whose last upload was **2010-06-19** (version 0.2.0,
GNU GPL v2). Anyone following the README today installs that. This is a live defect in a public
repository and is independent of everything else here.

**3. The version number has two sources of truth.** `pyproject.toml` declares `version = "0.1.0"` and
`talaria/__init__.py` separately declares `__version__ = "0.1.0"`. Nothing reconciles them, and there
is no `--version` flag, so a user cannot tell you what they are running and a bug report cannot be
tied to a build.

**4. The superseded TypeScript tree is not entirely superseded**, and this is the finding that changed
the plan. `src/` holds eight tracked files. Five are the dead Ink-based interface bootstrap that
ADR-0004 and ADR-0005 replaced. **Two are a live oracle**: `src/record/recorder.ts` and
`src/record/redact.ts` are the reference implementation that
`tests/recorder/test_equivalence.py` runs the Python recorder against, asserting KTD6's equivalence
relation over the *credential redaction boundary* — the blocking bridges, `model.save_key`, nested and
array credentials, camelCase keys, unparseable payloads. Deleting `src/` wholesale, which `CLAUDE.md`
anticipates in general terms, would silently delete a credential-redaction guarantee.

## Decisions taken 2026-08-07, by the operator

**Distribution: a git tag and a GitHub Release, not PyPI.** Install is
`uv tool install git+https://github.com/infiquetra/talaria@v0.1.0`. The reasoning is that the channel
should match the evidence: the gate says READY with six stated limits, two of which are that nobody
has driven the interface on Linux and no run on either platform has used a real terminal emulator
rather than a bare pseudo-terminal. Publishing to PyPI says "this is for anyone, on any machine",
which those two limits do not support. A tag is reversible in a way a published package is not.

**The PyPI name: file a PEP 541 request for `talaria`, in parallel.** Sixteen years dormant is a
strong case under PyPI's abandoned-project policy, and pursuing it blocks nothing because the release
does not depend on it. Rejected: renaming to `talaria-tui` now, which would make the install name and
the import name differ permanently in exchange for reach this release is not seeking.

## Steps

Each step is a separate commit with the project check green — `uv sync --all-groups`,
`uv run ruff check .`, `uv run mypy`, `uv run pytest`, `uv run bandit -r talaria -q`,
`git diff --check`.

### S1 — Make the install instruction true and the version single-sourced

- Replace the README's `uv tool install talaria` with the git-based instruction, and say plainly in
  the same block that the PyPI name currently belongs to an unrelated project, so a reader who tries
  the obvious thing understands what happened.
- Make `pyproject.toml` read the version from `talaria/__init__.py` (`dynamic = ["version"]` with
  hatchling's version source). One source of truth by construction, which is better than a test
  asserting two values agree.
- Add `talaria --version`. It prints the same string the package reports.

**Verified by:** the existing install job already runs the built console script under `env -i`; extend
it to run `--version` and assert the output matches the tag on a tagged build.

### S2 — Shrink the TypeScript tree to the oracle it still is

Keep `src/record/recorder.ts`, `src/record/redact.ts` and `src/record/redact.test.ts` — the reference
implementation and its own tests. Remove the five files nothing depends on any more: `src/app.tsx`,
`src/app.test.ts`, `src/cli.tsx`, `src/record/command.ts`, `src/transport/attach.ts`. Drop `ink` from
`package.json`. Update `CLAUDE.md`'s note so it names what remains and *why*, rather than describing
the whole tree as awaiting removal.

**Verified by:** `tests/recorder/test_equivalence.py` still passes with the bridge enforced —
the CI leg that sets `TALARIA_REQUIRE_TS_BRIDGE=1` is the one that matters, because these tests
silently skipped once already and a guard was added specifically to stop that recurring.

**Optional, and it is a judgement call rather than a clear win.** This step is not required to ship.
It removes confusion for anyone who opens the repository and finds Node scaffolding in a Python
project, and it cuts what `npm run check` spends CI on. It can be deferred without affecting S3–S5.

### S3 — A release workflow, which refuses to ship over a red gate

On a pushed tag matching `v*`: build the wheel and source distribution with `uv build`, run the
project check, read the `gate` block in `docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`, and
create the GitHub Release with both artifacts attached.

**The gate check is the part worth arguing about.** Refusing to publish when the verdict is not READY
makes a tag mean something — it cannot ship over a gate the repository's own document says is shut.
It also has an obvious failure mode, recorded in the pre-mortem below.

**Verified by:** a dry run against a pre-release tag (`v0.1.0-rc1`) before the real one, so the
workflow's first execution is not also the first release.

### S4 — Cut v0.1.0

- A `CHANGELOG.md` written for users, not maintainers. The engineering journal answers "why did this
  happen" for whoever maintains this; a changelog answers "what is different for me".
- Tag `v0.1.0`, push it, let S3's workflow produce the release.
- Release notes lead with the six limits from the verdict rather than burying them. Someone installing
  from a release page should meet "nobody has driven this on Linux" before they meet the feature list.

### S5 — File the PEP 541 request — operator-owned

The template at `pypi/support` (`.github/ISSUE_TEMPLATE/pep541-request.yml`) requires the project
name, **your PyPI username**, the reasons, whether this is maintenance or replacement, source
repository URLs, and a **required "Contact and additional research" field** — evidence of an attempt
to reach the current owner. The contact attempt and the username are yours; the rest can be drafted
here.

This step is not on the critical path and its outcome does not change S1–S4.

## What this plan deliberately does not do

- **It does not publish to PyPI**, under any name. That is a separate decision to be taken when the
  Linux and real-terminal-emulator gaps are closed, or when there is a person waiting for it.
- **It does not re-open the v0.1 gate.** No row is re-graded and no verdict moves.
- **It does not delete the TypeScript recorder oracle**, and any future proposal to do so has to say
  what replaces the redaction equivalence guarantee first.
- **It does not build a standalone binary.** `uv tool install` handles the Python runtime; a frozen
  binary is a different problem to solve when there is evidence someone needs it.

## Pre-mortem — the most likely ways this goes wrong

**The gate check blocks the release that would fix the regression.** If a defect drops the verdict to
NOT READY, S3's check refuses every tag — including the patch that repairs it. This is the classic
shape of a safety interlock that fires hardest exactly when you need to move. Mitigation: make the
check overridable by an explicit, logged workflow input rather than absolute, so bypassing it is a
visible act in the run record instead of an argument at three in the morning.

**The release ships and someone installs it on Linux.** The README and release notes will say nobody
has driven it there, and people install things without reading. The failure would be first-contact and
public. This is an accepted risk of releasing at all, and it is the reason the channel is a git URL
rather than a package index — someone installing from a git tag has already read something.

**The version drift returns through the tag.** A tag saying `v0.1.1` over a package still reporting
`0.1.0` is exactly the confusion S1 exists to prevent, reintroduced one layer out. Mitigation: S3
asserts the tag and the package version agree before it builds anything.

**S2 removes a file something still needed.** The import graph was traced rather than assumed —
the bridge loads `recorder.ts`, which imports `redact.ts`, and nothing else reaches the five files
marked for removal. If that trace is wrong, the equivalence test fails loudly on the CI leg that
enforces the bridge, which is the correct place for it to fail.
