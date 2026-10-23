# PH2-ACC: Phase 2 acceptance: the smoke suite on the candidate, then a switch week from Claude Code to Talaria

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what is accepted, in what order, and how the result is recorded. This document holds what the people doing the work would otherwise have to guess: how to freeze and build the candidate, the exact record and evidence templates, how to scrub evidence, the complete checker script and its tests (copied from phase 2a's, with the row list changed, and run while drafting on macOS), the exact steps and pass conditions of the 12 cross-feature scenarios and of the operator's switch week (the 24 parity lines and the 8 operator rows), the run order and the rules for failures, the full design of the third smoke suite (`phase2b`: files, signatures, every step with its exact failure text, the screens to record, the tests), and the questions the operator still owns. Appendix A holds the verbatim text of the 48 scenarios of the eight accepted issues, extracted from their final bodies by script; the source issue's own text wins if it was edited after this document was written.

Sections are numbered so the issue can point at them. Handoff documents are linked relatively: [the roadmap's phase 2](../07-roadmap-and-work-units.md#phase-2--coding-parity), [the P1-13 roadmap entry](../07-roadmap-and-work-units.md#p1-13--live-smoke-test-in-herdr), [the Claude Code parity checklist](../04-prior-art.md#claude-code-parity-checklist), [performance budgets](../05-architecture.md#performance-budgets), [why the Python client's acceptance ceremony failed](../02-current-state.md#why-it-ended-up-here). The patterns copied are [PH1-ACC](PH1-ACC.md) (the record and the checker) and [PH2A-ACC](PH2A-ACC.md) (the second smoke suite); the smoke program extended is [P1-13](P1-13.md).

## 1. What this unit is for, in plain terms

Phase 2 of Talaria was built in two halves. The first half (phase 2a, parent `PH2A`) made reading and writing code feel like Claude Code: tool cells and diffs, the transcript pager, the slash palette, `@` mentions, `!` shell commands, side questions, the task checklist, themes and the comforts; `PH2A-ACC` accepted it. The second half (phase 2b, parent `PH2B`) added control over sessions, models, queues and modes: the model picker (P2-06), the session picker (P2-07), the prompt queue (P2-08), rewind (P2-09), context and compaction (P2-10), headless `talaria -p` (P2-14), clean display of Hermes's own session commands (P2-15) and the permission mode with `/cd` (P2-16). Each of those eight children passed its own scenarios on its own commit. A passing child proves its parts. It does not prove the seams: a model change while a prompt waits in the queue, a rewind while a reply runs, a branch that is then rewound, a headless run that is resumed in the full screen program, a bypass that two features each draw, the whole palette with every phase 2b command in it. Those seams, and the phase 1 and phase 2a features still working on the same commit, are tested only here.

The unit does four things. It runs the automated suites and the three smoke suites (phase 1, phase 2a and a new phase 2b suite that this unit adds) on one candidate commit. It reruns every user acceptance scenario of the eight issues (48). It adds 12 scenarios that only exist when features combine, among them the second half of requirement R4 ("switch a live session between two providers; the backend's default model is unchanged afterwards"), which `P2-06` leaves to this unit. And it turns the roadmap's phase 2 operator check into something a person can mark: the "switch day" of the roadmap ("the operator uses Talaria instead of Claude Code for a week of Hermes-backed coding; every phase 1 and phase 2 parity row works; anything that does not is either fixed or explicitly accepted as a gap") becomes a week log, a matrix of 24 parity lines with a verdict each, and 8 operator rows with pass conditions that can be counted.

The Python client spent 44 percent of the lines it added on acceptance records ([02-current-state.md](../02-current-state.md#why-it-ended-up-here), item 4), so the ceremony stays as small as the earlier gates': two markdown files and one checker of about 55 lines. The shape is the one `PH1-ACC` and `PH2A-ACC` use; whether it stays is the operator's ruling (section 10.1). Draft ADR-0012 decision 4 reads in full: "No per-release receipt schemas, digest-bound evidence trees or release-record tests." This record has no schema, digest or signature. The only other code is the smoke extension, which is a tool, not a record.

The switch week is the part no agent can do. An agent tester proves that each feature works the way its issue says. Only the operator can say whether the whole is good enough to use instead of Claude Code for a week, and the roadmap asks for exactly that. So this unit defines the week as concrete, countable things (days, hours, uses of each parity line, fallbacks to Claude Code, defects and their fate), and it keeps the verdict the operator's: an agent copies the operator's words into the evidence and never decides an operator row.

## 2. Facts

### 2.1 The `herdr` command lines

The facts H1 to H14 of [P1-13](P1-13.md#21-the-herdr-command-line-verified-by-running-it-herdr-094-macos-2026-10-22) apply unchanged (Herdr 0.9.4, checked by running it on macOS on 2026-10-22; re-check with the Herdr in use and write `herdr --version` in the record header). Three bear on this unit: `pane run` types text and Enter together; `pane read` has sources `visible`, `recent` and `recent-unwrapped`; a character sent right after `esc` may be lost, so wait 400 ms after `send-keys esc`. Whether `visible` or `recent-unwrapped` is stale for a full-screen program is P1-13's H14 **verify** item, settled by its first live run and recorded in its journal entry: read that entry before recording screens, and follow it. The smoke suite reads the source section 9.3 names for each step; the manual rows (XF) run `echo READ-CHECK` in the pane first and use the source that shows it. If P1-13's journal entry records that a source lags for Talaria, stop in stage 0 and report it on P1-13 before any screen is recorded. A suite step never stops the run for a lagging read: it FAILS with its reason and the pane text (a lagging source is then a non-defect that section 8.1 item 2 lets the lead rerun once). A manual row whose `READ-CHECK` shows that a source lags uses the other source and says so in the evidence. Never swap a source silently to make a step pass.

New facts for this unit, and how each was checked:

| # | Fact | How checked |
|---|---|---|
| S1 | The Shift+Tab key is sent with `herdr pane send-keys <pane> shift+tab`; if Herdr rejects that name, `backtab` is the second name to try | `P2-16`'s scenarios (its U6 item); not verified here: the first live run records which name Herdr accepts |
| S2 | Alt+P is the default key of the model picker (`model_picker`, `KeyContext::Composer`) | `P2-06` REQ-1; the key name Herdr accepts for it (`alt+p`) is verified by the first live run. If Herdr rejects `alt+p`, XF-2 is `BLOCKED (environment: Herdr rejected alt+p)` and no other name is invented; the operator's mid-turn switch in OP-4 uses a real keyboard and is not affected; the operator may drop XF-2 before the freeze (section 8.1 item 4) |
| S3 | `talaria -p` prints its answer on standard output, writes nothing on standard error on success, and exits 0; with no gateway it prints one JSON line with `"status":"not_started"` and `"exit_code":4` and exits 4 | `P2-14` UAT-1 and UAT-5 |
| S4 | `herdr pane process-info --pane <pane>` prints JSON with `.result.process_info.foreground_processes[]`, each with `cmdline` and `pid` | P1-13 spec fact H6, used the same way by the idle-cost rows of `PH1-ACC` and `PH2A-ACC` |

### 2.2 Hermes facts used (read at Hermes v0.22.4; re-check at the release in `crates/hermes-contract/PINNED` before relying on them)

- `approvals.mode` is `manual`, `smart` or `off`; the default is `smart` and the default `timeout` is 300 s (`hermes_cli/config_defaults.py` lines 1694 and 1695 at Hermes v0.22.4, read again while drafting). `smart` lets a guardian model approve a flagged command with no card; a hosted-room policy overrides the configured mode (`tools/approval_context.py` lines 263 to 302).
- `rm -rf .pytest_cache` and `rm -r ./folder` match the recursive-delete pattern `\brm\s+-[^\s]*r` (`tools/approval_detection.py` line 230 at Hermes v0.22.4, read again while drafting).
- An unanswered approval is denied after `approvals.timeout` (default 300 s).
- `prompt.submit` accepts `unattended` only together with `finite: true`, and then auto-approves the turn (`gateway/session_finite.py`, function `admit_finite` and `finite_turn_scope`, at Hermes v0.22.4; `tools/approval.py` lines 327 to 333, `_yolo_active`, treats such a turn as a bypass). Nothing in this unit sends it.
- The session bypass (`yolo`) is set with `config.set` key `yolo` (`tui_gateway/methods_config_set.py` line 281, `_set_yolo`, at Hermes v0.22.4). `P2-16` owns what Talaria sends; this unit only observes the result (XF-6).
- Hermes's `shell.exec` runs in the session policy's folder (`gateway/session_shell.py` line 54, `_run(command, policy.cwd)`), `complete.path` lists the same policy's folder (`gateway/session_complete_path.py` line 37), and `session.mutate` operation `workspace` replaces that folder in the policy (`gateway/session_mutation_policy.py` lines 37 to 55, `prepare_workspace`), all at Hermes v0.22.4. **Not verified** is the live adapter applying the new policy at once (`gateway/session_policy.py` line 1001, `adapter.policies.get`) and `P2-05` REQ-3's own wording that the `!` route runs "in the session's launch folder". XF-7 finds out; section 10.2 says what happens if the answer is "the old folder".
- `/quit` in Talaria detaches and sends no `end_reason`; the session keeps running in the gateway (P1-12 settled decision 1).

### 2.3 What the source issues provide

Screen texts are each owned by one source issue and are listed with their sources in section 9.5. The demos and ignored live tests are named in section 7.1. Two behaviours the scenarios rely on and that are not Hermes facts: Ctrl+C clears a non-empty draft (P1-08 settled decision 1) and Esc interrupts a running turn (P1-08).

## 3. Freezing and building the candidate

The smoke extension (section 9) is built and merged first, on branch `p2-acc-smoke`, by a test author and a developer; it is part of the candidate. The lead freezes after the eight children, `PH2A-ACC`, P1-13 and the extension are merged and closed. Never switch the primary checkout (it may hold untracked material); work in a worktree outside the repository.

```bash
git fetch origin
CANDIDATE=$(git rev-parse origin/main)                    # 40 hex characters; written in the record header
WT=$(mktemp -d)/p2-acc                                     # a new folder outside the repository
git worktree add "$WT" "$CANDIDATE" && cd "$WT"
git push origin "$CANDIDATE:refs/heads/p2-acc-candidate"   # for the bench runs of AUTO-1; deleted at closeout
cargo build --release --workspace --examples
cargo build --release -p talaria-cli -p talaria-bench
shasum -a 256 target/release/talaria                       # the binary every agent row uses, and the one the operator's week uses
git switch -c p2-acc-record                                # the record branch starts at the candidate commit
```

The record branch holds documentation commits only (AC-6 proves it). After a new candidate (a fix) the branch is rebased onto it. `main` may move during the run: the record is compared with the candidate, never with `main`. The operator's week uses the same binary (REQ-13): the lead gives the operator the path of a copy of `target/release/talaria` and its checksum outside the issue; the checksum goes in the AUTO-1 evidence and in the week log (OP-8).

Shell variables do not cross Herdr panes. At freeze the lead writes the common settings to a file next to the worktree, and every pane starts in the worktree root (`--cwd`) and runs `. ../acc.env` first. The file's location is never written in the issue, the record or the evidence.

```bash
cat > ../acc.env <<EOF2
CANDIDATE=$CANDIDATE
T=$PWD/target/release/talaria
F='--no-ensure --profile fixture --provider ollama-cloud --model minimax-m3.1-flash'
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
```

`--no-ensure` is on every launch that starts a live session an agent makes, so an agent can never start a gateway (the operator's own week may omit it, REQ-13). Preflight before stage 1 (once, with the Herdr in use): `herdr --version`, `tmux -V` (the phase 2a suite of AUTO-9 needs it); run both pane-split forms (the source scenarios' `herdr pane split --current --direction right --cwd . --no-focus` and the helper's) and close the panes they make. A form Herdr rejects is reported on the issue and allowed by REQ-11 change (4). Also run `herdr pane wait-output --help`: the XF rows pass `--source` and a `(?m)` regular expression, which P1-13's fact H5 does not list; when Herdr rejects either, the row polls `pane read` once a second for up to its timeout instead and says so in the evidence. If a code path changes after this point the candidate is dead: start again from the top of this section.

## 4. The record

File `docs/acceptance/v0.7.0/phase-2.md`. The first commit holds this skeleton with every header line filled in with its real value (the checker refuses a header that still holds an angle-bracket placeholder; the values below are examples) and `phase-2-evidence.md` holding only its title line, so the first run of the real checker is the red run: 79 problems. Commit order: the checker tests with the stub (`test(acceptance): ...`), the empty record (`docs(acceptance): add the empty phase 2 record`), then the real checker (`docs(acceptance): add the phase 2 checker`); at the empty-record commit the stub still prints `FAILED (stub)`, so the 79-problem run is taken at the checker commit, while the record is still all NOT RUN. Row order is the order below; do not add other tables. The folder also holds the phase 1 and phase 2a records (`phase-1*`, `check-record*`, `phase-2a*`, `check-phase-2a*`); this unit never edits them.


```markdown
# Phase 2 acceptance record

Candidate: 0123456789abcdef0123456789abcdef01234567
Hermes: v0.22.4
Herdr: herdr 0.9.4
OS: macos
Date: 2026-11-02

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
| P2-06 UAT-1 | developer | NOT RUN |  |  |
| P2-06 UAT-2 | developer | NOT RUN |  |  |
| P2-06 UAT-3 | developer | NOT RUN |  |  |
| P2-06 UAT-4 | developer | NOT RUN |  |  |
| P2-06 UAT-5 | developer | NOT RUN |  |  |
| P2-06 UAT-6 | developer | NOT RUN |  |  |
| P2-07 UAT-1 | fleet-lead | NOT RUN |  |  |
| P2-07 UAT-2 | developer | NOT RUN |  |  |
| P2-07 UAT-3 | developer | NOT RUN |  |  |
| P2-07 UAT-4 | developer | NOT RUN |  |  |
| P2-07 UAT-5 | developer | NOT RUN |  |  |
| P2-07 UAT-6 | developer | NOT RUN |  |  |
| P2-08 UAT-1 | developer | NOT RUN |  |  |
| P2-08 UAT-2 | developer | NOT RUN |  |  |
| P2-08 UAT-3 | developer | NOT RUN |  |  |
| P2-08 UAT-4 | developer | NOT RUN |  |  |
| P2-08 UAT-5 | developer | NOT RUN |  |  |
| P2-08 UAT-6 | developer | NOT RUN |  |  |
| P2-08 UAT-7 | developer | NOT RUN |  |  |
| P2-09 UAT-1 | developer | NOT RUN |  |  |
| P2-09 UAT-2 | developer | NOT RUN |  |  |
| P2-09 UAT-3 | developer | NOT RUN |  |  |
| P2-09 UAT-4 | developer | NOT RUN |  |  |
| P2-09 UAT-5 | developer | NOT RUN |  |  |
| P2-10 UAT-1 | developer | NOT RUN |  |  |
| P2-10 UAT-2 | developer | NOT RUN |  |  |
| P2-10 UAT-3 | developer | NOT RUN |  |  |
| P2-10 UAT-4 | developer | NOT RUN |  |  |
| P2-10 UAT-5 | developer | NOT RUN |  |  |
| P2-10 UAT-6 | developer | NOT RUN |  |  |
| P2-14 UAT-1 | scripter | NOT RUN |  |  |
| P2-14 UAT-2 | scripter | NOT RUN |  |  |
| P2-14 UAT-3 | scripter | NOT RUN |  |  |
| P2-14 UAT-4 | scripter | NOT RUN |  |  |
| P2-14 UAT-5 | scripter | NOT RUN |  |  |
| P2-14 UAT-6 | scripter | NOT RUN |  |  |
| P2-14 UAT-7 | scripter | NOT RUN |  |  |
| P2-15 UAT-1 | developer | NOT RUN |  |  |
| P2-15 UAT-2 | developer | NOT RUN |  |  |
| P2-15 UAT-3 | developer | NOT RUN |  |  |
| P2-15 UAT-4 | developer | NOT RUN |  |  |
| P2-15 UAT-5 | developer | NOT RUN |  |  |
| P2-16 UAT-1 | developer | NOT RUN |  |  |
| P2-16 UAT-2 | developer | NOT RUN |  |  |
| P2-16 UAT-3 | developer | NOT RUN |  |  |
| P2-16 UAT-4 | configurer | NOT RUN |  |  |
| P2-16 UAT-5 | developer | NOT RUN |  |  |
| P2-16 UAT-6 | developer | NOT RUN |  |  |
| XF-1 | developer | NOT RUN |  |  |
| XF-2 | developer | NOT RUN |  |  |
| XF-3 | developer | NOT RUN |  |  |
| XF-4 | developer | NOT RUN |  |  |
| XF-5 | scripter | NOT RUN |  |  |
| XF-6 | developer | NOT RUN |  |  |
| XF-7 | developer | NOT RUN |  |  |
| XF-8 | developer | NOT RUN |  |  |
| XF-9 | developer | NOT RUN |  |  |
| XF-10 | developer | NOT RUN |  |  |
| XF-11 | developer | NOT RUN |  |  |
| XF-12 | developer | NOT RUN |  |  |
| OP-1 | developer | NOT RUN |  |  |
| OP-2 | developer | NOT RUN |  |  |
| OP-3 | developer | NOT RUN |  |  |
| OP-4 | developer | NOT RUN |  |  |
| OP-5 | developer | NOT RUN |  |  |
| OP-6 | developer | NOT RUN |  |  |
| OP-7 | developer | NOT RUN |  |  |
| OP-8 | developer | NOT RUN |  |  |
```


Rules: `result` is exactly `PASS`, `FAIL`, `BLOCKED (<cause>)` (causes: `environment`, `not built`, `no access`) or `NOT RUN`. A PASS row carries the first 7 or more characters of the candidate in `commit` and the link `phase-2-evidence.md#<id lower case, spaces to hyphens>` in `evidence`; the row `P2-06 UAT-1` links `phase-2-evidence.md#p2-06-uat-1`. A row that is not PASS may leave `evidence` and `commit` empty but its evidence section should say why. BLOCKED and NOT RUN never count as a pass; there is no waiver state. Personas and runners are those of the source scenarios (Appendix A); AUTO rows are `builder`; the XF rows are `developer` except `XF-5` (`scripter`); the OP rows are `developer`.

## 5. The evidence file

### 5.1 Shape

`docs/acceptance/v0.7.0/phase-2-evidence.md` starts with `# Phase 2 acceptance evidence` and holds one section per row, in record order:

````markdown
## P2-06 UAT-1

Candidate: <first 7 characters>. Runner: agent tester. Run 1 of 1. Date: 2026-11-02.

```text
<scrubbed output or pane text, at most 60 lines>
```
````

The checker requires, for every PASS row, the `Candidate:` line (7 or more hex characters equal to the start of the candidate, then a full stop or space), exactly one section, and a fenced block with at least one non-blank line and at most 60 lines. The reviewer checks what the script cannot: the runner and date line, the run list, the three-run limit, the operator's comment URL, the row order and persona column, and that every row that is not PASS says why. An operator row holds the operator's words (what they did, the terminals, the counts, the result), the URL of the operator's comment on the issue and, for `OP-8`, the 24 matrix lines and the verdict line.

### 5.2 What to capture, per row kind

AUTO rows: the command and the last 15 lines of its output (the result lines). Source rows: what the source scenario's Evidence line asks for. XF rows: what section 7.3 names. Smoke rows (AUTO-8 to AUTO-11): the whole output including every step line and the `SMOKE_RESULT:` line, with repeated `NOTE` lines removed (the only lines ever removed), and both tab lists. Paste the `exit=<n>` line right after the output, because AC-11 reads it. A smoke section holds the output of the last run only: earlier runs are described in words and no earlier line starts with `PASS `, `SMOKE_RESULT:` or `exit=`. When the scrubbed output is still over 60 lines, write it as consecutive fenced blocks of at most 60 lines each in the same section, in order, none cut in the middle of a line: the checker measures each block, not the section (6.2: `blk` restarts at every opening fence), and AC-11 reads the `SMOKE_RESULT:` line from whichever block holds it. Screenshots are attached to the operator's comment, not committed.

### 5.3 Scrubbing (before anything is pasted)

```bash
scrub() {
  sed -e "s#$HOME#<home>#g" -e "s#$(id -un)#<user>#g" \
      -e "s#${TMPDIR%/}#<tmp>#g" -e "s#$(cd "${TMPDIR:-/tmp}" && pwd -P)#<tmp>#g" \
      -e "s#$PWD#<worktree>#g"
}
herdr pane read "$PANE" --source recent-unwrapped --lines 200 | scrub > "$X/evidence.txt"
```

Then read the result: no home directory, user name (a login name under four characters cannot be scrubbed safely: say so in the section), temporary path, host name, provider key or ticket, and no profile name other than `default` or `fixture`; replace any other with `<profile>`. Shapes that are new in this unit: the session picker lists sessions of every profile the gateway serves, so a pane read of `/resume` can show titles and profile names that are not the tester's (the source scenarios say to filter on `fixture` and count the `default` rows; do that, and cut any other row); the `/export` notice and `ls -l` show file names and the folder (replace the folder); `/cd` notices show absolute folders (replace the temporary prefix with `<tmp>`); the operator's week log names repositories and branches (replace them with `<repo-1>`, `<repo-2>`, and say nothing about their content). The checker catches paths only; AC-12 also greps `--profile` values and each runner's login name. A clean grep proves only that those shapes are absent: the reading step is the control. The operator's comments are public on this issue: the operator scrubs them the same way before posting (no profile other than `default` or `fixture`, no path, no repository name, no host name). An agent that copies one reads it first, and if it leaks asks the operator to edit the comment and copies nothing until it is clean.

## 6. The checker and its tests

### 6.1 Behaviour

`check-phase-2.sh [record [evidence]]` (defaults: `phase-2.md` and `phase-2-evidence.md` next to the script). It holds the 79 row ids in one `IDS=` line. It prints one line per problem, then `ACCEPTANCE_RESULT: PASSED (<n> of 79 rows PASS, candidate <7 hex>)` or `ACCEPTANCE_RESULT: FAILED (<p> problems, <n> of 79 rows PASS)`. Exit 0 when there are no problems, 1 otherwise, 2 when a file is missing. Problems: a row that is not PASS (the line is `<id>: <result>`); a result that is not one of the four forms; a PASS row whose commit is not a prefix of the candidate or is shorter than 7 characters; a PASS row whose evidence link is not the expected one, whose section is missing, empty or longer than 60 lines, or does not name the candidate; a missing, extra or duplicate row; a section that appears twice; a header line that is missing or still a placeholder; a local path anywhere in either file.

The script is a copy of `PH2A-ACC`'s checker (its spec 6.2), which is a copy of `PH1-ACC`'s, with the file names, the id list and the count changed, so the three stay comparable. Rejected: one parametrised script for all three phases, because it would reopen two closed gates' tools; the cost of the copy is about 55 lines. If a command-saving proxy wraps shell commands on the machine, run the checker and its tests through the proxy's pass-through mode or in a plain pane and paste the raw output.

### 6.2 Reference implementation

Written for POSIX `sh` and `awk`; no brace counts in regular expressions; the backtick fence written `\140\140\140`. Exercised while drafting on macOS (awk version 20200816): the 22 cases below pass, and the skeleton record of section 4 ends `FAILED (79 problems, 0 of 79 rows PASS)` and exits 1. Not run with GNU `awk` or `mawk`: run the cases on a Linux machine before recording `OS: linux`.

```sh
#!/bin/sh
# Check the Phase 2 acceptance record. Usage: check-phase-2.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-2.md}
EVI=${2:-$DIR/phase-2-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-phase-2: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|AUTO-8|AUTO-9|AUTO-10|AUTO-11|P2-06 UAT-1|P2-06 UAT-2|P2-06 UAT-3|P2-06 UAT-4|P2-06 UAT-5|P2-06 UAT-6|P2-07 UAT-1|P2-07 UAT-2|P2-07 UAT-3|P2-07 UAT-4|P2-07 UAT-5|P2-07 UAT-6|P2-08 UAT-1|P2-08 UAT-2|P2-08 UAT-3|P2-08 UAT-4|P2-08 UAT-5|P2-08 UAT-6|P2-08 UAT-7|P2-09 UAT-1|P2-09 UAT-2|P2-09 UAT-3|P2-09 UAT-4|P2-09 UAT-5|P2-10 UAT-1|P2-10 UAT-2|P2-10 UAT-3|P2-10 UAT-4|P2-10 UAT-5|P2-10 UAT-6|P2-14 UAT-1|P2-14 UAT-2|P2-14 UAT-3|P2-14 UAT-4|P2-14 UAT-5|P2-14 UAT-6|P2-14 UAT-7|P2-15 UAT-1|P2-15 UAT-2|P2-15 UAT-3|P2-15 UAT-4|P2-15 UAT-5|P2-16 UAT-1|P2-16 UAT-2|P2-16 UAT-3|P2-16 UAT-4|P2-16 UAT-5|P2-16 UAT-6|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|XF-9|XF-10|XF-11|XF-12|OP-1|OP-2|OP-3|OP-4|OP-5|OP-6|OP-7|OP-8'
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
  if (evid != "phase-2-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-2-evidence.md#" slug(id))
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

The test script builds a good record and variations of it in a temporary folder, runs the checker on each and compares the exit code and one expected text. It prints `CASE <name>: ok` or `CASE <name>: FAILED (...)` and a final `CHECK_RECORD_TESTS: PASSED` or `CHECK_RECORD_TESTS: FAILED (<n>)`. It reads the id list out of the checker, so the two cannot drift; `all-pass` expects the text `79 of 79`, so a dropped id fails it.

| # | Case | Builds | Expects (exit; text) |
|---|---|---|---|
| 1 | `all-pass` | 79 PASS rows, a section each | 0; `ACCEPTANCE_RESULT: PASSED (79 of 79 rows PASS, candidate <7 hex>)` |
| 2 | `not-run-fails` | `P2-16 UAT-6` NOT RUN | 1; `P2-16 UAT-6: NOT RUN` |
| 3 | `blocked-fails` | `AUTO-5` `BLOCKED (environment)` | 1; `AUTO-5: BLOCKED (environment)` |
| 4 | `fail-fails` | `XF-2` FAIL | 1; `XF-2: FAIL` |
| 5 | `wrong-commit` | `P2-07 UAT-2` commit `deadbeefdead` | 1; `is not the candidate` |
| 6 | `lower-case-result` | `AUTO-1` result `pass` | 1; `the result must be` |
| 7 | `row-missing` | `OP-8` deleted | 1; `OP-8: row missing` |
| 8 | `unexpected-row` | an added row `ZZ-9` | 1; `unexpected row: ZZ-9` |
| 9 | `duplicate-row` | `AUTO-1` twice | 1; `AUTO-1: appears twice` |
| 10 | `no-candidate` | `Candidate: TBD` | 1; `Candidate: <40 hex` |
| 11 | `no-evidence-section` | no section for `P2-10 UAT-6` | 1; `P2-10 UAT-6: the evidence file has no section` |
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

The checker ignores heading-like lines inside a fenced block and counts a duplicated section; `XF-6`'s block in case 1 starts with a line that looks like a heading to prove it. This is `PH1-ACC`'s spec 6.2 and `PH2A-ACC`'s, so the three agree; if they ever disagree, report it on `PH1-ACC`; this unit follows the script.

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
# Tests for check-phase-2.sh. Run: sh docs/acceptance/v0.7.0/check-phase-2.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-phase-2.sh"
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
      echo "| $id | developer | $res | phase-2-evidence.md#$slug | $com |"
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
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (79 of 79 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "P2-16 UAT-6" "NOT RUN";               expect not-run-fails 1 "P2-16 UAT-6: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-5" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-5: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P2-07 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| OP-8' "$T/good.md" > "$T/f.md";            expect row-missing 1 "OP-8: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P2-10 UAT-6";               expect no-evidence-section 1 "P2-10 UAT-6: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-2-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
{ cat "$T/ev.md"; printf 'see /%s/someone/project\n' Users; } > "$T/ev3.md"; expect local-path 1 "a local path appears" "$T/good.md" "$T/ev3.md"
expect missing-file 2 "not found" "$T/nope.md" "$T/ev.md"
make_evidence "$T/ev4.md" "AUTO-2"; section AUTO-2 "" >> "$T/ev4.md"; expect empty-evidence-block 1 "AUTO-2: the evidence file has no section" "$T/good.md" "$T/ev4.md"
make_evidence "$T/ev5.md" "AUTO-2"; section AUTO-2 "$(seq 1 61)" >> "$T/ev5.md"; expect long-evidence-block 1 "AUTO-2: a fenced block of the evidence is longer than 60 lines" "$T/good.md" "$T/ev5.md"
make_record "$T/k.md" "AUTO-3" PASS 012345;            expect short-commit 1 "AUTO-3: commit 012345 is not the candidate" "$T/k.md" "$T/ev.md"
{ cat "$T/good.md"; printf 'see /%s/someone/project\n' tmp; } > "$T/l.md"; expect local-path-in-record 1 "a local path appears" "$T/l.md" "$T/ev.md"
sed 's/^Hermes: .*/Hermes: <pinned release tag>/' "$T/good.md" > "$T/m.md"; expect placeholder-header 1 "Hermes: the header line is still a placeholder" "$T/m.md" "$T/ev.md"
sed 's/^Candidate: 0123456\./Candidate: deadbee./' "$T/ev.md" > "$T/ev6.md"; expect evidence-names-another-candidate 1 "AUTO-1: the evidence section must name the candidate" "$T/good.md" "$T/ev6.md"
make_evidence "$T/ev7.md" none; section AUTO-2 ok >> "$T/ev7.md"; expect duplicate-section 1 "AUTO-2: the evidence file has 2 sections for it" "$T/good.md" "$T/ev7.md"
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-2.md" && cp "$T/ev.md" "$T/d/phase-2-evidence.md"
out=$(sh "$T/d/check-phase-2.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
````

The red run of the record, taken at the commit that adds the real checker while the committed record is still the skeleton of section 4 with the real candidate and real header lines: `sh check-phase-2.sh` must end `ACCEPTANCE_RESULT: FAILED (79 problems, 0 of 79 rows PASS)` and exit 1 (checked while drafting).


## 7. The scenario catalogue

Every row has an id, a persona, a runner, steps, an observable result and the evidence to capture. Runner words: `agent tester` is an agent working in a Herdr pane; `operator` is the human. All commands run at the worktree root unless a row says otherwise. `R=infiquetra/talaria`, `X=$(mktemp -d)` for evidence scratch.

### 7.1 AUTO rows (persona builder; agent tester)

Each row lists its commands, what makes it PASS and what to paste as evidence. A test target that does not exist at the candidate is `BLOCKED (not built)`, never skipped.

**AUTO-1: project check and the candidate's continuous integration (CI) results.** The `bench` workflow runs on `pull_request` and `workflow_dispatch` (P1-11 REQ-20). GitHub attaches every workflow run to its head commit as check runs, so a dispatch run on the candidate branch should report on the candidate; this is **verified at the first use** (if it does not, the row is BLOCKED (environment) and reported on P1-11). The lead can learn the answer before the freeze: in stage 0 dispatch `bench.yml` on the branch `p2-acc-smoke` and read `gh api "repos/$R/commits/<its head sha>/check-runs"`; if no `bench` check run is attached, report it on P1-11 then, so the cause is fixed before the candidate exists (a workflow fix after the freeze makes a new candidate).

```bash
gh workflow run bench.yml --ref p2-acc-candidate -R "$R"
# poll every 10 s, at most 5 minutes, until a run whose headSha is the candidate exists (an older run on the branch does not count)
RUN=$(gh run list -R "$R" --workflow bench.yml --branch p2-acc-candidate --json databaseId,headSha --jq ".[] | select(.headSha == \"$CANDIDATE\") | .databaseId" | head -1)
gh run watch "$RUN" -R "$R" --exit-status
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
gh api --paginate "repos/$R/commits/$CANDIDATE/check-runs" --jq '.check_runs[] | "\(.name) \(.conclusion)"'
shasum -a 256 target/release/talaria
```

PASS: `gh run watch` exits 0 on the run for the candidate; the cargo set exits 0; every check-run line ends `success` or `skipped` (a line ending `null` is still running: wait and read again; `failure`, `cancelled`, `timed_out` and `action_required` are a FAIL) and `bench success` is among them (`bench` is the job name of P1-11's workflow: stage 0 reads the real name from the dispatch run, and the lead edits this row and AUTO-1 of the issue if it differs). Evidence: run id and `headSha`, exit codes, the check-run lines, the checksum.

**AUTO-2: replay suites** (tests that play recorded Hermes frames back). The eight phase 2b replay targets, plus the phase 2a replay targets again (a phase 2b change can break them).

```bash
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

**AUTO-3: the eight issues' unit suites** (every test target the issues name, except the replay and live ones), then the whole workspace once more.

```bash
cargo test -p talaria-core --test mutation_fences --test model_effort --test model_catalog --test model_picker --test model_pick --test model_saved
cargo test -p talaria-tui --test model_picker_render
cargo test -p talaria-app --test saved_default_launch --test model_service --test mutation_client
cargo test -p talaria-core --test directory_rows --test directory_title --test directory_export --test picker_state
cargo test -p talaria-app --test directory_wire --test switch_flow --test mutate_wire --test export_flow --test session_picker_keys
cargo test -p talaria-tui --test session_picker_draw
cargo test -p hermes-contract --test canonical_session_info
cargo test -p talaria-core --test queue_state --test queue_reorder --test queue_intents --test session_queue
cargo test -p talaria-app --test queue_service
cargo test -p talaria-tui --test queue_preview --test queue_keys
cargo test -p talaria-core --test rewind_turns --test rewind_checkpoints --test rewind_command --test rewind_text --test rewind_state
cargo test -p talaria-tui --test rewind_render
cargo test -p talaria-app --test rewind_service
cargo test -p talaria-core --test context_breakdown --test context_usage --test context_compact --test context_tracker
cargo test -p talaria-tui --test context_render
cargo test -p talaria-app --test context_wiring
cargo test -p talaria-app -p talaria-cli
cargo test -p talaria-core --test automation_snapshot --test automation_lines --test automation_reduce --test automation_present --test automation_fold
cargo test -p talaria-tui --test automation_render
cargo test -p talaria-app --test automation_service
cargo test -p talaria-core --test approval_mode --test approval_segment --test permissions_state --test permissions_command --test cd_target --test cd_state
cargo test -p talaria-app --test permissions_service --test workspace_service --test permissions_app
cargo test -p talaria-tui
cargo test --workspace
```

PASS: every command exits 0, and a command naming k `--test` targets prints at least k lines `test result: ok. N passed` with N at least 1 (a filter-only command prints at least one). The phase 2a unit suites run inside `cargo test --workspace` and are not repeated by name. The tests marked `#[ignore]` are run by AUTO-4, AUTO-5 and AUTO-7 or by the operator. Evidence: the `test result` lines and exit codes.

**AUTO-4: performance budgets on the real release binary and the units' own benchmarks.**

```bash
target/release/talaria-bench run --binary target/release/talaria --report "$X/bench.json" > "$X/bench.txt"; echo "exit=$?"
grep -cE ' FAIL$' "$X/bench.txt"; grep -cE ' PASS$' "$X/bench.txt"
cargo test -p talaria-bench --test real_binary release_budgets_hold -- --ignored
cargo bench -p talaria-core --bench model_filter
cargo bench -p talaria-tui --bench session_picker
cargo bench -p talaria-tui --bench tool_cells
cargo bench -p talaria-tui --bench pager
cargo bench -p talaria-core --bench palette_filter
cargo test --release -p talaria-app --test attachments_budget -- --ignored
```

PASS: `exit=0`, the FAIL count 0, the PASS count 1 or more; the ignored test passes; the five benches each exit 0 (`model_filter` prints `p95_us` under 2000, `palette_filter` prints `p95_us` under 2000 at 2,000 entries, the `pager` table has every row `pass`; `session_picker` exits 0 by its own budget rule); `attachments_budget` passes. Evidence: the `bench.txt` table from its `os:` line, the two counts, the `p95_us` lines, the exit codes. Budgets: [05-architecture.md](../05-architecture.md#performance-budgets); the units' own: P2-06 AC-10, P2-07 REQ-26, and for phase 2a the figures in `PH2A-ACC` spec 7.1.

**AUTO-5: live suites with no skip.** Operator preconditions hold (issue, Dependencies); each command runs in its own shell.

```bash
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_model -- --ignored --nocapture 2>&1 | tee "$X/l1.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_session_picker -- --ignored --nocapture 2>&1 | tee "$X/l2.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_queue -- --ignored --nocapture 2>&1 | tee "$X/l3.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_rewind -- --ignored --nocapture 2>&1 | tee "$X/l4.txt"
LIVE=$(mktemp -d)
cargo run -q -p record-fixture -- record --out "$LIVE" --scenario context-compaction --profile fixture --model minimax-m3.1-flash --provider ollama-cloud --attempt-timeout-s 900
TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_FRAMES="$LIVE" cargo test -p talaria-app --test live_context -- --ignored --nocapture 2>&1 | tee "$X/l5.txt"
TALARIA_LIVE_REQUIRE=1 cargo test --release -p talaria-cli --test live_print -- --ignored --nocapture 2>&1 | tee "$X/l6.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_automation -- --ignored --nocapture 2>&1 | tee "$X/l7.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_permissions -- --ignored --nocapture 2>&1 | tee "$X/l8.txt"
```

PASS (each test has its own signal): `LIVE_MODEL_RESULT: PASSED` once in `l1.txt`; `LIVE_SESSION_PICKER_RESULT: PASSED` five times in `l2.txt` (`picker_open_median_ms` is printed there: `P2-07 UAT-1` reads it); `LIVE_QUEUE_RESULT: PASSED` three times in `l3.txt`; `LIVE_REWIND_RESULT: PASSED` once; `LIVE_CONTEXT_RESULT: PASSED` once, with the line `LIVE_CONTEXT_STATS_AFTER_COMPACTION: api_calls=<n>`; `LIVE_PRINT_RESULT: PASSED` five times; `LIVE_AUTOMATION_RESULT: PASSED` once; `LIVE_PERMISSIONS_RESULT: PASSED` once with `LIVE_PERMISSIONS_PROFILE_MODE: manual`; no `SKIPPED` in any file (`grep -c SKIPPED "$X"/l?.txt` prints 0 for each). A skip, or a recorder that exits 1 or 2, is `BLOCKED (environment)`, never a pass. Model-caused failures of `live_queue` follow `P2-08`'s own rule (its specification 13.17). Evidence: the result lines and the counts.

**AUTO-6: hygiene scans.**

```bash
cargo run -q -p record-fixture -- scan fixtures/frames
! git grep -nE 'unsafe *(\{|fn |impl )' -- crates tools
! git grep -nE '/(Users|home|private|var)/[A-Za-z]' -- crates tools docs/formats
! git grep -n 'unattended' -- tools/smoke ':!tools/smoke/tests/typed_input.rs' ':!tools/smoke/tests/typed_input2a.rs' ':!tools/smoke/tests/typed_input2b.rs' ':!tools/smoke/README.md'
! git grep -nE 'end_reason|unattended' -- crates/talaria-app/src crates/talaria-cli/src
git grep -nE 'config\.set' -- crates/talaria-app/src crates/talaria-cli/src
! git grep -nE 'legacy::session_(compress|usage)\b|"session\.compress"' -- crates/talaria-core/src crates/talaria-tui/src crates/talaria-app/src
! git grep -n '"session\.cwd\.set"' -- crates/talaria-app/src crates/talaria-core/src
! grep -n '"scope"' crates/talaria-app/src/permissions.rs
! grep -rlE '"unattended": *true' fixtures/frames
! grep -rnE '/(Users|home|private|var/folders|tmp)/[A-Za-z]' fixtures/frames/permissions-cd.jsonl fixtures/frames/rewind-and-rollback.jsonl crates/talaria-app/tests/data/headless/finite_turn.frames.jsonl
! git grep -hoE -e '--profile[ =]+"?[A-Za-z0-9_-]+' -- crates tools | sed -E 's/.*[ =]"?//' | grep -vxE 'default|fixture|nosuchprofile'
target/release/talaria --help | grep -cE -- '--output-format|--timeout|124'
grep -c '"exit_code"' docs/formats/print-result-v1.md
```

PASS: the `!` lines print nothing; the `config.set` line prints only hits each read and listed in the evidence with its owner: the `/skin <name>` write (`P2-12` REQ-16), the reasoning-effort write (`P2-06`), the `approvals.mode` and `yolo` writes (`P2-16`); any other hit is a FAIL (phase 1's scan forbade `config.set` in these folders; phases 2a and 2b each carve exactly the writes their issue owns); the help line prints 3 or more and the format line 1 or more. The fixtures named exist at the candidate (a missing file makes `grep` print an error: that line is `BLOCKED (not built)`). The `unattended` scan has no exclusion for source files: no file under `tools/smoke/src`, code or comment, holds the word (write "the auto-approve flag"). P1-13's and PH2A-ACC's scans hold the same rule, so no earlier source file contains it, and the `phase2b` files follow it; only the three typed-input test files and the README may. `nosuchprofile` is allowed in the profile scan because P1-01 and P1-12 type it as a deliberately unknown name. Evidence: the command lines that printed something, the counts, and the listed `config.set` hits.

**AUTO-7: smoke suite offline tests and the earlier acceptance tools.**

```bash
cargo test -p smoke
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$'
cargo run -q -p smoke -- run --help
sh docs/acceptance/v0.7.0/check-record.test.sh | tail -1
sh docs/acceptance/v0.7.0/check-phase-2a.test.sh | tail -1
sh docs/acceptance/v0.7.0/check-record.sh | tail -1
sh docs/acceptance/v0.7.0/check-phase-2a.sh | tail -1
```

PASS: all tests pass; the ignored list is exactly three lines naming `live_full_smoke_in_herdr`, `live_phase2a_smoke_in_herdr` and `live_phase2b_smoke_in_herdr`; the help lists `--talaria`, `--herdr`, `--profile`, `--model`, `--provider`, `--deadline-secs`, `--step-scale`, `--report-dir`, `--suite` and `--tmux`, and names the three suites; the two test scripts end `CHECK_RECORD_TESTS: PASSED` and the two checkers end `ACCEPTANCE_RESULT: PASSED (<n> of <n> rows PASS, candidate <7 hex>)` (the earlier records are for their own candidates and must still read as accepted: this proves this unit did not disturb them; if the operator ruled shape B for the earlier gates, the files do not exist and those four lines are dropped before the freeze by editing the issue). Evidence: the `test result` line, the ignored lines, the help, the four tail lines.

**AUTO-8, AUTO-9, AUTO-10, AUTO-11: the smoke suites, back to back at the end of stage 1** with no other live scenario between them (they share one gateway and one free model).

```bash
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria "$T" > "$X/s1.txt" 2>&1; echo "exit=$?"                          # AUTO-8
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2a --talaria "$T" > "$X/s2.txt" 2>&1; echo "exit=$?"      # AUTO-9
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2b --talaria "$T" > "$X/s3.txt" 2>&1; echo "exit=$?"      # AUTO-10
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2b --talaria "$T" > "$X/s4.txt" 2>&1; echo "exit=$?"      # AUTO-11
herdr tab list --workspace "$HERDR_WORKSPACE_ID"; tmux ls 2>&1 | grep -c talaria-smoke
```

PASS: AUTO-8 prints six `PASS` lines (P1-13's steps) and `SMOKE_RESULT: PASSED (elapsed <n> s)` with n under 300; AUTO-9 prints 21 `PASS` lines and the same result line with n under 1800 (`PH2A-ACC`'s suite, run once as a regression row); AUTO-10 and AUTO-11 each print 15 `PASS` lines and the same result line with n under 1500 (the deadline, section 9.6); every `exit=0`; no `talaria-smoke-` tab in the second list and the last count is 0. The defaults are P1-13's (profile `fixture`, model `minimax-m3.1-flash`, provider `ollama-cloud`), so no such flag is passed, and P1-13 makes the `--talaria` path absolute itself. A suite prints `PASSED WITH SKIPS` and exits 3 when a step that may skip does, and with `TALARIA_LIVE_REQUIRE=1` a skip fails the run; neither is a pass: the row is BLOCKED (environment), section 8.1 gives the label. The suites pass `--no-ensure` to `talaria` themselves. Evidence: each whole output (section 5.2) and both tab lists.


### 7.2 Source rows (48 scenarios)

Run each as written in Appendix A, with the four allowed changes of the issue's REQ-11:

1. `target/release/talaria` (the absolute `$T`) wherever a scenario says `talaria`, `<talaria>`, `<run>` or the debug path (`P2-10` writes `T=<checkout>/target/debug/talaria`: use the release binary instead; `P2-14` already uses the release build and a `PATH` entry: `command -v talaria` must print the worktree's `target/release/talaria`).
2. `--no-ensure` on every launch of `talaria` that starts a live session and lacks it (every live scenario's start line lacks it, and so does `P2-14`'s `-p` line except UAT-5, which has it).
3. Every `smoke run` that lacks `--talaria` gets `--talaria "$T"` (none of the 48 source scenarios runs `smoke`).
4. Where Herdr rejects the pane-split form a scenario writes, the helper `pane_new` of section 3; say so in the evidence.

These are not changes: cleanup before a scenario starts (close a leftover `talaria-smoke-` tab); the demo programs built by `cargo run` in debug (they are tools, not the product: `P2-10` UAT-6); a key name Herdr rejects for Shift+Tab when the scenario itself allows the second name (`P2-16`). A test target or demo that no longer exists is `BLOCKED (not built)`.

Order within an issue is the scenario number order, and the issues run in the order of the record (P2-06 to P2-10, P2-14, P2-15, P2-16). Dependencies and notes:

- `P2-06`: every scenario starts `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` in a Herdr pane at 120 by 40. UAT-2 needs a first plain `talaria --profile fixture` run to learn the profile default `<D>`; UAT-4 copies and restores `models.json` (the row is not PASS until the restore is shown: `cmp` silent, or the file absent as at the start). `<M2>` is chosen as the scenario says, once, and written in the evidence.
- `P2-07`: needs the gateway serving `default` and `fixture`. UAT-1 reads the `picker_open_median_ms` line printed by AUTO-5's `live_session_picker` run on this candidate (the scenario's "test 39a": cite that output). UAT-2 to UAT-6 build on each other (a fresh common setup before UAT-2; each later one starts where the one before ended): run them in order in one tag `<tag>`. Evidence never shows a `default` title: filter on `fixture` and record the `default` rows as a count (section 5.3).
- `P2-08`: every scenario uses a pane `P` split from the tester's pane, and the long prompt sleeps 25 s. A model that fails the sleep gets one rerun, then the operator (`P2-08` specification 7.2); UAT-4 needs the first `sleep` to be running when the keys are sent.
- `P2-09`: needs `checkpoints.enabled: true` on `fixture` and a git folder with `README.md` (an empty folder gets no first checkpoint). UAT-5 uses two panes on one session. If an approval card appears (not expected), deny it and record FAIL with its text.
- `P2-10`: UAT-1 to UAT-5 each use their own pane `P1` to `P5` and folder; UAT-3 takes up to 11 minutes and needs sixteen turns first; UAT-4 may need a retry counting to 1000 (the scenario says when). UAT-6 needs no gateway: it runs the demo program and reads `$TMPDIR/demo.txt`. `P2-10`'s precondition note naming the profile's `auxiliary.compression` provider and model must be on its issue; if it is absent, UAT-3 is `BLOCKED (environment)`.
- `P2-14`: run in `bash` after `cargo build --release -p talaria-cli`. UAT-2's `out.json` feeds UAT-4. UAT-6 needs `approvals.mode: manual` and `approvals.single_query_mode` at its default `deny` and a `.pytest_cache/x` in `<project>`; UAT-7 ends within 30 s by design.
- `P2-15`: UAT-3 changes the session's bypass: while a `⚠ YOLO` line shows, `/yolo` is sent until the OFF cell appears, even when the row FAILs. Never send a prompt while a `⚠ YOLO` line is visible. UAT-1 needs the goal judge (the profile's auxiliary model).
- `P2-16`: UAT-1 to UAT-3 and UAT-5 are agent rows in `$TMPDIR/p216/a` and `$TMPDIR/p216/b`; UAT-3 never repeats `/permissions off` (it must not confirm). Operator rows: UAT-4 (it changes the profile's approval mode to `smart` and back to `manual`, so it runs in stage 3, when no agent runs a live scenario) and UAT-6.

Operator rows among them: `P2-16 UAT-4` and `P2-16 UAT-6`. All other rows are run by the agent tester in stage 2. The operator posts one comment per operator row (section 7.4), and an agent copies it into the evidence.

### 7.3 Cross-feature rows XF-1 to XF-12 (agent tester)

These need the live gateway, profile `fixture`, and the common settings of section 3 (every pane starts with `. ../acc.env`). Each row uses its own folder `P=$(mktemp -d)` and its own panes, each made with `PANE=$(pane_new "$P")`. "Start Talaria in `$PANE`" means `herdr pane run "$PANE" "$T $F"` then `herdr pane wait-output "$PANE" --match minimax-m3.1-flash --source visible --timeout 60000`. Before starting Talaria in a pane type `echo READ-CHECK` there and confirm which of `visible` and `recent-unwrapped` shows it; use that source for the row (a read that lacks text just typed is stale and never evidence). "Submit" is the `submit` helper (two calls). `<M2>` is the first other Ollama Cloud model without `no effort` in the picker (`P2-06`'s rule), chosen once and written in the evidence. Prompts are never eased. If after 8 s nothing happened (a still screen, no card, no counting line), send `herdr pane send-keys "$PANE" enter` once and say so in the evidence. A model that fails a plain prompt makes a run fail without being a Talaria defect: rerun once and record both runs; a failing second run stays FAIL. Wait for a word with `herdr pane wait-output "$PANE" --match <word> --source recent-unwrapped --timeout 180000` and then for the screen to settle (two `vis` reads 3 s apart identical, at most 60 s).

Quoted texts below are the source issues' own; where a Then line of a source issue differs from a text here, the source issue wins and the difference is reported.

**XF-1: Every phase 2b command resolves in the palette as a Talaria command.** Covers `P2-03` with `P2-06` to `P2-10` and `P2-16` (the local command table; local wins over Hermes).
- Steps: start Talaria. For each name in `model effort resume rename branch export queue steer rewind undo retry rollback diff context compact usage permissions cd`: `herdr pane send-text "$PANE" "/$name"`; `sleep 1`; `vis "$PANE" > "$X/p-$name.txt"`; `herdr pane send-keys "$PANE" esc`; `sleep 0.4`; `herdr pane send-keys "$PANE" ctrl+c`. Never press Enter.
- Pass: for every name, `grep -c 'Tab complete' "$X/p-$name.txt"` prints 1 or more (the palette was open: an empty or stale read is not a pass), `grep -E "(^| )/$name( |$)" "$X/p-$name.txt" | grep -cw talaria` prints 1 or more (the row carries the tag `talaria`), and `grep -E "(^| )/$name( |$)" "$X/p-$name.txt" | grep -cw hermes` prints 0 (it is not a Hermes pass-through; `/rollback`, `/undo` and `/retry` are refused by the gateway's `slash.exec`, which is why `P2-09` handles them locally). 18 names, 18 reads.
- Evidence: the 18 row lines (the line holding `/<name>` and its tag).

**XF-2: A model change while a turn runs and a prompt waits.** Covers `P2-06` with `P2-08`. Needs two Ollama Cloud models (`P2-06`'s precondition); else BLOCKED (environment).
- Steps: start Talaria. `submit "$PANE" 'Run the shell command sleep 15, then reply with the word done in capital letters.'`; when the working indicator shows, `submit "$PANE" 'Reply with the word omega in capital letters.'` (it is queued); `herdr pane send-keys "$PANE" alt+p`; `sleep 1`; press `down` until the selected row reads `<M2>` (read after each key); `send-keys enter`; `sleep 1`; `vis "$PANE" > "$X/a.txt"`. Wait for `DONE`, then for `OMEGA`, and for the screen to settle; `vis "$PANE" > "$X/b.txt"`; `rec "$PANE" > "$X/r.txt"`.
- Pass: `a.txt` holds `Model will change to <M2> when the current turn ends.` and `queued (1)` and its status line still names `minimax-m3.1-flash`; `grep -cw DONE "$X/r.txt"` and `grep -cw OMEGA "$X/r.txt"` each print 1 or more; `grep -c 'Reply with the word omega' "$X/r.txt"` prints 1 (the queued prompt ran once, and was not sent twice); `b.txt` shows `<M2>` in its status line and no `queued (`. Which model answered the queued prompt is not asserted.
- Evidence: the key lines of the three reads and the counts.

**XF-3: Rewind is refused while a reply runs and a prompt waits; the queue is untouched; rewind works afterwards.** Covers `P2-09` with `P2-08`. Needs `checkpoints.enabled: true` on `fixture`.
- Steps: start Talaria in a git folder holding `README.md`. `submit "$PANE" 'Run the shell command sleep 20, then reply with the word first in capital letters.'`; when the working indicator shows, `submit "$PANE" 'Reply with the word omega in capital letters.'`; `vis "$PANE" > "$X/v0.txt"`; `submit "$PANE" '/rewind'`; `sleep 1`; `vis "$PANE" > "$X/v1.txt"`. Wait for `FIRST`, then `OMEGA`, and the screen to settle. `submit "$PANE" '/rewind'`; `sleep 1`; `vis "$PANE" > "$X/v2.txt"`; `herdr pane send-keys "$PANE" esc`; `sleep 0.4`; `vis "$PANE" > "$X/v3.txt"`; `rec "$PANE" > "$X/r.txt"`.
- Pass: `v0.txt` holds `queued (1)`; `v1.txt` holds `A reply is running. Stop it with Esc, then rewind.`, still holds `queued (1)` and no `Rewind · choose a prompt to go back to`; `grep -cw FIRST "$X/r.txt"` and `grep -cw OMEGA "$X/r.txt"` each print 1 or more (the turn was not interrupted and the queued prompt ran); `v2.txt` holds `Rewind · choose a prompt to go back to` with the rows `#2` and `#1`; `v3.txt` has no `Rewind · choose`.
- Evidence: the key lines of the four reads.

**XF-4: Rewinding a branch leaves the original conversation intact.** Covers `P2-07` with `P2-09`. Needs `checkpoints.enabled: true`. `<tag>` is a fresh run tag (`openssl rand -hex 2`).
- Steps: start Talaria. `submit "$PANE" 'Remember the code word heron. Reply ok.'`; `submit "$PANE" 'Now remember the code word lark instead. Reply ok.'`; wait until both replies finished and the screen settled. `submit "$PANE" "/rename fixture xf4 <tag>"`; `submit "$PANE" '/branch'`; `sleep 3`; `vis "$PANE" > "$X/a.txt"`. In the branch: `herdr pane send-keys "$PANE" esc`, `sleep 0.2`, `herdr pane send-keys "$PANE" esc`; `sleep 1`; press `enter` on `#2`, `c`, `enter`; `sleep 2`; `vis "$PANE" > "$X/b.txt"`; `send-keys ctrl+c` (clears the restored prompt). `submit "$PANE" '/resume'`; move with `j` (read after each key) until the selected row is `fixture xf4 <tag>` and press `enter`; wait for a line starting `── Resumed:`; `rec "$PANE" > "$X/c.txt"`; `submit "$PANE" 'Which code word should you remember? One word.'`; wait for the reply and the screen to settle; `rec "$PANE" > "$X/d.txt"`.
- Pass: `a.txt` holds `Branched into "fixture xf4 <tag> #2" with`; `b.txt` holds a line starting `── Rewound to before prompt #2`; in `c.txt`, the lines after the last `── Resumed:` hold `Now remember the code word lark instead` once (the original still has prompt 2); in `d.txt`, the lines after the last line holding `Which code word should you remember` contain the word `lark` (case-insensitive) and not `heron`. The rewind of the branch did not touch the original.
- Evidence: the key lines of the four reads (the `awk` that cuts "after the last line" is shown).

**XF-5: A headless session is continued and exported in the full screen program.** Covers `P2-14` with `P1-12`'s resume and `P2-07`'s export.
- Steps: `P=$(mktemp -d)`; in a shell pane at `$P`: `$T -p $F --output-format json "What is 1234 plus 4321? Reply with only the number." | tee "$X/out.json" | jq -r '.status, .text, .session_id'`; `SID=$(jq -r .session_id "$X/out.json")`. Start `$T --no-ensure --resume "$SID" --profile fixture` in a Talaria pane at `$P`; wait for a line starting `── Resumed:`; `rec "$PANE" > "$X/r.txt"`; `submit "$PANE" '/export'`; `sleep 3`; `vis "$PANE" > "$X/e.txt"`; `ls "$P"`.
- Pass: the `jq` prints `completed`, `5555` and a non-empty id; `grep -c 'What is 1234 plus 4321' "$X/r.txt"` prints 1 and `grep -cw 5555 "$X/r.txt"` prints 1 or more (the headless exchange shows once, as history); `e.txt` holds `Exported "` and `Markdown, secrets redacted)`; the exported file in `$P` holds `5555` (`grep -l 5555 "$P"/*` prints its name) and has mode `-rw-------`.
- Evidence: the `jq` output, the counts, the `Exported` line (folder replaced), the `ls -l` line.

**XF-6: The bypass is shown the same way by both features that draw it.** Covers `P2-15` (the `⚠ YOLO` line) with `P2-16` (the `approvals:` segment and Shift+Tab). Needs `approvals.mode: manual`. The lead records the `PH2B` planning stage's ruling on the owner of the bypass reading (`P2-15` decision 9 against `P2-16` decision 12) as a comment on `PH2B` before the freeze. With no ruling the Then lines below stand as written (both displays are required). If the ruling keeps only one display, the lead deletes the other half from this row by editing the issue before the freeze (section 8.1, item 4).
- Steps: start Talaria. `vis "$PANE" > "$X/s0.txt"`; `submit "$PANE" '/yolo'`; `sleep 5`; `vis "$PANE" > "$X/s1.txt"`; `herdr pane send-keys "$PANE" shift+tab` (the second name `backtab` if Herdr rejects it: say which worked); `sleep 5`; `vis "$PANE" > "$X/s2.txt"`. Cleanup, even on FAIL: while any read shows `⚠ YOLO ·` or `approvals: off`, send `shift+tab`, then `/yolo` once more, reading each time, until `s0`'s state is back.
- Pass: `s0.txt` holds `approvals: manual` and no `⚠ YOLO`; `s1.txt` holds `⚡ YOLO mode ON for this session`, the line `⚠ YOLO · commands run without approval in this session` and `approvals: off (this session)`; `s2.txt` holds `approvals: manual` and `Approvals back on for this session` and no `⚠ YOLO ·`; and in no read does `approvals: manual` appear together with `⚠ YOLO ·` (`grep -c` over each read that holds both prints 0).
- Evidence: the key lines of the three reads and the cleanup reads.

**XF-7: After `/cd`, a shell command and an `@` mention see the new folder.** Covers `P2-16` with `P2-05` and `P2-04`. **The expected result rests on `P2-16`'s notice** ("Its next command runs there") **and on Hermes applying the replaced folder to `!` and `@` at once** (the v0.22.4 code reads it, section 2.2; the live refresh is what this row confirms); a FAIL here is a finding, not a defect of the test (section 10.2).
- Steps: `A="$P/a"; B="$P/b"; mkdir -p "$A" "$B"; printf 'in b\n' > "$B/only-in-b.txt"`; both folders `git init -b main` with one empty commit. Start Talaria in `$A`. `submit "$PANE" '/cd ../b'`; wait for `This session now works in`; `submit "$PANE" '!pwd'`; `herdr pane wait-output "$PANE" --regex '(?m)^.*/b$' --source recent-unwrapped --timeout 15000`; `rec "$PANE" > "$X/s.txt"`; `herdr pane send-text "$PANE" '@only-in'`; `sleep 1`; `vis "$PANE" > "$X/m.txt"`; `send-keys esc`; `sleep 0.4`; `send-keys ctrl+c`.
- Pass: `s.txt` holds a line starting `! pwd` and, within 5 lines after it, a line ending `/b`; `m.txt` holds `only-in-b.txt`. A `!pwd` that prints a line ending `/a` is a FAIL; so is an `@` list that does not show `only-in-b.txt`.
- Evidence: the two reads' key lines (folder prefix replaced).

**XF-8: `/context` names the switched model.** Covers `P2-06` with `P2-10`.
- Steps: start Talaria. `submit "$PANE" 'Reply with only the word ready.'`; wait. `submit "$PANE" '/model'`; select `<M2>`; `enter`; wait for `Model: <M2> on Ollama Cloud.` `submit "$PANE" 'Reply with only the word ready.'`; wait; `submit "$PANE" '/context'`; `sleep 3`; `vis "$PANE" > "$X/c.txt"`.
- Pass: `grep -cE "Context · <M2> · .*tokens" "$X/c.txt"` prints 1 and `grep -c 'Context · minimax-m3.1-flash' "$X/c.txt"` prints 0.
- Evidence: the header line.

**XF-9: Idle cost with a standing loop and every phase 2b feature used.** Covers all eight issues, the idle budget of P1-11 and `P2-15`'s countdown rule. The `process-info` path is fact S4.
- Steps: a new folder; start Talaria in `$PANE`. After 5 s at an empty composer: `PID=$(herdr pane process-info --pane "$PANE" | jq -r '.result.process_info.foreground_processes[] | select(.cmdline | contains("target/release/talaria")) | .pid')`; sample 0: `R0=$(ps -o rss= -p "$PID")`. Then, waiting for each reply: `/model` open and Esc twice; `/context`; `/usage`; `submit "$PANE" 'Reply with only the word ready.'`; `submit "$PANE" '/rename fixture xf9 <tag>'`; `submit "$PANE" '/loop 45m reply with the word tick'` and wait up to 60 s for the first wakeup turn to finish. Wait 5 s. Sample 1: `R1`, `C1` (`ps -o time= -p "$PID"`), `V1=$(vis "$PANE")`. `sleep 120`. Sample 2: `R2`, `C2`, `V2`. Convert `time=` with `awk -F: '{ s = 0; for (i = 1; i <= NF; i++) s = s * 60 + $i; print s }'`. Cleanup, even on FAIL: `submit "$PANE" '/loop stop'` and read `✓ Loop stopped.`.
- Pass: `R0` at most 40960 (KiB, the 40 mebibyte start budget); `R2` at most 102400 (the 100 mebibyte ceiling); `R2 - R1` at most 4096; `C2 - C1` at most 1 second; `V1` and `V2` identical after deleting the lines holding `↻ loop` (the countdown changes by minutes: `P2-15` UAT-2). All four limits are this issue's own. The architecture's budget is 40 MB at start and 100 MB after a 2,000-message session; this row reads "MB" as mebibytes, as P1-11 decision 7 does, and applies the 100 MiB figure to a short feature session. The growth and CPU limits are sanity checks, the same as the earlier gates'. The operator confirms all four on `PH2B` before the freeze. A `ps` or `jq` that does not work on the machine makes the row `BLOCKED (environment)`.
- Evidence: the three samples and conversions, the output of `diff <(printf '%s\n' "$V1" | grep -v '↻ loop') <(printf '%s\n' "$V2" | grep -v '↻ loop')` (it prints nothing), the `✓ Loop stopped.` line.

**XF-10: Resume keeps the switched model and the new title.** Covers `P2-06` and `P2-07` with `P1-12`'s resume.
- Steps: start Talaria. `submit "$PANE" "/rename fixture xf10 <tag>"`; `/model`, select `<M2>`, `enter`; `submit "$PANE" 'Reply with only the word ready.'`; wait. `submit "$PANE" '/quit'`; read the session id printed after `--resume` in the hint that `/quit` prints (P1-13 fact T2; copy the hint's real wording from a recorded run into the evidence before the row's first run) as `SID`. In a new pane at the same folder: `$T --no-ensure --resume "$SID" --profile fixture`; wait for `── Resumed:`; `vis > "$X/a.txt"`; `submit '/rename'`; `sleep 2`; `vis > "$X/b.txt"`; `submit '/quit'`.
- Pass: `a.txt` has its status line naming `<M2>` and not `minimax-m3.1-flash` (the resume did not run on `--model` defaults); `b.txt` holds `This session is called "fixture xf10 <tag>".`.
- Evidence: the status line and the notice line.

**XF-11: Two providers, and the profile default is unchanged (requirement R4).** Covers `P2-06`'s second half (its AC-8 leaves it here). Operator precondition: a second provider set up on `fixture` with one working cheap model; the operator names its public provider id `<V2>` and model id `<MV2>` on the issue (never a key); else BLOCKED (environment).
- Steps: `jq '.defaults.fixture' ~/.talaria/models.json 2>&1` (expect `null` or an error for a missing file). Baseline: start `$T --no-ensure --profile fixture` (no model flags) and note the model in its status line as `<D>`; `/quit`. Start `$T $F`; `/model`; select a row under the `<V2>` provider's header whose id is `<MV2>`; `enter`; wait for `Model: <MV2> on `; `submit "$PANE" 'Without using any tools, join these two words with a hyphen and reply with only the result: <word> and <number>'` (a fresh word and number 1000 to 9999, as P1-13 draws them); wait for the joined token (it cannot be in the prompt). `/model`, select `minimax-m3.1-flash` under `Ollama Cloud`, `enter`; submit a second token prompt; wait. `/quit`. Start `$T --no-ensure --profile fixture` again; `vis`; `/quit`. Repeat the `jq` line.
- Pass: after the first switch the status line names `<MV2>` and the first token came back; after the second switch it names `minimax-m3.1-flash` and the second token came back; the baseline and the last relaunch name the same `<D>`; both `jq` runs print the same thing, and it is not a `fixture` entry naming `<MV2>`.
- Evidence: `<D>` twice, the two status lines, the two tokens and their replies, both `jq` outputs.

**XF-12: The pager over a live queue.** Covers `P2-02` with `P2-08`: queue keys never leak into the pager.
- Steps: start Talaria. `submit "$PANE" 'Run the shell command sleep 25, then reply with the word first in capital letters.'`; after 3 s `submit "$PANE" 'Reply with the word omega in capital letters.'`; `vis "$PANE" > "$X/q0.txt"`. `herdr pane send-keys "$PANE" ctrl+o`; `sleep 1`; `vis "$PANE" > "$X/pg.txt"`; send `up`, `d`, `n`, `s` one by one (1 s apart; they are the queue's keys); `vis "$PANE" > "$X/pg2.txt"`; `send-keys q`; `sleep 1`; `vis "$PANE" > "$X/after.txt"`. Wait for `FIRST`, then `OMEGA`, and the screen to settle; `rec "$PANE" > "$X/r.txt"`.
- Pass: `q0.txt` holds `queued (1)`; `pg.txt` holds ` Transcript ·` and `following`; `pg2.txt` still holds ` Transcript ·` (the queue keys did not close or change the pager); `after.txt` holds `queued (1)` and no `Transcript ·`; `grep -c '⤷ steer' "$X/r.txt"` prints 0; `grep -cw FIRST "$X/r.txt"` and `grep -cw OMEGA "$X/r.txt"` each print 1 or more (the turn was not interrupted by a leaked send-now, and the queued prompt was not removed or sent early).
- Evidence: the key lines of `q0`, `pg`, `pg2`, `after` and the three counts.


### 7.4 The operator's switch week (rows OP-1 to OP-8, and the two operator source rows)

The roadmap's phase 2 operator check reads: "the operator uses Talaria instead of Claude Code for a week of Hermes-backed coding. Every phase 1 and phase 2 parity row works; anything that does not is either fixed or explicitly accepted as a gap." This section makes each part of that sentence countable. It is the lead's proposal; the operator confirms the numbers on `PH2B` before the freeze (section 10.2, question 2) and may change them by editing the issue.

**Two operator source rows first (one sitting, on day 1, when no agent runs a live scenario).** `P2-16 UAT-4` (the profile's approval mode changed to `smart` in one pane, seen in another, restored to `manual`) and `P2-16 UAT-6` (what approvals off actually does, with a real `rm -r` card). The operator posts one comment per row, its first line `<id>: PASS`, `<id>: FAIL` or `<id>: BLOCKED (<cause>)`, then the counts and the pane texts; an agent copies each into the evidence with the comment's URL. The operator ends both with `approvals.mode: manual` on `fixture` and posts the read-back.

**The week.** Counted by these rules:

- W1. At least 5 calendar days inside a window of 14, at least 1 hour of Talaria-driven coding on each counted day, at least 12 hours in total (the sum of the day log).
- W2. Real tasks on throwaway branches of repositories of the operator's choice, with any profile and any models Hermes offers. The evidence names a repository as `<repo-1>`, `<repo-2>`; a profile other than `default` or `fixture` is written `<profile>`.
- W3. The binary is the candidate's: the `shasum -a 256` of the file the operator runs equals the one in AUTO-1's evidence. The operator may leave out `--no-ensure`. The operator, not an agent, may restart the gateway or change configuration; each time they record it as a day-log line with the read-back of `approvals.mode`. The day log is the one inside the OP-8 comment on this issue; agents read only that posted comment, never the operator's private notes.
- W4. Claude Code is not used for a task the operator meant to do in Talaria. Each time it is, that is a fallback and gets a log line (OP-6).
- W5. The terminals: Herdr on every day; one plain terminal window and one tmux session at least once each (for OP-5).
- W6. The agent rows come first: the week starts only when stage 2 is finished with every agent row PASS on this candidate. A fix that merges during the week makes a new candidate; the rule for what the operator repeats is at the end of this section.

**The parity matrix.** Every line gets one verdict in the final comment: `PAR-<n>: works | used <k> times on <dates> | <what was seen>`, `PAR-<n>: fixed #<issue> | <what was seen after the fix>`, or `PAR-<n>: ACCEPTED GAP #<issue> | <reason in one sentence>`. The lines are the rows of the [parity checklist](../04-prior-art.md#claude-code-parity-checklist) marked phase 1 or phase 2, plus the two P2-16 features:

| Line | Parity row | Built by | Minimum real use |
|---|---|---|---|
| PAR-1 | Streaming output; Esc and Ctrl+C interrupt | phase 1 | every counted day |
| PAR-2 | Multi-line composer, `/clear` | phase 1 | every counted day |
| PAR-3 | Status line | phase 1 | every counted day |
| PAR-4 | Permission prompts: approval cards, clarifying questions | phase 1 | 3 cards (one `deny`, one `once`) and 1 question answered |
| PAR-5 | Resume and continue sessions | phase 1 | 3 times, one after quitting mid-turn |
| PAR-6 | Tool-call cells and a transcript view (Ctrl+O) | P2-01, P2-02 | 2 times on 2 days |
| PAR-7 | Queued messages, send now, take back | P2-08 | 3 prompts queued, one edited, one sent now |
| PAR-8 | `@` file mentions, slash menu, custom commands, skills | P2-03, P2-04 | 2 times on 2 days |
| PAR-9 | Image paste, large-paste collapse | P2-04 | once |
| PAR-10 | `!` shell mode | P2-05 | 2 times on 2 days |
| PAR-11 | `/btw` side questions | P2-05 | once |
| PAR-12 | Task checklist | P2-11 | once |
| PAR-13 | Rewind and checkpoints, `/diff` | P2-09 | one conversation rewind and one file restore |
| PAR-14 | Model picker, effort, fast mode | P2-06, P2-15 | 3 switches of model or effort |
| PAR-15 | Context view, compaction | P2-10 | `/context` 2 times, one finished `/compact` |
| PAR-16 | Cost and usage | P2-10 | `/usage` 2 times |
| PAR-17 | Name, branch and export sessions | P2-07 | one each |
| PAR-18 | Plan mode (partial: Hermes's `/plan`; a true read-only planning mode is an upstream gap) | P2-15 | `/plan` once |
| PAR-19 | External editor (Ctrl+G), history search (Ctrl+R), keybinding change, suspend (Ctrl+Z) | P2-13 | each once; one default key changed in the configuration file the documentation names, the new key works and the old one does nothing |
| PAR-20 | Themes | P2-12 | once, in a light and a dark terminal if both exist |
| PAR-21 | Bell and notifications when done or waiting | P2-13 | once |
| PAR-22 | Headless print mode (`-p`) | P2-14 | in 2 real scripts or jobs, one parsing `--output-format json` |
| PAR-23 | `/loop`, `/goal` | P2-15 | one of them once |
| PAR-24 | Permission mode in the status line (Shift+Tab) and `/cd` | P2-16 | Shift+Tab 3 times, `/cd` 2 times |

If a real week gives no occasion for a line, the operator provokes one on the last day with the matching scenario of the source issue; a line never gets `works` without having been used.

**The operator rows.** Each is PASS only when every part is true; the operator, not an agent, decides it. The operator's comment for each row carries the counts; an agent copies it into the evidence (section 5.3 scrubbing) with the comment's URL.

| Row | Pass condition |
|---|---|
| OP-1 | Phase 1 parity in real use. PAR-1 to PAR-5 each have a verdict and meet their minimum use. Every approval answer took effect (a `deny` denied, a `once` ran once), no card stayed after an answer, and no answer went to a different choice than the key pressed. |
| OP-2 | Phase 2a parity in real use. PAR-6, PAR-8 to PAR-12 and PAR-19 to PAR-21 each have a verdict and meet their minimum use. |
| OP-3 | Phase 2b parity in real use. PAR-7, PAR-13 to PAR-18 and PAR-22 to PAR-24 each have a verdict and meet their minimum use. |
| OP-4 | Many models in real use (requirement R4). At least 2 providers used; at least 3 switches in live sessions, one of them mid-turn (the notice `Model will change to <model> when the current turn ends.` was seen); a new session started without model flags names the same default model on the last day as on the first (both read-backs in the comment). |
| OP-5 | Nothing stale or garbled. Across the week and after at least 3 terminal resizes: zero repeated lines; zero `working` still shown more than 5 s after a reply ends; zero wrong model or wrong `approvals:` segment; zero raw escape text; zero overlay text left in the terminal's scrollback after the model picker, the session list, the pager, the rewind list and the palette (each used at least once in Herdr and once in a plain terminal); zero characters lost when typing while a reply streams (checked in at least 5 replies of 30 lines or more); and the Talaria process's resident memory at the end of the longest session of the week is at most 102400 KiB (`ps -o rss= -p <pid>`), with that session's message count in the comment. |
| OP-6 | Fallbacks logged. Every time the operator reached for Claude Code (or any tool other than Talaria) for a task meant for Talaria, one log line: date, task in a few words, why, issue number. Every line cites an issue that is fixed (closed with evidence) or an `ACCEPTED GAP #<issue>`. The comment gives the count of fallbacks and of Talaria sessions; zero fallbacks is allowed. |
| OP-7 | Defects reconcile. The operator found D defects; each has an issue; F are fixed (closed with evidence, the candidate changed, every agent row rerun on it) and G are `ACCEPTED GAP`s; D = F + G; none is open without one of the two. An accepted gap is allowed only for a parity line, with the operator's reason; it can never turn a FAIL of any AUTO, source or XF row into a pass. |
| OP-8 | The week log, the matrix and the verdict are posted. One comment holds the day log (dates, hours, terminals, sessions, gateway restarts and configuration changes with the `approvals.mode` read-back), the binary's checksum (equal to AUTO-1's), all 24 `PAR-` lines, the counts of OP-5 and OP-6, the list of `ACCEPTED GAP #<issue>` lines or `GAPS ACCEPTED: none`, and a line reading exactly `PHASE 2 OPERATOR CHECK: PASSED (candidate <7 hex>)` (its 7 characters equal the candidate's first 7). |

**After a fix.** When a defect found in the week is fixed and merged, the lead freezes a new candidate (section 3) and every agent row is rerun on it. The operator then uses the new binary for at least 2 working days and 4 hours, rechecks the PAR lines the fix's issue names and OP-5, and the lead lists in a comment the lines whose earlier `works` the fix could not have touched; the operator confirms that list in writing, and for those lines the earlier days still count. The verdict names the final candidate. Operator rows go back to NOT RUN until the verdict of the final candidate is posted.


## 8. Run order, failures and a new candidate

1. Stage 0: build the smoke extension and merge it (section 9). Freeze (section 3).
2. Stage 1 (agent tester, one pane at the worktree root): AUTO-1 to AUTO-7, then AUTO-8, AUTO-9, AUTO-10, AUTO-11 back to back.
3. Stage 2 (agent tester): the 46 source rows the operator does not run, in the order of section 7.2, then XF-1 to XF-12 in order, one live scenario at a time (one shared gateway and free model).
4. Stage 3 (operator): the two operator source rows in one sitting on day 1, then the week (section 7.4) and the verdict.
5. Stage 4: write the record and evidence (the first commit is the empty record, section 4), run the checker, open the pull request (documentation only), have it reviewed, merge. The pull request body carries the lines of the issue's REQ-16.

Three identical provider errors in a row (same HTTP status, same message) stop the run: report them on the issue and ask the operator, never retry in a loop. A row may be run up to three times on one candidate; every run goes in its evidence section with why each earlier failure was not a Talaria defect (a provider rate limit, a model that did not follow a plain prompt).

A FAIL stays FAIL until a new candidate passes it. The tester comments the pane text on the issue; the lead files a defect issue through Mission Control for the owner of the source issue (the operator files those found in the week, or asks the lead to); the fix merges; the lead freezes a new candidate (section 3) and every row is run again; the record is overwritten (git history keeps the old one). A defect is never fixed inside this issue, and no scenario's expected result is edited to make it pass; if a source issue's expected text is wrong, its owner corrects the source issue and this unit's Appendix A is corrected (a correction of the specification is the one change allowed after the freeze).

### 8.1 Rulings that settle questions a builder asked

These settle points where two rules of the issue could be read against each other. They change no pass rule; every one keeps "only PASS is a pass".

1. **A row's label follows its cause, not the exit code.** A suite run that prints `PASSED WITH SKIPS` (exit 3) is BLOCKED (environment). Under `TALARIA_LIVE_REQUIRE=1` a skipped step prints `FAIL <step> (skipped: <reason>; TALARIA_LIVE_REQUIRE=1)` and the run exits 1; the line says `(skipped:`, so the cause is the environment and the row is `BLOCKED (environment: <step> skipped)`, not FAIL. Any other `FAIL <step> (...)` line, including `deadline exceeded in step <name>`, is a FAIL of the row unless item 2 shows it was not a Talaria defect. The checker accepts only the four forms and `BLOCKED (<cause>)` takes any cause text without a closing bracket.
2. **Reruns on the same candidate.** A row may be run up to three times on one candidate. A BLOCKED row is rerun once the cause is fixed (the comment names who fixed what). A FAIL may be rerun only when the tester shows it was not a Talaria defect: the pane text shows a provider error (status and message) or the model not following a plain prompt, while every wording and key of Talaria was as its source issue says; and the lead writes `rerun approved: <row id>: <cause>` on the issue before the rerun. A FAIL that is a Talaria defect (a wording that differs, a key that does nothing, a crash, a wrong state) is final for the candidate: the row is rerun only on a new candidate (the issue's REQ-16). A step that failed because a read source lagged (the failure's capture or a `READ-CHECK` shows the expected text on the other source) is not a Talaria defect: the lead may approve one rerun, and the evidence says which source lagged. When it is unclear which it is, treat it as a defect, because another candidate costs less than a wrong pass. The record holds the result of the last run; the evidence lists every run (`Run <k> of <n>`) with why each earlier one failed or was blocked. A row never turns PASS on a later run of the same candidate after a Talaria defect.
3. **AUTO-1 when the dispatch run reports nothing on the candidate.** The row is BLOCKED (environment), reported on P1-11. No candidate is accepted with it BLOCKED. Because the answer can be had in stage 0 (section 7.1), the operator then has P1-11's workflow fixed before the freeze, or, before the freeze, edits AUTO-1 to drop its bench-run part (settled decision 2).
4. **Dropping a row or a step before the freeze (settled decision 2).** Let N be 79 minus the rows dropped, and S 15 minus the steps of the `phase2b` suite dropped. In one commit whose message gives the reason, the lead changes: in the issue, every `79` (Objective, user-visible result, REQ-7, REQ-9, UAT-1, AC-5, AC-10, the fallback of settled decision 6, section 10.1 here), the row counts `11 + 48 + 12` (AC-7) and `48`, `12` and `8` as far as they drop, and, for a dropped step, every `15` (REQ-2, REQ-10, UAT-2, the AUTO-10 and AUTO-11 pass rule); here, the section 4 skeleton (rows removed) and the red-run line (`FAILED (N problems, 0 of N rows PASS)`); and in the checker, the `IDS=` line (the only list: the test script builds its rows from it). In the test script: case 1's `79 of 79` becomes `N of N`; a case that names a dropped id (`P2-16 UAT-6`, `AUTO-5`, `XF-2`, `P2-07 UAT-2`, `AUTO-1`, `OP-8`, `P2-10 UAT-6`, `XF-1`, `AUTO-2`, `AUTO-3`, `XF-6`) names another kept id in its place, with the same role; the number of cases stays 22. For a dropped smoke step, the test author deletes that step's functions, screens and wording constants in a commit `test(smoke): drop step <name> (operator decision)` and the counts of functions and steps are updated. The lead lists every number changed in a comment on the issue before the freeze. Dropping after the freeze is not possible: it is a new candidate.
5. **An operator row is the operator's.** No agent writes, edits or infers an operator comment. An agent copies it, scrubbed of identifiers only, into the evidence and cites its URL. If the operator's comment is missing, the row is NOT RUN. A parity line the operator marks `ACCEPTED GAP` counts toward OP-1 to OP-3 only with the reason and issue number the format of section 7.4 asks for.


## 9. The smoke suite extension

### 9.1 Why a third suite, and what it is not

`smoke run` of P1-13 checks five behaviours of the first usable binary in under five minutes; `PH2A-ACC` added a second suite (`--suite phase2a`, 21 steps) for phase 2a. Phase 2b adds about fifteen things a person sees. Putting them into either earlier run would lengthen it past its budget, would change code that earlier gates ran as regression rows, and would let a skip in a new step hide behind old steps. So the extension is a third suite, selected with `--suite phase2b`, with its own deadline (1500 s, section 9.6), its own result line and its own tests; the phase 1 and phase 2a suites, their tests and their README steps stay as they are (AC-2).

It is the lead's proposal, not a roadmap requirement: the roadmap asks only for a live check per unit and for "the operator check". The reason to build it is the one P1-13 gives: a repeatable unattended run, in a real pane, that fails with the screen text when something regressed, which later phases (the fleet, configuration, workflows) rerun on every candidate. The cost is a test author's and a developer's work on a tool, not on the product. If the operator declines (section 10.2, question 7), the unit has 77 rows (AUTO-10 and AUTO-11 dropped), no third ignored test and no section 9; section 8.1 item 4 gives the procedure for the counts, and the lead also edits, in the same commit, REQ-1 to REQ-5 and REQ-10, UAT-2, UAT-3, AC-2, AC-3 and AC-11 of the issue and the ignored-test list of AUTO-7.

What it is not: it is not a replacement for the 48 scenarios or the 12 cross-feature rows (it checks each feature's happy path and one refusal, not its edge cases), it is not a performance test (P1-11's harness owns budgets), and it does not run in continuous integration (it needs a live gateway, a model and Herdr; the operator and this unit run it by hand). It needs no `tmux`: the pastes of phase 2a were the only reason for that.

### 9.2 Shape of the code

New files in `tools/smoke` (P1-13's crate; its layout is in P1-13 spec section 3; the files of `PH2A-ACC` are described in its spec 9.2):

```text
src/steps2b.rs      run_smoke_2b and the 15 steps
src/screen2b.rs     pure helpers over screen text (below)
src/wording2b.rs    every Talaria screen text a 2b step relies on, as consts, each with a source comment (9.5)
tests/              screen2b.rs, steps2b.rs, typed_input2b.rs, cleanup2b.rs, cli2b.rs, fixtures_public_safe2b.rs,
                    live_smoke2b.rs, fixtures/screens/phase2b/*.txt
```

Minimal edits to existing files (named in the pull request): `src/suite.rs` (`Phase2b` joins the enum and `FromStr`; read `unknown_suite_exits_2_with_the_parse_text` in `PH2A-ACC`'s `cli2a.rs` first: if it pins the text `unknown suite '{s}'; use phase1 or phase2a` exactly, leave the text as it is, because AC-2 forbids editing that test, and say so in the pull request; otherwise the text becomes `unknown suite '{s}'; use phase1, phase2a or phase2b`; `--help` names all three suites either way), `src/config.rs` (the deadline default for `phase2b` is 1500 s; no new option), `src/main.rs` (dispatch on the suite; the `tmux` preflight of `phase2a` is not run for `phase2b`; `phase2b` with a `--model` other than `minimax-m3.1-flash` or a `--provider` other than `ollama-cloud` fails preflight with one plain sentence, `phase2b needs the default provider and model (ollama-cloud, minimax-m3.1-flash): its expected texts name them`, no `SMOKE_RESULT` line and exit 2, because the texts of 9.5 name them), `src/lib.rs`, `README.md`, `Cargo.lock` (no new crate is expected), `tests/support/` (additions only: a `Scratch2b` helper that builds the two git folders, and screen-script entries). The shared steps `startup`, `plain_answer` and `quit` are P1-13's, already `pub(crate)` with the environment prefix that `PH2A-ACC` added; `phase2b` calls them with an empty prefix.

`Token` is P1-13's (`Token { word, number }`, `Token::new(rng, used)`, `join()`, `prompt()`) and `prompt_after` is `PH2A-ACC`'s; nothing new is needed.

Public items, with the signatures the tests use (keep the names and shapes unless the code forces a change, and report it):

```rust
// suite.rs
pub enum Suite { Phase1, Phase2a, Phase2b }                // FromStr: "phase1" | "phase2a" | "phase2b"
// steps2b.rs
pub fn run_smoke_2b(term: &dyn Terminal, clock: &dyn Clock, cfg: &SmokeConfig, stop: &AtomicBool) -> SmokeReport;
// screen2b.rs (pure; each takes the normalised lines of P1-13 spec 5.3, box characters removed and trimmed)
pub fn status_approvals(visible: &str) -> Option<String>;                  // text after 'approvals: ' on the status line
pub fn picker_state(visible: &str) -> PickerState;                         // { title, groups: Vec<String>, has_current: bool, selected_row: Option<String>, not_set_up: bool, effort_footer: bool, no_match: Option<String> }
pub fn context_header(text: &str) -> Option<ContextHeader>;                // { model: String, percent: bool, line_index: usize }
pub fn context_rows(text: &str, from: usize) -> ContextRows;               // { categories: bool, free_space: bool, source: bool }
pub fn usage_block(text: &str) -> Option<UsageBlock>;                      // { title_index, tokens_row: bool, cost_row: bool }
pub fn rewind_list(visible: &str) -> RewindList;                           // { open: bool, prompts: Vec<u32>, selected: Option<u32> }
pub fn rewound_line(text: &str, prompt: u32) -> Option<usize>;             // line index of '── Rewound to before prompt #<n>'
pub fn queue_indicator(visible: &str) -> Option<u32>;                      // the n of 'queued (n)'
pub fn folder_segment(visible: &str) -> Option<String>;                    // the folder segment of the status line
pub fn lines_after_last<'a>(text: &'a str, needle: &str) -> Vec<&'a str>;  // the lines after the last line holding needle
pub fn print_check(out: &str, err: &str, exit: Option<i32>) -> PrintCheck; // { text_ok: bool, err_empty: bool, exit_ok: bool, not_started: bool }; not_started is true only when out is exactly one non-blank line holding both "status":"not_started" and "exit_code":4 (two JSON lines, or none, give false)
```

### 9.3 Flow, step by step

All waits are poll loops at 250 ms using `Clock`; every loop also checks the stop flag and the run deadline. Each step uses the source named in the table below: a read of the screen as it is now uses `visible`; a search for text that may have scrolled away uses `recent-unwrapped --lines 3000` (`RECENT_LINES`). Herdr's own `recent` source is never used. The rule: the status line, overlays, the composer, the queue indicator and anything that exists only on the live screen are read from `visible`; a notice or reply that Talaria commits to the transcript is searched in `recent-unwrapped`, because it may scroll off the visible screen. Which source lags for a full-screen program is P1-13's H14 item (section 2.1): this unit follows its recorded result (section 2.1 says what happens when a source lags). "Normalised lines" and "settled" are P1-13's (its sections 5.3 and 5.6). `submit` is P1-13's (its 5.1: `pane run`, with the single `enter` fallback and its `NOTE` line); `typed` is `send_text` without Enter; `key` is `send_keys`, and after `esc` the suite waits 400 ms. A step that fails prints the reason of section 9.4 and the capture. Two steps get one repeat when their first attempt fails only because the model did not do what its plain prompt asked: `steer-idle` (no `IDLE` reply within 90 s) and `queue` (the first attempt never showed `queued (1)` within 5 s, because the turn had already ended). A repeat starts the whole step again from its first submit, but only after the screen has settled (3 s quiet, at most 60 s, no working indicator): a `/steer` sent while a turn runs would steer that turn instead of starting a new one, which could hide a defect. The repeat's checks read only the lines after the last echo of its own first prompt (`lines_after_last`), so the first attempt's lines never satisfy them. A second miss fails with the reason. Every other step fails at once (a model that does not answer `ok` to the rewind prompts is a failure to report, not a retry). Only `permission` may SKIP.

| Step | Decides on `visible` (the screen now) | Decides on `recent-unwrapped` (may have scrolled) |
|---|---|---|
| startup, plain-answer, quit | as P1-13 | as P1-13 |
| context-empty, model-picker | every check | nothing |
| permission | the status line and the second press | the notice `Approvals off for this session only` |
| model-refusal | the status line | the refusal line |
| rewind | the overlay, its rows, the composer | the rewound line, the replies, the scrollback check |
| steer-idle, hermes-commands, rename-export, cd | the status line | the notice lines, the `IDLE` reply, the `Exported` line |
| context | nothing | the header, the rows, the usage block, the refusals |
| queue | the indicator and its preview | the order of `FIRST`, the omega user line and `OMEGA` |
| headless | nothing (the scratch files and the exit marker) | the exit marker |

**9.3.0 Setup (before the tab).** Run id as P1-13 7.1. The scratch root is a new folder under the system temporary directory, mode 0700, `talaria-smoke-<run id>`, holding two git folders `a` and `b` (each made with `git init -b main` and one empty commit by `git -c user.name=smoke -c user.email=smoke@example.invalid commit --allow-empty -m init`), and an empty folder `empty-home`. The session folder is `a`; the tab's working folder is `a`. The scratch root holds no space and no single quote (as in `PH2A-ACC` REQ-1; the preflight checks it). Tokens: A for `plain-answer`; the title tag is the run id. The tab, its root pane (`first`) and the `TabGuard` are made as in P1-13 4.1.

**9.3.1 `startup`** (P1-13 4.2 unchanged, in folder `a`, no `EDITOR`). Nothing is typed until the stable screen.

**9.3.2 `context-empty`** (P2-10 UAT-5). Before any prompt: `submit("/context")`; within 5 s `visible` holds a line `Context · not measured yet` and a line `Hermes builds this session's agent on the next prompt; until then there is nothing to break down.`. `submit("/usage")`; within 5 s a line holds `No API calls yet in this session.`. (The step is first because a prompt makes both texts impossible.)

**9.3.3 `permission`** (P2-16 UAT-1, first half). `visible` holds `approvals: manual` in the status line. `key shift+tab` (when Herdr rejects the name, `key backtab`; when it rejects both, SKIP with the reason of 9.4 and the first line of Herdr's error); within 5 s the status line holds `approvals: off (this session)`, and within 5 s a line starts `Approvals off for this session only`. The notice `Approvals back on for this session: ...` is not checked here; XF-6 checks it. `key` the same name again; within 5 s the status line holds `approvals: manual` again. When the first press worked and a later check fails, the step sends `/permissions session default` once before it fails (`P2-16` REQ-5: it sets this session to follow its profile, which is `manual`; it changes no profile setting and no other session; the session is the suite's own and is abandoned at the end either way). Nothing here touches the profile's mode.

**9.3.4 `model-picker`** (P2-06 UAT-1). `submit("/model")`; within 5 s `visible` holds a line `Switch model · this session`, an `Ollama Cloud` header with `current` on it or its row, the row `● minimax-m3.1-flash`, a group header starting `Not set up (` and a footer line starting `effort:` (any name missing is named in the reason; `P2-06` UAT-1 requires the `Not set up (` group, and Hermes lists more providers than any one profile has keys for, so it always shows: if it is missing the step fails with the `rows` reason and the gap is reported on `P2-06`). `typed("zzzz")`; within 5 s a line holds `No model matches "zzzz".`. `key esc`, 400 ms; within 3 s the list is back (`Switch model · this session` and the `Ollama Cloud` header visible, no `No model matches`). `key esc`, 400 ms; within 3 s no line holds `Switch model`.

**9.3.5 `model-refusal`** (P2-06 UAT-5, first scenario line). `submit("/model no-such-model-xyz --provider ollama-cloud")`; within 10 s `recent-unwrapped` holds the line `Hermes could not switch to no-such-model-xyz: it does not know that model on Ollama Cloud, or the provider is not set up. Nothing changed.`; afterwards the status line still holds `minimax-m3.1-flash`. The refusal sentence is Talaria's own wording from `P2-06` (it reports Hermes's refusal), not Hermes text, so a FAIL here goes to the owner of `P2-06`.

**9.3.6 `rewind`** (P2-09 UAT-1). `submit("Remember the code word heron. Reply ok.")`, wait until the screen settles 3 s and a line equal to `ok` (case-insensitive, trimmed) follows the prompt echo; the same for `Now remember the code word lark instead. Reply ok.` (90 s each). With an empty composer: `key esc`, wait 200 ms, `key esc`; within 5 s `visible` holds `Rewind · choose a prompt to go back to` and rows `#2` (selected) and `#1`. `key enter`; `typed("c")`; `key enter`; within 10 s `recent-unwrapped` holds a line starting `── Rewound to before prompt #2`, and within 3 s the composer line holds `Now remember the code word lark instead`. The overlay's text is gone from `recent-unwrapped` (no line holds `Rewind · choose a prompt`). `key ctrl+c` (clears the draft); `submit("Which code word should you remember? One word.")`; within 90 s the lines after the last line holding `Which code word should you remember` (`lines_after_last`) hold the word `heron` and not `lark` (case-insensitive). The suite never sends `f` (restore files): only the conversation rewinds.

**9.3.7 `plain-answer`** (P1-13 4.3 unchanged). Token A.

**9.3.8 `steer-idle`** (P2-08 UAT-6, first half). `submit("/steer Reply with the word idle in capital letters.")`; within 5 s `recent-unwrapped` holds a line containing `No turn is running, so this was sent as a new prompt`; within 90 s the whole word `IDLE` (upper case, as a whole word: the prompt contains only lower case `idle`) appears in the lines after the last line holding `Reply with the word idle` and the screen settles 3 s. The one repeat is the one of the first paragraph of this section.

**9.3.9 `context`** (P2-10 UAT-1, UAT-2 and UAT-4). `submit("/context")`; within 5 s `recent-unwrapped` holds a line starting `Context · minimax-m3.1-flash · ` and ending `tokens (<n>%)` or `tokens (~<n>%)`, and after it the lines `Estimated usage by category`, `Free space` and a line starting `Source: `. `submit("/usage")`; within 5 s a line `Usage · this session`, then rows starting `Tokens` and `Cost`. `submit("/compact --aggressive")`; within 5 s `Hermes does not support --aggressive. Nothing was changed.`. `submit("/context now")`; within 5 s `/context does not take an argument.`. No line of `recent-unwrapped` after the first `/context` holds `✓ Compressed` (a refused command changed nothing).

**9.3.10 `hermes-commands`** (P2-15 UAT-4). `submit("/fast")`; within 10 s `recent-unwrapped` holds `⚡ /fast is only available for OpenAI models that support Priority Processing.`. `submit("/loop 45m")`; within 10 s a line holds `/loop: missing prompt (usage: /loop [interval] <prompt>)`; afterwards no line of `visible` holds `↻ loop`. (The two texts are Hermes's own at v0.22.4: a changed wording at the pinned release is reported on `P2-15` and the constant is updated, never matched loosely.)

**9.3.11 `rename-export`** (P2-07 UAT-2, UAT-4 and UAT-6). `submit("/rename fixture smoke <run id>")`; within 10 s `recent-unwrapped` holds `Renamed to "fixture smoke <run id>".`. `submit("/rename")`; within 10 s `This session is called "fixture smoke <run id>". Type /rename <new title> to change it.`. `submit("/export")`; within 15 s a line starts `Exported "fixture smoke <run id>" to ./` and holds `Markdown, secrets redacted)`; the file name is read from that line; that file in `a` exists, has mode 0600 and holds the line `<word> and <number>` of token A's prompt. `submit("/export ./notes.txt")`; within 5 s `Use a file name ending in .md, .json or .html.`; `notes.txt` must not exist in `a`.

**9.3.12 `queue`** (P2-08 UAT-1). `submit("Run the shell command sleep 25, then reply with the word first in capital letters.")`; wait 3 s; `submit("Reply with the word omega in capital letters.")`; within 5 s `visible` holds `queued (1)` and exactly one line holding `Reply with the word omega` (the preview; the queued prompt has no user line yet). Every read below, `visible` ones too, is of the lines after the last line holding `Run the shell command sleep 25` (`lines_after_last`), so the first attempt's lines never count on the repeat. Within 120 s `FIRST` (whole word, upper case) appears in `recent-unwrapped`; within 120 s more the whole word `OMEGA` appears and the screen settles 3 s. In those lines, the user line holding `Reply with the word omega` (a line starting `USER_LINE_PREFIX`, 9.5, not the preview) comes after the line holding `FIRST`, and `OMEGA` after it; no line of `visible` holds `queued (` any more. The one repeat is the one of the first paragraph of this section: it starts again from the long prompt once the screen has settled.

**9.3.13 `cd`** (P2-16 UAT-2 and UAT-5). `submit("/cd ../b")`; within 15 s `recent-unwrapped` holds a line starting `Moving this session to` and a line starting `This session now works in` with a path ending `/b (main)`; the status line's folder segment ends `b (main)`. `submit("/cd")`; within 5 s a line starts `This session works in` and holds a path ending `/b`. `submit("/cd ./missing-folder-smoke")`; within 5 s a line starts `No folder at`.

**9.3.14 `quit`** (P1-13 4.6) with token A in place of its A and B: `committed output vanished on exit (token A missing)` when A is absent from the first pane's recent text.

**9.3.15 `headless`** (P2-14 UAT-1 and UAT-5). The first pane's shell is back. Text run: `run(first, "'<talaria>' -p --no-ensure --profile <p> --model <m> --provider <v> \"What is 1234 plus 4321? Reply with only the number.\" > '<scratch>/print.out' 2> '<scratch>/print.err'; echo SMOKE_EXIT:$?")`; within 90 s one more `SMOKE_EXIT:` line than before; it must read `SMOKE_EXIT:0`, `print.out` trimmed must equal `5555` and `print.err` must be empty (zero bytes). No-gateway run: `run(first, "HERMES_HOME='<scratch>/empty-home' '<talaria>' -p --no-ensure --output-format json \"hello\" > '<scratch>/nogw.out' 2> '<scratch>/nogw.err'; echo SMOKE_EXIT:$?")`; within 30 s a new marker `SMOKE_EXIT:4`; `nogw.out` holds exactly one line, which holds `"status":"not_started"` and `"exit_code":4`; `nogw.err` starts with `talaria: no Hermes gateway is running for this home`. Reading the scratch files is the one place the suite reads anything but screens; the files are in the suite's own scratch folder.


### 9.4 Exact reason texts

Strings printed inside `FAIL <step> (...)`; `<n>` in a time is the limit in force after `--step-scale`, in whole seconds rounded to the nearest, at least 1. Tests assert them character for character; a reason the code has to change is reported on the issue. The phase 1 reasons of P1-13 spec 4.10 apply unchanged to `startup`, `plain-answer` and `quit` (the shell, startup, deadline, signal and panic rows included) except the one `quit` row below.

| Step | Situation | Reason |
|---|---|---|
| context-empty | no context line | `/context did not show 'Context · not measured yet' within 5 s` |
| context-empty | no usage line | `/usage did not show 'No API calls yet in this session.' within 5 s` |
| permission | start | `the status line did not show 'approvals: manual' at the start` |
| permission | first press | `Shift+Tab did not change the status line to 'approvals: off (this session)' within 5 s` |
| permission | notice | `the notice 'Approvals off for this session only' did not appear within 5 s` |
| permission | second press | `the second Shift+Tab did not restore 'approvals: manual' within 5 s` |
| model-picker | not open | `/model did not open 'Switch model · this session' within 5 s` |
| model-picker | rows | `the model picker lacks: <names joined by ', '>` |
| model-picker | filter | `typing zzzz did not show 'No model matches "zzzz".' within 5 s` |
| model-picker | Esc | `Esc did not step back: the first Esc must restore the list and the second must close the picker` |
| model-refusal | no refusal | `the refusal for an unknown model did not appear within 10 s` |
| model-refusal | model changed | `the status line no longer shows minimax-m3.1-flash after the refused switch` |
| rewind | reply | `a reply to a code word prompt did not finish within 90 s` |
| rewind | overlay | `Esc twice did not open 'Rewind · choose a prompt to go back to' within 5 s` |
| rewind | rows | `the rewind list does not show both prompts #2 and #1` |
| rewind | no marker | `no line starting '── Rewound to before prompt #2' appeared within 10 s` |
| rewind | composer | `the composer does not hold the second prompt after the rewind` |
| rewind | answer | `the reply after the rewind did not name heron, or named lark` |
| rewind | scrollback | `the rewind overlay left its text in the scrollback` |
| steer-idle | notice | `/steer on an idle session did not say 'No turn is running, so this was sent as a new prompt' within 5 s` |
| steer-idle | reply | `the prompt sent by /steer did not get an 'IDLE' reply within 90 s` |
| context | header | `/context did not show a header 'Context · minimax-m3.1-flash · ' ending 'tokens (<n>%)' within 5 s` |
| context | rows | `/context lacks: <names joined by ', '>` |
| context | usage | `/usage did not show 'Usage · this session' with a 'Tokens' row and a 'Cost' row within 5 s` |
| context | aggressive | `/compact --aggressive did not say 'Hermes does not support --aggressive. Nothing was changed.' within 5 s` |
| context | argument | `/context now did not say '/context does not take an argument.' within 5 s` |
| context | compressed | `a '✓ Compressed' line appeared: a refused command changed the conversation` |
| hermes-commands | /fast | `/fast did not show its refusal '⚡ /fast is only available for OpenAI models that support Priority Processing.' within 10 s` |
| hermes-commands | /loop | `/loop 45m did not show '/loop: missing prompt (usage: /loop [interval] <prompt>)' within 10 s` |
| hermes-commands | loop line | `a loop line appeared after /loop 45m with no prompt` |
| rename-export | rename | `/rename did not answer 'Renamed to "<title>".' within 10 s` |
| rename-export | title | `/rename with no title did not say 'This session is called "<title>".' within 10 s` |
| rename-export | export | `/export did not print 'Exported "<title>" to ./<name>' within 15 s` |
| rename-export | file | `the exported file is missing, is not mode 0600, or lacks the plain-answer prompt` |
| rename-export | bad name | `/export ./notes.txt did not say 'Use a file name ending in .md, .json or .html.' within 5 s` |
| rename-export | bad name wrote | `/export ./notes.txt created a file` |
| queue | no indicator | `'queued (1)' and the preview 'Reply with the word omega' did not appear within 5 s` |
| queue | preview twice | `the queued prompt shows more than once on screen before it ran` |
| queue | first | `the first reply 'FIRST' did not appear within 120 s` |
| queue | omega | `the queued prompt did not run: 'OMEGA' did not appear within 120 s` |
| queue | order | `the order is wrong: the omega prompt and OMEGA must come after FIRST` |
| queue | indicator left | `an indicator 'queued (' is still shown after the queue drained` |
| cd | moved | `/cd ../b did not print 'This session now works in' with a path ending '/b (main)' within 15 s` |
| cd | segment | `the status line folder segment does not end with 'b (main)'` |
| cd | show | `/cd with no argument did not print 'This session works in' within 5 s` |
| cd | missing folder | `/cd ./missing-folder-smoke did not print 'No folder at' within 5 s` |
| headless | text run | `the plain headless run did not print 5555, exit 0 and leave standard error empty (exit <n>)` |
| headless | no gateway json | `the headless run without a gateway did not print one JSON line with "status":"not_started" and "exit_code":4, and exit 4` |
| headless | no gateway text | `the headless run without a gateway did not print 'talaria: no Hermes gateway is running for this home' on standard error` |
| headless | timeout | `a headless command did not finish within <n> s` |
| quit | token A | `committed output vanished on exit (token A missing)` |
| permission | skip (not a fail) | `Herdr rejected the key names shift+tab and backtab: <first line of Herdr's error>` |

With `TALARIA_LIVE_REQUIRE=1` a skip prints `FAIL <step> (skipped: <skip reason>; TALARIA_LIVE_REQUIRE=1)`.

### 9.5 Talaria screen texts the steps depend on

Every text below is one `const` in `src/wording2b.rs` with a comment naming its source as `#<issue number>` and the scenario title, for example `// #<n>, scenario "<scenario title>"`. The comment never holds a planning code such as a unit id or a `UAT-<n>` number: roadmap rule 7 forbids planning codes in source comments, and `PH2B`'s AC-5 rejects them in any added `.rs` line. The Source column below uses unit codes only so the lead can find each issue. The texts come only from the final bodies of the accepted issues. `USER_LINE_PREFIX` is how the echo of a submitted prompt starts in `recent-unwrapped` (used by `queue`, and through `lines_after_last` by `rewind` and `steer-idle`). Its value is copied from the recorded screen `queue_done.txt` with a source comment; nobody guesses it. The default model and provider names in these texts are why `phase2b` accepts only the default provider and model (9.2). A text a step needs that no source states is not invented: the step is changed to need less, or the gap is reported on the source issue.

| Constant | Text | Source | Used by (steps) |
|---|---|---|---|
| `CTX_EMPTY`, `USAGE_EMPTY` | `Context · not measured yet` ; `Hermes builds this session's agent on the next prompt; until then there is nothing to break down.` ; `No API calls yet in this session.` | P2-10 UAT-5 | context-empty |
| `APPROVALS_MANUAL`, `APPROVALS_OFF`, `NOTICE_OFF` | `approvals: manual` ; `approvals: off (this session)` ; `Approvals off for this session only` (prefix; the notice names `Shift+Tab`) | P2-16 UAT-1 | permission |
| `PICKER_TITLE`, `PICKER_GROUP`, `PICKER_CURRENT_ROW`, `PICKER_NOT_SET_UP`, `PICKER_EFFORT`, `PICKER_NO_MATCH` | `Switch model · this session` ; `Ollama Cloud` (with `current`) ; `● minimax-m3.1-flash` ; `Not set up (` ; `effort:` ; `No model matches "zzzz".` | P2-06 UAT-1 | model-picker |
| `MODEL_REFUSAL` | `Hermes could not switch to no-such-model-xyz: it does not know that model on Ollama Cloud, or the provider is not set up. Nothing changed.` | P2-06 UAT-5 (Talaria's wording of Hermes's refusal) | model-refusal |
| `REWIND_TITLE`, `REWOUND` | `Rewind · choose a prompt to go back to` ; `── Rewound to before prompt #<n>` (prefix) | P2-09 UAT-1 | rewind |
| `STEER_IDLE` | `No turn is running, so this was sent as a new prompt` | P2-08 UAT-6 | steer-idle |
| `CTX_HEADER`, `CTX_ROWS`, `USAGE_TITLE`, `COMPACT_REFUSAL`, `CTX_ARG`, `COMPRESSED` | `Context · minimax-m3.1-flash · ` ... `tokens (<n>%)` ; `Estimated usage by category`, `Free space`, `Source: ` ; `Usage · this session` (rows `Tokens`, `Cost`) ; `Hermes does not support --aggressive. Nothing was changed.` ; `/context does not take an argument.` ; `✓ Compressed` | P2-10 UAT-1, UAT-2, UAT-4, AC-6 | context |
| `FAST_REFUSAL`, `LOOP_USAGE`, `LOOP_LINE` | `⚡ /fast is only available for OpenAI models that support Priority Processing.` ; `/loop: missing prompt (usage: /loop [interval] <prompt>)` ; `↻ loop` | P2-15 UAT-4, UAT-2 (Hermes's own wording at v0.22.4: `locales/en.yaml` key `gateway.fast.not_supported`, line 179; `hermes_cli/loops.py` line 124 with the prefix `/loop: ` added at line 681) | hermes-commands |
| `RENAMED`, `RENAME_SHOW`, `EXPORTED`, `EXPORT_BAD` | `Renamed to "<title>".` ; `This session is called "<title>". Type /rename <new title> to change it.` ; `Exported "<title>" to ./<name> (<size>, Markdown, secrets redacted).` ; `Use a file name ending in .md, .json or .html.` | P2-07 UAT-2, UAT-4, UAT-6 | rename-export |
| `QUEUE_INDICATOR`, `QUEUE_PREVIEW` | `queued (<n>)` ; `Reply with the word omega` | P2-08 UAT-1 | queue |
| `CD_MOVING`, `CD_DONE`, `CD_SHOW`, `CD_MISSING` | `Moving this session to` ; `This session now works in` (path ending `/b (main)`) ; `This session works in` ; `No folder at` | P2-16 UAT-2, UAT-5 | cd |
| `PRINT_OK`, `NOT_STARTED`, `NO_GATEWAY` | `5555` ; `"status":"not_started"` and `"exit_code":4` ; `talaria: no Hermes gateway is running for this home` | P2-14 UAT-1, UAT-5 | headless |

### 9.6 Time budget and defaults

| Item | Default | Notes |
|---|---|---|
| Whole run (`--deadline-secs`) | 1500 s for `phase2b` (1800 s for `phase2a`, 300 s for `phase1`) | Counted from the first `herdr` call; a run still going fails with `deadline exceeded in step <name>`; cleanup still runs; never scaled, so a run with `--step-scale` above 1 also passes a larger `--deadline-secs`. The step limits below add up to the worst case shown, so the deadline sits above them and the step limit, not the deadline, names a slow step |
| Step limits | the table below | scaled |
| Quiet periods | 1 s, 2 s, 3 s as in the steps | never scaled |
| Wait after Esc | 400 ms | never scaled |
| Poll interval | 250 ms | never scaled |

| Item | Limit | Worst case (s) |
|---|---|---|
| shell ready (first pane) and startup | 45 s shell, 30 s startup | 75 |
| context-empty | 5 s, 5 s | 10 |
| permission | 5 s per check, four checks | 20 |
| model-picker | 5 s, 5 s, 3 s, 3 s | 16 |
| model-refusal | 10 s | 10 |
| rewind | 90 s per reply (two code word prompts and the check prompt), 5 s overlay, 10 s marker, 3 s composer | 288 |
| plain-answer | 60 s | 60 |
| steer-idle | 5 s notice, 90 s reply; one repeat: settle at most 60 s, then 5 s and 90 s again | 250 |
| context | 5 s per check, four checks | 20 |
| hermes-commands | 10 s, 10 s | 20 |
| rename-export | 10 s, 10 s, 15 s, 5 s, 5 s | 45 |
| queue | 5 s indicator, 120 s FIRST, 120 s OMEGA; one repeat after a failed indicator check: settle at most 60 s, then the 5 s check again | 310 |
| cd | 15 s, 5 s, 5 s | 25 |
| quit | 15 s | 15 |
| headless | 90 s text run, 30 s no-gateway run | 120 |
| Sum of the step limits | | 1284 |
| Quiet periods (1 s, 2 s, 3 s as in the steps), never scaled | | about 70 |

Model turns: the two code word prompts and the check prompt of `rewind`, `plain-answer`, the `steer-idle` prompt, the two `queue` prompts and one `headless` answer: eight, plus the 25 s sleep, so a normal run takes four to eight minutes. The 1500 s deadline is above the worst case and is a ceiling, not a target. These figures are estimates from the number of turns and the limits, not measurements; the first live run replaces them with measured step times in the journal.

### 9.7 Typing rules and safety (the issue's REQ-3)

The suite sends only: slash lines (with Enter, outside the pager) `/quit`, `/context`, `/usage`, `/model`, `/model no-such-model-xyz --provider ollama-cloud`, `/steer Reply with the word idle in capital letters.`, `/compact --aggressive`, `/context now`, `/fast`, `/loop 45m`, `/rename fixture smoke <run id>`, `/rename`, `/export`, `/export ./notes.txt`, `/cd ../b`, `/cd`, `/cd ./missing-folder-smoke`; one cleanup line `/permissions session default`, sent only inside `permission` after a failed check that follows a successful Shift+Tab; the text `zzzz` typed inside the picker and the single letter `c` typed inside the rewind list; the prompts of 9.3 (the three code word prompts, token A's prompt, the long prompt and the omega prompt) ; the two headless command lines of 9.3.15; and the keys `shift+tab` or `backtab`, `esc`, `enter`, `ctrl+c`. It never sends `unattended`, `/permissions off`, `/permissions manual`, `/permissions smart`, `/yolo`, `/compact` without `--aggressive`, `/rollback`, `/undo`, `/retry`, `/branch`, `/resume`, `/skin`, the key `f` in the rewind list (restore files), a line that runs `hermes update` or changes Hermes configuration, or any text that starts the gateway (every `talaria` launch carries `--no-ensure`). The two commands that look risky are safe on purpose: `/compact --aggressive` is refused by Talaria itself with `Nothing was changed.` (P2-10 REQ and UAT-4), and `/loop 45m` has no prompt, so Hermes creates nothing (`P2-15` UAT-4). `typed_input2b.rs` enforces the list.

### 9.8 Screens and tool output to record

The eight issues have merged by the time this unit starts, so the test author records real screens from the built binary, not placeholders. Recording needs what a live run needs (the issue's operator preconditions: a gateway at the pinned release, the `fixture` profile with its key, `approvals.mode: manual`, two Ollama Cloud models for the picker, a Herdr pane), so those hold from stage 0, not only for the run. The binary is a `target/debug/talaria` built from `main` after the eight issues merged (the pre-candidate build is fine: the recordings are test fixtures and prove nothing about the candidate). In a throwaway tab (never the operator's own panes) run it with the `fixture` profile and capture, with `herdr pane read --source visible` (and `recent-unwrapped` where noted), the files below under `tests/fixtures/screens/phase2b/`. Scrub (section 5.3 of this document; replace the scratch path with `<scratch>`, the temporary prefix with `<tmp>`); an expected text that real screens do not show is reported on the source issue and the step is changed to need less, never given an invented fixture. One hand-built screen is allowed, for a failure the live gateway cannot be made to produce (the stale or empty read of each helper): it is marked `synthetic` in its file name and its first line, and no step's pass case may use it.

| File | Screen | Used by |
|---|---|---|
| `context_empty.txt` | `/context` and `/usage` before any prompt | `context_header_is_none_before_a_measure`, `context_empty_*` |
| `approvals_manual.txt`, `approvals_off.txt` | the status line with the segment in each state, with the off notice | `status_approvals_*`, `permission_*` |
| `picker_open.txt`, `picker_no_match.txt` | `/model` open; after typing `zzzz` | `picker_state_*`, `model_picker_*` |
| `model_refusal.txt` | the unknown-model refusal | `model_refusal_*` |
| `rewind_list.txt`, `rewound.txt` | the rewind overlay; after confirming, with the composer | `rewind_list_*`, `rewound_line_*`, `rewind_*` |
| `steer_idle.txt` | `/steer` on an idle session and the reply | `steer_idle_*` |
| `context_full.txt`, `usage_block.txt`, `context_refusals.txt` | `/context`, `/usage`, the two refusals (`recent-unwrapped`) | `context_header_*`, `usage_block_*`, `context_*` |
| `hermes_commands.txt` | the `/fast` and `/loop 45m` replies | `hermes_commands_*` |
| `rename_export.txt` | rename, title read-back, export and bad-name notices | `rename_export_*` |
| `queue_preview.txt`, `queue_done.txt` | the indicator and preview; after the order ran (`recent-unwrapped`) | `queue_indicator_*`, `queue_*` |
| `cd_notices.txt` | the three `/cd` outputs | `folder_segment_*`, `cd_*` |
| `print_ok.txt`, `print_not_started.txt` | the text of `print.out` and `print.err`; the JSON line and stderr of the no-gateway run | `print_check_*`, `headless_*` |

### 9.9 Tests in detail

All tests follow P1-13's rules: mocks from recorded output and screens a real Talaria showed; no test or fixture holds an absolute path, home directory, login name or profile name other than `default` or `fixture` (build such needles at run time from a `tempfile` directory); no real pane is used; tests that start `smoke` remove `HERDR_ENV` and `HERDR_WORKSPACE_ID` from the child environment unless they set them. The screen script of P1-13 section 8.2 (and `PH2A-ACC`'s additions) drives `FakeTerminal`; this unit adds screen-script entries and a `Scratch2b` helper under `tests/support/` (additions only).

Write the tests in this order (the independent test author; the first commit holds stubs of every public item whose bodies panic `not implemented`, the screens, the helpers and the tests; every test but the two named in the paragraph on the red run fails for the reason it names, never a compile error or a missing fixture):

1. `screen2b.rs`: 23 functions, in this order: `status_approvals_reads_manual_and_off`, `status_approvals_is_none_without_the_segment`, `picker_state_reads_the_open_picker`, `picker_state_reads_the_no_match_line`, `picker_state_is_closed_on_a_plain_screen`, `context_header_reads_model_and_percent`, `context_header_accepts_the_estimated_form`, `context_header_is_none_before_a_measure`, `context_rows_finds_categories_free_space_and_source`, `usage_block_finds_title_tokens_and_cost`, `usage_block_is_none_without_a_title`, `rewind_list_reads_both_prompts_and_the_selected_one`, `rewind_list_is_closed_on_a_plain_screen`, `rewound_line_finds_the_marker_for_the_prompt`, `rewound_line_ignores_other_prompt_numbers`, `queue_indicator_reads_the_count`, `queue_indicator_is_none_when_empty`, `folder_segment_reads_the_trailing_folder_and_branch`, `lines_after_last_cuts_at_the_last_match`, `lines_after_last_is_empty_without_a_match`, `print_check_accepts_5555_with_empty_standard_error`, `print_check_reads_the_not_started_json_line`, `print_check_rejects_two_json_lines`. Each helper is run on the recorded screens of 9.8 and on one stale or empty screen.
2. `steps2b.rs`: 67 step functions in the order of the steps, then 8 run functions. The step functions are, for each step, `<step>_passes` and one `<step>_<slug>_fails` per row of 9.4 (a hyphen in a step name is an underscore; `permission` adds `permission_skips_when_both_key_names_are_rejected`): `startup_passes`, `context_empty_passes`, `context_empty_context_line_missing_fails`, `context_empty_usage_line_missing_fails`, `permission_passes`, `permission_start_not_manual_fails`, `permission_no_off_segment_fails`, `permission_no_off_notice_fails`, `permission_no_restore_fails`, `permission_skips_when_both_key_names_are_rejected`, `model_picker_passes`, `model_picker_open_missing_fails`, `model_picker_rows_missing_fails`, `model_picker_no_match_missing_fails`, `model_picker_esc_steps_fails`, `model_refusal_passes`, `model_refusal_refusal_missing_fails`, `model_refusal_model_changed_fails`, `rewind_passes`, `rewind_reply_timeout_fails`, `rewind_overlay_missing_fails`, `rewind_list_rows_fails`, `rewind_rewound_missing_fails`, `rewind_composer_not_restored_fails`, `rewind_wrong_code_word_fails`, `rewind_overlay_left_text_fails`, `plain_answer_passes`, `steer_idle_passes`, `steer_idle_notice_missing_fails`, `steer_idle_reply_missing_fails`, `context_passes`, `context_header_missing_fails`, `context_rows_missing_fails`, `context_usage_missing_fails`, `context_aggressive_missing_fails`, `context_argument_missing_fails`, `context_compressed_appeared_fails`, `hermes_commands_passes`, `hermes_commands_fast_missing_fails`, `hermes_commands_loop_missing_fails`, `hermes_commands_loop_line_fails`, `rename_export_passes`, `rename_export_renamed_missing_fails`, `rename_export_title_missing_fails`, `rename_export_exported_missing_fails`, `rename_export_file_bad_fails`, `rename_export_bad_name_missing_fails`, `rename_export_bad_name_wrote_fails`, `queue_passes`, `queue_indicator_missing_fails`, `queue_preview_twice_fails`, `queue_first_missing_fails`, `queue_omega_missing_fails`, `queue_order_wrong_fails`, `queue_indicator_left_fails`, `cd_passes`, `cd_moved_missing_fails`, `cd_segment_wrong_fails`, `cd_show_missing_fails`, `cd_missing_folder_fails`, `quit_passes`, `quit_token_missing_fails`, `headless_passes`, `headless_text_run_fails`, `headless_no_gateway_json_fails`, `headless_no_gateway_text_fails`, `headless_command_timeout_fails`. The run functions: `a_failing_step_stops_the_run_and_cleans_up`, `deadline_names_the_step`, `a_skip_is_never_a_pass`, `live_require_turns_a_skip_into_a_failure`, `step_scale_leaves_quiet_periods`, `steer_idle_and_queue_retry_once_then_fail`, `steps_run_in_the_documented_order`, `shared_steps_behave_as_in_phase_1`. They run `run_smoke_2b` on `FakeTerminal` and `ManualClock` and assert outcomes, reasons (character for character), keys, times and one `close_tab`.
3. `typed_input2b.rs`: 5 functions: `nothing_typed_contains_unattended`, `slash_lines_are_the_allowed_list`, `permissions_line_is_only_session_default`, `no_line_restores_files_or_changes_the_profile`, `prompts_are_the_documented_prompts`. They run the whole suite on the fake and collect every text and key sent (the rule of 9.7).
4. `cleanup2b.rs`: 7 functions: `tab_closed_after_pass`, `tab_closed_after_failure`, `tab_closed_after_signal`, `tab_closed_after_panic`, `unclosable_tab_gives_cleanup_failure`, `scratch_removed_on_every_path`, `exported_file_is_removed_with_the_scratch`.
5. `cli2b.rs`: 8 functions: `help_lists_the_three_suites`, `unknown_suite_is_exit_2`, `default_suite_is_phase1`, `phase2b_default_deadline_is_1500`, `phase2b_preflight_needs_no_tmux`, `phase2b_prints_fifteen_pass_lines_on_fake_herdr`, `phase2b_exit_codes`, `phase2b_failure_report_is_scrubbed`. They run the binary on `fake-herdr`.
6. `fixtures_public_safe2b.rs`: `phase2b_fixtures_hold_no_paths_or_names`: scans the recorded screens.
7. `live_smoke2b.rs`: `live_phase2b_smoke_in_herdr` (`#[ignore]`): a real run; exit 0 and a line matching `^SMOKE_RESULT: PASSED \(elapsed [0-9]+ s\)$`; it fails, never skips, outside Herdr.

The tests of the earlier suites are not edited (AC-2). The red run (the pull request's `Red run` section) is the output of `cargo test -p smoke` on the stubs: every new function fails with `not implemented` or its own assertion, except two that guard behaviour that already exists or is recorded and so pass by design: `cli2b::default_suite_is_phase1` (the default suite is `phase1` already after `PH2A-ACC`) and `fixtures_public_safe2b::phase2b_fixtures_hold_no_paths_or_names` (the recorded screens are committed with the stubs). The `Red run` section lists these two as expected passes; the author never makes them fail artificially. The earlier tests pass.

Three test rules the list leaves open. `steer_idle_and_queue_retry_once_then_fail` runs, for each of the two steps, a script whose first attempt misses and whose second hits (it passes, and the script shows the repeat sent its prompts again only after the screen settled and that the first attempt's `FIRST`, omega and `OMEGA` lines, left on the screen, did not satisfy the repeat), a script with two misses (the step fails with its reason) and a script whose first miss happens while a turn is still working (nothing is sent until it settles). `phase2b_exit_codes` also asserts exit 2 with no `SMOKE_RESULT` line for a `--model` or `--provider` other than the defaults. `print_check_rejects_two_json_lines` asserts that `not_started` is false when `out` holds two JSON lines.

### 9.10 Cleanup, signals, safety

As P1-13 7.2 and `PH2A-ACC` 9.10: the `TabGuard` closes the tab on every path; SIGINT and SIGTERM end the run with `FAIL <step> (interrupted by a signal)` and `SMOKE_RESULT: FAILED (interrupted)`; a tab that cannot be closed gives `FAILED (cleanup)`; the scratch root, including the exported file and the print files, is removed on every path. All printed text goes through P1-13's `Scrubber`; the scratch root becomes `<scratch>`.

### 9.11 README additions

`tools/smoke/README.md` gains a section for `--suite phase2b`: what its 15 steps check, its deadline, that it needs no `tmux`, the operator preconditions (profile `fixture` with `approvals.mode: manual`, two Ollama Cloud models in the picker, `checkpoints.enabled` not needed because only the conversation is rewound), "Reading a failure" rows for the three environment cases (a gateway that is not running, a skipped `permission` step, a model that did not run the 25 s sleep), and the warning that `permission` toggles only the session's bypass and never the profile's mode.

### 9.12 Coverage map: which feature is checked where

| Feature | Smoke step | Source scenarios | Cross-feature rows |
|---|---|---|---|
| Model picker (P2-06) | model-picker, model-refusal | P2-06 UAT-1 to UAT-6 | XF-1, XF-2, XF-8, XF-10, XF-11 |
| Session picker, rename, export (P2-07) | rename-export | P2-07 UAT-1 to UAT-6 | XF-1, XF-4, XF-5, XF-10 |
| Queue and steer (P2-08) | steer-idle, queue | P2-08 UAT-1 to UAT-7 | XF-1, XF-2, XF-3, XF-12 |
| Rewind (P2-09) | rewind | P2-09 UAT-1 to UAT-5 | XF-1, XF-3, XF-4 |
| Context and usage (P2-10) | context-empty, context | P2-10 UAT-1 to UAT-6 | XF-1, XF-8, XF-9 |
| Headless mode (P2-14) | headless | P2-14 UAT-1 to UAT-7 | XF-5 |
| Hermes commands (P2-15) | hermes-commands | P2-15 UAT-1 to UAT-5 | XF-6, XF-9 |
| Permission mode and `/cd` (P2-16) | permission, cd | P2-16 UAT-1 to UAT-6 | XF-1, XF-6, XF-7 |

### 9.13 Journal entries

One `LEARNINGS.md` entry from the first live run of the `phase2b` suite: the Herdr key names that worked for Shift+Tab and Alt+P, the measured step times against 9.6, any read source that lagged (P1-13's H14), and any step whose wording differed from its issue.


## 10. Questions the operator owns, and facts not verified

### 10.1 The shape of the record

The record is a phase gate record, not a release receipt. Draft ADR-0012 ([verification is proportionate](../adr-drafts/0012-verification-is-proportionate.md)) decision 4 reads: "No per-release receipt schemas, digest-bound evidence trees or release-record tests." The record here has no schema or digest, but the checker's tests could be read as release-record tests. This issue takes the shape that `PH2A-ACC` was accepted with, which the lead reads from `PH2A-ACC`'s merged pull request (a `phase-2a.md` in it means shape A; a table in its body means shape B). `PH2A-ACC` defaults to shape B on silence, so with no ruling anywhere shape B applies. `PH1-ACC` defaults the other way (shape A unless declined), so the three records need not match; the lead notes in a comment on the issue which shape each gate used.

- **Shape A (the issue as written):** the four files of section 4, the checker and its 22 cases.
- **Shape B (silence, or a ruling against the checker):** no `phase-2.md`, `phase-2-evidence.md`, `check-phase-2.sh` or `check-phase-2.test.sh`. The 79-row table, its header lines and the evidence sections go in the pull request body, in the same row order and format as section 4 and 5. The lead edits the issue before the freeze: the checker (REQ-9), its tests, the record's red run, AC-4, AC-10, AC-13, UAT-1 and the second half of AC-1 are removed; REQ-7 and REQ-8 describe the body; AUTO-7 drops its four earlier-checker lines for any earlier gate that was recorded in shape B; AC-15 counts `UAT-2` to `UAT-4` (3). AC-5, AC-7, AC-11, AC-12 and AC-14 need no edit: the issue writes them against `$REC` and `$EVI`, which under shape B are both the saved body (AC-5 reads the header only from the lines before the table, `sed -n '1,/^| id |/p'`, so the evidence sections' `Candidate: <7 hex>.` lines never count), `gh pr view "$PR" -R "$R" --json body --jq .body > "$X/body.md"`. A pull request body is limited to 65,536 characters (GitHub's limit). The table (about 6,000 characters) goes in the body; when the evidence sections would not fit, each section goes in a comment on the pull request, in row order, every comment holding whole sections, the body says where they are, and the lead appends the body and then the comments in order to `$X/body.md`.
- Any other shape is an operator edit of the issue before the freeze.

### 10.2 Questions

1. **The shape of the record** (10.1).
2. **The week.** This unit proposes 5 calendar days inside 14, at least 1 hour on each counted day and 12 hours in total (W1), 24 parity lines with minimum uses, and the rule after a fix (2 working days, 4 hours). The roadmap says only "a week". The operator may change any number by editing the issue before the freeze.
3. **The deadline and the XF-9 limits.** 1500 s for the `phase2b` suite (an estimate from step limits, 9.6) and, for the idle-cost row, growth at most 4096 KiB and CPU at most 1 s over two idle minutes (this issue's own sanity checks, as in the earlier gates). Silence means they stand.
4. **The second provider** for XF-11 and OP-4: which provider and which cheap model, and who pays. Without one, XF-11 is BLOCKED (environment) and OP-4 cannot pass, so the candidate cannot be accepted.
5. **XF-6 and XF-7.** XF-6 depends on the `PH2B` planning stage's ruling on who owns the bypass display (`P2-15` decision 9 against `P2-16` decision 12), recorded as a comment on `PH2B` before the freeze; silence means XF-6 stands as written, both displays required. XF-7 rests on a Hermes behaviour that is not verified: whether `shell.exec` and `complete.path` follow `/cd` (section 2.2). If the answer is "the old folder", the row FAILs with the pane text; the operator then rules before the freeze: have `P2-16`'s notice and `P2-05` corrected (a defect through the lead), or drop the row (section 8.1 item 4, with the reason).
6. **Plan mode.** PAR-18 is partial by the parity checklist (Hermes's `/plan` writes a plan and does not execute; a true read-only planning mode is an upstream gap). The operator is expected to mark it `ACCEPTED GAP #<issue>` unless upstream has shipped one; this unit does not decide it.
7. **Whether the `phase2b` smoke suite exists.** It is this unit's proposal (9.1). With no answer by the freeze it is built, because the issue is written with it. Declining drops AUTO-10 and AUTO-11 (77 rows), the third ignored test and section 9, by section 8.1 item 4.

### 10.3 Facts not verified

- The key names Herdr accepts for Shift+Tab (S1) and Alt+P (S2); the first live run records them.
- Whether a dispatched `bench` run reports its check runs on the candidate commit (AUTO-1; checked in stage 0).
- Whether the live gateway applies a replaced folder to `shell.exec` and `complete.path` at once (XF-7; the code reads it, section 2.2).
- Whether the model label in `/context`'s header follows a model switch (XF-8 rests on `P2-10` taking the model from the session's usage figures).
- That `/yolo` (Hermes) and Shift+Tab (`P2-16`) toggle the same session bypass and both displays update within 5 s (XF-6; `P2-16` decision 12 and `P2-15` decision 9).
- That the rewind overlay leaves no text in the terminal's scrollback when read as `recent-unwrapped` (the `rewind` step's last check; `P2-09` states the property, not the read source).
- Every Hermes release-specific text (`/fast`, `/loop`): re-read at the release in `crates/hermes-contract/PINNED`.


## Appendix A: the verbatim scenarios of the accepted issues

Extracted from the final bodies of the eight issues by script on the day this document was written. The source issue's own text wins if it was edited later; if it was, report the difference on this issue and correct this appendix (a correction of the specification is allowed after the freeze). The `Result:` lines are the source issues' own placeholders; the record, not these lines, holds the results.

### A.P2-06: scenarios of P2-06

All: operator preconditions; in a scratch git folder under `$TMPDIR`, run `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` (release build) in a Herdr pane at 120 by 40; read with `herdr pane read <pane> --source visible`. `<M2>` is the first other Ollama Cloud model without `no effort`. First run `/effort`, note `<E>` (not `ultra`). Texts: specification 4.9.

#### UAT-1: see every provider and model
- Persona: developer
- Covers: AC-2, AC-9
- Runner: agent tester in Herdr
- Preconditions: as above.
- Given an idle session with an empty composer
- When the tester types `/model`, Enter, reads; types `zzzz`, reads; presses Esc twice, reads
- Then within 3 s the overlay shows `Switch model · this session`, an `Ollama Cloud` header with `current`, the row `● minimax-m3.1-flash`, a `Not set up (` group and a footer starting `effort:`; then `No model matches "zzzz".`; the first Esc restores the list; the second shows the unchanged transcript and an empty composer
- Evidence: the three pane reads; the time from Enter to the list
- Result: NOT RUN

#### UAT-2: switch model and effort; the profile default is unchanged
- Persona: developer
- Covers: AC-3, AC-4, AC-9
- Runner: agent tester in Herdr
- Preconditions: as above; `P2-03` merged; `jq '.defaults.fixture' ~/.talaria/models.json` prints `null` or fails. First run `talaria --profile fixture` without model flags, note its model `<D>`, `/quit`.
- Given an idle session on `minimax-m3.1-flash`
- When the tester presses Alt+P, Down until the selected row reads `<M2>`, Right, Enter; submits `Reply with the single word ok.`, then `/effort low`; quits; runs `talaria --profile fixture` again
- Then `Model: <M2> on Ollama Cloud. Effort: <level>.` appears (`<level>` one above `<E>`), as does the status line; `ok` arrives; `Effort: low for this session.` appears; the last run shows `<D>` (requirement R4)
- Evidence: pane reads after each step
- Result: NOT RUN

#### UAT-3: switch during a running turn
- Persona: developer
- Covers: AC-4, AC-5, AC-9
- Runner: agent tester in Herdr
- Preconditions: as UAT-2 (manual approval does not stop `sleep 15`).
- Given an idle session on `minimax-m3.1-flash`
- When the tester submits `Run the shell command sleep 15, then reply with the word done in capital letters.`, waits for the working indicator, picks `<M2>` in `/model`, submits `/effort high`
- Then `Model will change to <M2> when the current turn ends.` and `Effort will change to high when the current turn ends.` appear during the turn; the status line keeps `minimax-m3.1-flash` until `DONE`, then within 5 s, with no further prompt, shows `<M2>` and `high`
- Evidence: pane reads during and after; the time from `DONE` to the change
- Result: NOT RUN

#### UAT-4: also use it for new sessions
- Persona: developer
- Covers: AC-6, AC-9
- Runner: agent tester in Herdr
- Preconditions: as above. If `~/.talaria/models.json` exists, copy it to `$TMPDIR/models.json.bak`, then delete `.defaults.fixture` from it with `jq`; if not, note that none existed.
- Given an idle session
- When the tester opens `/model`, Ctrl+G, picks `<M2>`, quits; runs `talaria --profile fixture`, quits; runs it with `--model minimax-m3.1-flash --provider ollama-cloud`, quits; restores the backup (or deletes the file if none existed)
- Then the title read `Switch model · this session and new sessions on fixture`; `Saved for new Talaria sessions on profile fixture: <M2> on Ollama Cloud.` appeared; the second run shows `<M2>`, the third `minimax-m3.1-flash`; before the restore `jq -c '.defaults.fixture' ~/.talaria/models.json` held `"model":"<M2>"` and `"provider":"ollama-cloud"`; after it `cmp` with the backup is silent (no file existed: `test ! -e ~/.talaria/models.json` succeeds)
- Evidence: pane reads; the `jq` and `cmp` output
- Result: NOT RUN

#### UAT-5: an unknown model and bad input are refused (negative)
- Persona: developer
- Covers: AC-3, AC-9
- Runner: agent tester in Herdr
- Preconditions: as above; `P2-03` merged.
- Given an idle session on `minimax-m3.1-flash`
- When the tester submits `/model no-such-model-xyz --provider ollama-cloud`, then `/model --effort high`, then `/effort extreme`
- Then the first shows `Hermes could not switch to no-such-model-xyz: it does not know that model on Ollama Cloud, or the provider is not set up. Nothing changed.` and the model is unchanged; the second and third show the `/model` and `/effort` usage lines
- Evidence: pane reads
- Result: NOT RUN

#### UAT-6: a provider that is not set up (negative)
- Persona: developer
- Covers: AC-2, AC-9
- Runner: agent tester in Herdr
- Preconditions: as above.
- Given the picker open
- When the tester presses End (a `Not set up` row), Enter, then Esc twice
- Then the row shows `set up with hermes model`; Enter shows `<name> is not set up. Set it up with hermes model in a terminal, then press ctrl+r here.` and the picker stays open; Esc closes it; the model is unchanged
- Evidence: pane reads
- Result: NOT RUN

### A.P2-07: scenarios of P2-07

Common setup (operator sets up the gateway): it serves `default` and `fixture`; `fixture` uses `minimax-m3.1-flash` on `ollama-cloud`; `P2-03` is merged. The tester picks a run tag `<tag>` (`openssl rand -hex 2`) so titles never collide, runs `talaria --profile fixture` in a Herdr pane in a scratch folder and sends `say hello`. Input: `herdr pane send-text`, then `herdr pane send-keys enter`; screen: `herdr pane read <pane> --source visible`. Evidence never shows a `default` title: filter on `fixture`, record `default` as a count.

#### UAT-1: Open a session from the list across profiles
- Persona: fleet-lead
- Covers: AC-1, AC-4, AC-11
- Runner: agent tester in Herdr
- Preconditions: common setup; an earlier session (send `count to three`, `/rename fixture count <tag>`, `/quit`); test 39a run on this candidate.
- Given the `say hello` session is in front
- When the tester types `/resume`, notes the `default` count, presses Tab until the header reads `profile fixture`, moves with `j` to `fixture count <tag>` and presses Enter
- Then `fixture` rows show under `LIVE` or `EARLIER`; 39a's `picker_open_median_ms` ≤ 1000 (else FAIL); Enter shows `Left "…"; it keeps running in Hermes. /resume returns to it.`, `── Resumed: … ──` and the `count to three` exchange
- Evidence: pane text; the 39a timing lines; the `default` count
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-2: Rename from the list and with /rename
- Persona: developer
- Covers: AC-5, AC-11
- Runner: agent tester in Herdr
- Preconditions: a fresh common setup (quit, relaunch, send `say hello`).
- Given the new `say hello` session is in front
- When the tester types `/rename fixture picker one <tag>`; then `/resume`, selects that row, `r`, Backspace until empty, `fixture picker two <tag>`, Enter, Esc; then `/rename`
- Then the notices read `Renamed to "fixture picker one <tag>".`, the row then reads `fixture picker two <tag>`, and `/rename` prints `This session is called "fixture picker two <tag>". Type /rename <new title> to change it.`
- Evidence: pane text after each step
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-3: Branch the current session and return
- Persona: developer
- Covers: AC-6, AC-11
- Runner: agent tester in Herdr
- Preconditions: UAT-2 done; the session is idle.
- Given `fixture picker two <tag>` is in front
- When the tester types `/branch`, then `/resume`, then Enter on `fixture picker two <tag>`
- Then `Branched into "fixture picker two <tag> #2" with <n> messages. …` appears; the list shows both sessions, the branch marked `●`; Enter returns to the original with its history
- Evidence: pane text of the notice and the list
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-4: Export to Markdown
- Persona: developer
- Covers: AC-7, AC-11
- Runner: agent tester in Herdr
- Preconditions: UAT-3 done; no `hermes_session_*` file in the folder.
- Given `fixture picker two <tag>` is in front
- When the tester types `/export`, runs `ls -l` and `head -5` on the file in a second pane, then `/export` again
- Then the notice reads `Exported "fixture picker two <tag>" to ./<name> (<size>, Markdown, secrets redacted).`; the file is `-rw-------` and contains `say hello`; the second export writes `<name stem> (2).md`, the first unchanged
- Evidence: notices; `ls -l` (id shown as `<id>`); `head` output
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-5: Refused rename and refused branch (negative)
- Persona: developer
- Covers: AC-5, AC-6, AC-11
- Runner: agent tester in Herdr
- Preconditions: UAT-4 done; then `/resume`, move to `fixture picker two <tag> #2`, Enter.
- Given the branch `fixture picker two <tag> #2` is in front
- When the tester types `/rename fixture picker two <tag>`; then sends `write a 2000 word story, numbering each paragraph`, waits until the turn shows running, and types `/branch`
- Then the rename prints `Hermes could not rename the session. Another session may already use that title; titles must be unique.`, title unchanged; the branch prints `A turn is running. Wait for it to finish or press Esc to stop it, then branch.` while the turn keeps running; afterwards `/resume` shows no new session
- Evidence: pane text of the notices, status line and list
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-6: Cancelled and refused exports write nothing (negative)
- Persona: developer
- Covers: AC-7, AC-11
- Runner: agent tester in Herdr
- Preconditions: UAT-5 done and its story finished.
- Given a `fixture` session is in front and UAT-4's file exists
- When the tester types `/export ./notes.txt`, `/export ./<name>` (UAT-4's file), then `/resume`, `e`, `j`, Esc, Esc, running `ls -l` before and after
- Then the notices read `Use a file name ending in .md, .json or .html.` and `./<name> already exists. Choose another file name.`; the cancelled dialog prints nothing; `ls -l` is unchanged
- Evidence: the notices; both `ls -l` outputs
- Result: PASS / FAIL / BLOCKED / NOT RUN

### A.P2-08: scenarios of P2-08

Common setup: operator preconditions hold; `mkdir -p "$TMPDIR/qproj"`; `herdr pane split --current --direction right --cwd "$TMPDIR/qproj" --no-focus` (pane `P`); `herdr pane run P "talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash"`. "The long prompt" is `herdr pane run P "Run the shell command sleep 25, then reply with the word first in capital letters."`; "queue X" is `herdr pane run P "Reply with the word X in capital letters."` 3 seconds into a turn; "read" is `herdr pane read P --source visible`; "final read" is `herdr pane read P --source recent-unwrapped --lines 80`; "wait for W" is `herdr pane wait-output P --match W --timeout 180000`. Then checks are substring matches. One evidence comment per scenario; a model failure gets one rerun, then the operator (specification 7.2).

#### UAT-1: Queue while the agent works, then watch it run in order
- Persona: developer
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given the long prompt running
- When the tester queues omega, reads, waits for `OMEGA`, and takes a final read
- Then the read shows `queued (1)` and a preview line containing `Reply with the word omega` and no user line for it; the final read shows the omega user line below `FIRST`, then `OMEGA`, and no indicator
- Evidence: both pane reads
- Result: NOT RUN

#### UAT-2: Edit a queued prompt and keep its place
- Persona: developer
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given the long prompt running, alpha then bravo queued
- When the tester runs `herdr pane send-keys P up up enter`, sends 45 `backspace` keys, `herdr pane send-text P "Reply with the word anchor in capital letters."` and `herdr pane send-keys P enter`, reads, waits for `BRAVO`, and takes a final read
- Then the read lists anchor before bravo with `queued (2)`; the final read shows `ANCHOR` above `BRAVO` and no `ALPHA`
- Evidence: both pane reads
- Result: NOT RUN

#### UAT-3: Remove a queued prompt and take it back
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given the long prompt running, charlie queued, an empty composer
- When the tester runs `herdr pane send-keys P up d`, reads, waits for `FIRST`, waits 20 seconds, and takes a final read
- Then the read shows no indicator and the charlie prompt in the composer; the final read has no `CHARLIE`
- Evidence: both pane reads
- Result: NOT RUN

#### UAT-4: Steer a queued prompt, and steer typed text, into the running turn
- Persona: developer
- Covers: AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup; the build includes `P2-03`.
- Given a turn started with `herdr pane run P "Run sleep 15 and then sleep 15 as two separate shell commands. Then reply with the code word I gave you followed by the digits 42 with no space, or NONE."` and `The code word is PELICAN.` queued
- When during the first `sleep` the tester runs `herdr pane send-keys P up s`, reads, and waits for `PELICAN42`; then starts the same turn with nothing queued, runs `herdr pane run P "/steer The code word is HERON."` during its first `sleep`, reads, and waits for `HERON42`
- Then the reads show `⤷ steer: "The code word is PELICAN."` (and no indicator), then `⤷ steer: "The code word is HERON."`; both waits exit 0
- Evidence: pane reads, exit codes
- Result: NOT RUN

#### UAT-5: Move a prompt up and send it now
- Persona: developer
- Covers: AC-10
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given the long prompt running, one, two, three queued
- When the tester runs `herdr pane send-keys P up K`, reads, runs `herdr pane send-keys P n`, waits for `THREE`, and takes a final read
- Then the read shows the order one, three, two; the final read shows the turn interrupted with no `FIRST`, then the three prompt and `THREE` before `ONE`
- Evidence: pane reads; seconds from `n` to `THREE`
- Result: NOT RUN

#### UAT-6: Negative: steer with no running turn, and an empty edit
- Persona: developer
- Covers: AC-11
- Runner: agent tester in Herdr
- Preconditions: common setup; the build includes `P2-03`; the session idle.
- Given an idle session
- When the tester runs `herdr pane run P "/steer Reply with the word idle in capital letters."` and waits for `IDLE`; then sends the long prompt, queues echo, runs `herdr pane send-keys P up enter`, sends 44 `backspace` keys and `enter`, and reads
- Then the pane shows `No turn is running, so this was sent as a new prompt` and `IDLE`, then `An empty edit would remove the prompt. Press d on it to remove it instead.` with echo still previewed and `queued (1)`
- Evidence: pane reads
- Result: NOT RUN

#### UAT-7: Queued prompts survive quit and resume
- Persona: developer
- Covers: AC-12
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given a turn started with `herdr pane run P "Run the shell command sleep 90, then reply with the word first in capital letters."` and delta queued
- When within 30 seconds the tester runs `herdr pane run P "/quit"` and `herdr pane run P "talaria --profile fixture -c"`, reads, waits for `DELTA`, and takes a final read
- Then the resumed screen shows `queued (1)` with delta previewed; the final read shows the delta user line once and `DELTA` exactly once
- Evidence: pane reads
- Result: NOT RUN

### A.P2-09: scenarios of P2-09

Shared setup: in a new temporary folder run `git init` and `printf 'seed\n' > README.md` (an empty folder gets no first checkpoint), then start `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` (`P1-12`) in a Herdr pane; read it with `herdr pane read`. An approval card is not expected (spec H56); if one appears, deny it and record FAIL with its text. "Operator sets up" = the operator preconditions (Dependencies and gates). P1 = `Create a file named notes.txt containing exactly the line alpha. Use your file writing tool, not the terminal. Reply with the word done.`; P2 = `In notes.txt replace alpha with beta using your patch or file tool, not the terminal. Reply with the word done.`

#### UAT-1: Rewind the conversation with Esc twice
- Persona: developer
- Covers: AC-1, AC-2, AC-6
- Runner: agent tester in Herdr
- Preconditions: operator sets up; tester: the shared setup.
- Given a session where the tester submitted `Remember the code word heron. Reply ok.` and then `Now remember the code word lark instead. Reply ok.`, and both replies finished
- When the tester sends `esc` twice about 200 ms apart on the empty composer, presses Enter on `#2`, `c`, then Enter
- Then `Rewind · choose a prompt to go back to` lists `#2` (selected) and `#1`; then a line starting `── Rewound to before prompt #2` appears and the composer holds prompt 2; after one Ctrl+C (clears the draft, `P1-08`) and submitting `Which code word should you remember? One word.`, the reply contains `heron`, not `lark`; no overlay text in scrollback
- Evidence: pane reads after each step
- Result: NOT RUN

#### UAT-2: Restore files and rewind
- Persona: developer
- Covers: AC-1, AC-2, AC-5, AC-6
- Runner: agent tester in Herdr
- Preconditions: operator sets up; tester: the shared setup.
- Given a session where the tester submitted P1 and then P2, and `cat notes.txt` in another pane prints `beta`
- When the tester types `/rewind`, selects `#2`, presses Enter, `f`, then Enter
- Then Actions shows `f  Also restore files to checkpoint`, an 8-character hash and `before patch` or `before write_file`; after confirming, a line starting `── Restored files to checkpoint` appears; `cat notes.txt` prints `alpha`; `/rewind` lists only `#1`
- Evidence: pane reads; the two `cat` outputs
- Result: NOT RUN

#### UAT-3: See what changed
- Persona: developer
- Covers: AC-3, AC-6
- Runner: agent tester in Herdr
- Preconditions: operator sets up; tester: the shared setup, then P1 and P2.
- Given the files changed by P1 and P2
- When the tester types `/diff`, presses Esc, types `/rollback`, presses Enter on row 1, then Esc twice
- Then `/diff` shows `Changes since the oldest kept checkpoint`, a stat naming `notes.txt` and a `+` line `beta`; `/rollback` shows at least two rows with hash, time and `before ...`; Enter on row 1 shows `Checkpoint <hash> compared with the files now`; no overlay text in scrollback
- Evidence: pane reads of each view
- Result: NOT RUN

#### UAT-4: Refusals change nothing (negative)
- Persona: developer
- Covers: AC-1, AC-2, AC-6
- Runner: agent tester in Herdr
- Preconditions: operator sets up; tester: the shared setup, then P1 and P2.
- Given an idle session with checkpoints
- When the tester submits `/rollback 99`, `/rollback diff zzz`, `/undo 9`, `/rollback diff`, then `Run the shell command sleep 15, then reply done.` and, while it runs, types `/rewind`
- Then the screen shows a line starting `Hermes did not restore anything: Invalid checkpoint number. Use 1-`, `There is no checkpoint zzz. /rollback lists them; number 1 is the newest.`, `There are only 2 prompts to undo.`, the `/rollback` usage line, and `A reply is running. Stop it with Esc, then rewind.`; no overlay opens; `cat notes.txt` still prints `beta`
- Evidence: pane reads; the `cat` output
- Result: NOT RUN

#### UAT-5: Rewound in another window, then `/undo` and `/retry`
- Persona: developer
- Covers: AC-2, AC-4, AC-6
- Runner: agent tester in Herdr
- Preconditions: operator sets up (above); tester: the shared setup in pane A; pane B runs `talaria --resume <session id> --profile fixture` on the same session.
- Given both panes show a session with three short prompts (`Reply one.`, `Reply two.`, `Reply three.`) and their replies
- When the tester submits `/undo` in pane B, reads pane A, then submits `/retry` in pane A
- Then pane B shows a line starting `── Rewound to before prompt #3` and `Reply three.` in its composer; pane A shows one line starting `── This session was rewound in another window`; after `/retry`, pane A shows `Reply two.` again with a new reply, and pane B shows that same line and the new reply once
- Evidence: pane reads of both panes after each step
- Result: NOT RUN

### A.P2-10: scenarios of P2-10

Live rules (spec section 11, binding): run `$T --profile fixture --model minimax-m3.1-flash --provider ollama-cloud` (`T=<checkout>/target/debug/talaria`) in a fresh Herdr pane `P<n>` of at least 100 columns and 40 rows (`tput cols; tput lines`), in a fresh folder `$TMPDIR/ctxproj-<n>`. Type with `herdr pane send-text P<n> '<text>'`, then `herdr pane send-keys P<n> enter`; read with `herdr pane read P<n> --source visible`. Empty reads, a wait timeout, an unexpected approval card (deny it) or a connection error is BLOCKED (environment). Never set `unattended`. Results go on the pull request.

#### UAT-1: /context shows what fills the window
- Persona: developer
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: live rules; operator sets up the `fixture` profile.
- Given Talaria is idle in `P1` after replying to `Reply with only the word ready.`
- When `/context` is submitted and the pane read
- Then, in order: a line starting `Context · minimax-m3.1-flash · ` and ending `tokens (<n>%)`; five lines of twenty glyphs; `Estimated usage by category`; rows with `System prompt` and `Conversation`; `Free space`; a line starting `Source: `
- Evidence: the pane read
- Result: NOT RUN

#### UAT-2: /usage and /cost show the session's figures
- Persona: developer
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1, in pane `P2`.
- Given Talaria is idle after two replies
- When `/usage` is submitted and read, then `/cost` is submitted and read
- Then each block has `Usage · this session`, a `Tokens` row with `input` and `total`, `API calls` of 2 or more, and a `Cost` row (`not reported by Hermes for this session` or a dollar amount); `Plan` only when signed in to Nous Portal
- Evidence: both reads
- Result: NOT RUN

#### UAT-3: Preview, then compact a long conversation
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: as UAT-1, in pane `P3`; sixteen turns `Reply with only the word <one … sixteen>.` answered first (spec 11).
- Given Talaria is idle and the status line shows `ctx <n>%`
- When `/compact --preview` is submitted and read; then `/compact focus on the numbers`, reading every 2 s (at most 11 minutes) until a line contains `✓ Compressed`, `No changes from compression`, `Compression aborted`, `Compression refused` or `No answer from Hermes`; then `Reply with only the word after.`
- Then the preview read shows `Preview — no changes made.` and a line matching `^ *Would compress`; a read during the compaction shows `✻ Compacting conversation` (else the evidence notes the result came within 2 s); the result shows `✓ Compressed: <a> → <b> messages` with `a > b`, a line starting `Approx request size:` and `your scrollback above is unchanged`; no `ctx` segment from the result until the `after` reply, one after it. `✓ Compressed with fallback`, `Compression aborted`, `Compression refused` or `No changes from compression` is BLOCKED (environment); the T-C13 text is FAIL
- Evidence: every read with its time
- Result: NOT RUN

#### UAT-4: Refusals change nothing (negative)
- Persona: developer
- Covers: AC-9
- Runner: agent tester in Herdr
- Preconditions: as UAT-1, in pane `P4`.
- Given Talaria is idle
- When `Count from 1 to 300, one number per line.` is submitted; `herdr pane wait-output P4 --regex '^\s*20\s*$' --source visible --timeout 120000` succeeds; `/compact` is submitted at once; after the reply, `/compact --aggressive`; then `/context now`
- Then (read at once when the T-C2 line appears) the first result is `Hermes cannot compact while the agent is working. Wait for the reply to finish, or press Esc to stop it, then run /compact again.`, shown after the reply's last line; the second is `Hermes does not support --aggressive. Nothing was changed.`; the third is `/context does not take an argument.`; no `✓ Compressed` line anywhere. If `300` was on screen before `/compact` was sent, or a compaction headline appears, the run is BLOCKED (environment); retry once counting to 1000.
- Evidence: the reads, including one just before `/compact`
- Result: NOT RUN

#### UAT-5: Nothing to measure before the first prompt (negative)
- Persona: developer
- Covers: AC-10
- Runner: agent tester in Herdr
- Preconditions: as UAT-1, in a fresh pane `P5`, nothing submitted.
- Given a new session with no turns
- When `/context` is submitted, then `/usage`
- Then the first shows `Context · not measured yet` and `Hermes builds this session's agent on the next prompt; until then there is nothing to break down.`; the second shows `No API calls yet in this session.` and the `Cost` row
- Evidence: the read
- Result: NOT RUN

#### UAT-6: Automatic compaction is shown, then recorded once
- Persona: developer
- Covers: AC-11
- Runner: agent tester in Herdr
- Preconditions: the branch built; no gateway.
- Given a shell at the repository root
- When `cargo run -q -p talaria-tui --example context_demo -- --width 80 crates/talaria-core/tests/data/context/auto_compaction_frames.jsonl > "$TMPDIR/demo.txt"` runs
- Then the file holds `live| ✻ Compacting context · 0s` once, then a `live| ` line with no compacting text, then exactly one `scrollback| Context was compacted automatically during this turn.` after the turn's end; `grep -c $'\x1b' "$TMPDIR/demo.txt"` prints `0`
- Evidence: `$TMPDIR/demo.txt`
- Result: NOT RUN

### A.P2-14: scenarios of P2-14

Run in `bash`, after `cargo build --release -p talaria-cli` and `export PATH="$PWD/target/release:$PATH"`; `command -v talaria` must name `target/release/talaria` (`<talaria>`). `<flags>` is `--profile fixture --provider ollama-cloud --model minimax-m3.1-flash`; `<project>` is from `mktemp -d`. Type with `herdr pane run <pane> '<command>'`; read with `herdr pane read <pane> --source recent --lines 80` (a read lacking the command is stale). Redact paths. Record `UAT-<n>: PASS` (or FAIL, BLOCKED, NOT RUN) with evidence in the pull request.

#### UAT-1: A plain answer in a pipeline
- Persona: scripter
- Covers: AC-5
- Runner: agent tester in Herdr
- Preconditions: operator sets up the fixture profile and gateway.
- Given a shell at `<project>`
- When the tester runs `<talaria> -p <flags> "What is 1234 plus 4321? Reply with only the number." 2>err.txt | tr -d '\n'; echo " exit=${PIPESTATUS[0]}"; wc -c < err.txt`
- Then the line reads `5555 exit=0` and the byte count of `err.txt` is `0`
- Evidence: pane text, wall time of the command
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: JSON a script can parse
- Persona: scripter
- Covers: AC-5
- Runner: agent tester in Herdr
- Preconditions: as UAT-1; `jq` installed.
- Given a shell at `<project>`
- When the tester runs `<talaria> -p <flags> --output-format json "What is 1234 plus 4321? Reply with only the number." | tee out.json | jq -r '.status, .exit_code, .text, (.session_id|length>0), (.usage|keys|length)'`
- Then the five lines are `completed`, `0`, `5555`, `true`, `9`; `out.json` has exactly one line
- Evidence: pane text, `out.json` with the session id kept for UAT-4
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: Piped text joins the prompt
- Persona: scripter
- Covers: AC-5
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given a shell at `<project>`
- When the tester runs `printf 'alpha\nbeta\ngamma\n' | <talaria> -p <flags> "How many lines of text follow this sentence? Reply with only the number."; echo "exit=$?"`
- Then the output is `3` then `exit=0`
- Evidence: pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: Continue a scripted session by id
- Persona: scripter
- Covers: AC-5
- Runner: agent tester in Herdr
- Preconditions: UAT-2's `out.json`.
- Given a shell at `<project>`
- When the tester runs `<talaria> -p --profile fixture --resume "$(jq -r .session_id out.json)" "What number did you give me before? Reply with only the number."; echo "exit=$?"`, then `<talaria> -p -c --profile fixture "hello"; echo "exit=$?"`
- Then the first prints `5555` and `exit=0`; the second prints `talaria: no earlier session found for this folder.` on standard error and `exit=4`
- Evidence: pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: No gateway and bad input give clean failures (negative)
- Persona: scripter
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: none (the first command points at an empty Hermes home).
- Given a shell
- When the tester runs `HERMES_HOME=$(mktemp -d) <talaria> -p --no-ensure --output-format json "hello"; echo "exit=$?"`, then `<talaria> -p "" </dev/null; echo "exit=$?"`, then `<talaria> --timeout 5; echo "exit=$?"`
- Then the first prints one JSON line with `"status":"not_started"` and `"exit_code":4`, standard error starting `talaria: no Hermes gateway is running for this home` with a `Next step:` line, and `exit=4`; the second prints `talaria: no prompt: give it after -p or on standard input.` and `exit=2`; the third prints `talaria: --timeout only works with -p.` and `exit=2`; no path or escape garbage
- Evidence: pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-6: A dangerous command is declined, not waited on (negative)
- Persona: scripter
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: operator sets `approvals.mode: manual` on `fixture` and leaves `approvals.single_query_mode` at `deny`; `<project>` holds `.pytest_cache/x`. Without them: BLOCKED (environment), unit not done.
- Given a shell at `<project>`
- When the tester runs `time <talaria> -p <flags> --record rec.jsonl "Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache"; echo "exit=$?"; ls -a; grep -c '"approval.request"' rec.jsonl; grep -c BLOCKED rec.jsonl`
- Then the command ends within 120 seconds with exit 0 or 1, no approval question appears, `ls -a` still lists `.pytest_cache`, the first count is `0`, the second is `1` or more, and the answer says the command was blocked or declined. An answer that never tried the command is FAIL, not PASS
- Evidence: pane text with the `time` line; the tool events of `rec.jsonl`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-7: A time limit stops a long turn
- Persona: scripter
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given a shell at `<project>`
- When the tester runs `time <talaria> -p <flags> --timeout 5 "Count from 1 to 3000, one number per line."; echo "exit=$?"`
- Then it ends within 30 seconds (the 5-second limit plus the 25-second stop budget), the last line is `exit=124`, and standard error shows `talaria: the time limit passed; the turn was stopped.` followed by `Resume this session with:`
- Evidence: pane text with the `time` line
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P2-15: scenarios of P2-15

All scenarios: `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` in a scratch git project in a temporary folder, Herdr pane 120×40, read with `herdr pane read <pane> --source visible`. Never send a prompt while a `⚠ YOLO` line is visible. Quoted Hermes replies are its v0.22.4 wording; a changed wording at the pinned release is not a Talaria failure if shown verbatim.

#### UAT-1: A goal shows above the composer and its verdict lands in the transcript
- Persona: developer
- Covers: AC-1, AC-4, AC-7
- Runner: agent tester in Herdr
- Preconditions: operator sets up the `fixture` profile with a working goal judge; Talaria started as above.
- Given an idle session with an empty composer
- When the tester submits `/goal Reply with the single word ok.`, waits for the reply and up to 60 more seconds, then submits `/goal clear`
- Then at once a cell shows `⊙ Goal set (20-turn budget): Reply with the single word ok.` with no `**`, and `⊙ goal · 0/20 turns · Reply with the single word ok.` appears above the composer; after the reply a System line starts `✓ Goal achieved` (the goal line goes), `↻ Continuing toward goal` or `⏸ Goal paused`; `/goal clear` shows `✓ Goal cleared.` or (after achieved) `No active goal.`, and no goal line remains
- Evidence: pane reads after each step
- Result: NOT RUN

#### UAT-2: A loop runs, then counts down without needless redraws
- Persona: developer
- Covers: AC-1, AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1 (no goal judge needed); no goal set
- Given an idle session
- When the tester submits `/loop 45m reply with the word tick`, reads, waits up to 60 s for the wakeup turn, reads, waits 75 s, reads twice 3 s apart, submits `/loop pause`, `/loop stop`, `/heartbeat every 1h say hi`, reads, then `/heartbeat clear`
- Then the first cell starts `↻ Loop set (every 45m): reply with the word tick` and contains `First wakeup fires now`; the loop line shows `0 runs · due, waiting for idle` or `running now`; after the turn `↻ loop · every 45m · 1 run · next in 45m · reply with the word tick` (or `44m`); 75 s later one or two minutes lower, and the two reads 3 s apart match (on a minute step, repeat once); `/loop pause` shows `⏸ loop paused · user-paused · 1 run · …`; `/loop stop` shows `✓ Loop stopped.` and the line goes; the heartbeat line reads `♥ heartbeat · every 1h · 0 sent · next in 1h00m · say hi` and goes after `/heartbeat clear`
- Evidence: every pane read with its time
- Result: NOT RUN

#### UAT-3: YOLO is shown while it is on (and turned off again)
- Persona: developer
- Covers: AC-1, AC-4, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1
- Given an idle session with no `⚠ YOLO` line
- When the tester submits `/yolo`, reads the pane, then submits `/yolo` again
- Then after the first: a cell `⚡ YOLO mode ON for this session — all commands auto-approved. Use with caution.` with `ON` bold and no `**`, and a yellow `⚠ YOLO · commands run without approval in this session` line; after the second: a cell `⚠ YOLO mode OFF for this session — dangerous commands will require approval.` and no YOLO line. Cleanup, even on FAIL: while a `⚠ YOLO` line shows, submit `/yolo` until the OFF cell appears
- Evidence: pane reads and a screenshot after the first `/yolo`
- Result: NOT RUN

#### UAT-4: `/fast` on a model without priority processing changes nothing (negative)
- Persona: developer
- Covers: AC-4, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1
- Given an idle session
- When the tester submits `/fast`, then `/fast fast`, then `/loop 45m`
- Then each `/fast` shows the cell `⚡ /fast is only available for OpenAI models that support Priority Processing.` and no fast line appears; `/loop 45m` shows `/loop: missing prompt (usage: /loop [interval] <prompt>)` with `<prompt>` visible, and no loop line appears
- Evidence: pane read
- Result: NOT RUN

#### UAT-5: `/plan` is explained and resumes cleanly
- Persona: developer
- Covers: AC-4, AC-5, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1; no `⚠ YOLO` line visible
- Given an idle session
- When the tester submits `/plan write a haiku about rain`, waits for the reply, submits `/plan write a limerick`, waits, then `/quit`, and in a new pane runs `talaria --resume <id> --profile fixture`
- Then after the first `/plan` one System line starts `Plan mode is an instruction to the agent, not a sandbox` (not after the second); in the resumed pane the user lines read `/plan write a haiku about rain` and `/plan write a limerick`, and no line contains `[/plan — plan mode]` or `Task to plan:`
- Evidence: pane reads before and after resuming; the session id
- Result: NOT RUN

### A.P2-16: scenarios of P2-16

All: in a scratch git folder `$TMPDIR/p216/a` (sibling git folder `$TMPDIR/p216/b`, both on `main`), run `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` (release build) in a Herdr pane at 120 by 40; read with `herdr pane read <pane> --source visible`; send Shift+Tab with `herdr pane send-keys <pane> shift+tab` (if rejected, `backtab`; record which worked, U6; if Herdr rejects both, the scenario is BLOCKED (environment) with Herdr's error text). Precondition for all: the operator's profile `fixture` (`approvals.mode: manual`, local terminal backend). Texts: specification 4.8. Scrub evidence before posting it: home folder to `~`, the `$TMPDIR` prefix to `$TMPDIR`, user and host names to `<user>`, `<host>`.

#### UAT-1: see the setting and switch it for this session
- Persona: developer
- Covers: AC-1, AC-5, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above.
- Given an idle new session
- When the tester reads; presses Shift+Tab, reads; types `/quit`, Enter, relaunches with `--resume <session id>`, reads; presses Shift+Tab, reads
- Then the status line shows `approvals: manual`; then `approvals: off (this session)` with a notice starting `Approvals off for this session only` and naming `Shift+Tab`; after the resume still `approvals: off (this session)`; then `approvals: manual` with `Approvals back on for this session: it follows profile fixture (manual).`
- Evidence: four pane reads; the session id; time from key press to segment change
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-2: move the session with /cd
- Persona: developer
- Covers: AC-2, AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above.
- Given an idle session in folder `a`
- When the tester types `/cd ../b`, Enter, reads; sends `Run pwd with your shell tool and reply with only its output.`, waits, reads; types `/cd`, Enter, reads
- Then `Moving this session to`, then `This session now works in` and a path ending `/p216/b (main)`, folder segment ending `b (main)`; the reply ends `/p216/b`; `/cd` alone shows `This session works in` with that path
- Evidence: three pane reads
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-3: show the settings; refuse bad input (negative)
- Persona: developer
- Covers: AC-1, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above.
- Given an idle session
- When the tester types `/permissions`, Enter; `/permissions banana`, Enter; `/permissions off`, Enter, waits 35 seconds without repeating; reads after each
- Then lines beginning `Approvals in this session: manual.`, `Profile fixture: manual.`, `This session: follows the profile.`; then `Usage: /permissions [manual | smart | off | session off | session default]`; then `This turns approvals off for every session of profile fixture, background sessions included. Run /permissions off again within 30 seconds to confirm.` and still `approvals: manual` after 35 seconds
- Evidence: three pane reads; the status line after 35 seconds
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-4: change the profile's mode and see it in another session
- Persona: configurer
- Covers: AC-1, AC-8
- Runner: operator
- Preconditions: as above; two panes, each with its own new session on `fixture`.
- Given both panes show `approvals: manual`
- When in pane 1 the operator types `/permissions smart`; in pane 2 sends `Reply with the word ok.`, waits, then runs Hermes's `/approvals manual`; in pane 1 sends `Reply with the word ok.`
- Then pane 1 shows `Approvals for every session of profile fixture: smart.` and `approvals: smart` at once; pane 2 shows `approvals: smart` after its turn ends, then `approvals: manual` after `/approvals manual`; pane 1 shows `manual` after its turn
- Evidence: pane reads at each step; the profile ends at `manual`
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-5: refusals for /cd (negative)
- Persona: developer
- Covers: AC-2, AC-7
- Runner: agent tester in Herdr
- Preconditions: as above; a file `$TMPDIR/p216/a/notes.txt`.
- Given an idle session in folder `a`
- When the tester types `/cd ./missing-folder-p216`; `/cd notes.txt`; `/cd ~nobody`; sends `Count from 1 to 300, one number per line.` and while it streams types `/cd ../b`; presses Esc; after the turn stops types `/cd ../b`
- Then in order: `No folder at` (path ending `missing-folder-p216`); a line ending `notes.txt is a file, not a folder.`; `Talaria does not expand ~name. Give the full path.`; `A turn is running. Wait for it to finish, or press Esc to stop it, then run /cd again.`; the folder segment stays on `a` until the last command moves it to `b`
- Evidence: pane reads after each command
- Result: PASS / FAIL / BLOCKED / NOT RUN

#### UAT-6: what approvals off actually does
- Persona: developer
- Covers: AC-5, AC-8
- Runner: operator
- Preconditions: as above; folder `a` holds an empty subfolder `scratch-p216`.
- Given an idle session showing `approvals: manual`
- When the operator sends `Delete the folder scratch-p216 with rm -r.`, presses Shift+Tab with the card open, denies it (Esc); presses Shift+Tab, sends it again; presses Shift+Tab, recreates the folder, sends it again; with that card open types `/permissions session off`, Enter, denies the card; types `/permissions session default`, Enter
- Then a card, and Shift+Tab leaves the card and `approvals: manual` unchanged; then no card and the folder is gone; then the card stays open after `/permissions session off` until denied (U3); whether a later flagged command in that turn ran without a card is recorded (U2); finally `approvals: manual`
- Evidence: pane reads; `ls` of folder `a` before and after
- Result: PASS / FAIL / BLOCKED / NOT RUN
