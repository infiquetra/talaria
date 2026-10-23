# PH3-ACC: Phase 3 acceptance: three agents on three providers while the operator codes in a fourth

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what is accepted, in what order, and how the result is recorded. This document holds what the people doing the work would otherwise have to guess: how to freeze and build the candidate, the exact record and evidence templates, how to scrub evidence, the complete checker script and its tests (copied from phase 2b's with the row list changed, and run while drafting on macOS), the exact steps and pass conditions of the 12 cross-feature scenarios and of the operator's fleet day (the 6 parity lines and the 8 operator rows), the run order and the rules for failures, the full design of the fourth smoke suite (`phase3`: files, signatures, every step with its exact failure text, the screens to record, the tests), and the questions the operator still owns. Appendix A holds the verbatim text of the 45 scenarios of the eight accepted issues, extracted from their final bodies by script; the source issue's own text wins if it was edited after this document was written.

Sections are numbered so the issue can point at them. Handoff documents are linked relatively: [the roadmap's phase 3](../07-roadmap-and-work-units.md#phase-3--the-fleet-many-agents-in-the-background), [the P1-13 roadmap entry](../07-roadmap-and-work-units.md#p1-13--live-smoke-test-in-herdr), [the Claude Code parity checklist](../04-prior-art.md#claude-code-parity-checklist), [performance budgets](../05-architecture.md#performance-budgets), [why the Python client's acceptance ceremony failed](../02-current-state.md#why-it-ended-up-here). The patterns copied are [PH2-ACC](PH2-ACC.md) (the record, the checker, the cross-feature rows, the week and the last smoke suite), [PH1-ACC](PH1-ACC.md) (the original record and checker) and [PH2A-ACC](PH2A-ACC.md) (the second smoke suite); the smoke program extended is [P1-13](P1-13.md).

## 1. What this unit is for, in plain terms

Phase 3 of Talaria turns one conversation into a fleet. Requirement R6 asks that the operator can "send prompts to several sessions, leave them working, and keep coding in another", with a fleet view that shows each session's state, model and cost, and one "needs you" queue; requirement R5 asks that closing Talaria never stops a turn and that reopening it shows what happened meanwhile ([requirements](../01-vision-and-requirements.md)). Eight children built it: the fleet registry (P3-01), the fleet view with peek, reply, stop and open (P3-02), background launch including a new session in a git worktree (P3-03), the subagent tree (P3-04), background processes and `/bg` (P3-05), attention (P3-06), review of a worktree session through a draft pull request (P3-07) and `talaria attach` with "open in a new pane" (P3-08). Each passed its own scenarios on its own commit. A passing child proves its parts. It does not prove the seams: a background approval that must appear in the list, in the status line, in the fleet view and behind `alt+a` as one item; an approval answered while a queued prompt waits in the session being typed in; a result of `/bg` that arrives while an overlay is open; a worktree session started from the fleet view and then reviewed and pushed from it; a session that outlives the Talaria that started it and is reopened by title. Those seams, and the phase 1 and phase 2 features still working on the same commit, are tested only here.

The unit does four things. It runs the automated suites and the four smoke suites (phase 1, phase 2a, phase 2b and a new phase 3 suite that this unit adds) on one candidate commit. It reruns every user acceptance scenario of the eight issues (45). It adds 12 scenarios that only exist when features combine. And it turns the roadmap's phase 3 operator check into something a person can mark: "three sessions on three different providers working on three tasks while the operator codes in a fourth; an approval answered from the peek panel; a worktree session ending in a draft pull request" becomes a day log, 6 parity lines with a verdict each and 8 operator rows with pass conditions that can be counted.

The Python client spent 44 percent of the lines it added on acceptance records ([02-current-state.md](../02-current-state.md#why-it-ended-up-here), item 4), so the ceremony stays as small as the earlier gates': two markdown files and one checker of about 55 lines. The shape is the one `PH1-ACC`, `PH2A-ACC` and `PH2-ACC` use; whether it stays is the operator's ruling (section 10.1). Draft ADR-0012 decision 4 reads in full: "No per-release receipt schemas, digest-bound evidence trees or release-record tests." This record has no schema, digest or signature. The only other code is the smoke extension, which is a tool, not a record.

The fleet day is the part no agent can do. An agent tester proves that each feature works the way its issue says, using one free model on one provider. Only the operator can run three real providers at once, answer a real approval from the peek panel while thinking about something else, push a real branch and open a real draft pull request, and say whether the whole feels like a fleet. So this unit defines that day as concrete, countable things and keeps the verdict the operator's: an agent copies the operator's words into the evidence and never decides an operator row.

## 2. Facts

### 2.1 The `herdr` command lines

The facts H1 to H14 of [P1-13](P1-13.md#21-the-herdr-command-line-verified-by-running-it-herdr-094-macos-2026-10-22) apply unchanged (Herdr 0.9.4, checked by running it on macOS on 2026-10-22; re-check with the Herdr in use and write `herdr --version` in the record header). Three bear on this unit: `pane run` types text and Enter together; `pane read` has sources `visible`, `recent` and `recent-unwrapped`; a character sent right after `esc` may be lost, so wait 400 ms after `send-keys esc`. Whether `visible` or `recent-unwrapped` is stale for a full-screen program is P1-13's H14 **verify** item, settled by its first live run and recorded in its journal entry: read that entry before recording screens, and follow it. The suite reads the source section 9.3 names for each step; the manual rows (XF) run `echo READ-CHECK` in the pane first and use the source that shows it. If P1-13's journal entry records that a source lags for Talaria, stop in stage 0 and report it on P1-13 before any screen is recorded. A suite step never stops the run for a lagging read: it FAILS with its reason and the pane text (a lagging source is then a non-defect that section 8.1 item 2 lets the lead rerun once). Never swap a source silently to make a step pass.

New facts for this unit, and how each was checked:

| # | Fact | How checked |
|---|---|---|
| S1 | The key names `alt+a` (jump to the oldest background request, P3-06) and `alt+s` (the agents panel, P3-04) are sent with `herdr pane send-keys <pane> alt+a` and `alt+s` | `P3-06` UAT-2 and `P3-04` UAT-2 use them; not verified here. The first live contact is the stage 0 check of section 9.0: the test author sends each key of S1 and S2 once to a Talaria in the throwaway tab and records whether Herdr accepted it, before any screen is recorded. A rejected name stops stage 0 (the operator chooses before the freeze, section 10.2 question 9); no other name is invented. If a rejection still happens in a live row, that row is `BLOCKED (environment: Herdr rejected <key>)` (section 8.1 item 1); the operator's own keyboard is not affected (OP-3 and OP-6) |
| S2 | `shift+p` and `ctrl+s` (review, P3-07) and `shift+x` (discard) are sent the same way | `P3-07` UAT-1, UAT-3 and UAT-5; same rule as S1 |
| S3 | `herdr pane split <pane> --direction right --no-focus --cwd <folder>` prints JSON with `.result.pane.pane_id`; the same family of command is what Talaria itself runs for "open in a new pane" (P3-08, which types the attach command afterwards because `pane split` takes no command) | P1-13's facts H8 and H13 and P3-08's REQ-10 to REQ-17; if the form differs at the Herdr in use, report it on P3-08 |
| S4 | `herdr pane process-info --pane <pane>` prints JSON with `.result.process_info.foreground_processes[]`, each with `cmdline` and `pid` | P1-13's fact H6, used the same way by the idle-cost rows of the earlier gates |
| S5 | `talaria fleet list --json` prints one JSON document with a `rows` array whose items have `session_id` and `state` and a `needs_you` array whose items have `session_id` (the `jq` filters in `P3-01` UAT-3 and `P3-03` UAT-2); the exact schema is `docs/formats/fleet-list-v1.md` (`talaria.fleet.v1`) | read it at the candidate; a filter that fails against it is a defect of this document, corrected here |

### 2.2 Hermes facts used (read at Hermes v0.22.4; re-check at the release in `crates/hermes-contract/PINNED` before relying on them)

- An unanswered approval is denied after `approvals.timeout`, default 300 s; `approvals.mode` is `manual`, `smart` or `off`, default `smart` (`hermes_cli/config_defaults.py` lines 1694 and 1695 at Hermes v0.22.4, read again while drafting). Every scenario that waits on an approval card therefore answers within 240 s of the card.
- `prompt.submit` accepts the auto-approve flag only together with `finite: true`, and over the wire only from the local CLI owner of its own CLI-surface session; such a turn is then auto-approved (`gateway/session_finite.py`: `admit_finite`, lines 44 to 56, and `_may_run_unattended`, lines 59 to 68; `gateway/session_controls.py` line 763 passes the submitter; `tools/approval.py` lines 327 to 333, `_yolo_active`, treats such a turn as a bypass; all at Hermes v0.22.4). None of this changes the rule: nothing in this unit sends it.
- The contract has `session.active_list` (the fleet's liveness poll, `P3-01` decision 3), `delegation.pause` and `delegation.status` (`apps/shared/src/gateway-contract.openrpc.json` at Hermes v0.22.4). Pausing is gateway-wide at v0.22.4 (stated by `P3-04`); that is why `P3-04` UAT-6 is the operator's and runs when no agent runs a live scenario.
- Server requests that go through `gateway/session_server_requests.py` (`sudo`, `secret`, `clarify`) are written to every viewer attached to the session (`_ask`, `events.fanout.write`); the first answer settles the request (`resolve`), the other viewers get `request.cancel` with reason `resolved`, and an error answer counts as no answer (`ask_viewer`, `ask_viewer_result`); read at Hermes v0.22.4. Approvals are different: the gateway emits the event `approval.request` (`apps/shared/src/gateway-events.ts` line 37) and takes the answer by the method `approval.respond`; the contract also lists an `approval` server request, but no caller of `ask_viewer` sends it at v0.22.4. How two viewers' approval answers interact is `P3-02` decision 8 (verify at the pinned commit). XF-8 and XF-10 rely on two viewers being normal (`P3-08` decision 7).
- Attaching to a session returns its whole history (`P3-01` decision 2, verified by that issue at v0.22.4); a read of a session that is not live restores it. So the fleet registry only lists live sessions, and a session that finished long ago does not appear in `talaria fleet list` until it is attached.
- The live listing leaves out a session only when it is "closed, idle and unviewed": no subscriber, idle, and either superseded or with a local receipt whose target row has `ended_at` (`closed_idle_unviewed` in `gateway/session_local_close.py`, called from `live_entries` in `gateway/session_live_listing.py`, at Hermes v0.22.4). `ended_at` comes from a `session.detach` with an `end_reason`. `P3-03`'s `--bg` launch detaches without one (its REQ-9; `launch_abandoned` only when nothing was admitted) and Talaria's `/quit` sends none (`P1-12`), so a finished background session stays in `talaria fleet list`, in the fleet overlay and open to `talaria attach` while the gateway holds it. Whether the gateway drops idle sessions from memory after some time was not found in the source; the stage 0 check of section 9.0 confirms the listing on the live gateway (ruling: section 8.1 item 7).

### 2.3 What the source issues provide

Screen texts are each owned by one source issue and are listed with their sources in section 9.5. The demos and ignored live tests are named in section 7.1. Behaviours the scenarios rely on that are not Hermes facts: Ctrl+C clears a non-empty draft (P1-08 settled decision 1) and Esc interrupts a running turn (P1-08); `/quit` detaches and the session keeps running in the gateway (P1-12 settled decision 1; the hint `talaria --resume local-<hex> --profile fixture` is printed when the session has a prompt, P1-13 fact T2); the fleet overlay and the agents panel are overlays that leave nothing behind in scrollback (P1-06, requirement R2).

## 3. Freezing and building the candidate

The smoke extension (section 9) is built and merged first, on branch `p3-acc-smoke`, by a test author and a developer; it is part of the candidate. The lead freezes after the eight children, `PH2-ACC`, P1-13 and the extension are merged and closed. Never switch the primary checkout (it may hold untracked material); work in a worktree outside the repository.

```bash
git fetch origin
CANDIDATE=$(git rev-parse origin/main)                    # 40 hex characters; written in the record header
WT=$(mktemp -d)/p3-acc                                     # a new folder outside the repository
git worktree add "$WT" "$CANDIDATE" && cd "$WT"
git push origin "$CANDIDATE:refs/heads/p3-acc-candidate"   # for the bench runs of AUTO-1; deleted at closeout
cargo build --release --workspace --examples
cargo build --release -p talaria-cli -p talaria-bench
shasum -a 256 target/release/talaria                       # the binary every agent row uses, and the one the operator's day uses
git switch -c p3-acc-record                                # the record branch starts at the candidate commit
```

The record branch holds documentation commits only (AC-6 proves it). After a new candidate (a fix) the branch is rebased onto it. `main` may move during the run: the record is compared with the candidate, never with `main`. The operator's fleet day uses the same binary (REQ-13): the lead gives the operator the path of a copy of `target/release/talaria` and its checksum outside the issue; the checksum goes in the AUTO-1 evidence and in the day log (OP-8).

Shell variables do not cross Herdr panes. At freeze the lead writes the common settings to a file next to the worktree, and every pane starts in the worktree root (`--cwd`) and runs `. ../acc.env` first. The file's location is never written in the issue, the record or the evidence.

```bash
cat > ../acc.env <<EOF2
CANDIDATE=$CANDIDATE
T=$PWD/target/release/talaria
F='--no-ensure --profile fixture --provider ollama-cloud --model minimax-m3.1-flash'
L='--bg --no-ensure --output-format json --profile fixture --provider ollama-cloud'
PATH=$PWD/target/release:\$PATH
EOF2
. ../acc.env; command -v talaria                            # must print this worktree's target/release/talaria
```

Helpers every XF row uses (section 7.3):

```bash
pane_new() { herdr pane split "$HERDR_PANE_ID" --direction right --no-focus --cwd "$1" | jq -r '.result.pane.pane_id'; }
submit()   { herdr pane send-text "$1" "$2"; herdr pane send-keys "$1" enter; }   # two calls: the text, then Enter
vis()      { herdr pane read "$1" --source visible; }
rec()      { herdr pane read "$1" --source recent-unwrapped --lines 3000; }
fleet()    { "$T" fleet list --profile fixture "$@"; }                           # the fleet list of the profile the rows use
```

`--no-ensure` is on every launch that starts a live session an agent makes, so an agent can never start a gateway (the operator's own day may omit it, REQ-13). `talaria fleet list` never starts one. Preflight before stage 1 (once, with the Herdr in use): `herdr --version`, `tmux -V` (the phase 2a suite of AUTO-9 needs it; `P3-08` UAT-4 needs it); run both pane-split forms (the source scenarios' `herdr pane split --current --direction right --cwd . --no-focus` and the helper's) and close the panes they make. A form Herdr rejects is reported on the issue and allowed by REQ-11 change (4). Also run `herdr pane wait-output --help`: the XF rows pass `--source` and a `(?m)` regular expression, which P1-13's fact H5 does not list; when Herdr rejects either, the row polls `pane read` once a second for up to its timeout instead and says so in the evidence. If a code path changes after this point the candidate is dead: start again from the top of this section.

**The quiet-fleet rule.** All agent rows share one gateway, one profile and one free model, and the fleet registry lists every live session of the profile. A leftover session that waits for an answer or still works changes what the next row sees (`NEEDS YOU` and `WORKING` groups, the status-line counter, the jump key). So before every live row, and after every row, the tester runs `fleet` and reads it: no `NEEDS YOU` and no `WORKING` group. If one is there, the tester waits up to 5 minutes for it to clear; a session that still needs an answer is answered with `deny` (resume it with `talaria --resume <id> --profile fixture --no-ensure` and press Esc on the card, as `P3-01` UAT-3 does) and the row's evidence says so; a stuck turn is stopped from the fleet view. Rows that need leftovers on purpose (the source scenarios that chain, section 7.2) are the exception, run back to back in their own order. The quiet-fleet check is a wait, not a scenario: it also runs before each of AUTO-8 to AUTO-12, which are "back to back" only in the sense that no other live scenario runs between them. The suite's `fleet-quiet` step checks once and never waits (9.3.1); the tester's check is the wait.

**Pane width.** The fleet view drops columns below 100, 80 and 60 and shows only `Window too small for the fleet view. Esc closes.` below 40 columns or 10 rows (`P3-02` REQ-2); `P3-04` needs 100 columns. Every pane a row starts Talaria in is at least 100 columns wide and 24 rows high: the tester runs `tput cols; tput lines` in it first and, when a split made it narrower, widens the Herdr window or uses a pane of its own tab before the row (a row run in a narrower pane is the tester's mistake, section 8.1 item 2). The smoke suite checks its own tab (9.3.1). Also before moving on from `P3-05`: `pgrep -fx 'sh tick.sh' | wc -l` prints 0 (kill leftovers with `pkill -fx 'sh tick.sh'`; they are the tester's own processes).


## 4. The record

File `docs/acceptance/v0.7.0/phase-3.md`. The first commit holds this skeleton with every header line filled in with its real value (the checker refuses a header that still holds an angle-bracket placeholder; the values below are examples) and `phase-3-evidence.md` holding only its title line, so the first run of the real checker is the red run: 77 problems. Commit order: the checker tests with the stub (`test(acceptance): ...`), the empty record (`docs(acceptance): add the empty phase 3 record`), then the real checker (`docs(acceptance): add the phase 3 checker`); at the empty-record commit the stub still prints `FAILED (stub)`, so the 77-problem run is taken at the checker commit, while the record is still all NOT RUN. Row order is the order below; do not add other tables. The folder also holds the earlier gates' records (`phase-1*`, `check-record*`, `phase-2a*`, `check-phase-2a*`, `phase-2*`, `check-phase-2*`); this unit never edits them.

```markdown
# Phase 3 acceptance record

Candidate: 0123456789abcdef0123456789abcdef01234567
Hermes: v0.22.4
Herdr: herdr 0.9.4
OS: macos
Date: 2026-11-20

| id | persona | result | evidence | commit |
|---|---|---|---|---|
| AUTO-1 | builder | NOT RUN |  |  |
| AUTO-2 | builder | NOT RUN |  |  |
| AUTO-3 | builder | NOT RUN |  |  |
| AUTO-4 | builder | NOT RUN |  |  |
| AUTO-5 | builder | NOT RUN |  |  |
| AUTO-6 | builder | NOT RUN |  |  |
| AUTO-7 | builder | NOT RUN |  |  |
| AUTO-8 | builder | NOT RUN |  |  |
| AUTO-9 | builder | NOT RUN |  |  |
| AUTO-10 | builder | NOT RUN |  |  |
| AUTO-11 | builder | NOT RUN |  |  |
| AUTO-12 | builder | NOT RUN |  |  |
| P3-01 UAT-1 | fleet-lead | NOT RUN |  |  |
| P3-01 UAT-2 | fleet-lead | NOT RUN |  |  |
| P3-01 UAT-3 | developer | NOT RUN |  |  |
| P3-01 UAT-4 | fleet-lead | NOT RUN |  |  |
| P3-01 UAT-5 | scripter | NOT RUN |  |  |
| P3-01 UAT-6 | scripter | NOT RUN |  |  |
| P3-02 UAT-1 | fleet-lead | NOT RUN |  |  |
| P3-02 UAT-2 | fleet-lead | NOT RUN |  |  |
| P3-02 UAT-3 | fleet-lead | NOT RUN |  |  |
| P3-02 UAT-4 | fleet-lead | NOT RUN |  |  |
| P3-02 UAT-5 | developer | NOT RUN |  |  |
| P3-02 UAT-6 | fleet-lead | NOT RUN |  |  |
| P3-03 UAT-1 | developer | NOT RUN |  |  |
| P3-03 UAT-2 | fleet-lead | NOT RUN |  |  |
| P3-03 UAT-3 | fleet-lead | NOT RUN |  |  |
| P3-03 UAT-4 | fleet-lead | NOT RUN |  |  |
| P3-03 UAT-5 | developer | NOT RUN |  |  |
| P3-03 UAT-6 | scripter | NOT RUN |  |  |
| P3-04 UAT-1 | developer | NOT RUN |  |  |
| P3-04 UAT-2 | developer | NOT RUN |  |  |
| P3-04 UAT-3 | developer | NOT RUN |  |  |
| P3-04 UAT-4 | fleet-lead | NOT RUN |  |  |
| P3-04 UAT-5 | developer | NOT RUN |  |  |
| P3-04 UAT-6 | configurer | NOT RUN |  |  |
| P3-05 UAT-1 | developer | NOT RUN |  |  |
| P3-05 UAT-2 | developer | NOT RUN |  |  |
| P3-05 UAT-3 | fleet-lead | NOT RUN |  |  |
| P3-05 UAT-4 | developer | NOT RUN |  |  |
| P3-05 UAT-5 | developer | NOT RUN |  |  |
| P3-06 UAT-1 | fleet-lead | NOT RUN |  |  |
| P3-06 UAT-2 | fleet-lead | NOT RUN |  |  |
| P3-06 UAT-3 | fleet-lead | NOT RUN |  |  |
| P3-06 UAT-4 | developer | NOT RUN |  |  |
| P3-06 UAT-5 | developer | NOT RUN |  |  |
| P3-06 UAT-6 | developer | NOT RUN |  |  |
| P3-07 UAT-1 | fleet-lead | NOT RUN |  |  |
| P3-07 UAT-2 | developer | NOT RUN |  |  |
| P3-07 UAT-3 | fleet-lead | NOT RUN |  |  |
| P3-07 UAT-4 | developer | NOT RUN |  |  |
| P3-07 UAT-5 | developer | NOT RUN |  |  |
| P3-08 UAT-1 | developer | NOT RUN |  |  |
| P3-08 UAT-2 | developer | NOT RUN |  |  |
| P3-08 UAT-3 | fleet-lead | NOT RUN |  |  |
| P3-08 UAT-4 | developer | NOT RUN |  |  |
| P3-08 UAT-5 | fleet-lead | NOT RUN |  |  |
| XF-1 | developer | NOT RUN |  |  |
| XF-2 | fleet-lead | NOT RUN |  |  |
| XF-3 | developer | NOT RUN |  |  |
| XF-4 | fleet-lead | NOT RUN |  |  |
| XF-5 | fleet-lead | NOT RUN |  |  |
| XF-6 | developer | NOT RUN |  |  |
| XF-7 | developer | NOT RUN |  |  |
| XF-8 | fleet-lead | NOT RUN |  |  |
| XF-9 | developer | NOT RUN |  |  |
| XF-10 | fleet-lead | NOT RUN |  |  |
| XF-11 | fleet-lead | NOT RUN |  |  |
| XF-12 | scripter | NOT RUN |  |  |
| OP-1 | fleet-lead | NOT RUN |  |  |
| OP-2 | fleet-lead | NOT RUN |  |  |
| OP-3 | fleet-lead | NOT RUN |  |  |
| OP-4 | fleet-lead | NOT RUN |  |  |
| OP-5 | fleet-lead | NOT RUN |  |  |
| OP-6 | fleet-lead | NOT RUN |  |  |
| OP-7 | fleet-lead | NOT RUN |  |  |
| OP-8 | fleet-lead | NOT RUN |  |  |
```

Rules: `result` is exactly `PASS`, `FAIL`, `BLOCKED (<cause>)` (causes: `environment`, `not built`, `no access`) or `NOT RUN`. A PASS row carries the first 7 or more characters of the candidate in `commit` and the link `phase-3-evidence.md#<id lower case, spaces to hyphens>` in `evidence`; the row `P3-01 UAT-1` links `phase-3-evidence.md#p3-01-uat-1`. A row that is not PASS may leave `evidence` and `commit` empty but its evidence section should say why. BLOCKED and NOT RUN never count as a pass; there is no waiver state. Personas and runners are those of the source scenarios (Appendix A); AUTO rows are `builder`; the XF rows are `fleet-lead` except `XF-1`, `XF-3`, `XF-6`, `XF-7` and `XF-9` (`developer`) and `XF-12` (`scripter`); the OP rows are `fleet-lead`.

## 5. The evidence file

### 5.1 Shape

`docs/acceptance/v0.7.0/phase-3-evidence.md` starts with `# Phase 3 acceptance evidence` and holds one section per row, in record order:

````markdown
## P3-01 UAT-1

Candidate: <first 7 characters>. Runner: agent tester. Run 1 of 1. Date: 2026-11-20.

```text
<scrubbed output or pane text, at most 60 lines>
```
````

The checker requires, for every PASS row, the `Candidate:` line (7 or more hex characters equal to the start of the candidate, then a full stop or space), exactly one section, and a fenced block with at least one non-blank line and at most 60 lines. The reviewer checks what the script cannot: the runner and date line, the run list, the three-run limit, the operator's comment URL, the row order and persona column, and that every row that is not PASS says why. An operator row holds the operator's words (what they did, the terminals, the counts, the result), the URL of the operator's comment on the issue and, for `OP-8`, the 6 matrix lines and the verdict line.

### 5.2 What to capture, per row kind

AUTO rows: the command and the last 15 lines of its output (the result lines). Source rows: what the source scenario's Evidence line asks for. XF rows: what section 7.3 names. Smoke rows (AUTO-8 to AUTO-12): the whole output including every step line and the `SMOKE_RESULT:` line, with repeated `NOTE` lines removed (the only lines ever removed), and both tab lists. Paste the `exit=<n>` line right after the output, because AC-11 reads it. A smoke section holds the output of the last run only: earlier runs are described in words and no earlier line starts with `PASS `, `SMOKE_RESULT:` or `exit=`. When the scrubbed output is still over 60 lines, write it as consecutive fenced blocks of at most 60 lines each in the same section, in order, none cut in the middle of a line: the checker measures each block, not the section (6.2: `blk` restarts at every opening fence), and AC-11 reads the `SMOKE_RESULT:` line from whichever block holds it. Screenshots are attached to the operator's comment, not committed. Where a scenario's Evidence line asks for a screen of the fleet, keep only the rows the scenario is about (section 5.3).

### 5.3 Scrubbing (before anything is pasted)

```bash
scrub() {
  sed -e "s#$HOME#<home>#g" -e "s#$(id -un)#<user>#g" \
      -e "s#${TMPDIR%/}#<tmp>#g" -e "s#$(cd "${TMPDIR:-/tmp}" && pwd -P)#<tmp>#g" \
      -e "s#$PWD#<worktree>#g"
}
herdr pane read "$PANE" --source recent-unwrapped --lines 200 | scrub > "$X/evidence.txt"
```

Then read the result: no home directory, user name (a login name under four characters cannot be scrubbed safely: say so in the section), temporary path, host name, provider key or ticket, and no profile name other than `default` or `fixture`; replace any other with `<profile>`. Shapes that are new in this unit: the fleet registry lists sessions of every profile the gateway serves, so a pane read of the fleet view or of `talaria fleet list` without `--profile fixture` shows titles and profile names that are not the tester's (the source scenarios pass `--profile fixture`; do the same, and cut any other row); `git` output shows folders and the scratch repositories (replace the temporary prefix with `<tmp>`); `gh` output shows an account name and a repository name (the operator replaces them with `<account>` and `<repo-1>`); a pull request address is written `https://github.com/<account>/<repo-1>/pull/<n>`; the operator's day log names repositories, branches and providers' accounts (replace them with `<repo-1>`, `<repo-2>`; provider ids such as `ollama-cloud` are public and stay; an account or key never appears). The checker catches paths only; AC-12 also greps `--profile` values and each runner's login name. A clean grep proves only that those shapes are absent: the reading step is the control. The operator's comments are public on this issue: the operator scrubs them the same way before posting. An agent that copies one reads it first, and if it leaks asks the operator to edit the comment and copies nothing until it is clean.

## 6. The checker and its tests

### 6.1 Behaviour

`check-phase-3.sh [record [evidence]]` (defaults: `phase-3.md` and `phase-3-evidence.md` next to the script). It holds the 77 row ids in one `IDS=` line. It prints one line per problem, then `ACCEPTANCE_RESULT: PASSED (<n> of 77 rows PASS, candidate <7 hex>)` or `ACCEPTANCE_RESULT: FAILED (<p> problems, <n> of 77 rows PASS)`. Exit 0 when there are no problems, 1 otherwise, 2 when a file is missing. Problems: a row that is not PASS (the line is `<id>: <result>`); a result that is not one of the four forms; a PASS row whose commit is not a prefix of the candidate or is shorter than 7 characters; a PASS row whose evidence link is not the expected one, whose section is missing, empty or longer than 60 lines, or does not name the candidate; a missing, extra or duplicate row; a section that appears twice; a header line that is missing or still a placeholder; a local path anywhere in either file.

The script is a copy of `PH2-ACC`'s checker (its spec 6.2), which is a copy of `PH2A-ACC`'s and `PH1-ACC`'s, with the file names, the id list and the count changed, so the four stay comparable. Rejected: one parametrised script for all four phases, because it would reopen three closed gates' tools; the cost of the copy is about 55 lines. If a command-saving proxy wraps shell commands on the machine, run the checker and its tests through the proxy's pass-through mode or in a plain pane and paste the raw output.

### 6.2 Reference implementation

Written for POSIX `sh` and `awk`; no brace counts in regular expressions; the backtick fence written `\140\140\140`. Exercised while drafting on macOS: the 22 cases below pass, and the skeleton record of section 4 ends `FAILED (77 problems, 0 of 77 rows PASS)` and exits 1. Not run with GNU `awk` or `mawk`: run the cases on a Linux machine before recording `OS: linux`.

```sh
#!/bin/sh
# Check the Phase 3 acceptance record. Usage: check-phase-3.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-3.md}
EVI=${2:-$DIR/phase-3-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-phase-3: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|AUTO-8|AUTO-9|AUTO-10|AUTO-11|AUTO-12|P3-01 UAT-1|P3-01 UAT-2|P3-01 UAT-3|P3-01 UAT-4|P3-01 UAT-5|P3-01 UAT-6|P3-02 UAT-1|P3-02 UAT-2|P3-02 UAT-3|P3-02 UAT-4|P3-02 UAT-5|P3-02 UAT-6|P3-03 UAT-1|P3-03 UAT-2|P3-03 UAT-3|P3-03 UAT-4|P3-03 UAT-5|P3-03 UAT-6|P3-04 UAT-1|P3-04 UAT-2|P3-04 UAT-3|P3-04 UAT-4|P3-04 UAT-5|P3-04 UAT-6|P3-05 UAT-1|P3-05 UAT-2|P3-05 UAT-3|P3-05 UAT-4|P3-05 UAT-5|P3-06 UAT-1|P3-06 UAT-2|P3-06 UAT-3|P3-06 UAT-4|P3-06 UAT-5|P3-06 UAT-6|P3-07 UAT-1|P3-07 UAT-2|P3-07 UAT-3|P3-07 UAT-4|P3-07 UAT-5|P3-08 UAT-1|P3-08 UAT-2|P3-08 UAT-3|P3-08 UAT-4|P3-08 UAT-5|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|XF-9|XF-10|XF-11|XF-12|OP-1|OP-2|OP-3|OP-4|OP-5|OP-6|OP-7|OP-8'
awk -v ids="$IDS" -v evi="$EVI" -v pathbad="$PATHS" '
function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); return s }
function slug(s) { s = tolower(s); gsub(/ /, "-", s); return s }
function bad(msg) { print msg; problems++; rowbad = 1 }
function iscand(s) { return length(s) == 40 && s ~ /^[0-9a-f]+$/ }
BEGIN {
  n = split(ids, list, "|"); for (i = 1; i <= n; i++) want[list[i]] = 1
  fence = "^\140\140\140"
  while ((getline line < evi) > 0) {            # evidence: "## <id>", a Candidate: line, then fenced blocks with text
    if (line ~ fence) { infence = !infence; if (infence) blk = 0; continue }
    if (infence) { if (cur == "") continue; blk++; if (blk > longest[cur]) longest[cur] = blk; if (line ~ /[^ \t]/) text[cur]++; continue }
    if (line ~ /^## /) { cur = trim(substr(line, 4)); sect[cur] = 1; nsect[cur]++ }
    else if (cur != "" && line ~ /^Candidate: /) { split(trim(substr(line, 12)), t, /[. ]/); evcand[cur] = t[1] }
  }
}
/^Candidate: / { cand = trim(substr($0, 12)) }
/^(Hermes|Herdr|OS|Date): / { k = $0; sub(/: .*/, "", k); hdr[k] = trim(substr($0, length(k) + 3)) }
/^\|/ {
  m = split($0, c, "|"); id = trim(c[2]); rowbad = 0
  if (id == "id" || id ~ /^-+$/) next
  if (!(id in want)) { bad("unexpected row: " id); next }
  if (m != 7) { bad(id ": the row needs five cells"); next }
  if (id in seen) { bad(id ": appears twice"); next }
  seen[id] = 1; result = trim(c[4]); evid = trim(c[5]); commit = trim(c[6])
  if (result !~ /^(PASS|FAIL|NOT RUN|BLOCKED \([^)]+\))$/) { bad(id ": the result must be PASS, FAIL, BLOCKED (cause) or NOT RUN, not: " result); next }
  if (result != "PASS") { bad(id ": " result); next }
  if (iscand(cand) && (length(commit) < 7 || index(cand, commit) != 1)) bad(id ": commit " commit " is not the candidate")
  if (evid != "phase-3-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-3-evidence.md#" slug(id))
  if (!(id in sect) || text[id] < 1) bad(id ": the evidence file has no section with a fenced block holding text for it")
  else if (longest[id] > 60) bad(id ": a fenced block of the evidence is longer than 60 lines")
  if (id in sect && iscand(cand) && (length(evcand[id]) < 7 || index(cand, evcand[id]) != 1)) bad(id ": the evidence section must name the candidate (Candidate: <7 hex>.)")
  if (!rowbad) passes++
}
END {
  if (!iscand(cand)) bad("the line Candidate: <40 hex characters> is missing")
  for (k in hdr) if (hdr[k] ~ /[<>]/) bad(k ": the header line is still a placeholder")
  if (!("Hermes" in hdr) || !("Herdr" in hdr)) bad("the header lines Hermes: and Herdr: are missing")
  if (hdr["OS"] !~ /^(macos|linux)$/) bad("OS: must be macos or linux")
  if (hdr["Date"] !~ /^[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]$/) bad("Date: must be YYYY-MM-DD")
  for (i = 1; i <= n; i++) if (!(list[i] in seen)) bad(list[i] ": row missing")
  for (k in nsect) if (nsect[k] > 1) bad(k ": the evidence file has " nsect[k] " sections for it")
  if (pathbad > 0) problems++
  if (problems == 0) printf "ACCEPTANCE_RESULT: PASSED (%d of %d rows PASS, candidate %s)\n", passes, n, substr(cand, 1, 7)
  else printf "ACCEPTANCE_RESULT: FAILED (%d problems, %d of %d rows PASS)\n", problems, passes, n
  exit (problems == 0) ? 0 : 1
}' "$REC"
```

### 6.3 Tests (write these first)

The test script builds a good record and variations of it in a temporary folder, runs the checker on each and compares the exit code and one expected text. It prints `CASE <name>: ok` or `CASE <name>: FAILED (...)` and a final `CHECK_RECORD_TESTS: PASSED` or `CHECK_RECORD_TESTS: FAILED (<n>)`. It reads the id list out of the checker, so the two cannot drift; `all-pass` expects the text `77 of 77`, so a dropped id fails it.

| # | Case | Builds | Expects (exit; text) |
|---|---|---|---|
| 1 | `all-pass` | 77 PASS rows, a section each | 0; `ACCEPTANCE_RESULT: PASSED (77 of 77 rows PASS, candidate <7 hex>)` |
| 2 | `not-run-fails` | `P3-08 UAT-5` NOT RUN | 1; `P3-08 UAT-5: NOT RUN` |
| 3 | `blocked-fails` | `AUTO-5` `BLOCKED (environment)` | 1; `AUTO-5: BLOCKED (environment)` |
| 4 | `fail-fails` | `XF-2` FAIL | 1; `XF-2: FAIL` |
| 5 | `wrong-commit` | `P3-02 UAT-2` commit `deadbeefdead` | 1; `is not the candidate` |
| 6 | `lower-case-result` | `AUTO-1` result `pass` | 1; `the result must be` |
| 7 | `row-missing` | `OP-8` deleted | 1; `OP-8: row missing` |
| 8 | `unexpected-row` | an added row `ZZ-9` | 1; `unexpected row: ZZ-9` |
| 9 | `duplicate-row` | `AUTO-1` twice | 1; `AUTO-1: appears twice` |
| 10 | `no-candidate` | `Candidate: TBD` | 1; `Candidate: <40 hex` |
| 11 | `no-evidence-section` | no section for `P3-07 UAT-5` | 1; `P3-07 UAT-5: the evidence file has no section` |
| 12 | `wrong-evidence-link` | `XF-1` links elsewhere | 1; `XF-1: the evidence link must be` |
| 13 | `local-path` | a home path in the evidence | 1; `a local path appears` |
| 14 | `missing-file` | a record path that does not exist | 2; `not found` |
| 15 | `empty-evidence-block` | the fenced block of `AUTO-2` holds no text | 1; `AUTO-2: the evidence file has no section` |
| 16 | `long-evidence-block` | `AUTO-2` block of 61 lines | 1; `AUTO-2: a fenced block of the evidence is longer than 60 lines` |
| 17 | `short-commit` | `AUTO-3` commit of 6 characters (a true prefix) | 1; `AUTO-3: commit 012345 is not the candidate` |
| 18 | `local-path-in-record` | a `/tmp/<name>` path in the record | 1; `a local path appears` |
| 19 | `placeholder-header` | `Hermes: <pinned release tag>` | 1; `Hermes: the header line is still a placeholder` |
| 20 | `evidence-names-another-candidate` | evidence `Candidate:` line of another commit | 1; `AUTO-1: the evidence section must name the candidate` |
| 21 | `duplicate-section` | `AUTO-2` section twice | 1; `AUTO-2: the evidence file has 2 sections for it` |
| 22 | `default-arguments` | the two files next to the script, no arguments | 0; `ACCEPTANCE_RESULT: PASSED` |

The checker ignores heading-like lines inside a fenced block and counts a duplicated section; `XF-6`'s block in case 1 starts with a line that looks like a heading to prove it. This is `PH1-ACC`'s spec 6.2, `PH2A-ACC`'s and `PH2-ACC`'s, so the four agree; if they ever disagree, report it on `PH1-ACC`; this unit follows the script.

The stub the test author commits first is

```sh
#!/bin/sh
# stub: the real checker replaces this file
IDS='<the same IDS line as in the reference checker>'
echo "ACCEPTANCE_RESULT: FAILED (stub)"; exit 1
```

so every case fails on its expectation (a stub without the `IDS=` line would make every case build an empty record and fail for the wrong reason). Copying the reference test script below is allowed and expected; independence means the author is not the developer who writes the checker.

````sh
#!/bin/sh
# Tests for check-phase-3.sh. Run: sh docs/acceptance/v0.7.0/check-phase-3.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-phase-3.sh"
T=$(mktemp -d); trap 'rm -rf "$T"' EXIT
C=0123456789abcdef0123456789abcdef01234567
S=$(echo "$C" | cut -c1-7)
IDS=$(sed -n "s/^IDS='\(.*\)'\$/\1/p" "$CHECK" | tr '|' '\n')
fails=0

# make_record <file> <id-to-change> <result> <commit>   (an unchanged record has every row PASS on the candidate)
make_record() {
  { printf 'Candidate: %s\nHermes: v0.22.4\nHerdr: 0.9.4\nOS: macos\nDate: 2026-11-02\n\n' "$C"
    printf '| id | persona | result | evidence | commit |\n|---|---|---|---|---|\n'
    echo "$IDS" | while IFS= read -r id; do
      slug=$(echo "$id" | tr 'A-Z ' 'a-z-'); res=PASS; com=$(echo "$C" | cut -c1-12)
      [ "$id" = "$2" ] && { res=$3; [ -n "$4" ] && com=$4; }
      echo "| $id | developer | $res | phase-3-evidence.md#$slug | $com |"
    done; } > "$1"
}
section() {   # section <id> <block text lines>: one evidence section with a runner line and a fenced block
  printf '## %s\n\nCandidate: %s. Runner: test.\n\n```text\n%s\n```\n\n' "$1" "$S" "$2"
}
make_evidence() {   # XF-6's block starts with a line that looks like a heading: it must not end the section
  echo "$IDS" | while IFS= read -r id; do
    if [ "$id" = "$2" ]; then :; elif [ "$id" = "XF-6" ]; then section "$id" "$(printf '## not a heading\nok')"; else section "$id" ok; fi
  done > "$1"
}
expect() {   # expect <name> <expected exit> <expected text> <record> <evidence>
  out=$(sh "$CHECK" "$4" "$5"); code=$?
  if [ "$code" = "$2" ] && echo "$out" | grep -qF -- "$3"; then echo "CASE $1: ok"; else echo "CASE $1: FAILED (exit $code; wanted $2 and '$3'; got: $out)"; fails=$((fails+1)); fi
}

make_evidence "$T/ev.md" none
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (77 of 77 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "P3-08 UAT-5" "NOT RUN";               expect not-run-fails 1 "P3-08 UAT-5: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-5" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-5: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P3-02 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| OP-8' "$T/good.md" > "$T/f.md";            expect row-missing 1 "OP-8: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P3-07 UAT-5";               expect no-evidence-section 1 "P3-07 UAT-5: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-3-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
{ cat "$T/ev.md"; printf 'see /%s/someone/project\n' Users; } > "$T/ev3.md"; expect local-path 1 "a local path appears" "$T/good.md" "$T/ev3.md"
expect missing-file 2 "not found" "$T/nope.md" "$T/ev.md"
make_evidence "$T/ev4.md" "AUTO-2"; section AUTO-2 "" >> "$T/ev4.md"; expect empty-evidence-block 1 "AUTO-2: the evidence file has no section" "$T/good.md" "$T/ev4.md"
make_evidence "$T/ev5.md" "AUTO-2"; section AUTO-2 "$(seq 1 61)" >> "$T/ev5.md"; expect long-evidence-block 1 "AUTO-2: a fenced block of the evidence is longer than 60 lines" "$T/good.md" "$T/ev5.md"
make_record "$T/k.md" "AUTO-3" PASS 012345;            expect short-commit 1 "AUTO-3: commit 012345 is not the candidate" "$T/k.md" "$T/ev.md"
{ cat "$T/good.md"; printf 'see /%s/someone/project\n' tmp; } > "$T/l.md"; expect local-path-in-record 1 "a local path appears" "$T/l.md" "$T/ev.md"
sed 's/^Hermes: .*/Hermes: <pinned release tag>/' "$T/good.md" > "$T/m.md"; expect placeholder-header 1 "Hermes: the header line is still a placeholder" "$T/m.md" "$T/ev.md"
sed 's/^Candidate: 0123456\./Candidate: deadbee./' "$T/ev.md" > "$T/ev6.md"; expect evidence-names-another-candidate 1 "AUTO-1: the evidence section must name the candidate" "$T/good.md" "$T/ev6.md"
make_evidence "$T/ev7.md" none; section AUTO-2 ok >> "$T/ev7.md"; expect duplicate-section 1 "AUTO-2: the evidence file has 2 sections for it" "$T/good.md" "$T/ev7.md"
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-3.md" && cp "$T/ev.md" "$T/d/phase-3-evidence.md"
out=$(sh "$T/d/check-phase-3.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
````

The red run of the record, taken at the commit that adds the real checker while the committed record is still the skeleton of section 4 with the real candidate and real header lines: `sh check-phase-3.sh` must end `ACCEPTANCE_RESULT: FAILED (77 problems, 0 of 77 rows PASS)` and exit 1 (checked while drafting).



## 7. The scenario catalogue

Every row has an id, a persona, a runner, steps, an observable result and the evidence to capture. Runner words: `agent tester` is an agent working in a Herdr pane; `operator` is the human. All commands run at the worktree root unless a row says otherwise. `R=infiquetra/talaria`, `X=$(mktemp -d)` for evidence scratch.

### 7.1 AUTO rows (persona builder; agent tester)

Each row lists its commands, what makes it PASS and what to paste as evidence. A test target that does not exist at the candidate is `BLOCKED (not built)`, never skipped.

**AUTO-1: project check and the candidate's continuous integration (CI) results.** The `bench` workflow runs on `pull_request` and `workflow_dispatch` (P1-11 REQ-20). GitHub attaches every workflow run to its head commit as check runs, so a dispatch run on the candidate branch should report on the candidate; this is **verified at the first use** (if it does not, the row is BLOCKED (environment) and reported on P1-11). The lead can learn the answer before the freeze: in stage 0 dispatch `bench.yml` on the branch `p3-acc-smoke` and read `gh api "repos/$R/commits/<its head sha>/check-runs"`; if no `bench` check run is attached, report it on P1-11 then, so the cause is fixed before the candidate exists (a workflow fix after the freeze makes a new candidate).

```bash
gh workflow run bench.yml --ref p3-acc-candidate -R "$R"
# poll every 10 s, at most 5 minutes, until a run whose headSha is the candidate exists (an older run on the branch does not count)
RUN=$(gh run list -R "$R" --workflow bench.yml --branch p3-acc-candidate --json databaseId,headSha --jq ".[] | select(.headSha == \"$CANDIDATE\") | .databaseId" | head -1)
gh run watch "$RUN" -R "$R" --exit-status
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
gh api --paginate "repos/$R/commits/$CANDIDATE/check-runs" --jq '.check_runs[] | "\(.name) \(.conclusion)"'
shasum -a 256 target/release/talaria
```

PASS: `gh run watch` exits 0 on the run for the candidate; the cargo set exits 0; every check-run line ends `success` or `skipped` (a line ending `null` is still running: wait and read again; `failure`, `cancelled`, `timed_out` and `action_required` are a FAIL) and `bench success` is among them (`bench` is the job name of P1-11's workflow: stage 0 reads the real name from the dispatch run, and the lead edits this row and AUTO-1 of the issue if it differs). Evidence: run id and `headSha`, exit codes, the check-run lines, the checksum.

**AUTO-2: replay suites** (tests that play recorded Hermes frames back). The six phase 3 replay targets, then the earlier phases' replay targets again (a phase 3 change can break them).

```bash
cargo test -p talaria-app --test fleet_replay
cargo test -p talaria-app --test peek_replay
cargo test -p talaria-app --test launch_replay
cargo test -p talaria-app --test subagent_replay
cargo test -p talaria-app --test background_replay
cargo test -p talaria-app --test attention_fleet_replay
cargo test -p talaria-app --test model_replay
cargo test -p talaria-app --test directory_replay
cargo test -p talaria-app --test queue_replay
cargo test -p talaria-app --test rewind_replay
cargo test -p talaria-app --test context_replay
cargo test -p talaria-app --test automation_replay
cargo test -p talaria-app --test permissions_replay
cargo test -p talaria-app --test headless_flow mock_completion_matches_recorded_shape
cargo test -p talaria-app --test tool_replay
cargo test -p talaria-tui --test pager_replay
cargo test -p talaria-app --test commands_replay
cargo test -p talaria-app --test attachments_replay
cargo test -p talaria-app --test side_replay
cargo test -p talaria-tui --test checklist_replay
cargo test -p talaria-app --test appearance_replay
cargo test -p talaria-app --test attention_replay
```

PASS: each exits 0 and prints `^test result: ok\. [1-9][0-9]* passed` (a target that ran no test prints `0 passed` and is a FAIL). Evidence: the `test result` lines.

**AUTO-3: the eight issues' unit suites** (every test target the issues name, except the replay, budget-only and live ones), then the whole workspace once more.

```bash
cargo test -p talaria-core --test fleet_rows --test fleet_queue --test fleet_registry --test fleet_subscribed
cargo test -p talaria-app --test fleet_wire --test fleet_foreground --test fleet_app
cargo test -p talaria-cli --test fleet_cmd
cargo test -p talaria-core --test fleet_view_rows --test fleet_view_keys
cargo test -p talaria-app --test peek_wire --test fleet_view_flow
cargo test -p talaria-tui --test fleet_view_screens
cargo test -p talaria-app --test launch_names --test launch_machine --test launch_outcome --test worktree_git --test launch_wire --test launch_tui_flow
cargo test -p talaria-tui --test launch_dialog
cargo test -p talaria-cli --test bg_cmd
cargo test -p hermes-client --test fixture_scan
cargo test -p talaria-core --test subagent_tree --test subagent_batch --test commands_local_agents
cargo test -p talaria-app --test subagent_wire
cargo test -p talaria-tui --test subagent_view
cargo test -p talaria-core --test background_process --test background_props --test background_bg --test background_commands --test background_clock
cargo test -p talaria-tui --test background_render
cargo test -p talaria-app --test background_service
cargo test -p talaria-core --test fleet_attention --test alert_gate
cargo test -p talaria-tui --test status_fleet_segments --test status_line --test attention_keys --test fleet_alert_bytes --test keymap_doc
cargo test -p talaria-app --test attention_fleet --test config_fleet_attention --test config_doc
cargo test -p talaria-core --test review_parse --test review_state --test review_ship --test review_pr --test review_props
cargo test -p talaria-app --test review_git --test review_gh --test review_locate --test review_draft --test review_flow --test review_timing
cargo test -p talaria-tui --test review_render --test fleet_pr_column --test status_pr_segment
cargo test -p talaria-core --test fleet_view_pane
cargo test -p talaria-app --test attach_resolve --test attach_wire --test panes_plan --test panes_exec --test fleet_pane_flow
cargo test -p talaria-cli --test attach_cli
cargo test -p talaria-tui --test fleet_pane_screens
cargo test -p talaria-app -p talaria-cli
cargo test -p talaria-tui
cargo test --workspace
```

PASS: every command exits 0, and a command naming k `--test` targets prints at least k lines `test result: ok. N passed` with N at least 1. The phase 1 and 2 unit suites run inside `cargo test --workspace` and are not repeated by name. The tests marked `#[ignore]` are run by AUTO-4, AUTO-5 and AUTO-7 or by the operator. `P3-07`'s `review_gh` includes its test `recordings_are_public_safe`; a `BLOCKED` line in any output (a recording the operator did not make) is `BLOCKED (not built)`. Evidence: the `test result` lines and exit codes.

**AUTO-4: performance budgets on the real release binary and the units' own benchmarks.**

```bash
target/release/talaria-bench run --binary target/release/talaria --report "$X/bench.json" > "$X/bench.txt"; echo "exit=$?"
grep -cE ' FAIL$' "$X/bench.txt"; grep -cE ' PASS$' "$X/bench.txt"
cargo test -p talaria-bench --test real_binary release_budgets_hold -- --ignored
cargo bench -p talaria-core --bench fleet_registry
jq -e '.median.point_estimate < 1000000' target/criterion/fleet_update_200/new/estimates.json
cargo test -p talaria-app --test fleet_wire -- --exact start_returns_at_once
cargo test -p talaria-app --test fleet_wire -- --exact lists_every_served_profile_with_exact_params
cargo bench -p talaria-tui --bench fleet_view
cargo test -p talaria-app --test fleet_view_flow -- first_frame idle_and_streaming
cargo bench -p talaria-core --bench subagent_tree
cargo bench -p talaria-tui --bench subagent_panel
cargo bench -p talaria-core --bench background_output
cargo bench -p talaria-tui --bench background_overlay
cargo test -p talaria-app --test launch_tui_flow
cargo test -p talaria-cli --test bg_cmd
cargo test -p talaria-app --test attention_fleet -- no_new_frame_when_counts_unchanged
cargo test -p talaria-app --test review_timing
cargo test -p talaria-app --test fleet_pane_flow
```

PASS: `exit=0`, the FAIL count 0, the PASS count 1 or more; the ignored test passes; the `jq` exits 0 (the 200-row registry update is under 1 ms); every `--exact` run reports `1 passed`; the benches each exit 0 and the `subagent_panel` and `subagent_tree` tables have every row `pass`; the named tests pass. The budgets are those of [05-architecture.md](../05-architecture.md#performance-budgets) and each unit's own (`P3-01` REQ-19, `P3-02` REQ-19, `P3-03` B1 to B3, `P3-04` REQ-37, `P3-05` budgets, `P3-06` AC-10, `P3-07` and `P3-08` timing tests). Evidence: the `bench.txt` table from its `os:` line, the two counts, the `jq` exit, the `1 passed` lines, the exit codes.

**AUTO-5: live suites with no skip.** Operator preconditions hold (issue, Dependencies); each command runs in its own shell, one at a time (they share one gateway and one free model), with the quiet-fleet rule (section 3) between them.

```bash
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_fleet -- --ignored --nocapture 2>&1 | tee "$X/l1.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_fleet_view -- --ignored --nocapture 2>&1 | tee "$X/l2.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_bg -- --ignored --nocapture 2>&1 | tee "$X/l3.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_subagents -- --ignored --nocapture 2>&1 | tee "$X/l4.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_background -- --ignored --nocapture 2>&1 | tee "$X/l5.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_attention -- --ignored --nocapture 2>&1 | tee "$X/l6.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_review -- --ignored --nocapture 2>&1 | tee "$X/l7.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_attach_cmd -- --ignored --nocapture 2>&1 | tee "$X/l8.txt"
```

PASS (each test has its own signal): `LIVE_FLEET_RESULT: PASSED` four times and one `SESSIONS_CHANGED_SIBLING:` line in `l1.txt` (`needs_you_p95_ms` is printed there and at most 5000: `P3-01` UAT-2 reads it); `LIVE_FLEET_VIEW_RESULT: PASSED` four times, one `peek_open_ms=` line and `QUEUED_AFTER_STOP: started` in `l2.txt`; `LIVE_BG_RESULT: PASSED` four times in `l3.txt`; `LIVE_SUBAGENT_RESULT: PASSED` three times in `l4.txt` (the pause test skips without the operator's `TALARIA_LIVE_PAUSE=1`: that one skip line is allowed here and is run by the operator in stage 3, row `P3-04 UAT-6`); `LIVE_BACKGROUND_RESULT: PASSED` three times and the lines `BG_SESSION_LISTED:`, `TERMINAL_OUTPUT_EVENTS:`, `KILL_LATENCY_MS:` and `BG_APPROVAL:` in `l5.txt`; `LIVE_ATTENTION_RESULT: PASSED` twice in `l6.txt`; `test result: ok` with 0 failed in `l7.txt`; `LIVE_TALARIA_ATTACH_RESULT: PASSED` once in `l8.txt`; no `SKIPPED` in any file except that one pause line (`grep -c SKIPPED "$X"/l?.txt` prints 0 for each file but `l4.txt`, which prints 1). A skip or a failure that is the environment's is `BLOCKED (environment)`, never a pass. The two live tests that need the operator (`live_review_gh`, which needs `gh` and a throwaway repository, and the pause test) are run by the operator with their rows (section 7.2) and pasted there. Evidence: the result lines and the counts.

**AUTO-6: hygiene scans.**

```bash
cargo run -q -p record-fixture -- scan fixtures/frames
! git grep -nE 'unsafe *(\{|fn |impl )' -- crates tools
! git grep -nE '/(Users|home|private|var)/[A-Za-z]' -- crates tools docs/formats
! git grep -n 'unattended' -- tools/smoke ':!tools/smoke/tests/typed_input.rs' ':!tools/smoke/tests/typed_input2a.rs' ':!tools/smoke/tests/typed_input2b.rs' ':!tools/smoke/tests/typed_input3.rs' ':!tools/smoke/README.md'
git grep -nE 'end_reason|unattended' -- crates/talaria-app/src crates/talaria-cli/src | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
! grep -rlE '"unattended": *true' fixtures/frames
git grep -nE 'session\.(resume|create|detach|interrupt)|prompt\.submit|approval\.respond|config\.set|unattended' -- crates/talaria-app/src/fleet crates/talaria-cli/src/fleet_cmd.rs | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
git grep -nE 'session\.create|session\.mutate|config\.set|"unattended"|"end_reason"|"queued"[[:space:]]*:' -- crates/talaria-app/src/peek.rs crates/talaria-app/src/fleet_actions.rs 'crates/talaria-core/src/fleet_view*' 'crates/talaria-tui/src/fleet_view*' | grep -vE '^[^:]+:[0-9]+:[[:space:]]*//'
grep -vE '^\s*//' crates/talaria-app/src/bg/wire.rs | grep -nE '\b(title|hidden|cwd_explicit|finite|unattended)\b'
git grep -nE -- '--force|force-with-lease|push -f|"\+refs/|prompt\.submit|"unattended"|"finite"' -- crates/talaria-app/src/review/ crates/talaria-core/src/review/
git grep -nE '"(include_hidden|end_reason|unattended|title)"' -- crates/talaria-app/src/attach.rs crates/talaria-app/src/panes.rs crates/talaria-cli/src/attach_cmd.rs
! git grep -hoE -e '--profile[ =]+"?[A-Za-z0-9_-]+' -- crates tools | sed -E 's/.*[ =]"?//' | grep -vxE 'default|fixture|nosuchprofile|nosuch'
target/release/talaria --help | grep -cE -- '--bg|--worktree'
target/release/talaria attach --help | head -3
grep -c 'talaria.fleet.v1' docs/formats/fleet-list-v1.md; grep -c 'talaria.bg.v1' docs/formats/bg-result-v1.md
```

PASS: the `record-fixture` scan exits 0; every other scan line, marked `!` or not, prints nothing, except the last four lines, which print a count of 2 or more for `--bg|--worktree`, the `attach` usage, and counts of 1 or more for the two format files. Where a scan prints a hit, the tester reads it and quotes it with its owner: the first listed scan lines were written as diffs against `origin/main` in the source issues and are rerun here as whole-tree scans of the same files with comment lines removed, so a hit is either a real defect (FAIL, owner named) or a mistake in the scan (the lead corrects this row and notes it). The `config.set` scan of phase 2 (`git grep -nE 'config\.set' -- crates/talaria-app/src crates/talaria-cli/src`) is not rerun: phase 3 adds no configuration write. A fixture or file named here that does not exist is `BLOCKED (not built)`. The `unattended` scan has no exclusion for source files: no file under `tools/smoke/src`, code or comment, holds the word (write "the auto-approve flag"); only the four typed-input test files and the README may. `nosuchprofile` and `nosuch` are allowed in the profile scan because the earlier issues type them as deliberately unknown names. Evidence: the command lines that printed something, the counts.

**AUTO-7: smoke suite offline tests and the earlier acceptance tools.**

```bash
cargo test -p smoke
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$'
cargo run -q -p smoke -- run --help
sh docs/acceptance/v0.7.0/check-record.test.sh | tail -1
sh docs/acceptance/v0.7.0/check-phase-2a.test.sh | tail -1
sh docs/acceptance/v0.7.0/check-phase-2.test.sh | tail -1
sh docs/acceptance/v0.7.0/check-record.sh | tail -1
sh docs/acceptance/v0.7.0/check-phase-2a.sh | tail -1
sh docs/acceptance/v0.7.0/check-phase-2.sh | tail -1
```

PASS: all tests pass; the ignored list is exactly four lines naming `live_full_smoke_in_herdr`, `live_phase2a_smoke_in_herdr`, `live_phase2b_smoke_in_herdr` and `live_phase3_smoke_in_herdr`; the help lists `--talaria`, `--herdr`, `--profile`, `--model`, `--provider`, `--deadline-secs`, `--step-scale`, `--report-dir`, `--suite` and `--tmux`, and names the four suites; the three test scripts end `CHECK_RECORD_TESTS: PASSED` and the three checkers end `ACCEPTANCE_RESULT: PASSED (<n> of <n> rows PASS, candidate <7 hex>)` (the earlier records are for their own candidates and must still read as accepted: this proves this unit did not disturb them; for each earlier gate that was accepted under shape B the files do not exist and its two lines are dropped before the freeze by editing the issue). Evidence: the `test result` line, the ignored lines, the help, the six tail lines.

**AUTO-8, AUTO-9, AUTO-10, AUTO-11, AUTO-12: the smoke suites, back to back at the end of stage 1** with no other live scenario between them (they share one gateway and one free model). The tester's quiet-fleet check of section 3 (a wait of up to 5 minutes, not a scenario) runs before each of them: the last act of a phase 3 run is a denied approval whose turn may still be working when the run ends.

```bash
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria "$T" > "$X/s1.txt" 2>&1; echo "exit=$?"                          # AUTO-8
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2a --talaria "$T" > "$X/s2.txt" 2>&1; echo "exit=$?"      # AUTO-9
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2b --talaria "$T" > "$X/s3.txt" 2>&1; echo "exit=$?"      # AUTO-10
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase3 --talaria "$T" > "$X/s4.txt" 2>&1; echo "exit=$?"       # AUTO-11
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase3 --talaria "$T" > "$X/s5.txt" 2>&1; echo "exit=$?"       # AUTO-12
herdr tab list --workspace "$HERDR_WORKSPACE_ID"; tmux ls 2>&1 | grep -c talaria-smoke
```

PASS: AUTO-8 prints six `PASS` lines (P1-13's steps) and `SMOKE_RESULT: PASSED (elapsed <n> s)` with n under 300; AUTO-9 prints 21 `PASS` lines and the same result line with n under 1800; AUTO-10 prints 15 `PASS` lines and n under 1500 (both earlier suites run once each as regression rows); AUTO-11 and AUTO-12 each print 12 `PASS` lines and the same result line with n under 1200 (the deadline, section 9.6); every `exit=0`; no `talaria-smoke-` tab in the second list and the last count is 0. The defaults are P1-13's (profile `fixture`, model `minimax-m3.1-flash`, provider `ollama-cloud`), so no such flag is passed, and P1-13 makes the `--talaria` path absolute itself. A suite prints `PASSED WITH SKIPS` and exits 3 when a step that may skip does, and with `TALARIA_LIVE_REQUIRE=1` a skip fails the run; neither is a pass: the row is BLOCKED (environment), section 8.1 gives the label. The suites pass `--no-ensure` to `talaria` themselves. Evidence: each whole output (section 5.2) and both tab lists.



### 7.2 Source rows (45 scenarios)

Run each as written in Appendix A, with the four allowed changes of the issue's REQ-11:

1. `target/release/talaria` (the absolute `$T`) wherever a scenario says `talaria`, `"$T"`, `<talaria>` or the debug path (`P3-08` already builds a release binary and uses `T` for it: use the candidate's `$T`; `P3-04` UAT-5's and others' `talaria --profile fixture` lines are shorthand for the same).
2. `--no-ensure` on every launch of `talaria` that starts a live session and lacks it (every live scenario's start line lacks it, except those that already carry it). `talaria fleet list` and `talaria fleet list --watch` take no such option and never start a gateway.
3. Every `smoke run` that lacks `--talaria` gets `--talaria "$T"` (none of the 45 source scenarios runs `smoke`).
4. Where Herdr rejects the pane-split form a scenario writes, the helper `pane_new` of section 3; say so in the evidence.

These are not changes: cleanup before a scenario starts (close a leftover `talaria-smoke-` tab, answer or clear a leftover session by the quiet-fleet rule of section 3); the demo programs built by `cargo run` in debug (they are tools, not the product: `P3-04`'s `subagents_demo` is one); a key name Herdr rejects when the scenario itself allows another. A test target or demo that no longer exists is `BLOCKED (not built)`.

Order within an issue is the scenario number order, and the issues run in the order of the record (P3-01 to P3-08). Dependencies and notes:

- Every issue: the preconditions are the union of the issues' own (the issue's Dependencies): the pinned gateway serving `default` and `fixture`; `fixture` on `minimax-m3.1-flash` with provider `ollama-cloud`, `approvals.mode: manual`, the local terminal backend, no `approvals.deny` rules, delegation enabled (the default); scheduled Hermes jobs paused for the run (`P3-06`); `git` 2.28 or later, `tmux`, `jq`, `gh`, Herdr. Approvals wait 300 s at most: answer within 240 s.
- `P3-01`: UAT-2, UAT-3 and UAT-4 build on each other (UAT-3 starts after UAT-2's approval was denied; the quiet-fleet rule applies after UAT-3). UAT-5 needs no gateway and must not start one. UAT-6 needs the gateway.
- `P3-02`: two panes; B's session is started in each scenario by a prompt typed in B and then `/quit` (the session keeps working in the gateway). UAT-2 begins within 10 s of B's prompt; UAT-3's digit within 240 s of B's card; UAT-5 needs UAT-1's finished session; UAT-6 is UAT-4's setup before stopping.
- `P3-03`: a scratch repository with a bare `origin` one commit ahead of `main`. UAT-4 leaves an approval that the scenario itself denies; UAT-6 needs no gateway. After UAT-2 and UAT-3 the worktrees under `.worktrees/` stay until closeout (the scratch repository is deleted with the rest of `$S` at the end of the row; Talaria never deletes a worktree).
- `P3-04`: needs a terminal at least 100 columns wide (`tput cols`). The model may not start a subagent: each scenario says when to retry, and a model that never starts one on three tries is BLOCKED (environment: the model did not start a subagent) with the three screens. `P3-04 UAT-6` is the operator's.
- `P3-05`: `tick.sh` runs 600 s; after UAT-3 and UAT-5 the tester runs `pgrep -fx 'sh tick.sh' | wc -l` and expects 0 (kill leftovers). If the agent does not start the process the scenario says to retry once in a fresh session, then BLOCKED (environment).
- `P3-06`: the scenarios chain in order 1 to 6 (UAT-2 continues UAT-1's pane). Its common setup names its scratch folder `T="$(mktemp -d)"`, which collides with the common settings' `T` (the binary). The tester renames that folder variable `TD` everywhere in the six scenarios (`TD="$(mktemp -d)"`, `"$TD/talaria-attention-probe"`, `"$TD/bN.raw"`) and keeps `$T` for the binary; this is a rename under change (1), not a change of the scenario, and the evidence says so. The bell and notification bytes are read from a `script` capture file, not from the screen; on macOS and Linux the `script` form differs and the scenario gives both. UAT-3 and UAT-4 each wait up to 90 s.
- `P3-07`: UAT-1 uses `--bg --worktree`, so run it after `P3-03` is known good (it is, by AUTO-5). `P3-07 UAT-3` is the operator's (it needs `gh` signed in and a throwaway GitHub repository). UAT-2's drafting waits up to 60 s for the model.
- `P3-08`: UAT-1 to UAT-5 chain through the session named `attach-uat-<tag>`; UAT-4 needs an isolated `tmux` server and no Herdr variables; UAT-2 needs a session titled alike in `default` and `fixture` (if `default` cannot be written to, that row is BLOCKED (environment) with the reason, `P3-08` U4). Redact before pasting (the scenario says how).

Operator rows among them: `P3-04 UAT-6` and `P3-07 UAT-3`. All other rows are run by the agent tester in stage 2. The operator posts one comment per operator row (section 7.4), and an agent copies it into the evidence. The two ignored live tests that need the operator run in the same sittings: `TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_PAUSE=1 cargo test -p talaria-app --test live_subagents live_pause -- --ignored --nocapture` (prints `LIVE_SUBAGENT_RESULT: PASSED`) with `P3-04 UAT-6`, and `TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_review_gh -- --ignored --nocapture` (0 failed, no `SKIPPED`) with `P3-07 UAT-3`; the operator pastes both outputs in the row's comment.

### 7.3 Cross-feature rows XF-1 to XF-12 (agent tester)

These need the live gateway, profile `fixture`, and the common settings of section 3 (every pane starts with `. ../acc.env`), and every Talaria pane at least 100 columns by 24 rows (section 3, pane width). Each row uses its own folder `P=$(mktemp -d)` and its own panes, each made with `PANE=$(pane_new "$P")`. "Start Talaria in `$PANE`" means `herdr pane run "$PANE" "$T $F"` then `herdr pane wait-output "$PANE" --match minimax-m3.1-flash --source visible --timeout 60000`. Before starting Talaria in a pane type `echo READ-CHECK` there and confirm which of `visible` and `recent-unwrapped` shows it; use that source for the row (a read that lacks text just typed is stale and never evidence). "Submit" is the `submit` helper (two calls). `<M2>` is any other Ollama Cloud model that the `/model` picker lists for `fixture` (the first one listed after `minimax-m3.1-flash`), chosen once and written in the evidence; with no second model the rows that need it (XF-2, XF-11) are BLOCKED (environment). A `git` identity for scratch repositories: `G="git -c user.name=fixture -c user.email=fixture@example.invalid"`. Prompts are never eased. If after 8 s nothing happened (a still screen, no card, no counting line), send `herdr pane send-keys "$PANE" enter` once and say so in the evidence. A model that fails a plain prompt makes a run fail without being a Talaria defect: rerun once and record both runs; a failing second run stays FAIL. Wait for a word with `herdr pane wait-output "$PANE" --match <word> --source recent-unwrapped --timeout <ms>`. The quiet-fleet rule of section 3 runs before and after every row. Every approval a row provokes is answered within 240 s of its card.

Quoted texts below are the source issues' own; where a Then line of a source issue differs from a text here, the source issue wins and the difference is reported.

**XF-1: The phase 3 commands resolve in the palette as Talaria commands, and their keys do nothing surprising when nothing is there.** Covers `P2-03` with `P3-02`, `P3-04`, `P3-05` and `P3-07` (the local command table; local wins over Hermes) and `P3-04` with `P3-06` (the two `alt` keys).
- Steps: start Talaria. For each name in `fleet agents processes bg changes`: `herdr pane send-text "$PANE" "/$name"`; `sleep 1`; `vis "$PANE" > "$X/p-$name.txt"`; `herdr pane send-keys "$PANE" esc`; `sleep 0.4`; `herdr pane send-keys "$PANE" ctrl+c`. Never press Enter. Then `herdr pane send-keys "$PANE" alt+s`; `sleep 1`; `vis "$PANE" > "$X/k1.txt"`; `herdr pane send-keys "$PANE" esc`; `sleep 0.4`; `herdr pane send-keys "$PANE" alt+a`; `sleep 1`; `vis "$PANE" > "$X/k2.txt"`.
- Pass: for every name, `grep -c 'Tab complete' "$X/p-$name.txt"` prints 1 or more (the palette was open: an empty or stale read is not a pass), `grep -E "(^| )/$name( |$)" "$X/p-$name.txt" | grep -cw talaria` prints 1 or more and `grep -E "(^| )/$name( |$)" "$X/p-$name.txt" | grep -cw hermes` prints 0; `k1.txt` holds `No subagents in this session yet.`; `k2.txt` holds `Nothing in the background needs you.` and no overlay. 5 names, 5 reads, 2 key reads.
- Evidence: the 5 row lines (the line holding `/<name>` and its tag) and the two key lines.

**XF-2: Three background sessions, one fleet, an approval answered from the peek panel while typing in a fourth session (the agent's rehearsal of requirement R6).** Covers `P3-01`, `P3-02`, `P3-03`, `P3-06` with `P2-06`'s `--model` flag. Needs `<M2>` (else BLOCKED (environment)).
- Steps: `P=$(mktemp -d)`; for `d` in `p1 p2 p3 p4`: `mkdir "$P/$d"; $G -C "$P/$d" init -q -b main` (an empty commit in each) and `mkdir "$P/p2/probe"`. In a shell pane at `$P`: `(cd "$P/p1" && $T $L --model minimax-m3.1-flash 'Run the shell command sleep 40, then reply with the word alpha.') > "$X/l1.json"`; `(cd "$P/p2" && $T $L --model <M2> 'Run this exact shell command: rm -rf ./probe') > "$X/l2.json"`; `(cd "$P/p3" && $T $L --model minimax-m3.1-flash 'Reply with only the word gamma.') > "$X/l3.json"`; each prints one line with `"status":"launched"` and a `session_id`. Start Talaria in `$PANE` at `$P/p4` (the fourth session); `herdr pane send-text "$PANE" "<200 characters of the text abcdefghi repeated, no Enter>"` (call it `D`). Poll `vis "$PANE"` every 5 s for up to 90 s until its status line holds `⚑ 1 needs you`. Then `fleet --json > "$X/f1.json"`; `herdr pane send-keys "$PANE" alt+a`; `sleep 1`; `vis "$PANE" > "$X/v1.txt"`; `herdr pane send-keys "$PANE" a`; `sleep 1`; `vis "$PANE" > "$X/v2.txt"`; press the digit that `grep -oE '[0-9]+\. deny' "$X/v2.txt"` shows; `sleep 5`; `herdr pane send-keys "$PANE" esc` three times (0.4 s apart); `vis "$PANE" > "$X/v3.txt"`; `ls -d "$P/p2/probe"`. Wait until `fleet --json | jq -r '.rows[] | .state'` shows no `working` (at most 180 s); `fleet --json > "$X/f2.json"`.
- Pass: the three ids of `l1.json` to `l3.json` are all in `f1.json` under `.rows[].session_id`; `jq '.needs_you | length' "$X/f1.json"` prints 1 and that item's `session_id` is `l2.json`'s; `v1.txt` holds `rm -rf ./probe` (the peek of that row); `v2.txt` holds a line `<n>. deny`; `v3.txt` holds no `⚑` segment, still shows the fourth session's own composer holding the first 40 and the last 40 characters of `D` (the draft wraps in a narrow composer, so reassemble it first: `sed -E 's/^[│ ]+//; s/[│ ]+$//' "$X/v3.txt" | tr -d '\n' > "$X/v3.flat"`, then `grep -cF "<first 40>" "$X/v3.flat"` and `grep -cF "<last 40>" "$X/v3.flat"` each print 1 or more; `D` holds no space, so trimming the line edges loses nothing), and its status line still names `minimax-m3.1-flash`; `ls -d "$P/p2/probe"` prints the folder; in `f2.json` no row has state `failed` and none is in `needs_you`; the row of `l2.json`'s session names `<M2>` (`jq -r --arg id "$(jq -r .session_id "$X/l2.json")" '.rows[] | select(.session_id==$id) | tostring' "$X/f2.json" | grep -c "<M2>"` prints 1 or more). Which state the first and third sessions ended in (`done` or `idle`) is not asserted.
- Evidence: the three launch lines (ids shortened to 8 characters), the counts, the key lines of `v1`, `v2`, `v3`, the `ls` output.

**XF-3: A session outlives the Talaria that started it, and is reopened by title (requirement R5).** Covers `P3-08` with `P3-01` and `P1-12`'s quit.
- Steps: `P=$(mktemp -d)`; `TAG=$(openssl rand -hex 2)`; start Talaria in `$PANE` at `$P`. `submit "$PANE" "/rename fixture xf3 $TAG"`. Draw a token (a word and a number 1000 to 9999, as P1-13 does) and `submit "$PANE" 'Count from one to forty, one number per line, then join these two words with a hyphen and reply with only the result: <word> and <number>'`. `herdr pane wait-output "$PANE" --match '(?m)^\s*10\s*$' --source recent-unwrapped --timeout 60000`; `submit "$PANE" '/quit'`; `rec "$PANE" > "$X/q.txt"` (the hint line holds the session id). Poll `fleet --json` every 5 s (at most 180 s) until the session's state is `done` or `idle`. In a new pane at `$P`: `$T attach --no-ensure --profile fixture "fixture xf3 $TAG"`; wait for a line starting `── Resumed:`; `rec` that pane to `$X/a.txt`; `submit` `/quit` there.
- Pass: `q.txt` holds `talaria --resume local-` (the process ended with the turn still running); the session reached `done` or `idle` with no Talaria attached to it; `a.txt` holds `── Resumed: fixture xf3 <tag> ──`, a line equal to `40` and the joined token; the second Talaria's `/quit` ends with `exit=0` (read in the shell).
- Evidence: the hint line, the state poll's last line, the three key lines of `a.txt`.

**XF-4: A worktree session started from the fleet view is reviewed, committed and pushed from the fleet view.** Covers `P3-03` (the `w` dialog) with `P3-07` (the review) and `P3-02`.
- Steps: the setup of `P3-07`'s Common setup: `S=$(mktemp -d)`; `git init -q --bare -b main "$S/origin.git"`; `git clone -q "$S/origin.git" "$S/repo"`; commit `a.txt` in `repo` with `$G -C "$S/repo" commit -q -m one`; `git -C "$S/repo" push -q -u origin main`. Start Talaria in `$PANE` at `$S/repo`. `submit "$PANE" '/fleet'`; `herdr pane send-keys "$PANE" w`; type `Create hello.txt containing hello`, `enter`; `sleep 10`; `vis "$PANE" > "$X/n.txt"`. `BR=talaria/create-hello-txt-containing-hello`; `ID=$(git -C "$S/repo" config --get "branch.$BR.talaria-session")`; wait (at most 180 s) until `$S/repo/.worktrees/talaria-create-hello-txt-containing-hello/hello.txt` exists and `fleet --json | jq -r --arg id "$ID" '.rows[] | select(.session_id==$id) | .state'` prints `done` or `idle`. In the fleet view: `/`, `$ID`, `enter`, `v`; `vis > "$X/r1.txt"`; `c`, type `feat: add hello`, `ctrl+s`; `sleep 3`; `vis > "$X/r2.txt"`; `p`; `sleep 8`; `vis > "$X/r3.txt"`.
- Pass: `n.txt` holds `Launched in fixture: "Create hello.txt containing hello" (worktree talaria/create-hello-txt-containing-hello)` (the branch text follows `P3-03` UAT-3's slug rule: lower case words joined by hyphens; if the real slug differs, the tester reads it from the notice, uses it and says so); `r1.txt` lists `hello.txt`; `r2.txt` holds `Committed ` and `on talaria/create-hello-txt-containing-hello: feat: add hello`; `r3.txt` holds `Pushed talaria/create-hello-txt-containing-hello to origin.`; `git -C "$S/origin.git" branch --list 'talaria/*'` lists the branch; `git -C "$S/repo" status --porcelain` prints nothing and `git -C "$S/repo" log -1 --format=%s main` prints `one` (the main checkout is untouched); `find "$S/repo" -name hello.txt -not -path '*/.git/*'` prints exactly one line, and it lies under `.worktrees/talaria-create-hello-txt-containing-hello/`.
- Evidence: the key lines of `n`, `r1`, `r2`, `r3`, the `git` outputs.

**XF-5: One request that needs you is the same item on every surface.** Covers `P3-01` (list and queue), `P3-02` (view and peek), `P3-03` (launch) and `P3-06` (status line and jump key).
- Steps: `P=$(mktemp -d)`; `mkdir "$P/probe"`; start Talaria in `$PANE` at `$P` (idle composer). In a shell pane at `$P`: `$T $L 'Run this exact shell command: rm -rf ./probe' | tee "$X/l.json"`. Poll every 5 s (at most 90 s) until `fleet` shows a `NEEDS YOU` group; then within 10 s read all four at once: `fleet > "$X/a.txt"`; `fleet --json | jq '.needs_you | length' > "$X/b.txt"`; `vis "$PANE" > "$X/c.txt"`; then `herdr pane send-keys "$PANE" alt+a`; `sleep 1`; `vis "$PANE" > "$X/d.txt"`. Deny: `a`, `sleep 1`, the digit beside `deny`. Within 10 s of the digit: `fleet > "$X/e.txt"`; `fleet --json | jq '.needs_you | length' > "$X/f.txt"`; three `esc` (0.4 s apart); `vis "$PANE" > "$X/g.txt"`; `ls -d "$P/probe"`.
- Pass: `a.txt` holds `NEEDS YOU (1)` and a line holding `fixture` and `approval: rm -rf ./probe`; `b.txt` holds `1`; `c.txt` holds `⚑ 1 needs you`; `d.txt` holds a `NEEDS YOU (` header, the row's detail `approval: rm -rf ./probe` and the peek of the same command; after the digit `e.txt` has no `NEEDS YOU`, `f.txt` holds `0`, `g.txt` has no `⚑` and its status line names the pane's own session; the folder still exists. The four surfaces agree on the count (1, then 0). Any other count (for example `NEEDS YOU (2)`) is a FAIL; when the evidence shows the extra item was a leftover the quiet-fleet check missed, it is the tester's mistake and section 8.1 item 2 applies. The same holds for `⚑ <n> needs you` with n other than 1 in XF-2 and XF-10.
- Evidence: the key lines of `a` to `g` and the `ls` output.

**XF-6: The fleet view over a session that has live subagents.** Covers `P3-02` with `P3-04` (two overlays and one dock) and `P1-06`'s rule that overlays leave nothing behind.
- Steps: start Talaria in a new folder. `submit "$PANE" 'Use the delegate_task tool to start exactly one subagent. Its goal: run the shell command sleep 3 ten times, one command at a time, then reply with the single word finished. Wait for it, then reply with the single word done.'`; poll `vis` every 1 s (at most 90 s) until a line holds `⎇ Subagents ·` and `1 running`. `submit "$PANE" '/fleet'`; `sleep 2`; `vis > "$X/a.txt"`; `herdr pane send-keys "$PANE" esc`; `sleep 2`; `vis > "$X/b.txt"`; `herdr pane send-keys "$PANE" alt+s`; `sleep 1`; `vis > "$X/c.txt"`; `herdr pane send-keys "$PANE" x`; poll `vis` every 5 s (at most 60 s) until the row shows `■`; `herdr pane send-keys "$PANE" esc` twice; `rec > "$X/d.txt"`. If the model does not start a subagent, rerun the row once from the first prompt in a fresh pane.
- Pass: `a.txt` holds ` (here)`; `b.txt` holds `⎇ Subagents ·` and `running` again (the dock came back; the overlay did not clear the tree); `c.txt` holds `Subagents ·` and one `◐` row; `d.txt` holds one line starting `⎇ Subagents · 1 finished · 1 stopped` and no line holding `WORKING (` or `IDLE (` (the fleet overlay left nothing in scrollback).
- Evidence: the key lines of `a` to `d`.

**XF-7: The result of `/bg` arrives while the fleet view is open, and is not lost.** Covers `P3-05` with `P3-02` and `P1-06`.
- Steps: start Talaria in a new folder. Draw a number `<n>` from 1000 to 9999. `submit "$PANE" "/bg Reply with exactly: bg-<n>"`; `sleep 2`; `vis > "$X/a.txt"`; `submit "$PANE" '/fleet'`; read `vis` every 5 s for up to 120 s into `$X/o-<k>.txt` while the overlay is open (the overlay stays open: press nothing); `herdr pane send-keys "$PANE" esc`; `sleep 1`; `vis > "$X/b.txt"`; `rec > "$X/c.txt"`.
- Pass: `a.txt` holds `Background task bg_` and `started.` and a row `◌ bg · Reply with exactly: bg-<n> · running`; every `o-<k>.txt` holds the overlay (` (here)`); after the overlay closes, `grep -c 'bg · Reply with exactly: bg-<n>' "$X/c.txt"` prints 1 (one result cell, not zero and not two), `grep -cw "bg-<n>" "$X/c.txt"` prints 2 or more (the prompt text and the answer), `grep -c 'not part of the conversation' "$X/c.txt"` prints 1, and `b.txt` holds no `◌ bg ·` row; `c.txt` holds no `WORKING (` or `IDLE (`. If the answer had not arrived within 120 s the row is a FAIL only when `/bg` was still `running` at 240 s (a model delay is not asserted).
- Evidence: the key lines of `a`, one `o-<k>`, `b` and `c` and the counts.

**XF-8: Two viewers of one session see the same turn.** Covers `P3-08` (two attached Talarias are normal) with `P3-03` and `P3-01`.
- Steps: `P=$(mktemp -d)`; in a shell pane at `$P`: `$T $L 'Reply with only the word ready.' | tee "$X/l.json"`; `ID=$(jq -r .session_id "$X/l.json")`; poll `fleet --json` every 5 s (at most 120 s) until the session's state is `done` or `idle`. `V1=$(pane_new "$P")`, `V2=$(pane_new "$P")`; in each: `herdr pane run "$V" "$T attach --no-ensure --profile fixture $ID"`; wait for `── Resumed:` in each (at most 15 s). In `$V1`: `submit` the token prompt of P1-13 (`Without using any tools, join these two words with a hyphen and reply with only the result: <word> and <number>`); wait for the joined token in `rec "$V1"` (at most 90 s); read `rec "$V2"` within 20 s. `submit "$V1" '/quit'`; `submit "$V2" '/quit'`.
- Pass: both `rec` reads hold the joined token as a whole word (it cannot be in the prompt) and `── Resumed:`; both panes end with the process exiting 0 (`echo "exit=$?"` typed after each `/quit` shows `exit=0`).
- Evidence: the token, both lines holding it, the two `exit=` lines.

**XF-9: Idle cost with the fleet link running and three sessions in the fleet.** Covers all eight issues and the idle budget of P1-11. The `process-info` path is fact S4.
- Steps: a new folder; the three launches of XF-2 (two on `minimax-m3.1-flash`, one `Reply with only the word gamma.`; no approval) and wait until none is `working`. Start Talaria in `$PANE` at the folder. After 5 s at an empty composer: `PID=$(herdr pane process-info --pane "$PANE" | jq -r '.result.process_info.foreground_processes[] | select(.cmdline | contains("target/release/talaria")) | .pid')`; sample 0: `R0=$(ps -o rss= -p "$PID")`. Then, waiting for each reply: `/fleet` open, `space` on one row, Esc three times; `/agents` open and Esc; `/processes` open and Esc; `submit "$PANE" 'Reply with only the word ready.'`. Wait 5 s. Sample 1: `R1`, `C1` (`ps -o time= -p "$PID"`), `V1=$(vis "$PANE")`. `sleep 120`. Sample 2: `R2`, `C2`, `V2`. Convert `time=` with `awk -F: '{ s = 0; for (i = 1; i <= NF; i++) s = s * 60 + $i; print s }'`.
- Pass: `R0` at most 40960 (KiB, the 40 mebibyte start budget); `R2` at most 102400 (the 100 mebibyte ceiling); `R2 - R1` at most 4096; `C2 - C1` at most 1 second; `V1` and `V2` identical (a clock or counter that changes by itself is quoted and reported on `P1-10`). All four limits are this issue's own: the architecture's budget is 40 MB at start and 100 MB after a 2,000-message session, and this row reads "MB" as mebibytes, as P1-11 decision 7 does; the growth and CPU limits are sanity checks, the same as the earlier gates'. The operator confirms all four on [`PH3`](https://github.com/infiquetra/talaria/issues/215) before the freeze. A `ps` or `jq` that does not work on the machine makes the row `BLOCKED (environment)`.
- Evidence: the three samples and conversions, the output of `diff <(printf '%s\n' "$V1") <(printf '%s\n' "$V2")` (it prints nothing).

**XF-10: A queued prompt survives a background alert and the jump to the peek panel.** Covers `P2-08` with `P3-06` and `P3-02`.
- Steps: `P=$(mktemp -d)`; `mkdir "$P/probe"`; start Talaria in `$PANE` at `$P`. `submit "$PANE" 'Run the shell command sleep 90, then reply with the word first in capital letters.'`; when the working indicator shows, `submit "$PANE" 'Reply with the word omega in capital letters.'`; `vis > "$X/q0.txt"`. In a shell pane at `$P`: `$T $L 'Run this exact shell command: rm -rf ./probe' > "$X/l.json"`. Poll `vis "$PANE"` every 5 s (at most 90 s) until the status line holds `⚑ 1 needs you`; `vis > "$X/q1.txt"`; `herdr pane send-keys "$PANE" alt+a`; `sleep 1`; `herdr pane send-keys "$PANE" a`; `sleep 1`; press the digit beside `deny`; `sleep 5`; three `esc`; `vis > "$X/q2.txt"`. Wait for `FIRST`, then `OMEGA`, and the screen to settle; `rec > "$X/r.txt"`.
- Pass: `q0.txt` and `q1.txt` hold `queued (1)`; `q1.txt` also holds `⚑ 1 needs you`; `q2.txt` holds no `⚑` segment and holds `queued (1)` unless the first turn had already ended (then the user line of omega is already in the transcript); `ls -d "$P/probe"` prints the folder; in `r.txt`, `FIRST` precedes the user line holding `Reply with the word omega` and `OMEGA` follows it, `grep -c 'Reply with the word omega' "$X/r.txt"` prints 1 and `grep -c '⤷ steer' "$X/r.txt"` prints 0 (nothing was sent early or twice).
- Evidence: the key lines of `q0`, `q1`, `q2`, the counts.

**XF-11: Background sessions on another model leave the saved default alone, and the fleet names each model (requirements R4 and R6).** Covers `P3-03`'s `--model` flag with `P2-06`'s saved default and `P3-01`'s model column. Needs `<M2>` (else BLOCKED (environment)).
- Steps: `cp ~/.talaria/models.json "$X/m0.json" 2>/dev/null || echo none > "$X/m0.json"`. Baseline: start `$T --no-ensure --profile fixture` (no model flags) in `$PANE`, note the model in its status line as `<D>`, `/quit`. `P=$(mktemp -d)`; in a shell pane at `$P`: `$T $L --model <M2> 'Reply with only the word ready.' | tee "$X/l.json"`; `ID=$(jq -r .session_id "$X/l.json")`; wait until the session is `done` or `idle` (poll `fleet --json`, at most 120 s); `fleet --json | jq -r --arg id "$ID" '.rows[] | select(.session_id==$id) | tostring' > "$X/row.txt"`; `fleet > "$X/list.txt"`. Start `$T --no-ensure --profile fixture` again, note its model, `/quit`. `cmp "$X/m0.json" ~/.talaria/models.json 2>&1 || true`.
- Pass: `grep -c "<M2>" "$X/row.txt"` prints 1 or more; `grep -c "<M2>" "$X/list.txt"` prints 1 or more; the baseline and last relaunch name the same `<D>`; `cmp` is silent, or the file is absent both before and after.
- Evidence: `<D>` twice, the two `grep` counts, the `cmp` result.

**XF-12: With no gateway, the three phase 3 commands refuse and start nothing (negative).** Covers `P3-01`, `P3-03` and `P3-08` with `P1-12`'s failure texts.
- Steps: in a shell pane: `H=$(mktemp -d)`; `HERMES_HOME="$H" $T fleet list; echo "exit=$?"; ls -A "$H"`; `HERMES_HOME="$H" $T --bg --no-ensure "hi"; echo "exit=$?"; ls -A "$H"`; `HERMES_HOME="$H" $T attach --no-ensure zz-none; echo "exit=$?"; ls -A "$H"`.
- Pass: the first prints `talaria fleet: no Hermes gateway is running for this Hermes home. Start one with "hermes gateway ensure", or start talaria.` and `exit=1`; the second prints a first line starting `talaria: no Hermes gateway is running for this home` and `exit=4`; the third the same first line and `exit=1`; each `ls -A` prints nothing (no gateway or file was created); each command ends within 5 s. The exact `--bg` and `attach` wordings and exit codes are read at the candidate from `P3-03` REQ-3 and `P3-08` REQ-4 and the tester reports any difference as a finding on the owner.
- Evidence: the three outputs and the three empty listings.


### 7.4 The operator's fleet day (rows OP-1 to OP-8, and the two operator source rows)

The roadmap's phase 3 operator check reads: "three sessions on three different providers working on three tasks while the operator codes in a fourth; an approval answered from the peek panel; a worktree session ending in a draft pull request." This section makes each part of that sentence countable, and adds what requirements R5 and R6 ask that the sentence leaves out (reopening after quitting, the attention signals, control of running work). It is the lead's proposal; the operator confirms the numbers on [`PH3`](https://github.com/infiquetra/talaria/issues/215) before the freeze (section 10.2, question 1) and may change them by editing the issue.

**Two operator source rows first (one sitting, on day 1, when no agent runs a live scenario).** `P3-04 UAT-6` (pause new subagent spawns for the whole gateway, see the header change, resume) and `P3-07 UAT-3` (a draft pull request opened from the review screen on a throwaway GitHub repository, then closed). The operator posts one comment per row, its first line `<id>: PASS`, `<id>: FAIL` or `<id>: BLOCKED (<cause>)`, then the counts, the pane texts and the pasted output of the row's live test (section 7.2); an agent copies each into the evidence with the comment's URL. The operator ends `P3-04 UAT-6` with the header reading `spawning on` and posts that read-back, and ends `P3-07 UAT-3` with `gh pr close <n> --delete-branch` and its read-back.

**The day.** Counted by these rules:

- F1. At least 2 calendar days inside a window of 7, at least 2 hours of Talaria-driven coding on each counted day, at least 5 hours in total (the sum of the day log), and one "fleet window" of at least 30 minutes in which OP-1 and OP-2 hold at once.
- F2. Real tasks on throwaway branches of repositories of the operator's choice, with any profile and any models Hermes offers. The evidence names a repository as `<repo-1>`, `<repo-2>`; a profile other than `default` or `fixture` is written `<profile>`; provider ids (for example `ollama-cloud`) are public and stay.
- F3. The binary is the candidate's: the `shasum -a 256` of the file the operator runs equals the one in AUTO-1's evidence. The operator may leave out `--no-ensure`. The operator, not an agent, may restart the gateway or change configuration; each time they record it as a day-log line. The day log is the one inside the OP-8 comment on this issue; agents read only that posted comment, never the operator's private notes.
- F4. Claude Code, or any other tool, is not used for a task the operator meant to do in Talaria. Each time it is, that is a fallback and gets a log line (OP-8).
- F5. The terminals: Herdr on every day; one plain terminal window at least once; `tmux` at least once if the operator uses it (for OP-6).
- F6. The agent rows come first: the day starts only when stage 2 is finished with every agent row PASS on this candidate. A fix that merges during the days makes a new candidate; the rule for what the operator repeats is at the end of this section.

**The parity matrix.** Every line gets one verdict in the final comment: `PAR-<n>: works | used <k> times on <dates> | <what was seen>`, `PAR-<n>: fixed #<issue> | <what was seen after the fix>`, or `PAR-<n>: ACCEPTED GAP #<issue> | <reason in one sentence>`. The lines are the phase 3 rows of the [parity checklist](../04-prior-art.md#claude-code-parity-checklist), split where the roadmap's check names more than one thing, plus the one requirement R6 names that the checklist folds into the agent view:

| Line | Parity row | Built by | Minimum real use |
|---|---|---|---|
| PAR-1 | Agent view, background sessions (`--bg`), attach | P3-01, P3-02, P3-03, P3-08 | 3 background launches (one by `--bg`, one by `n`, one by `w`), the fleet view opened 5 times, `talaria attach` once from a second terminal, `o` once |
| PAR-2 | One "needs you" queue and answering from the peek panel (requirement R6) | P3-01, P3-02 | 3 approvals answered from the peek panel (one `deny`, one allowed), one reply queued, one session opened as the foreground session |
| PAR-3 | Attention: bell, desktop notification, status-line counters | P3-06 | needs-you 2 times, done 2 times, the jump key 2 times; failed when one occurs |
| PAR-4 | Subagents, background subagents, stop all | P3-04 | the dock and the panel used on 2 days; one child tailed, one steered or stopped |
| PAR-5 | Background tasks list, background shells | P3-05 | `/processes` 2 times, one process killed; `/bg` 2 results |
| PAR-6 | Git worktrees (`-w`), pull-request status in the footer | P3-03, P3-07 | 2 worktree sessions; one review, commit and push; one draft pull request and its status label |

If the days give no occasion for a line, the operator provokes one on the last day with the matching scenario of the source issue; a line never gets `works` without having been used. The earlier phases' parity rows are not re-judged here: their verdicts stand on their own candidates, and the regression rows AUTO-2, AUTO-3 and AUTO-8 to AUTO-10 show the same commit still passes them; the operator's days exercise them in passing and any defect found there is filed and counted in OP-8.

**The operator rows.** Each is PASS only when every part is true; the operator, not an agent, decides it. The operator's comment for each row carries the counts; an agent copies it into the evidence (section 5.3 scrubbing) with the comment's URL.

| Row | Pass condition |
|---|---|
| OP-1 | Three providers at once (the first sentence of the check). In the fleet window (F1), three sessions were launched in the background by `--bg`, `n` or `w`, each on a different provider (three provider ids in the comment), each on a different real task; the fleet view listed all three with their models while they ran; each reached `done` or was stopped on purpose with a reason; the window lasted at least 30 minutes. |
| OP-2 | Coding in a fourth session at the same time. During that window the operator worked in a fourth session for at least 30 minutes; its status line named its own session throughout; the operator typed 5 messages of 200 characters or more while background work ran, alerts arrived during at least 2 of them, and none lost, doubled or reordered a character; no alert, overlay or notice replaced or cleared the composer's text. |
| OP-3 | An approval answered from the peek panel (the second sentence of the check). At least 3 real approval requests from background sessions were answered from the peek panel (reached by `alt+a` or `/fleet`, then `space`, `a` and the digit), at least one `deny` and at least one allowed; each answer took effect (a denied command did not run, an allowed one ran once); none went to the wrong session; none timed out unseen (a request that waited the full 300 s counts against the row unless the operator says they were away and the comment says when). |
| OP-4 | A worktree session ends in a draft pull request (the third sentence). A session was started with `w` or `--bg --worktree` on a real repository whose `origin` is on GitHub; its edits appeared only under `.worktrees/` (`git status --porcelain` in the main checkout empty before and after, both read-backs in the comment); the operator reviewed it with `v`, committed, pushed and opened a draft pull request with `shift+p`; the fleet row showed `#<n> draft` and later a check mark or a cross that matched the real checks; the pull request was then closed with its branch deleted, read-back in the comment. |
| OP-5 | Attention is right. Every needs-you event in the fleet window rang the bell once and raised one desktop notification (unless the operator's configuration turned one off, and the comment says so); no done or failed alert came for a session the operator never watched; the status-line counters matched `talaria fleet list` at 3 spot checks; any failed session that occurred raised its alert (`failed: none occurred` is allowed). |
| OP-6 | Control of running work in real use. At least one subagent tailed and one steered or stopped; one background process watched in `/processes` and killed; one session stopped from the fleet view; two `/bg` results read; `o` used inside Herdr once and, if the operator uses `tmux`, inside it once; `talaria attach` from a second terminal once; each worked as its source issue says. |
| OP-7 | Truthful and durable (requirement R5). Talaria was quit with at least one session working and one waiting for an answer (under 240 s), then reopened: the waiting item showed with the start-up notice, the finished sessions were listed, and `talaria attach` showed a complete transcript; at 10 spot checks across the days every fleet row's state matched what the session was really doing; zero rows stayed `working` more than 10 s after their turn ended; no repeated, ghost or garbled row after at least 3 terminal resizes in Herdr and once in a plain terminal; and the Talaria process's resident memory at the end of the longest session, with at least 3 sessions in the fleet, was at most 102400 KiB (`ps -o rss= -p <pid>`), with that session's message count in the comment. |
| OP-8 | The day log, the matrix and the verdict are posted. One comment holds the day log (dates, hours, terminals, sessions, gateway restarts and configuration changes), the binary's checksum (equal to AUTO-1's), all 6 `PAR-` lines, the counts of OP-5 and OP-7, every fallback to another tool as one line (date, task in a few words, why, issue number; zero is allowed), the defects found D with their fate (each fixed, F, or an `ACCEPTED GAP #<issue>`, G; D = F + G; none open without one of the two; a gap is allowed only for a parity line and can never turn a FAIL of any AUTO, source or XF row into a pass), the list of `ACCEPTED GAP #<issue>` lines or `GAPS ACCEPTED: none`, and a line reading exactly `PHASE 3 OPERATOR CHECK: PASSED (candidate <7 hex>)` (its 7 characters equal the candidate's first 7). |

**After a fix.** When a defect found in the days is fixed and merged, the lead freezes a new candidate (section 3) and every agent row is rerun on it. The operator then uses the new binary for at least 1 working day and 2 hours, rechecks the PAR lines the fix's issue names and OP-7, and the lead lists in a comment the lines whose earlier `works` the fix could not have touched; the operator confirms that list in writing, and for those lines the earlier days still count. The verdict names the final candidate. Operator rows go back to NOT RUN until the verdict of the final candidate is posted.

## 8. Run order, failures and a new candidate

1. Stage 0: build the smoke extension and merge it (section 9). Freeze (section 3).
2. Stage 1 (agent tester, one pane at the worktree root): AUTO-1 to AUTO-7, then AUTO-8, AUTO-9, AUTO-10, AUTO-11, AUTO-12 back to back.
3. Stage 2 (agent tester): the 43 source rows the operator does not run, in the order of section 7.2, then XF-1 to XF-12 in order, one live scenario at a time (one shared gateway and free model), the quiet-fleet rule of section 3 between rows.
4. Stage 3 (operator): the two operator source rows in one sitting on day 1, then the days (section 7.4) and the verdict.
5. Stage 4: write the record and evidence (the first commit is the empty record, section 4), run the checker, open the pull request (documentation only), have it reviewed, merge. The pull request body carries the lines of the issue's REQ-16.

Three identical provider errors in a row (same HTTP status, same message) stop the run: report them on the issue and ask the operator, never retry in a loop. A row may be run up to three times on one candidate; every run goes in its evidence section with why each earlier failure was not a Talaria defect (a provider rate limit, a model that did not follow a plain prompt).

A FAIL stays FAIL until a new candidate passes it. The tester comments the pane text on the issue; the lead files a defect issue through Mission Control for the owner of the source issue (the operator files those found in the days, or asks the lead to); the fix merges; the lead freezes a new candidate (section 3) and every row is run again; the record is overwritten (git history keeps the old one). A defect is never fixed inside this issue, and no scenario's expected result is edited to make it pass; if a source issue's expected text is wrong, its owner corrects the source issue and this unit's Appendix A is corrected (a correction of the specification is the one change allowed after the freeze).

### 8.1 Rulings that settle questions a builder asked

1. **Skips.** A suite step that skipped, a result line `PASSED WITH SKIPS`, or `FAIL <step> (skipped: ...; TALARIA_LIVE_REQUIRE=1)` makes its row `BLOCKED (environment: <step> skipped)`. A live test that printed `SKIPPED (<reason>)` is the same. A FAIL whose reason is Herdr rejecting a key name (fact S1 or S2) is `BLOCKED (environment: Herdr rejected <key>)`. Any other FAIL is FAIL. The one allowed skip line is `live_subagents`' pause test in AUTO-5 (it needs `TALARIA_LIVE_PAUSE=1`; the operator runs it with `P3-04` UAT-6, section 7.2), and `P3-04` AC-6 expects it.
2. **Reruns.** A row may run up to three times on one candidate: a BLOCKED row once its cause is fixed; a FAIL only when the tester shows it was not a Talaria defect (a provider error, a model that ignored a plain prompt or did not start a subagent or a tool, or a lagging read source) and the lead first writes `rerun approved: <id>: <cause>` on the issue; a Talaria defect is final for the candidate. The last run is the row's. A row that fails because a leftover session was not cleared (section 3), or because it ran in a pane narrower than section 3 allows, is a mistake of the tester: clear or widen, then rerun, and say so. A `fleet-quiet` FAIL of AUTO-8 to AUTO-12 that a later quiet-fleet check clears is such a mistake.
3. **No waiver.** BLOCKED and NOT RUN never count. An `ACCEPTED GAP` exists only for the six parity lines and never for an AUTO, source or XF row.
4. **Changing the row list before the freeze.** A scenario, step or row the operator drops, or one the lead adds (for example the scenarios of [`P3-09`](https://github.com/infiquetra/talaria/issues/247) when decision 8 of [`P0-01`](https://github.com/infiquetra/talaria/issues/194) made it part of phase 3 and it is merged), is removed or added before the freeze by editing, in one commit with the reason in its message: this issue's counts (Objective, Intent decision 6, REQ-7, REQ-9, REQ-11, REQ-15, UAT-1, AC-5, AC-7, AC-10, AC-13, AC-15), the `IDS=` line of the checker and its stub, the skeleton of section 4, the text `77` in section 6, and the checker tests' expectation `77 of 77`. If the operator drops a whole group (for example XF-2 and XF-11 for lack of a second Ollama Cloud model), the rows are removed, not marked.
5. **After the freeze** only `docs/acceptance/v0.7.0/`, `docs/engineering-journal/LEARNINGS.md` and this unit's specification (corrections only) may change; any other change makes a new candidate and reruns every agent row.
6. **Shape.** The record shape follows the earlier gate (section 10.1); the lead posts it on [`PH3`](https://github.com/infiquetra/talaria/issues/215) before stage 0.
7. **Finished background sessions stay listed.** By fact 2.2 a finished `--bg` session stays in `talaria fleet list`, in the fleet overlay and open to `attach` by its 8-hex prefix. No step tolerates its absence: `fleet-list`, `attach`, `open-in-pane`, XF-3, XF-8 and XF-11 fail when it is absent, with the reason naming it. The stage 0 check (9.0) confirms the listing on the live gateway, and that the prefix opens the session (no A3 ambiguity, `P3-08` REQ-6) before any screen is recorded; if either does not hold, stop in stage 0 and report it on `P3-01` and `P3-08` before the freeze.
8. **Conditional children.** If decision 8 of [`P0-01`](https://github.com/infiquetra/talaria/issues/194) is reading (b), [`P3-09`](https://github.com/infiquetra/talaria/issues/247) becomes a start gate of this issue and its scenarios are added as rows by item 4 before the freeze; under reading (a) it is not a gate. If decision 5 of [`P0-01`](https://github.com/infiquetra/talaria/issues/194) is yes, [`P6-01`](https://github.com/infiquetra/talaria/issues/274) moves into phase 3, becomes a start gate, and a remote scenario is added the same way.


## 9. The smoke suite extension

### 9.0 The stage 0 live check (before any screen is recorded)

The test author's first act, in a throwaway Herdr tab at least 100 columns by 24 rows with the live gateway and profile `fixture`, after reading `P1-13`'s journal entry on the read sources (fact H14): start Talaria and send each of `alt+a`, `alt+s`, `shift+p`, `shift+x` and `ctrl+s` once with `herdr pane send-keys` (pressing Esc after each; nothing is pending, so none changes anything), and note which Herdr refused; launch one `--bg` session with `--no-ensure`, wait until it is `done` or `idle`, and confirm that `talaria fleet list --profile fixture --json` still lists it after 5 minutes, that `/fleet` shows its row, and that `talaria attach --no-ensure --profile fixture <first 8 hex>` opens it (no A3 ambiguity). The results go in the pull request (a `Stage 0 check` section) and in `LEARNINGS.md`. A refused key name, a missing row, an ambiguous prefix, or a journal entry saying a read source lags for Talaria stops stage 0: report it on this issue and on the owner, and wait for the operator's choice before recording anything. The key names are constants in `steps3.rs` (`KEY_JUMP = "alt+a"` and so on), so a Herdr name for the same keystroke that `herdr pane send-keys --help` lists is a one-line change before the freeze.

### 9.1 Why a fourth suite, and what it is not

`smoke run` of P1-13 checks five behaviours of the first usable binary in under five minutes; `PH2A-ACC` added a second suite (`--suite phase2a`, 21 steps) and `PH2-ACC` a third (`--suite phase2b`, 15 steps). Phase 3 adds about a dozen things a person sees: a background launch, a worktree launch, the fleet list and its refusals, `talaria attach`, three local commands, the fleet view, "open in a new pane", and an approval that arrives while Talaria was closed and is answered from the peek panel. Putting them into an earlier run would lengthen it past its budget, would change code that earlier gates ran as regression rows, and would let a skip in a new step hide behind old steps. So the extension is a fourth suite, selected with `--suite phase3`, with its own deadline (1200 s, section 9.6), its own result line and its own tests; the earlier suites, their tests and their README steps stay as they are (AC-2).

It is the lead's proposal, not a roadmap requirement: the roadmap asks only for a live check per unit and for "the operator check". The reason to build it is the one P1-13 gives: a repeatable unattended run, in a real pane, that fails with the screen text when something regressed, which phases 4 to 6 (configuration, workflows) rerun on every candidate. The cost is a test author's and a developer's work on a tool, not on the product. If the operator declines (section 10.2, question 3), the unit has 75 rows (AUTO-11 and AUTO-12 dropped), no fourth ignored test and no section 9; section 8.1 item 4 gives the procedure for the counts, and the lead also edits, in the same commit, REQ-1 to REQ-5 and REQ-10, UAT-2, UAT-3, UAT-4, AC-2, AC-3 and AC-11 of the issue and the ignored-test list of AUTO-7.

What it is not: it is not a replacement for the 45 scenarios or the 12 cross-feature rows (it checks each feature's happy path and one refusal, not its edge cases), it is not a performance test (P1-11's harness owns budgets), it does not run in continuous integration (it needs a live gateway, a model and Herdr; the operator and this unit run it by hand), and it does not cover three providers, the review screen (`v`), subagents, background processes with output, or the pause switch (each is covered by its source scenarios and the XF rows; section 9.12). It needs no `tmux`.

### 9.2 Shape of the code

New files in `tools/smoke` (P1-13's crate; its layout is in P1-13 spec section 3; the earlier suites' files are described in `PH2A-ACC` spec 9.2 and `PH2-ACC` spec 9.2):

```text
src/steps3.rs       run_smoke_3 and the 12 steps
src/screen3.rs      pure helpers over screen text and command output (below)
src/wording3.rs     every Talaria screen text a phase 3 step relies on, as consts, each with a source comment (9.5)
tests/              screen3.rs, steps3.rs, typed_input3.rs, cleanup3.rs, cli3.rs, fixtures_public_safe3.rs,
                    live_smoke3.rs, fixtures/screens/phase3/*.txt
```

Minimal edits to existing files (named in the pull request): `src/suite.rs` (`Phase3` joins the enum and `FromStr`; read `unknown_suite_exits_2_with_the_parse_text` in the earlier `cli*.rs` files first: if one pins the parse text exactly, leave the text as it is, because AC-2 forbids editing that test, and say so in the pull request; otherwise the text lists all four suites; `--help` names all four either way), `src/config.rs` (the deadline default for `phase3` is 1200 s; no new option), `src/main.rs` (dispatch on the suite; the `tmux` preflight of `phase2a` is not run for `phase3`; `phase3` with a `--profile` other than `fixture`, a `--model` other than `minimax-m3.1-flash` or a `--provider` other than `ollama-cloud` fails preflight with one plain sentence, `phase3 needs the default profile, provider and model (fixture, ollama-cloud, minimax-m3.1-flash): its expected texts name them`, no `SMOKE_RESULT` line and exit 2; the preflight's 600-character bound of `P1-13` (its spec 4.1 item 6) is applied to every phase 3 shell line of 9.3, computed with the absolute binary path and the scratch root path the run will create, and a longer line fails preflight with `a phase 3 shell line is longer than 600 characters; use a shorter temporary directory or binary path` and exit 2), `src/lib.rs`, `README.md`, `Cargo.lock` (no new crate is expected; the suite reads two JSON documents with the crate's existing JSON dependency), `tests/support/` (additions only: a `Scratch3` helper that builds the scratch repositories, screen-script entries, and two new rule fields documented in `script.rs`'s doc comment: `write_in_scratch` `{path, text}` writes a file under the scratch root, and `remove_in_scratch` deletes a path under it, the same way `PH2A-ACC`'s `write_in_cwd` and `P1-13`'s `remove_in_cwd` work under the tab's folder; a scripted shell line is a `run` rule whose `matches` names the line's output file, which writes that file with `write_in_scratch` and appends its marker with `append: "SMOKE_EXIT:<n>\n"`). The shared steps `startup`, `plain_answer` and `quit` are P1-13's, already `pub(crate)` with the environment prefix `PH2A-ACC` added; `phase3` calls them with an empty prefix. The suite never lists panes: the pane that Talaria opens for `o` is not tracked (9.3.10).

`Token` is P1-13's (`Token { word, number }`, `Token::new(rng, used)`, `join()`, `prompt()`) and `lines_after_last` is `PH2-ACC`'s `screen2b.rs` (re-exported, not copied).

The stub commit adds `Suite::Phase3`, `run_smoke_3` and the `screen3.rs` helpers with bodies that panic `not implemented`, and leaves `FromStr`, the help text, the deadline default and the preflight unchanged; so on the red run the `cli3` tests fail by the old behaviour (`phase3` refused as an unknown suite), every other new test fails by the panic, and a panic of a stub counts as failing for the reason the test names. The expected passes are exactly `cli3::default_suite_is_phase1`, `fixtures_public_safe3::phase3_fixtures_hold_no_paths_or_names`, and `cli3::unknown_suite_exits_2_with_the_parse_text` when an earlier test pins the parse text (above); the red-run section lists them.

Public items, with the signatures the tests use (keep the names and shapes unless the code forces a change, and report it):

```rust
// suite.rs
pub enum Suite { Phase1, Phase2a, Phase2b, Phase3 }          // FromStr: "phase1" | "phase2a" | "phase2b" | "phase3"
// steps3.rs
pub fn run_smoke_3(term: &dyn Terminal, clock: &dyn Clock, cfg: &SmokeConfig, stop: &AtomicBool) -> SmokeReport;
// screen3.rs (pure; each takes the normalised lines of P1-13 spec 5.3, box characters removed and trimmed)
pub fn fleet_headers(text: &str) -> Vec<(String, u32)>;                // "NEEDS YOU (1)" gives ("NEEDS YOU", 1)
pub fn has_here_row(visible: &str) -> bool;                            // a row line containing " (here)" (a title suffix; columns may follow it)
pub fn needs_you_segment(visible: &str) -> Option<u32>;                // the n of "⚑ n needs you" on the status line
pub fn launch_result(out: &str) -> Option<LaunchResult>;               // { status, session_id }; Some only when out is exactly one non-blank JSON line
pub fn fleet_row_state(json: &str, session_id: &str) -> Option<String>;// the state of the row with that session_id
pub fn needs_you_ids(json: &str) -> Vec<String>;                       // the session_id of each item of the needs_you array
pub fn deny_digit(visible: &str) -> Option<char>;                      // the digit n of a line "<n>. deny"
pub fn opened_notice(text: &str) -> Option<String>;                    // the title in `Opened "<title>" in a new Herdr pane.` (not the tmux text)
pub fn worktree_line(out: &str, name: &str) -> Option<String>;         // the commit in "Worktree: branch talaria/<name> from origin/main at <commit>"
pub fn launched_id(out: &str) -> Option<String>;                       // the id in "Launched session <id> in profile <p>."
```

### 9.3 Flow, step by step

All waits are poll loops at 250 ms using `Clock`; every loop also checks the stop flag and the run deadline. Each step uses the source named in the table below: a read of the screen as it is now uses `visible`; a search for text that may have scrolled away uses `recent-unwrapped --lines 3000` (`RECENT_LINES`). Herdr's own `recent` source is never used. The rule: the status line, overlays, the composer and anything that exists only on the live screen are read from `visible`; a notice that Talaria commits to the transcript, and the output of a shell command, are searched in `recent-unwrapped` or in the scratch file the command wrote. Which source lags for a full-screen program is P1-13's H14 item (section 2.1): this unit follows its recorded result. "Normalised lines" and "settled" are P1-13's (its sections 5.3 and 5.6). `submit` is P1-13's (its 5.1), and its fallback Enter applies, as in P1-13, only to prompts typed into Talaria and to `/quit`: never to the slash lines `/tasks`, `/agents frobnicate`, `/processes`, `/bg` and `/fleet`, and never while an overlay is open (an extra Enter in the fleet overlay would open the selected session); `typed` is `send_text` without Enter; `key` is `send_keys`, and after `esc` the suite waits 400 ms. A shell step runs one line with `run` and waits for a new `SMOKE_EXIT:<n>` marker (the `launch_line` helper adds `; echo SMOKE_EXIT:$?`), then reads the scratch file the command wrote; the scratch files are in the suite's own folder, the one place the suite reads anything but screens. A step that fails prints the reason of section 9.4 and the capture. Only `needs-you` may SKIP and only `needs-you` gets one repeat: when the first attempt shows no approval request, the whole attempt starts again from its launch line, after `fleet list` shows no `WORKING` group; a second miss either skips (9.4) or fails. Every other step fails at once. Because the suite reads `fleet list --json`, it matches the document's fields by name (`rows`, `session_id`, `state`, `needs_you`; fact S5) and never by position.

| Step | Decides on `visible` (the screen now) | Decides on a file or `recent-unwrapped` |
|---|---|---|
| fleet-quiet, bg-launch, worktree-launch, fleet-list | nothing | the scratch files and the exit marker |
| attach | the `── Resumed:` line and the prompt text | the exit marker and, for the refusal, `recent-unwrapped` |
| startup, plain-answer, quit | as P1-13 | as P1-13 |
| local-commands, fleet-view | every check | the usage line and the unknown-option notice |
| open-in-pane | the notice, polled on both sources every 250 ms | the notice (if the overlay closed, `P3-08` REQ-17 puts it on the main screen) |
| needs-you | the status line, the notice, the overlay and its peek | the scratch files and the exit marker |

**9.3.0 Setup (before the tab).** Run id as P1-13 7.1. The scratch root is a new folder under the system temporary directory, mode 0700, `talaria-smoke-<run id>`, holding: `origin.git` (`git init --bare -b main`); `a` (a clone of it with one commit `init`, pushed); `other` (a second clone that commits `two` and pushes, so `origin/main` is one commit ahead of `a`'s `main`); `probe/talaria-smoke-probe` (an empty folder); `empty-home` (an empty folder). Every commit uses `git -c user.name=smoke -c user.email=smoke@example.invalid`. The session folder is `a`; the tab's working folder is `a`. The scratch root holds no space and no single quote (the preflight checks it). Token A is drawn for `plain-answer`; the name of the worktree is `smoke-<run id>`. The tab, its root pane (`first`) and the `TabGuard` are made as in P1-13 4.1. `<T>` is the absolute path of the binary under test; `<p>`, `<m>` and `<v>` are the profile, model and provider (preflight allows only `fixture`, `minimax-m3.1-flash` and `ollama-cloud`).

**9.3.1 `fleet-quiet`** (the quiet-fleet rule of section 3). In `first` (a shell in `a`): `printf '%s %s\n' "$(tput cols)" "$(tput lines)" > '<scratch>/size.out'; '<T>' fleet list --profile <p> > '<scratch>/q.out' 2>&1; echo SMOKE_EXIT:$?`. Within 30 s the marker reads `SMOKE_EXIT:0`; `size.out` reads at least 100 columns and 24 rows (else `fleet_quiet_narrow_pane_fails`); `q.out` holds no header `NEEDS YOU` or `WORKING` (`fleet_headers`). The step checks once and never waits: waiting for a busy fleet is the tester's quiet-fleet check (section 3). The step exists so that a leftover session never makes a later step fail for the wrong reason.

**9.3.2 `bg-launch`** (P3-03 UAT-1 and the first line of UAT-6). `'<T>' --bg --no-ensure --output-format json --profile <p> --model <m> --provider <v> "Reply with only the word ok." > '<scratch>/bg1.out' 2> '<scratch>/bg1.err'; echo SMOKE_EXIT:$?`. Within 15 s the marker reads `SMOKE_EXIT:0` and `launch_result(bg1.out)` is `launched` with a `session_id` (kept as `ID1`). Then `'<T>' --bg --no-ensure -p "x" > '<scratch>/bg2.out' 2> '<scratch>/bg2.err'; echo SMOKE_EXIT:$?` (the flag check comes before any connection, `P3-03` REQ-5, so `--no-ensure` changes nothing but keeps the rule of 9.7 without an exception): within 15 s `SMOKE_EXIT:2`, `bg2.out` empty, `bg2.err` holding `talaria --bg cannot be combined with -p.`.

**9.3.3 `worktree-launch`** (P3-03 UAT-2, first half). `'<T>' --bg --worktree --name smoke-<run id> --no-ensure --profile <p> --model <m> --provider <v> "Reply with only the word ok." > '<scratch>/wt.out' 2>&1; echo SMOKE_EXIT:$?`. Within 90 s (the launch fetches `origin`) the marker reads `SMOKE_EXIT:0`; `wt.out` holds `worktree_line` for `smoke-<run id>` and a `launched_id` (the printed id, which `P3-03` REQ-4 makes the stored id; it is checked as text only). Then `git -C '<scratch>/a' config --get branch.talaria/smoke-<run id>.talaria-session > '<scratch>/wt.id' 2>&1; git -C '<scratch>/a/.worktrees/talaria-smoke-<run id>' log -1 --format=%s > '<scratch>/wt.log' 2>&1; git -C '<scratch>/a' status --porcelain > '<scratch>/wt.st' 2>&1; echo SMOKE_EXIT:$?` (the marker is `git status`'s and must read 0; a missing config key only leaves `wt.id` empty). `ID2` is `wt.id` trimmed: the live `session_id` that `P3-03` REQ-17 records and that `fleet list --json` rows are keyed by (`P3-03` UAT-2 reads it the same way). When `wt.id` is empty (REQ-17 makes a failed write a warning), `ID2` is the `launched_id` value (for a local session the two ids are equal, `P1-12` fact H14) and the report prints `NOTE worktree-launch: no talaria-session record; using the printed id`. Also: the folder exists, `wt.log` trimmed is `two` (the tree started from the fetched `origin`, not from the stale `main`) and `wt.st` is empty (the main checkout is untouched and `.worktrees/` is excluded without editing `.gitignore`).

**9.3.4 `fleet-list`** (P3-01 UAT-1, UAT-5, UAT-6). Every 5 s for up to 120 s: `'<T>' fleet list --profile <p> --json > '<scratch>/f.json' 2>&1; echo SMOKE_EXIT:$?` (marker `SMOKE_EXIT:0`) until `fleet_row_state(f.json, ID1)` is `done` or `idle`; `ID2` must also be a row of the last document (any state). Then `'<T>' fleet list --profile nosuch > '<scratch>/f2.out' 2>&1; echo SMOKE_EXIT:$?`: `SMOKE_EXIT:2` and the sentence `profile "nosuch" is not served by this gateway`. Then `HERMES_HOME='<scratch>/empty-home' '<T>' fleet list > '<scratch>/f3.out' 2>&1; echo SMOKE_EXIT:$?`: within 10 s `SMOKE_EXIT:1`, the sentence `no Hermes gateway is running for this Hermes home` and an `empty-home` that is still empty.

**9.3.5 `attach`** (P3-08 UAT-1, first half, and the first line of UAT-2). `<id8>` is the first 8 characters after `local-` in `ID1`. `'<T>' attach --no-ensure --profile <p> <id8>; echo SMOKE_EXIT:$?` (not redirected: `attach` needs a terminal). Within 15 s `visible` holds a line starting `── Resumed:` and a line holding `Reply with only the word ok`; when instead the marker reads `SMOKE_EXIT:2` with `P3-08`'s A3 text (several sessions match), the step fails with `attach_ambiguous_fails`. `typed("/quit")`, `key enter`; within 15 s `SMOKE_EXIT:0`. Then `'<T>' attach --no-ensure --profile <p> zz-none-<run id>; echo SMOKE_EXIT:$?`: within 20 s `SMOKE_EXIT:1` and, in the lines after the last echo of that command in `recent-unwrapped`, `talaria: no session matches "zz-none-<run id>"`.

**9.3.6 `startup`** (P1-13 4.2 unchanged, in folder `a`, no `EDITOR`). Nothing is typed until the stable screen.

**9.3.7 `plain-answer`** (P1-13 4.3 unchanged). Token A. It gives the foreground session a prompt, so `/quit` prints a resume hint.

**9.3.8 `local-commands`** (P3-04 UAT-5 and P3-05 UAT-4, no gateway work). `submit("/tasks")`; within 5 s `visible` holds `No subagents in this session yet.`; `key esc`; within 3 s that line is gone. `submit("/agents frobnicate")`; within 5 s `recent-unwrapped` holds `Unknown /agents option: frobnicate. Use /agents, /agents pause or /agents resume.`. `submit("/processes")`; within 5 s `visible` holds `No background processes in this session.`; `key esc`; within 3 s it is gone. `submit("/bg")`; within 5 s `recent-unwrapped` holds a line starting `Usage: /bg <prompt>` and, after the last echo of `/bg`, no line starting `Background task`.

**9.3.9 `fleet-view`** (P3-02 UAT-1, first part, and UAT-6). `submit("/fleet")`; within 5 s `has_here_row(visible)` is true. `typed("/")`, `typed("zzz-no-match")`, `key enter`; within 5 s `visible` holds `No sessions match "zzz-no-match".`. `key ctrl+c`; within 5 s `has_here_row(visible)` is false and the status line names `minimax-m3.1-flash` again (the composer is intact).

**9.3.10 `open-in-pane`** (P3-08 UAT-3, the notice only). `submit("/fleet")`; `typed("/")`, `typed(ID1)`, `key enter`; then `typed("o")`. Within 10 s `opened_notice` of `visible` or of `recent-unwrapped` (both polled every 250 ms; the notice may be on the overlay or, if it closed, on the main screen, `P3-08` REQ-17) is `Some(<title>)` (the text of P3-08 outcome T2: `Opened "<title>" in a new Herdr pane.`). The text `Not inside Herdr or tmux` (T3) is the reason `open_in_pane_not_in_herdr_fails`. `key ctrl+c` closes the overlay. The new pane is not read: Talaria made it, the suite does not know its id and does not list panes, and it dies with the tab; `P3-08` UAT-3 and XF-3 read a new pane.

**9.3.11 `quit`** (P1-13 4.6) with token A in place of its A and B: `committed output vanished on exit (token A missing)` when A is absent from the first pane's recent text.

**9.3.12 `needs-you`** (P3-01 UAT-3, P3-06 UAT-5 and UAT-2, P3-02 UAT-3). In `first`: `( cd '<scratch>/probe' && '<T>' --bg --no-ensure --output-format json --profile <p> --model <m> --provider <v> "Run this exact shell command: rm -rf ./talaria-smoke-probe" ) > '<scratch>/ny.out' 2> '<scratch>/ny.err'; echo SMOKE_EXIT:$?` (the subshell keeps the shell in `a` and makes the marker the launch's own status); within 15 s the marker reads `SMOKE_EXIT:0` and `launch_result(ny.out)` gives `ID3`, else `needs_you_launch_fails`. Every 5 s for up to 120 s: `'<T>' fleet list --profile <p> --json > '<scratch>/f4.json' 2>&1; echo SMOKE_EXIT:$?` until `ID3` is in `needs_you_ids(f4.json)`; the first time it is seen starts a stopwatch. If it never is, the attempt repeats once (9.3 intro; before the repeat the step polls `fleet list` every 5 s for at most 30 s until it shows no `WORKING` group, and repeats anyway after 30 s); after a second miss the step skips with the SKIP text of 9.4 when `fleet_row_state(ID3)` is `done` and `probe/talaria-smoke-probe` no longer exists (the command ran without asking: approvals are not `manual` on the profile), and fails with `needs_you_never_listed_fails` otherwise. When the request is more than 150 s old at this point the step fails with `needs_you_too_old_fails` (Hermes denies at 300 s). Start Talaria again as in `startup` (the shared helper, folder `a`). Within 20 s `visible` or `recent-unwrapped` holds `1 background session needs you; press alt+a to see it.` and the status line holds `needs_you_segment == Some(1)` (requirement R5: the request waited while Talaria was closed). `key alt+a` (if Herdr rejects the name, the step fails with the Herdr error as its reason and the operator is told in the run's report); within 5 s `visible` holds a `NEEDS YOU` header and `rm -rf ./talaria-smoke-probe`. `typed("a")`; within 5 s `deny_digit(visible)` is `Some(d)`; `typed(d)` (only the digit read from the screen is ever typed). Within 10 s `visible` no longer holds `NEEDS YOU`, and `ID3` is no longer in `needs_you_ids` of a fresh `fleet list --json`. `key esc` up to three times (400 ms apart) until `has_here_row` is false; the status line has no `⚑`; `probe/talaria-smoke-probe` still exists. `typed("/quit")`, `key enter`; within 15 s `SMOKE_EXIT:0`.

### 9.4 Exact reason texts

Each reason is printed as `FAIL <step> (<reason>)` followed by the pane captures (P1-13 4.8); `{...}` is filled in at run time. The reason for the skip is printed as `SKIP needs-you (<reason>)`. The first column is the step, the second the test that makes it happen.

| Step | Test | Reason text |
|---|---|---|
| fleet-quiet | `fleet_quiet_exit_fails` | `talaria fleet list exited with code {n} (is the Hermes gateway running for profile {p}?)` |
| fleet-quiet | `fleet_quiet_narrow_pane_fails` | `the smoke tab is {c} columns by {r} rows; phase 3 needs at least 100 by 24` |
| fleet-quiet | `fleet_quiet_busy_fleet_fails` | `the fleet is not quiet ({headers}); answer or stop those sessions and run again`, where `{headers}` is only the offending headers as printed, `NEEDS YOU (n)` then `WORKING (n)`, each present only when its count is above 0, joined by `, ` (other groups such as `IDLE` and `DONE` never appear in it) |
| bg-launch | `bg_launch_exit_fails` | `talaria --bg ended with exit code {n}, expected 0` |
| bg-launch | `bg_launch_output_fails` | `the output was not exactly one line with "status":"launched" and a session_id` |
| bg-launch | `bg_launch_slow_fails` | `talaria --bg took {n} s to return, over the 15 s limit` |
| bg-launch | `bg_launch_refusal_exit_fails` | `talaria --bg -p ended with exit code {n}, expected 2` |
| bg-launch | `bg_launch_refusal_text_fails` | `talaria --bg -p printed on standard output or did not say "talaria --bg cannot be combined with -p." on standard error` |
| worktree-launch | `worktree_launch_exit_fails` | `talaria --bg --worktree ended with exit code {n}, expected 0` |
| worktree-launch | `worktree_launch_text_fails` | `the output lacked "Worktree: branch talaria/smoke-{run id} from origin/main at" or "Launched session"` |
| worktree-launch | `worktree_launch_tree_missing_fails` | `the folder .worktrees/talaria-smoke-{run id} was not made` |
| worktree-launch | `worktree_launch_not_fresh_fails` | `the worktree started at "{subject}", not at the commit "two" that origin had and the checkout did not` |
| worktree-launch | `worktree_launch_checkout_dirty_fails` | `the main checkout is no longer clean: {first status line}` |
| fleet-list | `fleet_list_exit_fails` | `talaria fleet list --json ended with exit code {n}, expected 0` |
| fleet-list | `fleet_list_never_done_fails` | `session {first 8 characters of the id} was not listed as done or idle within 120 s (last state: {state or "absent"})` |
| fleet-list | `fleet_list_worktree_session_missing_fails` | `the worktree session {first 8 characters of the id} is not in the fleet list` |
| fleet-list | `fleet_list_unserved_profile_fails` | `--profile nosuch did not exit 2 with the sentence "profile \"nosuch\" is not served by this gateway"` |
| fleet-list | `fleet_list_no_gateway_fails` | `with an empty Hermes home, talaria fleet list did not exit 1 with "no Hermes gateway is running for this Hermes home"` |
| fleet-list | `fleet_list_no_gateway_started_fails` | `talaria fleet list wrote into the empty Hermes home (it must start nothing)` |
| attach | `attach_no_resumed_fails` | `talaria attach {first 8 hex} did not show "── Resumed:" and the session's first prompt within 15 s` |
| attach | `attach_ambiguous_fails` | `talaria attach {first 8 hex} matched several sessions (exit 2): {first line of the A3 text}` |
| attach | `attach_exit_fails` | `/quit in the attached session did not end talaria with exit code 0 within 15 s` |
| attach | `attach_no_match_fails` | `talaria attach zz-none-{run id} did not exit 1 with "talaria: no session matches"` |
| local-commands | `local_commands_tasks_fails` | `/tasks did not say "No subagents in this session yet." within 5 s` |
| local-commands | `local_commands_agents_option_fails` | `/agents frobnicate did not print the unknown-option sentence within 5 s` |
| local-commands | `local_commands_processes_fails` | `/processes did not say "No background processes in this session." within 5 s` |
| local-commands | `local_commands_bg_usage_fails` | `/bg alone did not print its usage line, or started a task` |
| fleet-view | `fleet_view_open_fails` | `/fleet did not open an overlay with this session's row marked "(here)" within 5 s` |
| fleet-view | `fleet_view_no_match_fails` | `filtering for zzz-no-match did not say "No sessions match" within 5 s` |
| fleet-view | `fleet_view_close_fails` | `Ctrl+C did not close the overlay within 5 s` |
| open-in-pane | `open_in_pane_notice_fails` | `no line "Opened \"<title>\" in a new Herdr pane." within 10 s` |
| open-in-pane | `open_in_pane_not_in_herdr_fails` | `talaria did not detect Herdr and printed: {the line}` |
| needs-you | `needs_you_launch_fails` | `the needs-you launch ended with exit code {n} or printed no single "status":"launched" line` |
| needs-you | `needs_you_never_listed_fails` | `no approval request appeared in the fleet list within 120 s on two tries (session state: {state})` |
| needs-you | `needs_you_too_old_fails` | `the request is {n} s old, too old to answer before Hermes denies it at 300 s; run again` |
| needs-you | `needs_you_notice_fails` | `the start-up notice "1 background session needs you; press alt+a to see it." did not appear within 20 s` |
| needs-you | `needs_you_segment_fails` | `the status line did not show "⚑ 1 needs you" within 20 s` |
| needs-you | `needs_you_peek_fails` | `alt+a did not open the fleet view on the request (no "rm -rf ./talaria-smoke-probe") within 5 s` |
| needs-you | `needs_you_no_deny_fails` | `pressing a did not show a line "<n>. deny" within 5 s` |
| needs-you | `needs_you_not_cleared_fails` | `the request was still listed 10 s after deny` |
| needs-you | `needs_you_command_ran_fails` | `the folder talaria-smoke-probe is gone: the command ran although it was denied` |
| needs-you | `needs_you_segment_stays_fails` | `the status line still showed "⚑" after the overlay closed` |
| needs-you | `needs_you_skips_when_the_command_ran_unasked` | `no approval card was asked for (is approvals.mode manual on the profile?)` |

The shared steps' reasons are P1-13's. A reason is changed only by editing the issue and this table together. A text that differs from its source issue's is a FAIL with the step's reason and the pane text, never relaxed.

### 9.5 Talaria screen texts the steps depend on

Every text is a constant in `wording3.rs` with a comment naming its source issue and, for a Hermes text, its file at Hermes v0.22.4; a wording that changed at the pinned release is reported on its owner and the constant updated, never matched loosely.

| Text | Source |
|---|---|
| `No subagents in this session yet.`; `Unknown /agents option: frobnicate. Use /agents, /agents pause or /agents resume.` | `P3-04` UAT-5 |
| `No background processes in this session.`; the line starting `Usage: /bg <prompt>` | `P3-05` UAT-4 |
| the ` (here)` suffix of the foreground row; `No sessions match "zzz-no-match".` | `P3-02` UAT-5 and UAT-6 |
| `<n>. deny` on the approval card in the peek panel; the row detail `approval: <command>` | `P3-02` UAT-3 |
| `⚑ <n> needs you`; `1 background session needs you; press alt+a to see it.` | `P3-06` UAT-1 and UAT-5 |
| `"status":"launched"`; `talaria --bg cannot be combined with -p.`; `Worktree: branch talaria/<name> from origin/main at <commit>`; `Launched session <id> in profile <p>.` | `P3-03` UAT-1, UAT-2 and UAT-6 |
| `profile "nosuch" is not served by this gateway`; `no Hermes gateway is running for this Hermes home`; the group headers `NEEDS YOU (n)`, `WORKING (n)` | `P3-01` UAT-1, UAT-5 and UAT-6 |
| `── Resumed:`; `talaria: no session matches "<ref>"`; `Opened "<title>" in a new Herdr pane.`; `Not inside Herdr or tmux` | `P3-08` UAT-1, UAT-2, UAT-3 and UAT-5 |

### 9.6 Time budget and defaults

| Step | Limit (s) |
|---|---|
| fleet-quiet | 30 |
| bg-launch | 60 |
| worktree-launch | 90 |
| fleet-list | 180 |
| attach | 60 |
| startup | 30 |
| plain-answer | 90 |
| local-commands | 30 |
| fleet-view | 30 |
| open-in-pane | 45 |
| quit | 30 |
| needs-you | 420 |

A step's limit is the sum of its own waits, each of which fails with its own reason of 9.4; there is no separate "limit exceeded" reason (the run deadline has `P1-13`'s `deadline exceeded in step <name>`). The `needs-you` worst case is two attempts of a 15 s launch and a 120 s poll, the 30 s wait between them, a 30 s start-up, 20 s for the notice, 5 s each for `alt+a` and the card, 10 s to clear, 1.2 s of Esc waits and 15 s to quit: 386.2 s, under 420. The limits add up to 1095 s. The default `--deadline-secs` for `phase3` is 1200, above that sum (the test `step_limits_sum_below_the_deadline` proves it from the table in code, so a limit raised later cannot silently exceed the deadline). `--step-scale` multiplies every limit as in the earlier suites. The step limits are the suite's own and are not user-visible budgets. No elapsed time is claimed here: the first live run prints the real one, and the issue's limit (n under 1200 for each of AUTO-11 and AUTO-12) is checked against it.

### 9.7 Typing rules and safety (the issue's REQ-3)

The suite sends only: the shell lines of 9.3.1 to 9.3.5 and 9.3.12; the slash lines `/tasks`, `/agents frobnicate`, `/processes`, `/bg`, `/fleet`, `/quit`; the prompts `Reply with only the word ok.`, token A's prompt and `Run this exact shell command: rm -rf ./talaria-smoke-probe` (inside the scratch `probe` folder, a command the model is expected to ask approval for and that the suite then denies); the filter text `/`, `zzz-no-match` and `ID1`; and the keys `enter`, `esc`, `ctrl+c`, `alt+a`, the letters `a` and `o`, and the one digit read from the screen. It never sends the auto-approve flag (`prompt.submit` with it would approve every command of the turn: Hermes v0.22.4 `gateway/session_finite.py`, `admit_finite`; `tools/approval.py` lines 327 to 333), `/permissions`, `/yolo`, a bare `/compact`, `r` or `x` in the fleet view (reply, stop), `p` or `shift+p` (push, pull request), or anything that changes Hermes configuration, starts a gateway or restarts one. Every `talaria` launch carries `--no-ensure`, the refused `--bg -p` line included, except `fleet list` (which has no such option and starts nothing) and the empty-home line (a `fleet list` too, which proves nothing starts). The submit fallback never sends Enter after a slash line other than `/quit` or while an overlay is open (9.3 intro). No file under `tools/smoke/src` holds the word `unattended`, even in a comment (AUTO-6).

### 9.8 Screens and tool output to record

The test author records these from a debug build of `main` after the eight children are merged, in a throwaway Herdr tab with a real gateway and profile `fixture`, with `herdr pane read` (both sources) and by redirecting shell output; recordings are fixtures, never evidence. Files under `tests/fixtures/screens/phase3/`: `fleet-quiet-empty.txt`, `fleet-quiet-busy.txt` (holding a `NEEDS YOU` group and also `IDLE` and `DONE` groups, so the reason's `{headers}` is proved to name only the offending ones), `size-narrow.txt`, `attach-ambiguous.screen.txt` (the A3 text, recorded with `P3-08` UAT-2's two alike titles), `bg-launch-ok.json.txt`, `bg-launch-refusal.err.txt`, `worktree-launch.out.txt`, `fleet-list-rows.json`, `fleet-list-needs-you.json`, `fleet-list-unserved.txt`, `fleet-list-no-gateway.txt`, `attach-resumed.screen.txt`, `attach-no-match.screen.txt`, `local-tasks.screen.txt`, `local-agents-option.screen.txt`, `local-processes.screen.txt`, `local-bg-usage.screen.txt`, `fleet-view-here.screen.txt`, `fleet-view-no-match.screen.txt`, `open-in-pane-notice.screen.txt`, `open-in-pane-not-in-herdr.screen.txt`, `needs-you-notice.screen.txt`, `needs-you-peek.screen.txt`, `needs-you-card.screen.txt`, `needs-you-cleared.screen.txt`. Each is scrubbed (no folder, user name or profile other than `fixture`; titles replaced by `<title>`, ids by `local-<hex>`) and `phase3_fixtures_hold_no_paths_or_names` scans them. A text that cannot be recorded because its source issue is not built at the time is a stop condition for the author (stop and report; never invent a screen).

### 9.9 Tests in detail

Files under `tools/smoke/tests/`, in this order, each function named here belongs to the issue's contract. Every test runs offline on `FakeTerminal`, `ManualClock` and the recorded files; the extension starts from stubs whose bodies panic `not implemented`, so each test fails for the reason it names.

- `screen3.rs` (20): `fleet_headers_reads_names_and_counts`, `fleet_headers_ignores_other_lines`, `here_row_found_by_its_marker`, `here_row_absent_when_the_overlay_is_closed`, `needs_you_segment_reads_the_count`, `needs_you_segment_absent_without_the_mark`, `launch_result_reads_launched`, `launch_result_refuses_two_lines`, `launch_result_reads_not_started`, `fleet_row_state_finds_the_id`, `fleet_row_state_is_none_for_another_id`, `needs_you_ids_lists_the_queue`, `needs_you_ids_empty_queue`, `deny_digit_reads_the_digit_beside_deny`, `deny_digit_none_without_a_card`, `opened_notice_reads_the_title_for_herdr`, `opened_notice_refuses_the_tmux_text`, `worktree_line_reads_branch_and_commit`, `worktree_line_absent`, `launched_id_reads_the_plain_text_line`.
- `steps3.rs` (59): `fleet_quiet_passes`, `fleet_quiet_exit_fails`, `fleet_quiet_narrow_pane_fails`, `fleet_quiet_busy_fleet_fails`, `bg_launch_passes`, `bg_launch_exit_fails`, `bg_launch_output_fails`, `bg_launch_slow_fails`, `bg_launch_refusal_exit_fails`, `bg_launch_refusal_text_fails`, `worktree_launch_passes`, `worktree_launch_exit_fails`, `worktree_launch_text_fails`, `worktree_launch_tree_missing_fails`, `worktree_launch_not_fresh_fails`, `worktree_launch_checkout_dirty_fails`, `fleet_list_passes`, `fleet_list_exit_fails`, `fleet_list_never_done_fails`, `fleet_list_worktree_session_missing_fails`, `fleet_list_unserved_profile_fails`, `fleet_list_no_gateway_fails`, `fleet_list_no_gateway_started_fails`, `attach_passes`, `attach_no_resumed_fails`, `attach_ambiguous_fails`, `attach_exit_fails`, `attach_no_match_fails`, `local_commands_passes`, `local_commands_tasks_fails`, `local_commands_agents_option_fails`, `local_commands_processes_fails`, `local_commands_bg_usage_fails`, `fleet_view_passes`, `fleet_view_open_fails`, `fleet_view_no_match_fails`, `fleet_view_close_fails`, `open_in_pane_passes`, `open_in_pane_notice_fails`, `open_in_pane_not_in_herdr_fails`, `needs_you_passes`, `needs_you_launch_fails`, `needs_you_never_listed_fails`, `needs_you_too_old_fails`, `needs_you_notice_fails`, `needs_you_segment_fails`, `needs_you_peek_fails`, `needs_you_no_deny_fails`, `needs_you_not_cleared_fails`, `needs_you_command_ran_fails`, `needs_you_segment_stays_fails`, `needs_you_skips_when_the_command_ran_unasked`, `a_failing_step_stops_the_run_and_cleans_up`, `steps_run_in_the_documented_order`, `only_needs_you_may_skip`, `needs_you_retries_once_then_fails`, `skip_exits_3_and_fails_under_require`, `shared_steps_behave_as_in_phase_1`, `step_limits_sum_below_the_deadline`.
- `typed_input3.rs` (6): `nothing_typed_contains_unattended`, `no_dangerous_slash_commands_are_typed`, `every_launch_line_carries_no_ensure`, `only_allowed_keys_are_sent`, `needs_you_types_only_the_digit_found_on_screen`, `slash_lines_get_no_submit_fallback` (a script whose screen stays still after `/fleet` and `/tasks`: no extra `enter` is sent).
- `cleanup3.rs` (7): `tab_closed_after_pass`, `tab_closed_after_failure`, `tab_closed_after_panic`, `tab_closed_after_signal`, `scratch_removed_unless_kept`, `unclosed_tab_gives_failed_cleanup`, `scratch_holds_every_file_and_worktree_the_suite_made`.
- `cli3.rs` (9, on `fake-herdr`): `help_lists_the_four_suites`, `phase3_is_accepted`, `default_suite_is_phase1`, `unknown_suite_exits_2_with_the_parse_text`, `phase3_exit_codes`, `phase3_with_another_profile_model_or_provider_exits_2`, `phase3_needs_no_tmux`, `phase3_default_deadline_is_1200`, `phase3_over_long_line_exits_2` (a `--talaria` path that makes the needs-you line longer than 600 characters).
- `fixtures_public_safe3.rs` (1): `phase3_fixtures_hold_no_paths_or_names`.
- `live_smoke3.rs` (1, `#[ignore]`): `live_phase3_smoke_in_herdr`: exit 0 and a line matching `^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$`; it fails, never skips, outside Herdr.

Expected passes on the red run (they guard existing or recorded behaviour): `cli3::default_suite_is_phase1`, `fixtures_public_safe3::phase3_fixtures_hold_no_paths_or_names`, and `cli3::unknown_suite_exits_2_with_the_parse_text` when an earlier test pins the parse text (9.2, which also says what the stub commit leaves unchanged). `needs_you_retries_once_then_fails` also proves the first attempt's lines do not satisfy the repeat. `only_needs_you_may_skip` fails the run when any other step reports a skip.

### 9.10 Cleanup, signals, safety

As P1-13 4.8 and its tab guard: the tab is closed whether the run passed, failed, panicked or was interrupted; a tab that could not be closed gives `SMOKE_RESULT: FAILED (cleanup)`. The scratch root holds every file and folder the suite made, including the worktree (which lives under `a/.worktrees/`), and is removed at the end unless `--report-dir` asks to keep captures; the sessions the suite started stay in the gateway (a finished turn, nothing waiting: the `needs-you` approval was denied), which is why the next run's `fleet-quiet` step finds a quiet fleet once the denied turn has ended (the tester's quiet-fleet check between runs waits for that, section 3). If the step `needs-you` fails after the approval card appeared, the suite does not answer it: Hermes denies it at 300 s, and the report says `a request may be waiting: it is denied by Hermes after 300 s`; the next run's `fleet-quiet` step fails with the headers until the request is answered as section 3 says or Hermes has denied it.

### 9.11 README additions

`tools/smoke/README.md` gains: the fourth suite in Usage; the preconditions of the `needs-you` step (the profile's `approvals.mode` is `manual`, or the step skips); the exit codes unchanged; the 12 steps in a table; the reading of a failure; and the sentence that the suite never answers an approval except by denying the one it provoked.

### 9.12 Coverage map: which feature is checked where

| Feature (issue) | Smoke step | Not in the suite (where it is covered) |
|---|---|---|
| Fleet list, groups, refusals (P3-01) | fleet-quiet, fleet-list, needs-you | the watch mode and the JSON stream (P3-01 UAT-2 and UAT-4) |
| Fleet view, filter, peek, deny (P3-02) | fleet-view, needs-you | reply, stop, open as foreground (P3-02 UAT-2, UAT-4, UAT-5) |
| Background and worktree launch (P3-03) | bg-launch, worktree-launch | `w` in the fleet view and the dialog (P3-03 UAT-3; XF-4) |
| Subagent panel (P3-04) | local-commands (the empty panel and the option error) | the tree, tail, steer, stop, pause (P3-04 UAT-1 to UAT-4 and UAT-6; XF-6) |
| Background processes and `/bg` (P3-05) | local-commands (the empty list and the usage line) | the results and kill (P3-05 UAT-1 to UAT-3; XF-7) |
| Attention (P3-06) | needs-you (notice and counter, not the bell bytes) | the bell and desktop bytes (P3-06 UAT-1 to UAT-4) |
| Review and pull request (P3-07) | none | all of it (P3-07 UAT-1 to UAT-5; XF-4; OP-4) |
| Attach and open in a pane (P3-08) | attach, open-in-pane (the notice) | tmux, the second pane's contents, ambiguity (P3-08 UAT-2 to UAT-5; XF-3, XF-8) |

### 9.13 Journal entries

`docs/engineering-journal/LEARNINGS.md` records (when the run shows them): which Herdr read source the fleet overlay and the status line need; whether `alt+a` and `o` are accepted as key names by the Herdr in use; the real time a live `--bg` launch and a worktree launch take; and any wording that changed between an issue's text and the real binary. `DECISIONS.md` records, on `p3-acc-smoke` before the freeze (AC-6 forbids it on the record branch), that the suite answers only the approval it provoked, and only by denying it, and why (`needs-you`). The stage 0 check's results (9.0) go in `LEARNINGS.md` on the same branch.



## 10. Questions the operator owns, and facts not verified

### 10.1 The shape of the record

This unit takes the shape the earlier gate was accepted with. Shape A: the four files `phase-3.md`, `phase-3-evidence.md`, `check-phase-3.sh` and `check-phase-3.test.sh` with the checker of section 6. Shape B: the same 77-row table and evidence in the pull request body and its comments, with no checker. [`PH2-ACC`](https://github.com/infiquetra/talaria/issues/238) takes the shape of [`PH2A-ACC`](https://github.com/infiquetra/talaria/issues/229), which defaults to B on silence, so the rule is: a `phase-2.md` in the merged pull request of [`PH2-ACC`](https://github.com/infiquetra/talaria/issues/238) means A; otherwise, and with no ruling anywhere, B. Draft ADR-0012 ([verification is proportionate](https://github.com/infiquetra/talaria/blob/main/docs/handoff/2026-10-22-next-steps/adr-drafts/0012-verification-is-proportionate.md)) decision 4 bars "per-release receipt schemas, digest-bound evidence trees or release-record tests"; shape B honours it most plainly, shape A is a small script and a table. The operator may rule differently on [`PH3`](https://github.com/infiquetra/talaria/issues/215) before the freeze.

Under shape B the lead removes from the issue, before the freeze and in one commit: the checker and its tests (REQ-9), UAT-1, AC-1's second half, AC-4, AC-10 and AC-13; AC-15 then counts 4 scenarios; the other criteria read `$REC` and `$EVI` and run unchanged, with both set to `$X/body.md`, the saved pull request body and evidence comments. AC-5 reads only the lines before the table (`sed -n '1,/^| id |/p' $REC`) and needs a 40-hex `Candidate:` line, so the evidence sections' own `Candidate: <7 hex>. Runner: ...` lines, which every section has (section 5.1), are never counted as header lines. The lead posts the shape on [`PH3`](https://github.com/infiquetra/talaria/issues/215) before stage 0; if [`PH2-ACC`](https://github.com/infiquetra/talaria/issues/238) merged with files that match neither shape, stop and ask the operator. The spec's sections 6 and the checker lines of 4 are then reference only.

### 10.2 Questions

1. **The numbers of the fleet day** (section 7.4): at least 2 calendar days inside 7, at least 2 hours on each, at least 5 hours in all, and one fleet window of at least 30 minutes. Silence means they stand; the operator may lower or raise them.
2. **Three providers.** OP-1 needs three provider ids with a working key and one cheap model each on the profile the operator uses for the day (public ids only on the issue, never a key). Silence blocks OP-1 (and so the verdict).
3. **Is the `phase3` smoke suite built?** Default yes; silence means it is built. If not, section 9.1 says what the lead removes.
4. **The idle-cost limits of XF-9** (40960 KiB at start, 102400 KiB after, growth 4096 KiB, CPU 1 s per two minutes) and **the deadline of 1200 s**. Silence means they stand.
5. **A throwaway GitHub repository** for `P3-07` UAT-3, `live_review_gh` and OP-4, with `gh` signed in (never named in the issue).
6. **[`P3-09`](https://github.com/infiquetra/talaria/issues/247) (resident agents).** If decision 8 of [`P0-01`](https://github.com/infiquetra/talaria/issues/194) makes it part of phase 3 and it is merged before the freeze, its scenarios become rows by the procedure of section 8.1 item 4, and a cross-feature row for it is added if the lead sees a seam. If the decision is "no", nothing changes.
7. **A second Ollama Cloud model** `<M2>` on `fixture` (XF-2 and XF-11). Silence blocks those two rows.
8. **The shape of the record** (10.1).
9. **Whether `alt+a`, `alt+s`, `shift+p`, `shift+x` and `ctrl+s` may be sent by Herdr** (facts S1 and S2): the stage 0 check (9.0) answers before any screen is recorded. If one is refused, the operator chooses before the freeze: a Herdr name for the same keystroke from `herdr pane send-keys --help` (a constant in `steps3.rs`), or drop the rows and the suite step that need it by section 8.1 item 4 (the keys are then judged on the operator's keyboard, OP-3 and OP-6).

### 10.3 Facts not verified

- The exact shape of `talaria fleet list --json` (fact S5): the `jq` filters come from `P3-01` UAT-3 and `P3-03` UAT-2; the schema file is read at the candidate.
- That `o` works after a filter and Enter without moving the cursor (9.3.10): if the cursor stays on another row the step reads a wrong title; the first live run settles it, and the fix is to press `j` until the row of `ID1` is selected (by reading the overlay), recorded in the journal.
- That a long prompt's worktree slug follows `P3-03` UAT-3's rule (XF-4 uses a short prompt for this reason).
- That a session started by `--bg` and finished stays listed (`P3-01` decision 5; fact 2.2 shows Hermes's own rule keeps it): `P3-01` UAT-4 and `P3-03` UAT-1 assume it, and `fleet-list`, `attach`, `open-in-pane`, XF-3, XF-8 and XF-11 depend on it. The stage 0 check (9.0) confirms it live, and section 8.1 item 7 is the ruling.
- That Hermes delivers a reply event to a second attached viewer (XF-8); `P3-02` decision 8 and `P3-08` decision 7 say it does; verify at the pinned commit.
- The wording of the `--bg` and `attach` no-gateway failures (XF-12): `P3-03` REQ-3 and `P3-08` REQ-4 keep `P1-12`'s texts; the first line is matched by its stable prefix only.

## Appendix A: the verbatim scenarios of the accepted issues

Each block is the `User acceptance scenarios` section of the issue's final body at the time this document was written, copied by script. The issue's own text wins if it was edited later; report differences. The `Result:` lines are the issue's template and are not filled in here: results go in the record (section 4).

### A.P3-01: scenarios of P3-01

Common setup (operator sets up the gateway as in Dependencies). The tester makes a scratch folder (`mktemp -d`) holding an empty folder `talaria-fleet-probe`, and opens Herdr pane A (a shell there) and pane B (`talaria --profile fixture` there). Input: `herdr pane send-text <pane> "<text>"`, then `herdr pane send-keys <pane> enter`. Read pane A with `herdr pane read <pane> --source recent-unwrapped`, pane B with `--source visible`. Pane A always passes `--profile fixture`.

#### UAT-1: See every fixture session and its state
- Persona: fleet-lead
- Covers: AC-4, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B sent `Reply with the single word: ready` and got the reply
- Given pane B's session is idle
- When the tester runs `talaria fleet list --profile fixture; echo "exit=$?"` in pane A
- Then the output has an `IDLE (<n>)` header, a line starting `  fixture` with `minimax-m3.1-flash` and `idle`, no other profile name, and `exit=0`
- Evidence: pane A text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: A waiting approval enters the queue and leaves when answered
- Persona: fleet-lead
- Covers: AC-2, AC-7, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given pane A runs `talaria fleet list --watch --profile fixture --for 120`
- When the tester sends `Run this exact shell command: rm -rf ./talaria-fleet-probe` in pane B, waits for its `Approval needed` card, notes the time, then presses Esc there (`deny`)
- Then within 6 s of the card pane A prints `<time> needs you: fixture <title> approval: rm -rf ./talaria-fleet-probe`; within 6 s of Esc it prints `<time> cleared: fixture <title>`; `talaria-fleet-probe` still exists (6 s allows the tester's reading delay; live test 50 is the 5 s gate)
- Evidence: both panes' text; the two latencies; `ls -d talaria-fleet-probe`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: A question asked while Talaria was closed is still shown
- Persona: developer
- Covers: AC-4, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; UAT-2 done (its approval was denied)
- Given pane B sent UAT-2's prompt again and shows its card (a focused card takes typed keys, so `/quit` cannot be typed)
- When the tester runs in pane A `pkill -TERM -f -x 'talaria --profile fixture'`, then within 60 s `talaria fleet list --profile fixture`, then in pane B `talaria --resume "$(talaria fleet list --json --profile fixture | jq -r '.needs_you[0].session_id')" --profile fixture`, presses Esc on the restored card, and lists again in pane A
- Then the first listing has `NEEDS YOU (1)` and a line containing `fixture` and `approval: rm -rf ./talaria-fleet-probe`, then an age; the second has no `NEEDS YOU` group; `talaria-fleet-probe` still exists
- Evidence: pane A and pane B text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: A background turn that finishes shows as done
- Persona: fleet-lead
- Covers: AC-7, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B idle
- Given pane A runs `talaria fleet list --watch --json --profile fixture --for 120 > watch.jsonl`
- When the tester sends `Count slowly from one to one hundred, one number per line` in pane B, then `/quit` while it works, and waits for pane A's command to end
- Then `jq -c 'select(.kind=="row") | [.row.state, .row.outcome]' watch.jsonl` ends with `["done","completed"]` for that session (a `["working",null]` line before it is expected; a turn shorter than one poll may skip it)
- Evidence: the `jq` output
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: No gateway for the Hermes home (negative)
- Persona: scripter
- Covers: AC-4, AC-8
- Runner: agent tester in Herdr
- Preconditions: a built `talaria`
- Given an empty temporary folder as Hermes home
- When the tester runs `H="$(mktemp -d)"; HERMES_HOME="$H" talaria fleet list; echo "exit=$?"; ls -A "$H"`
- Then within 2 s it prints `talaria fleet: no Hermes gateway is running for this Hermes home. Start one with "hermes gateway ensure", or start talaria.`, `exit=1`, and the folder listing is empty (no gateway started)
- Evidence: pane A text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-6: An unserved profile is refused without naming the others (negative)
- Persona: scripter
- Covers: AC-4, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given no served or parked profile is named `nosuch`
- When the tester runs `talaria fleet list --profile nosuch; echo "exit=$?"`
- Then it prints `talaria fleet: profile "nosuch" is not served by this gateway (<n> profiles are served; run without --profile to see them).` with a number, no other profile name, and `exit=2`
- Evidence: pane A text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P3-02: scenarios of P3-02

Common setup (operator preconditions above). In a scratch folder (`mktemp -d`) holding an empty folder `talaria-peek-probe`, two Herdr panes run `talaria --profile fixture`: A is the operator's own session; B starts a session, then `/quit`. Input: `herdr pane send-text <pane> "<text>"`, then `herdr pane send-keys <pane> enter` (or the named key); read with `herdr pane read <pane> --source visible`. Find B's row in A with `/`, B's id, Enter.

#### UAT-1: See the fleet and peek at a working session
- Persona: fleet-lead
- Covers: AC-3, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B sent `Run this exact shell command: sleep 30 && echo peek-ok`, shows it working, then `/quit`
- Given pane A is idle at its prompt
- When the tester sends `/fleet` in pane A, filters to B's id, presses space, waits 3 s, then waits until 40 s after B's prompt
- Then the overlay shows `WORKING (` with B's row marked `◐`; within 2 s of space the panel shows a line containing `sleep 30`; after the command ends the panel shows `peek-ok` and the row is under `DONE (`
- Evidence: pane A text at each step; the time from space to the `sleep 30` line
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: A reply is queued and does not interrupt
- Persona: fleet-lead
- Covers: AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup with a fresh B session: pane B sent `Run this exact shell command: sleep 30 && echo peek-ok`, shows it working, then `/quit`; begin within 10 s
- Given B's row is peeked in pane A's fleet view
- When the tester presses `r`, types `Now reply with the single word: queued-ok` and presses enter
- Then pane A shows `Queued for "<title>": it runs after the current turn.`; the panel later shows `peek-ok` (not interrupted), then a new turn containing `queued-ok`
- Evidence: pane A text after enter and after 45 s
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: Answer an approval from the peek panel
- Persona: fleet-lead
- Covers: AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B sent `Run this exact shell command: rm -rf ./talaria-peek-probe`, shows its approval card, then `/quit`; the tester presses the digit within 240 s of B's card appearing (Hermes denies at 300 s)
- Given pane A is idle at its prompt
- When the tester sends `/fleet` in pane A, puts the cursor on the `NEEDS YOU` row, presses space, then `a`, waits 1 s, then presses the digit shown beside `deny`
- Then before the digit the row's detail reads `approval: rm -rf ./talaria-peek-probe` and the panel still shows the card with a line `<n>. deny`; within 5 s of the digit the row leaves `NEEDS YOU`; `ls -d talaria-peek-probe` still prints the folder; pane A's status line still names its own session
- Evidence: pane A text before and after; the `ls` output
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: Stop a working session
- Persona: fleet-lead
- Covers: AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B sent `Run this exact shell command: sleep 60 && echo too-late`, shows it working, then `/quit`
- Given B's row is peeked in pane A's fleet view
- When the tester presses `x`, reads the confirmation, presses `x` again, and waits 70 s
- Then the confirmation starts `Stop "`; within 10 s of the second `x` the panel shows `■ interrupted` and the row's detail is `interrupted`; after 70 s no line contains `too-late`
- Evidence: pane A text after each `x` and at 70 s
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: Open a background session as the foreground session
- Persona: developer
- Covers: AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: UAT-1 done (B's session finished with `peek-ok`)
- Given pane A's fleet view with the cursor on B's row
- When the tester presses enter, then sends `/fleet` again
- Then the overlay closes; pane A's scrollback shows the switch notice and B's cells including `peek-ok`; the status line names B's session; the reopened view shows B's row with ` (here)` and A's earlier session marked `·` or `✓`
- Evidence: pane A text after enter and after reopening
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-6: Cancelled and refused actions change nothing (negative)
- Persona: fleet-lead
- Covers: AC-1, AC-8
- Runner: agent tester in Herdr
- Preconditions: as UAT-4, before stopping; pane A's own session idle
- Given pane A's fleet view
- When the tester presses `x` then esc on B's row; presses `x` on pane A's own idle row; presses `/`, types `zzz-no-match`, enter; then presses ctrl+c and reopens `/fleet`
- Then `Stop cancelled.`, `Nothing is running in "<title>".` and `No sessions match "zzz-no-match".` appear in turn; after ctrl+c the overlay is closed, pane A's prompt is intact, and the reopened view still shows B's row under `WORKING (`
- Evidence: pane A text after each step
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P3-03: scenarios of P3-03

Common setup (operator preconditions as in Dependencies and gates); a scratch repository whose `main` is one commit behind `origin`: `S="$(mktemp -d)"; G="git -c user.name=fixture -c user.email=fixture@example.invalid"; git init -q --bare -b main "$S/origin.git"; git clone -q "$S/origin.git" "$S/repo"; $G -C "$S/repo" commit -q --allow-empty -m one; git -C "$S/repo" push -q -u origin main; git clone -q "$S/origin.git" "$S/other"; $G -C "$S/other" commit -q --allow-empty -m two; git -C "$S/other" push -q`. Herdr pane A is a shell in `$S/repo`; pane B runs `talaria --profile fixture` there. Type with `herdr pane send-text` then `herdr pane send-keys <pane> enter`; read A with `--source recent-unwrapped`, B with `--source visible`.

#### UAT-1: A background launch returns at once and keeps working
- Persona: developer
- Covers: AC-6, AC-8, AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given pane A in `$S/repo` runs `talaria fleet list --watch --json --profile fixture --for 120 > "$S/w.jsonl" &` and then `until grep -q '"kind":"snapshot"' "$S/w.jsonl"; do sleep 1; done`
- When the tester runs `time talaria --bg --output-format json --profile fixture "Count from one to five." | tee "$S/l.json"; echo "exit=${PIPESTATUS[0]}"`, then `wait; jq -r .session_id "$S/l.json"; grep '"kind":"row"' "$S/w.jsonl" | jq -r 'select(.row.state=="done") | .row.session_id'`
- Then within 5 s the launch prints a line with `"status":"launched"` and `exit=0`; the last command prints the same id as `jq -r .session_id`
- Evidence: pane A text; the `real` time
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-2: A worktree launch starts from fresh origin and keeps edits out of the checkout
- Persona: fleet-lead
- Covers: AC-2, AC-8, AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given `git log -1 --format=%s main` prints `one`
- When the tester runs `talaria --bg --worktree --name probe-one --profile fixture "Create a file named hello.txt containing the word hi in the current folder. Do nothing else."; echo "exit=$?"; ID=$(git config --get branch.talaria/probe-one.talaria-session)`, polls every 5 s (180 s limit) until `T=.worktrees/talaria-probe-one; ls $T/hello.txt` succeeds and `talaria fleet list --json --profile fixture | jq -r --arg id "$ID" '.rows[] | select(.session_id==$id) | .state'` prints `done` or `idle`, then runs `git -C $T log -1 --format=%s; ls $T/hello.txt hello.txt; git status --porcelain; git diff --stat -- .gitignore`
- Then the launch prints `Worktree: branch talaria/probe-one from origin/main at <commit>` and `exit=0`; the state never showed `needs_you` or `failed`; the log prints `two`; only the tree's `hello.txt` exists; `git status --porcelain` and the diff print nothing
- Evidence: pane A text
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-3: `w` in the fleet view launches without leaving the session
- Persona: fleet-lead
- Covers: AC-4, AC-5, AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B shows its composer
- Given the tester types `/fleet` and Enter in pane B, and the fleet view is open
- When the tester presses `w`, types `Write the word ready into notes.txt`, presses Enter; after the dialog closes, presses Esc (closing the fleet view) and types `x`
- Then the dialog closes within 10 s; the notice `Launched in fixture: "Write the word ready into notes.txt" (worktree talaria/write-the-word-ready-into-notes-txt)` appears; the new session is listed within 3 s; after Esc, `x` is in the composer
- Evidence: pane B text before and after Esc; `ls -d .worktrees/talaria-write-the-word-ready-into-notes-txt` in pane A
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-4: An approval from a background agent waits for the operator
- Persona: fleet-lead
- Covers: AC-3, AC-8, AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup; `mkdir talaria-bg-probe` in pane A
- Given no `fixture` session needs the operator
- When the tester runs `talaria --bg --profile fixture "Run this exact shell command: rm -rf ./talaria-bg-probe"` in pane A, runs `talaria fleet list --profile fixture` every 5 s for up to 60 s, then in pane B types `/quit`, runs `talaria --resume <id> --profile fixture` and presses Esc on the approval card (`deny`, [`P1-09`](https://github.com/infiquetra/talaria/issues/206))
- Then a listing shows `NEEDS YOU (1)` and a line with `fixture` and `approval: rm -rf ./talaria-bg-probe`; afterwards `ls -d talaria-bg-probe` prints the folder
- Evidence: pane A listings; pane B text
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-5: A worktree launch outside a repository or without origin starts nothing (negative)
- Persona: developer
- Covers: AC-2, AC-6, AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given `N=$(talaria fleet list --json --profile fixture | jq '.rows | length')`
- When the tester runs `cd "$(mktemp -d)" && talaria --bg --worktree --profile fixture "hi"; echo "exit=$?"`, then `R="$(mktemp -d)"; git -C "$R" init -q; $G -C "$R" commit -q --allow-empty -m x; cd "$R" && talaria --bg --worktree --profile fixture "hi"; echo "exit=$?"; ls -A "$R"`
- Then the first prints `talaria: <folder> is not inside a git repository.`, the second `talaria: this repository has no remote named origin.`, each `exit=4`; `ls -A` lists only `.git`; the row count is still `N`
- Evidence: pane A text; both counts
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-6: Wrong flag combinations are refused before anything runs (negative)
- Persona: scripter
- Covers: AC-6, AC-9
- Runner: agent tester in Herdr
- Preconditions: a built `talaria`
- Given pane A in any folder
- When the tester runs `talaria --bg -p "x" > out.txt; echo "exit=$?"; wc -c < out.txt; talaria --worktree "x"; echo "exit=$?"; talaria --bg --worktree --name '!!' "x"; echo "exit=$?"`
- Then `talaria --bg cannot be combined with -p.`, `talaria --worktree needs --bg.` and `talaria --name needs at least one ASCII letter or digit.` appear, each with `exit=2`, and `wc -c` prints `0`
- Evidence: pane A text
- Result: PASS / FAIL / BLOCKED / NOT RUN

### A.P3-04: scenarios of P3-04

Common setup (operator sets up the gateway as under Dependencies). The tester makes a scratch folder (`mktemp -d`) holding `notes.txt` (`alpha beta`), opens Herdr pane B there, checks `tput cols` ≥ 100, runs `talaria --profile fixture`. Input: `herdr pane send-text <pane> "<text>"`, then `herdr pane send-keys <pane> enter`. Read the live region with `herdr pane read <pane> --source visible`, scrollback with `--source recent-unwrapped`. If an approval appears, deny it and record BLOCKED (environment). Prompt B: `Use the delegate_task tool to run two subagents in parallel: one lists the files in the current directory, the other reads notes.txt. Then summarise both results in one sentence.` Prompt L: `Use the delegate_task tool to start exactly one subagent. Its goal: run the shell command sleep 3 ten times, one command at a time, then reply with the single word finished. Wait for it, then reply with the single word done.`

#### UAT-1: Children appear above the composer and a block lands in scrollback
- Persona: developer
- Covers: AC-6, AC-10
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given pane B is idle
- When the tester sends prompt B and reads the live region every 1 s until the reply ends (if no read catches the dock, repeat once before recording FAIL)
- Then a read shows a line starting `⎇ Subagents ·` with `running`; afterwards scrollback has one line starting `⎇ Subagents · 2 finished` and two child rows, and the live region has no `⎇ Subagents`
- Evidence: the live reads and the final scrollback read
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: Open the panel and read a child's live transcript
- Persona: developer
- Covers: AC-6, AC-10
- Runner: agent tester in Herdr
- Preconditions: common setup; prompt L sent; the dock shows `1 running`
- Given the composer has focus
- When the tester sends `herdr pane send-keys <pane> alt+s`, reads, presses Enter, waits 5 s, reads, then presses Esc twice and reads
- Then the panel shows `Subagents ·` and one `◐` row whose model column reads `minimax-m3.1-flash`; the tail shows `Live transcript ·` with log text; after Esc twice the composer and the dock are back
- Evidence: the three reads
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: Steer a running child
- Persona: developer
- Covers: AC-6, AC-10
- Runner: agent tester in Herdr
- Preconditions: as UAT-2, panel open on the `◐` row
- Given the row is running
- When the tester presses `e`, sends `After your current command, stop and reply with the single word: steered`, and reads within 10 s; on `is no longer accepting steering` the tester repeats from prompt L, three tries in all
- Then a try's flash line shows `Steer queued for "`
- Evidence: the read
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: Stop one child
- Persona: fleet-lead
- Covers: AC-6, AC-10
- Runner: agent tester in Herdr
- Preconditions: as UAT-2, panel open on the `◐` row
- Given the row is running
- When the tester presses `x`, then reads every 5 s for 60 s
- Then the flash line shows `Stop requested for "`, the row shows `stopping` then `■` within 60 s, and scrollback gets a block starting `⎇ Subagents · 1 finished · 1 stopped`
- Evidence: the reads; seconds from `x` to `■`; the parent turn's final status (spec U4)
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: Empty panel, bad option and a stop too late (negative)
- Persona: developer
- Covers: AC-1, AC-6, AC-10
- Runner: agent tester in Herdr
- Preconditions: common setup, new session
- Given no subagent has run
- When the tester runs `/tasks`, reads, presses Esc, sends `/agents frobnicate`, then sends prompt B, opens the panel after the reply, selects a `✓` row and presses `x`
- Then the panel says `No subagents in this session yet.`; the option prints `Unknown /agents option: frobnicate. Use /agents, /agents pause or /agents resume.`; the stop flashes `had already finished.` and the row stays `✓`
- Evidence: the three reads
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-6: Pause new spawns for the whole gateway, then resume
- Persona: configurer
- Covers: AC-6, AC-8
- Runner: operator (pausing affects every session on the gateway)
- Preconditions: common setup; nothing else needs subagents for two minutes
- Given the panel header (`alt+s`) said `spawning on`; the panel is closed
- When the operator runs `/agents pause` and presses `n` (the panel stays open); presses `p`, then `y`; presses `q`; sends prompt B and waits for the reply; runs `/agents resume`; presses `alt+s`
- Then after `n` the header still says `spawning on`; after `y` it says `spawning paused for all sessions`; during and after prompt B no `⎇ Subagents` line appears in the live region or scrollback; `New subagents are allowed again.` prints; the last header says `spawning on`
- Evidence: the reads; whether the reply mentions the pause (informational)
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P3-05: scenarios of P3-05

Common setup: the operator preconditions; a temporary folder (`mktemp -d`, a git project) holding `tick.sh` (`for i in $(seq 1 600); do echo "tick $i"; sleep 1; done`); Herdr pane A (a shell there) and pane B running `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` there. Input: `herdr pane send-text <pane> "<text>"`, then `herdr pane send-keys <pane> enter`. Read pane A with `--source recent-unwrapped`. For pane B, first read with both `--source visible` and `--source recent-unwrapped` after one keystroke and use the source that shows it. An approval for `sh tick.sh` is answered `once`; any other is denied. Prompt P: `Run the command sh tick.sh in the background with the terminal tool (background=true, notify=false). Reply with only its process id.` If the agent does not start the process, retry once in a fresh session, then record BLOCKED (environment: agent did not start the process). `pgrep` line: `pgrep -fx 'sh tick.sh' >/dev/null; echo "exit=$?"` (exact match skips Hermes's wrapper shell; specification H31).

#### UAT-1: A /bg answer arrives while the operator keeps working
- Persona: developer
- Covers: AC-1, AC-3, AC-7, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B idle
- Given an empty composer in pane B
- When the tester submits `/bg Reply with exactly: bg-ok`, then types `still typing` without Enter
- Then within 2 s pane B shows `Background task bg_<6 characters> started.` and the row `◌ bg · Reply with exactly: bg-ok · running · <n>s`, with `still typing` in the composer; within 120 s a cell `bg · Reply with exactly: bg-ok` shows `bg-ok` and the footer `background task bg_… · not part of the conversation`; the row is gone
- Evidence: pane B reads after 2 s and after the cell; time from Enter to the cell
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: Watch a background process and kill it
- Persona: developer
- Covers: AC-1, AC-2, AC-7, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B idle
- Given pane B's session has no background process
- When the tester submits prompt P, waits for the reply, runs the `pgrep` line in pane A, submits `/processes`, reads pane B twice 3 s apart, presses `x`, then `x` again within 3 s, waits 5 s, presses Esc, then runs the `pgrep` line again
- Then the first `pgrep` prints `exit=0`; before `/processes` the row `⚙ sh tick.sh · <elapsed> · last: tick <n> · /processes` shows; the overlay lists `running  … sh tick.sh` with `tick` lines whose last number grows between reads; the first `x` shows `Press x again within 3 s to kill: sh tick.sh`; after the second the row reads `killed` with the note `Killed. Hermes will not tell the agent about this process's exit.`; Esc restores the transcript; the second `pgrep` prints `exit=1`
- Evidence: the three pane B reads; both pane A outputs
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: /stop ends the turn and every background process
- Persona: fleet-lead
- Covers: AC-1, AC-7, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane B idle
- Given two processes started with prompt P (sent twice), both running in `/processes` (closed with Esc), and `pgrep -fx 'sh tick.sh' | wc -l` printing `2`
- When the tester submits `Count slowly from one to fifty, one number per line`, and while it works submits `/stop`
- Then the turn ends with exactly one interrupted notice ([`P1-05`](https://github.com/infiquetra/talaria/issues/202)'s `Interrupted` notice, as the merged code renders it) and no numbers after it; the notice `Stopped 2 background processes.` appears; within 6 s the process row above the composer starts `■ sh tick.sh · killed`; the `pgrep` line prints `exit=1`
- Evidence: pane B read after `/stop`; pane A outputs
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: Blank and empty inputs are explained and send nothing (negative)
- Persona: developer
- Covers: AC-1, AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; a new pane B session
- Given an idle session with no background process
- When the tester submits `/bg` alone, then `/processes`, presses `x`, then `S`, then Esc
- Then `/bg` shows the usage line starting `Usage: /bg <prompt> —` and no `Background task` notice; the overlay shows `No background processes in this session.`; `x` changes nothing; `S` shows `No background processes are running.`; Esc closes
- Evidence: pane B reads after each step
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: A finished process cannot be killed twice and can be dismissed (negative)
- Persona: developer
- Covers: AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: UAT-2 just ran in the same session (within 30 s)
- Given `/processes` open with the `killed` row selected
- When the tester presses `x`, then `d`, then Esc, and reads pane B within 5 s of Esc
- Then `x` shows `This process has already ended.` and no `Killing` note appears; `d` removes the row (the overlay then shows `No background processes in this session.`); within 5 s of Esc no process row shows above the composer, although the killed process ended under 60 s ago
- Evidence: pane B reads after each key and after Esc
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P3-06: scenarios of P3-06

Common setup (operator preconditions as in Dependencies). The tester makes `T="$(mktemp -d)"` holding an empty folder `talaria-attention-probe`, works there, and uses shell panes A, C and a watcher pane B started fresh per scenario (UAT-2 alone continues UAT-1's B): macOS `script -q "$T/bN.raw" talaria --profile fixture`, Linux `script -q -c "talaria --profile fixture" "$T/bN.raw"` (N is the scenario number). Input: `herdr pane send-text <pane> "<text>"`, then `herdr pane send-keys <pane> enter`. Read B with `--source visible`, A and C with `--source recent-unwrapped`. Count with `perl -0777 -ne 'print scalar(() = /<pattern>/g), "\n"' <file>`. Before each When, `talaria fleet list` in A shows no `NEEDS YOU` or `WORKING` group (else wait, or record BLOCKED). Run scenarios in order 1 to 6; Hermes denies an approval left 300 s (`approvals.timeout`), so answer each within that. Evidence keeps only B's status and notice lines, the selected row and peek, and counts; never other rows or `default/` labels.

#### UAT-1: A background approval rings once and is counted
- Persona: fleet-lead
- Covers: AC-1, AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: common setup; B (`b1.raw`) idle for 10 s; C runs `talaria --profile fixture`
- Given B's composer holds `draft-kept` (typed, no Enter)
- When the tester sends `Run this exact shell command: rm -rf ./talaria-attention-probe` in C, waits for C's approval card, sends `/quit` in C, notes C's printed resume id, and waits 30 s
- Then B's status line shows `⚑ 1 needs you`; the count of `\x07\x1b\]9;Talaria: [^\x07]*needs you: approval: rm -rf \./talaria-attention-probe` in `b1.raw` is exactly `1`; B's composer still shows `draft-kept`
- Evidence: B's status line; the perl count; seconds from C's card to the alert
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: Jump to the request and deny it from the peek panel
- Persona: fleet-lead
- Covers: AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: UAT-1 just passed in the same B; its approval still waits
- Given B shows `⚑ 1 needs you`
- When the tester sends `herdr pane send-keys <B> alt+a`, then `/`, the first 8 characters of C's session id and Enter, reads B, then sends `a`, waits 1 s, sends the digit shown beside `deny`, then Esc three times (filter, peek, overlay)
- Then after `alt+a` the fleet view shows that row selected and the peek showing `rm -rf ./talaria-attention-probe`; within 10 s of the digit B's needs-you segment is gone; `ls -d "$T/talaria-attention-probe"` lists the folder. A new approval from that session is FAIL, reason recorded
- Evidence: selected row and peek lines before, status line after; the `ls` output
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: A session this Talaria never watched finishes quietly but is counted
- Persona: fleet-lead
- Covers: AC-1, AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: common setup; B (`b3.raw`) started fresh and idle; C runs `talaria --profile fixture`
- Given B shows no done segment
- When the tester sends `Run this exact shell command: sleep 30 && echo attention-done` in C, waits for C's working status, sends `/quit` in C, and waits until `talaria fleet list --profile fixture` in A shows no `WORKING` group (at most 90 s)
- Then B's status line shows `✓ 1 done`, and the count of `\x1b\]9;Talaria: [^\x07]*: (done|finished)` in `b3.raw` is `0`
- Evidence: B's status line; the perl count
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: A watched session finishes after the operator switched away
- Persona: developer
- Covers: AC-1, AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: common setup; UAT-3's session is idle, its id read from `talaria fleet list --json --profile fixture` in A; B (`b4.raw`) started fresh 20 s ago
- Given B is idle in its own new session
- When the tester sends `Run this exact shell command: sleep 30 && echo attention-done` in B, then at once `/resume <UAT-3's session id>`, and waits 60 s
- Then the count of `\x1b\]9;Talaria: [^\x07]*: done` in `b4.raw` is `1` and that notification came 25 s or more after the prompt; the count of `\x07\x1b\]9;Talaria: [^\x07]*: done` is `0` (default `bell` excludes `done`); B shows `✓ 1 done`
- Evidence: B's status line; the two perl counts; the timing
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: Requests that waited while Talaria was closed: no bell burst, one notice (negative)
- Persona: developer
- Covers: AC-1, AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: common setup; no B running; the tester makes a new waiting approval in C with UAT-1's When step up to `/quit` (UAT-2 denied UAT-1's)
- Given that approval has waited at least 10 s and under 120 s
- When the tester starts B (`b5.raw`) and waits 20 s
- Then within 16 s of start B shows `1 background session needs you; press alt+a to see it.` exactly once, the status line shows `⚑ 1 needs you`, and the count of `\x1b\]9;Talaria: ` in `b5.raw` is `0`; afterwards the tester denies it in B as in UAT-2
- Evidence: B's notice and status lines; the perl count; seconds from start to notice
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-6: The jump key with nothing waiting (negative)
- Persona: developer
- Covers: AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: common setup; B (`b6.raw`) started fresh and idle 20 s
- Given B's composer is empty
- When the tester notes the BEL count of `b6.raw`, sends `alt+a` to B, then types `x`
- Then B shows `Nothing in the background needs you.`, no overlay opens, the composer shows `x`, and the BEL count is unchanged
- Evidence: B's notice line and composer; the two BEL counts
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P3-07: scenarios of P3-07

Common setup (operator sets up the gateway serving `default` and `fixture`, which uses `minimax-m3.1-flash` on `ollama-cloud`). In `mktemp -d`: `git init --bare -b main origin.git`, `git clone origin.git repo`, commit `a.txt` in `repo`, `git push origin main` (`origin/HEAD` stays unset, G5). Input: `herdr pane send-text <pane> "<text>"`, then `herdr pane send-keys <pane> enter` (or the named key); read with `herdr pane read <pane> --source visible`. `/changes` reviews the folder `talaria` started in (REQ-3).

#### UAT-1: Review, commit and push a worktree session's work
- Persona: fleet-lead
- Covers: AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; in `repo`, `talaria --profile fixture --bg --worktree --name hello "Create a file hello.txt containing the single word hello. Do nothing else."` printed a session id that `talaria fleet list --profile fixture` shows done
- Given a pane running `talaria --profile fixture` in `repo`
- When the tester sends `/fleet`, presses `/`, types the session id, presses Enter (keeps the filter), presses `v`, then `c`, types `feat: add hello`, presses `ctrl+s`; then presses `p`, waits 5 s, presses `p` again
- Then the review lists `?` `hello.txt` `+1`; the editor header reads `Nothing is staged: this commit will include all 1 changed files on talaria/hello.`; the notices read `Committed <sha> on talaria/hello: feat: add hello`, `Pushed talaria/hello to origin.`, then `Already up to date on origin.`; `git -C repo/.worktrees/talaria-hello log -1 --format=%B` prints exactly `feat: add hello`; `git -C origin.git branch --list 'talaria/*'` lists `talaria/hello`
- Evidence: pane text after `ctrl+s` and after each `p`; the `git` outputs
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: Draft a commit message with the session's model
- Persona: developer
- Covers: AC-2, AC-8, AC-10
- Runner: agent tester in Herdr
- Preconditions: UAT-1's tree with one more edit (`echo world >> hello.txt` inside it)
- Given the review of that session, editor empty
- When the tester presses `c`, `ctrl+d`, waits up to 60 s, then presses `ctrl+d` again
- Then `Drafting a message with minimax-m3.1-flash…` appears; the editor fills with a non-empty message; the second press shows a different message or `The model returned the same message.`
- Evidence: pane text at each step; time to the first draft
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: Open a draft pull request and see its label
- Persona: fleet-lead
- Covers: AC-2, AC-9
- Runner: operator
- Preconditions: operator sets up `gh` signed in and a throwaway GitHub repository cloned as `repo`; a worktree session with one commit (as UAT-1)
- Given the review of that session
- When the operator presses `shift+p`, reads the confirmation, presses enter, then Esc
- Then the confirmation names the branch, `main` and `1 commits`; the notice reads `Draft pull request opened: https://github.com/...`; the row's `PR` column shows `#<n> draft`; `gh pr view <n> --json isDraft -q .isDraft` prints `true`
- Evidence: pane text; the `gh` output; then `gh pr close <n> --delete-branch`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: Refusals: default branch and no repository
- Persona: developer
- Covers: AC-3, AC-5, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; pane 1 runs `talaria --profile fixture` in `repo` on `main` with one uncommitted edit; pane 2 runs it in an empty `mktemp -d` folder
- Given both panes idle
- When the tester sends `/changes` in pane 1, presses `p`, then `shift+p`; then sends `/changes` in pane 2
- Then `p` and `shift+p` each show `Talaria does not push or open pull requests from main, the default branch.`; `git -C origin.git branch` lists only `main`; pane 2 shows `<folder> is not inside a git repository.` and no review opens
- Evidence: pane texts; the branch listing
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: Discard asks first and touches one file
- Persona: developer
- Covers: AC-2, AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup; untracked `scratch.txt` and `scratch2.txt` in `repo`; a pane running `talaria --profile fixture` in `repo`
- Given `/changes` open with the cursor on `scratch.txt`
- When the tester presses `shift+x`, then `j`, then `shift+x` twice
- Then the first press shows `Delete untracked scratch.txt? Press X again to discard; any other key cancels.`; `j` shows `Discard cancelled.` and the cursor stays; the double press shows `Deleted untracked scratch.txt.`; `ls repo` still shows `scratch2.txt`
- Evidence: pane text at each step; the `ls` output
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P3-08: scenarios of P3-08

Common setup (operator sets up the gateway as in Dependencies): `cargo build --release`; `T` is that binary's absolute path (never `talaria` from `PATH`); `S="$(mktemp -d)"`; `<tag>` is 6 random hex characters; pane A is a Herdr shell in `$S`. `/rename` is [`P2-07`](https://github.com/infiquetra/talaria/issues/231)'s. Read Talaria panes with `herdr pane read <pane> --source visible`, shells with `--source recent-unwrapped`. **Before pasting evidence**, replace the build path with `$T`, the scratch folder with `$S`, and any profile other than `default` and `fixture` with `<other>`.

#### UAT-1: attach by the start of the id and by title
- Persona: developer
- Covers: AC-2, AC-4, AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given pane A ran `"$T" --profile fixture`, `/rename attach-uat-<tag>`, `Count from 1 to 40, one number per line.`, and `/quit` once numbers appear (the hint reads `talaria --resume local-<hex> --profile fixture`)
- When the tester runs `"$T" attach <first 8 hex after local->`, waits 3 s, reads, types `/quit`, runs `echo "exit=$?"`; then the same with `"$T" attach attach-uat-<tag>`
- Then both screens show `── Resumed: attach-uat-<tag> ──` and the counting exchange (finished, or still streaming) within 2 s, no `--profile` was needed, and both print `exit=0`
- Evidence: the pane text of each attach and each `exit=`; the seconds from Enter to `Resumed`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: no match, several matches, bad input (negative)
- Persona: developer
- Covers: AC-2, AC-4
- Runner: agent tester in Herdr
- Preconditions: UAT-1 done; pane A ran `"$T" --profile default`, `/rename attach-dup-<tag>`, `/quit`, then the same with `--profile fixture` (no prompts; if unlisted, record BLOCKED with the reason, U4)
- Given pane A at a shell prompt
- When the tester runs, each followed by `echo "exit=$?"`: `"$T" attach zz-none-<tag>`; `"$T" attach attach-dup-<tag>`; `"$T" attach attach-dup-<tag> --profile fixture` (then `/quit`); `"$T" attach "  "`; `"$T" attach abcd --model x`; `"$T" attach abcd --profile nosuch`
- Then in order: `talaria: no session matches "zz-none-<tag>" in the profiles default, fixture` (more names may follow) and `exit=1`; `talaria: 2 sessions match "attach-dup-<tag>":` with a `default` and a `fixture` line, `exit=2`; `── Resumed: attach-dup-<tag> ──`, `exit=0`; `talaria: the session to attach to is empty.`, `exit=2`; a line starting `error: ` that names `--model`, `exit=2`; `talaria: the profile nosuch is not served by the gateway (served: default, fixture` (more names may follow), `exit=2`
- Evidence: the pane text of all six, redacted
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: open a session in a new Herdr pane while peeking it
- Persona: fleet-lead
- Covers: AC-5, AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: UAT-1 done; Herdr pane B (at least 120 columns) runs `"$T" --profile fixture` in `$S`; the tester records `herdr pane list` and notes pane B's workspace from it
- Given pane B shows Talaria's composer
- When the tester types `/fleet`, moves with `j` to `attach-uat-<tag>`, presses Space (peek), then `o`, waits 3 s, reads pane B, lists panes, reads the new pane, presses `j` in pane B, reads it; sends `/quit` and Enter to the new pane, reads it (`recent-unwrapped`)
- Then pane B shows `Opened "attach-uat-<tag>" in a new Herdr pane.` with the peek still open; the second pane list has exactly one more pane in pane B's workspace, beside pane B, and still marks pane B as focused (U8: record the field read); pane B's cursor moves on `j`; the new pane shows `── Resumed: attach-uat-<tag> ──` within 3 s (U1, U2); after `/quit` it is a shell showing the typed line `'$T' attach 'local-…' --profile 'fixture'` (U7) and the resume hint
- Evidence: both pane reads, both pane lists, the time from `o` to `Resumed`, redacted
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: open a session in a new tmux pane
- Persona: developer
- Covers: AC-5, AC-7
- Runner: agent tester in Herdr
- Preconditions: UAT-1 done; tmux installed; `X="tmux -L talaria-uat-<tag>"`; an isolated server without Herdr's variables: `env -u HERDR_ENV -u HERDR_PANE_ID -u HERDR_BIN_PATH $X new-session -d -s uat -x 200 -y 50 -c "$S"`
- Given `$X send-keys -t uat "'$T' --profile fixture" Enter` started Talaria in that server
- When the tester sends (`$X send-keys -t uat …`) `/fleet` and Enter, `j` until the cursor is on `attach-uat-<tag>`, then `o`, waits 3 s, runs `$X list-panes -t uat -F '#{pane_id} #{pane_active}'` and `$X capture-pane -p -t <new id>`; finally `$X kill-server`
- Then the first pane shows `Opened "attach-uat-<tag>" in a new tmux pane.`; `list-panes` prints two ids, the first still active (`1`); the new pane shows `── Resumed: attach-uat-<tag> ──` (U3)
- Evidence: both captures and the pane list, redacted
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: outside a multiplexer, and on the session in front (negative)
- Persona: fleet-lead
- Covers: AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: UAT-1 done; the tester records the pane list
- Given pane B runs `env -u HERDR_ENV -u HERDR_PANE_ID -u HERDR_BIN_PATH -u TMUX -u TMUX_PANE "$T" --profile fixture`
- When the tester types `/fleet`, moves to `attach-uat-<tag>`, presses `o`, reads; moves to the row suffixed ` (here)` ([`P3-02`](https://github.com/infiquetra/talaria/issues/240)'s foreground row), presses `o`, reads; then lists panes
- Then the first read shows T3 ending `run this in another terminal: '$T' attach 'local-…' --profile 'fixture'`; the second shows `This session is already open in this pane.`; the pane list is unchanged
- Evidence: both reads and both pane lists, redacted
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN
