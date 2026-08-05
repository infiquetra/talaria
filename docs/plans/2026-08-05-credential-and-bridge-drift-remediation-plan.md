---
title: Credential and bridge drift remediation (DRIFT-01, DRIFT-03)
type: fix
status: active
date: 2026-08-05
origin: docs/brainstorms/2026-08-02-talaria-v0-1-prototype-requirements.md
---

# Credential and bridge drift remediation (DRIFT-01, DRIFT-03)

## Summary

Close the two conformance findings that the v0.1 audit raised against R9: `talaria record` can only
authenticate by putting a live gateway token on the command line, and the recorder's redaction test
covers three of the four blocking bridges. Both were found by running the program, not by reading it.

The work is three small units, each independently landable, with a merge-and-verify boundary between
them.

## Problem Frame

R9 requires that attach credentials stay out of command-line arguments, shell history, and process
listings. Two things are wrong, and one of them is a live credential leak.

`talaria record` takes the gateway URL as a required positional argument (`talaria/cli.py:68-75`) and
the credential rides that URL as `?token=`. It never consults the credential chain, so there is no
other way to authenticate it. Anyone who can run `ps` sees the token for as long as the recording
runs, and the operator's shell history keeps a copy afterwards. This was verified by running the
command against a dead port with a canary value and reading it back out of `ps -ww -Ao pid,command`
from a separate process.

The recorder's live-socket redaction test is parametrized over the blocking bridges but lists three
(`tests/transport/test_bridges.py:236-252`): `secret.respond`, `sudo.respond`, `clarify.respond`. The
fourth, `terminal.read.respond`, is untested on that path — and it is one of the two fields
(`talaria/recorder/redact.py:396`) that the key-name net does **not** catch, so its deny-set entry is
the only thing standing between captured screen content and the frame log.

Three documents state the opposite of the first finding and are part of why it survived. The audit's
independent static reviewer read one of them and graded the requirement as an accepted divergence.

| Document | Claim | Status |
| --- | --- | --- |
| `docs/engineering-journal/QUEUED.md:106` | "The argv half holds and is measured" | False for `talaria record`; the cited test never launches it |
| `docs/engineering-journal/DECISIONS.md:406` | Attach credentials "never appear in argv" | True of the acquisition chain, stated absolutely |
| `README.md:78` | Documents `talaria record ws://…?token=<token>` | Instructs operators to perform the leak |

The reason the defect survived is narrow and worth naming: the process-surface sweep that measures
R9 builds its probe with `parse_args([])` (`tests/transport/test_process_surface.py:79`) — the bare
launcher, no subcommand. It measures one entry point and the journal reported the result as if it
measured all of them.

## Requirements

- **R1.** The fourth blocking bridge's respond value never reaches the frame log, proven over a real
  socket against the real recorder, consistent with the three bridges already covered.
- **R2.** That proof must fail when the deny-set entry is removed. A test that passes for the wrong
  reason is the failure mode this whole audit exists to catch.
- **R3.** `talaria record` never requires a credential on its command line.
- **R4.** `talaria record` resolves its credential through the same `CredentialProvider` chain the
  launcher uses, on every dial.
- **R5.** `talaria record` refuses a URL carrying a credential in **either** its query string or its
  userinfo, exits 2, and tells the operator the value they passed should be treated as exposed.
- **R6.** The refusal message never reproduces the credential, the URL that carried it, or any
  fragment of either.
- **R7.** The process-surface sweep covers every shipped entry point that can hold a credential.
- **R8.** A newly added subcommand fails a test until it is classified as credential-holding or not,
  so this cannot silently reopen. That failure must be demonstrated, not asserted — the guard's whole
  value is that it reds, and a guard that cannot be shown to red is the same defect this plan fixes.
- **R9.** No document asserts an argv guarantee broader than what is measured, and the corrections
  record that the previous claim was an overclaim rather than quietly becoming true.
- **R10.** No credential value appears in any source file, test fixture, document, or commit message
  produced by this work.

## Key Technical Decisions

**KTD1 — Refuse a credential-bearing URL; do not strip it silently.** By the time `argparse` sees the
argument the leak has already happened: the value is in the process table and in shell history.
Stripping it and continuing would preserve the exact habit that leaked it and teach the operator
nothing. Refusing costs one setup step, and `talaria refresh-credential` already reduces that step to
a single command that prints nothing secret. *Rejected:* silent strip (operator never learns the
token is exposed); warn-and-continue (the warning is the part people learn to skip).

