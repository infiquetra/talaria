# /work launch handoff — Talaria v0.1 prototype (unattended)

Written 2026-08-02. Launch branch: `main` (`53445c7` at time of writing). This file is the
operator's pre-recorded answer sheet for the launch-time ceremony of an **unattended** `/work` run:
the operator starts the session, confirms launch, and leaves. Nothing below invents new policy —
it points at the plan and records choices already made, so the session never blocks on a question
the operator has already answered. If a launch step would normally raise `AskUserQuestion` for
something answered here, take the answer from here instead.

## Invocation

The operator starts the session with `/saga:work` and the plan path as argument. Everything else in
this file is context for that run.

## What already exists — read in this order

1. **Plan (authoritative):** `docs/plans/2026-08-02-talaria-v0-1-prototype-plan.md`. Its
   **"Unattended execution contract"** section governs the entire run — credentials, the
   harness-launched acceptance gateway, corpus rules, push/PR/merge cadence, halt-vs-degrade
   semantics, pinned reads. Where this handoff and the plan differ, the plan wins.
2. **Execution spec:** `docs/plans/2026-08-02-talaria-v0-1-prototype-spec.json` — 10 units,
   validated (`OK: talaria-v0-1-prototype (10 units)`), per-unit model/effort tiers recorded,
   `returns` fields are snake_case key lists (see the 2026-08-02 LEARNINGS entry; do not revert
   them to prose).
3. **Doc-review artifact:** `docs/reviews/2026-08-02-talaria-v0-1-prototype-plan-doc-review.md` —
   `ready_for_work: true`, all thirty findings (D1–D30) dispositioned across four rounds. The
   doc-review gate is satisfied; do not re-run the review.
4. **Workflow preview:** `docs/plans/2026-08-02-talaria-v0-1-prototype.workflow.js` — a committed
   preview only; the backend re-emits fresh from the spec at launch per the skill's freshness rule.
5. **Active saga:** `task-talaria-v0-1-prototype` under `.claude/saga/` (git-ignored — never
   `git add` it). Restore and continue this saga; do not mint a duplicate.

## Pre-answered launch ceremony

- **Orchestration backend:** `cc-workflows-ultracode`. The return-gate defect that previously
  forced an inline recommendation was root-caused to the spec's `returns` typing and fixed on
  2026-08-02 (PR #8); the launch-time re-emit now produces sane gates. The emit-time concurrency
  check passes at `aggregate_max_concurrent: 3`.
- **Destination:** `merge` — the operator authorized merge-to-main at revertible milestones.
- **Ship ceremony:** this is `--kind task` work with no issue ref; skip the front-loaded draft-PR
  offer. Follow the contract's push/PR/merge cadence instead (U1 scaffold; end of milestone 1
  after the gate verdict; end of milestone 2 after the daily-driver verdict).
- **External engine offload:** decline for this run. All units execute natively in the workflow;
  coordinating external engines adds unattended failure modes the operator has not accepted.
- **If the Workflow tool is genuinely absent from the session** (the skill's HALT condition 1):
  the operator pre-authorizes falling back to the `inline` backend, recorded with an
  `orchestration_downgrade` note citing this handoff — an unattended halt that delivers nothing
  is worse than the downgrade. This pre-authorization covers only tool absence, not any other
  halt condition.

## Standing operator authorizations (granted 2026-08-02, recorded in the plan contract)

- **Push, PR, and merge to `main`** at revertible milestones; every PR body and merge message
  good enough to revert from alone.
- **Corpus self-capture** on `deepseek-v4-flash` (cheapest configured model as substitute,
  recorded with the corpus label) — but an operator-supplied corpus of ≥5,000 frames wins if one
  exists where the run's configuration points. Check for one before capturing.

## Unattended conduct — summary only; the plan contract is normative

- No interactive blocking on any execution path; credentials via environment or `~/.talaria/`.
- Halt vs degrade: a U5 gate failure **halts** with the results doc and U4 routing; U10 with a
  missing or drifted method completes with an honest **not-ready** verdict; live provocation is
  attempt-bounded and downgrades to stub-verified-only, recorded, never reported as live-verified.
- Mid-run Hermes reads are pinned: `git -C ~/.hermes/hermes-agent show 7f4d15515:<path>`.
- Concurrency: total in-flight agents ≤ 3 (the spec encodes this; any ad-hoc subagents count
  against the same cap).
- The TypeScript tree under `src/` is superseded bootstrap code — never extend it; throwaway
  capture tooling stays outside it.
- Public repository: no secrets, no private operational context, no session identifiers in
  anything committed. No attribution lines in commits or generated content.
- On full completion, or on any halt, write the closing state into the saga tick and — if a push
  notification tool is available — send the operator a one-line completion or halt notice.

## What done looks like

All ten units complete with their return-contract evidence, milestones merged to `main` per the
cadence, and either a daily-driver **ready** verdict or an honest not-ready/halt with the evidence
documents in place. Working software and a fully completed plan — or a truthful account of exactly
where and why it stopped.
