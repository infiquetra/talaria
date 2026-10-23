# PH3: Phase 3 planning specification (the fleet)

Status: `proposed`

This document holds the depth of the phase 3 capability issue `{{PH3}}`: the ledger's proofs, the comment formats its Verification reads, the meaning of each dependency gate, and the details of planning steps, rulings and authority that the issue only points to. The issue body wins on anything stated in both; if the two disagree, stop and report on `{{PH3}}`. It is committed to `docs/handoff/2026-10-22-next-steps/units/PH3.md` with the ten child specifications (planning step 0).

## 1. Requirement ownership ledger with proofs

Protocol rules behind the rows: L-44, L-48, L-49, L-51, L-52 rule 8; L-45 rule 10 by decision 4; L-46 rule 3; L-47 rules 1, 2; L-50 rules 4, 5.

Proof is the owner's test numbers and scenarios (U3 means the owner's UAT-3). The closeout's line for each row names the owner and one of these (`| L-n | <owner> <test or scenario> <evidence URL> | <main commit> | PASS |`).

| Row | Requirement | Owner | Proof |
|---|---|---|---|
| L-1 | Live sessions of all served profiles (if host scope is refused, only the foreground one, as `ProfileOnly` with its reason) | P3-01 | 27, 28, 31; U1, U6 |
| L-2 | States needs you, working, failed, done, idle | P3-01 | 4-7, 19, 34, 35; U1, U4 |
| L-3 | One needs-you queue; stable item identity across sources | P3-01 | 8-14, 32, 41; U2 |
| L-4 | A question asked while Talaria was closed | P3-01 | 17; U3 |
| L-5 | `talaria fleet list` text, JSON, `--watch`; exit codes | P3-01 | 44-47; U5 |
| L-6 | Overlay grouped by state; leaves nothing behind | P3-02 | 1-5, 34-39; U1 |
| L-7 | Peek: latest output or the open question | P3-02 | 6, 15-17, 36, 43; U1 |
| L-8 | Reply queued, never interrupting | P3-02 | 7, 18-20, 44; U2 |
| L-9 | Answer an approval from the peek panel | P3-02 | 11, 24, 45; U3 |
| L-10 | Stop a turn after confirming | P3-02 | 8, 22, 23, 46; U4 |
| L-11 | Open as the foreground session | P3-02 | 9, 27; U5 |
| L-12 | `--bg` returns at once, keeps model flags; the turn outlives it | P3-03 | 5, 30, 51; U1 |
| L-13 | `n` and `w` launch from the fleet view | P3-03 | 41-43, 50; U3 |
| L-14 | Worktree from fetched `origin`; edits stay in it | P3-03 | 18-29, 52; U2, U5 |
| L-15 | Background approvals wait in the queue (decision 5) | P3-03 | 36, 53; U4 |
| L-16 | Subagent dock, panel, detail, tail, batch block | P3-04 | 1-17, 39-45, 49; U1, U2 |
| L-17 | Steer and stop a child; stop all | P3-04 | 23-26, 37, 50, 51; U3, U4 |
| L-18 | Pause new spawns, then resume | P3-04 | 27, 36, 52; U6 |
| L-19 | Processes: list, live output, kill, `/stop` | P3-05 | 1-14, 24-27, 47, 48; U2, U3 |
| L-20 | `/bg` answer as one cell | P3-05 | 15-23, 45; U1 |
| L-21 | Bell and notification for background needs-you, done, failed | P3-06 | 1-16, 26-28, 34; U1, U3, U4 |
| L-22 | Status-line counters | P3-06 | 17-19, 23; U1 |
| L-23 | Jump to the oldest waiting request | P3-06 | 20, 21, 24, 25, 31; U2, U6 |
| L-24 | One start-up notice for waiting requests; no bell burst | P3-06 | 10-12, 29; U5 |
| L-25 | Review: list, diff, stage, discard, commit | P3-07 | 1-13, 19-22, 43-46; U1, U5 |
| L-26 | Commit message from the session's model | P3-07 | 30, 31, 39, 47; U2 |
| L-27 | Push and draft pull request; never forced; refusals | P3-07 | 14, 18, 23, 24, 26; U3, U4 |
| L-28 | Pull-request label on fleet rows and status line | P3-07 | 15-17, 27, 34, 35; U3 |
| L-29 | `talaria attach` by id, id start or title | P3-08 | 5-24, 42; U1, U2 |
| L-30 | Open in a new Herdr or tmux pane | P3-08 | 25-39; U3-U5 |
| L-31 | Resident agents pinned in the fleet view | P3-09 | 1-28, 42-45, 50-54; U1-U7 |
| L-32 | Three providers while the operator codes in a fourth | PH3-ACC | OP-1, OP-2, XF-2 |
| L-33 | A worktree session ends in a draft pull request | PH3-ACC | OP-4, XF-4 |
| L-34 | Parity verdict PAR-1 to PAR-6 | PH3-ACC | OP-8 |
| L-35 | Registry `start` 1 ms; 30 profiles of 200 rows in 1 s | P3-01 | 27, 39, 48 |
| L-36 | First frame 50 ms; keystrokes at 500 rows; idle zero frames | P3-02 | 40-42 |
| L-37 | `--bg` exits within 300 ms; a slow fetch never blocks the screen | P3-03 | 39, 49 |
| L-38 | Subagent budgets; no frame or call while nothing is live | P3-04 | 20, 47, 48 |
| L-39 | Output memory caps; an idle app wakes and draws nothing | P3-05 | 10, 42, 44 |
| L-40 | Unchanged counts request no frame | P3-06 | 30 |
| L-41 | Review list 300 ms at 200 files; keys while git or `gh` runs | P3-07 | 38, 42 |
| L-42 | A slow pane split never blocks keys | P3-08 | 41 |
| L-43 | All budgets with all features on; idle cost; memory | PH3-ACC | AUTO-4, XF-9, OP-7 |
| L-44 | A replaced connection's replies change nothing | P3-01 | 15, 23, 37 |
| L-45 | Fleet link never attaches; reads only live sessions | P3-01 | 33, 38, 40 |
| L-46 | Outcome from the turn's status, not event arrival | P3-01 | 3, 34, 35 |
| L-47 | Peek link never refuses a request; answers echo its ids | P3-02 | 24, 25, 32 |
| L-48 | A lost reply is unknown; only the operator resends | P3-02 | 12, 20 |
| L-49 | A lost launch submit: exit 1, tree kept, session never ended | P3-03 | 11, 35 |
| L-50 | Rows by `subagent_id`; terminal kept; no late child swallowed | P3-04 | 4, 5, 30 |
| L-51 | Kills and stops never retried; a fresh list follows | P3-05 | 26, 40 |
| L-52 | A lost resident create is never retried | P3-09 | 37 |
| L-53 | Fleet and peek text literal; no ticket, path or reply logged | P3-02 | 31, 38 |
| L-54 | Subagent text literal | P3-04 | 44 |
| L-55 | Process output and `/bg` answers literal | P3-05 | 36 |
| L-56 | Alert bytes escape-free | P3-06 | 22 |
| L-57 | git never prompts; tracked files and `.gitignore` untouched | P3-03 | 17, 24 |
| L-58 | `gh` gets an allow-listed environment; recordings public-safe | P3-07 | 25, 50 |
| L-59 | Pane command quoted; this binary; only the pane's shell | P3-08 | 27, 31, 35 |