**KTD2 — `record`'s positional URL becomes optional, and means the endpoint, not the credential.**
When present it is an endpoint override passed to `AttachTarget.from_environment(override=…)`, which
is exactly what the launcher does (`talaria/cli.py:266`). When absent, `record` resolves the endpoint
the same way the launcher does. This makes the two entry points one code path rather than two that
must be kept in agreement.

**KTD3 — Detect the credential on the raw argument, before any stripping.** `AttachTarget.from_url`
strips credential query keys as an invariant (`talaria/transport/attach.py:227-231`), which is the
precise silent behaviour KTD1 rejects. The refusal check therefore inspects the operator's argument
first and raises; only a clean argument reaches the target constructor.

The set of credential query keys is `CREDENTIAL_QUERY_KEYS` (`talaria/transport/attach.py:97`), which
is `{"token"}` unioned with `URL_ONLY_DENIED_QUERY_KEYS`. Use that constant; do not restate the list,
or the refusal and the stripping will disagree the first time one of them gains a key.

**Userinfo counts as a credential too, and is refused on the same terms.** A URL of the form
`ws://user:pass@host/api/ws` carries a credential outside the query string, and this repository
already treats userinfo as a credential — `redact_url` withholds it deliberately, and says why
(`talaria/recorder/redact.py:67-71`). Keying the refusal on query parameters alone would leave a hole
with the same shape as the one this unit closes.

The consequence differs from the query-parameter case and the message should not pretend otherwise.
Hermes reads the upgrade credential only from query parameters, so a userinfo URL would not have
authenticated anyway; the operator was never going to get a working session from it. What they *did*
get is a credential in their shell history and process table. That is precisely the thing KTD1 exists
to tell them about, so refusing it silently — or not at all — would contradict KTD1's own reasoning.

This is deliberately a refusal and not a supported channel. v0.1 targets loopback `?token=` only
(`DECISIONS.md`), so no operator inside the supported envelope is stranded by it.

**KTD7 — The refusal lives in the CLI dispatch and exits 2, leaving `run_record`'s contract intact.**
`run_record`'s documented exit codes are 0 for a normal close and 1 for never-attached or a
write failure (`talaria/recorder/command.py:75-80`), a contract that mirrors the TypeScript
reference. A refusal is neither of those. Putting the check in `main`'s `record` branch
(`talaria/cli.py:163-177`) keeps that contract untouched and matches the exit code the sibling
operator-error path already uses — `run_refresh_credential` returns 2 (`talaria/cli.py:429`).

**KTD4 — The fourth bridge gets a sibling test, not a fourth parametrize row.** `terminal.read` is
answered automatically from the projection with no human in the loop
(`tests/transport/test_bridges.py:336`), so `respond_live` — the call the shared parametrize body is
built around — is not on its path. The canary reaches the field by being pushed as transcript
content that the projection then serializes. Forcing this into the shared body would mean faking a
code path that does not exist.

**KTD5 — Extend the sweep by parametrizing over credential-holding entry points, and add a
classification guard.** The defect survived because the only probe was the bare launcher. A guard
test that pins the subcommand set converts "we remembered to check" into "the suite refuses to
forget": a new subcommand reds it until someone classifies it.

**KTD6 — The document corrections state that the previous claim was an overclaim.** After U2 and U3
land, `QUEUED.md:106` would become true if left alone. Letting a false sentence quietly age into a
true one destroys the only evidence of how the defect survived. The corrected text says what is now
measured and notes what the earlier claim asserted without measuring.

## Implementation Units

Land in order. Each unit is one commit and one merge, with the project check green before the next
begins.

### U1. Prove the fourth blocking bridge redacts, over a real socket

Adds the missing coverage for `terminal.read.respond` / `text`. Test-only; no production code changes.

**Files:** `tests/transport/test_bridges.py`; `talaria/recorder/redact.py` is edited **temporarily and
reverted** for the red demonstration below — it must not appear in this unit's diff.

**Approach:** A sibling test beside `test_a_respond_value_is_withheld_from_the_recording` (KTD4).
Push transcript events carrying a canary through the stub gateway, fire `terminal.read.request`, let
the app answer from the projection, then sweep the frame log the real recorder wrote. Assert the same
four properties the existing three bridges assert: the canary is absent from the raw bytes, the field
reads `[redacted]`, the redaction reason is exactly `deny-set:terminal.read.respond`, and the rest of
the frame survived.

**Why the reason assertion matters here more than elsewhere:** `text` is one of the two fields the
key-name net does not catch (`talaria/recorder/redact.py:396`), so the deny-set entry is the sole
control. An assertion that accepted either mechanism could not tell a working rule from a deleted one.

