---
title: Credential and bridge drift remediation — execution
date: 2026-08-05
plan: docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md
branch: fix/credential-and-bridge-drift
status: pr-ready
---

# Credential and bridge drift remediation — execution

Executes the three units of the drift remediation plan. Backend was
`cc-workflows-ultracode` — the operator's recorded pick over an `inline`
recommendation — run as workflow `wf_3cc744e4-d85`, three agents chained
`U1 → U2 → U3`, no parallel fan-out.

## What was built

**U1 — the fourth blocking bridge.** `tests/transport/test_bridges.py` gains
`test_a_terminal_read_value_is_withheld_from_the_recording`. The canary reaches
`terminal.read.respond` / `text` as transcript content the projection serializes,
not through `respond_live`, because `terminal.read` is answered automatically
from the projection and never crosses that call. The reason assertion pins
`deny-set:terminal.read.respond` specifically, so a test passing via the
key-name net instead could not masquerade as this one — which matters because
`text` is one of the two fields the key-name net does not catch.

Red demonstrated: with the deny-set entry removed, the test failed at
`assert CANARY.encode() not in raw`. Restoring it returned `redact.py` to a zero
diff, verified against git rather than taken on report.

**U2 — the credential leak.** `talaria record`'s positional argument is now
optional and means the endpoint, never the credential. Resolution moved out of
the dial loop into `resolve_record_target()`, which returns a `RecordTarget`
holding a credential-free `AttachTarget` and a `Credential` as separate halves,
joined only in `dial_url()`. That split is what lets U3's probe build a `record`
entry point that holds a credential without dialling.

A URL carrying a credential in its query string **or** its userinfo is refused
in the CLI dispatch with exit 2, before anything is constructed — `AttachTarget.
from_url` strips credential query keys as an invariant, which is the silent
behaviour KTD1 rejects, so the check has to run first.

One adjacent leak surfaced during the work and was closed: `websockets` writes
the whole URI into `InvalidURI`'s message, so a failed attach was interpolating
the credentialed URL into its own error. That now routes through
`describe_dial_error`.

**U3 — the sweep and the documents.** The process-surface probe is parametrized
over `ENTRY_POINTS` — launcher, `record`, `refresh-credential` — where before it
built one probe from `parse_args([])`. Each probe holds a live credential for
the observation window; a probe holding nothing passes every assertion by having
nothing to leak. `refresh-credential` writes where every other probe reads, so it
runs against a loopback stub with `TALARIA_CONFIG_DIR` redirected to `tmp_path`,
and the written path is asserted under `tmp_path` first.

`test_the_subcommand_set_is_exactly_the_classified_set` reads the live subparser
choices off `build_parser` and fails on any unclassified subcommand. Red
demonstrated: a throwaway subcommand failed the guard with
`unclassified=['throwaway-red-demo']`; removing it restored green.

`QUEUED.md` and `DECISIONS.md` now state what is measured **and** record that the
earlier wording was an overclaim, per KTD6 — a true sentence with a false history
reads exactly like a true sentence.

## Two defects the agents introduced, found in review

Both were caught by inspecting the tree rather than reading the agents' reports.

1. **U1 and U3 never committed.** Only U2 committed (`b06718d`); the other two
   left their work loose in the working tree. Committed here as `1c462c7` and
   `1046e78`. U2's commit did *not* sweep U1's uncommitted file in — the D1
   hygiene finding held.
2. **U3 filed its journal entry at the end of `LEARNINGS.md`**, under the
   `2026-08-03` heading, in a file whose convention is newest-first. Relocated to
   `2026-08-05` and given the `**Author.**` line 83 other entries carry. U2's
   entry also named the wrong finding — it credited DRIFT-01, which is the bridge
   coverage gap; the credential leak is DRIFT-03. Corrected.

## Verification beyond the suite

The plan's Success Signal is the audit's original probe re-run by hand, since the
defect was found by running the program rather than reading it.

- A credential-bearing URL (query string) exits 2. Its message reproduces neither
  the value, the host, the port, nor the `token=` form — 0 occurrences across
  stdout and stderr.
- A **running** `talaria record` holding a live credential from the file route was
  observed from a separate process with `ps -ww -Ao pid,command`. Its command line
  was `.venv/bin/python .venv/bin/talaria record --out <path>` — zero matches for
  the credential, `token=`, or `ws://`.

