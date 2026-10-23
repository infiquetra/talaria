# PH4B: Talaria v0.7.0 phase 4b: MCP servers, skills, plugins, memory, cron, connectors and /configure

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

This is the planning specification of the phase 4b capability issue [`PH4B`](https://github.com/infiquetra/talaria/issues/217). The issue says what the run delivers, the planning steps, the criteria and the stops; this document holds the detail a lead or planner needs to carry them out without guessing. It covers who creates the cards, the issues outside this slice and what each provides, the comment formats, the exact board commands, how graph edges are recorded, the planning steps in detail, the settlement edit lists, the operator's 30 items, and the closeout's evidence links. A pointer `spec <n>` in the issue means section `<n>` here. The lead commits this document to `docs/handoff/2026-10-22-next-steps/units/PH4B.md` with the seven child specifications (planning step 0).

## 1. Cards, membership and authority

The run never creates its cards. Before any run, the operator's issue-creation step for the whole v0.7.0 set does five things, in order, from the bodies committed beside these specifications:

1. It creates every issue of the set: this capability, its seven children, and every other issue that a placeholder reference (an id in double braces) in any body names.
2. It rewrites each body so that every placeholder reference becomes that issue's number.
3. It links each child as a native sub-issue of its parent (Mission Control's `flow link-sub-issue`).
4. It records each inventory dependency as a GitHub "blocked by" relationship where GitHub accepts it; the body states the gate either way.
5. It places each card on the Operations board with Stage `Intake`, Status `Backlog`, Objective `Talaria v0.7.0` and its priority.

Before the Base comment, the lead checks the result:

```bash
R=infiquetra/talaria; PARENT=<this issue's number>
gh api repos/$R/issues/$PARENT/sub_issues --paginate --jq '.[] | "\(.number) \(.title)"'
for n in $PARENT $(gh api repos/$R/issues/$PARENT/sub_issues --paginate --jq '.[].number'); do
  gh issue view $n -R $R --json body --jq .body | grep -qE '\{\{[A-Z0-9-]+\}\}' && echo "placeholder left: #$n"; done
```

The first command must list exactly seven cards. Six titles end `(unit P4-04)`, `(unit P4-05)`, `(unit P4-07)`, `(unit P4-08)`, `(unit P4-11)` and `(unit P4-09)`; the seventh starts `Phase 4 acceptance:`. The loop must print nothing. The lead then posts the `Members:` line, taking each number from that list.

If a card is missing, is not a sub-issue of this issue, or still holds a placeholder, the lead stops and reports here, and the operator reruns the creation step. The run never creates, deletes, links or unlinks a child card.

The only issues a run creates are [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262)'s defect issues (its REQ-16), filed through Mission Control, each naming its source child. A defect issue is never made a sub-issue of this issue, because AC-1 compares the sub-issue list with the seven members.

## 2. Issues outside this slice that the body names

Every placeholder reference resolves to a card of the same set (section 1). What this slice takes from each:

| Id | What it is | What this slice uses |
|---|---|---|
| [`PH4A`](https://github.com/infiquetra/talaria/issues/216) | phase 4a capability: settings, providers, model roles, profiles, doctor | its closeout, hand-off and `Settled C-1`, `C-2`, `C-8` comments (step 1) |
| [`PH4A-ACC`](https://github.com/infiquetra/talaria/issues/255) | phase 4a acceptance | its `PHASE 4A OPERATOR ROWS: PASSED` line (the lanes' start gate) and its record's shape (decision 11) |
| [`P4-00`](https://github.com/infiquetra/talaria/issues/249) | exploration under [`PH4A`](https://github.com/infiquetra/talaria/issues/216): which configuration calls work on the gateway socket and which need the dashboard API | its finding (below) |
| [`P4-01`](https://github.com/infiquetra/talaria/issues/250) | schema-driven settings | `RestClient::request`, `put_json`, `RestError::outcome_unknown()`, `apply_effect`, `MockRest` |
| [`P4-02`](https://github.com/infiquetra/talaria/issues/251) | providers, keys and sign-in | `SecretField`, `opener_argv` (C-3); the provider list in [`P4-09`](https://github.com/infiquetra/talaria/issues/261)'s brief |
| [`P4-03`](https://github.com/infiquetra/talaria/issues/252) | model roles | `ModelRolesService`, through which [`P4-09`](https://github.com/infiquetra/talaria/issues/261) applies the main model |
| [`P4-06`](https://github.com/infiquetra/talaria/issues/253) | profiles | the profile list in [`P4-09`](https://github.com/infiquetra/talaria/issues/261)'s brief and its hand-off to `/profiles` |
| [`P4-10`](https://github.com/infiquetra/talaria/issues/254) | doctor | the screen [`P4-11`](https://github.com/infiquetra/talaria/issues/260)'s `R` opens |
| [`P0-01`](https://github.com/infiquetra/talaria/issues/194) | the operator's decisions | draft ADRs accepted (precondition) |
| [`P0-03`](https://github.com/infiquetra/talaria/issues/196) | pinned contract and generated bindings | `crates/hermes-contract/PINNED`, `NativeMethod`, `overrides.rs` |
| [`P0-04`](https://github.com/infiquetra/talaria/issues/197) | patterns guide | `patterns-check` |
| [`P1-01`](https://github.com/infiquetra/talaria/issues/198) | find the gateway and attach | the attach example (step 8) |
| [`P1-11`](https://github.com/infiquetra/talaria/issues/208) | performance harness | `.github/workflows/bench.yml`, job `bench` (its REQ-20; AC-9) |
| [`P1-13`](https://github.com/infiquetra/talaria/issues/210) | live smoke test in Herdr | the smoke tool and its Herdr facts |
| [`P2-03`](https://github.com/infiquetra/talaria/issues/223) | slash-command palette | the `slash.exec` pass-through (section 8) |
| [`PH2A-ACC`](https://github.com/infiquetra/talaria/issues/229), [`PH2-ACC`](https://github.com/infiquetra/talaria/issues/238), [`PH3-ACC`](https://github.com/infiquetra/talaria/issues/248) | earlier acceptance | start gates of [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262); their smoke suites rerun as AUTO rows |
| [`PH5`](https://github.com/infiquetra/talaria/issues/218), [`P6-01`](https://github.com/infiquetra/talaria/issues/274) | later work | named only as out of scope |

**[`P4-00`](https://github.com/infiquetra/talaria/issues/249)'s finding.** During phase 4a, [`P4-00`](https://github.com/infiquetra/talaria/issues/249) merges a finding document at `docs/analysis/<YYYY-MM-DD>-hermes-configuration-surface.md` on `main` (its Deliverables item 1). It then comments on each gated phase 4a child with a link to that child's section. Its success criterion 3 also requires the finding to say what the phase 4b children under [`PH4B`](https://github.com/infiquetra/talaria/issues/217) should expect, without probing their calls. That part is "[`P4-00`](https://github.com/infiquetra/talaria/issues/249)'s phase 4b section" in planning step 2.

The finding names no fixed heading for that part. The planner reads the whole finding and takes every passage that names `PH4B`, phase 4b, or a call or route a phase 4b child uses. The step 2 comment quotes each passage's heading and names the child it affects. If no finding is on `main`, stop: phase 4a did not finish. If the finding says nothing about phase 4b, step 2 records `P4-00 phase 4b: none`, reports the gap on [`P4-00`](https://github.com/infiquetra/talaria/issues/249), and [`P4-05`](https://github.com/infiquetra/talaria/issues/257) and [`P4-08`](https://github.com/infiquetra/talaria/issues/259) meet their content gate with their own v0.22.4 facts.

## 3. Comment formats and who posts them

Verification reads only comments on this issue whose author is an OWNER or MEMBER of `infiquetra/talaria` (GitHub's `author_association`). Rulings and the operator's confirmation must come from the account `OP`.

| First line | Posted by | When |
|---|---|---|
| `Base: <40 hex>` | lead | after [`PH4A`](https://github.com/infiquetra/talaria/issues/216) closes with its operator rows passed |
| `Members: P4-04=#<n> P4-05=#<n> P4-07=#<n> P4-08=#<n> P4-11=#<n> P4-09=#<n> PH4-ACC=#<n>` | lead | right after Base (section 1) |
| `Planning step <k>: <result>` | planner | once per step 0 to 8 |
| `Settled C-<n>: <decision>` | planner | step 7, before the first building pull request |
| `Revised C-<n>: <decision>` | planner | later; the latest wins |
| `Ruling D-<n>: <decision>` | operator (`OP`) | before each deadline in Per-run inputs |
| `Ruling merge: <scope>`, `Ruling bench: granted` | operator (`OP`) | any time |
| `Freeze: <40 hex>` | lead | at [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262)'s freeze, equal to its record's `Candidate:` line, after `Ruling D-6:` |
| `## Closeout` | lead | at closeout (section 10) |

The lead and planner post with the account the operator names in the Per-run inputs' Account column. If that account is not an OWNER or MEMBER, the operator either adds it to the organisation or reposts each comment verbatim from their own account. An agent never posts as the operator without that instruction. If neither happens, AC-3 can never pass: stop and report.

AC-3 and AC-12 use the later of a comment's creation and last edit time. A change to a settled decision therefore goes in a new `Revised C-<n>:` comment, never an edit.

[`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262) REQ-6 freezes the candidate but does not post here. The lead posts the `Freeze:` line on this issue when [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262) freezes.

## 4. Board fields through Mission Control

The board is the organisation's Operations project (number 3). The run writes two single-select fields, Stage and Status. Objective and Priority are set by the creation step and never changed. The option names below were read on 2026-10-22 with `flow field-options --project operations --field Stage` and `--field Status`. The lead re-reads them at launch; a missing option is a stop.

- Stage: `Intake`, `Shaping`, `Planning`, `Active`, `Verify`, `Retro`.
- Status values this run uses: `Backlog`, `Execution planning`, `Ready for Active`, `Implementing`, `Code review`, `Ready to merge`, `Ready to close`, `Closeout`.

Writes go through Mission Control's command-line manager, `sdlc_manager.py`, run by the lead from the operator's Mission Control installation:

```bash
python3 sdlc_manager.py flow set-field --project operations --repo talaria --number <n> \
  --field Stage --option <Stage> --reason "<the evidence: a pull request or comment URL>"
python3 sdlc_manager.py board move --project operations --repo talaria --number <n> --status "<Status>"
```

`board move` writes Status on every board that carries the card. After every write the lead reads the card back; a mismatch stops the run:

```bash
gh api graphql -F n=<n> -f query='query($n:Int!){repository(owner:"infiquetra",name:"talaria"){issue(number:$n){projectItems(first:5){nodes{project{title} fieldValues(first:20){nodes{... on ProjectV2ItemFieldSingleSelectValue{name field{... on ProjectV2SingleSelectField{name}}}}}}}}}}' \
  --jq '.data.repository.issue.projectItems.nodes[] | select(.project.title=="Operations") | .fieldValues.nodes[] | select(.field.name=="Stage" or .field.name=="Status") | "\(.field.name)=\(.name)"'
```

| When | Cards | Stage | Status |
|---|---|---|---|
| created (section 1) | all eight | `Intake` | `Backlog` |
| Base comment posted | all eight | `Planning` | `Execution planning` |
| step 7 done and `Ruling D-1:` posted | [`P4-04`](https://github.com/infiquetra/talaria/issues/256), [`P4-05`](https://github.com/infiquetra/talaria/issues/257), [`P4-07`](https://github.com/infiquetra/talaria/issues/258), [`P4-08`](https://github.com/infiquetra/talaria/issues/259), [`P4-11`](https://github.com/infiquetra/talaria/issues/260) | `Planning` | `Ready for Active` |
| its start gates merged | [`P4-09`](https://github.com/infiquetra/talaria/issues/261), then [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262) | `Planning` | `Ready for Active` |
| the test author starts | that child | `Active` | `Implementing` |
| pull request open with red run and criteria | that child | `Active` | `Code review` |
| review approved and checks green | that child | `Active` | `Ready to merge` |
| merged | that child | `Verify` | `Ready to close` |
| [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262) merged | this card | `Verify` | `Closeout` |
| closeout posted | this card | `Verify` | `Ready to close` |

Each child moves alone, with the evidence named in `--reason`; no card moves at closeout in a batch.

## 5. Gates and how edges are recorded

| Gate | Before the other issue merges, the child may | It may not |
|---|---|---|
| start | nothing: no branch, no `test(...)` commit | start at all |
| content | branch, write its tests against stubs, build what does not call the other's names | call the other's names or merge |
| merge | build in full | merge |

A `-.-` line is coordination only: the pair read each other's settled text, and neither waits.

At step 7 the lead records each edge of the issue's graph on the child it gates, in two ways:

1. One line in the gated child's Dependencies section listing all its gates, `- Gate (phase 4b plan): <start|content|merge> after <issue>[, <issue>...]`. For [`P4-09`](https://github.com/infiquetra/talaria/issues/261): `- Gate (phase 4b plan): start after P4-04 (#256), P4-05 (#257), P4-08 (#259), P4-11 (#260), P4-02 (#251), P4-03 (#252), P4-06 (#253)`.
2. For each start or merge gate, a GitHub "blocked by" relationship from the gated child to the gating issue, unless the creation step already recorded it:

```bash
ID=$(gh api repos/infiquetra/talaria/issues/<gating number> --jq .id)
gh api -X POST repos/infiquetra/talaria/issues/<gated number>/dependencies/blocked_by -F issue_id=$ID
gh api repos/infiquetra/talaria/issues/<gated number>/dependencies/blocked_by --jq '.[].number'   # read back
```

Content gates from outside the slice are already met when [`PH4A`](https://github.com/infiquetra/talaria/issues/216) closes, so they get only the Dependencies line. If GitHub refuses a relationship, the Dependencies line stands and the step 7 comment says so.

Reasons for each edge:

| Edge | Gate | Reason |
|---|---|---|
| [`PH4A-ACC`](https://github.com/infiquetra/talaria/issues/255) to the five lanes | start | the lanes build on accepted phase 4a code |
| [`P4-01`](https://github.com/infiquetra/talaria/issues/250) to the five lanes | content | `RestClient`, `put_json`, `outcome_unknown`, `apply_effect`, `MockRest` |
| [`P4-02`](https://github.com/infiquetra/talaria/issues/251) to [`P4-04`](https://github.com/infiquetra/talaria/issues/256), [`P4-07`](https://github.com/infiquetra/talaria/issues/258), [`P4-11`](https://github.com/infiquetra/talaria/issues/260) | content | `SecretField`, `opener_argv` (C-3) |
| [`P4-00`](https://github.com/infiquetra/talaria/issues/249) to [`P4-05`](https://github.com/infiquetra/talaria/issues/257), [`P4-08`](https://github.com/infiquetra/talaria/issues/259) | content | the finding's phase 4b passages (section 2) |
| [`P4-10`](https://github.com/infiquetra/talaria/issues/254) to [`P4-11`](https://github.com/infiquetra/talaria/issues/260) | content | the doctor that `R` opens |
| [`P4-05`](https://github.com/infiquetra/talaria/issues/257) with [`P4-11`](https://github.com/infiquetra/talaria/issues/260) | coordination | the mock seam (C-2) |
| [`P4-04`](https://github.com/infiquetra/talaria/issues/256), [`P4-05`](https://github.com/infiquetra/talaria/issues/257), [`P4-08`](https://github.com/infiquetra/talaria/issues/259), [`P4-11`](https://github.com/infiquetra/talaria/issues/260) to [`P4-09`](https://github.com/infiquetra/talaria/issues/261) | start, content | [`P4-09`](https://github.com/infiquetra/talaria/issues/261) applies through `McpService`, `CapabilitiesService::apply` and `AddRequest`, and reads [`P4-11`](https://github.com/infiquetra/talaria/issues/260)'s lists |
| [`P4-03`](https://github.com/infiquetra/talaria/issues/252) to [`P4-09`](https://github.com/infiquetra/talaria/issues/261) | start, content | `ModelRolesService::execute` applies the main model |
| [`P4-02`](https://github.com/infiquetra/talaria/issues/251), [`P4-06`](https://github.com/infiquetra/talaria/issues/253) to [`P4-09`](https://github.com/infiquetra/talaria/issues/261) | start, content | the provider and profile lists in the brief, and the hand-offs to `/providers` and `/profiles` |
| [`P4-07`](https://github.com/infiquetra/talaria/issues/258) and [`P4-09`](https://github.com/infiquetra/talaria/issues/261) | none | [`P4-09`](https://github.com/infiquetra/talaria/issues/261) never applies through [`P4-07`](https://github.com/infiquetra/talaria/issues/258)'s service; C-5 settles `memory.provider` |
| the six building children to [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262) | start | the record matches real screen texts |
| [`P1-13`](https://github.com/infiquetra/talaria/issues/210), [`PH2A-ACC`](https://github.com/infiquetra/talaria/issues/229), [`PH2-ACC`](https://github.com/infiquetra/talaria/issues/238), [`PH3-ACC`](https://github.com/infiquetra/talaria/issues/248), [`PH4A-ACC`](https://github.com/infiquetra/talaria/issues/255) to [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262) | start | the earlier smoke suites it reruns and the record shape it copies |

The phase 4a edges into [`P4-09`](https://github.com/infiquetra/talaria/issues/261) are met before the run starts, because those issues merged in phase 4a.

## 6. Planning step details

- **Step 0.** Verification's `dead:` loop covers the seven child specifications and this one; it prints nothing.
- **Step 2.** Where [`P4-00`](https://github.com/infiquetra/talaria/issues/249)'s phase 4b section is, and what to do without one: section 2.
- **Step 3.** The consumed names most at risk: [`P4-01`](https://github.com/infiquetra/talaria/issues/250)'s `RestClient::request`, `put_json`, `RestError::outcome_unknown()`, `apply_effect`; [`P4-02`](https://github.com/infiquetra/talaria/issues/251)'s `SecretField`, `opener_argv`; [`P4-03`](https://github.com/infiquetra/talaria/issues/252)'s `ModelRolesService`; [`P0-03`](https://github.com/infiquetra/talaria/issues/196)'s `overrides.rs` replacing a params type ([`P4-04`](https://github.com/infiquetra/talaria/issues/256)) and a result field type ([`P4-08`](https://github.com/infiquetra/talaria/issues/259)). A missing name is an edit to the consumer or a report on the owner.
- **Step 5, marked facts.** At a release other than v0.22.4, re-read each child's marked facts (its specification's section 9 or 12), first `reload.mcp`'s `always` ([`P4-04`](https://github.com/infiquetra/talaria/issues/256)), H6, H21 and H33 ([`P4-05`](https://github.com/infiquetra/talaria/issues/257)), `last_fire_error` ([`P4-08`](https://github.com/infiquetra/talaria/issues/259)) and `info.tools` ([`P4-09`](https://github.com/infiquetra/talaria/issues/261)).
- **Step 5, the profile selector (C-1).** The dashboard routes are FastAPI functions in `hermes_cli/web_routers/` at Hermes v0.22.4. They read a profile in one of three ways. A function parameter `profile: Optional[str] = None` that is not part of the path is a query parameter: `GET /api/skills` (`hermes_cli/web_routers/skills.py` line 344). Some write routes read a JSON body field first and the query parameter second, `body.profile or profile`: `PUT /api/skills/toggle` (lines 379 to 384) and the hub install and uninstall routes (lines 98 to 110). A route with neither ignores any profile and acts on the gateway's launch profile. For every route any child calls, the planner records one line in the step 5 comment, `<METHOD> <route>: <reading> (<file>:<line>)`, where `<reading>` is one of `query`, `body, else query`, `body` or `none`. Settled C-1 then states, per route, how the child sends the connected profile: the query parameter for `query`, the body field for `body` and `body, else query`, and for `none` what the child does instead. Re-read at `PINNED` before relying on these lines.
- **Step 6, the documentation limit.** No estimate of unwritten lines is needed. The planner records in the step 6 comment `wc -l < docs/configuration.md` on `main`, `LIMIT` ([`PH4A`](https://github.com/infiquetra/talaria/issues/216)'s Settled C-8) and the headroom. Each child's documentation criterion named in C-6 checks the file is below `LIMIT` on its own pull request head. A child that would reach `LIMIT` stops and reports here (C-6). The planner also confirms that `.github/workflows/bench.yml` exists on `main` with a job named `bench` ([`P1-11`](https://github.com/infiquetra/talaria/issues/208) REQ-20); if it does not, AC-9 cannot run: stop.
- **Step 8, the attach check.** [`P1-01`](https://github.com/infiquetra/talaria/issues/198) REQ-18 defines the example `examples/attach.rs` with `--home`, `--profile`, `--no-ensure` and `--hermes`, printing `gateway code_version=<v> code_sha=<first 12 characters>`. From the repository root:

```bash
OUT=$(cargo run -q -p hermes-client --example attach -- --no-ensure --profile fixture); echo "exit=$?"
S=$(echo "$OUT" | sed -nE 's/^gateway code_version=.* code_sha=([0-9a-f]{12})$/\1/p')
P=$(sed -nE 's/^commit=([0-9a-f]{40})$/\1/p' crates/hermes-contract/PINNED | cut -c1-12)
[ -n "$S" ] && [ "$S" = "$P" ] && echo "code_sha=$S matches PINNED" || echo "STOP: code_sha '$S', PINNED '$P'"
```

The example missing, a flag refused, a non-zero exit, no `code_sha` line, or a mismatch is a stop, reported on [`P1-01`](https://github.com/infiquetra/talaria/issues/198) when the example or a flag is missing. The example is never run without `--no-ensure`. The step comment holds the last line and `exit=`.

## 7. Settlement edit lists for C-1 to C-7

The issue's Stop conditions state each gap and its proposal. Settling a gap means editing these bodies (and their specifications to match), then posting `Settled C-<n>:`.

| Gap | Bodies to edit |
|---|---|
| C-1 | [`P4-04`](https://github.com/infiquetra/talaria/issues/256) (REQ-5, decision 7, test 30), [`P4-07`](https://github.com/infiquetra/talaria/issues/258) (decision 3), [`P4-08`](https://github.com/infiquetra/talaria/issues/259) (decision 2), [`P4-09`](https://github.com/infiquetra/talaria/issues/261) (decision 6); any child whose route the step 5 table contradicts |
| C-2 | [`P4-07`](https://github.com/infiquetra/talaria/issues/258) (Interfaces, test 28), [`P4-08`](https://github.com/infiquetra/talaria/issues/259) (Interfaces, tests 28 and 29) |
| C-3 | [`P4-11`](https://github.com/infiquetra/talaria/issues/260) (Dependencies, Work surface) |
| C-4 | [`P4-07`](https://github.com/infiquetra/talaria/issues/258) (UAT-6, AC-14) |
| C-5 | [`P4-09`](https://github.com/infiquetra/talaria/issues/261) (REQ-9, test 4) |
| C-6 | the limit into [`P4-04`](https://github.com/infiquetra/talaria/issues/256) AC-13, [`P4-05`](https://github.com/infiquetra/talaria/issues/257) AC-11, [`P4-07`](https://github.com/infiquetra/talaria/issues/258) AC-15, [`P4-08`](https://github.com/infiquetra/talaria/issues/259) AC-13, [`P4-11`](https://github.com/infiquetra/talaria/issues/260) AC-15, [`P4-09`](https://github.com/infiquetra/talaria/issues/261) AC-15 |
| C-7 | only the body that `Ruling D-7:` assigns |

C-7 in full. Three needs have no owner: hooks, sandboxed terminal backends and instruction files (parity rows "MCP servers, plugins, hooks, sandboxed terminal backends" and "Instruction files and memory"). Two roadmap promises are not kept: device-code MCP sign-in ([`P4-04`](https://github.com/infiquetra/talaria/issues/256) Q3 only shows the command) and `tools.configure` ([`P4-05`](https://github.com/infiquetra/talaria/issues/257) forbids it). Four features the children leave out by their own text: per-tool MCP server tool selection ([`P4-04`](https://github.com/infiquetra/talaria/issues/256) shows it read-only; D-2 Q2), skill uninstall ([`P4-05`](https://github.com/infiquetra/talaria/issues/257) operator question 2), editing a cron job ([`P4-08`](https://github.com/infiquetra/talaria/issues/259): remove and re-create meanwhile), and memory-provider sign-in ([`P4-07`](https://github.com/infiquetra/talaria/issues/258): no bundled provider offers it at v0.22.4). All of them are the issue's "Not delivered here" list, ruled under D-7.

## 8. Ledger sources and pass-through

Sources of the ledger rows: L-1 to L-30 come from R7 and the parity rows "MCP servers, plugins, hooks, sandboxed terminal backends", "Instruction files and memory" and "Scheduled routines"; L-31 from R7's acceptance test; L-32 to L-34 from R2; L-35 to L-41 from the budgets (frame 50 ms, keystroke 16 ms at the 95th percentile, zero idle frames, memory 40 MB then 100 MB); L-42 to L-51 from the protocol rules; L-52 to L-63 from security.

Pass-through. Decision 2 keeps Hermes's own `/skills`, `/plugins`, `/memory`, `/curator` and `/platforms`. The pass-through itself is [`P2-03`](https://github.com/infiquetra/talaria/issues/223)'s `slash.exec`, delivered in phase 2; this slice adds nothing to it. [`P4-07`](https://github.com/infiquetra/talaria/issues/258) proves `/memory` and `/curator` still reach Hermes (L-15). For `/skills`, `/plugins` and `/platforms`, the slice's only duty is not to bind those names locally ([`P4-05`](https://github.com/infiquetra/talaria/issues/257) decision 1, [`P4-11`](https://github.com/infiquetra/talaria/issues/260) decision 1). AC-6's local-command grep checks that no added line of `crates/talaria-app/src/commands/local.rs` names one of them.

## 9. What a reload does (decision 5)

At Hermes v0.22.4, `reload.mcp` reconciles the profile's MCP servers and refreshes its cached agents (`gateway/session_ancillary.py`, lines 254 to 295). The calling chat adopts the new tools at once (`applied`), or `busy` until its turn ends. Other chats of the profile adopt them only through their own apply or a later attach. The `confirm_required` message, "Reloading MCP refreshes every chat of this profile.", is Hermes's own wording. Re-read at `PINNED`.

## 10. The operator's 30 items and the closeout

The "Operator runs (child pull request)" column of the issue's scenario ledger says who runs a scenario in its own child's pull request. In [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262)'s sitting the rule is stricter (its specification section 7.2): the operator runs every source row that writes `fixture`, saves a key, signs in or uses an account. The 30 items, in [`PH4-ACC`](https://github.com/infiquetra/talaria/issues/262) specification section 7.5's order:

| Items | Rows |
|---|---|
| 1 to 5 | OPC-1 to OPC-5 |
| 6 to 10 | [`P4-04`](https://github.com/infiquetra/talaria/issues/256) UAT-2, UAT-3, UAT-5, UAT-4, UAT-6 |
| 11 to 13 | [`P4-05`](https://github.com/infiquetra/talaria/issues/257) UAT-2, UAT-3, UAT-4 |
| 14, 15 | [`P4-07`](https://github.com/infiquetra/talaria/issues/258) UAT-4, UAT-5 |
| 16 to 20 | [`P4-08`](https://github.com/infiquetra/talaria/issues/259) UAT-2 to UAT-6 |
| 21 to 24 | [`P4-11`](https://github.com/infiquetra/talaria/issues/260) UAT-3 to UAT-6 |
| 25, 26 | [`P4-09`](https://github.com/infiquetra/talaria/issues/261) UAT-1, UAT-2 |
| 27 to 29 | XF-9, XF-10, XF-11 |
| 30 | OP-1 (the live write suites) |

Items 6 to 26 are the 21 operator source rows; an agent tester runs the other 15.

Closeout rows. Each of L-1 to L-63 gets one row, `| L-n | <owner key> <test or scenario> <evidence URL> | <main commit> | PASS |`. The test or scenario is one from the row's Proof cell. The evidence URL is one of these kinds, all under `https://github.com/infiquetra/talaria/` (AC-11 accepts no other):

- `pull/<n>`: the owner's pull request, whose Red run, Criteria and scenario sections hold the result;
- `pull/<n>#issuecomment-<id>` or `issues/<n>#issuecomment-<id>`: a comment, such as the operator's `UAT-<n>: PASS` or an OPC item;
- `actions/runs/<id>`: a CI or `bench` run;
- `blob/<40 hex>/<path>`: a committed record file at a commit, such as `docs/acceptance/v0.7.0/phase-4-evidence.md`.

Journal lines: `- Journal: <key> <merge commit>: <heading>` when the child's merge added a journal entry with that heading, or `- Journal: <key> none: <reason>`.

## 11. What AC-6's greps check

- The `unattended` grep finds a flag in code. Lines whose code starts with `//` or `#` are skipped, so a comment explaining the ban does not trip it. [`P4-05`](https://github.com/infiquetra/talaria/issues/257)'s own whole-word rule (its REQ-13) is stricter and still applies to its files.
- The planning-code grep covers every added `.rs` line, comments included (decision 1: no planning codes in `.rs` files).
- The `unsafe` and `hermes` grep covers added `.rs` lines that are not comments. It finds `unsafe` blocks, functions, implementations, traits and `extern` blocks, and `Command::new("hermes")` (decision 9: Talaria code never runs a `hermes` command). Code from earlier phases, such as [`P1-01`](https://github.com/infiquetra/talaria/issues/198)'s ensure step, is before `BASE` and not checked.
- The local-command grep: section 8.
- The ticket, connect-link and identifier greps are unchanged; synthetic sentinels contain `sentinel` and `https://example.invalid/` links are allowed (the issue's Privacy list).