**Test scenario:** `tests/transport/test_bridges.py` — the canary never reaches the frame log; the
`text` field is marked `[redacted]`; the reason is the deny-set entry, not the key-name net.

**Acceptance (R2):** Demonstrate the red. Temporarily remove the `terminal.read.respond` entry from
`_DENY_BY_METHOD` (`talaria/recorder/redact.py:131-146`), show the new test fails, restore the entry,
show it passes. Record the observed failure output in the commit message. A green run alone does not
close this unit.

### U2. `talaria record` resolves its credential through the chain, and refuses one on the command line

The behaviour fix. Closes the leak and rewrites the documentation that instructed it.

**Files:** `talaria/cli.py`, `talaria/recorder/command.py`, `README.md`, `docs/formats/frame-log.md`,
`docs/engineering-journal/LEARNINGS.md`

**Approach:** Make the positional `url` optional (KTD2). Before constructing anything, inspect the
raw argument for a credential query key and refuse if one is present (KTD3), exiting non-zero with a
message that names the two supported routes and tells the operator to rotate the value they just
exposed. Otherwise resolve the endpoint through `AttachTarget.from_environment(override=url)` and the
credential through a primed `LoopbackTokenProvider`, then dial `target.dial_url(credential)`.

**Factor the resolution out of the dial loop.** `run_record` currently receives a URL string and
passes it straight to `connector(url)` (`talaria/recorder/command.py:74`, `:112`). Resolution must
become a separate step so that U3's probe can construct a `record` entry point that *holds* a
credential without dialling anything.

**Preserve what already works:** the printed endpoint and the frame-log header are both redacted
today (`redact_url`) and must stay that way. `record` keeps its name and its `--out` flag; it is
referenced nine times across seven documents and is not being retired.

**Documentation, in the same commit:** `README.md:78` currently reads
`uv run talaria record ws://127.0.0.1:9119/api/ws?token=<token>` and must stop showing a
credential-bearing command, or it will tell operators to run something that now exits non-zero. The
two lines above it already document `talaria refresh-credential`, so the replacement has a supported
route to point at. `docs/formats/frame-log.md:8` names `record` as the producer of the format but
does not show its argument shape — verify only; no edit is expected there.

**Test scenarios:** `tests/test_cli.py` — a credential-bearing URL exits 2 and the message contains no
fragment of the value (R6); a bare `talaria record` resolves the endpoint the same way the launcher
does; an endpoint-only URL is accepted as an override. This module already exercises the `record`
dispatch with a fake `run_record` (`tests/test_cli.py:323-338`), so the refusal path is testable
without opening a socket.

`tests/recorder/test_command.py` (new) — the dial URL carries the credential exactly once, and the
printed endpoint and the frame-log header stay redacted. This is a new module: the existing
`tests/recorder/test_recorder.py` covers `FrameRecorder` from `talaria.recorder.framelog` only and
never imports `talaria.recorder.command`.

### U3. Measure the process surface at every entry point that can hold a credential

Closes the hole that let U2's defect survive, and corrects the documents that overclaimed.

**Files:** `tests/transport/test_process_surface.py`, `docs/engineering-journal/QUEUED.md`,
`docs/engineering-journal/DECISIONS.md`, `docs/engineering-journal/LEARNINGS.md`

**Approach:** Parametrize the existing subprocess probe over the entry points that can hold a
credential — the bare launcher, `record`, and `refresh-credential` — rather than only
`parse_args([])`. Each probe resolves a real credential and holds it in memory for the observation
window, which is what makes the sweep meaningful; a probe holding nothing passes every assertion by
having nothing to leak.

**`refresh-credential` needs a stub dashboard.** Its credential-holding moment is the fetch, so the
probe points at a loopback stub rather than a real dashboard. The mechanism already exists —
`tests/transport/test_refresh.py:67` stands up an `HTTPServer` on `127.0.0.1:0` — so the harness
starts one and passes its origin to the probe through the environment.

**This probe writes, and every existing probe only reads.** `refresh-credential` creates or updates
the credential file. The harness must set `TALARIA_CONFIG_DIR` to a `tmp_path` for this probe, as the
existing cases already do (`tests/transport/test_process_surface.py:201`, `:341`); without it the
probe would overwrite the developer's real `~/.talaria/credentials`. Assert the written path is under
`tmp_path` before asserting anything else.

**The guard (R8):** a test asserting the subparser choice set is exactly the known set. A new
subcommand reds it until someone adds it to the credential-holding list or explicitly to the
does-not-hold list.