One confound worth recording: the first pass of the hand probe reported a
credential on a command line. It was the probe harness's own shell, whose argv
held the whole script text. This is the same hazard the suite's probes avoid by
reading the expected host from the environment instead of hard-coding literals —
and it is why a crude "does the string appear anywhere in `ps`" check is not the
measurement, attribution to a PID is.

## Code-review gate

Ran programmatically at `23ee6ac`. **One P1, now fixed** in `572990e`; re-gated clean.

**The finding.** A credential in the endpoint's URL *fragment* was refused by
nothing. `url_carries_credential` read `netloc` for userinfo and
`parse_qsl(query)` for credential keys, and `urlsplit` puts everything after `#`
in `fragment`. Reproduced end to end before fixing: the command was accepted, the
canary was echoed to the terminal twice, and it was written into the frame-log
header verbatim.

Of the three shapes a credential can ride in on, the fragment was the only one
caught by nothing — `strip_credential_query` drops credential query keys,
`redact_url` withholds userinfo, neither touches a fragment — so it was also the
only one that reached disk. Any fragment is now refused outright; a WebSocket
endpoint has no legitimate use for one and `websockets` rejects such a URI
anyway. The redactor's own fragment blindness is pre-existing and reaches
`AttachTarget.url` from the environment and credential file too, so it is filed
as P2 in `QUEUED.md` rather than widened here — that change alters what the
frame-log format promises, which is a decision rather than a fix.

> **Closed 2026-08-05, after this session.** That P2 was taken and the entry has
> left `QUEUED.md`. `strip_credential_query` now drops the fragment and
> `redact_url` withholds it, making the frame log cover all three positions a
> credential can ride a URL in. The format promise and the reasoning are in
> `DECISIONS.md`, "A URL fragment is withheld whole in a recording and dropped
> outright from a dialled endpoint".

**The finding underneath the finding, which matters more.** Writing the test for
the fix exposed that the refusal's existing tests did not test the refusal.
Replacing the check's condition with `if False:` left **seven of its eight tests
passing**. `run_record_command` exits 2 for two unrelated reasons by KTD7's own
design — the refusal, and a credential the chain cannot supply — and under pytest
the chain supplies nothing and cannot prompt, so it raised `CredentialError` and
exited 2 by the other route. The R6 tests were worst affected: "the refusal does
not echo the credential" is vacuously true of any output that is not the refusal.

Every refusal test now asserts `REFUSAL_SIGNATURE` before anything else. The same
deletion now fails 7 of 8, exactly inverting the original result; removing only
the fragment branch fails exactly the 2 fragment rows. Recorded in `LEARNINGS.md`
— the generalizable rule is that an exit-code assertion tests nothing when more
than one route reaches that exit, and a test harness with no credential, no
terminal and no network reaches precisely the routes that mimic a refusal.

## Files modified

`talaria/cli.py`, `talaria/recorder/command.py`, `talaria/transport/credentials.py`,
`README.md`, `tests/transport/test_bridges.py`,
`tests/transport/test_process_surface.py`, `tests/recorder/test_command.py` (new),
`tests/test_cli.py`, `docs/engineering-journal/{QUEUED,DECISIONS,LEARNINGS}.md`

## Checks run

Against the committed tree, all green: `ruff` clean; `mypy` 106 source files, no
issues; `pytest` **1073 passed, 1 skipped** (up from 1046 passed, 1 skipped);
`bandit` exit 0; `git diff --check` clean.

R10 was verified directly rather than assumed: the developer's real credential
value appears **zero** times across the committed diff, the working diff, and all
three commit messages. Every long literal introduced is a self-describing canary
(`NOT-A-REAL-*`). The real `~/.talaria/credentials` was not written by any probe.

## Deviation from the plan

The plan says each unit is "one commit and one merge." All three landed as one
commit each on one branch, for a single PR — the workflow chained them in one run
with the project check green between units, which preserves the property the
merge boundary was for. Commit order is U2, U1, U3 rather than U1, U2, U3,
because U2 was the only unit that committed itself. History was not rewritten to
make the plan's sequencing claim retroactively true.

## Next step

Open the PR. The gate is clean and the review is fresh at `572990e`.
