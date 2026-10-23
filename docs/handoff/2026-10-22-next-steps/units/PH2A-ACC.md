# PH2A-ACC: Phase 2a acceptance: extend the smoke suite to every phase 2a feature and run it on the candidate

Status: `proposed`. The GitHub issue for this unit is the contract; this document is its reference specification. If they disagree, the issue wins and the disagreement is reported on the issue.

## How to use this document

Read the issue first. It says what is accepted, in what order, and how the result is recorded. This document holds what the people doing the work would otherwise have to guess: the facts about `herdr` and `tmux` the smoke extension relies on, how to freeze and build the candidate, the exact record and evidence templates, how to scrub evidence, the complete checker script and its tests (adapted from phase 1's and run while drafting, on macOS), the exact steps and pass conditions of the 11 cross-feature scenarios and the operator session, the run order and the rules for failures, the full design of the smoke extension (files, signatures, every step with its exact failure text, the screens to record, the tests), and the questions the operator still owns. Appendix A holds the verbatim text of the 48 scenarios of the accepted issues, extracted from their final bodies by script; the source issue's own text wins if it was edited after this document was written.

Sections are numbered so the issue can point at them. Handoff documents are linked relatively: [the roadmap's phase 2](../07-roadmap-and-work-units.md#phase-2--coding-parity), [the P1-13 roadmap entry](../07-roadmap-and-work-units.md#p1-13--live-smoke-test-in-herdr), [the Claude Code parity checklist](../04-prior-art.md#claude-code-parity-checklist), [performance budgets](../05-architecture.md#performance-budgets), [why the Python client's acceptance ceremony failed](../02-current-state.md#why-it-ended-up-here), [driving Talaria through Herdr by hand](../research/u13-hands-on-talaria-drive.md). The pattern copied is [PH1-ACC](PH1-ACC.md); the smoke suite extended is [P1-13](P1-13.md).

## 1. What this unit is for, in plain terms

Eight children of the phase 2a capability each passed their own scenarios on their own commit: tool cells and diffs (P2-01), the transcript pager (P2-02), the slash palette (P2-03), mentions and attachments (P2-04), shell commands and side questions (P2-05), the task checklist (P2-11), themes (P2-12) and the comforts (P2-13). A passing child proves its parts. What no child proves is the seams: P2-03 and P2-04 share four completion keys and must never both open a list; P2-04 and P2-05 share the submit path; P2-02's pager has to draw the cells P2-01 and P2-05 add; P2-13 hands the terminal to an editor and takes it back while P2-04's paste labels sit in the draft. Those seams are tested only here, on one commit, against the live gateway.

The unit does three things. It extends the smoke suite of P1-13 so that a 21-step run drives every phase 2a feature in a real pane and fails with the screen text when one is wrong. It reruns every user acceptance scenario of the eight issues (48) and adds 11 scenarios that only exist when features combine. And it records the result in two markdown files, checked by a short script.

The roadmap's phase 2 operator check is a "switch day": the operator uses Talaria instead of Claude Code for a week of Hermes-backed coding. It covers phases 1 and 2 together, so it belongs to PH2-ACC, the acceptance child of the phase 2b capability, which depends on this unit. This issue defines none. The operator's part is seven items run in one sitting: the six source scenarios no agent can run (a desktop clipboard, real terminals, a throwaway skill, a skin switch, notifications) and the keyboard run of P2-11 UAT-3, which that scenario's own Runner line asks the operator to make outside Herdr.

The Python client spent 44 percent of the lines it added on acceptance records ([02-current-state.md](../02-current-state.md#why-it-ended-up-here), item 4). This unit therefore keeps the ceremony small: two markdown files and one checker of about 55 lines (the same shape as PH1-ACC, which carries the argument about draft ADR-0012 and the repository reset; the shape is the lead's proposal and the operator confirms it on the parent issue before the candidate is frozen, otherwise the fallback of section 10 applies). Draft ADR-0012 decision 4 reads in full: "No per-release receipt schemas, digest-bound evidence trees or release-record tests." This record has no schema, digest or signature. Whether the checker's own tests count as "release-record tests" is the operator's ruling; section 10 says what happens either way. The only other code is the smoke extension, which is a tool, not a record.

## 2. Facts

### 2.1 The `herdr` and `tmux` command lines

The facts H1 to H14 of [P1-13](P1-13.md#21-the-herdr-command-line-verified-by-running-it-herdr-094-macos-2026-10-22) apply unchanged (Herdr 0.9.4, checked by running it on macOS on 2026-10-22; re-check with the Herdr in use and write `herdr --version` in the record header). Three bear on this unit: `pane run` types text and Enter together; `pane read` has sources `visible`, `recent` and `recent-unwrapped`; a character sent right after `esc` may be lost, so wait 400 ms after `send-keys esc`. Whether `visible` or `recent-unwrapped` is stale for a full-screen program is P1-13's H14 **verify** item, settled by its first live run and recorded in its journal entry: read that entry before recording screens, and follow it. The smoke suite reads the source section 9.3 names for each step; the manual rows (XF) run `echo READ-CHECK` in the pane first and use the source that shows it. If a source lags or is empty for Talaria, stop and report it; never swap a source silently to make a step pass.

New facts for this unit:

| # | Fact | How checked |
|---|---|---|
| T1 | `tmux` 3.7c lists `load-buffer [-b buffer-name] [-t target-client] path`, `paste-buffer [-dprS] [-s separator] [-b buffer-name] [-t target-pane]`, `has-session [-t target-session]`, `kill-session [-aCg] [-t target-session]`, `new-session [-AdDEPX] [-c start-directory] [-e environment] ... [-s session-name] ... [shell-command ...]`, `capture-pane [-aeFHLpPqCJMN] [-b buffer-name] [-E end-line] [-S start-line] [-t target-pane]`, `send-keys [-FHKlMRX] ... [-t target-pane] [key ...]` | `tmux list-commands`, 2026-10-22. `load-buffer` and `paste-buffer` print nothing on success, so their exact argument lists in the tests come from this command list (T1) and from T5, not from recorded output |
| T2 | `paste-buffer`: "If -p is specified, paste bracket control codes are inserted around the buffer if the application has requested bracketed paste mode"; `-r` does no linefeed replacement (the default replaces LF with CR); `-d` deletes the buffer afterwards; control characters are sanitized with `vis(3)` unless `-S` | `man tmux` |
| T3 | `tmux has-session -t <name>` exits 1 when there is no such session; with no server running it prints `error connecting to <socket path> (No such file or directory)` and exits 1. That text holds a local path: a recorded fixture replaces it with `<tmp>/...` | run |
| T4 | `capture-pane -p` prints the pane text; `-S -` starts at the beginning of the history; `-e` keeps colour escape sequences (used by XF-7) | `man tmux` |
| T5 | The source scenarios paste with `tmux load-buffer -b <name> <file>` then `tmux paste-buffer -p -r -d -b <name> -t <session>` (P2-04 shared setup); this unit uses the same two commands | P2-04 body |
| T6 | `herdr pane get <pane_id>` exists ("Show a pane", Herdr 0.9.4 help). **Unverified:** that its output names the reported agent and its state. P2-13 UAT-5 relies on it and falls back to recording the Herdr sidebar | P2-13 spec D13 |
| T7 | Not verified: whether Herdr's `pane run` or `send-text` delivers text as a bracketed paste. Talaria's composer treats unbracketed fast input as a paste burst that is never collapsed (P2-04 REQ-9), so the paste steps cannot rely on Herdr; they use tmux | P1-13 spec 2.1; P2-04 body |

### 2.2 Hermes facts used (read at Hermes v0.22.4, commit 4d04303; re-check at the release in `crates/hermes-contract/PINNED` before relying on them)

- `approvals.mode` is `manual`, `smart` or `off`; the default is `smart` and the default `timeout` is 300 s (`hermes_cli/config_defaults.py` lines 1694 and 1695 at Hermes v0.22.4). `smart` lets a guardian model approve a flagged command with no card; a hosted-room policy overrides the configured mode (`tools/approval_context.py` lines 263 to 302).
- `rm -rf .pytest_cache` matches the recursive-delete pattern `\brm\s+-[^\s]*r` (`tools/approval_detection.py` line 230); so does `rm -r ./keepme`. The `!` shell route refuses whatever that detector flags, in any approval mode (P2-05 REQ-3).
- An unanswered approval is denied after `approvals.timeout` (default 300 s).
- The `!` route is Hermes's `shell.exec`: it returns error 4005 (`blocked: <description>. Use the agent for dangerous commands.`) for whatever `detect_dangerous_command` flags, with no approval-mode check (`tui_gateway/methods_tools.py` lines 1869 to 1885 at Hermes v0.22.4). The wording `Hermes refused to run this: it looks dangerous` is Talaria's own text (P2-05).
- `prompt.submit` accepts `unattended` only together with `finite: true`, and then auto-approves the turn (`gateway/session_finite.py` lines 34 to 57, `tools/approval.py` lines 325 to 333). Nothing here sends it.
- `file.attach` for a file outside the session folder copies it into the profile's `attachments/` folder (P2-04 REQ-12), so the `file-drop` step leaves one small uniquely named file there per run.
- `/quit` in Talaria detaches and sends no `end_reason`; the session keeps running in the gateway (P1-12 settled decision 1).

### 2.3 What the source issues provide

Screen texts are each owned by one source issue and are listed with their sources in section 9.5. The demos and ignored live tests are named in section 7.1. Two behaviours the scenarios rely on and that are not Hermes facts: Ctrl+C clears a non-empty draft (P1-08 settled decision 1, the rule XF-2 of PH1-ACC relied on) and Esc interrupts a running turn (P1-08, the key P2-01 UAT-4 sends).

## 3. Freezing and building the candidate

The smoke extension (section 9) is built and merged first, on branch `p2a-acc-smoke`, by a test author and a developer; it is part of the candidate. The lead freezes after the eight children, P1-13 and the extension are merged and closed. Never switch the primary checkout (it may hold untracked material); work in a worktree outside the repository.

```bash
git fetch origin
CANDIDATE=$(git rev-parse origin/main)                    # 40 hex characters; written in the record header
WT=$(mktemp -d)/p2a-acc                                    # a new folder outside the repository
git worktree add "$WT" "$CANDIDATE" && cd "$WT"
git push origin "$CANDIDATE:refs/heads/p2a-acc-candidate"  # for the bench runs of AUTO-1; deleted at closeout
cargo build --release --workspace --examples
cargo build --release -p talaria-cli -p talaria-bench
shasum -a 256 target/release/talaria                       # the binary every row uses
git switch -c p2a-acc-record                               # the record branch starts at the candidate commit
```

The record branch holds documentation commits only (AC-6 proves it). After a new candidate (a fix) the branch is rebased onto it. `main` may move during the run: the record is compared with the candidate, never with `main`.

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

`--no-ensure` is on every launch that starts a live session, so Talaria can never start a gateway. Preflight before stage 1 (once, with the Herdr in use): `herdr --version`, `tmux -V`; run both pane-split forms (the source scenarios' `herdr pane split --current --direction right --cwd . --no-focus` and the helper's) and close the panes they make. A form Herdr rejects is reported on the issue and allowed by REQ-11 change (4). If a code path changes after this point the candidate is dead: start again from the top of this section.

## 4. The record

File `docs/acceptance/v0.7.0/phase-2a.md`. The first commit holds this skeleton with every header line filled in with its real value (the checker refuses a header that still holds an angle-bracket placeholder; the values below are examples) and `phase-2a-evidence.md` holding only its title line, so the first run of the real checker is the red run: 69 problems. Commit order: the checker tests with the stub (`test(acceptance): ...`), the empty record (`docs(acceptance): add the empty phase 2a record`), then the real checker (`docs(acceptance): add the phase 2a checker`); at the empty-record commit the stub still prints `FAILED (stub)`, so the 69-problem run is taken at the checker commit, while the record is still all NOT RUN. Row order is the order below; do not add other tables.

```markdown
# Phase 2a acceptance record

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
| P2-01 UAT-1 | developer | NOT RUN |  |  |
| P2-01 UAT-2 | developer | NOT RUN |  |  |
| P2-01 UAT-3 | developer | NOT RUN |  |  |
| P2-01 UAT-4 | developer | NOT RUN |  |  |
| P2-01 UAT-5 | developer | NOT RUN |  |  |
| P2-01 UAT-6 | developer | NOT RUN |  |  |
| P2-02 UAT-1 | developer | NOT RUN |  |  |
| P2-02 UAT-2 | developer | NOT RUN |  |  |
| P2-02 UAT-3 | developer | NOT RUN |  |  |
| P2-02 UAT-4 | developer | NOT RUN |  |  |
| P2-02 UAT-5 | developer | NOT RUN |  |  |
| P2-02 UAT-6 | developer | NOT RUN |  |  |
| P2-03 UAT-1 | developer | NOT RUN |  |  |
| P2-03 UAT-2 | developer | NOT RUN |  |  |
| P2-03 UAT-3 | developer | NOT RUN |  |  |
| P2-03 UAT-4 | developer | NOT RUN |  |  |
| P2-03 UAT-5 | developer | NOT RUN |  |  |
| P2-03 UAT-6 | configurer | NOT RUN |  |  |
| P2-03 UAT-7 | developer | NOT RUN |  |  |
| P2-04 UAT-1 | developer | NOT RUN |  |  |
| P2-04 UAT-2 | developer | NOT RUN |  |  |
| P2-04 UAT-3 | developer | NOT RUN |  |  |
| P2-04 UAT-4 | developer | NOT RUN |  |  |
| P2-04 UAT-5 | developer | NOT RUN |  |  |
| P2-05 UAT-1 | developer | NOT RUN |  |  |
| P2-05 UAT-2 | developer | NOT RUN |  |  |
| P2-05 UAT-3 | developer | NOT RUN |  |  |
| P2-05 UAT-4 | developer | NOT RUN |  |  |
| P2-05 UAT-5 | developer | NOT RUN |  |  |
| P2-11 UAT-1 | developer | NOT RUN |  |  |
| P2-11 UAT-2 | developer | NOT RUN |  |  |
| P2-11 UAT-3 | developer | NOT RUN |  |  |
| P2-11 UAT-4 | developer | NOT RUN |  |  |
| P2-11 UAT-5 | developer | NOT RUN |  |  |
| P2-11 UAT-6 | developer | NOT RUN |  |  |
| P2-12 UAT-1 | developer | NOT RUN |  |  |
| P2-12 UAT-2 | developer | NOT RUN |  |  |
| P2-12 UAT-3 | developer | NOT RUN |  |  |
| P2-12 UAT-4 | configurer | NOT RUN |  |  |
| P2-12 UAT-5 | developer | NOT RUN |  |  |
| P2-12 UAT-6 | configurer | NOT RUN |  |  |
| P2-13 UAT-1 | developer | NOT RUN |  |  |
| P2-13 UAT-2 | developer | NOT RUN |  |  |
| P2-13 UAT-3 | developer | NOT RUN |  |  |
| P2-13 UAT-4 | developer | NOT RUN |  |  |
| P2-13 UAT-5 | fleet-lead | NOT RUN |  |  |
| P2-13 UAT-6 | developer | NOT RUN |  |  |
| P2-13 UAT-7 | developer | NOT RUN |  |  |
| XF-1 | developer | NOT RUN |  |  |
| XF-2 | developer | NOT RUN |  |  |
| XF-3 | developer | NOT RUN |  |  |
| XF-4 | developer | NOT RUN |  |  |
| XF-5 | developer | NOT RUN |  |  |
| XF-6 | developer | NOT RUN |  |  |
| XF-7 | developer | NOT RUN |  |  |
| XF-8 | fleet-lead | NOT RUN |  |  |
| XF-9 | developer | NOT RUN |  |  |
| XF-10 | developer | NOT RUN |  |  |
| XF-11 | developer | NOT RUN |  |  |
```

Rules: `result` is exactly `PASS`, `FAIL`, `BLOCKED (<cause>)` (causes: `environment`, `not built`, `no access`) or `NOT RUN`. A PASS row carries the first 7 or more characters of the candidate in `commit` and the link `phase-2a-evidence.md#<id lower case, spaces to hyphens>` in `evidence`; the row `P2-03 UAT-1` links `phase-2a-evidence.md#p2-03-uat-1`. A row that is not PASS may leave `evidence` and `commit` empty but its evidence section should say why. BLOCKED and NOT RUN never count as a pass; there is no waiver state. Personas and runners are those of the source scenarios (Appendix A); AUTO rows are `builder`.

## 5. The evidence file

### 5.1 Shape

`docs/acceptance/v0.7.0/phase-2a-evidence.md` starts with `# Phase 2a acceptance evidence` and holds one section per row, in record order:

````markdown
## P2-03 UAT-1

Candidate: <first 7 characters>. Runner: agent tester. Run 1 of 1. Date: 2026-11-02.

```text
<scrubbed output or pane text, at most 60 lines>
```
````

The checker requires, for every PASS row, the `Candidate:` line (7 or more hex characters equal to the start of the candidate, then a full stop or space), exactly one section, and a fenced block with at least one non-blank line and at most 60 lines. The reviewer checks what the script cannot: the runner and date line, the run list, the three-run limit, the operator's comment URL, the row order and persona column, and that every row that is not PASS says why. An operator row holds the operator's words (what they did, the terminals, the result), the URL of the operator's comment on the issue and, for `P2-12 UAT-4`, the read-back of the restored skin.

### 5.2 What to capture, per row kind

AUTO rows: the command and the last 15 lines of its output (the result lines). Source rows: what the source scenario's Evidence line asks for. XF rows: what section 7.3 names. Smoke rows (AUTO-8 to AUTO-10): the whole output including every step line and the `SMOKE_RESULT:` line, with repeated `NOTE` lines removed (the only lines ever removed), and both tab lists. When the scrubbed output is still over 60 lines, write it as consecutive fenced blocks of at most 60 lines each in the same section, in order, none cut in the middle of a line: the checker measures each block, not the section (6.2: `blk` restarts at every opening fence), and AC-10 reads the `SMOKE_RESULT:` line from whichever block holds it. Screenshots are attached to the operator's comment, not committed.

### 5.3 Scrubbing (before anything is pasted)

```bash
scrub() {
  sed -e "s#$HOME#<home>#g" -e "s#$(id -un)#<user>#g" \
      -e "s#${TMPDIR%/}#<tmp>#g" -e "s#$(cd "${TMPDIR:-/tmp}" && pwd -P)#<tmp>#g" \
      -e "s#$PWD#<worktree>#g"
}
herdr pane read "$PANE" --source recent-unwrapped --lines 200 | scrub > "$X/evidence.txt"
```

Then read the result: no home directory, user name (a login name under four characters cannot be scrubbed safely: say so in the section), temporary path, host name, provider key or ticket, and no profile name other than `default` or `fixture`; replace any other with `<profile>`. Two shapes are new in phase 2a: the Herdr pane title and `herdr pane get` output may hold a working folder, and the P2-13 history scenario shows prompts from the operator's own history file, which may hold earlier prompts that are not the tester's; read them and cut any that are not. The checker catches paths only; AC-11 also greps `--profile` values and each runner's login name. A clean grep proves only that those shapes are absent: the reading step is the control.

## 6. The checker and its tests

### 6.1 Behaviour

`check-phase-2a.sh [record [evidence]]` (defaults: `phase-2a.md` and `phase-2a-evidence.md` next to the script). It holds the 69 row ids in one `IDS=` line. It prints one line per problem, then `ACCEPTANCE_RESULT: PASSED (<n> of 69 rows PASS, candidate <7 hex>)` or `ACCEPTANCE_RESULT: FAILED (<p> problems, <n> of 69 rows PASS)`. Exit 0 when there are no problems, 1 otherwise, 2 when a file is missing. Problems: a row that is not PASS (the line is `<id>: <result>`); a result that is not one of the four forms; a PASS row whose commit is not a prefix of the candidate or is shorter than 7 characters; a PASS row whose evidence link is not the expected one, whose section is missing, empty or longer than 60 lines, or does not name the candidate; a missing, extra or duplicate row; a section that appears twice; a header line that is missing or still a placeholder; a local path anywhere in either file.

The script is a copy of PH1-ACC's checker (its spec 6.2) with the file names, the id list and the count changed, so the two stay comparable. Rejected: one parametrised script for both phases, because it would reopen a closed gate's tool; the cost of the copy is about 55 lines. If a command-saving proxy wraps shell commands on the machine, run the checker and its tests through the proxy's pass-through mode or in a plain pane and paste the raw output.

### 6.2 Reference implementation

Written for POSIX `sh` and `awk`; no brace counts in regular expressions; the backtick fence written `\140\140\140`. Exercised while drafting on macOS (awk version 20200816): the 22 cases below pass, the stub fails every one of them on its stated expectation, and the skeleton record of section 4 ends `FAILED (69 problems, 0 of 69 rows PASS)`. Not run with GNU `awk` or `mawk`: run the cases on a Linux machine before recording `OS: linux`.

```sh
#!/bin/sh
# Check the Phase 2a acceptance record. Usage: check-phase-2a.sh [record.md [evidence.md]]
# Prints one line per problem, then ACCEPTANCE_RESULT: PASSED or FAILED. Exit 0, 1 (not accepted) or 2 (file missing).
DIR=$(dirname "$0")
REC=${1:-$DIR/phase-2a.md}
EVI=${2:-$DIR/phase-2a-evidence.md}
[ -f "$REC" ] && [ -f "$EVI" ] || { echo "check-phase-2a: record or evidence file not found"; exit 2; }
PATHS=$(cat "$REC" "$EVI" | grep -cE '/(Users|home|private|var|tmp)/[A-Za-z]')
[ "$PATHS" -eq 0 ] || echo "a local path appears on $PATHS line(s) of the record or the evidence"
IDS='AUTO-1|AUTO-2|AUTO-3|AUTO-4|AUTO-5|AUTO-6|AUTO-7|AUTO-8|AUTO-9|AUTO-10|P2-01 UAT-1|P2-01 UAT-2|P2-01 UAT-3|P2-01 UAT-4|P2-01 UAT-5|P2-01 UAT-6|P2-02 UAT-1|P2-02 UAT-2|P2-02 UAT-3|P2-02 UAT-4|P2-02 UAT-5|P2-02 UAT-6|P2-03 UAT-1|P2-03 UAT-2|P2-03 UAT-3|P2-03 UAT-4|P2-03 UAT-5|P2-03 UAT-6|P2-03 UAT-7|P2-04 UAT-1|P2-04 UAT-2|P2-04 UAT-3|P2-04 UAT-4|P2-04 UAT-5|P2-05 UAT-1|P2-05 UAT-2|P2-05 UAT-3|P2-05 UAT-4|P2-05 UAT-5|P2-11 UAT-1|P2-11 UAT-2|P2-11 UAT-3|P2-11 UAT-4|P2-11 UAT-5|P2-11 UAT-6|P2-12 UAT-1|P2-12 UAT-2|P2-12 UAT-3|P2-12 UAT-4|P2-12 UAT-5|P2-12 UAT-6|P2-13 UAT-1|P2-13 UAT-2|P2-13 UAT-3|P2-13 UAT-4|P2-13 UAT-5|P2-13 UAT-6|P2-13 UAT-7|XF-1|XF-2|XF-3|XF-4|XF-5|XF-6|XF-7|XF-8|XF-9|XF-10|XF-11'
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
  if (evid != "phase-2a-evidence.md#" slug(id)) bad(id ": the evidence link must be phase-2a-evidence.md#" slug(id))
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

The test script builds a good record and variations of it in a temporary folder, runs the checker on each and compares the exit code and one expected text. It prints `CASE <name>: ok` or `CASE <name>: FAILED (...)` and a final `CHECK_RECORD_TESTS: PASSED` or `CHECK_RECORD_TESTS: FAILED (<n>)`. It reads the id list out of the checker, so the two cannot drift; `all-pass` expects the text `69 of 69`, so a dropped id fails it.

| # | Case | Builds | Expects (exit; text) |
|---|---|---|---|
| 1 | `all-pass` | 69 PASS rows, a section each | 0; `ACCEPTANCE_RESULT: PASSED (69 of 69 rows PASS, candidate <7 hex>)` |
| 2 | `not-run-fails` | `P2-13 UAT-6` NOT RUN | 1; `P2-13 UAT-6: NOT RUN` |
| 3 | `blocked-fails` | `AUTO-5` `BLOCKED (environment)` | 1; `AUTO-5: BLOCKED (environment)` |
| 4 | `fail-fails` | `XF-2` FAIL | 1; `XF-2: FAIL` |
| 5 | `wrong-commit` | `P2-03 UAT-2` commit `deadbeefdead` | 1; `is not the candidate` |
| 6 | `lower-case-result` | `AUTO-1` result `pass` | 1; `the result must be` |
| 7 | `row-missing` | `XF-11` deleted | 1; `XF-11: row missing` |
| 8 | `unexpected-row` | an added row `ZZ-9` | 1; `unexpected row: ZZ-9` |
| 9 | `duplicate-row` | `AUTO-1` twice | 1; `AUTO-1: appears twice` |
| 10 | `no-candidate` | `Candidate: TBD` | 1; `Candidate: <40 hex` |
| 11 | `no-evidence-section` | no section for `P2-11 UAT-6` | 1; `P2-11 UAT-6: the evidence file has no section` |
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

The checker ignores heading-like lines inside a fenced block and counts a duplicated section; this is PH1-ACC's spec 6.2 and its issue now says 22 cases as well, so the two agree. If they ever disagree again, report it on PH1-ACC; this unit follows the script.

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
# Tests for check-phase-2a.sh. Run: sh docs/acceptance/v0.7.0/check-phase-2a.test.sh   Prints CHECK_RECORD_TESTS: PASSED or FAILED.
DIR=$(cd "$(dirname "$0")" && pwd)
CHECK="$DIR/check-phase-2a.sh"
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
      echo "| $id | developer | $res | phase-2a-evidence.md#$slug | $com |"
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
make_record "$T/good.md" none PASS;                    expect all-pass 0 "ACCEPTANCE_RESULT: PASSED (69 of 69 rows PASS, candidate $S)" "$T/good.md" "$T/ev.md"
make_record "$T/a.md" "P2-13 UAT-6" "NOT RUN";               expect not-run-fails 1 "P2-13 UAT-6: NOT RUN" "$T/a.md" "$T/ev.md"
make_record "$T/b.md" "AUTO-5" "BLOCKED (environment)"; expect blocked-fails 1 "AUTO-5: BLOCKED (environment)" "$T/b.md" "$T/ev.md"
make_record "$T/c.md" "XF-2" FAIL;                     expect fail-fails 1 "XF-2: FAIL" "$T/c.md" "$T/ev.md"
make_record "$T/d.md" "P2-03 UAT-2" PASS deadbeefdead; expect wrong-commit 1 "is not the candidate" "$T/d.md" "$T/ev.md"
make_record "$T/e.md" "AUTO-1" pass;                   expect lower-case-result 1 "the result must be" "$T/e.md" "$T/ev.md"
grep -v '^| XF-11' "$T/good.md" > "$T/f.md";            expect row-missing 1 "XF-11: row missing" "$T/f.md" "$T/ev.md"
{ cat "$T/good.md"; echo '| ZZ-9 | developer | PASS | x | y |'; } > "$T/g.md"; expect unexpected-row 1 "unexpected row: ZZ-9" "$T/g.md" "$T/ev.md"
{ cat "$T/good.md"; grep '^| AUTO-1 ' "$T/good.md"; } > "$T/h.md"; expect duplicate-row 1 "AUTO-1: appears twice" "$T/h.md" "$T/ev.md"
sed "s/^Candidate: .*/Candidate: TBD/" "$T/good.md" > "$T/i.md"; expect no-candidate 1 "Candidate: <40 hex" "$T/i.md" "$T/ev.md"
make_evidence "$T/ev2.md" "P2-11 UAT-6";               expect no-evidence-section 1 "P2-11 UAT-6: the evidence file has no section" "$T/good.md" "$T/ev2.md"
sed 's#phase-2a-evidence.md\#xf-1#elsewhere.md#' "$T/good.md" > "$T/j.md"; expect wrong-evidence-link 1 "XF-1: the evidence link must be" "$T/j.md" "$T/ev.md"
{ cat "$T/ev.md"; printf 'see /%s/someone/project\n' Users; } > "$T/ev3.md"; expect local-path 1 "a local path appears" "$T/good.md" "$T/ev3.md"
expect missing-file 2 "not found" "$T/nope.md" "$T/ev.md"
make_evidence "$T/ev4.md" "AUTO-2"; section AUTO-2 "" >> "$T/ev4.md"; expect empty-evidence-block 1 "AUTO-2: the evidence file has no section" "$T/good.md" "$T/ev4.md"
make_evidence "$T/ev5.md" "AUTO-2"; section AUTO-2 "$(seq 1 61)" >> "$T/ev5.md"; expect long-evidence-block 1 "AUTO-2: a fenced block of the evidence is longer than 60 lines" "$T/good.md" "$T/ev5.md"
make_record "$T/k.md" "AUTO-3" PASS 012345;            expect short-commit 1 "AUTO-3: commit 012345 is not the candidate" "$T/k.md" "$T/ev.md"
{ cat "$T/good.md"; printf 'see /%s/someone/project\n' tmp; } > "$T/l.md"; expect local-path-in-record 1 "a local path appears" "$T/l.md" "$T/ev.md"
sed 's/^Hermes: .*/Hermes: <pinned release tag>/' "$T/good.md" > "$T/m.md"; expect placeholder-header 1 "Hermes: the header line is still a placeholder" "$T/m.md" "$T/ev.md"
sed 's/^Candidate: 0123456\./Candidate: deadbee./' "$T/ev.md" > "$T/ev6.md"; expect evidence-names-another-candidate 1 "AUTO-1: the evidence section must name the candidate" "$T/good.md" "$T/ev6.md"
make_evidence "$T/ev7.md" none; section AUTO-2 ok >> "$T/ev7.md"; expect duplicate-section 1 "AUTO-2: the evidence file has 2 sections for it" "$T/good.md" "$T/ev7.md"
mkdir "$T/d" && cp "$CHECK" "$T/d/" && cp "$T/good.md" "$T/d/phase-2a.md" && cp "$T/ev.md" "$T/d/phase-2a-evidence.md"
out=$(sh "$T/d/check-phase-2a.sh"); code=$?
if [ "$code" = 0 ] && echo "$out" | grep -qF "ACCEPTANCE_RESULT: PASSED"; then echo "CASE default-arguments: ok"; else echo "CASE default-arguments: FAILED (exit $code; got: $out)"; fails=$((fails+1)); fi

[ "$fails" -eq 0 ] && echo "CHECK_RECORD_TESTS: PASSED" || { echo "CHECK_RECORD_TESTS: FAILED ($fails)"; exit 1; }
````

The red run of the record (the issue's "red run of the record"), taken at the commit that adds the real checker while the committed record is still the skeleton of section 4 with the real candidate and real header lines: `sh check-phase-2a.sh` must end `ACCEPTANCE_RESULT: FAILED (69 problems, 0 of 69 rows PASS)` and exit 1 (checked while drafting).

## 7. The scenario catalogue

Every row has an id, a persona, a runner, steps, an observable result and the evidence to capture. Runner words: `agent tester` is an agent working in a Herdr pane; `operator` is the human. All commands run at the worktree root unless a row says otherwise. `R=infiquetra/talaria`, `X=$(mktemp -d)` for evidence scratch.

### 7.1 AUTO rows (persona builder; agent tester)

Each row lists its commands, what makes it PASS and what to paste as evidence.

**AUTO-1: project check and the candidate's continuous integration (CI) results.** The `bench` workflow runs on `pull_request` and `workflow_dispatch` (P1-11 REQ-20). GitHub attaches every workflow run to its head commit as check runs, so a dispatch run on the candidate branch should report on the candidate; this is **verified at the first use** (if it does not, the row is BLOCKED (environment) and reported on P1-11). The lead can learn the answer before the freeze: in stage 0 dispatch `bench.yml` on the branch `p2a-acc-smoke` and read `gh api "repos/$R/commits/<its head sha>/check-runs"`; if no `bench` check run is attached, report it on P1-11 then, so the cause is fixed before the candidate exists (a workflow fix after the freeze makes a new candidate).

```bash
gh workflow run bench.yml --ref p2a-acc-candidate -R "$R"
# poll every 10 s, at most 5 minutes, until a run whose headSha is the candidate exists (an older run on the branch does not count)
RUN=$(gh run list -R "$R" --workflow bench.yml --branch p2a-acc-candidate --json databaseId,headSha --jq ".[] | select(.headSha == \"$CANDIDATE\") | .databaseId" | head -1)
gh run watch "$RUN" -R "$R" --exit-status
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test --workspace
gh api --paginate "repos/$R/commits/$CANDIDATE/check-runs" --jq '.check_runs[] | "\(.name) \(.conclusion)"'
shasum -a 256 target/release/talaria
```

PASS: `gh run watch` exits 0 on the run for the candidate; the cargo set exits 0; every check-run line ends `success` or `skipped` (a line ending `null` is still running: wait and read again; `failure`, `cancelled`, `timed_out` and `action_required` are a FAIL) and `bench success` is among them. Evidence: run id and `headSha`, exit codes, the check-run lines, the checksum.

**AUTO-2: replay suites** (tests that play recorded Hermes frames back).

```bash
cargo test -p talaria-app --test tool_replay
cargo test -p talaria-tui --test pager_replay
cargo test -p talaria-app --test commands_replay
cargo test -p talaria-app --test attachments_replay
cargo test -p talaria-app --test side_replay
cargo test -p talaria-tui --test checklist_replay
cargo test -p talaria-app --test appearance_replay
cargo test -p talaria-app --test attention_replay
```

PASS: each exits 0 and prints `^test result: ok\. [1-9][0-9]* passed` (a target that ran no test prints `0 passed` and is a FAIL). A target that does not exist at the candidate is `BLOCKED (not built)`. Evidence: the `test result` lines.

**AUTO-3: the eight issues' unit suites** (every test target the issues name, except the replay and live ones).

```bash
cargo test -p talaria-core --test diff_parse --test tool_view --test tool_generating
cargo test -p talaria-tui --test tool_cells
cargo test -p talaria-tui --test pager_search --test pager_nav --test pager_render --test pager_prop
cargo test -p talaria-app --test pager_wiring --test pager_keys
cargo test -p talaria-core --test session_history_truncated
cargo test -p talaria-core --test commands_parse --test commands_catalog --test commands_local --test commands_palette --test commands_outcome --test commands_state
cargo test -p talaria-app --test commands_mock --test palette_routing
cargo test -p talaria-tui --test palette_render --test keys_completion
cargo test -p talaria-core --test mention_query --test composer_chips --test drop_path
cargo test -p talaria-app --test attachments_service --test attachments_routing --test clipboard_reader
cargo test -p talaria-tui --test mention_popup
cargo test -p talaria-core --test side_classify --test side_text --test side_reduce
cargo test -p talaria-tui --test side_render
cargo test -p talaria-app --test side_service
cargo test -p talaria-core --test checklist_model --test checklist_tracker --test checklist_snapshot
cargo test -p talaria-tui --test checklist_render --test checklist_keys
cargo test -p talaria-app --test checklist_wiring --test config_doc
cargo test -p talaria-tui --test keymap_doc
cargo test -p talaria-tui --lib keys::tests::defaults_are_safe_and_unambiguous
cargo test -p talaria-core --test appearance
cargo test -p talaria-tui --test theme_color --test theme_detect --test theme_resolve --test theme_probe --test theme_snapshots --test no_blink
cargo test -p talaria-app config::
cargo test -p talaria-app --test appearance_flow
cargo test -p talaria-core --test attention
cargo test -p talaria-tui --test comfort_keys --test history_search --test alert_bytes --test release --test escape_custody
cargo test -p talaria-app --test prompt_history --test editor --test herdr --test suspend --test config_comforts
CI=1 cargo test -p talaria-tui --test comforts_pty -- --nocapture
```

PASS: every command exits 0, and a command naming k `--test` targets prints at least k lines `test result: ok. N passed` with N at least 1 (a filter-only command prints at least one). `CI=1` makes a missing job control fail instead of skip (P2-13 AC-4). A target that no longer exists is `BLOCKED (not built)`. Evidence: the `test result` lines and exit codes. The tests marked `#[ignore]` are run by AUTO-4 and AUTO-5 or by the operator.

**AUTO-4: performance budgets on the real release binary and the units' own benchmarks.**

```bash
target/release/talaria-bench run --binary target/release/talaria --report "$X/bench.json" > "$X/bench.txt"; echo "exit=$?"
grep -cE ' FAIL$' "$X/bench.txt"; grep -cE ' PASS$' "$X/bench.txt"
cargo test -p talaria-bench --test real_binary release_budgets_hold -- --ignored
cargo bench -p talaria-tui --bench tool_cells
cargo bench -p talaria-tui --bench pager
cargo bench -p talaria-core --bench palette_filter
cargo test --release -p talaria-app --test attachments_budget -- --ignored
```

PASS: `exit=0`, the FAIL count 0, the PASS count 1 or more; the ignored test, the three benches (each exits 0; the `pager` table has every row `pass`; `palette_filter` prints `p95_us` under 2000 at 2,000 entries) and `attachments_budget` pass. Evidence: the `bench.txt` table from its `os:` line, the two counts, the `p95_us` line, the exit codes. Budgets: [05-architecture.md](../05-architecture.md#performance-budgets); the units' own: P2-01 REQ-27, P2-02 REQ-21, P2-03 REQ-25, P2-04 REQ-21.

**AUTO-5: live suites with no skip.** Operator preconditions hold; each command runs in its own shell.

```bash
LIVE=$(mktemp -d)
cargo run -q -p record-fixture -- record --out "$LIVE" --scenario tool-diff --scenario approval --profile fixture --model minimax-m3.1-flash --provider ollama-cloud   # exit 1 after 3 attempts: BLOCKED (environment)
TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_FRAMES="$LIVE" cargo test -p talaria-app --test live_tool_cells -- --ignored --nocapture
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_pager -- --ignored --nocapture
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-app --test live_commands -- --ignored --nocapture
cargo test -p talaria-cli --test live_attachments -- --ignored --nocapture 2>&1 | tee "$X/att.txt"
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_side -- --ignored --nocapture
LIVE2=$(mktemp -d)
cargo run -q -p record-fixture -- record --out "$LIVE2" --scenario todo-checklist --profile fixture --model minimax-m3.1-flash --provider ollama-cloud
TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_FRAMES="$LIVE2" cargo test -p talaria-tui --test live_checklist -- --ignored --nocapture
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_skin live_handshake_skin_resolves -- --ignored --nocapture
TALARIA_LIVE_REQUIRE=1 cargo test -p talaria-cli --test live_comforts -- --ignored --nocapture
```

PASS (each test has its own signal): the six lines `LIVE_TOOL_CELLS_RESULT: PASSED`, `LIVE_PAGER_RESULT: PASSED`, `LIVE_SIDE_RESULT: PASSED`, `LIVE_CHECKLIST_RESULT: PASSED`, `LIVE_SKIN_RESULT: PASSED` and `LIVE_COMFORTS_RESULT: PASSED` each appear once (`live_tool_cells`, `live_pager`, `live_side`, `live_checklist`, `live_skin`, `live_comforts`); `live_commands` exits 0 and prints five timed result lines (P2-03 AC-9); `grep -q 'test result: ok' "$X/att.txt" && ! grep -q '^SKIPPED:' "$X/att.txt"` exits 0 (P2-04 AC-4); no `SKIPPED` anywhere. A skip, or a recorder that exits 1 or 2, is `BLOCKED (environment)`, never a pass. The live skin-switch test (`TALARIA_LIVE_SKIN_SWITCH=1`, `live_skin_switch_round_trip`) writes the profile's skin: it is the operator's, with row `P2-12 UAT-4`. Evidence: the result lines.

**AUTO-6: hygiene scans.**

```bash
cargo run -q -p record-fixture -- scan fixtures/frames
! git grep -nE '"(clipboard\.paste|image\.attach|image\.detach|input\.detect_drop|pdf\.attach|paste\.collapse)"' -- 'crates/talaria-*/src'
! git grep -nE 'unsafe *(\{|fn |impl )' -- crates tools
! git grep -nE '/(Users|home|private|var)/[A-Za-z]' -- crates tools
! git grep -n 'unattended' -- tools/smoke ':!tools/smoke/tests/typed_input.rs' ':!tools/smoke/tests/typed_input2a.rs' ':!tools/smoke/README.md'
! git grep -nE 'end_reason|unattended' -- crates/talaria-app/src crates/talaria-cli/src
git grep -nE 'config\.set' -- crates/talaria-app/src crates/talaria-cli/src
! git grep -hoE -e '--profile[ =]+"?[A-Za-z0-9_-]+' -- crates tools | sed -E 's/.*[ =]"?//' | grep -vxE 'default|fixture|nosuchprofile'
! grep -n '"unattended"' fixtures/frames/slash-palette.jsonl fixtures/frames/shell-and-btw.jsonl
test "$(grep -c talaria-uat fixtures/frames/slash-palette.jsonl)" -eq 0
test "$(head -1 fixtures/frames/slash-palette.jsonl | grep -c synthetic)" -eq 0
test "$(grep -cE '^(transcript_open|pager_[a-z_]+) *=' docs/configuration.md)" -eq 16
test "$(wc -l < docs/configuration.md)" -lt 200
grep -q 'paste_collapse_lines' docs/configuration.md && grep -q 'attach_clipboard_image' docs/configuration.md && grep -q 'completion_accept' docs/configuration.md && grep -q 'toggle_tasks' docs/configuration.md && grep -q 'reduced_motion' docs/configuration.md
grep -q 'at most once per second' docs/themes.md
```

PASS: the `!` lines print nothing (positive control for the first scan: `printf '%s\n' '"image.attach"' | grep -cE '"(clipboard\.paste|image\.attach|image\.detach|input\.detect_drop|pdf\.attach|paste\.collapse)"'` prints 1; the verbs are the legacy attach methods that P2-04's settled decision 1 replaces with `file.attach` and `image.attach_bytes`; `nosuchprofile` is allowed in the profile scan because P1-01 and P1-12 type it as a deliberately unknown name); every other line exits 0; the one `config.set` line prints only hits inside the code that sends the `/skin <name>` write of P2-12 REQ-16 (`key: "skin"`), each hit read and listed in the evidence, and any other hit is a FAIL (phase 1's scan forbade `config.set` in these folders; from phase 2a the `/skin` command writes one setting through it, and P2-05 and P2-13 forbid it for themselves). `slash-palette.jsonl` must be a recording, not the synthetic file (P2-03 AC-7). The unsafe scan covers the whole tree; any finding is a FAIL reported on this issue, and a new candidate follows. Evidence: the command lines that printed something, and `0` counts.

**AUTO-7: smoke suite offline tests.**

```bash
cargo test -p smoke
cargo test -p smoke -- --list --ignored 2>/dev/null | grep ': test$'
cargo run -q -p smoke -- run --help
```

PASS: all tests pass; the ignored list is exactly two lines naming `live_full_smoke_in_herdr` and `live_phase2a_smoke_in_herdr`; the help lists `--talaria`, `--herdr`, `--profile`, `--model`, `--provider`, `--deadline-secs`, `--step-scale`, `--report-dir`, `--suite` and `--tmux`. Evidence: the `test result` line, the ignored lines, the help.

**AUTO-8, AUTO-9, AUTO-10: the smoke suites, back to back at the end of stage 1** with no other live scenario between them (they share one gateway and one free model).

```bash
herdr tab list --workspace "$HERDR_WORKSPACE_ID"
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --talaria "$T" > "$X/s1.txt" 2>&1; echo "exit=$?"                       # AUTO-8
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2a --talaria "$T" > "$X/s2.txt" 2>&1; echo "exit=$?"   # AUTO-9
TALARIA_LIVE_REQUIRE=1 cargo run -q -p smoke -- run --suite phase2a --talaria "$T" > "$X/s3.txt" 2>&1; echo "exit=$?"   # AUTO-10
herdr tab list --workspace "$HERDR_WORKSPACE_ID"; tmux ls 2>&1 | grep -c talaria-smoke
```

PASS: AUTO-8 prints six `PASS` lines (P1-13's steps: startup, plain-answer, approval, interrupt, quit, resume) and `SMOKE_RESULT: PASSED (elapsed <n> s)` with n under 300; AUTO-9 and AUTO-10 each print 21 `PASS` lines and the same result line with n under 1800 (the deadline, section 9.6); every `exit=0`; no `talaria-smoke-` tab in the second list and the last count is 0. The defaults are P1-13's (profile `fixture`, model `minimax-m3.1-flash`, provider `ollama-cloud`), so no such flag is passed, and P1-13 makes the `--talaria` path absolute itself (the commands pass `"$T"`, which is already absolute). A suite prints `PASSED WITH SKIPS` and exits 3 when `pane-state` or `checklist` skips, and with `TALARIA_LIVE_REQUIRE=1` a skip fails the run; neither is a pass: the row is BLOCKED (environment). Section 8.1 gives the rule for the row's label. So a model that never uses its todo tool in two attempts, or a `herdr pane get` that shows no agent state (T6), blocks AUTO-9 and AUTO-10 until the operator fixes the cause or, before the freeze, drops the step by editing the issue (settled decision 2). The suites pass `--no-ensure` to `talaria` themselves. Evidence: each whole output (section 5.2) and both tab lists.

### 7.2 Source rows (48 scenarios)

Run each as written in Appendix A, with the four allowed changes of the issue's REQ-11:

1. `target/release/talaria` (the absolute `$T`) wherever a scenario says `talaria`, `<talaria>`, `<run>` or the debug path (P2-11 writes `T=<checkout>/target/debug/talaria`: use the release binary instead).
2. `--no-ensure` on every launch of `talaria` that starts a live session and lacks it (every live scenario's start line lacks it).
3. Every `smoke run` that lacks `--talaria` gets `--talaria "$T"` (none of the 48 source scenarios runs `smoke`).
4. Where Herdr rejects the pane-split form a scenario writes, the helper `pane_new` of section 3; say so in the evidence.

These are not changes: cleanup before a scenario starts (kill a leftover tmux session of the same fixed name such as `uat` with `tmux kill-session -t uat 2>/dev/null`; close a leftover `talaria-smoke-` tab); the demo programs built by `cargo run` in debug (they are tools, not the product); a key name Herdr rejects sent with `tmux send-keys` where the scenario's own session is tmux. A test target or demo that no longer exists is `BLOCKED (not built)`.

Order within an issue is the scenario number order. Dependencies and notes:

- P2-01: UAT-1 and UAT-5 need no gateway (the demo reads a recorded log). UAT-2 to UAT-4 each use the common live setup. UAT-6 (operator) continues from a session where UAT-2 and UAT-3 ran.
- P2-02: every scenario creates `$TMPDIR/pager/.pytest_cache` and its own pane; UAT-6 runs `talaria -c --profile fixture` after `/quit` (add `--no-ensure`). UAT-5 needs `approvals.mode: manual`.
- P2-03: UAT-1 to UAT-5 and UAT-7 are agent rows; UAT-6 is the operator's (a throwaway skill in the `fixture` profile home, created and removed by the operator). UAT-7's turn is long on purpose; rerun once with 1000 numbers if it ends before the keys.
- P2-04: every scenario runs `talaria` inside a tmux session named `uat` (for bracketed pastes); UAT-3 is the operator's (clipboard image on an image-capable route). UAT-4 uses a new random file name each run; the file Hermes copies into the profile's attachments folder is a side effect the evidence notes. UAT-5 needs a BMP file and a text-only clipboard.
- P2-05: UAT-3 needs P2-03 merged (it is). UAT-5 submits four `!sleep` commands quickly; Hermes runs them one at a time.
- P2-11: each live scenario uses its own pane and folder (`tasksproj-<n>`); UAT-6 needs UAT-4's folder and its stored session id; UAT-3's source also asks the operator to press Ctrl+T on the keyboard outside Herdr: the agent runs it first in Herdr; the operator always makes the keyboard run (section 7.4); the row is PASS when both pass. Herdr swallows Ctrl+T when, after the agent sends `ctrl+t` while the sleep step still runs, the screen still shows `▾ Tasks` and the `[>]` row; the agent run is then BLOCKED (environment) and, as the source says, the keyboard run alone decides (that is the only case where it does). An agent run that ends before both Ctrl+T reads, because the model's sleep was shorter than the 30 s asked, says nothing about the key: it is rerun with a fresh session (section 8.1, at most three runs) and the keyboard run does not replace it; three short runs are the stop condition of the issue on the sleep length. A todo tool the model does not call in three sessions is `BLOCKED (environment)`.
- P2-12: UAT-1 to UAT-3 and UAT-5 are agent rows. UAT-4 and UAT-6 change the `fixture` skin and run last of all rows (stage 3): the operator restores it and logs the read-back.
- P2-13: UAT-1 needs `EDITOR=vi`, UAT-2 `EDITOR=talaria-no-such-editor`, set in the pane before launch. UAT-3 restarts in the same folder and reads Talaria's own history file; evidence shows only the prompts the tester typed. UAT-4 starts Talaria from the pane's interactive shell (not `exec`); UAT-7 uses `exec`. UAT-5 needs `approvals.mode: manual` and `.pytest_cache/x`. UAT-6 is the operator's, in a real terminal.

Operator rows among them: `P2-01 UAT-6`, `P2-03 UAT-6`, `P2-04 UAT-3`, `P2-12 UAT-4`, `P2-12 UAT-6`, `P2-13 UAT-6`, and the keyboard run of `P2-11 UAT-3` (recorded in that row's evidence). All other rows are run by the agent tester in stage 2.

### 7.3 Cross-feature rows XF-1 to XF-11 (agent tester)

These need the live gateway, profile `fixture`, and the common settings of section 3 (every pane starts with `. ../acc.env`). Each row uses its own folder `P=$(mktemp -d)` and its own panes, each made with `PANE=$(pane_new "$P")`. "Start Talaria in `$PANE`" means `herdr pane run "$PANE" "$T $F"` then `herdr pane wait-output "$PANE" --match minimax-m3.1-flash --source visible --timeout 60000`. Before starting Talaria in a pane type `echo READ-CHECK` there and confirm which of `visible` and `recent-unwrapped` shows it; use that source for the row (a read that lacks text just typed is stale and never evidence). "Submit" is the `submit` helper (two calls). Prompts are never eased. If after 8 s nothing happened (a still screen, no card, no counting line), send `herdr pane send-keys "$PANE" enter` once and say so in the evidence. A model that fails a plain prompt makes a run fail without being a Talaria defect: rerun once and record both runs; a failing second run stays FAIL.

```bash
ESC=$(printf '\033')
```

**XF-1: The pager shows tool cells in full, and the scrollback hint names the pager's key.** Covers P2-01 with P2-02.
- Steps: `printf 'alpha\nbeta\n' > "$P/notes.txt"`; start Talaria. `submit "$PANE" 'Run the shell command seq 1 10, then run the shell command ls /talaria-no-such-dir, then reply with the single word: done.'`; `herdr pane wait-output "$PANE" --regex '(?mi)^\W*done\W*$' --source recent-unwrapped --timeout 180000`; `rec "$PANE" > "$X/r1.txt"`; `herdr pane send-keys "$PANE" ctrl+o`; `sleep 1`; `herdr pane send-keys "$PANE" g`; `sleep 1`; `vis "$PANE" > "$X/pg1.txt"`; `herdr pane send-keys "$PANE" t`; `sleep 1`; `vis "$PANE" > "$X/pg2.txt"`; `herdr pane send-keys "$PANE" q`; `sleep 1`; `vis "$PANE" > "$X/in.txt"`.
- Pass: `grep -c '… +7 lines (Ctrl+O to view)' "$X/r1.txt"` prints 1 or more (the hint is the pager key's label, wired by whichever of P2-01 and P2-02 merged second); `grep -cE '^ *Transcript ·' "$X/pg1.txt"` prints 1; `grep -cE '^ +Arguments *$' "$X/pg1.txt"` and `grep -cE '^ +Output *$' "$X/pg1.txt"` print 1 or more; `grep -cE '^ *10 *$' "$X/pg1.txt"` prints 1 or more (all ten lines of output, not the preview); `grep -cE '\(\+[0-9]+ lines\)' "$X/pg2.txt"` prints 1 or more and `grep -cE '^ +Arguments' "$X/pg2.txt"` prints 0 (compact); `grep -c 'Transcript ·' "$X/in.txt"` prints 0.
- Evidence: the key lines of the five reads.

**XF-2: The slash palette and the `@` list never open together.** Covers P2-03 with P2-04 (they share the four `completion_*` keys).
- Steps: `mkdir -p "$P/src"; printf 'def average(values): return sum(values) / len(values)\n' > "$P/src/calc.py"`; start Talaria. `herdr pane send-text "$PANE" '/plan fix @src'`; `sleep 1`; `vis > "$X/a.txt"`; `send-keys ctrl+c`; `send-text '@cal'`; `sleep 1`; `vis > b.txt`; `send-keys esc`; `sleep 0.4`; `vis > c.txt`; `send-keys ctrl+c`; `send-text '/pla'`; `sleep 1`; `vis > d.txt`; `send-keys esc`; `sleep 0.4`; `send-keys ctrl+c`.
- Pass: in `a.txt` neither footer appears: `grep -c 'Tab complete' a.txt` and `grep -c 'tab complete · enter insert · esc close' a.txt` print 0 (a draft starting with `/` opens no `@` list, and the palette is closed when the caret word starts with `@`); `b.txt` has the `@` footer (`tab complete · enter insert · esc close`, lower case) and a `calc.py` row and no `Tab complete`; `c.txt` has neither footer and `grep -ci interrupt "$X/c.txt"` prints 0; `d.txt` has `Tab complete` and not the `@` footer.
- Evidence: the four reads' footer lines.

**XF-3: The pager finds the text of `!` shell cells and `/btw` cells.** Covers P2-02 with P2-05 (the pager draws kinds it does not own through its fallback).
- Steps: `TK=heron-$((RANDOM % 9000 + 1000))`; start Talaria; `submit "$PANE" "!echo $TK"`; `herdr pane wait-output "$PANE" --match "$TK" --source recent-unwrapped --timeout 15000`. `submit "$PANE" '/btw what is two plus two? Answer with one word.'`; `herdr pane wait-output "$PANE" --match 'not part of the conversation' --source recent-unwrapped --timeout 90000`. `send-keys ctrl+o`; `sleep 1`; `send-text '/'`; `send-text "$TK"`; `send-keys enter`; `sleep 1`; `vis > pg1.txt`; `send-text '/'`; `send-text 'two plus two'`; `send-keys enter`; `sleep 1`; `vis > pg2.txt`; `send-keys q`.
- Pass: `grep -cE "\"$TK\" [0-9]+ of [0-9]+" pg1.txt` prints 1 and `grep -c "$TK" pg1.txt` prints 2 or more (footer plus the cell's text); `grep -cE '"two plus two" [0-9]+ of [0-9]+' pg2.txt` prints 1 and `grep -c 'two plus two' pg2.txt` prints 2 or more. `no match for` in either read is a FAIL, reported to P2-02's owner (its wildcard arm drew nothing searchable).
- Evidence: the two footers and the lines holding the token.

**XF-4: Ctrl+T does nothing under the pager and the task record is written once.** Covers P2-11 with P2-02.
- Steps: start Talaria. `submit "$PANE" 'Use your todo_list tool to write a plan with exactly three items: create a.txt, run sleep 30, create b.txt. Before each step mark it in_progress with the todo_list tool, and after it mark it completed. Do the steps in this folder. Then reply with the single word: done.'`; `herdr pane wait-output "$PANE" --regex '(?i)\[>\] .*sleep' --source visible --timeout 120000`; at once `send-keys ctrl+o`; `sleep 1`; `send-keys ctrl+t`; `sleep 1`; `vis > pg.txt`. Keep the pager open until the turn ends: poll `vis` every 3 s until two reads are identical (limit 150 s). `send-keys q`; `sleep 1`; `vis > after.txt`; `rec > r.txt`.
- Pass: `grep -c 'Transcript ·' pg.txt` prints 1 or more (Ctrl+T did not close or change the pager); `grep -cE '^ *Tasks [0-9]+/[0-9]+ · ' r.txt` prints 1 (the record was written once while the pager was open); `grep -cE '(▾|▸) Tasks' after.txt` prints 0.
- BLOCKED (environment) if the model never calls the todo tool: at most three sessions (P2-11 UAT rules).
- Evidence: the three greps and the record line.

**XF-5: A paste label survives the external editor and the full text is sent.** Covers P2-04 with P2-13. tmux required.
- Steps: `S=accxf5; tmux kill-session -t $S 2>/dev/null`; `E5=tangerine-$((RANDOM % 9000 + 1000))`; `awk -v m="$E5" 'BEGIN { for (i = 1; i <= 120; i++) print (i == 87 ? "the marker is " m : "line " i) }' > "$P/big.txt"`; `printf '#!/bin/sh\nprintf " plus-two" >> "$1"\n' > "$P/ed.sh"; chmod 700 "$P/ed.sh"`; `tmux new-session -d -s $S -x 120 -y 40 -c "$P" "EDITOR=$P/ed.sh $T $F"`; poll `tmux capture-pane -p -t $S` until it shows `minimax-m3.1-flash`. `tmux load-buffer -b xf5 "$P/big.txt"; tmux paste-buffer -p -r -d -b xf5 -t $S; sleep 1; tmux capture-pane -p -t $S > p1.txt`. `tmux send-keys -t $S C-g; sleep 3; tmux capture-pane -p -t $S > p2.txt`. `tmux send-keys -t $S -l ' What is the marker on line 87? Reply with only the marker word.'; tmux send-keys -t $S Enter`; poll `tmux capture-pane -p -S - -t $S | grep -cw "$E5"` until 1 or more and two captures 3 s apart are identical (limit 120 s); `tmux capture-pane -p -S - -t $S > p3.txt`.
- Pass: `grep -c '\[Pasted text #1 +120 lines\]' p1.txt` prints 1 and `grep -c 'the marker is' p1.txt` prints 0 (collapsed); `grep -c '\[Pasted text #1 +120 lines\] plus-two' p2.txt` prints 1 (the label survived the editor round trip); `grep -cw "$E5" p3.txt` prints 1 or more (Hermes received the whole text).
- Evidence: the three greps, `tmux -V`. The session stays up for XF-6.

**XF-6: Recalling that prompt with Ctrl+R brings back the label, and the full text is sent again.** Covers P2-04 with P2-13; continues XF-5's session; XF-5 must be PASS.
- Steps: `N5=$(grep -cw "$E5" p3.txt)`; `tmux send-keys -t $S C-r; sleep 1; tmux send-keys -t $S -l 'marker'; sleep 1; tmux capture-pane -p -t $S > h1.txt`; `tmux send-keys -t $S Enter; sleep 1; tmux capture-pane -p -t $S > h2.txt`; `tmux send-keys -t $S Enter`; poll as in XF-5 until `grep -cw "$E5"` of the full capture exceeds `N5` and the screen settles (limit 120 s); `tmux capture-pane -p -S - -t $S > h3.txt`; `tmux send-keys -t $S -l '/quit'; tmux send-keys -t $S Enter`; `tmux kill-session -t $S 2>/dev/null`.
- Pass: `grep -c 'marker' h1.txt` prints 1 or more (the search box lists the earlier prompt); `grep -c '\[Pasted text #1 +120 lines\]' h2.txt` prints 1 or more (the label came back unsent: `grep -c 'working' h2.txt` prints 0); `grep -cw "$E5" h3.txt` is greater than `N5` (the recalled label still resolved to its text).
- Evidence: `N5`, the final count, the three reads' key lines.

**XF-7: `NO_COLOR` removes colour from diff rows and status but keeps the signs and marks.** Covers P2-01 with P2-12. tmux required.
- Steps: two folders `PA` and `PB`, each with `notes.txt` (`alpha`, `beta`). `tmux new-session -d -s accxf7a -x 120 -y 40 -c "$PA" "$T $F"` and `tmux new-session -d -s accxf7b -x 120 -y 40 -c "$PB" "NO_COLOR=1 $T $F"`. In each, once `minimax-m3.1-flash` shows: `tmux send-keys -t <s> -l 'In notes.txt change the word beta to gamma. Use your file editing tool, then reply with the single word: done.'; tmux send-keys -t <s> Enter`; poll `tmux capture-pane -p -S - -t <s>` for a line holding only `done` (`grep -cE '^[^A-Za-z0-9]*done[^A-Za-z0-9]*$'` 1 or more) and a settled screen. `tmux capture-pane -e -p -S - -t accxf7a > cA.txt`; the same for `accxf7b` into `cB.txt`. `tmux kill-session` both.
- Pass: `grep '2 + gamma' cA.txt | grep -cE "$ESC\\[[0-9;]*(3[0-7]|9[0-7]|38;[25])"` prints 1 or more (an added row is coloured); `grep -cE "$ESC\\[[0-9;]*(3[0-7]|4[0-7]|9[0-7]|10[0-7]|38;|48;)" cB.txt` prints 0 (no colour sequence anywhere under `NO_COLOR`); `grep -c '2 + gamma' cB.txt` and `grep -c '✓ +1 -1' cB.txt` print 1 or more (signs and marks stay).
- Evidence: the four counts and the one coloured row with escapes shown as `<ESC>`.

**XF-8: Herdr shows `blocked` while the pager covers an approval card.** Covers P2-13 with P2-02 and P1-09's card. Needs `approvals.mode: manual`; else BLOCKED (environment).
- Steps: `mkdir "$P/.pytest_cache" && touch "$P/.pytest_cache/x"`; `PANE` (Talaria) and `PANE2` (a shell), both from `pane_new "$P"`. Start Talaria. `submit "$PANE" 'Run this exact shell command and then tell me its exit code: rm -rf .pytest_cache'`; poll `vis` up to 90 s until it holds `rm -rf .pytest_cache` and the lines `1. once` and `4. deny`. `herdr pane get "$PANE" > g1.txt`; `send-keys ctrl+o`; `sleep 1`; `vis > pg.txt`; `herdr pane get "$PANE" > g2.txt`; `send-keys q`; `sleep 1`; `vis > card.txt`; `send-keys esc` (Esc on the card sends `deny`); poll until two `vis` reads 3 s apart are identical (limit 60 s); `herdr pane get "$PANE" > g3.txt`; `herdr pane run "$PANE2" "ls -a $P"`; read.
- Pass: `grep -ci blocked g1.txt` and `grep -ci blocked g2.txt` print 1 or more (and both hold `talaria`); `grep -c 'Hermes is waiting for your answer' pg.txt` prints 1 (the banner); `grep -c '1\. once' card.txt` and `grep -c '4\. deny' card.txt` each print 1 or more (the card is back); `grep -ci idle g3.txt` prints 1 or more; the listing still shows `.pytest_cache`. If `herdr pane get` shows no agent at all (T6), the row is `BLOCKED (environment)` and the operator may read the Herdr sidebar by hand and post the result.
- Evidence: the three `pane get` outputs (scrubbed), the banner line, the listing.

**XF-9: Idle cost after every feature was used.** Covers all eight issues, the idle budget of P1-11 and P2-11 REQ-24. The `process-info` JSON path below (`.result.process_info.foreground_processes[]`, each with `cmdline` and `pid`) is P1-13's recorded fact H6 (its spec 2.1, Herdr 0.9.4) and the same line PH1-ACC's idle-cost row uses; re-check it with the Herdr in use.
- Steps: new folder with `notes.txt` and `src/calc.py`; start Talaria in `$PANE`. After 5 s at an empty composer: `PID=$(herdr pane process-info --pane "$PANE" | jq -r '.result.process_info.foreground_processes[] | select(.cmdline | contains("target/release/talaria")) | .pid')`; sample 0: `R0=$(ps -o rss= -p "$PID")`. Then, waiting for each reply: submit the edit prompt of XF-7; `submit "$PANE" '!echo idle-check'`; `submit "$PANE" '/btw say yes'` and wait for `not part of the conversation`; `send-keys ctrl+o`, `sleep 1`, `send-keys q`; `submit "$PANE" '/theme'`. Wait 5 s. Sample 1: `R1`, `C1` (`ps -o time= -p "$PID"`), `V1=$(vis "$PANE")`. `sleep 120`. Sample 2: `R2`, `C2`, `V2`. Convert `time=` with `awk -F: '{ s = 0; for (i = 1; i <= NF; i++) s = s * 60 + $i; print s }'` (on macOS `ps -o time=` prints `M:SS.cc`, for example `0:00.01`, checked on 2026-10-22; the conversion also handles `H:MM:SS`).
- Pass: `R0` at most 40960 (KiB, the 40 mebibyte start budget); `R2` at most 102400 (the 100 mebibyte ceiling); `R2 - R1` at most 4096; `C2 - C1` at most 1 second; `V1` and `V2` identical (`cmp`). The first two limits are the architecture's budgets ("MB" read as mebibytes, as P1-11 decision 7 does); the growth and CPU limits are this issue's own sanity checks, the same as PH1-ACC's XF-7, and the operator confirms them on the parent issue before the freeze. The `jq` path is the shape P1-13 reads from `herdr pane process-info`; a `ps` or `jq` that does not work on the machine makes the row `BLOCKED (environment)`.
- Evidence: the three samples and conversions, the `cmp` result.

**XF-10: Esc during a running tool marks it not finished and keeps the pending side question (negative).** Covers P2-01 with P2-05.
- Steps: start Talaria. `submit "$PANE" 'Run the shell command sleep 30, then reply with the single word: done.'`; `herdr pane wait-output "$PANE" --regex 'running [0-9]+s' --source recent --timeout 60000`. `submit "$PANE" '/btw what is the capital of France? Answer with one word.'`; `herdr pane wait-output "$PANE" --regex '(answering|side answer · not part)' --source visible --timeout 10000`. `herdr pane send-keys "$PANE" esc`; `sleep 0.4`; poll until two `vis` reads 3 s apart are identical (limit 30 s); `rec > r1.txt`. `herdr pane wait-output "$PANE" --match 'not part of the conversation' --source recent-unwrapped --timeout 90000`; `rec > r2.txt`; `vis > v2.txt`. `submit "$PANE" 'Reply with only the word OK.'`; wait for a line holding only `OK`; `rec > r3.txt`.
- Pass: `grep -cE '⏺ Shell\(sleep 30\) +◌ not finished' r1.txt` prints 1; `grep -c 'btw · what is the capital of France' r2.txt` prints 1 and `grep -c 'side answer · not part of the conversation' r2.txt` prints 1; `grep -c 'answering' v2.txt` prints 0; `grep -cE '^[^A-Za-z0-9]*OK\.?[^A-Za-z0-9]*$' r3.txt` prints 1 or more (the session still works).
- Evidence: the four greps, the three reads' key lines.

**XF-11: Resume prints history once, drops the local cells, and the pager finds the old prompt.** Covers P2-01, P2-02, P2-05 and P2-11's record.
- Steps: `N=$((RANDOM % 9000 + 1000))`; folder with `notes.txt`; start Talaria in `$PANE`. `submit "$PANE" "In notes.txt change the word beta to gamma. Use your file editing tool, then reply with MARK-$N alone on the last line."`; wait until `grep -cw "MARK-$N"` of `rec` is 2 or more and the screen settles. `submit "$PANE" "!echo shell-$N"` and wait for the output; `submit "$PANE" '/btw what did I ask you to do first?'` and wait for `not part of the conversation`. `K1=$(rec "$PANE" | grep -cw "MARK-$N")`, `H1=$(rec "$PANE" | grep -cE '^ *⏺ .*notes\.txt')`. `submit "$PANE" '/quit'`; read the session id printed after `--resume` as `SID`. `PANE2=$(pane_new "$P")`; `herdr pane run "$PANE2" "$T --no-ensure --resume $SID --profile fixture"`; wait for a line starting `── Resumed:`. `K2`, `H2` the same greps on `$PANE2`; `S2=$(rec "$PANE2" | grep -c "shell-$N")`; `B2=$(rec "$PANE2" | grep -c 'btw ·')`. `send-keys ctrl+o`; `send-text '/'`; `send-text "MARK-$N"`; `send-keys enter`; `sleep 1`; `vis "$PANE2" > pg.txt`; `send-keys q`; `submit "$PANE2" '/quit'`.
- Pass: `grep -c '^── Resumed:'` of the `$PANE2` read prints 1; `K2` equals `K1` and is 2 or more (history once); `H2` equals `H1` (the tool cell once, in whatever form a resumed cell takes); `S2` and `B2` are 0 (local cells are not stored by Hermes: P2-05 REQ-16); `grep -cE "\"MARK-$N\" [0-9]+ of [0-9]+" pg.txt` prints 1.
- Evidence: `K1 K2 H1 H2 S2 B2`, the `Resumed` line, the last 15 lines of each pane.

### 7.4 Operator rows (seven items, one sitting)

The operator runs the six source scenarios no agent can and the keyboard run of `P2-11 UAT-3` (Ctrl+T pressed on the keyboard in a terminal outside Herdr, the steps of that scenario; its comment is `P2-11 UAT-3 keyboard: PASS`, `FAIL` or `BLOCKED (<cause>)`), on the candidate binary `target/release/talaria` (the lead tells them where it is outside the issue). Order: `P2-01 UAT-6`, `P2-03 UAT-6`, `P2-04 UAT-3`, `P2-13 UAT-6`, `P2-11 UAT-3` (keyboard), then `P2-12 UAT-4` and `P2-12 UAT-6` last. Start every session with `--no-ensure`. There is no minimum duration and no switch day here (that is PH2-ACC's).

| Row | What the operator does (source text: Appendix A) | Needs | Also |
|---|---|---|---|
| `P2-01 UAT-6` | scroll back, search `gamma`, copy one added diff row, in Herdr, in tmux and in a plain terminal | three terminals | one screenshot per terminal and the pasted copy |
| `P2-03 UAT-6` | create `talaria-uat-skill` in the `fixture` profile home, see it listed first within 10 s, run it, remove it, see it gone | a throwaway skill folder | elapsed times |
| `P2-04 UAT-3` | Ctrl+V with a red-square PNG on the clipboard, ask the colour | a desktop clipboard, an image-capable route on `fixture` | `cargo test -p talaria-app --test clipboard_reader real_tools_read_a_known_image -- --ignored` (overwrites the clipboard) and its result line |
| `P2-13 UAT-6` | bell and desktop notification for an approval and for a 30-second turn, in Ghostty or iTerm2 | `approvals.mode: manual` | a table for Herdr and tmux (does not decide the result) |
| `P2-12 UAT-4` | `/skin ares` live, scrollback colours kept, restore | permission to switch the `fixture` skin; a 24-bit terminal | `TALARIA_LIVE_REQUIRE=1 TALARIA_LIVE_SKIN_SWITCH=1 cargo test -p talaria-cli --test live_skin live_skin_switch_round_trip -- --ignored --nocapture` prints `LIVE_SKIN_RESULT: PASSED`; the read-back of the restored skin |
| `P2-12 UAT-6` | an unknown skin name, `/theme` on a light and a dark terminal and under tmux, restore | a light and a dark terminal | a screenshot and the `/theme` text per terminal |

The operator posts one comment per row, its first line `<id>: PASS`, `<id>: FAIL` or `<id>: BLOCKED (<cause>)`, then the counts, the terminals and the screenshots; the text carries no home directory, user name or profile name other than `default` or `fixture` (the operator edits the comment if a scan finds one; an agent never edits it). When all seven pass the operator posts one more comment with the line `PHASE 2A OPERATOR ROWS: PASSED (candidate <7 hex>)` (the 7 characters equal the candidate's first 7). An agent copies each result and its notes into the evidence section and cites the comment's URL. The operator, not an agent, decides a result.

## 8. Run order, failures and a new candidate

1. Stage 0: build the smoke extension and merge it (section 9). Freeze (section 3).
2. Stage 1 (agent tester, one pane at the worktree root): AUTO-1 to AUTO-7, then AUTO-8, AUTO-9, AUTO-10 back to back.
3. Stage 2 (agent tester): the 42 source rows the operator does not run (P2-11 UAT-3 among them, its keyboard run in stage 3), in issue order (section 7.2), then XF-1 to XF-11 in order, one live scenario at a time. XF-6 continues XF-5's tmux session.
4. Stage 3 (operator): section 7.4 (seven items).
5. Stage 4: write the record and evidence (the first commit is the empty record, section 4), run the checker, open the pull request (documentation only), have it reviewed, merge. The pull request body carries the lines of the issue's REQ-15.

Three identical provider errors in a row (same HTTP status, same message) stop the run: report them on the issue and ask the operator, never retry in a loop. A row may be run up to three times on one candidate; every run goes in its evidence section with why each earlier failure was not a Talaria defect (a provider rate limit, a model that did not follow a plain prompt).

A FAIL stays FAIL until a new candidate passes it. The tester comments the pane text on the issue; the lead files a defect issue through Mission Control for the owner of the source issue (the operator files those found in their rows, or asks the lead to); the fix merges; the lead freezes a new candidate (section 3) and every row is run again; the record is overwritten (git history keeps the old one). A defect is never fixed inside this issue, and no scenario's expected result is edited to make it pass; if a source issue's expected text is wrong, its owner corrects the source issue and this unit's Appendix A is corrected (a correction of the specification is the one change allowed after the freeze).

### 8.1 Rulings that settle questions a builder asked

These settle points where two rules of the issue could be read against each other. They change no pass rule; every one keeps "only PASS is a pass".

1. **A row's label follows its cause, not the exit code.** A suite run that prints `PASSED WITH SKIPS` (exit 3) is BLOCKED (environment). Under `TALARIA_LIVE_REQUIRE=1` a skipped step prints `FAIL <step> (skipped: <reason>; TALARIA_LIVE_REQUIRE=1)` and the run exits 1; the line says `(skipped:`, so the cause is the environment and the row is `BLOCKED (environment: <step> skipped)`, not FAIL. Any other `FAIL <step> (...)` line, including `deadline exceeded in step <name>`, is a FAIL of the row unless item 2 shows it was not a Talaria defect. The checker accepts only the four forms and `BLOCKED (<cause>)` takes any cause text without a closing bracket.
2. **Reruns on the same candidate.** A row may be run up to three times on one candidate. A BLOCKED row is rerun once the cause is fixed (the comment names who fixed what). A FAIL may be rerun only when the tester shows it was not a Talaria defect: the pane text shows a provider error (status and message) or the model not following a plain prompt, while every wording and key of Talaria was as its source issue says; and the lead writes `rerun approved: <row id>: <cause>` on the issue before the rerun. A FAIL that is a Talaria defect (a wording that differs, a key that does nothing, a crash, a wrong state) is final for the candidate: the row is rerun only on a new candidate (the issue's REQ-15). When it is unclear which it is, treat it as a defect, because another candidate costs less than a wrong pass. The record holds the result of the last run; the evidence lists every run (`Run <k> of <n>`) with why each earlier one failed or was blocked. A row never turns PASS on a later run of the same candidate after a Talaria defect.
3. **AUTO-1 when the dispatch run reports nothing on the candidate.** The row is BLOCKED (environment), reported on P1-11. No candidate is accepted with it BLOCKED. Because the answer can be had in stage 0 (section 7.1), the operator then has P1-11's workflow fixed before the freeze, or, before the freeze, edits AUTO-1 to drop its bench-run part (settled decision 2).
4. **Dropping a row or a step before the freeze (settled decision 2).** Let N be 69 minus the rows dropped, and S 21 minus the steps dropped. In one commit whose message gives the reason, the lead changes: in the issue, every `69` (Objective, user-visible result, REQ-7, REQ-9, UAT-2, AC-5, AC-9, the fallback of settled decision 6, section 10.1 here), the row counts `10 + 48 + 11` (AC-7) and `48`/`11` (REQ-10 to REQ-12) as far as they drop, and, for a dropped step, every `21` (REQ-2, REQ-10, UAT-3, the AUTO-9 and AUTO-10 pass rule); here, the section 4 skeleton (rows removed) and the red-run line (`FAILED (N problems, 0 of N rows PASS)`); and in the checker, the `IDS=` line (the `IDS=` ids are the only list: the test script builds its rows from it). In the test script: case 1's `69 of 69` becomes `N of N`; a case that names a dropped id (`P2-13 UAT-6`, `AUTO-5`, `XF-2`, `P2-03 UAT-2`, `AUTO-1`, `XF-11`, `P2-11 UAT-6`, `XF-1`, `AUTO-2`, `AUTO-3`, `XF-6`) names another kept id in its place, with the same role; the number of cases stays 22. For a dropped smoke step, the test author deletes that step's functions, screens and wording constants in a commit `test(smoke): drop step <name> (operator decision)`, and the counts of functions (55 step functions, the lists in 9.9) and steps are updated. The lead lists every number changed in a comment on the issue before the freeze. Dropping after the freeze is not possible: it is a new candidate.

## 9. The smoke suite extension

### 9.1 Why a second suite, and what it is not

P1-13's `smoke run` checks five behaviours of the first usable binary in under five minutes. Phase 2a adds about twenty things a person sees, several of which need a special setup (an editor program, a bracketed paste, a file outside the project). Putting them into the phase 1 run would lengthen it past its 300 s budget, would change the code PH1-ACC ran as a regression row, and would let a skip in a new step hide behind the old steps. So the extension is a second suite, selected with `--suite phase2a`, with its own deadline (1800 s, section 9.6), its own result line and its own tests; the phase 1 suite, its tests and its README steps stay as they are (AC-2).

What it is not: it is not a replacement for the 48 scenarios or the 11 cross-feature rows (it checks each feature's happy path and one refusal, not its edge cases), it is not a performance test (P1-11's harness owns budgets), and it does not run in continuous integration (it needs a live gateway, a model and Herdr; the operator and this unit run it by hand).

### 9.2 Shape of the code

New files in `tools/smoke` (P1-13's crate; its layout is in P1-13 spec section 3):

```text
src/suite.rs        enum Suite { Phase1, Phase2a }; FromStr
src/wording.rs      every Talaria screen text a step relies on, as consts, each with a source comment (9.5)
src/screen2a.rs     pure helpers over screen text (below)
src/steps2a.rs      run_smoke_2a and the 21 steps
src/tmux.rs         trait Paster; TmuxCli; TmuxGuard
tests/              tmux_cli.rs, screen2a.rs, steps2a.rs, typed_input2a.rs, cleanup2a.rs, cli2a.rs,
                    fixtures_public_safe2a.rs, live_smoke2a.rs, fixtures/screens/phase2a/*.txt, fixtures/tmux/*.txt,
                    support/fake_tmux.rs (a helper binary: Cargo.toml gains a [[bin]] entry named fake_tmux)
```

Minimal edits to existing files (named in the pull request): `src/config.rs` (the `--suite` and `--tmux` options; `launch_line_env`; the deadline default), `src/main.rs` (dispatch on the suite; the two extra preflight checks run only with `--suite phase2a`), `src/terminal.rs` (the `pane_get` method with a default body), `src/steps.rs` (the shared steps `startup`, `plain_answer`, `quit` and `resume` become `pub(crate)` and take an environment prefix for the launch line, empty for phase 1; nothing else, so `run_smoke` behaves exactly as before although its call sites of those steps change: list them in the pull request), `src/lib.rs`, `README.md`, `Cargo.toml` (the `[[bin]]` entry for `fake_tmux`) and `Cargo.lock` (no new crate is expected; any new one is pinned with `=`), `tests/support/` (additions only).

`Token` is P1-13's (`screen.rs`: `Token { word, number }`, `Token::new(rng, used)`, `join()` giving `word-number`, `prompt()`; the words are `maple, cobalt, juniper, saffron, tundra, walnut, harbor, quartz`, the numbers 1000 to 9999, and `new` never repeats a pair of one run: P1-13 spec 4.3 and 5.2). Phase 2a adds only `prompt_after` below, a pure function with its own test; it needs no new word list or number range.

Public items, with the signatures the tests use (keep the names and shapes unless the code forces a change, and report it):

```rust
// suite.rs
pub enum Suite { Phase1, Phase2a }                       // FromStr: "phase1" | "phase2a"; else Err(format!("unknown suite '{s}'; use phase1 or phase2a"))
// config.rs additions
pub struct Phase2aConfig { pub tmux: PathBuf }           // --tmux, default "tmux"
pub fn launch_line_env(env: &[(&str, &str)], talaria: &Path, args: &[&str]) -> String;
                                                         // KEY='value' '<path>' args...; echo SMOKE_EXIT:$?
// terminal.rs addition (default body: Err(TermError::Parse("pane get is not supported".into())), so P1-13's fakes compile unchanged)
fn pane_get(&self, pane: &PaneId) -> Result<String, TermError>;   // text of `herdr pane get <pane>`
// tmux.rs
pub trait Paster {
    fn paste(&self, session: &str, text: &str) -> Result<(), TermError>;   // bracketed paste into the session's pane
    fn session_exists(&self, session: &str) -> Result<bool, TermError>;
    fn kill_session(&self, session: &str) -> Result<(), TermError>;        // Ok when the session is already gone
    fn exit_code(&self) -> Option<i32>;                                     // from the marker file launch.sh writes
    fn version(&self) -> String;                                            // `tmux -V`
}
pub struct TmuxCli { /* executable, marker file path, scratch folder */ }
impl TmuxCli { pub fn new(executable: &Path, marker: &Path, scratch: &Path) -> Self }
pub struct TmuxGuard<'a> { /* paster, session name */ }
impl TmuxGuard<'_> { pub fn close(self) -> Result<(), CleanupError> }      // Drop is only a fallback
// steps2a.rs
pub fn run_smoke_2a(term: &dyn Terminal, paster: &dyn Paster, clock: &dyn Clock,
                    cfg: &SmokeConfig, cfg2: &Phase2aConfig, stop: &AtomicBool) -> SmokeReport;
// screen2a.rs (pure; each takes the normalised lines of P1-13 spec 5.3, box characters removed and trimmed)
pub fn tool_header(text: &str, command: &str) -> Option<ToolHeader>;          // { line_index, mark: Mark, exit: Option<i32> }
pub fn rows_after(text: &str, from: usize, within: usize, needle: &str) -> bool;
pub fn edit_header(text: &str, file: &str) -> Option<EditHeader>;             // { line_index, stat_ok: bool }
pub fn has_diff_rows(text: &str, from: usize) -> bool;                         // '2 - beta' and '2 + gamma'
pub fn has_debris(text: &str) -> bool;                                         // '┊', ESC, '[38;2'
pub fn palette_state(visible: &str) -> Palette;                                // { footer: bool, rows: Vec<PaletteRow> } PaletteRow { name, tag: Option<String> }
pub fn mention_list(visible: &str) -> Option<MentionRow>;                      // calc.py with its folder
pub fn paste_labels(text: &str) -> Vec<PasteLabel>;                            // { number, lines }
pub fn file_label(text: &str, name: &str) -> bool;
pub fn shell_cell(text: &str, command: &str) -> Option<ShellCell>;             // { header_index, output_has: Vec<String> }
pub fn side_cell(text: &str, question: &str) -> Option<SideCell>;              // { header_index, footer_index, answer: String, failed: bool, empty: bool }
pub fn pager_state(visible: &str) -> Pager;                                    // { open, following, search: Option<(k, n, query)>, no_match }
pub fn theme_report(text: &str) -> ThemeReport;                                // { present: [bool; 5] }
pub fn task_view(visible: &str) -> TaskView;                                   // Expanded | Collapsed | Absent, with the in-progress row
pub fn task_records(text: &str) -> Vec<(u32, u32)>;                            // '^Tasks c/t · '
pub fn agent_state(pane_get_text: &str) -> Option<AgentState>;                 // Idle | Working | Blocked, with the label 'talaria' seen
pub fn prompt_after(token: &Token, action: &str) -> String;                   // "<action>, then join these two words with a hyphen and reply with only the result: <word> and <number>"
```

### 9.3 Flow, step by step

All waits are poll loops at 250 ms using `Clock`; every loop also checks the stop flag and the run deadline. Each step uses the source named in the table below: a read of the screen as it is now uses `visible`; a search for text that may have scrolled away uses `recent-unwrapped --lines 3000` (`RECENT_LINES`). Which source lags for a full-screen program is P1-13's H14 item (section 2.1): this unit follows its recorded result and stops if a source lags. "Normalised lines" are P1-13's (section 5.3): the text's lines with box-drawing characters (U+2500 to U+257F) removed and each line trimmed. "Settled" is P1-13's (section 5.6): every read over the quiet period is identical. A reply step also needs its token. `submit` is P1-13's (section 5.1: `pane run`, with the single `enter` fallback and its `NOTE` line); `typed` is `send_text` without Enter; `key` is `send_keys`, and after `esc` the suite waits 400 ms. A step that fails prints the reason of section 9.4 and the capture.

| Step | Decides on `visible` (the screen now) | Decides on `recent-unwrapped` (may have scrolled) |
|---|---|---|
| startup, plain-answer, quit, resume | as P1-13 | as P1-13 |
| tool-shell, tool-edit | nothing | the tool cell, its output and diff rows, debris, the reply token |
| palette | the list, footer, group and closing | the `/version` block after Enter |
| mention | the `@` list and the inserted text | the reply word |
| shell | nothing | the `! echo` cell and the refusal line |
| btw | nothing | the `btw ·` cell, its footer and its answer |
| pager | every check (header, `following`, footer, close) | the `interrupt` counts L0 and L1 |
| history-search, editor | every check | the check that no reply started (editor) |
| theme | nothing | the report and the two usage lines |
| pane-state | the counting lines | nothing (`pane_get` is a third source) |
| checklist | the panel, the collapse and expand, the panel gone | the one `Tasks <c>/<t> · ` record |
| tmux-pane, paste-collapse, file-drop, paste-editor | the third pane's labels and composer text | the reply token |
| tmux-quit | nothing (`session_exists` and `exit_code`) | nothing |


**9.3.0 Setup (before the tab)**

- Run id and scratch project as P1-13 7.1 (a new folder under the system temporary directory, mode 0700, `talaria-smoke-<run id>`), now holding: `notes.txt` (`alpha`, `beta`, each on its line); `src/calc.py` (`def average(values): return sum(values) / len(values)`); `keepme/keep.txt` (`keep`); `big1.txt` and `big2.txt` (120 lines each: `line <i>`, and on line 87 `the marker is <token E>` and `the marker is <token G>`); `smoke-editor.sh` (mode 0700: `#!/bin/sh` then `printf ' plus-two' >> "$1"`); `launch.sh` (mode 0700, written when the tmux section starts: `#!/bin/sh`, then `unset HERDR_ENV HERDR_PANE_ID HERDR_BIN_PATH` (P2-13 REQ-15 reports the pane state only when these are set, and a tmux server that was already running would hand `talaria` another pane's id), then the launch line `EDITOR='<scratch>/smoke-editor.sh' '<talaria>' --no-ensure --profile <p> --model <m> --provider <v>`, then `echo "SMOKE_EXIT:$?" > '<scratch>/tmux-exit.txt'`). A second folder `talaria-smoke-<run id>-drop` (mode 0700) holds `notes-<8 hex>.txt` with the line `The secret word is <token F>.` The 8 hex characters are fresh each run, so Hermes copies a new file into the profile's attachments folder each time.
- Preflight additions (REQ-1): `tmux -V` runs; the scratch path holds no space and no single quote (it is written into single-quoted scripts; a double quote is allowed, because `mktemp -d` never makes one and the scripts do not break on it); both are one-sentence failures with exit 2 and nothing created. The issue's words "a space or a quote" mean exactly this character set.
- Eight tokens (P1-13's `Token`, never repeated in a run; each reply step's prompt is `prompt_after(token, <action>)` or P1-13's `prompt()`): A plain answer, B tool-shell, C tool-edit, D the shell echo, E first paste marker, F file secret, G second paste marker, H the third prompt after resume; a retry draws a new token.
- The tab, its root pane (`first`) and the `TabGuard` are made as in P1-13 4.1. The `EDITOR` setting rides on the launch line of every Talaria start: `launch_line_env(&[("EDITOR", "<scratch>/smoke-editor.sh")], talaria, args)`.

**9.3.1 `startup`, `plain-answer`**: P1-13 steps 4.2 and 4.3 unchanged, with the launch line above. `plain-answer` yields token A.

**9.3.2 `tool-shell`** (P2-01 UAT-3). Submit `Token::prompt_after("Run the shell command ls /talaria-no-such-dir")` with token B. Within 90 s the joined token B must appear in `recent` and the screen settle. Then on the normalised lines of `recent`: a line starting `⏺ Shell(ls /talaria-no-such-dir)` and holding `✗ exit` followed by an integer must exist (`tool_header`), and within the next 8 lines a line holding `No such file or directory`. Any exit number passes (`ls` exits 1 on macOS and 2 with GNU `ls`).

**9.3.3 `tool-edit`** (P2-01 UAT-2). Write `notes.txt` back to `alpha`, `beta`. Submit `Token::prompt_after("In notes.txt change the word beta to gamma. Use your file editing tool")` with token C. Within 90 s token C and a settled screen. Then: a line starting `⏺ Edit` or `⏺ Write` that holds `notes.txt` (`edit_header`); that line holds `✓ +1 -1`; within the next 12 lines the lines `2 - beta` and `2 + gamma` (the number column and sign collapse to single spaces; `has_diff_rows`); no line of `recent` holds `┊`, the escape character U+001B or `[38;2` (`has_debris`); and `notes.txt` on disk holds `gamma` and no `beta`. If no `⏺ Edit` or `⏺ Write` line exists (the model used the shell or no tool) reset `notes.txt` and retry once with `Token::prompt_after("Use your file editing tool now to change the word beta to gamma in notes.txt")` and a new token; after the second attempt fail with `tool-edit.nottool`.

**9.3.4 `palette`** (P2-03 UAT-1, UAT-2). `typed("/vers")`; within 5 s `visible` holds a normalised line containing `/version` and, anywhere in the last 25 lines, a line containing `Tab complete`. `key enter`; within 15 s `recent` holds a line starting `/version` followed within 3 lines by a non-blank line, and the screen settles for 1 s. `typed("/")`; within 5 s `visible` holds a line equal to `Talaria` and, in the lines after it, the names `/quit`, `/btw`, `/skin` and `/theme`, each on a line that also holds the whole word `talaria` (the tag); any name missing is named in the reason. `key esc`, 400 ms, `key ctrl+c`; within 3 s no line of `visible` holds `Tab complete`.

**9.3.5 `mention`** (P2-04 UAT-1). `typed("Summarize @calc")`; within 5 s `visible` holds a line with `calc.py` and `src`. `key tab`; within 3 s a line holds `@file:src/calc.py`. `typed(" in one sentence")`, `key enter`; within 60 s the whole word `average`, `averages` or `mean` (case-insensitive) appears in `recent` (the prompt echo shows `@file:src/calc.py` and cannot hold any of them) and the screen settles 3 s.

**9.3.6 `shell`** (P2-05 UAT-1, UAT-2). `typed("!echo <D>")`, `key enter`; within 5 s `recent` holds a line starting `! echo <D>` and, within the next 5 lines, a line equal to `<D>` (`shell_cell`). `typed("!rm -r ./keepme")`, `key enter`; within 5 s a line holds `Hermes refused to run this: it looks dangerous`; then `<scratch>/keepme/keep.txt` must still exist. Nothing else is typed after `!`: the refused line is the only destructive text the suite sends, and Hermes refuses it before anything runs (P2-05 REQ-3).

**9.3.7 `btw`** (P2-05 UAT-3). Submit `/btw which two words did I ask you to join in my first message?` (`submit`, whose prompt-fallback also applies; a `/btw` line is allowed by the slash list of section 9.10). Within 90 s a line starts `btw · which two words` and a later line holds `side answer · not part of the conversation` (`side_cell`). The answer is the text between them; it must not start with `The side question failed:` and must not be `(no answer)`, and it must hold the word of token A (the part before the hyphen, `Token::word`) as a whole word, case-insensitive; the joined form is not required. The question is about the first message so the answer names token A's words rather than B's or C's.

**9.3.8 `pager`** (P2-02 UAT-1, UAT-2). Count `L0`, the lines of `recent` holding `interrupt` (case-insensitive). `key ctrl+o`; within 5 s `visible` has a line starting `Transcript ·` and a line holding `following`. `typed("/")`, `typed(<A joined>)`, `key enter`; within 5 s a line holds `"<A joined>" <k> of <n>` with n at least 1. The prompt of token A gives the word and the number apart, so the joined token is in the reply and not in the user cell; P2-02 UAT-1 needs two matches only because its prompt names the word itself. Counts are relations (reasoning cells are searched too), so n may be larger and is never required to be exact. `typed("/")`, `typed("No such file")`, `key enter`; within 5 s `visible` holds `No such file or directory` and a footer with ` of ` and without `no match for`. (The two `/` here, like the one in 9.3.14, are pager search keys typed between Ctrl+O and `q`, not composer lines.) `key q`; within 5 s no line of `visible` starts `Transcript ·`. Last, `L1` (the same count) equals `L0` (neither Esc nor `q` interrupted anything; P2-02 UAT-4).

**9.3.9 `history-search`** (P2-13 UAT-3). Count `V0`, the lines of `visible` holding `<A word> and <A number>` (the plain-answer prompt of this run, P1-13 4.3: its word and number are unique to the run, so prompts that earlier runs left in the history file cannot match). `key ctrl+r`; `typed("<A word> and <A number>")`; within 5 s `visible` holds a line with the first prompt's text. `key enter`; within 3 s the count of those lines is `V0 + 1` (the composer now holds the loaded prompt; the box closed). `key ctrl+c`; within 3 s the count is `V0` again. Nothing is sent.

**9.3.10 `editor`** (P2-13 UAT-1). `typed("draft-one")`, `key ctrl+g`; within 10 s `visible` holds a line equal to `draft-one plus-two` (after the prompt marker) and no reply started (the screen is settled for 2 s and `recent` holds no new reply line; this is the one `recent` read of the step). `key ctrl+c` clears it.

**9.3.11 `theme`** (P2-12 UAT-1, UAT-5). `submit("/theme")`; within 5 s `recent` holds lines `Theme`, then (each trimmed line starting) `Skin:`, `Background:`, `Colours:` and `Motion:`. `submit("/skin ../etc")`; within 5 s a line starts `Usage: /skin [name].`. `submit("/theme purple")`; within 5 s a line starts `Usage: /theme [light|dark|auto]`. No line of `recent` after the first `/theme` holds `Hermes refused`. Only the line prefixes are checked, because the values depend on the terminal and environment of the run (`NO_COLOR`, `TALARIA_THEME`).

**9.3.12 `pane-state`** (P2-13 UAT-5; T6 is unverified). `pane_get(first)`; if its text holds no `talaria` or no `idle` (case-insensitive), SKIP with the pane-state reason of 9.4 (a skip, never a failure: the output format of `pane get` is not verified). Otherwise submit `Count from 1 to 2000, one number per line, and do not stop early.` (P1-13's interrupt prompt); within 60 s 15 counting lines (P1-13 5.5); `pane_get` must hold `working`; `key esc`, 400 ms; within 20 s the screen settles; within 10 s `pane_get` holds `idle`. Esc is sent as soon as `working` is seen, so the counted output stays small: the `quit` step reads only the last 3000 lines, and a count that scrolls token A out of that window would fail `quit` with its `committed output vanished` reason.

**9.3.13 `checklist`** (P2-11 UAT-2, UAT-3). Submit P2-11's plan prompt (its spec 9, verbatim): `Use your todo_list tool to write a plan with exactly three items: create a.txt, run sleep 30, create b.txt. Before each step mark it in_progress with the todo_list tool, and after it mark it completed. Do the steps in this folder. Then reply with the single word: done.` Within 120 s `visible` must match `(?i)\[>\] .*sleep` and hold a line starting `▾ Tasks`. If it does not and the screen settles, repeat once with the same prompt; a second miss is SKIP (`the model did not use the todo tool in two attempts`). With the panel shown: `key ctrl+t`; within 3 s `visible` holds exactly one line starting `▸ Tasks` and ending `Ctrl+T to expand` and, after it, no row holding `[ ]`, `[>]`, `[x]` or `[-]`; `key ctrl+t`; within 3 s a line starts `▾ Tasks` again and the `[>]` row is back. Wait up to 150 s for the screen to settle with no `▾ Tasks` or `▸ Tasks` line in `visible`; then `recent` must hold exactly one line matching `^Tasks [0-9]+/[0-9]+ · ` (the record P2-11 REQ-17 writes; the count rule is P2-11's UAT-2 and AC-6). The sleep step lasts 30 s, so both Ctrl+T reads happen inside it; if the panel has already gone the step fails with `checklist.collapse`.

**9.3.14 `quit`, `resume`**: P1-13 steps 4.6 and 4.7 with tokens A and B (B is `tool-shell`'s token) in place of its A and B, plus, in the resumed pane after the restore check and before the third prompt: (a) `recent` holds no line containing `! echo <D>` or `<D>` and no line starting `btw ·` or holding `side answer ·` (local cells are not stored by Hermes: P2-05 REQ-16); (b) the pager check of 9.3.8 reduced to its first search: `ctrl+o`, `/`, the A token, `enter`, a footer with ` of ` and n at least 1, then `q`; (c) when `checklist` passed, `recent` holds exactly one line containing `(from before this resume)` (P2-11 REQ-17); when it SKIPped, no check. The third prompt is token H (a plain-answer prompt), then `/quit`, as in P1-13.

**9.3.15 `tmux-pane`**. Write `launch.sh` (its `unset` line keeps the tmux session from reporting the Herdr pane state); `split_pane(first)` gives the `third` pane; wait for its shell (45 s, as P1-13); `run(third, "tmux new-session -s <session> '<scratch>/launch.sh'")` with `<session>` `talaria-smoke-<run id>`. Within 30 s `session_exists(<session>)` is true and `visible` of the third pane is unchanged for 2 s. If `exit_code()` becomes `Some(n)` first, fail with the exit reason. A `TmuxGuard` now owns the session.

**9.3.16 `paste-collapse`** (P2-04 UAT-2). `paste(<session>, <big1.txt text>)`; within 5 s `visible` of `third` holds exactly one `[Pasted text #<n> +120 lines]` with n equal to 1 and no line holding `the marker is`. `typed(" What is the marker on line 87? Reply with only the marker word.")`, `key enter`; within 60 s token E (case-insensitive whole word) appears in `recent` and the screen settles 3 s. The user cell shows the label, not the text, so token E can appear only in the reply.

**9.3.17 `file-drop`** (P2-04 UAT-4). `paste(<session>, "<drop folder>/notes-<hex>.txt")` (the absolute path, no newline); within 5 s a line holds `[File #1: notes-<hex>.txt]` (outside the session folder, so Hermes copies it and Talaria shows its own label). `typed(" What is the secret word? Reply with only that word.")`, `key enter`; within 60 s token F appears and the screen settles.

**9.3.18 `paste-editor`** (P2-04 UAT-2 with P2-13 UAT-1; XF-5's smoke form). `paste(<session>, <big2.txt text>)`; within 5 s one `[Pasted text #<m> +120 lines]` with m greater than 1 (paste numbers count up for the whole process: P2-04 REQ-13). `key ctrl+g`; within 10 s a line holds `[Pasted text #<m> +120 lines] plus-two`. `typed(" What is the marker on line 87 of the paste? Reply with only the marker word.")`, `key enter`; within 60 s token G appears and the screen settles.

**9.3.19 `tmux-quit`**. `typed("/quit")`, `key enter`; within 15 s `session_exists` is false and `exit_code()` is `Some(0)`; then `TmuxGuard::close`. Then the tab is closed as in P1-13 4.9.

### 9.4 Exact reason texts

Strings printed inside `FAIL <step> (...)`; `<n>` in a time is the limit in force after `--step-scale`, in whole seconds rounded to the nearest, at least 1. Tests assert them character for character; a reason the code has to change is reported on the issue. The phase 1 reasons of P1-13 spec 4.10 apply unchanged to `startup`, `plain-answer`, `quit` and `resume` (the shell, startup, deadline, signal and panic rows included).

| Step | Situation | Reason |
|---|---|---|
| tool-shell | token missing | `the reply token did not appear and settle within 90 s` |
| tool-shell | no header | `no tool cell '⏺ Shell(ls /talaria-no-such-dir)  ✗ exit <n>' appeared` |
| tool-shell | no output row | `the failing command's output row 'No such file or directory' is missing` |
| tool-edit | token missing | `the reply token did not appear and settle within 90 s` |
| tool-edit | no edit tool in two attempts | `the model did not use a file editing tool in two attempts` |
| tool-edit | no stat | `the edit cell lacks '✓ +1 -1'` |
| tool-edit | no diff rows | `the diff rows '2 - beta' and '2 + gamma' are missing` |
| tool-edit | debris | `the edit cell shows Hermes's diff header or escape debris` |
| tool-edit | file | `notes.txt does not hold the edit on disk` |
| palette | not open | `the slash palette did not list /version with its footer within 5 s` |
| palette | no block | `/vers and Enter did not print the /version block within 15 s` |
| palette | group | `the Talaria group of the palette lacks: <names joined by ', '>` |
| palette | not closed | `the palette stayed open after Esc and Ctrl+C` |
| mention | list | `the @ list did not show calc.py with its folder src within 5 s` |
| mention | tab | `Tab did not insert @file:src/calc.py within 3 s` |
| mention | reply | `the reply did not mention the file's content (average) within 60 s` |
| shell | output | `the output of the ! command did not appear within 5 s` |
| shell | refusal | `the refusal 'Hermes refused to run this: it looks dangerous' did not appear within 5 s` |
| shell | folder | `the refused command removed the keepme folder` |
| btw | no cell | `no side answer cell appeared within 90 s` |
| btw | failed | `the side question failed: <text after 'The side question failed:'>` |
| btw | empty | `the side answer was empty` |
| btw | word | `the side answer did not contain the word of the first token` |
| pager | open | `Ctrl+O did not open the transcript pager within 5 s` |
| pager | follow | `the pager footer does not say 'following'` |
| pager | token search | `searching the pager for the first token found no match` |
| pager | tool search | `the pager did not find the tool output 'No such file or directory'` |
| pager | close | `q did not return to the inline view within 5 s` |
| pager | interrupt | `the pager's q or Esc interrupted the agent` |
| history-search | box | `Ctrl+R did not list the earlier prompt within 5 s` |
| history-search | load | `Enter did not load the earlier prompt into the composer` |
| history-search | clear | `Ctrl+C did not clear the recalled draft` |
| editor | draft | `Ctrl+G did not return the edited draft 'draft-one plus-two' within 10 s` |
| editor | sent | `the edited draft was sent without Enter` |
| theme | report | `/theme did not print its report (missing: <names joined by ', '>) within 5 s` |
| theme | skin | `/skin ../etc did not print its usage line within 5 s` |
| theme | argument | `/theme purple did not print its usage line within 5 s` |
| theme | refused | `a bad /skin or /theme argument reached Hermes: a 'Hermes refused' line appeared` |
| pane-state | skip | `herdr pane get shows no agent state for talaria; read the Herdr sidebar by hand (unit P2-13 UAT-5)` |
| pane-state | counting | `fewer than 15 counting lines appeared within 60 s` |
| pane-state | working | `the pane did not report 'working' while the reply streamed` |
| pane-state | idle | `the pane did not report 'idle' within 10 s after the interrupt` |
| checklist | skip | `the model did not use the todo tool in two attempts` |
| checklist | collapse | `Ctrl+T did not collapse the task panel to one row within 3 s` |
| checklist | expand | `Ctrl+T did not expand the task panel within 3 s` |
| checklist | end | `the task turn did not end within 150 s` |
| checklist | panel left | `the task panel was still shown after the turn ended` |
| checklist | records | `the turn ended with <n> 'Tasks <c>/<t> · ' records in scrollback, expected 1` |
| resume (added) | local cell | `a local cell was printed again after resume ('! echo' or 'btw ·'): Hermes does not store it` |
| resume (added) | pager | `the pager of the resumed session did not find the first token` |
| resume (added) | record | `the resumed task list was printed <n> times, expected 1` |
| tmux-pane | start | `tmux did not start the talaria session within 30 s` |
| tmux-pane | exit | `talaria exited with code <n> before showing its screen` |
| paste-collapse | label | `the pasted text did not collapse into '[Pasted text #1 +120 lines]' within 5 s` |
| paste-collapse | leak | `the collapsed paste still shows its text in the composer` |
| paste-collapse | reply | `the reply did not contain the marker from the pasted text within 60 s` |
| file-drop | label | `the pasted file path did not become '[File #1: <name>]' within 5 s` |
| file-drop | reply | `the reply did not contain the word from the dropped file within 60 s` |
| paste-editor | label | `the second paste did not collapse into a '[Pasted text #<n> +120 lines]' label within 5 s` |
| paste-editor | editor | `the paste label did not survive the external editor` |
| paste-editor | reply | `the reply did not contain the marker from the second paste within 60 s` |
| tmux-quit | exit | `/quit in the tmux pane did not end talaria with exit code 0 within 15 s` |
| tmux-quit | session | `the tmux session was still running after /quit` |
| any | tmux error | `tmux failed: <message>` |
| any | pane get error | `herdr pane get failed: <message>` |

With `TALARIA_LIVE_REQUIRE=1` a skip prints `FAIL <step> (skipped: <skip reason>; TALARIA_LIVE_REQUIRE=1)`.

### 9.5 Talaria screen texts the steps depend on

Every text below is one `const` in `src/wording.rs` with a comment naming its source. They come only from the final bodies of the accepted issues. A text a step needs that no source states is not invented: the step is changed to need less, or the gap is reported on the source issue.

| Constant | Text | Source | Used by (steps) |
|---|---|---|---|
| `SHELL_HEADER` | `⏺ Shell(` ; failure mark `✗ exit ` ; `No such file or directory` | P2-01 UAT-3 | tool-shell |
| `EDIT_HEADERS`, `EDIT_STAT` | `⏺ Edit`, `⏺ Write` ; `✓ +1 -1` | P2-01 UAT-2 | tool-edit |
| `DIFF_ROWS`, `DEBRIS` | `2 - beta`, `2 + gamma` ; `┊`, ESC, `[38;2` | P2-01 UAT-2 | tool-edit |
| `PALETTE_FOOTER`, `PALETTE_GROUP`, `PALETTE_TAG` | `Tab complete` (inside `↑↓ choose · Tab complete · Enter run · Esc close`) ; `Talaria` ; `talaria` | P2-03 REQ-22, UAT-1 | palette |
| `LOCAL_COMMANDS` | `/quit`, `/btw`, `/skin`, `/theme` | P2-03 (the table; `/quit`), P2-05 REQ-7, P2-12 REQ-15, REQ-18 | palette |
| `MENTION_ROW`, `MENTION_INSERT`, `MENTION_FOOTER` | `calc.py` with `src` ; `@file:src/calc.py` ; `tab complete · enter insert · esc close` | P2-04 UAT-1, REQ-5 | mention |
| `PASTE_LABEL`, `FILE_LABEL` | `[Pasted text #<n> +120 lines]` ; `[File #<n>: <name>]` | P2-04 REQ-13, REQ-12, UAT-2, UAT-4 | paste-collapse, file-drop, paste-editor |
| `SHELL_CELL`, `SHELL_REFUSAL` | `! <first command line>` ; `Hermes refused to run this: it looks dangerous` (the sentence continues; only this prefix is checked) | P2-05 REQ-4, UAT-2 | shell, resume |
| `SIDE_HEADER`, `SIDE_FOOTER`, `SIDE_FAILED`, `SIDE_EMPTY`, `SIDE_PENDING` | `btw · ` ; `side answer · not part of the conversation` ; `The side question failed:` ; `(no answer)` ; `answering` | P2-05 REQ-10, REQ-12 | btw, resume |
| `PAGER_HEADER`, `PAGER_FOLLOW`, `PAGER_MATCH`, `PAGER_NOMATCH` | `Transcript ·` ; `following` ; `"<query>" <k> of <n>` ; `no match for` | P2-02 REQ-7, REQ-12, REQ-18 | pager, resume |
| `THEME_LINES`, `USAGE_SKIN`, `USAGE_THEME`, `REFUSED` | `Theme`, `Skin:`, `Background:`, `Colours:`, `Motion:` ; `Usage: /skin [name].` ; `Usage: /theme [light\|dark\|auto]` ; `Hermes refused` | P2-12 REQ-16, REQ-17, REQ-18, UAT-5 | theme |
| `TASK_EXPANDED`, `TASK_COLLAPSED`, `TASK_EXPAND_HINT`, `TASK_ACTIVE`, `TASK_RECORD`, `TASK_RESUMED` | `▾ Tasks` ; `▸ Tasks` ; `Ctrl+T to expand` ; `[>] ` ; `^Tasks <c>/<t> · ` ; `(from before this resume)` | P2-11 REQ-15, REQ-16, REQ-17, UAT-2 | checklist, resume |
| `AGENT_WORDS` | `talaria`, `idle`, `working` | P2-13 REQ-15 (state names), UAT-5; the format of `pane get` is T6, unverified | pane-state |
| `RESUMED_LINE` | `── Resumed:` | P1-12 | resume |

If a text differs on the real binary from its source issue, the step fails with its section 9.4 reason and prints the pane text; the tester finds the constant in the 'Used by' column and names it in the failure comment (REQ-4 of the issue). The tester then reads the screen: if Talaria drifted from its own issue, that is a defect for the source issue's owner; if the issue's text was edited, the constant and its source comment are updated in the same pull request that updates the step's test.

### 9.6 Time budget and defaults

| Item | Default | Notes |
|---|---|---|
| Whole run (`--deadline-secs`) | 1800 s for `phase2a` (300 s for `phase1`) | Counted from the first `herdr` call; a run still going fails with `deadline exceeded in step <name>`; cleanup still runs; never scaled, so a run with `--step-scale` above 1 also passes a larger `--deadline-secs`. The step limits below add up to about 1,660 s in the worst case (see the sum under the table), so the deadline sits above them and the step limit, not the deadline, names a slow step |
| Shell ready, startup, plain-answer, quit, resume | as P1-13 section 6 | scaled |
| tool-shell | 90 s | scaled |
| tool-edit | 90 s per attempt, two attempts | scaled |
| palette | 5 s, 15 s, 5 s, 3 s | scaled |
| mention | 5 s, 3 s, 60 s | scaled |
| shell | 5 s, 5 s | scaled |
| btw | 90 s | scaled |
| pager | 5 s per check | scaled |
| history-search, editor, theme | 3 to 10 s per check | scaled |
| pane-state | 60 s to count, 20 s to settle, 10 s for `idle` | scaled |
| checklist | 120 s to the panel (twice at most), 3 s per Ctrl+T check, 150 s to the end | scaled |
| tmux-pane | 45 s shell, 30 s session | scaled |
| paste-collapse, file-drop, paste-editor | 5 s label, 10 s editor, 60 s reply | scaled |
| tmux-quit | 15 s | scaled |
| Quiet periods | 1 s, 2 s, 3 s as in the steps above | never scaled |
| Wait after Esc | 400 ms | never scaled |
| Poll interval | 250 ms | never scaled |

Model turns: A, B, C, mention, btw, the count prompt, the plan (twice at worst), H, E, F, G: eleven to twelve, plus the 30 s sleep, so a normal run takes four to ten minutes. Worst case, every step using its whole limit: shell ready, startup and plain-answer 135 s; tool-shell and tool-edit 270 s; palette, mention, shell and btw 196 s; pager 30 s; history-search, editor and theme 38 s; pane-state 90 s; checklist 396 s; quit and resume about 140 s; tmux-pane 75 s; paste-collapse, file-drop and paste-editor 205 s; tmux-quit 15 s; about 1,590 s, plus about 70 s of quiet periods, so about 1,660 s. The 1800 s deadline is above that sum and is a ceiling, not a target. These figures are estimates from the number of turns, not measurements; the first live run replaces them with measured step times in the journal.

### 9.7 The tmux section

Why tmux and not Herdr for pastes: T7. Talaria enables bracketed paste mode (P1-08) and a tmux `paste-buffer -p` brackets only when the application asked for it (T2), which is exactly what a terminal's own paste does. The source scenarios use the same two commands (T5).

`TmuxCli::paste(session, text)` writes `text` to a mode 0600 file in the scratch folder (the text is never placed on a command line, which would break on quotes and length), runs `tmux load-buffer -b smoke-<run id> <file>` then `tmux paste-buffer -p -r -d -b smoke-<run id> -t <session>`, removes the file, and maps a non-zero exit to `TermError::Io` with tmux's message scrubbed. `session_exists` runs `tmux has-session -t <session>` (exit 0 true, exit 1 false; T3). `kill_session` runs `tmux kill-session -t <session>` and treats a missing session and a missing server (tmux's `can't find session` and `error connecting to` messages) as success. `exit_code` reads `tmux-exit.txt`: a line `SMOKE_EXIT:<n>` gives `Some(n)`, a missing file `None`.

Reads and keys of the third pane still go through Herdr (`herdr pane read`, `send-keys`, `send-text`), because the tmux client is attached in that pane and shows Talaria's screen; the tmux status line is one extra row in the pane text and is ignored by every helper (a row that is not part of any pattern). Sessions are named `talaria-smoke-<run id>`, so a leftover is found with `tmux ls | grep talaria-smoke` and killed with `tmux kill-session -t <name>`.

### 9.8 Screens and tool output to record

The eight issues have merged by the time this unit starts, so the test author records real screens from the built binary, not placeholders. Recording needs what a live run needs (the issue's operator preconditions: a gateway at the pinned release, the `fixture` profile with its key, `approvals.mode: manual`, a Herdr pane, `tmux`), so those hold from stage 0, not only for the run. The binary is a `target/debug/talaria` built from `main` after the eight issues merged (the pre-candidate build is fine: the recordings are test fixtures and prove nothing about the candidate). In a throwaway tab (never the operator's own panes) run the candidate-to-be `target/debug/talaria` with the `fixture` profile and capture, with `herdr pane read --source visible` (and `recent-unwrapped` where noted), the files below under `tests/fixtures/screens/phase2a/`. Scrub (section 5.3 of this document; replace the scratch path by `<scratch>`), keep at most 60 lines each, and note in `tests/fixtures/screens/phase2a/README.md` the Herdr version and how each file was made. A file that could not be recorded because a feature does not behave as its issue says is reported on that issue and replaced by a hand-built screen marked `hand-built` in the README until the defect is fixed. This is the only case where a screen is hand-built, and it does not conflict with the issue's rule that mocks come from real screens: the rule forbids inventing an interface (a method, a flag, a command line, an output format), and a screen the source issue describes is not an interface. A hand-built screen is written word for word from the source issue's own text, holds nothing the issue does not say, and is listed in the red-run record as `hand-built`. It lets the step's test exist; it never lets a row pass, because while the defect stands the row is FAIL and the candidate is rejected (REQ-15). Replace it with the real recording once the defect is fixed (a test-file change, made by the test author in a `test(smoke): ...` commit that names the defect issue). Any other hand-built screen, and any hand-built `herdr` or `tmux` output, is a defect of the test suite.

| File | Screen | Used by |
|---|---|---|
| `tool_shell_failed.txt` | the failed `ls` cell after the reply | `tool_header_*`, `tool_shell_*` |
| `tool_edit_diff.txt` | an edit cell with numbered rows | `edit_header_*`, `tool_edit_*` |
| `palette_open.txt`, `palette_browse.txt` | `/vers`, and bare `/` | `palette_*` |
| `mention_list.txt` | `Summarize @calc` with its list | `mention_*` |
| `paste_label.txt`, `file_label.txt` | after a bracketed paste and after a dropped path | `paste_*`, `file_*` |
| `shell_cells.txt` | an `!echo` cell and the refusal | `shell_*` |
| `side_answer.txt`, `side_failed.txt` | a `/btw` answer; a failed one if one can be provoked | `side_cell_*` |
| `pager_open.txt`, `pager_search.txt`, `pager_nomatch.txt` | the pager at the bottom, with a search, with no match | `pager_*` |
| `theme_report.txt`, `usage_lines.txt` | `/theme` output; the two usage lines | `theme_*` |
| `tasks_expanded.txt`, `tasks_collapsed.txt`, `tasks_record.txt` | the panel, collapsed, and the record | `task_*`, `checklist_*` |
| `pane_get.txt` | the real `herdr pane get` output for an idle Talaria pane, with its folder scrubbed (T6) | `agent_state_*`, `pane_state_*` |
| `history_box.txt`, `editor_result.txt` | Ctrl+R box; the composer after Ctrl+G | `history_*`, `editor_*` |

`tests/fixtures/tmux/` holds the recorded output of `tmux -V`, `tmux has-session` for an existing and a missing session and with no server (socket path replaced by `<tmp>`), and `tmux kill-session` for a missing session, plus a README with the tmux version.

### 9.9 Tests in detail

All tests follow P1-13's rules: mocks from recorded output and screens a real Talaria showed; no test or fixture holds an absolute path, home directory, login name or profile name other than `default` or `fixture` (build such needles at run time from a `tempfile` directory); no real pane is used; tests that start `smoke` remove `HERDR_ENV` and `HERDR_WORKSPACE_ID` from the child environment unless they set them. The screen script of P1-13 section 8.2 gains:

| Field | Meaning |
|---|---|
| `on: "paste"` | a rule fired by `Paster::paste` (the fake paster feeds the same machine; `matches` runs over the pasted text) |
| `write_in_cwd` | `{path, text}`: write this file under the tab's `--cwd` (what a real edit does) |
| `get` | the text `pane_get` returns from now on (a rule can change it: idle, working) |
| a third pane `third` | next to `first` and `second` |

`FakePaster` shares the machine, logs `paste`, `session_exists`, `kill_session` calls and returns a scripted `exit_code`. `fake-herdr` gains `pane get` (answers from the script's `get`); a `fake-tmux` helper binary (`tests/support/fake_tmux.rs`) answers from `tests/fixtures/tmux/`, with one state file for sessions, and logs its arguments.

Function lists (tests 1 to 8 of the issue; every name here is part of the contract, written in this order). Lists for tests 1, 4, 5, 6 and 7 are given after the notes.

`screen2a.rs` (22): `tool_header_found_with_mark_and_exit`, `tool_header_for_another_command_is_refused`, `edit_header_diff_rows_and_no_debris`, `palette_rows_footer_and_group`, `palette_footer_absent_when_closed`, `mention_row_and_inserted_text`, `paste_label_with_count_and_number`, `file_label_with_name`, `shell_cell_header_and_output_row`, `shell_refusal_prefix`, `side_cell_header_footer_and_answer_region`, `side_cell_failure_and_empty_answer`, `pager_header_following_and_k_of_n`, `inline_view_has_no_transcript_header`, `theme_report_and_usage_lines`, `task_panel_collapsed_expanded_and_record`, `record_count_exactly_one`, `pane_get_state_words`, `phase_one_counting_rules_unchanged`, `escape_and_box_characters_ignored`, `property_labels_never_match_plain_text`, `prompt_after_builds_the_sentence_and_keeps_the_token_out` (the sentence is `<action>, then join these two words with a hyphen and reply with only the result: <word> and <number>`; the joined `word-number` is not in it).

`steps2a.rs` (55 step functions, then 5 run-level ones). Each `<step>_passes` runs a script built from the recorded screens and asserts `PASS`, the keys sent in order and one `close_tab`. Each `_fails_` function is table-driven: it runs one script per 9.4 row named in the table and asserts the printed reason exactly, so every row of 9.4 is covered; the column '9.4 rows asserted' is the reason-to-test map (the test author copies it as a comment table at the top of `steps2a.rs`). The 55 functions of the first group include `happy_path_passes_every_step_and_closes_everything` and `launch_lines_set_editor_and_no_ensure`; a few do not follow the `<step>_passes` or `_fails_` pattern (`resume_checks_local_cells_are_gone`, `tmux_start_fails_with_exit_code`, `tmux_quit_needs_exit_zero_and_session_gone`); the names are the contract, the pattern is not.

| Function | 9.4 rows asserted |
|---|---|
| `happy_path_passes_every_step_and_closes_everything` | all 21 `PASS` lines, `SMOKE_RESULT: PASSED`, one `close_tab`, one `kill_session` |
| `launch_lines_set_editor_and_no_ensure` | every launch line holds `EDITOR='<scratch>/smoke-editor.sh'` and `--no-ensure` |
| `tool_shell_passes`, `tool_shell_fails_without_header` | tool-shell: token missing, no header |
| `tool_shell_fails_without_output_row`, `tool_shell_other_command_is_refused` | tool-shell: no output row; no header (a header for `ls /other`) |
| `tool_edit_passes_and_checks_the_file`, `tool_edit_retries_once_after_a_shell_edit` | the retry path (two prompts, the second stronger, a new token) |
| `tool_edit_fails_on_debris` | tool-edit: debris, no stat, no diff rows |
| `tool_edit_fails_when_file_unchanged_twice` | tool-edit: no edit tool in two attempts, file, token missing |
| `palette_passes`, `palette_fails_without_footer` | palette: not open, not closed |
| `palette_fails_without_version_block`, `palette_fails_when_group_lacks_a_command` | palette: no block; group (names listed in order) |
| `mention_passes`, `mention_fails_without_row`, `mention_fails_when_tab_inserts_nothing`, `mention_fails_when_reply_misses_the_word` | mention: list; tab; reply |
| `shell_passes`, `shell_fails_without_output`, `shell_refusal_needs_text_and_surviving_folder`, `shell_fails_when_folder_removed` | shell: output; refusal; folder |
| `btw_passes`, `btw_fails_without_footer`, `btw_fails_on_failed_question`, `btw_fails_when_answer_lacks_the_word` | btw: no cell and empty; failed; word (an answer naming only token B's word fails) |
| `pager_passes`, `pager_fails_when_not_following`, `pager_fails_when_a_search_finds_nothing`, `pager_fails_when_q_does_not_close` | pager: open and follow; token search and tool search (a footer `no match for ...`); close and interrupt (`pager_passes` accepts a footer `"<token>" 1 of 1`) |
| `history_search_passes`, `history_search_fails_on_empty_box`, `history_search_fails_when_draft_not_loaded` | history-search: box; load and clear |
| `editor_passes`, `editor_fails_when_draft_unchanged` | editor: draft, sent |
| `theme_passes`, `theme_fails_without_a_line`, `theme_fails_when_skin_usage_shows_a_refusal` | theme: report, skin, argument; refused |
| `pane_state_passes`, `pane_state_skips_when_no_agent_shown`, `pane_state_fails_without_working` | pane-state: skip (result line `PASSED WITH SKIPS (pane-state)`, exit 3); counting, working, idle |
| `checklist_passes_with_collapse_and_record`, `checklist_skips_after_two_attempts`, `checklist_fails_when_panel_stays`, `checklist_fails_with_two_records` | checklist: skip (two prompts sent, two settled screens); collapse, expand, end, panel left; records (`2`) |
| `resume_checks_local_cells_are_gone`, `resume_fails_when_record_printed_twice` | resume: local cell; record, pager |
| `tmux_start_fails_with_exit_code` | tmux-pane: start, exit |
| `paste_collapse_passes`, `paste_collapse_fails_without_label` | paste-collapse: label, leak, reply |
| `file_drop_passes`, `file_drop_fails_without_label` | file-drop: label, reply |
| `paste_editor_passes`, `paste_editor_fails_when_label_lost` | paste-editor: label, editor, reply |
| `tmux_quit_needs_exit_zero_and_session_gone` | tmux-quit: exit, session |
| `a_failing_step_stops_the_run_and_cleans_up` | later steps print nothing; one `close_tab`, one `kill_session`; any: tmux error, pane get error (one case each) |
| `deadline_names_the_step` | the 1800 s deadline, one step past it: `deadline exceeded in step <name>` |
| `a_skip_is_never_a_pass`, `live_require_turns_a_skip_into_a_failure` | `PASSED WITH SKIPS`, exit 3; with `TALARIA_LIVE_REQUIRE=1`: `FAIL pane-state (skipped: ...; TALARIA_LIVE_REQUIRE=1)`, exit 1 |
| `step_scale_leaves_quiet_periods` | with scale 0.5 the 5 s limits become 3 s (rounded) and the 3 s quiet period stays 3 s |

A function that the table lists against several 9.4 rows covers every one of them (for example `pane_state_fails_without_working` covers the pane-state reasons `counting`, `working` and `idle`, and `resume_fails_when_record_printed_twice` covers the record and pager reasons of resume); each such row still needs its own script inside the function, so no 9.4 row is untested, and no new function name is required. The test author may add functions, never remove or rename the listed ones.

Further notes (values and inputs):

1. `tmux_cli.rs`: the argument lists are exactly the two commands of T5 plus `-b smoke-<run id>` (their flags come from tmux's command list, T1: neither command prints anything on success, so no recorded output exists for them; only `-V`, `has-session` and `kill-session` have recorded output, section 9.8); the pasted file is mode 0600 and gone after the call; the text never appears in the logged arguments (a text with quotes, `$(` and a newline); `session_exists` is true on exit 0 and false on exit 1 for both error texts of T3; `kill_session` of a missing session is `Ok`; `exit_code` parses `SMOKE_EXIT:0` and `SMOKE_EXIT:7`, and a missing file is `None`; a missing executable is an error whose text names `tmux`.
2. `screen2a.rs`: the recorded screens give the expected structures; the negative inputs are a reply that merely quotes `⏺ Shell(ls /talaria-no-such-dir)` without the mark (`None`), a header for another command (`ls /other`), a diff row `2 - beta` shown in prose, `Tab complete` in a reply rather than a footer line (the footer must be within the last 25 lines and the line must hold the separator `·`), `Transcript ·` quoted mid-line (it must start the line), a task record quoted inside a code span, `herdr pane get` text without the agent; `escape_and_box_characters_ignored` puts box characters and an ESC inside rows; `property_labels_never_match_plain_text` (`proptest`): text without `[` never yields a label.
3. `steps2a.rs`: every step has a passing script built from the recorded screens (`happy_path_passes_every_step_and_closes_everything` is the passing script of startup, plain-answer, quit, resume, tmux-pane and tmux-quit, which have no separate `_passes` function), and a failing script per row of 9.4 that asserts the printed reason exactly, the keys sent in order, the times, and one `close_tab`. Specific cases: `tool_edit_retries_once_after_a_shell_edit` (first attempt shows `⏺ Shell(sed ...)`, the file changed: reset, second attempt passes, two prompts sent, the second with the stronger wording); `palette_fails_when_group_lacks_a_command` (the reason lists the missing names in order); `btw_fails_when_answer_lacks_the_word` (an answer naming only token B's word fails); `pager_fails_when_a_search_finds_nothing` (a footer `no match for` fails the token search; another fails the tool search; a footer `1 of 1` passes in `pager_passes`); `pane_state_skips_when_no_agent_shown` (the first `pane get` holds no `talaria`: `SKIP`, the run continues, the result line is `PASSED WITH SKIPS (pane-state)` and exit 3); `checklist_skips_after_two_attempts` (two prompts, two settled screens without a panel); `checklist_fails_with_two_records` (`records` reason with `2`); `resume_checks_local_cells_are_gone` (a resumed pane that still shows `btw ·` fails with the `local cell` reason); `live_require_turns_a_skip_into_a_failure` (`FAIL pane-state (skipped: ...; TALARIA_LIVE_REQUIRE=1)`, exit 1); `a_skip_is_never_a_pass`; `step_scale_leaves_quiet_periods` (with scale 0.5 the 5 s limits become 3 s (rounded) and the 3 s quiet period stays 3 s); `deadline_names_the_step` (the 1800 s deadline, one step past it).
4. `typed_input2a.rs`: over every path of test 3, collect all text and keys sent: none contains `unattended`; every slash line is one of `/quit`, `/vers`, `/theme`, `/theme purple`, `/skin ../etc` or starts `/btw ` (a slash line is text starting `/` that is sent with Enter outside the pager; `/version` is never typed, it is reached by completing `/vers`; a bare `/` typed and then cleared with Esc and Ctrl+C is allowed; the test tracks the pager as the span from `ctrl+o` to `q`, and text typed inside it is search input, exempt; it also fails a bare `/` that is followed by Enter outside the pager); the only line starting `!` other than `!echo <token>` is exactly `!rm -r ./keepme`; no line contains `hermes update`, `config.set` or `rm -rf`; no line contains an absolute path except the pasted drop-file path (sent through `paste`, not `run`) and the launch lines (which hold the scratch path and the binary path).
5. `cleanup2a.rs`: as P1-13's `cleanup.rs`, with the tmux session: a `FakeTerminal` that panics at a chosen call gives `FAIL <step> (internal error: ...)`, one `close_tab` and one `kill_session`; SIGINT and SIGTERM go to the real `smoke` against `fake-herdr` and `fake-tmux` once the fake logged `pane run`: `tab close` and `kill-session` logged, `SMOKE_RESULT: FAILED (interrupted)`, exit 1; a tmux session that will not die ends `SMOKE_RESULT: FAILED (cleanup)` with the line `cleanup: tmux session <name> was not closed`; scratch and drop folders are gone on every path.
6. `cli2a.rs`: `--help` lists both new options; no `--suite` equals `phase1` and its output is byte for byte that of P1-13's `cli.rs` expectations; `--suite phase2a` without a working `--tmux` exits 2 with one sentence (`tmux was not found or did not run: <path>`) and creates nothing, while the same missing `--tmux` with `--suite phase1` (or no suite) changes nothing in the phase 1 output; a scratch path with a space or a single quote exits 2 for `phase2a` only; `--suite bogus` exits 2 with the FromStr text; the phase 2a default deadline is 1800 and an explicit `--deadline-secs 300` wins; the scratch folder is mode 0700 and holds `launch.sh` (its second line is the `unset HERDR_ENV HERDR_PANE_ID HERDR_BIN_PATH` line) and `smoke-editor.sh` with mode 0700 and the files of 9.3.0; the failure report names `first`, `second` and `third` and holds no private path.
7. `fixtures_public_safe2a.rs`: asserts the files of 9.8 were scanned, then scans them and the `tmux` fixtures for home directories, login names, temporary paths and profile names other than `default` and `fixture`.
8. `live_smoke2a.rs`: runs `smoke run --suite phase2a --talaria <debug build>`; fails (never skips) without `HERDR_ENV`.

Function lists for the other test files (every name is part of the contract; the notes above give the inputs and expected values; the test author may add functions, never remove or rename these):

- `tmux_cli.rs` (6): `paste_loads_a_private_buffer_then_pastes_with_p_and_r`, `pasted_file_is_private_and_removed_and_text_never_in_arguments`, `session_exists_follows_the_exit_status_for_both_error_texts`, `kill_session_treats_a_missing_session_or_server_as_ok`, `exit_code_reads_the_marker_file`, `missing_tmux_is_a_clear_error`.
- `typed_input2a.rs` (4): `nothing_typed_contains_unattended`, `slash_lines_are_the_allowed_list`, `only_destructive_line_is_the_refused_rm_r_keepme`, `no_line_runs_hermes_update_or_config_set`.
- `cleanup2a.rs` (7): `tab_and_session_closed_after_pass`, `tab_and_session_closed_after_a_failing_step`, `tab_and_session_closed_after_a_panic`, `sigint_closes_tab_and_session`, `sigterm_closes_tab_and_session`, `an_unkillable_session_ends_with_cleanup_failed`, `scratch_and_drop_folders_removed_on_every_path`.
- `cli2a.rs` (8): `help_lists_suite_and_tmux_options`, `default_suite_is_phase1_and_its_output_is_unchanged`, `phase2a_without_working_tmux_exits_2_and_creates_nothing`, `scratch_path_with_space_or_quote_exits_2_for_phase2a_only`, `unknown_suite_exits_2_with_the_parse_text`, `phase2a_deadline_defaults_to_1800_and_an_explicit_value_wins`, `scratch_folder_is_private_and_holds_the_launch_and_editor_scripts`, `failure_report_names_three_panes_and_is_scrubbed`.
- `fixtures_public_safe2a.rs` (1): `phase2a_fixtures_hold_no_paths_or_names`. `live_smoke2a.rs` (1, ignored): `live_phase2a_smoke_in_herdr`.

Where the red run is recorded: the test author pastes it into the description of the pull request of branch `p2a-acc-smoke` under a heading `Red run`, one line per test file or function with the first failure line (`not implemented` or the assertion), the `hand-built` screens listed, and no path; the checker tests' red run goes in a comment on the issue. The reviewer reads it before the developer's code is reviewed. AC-1 checks only the commit order, so the reading is the control.

Red runs: each test fails on its stub for the reason it names (`not implemented`); test 7 fails because the recorded files it must scan are present but the scan helper is a stub, and test 8 fails on the stub `smoke` binary (a stub that exits 1 or panics), never on a missing file. The recorded screens and tmux outputs are committed in the first commit with the stubs, so no test fails for a missing fixture.

### 9.10 Cleanup, signals, safety

Cleanup extends P1-13's (sections 7.1 to 7.4): the `TabGuard` closes the tab (and all three panes); the `TmuxGuard` kills the tmux session before the tab closes (a session left behind would hold Talaria running after the pane is gone); the scratch and drop folders are removed on every path; a signal ends the run through the normal return path; an unclosed tab or session gives `SMOKE_RESULT: FAILED (cleanup)`; all output is scrubbed with the drop folder as one more needle (`<drop>`).

Safety rules (REQ-5 of the issue and P1-13 REQ-15): no `unattended`, no `config.set`, no `hermes update`, no gateway start (`--no-ensure` on every launch, including the one in `launch.sh`), no provider key read or printed; the only slash lines sent are `/quit`, `/vers` (completes to `/version`), `/theme`, `/theme purple`, `/skin ../etc` (refused by Talaria before anything is sent: P2-12 REQ-16) and `/btw <question>`; a bare `/` is typed and never sent, and search text typed inside the pager is exempt (the rule of the issue's REQ-5 and of `typed_input2a.rs`); the only destructive text is `!rm -r ./keepme`, refused by Hermes. Sessions stay in the `fixture` profile. Residue per run: the sessions the run created (as phase 1), one small `notes-<hex>.txt` copy in the profile's attachments folder (P2-04 REQ-12), and entries in Talaria's own prompt history file (P2-13 REQ-6).

### 9.11 README additions

`tools/smoke/README.md` keeps its five level-two headings. `## Usage` gains the `--suite` and `--tmux` options; `## Preconditions` gains `tmux`, the free model's tool use (the file editing and todo tools), a local terminal backend and the attachments residue; `## Steps` gains the 21 steps of the phase 2a suite; `## Exit codes` states that `pane-state` and `checklist` can skip; `## Reading a failure` gains one paragraph: a reason that names a constant of `wording.rs` and a pane text that differs from the source issue's text is a Talaria defect or an edited issue, never a reason to relax the step.

### 9.12 Coverage map: which feature is checked where

| Source | Smoke step | Cross-feature row | Source scenarios | Not automated, and why |
|---|---|---|---|---|
| P2-01 tool cells | tool-shell, tool-edit | XF-1, XF-7, XF-10, XF-11 | UAT-1 to UAT-6 | colours and copy in real terminals (operator) |
| P2-02 pager | pager, resume | XF-1, XF-3, XF-4, XF-8, XF-11 | UAT-1 to UAT-6 | none |
| P2-03 palette | palette | XF-2 | UAT-1 to UAT-7 | a new skill appearing (operator) |
| P2-04 mentions | mention, paste-collapse, file-drop, paste-editor | XF-2, XF-5, XF-6 | UAT-1 to UAT-5 | clipboard image (operator) |
| P2-05 shell, `/btw` | shell, btw | XF-3, XF-9, XF-10, XF-11 | UAT-1 to UAT-5 | a running `!` delaying an interrupt (not provoked) |
| P2-11 checklist | checklist, resume | XF-4, XF-11 | UAT-1 to UAT-6 | hostile item text (offline demo) |
| P2-12 themes | theme | XF-7 | UAT-1 to UAT-6 | the skin switch, light and dark detection (operator) |
| P2-13 comforts | history-search, editor, pane-state, paste-editor | XF-5, XF-6, XF-8 | UAT-1 to UAT-7 | suspend, notifications (job control, real terminals) |

### 9.13 Journal entries

`LEARNINGS.md`, one entry per learning of the run with Evidence, Mechanism and a one-line rule, when one exists (for example what `herdr pane get` really prints, whether `tmux paste-buffer -p` reached Talaria as one paste, which wording drifted). No `DECISIONS.md` entry is expected unless the operator changes the shape of the record (section 1).

## 10. Questions the operator owns, and facts not verified

Decisions, to confirm on the parent issue before the candidate is frozen (building may start first; a rejection is a stop condition):

1. The shape of the record: two markdown files and a copied checker (section 1). Draft ADR-0012 decision 4 reads "No per-release receipt schemas, digest-bound evidence trees or release-record tests."; the operator rules whether the checker and its tests are release-record tests. Two shapes are offered; a third is an operator edit of the issue before the freeze.
   - Shape A (the operator rules they are not): the two files and the checker, as sections 4 to 6.
   - Shape B (the fallback; silence at the freeze means B): no `phase-2a*` or `check-phase-2a*` file is committed. The table of section 4 and the evidence sections of section 5 live in the record pull request. GitHub limits a body to 65,536 characters, so the body holds the table and as many evidence sections as fit; the rest go in comments of that pull request, one comment per group of rows, each section keeping its `## <id>` heading. The pull request needs a diff: it carries the journal entry; when the run taught nothing and there is no diff, the lead puts the record in comments on this issue instead and reads them with `gh issue view "$ISSUE" -R "$R" --json comments` in place of `gh pr view` below (verify at first use that GitHub accepts the form chosen). The lead edits the issue to remove `check-phase-2a.sh`, `check-phase-2a.test.sh`, the red run of the record, AC-4, AC-9, AC-12, UAT-1 and UAT-2, and the `phase-2a*` and `check-phase-2a*` paths from "Files expected to change"; AC-6 is unchanged. Let `B=$(gh pr view "$PR" -R "$R" --json body,comments --jq '[.body, (.comments[].body)] | join("\n")')`. The commands that replace the removed readings: AC-5 `echo "$B" | grep -cE '^(Candidate|Hermes|Herdr|OS|Date): '` prints 5, `echo "$B" | grep -cE '^(Hermes|Herdr|OS|Date): .*<'` prints 0, `echo "$B" | grep -cE '^\| (AUTO|P2|XF)'` prints 69; AC-7 `echo "$B" | grep -E '^\| (AUTO|P2|XF)' | grep -vcE '^\| [^|]+\| [^|]+\| PASS \|'` prints 0; AC-10 the same awk, reading `echo "$B" |` in place of the evidence file; AC-11 the same three scans on `echo "$B" |` in place of `$E` (each login name prints 0). A reviewer reads every PASS row's commit and evidence, because no checker does. Sections 4 to 6 of this document then serve only as templates.
2. tmux as a dependency of the paste steps (section 9.7). Alternative: a Herdr paste verb, if Herdr offers one that brackets (T7), would remove tmux from the suite.
3. The 1800 s deadline of the phase 2a suite (above the sum of the step limits, section 9.6) and the idle-cost limits of XF-9 (the growth limit of 4096 KiB and the CPU limit of 1 s per two minutes are this issue's own sanity checks, as in PH1-ACC; the 40 and 100 MiB limits are the architecture's). The operator confirms them in a comment on [`PH2A`](https://github.com/infiquetra/talaria/issues/213) before the freeze; silence means they stand, and a rejection is a stop condition of the issue.
4. Accepting that `file-drop` leaves one small file per run in the `fixture` profile's attachments folder.

Facts not verified, to check at build time and record:

- T6 and T7 (the output of `herdr pane get`; whether Herdr brackets pasted text). If `pane get` shows no agent state, `pane-state` SKIPs, AUTO-9 and AUTO-10 cannot pass, and the operator decides before the freeze: have the cause fixed, or drop the step and rows by editing the issue.
- Whether a tmux server started inside a Herdr pane hands the Herdr variables to the process inside it: the launch script unsets `HERDR_ENV`, `HERDR_PANE_ID` and `HERDR_BIN_PATH` so it does not matter; record what is observed.
- The length of the free model's `sleep` in the `checklist` step. Both Ctrl+T checks must fall inside the 30 s sleep (9.3.13); the first live run records the measured sleep and step times in the journal, and a shorter sleep is a stop condition of the issue.
- Whether a `workflow_dispatch` run reports a check run on the candidate (AUTO-1), and the macOS `ps -o time=` format (`M:SS.cc`, checked on 2026-10-22).
- That Ctrl+C clears a non-empty draft and Esc closes the palette and the `@` list without interrupting (P1-08, P2-03 REQ-20, P2-04 REQ-6): the `palette`, `history-search`, `editor` steps and XF-2 rely on them.
- That `/vers` plus Enter completes and runs `/version` (P2-03 UAT-2) and that the Talaria group lists exactly the four local commands (P2-03, P2-05, P2-12 as written); if a later unit adds a local command the check is a subset check, so it still holds.
- That the pager draws `!` and `/btw` cells searchably (XF-3): P2-02 says "other kinds go to the other renderer", and P2-05 does not say how the pager draws its cells. A FAIL there is a seam defect, not a flaw of the row.
- That a resumed session prints a task record once when idle (P2-11 REQ-11) and that Hermes keeps no `!` or `/btw` cells (P2-05 REQ-16).
- The real wording of every constant in 9.5 on the built binary.

## Appendix A: the verbatim scenarios of the accepted issues

Extracted by script from the final bodies of the eight issues on the day this document was written, with the double-brace issue references written as plain ids. Use the source issue's own text if it was edited since. Each scenario's Persona and Runner decide who runs it (section 7.2). The preamble of each source is its common setup.

### A.P2-01: scenarios of P2-01

Common live setup (operator preconditions in "Dependencies and gates"): `mkdir -p "$TMPDIR/tcproj" && printf 'alpha\nbeta\n' > "$TMPDIR/tcproj/notes.txt"` (the same file as `P1-04`'s fixture); `herdr pane split --current --direction right --cwd "$TMPDIR/tcproj" --no-focus` (pane `P`); `herdr pane run P "talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash"`. Prompts are typed with `herdr pane run P "<text>"`; the reply is awaited with `herdr pane wait-output P --regex '(?mi)^\W*done\W*$' --source recent-unwrapped --timeout 120000` (it cannot match the longer prompt line). "Read `P`" means `herdr pane read P` with `--source visible` and with `--source recent-unwrapped --lines 60`, both attached; a Then line passes when either current read shows it (one source can be empty or stale). Never send `unattended: true`.

#### UAT-1: A recorded edit renders as a numbered diff without a gateway
- Persona: developer
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: the branch built; no gateway; a 100-column pane `D` in the repository.
- Given the recorded `fixtures/frames/tool-diff.jsonl`
- When the tester runs `herdr pane run D "cargo run -q -p talaria-app --example tool_cells_demo -- fixtures/frames/tool-diff.jsonl"`, reads `herdr pane read D --source recent --lines 40`, then repeats with `NO_COLOR=1` before `cargo`
- Then a row starting `⏺ Edit` (or `⏺ Write`) holds `✓ +1 -1`, followed by rows `2 - beta` and `2 + gamma`; `diff` of the two reads' tool rows is empty
- Evidence: both pane reads
- Result: NOT RUN

#### UAT-2: A live edit shows its diff in scrollback
- Persona: developer
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: common live setup.
- Given Talaria running in `P`
- When the tester types `In notes.txt change the word beta to gamma. Use your file editing tool, then reply with the single word: done.`, waits for the reply, then reads `P`
- Then a row starting `⏺ Edit` or `⏺ Write` holds `notes.txt`, `✓ +1 -1` and a duration, then rows `1   alpha`, `2 - beta`, `2 + gamma`; no `┊` and no escape debris such as `[38;2`
- Evidence: the pane read; `cat "$TMPDIR/tcproj/notes.txt"` prints `alpha` and `gamma`
- Result: NOT RUN

#### UAT-3: Shell output preview and a failing command (negative case)
- Persona: developer
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: common live setup.
- Given Talaria running in `P`
- When the tester types `Run the shell command seq 1 10, then run the shell command ls /talaria-no-such-dir, then reply with the single word: done.`, waits for the reply, then reads `P`
- Then `⏺ Shell(seq 1 10)  ✓` and a duration, followed by `  ⎿  1`, `2`, `3` and `… +7 lines`; and a row holding `⏺ Shell(ls /talaria-no-such-dir)  ✗ exit ` and a number is followed by a row holding `No such file or directory`
- Evidence: the pane read
- Result: NOT RUN

#### UAT-4: Interrupting a running tool leaves a "not finished" cell (negative case)
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: common live setup.
- Given Talaria running in `P`
- When the tester types `Run the shell command sleep 30, then reply with the single word: done.`, runs `herdr pane wait-output P --regex 'running [0-9]+s' --source recent --timeout 60000`, reads `P`, presses `herdr pane send-keys P esc` (`P1-08`'s interrupt key), waits 5 seconds and reads `P`
- Then the first read shows `⏺ Shell(sleep 30)  … running` and a number of seconds; the last shows `⏺ Shell(sleep 30)  ◌ not finished`, no `✓` for it, and no `Preparing` line
- Evidence: both pane reads; seconds from Esc to the "not finished" row
- Result: NOT RUN

#### UAT-5: Hostile output stays literal (negative case)
- Persona: developer
- Covers: AC-9, AC-11
- Runner: agent tester in Herdr
- Preconditions: the branch built; `crates/talaria-app/tests/data/tool_cells/hostile.jsonl` (spec 4: `terminal` output holding a screen clear, a title change and a right-to-left override).
- Given a Herdr pane `D` with some earlier text on screen
- When the tester runs `herdr pane run D "cargo run -q -p talaria-app --example tool_cells_demo -- crates/talaria-app/tests/data/tool_cells/hostile.jsonl"` and `herdr pane read D --source recent --lines 30`
- Then the earlier text is still on screen, Herdr shows the same pane title as before, and the output rows hold the words `before`, `after` and `evil` as text
- Evidence: the pane read; the pane title before and after
- Result: NOT RUN

#### UAT-6: Colours, scrollback search and copy in three terminals
- Persona: developer
- Covers: AC-9
- Runner: operator
- Preconditions: common live setup in Herdr, then the same in tmux and in one plain terminal.
- Given a session where UAT-2 and UAT-3 ran
- When the operator scrolls back, searches for `gamma` and copies one added diff row
- Then added rows are green, removed rows red, the search finds the row, the copy is `2 + gamma` with no stray characters, and nothing is drawn twice, in each terminal
- Evidence: one screenshot per terminal and the pasted copy
- Result: NOT RUN

### A.P2-02: scenarios of P2-02

Common setup: preconditions hold; `talaria` on `PATH` is this branch's release build (`command -v talaria` names it); `-c`, `--provider`, `--model` and `/quit` are `P1-12`'s (REQ-1, REQ-9, REQ-19); `mkdir -p "$TMPDIR/pager/.pytest_cache"`; `herdr pane split --current --direction right --cwd "$TMPDIR/pager" --no-focus` (pane `P`); `herdr pane run P "talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash"`. Awaited words are capitalised only in replies (`wait-output` also matches text on screen). Counts are relations: reasoning cells are searched too. Reads use `herdr pane read P --source visible`.

#### UAT-1: Find a word in the transcript, with smart case
- Persona: developer
- Covers: AC-7
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given `herdr pane run P "Reply with the word heron in capital letters and nothing else."` and `herdr pane wait-output P --match HERON --timeout 120000`
- When the tester sends `ctrl+o` (`herdr pane send-keys P ctrl+o`), reads, sends `/`, `herdr pane send-text P heron`, `enter`, reads, then `/`, `HERON`, `enter`, reads
- Then the first read has a top line starting ` Transcript ·` and a footer ending `following`; the second footer reads `"heron" <k> of <n>` with n ≥ 2; the third reads `"HERON" <k> of <m>` with 1 ≤ m < n
- Evidence: the three reads
- Result: NOT RUN

#### UAT-2: Tool output is expanded and searchable
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given `herdr pane run P 'Run the shell command echo PIPIT-$((6*7)) and then reply with the word done in capital letters.'` and `herdr pane wait-output P --match DONE --timeout 180000` (no approval card is expected; if one appears for that command, press `1` for `once`)
- When the tester sends `ctrl+o`, `/`, types `pipit-42`, presses `enter` and reads
- Then the pager shows a line starting `⏺` for Hermes's shell tool `terminal` (`terminal(…)` before `P2-01` merges, `Shell(…)` after) and, below it, a line containing `PIPIT-42`; the footer contains `"pipit-42"` and ` of `
- Evidence: the read
- Result: NOT RUN

#### UAT-3: Following new output, scrolling away and jumping back
- Persona: developer
- Covers: AC-9
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given `herdr pane run P "Count from 1 to 150, one number per line, then write the word ready in capital letters."`, `herdr pane wait-output P --match READY --timeout 120000`, then `herdr pane run P "Run the shell command sleep 10, then count from 1 to 80, one number per line, then write the word finish in capital letters."` and at once `ctrl+o`
- When the tester sends `g`, waits 20 seconds, reads; sends `G`, runs `herdr pane wait-output P --match FINISH --timeout 120000`, reads
- Then the first footer contains `cell 1/` and `↓ new output · End` and no `following`; the second read shows `FINISH` on the last body line and `following`
- Evidence: both reads
- Result: NOT RUN

#### UAT-4: Esc and a failed search never stop the agent (negative)
- Persona: developer
- Covers: AC-10
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given `herdr pane run P "Run the shell command sleep 20, then reply with the word omega in capital letters."` and, 3 seconds later, `ctrl+o`
- When the tester sends `/`, types `zzqx-nomatch`, `enter`, reads; `esc`, reads; `esc`, reads; then runs `herdr pane wait-output P --match OMEGA --timeout 120000`
- Then the first footer contains `no match for "zzqx-nomatch"`; the second lacks it; the third is the inline view (no ` Transcript ·` line); `wait-output` exits 0 and `herdr pane read P --source recent --lines 80 | grep -ci interrupt` prints 0
- Evidence: reads, exit code
- Result: NOT RUN

#### UAT-5: An approval arriving while the pager is open is announced
- Persona: developer
- Covers: AC-11
- Runner: agent tester in Herdr
- Preconditions: common setup; operator sets up approval mode `manual` on `fixture`. Hermes's detector flags `rm -rf .pytest_cache`, so a card is expected.
- Given `herdr pane run P "Run the shell command rm -rf .pytest_cache in this folder."` and at once `ctrl+o`
- When the tester runs `herdr pane wait-output P --match 'waiting for your answer' --timeout 120000`, reads, sends `q`, reads, then sends `esc` (Esc on a `P1-09` approval card sends `deny`)
- Then the first read shows `⚑ Hermes is waiting for your answer.` under the header; after `q` the card for `rm -rf .pytest_cache` is visible; the folder still exists after the deny
- Evidence: both reads; `test -d "$TMPDIR/pager/.pytest_cache"; echo $?` prints 0
- Result: NOT RUN

#### UAT-6: History from before a resume is in the pager
- Persona: developer
- Covers: AC-12
- Runner: agent tester in Herdr
- Preconditions: common setup.
- Given `herdr pane run P "Reply with the word kestrel in capital letters and nothing else."`, `herdr pane wait-output P --match KESTREL --timeout 120000`, `herdr pane run P "/quit"`, `herdr pane run P "talaria -c --profile fixture"`
- When the tester sends `ctrl+o`, `/`, types `kestrel`, `enter`, reads, then sends `n` and reads until `<k>` returns to its first value
- Then the footer reads `"kestrel" <k> of <n>` with n ≥ 2 and the header shows `idle`; one read shows a line starting `› Reply with the word kestrel`, another a line containing `KESTREL`
- Evidence: the reads
- Result: NOT RUN

### A.P2-03: scenarios of P2-03

Release build in a Herdr pane at 120×40; capture with `herdr pane read <pane> --source visible`. "Started as UAT-1": operator sets up the `fixture` profile; `talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud` runs in a scratch folder under `$TMPDIR`. Captures follow AC-11's redaction rule.

#### UAT-1: Browse every command with provenance
- Persona: developer
- Covers: AC-1, AC-4
- Runner: agent tester in Herdr
- Preconditions: started as UAT-1
- Given an idle session, empty composer
- When the tester types `/`, waits one second, then presses Down repeatedly, capturing after every 8 presses, until `/quit` is selected again
- Then the first capture shows header `Talaria` with `/quit` tagged `talaria`, header `Session` with rows tagged `hermes`, and the footer `↑↓ choose · Tab complete · Enter run · Esc close` with `<m> of <m>` (equal counts), m at least 50; later captures show `Configuration`, `Info`, `Tools & Skills` in that order; none shows `Exit`, `/density`, `/logs` or `/mouse`
- Evidence: every capture
- Result: NOT RUN

#### UAT-2: Filter, complete and run a Hermes read command
- Persona: developer
- Covers: AC-2, AC-4
- Runner: agent tester in Herdr
- Preconditions: started as UAT-1
- Given an idle session, empty composer
- When the tester types `/vers`, presses Enter, then types `/hel`, presses Tab, then Enter
- Then after the first Enter the composer is empty, the palette gone, and scrollback shows a block starting `/version` with Hermes's version text; after Tab the composer reads `/help `; after Enter scrollback shows `/help` and a command list; no turn starts
- Evidence: pane text after each Enter
- Result: NOT RUN

#### UAT-3: A prompt-building command shows what was typed, not the scaffold
- Persona: developer
- Covers: AC-2, AC-5
- Runner: agent tester in Herdr
- Preconditions: started as UAT-1
- Given an idle session
- When the tester types `/plan write a haiku about rain` and presses Enter
- Then a System line `Planning: write a haiku about rain`, a user line `/plan write a haiku about rain`, a turn and a reply appear; no line contains `[/plan — plan mode]` or `Task to plan:`
- Evidence: pane text to the end of the reply
- Result: NOT RUN

#### UAT-4: A refused command is explained and then dimmed (negative)
- Persona: developer
- Covers: AC-3
- Runner: agent tester in Herdr
- Preconditions: started as UAT-1
- Given an idle session
- When the tester types `/worktree`, presses Enter, then types `/work`
- Then scrollback shows `/worktree is not available on the shared Hermes gateway.` and, next line, Hermes's hint starting "Start in a fresh worktree with"; no turn starts; the palette shows `/worktree` dimmed, tag `not available here`
- Evidence: pane text
- Result: NOT RUN

#### UAT-5: An unknown command sends nothing to the model (negative)
- Persona: developer
- Covers: AC-3
- Runner: agent tester in Herdr
- Preconditions: started as UAT-1
- Given an idle session
- When the tester types `/nonexistent-talaria-check please` and presses Enter
- Then scrollback shows `Hermes does not know /nonexistent-talaria-check. Type / to see the commands you can use.`; no user line, no turn; the session cost is unchanged
- Evidence: pane text before and after
- Result: NOT RUN

#### UAT-6: An installed skill appears without a restart and runs as typed
- Persona: configurer
- Covers: AC-5, AC-6
- Runner: operator
- Preconditions: started as UAT-1; the throwaway skill of "Dependencies and gates" absent
- Given Talaria open on the `fixture` profile
- When the operator creates the skill, waits 10 s, types `/talaria-uat`; runs `/talaria-uat-skill tidy` and waits for the reply; removes the skill, waits 10 s, types `/talaria-uat` again
- Then a row `/talaria-uat-skill` tagged `skill · local` is listed first; the run shows a user line `/talaria-uat-skill tidy`, a turn and a reply, and no line contains `[IMPORTANT: The user has invoked the`; at the end the row is gone; Talaria was never restarted
- Evidence: pane text at each step; elapsed times
- Result: NOT RUN

#### UAT-7: Esc closes the palette mid-turn; commands follow Hermes's busy rules (negative)
- Persona: developer
- Covers: AC-4
- Runner: agent tester in Herdr
- Preconditions: started as UAT-1
- Given a turn running from `Count from 1 to 400, one number per line.` (1000 if it ends before the keys)
- When, while it streams, the tester types `/he`, presses Esc, clears the draft, types `/version` + Enter, then `/plan x` + Enter
- Then after Esc the palette is gone, `/he` stays and the status line still shows working; a `/version` block appears; a System line containing `can't run mid-turn` appears and no user line `/plan x`; the reply ends with no `Interrupted` notice
- Evidence: pane text after each key; the reply's last lines
- Result: NOT RUN

### A.P2-04: scenarios of P2-04

Shared setup: the operator sets up `fixture` on the pinned Hermes (terminal backend `local`). A temporary project holds `src/calc.py` (`def average(values): return sum(values) / len(values)`). In a Herdr pane there, run `tmux new-session -s uat 'talaria --profile fixture --model minimax-m3.1-flash --provider ollama-cloud'` (every scenario). Text: `herdr pane send-text <pane> '<text>'` (no Enter); keys: `herdr pane send-keys <pane> <key>`; reads: `herdr pane read <pane> --source visible`. A bracketed paste, from the tester's own shell: `tmux load-buffer -b p <file>`, then `tmux paste-buffer -p -r -d -b p -t uat` (`-p` adds the paste brackets Talaria asked for; `-r` keeps line feeds).

#### UAT-1: Mention a file with @
- Persona: developer
- Covers: AC-1, AC-6
- Runner: agent tester in Herdr
- Preconditions: shared setup
- Given an idle session with an empty composer
- When the tester sends the text `Summarize @calc`, waits 1 s, reads the pane, sends `tab`, sends the text `in one sentence`, then sends `enter`
- Then after the wait a row `calc.py` with `src` beside it is shown; after Tab the composer reads `Summarize @file:src/calc.py `; the reply mentions an average
- Evidence: pane text after the wait, after Tab and after the reply
- Result: NOT RUN

#### UAT-2: A large paste collapses and is sent in full
- Persona: developer
- Covers: AC-1, AC-6
- Runner: either
- Preconditions: shared setup; a 120-line text file whose line 87 reads `the marker is TANGERINE`
- Given an idle session
- When the tester pastes the file as a bracketed paste, sends the text ` What is the marker on line 87?` and sends `enter`
- Then before Enter the composer shows `[Pasted text #1 +120 lines] What is the marker on line 87?`; the user cell shows the same; the reply contains `TANGERINE`
- Evidence: pane text before Enter and after the reply
- Result: NOT RUN

#### UAT-3: Ctrl+V attaches the clipboard image
- Persona: developer
- Covers: AC-1, AC-6
- Runner: operator (needs the desktop clipboard)
- Preconditions: shared setup; operator sets up an image-capable route on `fixture`; a PNG of a solid red square on the clipboard
- Given an idle session
- When the operator presses Ctrl+V, types ` What colour is this image? One word.` and presses Enter
- Then before Enter `[Image #1]` is shown in the accent colour without `uploading`; the reply says red
- Evidence: a terminal screenshot before Enter; the pane text after the reply
- Result: NOT RUN

#### UAT-4: Drop a file from outside the project
- Persona: developer
- Covers: AC-1, AC-6
- Runner: either
- Preconditions: shared setup; in a second temporary folder, `notes-<8 random hex>.txt` (a new name each run) containing `The secret word is OBSIDIAN.`
- Given an idle session
- When the tester pastes that file's absolute path as a bracketed paste, sends the text ` What is the secret word?` and sends `enter`
- Then before Enter the composer shows `[File #1: notes-<same hex>.txt] What is the secret word?`; the reply contains `OBSIDIAN`
- Evidence: pane text before Enter and after the reply
- Result: NOT RUN

#### UAT-5: Refused image, empty clipboard, no matches (negative)
- Persona: developer
- Covers: AC-1, AC-6
- Runner: either
- Preconditions: shared setup; a BMP file in the project; the clipboard holds text only
- Given an idle session
- When the tester pastes the BMP's path as a bracketed paste; then sends `ctrl+v`; then sends the text `@zzzqqq` and waits 1 s
- Then the first shows `Hermes accepts PNG, JPEG, GIF and WebP images.` and no label; the second shows `No image on the clipboard. To paste text, use your terminal's paste (Cmd+V or Ctrl+Shift+V).`; the third shows `No matches for @zzzqqq`; no new user cell appears
- Evidence: pane text after each step
- Result: NOT RUN

### A.P2-05: scenarios of P2-05

All scenarios: in a scratch git project in a temporary folder, start `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash` (flags: `P1-12`) in a Herdr pane; read it with `herdr pane read`. Texts: 3.6.

#### UAT-1: run commands and see their output
- Persona: developer
- Covers: AC-1, AC-3, AC-7
- Runner: agent tester in Herdr
- Preconditions: the operator preconditions under Dependencies and gates; Talaria started as above.
- Given an idle session with an empty composer
- When the tester types `!echo talaria-shell-ok` and presses Enter, then submits `!echo oops >&2; exit 3`
- Then within 2 seconds a cell headed `! echo talaria-shell-ok` with a duration and the line `talaria-shell-ok` appears, with no `exit` line; while `!echo` was typed the composer title read `shell · runs in the session folder on the gateway host · the agent does not see the output`; the second cell, headed `! echo oops >&2; exit 3`, shows `oops` and then `exit 3`, both in colours different from ordinary text
- Evidence: pane reads around each Enter; the time to output; a screenshot of the second cell
- Result: NOT RUN

#### UAT-2: a dangerous command is refused and nothing runs (negative)
- Persona: developer
- Covers: AC-2, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1; the tester first runs `mkdir keepme` in the scratch project from another pane.
- Given an idle session
- When the tester submits `!rm -r ./keepme`, then `!` alone
- Then the first cell shows `Hermes refused to run this: it looks dangerous, and ! runs only safe commands. Ask the agent instead, so the command goes through approval.`; `ls` in the other pane still lists `keepme`; the bare `!` shows `Type a command after !, for example !git status.` and no shell cell
- Evidence: pane read; `ls` output from the other pane
- Result: NOT RUN

#### UAT-3: a side question while the agent works
- Persona: developer
- Covers: AC-4, AC-5, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1; `P2-03` merged. In manual mode Hermes asks only about a command its detector flags (`check_all_command_guards` in `tools/approval.py` at v0.22.4); `sleep 20` is not flagged. An approval card means FAIL.
- Given a session where the tester submitted `Remember the code word heron. Reply with the single word ok.` and saw `ok`
- When the tester submits `Run the shell command sleep 20, then reply with the word done in capital letters.`, waits for the working indicator, submits `/btw what code word did I give you?`, then (still during the turn) `!echo during-turn`
- Then a `◌ btw · what code word did I give you? · answering` row appears at once; `during-turn` appears within 2 seconds, above the streaming reply; a `btw · what code word did I give you?` cell appears whose answer contains `heron`, with the footer `side answer · not part of the conversation`; the turn still ends with `DONE`; after `/quit` and `talaria --resume <id> --profile fixture` in a new pane, no line contains `what code word did I give you?`
- Evidence: pane reads at each step; the session id; the resumed pane text
- Result: NOT RUN

#### UAT-4: `/btw` without a question (negative)
- Persona: developer
- Covers: AC-4, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-3.
- Given an idle session
- When the tester submits `/btw`, then `/btw    ` (the usage notice clears the composer between them)
- Then each shows `Usage: /btw <question>. Asks about this conversation without adding to it; the agent does not see the question or the answer.`, and no `answering` row or side cell appears
- Evidence: pane read
- Result: NOT RUN

#### UAT-5: several pending commands keep the screen tidy
- Persona: developer
- Covers: AC-3, AC-6, AC-7
- Runner: agent tester in Herdr
- Preconditions: as UAT-1.
- Given an idle session
- When the tester submits, quickly one after another, `!sleep 1`, `!sleep 2`, `!sleep 3`, `!sleep 4`
- Then the live region at once shows three `◌ ! sleep … · running` rows and `◌ +1 more`, seconds counting up; the four cells appear in order (Hermes runs them one at a time), the last about 10 seconds after the first Enter; the section then disappears and two reads 3 seconds apart are identical
- Evidence: pane reads; the time of the last cell; the two identical reads
- Result: NOT RUN

### A.P2-11: scenarios of P2-11

Live rules (commands, read sources and their fallback order: spec section 12). Run the branch build `T=<checkout>/target/debug/talaria` by absolute path. Each live scenario uses a fresh pane `P<n>` in a fresh folder `$TMPDIR/tasksproj-<n>`, the session's working folder. Submit to Talaria in two calls, `herdr pane send-text` then `herdr pane send-keys <pane> enter`. `Q` is spec 9's plan prompt. All reads empty, a wait timeout, a skipped todo step, an approval card (denied) or a connection error is BLOCKED (environment), never PASS; a non-empty read without the expected line is FAIL; at most three fresh sessions. Never send `unattended: true`. Each result (PASS, FAIL, or BLOCKED and why) goes on the pull request, headed with the scenario id and commit.

#### UAT-1: A hand-built frame log shows the panel, then the record, without a gateway
- Persona: developer
- Covers: AC-3
- Runner: agent tester in Herdr
- Preconditions: the branch built; no gateway.
- Given a shell at the repository root
- When spec section 12's demo script runs on `crates/talaria-tui/tests/data/todo_frames.jsonl`, writing `$TMPDIR/demo.txt` and printing `grep -c $'\x1b'` of it
- Then the file holds, in order, `live| ▾ Tasks 0/3 · Ctrl+T to collapse`, `live|   [>] Fix average()`, and `scrollback| Tasks 3/3 · done` followed by three `scrollback|   [x] ` rows; the `grep -c` prints `0`
- Evidence: `$TMPDIR/demo.txt`
- Result: NOT RUN

#### UAT-2: The checklist follows a live multi-step turn and is recorded at the end
- Persona: developer
- Covers: AC-6
- Runner: agent tester in Herdr
- Preconditions: live rules above; operator sets up the `fixture` profile.
- Given Talaria is idle in a fresh pane `P2`
- When `Q` is submitted, the tester runs `herdr pane wait-output P2 --regex '(?i)\[>\] .*sleep' --source visible --timeout 120000` and reads the live region, then waits for the reply (spec 12) and reads scrollback and the live region
- Then the first read shows `▾ Tasks` and three rows, one `[>]` naming the sleep step; scrollback has one line matching `^ *Tasks 3/3 · done *$` then three `[x]` rows; the last live read has no `▾ Tasks`
- Evidence: the reads with their times and sources
- Result: NOT RUN

#### UAT-3: Ctrl+T collapses and expands the panel during a turn
- Persona: developer
- Covers: AC-7
- Runner: agent tester in Herdr, plus one operator run outside Herdr pressing Ctrl+T on the keyboard
- Preconditions: as UAT-2, in a fresh pane `P3`.
- Given `Q` was submitted and the panel shows `[>]` on the sleep step
- When `herdr pane send-keys P3 ctrl+t` is sent and the live region read, then `ctrl+t` again and read again, all within the `sleep 30` step
- Then the first read has one row starting `▸ Tasks`, naming the in-progress step and ending `Ctrl+T to expand`, and no item rows below it; the second shows `▾ Tasks` and the rows again; the composer is unchanged. If the step ends before both reads, or Herdr swallows Ctrl+T, the Herdr run is BLOCKED (environment) and the operator run decides
- Evidence: both pane reads; the operator's note
- Result: NOT RUN

#### UAT-4: An interrupted plan is recorded as not finished and does not come back
- Persona: developer
- Covers: AC-8
- Runner: agent tester in Herdr
- Preconditions: as UAT-2, in a fresh pane `P4`.
- Given `Q` was submitted and the panel shows the sleep step as `[>]`
- When `herdr pane send-keys P4 esc` interrupts the turn, and after the interrupted notice `say hi` is submitted and its reply awaited
- Then after the interrupt scrollback has one line matching `^ *Tasks [0-9]+/3 · [0-9]+ not finished` and the live region no `▾ Tasks` or `▸ Tasks`; after the reply still exactly one line matching `^ *Tasks [0-9]+/[0-9]+ · ` and no panel
- Evidence: scrollback and live reads after each step
- Result: NOT RUN

#### UAT-5: Hostile or malformed item text is shown literally and safely
- Persona: developer
- Covers: AC-9
- Runner: agent tester in Herdr
- Preconditions: the branch built; `$TMPDIR/hostile.jsonl`, an uncommitted copy of `todo_frames.jsonl` edited per spec section 12.
- Given a shell at the repository root
- When the demo script runs on that copy as in UAT-1
- Then the output holds `scrollback|   [x] Read ␛[2J the ‹U+202E› test`, no `[ ] x` or `[x] x` row, the block ends `scrollback|   (1 item could not be shown)`, and `grep -c` prints `0`
- Evidence: the output file
- Result: NOT RUN

#### UAT-6: Resuming an idle session shows where its plan stood, once
- Persona: developer
- Covers: AC-10
- Runner: agent tester in Herdr
- Preconditions: UAT-4 finished; Talaria in `P4` quit with `/quit`, which printed `talaria --resume <stored_session_id> --profile fixture`.
- Given a fresh pane `P6` opened in UAT-4's folder
- When `herdr pane run P6 "$T --resume <stored_session_id> --profile fixture"` runs
- Then after the resumed history the pane shows one line ending `not finished (from before this resume)` followed by the item rows, and no live `Tasks` panel
- Evidence: scrollback and live reads
- Result: NOT RUN

### A.P2-12: scenarios of P2-12

The run's agent tester runs the Herdr ones, the operator the rest. Common setup: operator preconditions hold; `cargo build --release -p talaria-cli`; `mkdir -p "$TMPDIR/theme"`, then `herdr pane split --current --direction right --cwd "$TMPDIR/theme" --no-focus` gives pane `P`; read it with `herdr pane read P --source recent --lines 40`. `<run>` is the absolute path of `target/release/talaria` followed by `--profile fixture --provider ollama-cloud --model minimax-m3.1-flash`.

#### UAT-1: Talaria reports the profile's skin and the detected background
- Persona: developer
- Covers: AC-1, AC-4
- Runner: agent tester in Herdr
- Preconditions: common setup; the `fixture` skin left as the operator set it
- Given Talaria started with `herdr pane run P "<run>"`
- When the tester runs `herdr pane run P "/theme"`
- Then the notice shows `Theme`, `Skin:`, `Background: light` or `dark` with a source, `Colours:` and `Motion: full (default)`
- Evidence: pane text; terminal and Herdr versions
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-2: Light and dark can be forced, and the session override works
- Persona: developer
- Covers: AC-1, AC-4
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given Talaria started with `herdr pane run P "TALARIA_THEME=light <run>"`
- When the tester runs `/theme`, `/theme dark`, `/theme`, `/theme auto`, reading after each
- Then the reports show `Background: light (TALARIA_THEME)`, then `Background set to dark for this session. Lines already in scrollback keep their colours.`, then `Background: dark (set with /theme)`, then `Background detection restored: light (TALARIA_THEME).`
- Evidence: pane text after each step
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-3: NO_COLOR and reduced motion are honoured and Talaria stays usable
- Persona: developer
- Covers: AC-1, AC-4, AC-5
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given Talaria started with `herdr pane run P "NO_COLOR=1 TALARIA_REDUCED_MOTION=1 <run>"`
- When the tester runs `/theme`, then submits `Reply with only the word READY.`
- Then the report shows `Colours: off (NO_COLOR)` and `Motion: reduced (TALARIA_REDUCED_MOTION)`, and `READY` appears once with the status line intact
- Evidence: pane text; time from submit to `READY`
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-4: Switching the skin recolours Talaria live, without touching scrollback
- Persona: configurer
- Covers: AC-2, AC-3, AC-7
- Runner: operator
- Preconditions: common setup; operator sets up permission to switch the `fixture` skin; a 24-bit terminal
- Given Talaria on `fixture` with one answered prompt in scrollback; the original name noted from `/skin`
- When the operator runs `/skin ares`, reads the pane at once, then runs `/skin <original name>` and `/skin`
- Then the first read shows `Skin set to "ares" for profile fixture.` and the rest of the 9.1 line; the composer border turns crimson; the earlier answer keeps its colours; the final `/skin` names the original skin
- Evidence: before and after screenshots; pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-5: Bad arguments are refused before anything is sent
- Persona: developer
- Covers: AC-3, AC-4
- Runner: agent tester in Herdr
- Preconditions: common setup
- Given Talaria running on `fixture`
- When the tester runs `/skin ../etc`, `/theme purple`, then `/skin`
- Then the first two print the REQ-16 usage line and `Usage: /theme [light|dark|auto]`, no `Hermes refused` line appears, and `/skin` names the UAT-1 skin
- Evidence: pane text
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

#### UAT-6: An unknown skin name is explained, and detection is right on both backgrounds
- Persona: configurer
- Covers: AC-3, AC-7
- Runner: operator
- Preconditions: UAT-4's permission; a light and a dark terminal, tmux inside one
- Given Talaria on `fixture` in each terminal in turn; the original skin name noted from `/skin`
- When the operator runs `/skin nosuch-skin`, `/theme`, asks for an answer with a heading and a code span, then `/skin <original name>` and `/skin`
- Then the first shows `Hermes has no skin named "nosuch-skin"; it is using "default". Run /skin default to clear the setting.`; `/theme` shows `Background: light` in the light terminal and `dark` in the dark one, each with a source, and no `Ignored skin values` line; the final `/skin` names the original skin
- Evidence: a screenshot and the `/theme` text per terminal, including under tmux
- Result: PASS / FAIL / BLOCKED (environment | not built | no access) / NOT RUN

### A.P2-13: scenarios of P2-13

Unless stated: Talaria in a Herdr pane, in a scratch git project, no `[editor]` in `~/.talaria/config.toml`, started with `talaria --profile fixture --provider ollama-cloud --model minimax-m3.1-flash`. Read the pane with `herdr pane read <pane> --source recent-unwrapped` and `--source visible`; record which showed it.

#### UAT-1: edit a draft in vi and send it yourself
- Persona: developer
- Covers: AC-2, AC-10
- Runner: agent tester in Herdr
- Preconditions: `EDITOR=vi` exported, `VISUAL` unset.
- Given an idle session with the draft `draft one`
- When the tester presses Ctrl+G, then in vi sends `A`, ` plus two`, Esc, `:wq`, Enter
- Then a `visible` read shows vi; then the composer shows `draft one plus two`, unsent, lines above unchanged, none duplicated; Enter sends it
- Evidence: pane reads before Ctrl+G, inside vi, after `:wq`, after Enter
- Result: NOT RUN

#### UAT-2: a missing editor is reported (negative)
- Persona: developer
- Covers: AC-2, AC-10
- Runner: agent tester in Herdr
- Preconditions: `EDITOR=talaria-no-such-editor`, `VISUAL` unset.
- Given the draft `keep me`
- When the tester presses Ctrl+G, then types ` ok`
- Then notice E1 with `"talaria-no-such-editor": not found.` shows; the composer reads `keep me ok`
- Evidence: pane reads after the notice and after typing
- Result: NOT RUN

#### UAT-3: find a prompt from an earlier run
- Persona: developer
- Covers: AC-3, AC-10
- Runner: agent tester in Herdr
- Preconditions: none beyond the default start.
- Given the tester submitted `Reply with the word alpha.` and `Reply with the word beta.`, quit with `/quit`, and restarted in the same folder
- When the tester presses Ctrl+R, types `alp`, Enter; then Ctrl+R, types `zzzz`, Esc
- Then the first box lists `Reply with the word alpha.` and Enter loads it unsent; the second shows `No earlier prompt matches "zzzz".` and Esc restores the composer exactly
- Evidence: pane reads of both boxes and of the composer after each
- Result: NOT RUN

#### UAT-4: suspend during a streaming reply
- Persona: developer
- Covers: AC-4, AC-10
- Runner: agent tester in Herdr
- Preconditions: Talaria started from the pane's interactive shell (not `exec`).
- Given the tester submitted `Count from 1 to 60, one number per line.`
- When, while numbers stream, the tester presses Ctrl+Z, waits 10 seconds, runs `fg`
- Then after Ctrl+Z a shell prompt and a stopped-job line show, no composer box; after `fg` no S2, the composer is back, the reply ends at `60`, each number once
- Evidence: pane reads after Ctrl+Z and after the reply; the count of lines matching `^ *[0-9]+$` (60)
- Result: NOT RUN

#### UAT-5: Herdr follows Talaria's state
- Persona: fleet-lead
- Covers: AC-5, AC-10
- Runner: agent tester in Herdr
- Preconditions: operator sets up `approvals.mode: manual` on `fixture`; the project holds `.pytest_cache/x`. Verify before relying on it: `herdr pane get <pane>` shows the agent and state; if not, record the sidebar.
- Given Talaria idle in pane `<pane>`
- When the tester runs `herdr pane get <pane>`; submits `Run rm -rf .pytest_cache`; runs it while the card shows; denies; runs it after the turn; runs `/quit`; runs it again
- Then the agent `talaria` is `idle`, then `blocked` with a message starting `approval:`, then `idle`; no R1 notice shows; after `/quit` no `talaria` agent; `.pytest_cache/x` still exists
- Evidence: the four outputs, a pane read, `ls .pytest_cache`
- Result: NOT RUN

#### UAT-6: notifications in a real terminal
- Persona: developer
- Covers: AC-5, AC-10
- Runner: operator
- Preconditions: operator sets up `approvals.mode: manual` on `fixture`.
- Given Talaria run directly in Ghostty or iTerm2 (no multiplexer)
- When the operator submits `Run rm -rf .pytest_cache` and switches away; then denies, submits `Wait 30 seconds using sleep, then say done.` and switches away
- Then a bell and `Talaria: needs you: approval: rm -rf .pytest_cache` arrive, then `Talaria: done: ...` after about 30 seconds. Results inside Herdr and tmux (`allow-passthrough on`) go in a table and do not decide the result
- Evidence: screenshots; the table in the pull request
- Result: NOT RUN

#### UAT-7: Ctrl+Z without job control (negative)
- Persona: developer
- Covers: AC-4, AC-10
- Runner: agent tester in Herdr
- Preconditions: a fresh pane; Talaria started with `exec` before the default command.
- Given an idle session with the draft `still here`
- When the tester presses Ctrl+Z
- Then notice S1 (`Ctrl+Z cannot suspend here: ...`) shows, the composer reads `still here`, keys still work
- Evidence: pane read after Ctrl+Z
- Result: NOT RUN