**Rows added by D-8.** When Ruling D-8 gives a "Not delivered" gap an owner child, the planner adds a row to the issue's ledger and to this table, numbered from L-60 upward in the issue's three-column form (`| L-60 | <requirement> | <owner key> |`), adds the owner's tests and scenario to that child's body, and, when the gap needs a live check, a row to `{{PH3-ACC}}`'s specification section 4 by its section 8.1 item 4 before the freeze. AC-10 then prints the new row count (59 plus the added rows) and checks the new rows like the others. An accepted gap gets a `docs/engineering-journal/QUEUED.md` entry in the closeout pull request and no ledger row.

## 2. Comment formats and who posts them

Verification reads only comments on `{{PH3}}` whose author is an OWNER or MEMBER of `infiquetra/talaria` (GitHub's `author_association`). Every comment below must therefore come from such an account.

| First line | Posted by | When |
|---|---|---|
| `Base: <40 hex>` | lead | after `{{PH2-ACC}}`'s operator check |
| `Decision 8: reading (a)` or `Decision 8: reading (b)` | planner | step 2 |
| `Settled C-<n>: <decision>`; C-3 as `Settled C-3: limit <n> lines; <decision>` | planner | step 7, before the first child pull request |
| `Revised C-<n>: <decision>`; C-3 as `Revised C-3: limit <n> lines; <decision>` | planner | later; the latest wins |
| `Journal expected: <heading>` (one line per heading; several may share one comment) | planner | step 7 |
| `Ruling D-<n>: <decision>` | operator | before each deadline (section 7) |
| `Ruling merge: <scope>`, `Ruling bench: granted`, `Ruling decision 5: P6-01 joins phase 3` | operator | any time |

The lead and planner post with the account the operator names in the Per-run inputs' Account column. If that account is not OWNER or MEMBER, the operator either adds it to the organisation or reposts each comment verbatim from their own account; an agent never posts as the operator without that instruction. If neither happens, AC-3 can never pass: stop and report.

## 3. Gates

| Gate | Before the other issue merges, the child may | It may not |
|---|---|---|
| start | nothing: no branch, no `test(...)` commit | start at all |
| content | branch, write its tests against stubs, build what does not call the other's names | call the other's names or merge |
| merge | build in full | merge |

A `-.-` line is coordination only: the pair read each other's settled text (C-4: `{{P3-06}}` and `{{P3-07}}`), and neither waits.

At step 7 the lead records each edge of the issue's graph on the child it gates, in two ways. First, a line in the child's Dependencies section, `- Gate (phase 3 plan): <start|content|merge> after {{ID}}` (for `{{P3-07}}`: `- Gate (phase 3 plan): start after {{P3-03}}; merge after {{P3-02}}`). Second, for start and merge gates, a GitHub "blocked by" issue dependency to the gating issue, written by Mission Control and read back. Under reading (b) `{{P3-09}}` is also a start gate of `{{PH3-ACC}}`; under reading (a) it is no gate.

## 4. Staffing

The Developer row's simultaneous cap is the operator's number. Three are suggested: one each for lanes A, B with C, and D. The cap counts developers working at once anywhere in the slice. Pull requests that touch the shared files (`app.rs`, `keys.rs`, the mock gateway, `fleet_view/state.rs`) merge one at a time; the later one rebases and keeps both sides.

Custody of shared code: on a conflict the later pull request keeps both sides. `fleet/` is {{P3-01}}'s ({{P3-09}} adds pins); `fleet_view/` {{P3-02}}'s (others add variants and arms only); `worktree/` {{P3-03}}'s; `status/segments.rs` takes {{P3-06}}'s and {{P3-07}}'s segments (C-4).

## 5. Planning step details

- Step 2: `{{P0-01}}` records decisions 5 and 8 as ticked checkboxes in its body ("Decision 8: accept the recommendation (reading (a) ...)" or "choose the alternative (reading (b) ...)"), confirmed by its `decisions recorded` comment. The planner copies the reading into the `Decision 8:` comment; Verification's `D8` is that letter.
- Step 8: `{{P1-01}}`'s attach example prints a line `gateway code_version=<v> code_sha=<12 characters>` (its REQ-18 and UAT). From the repository root:

```bash
OUT=$(cargo run -q -p hermes-client --example attach -- --no-ensure --profile fixture); echo "exit=$?"
S=$(echo "$OUT" | sed -nE 's/^gateway code_version=.* code_sha=([0-9a-f]{12})$/\1/p')
P=$(sed -nE 's/^commit=([0-9a-f]{40})$/\1/p' crates/hermes-contract/PINNED | cut -c1-12)
[ -n "$S" ] && [ "$S" = "$P" ] && echo "code_sha=$S matches PINNED" || echo "STOP: code_sha '$S', PINNED '$P'"
```

A non-zero exit, no `code_sha` line or a mismatch is a stop; the comment holds the last line and `exit=`.

## 6. Test-first and evidence formats

Each child body states its own commit scope, branch and checks; these are the shared forms.

- First commit `test(<scope>): ...`, where scope is the main crate or tool the child touches (`core`, `app`, `tui`, `cli`, `smoke`, `acceptance`).
- `Red run` section of the pull request body: the command, then each test with the reason it failed; expected passes (test-first rule 1) are listed under their own line.
- `Criteria` section: each `AC-<n>`, its check command and its output, run on the pull request head.
- `bench`: a green run of `bench.yml` on the head. Agents start it only after `Ruling bench: granted`; otherwise the operator starts it, and the pull request waits in `Code review`.
- Scenario evidence: `UAT-<n>: PASS` with the text of `herdr pane read`, in the forms of `{{P1-13}}`'s Herdr facts H1 to H14, redacted.

## 7. Rulings

| Ruling | Deadline (AC-11 checks it) | What it answers |
|---|---|---|
| D-1 | before `{{P3-01}}` merges | its question 1, the fleet link as an exception to draft ADR 0007; question 2 (2 s hot, 4 s cold polls, 30 s backstop) keeps the draft unless answered; question 3 is an upstream ask (a "session started waiting" signal, `omit_messages` on attach) |
| D-2 | before `{{P3-02}}` merges | its question 1, the peek link and its ignore rule (`{{P3-03}}` decision 7 and `{{P3-09}}` decision 6 depend on it); questions 2 (close or end sessions) and 3 (a default key) keep the draft |
| D-3 | before `{{P3-03}}` merges | its questions 1 to 4: tree paths, linked folders, Hermes's sweep, a failed fetch, `launch_abandoned` |
| D-4 | before `{{P3-05}}` merges | the `/background` alias and the double press for `x` |
| D-5 | before `{{P3-07}}` merges | the throwaway GitHub repository and `gh` account (its test 41, UAT-3); filter drivers keep the draft |
| D-6 | before `{{P3-09}}` merges, reading (b) only | its questions 1 and 2 |
| D-7 | before `{{PH3-ACC}}`'s freeze (AC-11 can only check that it precedes the `p3-acc-record` pull request; the reviewer checks it precedes the AUTO-1 evidence) | `{{PH3-ACC}}` specification section 10.2: Q1 the fleet days' numbers; Q2 the three providers with a cheap model each; Q4 the XF-9 limits and the 1200 s deadline; Q7 the second Ollama Cloud model; Q8 the record shape; and the rows dropped by its section 8.1 item 4 |
| D-8 | before the first child pull request (step 7) | each of the three gaps: R6's cost, "steered" and "closed"; Herdr pane state for background requests (`{{P3-06}}` question 2); alerts for `{{P3-05}}`'s signals |

Line formats AC-11 matches: `Ruling D-7: Q1 <answer>; Q2 <answer>; Q4 <answer>; Q7 <answer>; Q8 <answer>; dropped rows <none or ids>` and `Ruling D-8: cost, steered, closed: <accepted or owner key>; pane state: <...>; process alerts: <...>`. A ruling that only confirms defaults still says so per item.

## 8. Authority details

- Notes branches: exactly `p3-01-fleet-registry`, `p3-02-fleet-view`, `p3-03-background-launch`, `p3-04-subagents`, `p3-05-background-work`, `p3-06-attention`, `p3-07-review`, `p3-08-attach-anywhere`, `p3-09-resident-agents`, `p3-acc-smoke`, `p3-acc-record`, `p3-acc-candidate`, and the settlement branches below. Other branches exist only in temporary repositories.
- Settlement: the step 7 specification edits go in one pull request, branch `p3-plan-settle`, merged before the first child pull request. A later `Revised C-<n>` goes in its own pull request, branch `p3-plan-revise-c<n>`, merged before any child pull request whose text it changes merges.
- GitHub outside `infiquetra/talaria`: the operator pushes and opens the pull requests of `{{P3-07}}` test 41, UAT-3 and OP-4 in the throwaway repository with their own `gh`; agents push only to temporary repositories whose `origin` is a local bare repository.
- Decision 5 yes: the operator comments `Ruling decision 5: P6-01 joins phase 3`. That ruling authorises the lead to add `{{P6-01}}` as an eleventh child and edit the issue (membership, graph, ledger, the scenario ledger) and `{{PH3-ACC}}` before any child starts. Verification then uses `N=11`: AC-1 prints 11; AC-2 prints `completed=11` or `completed=10 not_planned=1`.

## 9. The record (AC-6)

- Shape B: the merged `p3-acc-record` pull request is required; its body and comments together are the record.
- Shape A: the `P=` and `REC=` lines are not run; the shape A line replaces them, and the record is `phase-3.md` followed by `phase-3-evidence.md`.
- Rows: `| <id> | <persona> | <result> | <evidence> | <commit> |`, ids and personas as in `{{PH3-ACC}}` specification section 4.
- Evidence: one heading line `## <id>`, the id exactly as in its row (`## P3-01 UAT-1`); the first `Candidate: <7 or more hex>` line after it, before the next `## `, ties it to the candidate.

## 10. The configuration document (C-3, AC-9)

The settled limit is the number in the latest `Settled C-3:` or `Revised C-3:` line (`limit <n> lines`), whatever form C-3 picks; AC-9 reads it, and has no default. `keymap_doc`'s test proves that every default key binding appears in the document; the grep in AC-9 spot-checks seven configuration names; each key-adding child's own criteria name the rest of its keys.

## 11. What Verification checks in the closeout

Checked by commands: the L-n lines (AC-10), the `- Journal:` lines (AC-12), the `## Closeout` heading (AC-13); the candidate, bench runs and rulings come from AC-6 to AC-8 and AC-11, not from the closeout text. Checked by the reviewer, who replies `Closeout reviewed` with any gap: the base, candidate and final commits; each child's pull request, merge commit and scenario results; the C-n list; the D-n list; the expected row count; `PINNED` and `code_sha`; the bench list; the hand-off.

## 12. The gaps C-1 to C-7 in full

The issue's Stop conditions summarise these; the planner settles each at step 7 and the settled text wins over the proposal.

- C-1, {{P3-03}}'s gate on {{P3-02}}: {{P3-03}} makes {{P3-02}} a start gate; {{P3-02}}'s Dependencies and spec 18 say {{P3-03}} may start after {{P3-01}} and merges after {{P3-02}}. Proposal: the start gate ({{P3-03}} tests 38, 50 drive {{P3-02}}'s overlay). Edit {{P3-02}}'s Dependencies and spec 18, or {{P3-03}}'s.
- C-2, live-region placement: {{P3-04}} spec 7.1 leaves the order "to be confirmed"; {{P3-05}} spec 13.1 cites {{P3-04}}'s REQ-31 for it (it is REQ-29). Proposal: decision 9. Edit {{P3-04}} spec 7.1 and {{P3-05}} spec 13.1.
- C-3, configuration length: only {{P3-06}} (AC-9, stop condition 4) keeps `docs/configuration.md` under 200 lines; six others add keys (about 20 from {{P3-07}}, 15 from {{P3-02}}). Decide the limit or a compact form; put it in every key-adding child's criteria.
- C-4, status-line ladder: {{P3-06}} renumbers `Segment` to ten levels (test 18 asserts exact lines); {{P3-07}} adds `PullRequest` above `External`. Proposal: {{P3-06}} merges first, or its test 18 asserts relative order.
- C-5, {{P3-09}} in the gate: it says decision 8 "yes" (elsewhere: reading (b)); {{PH3-ACC}} reruns every row, but its UAT-7 runs once. Proposal: under reading (b) its UAT-1 to UAT-6 become rows (83) by {{PH3-ACC}} spec 8.1 item 4, UAT-7 cited as evidence; this body counts both ways. Edit {{P3-09}}'s Dependencies and {{PH3-ACC}}'s REQ-11, counts and checker.
- C-6, R6's verbs: R6 wants sessions "steered" and "closed" from the fleet view, with cost; {{P3-02}} queues replies and leaves close to its question 2; {{P3-01}} reports no cost (decision 6). Settled by D-8 only.
- C-7, {{P3-08}} UAT-2: its preconditions have the tester launch and rename a `default` session, which Authority boundaries forbid. Proposal: the operator creates `attach-dup-<tag>` on `default` (tag from the tester) and later removes it; the tester does the `fixture` half and the read-only checks; unprepared: BLOCKED (no access). Edit {{P3-08}} UAT-2 and {{PH3-ACC}} REQ-11 (a fifth allowed change).
