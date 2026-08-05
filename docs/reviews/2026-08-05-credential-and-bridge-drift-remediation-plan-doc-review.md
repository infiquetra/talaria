# Doc review — credential and bridge drift remediation plan

**Target.** `docs/plans/2026-08-05-credential-and-bridge-drift-remediation-plan.md`

**Reviewed revision.** Working tree at `7eae211` on `main` (the plan is untracked at review time).

**Blocked.** No, as of 2026-08-05. The review found one `P1`; it and the three lower findings were
all fixed after the review ran. See the status column below.

**Related artifacts.** Spec `docs/plans/2026-08-05-credential-and-bridge-drift-remediation-spec.json`;
workflow `docs/plans/2026-08-05-credential-and-bridge-drift-remediation.workflow.js`; saga
`task-talaria-drift-remediation`.

## Readiness summary

The plan can drive implementation. Its units are grounded in code that was read rather than assumed,
each cites `path:line`, and the two operator decisions it depends on were taken explicitly rather
than defaulted.

The review found no invented decisions and no unverified assumption that would change what gets
built. Five mappings were wrong or underspecified and have been corrected in place; the four
remaining findings are one execution-hygiene blocker, two consistency gaps, and one polish item.

The security lens applies throughout — the plan's subject is a credential leak — so every finding
below was checked against what an agent would actually do if it followed the document literally.

## Applied fixes

| # | Fix | Evidence |
| --- | --- | --- |
| 1 | U2's dial-URL test scenario pointed at `tests/recorder/test_recorder.py`, which cannot host it. Repointed to a new `tests/recorder/test_command.py`, and named `tests/test_cli.py:323-338` as the existing `record` dispatch test | `tests/recorder/test_recorder.py` imports only `talaria.recorder.framelog`; it never imports `talaria.recorder.command` |
| 2 | KTD3 said "a credential query key" without defining the set. Named `CREDENTIAL_QUERY_KEYS` and warned against restating the list | `talaria/transport/attach.py:97` — `frozenset({"token"}) \| URL_ONLY_DENIED_QUERY_KEYS` |
| 3 | The refusal's exit code and location were unpinned. Added KTD7: the check lives in the CLI dispatch and exits 2, so `run_record`'s documented 0/1 contract survives | `talaria/recorder/command.py:75-80` pins 0/1; `talaria/cli.py:429` already uses 2 for an operator error; exit 2 is the behaviour the operator approved |
| 4 | U1's file list omitted `talaria/recorder/redact.py`, which the red demonstration must edit. Added to the plan and the spec, marked temporary-and-reverted | The spec's `files` list is the concurrent-writer collision oracle; an undeclared write is invisible to it |
| 5 | U3's `refresh-credential` probe writes where every existing probe only reads. Required `TALARIA_CONFIG_DIR` be set to a `tmp_path`, and an assertion that the written path is under it | `tests/transport/test_process_surface.py:201`, `:341` already establish the sandbox pattern |

Fix 5 was the one that mattered. Without it an agent following the plan literally would have written a
probe that overwrites the developer's real `~/.talaria/credentials` on every test run.

## Remaining findings

All four were resolved on 2026-08-05, after the review, at the operator's instruction to fix
everything rather than only the blocker.

| Key | Priority | Finding | Status |
| --- | --- | --- | --- |
| D1 | P1 | Uncommitted working-tree state will contaminate the units' commits | **Resolved** — journal edit and plan artifacts committed on their own branch; `.qwen/` removed |
| D2 | P2 | Userinfo-borne credentials fall outside the refusal, inconsistent with KTD1's own rationale | **Resolved** — R5 and KTD3 extended to refuse userinfo on the same terms |
| D3 | P2 | U3's guard test carries no red demonstration, though U1's does | **Resolved** — R8 and U3 acceptance now require demonstrating the red; spec returns a `guard_red_demonstration_output` |
| D4 | P3 | Line-number citations will drift as the cited documents are edited | **Resolved** — U3's corrections now quote the sentences to change, with line numbers demoted to a hint |

