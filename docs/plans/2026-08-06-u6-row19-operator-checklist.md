---
title: U6 operator checklist — row 19's remaining live-evidence branches
type: checklist
status: ready-for-operator
date: 2026-08-06
source: docs/plans/2026-08-06-model-picker-and-v0-1-closure-plan.md (unit U6, KTD9)
---

# Operator checklist — row 19 live acceptance run

This checklist is the deliverable of unit U6. It cannot be executed by an agent: KTD9 (the plan's
rule that row 19's branches are a checklist a person runs, not a thing an agent can simulate) and
step 6 both require a human watching the screen. This document exists to hand the operator exactly
what to run, what to record, and where the evidence lands. Complete each step under
`talaria --record` and keep the resulting corpus.

## Preconditions

- A live Hermes gateway reachable over the transport (not a stub).
- Talaria built from the tree that has landed U1–U5 of this plan (U6 depends on U5, and through it
  U1–U4), and after U3 specifically, because U3 rewrites the authentication-failure refusal text that
  step 3 below records — running this checklist against the pre-U3 wording records evidence for a
  Talaria that no longer exists.
- `talaria refresh-credential` available, for step 3's restoration.
- A throwaway/isolated session identity you are willing to discard after step 5.

## Steps

1. **Startup precedence — `--resume` and `--session <id>`.**
   Launch Talaria twice: once with `talaria --record --resume`, once with
   `talaria --record --session <id>` (pick or create a known session id). For each launch, record
   which session it actually lands in. Neither path has ever run against a live Hermes gateway; this
   settles whether the startup precedence chain named in KTD7 of the 2026-08-02 prototype plan
   (`--session <id>` beats `--resume` beats default-new) holds live. Note: this is a different KTD7
   than this plan's own KTD7, which concerns model-cost confirmation, not startup precedence — do not
   conflate the two in the writeup.

2. **Compatibility check's on-screen output.**
   Under `talaria --record`, capture the real on-screen output of the compatibility check: how many
   of the five read-only startup probes come back `present`, and whether `spawn_tree.list` is refused
   against the running fixture. Record the literal on-screen text, not a paraphrase.

3. **Authentication-failure branch.**
   Force a stale credential (e.g. restart the Hermes gateway process so the current
   `~/.talaria/credentials` token goes dead — see MEMORY.md: "a Hermes restart invalidates the
   talaria credential"), launch Talaria under `--record`, and observe the refusal on screen verbatim.
   Then restore access with `talaria refresh-credential` and confirm the next launch succeeds.

4. **Absent-capability branch.**
   Reach the branch through U1's 404 handling (drive Talaria against a gateway/profile missing the
   capability U1 checks for) and record the on-screen behavior under `--record`.

5. **F1 end to end, isolated session.**
   In a throwaway session nobody else is using, run F1 (first run) end to end under `--record`:
   authenticate against the real Hermes dashboard, confirm all five read-only startup probes of row 1
   answer, and land in a session. This is the isolated-session run row 19 currently lacks — the prior
   corpus (`talaria-live-corpus-v1-2659f-bd69e537f1d9`) exercised the bare startup path but never in
   an isolated throwaway session and never with `--resume`/`--session` present.

6. **F7 — gateway survives Talaria's exit (person-observed, not automatable).**
   Before attaching Talaria, sample the Hermes gateway process's PID and/or start time. Run Talaria
   normally, then exit it. Immediately after exit, sample the gateway process's PID/start time again
   and confirm by direct observation (not a frame log — the log ends at the exit being tested) that
   it is still serving and is the same process. Record both samples verbatim (timestamps, PID,
   command used to sample, e.g. `ps`).

## What to hand back after running this

- The `talaria --record` output directories/corpus identifiers for each step above.
- For each recording: sha256 digest and frame count (per R11 of the plan — corpora are cited by
  digest and count, never by local path; do not include operator-specific profile names/paths per
  R12, this is a public repository).
- The literal on-screen text captured in steps 2 and 3.
- The two process samples from step 6, with the observation method stated.
- Whether each of steps 1–6 passed, and if not, exactly what was observed instead.

## After the operator returns evidence

Digest and count the recordings (R11), fold them into row 19's evidence in
`docs/analysis/2026-08-02-v0-1-daily-driver-verdict.md`, and hand off to U7 (re-grade the gate) which
also re-grades row 6 against the methods actually called and row 13 per U3's decision. Do not restate
the verdict table until the operator's evidence for all six steps is in hand.
