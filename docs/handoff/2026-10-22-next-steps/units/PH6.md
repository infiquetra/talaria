# PH6: Talaria v0.7.0 phase 6: later work, in the order the operator chooses

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

How to use this document: the phase 6 capability issue is a run contract for eight independent children. This document holds the run procedure the issue points to: who may do what, the exact comment and evidence formats, the serial-execution and base-revision rules, the fleet seam stage inside `P6-02`, the end-to-end slice rules, precedence, and how each acceptance criterion is checked. It adds no requirement of its own. Unit ids below are written plainly; the issue carries the clickable references.

## 1. Roles and grants

| Role | Who | May do |
|---|---|---|
| Operator | The repository owner, whose GitHub login the run declaration records | Every decision in section 2 marked "operator"; Hermes configuration, gateway restarts, tags, releases, the tap, installs, merge grants |
| Lead | The coordinating agent named in each child's run declaration | Post the lead comments in section 2; re-parent `P6-01` to the phase 3 parent after the operator's decision 5 ruling; edit the inventory table's End-to-end slice column of the issue body; ask Mission Control for board moves |
| Planner | Named in the run declaration | Re-verify a scheduled child; edit that child's body and specification and this issue's ledgers; post the planner comments in section 2 |
| Test author | A different agent session from the developer, named separately in the run declaration | Write and run the listed tests first; post the red run |
| Developer | Named in the run declaration | Write code on the child's branch until the tests pass |
| Reviewer | Named in the run declaration, not the developer | Review; post `Seam stage reviewed:` for `P6-02` |
| Agent tester | Named in the run declaration | Run agent-runnable scenarios in Herdr and record evidence |

No role but the operator may post an operator comment. A comment whose author is not the role named here does not count.

## 2. Comment formats on the phase 6 issue

Each is one line, at the start of its own comment, exactly as written (angle brackets are filled in).

| Line | Author | When |
|---|---|---|
| `Members: P6-01=#<n> ... P6-08=#<n>` | lead | Before the first child; again after any membership change. After the decision 5 move: `Members: P6-02=#<n> P6-03=#<n> P6-04=#<n> P6-05=#<n> P6-06=#<n> P6-07=#<n> P6-08=#<n>`. The latest one counts. |
| `Phase 6 schedule: <key>` | operator | To schedule one child |
| `Phase 6 ruling: <key> not planned` | operator | To drop a child |
| `Operator answer <key>: <question>: <answer>` | operator | For each per-child question in section 10 |
| `Merge granted: <key>` (or `Merge granted: phase 6`) | operator | Lets the run merge that child's pull request after green checks and review |
| `Base revision <key>: <40 hex>` | lead | After the schedule comment and after the previous building child left flight (section 4); the value is `main` at the moment of posting |
| `Re-verification <key>: <edits, one per line> ` or `Re-verification <key>: no edits` | planner | After re-verification, before the child's first commit |
| `End-to-end slice: <key>` then the command, the success output and the scenario ids on the following lines; or `End-to-end slice: not applicable` then one full ruling-comment URL per building child | planner | Section 6 |
| `Release ordering: default` or `Release ordering: REL-03 amended` | planner | When `P6-07` is scheduled before the v0.7.0 release |
| `Seam stage reviewed: <40 hex>` | reviewer | On `P6-02`'s pull request, section 5 |
| `Terminal waiver: <terminal and multiplexer as named in the matrix>` | operator | On `P6-04`'s pull request, one comment per waived terminal |

## 3. Child pull request evidence block

Every completed child's pull request body ends with a section headed `## Acceptance evidence` holding, each on its own line:

- `Candidate: <40 hex>`: the head commit on which every result below was taken. If the head changes (a retry, a review fix), every result is taken again on the new head and this line is replaced; there is one candidate.
- `Red run: <link to the test author's comment>`.
- `AC-<n>: PASS` for every acceptance criterion, in order, followed by its evidence lines (command, exit status, the output lines the criterion names, or a link).
- `UAT-<n>: PASS` for every scenario, exactly that text alone on the line (the form `P6-05`'s own checks match with `^UAT-<n>: PASS$`), followed by its evidence (pane text from `herdr pane read`, timings, the operator's note for operator rows).

A row that is not PASS is written with its real label (`FAIL`, `BLOCKED (<reason>)`, `NOT RUN`) and the child is not complete. The only exception is a `P6-04` terminal row the operator waived (section 2): it stays `FAIL` in the terminal matrix, and the `UAT-6` line reads `UAT-6: PASS` only when every unwaived terminal passed, as `P6-04`'s AC-5 defines.

Post-merge criteria (`P6-07` AC-8 and AC-9) are recorded later as comments on the child issue in the same `AC-<n>: PASS` form.

## 4. Serial execution and base revisions

- A building child (`P6-01` to `P6-07`) is in flight from its `Phase 6 schedule:` comment until its pull request merges with every pre-merge criterion and scenario recorded PASS in its evidence block, or until its `Phase 6 ruling: ... not planned` comment. Post-merge criteria (`P6-07` AC-8, AC-9) do not hold the next child.
- At most one building child is in flight. The lead posts the next child's `Base revision` only after the previous one left flight, so the base always contains the previous child's merged change.
- `P6-08` is exempt: it changes only `docs/analysis/` and `tools/upstream-contributions/`, may overlap any building child, and takes its own `Base revision P6-08:` when scheduled.

## 5. The fleet seam stage inside P6-02

- Marker: every seam commit uses the conventional-commit scope `fleet-seam` (for example `feat(fleet-seam): add backend discriminator to FleetRow`). Adapter commits use any other scope.
- Gate, at the newest `fleet-seam` commit, before any adapter commit: these pass unchanged (the commands of the phase 3 fleet children's own Verification sections, as drafted; the planner re-reads them at scheduling):

```bash
cargo test -p talaria-core --test fleet_rows --test fleet_queue --test fleet_registry --test fleet_subscribed
cargo test -p talaria-app --test fleet_wire --test fleet_foreground --test fleet_app --test fleet_replay
cargo test -p talaria-cli --test fleet_cmd
cargo test -p talaria-core --test fleet_view_rows --test fleet_view_keys
cargo test -p talaria-app --test peek_wire --test fleet_view_flow --test peek_replay
cargo test -p talaria-tui --test fleet_view_screens
cargo test -p talaria-core --test fleet_attention --test alert_gate
cargo test -p talaria-tui --test status_fleet_segments --test status_line --test attention_keys --test fleet_alert_bytes
cargo test -p talaria-app --test attention_fleet --test attention_fleet_replay
```

  plus the seam tests the planner names for S1 (row tag and `fleet list` fields), S2 (external input and responder-keyed slot), S5 (launch affordance) and S6 (one sanitiser, custody check in both crates). Then the reviewer posts `Seam stage reviewed: <that commit>`.
- S3 (transcript cell type and peek source) and S4 (backend-routed answer and stop) are seam code but are verified only at merge, by the adapter's tests in the same pull request (S4 by test 21, `acp_wire.rs::mixed_backend_actions_send_no_hermes_requests`).
- A seam fix after adapter commits started is a new `fleet-seam` commit; the gate commands are rerun at it and the reviewer posts a new `Seam stage reviewed:` line. The latest one counts.

## 6. End-to-end slice

- Default: `P6-05`. It is locked as the slice the moment it completes; nothing replaces it after that.
- Replacement: only if the operator rules `P6-05` not planned. "Completed" means closed as completed; "next scheduled" means it has a `Phase 6 schedule:` comment and has not started. The planner takes the first child in this order that is completed or next scheduled: `P6-03`, `P6-02`, `P6-01`, `P6-06`, `P6-07`. If none qualifies yet, the planner waits and decides when the operator schedules the next building child. The criterion to copy is that child's required live run (as drafted: `P6-03` AC-5, `P6-02` AC-7, `P6-01` AC-6, `P6-06` AC-11, `P6-07` AC-4); the planner copies the current number, command and output from the child body at the time it posts the slice comment.
- Not applicable: when every building child has a not-planned ruling (including the case where only `P6-08` completed). The comment lists one full ruling-comment URL per building child. This is never PASS.
- Matching for branch (b) of the issue's AC-3: the recorded command equals the slice comment's command after collapsing runs of whitespace; each named success line appears exactly, and no `SKIPPED` line appears.
- `P6-05`'s check expects `LIVE_STREAM_RESULT: PASSED` four times because its live suite has four ignored tests (its tests 37 to 40), each printing one line; `--release` is how its AC-4 runs them. If re-verification changes that count or flag, the planner edits the issue's AC-3 in the same `Re-verification P6-05:` report.
- The lead edits the End-to-end slice column of the issue's inventory table to match the slice comment.

## 7. Precedence and ledger upkeep

- Facts at the pinned Hermes release and in merged code win over both this parent and a child. The planner edits the child body (and its specification), and if the parent is affected, edits the parent too, reporting both in the `Re-verification` comment.
- Between a child body and this parent, the parent wins; the planner edits the child.
- Ledger references (UAT and AC numbers, test paths) are as drafted. A planner who renumbers a child updates the issue's two ledgers, the replacement list in section 6 and AC-4's operator rows in the same edit.

## 8. Shared files

"Additive" means adding new items (an enum variant, a match arm for that variant, a function, a configuration key, a documentation section, a test). Changing or deleting an existing line of a shared file is allowed only where the child body names it as a minimal touch; anything else is the stop condition "a child needs another owner's file".

Shared files, as drafted: `crates/talaria-app/src/app.rs` and `crates/talaria-app/src/config.rs` (P6-02, P6-04, P6-06); `crates/talaria-tui/src/keys.rs` (P6-04, P6-06); `crates/hermes-client/src/lib.rs` (P6-01, P6-03, P6-06); `crates/talaria-app/src/lib.rs` (P6-02, P6-06); `crates/talaria-cli/src/args.rs` (P6-02, P6-05); `docs/configuration.md` (P6-02, P6-04, P6-06); `docs/engineering-journal/LEARNINGS.md` (all).

## 9. Test-first detail

- The test author is a different agent session from the developer; the run declaration names both.
- The red run is a pull request comment starting `Red run:` that gives the command and, per test, its name and the failure line. Each failure must match the reason the test names. For `P6-08`, the red run is `tools/upstream-contributions/check.sh` run against the missing finding, showing each check failing for that reason.
- Committed tests, fixtures and evidence hold no absolute path, home directory, login name or profile other than `default` or `fixture`. Temporary directories created at run time are fine and appear in evidence as `<tmp>`.

## 10. Operator questions per child

Answered as `Operator answer <key>:` comments before that child's planner starts:

- `P6-01`: decision 5 (phase 3 or 6); the second machine, the tunnel, `dashboard.public_url` and the sign-in provider are ready.
- `P6-02`: which non-Hermes agent comes first (installed by the operator); whether any agent gets file or terminal powers, and which; whether ACP subagents should get rows later.
- `P6-06`: the speech-to-text and text-to-speech providers in `fixture`; microphone permission and hardware ready.
- `P6-07`: Intel macOS (default yes); musl rather than glibc (default musl); tap updates by pull request (default yes); attestations (default yes); notarisation (default later); merge after the v0.7.0 tag (default yes); the tap repository and the name of its repository secret (never the value); consent for an agent to install a formula; the Mac with Homebrew used for its operator scenarios (the operator's by default).
- `P6-08`: which proposals, if any, are filed upstream.

## 11. How each acceptance criterion is checked

```bash
R=infiquetra/talaria
# AC-1: set equality, and each mapped title contains its key as an exact substring (for example "P6-04").
gh api repos/$R/issues/$ISSUE/sub_issues --jq '[.[].number] | sort'
gh issue view <n> -R "$R" --json title --jq .title
# AC-2 and AC-7: comments with author and time.
gh api --paginate repos/$R/issues/$ISSUE/comments --jq '.[] | [.user.login, .created_at, (.body | split("\n")[0])] | @tsv'
# AC-2: closure state per child.
gh issue view <n> -R "$R" --json state,stateReason
# AC-7: the child's first commit time, compared with the Base revision and Re-verification comment times.
gh pr view <child pr> -R "$R" --json commits --jq '.commits[0].committedDate'
# AC-3, AC-4: the evidence block of section 3.
gh pr view <child pr> -R "$R" --json body --jq .body
# AC-6: on the closeout commit named in the closeout comment, even if main advanced since.
git checkout <closeout sha>
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
```

AC-2 counts a `Phase 6 ruling` only when its author is the operator's recorded login. AC-7 passes for a child when both its `Base revision` and `Re-verification` comments were created before its first commit's date.

## 12. Facts checked while drafting

- The Hermes v0.22.4 contract has 80 `x-notifications` entries (`jq '."x-notifications" | length'` on `apps/shared/src/gateway-contract.openrpc.json` at v0.22.4). The roadmap's `P0-03` text says 79; `P6-08` recounts at the pin.
- `P6-08`'s pin check is `P0-03`'s: `python3 -I tools/contract-gen/contract_gen.py verify-vendor` exits 0 (it compares the vendored contract with its recorded sha256), and `crates/hermes-contract/PINNED` has the two lines `tag=<tag>` and `commit=<sha>`.
- `P6-08`'s baseline rows are the table under "Verified baseline at Hermes v0.22.4" in that issue's Context section.

## 13. Earlier-phase gates per child

Copied from each child's Dependencies and gates as drafted; the child body is the authority.

| Child | Earlier-phase gates (all content unless stated) |
|---|---|
| P6-01 | P1-01, P1-02, P1-03, P1-04, P1-12, P5-04; P0-01 for decision 5. If decision 5 moves it to phase 3, its start gate becomes PH2-ACC and its REQ-1 fallback (its own copy of the configuration-folder rule) applies. |
| P6-02 | P3-01, P3-02; P3-06 for attention parity; the seam is its own first stage (decision 6), not a gate. |
| P6-03 | P4-01 (REST client, gains `Method::Patch`), P5-01, P5-03, P5-04, P5-05, P5-06, P1-06, P1-08. Remote boards wait for P6-01; not a gate. |
| P6-04 | P1-08, P1-06, P2-04, P2-13, P1-12. |
| P6-05 | P2-14; P1-02 and P1-05 for the raw payload; P1-04 for the recorded fixture. |
| P6-06 | P1-02, P0-03, P1-04, P1-05, P1-07, P1-08, P1-09, P1-10, P1-12, P2-03; keeps the status segments of P2-16 and P3-06. |
| P6-07 | P0-02, P1-12, P4-10 (light); REL-02 either order; REL-01 and REL-03 merge gates as drawn, unless the release lead first amends REL-03 as `P6-07` describes. |
| P6-08 | P0-01 for decision 6; P0-03 merged with `crates/hermes-contract/PINNED` and the matching vendored contract present: `python3 -I tools/contract-gen/contract_gen.py verify-vendor` exits 0 (else BLOCKED). It gates no sibling. |

Links: [roadmap phase 6](../07-roadmap-and-work-units.md#phase-6--later-in-the-order-the-operator-chooses), [operator decisions](../08-risks-and-open-questions.md#decisions-the-operator-owns).