**How D2 was decided.** The review named two defensible options — extend the check, or state the
exclusion. Extending it was chosen because KTD1's stated purpose is to *tell the operator their value
is exposed*, and staying silent on a shape the repository already classifies as a credential
(`redact_url` withholds userinfo, `talaria/recorder/redact.py:67-71`) would contradict that purpose.
Nobody inside the supported envelope is stranded: v0.1 targets loopback `?token=` only, so a userinfo
URL was never a working channel. The plan now says this explicitly rather than leaving the reasoning
in this review.

### D1 (P1) — uncommitted working-tree state will contaminate the units' commits

The working tree carries an unrelated modification to `docs/engineering-journal/LEARNINGS.md` (the
qwen-code prior-art entry) and an untracked `.qwen/` directory. The plan's own three artifacts are
also untracked.

U2 and U3 both edit `LEARNINGS.md`. An agent that stages that file will sweep the prior-art entry into
a credential-fix commit, and the repository's standing rule that journal entries ship with their
change makes staging it the natural move.

**Resolution.** Commit or stash the `LEARNINGS.md` edit and commit the plan artifacts before `/work`
starts; remove or ignore `.qwen/`. This is the audit's leftover housekeeping, already outstanding.

### D2 (P2) — userinfo credentials fall outside the refusal

KTD3 keys the refusal on `CREDENTIAL_QUERY_KEYS`, which covers query parameters only. A URL of the
form `ws://user:pass@host/api/ws` carries a credential in its userinfo, which the repository already
treats as a credential — `redact_url` withholds it deliberately (`talaria/recorder/redact.py:67-71`).
Such a URL would not be refused.

The consequence is bounded and worth stating precisely: Hermes reads the upgrade credential only from
query parameters, so a userinfo URL would not authenticate anyway. The operator gets an auth failure
rather than a working-but-leaking session. The gap is therefore not a live leak path but an
inconsistency in KTD1's stated reasoning — the plan refuses in order to *tell the operator their value
is exposed*, and this shape leaves them untold.

**Resolution.** Either extend the check to a URL carrying userinfo, or state in KTD3 that userinfo is
deliberately out of scope and why. Both are defensible; leaving it unsaid is not.

### D3 (P2) — U3's guard test carries no red demonstration

U1 must prove its new test fails when the deny-set entry is removed (R2), because this whole audit
exists to catch tests that pass for the wrong reason. KTD5 introduces a guard test asserting the
subcommand set matches the classified set — a test whose entire value is that it reds when someone
adds a subcommand. R8 requires the behaviour but not the demonstration.

**Resolution.** Extend R8, or U3's acceptance, to require adding a throwaway subcommand, observing the
guard fail, and removing it — the same shape as U1's acceptance.

### D4 (P3) — line-number citations will drift

The plan cites `QUEUED.md:106`, `DECISIONS.md:406` and `README.md:78`. U2 and U3 edit all three, so
by the time a later reader follows these citations the line numbers will be wrong.

**Resolution.** No action needed before execution. When the units land, prefer quoting the sentence
being corrected over citing its line.

## Residual risk from limited evidence

Two things were not verified by execution and rest on reading.

The `refresh-credential` probe is asserted feasible because `tests/transport/test_refresh.py:67` runs
a loopback `HTTPServer` stub. That the mechanism exists is verified; that it composes with a
subprocess probe holding the fetched credential across an observation window is reasoned, not
demonstrated. If it proves disproportionate, R7 is unmet and the unit is not done — silently
narrowing coverage is the precise failure this plan exists to fix.

The claim that `talaria record` is referenced nine times across seven documents was measured with a
literal-string search. A reference phrased differently would not have been counted, so nine is a
floor rather than an exact figure. It is used only to reject retiring the subcommand, which a floor
supports.