**Acceptance for the guard — demonstrate the red.** Add a throwaway subcommand to the parser, show
the guard fails, then remove it and show it passes. Record the observed failure output in the commit
message. This is the same acceptance U1 carries, for the same reason: a guard whose red has never
been seen is indistinguishable from a guard that cannot red, and that is the defect this whole plan
exists to close.

**Keep the existing safety property:** the probe source is launched as `python -c`, so every literal
in it lands in that probe's own argv and the sweep searches argv for those strings. The expected host
is read from the environment at runtime for exactly this reason
(`tests/transport/test_process_surface.py:66-71`). Do not hard-code an endpoint, a port, or a token
into any new probe text. Never put `Surface.environ` on either side of an assertion — this is a public
repository with public CI logs, and pytest prints the operands of a failing assertion.

**Document corrections (KTD6).** Find these by their text, not by line number — this unit edits the
files it cites, so the numbers below are a starting hint that goes stale the moment work begins.

In `docs/engineering-journal/QUEUED.md` (near `:106`), the sentence beginning:

> The argv half holds and is measured: a running process built by the real launcher and holding a
> live credential shows no token…

Rewrite it to say what is now measured, across which entry points, and to record that the earlier
sentence claimed an argv guarantee the cited test did not cover.

In `docs/engineering-journal/DECISIONS.md` (near `:406`), the clause:

> Attach credentials are acquired env-first via `HERMES_DASHBOARD_SESSION_TOKEN`, then
> `~/.talaria/credentials` at `0600`, then a hidden prompt, and never appear in argv

Narrow "never appear in argv" to the acquisition chain it is actually about. While in that entry, its
*Rejected alternatives* list still rejects "the query-parameter token (reaches URLs, the frame-log
`endpoint` field, and process listings)" — a line the same entry's own amendment superseded when it
established that the query parameter is the only form Hermes accepts.

**Test scenarios:** `tests/transport/test_process_surface.py` — for each credential-holding entry
point, a running process holds a live credential and its command line carries no token, no `?token=`
URL and no endpoint; every environment entry carrying the credential is one the process was launched
with; the subcommand set matches the classified set.

## Scope Boundaries

**Out of scope — true non-goals.**

The open operator decision about credential-file versus environment-variable precedence
(`QUEUED.md:99`) is not settled here. This work routes `record` through whatever the chain is; it
does not change the chain's shape. That decision remains open and is unblocked by this plan either
way.

Retiring `talaria record` in favour of `talaria --record`. Considered and rejected — the subcommand is
referenced nine times across seven documents including the frame-log format specification.

DRIFT-02 and DRIFT-04, the other two open audit findings. Both are evidence and record-keeping
findings, not code defects, and neither blocks these.

**Deferred to follow-up work.**

Graduating the audit's drift list from the session scratchpad into a committed repository document.
It belongs in the repo, but it is not part of fixing these two findings.

Extending the `compare_shape` baseline past its top-level-only comparison (`QUEUED.md:389`), which
the audit re-confirmed as a real limitation.

## Risk Analysis & Mitigation

| Risk | Mitigation |
| --- | --- |
| U2 breaks the workflow of anyone using the documented `record` form | The refusal message names both supported routes; `talaria refresh-credential` makes the file route one command; `README.md` is rewritten in the same commit |
| The refusal path echoes the URL it is refusing, leaking the token into stderr and CI logs | R6 is an explicit test: assert no fragment of the value appears in the message. The existing code already models this — `resolve_endpoint` deliberately does not echo an unparseable URL (`talaria/transport/credentials.py:428-431`) |
| The `refresh-credential` probe proves disproportionately complex and gets quietly dropped | The stub mechanism is already proven at `tests/transport/test_refresh.py:67`. If it is dropped, R7 is unmet and the unit is not done — silently narrowing coverage is the exact failure being fixed |
| The new subprocess probes slow the suite | Each probe holds a short observation window and does not dial; the existing probe already establishes the pattern and its cost |
| U1's new test passes for the wrong reason | R2 requires demonstrating the red with the deny-set entry removed, and recording that output |

## Success Signal

`talaria record` cannot be made to carry a credential on its command line; the process-surface sweep
covers every entry point that can hold one and a new subcommand cannot bypass it; the fourth blocking
bridge's redaction is proven over a real socket and shown to fail without its deny-set entry; and no
document in the repository claims an argv guarantee broader than what is measured.

Re-running the audit's original probe — `talaria record` with a canary value, observed from `ps` in a
separate process — must find nothing.
